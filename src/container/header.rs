//! The fixed `.voldoc` header.
//!
//! A descriptor begins with exactly [`HEADER_LEN`] bytes. The header is
//! self-checked with CRC32C so that framing corruption is detected before any
//! record parsing. Unknown *mandatory* feature bits fail closed; unknown
//! optional bits are recorded and ignored.

use crate::error::{Error, Result};
use crate::integrity::crc32c;
use crate::{EXACTNESS_PROFILE_EXACT_BYTES, SOURCE_FORMAT_OPAQUE};

/// Size of the fixed header in bytes.
pub const HEADER_LEN: usize = 64;

/// Container magic: `VOLDOC` followed by a 0x1A text-EOF marker and NUL.
pub const MAGIC: [u8; 8] = *b"VOLDOC\x1A\x00";

/// Major format version implemented by this build (provisional pre-1.0).
pub const FORMAT_MAJOR: u16 = 0;
/// Minor format version implemented by this build.
pub const FORMAT_MINOR: u16 = 1;

/// Mandatory feature bit: the descriptor carries at least one `DEFLATE_REPLAY`
/// op and so requires a decoder built with exact-DEFLATE-replay support.
pub const FEATURE_DEFLATE_REPLAY: u32 = 1 << 0;

/// Optional feature bit: the descriptor carries an `OBSERVATION_INDEX` record
/// and so advertises a partial-decode lane.
///
/// Optional bits are ignorable: a decoder built without partial-decode support
/// still materializes the source exactly, because the reconstruction program
/// alone is complete. Exactness never requires this bit.
pub const FEATURE_OBSERVATION_INDEX: u32 = 1 << 0;

/// Feature bits this build understands and supports.
///
/// Without the `deflate-replay` cargo feature the replay bit is *not* supported,
/// so a descriptor that declares it fails closed at header validation with
/// [`crate::ErrorClass::UnsupportedFeature`] rather than being reinterpreted.
#[cfg(feature = "deflate-replay")]
pub const SUPPORTED_MANDATORY_FEATURES: u32 = FEATURE_DEFLATE_REPLAY;
/// Feature bits this build understands and supports (no replay support).
#[cfg(not(feature = "deflate-replay"))]
pub const SUPPORTED_MANDATORY_FEATURES: u32 = 0;

/// The parsed, validated fixed header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Major version.
    pub major: u16,
    /// Minor version.
    pub minor: u16,
    /// Feature bits a decoder *must* understand.
    pub mandatory_features: u32,
    /// Feature bits a decoder *may* ignore.
    pub optional_features: u32,
    /// Exactness profile selector (only `EXACT_BYTES` is normative).
    pub exactness_profile: u8,
    /// Source-format class selector.
    pub source_format: u8,
    /// First 16 bytes of SHA-256 over the universe declaration string.
    pub universe_id: [u8; 16],
    /// Declared length of the fully reconstructed source.
    pub declared_source_len: u64,
}

impl Header {
    /// Build a header for the current format version.
    pub fn new(
        universe_id: [u8; 16],
        declared_source_len: u64,
        exactness_profile: u8,
        source_format: u8,
    ) -> Self {
        Header {
            major: FORMAT_MAJOR,
            minor: FORMAT_MINOR,
            mandatory_features: 0,
            optional_features: 0,
            exactness_profile,
            source_format,
            universe_id,
            declared_source_len,
        }
    }

    /// Encode the header to exactly [`HEADER_LEN`] bytes, computing the CRC.
    pub fn encode(&self) -> [u8; HEADER_LEN] {
        let mut b = [0u8; HEADER_LEN];
        b[0..8].copy_from_slice(&MAGIC);
        b[8..10].copy_from_slice(&self.major.to_le_bytes());
        b[10..12].copy_from_slice(&self.minor.to_le_bytes());
        b[12..16].copy_from_slice(&self.mandatory_features.to_le_bytes());
        b[16..20].copy_from_slice(&self.optional_features.to_le_bytes());
        b[20] = self.exactness_profile;
        b[21] = self.source_format;
        // b[22..24] reserved_a = 0
        b[24..40].copy_from_slice(&self.universe_id);
        b[40..48].copy_from_slice(&self.declared_source_len.to_le_bytes());
        // b[48..60] reserved_b = 0
        let crc = crc32c(&b[0..60]);
        b[60..64].copy_from_slice(&crc.to_le_bytes());
        b
    }

    /// Decode and validate the header from the start of `bytes`.
    pub fn decode(bytes: &[u8]) -> Result<Header> {
        if bytes.len() < HEADER_LEN {
            return Err(Error::invalid_container(format!(
                "input is {} bytes; need at least {HEADER_LEN} for a header",
                bytes.len()
            )));
        }
        let b = &bytes[0..HEADER_LEN];
        if b[0..8] != MAGIC {
            return Err(Error::invalid_container("bad container magic"));
        }
        let want_crc = u32::from_le_bytes([b[60], b[61], b[62], b[63]]);
        let got_crc = crc32c(&b[0..60]);
        if want_crc != got_crc {
            return Err(Error::invalid_container(format!(
                "header CRC32C mismatch: declared {want_crc:#010x}, computed {got_crc:#010x}"
            )));
        }
        if b[22] != 0 || b[23] != 0 {
            return Err(Error::invalid_container("header reserved_a must be zero"));
        }
        if b[48..60].iter().any(|&x| x != 0) {
            return Err(Error::invalid_container("header reserved_b must be zero"));
        }

        let major = u16::from_le_bytes([b[8], b[9]]);
        let minor = u16::from_le_bytes([b[10], b[11]]);
        if major != FORMAT_MAJOR || minor > FORMAT_MINOR {
            return Err(Error::unsupported_version(format!(
                "container version {major}.{minor} is not supported by this build ({FORMAT_MAJOR}.{FORMAT_MINOR})"
            )));
        }

        let mandatory_features = u32::from_le_bytes([b[12], b[13], b[14], b[15]]);
        let unsupported = mandatory_features & !SUPPORTED_MANDATORY_FEATURES;
        if unsupported != 0 {
            return Err(Error::unsupported_feature(format!(
                "descriptor requires unsupported mandatory feature bits {unsupported:#010x}"
            )));
        }

        let exactness_profile = b[20];
        if exactness_profile != EXACTNESS_PROFILE_EXACT_BYTES {
            return Err(Error::unsupported_feature(format!(
                "exactness profile {exactness_profile} is not the normative EXACT_BYTES profile"
            )));
        }
        let source_format = b[21];

        let mut universe_id = [0u8; 16];
        universe_id.copy_from_slice(&b[24..40]);
        let declared_source_len =
            u64::from_le_bytes([b[40], b[41], b[42], b[43], b[44], b[45], b[46], b[47]]);

        Ok(Header {
            major,
            minor,
            mandatory_features,
            optional_features: u32::from_le_bytes([b[16], b[17], b[18], b[19]]),
            exactness_profile,
            source_format,
            universe_id,
            declared_source_len,
        })
    }

    /// True if this header describes the opaque source-format class.
    pub fn is_opaque(&self) -> bool {
        self.source_format == SOURCE_FORMAT_OPAQUE
    }

    /// True if this build has an adapter for the declared source-format class.
    ///
    /// Unknown classes fail closed: a descriptor naming a class this build does
    /// not implement is refused rather than reinterpreted as opaque.
    pub fn source_format_supported(&self) -> bool {
        matches!(self.source_format, 0 | 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let h = Header::new(
            [7u8; 16],
            1234,
            EXACTNESS_PROFILE_EXACT_BYTES,
            SOURCE_FORMAT_OPAQUE,
        );
        let enc = h.encode();
        assert_eq!(enc.len(), HEADER_LEN);
        let dec = Header::decode(&enc).unwrap();
        assert_eq!(h, dec);
    }

    #[test]
    fn rejects_bad_magic() {
        let mut enc = Header::new([0u8; 16], 0, 0, 0).encode();
        enc[0] = b'X';
        match Header::decode(&enc) {
            Err(e) => assert_eq!(e.class(), crate::ErrorClass::InvalidContainer),
            Ok(_) => panic!("expected failure"),
        }
    }

    #[test]
    fn detects_corruption_via_crc() {
        let mut enc = Header::new([0u8; 16], 5, 0, 0).encode();
        enc[41] ^= 0x01; // flip a byte inside declared_source_len
        let e = Header::decode(&enc).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::InvalidContainer);
    }

    #[test]
    fn rejects_truncated() {
        let e = Header::decode(&[0u8; 10]).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::InvalidContainer);
    }

    #[test]
    fn rejects_unknown_mandatory_feature() {
        let mut h = Header::new([0u8; 16], 0, 0, 0);
        h.mandatory_features = 0x8000_0000;
        let e = Header::decode(&h.encode()).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::UnsupportedFeature);
    }

    #[test]
    fn rejects_future_major_version() {
        let mut h = Header::new([0u8; 16], 0, 0, 0);
        h.major = 9;
        let e = Header::decode(&h.encode()).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::UnsupportedVersion);
    }
}
