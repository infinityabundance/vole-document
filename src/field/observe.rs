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
use crate::error::{Error, Result};
use crate::field::cache::DerivedCache;
use crate::field::dag::{self, EvalBudget, ReuseStats, SourceServer};
use crate::field::document_format::DocumentFormat;
#[cfg(feature = "docx")]
use crate::field::index::SEL_DOCX_MODEL;
#[cfg(feature = "epub")]
use crate::field::index::SEL_EPUB_MODEL;
#[cfg(feature = "odt")]
use crate::field::index::SEL_ODT_MODEL;
#[cfg(feature = "opc")]
use crate::field::index::SEL_OPC_MODEL;
use crate::field::index::{
    FsIndexStore, IndexEntry, SEL_OBJECT, SEL_PACKAGE_MEMBER_DECODED, SEL_PACKAGE_MEMBER_RAW,
    SEL_PAGE, SEL_REVISION, SEL_STREAM, SEL_STREAM_DECODED, SelectorKey, lookup,
};
use crate::field::ingest;
use crate::field::manifest::FieldRoot;
use crate::field::node::{NodeKind, SeedNode, read_u32_params, span_params, u32_params};
use crate::field::partial::{PartialDescriptor, PartialLoad};
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
                istore,
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
    use Representation as R;
    if !req.use_cache {
        return Ok(NarrowProbe::NotEligible);
    }
    // The short-circuit is a further step of the seek-based *partial* lane: on a
    // backend with no partial descriptor (EntropyFS) the honest label would be
    // `full`, so leave that path unchanged.
    if !store.supports_partial_descriptor() {
        return Ok(NarrowProbe::NotEligible);
    }
    let cacheable = matches!(
        (&req.selector, req.representation),
        (Selector::Page(_), R::Text | R::Preview | R::Structure)
            | (Selector::Stream(_), R::DecodedBytes | R::Operators)
    );
    if !cacheable {
        return Ok(NarrowProbe::NotEligible);
    }

    let io_before = store.io().snapshot();
    let manifest = store.get_field(id)?;
    let mut prefetched = PrefetchedIndex::default();
    if !manifest.has_index() {
        // Nothing to resolve from; let the normal path produce its typed error.
        let base_io = io_before.delta(&store.io().snapshot());
        return Ok(NarrowProbe::Probed {
            hit: false,
            manifest: Box::new(manifest),
            carry: ProbeCarry {
                base_io,
                prefetched,
                output: None,
            },
        });
    }

    let istore = FsIndexStore::open_with_io(store.root(), store.io().handle())?;
    let root = NodeId::from_bytes(manifest.index_root);
    let seeds = store.seed_substrate();

    // Compute the deterministic target `(id, max_output_bytes)`.
    let target: Option<(NodeId, u64)> = match (&req.selector, req.representation) {
        (Selector::Page(page), R::Text | R::Preview | R::Structure) => {
            let key = SelectorKey::new(SEL_PAGE, *page);
            let entries = lookup(&istore, &root, &key)?;
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
            let enc = lookup(&istore, &root, &enc_key)?;
            prefetched.insert(enc_key, enc.clone());
            let dec_key = SelectorKey::new(SEL_STREAM_DECODED, *object);
            let dec = lookup(&istore, &root, &dec_key)?;
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
        manifest: Box::new(manifest),
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
    observe_view(store, FieldView::from_field(field), req, limits, started)
}

/// [`observe_with_field`] with the resident typed-model memo and explicit
/// one-time open attribution. `open_io` is attributed to this observation only
/// (the session passes the field-open bytes on its first call, default after).
///
/// The field is already open, so this never runs `narrow_probe` and never opens
/// an [`OpenedField`]: it feeds the ordinary evaluation core a Full view.
pub(crate) fn observe_with_field_memo(
    store: &mut FieldStore,
    field: &Field,
    open_io: IoSnapshot,
    req: &ObserveRequest,
    limits: Limits,
    models: ModelMemo,
) -> Result<(FieldAnswer, ObserveStats, FieldId)> {
    let started = Instant::now();
    let (seeds, istore) = open_sub_stores(store)?;
    let mut view = FieldView::from_field(field);
    view.open_io = open_io;
    observe_with_stores_pre(
        store,
        view,
        req,
        limits,
        started,
        seeds,
        istore,
        ProbeCarry::default(),
        models,
    )
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
        istore,
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
        istore,
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
    istore: FsIndexStore,
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
    let cache = DerivedCache::open(store.root().join("cache"))?;
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

    let answer = ctx.dispatch(req)?;
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
    istore: FsIndexStore,
    /// Index entries the cache-first probe already resolved, keyed by selector.
    prefetched: PrefetchedIndex,
    /// The target's cache bytes, already read and integrity-checked by the probe.
    prefetched_output: Option<(NodeId, Vec<u8>)>,
    limits: Limits,
    budget: EvalBudget,
    stats: ObserveStats,
    use_cache: bool,
    cache: DerivedCache,
    reuse: ReuseStats,
    /// Resident typed-model memo (Phase 15.2); consulted only when `use_cache`.
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

#[cfg(feature = "docx")]
fn opt_u8_json(v: Option<u8>) -> String {
    match v {
        Some(n) => n.to_string(),
        None => "null".to_string(),
    }
}

#[cfg(feature = "docx")]
fn opt_str_json(v: Option<&str>) -> String {
    match v {
        Some(s) => format!("\"{}\"", json_escape(s)),
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
            | NodeKind::OdtContent => {
                self.stats.xml_parses = self.stats.xml_parses.saturating_add(1);
            }
            _ => {}
        }
        let depth = node.limits.max_depth;
        if self.use_cache {
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
                &mut self.cache,
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
        }
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
                lookup(&self.istore, &root, &key)?
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
        let node = SeedNode::new(
            NodeKind::SourceSlice,
            len,
            span_params(offset, len),
            Vec::new(),
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
        let decoded = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&encoded, cap)
            .map_err(|e| {
                Error::unsupported_feature(format!(
                    "stream {object} has no recovered decoded representation: {:?}",
                    e.status
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

/// The standard `unsupported observation` error for a common pair that reached a
/// representation the capability guard admitted but the adapter does not serve.
#[cfg(any(feature = "docx", feature = "epub", feature = "odt"))]
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
            let root = temp_root(label);
            let mut store = FieldStore::open(&root).unwrap();
            let source = fixture_pdf(with_image);
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
