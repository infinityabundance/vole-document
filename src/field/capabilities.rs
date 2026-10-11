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
const ODT_NATIVE: &[&str] = &[
    "odt-part",
    "odt-paragraph",
    "odt-heading",
    "odt-table",
    "odt-cell",
    "odt-list",
    "odt-find",
    "package-part",
    "member",
];
const PDF_NATIVE: &[&str] = &[
    "document",
    "object",
    "stream",
    "revision",
    "revisions",
    "page",
    "byte-range",
    "text-match",
];
const ODS_NATIVE: &[&str] = &[
    "ods-sheet",
    "ods-cell",
    "ods-find",
    "ods-styles",
    "ods-named-expressions",
    "ods-comments",
    "package-part",
    "member",
];
const XLSX_NATIVE: &[&str] = &[
    "xlsx-sheet",
    "xlsx-cell",
    "xlsx-find",
    "xlsx-styles",
    "xlsx-defined-names",
    "xlsx-external-rels",
    "xlsx-comments",
    "xlsx-hyperlinks",
    "xlsx-tables",
    "xlsx-drawing",
    "package-part",
    "relationship",
    "member",
];
const PPTX_NATIVE: &[&str] = &[
    "pptx-slide",
    "pptx-shape",
    "pptx-notes",
    "pptx-layouts",
    "pptx-masters",
    "pptx-theme",
    "pptx-media",
    "pptx-tables",
    "pptx-find",
    "package-part",
    "relationship",
    "member",
];
const ODP_NATIVE: &[&str] = &[
    "odp-slide",
    "odp-shape",
    "odp-notes",
    "odp-masters",
    "odp-media",
    "odp-tables",
    "odp-find",
    "package-part",
    "member",
];
const JSON_NATIVE: &[&str] = &["json-pointer", "json-node", "json-find"];
const JSON5_NATIVE: &[&str] = &[
    "json5-pointer",
    "json5-node",
    "json5-find",
    "json5-comments",
];
const YAML_NATIVE: &[&str] = &[
    "yaml-path",
    "yaml-node",
    "yaml-documents",
    "yaml-anchor",
    "yaml-find",
];
const CSV_NATIVE: &[&str] = &["csv-row", "csv-cell", "csv-header", "csv-range", "csv-find"];
const MARKDOWN_NATIVE: &[&str] = &["md-heading", "md-block", "md-code", "md-link", "md-find"];
const XML_NATIVE: &[&str] = &[
    "xml-path",
    "xml-element",
    "xml-attr",
    "xml-namespaces",
    "xml-find",
];
const HTML_NATIVE: &[&str] = &[
    "html-path",
    "html-element",
    "html-attr",
    "html-scripts",
    "html-find",
];
const TOML_NATIVE: &[&str] = &["toml-path", "toml-table", "toml-find"];
const JSONL_NATIVE: &[&str] = &["jsonl-line", "jsonl-pointer", "jsonl-find"];
const EML_NATIVE: &[&str] = &[
    "eml-header",
    "eml-part",
    "eml-attachments",
    "eml-body",
    "eml-find",
];
const PARQUET_NATIVE: &[&str] = &[
    "parquet-schema",
    "parquet-column",
    "parquet-row-group",
    "parquet-cell",
];
const ARROW_NATIVE: &[&str] = &["arrow-schema", "arrow-column", "arrow-batch", "arrow-cell"];
const CBOR_NATIVE: &[&str] = &["cbor-pointer", "cbor-node", "cbor-find"];
const MSGPACK_NATIVE: &[&str] = &["msgpack-pointer", "msgpack-node", "msgpack-find"];
const CONFIG_NATIVE: &[&str] = &[
    "config-line",
    "config-entry",
    "config-section",
    "config-find",
];
const FEED_NATIVE: &[&str] = &[
    "feed-channel",
    "feed-field",
    "feed-entry",
    "feed-entry-field",
    "feed-find",
];
const GEOJSON_NATIVE: &[&str] = &[
    "geojson-type",
    "geojson-feature",
    "geojson-geometry",
    "geojson-coordinates",
    "geojson-property",
    "geojson-find",
];
const GIS_NATIVE: &[&str] = &[
    "gis-root",
    "gis-field",
    "gis-record",
    "gis-record-field",
    "gis-point",
    "gis-find",
];
const NOTEBOOK_NATIVE: &[&str] = &[
    "notebook-nbformat",
    "notebook-cell",
    "notebook-cell-type",
    "notebook-cell-source",
    "notebook-cell-output",
    "notebook-find",
];
const FIXEDWIDTH_NATIVE: &[&str] = &[
    "fixedwidth-row",
    "fixedwidth-cell",
    "fixedwidth-header",
    "fixedwidth-columns",
    "fixedwidth-range",
    "fixedwidth-find",
];
const RST_NATIVE: &[&str] = &[
    "rst-heading",
    "rst-block",
    "rst-directive",
    "rst-inline",
    "rst-find",
];
const ASCIIDOC_NATIVE: &[&str] = &[
    "adoc-heading",
    "adoc-block",
    "adoc-attribute",
    "adoc-inline",
    "adoc-find",
];
const MDX_NATIVE: &[&str] = &[
    "mdx-heading",
    "mdx-block",
    "mdx-esm",
    "mdx-jsx",
    "mdx-expression",
    "mdx-find",
];

const MHTML_NATIVE: &[&str] = &[
    "mhtml-root",
    "mhtml-resource",
    "mhtml-location",
    "mhtml-find",
];

const LOGSTREAM_NATIVE: &[&str] = &["logstream-line", "logstream-field", "logstream-find"];

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
            DocumentFormat::Odt => (
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
                odt_profiles(),
                ODT_NATIVE.to_vec(),
            ),
            DocumentFormat::Ods => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("table", COMMON_TEXT_META),
                    caps("cell", COMMON_TEXT_META),
                    caps("search-match", SEARCH),
                ],
                ods_profiles(),
                ODS_NATIVE.to_vec(),
            ),
            DocumentFormat::Xlsx => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("table", COMMON_TEXT_META),
                    caps("cell", COMMON_TEXT_META),
                    caps("search-match", SEARCH),
                ],
                xlsx_profiles(),
                XLSX_NATIVE.to_vec(),
            ),
            DocumentFormat::Pptx => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("table", COMMON_TEXT_META),
                    caps("cell", COMMON_TEXT_META),
                    caps("search-match", SEARCH),
                ],
                pptx_profiles(),
                PPTX_NATIVE.to_vec(),
            ),
            DocumentFormat::Odp => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("table", COMMON_TEXT_META),
                    caps("cell", COMMON_TEXT_META),
                    caps("search-match", SEARCH),
                ],
                odp_profiles(),
                ODP_NATIVE.to_vec(),
            ),
            DocumentFormat::Json => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("search-match", SEARCH),
                ],
                Vec::new(),
                JSON_NATIVE.to_vec(),
            ),
            DocumentFormat::Json5 => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("search-match", SEARCH),
                ],
                Vec::new(),
                JSON5_NATIVE.to_vec(),
            ),
            DocumentFormat::Yaml => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("search-match", SEARCH),
                ],
                Vec::new(),
                YAML_NATIVE.to_vec(),
            ),
            DocumentFormat::Csv => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("table", COMMON_TEXT_META),
                    caps("cell", COMMON_TEXT_META),
                    caps("search-match", SEARCH),
                ],
                Vec::new(),
                CSV_NATIVE.to_vec(),
            ),
            DocumentFormat::Markdown => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("heading", COMMON_TEXT_META),
                    caps("block", COMMON_TEXT_META),
                    caps("search-match", SEARCH),
                ],
                Vec::new(),
                MARKDOWN_NATIVE.to_vec(),
            ),
            DocumentFormat::Xml => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("search-match", SEARCH),
                ],
                Vec::new(),
                XML_NATIVE.to_vec(),
            ),
            DocumentFormat::Html => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("heading", COMMON_TEXT_META),
                    caps("link", COMMON_METADATA),
                    caps("search-match", SEARCH),
                ],
                Vec::new(),
                HTML_NATIVE.to_vec(),
            ),
            DocumentFormat::Toml => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("search-match", SEARCH),
                ],
                Vec::new(),
                TOML_NATIVE.to_vec(),
            ),
            DocumentFormat::Jsonl => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("search-match", SEARCH),
                ],
                Vec::new(),
                JSONL_NATIVE.to_vec(),
            ),
            DocumentFormat::Eml => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("resource", COMMON_RESOURCE_BYTES),
                    caps("search-match", SEARCH),
                ],
                Vec::new(),
                EML_NATIVE.to_vec(),
            ),
            DocumentFormat::Parquet => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("table", COMMON_TEXT_META),
                    caps("cell", COMMON_TEXT_META),
                    caps("search-match", SEARCH),
                ],
                Vec::new(),
                PARQUET_NATIVE.to_vec(),
            ),
            DocumentFormat::ArrowIpc => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("table", COMMON_TEXT_META),
                    caps("cell", COMMON_TEXT_META),
                    caps("search-match", SEARCH),
                ],
                Vec::new(),
                ARROW_NATIVE.to_vec(),
            ),
            DocumentFormat::Cbor => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("search-match", SEARCH),
                ],
                Vec::new(),
                CBOR_NATIVE.to_vec(),
            ),
            DocumentFormat::Msgpack => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("search-match", SEARCH),
                ],
                Vec::new(),
                MSGPACK_NATIVE.to_vec(),
            ),
            DocumentFormat::Config => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("search-match", SEARCH),
                ],
                Vec::new(),
                CONFIG_NATIVE.to_vec(),
            ),
            DocumentFormat::Feed => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("search-match", SEARCH),
                ],
                Vec::new(),
                FEED_NATIVE.to_vec(),
            ),
            DocumentFormat::Geojson => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("search-match", SEARCH),
                ],
                Vec::new(),
                GEOJSON_NATIVE.to_vec(),
            ),
            DocumentFormat::Gis => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("search-match", SEARCH),
                ],
                Vec::new(),
                GIS_NATIVE.to_vec(),
            ),
            DocumentFormat::Notebook => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("search-match", SEARCH),
                ],
                Vec::new(),
                NOTEBOOK_NATIVE.to_vec(),
            ),
            DocumentFormat::FixedWidth => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("table", COMMON_TEXT_META),
                    caps("cell", COMMON_TEXT_META),
                    caps("search-match", SEARCH),
                ],
                Vec::new(),
                FIXEDWIDTH_NATIVE.to_vec(),
            ),
            DocumentFormat::Rst => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("heading", COMMON_TEXT_META),
                    caps("block", COMMON_TEXT_META),
                    caps("search-match", SEARCH),
                ],
                Vec::new(),
                RST_NATIVE.to_vec(),
            ),
            DocumentFormat::Asciidoc => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("heading", COMMON_TEXT_META),
                    caps("block", COMMON_TEXT_META),
                    caps("search-match", SEARCH),
                ],
                Vec::new(),
                ASCIIDOC_NATIVE.to_vec(),
            ),
            DocumentFormat::Mdx => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("heading", COMMON_TEXT_META),
                    caps("block", COMMON_TEXT_META),
                    caps("search-match", SEARCH),
                ],
                Vec::new(),
                MDX_NATIVE.to_vec(),
            ),
            DocumentFormat::Mhtml => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("resource", COMMON_RESOURCE_BYTES),
                    caps("search-match", SEARCH),
                ],
                Vec::new(),
                MHTML_NATIVE.to_vec(),
            ),
            DocumentFormat::Logstream => (
                vec![
                    caps("metadata", COMMON_METADATA),
                    caps("text", COMMON_TEXT),
                    caps("search-match", SEARCH),
                ],
                Vec::new(),
                LOGSTREAM_NATIVE.to_vec(),
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

#[cfg(feature = "odt")]
fn odt_profiles() -> Vec<String> {
    vec![crate::adapter::odt::OdtExtractProfile::DEFAULT.fingerprint()]
}
#[cfg(not(feature = "odt"))]
fn odt_profiles() -> Vec<String> {
    Vec::new()
}

#[cfg(feature = "ods")]
fn ods_profiles() -> Vec<String> {
    vec![crate::adapter::ods::OdsExtractProfile::DEFAULT.fingerprint()]
}
#[cfg(not(feature = "ods"))]
fn ods_profiles() -> Vec<String> {
    Vec::new()
}

#[cfg(feature = "xlsx")]
fn xlsx_profiles() -> Vec<String> {
    vec![crate::adapter::xlsx::XlsxExtractProfile::DEFAULT.fingerprint()]
}
#[cfg(not(feature = "xlsx"))]
fn xlsx_profiles() -> Vec<String> {
    Vec::new()
}

#[cfg(feature = "pptx")]
fn pptx_profiles() -> Vec<String> {
    vec![crate::adapter::pptx::PptxExtractProfile::DEFAULT.fingerprint()]
}
#[cfg(not(feature = "pptx"))]
fn pptx_profiles() -> Vec<String> {
    Vec::new()
}

#[cfg(feature = "odp")]
fn odp_profiles() -> Vec<String> {
    vec![crate::adapter::odp::OdpExtractProfile::DEFAULT.fingerprint()]
}
#[cfg(not(feature = "odp"))]
fn odp_profiles() -> Vec<String> {
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
