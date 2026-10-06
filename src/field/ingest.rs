//! Progressive early inverse-proceduralization of a PDF field (Phase 11.4).
//!
//! [`ingest_pdf`] runs Stage A (durable exact capture, delegated to
//! [`FieldStore::ingest`]) followed by Stage B (cheap eager inversion): exact
//! physical spans become `PdfObject` / `PdfRevision` / `PdfStreamEncoded` nodes,
//! a lone-`/FlateDecode` stream is inflated once to learn its exact decoded
//! length and becomes a `PdfStreamDecoded` node (an unfiltered stream's encoded
//! bytes are used directly as content), and a bounded page-tree walk builds
//! `PageContent` nodes. Page keys (`/Type`, `/Kids`, `/Contents`, `/Pages`) are
//! read only at the leading dictionary's top level, so a nested sub-dictionary
//! cannot shadow them. Every recovered observation is registered in the
//! hierarchical index and the manifest is re-written with the richer root.
//!
//! Stage B is **best-effort and never fatal**: a heuristic step that declines
//! (an unparseable scan, a non-inflatable stream, an ambiguous page) only lowers
//! the recovered depth; it never fails the ingest or weakens exactness. Stage A
//! remains the sole archival authority, so `materialize_exact` is byte-identical
//! regardless of what Stage B recovered.
//!
//! Stage C ([`deepen_page`]) is demand-driven: it adds `ContentOperators`,
//! `TextRuns`, and `PagePreview` nodes beneath an existing `PageContent` node and
//! writes a *new* manifest (`DEC-7`: promotion is additive). The older field id
//! keeps materializing exactly, so promotion is monotonic.
//!
//! Nothing here serializes known structure and reparses it (plan §13): the page
//! tree and content references are read directly from the Phase-3 lexical cover,
//! and a decoded stream is never re-DEFLATEd to be re-inflated.

use std::collections::{BTreeMap, BTreeSet};

use crate::adapter::pdf::cos::FilterClass;
use crate::adapter::pdf::lexer::lex;
use crate::adapter::pdf::physical::{PdfPhysical, scan};
use crate::adapter::pdf::span::{Span, SpanKind};
use crate::container::observation::{ObservationIndex, OpEntry, SECTION_OP_TABLE};
use crate::container::{Descriptor, ParsedDescriptor};
use crate::error::{Error, Result};
use crate::field::dag;
use crate::field::index::{
    FsIndexStore, IndexEntry, SEL_OBJECT, SEL_PAGE, SEL_REVISION, SEL_STREAM, SEL_STREAM_DECODED,
    SelectorKey, build, lookup, validate,
};
use crate::field::manifest::FieldRoot;
use crate::field::node::{MAX_NODE_DEPS, NodeKind, SeedNode, object_params, u32_params};
use crate::field::{Field, FieldId, FieldStore};
use crate::limits::Limits;
use crate::store::NodeId;

/// Hard cap on seed nodes one ingest may add.
pub const MAX_INGEST_NODES: u64 = 1 << 20;
/// Hard cap on index entries one ingest may add.
pub const MAX_INGEST_INDEX_ENTRIES: usize = 1 << 20;
/// Maximum decoded length admitted for a single stream (32 MiB).
pub const MAX_STREAM_DECODE: u64 = 32 * 1024 * 1024;
/// Maximum total decoded bytes admitted across one ingest (512 MiB).
pub const MAX_TOTAL_DECODED: u64 = 512 * 1024 * 1024;
/// Maximum number of page-tree objects visited during the Stage-B walk.
const MAX_PAGE_TREE_NODES: usize = 1 << 16;
/// Maximum number of references gathered from one `/Kids` or `/Contents`.
const MAX_REFS: usize = 1 << 16;
/// Maximum number of object streams indexed in one page-recovery pass.
const MAX_OBJSTM: usize = 1 << 12;
/// Maximum `/N` (object count) admitted from one object stream.
const MAX_OBJSTM_OBJECTS: usize = 1 << 16;
/// Maximum total decoded object-stream bytes retained for page recovery (64 MiB).
const MAX_OBJSTM_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum pages recovered in one page-tree walk.
const MAX_PAGES: usize = 1 << 16;

/// What one ingest recovered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestReport {
    /// The (richest) field id, whose manifest binds the recovered index.
    pub field: FieldId,
    /// The exact `DocumentExact` root node id.
    pub root_node: NodeId,
    /// The hierarchical index root, or `None` when no index was built.
    pub index_root: Option<NodeId>,
    /// Seed nodes in the manifest closure at ingest time (root + Stage-B nodes).
    pub node_count: u64,
    /// Number of index nodes in the built tree.
    pub index_node_count: u64,
    /// Exact reconstructed source length.
    pub source_len: u64,
    /// Exact per-object nodes created.
    pub object_nodes: u64,
    /// Encoded stream nodes created.
    pub stream_nodes: u64,
    /// Decoded stream nodes created.
    pub decoded_stream_nodes: u64,
    /// Page content nodes created.
    pub page_nodes: u64,
    /// Revision nodes created.
    pub revision_nodes: u64,
    /// Lone-`FlateDecode` streams we refused to inflate.
    pub declined_streams: u64,
}

/// Stage A (durable exact capture) + Stage B (cheap eager inversion).
pub fn ingest_pdf(
    store: &mut FieldStore,
    descriptor_bytes: &[u8],
    limits: Limits,
) -> Result<IngestReport> {
    // Stage A: the exact descriptor blob, the DocumentExact root, and a manifest.
    // The stored blob gains a minimal advisory observation-index op table when it
    // lacks one, so a later narrow observation can use the seek-based partial
    // lane. Exactness is unchanged; only the ignorable record is added.
    let observable = with_observation_index(descriptor_bytes, limits)?;
    let base_id = store.ingest(&observable, limits)?;
    let (manifest, source) = {
        let field = Field::open(store, &base_id, limits)?;
        (field.manifest().clone(), field.materialize_exact(limits)?)
    };
    let source_len = source.len() as u64;

    let mut acc = StageB::new(manifest.node_count);
    let scanned = match scan(&source, limits) {
        Ok(physical) => {
            run_stage_b(store, &source, &physical, limits, &mut acc)?;
            true
        }
        Err(_) => false,
    };

    let (index_root, index_node_count, provenance) = if scanned && !acc.entries.is_empty() {
        let mut istore = FsIndexStore::open(store.root())?;
        let root = build(&mut istore, &acc.entries)?;
        let (count, _depth) = validate(&istore, &root)?;
        let prov = format!(
            "field:ingest-b;objects={};streams={};decoded={};pages={};revs={}",
            acc.object_nodes,
            acc.stream_nodes,
            acc.decoded_stream_nodes,
            acc.page_nodes,
            acc.revision_nodes
        );
        (Some(root), count, prov)
    } else if scanned {
        (None, 0, "field:ingest-b;entries=0".to_string())
    } else {
        (None, 0, "field:ingest-b;declined=scan".to_string())
    };

    let mut new_manifest = manifest.clone();
    if let Some(root) = &index_root {
        new_manifest.index_root = *root.as_bytes();
    }
    new_manifest.node_count = acc.node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        field,
        root_node: manifest.root_node,
        index_root,
        node_count: acc.node_count,
        index_node_count,
        source_len,
        object_nodes: acc.object_nodes,
        stream_nodes: acc.stream_nodes,
        decoded_stream_nodes: acc.decoded_stream_nodes,
        page_nodes: acc.page_nodes,
        revision_nodes: acc.revision_nodes,
        declined_streams: acc.declined_streams,
    })
}

/// Add a minimal observation-index op table to a descriptor blob that lacks one,
/// so the seek-based partial lane is available to narrow observations.
///
/// This never changes the reconstruction program, objects, channels, or declared
/// source: it adds only the ignorable `OBSERVATION_INDEX` record whose op table is
/// derived from [`Program::analyze_ops`] and re-validated by
/// [`Descriptor::parse`] when the enriched blob is stored. When the descriptor
/// already carries an index, or an op length does not fit the index's `u32`
/// field, the input is returned unchanged (the observation then uses the full
/// descriptor path).
fn with_observation_index(bytes: &[u8], limits: Limits) -> Result<Vec<u8>> {
    let parsed: ParsedDescriptor = Descriptor::parse(bytes, limits)?;
    if parsed.descriptor.observation_index.is_some() {
        return Ok(bytes.to_vec());
    }
    let d = parsed.descriptor;
    let object_lens: Vec<u64> = d.objects.iter().map(|o| o.len()).collect();
    let channel_lens: Vec<u64> = d.channels.iter().map(|c| c.decoded_length).collect();
    let per_op = match d.program.analyze_ops(&object_lens, &channel_lens, limits) {
        Ok(v) => v,
        Err(_) => return Ok(bytes.to_vec()),
    };
    let mut ops: Vec<OpEntry> = Vec::with_capacity(per_op.len());
    for (i, len) in per_op.iter().enumerate() {
        let Ok(out_len) = u32::try_from(*len) else {
            return Ok(bytes.to_vec());
        };
        let (dep_kind, dep_id) =
            crate::container::observation::primary_dependency(&d.program.ops[i]);
        ops.push(OpEntry {
            out_len,
            dep_kind,
            dep_id,
        });
    }
    let mut enriched = d;
    enriched.observation_index = Some(ObservationIndex {
        section_flags: SECTION_OP_TABLE,
        ops,
        selectors: Vec::new(),
        digests: Vec::new(),
    });
    match enriched.serialize() {
        Ok((out, _cost)) => Ok(out),
        Err(_) => Ok(bytes.to_vec()),
    }
}

/// Stage C: add decoded-stream operators/text for one page, returning a new
/// field id. Idempotent: repeating on an already-promoted field returns it
/// unchanged, and the original field id keeps materializing exactly.
pub fn deepen_page(
    store: &mut FieldStore,
    field: &FieldId,
    page: u32,
    limits: Limits,
) -> Result<FieldId> {
    let manifest = Field::open(store, field, limits)?.manifest().clone();
    deepen_page_with_manifest(store, &manifest, page)
}

/// Stage C against an already-loaded manifest, so an observation that already
/// holds the parsed field does not re-read the descriptor blob just to promote a
/// page (review fix #2: no hidden double descriptor load).
pub fn deepen_page_with_manifest(
    store: &mut FieldStore,
    manifest: &FieldRoot,
    page: u32,
) -> Result<FieldId> {
    let field = manifest.content_id();
    if !manifest.has_index() {
        return Ok(field);
    }
    // Already promoted for this page: idempotent no-op.
    if manifest.provenance == format!("field:deepen;page={page}") {
        return Ok(field);
    }

    let istore = FsIndexStore::open(store.root())?;
    let root = NodeId::from_bytes(manifest.index_root);
    let found = lookup(&istore, &root, &SelectorKey::new(SEL_PAGE, page))?;
    let Some(entry) = found.first() else {
        return Ok(field);
    };
    let page_content_id = entry.node_id;
    let page_content = dag::load_node(store.seeds(), &page_content_id)?;
    if page_content.kind != NodeKind::PageContent {
        return Ok(field);
    }

    // Add only nodes: the page's SEL_PAGE entry keeps its exact semantics.
    let (ops, text, preview) = derived_chain(page, page_content_id);
    store.seeds_mut().put_node(&ops.encode_canonical())?;
    store.seeds_mut().put_node(&text.encode_canonical())?;
    store.seeds_mut().put_node(&preview.encode_canonical())?;

    let mut new_manifest = manifest.clone();
    new_manifest.node_count = manifest.node_count.saturating_add(3);
    new_manifest.provenance = format!("field:deepen;page={page}");
    store.put_field(&new_manifest)
}

/// The deterministic Stage-C chain beneath a page's `PageContent` node.
fn derived_chain(page: u32, page_content_id: NodeId) -> (SeedNode, SeedNode, SeedNode) {
    let ops = SeedNode::new(
        NodeKind::ContentOperators,
        0,
        u32_params(page),
        vec![page_content_id],
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
        vec![page_content_id],
        "pdf:page-preview",
    );
    (ops, text, preview)
}

/// The deterministic `PdfStreamDecoded` node for an encoded stream node.
fn stream_decoded_node(object: u32, encoded_id: NodeId, decoded_len: u64) -> SeedNode {
    SeedNode::new(
        NodeKind::PdfStreamDecoded,
        decoded_len,
        u32_params(object),
        vec![encoded_id],
        "pdf:stream-decoded",
    )
}

/// Mutable Stage-B accumulator.
struct StageB {
    entries: Vec<IndexEntry>,
    decoded_by_object: BTreeMap<u32, (NodeId, u64)>,
    /// Encoded node for streams with *no* `/Filter`, whose bytes are the raw
    /// content (used to recover pages with unfiltered content streams).
    plain_by_object: BTreeMap<u32, (NodeId, u64)>,
    node_count: u64,
    object_nodes: u64,
    stream_nodes: u64,
    decoded_stream_nodes: u64,
    page_nodes: u64,
    revision_nodes: u64,
    declined_streams: u64,
    total_decoded: u64,
}

impl StageB {
    fn new(node_count: u64) -> Self {
        StageB {
            entries: Vec::new(),
            decoded_by_object: BTreeMap::new(),
            plain_by_object: BTreeMap::new(),
            node_count,
            object_nodes: 0,
            stream_nodes: 0,
            decoded_stream_nodes: 0,
            page_nodes: 0,
            revision_nodes: 0,
            declined_streams: 0,
            total_decoded: 0,
        }
    }

    fn put(&mut self, store: &mut FieldStore, node: &SeedNode) -> Result<NodeId> {
        if self.node_count >= MAX_INGEST_NODES {
            return Err(Error::resource_limit(format!(
                "ingest would exceed {MAX_INGEST_NODES} seed nodes"
            )));
        }
        let id = store.seeds_mut().put_node(&node.encode_canonical())?;
        self.node_count += 1;
        Ok(id)
    }

    fn add_entry(&mut self, entry: IndexEntry) -> Result<()> {
        if self.entries.len() >= MAX_INGEST_INDEX_ENTRIES {
            return Err(Error::resource_limit(format!(
                "ingest would exceed {MAX_INGEST_INDEX_ENTRIES} index entries"
            )));
        }
        self.entries.push(entry);
        Ok(())
    }
}

/// Stage B: exact spans, decoded streams, and the bounded page tree.
fn run_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    physical: &PdfPhysical,
    limits: Limits,
    acc: &mut StageB,
) -> Result<()> {
    // Exact indirect-object spans.
    for obj in &physical.objects {
        if obj.number == 0 {
            continue;
        }
        let (Some((number, generation)), Some((offset, len))) = (
            identity32(obj.number, obj.generation),
            span32(obj.start, obj.end.saturating_sub(obj.start)),
        ) else {
            continue;
        };
        let node = SeedNode::new(
            NodeKind::PdfObject,
            len,
            object_params(number, generation, (offset << 32) | len),
            Vec::new(),
            "pdf:object",
        );
        let id = acc.put(store, &node)?;
        acc.object_nodes += 1;
        acc.add_entry(IndexEntry {
            key: SelectorKey::new(SEL_OBJECT, number),
            out_off: offset,
            out_len: len,
            node_id: id,
        })?;
    }

    // Exact revision spans.
    for rev in &physical.revisions {
        let Some((offset, len)) = span32(rev.start, rev.end.saturating_sub(rev.start)) else {
            continue;
        };
        let node = SeedNode::new(
            NodeKind::PdfRevision,
            len,
            object_params(rev.index, 0, (offset << 32) | len),
            Vec::new(),
            "pdf:revision",
        );
        let id = acc.put(store, &node)?;
        acc.revision_nodes += 1;
        acc.add_entry(IndexEntry {
            key: SelectorKey::new(SEL_REVISION, rev.index),
            out_off: offset,
            out_len: len,
            node_id: id,
        })?;
    }

    // Encoded stream spans, plus one eager decode for a lone Flate stream.
    for stream in &physical.streams {
        let (Some((number, generation)), Some((offset, len))) = (
            identity32(stream.object, stream.generation),
            span32(stream.data_start, stream.data_len),
        ) else {
            continue;
        };
        let encoded = SeedNode::new(
            NodeKind::PdfStreamEncoded,
            len,
            object_params(number, generation, (offset << 32) | len),
            Vec::new(),
            "pdf:stream-encoded",
        );
        let encoded_id = acc.put(store, &encoded)?;
        acc.stream_nodes += 1;
        acc.add_entry(IndexEntry {
            key: SelectorKey::new(SEL_STREAM, number),
            out_off: offset,
            out_len: len,
            node_id: encoded_id,
        })?;

        // An unfiltered stream's encoded bytes *are* its content: remember the
        // encoded node so a page whose `/Contents` is unfiltered can use it.
        if stream.filter == FilterClass::Absent {
            acc.plain_by_object.insert(number, (encoded_id, len));
        }
        if stream.filter != FilterClass::FlateDecode {
            continue;
        }
        match try_decode_len(source, stream.data_start, stream.data_len, limits) {
            Some(decoded_len)
                if acc
                    .total_decoded
                    .checked_add(decoded_len)
                    .is_some_and(|total| total <= MAX_TOTAL_DECODED) =>
            {
                let decoded = stream_decoded_node(number, encoded_id, decoded_len);
                let decoded_id = acc.put(store, &decoded)?;
                acc.decoded_stream_nodes += 1;
                acc.total_decoded += decoded_len;
                acc.decoded_by_object
                    .insert(number, (decoded_id, decoded_len));
                // Register the decoded node so a later `Stream(n) + DecodedBytes`
                // observation resolves in O(depth) index reads, not by scanning
                // the seed store (review fix #3). The span is the encoded
                // stream's exact source span the node is derived from.
                acc.add_entry(IndexEntry {
                    key: SelectorKey::new(SEL_STREAM_DECODED, number),
                    out_off: offset,
                    out_len: len,
                    node_id: decoded_id,
                })?;
            }
            _ => acc.declined_streams += 1,
        }
    }

    recover_pages(store, source, physical, limits, acc)
}

/// Best-effort page-tree recovery: `/Root` catalog → `/Pages` → `/Kids` → `/Page`.
///
/// Object bodies are read from the exact physical source *or* from a decoded
/// `/ObjStm` buffer, so a producer that keeps its whole page tree inside an
/// object stream (pdfTeX) still recovers its pages. Physical objects shadow
/// object-stream objects of the same number, and every key read is
/// top-level-dictionary-depth-1 correct so a nested `/Type` cannot shadow one.
fn recover_pages(
    store: &mut FieldStore,
    source: &[u8],
    physical: &PdfPhysical,
    limits: Limits,
    acc: &mut StageB,
) -> Result<()> {
    if physical.objects.is_empty() {
        return Ok(());
    }
    let lexed = match lex(source, limits) {
        Ok(lexed) => lexed,
        Err(_) => return Ok(()),
    };
    let spans = lexed.spans.spans;

    // Object number -> object index (a later revision shadows an earlier one).
    let mut obj_index: BTreeMap<u64, usize> = BTreeMap::new();
    for (i, obj) in physical.objects.iter().enumerate() {
        obj_index.insert(obj.number, i);
    }

    // The leading dict/array range of every physical object.
    let mut containers: Vec<Option<(bool, u64, u64)>> = Vec::with_capacity(physical.objects.len());
    for obj in &physical.objects {
        containers.push(leading_container(span_window(&spans, obj.start, obj.end)));
    }

    // Streams already decoded by Stage B (needed to read an `/ObjStm`'s bytes).
    // Taken out of `acc` so the walk below can mutate it without a borrow clash.
    let decoded_by_object = std::mem::take(&mut acc.decoded_by_object);
    let plain_by_object = std::mem::take(&mut acc.plain_by_object);

    // Index object streams: map each contained object number to its body range
    // inside the retained decoded buffer. A stream with no decoded node is
    // skipped outright -- the bytes are never guessed at.
    let mut buffers: Vec<ObjStmBuf> = Vec::new();
    let mut objstm: BTreeMap<u64, Resolved> = BTreeMap::new();
    let mut total_objstm: u64 = 0;
    for stream in &physical.streams {
        if buffers.len() >= MAX_OBJSTM {
            break;
        }
        let Ok(number32) = u32::try_from(stream.object) else {
            continue;
        };
        let Some(&(_node, decoded_len)) = decoded_by_object.get(&number32) else {
            continue;
        };
        let Some(&idx) = obj_index.get(&stream.object) else {
            continue;
        };
        let Some((is_array, lo, hi)) = containers[idx] else {
            continue;
        };
        if is_array {
            continue;
        }
        let win = span_window(&spans, lo, hi);
        if top_level_name_value(source, win, lo, hi, b"Type") != Some(&b"ObjStm"[..]) {
            continue;
        }
        let (Some(start), Some(end)) = (
            usize::try_from(stream.data_start).ok(),
            stream
                .data_start
                .checked_add(stream.data_len)
                .and_then(|e| usize::try_from(e).ok()),
        ) else {
            continue;
        };
        let Some(encoded) = source.get(start..end) else {
            continue;
        };
        let Ok(decoded) = crate::field::derive::inflate_zlib(encoded, decoded_len, limits) else {
            continue;
        };
        if decoded.is_empty() {
            continue;
        }
        let Some(total) = total_objstm.checked_add(decoded.len() as u64) else {
            continue;
        };
        if total > MAX_OBJSTM_BYTES {
            continue;
        }
        let Some(n) = top_level_integer_value(source, win, lo, hi, b"N") else {
            continue;
        };
        if n == 0 || n > MAX_OBJSTM_OBJECTS as u64 {
            continue;
        }
        let first = top_level_integer_value(source, win, lo, hi, b"First");
        let Some(pairs) = parse_objstm_header(&decoded, first, n as usize) else {
            continue;
        };
        let Ok(decoded_lexed) = lex(&decoded, limits) else {
            continue;
        };
        let buf_spans = decoded_lexed.spans.spans;
        // Per spec the pair offsets are relative to `/First` (the header end);
        // when `/First` is absent the offsets are treated as absolute.
        let base = first.unwrap_or(0);
        let buf_index = buffers.len();
        let len = decoded.len() as u64;
        let mut entries: Vec<(u64, Resolved)> = Vec::with_capacity(pairs.len());
        for (i, &(object, offset)) in pairs.iter().enumerate() {
            if object == 0 {
                continue;
            }
            let Some(body_lo) = base.checked_add(offset) else {
                continue;
            };
            let body_hi = pairs
                .get(i + 1)
                .and_then(|&(_, next)| base.checked_add(next))
                .filter(|&next| next >= body_lo && next <= len)
                .unwrap_or(len);
            if body_lo > body_hi || body_hi > len {
                continue;
            }
            let Some((body_is_array, clo, chi)) =
                leading_container(span_window(&buf_spans, body_lo, body_hi))
            else {
                continue;
            };
            if clo < body_lo || chi > body_hi {
                continue;
            }
            entries.push((
                object,
                Resolved {
                    src: Src::ObjStm(buf_index),
                    is_array: body_is_array,
                    lo: clo,
                    hi: chi,
                },
            ));
        }
        buffers.push(ObjStmBuf {
            bytes: decoded,
            spans: buf_spans,
        });
        total_objstm = total;
        for (object, resolved) in entries {
            objstm.insert(object, resolved);
        }
    }

    // Unified resolver: physical objects shadow object-stream objects.
    let physical_numbers: BTreeSet<u64> = physical
        .objects
        .iter()
        .map(|o| o.number)
        .filter(|&n| n != 0)
        .collect();
    let mut map: BTreeMap<u64, Resolved> = BTreeMap::new();
    for (i, obj) in physical.objects.iter().enumerate() {
        if obj.number == 0 {
            continue;
        }
        if let Some((is_array, lo, hi)) = containers[i] {
            map.insert(
                obj.number,
                Resolved {
                    src: Src::Physical,
                    is_array,
                    lo,
                    hi,
                },
            );
        }
    }
    for (object, resolved) in objstm {
        if !physical_numbers.contains(&object) {
            map.insert(object, resolved);
        }
    }
    let resolver = ObjResolver {
        source,
        source_spans: &spans,
        buffers: &buffers,
        map,
    };

    // The catalog is the lowest-numbered object whose *top-level* `/Type` is
    // `/Catalog`, whether it lives in the physical source or an object stream.
    let Some(catalog) = resolver
        .map
        .values()
        .find(|&&res| resolver.is_type(res, b"Catalog"))
        .copied()
    else {
        return Ok(());
    };
    let Some(pages_refs) = resolver.key_refs(catalog, b"Pages") else {
        return Ok(());
    };
    let Some(&pages_root) = pages_refs.first() else {
        return Ok(());
    };

    // Bounded depth-first walk in page order.
    let mut stack = vec![pages_root];
    let mut visited: BTreeSet<u64> = BTreeSet::new();
    let mut pages: Vec<u64> = Vec::new();
    while let Some(number) = stack.pop() {
        if !visited.insert(number) {
            continue;
        }
        if visited.len() > MAX_PAGE_TREE_NODES {
            break;
        }
        let Some(res) = resolver.resolve(number) else {
            continue;
        };
        if res.is_array {
            continue;
        }
        if resolver.is_type(res, b"Page") {
            if pages.len() < MAX_PAGES {
                pages.push(number);
            }
        } else if let Some(kids) = resolver.key_refs(res, b"Kids") {
            // A `/Pages` node, or a dict with no usable `/Type` that still
            // carries `/Kids`: both are internal page-tree nodes. Best-effort.
            let kids = resolver.expand(kids);
            for kid in kids.into_iter().rev() {
                if !visited.contains(&kid) {
                    stack.push(kid);
                }
            }
        }
    }

    let mut page_number: u32 = 0;
    for page_obj in pages {
        let Some(res) = resolver.resolve(page_obj) else {
            continue;
        };
        let mut deps: Vec<NodeId> = Vec::new();
        let mut total: u64 = 0;
        let mut min_start: Option<u64> = None;
        let mut max_end: u64 = 0;
        if let Some(content_refs) = resolver.key_refs(res, b"Contents") {
            let content_refs = resolver.expand(content_refs);
            if content_refs.len() <= MAX_NODE_DEPS {
                for content in &content_refs {
                    let Ok(content_number) = u32::try_from(*content) else {
                        continue;
                    };
                    // `/Contents` streams are physical (a stream cannot live in
                    // an `/ObjStm`). Prefer the inflated node; otherwise the
                    // encoded node of an unfiltered stream, whose bytes are the
                    // content.
                    let resolved = decoded_by_object
                        .get(&content_number)
                        .copied()
                        .or_else(|| plain_by_object.get(&content_number).copied());
                    let Some((node_id, content_len)) = resolved else {
                        continue;
                    };
                    deps.push(node_id);
                    total = total
                        .checked_add(content_len)
                        .ok_or_else(|| Error::resource_limit("page content length overflow"))?;
                    if let Some(&ci) = obj_index.get(content) {
                        let obj = &physical.objects[ci];
                        min_start = Some(min_start.map_or(obj.start, |m| m.min(obj.start)));
                        max_end = max_end.max(obj.end);
                    }
                }
            }
        }
        // A page with no resolvable content is still a page: record an empty
        // `PageContent` so `SEL_PAGE` numbering stays contiguous and meaningful.
        page_number = page_number
            .checked_add(1)
            .ok_or_else(|| Error::resource_limit("page number overflow"))?;
        let node = SeedNode::new(
            NodeKind::PageContent,
            total,
            u32_params(page_number),
            deps,
            "pdf:page-content",
        );
        let node_id = acc.put(store, &node)?;
        let (out_off, out_len) = match min_start {
            Some(start) => (start, max_end.saturating_sub(start)),
            None => match obj_index.get(&page_obj) {
                Some(&pi) => {
                    let obj = &physical.objects[pi];
                    (obj.start, obj.end.saturating_sub(obj.start))
                }
                None => (0, 0),
            },
        };
        acc.add_entry(IndexEntry {
            key: SelectorKey::new(SEL_PAGE, page_number),
            out_off,
            out_len,
            node_id,
        })?;
        acc.page_nodes += 1;
    }

    Ok(())
}

/// One decoded object stream retained for byte-level page recovery.
struct ObjStmBuf {
    bytes: Vec<u8>,
    spans: Vec<Span>,
}

/// Where a resolved object's body bytes live.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Src {
    /// The exact physical source (`N 0 obj ... endobj`).
    Physical,
    /// A decoded `/ObjStm` buffer, by index into the retained buffers.
    ObjStm(usize),
}

/// A resolved object body: its source, leading-container kind, and byte range.
#[derive(Clone, Copy)]
struct Resolved {
    src: Src,
    is_array: bool,
    lo: u64,
    hi: u64,
}

/// A read-only resolver over physical and object-stream object bodies.
struct ObjResolver<'a> {
    source: &'a [u8],
    source_spans: &'a [Span],
    buffers: &'a [ObjStmBuf],
    map: BTreeMap<u64, Resolved>,
}

impl<'a> ObjResolver<'a> {
    /// The byte slice and lexical cover backing a resolved object body.
    fn view(&self, res: Resolved) -> (&'a [u8], &'a [Span]) {
        match res.src {
            Src::Physical => (self.source, self.source_spans),
            Src::ObjStm(i) => {
                let buf = &self.buffers[i];
                (&buf.bytes, &buf.spans)
            }
        }
    }

    /// Resolve an object number to its body, if it has a leading container.
    fn resolve(&self, number: u64) -> Option<Resolved> {
        self.map.get(&number).copied()
    }

    /// Whether the object's *top-level* `/Type` is `want`.
    fn is_type(&self, res: Resolved, want: &[u8]) -> bool {
        if res.is_array {
            return false;
        }
        let (bytes, spans) = self.view(res);
        let win = span_window(spans, res.lo, res.hi);
        top_level_name_value(bytes, win, res.lo, res.hi, b"Type") == Some(want)
    }

    /// The indirect references following a top-level `key` in the object dict.
    fn key_refs(&self, res: Resolved, key: &[u8]) -> Option<Vec<u64>> {
        if res.is_array {
            return None;
        }
        let (bytes, spans) = self.view(res);
        let win = span_window(spans, res.lo, res.hi);
        collect_key_refs(bytes, win, res.lo, res.hi, key)
    }

    /// Expand references that point at an array object into the refs inside it
    /// (one level, best-effort), across both sources.
    fn expand(&self, refs: Vec<u64>) -> Vec<u64> {
        let mut out: Vec<u64> = Vec::new();
        for reference in refs {
            if out.len() > MAX_REFS {
                break;
            }
            let expanded = self.resolve(reference).and_then(|res| {
                if !res.is_array {
                    return None;
                }
                let (bytes, spans) = self.view(res);
                let win = span_window(spans, res.lo, res.hi);
                parse_ref_array(bytes, win, 0, res.hi)
            });
            match expanded {
                Some(items) => out.extend(items),
                None => out.push(reference),
            }
        }
        out
    }
}

/// Learn a lone zlib stream's exact decoded length, or decline.
///
/// The decoded length is unknown a priori, so it is learned by inflating under a
/// hard cap rather than invented. Exceeding the cap declines (both when miniz
/// errors and when it would silently clamp). The compressed payload is never
/// re-baked.
fn try_decode_len(source: &[u8], data_start: u64, data_len: u64, limits: Limits) -> Option<u64> {
    if data_len > MAX_STREAM_DECODE {
        return None;
    }
    let start = usize::try_from(data_start).ok()?;
    let end = usize::try_from(data_start.checked_add(data_len)?).ok()?;
    let encoded = source.get(start..end)?;
    if !zlib_shape_ok(encoded) {
        return None;
    }
    // `+1` distinguishes "there are more bytes" from "exactly at the cap" for
    // implementations that clamp instead of erroring.
    let cap_u64 = MAX_STREAM_DECODE
        .min(limits.max_output_bytes)
        .checked_add(1)?;
    let cap = usize::try_from(cap_u64).ok()?;
    let decoded = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(encoded, cap).ok()?;
    let decoded_len = decoded.len() as u64;
    if decoded_len > MAX_STREAM_DECODE || decoded_len > limits.max_output_bytes {
        return None;
    }
    Some(decoded_len)
}

/// Whether `bytes` begins with a structurally valid zlib (RFC 1950) header.
///
/// Mirrors the shape test in `codec::deflate::zlib_header_valid`; that function
/// is behind the opt-in `deflate-replay` feature, which the `field` feature does
/// not imply. This is a shape test only, never authority: the decode above
/// decides.
fn zlib_shape_ok(bytes: &[u8]) -> bool {
    if bytes.len() < 2 {
        return false;
    }
    let cmf = bytes[0];
    let flg = bytes[1];
    cmf & 0x0f == 8 && cmf >> 4 <= 7 && (u16::from(cmf) * 256 + u16::from(flg)).is_multiple_of(31)
}

/// Pack an object number/generation into the canonical `(u32, u16)` identity.
fn identity32(number: u64, generation: u64) -> Option<(u32, u16)> {
    Some((u32::try_from(number).ok()?, u16::try_from(generation).ok()?))
}

/// Require `offset` and `len` to fit in 32 bits (the `object_params` packing).
fn span32(offset: u64, len: u64) -> Option<(u64, u64)> {
    if offset >> 32 != 0 || len >> 32 != 0 {
        return None;
    }
    Some((offset, len))
}

/// Spans whose start lies in `[lo, hi)`.
fn span_window(spans: &[Span], lo: u64, hi: u64) -> &[Span] {
    let a = spans.partition_point(|s| s.start < lo);
    let b = spans.partition_point(|s| s.start < hi);
    &spans[a..b]
}

/// The leading `<< ... >>` or `[ ... ]` container byte range within a window.
///
/// Returns `(is_array, lo, hi)`. The first container opened in the window wins,
/// so an indirect-object header (`N G obj`) is skipped and a nested container
/// cannot be mistaken for the object's own body. `None` when the window holds no
/// balanced container (fail closed rather than guess).
fn leading_container(win: &[Span]) -> Option<(bool, u64, u64)> {
    for (i, span) in win.iter().enumerate() {
        let closer = match span.kind {
            SpanKind::DictOpen => SpanKind::DictClose,
            SpanKind::ArrayOpen => SpanKind::ArrayClose,
            _ => continue,
        };
        let opener = span.kind;
        let mut depth: u32 = 0;
        for s in &win[i..] {
            if s.kind == opener {
                depth = depth.checked_add(1)?;
            } else if s.kind == closer {
                if depth == 0 {
                    continue;
                }
                depth -= 1;
                if depth == 0 {
                    return Some((
                        opener == SpanKind::ArrayOpen,
                        span.start,
                        s.start.checked_add(s.len)?,
                    ));
                }
            }
        }
        return None;
    }
    None
}

/// Collect the indirect references following a *top-level* `key` inside the
/// leading dictionary `[lo, hi)`.
///
/// Only a key at bracket-depth 1 and outside every array is considered, so a
/// nested sub-dictionary (e.g. Cairo's `/Group << ... >>` before `/Type /Page`)
/// can never shadow it. Accepts a single `N G R` or an inline array of them.
/// Returns `None` when the key is absent, the value is neither shape, or more
/// than [`MAX_REFS`] entries appear (fail closed rather than guess).
fn collect_key_refs(source: &[u8], win: &[Span], lo: u64, hi: u64, key: &[u8]) -> Option<Vec<u64>> {
    let name = find_top_level_name(win, source, lo, hi, key)?;
    let t0 = next_sig(win, name + 1, hi)?;
    if win[t0].kind == SpanKind::ArrayOpen {
        parse_ref_array(source, win, t0, hi)
    } else {
        let (number, _generation, _next) = read_ref(source, win, t0, hi)?;
        Some(vec![number])
    }
}

/// The `/Name` value of a *top-level* `key` in the leading dictionary.
///
/// Depth-aware like [`collect_key_refs`], so a nested `/Type` (e.g. inside a
/// `/Group` sub-dictionary) is not mistaken for the object's own type.
fn top_level_name_value<'a>(
    source: &'a [u8],
    win: &[Span],
    lo: u64,
    hi: u64,
    key: &[u8],
) -> Option<&'a [u8]> {
    let name = find_top_level_name(win, source, lo, hi, key)?;
    let t = next_sig(win, name + 1, hi)?;
    let span = win[t];
    if span.kind != SpanKind::Name {
        return None;
    }
    span_bytes(source, span)?.strip_prefix(b"/")
}

/// Parse an inline `[ N G R ... ]` array of references beginning at span index
/// `open_idx` (a `ArrayOpen`), bounded by `hi`.
fn parse_ref_array(source: &[u8], win: &[Span], open_idx: usize, hi: u64) -> Option<Vec<u64>> {
    let mut out: Vec<u64> = Vec::new();
    let mut i = open_idx + 1;
    loop {
        let t = next_sig(win, i, hi)?;
        if win[t].kind == SpanKind::ArrayClose {
            return Some(out);
        }
        let (number, _generation, next) = read_ref(source, win, t, hi)?;
        out.push(number);
        if out.len() > MAX_REFS {
            return None;
        }
        i = next;
    }
}

/// The integer value of a *top-level* `key` in the given dictionary window.
fn top_level_integer_value(
    source: &[u8],
    win: &[Span],
    lo: u64,
    hi: u64,
    key: &[u8],
) -> Option<u64> {
    let name = find_top_level_name(win, source, lo, hi, key)?;
    let t = next_sig(win, name + 1, hi)?;
    integer_span(source, win[t])
}

/// Parse an `/ObjStm` header of `n` `objnum offset` integer pairs.
///
/// When `/First` is present the header is bounded to the bytes before it and the
/// remainder must be blank; when it is absent the pairs are read from the start of
/// the buffer. Bounded by `n`, so a corrupted count cannot scan unboundedly.
fn parse_objstm_header(bytes: &[u8], first: Option<u64>, n: usize) -> Option<Vec<(u64, u64)>> {
    let mut pos = 0usize;
    let mut pairs = Vec::with_capacity(n.min(4096));
    for _ in 0..n {
        let object = read_uint_ws(bytes, &mut pos)?;
        let offset = read_uint_ws(bytes, &mut pos)?;
        pairs.push((object, offset));
    }
    if let Some(f) = first {
        let f = usize::try_from(f).ok()?;
        if f > bytes.len() || pos > f {
            return None;
        }
        if bytes[pos..f].iter().any(|&b| !is_pdf_ws(b)) {
            return None;
        }
    }
    Some(pairs)
}

/// Read a whitespace-delimited unsigned decimal integer, advancing `pos`.
fn read_uint_ws(bytes: &[u8], pos: &mut usize) -> Option<u64> {
    while *pos < bytes.len() && is_pdf_ws(bytes[*pos]) {
        *pos += 1;
    }
    let start = *pos;
    let mut value: u64 = 0;
    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        value = value
            .checked_mul(10)?
            .checked_add(u64::from(bytes[*pos] - b'0'))?;
        *pos += 1;
    }
    if *pos == start {
        return None;
    }
    Some(value)
}

/// Whether `b` is a PDF whitespace byte (PDF 32000-1 Table 1).
fn is_pdf_ws(b: u8) -> bool {
    matches!(b, 0x00 | 0x09 | 0x0A | 0x0C | 0x0D | 0x20)
}

/// Read one `N G R` reference at a significant span index.
fn read_ref(source: &[u8], win: &[Span], at: usize, hi: u64) -> Option<(u64, u64, usize)> {
    let i = next_sig(win, at, hi)?;
    let number = integer_span(source, win[i])?;
    let j = next_sig(win, i + 1, hi)?;
    let generation = integer_span(source, win[j])?;
    let k = next_sig(win, j + 1, hi)?;
    if !is_regular(source, win[k], b"R") {
        return None;
    }
    Some((number, generation, k + 1))
}

/// Index of a *top-level* (bracket-depth 1, outside any array) `Name` span equal
/// to `/<key>` fully inside `[lo, hi)`.
///
/// A nested dictionary or array increments the tracked depth, so a key that only
/// appears inside one is never returned.
fn find_top_level_name(win: &[Span], source: &[u8], lo: u64, hi: u64, key: &[u8]) -> Option<usize> {
    let mut dict_depth: i32 = 0;
    let mut array_depth: i32 = 0;
    for (i, span) in win.iter().enumerate() {
        if span.start < lo || span.start >= hi {
            continue;
        }
        match span.kind {
            SpanKind::DictOpen => dict_depth += 1,
            SpanKind::DictClose => dict_depth -= 1,
            SpanKind::ArrayOpen => array_depth += 1,
            SpanKind::ArrayClose => array_depth -= 1,
            SpanKind::Name
                if dict_depth == 1
                    && array_depth == 0
                    && span_bytes(source, *span).is_some_and(|b| {
                        b.len() == key.len() + 1 && b[0] == b'/' && &b[1..] == key
                    }) =>
            {
                return Some(i);
            }
            _ => {}
        }
    }
    None
}

/// Index of the next non-whitespace, non-comment span starting before `hi`.
fn next_sig(win: &[Span], from: usize, hi: u64) -> Option<usize> {
    let mut j = from;
    while j < win.len() {
        let span = win[j];
        if span.start >= hi {
            return None;
        }
        match span.kind {
            SpanKind::Whitespace | SpanKind::Comment => j += 1,
            _ => return Some(j),
        }
    }
    None
}

/// Parse a `Regular` decimal integer span.
fn integer_span(source: &[u8], span: Span) -> Option<u64> {
    if span.kind != SpanKind::Regular {
        return None;
    }
    let bytes = span_bytes(source, span)?;
    if bytes.is_empty() {
        return None;
    }
    let mut value: u64 = 0;
    for &b in bytes {
        if !b.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(u64::from(b - b'0'))?;
    }
    Some(value)
}

/// Whether `span` is the `Regular` keyword `keyword`.
fn is_regular(source: &[u8], span: Span, keyword: &[u8]) -> bool {
    span.kind == SpanKind::Regular && span_bytes(source, span) == Some(keyword)
}

/// The bytes backing `span`, or `None` if the offset is out of range.
fn span_bytes(source: &[u8], span: Span) -> Option<&[u8]> {
    let start = usize::try_from(span.start).ok()?;
    let end = usize::try_from(span.start.checked_add(span.len)?).ok()?;
    source.get(start..end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::{Descriptor, ObjectSource};
    use crate::dra::{Op, Program};
    use crate::field::dag::EvalBudget;
    use std::fs;
    use std::path::PathBuf;

    fn temp_root(label: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "vole-ingest-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        p
    }

    /// Wrap raw bytes as an opaque exact `.voldoc` descriptor.
    fn opaque_descriptor(source: &[u8]) -> Vec<u8> {
        let d = Descriptor {
            universe: crate::container::UNIVERSE.to_string(),
            source_format: crate::SOURCE_FORMAT_PDF,
            format_basis: "pdf:ingest-test".to_string(),
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

    /// A genuine zlib stream using stored (uncompressed) DEFLATE blocks, so the
    /// fixture needs no compressor dependency and is byte-deterministic.
    fn zlib_stored(data: &[u8]) -> Vec<u8> {
        assert!(!data.is_empty());
        let mut out = vec![0x78, 0x01];
        let chunks: Vec<&[u8]> = data.chunks(0xFFFF).collect();
        for (i, chunk) in chunks.iter().enumerate() {
            let final_block = u8::from(i + 1 == chunks.len());
            out.push(final_block); // BFINAL, BTYPE=00 (stored)
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
        /// A trailer (with no xref body) pointing at `/Root`; enough for the
        /// physical scan, which never consults the cross-reference table. Used by
        /// fixtures whose catalog is not a physical object (it lives in an
        /// `/ObjStm`, which has no physical offset to record).
        fn raw_trailer(&mut self, size: u64, root: u64) {
            self.text(&format!(
                "trailer\n<< /Size {size} /Root {root} 0 R >>\n%%EOF\n"
            ));
        }
    }

    /// A classic-xref PDF with one page and one lone-Flate content stream whose
    /// plaintext contains `(Hello) Tj`.
    fn fixture_pdf() -> Vec<u8> {
        let content = b"BT /F1 12 Tf 72 720 Td (Hello) Tj ET\n";
        let encoded = zlib_stored(content);
        let mut w = PdfBuilder::new();
        w.text("%PDF-1.5\n");
        w.obj(1, b"<< /Type /Catalog /Pages 2 0 R >>");
        w.obj(2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
        w.obj(
            3,
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
        );
        w.stream_obj(4, " /Filter /FlateDecode", &encoded);
        w.obj(5, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
        w.classic_trailer(6, " /Root 1 0 R");
        w.buf
    }

    /// A one-page PDF whose page object carries a nested `/Group << ... /Type
    /// /Group ... >>` *before* its own `/Type /Page` (Cairo-style key order).
    fn fixture_pdf_nested_type() -> Vec<u8> {
        let content = b"BT /F1 12 Tf 72 720 Td (Nested) Tj ET\n";
        let encoded = zlib_stored(content);
        let mut w = PdfBuilder::new();
        w.text("%PDF-1.5\n");
        w.obj(1, b"<< /Type /Catalog /Pages 2 0 R >>");
        w.obj(2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
        w.obj(
            3,
            b"<< /Contents 4 0 R /Group << /S /Transparency /Type /Group >> /MediaBox [0 0 612 792] /Parent 2 0 R /Resources << /Font << /F1 5 0 R >> >> /Type /Page >>",
        );
        w.stream_obj(4, " /Filter /FlateDecode", &encoded);
        w.obj(5, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
        w.classic_trailer(6, " /Root 1 0 R");
        w.buf
    }

    /// A one-page PDF whose content stream has no `/Filter`, so its encoded
    /// bytes are the content.
    fn fixture_pdf_unfiltered() -> Vec<u8> {
        let content = b"BT /F1 12 Tf 72 720 Td (Plain) Tj ET\n";
        let mut w = PdfBuilder::new();
        w.text("%PDF-1.5\n");
        w.obj(1, b"<< /Type /Catalog /Pages 2 0 R >>");
        w.obj(2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
        w.obj(
            3,
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
        );
        w.stream_obj(4, "", content);
        w.obj(5, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
        w.classic_trailer(6, " /Root 1 0 R");
        w.buf
    }

    /// Build an `/ObjStm` payload: an `objnum offset` header plus the bodies.
    ///
    /// Returns the stored-block-zlib-compressed stream, the pair count (`/N`),
    /// and the header length (`/First`). Offsets are relative to `/First` and
    /// bodies are newline-separated, as PDF 32000-1 §7.5.7 requires.
    fn objstm_stream(objs: &[(u64, &[u8])]) -> (Vec<u8>, u64, u64) {
        let mut bodies = Vec::new();
        let mut offsets = Vec::new();
        for (_, body) in objs {
            offsets.push(bodies.len());
            bodies.extend_from_slice(body);
            bodies.push(b'\n');
        }
        let mut header = String::new();
        for (i, (number, _)) in objs.iter().enumerate() {
            if i > 0 {
                header.push(' ');
            }
            header.push_str(&format!("{number} {}", offsets[i]));
        }
        header.push('\n');
        let first = header.len() as u64;
        let mut decoded = header.into_bytes();
        decoded.extend_from_slice(&bodies);
        (zlib_stored(&decoded), objs.len() as u64, first)
    }

    /// A PDF whose `/Catalog`, `/Pages`, and `/Page` all live inside `/ObjStm`
    /// object 1, with a physical (Flate) content stream. The plaintext says
    /// `(Streamed)`.
    fn fixture_pdf_objstm() -> Vec<u8> {
        let content = b"BT /F1 12 Tf 72 720 Td (Streamed) Tj ET\n";
        let encoded = zlib_stored(content);
        let page = b"<< /Type /Page /Parent 3 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 6 0 R >> >> /Contents 5 0 R >>";
        let pages = b"<< /Type /Pages /Kids [2 0 R] /Count 1 >>";
        let catalog = b"<< /Type /Catalog /Pages 3 0 R >>";
        let (objstm, n, first) = objstm_stream(&[(2, page), (3, pages), (4, catalog)]);
        let mut w = PdfBuilder::new();
        w.text("%PDF-1.5\n");
        w.stream_obj(
            1,
            &format!(" /Filter /FlateDecode /Type /ObjStm /N {n} /First {first}"),
            &objstm,
        );
        w.stream_obj(5, " /Filter /FlateDecode", &encoded);
        w.obj(6, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
        w.raw_trailer(7, 4);
        w.buf
    }

    /// Like [`fixture_pdf_objstm`] but the object stream's own `/Pages` tree is
    /// shadowed by a *later physical* catalog and page tree. The physical tree's
    /// plaintext says `(Physical)`; the object stream's says `(Streamed)`.
    fn fixture_pdf_objstm_shadowed() -> Vec<u8> {
        let streamed = zlib_stored(b"BT /F1 12 Tf 72 720 Td (Streamed) Tj ET\n");
        let physical = zlib_stored(b"BT /F1 12 Tf 72 720 Td (Physical) Tj ET\n");
        let page = b"<< /Type /Page /Parent 2 0 R /Contents 5 0 R >>";
        let pages = b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>";
        let catalog = b"<< /Type /Catalog /Pages 2 0 R >>";
        let (objstm, n, first) = objstm_stream(&[(2, pages), (3, page), (4, catalog)]);
        let mut w = PdfBuilder::new();
        w.text("%PDF-1.5\n");
        w.stream_obj(
            1,
            &format!(" /Filter /FlateDecode /Type /ObjStm /N {n} /First {first}"),
            &objstm,
        );
        // A later physical object number 4 shadows the object-stream catalog.
        w.obj(4, b"<< /Type /Catalog /Pages 7 0 R >>");
        w.stream_obj(5, " /Filter /FlateDecode", &streamed);
        w.obj(7, b"<< /Type /Pages /Kids [8 0 R] /Count 1 >>");
        w.obj(
            8,
            b"<< /Type /Page /Parent 7 0 R /MediaBox [0 0 612 792] /Contents 9 0 R >>",
        );
        w.stream_obj(9, " /Filter /FlateDecode", &physical);
        w.raw_trailer(10, 4);
        w.buf
    }

    fn ingest(store: &mut FieldStore, pdf: &[u8]) -> IngestReport {
        let descriptor = opaque_descriptor(pdf);
        ingest_pdf(store, &descriptor, Limits::DEFAULT).unwrap()
    }

    #[test]
    fn ingest_recovers_streams_and_index() {
        let root = temp_root("basic");
        let mut store = FieldStore::open(&root).unwrap();
        let pdf = fixture_pdf();
        let report = ingest(&mut store, &pdf);

        assert!(report.stream_nodes >= 1);
        assert!(report.decoded_stream_nodes >= 1);
        assert!(report.object_nodes >= 3);
        assert!(report.revision_nodes >= 1);
        assert!(report.index_root.is_some());
        assert_eq!(report.index_node_count, 1);

        // Every recovered node materializes through the DAG.
        let field = Field::open(&store, &report.field, Limits::DEFAULT).unwrap();
        let mut budget = EvalBudget::default();
        let root_bytes = field
            .materialize_node(&report.root_node, Limits::DEFAULT, &mut budget)
            .unwrap();
        assert_eq!(root_bytes, pdf);

        let istore = FsIndexStore::open(store.root()).unwrap();
        let index_root = report.index_root.unwrap();
        let stream_entries =
            lookup(&istore, &index_root, &SelectorKey::new(SEL_STREAM, 4)).unwrap();
        assert!(!stream_entries.is_empty());
        for entry in &stream_entries {
            let bytes = field
                .materialize_node(&entry.node_id, Limits::DEFAULT, &mut budget)
                .unwrap();
            assert_eq!(
                bytes,
                pdf[entry.out_off as usize..(entry.out_off + entry.out_len) as usize]
            );
        }
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn exactness_is_preserved_after_ingest() {
        let root = temp_root("exact");
        let mut store = FieldStore::open(&root).unwrap();
        let pdf = fixture_pdf();
        let report = ingest(&mut store, &pdf);
        let field = Field::open(&store, &report.field, Limits::DEFAULT).unwrap();
        let materialized = field.materialize_exact(Limits::DEFAULT).unwrap();
        assert_eq!(materialized.len() as u64, report.source_len);
        assert_eq!(
            crate::integrity::sha256(&materialized),
            crate::integrity::sha256(&pdf)
        );
        assert_eq!(materialized, pdf);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn every_exact_node_matches_its_declared_span() {
        let root = temp_root("spans");
        let mut store = FieldStore::open(&root).unwrap();
        let pdf = fixture_pdf();
        let report = ingest(&mut store, &pdf);
        let field = Field::open(&store, &report.field, Limits::DEFAULT).unwrap();
        let istore = FsIndexStore::open(store.root()).unwrap();
        let index_root = report.index_root.unwrap();
        let physical = scan(&pdf, Limits::DEFAULT).unwrap();
        let mut budget = EvalBudget::default();

        let mut check = |key: SelectorKey, offset: u64, len: u64| {
            let entries = lookup(&istore, &index_root, &key).unwrap();
            assert!(!entries.is_empty(), "missing entry for {key:?}");
            let expected = &pdf[offset as usize..(offset + len) as usize];
            for entry in entries {
                let bytes = field
                    .materialize_node(&entry.node_id, Limits::DEFAULT, &mut budget)
                    .unwrap();
                assert_eq!(bytes, expected);
            }
        };

        for obj in &physical.objects {
            if obj.number == 0 {
                continue;
            }
            check(
                SelectorKey::new(SEL_OBJECT, obj.number as u32),
                obj.start,
                obj.end - obj.start,
            );
        }
        for stream in &physical.streams {
            check(
                SelectorKey::new(SEL_STREAM, stream.object as u32),
                stream.data_start,
                stream.data_len,
            );
        }
        for rev in &physical.revisions {
            check(
                SelectorKey::new(SEL_REVISION, rev.index),
                rev.start,
                rev.end - rev.start,
            );
        }
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn decoded_stream_matches_inflated_bytes() {
        let root = temp_root("decoded");
        let mut store = FieldStore::open(&root).unwrap();
        let pdf = fixture_pdf();
        let report = ingest(&mut store, &pdf);
        let field = Field::open(&store, &report.field, Limits::DEFAULT).unwrap();
        let istore = FsIndexStore::open(store.root()).unwrap();
        let index_root = report.index_root.unwrap();
        let physical = scan(&pdf, Limits::DEFAULT).unwrap();

        let stream = &physical.streams[0];
        let encoded =
            &pdf[stream.data_start as usize..(stream.data_start + stream.data_len) as usize];
        let expected = miniz_oxide::inflate::decompress_to_vec_zlib(encoded).unwrap();

        let encoded_entry = lookup(
            &istore,
            &index_root,
            &SelectorKey::new(SEL_STREAM, stream.object as u32),
        )
        .unwrap()
        .pop()
        .unwrap();
        let node = stream_decoded_node(
            stream.object as u32,
            encoded_entry.node_id,
            expected.len() as u64,
        );
        let mut budget = EvalBudget::default();
        let bytes = field
            .materialize_node(&node.content_id(), Limits::DEFAULT, &mut budget)
            .unwrap();
        assert_eq!(bytes, expected);
        assert_eq!(bytes, b"BT /F1 12 Tf 72 720 Td (Hello) Tj ET\n");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn object_and_stream_selectors_resolve() {
        let root = temp_root("selectors");
        let mut store = FieldStore::open(&root).unwrap();
        let pdf = fixture_pdf();
        let report = ingest(&mut store, &pdf);
        let istore = FsIndexStore::open(store.root()).unwrap();
        let index_root = report.index_root.unwrap();
        assert!(
            !lookup(&istore, &index_root, &SelectorKey::new(SEL_OBJECT, 1))
                .unwrap()
                .is_empty()
        );
        assert!(
            !lookup(&istore, &index_root, &SelectorKey::new(SEL_STREAM, 4))
                .unwrap()
                .is_empty()
        );
        assert!(
            lookup(&istore, &index_root, &SelectorKey::new(SEL_OBJECT, 999))
                .unwrap()
                .is_empty()
        );
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn hostile_input_never_panics() {
        let root = temp_root("hostile");
        let mut store = FieldStore::open(&root).unwrap();
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut garbage = Vec::with_capacity(4096);
        while garbage.len() < 4096 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            garbage.extend_from_slice(&state.to_le_bytes());
        }
        let descriptor = opaque_descriptor(&garbage);
        match ingest_pdf(&mut store, &descriptor, Limits::DEFAULT) {
            Ok(report) => {
                let field = Field::open(&store, &report.field, Limits::DEFAULT).unwrap();
                assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), garbage);
            }
            Err(e) => assert_eq!(e.class(), crate::ErrorClass::InvalidPdfStructure),
        }
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn deepen_is_idempotent_and_old_field_stays_exact() {
        let root = temp_root("deepen");
        let mut store = FieldStore::open(&root).unwrap();
        let pdf = fixture_pdf();
        let report = ingest(&mut store, &pdf);

        let first = report.field;
        let deepened = deepen_page(&mut store, &first, 1, Limits::DEFAULT).unwrap();
        let deepened_again = deepen_page(&mut store, &first, 1, Limits::DEFAULT).unwrap();
        assert_eq!(deepened, deepened_again);
        assert_eq!(
            deepen_page(&mut store, &deepened, 1, Limits::DEFAULT).unwrap(),
            deepened
        );
        assert_ne!(deepened, first);

        // The original field id still materializes the exact source.
        let old = Field::open(&store, &first, Limits::DEFAULT).unwrap();
        assert_eq!(old.materialize_exact(Limits::DEFAULT).unwrap(), pdf);
        let new = Field::open(&store, &deepened, Limits::DEFAULT).unwrap();
        assert_eq!(new.materialize_exact(Limits::DEFAULT).unwrap(), pdf);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn deepened_page_yields_nonempty_text_runs() {
        let root = temp_root("text");
        let mut store = FieldStore::open(&root).unwrap();
        let pdf = fixture_pdf();
        let report = ingest(&mut store, &pdf);
        assert_eq!(report.page_nodes, 1);

        let istore = FsIndexStore::open(store.root()).unwrap();
        let index_root = report.index_root.unwrap();
        let page_entry = lookup(&istore, &index_root, &SelectorKey::new(SEL_PAGE, 1))
            .unwrap()
            .pop()
            .unwrap();
        let page_content_id = page_entry.node_id;

        let deepened = deepen_page(&mut store, &report.field, 1, Limits::DEFAULT).unwrap();
        let field = Field::open(&store, &deepened, Limits::DEFAULT).unwrap();

        let (_ops, text, _preview) = derived_chain(1, page_content_id);
        let mut budget = EvalBudget::default();
        let bytes = field
            .materialize_node(&text.content_id(), Limits::DEFAULT, &mut budget)
            .unwrap();
        assert!(!bytes.is_empty());
        assert!(String::from_utf8_lossy(&bytes).contains("Hello"));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn nested_type_dict_does_not_shadow_page() {
        let root = temp_root("nested-type");
        let mut store = FieldStore::open(&root).unwrap();
        let pdf = fixture_pdf_nested_type();
        let report = ingest(&mut store, &pdf);
        assert_eq!(
            report.page_nodes, 1,
            "a nested /Type /Group must not hide the page"
        );

        let istore = FsIndexStore::open(store.root()).unwrap();
        let index_root = report.index_root.unwrap();
        let page_entry = lookup(&istore, &index_root, &SelectorKey::new(SEL_PAGE, 1))
            .unwrap()
            .pop()
            .unwrap();

        let deepened = deepen_page(&mut store, &report.field, 1, Limits::DEFAULT).unwrap();
        let field = Field::open(&store, &deepened, Limits::DEFAULT).unwrap();
        let (_ops, text, _preview) = derived_chain(1, page_entry.node_id);
        let mut budget = EvalBudget::default();
        let bytes = field
            .materialize_node(&text.content_id(), Limits::DEFAULT, &mut budget)
            .unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("Nested"));

        assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), pdf);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn unfiltered_content_stream_recovers_page() {
        let root = temp_root("unfiltered");
        let mut store = FieldStore::open(&root).unwrap();
        let pdf = fixture_pdf_unfiltered();
        let report = ingest(&mut store, &pdf);
        assert_eq!(
            report.page_nodes, 1,
            "an unfiltered /Contents stream must not be dropped"
        );

        let istore = FsIndexStore::open(store.root()).unwrap();
        let index_root = report.index_root.unwrap();
        let page_entry = lookup(&istore, &index_root, &SelectorKey::new(SEL_PAGE, 1))
            .unwrap()
            .pop()
            .unwrap();

        let deepened = deepen_page(&mut store, &report.field, 1, Limits::DEFAULT).unwrap();
        let field = Field::open(&store, &deepened, Limits::DEFAULT).unwrap();
        let (_ops, text, _preview) = derived_chain(1, page_entry.node_id);
        let mut budget = EvalBudget::default();
        let bytes = field
            .materialize_node(&text.content_id(), Limits::DEFAULT, &mut budget)
            .unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("Plain"));

        assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), pdf);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn objstm_page_tree_is_recovered_and_deepened() {
        let root = temp_root("objstm-page");
        let mut store = FieldStore::open(&root).unwrap();
        let pdf = fixture_pdf_objstm();
        let report = ingest(&mut store, &pdf);
        assert_eq!(
            report.page_nodes, 1,
            "a page whose dictionary lives in an /ObjStm must be recovered"
        );

        let istore = FsIndexStore::open(store.root()).unwrap();
        let index_root = report.index_root.unwrap();
        let page_entry = lookup(&istore, &index_root, &SelectorKey::new(SEL_PAGE, 1))
            .unwrap()
            .pop()
            .unwrap();

        let deepened = deepen_page(&mut store, &report.field, 1, Limits::DEFAULT).unwrap();
        let field = Field::open(&store, &deepened, Limits::DEFAULT).unwrap();
        let (_ops, text, _preview) = derived_chain(1, page_entry.node_id);
        let mut budget = EvalBudget::default();
        let bytes = field
            .materialize_node(&text.content_id(), Limits::DEFAULT, &mut budget)
            .unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("Streamed"));

        assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), pdf);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn malformed_objstm_is_skipped_without_panic() {
        let root = temp_root("objstm-bad");
        let mut store = FieldStore::open(&root).unwrap();
        // (a) an absurd `/N` above the object cap, (b) a `/First` past the end of
        // the decoded buffer (a truncated header).
        for (n, first) in [(100_000u64, 3u64), (3, 4096)] {
            let encoded = zlib_stored(b"BT /F1 12 Tf 72 720 Td (X) Tj ET\n");
            let page = b"<< /Type /Page /Parent 3 0 R /Contents 5 0 R >>";
            let pages = b"<< /Type /Pages /Kids [2 0 R] /Count 1 >>";
            let catalog = b"<< /Type /Catalog /Pages 3 0 R >>";
            let (objstm, _, _) = objstm_stream(&[(2, page), (3, pages), (4, catalog)]);
            let mut w = PdfBuilder::new();
            w.text("%PDF-1.5\n");
            w.stream_obj(
                1,
                &format!(" /Filter /FlateDecode /Type /ObjStm /N {n} /First {first}"),
                &objstm,
            );
            w.stream_obj(5, " /Filter /FlateDecode", &encoded);
            w.obj(6, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
            w.raw_trailer(7, 4);
            let pdf = w.buf;

            let report = ingest(&mut store, &pdf);
            assert_eq!(
                report.page_nodes, 0,
                "a malformed /ObjStm must yield no pages"
            );
            let field = Field::open(&store, &report.field, Limits::DEFAULT).unwrap();
            assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), pdf);
        }
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn physical_object_shadows_objstm_object() {
        let root = temp_root("objstm-shadow");
        let mut store = FieldStore::open(&root).unwrap();
        let pdf = fixture_pdf_objstm_shadowed();
        let report = ingest(&mut store, &pdf);
        assert_eq!(report.page_nodes, 1);

        // The later physical catalog 4 and its page tree must win over the
        // object-stream catalog 4, so the recovered text says `Physical`.
        let istore = FsIndexStore::open(store.root()).unwrap();
        let index_root = report.index_root.unwrap();
        let page_entry = lookup(&istore, &index_root, &SelectorKey::new(SEL_PAGE, 1))
            .unwrap()
            .pop()
            .unwrap();
        let deepened = deepen_page(&mut store, &report.field, 1, Limits::DEFAULT).unwrap();
        let field = Field::open(&store, &deepened, Limits::DEFAULT).unwrap();
        let (_ops, text, _preview) = derived_chain(1, page_entry.node_id);
        let mut budget = EvalBudget::default();
        let bytes = field
            .materialize_node(&text.content_id(), Limits::DEFAULT, &mut budget)
            .unwrap();
        let rendered = String::from_utf8_lossy(&bytes);
        assert!(rendered.contains("Physical"), "got {rendered:?}");
        assert!(!rendered.contains("Streamed"), "got {rendered:?}");

        assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), pdf);
        fs::remove_dir_all(&root).ok();
    }
}
