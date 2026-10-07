//! PDF layout candidate: regenerate classic cross-reference entry offsets and
//! the `startxref` value from positions marked during materialization.
//!
//! This is the first Phase-5 candidate that replaces literal structural bytes
//! with *procedurally determined* ones. The mechanism is deliberately narrow and
//! conservative:
//!
//! * It applies only to files that already carry a classic cross-reference
//!   section ([`PhysicalKind::XrefSection`]) and contain no
//!   [`ObjRole::XRefStream`] object. Anything else declines (`Ok(None)`).
//! * Every literal byte is accumulated into a single data object and the whole
//!   reconstruction is one compact [`Op::PackSegments`] item table, so the
//!   per-segment framing is paid once rather than once per span or xref entry.
//! * It records each indirect object's introducer offset with a
//!   [`PackItem::Mark`] (slot = the object's index in [`PdfPhysical::objects`])
//!   and, for every `n`-status xref entry whose 10-digit offset field equals the
//!   marked position of its target object, emits that field with a
//!   [`PackItem::Emit`] rather than storing the digits literally.
//! * The most recent `xref` section start is marked in the reserved slot
//!   [`XREF_SLOT`] (`255`); each `startxref` value is regenerated from it only
//!   when the emitted position equals the source value.
//! * Whenever a precondition fails — a non-standard offset field, a mismatched
//!   position, a malformed table, too many objects — the site (or the whole
//!   section) falls back to a literal [`PackItem::Literal`]. Prediction never
//!   invents bytes: a fallback is always byte-exact, and a prediction is only
//!   emitted when it reproduces the source digits exactly.
//!
//! After building the program the candidate is verified end-to-end (serialize,
//! parse, materialize, byte-compare) before it is returned; if that round trip
//! is not exact, the proposal declines rather than emitting an inexact
//! candidate.

use crate::SOURCE_FORMAT_PDF;
use crate::container::{Descriptor, ObjectSource, UNIVERSE};
use crate::dra::op::PackItem;
use crate::dra::{Op, Program};
use crate::encode::candidates::{Candidate, CandidateKind};
use crate::error::Result;
use crate::integrity::sha256;
use crate::limits::Limits;

#[cfg(feature = "rans")]
use crate::entropy::{
    ALPHABET, CODER_ORDER0_BYTE_RANS, CODER_VERSION_1, EntropyChannelDescriptor, EntropyModel,
    encode_channel,
};

use super::physical::{ObjRole, PdfObjectSpan, PhysicalKind, scan};

/// Reserved slot index for the most recent classic `xref` section start.
pub const XREF_SLOT: u8 = u8::MAX;

/// Largest number of indirect objects that can be marked (indices `0..=254`),
/// leaving slot `255` free for [`XREF_SLOT`].
pub const MAX_MARKED_OBJECTS: usize = XREF_SLOT as usize;

/// The structural layout plan for a classic-cross-reference PDF: the ordered
/// packed item table, the single literal data object it consumes, and the
/// prediction counters used to describe the plan.
pub struct LayoutPlan {
    /// Ordered reconstruction items (literal runs, position marks, emitted
    /// offsets), as consumed by [`Op::PackSegments`] and [`Op::PackedChannels`].
    pub items: Vec<PackItem>,
    /// Every literal byte, in item order. Must be consumed exactly by `items`.
    pub data: Vec<u8>,
    /// Number of xref entry offsets regenerated from a marked position.
    pub xref_predicted: usize,
    /// Number of xref entry offsets stored literally (precondition failed).
    pub xref_literal: usize,
    /// Whether at least one `startxref` value was regenerated.
    pub startxref_predicted: bool,
}

/// Build the structural layout plan for `input`, or `None` when the input is not
/// a classic-cross-reference PDF this mechanism can express exactly.
///
/// Declines (`Ok(None)`) whenever a precondition fails — no classic `xref`
/// section, a cross-reference stream present, too many objects, a non-contiguous
/// span cover, or a literal run that cannot fit a `u32`. See the module
/// documentation for the algorithm. The caller is responsible for the final
/// byte-exactness check through the normative decoder.
pub fn build_layout_plan(input: &[u8], limits: Limits) -> Result<Option<LayoutPlan>> {
    let physical = match scan(input, limits) {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };

    // Precondition: a classic cross-reference section must exist, and no
    // cross-reference stream may be present.
    if !physical
        .spans
        .iter()
        .any(|s| s.kind == PhysicalKind::XrefSection)
    {
        return Ok(None);
    }
    if physical
        .objects
        .iter()
        .any(|o| o.role == ObjRole::XRefStream)
    {
        return Ok(None);
    }
    if physical.objects.len() > MAX_MARKED_OBJECTS {
        return Ok(None);
    }

    // The data object carries every literal byte; the compact item table
    // interleaves literal runs, position marks, and regenerated offsets so the
    // per-segment framing is paid once rather than once per span/entry.
    let mut data: Vec<u8> = Vec::new();
    let mut items: Vec<PackItem> = Vec::new();
    // Simulated output position. The physical cover is contiguous, so this
    // tracks the source offset of the next byte exactly: literals add their
    // length, emits add their width, and marks add nothing.
    let mut pos: u64 = 0;
    let mut slot_value = [0u64; 256];
    let mut slot_marked = [false; 256];
    let mut xref_predicted: usize = 0;
    let mut xref_literal: usize = 0;
    let mut startxref_predicted = false;

    for span in &physical.spans {
        let start = span.start as usize;
        let end = start + span.len as usize;
        let bytes = &input[start..end];

        if pos != span.start {
            // A non-contiguous simulation would break the offset contract; bail.
            return Ok(None);
        }

        match span.kind {
            PhysicalKind::ObjHeader => {
                if let Some(idx) = object_index_at(&physical.objects, span.start) {
                    let slot = idx as u8;
                    items.push(PackItem::Mark { slot });
                    slot_value[slot as usize] = pos;
                    slot_marked[slot as usize] = true;
                }
                if !push_pack_literal(&mut data, &mut items, bytes, &mut pos) {
                    return Ok(None);
                }
            }
            PhysicalKind::XrefSection => {
                // Mark this section's start in the reserved slot, then emit.
                items.push(PackItem::Mark { slot: XREF_SLOT });
                slot_value[XREF_SLOT as usize] = pos;
                slot_marked[XREF_SLOT as usize] = true;

                match parse_classic_xref(bytes) {
                    Some(pieces) => {
                        for piece in pieces {
                            match piece {
                                XrefPiece::Literal { start, len } => {
                                    if !push_pack_literal(
                                        &mut data,
                                        &mut items,
                                        &bytes[start..start + len],
                                        &mut pos,
                                    ) {
                                        return Ok(None);
                                    }
                                }
                                XrefPiece::Entry {
                                    start,
                                    number,
                                    offset,
                                    in_use,
                                } => {
                                    let slot = if in_use {
                                        offset.and_then(|value| {
                                            predicted_slot(
                                                &physical.objects,
                                                &slot_value,
                                                &slot_marked,
                                                number,
                                                value,
                                            )
                                        })
                                    } else {
                                        None
                                    };
                                    match slot {
                                        Some(slot) => {
                                            items.push(PackItem::Emit { slot, width: 10 });
                                            pos += 10;
                                            if !push_pack_literal(
                                                &mut data,
                                                &mut items,
                                                &bytes[start + 10..start + 20],
                                                &mut pos,
                                            ) {
                                                return Ok(None);
                                            }
                                            xref_predicted += 1;
                                        }
                                        None => {
                                            if !push_pack_literal(
                                                &mut data,
                                                &mut items,
                                                &bytes[start..start + 20],
                                                &mut pos,
                                            ) {
                                                return Ok(None);
                                            }
                                            xref_literal += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    None => {
                        // Not a classic table we understand: literal whole section.
                        if !push_pack_literal(&mut data, &mut items, bytes, &mut pos) {
                            return Ok(None);
                        }
                    }
                }
            }
            PhysicalKind::StartXref => match predict_startxref(bytes, &slot_value, &slot_marked) {
                Some((prefix_len, width)) => {
                    if !push_pack_literal(&mut data, &mut items, &bytes[..prefix_len], &mut pos) {
                        return Ok(None);
                    }
                    items.push(PackItem::Emit {
                        slot: XREF_SLOT,
                        width,
                    });
                    pos += width as u64;
                    startxref_predicted = true;
                }
                None => {
                    if !push_pack_literal(&mut data, &mut items, bytes, &mut pos) {
                        return Ok(None);
                    }
                }
            },
            _ => {
                if !push_pack_literal(&mut data, &mut items, bytes, &mut pos) {
                    return Ok(None);
                }
            }
        }
    }

    Ok(Some(LayoutPlan {
        items,
        data,
        xref_predicted,
        xref_literal,
        startxref_predicted,
    }))
}

/// Propose a layout candidate that regenerates xref offsets / startxref, or
/// `None`.
///
/// Wraps [`build_layout_plan`] into a single [`Op::PackSegments`] program over
/// one literal data object. Declines (`Ok(None)`) whenever the plan cannot be
/// built or the assembled program does not materialize byte-for-byte. See the
/// module documentation for the algorithm.
pub fn propose_pdf_layout(input: &[u8], limits: Limits) -> Result<Option<Candidate>> {
    let plan = match build_layout_plan(input, limits)? {
        Some(p) => p,
        None => return Ok(None),
    };

    // `startxref` is predicted once per `StartXref` span; every `Emit` is either
    // a predicted xref entry or a predicted startxref, so the count is exact.
    let startxref_predicted = emit_count(&plan.items).saturating_sub(plan.xref_predicted);
    let format_basis = format!(
        "pdf-layout;objects={};xref_predicted={};xref_literal={};startxref_predicted={}",
        marked_object_count(&plan.items),
        plan.xref_predicted,
        plan.xref_literal,
        startxref_predicted
    );

    let descriptor = Descriptor {
        universe: UNIVERSE.to_string(),
        source_format: SOURCE_FORMAT_PDF,
        format_basis,
        models: vec![],
        channels: vec![],
        objects: vec![ObjectSource::Inline(plan.data)],
        program: Program::new(vec![Op::PackSegments {
            data_object: 0,
            items: plan.items,
        }]),
        observation_index: None,
        seek_directory: false,
        checkpoints: None,
        source_sha256: sha256(input),
        source_len: input.len() as u64,
    };

    let candidate = Candidate {
        kind: CandidateKind::PdfLayout,
        descriptor,
    };

    // Verify byte-exactness through the normative decoder before returning. An
    // inexact program must never be emitted.
    let (encoded, _) = candidate.descriptor.serialize()?;
    let parsed = match Descriptor::parse(&encoded, limits) {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };
    let out = match crate::materialize::materialize(&parsed, limits) {
        Ok(o) => o,
        Err(_) => return Ok(None),
    };
    if out != input {
        return Ok(None);
    }

    Ok(Some(candidate))
}

/// Propose a layout + rANS candidate, or `None`.
///
/// Builds the same [`LayoutPlan`] as [`propose_pdf_layout`], then entropy-codes
/// its parts into two rANS channels: channel `0` carries the plan's literal data
/// object, and channel `1` carries `encode_items` of the plan's item table. A
/// single [`Op::PackedChannels`] reconstructs the source from both, so the whole
/// plan — data *and* item table — pays entropy-coding cost instead of being
/// stored as literal bytes. Each channel uses its own order-0 byte model
/// normalized from its own byte histogram at `scale_bits` 12.
///
/// Exactly as with the literal layout lane, an end-to-end serialize / parse /
/// materialize / byte-compare check gates the return: an inexact program yields
/// `Ok(None)` rather than an inexact candidate.
#[cfg(feature = "rans")]
pub fn propose_pdf_layout_rans(input: &[u8], limits: Limits) -> Result<Option<Candidate>> {
    let plan = match build_layout_plan(input, limits)? {
        Some(p) => p,
        None => return Ok(None),
    };

    let plan_bytes = crate::dra::op::encode_items(&plan.items)?;

    // Channel 0: the literal data object, coded against its own byte histogram.
    let mut data_counts = [0u64; ALPHABET];
    for &b in &plan.data {
        data_counts[b as usize] += 1;
    }
    let data_model = EntropyModel::from_counts(&data_counts, 12)?;
    let data_capsule = encode_channel(&data_model, &plan.data)?;

    // Channel 1: the serialized item table, coded against its own histogram.
    let mut plan_counts = [0u64; ALPHABET];
    for &b in &plan_bytes {
        plan_counts[b as usize] += 1;
    }
    let plan_model = EntropyModel::from_counts(&plan_counts, 12)?;
    let plan_capsule = encode_channel(&plan_model, &plan_bytes)?;

    let data_channel = EntropyChannelDescriptor {
        coder: CODER_ORDER0_BYTE_RANS,
        coder_version: CODER_VERSION_1,
        scale_bits: data_model.scale_bits,
        lane_count: 1,
        model_id: 0,
        symbol_count: data_capsule.symbol_count,
        decoded_length: data_capsule.decoded_length,
        initial_state: data_capsule.initial_state,
        payload: data_capsule.payload,
    };
    let plan_channel = EntropyChannelDescriptor {
        coder: CODER_ORDER0_BYTE_RANS,
        coder_version: CODER_VERSION_1,
        scale_bits: plan_model.scale_bits,
        lane_count: 1,
        model_id: 1,
        symbol_count: plan_capsule.symbol_count,
        decoded_length: plan_capsule.decoded_length,
        initial_state: plan_capsule.initial_state,
        payload: plan_capsule.payload,
    };

    let format_basis = format!(
        "pdf-layout-rans;objects={};xref_predicted={};xref_literal={}",
        marked_object_count(&plan.items),
        plan.xref_predicted,
        plan.xref_literal
    );

    let descriptor = Descriptor {
        universe: UNIVERSE.to_string(),
        source_format: SOURCE_FORMAT_PDF,
        format_basis,
        models: vec![data_model, plan_model],
        channels: vec![data_channel, plan_channel],
        objects: vec![],
        program: Program::new(vec![Op::PackedChannels {
            data_channel: 0,
            plan_channel: 1,
            declared_output_len: input.len() as u64,
        }]),
        observation_index: None,
        seek_directory: false,
        checkpoints: None,
        source_sha256: sha256(input),
        source_len: input.len() as u64,
    };

    let candidate = Candidate {
        kind: CandidateKind::PdfLayoutRans,
        descriptor,
    };

    // Verify byte-exactness through the normative decoder before returning. An
    // inexact program must never be emitted.
    let (encoded, _) = candidate.descriptor.serialize()?;
    let parsed = match Descriptor::parse(&encoded, limits) {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };
    let out = match crate::materialize::materialize(&parsed, limits) {
        Ok(o) => o,
        Err(_) => return Ok(None),
    };
    if out != input {
        return Ok(None);
    }

    Ok(Some(candidate))
}

/// Number of indirect objects whose introducer was marked in the item table.
fn marked_object_count(items: &[PackItem]) -> usize {
    items
        .iter()
        .filter(|item| matches!(item, PackItem::Mark { slot } if *slot != XREF_SLOT))
        .count()
}

/// Number of emitted offsets in the item table.
fn emit_count(items: &[PackItem]) -> usize {
    items
        .iter()
        .filter(|item| matches!(item, PackItem::Emit { .. }))
        .count()
}

/// Append `bytes` to the packed data object, recording one [`PackItem::Literal`]
/// when non-empty, and advance the simulated output position. Returns `false`
/// (so the caller declines) when a single run would not fit a `u32` length.
fn push_pack_literal(
    data: &mut Vec<u8>,
    items: &mut Vec<PackItem>,
    bytes: &[u8],
    pos: &mut u64,
) -> bool {
    if !push_literal(items, data, bytes) {
        return false;
    }
    *pos += bytes.len() as u64;
    true
}

/// Append `bytes` to the packed data object, coalescing them into the immediately
/// preceding [`PackItem::Literal`] when one is present so that consecutive literal
/// runs collapse into the fewest possible items. The merge never crosses a
/// [`PackItem::Mark`] or [`PackItem::Emit`], empty pushes are ignored, and the
/// combined length must remain expressible as a `u32`. Returns `false` (so the
/// caller declines) when no safe item shape exists.
pub(crate) fn push_literal(items: &mut Vec<PackItem>, data: &mut Vec<u8>, bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return true;
    }
    let Ok(len) = u32::try_from(bytes.len()) else {
        return false;
    };
    if let Some(PackItem::Literal { len: prev }) = items.last_mut() {
        let Some(total) = prev.checked_add(len) else {
            return false;
        };
        *prev = total;
    } else {
        items.push(PackItem::Literal { len });
    }
    data.extend_from_slice(bytes);
    true
}

/// Index of the first object whose introducer starts at `start`.
fn object_index_at(objects: &[PdfObjectSpan], start: u64) -> Option<usize> {
    objects.iter().position(|o| o.start == start)
}

/// The slot marking the target object `number` at position `value`, if any
/// earlier [`Op::MarkOffset`] recorded exactly that position.
fn predicted_slot(
    objects: &[PdfObjectSpan],
    slot_value: &[u64; 256],
    slot_marked: &[bool; 256],
    number: u64,
    value: u64,
) -> Option<u8> {
    objects.iter().enumerate().find_map(|(i, o)| {
        let slot = i as u8;
        (o.number == number && slot_marked[slot as usize] && slot_value[slot as usize] == value)
            .then_some(slot)
    })
}

/// Predict a whole `startxref` span: the trailing run of decimal digits is the
/// value. Returns `(prefix_len, width)` when the value equals the marked `xref`
/// position and its width is in `1..=20`.
fn predict_startxref(
    bytes: &[u8],
    slot_value: &[u64; 256],
    slot_marked: &[bool; 256],
) -> Option<(usize, u8)> {
    if !slot_marked[XREF_SLOT as usize] {
        return None;
    }
    let mut i = bytes.len();
    while i > 0 && bytes[i - 1].is_ascii_digit() {
        i -= 1;
    }
    let width = bytes.len() - i;
    if width == 0 || width > 20 {
        return None;
    }
    let value = parse_digits(&bytes[i..])?;
    if value != slot_value[XREF_SLOT as usize] {
        return None;
    }
    Some((i, width as u8))
}

/// One ordered slice of a classic `xref` section: either literal bytes or a
/// 20-byte entry whose offset field may be regenerated.
pub(crate) enum XrefPiece {
    /// Verbatim bytes `[start, start + len)` of the section.
    Literal { start: usize, len: usize },
    /// A 20-byte entry `[start, start + 20)`.
    Entry {
        /// Offset of the entry within the section.
        start: usize,
        /// Target object number (`subsection_start + i`).
        number: u64,
        /// Parsed value of the 10-digit offset field, if it is all digits.
        offset: Option<u64>,
        /// Whether the status byte is `n` (in use).
        in_use: bool,
    },
}

/// Parse a classic cross-reference table from a section's bytes, returning an
/// ordered tiling of the section. Returns `None` (so the caller emits the whole
/// section literally) when the bytes do not match the classic grammar.
///
/// Grammar accepted: `xref` EOL, then one or more `<start> <count>` EOL headers
/// each followed by exactly `count` 20-byte entries of the shape
/// `10-digit-offset SP 5-digit-generation SP status 2-byte-EOL`. The two EOL
/// bytes may be `CR LF`, `LF CR`, `SP LF`, or `SP CR`.
pub(crate) fn parse_classic_xref(bytes: &[u8]) -> Option<Vec<XrefPiece>> {
    if !bytes.starts_with(b"xref") {
        return None;
    }
    let mut pieces = Vec::new();
    let mut pos = 4usize;

    // EOL after the `xref` keyword.
    let eol = eol_len(&bytes[pos..])?;
    pieces.push(XrefPiece::Literal {
        start: 0,
        len: pos + eol,
    });
    pos += eol;

    let mut any = false;
    while pos < bytes.len() {
        // Subsection header: `<start> <count>` EOL.
        let header_start = pos;
        let (start, after_start) = parse_uint_at(bytes, pos)?;
        pos = after_start;
        let spaces_start = pos;
        while pos < bytes.len() && bytes[pos] == b' ' {
            pos += 1;
        }
        if pos == spaces_start {
            return None;
        }
        let (count, after_count) = parse_uint_at(bytes, pos)?;
        pos = after_count;
        let eol = eol_len(&bytes[pos..])?;
        let header_end = pos + eol;
        pieces.push(XrefPiece::Literal {
            start: header_start,
            len: header_end - header_start,
        });
        pos = header_end;

        for i in 0..count {
            let end = pos.checked_add(20)?;
            if end > bytes.len() {
                return None;
            }
            let entry = &bytes[pos..end];
            if !is_entry_shape(entry) {
                return None;
            }
            let number = start.checked_add(i)?;
            let in_use = entry[17] == b'n';
            let offset = parse_digits(&entry[0..10]);
            pieces.push(XrefPiece::Entry {
                start: pos,
                number,
                offset,
                in_use,
            });
            pos = end;
        }
        any = true;
    }

    if !any || pos != bytes.len() {
        return None;
    }
    Some(pieces)
}

/// Whether a 20-byte window matches the classic cross-reference entry shape.
fn is_entry_shape(entry: &[u8]) -> bool {
    if entry.len() != 20 {
        return false;
    }
    if entry[10] != b' ' || entry[16] != b' ' {
        return false;
    }
    if !entry[11..16].iter().all(u8::is_ascii_digit) {
        return false;
    }
    if entry[17] != b'n' && entry[17] != b'f' {
        return false;
    }
    matches!(
        (entry[18], entry[19]),
        (b'\r', b'\n') | (b'\n', b'\r') | (b' ', b'\n') | (b' ', b'\r')
    )
}

/// Length of an end-of-line marker at the start of `bytes`, if any.
fn eol_len(bytes: &[u8]) -> Option<usize> {
    match bytes {
        [b'\r', b'\n', ..] => Some(2),
        [b'\n', ..] | [b'\r', ..] => Some(1),
        _ => None,
    }
}

/// Parse a non-negative decimal integer at `at`, returning `(value, next)`.
fn parse_uint_at(bytes: &[u8], at: usize) -> Option<(u64, usize)> {
    let mut i = at;
    let mut value: u64 = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        value = value
            .checked_mul(10)?
            .checked_add(u64::from(bytes[i] - b'0'))?;
        i += 1;
    }
    if i == at {
        return None;
    }
    Some((value, i))
}

/// Parse an all-digit slice as a non-negative decimal integer.
pub(crate) fn parse_digits(digits: &[u8]) -> Option<u64> {
    if digits.is_empty() {
        return None;
    }
    let mut value: u64 = 0;
    for &b in digits {
        if !b.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(u64::from(b - b'0'))?;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::pdf::samples::{is_negative_control, sample_pdfs};
    use crate::container::Descriptor;

    fn sample(name: &str) -> Vec<u8> {
        sample_pdfs()
            .into_iter()
            .find(|(n, _)| *n == name)
            .unwrap_or_else(|| panic!("sample {name} missing"))
            .1
    }

    fn basis_field(basis: &str, key: &str) -> Option<u64> {
        basis.split(';').find_map(|part| {
            let (k, v) = part.split_once('=')?;
            (k == key).then(|| v.parse().ok()).flatten()
        })
    }

    /// The item table of the single `PackSegments` op this candidate builds.
    fn pack_items(cand: &Candidate) -> &[PackItem] {
        cand.descriptor
            .program
            .ops
            .iter()
            .find_map(|op| match op {
                Op::PackSegments { items, .. } => Some(items.as_slice()),
                _ => None,
            })
            .expect("layout program is one PackSegments op")
    }

    /// Number of regenerated offsets in the packed item table.
    fn pack_emit_count(cand: &Candidate) -> usize {
        pack_items(cand)
            .iter()
            .filter(|item| matches!(item, PackItem::Emit { .. }))
            .count()
    }

    fn assert_materializes_exactly(name: &str, bytes: &[u8]) {
        let cand = propose_pdf_layout(bytes, Limits::DEFAULT)
            .unwrap()
            .unwrap_or_else(|| panic!("{name} must propose a layout candidate"));
        assert_eq!(cand.kind, CandidateKind::PdfLayout);
        assert_eq!(cand.descriptor.source_format, SOURCE_FORMAT_PDF);
        assert_eq!(cand.descriptor.source_len, bytes.len() as u64);
        // Exactly one object: the packed data object holding every literal byte.
        assert_eq!(
            cand.descriptor.objects.len(),
            1,
            "{name} layout carries one packed data object"
        );
        assert!(matches!(
            cand.descriptor.program.ops.as_slice(),
            [Op::PackSegments { .. }]
        ));
        assert!(cand.descriptor.models.is_empty());
        assert!(cand.descriptor.channels.is_empty());

        let (encoded, _) = cand.descriptor.serialize().unwrap();
        let parsed = Descriptor::parse(&encoded, Limits::DEFAULT).unwrap();
        let out = crate::materialize::materialize(&parsed, Limits::DEFAULT).unwrap();
        assert_eq!(out, bytes, "{name} layout candidate materializes exactly");
        assert_eq!(sha256(&out), sha256(bytes), "{name} layout sha");
    }

    #[test]
    fn layout_is_exact_on_corpus() {
        let mut accepted = 0usize;
        for (name, bytes) in sample_pdfs() {
            if is_negative_control(name) {
                assert!(
                    propose_pdf_layout(&bytes, Limits::DEFAULT)
                        .unwrap()
                        .is_none(),
                    "{name} is not a classic-xref PDF and must decline"
                );
                continue;
            }
            if propose_pdf_layout(&bytes, Limits::DEFAULT)
                .unwrap()
                .is_some()
            {
                assert_materializes_exactly(name, &bytes);
                accepted += 1;
            }
        }
        assert!(
            accepted >= 3,
            "expected several accepted classic-xref samples, found {accepted}"
        );
    }

    #[test]
    fn layout_v2_exact() {
        for name in ["classic.pdf", "many.pdf", "bigtext.pdf"] {
            assert_materializes_exactly(name, &sample(name));
        }
    }

    #[test]
    fn layout_v2_predicts_many_entries() {
        let bytes = sample("many.pdf");
        let cand = propose_pdf_layout(&bytes, Limits::DEFAULT)
            .unwrap()
            .unwrap();
        let emits = pack_emit_count(&cand);
        assert!(
            emits >= 100,
            "many.pdf must predict at least 100 xref offsets, got {emits}"
        );
        // Every Emit is either a predicted xref entry or the predicted startxref.
        let predicted = basis_field(&cand.descriptor.format_basis, "xref_predicted")
            .expect("format basis must report xref_predicted");
        let startxref = basis_field(&cand.descriptor.format_basis, "startxref_predicted")
            .expect("format basis must report startxref_predicted");
        assert_eq!(predicted + startxref, emits as u64);
        assert!(predicted >= 100, "many.pdf xref predictions: {predicted}");

        // The classic sample still predicts, and the pack framing stays compact.
        let classic = propose_pdf_layout(&sample("classic.pdf"), Limits::DEFAULT)
            .unwrap()
            .unwrap();
        assert!(pack_emit_count(&classic) > 0);
    }

    #[test]
    fn layout_falls_back_on_bad_offset() {
        // Object 1's real offset is `off1`, but the xref entry records a wrong
        // value. The entry must stay literal. `startxref` is still correct, so it
        // is the only predicted offset in the whole program.
        let mut b: Vec<u8> = Vec::new();
        b.extend_from_slice(b"%PDF-1.4\n");
        let off1 = b.len() as u64;
        b.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");
        let xref = b.len() as u64;
        let wrong = off1 + 3;
        b.extend_from_slice(
            format!("xref\n0 2\n0000000000 65535 f \n{wrong:010} 00000 n \n").as_bytes(),
        );
        b.extend_from_slice(
            format!("trailer\n<< /Size 2 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
        );

        let cand = propose_pdf_layout(&b, Limits::DEFAULT).unwrap().unwrap();
        assert_eq!(
            basis_field(&cand.descriptor.format_basis, "xref_predicted"),
            Some(0),
            "a mismatched offset must never be predicted"
        );

        // The only regenerated offset is the correctly predicted `startxref`.
        let emits = pack_emit_count(&cand);
        assert_eq!(
            emits, 1,
            "only startxref is predicted; bad entry is literal"
        );

        let (encoded, _) = cand.descriptor.serialize().unwrap();
        let parsed = Descriptor::parse(&encoded, Limits::DEFAULT).unwrap();
        let out = crate::materialize::materialize(&parsed, Limits::DEFAULT).unwrap();
        assert_eq!(out, b, "fallback must still be byte-exact");
    }

    #[test]
    fn layout_v2_declines() {
        // A cross-reference-stream PDF has no classic table to regenerate.
        assert!(
            propose_pdf_layout(&sample("xrefstream.pdf"), Limits::DEFAULT)
                .unwrap()
                .is_none(),
            "an xref-stream PDF must decline the layout candidate"
        );
        // More objects than the 255 markable slots cannot be expressed exactly
        // under the slot bound, so the candidate must decline rather than guess.
        assert!(
            propose_pdf_layout(&classic_with_objects(256), Limits::DEFAULT)
                .unwrap()
                .is_none(),
            "256 objects exceeds the 255 markable slots"
        );
        // 255 objects still fit: indices 0..=254, slot 255 reserved for xref.
        assert!(
            propose_pdf_layout(&classic_with_objects(255), Limits::DEFAULT)
                .unwrap()
                .is_some(),
            "255 objects must still be markable"
        );
    }

    /// Build a classic-xref PDF with `n` indirect objects and correct offsets.
    fn classic_with_objects(n: usize) -> Vec<u8> {
        let mut b: Vec<u8> = Vec::new();
        b.extend_from_slice(b"%PDF-1.4\n");
        let mut offsets = Vec::with_capacity(n);
        for number in 1..=n {
            offsets.push(b.len() as u64);
            b.extend_from_slice(format!("{number} 0 obj\n<< >>\nendobj\n").as_bytes());
        }
        let xref = b.len() as u64;
        b.extend_from_slice(format!("xref\n0 {}\n", n + 1).as_bytes());
        b.extend_from_slice(b"0000000000 65535 f \n");
        for &off in &offsets {
            b.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
        }
        b.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                n + 1
            )
            .as_bytes(),
        );
        b
    }

    #[test]
    fn layout_v2_deterministic() {
        for name in ["classic.pdf", "many.pdf", "bigtext.pdf"] {
            let bytes = sample(name);
            let a = propose_pdf_layout(&bytes, Limits::DEFAULT)
                .unwrap()
                .unwrap()
                .descriptor
                .serialize()
                .unwrap()
                .0;
            let b = propose_pdf_layout(&bytes, Limits::DEFAULT)
                .unwrap()
                .unwrap()
                .descriptor
                .serialize()
                .unwrap()
                .0;
            assert_eq!(a, b, "{name} layout bytes must be deterministic");
        }
    }

    /// Force the layout+rANS candidate for `bytes` and assert the full exact
    /// triple end-to-end through the normative decoder.
    #[cfg(feature = "rans")]
    fn assert_rans_materializes_exactly(name: &str, bytes: &[u8]) {
        let cand = propose_pdf_layout_rans(bytes, Limits::DEFAULT)
            .unwrap()
            .unwrap_or_else(|| panic!("{name} must propose a layout-rANS candidate"));
        assert_eq!(cand.kind, CandidateKind::PdfLayoutRans);
        assert_eq!(cand.descriptor.source_format, SOURCE_FORMAT_PDF);
        assert_eq!(cand.descriptor.source_len, bytes.len() as u64);
        // No literal objects: the data and the plan both travel in channels.
        assert!(cand.descriptor.objects.is_empty());
        assert_eq!(cand.descriptor.models.len(), 2);
        assert_eq!(cand.descriptor.channels.len(), 2);
        assert_eq!(cand.descriptor.channels[0].model_id, 0);
        assert_eq!(cand.descriptor.channels[1].model_id, 1);
        assert!(matches!(
            cand.descriptor.program.ops.as_slice(),
            [Op::PackedChannels {
                data_channel: 0,
                plan_channel: 1,
                ..
            }]
        ));

        let (encoded, _) = cand.descriptor.serialize().unwrap();
        let parsed = Descriptor::parse(&encoded, Limits::DEFAULT).unwrap();
        let out = crate::materialize::materialize(&parsed, Limits::DEFAULT).unwrap();
        assert_eq!(out, bytes, "{name} layout-rANS materializes exactly");
        assert_eq!(sha256(&out), sha256(bytes), "{name} layout-rANS sha");

        // The forced lane must survive the court's own decode-before-commit.
        let (forced, report) =
            crate::encode::encode_with(bytes, Limits::DEFAULT, Some(CandidateKind::PdfLayoutRans))
                .unwrap();
        assert_eq!(report.kind, CandidateKind::PdfLayoutRans);
        let (forced_out, _) =
            crate::materialize::decode_to_bytes(&forced, Limits::DEFAULT).unwrap();
        assert_eq!(forced_out, bytes, "{name} forced layout-rANS bytes");
        assert_eq!(
            sha256(&forced_out),
            sha256(bytes),
            "{name} forced layout-rANS sha"
        );
    }

    #[cfg(feature = "rans")]
    #[test]
    fn layout_rans_exact() {
        for name in ["classic.pdf", "bigtext.pdf", "many.pdf"] {
            assert_rans_materializes_exactly(name, &sample(name));
        }
    }

    #[cfg(feature = "rans")]
    #[test]
    fn layout_rans_deterministic() {
        for name in ["classic.pdf", "bigtext.pdf", "many.pdf"] {
            let bytes = sample(name);
            let a = propose_pdf_layout_rans(&bytes, Limits::DEFAULT)
                .unwrap()
                .unwrap()
                .descriptor
                .serialize()
                .unwrap()
                .0;
            let b = propose_pdf_layout_rans(&bytes, Limits::DEFAULT)
                .unwrap()
                .unwrap()
                .descriptor
                .serialize()
                .unwrap()
                .0;
            assert_eq!(a, b, "{name} layout-rANS bytes must be deterministic");
        }
    }

    #[cfg(feature = "rans")]
    #[test]
    fn layout_rans_declines_on_non_pdf() {
        // Non-PDF controls and cross-reference-stream PDFs: nothing to plan, so
        // the candidate must decline rather than store a non-plan.
        for name in ["notpdf.bin", "malformed.pdf", "xrefstream.pdf"] {
            assert!(
                propose_pdf_layout_rans(&sample(name), Limits::DEFAULT)
                    .unwrap()
                    .is_none(),
                "{name} must decline the layout-rANS candidate"
            );
        }
    }

    #[test]
    fn layout_measurements_report() {
        for (name, bytes) in sample_pdfs() {
            let Some(cand) = propose_pdf_layout(&bytes, Limits::DEFAULT).unwrap() else {
                eprintln!("layout[{name}]: declined");
                continue;
            };
            let (layout_bytes, _) = cand.descriptor.serialize().unwrap();
            let emits = pack_emit_count(&cand);
            eprintln!(
                "layout[{name}] source={} items={} emits={emits} layout={}",
                bytes.len(),
                pack_items(&cand).len(),
                layout_bytes.len(),
            );
        }

        for name in ["classic.pdf", "bigtext.pdf", "many.pdf"] {
            let bytes = sample(name);
            let (layout_bytes, _) =
                crate::encode::encode_with(&bytes, Limits::DEFAULT, Some(CandidateKind::PdfLayout))
                    .unwrap();
            let (raw_bytes, _) =
                crate::encode::encode_with(&bytes, Limits::DEFAULT, Some(CandidateKind::Raw))
                    .unwrap();
            #[cfg(feature = "rans")]
            let (byte_rans_bytes, _) =
                crate::encode::encode_with(&bytes, Limits::DEFAULT, Some(CandidateKind::ByteRans))
                    .unwrap();
            #[cfg(not(feature = "rans"))]
            let byte_rans_bytes: Vec<u8> = Vec::new();
            #[cfg(feature = "rans")]
            let (layout_rans_bytes, _) = crate::encode::encode_with(
                &bytes,
                Limits::DEFAULT,
                Some(CandidateKind::PdfLayoutRans),
            )
            .unwrap();
            #[cfg(not(feature = "rans"))]
            let layout_rans_bytes: Vec<u8> = Vec::new();
            let (_, auto) = crate::encode::encode(&bytes, Limits::DEFAULT).unwrap();
            eprintln!(
                "sizes[{name}] source={} raw={} byte_rans={} layout={} layout_rans={} auto={}({})",
                bytes.len(),
                raw_bytes.len(),
                byte_rans_bytes.len(),
                layout_bytes.len(),
                layout_rans_bytes.len(),
                auto.kind.name(),
                auto.encoded_len,
            );

            #[cfg(feature = "rans")]
            {
                let plan = build_layout_plan(&bytes, Limits::DEFAULT).unwrap().unwrap();
                let plan_bytes_len = crate::dra::op::encode_items(&plan.items).unwrap().len();
                let cand = propose_pdf_layout_rans(&bytes, Limits::DEFAULT)
                    .unwrap()
                    .unwrap();
                let model_bytes: usize = cand
                    .descriptor
                    .models
                    .iter()
                    .map(|m| m.encode().unwrap().len())
                    .sum();
                let payload_bytes: usize = cand
                    .descriptor
                    .channels
                    .iter()
                    .map(|c| c.payload.len())
                    .sum();
                eprintln!(
                    "rans[{name}] data={} plan_bytes={} models={} payloads={} total={}",
                    plan.data.len(),
                    plan_bytes_len,
                    model_bytes,
                    payload_bytes,
                    layout_rans_bytes.len(),
                );
            }
        }
    }
}
