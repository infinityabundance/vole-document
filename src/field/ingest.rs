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
use crate::adapter::pdf::physical::{PdfPhysical, PdfStreamSpan, RevisionInfo, scan};
use crate::adapter::pdf::span::{Span, SpanKind};
use crate::container::{Descriptor, ParsedDescriptor};
use crate::error::{Error, Result};
use crate::field::dag;
use crate::field::index::{
    FsIndexStore, IndexEntry, SEL_OBJECT, SEL_PAGE, SEL_REVISION, SEL_REVISION_LINEAGE,
    SEL_REVISIONS, SEL_STREAM, SEL_STREAM_DECODED, SelectorKey, build, lookup, validate,
};
use crate::field::manifest::FieldRoot;
use crate::field::node::{MAX_NODE_DEPS, NodeKind, SeedNode, object_params, u32_params};
use crate::field::{Field, FieldId, FieldStore};
use crate::limits::Limits;
use crate::parallel::WorkerPool;
use crate::store::NodeId;
#[cfg(feature = "parallel")]
use rayon::prelude::*;

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
    /// The detected document format (byte-based, Phase 12.7).
    pub format: crate::field::document_format::DocumentFormat,
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
    /// Content-addressed shared-resource blobs registered. Always `0`: the
    /// Phase-11 PDF adapter does not extract embedded images/fonts as resource
    /// blobs, so a PDF shares no resource with any other document in Phase 12
    /// (a recorded limitation, not a failure). Package formats (DOCX/EPUB) do.
    pub resource_blob_nodes: u64,
    /// Resource blobs shared with an earlier document. Always `0` for PDF (see
    /// [`Self::resource_blob_nodes`]).
    pub shared_resource_ids: u64,
    /// Resource bytes not rewritten because they were already present. Always
    /// `0` for PDF (see [`Self::resource_blob_nodes`]).
    pub shared_resource_bytes: u64,
    /// Seed nodes whose content id already existed (nothing new written).
    pub nodes_id_shared: u64,
    /// Seed-node canonical bytes physically written by this ingest.
    pub seed_bytes_written: u64,
}

/// Stage A (durable exact capture) + Stage B (cheap eager inversion).
pub fn ingest_pdf(
    store: &mut FieldStore,
    descriptor_bytes: &[u8],
    limits: Limits,
) -> Result<IngestReport> {
    ingest_pdf_with(store, descriptor_bytes, limits, None)
}

/// As [`ingest_pdf`], but with an optional bounded worker pool for the pure,
/// order-independent Stage-B work (per-stream inflate, `/ObjStm` decode). `None`
/// is byte-for-byte the serial path; the merge fold always runs serially in
/// physical order, so the pool can never change a node id, a counter, or a limit.
pub fn ingest_pdf_with(
    store: &mut FieldStore,
    descriptor_bytes: &[u8],
    limits: Limits,
    pool: Option<&WorkerPool>,
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
    ingest_pdf_stage_b(store, &source, manifest, limits, pool)
}

/// Direct-build variant of [`ingest_pdf_with`] for the fixed-profile
/// [`crate::field::build`] path (Phase 18.2).
///
/// The caller already holds the exact original `source` and the already-enriched
/// `observable` authority blob, so neither is reconstructed here: Stage A stores
/// the authority and verifies it materializes to `source`
/// ([`FieldStore::ingest_verified`]), and Stage B scans the **original** bytes.
/// The observations, node ids, index, and manifest are byte-identical to
/// [`ingest_pdf_with`]; only the redundant source → authority → source round
/// trip is removed.
pub(crate) fn ingest_pdf_direct(
    store: &mut FieldStore,
    observable: &[u8],
    source: &[u8],
    limits: Limits,
    pool: Option<&WorkerPool>,
) -> Result<IngestReport> {
    let base_id = store.ingest_verified(observable, source, limits)?;
    let manifest = Field::open(store, &base_id, limits)?.manifest().clone();
    ingest_pdf_stage_b(store, source, manifest, limits, pool)
}

/// Shared Stage-B/C tail: scan `source` (the exact materialization of the
/// authority `manifest` already binds) and write the richer manifest.
fn ingest_pdf_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    manifest: FieldRoot,
    limits: Limits,
    pool: Option<&WorkerPool>,
) -> Result<IngestReport> {
    let source_len = source.len() as u64;
    // Byte-based format detection, recorded in the manifest provenance so the
    // universal observation API can dispatch common selectors without re-reading
    // the source (Phase 12.7). Never derived from a file name.
    let fmt = crate::field::document_format::detect_document_format(source, limits);

    // JSON is a Wave-2 structured-tree format with no package layer: it is
    // inverted by a dedicated (non-PDF) tail that adds the derived `JsonModel`
    // node over the exact root (Phase 21.5.1).
    #[cfg(feature = "json")]
    if fmt == crate::field::document_format::DocumentFormat::Json {
        return ingest_json_stage_b(store, source, manifest, limits);
    }

    // GeoJSON is the spatial Wave-2 format, also with no package layer: it is
    // inverted by a dedicated (non-PDF) tail that adds the derived `GeojsonModel`
    // node over the exact root (Phase 21.22). Detection is a semantic sub-test run
    // before the generic JSON detector, so a GeoJSON source is never misrouted to
    // the plain JSON tail.
    #[cfg(feature = "geojson")]
    if fmt == crate::field::document_format::DocumentFormat::Geojson {
        return ingest_geojson_stage_b(store, source, manifest, limits);
    }

    // A Jupyter notebook is the document-shaped Wave-2 format, also with no package
    // layer: it is inverted by a dedicated (non-PDF) tail that adds the derived
    // `NotebookModel` node over the exact root (Phase 21.24). Detection is a semantic
    // sub-test run before the generic JSON detector, so a notebook source is never
    // misrouted to the plain JSON tail.
    #[cfg(feature = "notebook")]
    if fmt == crate::field::document_format::DocumentFormat::Notebook {
        return ingest_notebook_stage_b(store, source, manifest, limits);
    }

    // JSON5/JSONC is the structured-extra Wave-2 format, also with no package layer:
    // it is inverted by a dedicated (non-PDF) tail that adds the derived
    // `Json5Model` node over the exact root (Phase 21.17.1). It is checked here —
    // immediately after strict JSON — so a JSON5/JSONC source never reaches the
    // line/stream or tabular tails.
    #[cfg(feature = "json5")]
    if fmt == crate::field::document_format::DocumentFormat::Json5 {
        return ingest_json5_stage_b(store, source, manifest, limits);
    }

    // JSONL/NDJSON is the line/event-stream Wave-2 format, also with no package
    // layer: it is inverted by a dedicated (non-PDF) tail that adds the derived
    // `JsonlModel` node over the exact root (Phase 21.12).
    #[cfg(feature = "jsonl")]
    if fmt == crate::field::document_format::DocumentFormat::Jsonl {
        return ingest_jsonl_stage_b(store, source, manifest, limits);
    }

    // EML/MIME is the messaging Wave-2 format, also with no package layer: it is
    // inverted by a dedicated (non-PDF) tail that adds the derived `EmlModel` node
    // over the exact root (Phase 21.13).
    #[cfg(feature = "eml")]
    if fmt == crate::field::document_format::DocumentFormat::Eml {
        return ingest_eml_stage_b(store, source, manifest, limits);
    }

    // CBOR is the binary structured-tree Wave-2 format, also with no package layer:
    // it is inverted by a dedicated (non-PDF) tail that adds the derived `CborModel`
    // node over the exact root (Phase 21.18). It is checked here — after the JSON
    // family — so a CBOR source (whose detection requires a container/tag root) is
    // never misrouted to the textual or analytical tails.
    #[cfg(feature = "cbor")]
    if fmt == crate::field::document_format::DocumentFormat::Cbor {
        return ingest_cbor_stage_b(store, source, manifest, limits);
    }

    // MessagePack is the binary structured-tree Wave-2 sibling of CBOR, also with no
    // package layer: it is inverted by a dedicated (non-PDF) tail that adds the
    // derived `MsgpackModel` node over the exact root (Phase 21.19). It is checked
    // here — immediately after CBOR — so a MessagePack source (whose detection
    // requires a container root, and which is only reached when CBOR declines) is
    // never misrouted to the textual or analytical tails.
    #[cfg(feature = "msgpack")]
    if fmt == crate::field::document_format::DocumentFormat::Msgpack {
        return ingest_msgpack_stage_b(store, source, manifest, limits);
    }

    // The config family (INI / `.env` / Java `.properties`) is the key/value line
    // Wave-2 format, also with no package layer: it is inverted by a dedicated
    // (non-PDF) tail that adds the derived `ConfigModel` node over the exact root
    // (Phase 21.20). It is checked here — after the JSON family, the binary
    // structured-tree family, and TOML — so a config source (which detection admits
    // only on a dialect-distinguishing signal) is never misrouted to the tabular or
    // prose tails.
    #[cfg(feature = "config")]
    if fmt == crate::field::document_format::DocumentFormat::Config {
        return ingest_config_stage_b(store, source, manifest, limits);
    }

    // RSS/Atom is the syndication Wave-2 format, also with no package layer: it is
    // inverted by a dedicated (non-PDF) tail that adds the derived `FeedModel` node
    // over the exact root (Phase 21.21). Detection is a semantic sub-test run before
    // the generic XML detector, so a feed source is never misrouted to the XML tail.
    #[cfg(feature = "feed")]
    if fmt == crate::field::document_format::DocumentFormat::Feed {
        return ingest_feed_stage_b(store, source, manifest, limits);
    }

    // KML/GPX is the geospatial Wave-2 format, also with no package layer: it is
    // inverted by a dedicated (non-PDF) tail that adds the derived `GisModel` node
    // over the exact root (Phase 21.23). Detection is a semantic sub-test run before
    // the generic XML detector, so a KML/GPX source is never misrouted to the XML
    // tail.
    #[cfg(feature = "gis")]
    if fmt == crate::field::document_format::DocumentFormat::Gis {
        return ingest_gis_stage_b(store, source, manifest, limits);
    }

    // Parquet is the analytical Wave-2 format, also with no package layer: it is
    // inverted by a dedicated (non-PDF) tail that adds the derived `ParquetModel`
    // node over the exact root (Phase 21.14).
    #[cfg(feature = "parquet")]
    if fmt == crate::field::document_format::DocumentFormat::Parquet {
        return ingest_parquet_stage_b(store, source, manifest, limits);
    }

    // Arrow IPC is the last listed analytical Wave-2 format, also with no package
    // layer: it is inverted by a dedicated (non-PDF) tail that adds the derived
    // `ArrowModel` node over the exact root (Phase 21.16).
    #[cfg(feature = "arrow")]
    if fmt == crate::field::document_format::DocumentFormat::ArrowIpc {
        return ingest_arrow_stage_b(store, source, manifest, limits);
    }

    // YAML is the second Wave-2 structured-tree format, also with no package layer:
    // it is inverted by a dedicated (non-PDF) tail that adds the derived
    // `YamlModel` node over the exact root (Phase 21.6.1).
    #[cfg(feature = "yaml")]
    if fmt == crate::field::document_format::DocumentFormat::Yaml {
        return ingest_yaml_stage_b(store, source, manifest, limits);
    }

    // CSV/TSV is the first tabular Wave-2 format, also with no package layer: it is
    // inverted by a dedicated (non-PDF) tail that adds the derived `CsvModel` node
    // over the exact root (Phase 21.7.1).
    #[cfg(feature = "csv")]
    if fmt == crate::field::document_format::DocumentFormat::Csv {
        return ingest_csv_stage_b(store, source, manifest, limits);
    }

    // Fixed-width (column-position) text is the second tabular Wave-2 format, also
    // with no package layer: it is inverted by a dedicated (non-PDF) tail that adds the
    // derived `FixedWidthModel` node over the exact root (Phase 21.25).
    #[cfg(feature = "fixedwidth")]
    if fmt == crate::field::document_format::DocumentFormat::FixedWidth {
        return ingest_fixedwidth_stage_b(store, source, manifest, limits);
    }

    // Markdown is the first prose Wave-2 format, also with no package layer: it is
    // inverted by a dedicated (non-PDF) tail that adds the derived `MarkdownModel`
    // node over the exact root (Phase 21.8.1).
    #[cfg(feature = "markdown")]
    if fmt == crate::field::document_format::DocumentFormat::Markdown {
        return ingest_markdown_stage_b(store, source, manifest, limits);
    }

    // reStructuredText is the next prose Wave-2 format, also with no package layer: it
    // is inverted by a dedicated (non-PDF) tail that adds the derived `RstModel` node
    // over the exact root (Phase 21.26.1).
    #[cfg(feature = "rst")]
    if fmt == crate::field::document_format::DocumentFormat::Rst {
        return ingest_rst_stage_b(store, source, manifest, limits);
    }

    // AsciiDoc is the third prose Wave-2 format, also with no package layer: it is
    // inverted by a dedicated (non-PDF) tail that adds the derived `AsciidocModel` node
    // over the exact root (Phase 21.26.2).
    #[cfg(feature = "asciidoc")]
    if fmt == crate::field::document_format::DocumentFormat::Asciidoc {
        return ingest_asciidoc_stage_b(store, source, manifest, limits);
    }

    // MDX (Markdown + JSX/ESM) is the next prose Wave-2 format, also with no package
    // layer: it is inverted by a dedicated (non-PDF) tail that adds the derived
    // `MdxModel` node over the exact root (Phase 21.26.3).
    #[cfg(feature = "mdx")]
    if fmt == crate::field::document_format::DocumentFormat::Mdx {
        return ingest_mdx_stage_b(store, source, manifest, limits);
    }

    // MHTML (MIME HTML) is the web-archive Wave-2 format, also with no package layer:
    // it is inverted by a dedicated (non-PDF) tail that adds the derived `MhtmlModel`
    // node over the exact root (Phase 21.27).
    #[cfg(feature = "mhtml")]
    if fmt == crate::field::document_format::DocumentFormat::Mhtml {
        return ingest_mhtml_stage_b(store, source, manifest, limits);
    }

    // A syslog / log stream is the line/event-stream Wave-2 format, also with no
    // package layer: it is inverted by a dedicated (non-PDF) tail that adds the
    // derived `LogstreamModel` node over the exact root (Phase 21.28).
    #[cfg(feature = "logstream")]
    if fmt == crate::field::document_format::DocumentFormat::Logstream {
        return ingest_logstream_stage_b(store, source, manifest, limits);
    }

    // XML is the structured-tree Wave-2 format for a bare XML source, also with no
    // package layer: it is inverted by a dedicated (non-PDF) tail that adds the
    // derived `XmlModel` node over the exact root (Phase 21.9).
    #[cfg(feature = "xml")]
    if fmt == crate::field::document_format::DocumentFormat::Xml {
        return ingest_xml_stage_b(store, source, manifest, limits);
    }

    // HTML is the error-recovering markup Wave-2 format for a bare HTML source,
    // also with no package layer: it is inverted by a dedicated (non-PDF) tail that
    // adds the derived `HtmlModel` node over the exact root (Phase 21.10).
    #[cfg(feature = "html")]
    if fmt == crate::field::document_format::DocumentFormat::Html {
        return ingest_html_stage_b(store, source, manifest, limits);
    }

    // TOML is the next Wave-2 structured-tree format, also with no package layer:
    // it is inverted by a dedicated (non-PDF) tail that adds the derived
    // `TomlModel` node over the exact root (Phase 21.11).
    #[cfg(feature = "toml")]
    if fmt == crate::field::document_format::DocumentFormat::Toml {
        return ingest_toml_stage_b(store, source, manifest, limits);
    }

    let mut acc = StageB::new(manifest.node_count);
    let scanned = match scan(source, limits) {
        Ok(physical) => {
            run_stage_b(
                store,
                source,
                &physical,
                limits,
                pool,
                &mut acc,
                manifest.root_node,
            )?;
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
    // Prefix the machine-readable format token (idempotent if already present),
    // then the Phase-12.8 sharing counters (representation facts, read back by
    // observations so `nodes_id_shared` is reported alongside `nodes_reused`).
    let provenance = format!(
        "{}id_shared={};res_shared={};{}",
        fmt.provenance_prefix(),
        acc.nodes_id_shared,
        acc.shared_resource_ids,
        provenance
    );

    let mut new_manifest = manifest.clone();
    if let Some(root) = &index_root {
        new_manifest.index_root = *root.as_bytes();
    }
    new_manifest.node_count = acc.node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        format: fmt,
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
        resource_blob_nodes: acc.resource_blob_nodes,
        shared_resource_ids: acc.shared_resource_ids,
        shared_resource_bytes: acc.shared_resource_bytes,
        nodes_id_shared: acc.nodes_id_shared,
        seed_bytes_written: acc.seed_bytes_written,
    })
}

/// The JSON (Wave-2 structured-tree) ingest tail (Phase 21.5.1).
///
/// JSON has no package layer, so there is nothing to
/// scan: the exact authority is the whole source (`DocumentExact`) and the only
/// derived node is the representation-preserving [`NodeKind::JsonModel`], whose
/// single dependency is that exact root (keyed by `sha256(source)`), satisfying
/// ADR-0060: the node reads the source bytes, so it must carry a source-identity
/// input and can never alias another field's source. The model is never on the
/// exactness path.
#[cfg(feature = "json")]
fn ingest_json_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    manifest: FieldRoot,
    limits: Limits,
) -> Result<IngestReport> {
    use crate::field::index::{IndexEntry, SEL_JSON_MODEL, SelectorKey};

    let source_len = source.len() as u64;
    let mut model = SeedNode::new(
        NodeKind::JsonModel,
        limits.max_output_bytes,
        Vec::new(),
        vec![manifest.root_node],
        "json:model",
    );
    model.limits.max_output_bytes = limits.max_output_bytes;
    let model_id = model.content_id();
    let model_bytes = model.encode_canonical();
    let (nodes_id_shared, seed_bytes_written) = if store.seeds().contains_node(&model_id)? {
        (1u64, 0u64)
    } else {
        let written = model_bytes.len() as u64;
        store.seeds_mut().put_node(&model_bytes)?;
        (0u64, written)
    };
    let node_count = manifest.node_count.saturating_add(1);

    let entries = vec![IndexEntry {
        key: SelectorKey::new(SEL_JSON_MODEL, 0),
        out_off: 0,
        out_len: 0,
        node_id: model_id,
    }];
    let mut istore = FsIndexStore::open(store.root())?;
    let index_root = build(&mut istore, &entries)?;
    let (index_node_count, _depth) = validate(&istore, &index_root)?;

    let provenance = format!(
        "{}field:ingest-json;model=1;nodes={node_count}",
        crate::field::document_format::DocumentFormat::Json.provenance_prefix()
    );
    let mut new_manifest = manifest.clone();
    new_manifest.index_root = *index_root.as_bytes();
    new_manifest.node_count = node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        format: crate::field::document_format::DocumentFormat::Json,
        field,
        root_node: manifest.root_node,
        index_root: Some(index_root),
        node_count,
        index_node_count,
        source_len,
        object_nodes: 0,
        stream_nodes: 0,
        decoded_stream_nodes: 0,
        page_nodes: 0,
        revision_nodes: 0,
        declined_streams: 0,
        resource_blob_nodes: 0,
        shared_resource_ids: 0,
        shared_resource_bytes: 0,
        nodes_id_shared,
        seed_bytes_written,
    })
}

/// The JSON5/JSONC (structured-extra Wave-2) ingest tail (Phase 21.17.1).
///
/// JSON5 has no package layer, so there is nothing to scan: the exact authority is
/// the whole source (`DocumentExact`) and the only derived node is the
/// representation-preserving [`NodeKind::Json5Model`], whose single dependency is
/// that exact root (keyed by `sha256(source)`), satisfying ADR-0060: the node reads
/// the source bytes, so it must carry a source-identity input and can never alias
/// another field's source. The model is never on the exactness path.
#[cfg(feature = "json5")]
fn ingest_json5_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    manifest: FieldRoot,
    limits: Limits,
) -> Result<IngestReport> {
    use crate::field::index::{IndexEntry, SEL_JSON5_MODEL, SelectorKey};

    let source_len = source.len() as u64;
    let mut model = SeedNode::new(
        NodeKind::Json5Model,
        limits.max_output_bytes,
        Vec::new(),
        vec![manifest.root_node],
        "json5:model",
    );
    model.limits.max_output_bytes = limits.max_output_bytes;
    let model_id = model.content_id();
    let model_bytes = model.encode_canonical();
    let (nodes_id_shared, seed_bytes_written) = if store.seeds().contains_node(&model_id)? {
        (1u64, 0u64)
    } else {
        let written = model_bytes.len() as u64;
        store.seeds_mut().put_node(&model_bytes)?;
        (0u64, written)
    };
    let node_count = manifest.node_count.saturating_add(1);

    let entries = vec![IndexEntry {
        key: SelectorKey::new(SEL_JSON5_MODEL, 0),
        out_off: 0,
        out_len: 0,
        node_id: model_id,
    }];
    let mut istore = FsIndexStore::open(store.root())?;
    let index_root = build(&mut istore, &entries)?;
    let (index_node_count, _depth) = validate(&istore, &index_root)?;

    let provenance = format!(
        "{}field:ingest-json5;model=1;nodes={node_count}",
        crate::field::document_format::DocumentFormat::Json5.provenance_prefix()
    );
    let mut new_manifest = manifest.clone();
    new_manifest.index_root = *index_root.as_bytes();
    new_manifest.node_count = node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        format: crate::field::document_format::DocumentFormat::Json5,
        field,
        root_node: manifest.root_node,
        index_root: Some(index_root),
        node_count,
        index_node_count,
        source_len,
        object_nodes: 0,
        stream_nodes: 0,
        decoded_stream_nodes: 0,
        page_nodes: 0,
        revision_nodes: 0,
        declined_streams: 0,
        resource_blob_nodes: 0,
        shared_resource_ids: 0,
        shared_resource_bytes: 0,
        nodes_id_shared,
        seed_bytes_written,
    })
}

/// The CBOR (binary structured-tree Wave-2) ingest tail (Phase 21.18).
///
/// CBOR has no package layer, so there is nothing to scan: the exact authority is
/// the whole source (`DocumentExact`) and the only derived node is the
/// representation-preserving [`NodeKind::CborModel`], whose single dependency is
/// that exact root (keyed by `sha256(source)`), satisfying ADR-0060: the node reads
/// the source bytes, so it must carry a source-identity input and can never alias
/// another field's source. The model is never on the exactness path.
#[cfg(feature = "cbor")]
fn ingest_cbor_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    manifest: FieldRoot,
    limits: Limits,
) -> Result<IngestReport> {
    use crate::field::index::{IndexEntry, SEL_CBOR_MODEL, SelectorKey};

    let source_len = source.len() as u64;
    let mut model = SeedNode::new(
        NodeKind::CborModel,
        limits.max_output_bytes,
        Vec::new(),
        vec![manifest.root_node],
        "cbor:model",
    );
    model.limits.max_output_bytes = limits.max_output_bytes;
    let model_id = model.content_id();
    let model_bytes = model.encode_canonical();
    let (nodes_id_shared, seed_bytes_written) = if store.seeds().contains_node(&model_id)? {
        (1u64, 0u64)
    } else {
        let written = model_bytes.len() as u64;
        store.seeds_mut().put_node(&model_bytes)?;
        (0u64, written)
    };
    let node_count = manifest.node_count.saturating_add(1);

    let entries = vec![IndexEntry {
        key: SelectorKey::new(SEL_CBOR_MODEL, 0),
        out_off: 0,
        out_len: 0,
        node_id: model_id,
    }];
    let mut istore = FsIndexStore::open(store.root())?;
    let index_root = build(&mut istore, &entries)?;
    let (index_node_count, _depth) = validate(&istore, &index_root)?;

    let provenance = format!(
        "{}field:ingest-cbor;model=1;nodes={node_count}",
        crate::field::document_format::DocumentFormat::Cbor.provenance_prefix()
    );
    let mut new_manifest = manifest.clone();
    new_manifest.index_root = *index_root.as_bytes();
    new_manifest.node_count = node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        format: crate::field::document_format::DocumentFormat::Cbor,
        field,
        root_node: manifest.root_node,
        index_root: Some(index_root),
        node_count,
        index_node_count,
        source_len,
        object_nodes: 0,
        stream_nodes: 0,
        decoded_stream_nodes: 0,
        page_nodes: 0,
        revision_nodes: 0,
        declined_streams: 0,
        resource_blob_nodes: 0,
        shared_resource_ids: 0,
        shared_resource_bytes: 0,
        nodes_id_shared,
        seed_bytes_written,
    })
}

/// The MessagePack (binary structured-tree Wave-2) ingest tail (Phase 21.19).
///
/// MessagePack has no package layer, so there is nothing to scan: the exact
/// authority is the whole source (`DocumentExact`) and the only derived node is the
/// representation-preserving [`NodeKind::MsgpackModel`], whose single dependency is
/// that exact root (keyed by `sha256(source)`), satisfying ADR-0060: the node reads
/// the source bytes, so it must carry a source-identity input and can never alias
/// another field's source. The model is never on the exactness path.
#[cfg(feature = "msgpack")]
fn ingest_msgpack_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    manifest: FieldRoot,
    limits: Limits,
) -> Result<IngestReport> {
    use crate::field::index::{IndexEntry, SEL_MSGPACK_MODEL, SelectorKey};

    let source_len = source.len() as u64;
    let mut model = SeedNode::new(
        NodeKind::MsgpackModel,
        limits.max_output_bytes,
        Vec::new(),
        vec![manifest.root_node],
        "msgpack:model",
    );
    model.limits.max_output_bytes = limits.max_output_bytes;
    let model_id = model.content_id();
    let model_bytes = model.encode_canonical();
    let (nodes_id_shared, seed_bytes_written) = if store.seeds().contains_node(&model_id)? {
        (1u64, 0u64)
    } else {
        let written = model_bytes.len() as u64;
        store.seeds_mut().put_node(&model_bytes)?;
        (0u64, written)
    };
    let node_count = manifest.node_count.saturating_add(1);

    let entries = vec![IndexEntry {
        key: SelectorKey::new(SEL_MSGPACK_MODEL, 0),
        out_off: 0,
        out_len: 0,
        node_id: model_id,
    }];
    let mut istore = FsIndexStore::open(store.root())?;
    let index_root = build(&mut istore, &entries)?;
    let (index_node_count, _depth) = validate(&istore, &index_root)?;

    let provenance = format!(
        "{}field:ingest-msgpack;model=1;nodes={node_count}",
        crate::field::document_format::DocumentFormat::Msgpack.provenance_prefix()
    );
    let mut new_manifest = manifest.clone();
    new_manifest.index_root = *index_root.as_bytes();
    new_manifest.node_count = node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        format: crate::field::document_format::DocumentFormat::Msgpack,
        field,
        root_node: manifest.root_node,
        index_root: Some(index_root),
        node_count,
        index_node_count,
        source_len,
        object_nodes: 0,
        stream_nodes: 0,
        decoded_stream_nodes: 0,
        page_nodes: 0,
        revision_nodes: 0,
        declined_streams: 0,
        resource_blob_nodes: 0,
        shared_resource_ids: 0,
        shared_resource_bytes: 0,
        nodes_id_shared,
        seed_bytes_written,
    })
}

/// The config-family (INI / `.env` / Java `.properties`) ingest tail (Phase 21.20).
///
/// The config family has no package layer, so there is nothing to scan: the exact
/// authority is the whole source (`DocumentExact`) and the only derived node is
/// the representation-preserving [`NodeKind::ConfigModel`], whose single dependency
/// is that exact root (keyed by `sha256(source)`), satisfying ADR-0060: the node
/// reads the source bytes, so it must carry a source-identity input and can never
/// alias another field's source. The model is never on the exactness path. The
/// recorded dialect is echoed into the provenance token (as CSV records its
/// dialect), so a common observation can report it without re-reading the source.
#[cfg(feature = "config")]
fn ingest_config_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    manifest: FieldRoot,
    limits: Limits,
) -> Result<IngestReport> {
    use crate::field::index::{IndexEntry, SEL_CONFIG_MODEL, SelectorKey};

    let source_len = source.len() as u64;
    // A bounded dialect sniff so the provenance token is informative without
    // parsing the whole document. Detection already admitted the source, so a
    // failure here is only possible under a tighter cap; it degrades gracefully.
    let dialect = crate::adapter::config::classify(source, limits).ok();
    let mut model = SeedNode::new(
        NodeKind::ConfigModel,
        limits.max_output_bytes,
        Vec::new(),
        vec![manifest.root_node],
        "config:model",
    );
    model.limits.max_output_bytes = limits.max_output_bytes;
    let model_id = model.content_id();
    let model_bytes = model.encode_canonical();
    let (nodes_id_shared, seed_bytes_written) = if store.seeds().contains_node(&model_id)? {
        (1u64, 0u64)
    } else {
        let written = model_bytes.len() as u64;
        store.seeds_mut().put_node(&model_bytes)?;
        (0u64, written)
    };
    let node_count = manifest.node_count.saturating_add(1);

    let entries = vec![IndexEntry {
        key: SelectorKey::new(SEL_CONFIG_MODEL, 0),
        out_off: 0,
        out_len: 0,
        node_id: model_id,
    }];
    let mut istore = FsIndexStore::open(store.root())?;
    let index_root = build(&mut istore, &entries)?;
    let (index_node_count, _depth) = validate(&istore, &index_root)?;

    let dialect_token = match dialect {
        Some(d) => format!("dialect={}", crate::adapter::config::dialect_name(d)),
        None => "dialect=none".to_string(),
    };
    let provenance = format!(
        "{}field:ingest-config;model=1;{};nodes={node_count}",
        crate::field::document_format::DocumentFormat::Config.provenance_prefix(),
        dialect_token,
    );
    let mut new_manifest = manifest.clone();
    new_manifest.index_root = *index_root.as_bytes();
    new_manifest.node_count = node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        format: crate::field::document_format::DocumentFormat::Config,
        field,
        root_node: manifest.root_node,
        index_root: Some(index_root),
        node_count,
        index_node_count,
        source_len,
        object_nodes: 0,
        stream_nodes: 0,
        decoded_stream_nodes: 0,
        page_nodes: 0,
        revision_nodes: 0,
        declined_streams: 0,
        resource_blob_nodes: 0,
        shared_resource_ids: 0,
        shared_resource_bytes: 0,
        nodes_id_shared,
        seed_bytes_written,
    })
}

/// The RSS/Atom feed ingest tail (Phase 21.21).
///
/// A feed has no package layer, so there is nothing to scan: the exact authority is
/// the whole source (`DocumentExact`) and the only derived node is the
/// representation-preserving [`NodeKind::FeedModel`], whose single dependency is
/// that exact root (keyed by `sha256(source)`), satisfying ADR-0060: the node reads
/// the source bytes, so it must carry a source-identity input and can never alias
/// another field's source. The model is never on the exactness path. The recorded
/// dialect is echoed into the provenance token (as CSV/config record their
/// dialects), so a common observation can report it without re-reading the source.
#[cfg(feature = "feed")]
fn ingest_feed_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    manifest: FieldRoot,
    limits: Limits,
) -> Result<IngestReport> {
    use crate::field::index::{IndexEntry, SEL_FEED_MODEL, SelectorKey};

    let source_len = source.len() as u64;
    // A bounded dialect sniff so the provenance token is informative without
    // parsing the whole document. Detection already admitted the source, so a
    // failure here is only possible under a tighter cap; it degrades gracefully.
    let dialect = crate::adapter::feed::classify(source, limits).ok();
    let mut model = SeedNode::new(
        NodeKind::FeedModel,
        limits.max_output_bytes,
        Vec::new(),
        vec![manifest.root_node],
        "feed:model",
    );
    model.limits.max_output_bytes = limits.max_output_bytes;
    let model_id = model.content_id();
    let model_bytes = model.encode_canonical();
    let (nodes_id_shared, seed_bytes_written) = if store.seeds().contains_node(&model_id)? {
        (1u64, 0u64)
    } else {
        let written = model_bytes.len() as u64;
        store.seeds_mut().put_node(&model_bytes)?;
        (0u64, written)
    };
    let node_count = manifest.node_count.saturating_add(1);

    let entries = vec![IndexEntry {
        key: SelectorKey::new(SEL_FEED_MODEL, 0),
        out_off: 0,
        out_len: 0,
        node_id: model_id,
    }];
    let mut istore = FsIndexStore::open(store.root())?;
    let index_root = build(&mut istore, &entries)?;
    let (index_node_count, _depth) = validate(&istore, &index_root)?;

    let dialect_token = match dialect {
        Some(d) => format!("dialect={}", crate::adapter::feed::dialect_name(d)),
        None => "dialect=none".to_string(),
    };
    let provenance = format!(
        "{}field:ingest-feed;model=1;{};nodes={node_count}",
        crate::field::document_format::DocumentFormat::Feed.provenance_prefix(),
        dialect_token,
    );
    let mut new_manifest = manifest.clone();
    new_manifest.index_root = *index_root.as_bytes();
    new_manifest.node_count = node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        format: crate::field::document_format::DocumentFormat::Feed,
        field,
        root_node: manifest.root_node,
        index_root: Some(index_root),
        node_count,
        index_node_count,
        source_len,
        object_nodes: 0,
        stream_nodes: 0,
        decoded_stream_nodes: 0,
        page_nodes: 0,
        revision_nodes: 0,
        declined_streams: 0,
        resource_blob_nodes: 0,
        shared_resource_ids: 0,
        shared_resource_bytes: 0,
        nodes_id_shared,
        seed_bytes_written,
    })
}

/// The GeoJSON ingest tail (Phase 21.22).
///
/// GeoJSON has no package layer, so there is nothing to scan: the exact authority is
/// the whole source (`DocumentExact`) and the only derived node is the
/// representation-preserving [`NodeKind::GeojsonModel`], whose single dependency is
/// that exact root (keyed by `sha256(source)`), satisfying ADR-0060: the node reads
/// the source bytes, so it must carry a source-identity input and can never alias
/// another field's source. The model is never on the exactness path. The recorded
/// root class is echoed into the provenance token (as a feed records its dialect), so
/// a common observation can report it without re-reading the source.
#[cfg(feature = "geojson")]
fn ingest_geojson_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    manifest: FieldRoot,
    limits: Limits,
) -> Result<IngestReport> {
    use crate::field::index::{IndexEntry, SEL_GEOJSON_MODEL, SelectorKey};

    let source_len = source.len() as u64;
    // A bounded class sniff so the provenance token is informative without parsing
    // the whole document twice. Detection already admitted the source, so a failure
    // here is only possible under a tighter cap; it degrades gracefully.
    let class = crate::adapter::geojson::classify(source, limits).ok();
    let mut model = SeedNode::new(
        NodeKind::GeojsonModel,
        limits.max_output_bytes,
        Vec::new(),
        vec![manifest.root_node],
        "geojson:model",
    );
    model.limits.max_output_bytes = limits.max_output_bytes;
    let model_id = model.content_id();
    let model_bytes = model.encode_canonical();
    let (nodes_id_shared, seed_bytes_written) = if store.seeds().contains_node(&model_id)? {
        (1u64, 0u64)
    } else {
        let written = model_bytes.len() as u64;
        store.seeds_mut().put_node(&model_bytes)?;
        (0u64, written)
    };
    let node_count = manifest.node_count.saturating_add(1);

    let entries = vec![IndexEntry {
        key: SelectorKey::new(SEL_GEOJSON_MODEL, 0),
        out_off: 0,
        out_len: 0,
        node_id: model_id,
    }];
    let mut istore = FsIndexStore::open(store.root())?;
    let index_root = build(&mut istore, &entries)?;
    let (index_node_count, _depth) = validate(&istore, &index_root)?;

    let class_token = match class {
        Some(c) => format!("geojson-type={}", crate::adapter::geojson::class_name(c)),
        None => "geojson-type=none".to_string(),
    };
    let provenance = format!(
        "{}field:ingest-geojson;model=1;{};nodes={node_count}",
        crate::field::document_format::DocumentFormat::Geojson.provenance_prefix(),
        class_token,
    );
    let mut new_manifest = manifest.clone();
    new_manifest.index_root = *index_root.as_bytes();
    new_manifest.node_count = node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        format: crate::field::document_format::DocumentFormat::Geojson,
        field,
        root_node: manifest.root_node,
        index_root: Some(index_root),
        node_count,
        index_node_count,
        source_len,
        object_nodes: 0,
        stream_nodes: 0,
        decoded_stream_nodes: 0,
        page_nodes: 0,
        revision_nodes: 0,
        declined_streams: 0,
        resource_blob_nodes: 0,
        shared_resource_ids: 0,
        shared_resource_bytes: 0,
        nodes_id_shared,
        seed_bytes_written,
    })
}

/// The KML/GPX geospatial ingest tail (Phase 21.23).
///
/// A GIS document has no package layer, so there is nothing to scan: the exact
/// authority is the whole source (`DocumentExact`) and the only derived node is the
/// representation-preserving [`NodeKind::GisModel`], whose single dependency is that
/// exact root (keyed by `sha256(source)`), satisfying ADR-0060: the node reads the
/// source bytes, so it must carry a source-identity input and can never alias
/// another field's source. The model is never on the exactness path. The recorded
/// dialect is echoed into the provenance token (as CSV/config/feed record their
/// dialects), so a common observation can report it without re-reading the source.
#[cfg(feature = "gis")]
fn ingest_gis_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    manifest: FieldRoot,
    limits: Limits,
) -> Result<IngestReport> {
    use crate::field::index::{IndexEntry, SEL_GIS_MODEL, SelectorKey};

    let source_len = source.len() as u64;
    // A bounded dialect sniff so the provenance token is informative without parsing
    // the whole document. Detection already admitted the source, so a failure here is
    // only possible under a tighter cap; it degrades gracefully.
    let dialect = crate::adapter::gis::classify(source, limits).ok();
    let mut model = SeedNode::new(
        NodeKind::GisModel,
        limits.max_output_bytes,
        Vec::new(),
        vec![manifest.root_node],
        "gis:model",
    );
    model.limits.max_output_bytes = limits.max_output_bytes;
    let model_id = model.content_id();
    let model_bytes = model.encode_canonical();
    let (nodes_id_shared, seed_bytes_written) = if store.seeds().contains_node(&model_id)? {
        (1u64, 0u64)
    } else {
        let written = model_bytes.len() as u64;
        store.seeds_mut().put_node(&model_bytes)?;
        (0u64, written)
    };
    let node_count = manifest.node_count.saturating_add(1);

    let entries = vec![IndexEntry {
        key: SelectorKey::new(SEL_GIS_MODEL, 0),
        out_off: 0,
        out_len: 0,
        node_id: model_id,
    }];
    let mut istore = FsIndexStore::open(store.root())?;
    let index_root = build(&mut istore, &entries)?;
    let (index_node_count, _depth) = validate(&istore, &index_root)?;

    let dialect_token = match dialect {
        Some(d) => format!("dialect={}", crate::adapter::gis::dialect_name(d)),
        None => "dialect=none".to_string(),
    };
    let provenance = format!(
        "{}field:ingest-gis;model=1;{};nodes={node_count}",
        crate::field::document_format::DocumentFormat::Gis.provenance_prefix(),
        dialect_token,
    );
    let mut new_manifest = manifest.clone();
    new_manifest.index_root = *index_root.as_bytes();
    new_manifest.node_count = node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        format: crate::field::document_format::DocumentFormat::Gis,
        field,
        root_node: manifest.root_node,
        index_root: Some(index_root),
        node_count,
        index_node_count,
        source_len,
        object_nodes: 0,
        stream_nodes: 0,
        decoded_stream_nodes: 0,
        page_nodes: 0,
        revision_nodes: 0,
        declined_streams: 0,
        resource_blob_nodes: 0,
        shared_resource_ids: 0,
        shared_resource_bytes: 0,
        nodes_id_shared,
        seed_bytes_written,
    })
}

/// The Jupyter notebook ingest tail (Phase 21.24).
///
/// A notebook has no package layer, so there is nothing to scan: the exact authority
/// is the whole source (`DocumentExact`) and the only derived node is the
/// representation-preserving [`NodeKind::NotebookModel`], whose single dependency is
/// that exact root (keyed by `sha256(source)`), satisfying ADR-0060: the node reads
/// the source bytes, so it must carry a source-identity input and can never alias
/// another field's source. The model is never on the exactness path. The recorded
/// `nbformat` is echoed into the provenance token (as a feed records its dialect), so
/// a common observation can report it without re-reading the source.
#[cfg(feature = "notebook")]
fn ingest_notebook_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    manifest: FieldRoot,
    limits: Limits,
) -> Result<IngestReport> {
    use crate::field::index::{IndexEntry, SEL_NOTEBOOK_MODEL, SelectorKey};

    let source_len = source.len() as u64;
    // A bounded parse so the provenance token is informative without parsing the whole
    // document twice. Detection already admitted the source, so a failure here is only
    // possible under a tighter cap; it degrades gracefully.
    let nbformat = crate::adapter::notebook::classify(source, limits).ok();
    let mut model = SeedNode::new(
        NodeKind::NotebookModel,
        limits.max_output_bytes,
        Vec::new(),
        vec![manifest.root_node],
        "notebook:model",
    );
    model.limits.max_output_bytes = limits.max_output_bytes;
    let model_id = model.content_id();
    let model_bytes = model.encode_canonical();
    let (nodes_id_shared, seed_bytes_written) = if store.seeds().contains_node(&model_id)? {
        (1u64, 0u64)
    } else {
        let written = model_bytes.len() as u64;
        store.seeds_mut().put_node(&model_bytes)?;
        (0u64, written)
    };
    let node_count = manifest.node_count.saturating_add(1);

    let entries = vec![IndexEntry {
        key: SelectorKey::new(SEL_NOTEBOOK_MODEL, 0),
        out_off: 0,
        out_len: 0,
        node_id: model_id,
    }];
    let mut istore = FsIndexStore::open(store.root())?;
    let index_root = build(&mut istore, &entries)?;
    let (index_node_count, _depth) = validate(&istore, &index_root)?;

    let nbformat_token = match nbformat {
        Some(n) => format!("notebook-nbformat={n}"),
        None => "notebook-nbformat=none".to_string(),
    };
    let provenance = format!(
        "{}field:ingest-notebook;model=1;{};nodes={node_count}",
        crate::field::document_format::DocumentFormat::Notebook.provenance_prefix(),
        nbformat_token,
    );
    let mut new_manifest = manifest.clone();
    new_manifest.index_root = *index_root.as_bytes();
    new_manifest.node_count = node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        format: crate::field::document_format::DocumentFormat::Notebook,
        field,
        root_node: manifest.root_node,
        index_root: Some(index_root),
        node_count,
        index_node_count,
        source_len,
        object_nodes: 0,
        stream_nodes: 0,
        decoded_stream_nodes: 0,
        page_nodes: 0,
        revision_nodes: 0,
        declined_streams: 0,
        resource_blob_nodes: 0,
        shared_resource_ids: 0,
        shared_resource_bytes: 0,
        nodes_id_shared,
        seed_bytes_written,
    })
}

/// The JSONL/NDJSON (line/event-stream Wave-2) ingest tail (Phase 21.12).
///
/// JSONL has no package layer, so there is nothing to scan: the exact authority is
/// the whole source (`DocumentExact`) and the only derived node is the per-line
/// representation-preserving [`NodeKind::JsonlModel`], whose single dependency is
/// that exact root (keyed by `sha256(source)`), satisfying ADR-0060: the node reads
/// the source bytes, so it must carry a source-identity input and can never alias
/// another field's source. The model is never on the exactness path.
#[cfg(feature = "jsonl")]
fn ingest_jsonl_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    manifest: FieldRoot,
    limits: Limits,
) -> Result<IngestReport> {
    use crate::field::index::{IndexEntry, SEL_JSONL_MODEL, SelectorKey};

    let source_len = source.len() as u64;
    let mut model = SeedNode::new(
        NodeKind::JsonlModel,
        limits.max_output_bytes,
        Vec::new(),
        vec![manifest.root_node],
        "jsonl:model",
    );
    model.limits.max_output_bytes = limits.max_output_bytes;
    let model_id = model.content_id();
    let model_bytes = model.encode_canonical();
    let (nodes_id_shared, seed_bytes_written) = if store.seeds().contains_node(&model_id)? {
        (1u64, 0u64)
    } else {
        let written = model_bytes.len() as u64;
        store.seeds_mut().put_node(&model_bytes)?;
        (0u64, written)
    };
    let node_count = manifest.node_count.saturating_add(1);

    let entries = vec![IndexEntry {
        key: SelectorKey::new(SEL_JSONL_MODEL, 0),
        out_off: 0,
        out_len: 0,
        node_id: model_id,
    }];
    let mut istore = FsIndexStore::open(store.root())?;
    let index_root = build(&mut istore, &entries)?;
    let (index_node_count, _depth) = validate(&istore, &index_root)?;

    let provenance = format!(
        "{}field:ingest-jsonl;model=1;nodes={node_count}",
        crate::field::document_format::DocumentFormat::Jsonl.provenance_prefix()
    );
    let mut new_manifest = manifest.clone();
    new_manifest.index_root = *index_root.as_bytes();
    new_manifest.node_count = node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        format: crate::field::document_format::DocumentFormat::Jsonl,
        field,
        root_node: manifest.root_node,
        index_root: Some(index_root),
        node_count,
        index_node_count,
        source_len,
        object_nodes: 0,
        stream_nodes: 0,
        decoded_stream_nodes: 0,
        page_nodes: 0,
        revision_nodes: 0,
        declined_streams: 0,
        resource_blob_nodes: 0,
        shared_resource_ids: 0,
        shared_resource_bytes: 0,
        nodes_id_shared,
        seed_bytes_written,
    })
}

/// The syslog / log-stream (line/event-stream Wave-2) ingest tail (Phase 21.28).
///
/// A log stream has no package layer, so there is nothing to scan: the exact
/// authority is the whole source (`DocumentExact`) and the only derived node is the
/// representation-preserving [`NodeKind::LogstreamModel`], whose single dependency is
/// that exact root (keyed by `sha256(source)`), satisfying ADR-0060: the node reads
/// the source bytes, so it must carry a source-identity input and can never alias
/// another field's source. The model is never on the exactness path.
#[cfg(feature = "logstream")]
fn ingest_logstream_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    manifest: FieldRoot,
    limits: Limits,
) -> Result<IngestReport> {
    use crate::field::index::{IndexEntry, SEL_LOGSTREAM_MODEL, SelectorKey};

    let source_len = source.len() as u64;
    let mut model = SeedNode::new(
        NodeKind::LogstreamModel,
        limits.max_output_bytes,
        Vec::new(),
        vec![manifest.root_node],
        "logstream:model",
    );
    model.limits.max_output_bytes = limits.max_output_bytes;
    let model_id = model.content_id();
    let model_bytes = model.encode_canonical();
    let (nodes_id_shared, seed_bytes_written) = if store.seeds().contains_node(&model_id)? {
        (1u64, 0u64)
    } else {
        let written = model_bytes.len() as u64;
        store.seeds_mut().put_node(&model_bytes)?;
        (0u64, written)
    };
    let node_count = manifest.node_count.saturating_add(1);

    let entries = vec![IndexEntry {
        key: SelectorKey::new(SEL_LOGSTREAM_MODEL, 0),
        out_off: 0,
        out_len: 0,
        node_id: model_id,
    }];
    let mut istore = FsIndexStore::open(store.root())?;
    let index_root = build(&mut istore, &entries)?;
    let (index_node_count, _depth) = validate(&istore, &index_root)?;

    let provenance = format!(
        "{}field:ingest-logstream;model=1;nodes={node_count}",
        crate::field::document_format::DocumentFormat::Logstream.provenance_prefix()
    );
    let mut new_manifest = manifest.clone();
    new_manifest.index_root = *index_root.as_bytes();
    new_manifest.node_count = node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        format: crate::field::document_format::DocumentFormat::Logstream,
        field,
        root_node: manifest.root_node,
        index_root: Some(index_root),
        node_count,
        index_node_count,
        source_len,
        object_nodes: 0,
        stream_nodes: 0,
        decoded_stream_nodes: 0,
        page_nodes: 0,
        revision_nodes: 0,
        declined_streams: 0,
        resource_blob_nodes: 0,
        shared_resource_ids: 0,
        shared_resource_bytes: 0,
        nodes_id_shared,
        seed_bytes_written,
    })
}

/// The EML/MIME (messaging Wave-2) ingest tail (Phase 21.13).
///
/// EML has no package layer, so there is nothing to scan: the exact authority is the
/// whole source (`DocumentExact`) and the only derived node is the
/// representation-preserving [`NodeKind::EmlModel`], whose single dependency is that
/// exact root (keyed by `sha256(source)`), satisfying ADR-0060: the node reads the
/// source bytes, so it must carry a source-identity input and can never alias
/// another field's source. The model is never on the exactness path.
#[cfg(feature = "eml")]
fn ingest_eml_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    manifest: FieldRoot,
    limits: Limits,
) -> Result<IngestReport> {
    use crate::field::index::{IndexEntry, SEL_EML_MODEL, SelectorKey};

    let source_len = source.len() as u64;
    let mut model = SeedNode::new(
        NodeKind::EmlModel,
        limits.max_output_bytes,
        Vec::new(),
        vec![manifest.root_node],
        "eml:model",
    );
    model.limits.max_output_bytes = limits.max_output_bytes;
    let model_id = model.content_id();
    let model_bytes = model.encode_canonical();
    let (nodes_id_shared, seed_bytes_written) = if store.seeds().contains_node(&model_id)? {
        (1u64, 0u64)
    } else {
        let written = model_bytes.len() as u64;
        store.seeds_mut().put_node(&model_bytes)?;
        (0u64, written)
    };
    let node_count = manifest.node_count.saturating_add(1);

    let entries = vec![IndexEntry {
        key: SelectorKey::new(SEL_EML_MODEL, 0),
        out_off: 0,
        out_len: 0,
        node_id: model_id,
    }];
    let mut istore = FsIndexStore::open(store.root())?;
    let index_root = build(&mut istore, &entries)?;
    let (index_node_count, _depth) = validate(&istore, &index_root)?;

    let provenance = format!(
        "{}field:ingest-eml;model=1;nodes={node_count}",
        crate::field::document_format::DocumentFormat::Eml.provenance_prefix()
    );
    let mut new_manifest = manifest.clone();
    new_manifest.index_root = *index_root.as_bytes();
    new_manifest.node_count = node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        format: crate::field::document_format::DocumentFormat::Eml,
        field,
        root_node: manifest.root_node,
        index_root: Some(index_root),
        node_count,
        index_node_count,
        source_len,
        object_nodes: 0,
        stream_nodes: 0,
        decoded_stream_nodes: 0,
        page_nodes: 0,
        revision_nodes: 0,
        declined_streams: 0,
        resource_blob_nodes: 0,
        shared_resource_ids: 0,
        shared_resource_bytes: 0,
        nodes_id_shared,
        seed_bytes_written,
    })
}

/// The Parquet (analytical Wave-2) ingest tail (Phase 21.14).
///
/// Parquet has no package layer, so there is nothing to scan: the exact authority is
/// the whole source (`DocumentExact`) and the only derived node is the bounded
/// [`NodeKind::ParquetModel`], whose single dependency is that exact root (keyed by
/// `sha256(source)`), satisfying ADR-0060: the node reads the source bytes, so it
/// must carry a source-identity input and can never alias another field's source.
/// The model is never on the exactness path.
#[cfg(feature = "parquet")]
fn ingest_parquet_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    manifest: FieldRoot,
    limits: Limits,
) -> Result<IngestReport> {
    use crate::field::index::{IndexEntry, SEL_PARQUET_MODEL, SelectorKey};

    let source_len = source.len() as u64;
    let mut model = SeedNode::new(
        NodeKind::ParquetModel,
        limits.max_output_bytes,
        Vec::new(),
        vec![manifest.root_node],
        "parquet:model",
    );
    model.limits.max_output_bytes = limits.max_output_bytes;
    let model_id = model.content_id();
    let model_bytes = model.encode_canonical();
    let (nodes_id_shared, seed_bytes_written) = if store.seeds().contains_node(&model_id)? {
        (1u64, 0u64)
    } else {
        let written = model_bytes.len() as u64;
        store.seeds_mut().put_node(&model_bytes)?;
        (0u64, written)
    };
    let node_count = manifest.node_count.saturating_add(1);

    let entries = vec![IndexEntry {
        key: SelectorKey::new(SEL_PARQUET_MODEL, 0),
        out_off: 0,
        out_len: 0,
        node_id: model_id,
    }];
    let mut istore = FsIndexStore::open(store.root())?;
    let index_root = build(&mut istore, &entries)?;
    let (index_node_count, _depth) = validate(&istore, &index_root)?;

    let provenance = format!(
        "{}field:ingest-parquet;model=1;nodes={node_count}",
        crate::field::document_format::DocumentFormat::Parquet.provenance_prefix()
    );
    let mut new_manifest = manifest.clone();
    new_manifest.index_root = *index_root.as_bytes();
    new_manifest.node_count = node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        format: crate::field::document_format::DocumentFormat::Parquet,
        field,
        root_node: manifest.root_node,
        index_root: Some(index_root),
        node_count,
        index_node_count,
        source_len,
        object_nodes: 0,
        stream_nodes: 0,
        decoded_stream_nodes: 0,
        page_nodes: 0,
        revision_nodes: 0,
        declined_streams: 0,
        resource_blob_nodes: 0,
        shared_resource_ids: 0,
        shared_resource_bytes: 0,
        nodes_id_shared,
        seed_bytes_written,
    })
}

/// The Arrow IPC (analytical Wave-2) ingest tail (Phase 21.16).
///
/// Arrow has no package layer, so there is nothing to scan: the exact authority is
/// the whole source (`DocumentExact`) and the only derived node is the bounded
/// [`NodeKind::ArrowModel`], whose single dependency is that exact root (keyed by
/// `sha256(source)`), satisfying ADR-0060: the node reads the source bytes, so it
/// must carry a source-identity input and can never alias another field's source.
/// The model is never on the exactness path.
#[cfg(feature = "arrow")]
fn ingest_arrow_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    manifest: FieldRoot,
    limits: Limits,
) -> Result<IngestReport> {
    use crate::field::index::{IndexEntry, SEL_ARROW_MODEL, SelectorKey};

    let source_len = source.len() as u64;
    let mut model = SeedNode::new(
        NodeKind::ArrowModel,
        limits.max_output_bytes,
        Vec::new(),
        vec![manifest.root_node],
        "arrow:model",
    );
    model.limits.max_output_bytes = limits.max_output_bytes;
    let model_id = model.content_id();
    let model_bytes = model.encode_canonical();
    let (nodes_id_shared, seed_bytes_written) = if store.seeds().contains_node(&model_id)? {
        (1u64, 0u64)
    } else {
        let written = model_bytes.len() as u64;
        store.seeds_mut().put_node(&model_bytes)?;
        (0u64, written)
    };
    let node_count = manifest.node_count.saturating_add(1);

    let entries = vec![IndexEntry {
        key: SelectorKey::new(SEL_ARROW_MODEL, 0),
        out_off: 0,
        out_len: 0,
        node_id: model_id,
    }];
    let mut istore = FsIndexStore::open(store.root())?;
    let index_root = build(&mut istore, &entries)?;
    let (index_node_count, _depth) = validate(&istore, &index_root)?;

    let provenance = format!(
        "{}field:ingest-arrow;model=1;nodes={node_count}",
        crate::field::document_format::DocumentFormat::ArrowIpc.provenance_prefix()
    );
    let mut new_manifest = manifest.clone();
    new_manifest.index_root = *index_root.as_bytes();
    new_manifest.node_count = node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        format: crate::field::document_format::DocumentFormat::ArrowIpc,
        field,
        root_node: manifest.root_node,
        index_root: Some(index_root),
        node_count,
        index_node_count,
        source_len,
        object_nodes: 0,
        stream_nodes: 0,
        decoded_stream_nodes: 0,
        page_nodes: 0,
        revision_nodes: 0,
        declined_streams: 0,
        resource_blob_nodes: 0,
        shared_resource_ids: 0,
        shared_resource_bytes: 0,
        nodes_id_shared,
        seed_bytes_written,
    })
}

/// The YAML (Wave-2 structured-tree) ingest tail (Phase 21.6.1).
///
/// YAML has no package layer, so there is nothing to scan: the exact authority is
/// the whole source (`DocumentExact`) and the only derived node is the
/// representation-preserving [`NodeKind::YamlModel`], whose single dependency is
/// that exact root (keyed by `sha256(source)`), satisfying ADR-0060: the node reads
/// the source bytes, so it must carry a source-identity input and can never alias
/// another field's source. The model is never on the exactness path.
#[cfg(feature = "yaml")]
fn ingest_yaml_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    manifest: FieldRoot,
    limits: Limits,
) -> Result<IngestReport> {
    use crate::field::index::{IndexEntry, SEL_YAML_MODEL, SelectorKey};

    let source_len = source.len() as u64;
    let mut model = SeedNode::new(
        NodeKind::YamlModel,
        limits.max_output_bytes,
        Vec::new(),
        vec![manifest.root_node],
        "yaml:model",
    );
    model.limits.max_output_bytes = limits.max_output_bytes;
    let model_id = model.content_id();
    let model_bytes = model.encode_canonical();
    let (nodes_id_shared, seed_bytes_written) = if store.seeds().contains_node(&model_id)? {
        (1u64, 0u64)
    } else {
        let written = model_bytes.len() as u64;
        store.seeds_mut().put_node(&model_bytes)?;
        (0u64, written)
    };
    let node_count = manifest.node_count.saturating_add(1);

    let entries = vec![IndexEntry {
        key: SelectorKey::new(SEL_YAML_MODEL, 0),
        out_off: 0,
        out_len: 0,
        node_id: model_id,
    }];
    let mut istore = FsIndexStore::open(store.root())?;
    let index_root = build(&mut istore, &entries)?;
    let (index_node_count, _depth) = validate(&istore, &index_root)?;

    let provenance = format!(
        "{}field:ingest-yaml;model=1;nodes={node_count}",
        crate::field::document_format::DocumentFormat::Yaml.provenance_prefix()
    );
    let mut new_manifest = manifest.clone();
    new_manifest.index_root = *index_root.as_bytes();
    new_manifest.node_count = node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        format: crate::field::document_format::DocumentFormat::Yaml,
        field,
        root_node: manifest.root_node,
        index_root: Some(index_root),
        node_count,
        index_node_count,
        source_len,
        object_nodes: 0,
        stream_nodes: 0,
        decoded_stream_nodes: 0,
        page_nodes: 0,
        revision_nodes: 0,
        declined_streams: 0,
        resource_blob_nodes: 0,
        shared_resource_ids: 0,
        shared_resource_bytes: 0,
        nodes_id_shared,
        seed_bytes_written,
    })
}

/// The CSV/TSV (tabular Wave-2) ingest tail (Phase 21.7.1).
///
/// CSV has no package layer, so there is nothing to scan: the exact authority is
/// the whole source (`DocumentExact`) and the only derived node is the
/// representation-preserving [`NodeKind::CsvModel`], whose single dependency is
/// that exact root (keyed by `sha256(source)`), satisfying ADR-0060: the node reads
/// the source bytes, so it must carry a source-identity input and can never alias
/// another field's source. The dialect is sniffed with a bounded sample for the
/// provenance token; the model itself is never built at ingest (so a very large
/// file ingests in bounded memory) and is never on the exactness path.
#[cfg(feature = "csv")]
fn ingest_csv_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    manifest: FieldRoot,
    limits: Limits,
) -> Result<IngestReport> {
    use crate::field::index::{IndexEntry, SEL_CSV_MODEL, SelectorKey};

    let source_len = source.len() as u64;
    // A bounded dialect sniff (sampled) so the provenance token is informative
    // without parsing the whole table. Detection already admitted the source, so a
    // failure here is only possible under a tighter cap; it degrades gracefully.
    let dialect = crate::adapter::csv::sniff_dialect(source, limits).ok();
    let mut model = SeedNode::new(
        NodeKind::CsvModel,
        limits.max_output_bytes,
        Vec::new(),
        vec![manifest.root_node],
        "csv:model",
    );
    model.limits.max_output_bytes = limits.max_output_bytes;
    let model_id = model.content_id();
    let model_bytes = model.encode_canonical();
    let (nodes_id_shared, seed_bytes_written) = if store.seeds().contains_node(&model_id)? {
        (1u64, 0u64)
    } else {
        let written = model_bytes.len() as u64;
        store.seeds_mut().put_node(&model_bytes)?;
        (0u64, written)
    };
    let node_count = manifest.node_count.saturating_add(1);

    let entries = vec![IndexEntry {
        key: SelectorKey::new(SEL_CSV_MODEL, 0),
        out_off: 0,
        out_len: 0,
        node_id: model_id,
    }];
    let mut istore = FsIndexStore::open(store.root())?;
    let index_root = build(&mut istore, &entries)?;
    let (index_node_count, _depth) = validate(&istore, &index_root)?;

    let dialect_token = match dialect {
        Some(d) => format!(
            "delimiter={};terminator={};bom={}",
            d.delimiter_name(),
            d.terminator_name(),
            d.bom_len
        ),
        None => "delimiter=none".to_string(),
    };
    let provenance = format!(
        "{}field:ingest-csv;model=1;{};nodes={node_count}",
        crate::field::document_format::DocumentFormat::Csv.provenance_prefix(),
        dialect_token,
    );
    let mut new_manifest = manifest.clone();
    new_manifest.index_root = *index_root.as_bytes();
    new_manifest.node_count = node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        format: crate::field::document_format::DocumentFormat::Csv,
        field,
        root_node: manifest.root_node,
        index_root: Some(index_root),
        node_count,
        index_node_count,
        source_len,
        object_nodes: 0,
        stream_nodes: 0,
        decoded_stream_nodes: 0,
        page_nodes: 0,
        revision_nodes: 0,
        declined_streams: 0,
        resource_blob_nodes: 0,
        shared_resource_ids: 0,
        shared_resource_bytes: 0,
        nodes_id_shared,
        seed_bytes_written,
    })
}

/// The fixed-width (column-position tabular Wave-2) ingest tail (Phase 21.25).
///
/// Fixed-width has no package layer, so there is nothing to scan: the exact authority
/// is the whole source (`DocumentExact`) and the only derived node is the
/// representation-preserving [`NodeKind::FixedWidthModel`], whose single dependency is
/// that exact root (keyed by `sha256(source)`), satisfying ADR-0060: the node reads the
/// source bytes, so it must carry a source-identity input and can never alias another
/// field's source. The inferred layout is sniffed with a bounded sample for the
/// provenance token; the model itself is never built at ingest (so a very large file
/// ingests in bounded memory) and is never on the exactness path.
#[cfg(feature = "fixedwidth")]
fn ingest_fixedwidth_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    manifest: FieldRoot,
    limits: Limits,
) -> Result<IngestReport> {
    use crate::field::index::{IndexEntry, SEL_FIXEDWIDTH_MODEL, SelectorKey};

    let source_len = source.len() as u64;
    // A bounded layout sniff (sampled) so the provenance token is informative without
    // parsing the whole table. Detection already admitted the source, so a failure here
    // is only possible under a tighter cap; it degrades gracefully.
    let layout = crate::adapter::fixedwidth::infer_layout(source, limits).ok();
    let mut model = SeedNode::new(
        NodeKind::FixedWidthModel,
        limits.max_output_bytes,
        Vec::new(),
        vec![manifest.root_node],
        "fixedwidth:model",
    );
    model.limits.max_output_bytes = limits.max_output_bytes;
    let model_id = model.content_id();
    let model_bytes = model.encode_canonical();
    let (nodes_id_shared, seed_bytes_written) = if store.seeds().contains_node(&model_id)? {
        (1u64, 0u64)
    } else {
        let written = model_bytes.len() as u64;
        store.seeds_mut().put_node(&model_bytes)?;
        (0u64, written)
    };
    let node_count = manifest.node_count.saturating_add(1);

    let entries = vec![IndexEntry {
        key: SelectorKey::new(SEL_FIXEDWIDTH_MODEL, 0),
        out_off: 0,
        out_len: 0,
        node_id: model_id,
    }];
    let mut istore = FsIndexStore::open(store.root())?;
    let index_root = build(&mut istore, &entries)?;
    let (index_node_count, _depth) = validate(&istore, &index_root)?;

    let layout_token = match layout {
        Some(l) => format!(
            "columns={};width={};terminator={};bom={}",
            l.column_count(),
            l.width,
            l.terminator_name(),
            l.bom_len
        ),
        None => "columns=none".to_string(),
    };
    let provenance = format!(
        "{}field:ingest-fixedwidth;model=1;{};nodes={node_count}",
        crate::field::document_format::DocumentFormat::FixedWidth.provenance_prefix(),
        layout_token,
    );
    let mut new_manifest = manifest.clone();
    new_manifest.index_root = *index_root.as_bytes();
    new_manifest.node_count = node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        format: crate::field::document_format::DocumentFormat::FixedWidth,
        field,
        root_node: manifest.root_node,
        index_root: Some(index_root),
        node_count,
        index_node_count,
        source_len,
        object_nodes: 0,
        stream_nodes: 0,
        decoded_stream_nodes: 0,
        page_nodes: 0,
        revision_nodes: 0,
        declined_streams: 0,
        resource_blob_nodes: 0,
        shared_resource_ids: 0,
        shared_resource_bytes: 0,
        nodes_id_shared,
        seed_bytes_written,
    })
}

/// The Markdown (prose Wave-2) ingest tail (Phase 21.8.1).
///
/// Markdown has no package layer, so there is nothing to scan: the exact authority
/// is the whole source (`DocumentExact`) and the only derived node is the
/// representation-preserving [`NodeKind::MarkdownModel`], whose single dependency
/// is that exact root (keyed by `sha256(source)`), satisfying ADR-0060: the node
/// reads the source bytes, so it must carry a source-identity input and can never
/// alias another field's source. The model itself is never built at ingest (so a
/// large file ingests in bounded memory) and is never on the exactness path.
#[cfg(feature = "markdown")]
fn ingest_markdown_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    manifest: FieldRoot,
    limits: Limits,
) -> Result<IngestReport> {
    use crate::field::index::{IndexEntry, SEL_MARKDOWN_MODEL, SelectorKey};

    let source_len = source.len() as u64;
    let mut model = SeedNode::new(
        NodeKind::MarkdownModel,
        limits.max_output_bytes,
        Vec::new(),
        vec![manifest.root_node],
        "markdown:model",
    );
    model.limits.max_output_bytes = limits.max_output_bytes;
    let model_id = model.content_id();
    let model_bytes = model.encode_canonical();
    let (nodes_id_shared, seed_bytes_written) = if store.seeds().contains_node(&model_id)? {
        (1u64, 0u64)
    } else {
        let written = model_bytes.len() as u64;
        store.seeds_mut().put_node(&model_bytes)?;
        (0u64, written)
    };
    let node_count = manifest.node_count.saturating_add(1);

    let entries = vec![IndexEntry {
        key: SelectorKey::new(SEL_MARKDOWN_MODEL, 0),
        out_off: 0,
        out_len: 0,
        node_id: model_id,
    }];
    let mut istore = FsIndexStore::open(store.root())?;
    let index_root = build(&mut istore, &entries)?;
    let (index_node_count, _depth) = validate(&istore, &index_root)?;

    let provenance = format!(
        "{}field:ingest-markdown;model=1;nodes={node_count}",
        crate::field::document_format::DocumentFormat::Markdown.provenance_prefix()
    );
    let mut new_manifest = manifest.clone();
    new_manifest.index_root = *index_root.as_bytes();
    new_manifest.node_count = node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        format: crate::field::document_format::DocumentFormat::Markdown,
        field,
        root_node: manifest.root_node,
        index_root: Some(index_root),
        node_count,
        index_node_count,
        source_len,
        object_nodes: 0,
        stream_nodes: 0,
        decoded_stream_nodes: 0,
        page_nodes: 0,
        revision_nodes: 0,
        declined_streams: 0,
        resource_blob_nodes: 0,
        shared_resource_ids: 0,
        shared_resource_bytes: 0,
        nodes_id_shared,
        seed_bytes_written,
    })
}

/// The reStructuredText (prose Wave-2) ingest tail (Phase 21.26.1).
///
/// reStructuredText has no package layer, so there is nothing to scan: the exact
/// authority is the whole source (`DocumentExact`) and the only derived node is the
/// representation-preserving [`NodeKind::RstModel`], whose single dependency is that
/// exact root (keyed by `sha256(source)`), satisfying ADR-0060: the node reads the
/// source bytes, so it must carry a source-identity input and can never alias another
/// field's source. The model itself is never built at ingest (so a large file ingests
/// in bounded memory) and is never on the exactness path.
#[cfg(feature = "rst")]
fn ingest_rst_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    manifest: FieldRoot,
    limits: Limits,
) -> Result<IngestReport> {
    use crate::field::index::{IndexEntry, SEL_RST_MODEL, SelectorKey};

    let source_len = source.len() as u64;
    let mut model = SeedNode::new(
        NodeKind::RstModel,
        limits.max_output_bytes,
        Vec::new(),
        vec![manifest.root_node],
        "rst:model",
    );
    model.limits.max_output_bytes = limits.max_output_bytes;
    let model_id = model.content_id();
    let model_bytes = model.encode_canonical();
    let (nodes_id_shared, seed_bytes_written) = if store.seeds().contains_node(&model_id)? {
        (1u64, 0u64)
    } else {
        let written = model_bytes.len() as u64;
        store.seeds_mut().put_node(&model_bytes)?;
        (0u64, written)
    };
    let node_count = manifest.node_count.saturating_add(1);

    let entries = vec![IndexEntry {
        key: SelectorKey::new(SEL_RST_MODEL, 0),
        out_off: 0,
        out_len: 0,
        node_id: model_id,
    }];
    let mut istore = FsIndexStore::open(store.root())?;
    let index_root = build(&mut istore, &entries)?;
    let (index_node_count, _depth) = validate(&istore, &index_root)?;

    let provenance = format!(
        "{}field:ingest-rst;model=1;nodes={node_count}",
        crate::field::document_format::DocumentFormat::Rst.provenance_prefix()
    );
    let mut new_manifest = manifest.clone();
    new_manifest.index_root = *index_root.as_bytes();
    new_manifest.node_count = node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        format: crate::field::document_format::DocumentFormat::Rst,
        field,
        root_node: manifest.root_node,
        index_root: Some(index_root),
        node_count,
        index_node_count,
        source_len,
        object_nodes: 0,
        stream_nodes: 0,
        decoded_stream_nodes: 0,
        page_nodes: 0,
        revision_nodes: 0,
        declined_streams: 0,
        resource_blob_nodes: 0,
        shared_resource_ids: 0,
        shared_resource_bytes: 0,
        nodes_id_shared,
        seed_bytes_written,
    })
}

/// The AsciiDoc (prose Wave-2) ingest tail (Phase 21.26.2).
///
/// AsciiDoc has no package layer, so there is nothing to scan: the exact authority is
/// the whole source (`DocumentExact`) and the only derived node is the
/// representation-preserving [`NodeKind::AsciidocModel`], whose single dependency is
/// that exact root (keyed by `sha256(source)`), satisfying ADR-0060: the node reads the
/// source bytes, so it must carry a source-identity input and can never alias another
/// field's source. The model itself is never built at ingest (so a large file ingests
/// in bounded memory) and is never on the exactness path.
#[cfg(feature = "asciidoc")]
fn ingest_asciidoc_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    manifest: FieldRoot,
    limits: Limits,
) -> Result<IngestReport> {
    use crate::field::index::{IndexEntry, SEL_ASCIIDOC_MODEL, SelectorKey};

    let source_len = source.len() as u64;
    let mut model = SeedNode::new(
        NodeKind::AsciidocModel,
        limits.max_output_bytes,
        Vec::new(),
        vec![manifest.root_node],
        "asciidoc:model",
    );
    model.limits.max_output_bytes = limits.max_output_bytes;
    let model_id = model.content_id();
    let model_bytes = model.encode_canonical();
    let (nodes_id_shared, seed_bytes_written) = if store.seeds().contains_node(&model_id)? {
        (1u64, 0u64)
    } else {
        let written = model_bytes.len() as u64;
        store.seeds_mut().put_node(&model_bytes)?;
        (0u64, written)
    };
    let node_count = manifest.node_count.saturating_add(1);

    let entries = vec![IndexEntry {
        key: SelectorKey::new(SEL_ASCIIDOC_MODEL, 0),
        out_off: 0,
        out_len: 0,
        node_id: model_id,
    }];
    let mut istore = FsIndexStore::open(store.root())?;
    let index_root = build(&mut istore, &entries)?;
    let (index_node_count, _depth) = validate(&istore, &index_root)?;

    let provenance = format!(
        "{}field:ingest-asciidoc;model=1;nodes={node_count}",
        crate::field::document_format::DocumentFormat::Asciidoc.provenance_prefix()
    );
    let mut new_manifest = manifest.clone();
    new_manifest.index_root = *index_root.as_bytes();
    new_manifest.node_count = node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        format: crate::field::document_format::DocumentFormat::Asciidoc,
        field,
        root_node: manifest.root_node,
        index_root: Some(index_root),
        node_count,
        index_node_count,
        source_len,
        object_nodes: 0,
        stream_nodes: 0,
        decoded_stream_nodes: 0,
        page_nodes: 0,
        revision_nodes: 0,
        declined_streams: 0,
        resource_blob_nodes: 0,
        shared_resource_ids: 0,
        shared_resource_bytes: 0,
        nodes_id_shared,
        seed_bytes_written,
    })
}

/// The MDX (prose Wave-2) ingest tail (Phase 21.26.3).
///
/// MDX has no package layer, so there is nothing to scan: the exact authority is the
/// whole source (`DocumentExact`) and the only derived node is the
/// representation-preserving [`NodeKind::MdxModel`], whose single dependency is that
/// exact root (keyed by `sha256(source)`), satisfying ADR-0060: the node reads the
/// source bytes, so it must carry a source-identity input and can never alias another
/// field's source. The model itself is never built at ingest (so a large file ingests
/// in bounded memory) and is never on the exactness path.
#[cfg(feature = "mdx")]
fn ingest_mdx_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    manifest: FieldRoot,
    limits: Limits,
) -> Result<IngestReport> {
    use crate::field::index::{IndexEntry, SEL_MDX_MODEL, SelectorKey};

    let source_len = source.len() as u64;
    let mut model = SeedNode::new(
        NodeKind::MdxModel,
        limits.max_output_bytes,
        Vec::new(),
        vec![manifest.root_node],
        "mdx:model",
    );
    model.limits.max_output_bytes = limits.max_output_bytes;
    let model_id = model.content_id();
    let model_bytes = model.encode_canonical();
    let (nodes_id_shared, seed_bytes_written) = if store.seeds().contains_node(&model_id)? {
        (1u64, 0u64)
    } else {
        let written = model_bytes.len() as u64;
        store.seeds_mut().put_node(&model_bytes)?;
        (0u64, written)
    };
    let node_count = manifest.node_count.saturating_add(1);

    let entries = vec![IndexEntry {
        key: SelectorKey::new(SEL_MDX_MODEL, 0),
        out_off: 0,
        out_len: 0,
        node_id: model_id,
    }];
    let mut istore = FsIndexStore::open(store.root())?;
    let index_root = build(&mut istore, &entries)?;
    let (index_node_count, _depth) = validate(&istore, &index_root)?;

    let provenance = format!(
        "{}field:ingest-mdx;model=1;nodes={node_count}",
        crate::field::document_format::DocumentFormat::Mdx.provenance_prefix()
    );
    let mut new_manifest = manifest.clone();
    new_manifest.index_root = *index_root.as_bytes();
    new_manifest.node_count = node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        format: crate::field::document_format::DocumentFormat::Mdx,
        field,
        root_node: manifest.root_node,
        index_root: Some(index_root),
        node_count,
        index_node_count,
        source_len,
        object_nodes: 0,
        stream_nodes: 0,
        decoded_stream_nodes: 0,
        page_nodes: 0,
        revision_nodes: 0,
        declined_streams: 0,
        resource_blob_nodes: 0,
        shared_resource_ids: 0,
        shared_resource_bytes: 0,
        nodes_id_shared,
        seed_bytes_written,
    })
}

/// The MHTML (MIME HTML) ingest tail (Phase 21.27).
///
/// MHTML has no package layer, so there is nothing to scan: the exact authority is
/// the whole source (`DocumentExact`) and the only derived node is the
/// representation-preserving [`NodeKind::MhtmlModel`], whose single dependency is
/// that exact root (keyed by `sha256(source)`), satisfying ADR-0060: the node reads
/// the source bytes, so it must carry a source-identity input and can never alias
/// another field's source. The model itself is never built at ingest (so a large file
/// ingests in bounded memory) and is never on the exactness path.
#[cfg(feature = "mhtml")]
fn ingest_mhtml_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    manifest: FieldRoot,
    limits: Limits,
) -> Result<IngestReport> {
    use crate::field::index::{IndexEntry, SEL_MHTML_MODEL, SelectorKey};

    let source_len = source.len() as u64;
    let mut model = SeedNode::new(
        NodeKind::MhtmlModel,
        limits.max_output_bytes,
        Vec::new(),
        vec![manifest.root_node],
        "mhtml:model",
    );
    model.limits.max_output_bytes = limits.max_output_bytes;
    let model_id = model.content_id();
    let model_bytes = model.encode_canonical();
    let (nodes_id_shared, seed_bytes_written) = if store.seeds().contains_node(&model_id)? {
        (1u64, 0u64)
    } else {
        let written = model_bytes.len() as u64;
        store.seeds_mut().put_node(&model_bytes)?;
        (0u64, written)
    };
    let node_count = manifest.node_count.saturating_add(1);

    let entries = vec![IndexEntry {
        key: SelectorKey::new(SEL_MHTML_MODEL, 0),
        out_off: 0,
        out_len: 0,
        node_id: model_id,
    }];
    let mut istore = FsIndexStore::open(store.root())?;
    let index_root = build(&mut istore, &entries)?;
    let (index_node_count, _depth) = validate(&istore, &index_root)?;

    let provenance = format!(
        "{}field:ingest-mhtml;model=1;nodes={node_count}",
        crate::field::document_format::DocumentFormat::Mhtml.provenance_prefix()
    );
    let mut new_manifest = manifest.clone();
    new_manifest.index_root = *index_root.as_bytes();
    new_manifest.node_count = node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        format: crate::field::document_format::DocumentFormat::Mhtml,
        field,
        root_node: manifest.root_node,
        index_root: Some(index_root),
        node_count,
        index_node_count,
        source_len,
        object_nodes: 0,
        stream_nodes: 0,
        decoded_stream_nodes: 0,
        page_nodes: 0,
        revision_nodes: 0,
        declined_streams: 0,
        resource_blob_nodes: 0,
        shared_resource_ids: 0,
        shared_resource_bytes: 0,
        nodes_id_shared,
        seed_bytes_written,
    })
}

///
/// TOML has no package layer, so there is nothing to scan: the exact authority is
/// the whole source (`DocumentExact`) and the only derived node is the
/// representation-preserving [`NodeKind::TomlModel`], whose single dependency is
/// that exact root (keyed by `sha256(source)`), satisfying ADR-0060: the node reads
/// the source bytes, so it must carry a source-identity input and can never alias
/// another field's source. The model is never on the exactness path.
#[cfg(feature = "toml")]
fn ingest_toml_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    manifest: FieldRoot,
    limits: Limits,
) -> Result<IngestReport> {
    use crate::field::index::{IndexEntry, SEL_TOML_MODEL, SelectorKey};

    let source_len = source.len() as u64;
    let mut model = SeedNode::new(
        NodeKind::TomlModel,
        limits.max_output_bytes,
        Vec::new(),
        vec![manifest.root_node],
        "toml:model",
    );
    model.limits.max_output_bytes = limits.max_output_bytes;
    let model_id = model.content_id();
    let model_bytes = model.encode_canonical();
    let (nodes_id_shared, seed_bytes_written) = if store.seeds().contains_node(&model_id)? {
        (1u64, 0u64)
    } else {
        let written = model_bytes.len() as u64;
        store.seeds_mut().put_node(&model_bytes)?;
        (0u64, written)
    };
    let node_count = manifest.node_count.saturating_add(1);

    let entries = vec![IndexEntry {
        key: SelectorKey::new(SEL_TOML_MODEL, 0),
        out_off: 0,
        out_len: 0,
        node_id: model_id,
    }];
    let mut istore = FsIndexStore::open(store.root())?;
    let index_root = build(&mut istore, &entries)?;
    let (index_node_count, _depth) = validate(&istore, &index_root)?;

    let provenance = format!(
        "{}field:ingest-toml;model=1;nodes={node_count}",
        crate::field::document_format::DocumentFormat::Toml.provenance_prefix()
    );
    let mut new_manifest = manifest.clone();
    new_manifest.index_root = *index_root.as_bytes();
    new_manifest.node_count = node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        format: crate::field::document_format::DocumentFormat::Toml,
        field,
        root_node: manifest.root_node,
        index_root: Some(index_root),
        node_count,
        index_node_count,
        source_len,
        object_nodes: 0,
        stream_nodes: 0,
        decoded_stream_nodes: 0,
        page_nodes: 0,
        revision_nodes: 0,
        declined_streams: 0,
        resource_blob_nodes: 0,
        shared_resource_ids: 0,
        shared_resource_bytes: 0,
        nodes_id_shared,
        seed_bytes_written,
    })
}

/// The HTML (error-recovering markup Wave-2) ingest tail (Phase 21.10).
///
/// HTML has no package layer, so there is nothing to scan: the exact authority is
/// the whole source (`DocumentExact`) and the only derived node is the
/// representation-preserving [`NodeKind::HtmlModel`], whose single dependency is
/// that exact root (keyed by `sha256(source)`), satisfying ADR-0060: the node reads
/// the source bytes, so it must carry a source-identity input and can never alias
/// another field's source. The model is never on the exactness path.
#[cfg(feature = "html")]
fn ingest_html_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    manifest: FieldRoot,
    limits: Limits,
) -> Result<IngestReport> {
    use crate::field::index::{IndexEntry, SEL_HTML_MODEL, SelectorKey};

    let source_len = source.len() as u64;
    let mut model = SeedNode::new(
        NodeKind::HtmlModel,
        limits.max_output_bytes,
        Vec::new(),
        vec![manifest.root_node],
        "html:model",
    );
    model.limits.max_output_bytes = limits.max_output_bytes;
    let model_id = model.content_id();
    let model_bytes = model.encode_canonical();
    let (nodes_id_shared, seed_bytes_written) = if store.seeds().contains_node(&model_id)? {
        (1u64, 0u64)
    } else {
        let written = model_bytes.len() as u64;
        store.seeds_mut().put_node(&model_bytes)?;
        (0u64, written)
    };
    let node_count = manifest.node_count.saturating_add(1);

    let entries = vec![IndexEntry {
        key: SelectorKey::new(SEL_HTML_MODEL, 0),
        out_off: 0,
        out_len: 0,
        node_id: model_id,
    }];
    let mut istore = FsIndexStore::open(store.root())?;
    let index_root = build(&mut istore, &entries)?;
    let (index_node_count, _depth) = validate(&istore, &index_root)?;

    let provenance = format!(
        "{}field:ingest-html;model=1;nodes={node_count}",
        crate::field::document_format::DocumentFormat::Html.provenance_prefix()
    );
    let mut new_manifest = manifest.clone();
    new_manifest.index_root = *index_root.as_bytes();
    new_manifest.node_count = node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        format: crate::field::document_format::DocumentFormat::Html,
        field,
        root_node: manifest.root_node,
        index_root: Some(index_root),
        node_count,
        index_node_count,
        source_len,
        object_nodes: 0,
        stream_nodes: 0,
        decoded_stream_nodes: 0,
        page_nodes: 0,
        revision_nodes: 0,
        declined_streams: 0,
        resource_blob_nodes: 0,
        shared_resource_ids: 0,
        shared_resource_bytes: 0,
        nodes_id_shared,
        seed_bytes_written,
    })
}

/// The XML (structured-tree Wave-2) ingest tail (Phase 21.9).
///
/// XML has no package layer, so there is nothing to scan: the exact authority is the
/// whole source (`DocumentExact`) and the only derived node is the
/// representation-preserving [`NodeKind::XmlModel`], whose single dependency is that
/// exact root (keyed by `sha256(source)`), satisfying ADR-0060: the node reads the
/// source bytes, so it must carry a source-identity input and can never alias another
/// field's source. The model is never on the exactness path.
#[cfg(feature = "xml")]
fn ingest_xml_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    manifest: FieldRoot,
    limits: Limits,
) -> Result<IngestReport> {
    use crate::field::index::{IndexEntry, SEL_XML_MODEL, SelectorKey};

    let source_len = source.len() as u64;
    let mut model = SeedNode::new(
        NodeKind::XmlModel,
        limits.max_output_bytes,
        Vec::new(),
        vec![manifest.root_node],
        "xml:model",
    );
    model.limits.max_output_bytes = limits.max_output_bytes;
    let model_id = model.content_id();
    let model_bytes = model.encode_canonical();
    let (nodes_id_shared, seed_bytes_written) = if store.seeds().contains_node(&model_id)? {
        (1u64, 0u64)
    } else {
        let written = model_bytes.len() as u64;
        store.seeds_mut().put_node(&model_bytes)?;
        (0u64, written)
    };
    let node_count = manifest.node_count.saturating_add(1);

    let entries = vec![IndexEntry {
        key: SelectorKey::new(SEL_XML_MODEL, 0),
        out_off: 0,
        out_len: 0,
        node_id: model_id,
    }];
    let mut istore = FsIndexStore::open(store.root())?;
    let index_root = build(&mut istore, &entries)?;
    let (index_node_count, _depth) = validate(&istore, &index_root)?;

    let provenance = format!(
        "{}field:ingest-xml;model=1;nodes={node_count}",
        crate::field::document_format::DocumentFormat::Xml.provenance_prefix()
    );
    let mut new_manifest = manifest.clone();
    new_manifest.index_root = *index_root.as_bytes();
    new_manifest.node_count = node_count;
    new_manifest.index_node_count = index_node_count;
    new_manifest.provenance = provenance;
    let field = store.put_field(&new_manifest)?;

    Ok(IngestReport {
        format: crate::field::document_format::DocumentFormat::Xml,
        field,
        root_node: manifest.root_node,
        index_root: Some(index_root),
        node_count,
        index_node_count,
        source_len,
        object_nodes: 0,
        stream_nodes: 0,
        decoded_stream_nodes: 0,
        page_nodes: 0,
        revision_nodes: 0,
        declined_streams: 0,
        resource_blob_nodes: 0,
        shared_resource_ids: 0,
        shared_resource_bytes: 0,
        nodes_id_shared,
        seed_bytes_written,
    })
}

/// A universal ingest outcome: which native inverse compiler ran.
#[cfg(feature = "package")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngestOutcome {
    /// A PDF (or opaque non-ZIP) field, produced by [`ingest_pdf`].
    Pdf(IngestReport),
    /// A ZIP-based package (DOCX/EPUB/generic ZIP), produced by
    /// [`crate::field::ingest_package::ingest_package`].
    Package(crate::field::ingest_package::PackageIngestReport),
}

/// Detect the source format **from bytes** and invert it with the right adapter
/// (Phase 12.7). A validated ZIP is inverted through the byte-authoritative
/// package layer; everything else (PDF and the opaque floor) goes through
/// [`ingest_pdf`]. Never consults a file name.
#[cfg(feature = "package")]
pub fn ingest(
    store: &mut FieldStore,
    descriptor_bytes: &[u8],
    limits: Limits,
) -> Result<IngestOutcome> {
    ingest_with(store, descriptor_bytes, limits, None)
}

/// As [`ingest`], but with an optional bounded worker pool threaded into whichever
/// adapter runs. `None` is byte-for-byte the serial path.
#[cfg(feature = "package")]
pub fn ingest_with(
    store: &mut FieldStore,
    descriptor_bytes: &[u8],
    limits: Limits,
    pool: Option<&WorkerPool>,
) -> Result<IngestOutcome> {
    let parsed = crate::container::Descriptor::parse(descriptor_bytes, limits)?;
    let source = crate::materialize::materialize(&parsed, limits)?;
    if crate::field::document_format::is_zip(&source, limits) {
        Ok(IngestOutcome::Package(
            crate::field::ingest_package::ingest_package_with(
                store,
                descriptor_bytes,
                limits,
                pool,
            )?,
        ))
    } else {
        Ok(IngestOutcome::Pdf(ingest_pdf_with(
            store,
            descriptor_bytes,
            limits,
            pool,
        )?))
    }
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
pub(crate) fn with_observation_index(bytes: &[u8], limits: Limits) -> Result<Vec<u8>> {
    let parsed: ParsedDescriptor = Descriptor::parse(bytes, limits)?;
    if parsed.descriptor.observation_index.is_some() {
        return Ok(bytes.to_vec());
    }
    // The derivation is now a pure method on the descriptor (Phase 18.3), so the
    // direct build can attach the same record before its single serialize pass.
    // The fallbacks are preserved exactly: any decline (a limit breach, an op
    // length that does not fit `u32`, a refused serialize) returns the input.
    let enriched = parsed.descriptor.with_observation_index(limits);
    if enriched.observation_index.is_none() {
        return Ok(bytes.to_vec());
    }
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
    // Already promoted for this page: idempotent no-op. The format token is
    // preserved so a promoted field still serves common observations.
    let format_token = manifest
        .provenance
        .split(';')
        .next()
        .filter(|t| t.starts_with("format="))
        .map_or(String::new(), |t| format!("{t};"));
    let target = format!("{format_token}field:deepen;page={page}");
    if manifest.provenance == target {
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
    new_manifest.provenance = format!("{format_token}field:deepen;page={page}");
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
    /// Phase 12.8 cross-document sharing counters.
    resource_blob_nodes: u64,
    shared_resource_ids: u64,
    shared_resource_bytes: u64,
    nodes_id_shared: u64,
    seed_bytes_written: u64,
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
            resource_blob_nodes: 0,
            shared_resource_ids: 0,
            shared_resource_bytes: 0,
            nodes_id_shared: 0,
            seed_bytes_written: 0,
        }
    }

    /// Content-addressed `put_node` that records id-shared and newly-written
    /// bytes (Phase 12.8). The `put_node` call is idempotent, so an id that
    /// already existed writes nothing.
    fn put(&mut self, store: &mut FieldStore, node: &SeedNode) -> Result<NodeId> {
        if self.node_count >= MAX_INGEST_NODES {
            return Err(Error::resource_limit(format!(
                "ingest would exceed {MAX_INGEST_NODES} seed nodes"
            )));
        }
        let id = node.content_id();
        let preexisting = store.seeds().contains_node(&id)?;
        let canonical = node.encode_canonical();
        store.seeds_mut().put_node(&canonical)?;
        self.node_count += 1;
        if preexisting {
            self.nodes_id_shared += 1;
        } else {
            self.seed_bytes_written = self
                .seed_bytes_written
                .saturating_add(canonical.len() as u64);
        }
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
///
/// The source-reading leaves (`PdfObject`, `PdfRevision`, `PdfStreamEncoded`) get
/// the field's exact-authority `root_id` as a dependency, so their output (a
/// source span) is a pure function of their identity and two byte-different spans
/// at the same `(offset, len)` in two sources never alias in the shared cache.
fn run_stage_b(
    store: &mut FieldStore,
    source: &[u8],
    physical: &PdfPhysical,
    limits: Limits,
    pool: Option<&WorkerPool>,
    acc: &mut StageB,
    root_id: NodeId,
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
            vec![root_id],
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
            vec![root_id],
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

    // Revision lineage (Phase 17): a compact derived projection of the physical
    // revision chain, computed once here (the scan is already in hand) so a
    // `Revisions`/`Revision(n) + Lineage` observation resolves through the index
    // in O(depth) instead of re-scanning the source. One document-level node plus
    // one node per revision; the per-revision entry carries the revision's exact
    // source span so the scoped answer reports it without a second lookup.
    if !physical.revisions.is_empty() {
        let full = pdf_revision_lineage_json(source, physical);
        let node = SeedNode::new(
            NodeKind::PdfRevisionLineage,
            full.len() as u64,
            full.into_bytes(),
            Vec::new(),
            "pdf:revision-lineage",
        );
        let id = acc.put(store, &node)?;
        acc.add_entry(IndexEntry {
            key: SelectorKey::new(SEL_REVISIONS, 0),
            out_off: 0,
            out_len: 0,
            node_id: id,
        })?;
        for rev in &physical.revisions {
            let Some((offset, len)) = span32(rev.start, rev.end.saturating_sub(rev.start)) else {
                continue;
            };
            let json = pdf_revision_json(rev, physical);
            let node = SeedNode::new(
                NodeKind::PdfRevisionLineage,
                json.len() as u64,
                json.into_bytes(),
                Vec::new(),
                "pdf:revision-lineage",
            );
            let id = acc.put(store, &node)?;
            acc.add_entry(IndexEntry {
                key: SelectorKey::new(SEL_REVISION_LINEAGE, rev.index),
                out_off: offset,
                out_len: len,
                node_id: id,
            })?;
        }
    }

    // Encoded stream spans, plus one eager decode for a lone Flate stream. The
    // decoded length of every candidate is learned up front (pure, order-free);
    // the running `MAX_TOTAL_DECODED` gate below still runs serially in stream
    // order, so the pool can never change which streams are admitted.
    let lengths = decode_lengths(pool, source, physical, limits);
    for (i, stream) in physical.streams.iter().enumerate() {
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
            vec![root_id],
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
        match lengths[i] {
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

    recover_pages(store, source, physical, limits, pool, acc)
}

/// The exact decoded length of every stream, in physical order.
///
/// Only a lone `/FlateDecode` stream is inflated, and only to learn its length;
/// the decoded bytes are discarded. This is a pure function of the immutable
/// inputs, so it may run on a worker pool. Indexed `par_iter().collect()`
/// preserves order, so `lengths[i]` is stream `i`'s length whether the pool is
/// used or not.
///
/// The running `MAX_TOTAL_DECODED` gate is deliberately **not** applied here: it
/// is replayed serially by [`run_stage_b`], in stream order.
fn decode_lengths(
    pool: Option<&WorkerPool>,
    source: &[u8],
    physical: &PdfPhysical,
    limits: Limits,
) -> Vec<Option<u64>> {
    let compute = |s: &PdfStreamSpan| {
        if s.filter == FilterClass::FlateDecode {
            try_decode_len(source, s.data_start, s.data_len, limits)
        } else {
            None
        }
    };
    #[cfg(feature = "parallel")]
    if let Some(p) = pool.filter(|p| p.workers() > 1) {
        return p.install(|| physical.streams.par_iter().map(compute).collect());
    }
    #[cfg(not(feature = "parallel"))]
    let _ = pool;
    physical.streams.iter().map(compute).collect()
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
    pool: Option<&WorkerPool>,
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
    //
    // Candidate selection is pure and cheap (no inflate); the expensive pure work
    // (inflate, header parse, and the per-buffer lex) runs on the pool. The result
    // is index-aligned with `physical.streams`, so the serial fold below keeps the
    // `MAX_OBJSTM`/`MAX_OBJSTM_BYTES` caps and the `buffers`/`objstm` insertion
    // order identical to the serial path.
    let mut inputs: Vec<Option<ObjStmInput>> = Vec::with_capacity(physical.streams.len());
    for stream in &physical.streams {
        inputs.push(objstm_input(
            source,
            stream,
            &decoded_by_object,
            &obj_index,
            &containers,
            &spans,
        ));
    }
    let precomputed = precompute_objstm(pool, source, &inputs, limits);

    let mut buffers: Vec<ObjStmBuf> = Vec::new();
    let mut objstm: BTreeMap<u64, Resolved> = BTreeMap::new();
    let mut total_objstm: u64 = 0;
    for pre in precomputed.into_iter().flatten() {
        if buffers.len() >= MAX_OBJSTM {
            break;
        }
        let decoded = pre.decoded;
        let Some(total) = total_objstm.checked_add(decoded.len() as u64) else {
            continue;
        };
        if total > MAX_OBJSTM_BYTES {
            continue;
        }
        let len = decoded.len() as u64;
        // Per spec the pair offsets are relative to `/First` (the header end);
        // when `/First` is absent the offsets are treated as absolute.
        let base = pre.first.unwrap_or(0);
        let buf_index = buffers.len();
        let mut entries: Vec<(u64, Resolved)> = Vec::with_capacity(pre.pairs.len());
        for (i, &(object, offset)) in pre.pairs.iter().enumerate() {
            if object == 0 {
                continue;
            }
            let Some(body_lo) = base.checked_add(offset) else {
                continue;
            };
            let body_hi = pre
                .pairs
                .get(i + 1)
                .and_then(|&(_, next)| base.checked_add(next))
                .filter(|&next| next >= body_lo && next <= len)
                .unwrap_or(len);
            if body_lo > body_hi || body_hi > len {
                continue;
            }
            let Some((body_is_array, clo, chi)) =
                leading_container(span_window(&pre.spans, body_lo, body_hi))
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
            spans: pre.spans,
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

/// The cheap, store-free inputs needed to inflate and parse one candidate
/// `/ObjStm`, gathered during serial candidate selection.
struct ObjStmInput {
    /// Payload byte range in the source.
    start: usize,
    end: usize,
    /// The exact decoded length already learned by Stage B.
    decoded_len: u64,
    /// The object-stream header fields read from the source dictionary.
    first: Option<u64>,
    n: u64,
}

/// The expensive, pure result for one `/ObjStm`: its inflated bytes, their
/// lexical cover, the parsed header pairs, and `/First`.
struct ObjStmPre {
    decoded: Vec<u8>,
    spans: Vec<Span>,
    pairs: Vec<(u64, u64)>,
    first: Option<u64>,
}

/// Test whether `stream` is a candidate `/ObjStm` and, if so, gather the inputs
/// needed to inflate and parse it. Pure: no store, no counters.
///
/// The `/N` bound is checked here (before the inflate) rather than after it, as
/// the serial path did; that changes only how much work a rejected stream costs,
/// never the outcome, since a rejected stream is skipped either way.
fn objstm_input(
    source: &[u8],
    stream: &PdfStreamSpan,
    decoded_by_object: &BTreeMap<u32, (NodeId, u64)>,
    obj_index: &BTreeMap<u64, usize>,
    containers: &[Option<(bool, u64, u64)>],
    spans: &[Span],
) -> Option<ObjStmInput> {
    let number32 = u32::try_from(stream.object).ok()?;
    let &(_node, decoded_len) = decoded_by_object.get(&number32)?;
    let &idx = obj_index.get(&stream.object)?;
    let (is_array, lo, hi) = containers[idx]?;
    if is_array {
        return None;
    }
    let win = span_window(spans, lo, hi);
    if top_level_name_value(source, win, lo, hi, b"Type") != Some(&b"ObjStm"[..]) {
        return None;
    }
    let n = top_level_integer_value(source, win, lo, hi, b"N")?;
    if n == 0 || n > MAX_OBJSTM_OBJECTS as u64 {
        return None;
    }
    let start = usize::try_from(stream.data_start).ok()?;
    let end = stream
        .data_start
        .checked_add(stream.data_len)
        .and_then(|e| usize::try_from(e).ok())?;
    let first = top_level_integer_value(source, win, lo, hi, b"First");
    Some(ObjStmInput {
        start,
        end,
        decoded_len,
        first,
        n,
    })
}

/// Inflate and lex every candidate `/ObjStm`, in parallel but index-aligned with
/// `inputs` (indexed `par_iter().collect()` preserves order). Pure: the inflated
/// bytes are owned and no store, counter, or cache is touched. The running
/// `MAX_OBJSTM*` caps are **not** applied here; they are replayed serially.
fn precompute_objstm(
    pool: Option<&WorkerPool>,
    source: &[u8],
    inputs: &[Option<ObjStmInput>],
    limits: Limits,
) -> Vec<Option<ObjStmPre>> {
    let compute = |input: &Option<ObjStmInput>| -> Option<ObjStmPre> {
        let input = input.as_ref()?;
        let encoded = source.get(input.start..input.end)?;
        let inflated = crate::field::derive::inflate_zlib(encoded, input.decoded_len, limits);
        let decoded = inflated.ok()?;
        if decoded.is_empty() {
            return None;
        }
        let pairs = parse_objstm_header(&decoded, input.first, input.n as usize)?;
        let lexed = lex(&decoded, limits).ok()?;
        Some(ObjStmPre {
            decoded,
            spans: lexed.spans.spans,
            pairs,
            first: input.first,
        })
    };
    #[cfg(feature = "parallel")]
    if let Some(p) = pool.filter(|p| p.workers() > 1) {
        return p.install(|| inputs.par_iter().map(compute).collect());
    }
    #[cfg(not(feature = "parallel"))]
    let _ = pool;
    inputs.iter().map(compute).collect()
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
/// hard cap rather than invented. Exceeding the cap declines (whether the
/// inflater errors or would silently clamp). The compressed payload is never
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
    let decoded_len =
        super::inflate::inflate_len(encoded, cap, super::inflate::Wrapper::Zlib).ok()?;
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

/// The `%PDF-x.y` header version string, from the scanned header span.
fn pdf_header_version(source: &[u8], physical: &PdfPhysical) -> Option<String> {
    let (off, len) = physical.header?;
    let start = usize::try_from(off).ok()?;
    let end = usize::try_from(off.checked_add(len)?).ok()?;
    Some(
        String::from_utf8_lossy(source.get(start..end)?)
            .trim()
            .to_string(),
    )
}

fn pdf_u64_array(values: &[u64]) -> String {
    values
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// One revision's lineage entry: its byte span, resolved `startxref`/`/Prev`
/// headers, and the indirect objects and streams whose bytes it defines.
fn pdf_revision_json(rev: &RevisionInfo, physical: &PdfPhysical) -> String {
    let objects: Vec<u64> = physical
        .objects
        .iter()
        .filter(|o| o.start >= rev.start && o.start < rev.end)
        .map(|o| o.number)
        .collect();
    let streams: Vec<u64> = physical
        .streams
        .iter()
        .filter(|s| s.data_start >= rev.start && s.data_start < rev.end)
        .map(|s| s.object)
        .collect();
    let startxref = rev
        .startxref
        .map_or_else(|| "null".to_string(), |v| v.to_string());
    let prev = rev
        .prev
        .map_or_else(|| "null".to_string(), |v| v.to_string());
    format!(
        "{{\"index\":{},\"start\":{},\"end\":{},\"len\":{},\"startxref\":{},\"prev\":{},\"objects\":[{}],\"streams\":[{}]}}",
        rev.index,
        rev.start,
        rev.end,
        rev.end.saturating_sub(rev.start),
        startxref,
        prev,
        pdf_u64_array(&objects),
        pdf_u64_array(&streams)
    )
}

/// The whole-document revision lineage (Phase 17): a deterministic JSON object
/// with the `%PDF-` header, the revision count, and one entry per revision.
fn pdf_revision_lineage_json(source: &[u8], physical: &PdfPhysical) -> String {
    let header = match pdf_header_version(source, physical) {
        Some(h) => format!("\"{}\"", crate::field::provenance::json_escape(&h)),
        None => "null".to_string(),
    };
    let revs = physical
        .revisions
        .iter()
        .map(|r| pdf_revision_json(r, physical))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"format\":\"pdf\",\"header\":{},\"count\":{},\"revisions\":[{}]}}",
        header,
        physical.revisions.len(),
        revs
    )
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
            checkpoints: None,
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

    /// Ingest `pdf` once serial and once with a `workers`-thread pool, and assert
    /// the two runs are byte-identical: the same report (node ids, counters, field
    /// id) and the same exact materialization as the source.
    #[cfg(feature = "parallel")]
    fn assert_parallel_matches_serial(label: &str, pdf: &[u8], workers: usize) {
        use crate::parallel::WorkerPool;

        let descriptor = opaque_descriptor(pdf);
        let serial_root = temp_root(&format!("{label}-serial"));
        let par_root = temp_root(&format!("{label}-parallel"));

        let mut serial_store = FieldStore::open(&serial_root).unwrap();
        let serial = ingest_pdf(&mut serial_store, &descriptor, Limits::DEFAULT).unwrap();

        let mut par_store = FieldStore::open(&par_root).unwrap();
        let pool = WorkerPool::new(workers).unwrap();
        let parallel =
            ingest_pdf_with(&mut par_store, &descriptor, Limits::DEFAULT, Some(&pool)).unwrap();

        assert!(
            serial == parallel,
            "{label}: parallel report must match serial"
        );

        let serial_field = Field::open(&serial_store, &serial.field, Limits::DEFAULT).unwrap();
        let par_field = Field::open(&par_store, &parallel.field, Limits::DEFAULT).unwrap();
        let serial_bytes = serial_field.materialize_exact(Limits::DEFAULT).unwrap();
        let par_bytes = par_field.materialize_exact(Limits::DEFAULT).unwrap();
        assert_eq!(serial_bytes, par_bytes, "{label}: exact bytes must match");
        assert_eq!(par_bytes, pdf, "{label}: exact bytes must equal the source");
        assert_eq!(
            crate::integrity::sha256(&par_bytes),
            crate::integrity::sha256(pdf)
        );

        fs::remove_dir_all(&serial_root).ok();
        fs::remove_dir_all(&par_root).ok();
    }

    /// The parallel path must be byte-identical to serial, for the rank-1
    /// (per-stream inflate) and rank-2 (`/ObjStm` decode + lex) sites.
    #[cfg(feature = "parallel")]
    #[test]
    fn parallel_ingest_is_byte_identical_to_serial() {
        assert_parallel_matches_serial("pdf", &fixture_pdf(), 4);
        assert_parallel_matches_serial("pdf-nested", &fixture_pdf_nested_type(), 4);
        assert_parallel_matches_serial("pdf-unfiltered", &fixture_pdf_unfiltered(), 2);
        assert_parallel_matches_serial("pdf-objstm", &fixture_pdf_objstm(), 4);
        assert_parallel_matches_serial("pdf-objstm-shadowed", &fixture_pdf_objstm_shadowed(), 8);
    }

    /// The rank-1 length table is exactly the serial one whatever the pool: the
    /// indexed collect preserves physical order, which is what makes the running
    /// `MAX_TOTAL_DECODED` gate (replayed serially in `run_stage_b`) order-safe.
    #[cfg(feature = "parallel")]
    #[test]
    fn decoded_lengths_are_identical_with_and_without_a_pool() {
        use crate::parallel::WorkerPool;
        let pdf = fixture_pdf_objstm();
        let physical = scan(&pdf, Limits::DEFAULT).unwrap();
        let serial = decode_lengths(None, &pdf, &physical, Limits::DEFAULT);
        let pool = WorkerPool::new(4).unwrap();
        let parallel = decode_lengths(Some(&pool), &pdf, &physical, Limits::DEFAULT);
        assert!(serial == parallel, "length table must be pool-independent");
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
