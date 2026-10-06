//! The observation query engine (Phase 11.5–11.7).
//!
//! An [`observe`] call resolves one typed selector/representation pair against a
//! persisted field and returns a [`FieldAnswer`] with full provenance plus
//! [`ObserveStats`] describing the work actually done, and the current/promoted
//! [`FieldId`] so a caller can chain without re-deepening. The engine is
//! deterministic and read-only with respect to archival authority: it never
//! consults an agent, model, or search process, and it can never influence
//! `materialize_exact` (ADR-0024, DEC-6).
//!
//! ## The frontier rule (11.6)
//!
//! A narrow observation materializes only the dependency closure it needs. In
//! particular, `Page(n) + Structure` reads the page's decoded content streams and
//! its preview node — it never reads image/XObject bytes and never reconstructs
//! the whole document. The split byte classes in [`ObserveStats`] make this
//! auditable: the **seed** closure is reported separately as `seed_bytes_read`,
//! and the descriptor bytes a narrow observation necessarily reads to open the
//! field are charged honestly in `descriptor_bytes_read` rather than hidden.
//!
//! ## No guessing
//!
//! An unsupported selector/representation pair is a typed
//! [`crate::ErrorClass::UnsupportedFeature`], never a silently empty answer.

use std::cell::Cell;
use std::time::Instant;

#[cfg(feature = "docx")]
use crate::adapter::docx::wml::StoryModel;
#[cfg(feature = "docx")]
use crate::adapter::docx::{DocxExtractProfile, DocxModel, DocxPartRef, DocxStory, story_params};
use crate::error::{Error, Result};
use crate::field::cache::DerivedCache;
use crate::field::dag::{self, EvalBudget, ReuseStats, SourceServer};
#[cfg(feature = "docx")]
use crate::field::index::SEL_DOCX_MODEL;
#[cfg(feature = "opc")]
use crate::field::index::SEL_OPC_MODEL;
use crate::field::index::{
    FsIndexStore, IndexEntry, SEL_OBJECT, SEL_PACKAGE_MEMBER_DECODED, SEL_PACKAGE_MEMBER_RAW,
    SEL_PAGE, SEL_REVISION, SEL_STREAM, SEL_STREAM_DECODED, SelectorKey, lookup,
};
use crate::field::ingest;
use crate::field::manifest::FieldRoot;
use crate::field::node::{NodeKind, SeedNode, read_u32_params, span_params, u32_params};
use crate::field::partial::{PartialDescriptor, PartialLoad};
use crate::field::{Field, FieldId, FieldStore, SeedSubstrate};
use crate::limits::Limits;
use crate::store::{Id, IoSnapshot, NodeId, SeedStore};

use super::provenance::{AnswerValue, Basis, FieldAnswer, IntegrityScope, json_escape};

/// Upper bound on pages a single `TextMatch` scan will visit.
const MAX_TEXTMATCH_PAGES: u32 = 1 << 20;

/// A typed observation selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selector {
    /// The whole document.
    Document,
    /// A page, by 1-based page number (as recovered by ingest).
    Page(u32),
    /// An indirect object, by object number.
    Object(u32),
    /// An encoded stream, by owning object number.
    Stream(u32),
    /// A physical revision, by 0-based index.
    Revision(u32),
    /// A package (ZIP/OCF/OPC) member, by central-directory ordinal. The ordinal is
    /// the physical identity; duplicate names stay distinct (Phase 12.2).
    Member(u32),
    /// A generic OPC package part, by absolute part name (Phase 12.3). Part-name
    /// equivalence is case-insensitive. Resolution is by the OPC relationship graph,
    /// never by a hardcoded path.
    PackagePart(String),
    /// A generic OPC relationship, by id (Phase 12.3). Ids are only unique within one
    /// `.rels` part, so a duplicated id across owners is a typed ambiguity decline.
    Relationship(String),
    /// A half-open exact source byte range.
    ByteRange {
        /// Start offset.
        offset: u64,
        /// Length in bytes.
        len: u64,
    },
    /// Every text line containing a pattern (case-sensitive).
    TextMatch(String),
    /// A DOCX story, scoped to exactly one story and one extraction profile
    /// (Phase 12.4). A story is never silently mixed with another.
    #[cfg(feature = "docx")]
    DocxStory {
        /// The story to observe.
        story: DocxStory,
        /// The extraction profile identity.
        profile: DocxExtractProfile,
    },
    /// A body-level paragraph of a DOCX story, by 0-based document-order index.
    #[cfg(feature = "docx")]
    DocxParagraph {
        /// The owning story.
        story: DocxStory,
        /// The paragraph index.
        index: u32,
        /// The extraction profile identity.
        profile: DocxExtractProfile,
    },
    /// A top-level DOCX table, by 0-based index.
    #[cfg(feature = "docx")]
    DocxTable {
        /// The owning story.
        story: DocxStory,
        /// The table index.
        index: u32,
        /// The extraction profile identity.
        profile: DocxExtractProfile,
    },
    /// A DOCX table cell, addressed by an A1-style reference (e.g. `B7`).
    #[cfg(feature = "docx")]
    DocxCell {
        /// The owning story.
        story: DocxStory,
        /// The table index.
        table: u32,
        /// The cell reference (`B7`: column `B`, 1-based row `7`).
        cell: String,
        /// The extraction profile identity.
        profile: DocxExtractProfile,
    },
    /// A story-scoped text search over paragraphs.
    #[cfg(feature = "docx")]
    DocxFind {
        /// The owning story.
        story: DocxStory,
        /// The pattern (case-sensitive substring).
        pattern: String,
        /// The extraction profile identity.
        profile: DocxExtractProfile,
    },
}

impl Selector {
    /// Canonical selector text, e.g. `page:1` or `byte-range:10:4`.
    pub fn canonical(&self) -> String {
        match self {
            Selector::Document => "document".to_string(),
            Selector::Page(n) => format!("page:{n}"),
            Selector::Object(n) => format!("object:{n}"),
            Selector::Stream(n) => format!("stream:{n}"),
            Selector::Revision(n) => format!("revision:{n}"),
            Selector::Member(n) => format!("member:{n}"),
            Selector::PackagePart(name) => format!("package-part:{name}"),
            Selector::Relationship(id) => format!("relationship:{id}"),
            Selector::ByteRange { offset, len } => format!("byte-range:{offset}:{len}"),
            Selector::TextMatch(p) => format!("text-match:{p}"),
            #[cfg(feature = "docx")]
            Selector::DocxStory { story, profile } => {
                format!(
                    "docx-story:{};profile={}",
                    story.name(),
                    profile.fingerprint()
                )
            }
            #[cfg(feature = "docx")]
            Selector::DocxParagraph {
                story,
                index,
                profile,
            } => format!(
                "docx-paragraph:{}:{};profile={}",
                story.name(),
                index,
                profile.fingerprint()
            ),
            #[cfg(feature = "docx")]
            Selector::DocxTable {
                story,
                index,
                profile,
            } => format!(
                "docx-table:{}:{};profile={}",
                story.name(),
                index,
                profile.fingerprint()
            ),
            #[cfg(feature = "docx")]
            Selector::DocxCell {
                story,
                table,
                cell,
                profile,
            } => format!(
                "docx-cell:{}:{}:{};profile={}",
                story.name(),
                table,
                cell,
                profile.fingerprint()
            ),
            #[cfg(feature = "docx")]
            Selector::DocxFind {
                story,
                pattern,
                profile,
            } => format!(
                "docx-find:{}:{};profile={}",
                story.name(),
                pattern,
                profile.fingerprint()
            ),
        }
    }
}

/// How the selected thing should be represented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Representation {
    /// Document-level metadata.
    Metadata,
    /// Extracted text runs.
    Text,
    /// A structured page description.
    Structure,
    /// A decoded content operator stream.
    Operators,
    /// Exact encoded stream bytes.
    EncodedBytes,
    /// Decoded (inflated) stream bytes.
    DecodedBytes,
    /// Exact source bytes.
    ExactBytes,
    /// A deterministic structured page preview.
    Preview,
    /// The full source document.
    FullDocument,
}

impl Representation {
    /// Stable lower-case name.
    pub const fn name(self) -> &'static str {
        match self {
            Representation::Metadata => "metadata",
            Representation::Text => "text",
            Representation::Structure => "structure",
            Representation::Operators => "operators",
            Representation::EncodedBytes => "encoded",
            Representation::DecodedBytes => "decoded",
            Representation::ExactBytes => "exact",
            Representation::Preview => "preview",
            Representation::FullDocument => "full",
        }
    }
}

/// Output bounds for one observation.
#[derive(Debug, Clone, Copy)]
pub struct ObserveBudget {
    /// Maximum bytes the returned value may occupy.
    pub max_output_bytes: u64,
    /// Maximum seed nodes the observation may evaluate.
    pub max_nodes: u64,
}

impl Default for ObserveBudget {
    fn default() -> Self {
        ObserveBudget {
            max_output_bytes: 64 * 1024 * 1024,
            max_nodes: 1 << 20,
        }
    }
}

/// One observation request.
#[derive(Debug, Clone)]
pub struct ObserveRequest {
    /// What to observe.
    pub selector: Selector,
    /// How to represent it.
    pub representation: Representation,
    /// Output bounds.
    pub budget: ObserveBudget,
    /// Whether the disposable derived cache (11.8) may be consulted and filled.
    /// `false` forces a cold, recompute-everything court.
    pub use_cache: bool,
}

impl ObserveRequest {
    /// A request with the default budget, caching enabled.
    pub fn new(selector: Selector, representation: Representation) -> Self {
        ObserveRequest {
            selector,
            representation,
            budget: ObserveBudget::default(),
            use_cache: true,
        }
    }
}

/// Which descriptor read path an observation took.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DescriptorReadMode {
    /// The whole `.voldoc` blob was read and parsed (the archival/full path).
    #[default]
    Full,
    /// A seek-based partial read served only the record closure the query needs.
    Partial,
}

impl DescriptorReadMode {
    /// Stable lower-case name (used in EXPLAIN ANALYZE JSON).
    pub const fn name(self) -> &'static str {
        match self {
            DescriptorReadMode::Full => "full",
            DescriptorReadMode::Partial => "partial",
        }
    }
}

/// Statistics of one observation — the evidence surface (ADR-0027).
///
/// Peak RSS and CPU time are deliberately **not** claimed here: `std` exposes no
/// portable CPU-time API, and peak RSS is a Linux-only `/proc` read that the court
/// already measures externally (the court reads `Maximum resident set size` from
/// `/usr/bin/time -v`). Claiming either in-process would add a platform-specific, easily-misread field for no gain.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObserveStats {
    /// Index entries consulted during the observation (the reference descent is
    /// counted by the entries it returns).
    pub index_nodes_read: u64,
    /// Seed nodes fetched from the seed store.
    pub seed_nodes_fetched: u64,
    /// Seed nodes evaluated/materialized (including dependencies). With reuse
    /// enabled this counts cache misses only, so it never exceeds
    /// `seed_nodes_executed`.
    pub seed_nodes_materialized: u64,
    /// Seed nodes actually executed during this observation (cache misses).
    pub seed_nodes_executed: u64,
    /// Seed nodes served whole from the persisted derived cache (their subtrees
    /// were not traversed).
    pub seed_nodes_reused: u64,
    /// Output bytes written to the derived cache during this observation.
    pub cache_bytes_written: u64,
    /// Descriptor bytes physically fetched to open this observation's field.
    /// For the full path this is the whole `.voldoc` blob; for the seek-based
    /// partial path it is only the record closure the query needed (see
    /// [`Self::descriptor_read_mode`]). It is **not** hidden behind `bytes_read`.
    pub descriptor_bytes_read: u64,
    /// Which descriptor read path produced [`Self::descriptor_bytes_read`]:
    /// `full` for the whole-blob parse, `partial` for a seek-based closure read.
    pub descriptor_read_mode: DescriptorReadMode,
    /// Field-manifest bytes physically fetched.
    pub manifest_bytes_read: u64,
    /// Hierarchical-index-node bytes physically fetched.
    pub index_bytes_read: u64,
    /// Seed-node bytes physically fetched (`get_node` + `get_node_range`).
    pub seed_bytes_read: u64,
    /// Total **physical** bytes fetched by this observation:
    /// `descriptor_bytes_read + manifest_bytes_read + index_bytes_read +
    /// seed_bytes_read`. Unrelated to `bytes_returned` (the output size): a
    /// narrow observation can return fewer bytes than it reads.
    pub bytes_read: u64,
    /// Bytes returned to the caller.
    pub bytes_returned: u64,
    /// Whether a Stage-C promotion (deepening) happened during this observation.
    pub deepened: bool,
    /// Wall-clock duration in microseconds.
    pub wall_micros: u64,
}

/// A seed-store wrapper that counts node *fetches*. All physical bytes are
/// accounted by the underlying store's [`crate::store::IoCounters`]; this wrapper
/// only exposes the fetch count for `seed_nodes_fetched`. Generic over the seed
/// substrate so a test can substitute a store that forbids enumeration.
struct CountingSeedStore<S: SeedStore> {
    inner: S,
    gets: Cell<u64>,
}

impl<S: SeedStore> CountingSeedStore<S> {
    fn new(inner: S) -> Self {
        CountingSeedStore {
            inner,
            gets: Cell::new(0),
        }
    }

    fn gets(&self) -> u64 {
        self.gets.get()
    }

    fn note(&self) {
        self.gets.set(self.gets.get() + 1);
    }
}

impl<S: SeedStore> SeedStore for CountingSeedStore<S> {
    fn put_node(&mut self, canonical: &[u8]) -> Result<NodeId> {
        self.inner.put_node(canonical)
    }

    fn get_node(&self, id: &NodeId) -> Result<Vec<u8>> {
        self.note();
        self.inner.get_node(id)
    }

    fn get_node_range(&self, id: &NodeId, offset: u64, len: u64) -> Result<Vec<u8>> {
        self.note();
        self.inner.get_node_range(id, offset, len)
    }

    fn contains_node(&self, id: &NodeId) -> Result<bool> {
        self.inner.contains_node(id)
    }

    fn list_nodes(&self) -> Result<Vec<(NodeId, u64)>> {
        self.inner.list_nodes()
    }
}

/// Observe one selector/representation pair.
///
/// Returns the answer, its [`ObserveStats`], and the **current/promoted** field
/// id: the promoted id when a Stage-C deepen happened during this call, else the
/// input id. A caller can chain the returned id to observe again without
/// re-deepening.
pub fn observe(
    store: &mut FieldStore,
    id: &FieldId,
    req: &ObserveRequest,
    limits: Limits,
) -> Result<(FieldAnswer, ObserveStats, FieldId)> {
    let started = Instant::now();
    match narrow_probe(store, id, req)? {
        // The target is served wholly from the disposable derived cache: the
        // descriptor is never opened. The ordinary evaluation core still runs,
        // against a trip-wire source, so the answer and every work counter are
        // those of the normal path while `descriptor_bytes_read` stays zero.
        NarrowProbe::Probed {
            hit: true,
            manifest,
            carry,
        } => {
            let view = FieldView {
                manifest: manifest.as_ref(),
                id: *id,
                open_io: IoSnapshot::default(),
                source: &NO_SOURCE,
                loader: None,
                object_count: 0,
                graph_ops: 0,
                read_mode: DescriptorReadMode::Partial,
            };
            let (seeds, istore) = open_sub_stores(store)?;
            observe_with_stores_pre(store, view, req, limits, started, seeds, istore, carry)
        }
        // The selector resolved from the manifest + hierarchical index, but the
        // target is not cached: fall through to the normal path, reusing the
        // manifest, the probe's physical bytes, and its resolved index entries
        // so nothing is read a second time.
        NarrowProbe::Probed {
            hit: false,
            manifest,
            carry,
        } => {
            let opened = OpenedField::open_with_manifest(store, req, *manifest, limits)?;
            observe_view_pre(store, opened.view(), req, limits, started, carry)
        }
        NarrowProbe::NotEligible => {
            let opened = OpenedField::open(store, id, req, limits)?;
            observe_view(store, opened.view(), req, limits, started)
        }
    }
}

/// A [`SourceServer`] that serves nothing. A fully cache-served observation must
/// never call it; reaching it means the short-circuit admitted a request it
/// could not answer from the cache, which is a hard internal invariant failure
/// rather than a silent descriptor read.
struct NoSource;

static NO_SOURCE: NoSource = NoSource;

impl SourceServer for NoSource {
    fn serve_range(&self, _offset: u64, _len: u64, _limits: Limits) -> Result<Vec<u8>> {
        Err(Error::internal_invariant(
            "a cache-served observation attempted a descriptor range read",
        ))
    }

    fn serve_document(&self, _limits: Limits) -> Result<Vec<u8>> {
        Err(Error::internal_invariant(
            "a cache-served observation attempted a descriptor document read",
        ))
    }
}

/// Hierarchical-index entries the cache-first probe already resolved, so the
/// evaluation that follows never reads the same index nodes a second time.
#[derive(Default)]
struct PrefetchedIndex {
    entries: Vec<(SelectorKey, Vec<IndexEntry>)>,
}

impl PrefetchedIndex {
    fn insert(&mut self, key: SelectorKey, entries: Vec<IndexEntry>) {
        self.entries.push((key, entries));
    }

    fn get(&self, key: &SelectorKey) -> Option<&Vec<IndexEntry>> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
}

/// State the cache-first probe already produced, carried into the path that
/// follows so nothing it fetched, resolved, or read is done twice.
#[derive(Default)]
struct ProbeCarry {
    /// Physical bytes the probe fetched before the field was opened.
    base_io: IoSnapshot,
    /// Hierarchical-index entries the probe resolved.
    prefetched: PrefetchedIndex,
    /// The target's cache bytes, already integrity-checked by the probe, so the
    /// evaluation core serves them without re-reading the cache file.
    output: Option<(NodeId, Vec<u8>)>,
}

/// The outcome of the cache-first narrow probe.
enum NarrowProbe {
    /// The request is not one the short-circuit serves.
    NotEligible,
    /// The selector resolved without reading the descriptor. `hit` is whether
    /// the target derived node is served by the disposable cache.
    Probed {
        hit: bool,
        manifest: Box<FieldRoot>,
        carry: ProbeCarry,
    },
}

/// Resolve a narrow observation's target node from the field manifest and the
/// hierarchical index **only** — never the descriptor — and report whether the
/// disposable cache can serve it whole.
///
/// The target ids are computed with the same constructors `ingest`/`deepen` use
/// ([`derived_nodes`], the `ContentOperators`/`PdfStreamDecoded` builders), so a
/// hit means the *identical* node the normal path would materialize. A miss
/// carries the manifest, the physical bytes the probe fetched, and the resolved
/// index entries back to the normal path so a cold observation pays nothing
/// extra.
fn narrow_probe(store: &FieldStore, id: &FieldId, req: &ObserveRequest) -> Result<NarrowProbe> {
    use Representation as R;
    if !req.use_cache {
        return Ok(NarrowProbe::NotEligible);
    }
    // The short-circuit is a further step of the seek-based *partial* lane: on a
    // backend with no partial descriptor (EntropyFS) the honest label would be
    // `full`, so leave that path unchanged.
    if !store.supports_partial_descriptor() {
        return Ok(NarrowProbe::NotEligible);
    }
    let cacheable = matches!(
        (&req.selector, req.representation),
        (Selector::Page(_), R::Text | R::Preview | R::Structure)
            | (Selector::Stream(_), R::DecodedBytes | R::Operators)
    );
    if !cacheable {
        return Ok(NarrowProbe::NotEligible);
    }

    let io_before = store.io().snapshot();
    let manifest = store.get_field(id)?;
    let mut prefetched = PrefetchedIndex::default();
    if !manifest.has_index() {
        // Nothing to resolve from; let the normal path produce its typed error.
        let base_io = io_before.delta(&store.io().snapshot());
        return Ok(NarrowProbe::Probed {
            hit: false,
            manifest: Box::new(manifest),
            carry: ProbeCarry {
                base_io,
                prefetched,
                output: None,
            },
        });
    }

    let istore = FsIndexStore::open_with_io(store.root(), store.io().handle())?;
    let root = NodeId::from_bytes(manifest.index_root);
    let seeds = store.seed_substrate();

    // Compute the deterministic target `(id, max_output_bytes)`.
    let target: Option<(NodeId, u64)> = match (&req.selector, req.representation) {
        (Selector::Page(page), R::Text | R::Preview | R::Structure) => {
            let key = SelectorKey::new(SEL_PAGE, *page);
            let entries = lookup(&istore, &root, &key)?;
            prefetched.insert(key, entries.clone());
            match entries.first() {
                Some(entry) => {
                    let (ops, text, preview) = derived_nodes(*page, entry.node_id);
                    // The short-circuit only applies once the whole derived chain
                    // already exists, so the normal path cannot promote (deepen)
                    // and the answer is a pure cache read.
                    if seeds.contains_node(&ops.content_id())?
                        && seeds.contains_node(&text.content_id())?
                        && seeds.contains_node(&preview.content_id())?
                    {
                        let node = match req.representation {
                            R::Preview | R::Structure => preview,
                            _ => text,
                        };
                        Some((node.content_id(), node.limits.max_output_bytes))
                    } else {
                        None
                    }
                }
                None => None,
            }
        }
        (Selector::Stream(object), R::DecodedBytes | R::Operators) => {
            let enc_key = SelectorKey::new(SEL_STREAM, *object);
            let enc = lookup(&istore, &root, &enc_key)?;
            prefetched.insert(enc_key, enc.clone());
            let dec_key = SelectorKey::new(SEL_STREAM_DECODED, *object);
            let dec = lookup(&istore, &root, &dec_key)?;
            prefetched.insert(dec_key, dec.clone());
            // The normal path requires a `SEL_STREAM` entry and, for a pure cache
            // hit, an already-registered decoded node; otherwise it would deepen
            // from the descriptor.
            if enc.is_empty() {
                None
            } else {
                match dec.first() {
                    // The index entry's id *is* the decoded node's content id.
                    // Every decoded node is built with `NodeLimits::DEFAULT`.
                    Some(entry) if req.representation == R::DecodedBytes => Some((
                        entry.node_id,
                        crate::field::node::NodeLimits::DEFAULT.max_output_bytes,
                    )),
                    Some(entry) => {
                        let node = SeedNode::new(
                            NodeKind::ContentOperators,
                            0,
                            Vec::new(),
                            vec![entry.node_id],
                            "pdf:content-operators",
                        );
                        Some((node.content_id(), node.limits.max_output_bytes))
                    }
                    None => None,
                }
            }
        }
        _ => None,
    };

    // Read the target's cached bytes at most once here: a hit is served from
    // this buffer, so neither the cache nor the descriptor is read again.
    let (hit, output) = match target {
        // Mirror `dag::materialize_inner`'s hit guard exactly: a cache error or
        // an oversized entry is a miss, never a wrong answer.
        Some((target_id, max_output_bytes)) => {
            match DerivedCache::open(store.root().join("cache"))?.get(&target_id) {
                Ok(Some(bytes)) if bytes.len() as u64 <= max_output_bytes => {
                    (true, Some((target_id, bytes)))
                }
                _ => (false, None),
            }
        }
        None => (false, None),
    };
    let base_io = io_before.delta(&store.io().snapshot());
    Ok(NarrowProbe::Probed {
        hit,
        manifest: Box::new(manifest),
        carry: ProbeCarry {
            base_io,
            prefetched,
            output,
        },
    })
}

/// Observe against an already-opened [`OpenedField`], for callers that open once
/// and both plan and evaluate (e.g. EXPLAIN ANALYZE).
pub(crate) fn observe_opened(
    store: &mut FieldStore,
    opened: &OpenedField,
    req: &ObserveRequest,
    limits: Limits,
) -> Result<(FieldAnswer, ObserveStats, FieldId)> {
    let started = Instant::now();
    observe_view(store, opened.view(), req, limits, started)
}

/// Observe against an **already-open** field, opening nothing extra.
///
/// This is the single-open entry point (review fix #2): [`crate::field::explain::explain_analyze`]
/// opens the field once, plans against it, and then evaluates the observation
/// here, so the descriptor blob is read exactly once per analysis instead of
/// twice. Physical bytes are attributed from the store's I/O counters, so the
/// bytes read to open `field` are still reported honestly.
pub fn observe_with_field(
    store: &mut FieldStore,
    field: &Field,
    req: &ObserveRequest,
    limits: Limits,
) -> Result<(FieldAnswer, ObserveStats, FieldId)> {
    let started = Instant::now();
    observe_view(store, FieldView::from_field(field), req, limits, started)
}

/// A descriptor opened for one observation: the full parse, or a seek-based
/// partial loader when the request is narrow and the descriptor carries an op
/// table. Both expose a [`FieldView`] over the same evaluation core.
pub(crate) enum OpenedField {
    /// The whole `.voldoc` blob was read and parsed.
    Full(Box<Field>),
    /// Only the record closure the observation needs will be read.
    Partial(Box<PartialField>),
}

impl OpenedField {
    /// Open the cheapest descriptor path admissible for `req`.
    pub(crate) fn open(
        store: &FieldStore,
        id: &FieldId,
        req: &ObserveRequest,
        limits: Limits,
    ) -> Result<OpenedField> {
        if store.supports_partial_descriptor()
            && partial_eligible(req)
            && let Some(pf) = PartialField::try_open(store, id, limits)?
        {
            return Ok(OpenedField::Partial(Box::new(pf)));
        }
        Ok(OpenedField::Full(Box::new(Field::open(store, id, limits)?)))
    }

    /// Open the cheapest admissible path from an **already-read** manifest. The
    /// manifest bytes are charged by the caller (the narrow probe counts them in
    /// its `base_io`), so `open_io` here never re-reads them.
    pub(crate) fn open_with_manifest(
        store: &FieldStore,
        req: &ObserveRequest,
        manifest: FieldRoot,
        limits: Limits,
    ) -> Result<OpenedField> {
        if store.supports_partial_descriptor()
            && partial_eligible(req)
            && let Some(pf) =
                PartialField::finish_open(store, manifest.clone(), store.io().snapshot(), limits)?
        {
            return Ok(OpenedField::Partial(Box::new(pf)));
        }
        Ok(OpenedField::Full(Box::new(Field::open_after_manifest(
            store,
            manifest,
            store.io().snapshot(),
            limits,
        )?)))
    }

    /// A view over this opened field for the evaluation core.
    pub(crate) fn view(&self) -> FieldView<'_> {
        match self {
            OpenedField::Full(f) => FieldView::from_field(f),
            OpenedField::Partial(p) => p.view(),
        }
    }

    /// The field manifest.
    pub(crate) fn manifest(&self) -> &FieldRoot {
        match self {
            OpenedField::Full(f) => f.manifest(),
            OpenedField::Partial(p) => &p.manifest,
        }
    }
}

/// The metadata and source server one observation needs, independent of whether
/// the descriptor was fully parsed or partially loaded.
pub(crate) struct FieldView<'a> {
    pub manifest: &'a FieldRoot,
    pub id: FieldId,
    pub open_io: IoSnapshot,
    pub source: &'a dyn SourceServer,
    /// The partial loader, when this view came from one, so the evaluation can
    /// charge the bytes its record reads fetched.
    pub loader: Option<&'a PartialDescriptor>,
    pub object_count: usize,
    pub graph_ops: usize,
    pub read_mode: DescriptorReadMode,
}

impl<'a> FieldView<'a> {
    pub(crate) fn from_field(field: &'a Field) -> FieldView<'a> {
        let parsed = field.parsed();
        FieldView {
            manifest: field.manifest(),
            id: field.id(),
            open_io: field.open_io(),
            source: parsed,
            loader: None,
            object_count: parsed.descriptor.objects.len(),
            graph_ops: parsed.descriptor.program.ops.len(),
            read_mode: DescriptorReadMode::Full,
        }
    }
}

/// A field opened through the seek-based partial descriptor loader.
pub(crate) struct PartialField {
    pub(crate) manifest: FieldRoot,
    pub(crate) id: FieldId,
    pub(crate) open_io: IoSnapshot,
    loader: PartialDescriptor,
}

impl PartialField {
    /// Try to open `id` lazily. `Ok(None)` means the descriptor is ineligible
    /// (no op table, external objects, or a framing fault) and the caller must
    /// fall back to the full path.
    pub(crate) fn try_open(
        store: &FieldStore,
        id: &FieldId,
        limits: Limits,
    ) -> Result<Option<PartialField>> {
        let io_before = store.io().snapshot();
        let manifest = store.get_field(id)?;
        PartialField::finish_open(store, manifest, io_before, limits)
    }

    /// The body of [`PartialField::try_open`] from an already-read manifest.
    ///
    /// `io_before` is the snapshot `open_io` is measured from: pass one taken
    /// *before* the manifest read to charge it to this open (the ordinary
    /// path), or one taken after it to charge it elsewhere (the narrow probe,
    /// which already counted the manifest in its `base_io`).
    pub(crate) fn finish_open(
        store: &FieldStore,
        manifest: FieldRoot,
        io_before: IoSnapshot,
        limits: Limits,
    ) -> Result<Option<PartialField>> {
        let descriptor_id = Id::from_bytes(manifest.descriptor_id);
        let Some(path) = store.descriptor_path(&descriptor_id) else {
            // The descriptor is not a filesystem file (EntropyFS backend): the
            // seek-based partial lane is unavailable, so fall back to the full
            // descriptor path.
            return Ok(None);
        };
        let loader = match PartialDescriptor::open(&path, limits)? {
            PartialLoad::Ready(l) => l,
            PartialLoad::Ineligible { bytes_read } => {
                // Charge the bytes the inspection did fetch before declining, so
                // the honest fallback is not under-counted.
                store.io().add_descriptor(bytes_read);
                return Ok(None);
            }
        };
        if loader.source_len() != manifest.source_len
            || loader.source_sha256() != manifest.source_sha256
        {
            return Err(Error::integrity_mismatch(
                "partial descriptor does not match its field manifest's declared source",
            ));
        }
        // The loader's physical bytes are charged to the descriptor class after
        // the observation completes (once, so the read *count* stays one), so the
        // open snapshot carries only the manifest read here.
        let open_io = io_before.delta(&store.io().snapshot());
        let id = manifest.content_id();
        Ok(Some(PartialField {
            id,
            manifest,
            open_io,
            loader: *loader,
        }))
    }

    pub(crate) fn view(&self) -> FieldView<'_> {
        FieldView {
            manifest: &self.manifest,
            id: self.id,
            open_io: self.open_io,
            source: &self.loader,
            loader: Some(&self.loader),
            object_count: self.loader.object_count(),
            graph_ops: self.loader.graph_ops(),
            read_mode: DescriptorReadMode::Partial,
        }
    }
}

/// Whether a request is served by the seek-based partial lane when available.
fn partial_eligible(req: &ObserveRequest) -> bool {
    use Representation as R;
    matches!(
        (&req.selector, req.representation),
        (Selector::ByteRange { .. }, R::ExactBytes)
            | (Selector::Object(_), R::ExactBytes | R::EncodedBytes)
            | (Selector::Revision(_), R::ExactBytes)
            | (Selector::Stream(_), R::EncodedBytes)
            | (Selector::Member(_), R::EncodedBytes | R::DecodedBytes)
            | (Selector::Page(_), R::Text | R::Preview | R::Structure)
    )
}

/// Build the seed and index sub-stores, sharing the field store's I/O counters.
fn open_sub_stores(store: &FieldStore) -> Result<(CountingSeedStore<SeedSubstrate>, FsIndexStore)> {
    let io = store.io();
    let seeds = CountingSeedStore::new(store.seed_substrate());
    let istore = FsIndexStore::open_with_io(store.root(), io.handle())?;
    Ok((seeds, istore))
}

fn observe_view<'a>(
    store: &'a mut FieldStore,
    view: FieldView<'a>,
    req: &ObserveRequest,
    limits: Limits,
    started: Instant,
) -> Result<(FieldAnswer, ObserveStats, FieldId)> {
    observe_view_pre(store, view, req, limits, started, ProbeCarry::default())
}

/// [`observe_view`] with the cache-first probe's carried state: physical bytes it
/// already fetched, index entries it resolved, and integrity-checked cache bytes
/// — so the evaluation core never reads any of them a second time.
fn observe_view_pre<'a>(
    store: &'a mut FieldStore,
    view: FieldView<'a>,
    req: &ObserveRequest,
    limits: Limits,
    started: Instant,
    carry: ProbeCarry,
) -> Result<(FieldAnswer, ObserveStats, FieldId)> {
    let (seeds, istore) = open_sub_stores(store)?;
    observe_with_stores_pre(store, view, req, limits, started, seeds, istore, carry)
}

/// Test-only wrapper over [`observe_with_stores_pre`] with no probe state, so a
/// test can supply a seed store wrapper that forbids enumeration.
#[cfg(test)]
fn observe_with_stores<'a, S: SeedStore>(
    store: &'a mut FieldStore,
    view: FieldView<'a>,
    req: &ObserveRequest,
    limits: Limits,
    started: Instant,
    seeds: CountingSeedStore<S>,
    istore: FsIndexStore,
) -> Result<(FieldAnswer, ObserveStats, FieldId)> {
    observe_with_stores_pre(
        store,
        view,
        req,
        limits,
        started,
        seeds,
        istore,
        ProbeCarry::default(),
    )
}

/// The evaluation core. Takes explicit sub-stores so a test can supply a seed
/// store wrapper that forbids enumeration; `carry` is the cache-first probe's
/// already-counted bytes, resolved index entries, and cached target bytes.
#[allow(clippy::too_many_arguments)]
fn observe_with_stores_pre<'a, S: SeedStore>(
    store: &'a mut FieldStore,
    view: FieldView<'a>,
    req: &ObserveRequest,
    limits: Limits,
    started: Instant,
    seeds: CountingSeedStore<S>,
    istore: FsIndexStore,
    carry: ProbeCarry,
) -> Result<(FieldAnswer, ObserveStats, FieldId)> {
    let ProbeCarry {
        base_io,
        prefetched,
        output,
    } = carry;
    // Snapshot after the field is open: only the reads this observation performs
    // during evaluation are counted as deltas; the field-open bytes come from
    // `view.open_io` below so they cannot be dropped on the floor.
    let io_base = store.io().snapshot();
    let budget = EvalBudget {
        max_nodes: req.budget.max_nodes,
        ..EvalBudget::default()
    };
    let cache = DerivedCache::open(store.root().join("cache"))?;
    let field_id = view.id;
    let mut ctx = Ctx {
        store,
        manifest: view.manifest,
        source: view.source,
        loader: view.loader,
        open_io: view.open_io,
        object_count: view.object_count,
        graph_ops: view.graph_ops,
        read_mode: view.read_mode,
        seeds,
        istore,
        prefetched,
        prefetched_output: output,
        limits,
        budget,
        stats: ObserveStats::default(),
        use_cache: req.use_cache,
        cache,
        reuse: ReuseStats::default(),
        current_id: field_id,
    };

    let answer = ctx.dispatch(req)?;
    let produced = answer.value.byte_len();
    if produced > req.budget.max_output_bytes {
        return Err(Error::resource_limit(format!(
            "observation produced {produced} bytes, exceeding the {}-byte budget",
            req.budget.max_output_bytes
        )));
    }

    let mut stats = ctx.stats;
    // A partial loader reads its records lazily, so its physical bytes accrue
    // during dispatch; charge them once (one descriptor read *count*) before
    // closing the interval.
    if let Some(loader) = ctx.loader {
        ctx.store.io().add_descriptor(loader.bytes_read());
    }
    // Every physical byte fetched by this observation: the probe's `base_io`, the
    // field-open bytes, plus any additional reads (e.g. a Stage-C promotion)
    // performed during dispatch.
    let open = ctx.open_io;
    let extra = io_base.delta(&ctx.store.io().snapshot());
    stats.descriptor_bytes_read = base_io
        .descriptor_bytes
        .saturating_add(open.descriptor_bytes)
        .saturating_add(extra.descriptor_bytes);
    stats.descriptor_read_mode = ctx.read_mode;
    stats.manifest_bytes_read = base_io
        .manifest_bytes
        .saturating_add(open.manifest_bytes)
        .saturating_add(extra.manifest_bytes);
    stats.index_bytes_read = base_io.index_bytes.saturating_add(extra.index_bytes);
    stats.seed_bytes_read = base_io.seed_bytes.saturating_add(extra.seed_bytes);
    stats.bytes_read = stats
        .descriptor_bytes_read
        .saturating_add(stats.manifest_bytes_read)
        .saturating_add(stats.index_bytes_read)
        .saturating_add(stats.seed_bytes_read);
    stats.seed_nodes_fetched = ctx.seeds.gets();
    stats.seed_nodes_materialized = ctx.budget.nodes;
    stats.seed_nodes_executed = ctx.reuse.nodes_executed;
    stats.seed_nodes_reused = ctx.reuse.nodes_reused;
    stats.cache_bytes_written = ctx.reuse.cache_bytes_written;
    stats.bytes_returned = produced;
    stats.wall_micros = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
    Ok((answer, stats, ctx.current_id))
}

/// Observation execution context.
struct Ctx<'a, S: SeedStore> {
    store: &'a mut FieldStore,
    manifest: &'a FieldRoot,
    source: &'a dyn SourceServer,
    loader: Option<&'a PartialDescriptor>,
    open_io: IoSnapshot,
    object_count: usize,
    graph_ops: usize,
    read_mode: DescriptorReadMode,
    seeds: CountingSeedStore<S>,
    istore: FsIndexStore,
    /// Index entries the cache-first probe already resolved, keyed by selector.
    prefetched: PrefetchedIndex,
    /// The target's cache bytes, already read and integrity-checked by the probe.
    prefetched_output: Option<(NodeId, Vec<u8>)>,
    limits: Limits,
    budget: EvalBudget,
    stats: ObserveStats,
    use_cache: bool,
    cache: DerivedCache,
    reuse: ReuseStats,
    current_id: FieldId,
}

/// A resolved DOCX story view: the parsed story model plus the provenance it is
/// bound to (backing part, dependency ids, and the exact compressed member span).
#[cfg(feature = "docx")]
struct DocxStoryView {
    model: StoryModel,
    part: DocxPartRef,
    deps: Vec<NodeId>,
    span: Option<(u64, u64)>,
}

#[cfg(feature = "docx")]
fn opt_u8_json(v: Option<u8>) -> String {
    match v {
        Some(n) => n.to_string(),
        None => "null".to_string(),
    }
}

#[cfg(feature = "docx")]
fn opt_str_json(v: Option<&str>) -> String {
    match v {
        Some(s) => format!("\"{}\"", json_escape(s)),
        None => "null".to_string(),
    }
}

/// Parse an A1-style cell reference (`B7`) into a 0-based grid column and a
/// 0-based row index. Column letters are case-insensitive; row numbers are
/// 1-based and must be non-zero.
#[cfg(feature = "docx")]
fn parse_cell_ref(s: &str) -> Option<(u32, u32)> {
    let letters: String = s.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let digits: String = s.chars().skip(letters.len()).collect();
    if letters.is_empty() || digits.is_empty() || digits.len() != s.len() - letters.len() {
        return None;
    }
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut col: u32 = 0;
    for c in letters.chars() {
        let v = c.to_ascii_uppercase() as u32 - 'A' as u32 + 1;
        col = col.checked_mul(26)?.checked_add(v)?;
    }
    let col = col.checked_sub(1)?;
    let row: u32 = digits.parse().ok()?;
    if row == 0 {
        return None;
    }
    Some((col, row - 1))
}

impl<S: SeedStore> Ctx<'_, S> {
    fn materialize(&mut self, node: &SeedNode) -> Result<Vec<u8>> {
        let depth = node.limits.max_depth;
        if self.use_cache {
            // The cache-first probe may have already read and integrity-checked
            // this exact node's output. Serving it here is byte-identical to a
            // cache hit and avoids reading the entry a second time.
            if let Some((id, bytes)) = self.prefetched_output.take() {
                if id == node.content_id() {
                    self.reuse.nodes_reused = self.reuse.nodes_reused.saturating_add(1);
                    self.budget.charge_bytes(bytes.len() as u64)?;
                    return Ok(bytes);
                }
                self.prefetched_output = Some((id, bytes));
            }
            dag::materialize_node_cached_with(
                self.source,
                &self.seeds,
                &mut self.cache,
                node,
                self.limits,
                &mut self.budget,
                depth,
                &mut self.reuse,
            )
        } else {
            let mut cache = dag::NoCache;
            dag::materialize_node_cached_with(
                self.source,
                &self.seeds,
                &mut cache,
                node,
                self.limits,
                &mut self.budget,
                depth,
                &mut self.reuse,
            )
        }
    }

    fn load(&self, id: &NodeId) -> Result<SeedNode> {
        dag::load_node(&self.seeds, id)
    }

    fn lookup(&mut self, key: SelectorKey) -> Result<Vec<IndexEntry>> {
        if !self.manifest.has_index() {
            return Ok(Vec::new());
        }
        // A probe-resolved key is served from memory: the index nodes were read
        // (and charged) before the field opened, so reading them again would both
        // double the bytes and lie about the work. Counting the entries keeps
        // `index_nodes_read` identical to the normal path.
        let prefetched = self.prefetched.get(&key).cloned();
        let entries = match prefetched {
            Some(entries) => entries,
            None => {
                let root = NodeId::from_bytes(self.manifest.index_root);
                lookup(&self.istore, &root, &key)?
            }
        };
        self.stats.index_nodes_read += entries.len() as u64;
        Ok(entries)
    }

    fn require_entry(&mut self, key: SelectorKey, what: &str) -> Result<IndexEntry> {
        let entries = self.lookup(key)?;
        entries.into_iter().next().ok_or_else(|| {
            Error::unsupported_feature(format!(
                "no {what} matching selector number {} in the observation index",
                key.number
            ))
        })
    }

    fn dispatch(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        use Representation as R;
        match (&req.selector, req.representation) {
            (Selector::Document, R::FullDocument | R::ExactBytes) => self.document_full(req),
            (Selector::Document, R::Metadata) => self.document_metadata(req),
            (Selector::ByteRange { offset, len }, R::ExactBytes) => {
                self.byte_range(req, *offset, *len)
            }
            (Selector::Object(n), R::ExactBytes | R::EncodedBytes) => {
                self.indexed_exact(req, SelectorKey::new(SEL_OBJECT, *n), "object")
            }
            (Selector::Revision(n), R::ExactBytes) => {
                self.indexed_exact(req, SelectorKey::new(SEL_REVISION, *n), "revision")
            }
            (Selector::Member(n), R::EncodedBytes) => self.indexed_exact(
                req,
                SelectorKey::new(SEL_PACKAGE_MEMBER_RAW, *n),
                "package member",
            ),
            (Selector::Member(n), R::DecodedBytes) => self.member_decoded(req, *n),
            (Selector::PackagePart(_), R::Metadata | R::ExactBytes | R::DecodedBytes) => {
                self.package_part_opc(req)
            }
            (Selector::Relationship(_), R::Metadata | R::ExactBytes | R::DecodedBytes) => {
                self.relationship_opc(req)
            }
            (Selector::Stream(n), R::EncodedBytes) => {
                self.indexed_exact(req, SelectorKey::new(SEL_STREAM, *n), "stream")
            }
            (Selector::Stream(n), R::DecodedBytes) => self.stream_decoded(req, *n),
            (Selector::Stream(n), R::Operators) => self.stream_operators(req, *n),
            (Selector::Page(n), R::Text) => self.page_text(req, *n),
            (Selector::Page(n), R::Preview) => self.page_preview(req, *n),
            (Selector::Page(n), R::Structure) => self.page_structure(req, *n),
            (Selector::TextMatch(p), R::Text) => self.text_match(req, p),
            #[cfg(feature = "docx")]
            (Selector::DocxStory { story, profile }, R::Text) => {
                self.docx_story_text(req, *story, profile)
            }
            #[cfg(feature = "docx")]
            (Selector::DocxStory { story, profile }, R::Structure) => {
                self.docx_story_structure(req, *story, profile)
            }
            #[cfg(feature = "docx")]
            (Selector::DocxStory { story, profile }, R::Metadata) => {
                self.docx_story_metadata(req, *story, profile)
            }
            #[cfg(feature = "docx")]
            (
                Selector::DocxParagraph {
                    story,
                    index,
                    profile,
                },
                R::Text | R::Metadata,
            ) => self.docx_paragraph(req, *story, *index, profile),
            #[cfg(feature = "docx")]
            (
                Selector::DocxTable {
                    story,
                    index,
                    profile,
                },
                R::Text | R::Metadata,
            ) => self.docx_table(req, *story, *index, profile),
            #[cfg(feature = "docx")]
            (
                Selector::DocxCell {
                    story,
                    table,
                    cell,
                    profile,
                },
                R::Text | R::Metadata,
            ) => self.docx_cell(req, *story, *table, cell, profile),
            #[cfg(feature = "docx")]
            (
                Selector::DocxFind {
                    story,
                    pattern,
                    profile,
                },
                R::Text,
            ) => self.docx_find(req, *story, pattern, profile),
            _ => Err(Error::unsupported_feature(format!(
                "unsupported observation: selector {} with representation {}",
                req.selector.canonical(),
                req.representation.name()
            ))),
        }
    }

    fn document_full(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        let bytes = self.source.serve_document(self.limits)?;
        Ok(FieldAnswer {
            value: AnswerValue::Bytes(bytes),
            basis: Basis::DirectlyObserved,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: Some((0, self.manifest.source_len)),
            provenance: String::new(),
            dependency_ids: vec![self.manifest.root_node],
            integrity_scope: IntegrityScope::WholeSource,
            exact: true,
        })
    }

    fn document_metadata(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        let json = format!(
            concat!(
                "{{",
                "\"source_len\":{},",
                "\"source_sha256\":\"{}\",",
                "\"object_count\":{},",
                "\"graph_ops\":{},",
                "\"node_count\":{}",
                "}}"
            ),
            self.manifest.source_len,
            crate::integrity::to_hex(&self.manifest.source_sha256),
            self.object_count,
            self.graph_ops,
            self.manifest.node_count,
        );
        Ok(FieldAnswer {
            value: AnswerValue::Json(json),
            basis: Basis::DeterministicallyDerived,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: None,
            provenance: String::new(),
            dependency_ids: Vec::new(),
            integrity_scope: IntegrityScope::None,
            exact: false,
        })
    }

    fn byte_range(&mut self, req: &ObserveRequest, offset: u64, len: u64) -> Result<FieldAnswer> {
        let end = offset
            .checked_add(len)
            .ok_or_else(|| Error::usage("byte-range end overflows"))?;
        let node = SeedNode::new(
            NodeKind::SourceSlice,
            len,
            span_params(offset, len),
            Vec::new(),
            "field:observe;source-slice",
        );
        let bytes = self.materialize(&node)?;
        Ok(FieldAnswer {
            value: AnswerValue::Bytes(bytes),
            basis: Basis::DirectlyObserved,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: Some((offset, end)),
            provenance: String::new(),
            dependency_ids: Vec::new(),
            integrity_scope: IntegrityScope::Node,
            exact: true,
        })
    }

    fn indexed_exact(
        &mut self,
        req: &ObserveRequest,
        key: SelectorKey,
        what: &str,
    ) -> Result<FieldAnswer> {
        let entry = self.require_entry(key, what)?;
        let node = self.load(&entry.node_id)?;
        let bytes = self.materialize(&node)?;
        let end = entry.out_off.saturating_add(entry.out_len);
        Ok(FieldAnswer {
            value: AnswerValue::Bytes(bytes),
            basis: Basis::DirectlyObserved,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: Some((entry.out_off, end)),
            provenance: String::new(),
            dependency_ids: vec![entry.node_id],
            integrity_scope: IntegrityScope::Node,
            exact: true,
        })
    }

    fn stream_decoded(&mut self, req: &ObserveRequest, object: u32) -> Result<FieldAnswer> {
        let entry = self.require_entry(SelectorKey::new(SEL_STREAM, object), "stream")?;
        let node = self.decoded_node(object, &entry.node_id)?;
        let id = node.content_id();
        let bytes = self.materialize(&node)?;
        Ok(FieldAnswer {
            value: AnswerValue::Bytes(bytes),
            basis: Basis::DeterministicallyDerived,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: None,
            provenance: String::new(),
            dependency_ids: vec![id],
            integrity_scope: IntegrityScope::None,
            exact: false,
        })
    }

    /// A package member's decoded bytes (Phase 12.2).
    ///
    /// Resolved through the index to the `PackageMemberDecoded` node, which is a
    /// deterministic function of its raw node, so the observation never enumerates
    /// the seed store. The answer is `DeterministicallyDerived`, never exact: it is
    /// not a byte-identical observation of the source.
    fn member_decoded(&mut self, req: &ObserveRequest, ordinal: u32) -> Result<FieldAnswer> {
        let entry = self.require_entry(
            SelectorKey::new(SEL_PACKAGE_MEMBER_DECODED, ordinal),
            "decoded package member",
        )?;
        let node = self.load(&entry.node_id)?;
        let id = node.content_id();
        let raw_deps = node.deps.clone();
        let bytes = self.materialize(&node)?;
        let mut dependency_ids = vec![id];
        dependency_ids.extend(raw_deps);
        Ok(FieldAnswer {
            value: AnswerValue::Bytes(bytes),
            basis: Basis::DeterministicallyDerived,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: None,
            provenance: String::new(),
            dependency_ids,
            integrity_scope: IntegrityScope::None,
            exact: false,
        })
    }

    /// Materialize and decode the generic OPC model (derived, `Q_gen`).
    #[cfg(feature = "opc")]
    fn opc_model(&mut self) -> Result<crate::adapter::package::opc::OpcModel> {
        let entry = self.require_entry(SelectorKey::new(SEL_OPC_MODEL, 0), "OPC model")?;
        let node = self.load(&entry.node_id)?;
        let bytes = self.materialize(&node)?;
        crate::adapter::package::opc::OpcModel::decode(&bytes)
    }

    /// A generic OPC part observation (Phase 12.3): exact/decoded bytes resolve
    /// through the OPC part's physical member ordinal, so the exact leaf stays the
    /// 12.2 raw member span. Metadata is derived (`Q_gen`).
    #[cfg(feature = "opc")]
    fn package_part_opc(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        use Representation as R;
        let Selector::PackagePart(name) = &req.selector else {
            return Err(Error::internal_invariant(
                "package_part_opc needs PackagePart",
            ));
        };
        let name = name.clone();
        let model = self.opc_model()?;
        let part = model.part_by_name(&name).ok_or_else(|| {
            Error::invalid_package_structure(format!("no package part named {name:?}"))
        })?;
        let ordinal = part.ordinal;
        match req.representation {
            R::ExactBytes => self.indexed_exact(
                req,
                SelectorKey::new(SEL_PACKAGE_MEMBER_RAW, ordinal),
                "package part",
            ),
            R::DecodedBytes => self.member_decoded(req, ordinal),
            R::Metadata => {
                let rel_count = model
                    .part_rels
                    .iter()
                    .find(|(o, _)| *o == ordinal)
                    .map_or(0, |(_, r)| r.len());
                let ct = match &part.content_type {
                    Some(c) => format!("\"{}\"", json_escape(c)),
                    None => "null".to_string(),
                };
                let json = format!(
                    "{{\"name\":\"{}\",\"ordinal\":{},\"content_type\":{},\"relationships\":{}}}",
                    json_escape(&part.name),
                    ordinal,
                    ct,
                    rel_count
                );
                Ok(FieldAnswer {
                    value: AnswerValue::Json(json),
                    basis: Basis::DeterministicallyDerived,
                    selector: req.selector.canonical(),
                    representation: req.representation.name().to_string(),
                    source_span: None,
                    provenance: String::new(),
                    dependency_ids: Vec::new(),
                    integrity_scope: IntegrityScope::None,
                    exact: false,
                })
            }
            _ => Err(Error::unsupported_feature(format!(
                "unsupported observation: selector {} with representation {}",
                req.selector.canonical(),
                req.representation.name()
            ))),
        }
    }

    /// A generic OPC relationship observation (Phase 12.3). For an internal
    /// relationship, `ExactBytes`/`DecodedBytes` resolve to the target part's member
    /// bytes. An external relationship is an inert identifier: asking for its bytes
    /// is a typed decline, never a fetch.
    #[cfg(feature = "opc")]
    fn relationship_opc(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        use Representation as R;
        let Selector::Relationship(id) = &req.selector else {
            return Err(Error::internal_invariant(
                "relationship_opc needs Relationship",
            ));
        };
        let id = id.clone();
        let model = self.opc_model()?;
        let (rel, owner) = model.relationship_by_id(&id)?.ok_or_else(|| {
            Error::invalid_package_structure(format!("no package relationship with id {id:?}"))
        })?;
        match req.representation {
            R::Metadata => {
                let resolved = match &rel.resolved {
                    Some(r) => format!("\"{}\"", json_escape(r)),
                    None => "null".to_string(),
                };
                let owner_json = match owner {
                    Some(o) => o.to_string(),
                    None => "null".to_string(),
                };
                let json = format!(
                    concat!(
                        "{{\"id\":\"{}\",\"type\":\"{}\",\"target\":\"{}\",",
                        "\"target_mode\":\"{}\",\"resolved\":{},\"owner\":{}}}"
                    ),
                    json_escape(&rel.id),
                    json_escape(&rel.rel_type),
                    json_escape(&rel.target),
                    rel.mode.name(),
                    resolved,
                    owner_json
                );
                Ok(FieldAnswer {
                    value: AnswerValue::Json(json),
                    basis: Basis::DeterministicallyDerived,
                    selector: req.selector.canonical(),
                    representation: req.representation.name().to_string(),
                    source_span: None,
                    provenance: String::new(),
                    dependency_ids: Vec::new(),
                    integrity_scope: IntegrityScope::None,
                    exact: false,
                })
            }
            R::ExactBytes | R::DecodedBytes => {
                let resolved = rel.resolved.clone().ok_or_else(|| {
                    Error::invalid_package_structure(format!(
                        "relationship {id:?} is external: its target is an inert identifier, never fetched"
                    ))
                })?;
                let part = model.part_by_name(&resolved).ok_or_else(|| {
                    Error::invalid_package_structure(format!(
                        "relationship {id:?} target {resolved:?} is not a package part"
                    ))
                })?;
                let ordinal = part.ordinal;
                if req.representation == R::ExactBytes {
                    self.indexed_exact(
                        req,
                        SelectorKey::new(SEL_PACKAGE_MEMBER_RAW, ordinal),
                        "relationship target part",
                    )
                } else {
                    self.member_decoded(req, ordinal)
                }
            }
            _ => Err(Error::unsupported_feature(format!(
                "unsupported observation: selector {} with representation {}",
                req.selector.canonical(),
                req.representation.name()
            ))),
        }
    }

    /// Non-OPC builds keep the selector surface stable but fail closed.
    #[cfg(not(feature = "opc"))]
    fn package_part_opc(&mut self, _req: &ObserveRequest) -> Result<FieldAnswer> {
        Err(Error::unsupported_feature(
            "OPC support is not compiled in (feature `opc`)",
        ))
    }

    /// Non-OPC builds keep the selector surface stable but fail closed.
    #[cfg(not(feature = "opc"))]
    fn relationship_opc(&mut self, _req: &ObserveRequest) -> Result<FieldAnswer> {
        Err(Error::unsupported_feature(
            "OPC support is not compiled in (feature `opc`)",
        ))
    }

    fn stream_operators(&mut self, req: &ObserveRequest, object: u32) -> Result<FieldAnswer> {
        let entry = self.require_entry(SelectorKey::new(SEL_STREAM, object), "stream")?;
        let decoded = self.decoded_node(object, &entry.node_id)?;
        let decoded_id = decoded.content_id();
        let node = SeedNode::new(
            NodeKind::ContentOperators,
            0,
            Vec::new(),
            vec![decoded_id],
            "pdf:content-operators",
        );
        let id = node.content_id();
        let bytes = self.materialize(&node)?;
        Ok(FieldAnswer {
            value: AnswerValue::Bytes(bytes),
            basis: Basis::DeterministicallyDerived,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: None,
            provenance: String::new(),
            dependency_ids: vec![id, decoded_id],
            integrity_scope: IntegrityScope::None,
            exact: false,
        })
    }

    fn page_content_id(&mut self, what_number: u32) -> Result<NodeId> {
        Ok(self
            .require_entry(SelectorKey::new(SEL_PAGE, what_number), "page")?
            .node_id)
    }

    /// Ensure the page's derived chain exists, deepening once if needed.
    ///
    /// Idempotent by content id: the three derived nodes have deterministic ids,
    /// so this **computes** them and does an O(1) `contains_node` check per id —
    /// it never enumerates the seed store. If they are already present the
    /// promotion is skipped entirely and no new field id is needed, which is what
    /// makes a repeated observation of the same page cheap.
    fn ensure_page_derived(
        &mut self,
        page: u32,
        page_content: NodeId,
    ) -> Result<(SeedNode, SeedNode, SeedNode)> {
        let (ops, text, preview) = derived_nodes(page, page_content);
        let present = self.seeds.contains_node(&ops.content_id())?
            && self.seeds.contains_node(&text.content_id())?
            && self.seeds.contains_node(&preview.content_id())?;
        if !present {
            // Promote against the manifest we already hold, so the descriptor
            // blob is not re-read just to learn the current manifest (fix #2).
            let promoted = ingest::deepen_page_with_manifest(self.store, self.manifest, page)?;
            self.stats.deepened = true;
            self.current_id = promoted;
        }
        Ok((ops, text, preview))
    }

    fn page_text(&mut self, req: &ObserveRequest, page: u32) -> Result<FieldAnswer> {
        let pc = self.page_content_id(page)?;
        let (ops, text, _preview) = self.ensure_page_derived(page, pc)?;
        let text_id = text.content_id();
        let bytes = self.materialize(&text)?;
        let value = AnswerValue::Text(String::from_utf8_lossy(&bytes).into_owned());
        Ok(FieldAnswer {
            value,
            basis: Basis::Heuristic,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: None,
            provenance: String::new(),
            dependency_ids: vec![text_id, ops.content_id(), pc],
            integrity_scope: IntegrityScope::None,
            exact: false,
        })
    }

    fn page_preview(&mut self, req: &ObserveRequest, page: u32) -> Result<FieldAnswer> {
        let pc = self.page_content_id(page)?;
        let (_ops, _text, preview) = self.ensure_page_derived(page, pc)?;
        let preview_id = preview.content_id();
        let bytes = self.materialize(&preview)?;
        Ok(FieldAnswer {
            value: AnswerValue::Bytes(bytes),
            basis: Basis::Heuristic,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: None,
            provenance: String::new(),
            dependency_ids: vec![preview_id, pc],
            integrity_scope: IntegrityScope::None,
            exact: false,
        })
    }

    /// Selective late materialization: read only the page's content streams and
    /// preview node. Image/XObject bytes and the full document are never touched.
    fn page_structure(&mut self, req: &ObserveRequest, page: u32) -> Result<FieldAnswer> {
        let pc = self.page_content_id(page)?;
        let (_ops, _text, preview) = self.ensure_page_derived(page, pc)?;
        let preview_id = preview.content_id();
        let preview_bytes = self.materialize(&preview)?;
        let (text_bytes, draw_ops, path_ops) = preview_stats(&preview_bytes);

        // The page's content streams are the direct dependencies of its
        // `PageContent` node; their `u32` params are the content object numbers.
        let pc_node = self.load(&pc)?;
        let mut streams: Vec<u32> = Vec::new();
        for dep in &pc_node.deps {
            let dep_node = self.load(dep)?;
            // Only a stream node's params are a content object number. An edited
            // page's `PageContent` may depend on a raw `Literal` whose params *are*
            // the content bytes, which must never be read as an object number.
            if !matches!(
                dep_node.kind,
                NodeKind::PdfStreamDecoded | NodeKind::PdfStreamEncoded
            ) {
                continue;
            }
            if let Ok(object) = read_u32_params(&dep_node.params) {
                streams.push(object);
            }
        }
        let streams_json = streams
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");

        let json = format!(
            "{{\"page\":{page},\"text_bytes\":{text_bytes},\"draw_ops\":{draw_ops},\"path_ops\":{path_ops},\"content_streams\":[{streams_json}]}}"
        );
        Ok(FieldAnswer {
            value: AnswerValue::Json(json),
            basis: Basis::DeterministicallyDerived,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: None,
            provenance: String::new(),
            dependency_ids: vec![preview_id, pc],
            integrity_scope: IntegrityScope::None,
            exact: false,
        })
    }

    fn text_match(&mut self, req: &ObserveRequest, pattern: &str) -> Result<FieldAnswer> {
        let mut items: Vec<(u32, String)> = Vec::new();
        let mut estimated: u64 = 0;
        let mut page: u32 = 1;
        while page <= MAX_TEXTMATCH_PAGES {
            let entries = self.lookup(SelectorKey::new(SEL_PAGE, page))?;
            let Some(entry) = entries.into_iter().next() else {
                break;
            };
            let pc = entry.node_id;
            let (_ops, text, _preview) = self.ensure_page_derived(page, pc)?;
            let bytes = self.materialize(&text)?;
            let rendered = String::from_utf8_lossy(&bytes).into_owned();
            for line in rendered.split('\n') {
                if line.contains(pattern) {
                    estimated = estimated.saturating_add(line.len() as u64 + 32);
                    if estimated > req.budget.max_output_bytes {
                        return Err(Error::resource_limit(format!(
                            "text match exceeded the {}-byte budget",
                            req.budget.max_output_bytes
                        )));
                    }
                    items.push((page, line.to_string()));
                }
            }
            page += 1;
        }
        let body = items
            .iter()
            .map(|(p, line)| format!("{{\"page\":{p},\"line\":\"{}\"}}", json_escape(line)))
            .collect::<Vec<_>>()
            .join(",");
        Ok(FieldAnswer {
            value: AnswerValue::Json(format!("[{body}]")),
            basis: Basis::Heuristic,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: None,
            provenance: String::new(),
            dependency_ids: Vec::new(),
            integrity_scope: IntegrityScope::None,
            exact: false,
        })
    }

    // -- DOCX (Phase 12.4) --------------------------------------------------

    /// Materialize and decode the DOCX discovery model (derived, `Q_gen`).
    #[cfg(feature = "docx")]
    fn docx_model(&mut self) -> Result<DocxModel> {
        let entry = self.require_entry(SelectorKey::new(SEL_DOCX_MODEL, 0), "DOCX model")?;
        let node = self.load(&entry.node_id)?;
        let bytes = self.materialize(&node)?;
        DocxModel::decode(&bytes)
    }

    /// Resolve one story to its parsed [`StoryModel`], parsing **only** that
    /// story's part (plus the shared styles part) and persisting the canonical
    /// result in the derived cache. A story is never silently mixed with another.
    #[cfg(feature = "docx")]
    fn docx_story_view(
        &mut self,
        story: DocxStory,
        profile: &DocxExtractProfile,
    ) -> Result<DocxStoryView> {
        if story.kind_index().is_none() {
            return Err(Error::unsupported_feature(format!(
                "DOCX story {} is declared but not part-backed; preserved exactly, not interpreted",
                story.name()
            )));
        }
        let model = self.docx_model()?;
        let part = model.story_part(story).cloned().ok_or_else(|| {
            Error::unsupported_feature(format!(
                "DOCX package has no part for story {}",
                story.name()
            ))
        })?;
        let dec = self.require_entry(
            SelectorKey::new(SEL_PACKAGE_MEMBER_DECODED, part.ordinal),
            "DOCX story part decoded bytes",
        )?;
        let mut deps = vec![dec.node_id];
        if let Some(styles) = &model.styles
            && let Ok(e) = self.require_entry(
                SelectorKey::new(SEL_PACKAGE_MEMBER_DECODED, styles.ordinal),
                "DOCX styles decoded bytes",
            )
        {
            deps.push(e.node_id);
        }
        let span = self
            .lookup(SelectorKey::new(SEL_PACKAGE_MEMBER_RAW, part.ordinal))?
            .into_iter()
            .next()
            .map(|e| (e.out_off, e.out_off.saturating_add(e.out_len)));
        let mut node = SeedNode::new(
            NodeKind::DocxStory,
            self.limits.max_output_bytes,
            story_params(story, &part.name, profile),
            deps.clone(),
            "docx:story",
        );
        node.limits.max_output_bytes = self.limits.max_output_bytes;
        let id = node.content_id();
        let bytes = self.materialize(&node)?;
        let sm = StoryModel::decode(&bytes)?;
        let mut ids = vec![id];
        ids.extend(deps);
        Ok(DocxStoryView {
            model: sm,
            part,
            deps: ids,
            span,
        })
    }

    #[cfg(feature = "docx")]
    fn docx_answer(
        &self,
        req: &ObserveRequest,
        value: AnswerValue,
        provenance: String,
        span: Option<(u64, u64)>,
        deps: Vec<NodeId>,
    ) -> FieldAnswer {
        FieldAnswer {
            value,
            basis: Basis::DeterministicallyDerived,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: span,
            provenance,
            dependency_ids: deps,
            integrity_scope: IntegrityScope::None,
            exact: false,
        }
    }

    #[cfg(feature = "docx")]
    fn docx_story_text(
        &mut self,
        req: &ObserveRequest,
        story: DocxStory,
        profile: &DocxExtractProfile,
    ) -> Result<FieldAnswer> {
        let v = self.docx_story_view(story, profile)?;
        let text = v.model.text();
        let provenance = format!(
            "docx;story={};part={};profile={}",
            story.name(),
            v.part.name,
            profile.fingerprint()
        );
        Ok(self.docx_answer(req, AnswerValue::Text(text), provenance, v.span, v.deps))
    }

    #[cfg(feature = "docx")]
    fn docx_story_metadata(
        &mut self,
        req: &ObserveRequest,
        story: DocxStory,
        profile: &DocxExtractProfile,
    ) -> Result<FieldAnswer> {
        let v = self.docx_story_view(story, profile)?;
        let json = format!(
            concat!(
                "{{\"story\":\"{}\",\"part\":\"{}\",\"ordinal\":{},\"root\":\"{}\",",
                "\"paragraphs\":{},\"tables\":{},\"hyperlinks\":{},\"bookmarks\":{},",
                "\"resources\":{},\"sections\":{},\"profile\":\"{}\"}}"
            ),
            json_escape(&story.name()),
            json_escape(&v.part.name),
            v.part.ordinal,
            json_escape(&v.model.root_local),
            v.model.paragraphs().count(),
            v.model.tables().count(),
            v.model.hyperlinks.len(),
            v.model.bookmarks.len(),
            v.model.resources.len(),
            v.model.section_count,
            profile.fingerprint(),
        );
        let provenance = format!("docx;story={};part={}", story.name(), v.part.name);
        Ok(self.docx_answer(req, AnswerValue::Json(json), provenance, v.span, v.deps))
    }

    #[cfg(feature = "docx")]
    fn docx_story_structure(
        &mut self,
        req: &ObserveRequest,
        story: DocxStory,
        profile: &DocxExtractProfile,
    ) -> Result<FieldAnswer> {
        let v = self.docx_story_view(story, profile)?;
        let paras = v
            .model
            .paragraphs()
            .map(|p| {
                format!(
                    "{{\"index\":{},\"heading\":{},\"style\":{},\"text_len\":{}}}",
                    p.index,
                    opt_u8_json(p.heading_level),
                    opt_str_json(p.style_id.as_deref()),
                    p.text.len()
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let tables = v
            .model
            .tables()
            .map(|t| {
                format!(
                    "{{\"index\":{},\"rows\":{},\"cols_row0\":{}}}",
                    t.index,
                    t.rows.len(),
                    t.rows.first().map_or(0, |r| r.cells.len())
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let json = format!(
            concat!(
                "{{\"story\":\"{}\",\"part\":\"{}\",\"blocks\":{},",
                "\"paragraphs\":[{}],\"tables\":[{}],\"profile\":\"{}\"}}"
            ),
            json_escape(&story.name()),
            json_escape(&v.part.name),
            v.model.blocks.len(),
            paras,
            tables,
            profile.fingerprint(),
        );
        Ok(self.docx_answer(
            req,
            AnswerValue::Json(json),
            format!("docx;story={};part={}", story.name(), v.part.name),
            v.span,
            v.deps,
        ))
    }

    #[cfg(feature = "docx")]
    fn docx_paragraph(
        &mut self,
        req: &ObserveRequest,
        story: DocxStory,
        index: u32,
        profile: &DocxExtractProfile,
    ) -> Result<FieldAnswer> {
        let v = self.docx_story_view(story, profile)?;
        let p = v
            .model
            .paragraphs()
            .find(|p| p.index == index)
            .ok_or_else(|| {
                Error::unsupported_feature(format!(
                    "DOCX story {} has no body paragraph {index}",
                    story.name()
                ))
            })?;
        let text = p.text.clone();
        let style = p.style_id.clone();
        let heading = p.heading_level;
        let run_count = p.runs.len();
        let provenance = format!(
            "docx;story={};part={};paragraph={};profile={}",
            story.name(),
            v.part.name,
            index,
            profile.fingerprint()
        );
        let value = match req.representation {
            Representation::Text => AnswerValue::Text(text),
            Representation::Metadata => AnswerValue::Json(format!(
                concat!(
                    "{{\"story\":\"{}\",\"part\":\"{}\",\"paragraph\":{},",
                    "\"style\":{},\"heading\":{},\"runs\":{},\"text_len\":{}}}"
                ),
                json_escape(&story.name()),
                json_escape(&v.part.name),
                index,
                opt_str_json(style.as_deref()),
                opt_u8_json(heading),
                run_count,
                text.len(),
            )),
            _ => {
                return Err(Error::unsupported_feature(format!(
                    "unsupported observation: selector {} with representation {}",
                    req.selector.canonical(),
                    req.representation.name()
                )));
            }
        };
        Ok(self.docx_answer(req, value, provenance, v.span, v.deps))
    }

    #[cfg(feature = "docx")]
    fn docx_table(
        &mut self,
        req: &ObserveRequest,
        story: DocxStory,
        index: u32,
        profile: &DocxExtractProfile,
    ) -> Result<FieldAnswer> {
        let v = self.docx_story_view(story, profile)?;
        let t = v.model.tables().find(|t| t.index == index).ok_or_else(|| {
            Error::unsupported_feature(format!("DOCX story {} has no table {index}", story.name()))
        })?;
        let text = t.text();
        let rows = t.rows.len();
        let cells: Vec<usize> = t.rows.iter().map(|r| r.cells.len()).collect();
        let provenance = format!(
            "docx;story={};part={};table={};profile={}",
            story.name(),
            v.part.name,
            index,
            profile.fingerprint()
        );
        let value = match req.representation {
            Representation::Text => AnswerValue::Text(text),
            Representation::Metadata => {
                let dims = cells
                    .iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                AnswerValue::Json(format!(
                    concat!(
                        "{{\"story\":\"{}\",\"part\":\"{}\",\"table\":{},",
                        "\"rows\":{},\"cells_per_row\":[{}],\"profile\":\"{}\"}}"
                    ),
                    json_escape(&story.name()),
                    json_escape(&v.part.name),
                    index,
                    rows,
                    dims,
                    profile.fingerprint(),
                ))
            }
            _ => {
                return Err(Error::unsupported_feature(format!(
                    "unsupported observation: selector {} with representation {}",
                    req.selector.canonical(),
                    req.representation.name()
                )));
            }
        };
        Ok(self.docx_answer(req, value, provenance, v.span, v.deps))
    }

    #[cfg(feature = "docx")]
    fn docx_cell(
        &mut self,
        req: &ObserveRequest,
        story: DocxStory,
        table: u32,
        cell: &str,
        profile: &DocxExtractProfile,
    ) -> Result<FieldAnswer> {
        let (col, row_idx) = parse_cell_ref(cell).ok_or_else(|| {
            Error::usage(format!("cell reference {cell:?} is not A1-style (e.g. B7)"))
        })?;
        let v = self.docx_story_view(story, profile)?;
        let t = v.model.tables().find(|t| t.index == table).ok_or_else(|| {
            Error::unsupported_feature(format!("DOCX story {} has no table {table}", story.name()))
        })?;
        let r = t.rows.get(row_idx as usize).ok_or_else(|| {
            Error::unsupported_feature(format!("DOCX table {table} has no row {}", row_idx + 1))
        })?;
        let found = r
            .cells
            .iter()
            .find(|c| col >= c.grid_col && col < c.grid_col.saturating_add(c.grid_span))
            .ok_or_else(|| {
                Error::unsupported_feature(format!(
                    "DOCX table {table} row {} has no cell {cell}",
                    row_idx + 1
                ))
            })?;
        let text = found.text.clone();
        let grid_col = found.grid_col;
        let grid_span = found.grid_span;
        let vmerge = found.vmerge_continue;
        let provenance = format!(
            "docx;story={};part={};table={};row={};cell={};profile={}",
            story.name(),
            v.part.name,
            table,
            row_idx + 1,
            cell,
            profile.fingerprint()
        );
        let value = match req.representation {
            Representation::Text => AnswerValue::Text(text),
            Representation::Metadata => AnswerValue::Json(format!(
                concat!(
                    "{{\"story\":\"{}\",\"part\":\"{}\",\"table\":{},",
                    "\"row\":{},\"cell\":\"{}\",\"grid_col\":{},\"grid_span\":{},",
                    "\"vmerge_continue\":{},\"text_len\":{},\"profile\":\"{}\"}}"
                ),
                json_escape(&story.name()),
                json_escape(&v.part.name),
                table,
                row_idx + 1,
                json_escape(cell),
                grid_col,
                grid_span,
                vmerge,
                text.len(),
                profile.fingerprint(),
            )),
            _ => {
                return Err(Error::unsupported_feature(format!(
                    "unsupported observation: selector {} with representation {}",
                    req.selector.canonical(),
                    req.representation.name()
                )));
            }
        };
        Ok(self.docx_answer(req, value, provenance, v.span, v.deps))
    }

    #[cfg(feature = "docx")]
    fn docx_find(
        &mut self,
        req: &ObserveRequest,
        story: DocxStory,
        pattern: &str,
        profile: &DocxExtractProfile,
    ) -> Result<FieldAnswer> {
        let v = self.docx_story_view(story, profile)?;
        let mut items: Vec<String> = Vec::new();
        let mut estimated: u64 = 0;
        for p in v.model.paragraphs() {
            if p.text.contains(pattern) {
                estimated = estimated.saturating_add(p.text.len() as u64 + 48);
                if estimated > req.budget.max_output_bytes {
                    return Err(Error::resource_limit(format!(
                        "DOCX find exceeded the {}-byte budget",
                        req.budget.max_output_bytes
                    )));
                }
                items.push(format!(
                    "{{\"paragraph\":{},\"text\":\"{}\"}}",
                    p.index,
                    json_escape(&p.text)
                ));
            }
        }
        let provenance = format!(
            "docx;story={};part={};profile={}",
            story.name(),
            v.part.name,
            profile.fingerprint()
        );
        Ok(self.docx_answer(
            req,
            AnswerValue::Json(format!("[{}]", items.join(","))),
            provenance,
            v.span,
            v.deps,
        ))
    }

    /// Resolve the `PdfStreamDecoded` node for `object`.
    ///
    /// A decoded node is a deterministic function of its encoded node, the
    /// materializer, and the decoded length, so it is registered under
    /// [`SEL_STREAM_DECODED`] at ingest and found here in `O(depth)` index reads.
    /// Only when that entry is absent (ingest declined the eager decode) do we
    /// fall back to [`Ctx::deepen_stream`], which recomputes just this one node.
    /// Either path never enumerates the seed store (fix #3).
    fn decoded_node(&mut self, object: u32, encoded_id: &NodeId) -> Result<SeedNode> {
        let entries = self.lookup(SelectorKey::new(SEL_STREAM_DECODED, object))?;
        if let Some(entry) = entries.into_iter().next() {
            return self.load(&entry.node_id);
        }
        self.deepen_stream(object, encoded_id)
    }

    /// Persist a decoded-stream node for `object` if it is recoverable, then
    /// return it. A stream with no exact decoded representation is unsupported.
    fn deepen_stream(&mut self, object: u32, encoded_id: &NodeId) -> Result<SeedNode> {
        let encoded_node = self.load(encoded_id)?;
        let encoded = self.materialize(&encoded_node)?;
        let cap = usize::try_from(self.limits.max_output_bytes).unwrap_or(usize::MAX);
        let decoded = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&encoded, cap)
            .map_err(|e| {
                Error::unsupported_feature(format!(
                    "stream {object} has no recovered decoded representation: {:?}",
                    e.status
                ))
            })?;
        let node = SeedNode::new(
            NodeKind::PdfStreamDecoded,
            decoded.len() as u64,
            u32_params(object),
            vec![*encoded_id],
            "pdf:stream-decoded",
        );
        self.store.seeds_mut().put_node(&node.encode_canonical())?;
        self.stats.deepened = true;
        Ok(node)
    }
}

/// The deterministic Stage-C chain beneath a page's `PageContent` node. Must
/// match `ingest::derived_chain` byte-for-byte so node ids coincide.
pub(crate) fn derived_nodes(page: u32, page_content: NodeId) -> (SeedNode, SeedNode, SeedNode) {
    let ops = SeedNode::new(
        NodeKind::ContentOperators,
        0,
        u32_params(page),
        vec![page_content],
        "pdf:content-operators",
    );
    let text = SeedNode::new(
        NodeKind::TextRuns,
        0,
        Vec::new(),
        vec![ops.content_id()],
        "pdf:text-runs",
    );
    let preview = SeedNode::new(
        NodeKind::PagePreview,
        0,
        u32_params(page),
        vec![page_content],
        "pdf:page-preview",
    );
    (ops, text, preview)
}

/// Parse the numeric fields of a `PagePreview` header.
fn preview_stats(bytes: &[u8]) -> (u64, u64, u64) {
    let rendered = String::from_utf8_lossy(bytes);
    let mut text_bytes = 0u64;
    let mut draw_ops = 0u64;
    let mut path_ops = 0u64;
    for line in rendered.lines() {
        if let Some(v) = line.strip_prefix("text-bytes ") {
            text_bytes = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("draw-ops ") {
            draw_ops = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("path-ops ") {
            path_ops = v.trim().parse().unwrap_or(0);
        }
    }
    (text_bytes, draw_ops, path_ops)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::{Descriptor, ObjectSource};
    use crate::dra::{Op, Program};
    use crate::field::plan;
    use crate::store::FsSeedStore;
    use std::fs;
    use std::path::PathBuf;

    fn temp_root(label: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "vole-observe-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        p
    }

    fn opaque_descriptor(source: &[u8]) -> Vec<u8> {
        let d = Descriptor {
            universe: crate::container::UNIVERSE.to_string(),
            source_format: crate::SOURCE_FORMAT_PDF,
            format_basis: "pdf:observe-test".to_string(),
            models: vec![],
            channels: vec![],
            objects: vec![ObjectSource::Inline(source.to_vec())],
            program: Program::new(vec![Op::EmitObject { object_id: 0 }]),
            observation_index: None,
            seek_directory: false,
            source_sha256: crate::integrity::sha256(source),
            source_len: source.len() as u64,
        };
        d.serialize().unwrap().0
    }

    fn adler32(data: &[u8]) -> u32 {
        let mut a: u32 = 1;
        let mut b: u32 = 0;
        for &byte in data {
            a = (a + u32::from(byte)) % 65521;
            b = (b + a) % 65521;
        }
        (b << 16) | a
    }

    fn zlib_stored(data: &[u8]) -> Vec<u8> {
        assert!(!data.is_empty());
        let mut out = vec![0x78, 0x01];
        let chunks: Vec<&[u8]> = data.chunks(0xFFFF).collect();
        for (i, chunk) in chunks.iter().enumerate() {
            let final_block = u8::from(i + 1 == chunks.len());
            out.push(final_block);
            let len = chunk.len() as u16;
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&(!len).to_le_bytes());
            out.extend_from_slice(chunk);
        }
        out.extend_from_slice(&adler32(data).to_be_bytes());
        out
    }

    struct PdfBuilder {
        buf: Vec<u8>,
        offsets: Vec<(u64, u64)>,
    }

    impl PdfBuilder {
        fn new() -> Self {
            PdfBuilder {
                buf: Vec::new(),
                offsets: Vec::new(),
            }
        }
        fn text(&mut self, s: &str) {
            self.buf.extend_from_slice(s.as_bytes());
        }
        fn raw(&mut self, b: &[u8]) {
            self.buf.extend_from_slice(b);
        }
        fn obj(&mut self, number: u64, body: &[u8]) {
            self.offsets.push((number, self.buf.len() as u64));
            self.text(&format!("{number} 0 obj\n"));
            self.raw(body);
            self.text("\nendobj\n");
        }
        fn stream_obj(&mut self, number: u64, extra: &str, data: &[u8]) {
            self.offsets.push((number, self.buf.len() as u64));
            self.text(&format!(
                "{number} 0 obj\n<< /Length {}{extra} >>\nstream\n",
                data.len()
            ));
            self.raw(data);
            self.text("\nendstream\nendobj\n");
        }
        fn offset_of(&self, number: u64) -> u64 {
            self.offsets
                .iter()
                .find(|&&(n, _)| n == number)
                .map(|&(_, off)| off)
                .unwrap()
        }
        fn classic_trailer(&mut self, size: u64, extra: &str) {
            let xref = self.buf.len() as u64;
            self.text(&format!("xref\n0 {size}\n"));
            self.raw(b"0000000000 65535 f \n");
            for number in 1..size {
                let off = self.offset_of(number);
                self.text(&format!("{off:010} 00000 n \n"));
            }
            self.text(&format!(
                "trailer\n<< /Size {size}{extra} >>\nstartxref\n{xref}\n%%EOF\n"
            ));
        }
    }

    /// A classic-xref PDF with one page, a lone-Flate content stream containing
    /// `(Hello) Tj`, and (optionally) a large image XObject.
    fn fixture_pdf(with_image: bool) -> Vec<u8> {
        let content = b"BT /F1 12 Tf 72 720 Td (Hello) Tj ET\n";
        let encoded = zlib_stored(content);
        let mut w = PdfBuilder::new();
        w.text("%PDF-1.5\n");
        w.obj(1, b"<< /Type /Catalog /Pages 2 0 R >>");
        w.obj(2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
        if with_image {
            w.obj(
                3,
                b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> /XObject << /Im0 6 0 R >> >> /Contents 4 0 R >>",
            );
        } else {
            w.obj(
                3,
                b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
            );
        }
        w.stream_obj(4, " /Filter /FlateDecode", &encoded);
        w.obj(5, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
        if with_image {
            let image = vec![0x80u8; 256 * 256];
            let image_encoded = zlib_stored(&image);
            w.stream_obj(
                6,
                " /Type /XObject /Subtype /Image /Width 256 /Height 256 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode",
                &image_encoded,
            );
            w.classic_trailer(7, " /Root 1 0 R");
        } else {
            w.classic_trailer(6, " /Root 1 0 R");
        }
        w.buf
    }

    struct Fixture {
        root: PathBuf,
        store: FieldStore,
        field: FieldId,
        source: Vec<u8>,
    }

    impl Fixture {
        fn new(label: &str, with_image: bool) -> Fixture {
            let root = temp_root(label);
            let mut store = FieldStore::open(&root).unwrap();
            let source = fixture_pdf(with_image);
            let descriptor = opaque_descriptor(&source);
            let report = ingest::ingest_pdf(&mut store, &descriptor, Limits::DEFAULT).unwrap();
            Fixture {
                root,
                store,
                field: report.field,
                source,
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).ok();
        }
    }

    fn observe_req(
        fx: &mut Fixture,
        selector: Selector,
        representation: Representation,
    ) -> (FieldAnswer, ObserveStats) {
        let req = ObserveRequest::new(selector, representation);
        let (answer, stats, _field) =
            observe(&mut fx.store, &fx.field, &req, Limits::DEFAULT).unwrap();
        (answer, stats)
    }

    #[test]
    fn document_full_document_is_exact() {
        let mut fx = Fixture::new("full", false);
        let (answer, stats) =
            observe_req(&mut fx, Selector::Document, Representation::FullDocument);
        assert_eq!(answer.value, AnswerValue::Bytes(fx.source.clone()));
        assert_eq!(answer.basis, Basis::DirectlyObserved);
        assert_eq!(answer.integrity_scope, IntegrityScope::WholeSource);
        assert!(answer.exact);
        assert_eq!(answer.source_span, Some((0, fx.source.len() as u64)));
        assert_eq!(stats.bytes_returned, fx.source.len() as u64);
    }

    #[test]
    fn byte_range_returns_exact_bytes() {
        let mut fx = Fixture::new("range", false);
        let (answer, _) = observe_req(
            &mut fx,
            Selector::ByteRange { offset: 9, len: 8 },
            Representation::ExactBytes,
        );
        assert_eq!(answer.value, AnswerValue::Bytes(fx.source[9..17].to_vec()));
        assert_eq!(answer.source_span, Some((9, 17)));
        assert!(answer.exact);
        assert_eq!(answer.integrity_scope, IntegrityScope::Node);
    }

    #[test]
    fn page_text_is_heuristic_and_nonempty() {
        let mut fx = Fixture::new("text", false);
        let (answer, _) = observe_req(&mut fx, Selector::Page(1), Representation::Text);
        match &answer.value {
            AnswerValue::Text(t) => assert!(t.contains("Hello"), "got {t:?}"),
            other => panic!("expected text, got {other:?}"),
        }
        assert_eq!(answer.basis, Basis::Heuristic);
        assert!(!answer.exact);
    }

    /// The *procedural/seed closure* of a page-structure observation excludes the
    /// image seed node. This is a statement about **seed reads only**: the
    /// descriptor blob is charged separately in `descriptor_bytes_read`, and
    /// `bytes_read` is the sum of all four classes, so this must never be read as
    /// "the OS avoided reading the whole document" (review fix #1/#2).
    #[test]
    fn page_structure_procedural_closure_excludes_image_seed_node() {
        let mut fx = Fixture::new("structure", true);
        let (answer, stats) = observe_req(&mut fx, Selector::Page(1), Representation::Structure);
        match &answer.value {
            AnswerValue::Json(j) => {
                assert!(j.contains("\"page\":1"), "got {j}");
                assert!(j.contains("\"content_streams\":[4]"), "got {j}");
            }
            other => panic!("expected json, got {other:?}"),
        }
        assert_eq!(answer.basis, Basis::DeterministicallyDerived);
        assert!(!answer.exact);
        // Seed closure: the 64 KiB image payload's seed node is never fetched.
        assert!(
            stats.seed_bytes_read < 16 * 1024,
            "structure observation read {} seed bytes (expected < 16384)",
            stats.seed_bytes_read
        );
        // The honest total *does* include the descriptor blob, which a narrow
        // observation necessarily read to open the field.
        assert!(
            stats.descriptor_bytes_read > 0,
            "the descriptor read must be accounted, not hidden"
        );
        assert!(
            stats.bytes_read >= stats.descriptor_bytes_read,
            "bytes_read {} must include descriptor_bytes_read {}",
            stats.bytes_read,
            stats.descriptor_bytes_read
        );
        assert_eq!(
            stats.bytes_read,
            stats
                .descriptor_bytes_read
                .saturating_add(stats.manifest_bytes_read)
                .saturating_add(stats.index_bytes_read)
                .saturating_add(stats.seed_bytes_read),
            "bytes_read must be the exact sum of the four physical classes"
        );
    }

    #[test]
    fn plan_is_pure_and_deterministic() {
        let fx = Fixture::new("plan", false);
        let req = ObserveRequest::new(Selector::Page(1), Representation::Text);
        let field = Field::open(&fx.store, &fx.field, Limits::DEFAULT).unwrap();
        let before = fx.store.seeds().list_nodes().unwrap().len();
        let a = plan::plan(field.manifest(), &fx.store, &req).unwrap();
        let b = plan::plan(field.manifest(), &fx.store, &req).unwrap();
        assert_eq!(a, b);
        let after = fx.store.seeds().list_nodes().unwrap().len();
        assert_eq!(before, after, "plan must not add seed nodes");
    }

    #[test]
    fn explain_json_keys_and_unsupported_pair() {
        use crate::field::explain;
        let mut fx = Fixture::new("explain", false);
        let req = ObserveRequest::new(Selector::Document, Representation::FullDocument);
        let (plan, actual) =
            explain::explain_analyze(&mut fx.store, &fx.field, &req, Limits::DEFAULT).unwrap();
        assert_eq!(
            plan.json,
            "{\"selector\":\"document\",\"representation\":\"full\",\"shape\":\"full_materialize\",\"index_reads\":0,\"required_nodes\":1,\"will_materialize\":[\"DocumentExact\"],\"will_not_materialize\":[]}"
        );
        let actual_json = actual.to_json();
        let mut keys = top_level_keys(&actual_json);
        keys.sort();
        let mut expected = vec![
            "basis",
            "bytes_read",
            "bytes_returned",
            "deepened",
            "descriptor_bytes_read",
            "descriptor_read_mode",
            "exact",
            "index_bytes_read",
            "index_nodes_read",
            "manifest_bytes_read",
            "seed_bytes_read",
            "seed_nodes_fetched",
            "seed_nodes_materialized",
            "wall_micros",
        ];
        expected.sort_unstable();
        assert_eq!(keys, expected, "actual json keys: {actual_json}");

        // An unsupported selector/representation pair is typed, never guessed.
        let bad = ObserveRequest::new(Selector::Document, Representation::Text);
        let err = observe(&mut fx.store, &fx.field, &bad, Limits::DEFAULT).unwrap_err();
        assert_eq!(err.class(), crate::ErrorClass::UnsupportedFeature);
        let field = Field::open(&fx.store, &fx.field, Limits::DEFAULT).unwrap();
        let perr = plan::plan(field.manifest(), &fx.store, &bad).unwrap_err();
        assert_eq!(perr.class(), crate::ErrorClass::UnsupportedFeature);
    }

    #[test]
    fn budget_yields_resource_limit_not_truncation() {
        let mut fx = Fixture::new("budget", false);
        let req = ObserveRequest {
            selector: Selector::Document,
            representation: Representation::FullDocument,
            budget: ObserveBudget {
                max_output_bytes: 4,
                max_nodes: 1 << 20,
            },
            use_cache: true,
        };
        let err = observe(&mut fx.store, &fx.field, &req, Limits::DEFAULT).unwrap_err();
        assert_eq!(err.class(), crate::ErrorClass::ResourceLimit);
    }

    #[test]
    fn find_returns_matching_line() {
        let mut fx = Fixture::new("find", false);
        let (answer, _) = observe_req(
            &mut fx,
            Selector::TextMatch("Hello".to_string()),
            Representation::Text,
        );
        match &answer.value {
            AnswerValue::Json(j) => {
                assert!(j.contains("\"page\":1"), "got {j}");
                assert!(j.contains("Hello"), "got {j}");
            }
            other => panic!("expected json, got {other:?}"),
        }
        assert_eq!(answer.basis, Basis::Heuristic);
    }

    #[test]
    fn preview_is_deterministic() {
        let mut fx = Fixture::new("preview", false);
        let (a, _) = observe_req(&mut fx, Selector::Page(1), Representation::Preview);
        let (b, _) = observe_req(&mut fx, Selector::Page(1), Representation::Preview);
        assert_eq!(a.value, b.value);
        match &a.value {
            AnswerValue::Bytes(bytes) => {
                assert!(String::from_utf8_lossy(bytes).contains("VOLE-PREVIEW v1"))
            }
            other => panic!("expected preview bytes, got {other:?}"),
        }
    }

    #[test]
    fn decoded_and_operators_streams_resolve() {
        let mut fx = Fixture::new("decoded", false);
        let (decoded, _) = observe_req(&mut fx, Selector::Stream(4), Representation::DecodedBytes);
        match &decoded.value {
            AnswerValue::Bytes(b) => {
                assert_eq!(b, b"BT /F1 12 Tf 72 720 Td (Hello) Tj ET\n");
            }
            other => panic!("expected bytes, got {other:?}"),
        }
        assert_eq!(decoded.basis, Basis::DeterministicallyDerived);
        let (ops, _) = observe_req(&mut fx, Selector::Stream(4), Representation::Operators);
        assert!(matches!(ops.value, AnswerValue::Bytes(ref b) if !b.is_empty()));
    }

    /// Every physical byte class is charged, and `bytes_read` is their exact sum.
    #[test]
    fn observation_byte_classes_sum_and_are_all_charged() {
        let mut fx = Fixture::new("io-sum", false);
        let (_, stats) = observe_req(&mut fx, Selector::Page(1), Representation::Structure);
        assert!(
            stats.descriptor_bytes_read > 0,
            "descriptor bytes: {stats:?}"
        );
        assert!(stats.manifest_bytes_read > 0, "manifest bytes: {stats:?}");
        assert!(stats.index_bytes_read > 0, "index bytes: {stats:?}");
        assert!(stats.seed_bytes_read > 0, "seed bytes: {stats:?}");
        assert_eq!(
            stats.bytes_read,
            stats
                .descriptor_bytes_read
                .saturating_add(stats.manifest_bytes_read)
                .saturating_add(stats.index_bytes_read)
                .saturating_add(stats.seed_bytes_read),
            "bytes_read must equal the class sum: {stats:?}"
        );
    }

    /// `explain_analyze` must open the descriptor exactly once: the plan and the
    /// evaluation share one `Field`, and a Stage-C promotion no longer re-reads it
    /// (review fix #2).
    #[test]
    fn explain_analyze_opens_the_descriptor_once() {
        use crate::field::explain;
        let mut fx = Fixture::new("opens-once", false);

        // A metadata observation does no promotion; one open.
        let req = ObserveRequest::new(Selector::Document, Representation::Metadata);
        let before = fx.store.io().descriptor_reads();
        let (_, actual) =
            explain::explain_analyze(&mut fx.store, &fx.field, &req, Limits::DEFAULT).unwrap();
        assert_eq!(
            fx.store.io().descriptor_reads() - before,
            1,
            "explain_analyze must open the descriptor exactly once"
        );
        assert!(
            actual.stats.descriptor_bytes_read > 0,
            "the single open must still be accounted: {:?}",
            actual.stats
        );

        // A cold page observation *does* promote (Stage C); even then the
        // descriptor is read once, because promotion reuses the open manifest.
        let page = ObserveRequest::new(Selector::Page(1), Representation::Text);
        let before = fx.store.io().descriptor_reads();
        let (_, page_actual) =
            explain::explain_analyze(&mut fx.store, &fx.field, &page, Limits::DEFAULT).unwrap();
        assert_eq!(
            fx.store.io().descriptor_reads() - before,
            1,
            "a cold promotion must not re-open the field for its manifest"
        );
        assert!(page_actual.stats.deepened, "the cold page must promote");
        assert!(page_actual.stats.descriptor_bytes_read > 0);
    }

    /// A seed store that forbids enumeration and bounds fetches. Putting it on the
    /// observation path proves a `Stream(n)+DecodedBytes` resolves through the
    /// hierarchical index, never by scanning the store (review fix #3).
    struct BoundedSeedStore {
        inner: FsSeedStore,
        fetches: Cell<u64>,
        limit: u64,
    }

    impl SeedStore for BoundedSeedStore {
        fn put_node(&mut self, canonical: &[u8]) -> Result<NodeId> {
            self.inner.put_node(canonical)
        }

        fn get_node(&self, id: &NodeId) -> Result<Vec<u8>> {
            let n = self.fetches.get() + 1;
            assert!(
                n <= self.limit,
                "get_node #{n} exceeds the {}-fetch bound: the store was enumerated",
                self.limit
            );
            self.fetches.set(n);
            self.inner.get_node(id)
        }

        fn get_node_range(&self, id: &NodeId, offset: u64, len: u64) -> Result<Vec<u8>> {
            let n = self.fetches.get() + 1;
            assert!(
                n <= self.limit,
                "get_node_range #{n} exceeds the {}-fetch bound: the store was enumerated",
                self.limit
            );
            self.fetches.set(n);
            self.inner.get_node_range(id, offset, len)
        }

        fn contains_node(&self, id: &NodeId) -> Result<bool> {
            self.inner.contains_node(id)
        }

        fn list_nodes(&self) -> Result<Vec<(NodeId, u64)>> {
            panic!("an observation must never enumerate the seed store")
        }
    }

    #[test]
    fn decoded_stream_resolves_without_enumerating_the_store() {
        let mut fx = Fixture::new("no-scan", true);
        // Many unrelated decoy nodes: a whole-store scan would fetch them all.
        for i in 0..256u32 {
            let decoy = SeedNode::new(
                NodeKind::PdfObject,
                1,
                u32_params(10_000 + i),
                Vec::new(),
                "decoy",
            );
            fx.store
                .seeds_mut()
                .put_node(&decoy.encode_canonical())
                .unwrap();
        }
        let total = fx.store.seeds().list_nodes().unwrap().len() as u64;
        assert!(total > 200, "expected many seed nodes, got {total}");

        let field = Field::open(&fx.store, &fx.field, Limits::DEFAULT).unwrap();
        let io = fx.store.io().handle();
        let seeds = CountingSeedStore::new(BoundedSeedStore {
            inner: FsSeedStore::open_with_io(fx.store.root(), io.handle()).unwrap(),
            fetches: Cell::new(0),
            limit: 8,
        });
        let istore = FsIndexStore::open_with_io(fx.store.root(), io.handle()).unwrap();
        let req = ObserveRequest::new(Selector::Stream(4), Representation::DecodedBytes);
        let (answer, stats, _) = observe_with_stores(
            &mut fx.store,
            FieldView::from_field(&field),
            &req,
            Limits::DEFAULT,
            Instant::now(),
            seeds,
            istore,
        )
        .unwrap();
        match &answer.value {
            AnswerValue::Bytes(b) => {
                assert_eq!(b, b"BT /F1 12 Tf 72 720 Td (Hello) Tj ET\n")
            }
            other => panic!("expected bytes, got {other:?}"),
        }
        assert!(
            stats.seed_nodes_fetched <= 8,
            "fetched {} seed nodes for one decoded stream",
            stats.seed_nodes_fetched
        );
        assert!(
            stats.seed_nodes_fetched < total,
            "must not enumerate the {total}-node store; fetched {}",
            stats.seed_nodes_fetched
        );
    }

    /// Extract top-level object keys from a flat JSON object (test helper).
    pub(super) fn top_level_keys(json: &str) -> Vec<String> {
        let b = json.as_bytes();
        let mut keys = Vec::new();
        let mut depth = 0i32;
        let mut in_str = false;
        let mut esc = false;
        let mut i = 0;
        while i < b.len() {
            let c = b[i];
            if in_str {
                if esc {
                    esc = false;
                } else if c == b'\\' {
                    esc = true;
                } else if c == b'"' {
                    in_str = false;
                    if depth == 1 && b.get(i + 1) == Some(&b':') {
                        let mut j = i;
                        while j > 0 {
                            j -= 1;
                            if b[j] == b'"' {
                                keys.push(String::from_utf8_lossy(&b[j + 1..i]).into_owned());
                                break;
                            }
                        }
                    }
                }
            } else {
                match c {
                    b'"' => in_str = true,
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => depth -= 1,
                    _ => {}
                }
            }
            i += 1;
        }
        keys
    }
}
