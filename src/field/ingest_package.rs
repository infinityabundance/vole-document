//! Package (ZIP/OCF/OPC) procedural state (Phase 12.2).
//!
//! This is the ZIP analogue of [`crate::field::ingest::ingest_pdf`]'s Stage A/B:
//! given a ZIP source and its exact `.voldoc` descriptor, it creates a field whose
//! root materializes the exact source and whose **members** are index-navigable
//! seed nodes on the Phase-11 DAG.
//!
//! The layering is deliberate (plan §DEC-1/DEC-3/DEC-7):
//!
//! * The exact authority is the ordinary `.voldoc` descriptor — no new wire record
//!   is invented. Member metadata lives only in the seed DAG and the hierarchical
//!   index.
//! * A member's exact leaf is its **raw compressed/stored span**
//!   ([`NodeKind::PackageMemberRaw`]): no unzip/rezip, no bit-exact DEFLATE
//!   recompression.
//! * Decoding is **progressive**: a [`NodeKind::PackageMemberDecoded`] node is a
//!   registered computation over its raw dependency (raw DEFLATE for method 8,
//!   identity for stored method 0). The inflate runs only on demand and its output
//!   is persisted in the disposable derived cache, so a later query (or a later
//!   process) reuses it without re-decoding.
//!
//! Physical identity is the central-directory **ordinal**
//! ([`crate::adapter::package::PhysicalMemberId`]); duplicate names are never
//! collapsed. Encrypted or otherwise unsupported members keep their exact raw
//! bytes and simply have no decoded node (a typed decline on observation).

use crate::adapter::package::zip::{ZipMember, scan};
use crate::container::{Descriptor, ParsedDescriptor};
use crate::error::{Error, Result};
#[cfg(feature = "docx")]
use crate::field::index::SEL_DOCX_MODEL;
#[cfg(feature = "epub")]
use crate::field::index::SEL_EPUB_MODEL;
#[cfg(feature = "ods")]
use crate::field::index::SEL_ODS_MODEL;
#[cfg(feature = "odt")]
use crate::field::index::SEL_ODT_MODEL;
#[cfg(feature = "opc")]
use crate::field::index::SEL_OPC_MODEL;
#[cfg(feature = "pptx")]
use crate::field::index::SEL_PPTX_MODEL;
#[cfg(feature = "xlsx")]
use crate::field::index::SEL_XLSX_MODEL;
use crate::field::index::{
    FsIndexStore, IndexEntry, SEL_PACKAGE_MEMBER_DECODED, SEL_PACKAGE_MEMBER_RAW, SelectorKey,
    build, validate,
};
use crate::field::ingest::{MAX_INGEST_INDEX_ENTRIES, MAX_INGEST_NODES, with_observation_index};
use crate::field::manifest::{ABSENT_ROOT, FieldRoot};
use crate::field::node::{NodeKind, SeedNode, object_params, span_params};
use crate::field::resource::{is_shareable_resource, resource_blob_node};
use crate::field::{FieldId, FieldStore, PACKAGE_UNIVERSE};
use crate::limits::Limits;
use crate::parallel::WorkerPool;
use crate::store::NodeId;
#[cfg(feature = "parallel")]
use rayon::prelude::*;

/// General-purpose flag bit 0: the member is encrypted.
const FLAG_ENCRYPTED: u16 = 0x0001;

/// What one package ingest recovered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageIngestReport {
    /// The detected document format (byte-based, Phase 12.7).
    pub format: crate::field::document_format::DocumentFormat,
    /// The field id, whose manifest binds the recovered index.
    pub field: FieldId,
    /// The exact `PackageRoot` node id.
    pub root_node: NodeId,
    /// The hierarchical index root, or `None` when there were no entries.
    pub index_root: Option<NodeId>,
    /// Seed nodes created (root + raw + decoded).
    pub node_count: u64,
    /// Number of index nodes in the built tree.
    pub index_node_count: u64,
    /// Exact reconstructed source length.
    pub source_len: u64,
    /// Zip members discovered in the central directory.
    pub member_count: u64,
    /// Exact raw member nodes created.
    pub raw_nodes: u64,
    /// Decoded member nodes registered.
    pub decoded_nodes: u64,
    /// Members whose decode was declined (encrypted or unsupported method).
    pub declined_decodes: u64,
    /// Whether a generic OPC model node was registered (feature `opc`).
    pub opc_model_nodes: u64,
    /// Whether a DOCX discovery model node was registered (feature `docx`).
    pub docx_model_nodes: u64,
    /// Whether an EPUB (OCF) discovery model node was registered (feature `epub`).
    pub epub_model_nodes: u64,
    /// Whether an ODT (ODF) discovery model node was registered (feature `odt`).
    pub odt_model_nodes: u64,
    /// Whether an ODS (ODF spreadsheet) discovery model node was registered
    /// (feature `ods`, Phase 21.3.1).
    pub ods_model_nodes: u64,
    /// Whether an XLSX (SpreadsheetML) discovery model node was registered
    /// (feature `xlsx`, Phase 21.1.1).
    pub xlsx_model_nodes: u64,
    /// Whether a PPTX (PresentationML) discovery model node was registered
    /// (feature `pptx`, Phase 21.2.1).
    pub pptx_model_nodes: u64,
    /// Content-addressed shared-resource blobs registered (Phase 12.8).
    pub resource_blob_nodes: u64,
    /// Resource blobs whose content id already existed in the store, i.e. bytes
    /// physically shared with an earlier document (a representation fact).
    pub shared_resource_ids: u64,
    /// Bytes held by [`Self::shared_resource_ids`] — the resource bytes this
    /// ingest did **not** rewrite because they were already present.
    pub shared_resource_bytes: u64,
    /// Seed nodes whose content id already existed (nothing new written).
    pub nodes_id_shared: u64,
    /// Seed-node canonical bytes physically written by this ingest.
    pub seed_bytes_written: u64,
    /// Index-node bytes physically written by this ingest.
    pub index_bytes_written: u64,
}

fn charge_node(node_count: &mut u64) -> Result<()> {
    if *node_count >= MAX_INGEST_NODES {
        return Err(Error::resource_limit(format!(
            "package ingest would exceed {MAX_INGEST_NODES} seed nodes"
        )));
    }
    *node_count += 1;
    Ok(())
}

/// Mutable share/work counters threaded through a package ingest (Phase 12.8).
#[derive(Debug, Default)]
struct ShareCounters {
    resource_blob_nodes: u64,
    shared_resource_ids: u64,
    shared_resource_bytes: u64,
    nodes_id_shared: u64,
    seed_bytes_written: u64,
}

/// Content-addressed `put_node` that records whether the id already existed (a
/// zero-write share) and how many canonical bytes were newly persisted.
fn put_counted(
    store: &mut FieldStore,
    node: &SeedNode,
    counters: &mut ShareCounters,
) -> Result<(NodeId, bool)> {
    let enc = encode_seed(node);
    let preexisting = publish_counted(store, &enc, counters)?;
    Ok((enc.id, preexisting))
}

/// A node's canonical bytes and content id: the pure part of publishing, which
/// touches no store and so may run on a worker.
struct Encoded {
    id: NodeId,
    canonical: Vec<u8>,
}

/// Encode one node to its canonical bytes and content id. Pure and order-free.
fn encode_seed(node: &SeedNode) -> Encoded {
    let canonical = node.encode_canonical();
    let id = NodeId::of_node(&canonical);
    Encoded { id, canonical }
}

/// Publish an already-encoded node: the store-touching, order-dependent half.
/// The existence probe and the write must stay serial (and in physical order) so
/// `nodes_id_shared`/`seed_bytes_written` are identical to the serial path.
fn publish_counted(
    store: &mut FieldStore,
    enc: &Encoded,
    counters: &mut ShareCounters,
) -> Result<bool> {
    let preexisting = store.seeds().contains_node(&enc.id)?;
    store.seeds_mut().put_node(&enc.canonical)?;
    if preexisting {
        counters.nodes_id_shared += 1;
    } else {
        counters.seed_bytes_written = counters
            .seed_bytes_written
            .saturating_add(enc.canonical.len() as u64);
    }
    Ok(preexisting)
}

fn push_entry(entries: &mut Vec<IndexEntry>, entry: IndexEntry) -> Result<()> {
    if entries.len() >= MAX_INGEST_INDEX_ENTRIES {
        return Err(Error::resource_limit(format!(
            "package ingest would exceed {MAX_INGEST_INDEX_ENTRIES} index entries"
        )));
    }
    entries.push(entry);
    Ok(())
}

/// The pure, store-free encodings of one member's nodes. Every identity is a
/// function of the member's immutable inputs **and its source identity** (the
/// package root it belongs to), so the same member of the same source always
/// yields the same bytes regardless of scheduling, while the same *span* of a
/// different source never aliases it.
struct MemberEncoded {
    /// The exact raw leaf (the compressed/stored span).
    raw: Encoded,
    /// The content-addressed resource blob, for a *stored* member whose bytes are
    /// a recognized resource (`method == 0`); `None` otherwise. Carries the blob
    /// length for the shared-bytes counter.
    blob: Option<(Encoded, u64)>,
    /// The decoded member node, unless the member declined decode.
    decoded: Option<Encoded>,
}

/// Encode one member's nodes purely: no store, no counters, no I/O. The decode
/// decision (method/flag/length) and the resource-recognition decision are pure
/// byte facts, so they are made here rather than in the serial merge.
fn encode_member(source: &[u8], member: &ZipMember, root_id: NodeId) -> MemberEncoded {
    let ordinal = member.id.ordinal;
    let (data_off, data_len) = member.data;

    // The exact leaf: the member's raw compressed/stored span. Its output is
    // `source[data_off..data_off+data_len]`, which depends on *which* source the
    // span is read against — so the span coordinates alone are not an identity.
    // The package `root_id` (a function of the source bytes) is a dependency, so
    // two byte-different members that happen to share an `(offset, len)` in two
    // different sources get **distinct** ids and can never alias in the shared
    // derived cache.
    let mut raw = SeedNode::new(
        NodeKind::PackageMemberRaw,
        data_len,
        span_params(data_off, data_len),
        vec![root_id],
        "pkg:member-raw",
    );
    raw.limits.max_output_bytes = raw.limits.max_output_bytes.max(data_len);
    let raw = encode_seed(&raw);

    // A *stored* member whose bytes carry a recognized resource signature.
    let blob = if member.method == 0 {
        match source.get(data_off as usize..(data_off + data_len) as usize) {
            Some(bytes) if is_shareable_resource(bytes) => {
                let node = resource_blob_node(bytes);
                Some((encode_seed(&node), bytes.len() as u64))
            }
            _ => None,
        }
    } else {
        None
    };

    // Progressive decode: register the computation now; inflate only on demand.
    let decodable = member.method == 0 || member.method == 8;
    let encrypted = member.flags & FLAG_ENCRYPTED != 0;
    let decoded = if !decodable || encrypted || data_len == 0 {
        None
    } else {
        // A resource-backed decoded member is content-addressed (ordinal 0, the
        // blob as its whole input); every other decoded member keeps its per-source
        // raw-span dependency.
        let (decoded_params, decoded_deps) = match &blob {
            Some((enc, _)) => (object_params(0, member.method, 0), vec![enc.id]),
            None => (object_params(ordinal, member.method, 0), vec![raw.id]),
        };
        let mut node = SeedNode::new(
            NodeKind::PackageMemberDecoded,
            member.uncompressed_size,
            decoded_params,
            decoded_deps,
            "pkg:member-decoded",
        );
        node.limits.max_output_bytes = node.limits.max_output_bytes.max(member.uncompressed_size);
        Some(encode_seed(&node))
    };

    MemberEncoded { raw, blob, decoded }
}

/// Encode every member, in parallel but index-aligned with `members` (indexed
/// `par_iter().collect()` preserves order). Pure: no store is touched.
fn encode_members(
    pool: Option<&WorkerPool>,
    source: &[u8],
    members: &[ZipMember],
    root_id: NodeId,
) -> Vec<MemberEncoded> {
    let compute = |m: &ZipMember| encode_member(source, m, root_id);
    #[cfg(feature = "parallel")]
    if let Some(p) = pool.filter(|p| p.workers() > 1) {
        return p.install(|| members.par_iter().map(compute).collect());
    }
    #[cfg(not(feature = "parallel"))]
    let _ = pool;
    members.iter().map(compute).collect()
}

/// Stage A (durable exact capture) + package member inversion for a ZIP source.
///
/// The descriptor must parse and materialize exactly; the materialized bytes are
/// then scanned into a byte-authoritative cover. A non-ZIP input is a typed
/// rejection ([`crate::ErrorClass::InvalidZipStructure`]) — this entry point is
/// deliberately ZIP-specific.
pub fn ingest_package(
    store: &mut FieldStore,
    descriptor_bytes: &[u8],
    limits: Limits,
) -> Result<PackageIngestReport> {
    ingest_package_with(store, descriptor_bytes, limits, None)
}

/// As [`ingest_package`], but with an optional bounded worker pool for the pure,
/// order-independent member encoding. `None` is byte-for-byte the serial path;
/// the serial merge below is what performs every store probe, write, counter
/// update, and index append, in member order, so the pool cannot change a node
/// id, a counter, or an index entry.
pub fn ingest_package_with(
    store: &mut FieldStore,
    descriptor_bytes: &[u8],
    limits: Limits,
    pool: Option<&WorkerPool>,
) -> Result<PackageIngestReport> {
    // Stage A: exact descriptor blob and exact source. The stored blob gains a
    // minimal advisory observation-index op table when it lacks one, so narrow
    // member observations can use the seek-based partial lane. Exactness is
    // unchanged; only the ignorable record is added.
    let observable = with_observation_index(descriptor_bytes, limits)?;
    let parsed = Descriptor::parse(&observable, limits)?;
    let source = crate::materialize::materialize(&parsed, limits)?;
    ingest_package_from_source(store, &observable, &parsed, &source, limits, pool)
}

/// Direct-build variant of [`ingest_package_with`] for the fixed-profile
/// [`crate::field::build`] path (Phase 18.2).
///
/// The caller already holds the exact original `source` and the already-enriched
/// `observable` authority blob. The authority is stored unchanged and verified to
/// materialize to `source`; the physical scan then runs over the **original**
/// bytes, so the source is never re-materialized for scanning. The member nodes,
/// index, counters, and manifest are byte-identical to [`ingest_package_with`].
pub(crate) fn ingest_package_direct(
    store: &mut FieldStore,
    observable: &[u8],
    source: &[u8],
    limits: Limits,
    pool: Option<&WorkerPool>,
) -> Result<PackageIngestReport> {
    let mut parsed = Descriptor::parse(observable, limits)?;
    let materialized = crate::materialize::materialize_in_place(&mut parsed, limits)?;
    if materialized.len() != source.len() || materialized != source {
        return Err(Error::reconstruction_mismatch(format!(
            "descriptor materialized {} bytes that differ from the supplied {} byte source",
            materialized.len(),
            source.len()
        )));
    }
    ingest_package_from_source(store, observable, &parsed, source, limits, pool)
}

/// Shared body of [`ingest_package_with`] / [`ingest_package_direct`]: `source`
/// is the exact materialization of the authority `observable`/`parsed`.
fn ingest_package_from_source(
    store: &mut FieldStore,
    observable: &[u8],
    parsed: &ParsedDescriptor,
    source: &[u8],
    limits: Limits,
    pool: Option<&WorkerPool>,
) -> Result<PackageIngestReport> {
    let source_len = source.len() as u64;
    // Byte-based format detection (never a file name), recorded in the manifest.
    let detected_format = crate::field::document_format::detect_document_format(source, limits);
    let descriptor_id = store.put_descriptor(observable)?;

    // The physical cover is the authority; `reemits` proves the cover is an exact
    // partition of the source, so `materialize(descriptor) == original_bytes`.
    let physical = scan(source, limits)?;
    physical.validate(source_len)?;
    physical.reemits(source)?;

    // The package field's root is the exact whole source. Its params are the
    // source's SHA-256 (a function of the source *bytes*, not of the
    // reconstruction program): two distinct sources must never share a root id,
    // because the root's output is the whole source and the shared derived cache
    // keys on the node id alone.
    let mut root = SeedNode::new(
        NodeKind::PackageRoot,
        source_len,
        crate::integrity::sha256(source).to_vec(),
        Vec::new(),
        "pkg:root",
    );
    root.limits.max_output_bytes = root.limits.max_output_bytes.max(source_len);
    let mut share = ShareCounters::default();
    let (root_id, _) = put_counted(store, &root, &mut share)?;

    let mut entries: Vec<IndexEntry> = Vec::new();
    let mut node_count: u64 = 1;
    let mut raw_nodes: u64 = 0;
    let mut decoded_nodes: u64 = 0;
    let mut declined_decodes: u64 = 0;
    let opc_model_nodes: u64;
    let docx_model_nodes: u64;
    let epub_model_nodes: u64;
    let odt_model_nodes: u64;
    let ods_model_nodes: u64;
    let xlsx_model_nodes: u64;
    let pptx_model_nodes: u64;
    let opc_model_id: Option<NodeId>;

    // Pure member encoding (canonical bytes + content ids) may run on the pool;
    // every store probe, write, counter, and index append stays in the serial
    // merge below, in member order. Indexed `par_iter().collect()` is ordered, so
    // `encoded[i]` is member `i` whether the pool is used or not.
    let encoded = encode_members(pool, source, &physical.members, root_id);
    for (member, enc) in physical.members.iter().zip(encoded.iter()) {
        let ordinal = member.id.ordinal;
        let (data_off, data_len) = member.data;

        // The exact leaf: the member's raw compressed/stored span.
        charge_node(&mut node_count)?;
        publish_counted(store, &enc.raw, &mut share)?;
        raw_nodes += 1;
        push_entry(
            &mut entries,
            IndexEntry {
                key: SelectorKey::new(SEL_PACKAGE_MEMBER_RAW, ordinal),
                out_off: data_off,
                out_len: data_len,
                node_id: enc.raw.id,
            },
        )?;

        // Cross-document sharing (Phase 12.8): a *stored* member whose bytes are a
        // recognized binary resource is recorded as one content-addressed blob.
        // The blob's id depends only on the bytes, so an identical resource in
        // another document resolves to this same node with nothing re-written, and
        // the decoded member below can depend on it (content identity, not the
        // per-source span) so the decoded state is shared too. A DEFLATE resource
        // keeps its per-source raw leaf: its two occurrences may differ in
        // compression, so byte identity is not guaranteed.
        if let Some((blob, blob_len)) = &enc.blob {
            let preexisting = publish_counted(store, blob, &mut share)?;
            share.resource_blob_nodes += 1;
            if preexisting {
                share.shared_resource_ids += 1;
                share.shared_resource_bytes = share.shared_resource_bytes.saturating_add(*blob_len);
            }
        }

        // Progressive decode: register the computation now; inflate only on demand.
        let Some(decoded) = &enc.decoded else {
            declined_decodes += 1;
            continue;
        };
        charge_node(&mut node_count)?;
        publish_counted(store, decoded, &mut share)?;
        decoded_nodes += 1;
        push_entry(
            &mut entries,
            IndexEntry {
                key: SelectorKey::new(SEL_PACKAGE_MEMBER_DECODED, ordinal),
                out_off: data_off,
                out_len: data_len,
                node_id: decoded.id,
            },
        )?;
    }

    // The generic OPC model (Phase 12.3): a single derived node that materializes
    // the canonical OPC graph on demand from the exact package root. It is created
    // here so an OPC observation resolves through the index without enumerating the
    // store; the XML parse happens only when the node is first materialized.
    #[cfg(feature = "opc")]
    {
        let mut model = SeedNode::new(
            NodeKind::PackageOpcModel,
            limits.max_output_bytes,
            Vec::new(),
            vec![root_id],
            "pkg:opc-model",
        );
        model.limits.max_output_bytes = limits.max_output_bytes;
        charge_node(&mut node_count)?;
        let (model_id, _) = put_counted(store, &model, &mut share)?;
        push_entry(
            &mut entries,
            IndexEntry {
                key: SelectorKey::new(SEL_OPC_MODEL, 0),
                out_off: 0,
                out_len: 0,
                node_id: model_id,
            },
        )?;
        opc_model_id = Some(model_id);
        opc_model_nodes = 1;
    }
    #[cfg(not(feature = "opc"))]
    {
        opc_model_id = None;
        opc_model_nodes = 0;
    }

    // The DOCX discovery model (Phase 12.4): a single derived node that resolves
    // the main part by the `officeDocument` relationship (never a hardcoded path)
    // and enumerates the story parts, computed on demand from the OPC model. It is
    // created for any package under the feature; a non-DOCX package simply declines
    // typed when the node is first materialized. Exactness is untouched.
    #[cfg(feature = "docx")]
    {
        let model_id = opc_model_id
            .ok_or_else(|| Error::internal_invariant("docx requires the OPC model node"))?;
        let mut docx_model = SeedNode::new(
            NodeKind::DocxModel,
            limits.max_output_bytes,
            Vec::new(),
            vec![model_id],
            "pkg:docx-model",
        );
        docx_model.limits.max_output_bytes = limits.max_output_bytes;
        charge_node(&mut node_count)?;
        let (docx_id, _) = put_counted(store, &docx_model, &mut share)?;
        push_entry(
            &mut entries,
            IndexEntry {
                key: SelectorKey::new(SEL_DOCX_MODEL, 0),
                out_off: 0,
                out_len: 0,
                node_id: docx_id,
            },
        )?;
        docx_model_nodes = 1;
    }
    #[cfg(not(feature = "docx"))]
    {
        let _ = opc_model_id;
        docx_model_nodes = 0;
    }

    // The EPUB (OCF) discovery model (Phase 12.5): a single derived node that
    // resolves the container rootfile semantically from `META-INF/container.xml`
    // (never a hardcoded `OEBPS/content.opf`) and parses the Package Document
    // metadata/manifest/spine on demand from the exact package source. It does
    // **not** route through OPC (EPUB has no `[Content_Types].xml`). It is created
    // for any package under the feature; a non-EPUB package simply declines typed
    // when the node is first materialized. Exactness is untouched.
    #[cfg(feature = "epub")]
    {
        let mut epub_model = SeedNode::new(
            NodeKind::EpubModel,
            limits.max_output_bytes,
            Vec::new(),
            vec![root_id],
            "pkg:epub-model",
        );
        epub_model.limits.max_output_bytes = limits.max_output_bytes;
        charge_node(&mut node_count)?;
        let (epub_id, _) = put_counted(store, &epub_model, &mut share)?;
        push_entry(
            &mut entries,
            IndexEntry {
                key: SelectorKey::new(SEL_EPUB_MODEL, 0),
                out_off: 0,
                out_len: 0,
                node_id: epub_id,
            },
        )?;
        epub_model_nodes = 1;
    }
    #[cfg(not(feature = "epub"))]
    {
        epub_model_nodes = 0;
    }

    // The ODT (ODF) discovery model (Phase 13.3): a single derived node that resolves
    // the main content part semantically from `META-INF/manifest.xml` (never a
    // hardcoded `content.xml`). It does **not** route through OPC (ODF has no
    // `[Content_Types].xml`). It is created for any package under the feature; a
    // non-ODT package simply declines typed when the node is first materialized.
    // Exactness is untouched.
    #[cfg(feature = "odt")]
    {
        let mut odt_model = SeedNode::new(
            NodeKind::OdtModel,
            limits.max_output_bytes,
            Vec::new(),
            vec![root_id],
            "pkg:odt-model",
        );
        odt_model.limits.max_output_bytes = limits.max_output_bytes;
        charge_node(&mut node_count)?;
        let (odt_id, _) = put_counted(store, &odt_model, &mut share)?;
        push_entry(
            &mut entries,
            IndexEntry {
                key: SelectorKey::new(SEL_ODT_MODEL, 0),
                out_off: 0,
                out_len: 0,
                node_id: odt_id,
            },
        )?;
        odt_model_nodes = 1;
    }
    #[cfg(not(feature = "odt"))]
    {
        odt_model_nodes = 0;
    }

    // The ODS (ODF spreadsheet) discovery model (Phase 21.3.1): a single derived node
    // that resolves the main content part semantically from `META-INF/manifest.xml`
    // (never a hardcoded `content.xml`). Like ODT it does **not** route through OPC
    // (ODF has no `[Content_Types].xml`). It is created for any package under the
    // feature; a non-ODS package simply declines typed when the node is first
    // materialized. Exactness is untouched.
    #[cfg(feature = "ods")]
    {
        let mut ods_model = SeedNode::new(
            NodeKind::OdsModel,
            limits.max_output_bytes,
            Vec::new(),
            vec![root_id],
            "pkg:ods-model",
        );
        ods_model.limits.max_output_bytes = limits.max_output_bytes;
        charge_node(&mut node_count)?;
        let (ods_id, _) = put_counted(store, &ods_model, &mut share)?;
        push_entry(
            &mut entries,
            IndexEntry {
                key: SelectorKey::new(SEL_ODS_MODEL, 0),
                out_off: 0,
                out_len: 0,
                node_id: ods_id,
            },
        )?;
        ods_model_nodes = 1;
    }
    #[cfg(not(feature = "ods"))]
    {
        ods_model_nodes = 0;
    }

    // The XLSX (SpreadsheetML) discovery model (Phase 21.1.1): a single derived
    // node that resolves the workbook part by the `officeDocument` relationship
    // and its SpreadsheetML content type (never a hardcoded `/xl/workbook.xml`)
    // and enumerates the worksheet parts, computed on demand from the OPC model.
    // It is created for any package under the feature; a non-XLSX package simply
    // declines typed when the node is first materialized. Exactness is untouched.
    #[cfg(feature = "xlsx")]
    {
        let model_id = opc_model_id
            .ok_or_else(|| Error::internal_invariant("xlsx requires the OPC model node"))?;
        let mut xlsx_model = SeedNode::new(
            NodeKind::XlsxModel,
            limits.max_output_bytes,
            Vec::new(),
            vec![model_id],
            "pkg:xlsx-model",
        );
        xlsx_model.limits.max_output_bytes = limits.max_output_bytes;
        charge_node(&mut node_count)?;
        let (xlsx_id, _) = put_counted(store, &xlsx_model, &mut share)?;
        push_entry(
            &mut entries,
            IndexEntry {
                key: SelectorKey::new(SEL_XLSX_MODEL, 0),
                out_off: 0,
                out_len: 0,
                node_id: xlsx_id,
            },
        )?;
        xlsx_model_nodes = 1;
    }
    #[cfg(not(feature = "xlsx"))]
    {
        xlsx_model_nodes = 0;
    }

    // The PPTX (PresentationML) discovery model (Phase 21.2.1): a single derived
    // node that resolves the presentation part by the `officeDocument` relationship
    // and its PresentationML content type (never a hardcoded `/ppt/presentation.xml`)
    // and enumerates the slide parts, computed on demand from the OPC model. It is
    // created for any package under the feature; a non-PPTX package simply declines
    // typed when the node is first materialized. Exactness is untouched.
    #[cfg(feature = "pptx")]
    {
        let model_id = opc_model_id
            .ok_or_else(|| Error::internal_invariant("pptx requires the OPC model node"))?;
        let mut pptx_model = SeedNode::new(
            NodeKind::PptxModel,
            limits.max_output_bytes,
            Vec::new(),
            vec![model_id],
            "pkg:pptx-model",
        );
        pptx_model.limits.max_output_bytes = limits.max_output_bytes;
        charge_node(&mut node_count)?;
        let (pptx_id, _) = put_counted(store, &pptx_model, &mut share)?;
        push_entry(
            &mut entries,
            IndexEntry {
                key: SelectorKey::new(SEL_PPTX_MODEL, 0),
                out_off: 0,
                out_len: 0,
                node_id: pptx_id,
            },
        )?;
        pptx_model_nodes = 1;
    }
    #[cfg(not(feature = "pptx"))]
    {
        pptx_model_nodes = 0;
    }

    let index_before = dir_bytes(&store.root().join("index"));
    let (index_root, index_node_count) = if entries.is_empty() {
        (None, 0)
    } else {
        let mut istore = FsIndexStore::open(store.root())?;
        let root = build(&mut istore, &entries)?;
        let (count, _depth) = validate(&istore, &root)?;
        (Some(root), count)
    };
    let index_bytes_written = dir_bytes(&store.root().join("index")).saturating_sub(index_before);

    let manifest = FieldRoot {
        universe_id: crate::container::universe_id_from_str(PACKAGE_UNIVERSE),
        source_sha256: parsed.descriptor.source_sha256,
        source_len,
        descriptor_id: *descriptor_id.as_bytes(),
        root_node: root_id,
        index_root: index_root.map_or(ABSENT_ROOT, |r| *r.as_bytes()),
        node_count,
        index_node_count,
        provenance: format!(
            "{}id_shared={};res_shared={};field:package;members={};raw={};decoded={};declined={};opc={};docx={};epub={};odt={};ods={};xlsx={};pptx={};resource_blobs={}",
            detected_format.provenance_prefix(),
            share.nodes_id_shared,
            share.shared_resource_ids,
            physical.members.len(),
            raw_nodes,
            decoded_nodes,
            declined_decodes,
            opc_model_nodes,
            docx_model_nodes,
            epub_model_nodes,
            odt_model_nodes,
            ods_model_nodes,
            xlsx_model_nodes,
            pptx_model_nodes,
            share.resource_blob_nodes,
        ),
    };
    let field = store.put_field(&manifest)?;

    Ok(PackageIngestReport {
        format: detected_format,
        field,
        root_node: root_id,
        index_root,
        node_count,
        index_node_count,
        source_len,
        member_count: physical.members.len() as u64,
        raw_nodes,
        decoded_nodes,
        declined_decodes,
        opc_model_nodes,
        docx_model_nodes,
        epub_model_nodes,
        odt_model_nodes,
        ods_model_nodes,
        xlsx_model_nodes,
        pptx_model_nodes,
        resource_blob_nodes: share.resource_blob_nodes,
        shared_resource_ids: share.shared_resource_ids,
        shared_resource_bytes: share.shared_resource_bytes,
        nodes_id_shared: share.nodes_id_shared,
        seed_bytes_written: share.seed_bytes_written,
        index_bytes_written,
    })
}

/// Total byte length of every regular file under `root` (0 when it does not
/// exist). A cheap physical witness for the procedural-index bytes one ingest
/// writes; never derived from a logical size.
fn dir_bytes(root: &std::path::Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(root) else {
        return 0;
    };
    let mut total = 0u64;
    for entry in entries.flatten() {
        match entry.metadata() {
            Ok(meta) if meta.is_file() => total = total.saturating_add(meta.len()),
            Ok(meta) if meta.is_dir() => total = total.saturating_add(dir_bytes(&entry.path())),
            _ => {}
        }
    }
    total
}
