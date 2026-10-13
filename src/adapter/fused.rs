//! Generalized fused execution kernel (Phase 29.1).
//!
//! This module is the **format-independent half** of the fused kernel. It defines
//! one versioned internal interface — a logical plan of *(projection + predicate +
//! accumulator)* — and the typed, validity-aware accumulator that every physical
//! scanner folds native values into:
//!
//! * [`FusedPlan`] is the versioned logical plan. The physical scanner is chosen by
//!   the caller (Arrow today; Parquet and the delimited/fixed-width tabular readers
//!   in the same phase) and only the addressed column is decoded.
//! * [`FusedPredicate`] is an equality or (optionally half-bounded) inclusive range
//!   predicate. `Gt` is retained for the Phase-28.5 contract.
//! * [`FusedOp`] is the accumulator: `count`, `sum`, `min`, `max`, `avg`, or a
//!   bounded grouped count.
//! * [`Accumulator`] consumes chunks of **native** `i64`/`f64` values (plus an
//!   optional validity mask) and never materializes a `Value` enum, allocates per
//!   value, renders text, or builds JSON.
//!
//! ## Null semantics (the exact rule, SQL-like)
//!
//! A `NULL` value is never equal to, less than, or greater than anything, so a
//! predicate is **false** on a null row. Every aggregate therefore excludes nulls:
//!
//! * `count` is the number of **non-null** values satisfying the predicate;
//! * `sum`/`min`/`max`/`avg` are over the non-null satisfying values;
//! * an aggregate over **zero** satisfying rows is SQL `NULL` ([`FusedMeasure::Null`]);
//! * a grouped count counts the **non-null** rows per distinct integer value.
//!
//! This is exactly the rule pyarrow and DuckDB apply, so the fused answer is
//! directly comparable to the oracle lanes.
//!
//! ## Typed declines
//!
//! Anything outside the supported subset (a text/nested/decimal column, an
//! unsupported page encoding, an unparseable delimited field, a group count above
//! its bound) is a **typed decline** at the scanner, never a wrong answer.

use std::collections::BTreeMap;

use crate::error::{Error, Result};

/// Wire version of the logical plan. Bumped when the plan's meaning changes.
pub const FUSED_PLAN_VERSION: u8 = 1;

/// The logical kind of the addressed column, as decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    /// A signed or unsigned integer column (compared as `i128`).
    Int,
    /// A floating-point column (compared as `f64`).
    Float,
}

impl ColumnKind {
    /// Stable lowercase name.
    pub const fn name(self) -> &'static str {
        match self {
            ColumnKind::Int => "int",
            ColumnKind::Float => "float",
        }
    }
}

/// An equality or inclusive-range predicate over a numeric column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FusedPredicate {
    /// No predicate: every non-null value passes.
    Always,
    /// Strictly greater than the threshold (retained from the Phase-28.5 contract).
    Gt(i64),
    /// Equal to the threshold.
    Eq(i64),
    /// Inclusive bounds; `None` is unbounded on that side.
    Range {
        /// The inclusive lower bound (`None` = unbounded).
        lo: Option<i64>,
        /// The inclusive upper bound (`None` = unbounded).
        hi: Option<i64>,
    },
}

impl FusedPredicate {
    /// Does an integer value (as `i128`) satisfy the predicate?
    #[inline(always)]
    pub fn int_matches(self, v: i128) -> bool {
        match self {
            FusedPredicate::Always => true,
            FusedPredicate::Gt(t) => v > i128::from(t),
            FusedPredicate::Eq(t) => v == i128::from(t),
            FusedPredicate::Range { lo, hi } => {
                lo.is_none_or(|l| v >= i128::from(l)) && hi.is_none_or(|h| v <= i128::from(h))
            }
        }
    }

    /// Does an `i64` value satisfy the predicate? (Avoids the widening in
    /// [`Self::int_matches`] on the native-value hot path.)
    #[inline(always)]
    pub fn int_matches_i64(self, v: i64) -> bool {
        match self {
            FusedPredicate::Always => true,
            FusedPredicate::Gt(t) => v > t,
            FusedPredicate::Eq(t) => v == t,
            FusedPredicate::Range { lo, hi } => {
                lo.is_none_or(|l| v >= l) && hi.is_none_or(|h| v <= h)
            }
        }
    }

    /// Does a floating-point value satisfy the predicate?
    #[inline(always)]
    pub fn float_matches(self, v: f64) -> bool {
        match self {
            FusedPredicate::Always => true,
            FusedPredicate::Gt(t) => v > t as f64,
            FusedPredicate::Eq(t) => v == t as f64,
            FusedPredicate::Range { lo, hi } => {
                lo.is_none_or(|l| v >= l as f64) && hi.is_none_or(|h| v <= h as f64)
            }
        }
    }

    /// Parse `any`, `gt:K`, `eq:K`, or `range:LO:HI` (either bound may be empty).
    pub fn parse(spec: &str) -> Result<Self> {
        let spec = spec.trim();
        if spec == "any" || spec.is_empty() {
            return Ok(FusedPredicate::Always);
        }
        if let Some(rest) = spec.strip_prefix("gt:") {
            return Ok(FusedPredicate::Gt(parse_int(rest, "gt")?));
        }
        if let Some(rest) = spec.strip_prefix("eq:") {
            return Ok(FusedPredicate::Eq(parse_int(rest, "eq")?));
        }
        if let Some(rest) = spec.strip_prefix("range:") {
            let (lo, hi) = rest
                .split_once(':')
                .ok_or_else(|| Error::usage("range predicate needs `range:LO:HI`"))?;
            let lo = if lo.trim().is_empty() {
                None
            } else {
                Some(parse_int(lo, "range lo")?)
            };
            let hi = if hi.trim().is_empty() {
                None
            } else {
                Some(parse_int(hi, "range hi")?)
            };
            return Ok(FusedPredicate::Range { lo, hi });
        }
        Err(Error::usage(format!(
            "unknown predicate {spec:?} (use any|gt:K|eq:K|range:LO:HI)"
        )))
    }
}

fn parse_int(s: &str, what: &str) -> Result<i64> {
    s.trim()
        .parse::<i64>()
        .map_err(|_| Error::usage(format!("{what} bound {s:?} is not an integer")))
}

/// The accumulator operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FusedOp {
    /// Count the non-null rows satisfying the predicate.
    Count,
    /// Exact (i128) sum, or f64 sum for a float column.
    Sum,
    /// Minimum satisfying value.
    Min,
    /// Maximum satisfying value.
    Max,
    /// Mean of the satisfying values (f64).
    Avg,
    /// Bounded grouped counts keyed by the integer value (integer columns only).
    Grouped,
}

impl FusedOp {
    /// Stable lowercase name.
    pub const fn name(self) -> &'static str {
        match self {
            FusedOp::Count => "count",
            FusedOp::Sum => "sum",
            FusedOp::Min => "min",
            FusedOp::Max => "max",
            FusedOp::Avg => "avg",
            FusedOp::Grouped => "grouped",
        }
    }

    /// `grouped` counts only a bounded number of distinct keys.
    pub const fn needs_int_column(self) -> bool {
        matches!(self, FusedOp::Grouped)
    }
}

/// The measured aggregate.
#[derive(Debug, Clone, PartialEq)]
pub enum FusedMeasure {
    /// A count (always present for every op).
    Count(u64),
    /// An exact integer sum.
    Sum(i128),
    /// A floating-point sum.
    SumF(f64),
    /// The minimum integer value.
    MinI(i64),
    /// The maximum integer value.
    MaxI(i64),
    /// The minimum float value.
    MinF(f64),
    /// The maximum float value.
    MaxF(f64),
    /// The arithmetic mean.
    Avg(f64),
    /// Grouped counts, in ascending key order.
    Grouped(Vec<(i64, u64)>),
    /// SQL `NULL` (an aggregate over zero satisfying rows).
    Null,
}

/// The versioned logical plan.
#[derive(Debug, Clone, PartialEq)]
pub struct FusedPlan {
    /// The plan wire version ([`FUSED_PLAN_VERSION`]).
    pub version: u8,
    /// The predicate applied to the projected column.
    pub predicate: FusedPredicate,
    /// The accumulator.
    pub op: FusedOp,
    /// The maximum number of distinct group keys (bounded grouped counts).
    pub group_cap: usize,
}

impl FusedPlan {
    /// A plan at the current version.
    pub fn new(predicate: FusedPredicate, op: FusedOp, group_cap: usize) -> Self {
        FusedPlan {
            version: FUSED_PLAN_VERSION,
            predicate,
            op,
            group_cap,
        }
    }
}

/// The result of a fused scan.
#[derive(Debug, Clone, PartialEq)]
pub struct FusedAnswer {
    /// The decoded column kind.
    pub kind: ColumnKind,
    /// The accumulator that produced [`Self::measure`].
    pub op: FusedOp,
    /// Rows addressed by the scan (summed over decoded chunks).
    pub scanned: u64,
    /// Null rows encountered (excluded from the predicate and every aggregate).
    pub nulls: u64,
    /// Non-null rows satisfying the predicate.
    pub matched: u64,
    /// The measured value.
    pub measure: FusedMeasure,
    /// Whether the group-key bound was exceeded (the group list is then partial).
    pub groups_truncated: bool,
}

impl FusedAnswer {
    /// The count of non-null satisfying rows (available for every operator).
    pub fn count(&self) -> u64 {
        self.matched
    }
}

/// The typed-decline error for an unsigned value above the `i64` range.
#[cold]
fn unsigned_overflow() -> Error {
    Error::unsupported_feature("an unsigned integer value exceeds the i64 range")
}

/// Read one little-endian fixed-width integer chunk as `i64`. Returns `None` only
/// for an unsigned 64-bit value above `i64::MAX` (a typed decline). The width and
/// signedness branches are resolved at monomorphization.
#[inline(always)]
fn read_le_int<const W: usize, const SIGNED: bool>(chunk: &[u8]) -> Option<i64> {
    if W == 8 {
        let mut a = [0u8; 8];
        a.copy_from_slice(chunk);
        if SIGNED {
            Some(i64::from_le_bytes(a))
        } else {
            let u = u64::from_le_bytes(a);
            if u > i64::MAX as u64 {
                None
            } else {
                Some(u as i64)
            }
        }
    } else if W == 4 {
        let mut a = [0u8; 4];
        a.copy_from_slice(chunk);
        Some(if SIGNED {
            i64::from(i32::from_le_bytes(a))
        } else {
            i64::from(u32::from_le_bytes(a))
        })
    } else if W == 2 {
        let mut a = [0u8; 2];
        a.copy_from_slice(chunk);
        Some(if SIGNED {
            i64::from(i16::from_le_bytes(a))
        } else {
            i64::from(u16::from_le_bytes(a))
        })
    } else {
        Some(if SIGNED {
            i64::from(chunk[0] as i8)
        } else {
            i64::from(chunk[0])
        })
    }
}

/// Read one little-endian fixed-width floating-point chunk as `f64`.
#[inline(always)]
fn read_le_f64<const W: usize>(chunk: &[u8]) -> f64 {
    if W == 8 {
        let mut a = [0u8; 8];
        a.copy_from_slice(chunk);
        f64::from_le_bytes(a)
    } else {
        let mut a = [0u8; 4];
        a.copy_from_slice(chunk);
        f64::from(f32::from_le_bytes(a))
    }
}

/// A validity-aware accumulator over native chunks.
///
/// `push_*` folds a chunk of native values with an optional parallel validity
/// mask (`None` = all valid). Chunks are typically one record batch (Arrow) or one
/// data page / row group (Parquet), or a streamed delimited block.
#[derive(Debug, Clone)]
pub struct Accumulator {
    op: FusedOp,
    predicate: FusedPredicate,
    group_cap: usize,
    scanned: u64,
    nulls: u64,
    matched: u64,
    sum_int: i128,
    min_int: Option<i64>,
    max_int: Option<i64>,
    sum_float: f64,
    min_float: Option<f64>,
    max_float: Option<f64>,
    groups: BTreeMap<i64, u64>,
    groups_truncated: bool,
}

impl Accumulator {
    /// A fresh accumulator for `plan`.
    pub fn new(plan: &FusedPlan) -> Self {
        Accumulator {
            op: plan.op,
            predicate: plan.predicate,
            group_cap: plan.group_cap.max(1),
            scanned: 0,
            nulls: 0,
            matched: 0,
            sum_int: 0,
            min_int: None,
            max_int: None,
            sum_float: 0.0,
            min_float: None,
            max_float: None,
            groups: BTreeMap::new(),
            groups_truncated: false,
        }
    }

    /// Fold one chunk of native `i64` values.
    pub fn push_i64(&mut self, values: &[i64], valid: Option<&[bool]>) {
        let op = self.op;
        let pred = self.predicate;
        self.scanned += values.len() as u64;
        match valid {
            None => {
                for &v in values {
                    if pred.int_matches_i64(v) {
                        self.fold_int(v, op);
                    }
                }
            }
            Some(m) => {
                for (i, &v) in values.iter().enumerate() {
                    if !m[i] {
                        self.nulls += 1;
                        continue;
                    }
                    if pred.int_matches_i64(v) {
                        self.fold_int(v, op);
                    }
                }
            }
        }
    }

    /// Fold one chunk of native `f64` values.
    pub fn push_f64(&mut self, values: &[f64], valid: Option<&[bool]>) {
        let op = self.op;
        let pred = self.predicate;
        self.scanned += values.len() as u64;
        match valid {
            None => {
                for &v in values {
                    if pred.float_matches(v) {
                        self.fold_float(v, op);
                    }
                }
            }
            Some(m) => {
                for (i, &v) in values.iter().enumerate() {
                    if !m[i] {
                        self.nulls += 1;
                        continue;
                    }
                    if pred.float_matches(v) {
                        self.fold_float(v, op);
                    }
                }
            }
        }
    }

    /// Fold a contiguous little-endian, fixed-width **integer** buffer directly.
    ///
    /// `data.len()` must be an exact multiple of `W` (the caller reads a bounded
    /// slice of `rows * W` bytes). `W` is the stored byte width and `SIGNED`
    /// selects the two's-complement interpretation, so the width and signedness
    /// dispatch is resolved at monomorphization and the inner loop reads, tests,
    /// and folds in one pass with no per-value `Vec`, `Value`, text, or `Result`.
    /// `validity` is the raw little-endian validity bitmap (`None` = all valid);
    /// the all-valid case is a branch-free inner loop.
    ///
    /// Typed decline: an **unsigned** 64-bit value above `i64::MAX` cannot be
    /// represented in the column model and is returned as
    /// [`Error::unsupported_feature`], never folded as a wrong value.
    #[inline]
    #[allow(clippy::chunks_exact_to_as_chunks)]
    pub fn push_i64_le<const W: usize, const SIGNED: bool>(
        &mut self,
        data: &[u8],
        validity: Option<&[u8]>,
    ) -> Result<()> {
        debug_assert!(matches!(W, 1 | 2 | 4 | 8));
        let rows = data.len() / W;
        self.scanned += rows as u64;
        let pred = self.predicate;
        let op = self.op;
        match validity {
            None => {
                for chunk in data.chunks_exact(W) {
                    let Some(v) = read_le_int::<W, SIGNED>(chunk) else {
                        return Err(unsigned_overflow());
                    };
                    if pred.int_matches_i64(v) {
                        self.fold_int(v, op);
                    }
                }
            }
            Some(vb) => {
                let mut i = 0usize;
                for chunk in data.chunks_exact(W) {
                    if (vb[i >> 3] >> (i & 7)) & 1 == 0 {
                        self.nulls += 1;
                        i += 1;
                        continue;
                    }
                    let Some(v) = read_le_int::<W, SIGNED>(chunk) else {
                        return Err(unsigned_overflow());
                    };
                    if pred.int_matches_i64(v) {
                        self.fold_int(v, op);
                    }
                    i += 1;
                }
            }
        }
        Ok(())
    }

    /// Fold a contiguous little-endian, fixed-width **floating-point** buffer
    /// directly. `W` is 4 (`f32`) or 8 (`f64`); `data.len()` must be an exact
    /// multiple of `W`. Same single-pass, monomorphized contract as
    /// [`Self::push_i64_le`], with the all-valid case a branch-free inner loop.
    #[inline]
    #[allow(clippy::chunks_exact_to_as_chunks)]
    pub fn push_f64_le<const W: usize>(&mut self, data: &[u8], validity: Option<&[u8]>) {
        debug_assert!(matches!(W, 4 | 8));
        let rows = data.len() / W;
        self.scanned += rows as u64;
        let pred = self.predicate;
        let op = self.op;
        match validity {
            None => {
                for chunk in data.chunks_exact(W) {
                    let v = read_le_f64::<W>(chunk);
                    if pred.float_matches(v) {
                        self.fold_float(v, op);
                    }
                }
            }
            Some(vb) => {
                let mut i = 0usize;
                for chunk in data.chunks_exact(W) {
                    if (vb[i >> 3] >> (i & 7)) & 1 == 0 {
                        self.nulls += 1;
                        i += 1;
                        continue;
                    }
                    let v = read_le_f64::<W>(chunk);
                    if pred.float_matches(v) {
                        self.fold_float(v, op);
                    }
                    i += 1;
                }
            }
        }
    }

    /// Record `rows` addressed-but-not-decoded rows (a statistics skip), plus the
    /// nulls the chunk's statistics declare (so the validity accounting matches a
    /// decoded scan).
    pub fn skip(&mut self, rows: u64, nulls: u64) {
        self.scanned += rows;
        self.nulls += nulls.min(rows);
    }

    /// Record a whole chunk that provably **all** matched (a statistics
    /// short-circuit). Only meaningful for [`FusedOp::Count`]; the caller guards
    /// the operator.
    pub fn add_all_matched(&mut self, matched: u64, nulls: u64) {
        self.scanned += matched + nulls;
        self.matched += matched;
        self.nulls += nulls;
    }

    #[inline(always)]
    fn fold_int(&mut self, v: i64, op: FusedOp) {
        self.matched += 1;
        match op {
            FusedOp::Count => {}
            FusedOp::Sum => self.sum_int += i128::from(v),
            FusedOp::Min => {
                self.min_int = Some(self.min_int.map_or(v, |m| m.min(v)));
            }
            FusedOp::Max => {
                self.max_int = Some(self.max_int.map_or(v, |m| m.max(v)));
            }
            FusedOp::Avg => self.sum_int += i128::from(v),
            FusedOp::Grouped => {
                if let Some(c) = self.groups.get_mut(&v) {
                    *c += 1;
                } else if self.groups.len() < self.group_cap {
                    self.groups.insert(v, 1);
                } else {
                    self.groups_truncated = true;
                }
            }
        }
    }

    #[inline(always)]
    fn fold_float(&mut self, v: f64, op: FusedOp) {
        self.matched += 1;
        match op {
            FusedOp::Count => {}
            FusedOp::Sum => self.sum_float += v,
            FusedOp::Min => {
                self.min_float = Some(self.min_float.map_or(v, |m| m.min(v)));
            }
            FusedOp::Max => {
                self.max_float = Some(self.max_float.map_or(v, |m| m.max(v)));
            }
            FusedOp::Avg => self.sum_float += v,
            // Grouped counts are integer-only; a float scanner must never reach here.
            FusedOp::Grouped => {}
        }
    }

    /// Merge `other` — a partial accumulator over a **later, contiguous** chunk of
    /// the same input — into `self`, in physical chunk order.
    ///
    /// This is the bounded parallel delimited scan's reduction (Phase 30.2): each
    /// worker folds one record-aligned chunk (a [`Self`]) and the caller merges the
    /// partials left-to-right. Every order-independent measure (`count`, integer
    /// `sum`/`min`/`max`, grouped counts) is therefore **identical** to the serial
    /// fold; a floating-point `sum`/`avg` reassociates across chunk boundaries, which
    /// is deterministic (the boundaries are fixed by the input length, not the thread
    /// count) and stays within the oracle's `1e-9` relative tolerance.
    pub fn merge(&mut self, other: Accumulator) {
        self.scanned += other.scanned;
        self.nulls += other.nulls;
        self.matched += other.matched;
        self.sum_int += other.sum_int;
        self.sum_float += other.sum_float;
        if let Some(v) = other.min_int {
            self.min_int = Some(self.min_int.map_or(v, |m| m.min(v)));
        }
        if let Some(v) = other.max_int {
            self.max_int = Some(self.max_int.map_or(v, |m| m.max(v)));
        }
        if let Some(v) = other.min_float {
            self.min_float = Some(self.min_float.map_or(v, |m| m.min(v)));
        }
        if let Some(v) = other.max_float {
            self.max_float = Some(self.max_float.map_or(v, |m| m.max(v)));
        }
        for (k, c) in other.groups {
            if let Some(dst) = self.groups.get_mut(&k) {
                *dst += c;
            } else if self.groups.len() < self.group_cap {
                self.groups.insert(k, c);
            } else {
                self.groups_truncated = true;
            }
        }
        self.groups_truncated |= other.groups_truncated;
    }

    /// Finalize into a [`FusedAnswer`] for a column of `kind`.
    pub fn finish(self, kind: ColumnKind) -> FusedAnswer {
        let measure = match (self.op, kind) {
            (FusedOp::Count, _) => FusedMeasure::Count(self.matched),
            (FusedOp::Sum, ColumnKind::Int) => {
                if self.matched == 0 {
                    FusedMeasure::Null
                } else {
                    FusedMeasure::Sum(self.sum_int)
                }
            }
            (FusedOp::Sum, ColumnKind::Float) => {
                if self.matched == 0 {
                    FusedMeasure::Null
                } else {
                    FusedMeasure::SumF(self.sum_float)
                }
            }
            (FusedOp::Min, ColumnKind::Int) => match self.min_int {
                Some(v) => FusedMeasure::MinI(v),
                None => FusedMeasure::Null,
            },
            (FusedOp::Max, ColumnKind::Int) => match self.max_int {
                Some(v) => FusedMeasure::MaxI(v),
                None => FusedMeasure::Null,
            },
            (FusedOp::Min, ColumnKind::Float) => match self.min_float {
                Some(v) => FusedMeasure::MinF(v),
                None => FusedMeasure::Null,
            },
            (FusedOp::Max, ColumnKind::Float) => match self.max_float {
                Some(v) => FusedMeasure::MaxF(v),
                None => FusedMeasure::Null,
            },
            (FusedOp::Avg, ColumnKind::Int) => {
                if self.matched == 0 {
                    FusedMeasure::Null
                } else {
                    // The exact `i128` sum is divided once, matching the reference
                    // and pyarrow/DuckDB within the oracle tolerance.
                    FusedMeasure::Avg(self.sum_int as f64 / self.matched as f64)
                }
            }
            (FusedOp::Avg, ColumnKind::Float) => {
                if self.matched == 0 {
                    FusedMeasure::Null
                } else {
                    FusedMeasure::Avg(self.sum_float / self.matched as f64)
                }
            }
            (FusedOp::Grouped, _) => FusedMeasure::Grouped(self.groups.into_iter().collect()),
        };
        FusedAnswer {
            kind,
            op: self.op,
            scanned: self.scanned,
            nulls: self.nulls,
            matched: self.matched,
            measure,
            groups_truncated: self.groups_truncated,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acc(pred: FusedPredicate, op: FusedOp) -> Accumulator {
        Accumulator::new(&FusedPlan::new(pred, op, 64))
    }

    #[test]
    fn count_excludes_nulls_and_non_matches() {
        let mut a = acc(FusedPredicate::Gt(0), FusedOp::Count);
        a.push_i64(&[1, -1, 2, -2], None);
        a.push_i64(&[3, 4], Some(&[true, false]));
        let ans = a.finish(ColumnKind::Int);
        assert_eq!(ans.matched, 3);
        assert_eq!(ans.scanned, 6);
        assert_eq!(ans.nulls, 1);
        assert_eq!(ans.measure, FusedMeasure::Count(3));
    }

    #[test]
    fn sum_min_max_avg_over_matching_values() {
        let mut a = acc(
            FusedPredicate::Range {
                lo: Some(0),
                hi: None,
            },
            FusedOp::Sum,
        );
        a.push_i64(&[-5, 0, 5, 10], None);
        assert_eq!(a.finish(ColumnKind::Int).measure, FusedMeasure::Sum(15));

        let mut a = acc(FusedPredicate::Always, FusedOp::Min);
        a.push_i64(&[7, 3, 9, -1], None);
        assert_eq!(a.finish(ColumnKind::Int).measure, FusedMeasure::MinI(-1));

        let mut a = acc(FusedPredicate::Eq(2), FusedOp::Max);
        a.push_i64(&[1, 2, 3], None);
        assert_eq!(a.finish(ColumnKind::Int).measure, FusedMeasure::MaxI(2));

        let mut a = acc(FusedPredicate::Always, FusedOp::Avg);
        a.push_f64(&[1.0, 2.0, 3.0, 4.0], None);
        assert_eq!(a.finish(ColumnKind::Float).measure, FusedMeasure::Avg(2.5));
    }

    #[test]
    fn empty_aggregate_is_null_but_count_is_zero() {
        let a = acc(FusedPredicate::Gt(100), FusedOp::Sum);
        assert_eq!(a.finish(ColumnKind::Int).measure, FusedMeasure::Null);
        let a = acc(FusedPredicate::Gt(100), FusedOp::Count);
        assert_eq!(a.finish(ColumnKind::Int).measure, FusedMeasure::Count(0));
    }

    #[test]
    fn grouped_counts_are_bounded_and_sorted() {
        let mut a = acc(FusedPredicate::Always, FusedOp::Grouped);
        a.push_i64(&[2, 1, 2, 3, 1, 2], None);
        let ans = a.finish(ColumnKind::Int);
        assert_eq!(
            ans.measure,
            FusedMeasure::Grouped(vec![(1, 2), (2, 3), (3, 1)])
        );
        assert!(!ans.groups_truncated);

        let plan = FusedPlan::new(FusedPredicate::Always, FusedOp::Grouped, 2);
        let mut a = Accumulator::new(&plan);
        a.push_i64(&[1, 2, 3, 4], None);
        let ans = a.finish(ColumnKind::Int);
        assert!(ans.groups_truncated);
        assert_eq!(ans.matched, 4);
    }

    #[test]
    fn predicates_parse() {
        assert_eq!(
            FusedPredicate::parse("any").unwrap(),
            FusedPredicate::Always
        );
        assert_eq!(
            FusedPredicate::parse("gt:5").unwrap(),
            FusedPredicate::Gt(5)
        );
        assert_eq!(
            FusedPredicate::parse("eq:-3").unwrap(),
            FusedPredicate::Eq(-3)
        );
        assert_eq!(
            FusedPredicate::parse("range:1:9").unwrap(),
            FusedPredicate::Range {
                lo: Some(1),
                hi: Some(9)
            }
        );
        assert_eq!(
            FusedPredicate::parse("range:1:").unwrap(),
            FusedPredicate::Range {
                lo: Some(1),
                hi: None
            }
        );
        assert!(FusedPredicate::parse("nope").is_err());
    }
}
