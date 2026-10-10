//! Bounded hierarchical observation index (Phase 11.3).
//!
//! An index is an **advisory accelerator**, never authority (ADR-0024, DEC-4). It
//! maps an observation *selector* — a page, object, stream, revision, or resource
//! identified by `(kind, number, generation)` — to an exact source byte span and
//! the seed node that serves it. A lying, corrupt, cyclic, out-of-depth, missing,
//! or oversized node is rejected fail-closed; the exact materialization path never
//! depends on the index.
//!
//! ## Why hierarchical
//!
//! A flat selector table is `Θ(document)`: one query reads the whole table. This
//! module is keyed so a lookup descends `root -> internal(s) -> leaf`, reading
//! only `O(depth)` small nodes and never a global table. `MAX_DEPTH = 3` bounds the
//! descent to at most `MAX_DEPTH + 1` node reads.
//!
//! ## Wire shape (little-endian, packed, no padding, no serde)
//!
//! ```text
//! header : magic:u8 | version:u8 | kind:u8 (0=leaf,1=internal) | depth:u8 | entry_count:u32
//! leaf   : kind:u8 | generation:u16 | number:u32 | out_off:u64 | out_len:u64 | node_id:[u8;32]
//! internal: min_kind:u8 | min_number:u32 | max_kind:u8 | max_number:u32 | child_id:[u8;32]
//! ```
//!
//! Nodes are stored content-addressed (`NodeId = BLAKE3-256("VOLE:PSEED:v1" || node
//! bytes)`), and every read verifies `NodeId::of_node(bytes) == id`.
//!
//! ## Bounds
//!
//! * `MAX_FANOUT = 256` entries per node (the absolute cap; the byte cap binds
//!   first — a leaf holds at most 148 entries and an internal at most 194).
//! * `MAX_DEPTH = 3` internal levels below which leaves sit at depth 0.
//! * `MAX_INDEX_NODE_BYTES = 8 KiB` per node.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::store::{IoCounters, NodeId};

/// Reserved HIER_INDEX record tag (`RecordTag::HierIndex`), reused as the node
/// magic so an index node is self-identifying off the wire.
pub const INDEX_MAGIC: u8 = 0x72;
/// Canonical index-node format version.
pub const INDEX_VERSION: u8 = 1;
/// Absolute cap on entries in any node (the byte cap binds first for large entries).
pub const MAX_FANOUT: usize = 256;
/// Maximum internal depth; leaves always sit at depth 0.
pub const MAX_DEPTH: u8 = 3;
/// Maximum encoded size of a single node.
pub const MAX_INDEX_NODE_BYTES: usize = 8 * 1024;

/// Selector kind: a PDF page.
pub const SEL_PAGE: u8 = 1;
/// Selector kind: a PDF indirect object.
pub const SEL_OBJECT: u8 = 2;
/// Selector kind: an encoded stream.
pub const SEL_STREAM: u8 = 3;
/// Selector kind: a document revision.
pub const SEL_REVISION: u8 = 4;
/// Selector kind: a named resource.
pub const SEL_RESOURCE: u8 = 5;
/// Selector kind: a **decoded** stream, keyed by its owning object number.
///
/// A `PdfStreamDecoded` node is a deterministic function of its encoded node,
/// the materializer, and the decoded length, so it gets its own index entry
/// (Phase 11.9 review fix #3). This lets a `Stream(n) + DecodedBytes`/`Operators`
/// observation resolve in `O(depth)` index reads instead of enumerating the
/// whole seed store.
pub const SEL_STREAM_DECODED: u8 = 6;
/// Selector kind: a package (ZIP/OCF/OPC) member's exact raw compressed/stored
/// span, keyed by the member's central-directory **ordinal** (Phase 12.2).
///
/// The key number is the physical ordinal ([`crate::adapter::package::PhysicalMemberId`]'s
/// `ordinal`), never the member name: duplicate names therefore stay distinct.
pub const SEL_PACKAGE_MEMBER_RAW: u8 = 7;
/// Selector kind: a package member's decoded bytes, keyed by the same ordinal.
///
/// A `PackageMemberDecoded` node is a deterministic function of its raw node and
/// method, so it gets its own entry (mirroring `SEL_STREAM_DECODED`); a
/// `Member(n) + DecodedBytes` observation resolves in `O(depth)` index reads.
pub const SEL_PACKAGE_MEMBER_DECODED: u8 = 8;
/// Selector kind: the generic OPC package model (Phase 12.3).
///
/// There is exactly one entry, keyed by number `0`, whose node materializes the
/// canonical OPC graph (content types + parts + package/part relationships) as
/// `Q_gen` derived state. It is computed on demand from the exact package source,
/// never eagerly at ingest, and the exact bytes remain the 12.2 member raw spans.
pub const SEL_OPC_MODEL: u8 = 9;
/// Selector kind: the canonical DOCX discovery model (Phase 12.4).
///
/// There is exactly one entry, keyed by number `0`, whose node materializes the
/// DOCX main-part/story discovery (main part via the `officeDocument`
/// relationship, styles, headers/footers, notes, comments) as `Q_gen` derived
/// state, computed on demand from the OPC model.
pub const SEL_DOCX_MODEL: u8 = 10;
/// Selector kind: the canonical EPUB (OCF) discovery model (Phase 12.5).
///
/// There is exactly one entry, keyed by number `0`, whose node materializes the
/// EPUB container + Package Document graph (`mimetype` facts, rootfiles, metadata,
/// manifest, spine, nav identity) as `Q_gen` derived state, computed on demand from
/// the exact package source — never via OPC (EPUB has no `[Content_Types].xml`).
pub const SEL_EPUB_MODEL: u8 = 11;
/// Selector kind: the canonical ODT (ODF) discovery model (Phase 13.3).
///
/// There is exactly one entry, keyed by number `0`, whose node materializes the
/// ODT package graph (`mimetype` facts and the parsed `META-INF/manifest.xml` file
/// entries, with the main content part resolved semantically) as `Q_gen` derived
/// state, computed on demand from the exact package source — never via OPC (ODF has
/// no `[Content_Types].xml`).
pub const SEL_ODT_MODEL: u8 = 12;
/// Selector kind: the PDF **revision lineage** as a whole (Phase 17).
///
/// There is exactly one entry, keyed by number `0`, whose node materializes the
/// PDF's incremental revision chain (count, ordered indices, byte spans,
/// `startxref`/`/Prev`, object/stream membership) as derived state, computed once
/// at ingest from the byte-authoritative physical scan. A field with no revision
/// structure has no such entry, so the observation is a typed decline.
pub const SEL_REVISIONS: u8 = 13;
/// Selector kind: one PDF revision's lineage entry (Phase 17), keyed by the
/// revision's 0-based index. Its node materializes that revision's lineage JSON;
/// the entry's span is the revision's exact source span.
pub const SEL_REVISION_LINEAGE: u8 = 14;
/// Selector kind: the canonical XLSX (SpreadsheetML) discovery model (Phase 21.1.1).
///
/// There is exactly one entry, keyed by number `0`, whose node materializes the
/// XLSX workbook-part/styles/shared-strings/worksheet discovery as `Q_gen` derived
/// state, computed on demand from the OPC model. The decoded workbook inventory
/// and each worksheet's cell model are computed on demand from their decoded
/// member nodes (no index entry of their own, mirroring `DocxStory`).
pub const SEL_XLSX_MODEL: u8 = 15;
/// Selector kind: the canonical PPTX (PresentationML) discovery model (Phase 21.2.1).
///
/// There is exactly one entry, keyed by number `0`, whose node materializes the
/// PPTX presentation-part/masters/layouts/themes/media/slide discovery as `Q_gen`
/// derived state, computed on demand from the OPC model. The parsed presentation
/// inventory and each slide's shape model are computed on demand from their
/// decoded member nodes (no index entry of their own, mirroring `XlsxSheet`).
pub const SEL_PPTX_MODEL: u8 = 16;
/// Selector kind: the canonical ODS (ODF spreadsheet) discovery model
/// (Phase 21.3.1).
///
/// There is exactly one entry, keyed by number `0`, whose node materializes the
/// ODS package graph (`mimetype` facts and the parsed `META-INF/manifest.xml` file
/// entries, with the main content part resolved semantically) as `Q_gen` derived
/// state, computed on demand from the exact package source — never via OPC (ODF has
/// no `[Content_Types].xml`). The decoded spreadsheet content model and the styles
/// model are computed on demand from their decoded member nodes (no index entry of
/// their own, mirroring `OdtContent`).
pub const SEL_ODS_MODEL: u8 = 17;
/// Selector kind: the canonical ODP (ODF presentation) discovery model
/// (Phase 21.4.1).
///
/// There is exactly one entry, keyed by number `0`, whose node materializes the
/// ODP package graph (`mimetype` facts and the parsed `META-INF/manifest.xml` file
/// entries, with the main content part and the `Pictures/*` media parts resolved
/// semantically) as `Q_gen` derived state, computed on demand from the exact
/// package source — never via OPC (ODF has no `[Content_Types].xml`). The decoded
/// presentation content model and the styles model are computed on demand from
/// their decoded member nodes (no index entry of their own, mirroring `OdsContent`).
pub const SEL_ODP_MODEL: u8 = 18;
/// Selector kind: the canonical JSON structured-tree model (Phase 21.5.1).
///
/// There is exactly one entry, keyed by number `0`, whose node materializes the
/// whole source parsed as exactly one JSON value into a bounded,
/// representation-preserving arena (token spans, member order, duplicate keys,
/// spelling) as `Q_gen` derived state. It is computed on demand from the exact
/// source (its single dependency is the `DocumentExact` root, keyed by
/// `sha256(source)` per ADR-0060); JSON has no package layer.
pub const SEL_JSON_MODEL: u8 = 19;
/// Selector kind: the canonical YAML structured-tree model (Phase 21.6.1).
///
/// There is exactly one entry, keyed by number `0`, whose node materializes the
/// whole source parsed as a bounded stream of YAML documents into a bounded,
/// representation-preserving arena (node spans/kinds, mapping order, duplicate
/// keys, anchors/aliases as a graph, tags, scalar styles, comments) as `Q_gen`
/// derived state. It is computed on demand from the exact source (its single
/// dependency is the `DocumentExact` root, keyed by `sha256(source)` per
/// ADR-0060); YAML has no package layer.
pub const SEL_YAML_MODEL: u8 = 20;
/// Selector kind: the canonical CSV/TSV tabular model (Phase 21.7.1).
///
/// There is exactly one entry, keyed by number `0`, whose node materializes the
/// whole source parsed as a bounded CSV/TSV table into a bounded,
/// representation-preserving arena (record/field spans, dialect, header) as
/// `Q_gen` derived state. It is computed on demand from the exact source (its
/// single dependency is the `DocumentExact` root, keyed by `sha256(source)` per
/// ADR-0060); CSV/TSV has no package layer.
pub const SEL_CSV_MODEL: u8 = 21;
/// Selector kind: the canonical Markdown prose model (Phase 21.8.1).
///
/// There is exactly one entry, keyed by number `0`, whose node materializes the
/// whole source parsed as a bounded, representation-preserving prose model (block
/// and inline spans, headings/levels, lists, code, blockquotes, tables, links,
/// reference definitions, footnotes, front matter) as `Q_gen` derived state. It is
/// computed on demand from the exact source (its single dependency is the
/// `DocumentExact` root, keyed by `sha256(source)` per ADR-0060); Markdown has no
/// package layer.
pub const SEL_MARKDOWN_MODEL: u8 = 22;
/// Selector kind: the canonical XML structured-tree model (Phase 21.9).
///
/// There is exactly one entry, keyed by number `0`, whose node materializes the
/// whole source parsed as well-formed XML into a bounded, representation-preserving
/// arena (element/attribute/text/CDATA/comment/PI/DOCTYPE/namespace spans, in
/// document order) as `Q_gen` derived state. It is computed on demand from the
/// exact source (its single dependency is the `DocumentExact` root, keyed by
/// `sha256(source)` per ADR-0060); XML has no package layer.
pub const SEL_XML_MODEL: u8 = 23;
/// Selector kind: the canonical HTML document model (Phase 21.10).
///
/// There is exactly one entry, keyed by number `0`, whose node materializes the
/// whole source parsed as a bounded, **error-recovering**, representation-preserving
/// HTML arena (element/attribute/text/comment/DOCTYPE/raw-`script`-`style` spans, in
/// document order) as `Q_gen` derived state. It is computed on demand from the exact
/// source (its single dependency is the `DocumentExact` root, keyed by
/// `sha256(source)` per ADR-0060); HTML has no package layer.
pub const SEL_HTML_MODEL: u8 = 24;
/// Selector kind: the canonical TOML structured-tree model (Phase 21.11).
///
/// There is exactly one entry, keyed by number `0`, whose node materializes the
/// whole source parsed as TOML into a bounded, representation-preserving arena
/// (table/array/inline-table/key/value/comment spans, dotted keys, exact scalar
/// spelling) as `Q_gen` derived state. It is computed on demand from the exact
/// source (its single dependency is the `DocumentExact` root, keyed by
/// `sha256(source)` per ADR-0060); TOML has no package layer.
pub const SEL_TOML_MODEL: u8 = 25;
/// Selector kind: the canonical per-line JSONL/NDJSON model (Phase 21.12).
///
/// There is exactly one entry, keyed by number `0`, whose node materializes the
/// whole source split into physical lines, each non-blank line parsed by the shared
/// JSON parser into a bounded, representation-preserving record (exact line span and
/// terminator, member order, duplicate keys, token spelling and spans) as `Q_gen`
/// derived state. It is computed on demand from the exact source (its single
/// dependency is the `DocumentExact` root, keyed by `sha256(source)` per
/// ADR-0060); JSONL has no package layer. Derived, never exact.
pub const SEL_JSONL_MODEL: u8 = 26;
/// Selector kind: the canonical EML/MIME message model (Phase 21.13).
///
/// There is exactly one entry, keyed by number `0`, whose node materializes the
/// whole source parsed as a bounded, representation-preserving RFC 5322 + MIME
/// message (every header's exact spans and order, duplicate headers, the
/// `multipart/*` tree, nested `message/rfc822`, and the exact
/// `Content-Transfer-Encoding`-decoded constituents) as `Q_gen` derived state. It is
/// computed on demand from the exact source (its single dependency is the
/// `DocumentExact` root, keyed by `sha256(source)` per ADR-0060); EML has no package
/// layer.
pub const SEL_EML_MODEL: u8 = 27;
/// Selector kind: the canonical bounded Parquet model (Phase 21.14).
///
/// There is exactly one entry, keyed by number `0`, whose node materializes the
/// whole source parsed into the derived Parquet inventory (the flattened schema, the
/// logical leaf columns, and the row-group/column-chunk descriptors with exact
/// source spans and statistics) as `Q_gen` derived state. It is computed on demand
/// from the exact source (its single dependency is the `DocumentExact` root, keyed by
/// `sha256(source)` per ADR-0060); Parquet has no package layer. Derived, never
/// exact.
pub const SEL_PARQUET_MODEL: u8 = 28;

/// Node kind: a run of leaf entries.
const KIND_LEAF: u8 = 0;
/// Node kind: a run of child pointers.
const KIND_INTERNAL: u8 = 1;

/// Encoded header length.
const HEADER_LEN: usize = 8;
/// One leaf entry: kind(1) + generation(2) + number(4) + out_off(8) + out_len(8)
/// + node_id(32).
const LEAF_ENTRY_LEN: usize = 1 + 2 + 4 + 8 + 8 + 32;
/// One internal entry: min_kind(1) + min_number(4) + max_kind(1) + max_number(4)
/// + child_id(32).
const INTERNAL_ENTRY_LEN: usize = 1 + 4 + 1 + 4 + 32;

/// Effective leaf capacity: the fanout cap and the node-size cap, whichever binds.
const MAX_LEAF_ENTRIES: usize = {
    let by_bytes = (MAX_INDEX_NODE_BYTES - HEADER_LEN) / LEAF_ENTRY_LEN;
    if MAX_FANOUT < by_bytes {
        MAX_FANOUT
    } else {
        by_bytes
    }
};

/// Effective internal fanout: the fanout cap and the node-size cap, whichever binds.
const MAX_INTERNAL_CHILDREN: usize = {
    let by_bytes = (MAX_INDEX_NODE_BYTES - HEADER_LEN) / INTERNAL_ENTRY_LEN;
    if MAX_FANOUT < by_bytes {
        MAX_FANOUT
    } else {
        by_bytes
    }
};

/// A selector key: an observation kind, its number, and its generation.
///
/// Ordering is lexicographic by `(kind, number, generation)`, which is the
/// canonical index order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SelectorKey {
    /// [`SEL_PAGE`], [`SEL_OBJECT`], [`SEL_STREAM`], [`SEL_STREAM_DECODED`],
    /// [`SEL_REVISION`], [`SEL_RESOURCE`], [`SEL_PACKAGE_MEMBER_RAW`],
    /// [`SEL_PACKAGE_MEMBER_DECODED`], [`SEL_OPC_MODEL`], [`SEL_REVISIONS`], or
    /// [`SEL_REVISION_LINEAGE`].
    pub kind: u8,
    /// The page/object/stream/revision/resource number, or a package member's
    /// central-directory ordinal.
    pub number: u32,
    /// The generation (`0` where the kind has none).
    pub generation: u16,
}

impl SelectorKey {
    /// A key with generation `0`.
    pub const fn new(kind: u8, number: u32) -> Self {
        SelectorKey {
            kind,
            number,
            generation: 0,
        }
    }

    /// A key with an explicit generation.
    pub const fn with_generation(kind: u8, number: u32, generation: u16) -> Self {
        SelectorKey {
            kind,
            number,
            generation,
        }
    }
}

/// One resolved observation: where its bytes live and which seed node serves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    /// The selector this entry answers.
    pub key: SelectorKey,
    /// Start of the exact source byte span.
    pub out_off: u64,
    /// Length of the exact source byte span.
    pub out_len: u64,
    /// The seed node that materializes the span.
    pub node_id: NodeId,
}

/// The reference index substrate: one file per node under `<root>/index`.
///
/// Layout: `<root>/index/<aa>/<bb>/<64-hex>` where `aa`/`bb` are the first two
/// bytes of the id in hex. Writes are atomic (`tmp -> fsync -> rename`); reads are
/// hash-verified by [`FsIndexStore::get`].
pub struct FsIndexStore {
    root: PathBuf,
    io: IoCounters,
}

impl std::fmt::Debug for FsIndexStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FsIndexStore")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl FsIndexStore {
    /// Open (creating if needed) an index store rooted at `root`, using
    /// `root/index`, with a fresh, private I/O counter set.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_io(root, IoCounters::new())
    }

    /// Open an index store that accounts every node read against `io`.
    pub fn open_with_io(root: impl AsRef<Path>, io: IoCounters) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        crate::store::durable::create_dir_all(&root.join("index"))?;
        Ok(FsIndexStore { root, io })
    }

    /// Store one canonical node, returning its content id. Idempotent and atomic.
    pub fn put(&mut self, canonical: &[u8]) -> Result<NodeId> {
        let id = NodeId::of_node(canonical);
        let path = self.node_path(&id);
        if path.exists() {
            return Ok(id);
        }
        let dir = path
            .parent()
            .ok_or_else(|| Error::internal_invariant("index node path has no parent"))?;
        crate::store::durable::create_dir_all(dir)?;
        let tmp = dir.join(format!(".{}.tmp-{}", id.to_hex(), std::process::id()));
        {
            let mut f = crate::store::durable::create_file(&tmp)?;
            crate::store::durable::write_all(&mut f, &tmp, canonical)?;
            crate::store::durable::sync_all(&f, &tmp)?;
        }
        crate::store::durable::rename(&tmp, &path)?;
        // Phase 23, GAP 1: make the rename durable in the parent directory.
        crate::store::durable::sync_dir(dir)?;
        Ok(id)
    }

    /// Fetch a node, verifying `NodeId::of_node(bytes) == id`.
    pub fn get(&self, id: &NodeId) -> Result<Vec<u8>> {
        let path = self.node_path(id);
        let bytes = fs::read(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::missing_external_object(format!("index node {id} is not present"))
            } else {
                Error::io(format!("reading index node {id}: {e}"))
            }
        })?;
        let actual = NodeId::of_node(&bytes);
        if actual != *id {
            return Err(Error::integrity_mismatch(format!(
                "index node {id} content hashes to {actual}"
            )));
        }
        self.io.add_index(bytes.len() as u64);
        Ok(bytes)
    }

    /// Whether a node id is present.
    pub fn contains(&self, id: &NodeId) -> Result<bool> {
        Ok(self.node_path(id).exists())
    }

    /// Every node id physically present in the index namespace.
    ///
    /// Used to tell a genuinely new node from one a previous build already wrote
    /// (the index is content-addressed, so an unchanged node has an unchanged
    /// id and is never rewritten).
    pub fn list_ids(&self) -> Result<Vec<NodeId>> {
        let mut out: Vec<NodeId> = Vec::new();
        let mut stack = vec![self.root.join("index")];
        while let Some(dir) = stack.pop() {
            let entries = match fs::read_dir(&dir) {
                Ok(e) => e,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if let Some(name) = path.file_name().and_then(|n| n.to_str())
                    && name.len() == 64
                    && let Ok(id) = NodeId::from_hex(name)
                {
                    out.push(id);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        Ok(out)
    }

    /// Number of stored nodes.
    pub fn count(&self) -> Result<u64> {
        let mut n = 0u64;
        let mut stack = vec![self.root.join("index")];
        while let Some(dir) = stack.pop() {
            let entries = match fs::read_dir(&dir) {
                Ok(e) => e,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if let Some(name) = path.file_name().and_then(|n| n.to_str())
                    && name.len() == 64
                    && NodeId::from_hex(name).is_ok()
                {
                    n += 1;
                }
            }
        }
        Ok(n)
    }

    fn node_path(&self, id: &NodeId) -> PathBuf {
        let hex = id.to_hex();
        self.root
            .join("index")
            .join(&hex[0..2])
            .join(&hex[2..4])
            .join(&hex)
    }
}

/// The minimal read surface a descent needs. Implemented by [`FsIndexStore`] and,
/// in tests only, by a counting wrapper.
trait NodeReader {
    fn read_node(&self, id: &NodeId) -> Result<Vec<u8>>;
}

impl NodeReader for FsIndexStore {
    fn read_node(&self, id: &NodeId) -> Result<Vec<u8>> {
        self.get(id)
    }
}

/// A half-open `(kind, number)` interval covered by one child pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct KeyRange {
    min_kind: u8,
    min_number: u32,
    max_kind: u8,
    max_number: u32,
}

impl KeyRange {
    /// Whether this range can contain `key` (generation is not part of a range).
    fn contains(&self, key: &SelectorKey) -> bool {
        self.contains_pair(key.kind, key.number)
    }

    /// Whether `(kind, number)` lies within this range.
    fn contains_pair(&self, kind: u8, number: u32) -> bool {
        (self.min_kind, self.min_number) <= (kind, number)
            && (kind, number) <= (self.max_kind, self.max_number)
    }

    /// Whether `other` is fully contained in this range.
    fn contains_range(&self, other: &KeyRange) -> bool {
        (self.min_kind, self.min_number) <= (other.min_kind, other.min_number)
            && (other.max_kind, other.max_number) <= (self.max_kind, self.max_number)
    }
}

/// A child pointer in an internal node.
#[derive(Debug, Clone, Copy)]
struct ChildRef {
    range: KeyRange,
    child_id: NodeId,
}

/// A decoded node. Exactly one of `leaf`/`internal` is populated, per `kind`.
#[derive(Debug)]
struct DecodedNode {
    kind: u8,
    depth: u8,
    leaf: Vec<IndexEntry>,
    internal: Vec<ChildRef>,
}

/// One pending descent step in validation.
#[derive(Debug, Clone, Copy)]
struct Descend {
    id: NodeId,
    expected_depth: Option<u8>,
    expected_range: Option<KeyRange>,
}

/// Canonical total order for entries and query results.
fn entry_order(a: &IndexEntry, b: &IndexEntry) -> std::cmp::Ordering {
    a.key
        .cmp(&b.key)
        .then(a.out_off.cmp(&b.out_off))
        .then(a.out_len.cmp(&b.out_len))
        .then(a.node_id.cmp(&b.node_id))
}

/// Build a canonical index tree over `entries`; returns the root id.
///
/// Entries are de-duplicated and sorted by `(kind, number, generation, out_off)`
/// (a total order over the remaining fields breaks ties), so a shuffled input
/// yields exactly one canonical tree. Returns [`crate::ErrorClass::ResourceLimit`]
/// if a node would exceed [`MAX_INDEX_NODE_BYTES`], if depth would exceed
/// [`MAX_DEPTH`], or if fanout caps are violated.
pub fn build(store: &mut FsIndexStore, entries: &[IndexEntry]) -> Result<NodeId> {
    let mut sorted = entries.to_vec();
    sorted.sort_by(entry_order);
    sorted.dedup();

    if sorted.is_empty() {
        return store.put(&encode_leaf(&[], 0)?);
    }

    let mut level: Vec<ChildRef> = Vec::new();
    for chunk in sorted.chunks(MAX_LEAF_ENTRIES) {
        let bytes = encode_leaf(chunk, 0)?;
        let id = store.put(&bytes)?;
        level.push(child_from_leaf(chunk, id)?);
    }

    let mut depth = 0u8;
    while level.len() > 1 {
        if depth == MAX_DEPTH {
            return Err(Error::resource_limit(format!(
                "index would exceed MAX_DEPTH {MAX_DEPTH}"
            )));
        }
        depth += 1;
        let mut next: Vec<ChildRef> = Vec::new();
        for chunk in level.chunks(MAX_INTERNAL_CHILDREN) {
            let bytes = encode_internal(chunk, depth)?;
            let id = store.put(&bytes)?;
            next.push(child_from_internal(chunk, id)?);
        }
        level = next;
    }

    level
        .pop()
        .map(|root| root.child_id)
        .ok_or_else(|| Error::internal_invariant("index build produced no node"))
}

/// Look up all entries whose key exactly matches `key`.
///
/// Every node on the descent is validated: framing, magic/version/kind/depth,
/// content-id binding, and strictly decreasing depth. A corrupt, lying, missing,
/// oversized, or out-of-depth node is a typed error, never a silent empty result.
/// Only the nodes on the `root -> internal(s) -> leaf` path are read.
pub fn lookup(store: &FsIndexStore, root: &NodeId, key: &SelectorKey) -> Result<Vec<IndexEntry>> {
    crate::field::prof::inc_lookup();
    lookup_impl(store, root, key)
}

/// Validate an entire tree. Returns the number of distinct nodes and the maximum
/// depth observed. Any structural fault, identity mismatch, inconsistent
/// duplicate, dangling child, or range violation is a typed error.
pub fn validate(store: &FsIndexStore, root: &NodeId) -> Result<(u64, u8)> {
    let (count, depth, _ids) = validate_impl_nodes(store, root)?;
    Ok((count, depth))
}

/// Like [`validate`], but also returns every distinct tree-node id. A caller that
/// must compare two trees can validate and enumerate in one pass instead of
/// reading the same nodes twice.
pub fn validate_nodes(store: &FsIndexStore, root: &NodeId) -> Result<(u64, u8, Vec<NodeId>)> {
    validate_impl_nodes(store, root)
}

/// A single full traversal of an index tree: every leaf entry and every
/// distinct tree-node id.
///
/// Both are collected in one pass so a caller that needs to carry untouched
/// selector bindings forward (the immutable-edit witness) does not read the
/// tree twice. Every node read is hash-checked by [`FsIndexStore::get`].
pub struct TreeInspection {
    /// Every leaf entry, sorted and deduplicated by [`entry_order`].
    pub entries: Vec<IndexEntry>,
    /// Every distinct node id reachable from the root (leaves and internals).
    pub nodes: Vec<NodeId>,
}

/// Traverse the whole tree rooted at `root`, collecting every leaf entry and
/// every distinct node id. This reads every node (not just a lookup path).
pub fn inspect(store: &FsIndexStore, root: &NodeId) -> Result<TreeInspection> {
    let mut entries: Vec<IndexEntry> = Vec::new();
    let mut nodes: Vec<NodeId> = Vec::new();
    let mut stack: Vec<NodeId> = vec![*root];
    while let Some(id) = stack.pop() {
        if nodes.contains(&id) {
            continue;
        }
        let bytes = store.get(&id)?;
        let node = parse_node(&bytes)?;
        nodes.push(id);
        match node.kind {
            KIND_LEAF => entries.extend(node.leaf),
            _ => {
                for c in node.internal {
                    stack.push(c.child_id);
                }
            }
        }
    }
    entries.sort_by(entry_order);
    entries.dedup();
    Ok(TreeInspection { entries, nodes })
}

fn lookup_impl<R: NodeReader>(
    store: &R,
    root: &NodeId,
    key: &SelectorKey,
) -> Result<Vec<IndexEntry>> {
    let mut out: Vec<IndexEntry> = Vec::new();
    let mut stack: Vec<(NodeId, Option<u8>)> = vec![(*root, None)];
    while let Some((id, expected)) = stack.pop() {
        let t_read = crate::field::prof::start();
        let bytes = store.read_node(&id)?;
        crate::field::prof::add_index_read(t_read);
        crate::field::prof::inc_index_node();
        let t_parse = crate::field::prof::start();
        let node = parse_node(&bytes)?;
        crate::field::prof::add_index_parse(t_parse);
        if let Some(exp) = expected
            && node.depth != exp
        {
            return Err(Error::integrity_mismatch(format!(
                "index node {id} declares depth {} but was reached at depth {exp}",
                node.depth
            )));
        }
        match node.kind {
            KIND_LEAF => {
                for e in node.leaf {
                    if e.key == *key {
                        out.push(e);
                    }
                }
            }
            _ => {
                let child_depth = node
                    .depth
                    .checked_sub(1)
                    .ok_or_else(|| Error::integrity_mismatch("index internal node has depth 0"))?;
                for child in node.internal {
                    if child.range.contains(key) {
                        stack.push((child.child_id, Some(child_depth)));
                    }
                }
            }
        }
    }
    out.sort_by(entry_order);
    out.dedup();
    Ok(out)
}

fn validate_impl_nodes<R: NodeReader>(store: &R, root: &NodeId) -> Result<(u64, u8, Vec<NodeId>)> {
    let mut seen: Vec<(NodeId, Vec<u8>, u8)> = Vec::new();
    let mut count = 0u64;
    let mut max_depth = 0u8;
    let mut stack: Vec<Descend> = vec![Descend {
        id: *root,
        expected_depth: None,
        expected_range: None,
    }];
    while let Some(step) = stack.pop() {
        let bytes = store.read_node(&step.id)?;
        let node = parse_node(&bytes)?;
        if let Some(exp) = step.expected_depth
            && node.depth != exp
        {
            return Err(Error::integrity_mismatch(format!(
                "index node {} declares depth {} but was reached at depth {exp}",
                step.id, node.depth
            )));
        }
        if let Some(range) = step.expected_range {
            for e in &node.leaf {
                if !range.contains_pair(e.key.kind, e.key.number) {
                    return Err(Error::integrity_mismatch(format!(
                        "index leaf entry {:?} lies outside its parent range",
                        e.key
                    )));
                }
            }
            for c in &node.internal {
                if !range.contains_range(&c.range) {
                    return Err(Error::integrity_mismatch(format!(
                        "index child range {:?} exceeds its parent range",
                        c.range
                    )));
                }
            }
        }
        if !note_node(&mut seen, step.id, &bytes, node.depth)? {
            continue;
        }
        count += 1;
        if node.depth > max_depth {
            max_depth = node.depth;
        }
        if node.kind == KIND_INTERNAL {
            validate_child_order(&node.internal)?;
            let child_depth = node
                .depth
                .checked_sub(1)
                .ok_or_else(|| Error::integrity_mismatch("index internal node has depth 0"))?;
            for c in node.internal {
                stack.push(Descend {
                    id: c.child_id,
                    expected_depth: Some(child_depth),
                    expected_range: Some(c.range),
                });
            }
        }
    }
    Ok((
        count,
        max_depth,
        seen.into_iter().map(|(id, _, _)| id).collect(),
    ))
}

/// Record a visited node. Returns `true` if newly seen and `false` if already
/// visited. A repeated id with differing bytes, or at an inconsistent depth, is
/// an [`crate::ErrorClass::IntegrityMismatch`].
fn note_node(
    seen: &mut Vec<(NodeId, Vec<u8>, u8)>,
    id: NodeId,
    bytes: &[u8],
    depth: u8,
) -> Result<bool> {
    if let Some((_, prev, prev_depth)) = seen.iter().find(|(seen_id, _, _)| *seen_id == id) {
        if prev.as_slice() != bytes {
            return Err(Error::integrity_mismatch(format!(
                "index node {id} decoded twice with differing bytes"
            )));
        }
        if *prev_depth != depth {
            return Err(Error::integrity_mismatch(format!(
                "index node {id} appears at inconsistent depths"
            )));
        }
        return Ok(false);
    }
    seen.push((id, bytes.to_vec(), depth));
    Ok(true)
}

/// Check that internal children declare `min <= max` and ascend by `min`.
fn validate_child_order(children: &[ChildRef]) -> Result<()> {
    let mut prev_min: Option<(u8, u32)> = None;
    for c in children {
        let min = (c.range.min_kind, c.range.min_number);
        let max = (c.range.max_kind, c.range.max_number);
        if min > max {
            return Err(Error::integrity_mismatch(
                "index internal child has min greater than max",
            ));
        }
        if let Some(prev) = prev_min
            && min < prev
        {
            return Err(Error::integrity_mismatch(
                "index internal children are not sorted",
            ));
        }
        prev_min = Some(min);
    }
    Ok(())
}

fn child_from_leaf(chunk: &[IndexEntry], id: NodeId) -> Result<ChildRef> {
    let first = chunk
        .first()
        .ok_or_else(|| Error::internal_invariant("empty leaf chunk"))?;
    let last = chunk
        .last()
        .ok_or_else(|| Error::internal_invariant("empty leaf chunk"))?;
    Ok(ChildRef {
        range: KeyRange {
            min_kind: first.key.kind,
            min_number: first.key.number,
            max_kind: last.key.kind,
            max_number: last.key.number,
        },
        child_id: id,
    })
}

fn child_from_internal(chunk: &[ChildRef], id: NodeId) -> Result<ChildRef> {
    let first = chunk
        .first()
        .ok_or_else(|| Error::internal_invariant("empty internal chunk"))?;
    let last = chunk
        .last()
        .ok_or_else(|| Error::internal_invariant("empty internal chunk"))?;
    Ok(ChildRef {
        range: KeyRange {
            min_kind: first.range.min_kind,
            min_number: first.range.min_number,
            max_kind: last.range.max_kind,
            max_number: last.range.max_number,
        },
        child_id: id,
    })
}

fn push_header(out: &mut Vec<u8>, kind: u8, depth: u8, count: u32) {
    out.push(INDEX_MAGIC);
    out.push(INDEX_VERSION);
    out.push(kind);
    out.push(depth);
    out.extend_from_slice(&count.to_le_bytes());
}

fn encode_leaf(entries: &[IndexEntry], depth: u8) -> Result<Vec<u8>> {
    if entries.len() > MAX_FANOUT {
        return Err(Error::resource_limit(format!(
            "index leaf entry_count {} exceeds MAX_FANOUT {MAX_FANOUT}",
            entries.len()
        )));
    }
    let count = u32::try_from(entries.len())
        .map_err(|_| Error::resource_limit("index leaf entry_count exceeds u32"))?;
    let mut out = Vec::with_capacity(HEADER_LEN + entries.len() * LEAF_ENTRY_LEN);
    push_header(&mut out, KIND_LEAF, depth, count);
    for e in entries {
        out.push(e.key.kind);
        out.extend_from_slice(&e.key.generation.to_le_bytes());
        out.extend_from_slice(&e.key.number.to_le_bytes());
        out.extend_from_slice(&e.out_off.to_le_bytes());
        out.extend_from_slice(&e.out_len.to_le_bytes());
        out.extend_from_slice(e.node_id.as_bytes());
    }
    if out.len() > MAX_INDEX_NODE_BYTES {
        return Err(Error::resource_limit(format!(
            "index leaf node is {} bytes, exceeding MAX_INDEX_NODE_BYTES {MAX_INDEX_NODE_BYTES}",
            out.len()
        )));
    }
    Ok(out)
}

fn encode_internal(children: &[ChildRef], depth: u8) -> Result<Vec<u8>> {
    if children.len() > MAX_FANOUT {
        return Err(Error::resource_limit(format!(
            "index internal entry_count {} exceeds MAX_FANOUT {MAX_FANOUT}",
            children.len()
        )));
    }
    let count = u32::try_from(children.len())
        .map_err(|_| Error::resource_limit("index internal entry_count exceeds u32"))?;
    let mut out = Vec::with_capacity(HEADER_LEN + children.len() * INTERNAL_ENTRY_LEN);
    push_header(&mut out, KIND_INTERNAL, depth, count);
    for c in children {
        out.push(c.range.min_kind);
        out.extend_from_slice(&c.range.min_number.to_le_bytes());
        out.push(c.range.max_kind);
        out.extend_from_slice(&c.range.max_number.to_le_bytes());
        out.extend_from_slice(c.child_id.as_bytes());
    }
    if out.len() > MAX_INDEX_NODE_BYTES {
        return Err(Error::resource_limit(format!(
            "index internal node is {} bytes, exceeding MAX_INDEX_NODE_BYTES {MAX_INDEX_NODE_BYTES}",
            out.len()
        )));
    }
    Ok(out)
}

fn parse_node(bytes: &[u8]) -> Result<DecodedNode> {
    if bytes.len() < HEADER_LEN {
        return Err(Error::integrity_mismatch(
            "index node is shorter than its header",
        ));
    }
    let magic = bytes[0];
    if magic != INDEX_MAGIC {
        return Err(Error::integrity_mismatch(format!(
            "index node has bad magic 0x{magic:02x}"
        )));
    }
    let version = bytes[1];
    if version != INDEX_VERSION {
        return Err(Error::unsupported_version(format!(
            "index node version {version} is not supported"
        )));
    }
    let kind = bytes[2];
    if kind != KIND_LEAF && kind != KIND_INTERNAL {
        return Err(Error::integrity_mismatch(format!(
            "index node has unknown kind {kind}"
        )));
    }
    let depth = bytes[3];
    if depth > MAX_DEPTH {
        return Err(Error::resource_limit(format!(
            "index node depth {depth} exceeds MAX_DEPTH {MAX_DEPTH}"
        )));
    }
    if kind == KIND_LEAF && depth != 0 {
        return Err(Error::integrity_mismatch(
            "index leaf node has non-zero depth",
        ));
    }
    if kind == KIND_INTERNAL && depth == 0 {
        return Err(Error::integrity_mismatch("index internal node has depth 0"));
    }
    let count = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    if count as usize > MAX_FANOUT {
        return Err(Error::resource_limit(format!(
            "index node entry_count {count} exceeds MAX_FANOUT {MAX_FANOUT}"
        )));
    }
    let entry_len = if kind == KIND_LEAF {
        LEAF_ENTRY_LEN
    } else {
        INTERNAL_ENTRY_LEN
    };
    let count = count as usize;
    let expected = HEADER_LEN
        .checked_add(
            count
                .checked_mul(entry_len)
                .ok_or_else(|| Error::resource_limit("index node size overflow"))?,
        )
        .ok_or_else(|| Error::resource_limit("index node size overflow"))?;
    if bytes.len() < expected {
        return Err(Error::integrity_mismatch(format!(
            "index node is truncated: need {expected} bytes, have {}",
            bytes.len()
        )));
    }
    if bytes.len() > expected {
        return Err(Error::integrity_mismatch(format!(
            "index node has {} trailing bytes",
            bytes.len() - expected
        )));
    }
    if bytes.len() > MAX_INDEX_NODE_BYTES {
        return Err(Error::resource_limit(format!(
            "index node is {} bytes, exceeding MAX_INDEX_NODE_BYTES {MAX_INDEX_NODE_BYTES}",
            bytes.len()
        )));
    }

    let mut p = HEADER_LEN;
    let mut leaf = Vec::new();
    let mut internal = Vec::new();
    if kind == KIND_LEAF {
        leaf.reserve(count);
        for _ in 0..count {
            let kind = read_u8(bytes, &mut p)?;
            let generation = read_u16(bytes, &mut p)?;
            let number = read_u32(bytes, &mut p)?;
            let out_off = read_u64(bytes, &mut p)?;
            let out_len = read_u64(bytes, &mut p)?;
            let node_id = read_node_id(bytes, &mut p)?;
            leaf.push(IndexEntry {
                key: SelectorKey {
                    kind,
                    number,
                    generation,
                },
                out_off,
                out_len,
                node_id,
            });
        }
    } else {
        internal.reserve(count);
        for _ in 0..count {
            let min_kind = read_u8(bytes, &mut p)?;
            let min_number = read_u32(bytes, &mut p)?;
            let max_kind = read_u8(bytes, &mut p)?;
            let max_number = read_u32(bytes, &mut p)?;
            let child_id = read_node_id(bytes, &mut p)?;
            internal.push(ChildRef {
                range: KeyRange {
                    min_kind,
                    min_number,
                    max_kind,
                    max_number,
                },
                child_id,
            });
        }
    }
    Ok(DecodedNode {
        kind,
        depth,
        leaf,
        internal,
    })
}

fn read_u8(bytes: &[u8], p: &mut usize) -> Result<u8> {
    let v = *bytes
        .get(*p)
        .ok_or_else(|| Error::integrity_mismatch("truncated index node"))?;
    *p += 1;
    Ok(v)
}

fn read_u16(bytes: &[u8], p: &mut usize) -> Result<u16> {
    let end = p
        .checked_add(2)
        .ok_or_else(|| Error::integrity_mismatch("index node cursor overflow"))?;
    let s = bytes
        .get(*p..end)
        .ok_or_else(|| Error::integrity_mismatch("truncated index node"))?;
    *p = end;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}

fn read_u32(bytes: &[u8], p: &mut usize) -> Result<u32> {
    let end = p
        .checked_add(4)
        .ok_or_else(|| Error::integrity_mismatch("index node cursor overflow"))?;
    let s = bytes
        .get(*p..end)
        .ok_or_else(|| Error::integrity_mismatch("truncated index node"))?;
    *p = end;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn read_u64(bytes: &[u8], p: &mut usize) -> Result<u64> {
    let end = p
        .checked_add(8)
        .ok_or_else(|| Error::integrity_mismatch("index node cursor overflow"))?;
    let s = bytes
        .get(*p..end)
        .ok_or_else(|| Error::integrity_mismatch("truncated index node"))?;
    *p = end;
    Ok(u64::from_le_bytes([
        s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
    ]))
}

fn read_node_id(bytes: &[u8], p: &mut usize) -> Result<NodeId> {
    let end = p
        .checked_add(32)
        .ok_or_else(|| Error::integrity_mismatch("index node cursor overflow"))?;
    let s = bytes
        .get(*p..end)
        .ok_or_else(|| Error::integrity_mismatch("truncated index node"))?;
    let mut raw = [0u8; 32];
    raw.copy_from_slice(s);
    *p = end;
    Ok(NodeId::from_bytes(raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// A read-counting wrapper used only to prove lookup's working set is bounded.
    struct CountingStore<'a> {
        inner: &'a FsIndexStore,
        gets: Cell<u64>,
    }

    impl<'a> CountingStore<'a> {
        fn new(inner: &'a FsIndexStore) -> Self {
            CountingStore {
                inner,
                gets: Cell::new(0),
            }
        }
    }

    impl NodeReader for CountingStore<'_> {
        fn read_node(&self, id: &NodeId) -> Result<Vec<u8>> {
            self.gets.set(self.gets.get() + 1);
            self.inner.get(id)
        }
    }

    fn temp_root(label: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "vole-index-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn entry(kind: u8, number: u32, tag: &[u8]) -> IndexEntry {
        IndexEntry {
            key: SelectorKey::new(kind, number),
            out_off: u64::from(number) * 10,
            out_len: 5,
            node_id: NodeId::of_node(tag),
        }
    }

    fn sample_entries(n: u32) -> Vec<IndexEntry> {
        let kinds = [SEL_PAGE, SEL_OBJECT, SEL_STREAM, SEL_REVISION, SEL_RESOURCE];
        (0..n)
            .map(|i| {
                let kind = kinds[(i as usize) % kinds.len()];
                entry(kind, i, &i.to_le_bytes())
            })
            .collect()
    }

    fn node_files(index_root: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![index_root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for e in entries.flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                } else if let Some(name) = path.file_name().and_then(|n| n.to_str())
                    && name.len() == 64
                    && NodeId::from_hex(name).is_ok()
                {
                    out.push(path);
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn one_page_build_and_lookup() {
        let root = temp_root("one-page");
        let mut store = FsIndexStore::open(&root).unwrap();
        let e = entry(SEL_PAGE, 1, b"page-1");
        let root_id = build(&mut store, std::slice::from_ref(&e)).unwrap();
        assert_eq!(lookup(&store, &root_id, &e.key).unwrap(), vec![e.clone()]);
        assert!(
            lookup(&store, &root_id, &SelectorKey::new(SEL_OBJECT, 7))
                .unwrap()
                .is_empty()
        );
        assert_eq!(validate(&store, &root_id).unwrap(), (1, 0));
        assert_eq!(store.count().unwrap(), 1);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn multi_level_build_finds_every_entry() {
        let root = temp_root("multi");
        let mut store = FsIndexStore::open(&root).unwrap();
        let entries = sample_entries(600);
        let root_id = build(&mut store, &entries).unwrap();
        let (nodes, depth) = validate(&store, &root_id).unwrap();
        assert!(nodes > 1, "expected internal nodes, got {nodes}");
        assert!(depth >= 1, "expected depth >= 1, got {depth}");
        for e in &entries {
            let got = lookup(&store, &root_id, &e.key).unwrap();
            assert_eq!(got, vec![e.clone()], "key {:?} not found", e.key);
        }
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn deep_tree_reaches_depth_two() {
        let root = temp_root("deep");
        let mut store = FsIndexStore::open(&root).unwrap();
        // 30_000 entries -> 203 leaves -> 2 internal nodes -> root depth 2.
        let entries = sample_entries(30_000);
        let root_id = build(&mut store, &entries).unwrap();
        let (nodes, depth) = validate(&store, &root_id).unwrap();
        assert!(nodes >= 203, "expected a deep tree, got {nodes} nodes");
        assert_eq!(depth, 2);
        for e in [&entries[0], &entries[12_345], &entries[29_999]] {
            assert_eq!(lookup(&store, &root_id, &e.key).unwrap(), vec![e.clone()]);
        }
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn missing_key_is_empty_not_error() {
        let root = temp_root("missing");
        let mut store = FsIndexStore::open(&root).unwrap();
        let root_id = build(&mut store, &sample_entries(50)).unwrap();
        let miss = SelectorKey::with_generation(SEL_PAGE, 999_999, 3);
        assert!(lookup(&store, &root_id, &miss).unwrap().is_empty());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn empty_index_roundtrips() {
        let root = temp_root("empty");
        let mut store = FsIndexStore::open(&root).unwrap();
        let root_id = build(&mut store, &[]).unwrap();
        assert!(
            lookup(&store, &root_id, &SelectorKey::new(SEL_PAGE, 1))
                .unwrap()
                .is_empty()
        );
        assert_eq!(validate(&store, &root_id).unwrap(), (1, 0));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn corrupt_node_is_rejected() {
        let root = temp_root("corrupt");
        let mut store = FsIndexStore::open(&root).unwrap();
        let root_id = build(&mut store, &sample_entries(600)).unwrap();
        let root_hex = root_id.to_hex();
        let target = node_files(&root.join("index"))
            .into_iter()
            .find(|p| p.file_name().and_then(|n| n.to_str()) != Some(root_hex.as_str()))
            .expect("expected a non-root node");
        fs::write(&target, b"corrupted bytes").unwrap();
        let err = validate(&store, &root_id).unwrap_err();
        assert_eq!(err.class(), crate::ErrorClass::IntegrityMismatch);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn missing_child_node_is_missing_external_object() {
        let root = temp_root("dangling");
        let mut store = FsIndexStore::open(&root).unwrap();
        let root_id = build(&mut store, &sample_entries(600)).unwrap();
        let root_hex = root_id.to_hex();
        let target = node_files(&root.join("index"))
            .into_iter()
            .find(|p| p.file_name().and_then(|n| n.to_str()) != Some(root_hex.as_str()))
            .expect("expected a non-root node");
        fs::remove_file(&target).unwrap();
        let err = validate(&store, &root_id).unwrap_err();
        assert_eq!(err.class(), crate::ErrorClass::MissingExternalObject);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn duplicate_id_with_differing_bytes_is_rejected() {
        let mut seen: Vec<(NodeId, Vec<u8>, u8)> = Vec::new();
        let id = NodeId::of_node(b"a");
        assert!(note_node(&mut seen, id, b"a", 0).unwrap());
        // A consistent repeat is fine.
        assert!(!note_node(&mut seen, id, b"a", 0).unwrap());
        // The same id with differing bytes is a live inconsistency.
        let err = note_node(&mut seen, id, b"b", 0).unwrap_err();
        assert_eq!(err.class(), crate::ErrorClass::IntegrityMismatch);
    }

    #[test]
    fn caps_are_enforced() {
        // Node-size cap: a leaf one entry over capacity cannot be encoded.
        let too_many = sample_entries(MAX_LEAF_ENTRIES as u32 + 1);
        let err = encode_leaf(&too_many, 0).unwrap_err();
        assert_eq!(err.class(), crate::ErrorClass::ResourceLimit);

        // Fanout cap at parse (checked before allocating the entry array).
        let mut node = vec![INDEX_MAGIC, INDEX_VERSION, KIND_LEAF, 0];
        node.extend_from_slice(&u32::try_from(MAX_FANOUT + 1).unwrap().to_le_bytes());
        node.resize(HEADER_LEN + (MAX_FANOUT + 1) * LEAF_ENTRY_LEN, 0);
        let err = parse_node(&node).unwrap_err();
        assert_eq!(err.class(), crate::ErrorClass::ResourceLimit);

        // Node-size cap at parse: a valid count whose payload exceeds 8 KiB.
        let oversize = MAX_LEAF_ENTRIES + 1;
        let mut node = vec![INDEX_MAGIC, INDEX_VERSION, KIND_LEAF, 0];
        node.extend_from_slice(&u32::try_from(oversize).unwrap().to_le_bytes());
        node.resize(HEADER_LEN + oversize * LEAF_ENTRY_LEN, 0);
        let err = parse_node(&node).unwrap_err();
        assert_eq!(err.class(), crate::ErrorClass::ResourceLimit);

        // Depth cap at parse.
        let node = vec![
            INDEX_MAGIC,
            INDEX_VERSION,
            KIND_INTERNAL,
            MAX_DEPTH + 1,
            0,
            0,
            0,
            0,
        ];
        let err = parse_node(&node).unwrap_err();
        assert_eq!(err.class(), crate::ErrorClass::ResourceLimit);
    }

    #[test]
    fn framing_faults_are_rejected() {
        // Bad magic.
        let mut node = vec![0x00, INDEX_VERSION, KIND_LEAF, 0, 0, 0, 0, 0];
        assert_eq!(
            parse_node(&node).unwrap_err().class(),
            crate::ErrorClass::IntegrityMismatch
        );
        // Unknown version.
        node[0] = INDEX_MAGIC;
        node[1] = INDEX_VERSION + 1;
        assert_eq!(
            parse_node(&node).unwrap_err().class(),
            crate::ErrorClass::UnsupportedVersion
        );
        // Unknown kind.
        node[1] = INDEX_VERSION;
        node[2] = 9;
        assert_eq!(
            parse_node(&node).unwrap_err().class(),
            crate::ErrorClass::IntegrityMismatch
        );
        // Truncated header.
        assert_eq!(
            parse_node(&[INDEX_MAGIC, INDEX_VERSION])
                .unwrap_err()
                .class(),
            crate::ErrorClass::IntegrityMismatch
        );
        // Trailing bytes on a zero-entry leaf.
        let node = vec![INDEX_MAGIC, INDEX_VERSION, KIND_LEAF, 0, 0, 0, 0, 0, 0xff];
        assert_eq!(
            parse_node(&node).unwrap_err().class(),
            crate::ErrorClass::IntegrityMismatch
        );
    }

    #[test]
    fn lookup_reads_bounded_nodes_regardless_of_entry_count() {
        let root = temp_root("bounded");
        let mut store = FsIndexStore::open(&root).unwrap();
        let entries = sample_entries(5_000);
        let root_id = build(&mut store, &entries).unwrap();
        let probe = entries[2_500].key;
        let counter = CountingStore::new(&store);
        let found = lookup_impl(&counter, &root_id, &probe).unwrap();
        assert_eq!(found.len(), 1);
        let reads = counter.gets.get();
        assert!(
            reads <= u64::from(MAX_DEPTH) + 1,
            "lookup read {reads} nodes, exceeding MAX_DEPTH+1"
        );
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn build_is_deterministic_under_shuffle() {
        let root = temp_root("determinism");
        let mut store = FsIndexStore::open(&root).unwrap();
        let entries = sample_entries(1_000);
        let id_a = build(&mut store, &entries).unwrap();
        let mut shuffled = entries.clone();
        shuffled.rotate_left(137);
        shuffled.reverse();
        let id_b = build(&mut store, &shuffled).unwrap();
        assert_eq!(id_a, id_b);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn duplicate_entries_are_deduplicated() {
        let root = temp_root("dedup");
        let mut store = FsIndexStore::open(&root).unwrap();
        let entries = sample_entries(10);
        let id_a = build(&mut store, &entries).unwrap();
        let mut with_dupes = entries.clone();
        with_dupes.extend(entries.iter().cloned());
        let id_b = build(&mut store, &with_dupes).unwrap();
        assert_eq!(id_a, id_b);
        fs::remove_dir_all(&root).ok();
    }
}
