//! Byte-based document-format detection for the universal observation API
//! (Phase 12.7, ADR-0031, plan §DEC-5).
//!
//! The format of a field is decided from its **source bytes**, never from a file
//! name or extension. Detection is deliberately conservative: an ambiguous or
//! malformed input falls back to [`DocumentFormat::Opaque`] rather than guessing,
//! and a format whose adapter is not compiled in cannot be detected (the input is
//! then `Opaque`), so the reported capability set always matches what the build
//! can actually serve.
//!
//! * **PDF** — the byte-authoritative physical scanner admits a validated PDF
//!   ([`crate::adapter::pdf::detect`]).
//! * **DOCX** — a valid ZIP that also carries the OPC content-types part
//!   (`[Content_Types].xml`) and a package `officeDocument` relationship.
//! * **EPUB** — a valid ZIP that is an OCF container: the mandatory stored
//!   `mimetype` member equals `application/epub+zip`, or `META-INF/container.xml`
//!   names the OCF namespace and an OPF (`application/oebps-package+xml`) rootfile.
//! * **ODT** — a valid ZIP that is an OpenDocument (ODF) package: the mandatory
//!   stored `mimetype` member is an OpenDocument *text* media type, or
//!   `META-INF/manifest.xml` declares one.
//! * **ODS** — a valid ZIP that is an OpenDocument (ODF) package whose mandatory
//!   stored `mimetype` member is an OpenDocument *spreadsheet* media type, or whose
//!   `META-INF/manifest.xml` declares one (Phase 21.3.1). Mutually exclusive with
//!   ODT (a text document declares the text media type, a spreadsheet the
//!   spreadsheet one).
//! * **ODP** — a valid ZIP that is an OpenDocument (ODF) package whose mandatory
//!   stored `mimetype` member is an OpenDocument *presentation* media type, or whose
//!   `META-INF/manifest.xml` declares one (Phase 21.4.1). Mutually exclusive with
//!   ODT/ODS (a presentation declares the presentation media type).
//! * **Opaque** — everything else, including a ZIP that matches none of the above
//!   (or more than one — an ambiguous ZIP fails safe).
//! * **JSON** — the whole source parses as exactly one JSON value within the
//!   JSON caps (Phase 21.5.1). JSON is a Wave-2 structured-tree format with no
//!   package layer, so it is detected directly (never via `detect_zip_family`);
//!   a malformed or oversized input stays `Opaque`.
//!
//! The detected format is recorded in the field manifest's provenance (a
//! machine-readable `format=<name>;` prefix, see [`DocumentFormat::from_provenance`]),
//! so `observe`/`find`/`explain` can dispatch common selectors without reading the
//! whole source again.

use crate::limits::Limits;

/// The mandatory OCF `mimetype` payload.
pub const EPUB_MIMETYPE: &[u8] = b"application/epub+zip";
/// The mandatory OCF container descriptor member.
pub const CONTAINER_MEMBER: &[u8] = b"META-INF/container.xml";
/// The OCF container-descriptor namespace.
pub const CONTAINER_NS: &[u8] = b"urn:oasis:names:tc:opendocument:xmlns:container";
/// The default (and only normative) package-document media type.
pub const OPF_MEDIA_TYPE: &[u8] = b"application/oebps-package+xml";
/// The OPC content-types part.
#[cfg(feature = "package")]
const CONTENT_TYPES_MEMBER: &[u8] = b"[Content_Types].xml";
/// The OPC package-relationships part.
#[cfg(feature = "package")]
const PACKAGE_RELS_MEMBER: &[u8] = b"_rels/.rels";
/// The `officeDocument` relationship type fragment (transitional and strict).
#[cfg(feature = "package")]
const OFFICE_DOCUMENT_FRAGMENT: &[u8] = b"officeDocument";
/// The WordprocessingML document main content-type fragment (Phase 21.1.2).
#[cfg(feature = "package")]
const DOCX_MAIN_FRAGMENT: &[u8] = b"wordprocessingml.document.main+xml";
/// The canonical WordprocessingML main-part target fragment (Phase 21.1.2).
#[cfg(feature = "package")]
const DOCX_MAIN_TARGET: &[u8] = b"word/document.xml";
/// The ODF package manifest member (Phase 13.3).
#[cfg(feature = "odt")]
const ODT_MANIFEST_MEMBER: &[u8] = b"META-INF/manifest.xml";
/// The OpenDocument *text* media-type fragment (Phase 13.3).
#[cfg(feature = "odt")]
const ODT_TEXT_FRAGMENT: &[u8] = b"application/vnd.oasis.opendocument.text";
/// The ODF package manifest member for the ODS detection rule (Phase 21.3.1).
#[cfg(feature = "ods")]
const ODS_MANIFEST_MEMBER: &[u8] = b"META-INF/manifest.xml";
/// The OpenDocument *spreadsheet* media-type fragment (Phase 21.3.1).
#[cfg(feature = "ods")]
const ODS_SPREADSHEET_FRAGMENT: &[u8] = b"application/vnd.oasis.opendocument.spreadsheet";
/// The ODF package manifest member for the ODP detection rule (Phase 21.4.1).
#[cfg(feature = "odp")]
const ODP_MANIFEST_MEMBER: &[u8] = b"META-INF/manifest.xml";
/// The OpenDocument *presentation* media-type fragment (Phase 21.4.1).
#[cfg(feature = "odp")]
const ODP_PRESENTATION_FRAGMENT: &[u8] = b"application/vnd.oasis.opendocument.presentation";
/// The ODF media-type prefix shared by every OpenDocument package.
#[cfg(any(feature = "odt", feature = "ods", feature = "odp"))]
const ODF_MEDIA_PREFIX: &[u8] = b"application/vnd.oasis.opendocument.";
/// The SpreadsheetML workbook main content-type fragment (Phase 21.1.1).
#[cfg(feature = "xlsx")]
const XLSX_MAIN_FRAGMENT: &[u8] = b"spreadsheetml.sheet.main+xml";
/// The SpreadsheetML content-type namespace fragment (Phase 21.1.1).
#[cfg(feature = "xlsx")]
const XLSX_NS_FRAGMENT: &[u8] = b"spreadsheetml";
/// A SpreadsheetML workbook part-target fragment (Phase 21.1.1).
#[cfg(feature = "xlsx")]
const XLSX_WORKBOOK_TARGET: &[u8] = b"xl/workbook.xml";
/// The PresentationML presentation main content-type fragment (Phase 21.2.1).
#[cfg(feature = "pptx")]
const PPTX_MAIN_FRAGMENT: &[u8] = b"presentationml.presentation.main+xml";
/// The PresentationML content-type namespace fragment (Phase 21.2.1).
#[cfg(feature = "pptx")]
const PPTX_NS_FRAGMENT: &[u8] = b"presentationml";
/// The canonical PresentationML main-part target fragment (Phase 21.2.1).
#[cfg(feature = "pptx")]
const PPTX_MAIN_TARGET: &[u8] = b"ppt/presentation.xml";

/// A detected document format (the class of the field's source bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentFormat {
    /// A validated PDF (physical indicators).
    Pdf,
    /// An OPC package with a WordprocessingML `officeDocument` part.
    Docx,
    /// An OCF container with an EPUB package document.
    Epub,
    /// An ODF package with an OpenDocument text content part.
    Odt,
    /// An ODF package with an OpenDocument spreadsheet content part.
    Ods,
    /// An ODF package with an OpenDocument presentation content part.
    Odp,
    /// An OPC package with a SpreadsheetML workbook part.
    Xlsx,
    /// An OPC package with a PresentationML presentation part.
    Pptx,
    /// A structured JSON document (the whole source parses as exactly one JSON
    /// value). Not a package: the exact leaf is the whole source (Phase 21.5.1).
    Json,
    /// A structured JSON5 / JSONC document (the whole source parses as exactly one
    /// JSON5 value and uses at least one JSON5/JSONC-only construct — a comment, a
    /// trailing comma, an unquoted key, a single-quoted string, a hex/leading-dot/
    /// `Infinity`/`NaN` number, a string continuation, or the extended whitespace
    /// set). A source that is **strict JSON** stays [`DocumentFormat::Json`]; the
    /// recorded dialect distinguishes `jsonc` (comments/trailing commas only) from
    /// `json5`. Not a package: the exact leaf is the whole source (Phase 21.17.1).
    Json5,
    /// A structured YAML document (the whole source parses as a bounded YAML
    /// stream whose every document root is a mapping or a sequence). Not a package:
    /// the exact leaf is the whole source (Phase 21.6.1).
    Yaml,
    /// A CSV/TSV table (the whole source parses under a comma or tab delimiter as a
    /// table with a consistent field count across a sampled majority of at least
    /// two records). Not a package: the exact leaf is the whole source, and the
    /// recorded dialect distinguishes comma from tab (Phase 21.7.1).
    Csv,
    /// A Markdown prose document (the whole source carries at least one structural
    /// mark — an ATX heading, a fenced code block, front matter, a table, a
    /// reference definition, or a footnote definition). Not a package: the exact
    /// leaf is the whole source (Phase 21.8.1).
    Markdown,
    /// A standalone XML document (the whole source begins with `<` and parses as
    /// well-formed XML with exactly one root element under the XML caps). Not a
    /// package: the exact leaf is the whole source, and every node/attribute span
    /// is a `Q_gen` projection (Phase 21.9).
    Xml,
    /// A standalone HTML document (the source is not well-formed XML and carries
    /// clear HTML structure — a `<!doctype html>`, an `<html`/`<head`/`<body` tag,
    /// or a preponderance of known HTML tags). Parsed by a bounded,
    /// **error-recovering** scanner. Not a package: the exact leaf is the whole
    /// source, and every element/attribute/text/comment/raw-text span is a `Q_gen`
    /// projection (Phase 21.10).
    Html,
    /// A TOML document (the source is not any earlier format, parses as TOML 1.0
    /// under the TOML caps with no errors, and carries at least one key/value
    /// assignment). Not a package: the exact leaf is the whole source, and every
    /// table/array/key/value/comment span is a `Q_gen` projection. TOML's
    /// duplicate-key/redefinition rules are enforced (a violation is a typed
    /// decline) (Phase 21.11).
    Toml,
    /// A JSONL / NDJSON line/event stream (the source carries at least two non-blank
    /// lines and **every** non-blank line parses as exactly one JSON value under the
    /// caps, each line parsed by the shared JSON parser). Not a package: the exact
    /// leaf is the whole source, and every record's exact line span, terminator, and
    /// per-token spans are `Q_gen` projections. A single JSON value stays
    /// [`DocumentFormat::Json`] (Phase 21.12).
    Jsonl,
    /// An EML / MIME internet message (an RFC 5322 header block terminated by a
    /// blank line, carrying `From`/`Date`/`Message-ID`, or an explicit
    /// `MIME-Version`). Not a package: the exact leaf is the whole source, and every
    /// header/part span and every `Content-Transfer-Encoding`-decoded constituent is
    /// a `Q_gen` projection (Phase 21.13).
    Eml,
    /// An Apache Parquet file (the source begins with `PAR1` and ends with `PAR1`,
    /// and the 4-byte little-endian footer length before the trailing magic is
    /// consistent with the file length). Not a package: the exact leaf is the whole
    /// source, and the parsed footer inventory (schema, row groups, column chunks,
    /// statistics) and any decoded values are `Q_gen` projections (Phase 21.14).
    Parquet,
    /// An Apache Arrow IPC file/stream (the source begins with the `ARROW1` magic
    /// and is either the **file** format — a trailing `ARROW1` magic preceded by a
    /// consistent little-endian `int32` footer length — or the **stream** format —
    /// a valid encapsulated `Schema` message at the 8-byte magic+padding prefix).
    /// Not a package: the exact leaf is the whole source, and the parsed schema and
    /// record-batch inventory (with each batch's exact source span) and any decoded
    /// columnar values are `Q_gen` projections (Phase 21.16).
    ArrowIpc,
    /// A CBOR (RFC 8949) structured-tree document. CBOR has **no magic bytes**, so
    /// detection is deliberately conservative: either the self-described-CBOR tag
    /// `55799` (`0xd9 0xd9 0xf7`) at the start of a source that then parses, in full,
    /// as exactly one well-formed CBOR item; or a full-input well-formed parse whose
    /// root is a container (array/map) or a tag and which reaches at least three
    /// nodes (so a single scalar, an empty container, and any structurally trivial
    /// input stay `Opaque`). A container head byte is always `>= 0x80`, so a
    /// pure-ASCII text document is never claimed; and the whole-number/short-
    /// container prefix overlaps MessagePack's fixint/fixarray encodings, so an
    /// ambiguous or trivial input stays `Opaque` rather than being guessed
    /// (recorded honestly; a Phase-21.19 MessagePack adapter must share the seam).
    /// Not a package: the exact leaf is the whole source, and every item's kind,
    /// exact span, encoding width, tag number, float width, and definite/indefinite
    /// form are `Q_gen` projections (Phase 21.18).
    Cbor,
    /// A MessagePack structured-tree document. MessagePack has **no magic bytes**, so
    /// detection is deliberately conservative: a full-input well-formed parse of
    /// exactly one item whose root is a container (array/map) reaching a node/byte
    /// threshold, or the same with an unambiguous MessagePack-only head byte
    /// (`0xdc..=0xdf`, which CBOR's grammar rejects). A lone scalar, an empty
    /// container, and any structurally trivial or ambiguous input stay `Opaque`. A
    /// container head byte is always `>= 0x80`, so a pure-ASCII text document is never
    /// claimed; and the whole-number/short-container prefix overlaps CBOR's
    /// `fixint`/short containers, so CBOR (tried first) owns any input well-formed
    /// under both. Not a package: the exact leaf is the whole source, and every item's
    /// kind, exact span, exact format byte (encoding width and signedness), `str` vs
    /// `bin`, float width, and extension type/length are `Q_gen` projections
    /// (Phase 21.19).
    Msgpack,
    /// A config-family document (INI, `.env`, or Java `.properties`). The whole
    /// source is a bounded, line-based key/value document; detection is
    /// deliberately conservative and requires a **dialect-distinguishing signal**,
    /// so the pure `KEY=VALUE` overlap between `env` and `properties` (and plain
    /// prose, and a `#!` script) stays `Opaque` rather than being guessed. Not a
    /// package: the exact leaf is the whole source, the recorded dialect lives in
    /// the model, and every line's content span, key/separator/value span, quoting,
    /// inline-comment, `export`, continuation, and `\uXXXX`-spelling fact is a
    /// `Q_gen` projection (Phase 21.20).
    Config,
    /// An RSS 2.0 / Atom 1.0 syndication feed. A feed is XML, so its physical
    /// bytes are shared with [`DocumentFormat::Xml`]; the recorded dialect
    /// (`rss`/`atom`) lives in the model, exactly as CSV/config record their
    /// dialects. Detection is a **bounded semantic test** run before the generic
    /// XML detector: an RSS root `<rss>` with a `<channel>` child, or an Atom root
    /// `<feed>` in the Atom namespace (`http://www.w3.org/2005/Atom`) with at least
    /// one `<entry>` child. Not a package: the exact leaf is the whole source, and
    /// every element/attribute span, element order, attribute spelling, and the
    /// namespace declaration is a `Q_gen` projection (Phase 21.21).
    Feed,
    /// A GeoJSON (RFC 7946) document. GeoJSON is JSON, so its physical bytes are
    /// shared with [`DocumentFormat::Json`]; the recorded root class (one of the nine
    /// RFC 7946 type names) lives in the model, exactly as a feed records its
    /// dialect. Detection is a **bounded semantic test** run before the generic JSON
    /// detector: the source parses as exactly one JSON value whose root is an object
    /// with a `"type"` string equal to one of the nine type names and whose shape is
    /// consistent (a geometry has an array `coordinates`/`geometries`, a `Feature` has
    /// `geometry`/`properties`, a `FeatureCollection` has an array `features`). A plain
    /// JSON document, and a JSON document whose `"type"` is an unrelated string, stay
    /// [`DocumentFormat::Json`]. Not a package: the exact leaf is the whole source, and
    /// every JSON token span, `coordinates` nesting, `properties` order/duplicates, and
    /// foreign member is a `Q_gen` projection (Phase 21.22).
    Geojson,
    /// A KML 2.2 / GPX 1.1 geospatial document. KML and GPX are XML, so their
    /// physical bytes are shared with [`DocumentFormat::Xml`]; the recorded dialect
    /// (`kml`/`gpx`) lives in the model, exactly as a feed records its dialect.
    /// Detection is a **bounded semantic test** run before the generic XML detector:
    /// a `<kml>` root in the KML namespace (`http://www.opengis.net/kml/2.2`) with a
    /// `Document`/`Folder`/`Placemark` child, or a `<gpx>` root in the GPX namespace
    /// (`http://www.topografix.com/GPX/1/1`) with a `metadata`/`wpt`/`rte`/`trk`
    /// child. A plain XML document, and a shaped-but-invalid KML/GPX, stay
    /// [`DocumentFormat::Xml`]. Not a package: the exact leaf is the whole source,
    /// and every element/attribute span, element order, attribute spelling (KML
    /// geometry `coordinates`, GPX `lat`/`lon`), and the namespace declaration is a
    /// `Q_gen` projection (Phase 21.23).
    Gis,
    /// A Jupyter notebook (`.ipynb`, nbformat) document. A notebook's physical bytes
    /// are JSON, so its bytes are shared with [`DocumentFormat::Json`]; the recorded
    /// `nbformat`/`nbformat_minor` and the cell anchors live in the model, exactly as a
    /// feed records its dialect. Detection is a **bounded semantic test** run before the
    /// generic JSON detector: the source parses as exactly one JSON value whose root is
    /// an object with a plain non-negative integer-literal `nbformat` (≥ 1) and an array
    /// `cells`, every cell an object with a string `cell_type`, and every present
    /// recognized field nbformat-shaped. A plain JSON document, and a JSON document that
    /// merely has a `cells` key but is not nbformat-shaped, stay
    /// [`DocumentFormat::Json`]. Not a package: the exact leaf is the whole source, and
    /// every JSON token span, the exact `cell_type`/`output_type` strings, the exact
    /// `source` representation, and the cell/output order are `Q_gen` projections
    /// (Phase 21.24).
    Notebook,
    /// A fixed-width (column-position) text table. Unlike CSV/TSV/PSV, its columns
    /// are defined by **character positions**, not a delimiter, so it is a distinct
    /// format rather than a CSV dialect. Detection is **maximally conservative**
    /// (fixed-width is inherently ambiguous): at least three sampled records of
    /// identical byte width, an inferred column layout whose interior whitespace gaps
    /// are at least two columns wide, at least two non-empty columns, and an outright
    /// decline of anything that also parses as a delimited (CSV/TSV/PSV) or Markdown
    /// table. Not a package: the exact leaf is the whole source, and the recorded
    /// inferred layout, per-record spans, per-column spans, padding, terminator, BOM,
    /// and header row are `Q_gen` projections (Phase 21.25).
    FixedWidth,
    /// Anything else; preserved exactly by the opaque floor.
    Opaque,
}

impl DocumentFormat {
    /// Stable lower-case name (used in JSON and in the manifest provenance token).
    pub const fn name(self) -> &'static str {
        match self {
            DocumentFormat::Pdf => "pdf",
            DocumentFormat::Docx => "docx",
            DocumentFormat::Epub => "epub",
            DocumentFormat::Odt => "odt",
            DocumentFormat::Ods => "ods",
            DocumentFormat::Odp => "odp",
            DocumentFormat::Xlsx => "xlsx",
            DocumentFormat::Pptx => "pptx",
            DocumentFormat::Json => "json",
            DocumentFormat::Json5 => "json5",
            DocumentFormat::Yaml => "yaml",
            DocumentFormat::Csv => "csv",
            DocumentFormat::Markdown => "markdown",
            DocumentFormat::Xml => "xml",
            DocumentFormat::Html => "html",
            DocumentFormat::Toml => "toml",
            DocumentFormat::Jsonl => "jsonl",
            DocumentFormat::Eml => "eml",
            DocumentFormat::Parquet => "parquet",
            DocumentFormat::ArrowIpc => "arrow",
            DocumentFormat::Cbor => "cbor",
            DocumentFormat::Msgpack => "msgpack",
            DocumentFormat::Config => "config",
            DocumentFormat::Feed => "feed",
            DocumentFormat::Geojson => "geojson",
            DocumentFormat::Gis => "gis",
            DocumentFormat::Notebook => "notebook",
            DocumentFormat::FixedWidth => "fixedwidth",
            DocumentFormat::Opaque => "opaque",
        }
    }

    /// The adapter that serves this format (the observation layer's name for it).
    pub const fn adapter(self) -> &'static str {
        match self {
            DocumentFormat::Pdf => "pdf",
            DocumentFormat::Docx => "docx",
            DocumentFormat::Epub => "epub",
            DocumentFormat::Odt => "odt",
            DocumentFormat::Ods => "ods",
            DocumentFormat::Odp => "odp",
            DocumentFormat::Xlsx => "xlsx",
            DocumentFormat::Pptx => "pptx",
            DocumentFormat::Json => "json",
            DocumentFormat::Json5 => "json5",
            DocumentFormat::Yaml => "yaml",
            DocumentFormat::Csv => "csv",
            DocumentFormat::Markdown => "markdown",
            DocumentFormat::Xml => "xml",
            DocumentFormat::Html => "html",
            DocumentFormat::Toml => "toml",
            DocumentFormat::Jsonl => "jsonl",
            DocumentFormat::Eml => "eml",
            DocumentFormat::Parquet => "parquet",
            DocumentFormat::ArrowIpc => "arrow",
            DocumentFormat::Cbor => "cbor",
            DocumentFormat::Msgpack => "msgpack",
            DocumentFormat::Config => "config",
            DocumentFormat::Feed => "feed",
            DocumentFormat::Geojson => "geojson",
            DocumentFormat::Gis => "gis",
            DocumentFormat::Notebook => "notebook",
            DocumentFormat::FixedWidth => "fixedwidth",
            DocumentFormat::Opaque => "opaque",
        }
    }

    /// Whether the adapter for this format is compiled into this build.
    pub const fn compiled(self) -> bool {
        match self {
            DocumentFormat::Pdf | DocumentFormat::Opaque => true,
            DocumentFormat::Docx => cfg!(feature = "docx"),
            DocumentFormat::Epub => cfg!(feature = "epub"),
            DocumentFormat::Odt => cfg!(feature = "odt"),
            DocumentFormat::Ods => cfg!(feature = "ods"),
            DocumentFormat::Odp => cfg!(feature = "odp"),
            DocumentFormat::Xlsx => cfg!(feature = "xlsx"),
            DocumentFormat::Pptx => cfg!(feature = "pptx"),
            DocumentFormat::Json => cfg!(feature = "json"),
            DocumentFormat::Json5 => cfg!(feature = "json5"),
            DocumentFormat::Yaml => cfg!(feature = "yaml"),
            DocumentFormat::Csv => cfg!(feature = "csv"),
            DocumentFormat::Markdown => cfg!(feature = "markdown"),
            DocumentFormat::Xml => cfg!(feature = "xml"),
            DocumentFormat::Html => cfg!(feature = "html"),
            DocumentFormat::Toml => cfg!(feature = "toml"),
            DocumentFormat::Jsonl => cfg!(feature = "jsonl"),
            DocumentFormat::Eml => cfg!(feature = "eml"),
            DocumentFormat::Parquet => cfg!(feature = "parquet"),
            DocumentFormat::ArrowIpc => cfg!(feature = "arrow"),
            DocumentFormat::Cbor => cfg!(feature = "cbor"),
            DocumentFormat::Msgpack => cfg!(feature = "msgpack"),
            DocumentFormat::Config => cfg!(feature = "config"),
            DocumentFormat::Feed => cfg!(feature = "feed"),
            DocumentFormat::Geojson => cfg!(feature = "geojson"),
            DocumentFormat::Gis => cfg!(feature = "gis"),
            DocumentFormat::Notebook => cfg!(feature = "notebook"),
            DocumentFormat::FixedWidth => cfg!(feature = "fixedwidth"),
        }
    }

    /// The machine-readable `format=<name>;` provenance prefix recorded at ingest.
    pub fn provenance_prefix(self) -> String {
        format!("format={};", self.name())
    }

    /// Recover the recorded format from a field manifest's provenance string.
    ///
    /// Returns `None` for a manifest that does not carry the token (e.g. a field
    /// written before this subphase); such a field still serves every native
    /// selector, but common observations decline typed rather than guessing.
    pub fn from_provenance(provenance: &str) -> Option<DocumentFormat> {
        let rest = provenance.strip_prefix("format=")?;
        let name = rest.split(';').next()?;
        match name {
            "pdf" => Some(DocumentFormat::Pdf),
            "docx" => Some(DocumentFormat::Docx),
            "epub" => Some(DocumentFormat::Epub),
            "odt" => Some(DocumentFormat::Odt),
            "ods" => Some(DocumentFormat::Ods),
            "odp" => Some(DocumentFormat::Odp),
            "xlsx" => Some(DocumentFormat::Xlsx),
            "pptx" => Some(DocumentFormat::Pptx),
            "json" => Some(DocumentFormat::Json),
            "json5" => Some(DocumentFormat::Json5),
            "yaml" => Some(DocumentFormat::Yaml),
            "csv" => Some(DocumentFormat::Csv),
            "markdown" => Some(DocumentFormat::Markdown),
            "xml" => Some(DocumentFormat::Xml),
            "html" => Some(DocumentFormat::Html),
            "toml" => Some(DocumentFormat::Toml),
            "jsonl" => Some(DocumentFormat::Jsonl),
            "eml" => Some(DocumentFormat::Eml),
            "parquet" => Some(DocumentFormat::Parquet),
            "arrow" => Some(DocumentFormat::ArrowIpc),
            "cbor" => Some(DocumentFormat::Cbor),
            "msgpack" => Some(DocumentFormat::Msgpack),
            "config" => Some(DocumentFormat::Config),
            "feed" => Some(DocumentFormat::Feed),
            "geojson" => Some(DocumentFormat::Geojson),
            "gis" => Some(DocumentFormat::Gis),
            "notebook" => Some(DocumentFormat::Notebook),
            "fixedwidth" => Some(DocumentFormat::FixedWidth),
            "opaque" => Some(DocumentFormat::Opaque),
            _ => None,
        }
    }
}

/// Detect the document format of `source` from its bytes alone.
///
/// Never consults a file name or extension. A malformed or ambiguous input (or a
/// format whose adapter is not compiled) falls back to [`DocumentFormat::Opaque`].
pub fn detect_document_format(source: &[u8], limits: Limits) -> DocumentFormat {
    if crate::adapter::pdf::detect(source, limits) {
        return DocumentFormat::Pdf;
    }
    #[cfg(feature = "package")]
    {
        if let Some(format) = detect_zip_family(source, limits) {
            return format;
        }
    }
    // Parquet is the **analytical** Wave-2 format (Phase 21.14), with no package
    // layer. Its detection is a strong magic-byte contract — `PAR1` at both ends
    // plus a consistent little-endian footer length — so it runs immediately after
    // the package families and **before** the weak, no-magic-byte heuristics
    // (JSON/YAML/CSV/Markdown/XML/HTML), which a binary columnar file must never be
    // guessed to be. It is deliberately conservative: a file that merely begins or
    // ends with `PAR1` but whose footer length overruns the leading magic stays
    // `Opaque`.
    #[cfg(feature = "parquet")]
    if crate::adapter::parquet::detect(source, limits) {
        return DocumentFormat::Parquet;
    }
    // Arrow IPC is the **analytical** Wave-2 format (Phase 21.16), with no package
    // layer. Its detection is a strong magic-byte contract — the `ARROW1` magic at
    // the start plus either a consistent trailing footer (file format) or a valid
    // encapsulated `Schema` message (stream format) — so it runs immediately after
    // Parquet and **before** the weak, no-magic-byte heuristics, which a binary
    // columnar file must never be guessed to be. It is deliberately conservative:
    // `ARROW1`-prefixed junk with neither a consistent footer nor a valid schema
    // message stays `Opaque`.
    #[cfg(feature = "arrow")]
    if crate::adapter::arrow::detect(source, limits) {
        return DocumentFormat::ArrowIpc;
    }
    // JSON is a Wave-2 structured-tree format with **no** package layer, so it is
    // detected directly from the whole source (never through `detect_zip_family`).
    // Conservative: the entire source must parse as exactly one JSON value within
    // the JSON caps, else the input stays Opaque (Phase 21.5.1).
    //
    // GeoJSON is the **spatial** Wave-2 format (Phase 21.22), whose physical bytes
    // are JSON, so it is a **bounded semantic sub-detection** run **before** the
    // generic JSON detector below: the source must parse as exactly one JSON value
    // whose root is an object with a `"type"` string equal to one of the nine RFC 7946
    // type names **and** whose shape is consistent (a geometry has an array
    // `coordinates`/`geometries`, a `Feature` has `geometry`/`properties`, a
    // `FeatureCollection` has an array `features`). A more specific claim than a bare
    // JSON value, so it is tried first; a plain JSON document, and a JSON document
    // whose `"type"` is an unrelated string, decline here and are then claimed by the
    // JSON detector (staying `Json`). Prose and malformed input stay `Opaque`.
    #[cfg(feature = "geojson")]
    if crate::adapter::geojson::detect(source, limits) {
        return DocumentFormat::Geojson;
    }
    // A Jupyter notebook (`.ipynb`, nbformat) is the **document-shaped** Wave-2 format
    // (Phase 21.24), whose physical bytes are JSON, so it is a **bounded semantic
    // sub-detection** run **before** the generic JSON detector below: the source must
    // parse as exactly one JSON value that is an nbformat-shaped object — a plain
    // non-negative integer-literal `nbformat` (≥ 1), an array `cells`, every cell an
    // object with a string `cell_type`, and every present recognized field
    // nbformat-shaped (a string-or-array-of-strings `source`, a number-or-null
    // `execution_count`, object `metadata`/`attachments`, an array `outputs` of objects
    // with a string `output_type`). A more specific claim than a bare JSON value, so it
    // is tried first; a plain JSON document, and a JSON document that merely has a
    // `cells` key but is not nbformat-shaped, decline here and are then claimed by the
    // JSON detector (staying `Json`). Prose and malformed input stay `Opaque`.
    #[cfg(feature = "notebook")]
    if crate::adapter::notebook::detect(source, limits) {
        return DocumentFormat::Notebook;
    }
    #[cfg(feature = "json")]
    if crate::adapter::json::detect(source, limits) {
        return DocumentFormat::Json;
    }
    // JSON5/JSONC is the **structured-extra** Wave-2 format (Phase 21.17.1), also
    // with no package layer. It is tried **immediately after JSON** and before
    // JSONL/YAML/TOML/CSV/Markdown/XML/HTML: JSON5 is a superset of JSON, so a
    // strict JSON source is already claimed by the JSON detector above, and this
    // detector requires at least one JSON5/JSONC-only construct (a comment, a
    // trailing comma, an unquoted key, a single quote, a hex/leading-dot/
    // `Infinity`/`NaN` number, a string continuation, or the extended whitespace
    // set). A strict JSON document is therefore never reclassified, and a plain
    // non-JSON blob stays Opaque.
    #[cfg(feature = "json5")]
    if crate::adapter::json5::detect(source, limits) {
        return DocumentFormat::Json5;
    }
    // JSONL/NDJSON is the **line/event-stream** Wave-2 format (Phase 21.12), also
    // with no package layer. It is tried **immediately after JSON** because it is the
    // most specific signal for a newline-separated JSON stream: each non-blank line
    // must parse as exactly one JSON value, which is stricter than — and must run
    // before — the YAML stream reader (which would otherwise be free to reinterpret a
    // line-delimited JSON stream as a stream of documents). Detection is
    // conservative: at least two non-blank lines are required and every non-blank
    // line must parse as exactly one JSON value under the caps, so a single JSON
    // value, a malformed line, and a bag of values with a non-newline separator all
    // stay `Opaque`.
    #[cfg(feature = "jsonl")]
    if crate::adapter::jsonl::detect(source, limits) {
        return DocumentFormat::Jsonl;
    }
    // CBOR is the **binary** structured-tree Wave-2 format (Phase 21.18), also with
    // no package layer. It is tried **immediately after the JSON family** (JSON,
    // JSON5, JSONL) and **before** the remaining textual heuristics
    // (EML/YAML/TOML/CSV/Markdown/XML/HTML): the strong magic-byte binaries
    // (PDF/ZIP/Parquet/Arrow) run above and are never reconsidered. CBOR has **no
    // magic bytes**, so its detector is deliberately conservative — the
    // self-described-CBOR tag `55799` (`0xd9 0xd9 0xf7`), or a full-input,
    // well-formed parse whose root is a container/tag and which reaches at least
    // three nodes. A container head byte is always `>= 0x80`, so no pure-ASCII
    // document (JSON/YAML/TOML/XML/HTML/prose) can be claimed; a lone scalar, an
    // empty container, a truncated item, and a structurally trivial input all stay
    // `Opaque` rather than being guessed. The small-integer/short-array encodings
    // overlap MessagePack's fixint/fixarray (recorded honestly; a Phase-21.19
    // MessagePack adapter must share this seam).
    #[cfg(feature = "cbor")]
    if crate::adapter::cbor::detect(source, limits) {
        return DocumentFormat::Cbor;
    }
    // MessagePack is the **binary** structured-tree Wave-2 sibling of CBOR (Phase
    // 21.19), also with no package layer. It is tried **immediately after CBOR** and
    // **before** the remaining textual heuristics (EML/YAML/TOML/CSV/Markdown/XML/
    // HTML): the strong magic-byte binaries (PDF/ZIP/Parquet/Arrow) and the JSON
    // family run above and are never reconsidered. **Placement is load-bearing: the
    // two binary detectors share the whole-number/short-container prefix, so putting
    // CBOR first guarantees its stronger self-described-tag signal (and every input
    // well-formed under both grammars) is never stolen from it.** MessagePack has
    // **no magic bytes**, so its detector is deliberately conservative — a
    // full-input, well-formed parse whose root is a container (array/map) reaching at
    // least three items and eight bytes, or the same with an unambiguous
    // MessagePack-only head byte (`0xdc..=0xdf`, which CBOR's grammar rejects). A
    // container head byte is always `>= 0x80`, so no pure-ASCII document can be
    // claimed; a lone scalar, an empty container, the ambiguous
    // `fixarray(3)`/`fixmap(2)` overlap fixtures, and a structurally trivial input
    // all stay `Opaque` rather than being guessed.
    #[cfg(feature = "msgpack")]
    if crate::adapter::msgpack::detect(source, limits) {
        return DocumentFormat::Msgpack;
    }
    // EML/MIME is the **messaging** Wave-2 format (Phase 21.13), also with no package
    // layer. It is tried **immediately after JSONL** and **before YAML/TOML/CSV/
    // Markdown/XML/HTML**: a raw message begins with an RFC 5322 header block, which
    // the YAML stream reader would otherwise reinterpret as a mapping (e.g. `From: a`
    // / `Date: b`). Detection is conservative: a genuine header block terminated by a
    // blank line carrying `From`/`Date`/`Message-ID`, or an explicit `MIME-Version`,
    // is required, so prose without a header block and a colon-bearing note stay
    // `Opaque`. Only the header block is scanned (never the MIME tree).
    #[cfg(feature = "eml")]
    if crate::adapter::eml::detect(source, limits) {
        return DocumentFormat::Eml;
    }
    // YAML is the second Wave-2 structured-tree format (Phase 21.6.1), also with
    // **no** package layer. It is detected directly and conservatively: the whole
    // source must parse as a bounded YAML stream under the YAML caps, and every
    // document root must be a mapping or a sequence, else the input stays Opaque.
    #[cfg(feature = "yaml")]
    if crate::adapter::yaml::detect(source, limits) {
        return DocumentFormat::Yaml;
    }
    // TOML is the next Wave-2 structured-tree format (Phase 21.11). Precedence
    // (explicit): TOML is tried **after** the strong, fully-parsed tree formats
    // (JSON, YAML) but **before** the weak, no-magic-byte heuristics (CSV,
    // Markdown, XML, HTML). TOML's positive signal is a complete, error-free parse,
    // which is far stronger than the CSV/Markdown heuristics; crucially, a TOML
    // comment (`# …` at column 0) is otherwise misread as a Markdown ATX heading and
    // would steal the document. A source that is valid JSON/YAML still wins (it is
    // tried first); a source that is valid XML/HTML never parses as TOML (it begins
    // with `<`), so nothing is lost there. TOML has no magic bytes and a plain prose
    // paragraph is not TOML, so detection is conservative: the whole source must
    // parse as TOML 1.0 under the TOML caps with no errors **and** carry at least one
    // key/value assignment. TOML's duplicate-key/redefinition rules are enforced, so
    // a doc that violates them is not detected. Plain prose and non-TOML text stay
    // Opaque.
    #[cfg(feature = "toml")]
    if crate::adapter::toml::detect(source, limits) {
        return DocumentFormat::Toml;
    }
    // The config family (INI / `.env` / Java `.properties`) is the key/value line
    // Wave-2 format (Phase 21.20). Precedence (explicit): it is tried **after** the
    // strong, fully-parsed tree formats (JSON/JSON5/JSONL/YAML/TOML) — a TOML
    // document is very often a syntactically valid INI file, so a source that is
    // valid TOML must stay `Toml` (never reclassified) — and **before** CSV/Markdown/
    // XML/HTML. It precedes CSV deliberately: a config file whose values contain
    // commas (e.g. `A=x,y`) has a consistent field count and would otherwise be
    // stolen by the CSV detector, whereas the config `KEY=VALUE` shape is the more
    // specific signal. Config has **no magic bytes**, so detection is deliberately
    // conservative and requires a **dialect-distinguishing signal**: an INI
    // `[section]` header; a properties `:`/whitespace separator, `!` comment,
    // `\uXXXX` escape, continuation, or non-identifier key (plus a strong `=`/`:`
    // separator somewhere); or an `env` `export ` prefix. The pure `KEY=VALUE`
    // overlap between `env` and `properties`, plain prose, a two-column `a b`
    // blob, and a `#!` script all stay `Opaque` rather than being guessed; a plain
    // `.txt`/Markdown/code source is never claimed.
    #[cfg(feature = "config")]
    if crate::adapter::config::detect(source, limits) {
        return DocumentFormat::Config;
    }
    // CSV/TSV is the first **tabular** Wave-2 format (Phase 21.7.1). It has no
    // magic bytes, so it is detected last and conservatively: only after the
    // PDF/ZIP/JSON/YAML families are declined does a source qualify, and then only
    // if it parses under a specific delimiter (`,`, tab, or `|`) as a table with a
    // consistent field count across a sampled majority of records and at least two
    // columns. Phase 21.25 adds the pipe delimiter to this dialect set; the pipe
    // dialect is tried after comma/tab and is declined when the source carries a
    // GFM/Markdown delimiter row (so a Markdown table is never stolen). When in
    // doubt the input stays Opaque.
    #[cfg(feature = "csv")]
    if crate::adapter::csv::detect(source, limits) {
        return DocumentFormat::Csv;
    }
    // Markdown is the first **prose** Wave-2 format (Phase 21.8.1). It has no
    // magic bytes either, and a plain prose paragraph is itself valid Markdown, so
    // detection is deliberately conservative: only after the PDF/ZIP/JSON/YAML/CSV
    // families are declined, and only when the source carries a clear structural
    // mark (an ATX heading, a fenced code block, front matter, a table, a reference
    // definition, or a footnote definition), is it admitted. Plain prose stays
    // Opaque rather than being guessed to be Markdown.
    #[cfg(feature = "markdown")]
    if crate::adapter::markdown::detect(source, limits) {
        return DocumentFormat::Markdown;
    }
    // Fixed-width (column-position) text is the second **tabular** Wave-2 format
    // (Phase 21.25), but its columns are defined by character positions, not a
    // delimiter, so it is a distinct format (and adapter), never a CSV dialect. It is
    // tried **last among the tabular/prose heuristics** — after CSV/TSV/PSV and after
    // Markdown — so a delimited table or a Markdown table always keeps its own format
    // (the fixed-width detector also declines both explicitly, defence in depth).
    // Fixed-width is **genuinely ambiguous** (nearly any aligned text can look
    // tabular), so detection is maximally conservative: at least three sampled records
    // of identical byte width, an inferred column layout whose interior whitespace gaps
    // are at least two columns wide, and at least two non-empty columns; a
    // variable-length or single-space-separated blob stays Opaque.
    #[cfg(feature = "fixedwidth")]
    if crate::adapter::fixedwidth::detect(source, limits) {
        return DocumentFormat::FixedWidth;
    }
    // RSS/Atom is the **syndication** Wave-2 format (Phase 21.21). A feed's physical
    // bytes are XML, so this is a **bounded semantic sub-detection** run **before**
    // the generic standalone-XML detector below: an RSS root `<rss>` with a
    // `<channel>` child, or an Atom root `<feed>` in the Atom namespace
    // (`http://www.w3.org/2005/Atom`) with at least one `<entry>` child. A feed is a
    // more specific claim than a bare XML tree, so it is tried first; a plain XML
    // document (a root that is neither `<rss>` nor an Atom `<feed>`, or an
    // `<rss>`-shaped-but-invalid / non-Atom / record-less `<feed>`) declines here and
    // falls through to the XML detector (staying `Xml`) or to `Opaque`. An HTML
    // document (`<!doctype html>` / an `<html>` root) simply declines here and is
    // then claimed by the HTML document-level marker immediately below.
    #[cfg(feature = "feed")]
    if crate::adapter::feed::detect(source, limits) {
        return DocumentFormat::Feed;
    }
    // KML/GPX is the **geospatial** Wave-2 format (Phase 21.23). Its physical bytes
    // are XML, so this is a **bounded semantic sub-detection** run **before** the
    // generic standalone-XML detector below: a `<kml>` root in the KML namespace
    // (`http://www.opengis.net/kml/2.2`) with a `Document`/`Folder`/`Placemark` child,
    // or a `<gpx>` root in the GPX namespace (`http://www.topografix.com/GPX/1/1`)
    // with a `metadata`/`wpt`/`rte`/`trk` child. A geospatial document is a more
    // specific claim than a bare XML tree, so it is tried first; a plain XML document
    // (a root that is neither `<kml>` nor `<gpx>`, or a shaped-but-invalid / non-GIS
    // namespace / child-less root) declines here and falls through to the XML detector
    // (staying `Xml`) or to `Opaque`. An older KML (2.0/2.1) or GPX 1.0 namespace is
    // not the claimed URI, so it also stays `Xml`. An HTML document simply declines
    // here and is then claimed by the HTML document-level marker immediately below.
    #[cfg(feature = "gis")]
    if crate::adapter::gis::detect(source, limits) {
        return DocumentFormat::Gis;
    }
    // HTML document-level marker (Phase 21.10 / 21.15 fix): a `<!doctype html>` or
    // an `<html>` root element is a more specific signal than the generic XML
    // fallback, so a **well-formed XHTML page** is classified `Html`, not `Xml`
    // (XML's well-formedness parser would otherwise claim it). The general XML
    // branch below is still tried before the general HTML branch, so a bare XML
    // tree stays XML and HTML only claims the sources XML declines.
    #[cfg(feature = "html")]
    if crate::adapter::html::has_document_marker(source)
        && crate::adapter::html::detect(source, limits)
    {
        return DocumentFormat::Html;
    }
    // XML is the structured-tree Wave-2 format for a bare XML source (Phase 21.9).
    // It has no package layer and no magic bytes beyond `<`, so it is detected last
    // and conservatively: only after the PDF/ZIP/JSON/YAML/CSV/Markdown families are
    // declined, only when the source begins with `<` (an XML prolog or a root
    // element start), and only when the whole source parses as well-formed XML with
    // exactly one root element under the XML caps. `<-prefixed` junk and prose stay
    // Opaque rather than being guessed to be XML.
    #[cfg(feature = "xml")]
    if crate::adapter::xml::detect(source, limits) {
        return DocumentFormat::Xml;
    }
    // HTML is the error-recovering markup Wave-2 format (Phase 21.10). Precedence
    // note (explicit): a *document-level* HTML marker is handled above (before XML);
    // for every other `<`-bearing source XML is tried before HTML, so any document
    // XML accepts (a bare XML tree) is genuinely XML. HTML then claims the
    // `<`-bearing sources XML declines — real HTML5 is almost never well-formed XML
    // (void elements, optional end tags, unquoted attributes, undeclared named
    // entities). Detection is conservative: the source must carry clear HTML
    // structure (a `<!doctype html>`, an `<html`/`<head`/`<body` tag, or a
    // preponderance of known HTML tags). Plain prose and non-HTML `<`-junk stay
    // Opaque rather than being guessed to be HTML.
    #[cfg(feature = "html")]
    if crate::adapter::html::detect(source, limits) {
        return DocumentFormat::Html;
    }
    DocumentFormat::Opaque
}

/// Whether `source` is a structurally valid ZIP archive.
///
/// Used by the universal ingest dispatcher so a generic ZIP (which detection
/// reports as `Opaque`) is still inverted through the byte-authoritative package
/// layer rather than the opaque floor.
#[cfg(feature = "package")]
pub fn is_zip(source: &[u8], limits: Limits) -> bool {
    crate::adapter::package::scan(source, limits).is_ok()
}

#[cfg(feature = "package")]
fn detect_zip_family(source: &[u8], limits: Limits) -> Option<DocumentFormat> {
    let physical = crate::adapter::package::scan(source, limits).ok()?;

    // EPUB (OCF): the mandatory `mimetype` member, or an OCF container that
    // resolves to an OPF rootfile.
    let mimetype = member_decoded(&physical, source, EPUB_MIMETYPE_MEMBER, limits);
    let mimetype_ok = mimetype.as_deref() == Some(EPUB_MIMETYPE);
    let container = member_decoded(&physical, source, CONTAINER_MEMBER, limits);
    let container_ok = container
        .as_deref()
        .is_some_and(|c| contains(c, CONTAINER_NS) && contains(c, OPF_MEDIA_TYPE));
    let is_epub = mimetype_ok || container_ok;

    // DOCX: an OPC package whose content types declare a WordprocessingML main
    // part, or whose package relationships declare an `officeDocument` part that
    // targets `word/document.xml`. The positive WordprocessingML signal (rather
    // than the mere absence of a SpreadsheetML one) keeps DOCX and XLSX mutually
    // exclusive without misclassifying a Word document that *embeds* an Excel
    // workbook (whose package declares SpreadsheetML content types for the
    // embedded part, but no SpreadsheetML workbook main part). Without `xlsx`
    // and `pptx` the legacy relationship-only rule stands.
    let content_types = member_decoded(&physical, source, CONTENT_TYPES_MEMBER, limits);
    let rels = member_decoded(&physical, source, PACKAGE_RELS_MEMBER, limits);
    #[cfg(any(feature = "xlsx", feature = "pptx"))]
    let is_docx = content_types
        .as_deref()
        .is_some_and(|ct| contains(ct, DOCX_MAIN_FRAGMENT))
        || rels.as_deref().is_some_and(|r| {
            contains(r, OFFICE_DOCUMENT_FRAGMENT) && contains(r, DOCX_MAIN_TARGET)
        });
    #[cfg(not(any(feature = "xlsx", feature = "pptx")))]
    let is_docx = content_types.is_some()
        && rels
            .as_deref()
            .is_some_and(|r| contains(r, OFFICE_DOCUMENT_FRAGMENT));

    // XLSX: an OPC package whose content types declare a SpreadsheetML workbook
    // (or whose `officeDocument` relationship targets a workbook part).
    #[cfg(feature = "xlsx")]
    let is_xlsx = content_types.as_deref().is_some_and(|ct| {
        contains(ct, XLSX_MAIN_FRAGMENT)
            || (contains(ct, XLSX_NS_FRAGMENT)
                && rels
                    .as_deref()
                    .is_some_and(|r| contains(r, XLSX_WORKBOOK_TARGET)))
    });
    #[cfg(not(feature = "xlsx"))]
    let is_xlsx = false;

    // PPTX: an OPC package whose content types declare a PresentationML main part
    // (or whose content types name PresentationML and whose `officeDocument`
    // relationship targets `ppt/presentation.xml`). The positive PresentationML
    // signal keeps it mutually exclusive with DOCX and XLSX: a Word/Excel document
    // that *embeds* a PowerPoint part declares only the PresentationML embed type
    // (`…presentationml.presentation`, not the `.main+xml` main part) and its
    // `officeDocument` relationship targets `word/document.xml`/`xl/workbook.xml`,
    // so it is never misclassified as PPTX.
    #[cfg(feature = "pptx")]
    let is_pptx = content_types.as_deref().is_some_and(|ct| {
        contains(ct, PPTX_MAIN_FRAGMENT)
            || (contains(ct, PPTX_NS_FRAGMENT)
                && rels
                    .as_deref()
                    .is_some_and(|r| contains(r, PPTX_MAIN_TARGET)))
    });
    #[cfg(not(feature = "pptx"))]
    let is_pptx = false;

    // The mandatory ODF `mimetype` member is **authoritative** when it names an
    // OpenDocument media type (ODF 1.2 §3.2: the package's own media type). An ODP
    // that *embeds a spreadsheet* lists both media types in `META-INF/manifest.xml`
    // (one per file-entry), so scanning the whole manifest would make it look like an
    // ODP *and* an ODS and force the ambiguous case below to Opaque. Only when the
    // `mimetype` member is absent or is not an ODF media type do we fall back to the
    // manifest scan (for a non-conformant package).
    #[cfg(any(feature = "odt", feature = "ods", feature = "odp"))]
    let odf_mimetype = mimetype
        .as_deref()
        .filter(|m| contains(m, ODF_MEDIA_PREFIX));

    // ODT: an ODF package whose mandatory `mimetype` (or, absent that,
    // `META-INF/manifest.xml`) declares an OpenDocument text media type.
    #[cfg(feature = "odt")]
    let is_odt = match odf_mimetype {
        Some(m) => contains(m, ODT_TEXT_FRAGMENT),
        None => member_decoded(&physical, source, ODT_MANIFEST_MEMBER, limits)
            .as_deref()
            .is_some_and(|m| contains(m, ODT_TEXT_FRAGMENT)),
    };
    #[cfg(not(feature = "odt"))]
    let is_odt = false;

    // ODS: an ODF package whose mandatory `mimetype` (or, absent that, the manifest)
    // declares an OpenDocument *spreadsheet* media type. The positive spreadsheet
    // fragment keeps it mutually exclusive with ODT (a text document declares the
    // text media type, never the spreadsheet one).
    #[cfg(feature = "ods")]
    let is_ods = match odf_mimetype {
        Some(m) => contains(m, ODS_SPREADSHEET_FRAGMENT),
        None => member_decoded(&physical, source, ODS_MANIFEST_MEMBER, limits)
            .as_deref()
            .is_some_and(|m| contains(m, ODS_SPREADSHEET_FRAGMENT)),
    };
    #[cfg(not(feature = "ods"))]
    let is_ods = false;

    // ODP: an ODF package whose mandatory `mimetype` (or, absent that, the manifest)
    // declares an OpenDocument *presentation* media type. The positive presentation
    // fragment keeps it mutually exclusive with ODT/ODS.
    #[cfg(feature = "odp")]
    let is_odp = match odf_mimetype {
        Some(m) => contains(m, ODP_PRESENTATION_FRAGMENT),
        None => member_decoded(&physical, source, ODP_MANIFEST_MEMBER, limits)
            .as_deref()
            .is_some_and(|m| contains(m, ODP_PRESENTATION_FRAGMENT)),
    };
    #[cfg(not(feature = "odp"))]
    let is_odp = false;

    // A ZIP matching more than one native signature is ambiguous: fail safe.
    let matches = [is_docx, is_epub, is_odt, is_ods, is_odp, is_xlsx, is_pptx]
        .iter()
        .filter(|b| **b)
        .count();
    match matches {
        1 if is_docx => Some(DocumentFormat::Docx),
        1 if is_epub => Some(DocumentFormat::Epub),
        1 if is_odt => Some(DocumentFormat::Odt),
        1 if is_ods => Some(DocumentFormat::Ods),
        1 if is_odp => Some(DocumentFormat::Odp),
        1 if is_xlsx => Some(DocumentFormat::Xlsx),
        1 if is_pptx => Some(DocumentFormat::Pptx),
        _ => None,
    }
}

#[cfg(feature = "package")]
const EPUB_MIMETYPE_MEMBER: &[u8] = b"mimetype";

/// Decode one member's bytes by exact name, bounded and decline-safe: encrypted,
/// oversized, unsupported-method, or out-of-range members yield `None`. The name
/// match is ASCII case-insensitive, mirroring the OPC layer
/// ([`crate::field::opc`]), which resolves the well-known control parts
/// (`[Content_Types].xml`, `_rels/.rels`) case-insensitively; a package that
/// lowercases them is still identified (Phase 21.15 fix).
#[cfg(feature = "package")]
fn member_decoded(
    physical: &crate::adapter::package::ZipPhysical,
    source: &[u8],
    name: &[u8],
    limits: Limits,
) -> Option<Vec<u8>> {
    const FLAG_ENCRYPTED: u16 = 0x0001;
    let member = physical
        .members
        .iter()
        .find(|m| m.name.eq_ignore_ascii_case(name))?;
    if member.flags & FLAG_ENCRYPTED != 0 || member.uncompressed_size > limits.max_xml_part_bytes {
        return None;
    }
    let off = usize::try_from(member.data.0).ok()?;
    let len = usize::try_from(member.data.1).ok()?;
    let raw = source.get(off..off.checked_add(len)?)?;
    match member.method {
        0 => Some(raw.to_vec()),
        8 => crate::field::derive::inflate_raw_deflate(raw, member.uncompressed_size, limits).ok(),
        _ => None,
    }
}

/// Byte-substring search (no allocation, case-sensitive).
#[cfg(feature = "package")]
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && needle.len() <= haystack.len()
        && haystack.windows(needle.len()).any(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_provenance_roundtrip() {
        for f in [
            DocumentFormat::Pdf,
            DocumentFormat::Docx,
            DocumentFormat::Epub,
            DocumentFormat::Odt,
            DocumentFormat::Ods,
            DocumentFormat::Odp,
            DocumentFormat::Xlsx,
            DocumentFormat::Pptx,
            DocumentFormat::Json,
            DocumentFormat::Json5,
            DocumentFormat::Yaml,
            DocumentFormat::Csv,
            DocumentFormat::Markdown,
            DocumentFormat::Xml,
            DocumentFormat::Html,
            DocumentFormat::Toml,
            DocumentFormat::Jsonl,
            DocumentFormat::Eml,
            DocumentFormat::Parquet,
            DocumentFormat::ArrowIpc,
            DocumentFormat::Cbor,
            DocumentFormat::Msgpack,
            DocumentFormat::Config,
            DocumentFormat::Feed,
            DocumentFormat::Geojson,
            DocumentFormat::Gis,
            DocumentFormat::Notebook,
            DocumentFormat::FixedWidth,
            DocumentFormat::Opaque,
        ] {
            let token = format!("{}field:package;members=1", f.provenance_prefix());
            assert_eq!(DocumentFormat::from_provenance(&token), Some(f));
        }
        assert_eq!(DocumentFormat::from_provenance("field:ingest-b"), None);
        assert_eq!(DocumentFormat::from_provenance("format=exotic;x"), None);
    }

    #[test]
    fn plain_bytes_are_opaque() {
        assert_eq!(
            detect_document_format(b"not a document", Limits::DEFAULT),
            DocumentFormat::Opaque
        );
    }

    #[test]
    fn a_corpus_pdf_is_detected() {
        let (_, pdf) = crate::adapter::pdf::sample_pdfs()
            .into_iter()
            .next()
            .expect("the PDF corpus is non-empty");
        assert_eq!(
            detect_document_format(&pdf, Limits::DEFAULT),
            DocumentFormat::Pdf
        );
    }
}
