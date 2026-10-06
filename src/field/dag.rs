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

    fn charge_bytes(&mut self, n: u64) -> Result<()> {
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

/// Materialize one node's output bytes, recursively resolving dependencies.
pub fn materialize_node(
    parsed: &ParsedDescriptor,
    store: &dyn SeedStore,
    node: &SeedNode,
    limits: Limits,
    budget: &mut EvalBudget,
    depth: u16,
) -> Result<Vec<u8>> {
    if depth == 0 {
        return Err(Error::resource_limit("seed DAG exceeded its depth bound"));
    }
    budget.charge_node()?;

    let out = match node.kind {
        NodeKind::DocumentExact => crate::materialize::materialize(parsed, limits)?,
        NodeKind::SourceSlice | NodeKind::ResourceRef => {
            let (offset, len) = read_span_params(&node.params)?;
            serve_source_range(parsed, offset, len, limits)?
        }
        NodeKind::PdfRevision | NodeKind::PdfObject | NodeKind::PdfStreamEncoded => {
            let (_number, _generation, extra) = read_object_params(&node.params)?;
            // `extra` packs `offset` in the high 32 bits and `len` in the low 32.
            let offset = extra >> 32;
            let len = extra & 0xFFFF_FFFF;
            serve_source_range(parsed, offset, len, limits)?
        }
        NodeKind::Concat | NodeKind::PageContent => {
            let mut out = Vec::new();
            for dep in &node.deps {
                let child = load_node(store, dep)?;
                let bytes = materialize_node(parsed, store, &child, limits, budget, depth - 1)?;
                budget.charge_bytes(bytes.len() as u64)?;
                out.extend_from_slice(&bytes);
            }
            out
        }
        NodeKind::Literal => node.params.clone(),
        NodeKind::PdfStreamDecoded => {
            let dep = node
                .deps
                .first()
                .ok_or_else(|| Error::usage("PdfStreamDecoded has no dependency"))?;
            let child = load_node(store, dep)?;
            let encoded = materialize_node(parsed, store, &child, limits, budget, depth - 1)?;
            derive::inflate_zlib(&encoded, node.logical_output_len, limits)?
        }
        NodeKind::ContentOperators => {
            let dep = node
                .deps
                .first()
                .ok_or_else(|| Error::usage("ContentOperators has no dependency"))?;
            let child = load_node(store, dep)?;
            let decoded = materialize_node(parsed, store, &child, limits, budget, depth - 1)?;
            derive::content_operators(&decoded, limits)?
        }
        NodeKind::TextRuns => {
            let dep = node
                .deps
                .first()
                .ok_or_else(|| Error::usage("TextRuns has no dependency"))?;
            let child = load_node(store, dep)?;
            let ops = materialize_node(parsed, store, &child, limits, budget, depth - 1)?;
            derive::text_runs(&ops, limits)?
        }
        NodeKind::PagePreview => {
            let dep = node
                .deps
                .first()
                .ok_or_else(|| Error::usage("PagePreview has no dependency"))?;
            let child = load_node(store, dep)?;
            let content = materialize_node(parsed, store, &child, limits, budget, depth - 1)?;
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
