//! Optional `OBSERVATION_INDEX` record (Phase 7.3).
//!
//! The index is an **advisory, checked** map from reconstruction output to the
//! instructions, PDF selectors, and block digests that produced it. It exists to
//! let a later partial-decode lane skip work, but it is **never authority**: every
//! entry is re-derived from the program at parse time and any disagreement is
//! rejected. A decoder that ignores the record still materializes the source
//! byte-for-byte, because the reconstruction program alone is complete.
//!
//! ## Wire payload (`observation_index_v1`)
//!
//! ```text
//! observation_index_v1 :=
//!     version:u8 = 1
//!     section_flags:u8            # bit0 OP_TABLE, bit1 PDF_SELECTORS, bit2 DIGESTS
//!     # if bit0 (OP_TABLE):
//!     op_count:u32 LE
//!     op_entry[op_count]        # ascending op index
//!     # if bit1 (PDF_SELECTORS):
//!     selector_count:u32 LE
//!     selector[selector_count]  # ascending (kind, number, generation, out_off)
//!     # if bit2 (DIGESTS):
//!     digest_count:u32 LE
//!     digest[digest_count]      # ascending out_off
//!
//! op_entry := out_len:u32 LE | dep_kind:u8 | dep_id:u32 LE
//! selector := kind:u8 | number:u32 LE | generation:u32 LE | out_off:u64 LE | out_len:u64 LE
//! digest   := out_off:u64 LE | out_len:u64 LE | sha256:[u8;32]
//! ```
//!
//! All integers are little-endian. Sections are independent: the encoder may
//! omit any section that does not pay for itself. An unknown `section_flags` bit
//! or an unknown `version` fails closed.

use crate::dra::Program;
use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the observation-index payload.
pub const OBSERVATION_INDEX_VERSION: u8 = 1;

/// Section flag: the op table (bit 0) is present.
pub const SECTION_OP_TABLE: u8 = 0x01;
/// Section flag: PDF selectors (bit 1) are present.
pub const SECTION_PDF_SELECTORS: u8 = 0x02;
/// Section flag: output-block digests (bit 2) are present.
pub const SECTION_DIGESTS: u8 = 0x04;

/// The set of section flags this version understands. Any other bit fails closed.
const KNOWN_SECTION_FLAGS: u8 = SECTION_OP_TABLE | SECTION_PDF_SELECTORS | SECTION_DIGESTS;

/// Dependency kind: no dependency.
pub const DEP_NONE: u8 = 0;
/// Dependency kind: an entry in the descriptor's object table.
pub const DEP_OBJECT: u8 = 1;
/// Dependency kind: an entry in the descriptor's entropy-channel table.
pub const DEP_CHANNEL: u8 = 2;

/// Selector kind: an indirect object.
pub const SELECTOR_OBJECT: u8 = 0;
/// Selector kind: an encoded stream.
pub const SELECTOR_STREAM: u8 = 1;
/// Selector kind: a revision.
pub const SELECTOR_REVISION: u8 = 2;

const OP_ENTRY_LEN: usize = 4 + 1 + 4;
const SELECTOR_ENTRY_LEN: usize = 1 + 4 + 4 + 8 + 8;
const DIGEST_ENTRY_LEN: usize = 8 + 8 + 32;

/// One op's entry: the exact output length it produces and its optional single
/// dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpEntry {
    /// Bytes this op contributes to the output.
    pub out_len: u32,
    /// [`DEP_NONE`], [`DEP_OBJECT`], or [`DEP_CHANNEL`].
    pub dep_kind: u8,
    /// Object or channel index when `dep_kind` names one; ignored otherwise.
    pub dep_id: u32,
}

/// A PDF-layer selector resolved to an output range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservationSelector {
    /// [`SELECTOR_OBJECT`], [`SELECTOR_STREAM`], or [`SELECTOR_REVISION`].
    pub kind: u8,
    /// Object number, or revision index for `SELECTOR_REVISION`.
    pub number: u32,
    /// Object generation (`0` for revisions).
    pub generation: u32,
    /// Start of the selected output range.
    pub out_off: u64,
    /// Length of the selected output range (non-empty).
    pub out_len: u64,
}

/// A digest of one output block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservationDigest {
    /// Start of the output block.
    pub out_off: u64,
    /// Length of the output block (non-empty).
    pub out_len: u64,
    /// SHA-256 of the output block bytes.
    pub sha256: [u8; 32],
}

/// A decoded `OBSERVATION_INDEX` record.
///
/// Field vectors correspond exactly to the sections named by `section_flags`;
/// vectors for absent sections are empty.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ObservationIndex {
    /// Section flags present in the encoded record.
    pub section_flags: u8,
    /// Per-op entries, present when [`SECTION_OP_TABLE`] is set.
    pub ops: Vec<OpEntry>,
    /// PDF selectors, present when [`SECTION_PDF_SELECTORS`] is set.
    pub selectors: Vec<ObservationSelector>,
    /// Output-block digests, present when [`SECTION_DIGESTS`] is set.
    pub digests: Vec<ObservationDigest>,
}

impl ObservationIndex {
    /// Encode the payload to canonical bytes.
    ///
    /// A section's flag and its vector must agree: a set flag with an empty
    /// vector is encoded as a count of zero (valid), but a *clear* flag with a
    /// non-empty vector is an error rather than a silently dropped section.
    pub fn encode(&self) -> Result<Vec<u8>> {
        if self.section_flags & !KNOWN_SECTION_FLAGS != 0 {
            return Err(Error::invalid_container(
                "observation index has unknown section flags",
            ));
        }
        if self.section_flags & SECTION_OP_TABLE == 0 && !self.ops.is_empty() {
            return Err(Error::invalid_container(
                "observation index carries ops without the op-table section flag",
            ));
        }
        if self.section_flags & SECTION_PDF_SELECTORS == 0 && !self.selectors.is_empty() {
            return Err(Error::invalid_container(
                "observation index carries selectors without the selector section flag",
            ));
        }
        if self.section_flags & SECTION_DIGESTS == 0 && !self.digests.is_empty() {
            return Err(Error::invalid_container(
                "observation index carries digests without the digest section flag",
            ));
        }

        let mut out = Vec::new();
        out.push(OBSERVATION_INDEX_VERSION);
        out.push(self.section_flags);

        if self.section_flags & SECTION_OP_TABLE != 0 {
            let count = u32::try_from(self.ops.len())
                .map_err(|_| Error::resource_limit("observation index op_count exceeds u32"))?;
            out.extend_from_slice(&count.to_le_bytes());
            for e in &self.ops {
                out.extend_from_slice(&e.out_len.to_le_bytes());
                out.push(e.dep_kind);
                out.extend_from_slice(&e.dep_id.to_le_bytes());
            }
        }
        if self.section_flags & SECTION_PDF_SELECTORS != 0 {
            let count = u32::try_from(self.selectors.len()).map_err(|_| {
                Error::resource_limit("observation index selector_count exceeds u32")
            })?;
            out.extend_from_slice(&count.to_le_bytes());
            for s in &self.selectors {
                out.push(s.kind);
                out.extend_from_slice(&s.number.to_le_bytes());
                out.extend_from_slice(&s.generation.to_le_bytes());
                out.extend_from_slice(&s.out_off.to_le_bytes());
                out.extend_from_slice(&s.out_len.to_le_bytes());
            }
        }
        if self.section_flags & SECTION_DIGESTS != 0 {
            let count = u32::try_from(self.digests.len())
                .map_err(|_| Error::resource_limit("observation index digest_count exceeds u32"))?;
            out.extend_from_slice(&count.to_le_bytes());
            for d in &self.digests {
                out.extend_from_slice(&d.out_off.to_le_bytes());
                out.extend_from_slice(&d.out_len.to_le_bytes());
                out.extend_from_slice(&d.sha256);
            }
        }
        Ok(out)
    }

    /// Decode and structurally validate a payload.
    ///
    /// This checks framing, version, and that every section fits its declared
    /// count. It does **not** check semantic agreement with a program; call
    /// [`ObservationIndex::validate`] with the decoded program for that.
    pub fn decode(bytes: &[u8], limits: Limits) -> Result<ObservationIndex> {
        if bytes.len() < 2 {
            return Err(Error::invalid_container(
                "truncated OBSERVATION_INDEX header",
            ));
        }
        let version = bytes[0];
        if version != OBSERVATION_INDEX_VERSION {
            return Err(Error::unsupported_version(format!(
                "observation index version {version} is not supported"
            )));
        }
        let section_flags = bytes[1];
        if section_flags & !KNOWN_SECTION_FLAGS != 0 {
            return Err(Error::invalid_container(
                "OBSERVATION_INDEX has unknown section flags",
            ));
        }

        let mut p = 2usize;
        let mut ops = Vec::new();
        if section_flags & SECTION_OP_TABLE != 0 {
            let count = read_u32(bytes, &mut p)?;
            if count > limits.max_graph_ops {
                return Err(Error::resource_limit(format!(
                    "observation index op_count {count} exceeds limit {}",
                    limits.max_graph_ops
                )));
            }
            let count = count as usize;
            require(bytes, p, count, OP_ENTRY_LEN)?;
            ops.reserve(count);
            for _ in 0..count {
                ops.push(OpEntry {
                    out_len: read_u32(bytes, &mut p)?,
                    dep_kind: read_u8(bytes, &mut p)?,
                    dep_id: read_u32(bytes, &mut p)?,
                });
            }
        }

        let mut selectors = Vec::new();
        if section_flags & SECTION_PDF_SELECTORS != 0 {
            let count = read_u32(bytes, &mut p)?;
            if count > limits.max_index_selectors {
                return Err(Error::resource_limit(format!(
                    "observation index selector_count {count} exceeds limit {}",
                    limits.max_index_selectors
                )));
            }
            let count = count as usize;
            require(bytes, p, count, SELECTOR_ENTRY_LEN)?;
            selectors.reserve(count);
            for _ in 0..count {
                selectors.push(ObservationSelector {
                    kind: read_u8(bytes, &mut p)?,
                    number: read_u32(bytes, &mut p)?,
                    generation: read_u32(bytes, &mut p)?,
                    out_off: read_u64(bytes, &mut p)?,
                    out_len: read_u64(bytes, &mut p)?,
                });
            }
        }

        let mut digests = Vec::new();
        if section_flags & SECTION_DIGESTS != 0 {
            let count = read_u32(bytes, &mut p)?;
            let count = count as usize;
            require(bytes, p, count, DIGEST_ENTRY_LEN)?;
            digests.reserve(count);
            for _ in 0..count {
                let out_off = read_u64(bytes, &mut p)?;
                let out_len = read_u64(bytes, &mut p)?;
                let mut sha256 = [0u8; 32];
                sha256.copy_from_slice(bytes.get(p..p + 32).ok_or_else(|| {
                    Error::invalid_container("truncated OBSERVATION_INDEX digest")
                })?);
                p += 32;
                digests.push(ObservationDigest {
                    out_off,
                    out_len,
                    sha256,
                });
            }
        }

        if p != bytes.len() {
            return Err(Error::invalid_container(format!(
                "OBSERVATION_INDEX has {} trailing bytes",
                bytes.len() - p
            )));
        }

        Ok(ObservationIndex {
            section_flags,
            ops,
            selectors,
            digests,
        })
    }

    /// Check the index against the program it claims to describe.
    ///
    /// The program (via [`Program::analyze_ops`]) is authoritative; the index is
    /// only ever a *claim* and must agree with it. All arithmetic is checked and
    /// every selector/digest range is bounded by the analyzed total.
    pub fn validate(
        &self,
        program: &Program,
        object_lens: &[u64],
        channel_lens: &[u64],
        limits: Limits,
    ) -> Result<()> {
        let per_op = program.analyze_ops(object_lens, channel_lens, limits)?;
        let mut total: u64 = 0;
        for len in &per_op {
            total = total
                .checked_add(*len)
                .ok_or_else(|| Error::coverage_violation("analysis output length overflow"))?;
        }

        if self.section_flags & SECTION_OP_TABLE != 0 {
            if self.ops.len() != program.ops.len() {
                return Err(Error::coverage_violation(format!(
                    "observation index op_count {} disagrees with program op count {}",
                    self.ops.len(),
                    program.ops.len()
                )));
            }
            for (i, entry) in self.ops.iter().enumerate() {
                if u64::from(entry.out_len) != per_op[i] {
                    return Err(Error::coverage_violation(format!(
                        "observation index op {i} out_len {} disagrees with analyzed {}",
                        entry.out_len, per_op[i]
                    )));
                }
                match entry.dep_kind {
                    DEP_NONE => {}
                    DEP_OBJECT => {
                        if entry.dep_id as usize >= object_lens.len() {
                            return Err(Error::coverage_violation(format!(
                                "observation index op {i} references missing object {}",
                                entry.dep_id
                            )));
                        }
                    }
                    DEP_CHANNEL => {
                        if entry.dep_id as usize >= channel_lens.len() {
                            return Err(Error::coverage_violation(format!(
                                "observation index op {i} references missing channel {}",
                                entry.dep_id
                            )));
                        }
                    }
                    other => {
                        return Err(Error::coverage_violation(format!(
                            "observation index op {i} has invalid dep_kind {other}"
                        )));
                    }
                }
            }
        }

        if self.section_flags & SECTION_PDF_SELECTORS != 0 {
            for (i, sel) in self.selectors.iter().enumerate() {
                if sel.kind > SELECTOR_REVISION {
                    return Err(Error::coverage_violation(format!(
                        "observation index selector {i} has invalid kind {}",
                        sel.kind
                    )));
                }
                if sel.out_len == 0 {
                    return Err(Error::coverage_violation(format!(
                        "observation index selector {i} has an empty range"
                    )));
                }
                let end = sel.out_off.checked_add(sel.out_len).ok_or_else(|| {
                    Error::coverage_violation(format!(
                        "observation index selector {i} range overflows"
                    ))
                })?;
                if end > total {
                    return Err(Error::coverage_violation(format!(
                        "observation index selector {i} range {}..{} exceeds total {total}",
                        sel.out_off, end
                    )));
                }
            }
        }

        if self.section_flags & SECTION_DIGESTS != 0 {
            for (i, digest) in self.digests.iter().enumerate() {
                if digest.out_len == 0 {
                    return Err(Error::coverage_violation(format!(
                        "observation index digest {i} has an empty range"
                    )));
                }
                let end = digest.out_off.checked_add(digest.out_len).ok_or_else(|| {
                    Error::coverage_violation(format!(
                        "observation index digest {i} range overflows"
                    ))
                })?;
                if end > total {
                    return Err(Error::coverage_violation(format!(
                        "observation index digest {i} range {}..{} exceeds total {total}",
                        digest.out_off, end
                    )));
                }
            }
        }

        Ok(())
    }
}

/// Ensure `count` entries of `entry_len` bytes fit in `bytes` from `p`.
fn require(bytes: &[u8], p: usize, count: usize, entry_len: usize) -> Result<()> {
    let need = count
        .checked_mul(entry_len)
        .ok_or_else(|| Error::invalid_container("OBSERVATION_INDEX section length overflow"))?;
    let available = bytes
        .len()
        .checked_sub(p)
        .ok_or_else(|| Error::invalid_container("OBSERVATION_INDEX cursor past end of payload"))?;
    if need > available {
        return Err(Error::invalid_container(format!(
            "OBSERVATION_INDEX section needs {need} bytes but only {available} remain"
        )));
    }
    Ok(())
}

fn read_u8(bytes: &[u8], p: &mut usize) -> Result<u8> {
    let v = *bytes
        .get(*p)
        .ok_or_else(|| Error::invalid_container("truncated OBSERVATION_INDEX payload"))?;
    *p += 1;
    Ok(v)
}

fn read_u32(bytes: &[u8], p: &mut usize) -> Result<u32> {
    let end = p
        .checked_add(4)
        .ok_or_else(|| Error::invalid_container("OBSERVATION_INDEX cursor overflow"))?;
    let slice = bytes
        .get(*p..end)
        .ok_or_else(|| Error::invalid_container("truncated OBSERVATION_INDEX payload"))?;
    *p = end;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_u64(bytes: &[u8], p: &mut usize) -> Result<u64> {
    let end = p
        .checked_add(8)
        .ok_or_else(|| Error::invalid_container("OBSERVATION_INDEX cursor overflow"))?;
    let slice = bytes
        .get(*p..end)
        .ok_or_else(|| Error::invalid_container("truncated OBSERVATION_INDEX payload"))?;
    *p = end;
    Ok(u64::from_le_bytes([
        slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dra::Op;

    fn simple_index() -> ObservationIndex {
        ObservationIndex {
            section_flags: SECTION_OP_TABLE,
            ops: vec![
                OpEntry {
                    out_len: 3,
                    dep_kind: DEP_OBJECT,
                    dep_id: 0,
                },
                OpEntry {
                    out_len: 0,
                    dep_kind: DEP_NONE,
                    dep_id: 0,
                },
            ],
            selectors: vec![],
            digests: vec![],
        }
    }

    #[test]
    fn op_table_roundtrips() {
        let idx = simple_index();
        let bytes = idx.encode().unwrap();
        let back = ObservationIndex::decode(&bytes, Limits::DEFAULT).unwrap();
        assert_eq!(back, idx);
    }

    #[test]
    fn all_sections_roundtrip() {
        let idx = ObservationIndex {
            section_flags: SECTION_OP_TABLE | SECTION_PDF_SELECTORS | SECTION_DIGESTS,
            ops: vec![OpEntry {
                out_len: 3,
                dep_kind: DEP_OBJECT,
                dep_id: 0,
            }],
            selectors: vec![ObservationSelector {
                kind: SELECTOR_STREAM,
                number: 4,
                generation: 0,
                out_off: 0,
                out_len: 3,
            }],
            digests: vec![ObservationDigest {
                out_off: 0,
                out_len: 3,
                sha256: [9u8; 32],
            }],
        };
        let bytes = idx.encode().unwrap();
        let back = ObservationIndex::decode(&bytes, Limits::DEFAULT).unwrap();
        assert_eq!(back, idx);
    }

    #[test]
    fn unknown_version_and_flags_fail_closed() {
        let mut bytes = simple_index().encode().unwrap();
        bytes[0] = 2;
        assert_eq!(
            ObservationIndex::decode(&bytes, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::UnsupportedVersion
        );

        let mut bytes = simple_index().encode().unwrap();
        bytes[1] = 0x80;
        assert_eq!(
            ObservationIndex::decode(&bytes, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidContainer
        );
    }

    #[test]
    fn truncated_section_is_rejected() {
        let mut bytes = simple_index().encode().unwrap();
        bytes.truncate(bytes.len() - 1);
        assert_eq!(
            ObservationIndex::decode(&bytes, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidContainer
        );
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut bytes = simple_index().encode().unwrap();
        bytes.push(0);
        assert_eq!(
            ObservationIndex::decode(&bytes, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidContainer
        );
    }

    #[test]
    fn validate_accepts_a_correct_index() {
        // Program: emit a 3-byte object, then an inline pair -> total 5.
        let program = Program::new(vec![
            Op::EmitObject { object_id: 0 },
            Op::Inline {
                bytes: b"ab".to_vec(),
            },
        ]);
        let object_lens = [3u64];
        let idx = ObservationIndex {
            section_flags: SECTION_OP_TABLE | SECTION_PDF_SELECTORS | SECTION_DIGESTS,
            ops: vec![
                OpEntry {
                    out_len: 3,
                    dep_kind: DEP_OBJECT,
                    dep_id: 0,
                },
                OpEntry {
                    out_len: 2,
                    dep_kind: DEP_NONE,
                    dep_id: 0,
                },
            ],
            selectors: vec![ObservationSelector {
                kind: SELECTOR_OBJECT,
                number: 1,
                generation: 0,
                out_off: 0,
                out_len: 3,
            }],
            digests: vec![ObservationDigest {
                out_off: 3,
                out_len: 2,
                sha256: [0u8; 32],
            }],
        };
        idx.validate(&program, &object_lens, &[], Limits::DEFAULT)
            .unwrap();
    }

    #[test]
    fn validate_rejects_wrong_op_len_dep_and_selector() {
        let program = Program::new(vec![Op::EmitObject { object_id: 0 }]);
        let object_lens = [3u64];

        let wrong_len = ObservationIndex {
            section_flags: SECTION_OP_TABLE,
            ops: vec![OpEntry {
                out_len: 4,
                dep_kind: DEP_OBJECT,
                dep_id: 0,
            }],
            ..Default::default()
        };
        assert_eq!(
            wrong_len
                .validate(&program, &object_lens, &[], Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::CoverageViolation
        );

        let bad_dep = ObservationIndex {
            section_flags: SECTION_OP_TABLE,
            ops: vec![OpEntry {
                out_len: 3,
                dep_kind: DEP_OBJECT,
                dep_id: 7,
            }],
            ..Default::default()
        };
        assert_eq!(
            bad_dep
                .validate(&program, &object_lens, &[], Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::CoverageViolation
        );

        let past_total = ObservationIndex {
            section_flags: SECTION_PDF_SELECTORS,
            selectors: vec![ObservationSelector {
                kind: SELECTOR_OBJECT,
                number: 1,
                generation: 0,
                out_off: 2,
                out_len: 5,
            }],
            ..Default::default()
        };
        assert_eq!(
            past_total
                .validate(&program, &object_lens, &[], Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::CoverageViolation
        );
    }
}
