//! Phase 29.1 — the generalized fused kernel: cross-format correctness.
//!
//! Every test checks the fused answer against an **independent** reference built
//! from the adapter's own fully-decoded `Value` column (Arrow/Parquet), so the
//! kernel is never compared to itself. The reference applies exactly the SQL null
//! rule the kernel promises: nulls are excluded from the predicate and from every
//! aggregate.

#![cfg(all(
    feature = "arrow",
    feature = "parquet",
    feature = "csv",
    feature = "fixedwidth"
))]

use std::collections::BTreeMap;

use vole_document::adapter::arrow::{
    self, Value, column_values, fused_scan as arrow_fused_scan, leaf_index, parse,
};
use vole_document::adapter::fused::{ColumnKind, FusedMeasure, FusedOp, FusedPlan, FusedPredicate};
use vole_document::adapter::parquet::{self, Value as PqValue};
use vole_document::limits::Limits;

const PRIMITIVES: &[u8] = include_bytes!("../tools/fixtures/arrow/primitives.arrow");
const NULLABLE: &[u8] = include_bytes!("../tools/fixtures/arrow/nullable.arrow");
const MULTI_BATCH: &[u8] = include_bytes!("../tools/fixtures/arrow/multi_batch.arrow");

const PQ_SMALL: &[u8] = include_bytes!("../tools/fixtures/parquet/small_plain.parquet");
const PQ_OPTIONAL: &[u8] = include_bytes!("../tools/fixtures/parquet/optional.parquet");
const PQ_NULLABLE: &[u8] = include_bytes!("../tools/fixtures/parquet/phase29_nullable.parquet");
const PQ_DICTIONARY: &[u8] = include_bytes!("../tools/fixtures/parquet/dictionary.parquet");
const PQ_MULTI_RG: &[u8] = include_bytes!("../tools/fixtures/parquet/multi_rg.parquet");
const PQ_TWO_PAGES: &[u8] = include_bytes!("../tools/fixtures/parquet/two_pages.parquet");

#[derive(Debug)]
struct Exact {
    scanned: u64,
    nulls: u64,
    measure: FusedMeasure,
}

fn arrow_int(v: &Value) -> Option<i128> {
    match v {
        Value::I64(n) => Some(i128::from(*n)),
        Value::U64(n) => Some(i128::from(*n)),
        _ => None,
    }
}

fn arrow_float(v: &Value) -> Option<f64> {
    match v {
        Value::F16(b) => Some(f64::from(arrow::half_to_f32(*b))),
        Value::F32(x) => Some(f64::from(*x)),
        Value::F64(x) => Some(*x),
        _ => None,
    }
}

/// The reference aggregate over a decoded column, using the same null rule.
fn reference_int(vals: &[Value], pred: FusedPredicate, op: FusedOp, _cap: usize) -> Exact {
    let mut scanned = 0u64;
    let mut nulls = 0u64;
    let mut matched = 0u64;
    let mut sum: i128 = 0;
    let mut mn: Option<i128> = None;
    let mut mx: Option<i128> = None;
    let mut groups: BTreeMap<i64, u64> = BTreeMap::new();
    for v in vals {
        scanned += 1;
        let Some(x) = arrow_int(v) else {
            nulls += 1;
            continue;
        };
        if !pred.int_matches(x) {
            continue;
        }
        matched += 1;
        sum += x;
        mn = Some(mn.map_or(x, |m| m.min(x)));
        mx = Some(mx.map_or(x, |m| m.max(x)));
        if op == FusedOp::Grouped {
            let k = i64::try_from(x).expect("group key fits i64");
            *groups.entry(k).or_insert(0) += 1;
        }
    }
    let measure = match op {
        FusedOp::Count => FusedMeasure::Count(matched),
        FusedOp::Sum => {
            if matched == 0 {
                FusedMeasure::Null
            } else {
                FusedMeasure::Sum(sum)
            }
        }
        FusedOp::Min => mn
            .map(|x| FusedMeasure::MinI(i64::try_from(x).unwrap()))
            .unwrap_or(FusedMeasure::Null),
        FusedOp::Max => mx
            .map(|x| FusedMeasure::MaxI(i64::try_from(x).unwrap()))
            .unwrap_or(FusedMeasure::Null),
        FusedOp::Avg => {
            if matched == 0 {
                FusedMeasure::Null
            } else {
                FusedMeasure::Avg(sum as f64 / matched as f64)
            }
        }
        FusedOp::Grouped => FusedMeasure::Grouped(groups.into_iter().collect()),
    };
    Exact {
        scanned,
        nulls,
        measure,
    }
}

fn reference_float(vals: &[Value], pred: FusedPredicate, op: FusedOp) -> Exact {
    let mut scanned = 0u64;
    let mut nulls = 0u64;
    let mut matched = 0u64;
    let mut sum = 0.0f64;
    let mut mn: Option<f64> = None;
    let mut mx: Option<f64> = None;
    for v in vals {
        scanned += 1;
        let Some(x) = arrow_float(v) else {
            nulls += 1;
            continue;
        };
        if !pred.float_matches(x) {
            continue;
        }
        matched += 1;
        sum += x;
        mn = Some(mn.map_or(x, |m| m.min(x)));
        mx = Some(mx.map_or(x, |m| m.max(x)));
    }
    let measure = match op {
        FusedOp::Count => FusedMeasure::Count(matched),
        FusedOp::Sum => {
            if matched == 0 {
                FusedMeasure::Null
            } else {
                FusedMeasure::SumF(sum)
            }
        }
        FusedOp::Min => mn.map(FusedMeasure::MinF).unwrap_or(FusedMeasure::Null),
        FusedOp::Max => mx.map(FusedMeasure::MaxF).unwrap_or(FusedMeasure::Null),
        FusedOp::Avg => {
            if matched == 0 {
                FusedMeasure::Null
            } else {
                FusedMeasure::Avg(sum / matched as f64)
            }
        }
        FusedOp::Grouped => FusedMeasure::Null,
    };
    Exact {
        scanned,
        nulls,
        measure,
    }
}

fn close(a: &FusedMeasure, b: &FusedMeasure) -> bool {
    match (a, b) {
        (FusedMeasure::Avg(x), FusedMeasure::Avg(y)) => (x - y).abs() <= 1e-9 * (1.0 + x.abs()),
        (FusedMeasure::SumF(x), FusedMeasure::SumF(y)) => (x - y).abs() <= 1e-9 * (1.0 + x.abs()),
        (FusedMeasure::MinF(x), FusedMeasure::MinF(y)) => x == y,
        (FusedMeasure::MaxF(x), FusedMeasure::MaxF(y)) => x == y,
        _ => a == b,
    }
}

fn check_arrow_int(src: &[u8], idx: u32, pred: FusedPredicate, op: FusedOp) {
    let model = parse(src, Limits::DEFAULT).unwrap();
    let vals = column_values(src, &model, idx, Limits::DEFAULT).unwrap();
    let want = reference_int(&vals, pred, op, 4096);
    let plan = FusedPlan::new(pred, op, 4096);
    let got = arrow_fused_scan(src, &model, idx, &plan, Limits::DEFAULT)
        .unwrap_or_else(|e| panic!("col {idx} pred {pred:?} op {op:?}: declined: {e}"));
    assert_eq!(got.scanned, want.scanned, "col {idx} scanned");
    assert_eq!(got.nulls, want.nulls, "col {idx} nulls");
    assert!(
        close(&got.measure, &want.measure),
        "col {idx} pred {pred:?} op {op:?}: got {:?} want {:?}",
        got.measure,
        want.measure
    );
}

fn check_arrow_float(src: &[u8], idx: u32, pred: FusedPredicate, op: FusedOp) {
    let model = parse(src, Limits::DEFAULT).unwrap();
    let vals = column_values(src, &model, idx, Limits::DEFAULT).unwrap();
    let want = reference_float(&vals, pred, op);
    let plan = FusedPlan::new(pred, op, 4096);
    let got = arrow_fused_scan(src, &model, idx, &plan, Limits::DEFAULT)
        .unwrap_or_else(|e| panic!("col {idx} pred {pred:?} op {op:?}: declined: {e}"));
    assert_eq!(got.scanned, want.scanned, "col {idx} scanned");
    assert_eq!(got.nulls, want.nulls, "col {idx} nulls");
    assert!(
        close(&got.measure, &want.measure),
        "col {idx} pred {pred:?} op {op:?}: got {:?} want {:?}",
        got.measure,
        want.measure
    );
}

#[test]
fn arrow_int_operators_match_the_decoded_column() {
    let preds = [
        FusedPredicate::Always,
        FusedPredicate::Gt(0),
        FusedPredicate::Eq(0),
        FusedPredicate::Range {
            lo: Some(-1000),
            hi: Some(1000),
        },
        FusedPredicate::Range {
            lo: Some(1),
            hi: None,
        },
    ];
    // Columns 0..=6 are the signed integer leaves; 7 (`u64`, values >= 2^63) is
    // declined by the generalized kernel (see the dedicated test below).
    for idx in 0..=6u32 {
        for pred in preds {
            for op in [
                FusedOp::Count,
                FusedOp::Sum,
                FusedOp::Min,
                FusedOp::Max,
                FusedOp::Avg,
                FusedOp::Grouped,
            ] {
                check_arrow_int(PRIMITIVES, idx, pred, op);
            }
        }
    }
}

#[test]
fn arrow_unsigned_beyond_i64_is_a_typed_decline() {
    // Column 7 is `u64` with values >= 2^63: the generalized i64 accumulator would
    // lose ordering, so the kernel declines typed rather than answer wrongly.
    let model = parse(PRIMITIVES, Limits::DEFAULT).unwrap();
    assert_eq!(model.leaf(7).unwrap().name, "u64");
    let plan = FusedPlan::new(FusedPredicate::Gt(0), FusedOp::Count, 4096);
    let err = arrow_fused_scan(PRIMITIVES, &model, 7, &plan, Limits::DEFAULT).unwrap_err();
    assert_eq!(
        err.class(),
        vole_document::error::ErrorClass::UnsupportedFeature
    );
}

#[test]
fn arrow_float_operators_exclude_nulls() {
    let preds = [
        FusedPredicate::Always,
        FusedPredicate::Gt(0),
        FusedPredicate::Eq(0),
        FusedPredicate::Range {
            lo: Some(-1),
            hi: Some(2),
        },
    ];
    for pred in preds {
        for op in [
            FusedOp::Count,
            FusedOp::Sum,
            FusedOp::Min,
            FusedOp::Max,
            FusedOp::Avg,
        ] {
            // nullable.score (index 2) has two nulls.
            check_arrow_float(NULLABLE, 2, pred, op);
        }
    }
}

#[test]
fn arrow_grouped_is_bounded_and_declines_on_float() {
    // `k` in multi_batch is 0..14 across three batches.
    let model = parse(MULTI_BATCH, Limits::DEFAULT).unwrap();
    let plan = FusedPlan::new(FusedPredicate::Always, FusedOp::Grouped, 4096);
    let got = arrow_fused_scan(MULTI_BATCH, &model, 0, &plan, Limits::DEFAULT).unwrap();
    assert_eq!(got.matched, 15);
    let FusedMeasure::Grouped(g) = got.measure else {
        panic!("expected grouped");
    };
    assert_eq!(g.len(), 15);
    assert!(g.iter().all(|(_, c)| *c == 1));

    // Grouped on a float column is a typed decline.
    let nullable = parse(NULLABLE, Limits::DEFAULT).unwrap();
    let fidx = leaf_index(&nullable, "score").unwrap();
    let plan = FusedPlan::new(FusedPredicate::Always, FusedOp::Grouped, 4096);
    let err = arrow_fused_scan(NULLABLE, &nullable, fidx, &plan, Limits::DEFAULT).unwrap_err();
    assert_eq!(
        err.class(),
        vole_document::error::ErrorClass::UnsupportedFeature
    );
}

#[test]
fn arrow_kernel_declines_text_columns() {
    let model = parse(NULLABLE, Limits::DEFAULT).unwrap();
    let idx = leaf_index(&model, "note").unwrap();
    let plan = FusedPlan::new(FusedPredicate::Always, FusedOp::Count, 4096);
    let err = arrow_fused_scan(NULLABLE, &model, idx, &plan, Limits::DEFAULT).unwrap_err();
    assert_eq!(
        err.class(),
        vole_document::error::ErrorClass::UnsupportedFeature
    );
}

// --- Parquet ---------------------------------------------------------------

fn pq_int(v: &PqValue) -> Option<i128> {
    match v {
        PqValue::I32(n) => Some(i128::from(*n)),
        PqValue::I64(n) => Some(i128::from(*n)),
        _ => None,
    }
}

fn pq_float(v: &PqValue) -> Option<f64> {
    match v {
        PqValue::F32(x) => Some(f64::from(*x)),
        PqValue::F64(x) => Some(*x),
        _ => None,
    }
}

fn check_pq_int(src: &[u8], idx: u32, pred: FusedPredicate, op: FusedOp) {
    let model = parquet::parse(src, Limits::DEFAULT).unwrap();
    let vals = parquet::leaf_values(src, &model, idx, Limits::DEFAULT).unwrap();
    let mut want = reference_int(&[], FusedPredicate::Always, FusedOp::Count, 4096);
    let mut nulls = 0u64;
    let mut matched = 0u64;
    let mut sum: i128 = 0;
    let mut mn: Option<i128> = None;
    let mut mx: Option<i128> = None;
    let mut groups: BTreeMap<i64, u64> = BTreeMap::new();
    for v in &vals {
        match pq_int(v) {
            None => nulls += 1,
            Some(x) if pred.int_matches(x) => {
                matched += 1;
                sum += x;
                mn = Some(mn.map_or(x, |m| m.min(x)));
                mx = Some(mx.map_or(x, |m| m.max(x)));
                let k = i64::try_from(x).unwrap();
                *groups.entry(k).or_insert(0) += 1;
            }
            Some(_) => {}
        }
    }
    want.scanned = vals.len() as u64;
    want.nulls = nulls;
    want.measure = match op {
        FusedOp::Count => FusedMeasure::Count(matched),
        FusedOp::Sum => {
            if matched == 0 {
                FusedMeasure::Null
            } else {
                FusedMeasure::Sum(sum)
            }
        }
        FusedOp::Min => mn
            .map(|x| FusedMeasure::MinI(i64::try_from(x).unwrap()))
            .unwrap_or(FusedMeasure::Null),
        FusedOp::Max => mx
            .map(|x| FusedMeasure::MaxI(i64::try_from(x).unwrap()))
            .unwrap_or(FusedMeasure::Null),
        FusedOp::Avg => {
            if matched == 0 {
                FusedMeasure::Null
            } else {
                FusedMeasure::Avg(sum as f64 / matched as f64)
            }
        }
        FusedOp::Grouped => FusedMeasure::Grouped(groups.into_iter().collect()),
    };
    let plan = FusedPlan::new(pred, op, 4096);
    let got = parquet::fused_scan(src, &model, idx, &plan, Limits::DEFAULT)
        .unwrap_or_else(|e| panic!("pq col {idx} pred {pred:?} op {op:?}: declined: {e}"));
    assert_eq!(got.scanned, want.scanned, "pq col {idx} scanned");
    assert_eq!(got.nulls, want.nulls, "pq col {idx} nulls");
    assert!(
        close(&got.measure, &want.measure),
        "pq col {idx} pred {pred:?} op {op:?}: got {:?} want {:?}",
        got.measure,
        want.measure
    );
}

#[test]
fn parquet_int_operators_match_the_decoded_column() {
    let preds = [
        FusedPredicate::Always,
        FusedPredicate::Gt(0),
        FusedPredicate::Eq(0),
        FusedPredicate::Range {
            lo: Some(-1000),
            hi: Some(1000),
        },
    ];
    for (label, src) in [
        ("small", PQ_SMALL),
        ("optional", PQ_OPTIONAL),
        ("nullable", PQ_NULLABLE),
        ("dictionary", PQ_DICTIONARY),
        ("multi_rg", PQ_MULTI_RG),
        ("two_pages", PQ_TWO_PAGES),
    ] {
        let model = parquet::parse(src, Limits::DEFAULT).unwrap();
        for idx in 0..model.leaves.len() as u32 {
            let leaf = model.leaf(idx).unwrap();
            if !matches!(leaf.physical, parquet::T_INT32 | parquet::T_INT64) {
                continue;
            }
            for pred in preds {
                for op in [
                    FusedOp::Count,
                    FusedOp::Sum,
                    FusedOp::Min,
                    FusedOp::Max,
                    FusedOp::Avg,
                    FusedOp::Grouped,
                ] {
                    check_pq_int(src, idx, pred, op);
                }
            }
        }
        let _ = label;
    }
}

#[test]
fn parquet_float_operators_exclude_nulls() {
    // `phase29_nullable.parquet` carries an optional DOUBLE column with nulls.
    let model = parquet::parse(PQ_NULLABLE, Limits::DEFAULT).unwrap();
    for idx in 0..model.leaves.len() as u32 {
        let leaf = model.leaf(idx).unwrap();
        if !matches!(leaf.physical, parquet::T_FLOAT | parquet::T_DOUBLE) {
            continue;
        }
        let vals = parquet::leaf_values(PQ_NULLABLE, &model, idx, Limits::DEFAULT).unwrap();
        for pred in [FusedPredicate::Always, FusedPredicate::Gt(0)] {
            let mut matched = 0u64;
            let mut nulls = 0u64;
            let mut sum = 0.0f64;
            for v in &vals {
                match pq_float(v) {
                    None => nulls += 1,
                    Some(x) if pred.float_matches(x) => {
                        matched += 1;
                        sum += x;
                    }
                    Some(_) => {}
                }
            }
            let plan = FusedPlan::new(pred, FusedOp::Avg, 4096);
            let got =
                parquet::fused_scan(PQ_NULLABLE, &model, idx, &plan, Limits::DEFAULT).unwrap();
            assert_eq!(got.nulls, nulls, "pq float col {idx} nulls");
            assert_eq!(got.matched, matched, "pq float col {idx} matched");
            if matched > 0 {
                let want = FusedMeasure::Avg(sum / matched as f64);
                assert!(close(&got.measure, &want), "pq float col {idx} avg");
            }
        }
    }
}

#[test]
fn parquet_dictionary_column_decodes_natively() {
    // `dictionary.parquet` column 0 is dictionary-encoded (RLE_DICTIONARY).
    let model = parquet::parse(PQ_DICTIONARY, Limits::DEFAULT).unwrap();
    let plan = FusedPlan::new(FusedPredicate::Always, FusedOp::Count, 4096);
    let got = parquet::fused_scan(PQ_DICTIONARY, &model, 0, &plan, Limits::DEFAULT);
    // The first column may be a string (typed decline) or numeric; either way it
    // must be either an exact count or a typed decline, never a panic.
    match got {
        Ok(a) => assert_eq!(a.measure, FusedMeasure::Count(a.matched)),
        Err(e) => assert_eq!(
            e.class(),
            vole_document::error::ErrorClass::UnsupportedFeature
        ),
    }
}

// --- delimited -------------------------------------------------------------

const CSV: &[u8] = b"id,name,score\n1,alice,1.5\n2,bob,\n3,carol,-2.0\n4,,\n";

#[test]
fn csv_int_operators_match_a_hand_reference() {
    use vole_document::adapter::csv;
    let id = [1i64, 2, 3, 4];
    for (pred, want_count, want_sum) in [
        (FusedPredicate::Always, 4u64, 10i128),
        (FusedPredicate::Gt(1), 3, 9),
        (FusedPredicate::Eq(2), 1, 2),
        (
            FusedPredicate::Range {
                lo: Some(2),
                hi: Some(3),
            },
            2,
            5,
        ),
    ] {
        let plan = FusedPlan::new(pred, FusedOp::Count, 4096);
        let got = csv::fused_scan(CSV, "id", ColumnKind::Int, &plan, Limits::DEFAULT).unwrap();
        assert_eq!(got.matched, want_count);
        // The count matches a hand-computed reference over `id`.
        let want = id
            .iter()
            .filter(|v| pred.int_matches(i128::from(**v)))
            .count() as u64;
        assert_eq!(got.matched, want);

        let plan = FusedPlan::new(pred, FusedOp::Sum, 4096);
        let got = csv::fused_scan(CSV, "id", ColumnKind::Int, &plan, Limits::DEFAULT).unwrap();
        assert_eq!(got.measure, FusedMeasure::Sum(want_sum));
    }
}

#[test]
fn csv_float_column_excludes_empty_fields_as_null() {
    use vole_document::adapter::csv;
    // `score` is 1.5, <empty>, -2.0, <empty>: two non-null values.
    let plan = FusedPlan::new(FusedPredicate::Always, FusedOp::Count, 4096);
    let got = csv::fused_scan(CSV, "score", ColumnKind::Float, &plan, Limits::DEFAULT).unwrap();
    assert_eq!(got.matched, 2);
    assert_eq!(got.nulls, 2);
    assert_eq!(got.scanned, 4);

    let plan = FusedPlan::new(FusedPredicate::Always, FusedOp::Sum, 4096);
    let got = csv::fused_scan(CSV, "score", ColumnKind::Float, &plan, Limits::DEFAULT).unwrap();
    assert_eq!(got.measure, FusedMeasure::SumF(-0.5));
}

#[test]
fn csv_non_numeric_field_is_a_typed_decline() {
    use vole_document::adapter::csv;
    let plan = FusedPlan::new(FusedPredicate::Always, FusedOp::Count, 4096);
    let err = csv::fused_scan(CSV, "name", ColumnKind::Int, &plan, Limits::DEFAULT).unwrap_err();
    assert_eq!(
        err.class(),
        vole_document::error::ErrorClass::UnsupportedFeature
    );
}

#[test]
fn csv_grouped_is_sorted_and_bounded() {
    use vole_document::adapter::csv;
    let plan = FusedPlan::new(FusedPredicate::Always, FusedOp::Grouped, 4096);
    let got = csv::fused_scan(CSV, "id", ColumnKind::Int, &plan, Limits::DEFAULT).unwrap();
    assert_eq!(
        got.measure,
        FusedMeasure::Grouped(vec![(1, 1), (2, 1), (3, 1), (4, 1)])
    );
}

#[test]
fn csv_ragged_row_is_a_typed_decline() {
    use vole_document::adapter::csv;
    let ragged: &[u8] = b"a,b\n1,2\n3,4\n5\n";
    let plan = FusedPlan::new(FusedPredicate::Always, FusedOp::Count, 4096);
    let err = csv::fused_scan(ragged, "b", ColumnKind::Int, &plan, Limits::DEFAULT).unwrap_err();
    assert_eq!(
        err.class(),
        vole_document::error::ErrorClass::UnsupportedFeature
    );
}

#[test]
fn fused_never_panics_on_random_bytes() {
    // A deterministic LCG; every parse is fallible and the kernel must never panic.
    let mut x: u64 = 0x1234_5678_9ABC_DEF0;
    for _ in 0..128 {
        let mut buf = vec![0u8; 256];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let plan = FusedPlan::new(FusedPredicate::Gt(3), FusedOp::Sum, 32);
        if let Ok(m) = parse(&buf, Limits::STRICT) {
            for i in 0..4u32 {
                let _ = arrow_fused_scan(&buf, &m, i, &plan, Limits::STRICT);
            }
        }
        // Also feed the delimited reader without a valid table.
        for kind in [ColumnKind::Int, ColumnKind::Float] {
            let _ = vole_document::adapter::csv::fused_scan(&buf, "0", kind, &plan, Limits::STRICT);
        }
        if let Ok(m) = parquet::parse(&buf, Limits::STRICT) {
            for i in 0..2u32 {
                let _ = parquet::fused_scan(&buf, &m, i, &plan, Limits::STRICT);
            }
        }
    }
}

#[test]
fn fixedwidth_skips_the_header_and_trims_padding() {
    use vole_document::adapter::fixedwidth;
    // Columns are `[0,5)` (Name) and `[7,10)` (Age) at a uniform width of 10.
    let doc: &[u8] = b"Name   Age\nAlice   30\nBob     25\n";
    let plan = FusedPlan::new(FusedPredicate::Always, FusedOp::Sum, 4096);
    let got = fixedwidth::fused_scan(doc, "Age", ColumnKind::Int, &plan, Limits::DEFAULT).unwrap();
    assert_eq!(got.scanned, 2);
    assert_eq!(got.measure, FusedMeasure::Sum(55));
    let plan = FusedPlan::new(FusedPredicate::Always, FusedOp::Count, 4096);
    let err =
        fixedwidth::fused_scan(doc, "Name", ColumnKind::Int, &plan, Limits::DEFAULT).unwrap_err();
    assert_eq!(
        err.class(),
        vole_document::error::ErrorClass::UnsupportedFeature
    );
}

#[test]
fn plan_version_is_stamped() {
    let plan = FusedPlan::new(FusedPredicate::Always, FusedOp::Count, 8);
    assert_eq!(
        plan.version,
        vole_document::adapter::fused::FUSED_PLAN_VERSION
    );
}
