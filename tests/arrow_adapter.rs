//! Phase 21.16 court: the Apache Arrow IPC analytical adapter.
//!
//! Gated on `arrow` (+ `field`), so a build without the feature compiles an empty
//! target. Arrow has no package layer: the exact leaf is the whole source (a
//! `DocumentExact`) and every derived observation is a bounded projection of it.
//! The court checks:
//!
//! 1. **Byte-based conservative detection** and the Opaque boundary.
//! 2. **The schema** (field names, type tags/parameters, nullability, children).
//! 3. **Decoded values** for the supported types, including OPTIONAL nulls, with
//!    checked arithmetic and exact source spans.
//! 4. **Selectors**: native `arrow-schema`/`arrow-column`/`arrow-batch`/`arrow-cell`
//!    plus common `metadata`/`text`/`table`/`cell`/`search-match`.
//! 5. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted.
//! 6. **Fail closed**: an unsupported type, a compressed body, a bomb, and a
//!    malformed Flatbuffers metadata all decline typed, never a wrong answer.
//! 7. **No cross-field aliasing** (ADR-0060): the model node depends on the exact
//!    `sha256(source)` root.

#![cfg(all(feature = "field", feature = "arrow"))]

use std::path::PathBuf;

use vole_document::adapter::arrow::{Value, build_arrow_model, column_values, parse};
use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_ARROW_MODEL, SelectorKey, lookup};
use vole_document::field::ingest::{IngestReport, ingest_pdf};
use vole_document::field::node::NodeKind;
use vole_document::field::observe::{ObserveRequest, Representation, Selector, observe};
use vole_document::field::provenance::{AnswerValue, FieldAnswer};
use vole_document::field::{Field, FieldId, FieldStore};
use vole_document::integrity::sha256;
use vole_document::limits::Limits;
use vole_document::store::NodeId;

const PRIMITIVES: &[u8] = include_bytes!("../tools/fixtures/arrow/primitives.arrow");
const NULLABLE: &[u8] = include_bytes!("../tools/fixtures/arrow/nullable.arrow");
const STRINGS: &[u8] = include_bytes!("../tools/fixtures/arrow/strings.arrow");
const TEMPORAL: &[u8] = include_bytes!("../tools/fixtures/arrow/temporal.arrow");
const MULTI_BATCH: &[u8] = include_bytes!("../tools/fixtures/arrow/multi_batch.arrow");
const FSB: &[u8] = include_bytes!("../tools/fixtures/arrow/fsb.arrow");
const STREAM: &[u8] = include_bytes!("../tools/fixtures/arrow/stream.arrow");
const UNSUPPORTED_DECIMAL: &[u8] =
    include_bytes!("../tools/fixtures/arrow/unsupported_decimal.arrow");
const UNSUPPORTED_NESTED: &[u8] =
    include_bytes!("../tools/fixtures/arrow/unsupported_nested.arrow");
const UNSUPPORTED_DICTIONARY: &[u8] =
    include_bytes!("../tools/fixtures/arrow/unsupported_dictionary.arrow");
const UNSUPPORTED_COMPRESSED: &[u8] =
    include_bytes!("../tools/fixtures/arrow/unsupported_compressed.arrow");
const BOMB: &[u8] = include_bytes!("../tools/fixtures/arrow/bomb.arrow");
const MALFORMED: &[u8] = include_bytes!("../tools/fixtures/arrow/malformed_flatbuf.arrow");
const PROSE: &[u8] = include_bytes!("../tools/fixtures/arrow/prose.txt");
const MAGIC_ONLY: &[u8] = include_bytes!("../tools/fixtures/arrow/magic_only.bin");
const TRUNCATED: &[u8] = include_bytes!("../tools/fixtures/arrow/truncated.arrow");
const BADLEN: &[u8] = include_bytes!("../tools/fixtures/arrow/badlen.arrow");

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-arrow-{label}-{}-{}",
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
        format_basis: "opaque:arrow-test".to_string(),
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

fn column_text(source: &[u8], index: u32) -> Vec<String> {
    let model = parse(source, Limits::DEFAULT).unwrap();
    let leaf = model.leaf(index).unwrap().clone();
    column_values(source, &model, index, Limits::DEFAULT)
        .unwrap()
        .iter()
        .map(|v| vole_document::adapter::arrow::value_text(v, &leaf))
        .collect()
}

fn col(name: &str) -> Selector {
    Selector::ArrowColumn {
        spec: name.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_byte_based_and_conservative() {
    for (bytes, want) in [
        (PRIMITIVES, DocumentFormat::ArrowIpc),
        (NULLABLE, DocumentFormat::ArrowIpc),
        (STRINGS, DocumentFormat::ArrowIpc),
        (TEMPORAL, DocumentFormat::ArrowIpc),
        (MULTI_BATCH, DocumentFormat::ArrowIpc),
        (FSB, DocumentFormat::ArrowIpc),
        (STREAM, DocumentFormat::ArrowIpc),
        (UNSUPPORTED_DECIMAL, DocumentFormat::ArrowIpc),
        (UNSUPPORTED_NESTED, DocumentFormat::ArrowIpc),
        (UNSUPPORTED_DICTIONARY, DocumentFormat::ArrowIpc),
        (UNSUPPORTED_COMPRESSED, DocumentFormat::ArrowIpc),
        (BOMB, DocumentFormat::ArrowIpc),
        (MALFORMED, DocumentFormat::ArrowIpc),
        (PROSE, DocumentFormat::Opaque),
        (MAGIC_ONLY, DocumentFormat::Opaque),
        // A truncated prefix and a file with an inconsistent footer length both
        // stay Opaque.
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
    // A trailing magic with a nonsense footer length is not Arrow.
    assert!(!vole_document::adapter::arrow::detect(
        b"ARROW1xxxxxxxxxxARROW1",
        Limits::DEFAULT
    ));
}

#[test]
fn schema_names_and_types_are_exposed() {
    let model = parse(PRIMITIVES, Limits::DEFAULT).unwrap();
    assert!(!model.stream);
    assert_eq!(model.endianness, 0);
    assert_eq!(model.leaves.len(), 12);
    let names: Vec<&str> = model.leaves.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f16", "f32", "f64", "flag"
        ]
    );
    // Signed vs unsigned Int widths are recorded.
    assert!(model.leaf(0).unwrap().int_signed);
    assert_eq!(model.leaf(0).unwrap().width, 1);
    assert!(!model.leaf(4).unwrap().int_signed);
    assert_eq!(model.leaf(7).unwrap().width, 8);
    // Float precisions.
    assert_eq!(model.leaf(8).unwrap().float_precision, 0);
    assert_eq!(model.leaf(9).unwrap().float_precision, 1);
    assert_eq!(model.leaf(10).unwrap().float_precision, 2);
    // The string columns use a 3-buffer layout.
    let m = parse(STRINGS, Limits::DEFAULT).unwrap();
    assert_eq!(m.leaf(0).unwrap().offsets_slot, 1);
    assert_eq!(m.leaf(0).unwrap().data_slot, 2);
    assert!(m.leaf(0).unwrap().is_utf8);
    assert!(m.leaf(1).unwrap().large_offsets);
    // The stream fixture carries no footer.
    let s = parse(STREAM, Limits::DEFAULT).unwrap();
    assert!(s.stream);
    assert_eq!(s.batches.len(), 2);
}

#[test]
fn decoded_values_match_the_expected_bytes() {
    assert_eq!(
        column_text(PRIMITIVES, 0),
        ["-4", "-3", "-2", "-1", "0", "1", "2", "3"]
    );
    assert_eq!(
        column_text(PRIMITIVES, 2),
        [
            "-4000", "-3000", "-2000", "-1000", "0", "1000", "2000", "3000"
        ]
    );
    assert_eq!(
        column_text(PRIMITIVES, 3),
        [
            "-4000000000",
            "-3000000000",
            "-2000000000",
            "-1000000000",
            "0",
            "1000000000",
            "2000000000",
            "3000000000"
        ]
    );
    // Unsigned 64-bit values above i64::MAX render correctly.
    assert_eq!(column_text(PRIMITIVES, 7)[0], "9223372036854775808");
    assert_eq!(column_text(PRIMITIVES, 7)[7], "9223372036854775815");
    // Half/single/double floats.
    assert_eq!(column_text(PRIMITIVES, 8)[0], "0.5");
    assert_eq!(column_text(PRIMITIVES, 8)[7], "7.5");
    assert_eq!(
        column_text(PRIMITIVES, 9),
        ["-2", "-0.5", "1", "2.5", "4", "5.5", "7", "8.5"]
    );
    assert_eq!(column_text(PRIMITIVES, 10)[0], "-1");
    // Boolean bit-packing.
    assert_eq!(
        column_text(PRIMITIVES, 11),
        [
            "true", "false", "false", "true", "false", "false", "true", "false"
        ]
    );

    // OPTIONAL nulls are preserved as `Null`.
    let nm = parse(NULLABLE, Limits::DEFAULT).unwrap();
    let notes = column_values(NULLABLE, &nm, 1, Limits::DEFAULT).unwrap();
    assert_eq!(notes[0], Value::Bytes(b"first".to_vec()));
    assert_eq!(notes[1], Value::Null);
    assert_eq!(notes[5], Value::Bytes(b"last".to_vec()));
    let scores = column_values(NULLABLE, &nm, 2, Limits::DEFAULT).unwrap();
    assert_eq!(scores[0], Value::F64(1.5));
    assert_eq!(scores[1], Value::Null);
    assert_eq!(scores[3], Value::F64(3.25));

    // Utf8/LargeUtf8/Binary/LargeBinary.
    assert_eq!(column_text(STRINGS, 0)[0], "alpha");
    assert_eq!(column_text(STRINGS, 0)[1], "");
    assert_eq!(column_text(STRINGS, 0)[3].len(), 300);
    assert_eq!(
        column_text(STRINGS, 1),
        ["one", "two", "three", "four", "five"]
    );
    assert_eq!(column_text(STRINGS, 2)[0], "0x0001"); // binary renders as hex
    assert_eq!(column_text(STRINGS, 3)[0], "0x78");
    assert_eq!(column_text(STRINGS, 3)[3], "0x");

    // Temporal types decode as raw integers; Date32 is 4 bytes wide.
    assert_eq!(column_text(TEMPORAL, 0)[0], "19000");
    assert_eq!(column_text(TEMPORAL, 1)[0], "1700000000000000");
    assert_eq!(column_text(TEMPORAL, 2)[0], "3600000000");
    assert_eq!(column_text(TEMPORAL, 3)[1], "1000000");

    // FixedSizeBinary.
    assert_eq!(column_text(FSB, 0)[0], "0x00010203");

    // Multiple record batches concatenate in order.
    let multi = column_text(MULTI_BATCH, 0);
    assert_eq!(multi.len(), 15);
    assert_eq!(multi[14], "14");
    assert_eq!(column_text(MULTI_BATCH, 1)[14], "row-14");

    // The stream format decodes the same way.
    assert_eq!(column_text(STREAM, 0), ["1", "2", "3", "4", "5"]);
    assert_eq!(column_text(STREAM, 1), ["x", "null", "z", "p", "q"]);
}

#[test]
fn batch_spans_are_exact_and_buffers_are_in_bounds() {
    let model = parse(MULTI_BATCH, Limits::DEFAULT).unwrap();
    let n = MULTI_BATCH.len() as u64;
    let mut prev_end = 8u64;
    for b in &model.batches {
        assert!(b.span_start >= 8);
        assert!(b.span_end <= n);
        assert!(b.span_start < b.span_end);
        assert!(b.span_start >= prev_end, "batch messages must not overlap");
        prev_end = b.span_end;
        // k (Int32) = 2 buffers; label (Utf8) = 3 buffers.
        assert_eq!(b.num_buffers, 5);
        assert_eq!(b.num_nodes, 2);
    }
    // A column's exact byte span lies within the source.
    let span = vole_document::adapter::arrow::column_span(MULTI_BATCH, &model, 0, Limits::DEFAULT)
        .unwrap()
        .unwrap();
    assert!(span.1 <= n && span.0 < span.1);
}

#[test]
fn native_and_common_selectors_resolve() {
    let mut fx = Fixture::new("selectors", PRIMITIVES);
    assert_eq!(fx.report.format, DocumentFormat::ArrowIpc);
    let field = fx.report.field;

    // Native: schema.
    let schema = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::ArrowSchema,
        Representation::Metadata,
    ));
    assert!(schema.contains("\"type\":\"Int(32, signed)\""));
    assert!(schema.contains("\"type\":\"FloatingPoint(DOUBLE)\""));
    assert!(schema.contains("\"format\":\"arrow\""));

    // Native: a column's decoded values (text) and inventory (metadata).
    let vals = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        col("i32"),
        Representation::Text,
    ));
    assert_eq!(vals.lines().next().unwrap(), "-4000");
    let inv = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        col("i32"),
        Representation::Metadata,
    ));
    assert!(inv.contains("\"values\":[\"-4000\""));
    assert!(inv.contains("\"supported\":true"));

    // A column addressed by 0-based index resolves to the same column.
    let by_index = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::ArrowColumn {
            spec: "2".to_string(),
        },
        Representation::Text,
    ));
    assert_eq!(by_index, vals);

    // Native: a record batch and a cell.
    let batch = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::ArrowBatch { index: 0 },
        Representation::Metadata,
    ));
    assert!(batch.contains("\"rows\":8"));
    assert!(batch.contains("\"buffers\":24"));
    let cell = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::ArrowCell {
            row: 2,
            col: "i32".to_string(),
        },
        Representation::Text,
    ));
    assert_eq!(cell, "-2000");

    // Native exact: the raw bytes of a column's buffers.
    let raw = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        col("i32"),
        Representation::ExactBytes,
    ));
    assert!(
        raw.starts_with(&[0x60, 0xf0, 0xff, 0xff]),
        "i32 = -4000 little-endian"
    );

    // Common: metadata, text, table, cell, search-match.
    let md = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(md.contains("\"columns\":12"));
    assert!(md.contains("\"batches\":1"));
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert!(text.starts_with("i8\ti16\ti32"));
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
            col: 2,
        },
        Representation::Text,
    ));
    assert_eq!(c, "-3000");
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("2000".to_string()),
        Representation::Text,
    ));
    assert!(found.contains("2000"));
}

#[test]
fn exact_materialization_is_byte_identical() {
    let fx = Fixture::new("exact", PRIMITIVES);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let out = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(out.len(), fx.source.len());
    assert_eq!(sha256(&out), sha256(&fx.source));
    assert_eq!(out, fx.source);
}

#[test]
fn removal_then_fresh_process_is_queryable_and_exact() {
    let root = temp_dir("removal");
    let src_path = root.join("data.arrow");
    let desc_path = root.join("data.voldoc");
    std::fs::write(&src_path, PRIMITIVES).unwrap();
    let descriptor = opaque_descriptor(PRIMITIVES);
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
        Selector::ArrowCell {
            row: 1,
            col: "i64".to_string(),
        },
        Representation::Text,
    ));
    assert_eq!(cell, "-3000000000");

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), PRIMITIVES.len());
    assert_eq!(sha256(&exact), sha256(PRIMITIVES));
    assert_eq!(exact, PRIMITIVES);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn unsupported_types_and_compression_decline_typed() {
    // The metadata parses (these are structurally valid Arrow files)…
    for (label, bytes, idx, want) in [
        (
            "decimal",
            UNSUPPORTED_DECIMAL,
            0u32,
            ErrorClass::UnsupportedFeature,
        ),
        (
            "nested",
            UNSUPPORTED_NESTED,
            0,
            ErrorClass::UnsupportedFeature,
        ),
        (
            "dictionary",
            UNSUPPORTED_DICTIONARY,
            0,
            ErrorClass::UnsupportedFeature,
        ),
        (
            "compressed",
            UNSUPPORTED_COMPRESSED,
            0,
            ErrorClass::UnsupportedFeature,
        ),
        ("bomb", BOMB, 0, ErrorClass::ResourceLimit),
    ] {
        let model = parse(bytes, Limits::DEFAULT).unwrap();
        assert_eq!(
            column_values(bytes, &model, idx, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            want,
            "decline mismatch for {label}"
        );
        // Through the store: metadata still works (inventory) while the decoded
        // column declines typed.
        let mut fx = Fixture::new(label, bytes);
        let field = fx.report.field;
        let _ = answer_json(&observe_ok(
            &mut fx.store,
            &field,
            Selector::Metadata,
            Representation::Metadata,
        ));
        assert_eq!(
            observe_err(&mut fx.store, &field, col("0"), Representation::Text),
            want,
            "observation decline mismatch for {label}"
        );
        // The opaque floor still closes the source exactly.
        let f = Field::open(&fx.store, &field, Limits::DEFAULT).unwrap();
        assert_eq!(f.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
    }
}

#[test]
fn malformed_flatbuffers_decline_typed_never_panic() {
    // Detection accepts it (a consistent trailing footer), but the Flatbuffers
    // metadata is garbage: parsing must be a typed decline, never a panic.
    assert!(vole_document::adapter::arrow::detect(
        MALFORMED,
        Limits::DEFAULT
    ));
    assert_eq!(
        parse(MALFORMED, Limits::DEFAULT).unwrap_err().class(),
        ErrorClass::InvalidArrowStructure
    );
    assert_eq!(
        build_arrow_model(MALFORMED, Limits::DEFAULT)
            .unwrap_err()
            .class(),
        ErrorClass::InvalidArrowStructure
    );
}

#[test]
fn out_of_range_selectors_decline_typed() {
    let mut fx = Fixture::new("range", PRIMITIVES);
    let field = fx.report.field;
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::ArrowColumn {
                spec: "nope".to_string(),
            },
            Representation::Metadata,
        ),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::ArrowBatch { index: 99 },
            Representation::Metadata,
        ),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::ArrowCell {
                row: 5000,
                col: "i32".to_string(),
            },
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
    let da = opaque_descriptor(PRIMITIVES);
    let db = opaque_descriptor(NULLABLE);
    let fa = ingest_pdf(&mut store, &da, Limits::DEFAULT).unwrap().field;
    let fb = ingest_pdf(&mut store, &db, Limits::DEFAULT).unwrap().field;

    let a = answer_text(&observe_ok(
        &mut store,
        &fa,
        col("i32"),
        Representation::Text,
    ));
    assert!(a.contains("-4000"));
    let b = answer_text(&observe_ok(
        &mut store,
        &fb,
        col("note"),
        Representation::Text,
    ));
    assert!(b.contains("first"));

    for (field, src) in [(fa, PRIMITIVES), (fb, NULLABLE)] {
        let f = Field::open(&store, &field, Limits::DEFAULT).unwrap();
        assert_eq!(f.materialize_exact(Limits::DEFAULT).unwrap(), src);
    }
    drop(store);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn arrow_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived Arrow model reads the source bytes, so its single
    // dependency must be the exact `DocumentExact` root (keyed by `sha256(source)`).
    let fx = Fixture::new("adr0060", PRIMITIVES);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let manifest = field.manifest().clone();
    let root = manifest.root_node;
    let root_node = load_node(fx.store.seeds(), &root).unwrap();
    assert_eq!(root_node.kind, NodeKind::DocumentExact);

    assert!(manifest.has_index());
    let istore = FsIndexStore::open(fx.store.root()).unwrap();
    let iroot = NodeId::from_bytes(manifest.index_root);
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_ARROW_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::ArrowModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the Arrow model must depend on exactly the exact root"
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
                let _ = column_values(&buf, &m, i, Limits::STRICT);
            }
        }
        let _ = build_arrow_model(&buf, Limits::STRICT);
    }
    // Random bytes that begin and end with the ARROW1 magic must still not panic.
    let mut x2: u64 = 0x0BAD_F00D_DEAD_BEEF;
    for _ in 0..256 {
        let mut buf = vec![0u8; 256];
        buf[0..6].copy_from_slice(b"ARROW1");
        for b in buf.iter_mut().skip(6) {
            x2 ^= x2 << 13;
            x2 ^= x2 >> 7;
            x2 ^= x2 << 17;
            *b = (x2 & 0xFF) as u8;
        }
        let n = buf.len();
        buf[n - 6..n].copy_from_slice(b"ARROW1");
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        if let Ok(m) = parse(&buf, Limits::STRICT) {
            for i in 0..m.leaves.len() as u32 {
                let _ = column_values(&buf, &m, i, Limits::STRICT);
            }
        }
    }
}
