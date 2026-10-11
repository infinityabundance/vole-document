//! Typed errors with stable classes and documented CLI exit codes.
//!
//! Malformed data is never reported as an "internal invariant"; each failure
//! class is distinct so callers and the CLI can react precisely.

use core::fmt;

/// Convenience alias for the crate's fallible operations.
pub type Result<T> = core::result::Result<T, Error>;

/// Stable error classes. These distinguish failure *kinds* rather than prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    /// Host I/O failure (read/write/rename/fsync).
    Io,
    /// Command-line or API misuse.
    Usage,
    /// The container framing/header is malformed.
    InvalidContainer,
    /// The container declares a version this build does not implement.
    UnsupportedVersion,
    /// The container declares a mandatory feature this build does not support.
    UnsupportedFeature,
    /// A digest, length, or checksum disagreed with the declared value.
    IntegrityMismatch,
    /// A declared or configured resource bound was exceeded.
    ResourceLimit,
    /// The reconstruction graph is malformed (cycles, bad references, gaps).
    InvalidGraph,
    /// An entropy model descriptor is malformed.
    InvalidModel,
    /// An entropy channel failed to decode.
    EntropyDecode,
    /// A PDF physical/lexical structure error (Phase 3+).
    InvalidPdfStructure,
    /// An exact codec-replay candidate failed (Phase 6+).
    CodecReplay,
    /// A referenced external store object is missing (Phase 9+).
    MissingExternalObject,
    /// The coverage certificate is invalid (gaps or overlapping authorities).
    CoverageViolation,
    /// Materialization completed but did not equal the source.
    ReconstructionMismatch,
    /// The operation was cooperatively cancelled.
    Cancelled,
    /// A ZIP container structure error (Phase 12+): malformed or contradictory
    /// local/central/ZIP64 records, an ambiguous byte cover, a broken descriptor,
    /// multi-disk layout, or a hostile member name.
    InvalidZipStructure,
    /// An XML part structure error (Phase 12+, reserved): malformed or forbidden
    /// constructs (DOCTYPE/XXE, encoding tricks) in a package part.
    InvalidXmlStructure,
    /// An OPC/OCF package structure error (Phase 12+, reserved): a missing or
    /// ambiguous main part, content-type abuse, `mimetype` trick, or an ambiguous
    /// part-name identity.
    InvalidPackageStructure,
    /// A JSON structured-tree structure error (Phase 21.5+): a malformed token,
    /// an unterminated string/container, a bad escape, or a trailing byte after
    /// the single top-level value.
    InvalidJsonStructure,
    /// A YAML structured-tree structure error (Phase 21.6+): a malformed or
    /// unsupported construct (a bad indentation, an unterminated quoted scalar, a
    /// declined directive/explicit key/flow-context block scalar).
    InvalidYamlStructure,
    /// A CSV/TSV tabular structure error (Phase 21.7+): an unterminated quoted
    /// field, a byte after a closing quote that is neither a delimiter nor an end
    /// of record, or an input that is not a table under either delimiter.
    InvalidCsvStructure,
    /// A Markdown prose structure error (Phase 21.8+): a malformed or declined
    /// construct (an unterminated fenced code block close, a bad reference
    /// definition, or an input that carries no structural mark).
    InvalidMarkdownStructure,
    /// An HTML document structure error (Phase 21.10+): a forbidden construct
    /// (a DOCTYPE with an internal subset), a non-UTF-8/UTF-16/NUL byte string, or
    /// an input that carries no HTML structure. Malformed HTML is otherwise
    /// **recovered** by the adapter, never a panic.
    InvalidHtmlStructure,
    /// A TOML structure error (Phase 21.11+): a malformed token, an unterminated
    /// string/array/inline table, an invalid number or escape, or a violation of
    /// TOML's duplicate-key/redefinition rules (which this adapter **enforces**).
    InvalidTomlStructure,
    /// A JSONL/NDJSON structure error (Phase 21.12+): a non-blank line that is not
    /// exactly one JSON value, too few record lines to distinguish the source from a
    /// single JSON value, or a non-newline-separated bag of JSON values.
    InvalidJsonlStructure,
    /// An EML/MIME message structure error (Phase 21.13+): no RFC 5322 header block,
    /// a malformed header line, a `multipart/*` without a boundary, or an invalid
    /// quoted-printable/base64 escape.
    InvalidEmlStructure,
    /// An Apache Parquet structure error (Phase 21.14+): missing/inconsistent `PAR1`
    /// magic, an inconsistent footer length, a truncated Thrift-Compact footer, a
    /// page or column-chunk span outside the file, or a value count that disagrees
    /// with the page layout.
    InvalidParquetStructure,
    /// An Apache Arrow IPC structure error (Phase 21.16+): missing/inconsistent
    /// `ARROW1` magic, a truncated or out-of-bounds Flatbuffers metadata table, an
    /// invalid message framing, or a buffer span outside the message body.
    InvalidArrowStructure,
    /// A JSON5/JSONC structure error (Phase 21.17.1+): a malformed token, an
    /// unterminated string/comment/container, a bad escape or identifier, or a
    /// trailing byte after the single top-level value.
    InvalidJson5Structure,
    /// A CBOR structure error (Phase 21.18+): a malformed head (reserved additional
    /// information, a non-minimal simple value), a truncated item, trailing bytes
    /// after the single top-level item, an unterminated indefinite-length item, a
    /// map key with no value, or a text string that is not valid UTF-8.
    InvalidCborStructure,
    /// A MessagePack structure error (Phase 21.19+): a malformed head (the never-used
    /// `0xc1` byte), a truncated item, trailing bytes after the single top-level item,
    /// a map key with no value, an over-long declared length, or a `str` that is not
    /// valid UTF-8.
    InvalidMsgpackStructure,
    /// A config-family structure error (Phase 21.20+): a line that is not a valid
    /// INI/`.env`/`.properties` construct under the detected dialect, an unterminated
    /// quoted value, or a source that carries no config-family dialect signal at all
    /// (so it stays `Opaque`).
    InvalidConfigStructure,
    /// An RSS/Atom feed structure error (Phase 21.21+): a source that is not
    /// well-formed XML, an `<rss>` root with no `<channel>`, an Atom `<feed>` whose
    /// namespace is not the Atom URI or which carries no `<entry>`, or a root that is
    /// not an RSS/Atom feed (so it stays `Xml`/`Opaque`).
    InvalidFeedStructure,
    /// A GeoJSON structure error (Phase 21.22+): a source that is not a single JSON
    /// value, a root that is not an object with a string `"type"` member, a `"type"`
    /// that is not one of the nine GeoJSON type names, or a type-consistent object
    /// whose shape is not consistent (a geometry with no array `coordinates`/`geometries`,
    /// a `Feature` with no `geometry`/`properties`, a `FeatureCollection` with no array
    /// `features`) — so it stays `Json`/`Opaque` rather than being claimed.
    InvalidGeojsonStructure,
    /// A KML/GPX geospatial structure error (Phase 21.23+): a source that is not
    /// well-formed XML, a `<kml>` root not in the KML namespace or with no
    /// `Document`/`Folder`/`Placemark` child, a `<gpx>` root not in the GPX namespace
    /// or with no `metadata`/`wpt`/`rte`/`trk` child, or a root that is not a KML/GPX
    /// document (so it stays `Xml`/`Opaque`).
    InvalidGisStructure,
    /// A Jupyter notebook structure error (Phase 21.24+): a source that is not a single
    /// JSON value, a root that is not an object with a plain non-negative integer-literal
    /// `nbformat` (≥ 1) and an array `cells`, a cell that is not an object with a string
    /// `cell_type`, a present-but-malformed recognized cell field (a `source` that is not
    /// a string or array of strings, an `execution_count` that is neither a number nor
    /// `null`, a non-object `metadata`/`attachments`, an `outputs` that is not an array of
    /// objects with a string `output_type`), or a non-object root `metadata` — so it stays
    /// `Json`/`Opaque` rather than being claimed.
    InvalidNotebookStructure,
    /// A fixed-width structure error (Phase 21.25+): a source that carries no strong
    /// fixed-width signal (fewer than the minimum sampled records, records that do not
    /// share an identical byte width, fewer than two non-empty columns, or an interior
    /// column gap narrower than the minimum), or a document whose later records
    /// disagree with the sampled layout — so it stays `Opaque` rather than being
    /// claimed.
    InvalidFixedWidthStructure,
    /// A reStructuredText structure error (Phase 21.26.1+): a source that carries no
    /// reST-specific signal (an explicit markup start, a grid/simple table, a field
    /// list, or an admissible section adornment), a malformed directive/marker, or an
    /// input that exceeds a cap — so it stays `Opaque` rather than being claimed.
    InvalidRstStructure,
    /// An AsciiDoc structure error (Phase 21.26.2+): a source that carries no
    /// AsciiDoc-specific signal (a document title followed by a block, a `==`+ section,
    /// a `|===` table, a complete delimited block, or a block attribute line followed by
    /// a block), a malformed attribute/list/macro marker, or an input that exceeds a cap
    /// — so it stays `Opaque` rather than being claimed.
    InvalidAsciidocStructure,
    /// An MDX structure error (Phase 21.26.3+): a source that carries no MDX-specific
    /// signal (a top-level ESM statement, a JSX component/fragment/attribute, or a
    /// whole-line block expression), an unbalanced/ambiguous brace in a non-code
    /// position, a malformed JSX/ESM construct, or an input that exceeds a cap — so it
    /// stays `Opaque` rather than being claimed.
    InvalidMdxStructure,
    /// An MHTML structure error (Phase 21.27+): a source that is not a well-formed MIME
    /// message, a root that is not `multipart/related`, a `multipart/related` document
    /// that carries no MHTML-specific signal (`Snapshot-Content-Location`/`Content-Base`,
    /// or a `From`+`Subject` envelope with a `text/html` part), a root part that cannot
    /// be decoded/parsed as HTML, or an input that exceeds a cap — so it stays
    /// `Eml`/`Html`/`Opaque` rather than being claimed.
    InvalidMhtmlStructure,
    /// A bug in this implementation; never a description of malformed input.
    InternalInvariant,
}

impl ErrorClass {
    /// Stable, documented CLI exit code for this class.
    pub const fn exit_code(self) -> i32 {
        match self {
            ErrorClass::Io => 3,
            ErrorClass::Usage => 2,
            ErrorClass::InvalidContainer => 4,
            ErrorClass::UnsupportedVersion => 5,
            ErrorClass::UnsupportedFeature => 6,
            ErrorClass::IntegrityMismatch => 7,
            ErrorClass::ResourceLimit => 8,
            ErrorClass::InvalidGraph => 9,
            ErrorClass::InvalidModel => 10,
            ErrorClass::EntropyDecode => 11,
            ErrorClass::InvalidPdfStructure => 12,
            ErrorClass::CodecReplay => 13,
            ErrorClass::MissingExternalObject => 14,
            ErrorClass::CoverageViolation => 15,
            ErrorClass::ReconstructionMismatch => 16,
            ErrorClass::Cancelled => 17,
            ErrorClass::InvalidZipStructure => 18,
            ErrorClass::InvalidXmlStructure => 19,
            ErrorClass::InvalidPackageStructure => 20,
            ErrorClass::InvalidJsonStructure => 21,
            ErrorClass::InvalidYamlStructure => 22,
            ErrorClass::InvalidCsvStructure => 23,
            ErrorClass::InvalidMarkdownStructure => 24,
            ErrorClass::InvalidHtmlStructure => 25,
            ErrorClass::InvalidTomlStructure => 26,
            ErrorClass::InvalidJsonlStructure => 27,
            ErrorClass::InvalidEmlStructure => 28,
            ErrorClass::InvalidParquetStructure => 29,
            ErrorClass::InvalidArrowStructure => 30,
            ErrorClass::InvalidJson5Structure => 31,
            ErrorClass::InvalidCborStructure => 32,
            ErrorClass::InvalidMsgpackStructure => 33,
            ErrorClass::InvalidConfigStructure => 34,
            ErrorClass::InvalidFeedStructure => 35,
            ErrorClass::InvalidGeojsonStructure => 36,
            ErrorClass::InvalidGisStructure => 37,
            ErrorClass::InvalidNotebookStructure => 38,
            ErrorClass::InvalidFixedWidthStructure => 39,
            ErrorClass::InvalidRstStructure => 40,
            ErrorClass::InvalidAsciidocStructure => 41,
            ErrorClass::InvalidMdxStructure => 42,
            ErrorClass::InvalidMhtmlStructure => 43,
            ErrorClass::InternalInvariant => 70,
        }
    }

    /// Short stable identifier used in receipts and machine-readable output.
    pub const fn as_str(self) -> &'static str {
        match self {
            ErrorClass::Io => "Io",
            ErrorClass::Usage => "Usage",
            ErrorClass::InvalidContainer => "InvalidContainer",
            ErrorClass::UnsupportedVersion => "UnsupportedVersion",
            ErrorClass::UnsupportedFeature => "UnsupportedFeature",
            ErrorClass::IntegrityMismatch => "IntegrityMismatch",
            ErrorClass::ResourceLimit => "ResourceLimit",
            ErrorClass::InvalidGraph => "InvalidGraph",
            ErrorClass::InvalidModel => "InvalidModel",
            ErrorClass::EntropyDecode => "EntropyDecode",
            ErrorClass::InvalidPdfStructure => "InvalidPdfStructure",
            ErrorClass::CodecReplay => "CodecReplay",
            ErrorClass::MissingExternalObject => "MissingExternalObject",
            ErrorClass::CoverageViolation => "CoverageViolation",
            ErrorClass::ReconstructionMismatch => "ReconstructionMismatch",
            ErrorClass::Cancelled => "Cancelled",
            ErrorClass::InvalidZipStructure => "InvalidZipStructure",
            ErrorClass::InvalidXmlStructure => "InvalidXmlStructure",
            ErrorClass::InvalidPackageStructure => "InvalidPackageStructure",
            ErrorClass::InvalidJsonStructure => "InvalidJsonStructure",
            ErrorClass::InvalidYamlStructure => "InvalidYamlStructure",
            ErrorClass::InvalidCsvStructure => "InvalidCsvStructure",
            ErrorClass::InvalidMarkdownStructure => "InvalidMarkdownStructure",
            ErrorClass::InvalidHtmlStructure => "InvalidHtmlStructure",
            ErrorClass::InvalidTomlStructure => "InvalidTomlStructure",
            ErrorClass::InvalidJsonlStructure => "InvalidJsonlStructure",
            ErrorClass::InvalidEmlStructure => "InvalidEmlStructure",
            ErrorClass::InvalidParquetStructure => "InvalidParquetStructure",
            ErrorClass::InvalidArrowStructure => "InvalidArrowStructure",
            ErrorClass::InvalidJson5Structure => "InvalidJson5Structure",
            ErrorClass::InvalidCborStructure => "InvalidCborStructure",
            ErrorClass::InvalidMsgpackStructure => "InvalidMsgpackStructure",
            ErrorClass::InvalidConfigStructure => "InvalidConfigStructure",
            ErrorClass::InvalidFeedStructure => "InvalidFeedStructure",
            ErrorClass::InvalidGeojsonStructure => "InvalidGeojsonStructure",
            ErrorClass::InvalidGisStructure => "InvalidGisStructure",
            ErrorClass::InvalidNotebookStructure => "InvalidNotebookStructure",
            ErrorClass::InvalidFixedWidthStructure => "InvalidFixedWidthStructure",
            ErrorClass::InvalidRstStructure => "InvalidRstStructure",
            ErrorClass::InvalidAsciidocStructure => "InvalidAsciidocStructure",
            ErrorClass::InvalidMdxStructure => "InvalidMdxStructure",
            ErrorClass::InvalidMhtmlStructure => "InvalidMhtmlStructure",
            ErrorClass::InternalInvariant => "InternalInvariant",
        }
    }
}

/// A typed, classified error.
#[derive(Debug, Clone)]
pub struct Error {
    class: ErrorClass,
    message: String,
}

impl Error {
    /// Construct an error of the given class with a human-readable message.
    pub fn new(class: ErrorClass, message: impl Into<String>) -> Self {
        Error {
            class,
            message: message.into(),
        }
    }

    /// The failure class.
    pub fn class(&self) -> ErrorClass {
        self.class
    }

    /// The stable CLI exit code for this error's class.
    pub fn exit_code(&self) -> i32 {
        self.class.exit_code()
    }

    /// The human-readable message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.class.as_str(), self.message)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::new(ErrorClass::Io, e.to_string())
    }
}

macro_rules! ctor {
    ($name:ident, $class:ident) => {
        /// Construct an error of this class.
        pub fn $name(message: impl Into<String>) -> Self {
            Error::new(ErrorClass::$class, message)
        }
    };
}

impl Error {
    ctor!(io, Io);
    ctor!(usage, Usage);
    ctor!(invalid_container, InvalidContainer);
    ctor!(unsupported_version, UnsupportedVersion);
    ctor!(unsupported_feature, UnsupportedFeature);
    ctor!(integrity_mismatch, IntegrityMismatch);
    ctor!(resource_limit, ResourceLimit);
    ctor!(invalid_graph, InvalidGraph);
    ctor!(invalid_model, InvalidModel);
    ctor!(entropy_decode, EntropyDecode);
    ctor!(invalid_pdf_structure, InvalidPdfStructure);
    ctor!(codec_replay, CodecReplay);
    ctor!(missing_external_object, MissingExternalObject);
    ctor!(coverage_violation, CoverageViolation);
    ctor!(reconstruction_mismatch, ReconstructionMismatch);
    ctor!(cancelled, Cancelled);
    ctor!(invalid_zip_structure, InvalidZipStructure);
    ctor!(invalid_xml_structure, InvalidXmlStructure);
    ctor!(invalid_package_structure, InvalidPackageStructure);
    ctor!(invalid_json_structure, InvalidJsonStructure);
    ctor!(invalid_yaml_structure, InvalidYamlStructure);
    ctor!(invalid_csv_structure, InvalidCsvStructure);
    ctor!(invalid_markdown_structure, InvalidMarkdownStructure);
    ctor!(invalid_html_structure, InvalidHtmlStructure);
    ctor!(invalid_toml_structure, InvalidTomlStructure);
    ctor!(invalid_jsonl_structure, InvalidJsonlStructure);
    ctor!(invalid_eml_structure, InvalidEmlStructure);
    ctor!(invalid_parquet_structure, InvalidParquetStructure);
    ctor!(invalid_arrow_structure, InvalidArrowStructure);
    ctor!(invalid_json5_structure, InvalidJson5Structure);
    ctor!(invalid_cbor_structure, InvalidCborStructure);
    ctor!(invalid_msgpack_structure, InvalidMsgpackStructure);
    ctor!(invalid_config_structure, InvalidConfigStructure);
    ctor!(invalid_feed_structure, InvalidFeedStructure);
    ctor!(invalid_geojson_structure, InvalidGeojsonStructure);
    ctor!(invalid_gis_structure, InvalidGisStructure);
    ctor!(invalid_notebook_structure, InvalidNotebookStructure);
    ctor!(invalid_fixedwidth_structure, InvalidFixedWidthStructure);
    ctor!(invalid_rst_structure, InvalidRstStructure);
    ctor!(invalid_asciidoc_structure, InvalidAsciidocStructure);
    ctor!(invalid_mdx_structure, InvalidMdxStructure);
    ctor!(invalid_mhtml_structure, InvalidMhtmlStructure);
    ctor!(internal_invariant, InternalInvariant);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_are_distinct_and_stable() {
        // A representative set; guards accidental collisions.
        let classes = [
            ErrorClass::Io,
            ErrorClass::Usage,
            ErrorClass::InvalidContainer,
            ErrorClass::UnsupportedVersion,
            ErrorClass::UnsupportedFeature,
            ErrorClass::IntegrityMismatch,
            ErrorClass::ResourceLimit,
            ErrorClass::InvalidGraph,
            ErrorClass::InvalidModel,
            ErrorClass::EntropyDecode,
            ErrorClass::InvalidPdfStructure,
            ErrorClass::CodecReplay,
            ErrorClass::MissingExternalObject,
            ErrorClass::CoverageViolation,
            ErrorClass::ReconstructionMismatch,
            ErrorClass::Cancelled,
            ErrorClass::InvalidZipStructure,
            ErrorClass::InvalidXmlStructure,
            ErrorClass::InvalidPackageStructure,
            ErrorClass::InvalidJsonStructure,
            ErrorClass::InvalidYamlStructure,
            ErrorClass::InvalidCsvStructure,
            ErrorClass::InvalidMarkdownStructure,
            ErrorClass::InvalidHtmlStructure,
            ErrorClass::InvalidTomlStructure,
            ErrorClass::InternalInvariant,
        ];
        let mut codes: Vec<i32> = classes.iter().map(|c| c.exit_code()).collect();
        let n = codes.len();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), n, "exit codes must be unique");
        assert_eq!(ErrorClass::InvalidContainer.exit_code(), 4);
    }

    #[test]
    fn display_includes_class() {
        let e = Error::invalid_container("bad magic");
        assert_eq!(e.to_string(), "InvalidContainer: bad magic");
    }
}
