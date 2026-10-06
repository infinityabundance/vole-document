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

use crate::adapter::package::zip::scan;
use crate::container::Descriptor;
use crate::error::{Error, Result};
use crate::field::index::{
    FsIndexStore, IndexEntry, SEL_PACKAGE_MEMBER_DECODED, SEL_PACKAGE_MEMBER_RAW, SelectorKey,
    build, validate,
};
use crate::field::ingest::{MAX_INGEST_INDEX_ENTRIES, MAX_INGEST_NODES, with_observation_index};
use crate::field::manifest::{ABSENT_ROOT, FieldRoot};
use crate::field::node::{NodeKind, SeedNode, object_params, span_params};
use crate::field::{FieldId, FieldStore, PACKAGE_UNIVERSE};
use crate::limits::Limits;
use crate::store::NodeId;

/// General-purpose flag bit 0: the member is encrypted.
const FLAG_ENCRYPTED: u16 = 0x0001;

/// What one package ingest recovered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageIngestReport {
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

fn push_entry(entries: &mut Vec<IndexEntry>, entry: IndexEntry) -> Result<()> {
    if entries.len() >= MAX_INGEST_INDEX_ENTRIES {
        return Err(Error::resource_limit(format!(
            "package ingest would exceed {MAX_INGEST_INDEX_ENTRIES} index entries"
        )));
    }
    entries.push(entry);
    Ok(())
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
    // Stage A: exact descriptor blob and exact source. The stored blob gains a
    // minimal advisory observation-index op table when it lacks one, so narrow
    // member observations can use the seek-based partial lane. Exactness is
    // unchanged; only the ignorable record is added.
    let observable = with_observation_index(descriptor_bytes, limits)?;
    let parsed = Descriptor::parse(&observable, limits)?;
    let source = crate::materialize::materialize(&parsed, limits)?;
    let source_len = source.len() as u64;
    let descriptor_id = store.put_descriptor(&observable)?;

    // The physical cover is the authority; `reemits` proves the cover is an exact
    // partition of the source, so `materialize(descriptor) == original_bytes`.
    let physical = scan(&source, limits)?;
    physical.validate(source_len)?;
    physical.reemits(&source)?;

    // The package field's root is the exact whole source.
    let mut root = SeedNode::new(
        NodeKind::PackageRoot,
        source_len,
        Vec::new(),
        Vec::new(),
        "pkg:root",
    );
    root.limits.max_output_bytes = root.limits.max_output_bytes.max(source_len);
    let root_id = store.seeds_mut().put_node(&root.encode_canonical())?;

    let mut entries: Vec<IndexEntry> = Vec::new();
    let mut node_count: u64 = 1;
    let mut raw_nodes: u64 = 0;
    let mut decoded_nodes: u64 = 0;
    let mut declined_decodes: u64 = 0;

    for member in &physical.members {
        let ordinal = member.id.ordinal;
        let (data_off, data_len) = member.data;

        // The exact leaf: the member's raw compressed/stored span.
        let mut raw = SeedNode::new(
            NodeKind::PackageMemberRaw,
            data_len,
            span_params(data_off, data_len),
            Vec::new(),
            "pkg:member-raw",
        );
        raw.limits.max_output_bytes = raw.limits.max_output_bytes.max(data_len);
        charge_node(&mut node_count)?;
        let raw_id = store.seeds_mut().put_node(&raw.encode_canonical())?;
        raw_nodes += 1;
        push_entry(
            &mut entries,
            IndexEntry {
                key: SelectorKey::new(SEL_PACKAGE_MEMBER_RAW, ordinal),
                out_off: data_off,
                out_len: data_len,
                node_id: raw_id,
            },
        )?;

        // Progressive decode: register the computation now; inflate only on demand.
        let decodable = member.method == 0 || member.method == 8;
        let encrypted = member.flags & FLAG_ENCRYPTED != 0;
        if !decodable || encrypted || data_len == 0 {
            declined_decodes += 1;
            continue;
        }
        let mut decoded = SeedNode::new(
            NodeKind::PackageMemberDecoded,
            member.uncompressed_size,
            object_params(ordinal, member.method, 0),
            vec![raw_id],
            "pkg:member-decoded",
        );
        decoded.limits.max_output_bytes = decoded
            .limits
            .max_output_bytes
            .max(member.uncompressed_size);
        charge_node(&mut node_count)?;
        let decoded_id = store.seeds_mut().put_node(&decoded.encode_canonical())?;
        decoded_nodes += 1;
        push_entry(
            &mut entries,
            IndexEntry {
                key: SelectorKey::new(SEL_PACKAGE_MEMBER_DECODED, ordinal),
                out_off: data_off,
                out_len: data_len,
                node_id: decoded_id,
            },
        )?;
    }

    let (index_root, index_node_count) = if entries.is_empty() {
        (None, 0)
    } else {
        let mut istore = FsIndexStore::open(store.root())?;
        let root = build(&mut istore, &entries)?;
        let (count, _depth) = validate(&istore, &root)?;
        (Some(root), count)
    };

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
            "field:package;members={};raw={};decoded={};declined={}",
            physical.members.len(),
            raw_nodes,
            decoded_nodes,
            declined_decodes
        ),
    };
    let field = store.put_field(&manifest)?;

    Ok(PackageIngestReport {
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
    })
}
