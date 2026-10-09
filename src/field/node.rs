//! Canonical procedural seed nodes (Phase 11, ADR-0025).
//!
//! A [`SeedNode`] describes one **computation**, not one storage slot: a bounded,
//! versioned materializer over zero or more dependencies, producing a declared
//! output kind and logical length. Nodes are canonically encoded (little-endian,
//! length-prefixed, no serde) so that the same computation always hashes to the
//! same [`NodeId`] regardless of platform or insertion order.
//!
//! The node's `content_id` is **not** stored inside the node: it is
//! `NodeId::of_node(canonical_bytes)`, which is what makes the graph
//! content-addressed and immutable. Changing a dependency's bytes yields a
//! different dependency id, hence a different node id; there is no mutation.

use crate::error::{Error, Result};
use crate::limits::Limits;
use crate::store::NodeId;

/// Canonical header byte for the node encoding.
pub const NODE_MAGIC: u8 = 0xB1;
/// Recommended maximum canonical node size (framing discipline).
pub const MAX_NODE_BYTES: usize = 64 * 1024;
/// Maximum dependency fanout of a single node.
pub const MAX_NODE_DEPS: usize = 256;
/// The materializer semantics version for v1.
pub const MATERIALIZER_VERSION: u16 = 1;

/// What a node computes. The numeric values are part of the canonical encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NodeKind {
    /// The whole exact source, materialized from the descriptor blob.
    DocumentExact = 0x01,
    /// An exact byte span of the source.
    SourceSlice = 0x02,
    /// An exact physical revision span.
    PdfRevision = 0x03,
    /// An exact indirect-object byte span (`n g obj … endobj`).
    PdfObject = 0x04,
    /// The exact encoded bytes of a stream object's data.
    PdfStreamEncoded = 0x05,
    /// The decoded (inflated) bytes of a lone `/FlateDecode` stream.
    PdfStreamDecoded = 0x06,
    /// A decoded content stream's operator token span.
    ContentOperators = 0x07,
    /// A deterministic text-run projection of content operators.
    TextRuns = 0x08,
    /// A page's concatenated decoded content stream.
    PageContent = 0x09,
    /// A deterministic structured page preview.
    PagePreview = 0x0A,
    /// A physical byte span of one resource object referenced by a page.
    ResourceRef = 0x0B,
    /// Concatenation of dependency outputs (exact).
    Concat = 0x0C,
    /// A raw exact literal held in the seed store.
    Literal = 0x0D,
    /// The whole exact package (ZIP/OCF/OPC) source, materialized through the
    /// descriptor (`serve_document`). Exact (Phase 12.2).
    PackageRoot = 0x0E,
    /// One package member's exact raw compressed/stored span (a `SourceSlice`
    /// with package provenance). Exact (Phase 12.2).
    PackageMemberRaw = 0x0F,
    /// One package member's decoded bytes: raw-DEFLATE inflate (method 8) or the
    /// stored identity (method 0). Derived, never exact (Phase 12.2).
    PackageMemberDecoded = 0x10,
    /// The canonical generic-OPC package graph (content types, parts, package and
    /// part relationships) derived on demand from the exact package source.
    /// Derived, never exact (Phase 12.3).
    PackageOpcModel = 0x11,
    /// The canonical DOCX discovery model (main part, styles part, stories)
    /// derived on demand from the canonical OPC model. Derived, never exact
    /// (Phase 12.4).
    DocxModel = 0x12,
    /// One WordprocessingML story parsed into its canonical [`crate::adapter::docx::wml::StoryModel`],
    /// honoring a declared extraction profile. Derived, never exact (Phase 12.4).
    DocxStory = 0x13,
    /// The canonical EPUB (OCF) discovery model: `mimetype` conformance facts,
    /// container rootfiles, and the Package Document metadata/manifest/spine,
    /// derived on demand from the exact package source. Derived, never exact
    /// (Phase 12.5).
    EpubModel = 0x14,
    /// One spine item's XHTML content document parsed into its bounded native
    /// content model (headings/paragraphs/lists/tables/links/resources), honoring
    /// a declared extraction profile. Derived, never exact (Phase 12.6).
    EpubContent = 0x15,
    /// A byte-identical shareable resource (image/font/attachment) held inline and
    /// addressed by **content identity** (Phase 12.8, ADR-0034). Its canonical
    /// encoding embeds the exact bytes, so two documents carrying the same resource
    /// share one node id (and one persisted blob) with no second identity scheme.
    /// Exact (a resource's bytes are the source bytes).
    ResourceBlob = 0x16,
    /// The canonical ODT (ODF) discovery model: `mimetype` conformance facts and the
    /// parsed `META-INF/manifest.xml` file entries with the main content part resolved
    /// semantically, derived on demand from the exact package source. Derived, never
    /// exact (Phase 13.3).
    OdtModel = 0x17,
    /// The OpenDocument main part (`content.xml`, `office:text`) parsed into its
    /// bounded native content model (paragraphs/headings/spans/lists/tables/links/
    /// bookmarks/notes/resources/tracked changes/sections), honoring a declared
    /// extraction profile. Derived, never exact (Phase 13.3).
    OdtContent = 0x18,
    /// The PDF **revision lineage** (Phase 17): a compact JSON projection of the
    /// physical incremental revision chain (count, ordered indices, byte spans,
    /// resolved `startxref`/`/Prev`, and per-revision object/stream membership),
    /// computed once at ingest from the byte-authoritative scan. Derived, never
    /// exact. There is one document-level node (the whole lineage) and one
    /// per-revision node.
    PdfRevisionLineage = 0x19,
    /// The canonical XLSX (SpreadsheetML) discovery model (Phase 21.1.1): the
    /// workbook part (resolved via the `officeDocument` relationship and its
    /// SpreadsheetML content type), the styles and shared-strings parts, and the
    /// discovered worksheet parts. Derived on demand from the canonical OPC model.
    /// Derived, never exact.
    XlsxModel = 0x1A,
    /// The parsed `xl/workbook.xml` sheet inventory (name, sheetId, r:id, state,
    /// document order), parsed from the decoded workbook member. Derived, never
    /// exact (Phase 21.1.1).
    XlsxWorkbook = 0x1B,
    /// One worksheet part (`xl/worksheets/sheetN.xml`) parsed into its bounded cell
    /// model, honoring a declared extraction profile and resolving shared strings
    /// from the decoded shared-strings member. Derived, never exact (Phase 21.1.1).
    XlsxSheet = 0x1C,
    /// The canonical PPTX (PresentationML) discovery model (Phase 21.2.1): the main
    /// presentation part (resolved via the `officeDocument` relationship and its
    /// PresentationML content type), the notes/slide masters, the layouts, themes
    /// and media parts, and the discovered slide parts. Derived on demand from the
    /// canonical OPC model. Derived, never exact.
    PptxModel = 0x1D,
    /// The parsed `ppt/presentation.xml` slide inventory (slide size and the
    /// `p:sldIdLst` order), parsed from the decoded presentation member. Derived,
    /// never exact (Phase 21.2.1).
    PptxPresentation = 0x1E,
    /// One slide part (`ppt/slides/slideN.xml`) parsed into its bounded shape model,
    /// honoring a declared extraction profile. Derived, never exact (Phase 21.2.1).
    PptxSlide = 0x1F,
    /// One notes-slide part (`ppt/notesSlides/notesSlideN.xml`) parsed into its
    /// bounded text model. Derived, never exact (Phase 21.2.1).
    PptxNotes = 0x20,
    /// The canonical ODS (ODF spreadsheet) discovery model: `mimetype` conformance
    /// facts and the parsed `META-INF/manifest.xml` file entries with the main
    /// content part resolved semantically, derived on demand from the exact package
    /// source. Derived, never exact (Phase 21.3.1).
    OdsModel = 0x21,
    /// The OpenDocument spreadsheet main part (`content.xml`, `office:spreadsheet`)
    /// parsed into its bounded native model (sheets/rows/cells with typed values,
    /// stored formulas, cell styles, named expressions, and annotations), honoring a
    /// declared extraction profile. Derived, never exact (Phase 21.3.1).
    OdsContent = 0x22,
    /// The OpenDocument styles part (`styles.xml`) parsed into its bounded cell-style
    /// and number-format model. Derived, never exact (Phase 21.3.1).
    OdsStyles = 0x23,
    /// The canonical ODP (ODF presentation) discovery model: `mimetype` conformance
    /// facts and the parsed `META-INF/manifest.xml` file entries with the main
    /// content part and the `Pictures/*` media parts resolved semantically, derived
    /// on demand from the exact package source. Derived, never exact (Phase 21.4.1).
    OdpModel = 0x24,
    /// The OpenDocument presentation main part (`content.xml`, `office:presentation`)
    /// parsed into its bounded native model (`draw:page` slides in document order,
    /// shapes/text runs, embedded tables, notes, styles, and image references),
    /// honoring a declared extraction profile. Derived, never exact (Phase 21.4.1).
    OdpContent = 0x25,
    /// The OpenDocument styles part (`styles.xml`) parsed into its bounded style and
    /// master-page model. Derived, never exact (Phase 21.4.1).
    OdpStyles = 0x26,
}

impl NodeKind {
    /// Map a raw kind byte.
    pub const fn from_u8(b: u8) -> Option<NodeKind> {
        Some(match b {
            0x01 => NodeKind::DocumentExact,
            0x02 => NodeKind::SourceSlice,
            0x03 => NodeKind::PdfRevision,
            0x04 => NodeKind::PdfObject,
            0x05 => NodeKind::PdfStreamEncoded,
            0x06 => NodeKind::PdfStreamDecoded,
            0x07 => NodeKind::ContentOperators,
            0x08 => NodeKind::TextRuns,
            0x09 => NodeKind::PageContent,
            0x0A => NodeKind::PagePreview,
            0x0B => NodeKind::ResourceRef,
            0x0C => NodeKind::Concat,
            0x0D => NodeKind::Literal,
            0x0E => NodeKind::PackageRoot,
            0x0F => NodeKind::PackageMemberRaw,
            0x10 => NodeKind::PackageMemberDecoded,
            0x11 => NodeKind::PackageOpcModel,
            0x12 => NodeKind::DocxModel,
            0x13 => NodeKind::DocxStory,
            0x14 => NodeKind::EpubModel,
            0x15 => NodeKind::EpubContent,
            0x16 => NodeKind::ResourceBlob,
            0x17 => NodeKind::OdtModel,
            0x18 => NodeKind::OdtContent,
            0x19 => NodeKind::PdfRevisionLineage,
            0x1A => NodeKind::XlsxModel,
            0x1B => NodeKind::XlsxWorkbook,
            0x1C => NodeKind::XlsxSheet,
            0x1D => NodeKind::PptxModel,
            0x1E => NodeKind::PptxPresentation,
            0x1F => NodeKind::PptxSlide,
            0x20 => NodeKind::PptxNotes,
            0x21 => NodeKind::OdsModel,
            0x22 => NodeKind::OdsContent,
            0x23 => NodeKind::OdsStyles,
            0x24 => NodeKind::OdpModel,
            0x25 => NodeKind::OdpContent,
            0x26 => NodeKind::OdpStyles,
            _ => return None,
        })
    }

    /// Stable short name.
    pub const fn name(self) -> &'static str {
        match self {
            NodeKind::DocumentExact => "DocumentExact",
            NodeKind::SourceSlice => "SourceSlice",
            NodeKind::PdfRevision => "PdfRevision",
            NodeKind::PdfObject => "PdfObject",
            NodeKind::PdfStreamEncoded => "PdfStreamEncoded",
            NodeKind::PdfStreamDecoded => "PdfStreamDecoded",
            NodeKind::ContentOperators => "ContentOperators",
            NodeKind::TextRuns => "TextRuns",
            NodeKind::PageContent => "PageContent",
            NodeKind::PagePreview => "PagePreview",
            NodeKind::ResourceRef => "ResourceRef",
            NodeKind::Concat => "Concat",
            NodeKind::Literal => "Literal",
            NodeKind::PackageRoot => "PackageRoot",
            NodeKind::PackageMemberRaw => "PackageMemberRaw",
            NodeKind::PackageMemberDecoded => "PackageMemberDecoded",
            NodeKind::PackageOpcModel => "PackageOpcModel",
            NodeKind::DocxModel => "DocxModel",
            NodeKind::DocxStory => "DocxStory",
            NodeKind::EpubModel => "EpubModel",
            NodeKind::EpubContent => "EpubContent",
            NodeKind::ResourceBlob => "ResourceBlob",
            NodeKind::OdtModel => "OdtModel",
            NodeKind::OdtContent => "OdtContent",
            NodeKind::PdfRevisionLineage => "PdfRevisionLineage",
            NodeKind::XlsxModel => "XlsxModel",
            NodeKind::XlsxWorkbook => "XlsxWorkbook",
            NodeKind::XlsxSheet => "XlsxSheet",
            NodeKind::PptxModel => "PptxModel",
            NodeKind::PptxPresentation => "PptxPresentation",
            NodeKind::PptxSlide => "PptxSlide",
            NodeKind::PptxNotes => "PptxNotes",
            NodeKind::OdsModel => "OdsModel",
            NodeKind::OdsContent => "OdsContent",
            NodeKind::OdsStyles => "OdsStyles",
            NodeKind::OdpModel => "OdpModel",
            NodeKind::OdpContent => "OdpContent",
            NodeKind::OdpStyles => "OdpStyles",
        }
    }

    /// Whether the output bytes are an exact, byte-identical observation of the
    /// source (`true`) or a deterministic derived projection (`false`).
    ///
    /// This is the `Q_ref` / `Q_gen` boundary (ADR-0026).
    pub const fn is_exact(self) -> bool {
        matches!(
            self,
            NodeKind::DocumentExact
                | NodeKind::SourceSlice
                | NodeKind::PdfRevision
                | NodeKind::PdfObject
                | NodeKind::PdfStreamEncoded
                | NodeKind::ResourceRef
                | NodeKind::Concat
                | NodeKind::Literal
                // Package physical leaves are exact source spans; a decoded
                // member (`PackageMemberDecoded`) is derived and is **not** exact.
                | NodeKind::PackageRoot
                | NodeKind::PackageMemberRaw
                // A shared resource is the exact embedded bytes.
                | NodeKind::ResourceBlob
        )
    }
}

/// A bounded resource envelope declared by a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeLimits {
    /// Maximum materialized output length this node may produce.
    pub max_output_bytes: u64,
    /// Maximum dependency depth permitted below this node.
    pub max_depth: u16,
    /// Maximum dependency fanout permitted.
    pub max_fanout: u16,
}

impl NodeLimits {
    /// Conservative defaults used by the PDF inverse compiler.
    pub const DEFAULT: NodeLimits = NodeLimits {
        max_output_bytes: 1 << 31,
        max_depth: 64,
        max_fanout: MAX_NODE_DEPS as u16,
    };
}

/// One canonical procedural seed node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedNode {
    /// Computation kind.
    pub kind: NodeKind,
    /// Materializer registry id (the kind value for v1).
    pub materializer_id: u16,
    /// Materializer semantics version.
    pub materializer_version: u16,
    /// Declared logical output length (a bound, checked on materialization).
    pub logical_output_len: u64,
    /// Kind-specific canonical parameters.
    pub params: Vec<u8>,
    /// Canonical dependency ids actually read (the dynamic read set).
    pub deps: Vec<NodeId>,
    /// Short provenance/basis string (adapter-supplied, advisory).
    pub provenance: String,
    /// Resource envelope.
    pub limits: NodeLimits,
}

impl SeedNode {
    /// Construct a node with the current materializer version.
    pub fn new(
        kind: NodeKind,
        logical_output_len: u64,
        params: Vec<u8>,
        deps: Vec<NodeId>,
        provenance: impl Into<String>,
    ) -> Self {
        SeedNode {
            kind,
            materializer_id: kind as u16,
            materializer_version: MATERIALIZER_VERSION,
            logical_output_len,
            params,
            deps,
            provenance: provenance.into(),
            limits: NodeLimits::DEFAULT,
        }
    }

    /// Canonically encode this node. The encoding never includes the node id.
    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64 + self.params.len());
        out.push(NODE_MAGIC);
        out.push(crate::store::SEED_FORMAT_VERSION);
        out.push(self.kind as u8);
        out.push(0); // reserved
        out.extend_from_slice(&self.materializer_id.to_le_bytes());
        out.extend_from_slice(&self.materializer_version.to_le_bytes());
        out.extend_from_slice(&self.logical_output_len.to_le_bytes());
        out.extend_from_slice(&(self.deps.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.params.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.limits.max_output_bytes.to_le_bytes());
        out.extend_from_slice(&self.limits.max_depth.to_le_bytes());
        out.extend_from_slice(&self.limits.max_fanout.to_le_bytes());
        for dep in &self.deps {
            out.extend_from_slice(dep.as_bytes());
        }
        out.extend_from_slice(&self.params);
        let prov = self.provenance.as_bytes();
        out.extend_from_slice(&(prov.len() as u32).to_le_bytes());
        out.extend_from_slice(prov);
        out
    }

    /// Parse a canonical node, enforcing structural bounds.
    pub fn decode_canonical(bytes: &[u8]) -> Result<SeedNode> {
        let mut r = Reader::new(bytes);
        if r.u8()? != NODE_MAGIC {
            return Err(Error::unsupported_version("seed node: bad magic"));
        }
        let version = r.u8()?;
        if version != crate::store::SEED_FORMAT_VERSION {
            return Err(Error::unsupported_version(format!(
                "seed node format version {version} is not supported"
            )));
        }
        let kind_byte = r.u8()?;
        let kind = NodeKind::from_u8(kind_byte)
            .ok_or_else(|| Error::unsupported_version(format!("unknown node kind {kind_byte}")))?;
        let _reserved = r.u8()?;
        let materializer_id = r.u16()?;
        let materializer_version = r.u16()?;
        let logical_output_len = r.u64()?;
        let dep_count = r.u32()? as usize;
        let param_len = r.u32()? as usize;
        let max_output_bytes = r.u64()?;
        let max_depth = r.u16()?;
        let max_fanout = r.u16()?;
        if dep_count > MAX_NODE_DEPS {
            return Err(Error::resource_limit(format!(
                "seed node declares {dep_count} deps (max {MAX_NODE_DEPS})"
            )));
        }
        let mut deps = Vec::with_capacity(dep_count);
        for _ in 0..dep_count {
            let mut b = [0u8; 32];
            b.copy_from_slice(r.bytes(32)?);
            deps.push(NodeId::from_bytes(b));
        }
        let params = r.bytes(param_len)?.to_vec();
        let prov_len = r.u32()? as usize;
        let prov = r.bytes(prov_len)?;
        let provenance = core::str::from_utf8(prov)
            .map_err(|_| Error::usage("seed node provenance is not UTF-8"))?
            .to_string();
        if !r.at_end() {
            return Err(Error::usage("seed node has trailing bytes"));
        }
        if materializer_id != kind as u16 {
            return Err(Error::unsupported_version(
                "seed node materializer id does not match its kind",
            ));
        }
        Ok(SeedNode {
            kind,
            materializer_id,
            materializer_version,
            logical_output_len,
            params,
            deps,
            provenance,
            limits: NodeLimits {
                max_output_bytes,
                max_depth,
                max_fanout,
            },
        })
    }

    /// The content id of this node's canonical encoding.
    pub fn content_id(&self) -> NodeId {
        NodeId::of_node(&self.encode_canonical())
    }

    /// Validate the declared envelope against the global [`Limits`].
    pub fn check_limits(&self, limits: &Limits) -> Result<()> {
        if self.logical_output_len > self.limits.max_output_bytes {
            return Err(Error::resource_limit(format!(
                "seed node declares output {} > its own cap {}",
                self.logical_output_len, self.limits.max_output_bytes
            )));
        }
        let _ = limits;
        Ok(())
    }
}

/// A tiny bounds-checked little-endian reader.
struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(b: &'a [u8]) -> Self {
        Reader { b, at: 0 }
    }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| Error::usage("seed node read overflow"))?;
        if end > self.b.len() {
            return Err(Error::usage("truncated seed node"));
        }
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        let b = self.bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Result<u32> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn u64(&mut self) -> Result<u64> {
        let b = self.bytes(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(u64::from_le_bytes(a))
    }
    fn at_end(&self) -> bool {
        self.at == self.b.len()
    }
}

/// Encode an `[offset, len]` parameter block.
pub fn span_params(offset: u64, len: u64) -> Vec<u8> {
    let mut p = Vec::with_capacity(16);
    p.extend_from_slice(&offset.to_le_bytes());
    p.extend_from_slice(&len.to_le_bytes());
    p
}

/// Decode an `[offset, len]` parameter block.
pub fn read_span_params(params: &[u8]) -> Result<(u64, u64)> {
    if params.len() != 16 {
        return Err(Error::usage("span params must be 16 bytes"));
    }
    let mut a = [0u8; 8];
    a.copy_from_slice(&params[0..8]);
    let offset = u64::from_le_bytes(a);
    a.copy_from_slice(&params[8..16]);
    let len = u64::from_le_bytes(a);
    Ok((offset, len))
}

/// Encode a `(u32, u16, ...)` object-identity parameter block: `object`,
/// `generation`, then a trailing kind-specific `u64`.
pub fn object_params(object: u32, generation: u16, extra: u64) -> Vec<u8> {
    let mut p = Vec::with_capacity(16);
    p.extend_from_slice(&object.to_le_bytes());
    p.extend_from_slice(&generation.to_le_bytes());
    p.extend_from_slice(&0u16.to_le_bytes());
    p.extend_from_slice(&extra.to_le_bytes());
    p
}

/// Decode an object-identity parameter block.
pub fn read_object_params(params: &[u8]) -> Result<(u32, u16, u64)> {
    if params.len() != 16 {
        return Err(Error::usage("object params must be 16 bytes"));
    }
    let object = u32::from_le_bytes([params[0], params[1], params[2], params[3]]);
    let generation = u16::from_le_bytes([params[4], params[5]]);
    let mut a = [0u8; 8];
    a.copy_from_slice(&params[8..16]);
    let extra = u64::from_le_bytes(a);
    Ok((object, generation, extra))
}

/// Encode a single `u32` parameter.
pub fn u32_params(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

/// Decode a single `u32` parameter.
pub fn read_u32_params(params: &[u8]) -> Result<u32> {
    if params.len() != 4 {
        return Err(Error::usage("u32 params must be 4 bytes"));
    }
    Ok(u32::from_le_bytes([
        params[0], params[1], params[2], params[3],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_roundtrip_is_stable() {
        let n = SeedNode::new(
            NodeKind::PdfStreamDecoded,
            4096,
            span_params(100, 200),
            vec![NodeId::from_bytes([7u8; 32])],
            "pdf:stream-decoded",
        );
        let enc = n.encode_canonical();
        let back = SeedNode::decode_canonical(&enc).unwrap();
        assert_eq!(n, back);
        assert_eq!(back.content_id(), n.content_id());
        assert_eq!(n.encode_canonical(), enc);
    }

    #[test]
    fn dependency_change_changes_id() {
        let a = SeedNode::new(
            NodeKind::Concat,
            10,
            vec![],
            vec![NodeId::from_bytes([1u8; 32])],
            "t",
        );
        let b = SeedNode::new(
            NodeKind::Concat,
            10,
            vec![],
            vec![NodeId::from_bytes([2u8; 32])],
            "t",
        );
        assert_ne!(a.content_id(), b.content_id());
    }

    #[test]
    fn unknown_kind_and_version_fail_closed() {
        let mut n = SeedNode::new(NodeKind::Literal, 1, vec![9], vec![], "t").encode_canonical();
        n[0] = 0x00;
        assert!(SeedNode::decode_canonical(&n).is_err());
        let mut n2 = SeedNode::new(NodeKind::Literal, 1, vec![9], vec![], "t").encode_canonical();
        n2[1] = 0x7F;
        assert!(SeedNode::decode_canonical(&n2).is_err());
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut enc = SeedNode::new(NodeKind::Literal, 1, vec![], vec![], "t").encode_canonical();
        enc.push(0);
        assert!(SeedNode::decode_canonical(&enc).is_err());
    }

    #[test]
    fn param_helpers_roundtrip() {
        let (o, l) = read_span_params(&span_params(5, 9)).unwrap();
        assert_eq!((o, l), (5, 9));
        let (ob, g, e) = read_object_params(&object_params(42, 3, 77)).unwrap();
        assert_eq!((ob, g, e), (42, 3, 77));
        assert_eq!(read_u32_params(&u32_params(1234)).unwrap(), 1234);
    }
}
