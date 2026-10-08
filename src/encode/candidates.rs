//! Candidate generators.
//!
//! Each generator proposes a bounded, deterministic reconstruction hypothesis.
//! Generators are cheap to decline and never trusted because their logic
//! "looks obvious": every candidate reaches the common court.

use crate::SOURCE_FORMAT_OPAQUE;
use crate::adapter::opaque;
use crate::container::{Descriptor, UNIVERSE};
use crate::dra::{Op, Program};
#[cfg(feature = "rans")]
use crate::entropy::{
    CODER_ORDER0_BYTE_RANS, CODER_VERSION_1, EntropyChannelDescriptor, EntropyModel, encode_channel,
};
use crate::error::Result;
use crate::integrity::sha256;
use crate::limits::Limits;

/// Stable candidate identity. The discriminant order is the final
/// deterministic tie-breaker when prices and work are equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum CandidateKind {
    /// Single literal object, literal program (Phase 1 floor).
    Raw = 0,
    /// Run-length generated repeats (Phase 2).
    Rle = 1,
    /// Typed byte rANS entropy channel (Phase 2).
    ByteRans = 2,
    /// PDF physical span partition as literal-span DRA ops (Phase 3).
    PdfPhysical = 3,
    /// PDF typed lexical channels, each entropy-coded (Phase 4).
    PdfChannels = 4,
    /// PDF classic cross-reference offsets regenerated from marked positions
    /// (Phase 5).
    PdfLayout = 5,
    /// PDF layout plan (data object + item table) carried through two rANS
    /// entropy channels (Phase 5.8).
    PdfLayoutRans = 6,
    /// PDF exact DEFLATE replay: eligible `/FlateDecode` streams are
    /// inverse-proceduralized to (plaintext, corrections) and replayed (Phase 6).
    PdfDeflateReplay = 7,
    /// PDF DEFLATE replay whose plaintexts are carried as shared order-0 byte-rANS
    /// entropy channels (Phase 6.5).
    PdfDeflateReplayRans = 8,
    /// `PDF_DEFLATE_REPLAY_RANS` plus an advisory `OBSERVATION_INDEX` built from
    /// the same physical scan, so narrow views can be served without walking the
    /// whole program (Phase 7.3).
    PdfDeflateReplayRansIndexed = 9,
    /// PDF `/Length` values and revision/xref structure regenerated from marked
    /// output positions as a *size* mechanism (Phase 13.1).
    PdfLengthRevision = 10,
    /// PDF COS-token phrase templates: recurring structural boilerplate
    /// (dictionary stems, object framing) stored once and instantiated by
    /// `EMIT_OBJECT` as a *size* mechanism (Phase 13.2).
    PdfCosTemplate = 11,
}

impl CandidateKind {
    /// Stable short name for reports and receipts.
    pub const fn name(self) -> &'static str {
        match self {
            CandidateKind::Raw => "RAW",
            CandidateKind::Rle => "RLE",
            CandidateKind::ByteRans => "BYTE_RANS",
            CandidateKind::PdfPhysical => "PDF_PHYSICAL",
            CandidateKind::PdfChannels => "PDF_CHANNELS",
            CandidateKind::PdfLayout => "PDF_LAYOUT",
            CandidateKind::PdfLayoutRans => "PDF_LAYOUT_RANS",
            CandidateKind::PdfDeflateReplay => "PDF_DEFLATE_REPLAY",
            CandidateKind::PdfDeflateReplayRans => "PDF_DEFLATE_REPLAY_RANS",
            CandidateKind::PdfDeflateReplayRansIndexed => "PDF_DEFLATE_REPLAY_RANS_INDEXED",
            CandidateKind::PdfLengthRevision => "PDF_LENGTH_REVISION",
            CandidateKind::PdfCosTemplate => "PDF_COS_TEMPLATE",
        }
    }
}

/// A proposed descriptor together with its originating family.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// Which family produced this proposal.
    pub kind: CandidateKind,
    /// The proposed descriptor.
    pub descriptor: Descriptor,
}

/// Generate the complete bounded candidate set for `input`.
///
/// Every candidate that currently applies is returned: RAW (always), then RLE,
/// BYTE_RANS, PDF_PHYSICAL, PDF_CHANNELS, and PDF_LAYOUT, each only when its
/// generator can express the input within `limits`. Order is deterministic and
/// matches the [`CandidateKind`] discriminant order, so the court's final
/// tie-break is stable.
///
/// This is the honest ablation surface: forcing a single kind must select from
/// exactly the same set the unforced court would have priced.
pub fn propose_all(input: &[u8], limits: Limits) -> Result<Vec<Candidate>> {
    let mut out = Vec::new();
    propose_each(input, limits, |c| {
        out.push(c);
        Ok(())
    })?;
    Ok(out)
}

/// Generate exactly the candidate of `kind`, or `Ok(None)` when it does not apply.
///
/// This is the forced-ablation path: it produces the **same** candidate the
/// unforced portfolio would have priced for `kind` (same proposer, same inputs)
/// but generates no other family. So a forced lane — e.g. `--force raw` for a
/// persistent-runtime ingest that assigns no value to the compression search —
/// costs only that family, instead of the whole portfolio.
#[allow(unreachable_patterns)]
pub fn propose_forced(
    input: &[u8],
    limits: Limits,
    kind: CandidateKind,
) -> Result<Option<Candidate>> {
    // The PDF families share one physical scan when asked for a single PDF kind.
    match kind {
        CandidateKind::PdfPhysical
        | CandidateKind::PdfChannels
        | CandidateKind::PdfLayout
        | CandidateKind::PdfLayoutRans
        | CandidateKind::PdfLengthRevision => {
            let physical = match crate::adapter::pdf::physical::scan(input, limits) {
                Ok(p) => p,
                Err(_) => return Ok(None),
            };
            match kind {
                CandidateKind::PdfPhysical => {
                    crate::adapter::pdf::adapter::propose_pdf_with(input, limits, &physical)
                }
                #[cfg(feature = "rans")]
                CandidateKind::PdfChannels => {
                    crate::adapter::pdf::adapter::propose_pdf_channels_with(
                        input, limits, &physical,
                    )
                }
                CandidateKind::PdfLayout => {
                    crate::adapter::pdf::layout::propose_pdf_layout_with(input, limits, &physical)
                }
                #[cfg(feature = "rans")]
                CandidateKind::PdfLayoutRans => {
                    crate::adapter::pdf::layout::propose_pdf_layout_rans_with(
                        input, limits, &physical,
                    )
                }
                CandidateKind::PdfLengthRevision => {
                    crate::adapter::pdf::length_revision::propose_pdf_length_revision_with(
                        input, limits, &physical,
                    )
                }
                _ => Ok(None),
            }
        }
        CandidateKind::Raw => Ok(Some(Candidate {
            kind,
            descriptor: opaque::propose(input, limits)?,
        })),
        CandidateKind::Rle => propose_rle(input, limits),
        #[cfg(feature = "rans")]
        CandidateKind::ByteRans => propose_byte_rans(input, limits),
        #[cfg(feature = "deflate-replay")]
        CandidateKind::PdfDeflateReplay => {
            crate::adapter::pdf::propose_pdf_deflate_replay(input, limits)
        }
        #[cfg(all(feature = "deflate-replay", feature = "rans"))]
        CandidateKind::PdfDeflateReplayRans => {
            crate::adapter::pdf::propose_pdf_deflate_replay_rans(input, limits)
        }
        #[cfg(all(feature = "deflate-replay", feature = "rans"))]
        CandidateKind::PdfDeflateReplayRansIndexed => {
            crate::adapter::pdf::propose_pdf_deflate_replay_rans_indexed(input, limits)
        }
        CandidateKind::PdfCosTemplate => {
            crate::adapter::pdf::propose_pdf_cos_template(input, limits)
        }
        _ => Ok(None),
    }
}

/// Generate the candidate set **one candidate at a time**, invoking `emit` for
/// each. Same set, same order as [`propose_all`], but the streaming form lets the
/// court keep only one unpriced candidate's payload resident, which bounds
/// encoder memory on large inputs (Phase 14).
///
pub fn propose_each<F>(input: &[u8], limits: Limits, mut emit: F) -> Result<()>
where
    F: FnMut(Candidate) -> Result<()>,
{
    emit(Candidate {
        kind: CandidateKind::Raw,
        descriptor: opaque::propose(input, limits)?,
    })?;
    if let Some(rle) = propose_rle(input, limits)? {
        emit(rle)?;
    }
    #[cfg(feature = "rans")]
    if let Some(byte_rans) = propose_byte_rans(input, limits)? {
        emit(byte_rans)?;
    }
    // One PDF physical scan, shared by every scan-dependent PDF proposer: each
    // used to re-scan, which made the portfolio ~5x one scan on large scanned
    // PDFs (Phase 14). A failed scan declines every scan-dependent candidate.
    let physical = crate::adapter::pdf::physical::scan(input, limits).ok();
    if let Some(p) = physical.as_ref() {
        if let Some(pdf) = crate::adapter::pdf::adapter::propose_pdf_with(input, limits, p)? {
            emit(pdf)?;
        }
        #[cfg(feature = "rans")]
        if let Some(pdf_channels) =
            crate::adapter::pdf::adapter::propose_pdf_channels_with(input, limits, p)?
        {
            emit(pdf_channels)?;
        }
        if let Some(pdf_layout) =
            crate::adapter::pdf::layout::propose_pdf_layout_with(input, limits, p)?
        {
            emit(pdf_layout)?;
        }
        #[cfg(feature = "rans")]
        if let Some(pdf_layout_rans) =
            crate::adapter::pdf::layout::propose_pdf_layout_rans_with(input, limits, p)?
        {
            emit(pdf_layout_rans)?;
        }
    }
    #[cfg(feature = "deflate-replay")]
    if let Some(pdf_deflate) = crate::adapter::pdf::propose_pdf_deflate_replay(input, limits)? {
        emit(pdf_deflate)?;
    }
    #[cfg(all(feature = "deflate-replay", feature = "rans"))]
    if let Some(pdf_deflate_rans) =
        crate::adapter::pdf::propose_pdf_deflate_replay_rans(input, limits)?
    {
        emit(pdf_deflate_rans)?;
    }
    #[cfg(all(feature = "deflate-replay", feature = "rans"))]
    if let Some(pdf_deflate_rans_indexed) =
        crate::adapter::pdf::propose_pdf_deflate_replay_rans_indexed(input, limits)?
    {
        emit(pdf_deflate_rans_indexed)?;
    }
    if let Some(p) = physical.as_ref()
        && let Some(pdf_length_revision) =
            crate::adapter::pdf::length_revision::propose_pdf_length_revision_with(
                input, limits, p,
            )?
    {
        emit(pdf_length_revision)?;
    }
    if let Some(pdf_cos_template) = crate::adapter::pdf::propose_pdf_cos_template(input, limits)? {
        emit(pdf_cos_template)?;
    }
    Ok(())
}

/// Generate the bounded candidate set for `input`.
///
/// Thin alias for [`propose_all`] kept for existing callers; it proposes exactly
/// the same set in the same order.
pub fn propose(input: &[u8], limits: Limits) -> Result<Vec<Candidate>> {
    propose_all(input, limits)
}

/// Propose an inline-run + `REPEAT_LAST` representation of `input`.
///
/// Each maximal run of equal bytes becomes one `INLINE` of that single byte,
/// optionally followed by a `REPEAT_LAST` repeating it `length - 1` more times.
/// The sequence is therefore always `INLINE, REPEAT_LAST, INLINE, ...`, so no
/// two `REPEAT_LAST` instructions are ever adjacent. `objects` is empty because
/// every byte is carried in the graph.
///
/// Returns `Ok(None)`, declining honestly, when the run list cannot be
/// expressed within `limits`: the program would need more instructions than
/// `max_graph_ops`, or a run is too long to reconstruct with a single
/// `REPEAT_LAST` (`length - 1` exceeds `max_repeat_count` or `u32::MAX`).
///
/// The decline check is done in a first **streaming** pass that keeps only the
/// running run count and the longest run in `O(1)` memory, and the `ops` are
/// materialized in a second pass only once the limits admit them. Storing a
/// `(u8, u64)` per run up front cost 16 bytes per run — ~16x the input for an
/// incompressible file — which OOM-killed a 409 MB PDF under a 6 GiB cap before
/// this candidate could decline (Phase 16.3). Because a candidate that passes
/// has `runs <= max_graph_ops / 2`, the second pass is bounded by the graph-op
/// budget and never by the raw input length.
pub fn propose_rle(input: &[u8], limits: Limits) -> Result<Option<Candidate>> {
    // Pass 1: count maximal runs (and the longest run) without materializing
    // the run list, so an incompressible input declines at O(1) extra memory.
    let mut run_count: u64 = 0;
    let mut max_run_len: u64 = 0;
    let mut prev: Option<u8> = None;
    let mut len: u64 = 0;
    for &b in input {
        if prev == Some(b) {
            len += 1;
        } else {
            max_run_len = max_run_len.max(len);
            run_count += 1;
            prev = Some(b);
            len = 1;
        }
    }
    max_run_len = max_run_len.max(len);

    // Worst case is one INLINE plus one REPEAT_LAST per run.
    if run_count.saturating_mul(2) > limits.max_graph_ops as u64 {
        return Ok(None);
    }

    // A run must be reconstructible by a single INLINE + REPEAT_LAST pair. Only
    // the longest run can violate this, so checking it is equivalent to the
    // former per-run loop.
    let max_extra = max_run_len.saturating_sub(1);
    if max_extra > limits.max_repeat_count || max_extra > u32::MAX as u64 {
        return Ok(None);
    }

    // Pass 2: build the ops. Safe from the bound above.
    let mut ops: Vec<Op> = Vec::with_capacity(run_count as usize * 2);
    let mut prev: Option<u8> = None;
    let mut len: u64 = 0;
    for &b in input {
        if prev == Some(b) {
            len += 1;
        } else {
            if let Some(byte) = prev {
                ops.push(Op::Inline { bytes: vec![byte] });
                if len > 1 {
                    ops.push(Op::RepeatLast {
                        count: (len - 1) as u32,
                    });
                }
            }
            prev = Some(b);
            len = 1;
        }
    }
    if let Some(byte) = prev {
        ops.push(Op::Inline { bytes: vec![byte] });
        if len > 1 {
            ops.push(Op::RepeatLast {
                count: (len - 1) as u32,
            });
        }
    }

    let descriptor = Descriptor {
        universe: UNIVERSE.to_string(),
        source_format: SOURCE_FORMAT_OPAQUE,
        format_basis: "opaque;rle-runs".to_string(),
        models: vec![],
        channels: vec![],
        objects: vec![],
        program: Program::new(ops),
        observation_index: None,
        seek_directory: false,
        checkpoints: None,
        source_sha256: sha256(input),
        source_len: input.len() as u64,
    };
    Ok(Some(Candidate {
        kind: CandidateKind::Rle,
        descriptor,
    }))
}

/// Propose an order-0 byte-rANS representation of the whole `input` as one
/// entropy channel.
///
/// The 256-entry normalized model and the channel header are fully serialized
/// and charged by the court, so this candidate only wins when order-0 coding
/// recovers more bytes than the model costs. Declines (`Ok(None)`) honestly:
///
/// - empty input: RAW is trivially smaller and there is nothing to code;
/// - `input.len() > limits.max_channel_symbols`: a single channel cannot carry
///   it, so the candidate is not expressible within the declared bounds.
///
/// Determinism follows from the pure `from_counts` normalizer and from
/// `encode_channel`, which both depend only on their inputs.
#[cfg(feature = "rans")]
pub fn propose_byte_rans(input: &[u8], limits: Limits) -> Result<Option<Candidate>> {
    if input.is_empty() || input.len() as u64 > limits.max_channel_symbols {
        return Ok(None);
    }

    let mut counts = [0u64; 256];
    for &b in input {
        counts[b as usize] += 1;
    }
    let model = EntropyModel::from_counts(&counts, 12)?;
    let capsule = encode_channel(&model, input)?;

    let channel = EntropyChannelDescriptor {
        coder: CODER_ORDER0_BYTE_RANS,
        coder_version: CODER_VERSION_1,
        scale_bits: model.scale_bits,
        lane_count: 1,
        model_id: 0,
        symbol_count: capsule.symbol_count,
        decoded_length: capsule.decoded_length,
        initial_state: capsule.initial_state,
        payload: capsule.payload,
    };

    let descriptor = Descriptor {
        universe: UNIVERSE.to_string(),
        source_format: SOURCE_FORMAT_OPAQUE,
        format_basis: "opaque;byte-rans".to_string(),
        models: vec![model],
        channels: vec![channel],
        objects: vec![],
        program: Program::new(vec![Op::DecodeChannel { channel_id: 0 }]),
        observation_index: None,
        seek_directory: false,
        checkpoints: None,
        source_sha256: sha256(input),
        source_len: input.len() as u64,
    };

    Ok(Some(Candidate {
        kind: CandidateKind::ByteRans,
        descriptor,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic xorshift64 PRNG for incompressible test data.
    fn xorshift64(state: &mut u64) -> u64 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *state = x;
        x
    }

    fn xorshift_bytes(n: usize, seed: u64) -> Vec<u8> {
        let mut state = seed | 1; // avoid the zero fixed point
        let mut out = Vec::with_capacity(n + 8);
        while out.len() < n {
            out.extend_from_slice(&xorshift64(&mut state).to_le_bytes());
        }
        out.truncate(n);
        out
    }

    fn assert_exact(bytes: &[u8], input: &[u8], limits: Limits) {
        let (out, parsed) = crate::materialize::decode_to_bytes(bytes, limits).unwrap();
        assert_eq!(out, input, "materialized bytes must equal the source");
        assert_eq!(out.len() as u64, parsed.descriptor.source_len);
        assert_eq!(
            crate::integrity::sha256(&out),
            crate::integrity::sha256(input)
        );
    }

    #[test]
    fn rle_wins_on_zeros() {
        let input = vec![0u8; 65536];
        let (bytes, report) = crate::encode::encode(&input, Limits::DEFAULT).unwrap();
        assert_eq!(report.kind, CandidateKind::Rle);
        assert!(
            report.encoded_len < 512,
            "RLE encoding of zeros was {} bytes",
            report.encoded_len
        );
        assert_exact(&bytes, &input, Limits::DEFAULT);
    }

    #[test]
    fn rle_wins_on_long_runs() {
        let mut input = Vec::new();
        input.extend_from_slice(&[0xAAu8; 1000]);
        input.extend_from_slice(&[0x00, 0x01]);
        input.extend_from_slice(&[0x55u8; 5000]);
        input.extend_from_slice(b"tail");

        let (bytes, report) = crate::encode::encode(&input, Limits::DEFAULT).unwrap();
        assert_eq!(report.kind, CandidateKind::Rle);
        assert!(report.encoded_len < 512);
        assert_exact(&bytes, &input, Limits::DEFAULT);
    }

    #[test]
    fn raw_wins_on_incompressible() {
        let input = xorshift_bytes(64 * 1024, 0x9E37_79B9_7F4A_7C15);
        let (bytes, report) = crate::encode::encode(&input, Limits::DEFAULT).unwrap();
        assert_eq!(
            report.kind,
            CandidateKind::Raw,
            "incompressible data must be stored RAW"
        );
        assert_exact(&bytes, &input, Limits::DEFAULT);
    }

    #[test]
    fn single_byte_is_rle() {
        let input = [7u8];
        let (bytes, report) = crate::encode::encode(&input, Limits::DEFAULT).unwrap();
        assert_eq!(report.kind, CandidateKind::Rle);
        assert!(report.encoded_len < 512);
        assert_exact(&bytes, &input, Limits::DEFAULT);
    }

    #[test]
    fn rle_declines_when_graph_too_big() {
        let input = vec![0u8; 100];
        let limits = Limits {
            max_graph_ops: 1,
            ..Limits::DEFAULT
        };
        assert!(
            propose_rle(&input, limits).unwrap().is_none(),
            "100 single-byte runs cannot fit in a one-op graph"
        );
        // With RLE declined, the court must fall back to another exact lane.
        // (The compact v2 model makes BYTE_RANS the winner here; whether it or
        // RAW wins depends on model cost, so only "not RLE" is pinned.)
        let (bytes, report) = crate::encode::encode(&input, limits).unwrap();
        assert_ne!(report.kind, CandidateKind::Rle);
        assert_exact(&bytes, &input, limits);
    }

    #[test]
    fn rle_declines_on_large_incompressible() {
        // A near-incompressible multi-megabyte buffer has far more runs than
        // `max_graph_ops / 2`, so RLE must decline. Before Phase 16.3 this built a
        // `(u8, u64)` entry per run first, costing ~16x the input length (the
        // 409 MB OOM); the streaming two-pass form must reach the same `None`
        // verdict at O(1) extra memory. The assertion is the verdict, which is
        // what the OOM run failed to reach.
        let input = xorshift_bytes(4 * 1024 * 1024, 0xD1B5_4A32_D192_ED03);
        assert!(
            propose_rle(&input, Limits::DEFAULT).unwrap().is_none(),
            "an incompressible buffer cannot be expressed as an RLE graph"
        );
    }

    #[test]
    fn rle_ops_match_run_list_reference() {
        // Pin the streaming two-pass construction to the original run-list
        // definition: one INLINE per maximal run, plus a REPEAT_LAST when the run
        // is longer than one byte, in file order.
        let mut input = Vec::new();
        for &(byte, length) in &[(b'a', 6u64), (b'b', 1), (b'c', 300), (b'a', 2), (b'z', 1)] {
            input.extend(std::iter::repeat_n(byte, length as usize));
        }
        let mut expected: Vec<Op> = Vec::new();
        for &(byte, length) in &[(b'a', 6u64), (b'b', 1), (b'c', 300), (b'a', 2), (b'z', 1)] {
            expected.push(Op::Inline { bytes: vec![byte] });
            if length > 1 {
                expected.push(Op::RepeatLast {
                    count: (length - 1) as u32,
                });
            }
        }
        let cand = propose_rle(&input, Limits::DEFAULT)
            .unwrap()
            .expect("a 5-run input is expressible");
        assert_eq!(cand.descriptor.program.ops, expected);
    }

    #[cfg(feature = "rans")]
    #[test]
    fn byte_rans_wins_on_text() {
        let input = b"The quick brown fox jumps over the lazy dog. ".repeat(1500);
        // Pin the byte-rANS lane directly rather than the auto winner: a stronger
        // phrase-template lane (`PDF_COS_TEMPLATE`, Phase 13.2) can win the full
        // court on a repeated phrase, so the order-0 claim is asserted by forcing
        // the one-element court rather than by observing the auto winner.
        let (bytes, report) =
            crate::encode::encode_with(&input, Limits::DEFAULT, Some(CandidateKind::ByteRans))
                .unwrap();
        assert_eq!(report.kind, CandidateKind::ByteRans);
        assert!(
            report.encoded_len < report.source_len,
            "order-0 rANS must beat RAW on low-entropy text: {} vs {}",
            report.encoded_len,
            report.source_len
        );
        assert_exact(&bytes, &input, Limits::DEFAULT);
    }

    #[cfg(feature = "rans")]
    #[test]
    fn byte_rans_exact_on_all_byte_values() {
        // Exercise every byte value through the channel. A uniform `0..=255`
        // stream is incompressible at order 0 (and the 516-byte model makes rANS
        // lose to RAW), so the body is a skewed, deterministic stream whose head
        // guarantees all 256 symbols are present and nonzero-frequency.
        let mut input: Vec<u8> = (0..=255u8).collect();
        let mut state = 0x2545_F491_4F6C_DD1D;
        while input.len() < 64 * 1024 {
            let r = xorshift64(&mut state);
            if !r.is_multiple_of(8) {
                input.push(0x00);
            } else {
                input.push((r >> 32) as u8);
            }
        }
        input.truncate(64 * 1024);

        let (bytes, report) = crate::encode::encode(&input, Limits::DEFAULT).unwrap();
        assert_eq!(report.kind, CandidateKind::ByteRans);
        assert_exact(&bytes, &input, Limits::DEFAULT);
    }

    #[test]
    fn byte_rans_is_deterministic() {
        let input = b"deterministic byte rANS stream ".repeat(600);
        let (a, _) = crate::encode::encode(&input, Limits::DEFAULT).unwrap();
        let (b, _) = crate::encode::encode(&input, Limits::DEFAULT).unwrap();
        assert_eq!(a, b, ".voldoc bytes must be identical across encodes");
    }

    #[cfg(feature = "rans")]
    #[test]
    fn byte_rans_declines_empty() {
        assert!(
            propose_byte_rans(&[], Limits::DEFAULT).unwrap().is_none(),
            "empty input must decline: RAW is trivially smaller"
        );
    }

    #[cfg(feature = "rans")]
    #[test]
    fn model_cost_is_charged() {
        // On a two-byte input the 516-byte canonical model cannot pay for
        // itself, so BYTE_RANS must lose the complete-cost court. This documents
        // model-cost honesty.
        let input = b"ab";
        let (bytes, report) = crate::encode::encode(input, Limits::DEFAULT).unwrap();
        assert_ne!(report.kind, CandidateKind::ByteRans);
        assert_exact(&bytes, input, Limits::DEFAULT);
    }
}
