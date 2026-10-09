//! The observation query engine (Phase 11.5–11.7).
//!
//! An [`observe`] call resolves one typed selector/representation pair against a
//! persisted field and returns a [`FieldAnswer`] with full provenance plus
//! [`ObserveStats`] describing the work actually done, and the current/promoted
//! [`FieldId`] so a caller can chain without re-deepening. The engine is
//! deterministic and read-only with respect to archival authority: it never
//! consults an agent, model, or search process, and it can never influence
//! `materialize_exact` (ADR-0024, DEC-6).
//!
//! ## The frontier rule (11.6)
//!
//! A narrow observation materializes only the dependency closure it needs. In
//! particular, `Page(n) + Structure` reads the page's decoded content streams and
//! its preview node — it never reads image/XObject bytes and never reconstructs
//! the whole document. The split byte classes in [`ObserveStats`] make this
//! auditable: the **seed** closure is reported separately as `seed_bytes_read`,
//! and the descriptor bytes a narrow observation necessarily reads to open the
//! field are charged honestly in `descriptor_bytes_read` rather than hidden.
//!
//! ## No guessing
//!
//! An unsupported selector/representation pair is a typed
//! [`crate::ErrorClass::UnsupportedFeature`], never a silently empty answer.

use std::cell::{Cell, RefCell};
#[cfg(feature = "docx")]
use std::collections::HashMap;
use std::rc::Rc;
#[cfg(feature = "docx")]
use std::sync::Arc;
use std::time::Instant;

#[cfg(feature = "docx")]
use crate::adapter::docx::wml::StoryModel;
#[cfg(feature = "docx")]
use crate::adapter::docx::{DocxExtractProfile, DocxModel, DocxPartRef, DocxStory, story_params};
#[cfg(feature = "epub")]
use crate::adapter::epub::{EpubExtractProfile, EpubModel, ManifestItem, PackageDoc};
#[cfg(feature = "odt")]
use crate::adapter::odt::{
    Block as OdtBlock, ContentModel as OdtContentModel, OdtExtractProfile, OdtModel,
};
#[cfg(feature = "pptx")]
use crate::adapter::pptx::{
    NotesModel as PptxNotesModel, PptxExtractProfile, PptxModel, PptxShape, PptxTable,
    PresentationModel as PptxPresentationModel, SlideModel as PptxSlideModel,
};
#[cfg(feature = "xlsx")]
use crate::adapter::xlsx::{
    SheetModel as XlsxSheetModel, WorkbookModel as XlsxWorkbookModel, XlsxExtractProfile, XlsxModel,
};
use crate::error::{Error, Result};
use crate::field::cache::DerivedCache;
use crate::field::dag::{self, EvalBudget, OutputCache, ReuseStats, SourceServer};
use crate::field::document_format::DocumentFormat;
#[cfg(feature = "docx")]
use crate::field::index::SEL_DOCX_MODEL;
#[cfg(feature = "epub")]
use crate::field::index::SEL_EPUB_MODEL;
#[cfg(feature = "odt")]
use crate::field::index::SEL_ODT_MODEL;
#[cfg(feature = "opc")]
use crate::field::index::SEL_OPC_MODEL;
#[cfg(feature = "pptx")]
use crate::field::index::SEL_PPTX_MODEL;
#[cfg(feature = "xlsx")]
use crate::field::index::SEL_XLSX_MODEL;
use crate::field::index::{
    FsIndexStore, IndexEntry, SEL_OBJECT, SEL_PACKAGE_MEMBER_DECODED, SEL_PACKAGE_MEMBER_RAW,
    SEL_PAGE, SEL_REVISION, SEL_REVISION_LINEAGE, SEL_REVISIONS, SEL_STREAM, SEL_STREAM_DECODED,
    SelectorKey, lookup,
};
use crate::field::ingest;
use crate::field::manifest::FieldRoot;
use crate::field::node::{NodeKind, SeedNode, read_u32_params, span_params, u32_params};
use crate::field::partial::{PartialDescriptor, PartialLoad};
use crate::field::promote::GovernedCache;
use crate::field::{Field, FieldId, FieldStore, SeedSubstrate};
use crate::limits::Limits;
use crate::store::{Id, IoSnapshot, NodeId, SeedStore};

use super::provenance::{AnswerValue, Basis, FieldAnswer, IntegrityScope, json_escape};

/// Upper bound on pages a single `TextMatch` scan will visit.
const MAX_TEXTMATCH_PAGES: u32 = 1 << 20;

/// A typed observation selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selector {
    /// The whole document.
    Document,
    /// A page, by 1-based page number (as recovered by ingest).
    Page(u32),
    /// An indirect object, by object number.
    Object(u32),
    /// An encoded stream, by owning object number.
    Stream(u32),
    /// A physical revision, by 0-based index.
    Revision(u32),
    /// The whole PDF **revision lineage** (Phase 17): the revision count, the
    /// ordered revision indices and byte spans, each revision's resolved
    /// `startxref`/`/Prev` headers, and which indirect objects and streams each
    /// revision defines. Incremental updates are a PDF concept with no analog in
    /// DOCX/EPUB/ODT, so a field with no revision structure is a typed decline,
    /// never an empty answer.
    Revisions,
    /// The **external** corpus/dataset lineage (Phase 20.4): the dataset family
    /// id, member id, and head flag, attached to the field as an explicit
    /// [`crate::field::external::ExternalContext`] and drawn from it with basis
    /// [`super::provenance::Basis::ExternalMetadata`]. These facts are **not** in
    /// the document bytes, so a field with no attached context is a typed
    /// decline, never a guess.
    ExternalLineage,
    /// A package (ZIP/OCF/OPC) member, by central-directory ordinal. The ordinal is
    /// the physical identity; duplicate names stay distinct (Phase 12.2).
    Member(u32),
    /// A generic OPC package part, by absolute part name (Phase 12.3). Part-name
    /// equivalence is case-insensitive. Resolution is by the OPC relationship graph,
    /// never by a hardcoded path.
    PackagePart(String),
    /// A generic OPC relationship, by id (Phase 12.3). Ids are only unique within one
    /// `.rels` part, so a duplicated id across owners is a typed ambiguity decline.
    Relationship(String),
    /// A half-open exact source byte range.
    ByteRange {
        /// Start offset.
        offset: u64,
        /// Length in bytes.
        len: u64,
    },
    /// Every text line containing a pattern (case-sensitive).
    TextMatch(String),
    /// **Common** document-level metadata projection (format-neutral). Only the
    /// detected format's native metadata is projected; the answer names the format
    /// and its native provenance (Phase 12.7, ADR-0031).
    Metadata,
    /// **Common** whole-document reading-text projection.
    Text,
    /// **Common** the `n`-th heading in reading order (0-based).
    Heading(u32),
    /// **Common** the `n`-th block (paragraph or table) in reading order (0-based).
    Block(u32),
    /// **Common** the `n`-th top-level table in reading order (0-based).
    Table(u32),
    /// **Common** a table cell by 0-based `table`, `row`, and physical grid `col`.
    Cell {
        /// 0-based table index in reading order.
        table: u32,
        /// 0-based row index.
        row: u32,
        /// 0-based physical column index (a DOCX grid column; an EPUB cell position).
        col: u32,
    },
    /// **Common** the `n`-th embedded resource (image/embedded object) in reading
    /// order (0-based).
    Resource(u32),
    /// **Common** the `n`-th hyperlink in reading order (0-based).
    Link(u32),
    /// **Common** a deterministic, case-sensitive lexical search over the
    /// document's reading text. Never an embedding or a model call.
    SearchMatch(String),
    /// A DOCX story, scoped to exactly one story and one extraction profile
    /// (Phase 12.4). A story is never silently mixed with another.
    #[cfg(feature = "docx")]
    DocxStory {
        /// The story to observe.
        story: DocxStory,
        /// The extraction profile identity.
        profile: DocxExtractProfile,
    },
    /// A body-level paragraph of a DOCX story, by 0-based document-order index.
    #[cfg(feature = "docx")]
    DocxParagraph {
        /// The owning story.
        story: DocxStory,
        /// The paragraph index.
        index: u32,
        /// The extraction profile identity.
        profile: DocxExtractProfile,
    },
    /// A top-level DOCX table, by 0-based index.
    #[cfg(feature = "docx")]
    DocxTable {
        /// The owning story.
        story: DocxStory,
        /// The table index.
        index: u32,
        /// The extraction profile identity.
        profile: DocxExtractProfile,
    },
    /// A DOCX table cell, addressed by an A1-style reference (e.g. `B7`).
    #[cfg(feature = "docx")]
    DocxCell {
        /// The owning story.
        story: DocxStory,
        /// The table index.
        table: u32,
        /// The cell reference (`B7`: column `B`, 1-based row `7`).
        cell: String,
        /// The extraction profile identity.
        profile: DocxExtractProfile,
    },
    /// A story-scoped text search over paragraphs.
    #[cfg(feature = "docx")]
    DocxFind {
        /// The owning story.
        story: DocxStory,
        /// The pattern (case-sensitive substring).
        pattern: String,
        /// The extraction profile identity.
        profile: DocxExtractProfile,
    },
    /// The EPUB (OCF) container + Package Document as a whole (Phase 12.5).
    #[cfg(feature = "epub")]
    EpubPackage,
    /// A Package Document manifest item, by 0-based document-order index. `ExactBytes`
    /// and `DecodedBytes` resolve to the item's container member; an external target
    /// is an inert identifier and is a typed decline, never a fetch.
    #[cfg(feature = "epub")]
    EpubManifestItem {
        /// The manifest index.
        index: u32,
    },
    /// A spine item as the reading-order coordinate (Phase 12.5). The index is into
    /// the reading order selected by `profile` (`linear-only` by default). This is
    /// **not** `Page(n)`: reflowable EPUB has no intrinsic pages.
    #[cfg(feature = "epub")]
    EpubSpineItem {
        /// The reading-order index.
        index: u32,
        /// The reading profile identity.
        profile: EpubExtractProfile,
    },
    /// The EPUB Navigation Document (its `toc`/`landmarks`/`page-list` sets).
    #[cfg(feature = "epub")]
    EpubNav,
    /// One flattened navigation entry, by 0-based index.
    #[cfg(feature = "epub")]
    EpubNavNode {
        /// The entry index.
        index: u32,
    },
    /// A container resource by member name (e.g. `OEBPS/text/ch1.xhtml`), resolved
    /// through the manifest; external targets are inert and never fetched.
    #[cfg(feature = "epub")]
    EpubResource(String),
    /// One block of a spine item's content document (Phase 12.6), by 0-based
    /// document-order index into that item's parsed [`crate::adapter::epub::Block`]
    /// list (heading/paragraph/list/table).
    #[cfg(feature = "epub")]
    EpubBlock {
        /// The reading-order spine index.
        index: u32,
        /// The block index.
        block: u32,
        /// The reading profile identity.
        profile: EpubExtractProfile,
    },
    /// One table cell of a spine item, addressed by **physical** position: the
    /// 0-based table index among the item's tables, the 0-based `tr` index, and the
    /// 0-based cell index within that row (spans are reported, never projected).
    #[cfg(feature = "epub")]
    EpubCell {
        /// The reading-order spine index.
        index: u32,
        /// The 0-based table index.
        table: u32,
        /// The 0-based row index.
        row: u32,
        /// The 0-based physical cell index within the row.
        col: u32,
        /// The reading profile identity.
        profile: EpubExtractProfile,
    },
    /// One link of a spine item's content document, by 0-based index.
    #[cfg(feature = "epub")]
    EpubLink {
        /// The reading-order spine index.
        index: u32,
        /// The link index.
        link: u32,
        /// The reading profile identity.
        profile: EpubExtractProfile,
    },
    /// A text search over one spine item's blocks, scoped by the reading profile.
    #[cfg(feature = "epub")]
    EpubFind {
        /// The reading-order spine index.
        index: u32,
        /// The pattern (case-sensitive substring).
        pattern: String,
        /// The reading profile identity.
        profile: EpubExtractProfile,
    },
    /// An ODT (ODF) package part by `manifest:full-path`, resolved through the ODF
    /// manifest (Phase 13.3). Metadata reports the declared media type and physical
    /// ordinal; `ExactBytes`/`DecodedBytes` resolve to the member span. Never a fetch.
    #[cfg(feature = "odt")]
    OdtPart(String),
    /// A body-level OpenDocument paragraph (`text:p`), by 0-based index among
    /// paragraphs (headings excluded).
    #[cfg(feature = "odt")]
    OdtParagraph {
        /// The paragraph index.
        index: u32,
        /// The extraction profile identity.
        profile: OdtExtractProfile,
    },
    /// A body-level OpenDocument heading (`text:h`), by 0-based index among
    /// headings.
    #[cfg(feature = "odt")]
    OdtHeading {
        /// The heading index.
        index: u32,
        /// The extraction profile identity.
        profile: OdtExtractProfile,
    },
    /// A top-level OpenDocument table (`table:table`), by 0-based index.
    #[cfg(feature = "odt")]
    OdtTable {
        /// The table index.
        index: u32,
        /// The extraction profile identity.
        profile: OdtExtractProfile,
    },
    /// A table cell of an OpenDocument table, by **physical** `(table, row, col)`
    /// position (spans are reported, never projected).
    #[cfg(feature = "odt")]
    OdtCell {
        /// The 0-based table index.
        table: u32,
        /// The 0-based row index.
        row: u32,
        /// The 0-based physical cell index within the row.
        col: u32,
        /// The extraction profile identity.
        profile: OdtExtractProfile,
    },
    /// A top-level OpenDocument list (`text:list`), by 0-based index.
    #[cfg(feature = "odt")]
    OdtList {
        /// The list index.
        index: u32,
        /// The extraction profile identity.
        profile: OdtExtractProfile,
    },
    /// A text search over the OpenDocument blocks, scoped by the extraction profile.
    #[cfg(feature = "odt")]
    OdtFind {
        /// The pattern (case-sensitive substring).
        pattern: String,
        /// The extraction profile identity.
        profile: OdtExtractProfile,
    },
    /// An XLSX worksheet by 0-based workbook-order index (Phase 21.1.1). Hidden
    /// sheets are still addressable by index; the profile only governs whether a
    /// whole-workbook projection includes them.
    #[cfg(feature = "xlsx")]
    XlsxSheet {
        /// The 0-based workbook-order sheet index.
        index: u32,
        /// The extraction profile identity.
        profile: XlsxExtractProfile,
    },
    /// One XLSX cell, addressed by an A1-style reference (`B7`) and its sheet.
    /// The stored formula and the cached result are distinct facets of the same
    /// cell, never conflated.
    #[cfg(feature = "xlsx")]
    XlsxCell {
        /// The 0-based workbook-order sheet index.
        sheet: u32,
        /// The cell reference (`B7`).
        cell: String,
        /// The extraction profile identity.
        profile: XlsxExtractProfile,
    },
    /// A text search over worksheet cells, scoped by the extraction profile.
    #[cfg(feature = "xlsx")]
    XlsxFind {
        /// The pattern (case-sensitive substring).
        pattern: String,
        /// The extraction profile identity.
        profile: XlsxExtractProfile,
    },
    /// The parsed SpreadsheetML style table (Phase 21.1.2): custom number formats,
    /// fonts, fills, and the `cellXfs` composition. A distinct observation from a
    /// cell's value, formula, or span.
    #[cfg(feature = "xlsx")]
    XlsxStyles,
    /// The workbook's defined/named ranges (Phase 21.1.2).
    #[cfg(feature = "xlsx")]
    XlsxDefinedNames,
    /// The package's external relationships (Phase 21.1.2). Typed metadata only;
    /// an external target is an inert identifier and is never dereferenced.
    #[cfg(feature = "xlsx")]
    XlsxExternalRels,
    /// The cell comments of one worksheet, keyed by cell (Phase 21.1.2).
    #[cfg(feature = "xlsx")]
    XlsxComments {
        /// The 0-based workbook-order sheet index.
        sheet: u32,
    },
    /// The hyperlinks declared in one worksheet (Phase 21.1.2). Internal
    /// (`location`) and external (`r:id`) links are distinct observations.
    #[cfg(feature = "xlsx")]
    XlsxHyperlinks {
        /// The 0-based workbook-order sheet index.
        sheet: u32,
    },
    /// The tables referenced by one worksheet's `tableParts` (Phase 21.1.2).
    #[cfg(feature = "xlsx")]
    XlsxTables {
        /// The 0-based workbook-order sheet index.
        sheet: u32,
    },
    /// The drawing(s) referenced by one worksheet (Phase 21.1.2). Charts are never
    /// evaluated; only the drawing part and its relationship graph are exposed.
    #[cfg(feature = "xlsx")]
    XlsxDrawing {
        /// The 0-based workbook-order sheet index.
        sheet: u32,
    },
    /// One PPTX slide by 0-based presentation-order index (Phase 21.2.1). Hidden
    /// slides are still addressable by index; the profile only governs whether a
    /// whole-deck projection includes them.
    #[cfg(feature = "pptx")]
    PptxSlide {
        /// The 0-based presentation-order slide index.
        index: u32,
        /// The extraction profile identity.
        profile: PptxExtractProfile,
    },
    /// One PPTX shape, addressed by slide index and a flattened pre-order shape
    /// index. A shape's text and its kind are a distinct observation from the
    /// slide's XML span.
    #[cfg(feature = "pptx")]
    PptxShape {
        /// The 0-based presentation-order slide index.
        slide: u32,
        /// The flattened pre-order shape index within the slide.
        index: u32,
        /// The extraction profile identity.
        profile: PptxExtractProfile,
    },
    /// One PPTX notes slide by 0-based notes-part index (Phase 21.2.1).
    #[cfg(feature = "pptx")]
    PptxNotes {
        /// The 0-based notes-slide index.
        index: u32,
        /// The extraction profile identity.
        profile: PptxExtractProfile,
    },
    /// The presentation's slide-layout parts (Phase 21.2.1).
    #[cfg(feature = "pptx")]
    PptxLayouts,
    /// The presentation's slide-master parts (Phase 21.2.1).
    #[cfg(feature = "pptx")]
    PptxMasters,
    /// The presentation's theme parts (Phase 21.2.1).
    #[cfg(feature = "pptx")]
    PptxTheme,
    /// A PPTX media resource by 0-based ordinal (Phase 21.2.1).
    #[cfg(feature = "pptx")]
    PptxMedia {
        /// The 0-based media-part index.
        ordinal: u32,
    },
    /// The embedded tables of one slide (Phase 21.2.1).
    #[cfg(feature = "pptx")]
    PptxTables {
        /// The 0-based presentation-order slide index.
        slide: u32,
        /// The extraction profile identity.
        profile: PptxExtractProfile,
    },
    /// A lexical text search over slides (Phase 21.2.1).
    #[cfg(feature = "pptx")]
    PptxFind {
        /// The pattern (case-sensitive substring).
        pattern: String,
        /// The extraction profile identity.
        profile: PptxExtractProfile,
    },
}

impl Selector {
    /// Canonical selector text, e.g. `page:1` or `byte-range:10:4`.
    pub fn canonical(&self) -> String {
        match self {
            Selector::Document => "document".to_string(),
            Selector::Page(n) => format!("page:{n}"),
            Selector::Object(n) => format!("object:{n}"),
            Selector::Stream(n) => format!("stream:{n}"),
            Selector::Revision(n) => format!("revision:{n}"),
            Selector::Revisions => "revisions".to_string(),
            Selector::ExternalLineage => "external-lineage".to_string(),
            Selector::Member(n) => format!("member:{n}"),
            Selector::PackagePart(name) => format!("package-part:{name}"),
            Selector::Relationship(id) => format!("relationship:{id}"),
            Selector::ByteRange { offset, len } => format!("byte-range:{offset}:{len}"),
            Selector::TextMatch(p) => format!("text-match:{p}"),
            Selector::Metadata => "metadata".to_string(),
            Selector::Text => "text".to_string(),
            Selector::Heading(n) => format!("heading:{n}"),
            Selector::Block(n) => format!("block:{n}"),
            Selector::Table(n) => format!("table:{n}"),
            Selector::Cell { table, row, col } => format!("cell:{table}:{row}:{col}"),
            Selector::Resource(n) => format!("resource:{n}"),
            Selector::Link(n) => format!("link:{n}"),
            Selector::SearchMatch(p) => format!("search-match:{p}"),
            #[cfg(feature = "docx")]
            Selector::DocxStory { story, profile } => {
                format!(
                    "docx-story:{};profile={}",
                    story.name(),
                    profile.fingerprint()
                )
            }
            #[cfg(feature = "docx")]
            Selector::DocxParagraph {
                story,
                index,
                profile,
            } => format!(
                "docx-paragraph:{}:{};profile={}",
                story.name(),
                index,
                profile.fingerprint()
            ),
            #[cfg(feature = "docx")]
            Selector::DocxTable {
                story,
                index,
                profile,
            } => format!(
                "docx-table:{}:{};profile={}",
                story.name(),
                index,
                profile.fingerprint()
            ),
            #[cfg(feature = "docx")]
            Selector::DocxCell {
                story,
                table,
                cell,
                profile,
            } => format!(
                "docx-cell:{}:{}:{};profile={}",
                story.name(),
                table,
                cell,
                profile.fingerprint()
            ),
            #[cfg(feature = "docx")]
            Selector::DocxFind {
                story,
                pattern,
                profile,
            } => format!(
                "docx-find:{}:{};profile={}",
                story.name(),
                pattern,
                profile.fingerprint()
            ),
            #[cfg(feature = "epub")]
            Selector::EpubPackage => "epub-package".to_string(),
            #[cfg(feature = "epub")]
            Selector::EpubManifestItem { index } => format!("epub-manifest-item:{index}"),
            #[cfg(feature = "epub")]
            Selector::EpubSpineItem { index, profile } => {
                format!("epub-spine-item:{index};profile={}", profile.fingerprint())
            }
            #[cfg(feature = "epub")]
            Selector::EpubNav => "epub-nav".to_string(),
            #[cfg(feature = "epub")]
            Selector::EpubNavNode { index } => format!("epub-nav-node:{index}"),
            #[cfg(feature = "epub")]
            Selector::EpubResource(name) => format!("epub-resource:{name}"),
            #[cfg(feature = "epub")]
            Selector::EpubBlock {
                index,
                block,
                profile,
            } => format!(
                "epub-block:{index}:{block};profile={}",
                profile.fingerprint()
            ),
            #[cfg(feature = "epub")]
            Selector::EpubCell {
                index,
                table,
                row,
                col,
                profile,
            } => format!(
                "epub-cell:{index}:{table}:{row}:{col};profile={}",
                profile.fingerprint()
            ),
            #[cfg(feature = "epub")]
            Selector::EpubLink {
                index,
                link,
                profile,
            } => format!("epub-link:{index}:{link};profile={}", profile.fingerprint()),
            #[cfg(feature = "epub")]
            Selector::EpubFind {
                index,
                pattern,
                profile,
            } => format!(
                "epub-find:{index}:{pattern};profile={}",
                profile.fingerprint()
            ),
            #[cfg(feature = "odt")]
            Selector::OdtPart(name) => format!("odt-part:{name}"),
            #[cfg(feature = "odt")]
            Selector::OdtParagraph { index, profile } => {
                format!("odt-paragraph:{index};profile={}", profile.fingerprint())
            }
            #[cfg(feature = "odt")]
            Selector::OdtHeading { index, profile } => {
                format!("odt-heading:{index};profile={}", profile.fingerprint())
            }
            #[cfg(feature = "odt")]
            Selector::OdtTable { index, profile } => {
                format!("odt-table:{index};profile={}", profile.fingerprint())
            }
            #[cfg(feature = "odt")]
            Selector::OdtCell {
                table,
                row,
                col,
                profile,
            } => format!(
                "odt-cell:{table}:{row}:{col};profile={}",
                profile.fingerprint()
            ),
            #[cfg(feature = "odt")]
            Selector::OdtList { index, profile } => {
                format!("odt-list:{index};profile={}", profile.fingerprint())
            }
            #[cfg(feature = "odt")]
            Selector::OdtFind { pattern, profile } => {
                format!("odt-find:{pattern};profile={}", profile.fingerprint())
            }
            #[cfg(feature = "xlsx")]
            Selector::XlsxSheet { index, profile } => {
                format!("xlsx-sheet:{index};profile={}", profile.fingerprint())
            }
            #[cfg(feature = "xlsx")]
            Selector::XlsxCell {
                sheet,
                cell,
                profile,
            } => format!("xlsx-cell:{sheet}:{cell};profile={}", profile.fingerprint()),
            #[cfg(feature = "xlsx")]
            Selector::XlsxFind { pattern, profile } => {
                format!("xlsx-find:{pattern};profile={}", profile.fingerprint())
            }
            #[cfg(feature = "xlsx")]
            Selector::XlsxStyles => "xlsx-styles".to_string(),
            #[cfg(feature = "xlsx")]
            Selector::XlsxDefinedNames => "xlsx-defined-names".to_string(),
            #[cfg(feature = "xlsx")]
            Selector::XlsxExternalRels => "xlsx-external-rels".to_string(),
            #[cfg(feature = "xlsx")]
            Selector::XlsxComments { sheet } => format!("xlsx-comments:{sheet}"),
            #[cfg(feature = "xlsx")]
            Selector::XlsxHyperlinks { sheet } => format!("xlsx-hyperlinks:{sheet}"),
            #[cfg(feature = "xlsx")]
            Selector::XlsxTables { sheet } => format!("xlsx-tables:{sheet}"),
            #[cfg(feature = "xlsx")]
            Selector::XlsxDrawing { sheet } => format!("xlsx-drawing:{sheet}"),
            #[cfg(feature = "pptx")]
            Selector::PptxSlide { index, profile } => {
                format!("pptx-slide:{index};profile={}", profile.fingerprint())
            }
            #[cfg(feature = "pptx")]
            Selector::PptxShape {
                slide,
                index,
                profile,
            } => format!(
                "pptx-shape:{slide}:{index};profile={}",
                profile.fingerprint()
            ),
            #[cfg(feature = "pptx")]
            Selector::PptxNotes { index, profile } => {
                format!("pptx-notes:{index};profile={}", profile.fingerprint())
            }
            #[cfg(feature = "pptx")]
            Selector::PptxLayouts => "pptx-layouts".to_string(),
            #[cfg(feature = "pptx")]
            Selector::PptxMasters => "pptx-masters".to_string(),
            #[cfg(feature = "pptx")]
            Selector::PptxTheme => "pptx-theme".to_string(),
            #[cfg(feature = "pptx")]
            Selector::PptxMedia { ordinal } => format!("pptx-media:{ordinal}"),
            #[cfg(feature = "pptx")]
            Selector::PptxTables { slide, profile } => {
                format!("pptx-tables:{slide};profile={}", profile.fingerprint())
            }
            #[cfg(feature = "pptx")]
            Selector::PptxFind { pattern, profile } => {
                format!("pptx-find:{pattern};profile={}", profile.fingerprint())
            }
        }
    }

    /// Whether this selector belongs to the format-neutral common vocabulary
    /// (Phase 12.7). Common selectors dispatch through the detected format's
    /// adapter; native selectors are first-class peers, never fallbacks.
    pub fn is_common(&self) -> bool {
        matches!(
            self,
            Selector::Metadata
                | Selector::Text
                | Selector::Heading(_)
                | Selector::Block(_)
                | Selector::Table(_)
                | Selector::Cell { .. }
                | Selector::Resource(_)
                | Selector::Link(_)
                | Selector::SearchMatch(_)
        )
    }
}

/// How the selected thing should be represented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Representation {
    /// Document-level metadata.
    Metadata,
    /// Extracted text runs.
    Text,
    /// A structured page description.
    Structure,
    /// A decoded content operator stream.
    Operators,
    /// Exact encoded stream bytes.
    EncodedBytes,
    /// Decoded (inflated) stream bytes.
    DecodedBytes,
    /// Exact source bytes.
    ExactBytes,
    /// A deterministic structured page preview.
    Preview,
    /// A structured metadata projection (e.g. a PDF revision lineage, Phase 17).
    Lineage,
    /// The full source document.
    FullDocument,
}

impl Representation {
    /// Stable lower-case name.
    pub const fn name(self) -> &'static str {
        match self {
            Representation::Metadata => "metadata",
            Representation::Text => "text",
            Representation::Structure => "structure",
            Representation::Operators => "operators",
            Representation::EncodedBytes => "encoded",
            Representation::DecodedBytes => "decoded",
            Representation::ExactBytes => "exact",
            Representation::Preview => "preview",
            Representation::Lineage => "lineage",
            Representation::FullDocument => "full",
        }
    }
}

/// Output bounds for one observation.
#[derive(Debug, Clone, Copy)]
pub struct ObserveBudget {
    /// Maximum bytes the returned value may occupy.
    pub max_output_bytes: u64,
    /// Maximum seed nodes the observation may evaluate.
    pub max_nodes: u64,
}

impl Default for ObserveBudget {
    fn default() -> Self {
        ObserveBudget {
            max_output_bytes: 64 * 1024 * 1024,
            max_nodes: 1 << 20,
        }
    }
}

/// One observation request.
#[derive(Debug, Clone)]
pub struct ObserveRequest {
    /// What to observe.
    pub selector: Selector,
    /// How to represent it.
    pub representation: Representation,
    /// Output bounds.
    pub budget: ObserveBudget,
    /// Whether the disposable derived cache (11.8) may be consulted and filled.
    /// `false` forces a cold, recompute-everything court.
    pub use_cache: bool,
}

impl ObserveRequest {
    /// A request with the default budget, caching enabled.
    pub fn new(selector: Selector, representation: Representation) -> Self {
        ObserveRequest {
            selector,
            representation,
            budget: ObserveBudget::default(),
            use_cache: true,
        }
    }
}

/// Which descriptor read path an observation took.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DescriptorReadMode {
    /// The whole `.voldoc` blob was read and parsed (the archival/full path).
    #[default]
    Full,
    /// A seek-based partial read served only the record closure the query needs.
    Partial,
}

impl DescriptorReadMode {
    /// Stable lower-case name (used in EXPLAIN ANALYZE JSON).
    pub const fn name(self) -> &'static str {
        match self {
            DescriptorReadMode::Full => "full",
            DescriptorReadMode::Partial => "partial",
        }
    }
}

/// Statistics of one observation — the evidence surface (ADR-0027).
///
/// Peak RSS and CPU time are deliberately **not** claimed here: `std` exposes no
/// portable CPU-time API, and peak RSS is a Linux-only `/proc` read that the court
/// already measures externally (the court reads `Maximum resident set size` from
/// `/usr/bin/time -v`). Claiming either in-process would add a platform-specific, easily-misread field for no gain.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObserveStats {
    /// Index entries consulted during the observation (the reference descent is
    /// counted by the entries it returns).
    pub index_nodes_read: u64,
    /// Seed nodes fetched from the seed store.
    pub seed_nodes_fetched: u64,
    /// Seed nodes evaluated/materialized (including dependencies). With reuse
    /// enabled this counts cache misses only, so it never exceeds
    /// `seed_nodes_executed`.
    pub seed_nodes_materialized: u64,
    /// Seed nodes actually executed during this observation (cache misses).
    pub seed_nodes_executed: u64,
    /// Seed nodes served whole from the persisted derived cache (their subtrees
    /// were not traversed).
    pub seed_nodes_reused: u64,
    /// Output bytes written to the derived cache during this observation.
    pub cache_bytes_written: u64,
    /// Content-shared representation fact (Phase 12.8): the number of seed nodes
    /// this field's **ingest** found already present by content id, so it wrote
    /// nothing for them. This is deliberately distinct from
    /// [`Self::seed_nodes_reused`], which is a *work* fact about this
    /// observation. Read from the manifest provenance; `0` for an older field.
    pub nodes_id_shared: u64,
    /// Resource blobs this field shares with an earlier document (Phase 12.8);
    /// `0` when the field shares none. Read from the manifest provenance.
    pub shared_resource_ids: u64,
    /// Descriptor bytes physically fetched to open this observation's field.
    /// For the full path this is the whole `.voldoc` blob; for the seek-based
    /// partial path it is only the record closure the query needed (see
    /// [`Self::descriptor_read_mode`]). It is **not** hidden behind `bytes_read`.
    pub descriptor_bytes_read: u64,
    /// Which descriptor read path produced [`Self::descriptor_bytes_read`]:
    /// `full` for the whole-blob parse, `partial` for a seek-based closure read.
    pub descriptor_read_mode: DescriptorReadMode,
    /// Field-manifest bytes physically fetched.
    pub manifest_bytes_read: u64,
    /// Hierarchical-index-node bytes physically fetched.
    pub index_bytes_read: u64,
    /// Seed-node bytes physically fetched (`get_node` + `get_node_range`).
    pub seed_bytes_read: u64,
    /// Total **physical** bytes fetched by this observation:
    /// `descriptor_bytes_read + manifest_bytes_read + index_bytes_read +
    /// seed_bytes_read`. Unrelated to `bytes_returned` (the output size): a
    /// narrow observation can return fewer bytes than it reads.
    pub bytes_read: u64,
    /// Bytes returned to the caller.
    pub bytes_returned: u64,
    /// Whether a Stage-C promotion (deepening) happened during this observation.
    pub deepened: bool,
    /// Decoded package members this observation required (Phase 12.7), counted
    /// where the adapter resolves them. A member served whole from the persisted
    /// cache still counts as required; [`Self::seed_nodes_reused`] tells you it was
    /// not re-executed, so a warm observation can report the requirement without
    /// claiming fresh work.
    pub member_decodes: u64,
    /// Materialization requests for **XML-derived model nodes**
    /// (`PackageOpcModel`/`DocxModel`/`DocxStory`/`EpubModel`/`EpubContent`) at the
    /// observation boundary (Phase 12.7). Same honest scope as [`Self::member_decodes`].
    pub xml_parses: u64,
    /// Wall-clock duration in microseconds.
    pub wall_micros: u64,
}

/// A seed-store wrapper that counts node *fetches*. All physical bytes are
/// accounted by the underlying store's [`crate::store::IoCounters`]; this wrapper
/// only exposes the fetch count for `seed_nodes_fetched`. Generic over the seed
/// substrate so a test can substitute a store that forbids enumeration.
struct CountingSeedStore<S: SeedStore> {
    inner: S,
    gets: Cell<u64>,
}

impl<S: SeedStore> CountingSeedStore<S> {
    fn new(inner: S) -> Self {
        CountingSeedStore {
            inner,
            gets: Cell::new(0),
        }
    }

    fn gets(&self) -> u64 {
        self.gets.get()
    }

    fn note(&self) {
        self.gets.set(self.gets.get() + 1);
    }
}

impl<S: SeedStore> SeedStore for CountingSeedStore<S> {
    fn put_node(&mut self, canonical: &[u8]) -> Result<NodeId> {
        self.inner.put_node(canonical)
    }

    fn get_node(&self, id: &NodeId) -> Result<Vec<u8>> {
        self.note();
        self.inner.get_node(id)
    }

    fn get_node_range(&self, id: &NodeId, offset: u64, len: u64) -> Result<Vec<u8>> {
        self.note();
        self.inner.get_node_range(id, offset, len)
    }

    fn contains_node(&self, id: &NodeId) -> Result<bool> {
        self.inner.contains_node(id)
    }

    fn list_nodes(&self) -> Result<Vec<(NodeId, u64)>> {
        self.inner.list_nodes()
    }
}

/// Answer an `ExternalLineage` observation directly from the field's external
/// context sidecar — without opening the field, the descriptor, the index, the
/// disposable cache, or the seed DAG.
///
/// Returns `None` for any other request, so the document-derived path is
/// untouched whenever the external selector is not used: attaching, querying,
/// or removing the context can never change a plain observation's answer.
///
/// The answer is `Basis::ExternalMetadata` and never exact; it reads no seed
/// node (`dependency_ids` is empty, `integrity_scope` is `None`). The sidecar
/// read is *not* one of the four document-derived byte classes `ObserveStats`
/// accounts, so it is reported explicitly in the answer's `provenance`.
fn external_lineage_answer(
    store: &FieldStore,
    id: &FieldId,
    req: &ObserveRequest,
    started: Instant,
) -> Option<Result<(FieldAnswer, ObserveStats, FieldId)>> {
    if !matches!(
        (&req.selector, req.representation),
        (Selector::ExternalLineage, Representation::Lineage)
    ) {
        return None;
    }
    Some((|| {
        let ctx = store.get_external_context(id)?.ok_or_else(|| {
            Error::unsupported_feature(
                "no external context is attached to this field; external lineage \
                 is supplied explicitly and is never inferred from document bytes",
            )
        })?;
        let json = ctx.answer_json();
        let bytes_returned = json.len() as u64;
        let answer = FieldAnswer {
            value: AnswerValue::Json(json),
            basis: Basis::ExternalMetadata,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: None,
            provenance: format!(
                "external-context;origin={};source={};external_bytes_read={}",
                ctx.origin.name(),
                ctx.source,
                ctx.encode_canonical().len(),
            ),
            dependency_ids: Vec::new(),
            integrity_scope: IntegrityScope::None,
            exact: false,
        };
        let stats = ObserveStats {
            bytes_returned,
            wall_micros: started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64,
            ..ObserveStats::default()
        };
        Ok((answer, stats, *id))
    })())
}

/// Observe one selector/representation pair.
///
/// Returns the answer, its [`ObserveStats`], and the **current/promoted** field
/// id: the promoted id when a Stage-C deepen happened during this call, else the
/// input id. A caller can chain the returned id to observe again without
/// re-deepening.
pub fn observe(
    store: &mut FieldStore,
    id: &FieldId,
    req: &ObserveRequest,
    limits: Limits,
) -> Result<(FieldAnswer, ObserveStats, FieldId)> {
    let started = Instant::now();
    if let Some(r) = external_lineage_answer(store, id, req, started) {
        return r;
    }
    match narrow_probe(store, id, req)? {
        // The target is served wholly from the disposable derived cache: the
        // descriptor is never opened. The ordinary evaluation core still runs,
        // against a trip-wire source, so the answer and every work counter are
        // those of the normal path while `descriptor_bytes_read` stays zero.
        NarrowProbe::Probed {
            hit: true,
            manifest,
            carry,
        } => {
            let view = FieldView {
                manifest: manifest.as_ref(),
                id: *id,
                open_io: IoSnapshot::default(),
                source: &NO_SOURCE,
                loader: None,
                object_count: 0,
                graph_ops: 0,
                read_mode: DescriptorReadMode::Partial,
            };
            let (seeds, istore) = open_sub_stores(store)?;
            observe_with_stores_pre(
                store,
                view,
                req,
                limits,
                started,
                seeds,
                &istore,
                carry,
                ModelMemo::default(),
            )
        }
        // The selector resolved from the manifest + hierarchical index, but the
        // target is not cached: fall through to the normal path, reusing the
        // manifest, the probe's physical bytes, and its resolved index entries
        // so nothing is read a second time.
        NarrowProbe::Probed {
            hit: false,
            manifest,
            carry,
        } => {
            let opened = OpenedField::open_with_manifest(store, req, *manifest, limits)?;
            observe_view_pre(store, opened.view(), req, limits, started, carry)
        }
        NarrowProbe::NotEligible => {
            let opened = OpenedField::open(store, id, req, limits)?;
            observe_view(store, opened.view(), req, limits, started)
        }
    }
}

/// A [`SourceServer`] that serves nothing. A fully cache-served observation must
/// never call it; reaching it means the short-circuit admitted a request it
/// could not answer from the cache, which is a hard internal invariant failure
/// rather than a silent descriptor read.
struct NoSource;

static NO_SOURCE: NoSource = NoSource;

impl SourceServer for NoSource {
    fn serve_range(&self, _offset: u64, _len: u64, _limits: Limits) -> Result<Vec<u8>> {
        Err(Error::internal_invariant(
            "a cache-served observation attempted a descriptor range read",
        ))
    }

    fn serve_document(&self, _limits: Limits) -> Result<Vec<u8>> {
        Err(Error::internal_invariant(
            "a cache-served observation attempted a descriptor document read",
        ))
    }
}

/// Hierarchical-index entries the cache-first probe already resolved, so the
/// evaluation that follows never reads the same index nodes a second time.
#[derive(Default)]
struct PrefetchedIndex {
    entries: Vec<(SelectorKey, Vec<IndexEntry>)>,
}

impl PrefetchedIndex {
    fn insert(&mut self, key: SelectorKey, entries: Vec<IndexEntry>) {
        self.entries.push((key, entries));
    }

    fn get(&self, key: &SelectorKey) -> Option<&Vec<IndexEntry>> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
}

/// State the cache-first probe already produced, carried into the path that
/// follows so nothing it fetched, resolved, or read is done twice.
#[derive(Default)]
struct ProbeCarry {
    /// Physical bytes the probe fetched before the field was opened.
    base_io: IoSnapshot,
    /// Hierarchical-index entries the probe resolved.
    prefetched: PrefetchedIndex,
    /// The target's cache bytes, already integrity-checked by the probe, so the
    /// evaluation core serves them without re-reading the cache file.
    output: Option<(NodeId, Vec<u8>)>,
}

/// The outcome of the cache-first narrow probe.
enum NarrowProbe {
    /// The request is not one the short-circuit serves.
    NotEligible,
    /// The selector resolved without reading the descriptor. `hit` is whether
    /// the target derived node is served by the disposable cache.
    Probed {
        hit: bool,
        manifest: Box<FieldRoot>,
        carry: ProbeCarry,
    },
}

/// Whether a request is one the cache-first short-circuit serves: caching on, a
/// backend that supports the seek-based partial descriptor, and a selector/
/// representation whose target the manifest + index can resolve alone.
fn probe_eligible(store: &FieldStore, req: &ObserveRequest) -> bool {
    use Representation as R;
    // The short-circuit is a further step of the seek-based *partial* lane: on a
    // backend with no partial descriptor (EntropyFS) the honest label would be
    // `full`, so leave that path unchanged.
    req.use_cache
        && store.supports_partial_descriptor()
        && matches!(
            (&req.selector, req.representation),
            (Selector::Page(_), R::Text | R::Preview | R::Structure)
                | (Selector::Stream(_), R::DecodedBytes | R::Operators)
        )
}

/// The cache-first probe over a field the caller has **already opened**: the
/// parsed `manifest` and an `istore` the caller keeps open are supplied, so the
/// probe re-reads neither. [`narrow_probe`] is the cold-path wrapper that reads
/// the manifest and opens the index store first.
fn narrow_probe_open(
    store: &FieldStore,
    manifest: &FieldRoot,
    istore: &FsIndexStore,
    req: &ObserveRequest,
) -> Result<NarrowProbe> {
    if !probe_eligible(store, req) {
        return Ok(NarrowProbe::NotEligible);
    }
    let io_before = store.io().snapshot();
    narrow_probe_core(store, manifest, istore, io_before, req)
}

/// Resolve a narrow observation's target node from the field manifest and the
/// hierarchical index **only** — never the descriptor — and report whether the
/// disposable cache can serve it whole.
///
/// The target ids are computed with the same constructors `ingest`/`deepen` use
/// ([`derived_nodes`], the `ContentOperators`/`PdfStreamDecoded` builders), so a
/// hit means the *identical* node the normal path would materialize. A miss
/// carries the manifest, the physical bytes the probe fetched, and the resolved
/// index entries back to the normal path so a cold observation pays nothing
/// extra.
fn narrow_probe(store: &FieldStore, id: &FieldId, req: &ObserveRequest) -> Result<NarrowProbe> {
    if !probe_eligible(store, req) {
        return Ok(NarrowProbe::NotEligible);
    }
    let io_before = store.io().snapshot();
    let manifest = store.get_field(id)?;
    let istore = FsIndexStore::open_with_io(store.root(), store.io().handle())?;
    narrow_probe_core(store, &manifest, &istore, io_before, req)
}

/// [`narrow_probe`] from an already-read manifest and open index store.
fn narrow_probe_core(
    store: &FieldStore,
    manifest: &FieldRoot,
    istore: &FsIndexStore,
    io_before: IoSnapshot,
    req: &ObserveRequest,
) -> Result<NarrowProbe> {
    use Representation as R;
    let mut prefetched = PrefetchedIndex::default();
    if !manifest.has_index() {
        // Nothing to resolve from; let the normal path produce its typed error.
        let base_io = io_before.delta(&store.io().snapshot());
        return Ok(NarrowProbe::Probed {
            hit: false,
            manifest: Box::new(manifest.clone()),
            carry: ProbeCarry {
                base_io,
                prefetched,
                output: None,
            },
        });
    }

    let root = NodeId::from_bytes(manifest.index_root);
    let seeds = store.seed_substrate();

    // Compute the deterministic target `(id, max_output_bytes)`.
    let target: Option<(NodeId, u64)> = match (&req.selector, req.representation) {
        (Selector::Page(page), R::Text | R::Preview | R::Structure) => {
            let key = SelectorKey::new(SEL_PAGE, *page);
            let entries = lookup(istore, &root, &key)?;
            prefetched.insert(key, entries.clone());
            match entries.first() {
                Some(entry) => {
                    let (ops, text, preview) = derived_nodes(*page, entry.node_id);
                    // The short-circuit only applies once the whole derived chain
                    // already exists, so the normal path cannot promote (deepen)
                    // and the answer is a pure cache read.
                    if seeds.contains_node(&ops.content_id())?
                        && seeds.contains_node(&text.content_id())?
                        && seeds.contains_node(&preview.content_id())?
                    {
                        let node = match req.representation {
                            R::Preview | R::Structure => preview,
                            _ => text,
                        };
                        Some((node.content_id(), node.limits.max_output_bytes))
                    } else {
                        None
                    }
                }
                None => None,
            }
        }
        (Selector::Stream(object), R::DecodedBytes | R::Operators) => {
            let enc_key = SelectorKey::new(SEL_STREAM, *object);
            let enc = lookup(istore, &root, &enc_key)?;
            prefetched.insert(enc_key, enc.clone());
            let dec_key = SelectorKey::new(SEL_STREAM_DECODED, *object);
            let dec = lookup(istore, &root, &dec_key)?;
            prefetched.insert(dec_key, dec.clone());
            // The normal path requires a `SEL_STREAM` entry and, for a pure cache
            // hit, an already-registered decoded node; otherwise it would deepen
            // from the descriptor.
            if enc.is_empty() {
                None
            } else {
                match dec.first() {
                    // The index entry's id *is* the decoded node's content id.
                    // Every decoded node is built with `NodeLimits::DEFAULT`.
                    Some(entry) if req.representation == R::DecodedBytes => Some((
                        entry.node_id,
                        crate::field::node::NodeLimits::DEFAULT.max_output_bytes,
                    )),
                    Some(entry) => {
                        let node = SeedNode::new(
                            NodeKind::ContentOperators,
                            0,
                            Vec::new(),
                            vec![entry.node_id],
                            "pdf:content-operators",
                        );
                        Some((node.content_id(), node.limits.max_output_bytes))
                    }
                    None => None,
                }
            }
        }
        _ => None,
    };

    // Read the target's cached bytes at most once here: a hit is served from
    // this buffer, so neither the cache nor the descriptor is read again.
    let (hit, output) = match target {
        // Mirror `dag::materialize_inner`'s hit guard exactly: a cache error or
        // an oversized entry is a miss, never a wrong answer.
        Some((target_id, max_output_bytes)) => {
            match DerivedCache::open(store.root().join("cache"))?.get(&target_id) {
                Ok(Some(bytes)) if bytes.len() as u64 <= max_output_bytes => {
                    (true, Some((target_id, bytes)))
                }
                _ => (false, None),
            }
        }
        None => (false, None),
    };
    let base_io = io_before.delta(&store.io().snapshot());
    Ok(NarrowProbe::Probed {
        hit,
        manifest: Box::new(manifest.clone()),
        carry: ProbeCarry {
            base_io,
            prefetched,
            output,
        },
    })
}

/// Observe against an already-opened [`OpenedField`], for callers that open once
/// and both plan and evaluate (e.g. EXPLAIN ANALYZE).
pub(crate) fn observe_opened(
    store: &mut FieldStore,
    opened: &OpenedField,
    req: &ObserveRequest,
    limits: Limits,
) -> Result<(FieldAnswer, ObserveStats, FieldId)> {
    let started = Instant::now();
    if let Some(r) = external_lineage_answer(store, &opened.manifest().content_id(), req, started) {
        return r;
    }
    observe_view(store, opened.view(), req, limits, started)
}

/// Observe against an **already-open** field, opening nothing extra.
///
/// This is the single-open entry point (review fix #2): [`crate::field::explain::explain_analyze`]
/// opens the field once, plans against it, and then evaluates the observation
/// here, so the descriptor blob is read exactly once per analysis instead of
/// twice. Physical bytes are attributed from the store's I/O counters, so the
/// bytes read to open `field` are still reported honestly.
pub fn observe_with_field(
    store: &mut FieldStore,
    field: &Field,
    req: &ObserveRequest,
    limits: Limits,
) -> Result<(FieldAnswer, ObserveStats, FieldId)> {
    let started = Instant::now();
    if let Some(r) = external_lineage_answer(store, &field.id(), req, started) {
        return r;
    }
    observe_view(store, FieldView::from_field(field), req, limits, started)
}

/// Open the index store a resident session keeps across observations, sharing
/// the field store's I/O counters so every index read it serves is charged to
/// the session's observations rather than a private, discarded counter set.
pub(crate) fn open_session_index(store: &FieldStore) -> Result<FsIndexStore> {
    FsIndexStore::open_with_io(store.root(), store.io().handle())
}

/// Observe against an already-open field for the resident
/// [`crate::field::session::DocumentFieldSession`], with the same cache-first
/// short-circuit the cold [`observe`] path takes.
///
/// The session hoists what the cold [`narrow_probe`] re-fetches on every call:
/// the parsed manifest comes from the already-open `field`, and `index` is an
/// index store kept open across observations. A fully-cached `(Page,
/// Text|Preview|Structure)` or `(Stream, DecodedBytes|Operators)` request is
/// therefore served without touching the descriptor. A miss or an ineligible
/// request falls through to the ordinary evaluation core — and since the field
/// is already open, even a miss never re-opens the descriptor. `open_io` is the
/// session's one-time field-open cost, attributed to this observation only (the
/// session passes it on its first call, default after).
pub(crate) fn observe_session(
    store: &mut FieldStore,
    field: &Field,
    index: &FsIndexStore,
    open_io: IoSnapshot,
    req: &ObserveRequest,
    limits: Limits,
    models: ModelMemo,
) -> Result<(FieldAnswer, ObserveStats, FieldId)> {
    crate::field::prof::reset();
    let started = Instant::now();
    if let Some(r) = external_lineage_answer(store, &field.id(), req, started) {
        return r;
    }
    let prof = crate::field::prof::enabled();
    let t_probe = crate::field::prof::start();
    let probe = narrow_probe_open(store, field.manifest(), index, req);
    crate::field::prof::add_probe(t_probe);
    let result = match probe? {
        // The target is served wholly from the disposable derived cache: the
        // descriptor is never opened. The ordinary evaluation core still runs,
        // against a trip-wire source, so the answer and every work counter are
        // those of the normal path while `descriptor_bytes_read` excludes any
        // fresh descriptor read.
        NarrowProbe::Probed {
            hit: true,
            manifest,
            carry,
        } => {
            let view = FieldView {
                manifest: manifest.as_ref(),
                id: field.id(),
                open_io,
                source: &NO_SOURCE,
                loader: None,
                object_count: 0,
                graph_ops: 0,
                read_mode: DescriptorReadMode::Partial,
            };
            let seeds = CountingSeedStore::new(store.seed_substrate());
            observe_with_stores_pre(
                store, view, req, limits, started, seeds, index, carry, models,
            )
        }
        // The selector resolved from the manifest + index, but the target is not
        // cached: continue on the resident full path, reusing the probe's
        // already-counted bytes and resolved index entries so nothing is read
        // twice.
        NarrowProbe::Probed {
            hit: false, carry, ..
        } => {
            let mut view = FieldView::from_field(field);
            view.open_io = open_io;
            let seeds = CountingSeedStore::new(store.seed_substrate());
            observe_with_stores_pre(
                store, view, req, limits, started, seeds, index, carry, models,
            )
        }
        NarrowProbe::NotEligible => {
            let mut view = FieldView::from_field(field);
            view.open_io = open_io;
            let seeds = CountingSeedStore::new(store.seed_substrate());
            observe_with_stores_pre(
                store,
                view,
                req,
                limits,
                started,
                seeds,
                index,
                ProbeCarry::default(),
                models,
            )
        }
    };
    crate::field::prof::add_observe(Some(started));
    if prof {
        let s = crate::field::prof::take();
        eprintln!(
            "[vole-profile-stage] observe_us={} probe_us={} lookup_calls={} index_nodes={} index_read_us={} index_parse_us={} dispatch_us={} materialize_us={}",
            s.observe_us,
            s.probe_us,
            s.lookup_calls,
            s.index_nodes,
            s.index_read_us,
            s.index_parse_us,
            s.dispatch_us,
            s.materialize_us
        );
    }
    result
}

/// A descriptor opened for one observation: the full parse, or a seek-based
/// partial loader when the request is narrow and the descriptor carries an op
/// table. Both expose a [`FieldView`] over the same evaluation core.
pub(crate) enum OpenedField {
    /// The whole `.voldoc` blob was read and parsed.
    Full(Box<Field>),
    /// Only the record closure the observation needs will be read.
    Partial(Box<PartialField>),
}

impl OpenedField {
    /// Open the cheapest descriptor path admissible for `req`.
    pub(crate) fn open(
        store: &FieldStore,
        id: &FieldId,
        req: &ObserveRequest,
        limits: Limits,
    ) -> Result<OpenedField> {
        if store.supports_partial_descriptor()
            && partial_eligible(req)
            && let Some(pf) = PartialField::try_open(store, id, limits)?
        {
            return Ok(OpenedField::Partial(Box::new(pf)));
        }
        Ok(OpenedField::Full(Box::new(Field::open(store, id, limits)?)))
    }

    /// Open the cheapest admissible path from an **already-read** manifest. The
    /// manifest bytes are charged by the caller (the narrow probe counts them in
    /// its `base_io`), so `open_io` here never re-reads them.
    pub(crate) fn open_with_manifest(
        store: &FieldStore,
        req: &ObserveRequest,
        manifest: FieldRoot,
        limits: Limits,
    ) -> Result<OpenedField> {
        if store.supports_partial_descriptor()
            && partial_eligible(req)
            && let Some(pf) =
                PartialField::finish_open(store, manifest.clone(), store.io().snapshot(), limits)?
        {
            return Ok(OpenedField::Partial(Box::new(pf)));
        }
        Ok(OpenedField::Full(Box::new(Field::open_after_manifest(
            store,
            manifest,
            store.io().snapshot(),
            limits,
        )?)))
    }

    /// A view over this opened field for the evaluation core.
    pub(crate) fn view(&self) -> FieldView<'_> {
        match self {
            OpenedField::Full(f) => FieldView::from_field(f),
            OpenedField::Partial(p) => p.view(),
        }
    }

    /// The field manifest.
    pub(crate) fn manifest(&self) -> &FieldRoot {
        match self {
            OpenedField::Full(f) => f.manifest(),
            OpenedField::Partial(p) => &p.manifest,
        }
    }
}

/// The metadata and source server one observation needs, independent of whether
/// the descriptor was fully parsed or partially loaded.
pub(crate) struct FieldView<'a> {
    pub manifest: &'a FieldRoot,
    pub id: FieldId,
    pub open_io: IoSnapshot,
    pub source: &'a dyn SourceServer,
    /// The partial loader, when this view came from one, so the evaluation can
    /// charge the bytes its record reads fetched.
    pub loader: Option<&'a PartialDescriptor>,
    pub object_count: usize,
    pub graph_ops: usize,
    pub read_mode: DescriptorReadMode,
}

impl<'a> FieldView<'a> {
    pub(crate) fn from_field(field: &'a Field) -> FieldView<'a> {
        let parsed = field.parsed();
        FieldView {
            manifest: field.manifest(),
            id: field.id(),
            open_io: field.open_io(),
            source: parsed,
            loader: None,
            object_count: parsed.descriptor.objects.len(),
            graph_ops: parsed.descriptor.program.ops.len(),
            read_mode: DescriptorReadMode::Full,
        }
    }
}

/// A field opened through the seek-based partial descriptor loader.
pub(crate) struct PartialField {
    pub(crate) manifest: FieldRoot,
    pub(crate) id: FieldId,
    pub(crate) open_io: IoSnapshot,
    loader: PartialDescriptor,
}

impl PartialField {
    /// Try to open `id` lazily. `Ok(None)` means the descriptor is ineligible
    /// (no op table, external objects, or a framing fault) and the caller must
    /// fall back to the full path.
    pub(crate) fn try_open(
        store: &FieldStore,
        id: &FieldId,
        limits: Limits,
    ) -> Result<Option<PartialField>> {
        let io_before = store.io().snapshot();
        let manifest = store.get_field(id)?;
        PartialField::finish_open(store, manifest, io_before, limits)
    }

    /// The body of [`PartialField::try_open`] from an already-read manifest.
    ///
    /// `io_before` is the snapshot `open_io` is measured from: pass one taken
    /// *before* the manifest read to charge it to this open (the ordinary
    /// path), or one taken after it to charge it elsewhere (the narrow probe,
    /// which already counted the manifest in its `base_io`).
    pub(crate) fn finish_open(
        store: &FieldStore,
        manifest: FieldRoot,
        io_before: IoSnapshot,
        limits: Limits,
    ) -> Result<Option<PartialField>> {
        let descriptor_id = Id::from_bytes(manifest.descriptor_id);
        let Some(path) = store.descriptor_path(&descriptor_id) else {
            // The descriptor is not a filesystem file (EntropyFS backend): the
            // seek-based partial lane is unavailable, so fall back to the full
            // descriptor path.
            return Ok(None);
        };
        let loader = match PartialDescriptor::open(&path, limits)? {
            PartialLoad::Ready(l) => l,
            PartialLoad::Ineligible { bytes_read } => {
                // Charge the bytes the inspection did fetch before declining, so
                // the honest fallback is not under-counted.
                store.io().add_descriptor(bytes_read);
                return Ok(None);
            }
        };
        if loader.source_len() != manifest.source_len
            || loader.source_sha256() != manifest.source_sha256
        {
            return Err(Error::integrity_mismatch(
                "partial descriptor does not match its field manifest's declared source",
            ));
        }
        // The loader's physical bytes are charged to the descriptor class after
        // the observation completes (once, so the read *count* stays one), so the
        // open snapshot carries only the manifest read here.
        let open_io = io_before.delta(&store.io().snapshot());
        let id = manifest.content_id();
        Ok(Some(PartialField {
            id,
            manifest,
            open_io,
            loader: *loader,
        }))
    }

    pub(crate) fn view(&self) -> FieldView<'_> {
        FieldView {
            manifest: &self.manifest,
            id: self.id,
            open_io: self.open_io,
            source: &self.loader,
            loader: Some(&self.loader),
            object_count: self.loader.object_count(),
            graph_ops: self.loader.graph_ops(),
            read_mode: DescriptorReadMode::Partial,
        }
    }
}

/// Whether a request is served by the seek-based partial lane when available.
fn partial_eligible(req: &ObserveRequest) -> bool {
    use Representation as R;
    matches!(
        (&req.selector, req.representation),
        (Selector::ByteRange { .. }, R::ExactBytes)
            | (Selector::Object(_), R::ExactBytes | R::EncodedBytes)
            | (Selector::Revision(_), R::ExactBytes)
            | (Selector::Stream(_), R::EncodedBytes)
            | (Selector::Member(_), R::EncodedBytes | R::DecodedBytes)
            | (Selector::Page(_), R::Text | R::Preview | R::Structure)
    )
}

/// Build the seed and index sub-stores, sharing the field store's I/O counters.
fn open_sub_stores(store: &FieldStore) -> Result<(CountingSeedStore<SeedSubstrate>, FsIndexStore)> {
    let io = store.io();
    let seeds = CountingSeedStore::new(store.seed_substrate());
    let istore = FsIndexStore::open_with_io(store.root(), io.handle())?;
    Ok((seeds, istore))
}

fn observe_view<'a>(
    store: &'a mut FieldStore,
    view: FieldView<'a>,
    req: &ObserveRequest,
    limits: Limits,
    started: Instant,
) -> Result<(FieldAnswer, ObserveStats, FieldId)> {
    observe_view_pre(store, view, req, limits, started, ProbeCarry::default())
}

/// [`observe_view`] with the cache-first probe's carried state: physical bytes it
/// already fetched, index entries it resolved, and integrity-checked cache bytes
/// — so the evaluation core never reads any of them a second time.
fn observe_view_pre<'a>(
    store: &'a mut FieldStore,
    view: FieldView<'a>,
    req: &ObserveRequest,
    limits: Limits,
    started: Instant,
    carry: ProbeCarry,
) -> Result<(FieldAnswer, ObserveStats, FieldId)> {
    let (seeds, istore) = open_sub_stores(store)?;
    observe_with_stores_pre(
        store,
        view,
        req,
        limits,
        started,
        seeds,
        &istore,
        carry,
        ModelMemo::default(),
    )
}

/// Test-only wrapper over [`observe_with_stores_pre`] with no probe state, so a
/// test can supply a seed store wrapper that forbids enumeration.
#[cfg(test)]
fn observe_with_stores<'a, S: SeedStore>(
    store: &'a mut FieldStore,
    view: FieldView<'a>,
    req: &ObserveRequest,
    limits: Limits,
    started: Instant,
    seeds: CountingSeedStore<S>,
    istore: FsIndexStore,
) -> Result<(FieldAnswer, ObserveStats, FieldId)> {
    observe_with_stores_pre(
        store,
        view,
        req,
        limits,
        started,
        seeds,
        &istore,
        ProbeCarry::default(),
        ModelMemo::default(),
    )
}

/// The evaluation core. Takes explicit sub-stores so a test can supply a seed
/// store wrapper that forbids enumeration; `carry` is the cache-first probe's
/// already-counted bytes, resolved index entries, and cached target bytes.
#[allow(clippy::too_many_arguments)]
fn observe_with_stores_pre<'a, S: SeedStore>(
    store: &'a mut FieldStore,
    view: FieldView<'a>,
    req: &ObserveRequest,
    limits: Limits,
    started: Instant,
    seeds: CountingSeedStore<S>,
    istore: &'a FsIndexStore,
    carry: ProbeCarry,
    models: ModelMemo,
) -> Result<(FieldAnswer, ObserveStats, FieldId)> {
    let ProbeCarry {
        base_io,
        prefetched,
        output,
    } = carry;
    // Snapshot after the field is open: only the reads this observation performs
    // during evaluation are counted as deltas; the field-open bytes come from
    // `view.open_io` below so they cannot be dropped on the floor.
    let io_base = store.io().snapshot();
    let budget = EvalBudget {
        max_nodes: req.budget.max_nodes,
        ..EvalBudget::default()
    };
    // Opt-in durable promotion (Phase 15.6): with `--promote` the cache becomes a
    // `GovernedCache` (durable `promoted/` first, disposable `cache/` second).
    // Off by default, so every existing court is byte-identical.
    let cache: Box<dyn OutputCache> = if req.use_cache && store.promote_policy().enabled {
        Box::new(GovernedCache::open(store.root(), store.governor())?)
    } else {
        Box::new(DerivedCache::open(store.root().join("cache"))?)
    };
    let field_id = view.id;
    let mut ctx = Ctx {
        store,
        manifest: view.manifest,
        source: view.source,
        loader: view.loader,
        open_io: view.open_io,
        object_count: view.object_count,
        graph_ops: view.graph_ops,
        read_mode: view.read_mode,
        seeds,
        istore,
        prefetched,
        prefetched_output: output,
        limits,
        budget,
        stats: ObserveStats::default(),
        use_cache: req.use_cache,
        cache,
        reuse: ReuseStats::default(),
        models,
        current_id: field_id,
    };

    let t_dispatch = crate::field::prof::start();
    let answer = ctx.dispatch(req)?;
    crate::field::prof::add_dispatch(t_dispatch);
    let produced = answer.value.byte_len();
    if produced > req.budget.max_output_bytes {
        return Err(Error::resource_limit(format!(
            "observation produced {produced} bytes, exceeding the {}-byte budget",
            req.budget.max_output_bytes
        )));
    }

    let mut stats = ctx.stats;
    // A partial loader reads its records lazily, so its physical bytes accrue
    // during dispatch; charge them once (one descriptor read *count*) before
    // closing the interval.
    if let Some(loader) = ctx.loader {
        ctx.store.io().add_descriptor(loader.bytes_read());
    }
    // Every physical byte fetched by this observation: the probe's `base_io`, the
    // field-open bytes, plus any additional reads (e.g. a Stage-C promotion)
    // performed during dispatch.
    let open = ctx.open_io;
    let extra = io_base.delta(&ctx.store.io().snapshot());
    stats.descriptor_bytes_read = base_io
        .descriptor_bytes
        .saturating_add(open.descriptor_bytes)
        .saturating_add(extra.descriptor_bytes);
    stats.descriptor_read_mode = ctx.read_mode;
    stats.manifest_bytes_read = base_io
        .manifest_bytes
        .saturating_add(open.manifest_bytes)
        .saturating_add(extra.manifest_bytes);
    stats.index_bytes_read = base_io.index_bytes.saturating_add(extra.index_bytes);
    stats.seed_bytes_read = base_io.seed_bytes.saturating_add(extra.seed_bytes);
    stats.bytes_read = stats
        .descriptor_bytes_read
        .saturating_add(stats.manifest_bytes_read)
        .saturating_add(stats.index_bytes_read)
        .saturating_add(stats.seed_bytes_read);
    stats.seed_nodes_fetched = ctx.seeds.gets();
    stats.seed_nodes_materialized = ctx.budget.nodes;
    stats.seed_nodes_executed = ctx.reuse.nodes_executed;
    stats.seed_nodes_reused = ctx.reuse.nodes_reused;
    stats.cache_bytes_written = ctx.reuse.cache_bytes_written;
    // Representation facts recorded at ingest (Phase 12.8): a same-id node is
    // *not* work reuse, so these are reported separately from `seed_nodes_reused`.
    stats.nodes_id_shared =
        crate::field::manifest::provenance_counter(view.manifest.provenance.as_str(), "id_shared");
    stats.shared_resource_ids =
        crate::field::manifest::provenance_counter(view.manifest.provenance.as_str(), "res_shared");
    stats.bytes_returned = produced;
    stats.wall_micros = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
    Ok((answer, stats, ctx.current_id))
}

/// In-memory, content-keyed memo of decoded typed models (Phase 15.2).
///
/// Keyed by the model node's `NodeId` = BLAKE3 of its canonical bytes, exactly
/// like `DerivedCache` (`src/field/cache.rs`): an unchanged closure is a hit, a
/// changed dependency is a miss. Disposable, off the exactness path, never
/// consulted for `materialize_exact`.
///
/// # Memory bound
///
/// The budget is a real byte cap on the **materialized node output** length. It
/// is a coarse clear-on-overflow rather than an LRU: the court serves one
/// document per session, so the maps hold 1-2 models; a long-lived session over
/// many fields grows until the budget is exceeded and then flushes. Either way
/// the memo cannot affect any answer — a miss simply recomputes.
#[derive(Clone, Default)]
pub(crate) struct ModelMemo {
    inner: Rc<RefCell<ModelMemoInner>>,
}

#[derive(Default)]
struct ModelMemoInner {
    /// Byte cap on materialized node output; `0` disables the memo.
    budget: u64,
    /// Approximate bytes currently held (costed by materialized node length).
    /// Only the `docx` maps are costed, so this is dead when that feature is off.
    #[cfg_attr(not(feature = "docx"), allow(dead_code))]
    used: u64,
    #[cfg(feature = "docx")]
    docx: HashMap<NodeId, Arc<DocxModel>>,
    #[cfg(feature = "docx")]
    story: HashMap<NodeId, Arc<StoryModel>>,
}

impl ModelMemo {
    pub(crate) fn with_budget(budget: u64) -> Self {
        let m = ModelMemo::default();
        m.inner.borrow_mut().budget = budget;
        m
    }

    // cost = the materialized node output length (`bytes.len()`); a coarse flush
    // when the budget is exceeded bounds memory without an LRU.
    #[cfg(feature = "docx")]
    fn get_docx(&self, id: &NodeId) -> Option<Arc<DocxModel>> {
        self.inner.borrow().docx.get(id).cloned()
    }

    #[cfg(feature = "docx")]
    fn put_docx(&self, id: NodeId, model: Arc<DocxModel>, cost: u64) {
        let mut inner = self.inner.borrow_mut();
        if inner.budget == 0 {
            return;
        }
        if inner.used.saturating_add(cost) > inner.budget {
            inner.docx.clear();
            inner.story.clear();
            inner.used = 0;
        }
        inner.used = inner.used.saturating_add(cost);
        inner.docx.insert(id, model);
    }

    #[cfg(feature = "docx")]
    fn get_story(&self, id: &NodeId) -> Option<Arc<StoryModel>> {
        self.inner.borrow().story.get(id).cloned()
    }

    #[cfg(feature = "docx")]
    fn put_story(&self, id: NodeId, model: Arc<StoryModel>, cost: u64) {
        let mut inner = self.inner.borrow_mut();
        if inner.budget == 0 {
            return;
        }
        if inner.used.saturating_add(cost) > inner.budget {
            inner.docx.clear();
            inner.story.clear();
            inner.used = 0;
        }
        inner.used = inner.used.saturating_add(cost);
        inner.story.insert(id, model);
    }
}

/// Observation execution context.
struct Ctx<'a, S: SeedStore> {
    store: &'a mut FieldStore,
    manifest: &'a FieldRoot,
    source: &'a dyn SourceServer,
    loader: Option<&'a PartialDescriptor>,
    open_io: IoSnapshot,
    object_count: usize,
    graph_ops: usize,
    read_mode: DescriptorReadMode,
    seeds: CountingSeedStore<S>,
    istore: &'a FsIndexStore,
    /// Index entries the cache-first probe already resolved, keyed by selector.
    prefetched: PrefetchedIndex,
    /// The target's cache bytes, already read and integrity-checked by the probe.
    prefetched_output: Option<(NodeId, Vec<u8>)>,
    limits: Limits,
    budget: EvalBudget,
    stats: ObserveStats,
    use_cache: bool,
    cache: Box<dyn OutputCache>,
    reuse: ReuseStats,
    /// Resident typed-model memo (Phase 15.2); consulted only when `use_cache`.
    /// Only the `docx` model paths read it, so it is dead without that feature.
    #[cfg_attr(not(feature = "docx"), allow(dead_code))]
    models: ModelMemo,
    current_id: FieldId,
}

/// A resolved DOCX story view: the parsed story model plus the provenance it is
/// bound to (backing part, dependency ids, and the exact compressed member span).
#[cfg(feature = "docx")]
struct DocxStoryView {
    model: Arc<StoryModel>,
    part: DocxPartRef,
    deps: Vec<NodeId>,
    span: Option<(u64, u64)>,
}

#[cfg(any(feature = "docx", feature = "odt"))]
#[cfg(any(feature = "docx", feature = "odt"))]
fn opt_u8_json(v: Option<u8>) -> String {
    match v {
        Some(n) => n.to_string(),
        None => "null".to_string(),
    }
}

#[cfg(any(feature = "docx", feature = "odt", feature = "xlsx", feature = "pptx"))]
fn opt_str_json(v: Option<&str>) -> String {
    match v {
        Some(s) => format!("\"{}\"", json_escape(s)),
        None => "null".to_string(),
    }
}

#[cfg(any(feature = "xlsx", feature = "pptx"))]
fn opt_u32_json(v: Option<u32>) -> String {
    match v {
        Some(n) => n.to_string(),
        None => "null".to_string(),
    }
}

/// Parse an A1-style cell reference (`B7`) into a 0-based grid column and a
/// 0-based row index. Column letters are case-insensitive; row numbers are
/// 1-based and must be non-zero.
#[cfg(feature = "docx")]
fn parse_cell_ref(s: &str) -> Option<(u32, u32)> {
    let letters: String = s.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let digits: String = s.chars().skip(letters.len()).collect();
    if letters.is_empty() || digits.is_empty() || digits.len() != s.len() - letters.len() {
        return None;
    }
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut col: u32 = 0;
    for c in letters.chars() {
        let v = c.to_ascii_uppercase() as u32 - 'A' as u32 + 1;
        col = col.checked_mul(26)?.checked_add(v)?;
    }
    let col = col.checked_sub(1)?;
    let row: u32 = digits.parse().ok()?;
    if row == 0 {
        return None;
    }
    Some((col, row - 1))
}

impl<S: SeedStore> Ctx<'_, S> {
    fn materialize(&mut self, node: &SeedNode) -> Result<Vec<u8>> {
        // Observation-boundary accounting (Phase 12.7): a requested XML-derived
        // model node is charged its class. This counts *requests* (a cache-served
        // request is still a request); `seed_nodes_reused` reports whether the
        // underlying work was reused rather than re-executed. Decoded-member
        // requests are counted where the adapter resolves them.
        match node.kind {
            NodeKind::PackageOpcModel
            | NodeKind::DocxModel
            | NodeKind::DocxStory
            | NodeKind::EpubModel
            | NodeKind::EpubContent
            | NodeKind::OdtModel
            | NodeKind::OdtContent
            | NodeKind::XlsxModel
            | NodeKind::XlsxWorkbook
            | NodeKind::XlsxSheet
            | NodeKind::PptxModel
            | NodeKind::PptxPresentation
            | NodeKind::PptxSlide
            | NodeKind::PptxNotes => {
                self.stats.xml_parses = self.stats.xml_parses.saturating_add(1);
            }
            _ => {}
        }
        let depth = node.limits.max_depth;
        let t_mat = crate::field::prof::start();
        let out = if self.use_cache {
            // The cache-first probe may have already read and integrity-checked
            // this exact node's output. Serving it here is byte-identical to a
            // cache hit and avoids reading the entry a second time.
            if let Some((id, bytes)) = self.prefetched_output.take() {
                if id == node.content_id() {
                    self.reuse.nodes_reused = self.reuse.nodes_reused.saturating_add(1);
                    self.budget.charge_bytes(bytes.len() as u64)?;
                    return Ok(bytes);
                }
                self.prefetched_output = Some((id, bytes));
            }
            dag::materialize_node_cached_with(
                self.source,
                &self.seeds,
                &mut *self.cache,
                node,
                self.limits,
                &mut self.budget,
                depth,
                &mut self.reuse,
            )
        } else {
            let mut cache = dag::NoCache;
            dag::materialize_node_cached_with(
                self.source,
                &self.seeds,
                &mut cache,
                node,
                self.limits,
                &mut self.budget,
                depth,
                &mut self.reuse,
            )
        };
        crate::field::prof::add_materialize(t_mat);
        out
    }

    fn load(&self, id: &NodeId) -> Result<SeedNode> {
        dag::load_node(&self.seeds, id)
    }

    fn lookup(&mut self, key: SelectorKey) -> Result<Vec<IndexEntry>> {
        if !self.manifest.has_index() {
            return Ok(Vec::new());
        }
        // A probe-resolved key is served from memory: the index nodes were read
        // (and charged) before the field opened, so reading them again would both
        // double the bytes and lie about the work. Counting the entries keeps
        // `index_nodes_read` identical to the normal path.
        let prefetched = self.prefetched.get(&key).cloned();
        let entries = match prefetched {
            Some(entries) => entries,
            None => {
                let root = NodeId::from_bytes(self.manifest.index_root);
                lookup(self.istore, &root, &key)?
            }
        };
        self.stats.index_nodes_read += entries.len() as u64;
        Ok(entries)
    }

    fn require_entry(&mut self, key: SelectorKey, what: &str) -> Result<IndexEntry> {
        let entries = self.lookup(key)?;
        entries.into_iter().next().ok_or_else(|| {
            Error::unsupported_feature(format!(
                "no {what} matching selector number {} in the observation index",
                key.number
            ))
        })
    }

    fn dispatch(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        // The common vocabulary dispatches through the detected format's adapter
        // (Phase 12.7). Native selectors fall through to the format-specific match.
        if req.selector.is_common() {
            return self.common_dispatch(req);
        }
        use Representation as R;
        match (&req.selector, req.representation) {
            (Selector::Document, R::FullDocument | R::ExactBytes) => self.document_full(req),
            (Selector::Document, R::Metadata) => self.document_metadata(req),
            (Selector::ByteRange { offset, len }, R::ExactBytes) => {
                self.byte_range(req, *offset, *len)
            }
            (Selector::Object(n), R::ExactBytes | R::EncodedBytes) => {
                self.indexed_exact(req, SelectorKey::new(SEL_OBJECT, *n), "object")
            }
            (Selector::Revision(n), R::ExactBytes) => {
                self.indexed_exact(req, SelectorKey::new(SEL_REVISION, *n), "revision")
            }
            (Selector::Revisions, R::Lineage) => self.pdf_revisions(req),
            (Selector::Revision(n), R::Lineage) => self.pdf_revision(req, *n),
            (Selector::Member(n), R::EncodedBytes) => self.indexed_exact(
                req,
                SelectorKey::new(SEL_PACKAGE_MEMBER_RAW, *n),
                "package member",
            ),
            (Selector::Member(n), R::DecodedBytes) => self.member_decoded(req, *n),
            (Selector::PackagePart(_), R::Metadata | R::ExactBytes | R::DecodedBytes) => {
                self.package_part_opc(req)
            }
            (Selector::Relationship(_), R::Metadata | R::ExactBytes | R::DecodedBytes) => {
                self.relationship_opc(req)
            }
            (Selector::Stream(n), R::EncodedBytes) => {
                self.indexed_exact(req, SelectorKey::new(SEL_STREAM, *n), "stream")
            }
            (Selector::Stream(n), R::DecodedBytes) => self.stream_decoded(req, *n),
            (Selector::Stream(n), R::Operators) => self.stream_operators(req, *n),
            (Selector::Page(n), R::Text) => self.page_text(req, *n),
            (Selector::Page(n), R::Preview) => self.page_preview(req, *n),
            (Selector::Page(n), R::Structure) => self.page_structure(req, *n),
            (Selector::TextMatch(p), R::Text) => self.text_match(req, p),
            #[cfg(feature = "docx")]
            (Selector::DocxStory { story, profile }, R::Text) => {
                self.docx_story_text(req, *story, profile)
            }
            #[cfg(feature = "docx")]
            (Selector::DocxStory { story, profile }, R::Structure) => {
                self.docx_story_structure(req, *story, profile)
            }
            #[cfg(feature = "docx")]
            (Selector::DocxStory { story, profile }, R::Metadata) => {
                self.docx_story_metadata(req, *story, profile)
            }
            #[cfg(feature = "docx")]
            (
                Selector::DocxParagraph {
                    story,
                    index,
                    profile,
                },
                R::Text | R::Metadata,
            ) => self.docx_paragraph(req, *story, *index, profile),
            #[cfg(feature = "docx")]
            (
                Selector::DocxTable {
                    story,
                    index,
                    profile,
                },
                R::Text | R::Metadata,
            ) => self.docx_table(req, *story, *index, profile),
            #[cfg(feature = "docx")]
            (
                Selector::DocxCell {
                    story,
                    table,
                    cell,
                    profile,
                },
                R::Text | R::Metadata,
            ) => self.docx_cell(req, *story, *table, cell, profile),
            #[cfg(feature = "docx")]
            (
                Selector::DocxFind {
                    story,
                    pattern,
                    profile,
                },
                R::Text,
            ) => self.docx_find(req, *story, pattern, profile),
            #[cfg(feature = "epub")]
            (Selector::EpubPackage, R::Metadata | R::Structure) => self.epub_package(req),
            #[cfg(feature = "epub")]
            (Selector::EpubManifestItem { index }, R::Metadata) => {
                self.epub_manifest_item_meta(req, *index)
            }
            #[cfg(feature = "epub")]
            (Selector::EpubManifestItem { index }, R::ExactBytes | R::DecodedBytes) => {
                self.epub_manifest_item_bytes(req, *index)
            }
            #[cfg(feature = "epub")]
            (Selector::EpubSpineItem { index, profile }, R::Metadata) => {
                self.epub_spine_item_meta(req, *index, profile)
            }
            #[cfg(feature = "epub")]
            (Selector::EpubSpineItem { index, profile }, R::Text) => {
                self.epub_spine_item_text(req, *index, profile)
            }
            #[cfg(feature = "epub")]
            (Selector::EpubSpineItem { index, profile }, R::Structure) => {
                self.epub_spine_item_structure(req, *index, profile)
            }
            #[cfg(feature = "epub")]
            (Selector::EpubSpineItem { index, profile }, R::Preview) => {
                self.epub_spine_item_preview(req, *index, profile)
            }
            #[cfg(feature = "epub")]
            (Selector::EpubSpineItem { index, profile }, R::ExactBytes | R::DecodedBytes) => {
                self.epub_spine_item_bytes(req, *index, profile)
            }
            #[cfg(feature = "epub")]
            (Selector::EpubNav, R::Metadata | R::Structure) => self.epub_nav(req),
            #[cfg(feature = "epub")]
            (Selector::EpubNavNode { index }, R::Metadata) => self.epub_nav_node(req, *index),
            #[cfg(feature = "epub")]
            (Selector::EpubResource(name), R::Metadata) => self.epub_resource_meta(req, name),
            #[cfg(feature = "epub")]
            (Selector::EpubResource(name), R::ExactBytes | R::DecodedBytes) => {
                self.epub_resource_bytes(req, name)
            }
            #[cfg(feature = "epub")]
            (
                Selector::EpubBlock {
                    index,
                    block,
                    profile,
                },
                R::Text | R::Metadata | R::Structure,
            ) => self.epub_block(req, *index, *block, profile),
            #[cfg(feature = "epub")]
            (
                Selector::EpubCell {
                    index,
                    table,
                    row,
                    col,
                    profile,
                },
                R::Text | R::Metadata,
            ) => self.epub_cell(req, *index, *table, *row, *col, profile),
            #[cfg(feature = "epub")]
            (
                Selector::EpubLink {
                    index,
                    link,
                    profile,
                },
                R::Metadata,
            ) => self.epub_link(req, *index, *link, profile),
            #[cfg(feature = "epub")]
            (
                Selector::EpubFind {
                    index,
                    pattern,
                    profile,
                },
                R::Text,
            ) => self.epub_find(req, *index, pattern, profile),
            #[cfg(feature = "odt")]
            (Selector::OdtPart(_), R::Metadata | R::ExactBytes | R::DecodedBytes) => {
                self.odt_part(req)
            }
            #[cfg(feature = "odt")]
            (Selector::OdtParagraph { index, profile }, R::Text | R::Metadata) => {
                self.odt_paragraph(req, *index, profile)
            }
            #[cfg(feature = "odt")]
            (Selector::OdtHeading { index, profile }, R::Text | R::Metadata) => {
                self.odt_heading(req, *index, profile)
            }
            #[cfg(feature = "odt")]
            (Selector::OdtTable { index, profile }, R::Text | R::Metadata) => {
                self.odt_table(req, *index, profile)
            }
            #[cfg(feature = "odt")]
            (
                Selector::OdtCell {
                    table,
                    row,
                    col,
                    profile,
                },
                R::Text | R::Metadata,
            ) => self.odt_cell(req, *table, *row, *col, profile),
            #[cfg(feature = "odt")]
            (Selector::OdtList { index, profile }, R::Text | R::Metadata) => {
                self.odt_list(req, *index, profile)
            }
            #[cfg(feature = "odt")]
            (Selector::OdtFind { pattern, profile }, R::Text) => {
                self.odt_find(req, pattern, profile)
            }
            #[cfg(feature = "xlsx")]
            (Selector::XlsxSheet { index, profile }, R::Text | R::Structure | R::Metadata) => {
                self.xlsx_sheet(req, *index, profile)
            }
            #[cfg(feature = "xlsx")]
            (
                Selector::XlsxCell {
                    sheet,
                    cell,
                    profile,
                },
                R::Text | R::Structure | R::Metadata | R::ExactBytes,
            ) => self.xlsx_cell(req, *sheet, cell, profile),
            #[cfg(feature = "xlsx")]
            (Selector::XlsxFind { pattern, profile }, R::Text) => {
                self.xlsx_find(req, pattern, profile)
            }
            #[cfg(feature = "xlsx")]
            (Selector::XlsxStyles, R::Metadata | R::Structure) => self.xlsx_styles_answer(req),
            #[cfg(feature = "xlsx")]
            (Selector::XlsxDefinedNames, R::Metadata | R::Structure) => {
                self.xlsx_defined_names(req)
            }
            #[cfg(feature = "xlsx")]
            (Selector::XlsxExternalRels, R::Metadata | R::Structure) => {
                self.xlsx_external_rels(req)
            }
            #[cfg(feature = "xlsx")]
            (Selector::XlsxComments { sheet }, R::Metadata | R::Structure) => {
                self.xlsx_comments(req, *sheet)
            }
            #[cfg(feature = "xlsx")]
            (Selector::XlsxHyperlinks { sheet }, R::Metadata | R::Structure) => {
                self.xlsx_hyperlinks(req, *sheet)
            }
            #[cfg(feature = "xlsx")]
            (Selector::XlsxTables { sheet }, R::Metadata | R::Structure) => {
                self.xlsx_tables(req, *sheet)
            }
            #[cfg(feature = "xlsx")]
            (
                Selector::XlsxDrawing { sheet },
                R::Metadata | R::Structure | R::ExactBytes | R::DecodedBytes,
            ) => self.xlsx_drawing(req, *sheet),
            #[cfg(feature = "pptx")]
            (Selector::PptxSlide { index, profile }, R::Text | R::Structure | R::Metadata) => {
                self.pptx_slide(req, *index, profile)
            }
            #[cfg(feature = "pptx")]
            (
                Selector::PptxShape {
                    slide,
                    index,
                    profile,
                },
                R::Text | R::Structure | R::Metadata,
            ) => self.pptx_shape(req, *slide, *index, profile),
            #[cfg(feature = "pptx")]
            (Selector::PptxNotes { index, profile }, R::Text | R::Metadata) => {
                self.pptx_notes(req, *index, profile)
            }
            #[cfg(feature = "pptx")]
            (Selector::PptxLayouts, R::Metadata | R::Structure) => self.pptx_layouts(req),
            #[cfg(feature = "pptx")]
            (Selector::PptxMasters, R::Metadata | R::Structure) => self.pptx_masters(req),
            #[cfg(feature = "pptx")]
            (Selector::PptxTheme, R::Metadata | R::Structure) => self.pptx_theme(req),
            #[cfg(feature = "pptx")]
            (
                Selector::PptxMedia { ordinal },
                R::Metadata | R::Structure | R::ExactBytes | R::DecodedBytes,
            ) => self.pptx_media(req, *ordinal),
            #[cfg(feature = "pptx")]
            (Selector::PptxTables { slide, profile }, R::Text | R::Structure | R::Metadata) => {
                self.pptx_tables(req, *slide, profile)
            }
            #[cfg(feature = "pptx")]
            (Selector::PptxFind { pattern, profile }, R::Text) => {
                self.pptx_find(req, pattern, profile)
            }
            _ => Err(Error::unsupported_feature(format!(
                "unsupported observation: selector {} with representation {}",
                req.selector.canonical(),
                req.representation.name()
            ))),
        }
    }

    fn document_full(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        let bytes = self.source.serve_document(self.limits)?;
        Ok(FieldAnswer {
            value: AnswerValue::Bytes(bytes),
            basis: Basis::DirectlyObserved,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: Some((0, self.manifest.source_len)),
            provenance: String::new(),
            dependency_ids: vec![self.manifest.root_node],
            integrity_scope: IntegrityScope::WholeSource,
            exact: true,
        })
    }

    fn document_metadata(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        let json = format!(
            concat!(
                "{{",
                "\"source_len\":{},",
                "\"source_sha256\":\"{}\",",
                "\"object_count\":{},",
                "\"graph_ops\":{},",
                "\"node_count\":{}",
                "}}"
            ),
            self.manifest.source_len,
            crate::integrity::to_hex(&self.manifest.source_sha256),
            self.object_count,
            self.graph_ops,
            self.manifest.node_count,
        );
        Ok(FieldAnswer {
            value: AnswerValue::Json(json),
            basis: Basis::DeterministicallyDerived,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: None,
            provenance: String::new(),
            dependency_ids: Vec::new(),
            integrity_scope: IntegrityScope::None,
            exact: false,
        })
    }

    fn byte_range(&mut self, req: &ObserveRequest, offset: u64, len: u64) -> Result<FieldAnswer> {
        let end = offset
            .checked_add(len)
            .ok_or_else(|| Error::usage("byte-range end overflows"))?;
        // A `SourceSlice`'s output is `source[offset..offset+len]`, so its span
        // coordinates alone are not an identity: the field's exact-authority root
        // (source-scoped) is a dependency, so the same `(offset, len)` in two
        // different sources gets **distinct** ids and can never alias in the
        // shared derived cache.
        let node = SeedNode::new(
            NodeKind::SourceSlice,
            len,
            span_params(offset, len),
            vec![self.manifest.root_node],
            "field:observe;source-slice",
        );
        let bytes = self.materialize(&node)?;
        Ok(FieldAnswer {
            value: AnswerValue::Bytes(bytes),
            basis: Basis::DirectlyObserved,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: Some((offset, end)),
            provenance: String::new(),
            dependency_ids: Vec::new(),
            integrity_scope: IntegrityScope::Node,
            exact: true,
        })
    }

    fn indexed_exact(
        &mut self,
        req: &ObserveRequest,
        key: SelectorKey,
        what: &str,
    ) -> Result<FieldAnswer> {
        let entry = self.require_entry(key, what)?;
        let node = self.load(&entry.node_id)?;
        let bytes = self.materialize(&node)?;
        let end = entry.out_off.saturating_add(entry.out_len);
        Ok(FieldAnswer {
            value: AnswerValue::Bytes(bytes),
            basis: Basis::DirectlyObserved,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: Some((entry.out_off, end)),
            provenance: String::new(),
            dependency_ids: vec![entry.node_id],
            integrity_scope: IntegrityScope::Node,
            exact: true,
        })
    }

    /// A typed decline when the field is not a PDF: revision lineage is a
    /// **PDF-native** observation (Phase 17), and incremental updates have no
    /// analog in DOCX/EPUB/ODT. A PDF with no revision structure also declines,
    /// because the ingest registers no lineage entry in that case.
    fn require_pdf_revision_structure(&self) -> Result<()> {
        match self.document_format() {
            Some(DocumentFormat::Pdf) => Ok(()),
            Some(other) => Err(Error::unsupported_feature(format!(
                "revision lineage is a PDF-native observation; format {} has no revision structure",
                other.name()
            ))),
            None => Err(Error::unsupported_feature(
                "the field manifest does not record a document format; revision lineage is unavailable",
            )),
        }
    }

    /// The whole revision lineage of the PDF (Phase 17).
    ///
    /// Resolved through the index to the persisted `PdfRevisionLineage` node the
    /// ingest computed from the byte-authoritative physical scan, so the answer is
    /// `O(depth)` and never re-reads or re-parses the source.
    fn pdf_revisions(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        self.require_pdf_revision_structure()?;
        let entry = self.require_entry(SelectorKey::new(SEL_REVISIONS, 0), "revision lineage")?;
        let node = self.load(&entry.node_id)?;
        let bytes = self.materialize(&node)?;
        let json = String::from_utf8(bytes)
            .map_err(|_| Error::internal_invariant("persisted revision lineage is not UTF-8"))?;
        Ok(FieldAnswer {
            value: AnswerValue::Json(json),
            basis: Basis::DeterministicallyDerived,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: None,
            provenance: "pdf;revision-lineage".to_string(),
            dependency_ids: vec![entry.node_id],
            integrity_scope: IntegrityScope::None,
            exact: false,
        })
    }

    /// One revision's lineage entry, scoped to a revision index (Phase 17).
    fn pdf_revision(&mut self, req: &ObserveRequest, index: u32) -> Result<FieldAnswer> {
        self.require_pdf_revision_structure()?;
        let entry = self.require_entry(
            SelectorKey::new(SEL_REVISION_LINEAGE, index),
            "revision lineage",
        )?;
        let node = self.load(&entry.node_id)?;
        let bytes = self.materialize(&node)?;
        let json = String::from_utf8(bytes)
            .map_err(|_| Error::internal_invariant("persisted revision lineage is not UTF-8"))?;
        let end = entry.out_off.saturating_add(entry.out_len);
        Ok(FieldAnswer {
            value: AnswerValue::Json(json),
            basis: Basis::DeterministicallyDerived,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: Some((entry.out_off, end)),
            provenance: format!("pdf;revision-lineage;index={index}"),
            dependency_ids: vec![entry.node_id],
            integrity_scope: IntegrityScope::None,
            exact: false,
        })
    }

    fn stream_decoded(&mut self, req: &ObserveRequest, object: u32) -> Result<FieldAnswer> {
        let entry = self.require_entry(SelectorKey::new(SEL_STREAM, object), "stream")?;
        let node = self.decoded_node(object, &entry.node_id)?;
        let id = node.content_id();
        let bytes = self.materialize(&node)?;
        Ok(FieldAnswer {
            value: AnswerValue::Bytes(bytes),
            basis: Basis::DeterministicallyDerived,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: None,
            provenance: String::new(),
            dependency_ids: vec![id],
            integrity_scope: IntegrityScope::None,
            exact: false,
        })
    }

    /// A package member's decoded bytes (Phase 12.2).
    ///
    /// Resolved through the index to the `PackageMemberDecoded` node, which is a
    /// deterministic function of its raw node, so the observation never enumerates
    /// the seed store. The answer is `DeterministicallyDerived`, never exact: it is
    /// not a byte-identical observation of the source.
    fn member_decoded(&mut self, req: &ObserveRequest, ordinal: u32) -> Result<FieldAnswer> {
        let entry = self.require_entry(
            SelectorKey::new(SEL_PACKAGE_MEMBER_DECODED, ordinal),
            "decoded package member",
        )?;
        let node = self.load(&entry.node_id)?;
        self.stats.member_decodes = self.stats.member_decodes.saturating_add(1);
        let id = node.content_id();
        let raw_deps = node.deps.clone();
        let bytes = self.materialize(&node)?;
        let mut dependency_ids = vec![id];
        dependency_ids.extend(raw_deps);
        Ok(FieldAnswer {
            value: AnswerValue::Bytes(bytes),
            basis: Basis::DeterministicallyDerived,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: None,
            provenance: String::new(),
            dependency_ids,
            integrity_scope: IntegrityScope::None,
            exact: false,
        })
    }

    /// Materialize and decode the generic OPC model (derived, `Q_gen`).
    #[cfg(feature = "opc")]
    fn opc_model(&mut self) -> Result<crate::adapter::package::opc::OpcModel> {
        let entry = self.require_entry(SelectorKey::new(SEL_OPC_MODEL, 0), "OPC model")?;
        let node = self.load(&entry.node_id)?;
        let bytes = self.materialize(&node)?;
        crate::adapter::package::opc::OpcModel::decode(&bytes)
    }

    /// A generic OPC part observation (Phase 12.3): exact/decoded bytes resolve
    /// through the OPC part's physical member ordinal, so the exact leaf stays the
    /// 12.2 raw member span. Metadata is derived (`Q_gen`).
    #[cfg(feature = "opc")]
    fn package_part_opc(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        use Representation as R;
        let Selector::PackagePart(name) = &req.selector else {
            return Err(Error::internal_invariant(
                "package_part_opc needs PackagePart",
            ));
        };
        let name = name.clone();
        let model = self.opc_model()?;
        let part = model.part_by_name(&name).ok_or_else(|| {
            Error::invalid_package_structure(format!("no package part named {name:?}"))
        })?;
        let ordinal = part.ordinal;
        match req.representation {
            R::ExactBytes => self.indexed_exact(
                req,
                SelectorKey::new(SEL_PACKAGE_MEMBER_RAW, ordinal),
                "package part",
            ),
            R::DecodedBytes => self.member_decoded(req, ordinal),
            R::Metadata => {
                let rel_count = model
                    .part_rels
                    .iter()
                    .find(|(o, _)| *o == ordinal)
                    .map_or(0, |(_, r)| r.len());
                let ct = match &part.content_type {
                    Some(c) => format!("\"{}\"", json_escape(c)),
                    None => "null".to_string(),
                };
                let json = format!(
                    "{{\"name\":\"{}\",\"ordinal\":{},\"content_type\":{},\"relationships\":{}}}",
                    json_escape(&part.name),
                    ordinal,
                    ct,
                    rel_count
                );
                Ok(FieldAnswer {
                    value: AnswerValue::Json(json),
                    basis: Basis::DeterministicallyDerived,
                    selector: req.selector.canonical(),
                    representation: req.representation.name().to_string(),
                    source_span: None,
                    provenance: String::new(),
                    dependency_ids: Vec::new(),
                    integrity_scope: IntegrityScope::None,
                    exact: false,
                })
            }
            _ => Err(Error::unsupported_feature(format!(
                "unsupported observation: selector {} with representation {}",
                req.selector.canonical(),
                req.representation.name()
            ))),
        }
    }

    /// A generic OPC relationship observation (Phase 12.3). For an internal
    /// relationship, `ExactBytes`/`DecodedBytes` resolve to the target part's member
    /// bytes. An external relationship is an inert identifier: asking for its bytes
    /// is a typed decline, never a fetch.
    #[cfg(feature = "opc")]
    fn relationship_opc(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        use Representation as R;
        let Selector::Relationship(id) = &req.selector else {
            return Err(Error::internal_invariant(
                "relationship_opc needs Relationship",
            ));
        };
        let id = id.clone();
        let model = self.opc_model()?;
        let (rel, owner) = model.relationship_by_id(&id)?.ok_or_else(|| {
            Error::invalid_package_structure(format!("no package relationship with id {id:?}"))
        })?;
        match req.representation {
            R::Metadata => {
                let resolved = match &rel.resolved {
                    Some(r) => format!("\"{}\"", json_escape(r)),
                    None => "null".to_string(),
                };
                let owner_json = match owner {
                    Some(o) => o.to_string(),
                    None => "null".to_string(),
                };
                let json = format!(
                    concat!(
                        "{{\"id\":\"{}\",\"type\":\"{}\",\"target\":\"{}\",",
                        "\"target_mode\":\"{}\",\"resolved\":{},\"owner\":{}}}"
                    ),
                    json_escape(&rel.id),
                    json_escape(&rel.rel_type),
                    json_escape(&rel.target),
                    rel.mode.name(),
                    resolved,
                    owner_json
                );
                Ok(FieldAnswer {
                    value: AnswerValue::Json(json),
                    basis: Basis::DeterministicallyDerived,
                    selector: req.selector.canonical(),
                    representation: req.representation.name().to_string(),
                    source_span: None,
                    provenance: String::new(),
                    dependency_ids: Vec::new(),
                    integrity_scope: IntegrityScope::None,
                    exact: false,
                })
            }
            R::ExactBytes | R::DecodedBytes => {
                let resolved = rel.resolved.clone().ok_or_else(|| {
                    Error::invalid_package_structure(format!(
                        "relationship {id:?} is external: its target is an inert identifier, never fetched"
                    ))
                })?;
                let part = model.part_by_name(&resolved).ok_or_else(|| {
                    Error::invalid_package_structure(format!(
                        "relationship {id:?} target {resolved:?} is not a package part"
                    ))
                })?;
                let ordinal = part.ordinal;
                if req.representation == R::ExactBytes {
                    self.indexed_exact(
                        req,
                        SelectorKey::new(SEL_PACKAGE_MEMBER_RAW, ordinal),
                        "relationship target part",
                    )
                } else {
                    self.member_decoded(req, ordinal)
                }
            }
            _ => Err(Error::unsupported_feature(format!(
                "unsupported observation: selector {} with representation {}",
                req.selector.canonical(),
                req.representation.name()
            ))),
        }
    }

    /// Non-OPC builds keep the selector surface stable but fail closed.
    #[cfg(not(feature = "opc"))]
    fn package_part_opc(&mut self, _req: &ObserveRequest) -> Result<FieldAnswer> {
        Err(Error::unsupported_feature(
            "OPC support is not compiled in (feature `opc`)",
        ))
    }

    /// Non-OPC builds keep the selector surface stable but fail closed.
    #[cfg(not(feature = "opc"))]
    fn relationship_opc(&mut self, _req: &ObserveRequest) -> Result<FieldAnswer> {
        Err(Error::unsupported_feature(
            "OPC support is not compiled in (feature `opc`)",
        ))
    }

    fn stream_operators(&mut self, req: &ObserveRequest, object: u32) -> Result<FieldAnswer> {
        let entry = self.require_entry(SelectorKey::new(SEL_STREAM, object), "stream")?;
        let decoded = self.decoded_node(object, &entry.node_id)?;
        let decoded_id = decoded.content_id();
        let node = SeedNode::new(
            NodeKind::ContentOperators,
            0,
            Vec::new(),
            vec![decoded_id],
            "pdf:content-operators",
        );
        let id = node.content_id();
        let bytes = self.materialize(&node)?;
        Ok(FieldAnswer {
            value: AnswerValue::Bytes(bytes),
            basis: Basis::DeterministicallyDerived,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: None,
            provenance: String::new(),
            dependency_ids: vec![id, decoded_id],
            integrity_scope: IntegrityScope::None,
            exact: false,
        })
    }

    fn page_content_id(&mut self, what_number: u32) -> Result<NodeId> {
        Ok(self
            .require_entry(SelectorKey::new(SEL_PAGE, what_number), "page")?
            .node_id)
    }

    /// Ensure the page's derived chain exists, deepening once if needed.
    ///
    /// Idempotent by content id: the three derived nodes have deterministic ids,
    /// so this **computes** them and does an O(1) `contains_node` check per id —
    /// it never enumerates the seed store. If they are already present the
    /// promotion is skipped entirely and no new field id is needed, which is what
    /// makes a repeated observation of the same page cheap.
    fn ensure_page_derived(
        &mut self,
        page: u32,
        page_content: NodeId,
    ) -> Result<(SeedNode, SeedNode, SeedNode)> {
        let (ops, text, preview) = derived_nodes(page, page_content);
        let present = self.seeds.contains_node(&ops.content_id())?
            && self.seeds.contains_node(&text.content_id())?
            && self.seeds.contains_node(&preview.content_id())?;
        if !present {
            // Promote against the manifest we already hold, so the descriptor
            // blob is not re-read just to learn the current manifest (fix #2).
            let promoted = ingest::deepen_page_with_manifest(self.store, self.manifest, page)?;
            self.stats.deepened = true;
            self.current_id = promoted;
        }
        Ok((ops, text, preview))
    }

    fn page_text(&mut self, req: &ObserveRequest, page: u32) -> Result<FieldAnswer> {
        let pc = self.page_content_id(page)?;
        let (ops, text, _preview) = self.ensure_page_derived(page, pc)?;
        let text_id = text.content_id();
        let bytes = self.materialize(&text)?;
        let value = AnswerValue::Text(String::from_utf8_lossy(&bytes).into_owned());
        Ok(FieldAnswer {
            value,
            basis: Basis::Heuristic,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: None,
            provenance: String::new(),
            dependency_ids: vec![text_id, ops.content_id(), pc],
            integrity_scope: IntegrityScope::None,
            exact: false,
        })
    }

    fn page_preview(&mut self, req: &ObserveRequest, page: u32) -> Result<FieldAnswer> {
        let pc = self.page_content_id(page)?;
        let (_ops, _text, preview) = self.ensure_page_derived(page, pc)?;
        let preview_id = preview.content_id();
        let bytes = self.materialize(&preview)?;
        Ok(FieldAnswer {
            value: AnswerValue::Bytes(bytes),
            basis: Basis::Heuristic,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: None,
            provenance: String::new(),
            dependency_ids: vec![preview_id, pc],
            integrity_scope: IntegrityScope::None,
            exact: false,
        })
    }

    /// Selective late materialization: read only the page's content streams and
    /// preview node. Image/XObject bytes and the full document are never touched.
    fn page_structure(&mut self, req: &ObserveRequest, page: u32) -> Result<FieldAnswer> {
        let pc = self.page_content_id(page)?;
        let (_ops, _text, preview) = self.ensure_page_derived(page, pc)?;
        let preview_id = preview.content_id();
        let preview_bytes = self.materialize(&preview)?;
        let (text_bytes, draw_ops, path_ops) = preview_stats(&preview_bytes);

        // The page's content streams are the direct dependencies of its
        // `PageContent` node; their `u32` params are the content object numbers.
        let pc_node = self.load(&pc)?;
        let mut streams: Vec<u32> = Vec::new();
        for dep in &pc_node.deps {
            let dep_node = self.load(dep)?;
            // Only a stream node's params are a content object number. An edited
            // page's `PageContent` may depend on a raw `Literal` whose params *are*
            // the content bytes, which must never be read as an object number.
            if !matches!(
                dep_node.kind,
                NodeKind::PdfStreamDecoded | NodeKind::PdfStreamEncoded
            ) {
                continue;
            }
            if let Ok(object) = read_u32_params(&dep_node.params) {
                streams.push(object);
            }
        }
        let streams_json = streams
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");

        let json = format!(
            "{{\"page\":{page},\"text_bytes\":{text_bytes},\"draw_ops\":{draw_ops},\"path_ops\":{path_ops},\"content_streams\":[{streams_json}]}}"
        );
        Ok(FieldAnswer {
            value: AnswerValue::Json(json),
            basis: Basis::DeterministicallyDerived,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: None,
            provenance: String::new(),
            dependency_ids: vec![preview_id, pc],
            integrity_scope: IntegrityScope::None,
            exact: false,
        })
    }

    fn text_match(&mut self, req: &ObserveRequest, pattern: &str) -> Result<FieldAnswer> {
        let mut items: Vec<(u32, String)> = Vec::new();
        let mut estimated: u64 = 0;
        let mut page: u32 = 1;
        while page <= MAX_TEXTMATCH_PAGES {
            let entries = self.lookup(SelectorKey::new(SEL_PAGE, page))?;
            let Some(entry) = entries.into_iter().next() else {
                break;
            };
            let pc = entry.node_id;
            let (_ops, text, _preview) = self.ensure_page_derived(page, pc)?;
            let bytes = self.materialize(&text)?;
            let rendered = String::from_utf8_lossy(&bytes).into_owned();
            for line in rendered.split('\n') {
                if line.contains(pattern) {
                    estimated = estimated.saturating_add(line.len() as u64 + 32);
                    if estimated > req.budget.max_output_bytes {
                        return Err(Error::resource_limit(format!(
                            "text match exceeded the {}-byte budget",
                            req.budget.max_output_bytes
                        )));
                    }
                    items.push((page, line.to_string()));
                }
            }
            page += 1;
        }
        let body = items
            .iter()
            .map(|(p, line)| format!("{{\"page\":{p},\"line\":\"{}\"}}", json_escape(line)))
            .collect::<Vec<_>>()
            .join(",");
        Ok(FieldAnswer {
            value: AnswerValue::Json(format!("[{body}]")),
            basis: Basis::Heuristic,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: None,
            provenance: String::new(),
            dependency_ids: Vec::new(),
            integrity_scope: IntegrityScope::None,
            exact: false,
        })
    }

    // -- DOCX (Phase 12.4) --------------------------------------------------

    /// Materialize and decode the DOCX discovery model (derived, `Q_gen`).
    ///
    /// Memoised by the model node's content-addressed `NodeId` when caching is
    /// enabled; `Arc` so every story view shares one decode.
    #[cfg(feature = "docx")]
    fn docx_model(&mut self) -> Result<Arc<DocxModel>> {
        let entry = self.require_entry(SelectorKey::new(SEL_DOCX_MODEL, 0), "DOCX model")?;
        if self.use_cache
            && let Some(m) = self.models.get_docx(&entry.node_id)
        {
            return Ok(m);
        }
        let node = self.load(&entry.node_id)?;
        let bytes = self.materialize(&node)?;
        let m = Arc::new(DocxModel::decode(&bytes)?);
        if self.use_cache {
            self.models
                .put_docx(entry.node_id, Arc::clone(&m), bytes.len() as u64);
        }
        Ok(m)
    }

    /// Resolve one story to its parsed [`StoryModel`], parsing **only** that
    /// story's part (plus the shared styles part) and persisting the canonical
    /// result in the derived cache. A story is never silently mixed with another.
    #[cfg(feature = "docx")]
    fn docx_story_view(
        &mut self,
        story: DocxStory,
        profile: &DocxExtractProfile,
    ) -> Result<DocxStoryView> {
        if story.kind_index().is_none() {
            return Err(Error::unsupported_feature(format!(
                "DOCX story {} is declared but not part-backed; preserved exactly, not interpreted",
                story.name()
            )));
        }
        let model = self.docx_model()?;
        let part = model.story_part(story).cloned().ok_or_else(|| {
            Error::unsupported_feature(format!(
                "DOCX package has no part for story {}",
                story.name()
            ))
        })?;
        let dec = self.require_entry(
            SelectorKey::new(SEL_PACKAGE_MEMBER_DECODED, part.ordinal),
            "DOCX story part decoded bytes",
        )?;
        let mut deps = vec![dec.node_id];
        if let Some(styles) = &model.styles
            && let Ok(e) = self.require_entry(
                SelectorKey::new(SEL_PACKAGE_MEMBER_DECODED, styles.ordinal),
                "DOCX styles decoded bytes",
            )
        {
            deps.push(e.node_id);
        }
        // The observation requires these decoded members; a cache-served decode is
        // still a required decoded member, and `seed_nodes_reused` reports reuse.
        self.stats.member_decodes = self.stats.member_decodes.saturating_add(deps.len() as u64);
        let span = self
            .lookup(SelectorKey::new(SEL_PACKAGE_MEMBER_RAW, part.ordinal))?
            .into_iter()
            .next()
            .map(|e| (e.out_off, e.out_off.saturating_add(e.out_len)));
        let mut node = SeedNode::new(
            NodeKind::DocxStory,
            self.limits.max_output_bytes,
            story_params(story, &part.name, profile),
            deps.clone(),
            "docx:story",
        );
        node.limits.max_output_bytes = self.limits.max_output_bytes;
        let id = node.content_id();
        // Memoise the decoded story by the story node's content-addressed `NodeId`
        // (a pure function of the node's canonical bytes), so a repeat observation
        // in one session skips the materialize + `StoryModel::decode`.
        let cached = if self.use_cache {
            self.models.get_story(&id)
        } else {
            None
        };
        let sm = match cached {
            Some(m) => m,
            None => {
                let bytes = self.materialize(&node)?;
                let m = Arc::new(StoryModel::decode(&bytes)?);
                if self.use_cache {
                    self.models
                        .put_story(id, Arc::clone(&m), bytes.len() as u64);
                }
                m
            }
        };
        let mut ids = vec![id];
        ids.extend(deps);
        Ok(DocxStoryView {
            model: sm,
            part,
            deps: ids,
            span,
        })
    }

    #[cfg(feature = "docx")]
    fn docx_answer(
        &self,
        req: &ObserveRequest,
        value: AnswerValue,
        provenance: String,
        span: Option<(u64, u64)>,
        deps: Vec<NodeId>,
    ) -> FieldAnswer {
        FieldAnswer {
            value,
            basis: Basis::DeterministicallyDerived,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: span,
            provenance,
            dependency_ids: deps,
            integrity_scope: IntegrityScope::None,
            exact: false,
        }
    }

    #[cfg(feature = "docx")]
    fn docx_story_text(
        &mut self,
        req: &ObserveRequest,
        story: DocxStory,
        profile: &DocxExtractProfile,
    ) -> Result<FieldAnswer> {
        let v = self.docx_story_view(story, profile)?;
        let text = v.model.text();
        let provenance = format!(
            "docx;story={};part={};profile={}",
            story.name(),
            v.part.name,
            profile.fingerprint()
        );
        Ok(self.docx_answer(req, AnswerValue::Text(text), provenance, v.span, v.deps))
    }

    #[cfg(feature = "docx")]
    fn docx_story_metadata(
        &mut self,
        req: &ObserveRequest,
        story: DocxStory,
        profile: &DocxExtractProfile,
    ) -> Result<FieldAnswer> {
        let v = self.docx_story_view(story, profile)?;
        let json = format!(
            concat!(
                "{{\"story\":\"{}\",\"part\":\"{}\",\"ordinal\":{},\"root\":\"{}\",",
                "\"paragraphs\":{},\"tables\":{},\"hyperlinks\":{},\"bookmarks\":{},",
                "\"resources\":{},\"sections\":{},\"profile\":\"{}\"}}"
            ),
            json_escape(&story.name()),
            json_escape(&v.part.name),
            v.part.ordinal,
            json_escape(&v.model.root_local),
            v.model.paragraphs().count(),
            v.model.tables().count(),
            v.model.hyperlinks.len(),
            v.model.bookmarks.len(),
            v.model.resources.len(),
            v.model.section_count,
            profile.fingerprint(),
        );
        let provenance = format!("docx;story={};part={}", story.name(), v.part.name);
        Ok(self.docx_answer(req, AnswerValue::Json(json), provenance, v.span, v.deps))
    }

    #[cfg(feature = "docx")]
    fn docx_story_structure(
        &mut self,
        req: &ObserveRequest,
        story: DocxStory,
        profile: &DocxExtractProfile,
    ) -> Result<FieldAnswer> {
        let v = self.docx_story_view(story, profile)?;
        let paras = v
            .model
            .paragraphs()
            .map(|p| {
                format!(
                    "{{\"index\":{},\"heading\":{},\"style\":{},\"text_len\":{}}}",
                    p.index,
                    opt_u8_json(p.heading_level),
                    opt_str_json(p.style_id.as_deref()),
                    p.text.len()
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let tables = v
            .model
            .tables()
            .map(|t| {
                format!(
                    "{{\"index\":{},\"rows\":{},\"cols_row0\":{}}}",
                    t.index,
                    t.rows.len(),
                    t.rows.first().map_or(0, |r| r.cells.len())
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let json = format!(
            concat!(
                "{{\"story\":\"{}\",\"part\":\"{}\",\"blocks\":{},",
                "\"paragraphs\":[{}],\"tables\":[{}],\"profile\":\"{}\"}}"
            ),
            json_escape(&story.name()),
            json_escape(&v.part.name),
            v.model.blocks.len(),
            paras,
            tables,
            profile.fingerprint(),
        );
        Ok(self.docx_answer(
            req,
            AnswerValue::Json(json),
            format!("docx;story={};part={}", story.name(), v.part.name),
            v.span,
            v.deps,
        ))
    }

    #[cfg(feature = "docx")]
    fn docx_paragraph(
        &mut self,
        req: &ObserveRequest,
        story: DocxStory,
        index: u32,
        profile: &DocxExtractProfile,
    ) -> Result<FieldAnswer> {
        let v = self.docx_story_view(story, profile)?;
        let p = v
            .model
            .paragraphs()
            .find(|p| p.index == index)
            .ok_or_else(|| {
                Error::unsupported_feature(format!(
                    "DOCX story {} has no body paragraph {index}",
                    story.name()
                ))
            })?;
        let text = p.text.clone();
        let style = p.style_id.clone();
        let heading = p.heading_level;
        let run_count = p.runs.len();
        let provenance = format!(
            "docx;story={};part={};paragraph={};profile={}",
            story.name(),
            v.part.name,
            index,
            profile.fingerprint()
        );
        let value = match req.representation {
            Representation::Text => AnswerValue::Text(text),
            Representation::Metadata => AnswerValue::Json(format!(
                concat!(
                    "{{\"story\":\"{}\",\"part\":\"{}\",\"paragraph\":{},",
                    "\"style\":{},\"heading\":{},\"runs\":{},\"text_len\":{}}}"
                ),
                json_escape(&story.name()),
                json_escape(&v.part.name),
                index,
                opt_str_json(style.as_deref()),
                opt_u8_json(heading),
                run_count,
                text.len(),
            )),
            _ => {
                return Err(Error::unsupported_feature(format!(
                    "unsupported observation: selector {} with representation {}",
                    req.selector.canonical(),
                    req.representation.name()
                )));
            }
        };
        Ok(self.docx_answer(req, value, provenance, v.span, v.deps))
    }

    #[cfg(feature = "docx")]
    fn docx_table(
        &mut self,
        req: &ObserveRequest,
        story: DocxStory,
        index: u32,
        profile: &DocxExtractProfile,
    ) -> Result<FieldAnswer> {
        let v = self.docx_story_view(story, profile)?;
        let t = v.model.tables().find(|t| t.index == index).ok_or_else(|| {
            Error::unsupported_feature(format!("DOCX story {} has no table {index}", story.name()))
        })?;
        let text = t.text();
        let rows = t.rows.len();
        let cells: Vec<usize> = t.rows.iter().map(|r| r.cells.len()).collect();
        let provenance = format!(
            "docx;story={};part={};table={};profile={}",
            story.name(),
            v.part.name,
            index,
            profile.fingerprint()
        );
        let value = match req.representation {
            Representation::Text => AnswerValue::Text(text),
            Representation::Metadata => {
                let dims = cells
                    .iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                AnswerValue::Json(format!(
                    concat!(
                        "{{\"story\":\"{}\",\"part\":\"{}\",\"table\":{},",
                        "\"rows\":{},\"cells_per_row\":[{}],\"profile\":\"{}\"}}"
                    ),
                    json_escape(&story.name()),
                    json_escape(&v.part.name),
                    index,
                    rows,
                    dims,
                    profile.fingerprint(),
                ))
            }
            _ => {
                return Err(Error::unsupported_feature(format!(
                    "unsupported observation: selector {} with representation {}",
                    req.selector.canonical(),
                    req.representation.name()
                )));
            }
        };
        Ok(self.docx_answer(req, value, provenance, v.span, v.deps))
    }

    #[cfg(feature = "docx")]
    fn docx_cell(
        &mut self,
        req: &ObserveRequest,
        story: DocxStory,
        table: u32,
        cell: &str,
        profile: &DocxExtractProfile,
    ) -> Result<FieldAnswer> {
        let (col, row_idx) = parse_cell_ref(cell).ok_or_else(|| {
            Error::usage(format!("cell reference {cell:?} is not A1-style (e.g. B7)"))
        })?;
        let v = self.docx_story_view(story, profile)?;
        let t = v.model.tables().find(|t| t.index == table).ok_or_else(|| {
            Error::unsupported_feature(format!("DOCX story {} has no table {table}", story.name()))
        })?;
        let r = t.rows.get(row_idx as usize).ok_or_else(|| {
            Error::unsupported_feature(format!("DOCX table {table} has no row {}", row_idx + 1))
        })?;
        let found = r
            .cells
            .iter()
            .find(|c| col >= c.grid_col && col < c.grid_col.saturating_add(c.grid_span))
            .ok_or_else(|| {
                Error::unsupported_feature(format!(
                    "DOCX table {table} row {} has no cell {cell}",
                    row_idx + 1
                ))
            })?;
        let text = found.text.clone();
        let grid_col = found.grid_col;
        let grid_span = found.grid_span;
        let vmerge = found.vmerge_continue;
        let provenance = format!(
            "docx;story={};part={};table={};row={};cell={};profile={}",
            story.name(),
            v.part.name,
            table,
            row_idx + 1,
            cell,
            profile.fingerprint()
        );
        let value = match req.representation {
            Representation::Text => AnswerValue::Text(text),
            Representation::Metadata => AnswerValue::Json(format!(
                concat!(
                    "{{\"story\":\"{}\",\"part\":\"{}\",\"table\":{},",
                    "\"row\":{},\"cell\":\"{}\",\"grid_col\":{},\"grid_span\":{},",
                    "\"vmerge_continue\":{},\"text_len\":{},\"profile\":\"{}\"}}"
                ),
                json_escape(&story.name()),
                json_escape(&v.part.name),
                table,
                row_idx + 1,
                json_escape(cell),
                grid_col,
                grid_span,
                vmerge,
                text.len(),
                profile.fingerprint(),
            )),
            _ => {
                return Err(Error::unsupported_feature(format!(
                    "unsupported observation: selector {} with representation {}",
                    req.selector.canonical(),
                    req.representation.name()
                )));
            }
        };
        Ok(self.docx_answer(req, value, provenance, v.span, v.deps))
    }

    #[cfg(feature = "docx")]
    fn docx_find(
        &mut self,
        req: &ObserveRequest,
        story: DocxStory,
        pattern: &str,
        profile: &DocxExtractProfile,
    ) -> Result<FieldAnswer> {
        let v = self.docx_story_view(story, profile)?;
        let mut items: Vec<String> = Vec::new();
        let mut estimated: u64 = 0;
        for p in v.model.paragraphs() {
            if p.text.contains(pattern) {
                estimated = estimated.saturating_add(p.text.len() as u64 + 48);
                if estimated > req.budget.max_output_bytes {
                    return Err(Error::resource_limit(format!(
                        "DOCX find exceeded the {}-byte budget",
                        req.budget.max_output_bytes
                    )));
                }
                items.push(format!(
                    "{{\"paragraph\":{},\"text\":\"{}\"}}",
                    p.index,
                    json_escape(&p.text)
                ));
            }
        }
        let provenance = format!(
            "docx;story={};part={};profile={}",
            story.name(),
            v.part.name,
            profile.fingerprint()
        );
        Ok(self.docx_answer(
            req,
            AnswerValue::Json(format!("[{}]", items.join(","))),
            provenance,
            v.span,
            v.deps,
        ))
    }

    /// Resolve the `PdfStreamDecoded` node for `object`.
    ///
    /// A decoded node is a deterministic function of its encoded node, the
    /// materializer, and the decoded length, so it is registered under
    /// [`SEL_STREAM_DECODED`] at ingest and found here in `O(depth)` index reads.
    /// Only when that entry is absent (ingest declined the eager decode) do we
    /// fall back to [`Ctx::deepen_stream`], which recomputes just this one node.
    /// Either path never enumerates the seed store (fix #3).
    fn decoded_node(&mut self, object: u32, encoded_id: &NodeId) -> Result<SeedNode> {
        let entries = self.lookup(SelectorKey::new(SEL_STREAM_DECODED, object))?;
        if let Some(entry) = entries.into_iter().next() {
            return self.load(&entry.node_id);
        }
        self.deepen_stream(object, encoded_id)
    }

    /// Persist a decoded-stream node for `object` if it is recoverable, then
    /// return it. A stream with no exact decoded representation is unsupported.
    fn deepen_stream(&mut self, object: u32, encoded_id: &NodeId) -> Result<SeedNode> {
        let encoded_node = self.load(encoded_id)?;
        let encoded = self.materialize(&encoded_node)?;
        let cap = usize::try_from(self.limits.max_output_bytes).unwrap_or(usize::MAX);
        let decoded = super::inflate::inflate_bounded(&encoded, cap, super::inflate::Wrapper::Zlib)
            .map_err(|e| {
                Error::unsupported_feature(format!(
                    "stream {object} has no recovered decoded representation: {e}"
                ))
            })?;
        let node = SeedNode::new(
            NodeKind::PdfStreamDecoded,
            decoded.len() as u64,
            u32_params(object),
            vec![*encoded_id],
            "pdf:stream-decoded",
        );
        self.store.seeds_mut().put_node(&node.encode_canonical())?;
        self.stats.deepened = true;
        Ok(node)
    }
}

#[cfg(feature = "epub")]
fn epub_opt_str(v: Option<&str>) -> String {
    match v {
        Some(s) => format!("\"{}\"", json_escape(s)),
        None => "null".to_string(),
    }
}

#[cfg(feature = "epub")]
fn epub_opt_u32(v: Option<u32>) -> String {
    match v {
        Some(n) => n.to_string(),
        None => "null".to_string(),
    }
}

#[cfg(feature = "epub")]
fn epub_str_array(items: &[String]) -> String {
    format!(
        "[{}]",
        items
            .iter()
            .map(|s| format!("\"{}\"", json_escape(s)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

#[cfg(feature = "epub")]
fn epub_dir_of(name: &str) -> String {
    match name.rfind('/') {
        Some(i) => name[..=i].to_string(),
        None => String::new(),
    }
}

#[cfg(feature = "epub")]
fn epub_mimetype_json(m: &crate::adapter::epub::MimetypeFacts) -> String {
    format!(
        "{{\"present\":{},\"first\":{},\"stored\":{},\"no_extra\":{},\"exact_bytes\":{},\"conformant\":{}}}",
        m.present, m.first, m.stored, m.no_extra, m.exact_bytes, m.conformant
    )
}

#[cfg(feature = "epub")]
fn epub_rootfile_json(r: &crate::adapter::epub::RootFile) -> String {
    let ord = if r.ordinal == u32::MAX {
        None
    } else {
        Some(r.ordinal)
    };
    format!(
        "{{\"full_path\":\"{}\",\"member\":\"{}\",\"media_type\":\"{}\",\"ordinal\":{}}}",
        json_escape(&r.full_path),
        json_escape(&r.member),
        json_escape(&r.media_type),
        epub_opt_u32(ord)
    )
}

#[cfg(feature = "epub")]
fn epub_meta_json(e: &crate::adapter::epub::MetadataEntry) -> String {
    format!(
        "{{\"name\":\"{}\",\"property\":{},\"refines\":{},\"id\":{},\"scheme\":{},\"value\":\"{}\"}}",
        json_escape(&e.name),
        epub_opt_str(e.property.as_deref()),
        epub_opt_str(e.refines.as_deref()),
        epub_opt_str(e.id.as_deref()),
        epub_opt_str(e.scheme.as_deref()),
        json_escape(&e.value)
    )
}

#[cfg(feature = "epub")]
fn epub_manifest_json(it: &crate::adapter::epub::ManifestItem) -> String {
    format!(
        "{{\"id\":\"{}\",\"href\":\"{}\",\"media_type\":\"{}\",\"properties\":{},\"fallback\":{},\"resolved\":{},\"ordinal\":{},\"external\":{}}}",
        json_escape(&it.id),
        json_escape(&it.href),
        json_escape(&it.media_type),
        epub_str_array(&it.properties),
        epub_opt_str(it.fallback.as_deref()),
        epub_opt_str(it.resolved.as_deref()),
        epub_opt_u32(it.resolved_ordinal()),
        it.external
    )
}

#[cfg(feature = "epub")]
fn epub_spine_json(s: &crate::adapter::epub::SpineItemRef) -> String {
    let index = if s.item_index == u32::MAX {
        None
    } else {
        Some(s.item_index)
    };
    let ord = if s.ordinal == u32::MAX {
        None
    } else {
        Some(s.ordinal)
    };
    format!(
        "{{\"idref\":\"{}\",\"linear\":{},\"properties\":{},\"item_index\":{},\"ordinal\":{}}}",
        json_escape(&s.idref),
        s.linear,
        epub_str_array(&s.properties),
        epub_opt_u32(index),
        epub_opt_u32(ord)
    )
}

#[cfg(feature = "epub")]
fn epub_nav_json(index: u32, e: &crate::adapter::epub::NavEntry) -> String {
    format!(
        "{{\"index\":{},\"depth\":{},\"nav\":\"{}\",\"label\":\"{}\",\"href\":\"{}\",\"member\":{},\"fragment\":{},\"external\":{}}}",
        index,
        e.depth,
        json_escape(&e.nav_type),
        json_escape(&e.label),
        json_escape(&e.href),
        epub_opt_str(e.member.as_deref()),
        epub_opt_str(e.fragment.as_deref()),
        e.external
    )
}

#[cfg(feature = "epub")]
fn epub_block_json(index: u32, b: &crate::adapter::epub::Block) -> String {
    use crate::adapter::epub::Block;
    match b {
        Block::Heading {
            level,
            id,
            epub_type,
            text,
        } => format!(
            "{{\"index\":{index},\"kind\":\"heading\",\"level\":{level},\"id\":{},\"epub_type\":{},\"text\":\"{}\"}}",
            epub_opt_str(id.as_deref()),
            epub_opt_str(epub_type.as_deref()),
            json_escape(text)
        ),
        Block::Paragraph { text } => format!(
            "{{\"index\":{index},\"kind\":\"paragraph\",\"text\":\"{}\"}}",
            json_escape(text)
        ),
        Block::List { ordered, items } => format!(
            "{{\"index\":{index},\"kind\":\"list\",\"ordered\":{ordered},\"items\":{}}}",
            epub_str_array(items)
        ),
        Block::Table { rows } => format!(
            "{{\"index\":{index},\"kind\":\"table\",\"rows\":{},\"cols\":{}}}",
            rows.len(),
            rows.first().map_or(0, |r| r.cells.len())
        ),
    }
}

#[cfg(feature = "epub")]
fn epub_cell_json(table: u32, row: u32, col: u32, c: &crate::adapter::epub::Cell) -> String {
    format!(
        "{{\"table\":{table},\"row\":{row},\"col\":{col},\"header\":{},\"colspan\":{},\"rowspan\":{},\"text_len\":{}}}",
        c.header,
        c.colspan,
        c.rowspan,
        c.text.len()
    )
}

#[cfg(feature = "epub")]
fn epub_link_json(index: u32, l: &crate::adapter::epub::Link) -> String {
    format!(
        "{{\"index\":{index},\"href\":\"{}\",\"text\":\"{}\",\"fragment\":{},\"member\":{},\"external\":{},\"epub_type\":{}}}",
        json_escape(&l.href),
        json_escape(&l.text),
        epub_opt_str(l.fragment.as_deref()),
        epub_opt_str(l.member.as_deref()),
        l.external,
        epub_opt_str(l.epub_type.as_deref())
    )
}

#[cfg(feature = "epub")]
fn epub_resource_json(index: usize, r: &crate::adapter::epub::Resource) -> String {
    format!(
        "{{\"index\":{index},\"kind\":\"{}\",\"attr\":\"{}\",\"value\":\"{}\",\"member\":{},\"external\":{}}}",
        json_escape(&r.kind),
        json_escape(&r.attr),
        json_escape(&r.value),
        epub_opt_str(r.member.as_deref()),
        r.external
    )
}

#[cfg(feature = "epub")]
fn epub_section_json(index: usize, s: &crate::adapter::epub::Section) -> String {
    format!(
        "{{\"index\":{index},\"local\":\"{}\",\"epub_type\":{},\"depth\":{}}}",
        json_escape(&s.local),
        epub_opt_str(s.epub_type.as_deref()),
        s.depth
    )
}

#[cfg(feature = "epub")]
fn epub_content_structure_json(
    index: u32,
    model: &crate::adapter::epub::ContentModel,
    item: &ManifestItem,
    profile: &EpubExtractProfile,
) -> String {
    let blocks = model
        .blocks
        .iter()
        .enumerate()
        .map(|(i, b)| epub_block_json(i as u32, b))
        .collect::<Vec<_>>()
        .join(",");
    let headings = model
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| matches!(b, crate::adapter::epub::Block::Heading { .. }))
        .map(|(i, b)| epub_block_json(i as u32, b))
        .collect::<Vec<_>>()
        .join(",");
    let links = model
        .links
        .iter()
        .enumerate()
        .map(|(i, l)| epub_link_json(i as u32, l))
        .collect::<Vec<_>>()
        .join(",");
    let resources = model
        .resources
        .iter()
        .enumerate()
        .map(|(i, r)| epub_resource_json(i, r))
        .collect::<Vec<_>>()
        .join(",");
    let sections = model
        .sections
        .iter()
        .enumerate()
        .map(|(i, s)| epub_section_json(i, s))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        concat!(
            "{{\"spine\":{},\"part\":\"{}\",\"profile\":\"{}\",\"root\":\"{}\",",
            "\"body\":{},\"scripted\":{},\"xhtml_nodes\":{},",
            "\"blocks\":[{}],\"headings\":[{}],\"links\":[{}],",
            "\"resources\":[{}],\"fragments\":{},\"sections\":[{}]}}"
        ),
        index,
        json_escape(item.resolved.as_deref().unwrap_or("")),
        profile.fingerprint(),
        json_escape(&model.root_local),
        model.body_seen,
        model.scripted,
        model.xhtml_nodes,
        blocks,
        headings,
        links,
        resources,
        epub_str_array(&model.fragments),
        sections
    )
}

#[cfg(feature = "epub")]
type EpubContentView = (
    crate::adapter::epub::ContentModel,
    ManifestItem,
    Option<(u64, u64)>,
    Vec<NodeId>,
);

#[cfg(feature = "epub")]
impl<S: SeedStore> Ctx<'_, S> {
    fn epub_model(&mut self) -> Result<EpubModel> {
        let entry = self.require_entry(SelectorKey::new(SEL_EPUB_MODEL, 0), "EPUB model")?;
        let node = self.load(&entry.node_id)?;
        let bytes = self.materialize(&node)?;
        EpubModel::decode(&bytes)
    }

    fn epub_package_doc(&mut self) -> Result<PackageDoc> {
        let model = self.epub_model()?;
        model.package.ok_or_else(|| {
            Error::invalid_package_structure("EPUB container has no resolvable package document")
        })
    }

    fn epub_member_decoded_bytes(&mut self, ordinal: u32) -> Result<Vec<u8>> {
        let entry = self.require_entry(
            SelectorKey::new(SEL_PACKAGE_MEMBER_DECODED, ordinal),
            "EPUB resource decoded bytes",
        )?;
        let node = self.load(&entry.node_id)?;
        self.stats.member_decodes = self.stats.member_decodes.saturating_add(1);
        self.materialize(&node)
    }

    fn epub_member_span(&mut self, ordinal: Option<u32>) -> Option<(u64, u64)> {
        let o = ordinal?;
        self.lookup(SelectorKey::new(SEL_PACKAGE_MEMBER_RAW, o))
            .ok()?
            .into_iter()
            .next()
            .map(|e| (e.out_off, e.out_off.saturating_add(e.out_len)))
    }

    fn epub_answer(
        &self,
        req: &ObserveRequest,
        value: AnswerValue,
        provenance: String,
        span: Option<(u64, u64)>,
        deps: Vec<NodeId>,
    ) -> FieldAnswer {
        FieldAnswer {
            value,
            basis: Basis::DeterministicallyDerived,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: span,
            provenance,
            dependency_ids: deps,
            integrity_scope: IntegrityScope::None,
            exact: false,
        }
    }

    fn epub_package(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        let model = self.epub_model()?;
        let doc = model.package.as_ref().ok_or_else(|| {
            Error::invalid_package_structure("EPUB container has no resolvable package document")
        })?;
        let rootfiles = model
            .rootfiles
            .iter()
            .map(epub_rootfile_json)
            .collect::<Vec<_>>()
            .join(",");
        let mut s = String::new();
        s.push_str("{\"package\":\"");
        s.push_str(&json_escape(&doc.member));
        s.push_str("\",\"version\":");
        s.push_str(&epub_opt_str(doc.version.as_deref()));
        s.push_str(",\"unique_identifier\":");
        s.push_str(&epub_opt_str(doc.unique_identifier.as_deref()));
        s.push_str(",\"page_progression_direction\":");
        s.push_str(&epub_opt_str(doc.page_progression.as_deref()));
        s.push_str(",\"rendition_layout\":");
        s.push_str(&epub_opt_str(doc.layout.as_deref()));
        s.push_str(",\"cover_id\":");
        s.push_str(&epub_opt_str(doc.cover_id.as_deref()));
        s.push_str(",\"nav_item\":");
        s.push_str(&epub_opt_u32(doc.nav_item));
        s.push_str(",\"ncx_item\":");
        s.push_str(&epub_opt_u32(doc.ncx_item));
        s.push_str(&format!(
            ",\"manifest_items\":{},\"spine_items\":{},\"metadata_entries\":{}",
            doc.manifest.len(),
            doc.spine.len(),
            doc.metadata.len()
        ));
        s.push_str(",\"rootfiles\":[");
        s.push_str(&rootfiles);
        s.push_str("],\"mimetype\":");
        s.push_str(&epub_mimetype_json(&model.mimetype));
        if req.representation == Representation::Structure {
            let man = doc
                .manifest
                .iter()
                .map(epub_manifest_json)
                .collect::<Vec<_>>()
                .join(",");
            let sp = doc
                .spine
                .iter()
                .map(epub_spine_json)
                .collect::<Vec<_>>()
                .join(",");
            let md = doc
                .metadata
                .iter()
                .map(epub_meta_json)
                .collect::<Vec<_>>()
                .join(",");
            s.push_str(",\"manifest\":[");
            s.push_str(&man);
            s.push_str("],\"spine\":[");
            s.push_str(&sp);
            s.push_str("],\"metadata\":[");
            s.push_str(&md);
            s.push(']');
        }
        s.push_str(",\"issues\":");
        s.push_str(&epub_str_array(&doc.issues));
        s.push('}');
        let provenance = format!("epub;package={}", doc.member);
        Ok(self.epub_answer(req, AnswerValue::Json(s), provenance, None, Vec::new()))
    }

    fn epub_manifest_item_meta(&mut self, req: &ObserveRequest, index: u32) -> Result<FieldAnswer> {
        let doc = self.epub_package_doc()?;
        let item = doc
            .manifest
            .get(index as usize)
            .ok_or_else(|| Error::unsupported_feature(format!("no EPUB manifest item {index}")))?;
        let json = epub_manifest_json(item);
        let provenance = format!("epub;manifest={index};id={}", item.id);
        let span = self.epub_member_span(item.resolved_ordinal());
        Ok(self.epub_answer(req, AnswerValue::Json(json), provenance, span, Vec::new()))
    }

    fn epub_manifest_item_bytes(
        &mut self,
        req: &ObserveRequest,
        index: u32,
    ) -> Result<FieldAnswer> {
        let doc = self.epub_package_doc()?;
        let item =
            doc.manifest.get(index as usize).cloned().ok_or_else(|| {
                Error::unsupported_feature(format!("no EPUB manifest item {index}"))
            })?;
        self.epub_item_bytes(req, &item)
    }

    fn epub_item_bytes(
        &mut self,
        req: &ObserveRequest,
        item: &ManifestItem,
    ) -> Result<FieldAnswer> {
        if item.external {
            return Err(Error::invalid_package_structure(format!(
                "EPUB manifest item {:?} is an external target: inert, never fetched",
                item.id
            )));
        }
        let ordinal = item.resolved_ordinal().ok_or_else(|| {
            Error::invalid_package_structure(format!(
                "EPUB manifest item {:?} has no resolvable container member",
                item.id
            ))
        })?;
        if req.representation == Representation::ExactBytes {
            self.indexed_exact(
                req,
                SelectorKey::new(SEL_PACKAGE_MEMBER_RAW, ordinal),
                "EPUB resource",
            )
        } else {
            self.member_decoded(req, ordinal)
        }
    }

    fn epub_spine_item_meta(
        &mut self,
        req: &ObserveRequest,
        index: u32,
        profile: &EpubExtractProfile,
    ) -> Result<FieldAnswer> {
        let doc = self.epub_package_doc()?;
        let order = doc.reading_order(profile);
        let mi = *order.get(index as usize).ok_or_else(|| {
            Error::unsupported_feature(format!(
                "no EPUB spine item {index} under profile {}",
                profile.fingerprint()
            ))
        })?;
        if mi == u32::MAX {
            return Err(Error::invalid_package_structure(
                "EPUB spine item does not resolve to a manifest item",
            ));
        }
        let item = doc.manifest.get(mi as usize).ok_or_else(|| {
            Error::invalid_package_structure("EPUB spine item manifest index is out of range")
        })?;
        let spine_ref = doc.spine.iter().find(|s| s.item_index == mi);
        let json = match spine_ref {
            Some(s) => format!(
                "{{\"index\":{},\"profile\":\"{}\",\"idref\":\"{}\",\"linear\":{},\"properties\":{},\"item\":{}}}",
                index,
                profile.fingerprint(),
                json_escape(&s.idref),
                s.linear,
                epub_str_array(&s.properties),
                epub_manifest_json(item)
            ),
            None => format!(
                "{{\"index\":{},\"profile\":\"{}\",\"idref\":null,\"item\":{}}}",
                index,
                profile.fingerprint(),
                epub_manifest_json(item)
            ),
        };
        let provenance = format!(
            "epub;spine={index};profile={};id={};part={}",
            profile.fingerprint(),
            item.id,
            item.resolved.as_deref().unwrap_or("")
        );
        let span = self.epub_member_span(item.resolved_ordinal());
        Ok(self.epub_answer(req, AnswerValue::Json(json), provenance, span, Vec::new()))
    }

    fn epub_spine_item_bytes(
        &mut self,
        req: &ObserveRequest,
        index: u32,
        profile: &EpubExtractProfile,
    ) -> Result<FieldAnswer> {
        let doc = self.epub_package_doc()?;
        let order = doc.reading_order(profile);
        let mi = *order.get(index as usize).ok_or_else(|| {
            Error::unsupported_feature(format!(
                "no EPUB spine item {index} under profile {}",
                profile.fingerprint()
            ))
        })?;
        let item = doc.manifest.get(mi as usize).cloned().ok_or_else(|| {
            Error::invalid_package_structure("EPUB spine item manifest index is out of range")
        })?;
        self.epub_item_bytes(req, &item)
    }

    /// Resolve a spine item to its parsed content model, parsing **only** that
    /// item's XHTML member (plus the shared model/decoded member) and persisting
    /// the derived content node in the disposable cache so later queries reuse it.
    /// Nothing here parses any *other* spine item.
    fn epub_content_view(
        &mut self,
        index: u32,
        profile: &EpubExtractProfile,
    ) -> Result<EpubContentView> {
        let doc = self.epub_package_doc()?;
        let order = doc.reading_order(profile);
        let mi = *order.get(index as usize).ok_or_else(|| {
            Error::unsupported_feature(format!(
                "no EPUB spine item {index} under profile {}",
                profile.fingerprint()
            ))
        })?;
        let item = doc.manifest.get(mi as usize).cloned().ok_or_else(|| {
            Error::invalid_package_structure("EPUB spine item manifest index is out of range")
        })?;
        let ordinal = item.resolved_ordinal().ok_or_else(|| {
            Error::invalid_package_structure(format!(
                "EPUB spine item {:?} has no resolvable container member",
                item.id
            ))
        })?;
        let dec = self.require_entry(
            SelectorKey::new(SEL_PACKAGE_MEMBER_DECODED, ordinal),
            "EPUB spine content decoded bytes",
        )?;
        self.stats.member_decodes = self.stats.member_decodes.saturating_add(1);
        let base_dir = epub_dir_of(item.resolved.as_deref().unwrap_or(""));
        let mut node = SeedNode::new(
            NodeKind::EpubContent,
            self.limits.max_output_bytes,
            crate::adapter::epub::content_params(index, ordinal, &base_dir, profile),
            vec![dec.node_id],
            "epub:content",
        );
        node.limits.max_output_bytes = self.limits.max_output_bytes;
        let id = node.content_id();
        let bytes = self.materialize(&node)?;
        let model = crate::adapter::epub::ContentModel::decode(&bytes)?;
        let span = self.epub_member_span(Some(ordinal));
        Ok((model, item, span, vec![id, dec.node_id]))
    }

    fn epub_spine_item_text(
        &mut self,
        req: &ObserveRequest,
        index: u32,
        profile: &EpubExtractProfile,
    ) -> Result<FieldAnswer> {
        let (model, item, span, deps) = self.epub_content_view(index, profile)?;
        let provenance = format!(
            "epub;spine={index};part={};profile={};content",
            item.resolved.as_deref().unwrap_or(""),
            profile.fingerprint()
        );
        Ok(self.epub_answer(req, AnswerValue::Text(model.text()), provenance, span, deps))
    }

    fn epub_spine_item_structure(
        &mut self,
        req: &ObserveRequest,
        index: u32,
        profile: &EpubExtractProfile,
    ) -> Result<FieldAnswer> {
        let (model, item, span, deps) = self.epub_content_view(index, profile)?;
        let json = epub_content_structure_json(index, &model, &item, profile);
        let provenance = format!(
            "epub;spine={index};part={};profile={};structure",
            item.resolved.as_deref().unwrap_or(""),
            profile.fingerprint()
        );
        Ok(self.epub_answer(req, AnswerValue::Json(json), provenance, span, deps))
    }

    fn epub_spine_item_preview(
        &mut self,
        req: &ObserveRequest,
        index: u32,
        profile: &EpubExtractProfile,
    ) -> Result<FieldAnswer> {
        let (model, item, span, deps) = self.epub_content_view(index, profile)?;
        let text = model.preview_text(
            index,
            item.resolved.as_deref().unwrap_or(""),
            &profile.fingerprint(),
        );
        let provenance = format!(
            "epub;spine={index};part={};profile={};preview",
            item.resolved.as_deref().unwrap_or(""),
            profile.fingerprint()
        );
        Ok(self.epub_answer(req, AnswerValue::Text(text), provenance, span, deps))
    }

    fn epub_block(
        &mut self,
        req: &ObserveRequest,
        index: u32,
        block: u32,
        profile: &EpubExtractProfile,
    ) -> Result<FieldAnswer> {
        let (model, item, span, deps) = self.epub_content_view(index, profile)?;
        let b = model.blocks.get(block as usize).ok_or_else(|| {
            Error::unsupported_feature(format!("EPUB spine item {index} has no block {block}"))
        })?;
        let provenance = format!(
            "epub;spine={index};part={};block={block};profile={}",
            item.resolved.as_deref().unwrap_or(""),
            profile.fingerprint()
        );
        let value = match req.representation {
            Representation::Text => AnswerValue::Text(b.text()),
            Representation::Metadata | Representation::Structure => {
                AnswerValue::Json(epub_block_json(block, b))
            }
            _ => {
                return Err(Error::unsupported_feature(format!(
                    "unsupported observation: selector {} with representation {}",
                    req.selector.canonical(),
                    req.representation.name()
                )));
            }
        };
        Ok(self.epub_answer(req, value, provenance, span, deps))
    }

    fn epub_cell(
        &mut self,
        req: &ObserveRequest,
        index: u32,
        table: u32,
        row: u32,
        col: u32,
        profile: &EpubExtractProfile,
    ) -> Result<FieldAnswer> {
        let (model, item, span, deps) = self.epub_content_view(index, profile)?;
        let t = model.table(table).ok_or_else(|| {
            Error::unsupported_feature(format!("EPUB spine item {index} has no table {table}"))
        })?;
        let crate::adapter::epub::Block::Table { rows } = t else {
            return Err(Error::internal_invariant(
                "table selector resolved a non-table",
            ));
        };
        let r = rows.get(row as usize).ok_or_else(|| {
            Error::unsupported_feature(format!("EPUB table {table} has no row {row}"))
        })?;
        let c = r.cells.get(col as usize).ok_or_else(|| {
            Error::unsupported_feature(format!("EPUB table {table} row {row} has no cell {col}"))
        })?;
        let provenance = format!(
            "epub;spine={index};part={};table={table};row={row};col={col};profile={}",
            item.resolved.as_deref().unwrap_or(""),
            profile.fingerprint()
        );
        let value = match req.representation {
            Representation::Text => AnswerValue::Text(c.text.clone()),
            Representation::Metadata => AnswerValue::Json(epub_cell_json(table, row, col, c)),
            _ => {
                return Err(Error::unsupported_feature(format!(
                    "unsupported observation: selector {} with representation {}",
                    req.selector.canonical(),
                    req.representation.name()
                )));
            }
        };
        Ok(self.epub_answer(req, value, provenance, span, deps))
    }

    fn epub_link(
        &mut self,
        req: &ObserveRequest,
        index: u32,
        link: u32,
        profile: &EpubExtractProfile,
    ) -> Result<FieldAnswer> {
        let (model, item, span, deps) = self.epub_content_view(index, profile)?;
        let l = model.links.get(link as usize).ok_or_else(|| {
            Error::unsupported_feature(format!("EPUB spine item {index} has no link {link}"))
        })?;
        let provenance = format!(
            "epub;spine={index};part={};link={link};profile={}",
            item.resolved.as_deref().unwrap_or(""),
            profile.fingerprint()
        );
        Ok(self.epub_answer(
            req,
            AnswerValue::Json(epub_link_json(link, l)),
            provenance,
            span,
            deps,
        ))
    }

    fn epub_find(
        &mut self,
        req: &ObserveRequest,
        index: u32,
        pattern: &str,
        profile: &EpubExtractProfile,
    ) -> Result<FieldAnswer> {
        let (model, item, span, deps) = self.epub_content_view(index, profile)?;
        let mut items: Vec<String> = Vec::new();
        let mut estimated: u64 = 0;
        for (i, b) in model.blocks.iter().enumerate() {
            let t = b.text();
            if t.contains(pattern) {
                estimated = estimated.saturating_add(t.len() as u64 + 48);
                if estimated > req.budget.max_output_bytes {
                    return Err(Error::resource_limit(format!(
                        "EPUB find exceeded the {}-byte budget",
                        req.budget.max_output_bytes
                    )));
                }
                items.push(format!(
                    "{{\"block\":{i},\"kind\":\"{}\",\"text\":\"{}\"}}",
                    b.kind(),
                    json_escape(&t)
                ));
            }
        }
        let provenance = format!(
            "epub;spine={index};part={};profile={};find",
            item.resolved.as_deref().unwrap_or(""),
            profile.fingerprint()
        );
        Ok(self.epub_answer(
            req,
            AnswerValue::Json(format!("[{}]", items.join(","))),
            provenance,
            span,
            deps,
        ))
    }

    fn epub_resource_meta(&mut self, req: &ObserveRequest, name: &str) -> Result<FieldAnswer> {
        let doc = self.epub_package_doc()?;
        let item = doc
            .manifest
            .iter()
            .find(|m| m.resolved.as_deref() == Some(name))
            .ok_or_else(|| {
                Error::invalid_package_structure(format!("no EPUB resource named {name:?}"))
            })?;
        let json = epub_manifest_json(item);
        let span = self.epub_member_span(item.resolved_ordinal());
        Ok(self.epub_answer(
            req,
            AnswerValue::Json(json),
            format!("epub;resource={name}"),
            span,
            Vec::new(),
        ))
    }

    fn epub_resource_bytes(&mut self, req: &ObserveRequest, name: &str) -> Result<FieldAnswer> {
        let doc = self.epub_package_doc()?;
        let item = doc
            .manifest
            .iter()
            .find(|m| m.resolved.as_deref() == Some(name))
            .cloned()
            .ok_or_else(|| {
                Error::invalid_package_structure(format!("no EPUB resource named {name:?}"))
            })?;
        self.epub_item_bytes(req, &item)
    }

    fn epub_nav(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        let doc = self.epub_package_doc()?;
        let idx = doc.nav_item.ok_or_else(|| {
            Error::invalid_package_structure(
                "EPUB package has no navigation document (properties nav)",
            )
        })?;
        let item = doc.manifest.get(idx as usize).cloned().ok_or_else(|| {
            Error::invalid_package_structure("EPUB nav manifest index is out of range")
        })?;
        let ordinal = item.resolved_ordinal().ok_or_else(|| {
            Error::invalid_package_structure("EPUB navigation document has no resolvable member")
        })?;
        let bytes = self.epub_member_decoded_bytes(ordinal)?;
        let base = epub_dir_of(item.resolved.as_deref().unwrap_or(""));
        let entries = crate::adapter::epub::parse_nav_document(&bytes, &base, self.limits)?;
        let body = entries
            .iter()
            .enumerate()
            .map(|(i, e)| epub_nav_json(i as u32, e))
            .collect::<Vec<_>>()
            .join(",");
        let json = format!(
            "{{\"nav_item\":\"{}\",\"entries\":[{}]}}",
            json_escape(&item.id),
            body
        );
        let provenance = format!(
            "epub;nav={};part={}",
            item.id,
            item.resolved.as_deref().unwrap_or("")
        );
        let span = self.epub_member_span(Some(ordinal));
        Ok(self.epub_answer(req, AnswerValue::Json(json), provenance, span, Vec::new()))
    }

    fn epub_nav_node(&mut self, req: &ObserveRequest, index: u32) -> Result<FieldAnswer> {
        let doc = self.epub_package_doc()?;
        let idx = doc.nav_item.ok_or_else(|| {
            Error::invalid_package_structure(
                "EPUB package has no navigation document (properties nav)",
            )
        })?;
        let item = doc.manifest.get(idx as usize).cloned().ok_or_else(|| {
            Error::invalid_package_structure("EPUB nav manifest index is out of range")
        })?;
        let ordinal = item.resolved_ordinal().ok_or_else(|| {
            Error::invalid_package_structure("EPUB navigation document has no resolvable member")
        })?;
        let bytes = self.epub_member_decoded_bytes(ordinal)?;
        let base = epub_dir_of(item.resolved.as_deref().unwrap_or(""));
        let entries = crate::adapter::epub::parse_nav_document(&bytes, &base, self.limits)?;
        let e = entries
            .get(index as usize)
            .ok_or_else(|| Error::unsupported_feature(format!("no EPUB nav entry {index}")))?;
        let json = epub_nav_json(index, e);
        Ok(self.epub_answer(
            req,
            AnswerValue::Json(json),
            format!("epub;nav-node={index}"),
            None,
            Vec::new(),
        ))
    }
}

// ---------------------------------------------------------------------------
// ODT (OpenDocument Text) observations (Phase 13.3)
// ---------------------------------------------------------------------------

#[cfg(feature = "odt")]
type OdtContentView = (
    OdtContentModel,
    crate::adapter::odt::PartRef,
    Option<(u64, u64)>,
    Vec<NodeId>,
);

#[cfg(feature = "odt")]
impl<S: SeedStore> Ctx<'_, S> {
    fn odt_model(&mut self) -> Result<OdtModel> {
        let entry = self.require_entry(SelectorKey::new(SEL_ODT_MODEL, 0), "ODT model")?;
        let node = self.load(&entry.node_id)?;
        let bytes = self.materialize(&node)?;
        OdtModel::decode(&bytes)
    }

    fn odt_member_span(&mut self, ordinal: Option<u32>) -> Option<(u64, u64)> {
        let o = ordinal?;
        self.lookup(SelectorKey::new(SEL_PACKAGE_MEMBER_RAW, o))
            .ok()?
            .into_iter()
            .next()
            .map(|e| (e.out_off, e.out_off.saturating_add(e.out_len)))
    }

    fn odt_answer(
        &self,
        req: &ObserveRequest,
        value: AnswerValue,
        provenance: String,
        span: Option<(u64, u64)>,
        deps: Vec<NodeId>,
    ) -> FieldAnswer {
        FieldAnswer {
            value,
            basis: Basis::DeterministicallyDerived,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: span,
            provenance,
            dependency_ids: deps,
            integrity_scope: IntegrityScope::None,
            exact: false,
        }
    }

    /// Resolve the main content part to its parsed [`OdtContentModel`], parsing
    /// **only** that part and persisting the derived node in the disposable cache.
    fn odt_content_view(&mut self, profile: &OdtExtractProfile) -> Result<OdtContentView> {
        let model = self.odt_model()?;
        let part = model.content.clone().ok_or_else(|| {
            Error::invalid_package_structure(
                "ODF package has no resolvable OpenDocument content part",
            )
        })?;
        if part.ordinal == u32::MAX {
            return Err(Error::invalid_package_structure(
                "ODF content part does not resolve to a package member",
            ));
        }
        let dec = self.require_entry(
            SelectorKey::new(SEL_PACKAGE_MEMBER_DECODED, part.ordinal),
            "ODT content decoded bytes",
        )?;
        self.stats.member_decodes = self.stats.member_decodes.saturating_add(1);
        let mut node = SeedNode::new(
            NodeKind::OdtContent,
            self.limits.max_output_bytes,
            crate::adapter::odt::content_params(part.ordinal, &part.name, profile),
            vec![dec.node_id],
            "odt:content",
        );
        node.limits.max_output_bytes = self.limits.max_output_bytes;
        let id = node.content_id();
        let bytes = self.materialize(&node)?;
        let cm = OdtContentModel::decode(&bytes)?;
        let span = self.odt_member_span(Some(part.ordinal));
        Ok((cm, part, span, vec![id, dec.node_id]))
    }

    /// An ODT (ODF) package part by `manifest:full-path`.
    fn odt_part(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        let name = match &req.selector {
            Selector::OdtPart(n) => n.clone(),
            _ => return Err(Error::internal_invariant("odt_part: wrong selector")),
        };
        let model = self.odt_model()?;
        let entry = model
            .manifest
            .iter()
            .find(|e| e.full_path == name)
            .cloned()
            .ok_or_else(|| {
                Error::unsupported_feature(format!("ODF manifest declares no part {name:?}"))
            })?;
        if entry.ordinal == u32::MAX {
            return Err(Error::invalid_package_structure(format!(
                "ODF manifest part {name:?} does not resolve to a package member"
            )));
        }
        let provenance = format!(
            "odt;part={};ordinal={};media_type={}",
            entry.full_path, entry.ordinal, entry.media_type
        );
        match req.representation {
            Representation::Metadata => {
                let json = format!(
                    "{{\"part\":\"{}\",\"ordinal\":{},\"media_type\":\"{}\",\"version\":{}}}",
                    json_escape(&entry.full_path),
                    entry.ordinal,
                    json_escape(&entry.media_type),
                    opt_str_json(entry.version.as_deref()),
                );
                Ok(self.odt_answer(req, AnswerValue::Json(json), provenance, None, Vec::new()))
            }
            Representation::ExactBytes => self.indexed_exact(
                req,
                SelectorKey::new(SEL_PACKAGE_MEMBER_RAW, entry.ordinal),
                "ODT part",
            ),
            Representation::DecodedBytes => self.member_decoded(req, entry.ordinal),
            _ => Err(unsupported_common(req)),
        }
    }

    fn odt_paragraph(
        &mut self,
        req: &ObserveRequest,
        index: u32,
        profile: &OdtExtractProfile,
    ) -> Result<FieldAnswer> {
        let (m, part, span, deps) = self.odt_content_view(profile)?;
        let p = m
            .paragraphs()
            .find(|p| !p.is_heading() && p.index == index)
            .ok_or_else(|| {
                Error::unsupported_feature(format!("ODT content has no paragraph {index}"))
            })?;
        let text = p.text.clone();
        let style = p.style_id.clone();
        let runs = p.runs.len();
        let provenance = format!(
            "odt;part={};paragraph={index};profile={}",
            part.name,
            profile.fingerprint()
        );
        let value = match req.representation {
            Representation::Text => AnswerValue::Text(text),
            Representation::Metadata => AnswerValue::Json(format!(
                "{{\"part\":\"{}\",\"paragraph\":{index},\"style\":{},\"runs\":{},\"text_len\":{}}}",
                json_escape(&part.name),
                opt_str_json(style.as_deref()),
                runs,
                text.len()
            )),
            _ => return Err(unsupported_common(req)),
        };
        Ok(self.odt_answer(req, value, provenance, span, deps))
    }

    fn odt_heading(
        &mut self,
        req: &ObserveRequest,
        index: u32,
        profile: &OdtExtractProfile,
    ) -> Result<FieldAnswer> {
        let (m, part, span, deps) = self.odt_content_view(profile)?;
        let h = m.headings().find(|h| h.index == index).ok_or_else(|| {
            Error::unsupported_feature(format!("ODT content has no heading {index}"))
        })?;
        let text = h.text.clone();
        let level = h.heading_level;
        let style = h.style_id.clone();
        let provenance = format!(
            "odt;part={};heading={index};profile={}",
            part.name,
            profile.fingerprint()
        );
        let value = match req.representation {
            Representation::Text => AnswerValue::Text(text),
            Representation::Metadata => AnswerValue::Json(format!(
                "{{\"part\":\"{}\",\"heading\":{index},\"level\":{},\"style\":{},\"text_len\":{}}}",
                json_escape(&part.name),
                opt_u8_json(level),
                opt_str_json(style.as_deref()),
                text.len()
            )),
            _ => return Err(unsupported_common(req)),
        };
        Ok(self.odt_answer(req, value, provenance, span, deps))
    }

    fn odt_table(
        &mut self,
        req: &ObserveRequest,
        index: u32,
        profile: &OdtExtractProfile,
    ) -> Result<FieldAnswer> {
        let (m, part, span, deps) = self.odt_content_view(profile)?;
        let t = m.tables().find(|t| t.index == index).ok_or_else(|| {
            Error::unsupported_feature(format!("ODT content has no table {index}"))
        })?;
        let text = t.text();
        let rows = t.rows.len();
        let cells: Vec<usize> = t.rows.iter().map(|r| r.cells.len()).collect();
        let provenance = format!(
            "odt;part={};table={index};profile={}",
            part.name,
            profile.fingerprint()
        );
        let value = match req.representation {
            Representation::Text => AnswerValue::Text(text),
            Representation::Metadata => {
                let dims = cells
                    .iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                AnswerValue::Json(format!(
                    "{{\"part\":\"{}\",\"table\":{index},\"rows\":{rows},\"cells_per_row\":[{dims}],\"profile\":\"{}\"}}",
                    json_escape(&part.name),
                    profile.fingerprint()
                ))
            }
            _ => return Err(unsupported_common(req)),
        };
        Ok(self.odt_answer(req, value, provenance, span, deps))
    }

    fn odt_cell(
        &mut self,
        req: &ObserveRequest,
        table: u32,
        row: u32,
        col: u32,
        profile: &OdtExtractProfile,
    ) -> Result<FieldAnswer> {
        let (m, part, span, deps) = self.odt_content_view(profile)?;
        let t = m.tables().find(|t| t.index == table).ok_or_else(|| {
            Error::unsupported_feature(format!("ODT content has no table {table}"))
        })?;
        let r = t.rows.get(row as usize).ok_or_else(|| {
            Error::unsupported_feature(format!("ODT table {table} has no row {row}"))
        })?;
        let c = r.cells.iter().find(|c| c.grid_col == col).ok_or_else(|| {
            Error::unsupported_feature(format!("ODT table {table} row {row} has no cell {col}"))
        })?;
        let text = c.text.clone();
        let col_span = c.col_span;
        let row_span = c.row_span;
        let covered = c.covered;
        let provenance = format!(
            "odt;part={};table={table};row={row};cell={col};profile={}",
            part.name,
            profile.fingerprint()
        );
        let value = match req.representation {
            Representation::Text => AnswerValue::Text(text),
            Representation::Metadata => AnswerValue::Json(format!(
                concat!(
                    "{{\"part\":\"{}\",\"table\":{},\"row\":{},\"col\":{},",
                    "\"cols_spanned\":{},\"rows_spanned\":{},",
                    "\"covered\":{},\"text_len\":{},\"profile\":\"{}\"}}"
                ),
                json_escape(&part.name),
                table,
                row,
                col,
                col_span,
                row_span,
                covered,
                text.len(),
                profile.fingerprint()
            )),
            _ => return Err(unsupported_common(req)),
        };
        Ok(self.odt_answer(req, value, provenance, span, deps))
    }

    fn odt_list(
        &mut self,
        req: &ObserveRequest,
        index: u32,
        profile: &OdtExtractProfile,
    ) -> Result<FieldAnswer> {
        let (m, part, span, deps) = self.odt_content_view(profile)?;
        let l = m.lists().find(|l| l.index == index).ok_or_else(|| {
            Error::unsupported_feature(format!("ODT content has no list {index}"))
        })?;
        let text = l.text();
        let items = l.items.len();
        let provenance = format!(
            "odt;part={};list={index};profile={}",
            part.name,
            profile.fingerprint()
        );
        let value = match req.representation {
            Representation::Text => AnswerValue::Text(text),
            Representation::Metadata => AnswerValue::Json(format!(
                "{{\"part\":\"{}\",\"list\":{index},\"items\":{items},\"profile\":\"{}\"}}",
                json_escape(&part.name),
                profile.fingerprint()
            )),
            _ => return Err(unsupported_common(req)),
        };
        Ok(self.odt_answer(req, value, provenance, span, deps))
    }

    fn odt_find(
        &mut self,
        req: &ObserveRequest,
        pattern: &str,
        profile: &OdtExtractProfile,
    ) -> Result<FieldAnswer> {
        let (m, part, span, deps) = self.odt_content_view(profile)?;
        let mut items: Vec<String> = Vec::new();
        let mut estimated: u64 = 0;
        for (i, b) in m.blocks.iter().enumerate() {
            let t = b.text();
            if t.contains(pattern) {
                estimated = estimated.saturating_add(t.len() as u64 + 48);
                if estimated > req.budget.max_output_bytes {
                    return Err(Error::resource_limit(format!(
                        "ODT find exceeded the {}-byte budget",
                        req.budget.max_output_bytes
                    )));
                }
                items.push(format!(
                    "{{\"block\":{i},\"text\":\"{}\"}}",
                    json_escape(&t)
                ));
            }
        }
        let provenance = format!("odt;part={};profile={}", part.name, profile.fingerprint());
        Ok(self.odt_answer(
            req,
            AnswerValue::Json(format!("[{}]", items.join(","))),
            provenance,
            span,
            deps,
        ))
    }
}

// ---------------------------------------------------------------------------
// XLSX (Phase 21.1.1)
// ---------------------------------------------------------------------------

#[cfg(feature = "xlsx")]
impl<S: SeedStore> Ctx<'_, S> {
    /// Materialize and decode the XLSX discovery model (derived, `Q_gen`).
    fn xlsx_model(&mut self) -> Result<XlsxModel> {
        let entry = self.require_entry(SelectorKey::new(SEL_XLSX_MODEL, 0), "XLSX model")?;
        let node = self.load(&entry.node_id)?;
        let bytes = self.materialize(&node)?;
        XlsxModel::decode(&bytes)
    }

    /// Resolve the decoded workbook inventory (`xl/workbook.xml`).
    fn xlsx_workbook(&mut self) -> Result<XlsxWorkbookModel> {
        let model = self.xlsx_model()?;
        let part = model.workbook.clone();
        let dec = self.require_entry(
            SelectorKey::new(SEL_PACKAGE_MEMBER_DECODED, part.ordinal),
            "XLSX workbook decoded bytes",
        )?;
        self.stats.member_decodes = self.stats.member_decodes.saturating_add(1);
        let mut node = SeedNode::new(
            NodeKind::XlsxWorkbook,
            self.limits.max_output_bytes,
            crate::adapter::xlsx::workbook_params(part.ordinal, &part.name),
            vec![dec.node_id],
            "xlsx:workbook",
        );
        node.limits.max_output_bytes = self.limits.max_output_bytes;
        let bytes = self.materialize(&node)?;
        XlsxWorkbookModel::decode(&bytes)
    }

    /// Resolve the worksheet part and sheet name for a 0-based workbook-order
    /// index. Hidden sheets are still addressable by index.
    fn xlsx_sheet_part(
        &self,
        model: &XlsxModel,
        workbook: &XlsxWorkbookModel,
        index: u32,
    ) -> Result<(crate::adapter::xlsx::XlsxPartRef, String)> {
        let ws = workbook
            .sheets
            .get(index as usize)
            .ok_or_else(|| Error::unsupported_feature(format!("workbook has no sheet {index}")))?;
        if let Some(rid) = ws.rel_id.as_deref()
            && let Some(s) = model
                .sheets
                .iter()
                .find(|s| s.rel_id.as_deref() == Some(rid))
        {
            return Ok((s.part.clone(), ws.name.clone()));
        }
        let s = model
            .sheets
            .get(index as usize)
            .ok_or_else(|| Error::unsupported_feature(format!("workbook has no sheet {index}")))?;
        Ok((s.part.clone(), ws.name.clone()))
    }

    /// Parse one worksheet into its derived [`XlsxSheetModel`], persisting the
    /// canonical result in the disposable cache. Only that sheet's part (and the
    /// shared-strings part) is decoded; no other worksheet is read.
    #[allow(clippy::type_complexity)]
    fn xlsx_sheet_view(
        &mut self,
        index: u32,
        profile: &XlsxExtractProfile,
    ) -> Result<(
        XlsxSheetModel,
        crate::adapter::xlsx::XlsxPartRef,
        String,
        Option<(u64, u64)>,
        Vec<NodeId>,
    )> {
        let model = self.xlsx_model()?;
        let workbook = self.xlsx_workbook()?;
        let (part, sheet_name) = self.xlsx_sheet_part(&model, &workbook, index)?;
        let dec = self.require_entry(
            SelectorKey::new(SEL_PACKAGE_MEMBER_DECODED, part.ordinal),
            "XLSX worksheet decoded bytes",
        )?;
        self.stats.member_decodes = self.stats.member_decodes.saturating_add(1);
        let mut deps = vec![dec.node_id];
        let mut shared_ordinal = None;
        if let Some(shared) = &model.shared_strings
            && let Ok(e) = self.require_entry(
                SelectorKey::new(SEL_PACKAGE_MEMBER_DECODED, shared.ordinal),
                "XLSX shared-strings decoded bytes",
            )
        {
            deps.push(e.node_id);
            shared_ordinal = Some(shared.ordinal);
            self.stats.member_decodes = self.stats.member_decodes.saturating_add(1);
        }
        let span = self
            .lookup(SelectorKey::new(SEL_PACKAGE_MEMBER_RAW, part.ordinal))?
            .into_iter()
            .next()
            .map(|e| (e.out_off, e.out_off.saturating_add(e.out_len)));
        let mut node = SeedNode::new(
            NodeKind::XlsxSheet,
            self.limits.max_output_bytes,
            crate::adapter::xlsx::sheet_params(
                part.ordinal,
                &part.name,
                &sheet_name,
                profile,
                shared_ordinal,
            ),
            deps.clone(),
            "xlsx:sheet",
        );
        node.limits.max_output_bytes = self.limits.max_output_bytes;
        let bytes = self.materialize(&node)?;
        let sheet = XlsxSheetModel::decode(&bytes)?;
        let mut ids = vec![node.content_id()];
        ids.extend(deps);
        Ok((sheet, part, sheet_name, span, ids))
    }

    fn xlsx_answer(
        &self,
        req: &ObserveRequest,
        value: AnswerValue,
        provenance: String,
        span: Option<(u64, u64)>,
        deps: Vec<NodeId>,
    ) -> FieldAnswer {
        FieldAnswer {
            value,
            basis: Basis::DeterministicallyDerived,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: span,
            provenance,
            dependency_ids: deps,
            integrity_scope: IntegrityScope::None,
            exact: false,
        }
    }

    /// Workbook-order indices of the sheets a whole-workbook projection includes.
    fn xlsx_projected_indices(
        &self,
        workbook: &XlsxWorkbookModel,
        profile: &XlsxExtractProfile,
    ) -> Vec<u32> {
        workbook
            .sheets
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                profile.include_hidden
                    || matches!(s.state, crate::adapter::xlsx::SheetState::Visible)
            })
            .map(|(i, _)| i as u32)
            .collect()
    }

    /// The minimal styles table, when the package carries a decoded styles part.
    fn xlsx_styles(&mut self, model: &XlsxModel) -> Option<crate::adapter::xlsx::StylesTable> {
        let styles = model.styles.as_ref()?;
        let e = self
            .require_entry(
                SelectorKey::new(SEL_PACKAGE_MEMBER_DECODED, styles.ordinal),
                "XLSX styles decoded bytes",
            )
            .ok()?;
        let node = self.load(&e.node_id).ok()?;
        let bytes = self.materialize(&node).ok()?;
        crate::adapter::xlsx::parse_styles_table(&bytes, self.limits).ok()
    }

    fn xlsx_sheet(
        &mut self,
        req: &ObserveRequest,
        index: u32,
        profile: &XlsxExtractProfile,
    ) -> Result<FieldAnswer> {
        let (sheet, part, sheet_name, span, deps) = self.xlsx_sheet_view(index, profile)?;
        let workbook = self.xlsx_workbook()?;
        let state = workbook
            .sheets
            .get(index as usize)
            .map(|s| s.state)
            .unwrap_or(crate::adapter::xlsx::SheetState::Visible);
        let provenance = format!(
            "xlsx;sheet={sheet_name};index={index};part={};profile={}",
            part.name,
            profile.fingerprint()
        );
        let value = match req.representation {
            Representation::Text => {
                AnswerValue::Text(sheet.text(profile.values, self.limits.max_xlsx_cells)?)
            }
            Representation::Metadata => AnswerValue::Json(format!(
                concat!(
                    "{{\"sheet\":\"{}\",\"index\":{},\"state\":\"{}\",\"part\":\"{}\",",
                    "\"ordinal\":{},\"dimension\":{},\"merges\":{},\"merge_count\":{},",
                    "\"hyperlinks\":{},\"tables\":{},",
                    "\"drawing\":{},\"legacy_drawing\":{},\"defined_names\":{},\"rows\":{},\"cells\":{},",
                    "\"profile\":\"{}\"}}"
                ),
                json_escape(&sheet_name),
                index,
                state.name(),
                json_escape(&part.name),
                part.ordinal,
                opt_str_json(sheet.dimension.as_deref()),
                xlsx_str_array(&sheet.merges),
                sheet.merges.len(),
                sheet.hyperlinks.len(),
                sheet.table_parts.len(),
                sheet.drawing_rel_id.is_some(),
                sheet.legacy_drawing_rel_id.is_some(),
                workbook.defined_names.len(),
                sheet.rows.len(),
                sheet.cell_count(),
                profile.fingerprint()
            )),
            Representation::Structure => {
                let rows = self.xlsx_rows_json(&sheet, &provenance, req)?;
                AnswerValue::Json(format!(
                    "{{\"sheet\":\"{}\",\"index\":{},\"merges\":{},\"merge_count\":{},\"rows\":[{}]}}",
                    json_escape(&sheet_name),
                    index,
                    xlsx_str_array(&sheet.merges),
                    sheet.merges.len(),
                    rows
                ))
            }
            _ => return Err(unsupported_common(req)),
        };
        Ok(self.xlsx_answer(req, value, provenance, span, deps))
    }

    fn xlsx_rows_json(
        &mut self,
        sheet: &XlsxSheetModel,
        provenance: &str,
        req: &ObserveRequest,
    ) -> Result<String> {
        let mut out: Vec<String> = Vec::new();
        let mut estimated: u64 = 0;
        for row in &sheet.rows {
            let mut cells: Vec<String> = Vec::new();
            for c in &row.cells {
                estimated =
                    estimated.saturating_add(64 + c.value.as_deref().unwrap_or("").len() as u64);
                if estimated > req.budget.max_output_bytes {
                    return Err(Error::resource_limit(format!(
                        "XLSX sheet structure exceeded the {}-byte budget",
                        req.budget.max_output_bytes
                    )));
                }
                cells.push(format!(
                    "{{\"ref\":\"{}\",\"col\":{},\"row\":{},\"kind\":\"{}\",\"type\":{},\"value\":{},\"formula\":{},\"style\":{}}}",
                    json_escape(&c.reference),
                    c.col,
                    c.row,
                    c.kind(),
                    opt_str_json(c.type_tag.as_deref()),
                    opt_str_json(c.value.as_deref()),
                    opt_str_json(c.formula.as_deref()),
                    opt_u32_json(c.style),
                ));
            }
            out.push(format!(
                "{{\"index\":{},\"cells\":[{}]}}",
                row.index,
                cells.join(",")
            ));
        }
        let _ = provenance;
        Ok(out.join(","))
    }

    fn xlsx_cell(
        &mut self,
        req: &ObserveRequest,
        sheet_index: u32,
        cell: &str,
        profile: &XlsxExtractProfile,
    ) -> Result<FieldAnswer> {
        let (col, row) = crate::adapter::xlsx::a1_to_col_row(cell).ok_or_else(|| {
            Error::usage(format!("cell reference {cell:?} is not A1-style (e.g. B7)"))
        })?;
        let (sheet, part, sheet_name, span, deps) = self.xlsx_sheet_view(sheet_index, profile)?;
        let found = sheet.cell_at(row, col).cloned().ok_or_else(|| {
            Error::unsupported_feature(format!(
                "sheet {sheet_index} ({sheet_name}) has no cell {cell}"
            ))
        })?;
        let provenance = format!(
            "xlsx;sheet={sheet_name};index={sheet_index};cell={cell};part={};profile={}",
            part.name,
            profile.fingerprint()
        );
        let value = match req.representation {
            Representation::Text => {
                AnswerValue::Text(sheet.facet(&found, profile.values).unwrap_or_default())
            }
            Representation::ExactBytes => {
                // The exact decoded-part byte span of the `<c>` element. This is a
                // derived (decompressed) span, not a source span; the answer keeps
                // the worksheet member's raw span for traceability.
                let dec = self.require_entry(
                    SelectorKey::new(SEL_PACKAGE_MEMBER_DECODED, part.ordinal),
                    "XLSX worksheet decoded bytes",
                )?;
                let node = self.load(&dec.node_id)?;
                let bytes = self.materialize(&node)?;
                let start = found.span_start as usize;
                let end = start.saturating_add(found.span_len as usize);
                let slice = bytes.get(start..end).unwrap_or_default().to_vec();
                AnswerValue::Bytes(slice)
            }
            Representation::Metadata | Representation::Structure => {
                let model = self.xlsx_model()?;
                let styles = self.xlsx_styles(&model);
                let resolved = found
                    .style
                    .and_then(|s| styles.as_ref().and_then(|t| t.style_for(s)));
                let style_json = xlsx_cell_style_json(found.style, resolved.as_ref());
                // The displayed value is a *deterministic projection* of the cached
                // value under the cell's number format — never a formula evaluation.
                // It is reported only where it differs from the cached value, with a
                // basis label so the distinction is explicit.
                let (displayed, display_basis) = match (
                    found.value.as_deref(),
                    resolved.as_ref().and_then(|s| s.format_code.as_deref()),
                ) {
                    (Some(v), Some(code)) => {
                        match crate::adapter::xlsx::format_displayed(v, code) {
                            Some(d) if d != v => (Some(d), format!("numFmt:{code}")),
                            Some(_) => (None, "identical".to_string()),
                            None => (None, "unsupported-format".to_string()),
                        }
                    }
                    (Some(_), None) => (None, "no-format".to_string()),
                    (None, _) => (None, "no-value".to_string()),
                };
                let comment = self.xlsx_cell_comment(part.ordinal, &found.reference)?;
                AnswerValue::Json(format!(
                    concat!(
                        "{{\"sheet\":\"{}\",\"index\":{},\"part\":\"{}\",",
                        "\"cell\":\"{}\",\"col\":{},\"row\":{},\"kind\":\"{}\",\"type\":{},",
                        "\"value\":{},\"formula\":{},\"displayed\":{},\"display_basis\":\"{}\",",
                        "\"style\":{},\"comment\":{},\"profile\":\"{}\"}}"
                    ),
                    json_escape(&sheet_name),
                    sheet_index,
                    json_escape(&part.name),
                    json_escape(&found.reference),
                    found.col,
                    found.row,
                    found.kind(),
                    opt_str_json(found.type_tag.as_deref()),
                    opt_str_json(found.value.as_deref()),
                    opt_str_json(found.formula.as_deref()),
                    opt_str_json(displayed.as_deref()),
                    display_basis,
                    style_json,
                    comment,
                    profile.fingerprint()
                ))
            }
            _ => return Err(unsupported_common(req)),
        };
        Ok(self.xlsx_answer(req, value, provenance, span, deps))
    }

    /// The comment on one cell, as JSON (`null` when the sheet has no comment for
    /// it). Keyed by the cell reference; a distinct observation from the value.
    fn xlsx_cell_comment(&mut self, owner: u32, reference: &str) -> Result<String> {
        let opc = self.opc_model()?;
        let Some(rel) = xlsx_find_rel(&opc, owner, "comments") else {
            return Ok("null".to_string());
        };
        let Some(resolved) = rel.resolved.as_deref() else {
            return Ok("null".to_string());
        };
        let Some(p) = opc.part_by_name(resolved) else {
            return Ok("null".to_string());
        };
        let bytes = self.xlsx_member_bytes(p.ordinal)?;
        let comments = crate::adapter::xlsx::parse_comments(&bytes, self.limits)?;
        Ok(match comments.iter().find(|c| c.cell == reference) {
            Some(c) => format!(
                "{{\"author\":{},\"text\":\"{}\"}}",
                opt_str_json(c.author.as_deref()),
                json_escape(&c.text)
            ),
            None => "null".to_string(),
        })
    }

    fn xlsx_find(
        &mut self,
        req: &ObserveRequest,
        pattern: &str,
        profile: &XlsxExtractProfile,
    ) -> Result<FieldAnswer> {
        let workbook = self.xlsx_workbook()?;
        let indices = self.xlsx_projected_indices(&workbook, profile);
        let mut items: Vec<String> = Vec::new();
        let mut estimated: u64 = 0;
        for index in indices {
            let (sheet, _part, sheet_name, _span, _deps) = self.xlsx_sheet_view(index, profile)?;
            for row in &sheet.rows {
                for c in &row.cells {
                    let Some(text) = sheet.facet(c, profile.values) else {
                        continue;
                    };
                    if text.contains(pattern) {
                        estimated = estimated.saturating_add(text.len() as u64 + 64);
                        if estimated > req.budget.max_output_bytes {
                            return Err(Error::resource_limit(format!(
                                "XLSX find exceeded the {}-byte budget",
                                req.budget.max_output_bytes
                            )));
                        }
                        items.push(format!(
                            "{{\"sheet\":\"{}\",\"index\":{},\"cell\":\"{}\",\"text\":\"{}\"}}",
                            json_escape(&sheet_name),
                            index,
                            json_escape(&c.reference),
                            json_escape(&text)
                        ));
                    }
                }
            }
        }
        let provenance = format!("xlsx;find;profile={}", profile.fingerprint());
        Ok(self.xlsx_answer(
            req,
            AnswerValue::Json(format!("[{}]", items.join(","))),
            provenance,
            None,
            Vec::new(),
        ))
    }

    /// Resolve the worksheet part and sheet name for a 0-based index.
    fn xlsx_sheet_scope(
        &mut self,
        index: u32,
    ) -> Result<(
        XlsxModel,
        XlsxWorkbookModel,
        crate::adapter::xlsx::XlsxPartRef,
        String,
    )> {
        let model = self.xlsx_model()?;
        let workbook = self.xlsx_workbook()?;
        let (part, name) = self.xlsx_sheet_part(&model, &workbook, index)?;
        Ok((model, workbook, part, name))
    }

    /// Materialize the decoded bytes of a package member by ordinal.
    fn xlsx_member_bytes(&mut self, ordinal: u32) -> Result<Vec<u8>> {
        let e = self.require_entry(
            SelectorKey::new(SEL_PACKAGE_MEMBER_DECODED, ordinal),
            "XLSX related part decoded bytes",
        )?;
        let node = self.load(&e.node_id)?;
        self.materialize(&node)
    }

    /// The parsed style table as a metadata observation (Phase 21.1.2).
    fn xlsx_styles_answer(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        let model = self.xlsx_model()?;
        let table = self.xlsx_styles(&model);
        let json = match &table {
            Some(t) => xlsx_styles_json(t),
            None => "{\"present\":false}".to_string(),
        };
        Ok(self.xlsx_answer(
            req,
            AnswerValue::Json(json),
            "xlsx;styles".to_string(),
            None,
            Vec::new(),
        ))
    }

    /// The workbook's defined/named ranges (Phase 21.1.2).
    fn xlsx_defined_names(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        let workbook = self.xlsx_workbook()?;
        let names = workbook
            .defined_names
            .iter()
            .map(|d| {
                format!(
                    concat!(
                        "{{\"name\":\"{}\",\"localSheetId\":{},\"hidden\":{},",
                        "\"function\":{},\"refersTo\":\"{}\"}}"
                    ),
                    json_escape(&d.name),
                    opt_u32_json(d.local_sheet_id),
                    d.hidden,
                    d.function,
                    json_escape(&d.refers_to)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let json = format!(
            "{{\"count\":{},\"names\":[{}]}}",
            workbook.defined_names.len(),
            names
        );
        Ok(self.xlsx_answer(
            req,
            AnswerValue::Json(json),
            "xlsx;defined-names".to_string(),
            None,
            Vec::new(),
        ))
    }

    /// The package's external relationships (Phase 21.1.2). Typed metadata only;
    /// external targets are inert identifiers and are never dereferenced.
    fn xlsx_external_rels(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        let model = self.opc_model()?;
        let mut items: Vec<String> = Vec::new();
        for r in &model.package_rels {
            if r.is_external() {
                items.push(xlsx_rel_json(r, None));
            }
        }
        for (owner, rels) in &model.part_rels {
            for r in rels {
                if r.is_external() {
                    items.push(xlsx_rel_json(r, Some(*owner)));
                }
            }
        }
        let json = format!(
            "{{\"count\":{},\"relationships\":[{}]}}",
            items.len(),
            items.join(",")
        );
        Ok(self.xlsx_answer(
            req,
            AnswerValue::Json(json),
            "xlsx;external-rels".to_string(),
            None,
            Vec::new(),
        ))
    }

    /// The cell comments of one worksheet, keyed by cell (Phase 21.1.2).
    fn xlsx_comments(&mut self, req: &ObserveRequest, sheet_index: u32) -> Result<FieldAnswer> {
        let (_model, _wb, part, sheet_name) = self.xlsx_sheet_scope(sheet_index)?;
        let opc = self.opc_model()?;
        let mut comments_json = "null".to_string();
        let mut vml_json = "null".to_string();
        if let Some(rel) = xlsx_find_rel(&opc, part.ordinal, "comments")
            && let Some(resolved) = rel.resolved.as_deref()
            && let Some(p) = opc.part_by_name(resolved)
        {
            let bytes = self.xlsx_member_bytes(p.ordinal)?;
            let comments = crate::adapter::xlsx::parse_comments(&bytes, self.limits)?;
            comments_json = format!(
                "[{}]",
                comments
                    .iter()
                    .map(|c| format!(
                        "{{\"cell\":\"{}\",\"author\":{},\"text\":\"{}\"}}",
                        json_escape(&c.cell),
                        opt_str_json(c.author.as_deref()),
                        json_escape(&c.text)
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
        if let Some(rel) = xlsx_find_rel(&opc, part.ordinal, "vmlDrawing")
            && let Some(resolved) = rel.resolved.as_deref()
            && let Some(p) = opc.part_by_name(resolved)
        {
            let bytes = self.xlsx_member_bytes(p.ordinal)?;
            let notes = crate::adapter::xlsx::parse_vml_notes(&bytes, self.limits)?;
            vml_json = format!(
                "[{}]",
                notes
                    .iter()
                    .map(|n| format!(
                        "{{\"cell\":\"{}\",\"shapeId\":{}}}",
                        json_escape(&n.cell),
                        opt_str_json(n.shape_id.as_deref())
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
        let json = format!(
            concat!(
                "{{\"sheet\":\"{}\",\"index\":{},\"part\":\"{}\",",
                "\"comments\":{},\"vml_notes\":{}}}"
            ),
            json_escape(&sheet_name),
            sheet_index,
            json_escape(&part.name),
            comments_json,
            vml_json
        );
        let provenance = format!("xlsx;sheet={sheet_name};index={sheet_index};comments");
        Ok(self.xlsx_answer(req, AnswerValue::Json(json), provenance, None, Vec::new()))
    }

    /// The hyperlinks declared in one worksheet (Phase 21.1.2), resolved through
    /// the sheet's relationships. Internal (`location`) and external (`r:id`) links
    /// are kept distinct; external targets are never dereferenced.
    fn xlsx_hyperlinks(&mut self, req: &ObserveRequest, sheet_index: u32) -> Result<FieldAnswer> {
        let profile = XlsxExtractProfile::DEFAULT;
        let (sheet, part, sheet_name, span, deps) = self.xlsx_sheet_view(sheet_index, &profile)?;
        let opc = self.opc_model()?;
        let mut items: Vec<String> = Vec::new();
        for h in &sheet.hyperlinks {
            let (external, target, resolved) = match h.rel_id.as_deref() {
                Some(id) => match xlsx_part_rel(&opc, part.ordinal, id) {
                    Some(rel) => (
                        rel.is_external(),
                        Some(rel.target.clone()),
                        rel.resolved.clone(),
                    ),
                    None => (false, None, None),
                },
                None => (false, None, None),
            };
            items.push(format!(
                concat!(
                    "{{\"ref\":\"{}\",\"relId\":{},\"location\":{},\"display\":{},",
                    "\"tooltip\":{},\"external\":{},\"target\":{},\"resolved\":{}}}"
                ),
                json_escape(&h.reference),
                opt_str_json(h.rel_id.as_deref()),
                opt_str_json(h.location.as_deref()),
                opt_str_json(h.display.as_deref()),
                opt_str_json(h.tooltip.as_deref()),
                external,
                opt_str_json(target.as_deref()),
                opt_str_json(resolved.as_deref())
            ));
        }
        let json = format!(
            concat!(
                "{{\"sheet\":\"{}\",\"index\":{},\"part\":\"{}\",",
                "\"count\":{},\"hyperlinks\":[{}]}}"
            ),
            json_escape(&sheet_name),
            sheet_index,
            json_escape(&part.name),
            items.len(),
            items.join(",")
        );
        let provenance = format!("xlsx;sheet={sheet_name};index={sheet_index};hyperlinks");
        Ok(self.xlsx_answer(req, AnswerValue::Json(json), provenance, span, deps))
    }

    /// The tables referenced by one worksheet's `tableParts` (Phase 21.1.2).
    fn xlsx_tables(&mut self, req: &ObserveRequest, sheet_index: u32) -> Result<FieldAnswer> {
        let profile = XlsxExtractProfile::DEFAULT;
        let (sheet, part, sheet_name, span, deps) = self.xlsx_sheet_view(sheet_index, &profile)?;
        let opc = self.opc_model()?;
        let mut tables: Vec<String> = Vec::new();
        for rid in &sheet.table_parts {
            let rel = xlsx_part_rel(&opc, part.ordinal, rid).ok_or_else(|| {
                Error::invalid_package_structure(format!(
                    "worksheet tablePart {rid:?} has no relationship"
                ))
            })?;
            let resolved = rel.resolved.clone().ok_or_else(|| {
                Error::invalid_package_structure(format!(
                    "worksheet tablePart {rid:?} target is external"
                ))
            })?;
            let p = opc.part_by_name(&resolved).ok_or_else(|| {
                Error::invalid_package_structure(format!(
                    "worksheet tablePart {rid:?} targets {resolved:?}, not a part"
                ))
            })?;
            let bytes = self.xlsx_member_bytes(p.ordinal)?;
            let table = crate::adapter::xlsx::parse_table(&bytes, self.limits)?;
            let columns = table
                .columns
                .iter()
                .map(|c| {
                    format!(
                        "{{\"id\":{},\"name\":\"{}\"}}",
                        opt_u32_json(c.id),
                        json_escape(&c.name)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            tables.push(format!(
                concat!(
                    "{{\"relId\":\"{}\",\"part\":\"{}\",\"ordinal\":{},",
                    "\"name\":{},\"displayName\":{},\"ref\":{},\"columns\":[{}]}}"
                ),
                json_escape(rid),
                json_escape(&p.name),
                p.ordinal,
                opt_str_json(table.name.as_deref()),
                opt_str_json(table.display_name.as_deref()),
                opt_str_json(table.reference.as_deref()),
                columns
            ));
        }
        let json = format!(
            concat!(
                "{{\"sheet\":\"{}\",\"index\":{},\"part\":\"{}\",",
                "\"count\":{},\"tables\":[{}]}}"
            ),
            json_escape(&sheet_name),
            sheet_index,
            json_escape(&part.name),
            tables.len(),
            tables.join(",")
        );
        let provenance = format!("xlsx;sheet={sheet_name};index={sheet_index};tables");
        Ok(self.xlsx_answer(req, AnswerValue::Json(json), provenance, span, deps))
    }

    /// The drawing(s) referenced by one worksheet (Phase 21.1.2). Charts are never
    /// evaluated; `ExactBytes`/`DecodedBytes` resolve the drawing part itself.
    fn xlsx_drawing(&mut self, req: &ObserveRequest, sheet_index: u32) -> Result<FieldAnswer> {
        use Representation as R;
        let profile = XlsxExtractProfile::DEFAULT;
        let (sheet, part, sheet_name, span, deps) = self.xlsx_sheet_view(sheet_index, &profile)?;
        let provenance = format!("xlsx;sheet={sheet_name};index={sheet_index};drawing");
        match req.representation {
            R::ExactBytes | R::DecodedBytes => {
                let rid = sheet.drawing_rel_id.as_deref().ok_or_else(|| {
                    Error::unsupported_feature(format!(
                        "sheet {sheet_index} ({sheet_name}) has no drawing"
                    ))
                })?;
                let opc = self.opc_model()?;
                let rel = xlsx_part_rel(&opc, part.ordinal, rid).ok_or_else(|| {
                    Error::invalid_package_structure(format!(
                        "worksheet drawing {rid:?} has no relationship"
                    ))
                })?;
                let resolved = rel.resolved.clone().ok_or_else(|| {
                    Error::invalid_package_structure("worksheet drawing target is external")
                })?;
                let p = opc.part_by_name(&resolved).ok_or_else(|| {
                    Error::invalid_package_structure(format!(
                        "worksheet drawing targets {resolved:?}, not a part"
                    ))
                })?;
                if req.representation == R::ExactBytes {
                    self.indexed_exact(
                        req,
                        SelectorKey::new(SEL_PACKAGE_MEMBER_RAW, p.ordinal),
                        "drawing part",
                    )
                } else {
                    self.member_decoded(req, p.ordinal)
                }
            }
            R::Metadata | R::Structure => {
                let opc = self.opc_model()?;
                let mut drawing_json = "null".to_string();
                let mut charts: Vec<String> = Vec::new();
                let mut media: Vec<String> = Vec::new();
                if let Some(rid) = sheet.drawing_rel_id.as_deref()
                    && let Some(rel) = xlsx_part_rel(&opc, part.ordinal, rid)
                    && let Some(resolved) = rel.resolved.as_deref()
                    && let Some(p) = opc.part_by_name(resolved)
                {
                    let bytes = self.xlsx_member_bytes(p.ordinal)?;
                    let d = crate::adapter::xlsx::parse_drawing(&bytes, self.limits)?;
                    for cid in &d.chart_rel_ids {
                        if let Some(crel) = xlsx_part_rel(&opc, p.ordinal, cid) {
                            charts.push(xlsx_rel_part_json(crel, &opc));
                        }
                    }
                    for iid in &d.image_rel_ids {
                        if let Some(irel) = xlsx_part_rel(&opc, p.ordinal, iid) {
                            media.push(xlsx_rel_part_json(irel, &opc));
                        }
                    }
                    drawing_json = format!(
                        "{{\"part\":\"{}\",\"ordinal\":{},\"anchors\":{}}}",
                        json_escape(&p.name),
                        p.ordinal,
                        d.anchors
                    );
                }
                let legacy = match &sheet.legacy_drawing_rel_id {
                    Some(id) => format!("\"{}\"", json_escape(id)),
                    None => "null".to_string(),
                };
                let json = format!(
                    concat!(
                        "{{\"sheet\":\"{}\",\"index\":{},\"part\":\"{}\",",
                        "\"drawing\":{},\"charts\":[{}],\"media\":[{}],\"legacyDrawing\":{}}}"
                    ),
                    json_escape(&sheet_name),
                    sheet_index,
                    json_escape(&part.name),
                    drawing_json,
                    charts.join(","),
                    media.join(","),
                    legacy
                );
                Ok(self.xlsx_answer(req, AnswerValue::Json(json), provenance, span, deps))
            }
            _ => Err(unsupported_common(req)),
        }
    }
}

// ---------------------------------------------------------------------------
// PPTX (Phase 21.2.1)
// ---------------------------------------------------------------------------

#[cfg(feature = "pptx")]
impl<S: SeedStore> Ctx<'_, S> {
    /// Materialize and decode the PPTX discovery model (derived, `Q_gen`).
    fn pptx_model(&mut self) -> Result<PptxModel> {
        let entry = self.require_entry(SelectorKey::new(SEL_PPTX_MODEL, 0), "PPTX model")?;
        let node = self.load(&entry.node_id)?;
        let bytes = self.materialize(&node)?;
        PptxModel::decode(&bytes)
    }

    /// Materialize and decode the parsed presentation inventory.
    fn pptx_presentation(&mut self) -> Result<PptxPresentationModel> {
        let model = self.pptx_model()?;
        let part = model.presentation.clone();
        let dec = self.require_entry(
            SelectorKey::new(SEL_PACKAGE_MEMBER_DECODED, part.ordinal),
            "PPTX presentation decoded bytes",
        )?;
        self.stats.member_decodes = self.stats.member_decodes.saturating_add(1);
        let mut node = SeedNode::new(
            NodeKind::PptxPresentation,
            self.limits.max_output_bytes,
            crate::adapter::pptx::presentation_params(part.ordinal, &part.name),
            vec![dec.node_id],
            "pptx:presentation",
        );
        node.limits.max_output_bytes = self.limits.max_output_bytes;
        let bytes = self.materialize(&node)?;
        PptxPresentationModel::decode(&bytes)
    }

    /// Resolve the slide part for a 0-based presentation-order index.
    fn pptx_slide_part(
        &self,
        model: &PptxModel,
        pres: &PptxPresentationModel,
        index: u32,
    ) -> Result<crate::adapter::pptx::PptxPartRef> {
        let s = pres.slides.get(index as usize).ok_or_else(|| {
            Error::unsupported_feature(format!("presentation has no slide {index}"))
        })?;
        if let Some(rid) = s.rel_id.as_deref()
            && let Some(m) = model
                .slides
                .iter()
                .find(|m| m.rel_id.as_deref() == Some(rid))
        {
            return Ok(m.part.clone());
        }
        model
            .slides
            .get(index as usize)
            .map(|m| m.part.clone())
            .ok_or_else(|| Error::unsupported_feature(format!("presentation has no slide {index}")))
    }

    /// Parse one slide into its derived [`PptxSlideModel`], persisting the canonical
    /// result in the disposable cache. Only that slide's part is decoded.
    #[allow(clippy::type_complexity)]
    fn pptx_slide_view(
        &mut self,
        index: u32,
        profile: &PptxExtractProfile,
    ) -> Result<(
        PptxSlideModel,
        crate::adapter::pptx::PptxPartRef,
        Option<(u64, u64)>,
        Vec<NodeId>,
    )> {
        let model = self.pptx_model()?;
        let pres = self.pptx_presentation()?;
        let part = self.pptx_slide_part(&model, &pres, index)?;
        let dec = self.require_entry(
            SelectorKey::new(SEL_PACKAGE_MEMBER_DECODED, part.ordinal),
            "PPTX slide decoded bytes",
        )?;
        self.stats.member_decodes = self.stats.member_decodes.saturating_add(1);
        let span = self
            .lookup(SelectorKey::new(SEL_PACKAGE_MEMBER_RAW, part.ordinal))?
            .into_iter()
            .next()
            .map(|e| (e.out_off, e.out_off.saturating_add(e.out_len)));
        let mut node = SeedNode::new(
            NodeKind::PptxSlide,
            self.limits.max_output_bytes,
            crate::adapter::pptx::slide_params(part.ordinal, &part.name, profile),
            vec![dec.node_id],
            "pptx:slide",
        );
        node.limits.max_output_bytes = self.limits.max_output_bytes;
        let bytes = self.materialize(&node)?;
        let slide = PptxSlideModel::decode(&bytes)?;
        Ok((slide, part, span, vec![node.content_id(), dec.node_id]))
    }

    /// The presentation-order indices of the slides a whole-deck projection would
    /// visit (all of them; hidden slides are filtered after each is parsed).
    fn pptx_slide_count(&mut self) -> Result<u32> {
        let pres = self.pptx_presentation()?;
        Ok(pres.slides.len() as u32)
    }

    /// Resolve the notes-slide parts (content type `…presentationml.notesSlide+xml`).
    fn pptx_notes_parts(&mut self) -> Result<Vec<crate::adapter::pptx::PptxPartRef>> {
        let opc = self.opc_model()?;
        let mut parts: Vec<crate::adapter::pptx::PptxPartRef> = opc
            .parts
            .iter()
            .filter(|p| {
                p.content_type
                    .as_deref()
                    .is_some_and(|ct| ct.ends_with("presentationml.notesSlide+xml"))
            })
            .map(|p| crate::adapter::pptx::PptxPartRef {
                name: p.name.clone(),
                ordinal: p.ordinal,
                content_type: p.content_type.clone(),
            })
            .collect();
        parts.sort_by(|a, b| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        });
        Ok(parts)
    }

    /// Parse one notes slide into its derived [`PptxNotesModel`].
    #[allow(clippy::type_complexity)]
    fn pptx_notes_view(
        &mut self,
        index: u32,
        profile: &PptxExtractProfile,
    ) -> Result<(
        PptxNotesModel,
        crate::adapter::pptx::PptxPartRef,
        Option<(u64, u64)>,
        Vec<NodeId>,
    )> {
        let parts = self.pptx_notes_parts()?;
        if parts.len() as u64 > u64::from(self.limits.max_pptx_notes) {
            return Err(Error::resource_limit(
                "presentation has more notes slides than max_pptx_notes",
            ));
        }
        let part = parts.get(index as usize).cloned().ok_or_else(|| {
            Error::unsupported_feature(format!("presentation has no notes slide {index}"))
        })?;
        let dec = self.require_entry(
            SelectorKey::new(SEL_PACKAGE_MEMBER_DECODED, part.ordinal),
            "PPTX notes decoded bytes",
        )?;
        self.stats.member_decodes = self.stats.member_decodes.saturating_add(1);
        let span = self
            .lookup(SelectorKey::new(SEL_PACKAGE_MEMBER_RAW, part.ordinal))?
            .into_iter()
            .next()
            .map(|e| (e.out_off, e.out_off.saturating_add(e.out_len)));
        let mut node = SeedNode::new(
            NodeKind::PptxNotes,
            self.limits.max_output_bytes,
            crate::adapter::pptx::notes_params(part.ordinal, &part.name, profile),
            vec![dec.node_id],
            "pptx:notes",
        );
        node.limits.max_output_bytes = self.limits.max_output_bytes;
        let bytes = self.materialize(&node)?;
        let notes = PptxNotesModel::decode(&bytes)?;
        Ok((notes, part, span, vec![node.content_id(), dec.node_id]))
    }

    /// The notes text attached to a slide, resolved through the slide's
    /// relationships (`notesSlide`), when present.
    fn pptx_slide_notes_text(
        &mut self,
        owner: u32,
        profile: &PptxExtractProfile,
    ) -> Result<Option<String>> {
        let opc = self.opc_model()?;
        let Some(rel) = opc
            .part_rels
            .iter()
            .find(|(o, _)| *o == owner)
            .and_then(|(_, rels)| {
                rels.iter()
                    .find(|r| r.rel_type == "notesSlide" || r.rel_type.ends_with("/notesSlide"))
            })
        else {
            return Ok(None);
        };
        let Some(resolved) = rel.resolved.clone() else {
            return Ok(None);
        };
        let Some(p) = opc.part_by_name(&resolved) else {
            return Ok(None);
        };
        let ordinal = p.ordinal;
        let dec = self.require_entry(
            SelectorKey::new(SEL_PACKAGE_MEMBER_DECODED, ordinal),
            "PPTX notes decoded bytes",
        )?;
        self.stats.member_decodes = self.stats.member_decodes.saturating_add(1);
        let mut node = SeedNode::new(
            NodeKind::PptxNotes,
            self.limits.max_output_bytes,
            crate::adapter::pptx::notes_params(ordinal, &resolved, profile),
            vec![dec.node_id],
            "pptx:notes",
        );
        node.limits.max_output_bytes = self.limits.max_output_bytes;
        let bytes = self.materialize(&node)?;
        let notes = PptxNotesModel::decode(&bytes)?;
        Ok(Some(notes.text))
    }

    fn pptx_answer(
        &self,
        req: &ObserveRequest,
        value: AnswerValue,
        provenance: String,
        span: Option<(u64, u64)>,
        deps: Vec<NodeId>,
    ) -> FieldAnswer {
        FieldAnswer {
            value,
            basis: Basis::DeterministicallyDerived,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: span,
            provenance,
            dependency_ids: deps,
            integrity_scope: IntegrityScope::None,
            exact: false,
        }
    }

    fn pptx_slide(
        &mut self,
        req: &ObserveRequest,
        index: u32,
        profile: &PptxExtractProfile,
    ) -> Result<FieldAnswer> {
        let (slide, part, span, deps) = self.pptx_slide_view(index, profile)?;
        let provenance = format!(
            "pptx;slide={index};part={};hidden={};profile={}",
            part.name,
            slide.hidden,
            profile.fingerprint()
        );
        let value = match req.representation {
            Representation::Text => AnswerValue::Text(slide.text()),
            Representation::Metadata => AnswerValue::Json(format!(
                concat!(
                    "{{\"index\":{},\"part\":\"{}\",\"ordinal\":{},\"hidden\":{},",
                    "\"shapes\":{},\"top_level\":{},\"tables\":{},\"text_len\":{},\"profile\":\"{}\"}}"
                ),
                index,
                json_escape(&part.name),
                part.ordinal,
                slide.hidden,
                slide.shape_count(),
                slide.top_level_count(),
                slide.tables.len(),
                slide.text().len(),
                profile.fingerprint()
            )),
            Representation::Structure => {
                let shapes = slide
                    .shapes
                    .iter()
                    .map(pptx_shape_json)
                    .collect::<Vec<_>>()
                    .join(",");
                AnswerValue::Json(format!(
                    "{{\"index\":{index},\"hidden\":{},\"shapes\":[{shapes}]}}",
                    slide.hidden
                ))
            }
            _ => return Err(unsupported_common(req)),
        };
        Ok(self.pptx_answer(req, value, provenance, span, deps))
    }

    fn pptx_shape(
        &mut self,
        req: &ObserveRequest,
        slide_index: u32,
        shape_index: u32,
        profile: &PptxExtractProfile,
    ) -> Result<FieldAnswer> {
        let (slide, part, span, deps) = self.pptx_slide_view(slide_index, profile)?;
        let shape = slide
            .shape_by_flat_index(shape_index)
            .cloned()
            .ok_or_else(|| {
                Error::unsupported_feature(format!(
                    "slide {slide_index} ({}) has no shape {shape_index}",
                    part.name
                ))
            })?;
        let provenance = format!(
            "pptx;slide={slide_index};shape={shape_index};part={};profile={}",
            part.name,
            profile.fingerprint()
        );
        let value = match req.representation {
            Representation::Text => AnswerValue::Text(shape.text_deep()),
            Representation::Metadata | Representation::Structure => {
                AnswerValue::Json(pptx_shape_json(&shape))
            }
            _ => return Err(unsupported_common(req)),
        };
        Ok(self.pptx_answer(req, value, provenance, span, deps))
    }

    fn pptx_notes(
        &mut self,
        req: &ObserveRequest,
        index: u32,
        profile: &PptxExtractProfile,
    ) -> Result<FieldAnswer> {
        let (notes, part, span, deps) = self.pptx_notes_view(index, profile)?;
        let provenance = format!(
            "pptx;notes={index};part={};profile={}",
            part.name,
            profile.fingerprint()
        );
        let value = match req.representation {
            Representation::Text => AnswerValue::Text(notes.text.clone()),
            Representation::Metadata => AnswerValue::Json(format!(
                "{{\"notes\":{index},\"part\":\"{}\",\"ordinal\":{},\"shapes\":{},\"text_len\":{},\"profile\":\"{}\"}}",
                json_escape(&part.name),
                part.ordinal,
                notes.shapes,
                notes.text.len(),
                profile.fingerprint()
            )),
            _ => return Err(unsupported_common(req)),
        };
        Ok(self.pptx_answer(req, value, provenance, span, deps))
    }

    fn pptx_layouts(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        let model = self.pptx_model()?;
        let json = pptx_parts_json("layouts", &model.layouts);
        Ok(self.pptx_answer(
            req,
            AnswerValue::Json(json),
            "pptx;layouts".to_string(),
            None,
            Vec::new(),
        ))
    }

    fn pptx_masters(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        let model = self.pptx_model()?;
        let json = pptx_parts_json("masters", &model.slide_masters);
        Ok(self.pptx_answer(
            req,
            AnswerValue::Json(json),
            "pptx;masters".to_string(),
            None,
            Vec::new(),
        ))
    }

    fn pptx_theme(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        let model = self.pptx_model()?;
        let json = pptx_parts_json("themes", &model.themes);
        Ok(self.pptx_answer(
            req,
            AnswerValue::Json(json),
            "pptx;theme".to_string(),
            None,
            Vec::new(),
        ))
    }

    fn pptx_media(&mut self, req: &ObserveRequest, ordinal: u32) -> Result<FieldAnswer> {
        use Representation as R;
        let model = self.pptx_model()?;
        let part = model.media.get(ordinal as usize).cloned().ok_or_else(|| {
            Error::unsupported_feature(format!("presentation has no media {ordinal}"))
        })?;
        match req.representation {
            R::ExactBytes => self.indexed_exact(
                req,
                SelectorKey::new(SEL_PACKAGE_MEMBER_RAW, part.ordinal),
                "media part",
            ),
            R::DecodedBytes => self.member_decoded(req, part.ordinal),
            R::Metadata | R::Structure => {
                let json = format!(
                    "{{\"media\":{ordinal},\"part\":\"{}\",\"ordinal\":{},\"contentType\":{}}}",
                    json_escape(&part.name),
                    part.ordinal,
                    opt_str_json(part.content_type.as_deref())
                );
                Ok(self.pptx_answer(
                    req,
                    AnswerValue::Json(json),
                    format!("pptx;media={ordinal}"),
                    None,
                    Vec::new(),
                ))
            }
            _ => Err(unsupported_common(req)),
        }
    }

    /// The `(slide_index, local_table_index)` of every embedded table across the
    /// projected slides, in slide order.
    fn pptx_table_refs(&mut self, profile: &PptxExtractProfile) -> Result<Vec<(u32, u32)>> {
        let count = self.pptx_slide_count()?;
        let mut out: Vec<(u32, u32)> = Vec::new();
        for i in 0..count {
            let (slide, _part, _span, _deps) = self.pptx_slide_view(i, profile)?;
            if slide.hidden && !profile.include_hidden {
                continue;
            }
            for local in 0..slide.tables.len() {
                out.push((i, local as u32));
            }
        }
        Ok(out)
    }

    fn pptx_tables(
        &mut self,
        req: &ObserveRequest,
        slide_index: u32,
        profile: &PptxExtractProfile,
    ) -> Result<FieldAnswer> {
        let (slide, part, span, deps) = self.pptx_slide_view(slide_index, profile)?;
        let provenance = format!(
            "pptx;slide={slide_index};part={};tables;profile={}",
            part.name,
            profile.fingerprint()
        );
        let value = match req.representation {
            Representation::Text => {
                let text = slide
                    .tables
                    .iter()
                    .map(|t| t.text())
                    .collect::<Vec<_>>()
                    .join("\n");
                AnswerValue::Text(text)
            }
            Representation::Metadata | Representation::Structure => {
                let tables = slide
                    .tables
                    .iter()
                    .map(pptx_table_json)
                    .collect::<Vec<_>>()
                    .join(",");
                AnswerValue::Json(format!(
                    "{{\"slide\":{slide_index},\"part\":\"{}\",\"count\":{},\"tables\":[{tables}]}}",
                    json_escape(&part.name),
                    slide.tables.len()
                ))
            }
            _ => return Err(unsupported_common(req)),
        };
        Ok(self.pptx_answer(req, value, provenance, span, deps))
    }

    fn pptx_find(
        &mut self,
        req: &ObserveRequest,
        pattern: &str,
        profile: &PptxExtractProfile,
    ) -> Result<FieldAnswer> {
        let count = self.pptx_slide_count()?;
        let mut items: Vec<String> = Vec::new();
        let mut estimated: u64 = 0;
        for i in 0..count {
            let (slide, _part, _span, _deps) = self.pptx_slide_view(i, profile)?;
            if slide.hidden && !profile.include_hidden {
                continue;
            }
            for shape in &slide.shapes {
                let mut matched: Vec<&PptxShape> = Vec::new();
                collect_matching_shapes(shape, pattern, &mut matched);
                for s in matched {
                    let t = s.text_deep();
                    estimated = estimated.saturating_add(t.len() as u64 + 64);
                    if estimated > req.budget.max_output_bytes {
                        return Err(Error::resource_limit(format!(
                            "PPTX find exceeded the {}-byte budget",
                            req.budget.max_output_bytes
                        )));
                    }
                    items.push(format!(
                        "{{\"slide\":{i},\"shape\":{},\"kind\":\"{}\",\"text\":\"{}\"}}",
                        s.index,
                        s.kind.name(),
                        json_escape(&t)
                    ));
                }
            }
        }
        let provenance = format!("pptx;find;profile={}", profile.fingerprint());
        Ok(self.pptx_answer(
            req,
            AnswerValue::Json(format!("[{}]", items.join(","))),
            provenance,
            None,
            Vec::new(),
        ))
    }
}

// ---------------------------------------------------------------------------
// PPTX common observations (Phase 21.2.1)
// ---------------------------------------------------------------------------

#[cfg(feature = "pptx")]
impl<S: SeedStore> Ctx<'_, S> {
    fn common_pptx(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        let profile = PptxExtractProfile::DEFAULT;
        match &req.selector {
            Selector::Metadata => self.pptx_common_metadata(req, &profile),
            Selector::Text => self.pptx_common_text(req, &profile),
            Selector::Table(i) => self.pptx_common_table(req, *i, &profile),
            Selector::Cell { table, row, col } => {
                self.pptx_common_cell(req, *table, *row, *col, &profile)
            }
            Selector::SearchMatch(p) => self.pptx_find(req, p, &profile),
            other => Err(Error::unsupported_feature(format!(
                "PPTX does not support common selector {}",
                other.canonical()
            ))),
        }
    }

    fn pptx_common_metadata(
        &mut self,
        req: &ObserveRequest,
        profile: &PptxExtractProfile,
    ) -> Result<FieldAnswer> {
        let model = self.pptx_model()?;
        let pres = self.pptx_presentation()?;
        let count = pres.slides.len() as u32;
        let title = if count > 0 {
            self.pptx_slide_view(0, profile)
                .ok()
                .and_then(|(s, _, _, _)| s.title())
        } else {
            None
        };
        let (cx, cy) = match pres.slide_size {
            Some((cx, cy)) => (Some(cx), Some(cy)),
            None => (None, None),
        };
        let json = format!(
            concat!(
                "{{\"format\":\"pptx\",\"presentation\":\"{}\",\"ordinal\":{},",
                "\"slides\":{},\"slide_size_cx\":{},\"slide_size_cy\":{},\"title\":{},",
                "\"masters\":{},\"layouts\":{},\"themes\":{},\"media\":{},\"profile\":\"{}\"}}"
            ),
            json_escape(&model.presentation.name),
            model.presentation.ordinal,
            count,
            opt_u64_json(cx),
            opt_u64_json(cy),
            opt_str_json(title.as_deref()),
            model.slide_masters.len(),
            model.layouts.len(),
            model.themes.len(),
            model.media.len(),
            profile.fingerprint()
        );
        let provenance = format!(
            "pptx;presentation={};profile={}",
            model.presentation.name,
            profile.fingerprint()
        );
        Ok(self.pptx_answer(req, AnswerValue::Json(json), provenance, None, Vec::new()))
    }

    fn pptx_common_text(
        &mut self,
        req: &ObserveRequest,
        profile: &PptxExtractProfile,
    ) -> Result<FieldAnswer> {
        let count = self.pptx_slide_count()?;
        let mut out = String::new();
        let mut slides: u64 = 0;
        for i in 0..count {
            let (slide, part, _span, _deps) = self.pptx_slide_view(i, profile)?;
            if slide.hidden && !profile.include_hidden {
                continue;
            }
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&slide.text());
            if profile.include_notes
                && let Some(notes) = self.pptx_slide_notes_text(part.ordinal, profile)?
                && !notes.is_empty()
            {
                out.push('\n');
                out.push_str(&notes);
            }
            if out.len() as u64 > req.budget.max_output_bytes {
                return Err(Error::resource_limit(format!(
                    "whole-deck text exceeded the {}-byte budget",
                    req.budget.max_output_bytes
                )));
            }
            slides += 1;
        }
        let provenance = format!("pptx;slides={slides};profile={}", profile.fingerprint());
        Ok(self.pptx_answer(req, AnswerValue::Text(out), provenance, None, Vec::new()))
    }

    fn pptx_common_table(
        &mut self,
        req: &ObserveRequest,
        ordinal: u32,
        profile: &PptxExtractProfile,
    ) -> Result<FieldAnswer> {
        let refs = self.pptx_table_refs(profile)?;
        let (slide_index, local) = *refs.get(ordinal as usize).ok_or_else(|| {
            Error::unsupported_feature(format!("deck has no projected table {ordinal}"))
        })?;
        let (slide, part, span, deps) = self.pptx_slide_view(slide_index, profile)?;
        let table = slide.tables.get(local as usize).cloned().ok_or_else(|| {
            Error::unsupported_feature(format!("slide {slide_index} has no table {local}"))
        })?;
        let provenance = format!(
            "pptx;slide={slide_index};table={local};ordinal={ordinal};part={};profile={}",
            part.name,
            profile.fingerprint()
        );
        let value = match req.representation {
            Representation::Text => AnswerValue::Text(table.text()),
            Representation::Metadata => AnswerValue::Json(pptx_table_json(&table)),
            _ => return Err(unsupported_common(req)),
        };
        Ok(self.pptx_answer(req, value, provenance, span, deps))
    }

    fn pptx_common_cell(
        &mut self,
        req: &ObserveRequest,
        table_ordinal: u32,
        row: u32,
        col: u32,
        profile: &PptxExtractProfile,
    ) -> Result<FieldAnswer> {
        let refs = self.pptx_table_refs(profile)?;
        let (slide_index, local) = *refs.get(table_ordinal as usize).ok_or_else(|| {
            Error::unsupported_feature(format!("deck has no projected table {table_ordinal}"))
        })?;
        let (slide, part, span, deps) = self.pptx_slide_view(slide_index, profile)?;
        let table = slide.tables.get(local as usize).ok_or_else(|| {
            Error::unsupported_feature(format!("slide {slide_index} has no table {local}"))
        })?;
        let r = table.rows.get(row as usize).ok_or_else(|| {
            Error::unsupported_feature(format!("table {table_ordinal} has no row {row}"))
        })?;
        let c = r.cells.get(col as usize).ok_or_else(|| {
            Error::unsupported_feature(format!("table {table_ordinal} row {row} has no cell {col}"))
        })?;
        let provenance = format!(
            "pptx;slide={slide_index};table={local};row={row};col={col};part={};profile={}",
            part.name,
            profile.fingerprint()
        );
        let value = match req.representation {
            Representation::Text => AnswerValue::Text(c.text.clone()),
            Representation::Metadata => AnswerValue::Json(format!(
                concat!(
                    "{{\"slide\":{},\"table\":{},\"row\":{},\"col\":{},",
                    "\"grid_span\":{},\"row_span\":{},\"text_len\":{}}}"
                ),
                slide_index,
                table_ordinal,
                row,
                col,
                c.grid_span,
                c.row_span,
                c.text.len()
            )),
            _ => return Err(unsupported_common(req)),
        };
        Ok(self.pptx_answer(req, value, provenance, span, deps))
    }
}

/// The JSON for one shape (recursive over group children).
#[cfg(feature = "pptx")]
fn pptx_shape_json(s: &PptxShape) -> String {
    let table = match &s.table {
        Some(t) => format!("{{\"rows\":{},\"cells\":{}}}", t.rows.len(), t.cell_count()),
        None => "null".to_string(),
    };
    let children = s
        .children
        .iter()
        .map(pptx_shape_json)
        .collect::<Vec<_>>()
        .join(",");
    format!(
        concat!(
            "{{\"index\":{},\"kind\":\"{}\",\"name\":{},\"shapeId\":{},",
            "\"placeholder\":{},\"text\":\"{}\",\"media\":{},\"chart\":{},",
            "\"table\":{},\"children\":[{}]}}"
        ),
        s.index,
        s.kind.name(),
        opt_str_json(s.name.as_deref()),
        opt_u32_json(s.shape_id),
        opt_str_json(s.placeholder.as_deref()),
        json_escape(&s.text),
        opt_str_json(s.media_rel_id.as_deref()),
        opt_str_json(s.chart_rel_id.as_deref()),
        table,
        children
    )
}

/// The JSON for one embedded table.
#[cfg(feature = "pptx")]
fn pptx_table_json(t: &PptxTable) -> String {
    let rows = t
        .rows
        .iter()
        .map(|r| {
            let cells = r
                .cells
                .iter()
                .map(|c| {
                    format!(
                        "{{\"text\":\"{}\",\"gridSpan\":{},\"rowSpan\":{},\"hMerge\":{},\"vMerge\":{}}}",
                        json_escape(&c.text),
                        c.grid_span,
                        c.row_span,
                        c.h_merge,
                        c.v_merge
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("{{\"cells\":[{cells}]}}")
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"rows\":{},\"cells\":{},\"detail\":[{rows}]}}",
        t.rows.len(),
        t.cell_count()
    )
}

/// The JSON for a list of parts (`layouts`/`masters`/`themes`).
#[cfg(feature = "pptx")]
fn pptx_parts_json(field: &str, parts: &[crate::adapter::pptx::PptxPartRef]) -> String {
    let items = parts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            format!(
                "{{\"index\":{},\"part\":\"{}\",\"ordinal\":{},\"contentType\":{}}}",
                i,
                json_escape(&p.name),
                p.ordinal,
                opt_str_json(p.content_type.as_deref())
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"count\":{},\"{field}\":[{items}]}}", parts.len())
}

#[cfg(feature = "pptx")]
fn opt_u64_json(v: Option<u64>) -> String {
    match v {
        Some(n) => n.to_string(),
        None => "null".to_string(),
    }
}

#[cfg(feature = "pptx")]
fn collect_matching_shapes<'a>(s: &'a PptxShape, pattern: &str, out: &mut Vec<&'a PptxShape>) {
    let own = s.own_text();
    if !own.is_empty() && own.contains(pattern) {
        out.push(s);
    }
    for c in &s.children {
        collect_matching_shapes(c, pattern, out);
    }
}

/// A JSON array of the (escaped) strings, used for the merged-range references.
#[cfg(feature = "xlsx")]
fn xlsx_str_array(items: &[String]) -> String {
    format!(
        "[{}]",
        items
            .iter()
            .map(|s| format!("\"{}\"", json_escape(s)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

/// A relationship's JSON metadata for the external-relationship observation.
#[cfg(feature = "xlsx")]
fn xlsx_rel_json(r: &crate::adapter::package::opc::Relationship, owner: Option<u32>) -> String {
    format!(
        concat!(
            "{{\"id\":\"{}\",\"type\":\"{}\",\"target\":\"{}\",",
            "\"target_mode\":\"{}\",\"owner\":{}}}"
        ),
        json_escape(&r.id),
        json_escape(&r.rel_type),
        json_escape(&r.target),
        r.mode.name(),
        match owner {
            Some(o) => o.to_string(),
            None => "null".to_string(),
        }
    )
}

/// The JSON for a relationship's resolved target part (a chart or image).
#[cfg(feature = "xlsx")]
fn xlsx_rel_part_json(
    r: &crate::adapter::package::opc::Relationship,
    model: &crate::adapter::package::opc::OpcModel,
) -> String {
    let (part, ordinal, ct) = match r.resolved.as_deref().and_then(|n| model.part_by_name(n)) {
        Some(p) => (
            format!("\"{}\"", json_escape(&p.name)),
            p.ordinal.to_string(),
            opt_str_json(p.content_type.as_deref()),
        ),
        None => ("null".to_string(), "null".to_string(), "null".to_string()),
    };
    format!(
        "{{\"relId\":\"{}\",\"part\":{},\"ordinal\":{},\"contentType\":{}}}",
        json_escape(&r.id),
        part,
        ordinal,
        ct
    )
}

/// The style table as deterministic JSON (Phase 21.1.2).
#[cfg(feature = "xlsx")]
fn xlsx_styles_json(t: &crate::adapter::xlsx::StylesTable) -> String {
    let fonts = t
        .fonts
        .iter()
        .enumerate()
        .map(|(i, f)| {
            format!(
                concat!(
                    "{{\"index\":{},\"bold\":{},\"italic\":{},",
                    "\"size\":{},\"name\":{}}}"
                ),
                i,
                f.bold,
                f.italic,
                opt_str_json(f.size.as_deref()),
                opt_str_json(f.name.as_deref())
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let fills = t
        .fills
        .iter()
        .enumerate()
        .map(|(i, f)| {
            format!(
                concat!(
                    "{{\"index\":{},\"patternType\":{},",
                    "\"fgColor\":{},\"bgColor\":{}}}"
                ),
                i,
                opt_str_json(f.pattern_type.as_deref()),
                opt_str_json(f.fg_color.as_deref()),
                opt_str_json(f.bg_color.as_deref())
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let fmts = t
        .num_fmts
        .iter()
        .map(|(id, code)| format!("{{\"id\":{id},\"formatCode\":\"{}\"}}", json_escape(code)))
        .collect::<Vec<_>>()
        .join(",");
    let xfs = t
        .cell_xfs
        .iter()
        .enumerate()
        .map(|(i, xf)| {
            format!(
                concat!(
                    "{{\"index\":{},\"numFmtId\":{},\"fontId\":{},\"fillId\":{},",
                    "\"alignment\":{}}}"
                ),
                i,
                xf.num_fmt_id,
                xf.font_id,
                xf.fill_id,
                xlsx_alignment_json(xf.alignment.as_ref())
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        concat!(
            "{{\"present\":true,\"fonts\":{},\"fills\":{},\"num_fmts\":{},\"cell_xfs\":{},",
            "\"fonts_detail\":[{}],\"fills_detail\":[{}],\"num_fmts_detail\":[{}],\"cell_xfs_detail\":[{}]}}"
        ),
        t.fonts.len(),
        t.fills.len(),
        t.num_fmts.len(),
        t.cell_xfs.len(),
        fonts,
        fills,
        fmts,
        xfs
    )
}

/// A resolved alignment as JSON, or `null`.
#[cfg(feature = "xlsx")]
fn xlsx_alignment_json(a: Option<&crate::adapter::xlsx::Alignment>) -> String {
    match a {
        Some(a) => format!(
            "{{\"horizontal\":{},\"vertical\":{},\"wrapText\":{}}}",
            opt_str_json(a.horizontal.as_deref()),
            opt_str_json(a.vertical.as_deref()),
            a.wrap_text
        ),
        None => "null".to_string(),
    }
}

/// A cell's resolved style as JSON, or `null` (Phase 21.1.2). Distinct from the
/// cell's value, formula, and span.
#[cfg(feature = "xlsx")]
fn xlsx_cell_style_json(
    style_index: Option<u32>,
    style: Option<&crate::adapter::xlsx::CellStyle>,
) -> String {
    match (style_index, style) {
        (Some(i), Some(s)) => {
            let font = match &s.font {
                Some(f) => format!(
                    concat!("{{\"bold\":{},\"italic\":{},", "\"size\":{},\"name\":{}}}"),
                    f.bold,
                    f.italic,
                    opt_str_json(f.size.as_deref()),
                    opt_str_json(f.name.as_deref())
                ),
                None => "null".to_string(),
            };
            let fill = match &s.fill {
                Some(f) => format!(
                    concat!("{{\"patternType\":{},", "\"fgColor\":{},\"bgColor\":{}}}"),
                    opt_str_json(f.pattern_type.as_deref()),
                    opt_str_json(f.fg_color.as_deref()),
                    opt_str_json(f.bg_color.as_deref())
                ),
                None => "null".to_string(),
            };
            format!(
                concat!(
                    "{{\"index\":{},\"numFmtId\":{},\"formatCode\":{},",
                    "\"font\":{},\"fill\":{},\"alignment\":{}}}"
                ),
                i,
                s.num_fmt_id,
                opt_str_json(s.format_code.as_deref()),
                font,
                fill,
                xlsx_alignment_json(s.alignment.as_ref())
            )
        }
        (Some(i), None) => format!("{{\"index\":{i}}}"),
        _ => "null".to_string(),
    }
}

/// A relationship scoped to one owner part ordinal (ids are only unique within a
/// `.rels` part, so the global lookup is deliberately not used here).
#[cfg(feature = "xlsx")]
fn xlsx_part_rel<'a>(
    model: &'a crate::adapter::package::opc::OpcModel,
    owner: u32,
    id: &str,
) -> Option<&'a crate::adapter::package::opc::Relationship> {
    model
        .part_rels
        .iter()
        .find(|(o, _)| *o == owner)?
        .1
        .iter()
        .find(|r| r.id == id)
}

/// The first relationship of `owner` whose type ends with `suffix`.
#[cfg(feature = "xlsx")]
fn xlsx_find_rel<'a>(
    model: &'a crate::adapter::package::opc::OpcModel,
    owner: u32,
    suffix: &str,
) -> Option<&'a crate::adapter::package::opc::Relationship> {
    let tail = format!("/{suffix}");
    model
        .part_rels
        .iter()
        .find(|(o, _)| *o == owner)?
        .1
        .iter()
        .find(|r| r.rel_type == suffix || r.rel_type.ends_with(&tail))
}

// ---------------------------------------------------------------------------
// Common (format-neutral) observations (Phase 12.7)
// ---------------------------------------------------------------------------

impl<S: SeedStore> Ctx<'_, S> {
    /// The detected document format recorded in the manifest provenance.
    fn document_format(&self) -> Option<DocumentFormat> {
        DocumentFormat::from_provenance(&self.manifest.provenance)
    }

    /// Tag a native answer with the common layer's format + native provenance.
    fn tag_common(&self, fmt: DocumentFormat, mut answer: FieldAnswer) -> FieldAnswer {
        answer.provenance = format!("format={};common;{}", fmt.name(), answer.provenance);
        answer
    }

    /// Dispatch a common selector through the detected format's adapter.
    fn common_dispatch(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        use crate::field::capabilities;
        let fmt = self.document_format().ok_or_else(|| {
            Error::unsupported_feature(
                "field manifest does not record a document format; common observations are unavailable",
            )
        })?;
        if !capabilities::common_supported(fmt, &req.selector, req.representation) {
            return Err(Error::unsupported_feature(format!(
                "unsupported common observation: format {} does not support selector {} with representation {}",
                fmt.name(),
                req.selector.canonical(),
                req.representation.name()
            )));
        }
        let answer = match fmt {
            DocumentFormat::Pdf => self.common_pdf(req)?,
            DocumentFormat::Docx => self.common_docx(req)?,
            DocumentFormat::Epub => self.common_epub(req)?,
            DocumentFormat::Odt => self.common_odt(req)?,
            DocumentFormat::Xlsx => self.common_xlsx(req)?,
            DocumentFormat::Pptx => self.common_pptx(req)?,
            DocumentFormat::Opaque => {
                return Err(Error::unsupported_feature(
                    "opaque fields have no common observations",
                ));
            }
        };
        Ok(self.tag_common(fmt, answer))
    }

    // -- PDF ---------------------------------------------------------------

    fn common_pdf(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        match &req.selector {
            Selector::Metadata => {
                let mut a = self.document_metadata(req)?;
                a.provenance = "pdf;document-metadata".to_string();
                Ok(a)
            }
            Selector::Text => self.pdf_document_text(req),
            Selector::SearchMatch(p) => self.text_match(req, p),
            other => Err(Error::unsupported_feature(format!(
                "PDF does not support common selector {}",
                other.canonical()
            ))),
        }
    }

    /// The whole-document reading text of a PDF: every recovered page's text in
    /// page order. Bounded by the output budget and the page-scan cap.
    fn pdf_document_text(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        let mut out = String::new();
        let mut pages: u64 = 0;
        let mut page: u32 = 1;
        while page <= MAX_TEXTMATCH_PAGES {
            let entries = self.lookup(SelectorKey::new(SEL_PAGE, page))?;
            let Some(entry) = entries.into_iter().next() else {
                break;
            };
            let pc = entry.node_id;
            let (_ops, text, _preview) = self.ensure_page_derived(page, pc)?;
            let bytes = self.materialize(&text)?;
            out.push_str(&String::from_utf8_lossy(&bytes));
            if !out.ends_with('\n') {
                out.push('\n');
            }
            if out.len() as u64 > req.budget.max_output_bytes {
                return Err(Error::resource_limit(format!(
                    "whole-document text exceeded the {}-byte budget",
                    req.budget.max_output_bytes
                )));
            }
            pages += 1;
            page += 1;
        }
        Ok(FieldAnswer {
            value: AnswerValue::Text(out),
            basis: Basis::Heuristic,
            selector: req.selector.canonical(),
            representation: req.representation.name().to_string(),
            source_span: None,
            provenance: format!("pdf;pages={pages}"),
            dependency_ids: Vec::new(),
            integrity_scope: IntegrityScope::None,
            exact: false,
        })
    }

    // -- DOCX --------------------------------------------------------------

    #[cfg(feature = "docx")]
    fn common_docx(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        let profile = DocxExtractProfile::DEFAULT;
        match &req.selector {
            Selector::Metadata => self.docx_common_metadata(req, &profile),
            Selector::Text => self.docx_story_text(req, DocxStory::Main, &profile),
            Selector::Heading(i) => self.docx_common_heading(req, *i, &profile),
            Selector::Block(i) => self.docx_common_block(req, *i, &profile),
            Selector::Table(i) => self.docx_table(req, DocxStory::Main, *i, &profile),
            Selector::Cell { table, row, col } => {
                let cell = a1_ref(*col, *row);
                self.docx_cell(req, DocxStory::Main, *table, &cell, &profile)
            }
            Selector::Resource(i) => self.docx_common_resource(req, *i, &profile),
            Selector::Link(i) => self.docx_common_link(req, *i, &profile),
            Selector::SearchMatch(p) => self.docx_find(req, DocxStory::Main, p, &profile),
            other => Err(Error::unsupported_feature(format!(
                "DOCX does not support common selector {}",
                other.canonical()
            ))),
        }
    }

    #[cfg(not(feature = "docx"))]
    fn common_docx(&mut self, _req: &ObserveRequest) -> Result<FieldAnswer> {
        Err(Error::unsupported_feature(
            "DOCX observations require a build with the docx feature",
        ))
    }

    #[cfg(feature = "docx")]
    fn docx_common_metadata(
        &mut self,
        req: &ObserveRequest,
        profile: &DocxExtractProfile,
    ) -> Result<FieldAnswer> {
        let v = self.docx_story_view(DocxStory::Main, profile)?;
        let json = format!(
            concat!(
                "{{\"format\":\"docx\",\"story\":\"main\",\"part\":\"{}\",\"ordinal\":{},",
                "\"root\":\"{}\",\"blocks\":{},\"paragraphs\":{},\"tables\":{},",
                "\"hyperlinks\":{},\"bookmarks\":{},\"resources\":{},\"sections\":{},",
                "\"profile\":\"{}\"}}"
            ),
            json_escape(&v.part.name),
            v.part.ordinal,
            json_escape(&v.model.root_local),
            v.model.blocks.len(),
            v.model.paragraphs().count(),
            v.model.tables().count(),
            v.model.hyperlinks.len(),
            v.model.bookmarks.len(),
            v.model.resources.len(),
            v.model.section_count,
            profile.fingerprint(),
        );
        let provenance = format!("docx;story=main;part={}", v.part.name);
        Ok(self.docx_answer(req, AnswerValue::Json(json), provenance, v.span, v.deps))
    }

    #[cfg(feature = "docx")]
    fn docx_common_heading(
        &mut self,
        req: &ObserveRequest,
        ordinal: u32,
        profile: &DocxExtractProfile,
    ) -> Result<FieldAnswer> {
        let v = self.docx_story_view(DocxStory::Main, profile)?;
        let p = v
            .model
            .paragraphs()
            .filter(|p| p.heading_level.is_some())
            .nth(ordinal as usize)
            .ok_or_else(|| {
                Error::unsupported_feature(format!("DOCX main story has no heading {ordinal}"))
            })?;
        let index = p.index;
        let text = p.text.clone();
        let level = p.heading_level;
        let style = p.style_id.clone();
        let value = match req.representation {
            Representation::Text => AnswerValue::Text(text),
            Representation::Metadata => AnswerValue::Json(format!(
                "{{\"story\":\"main\",\"heading\":{ordinal},\"paragraph\":{index},\"level\":{},\"style\":{},\"text_len\":{}}}",
                opt_u8_json(level),
                opt_str_json(style.as_deref()),
                text.len()
            )),
            _ => return Err(unsupported_common(req)),
        };
        let provenance = format!(
            "docx;story=main;part={};heading={ordinal};paragraph={index};profile={}",
            v.part.name,
            profile.fingerprint()
        );
        Ok(self.docx_answer(req, value, provenance, v.span, v.deps))
    }

    #[cfg(feature = "docx")]
    fn docx_common_block(
        &mut self,
        req: &ObserveRequest,
        ordinal: u32,
        profile: &DocxExtractProfile,
    ) -> Result<FieldAnswer> {
        use crate::adapter::docx::wml::Block as WmlBlock;
        let v = self.docx_story_view(DocxStory::Main, profile)?;
        let b = v.model.blocks.get(ordinal as usize).ok_or_else(|| {
            Error::unsupported_feature(format!("DOCX main story has no block {ordinal}"))
        })?;
        let (kind, text, detail) = match b {
            WmlBlock::Paragraph(p) => (
                "paragraph",
                p.text.clone(),
                format!(
                    "\"paragraph\":{},\"level\":{}",
                    p.index,
                    opt_u8_json(p.heading_level)
                ),
            ),
            WmlBlock::Table(t) => (
                "table",
                t.text(),
                format!("\"table\":{},\"rows\":{}", t.index, t.rows.len()),
            ),
        };
        let value = match req.representation {
            Representation::Text => AnswerValue::Text(text),
            Representation::Metadata => AnswerValue::Json(format!(
                "{{\"story\":\"main\",\"block\":{ordinal},\"kind\":\"{kind}\",{detail},\"text_len\":{}}}",
                text.len()
            )),
            _ => return Err(unsupported_common(req)),
        };
        let provenance = format!(
            "docx;story=main;part={};block={ordinal};profile={}",
            v.part.name,
            profile.fingerprint()
        );
        Ok(self.docx_answer(req, value, provenance, v.span, v.deps))
    }

    #[cfg(feature = "docx")]
    fn docx_common_resource(
        &mut self,
        req: &ObserveRequest,
        ordinal: u32,
        profile: &DocxExtractProfile,
    ) -> Result<FieldAnswer> {
        let v = self.docx_story_view(DocxStory::Main, profile)?;
        let rel = v.model.resources.get(ordinal as usize).ok_or_else(|| {
            Error::unsupported_feature(format!("DOCX main story has no resource {ordinal}"))
        })?;
        let json = format!(
            "{{\"story\":\"main\",\"resource\":{ordinal},\"rel\":\"{}\"}}",
            json_escape(rel)
        );
        let provenance = format!(
            "docx;story=main;part={};resource={ordinal};profile={}",
            v.part.name,
            profile.fingerprint()
        );
        Ok(self.docx_answer(req, AnswerValue::Json(json), provenance, v.span, v.deps))
    }

    #[cfg(feature = "docx")]
    fn docx_common_link(
        &mut self,
        req: &ObserveRequest,
        ordinal: u32,
        profile: &DocxExtractProfile,
    ) -> Result<FieldAnswer> {
        let v = self.docx_story_view(DocxStory::Main, profile)?;
        let l = v.model.hyperlinks.get(ordinal as usize).ok_or_else(|| {
            Error::unsupported_feature(format!("DOCX main story has no hyperlink {ordinal}"))
        })?;
        let json = format!(
            "{{\"story\":\"main\",\"link\":{ordinal},\"text\":\"{}\",\"rel_id\":{},\"anchor\":{}}}",
            json_escape(&l.text),
            opt_str_json(l.rel_id.as_deref()),
            opt_str_json(l.anchor.as_deref())
        );
        let provenance = format!(
            "docx;story=main;part={};link={ordinal};profile={}",
            v.part.name,
            profile.fingerprint()
        );
        Ok(self.docx_answer(req, AnswerValue::Json(json), provenance, v.span, v.deps))
    }

    // -- EPUB --------------------------------------------------------------

    #[cfg(feature = "epub")]
    fn common_epub(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        let profile = EpubExtractProfile::DEFAULT;
        match &req.selector {
            Selector::Metadata => self.epub_package(req),
            Selector::Text => self.epub_common_text(req, &profile),
            Selector::Heading(i) => self.epub_common_block(req, *i, true, &profile),
            Selector::Block(i) => self.epub_common_block(req, *i, false, &profile),
            Selector::Table(i) => self.epub_common_table(req, *i, &profile),
            Selector::Cell { table, row, col } => {
                self.epub_common_cell(req, *table, *row, *col, &profile)
            }
            Selector::Resource(i) => self.epub_common_resource(req, *i, &profile),
            Selector::Link(i) => self.epub_common_link(req, *i, &profile),
            Selector::SearchMatch(p) => self.epub_common_search(req, p, &profile),
            other => Err(Error::unsupported_feature(format!(
                "EPUB does not support common selector {}",
                other.canonical()
            ))),
        }
    }

    #[cfg(not(feature = "epub"))]
    fn common_epub(&mut self, _req: &ObserveRequest) -> Result<FieldAnswer> {
        Err(Error::unsupported_feature(
            "EPUB observations require a build with the epub feature",
        ))
    }

    #[cfg(not(feature = "odt"))]
    fn common_odt(&mut self, _req: &ObserveRequest) -> Result<FieldAnswer> {
        Err(Error::unsupported_feature(
            "ODT observations require a build with the odt feature",
        ))
    }

    #[cfg(not(feature = "xlsx"))]
    fn common_xlsx(&mut self, _req: &ObserveRequest) -> Result<FieldAnswer> {
        Err(Error::unsupported_feature(
            "XLSX observations require a build with the xlsx feature",
        ))
    }

    #[cfg(not(feature = "pptx"))]
    fn common_pptx(&mut self, _req: &ObserveRequest) -> Result<FieldAnswer> {
        Err(Error::unsupported_feature(
            "PPTX observations require a build with the pptx feature",
        ))
    }

    #[cfg(feature = "epub")]
    fn epub_common_text(
        &mut self,
        req: &ObserveRequest,
        profile: &EpubExtractProfile,
    ) -> Result<FieldAnswer> {
        let doc = self.epub_package_doc()?;
        let order_len = doc.reading_order(profile).len() as u32;
        let mut out = String::new();
        let mut items: u64 = 0;
        for index in 0..order_len {
            let (model, _item, _span, _deps) = self.epub_content_view(index, profile)?;
            out.push_str(&model.text());
            if !out.ends_with('\n') {
                out.push('\n');
            }
            if out.len() as u64 > req.budget.max_output_bytes {
                return Err(Error::resource_limit(format!(
                    "whole-document text exceeded the {}-byte budget",
                    req.budget.max_output_bytes
                )));
            }
            items += 1;
        }
        let provenance = format!("epub;spine-items={items};profile={}", profile.fingerprint());
        Ok(self.epub_answer(req, AnswerValue::Text(out), provenance, None, Vec::new()))
    }

    /// The `ordinal`-th block across the reading order; `headings_only` restricts
    /// the count to heading blocks.
    #[cfg(feature = "epub")]
    fn epub_common_block(
        &mut self,
        req: &ObserveRequest,
        ordinal: u32,
        headings_only: bool,
        profile: &EpubExtractProfile,
    ) -> Result<FieldAnswer> {
        let doc = self.epub_package_doc()?;
        let order_len = doc.reading_order(profile).len() as u32;
        let mut remaining = ordinal as usize;
        for index in 0..order_len {
            let (model, item, span, deps) = self.epub_content_view(index, profile)?;
            for (local, b) in model.blocks.iter().enumerate() {
                if headings_only && !matches!(b, crate::adapter::epub::Block::Heading { .. }) {
                    continue;
                }
                if remaining == 0 {
                    let text = b.text();
                    let value = match req.representation {
                        Representation::Text => AnswerValue::Text(text),
                        Representation::Metadata => {
                            AnswerValue::Json(epub_block_json(local as u32, b))
                        }
                        _ => return Err(unsupported_common(req)),
                    };
                    let provenance = format!(
                        "epub;spine={index};part={};block={local};ordinal={ordinal};profile={}",
                        item.resolved.as_deref().unwrap_or(""),
                        profile.fingerprint()
                    );
                    return Ok(self.epub_answer(req, value, provenance, span, deps));
                }
                remaining -= 1;
            }
        }
        let what = if headings_only { "heading" } else { "block" };
        Err(Error::unsupported_feature(format!(
            "EPUB reading order has no {what} {ordinal}"
        )))
    }

    #[cfg(feature = "epub")]
    fn epub_common_table(
        &mut self,
        req: &ObserveRequest,
        ordinal: u32,
        profile: &EpubExtractProfile,
    ) -> Result<FieldAnswer> {
        let doc = self.epub_package_doc()?;
        let order_len = doc.reading_order(profile).len() as u32;
        let mut remaining = ordinal as usize;
        for index in 0..order_len {
            let (model, item, span, deps) = self.epub_content_view(index, profile)?;
            for (local, b) in model.blocks.iter().enumerate() {
                if !matches!(b, crate::adapter::epub::Block::Table { .. }) {
                    continue;
                }
                if remaining == 0 {
                    let value = match req.representation {
                        Representation::Text => AnswerValue::Text(b.text()),
                        Representation::Metadata => {
                            AnswerValue::Json(epub_block_json(local as u32, b))
                        }
                        _ => return Err(unsupported_common(req)),
                    };
                    let provenance = format!(
                        "epub;spine={index};part={};table={local};ordinal={ordinal};profile={}",
                        item.resolved.as_deref().unwrap_or(""),
                        profile.fingerprint()
                    );
                    return Ok(self.epub_answer(req, value, provenance, span, deps));
                }
                remaining -= 1;
            }
        }
        Err(Error::unsupported_feature(format!(
            "EPUB reading order has no table {ordinal}"
        )))
    }

    #[cfg(feature = "epub")]
    fn epub_common_cell(
        &mut self,
        req: &ObserveRequest,
        table: u32,
        row: u32,
        col: u32,
        profile: &EpubExtractProfile,
    ) -> Result<FieldAnswer> {
        use crate::adapter::epub::Block as EBlock;
        let doc = self.epub_package_doc()?;
        let order_len = doc.reading_order(profile).len() as u32;
        let mut remaining = table as usize;
        for index in 0..order_len {
            let (model, item, span, deps) = self.epub_content_view(index, profile)?;
            for (local, b) in model.blocks.iter().enumerate() {
                let EBlock::Table { rows } = b else { continue };
                if remaining > 0 {
                    remaining -= 1;
                    continue;
                }
                let r = rows.get(row as usize).ok_or_else(|| {
                    Error::unsupported_feature(format!("EPUB table {table} has no row {row}"))
                })?;
                let c = r.cells.get(col as usize).ok_or_else(|| {
                    Error::unsupported_feature(format!(
                        "EPUB table {table} row {row} has no cell {col}"
                    ))
                })?;
                let value = match req.representation {
                    Representation::Text => AnswerValue::Text(c.text.clone()),
                    Representation::Metadata => {
                        AnswerValue::Json(epub_cell_json(table, row, col, c))
                    }
                    _ => return Err(unsupported_common(req)),
                };
                let provenance = format!(
                    "epub;spine={index};part={};table={local};row={row};col={col};profile={}",
                    item.resolved.as_deref().unwrap_or(""),
                    profile.fingerprint()
                );
                return Ok(self.epub_answer(req, value, provenance, span, deps));
            }
        }
        Err(Error::unsupported_feature(format!(
            "EPUB reading order has no table {table}"
        )))
    }

    #[cfg(feature = "epub")]
    fn epub_common_resource(
        &mut self,
        req: &ObserveRequest,
        ordinal: u32,
        profile: &EpubExtractProfile,
    ) -> Result<FieldAnswer> {
        let doc = self.epub_package_doc()?;
        let resources: Vec<&ManifestItem> = doc
            .manifest
            .iter()
            .filter(|i| !is_document_media_type(&i.media_type))
            .collect();
        let item = resources
            .get(ordinal as usize)
            .copied()
            .ok_or_else(|| Error::unsupported_feature(format!("EPUB has no resource {ordinal}")))?;
        if req.representation == Representation::Metadata {
            let json = format!(
                concat!(
                    "{{\"resource\":{},\"id\":\"{}\",\"href\":\"{}\",",
                    "\"media_type\":\"{}\",\"member\":{},\"profile\":\"{}\"}}"
                ),
                ordinal,
                json_escape(&item.id),
                json_escape(&item.href),
                json_escape(&item.media_type),
                epub_opt_str(item.resolved.as_deref()),
                profile.fingerprint()
            );
            let provenance = format!(
                "epub;resource={ordinal};id={};profile={}",
                item.id,
                profile.fingerprint()
            );
            return Ok(self.epub_answer(
                req,
                AnswerValue::Json(json),
                provenance,
                None,
                Vec::new(),
            ));
        }
        // Exact/decoded bytes resolve to the container member.
        self.epub_item_bytes(req, item)
    }

    #[cfg(feature = "epub")]
    fn epub_common_link(
        &mut self,
        req: &ObserveRequest,
        ordinal: u32,
        profile: &EpubExtractProfile,
    ) -> Result<FieldAnswer> {
        let doc = self.epub_package_doc()?;
        let order_len = doc.reading_order(profile).len() as u32;
        let mut remaining = ordinal as usize;
        for index in 0..order_len {
            let (model, item, span, deps) = self.epub_content_view(index, profile)?;
            for (local, l) in model.links.iter().enumerate() {
                if remaining == 0 {
                    let provenance = format!(
                        "epub;spine={index};part={};link={local};ordinal={ordinal};profile={}",
                        item.resolved.as_deref().unwrap_or(""),
                        profile.fingerprint()
                    );
                    return Ok(self.epub_answer(
                        req,
                        AnswerValue::Json(epub_link_json(local as u32, l)),
                        provenance,
                        span,
                        deps,
                    ));
                }
                remaining -= 1;
            }
        }
        Err(Error::unsupported_feature(format!(
            "EPUB reading order has no link {ordinal}"
        )))
    }

    #[cfg(feature = "epub")]
    fn epub_common_search(
        &mut self,
        req: &ObserveRequest,
        pattern: &str,
        profile: &EpubExtractProfile,
    ) -> Result<FieldAnswer> {
        let doc = self.epub_package_doc()?;
        let order_len = doc.reading_order(profile).len() as u32;
        let mut items: Vec<String> = Vec::new();
        let mut estimated: u64 = 0;
        for index in 0..order_len {
            let (model, item, _span, _deps) = self.epub_content_view(index, profile)?;
            let _ = item;
            for (local, b) in model.blocks.iter().enumerate() {
                let t = b.text();
                if t.contains(pattern) {
                    estimated = estimated.saturating_add(t.len() as u64 + 64);
                    if estimated > req.budget.max_output_bytes {
                        return Err(Error::resource_limit(format!(
                            "EPUB search exceeded the {}-byte budget",
                            req.budget.max_output_bytes
                        )));
                    }
                    items.push(format!(
                        "{{\"spine\":{index},\"block\":{local},\"kind\":\"{}\",\"text\":\"{}\"}}",
                        b.kind(),
                        json_escape(&t)
                    ));
                }
            }
        }
        let provenance = format!("epub;search;profile={}", profile.fingerprint());
        Ok(self.epub_answer(
            req,
            AnswerValue::Json(format!("[{}]", items.join(","))),
            provenance,
            None,
            Vec::new(),
        ))
    }
}

// ---------------------------------------------------------------------------
// ODT common observations (Phase 13.3)
// ---------------------------------------------------------------------------

#[cfg(feature = "odt")]
impl<S: SeedStore> Ctx<'_, S> {
    fn common_odt(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        let profile = OdtExtractProfile::DEFAULT;
        match &req.selector {
            Selector::Metadata => self.odt_common_metadata(req, &profile),
            Selector::Text => self.odt_content_text(req, &profile),
            Selector::Heading(i) => self.odt_common_heading(req, *i, &profile),
            Selector::Block(i) => self.odt_common_block(req, *i, &profile),
            Selector::Table(i) => self.odt_table(req, *i, &profile),
            Selector::Cell { table, row, col } => self.odt_cell(req, *table, *row, *col, &profile),
            Selector::Resource(i) => self.odt_common_resource(req, *i, &profile),
            Selector::Link(i) => self.odt_common_link(req, *i, &profile),
            Selector::SearchMatch(p) => self.odt_find(req, p, &profile),
            other => Err(Error::unsupported_feature(format!(
                "ODT does not support common selector {}",
                other.canonical()
            ))),
        }
    }

    fn odt_content_text(
        &mut self,
        req: &ObserveRequest,
        profile: &OdtExtractProfile,
    ) -> Result<FieldAnswer> {
        let (m, part, span, deps) = self.odt_content_view(profile)?;
        let provenance = format!("odt;part={};profile={}", part.name, profile.fingerprint());
        Ok(self.odt_answer(req, AnswerValue::Text(m.text()), provenance, span, deps))
    }

    fn odt_common_metadata(
        &mut self,
        req: &ObserveRequest,
        profile: &OdtExtractProfile,
    ) -> Result<FieldAnswer> {
        let model = self.odt_model()?;
        let part = model.content.as_ref().ok_or_else(|| {
            Error::invalid_package_structure(
                "ODF package has no resolvable OpenDocument content part",
            )
        })?;
        let (m, _part, span, deps) = self.odt_content_view(profile)?;
        let json = format!(
            concat!(
                "{{",
                "\"format\":\"odt\",",
                "\"part\":\"{}\",",
                "\"ordinal\":{},",
                "\"media_type\":{},",
                "\"root\":\"{}\",",
                "\"manifest_entries\":{},",
                "\"blocks\":{},",
                "\"paragraphs\":{},",
                "\"headings\":{},",
                "\"tables\":{},",
                "\"lists\":{},",
                "\"hyperlinks\":{},",
                "\"bookmarks\":{},",
                "\"notes\":{},",
                "\"resources\":{},",
                "\"sections\":{},",
                "\"profile\":\"{}\"",
                "}}"
            ),
            json_escape(&part.name),
            part.ordinal,
            opt_str_json(part.media_type.as_deref()),
            json_escape(&m.root_local),
            model.manifest.len(),
            m.blocks.len(),
            m.paragraphs().filter(|p| !p.is_heading()).count(),
            m.headings().count(),
            m.tables().count(),
            m.lists().count(),
            m.hyperlinks.len(),
            m.bookmarks.len(),
            m.notes.len(),
            m.resources.len(),
            m.sections.len(),
            profile.fingerprint(),
        );
        let provenance = format!("odt;part={};profile={}", part.name, profile.fingerprint());
        Ok(self.odt_answer(req, AnswerValue::Json(json), provenance, span, deps))
    }

    fn odt_common_heading(
        &mut self,
        req: &ObserveRequest,
        ordinal: u32,
        profile: &OdtExtractProfile,
    ) -> Result<FieldAnswer> {
        let (m, part, span, deps) = self.odt_content_view(profile)?;
        let h = m.headings().nth(ordinal as usize).ok_or_else(|| {
            Error::unsupported_feature(format!("ODT content has no common heading {ordinal}"))
        })?;
        let text = h.text.clone();
        let level = h.heading_level;
        let index = h.index;
        let style = h.style_id.clone();
        let value = match req.representation {
            Representation::Text => AnswerValue::Text(text),
            Representation::Metadata => AnswerValue::Json(format!(
                "{{\"part\":\"{}\",\"heading\":{ordinal},\"index\":{index},\"level\":{},\"style\":{},\"text_len\":{}}}",
                json_escape(&part.name),
                opt_u8_json(level),
                opt_str_json(style.as_deref()),
                text.len()
            )),
            _ => return Err(unsupported_common(req)),
        };
        let provenance = format!(
            "odt;part={};heading={ordinal};profile={}",
            part.name,
            profile.fingerprint()
        );
        Ok(self.odt_answer(req, value, provenance, span, deps))
    }

    fn odt_common_block(
        &mut self,
        req: &ObserveRequest,
        ordinal: u32,
        profile: &OdtExtractProfile,
    ) -> Result<FieldAnswer> {
        let (m, part, span, deps) = self.odt_content_view(profile)?;
        let b = m.blocks.get(ordinal as usize).ok_or_else(|| {
            Error::unsupported_feature(format!("ODT content has no block {ordinal}"))
        })?;
        let (kind, text) = match b {
            OdtBlock::Paragraph(p) => ("paragraph", p.text.clone()),
            OdtBlock::Table(t) => ("table", t.text()),
            OdtBlock::List(l) => ("list", l.text()),
        };
        let value = match req.representation {
            Representation::Text => AnswerValue::Text(text),
            Representation::Metadata => AnswerValue::Json(format!(
                "{{\"part\":\"{}\",\"block\":{ordinal},\"kind\":\"{kind}\",\"text_len\":{}}}",
                json_escape(&part.name),
                text.len()
            )),
            _ => return Err(unsupported_common(req)),
        };
        let provenance = format!(
            "odt;part={};block={ordinal};profile={}",
            part.name,
            profile.fingerprint()
        );
        Ok(self.odt_answer(req, value, provenance, span, deps))
    }

    fn odt_common_resource(
        &mut self,
        req: &ObserveRequest,
        ordinal: u32,
        profile: &OdtExtractProfile,
    ) -> Result<FieldAnswer> {
        let (m, part, span, deps) = self.odt_content_view(profile)?;
        let r = m.resources.get(ordinal as usize).ok_or_else(|| {
            Error::unsupported_feature(format!("ODT content has no resource {ordinal}"))
        })?;
        let json = format!(
            "{{\"part\":\"{}\",\"resource\":{ordinal},\"href\":\"{}\",\"member\":{},\"external\":{}}}",
            json_escape(&part.name),
            json_escape(&r.href),
            opt_str_json(r.member.as_deref()),
            r.external,
        );
        let provenance = format!(
            "odt;part={};resource={ordinal};profile={}",
            part.name,
            profile.fingerprint()
        );
        Ok(self.odt_answer(req, AnswerValue::Json(json), provenance, span, deps))
    }

    fn odt_common_link(
        &mut self,
        req: &ObserveRequest,
        ordinal: u32,
        profile: &OdtExtractProfile,
    ) -> Result<FieldAnswer> {
        let (m, part, span, deps) = self.odt_content_view(profile)?;
        let l = m.hyperlinks.get(ordinal as usize).ok_or_else(|| {
            Error::unsupported_feature(format!("ODT content has no hyperlink {ordinal}"))
        })?;
        let json = format!(
            "{{\"part\":\"{}\",\"link\":{ordinal},\"text\":\"{}\",\"href\":\"{}\",\"external\":{}}}",
            json_escape(&part.name),
            json_escape(&l.text),
            json_escape(&l.href),
            l.external,
        );
        let provenance = format!(
            "odt;part={};link={ordinal};profile={}",
            part.name,
            profile.fingerprint()
        );
        Ok(self.odt_answer(req, AnswerValue::Json(json), provenance, span, deps))
    }
}

// ---------------------------------------------------------------------------
// XLSX common observations (Phase 21.1.1)
// ---------------------------------------------------------------------------

#[cfg(feature = "xlsx")]
impl<S: SeedStore> Ctx<'_, S> {
    fn common_xlsx(&mut self, req: &ObserveRequest) -> Result<FieldAnswer> {
        let profile = XlsxExtractProfile::DEFAULT;
        match &req.selector {
            Selector::Metadata => self.xlsx_common_metadata(req, &profile),
            Selector::Text => self.xlsx_common_text(req, &profile),
            Selector::Table(i) => self.xlsx_table(req, *i, &profile),
            Selector::Cell { table, row, col } => {
                self.xlsx_common_cell(req, *table, *row, *col, &profile)
            }
            Selector::SearchMatch(p) => self.xlsx_find(req, p, &profile),
            other => Err(Error::unsupported_feature(format!(
                "XLSX does not support common selector {}",
                other.canonical()
            ))),
        }
    }

    fn xlsx_common_metadata(
        &mut self,
        req: &ObserveRequest,
        profile: &XlsxExtractProfile,
    ) -> Result<FieldAnswer> {
        let model = self.xlsx_model()?;
        let workbook = self.xlsx_workbook()?;
        let detail = workbook
            .sheets
            .iter()
            .enumerate()
            .map(|(i, s)| {
                format!(
                    "{{\"index\":{i},\"name\":\"{}\",\"state\":\"{}\",\"sheetId\":{}}}",
                    json_escape(&s.name),
                    s.state.name(),
                    opt_u32_json(s.sheet_id),
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let names = workbook
            .sheets
            .iter()
            .map(|s| format!("\"{}\"", json_escape(&s.name)))
            .collect::<Vec<_>>()
            .join(",");
        let json = format!(
            concat!(
                "{{\"format\":\"xlsx\",\"workbook\":\"{}\",\"ordinal\":{},",
                "\"styles\":{},\"shared_strings\":{},\"defined_names\":{},",
                "\"sheets\":{},\"sheet_names\":[{}],\"sheets_detail\":[{}],\"profile\":\"{}\"}}"
            ),
            json_escape(&model.workbook.name),
            model.workbook.ordinal,
            model.styles.is_some(),
            model.shared_strings.is_some(),
            workbook.defined_names.len(),
            workbook.sheets.len(),
            names,
            detail,
            profile.fingerprint()
        );
        let provenance = format!(
            "xlsx;workbook={};profile={}",
            model.workbook.name,
            profile.fingerprint()
        );
        Ok(self.xlsx_answer(req, AnswerValue::Json(json), provenance, None, Vec::new()))
    }

    fn xlsx_common_text(
        &mut self,
        req: &ObserveRequest,
        profile: &XlsxExtractProfile,
    ) -> Result<FieldAnswer> {
        let workbook = self.xlsx_workbook()?;
        let indices = self.xlsx_projected_indices(&workbook, profile);
        let mut out = String::new();
        let mut count: u64 = 0;
        for index in indices {
            let (sheet, _part, _name, _span, _deps) = self.xlsx_sheet_view(index, profile)?;
            let text = sheet.text(profile.values, self.limits.max_xlsx_cells)?;
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&text);
            if out.len() as u64 > req.budget.max_output_bytes {
                return Err(Error::resource_limit(format!(
                    "whole-workbook text exceeded the {}-byte budget",
                    req.budget.max_output_bytes
                )));
            }
            count += 1;
        }
        let provenance = format!("xlsx;sheets={count};profile={}", profile.fingerprint());
        Ok(self.xlsx_answer(req, AnswerValue::Text(out), provenance, None, Vec::new()))
    }

    fn xlsx_table(
        &mut self,
        req: &ObserveRequest,
        ordinal: u32,
        profile: &XlsxExtractProfile,
    ) -> Result<FieldAnswer> {
        let workbook = self.xlsx_workbook()?;
        let indices = self.xlsx_projected_indices(&workbook, profile);
        let index = *indices.get(ordinal as usize).ok_or_else(|| {
            Error::unsupported_feature(format!("workbook has no projected sheet {ordinal}"))
        })?;
        let (sheet, part, sheet_name, span, deps) = self.xlsx_sheet_view(index, profile)?;
        let provenance = format!(
            "xlsx;sheet={sheet_name};index={index};table={ordinal};part={};profile={}",
            part.name,
            profile.fingerprint()
        );
        let value = match req.representation {
            Representation::Text => {
                AnswerValue::Text(sheet.text(profile.values, self.limits.max_xlsx_cells)?)
            }
            Representation::Metadata => AnswerValue::Json(format!(
                "{{\"sheet\":\"{}\",\"index\":{},\"rows\":{},\"cells\":{},\"merges\":{},\"merge_count\":{},\"profile\":\"{}\"}}",
                json_escape(&sheet_name),
                index,
                sheet.rows.len(),
                sheet.cell_count(),
                xlsx_str_array(&sheet.merges),
                sheet.merges.len(),
                profile.fingerprint()
            )),
            _ => return Err(unsupported_common(req)),
        };
        Ok(self.xlsx_answer(req, value, provenance, span, deps))
    }

    fn xlsx_common_cell(
        &mut self,
        req: &ObserveRequest,
        table: u32,
        row: u32,
        col: u32,
        profile: &XlsxExtractProfile,
    ) -> Result<FieldAnswer> {
        let workbook = self.xlsx_workbook()?;
        let indices = self.xlsx_projected_indices(&workbook, profile);
        let index = *indices.get(table as usize).ok_or_else(|| {
            Error::unsupported_feature(format!("workbook has no projected sheet {table}"))
        })?;
        let (sheet, part, sheet_name, span, deps) = self.xlsx_sheet_view(index, profile)?;
        let reference = crate::adapter::xlsx::col_row_to_a1(col, row);
        let found = sheet.cell_at(row, col).cloned().ok_or_else(|| {
            Error::unsupported_feature(format!(
                "sheet {index} ({sheet_name}) has no cell {reference}"
            ))
        })?;
        let provenance = format!(
            "xlsx;sheet={sheet_name};index={index};cell={reference};part={};profile={}",
            part.name,
            profile.fingerprint()
        );
        let value = match req.representation {
            Representation::Text => {
                AnswerValue::Text(sheet.facet(&found, profile.values).unwrap_or_default())
            }
            Representation::Metadata => AnswerValue::Json(format!(
                concat!(
                    "{{\"sheet\":\"{}\",\"index\":{},\"cell\":\"{}\",\"col\":{},\"row\":{},",
                    "\"kind\":\"{}\",\"value\":{},\"formula\":{},\"profile\":\"{}\"}}"
                ),
                json_escape(&sheet_name),
                index,
                json_escape(&found.reference),
                found.col,
                found.row,
                found.kind(),
                opt_str_json(found.value.as_deref()),
                opt_str_json(found.formula.as_deref()),
                profile.fingerprint()
            )),
            _ => return Err(unsupported_common(req)),
        };
        Ok(self.xlsx_answer(req, value, provenance, span, deps))
    }
}

/// The standard `unsupported observation` error for a common pair that reached a
/// representation the capability guard admitted but the adapter does not serve.
#[cfg(any(
    feature = "docx",
    feature = "epub",
    feature = "odt",
    feature = "xlsx",
    feature = "pptx"
))]
fn unsupported_common(req: &ObserveRequest) -> Error {
    Error::unsupported_feature(format!(
        "unsupported observation: selector {} with representation {}",
        req.selector.canonical(),
        req.representation.name()
    ))
}

/// Convert a 0-based grid column and row into an A1-style reference (`B7`).
#[cfg(feature = "docx")]
fn a1_ref(col: u32, row: u32) -> String {
    let mut c = col + 1;
    let mut letters: Vec<char> = Vec::new();
    while c > 0 {
        let rem = ((c - 1) % 26) as u8;
        letters.push((b'A' + rem) as char);
        c = (c - 1) / 26;
    }
    letters.reverse();
    format!("{}{}", letters.into_iter().collect::<String>(), row + 1)
}

/// Whether a manifest media type is a reading document (not a binary resource).
#[cfg(feature = "epub")]
fn is_document_media_type(media_type: &str) -> bool {
    media_type == "application/xhtml+xml"
        || media_type == "application/oebps-package+xml"
        || media_type == "application/x-dtbncx+xml"
}

/// The deterministic Stage-C chain beneath a page's `PageContent` node. Must
/// match `ingest::derived_chain` byte-for-byte so node ids coincide.
pub(crate) fn derived_nodes(page: u32, page_content: NodeId) -> (SeedNode, SeedNode, SeedNode) {
    let ops = SeedNode::new(
        NodeKind::ContentOperators,
        0,
        u32_params(page),
        vec![page_content],
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
        vec![page_content],
        "pdf:page-preview",
    );
    (ops, text, preview)
}

/// Parse the numeric fields of a `PagePreview` header.
fn preview_stats(bytes: &[u8]) -> (u64, u64, u64) {
    let rendered = String::from_utf8_lossy(bytes);
    let mut text_bytes = 0u64;
    let mut draw_ops = 0u64;
    let mut path_ops = 0u64;
    for line in rendered.lines() {
        if let Some(v) = line.strip_prefix("text-bytes ") {
            text_bytes = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("draw-ops ") {
            draw_ops = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("path-ops ") {
            path_ops = v.trim().parse().unwrap_or(0);
        }
    }
    (text_bytes, draw_ops, path_ops)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::{Descriptor, ObjectSource};
    use crate::dra::{Op, Program};
    use crate::field::plan;
    use crate::store::FsSeedStore;
    use std::fs;
    use std::path::PathBuf;

    fn temp_root(label: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "vole-observe-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        p
    }

    fn opaque_descriptor(source: &[u8]) -> Vec<u8> {
        let d = Descriptor {
            universe: crate::container::UNIVERSE.to_string(),
            source_format: crate::SOURCE_FORMAT_PDF,
            format_basis: "pdf:observe-test".to_string(),
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

    fn zlib_stored(data: &[u8]) -> Vec<u8> {
        assert!(!data.is_empty());
        let mut out = vec![0x78, 0x01];
        let chunks: Vec<&[u8]> = data.chunks(0xFFFF).collect();
        for (i, chunk) in chunks.iter().enumerate() {
            let final_block = u8::from(i + 1 == chunks.len());
            out.push(final_block);
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
    }

    /// A classic-xref PDF with one page, a lone-Flate content stream containing
    /// `(Hello) Tj`, and (optionally) a large image XObject.
    fn fixture_pdf(with_image: bool) -> Vec<u8> {
        let content = b"BT /F1 12 Tf 72 720 Td (Hello) Tj ET\n";
        let encoded = zlib_stored(content);
        let mut w = PdfBuilder::new();
        w.text("%PDF-1.5\n");
        w.obj(1, b"<< /Type /Catalog /Pages 2 0 R >>");
        w.obj(2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
        if with_image {
            w.obj(
                3,
                b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> /XObject << /Im0 6 0 R >> >> /Contents 4 0 R >>",
            );
        } else {
            w.obj(
                3,
                b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
            );
        }
        w.stream_obj(4, " /Filter /FlateDecode", &encoded);
        w.obj(5, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
        if with_image {
            let image = vec![0x80u8; 256 * 256];
            let image_encoded = zlib_stored(&image);
            w.stream_obj(
                6,
                " /Type /XObject /Subtype /Image /Width 256 /Height 256 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode",
                &image_encoded,
            );
            w.classic_trailer(7, " /Root 1 0 R");
        } else {
            w.classic_trailer(6, " /Root 1 0 R");
        }
        w.buf
    }

    struct Fixture {
        root: PathBuf,
        store: FieldStore,
        field: FieldId,
        source: Vec<u8>,
    }

    impl Fixture {
        fn new(label: &str, with_image: bool) -> Fixture {
            Fixture::from_source(label, fixture_pdf(with_image))
        }

        fn from_source(label: &str, source: Vec<u8>) -> Fixture {
            let root = temp_root(label);
            let mut store = FieldStore::open(&root).unwrap();
            let descriptor = opaque_descriptor(&source);
            let report = ingest::ingest_pdf(&mut store, &descriptor, Limits::DEFAULT).unwrap();
            Fixture {
                root,
                store,
                field: report.field,
                source,
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).ok();
        }
    }

    fn observe_req(
        fx: &mut Fixture,
        selector: Selector,
        representation: Representation,
    ) -> (FieldAnswer, ObserveStats) {
        let req = ObserveRequest::new(selector, representation);
        let (answer, stats, _field) =
            observe(&mut fx.store, &fx.field, &req, Limits::DEFAULT).unwrap();
        (answer, stats)
    }

    /// A classic-xref PDF with **two** incremental revisions: revision 0 defines
    /// objects 1-5, revision 1 appends object 6 and its own xref/trailer with
    /// `/Prev` pointing at revision 0's `startxref`.
    fn two_revision_pdf() -> Vec<u8> {
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
        let xref0 = w.buf.len() as u64;
        w.text("xref\n0 6\n");
        w.raw(b"0000000000 65535 f \n");
        for number in 1..6 {
            let off = w.offset_of(number);
            w.text(&format!("{off:010} 00000 n \n"));
        }
        w.text(&format!(
            "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref0}\n%%EOF\n"
        ));
        // Revision 1: append object 6, then a partial xref + trailer with /Prev.
        w.obj(
            6,
            b"<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>",
        );
        let xref1 = w.buf.len() as u64;
        w.text("xref\n0 1\n0000000000 65535 f \n6 1\n");
        let off6 = w.offset_of(6);
        w.text(&format!("{off6:010} 00000 n \n"));
        w.text(&format!(
            "trailer\n<< /Size 7 /Root 1 0 R /Prev {xref0} >>\nstartxref\n{xref1}\n%%EOF\n"
        ));
        w.buf
    }

    #[test]
    fn revision_lineage_lists_ordered_revisions_and_membership() {
        let mut fx = Fixture::from_source("rev-lineage", two_revision_pdf());
        let (answer, _stats) = observe_req(&mut fx, Selector::Revisions, Representation::Lineage);
        assert_eq!(answer.selector, "revisions");
        assert_eq!(answer.representation, "lineage");
        assert!(!answer.exact);
        let AnswerValue::Json(json) = &answer.value else {
            panic!("revision lineage must be JSON");
        };
        assert!(json.contains("\"count\":2"), "{json}");
        assert!(json.contains("\"header\":\"%PDF-1.5\""), "{json}");
        // The two revisions are ordered and exactly cover the source.
        assert!(json.contains("\"index\":0"), "{json}");
        assert!(json.contains("\"index\":1"), "{json}");
        // Object 6 is defined in revision 1 only; objects 1-5 in revision 0 only.
        let rev0 = json.split("\"index\":0").nth(1).unwrap();
        let rev1 = json.split("\"index\":1").nth(1).unwrap();
        assert!(rev0.contains("\"objects\":[1,2,3,4,5]"), "{rev0}");
        assert!(rev1.contains("\"objects\":[6]"), "{rev1}");
        // Revision 1's trailer /Prev resolves to revision 0's xref anchor.
        assert!(rev1.contains("\"prev\":"), "{rev1}");
        assert!(!rev1.contains("\"prev\":null"), "{rev1}");
        // Deterministic: a second observation is byte-identical.
        let (again, _) = observe_req(&mut fx, Selector::Revisions, Representation::Lineage);
        assert_eq!(again.value, answer.value);
    }

    #[test]
    fn revision_scoped_lineage_matches_one_revision() {
        let mut fx = Fixture::from_source("rev-scoped", two_revision_pdf());
        let (answer, _stats) = observe_req(&mut fx, Selector::Revision(1), Representation::Lineage);
        assert_eq!(answer.selector, "revision:1");
        let AnswerValue::Json(json) = &answer.value else {
            panic!("revision lineage must be JSON");
        };
        assert!(json.contains("\"index\":1"), "{json}");
        assert!(json.contains("\"objects\":[6]"), "{json}");
        assert_eq!(answer.source_span, Some(fx_source_span(&mut fx, 1)));
    }

    /// Revision `n`'s exact `[start, end)` span, from the persisted revision node.
    fn fx_source_span(fx: &mut Fixture, n: u32) -> (u64, u64) {
        let req = ObserveRequest::new(Selector::Revision(n), Representation::ExactBytes);
        let (answer, _stats, _) = observe(&mut fx.store, &fx.field, &req, Limits::DEFAULT).unwrap();
        answer.source_span.unwrap()
    }

    #[test]
    fn revision_lineage_declines_typed_for_non_pdf() {
        let mut fx = Fixture::from_source("rev-decline", b"this is not a document".to_vec());
        for selector in [Selector::Revisions, Selector::Revision(0)] {
            let req = ObserveRequest::new(selector, Representation::Lineage);
            let err = observe(&mut fx.store, &fx.field, &req, Limits::DEFAULT).unwrap_err();
            assert_eq!(err.class(), crate::ErrorClass::UnsupportedFeature);
        }
    }

    /// Phase 20.4: the external lineage layer is typed, separate, and removable.
    ///
    /// It answers only from the attached sidecar (`ExternalMetadata`, never
    /// exact, no seed bytes), it declines typed when absent, and attaching /
    /// querying / removing it leaves every document-derived observation and the
    /// exact closure byte-identical.
    #[test]
    fn external_lineage_is_typed_separate_and_removable() {
        use crate::field::external::{ExternalContext, ExternalLineage, ExternalOrigin};
        let mut fx = Fixture::new("external-ctx", false);
        let req = ObserveRequest::new(Selector::ExternalLineage, Representation::Lineage);
        // A document-derived observation and the exact closure before any attach.
        let plain_before = observe_req(&mut fx, Selector::Page(1), Representation::Text).0;
        let exact_before = Field::open(&fx.store, &fx.field, Limits::DEFAULT)
            .unwrap()
            .materialize_exact(Limits::DEFAULT)
            .unwrap();
        // No context attached: a typed decline, never a guess.
        let err = observe(&mut fx.store, &fx.field, &req, Limits::DEFAULT).unwrap_err();
        assert_eq!(err.class(), crate::ErrorClass::UnsupportedFeature);
        // Attach the external context beside the field.
        let ctx = ExternalContext {
            dataset_id: Some("real100-v1".to_string()),
            lineage: ExternalLineage {
                family: Some("rev-nist-fips-140".to_string()),
                member: Some("nist-pdf-0017".to_string()),
                head: true,
                revision_family: None,
            },
            origin: ExternalOrigin::Harness,
            source: "real100-v1/manifest.tsv".to_string(),
        };
        fx.store.put_external_context(&fx.field, &ctx).unwrap();
        let (answer, stats, _) = observe(&mut fx.store, &fx.field, &req, Limits::DEFAULT).unwrap();
        assert_eq!(answer.basis, Basis::ExternalMetadata);
        assert!(!answer.exact);
        assert_eq!(answer.integrity_scope, IntegrityScope::None);
        assert!(answer.dependency_ids.is_empty());
        assert_eq!(answer.selector, "external-lineage");
        assert_eq!(answer.representation, "lineage");
        let AnswerValue::Json(json) = &answer.value else {
            panic!("external lineage must be JSON");
        };
        assert!(
            json.contains("\"family_id\":\"rev-nist-fips-140\""),
            "{json}"
        );
        assert!(json.contains("\"member_id\":\"nist-pdf-0017\""), "{json}");
        assert!(json.contains("\"is_head\":true"), "{json}");
        assert!(
            answer.provenance.contains("origin=harness"),
            "{}",
            answer.provenance
        );
        // The answer reads no document-derived byte class.
        assert_eq!(stats.descriptor_bytes_read, 0);
        assert_eq!(stats.manifest_bytes_read, 0);
        assert_eq!(stats.index_bytes_read, 0);
        assert_eq!(stats.seed_bytes_read, 0);
        assert_eq!(stats.bytes_read, 0);
        // A plain observation is byte-for-byte unchanged while attached.
        let plain_after = observe_req(&mut fx, Selector::Page(1), Representation::Text).0;
        assert_eq!(plain_before, plain_after);
        // The exact closure is untouched.
        let exact_after = Field::open(&fx.store, &fx.field, Limits::DEFAULT)
            .unwrap()
            .materialize_exact(Limits::DEFAULT)
            .unwrap();
        assert_eq!(exact_before, exact_after);
        assert_eq!(exact_after, fx.source);
        // Remove it: the external observation declines again; the plain
        // observation is still byte-identical.
        assert!(fx.store.clear_external_context(&fx.field).unwrap());
        let err = observe(&mut fx.store, &fx.field, &req, Limits::DEFAULT).unwrap_err();
        assert_eq!(err.class(), crate::ErrorClass::UnsupportedFeature);
        let plain_removed = observe_req(&mut fx, Selector::Page(1), Representation::Text).0;
        assert_eq!(plain_before, plain_removed);
    }

    #[test]
    fn document_full_document_is_exact() {
        let mut fx = Fixture::new("full", false);
        let (answer, stats) =
            observe_req(&mut fx, Selector::Document, Representation::FullDocument);
        assert_eq!(answer.value, AnswerValue::Bytes(fx.source.clone()));
        assert_eq!(answer.basis, Basis::DirectlyObserved);
        assert_eq!(answer.integrity_scope, IntegrityScope::WholeSource);
        assert!(answer.exact);
        assert_eq!(answer.source_span, Some((0, fx.source.len() as u64)));
        assert_eq!(stats.bytes_returned, fx.source.len() as u64);
    }

    #[test]
    fn byte_range_returns_exact_bytes() {
        let mut fx = Fixture::new("range", false);
        let (answer, _) = observe_req(
            &mut fx,
            Selector::ByteRange { offset: 9, len: 8 },
            Representation::ExactBytes,
        );
        assert_eq!(answer.value, AnswerValue::Bytes(fx.source[9..17].to_vec()));
        assert_eq!(answer.source_span, Some((9, 17)));
        assert!(answer.exact);
        assert_eq!(answer.integrity_scope, IntegrityScope::Node);
    }

    #[test]
    fn page_text_is_heuristic_and_nonempty() {
        let mut fx = Fixture::new("text", false);
        let (answer, _) = observe_req(&mut fx, Selector::Page(1), Representation::Text);
        match &answer.value {
            AnswerValue::Text(t) => assert!(t.contains("Hello"), "got {t:?}"),
            other => panic!("expected text, got {other:?}"),
        }
        assert_eq!(answer.basis, Basis::Heuristic);
        assert!(!answer.exact);
    }

    /// The *procedural/seed closure* of a page-structure observation excludes the
    /// image seed node. This is a statement about **seed reads only**: the
    /// descriptor blob is charged separately in `descriptor_bytes_read`, and
    /// `bytes_read` is the sum of all four classes, so this must never be read as
    /// "the OS avoided reading the whole document" (review fix #1/#2).
    #[test]
    fn page_structure_procedural_closure_excludes_image_seed_node() {
        let mut fx = Fixture::new("structure", true);
        let (answer, stats) = observe_req(&mut fx, Selector::Page(1), Representation::Structure);
        match &answer.value {
            AnswerValue::Json(j) => {
                assert!(j.contains("\"page\":1"), "got {j}");
                assert!(j.contains("\"content_streams\":[4]"), "got {j}");
            }
            other => panic!("expected json, got {other:?}"),
        }
        assert_eq!(answer.basis, Basis::DeterministicallyDerived);
        assert!(!answer.exact);
        // Seed closure: the 64 KiB image payload's seed node is never fetched.
        assert!(
            stats.seed_bytes_read < 16 * 1024,
            "structure observation read {} seed bytes (expected < 16384)",
            stats.seed_bytes_read
        );
        // The honest total *does* include the descriptor blob, which a narrow
        // observation necessarily read to open the field.
        assert!(
            stats.descriptor_bytes_read > 0,
            "the descriptor read must be accounted, not hidden"
        );
        assert!(
            stats.bytes_read >= stats.descriptor_bytes_read,
            "bytes_read {} must include descriptor_bytes_read {}",
            stats.bytes_read,
            stats.descriptor_bytes_read
        );
        assert_eq!(
            stats.bytes_read,
            stats
                .descriptor_bytes_read
                .saturating_add(stats.manifest_bytes_read)
                .saturating_add(stats.index_bytes_read)
                .saturating_add(stats.seed_bytes_read),
            "bytes_read must be the exact sum of the four physical classes"
        );
    }

    #[test]
    fn plan_is_pure_and_deterministic() {
        let fx = Fixture::new("plan", false);
        let req = ObserveRequest::new(Selector::Page(1), Representation::Text);
        let field = Field::open(&fx.store, &fx.field, Limits::DEFAULT).unwrap();
        let before = fx.store.seeds().list_nodes().unwrap().len();
        let a = plan::plan(field.manifest(), &fx.store, &req).unwrap();
        let b = plan::plan(field.manifest(), &fx.store, &req).unwrap();
        assert_eq!(a, b);
        let after = fx.store.seeds().list_nodes().unwrap().len();
        assert_eq!(before, after, "plan must not add seed nodes");
    }

    #[test]
    fn explain_json_keys_and_unsupported_pair() {
        use crate::field::explain;
        let mut fx = Fixture::new("explain", false);
        let req = ObserveRequest::new(Selector::Document, Representation::FullDocument);
        let (plan, actual) =
            explain::explain_analyze(&mut fx.store, &fx.field, &req, Limits::DEFAULT).unwrap();
        assert_eq!(
            plan.json,
            "{\"selector\":\"document\",\"representation\":\"full\",\"format\":\"pdf\",\"adapter\":\"pdf\",\"capability\":\"native\",\"index_route\":\"hier-index\",\"shape\":\"full_materialize\",\"index_reads\":0,\"required_nodes\":1,\"will_materialize\":[\"DocumentExact\"],\"will_not_materialize\":[]}"
        );
        let actual_json = actual.to_json();
        let mut keys = top_level_keys(&actual_json);
        keys.sort();
        let mut expected = vec![
            "adapter",
            "basis",
            "bytes_read",
            "bytes_returned",
            "deepened",
            "descriptor_bytes_read",
            "descriptor_read_mode",
            "exact",
            "format",
            "index_bytes_read",
            "index_nodes_read",
            "inverse_work_units",
            "manifest_bytes_read",
            "member_decodes",
            "nodes_id_shared",
            "seed_bytes_read",
            "seed_nodes_fetched",
            "seed_nodes_materialized",
            "shared_resource_ids",
            "wall_micros",
            "whole_source_materialized",
            "xml_parses",
        ];
        expected.sort_unstable();
        assert_eq!(keys, expected, "actual json keys: {actual_json}");

        // An unsupported selector/representation pair is typed, never guessed.
        let bad = ObserveRequest::new(Selector::Document, Representation::Text);
        let err = observe(&mut fx.store, &fx.field, &bad, Limits::DEFAULT).unwrap_err();
        assert_eq!(err.class(), crate::ErrorClass::UnsupportedFeature);
        let field = Field::open(&fx.store, &fx.field, Limits::DEFAULT).unwrap();
        let perr = plan::plan(field.manifest(), &fx.store, &bad).unwrap_err();
        assert_eq!(perr.class(), crate::ErrorClass::UnsupportedFeature);
    }

    #[test]
    fn budget_yields_resource_limit_not_truncation() {
        let mut fx = Fixture::new("budget", false);
        let req = ObserveRequest {
            selector: Selector::Document,
            representation: Representation::FullDocument,
            budget: ObserveBudget {
                max_output_bytes: 4,
                max_nodes: 1 << 20,
            },
            use_cache: true,
        };
        let err = observe(&mut fx.store, &fx.field, &req, Limits::DEFAULT).unwrap_err();
        assert_eq!(err.class(), crate::ErrorClass::ResourceLimit);
    }

    #[test]
    fn find_returns_matching_line() {
        let mut fx = Fixture::new("find", false);
        let (answer, _) = observe_req(
            &mut fx,
            Selector::TextMatch("Hello".to_string()),
            Representation::Text,
        );
        match &answer.value {
            AnswerValue::Json(j) => {
                assert!(j.contains("\"page\":1"), "got {j}");
                assert!(j.contains("Hello"), "got {j}");
            }
            other => panic!("expected json, got {other:?}"),
        }
        assert_eq!(answer.basis, Basis::Heuristic);
    }

    #[test]
    fn preview_is_deterministic() {
        let mut fx = Fixture::new("preview", false);
        let (a, _) = observe_req(&mut fx, Selector::Page(1), Representation::Preview);
        let (b, _) = observe_req(&mut fx, Selector::Page(1), Representation::Preview);
        assert_eq!(a.value, b.value);
        match &a.value {
            AnswerValue::Bytes(bytes) => {
                assert!(String::from_utf8_lossy(bytes).contains("VOLE-PREVIEW v1"))
            }
            other => panic!("expected preview bytes, got {other:?}"),
        }
    }

    #[test]
    fn decoded_and_operators_streams_resolve() {
        let mut fx = Fixture::new("decoded", false);
        let (decoded, _) = observe_req(&mut fx, Selector::Stream(4), Representation::DecodedBytes);
        match &decoded.value {
            AnswerValue::Bytes(b) => {
                assert_eq!(b, b"BT /F1 12 Tf 72 720 Td (Hello) Tj ET\n");
            }
            other => panic!("expected bytes, got {other:?}"),
        }
        assert_eq!(decoded.basis, Basis::DeterministicallyDerived);
        let (ops, _) = observe_req(&mut fx, Selector::Stream(4), Representation::Operators);
        assert!(matches!(ops.value, AnswerValue::Bytes(ref b) if !b.is_empty()));
    }

    /// Every physical byte class is charged, and `bytes_read` is their exact sum.
    #[test]
    fn observation_byte_classes_sum_and_are_all_charged() {
        let mut fx = Fixture::new("io-sum", false);
        let (_, stats) = observe_req(&mut fx, Selector::Page(1), Representation::Structure);
        assert!(
            stats.descriptor_bytes_read > 0,
            "descriptor bytes: {stats:?}"
        );
        assert!(stats.manifest_bytes_read > 0, "manifest bytes: {stats:?}");
        assert!(stats.index_bytes_read > 0, "index bytes: {stats:?}");
        assert!(stats.seed_bytes_read > 0, "seed bytes: {stats:?}");
        assert_eq!(
            stats.bytes_read,
            stats
                .descriptor_bytes_read
                .saturating_add(stats.manifest_bytes_read)
                .saturating_add(stats.index_bytes_read)
                .saturating_add(stats.seed_bytes_read),
            "bytes_read must equal the class sum: {stats:?}"
        );
    }

    /// `explain_analyze` must open the descriptor exactly once: the plan and the
    /// evaluation share one `Field`, and a Stage-C promotion no longer re-reads it
    /// (review fix #2).
    #[test]
    fn explain_analyze_opens_the_descriptor_once() {
        use crate::field::explain;
        let mut fx = Fixture::new("opens-once", false);

        // A metadata observation does no promotion; one open.
        let req = ObserveRequest::new(Selector::Document, Representation::Metadata);
        let before = fx.store.io().descriptor_reads();
        let (_, actual) =
            explain::explain_analyze(&mut fx.store, &fx.field, &req, Limits::DEFAULT).unwrap();
        assert_eq!(
            fx.store.io().descriptor_reads() - before,
            1,
            "explain_analyze must open the descriptor exactly once"
        );
        assert!(
            actual.stats.descriptor_bytes_read > 0,
            "the single open must still be accounted: {:?}",
            actual.stats
        );

        // A cold page observation *does* promote (Stage C); even then the
        // descriptor is read once, because promotion reuses the open manifest.
        let page = ObserveRequest::new(Selector::Page(1), Representation::Text);
        let before = fx.store.io().descriptor_reads();
        let (_, page_actual) =
            explain::explain_analyze(&mut fx.store, &fx.field, &page, Limits::DEFAULT).unwrap();
        assert_eq!(
            fx.store.io().descriptor_reads() - before,
            1,
            "a cold promotion must not re-open the field for its manifest"
        );
        assert!(page_actual.stats.deepened, "the cold page must promote");
        assert!(page_actual.stats.descriptor_bytes_read > 0);
    }

    /// The headline 15.2 gate: an observation served by a resident session must
    /// return the **same `FieldAnswer`** as a cold, single-shot observation.
    ///
    /// Equivalence is defined over `FieldAnswer` (`value`, `basis`, `selector`,
    /// `representation`, `source_span`, `provenance`, `dependency_ids`,
    /// `integrity_scope`, `exact`) — every field deterministic and independent of
    /// any cache. It is deliberately **not** the stats-bearing JSON envelope: the
    /// session hoists physical reads (the one-time descriptor/manifest open), so
    /// `ObserveStats` diverges by construction (design §0). Faking counters to
    /// force envelope equality would be dishonest accounting.
    #[test]
    fn session_answers_equal_cold_process_answers() {
        use crate::field::session::{DocumentFieldSession, SessionOptions};
        let mut fx = Fixture::new("session-eq", true);
        let reqs = [
            (Selector::Page(1), Representation::Text),
            (Selector::Page(1), Representation::Structure),
            (Selector::Page(1), Representation::Preview),
            (Selector::Stream(4), Representation::DecodedBytes),
            (Selector::Metadata, Representation::Metadata),
        ];
        let mut session =
            DocumentFieldSession::open(&fx.root, &fx.field.to_hex(), SessionOptions::default())
                .unwrap();
        for (i, (sel, rep)) in reqs.iter().enumerate() {
            let req = ObserveRequest::new(sel.clone(), *rep);
            let (cold, _cs, _ci) =
                observe(&mut fx.store, &fx.field, &req, Limits::DEFAULT).unwrap();
            let (warm, _ws, _wi) = session.observe(&req, Limits::DEFAULT).unwrap();
            assert_eq!(cold, warm, "FieldAnswer differs for request {i}");
        }
    }

    /// The resident session opens the descriptor (and manifest) exactly once: the
    /// one-time open is attributed to the first observation, and every later
    /// observation reports zero descriptor and manifest bytes.
    #[test]
    fn session_reads_descriptor_once() {
        use crate::field::session::{DocumentFieldSession, SessionOptions};
        let fx = Fixture::new("session-open-once", false);
        // A separate open of the same field over `fx.store` gives the descriptor
        // length the session's own one-time open must charge.
        let field = Field::open(&fx.store, &fx.field, Limits::DEFAULT).unwrap();
        let descriptor_len = field.descriptor_bytes().len() as u64;
        let mut session =
            DocumentFieldSession::open(&fx.root, &fx.field.to_hex(), SessionOptions::default())
                .unwrap();
        let req = ObserveRequest::new(Selector::Page(1), Representation::Text);
        let (_, s1, _) = session.observe(&req, Limits::DEFAULT).unwrap();
        let (_, s2, _) = session.observe(&req, Limits::DEFAULT).unwrap();
        let (_, s3, _) = session.observe(&req, Limits::DEFAULT).unwrap();
        assert_eq!(
            s1.descriptor_bytes_read, descriptor_len,
            "the first observation must carry exactly the one-time open: {s1:?}"
        );
        assert!(
            s1.manifest_bytes_read > 0,
            "the one-time open must include the manifest: {s1:?}"
        );
        assert_eq!(
            s2.descriptor_bytes_read, 0,
            "the second observation re-read the descriptor: {s2:?}"
        );
        assert_eq!(
            s3.descriptor_bytes_read, 0,
            "the third observation re-read the descriptor: {s3:?}"
        );
        assert_eq!(
            s2.manifest_bytes_read, 0,
            "the second observation re-read the manifest: {s2:?}"
        );
        assert_eq!(
            s3.manifest_bytes_read, 0,
            "the third observation re-read the manifest: {s3:?}"
        );
    }

    /// The resident session's hoisted probe returns the *same* `FieldAnswer` as
    /// the cold [`narrow_probe`] short-circuit, and actually takes it: against a
    /// warm derived cache both report `descriptor_read_mode == Partial` with no
    /// fresh descriptor read.
    #[test]
    fn session_probe_matches_narrow_probe_short_circuit() {
        use crate::field::session::{DocumentFieldSession, SessionOptions};
        let mut fx = Fixture::new("session-probe", false);
        let req = ObserveRequest::new(Selector::Page(1), Representation::Text);
        // The first cold observation materializes the derived chain and writes it
        // to the disposable cache; the second takes `narrow_probe`'s short-circuit.
        let (warm, _, _) = observe(&mut fx.store, &fx.field, &req, Limits::DEFAULT).unwrap();
        let (probe_answer, probe_stats, _) =
            observe(&mut fx.store, &fx.field, &req, Limits::DEFAULT).unwrap();
        assert_eq!(probe_answer, warm);
        assert_eq!(
            probe_stats.descriptor_read_mode,
            DescriptorReadMode::Partial,
            "the cold short-circuit did not fire: {probe_stats:?}"
        );
        assert_eq!(probe_stats.descriptor_bytes_read, 0, "{probe_stats:?}");

        // The resident session takes the *same* short-circuit — from the manifest
        // parsed at open and an index store kept open across observations, so a
        // single process need not re-read either.
        let mut session =
            DocumentFieldSession::open(&fx.root, &fx.field.to_hex(), SessionOptions::default())
                .unwrap();
        let (sess_answer, sess_stats, _) = session.observe(&req, Limits::DEFAULT).unwrap();
        assert_eq!(
            sess_answer, probe_answer,
            "session probe answer differs from the cold narrow_probe"
        );
        assert_eq!(
            sess_stats.descriptor_read_mode,
            DescriptorReadMode::Partial,
            "the session did not use the probe short-circuit: {sess_stats:?}"
        );
    }

    /// A seed store that forbids enumeration and bounds fetches. Putting it on the
    /// observation path proves a `Stream(n)+DecodedBytes` resolves through the
    /// hierarchical index, never by scanning the store (review fix #3).
    struct BoundedSeedStore {
        inner: FsSeedStore,
        fetches: Cell<u64>,
        limit: u64,
    }

    impl SeedStore for BoundedSeedStore {
        fn put_node(&mut self, canonical: &[u8]) -> Result<NodeId> {
            self.inner.put_node(canonical)
        }

        fn get_node(&self, id: &NodeId) -> Result<Vec<u8>> {
            let n = self.fetches.get() + 1;
            assert!(
                n <= self.limit,
                "get_node #{n} exceeds the {}-fetch bound: the store was enumerated",
                self.limit
            );
            self.fetches.set(n);
            self.inner.get_node(id)
        }

        fn get_node_range(&self, id: &NodeId, offset: u64, len: u64) -> Result<Vec<u8>> {
            let n = self.fetches.get() + 1;
            assert!(
                n <= self.limit,
                "get_node_range #{n} exceeds the {}-fetch bound: the store was enumerated",
                self.limit
            );
            self.fetches.set(n);
            self.inner.get_node_range(id, offset, len)
        }

        fn contains_node(&self, id: &NodeId) -> Result<bool> {
            self.inner.contains_node(id)
        }

        fn list_nodes(&self) -> Result<Vec<(NodeId, u64)>> {
            panic!("an observation must never enumerate the seed store")
        }
    }

    #[test]
    fn decoded_stream_resolves_without_enumerating_the_store() {
        let mut fx = Fixture::new("no-scan", true);
        // Many unrelated decoy nodes: a whole-store scan would fetch them all.
        for i in 0..256u32 {
            let decoy = SeedNode::new(
                NodeKind::PdfObject,
                1,
                u32_params(10_000 + i),
                Vec::new(),
                "decoy",
            );
            fx.store
                .seeds_mut()
                .put_node(&decoy.encode_canonical())
                .unwrap();
        }
        let total = fx.store.seeds().list_nodes().unwrap().len() as u64;
        assert!(total > 200, "expected many seed nodes, got {total}");

        let field = Field::open(&fx.store, &fx.field, Limits::DEFAULT).unwrap();
        let io = fx.store.io().handle();
        let seeds = CountingSeedStore::new(BoundedSeedStore {
            inner: FsSeedStore::open_with_io(fx.store.root(), io.handle()).unwrap(),
            fetches: Cell::new(0),
            limit: 8,
        });
        let istore = FsIndexStore::open_with_io(fx.store.root(), io.handle()).unwrap();
        let req = ObserveRequest::new(Selector::Stream(4), Representation::DecodedBytes);
        let (answer, stats, _) = observe_with_stores(
            &mut fx.store,
            FieldView::from_field(&field),
            &req,
            Limits::DEFAULT,
            Instant::now(),
            seeds,
            istore,
        )
        .unwrap();
        match &answer.value {
            AnswerValue::Bytes(b) => {
                assert_eq!(b, b"BT /F1 12 Tf 72 720 Td (Hello) Tj ET\n")
            }
            other => panic!("expected bytes, got {other:?}"),
        }
        assert!(
            stats.seed_nodes_fetched <= 8,
            "fetched {} seed nodes for one decoded stream",
            stats.seed_nodes_fetched
        );
        assert!(
            stats.seed_nodes_fetched < total,
            "must not enumerate the {total}-node store; fetched {}",
            stats.seed_nodes_fetched
        );
    }

    /// Extract top-level object keys from a flat JSON object (test helper).
    pub(super) fn top_level_keys(json: &str) -> Vec<String> {
        let b = json.as_bytes();
        let mut keys = Vec::new();
        let mut depth = 0i32;
        let mut in_str = false;
        let mut esc = false;
        let mut i = 0;
        while i < b.len() {
            let c = b[i];
            if in_str {
                if esc {
                    esc = false;
                } else if c == b'\\' {
                    esc = true;
                } else if c == b'"' {
                    in_str = false;
                    if depth == 1 && b.get(i + 1) == Some(&b':') {
                        let mut j = i;
                        while j > 0 {
                            j -= 1;
                            if b[j] == b'"' {
                                keys.push(String::from_utf8_lossy(&b[j + 1..i]).into_owned());
                                break;
                            }
                        }
                    }
                }
            } else {
                match c {
                    b'"' => in_str = true,
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => depth -= 1,
                    _ => {}
                }
            }
            i += 1;
        }
        keys
    }
}
