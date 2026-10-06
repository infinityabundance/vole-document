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
use crate::field::index::{FsIndexStore, IndexEntry, SEL_PAGE, SelectorKey, lookup};
use crate::field::manifest::FieldRoot;
use crate::store::{NodeId, SeedStore};

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
