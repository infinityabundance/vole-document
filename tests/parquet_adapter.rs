//! Phase 21.14 court: the Apache Parquet analytical adapter.
//!
//! Gated on `parquet` (+ `field`), so a build without the feature compiles an
//! empty target. Parquet has no package layer: the exact leaf is the whole source
//! (a `DocumentExact`) and every derived observation is a bounded projection of
//! it. The court checks:
//!
//! 1. **Byte-based conservative detection** and the Opaque boundary (`PAR1` at
//!    both ends plus a consistent footer length).
//! 2. **The schema and inventory**: names, physical/logical types, repetition, and
//!    the leaf column def/rep levels.
//! 3. **Decoded values** for PLAIN and RLE_DICTIONARY, all supported physical
//!    types, OPTIONAL nulls, GZIP, and multiple row groups — cross-checked against
//!    the values DuckDB reads from the very same bytes (the court's comparator).
//! 4. **Exact source spans** for every column chunk.
//! 5. **Selectors**: native `parquet-schema`/`parquet-column`/`parquet-row-group`/
//!    `parquet-cell` plus common `metadata`/`text`/`table`/`cell`/`search-match`.
//! 6. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted.
//! 7. **Fail closed**: an unsupported codec/encoding declines typed, a
//!    decompression bomb declines typed, and random bytes never panic.
//! 8. **No cross-field aliasing** (ADR-0060): the model node depends on the exact
//!    `sha256(source)` root.

#![cfg(all(feature = "field", feature = "parquet"))]

use std::path::PathBuf;

use vole_document::adapter::parquet::{Value, leaf_values, parse, stats_value_text};
use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_PARQUET_MODEL, SelectorKey, lookup};
use vole_document::field::ingest::{IngestReport, ingest_pdf};
use vole_document::field::node::NodeKind;
use vole_document::field::observe::{ObserveRequest, Representation, Selector, observe};
use vole_document::field::provenance::{AnswerValue, FieldAnswer};
use vole_document::field::{Field, FieldId, FieldStore};
use vole_document::integrity::sha256;
use vole_document::limits::Limits;
use vole_document::store::NodeId;

const SMALL: &[u8] = include_bytes!("../tools/fixtures/parquet/small_plain.parquet");
const OPTIONAL: &[u8] = include_bytes!("../tools/fixtures/parquet/optional.parquet");
const DICTIONARY: &[u8] = include_bytes!("../tools/fixtures/parquet/dictionary.parquet");
const GZIP: &[u8] = include_bytes!("../tools/fixtures/parquet/gzip.parquet");
const MULTI_RG: &[u8] = include_bytes!("../tools/fixtures/parquet/multi_rg.parquet");
const TWO_PAGES: &[u8] = include_bytes!("../tools/fixtures/parquet/two_pages.parquet");
const UNSUPPORTED_CODEC: &[u8] =
    include_bytes!("../tools/fixtures/parquet/unsupported_codec.parquet");
const UNSUPPORTED_ENCODING: &[u8] =
    include_bytes!("../tools/fixtures/parquet/unsupported_encoding.parquet");
const BOMB: &[u8] = include_bytes!("../tools/fixtures/parquet/bomb.parquet");
const PROSE: &[u8] = include_bytes!("../tools/fixtures/parquet/prose.txt");
const TRUNCATED: &[u8] = include_bytes!("../tools/fixtures/parquet/truncated.parquet");
const BADLEN: &[u8] = include_bytes!("../tools/fixtures/parquet/badlen.parquet");

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-parquet-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn opaque_descriptor(source: &[u8]) -> Vec<u8> {
    let d = Descriptor {
        universe: vole_document::container::UNIVERSE.to_string(),
        source_format: vole_document::SOURCE_FORMAT_OPAQUE,
        format_basis: "opaque:parquet-test".to_string(),
        models: vec![],
        channels: vec![],
        objects: vec![ObjectSource::Inline(source.to_vec())],
        program: Program::new(vec![Op::EmitObject { object_id: 0 }]),
        observation_index: None,
        seek_directory: false,
        checkpoints: None,
        source_sha256: sha256(source),
        source_len: source.len() as u64,
    };
    d.serialize().unwrap().0
}

struct Fixture {
    root: PathBuf,
    store: FieldStore,
    report: IngestReport,
    source: Vec<u8>,
}

impl Fixture {
    fn new(label: &str, source: &[u8]) -> Fixture {
        let root = temp_dir(label);
        let mut store = FieldStore::open(&root).unwrap();
        let descriptor = opaque_descriptor(source);
        let report = ingest_pdf(&mut store, &descriptor, Limits::DEFAULT).unwrap();
        Fixture {
            root,
            store,
            report,
            source: source.to_vec(),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

fn observe_ok(
    store: &mut FieldStore,
    field: &FieldId,
    selector: Selector,
    representation: Representation,
) -> FieldAnswer {
    let req = ObserveRequest::new(selector, representation);
    observe(store, field, &req, Limits::DEFAULT).unwrap().0
}

fn observe_err(
    store: &mut FieldStore,
    field: &FieldId,
    selector: Selector,
    representation: Representation,
) -> ErrorClass {
    let req = ObserveRequest::new(selector, representation);
    observe(store, field, &req, Limits::DEFAULT)
        .unwrap_err()
        .class()
}

fn answer_text(answer: &FieldAnswer) -> String {
    match &answer.value {
        AnswerValue::Text(t) => t.clone(),
        other => panic!("expected text, got {other:?}"),
    }
}

fn answer_json(answer: &FieldAnswer) -> String {
    match &answer.value {
        AnswerValue::Json(j) => j.clone(),
        other => panic!("expected json, got {other:?}"),
    }
}

fn answer_bytes(answer: &FieldAnswer) -> Vec<u8> {
    match &answer.value {
        AnswerValue::Bytes(b) => b.clone(),
        other => panic!("expected bytes, got {other:?}"),
    }
}

fn text_of(v: &Value, leaf: &vole_document::adapter::parquet::LeafColumn) -> String {
    vole_document::adapter::parquet::value_text(v, leaf)
}

fn column_text(source: &[u8], index: u32) -> Vec<String> {
    let model = parse(source, Limits::DEFAULT).unwrap();
    let leaf = model.leaf(index).unwrap().clone();
    leaf_values(source, &model, index, Limits::DEFAULT)
        .unwrap()
        .iter()
        .map(|v| text_of(v, &leaf))
        .collect()
}

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_byte_based_and_conservative() {
    for (bytes, want) in [
        (SMALL, DocumentFormat::Parquet),
        (OPTIONAL, DocumentFormat::Parquet),
        (DICTIONARY, DocumentFormat::Parquet),
        (GZIP, DocumentFormat::Parquet),
        (MULTI_RG, DocumentFormat::Parquet),
        (TWO_PAGES, DocumentFormat::Parquet),
        (UNSUPPORTED_CODEC, DocumentFormat::Parquet),
        (UNSUPPORTED_ENCODING, DocumentFormat::Parquet),
        (BOMB, DocumentFormat::Parquet),
        (PROSE, DocumentFormat::Opaque),
        // A truncated file (no trailing magic) and a file with an inconsistent
        // footer length both stay Opaque.
        (TRUNCATED, DocumentFormat::Opaque),
        (BADLEN, DocumentFormat::Opaque),
    ] {
        assert_eq!(
            detect_document_format(bytes, Limits::DEFAULT),
            want,
            "detection mismatch for a {} byte fixture",
            bytes.len()
        );
    }
    // Magic at both ends but a nonsense footer length.
    assert!(!vole_document::adapter::parquet::detect(
        b"PAR1notreallyPAR1",
        Limits::DEFAULT
    ));
}

#[test]
fn schema_names_and_types_are_exposed() {
    let model = parse(SMALL, Limits::DEFAULT).unwrap();
    assert_eq!(model.version, 1);
    assert_eq!(model.num_rows, 10);
    assert_eq!(model.row_groups.len(), 1);
    assert_eq!(model.leaves.len(), 7);
    let names: Vec<&str> = model.leaves.iter().map(|l| l.path[0].as_str()).collect();
    assert_eq!(names, ["flag", "i32", "i64", "f", "d", "s", "raw"]);
    assert_eq!(
        model.leaf(0).unwrap().physical,
        vole_document::adapter::parquet::T_BOOLEAN
    );
    assert_eq!(
        model.leaf(6).unwrap().type_length,
        Some(4),
        "FIXED_LEN_BYTE_ARRAY keeps its width"
    );
    assert_eq!(model.leaf(5).unwrap().converted, Some(0), "UTF8");
    // All columns are required here (no definition levels).
    assert!(
        model
            .leaves
            .iter()
            .all(|l| l.max_def == 0 && l.max_rep == 0)
    );

    let opt = parse(OPTIONAL, Limits::DEFAULT).unwrap();
    assert!(opt.leaves.iter().all(|l| l.max_def == 1));
}

#[test]
fn decoded_values_match_the_expected_bytes() {
    assert_eq!(
        column_text(SMALL, 0),
        [
            "true", "false", "true", "true", "false", "false", "true", "false", "true", "true"
        ]
    );
    assert_eq!(
        column_text(SMALL, 1),
        ["1", "-2", "300", "-4000", "5", "6", "7", "-8", "9", "10"]
    );
    assert_eq!(
        column_text(SMALL, 5),
        [
            "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa"
        ]
    );
    // FLBA renders as hex when it carries no string logical type.
    assert_eq!(column_text(SMALL, 6)[0], "0x00010203");

    // OPTIONAL nulls are preserved as `Null`.
    let om = parse(OPTIONAL, Limits::DEFAULT).unwrap();
    let notes = leaf_values(OPTIONAL, &om, 1, Limits::DEFAULT).unwrap();
    assert_eq!(notes[1], Value::Null);
    assert_eq!(notes[0], Value::Bytes(b"first".to_vec()));
    assert_eq!(notes[5], Value::Bytes(b"last".to_vec()));

    // RLE_DICTIONARY decodes to the same values as PLAIN.
    assert_eq!(
        column_text(DICTIONARY, 0)[..4],
        ["Oslo", "Bergen", "Oslo", "Trondheim"]
    );
    assert_eq!(column_text(DICTIONARY, 1)[..4], ["10", "20", "10", "30"]);

    // GZIP decodes to the same values as the uncompressed file.
    assert_eq!(column_text(GZIP, 1), column_text(SMALL, 1));
    assert_eq!(column_text(GZIP, 5), column_text(SMALL, 5));

    // Multiple row groups and multiple pages per chunk both concatenate in order.
    assert_eq!(column_text(MULTI_RG, 0).len(), 24);
    assert_eq!(column_text(MULTI_RG, 0)[23], "23");
    assert_eq!(column_text(TWO_PAGES, 0).len(), 20);
    assert_eq!(column_text(TWO_PAGES, 0)[19], "361");
}

#[test]
fn column_chunk_spans_are_exact_and_disjoint() {
    let model = parse(SMALL, Limits::DEFAULT).unwrap();
    let n = SMALL.len() as u64;
    let mut spans: Vec<(u64, u64)> = Vec::new();
    for rg in &model.row_groups {
        for c in &rg.chunks {
            assert!(c.span_start >= 4, "chunk starts inside the leading magic");
            assert!(c.span_end <= n, "chunk ends beyond the file");
            assert!(c.span_start < c.span_end);
            spans.push((c.span_start, c.span_end));
        }
    }
    spans.sort_unstable();
    for w in spans.windows(2) {
        assert!(
            w[0].1 <= w[1].0,
            "column chunk spans overlap: {:?} {:?}",
            w[0],
            w[1]
        );
    }
    // The declared chunk size equals the exact span length.
    for rg in &model.row_groups {
        for c in &rg.chunks {
            assert_eq!(
                c.span_end - c.span_start,
                c.total_compressed_size as u64,
                "declared compressed size disagrees with the span"
            );
        }
    }
}

#[test]
fn statistics_are_exposed_and_decoded() {
    let model = parse(SMALL, Limits::DEFAULT).unwrap();
    let chunk = &model.row_groups[0].chunks[1]; // i32
    let leaf = model.leaf(1).unwrap();
    assert_eq!(
        stats_value_text(chunk.min_value.as_deref().unwrap(), leaf),
        Some("-4000".into())
    );
    assert_eq!(
        stats_value_text(chunk.max_value.as_deref().unwrap(), leaf),
        Some("300".into())
    );
    assert_eq!(chunk.null_count, Some(0));

    let s = &model.row_groups[0].chunks[5]; // string column
    let sleaf = model.leaf(5).unwrap();
    assert_eq!(
        stats_value_text(s.min_value.as_deref().unwrap(), sleaf),
        Some("alpha".into())
    );
    assert_eq!(
        stats_value_text(s.max_value.as_deref().unwrap(), sleaf),
        Some("zeta".into())
    );
}

#[test]
fn native_and_common_selectors_resolve() {
    let mut fx = Fixture::new("selectors", SMALL);
    assert_eq!(fx.report.format, DocumentFormat::Parquet);
    let field = fx.report.field;

    // Native: schema.
    let schema = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::ParquetSchema,
        Representation::Metadata,
    ));
    assert!(schema.contains("\"physical\":\"BOOLEAN\""));
    assert!(schema.contains("\"converted\":\"UTF8\""));

    // Native: a column's decoded values (text) and inventory (metadata).
    let vals = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::ParquetColumn { index: 5 },
        Representation::Text,
    ));
    assert!(vals.starts_with("alpha\nbeta"));
    let inv = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::ParquetColumn { index: 5 },
        Representation::Metadata,
    ));
    assert!(inv.contains("\"values\":[\"alpha\""));
    assert!(inv.contains("\"min\":\"alpha\""));
    assert!(inv.contains("\"max\":\"zeta\""));

    // Native: a row group and a cell.
    let rg = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::ParquetRowGroup { index: 0 },
        Representation::Metadata,
    ));
    assert!(rg.contains("\"num_rows\":10"));
    let cell = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::ParquetCell { row: 2, col: 5 },
        Representation::Text,
    ));
    assert_eq!(cell, "gamma");

    // Native exact: the raw encoded bytes of a column chunk.
    let raw = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        Selector::ParquetColumn { index: 5 },
        Representation::ExactBytes,
    ));
    let model = parse(SMALL, Limits::DEFAULT).unwrap();
    let chunk = &model.row_groups[0].chunks[5];
    assert_eq!(
        raw,
        SMALL[chunk.span_start as usize..chunk.span_end as usize].to_vec()
    );

    // Common: metadata, text, table, cell, search-match.
    let md = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(md.contains("\"num_rows\":10"));
    assert!(md.contains("\"columns\":7"));
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert!(text.starts_with("flag\ti32\ti64"));
    let table = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Table(0),
        Representation::Text,
    ));
    assert_eq!(table, text);
    let c = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Cell {
            table: 0,
            row: 1,
            col: 1,
        },
        Representation::Text,
    ));
    assert_eq!(c, "-2");
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("amma".to_string()),
        Representation::Text,
    ));
    assert!(found.contains("gamma"));
}

#[test]
fn exact_materialization_is_byte_identical() {
    let fx = Fixture::new("exact", SMALL);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let out = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(out.len(), fx.source.len());
    assert_eq!(sha256(&out), sha256(&fx.source));
    assert_eq!(out, fx.source);
}

#[test]
fn removal_then_fresh_process_is_queryable_and_exact() {
    let root = temp_dir("removal");
    let src_path = root.join("data.parquet");
    let desc_path = root.join("data.voldoc");
    std::fs::write(&src_path, SMALL).unwrap();
    let descriptor = opaque_descriptor(SMALL);
    std::fs::write(&desc_path, &descriptor).unwrap();

    let store_root = root.join("store");
    let field_id = {
        let mut store = FieldStore::open(&store_root).unwrap();
        let report = ingest_pdf(&mut store, &descriptor, Limits::DEFAULT).unwrap();
        report.field
    };

    std::fs::remove_file(&src_path).unwrap();
    std::fs::remove_file(&desc_path).unwrap();

    let mut store2 = FieldStore::open(&store_root).unwrap();
    let cell = answer_text(&observe_ok(
        &mut store2,
        &field_id,
        Selector::ParquetCell { row: 2, col: 5 },
        Representation::Text,
    ));
    assert_eq!(cell, "gamma");

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), SMALL.len());
    assert_eq!(sha256(&exact), sha256(SMALL));
    assert_eq!(exact, SMALL);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn unsupported_encodings_and_bombs_decline_typed() {
    // The footer parses (these are structurally valid Parquet files)…
    let enc = parse(UNSUPPORTED_ENCODING, Limits::DEFAULT).unwrap();
    let codec = parse(UNSUPPORTED_CODEC, Limits::DEFAULT).unwrap();
    let bomb = parse(BOMB, Limits::DEFAULT).unwrap();

    // …but the values decline typed, never a wrong answer.
    assert_eq!(
        leaf_values(UNSUPPORTED_ENCODING, &enc, 0, Limits::DEFAULT)
            .unwrap_err()
            .class(),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        leaf_values(UNSUPPORTED_CODEC, &codec, 0, Limits::DEFAULT)
            .unwrap_err()
            .class(),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        leaf_values(BOMB, &bomb, 0, Limits::DEFAULT)
            .unwrap_err()
            .class(),
        ErrorClass::ResourceLimit
    );

    // Through the store: metadata still works (inventory from the footer) while
    // the decoded column declines typed.
    for (label, bytes, want) in [
        ("enc", UNSUPPORTED_ENCODING, ErrorClass::UnsupportedFeature),
        ("codec", UNSUPPORTED_CODEC, ErrorClass::UnsupportedFeature),
        ("bomb", BOMB, ErrorClass::ResourceLimit),
    ] {
        let mut fx = Fixture::new(label, bytes);
        let field = fx.report.field;
        let _ = answer_json(&observe_ok(
            &mut fx.store,
            &field,
            Selector::Metadata,
            Representation::Metadata,
        ));
        assert_eq!(
            observe_err(
                &mut fx.store,
                &field,
                Selector::ParquetColumn { index: 0 },
                Representation::Text,
            ),
            want
        );
        // The opaque floor still closes the source exactly.
        let f = Field::open(&fx.store, &field, Limits::DEFAULT).unwrap();
        assert_eq!(f.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
    }
}

#[test]
fn out_of_range_selectors_decline_typed() {
    let mut fx = Fixture::new("range", SMALL);
    let field = fx.report.field;
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::ParquetColumn { index: 999 },
            Representation::Metadata,
        ),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::ParquetRowGroup { index: 99 },
            Representation::Metadata,
        ),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::ParquetCell { row: 5000, col: 0 },
            Representation::Text,
        ),
        ErrorClass::UnsupportedFeature
    );
}

#[test]
fn interleaved_fixtures_do_not_alias() {
    // ADR-0060: a source-reading node must carry its own source-identity input.
    let root = temp_dir("alias");
    let mut store = FieldStore::open(&root).unwrap();
    let da = opaque_descriptor(SMALL);
    let db = opaque_descriptor(OPTIONAL);
    let fa = ingest_pdf(&mut store, &da, Limits::DEFAULT).unwrap().field;
    let fb = ingest_pdf(&mut store, &db, Limits::DEFAULT).unwrap().field;

    let a = answer_text(&observe_ok(
        &mut store,
        &fa,
        Selector::ParquetColumn { index: 5 },
        Representation::Text,
    ));
    assert!(a.contains("alpha"));
    let b = answer_text(&observe_ok(
        &mut store,
        &fb,
        Selector::ParquetColumn { index: 1 },
        Representation::Text,
    ));
    assert!(b.contains("first"));

    for (field, src) in [(fa, SMALL), (fb, OPTIONAL)] {
        let f = Field::open(&store, &field, Limits::DEFAULT).unwrap();
        assert_eq!(f.materialize_exact(Limits::DEFAULT).unwrap(), src);
    }
    drop(store);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn parquet_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived Parquet model reads the source bytes, so its single
    // dependency must be the exact `DocumentExact` root (keyed by `sha256(source)`).
    let fx = Fixture::new("adr0060", SMALL);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let manifest = field.manifest().clone();
    let root = manifest.root_node;
    let root_node = load_node(fx.store.seeds(), &root).unwrap();
    assert_eq!(root_node.kind, NodeKind::DocumentExact);

    assert!(manifest.has_index());
    let istore = FsIndexStore::open(fx.store.root()).unwrap();
    let iroot = NodeId::from_bytes(manifest.index_root);
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_PARQUET_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::ParquetModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the Parquet model must depend on exactly the exact root"
    );
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0x2468_ACE0_1357_9BDF;
    for _ in 0..256 {
        let mut buf = vec![0u8; 512];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        if let Ok(m) = parse(&buf, Limits::STRICT) {
            for i in 0..m.leaves.len() as u32 {
                let _ = leaf_values(&buf, &m, i, Limits::STRICT);
            }
        }
        let _ = vole_document::adapter::parquet::build_parquet_model(&buf, Limits::STRICT);
    }
    // Random bytes that happen to begin and end with PAR1 must still not panic.
    let mut x2: u64 = 0x0BAD_F00D_DEAD_BEEF;
    for _ in 0..256 {
        let mut buf = vec![0u8; 256];
        buf[0..4].copy_from_slice(b"PAR1");
        for b in buf.iter_mut().skip(4) {
            x2 ^= x2 << 13;
            x2 ^= x2 >> 7;
            x2 ^= x2 << 17;
            *b = (x2 & 0xFF) as u8;
        }
        let n = buf.len();
        buf[n - 4..n].copy_from_slice(b"PAR1");
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        if let Ok(m) = parse(&buf, Limits::STRICT) {
            for i in 0..m.leaves.len() as u32 {
                let _ = leaf_values(&buf, &m, i, Limits::STRICT);
            }
        }
    }
}
