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
                let count = u32::try_from(items.len())
                    .map_err(|_| Error::resource_limit("packed item table exceeds 4 GiB"))?;
                out.push(OP_PACK_SEGMENTS);
                out.extend_from_slice(&data_object.to_le_bytes());
                out.extend_from_slice(&count.to_le_bytes());
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
                if item_count > limits.max_graph_ops {
                    return Err(Error::resource_limit(format!(
                        "packed item count {item_count} exceeds limit {}",
                        limits.max_graph_ops
                    )));
                }
                let mut items = Vec::with_capacity(item_count.min(4096) as usize);
                for _ in 0..item_count {
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
                Ok(Op::PackSegments { data_object, items })
            }
            other => Err(Error::invalid_graph(format!(
                "unknown DRA opcode {other:#04x}"
            ))),
        }
    }
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
