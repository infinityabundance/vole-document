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
