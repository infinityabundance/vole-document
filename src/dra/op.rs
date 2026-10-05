//! DRA instruction set.
//!
//! The algebra is deliberately tiny, versioned, and non-Turing-complete. Every
//! instruction has a deterministic, statically boundable output length. There
//! is no I/O, no clock, no RNG, and no external authority.

use crate::error::{Error, Result};
use crate::limits::Limits;

/// A single reconstruction instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// Emit the exact bytes of the referenced object.
    EmitObject {
        /// Index into the descriptor's object table.
        object_id: u32,
    },
    /// Emit literal bytes carried inline in the graph record.
    Inline {
        /// The literal bytes.
        bytes: Vec<u8>,
    },
    /// Repeat the byte output of the immediately preceding instruction
    /// `count` additional times.
    RepeatLast {
        /// Number of extra copies.
        count: u32,
    },
    /// Emit the decoded bytes of the referenced entropy channel.
    DecodeChannel {
        /// Index into the descriptor's entropy channel table.
        channel_id: u32,
    },
    /// Reconstruct bytes by interleaving a contiguous range of per-kind payload
    /// channels. A *kind* channel holds one kind byte per token; a *lengths*
    /// channel holds one little-endian `u32` per token; payload channel
    /// `first_payload_channel + k` carries the token spans whose kind is `k`.
    InterleaveChannels {
        /// Channel holding one kind byte per token.
        kinds_channel: u32,
        /// Channel holding four little-endian length bytes per token.
        lengths_channel: u32,
        /// First channel of the contiguous per-kind payload channel range.
        first_payload_channel: u32,
        /// Number of payload channels; valid kinds are `0..payload_channel_count`.
        payload_channel_count: u8,
    },
    /// Record the current output position (a u64) into the named slot. Emits no
    /// output bytes; the recorded value is available to later [`Op::EmitOffset`]
    /// instructions.
    MarkOffset {
        /// Slot index; must be below [`crate::dra::program::MAX_OFFSET_SLOTS`].
        slot: u8,
    },
    /// Emit the decimal form of a previously marked output position, left
    /// zero-padded to exactly `width` bytes.
    EmitOffset {
        /// Slot index marked by an earlier [`Op::MarkOffset`].
        slot: u8,
        /// Exact output width in bytes.
        width: u8,
    },
    /// Reconstruct output from a compact item table over one data object,
    /// amortizing per-segment framing: literal runs copy contiguous data-object
    /// bytes, marks record output positions, and emits render marked positions as
    /// fixed-width decimals. The data object must be consumed exactly.
    PackSegments {
        /// Index into the descriptor's object table.
        data_object: u32,
        /// Ordered items describing the reconstruction.
        items: Vec<PackItem>,
    },
    /// Reconstruct output from a *data* entropy channel interpreted by a
    /// serialized item table carried in a *plan* entropy channel. This lets one
    /// layout plan travel through rANS-coded channels instead of a literal data
    /// object, so the plan and its data pay entropy-coding cost together. The
    /// plan channel holds exactly [`encode_items`] of the item table, and the
    /// data channel must be consumed exactly.
    PackedChannels {
        /// Index into the descriptor's entropy channel table for the data.
        data_channel: u32,
        /// Index into the descriptor's entropy channel table for the plan.
        plan_channel: u32,
        /// Exact reconstructed output length; must equal the produced length.
        declared_output_len: u64,
    },
    /// Reconstruct the *exact original raw DEFLATE bitstream* from a plaintext
    /// source plus an opaque correction object, replaying the producer's DEFLATE
    /// coding decisions. Emits raw DEFLATE (RFC 1951) bytes only; any zlib framing
    /// is composed by surrounding instructions. `declared_output_len` is the exact
    /// expected raw payload length, validated at evaluation.
    ///
    /// The plaintext may come from either the object table (`source_kind =
    /// `[`DEFLATE_SOURCE_OBJECT`]) or an entropy channel (`source_kind =
    /// `[`DEFLATE_SOURCE_CHANNEL`]); the channel form lets several streams share
    /// one stored plaintext capsule. Reconstruction can panic internally on
    /// hostile correction data, so it is isolated and never panics the decoder
    /// (see `crate::codec::deflate`).
    DeflateReplay {
        /// Replay-codec identity: which exact reconstruction semantics the
        /// `corrections` blob is bound to. Must be [`REPLAY_DEFLATE_PREFLATE_0_7_6`].
        /// The correction state is an opaque, version-coupled `preflate` blob; this
        /// tag makes that explicit on the wire so the format can never silently
        /// treat the current preflate internal representation as a stable standard.
        replay_codec: u8,
        /// Plaintext source kind ([`DEFLATE_SOURCE_OBJECT`] / [`DEFLATE_SOURCE_CHANNEL`]).
        source_kind: u8,
        /// Index into the object or channel table selected by `source_kind`.
        source_id: u32,
        /// Index into the descriptor's object table holding the corrections.
        corrections_object: u32,
        /// Exact length (bytes) of the raw DEFLATE payload the op reproduces.
        declared_output_len: u32,
    },
}

/// One item in a [`Op::PackSegments`] item table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackItem {
    /// Copy `len` contiguous bytes from the data object, advancing its cursor.
    Literal {
        /// Number of bytes to copy.
        len: u32,
    },
    /// Record the current output position into `slot`.
    Mark {
        /// Slot index; must be below [`crate::dra::program::MAX_OFFSET_SLOTS`].
        slot: u8,
    },
    /// Emit the marked value of `slot`, zero-padded to `width` decimal bytes.
    Emit {
        /// Slot index marked by an earlier [`PackItem::Mark`].
        slot: u8,
        /// Exact output width in bytes; `1..=20`.
        width: u8,
    },
}

/// Opcode byte for [`Op::EmitObject`].
pub const OP_EMIT_OBJECT: u8 = 0x01;
/// Opcode byte for [`Op::Inline`].
pub const OP_INLINE: u8 = 0x02;
/// Opcode byte for [`Op::RepeatLast`].
pub const OP_REPEAT_LAST: u8 = 0x03;
/// Opcode byte for [`Op::DecodeChannel`].
pub const OP_DECODE_CHANNEL: u8 = 0x04;
/// Opcode byte for [`Op::InterleaveChannels`].
pub const OP_INTERLEAVE_CHANNELS: u8 = 0x05;
/// Opcode byte for [`Op::MarkOffset`].
pub const OP_MARK_OFFSET: u8 = 0x06;
/// Opcode byte for [`Op::EmitOffset`].
pub const OP_EMIT_OFFSET: u8 = 0x07;
/// Opcode byte for [`Op::PackSegments`].
pub const OP_PACK_SEGMENTS: u8 = 0x08;
/// Opcode byte for [`Op::PackedChannels`].
pub const OP_PACKED_CHANNELS: u8 = 0x09;
/// Opcode byte for [`Op::DeflateReplay`].
pub const OP_DEFLATE_REPLAY: u8 = 0x0A;
/// [`Op::DeflateReplay`] plaintext source: the object table.
pub const DEFLATE_SOURCE_OBJECT: u8 = 0;
/// [`Op::DeflateReplay`] plaintext source: the entropy channel table.
pub const DEFLATE_SOURCE_CHANNEL: u8 = 1;
/// Replay-codec identity for exact DEFLATE replay: `preflate` 0.7.6 semantics.
///
/// This names an **experimental**, version-coupled decoder contract, not a frozen
/// archival standard: the correction blob is `preflate`'s private
/// bitcode+CABAC layout. A future VOLE-owned implementation would be a new codec
/// id (and a new universe).
pub const REPLAY_DEFLATE_PREFLATE_0_7_6: u8 = 1;

/// Item tag for [`PackItem::Literal`].
const PACK_ITEM_LITERAL: u8 = 0x01;
/// Item tag for [`PackItem::Mark`].
const PACK_ITEM_MARK: u8 = 0x02;
/// Item tag for [`PackItem::Emit`].
const PACK_ITEM_EMIT: u8 = 0x03;

impl Op {
    /// Encode this instruction into `out`.
    pub fn encode(&self, out: &mut Vec<u8>) -> Result<()> {
        match self {
            Op::EmitObject { object_id } => {
                out.push(OP_EMIT_OBJECT);
                out.extend_from_slice(&object_id.to_le_bytes());
            }
            Op::Inline { bytes } => {
                let len = u32::try_from(bytes.len())
                    .map_err(|_| Error::resource_limit("inline literal exceeds 4 GiB"))?;
                out.push(OP_INLINE);
                out.extend_from_slice(&len.to_le_bytes());
                out.extend_from_slice(bytes);
            }
            Op::RepeatLast { count } => {
                out.push(OP_REPEAT_LAST);
                out.extend_from_slice(&count.to_le_bytes());
            }
            Op::DecodeChannel { channel_id } => {
                out.push(OP_DECODE_CHANNEL);
                out.extend_from_slice(&channel_id.to_le_bytes());
            }
            Op::InterleaveChannels {
                kinds_channel,
                lengths_channel,
                first_payload_channel,
                payload_channel_count,
            } => {
                out.push(OP_INTERLEAVE_CHANNELS);
                out.extend_from_slice(&kinds_channel.to_le_bytes());
                out.extend_from_slice(&lengths_channel.to_le_bytes());
                out.extend_from_slice(&first_payload_channel.to_le_bytes());
                out.push(*payload_channel_count);
            }
            Op::MarkOffset { slot } => {
                out.push(OP_MARK_OFFSET);
                out.push(*slot);
            }
            Op::EmitOffset { slot, width } => {
                out.push(OP_EMIT_OFFSET);
                out.push(*slot);
                out.push(*width);
            }
            Op::PackSegments { data_object, items } => {
                out.push(OP_PACK_SEGMENTS);
                out.extend_from_slice(&data_object.to_le_bytes());
                write_item_table(items, out)?;
            }
            Op::PackedChannels {
                data_channel,
                plan_channel,
                declared_output_len,
            } => {
                out.push(OP_PACKED_CHANNELS);
                out.extend_from_slice(&data_channel.to_le_bytes());
                out.extend_from_slice(&plan_channel.to_le_bytes());
                out.extend_from_slice(&declared_output_len.to_le_bytes());
            }
            Op::DeflateReplay {
                replay_codec,
                source_kind,
                source_id,
                corrections_object,
                declared_output_len,
            } => {
                out.push(OP_DEFLATE_REPLAY);
                out.push(*replay_codec);
                out.push(*source_kind);
                out.extend_from_slice(&source_id.to_le_bytes());
                out.extend_from_slice(&corrections_object.to_le_bytes());
                out.extend_from_slice(&declared_output_len.to_le_bytes());
            }
        }
        Ok(())
    }

    /// Decode one instruction from `data[*pos..]`, advancing `*pos`.
    pub fn decode(data: &[u8], pos: &mut usize, limits: Limits) -> Result<Op> {
        let tag = *data
            .get(*pos)
            .ok_or_else(|| Error::invalid_graph("truncated instruction opcode"))?;
        *pos += 1;
        match tag {
            OP_EMIT_OBJECT => {
                let id = read_u32(data, pos)?;
                Ok(Op::EmitObject { object_id: id })
            }
            OP_INLINE => {
                let len = read_u32(data, pos)?;
                if len > limits.max_record_len {
                    return Err(Error::resource_limit(
                        "inline literal length exceeds record limit",
                    ));
                }
                let end = pos
                    .checked_add(len as usize)
                    .ok_or_else(|| Error::invalid_graph("inline length overflow"))?;
                if end > data.len() {
                    return Err(Error::invalid_graph("truncated inline literal"));
                }
                let bytes = data[*pos..end].to_vec();
                *pos = end;
                Ok(Op::Inline { bytes })
            }
            OP_REPEAT_LAST => {
                let count = read_u32(data, pos)?;
                Ok(Op::RepeatLast { count })
            }
            OP_DECODE_CHANNEL => {
                let id = read_u32(data, pos)?;
                Ok(Op::DecodeChannel { channel_id: id })
            }
            OP_INTERLEAVE_CHANNELS => {
                let kinds_channel = read_u32(data, pos)?;
                let lengths_channel = read_u32(data, pos)?;
                let first_payload_channel = read_u32(data, pos)?;
                let payload_channel_count = *data
                    .get(*pos)
                    .ok_or_else(|| Error::invalid_graph("truncated instruction operand"))?;
                *pos += 1;
                Ok(Op::InterleaveChannels {
                    kinds_channel,
                    lengths_channel,
                    first_payload_channel,
                    payload_channel_count,
                })
            }
            OP_MARK_OFFSET => {
                let slot = read_u8(data, pos)?;
                Ok(Op::MarkOffset { slot })
            }
            OP_EMIT_OFFSET => {
                let slot = read_u8(data, pos)?;
                let width = read_u8(data, pos)?;
                Ok(Op::EmitOffset { slot, width })
            }
            OP_PACK_SEGMENTS => {
                let data_object = read_u32(data, pos)?;
                let item_count = read_u32(data, pos)?;
                let items = read_items(data, pos, item_count, limits)?;
                Ok(Op::PackSegments { data_object, items })
            }
            OP_PACKED_CHANNELS => {
                let data_channel = read_u32(data, pos)?;
                let plan_channel = read_u32(data, pos)?;
                let declared_output_len = read_u64(data, pos)?;
                Ok(Op::PackedChannels {
                    data_channel,
                    plan_channel,
                    declared_output_len,
                })
            }
            OP_DEFLATE_REPLAY => {
                let replay_codec = read_u8(data, pos)?;
                if replay_codec != REPLAY_DEFLATE_PREFLATE_0_7_6 {
                    return Err(Error::unsupported_feature(format!(
                        "DEFLATE_REPLAY codec {replay_codec} is not the supported preflate-0.7.6 semantics"
                    )));
                }
                let source_kind = read_u8(data, pos)?;
                if source_kind != DEFLATE_SOURCE_OBJECT && source_kind != DEFLATE_SOURCE_CHANNEL {
                    return Err(Error::invalid_graph(format!(
                        "DEFLATE_REPLAY source kind {source_kind} is not 0 (object) or 1 (channel)"
                    )));
                }
                let source_id = read_u32(data, pos)?;
                let corrections_object = read_u32(data, pos)?;
                let declared_output_len = read_u32(data, pos)?;
                Ok(Op::DeflateReplay {
                    replay_codec,
                    source_kind,
                    source_id,
                    corrections_object,
                    declared_output_len,
                })
            }
            other => Err(Error::invalid_graph(format!(
                "unknown DRA opcode {other:#04x}"
            ))),
        }
    }
}

/// Encode `items` into the shared packed-item wire form
/// `[item_count u32 LE][items...]`.
///
/// This is the canonical codec used by both [`Op::PackSegments`] (inline in the
/// graph record) and [`Op::PackedChannels`] (carried in a plan entropy channel),
/// so the two never diverge.
pub fn encode_items(items: &[PackItem]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(4 + items.len() * 3);
    write_item_table(items, &mut out)?;
    Ok(out)
}

/// Decode a complete `[item_count u32 LE][items...]` table from `bytes`.
///
/// Bounded: rejects truncation, unknown item tags, a count above
/// [`Limits::max_graph_ops`], and trailing bytes past the table.
pub fn decode_items(bytes: &[u8], limits: Limits) -> Result<Vec<PackItem>> {
    if bytes.len() < 4 {
        return Err(Error::invalid_graph("truncated packed item table header"));
    }
    let count = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let mut pos = 4usize;
    let items = read_items(bytes, &mut pos, count, limits)?;
    if pos != bytes.len() {
        return Err(Error::invalid_graph(format!(
            "packed item table has {} trailing bytes",
            bytes.len() - pos
        )));
    }
    Ok(items)
}

/// Append the full item table (`[item_count u32 LE][items...]`) to `out`.
fn write_item_table(items: &[PackItem], out: &mut Vec<u8>) -> Result<()> {
    let count = u32::try_from(items.len())
        .map_err(|_| Error::resource_limit("packed item table exceeds 4 GiB"))?;
    out.extend_from_slice(&count.to_le_bytes());
    write_items_into(items, out)
}

/// Append the item list (without the count prefix) to `out`.
fn write_items_into(items: &[PackItem], out: &mut Vec<u8>) -> Result<()> {
    for item in items {
        match item {
            PackItem::Literal { len } => {
                out.push(PACK_ITEM_LITERAL);
                write_leb128_u32(*len, out);
            }
            PackItem::Mark { slot } => {
                out.push(PACK_ITEM_MARK);
                out.push(*slot);
            }
            PackItem::Emit { slot, width } => {
                out.push(PACK_ITEM_EMIT);
                out.push(*slot);
                out.push(*width);
            }
        }
    }
    Ok(())
}

/// Decode `count` items from `data[*pos..]`, advancing `*pos`. Shared by the
/// inline [`Op::PackSegments`] table and [`decode_items`].
fn read_items(data: &[u8], pos: &mut usize, count: u32, limits: Limits) -> Result<Vec<PackItem>> {
    if count > limits.max_graph_ops {
        return Err(Error::resource_limit(format!(
            "packed item count {count} exceeds limit {}",
            limits.max_graph_ops
        )));
    }
    let mut items = Vec::with_capacity(count.min(4096) as usize);
    for _ in 0..count {
        let tag = *data
            .get(*pos)
            .ok_or_else(|| Error::invalid_graph("truncated packed item"))?;
        *pos += 1;
        match tag {
            PACK_ITEM_LITERAL => {
                let len = read_leb128_u32(data, pos)?;
                items.push(PackItem::Literal { len });
            }
            PACK_ITEM_MARK => {
                let slot = read_u8(data, pos)?;
                items.push(PackItem::Mark { slot });
            }
            PACK_ITEM_EMIT => {
                let slot = read_u8(data, pos)?;
                let width = read_u8(data, pos)?;
                items.push(PackItem::Emit { slot, width });
            }
            other => {
                return Err(Error::invalid_graph(format!(
                    "unknown packed item tag {other:#04x}"
                )));
            }
        }
    }
    Ok(items)
}

fn read_u8(data: &[u8], pos: &mut usize) -> Result<u8> {
    let v = *data
        .get(*pos)
        .ok_or_else(|| Error::invalid_graph("truncated instruction operand"))?;
    *pos += 1;
    Ok(v)
}

/// Append `value` to `out` as a minimal unsigned LEB128 varint (at most 5 bytes).
fn write_leb128_u32(value: u32, out: &mut Vec<u8>) {
    let mut v = value;
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// Read an unsigned LEB128 varint bounded to `u32` from `data[*pos..]`, advancing
/// `*pos`. Rejects truncation, non-minimal overflow, and any encoding that would
/// exceed `u32` (more than five bytes).
fn read_leb128_u32(data: &[u8], pos: &mut usize) -> Result<u32> {
    let mut result: u32 = 0;
    for shift in [0u32, 7, 14, 21, 28] {
        let byte = *data
            .get(*pos)
            .ok_or_else(|| Error::invalid_graph("truncated LEB128 operand"))?;
        *pos += 1;
        let low = u32::from(byte & 0x7f);
        if shift == 28 && low > 0x0f {
            return Err(Error::invalid_graph("LEB128 varint overflows u32"));
        }
        result |= low << shift;
        if byte & 0x80 == 0 {
            return Ok(result);
        }
    }
    Err(Error::invalid_graph("LEB128 varint overflows u32"))
}

fn read_u64(data: &[u8], pos: &mut usize) -> Result<u64> {
    let end = pos
        .checked_add(8)
        .ok_or_else(|| Error::invalid_graph("operand offset overflow"))?;
    if end > data.len() {
        return Err(Error::invalid_graph("truncated instruction operand"));
    }
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&data[*pos..end]);
    *pos = end;
    Ok(u64::from_le_bytes(buf))
}

fn read_u32(data: &[u8], pos: &mut usize) -> Result<u32> {
    let end = pos
        .checked_add(4)
        .ok_or_else(|| Error::invalid_graph("operand offset overflow"))?;
    if end > data.len() {
        return Err(Error::invalid_graph("truncated instruction operand"));
    }
    let v = u32::from_le_bytes([data[*pos], data[*pos + 1], data[*pos + 2], data[*pos + 3]]);
    *pos = end;
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(op: Op) {
        let mut buf = Vec::new();
        op.encode(&mut buf).unwrap();
        let mut pos = 0;
        let back = Op::decode(&buf, &mut pos, Limits::DEFAULT).unwrap();
        assert_eq!(op, back);
        assert_eq!(pos, buf.len());
    }

    #[test]
    fn op_roundtrips() {
        roundtrip(Op::EmitObject { object_id: 7 });
        roundtrip(Op::Inline {
            bytes: vec![1, 2, 3, 4, 5],
        });
        roundtrip(Op::RepeatLast { count: 1_000_000 });
        roundtrip(Op::Inline { bytes: Vec::new() });
        roundtrip(Op::DecodeChannel { channel_id: 3 });
        roundtrip(Op::InterleaveChannels {
            kinds_channel: 0,
            lengths_channel: 1,
            first_payload_channel: 2,
            payload_channel_count: 3,
        });
        roundtrip(Op::MarkOffset { slot: 0 });
        roundtrip(Op::MarkOffset { slot: 15 });
        roundtrip(Op::EmitOffset { slot: 0, width: 1 });
        roundtrip(Op::EmitOffset { slot: 7, width: 20 });
        roundtrip(Op::PackSegments {
            data_object: 2,
            items: vec![
                PackItem::Literal { len: 3 },
                PackItem::Mark { slot: 0 },
                PackItem::Literal { len: 300 },
                PackItem::Emit { slot: 0, width: 3 },
            ],
        });
        roundtrip(Op::PackedChannels {
            data_channel: 0,
            plan_channel: 1,
            declared_output_len: 9,
        });
        roundtrip(Op::PackedChannels {
            data_channel: 3,
            plan_channel: 4,
            declared_output_len: u64::MAX,
        });
        roundtrip(Op::DeflateReplay {
            replay_codec: REPLAY_DEFLATE_PREFLATE_0_7_6,
            source_kind: DEFLATE_SOURCE_OBJECT,
            source_id: 0,
            corrections_object: 1,
            declared_output_len: 1234,
        });
        roundtrip(Op::DeflateReplay {
            replay_codec: REPLAY_DEFLATE_PREFLATE_0_7_6,
            source_kind: DEFLATE_SOURCE_CHANNEL,
            source_id: u32::MAX,
            corrections_object: u32::MAX,
            declared_output_len: u32::MAX,
        });
    }

    #[test]
    fn rejects_unknown_replay_codec_as_unsupported_feature() {
        // A graph record whose DEFLATE_REPLAY op names a replay codec this build
        // does not implement must fail as `UnsupportedFeature`, never be
        // silently rerun with the wrong (preflate 0.7.6) semantics (FIX2).
        let data = [OP_DEFLATE_REPLAY, 2];
        let mut pos = 0;
        let e = Op::decode(&data, &mut pos, Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::UnsupportedFeature);
    }

    #[test]
    fn rejects_unknown_opcode() {
        let data = [0xEEu8];
        let mut pos = 0;
        let e = Op::decode(&data, &mut pos, Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::InvalidGraph);
    }

    #[test]
    fn rejects_truncated_operand() {
        let data = [OP_EMIT_OBJECT, 0x01, 0x02];
        let mut pos = 0;
        let e = Op::decode(&data, &mut pos, Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::InvalidGraph);
    }
}
