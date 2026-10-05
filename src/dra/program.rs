//! Reconstruction program, coverage certificate, and bounded evaluation.

use crate::dra::op::Op;
use crate::error::{Error, Result};
use crate::limits::Limits;

/// DRA version carried in the graph record.
pub const DRA_VERSION: u8 = 4;

/// Number of positional-offset slots addressable by [`Op::MarkOffset`] and
/// [`Op::EmitOffset`]. Slot indices must be strictly below this bound.
pub const MAX_OFFSET_SLOTS: usize = 16;

/// Who is the reconstruction authority for an output interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authority {
    /// Bytes reproduced verbatim from a literal object or inline literal.
    Literal,
    /// Bytes deterministically generated (currently only by `REPEAT_LAST`).
    Generated,
    /// Bytes decoded from a typed entropy channel.
    EntropyChannel,
}

/// One output interval and its authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Start offset in the reconstructed output.
    pub start: u64,
    /// Length in bytes.
    pub len: u64,
    /// Reconstruction authority.
    pub authority: Authority,
}

/// A coverage certificate: the map from output intervals to authorities.
///
/// The invariant is that the spans are contiguous and cover exactly
/// `[0, total_len)` with no gaps and no overlapping authorities.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CoverageMap {
    /// Ordered, contiguous spans.
    pub spans: Vec<Span>,
}

impl CoverageMap {
    /// Total covered length.
    pub fn total_len(&self) -> u64 {
        self.spans.last().map(|s| s.start + s.len).unwrap_or(0)
    }

    /// Assert contiguity/gap-freedom and equality with a declared length.
    pub fn validate(&self, declared_len: u64) -> Result<()> {
        let mut expected = 0u64;
        for s in &self.spans {
            if s.start != expected {
                return Err(Error::coverage_violation(format!(
                    "coverage gap/overlap: expected span at {expected}, found {}",
                    s.start
                )));
            }
            expected = expected
                .checked_add(s.len)
                .ok_or_else(|| Error::coverage_violation("coverage length overflow"))?;
        }
        if expected != declared_len {
            return Err(Error::coverage_violation(format!(
                "coverage covers {expected} bytes but {declared_len} were declared"
            )));
        }
        Ok(())
    }
}

/// A bounded, ordered list of reconstruction instructions.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Program {
    /// Instructions, evaluated in order.
    pub ops: Vec<Op>,
}

impl Program {
    /// Wrap an instruction list.
    pub fn new(ops: Vec<Op>) -> Self {
        Program { ops }
    }

    /// Encode the program to graph-record payload bytes.
    pub fn encode(&self) -> Result<Vec<u8>> {
        let count = u32::try_from(self.ops.len())
            .map_err(|_| Error::resource_limit("too many DRA instructions"))?;
        let mut out = Vec::with_capacity(5 + self.ops.len() * 5);
        out.push(DRA_VERSION);
        out.extend_from_slice(&count.to_le_bytes());
        for op in &self.ops {
            op.encode(&mut out)?;
        }
        Ok(out)
    }

    /// Decode a graph-record payload.
    pub fn decode(data: &[u8], limits: Limits) -> Result<Program> {
        if data.is_empty() {
            return Err(Error::invalid_graph("empty graph record"));
        }
        if data[0] != DRA_VERSION {
            return Err(Error::unsupported_version(format!(
                "DRA version {} is not supported",
                data[0]
            )));
        }
        if data.len() < 5 {
            return Err(Error::invalid_graph("truncated graph header"));
        }
        let count = u32::from_le_bytes([data[1], data[2], data[3], data[4]]);
        if count > limits.max_graph_ops {
            return Err(Error::resource_limit(format!(
                "graph has {count} instructions, limit {}",
                limits.max_graph_ops
            )));
        }
        let mut pos = 5usize;
        let mut ops = Vec::with_capacity(count as usize);
        for _ in 0..count {
            ops.push(Op::decode(data, &mut pos, limits)?);
        }
        if pos != data.len() {
            return Err(Error::invalid_graph(format!(
                "graph record has {} trailing bytes",
                data.len() - pos
            )));
        }
        Ok(Program { ops })
    }

    /// Walk the program, validating instruction semantics and returning both
    /// the predicted output length and the coverage certificate, using only
    /// object and channel *lengths* (no byte materialization, no large
    /// allocation).
    pub fn analyze(
        &self,
        object_lens: &[u64],
        channel_lens: &[u64],
        limits: Limits,
    ) -> Result<(u64, CoverageMap)> {
        if self.ops.len() as u64 > limits.max_graph_ops as u64 {
            return Err(Error::resource_limit("graph instruction limit exceeded"));
        }
        let mut spans: Vec<Span> = Vec::new();
        let mut total: u64 = 0;
        let mut last_len: u64 = 0;
        let mut have_last = false;
        // Which positional slots have been marked earlier in program order.
        let mut marked = [false; MAX_OFFSET_SLOTS];

        for op in &self.ops {
            match op {
                Op::EmitObject { object_id } => {
                    let len = *object_lens.get(*object_id as usize).ok_or_else(|| {
                        Error::invalid_graph(format!("graph references missing object {object_id}"))
                    })?;
                    total = total
                        .checked_add(len)
                        .ok_or_else(|| Error::resource_limit("output length overflow"))?;
                    if len > 0 {
                        spans.push(Span {
                            start: total - len,
                            len,
                            authority: Authority::Literal,
                        });
                    }
                    last_len = len;
                    have_last = true;
                }
                Op::Inline { bytes } => {
                    let len = bytes.len() as u64;
                    total = total
                        .checked_add(len)
                        .ok_or_else(|| Error::resource_limit("output length overflow"))?;
                    if len > 0 {
                        spans.push(Span {
                            start: total - len,
                            len,
                            authority: Authority::Literal,
                        });
                    }
                    last_len = len;
                    have_last = true;
                }
                Op::DecodeChannel { channel_id } => {
                    let len = *channel_lens.get(*channel_id as usize).ok_or_else(|| {
                        Error::invalid_graph(format!(
                            "graph references missing entropy channel {channel_id}"
                        ))
                    })?;
                    total = total
                        .checked_add(len)
                        .ok_or_else(|| Error::resource_limit("output length overflow"))?;
                    if len > 0 {
                        spans.push(Span {
                            start: total - len,
                            len,
                            authority: Authority::EntropyChannel,
                        });
                    }
                    last_len = len;
                    have_last = true;
                }
                Op::InterleaveChannels {
                    first_payload_channel,
                    payload_channel_count,
                    ..
                } => {
                    let first = *first_payload_channel as usize;
                    let count = *payload_channel_count as usize;
                    let end = first
                        .checked_add(count)
                        .ok_or_else(|| Error::invalid_graph("interleave channel range overflow"))?;
                    if end > channel_lens.len() {
                        return Err(Error::invalid_graph(format!(
                            "interleave payload channel range {first}..{end} exceeds {} channels",
                            channel_lens.len()
                        )));
                    }
                    let mut len: u64 = 0;
                    for &seg in &channel_lens[first..end] {
                        len = len
                            .checked_add(seg)
                            .ok_or_else(|| Error::resource_limit("interleave length overflow"))?;
                    }
                    total = total
                        .checked_add(len)
                        .ok_or_else(|| Error::resource_limit("output length overflow"))?;
                    if len > 0 {
                        spans.push(Span {
                            start: total - len,
                            len,
                            authority: Authority::Generated,
                        });
                    }
                    last_len = len;
                    have_last = true;
                }
                Op::MarkOffset { slot } => {
                    let idx = *slot as usize;
                    if idx >= MAX_OFFSET_SLOTS {
                        return Err(Error::invalid_graph(format!(
                            "MARK_OFFSET slot {slot} exceeds {MAX_OFFSET_SLOTS} slots"
                        )));
                    }
                    // Bookkeeping only: contributes no output bytes and does not
                    // disturb the pending "last block" for REPEAT_LAST.
                    marked[idx] = true;
                }
                Op::EmitOffset { slot, width } => {
                    let idx = *slot as usize;
                    if idx >= MAX_OFFSET_SLOTS {
                        return Err(Error::invalid_graph(format!(
                            "EMIT_OFFSET slot {slot} exceeds {MAX_OFFSET_SLOTS} slots"
                        )));
                    }
                    if *width == 0 || *width > 20 {
                        return Err(Error::invalid_graph(format!(
                            "EMIT_OFFSET width {width} is outside 1..=20"
                        )));
                    }
                    if !marked[idx] {
                        return Err(Error::invalid_graph(format!(
                            "EMIT_OFFSET references unmarked slot {slot}"
                        )));
                    }
                    // The value is a decimal number left-zero-padded to `width`,
                    // so the predicted contribution is exactly `width` bytes.
                    let len = *width as u64;
                    total = total
                        .checked_add(len)
                        .ok_or_else(|| Error::resource_limit("output length overflow"))?;
                    spans.push(Span {
                        start: total - len,
                        len,
                        authority: Authority::Generated,
                    });
                    last_len = len;
                    have_last = true;
                }
                Op::RepeatLast { count } => {
                    if !have_last {
                        return Err(Error::invalid_graph(
                            "REPEAT_LAST has no preceding literal instruction",
                        ));
                    }
                    if (*count as u64) > limits.max_repeat_count {
                        return Err(Error::resource_limit(format!(
                            "REPEAT_LAST count {count} exceeds limit {}",
                            limits.max_repeat_count
                        )));
                    }
                    let extra = last_len
                        .checked_mul(*count as u64)
                        .ok_or_else(|| Error::resource_limit("repeat length overflow"))?;
                    total = total
                        .checked_add(extra)
                        .ok_or_else(|| Error::resource_limit("output length overflow"))?;
                    if extra > 0 {
                        spans.push(Span {
                            start: total - extra,
                            len: extra,
                            authority: Authority::Generated,
                        });
                    }
                    // Consecutive REPEAT_LAST is rejected to keep expansion
                    // statically bounded and unambiguous.
                    have_last = false;
                    last_len = 0;
                }
            }
            if total > limits.max_output_bytes {
                return Err(Error::resource_limit(format!(
                    "predicted output {total} exceeds limit {}",
                    limits.max_output_bytes
                )));
            }
        }

        Ok((total, CoverageMap { spans }))
    }

    /// Analyze using concrete object and channel tables.
    pub fn analyze_inputs(
        &self,
        objects: &[Vec<u8>],
        channels: &[Vec<u8>],
        limits: Limits,
    ) -> Result<(u64, CoverageMap)> {
        let object_lens: Vec<u64> = objects.iter().map(|o| o.len() as u64).collect();
        let channel_lens: Vec<u64> = channels.iter().map(|c| c.len() as u64).collect();
        self.analyze(&object_lens, &channel_lens, limits)
    }

    /// Convenience wrapper over [`Program::analyze`] for a program with no
    /// entropy channels.
    pub fn analyze_objects(
        &self,
        objects: &[Vec<u8>],
        limits: Limits,
    ) -> Result<(u64, CoverageMap)> {
        let lens: Vec<u64> = objects.iter().map(|o| o.len() as u64).collect();
        self.analyze(&lens, &[], limits)
    }

    /// Materialize the program's output, enforcing all bounds.
    pub fn eval(
        &self,
        objects: &[Vec<u8>],
        channels: &[Vec<u8>],
        limits: Limits,
    ) -> Result<Vec<u8>> {
        let (predicted, _coverage) = self.analyze_inputs(objects, channels, limits)?;
        let cap = predicted.min(64 * 1024 * 1024) as usize;
        let mut out: Vec<u8> = Vec::with_capacity(cap);
        let mut have_last = false;
        let mut block_len: usize = 0;
        // Recorded output positions and their marked state.
        let mut slots = [0u64; MAX_OFFSET_SLOTS];
        let mut marked = [false; MAX_OFFSET_SLOTS];

        for op in &self.ops {
            match op {
                Op::EmitObject { object_id } => {
                    let obj = objects.get(*object_id as usize).ok_or_else(|| {
                        Error::invalid_graph(format!("graph references missing object {object_id}"))
                    })?;
                    block_len = obj.len();
                    out.extend_from_slice(obj);
                    have_last = true;
                }
                Op::Inline { bytes } => {
                    block_len = bytes.len();
                    out.extend_from_slice(bytes);
                    have_last = true;
                }
                Op::DecodeChannel { channel_id } => {
                    let ch = channels.get(*channel_id as usize).ok_or_else(|| {
                        Error::invalid_graph(format!(
                            "graph references missing entropy channel {channel_id}"
                        ))
                    })?;
                    block_len = ch.len();
                    out.extend_from_slice(ch);
                    have_last = true;
                }
                Op::InterleaveChannels {
                    kinds_channel,
                    lengths_channel,
                    first_payload_channel,
                    payload_channel_count,
                } => {
                    let kinds = channels.get(*kinds_channel as usize).ok_or_else(|| {
                        Error::invalid_graph(format!(
                            "graph references missing entropy channel {kinds_channel}"
                        ))
                    })?;
                    let lengths = channels.get(*lengths_channel as usize).ok_or_else(|| {
                        Error::invalid_graph(format!(
                            "graph references missing entropy channel {lengths_channel}"
                        ))
                    })?;
                    let token_count = kinds.len();
                    let required = token_count
                        .checked_mul(4)
                        .ok_or_else(|| Error::invalid_graph("interleave lengths overflow"))?;
                    if lengths.len() != required {
                        return Err(Error::invalid_graph(format!(
                            "interleave length channel has {} bytes but {required} are required",
                            lengths.len()
                        )));
                    }
                    let first = *first_payload_channel as usize;
                    let count = *payload_channel_count as usize;
                    let end = first
                        .checked_add(count)
                        .ok_or_else(|| Error::invalid_graph("interleave channel range overflow"))?;
                    if end > channels.len() {
                        return Err(Error::invalid_graph(format!(
                            "interleave payload channel range {first}..{end} exceeds {} channels",
                            channels.len()
                        )));
                    }
                    let start = out.len();
                    let mut cursors = vec![0usize; count];
                    for (i, chunk) in lengths.as_chunks::<4>().0.iter().enumerate() {
                        let k = kinds[i] as usize;
                        if k >= count {
                            return Err(Error::invalid_graph(format!(
                                "interleave token {i} names kind {k} outside 0..{count}"
                            )));
                        }
                        let l = u32::from_le_bytes(*chunk) as usize;
                        let ch = &channels[first + k];
                        let cursor = cursors[k];
                        let seg_end = cursor.checked_add(l).ok_or_else(|| {
                            Error::invalid_graph("interleave payload cursor overflow")
                        })?;
                        if seg_end > ch.len() {
                            return Err(Error::invalid_graph(format!(
                                "interleave token {i} reads {l} bytes past channel {} ({cursor}..{seg_end} of {})",
                                first + k,
                                ch.len()
                            )));
                        }
                        out.extend_from_slice(&ch[cursor..seg_end]);
                        cursors[k] = seg_end;
                        if out.len() as u64 > limits.max_output_bytes {
                            return Err(Error::resource_limit(
                                "output exceeds materialization limit",
                            ));
                        }
                    }
                    for (k, &cursor) in cursors.iter().enumerate() {
                        let ch_len = channels[first + k].len();
                        if cursor != ch_len {
                            return Err(Error::invalid_graph(format!(
                                "interleave payload channel {} was not fully consumed ({cursor} of {ch_len})",
                                first + k
                            )));
                        }
                    }
                    block_len = out.len() - start;
                    have_last = true;
                }
                Op::MarkOffset { slot } => {
                    let idx = *slot as usize;
                    if idx >= MAX_OFFSET_SLOTS {
                        return Err(Error::invalid_graph(format!(
                            "MARK_OFFSET slot {slot} exceeds {MAX_OFFSET_SLOTS} slots"
                        )));
                    }
                    slots[idx] = out.len() as u64;
                    marked[idx] = true;
                }
                Op::EmitOffset { slot, width } => {
                    let idx = *slot as usize;
                    if idx >= MAX_OFFSET_SLOTS {
                        return Err(Error::invalid_graph(format!(
                            "EMIT_OFFSET slot {slot} exceeds {MAX_OFFSET_SLOTS} slots"
                        )));
                    }
                    if *width == 0 || *width > 20 {
                        return Err(Error::invalid_graph(format!(
                            "EMIT_OFFSET width {width} is outside 1..=20"
                        )));
                    }
                    if !marked[idx] {
                        return Err(Error::invalid_graph(format!(
                            "EMIT_OFFSET references unmarked slot {slot}"
                        )));
                    }
                    let digits = slots[idx].to_string();
                    if digits.len() > *width as usize {
                        return Err(Error::invalid_graph(format!(
                            "EMIT_OFFSET slot {slot} value {} needs {} bytes but width is {width}",
                            slots[idx],
                            digits.len()
                        )));
                    }
                    let start = out.len();
                    out.extend(std::iter::repeat_n(b'0', *width as usize - digits.len()));
                    out.extend_from_slice(digits.as_bytes());
                    debug_assert_eq!(out.len() - start, *width as usize);
                    block_len = out.len() - start;
                    have_last = true;
                }
                Op::RepeatLast { count } => {
                    if !have_last {
                        return Err(Error::invalid_graph(
                            "REPEAT_LAST has no preceding literal instruction",
                        ));
                    }
                    // The preceding literal instruction produced exactly the
                    // trailing `block_len` bytes; repeat that block `count`
                    // more times. Consecutive repeats were rejected during
                    // analysis, so `have_last` is now cleared.
                    let start = out.len() - block_len;
                    for _ in 0..*count {
                        out.extend_from_within(start..start + block_len);
                    }
                    have_last = false;
                    block_len = 0;
                }
            }
            if out.len() as u64 > limits.max_output_bytes {
                return Err(Error::resource_limit(
                    "output exceeds materialization limit",
                ));
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn objs(xs: &[&[u8]]) -> Vec<Vec<u8>> {
        xs.iter().map(|x| x.to_vec()).collect()
    }

    #[test]
    fn literal_concat_and_coverage() {
        let objects = objs(&[b"hello ", b"world"]);
        let p = Program::new(vec![
            Op::EmitObject { object_id: 0 },
            Op::EmitObject { object_id: 1 },
        ]);
        let (len, cov) = p.analyze_objects(&objects, Limits::DEFAULT).unwrap();
        assert_eq!(len, 11);
        cov.validate(11).unwrap();
        assert_eq!(
            p.eval(&objects, &[], Limits::DEFAULT).unwrap(),
            b"hello world"
        );
    }

    #[test]
    fn inline_and_repeat() {
        let objects = objs(&[]);
        let p = Program::new(vec![
            Op::Inline {
                bytes: b"ab".to_vec(),
            },
            Op::RepeatLast { count: 2 },
        ]);
        let (len, cov) = p.analyze_objects(&objects, Limits::DEFAULT).unwrap();
        assert_eq!(len, 6);
        cov.validate(6).unwrap();
        assert_eq!(p.eval(&objects, &[], Limits::DEFAULT).unwrap(), b"ababab");
        // Authority split: first "ab" literal, remaining "abab" generated.
        assert_eq!(
            cov.spans[0],
            Span {
                start: 0,
                len: 2,
                authority: Authority::Literal
            }
        );
        assert_eq!(
            cov.spans[1],
            Span {
                start: 2,
                len: 4,
                authority: Authority::Generated
            }
        );
    }

    #[test]
    fn decode_channel_and_repeat() {
        let objects = objs(&[]);
        let channels = objs(&[b"abc"]);
        let p = Program::new(vec![
            Op::DecodeChannel { channel_id: 0 },
            Op::RepeatLast { count: 1 },
        ]);
        let (len, cov) = p
            .analyze_inputs(&objects, &channels, Limits::DEFAULT)
            .unwrap();
        assert_eq!(len, 6);
        cov.validate(6).unwrap();
        assert_eq!(
            cov.spans[0],
            Span {
                start: 0,
                len: 3,
                authority: Authority::EntropyChannel,
            }
        );
        assert_eq!(
            cov.spans[1],
            Span {
                start: 3,
                len: 3,
                authority: Authority::Generated,
            }
        );
        assert_eq!(
            p.eval(&objects, &channels, Limits::DEFAULT).unwrap(),
            b"abcabc"
        );
    }

    #[test]
    fn rejects_missing_channel() {
        let p = Program::new(vec![Op::DecodeChannel { channel_id: 5 }]);
        let e = p.analyze_inputs(&[], &[], Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::InvalidGraph);
    }

    #[test]
    fn rejects_missing_object() {
        let objects = objs(&[]);
        let p = Program::new(vec![Op::EmitObject { object_id: 3 }]);
        let e = p.analyze_objects(&objects, Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::InvalidGraph);
    }

    #[test]
    fn rejects_leading_repeat() {
        let objects = objs(&[]);
        let p = Program::new(vec![Op::RepeatLast { count: 1 }]);
        let e = p.analyze_objects(&objects, Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::InvalidGraph);
    }

    #[test]
    fn rejects_consecutive_repeats() {
        let objects = objs(&[]);
        let p = Program::new(vec![
            Op::Inline {
                bytes: b"x".to_vec(),
            },
            Op::RepeatLast { count: 1 },
            Op::RepeatLast { count: 1 },
        ]);
        let e = p.analyze_objects(&objects, Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::InvalidGraph);
    }

    #[test]
    fn enforces_output_limit() {
        let objects = objs(&[b"abcdefgh"]);
        let p = Program::new(vec![
            Op::EmitObject { object_id: 0 },
            Op::RepeatLast { count: 1000 },
        ]);
        let limits = Limits {
            max_output_bytes: 64,
            ..Limits::DEFAULT
        };
        let e = p.analyze_objects(&objects, limits).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::ResourceLimit);
    }

    #[test]
    fn coverage_gap_is_rejected() {
        let cov = CoverageMap {
            spans: vec![
                Span {
                    start: 0,
                    len: 2,
                    authority: Authority::Literal,
                },
                Span {
                    start: 3,
                    len: 2,
                    authority: Authority::Literal,
                },
            ],
        };
        let e = cov.validate(4).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::CoverageViolation);
    }

    #[test]
    fn program_roundtrips_via_bytes() {
        let p = Program::new(vec![
            Op::EmitObject { object_id: 1 },
            Op::Inline {
                bytes: b"hi".to_vec(),
            },
            Op::RepeatLast { count: 3 },
        ]);
        let enc = p.encode().unwrap();
        let back = Program::decode(&enc, Limits::DEFAULT).unwrap();
        assert_eq!(p, back);
    }

    fn le_lengths(lens: &[u32]) -> Vec<u8> {
        let mut v = Vec::with_capacity(lens.len() * 4);
        for &l in lens {
            v.extend_from_slice(&l.to_le_bytes());
        }
        v
    }

    fn interleave_op() -> Op {
        Op::InterleaveChannels {
            kinds_channel: 0,
            lengths_channel: 1,
            first_payload_channel: 2,
            payload_channel_count: 2,
        }
    }

    /// Channels: kinds `[0,1,0,1]`, lengths `[2,3,1,2]`, and two payload
    /// channels. Token order interleaves to `ab` `def` `c` `gh` = "abdefcgh".
    fn interleave_channels() -> Vec<Vec<u8>> {
        vec![
            vec![0, 1, 0, 1],
            le_lengths(&[2, 3, 1, 2]),
            b"abc".to_vec(),
            b"defgh".to_vec(),
        ]
    }

    #[test]
    fn interleave_roundtrip() {
        let channels = interleave_channels();
        let p = Program::new(vec![interleave_op()]);
        let (len, cov) = p.analyze_inputs(&[], &channels, Limits::DEFAULT).unwrap();
        assert_eq!(len, 8);
        cov.validate(8).unwrap();
        assert_eq!(
            cov.spans,
            vec![Span {
                start: 0,
                len: 8,
                authority: Authority::Generated,
            }]
        );
        assert_eq!(
            p.eval(&[], &channels, Limits::DEFAULT).unwrap(),
            b"abdefcgh"
        );

        // Byte round-trip of the instruction through the graph record.
        let enc = p.encode().unwrap();
        assert_eq!(Program::decode(&enc, Limits::DEFAULT).unwrap(), p);

        // The op is also the "last block" for a following REPEAT_LAST.
        let p2 = Program::new(vec![interleave_op(), Op::RepeatLast { count: 1 }]);
        let (len2, cov2) = p2.analyze_inputs(&[], &channels, Limits::DEFAULT).unwrap();
        assert_eq!(len2, 16);
        cov2.validate(16).unwrap();
        assert_eq!(
            p2.eval(&[], &channels, Limits::DEFAULT).unwrap(),
            b"abdefcghabdefcgh"
        );
    }

    #[test]
    fn interleave_rejects_bad_channel_index() {
        // Payload range past the end of the channel table: rejected by analyze.
        let p = Program::new(vec![Op::InterleaveChannels {
            kinds_channel: 0,
            lengths_channel: 1,
            first_payload_channel: 2,
            payload_channel_count: 3,
        }]);
        let short = vec![vec![0u8], le_lengths(&[0]), vec![0u8]];
        let e = p.analyze_inputs(&[], &short, Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::InvalidGraph);

        // A token kind that names a channel outside the payload range: rejected
        // by eval (analyze only reasons about channel lengths).
        let channels = vec![vec![2u8], le_lengths(&[1]), b"x".to_vec(), b"y".to_vec()];
        let p = Program::new(vec![interleave_op()]);
        let e = p.eval(&[], &channels, Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::InvalidGraph);
    }

    #[test]
    fn interleave_rejects_unconsumed_payload() {
        // kind 0 length 1 consumes only one byte of a two-byte payload channel.
        let channels = vec![vec![0u8], le_lengths(&[1]), b"ab".to_vec(), b"z".to_vec()];
        let p = Program::new(vec![interleave_op()]);
        let e = p.eval(&[], &channels, Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::InvalidGraph);
    }

    #[test]
    fn interleave_lengths_must_be_4x_tokens() {
        // Two kind tokens but only one length (4 bytes instead of 8).
        let channels = vec![vec![0u8, 0u8], le_lengths(&[1]), b"ab".to_vec()];
        let p = Program::new(vec![Op::InterleaveChannels {
            kinds_channel: 0,
            lengths_channel: 1,
            first_payload_channel: 2,
            payload_channel_count: 1,
        }]);
        let e = p.eval(&[], &channels, Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::InvalidGraph);
    }

    #[test]
    fn interleave_predicts_from_channel_lens() {
        let p = Program::new(vec![interleave_op()]);
        // Only payload channel lengths (3 + 5) contribute; the kinds (4) and
        // lengths (16) channels are inputs, not output.
        let channel_lens = [4u64, 16, 3, 5];
        let (len, cov) = p.analyze(&[], &channel_lens, Limits::DEFAULT).unwrap();
        assert_eq!(len, 8);
        assert_eq!(
            cov.spans,
            vec![Span {
                start: 0,
                len: 8,
                authority: Authority::Generated,
            }]
        );
    }

    #[test]
    fn mark_emit_roundtrip() {
        // MarkOffset after "abc" records position 3; the later EmitOffset
        // renders it as a zero-padded 3-byte decimal, so the digits are "003".
        let p = Program::new(vec![
            Op::Inline {
                bytes: b"abc".to_vec(),
            },
            Op::MarkOffset { slot: 0 },
            Op::Inline {
                bytes: b"def".to_vec(),
            },
            Op::EmitOffset { slot: 0, width: 3 },
        ]);
        let (len, cov) = p.analyze_objects(&[], Limits::DEFAULT).unwrap();
        assert_eq!(len, 9);
        cov.validate(9).unwrap();
        assert_eq!(p.eval(&[], &[], Limits::DEFAULT).unwrap(), b"abcdef003");

        // Byte round-trip of both new instructions through the graph record.
        let enc = p.encode().unwrap();
        assert_eq!(Program::decode(&enc, Limits::DEFAULT).unwrap(), p);

        // EmitOffset is a valid "last block" for a following REPEAT_LAST, with
        // block length equal to `width`.
        let p2 = Program::new(vec![
            Op::MarkOffset { slot: 0 },
            Op::EmitOffset { slot: 0, width: 2 },
            Op::RepeatLast { count: 1 },
        ]);
        let (len2, cov2) = p2.analyze_objects(&[], Limits::DEFAULT).unwrap();
        assert_eq!(len2, 4);
        cov2.validate(4).unwrap();
        assert_eq!(p2.eval(&[], &[], Limits::DEFAULT).unwrap(), b"0000");
    }

    #[test]
    fn emit_zero_pads() {
        // A position of 0 rendered at width 3 is entirely zero-padded.
        let p = Program::new(vec![
            Op::MarkOffset { slot: 1 },
            Op::EmitOffset { slot: 1, width: 3 },
        ]);
        let (len, cov) = p.analyze_objects(&[], Limits::DEFAULT).unwrap();
        assert_eq!(len, 3);
        cov.validate(3).unwrap();
        assert_eq!(p.eval(&[], &[], Limits::DEFAULT).unwrap(), b"000");
    }

    #[test]
    fn emit_width_too_small_errors() {
        // Analysis predicts the bounded width, but the marked position 10 needs
        // two digits; materialization must refuse rather than emit a truncated
        // (wrong-width) value.
        let p = Program::new(vec![
            Op::Inline {
                bytes: b"0123456789".to_vec(),
            },
            Op::MarkOffset { slot: 0 },
            Op::EmitOffset { slot: 0, width: 1 },
        ]);
        let (len, _) = p.analyze_objects(&[], Limits::DEFAULT).unwrap();
        assert_eq!(len, 11);
        let e = p.eval(&[], &[], Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::InvalidGraph);
    }

    #[test]
    fn emit_unmarked_slot_errors() {
        // Emitting a slot that was never marked earlier in program order is
        // rejected statically by analysis.
        let p = Program::new(vec![Op::EmitOffset { slot: 0, width: 4 }]);
        let e = p.analyze_objects(&[], Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::InvalidGraph);
    }

    #[test]
    fn mark_slot_out_of_range_errors() {
        let p = Program::new(vec![Op::MarkOffset { slot: 16 }]);
        let e = p.analyze_objects(&[], Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::InvalidGraph);

        let p = Program::new(vec![
            Op::MarkOffset { slot: 0 },
            Op::EmitOffset { slot: 16, width: 1 },
        ]);
        let e = p.analyze_objects(&[], Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::InvalidGraph);
    }

    #[test]
    fn analyze_predicts_width_bytes() {
        // The predicted contribution is exactly `width`, regardless of the
        // eventual digit count, and the authority is Generated.
        let p = Program::new(vec![
            Op::MarkOffset { slot: 3 },
            Op::EmitOffset { slot: 3, width: 7 },
        ]);
        let (len, cov) = p.analyze_objects(&[], Limits::DEFAULT).unwrap();
        assert_eq!(len, 7);
        cov.validate(7).unwrap();
        assert_eq!(
            cov.spans,
            vec![Span {
                start: 0,
                len: 7,
                authority: Authority::Generated,
            }]
        );
    }
}
