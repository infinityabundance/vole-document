//! The optional `CHECKPOINT` record: a bounded, advisory per-op output table.
//!
//! A checkpoint is a self-describing, little-endian, versioned payload written
//! with [`crate::container::record::FLAG_OPTIONAL`] into a seekable descriptor
//! (one that also carries an `OBSERVATION_INDEX` and a seek `DIRECTORY`, which
//! locates it). It records, for every reconstruction op, that op's absolute
//! output start offset and length — a random-access boundary table a planner can
//! consume to select an op window **without** reading the `OBSERVATION_INDEX`
//! record.
//!
//! ```text
//! checkpoint_v1 :=
//!     version:u8 = 1
//!     kind:u8 = 1                # OP_BOUNDARIES
//!     reserved:u16 = 0
//!     entry_count:u32 LE
//!     source_len:u64 LE
//!     graph_crc32c:u32 LE        # CRC-32C of the GRAPH record payload
//!     entry[entry_count]: out_start:u64 LE | out_len:u64 LE
//! ```
//!
//! The checkpoint is **advisory, never authority** (mirroring the
//! `DIRECTORY`/`OBSERVATION_INDEX` discipline of ADR-0019). [`Self::validate`]
//! re-derives every length from the authoritative reconstruction program with
//! [`crate::dra::Program::analyze_ops`] and rejects any contradiction, so a
//! checkpoint can never change a served byte. A lying, corrupt, oversized, or
//! out-of-closure checkpoint is rejected: the full parser fails closed, and the
//! seek reader ignores it and falls back to the Phase-8 index lane.
//!
//! Unknown version, kind, or a non-zero reserved field fail closed. All
//! arithmetic is checked.

use crate::dra::Program;
use crate::error::{Error, Result};
use crate::integrity::crc32c;
use crate::limits::Limits;

/// Wire version of the checkpoint payload implemented by this build.
pub const CHECKPOINT_VERSION: u8 = 1;

/// Checkpoint kind: a per-op output boundary table.
pub const CHECKPOINT_KIND_OP_BOUNDARIES: u8 = 1;

/// The only checkpoint kind this build understands; any other kind fails closed.
pub const CHECKPOINT_KNOWN_KINDS: u8 = CHECKPOINT_KIND_OP_BOUNDARIES;

/// Encoded size of the fixed checkpoint header.
pub const CHECKPOINT_HEADER_LEN: usize = 1 + 1 + 2 + 4 + 8 + 4;
/// Encoded size of one boundary entry: `out_start u64 | out_len u64`.
pub const CHECKPOINT_ENTRY_LEN: usize = 16;

/// One op's absolute output boundary: where the op's output begins and how many
/// bytes it produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckpointEntry {
    /// Absolute output offset at which this op's bytes begin.
    pub out_start: u64,
    /// Bytes this op contributes to the output.
    pub out_len: u64,
}

/// A decoded `CHECKPOINT` record payload.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CheckpointTable {
    /// Checkpoint kind (only [`CHECKPOINT_KIND_OP_BOUNDARIES`] is valid here).
    pub kind: u8,
    /// Declared total output length (must equal the source length).
    pub source_len: u64,
    /// CRC-32C of the `GRAPH` payload this checkpoint describes.
    pub graph_crc32c: u32,
    /// Per-op boundaries, in program order.
    pub entries: Vec<CheckpointEntry>,
}

impl CheckpointTable {
    /// Build an OP_BOUNDARIES checkpoint from the authoritative program.
    ///
    /// Per-op lengths come from [`Program::analyze_ops`]; the boundaries are their
    /// running sum. `graph_crc32c` is the CRC-32C of `program.encode()` — the
    /// exact `GRAPH` payload the checkpoint is bound to. No allocation is sized by
    /// an untrusted count: the vector is bounded by [`Limits::max_graph_ops`].
    pub fn from_program(
        program: &Program,
        object_lens: &[u64],
        channel_lens: &[u64],
        source_len: u64,
        limits: Limits,
    ) -> Result<CheckpointTable> {
        if program.ops.len() as u64 > u64::from(limits.max_graph_ops) {
            return Err(Error::resource_limit(
                "checkpoint entry count exceeds the graph-op limit",
            ));
        }
        let per_op = program.analyze_ops(object_lens, channel_lens, limits)?;
        if per_op.len() != program.ops.len() {
            return Err(Error::internal_invariant(
                "analyze_ops returned a different op count",
            ));
        }
        let mut entries = Vec::with_capacity(per_op.len());
        let mut acc: u64 = 0;
        for &len in &per_op {
            entries.push(CheckpointEntry {
                out_start: acc,
                out_len: len,
            });
            acc = acc
                .checked_add(len)
                .ok_or_else(|| Error::invalid_graph("checkpoint output length overflow"))?;
        }
        if acc != source_len {
            return Err(Error::coverage_violation(format!(
                "checkpoint boundaries sum to {acc} but the source is {source_len} bytes"
            )));
        }
        let graph_crc32c = crc32c(&program.encode()?);
        Ok(CheckpointTable {
            kind: CHECKPOINT_KIND_OP_BOUNDARIES,
            source_len,
            graph_crc32c,
            entries,
        })
    }

    /// The per-op output lengths, in program order.
    pub fn lengths(&self) -> Vec<u64> {
        self.entries.iter().map(|e| e.out_len).collect()
    }

    /// Encode the payload to canonical bytes.
    pub fn encode(&self) -> Result<Vec<u8>> {
        if self.kind != CHECKPOINT_KIND_OP_BOUNDARIES {
            return Err(Error::invalid_container("checkpoint has an unknown kind"));
        }
        let count = u32::try_from(self.entries.len())
            .map_err(|_| Error::resource_limit("checkpoint entry_count exceeds u32"))?;
        let mut out =
            Vec::with_capacity(CHECKPOINT_HEADER_LEN + self.entries.len() * CHECKPOINT_ENTRY_LEN);
        out.push(CHECKPOINT_VERSION);
        out.push(self.kind);
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&self.source_len.to_le_bytes());
        out.extend_from_slice(&self.graph_crc32c.to_le_bytes());
        for e in &self.entries {
            out.extend_from_slice(&e.out_start.to_le_bytes());
            out.extend_from_slice(&e.out_len.to_le_bytes());
        }
        Ok(out)
    }

    /// Decode and structurally bound a payload.
    ///
    /// This checks the size bound, the version, the kind, the reserved field, and
    /// that the declared entry count fits the payload. It does **not** check
    /// agreement with a program; call [`CheckpointTable::validate`] for that.
    pub fn decode(bytes: &[u8], limits: Limits) -> Result<CheckpointTable> {
        if bytes.len() as u64 > u64::from(limits.max_checkpoint_bytes) {
            return Err(Error::resource_limit(format!(
                "checkpoint payload {} exceeds limit {}",
                bytes.len(),
                limits.max_checkpoint_bytes
            )));
        }
        if bytes.len() < CHECKPOINT_HEADER_LEN {
            return Err(Error::invalid_container("truncated CHECKPOINT header"));
        }
        let version = bytes[0];
        if version != CHECKPOINT_VERSION {
            return Err(Error::unsupported_version(format!(
                "checkpoint version {version} is not supported"
            )));
        }
        let kind = bytes[1];
        if kind & !CHECKPOINT_KNOWN_KINDS != 0 {
            return Err(Error::invalid_container("CHECKPOINT has an unknown kind"));
        }
        let reserved = u16::from_le_bytes([bytes[2], bytes[3]]);
        if reserved != 0 {
            return Err(Error::invalid_container(
                "CHECKPOINT reserved field must be zero",
            ));
        }
        let count = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        if count > limits.max_graph_ops {
            return Err(Error::resource_limit(format!(
                "checkpoint entry_count {count} exceeds limit {}",
                limits.max_graph_ops
            )));
        }
        let source_len = u64::from_le_bytes([
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        ]);
        let graph_crc32c = u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);

        let count = count as usize;
        let need = count
            .checked_mul(CHECKPOINT_ENTRY_LEN)
            .ok_or_else(|| Error::invalid_container("CHECKPOINT section length overflow"))?;
        let available = bytes.len() - CHECKPOINT_HEADER_LEN;
        if need != available {
            return Err(Error::invalid_container(format!(
                "CHECKPOINT declares {count} entries ({need} bytes) but {} bytes follow",
                available
            )));
        }
        let mut entries = Vec::with_capacity(count);
        let mut p = CHECKPOINT_HEADER_LEN;
        for _ in 0..count {
            let out_start = u64::from_le_bytes([
                bytes[p],
                bytes[p + 1],
                bytes[p + 2],
                bytes[p + 3],
                bytes[p + 4],
                bytes[p + 5],
                bytes[p + 6],
                bytes[p + 7],
            ]);
            let out_len = u64::from_le_bytes([
                bytes[p + 8],
                bytes[p + 9],
                bytes[p + 10],
                bytes[p + 11],
                bytes[p + 12],
                bytes[p + 13],
                bytes[p + 14],
                bytes[p + 15],
            ]);
            p += CHECKPOINT_ENTRY_LEN;
            entries.push(CheckpointEntry { out_start, out_len });
        }
        Ok(CheckpointTable {
            kind,
            source_len,
            graph_crc32c,
            entries,
        })
    }

    /// Validate the checkpoint against the authoritative program and graph.
    ///
    /// Requires: the declared kind is OP_BOUNDARIES; `graph_payload`'s CRC-32C
    /// equals the checkpoint's binding; the declared `source_len` matches; the
    /// entry count matches the program; the entries are contiguous and cover the
    /// source exactly; and every entry's `out_len` equals the program's own
    /// [`Program::analyze_ops`] view. Any disagreement is a typed rejection — the
    /// checkpoint is never trusted.
    pub fn validate(
        &self,
        program: &Program,
        graph_payload: &[u8],
        source_len: u64,
        object_lens: &[u64],
        channel_lens: &[u64],
        limits: Limits,
    ) -> Result<()> {
        if self.kind != CHECKPOINT_KIND_OP_BOUNDARIES {
            return Err(Error::invalid_container("checkpoint has an unknown kind"));
        }
        if self.graph_crc32c != crc32c(graph_payload) {
            return Err(Error::invalid_container(
                "checkpoint GRAPH-CRC binding disagrees with the GRAPH record",
            ));
        }
        if self.source_len != source_len {
            return Err(Error::integrity_mismatch(format!(
                "checkpoint declares source length {} but the descriptor declares {source_len}",
                self.source_len
            )));
        }
        if self.entries.len() != program.ops.len() {
            return Err(Error::invalid_container(format!(
                "checkpoint lists {} boundaries but the program has {} ops",
                self.entries.len(),
                program.ops.len()
            )));
        }
        // Contiguity and exact coverage of the source.
        let mut acc: u64 = 0;
        for (i, e) in self.entries.iter().enumerate() {
            if e.out_start != acc {
                return Err(Error::invalid_container(format!(
                    "checkpoint boundary {i} starts at {} but {acc} was expected",
                    e.out_start
                )));
            }
            acc = acc
                .checked_add(e.out_len)
                .ok_or_else(|| Error::invalid_container("checkpoint output length overflow"))?;
        }
        if acc != source_len {
            return Err(Error::coverage_violation(format!(
                "checkpoint boundaries sum to {acc} but the source is {source_len} bytes"
            )));
        }
        // The program is authority: re-derive every op length and reject any lie.
        let per_op = program.analyze_ops(object_lens, channel_lens, limits)?;
        if per_op.len() != self.entries.len() {
            return Err(Error::invalid_container(
                "checkpoint entry count disagrees with analyze_ops",
            ));
        }
        for (i, (e, want)) in self.entries.iter().zip(&per_op).enumerate() {
            if e.out_len != *want {
                return Err(Error::coverage_violation(format!(
                    "checkpoint boundary {i} claims {} bytes but the program produces {want}",
                    e.out_len
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dra::Op;

    fn tiny_program() -> Program {
        Program::new(vec![
            Op::EmitObject { object_id: 0 },
            Op::Inline {
                bytes: b"de".to_vec(),
            },
        ])
    }

    fn tiny_table() -> CheckpointTable {
        CheckpointTable::from_program(&tiny_program(), &[3], &[], 5, Limits::DEFAULT).unwrap()
    }

    #[test]
    fn from_program_builds_contiguous_boundaries() {
        let t = tiny_table();
        assert_eq!(t.kind, CHECKPOINT_KIND_OP_BOUNDARIES);
        assert_eq!(t.source_len, 5);
        assert_eq!(t.lengths(), vec![3, 2]);
        assert_eq!(t.entries[0].out_start, 0);
        assert_eq!(t.entries[1].out_start, 3);
    }

    #[test]
    fn roundtrip() {
        let t = tiny_table();
        let bytes = t.encode().unwrap();
        assert_eq!(
            bytes.len(),
            CHECKPOINT_HEADER_LEN + 2 * CHECKPOINT_ENTRY_LEN
        );
        let back = CheckpointTable::decode(&bytes, Limits::DEFAULT).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn validate_accepts_an_honest_checkpoint() {
        let program = tiny_program();
        let t = tiny_table();
        t.validate(
            &program,
            &program.encode().unwrap(),
            5,
            &[3],
            &[],
            Limits::DEFAULT,
        )
        .unwrap();
    }

    #[test]
    fn validate_rejects_a_wrong_length() {
        let program = tiny_program();
        let mut t = tiny_table();
        t.entries[1].out_len = 3;
        t.entries[1].out_start = 3;
        t.source_len = 6;
        // Contiguity/coverage hold internally, but the program disagrees.
        assert_eq!(
            t.validate(
                &program,
                &program.encode().unwrap(),
                6,
                &[3],
                &[],
                Limits::DEFAULT
            )
            .unwrap_err()
            .class(),
            crate::ErrorClass::CoverageViolation
        );
    }

    #[test]
    fn validate_rejects_a_foreign_graph_binding() {
        let program = tiny_program();
        let t = tiny_table();
        assert_eq!(
            t.validate(&program, b"not the graph", 5, &[3], &[], Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidContainer
        );
    }

    #[test]
    fn unknown_version_and_kind_fail_closed() {
        let mut bytes = tiny_table().encode().unwrap();
        bytes[0] = 9;
        assert_eq!(
            CheckpointTable::decode(&bytes, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::UnsupportedVersion
        );
        let mut bytes = tiny_table().encode().unwrap();
        bytes[1] = 0x80;
        assert_eq!(
            CheckpointTable::decode(&bytes, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidContainer
        );
    }

    #[test]
    fn truncation_and_over_length_are_rejected() {
        let bytes = tiny_table().encode().unwrap();
        assert_eq!(
            CheckpointTable::decode(&bytes[..bytes.len() - 1], Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidContainer
        );
        let limits = Limits {
            max_checkpoint_bytes: 8,
            ..Limits::DEFAULT
        };
        assert_eq!(
            CheckpointTable::decode(&bytes, limits).unwrap_err().class(),
            crate::ErrorClass::ResourceLimit
        );
    }
}
