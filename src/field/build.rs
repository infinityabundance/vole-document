//! Direct source → field ingestion (Phase 17).
//!
//! The runtime path deliberately **skips the compression candidate search**. The
//! field is not a compression product, and — this is the load-bearing fact — its
//! observations are *not* derived from the winning reconstruction program: both
//! [`crate::field::ingest::ingest_pdf_with`] (Stage B) and
//! [`crate::field::ingest_package::ingest_package_with`] materialize the exact
//! source from the descriptor and then **re-scan the source bytes** into exact
//! spans, object/stream/page nodes, package members, resource blobs, and the
//! hierarchical index. The descriptor's program is consulted only for the
//! advisory `OBSERVATION_INDEX` op table (a seek optimization; it falls back to
//! the full descriptor path when it cannot apply) and for the exactness check
//! itself.
//!
//! Therefore a single, deterministically chosen, exact program serves the same
//! observation surface as the searched winner. [`BuildProfile::Runtime`] fixes
//! that program to the literal [`CandidateKind::Raw`] floor: one `EMIT_OBJECT`
//! over one literal object. It costs a copy, not a search, and it is proved
//! exact by the same court ([`crate::encode::encode_with`]) the searched path
//! uses — the court is still run, over exactly one candidate.
//!
//! This module produces the exact authority (a valid `.voldoc` descriptor blob,
//! stored in the field's descriptor namespace and optionally written to disk) and
//! the field from one source buffer, in one process.

use crate::encode::candidates::CandidateKind;
use crate::encode::encode_with;
use crate::error::{Error, Result};
use crate::field::FieldId;
use crate::field::FieldStore;
use crate::field::ingest::IngestReport;
#[cfg(not(feature = "package"))]
use crate::field::ingest::ingest_pdf_with;
use crate::field::ingest::with_observation_index;
#[cfg(feature = "package")]
use crate::field::ingest::{IngestOutcome, ingest_with};
#[cfg(feature = "package")]
use crate::field::ingest_package::PackageIngestReport;
use crate::integrity::{sha256, to_hex};
use crate::limits::Limits;
use crate::parallel::WorkerPool;

/// Fixed, non-searched reconstruction profiles for the direct ingest path.
///
/// A profile names a *deterministic function source → exact descriptor*, never a
/// search. It exists so the runtime CLI can state which fixed program it chose;
/// adding a profile is a deliberate act, and every profile must remain exact
/// (proved per build by the complete-cost court) and must not reduce the
/// observation surface the field exposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildProfile {
    /// The literal `RAW` floor: one literal object, one `EMIT_OBJECT`. No
    /// candidate is searched; the court prices exactly this one program.
    Runtime,
}

impl BuildProfile {
    /// Stable lower-case name for reports and receipts.
    pub const fn name(self) -> &'static str {
        match self {
            BuildProfile::Runtime => "runtime",
        }
    }

    /// The single candidate family this profile fixes, with no search.
    pub const fn candidate(self) -> CandidateKind {
        match self {
            BuildProfile::Runtime => CandidateKind::Raw,
        }
    }

    /// Parse a `--profile` spelling.
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "runtime" => Ok(BuildProfile::Runtime),
            other => Err(Error::usage(format!(
                "unknown --profile {other:?}; expected one of: runtime"
            ))),
        }
    }
}

/// Which native inverse compiler ran for a direct build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectIngest {
    /// A PDF (or opaque non-ZIP) field.
    Pdf(IngestReport),
    /// A ZIP-based package (DOCX/EPUB/ODT/generic ZIP) field.
    #[cfg(feature = "package")]
    Package(PackageIngestReport),
}

impl DirectIngest {
    /// The id of the field the ingest produced.
    pub fn field_id(&self) -> FieldId {
        match self {
            DirectIngest::Pdf(r) => r.field,
            #[cfg(feature = "package")]
            DirectIngest::Package(r) => r.field,
        }
    }
}

/// What one direct (non-searched) build produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectBuildReport {
    /// The fixed profile used (`runtime`).
    pub profile: &'static str,
    /// The fixed candidate family actually serialized (`RAW`).
    pub candidate: &'static str,
    /// Candidates priced by the court. Always `1` for a fixed profile; the field
    /// is present so a receipt can witness that no search happened.
    pub candidates_evaluated: u32,
    /// Lower-case hex SHA-256 of the source (from the court's own check).
    pub source_sha256: String,
    /// Exact source length.
    pub source_len: u64,
    /// Length of the exact authority actually stored in the field (the serialized
    /// descriptor, including any advisory observation-index record the ingest
    /// added). This is the blob `--voldoc` writes and a later `field-ingest`
    /// would consume.
    pub encoded_len: u64,
    /// Lower-case hex SHA-256 of the stored exact authority.
    pub descriptor_sha256: String,
    /// The field ingest outcome (format, field id, recovered structure counters).
    pub ingest: DirectIngest,
}

/// Build a field directly from source bytes with a fixed, non-searched program.
///
/// Serial `None`-pool form; see [`build_field_with`].
pub fn build_field(
    source: &[u8],
    store: &mut FieldStore,
    limits: Limits,
    profile: BuildProfile,
) -> Result<DirectBuildReport> {
    build_field_with(source, store, limits, profile, None)
}

/// Build a field directly from source bytes with a fixed, non-searched program.
///
/// The exact authority is the descriptor returned by the court for
/// `profile.candidate()` — one candidate, serialized, decoded, and byte-compared
/// against `source`, so `materialize(descriptor) == source` holds by the same
/// proof the searched path uses. The descriptor is then inverted into the field
/// exactly as `field-ingest` would invert it.
pub fn build_field_with(
    source: &[u8],
    store: &mut FieldStore,
    limits: Limits,
    profile: BuildProfile,
    pool: Option<&WorkerPool>,
) -> Result<DirectBuildReport> {
    let (descriptor_bytes, encode_report) = encode_with(source, limits, Some(profile.candidate()))?;
    // The durable authority is the descriptor plus the advisory observation-index
    // record the ingest adds. `with_observation_index` is idempotent (it returns
    // the input unchanged when a record is already present), so computing it here
    // once yields exactly the blob the ingest will store, with no extra read-back.
    let authority = with_observation_index(&descriptor_bytes, limits)?;
    drop(descriptor_bytes);
    let encoded_len = authority.len() as u64;
    let descriptor_sha256 = to_hex(&sha256(&authority));
    let ingest = direct_ingest(store, &authority, limits, pool)?;
    Ok(DirectBuildReport {
        profile: profile.name(),
        candidate: encode_report.kind.name(),
        candidates_evaluated: encode_report.candidates_evaluated,
        source_sha256: encode_report.sha256_hex,
        source_len: encode_report.source_len,
        encoded_len,
        descriptor_sha256,
        ingest,
    })
}

#[cfg(feature = "package")]
fn direct_ingest(
    store: &mut FieldStore,
    descriptor_bytes: &[u8],
    limits: Limits,
    pool: Option<&WorkerPool>,
) -> Result<DirectIngest> {
    match ingest_with(store, descriptor_bytes, limits, pool)? {
        IngestOutcome::Package(r) => Ok(DirectIngest::Package(r)),
        IngestOutcome::Pdf(r) => Ok(DirectIngest::Pdf(r)),
    }
}

#[cfg(not(feature = "package"))]
fn direct_ingest(
    store: &mut FieldStore,
    descriptor_bytes: &[u8],
    limits: Limits,
    pool: Option<&WorkerPool>,
) -> Result<DirectIngest> {
    Ok(DirectIngest::Pdf(ingest_pdf_with(
        store,
        descriptor_bytes,
        limits,
        pool,
    )?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "vole-build-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn runtime_profile_prices_one_candidate_and_is_exact() {
        let source: Vec<u8> = b"the runtime path must not search the portfolio\n".repeat(64);
        let root = temp_root("exact");
        let mut store = FieldStore::open(&root).unwrap();
        let report =
            build_field(&source, &mut store, Limits::DEFAULT, BuildProfile::Runtime).unwrap();
        store.sync().unwrap();

        assert_eq!(report.profile, "runtime");
        assert_eq!(report.candidate, "RAW");
        assert_eq!(
            report.candidates_evaluated, 1,
            "a fixed profile must price exactly one candidate"
        );
        assert_eq!(report.source_len, source.len() as u64);

        // Exactness: materialize the field root and byte-compare.
        let field_id = report.ingest.field_id();
        let field = crate::field::Field::open(&store, &field_id, Limits::DEFAULT).unwrap();
        assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), source);
    }

    #[test]
    fn stable_repeated_build_gives_identical_bytes_and_structure() {
        let source = b"determinism is part of the contract".repeat(40);
        let a = temp_root("det-a");
        let b = temp_root("det-b");
        let mut sa = FieldStore::open(&a).unwrap();
        let mut sb = FieldStore::open(&b).unwrap();
        let ra = build_field(&source, &mut sa, Limits::DEFAULT, BuildProfile::Runtime).unwrap();
        let rb = build_field(&source, &mut sb, Limits::DEFAULT, BuildProfile::Runtime).unwrap();
        assert_eq!(
            ra.ingest.field_id(),
            rb.ingest.field_id(),
            "the same source must yield the same field id"
        );
    }

    #[test]
    fn profile_parse_rejects_unknown() {
        assert_eq!(
            BuildProfile::parse("runtime").unwrap(),
            BuildProfile::Runtime
        );
        assert!(BuildProfile::parse("fastest").is_err());
    }
}
