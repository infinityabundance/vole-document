//! Immutable node-level edit of a procedural field (Phase 11.12).
//!
//! ## The shape of the witness
//!
//! A field is not a stack of unrelated whole-document snapshots: it is a
//! content-addressed procedural **trajectory**. [`replace_page_content`] makes
//! that concrete. Given an already-ingested, indexed field `R0` and a page `p`,
//! it installs caller-supplied bytes as the new decoded content of exactly that
//! page and returns a new root `R1` that:
//!
//! * reuses the exact same `.voldoc` descriptor blob and the same
//!   `DocumentExact` root node id as `R0` (`descriptor_id` and `root_node` are
//!   copied verbatim), so `R0` and `R1` both materialize the *same* original
//!   bytes and neither weakens `materialize(root) == original_bytes`;
//! * replaces only the `SEL_PAGE(p)` binding in the hierarchical observation
//!   index with a new `PageContent` node whose single dependency is a raw
//!   `Literal` holding the new content bytes;
//! * carries **every other** selector binding forward by content id, so the new
//!   index tree shares all unchanged leaf nodes with `R0` (content addressing
//!   makes an unchanged leaf hash to an unchanged id; only the leaf that holds
//!   `p` and the internal spine above it are rewritten);
//! * and never reads the descriptor blob, never re-parses the document, and
//!   never re-bakes the old source.
//!
//! The derived per-page projections (`ContentOperators`, `TextRuns`,
//! `PagePreview`) are then recomputed lazily from the new content by the ordinary
//! observation path, unchanged.
//!
//! ## What is shared vs copied, stated exactly
//!
//! **Shared by id** (never re-written): the descriptor blob, the `DocumentExact`
//! root, every unaffected `PageContent`/`PdfObject`/`PdfStreamEncoded`/
//! `PdfStreamDecoded`/`PdfRevision` seed node, and every index leaf that does not
//! contain the edited page's selector.
//!
//! **Copied/newly written**: two new seed nodes (the `Literal` and its
//! `PageContent`), the index leaf containing `SEL_PAGE(p)` and the internal
//! spine from that leaf to a new root, and one new manifest.
//!
//! ## The honest supported subset
//!
//! This is **not** generic document editing. It is a single, narrow operation:
//! *decode-level content override of one existing page of an indexed field*.
//! The exact archive is untouched, so this is a *procedural* edit, not a rewrite
//! of the PDF. It makes **no** authorial-intent claim, offers no insert/delete/
//! reorder, no cross-reference or object-graph mutation, no re-encoding, and no
//! change to any other page. The caller-supplied content is bounded by
//! [`MAX_EDIT_CONTENT_BYTES`] (it must fit one canonical seed node). Any page
//! whose content was overridden reports no source byte span in its index entry
//! and no content-stream object numbers in its `structure` observation, because
//! the new bytes are not present in the source; `text` and `preview` projections
//! are computed from the new bytes as usual.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{Error, Result};
use crate::field::index::{self, FsIndexStore, SEL_PAGE, SelectorKey};
use crate::field::node::{MAX_NODE_BYTES, NodeKind, SeedNode, u32_params};
use crate::field::{FieldId, FieldStore};
use crate::store::{IoSnapshot, NodeId};

/// Maximum caller-supplied content bytes one edit may install.
///
/// The bytes live in a `Literal` node's parameters, so the canonical node must
/// fit [`MAX_NODE_BYTES`]. The margin covers the fixed node header (magic,
/// version, kind, materializer, lengths, limits) and the provenance string.
pub const MAX_EDIT_CONTENT_BYTES: usize = 48 * 1024;

/// Seed nodes one edit always declares for the edited page: the `Literal` and
/// its `PageContent`. This is a fixed count (not the number physically written),
/// so an identical repeated edit hashes to the identical manifest.
const EDIT_SEED_NODES: u64 = 2;

/// What one immutable edit produced.
///
/// Every counter is a measured fact about *this* call; none of them is a
/// claim about the document's semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditReport {
    /// The new field root `R1`.
    pub field: FieldId,
    /// The field that was edited `R0` (left byte-valid and unchanged).
    pub previous: FieldId,
    /// The edited page (1-based, as in the observation index).
    pub page: u32,
    /// The new `PageContent` node that serves `SEL_PAGE(page)` in `R1`.
    pub page_content: NodeId,
    /// The `Literal` node holding the caller-supplied content bytes.
    pub content_literal: NodeId,
    /// The `R1` hierarchical index root.
    pub index_root: NodeId,
    /// Total selector bindings in the `R1` index.
    pub index_entries: u64,
    /// Bindings carried forward unchanged (same key, same node id) from `R0`.
    pub index_entries_reused: u64,
    /// Bindings whose node id changed (the edited page).
    pub index_entries_replaced: u64,
    /// New seed nodes written (checked against the store before writing).
    pub seed_nodes_new: u64,
    /// Seed nodes the edit intended to write that already existed by id.
    pub seed_nodes_reused: u64,
    /// `R1` index-tree nodes that already existed before the edit (so they were
    /// not re-written): `R0`'s own tree nodes and any leaf an earlier edit left
    /// behind.
    pub index_nodes_reused: u64,
    /// `R1` index-tree nodes the edit physically wrote for the first time.
    pub index_nodes_new: u64,
    /// Canonical payload bytes the edit actually had to persist: the new seed
    /// nodes, the new index nodes, and the new manifest, each counted only when
    /// it was not already present by content id. It excludes filesystem framing,
    /// `.tmp` writes, and `fsync`.
    pub bytes_newly_persisted: u64,
    /// Descriptor-blob bytes the edit read (zero by construction).
    pub descriptor_bytes_read: u64,
    /// Field-manifest bytes the edit read.
    pub manifest_bytes_read: u64,
    /// Hierarchical-index bytes the edit read.
    pub index_bytes_read: u64,
    /// Seed-node bytes the edit read.
    pub seed_bytes_read: u64,
}

/// Replace one page's decoded content with `new_content`, returning a new field
/// root; the input field is never mutated.
///
/// See the module documentation for the exact supported subset. Fails closed
/// (typed error, no partial write to any *new* root) when the field has no
/// index, when the page is absent, or when the content exceeds
/// [`MAX_EDIT_CONTENT_BYTES`].
pub fn replace_page_content(
    store: &mut FieldStore,
    field: &FieldId,
    page: u32,
    new_content: &[u8],
) -> Result<EditReport> {
    if page == 0 {
        return Err(Error::usage("edit page numbers are 1-based"));
    }
    if new_content.len() > MAX_EDIT_CONTENT_BYTES {
        return Err(Error::resource_limit(format!(
            "edit content is {} bytes > {MAX_EDIT_CONTENT_BYTES}",
            new_content.len()
        )));
    }
    let io_before = store.io().snapshot();
    let io_handle = store.io().handle();
    let previous = *field;

    // Read the manifest only: no descriptor blob, no document parse.
    let manifest = store.get_field(field)?;
    if !manifest.has_index() {
        return Err(Error::unsupported_feature(
            "immutable edit requires an indexed field (ingest with a recovered index)",
        ));
    }
    let old_root = NodeId::from_bytes(manifest.index_root);
    let istore = FsIndexStore::open_with_io(store.root(), io_handle.clone())?;
    let before = index::inspect(&istore, &old_root)?;
    // Every index node that already exists physically (R0's tree plus any
    // orphan from a prior edit), so "reused/new" is a physical fact.
    let preexisting: BTreeSet<NodeId> = istore.list_ids()?.into_iter().collect();
    let old_by_key: BTreeMap<SelectorKey, NodeId> =
        before.entries.iter().map(|e| (e.key, e.node_id)).collect();

    let page_key = SelectorKey::new(SEL_PAGE, page);
    if !old_by_key.contains_key(&page_key) {
        return Err(Error::usage(format!(
            "field {previous} has no page {page} in its index"
        )));
    }

    // Build the two new seed nodes. The literal carries the bytes; the
    // page-content node is a one-dependency exact concatenation of them, so the
    // ordinary `PageContent` materializer serves the new bytes unchanged.
    let literal = SeedNode::new(
        NodeKind::Literal,
        new_content.len() as u64,
        new_content.to_vec(),
        Vec::new(),
        format!("field:edit;literal;page={page}"),
    );
    let literal_canonical = literal.encode_canonical();
    if literal_canonical.len() > MAX_NODE_BYTES {
        return Err(Error::resource_limit(format!(
            "edit content encodes to {} bytes > the {MAX_NODE_BYTES}-byte node framing limit",
            literal_canonical.len()
        )));
    }
    let literal_id = literal.content_id();
    let page_content = SeedNode::new(
        NodeKind::PageContent,
        new_content.len() as u64,
        u32_params(page),
        vec![literal_id],
        format!("field:edit;page-content;page={page}"),
    );
    let page_content_id = page_content.content_id();

    let mut seed_nodes_new = 0u64;
    let mut seed_nodes_reused = 0u64;
    let mut bytes_newly_persisted = 0u64;
    for node in [&literal, &page_content] {
        let canonical = node.encode_canonical();
        let id = NodeId::of_node(&canonical);
        if store.seeds().contains_node(&id)? {
            seed_nodes_reused = seed_nodes_reused.saturating_add(1);
        } else {
            seed_nodes_new = seed_nodes_new.saturating_add(1);
            bytes_newly_persisted = bytes_newly_persisted.saturating_add(canonical.len() as u64);
        }
        store.seeds_mut().put_node(&canonical)?;
    }

    // Carry every binding forward by id, replacing only the edited page. The
    // replaced entry loses its source span: the new bytes are not in the source.
    let mut entries = before.entries.clone();
    let mut index_entries_reused = 0u64;
    let mut index_entries_replaced = 0u64;
    for e in entries.iter_mut() {
        if e.key == page_key {
            e.node_id = page_content_id;
            e.out_off = 0;
            e.out_len = 0;
            index_entries_replaced = index_entries_replaced.saturating_add(1);
        } else if old_by_key.get(&e.key).copied() == Some(e.node_id) {
            index_entries_reused = index_entries_reused.saturating_add(1);
        }
    }

    let mut new_istore = FsIndexStore::open_with_io(store.root(), io_handle)?;
    let new_root = index::build(&mut new_istore, &entries)?;
    let (index_node_count, _depth, after_nodes) = index::validate_nodes(&new_istore, &new_root)?;
    let index_nodes_reused = after_nodes
        .iter()
        .filter(|id| preexisting.contains(id))
        .count() as u64;
    let index_nodes_new = after_nodes.len() as u64 - index_nodes_reused;
    for id in &after_nodes {
        if !preexisting.contains(id) {
            bytes_newly_persisted =
                bytes_newly_persisted.saturating_add(new_istore.get(id)?.len() as u64);
        }
    }

    // The new manifest changes only the index binding and counts; the descriptor
    // id, the `DocumentExact` root, the universe, and the declared source are
    // copied verbatim, which is what keeps `R1` exact for the original bytes.
    let mut new_manifest = manifest.clone();
    new_manifest.index_root = *new_root.as_bytes();
    new_manifest.index_node_count = index_node_count;
    new_manifest.node_count = manifest.node_count.saturating_add(EDIT_SEED_NODES);
    new_manifest.provenance = format!("field:edit;page={page};prev={previous}");
    let manifest_bytes = new_manifest.encode_canonical();
    let new_id = new_manifest.content_id();
    if !manifest_exists(store, &new_id)? {
        bytes_newly_persisted = bytes_newly_persisted.saturating_add(manifest_bytes.len() as u64);
    }
    store.put_field(&new_manifest)?;

    let io = io_before.delta(&store.io().snapshot());
    Ok(EditReport {
        field: new_id,
        previous,
        page,
        page_content: page_content_id,
        content_literal: literal_id,
        index_root: new_root,
        index_entries: entries.len() as u64,
        index_entries_reused,
        index_entries_replaced,
        seed_nodes_new,
        seed_nodes_reused,
        index_nodes_reused,
        index_nodes_new,
        bytes_newly_persisted,
        descriptor_bytes_read: io.descriptor_bytes,
        manifest_bytes_read: io.manifest_bytes,
        index_bytes_read: io.index_bytes,
        seed_bytes_read: io.seed_bytes,
    })
}

/// The raw physical-I/O accounting of an interval, re-exported for callers that
/// want to attribute an edit without unpacking the report.
pub fn io_of(report: &EditReport) -> IoSnapshot {
    IoSnapshot {
        descriptor_bytes: report.descriptor_bytes_read,
        manifest_bytes: report.manifest_bytes_read,
        index_bytes: report.index_bytes_read,
        seed_bytes: report.seed_bytes_read,
    }
}

/// Whether a manifest id is already present, without paying a manifest read on
/// the filesystem backend (where a manifest is one file). The EntropyFS backend
/// has no enumeration, so it falls back to a counted fetch.
fn manifest_exists(store: &FieldStore, id: &FieldId) -> Result<bool> {
    let path = store.root().join("field").join(id.to_hex());
    if path.exists() {
        return Ok(true);
    }
    Ok(store.get_field(id).is_ok())
}
