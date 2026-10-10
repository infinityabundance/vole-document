//! The deterministic observation planner (Phase 11.5–11.6, ADR-0026, DEC-9).
//!
//! [`plan`] is a pure function of validated metadata: it reads the field manifest
//! and (read-only) the hierarchical index, decides which frontier the observation
//! will select, and reports what will and will not be materialized. It never
//! materializes seed nodes and never writes to the store, so calling it cannot
//! change the outcome of a subsequent [`crate::field::observe::observe`].
//!
//! Correctness is a precondition, not a cost. Shapes are named, not scored here:
//! the reference implementation has exactly one admissible shape per supported
//! selector/representation pair, and an unsupported pair is a typed error.

use crate::error::{Error, Result};
use crate::field::FieldStore;
use crate::field::document_format::DocumentFormat;
use crate::field::index::{FsIndexStore, IndexEntry, SEL_PAGE, SelectorKey, lookup};
use crate::field::manifest::FieldRoot;
use crate::store::NodeId;

use super::observe::{ObserveRequest, Representation, Selector, derived_nodes};

/// The selected execution shape of an observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanShape {
    /// Answered purely from validated metadata (no node materialization).
    CachedObservation,
    /// Resolved through the hierarchical index to one exact node.
    IndexLookup,
    /// A `SourceSlice` node synthesized on the fly from the descriptor program.
    SourceSlice,
    /// One or more existing seed nodes are materialized as-is.
    NodeMaterialize,
    /// A Stage-C promotion runs first, then the derived node is materialized.
    DeepenThenObserve,
    /// The whole source is materialized (`materialize_exact`).
    FullMaterialize,
}

impl PlanShape {
    /// Stable lower-case name (used in EXPLAIN JSON).
    pub const fn name(self) -> &'static str {
        match self {
            PlanShape::CachedObservation => "cached_observation",
            PlanShape::IndexLookup => "index_lookup",
            PlanShape::SourceSlice => "source_slice",
            PlanShape::NodeMaterialize => "node_materialize",
            PlanShape::DeepenThenObserve => "deepen_then_observe",
            PlanShape::FullMaterialize => "full_materialize",
        }
    }
}

/// The plan for one observation. `required_nodes` and the materialization lists
/// are deterministic estimates derived from the selector and index shape; they do
/// not require loading node bodies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservePlan {
    /// The selected shape.
    pub shape: PlanShape,
    /// Index descents the observation will perform.
    pub index_reads: u64,
    /// Seed nodes the observation is expected to fetch.
    pub required_nodes: u64,
    /// Node kinds / byte classes that will be materialized.
    pub will_materialize: Vec<String>,
    /// Node kinds / byte classes that will not be materialized.
    pub will_not_materialize: Vec<String>,
}

/// Plan an observation. Pure: no materialization and no writes.
pub fn plan(manifest: &FieldRoot, store: &FieldStore, req: &ObserveRequest) -> Result<ObservePlan> {
    // Common (format-neutral) selectors plan through the detected format's
    // capability set, so an unsupported pair fails closed here exactly as it does
    // at evaluation time (Phase 12.7).
    if req.selector.is_common() {
        return common_plan(manifest, req);
    }
    use Representation as R;
    match (&req.selector, req.representation) {
        (Selector::Document, R::FullDocument | R::ExactBytes) => Ok(ObservePlan {
            shape: PlanShape::FullMaterialize,
            index_reads: 0,
            required_nodes: 1,
            will_materialize: kinds(&["DocumentExact"]),
            will_not_materialize: Vec::new(),
        }),
        (Selector::Document, R::Metadata) => Ok(ObservePlan {
            shape: PlanShape::CachedObservation,
            index_reads: 0,
            required_nodes: 0,
            will_materialize: Vec::new(),
            will_not_materialize: kinds(&["whole-document", "seed-nodes"]),
        }),
        (Selector::ByteRange { .. }, R::ExactBytes) => Ok(ObservePlan {
            shape: PlanShape::SourceSlice,
            index_reads: 0,
            required_nodes: 0,
            will_materialize: kinds(&["SourceSlice"]),
            will_not_materialize: kinds(&["whole-document"]),
        }),
        (Selector::Object(_), R::ExactBytes | R::EncodedBytes) => Ok(index_plan("PdfObject")),
        (Selector::Revision(_), R::ExactBytes) => Ok(index_plan("PdfRevision")),
        (Selector::Revisions, R::Lineage) | (Selector::Revision(_), R::Lineage) => {
            Ok(index_plan("PdfRevisionLineage"))
        }
        (Selector::ExternalLineage, R::Lineage) => Ok(ObservePlan {
            // The external context is answered from its own sidecar; no seed
            // node, descriptor byte, or index entry is read.
            shape: PlanShape::CachedObservation,
            index_reads: 0,
            required_nodes: 0,
            will_materialize: Vec::new(),
            will_not_materialize: kinds(&["seed-nodes", "descriptor"]),
        }),
        (Selector::Member(_), R::EncodedBytes) => Ok(ObservePlan {
            shape: PlanShape::IndexLookup,
            index_reads: 1,
            required_nodes: 1,
            will_materialize: kinds(&["PackageMemberRaw"]),
            will_not_materialize: kinds(&["other-members", "whole-document"]),
        }),
        (Selector::Member(_), R::DecodedBytes) => Ok(ObservePlan {
            shape: PlanShape::NodeMaterialize,
            index_reads: 1,
            required_nodes: 2,
            will_materialize: kinds(&["PackageMemberDecoded", "PackageMemberRaw"]),
            will_not_materialize: kinds(&["other-members", "whole-document"]),
        }),
        (Selector::PackagePart(_), R::Metadata) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 1,
            required_nodes: 2,
            will_materialize: kinds(&["PackageOpcModel"]),
            will_not_materialize: kinds(&["other-parts", "whole-document"]),
        }),
        (Selector::PackagePart(_), R::ExactBytes) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 2,
            required_nodes: 3,
            will_materialize: kinds(&["PackageOpcModel", "PackageMemberRaw"]),
            will_not_materialize: kinds(&["other-parts", "whole-document"]),
        }),
        (Selector::PackagePart(_), R::DecodedBytes) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 2,
            required_nodes: 4,
            will_materialize: kinds(&[
                "PackageOpcModel",
                "PackageMemberDecoded",
                "PackageMemberRaw",
            ]),
            will_not_materialize: kinds(&["other-parts", "whole-document"]),
        }),
        (Selector::Relationship(_), R::Metadata) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 1,
            required_nodes: 2,
            will_materialize: kinds(&["PackageOpcModel"]),
            will_not_materialize: kinds(&["other-relationships", "whole-document"]),
        }),
        (Selector::Relationship(_), R::ExactBytes) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 2,
            required_nodes: 3,
            will_materialize: kinds(&["PackageOpcModel", "PackageMemberRaw"]),
            will_not_materialize: kinds(&["external-targets", "whole-document"]),
        }),
        (Selector::Relationship(_), R::DecodedBytes) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 2,
            required_nodes: 4,
            will_materialize: kinds(&[
                "PackageOpcModel",
                "PackageMemberDecoded",
                "PackageMemberRaw",
            ]),
            will_not_materialize: kinds(&["external-targets", "whole-document"]),
        }),
        (Selector::Stream(_), R::EncodedBytes) => Ok(index_plan("PdfStreamEncoded")),
        (Selector::Stream(_), R::DecodedBytes) => Ok(ObservePlan {
            shape: PlanShape::NodeMaterialize,
            index_reads: 1,
            required_nodes: 2,
            will_materialize: kinds(&["PdfStreamDecoded", "PdfStreamEncoded"]),
            will_not_materialize: kinds(&["images", "whole-document"]),
        }),
        (Selector::Stream(_), R::Operators) => Ok(ObservePlan {
            shape: PlanShape::NodeMaterialize,
            index_reads: 1,
            required_nodes: 2,
            will_materialize: kinds(&["PdfStreamDecoded", "ContentOperators"]),
            will_not_materialize: kinds(&["images", "whole-document"]),
        }),
        (Selector::Page(n), R::Text) => page_plan(manifest, store, *n, PageRepr::Text),
        (Selector::Page(n), R::Preview) => page_plan(manifest, store, *n, PageRepr::Preview),
        (Selector::Page(n), R::Structure) => page_plan(manifest, store, *n, PageRepr::Structure),
        (Selector::TextMatch(_), R::Text) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 1,
            required_nodes: 3,
            will_materialize: kinds(&["PageContent", "ContentOperators", "TextRuns"]),
            will_not_materialize: kinds(&["images", "xobjects", "whole-document"]),
        }),
        #[cfg(feature = "docx")]
        (Selector::DocxStory { .. }, R::Text | R::Structure | R::Metadata)
        | (Selector::DocxParagraph { .. }, R::Text | R::Metadata)
        | (Selector::DocxTable { .. }, R::Text | R::Metadata)
        | (Selector::DocxCell { .. }, R::Text | R::Metadata)
        | (Selector::DocxFind { .. }, R::Text) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 3,
            required_nodes: 4,
            will_materialize: kinds(&[
                "DocxModel",
                "DocxStory",
                "PackageMemberDecoded",
                "PackageMemberRaw",
            ]),
            will_not_materialize: kinds(&["other-stories", "whole-document"]),
        }),
        #[cfg(feature = "epub")]
        (Selector::EpubPackage, R::Metadata | R::Structure)
        | (Selector::EpubNav, R::Metadata | R::Structure)
        | (Selector::EpubNavNode { .. }, R::Metadata) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 1,
            required_nodes: 3,
            will_materialize: kinds(&["EpubModel", "PackageMemberDecoded", "PackageMemberRaw"]),
            will_not_materialize: kinds(&["other-resources", "whole-document"]),
        }),
        #[cfg(feature = "epub")]
        (Selector::EpubManifestItem { .. }, R::Metadata)
        | (Selector::EpubSpineItem { .. }, R::Metadata)
        | (Selector::EpubResource(_), R::Metadata) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 1,
            required_nodes: 2,
            will_materialize: kinds(&["EpubModel"]),
            will_not_materialize: kinds(&["external-targets", "whole-document"]),
        }),
        #[cfg(feature = "epub")]
        (Selector::EpubSpineItem { .. }, R::Text | R::Structure | R::Preview)
        | (Selector::EpubBlock { .. }, R::Text | R::Metadata | R::Structure)
        | (Selector::EpubCell { .. }, R::Text | R::Metadata)
        | (Selector::EpubLink { .. }, R::Metadata)
        | (Selector::EpubFind { .. }, R::Text) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 2,
            required_nodes: 4,
            will_materialize: kinds(&[
                "EpubModel",
                "EpubContent",
                "PackageMemberDecoded",
                "PackageMemberRaw",
            ]),
            will_not_materialize: kinds(&["other-spine-items", "whole-document"]),
        }),
        #[cfg(feature = "epub")]
        (Selector::EpubManifestItem { .. }, R::ExactBytes | R::DecodedBytes)
        | (Selector::EpubSpineItem { .. }, R::ExactBytes | R::DecodedBytes)
        | (Selector::EpubResource(_), R::ExactBytes | R::DecodedBytes) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 2,
            required_nodes: 3,
            will_materialize: kinds(&["EpubModel", "PackageMemberRaw", "PackageMemberDecoded"]),
            will_not_materialize: kinds(&["external-targets", "whole-document"]),
        }),
        #[cfg(feature = "odt")]
        (Selector::OdtParagraph { .. }, R::Text | R::Metadata)
        | (Selector::OdtHeading { .. }, R::Text | R::Metadata)
        | (Selector::OdtTable { .. }, R::Text | R::Metadata)
        | (Selector::OdtCell { .. }, R::Text | R::Metadata)
        | (Selector::OdtList { .. }, R::Text | R::Metadata)
        | (Selector::OdtFind { .. }, R::Text) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 3,
            required_nodes: 4,
            will_materialize: kinds(&[
                "OdtModel",
                "OdtContent",
                "PackageMemberDecoded",
                "PackageMemberRaw",
            ]),
            will_not_materialize: kinds(&["other-parts", "whole-document"]),
        }),
        #[cfg(feature = "odt")]
        (Selector::OdtPart(_), R::Metadata) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 1,
            required_nodes: 2,
            will_materialize: kinds(&["OdtModel"]),
            will_not_materialize: kinds(&["other-parts", "whole-document"]),
        }),
        #[cfg(feature = "odt")]
        (Selector::OdtPart(_), R::ExactBytes | R::DecodedBytes) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 2,
            required_nodes: 3,
            will_materialize: kinds(&["OdtModel", "PackageMemberRaw", "PackageMemberDecoded"]),
            will_not_materialize: kinds(&["other-parts", "whole-document"]),
        }),
        #[cfg(feature = "ods")]
        (Selector::OdsSheet { .. }, R::Text | R::Structure | R::Metadata)
        | (Selector::OdsCell { .. }, R::Text | R::Metadata | R::Structure | R::ExactBytes)
        | (Selector::OdsFind { .. }, R::Text)
        | (Selector::OdsComments { .. }, R::Metadata | R::Structure)
        | (Selector::OdsNamedExpressions, R::Metadata | R::Structure) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 3,
            required_nodes: 4,
            will_materialize: kinds(&[
                "OdsModel",
                "OdsContent",
                "PackageMemberDecoded",
                "PackageMemberRaw",
            ]),
            will_not_materialize: kinds(&["other-sheets", "whole-document"]),
        }),
        #[cfg(feature = "ods")]
        (Selector::OdsStyles, R::Metadata | R::Structure) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 3,
            required_nodes: 5,
            will_materialize: kinds(&[
                "OdsModel",
                "OdsContent",
                "OdsStyles",
                "PackageMemberDecoded",
                "PackageMemberRaw",
            ]),
            will_not_materialize: kinds(&["other-sheets", "whole-document"]),
        }),
        #[cfg(feature = "xlsx")]
        (Selector::XlsxSheet { .. }, R::Text | R::Structure | R::Metadata)
        | (Selector::XlsxCell { .. }, R::Text | R::Metadata | R::Structure | R::ExactBytes)
        | (Selector::XlsxFind { .. }, R::Text) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 3,
            required_nodes: 5,
            will_materialize: kinds(&[
                "XlsxModel",
                "XlsxWorkbook",
                "XlsxSheet",
                "PackageMemberDecoded",
                "PackageMemberRaw",
            ]),
            will_not_materialize: kinds(&["other-sheets", "whole-document"]),
        }),
        #[cfg(feature = "pptx")]
        (Selector::PptxSlide { .. }, R::Text | R::Structure | R::Metadata)
        | (Selector::PptxShape { .. }, R::Text | R::Metadata | R::Structure | R::ExactBytes)
        | (Selector::PptxNotes { .. }, R::Text | R::Metadata)
        | (Selector::PptxTables { .. }, R::Text | R::Metadata)
        | (Selector::PptxFind { .. }, R::Text) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 3,
            required_nodes: 5,
            will_materialize: kinds(&[
                "PptxModel",
                "PptxPresentation",
                "PptxSlide",
                "PackageMemberDecoded",
                "PackageMemberRaw",
            ]),
            will_not_materialize: kinds(&["other-slides", "whole-document"]),
        }),
        #[cfg(feature = "pptx")]
        (Selector::PptxLayouts, R::Metadata | R::Structure)
        | (Selector::PptxMasters, R::Metadata | R::Structure)
        | (Selector::PptxTheme, R::Metadata | R::Structure) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 1,
            required_nodes: 2,
            will_materialize: kinds(&["PptxModel"]),
            will_not_materialize: kinds(&["other-parts", "whole-document"]),
        }),
        #[cfg(feature = "pptx")]
        (
            Selector::PptxMedia { .. },
            R::Metadata | R::Structure | R::ExactBytes | R::DecodedBytes,
        ) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 2,
            required_nodes: 3,
            will_materialize: kinds(&["PptxModel", "PackageMemberRaw", "PackageMemberDecoded"]),
            will_not_materialize: kinds(&["other-parts", "whole-document"]),
        }),
        #[cfg(feature = "odp")]
        (Selector::OdpSlide { .. }, R::Text | R::Structure | R::Metadata)
        | (Selector::OdpShape { .. }, R::Text | R::Metadata | R::Structure)
        | (Selector::OdpNotes { .. }, R::Text | R::Metadata)
        | (Selector::OdpTables { .. }, R::Text | R::Metadata | R::Structure)
        | (Selector::OdpFind { .. }, R::Text) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 3,
            required_nodes: 4,
            will_materialize: kinds(&[
                "OdpModel",
                "OdpContent",
                "PackageMemberDecoded",
                "PackageMemberRaw",
            ]),
            will_not_materialize: kinds(&["other-slides", "whole-document"]),
        }),
        #[cfg(feature = "odp")]
        (Selector::OdpMasters, R::Metadata | R::Structure) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 3,
            required_nodes: 5,
            will_materialize: kinds(&[
                "OdpModel",
                "OdpContent",
                "OdpStyles",
                "PackageMemberDecoded",
                "PackageMemberRaw",
            ]),
            will_not_materialize: kinds(&["other-slides", "whole-document"]),
        }),
        #[cfg(feature = "odp")]
        (
            Selector::OdpMedia { .. },
            R::Metadata | R::Structure | R::ExactBytes | R::DecodedBytes,
        ) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 2,
            required_nodes: 3,
            will_materialize: kinds(&["OdpModel", "PackageMemberRaw", "PackageMemberDecoded"]),
            will_not_materialize: kinds(&["other-parts", "whole-document"]),
        }),
        #[cfg(feature = "json")]
        (Selector::JsonPointer { .. }, R::Metadata | R::Structure | R::ExactBytes | R::Text)
        | (Selector::JsonNode { .. }, R::Metadata | R::Structure | R::ExactBytes | R::Text)
        | (Selector::JsonFind { .. }, R::Text | R::Metadata | R::Structure) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 1,
            required_nodes: 2,
            will_materialize: kinds(&["JsonModel", "DocumentExact"]),
            will_not_materialize: kinds(&["other-nodes", "whole-document"]),
        }),
        #[cfg(feature = "yaml")]
        (Selector::YamlPath { .. }, R::Metadata | R::Structure | R::ExactBytes | R::Text)
        | (Selector::YamlNode { .. }, R::Metadata | R::Structure | R::ExactBytes | R::Text)
        | (Selector::YamlDocuments, R::Metadata | R::Structure | R::Text)
        | (Selector::YamlAnchor { .. }, R::Metadata | R::Structure | R::ExactBytes | R::Text)
        | (Selector::YamlFind { .. }, R::Text | R::Metadata | R::Structure) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 1,
            required_nodes: 2,
            will_materialize: kinds(&["YamlModel", "DocumentExact"]),
            will_not_materialize: kinds(&["other-nodes", "whole-document"]),
        }),
        #[cfg(feature = "markdown")]
        (Selector::MdHeading { .. }, R::Metadata | R::Structure | R::ExactBytes | R::Text)
        | (Selector::MdBlock { .. }, R::Metadata | R::Structure | R::ExactBytes | R::Text)
        | (Selector::MdCode { .. }, R::Metadata | R::Structure | R::ExactBytes | R::Text)
        | (Selector::MdLink { .. }, R::Metadata | R::Structure | R::Text)
        | (Selector::MdFind { .. }, R::Text | R::Metadata | R::Structure) => Ok(ObservePlan {
            shape: PlanShape::DeepenThenObserve,
            index_reads: 1,
            required_nodes: 2,
            will_materialize: kinds(&["MarkdownModel", "DocumentExact"]),
            will_not_materialize: kinds(&["other-nodes", "whole-document"]),
        }),
        _ => Err(Error::unsupported_feature(format!(
            "unsupported observation: selector {} with representation {}",
            req.selector.canonical(),
            req.representation.name()
        ))),
    }
}

#[derive(Debug, Clone, Copy)]
enum PageRepr {
    Text,
    Preview,
    Structure,
}

fn kinds(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| (*s).to_string()).collect()
}

fn index_plan(kind: &str) -> ObservePlan {
    ObservePlan {
        shape: PlanShape::IndexLookup,
        index_reads: 1,
        required_nodes: 1,
        will_materialize: kinds(&[kind]),
        will_not_materialize: kinds(&["images", "xobjects", "PageContent", "whole-document"]),
    }
}

/// Plan a common (format-neutral) observation. Pure and capability-checked: an
/// unsupported pair fails closed with the same typed error the evaluator raises.
fn common_plan(manifest: &FieldRoot, req: &ObserveRequest) -> Result<ObservePlan> {
    use crate::field::capabilities;
    let fmt = DocumentFormat::from_provenance(&manifest.provenance).ok_or_else(|| {
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
    // Document metadata is answered from already-validated state; every other
    // common observation materializes the format's model/content closure.
    if matches!(req.selector, Selector::Metadata) {
        return Ok(ObservePlan {
            shape: PlanShape::CachedObservation,
            index_reads: 0,
            required_nodes: 0,
            will_materialize: Vec::new(),
            will_not_materialize: kinds(&["whole-document", "seed-nodes"]),
        });
    }
    Ok(ObservePlan {
        shape: PlanShape::DeepenThenObserve,
        index_reads: 2,
        required_nodes: 4,
        will_materialize: kinds(common_materialize(fmt)),
        will_not_materialize: kinds(&["other-spine-items", "other-stories", "whole-document"]),
    })
}

fn common_materialize(fmt: DocumentFormat) -> &'static [&'static str] {
    match fmt {
        DocumentFormat::Pdf => &["PageContent", "TextRuns"],
        DocumentFormat::Docx => &[
            "DocxModel",
            "DocxStory",
            "PackageMemberDecoded",
            "PackageMemberRaw",
        ],
        DocumentFormat::Epub => &[
            "EpubModel",
            "EpubContent",
            "PackageMemberDecoded",
            "PackageMemberRaw",
        ],
        DocumentFormat::Odt => &[
            "OdtModel",
            "OdtContent",
            "PackageMemberDecoded",
            "PackageMemberRaw",
        ],
        DocumentFormat::Ods => &[
            "OdsModel",
            "OdsContent",
            "PackageMemberDecoded",
            "PackageMemberRaw",
        ],
        DocumentFormat::Xlsx => &[
            "XlsxModel",
            "XlsxWorkbook",
            "XlsxSheet",
            "PackageMemberDecoded",
            "PackageMemberRaw",
        ],
        DocumentFormat::Pptx => &[
            "PptxModel",
            "PptxPresentation",
            "PptxSlide",
            "PackageMemberDecoded",
            "PackageMemberRaw",
        ],
        DocumentFormat::Odp => &[
            "OdpModel",
            "OdpContent",
            "PackageMemberDecoded",
            "PackageMemberRaw",
        ],
        DocumentFormat::Json => &["JsonModel", "DocumentExact"],
        DocumentFormat::Yaml => &["YamlModel", "DocumentExact"],
        DocumentFormat::Csv => &["CsvModel", "DocumentExact"],
        DocumentFormat::Markdown => &["MarkdownModel", "DocumentExact"],
        DocumentFormat::Opaque => &[],
    }
}

fn index_entries(
    manifest: &FieldRoot,
    store: &FieldStore,
    key: SelectorKey,
) -> Result<Vec<IndexEntry>> {
    if !manifest.has_index() {
        return Ok(Vec::new());
    }
    let istore = FsIndexStore::open(store.root())?;
    let root = NodeId::from_bytes(manifest.index_root);
    lookup(&istore, &root, &key)
}

fn page_plan(
    manifest: &FieldRoot,
    store: &FieldStore,
    page: u32,
    repr: PageRepr,
) -> Result<ObservePlan> {
    let entry = index_entries(manifest, store, SelectorKey::new(SEL_PAGE, page))?
        .into_iter()
        .next()
        .ok_or_else(|| {
            Error::unsupported_feature(format!(
                "no page matching selector number {page} in the observation index"
            ))
        })?;
    let (_ops, text, preview) = derived_nodes(page, entry.node_id);
    let target = match repr {
        PageRepr::Text => text,
        PageRepr::Preview | PageRepr::Structure => preview,
    };
    let present = store.seeds().contains_node(&target.content_id())?;
    let shape = if present {
        PlanShape::NodeMaterialize
    } else {
        PlanShape::DeepenThenObserve
    };
    let (materialize, required) = match repr {
        PageRepr::Text => (kinds(&["PageContent", "ContentOperators", "TextRuns"]), 3),
        PageRepr::Preview => (kinds(&["PageContent", "PagePreview"]), 2),
        PageRepr::Structure => (
            kinds(&["PageContent", "PagePreview", "ContentOperators"]),
            3,
        ),
    };
    Ok(ObservePlan {
        shape,
        index_reads: 1,
        required_nodes: required,
        will_materialize: materialize,
        will_not_materialize: kinds(&["images", "xobjects", "whole-document"]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_names_are_lower_snake_case() {
        for shape in [
            PlanShape::CachedObservation,
            PlanShape::IndexLookup,
            PlanShape::SourceSlice,
            PlanShape::NodeMaterialize,
            PlanShape::DeepenThenObserve,
            PlanShape::FullMaterialize,
        ] {
            let name = shape.name();
            assert!(name.chars().all(|c| c.is_ascii_lowercase() || c == '_'));
        }
    }
}
