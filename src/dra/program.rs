//! Reconstruction program, coverage certificate, and bounded evaluation.

use crate::dra::op::Op;
use crate::error::{Error, Result};
use crate::limits::Limits;

/// DRA version carried in the graph record.
pub const DRA_VERSION: u8 = 1;

/// Who is the reconstruction authority for an output interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authority {
    /// Bytes reproduced verbatim from a literal object or inline literal.
    Literal,
    /// Bytes deterministically generated (currently only by `REPEAT_LAST`).
    Generated,
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
    /// object *lengths* (no byte materialization, no large allocation).
    pub fn analyze(&self, object_lens: &[u64], limits: Limits) -> Result<(u64, CoverageMap)> {
        if self.ops.len() as u64 > limits.max_graph_ops as u64 {
            return Err(Error::resource_limit("graph instruction limit exceeded"));
        }
        let mut spans: Vec<Span> = Vec::new();
        let mut total: u64 = 0;
        let mut last_len: u64 = 0;
        let mut have_last = false;

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

    /// Analyze using a concrete object table.
    pub fn analyze_objects(
        &self,
        objects: &[Vec<u8>],
        limits: Limits,
    ) -> Result<(u64, CoverageMap)> {
        let lens: Vec<u64> = objects.iter().map(|o| o.len() as u64).collect();
        self.analyze(&lens, limits)
    }

    /// Materialize the program's output, enforcing all bounds.
    pub fn eval(&self, objects: &[Vec<u8>], limits: Limits) -> Result<Vec<u8>> {
        let (predicted, _coverage) = self.analyze_objects(objects, limits)?;
        let cap = predicted.min(64 * 1024 * 1024) as usize;
        let mut out: Vec<u8> = Vec::with_capacity(cap);
        let mut have_last = false;
        let mut block_len: usize = 0;

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
        assert_eq!(p.eval(&objects, Limits::DEFAULT).unwrap(), b"hello world");
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
        assert_eq!(p.eval(&objects, Limits::DEFAULT).unwrap(), b"ababab");
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
}
