//! Typed entropy-channel descriptor: the canonical, self-describing wire form
//! of a single rANS channel capsule.
//!
//! A channel is never a bare seed. It carries the coder identity, the model it
//! consumes, the decoder initial state, the renormalization payload, and the
//! exact symbol/decoded counts. [`encode`](EntropyChannelDescriptor::encode) and
//! [`decode`](EntropyChannelDescriptor::decode) are exact inverses; decoding
//! validates every field and never panics on hostile input.

use crate::entropy::model::{MAX_SCALE_BITS, MIN_SCALE_BITS};
use crate::error::{Error, Result};
use crate::limits::Limits;

/// Coder selector: single-lane order-0 byte rANS.
pub const CODER_ORDER0_BYTE_RANS: u8 = 1;
/// Coder wire version for [`CODER_ORDER0_BYTE_RANS`].
pub const CODER_VERSION_1: u16 = 1;

/// Fixed bytes preceding the payload in the wire encoding.
const WIRE_HEADER_LEN: usize = 33;

/// A complete decoder-entry descriptor for one entropy channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntropyChannelDescriptor {
    /// Coder selector (must be [`CODER_ORDER0_BYTE_RANS`]).
    pub coder: u8,
    /// Coder wire version (must be [`CODER_VERSION_1`]).
    pub coder_version: u16,
    /// Model scale bits; must match the referenced model.
    pub scale_bits: u8,
    /// Lane count (only single-lane channels are supported).
    pub lane_count: u8,
    /// Index into the descriptor's entropy-model table.
    pub model_id: u32,
    /// Number of symbols encoded.
    pub symbol_count: u64,
    /// Exact decoded length in bytes.
    pub decoded_length: u64,
    /// Decoder initial state (scalar single lane).
    pub initial_state: u32,
    /// Renormalization payload in forward decoder-consumption order.
    pub payload: Vec<u8>,
}

impl EntropyChannelDescriptor {
    /// Canonical wire encoding (little-endian):
    /// `[coder u8][coder_version u16][scale_bits u8][lane_count u8]`
    /// `[model_id u32][symbol_count u64][decoded_length u64]`
    /// `[initial_state u32][payload_len u32][payload]`.
    pub fn encode(&self) -> Result<Vec<u8>> {
        let payload_len = u32::try_from(self.payload.len())
            .map_err(|_| Error::resource_limit("entropy channel payload exceeds 4 GiB"))?;
        let mut out = Vec::with_capacity(WIRE_HEADER_LEN + self.payload.len());
        out.push(self.coder);
        out.extend_from_slice(&self.coder_version.to_le_bytes());
        out.push(self.scale_bits);
        out.push(self.lane_count);
        out.extend_from_slice(&self.model_id.to_le_bytes());
        out.extend_from_slice(&self.symbol_count.to_le_bytes());
        out.extend_from_slice(&self.decoded_length.to_le_bytes());
        out.extend_from_slice(&self.initial_state.to_le_bytes());
        out.extend_from_slice(&payload_len.to_le_bytes());
        out.extend_from_slice(&self.payload);
        Ok(out)
    }

    /// Parse and validate a canonical channel descriptor; `bytes` must be
    /// exactly consumed (no trailing bytes, no truncation).
    pub fn decode(bytes: &[u8], limits: Limits) -> Result<EntropyChannelDescriptor> {
        if bytes.len() < WIRE_HEADER_LEN {
            return Err(Error::invalid_container(
                "entropy channel descriptor shorter than header",
            ));
        }
        let coder = bytes[0];
        if coder != CODER_ORDER0_BYTE_RANS {
            return Err(Error::unsupported_feature(format!(
                "entropy coder {coder} is not supported"
            )));
        }
        let coder_version = u16::from_le_bytes([bytes[1], bytes[2]]);
        if coder_version != CODER_VERSION_1 {
            return Err(Error::unsupported_version(format!(
                "entropy coder version {coder_version}, expected {CODER_VERSION_1}"
            )));
        }
        let scale_bits = bytes[3];
        if !(MIN_SCALE_BITS..=MAX_SCALE_BITS).contains(&scale_bits) {
            let (min, max) = (MIN_SCALE_BITS, MAX_SCALE_BITS);
            return Err(Error::invalid_model(format!(
                "scale_bits {scale_bits} outside {min}..={max}"
            )));
        }
        let lane_count = bytes[4];
        if lane_count != 1 {
            return Err(Error::unsupported_feature(format!(
                "lane_count {lane_count}, only single-lane channels are supported"
            )));
        }
        let model_id = u32::from_le_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]);
        let symbol_count = u64::from_le_bytes([
            bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15], bytes[16],
        ]);
        let decoded_length = u64::from_le_bytes([
            bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22], bytes[23], bytes[24],
        ]);
        let initial_state = u32::from_le_bytes([bytes[25], bytes[26], bytes[27], bytes[28]]);
        let payload_len = u32::from_le_bytes([bytes[29], bytes[30], bytes[31], bytes[32]]);

        if symbol_count > limits.max_channel_symbols {
            return Err(Error::resource_limit(format!(
                "symbol_count {symbol_count} exceeds limit {}",
                limits.max_channel_symbols
            )));
        }
        if decoded_length > limits.max_output_bytes {
            return Err(Error::resource_limit(format!(
                "decoded_length {decoded_length} exceeds limit {}",
                limits.max_output_bytes
            )));
        }
        if payload_len > limits.max_record_len {
            return Err(Error::resource_limit(format!(
                "payload length {payload_len} exceeds limit {}",
                limits.max_record_len
            )));
        }
        let expected = WIRE_HEADER_LEN
            .checked_add(payload_len as usize)
            .ok_or_else(|| Error::resource_limit("entropy channel length overflow"))?;
        if bytes.len() != expected {
            return Err(Error::invalid_container(
                "entropy channel payload length mismatch (trailing or truncated)",
            ));
        }

        Ok(EntropyChannelDescriptor {
            coder,
            coder_version,
            scale_bits,
            lane_count,
            model_id,
            symbol_count,
            decoded_length,
            initial_state,
            payload: bytes[WIRE_HEADER_LEN..].to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorClass;

    fn descriptor() -> EntropyChannelDescriptor {
        EntropyChannelDescriptor {
            coder: CODER_ORDER0_BYTE_RANS,
            coder_version: CODER_VERSION_1,
            scale_bits: 12,
            lane_count: 1,
            model_id: 7,
            symbol_count: 1234,
            decoded_length: 1234,
            initial_state: 0xABCD_1234,
            payload: vec![1, 2, 3, 4, 5, 6, 7, 8],
        }
    }

    #[test]
    fn wire_length_is_header_plus_payload() {
        let d = descriptor();
        let bytes = d.encode().unwrap();
        assert_eq!(bytes.len(), 33 + d.payload.len());
    }

    #[test]
    fn roundtrip() {
        let d = descriptor();
        let bytes = d.encode().unwrap();
        let back = EntropyChannelDescriptor::decode(&bytes, Limits::DEFAULT).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn empty_payload_roundtrips() {
        let mut d = descriptor();
        d.payload.clear();
        let bytes = d.encode().unwrap();
        assert_eq!(bytes.len(), 33);
        let back = EntropyChannelDescriptor::decode(&bytes, Limits::DEFAULT).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn rejects_unknown_coder() {
        let mut d = descriptor();
        d.coder = 2;
        let bytes = d.encode().unwrap();
        let e = EntropyChannelDescriptor::decode(&bytes, Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), ErrorClass::UnsupportedFeature);
    }

    #[test]
    fn rejects_unknown_version() {
        let mut d = descriptor();
        d.coder_version = 2;
        let bytes = d.encode().unwrap();
        let e = EntropyChannelDescriptor::decode(&bytes, Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), ErrorClass::UnsupportedVersion);
    }

    #[test]
    fn rejects_bad_scale_bits() {
        for bits in [0u8, 16] {
            let mut d = descriptor();
            d.scale_bits = bits;
            let bytes = d.encode().unwrap();
            let e = EntropyChannelDescriptor::decode(&bytes, Limits::DEFAULT).unwrap_err();
            assert_eq!(e.class(), ErrorClass::InvalidModel);
        }
    }

    #[test]
    fn rejects_multilane() {
        let mut d = descriptor();
        d.lane_count = 2;
        let bytes = d.encode().unwrap();
        let e = EntropyChannelDescriptor::decode(&bytes, Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), ErrorClass::UnsupportedFeature);
    }

    #[test]
    fn rejects_truncated_header() {
        let e = EntropyChannelDescriptor::decode(&[0u8; 32], Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), ErrorClass::InvalidContainer);
    }

    #[test]
    fn rejects_trailing_bytes() {
        let mut bytes = descriptor().encode().unwrap();
        bytes.push(0);
        let e = EntropyChannelDescriptor::decode(&bytes, Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), ErrorClass::InvalidContainer);
    }

    #[test]
    fn rejects_truncated_payload() {
        let mut bytes = descriptor().encode().unwrap();
        bytes.pop();
        let e = EntropyChannelDescriptor::decode(&bytes, Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), ErrorClass::InvalidContainer);
    }

    #[test]
    fn enforces_payload_limit() {
        let mut d = descriptor();
        d.payload = vec![0u8; 100];
        let bytes = d.encode().unwrap();
        let limits = Limits {
            max_record_len: 10,
            ..Limits::DEFAULT
        };
        let e = EntropyChannelDescriptor::decode(&bytes, limits).unwrap_err();
        assert_eq!(e.class(), ErrorClass::ResourceLimit);
    }

    #[test]
    fn enforces_decoded_length_limit() {
        let mut d = descriptor();
        d.decoded_length = 1 << 30;
        let bytes = d.encode().unwrap();
        let limits = Limits {
            max_output_bytes: 1 << 20,
            ..Limits::DEFAULT
        };
        let e = EntropyChannelDescriptor::decode(&bytes, limits).unwrap_err();
        assert_eq!(e.class(), ErrorClass::ResourceLimit);
    }

    #[test]
    fn enforces_symbol_count_limit() {
        let mut d = descriptor();
        d.symbol_count = 1 << 30;
        let bytes = d.encode().unwrap();
        let limits = Limits {
            max_channel_symbols: 1 << 20,
            ..Limits::DEFAULT
        };
        let e = EntropyChannelDescriptor::decode(&bytes, limits).unwrap_err();
        assert_eq!(e.class(), ErrorClass::ResourceLimit);
    }
}
