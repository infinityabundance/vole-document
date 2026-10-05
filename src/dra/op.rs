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
}

/// Opcode byte for [`Op::EmitObject`].
pub const OP_EMIT_OBJECT: u8 = 0x01;
/// Opcode byte for [`Op::Inline`].
pub const OP_INLINE: u8 = 0x02;
/// Opcode byte for [`Op::RepeatLast`].
pub const OP_REPEAT_LAST: u8 = 0x03;

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
            other => Err(Error::invalid_graph(format!(
                "unknown DRA opcode {other:#04x}"
            ))),
        }
    }
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
