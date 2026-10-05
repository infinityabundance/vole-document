//! Canonical entropy models: a deterministic, integer-only (no floating point)
//! frequency-table normalizer and its wire serialization.
//!
//! A model is a frequency table over a fixed byte alphabet ([`ALPHABET`] = 256)
//! whose entries sum to exactly `1 << scale_bits`. Normalization from raw symbol
//! counts is a *pure function of the counts and `scale_bits`*: identical inputs
//! always produce identical output bytes. This is the substrate rANS will later
//! consume; no entropy coder lives here yet.

use crate::error::{Error, Result};

/// Supported alphabet size (bytes).
pub const ALPHABET: usize = 256;
/// Model wire version.
pub const MODEL_VERSION_1: u8 = 1;
/// Minimum / maximum scale bits. Frequencies must fit in u16, so <= 15.
pub const MIN_SCALE_BITS: u8 = 1;
pub const MAX_SCALE_BITS: u8 = 15;

/// Bytes preceding the frequency table: `version`, `scale_bits`, `count`.
const MODEL_HEADER_LEN: usize = 4;
/// Total canonical encoded length: header plus `ALPHABET` little-endian u16s.
const MODEL_ENCODED_LEN: usize = MODEL_HEADER_LEN + ALPHABET * 2;

/// A normalized frequency table over the byte alphabet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntropyModel {
    /// Number of scale bits; the table sums to `1 << scale_bits`.
    pub scale_bits: u8,
    /// Exactly `ALPHABET` entries; sum == `1 << scale_bits`.
    pub frequencies: Vec<u32>,
}

impl EntropyModel {
    /// Total frequency (== `1 << scale_bits`).
    pub fn total(&self) -> u32 {
        1u32 << self.scale_bits
    }

    /// Sum of all frequencies as a widened integer (avoids debug overflow on
    /// hand-built, out-of-contract tables).
    fn frequency_sum(&self) -> u64 {
        self.frequencies.iter().map(|&f| u64::from(f)).sum()
    }

    /// Canonical normalization of observed symbol counts into a frequency table.
    ///
    /// Requirements (all hold):
    /// - integer-only; no floating point anywhere;
    /// - deterministic: same input yields identical output bytes;
    /// - every symbol with `count > 0` gets frequency `>= 1`;
    /// - symbols with `count == 0` get frequency `0`;
    /// - `sum(frequencies) == 1 << scale_bits` exactly;
    /// - if all counts are `0`, return the canonical uniform model.
    ///
    /// Algorithm: after granting each *present* symbol a guaranteed minimum of
    /// 1, the remaining budget is apportioned by the largest-remainder (Hare)
    /// method using `u128` products, with ties broken by lower symbol index.
    pub fn from_counts(counts: &[u64; ALPHABET], scale_bits: u8) -> Result<EntropyModel> {
        if !(MIN_SCALE_BITS..=MAX_SCALE_BITS).contains(&scale_bits) {
            let (min, max) = (MIN_SCALE_BITS, MAX_SCALE_BITS);
            return Err(Error::invalid_model(format!(
                "scale_bits {scale_bits} outside {min}..={max}"
            )));
        }
        let target: u32 = 1u32 << scale_bits;

        // 2. Present symbols.
        let present: Vec<usize> = (0..ALPHABET).filter(|&i| counts[i] > 0).collect();
        if present.is_empty() {
            return Self::uniform(scale_bits);
        }

        // 3. Alphabet (as a set of present symbols) must fit in the target.
        if present.len() as u64 > u64::from(target) {
            return Err(Error::invalid_model(format!(
                "{} present symbols exceed target {target}",
                present.len()
            )));
        }

        // 4. Guaranteed minimum of 1 for each present symbol.
        let mut frequencies = vec![0u32; ALPHABET];
        for &i in &present {
            frequencies[i] = 1;
        }
        let remaining: u32 = target - present.len() as u32;

        if remaining > 0 {
            let total: u128 = present.iter().map(|&i| u128::from(counts[i])).sum();

            // 5. Integer quota + remainder per present symbol.
            let mut quota_sum: u64 = 0;
            let mut rems: Vec<(usize, u128)> = Vec::with_capacity(present.len());
            for &i in &present {
                let product = u128::from(counts[i]) * u128::from(remaining);
                let quota = product / total;
                let rem = product % total;
                frequencies[i] += u32::try_from(quota)
                    .map_err(|_| Error::internal_invariant("quota exceeded 32-bit range"))?;
                quota_sum += u64::try_from(quota)
                    .map_err(|_| Error::internal_invariant("quota exceeded 64-bit range"))?;
                rems.push((i, rem));
            }
            let leftover: u64 = u64::from(remaining) - quota_sum;

            // 6. Largest remainder, ties by lower index. There are always at
            // least `leftover` symbols with a nonzero remainder, so this pass
            // consumes the whole leftover in practice.
            rems.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            let mut added: u64 = 0;
            for &(i, _) in &rems {
                if added >= leftover {
                    break;
                }
                frequencies[i] += 1;
                added += 1;
            }

            // Deterministic fallback, kept for total-sum safety. It is
            // unreachable for well-formed inputs (the remainder pass above
            // always suffices), and adds to the present symbol with the largest
            // count, breaking ties by lower index.
            while added < leftover {
                let i = present
                    .iter()
                    .copied()
                    .max_by_key(|&i| (counts[i], core::cmp::Reverse(i)))
                    .expect("present is non-empty");
                frequencies[i] += 1;
                added += 1;
            }
        }

        let model = EntropyModel {
            scale_bits,
            frequencies,
        };
        if model.frequency_sum() != u64::from(target) {
            return Err(Error::internal_invariant(
                "normalized frequencies do not sum to target",
            ));
        }
        Ok(model)
    }

    /// Uniform model with every symbol getting an equal frequency.
    ///
    /// Requires `scale_bits >= 8`, i.e. `1 << scale_bits >= ALPHABET`; below
    /// that no integer table of 256 equal entries can sum to the target, so the
    /// request is rejected as [`ErrorClass::InvalidModel`](crate::error::ErrorClass::InvalidModel).
    pub fn uniform(scale_bits: u8) -> Result<EntropyModel> {
        if !(MIN_SCALE_BITS..=MAX_SCALE_BITS).contains(&scale_bits) {
            let (min, max) = (MIN_SCALE_BITS, MAX_SCALE_BITS);
            return Err(Error::invalid_model(format!(
                "scale_bits {scale_bits} outside {min}..={max}"
            )));
        }
        let target: u32 = 1u32 << scale_bits;
        if target < ALPHABET as u32 {
            return Err(Error::invalid_model(format!(
                "scale_bits {scale_bits} cannot hold {ALPHABET} equal frequencies"
            )));
        }
        // `target` is a power of two >= 256, hence exactly divisible by 256.
        let per = target / ALPHABET as u32;
        Ok(EntropyModel {
            scale_bits,
            frequencies: vec![per; ALPHABET],
        })
    }

    /// Canonical wire encoding (little-endian):
    /// `[version u8 = 1][scale_bits u8][count u16 = 256][freq u16 x 256]`.
    ///
    /// The model is validated before serialization so that `encode` cannot emit
    /// a non-canonical table.
    pub fn encode(&self) -> Result<Vec<u8>> {
        if !(MIN_SCALE_BITS..=MAX_SCALE_BITS).contains(&self.scale_bits) {
            let (min, max) = (MIN_SCALE_BITS, MAX_SCALE_BITS);
            return Err(Error::invalid_model(format!(
                "scale_bits {} outside {min}..={max}",
                self.scale_bits
            )));
        }
        if self.frequencies.len() != ALPHABET {
            return Err(Error::invalid_model(format!(
                "expected {ALPHABET} frequencies, got {}",
                self.frequencies.len()
            )));
        }
        let target = u64::from(self.total());
        if self.frequency_sum() != target {
            return Err(Error::invalid_model(
                "frequencies do not sum to 1 << scale_bits",
            ));
        }

        let mut out = Vec::with_capacity(MODEL_ENCODED_LEN);
        out.push(MODEL_VERSION_1);
        out.push(self.scale_bits);
        out.extend_from_slice(&(ALPHABET as u16).to_le_bytes());
        for &f in &self.frequencies {
            let f =
                u16::try_from(f).map_err(|_| Error::invalid_model("frequency does not fit u16"))?;
            out.extend_from_slice(&f.to_le_bytes());
        }
        Ok(out)
    }

    /// Parse and validate a canonical model; `bytes` must be exactly consumed.
    pub fn decode(bytes: &[u8]) -> Result<EntropyModel> {
        if bytes.len() < MODEL_HEADER_LEN {
            return Err(Error::invalid_model("model shorter than header"));
        }
        let version = bytes[0];
        if version != MODEL_VERSION_1 {
            return Err(Error::unsupported_version(format!(
                "model version {version}, expected {MODEL_VERSION_1}"
            )));
        }
        let scale_bits = bytes[1];
        if !(MIN_SCALE_BITS..=MAX_SCALE_BITS).contains(&scale_bits) {
            let (min, max) = (MIN_SCALE_BITS, MAX_SCALE_BITS);
            return Err(Error::invalid_model(format!(
                "scale_bits {scale_bits} outside {min}..={max}"
            )));
        }
        let count = u16::from_le_bytes([bytes[2], bytes[3]]);
        if count as usize != ALPHABET {
            return Err(Error::invalid_model(format!(
                "declared count {count}, expected {ALPHABET}"
            )));
        }
        if bytes.len() != MODEL_ENCODED_LEN {
            return Err(Error::invalid_model(
                "model length does not match declared count (trailing or truncated)",
            ));
        }

        let target = u64::from(1u32 << scale_bits);
        let mut frequencies = Vec::with_capacity(ALPHABET);
        let mut sum: u64 = 0;
        let (chunks, _rest) = bytes[MODEL_HEADER_LEN..].as_chunks::<2>();
        for chunk in chunks {
            let f = u32::from(u16::from_le_bytes(*chunk));
            sum += u64::from(f);
            frequencies.push(f);
        }
        if sum != target {
            return Err(Error::invalid_model(
                "frequencies do not sum to 1 << scale_bits",
            ));
        }
        Ok(EntropyModel {
            scale_bits,
            frequencies,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts_with(pairs: &[(usize, u64)]) -> [u64; ALPHABET] {
        let mut counts = [0u64; ALPHABET];
        for &(i, v) in pairs {
            counts[i] = v;
        }
        counts
    }

    #[test]
    fn uniform_sums_to_target() {
        for bits in [8u8, 12, 15] {
            let model = EntropyModel::uniform(bits).expect("uniform must succeed");
            assert_eq!(model.frequencies.len(), ALPHABET);
            assert_eq!(model.frequency_sum(), u64::from(1u32 << bits));
            let per = model.frequencies[0];
            assert!(model.frequencies.iter().all(|&f| f == per));
        }
    }

    #[test]
    fn from_counts_sums_exactly() {
        let inputs: &[&[(usize, u64)]] = &[
            &[(0, 1)],
            &[(0, 1), (1, 1)],
            &[(0, 300), (1, 1)],
            &[(0, 1), (1, 2), (2, 3), (3, 4)],
            &[(5, 10), (200, 1), (255, 7)],
            &[(0, u64::MAX), (255, 1)],
        ];
        for bits in [8u8, 12] {
            for input in inputs {
                let counts = counts_with(input);
                let model =
                    EntropyModel::from_counts(&counts, bits).expect("normalization must succeed");
                assert_eq!(model.scale_bits, bits);
                assert_eq!(model.frequency_sum(), u64::from(1u32 << bits));
                for &(i, _) in *input {
                    assert!(model.frequencies[i] >= 1);
                }
            }
        }
    }

    #[test]
    fn determinism() {
        let counts = counts_with(&[(0, 17), (3, 5), (7, 250), (255, 1)]);
        let a = EntropyModel::from_counts(&counts, 12)
            .expect("ok")
            .encode()
            .expect("ok");
        let b = EntropyModel::from_counts(&counts, 12)
            .expect("ok")
            .encode()
            .expect("ok");
        assert_eq!(a, b);
    }

    #[test]
    fn present_symbols_get_at_least_one() {
        // Every symbol present; at scale_bits 12 the target (4096) comfortably
        // exceeds the alphabet size (256).
        let counts = [1u64; ALPHABET];
        let model = EntropyModel::from_counts(&counts, 12).expect("ok");
        assert!(model.frequencies.iter().all(|&f| f >= 1));
        assert_eq!(model.frequency_sum(), 4096);
    }

    #[test]
    fn zero_symbols_get_zero() {
        let counts = counts_with(&[(0, 5), (10, 9), (255, 3)]);
        let model = EntropyModel::from_counts(&counts, 12).expect("ok");
        for (i, &f) in model.frequencies.iter().enumerate() {
            if counts[i] == 0 {
                assert_eq!(f, 0, "symbol {i} had zero count but nonzero frequency");
            }
        }
    }

    #[test]
    fn alphabet_too_big_errors() {
        let counts = [1u64; ALPHABET];
        for bits in 1u8..=7 {
            assert!(EntropyModel::from_counts(&counts, bits).is_err());
        }
    }

    #[test]
    fn roundtrip_encode_decode() {
        let counts = counts_with(&[(0, 1), (1, 2), (2, 3), (100, 400), (255, 7)]);
        for bits in [8u8, 12, 15] {
            let model = EntropyModel::from_counts(&counts, bits).expect("ok");
            let bytes = model.encode().expect("ok");
            assert_eq!(bytes.len(), MODEL_ENCODED_LEN);
            let decoded = EntropyModel::decode(&bytes).expect("ok");
            assert_eq!(decoded, model);
        }
    }

    #[test]
    fn decode_rejects_bad_sum() {
        let mut bytes = EntropyModel::uniform(8).expect("ok").encode().expect("ok");
        // Bump the last frequency so the total no longer equals 1 << 8.
        let last = bytes.len();
        let value = u16::from_le_bytes([bytes[last - 2], bytes[last - 1]]);
        let bumped = value.checked_add(1).expect("fits");
        bytes[last - 2..].copy_from_slice(&bumped.to_le_bytes());
        assert!(EntropyModel::decode(&bytes).is_err());
    }

    #[test]
    fn decode_rejects_bad_version() {
        let mut bytes = EntropyModel::uniform(8).expect("ok").encode().expect("ok");
        bytes[0] = 2;
        let err = EntropyModel::decode(&bytes).expect_err("must reject");
        assert_eq!(err.class(), crate::error::ErrorClass::UnsupportedVersion);
    }

    #[test]
    fn decode_rejects_trailing() {
        let mut bytes = EntropyModel::uniform(8).expect("ok").encode().expect("ok");
        bytes.push(0);
        assert!(EntropyModel::decode(&bytes).is_err());
    }

    #[test]
    fn all_zero_is_uniform() {
        let zero = [0u64; ALPHABET];
        for bits in [8u8, 12, 15] {
            let model = EntropyModel::from_counts(&zero, bits).expect("ok");
            assert_eq!(model, EntropyModel::uniform(bits).expect("ok"));
        }
    }
}
