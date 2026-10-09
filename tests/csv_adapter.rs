//! Phase 21.7.1 court: the CSV/TSV tabular adapter (the first Wave-2 tabular format).
//!
//! Gated on `csv`, so a build without the feature compiles an empty target.
//!
//! CSV/TSV has no package layer: the exact leaf is the whole source (a
//! `DocumentExact`), and every derived observation is a bounded,
//! representation-preserving projection of it. The court checks:
//!
//! 1. **Byte-based detection is conservative**: a table with a stable delimiter and
//!    framing (comma *or* tab) is detected; a plain-text blob, a one-column file,
//!    a single record, and malformed input are all Opaque (never a panic).
//! 2. **Representation preservation**: exact record/field bytes and spans, original
//!    quoting (`""` escapes), embedded delimiters/newlines, CRLF vs LF, a BOM, and
//!    the recorded dialect are all preserved.
//! 3. **Selectors**: native `csv-row`/`csv-cell`/`csv-header`/`csv-range`/`csv-find`
//!    and common `metadata`/`text`/`table`/`cell`/`search-match` resolve.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 5. **Fail closed**: a row/column bomb declines typed (resource limit); a huge
//!    table is still selectively readable in bounded memory; random bytes never panic.
//! 6. **No cross-field aliasing** (ADR-0060): two CSV fixtures interleaved in one
//!    store each answer from their own source, and the model node depends on the
//!    exact root.

#![cfg(all(feature = "field", feature = "csv"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_CSV_MODEL, SelectorKey, lookup};
use vole_document::field::ingest::{IngestReport, ingest_pdf};
use vole_document::field::node::NodeKind;
use vole_document::field::observe::{ObserveRequest, Representation, Selector, observe};
use vole_document::field::provenance::{AnswerValue, FieldAnswer};
use vole_document::field::{Field, FieldId, FieldStore};
use vole_document::integrity::sha256;
use vole_document::limits::Limits;
use vole_document::store::NodeId;

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-csv-{label}-{}-{}",
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
        format_basis: "opaque:csv-test".to_string(),
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
    limits: Limits,
) -> ErrorClass {
    let req = ObserveRequest::new(selector, representation);
    observe(store, field, &req, limits).unwrap_err().class()
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

fn row(index: u32) -> Selector {
    Selector::CsvRow { index }
}

fn cell(spec: &str) -> Selector {
    Selector::CsvCell {
        spec: spec.to_string(),
    }
}

fn range(spec: &str) -> Selector {
    Selector::CsvRange {
        spec: spec.to_string(),
    }
}

fn find(pattern: &str) -> Selector {
    Selector::CsvFind {
        pattern: pattern.to_string(),
    }
}

// A fixture with a header, an embedded comma, an embedded newline, `""`, and CRLF.
const DOC: &[u8] =
    b"name,note,age\r\nalice,\"a,b\",30\r\nbob,\"x\ny\",25\r\n\"q\"\"q\",plain,7\r\n";

fn parse_span(json: &str) -> (usize, usize) {
    let tag = "\"span\":[";
    let i = json.find(tag).unwrap() + tag.len();
    let rest = &json[i..];
    let close = rest.find(']').unwrap();
    let mut it = rest[..close].split(',');
    let s = it.next().unwrap().parse().unwrap();
    let e = it.next().unwrap().parse().unwrap();
    (s, e)
}

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_byte_based_and_conservative() {
    assert_eq!(
        detect_document_format(DOC, Limits::DEFAULT),
        DocumentFormat::Csv
    );
    // A tab-delimited table is CSV under the same variant.
    assert_eq!(
        detect_document_format(b"a\tb\nc\td\n", Limits::DEFAULT),
        DocumentFormat::Csv
    );
    // Plain text / one-column blob / single record are all Opaque (never a guess).
    assert_eq!(
        detect_document_format(b"hello world\njust some prose\n", Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    assert_eq!(
        detect_document_format(b"a,b,c", Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    assert_eq!(
        detect_document_format(b"Hello, world.\nThis is a test.\n", Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    assert_eq!(
        detect_document_format(b"", Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    // Malformed CSV (an unterminated quote / junk after a close quote) is Opaque.
    for bad in [&b"\"a,b\nc,d\n"[..], &b"\"a\"x,b\nc,d\n"[..]] {
        assert_eq!(
            detect_document_format(bad, Limits::DEFAULT),
            DocumentFormat::Opaque,
            "{bad:?}"
        );
    }

    let caps = capabilities_for_format(DocumentFormat::Csv);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "table", "cell", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for n in ["csv-row", "csv-cell", "csv-header", "csv-range", "csv-find"] {
        assert!(caps.native_selectors.contains(&n), "missing {n}");
    }
    assert!(caps.to_json().contains("\"format\":\"csv\""));
}

#[test]
fn preserves_quotes_spans_crlf_and_exact_bytes() {
    let mut fx = Fixture::new("preserve", DOC);
    let field = fx.report.field;

    // A quoted field keeps its exact bytes (embedded comma preserved).
    let bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        cell("1:1"),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"\"a,b\"");

    // An embedded newline inside a quoted field keeps its exact bytes.
    let bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        cell("2:1"),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"\"x\ny\"");

    // A `""` escape is preserved in ExactBytes and decoded in Text.
    let bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        cell("3:0"),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"\"q\"\"q\"");
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        cell("3:0"),
        Representation::Text,
    ));
    assert_eq!(text, "q\"q");

    // The reported span is the exact source span of the field, CRLF included in the
    // record span but not in the field.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        cell("1:2"),
        Representation::Metadata,
    ));
    let (start, end) = parse_span(&j);
    assert_eq!(&DOC[start..end], b"30");

    // A record's exact bytes exclude the terminator; the row's decoded text joins
    // fields by the delimiter.
    let bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        row(1),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"alice,\"a,b\",30");
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        row(1),
        Representation::Text,
    ));
    assert_eq!(text, "alice,a,b,30");

    // The dialect is reported as CRLF.
    let meta = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(meta.contains("\"delimiter\":\"comma\""), "{meta}");
    assert!(meta.contains("\"terminator\":\"crlf\""), "{meta}");
    assert!(meta.contains("\"rows\":4"), "{meta}");
    assert!(meta.contains("\"columns\":3"), "{meta}");
    assert!(meta.contains("\"ragged_records\":0"), "{meta}");
}

#[test]
fn bom_is_recorded_and_not_part_of_row_zero() {
    let mut src = vec![0xEF, 0xBB, 0xBF];
    src.extend_from_slice(b"a,b\nc,d\n");
    let mut fx = Fixture::new("bom", &src);
    let field = fx.report.field;
    let meta = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(meta.contains("\"bom_bytes\":3"), "{meta}");
    // Record 0's exact bytes start after the BOM.
    let bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        row(0),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"a,b");
}

#[test]
fn native_selectors_resolve() {
    let mut fx = Fixture::new("native", DOC);
    let field = fx.report.field;

    // Header names.
    let h = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::CsvHeader,
        Representation::Metadata,
    ));
    assert!(h.contains("\"count\":3"), "{h}");
    assert!(h.contains("\"name\""), "{h}");
    assert!(h.contains("\"note\""), "{h}");

    // A cell addressed by header name (`R:COLNAME`).
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        cell("1:name"),
        Representation::Text,
    ));
    assert_eq!(text, "alice");

    // A range.
    let r = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        range("0:0:1:1"),
        Representation::Metadata,
    ));
    assert!(r.contains("\"row\":0"), "{r}");
    assert!(r.contains("\"row\":1"), "{r}");
    assert!(r.contains("\"text\":\"alice\""), "{r}");

    // A lexical find.
    let f = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        find("ali"),
        Representation::Text,
    ));
    assert!(f.contains("\"record\":1"), "{f}");
    assert!(f.contains("\"column\":0"), "{f}");
    assert!(f.contains("\"text\":\"alice\""), "{f}");

    // Missing records and unknown header names decline typed (never empty).
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            row(999),
            Representation::Metadata,
            Limits::DEFAULT
        ),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            cell("0:nope"),
            Representation::Metadata,
            Limits::DEFAULT
        ),
        ErrorClass::UnsupportedFeature
    );
}

#[test]
fn common_selectors_resolve() {
    let mut fx = Fixture::new("common", DOC);
    let field = fx.report.field;

    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert!(text.starts_with("name,note,age"), "{text}");

    let table = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Table(0),
        Representation::Text,
    ));
    assert_eq!(table, text);

    let cellv = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Cell {
            table: 0,
            row: 1,
            col: 0,
        },
        Representation::Text,
    ));
    assert_eq!(cellv, "alice");

    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("plain".to_string()),
        Representation::Text,
    ));
    assert!(found.contains("\"record\":3"), "{found}");

    // A second table does not exist and declines typed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::Table(1),
            Representation::Text,
            Limits::DEFAULT
        ),
        ErrorClass::UnsupportedFeature
    );
}

#[test]
fn exact_materialization_is_byte_identical() {
    let fx = Fixture::new("exact", DOC);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let out = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(out.len(), fx.source.len());
    assert_eq!(sha256(&out), sha256(&fx.source));
    assert_eq!(out, fx.source);
}

#[test]
fn removal_then_fresh_process_is_queryable_and_exact() {
    let root = temp_dir("removal");
    let src_path = root.join("doc.csv");
    let desc_path = root.join("doc.voldoc");
    std::fs::write(&src_path, DOC).unwrap();
    let descriptor = opaque_descriptor(DOC);
    std::fs::write(&desc_path, &descriptor).unwrap();

    let store_root = root.join("store");
    let field_id = {
        let mut store = FieldStore::open(&store_root).unwrap();
        let report = ingest_pdf(&mut store, &descriptor, Limits::DEFAULT).unwrap();
        report.field
        // store drops here: a fresh process reopens the directory.
    };

    std::fs::remove_file(&src_path).unwrap();
    std::fs::remove_file(&desc_path).unwrap();

    let mut store2 = FieldStore::open(&store_root).unwrap();
    let bytes = answer_bytes(&observe_ok(
        &mut store2,
        &field_id,
        cell("2:1"),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"\"x\ny\"");

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), DOC.len());
    assert_eq!(sha256(&exact), sha256(DOC));
    assert_eq!(exact, DOC);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn row_bomb_declines_typed_but_streams_in_bounded_memory() {
    // A column bomb: two consistent records, each with far more columns than the
    // STRICT cap. Detection admits the *shape*; the model build declines typed.
    let mut line = String::new();
    for _ in 0..5000 {
        line.push_str("x,");
    }
    line.push_str("x\n");
    let bomb = format!("{line}{line}").into_bytes();
    assert_eq!(
        detect_document_format(&bomb, Limits::STRICT),
        DocumentFormat::Csv
    );
    let mut fx = Fixture::new("bomb", &bomb);
    let field = fx.report.field;
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::Metadata,
            Representation::Metadata,
            Limits::STRICT
        ),
        ErrorClass::ResourceLimit
    );
    // Exactness is independent of the derived decline.
    let f = Field::open(&fx.store, &field, Limits::DEFAULT).unwrap();
    assert_eq!(f.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);

    // A large table (more rows than the STRICT model cap) is still selectively
    // readable in bounded memory: `csv-row` streams instead of building the model.
    let mut big = String::new();
    for i in 0..200_000u32 {
        big.push_str(&format!("{i},{}\n", i * 2));
    }
    let big = big.into_bytes();
    let mut fx2 = Fixture::new("large", &big);
    let field2 = fx2.report.field;
    let bytes = answer_bytes(&observe_ok(
        &mut fx2.store,
        &field2,
        row(150_000),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"150000,300000");
    // The whole-table model is bounded: metadata declines typed under STRICT.
    assert_eq!(
        observe_err(
            &mut fx2.store,
            &field2,
            Selector::Metadata,
            Representation::Metadata,
            Limits::STRICT
        ),
        ErrorClass::ResourceLimit
    );
}

#[test]
fn interleaved_fixtures_do_not_alias() {
    // ADR-0060: a source-reading node must carry its own source-identity input.
    let root = temp_dir("alias");
    let mut store = FieldStore::open(&root).unwrap();

    let a = b"k,v\naaa,1\n";
    let b = b"k,v\nbbb,2\n";
    let da = opaque_descriptor(a);
    let db = opaque_descriptor(b);
    let fa = ingest_pdf(&mut store, &da, Limits::DEFAULT).unwrap().field;
    let fb = ingest_pdf(&mut store, &db, Limits::DEFAULT).unwrap().field;

    for (field, src, want) in [(fa, a, b"aaa".to_vec()), (fb, b, b"bbb".to_vec())] {
        let got = answer_bytes(&observe_ok(
            &mut store,
            &field,
            cell("1:0"),
            Representation::ExactBytes,
        ));
        assert_eq!(got, want, "aliased source for {field:?}");
        let field_handle = Field::open(&store, &field, Limits::DEFAULT).unwrap();
        assert_eq!(
            field_handle.materialize_exact(Limits::DEFAULT).unwrap(),
            src
        );
    }
    drop(store);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn csv_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived CSV model reads the source bytes, so its single
    // dependency must be the exact `DocumentExact` root (keyed by `sha256(source)`).
    let fx = Fixture::new("adr0060", DOC);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let manifest = field.manifest().clone();
    let root = manifest.root_node;
    let root_node = load_node(fx.store.seeds(), &root).unwrap();
    assert_eq!(root_node.kind, NodeKind::DocumentExact);

    assert!(manifest.has_index());
    let istore = FsIndexStore::open(fx.store.root()).unwrap();
    let iroot = NodeId::from_bytes(manifest.index_root);
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_CSV_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::CsvModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the CSV model must depend on exactly the exact root"
    );
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    for _ in 0..128 {
        let mut buf = vec![0u8; 512];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        if let Ok(d) = vole_document::adapter::csv::sniff_dialect(&buf, Limits::STRICT) {
            let _ = vole_document::adapter::csv::canonical_text(&buf, d, Limits::STRICT, 1 << 20);
            let _ = vole_document::adapter::csv::build_csv_model(&buf, Limits::STRICT);
            let _ = vole_document::adapter::csv::find(&buf, d, "a", Limits::STRICT, 1 << 20);
        }
    }
}
