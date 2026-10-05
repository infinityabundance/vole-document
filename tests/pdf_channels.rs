#![cfg(feature = "rans")]
//! Phase-4.4 acceptance/rejection gates for the PDF typed channels.
//!
//! These tests exercise the *typed-channel* lane (`ProposePdfChannels`) at a
//! scale where per-channel models can actually pay off. The honest experiment is
//! `large_text_channels_compete`: it forces RAW, BYTE_RANS, and PDF_CHANNELS
//! side by side and records the measured sizes, asserting only exactness and
//! determinism of the channel lane — never that it wins. When it loses, that is
//! recorded, not hidden.
//!
//! Every descriptor admitted here must satisfy the authoritative exact triple:
//! `materialized_length == source_length`, `SHA256(materialized) == SHA256(source)`,
//! and `byte_compare(materialized, source) == equal`.

use vole_document::adapter::opaque;
use vole_document::adapter::pdf::propose_pdf_channels;
use vole_document::adapter::pdf::samples::sample_pdfs;
use vole_document::container::Descriptor;
use vole_document::encode;
use vole_document::encode::candidates::{CandidateKind, propose_byte_rans};
use vole_document::integrity::sha256;
use vole_document::limits::Limits;
use vole_document::materialize;
use vole_document::{ErrorClass, encode::candidates::Candidate};

/// Fetch a named sample from the canonical corpus.
fn sample(name: &str) -> Vec<u8> {
    sample_pdfs()
        .into_iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("sample {name} is missing from the corpus"))
        .1
}

/// The larger, plain-content PDF introduced by Subphase 4.4.
fn bigtext() -> Vec<u8> {
    sample("bigtext.pdf")
}

/// Force the typed-channel descriptor for `src`, or panic with a clear message.
fn forced_channels(src: &[u8]) -> Candidate {
    propose_pdf_channels(src, Limits::DEFAULT)
        .unwrap_or_else(|e| panic!("propose_pdf_channels failed: {e}"))
        .unwrap_or_else(|| panic!("typed channels must be proposed for this input"))
}

/// Assert the authoritative exact triple for a serialized descriptor.
fn assert_exact(encoded: &[u8], src: &[u8], label: &str) {
    let (out, parsed) = materialize::decode_to_bytes(encoded, Limits::DEFAULT)
        .unwrap_or_else(|e| panic!("[{label}] decode failed: {e}"));
    assert_eq!(out.len(), src.len(), "[{label}] materialized length");
    assert_eq!(out, src, "[{label}] materialized bytes");
    assert_eq!(sha256(&out), sha256(src), "[{label}] digest");
    assert_eq!(out.len() as u64, parsed.descriptor.source_len);
}

/// Subphase 4.4 gate 1: a forced typed-channel descriptor is byte-exact and its
/// persisted cost is fully charged (models and entropy payload are non-zero, and
/// the parsed attribution sums to the serialized length).
#[test]
fn forced_channels_are_exact_and_fully_charged() {
    let src = bigtext();
    let candidate = forced_channels(&src);
    assert_eq!(candidate.kind, CandidateKind::PdfChannels);

    let (encoded, serialized_cost) = candidate.descriptor.serialize().unwrap();
    assert_exact(&encoded, &src, "forced-channels");

    let parsed = Descriptor::parse(&encoded, Limits::DEFAULT).unwrap();
    assert!(
        parsed.cost.models > 0,
        "per-channel models must be charged, not free"
    );
    assert!(
        parsed.cost.entropy_payload > 0,
        "entropy payloads must be charged, not free"
    );
    assert_eq!(
        parsed.cost.total(),
        encoded.len() as u64,
        "parsed cost attribution must sum to the serialized length"
    );
    assert_eq!(
        serialized_cost.total(),
        encoded.len() as u64,
        "serialize-time cost attribution must agree"
    );
}

/// Subphase 4.4 gate 2: the complete-cost court winner round-trips exactly and
/// never claims to be smaller than the source unless it really is (or it is a
/// trivial literal lane, where only fixed framing overhead is permitted).
#[test]
fn court_winner_is_exact_and_smallest() {
    let src = bigtext();
    let (encoded, report) = encode::encode(&src, Limits::DEFAULT).unwrap();
    assert_exact(&encoded, &src, "court-winner");

    let trivial = matches!(report.kind, CandidateKind::Raw | CandidateKind::Rle);
    assert!(
        report.encoded_len <= report.source_len || trivial,
        "winner {} reported {} bytes for a {} byte source",
        report.kind.name(),
        report.encoded_len,
        report.source_len
    );
    assert_eq!(report.encoded_len, encoded.len() as u64);
    assert_eq!(report.cost.total(), encoded.len() as u64);

    eprintln!(
        "court[bigtext.pdf]: winner={} source={} encoded={} ratio={:.3}",
        report.kind.name(),
        report.source_len,
        report.encoded_len,
        report.compression_ratio()
    );
}

/// Subphase 4.4 gate 7 (honest experiment): force RAW, BYTE_RANS, and
/// PDF_CHANNELS independently and record their measured sizes. We assert only
/// that the channel lane is byte-exact and deterministic; whether it beats the
/// baselines is reported, not asserted.
#[test]
fn large_text_channels_compete() {
    let src = bigtext();

    // RAW: the opaque unconditional floor.
    let raw = opaque::propose(&src, Limits::DEFAULT).unwrap();
    let (raw_bytes, _) = raw.serialize().unwrap();

    // BYTE_RANS: one order-0 model over the whole stream.
    let byte_rans = propose_byte_rans(&src, Limits::DEFAULT)
        .unwrap()
        .expect("BYTE_RANS must be expressible for this size");
    let (byte_rans_bytes, _) = byte_rans.descriptor.serialize().unwrap();

    // PDF_CHANNELS: fixed parallel lexical channels, each entropy-coded.
    let channels_a = forced_channels(&src);
    let (channels_bytes_a, _) = channels_a.descriptor.serialize().unwrap();

    // The channel lane must be byte-exact and deterministic.
    assert_exact(&channels_bytes_a, &src, "channels-a");
    let channels_b = forced_channels(&src);
    let (channels_bytes_b, _) = channels_b.descriptor.serialize().unwrap();
    assert_eq!(
        channels_bytes_a, channels_bytes_b,
        "forced channel lane must be deterministic"
    );

    let raw_len = raw_bytes.len() as u64;
    let byte_rans_len = byte_rans_bytes.len() as u64;
    let channels_len = channels_bytes_a.len() as u64;
    let source_len = src.len() as u64;

    eprintln!(
        "forced[bigtext.pdf]: source={source_len} RAW={raw_len} BYTE_RANS={byte_rans_len} PDF_CHANNELS={channels_len}"
    );
    if channels_len < raw_len {
        eprintln!(
            "forced[bigtext.pdf]: PDF_CHANNELS beats RAW by {} bytes ({:.1}%)",
            raw_len - channels_len,
            100.0 * (raw_len - channels_len) as f64 / raw_len as f64
        );
    } else {
        eprintln!(
            "forced[bigtext.pdf]: PDF_CHANNELS loses to RAW by {} bytes",
            channels_len - raw_len
        );
    }
    if channels_len < byte_rans_len {
        eprintln!(
            "forced[bigtext.pdf]: PDF_CHANNELS beats BYTE_RANS by {} bytes ({:.1}%)",
            byte_rans_len - channels_len,
            100.0 * (byte_rans_len - channels_len) as f64 / byte_rans_len as f64
        );
    } else {
        eprintln!(
            "forced[bigtext.pdf]: PDF_CHANNELS loses to BYTE_RANS by {} bytes",
            channels_len - byte_rans_len
        );
    }

    // The court's own pick, reported for honesty.
    let (_, report) = encode::encode(&src, Limits::DEFAULT).unwrap();
    eprintln!(
        "forced[bigtext.pdf]: court winner={} at {} bytes",
        report.kind.name(),
        report.encoded_len
    );
}

/// Subphase 4.4 gate 4 (rejection): on a small PDF the 14 model records cannot
/// pay for themselves, so the court must not select the channel lane.
#[test]
fn channels_rejected_on_small_pdf() {
    let src = sample("classic.pdf");
    let (encoded, report) = encode::encode(&src, Limits::DEFAULT).unwrap();
    assert_exact(&encoded, &src, "classic");
    assert_ne!(
        report.kind,
        CandidateKind::PdfChannels,
        "the 14 model records cannot pay off on a {} byte PDF",
        src.len()
    );
    eprintln!(
        "court[classic.pdf]: winner={} source={} encoded={}",
        report.kind.name(),
        report.source_len,
        report.encoded_len
    );
}

/// Subphase 4.4 gate 5: identical input yields identical descriptor bytes.
#[test]
fn channels_deterministic() {
    let src = bigtext();
    let (a, ra) = encode::encode(&src, Limits::DEFAULT).unwrap();
    let (b, rb) = encode::encode(&src, Limits::DEFAULT).unwrap();
    assert_eq!(a, b, "encode of bigtext.pdf must be deterministic");
    assert_eq!(ra.kind, rb.kind);
    assert_eq!(ra.encoded_len, rb.encoded_len);

    let fa = forced_channels(&src).descriptor.serialize().unwrap().0;
    let fb = forced_channels(&src).descriptor.serialize().unwrap().0;
    assert_eq!(fa, fb, "forced channel bytes must be deterministic");
}

/// Subphase 4.4 gate 6 (hostile input): a single flipped byte inside a channel
/// payload must yield a typed error or a non-matching reconstruction — never a
/// panic and never a silent success.
#[test]
fn corrupt_channel_is_rejected() {
    let src = bigtext();
    let (encoded, _) = forced_channels(&src).descriptor.serialize().unwrap();
    assert_exact(&encoded, &src, "corrupt-baseline");

    // (a) A raw flip in the serialized descriptor. The flip is almost certainly
    // inside a record payload, where CRC32C catches it; anywhere else, header or
    // structural validation catches it.
    let mut corrupted = encoded.clone();
    let mid = corrupted.len() / 2;
    corrupted[mid] ^= 0xFF;
    match materialize::decode_to_bytes(&corrupted, Limits::DEFAULT) {
        Ok((out, _)) => assert_ne!(
            out, src,
            "a corrupted descriptor must not silently reconstruct the source"
        ),
        Err(e) => assert_ne!(
            e.class(),
            ErrorClass::InternalInvariant,
            "hostile input must fail with a typed class, not an internal invariant"
        ),
    }

    // (b) Corrupt at a layer that passes CRC: mutate a decoded (parsed) channel
    // payload and re-serialize, which recomputes every record CRC.
    let mut parsed = Descriptor::parse(&encoded, Limits::DEFAULT).unwrap();
    let victim = parsed
        .descriptor
        .channels
        .iter_mut()
        .find(|c| !c.payload.is_empty())
        .expect("at least one channel carries a payload");
    victim.payload[0] ^= 0xFF;
    let (reserialized, _) = parsed.descriptor.serialize().unwrap();
    match materialize::decode_to_bytes(&reserialized, Limits::DEFAULT) {
        Ok((out, _)) => assert_ne!(
            out, src,
            "a re-serialized corrupted channel must not silently reconstruct"
        ),
        Err(e) => assert_ne!(
            e.class(),
            ErrorClass::InternalInvariant,
            "a CRC-passing corruption must still fail with a typed class"
        ),
    }
}

/// Subphase 4.4 gate 8 (resource bounds): a forced channel descriptor is
/// rejected with a typed `ResourceLimit`/`EntropyDecode` under reduced bounds,
/// rather than allocating unbounded memory.
#[test]
fn limits_bound_channels() {
    let src = bigtext();
    let (encoded, _) = forced_channels(&src).descriptor.serialize().unwrap();

    let tight = Limits {
        max_channel_symbols: 1,
        ..Limits::STRICT
    };
    let err = materialize::decode_to_bytes(&encoded, tight).expect_err(
        "a channel with more than one symbol must be refused under max_channel_symbols = 1",
    );
    assert!(
        matches!(
            err.class(),
            ErrorClass::ResourceLimit | ErrorClass::EntropyDecode
        ),
        "expected ResourceLimit or EntropyDecode, got {}",
        err.class().as_str()
    );
}
