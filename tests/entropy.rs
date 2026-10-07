#![cfg(feature = "rans")]
//! Phase-2 entropy court: negative controls and acceptance gates.
//!
//! Every test here is tied to a predeclared gate in
//! `docs/phases/phase-02-plan.md`:
//!
//! 1. Exact: every admitted descriptor materializes byte-for-byte.
//! 3. Complete cost: model bytes and framing are charged; no "free model".
//! 4. Negative control: on incompressible input RAW (or a trivial lane) wins.
//! 5. Determinism: same input => same descriptor bytes.
//! 7. Honest result: when rANS loses, that is recorded, not hidden.
//!
//! The gate numbers are repeated on each test so the mapping survives edits.

use vole_document::encode;
use vole_document::encode::candidates::CandidateKind;
use vole_document::integrity::sha256;
use vole_document::limits::Limits;
use vole_document::materialize;

/// Deterministic, seedable PRNG so corpus generation is reproducible.
///
/// This is the same xorshift64 pattern used by `tests/exact.rs`.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        // Avoid the zero fixed point of xorshift.
        Rng(seed | 1)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        let mut v = Vec::with_capacity(n + 8);
        while v.len() < n {
            v.extend_from_slice(&self.next_u64().to_le_bytes());
        }
        v.truncate(n);
        v
    }
}

/// Encode `input`, decode it back, and assert the authoritative exact triple.
///
/// Returns the serialized bytes and the encode report so callers can make
/// lane-specific assertions. Every court case runs through here, so exactness
/// (gate 1) is enforced even when the headline claim is about cost.
fn exact_roundtrip(input: &[u8], limits: Limits, label: &str) -> (Vec<u8>, encode::EncodeReport) {
    check_roundtrip(input, limits, None, label)
}

/// Like [`exact_roundtrip`] but forces a single candidate lane, so a claim about
/// *that* lane is not coupled to the auto winner. (Phase 13.2 added a phrase
/// template that can win the full court on repeated text, so an "order-0 rANS
/// wins" claim must force the lane rather than observe the auto winner.)
fn exact_roundtrip_forced(
    input: &[u8],
    limits: Limits,
    kind: CandidateKind,
    label: &str,
) -> (Vec<u8>, encode::EncodeReport) {
    check_roundtrip(input, limits, Some(kind), label)
}

fn check_roundtrip(
    input: &[u8],
    limits: Limits,
    force: Option<CandidateKind>,
    label: &str,
) -> (Vec<u8>, encode::EncodeReport) {
    let (bytes, report) = encode::encode_with(input, limits, force)
        .unwrap_or_else(|e| panic!("[{label}] encode failed: {e}"));
    let (out, parsed) = materialize::decode_to_bytes(&bytes, limits)
        .unwrap_or_else(|e| panic!("[{label}] decode failed: {e}"));

    // Gate 1: byte-exact reconstruction.
    assert_eq!(out.len(), input.len(), "[{label}] length mismatch");
    assert_eq!(out, input, "[{label}] byte mismatch");
    assert_eq!(sha256(&out), sha256(input), "[{label}] digest mismatch");
    assert_eq!(out.len() as u64, parsed.descriptor.source_len);

    // Gate 3: charged cost equals the physical descriptor length.
    assert_eq!(report.encoded_len, bytes.len() as u64, "[{label}] len");
    assert_eq!(
        report.cost.total(),
        bytes.len() as u64,
        "[{label}] cost attribution must sum to the serialized length"
    );
    (bytes, report)
}

/// A mixed acceptance matrix: varied sizes, entropies, and lane winners.
///
/// Gate 1 is applied to every member; the matrix deliberately mixes RAW, RLE,
/// and BYTE_RANS winners plus the empty input.
fn acceptance_matrix() -> Vec<(&'static str, Vec<u8>)> {
    let mut rng = Rng::new(0xA11CE);
    vec![
        ("empty", Vec::new()),
        ("one-byte", vec![0x42]),
        ("incrementing-3", vec![0, 1, 2]),
        ("short-text", b"hello, exact world".to_vec()),
        ("random-64k", rng.bytes(64 * 1024)),
        ("zeros-64k", vec![0u8; 64 * 1024]),
        ("text-60k", text_corpus()),
        ("skewed-32k", skewed_corpus()),
        ("blocks-256", alternating_blocks()),
        ("random-256k", rng.bytes(256 * 1024)),
    ]
}

/// Repeated English-like prose (~60 KiB, very low order-0 entropy).
fn text_corpus() -> Vec<u8> {
    b"The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs. "
        .repeat(700)
}

/// A stream where `0x00` occupies ~90% of the symbols.
fn skewed_corpus() -> Vec<u8> {
    (0..32_000u32)
        .map(|i| if i % 10 == 0 { (i % 256) as u8 } else { 0 })
        .collect()
}

/// Alternating 256-byte blocks of two distinct bytes: long runs for RLE.
fn alternating_blocks() -> Vec<u8> {
    let mut v = Vec::new();
    for i in 0..100u32 {
        let b = if i % 2 == 0 { 0xAA } else { 0x55 };
        v.extend_from_slice(&[b; 256]);
    }
    v
}

// ---------------------------------------------------------------------------
// Negative controls: RAW (or a trivial literal lane) must win.
// ---------------------------------------------------------------------------

/// Gate 4: incompressible data must be stored RAW, never forced into the model.
#[test]
fn random_64k_selects_raw() {
    let input = Rng::new(0x9E37_79B9_7F4A_7C15).bytes(64 * 1024);
    let (_, report) = exact_roundtrip(&input, Limits::DEFAULT, "random-64k");
    assert_eq!(
        report.kind,
        CandidateKind::Raw,
        "incompressible data must be stored RAW (gate 4)"
    );
}

/// Gate 4: tiny inputs cannot pay for the dense order-0 model.
///
/// The substantive negative-control claim is that the rANS lane loses: the
/// canonical 256-symbol model alone dominates a handful of bytes. The trivial
/// literal lanes are the observed winners.
///
/// Note on the observed winner: the plan sketch expected RAW for all six sizes,
/// but for 0..=3 bytes the RLE lane is strictly cheaper than RAW because it
/// omits the separate OBJECT record (5 records instead of 6) while still
/// carrying the literal bytes in the graph. This is the "trivial lane" the
/// gate's wording allows; we pin the observed outcome rather than hide it.
#[test]
fn tiny_inputs_select_raw() {
    for n in [0usize, 1, 2, 3, 8, 16] {
        // Incrementing bytes guarantee no run longer than one.
        let input: Vec<u8> = (0..n).map(|i| i as u8).collect();
        let (_, report) = exact_roundtrip(&input, Limits::DEFAULT, "tiny");
        assert_ne!(
            report.kind,
            CandidateKind::ByteRans,
            "tiny input len={n}: model cannot pay off (gate 4)"
        );
        if n <= 3 {
            assert_eq!(
                report.kind,
                CandidateKind::Rle,
                "tiny input len={n}: trivial RLE literal lane wins"
            );
        } else {
            assert_eq!(
                report.kind,
                CandidateKind::Raw,
                "tiny input len={n}: RAW literal lane wins"
            );
        }
    }
}

/// Gates 4 and 3: high-entropy input expands only by fixed framing overhead.
#[test]
fn high_entropy_selects_raw() {
    let input = Rng::new(0x0BAD_F00D_1234_5678).bytes(256 * 1024);
    let (bytes, report) = exact_roundtrip(&input, Limits::DEFAULT, "high-entropy-256k");
    assert_eq!(report.kind, CandidateKind::Raw, "gate 4");
    // Gate 3: the only expansion permitted is the fixed RAW framing, never a
    // hidden model or payload cost.
    let overhead = report.encoded_len - report.source_len;
    // Fixed RAW framing overhead for the Phase-9 universe (DRA v8 plus the
    // `+observation-index-v1+seek-directory-v1+external-objects-v1` suffixes;
    // this descriptor carries no index or directory record and no external
    // objects, so the only change is the longer universe string).
    assert!(
        overhead <= 454,
        "RAW expansion was {overhead} bytes; expected <= 454 fixed framing"
    );
    assert_eq!(bytes.len() as u64, report.encoded_len);
}

/// Gate 4: an "already-compressed-looking" control must not be recompressed.
#[test]
fn random_wrapped_looking_control() {
    let input = Rng::new(0xFEED_FACE_CAFE_BABE).bytes(1 << 20);
    let (_, report) = exact_roundtrip(&input, Limits::DEFAULT, "random-1MiB");
    assert_eq!(
        report.kind,
        CandidateKind::Raw,
        "already-random control must stay RAW (gate 4)"
    );
}

// ---------------------------------------------------------------------------
// Positive results: the lane must win AND the representation must be exact.
// ---------------------------------------------------------------------------

/// Gates 1 and 7: rANS must beat RAW on low-entropy text, exactly. Forced to the
/// `BYTE_RANS` lane so the claim is about order-0 rANS itself, not the auto
/// winner (which a phrase-template lane may take on repeated text).
#[test]
fn low_entropy_text_wins_and_is_exact() {
    let input = text_corpus();
    let (bytes, report) =
        exact_roundtrip_forced(&input, Limits::DEFAULT, CandidateKind::ByteRans, "text-60k");
    assert_eq!(
        report.kind,
        CandidateKind::ByteRans,
        "order-0 rANS should win"
    );
    assert!(
        report.encoded_len < report.source_len,
        "rANS must beat RAW on low-entropy text: {} vs {}",
        report.encoded_len,
        report.source_len
    );
    assert_eq!(bytes.len() as u64, report.encoded_len);
}

/// Gates 1 and 7: long runs must be captured by the RLE lane.
#[test]
fn long_runs_select_rle() {
    // A single 100 000-byte run.
    let zeros = vec![0u8; 100_000];
    let (_, report) = exact_roundtrip(&zeros, Limits::DEFAULT, "zeros-100k");
    assert_eq!(report.kind, CandidateKind::Rle, "long zero run => RLE");
    assert!(
        report.encoded_len < report.source_len / 10,
        "RLE of zeros was {} bytes",
        report.encoded_len
    );

    // Alternating long blocks: still run-structured, so RLE must win (or, if a
    // smaller lane won, the same strict ratio bound must hold).
    let blocks = alternating_blocks();
    let (_, report) = exact_roundtrip(&blocks, Limits::DEFAULT, "blocks-256");
    assert_eq!(
        report.kind,
        CandidateKind::Rle,
        "alternating long blocks => RLE (or a smaller lane under the ratio bound)"
    );
    assert!(
        report.encoded_len < report.source_len / 10,
        "alternating blocks: {} won at {} bytes (source {})",
        report.kind.name(),
        report.encoded_len,
        report.source_len
    );
}

/// Gates 1 and 7: a heavily skewed distribution must be coded by rANS.
#[test]
fn skewed_bytes_select_rans() {
    let input = skewed_corpus();
    let (_, report) = exact_roundtrip(&input, Limits::DEFAULT, "skewed-32k");
    assert_eq!(
        report.kind,
        CandidateKind::ByteRans,
        "90%-zero stream is ideal order-0 rANS input"
    );
    assert!(
        report.encoded_len < report.source_len / 2,
        "skewed rANS was {} bytes (source {})",
        report.encoded_len,
        report.source_len
    );
}

// ---------------------------------------------------------------------------
// Acceptance gates over the mixed matrix.
// ---------------------------------------------------------------------------

/// Gate 1: every admitted descriptor is byte-exact and fully accounted.
#[test]
fn every_admitted_descriptor_is_byte_exact() {
    for (label, input) in acceptance_matrix() {
        let (bytes, report) = exact_roundtrip(&input, Limits::DEFAULT, label);
        let (out, parsed) = materialize::decode_to_bytes(&bytes, Limits::DEFAULT).unwrap();

        assert_eq!(out, input, "[{label}] bytes");
        assert_eq!(out.len(), input.len(), "[{label}] length");
        assert_eq!(sha256(&out), sha256(&input), "[{label}] digest");
        assert_eq!(out.len() as u64, parsed.descriptor.source_len);
        assert_eq!(report.cost.total(), report.encoded_len, "[{label}] cost");
        assert_eq!(report.encoded_len, bytes.len() as u64, "[{label}] encoded");
    }
}

/// Gate 5: identical input => identical descriptor bytes.
#[test]
fn encoding_is_deterministic() {
    for (label, input) in acceptance_matrix() {
        let (a, ra) = encode::encode(&input, Limits::DEFAULT).unwrap();
        let (b, rb) = encode::encode(&input, Limits::DEFAULT).unwrap();
        assert_eq!(a, b, "[{label}] descriptor bytes differ across encodes");
        assert_eq!(ra.kind, rb.kind, "[{label}] winner kind differs");
        assert_eq!(ra.encoded_len, rb.encoded_len, "[{label}] length differs");
    }
}

/// Gate 3: the model cost is real; a two-byte input cannot use rANS.
///
/// The canonical model serializes to far more than two bytes, so admitting the
/// rANS lane here would mean the model was free. The court must decline it.
#[test]
fn model_cost_is_not_hidden() {
    let input = b"ab";
    let (_, report) = exact_roundtrip(input, Limits::DEFAULT, "two-byte");
    assert_ne!(
        report.kind,
        CandidateKind::ByteRans,
        "a two-byte input must not pay a dense model (gate 3)"
    );
}

/// Gate 5: the winner must not depend on limit noise.
///
/// A roomier `Limits` (larger `max_output_bytes`) must yield the same winning
/// kind and byte-identical descriptors; limits gate admission, not pricing.
#[test]
fn winner_is_stable_across_limits() {
    let input = b"the quick brown fox jumps over the lazy dog ".repeat(1000);
    let roomy = Limits {
        max_output_bytes: 1 << 41,
        ..Limits::DEFAULT
    };

    let (a, ra) = exact_roundtrip(&input, Limits::DEFAULT, "limits-default");
    let (b, rb) = exact_roundtrip(&input, roomy, "limits-roomy");

    assert_eq!(ra.kind, rb.kind, "winner changed with roomier limits");
    assert_eq!(a, b, "descriptor bytes changed with roomier limits");
    assert_eq!(ra.encoded_len, rb.encoded_len);
}
