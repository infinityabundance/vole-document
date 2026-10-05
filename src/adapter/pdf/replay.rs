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

use std::collections::HashMap;

use crate::SOURCE_FORMAT_PDF;
use crate::container::{Descriptor, UNIVERSE};
use crate::dra::op::{DEFLATE_SOURCE_CHANNEL, DEFLATE_SOURCE_OBJECT};
use crate::dra::{Op, Program};
use crate::encode::candidates::{Candidate, CandidateKind};
use crate::error::Result;
use crate::integrity::sha256;
use crate::limits::Limits;

use crate::codec::deflate::{ReplayPlan, try_replay};

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
        objects,
        program: Program::new(ops),
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
        objects,
        program: Program::new(ops),
        source_sha256: sha256(input),
        source_len: input.len() as u64,
    };

    Ok(Some(Candidate {
        kind: CandidateKind::PdfDeflateReplayRans,
        descriptor,
    }))
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
