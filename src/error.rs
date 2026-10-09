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
