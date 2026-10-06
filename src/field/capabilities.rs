//! Machine-readable capability discovery for the common observation layer
//! (Phase 12.7, ADR-0031, plan §DEC-5).
//!
//! Formats do not share coordinates, so the set of *common* observations a
//! format can serve is explicit and auditable rather than silently approximated.
//! [`capabilities_for_format`] returns the supported common selectors and their
//! admissible representations, the versioned extraction profiles, and the native
//! selectors; [`common_supported`] is the single predicate the planner and the
//! evaluator both use, so a capability error is never a surprise.

use crate::field::document_format::DocumentFormat;
use crate::field::observe::{Representation, Selector};

/// One common selector and the representations it admits for a format.
#[derive(Debug, Clone, Copy)]
pub struct SelectorCapability {
    /// Canonical common selector name.
    pub selector: &'static str,
    /// Admissible representation names.
    pub representations: &'static [&'static str],
}

/// The capability set for one detected format.
#[derive(Debug, Clone)]
pub struct Capabilities {
    /// The detected format.
    pub format: DocumentFormat,
    /// Whether the adapter for this format is compiled into this build.
    pub compiled: bool,
    /// Supported common selectors (empty when the adapter is not compiled).
    pub selectors: Vec<SelectorCapability>,
    /// Versioned extraction-profile fingerprints (DOCX/EPUB).
    pub profiles: Vec<String>,
    /// Native selectors retained as first-class peers of the common vocabulary.
    pub native_selectors: Vec<&'static str>,
}

impl Capabilities {
    /// A deterministic, flat JSON object (machine-readable capability discovery).
    pub fn to_json(&self) -> String {
        let selectors = self
            .selectors
            .iter()
            .map(|s| {
                let reps = s
                    .representations
                    .iter()
                    .map(|r| format!("\"{r}\""))
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    "{{\"selector\":\"{}\",\"representations\":[{reps}]}}",
                    s.selector
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let profiles = self
            .profiles
            .iter()
            .map(|p| format!("\"{}\"", crate::field::provenance::json_escape(p)))
            .collect::<Vec<_>>()
            .join(",");
        let native = self
            .native_selectors
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            concat!(
                "{{",
                "\"root\":\"document\",",
                "\"format\":\"{}\",",
                "\"adapter\":\"{}\",",
                "\"compiled\":{},",
                "\"selectors\":[{}],",
                "\"profiles\":[{}],",
                "\"native_selectors\":[{}]",
                "}}"
            ),
            self.format.name(),
            self.format.adapter(),
            self.compiled,
            selectors,
            profiles,
            native
        )
    }
}

const DOCX_NATIVE: &[&str] = &[
    "docx-story",
    "docx-paragraph",
    "docx-table",
    "docx-cell",
    "docx-find",
    "package-part",
    "relationship",
    "member",
];
const EPUB_NATIVE: &[&str] = &[
    "epub-package",
    "epub-manifest-item",
    "epub-spine-item",
    "epub-nav",
    "epub-nav-node",
    "epub-resource",
    "epub-block",
    "epub-cell",
    "epub-link",
    "epub-find",
    "member",
];
const PDF_NATIVE: &[&str] = &[
    "document",
    "object",
    "stream",
    "revision",
    "page",
    "byte-range",
    "text-match",
];

const COMMON_METADATA: &[&str] = &["metadata"];
const COMMON_TEXT: &[&str] = &["text"];
const COMMON_TEXT_META: &[&str] = &["text", "metadata"];
const COMMON_RESOURCE: &[&str] = &["metadata"];
const COMMON_RESOURCE_BYTES: &[&str] = &["metadata", "exact", "decoded"];
const SEARCH: &[&str] = &["text"];

fn caps(selector: &'static str, representations: &'static [&'static str]) -> SelectorCapability {
    SelectorCapability {
        selector,
        representations,
    }
}

/// The capability set for a detected format, filtered by what this build compiles.
pub fn capabilities_for_format(format: DocumentFormat) -> Capabilities {
    let compiled = format.compiled();
    if !compiled {
        return Capabilities {
            format,
            compiled,
            selectors: Vec::new(),
            profiles: Vec::new(),
            native_selectors: Vec::new(),
        };
    }
    let (selectors, profiles, native_selectors): (Vec<SelectorCapability>, Vec<String>, Vec<&str>) =
        match format {
            DocumentFormat::Pdf => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("search-match", SEARCH),
                ],
                Vec::new(),
                PDF_NATIVE.to_vec(),
            ),
            DocumentFormat::Docx => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("heading", COMMON_TEXT_META),
                    caps("block", COMMON_TEXT_META),
                    caps("table", COMMON_TEXT_META),
                    caps("cell", COMMON_TEXT_META),
                    caps("resource", COMMON_RESOURCE),
                    caps("link", COMMON_METADATA),
                    caps("search-match", SEARCH),
                ],
                docx_profiles(),
                DOCX_NATIVE.to_vec(),
            ),
            DocumentFormat::Epub => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("heading", COMMON_TEXT_META),
                    caps("block", COMMON_TEXT_META),
                    caps("table", COMMON_TEXT_META),
                    caps("cell", COMMON_TEXT_META),
                    caps("resource", COMMON_RESOURCE_BYTES),
                    caps("link", COMMON_METADATA),
                    caps("search-match", SEARCH),
                ],
                epub_profiles(),
                EPUB_NATIVE.to_vec(),
            ),
            DocumentFormat::Opaque => (Vec::new(), Vec::new(), Vec::new()),
        };
    Capabilities {
        format,
        compiled,
        selectors,
        profiles,
        native_selectors,
    }
}

#[cfg(feature = "docx")]
fn docx_profiles() -> Vec<String> {
    vec![crate::adapter::docx::DocxExtractProfile::DEFAULT.fingerprint()]
}
#[cfg(not(feature = "docx"))]
fn docx_profiles() -> Vec<String> {
    Vec::new()
}

#[cfg(feature = "epub")]
fn epub_profiles() -> Vec<String> {
    vec![crate::adapter::epub::EpubExtractProfile::DEFAULT.fingerprint()]
}
#[cfg(not(feature = "epub"))]
fn epub_profiles() -> Vec<String> {
    Vec::new()
}

/// The canonical name of a common selector, or `None` for a native selector.
pub fn common_selector_name(selector: &Selector) -> Option<&'static str> {
    match selector {
        Selector::Metadata => Some("metadata"),
        Selector::Text => Some("text"),
        Selector::Heading(_) => Some("heading"),
        Selector::Block(_) => Some("block"),
        Selector::Table(_) => Some("table"),
        Selector::Cell { .. } => Some("cell"),
        Selector::Resource(_) => Some("resource"),
        Selector::Link(_) => Some("link"),
        Selector::SearchMatch(_) => Some("search-match"),
        _ => None,
    }
}

/// The representations admissible for `selector` under `format`, if the pair is
/// structurally applicable.
pub fn common_representations(
    format: DocumentFormat,
    selector: &Selector,
) -> Option<&'static [&'static str]> {
    if !format.compiled() {
        return None;
    }
    let name = common_selector_name(selector)?;
    let caps = capabilities_for_format(format);
    caps.selectors
        .iter()
        .find(|c| c.selector == name)
        .map(|c| c.representations)
}

/// Whether `format` can serve `selector` with `representation`.
pub fn common_supported(
    format: DocumentFormat,
    selector: &Selector,
    representation: Representation,
) -> bool {
    common_representations(format, selector)
        .is_some_and(|reps| reps.contains(&representation.name()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_supports_text_and_search_but_not_tables() {
        assert!(common_supported(
            DocumentFormat::Pdf,
            &Selector::Text,
            Representation::Text
        ));
        assert!(common_supported(
            DocumentFormat::Pdf,
            &Selector::SearchMatch("x".into()),
            Representation::Text
        ));
        assert!(!common_supported(
            DocumentFormat::Pdf,
            &Selector::Table(0),
            Representation::Text
        ));
        assert!(!common_supported(
            DocumentFormat::Pdf,
            &Selector::Text,
            Representation::Metadata
        ));
    }

    #[test]
    fn opaque_and_unknown_selectors_are_unsupported() {
        assert!(!common_supported(
            DocumentFormat::Opaque,
            &Selector::Text,
            Representation::Text
        ));
        assert!(!common_supported(
            DocumentFormat::Pdf,
            &Selector::Document,
            Representation::Metadata
        ));
    }
}
