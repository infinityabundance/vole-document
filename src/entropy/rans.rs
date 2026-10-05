//! Single-lane order-0 byte rANS over the safe manual ryg-rans-rs API.
//!
//! This module deliberately avoids `ryg_rans_rs::alloc_utils`, whose `decode`
//! panics on truncated input. Everything here goes through the checked manual
//! API (`rans_byte_enc_put_symbol`, `rans_byte_enc_flush`,
//! `rans_byte_dec_get`, `rans_byte_dec_advance_symbol`). Symbol lookup is a
//! scalar, deterministic cumulative table; every value derived from untrusted
//! input is range-checked or uses checked arithmetic, and the decoder never
//! unwinds on malformed input.

use ryg_rans_rs::byte::{
    BackwardByteWriter, ByteReader, RANS_BYTE_L, RansByteDecSymbol, RansByteEncSymbol,
    RansByteState, rans_byte_dec_advance_symbol, rans_byte_dec_get, rans_byte_enc_flush,
    rans_byte_enc_put_symbol,
};

use crate::entropy::model::{ALPHABET, EntropyModel, MAX_SCALE_BITS, MIN_SCALE_BITS};
use crate::error::{Error, Result};
use crate::limits::Limits;

/// Complete decoder-entry capsule for one entropy channel.
/// The persisted physical form (states + renormalization payload + counts) —
/// never a bare scalar "seed".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capsule {
    /// One decoder initial state (scalar single lane). Encoder final state.
    pub initial_state: u32,
    /// Renormalization payload bytes, in the exact orientation the decoder
    /// consumes them: forward from index 0, the reverse of encoder emission
    /// order. This excludes the 4-byte flush state, which lives in
    /// [`Capsule::initial_state`].
    pub payload: Vec<u8>,
    /// Number of symbols encoded (== decoded_length for the byte alphabet).
    pub symbol_count: u64,
    /// Exact decoded length in bytes.
    pub decoded_length: u64,
}

/// Validate a model for use with the byte-rANS codec.
///
/// Returns the model total (`1 << scale_bits`). `decode` selects the error
/// class: decode paths report [`Error::entropy_decode`], encode paths report
/// [`Error::invalid_model`].
fn validate_model(model: &EntropyModel, decode: bool) -> Result<u32> {
    let fail = |msg: String| {
        if decode {
            Error::entropy_decode(msg)
        } else {
            Error::invalid_model(msg)
        }
    };

    if !(MIN_SCALE_BITS..=MAX_SCALE_BITS).contains(&model.scale_bits) {
        return Err(fail(format!(
            "scale_bits {} outside {}..={}",
            model.scale_bits, MIN_SCALE_BITS, MAX_SCALE_BITS
        )));
    }
    if model.frequencies.len() != ALPHABET {
        return Err(fail(format!(
            "expected {ALPHABET} frequencies, got {}",
            model.frequencies.len()
        )));
    }

    let target = 1u32 << model.scale_bits;
    let mut sum: u64 = 0;
    for &f in &model.frequencies {
        if u64::from(f) > u64::from(target) {
            return Err(fail("frequency exceeds 1 << scale_bits".to_string()));
        }
        sum += u64::from(f);
    }
    if sum != u64::from(target) {
        return Err(fail(
            "frequencies do not sum to 1 << scale_bits".to_string(),
        ));
    }
    Ok(target)
}

/// Encode `data` with `model`. Deterministic.
///
/// Symbols are consumed last-to-first (rANS stack discipline) and
/// renormalization bytes are emitted backward; the resulting payload is
/// returned in forward decoder-consumption order.
pub fn encode_channel(model: &EntropyModel, data: &[u8]) -> Result<Capsule> {
    let scale_bits = u32::from(model.scale_bits);
    let target = validate_model(model, false)?;

    // Build the per-symbol encoder table. Symbols with zero frequency cannot
    // be encoded; they are left absent and rejected if they occur in `data`.
    let mut enc_syms: [Option<RansByteEncSymbol>; ALPHABET] = [None; ALPHABET];
    let mut start: u32 = 0;
    for (symbol, &freq) in model.frequencies.iter().enumerate() {
        if freq > 0 {
            let sym = RansByteEncSymbol::new(start, freq, scale_bits)
                .map_err(|e| Error::invalid_model(format!("encoder symbol {symbol}: {e}")))?;
            enc_syms[symbol] = Some(sym);
        }
        start = start
            .checked_add(freq)
            .ok_or_else(|| Error::invalid_model("cumulative frequency overflow"))?;
    }
    if start != target {
        return Err(Error::invalid_model("cumulative table mismatch"));
    }

    // Worst-case output bound matching the upstream convenience API:
    // at most 4 bytes per symbol plus flush headroom.
    let max_size = data
        .len()
        .checked_mul(4)
        .and_then(|n| n.checked_add(24))
        .ok_or_else(|| Error::resource_limit("encoded size estimate overflow"))?;
    let mut buf = vec![0u8; max_size];
    let mut writer = BackwardByteWriter::new(&mut buf);

    let mut state = RansByteState::new();
    for &byte in data.iter().rev() {
        let sym = enc_syms[byte as usize]
            .as_ref()
            .ok_or_else(|| Error::invalid_model(format!("data byte {byte} has zero frequency")))?;
        rans_byte_enc_put_symbol(&mut state, &mut writer, sym)
            .map_err(|_| Error::internal_invariant("rANS encoder buffer exhausted"))?;
    }
    rans_byte_enc_flush(&state, &mut writer)
        .map_err(|_| Error::internal_invariant("rANS flush buffer exhausted"))?;

    // `encoded` begins with the 4-byte little-endian flush state (the last
    // write lands at the lowest address); the remainder is the renormalization
    // payload already in forward decoder-consumption order.
    let encoded = writer.encoded();
    let payload = encoded
        .get(4..)
        .ok_or_else(|| Error::internal_invariant("flush state missing from encoder output"))?
        .to_vec();

    Ok(Capsule {
        initial_state: state.get(),
        payload,
        symbol_count: data.len() as u64,
        decoded_length: data.len() as u64,
    })
}

/// Decode a channel, hostile-safe and bounded.
///
/// Never panics: truncated state/payload, illegal `symbol_count` /
/// `decoded_length`, an unsupported model, a length mismatch, or a failed
/// stream-integrity check all return [`Error::entropy_decode`].
pub fn decode_channel(model: &EntropyModel, capsule: &Capsule, limits: Limits) -> Result<Vec<u8>> {
    let scale_bits = u32::from(model.scale_bits);
    let target = validate_model(model, true)?;

    // Resource bounds checked before any large allocation or work.
    if capsule.symbol_count > limits.max_channel_symbols {
        return Err(Error::entropy_decode(format!(
            "symbol_count {} exceeds limit {}",
            capsule.symbol_count, limits.max_channel_symbols
        )));
    }
    if capsule.decoded_length > limits.max_output_bytes {
        return Err(Error::entropy_decode(format!(
            "decoded_length {} exceeds limit {}",
            capsule.decoded_length, limits.max_output_bytes
        )));
    }
    if capsule.payload.len() as u64 > u64::from(limits.max_record_len) {
        return Err(Error::entropy_decode(format!(
            "payload length {} exceeds limit {}",
            capsule.payload.len(),
            limits.max_record_len
        )));
    }
    if capsule.symbol_count != capsule.decoded_length {
        return Err(Error::entropy_decode(
            "symbol_count does not equal decoded_length".to_string(),
        ));
    }

    let symbol_count = usize::try_from(capsule.symbol_count)
        .map_err(|_| Error::entropy_decode("symbol_count does not fit platform usize"))?;

    // Stable cumulative decode table: slot in [0, 1<<scale_bits) maps to the
    // unique symbol `s` with cum[s] <= slot < cum[s] + freq[s].
    let mut dec_syms: [Option<RansByteDecSymbol>; ALPHABET] = [None; ALPHABET];
    let mut cum2sym = vec![0u8; target as usize];
    let mut start: u32 = 0;
    for (symbol, &freq) in model.frequencies.iter().enumerate() {
        if freq > 0 {
            let dsym = RansByteDecSymbol::new(start, freq)
                .map_err(|e| Error::entropy_decode(format!("decoder symbol {symbol}: {e}")))?;
            dec_syms[symbol] = Some(dsym);
            let end = start
                .checked_add(freq)
                .ok_or_else(|| Error::entropy_decode("cumulative frequency overflow"))?;
            for slot in &mut cum2sym[start as usize..end as usize] {
                *slot = symbol as u8;
            }
            start = end;
        }
    }
    if start != target {
        return Err(Error::entropy_decode(
            "cumulative table mismatch".to_string(),
        ));
    }

    let mut output: Vec<u8> = Vec::new();
    output
        .try_reserve(symbol_count)
        .map_err(|_| Error::entropy_decode("cannot allocate decode buffer"))?;

    let mut reader = ByteReader::new(&capsule.payload);
    // The public field is safe to construct directly; the value is untrusted
    // and every downstream arithmetic step is overflow-free for any u32
    // because model frequencies never exceed `1 << scale_bits`.
    let mut state = RansByteState(capsule.initial_state);

    for _ in 0..symbol_count {
        let slot = rans_byte_dec_get(&state, scale_bits);
        let symbol = cum2sym[slot as usize];
        output.push(symbol);
        let dsym = dec_syms[symbol as usize]
            .as_ref()
            .ok_or_else(|| Error::entropy_decode("slot mapped to zero-frequency symbol"))?;
        rans_byte_dec_advance_symbol(&mut state, &mut reader, dsym, scale_bits)
            .map_err(|_| Error::entropy_decode("truncated renormalization payload"))?;
    }

    if output.len() as u64 != capsule.decoded_length {
        return Err(Error::entropy_decode(format!(
            "decoded {} bytes, expected {}",
            output.len(),
            capsule.decoded_length
        )));
    }
    // A well-formed stream consumes the payload exactly and returns the state
    // to the encoder's initial lower bound. This rejects truncation and most
    // corrupted state/payload combinations outright.
    if state.get() != RANS_BYTE_L || reader.remaining() != 0 {
        return Err(Error::entropy_decode(
            "entropy stream integrity check failed".to_string(),
        ));
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorClass;

    /// Deterministic xorshift64 PRNG for reproducible test corpora.
    struct XorShift(u64);

    impl XorShift {
        fn new(seed: u64) -> Self {
            Self(seed | 1)
        }

        fn next_u32(&mut self) -> u32 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            (x >> 32) as u32
        }

        fn next_byte(&mut self) -> u8 {
            (self.next_u32() & 0xff) as u8
        }
    }

    fn model_from_data(data: &[u8], scale_bits: u8) -> EntropyModel {
        let mut counts = [0u64; ALPHABET];
        for &b in data {
            counts[b as usize] += 1;
        }
        EntropyModel::from_counts(&counts, scale_bits).expect("model normalizes")
    }

    fn roundtrip(data: &[u8], scale_bits: u8) {
        let model = model_from_data(data, scale_bits);
        let capsule = encode_channel(&model, data).expect("encode");
        assert_eq!(capsule.symbol_count, data.len() as u64);
        assert_eq!(capsule.decoded_length, data.len() as u64);

        let decoded = decode_channel(&model, &capsule, Limits::DEFAULT).expect("decode");
        assert_eq!(decoded, data);

        let again = encode_channel(&model, data).expect("encode again");
        assert_eq!(capsule, again, "encode must be deterministic");
    }

    #[test]
    fn roundtrip_empty() {
        for bits in [8u8, 12] {
            roundtrip(&[], bits);
        }
    }

    #[test]
    fn roundtrip_single_repeated_byte() {
        for bits in [8u8, 12] {
            roundtrip(&[0x41u8; 1000], bits);
        }
    }

    #[test]
    fn roundtrip_all_256_values() {
        let mut data = Vec::new();
        for _ in 0..8 {
            data.extend(0u8..=255);
        }
        for bits in [8u8, 12] {
            roundtrip(&data, bits);
        }
    }

    #[test]
    fn roundtrip_uniform_random() {
        let mut rng = XorShift::new(0x1234_5678_9abc_def0);
        let data: Vec<u8> = (0..4096).map(|_| rng.next_byte()).collect();
        for bits in [8u8, 12] {
            roundtrip(&data, bits);
        }
    }

    #[test]
    fn roundtrip_heavily_skewed() {
        let mut rng = XorShift::new(0xdead_beef_cafe_f00d);
        let data: Vec<u8> = (0..4096)
            .map(|i| if i % 100 == 0 { rng.next_byte() } else { 0x00 })
            .collect();
        for bits in [8u8, 12] {
            roundtrip(&data, bits);
        }
    }

    #[test]
    fn truncated_payload_is_rejected() {
        let mut rng = XorShift::new(0x0f0f_0f0f_1234_5678);
        let data: Vec<u8> = (0..4096).map(|_| rng.next_byte()).collect();
        let model = model_from_data(&data, 12);
        let capsule = encode_channel(&model, &data).expect("encode");
        assert!(!capsule.payload.is_empty());

        let mut truncated = capsule.clone();
        truncated.payload.pop();
        assert!(decode_channel(&model, &truncated, Limits::DEFAULT).is_err());
    }

    #[test]
    fn missing_payload_is_rejected() {
        let mut rng = XorShift::new(0x9988_7766_5544_3322);
        let data: Vec<u8> = (0..1024).map(|_| rng.next_byte()).collect();
        let model = model_from_data(&data, 12);
        let capsule = encode_channel(&model, &data).expect("encode");

        let mut missing = capsule.clone();
        missing.payload.clear();
        assert!(decode_channel(&model, &missing, Limits::DEFAULT).is_err());
    }

    #[test]
    fn corrupted_state_and_payload_never_panic() {
        let mut rng = XorShift::new(0xabcd_ef01_2345_6789);
        let data: Vec<u8> = (0..2048).map(|_| rng.next_byte()).collect();
        let model = model_from_data(&data, 12);
        let capsule = encode_channel(&model, &data).expect("encode");

        let mut states = vec![0u32, u32::MAX, RANS_BYTE_L, capsule.initial_state];
        for delta in [1u32, 0x8000, 0xffff_ffff] {
            states.push(capsule.initial_state.wrapping_add(delta));
        }
        for value in states {
            let mut c = capsule.clone();
            c.initial_state = value;
            match decode_channel(&model, &c, Limits::DEFAULT) {
                Ok(out) => assert_eq!(out.len() as u64, c.decoded_length),
                Err(e) => assert_eq!(e.class(), ErrorClass::EntropyDecode),
            }
        }

        for (i, _) in capsule.payload.iter().enumerate().take(64) {
            let mut c = capsule.clone();
            c.payload[i] ^= 0xff;
            match decode_channel(&model, &c, Limits::DEFAULT) {
                Ok(out) => assert_eq!(out.len() as u64, c.decoded_length),
                Err(e) => assert_eq!(e.class(), ErrorClass::EntropyDecode),
            }
        }
    }

    #[test]
    fn limits_reject_oversized_fields() {
        let mut rng = XorShift::new(0x1357_9bdf_2468_ace0);
        let data: Vec<u8> = (0..2048).map(|_| rng.next_byte()).collect();
        let model = model_from_data(&data, 12);
        let capsule = encode_channel(&model, &data).expect("encode");
        assert!(capsule.symbol_count > 0);
        assert!(capsule.decoded_length > 0);
        assert!(!capsule.payload.is_empty());

        let by_symbols = Limits {
            max_channel_symbols: capsule.symbol_count - 1,
            ..Limits::DEFAULT
        };
        assert!(decode_channel(&model, &capsule, by_symbols).is_err());

        let by_output = Limits {
            max_output_bytes: capsule.decoded_length - 1,
            ..Limits::DEFAULT
        };
        assert!(decode_channel(&model, &capsule, by_output).is_err());

        let by_record = Limits {
            max_record_len: capsule.payload.len() as u32 - 1,
            ..Limits::DEFAULT
        };
        assert!(decode_channel(&model, &capsule, by_record).is_err());

        // The strict profile still accepts this small channel.
        assert!(decode_channel(&model, &capsule, Limits::STRICT).is_ok());
    }

    #[test]
    fn inconsistent_lengths_are_rejected() {
        let data = b"length check".to_vec();
        let model = model_from_data(&data, 12);
        let capsule = encode_channel(&model, &data).expect("encode");

        let mut mismatched = capsule.clone();
        mismatched.symbol_count = capsule.symbol_count + 1;
        assert!(decode_channel(&model, &mismatched, Limits::DEFAULT).is_err());

        let mut bad_len = capsule.clone();
        bad_len.decoded_length = capsule.decoded_length + 1;
        assert!(decode_channel(&model, &bad_len, Limits::DEFAULT).is_err());
    }
}
