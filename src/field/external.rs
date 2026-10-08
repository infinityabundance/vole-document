//! External (non-document) context — Phase 20.4, ADR-0050 / ADR-0052.
//!
//! The C4b contract asks for **corpus/external lineage** (a dataset family id,
//! a member id, and a head/current marker). Those facts are *not* in the
//! document bytes: a single PDF cannot derive which corpus family it belongs to
//! or whether it is its family's head. Admitting them is therefore a separate,
//! explicitly-typed layer supplied by the caller — never inferred, never a
//! default, and never on the exactness path.
//!
//! ## Separation
//!
//! An [`ExternalContext`] is stored **beside** the document-derived field: one
//! self-describing record at `<store-root>/external/<field-id>` (see
//! [`crate::field::FieldStore::put_external_context`]). It is *not* in the seed
//! DAG, the observation index, the field manifest, the descriptor blob, or the
//! exactness authority. Removing it is one `unlink` and touches nothing else, so
//! every document-derived observation and `materialize(descriptor) ==
//! original_bytes` are byte-identical whether or not a context is attached.
//!
//! ## Honest provenance
//!
//! An answer drawn from this layer carries [`crate::field::provenance::Basis::ExternalMetadata`]:
//! `exact` is always false, no seed node is read, and no integrity scope of the
//! source is claimed. A field with **no** attached context is a typed decline,
//! never a guess ([`crate::ErrorClass::UnsupportedFeature`]).

use crate::error::{Error, Result};
use crate::field::provenance::json_escape;

/// The record magic (`VOLECTX1`), distinguishing it from any other store blob.
const EXTERNAL_MAGIC: &[u8; 8] = b"VOLECTX1";
/// The record format version.
const EXTERNAL_FORMAT_VERSION: u8 = 1;

/// Where an externally-supplied fact came from. A small **closed** vocabulary,
/// not a free string, so an origin can be compared and rejected rather than
/// accepted blindly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExternalOrigin {
    /// Supplied by the evaluation harness/campaign (the C4b court's corpus
    /// metadata).
    #[default]
    Harness,
    /// Supplied by a human operator.
    Operator,
    /// Supplied by an upstream catalog/registry.
    Catalog,
}

impl ExternalOrigin {
    /// Stable lower-case name (used in JSON and receipts).
    pub const fn name(self) -> &'static str {
        match self {
            ExternalOrigin::Harness => "harness",
            ExternalOrigin::Operator => "operator",
            ExternalOrigin::Catalog => "catalog",
        }
    }

    /// The stable on-disk code.
    const fn code(self) -> u8 {
        match self {
            ExternalOrigin::Harness => 0,
            ExternalOrigin::Operator => 1,
            ExternalOrigin::Catalog => 2,
        }
    }

    /// Parse the on-disk code.
    fn from_code(b: u8) -> Result<Self> {
        match b {
            0 => Ok(ExternalOrigin::Harness),
            1 => Ok(ExternalOrigin::Operator),
            2 => Ok(ExternalOrigin::Catalog),
            other => Err(Error::new(
                crate::ErrorClass::UnsupportedVersion,
                format!("external-context origin code {other} is not supported"),
            )),
        }
    }

    /// Parse a CLI spelling.
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "harness" => Ok(ExternalOrigin::Harness),
            "operator" => Ok(ExternalOrigin::Operator),
            "catalog" => Ok(ExternalOrigin::Catalog),
            other => Err(Error::usage(format!(
                "--origin expects harness|operator|catalog, got {other:?}"
            ))),
        }
    }
}

/// The typed external lineage tuple (the C4b `family_id` / `member_id` /
/// `is_head`, plus the dataset's revision-family id when it differs).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExternalLineage {
    /// The corpus/dataset family id (the C4b `family_id`), or `None` when the
    /// dataset declares no family.
    pub family: Option<String>,
    /// The member id within the family (the C4b `member_id`).
    pub member: Option<String>,
    /// Whether this member is its family's head/current revision.
    pub head: bool,
    /// The dataset's revision-family id, when it is distinct from `family`.
    pub revision_family: Option<String>,
}

/// A typed, externally-supplied context record, stored beside the field.
///
/// Every field is optional except the origin and the (advisory) source label, so
/// a caller can attach only the facts it actually holds rather than inventing
/// the rest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalContext {
    /// The dataset the document belongs to (e.g. `real100-v1`).
    pub dataset_id: Option<String>,
    /// The external lineage tuple.
    pub lineage: ExternalLineage,
    /// Where the facts came from.
    pub origin: ExternalOrigin,
    /// A human-readable provenance label (e.g. the manifest path or row).
    pub source: String,
}

fn put_opt_str(out: &mut Vec<u8>, s: Option<&str>) {
    match s {
        Some(v) => {
            out.push(1);
            out.extend_from_slice(&(v.len() as u32).to_le_bytes());
            out.extend_from_slice(v.as_bytes());
        }
        None => out.push(0),
    }
}

fn get_opt_str(bytes: &[u8], at: &mut usize) -> Result<Option<String>> {
    let present = *bytes
        .get(*at)
        .ok_or_else(|| Error::usage("truncated external-context record"))?;
    *at += 1;
    if present == 0 {
        return Ok(None);
    }
    let len_bytes = bytes
        .get(*at..*at + 4)
        .ok_or_else(|| Error::usage("truncated external-context record"))?;
    let len = u32::from_le_bytes(len_bytes.try_into().unwrap()) as usize;
    *at += 4;
    let end = at
        .checked_add(len)
        .ok_or_else(|| Error::usage("external-context string overflow"))?;
    let slice = bytes
        .get(*at..end)
        .ok_or_else(|| Error::usage("truncated external-context string"))?;
    let s = core::str::from_utf8(slice)
        .map_err(|_| Error::usage("external-context string is not UTF-8"))?
        .to_string();
    *at = end;
    Ok(Some(s))
}

fn opt_json(v: Option<&str>) -> String {
    match v {
        Some(s) => format!("\"{}\"", json_escape(s)),
        None => "null".to_string(),
    }
}

impl ExternalContext {
    /// Canonical, self-describing encoding.
    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(128);
        out.extend_from_slice(EXTERNAL_MAGIC);
        out.push(EXTERNAL_FORMAT_VERSION);
        out.push(self.origin.code());
        put_opt_str(&mut out, self.dataset_id.as_deref());
        put_opt_str(&mut out, self.lineage.family.as_deref());
        put_opt_str(&mut out, self.lineage.member.as_deref());
        out.push(u8::from(self.lineage.head));
        put_opt_str(&mut out, self.lineage.revision_family.as_deref());
        put_opt_str(&mut out, Some(self.source.as_str()));
        out
    }

    /// Parse a canonical external-context record, failing closed on any
    /// malformation rather than answering from a partially-understood record.
    pub fn decode_canonical(bytes: &[u8]) -> Result<ExternalContext> {
        if bytes.len() < 10 || &bytes[0..8] != EXTERNAL_MAGIC {
            return Err(Error::unsupported_version(
                "external-context record: bad magic",
            ));
        }
        if bytes[8] != EXTERNAL_FORMAT_VERSION {
            return Err(Error::unsupported_version(format!(
                "external-context record version {} is not supported",
                bytes[8]
            )));
        }
        let origin = ExternalOrigin::from_code(bytes[9])?;
        let mut at = 10;
        let dataset_id = get_opt_str(bytes, &mut at)?;
        let family = get_opt_str(bytes, &mut at)?;
        let member = get_opt_str(bytes, &mut at)?;
        let head = match bytes.get(at) {
            Some(0) => false,
            Some(1) => true,
            Some(_) => return Err(Error::usage("external-context head flag is not boolean")),
            None => return Err(Error::usage("truncated external-context record")),
        };
        at += 1;
        let revision_family = get_opt_str(bytes, &mut at)?;
        let source = get_opt_str(bytes, &mut at)?.unwrap_or_default();
        if at != bytes.len() {
            return Err(Error::usage("external-context record has trailing bytes"));
        }
        Ok(ExternalContext {
            dataset_id,
            lineage: ExternalLineage {
                family,
                member,
                head,
                revision_family,
            },
            origin,
            source,
        })
    }

    /// The typed answer body. The lineage tuple is named exactly as the C4b
    /// contract names it (`family_id` / `member_id` / `is_head`), so an answer
    /// can be compared to the baseline's tuple without a projection.
    pub fn answer_json(&self) -> String {
        format!(
            concat!(
                "{{",
                "\"family_id\":{},",
                "\"member_id\":{},",
                "\"is_head\":{},",
                "\"dataset_id\":{},",
                "\"revision_family\":{},",
                "\"origin\":\"{}\",",
                "\"source\":\"{}\"",
                "}}"
            ),
            opt_json(self.lineage.family.as_deref()),
            opt_json(self.lineage.member.as_deref()),
            self.lineage.head,
            opt_json(self.dataset_id.as_deref()),
            opt_json(self.lineage.revision_family.as_deref()),
            self.origin.name(),
            json_escape(&self.source),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ExternalContext {
        ExternalContext {
            dataset_id: Some("real100-v1".to_string()),
            lineage: ExternalLineage {
                family: Some("rev-nist-fips-140".to_string()),
                member: Some("nist-pdf-0017".to_string()),
                head: true,
                revision_family: None,
            },
            origin: ExternalOrigin::Harness,
            source: "real100-v1/manifest.tsv".to_string(),
        }
    }

    #[test]
    fn round_trips_canonically() {
        let ctx = sample();
        let bytes = ctx.encode_canonical();
        assert_eq!(ExternalContext::decode_canonical(&bytes).unwrap(), ctx);
        // Deterministic: encoding twice is byte-identical.
        assert_eq!(ctx.encode_canonical(), bytes);
    }

    #[test]
    fn round_trips_with_absent_fields() {
        let ctx = ExternalContext {
            dataset_id: None,
            lineage: ExternalLineage::default(),
            origin: ExternalOrigin::Operator,
            source: String::new(),
        };
        let bytes = ctx.encode_canonical();
        assert_eq!(ExternalContext::decode_canonical(&bytes).unwrap(), ctx);
    }

    #[test]
    fn rejects_bad_magic_version_and_trailing_bytes() {
        let mut bytes = sample().encode_canonical();
        bytes[0] ^= 0xff;
        assert_eq!(
            ExternalContext::decode_canonical(&bytes)
                .unwrap_err()
                .class(),
            crate::ErrorClass::UnsupportedVersion
        );
        let mut bytes = sample().encode_canonical();
        bytes[8] = 9;
        assert_eq!(
            ExternalContext::decode_canonical(&bytes)
                .unwrap_err()
                .class(),
            crate::ErrorClass::UnsupportedVersion
        );
        let mut bytes = sample().encode_canonical();
        bytes.push(0);
        assert_eq!(
            ExternalContext::decode_canonical(&bytes)
                .unwrap_err()
                .class(),
            crate::ErrorClass::Usage
        );
        assert!(ExternalContext::decode_canonical(b"short").is_err());
    }

    #[test]
    fn origin_parses_typed_and_rejects_unknown() {
        assert_eq!(ExternalOrigin::parse("harness").unwrap().name(), "harness");
        assert_eq!(
            ExternalOrigin::parse("operator").unwrap().name(),
            "operator"
        );
        assert_eq!(ExternalOrigin::parse("catalog").unwrap().name(), "catalog");
        assert_eq!(
            ExternalOrigin::parse("guess").unwrap_err().class(),
            crate::ErrorClass::Usage
        );
    }

    #[test]
    fn answer_json_names_the_c4b_tuple() {
        let json = sample().answer_json();
        assert!(
            json.contains("\"family_id\":\"rev-nist-fips-140\""),
            "{json}"
        );
        assert!(json.contains("\"member_id\":\"nist-pdf-0017\""), "{json}");
        assert!(json.contains("\"is_head\":true"), "{json}");
        assert!(json.contains("\"origin\":\"harness\""), "{json}");
    }
}
