//! Integrity primitives.
//!
//! Two digests with two distinct roles, per the architecture:
//!
//! * **CRC32C (Castagnoli)** guards physical framing and header bytes. It is a
//!   corruption detector, not a security boundary.
//! * **SHA-256** is the durable archival identity of the whole reconstructed
//!   source. Byte comparison remains the court authority during development;
//!   the digest is the fast, portable receipt.

use sha2::{Digest, Sha256};

/// A streaming SHA-256 hasher that hides the dependency's concrete type.
pub struct Sha256Hasher {
    inner: Sha256,
}

impl Sha256Hasher {
    /// Start a new hash.
    pub fn new() -> Self {
        Sha256Hasher {
            inner: Sha256::new(),
        }
    }

    /// Absorb bytes.
    pub fn update(&mut self, data: &[u8]) {
        self.inner.update(data);
    }

    /// Finish and return the 32-byte digest.
    pub fn finalize(self) -> [u8; 32] {
        let out = self.inner.finalize();
        let mut buf = [0u8; 32];
        buf.copy_from_slice(&out);
        buf
    }
}

impl Default for Sha256Hasher {
    fn default() -> Self {
        Self::new()
    }
}

/// SHA-256 of a byte slice.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h = Sha256Hasher::new();
    h.update(data);
    h.finalize()
}

/// Lower-case hex encoding (no external dependency).
pub fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0F) as usize] as char);
    }
    s
}

/// CRC-32C (Castagnoli), reflected, init/xorout `0xFFFFFFFF`.
///
/// Used only for framing/header corruption detection.
pub fn crc32c(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        let idx = ((crc ^ b as u32) & 0xFF) as usize;
        crc = (crc >> 8) ^ CRC32C_TABLE[idx];
    }
    !crc
}

const fn build_crc32c_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0usize;
    while i < 256 {
        let mut crc = i as u32;
        let mut bit = 0;
        while bit < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0x82F6_3B78;
            } else {
                crc >>= 1;
            }
            bit += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}

static CRC32C_TABLE: [u32; 256] = build_crc32c_table();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32c_standard_vector() {
        // The canonical CRC-32C check value for "123456789".
        assert_eq!(crc32c(b"123456789"), 0xE306_9283);
        assert_eq!(crc32c(b""), 0x0000_0000);
    }

    #[test]
    fn sha256_standard_vectors() {
        assert_eq!(
            to_hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            to_hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn streaming_matches_oneshot() {
        let data = b"the quick brown fox jumps over the lazy dog";
        let mut h = Sha256Hasher::new();
        h.update(&data[..10]);
        h.update(&data[10..]);
        assert_eq!(h.finalize(), sha256(data));
    }

    #[test]
    fn hex_roundtrip_shape() {
        assert_eq!(to_hex(&[0x00, 0x0f, 0xff]), "000fff");
    }
}
