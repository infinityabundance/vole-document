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

use crate::error::{Error, Result};
use crate::field::cache::DerivedCache;
use crate::field::dag::{self, EvalBudget, ReuseStats};
use crate::field::index::{
    FsIndexStore, IndexEntry, SEL_OBJECT, SEL_PAGE, SEL_REVISION, SEL_STREAM, SEL_STREAM_DECODED,
    SelectorKey, lookup,
};
use crate::field::ingest;
use crate::field::node::{NodeKind, SeedNode, read_u32_params, span_params, u32_params};
use crate::field::{Field, FieldId, FieldStore};
use crate::limits::Limits;
use crate::store::{FsSeedStore, NodeId, SeedStore};

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
    /// A half-open exact source byte range.
    ByteRange {
        /// Start offset.
        offset: u64,
        /// Length in bytes.
        len: u64,
    },
    /// Every text line containing a pattern (case-sensitive).
    TextMatch(String),
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
            Selector::ByteRange { offset, len } => format!("byte-range:{offset}:{len}"),
            Selector::TextMatch(p) => format!("text-match:{p}"),
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
    /// Descriptor-blob bytes physically fetched to open this observation's field.
    /// A narrow observation still needs descriptor state, so this is normally the
    /// whole `.voldoc` blob; it is **not** hidden behind `bytes_read` (fix #1/#2).
    pub descriptor_bytes_read: u64,
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
    let field = Field::open(store, id, limits)?;
    observe_inner(store, &field, req, limits, started)
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
    observe_inner(store, field, req, limits, started)
}

/// Build the seed and index sub-stores, sharing the field store's I/O counters.
fn open_sub_stores(store: &FieldStore) -> Result<(CountingSeedStore<FsSeedStore>, FsIndexStore)> {
    let root = store.root();
    let io = store.io();
    let seeds = CountingSeedStore::new(FsSeedStore::open_with_io(root, io.handle())?);
    let istore = FsIndexStore::open_with_io(root, io.handle())?;
    Ok((seeds, istore))
}

fn observe_inner<'a>(
    store: &'a mut FieldStore,
    field: &'a Field,
    req: &ObserveRequest,
    limits: Limits,
    started: Instant,
) -> Result<(FieldAnswer, ObserveStats, FieldId)> {
    let (seeds, istore) = open_sub_stores(store)?;
    observe_with_stores(store, field, req, limits, started, seeds, istore)
}

/// The evaluation core. Takes explicit sub-stores so a test can supply a seed
/// store wrapper that forbids enumeration.
fn observe_with_stores<'a, S: SeedStore>(
    store: &'a mut FieldStore,
    field: &'a Field,
    req: &ObserveRequest,
    limits: Limits,
    started: Instant,
    seeds: CountingSeedStore<S>,
    istore: FsIndexStore,
) -> Result<(FieldAnswer, ObserveStats, FieldId)> {
    // Snapshot after the field is open: only the reads this observation performs
    // during evaluation are counted as deltas; the field-open bytes come from
    // `field.open_io()` below so they cannot be dropped on the floor.
    let io_base = store.io().snapshot();
    let budget = EvalBudget {
        max_nodes: req.budget.max_nodes,
        ..EvalBudget::default()
    };
    let cache = DerivedCache::open(store.root().join("cache"))?;
    let mut ctx = Ctx {
        store,
        field,
        seeds,
        istore,
        limits,
        budget,
        stats: ObserveStats::default(),
        use_cache: req.use_cache,
        cache,
        reuse: ReuseStats::default(),
        current_id: field.id(),
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
    // Every physical byte fetched by this observation: the field-open bytes plus
    // any additional reads (e.g. a Stage-C promotion) performed during dispatch.
    let open = ctx.field.open_io();
    let extra = io_base.delta(&ctx.store.io().snapshot());
    stats.descriptor_bytes_read = open.descriptor_bytes.saturating_add(extra.descriptor_bytes);
    stats.manifest_bytes_read = open.manifest_bytes.saturating_add(extra.manifest_bytes);
    stats.index_bytes_read = extra.index_bytes;
    stats.seed_bytes_read = extra.seed_bytes;
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
    field: &'a Field,
    seeds: CountingSeedStore<S>,
    istore: FsIndexStore,
    limits: Limits,
    budget: EvalBudget,
    stats: ObserveStats,
    use_cache: bool,
    cache: DerivedCache,
    reuse: ReuseStats,
    current_id: FieldId,
}

impl<S: SeedStore> Ctx<'_, S> {
    fn materialize(&mut self, node: &SeedNode) -> Result<Vec<u8>> {
        let depth = node.limits.max_depth;
        if self.use_cache {
            dag::materialize_node_cached(
                self.field.parsed(),
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
            dag::materialize_node_cached(
                self.field.parsed(),
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
        if !self.field.manifest().has_index() {
            return Ok(Vec::new());
        }
        let root = NodeId::from_bytes(self.field.manifest().index_root);
        let entries = lookup(&self.istore, &root, &key)?;
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
            (Selector::Stream(n), R::EncodedBytes) => {
                self.indexed_exact(req, SelectorKey::new(SEL_STREAM, *n), "stream")
            }
            (Selector::Stream(n), R::DecodedBytes) => self.stream_decoded(req, *n),
            (Selector::Stream(n), R::Operators) => self.stream_operators(req, *n),
            (Selector::Page(n), R::Text) => self.page_text(req, *n),
            (Selector::Page(n), R::Preview) => self.page_preview(req, *n),
            (Selector::Page(n), R::Structure) => self.page_structure(req, *n),
            (Selector::TextMatch(p), R::Text) => self.text_match(req, p),
            _ => Err(Error::unsupported_feature(format!(
                "unsupported observation: selector {} with representation {}",
                req.selector.canonical(),
                req.representation.name()
            ))),
        }
    }

    fn document_full(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        let bytes = self.field.materialize_exact(self.limits)?;
        Ok(FieldAnswer {
            value: AnswerValue::Bytes(bytes),
            basis: Basis::DirectlyObserved,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: Some((0, self.field.manifest().source_len)),
            dependency_ids: vec![self.field.manifest().root_node],
            integrity_scope: IntegrityScope::WholeSource,
            exact: true,
        })
    }

    fn document_metadata(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        let d = &self.field.parsed().descriptor;
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
            d.source_len,
            crate::integrity::to_hex(&d.source_sha256),
            d.objects.len(),
            d.program.ops.len(),
            self.field.manifest().node_count,
        );
        Ok(FieldAnswer {
            value: AnswerValue::Json(json),
            basis: Basis::DeterministicallyDerived,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: None,
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
            dependency_ids: vec![id],
            integrity_scope: IntegrityScope::None,
            exact: false,
        })
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
            let promoted =
                ingest::deepen_page_with_manifest(self.store, self.field.manifest(), page)?;
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
            dependency_ids: Vec::new(),
            integrity_scope: IntegrityScope::None,
            exact: false,
        })
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
        let a = plan::plan(&field, &fx.store, &req).unwrap();
        let b = plan::plan(&field, &fx.store, &req).unwrap();
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
        let perr = plan::plan(&field, &fx.store, &bad).unwrap_err();
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
            &field,
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
