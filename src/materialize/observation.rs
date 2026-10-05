//! Partial materialization / observation views (Phase 7.3).
//!
//! [`materialize_observation`] serves a narrow byte range of the reconstructed
//! source from a parsed descriptor that carries an observation index. It is a
//! *view* API: the complete-materialization path
//! ([`crate::materialize::materialize`]) is unchanged and remains the authority
//! for whole-source exactness.
//!
//! ## Decline, never guess
//!
//! A descriptor without an index is declined with
//! `ErrorClass::UnsupportedFeature`; the function never silently falls back to
//! full materialization, so a reported cost is never a whole-file cost in
//! disguise.
//!
//! ## Op selection: linear skipping vs. a bounded prefix
//!
//! The op layout is re-derived from the program with
//! [`Program::analyze_ops`] (the index's op table is advisory; the program is
//! authoritative). Let the requested output range be `[a, b)`.
//!
//! * If every op is a *linear, independent block* producer — one of
//!   [`Op::EmitObject`], [`Op::Inline`], [`Op::DecodeChannel`],
//!   [`Op::DeflateReplay`], or [`Op::InterleaveChannels`] — then each op's output
//!   is a pure function of its own inputs and its absolute start offset. Only the
//!   ops whose output intersects `[a, b)` are evaluated; ops entirely before `a`
//!   are skipped (no bytes produced, no channel decoded).
//! * Otherwise (the program uses any position-dependent op: [`Op::RepeatLast`],
//!   [`Op::MarkOffset`], [`Op::EmitOffset`], [`Op::PackSegments`], or
//!   [`Op::PackedChannels`]) the ops `0..=last` are evaluated as a prefix. This
//!   is correct but can produce up to the whole document; it is the honest
//!   fallback. [`ObservationStats::work_amplification`] reports how much of the
//!   program was walked.
//!
//! ## Lazy entropy-channel decoding
//!
//! Only the entropy channels referenced by the evaluated ops are decoded; the
//! complete path in [`crate::materialize`] still decodes every channel. A
//! referenced channel is decoded in full, so the conversion-contract tail check
//! is preserved for exactly the channels that are used. Objects are already
//! resident in the descriptor, so "fetched" here means "read by an evaluated op".
//!
//! ## `descriptor_bytes_traversed` is an approximation
//!
//! v1 parses the whole framed descriptor into memory, so there is no true I/O
//! seek accounting yet. [`ObservationStats::descriptor_bytes_traversed`] reports
//! the sum of the serialized record payload lengths the path *needed* — the graph
//! record, the referenced object payloads, the referenced channel payloads, and
//! the index record — as a documented CPU-side approximation, not a byte-read
//! figure. The referenced channel payloads are already included here, so
//! [`ObservationStats::entropy_bytes_decoded`] is a **subset** of
//! `descriptor_bytes_traversed` and the two fields must never be summed. A later
//! stage may add an mmap/seek reader to make it a real I/O
//! number; the field name and this caveat are deliberate.

use crate::container::observation::{
    ObservationIndex, ObservationSelector as IndexSelector, SECTION_PDF_SELECTORS, SELECTOR_OBJECT,
    SELECTOR_REVISION, SELECTOR_STREAM,
};
use crate::container::{Descriptor, ParsedDescriptor};
use crate::dra::{Op, Program};
use crate::error::{Error, Result};
use crate::limits::Limits;

#[cfg(feature = "rans")]
use crate::entropy::rans::{Capsule, decode_channel};

/// A narrow observation of the reconstructed source.
///
/// Every selector resolves to a half-open output range `[a, b)` in the source
/// address space (output offset `O` is source offset `O`, by the exact profile).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationSelector {
    /// Raw output byte range `[offset, offset + len)`.
    ByteRange {
        /// First output byte.
        offset: u64,
        /// Number of bytes; must be non-empty.
        len: u64,
    },
    /// The full physical byte extent of indirect object `object generation`
    /// (the `N G obj` introducer through `endobj`).
    PdfIndirectObject {
        /// PDF object number.
        object: u32,
        /// PDF generation number.
        generation: u16,
    },
    /// The physical encoded stream data span of the stream in indirect object
    /// `object generation`: the compressed bytes exactly as they appear in the
    /// source.
    PdfEncodedStream {
        /// PDF object number of the enclosing indirect object.
        object: u32,
        /// PDF generation number of the enclosing indirect object.
        generation: u16,
    },
    /// Every source byte belonging to incremental-update revision `index`.
    PdfRevision {
        /// Zero-based revision index.
        index: u32,
    },
}

/// Cost attribution for one observation.
///
/// All fields are measured from the evaluated path, not estimated.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObservationStats {
    /// Number of ops the observation actually evaluated.
    pub ops_evaluated: usize,
    /// Total ops in the descriptor's reconstruction program.
    pub ops_total: usize,
    /// Number of distinct raw objects read by evaluated ops.
    pub objects_fetched: usize,
    /// Total raw objects in the descriptor.
    pub objects_total: usize,
    /// Number of entropy channels decoded for this observation.
    pub channels_decoded: usize,
    /// Total entropy channels in the descriptor.
    pub channels_total: usize,
    /// Sum of the encoded renormalization payload bytes of the decoded channels.
    ///
    /// This is the descriptor-side entropy bytes consumed, not the decoded symbol
    /// count (which is the referenced channels' `decoded_length`). It is a
    /// **subset** of [`Self::descriptor_bytes_traversed`], which already counts
    /// the same referenced channel payloads, so it is a breakdown and must
    /// **never** be added to that field — the sum double-counts.
    pub entropy_bytes_decoded: u64,
    /// Approximate descriptor bytes the path needed (see the module docs).
    ///
    /// The honest decode-side bound: the graph record, the index record, the
    /// referenced object payloads, and the referenced channel payloads (which
    /// include [`Self::entropy_bytes_decoded`]).
    pub descriptor_bytes_traversed: u64,
    /// Number of output bytes served.
    pub output_bytes: u64,
}

impl ObservationStats {
    /// Fraction of the program evaluated: `ops_evaluated / ops_total`.
    ///
    /// Returns `0.0` when the program has no ops, so the ratio is always finite.
    pub fn work_amplification(&self) -> f64 {
        if self.ops_total == 0 {
            0.0
        } else {
            self.ops_evaluated as f64 / self.ops_total as f64
        }
    }
}

/// A served observation: the exact requested bytes plus its cost attribution.
#[derive(Debug, Clone, PartialEq)]
pub struct ObservationReport {
    /// The resolved half-open output range `[a, b)` that was served. Reporting it
    /// lets a measurement harness compare the slice against the source and price
    /// the sequential baselines at the same output offset.
    pub range: (u64, u64),
    /// The exact requested output range.
    pub bytes: Vec<u8>,
    /// Measured cost attribution for serving that range.
    pub stats: ObservationStats,
}

/// Serve one observation from a parsed descriptor carrying an observation index.
///
/// The returned bytes equal `materialize(parsed)[a..b]` for the resolved range;
/// a descriptor without an index is declined (never silently fully
/// materialized). See the module documentation for the op-selection and lazy
/// decoding rules, and for the `descriptor_bytes_traversed` caveat.
pub fn materialize_observation(
    parsed: &ParsedDescriptor,
    selector: ObservationSelector,
    limits: Limits,
) -> Result<ObservationReport> {
    let d = &parsed.descriptor;
    let index = d
        .observation_index
        .as_ref()
        .ok_or_else(|| Error::unsupported_feature("descriptor has no observation index"))?;

    // 2. Resolve the selector to an output range `[a, b)`.
    let (a, b) = resolve_selector(index, selector, d.source_len)?;

    // 3. Per-op lengths and cumulative offsets. The program is authoritative; the
    //    index's op table was already cross-checked against it at parse time.
    let object_lens: Vec<u64> = d.objects.iter().map(|o| o.len() as u64).collect();
    let channel_lens: Vec<u64> = d.channels.iter().map(|c| c.decoded_length).collect();
    let per_op = d.program.analyze_ops(&object_lens, &channel_lens, limits)?;

    let mut starts: Vec<u64> = Vec::with_capacity(per_op.len());
    let mut ends: Vec<u64> = Vec::with_capacity(per_op.len());
    let mut acc: u64 = 0;
    for &len in &per_op {
        starts.push(acc);
        acc = acc
            .checked_add(len)
            .ok_or_else(|| Error::invalid_graph("observation op length overflow"))?;
        ends.push(acc);
    }

    let intersects = |i: usize| per_op[i] > 0 && starts[i] < b && ends[i] > a;
    let first = (0..per_op.len())
        .find(|&i| intersects(i))
        .ok_or_else(|| Error::invalid_graph("observation range is not covered by the program"))?;
    let last = (0..per_op.len())
        .rev()
        .find(|&i| intersects(i))
        .ok_or_else(|| Error::invalid_graph("observation range is not covered by the program"))?;

    // 4. Linear independent-block ops can be skipped; anything else needs a
    //    prefix. `buf_start` is the absolute output offset the evaluated buffer
    //    begins at.
    let linear = is_linear_independent(&d.program);
    let (selected_ops, buf_start): (Vec<Op>, u64) = if linear {
        let ops = d
            .program
            .ops
            .iter()
            .enumerate()
            .filter(|&(i, _)| intersects(i))
            .map(|(_, op)| op.clone())
            .collect();
        (ops, starts[first])
    } else {
        (d.program.ops[..=last].to_vec(), 0)
    };

    // 5. Mark the objects and channels the evaluated ops reference, then decode
    //    only those channels.
    let mut objects_used = vec![false; d.objects.len()];
    let mut channels_used = vec![false; d.channels.len()];
    for op in &selected_ops {
        mark_references(op, &mut objects_used, &mut channels_used);
    }
    let channels = decode_referenced_channels(d, &channels_used, limits)?;

    // 6. Evaluate the selected ops and slice the requested window. The op list is
    //    moved into the sub-program, so skipped inline bytes are never copied.
    let sub = Program::new(selected_ops);
    let out = sub.eval(&d.objects, &channels, limits)?;

    let lo = a
        .checked_sub(buf_start)
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(|| Error::internal_invariant("observation window precedes evaluated buffer"))?;
    let hi = b
        .checked_sub(buf_start)
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(|| Error::internal_invariant("observation window overflow"))?;
    if hi > out.len() {
        return Err(Error::internal_invariant(
            "evaluated buffer is shorter than the requested observation window",
        ));
    }
    let bytes = out[lo..hi].to_vec();

    // 7. Cost attribution.
    let mut entropy_bytes_decoded: u64 = 0;
    let mut referenced_channel_bytes: u64 = 0;
    let mut channels_decoded: usize = 0;
    for (id, used) in channels_used.iter().enumerate() {
        if *used {
            channels_decoded += 1;
            let payload_len = d.channels[id].payload.len() as u64;
            entropy_bytes_decoded += payload_len;
            referenced_channel_bytes += payload_len;
        }
    }
    let mut objects_fetched: usize = 0;
    let mut referenced_object_bytes: u64 = 0;
    for (id, used) in objects_used.iter().enumerate() {
        if *used {
            objects_fetched += 1;
            referenced_object_bytes += d.objects[id].len() as u64;
        }
    }

    let descriptor_bytes_traversed =
        parsed.cost.graph + parsed.cost.index + referenced_object_bytes + referenced_channel_bytes;

    let stats = ObservationStats {
        ops_evaluated: sub.ops.len(),
        ops_total: d.program.ops.len(),
        objects_fetched,
        objects_total: d.objects.len(),
        channels_decoded,
        channels_total: d.channels.len(),
        entropy_bytes_decoded,
        descriptor_bytes_traversed,
        output_bytes: bytes.len() as u64,
    };

    Ok(ObservationReport {
        range: (a, b),
        bytes,
        stats,
    })
}

/// Resolve a selector to a target output range `[a, b)`.
fn resolve_selector(
    index: &ObservationIndex,
    selector: ObservationSelector,
    source_len: u64,
) -> Result<(u64, u64)> {
    match selector {
        ObservationSelector::ByteRange { offset, len } => {
            if len == 0 {
                return Err(Error::usage("observation byte range must be non-empty"));
            }
            let end = offset
                .checked_add(len)
                .ok_or_else(|| Error::usage("observation byte range overflows"))?;
            if end > source_len {
                return Err(Error::usage(format!(
                    "observation byte range {offset}..{end} exceeds source length {source_len}"
                )));
            }
            Ok((offset, end))
        }
        ObservationSelector::PdfIndirectObject { object, generation } => resolve_pdf(
            index,
            SELECTOR_OBJECT,
            object,
            u32::from(generation),
            "indirect object",
        ),
        ObservationSelector::PdfEncodedStream { object, generation } => resolve_pdf(
            index,
            SELECTOR_STREAM,
            object,
            u32::from(generation),
            "encoded stream",
        ),
        ObservationSelector::PdfRevision { index: rev } => {
            if index.section_flags & SECTION_PDF_SELECTORS == 0 {
                return Err(Error::unsupported_feature(
                    "observation index has no PDF selector table",
                ));
            }
            let hits: Vec<&IndexSelector> = index
                .selectors
                .iter()
                .filter(|s| s.kind == SELECTOR_REVISION && s.number == rev)
                .collect();
            match hits.as_slice() {
                [one] => selector_range(one, "PDF revision", u64::from(rev)),
                [] => Err(Error::unsupported_feature(format!(
                    "observation index has no selector for PDF revision {rev}"
                ))),
                _ => Err(Error::invalid_container(format!(
                    "observation index has ambiguous selectors for PDF revision {rev}"
                ))),
            }
        }
    }
}

/// Look up a `(kind, number, generation)` PDF selector; absent or ambiguous is a
/// typed error (never a guess).
fn resolve_pdf(
    index: &ObservationIndex,
    kind: u8,
    number: u32,
    generation: u32,
    label: &str,
) -> Result<(u64, u64)> {
    if index.section_flags & SECTION_PDF_SELECTORS == 0 {
        return Err(Error::unsupported_feature(
            "observation index has no PDF selector table",
        ));
    }
    let hits: Vec<&IndexSelector> = index
        .selectors
        .iter()
        .filter(|s| s.kind == kind && s.number == number && s.generation == generation)
        .collect();
    match hits.as_slice() {
        [one] => selector_range(one, label, u64::from(number)),
        [] => Err(Error::unsupported_feature(format!(
            "observation index has no selector for {label} {number} {generation}"
        ))),
        _ => Err(Error::invalid_container(format!(
            "observation index has ambiguous selectors for {label} {number} {generation}"
        ))),
    }
}

/// Convert a validated index selector into a checked `[a, b)` range.
fn selector_range(selector: &IndexSelector, label: &str, id: u64) -> Result<(u64, u64)> {
    let end = selector
        .out_off
        .checked_add(selector.out_len)
        .ok_or_else(|| {
            Error::invalid_container(format!("{label} {id} selector range overflows"))
        })?;
    Ok((selector.out_off, end))
}

/// True when every op is a linear, independently evaluable block producer.
///
/// Position-dependent ops (`REPEAT_LAST`, `MARK_OFFSET`, `EMIT_OFFSET`,
/// `PACK_SEGMENTS`, `PACKED_CHANNELS`) disqualify skipping because their output
/// depends on the running output position or on a previous block's bytes.
fn is_linear_independent(program: &Program) -> bool {
    program.ops.iter().all(|op| {
        matches!(
            op,
            Op::EmitObject { .. }
                | Op::Inline { .. }
                | Op::DecodeChannel { .. }
                | Op::DeflateReplay { .. }
                | Op::InterleaveChannels { .. }
        )
    })
}

/// Record the objects and channels an op reads from.
fn mark_references(op: &Op, objects: &mut [bool], channels: &mut [bool]) {
    match op {
        Op::EmitObject { object_id } => mark(objects, *object_id),
        Op::Inline { .. }
        | Op::MarkOffset { .. }
        | Op::EmitOffset { .. }
        | Op::RepeatLast { .. } => {}
        Op::DecodeChannel { channel_id } => mark(channels, *channel_id),
        Op::InterleaveChannels {
            kinds_channel,
            lengths_channel,
            first_payload_channel,
            payload_channel_count,
        } => {
            mark(channels, *kinds_channel);
            mark(channels, *lengths_channel);
            for k in 0..u32::from(*payload_channel_count) {
                if let Some(id) = first_payload_channel.checked_add(k) {
                    mark(channels, id);
                }
            }
        }
        Op::PackSegments { data_object, .. } => mark(objects, *data_object),
        Op::PackedChannels {
            data_channel,
            plan_channel,
            ..
        } => {
            mark(channels, *data_channel);
            mark(channels, *plan_channel);
        }
        Op::DeflateReplay {
            source_kind,
            source_id,
            corrections_object,
            ..
        } => {
            match *source_kind {
                crate::dra::op::DEFLATE_SOURCE_OBJECT => mark(objects, *source_id),
                crate::dra::op::DEFLATE_SOURCE_CHANNEL => mark(channels, *source_id),
                _ => {}
            }
            mark(objects, *corrections_object);
        }
    }
}

/// Mark `id` in `flags` when it is in range; an out-of-range id is left to the
/// evaluator's own validation.
fn mark(flags: &mut [bool], id: u32) {
    if let Some(slot) = flags.get_mut(id as usize) {
        *slot = true;
    }
}

/// Decode only the entropy channels referenced by the evaluated ops.
#[cfg(feature = "rans")]
fn decode_referenced_channels(
    d: &Descriptor,
    channels_used: &[bool],
    limits: Limits,
) -> Result<Vec<Vec<u8>>> {
    let mut channels: Vec<Vec<u8>> = vec![Vec::new(); d.channels.len()];
    for (id, used) in channels_used.iter().enumerate() {
        if *used {
            channels[id] = decode_channel_by_id(d, id, limits)?;
        }
    }
    Ok(channels)
}

/// Without the `rans` feature a referenced channel cannot be decoded; decline
/// explicitly rather than silently producing wrong bytes.
#[cfg(not(feature = "rans"))]
fn decode_referenced_channels(
    d: &Descriptor,
    channels_used: &[bool],
    _limits: Limits,
) -> Result<Vec<Vec<u8>>> {
    if channels_used.iter().any(|&used| used) {
        return Err(Error::unsupported_feature(
            "this build was compiled without the `rans` feature",
        ));
    }
    Ok(vec![Vec::new(); d.channels.len()])
}

/// Decode one referenced channel against the model it names.
#[cfg(feature = "rans")]
fn decode_channel_by_id(d: &Descriptor, id: usize, limits: Limits) -> Result<Vec<u8>> {
    let channel = d.channels.get(id).ok_or_else(|| {
        Error::invalid_model(format!(
            "observation references missing entropy channel {id}"
        ))
    })?;
    let model = d.models.get(channel.model_id as usize).ok_or_else(|| {
        Error::invalid_model(format!(
            "entropy channel {id} references missing model {}",
            channel.model_id
        ))
    })?;
    let capsule = Capsule {
        initial_state: channel.initial_state,
        payload: channel.payload.clone(),
        symbol_count: channel.symbol_count,
        decoded_length: channel.decoded_length,
    };
    decode_channel(model, &capsule, limits)
}
