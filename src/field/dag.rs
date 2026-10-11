//! The procedural seed DAG: bounded closure traversal and materialization.
//!
//! Every node names a **bounded, versioned materializer**; there is no arbitrary
//! execution (ADR-0025). A materializer is a pure function of the field's exact
//! descriptor, the seed store, and the node's dependency outputs. Unknown kinds
//! or materializer versions fail closed.
//!
//! Exact node kinds (`Q_ref`) resolve to byte-identical spans of the source via
//! the *existing* partial-materialization machinery, so a narrow observation does
//! not force a whole-document reconstruction. Derived node kinds (`Q_gen`, e.g.
//! decoded streams, text runs, previews) are deterministic projections and are
//! always labelled as such.

use crate::container::{ObjectSource, ParsedDescriptor};
use crate::error::{Error, Result};
use crate::limits::Limits;
use crate::materialize::observation::{select_ops, selection_references, serve_selection};
use crate::store::{NodeId, SeedStore};

use super::derive;
use super::node::{NodeKind, SeedNode, read_object_params, read_span_params, read_u32_params};

/// Supplies exact source byte ranges to a seed-DAG materialization.
///
/// The full implementation is a parsed descriptor
/// ([`impl SourceServer for ParsedDescriptor`]); a partial loader that reads only
/// the records a query needs is the other. Every materializer that resolves an
/// exact `Q_ref` node goes through this one method, so the DAG logic cannot
/// diverge between the complete and partial sources.
pub trait SourceServer {
    /// Bytes of the source range `[offset, offset + len)`.
    fn serve_range(&self, offset: u64, len: u64, limits: Limits) -> Result<Vec<u8>>;
    /// The whole reconstructed source (only a `DocumentExact` node needs this).
    fn serve_document(&self, limits: Limits) -> Result<Vec<u8>>;
}

impl SourceServer for ParsedDescriptor {
    fn serve_range(&self, offset: u64, len: u64, limits: Limits) -> Result<Vec<u8>> {
        serve_source_range(self, offset, len, limits)
    }

    fn serve_document(&self, limits: Limits) -> Result<Vec<u8>> {
        crate::materialize::materialize(self, limits)
    }
}

/// Hard cap on the number of nodes one materialization may evaluate.
pub const MAX_EVAL_NODES: u64 = 1 << 20;
/// Hard cap on total intermediate+output bytes one materialization may produce.
pub const MAX_EVAL_BYTES: u64 = 1 << 32;

/// A bounded evaluation budget, shared across a recursive materialization.
#[derive(Debug, Clone)]
pub struct EvalBudget {
    /// Maximum nodes evaluated.
    pub max_nodes: u64,
    /// Maximum total bytes produced (intermediate + final).
    pub max_bytes: u64,
    /// Nodes evaluated so far.
    pub nodes: u64,
    /// Bytes produced so far.
    pub produced: u64,
}

impl Default for EvalBudget {
    fn default() -> Self {
        EvalBudget {
            max_nodes: MAX_EVAL_NODES,
            max_bytes: MAX_EVAL_BYTES,
            nodes: 0,
            produced: 0,
        }
    }
}

impl EvalBudget {
    fn charge_node(&mut self) -> Result<()> {
        self.nodes = self
            .nodes
            .checked_add(1)
            .ok_or_else(|| Error::resource_limit("seed evaluation node count overflow"))?;
        if self.nodes > self.max_nodes {
            return Err(Error::resource_limit(format!(
                "seed evaluation exceeded {} nodes",
                self.max_nodes
            )));
        }
        Ok(())
    }

    pub(crate) fn charge_bytes(&mut self, n: u64) -> Result<()> {
        self.produced = self
            .produced
            .checked_add(n)
            .ok_or_else(|| Error::resource_limit("seed evaluation byte count overflow"))?;
        if self.produced > self.max_bytes {
            return Err(Error::resource_limit(format!(
                "seed evaluation exceeded {} bytes",
                self.max_bytes
            )));
        }
        Ok(())
    }
}

/// Load and decode one node from the store by id, verifying content identity.
pub fn load_node(store: &dyn SeedStore, id: &NodeId) -> Result<SeedNode> {
    let bytes = store.get_node(id)?;
    let node = SeedNode::decode_canonical(&bytes)?;
    if node.content_id() != *id {
        return Err(Error::integrity_mismatch(format!(
            "seed node {id} decoded to a different content id"
        )));
    }
    node.check_limits(&Limits::DEFAULT)?;
    Ok(node)
}

/// The dependency ids of a node's already-decoded canonical bytes.
pub fn deps_of_canonical(bytes: &[u8]) -> Result<Vec<NodeId>> {
    Ok(SeedNode::decode_canonical(bytes)?.deps)
}

/// Serve an exact source byte range `[offset, offset+len)` using the descriptor's
/// program via the shared partial-materialization path. Does **not** materialize
/// the whole document.
fn serve_source_range(
    parsed: &ParsedDescriptor,
    offset: u64,
    len: u64,
    limits: Limits,
) -> Result<Vec<u8>> {
    let d = &parsed.descriptor;
    if d.objects
        .iter()
        .any(|o| matches!(o, ObjectSource::External { .. }))
    {
        return Err(Error::unsupported_feature(
            "field v1 requires an inline descriptor (no external objects)",
        ));
    }
    let end = offset
        .checked_add(len)
        .ok_or_else(|| Error::usage("source slice end overflows"))?;
    if end > d.source_len {
        return Err(Error::usage(format!(
            "source slice {offset}..{end} exceeds source length {}",
            d.source_len
        )));
    }
    let objects: Vec<Vec<u8>> = d
        .objects
        .iter()
        .map(|o| o.as_inline().unwrap_or(&[]).to_vec())
        .collect();
    let object_lens: Vec<u64> = objects.iter().map(|o| o.len() as u64).collect();
    let channel_lens: Vec<u64> = d.channels.iter().map(|c| c.decoded_length).collect();
    let window = select_ops(&d.program, &object_lens, &channel_lens, offset, end, limits)?;
    let (objects_used, channels_used) =
        selection_references(&window.ops, objects.len(), d.channels.len());
    let served = serve_selection(
        &objects,
        &d.channels,
        &d.models,
        window,
        &objects_used,
        &channels_used,
        offset,
        end,
        limits,
    )?;
    Ok(served.bytes)
}

/// Which cache event a [`CacheNote`] reports (Phase 15.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheEffect {
    /// The node was served whole from the cache (its subtree was not traversed).
    Hit,
    /// The node's freshly computed output was written to the cache.
    Stored,
}

/// A non-authoritative, best-effort cache event (Phase 15.6).
///
/// It never affects output bytes and is never consulted by
/// `materialize`/`decode`/`verify`; a cache implementation may ignore it
/// entirely. `wall_micros` is reserved for a measured per-node cost — the current
/// call sites pass `0`, and the promotion policy falls back to a deterministic
/// `bytes x kind_weight` estimate rather than a wall-clock reading (which would
/// make promotion decisions irreproducible).
#[derive(Debug, Clone, Copy)]
pub struct CacheNote<'a> {
    /// Whether the node was a hit or was just stored.
    pub effect: CacheEffect,
    /// What the node computes (drives the promotion cost weight).
    pub kind: NodeKind,
    /// The content-addressed node id.
    pub id: &'a NodeId,
    /// The node output length in bytes.
    pub bytes: u64,
    /// Reserved measured cost; `0` at the current call sites.
    pub wall_micros: u64,
}

/// A node-output cache keyed by [`NodeId`]. Because a node's id binds its full
/// dependency closure, an unchanged closure hits and a changed dependency misses;
/// there is no invalidation pass (ADR-0025).
///
/// Implementations are disposable: [`materialize_node_cached`] treats any `get`
/// error as a miss and never trusts bytes it cannot validate, so a corrupt cache
/// causes recomputation rather than wrong output.
pub trait OutputCache {
    /// Fetch a cached node output, or `None` on a miss. A `get` error is treated
    /// as a miss by the caller (the cache is disposable, never authority).
    fn get(&self, id: &NodeId) -> Result<Option<Vec<u8>>>;
    /// Store a node output. Best-effort: a `put` error does not fail the
    /// materialization.
    fn put(&mut self, id: &NodeId, bytes: &[u8]) -> Result<()>;
    /// Phase 15.6. Advisory, best-effort reuse/store event, reported for every
    /// cache hit and store. The default is a no-op, so `NoCache` and
    /// `DerivedCache` are byte-identical to a build without this method; it is
    /// never on the exactness path and can never change an output.
    fn note(&mut self, _note: CacheNote<'_>) {}
}

/// A cache that stores nothing; used by the backward-compatible
/// [`materialize_node`] wrapper and the `use_cache = false` cold court.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoCache;

impl OutputCache for NoCache {
    fn get(&self, _id: &NodeId) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }

    fn put(&mut self, _id: &NodeId, _bytes: &[u8]) -> Result<()> {
        Ok(())
    }
}

/// Execution accounting for one materialization (ADR-0027). Reuse is claimed by
/// an *execution counter*, never by wall-clock: `nodes_reused > 0` and a smaller
/// `nodes_executed` are the evidence that persisted work was served from disk.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReuseStats {
    /// Nodes actually evaluated (cache misses).
    pub nodes_executed: u64,
    /// Nodes served whole from the cache (their subtrees were not traversed).
    pub nodes_reused: u64,
    /// Output bytes written to the cache during this materialization.
    pub cache_bytes_written: u64,
}

/// The inverse work of one reconstruction, in abstract integer units (ADR-0034,
/// plan §91). A unit is one node execution **or** one cold input byte the
/// reconstruction had to read; it is never derived from the source size.
///
/// For a *cold* run (`use_cache = false`, or a cleared cache) `node_executions`
/// is the run's `nodes_executed`; for a warm run the numerator comes from the
/// difference the persisted store made.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InverseWork {
    /// Node executions the run performed.
    pub node_executions: u64,
    /// Cold input bytes the run read (descriptor + manifest + index + seed).
    pub input_bytes: u64,
}

impl InverseWork {
    /// Assemble a receipt from the two independently measured integers.
    pub const fn new(node_executions: u64, input_bytes: u64) -> Self {
        InverseWork {
            node_executions,
            input_bytes,
        }
    }

    /// Total work units: one per execution plus one per cold input byte.
    pub const fn units(self) -> u64 {
        self.node_executions.saturating_add(self.input_bytes)
    }
}

/// `retained_inverse_work_fraction` (ADR-0034, plan §91):
///
/// ```text
/// reused_persisted_inverse_work / total_inverse_work_required_by_cold_reconstruction
/// ```
///
/// `cold` and `warm` are two receipts of the **same** query (cold = cache
/// disabled or cleared; warm = the persisted store present). Work avoided is the
/// drop in executions plus the drop in cold input bytes; the denominator is the
/// cold run's total. Both are integer work units ([`InverseWork::units`]), so the
/// fraction is derived from receipted integers, never from source size.
///
/// Returns `1.0` when the cold run required no work (an empty reconstruction),
/// which is the honest limit rather than a fabricated ratio.
pub fn retained_inverse_work_fraction(cold: InverseWork, warm: InverseWork) -> f64 {
    let total = cold.units();
    if total == 0 {
        return 1.0;
    }
    let reused = cold
        .node_executions
        .saturating_sub(warm.node_executions)
        .saturating_add(cold.input_bytes.saturating_sub(warm.input_bytes));
    reused as f64 / total as f64
}

/// Materialize one node's output bytes, recursively resolving dependencies.
pub fn materialize_node(
    parsed: &ParsedDescriptor,
    store: &dyn SeedStore,
    node: &SeedNode,
    limits: Limits,
    budget: &mut EvalBudget,
    depth: u16,
) -> Result<Vec<u8>> {
    let mut cache = NoCache;
    let mut reuse = ReuseStats::default();
    materialize_inner(
        parsed, store, &mut cache, node, limits, budget, depth, &mut reuse,
    )
}

/// Materialize one node's output bytes against an explicit [`SourceServer`].
///
/// This is the partial-reader entry point: the caller supplies a source that can
/// serve ranges without being handed a fully parsed descriptor.
pub fn materialize_node_with(
    source: &dyn SourceServer,
    store: &dyn SeedStore,
    node: &SeedNode,
    limits: Limits,
    budget: &mut EvalBudget,
    depth: u16,
) -> Result<Vec<u8>> {
    let mut cache = NoCache;
    let mut reuse = ReuseStats::default();
    materialize_inner(
        source, store, &mut cache, node, limits, budget, depth, &mut reuse,
    )
}

/// Materialize one node's output, consulting `cache` at **every** node (including
/// dependencies).
///
/// A hit returns the cached bytes *without recursing into the node's dependency
/// closure* and increments [`ReuseStats::nodes_reused`]; a miss evaluates the node
/// (recursing through the same cache) and stores the output. Exact `Q_ref` nodes
/// are cached too, since their output is equally a pure function of their id.
#[allow(clippy::too_many_arguments)]
pub fn materialize_node_cached(
    parsed: &ParsedDescriptor,
    store: &dyn SeedStore,
    cache: &mut dyn OutputCache,
    node: &SeedNode,
    limits: Limits,
    budget: &mut EvalBudget,
    depth: u16,
    reuse: &mut ReuseStats,
) -> Result<Vec<u8>> {
    materialize_inner(parsed, store, cache, node, limits, budget, depth, reuse)
}

/// The [`SourceServer`] counterpart of [`materialize_node_cached`].
#[allow(clippy::too_many_arguments)]
pub fn materialize_node_cached_with(
    source: &dyn SourceServer,
    store: &dyn SeedStore,
    cache: &mut dyn OutputCache,
    node: &SeedNode,
    limits: Limits,
    budget: &mut EvalBudget,
    depth: u16,
    reuse: &mut ReuseStats,
) -> Result<Vec<u8>> {
    materialize_inner(source, store, cache, node, limits, budget, depth, reuse)
}

#[allow(clippy::too_many_arguments)]
fn materialize_inner(
    source: &dyn SourceServer,
    store: &dyn SeedStore,
    cache: &mut dyn OutputCache,
    node: &SeedNode,
    limits: Limits,
    budget: &mut EvalBudget,
    depth: u16,
    reuse: &mut ReuseStats,
) -> Result<Vec<u8>> {
    if depth == 0 {
        return Err(Error::resource_limit("seed DAG exceeded its depth bound"));
    }

    let id = node.content_id();
    // A hit is the whole subtree: return it without traversing dependencies. A
    // cache error is a miss (the cache is disposable, never authority). Oversized
    // cached bytes are likewise treated as a poisoned miss, not returned.
    if let Ok(Some(bytes)) = cache.get(&id)
        && bytes.len() as u64 <= node.limits.max_output_bytes
    {
        reuse.nodes_reused = reuse.nodes_reused.saturating_add(1);
        budget.charge_bytes(bytes.len() as u64)?;
        cache.note(CacheNote {
            effect: CacheEffect::Hit,
            kind: node.kind,
            id: &id,
            bytes: bytes.len() as u64,
            wall_micros: 0,
        });
        return Ok(bytes);
    }

    budget.charge_node()?;
    reuse.nodes_executed = reuse.nodes_executed.saturating_add(1);

    let out = match node.kind {
        NodeKind::DocumentExact => source.serve_document(limits)?,
        NodeKind::SourceSlice | NodeKind::ResourceRef => {
            let (offset, len) = read_span_params(&node.params)?;
            source.serve_range(offset, len, limits)?
        }
        NodeKind::PdfRevision | NodeKind::PdfObject | NodeKind::PdfStreamEncoded => {
            let (_number, _generation, extra) = read_object_params(&node.params)?;
            // `extra` packs `offset` in the high 32 bits and `len` in the low 32.
            let offset = extra >> 32;
            let len = extra & 0xFFFF_FFFF;
            source.serve_range(offset, len, limits)?
        }
        NodeKind::Concat | NodeKind::PageContent => {
            let mut out = Vec::new();
            for dep in &node.deps {
                let child = load_node(store, dep)?;
                let bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                budget.charge_bytes(bytes.len() as u64)?;
                out.extend_from_slice(&bytes);
            }
            out
        }
        NodeKind::Literal => node.params.clone(),
        // The revision lineage is computed once at ingest from the byte-
        // authoritative scan and held as its canonical JSON projection; the node
        // is a leaf whose bytes are its `params` (Phase 17).
        NodeKind::PdfRevisionLineage => node.params.clone(),
        // A shared resource's canonical payload *is* its exact bytes; identity is
        // content identity, so identical bytes across documents share this node.
        NodeKind::ResourceBlob => node.params.clone(),
        NodeKind::PackageRoot => source.serve_document(limits)?,
        NodeKind::PackageMemberRaw => {
            let (offset, len) = read_span_params(&node.params)?;
            source.serve_range(offset, len, limits)?
        }
        NodeKind::PackageMemberDecoded => {
            let (_ordinal, method, _extra) = read_object_params(&node.params)?;
            let dep = node
                .deps
                .first()
                .ok_or_else(|| Error::usage("PackageMemberDecoded has no dependency"))?;
            let child = load_node(store, dep)?;
            let encoded = materialize_inner(
                source,
                store,
                cache,
                &child,
                limits,
                budget,
                depth - 1,
                reuse,
            )?;
            match method {
                // Stored (method 0): the raw span *is* the decoded bytes.
                0 => {
                    if encoded.len() as u64 != node.logical_output_len {
                        return Err(Error::reconstruction_mismatch(format!(
                            "stored member is {} bytes but the node declared {}",
                            encoded.len(),
                            node.logical_output_len
                        )));
                    }
                    encoded
                }
                // Deflate (method 8): ZIP stores bare DEFLATE, not zlib-wrapped.
                8 => derive::inflate_raw_deflate(&encoded, node.logical_output_len, limits)?,
                // Any other method is a typed decline; the exact bytes are untouched.
                other => {
                    return Err(Error::unsupported_feature(format!(
                        "zip member compression method {other} has no decoded representation"
                    )));
                }
            }
        }
        NodeKind::PackageOpcModel => {
            // The canonical OPC graph is derived on demand from the exact package
            // source (the single dependency is the exact `PackageRoot`). XML parsing
            // and all bounds live in `field::opc` / `adapter::package::opc`.
            #[cfg(feature = "opc")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("PackageOpcModel has no dependency"))?;
                let child = load_node(store, dep)?;
                let source = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::field::opc::build_opc_model(&source, limits)?
            }
            #[cfg(not(feature = "opc"))]
            {
                return Err(Error::unsupported_feature(
                    "OPC support is not compiled in (feature `opc`)",
                ));
            }
        }
        NodeKind::DocxModel => {
            #[cfg(feature = "docx")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("DocxModel has no dependency"))?;
                let child = load_node(store, dep)?;
                let opc_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::docx::build_docx_model(&opc_bytes, limits)?
            }
            #[cfg(not(feature = "docx"))]
            {
                return Err(Error::unsupported_feature(
                    "DOCX support is not compiled in (feature `docx`)",
                ));
            }
        }
        NodeKind::DocxStory => {
            #[cfg(feature = "docx")]
            {
                let (story, part_name, profile) =
                    crate::adapter::docx::read_story_params(&node.params)?;
                let part_dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("DocxStory has no part dependency"))?;
                let part_node = load_node(store, part_dep)?;
                let part_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &part_node,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                let styles = match node.deps.get(1) {
                    Some(styles_dep) => {
                        let styles_node = load_node(store, styles_dep)?;
                        let styles_bytes = materialize_inner(
                            source,
                            store,
                            cache,
                            &styles_node,
                            limits,
                            budget,
                            depth - 1,
                            reuse,
                        )?;
                        Some(crate::adapter::docx::parse_styles(&styles_bytes, limits)?)
                    }
                    None => None,
                };
                crate::adapter::docx::wml::parse_story(
                    &part_bytes,
                    &part_name,
                    story,
                    &profile,
                    styles.as_ref(),
                    limits,
                )?
                .encode()
            }
            #[cfg(not(feature = "docx"))]
            {
                return Err(Error::unsupported_feature(
                    "DOCX support is not compiled in (feature `docx`)",
                ));
            }
        }
        NodeKind::EpubModel => {
            // The canonical EPUB (OCF) graph is derived on demand from the exact
            // package source (the single dependency is the exact `PackageRoot`). It
            // does not route through OPC. XML parsing and all bounds live in
            // `adapter::epub`.
            #[cfg(feature = "epub")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("EpubModel has no dependency"))?;
                let child = load_node(store, dep)?;
                let source = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::epub::build_epub_model(&source, limits)?
            }
            #[cfg(not(feature = "epub"))]
            {
                return Err(Error::unsupported_feature(
                    "EPUB support is not compiled in (feature `epub`)",
                ));
            }
        }
        NodeKind::EpubContent => {
            // One spine item's XHTML content document parsed into its bounded native
            // model. Its single dependency is the decoded member node; the parse is
            // bounded entirely inside `adapter::epub::content`. It never executes
            // scripts and never fetches an external target.
            #[cfg(feature = "epub")]
            {
                let (_spine, _ordinal, base_dir, _profile) =
                    crate::adapter::epub::read_content_params(&node.params)?;
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("EpubContent has no part dependency"))?;
                let child = load_node(store, dep)?;
                let bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::epub::parse_content(&bytes, &base_dir, limits)?.encode()
            }
            #[cfg(not(feature = "epub"))]
            {
                return Err(Error::unsupported_feature(
                    "EPUB support is not compiled in (feature `epub`)",
                ));
            }
        }
        NodeKind::OdtModel => {
            // The canonical ODT (ODF) graph is derived on demand from the exact
            // package source (the single dependency is the exact `PackageRoot`). It
            // does not route through OPC (ODF has no `[Content_Types].xml`). XML
            // parsing and all bounds live in `adapter::odt`.
            #[cfg(feature = "odt")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("OdtModel has no dependency"))?;
                let child = load_node(store, dep)?;
                let source = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::odt::build_odt_model(&source, limits)?
            }
            #[cfg(not(feature = "odt"))]
            {
                return Err(Error::unsupported_feature(
                    "ODT support is not compiled in (feature `odt`)",
                ));
            }
        }
        NodeKind::OdtContent => {
            // The OpenDocument main part (`content.xml`) parsed into its bounded
            // native model. Its single dependency is the decoded member node; the
            // parse is bounded entirely inside `adapter::odt`. No script execution,
            // no remote fetch.
            #[cfg(feature = "odt")]
            {
                let (_ordinal, part_name, profile) =
                    crate::adapter::odt::read_content_params(&node.params)?;
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("OdtContent has no part dependency"))?;
                let child = load_node(store, dep)?;
                let bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::odt::parse_content(&bytes, &part_name, &profile, limits)?.encode()
            }
            #[cfg(not(feature = "odt"))]
            {
                return Err(Error::unsupported_feature(
                    "ODT support is not compiled in (feature `odt`)",
                ));
            }
        }
        NodeKind::OdsModel => {
            // The canonical ODS (ODF spreadsheet) graph is derived on demand from the
            // exact package source (the single dependency is the exact `PackageRoot`).
            // It does not route through OPC (ODF has no `[Content_Types].xml`). XML
            // parsing and all bounds live in `adapter::ods`.
            #[cfg(feature = "ods")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("OdsModel has no dependency"))?;
                let child = load_node(store, dep)?;
                let source = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::ods::build_ods_model(&source, limits)?
            }
            #[cfg(not(feature = "ods"))]
            {
                return Err(Error::unsupported_feature(
                    "ODS support is not compiled in (feature `ods`)",
                ));
            }
        }
        NodeKind::OdsContent => {
            // The OpenDocument spreadsheet main part (`content.xml`) parsed into its
            // bounded native model. Its single dependency is the decoded member node;
            // the parse is bounded entirely inside `adapter::ods`. No script execution,
            // no remote fetch.
            #[cfg(feature = "ods")]
            {
                let (_ordinal, part_name, profile) =
                    crate::adapter::ods::read_content_params(&node.params)?;
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("OdsContent has no part dependency"))?;
                let child = load_node(store, dep)?;
                let bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::ods::parse_content(&bytes, &part_name, &profile, limits)?.encode()
            }
            #[cfg(not(feature = "ods"))]
            {
                return Err(Error::unsupported_feature(
                    "ODS support is not compiled in (feature `ods`)",
                ));
            }
        }
        NodeKind::OdsStyles => {
            // The OpenDocument styles part (`styles.xml`) parsed into its bounded
            // cell-style/number-format model. Its single dependency is the decoded
            // styles member node.
            #[cfg(feature = "ods")]
            {
                let (_ordinal, part_name) = crate::adapter::ods::read_styles_params(&node.params)?;
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("OdsStyles has no part dependency"))?;
                let child = load_node(store, dep)?;
                let bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::ods::parse_styles(&bytes, &part_name, limits)?.encode()
            }
            #[cfg(not(feature = "ods"))]
            {
                return Err(Error::unsupported_feature(
                    "ODS support is not compiled in (feature `ods`)",
                ));
            }
        }
        NodeKind::OdpModel => {
            // The canonical ODP (ODF presentation) graph is derived on demand from
            // the exact package source (the single dependency is the exact
            // `PackageRoot`). It does not route through OPC (ODF has no
            // `[Content_Types].xml`). XML parsing and all bounds live in
            // `adapter::odp`.
            #[cfg(feature = "odp")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("OdpModel has no dependency"))?;
                let child = load_node(store, dep)?;
                let source = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::odp::build_odp_model(&source, limits)?
            }
            #[cfg(not(feature = "odp"))]
            {
                return Err(Error::unsupported_feature(
                    "ODP support is not compiled in (feature `odp`)",
                ));
            }
        }
        NodeKind::OdpContent => {
            // The OpenDocument presentation main part (`content.xml`) parsed into
            // its bounded native model. Its single dependency is the decoded member
            // node; the parse is bounded entirely inside `adapter::odp`. No script
            // execution, no remote fetch.
            #[cfg(feature = "odp")]
            {
                let (_ordinal, part_name, profile) =
                    crate::adapter::odp::read_content_params(&node.params)?;
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("OdpContent has no part dependency"))?;
                let child = load_node(store, dep)?;
                let bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::odp::parse_content(&bytes, &part_name, &profile, limits)?.encode()
            }
            #[cfg(not(feature = "odp"))]
            {
                return Err(Error::unsupported_feature(
                    "ODP support is not compiled in (feature `odp`)",
                ));
            }
        }
        NodeKind::OdpStyles => {
            // The OpenDocument styles part (`styles.xml`) parsed into its bounded
            // style/master-page model. Its single dependency is the decoded styles
            // member node.
            #[cfg(feature = "odp")]
            {
                let (_ordinal, part_name) = crate::adapter::odp::read_styles_params(&node.params)?;
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("OdpStyles has no part dependency"))?;
                let child = load_node(store, dep)?;
                let bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::odp::parse_styles(&bytes, &part_name, limits)?.encode()
            }
            #[cfg(not(feature = "odp"))]
            {
                return Err(Error::unsupported_feature(
                    "ODP support is not compiled in (feature `odp`)",
                ));
            }
        }
        NodeKind::JsonModel => {
            // The canonical JSON structured-tree model is derived on demand from the
            // exact source. JSON has no package layer, so its single dependency is
            // the exact `DocumentExact` root (keyed by `sha256(source)` per
            // ADR-0060): the model node *reads the source bytes*, so it must carry a
            // source-identity input, never alias another field's source. Parsing and
            // all bounds live in `adapter::json`.
            #[cfg(feature = "json")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("JsonModel has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::json::build_json_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "json"))]
            {
                return Err(Error::unsupported_feature(
                    "JSON support is not compiled in (feature `json`)",
                ));
            }
        }
        NodeKind::Json5Model => {
            // The canonical JSON5/JSONC structured-tree model is derived on demand
            // from the exact source. JSON5 has no package layer, so its single
            // dependency is the exact `DocumentExact` root (keyed by `sha256(source)`
            // per ADR-0060): the model node *reads the source bytes*, so it must
            // carry a source-identity input, never alias another field's source.
            // Parsing and all bounds live in `adapter::json5`.
            #[cfg(feature = "json5")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("Json5Model has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::json5::build_json5_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "json5"))]
            {
                return Err(Error::unsupported_feature(
                    "JSON5 support is not compiled in (feature `json5`)",
                ));
            }
        }
        NodeKind::CborModel => {
            // The canonical CBOR structured-tree model is derived on demand from the
            // exact source. CBOR has no package layer, so its single dependency is the
            // exact `DocumentExact` root (keyed by `sha256(source)` per ADR-0060): the
            // model node *reads the source bytes*, so it must carry a source-identity
            // input, never alias another field's source. Parsing and all bounds live
            // in `adapter::cbor`.
            #[cfg(feature = "cbor")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("CborModel has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::cbor::build_cbor_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "cbor"))]
            {
                return Err(Error::unsupported_feature(
                    "CBOR support is not compiled in (feature `cbor`)",
                ));
            }
        }
        NodeKind::MsgpackModel => {
            // The canonical MessagePack structured-tree model is derived on demand from
            // the exact source. MessagePack has no package layer, so its single
            // dependency is the exact `DocumentExact` root (keyed by `sha256(source)`
            // per ADR-0060): the model node *reads the source bytes*, so it must carry
            // a source-identity input, never alias another field's source. Parsing and
            // all bounds live in `adapter::msgpack`.
            #[cfg(feature = "msgpack")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("MsgpackModel has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::msgpack::build_msgpack_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "msgpack"))]
            {
                return Err(Error::unsupported_feature(
                    "MessagePack support is not compiled in (feature `msgpack`)",
                ));
            }
        }
        NodeKind::ConfigModel => {
            // The canonical config-family model is derived on demand from the exact
            // source. The config family has no package layer, so its single dependency
            // is the exact `DocumentExact` root (keyed by `sha256(source)` per
            // ADR-0060): the model node *reads the source bytes*, so it must carry a
            // source-identity input, never alias another field's source. Parsing and
            // all bounds live in `adapter::config`.
            #[cfg(feature = "config")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("ConfigModel has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::config::build_config_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "config"))]
            {
                return Err(Error::unsupported_feature(
                    "config support is not compiled in (feature `config`)",
                ));
            }
        }
        NodeKind::FeedModel => {
            // The canonical RSS/Atom feed model is derived on demand from the exact
            // source, via the shared bounded XML parser. A feed has no package layer,
            // so its single dependency is the exact `DocumentExact` root (keyed by
            // `sha256(source)` per ADR-0060): the model node *reads the source bytes*,
            // so it must carry a source-identity input, never alias another field's
            // source. Parsing and all bounds live in `adapter::feed`.
            #[cfg(feature = "feed")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("FeedModel has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::feed::build_feed_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "feed"))]
            {
                return Err(Error::unsupported_feature(
                    "feed support is not compiled in (feature `feed`)",
                ));
            }
        }
        NodeKind::GeojsonModel => {
            // The canonical GeoJSON model is derived on demand from the exact source,
            // via the shared bounded JSON parser. GeoJSON has no package layer, so its
            // single dependency is the exact `DocumentExact` root (keyed by
            // `sha256(source)` per ADR-0060): the model node *reads the source bytes*,
            // so it must carry a source-identity input, never alias another field's
            // source. Parsing and all bounds live in `adapter::geojson`.
            #[cfg(feature = "geojson")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("GeojsonModel has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::geojson::build_geojson_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "geojson"))]
            {
                return Err(Error::unsupported_feature(
                    "GeoJSON support is not compiled in (feature `geojson`)",
                ));
            }
        }
        NodeKind::GisModel => {
            // The canonical KML/GPX geospatial model is derived on demand from the
            // exact source, via the shared bounded XML parser. A GIS document has no
            // package layer, so its single dependency is the exact `DocumentExact`
            // root (keyed by `sha256(source)` per ADR-0060): the model node *reads the
            // source bytes*, so it must carry a source-identity input, never alias
            // another field's source. Parsing and all bounds live in `adapter::gis`.
            #[cfg(feature = "gis")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("GisModel has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::gis::build_gis_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "gis"))]
            {
                return Err(Error::unsupported_feature(
                    "GIS support is not compiled in (feature `gis`)",
                ));
            }
        }
        NodeKind::NotebookModel => {
            // The canonical Jupyter notebook model is derived on demand from the exact
            // source, via the shared bounded JSON parser. A notebook has no package
            // layer, so its single dependency is the exact `DocumentExact` root (keyed by
            // `sha256(source)` per ADR-0060): the model node *reads the source bytes*,
            // so it must carry a source-identity input, never alias another field's
            // source. Parsing and all bounds live in `adapter::notebook`.
            #[cfg(feature = "notebook")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("NotebookModel has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::notebook::build_notebook_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "notebook"))]
            {
                return Err(Error::unsupported_feature(
                    "notebook support is not compiled in (feature `notebook`)",
                ));
            }
        }
        NodeKind::FixedWidthModel => {
            // The canonical fixed-width model is derived on demand from the exact
            // source. Fixed-width has no package layer, so its single dependency is the
            // exact `DocumentExact` root (keyed by `sha256(source)` per ADR-0060): the
            // model node *reads the source bytes*, so it must carry a source-identity
            // input, never alias another field's source. Parsing and all bounds live in
            // `adapter::fixedwidth`.
            #[cfg(feature = "fixedwidth")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("FixedWidthModel has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::fixedwidth::build_fixedwidth_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "fixedwidth"))]
            {
                return Err(Error::unsupported_feature(
                    "fixed-width support is not compiled in (feature `fixedwidth`)",
                ));
            }
        }
        NodeKind::RstModel => {
            // The canonical reStructuredText prose model is derived on demand from the
            // exact source. reStructuredText has no package layer, so its single
            // dependency is the exact `DocumentExact` root (keyed by `sha256(source)`
            // per ADR-0060): the model node *reads the source bytes*, so it must carry a
            // source-identity input, never alias another field's source. Parsing and all
            // bounds live in `adapter::rst`.
            #[cfg(feature = "rst")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("RstModel has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::rst::build_rst_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "rst"))]
            {
                return Err(Error::unsupported_feature(
                    "reStructuredText support is not compiled in (feature `rst`)",
                ));
            }
        }
        NodeKind::AsciidocModel => {
            // The canonical AsciiDoc prose model is derived on demand from the exact
            // source. AsciiDoc has no package layer, so its single dependency is the
            // exact `DocumentExact` root (keyed by `sha256(source)` per ADR-0060): the
            // model node *reads the source bytes*, so it must carry a source-identity
            // input, never alias another field's source. Parsing and all bounds live in
            // `adapter::asciidoc`.
            #[cfg(feature = "asciidoc")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("AsciidocModel has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::asciidoc::build_asciidoc_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "asciidoc"))]
            {
                return Err(Error::unsupported_feature(
                    "AsciiDoc support is not compiled in (feature `asciidoc`)",
                ));
            }
        }
        NodeKind::MdxModel => {
            // The canonical MDX (Markdown + JSX/ESM) model is derived on demand from
            // the exact source. MDX has no package layer, so its single dependency is
            // the exact `DocumentExact` root (keyed by `sha256(source)` per ADR-0060):
            // the model node *reads the source bytes*, so it must carry a
            // source-identity input, never alias another field's source. Parsing and
            // all bounds live in `adapter::mdx`.
            #[cfg(feature = "mdx")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("MdxModel has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::mdx::build_mdx_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "mdx"))]
            {
                return Err(Error::unsupported_feature(
                    "MDX support is not compiled in (feature `mdx`)",
                ));
            }
        }
        NodeKind::MhtmlModel => {
            // The canonical MHTML (MIME HTML) model is derived on demand from the exact
            // source. MHTML has no package layer, so its single dependency is the exact
            // `DocumentExact` root (keyed by `sha256(source)` per ADR-0060): the model
            // node *reads the source bytes*, so it must carry a source-identity input,
            // never alias another field's source. Parsing and all bounds live in
            // `adapter::mhtml` (which reuses `adapter::eml` and `adapter::html`).
            #[cfg(feature = "mhtml")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("MhtmlModel has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::mhtml::build_mhtml_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "mhtml"))]
            {
                return Err(Error::unsupported_feature(
                    "MHTML support is not compiled in (feature `mhtml`)",
                ));
            }
        }
        NodeKind::LogstreamModel => {
            // The canonical syslog / log-stream model is derived on demand from the
            // exact source. A log stream has no package layer, so its single dependency
            // is the exact `DocumentExact` root (keyed by `sha256(source)` per
            // ADR-0060): the model node *reads the source bytes*, so it must carry a
            // source-identity input, never alias another field's source. Parsing and
            // all bounds live in `adapter::logstream`.
            #[cfg(feature = "logstream")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("LogstreamModel has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::logstream::build_logstream_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "logstream"))]
            {
                return Err(Error::unsupported_feature(
                    "log-stream support is not compiled in (feature `logstream`)",
                ));
            }
        }
        NodeKind::PkgmetaModel => {
            // The canonical package-metadata model is derived on demand from the exact
            // source. A manifest has no package layer, so its single dependency is the
            // exact `DocumentExact` root (keyed by `sha256(source)` per ADR-0060): the
            // model node *reads the source bytes*, so it must carry a source-identity
            // input, never alias another field's source. Parsing (which reuses the JSON
            // and TOML parsers) and all bounds live in `adapter::pkgmeta`.
            #[cfg(feature = "pkgmeta")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("PkgmetaModel has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::pkgmeta::build_pkgmeta_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "pkgmeta"))]
            {
                return Err(Error::unsupported_feature(
                    "package-metadata support is not compiled in (feature `pkgmeta`)",
                ));
            }
        }
        NodeKind::ApispecModel => {
            // The canonical API/specification model is derived on demand from the exact
            // source, via the shared bounded JSON parser. An API spec has no package
            // layer, so its single dependency is the exact `DocumentExact` root (keyed
            // by `sha256(source)` per ADR-0060): the model node *reads the source
            // bytes*, so it must carry a source-identity input, never alias another
            // field's source. Parsing and all bounds live in `adapter::apispec`.
            #[cfg(feature = "apispec")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("ApispecModel has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::apispec::build_apispec_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "apispec"))]
            {
                return Err(Error::unsupported_feature(
                    "API-specification support is not compiled in (feature `apispec`)",
                ));
            }
        }
        NodeKind::YamlModel => {
            // The canonical YAML structured-tree model is derived on demand from the
            // exact source. YAML has no package layer, so its single dependency is
            // the exact `DocumentExact` root (keyed by `sha256(source)` per
            // ADR-0060): the model node *reads the source bytes*, so it must carry a
            // source-identity input, never alias another field's source. Parsing and
            // all bounds live in `adapter::yaml`.
            #[cfg(feature = "yaml")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("YamlModel has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::yaml::build_yaml_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "yaml"))]
            {
                return Err(Error::unsupported_feature(
                    "YAML support is not compiled in (feature `yaml`)",
                ));
            }
        }
        NodeKind::CsvModel => {
            // The canonical CSV/TSV tabular model is derived on demand from the
            // exact source. CSV has no package layer, so its single dependency is
            // the exact `DocumentExact` root (keyed by `sha256(source)` per
            // ADR-0060): the model node *reads the source bytes*, so it must carry a
            // source-identity input, never alias another field's source. Parsing and
            // all bounds live in `adapter::csv`.
            #[cfg(feature = "csv")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("CsvModel has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::csv::build_csv_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "csv"))]
            {
                return Err(Error::unsupported_feature(
                    "CSV support is not compiled in (feature `csv`)",
                ));
            }
        }
        NodeKind::MarkdownModel => {
            // The canonical Markdown prose model is derived on demand from the exact
            // source. Markdown has no package layer, so its single dependency is the
            // exact `DocumentExact` root (keyed by `sha256(source)` per ADR-0060): the
            // model node *reads the source bytes*, so it must carry a source-identity
            // input, never alias another field's source. Parsing and all bounds live
            // in `adapter::markdown`.
            #[cfg(feature = "markdown")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("MarkdownModel has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::markdown::build_markdown_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "markdown"))]
            {
                return Err(Error::unsupported_feature(
                    "Markdown support is not compiled in (feature `markdown`)",
                ));
            }
        }
        NodeKind::XmlModel => {
            // The canonical XML structured-tree model is derived on demand from the
            // exact source. XML has no package layer, so its single dependency is the
            // exact `DocumentExact` root (keyed by `sha256(source)` per ADR-0060): the
            // model node *reads the source bytes*, so it must carry a source-identity
            // input, never alias another field's source. Parsing and all bounds live
            // in `adapter::xml`.
            #[cfg(feature = "xml")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("XmlModel has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::xml::build_xml_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "xml"))]
            {
                return Err(Error::unsupported_feature(
                    "XML support is not compiled in (feature `xml`)",
                ));
            }
        }
        NodeKind::HtmlModel => {
            // The canonical HTML document model is derived on demand from the exact
            // source. HTML has no package layer, so its single dependency is the
            // exact `DocumentExact` root (keyed by `sha256(source)` per ADR-0060):
            // the model node *reads the source bytes*, so it must carry a
            // source-identity input, never alias another field's source. The parser
            // is bounded and error-recovering; all bounds live in `adapter::html`.
            #[cfg(feature = "html")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("HtmlModel has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::html::build_html_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "html"))]
            {
                return Err(Error::unsupported_feature(
                    "HTML support is not compiled in (feature `html`)",
                ));
            }
        }
        NodeKind::TomlModel => {
            // The canonical TOML model is derived on demand from the exact source.
            // TOML has no package layer, so its single dependency is the exact
            // `DocumentExact` root (keyed by `sha256(source)` per ADR-0060): the
            // model node *reads the source bytes*, so it must carry a
            // source-identity input, never alias another field's source. All bounds
            // live in `adapter::toml`.
            #[cfg(feature = "toml")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("TomlModel has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::toml::build_toml_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "toml"))]
            {
                return Err(Error::unsupported_feature(
                    "TOML support is not compiled in (feature `toml`)",
                ));
            }
        }
        NodeKind::JsonlModel => {
            // The canonical per-line JSONL model is derived on demand from the exact
            // source. JSONL has no package layer, so its single dependency is the
            // exact `DocumentExact` root (keyed by `sha256(source)` per ADR-0060): the
            // model node *reads the source bytes*, so it must carry a source-identity
            // input, never alias another field's source. Parsing and all bounds live
            // in `adapter::jsonl` (which reuses `adapter::json` per line).
            #[cfg(feature = "jsonl")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("JsonlModel has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::jsonl::build_jsonl_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "jsonl"))]
            {
                return Err(Error::unsupported_feature(
                    "JSONL support is not compiled in (feature `jsonl`)",
                ));
            }
        }
        NodeKind::EmlModel => {
            // The canonical EML/MIME model is derived on demand from the exact source.
            // EML has no package layer, so its single dependency is the exact
            // `DocumentExact` root (keyed by `sha256(source)` per ADR-0060): the model
            // node *reads the source bytes*, so it must carry a source-identity input,
            // never alias another field's source. Parsing and all bounds live in
            // `adapter::eml`.
            #[cfg(feature = "eml")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("EmlModel has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::eml::build_eml_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "eml"))]
            {
                return Err(Error::unsupported_feature(
                    "EML support is not compiled in (feature `eml`)",
                ));
            }
        }
        NodeKind::ParquetModel => {
            // The canonical Parquet model is derived on demand from the exact source.
            // Parquet has no package layer, so its single dependency is the exact
            // `DocumentExact` root (keyed by `sha256(source)` per ADR-0060): the model
            // node *reads the source bytes*, so it must carry a source-identity input,
            // never alias another field's source. Parsing and all bounds live in
            // `adapter::parquet`.
            #[cfg(feature = "parquet")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("ParquetModel has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::parquet::build_parquet_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "parquet"))]
            {
                return Err(Error::unsupported_feature(
                    "Parquet support is not compiled in (feature `parquet`)",
                ));
            }
        }
        NodeKind::ArrowModel => {
            // The canonical Arrow IPC model is derived on demand from the exact
            // source. Arrow has no package layer, so its single dependency is the
            // exact `DocumentExact` root (keyed by `sha256(source)` per ADR-0060): the
            // model node *reads the source bytes*, so it must carry a source-identity
            // input, never alias another field's source. Parsing and all bounds live
            // in `adapter::arrow`.
            #[cfg(feature = "arrow")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("ArrowModel has no source dependency"))?;
                let child = load_node(store, dep)?;
                let source_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::arrow::build_arrow_model(&source_bytes, limits)?
            }
            #[cfg(not(feature = "arrow"))]
            {
                return Err(Error::unsupported_feature(
                    "Arrow support is not compiled in (feature `arrow`)",
                ));
            }
        }
        NodeKind::XlsxModel => {
            // The canonical XLSX (SpreadsheetML) discovery model is derived on
            // demand from the canonical OPC model (the single dependency). XML
            // parsing and all bounds live in `adapter::xlsx`.
            #[cfg(feature = "xlsx")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("XlsxModel has no dependency"))?;
                let child = load_node(store, dep)?;
                let opc_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::xlsx::build_xlsx_model(&opc_bytes, limits)?
            }
            #[cfg(not(feature = "xlsx"))]
            {
                return Err(Error::unsupported_feature(
                    "XLSX support is not compiled in (feature `xlsx`)",
                ));
            }
        }
        NodeKind::XlsxWorkbook => {
            // The parsed `xl/workbook.xml` sheet inventory. Its single dependency
            // is the decoded workbook member; the parse is bounded in
            // `adapter::xlsx`.
            #[cfg(feature = "xlsx")]
            {
                let (_ordinal, part_name) =
                    crate::adapter::xlsx::read_workbook_params(&node.params)?;
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("XlsxWorkbook has no part dependency"))?;
                let child = load_node(store, dep)?;
                let bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                let workbook = crate::adapter::xlsx::parse_workbook(&bytes, limits)?;
                let _ = part_name;
                workbook.encode()
            }
            #[cfg(not(feature = "xlsx"))]
            {
                return Err(Error::unsupported_feature(
                    "XLSX support is not compiled in (feature `xlsx`)",
                ));
            }
        }
        NodeKind::XlsxSheet => {
            // One worksheet parsed into its bounded cell model. Dependency 0 is
            // the decoded worksheet member; the optional dependency 1 is the
            // decoded shared-strings member, resolved into string cells.
            #[cfg(feature = "xlsx")]
            {
                let (_ordinal, shared_ordinal, _profile, part_name, sheet_name) =
                    crate::adapter::xlsx::read_sheet_params(&node.params)?;
                let part_dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("XlsxSheet has no part dependency"))?;
                let part_node = load_node(store, part_dep)?;
                let part_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &part_node,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                let shared = match (shared_ordinal, node.deps.get(1)) {
                    (Some(_), Some(shared_dep)) => {
                        let shared_node = load_node(store, shared_dep)?;
                        let shared_bytes = materialize_inner(
                            source,
                            store,
                            cache,
                            &shared_node,
                            limits,
                            budget,
                            depth - 1,
                            reuse,
                        )?;
                        Some(crate::adapter::xlsx::parse_shared_strings(
                            &shared_bytes,
                            limits,
                        )?)
                    }
                    _ => None,
                };
                crate::adapter::xlsx::parse_worksheet(
                    &part_bytes,
                    &part_name,
                    &sheet_name,
                    shared.as_deref(),
                    limits,
                )?
                .encode()
            }
            #[cfg(not(feature = "xlsx"))]
            {
                return Err(Error::unsupported_feature(
                    "XLSX support is not compiled in (feature `xlsx`)",
                ));
            }
        }
        NodeKind::PptxModel => {
            // The canonical PPTX (PresentationML) discovery model is derived on
            // demand from the canonical OPC model (the single dependency). XML
            // parsing and all bounds live in `adapter::pptx`.
            #[cfg(feature = "pptx")]
            {
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("PptxModel has no dependency"))?;
                let child = load_node(store, dep)?;
                let opc_bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::pptx::build_pptx_model(&opc_bytes, limits)?
            }
            #[cfg(not(feature = "pptx"))]
            {
                return Err(Error::unsupported_feature(
                    "PPTX support is not compiled in (feature `pptx`)",
                ));
            }
        }
        NodeKind::PptxPresentation => {
            // The parsed `ppt/presentation.xml` slide inventory. Its single
            // dependency is the decoded presentation member; the parse is bounded
            // in `adapter::pptx`.
            #[cfg(feature = "pptx")]
            {
                let (_ordinal, part_name) =
                    crate::adapter::pptx::read_presentation_params(&node.params)?;
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("PptxPresentation has no part dependency"))?;
                let child = load_node(store, dep)?;
                let bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                let presentation = crate::adapter::pptx::parse_presentation(&bytes, limits)?;
                let _ = part_name;
                presentation.encode()
            }
            #[cfg(not(feature = "pptx"))]
            {
                return Err(Error::unsupported_feature(
                    "PPTX support is not compiled in (feature `pptx`)",
                ));
            }
        }
        NodeKind::PptxSlide => {
            // One slide parsed into its bounded shape model. Its single dependency
            // is the decoded slide member.
            #[cfg(feature = "pptx")]
            {
                let (_ordinal, profile, part_name) =
                    crate::adapter::pptx::read_slide_params(&node.params)?;
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("PptxSlide has no part dependency"))?;
                let child = load_node(store, dep)?;
                let bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::pptx::parse_slide(&bytes, &part_name, &profile, limits)?.encode()
            }
            #[cfg(not(feature = "pptx"))]
            {
                return Err(Error::unsupported_feature(
                    "PPTX support is not compiled in (feature `pptx`)",
                ));
            }
        }
        NodeKind::PptxNotes => {
            // One notes-slide parsed into its bounded text model. Its single
            // dependency is the decoded notes member.
            #[cfg(feature = "pptx")]
            {
                let (_ordinal, profile, part_name) =
                    crate::adapter::pptx::read_notes_params(&node.params)?;
                let dep = node
                    .deps
                    .first()
                    .ok_or_else(|| Error::usage("PptxNotes has no part dependency"))?;
                let child = load_node(store, dep)?;
                let bytes = materialize_inner(
                    source,
                    store,
                    cache,
                    &child,
                    limits,
                    budget,
                    depth - 1,
                    reuse,
                )?;
                crate::adapter::pptx::parse_notes(&bytes, &part_name, &profile, limits)?.encode()
            }
            #[cfg(not(feature = "pptx"))]
            {
                return Err(Error::unsupported_feature(
                    "PPTX support is not compiled in (feature `pptx`)",
                ));
            }
        }
        NodeKind::PdfStreamDecoded => {
            let dep = node
                .deps
                .first()
                .ok_or_else(|| Error::usage("PdfStreamDecoded has no dependency"))?;
            let child = load_node(store, dep)?;
            let encoded = materialize_inner(
                source,
                store,
                cache,
                &child,
                limits,
                budget,
                depth - 1,
                reuse,
            )?;
            derive::inflate_zlib(&encoded, node.logical_output_len, limits)?
        }
        NodeKind::ContentOperators => {
            let dep = node
                .deps
                .first()
                .ok_or_else(|| Error::usage("ContentOperators has no dependency"))?;
            let child = load_node(store, dep)?;
            let decoded = materialize_inner(
                source,
                store,
                cache,
                &child,
                limits,
                budget,
                depth - 1,
                reuse,
            )?;
            derive::content_operators(&decoded, limits)?
        }
        NodeKind::TextRuns => {
            let dep = node
                .deps
                .first()
                .ok_or_else(|| Error::usage("TextRuns has no dependency"))?;
            let child = load_node(store, dep)?;
            let ops = materialize_inner(
                source,
                store,
                cache,
                &child,
                limits,
                budget,
                depth - 1,
                reuse,
            )?;
            derive::text_runs(&ops, limits)?
        }
        NodeKind::PagePreview => {
            let dep = node
                .deps
                .first()
                .ok_or_else(|| Error::usage("PagePreview has no dependency"))?;
            let child = load_node(store, dep)?;
            let content = materialize_inner(
                source,
                store,
                cache,
                &child,
                limits,
                budget,
                depth - 1,
                reuse,
            )?;
            let page = read_u32_params(&node.params)?;
            derive::page_preview(page, &content, limits)?
        }
    };

    if out.len() as u64 > node.limits.max_output_bytes {
        return Err(Error::resource_limit(format!(
            "node {} produced {} bytes > its cap {}",
            node.kind.name(),
            out.len(),
            node.limits.max_output_bytes
        )));
    }
    budget.charge_bytes(out.len() as u64)?;
    // Best-effort persistence: a cache write failure never fails the observation.
    if cache.put(&id, &out).is_ok() {
        reuse.cache_bytes_written = reuse.cache_bytes_written.saturating_add(out.len() as u64);
        cache.note(CacheNote {
            effect: CacheEffect::Stored,
            kind: node.kind,
            id: &id,
            bytes: out.len() as u64,
            wall_micros: 0,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::ObjectSource;
    use crate::dra::{Op, Program};
    use crate::integrity::sha256;
    use crate::{EXACTNESS_PROFILE_EXACT_BYTES, SOURCE_FORMAT_OPAQUE};

    fn parsed_for(source: &[u8]) -> ParsedDescriptor {
        let d = crate::container::Descriptor {
            universe: crate::container::UNIVERSE.to_string(),
            source_format: SOURCE_FORMAT_OPAQUE,
            format_basis: "opaque:test".to_string(),
            models: vec![],
            channels: vec![],
            objects: vec![ObjectSource::Inline(source.to_vec())],
            program: Program::new(vec![Op::EmitObject { object_id: 0 }]),
            observation_index: None,
            seek_directory: false,
            checkpoints: None,
            source_sha256: sha256(source),
            source_len: source.len() as u64,
        };
        let (bytes, _cost) = d.serialize().unwrap();
        let _ = EXACTNESS_PROFILE_EXACT_BYTES;
        crate::container::Descriptor::parse(&bytes, Limits::DEFAULT).unwrap()
    }

    #[test]
    fn source_slice_matches_exact_bytes() {
        let parsed = parsed_for(b"hello field world");
        let mut budget = EvalBudget::default();
        let (off, len) = (6u64, 5u64);
        let node = SeedNode::new(
            NodeKind::SourceSlice,
            len,
            crate::field::node::span_params(off, len),
            vec![],
            "test",
        );
        let bytes = materialize_node(
            &parsed,
            &crate::store::FsSeedStore::open(
                std::env::temp_dir().join(format!("vole-dag-{}", std::process::id())),
            )
            .unwrap(),
            &node,
            Limits::DEFAULT,
            &mut budget,
            8,
        )
        .unwrap();
        assert_eq!(bytes, b"field");
    }

    #[test]
    fn literal_roundtrips() {
        let parsed = parsed_for(b"x");
        let mut budget = EvalBudget::default();
        let node = SeedNode::new(NodeKind::Literal, 3, b"abc".to_vec(), vec![], "test");
        let bytes = materialize_node(
            &parsed,
            &crate::store::FsSeedStore::open(
                std::env::temp_dir().join(format!("vole-lit-{}", std::process::id())),
            )
            .unwrap(),
            &node,
            Limits::DEFAULT,
            &mut budget,
            4,
        )
        .unwrap();
        assert_eq!(bytes, b"abc");
    }
}
