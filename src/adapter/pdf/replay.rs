//! The Phase-6 exact DEFLATE replay candidates (`PDF_DEFLATE_REPLAY`,
//! `PDF_DEFLATE_REPLAY_RANS`).
//!
//! VOLE-Document's own physical scanner finds the stream spans and their
//! `/Filter`; `preflate` is never allowed to discover streams. For each
//! eligible lone-`FlateDecode` stream whose encoded bytes are a zlib stream, a
//! verified [`ReplayPlan`](crate::codec::deflate::ReplayPlan) replaces the
//! stream's literal `INLINE` with the three-op sequence
//!
//! ```text
//! INLINE(zlib header) · DEFLATE_REPLAY(plaintext, corrections) · INLINE(adler32)
//! ```
//!
//! so the program reconstructs the *original* compressed bytes exactly. Every
//! other span stays a literal `INLINE`.
//!
//! Two variants differ only in how the plaintext is stored:
//!
//! * `PDF_DEFLATE_REPLAY` — plaintexts are raw `OBJECT`s (content-deduplicated).
//! * `PDF_DEFLATE_REPLAY_RANS` — each unique plaintext is its own order-0
//!   byte-rANS entropy channel and is referenced (shared) by every stream that
//!   produces it. The materializer decodes each channel once, so shared plaintext
//!   costs storage once and CPU once.
//!
//! Neither is assumed profitable; the complete-cost court decides.

use std::collections::{HashMap, HashSet};

use crate::SOURCE_FORMAT_PDF;
#[cfg(feature = "rans")]
use crate::container::observation::{
    ObservationIndex, ObservationSelector, OpEntry, SECTION_OP_TABLE, SECTION_PDF_SELECTORS,
    SELECTOR_OBJECT, SELECTOR_REVISION, SELECTOR_STREAM,
};
use crate::container::{Descriptor, ObjectSource, UNIVERSE};
use crate::dra::op::{DEFLATE_SOURCE_CHANNEL, DEFLATE_SOURCE_OBJECT};
use crate::dra::{Op, Program};
use crate::encode::candidates::{Candidate, CandidateKind};
use crate::error::Result;
use crate::integrity::sha256;
use crate::limits::Limits;

use crate::codec::deflate::{ReplayPlan, try_replay, try_replay_detailed};

use super::cos::FilterClass;
use super::physical::{PdfPhysical, scan};

/// Eligible replays keyed by their exact stream-data span `(start, len)`.
pub(super) type SpanPlans = HashMap<(u64, u64), ReplayPlan>;

/// The eligible replays keyed by their exact stream-data span, plus the physical
/// summary they were derived from. `None` when the file is not a validated PDF or
/// no stream replays exactly.
pub(super) fn collect_plans(
    input: &[u8],
    limits: Limits,
) -> Result<Option<(PdfPhysical, SpanPlans)>> {
    let physical = match scan(input, limits) {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };
    if physical.header.is_none() || physical.objects.is_empty() || physical.eofs.is_empty() {
        return Ok(None);
    }

    let mut by_span: SpanPlans = HashMap::new();
    for stream in &physical.streams {
        if stream.filter != FilterClass::FlateDecode {
            continue;
        }
        if by_span.len() as u64 >= u64::from(limits.max_object_count) {
            break;
        }
        let start = stream.data_start as usize;
        let end = match start.checked_add(stream.data_len as usize) {
            Some(e) if e <= input.len() => e,
            _ => continue,
        };
        if let Some(plan) = try_replay(&input[start..end], limits) {
            by_span.insert((stream.data_start, stream.data_len), plan);
        }
    }
    if by_span.is_empty() {
        return Ok(None);
    }
    Ok(Some((physical, by_span)))
}

/// Propose a `PDF_DEFLATE_REPLAY` candidate (raw plaintext objects).
///
/// Declines (`Ok(None)`) rather than truncating when the op or object count would
/// exceed `limits`, or when no stream replays exactly.
pub fn propose_pdf_deflate_replay(input: &[u8], limits: Limits) -> Result<Option<Candidate>> {
    let Some((physical, by_span)) = collect_plans(input, limits)? else {
        return Ok(None);
    };
    if physical.spans.len() as u64 > u64::from(limits.max_graph_ops) {
        return Ok(None);
    }

    let mut objects: Vec<Vec<u8>> = Vec::new();
    let mut index: HashMap<Vec<u8>, u32> = HashMap::new();
    let mut ops: Vec<Op> = Vec::with_capacity(physical.spans.len());
    let mut replayed = 0usize;

    for span in &physical.spans {
        let key = (span.start, span.len);
        if let Some(plan) = by_span.get(&key) {
            let plaintext_id = intern(&mut objects, &mut index, plan.plaintext.clone());
            let corrections_id = intern(&mut objects, &mut index, plan.corrections.clone());
            if objects.len() as u64 > u64::from(limits.max_object_count) {
                return Ok(None);
            }
            ops.push(Op::Inline {
                bytes: plan.header.to_vec(),
            });
            ops.push(Op::DeflateReplay {
                replay_codec: crate::dra::op::REPLAY_DEFLATE_PREFLATE_0_7_6,
                source_kind: DEFLATE_SOURCE_OBJECT,
                source_id: plaintext_id,
                corrections_object: corrections_id,
                declared_output_len: plan.raw_len,
            });
            ops.push(Op::Inline {
                bytes: plan.adler.to_vec(),
            });
            replayed += 1;
        } else {
            let start = span.start as usize;
            let end = start + span.len as usize;
            ops.push(Op::Inline {
                bytes: input[start..end].to_vec(),
            });
        }
        if ops.len() as u64 > u64::from(limits.max_graph_ops) {
            return Ok(None);
        }
    }

    let format_basis = format!(
        "pdf-deflate-replay;streams={};replayed={replayed};objects={}",
        physical.streams.len(),
        objects.len()
    );

    let descriptor = Descriptor {
        universe: UNIVERSE.to_string(),
        source_format: SOURCE_FORMAT_PDF,
        format_basis,
        models: vec![],
        channels: vec![],
        objects: objects.into_iter().map(ObjectSource::Inline).collect(),
        program: Program::new(ops),
        observation_index: None,
        seek_directory: false,
        source_sha256: sha256(input),
        source_len: input.len() as u64,
    };

    Ok(Some(Candidate {
        kind: CandidateKind::PdfDeflateReplay,
        descriptor,
    }))
}

/// Propose a `PDF_DEFLATE_REPLAY_RANS` candidate: each unique plaintext is coded
/// as its own order-0 byte-rANS channel and shared across every stream that
/// produces it; corrections stay deduplicated objects.
///
/// Declines when the graph, object, channel, or model count would exceed limits,
/// or when no stream replays exactly.
#[cfg(feature = "rans")]
pub fn propose_pdf_deflate_replay_rans(input: &[u8], limits: Limits) -> Result<Option<Candidate>> {
    Ok(
        build_pdf_deflate_replay_rans(input, limits)?.map(|(_, descriptor)| Candidate {
            kind: CandidateKind::PdfDeflateReplayRans,
            descriptor,
        }),
    )
}

/// Build the `PDF_DEFLATE_REPLAY_RANS` descriptor together with the physical
/// scan it was derived from.
///
/// The scan is returned so the indexed variant can attach an
/// [`ObservationIndex`] without re-scanning (and so the two views can never
/// disagree about offsets). The descriptor is byte-identical to what
/// [`propose_pdf_deflate_replay_rans`] returned before the indexed lane existed.
#[cfg(feature = "rans")]
fn build_pdf_deflate_replay_rans(
    input: &[u8],
    limits: Limits,
) -> Result<Option<(PdfPhysical, Descriptor)>> {
    use crate::entropy::{
        CODER_ORDER0_BYTE_RANS, CODER_VERSION_1, EntropyChannelDescriptor, EntropyModel,
        encode_channel,
    };

    let Some((physical, by_span)) = collect_plans(input, limits)? else {
        return Ok(None);
    };
    if physical.spans.len() as u64 > u64::from(limits.max_graph_ops) {
        return Ok(None);
    }

    let mut models: Vec<EntropyModel> = Vec::new();
    let mut channels: Vec<EntropyChannelDescriptor> = Vec::new();
    let mut channel_id: HashMap<Vec<u8>, u32> = HashMap::new();
    let mut objects: Vec<Vec<u8>> = Vec::new();
    let mut object_id: HashMap<Vec<u8>, u32> = HashMap::new();
    let mut ops: Vec<Op> = Vec::with_capacity(physical.spans.len());
    let mut replayed = 0usize;

    for span in &physical.spans {
        let key = (span.start, span.len);
        if let Some(plan) = by_span.get(&key) {
            // One shared channel per unique plaintext.
            let cid = if let Some(&id) = channel_id.get(&plan.plaintext) {
                id
            } else {
                if channels.len() as u64 >= u64::from(limits.max_channel_count)
                    || models.len() as u64 >= u64::from(limits.max_model_count)
                {
                    return Ok(None);
                }
                let mut counts = [0u64; crate::entropy::ALPHABET];
                for &b in &plan.plaintext {
                    counts[b as usize] += 1;
                }
                let model = EntropyModel::from_counts(&counts, 12)?;
                let scale_bits = model.scale_bits;
                let capsule = encode_channel(&model, &plan.plaintext)?;
                let id = channels.len() as u32;
                models.push(model);
                channels.push(EntropyChannelDescriptor {
                    coder: CODER_ORDER0_BYTE_RANS,
                    coder_version: CODER_VERSION_1,
                    scale_bits,
                    lane_count: 1,
                    model_id: id,
                    symbol_count: capsule.symbol_count,
                    decoded_length: capsule.decoded_length,
                    initial_state: capsule.initial_state,
                    payload: capsule.payload,
                });
                channel_id.insert(plan.plaintext.clone(), id);
                id
            };
            let corr = intern(&mut objects, &mut object_id, plan.corrections.clone());
            if objects.len() as u64 > u64::from(limits.max_object_count) {
                return Ok(None);
            }
            ops.push(Op::Inline {
                bytes: plan.header.to_vec(),
            });
            ops.push(Op::DeflateReplay {
                replay_codec: crate::dra::op::REPLAY_DEFLATE_PREFLATE_0_7_6,
                source_kind: DEFLATE_SOURCE_CHANNEL,
                source_id: cid,
                corrections_object: corr,
                declared_output_len: plan.raw_len,
            });
            ops.push(Op::Inline {
                bytes: plan.adler.to_vec(),
            });
            replayed += 1;
        } else {
            let start = span.start as usize;
            let end = start + span.len as usize;
            ops.push(Op::Inline {
                bytes: input[start..end].to_vec(),
            });
        }
        if ops.len() as u64 > u64::from(limits.max_graph_ops) {
            return Ok(None);
        }
    }

    let format_basis = format!(
        "pdf-deflate-replay-rans;streams={};replayed={replayed};channels={};objects={}",
        physical.streams.len(),
        channels.len(),
        objects.len()
    );

    let descriptor = Descriptor {
        universe: UNIVERSE.to_string(),
        source_format: SOURCE_FORMAT_PDF,
        format_basis,
        models,
        channels,
        objects: objects.into_iter().map(ObjectSource::Inline).collect(),
        program: Program::new(ops),
        observation_index: None,
        seek_directory: false,
        source_sha256: sha256(input),
        source_len: input.len() as u64,
    };

    Ok(Some((physical, descriptor)))
}

/// Propose a `PDF_DEFLATE_REPLAY_RANS_INDEXED` candidate: the exact
/// `PDF_DEFLATE_REPLAY_RANS` descriptor plus an advisory [`ObservationIndex`].
///
/// The index is built from the same [`PdfPhysical`] scan that produced the
/// descriptor's spans, so its PDF-selector ranges are the source offsets the
/// program reproduces. It is never authority: `Descriptor::parse` re-derives the
/// op table with `Program::analyze_ops` and rejects any disagreement, and the
/// complete-cost court charges the record's bytes.
///
/// Declines (`Ok(None)`) honestly on any limit breach -- too many ops or
/// selectors, an op output that does not fit `u32`, an index record larger than
/// `max_record_len`, a selector outside the source, or a value that does not fit
/// the selector's `u32`/`u16` fields -- so a reader never receives an index the
/// program contradicts.
#[cfg(all(feature = "deflate-replay", feature = "rans"))]
pub fn propose_pdf_deflate_replay_rans_indexed(
    input: &[u8],
    limits: Limits,
) -> Result<Option<Candidate>> {
    let Some((physical, mut descriptor)) = build_pdf_deflate_replay_rans(input, limits)? else {
        return Ok(None);
    };
    let Some(index) = build_observation_index(&physical, &descriptor, limits)? else {
        return Ok(None);
    };
    descriptor.observation_index = Some(index);
    // Only this candidate enables the optional seek DIRECTORY record; every other
    // construction site leaves `seek_directory == false`, so their bytes are the
    // exact Phase-7 record sequence.
    descriptor.seek_directory = true;
    Ok(Some(Candidate {
        kind: CandidateKind::PdfDeflateReplayRansIndexed,
        descriptor,
    }))
}

/// Build the advisory observation index for a linear replay program.
///
/// The op table is derived from [`Program::analyze_ops`] (authoritative), and
/// each entry records the op's primary dependency. The PDF selector table is
/// derived from the same physical scan: the replay program reproduces the source
/// in physical span order, so an output offset equals the corresponding source
/// offset, and the object/stream/revision ranges are the scanner's own spans.
#[cfg(feature = "rans")]
fn build_observation_index(
    physical: &PdfPhysical,
    descriptor: &Descriptor,
    limits: Limits,
) -> Result<Option<ObservationIndex>> {
    let object_lens: Vec<u64> = descriptor.objects.iter().map(|o| o.len()).collect();
    let channel_lens: Vec<u64> = descriptor
        .channels
        .iter()
        .map(|c| c.decoded_length)
        .collect();
    // The program is authoritative. A limit breach declines the indexed lane
    // rather than aborting the whole candidate court.
    let per_op = match descriptor
        .program
        .analyze_ops(&object_lens, &channel_lens, limits)
    {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    if per_op.len() != descriptor.program.ops.len() {
        return Ok(None);
    }

    let mut ops: Vec<OpEntry> = Vec::with_capacity(per_op.len());
    for (i, len) in per_op.iter().enumerate() {
        let Ok(out_len) = u32::try_from(*len) else {
            return Ok(None);
        };
        let (dep_kind, dep_id) = primary_dependency(&descriptor.program.ops[i]);
        ops.push(OpEntry {
            out_len,
            dep_kind,
            dep_id,
        });
    }

    // Bound the selector table before allocation.
    let raw_count = physical
        .objects
        .len()
        .saturating_add(physical.streams.len())
        .saturating_add(physical.revisions.len());
    if raw_count as u64 > u64::from(limits.max_index_selectors) {
        return Ok(None);
    }

    let mut selectors: Vec<ObservationSelector> = Vec::with_capacity(raw_count);
    let mut seen: HashSet<(u8, u32, u32)> = HashSet::new();

    for o in &physical.objects {
        let (Ok(number), Ok(generation)) = (u32::try_from(o.number), u32::try_from(o.generation))
        else {
            return Ok(None);
        };
        let len = o.end.saturating_sub(o.start);
        if len == 0
            || o.end > descriptor.source_len
            || !seen.insert((SELECTOR_OBJECT, number, generation))
        {
            continue;
        }
        selectors.push(ObservationSelector {
            kind: SELECTOR_OBJECT,
            number,
            generation,
            out_off: o.start,
            out_len: len,
        });
    }
    for s in &physical.streams {
        let (Ok(number), Ok(generation)) = (u32::try_from(s.object), u32::try_from(s.generation))
        else {
            return Ok(None);
        };
        let end = s.data_start.saturating_add(s.data_len);
        if s.data_len == 0
            || end > descriptor.source_len
            || !seen.insert((SELECTOR_STREAM, number, generation))
        {
            continue;
        }
        selectors.push(ObservationSelector {
            kind: SELECTOR_STREAM,
            number,
            generation,
            out_off: s.data_start,
            out_len: s.data_len,
        });
    }
    for r in &physical.revisions {
        let len = r.end.saturating_sub(r.start);
        if len == 0
            || r.end > descriptor.source_len
            || !seen.insert((SELECTOR_REVISION, r.index, 0))
        {
            continue;
        }
        selectors.push(ObservationSelector {
            kind: SELECTOR_REVISION,
            number: r.index,
            generation: 0,
            out_off: r.start,
            out_len: len,
        });
    }

    // Deterministic order required by the index contract.
    selectors.sort_by_key(|s| (s.kind, s.number, s.generation, s.out_off));

    let index = ObservationIndex {
        section_flags: SECTION_OP_TABLE | SECTION_PDF_SELECTORS,
        ops,
        selectors,
        digests: Vec::new(),
    };

    // The advisory record must fit one bounded record.
    if index.encode()?.len() as u64 > u64::from(limits.max_record_len) {
        return Ok(None);
    }
    Ok(Some(index))
}

/// The primary dependency of an op: the object or channel its output is read
/// from, or [`DEP_NONE`] for literal/bookkeeping ops.
///
/// Re-exported from the container's observation module so the index builder and
/// every other construction site agree on the label.
#[cfg(feature = "rans")]
fn primary_dependency(op: &Op) -> (u8, u32) {
    crate::container::observation::primary_dependency(op)
}

/// Append `bytes` to `objects`, returning the index of an existing equal object
/// when one is already present (content deduplication, first-seen order).
fn intern(objects: &mut Vec<Vec<u8>>, index: &mut HashMap<Vec<u8>, u32>, bytes: Vec<u8>) -> u32 {
    if let Some(&id) = index.get(&bytes) {
        return id;
    }
    let id = objects.len() as u32;
    index.insert(bytes.clone(), id);
    objects.push(bytes);
    id
}

/// rANS scale bits used for the `rans_plaintext_bytes` diagnostic. Matches the
/// order-0 byte-rANS scale the Phase-6 `PDF_DEFLATE_REPLAY_RANS` candidate uses.
pub const RANS_SCALE_BITS: u8 = 12;

/// Per-`FlateDecode`-stream DEFLATE replay statistics for diagnostics.
///
/// This is a *measurement* of the correction ratio, not a candidate and not a
/// wire form: it never influences what the complete-cost court selects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamStats {
    /// Object number of the enclosing indirect object.
    pub object: u64,
    /// Generation number of the enclosing indirect object.
    pub generation: u64,
    /// Exact stream-data byte length (`data_len`).
    pub compressed_bytes: u64,
    /// Whether the stream produced a verified exact-replay plan.
    pub replayed: bool,
    /// Stable snake_case decline reason when `replayed` is false.
    pub decline_reason: Option<&'static str>,
    /// Decompressed plaintext length, when replayed.
    pub plaintext_bytes: Option<u64>,
    /// `preflate` correction-blob length, when replayed.
    pub correction_bytes: Option<u64>,
    /// Bytes of the plaintext's own order-0 byte-rANS channel (model wire bytes
    /// plus the fixed channel-descriptor header plus the renormalization
    /// payload), when replayed and the `rans` feature is enabled.
    pub rans_plaintext_bytes: Option<u64>,
}

/// Aggregate over all `FlateDecode` streams of one input.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeflateSummary {
    /// Number of `FlateDecode` streams seen.
    pub flate_streams: u64,
    /// Number that produced a verified exact-replay plan.
    pub replayed: u64,
    /// Number that declined.
    pub declined: u64,
    /// Sum of `data_len` over every `FlateDecode` stream.
    pub compressed_bytes: u64,
    /// Sum of plaintext lengths over replayed streams.
    pub plaintext_bytes: u64,
    /// Sum of correction-blob lengths over replayed streams.
    pub correction_bytes: u64,
    /// Sum of `rans_plaintext_bytes` over replayed streams (per stream, not
    /// deduplicated; a diagnostic upper bound, not the shared-channel cost).
    pub rans_plaintext_bytes: u64,
    /// Deduplicated complete cost of the `PDF_DEFLATE_REPLAY_RANS` shape: the
    /// order-0 rANS size of each **unique** plaintext (charged once, however
    /// many streams reproduce it) plus the length of each **unique** correction
    /// blob. This mirrors what the shared-channel candidate actually stores, so
    /// it is ≤ the naive per-stream sum
    /// `rans_plaintext_bytes + correction_bytes`. Zero when the `rans` feature
    /// is disabled (the plaintext channels are not built).
    pub replayed_rans_dedup_bytes: u64,
}

/// The `deflate-stats` view of one input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeflateStats {
    /// Mirrors [`super::detect`].
    pub is_pdf: bool,
    /// One entry per `FlateDecode` stream (empty for a non-PDF).
    pub streams: Vec<StreamStats>,
    /// The aggregate over `streams`.
    pub summary: DeflateSummary,
}

/// Measure the exact-DEFLATE-replay correction ratio over every `FlateDecode`
/// stream of `input`.
///
/// A non-PDF yields `is_pdf: false` with no streams (never an error). A real
/// scan failure (I/O, resource limit, coverage) is returned as a typed error.
/// The decisive number is `correction_bytes / compressed_bytes`: how much of a
/// producer's compressed bitstream is *predictable* from the plaintext and the
/// pinned `preflate` model. This function is diagnostics only.
pub fn deflate_stats(input: &[u8], limits: Limits) -> Result<DeflateStats> {
    if !super::detect(input, limits) {
        return Ok(DeflateStats {
            is_pdf: false,
            streams: Vec::new(),
            summary: DeflateSummary::default(),
        });
    }
    let physical = scan(input, limits)?;
    let mut streams: Vec<StreamStats> = Vec::new();
    let mut summary = DeflateSummary::default();
    // Content-deduplicated accounting mirroring the shared-channel candidate:
    // one rANS charge per unique plaintext and one byte charge per unique
    // correction blob, however many streams reproduce them.
    let mut unique_plaintexts: HashSet<Vec<u8>> = HashSet::new();
    let mut unique_corrections: HashSet<Vec<u8>> = HashSet::new();

    for stream in &physical.streams {
        if stream.filter != FilterClass::FlateDecode {
            continue;
        }
        summary.flate_streams += 1;
        summary.compressed_bytes += stream.data_len;

        let mut st = StreamStats {
            object: stream.object,
            generation: stream.generation,
            compressed_bytes: stream.data_len,
            replayed: false,
            decline_reason: None,
            plaintext_bytes: None,
            correction_bytes: None,
            rans_plaintext_bytes: None,
        };

        let start = stream.data_start as usize;
        let end = start
            .checked_add(stream.data_len as usize)
            .filter(|&e| e <= input.len());
        match end {
            None => st.decline_reason = Some("span_out_of_range"),
            Some(end) => match try_replay_detailed(&input[start..end], limits) {
                Ok(plan) => {
                    let plaintext_bytes = plan.plaintext.len() as u64;
                    let correction_bytes = plan.corrections.len() as u64;
                    st.replayed = true;
                    st.plaintext_bytes = Some(plaintext_bytes);
                    st.correction_bytes = Some(correction_bytes);
                    st.rans_plaintext_bytes = rans_plaintext_bytes(&plan.plaintext);
                    summary.replayed += 1;
                    summary.plaintext_bytes += plaintext_bytes;
                    summary.correction_bytes += correction_bytes;
                    summary.rans_plaintext_bytes += st.rans_plaintext_bytes.unwrap_or(0);
                    if unique_corrections.insert(plan.corrections.clone()) {
                        summary.replayed_rans_dedup_bytes += correction_bytes;
                    }
                    if unique_plaintexts.insert(plan.plaintext.clone()) {
                        summary.replayed_rans_dedup_bytes += st.rans_plaintext_bytes.unwrap_or(0);
                    }
                }
                Err(reason) => st.decline_reason = Some(reason.name()),
            },
        }
        if !st.replayed {
            summary.declined += 1;
        }
        streams.push(st);
    }

    Ok(DeflateStats {
        is_pdf: true,
        streams,
        summary,
    })
}

/// Bytes of the plaintext's own order-0 byte-rANS channel: the encoded model
/// wire bytes, the fixed channel-descriptor header, and the renormalization
/// payload. `None` when the `rans` feature is disabled.
#[cfg(feature = "rans")]
fn rans_plaintext_bytes(plaintext: &[u8]) -> Option<u64> {
    use crate::entropy::codec::WIRE_HEADER_LEN;
    use crate::entropy::{ALPHABET, EntropyModel, encode_channel};

    let mut counts = [0u64; ALPHABET];
    for &b in plaintext {
        counts[b as usize] += 1;
    }
    let model = EntropyModel::from_counts(&counts, RANS_SCALE_BITS).ok()?;
    let model_bytes = model.encode().ok()?.len() as u64;
    let capsule = encode_channel(&model, plaintext).ok()?;
    Some(model_bytes + WIRE_HEADER_LEN as u64 + capsule.payload.len() as u64)
}

#[cfg(not(feature = "rans"))]
fn rans_plaintext_bytes(_plaintext: &[u8]) -> Option<u64> {
    None
}
