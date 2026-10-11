//! Phase 21.28 court: the syslog / log-stream adapter (the line/event-stream Wave-2
//! format).
//!
//! Gated on `logstream`, so a build without the feature compiles an empty target.
//!
//! A log stream has no package layer: the exact leaf is the whole source (a
//! `DocumentExact`), and every derived observation is a bounded,
//! representation-preserving projection of it (each line classified by a recorded
//! dialect). The court checks:
//!
//! 1. **Byte-based conservative detection** and the format boundaries: a log stream
//!    is `Logstream`; plain prose (including a single log-looking line inside prose)
//!    stays `Opaque`; a JSON document stays `Json`; a JSONL document stays `Jsonl`.
//! 2. **Per-record spans / terminators / blank lines / BOM**: each record's exact
//!    line span, terminator (LF/CRLF/none), physical line number, dialect, priority,
//!    and the reported blank-line / CRLF / trailing-newline / BOM facts.
//! 3. **Representation preservation**: exact field spans for RFC 5424 (pri/version/
//!    fields/structured-data with escaping/NILVALUE), RFC 3164 (timestamp spelling,
//!    tag/pid), and the generic dialect (level/timestamp spelling, raw remainder).
//! 4. **Selectors**: native `logstream-line`/`logstream-field`/`logstream-find` plus
//!    common `metadata`/`text`/`search-match`.
//! 5. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 6. **Fail closed**: over-cap lines/records/depth decline typed; random bytes never
//!    panic.
//! 7. **No cross-field aliasing** (ADR-0060): two log fixtures interleaved in one
//!    store each answer from their own source spans, and the model node depends on
//!    the exact `sha256(source)` root.

#![cfg(all(feature = "field", feature = "logstream"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_LOGSTREAM_MODEL, SelectorKey, lookup};
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
        "vole-logstream-{label}-{}-{}",
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
        format_basis: "opaque:logstream-test".to_string(),
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

fn line(index: u32) -> Selector {
    Selector::LogstreamLine { index }
}

fn field(spec: &str) -> Selector {
    Selector::LogstreamField {
        spec: spec.to_string(),
    }
}

/// A log stream spanning all three dialects, with a CRLF record and a blank line:
/// an RFC 3164 line (CRLF), a whitespace-only blank line, a generic timestamp+level
/// line, and an RFC 5424 line with structured data. Trailing newline.
const DOC: &[u8] = b"<34>Oct 11 22:14:15 mymachine su[123]: 'su root' failed\r\n \n2026-10-11T12:34:56Z INFO starting up\n<165>1 2026-10-11T12:34:56.789Z host app 1234 ID47 [ex@1 k=\"a b\"] hello <world>\n";

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_byte_based_and_conservative() {
    assert_eq!(
        detect_document_format(DOC, Limits::DEFAULT),
        DocumentFormat::Logstream
    );
    assert_eq!(
        detect_document_format(
            b"2026-10-11T00:00:00Z DEBUG a\nDEBUG b\n[ERROR] c\n",
            Limits::DEFAULT
        ),
        DocumentFormat::Logstream
    );
    // RFC 3164 (BSD) syslog with a realistic message (a `: ` inside the message keeps
    // the YAML detector from reading each line as a `key: value` mapping).
    assert_eq!(
        detect_document_format(
            b"<34>Oct 11 22:14:15 host app[1]: error: failed\n<34>Oct 11 22:14:16 host app[2]: warn: slow\n",
            Limits::DEFAULT
        ),
        DocumentFormat::Logstream
    );
    // Honest boundary: a BSD stream whose every message lacks a `: ` is a sequence of
    // YAML mappings and stays `yaml` (never stolen by logstream) under `--all-features`.
    #[cfg(feature = "yaml")]
    assert_eq!(
        detect_document_format(
            b"<34>Oct 11 22:14:15 host app[1]: aaa\n<34>Oct 11 22:14:16 host app[2]: bbb\n",
            Limits::DEFAULT
        ),
        DocumentFormat::Yaml
    );

    // Plain prose stays Opaque, including prose that contains one log-looking line
    // (not every non-blank line matches a dialect).
    assert_eq!(
        detect_document_format(
            b"This is just prose.\nINFO a single log-looking line\nand more prose here.\n",
            Limits::DEFAULT
        ),
        DocumentFormat::Opaque
    );
    assert_eq!(
        detect_document_format(
            b"fn main() {\n    let x = 1;\n    println!(\"{x}\");\n}\n",
            Limits::DEFAULT
        ),
        DocumentFormat::Opaque
    );

    // A JSON document stays Json; a JSONL document stays Jsonl (only meaningful when
    // those adapters are compiled; the court builds `--all-features`).
    #[cfg(feature = "json")]
    assert_eq!(
        detect_document_format(b"{\"a\":1}\n", Limits::DEFAULT),
        DocumentFormat::Json
    );
    #[cfg(all(feature = "json", feature = "jsonl"))]
    assert_eq!(
        detect_document_format(b"{\"a\":1}\n{\"b\":2}\n", Limits::DEFAULT),
        DocumentFormat::Jsonl
    );
    // With neither adapter compiled, such a source is never stolen by logstream.
    #[cfg(not(feature = "json"))]
    assert_ne!(
        detect_document_format(b"{\"a\":1}\n{\"b\":2}\n", Limits::DEFAULT),
        DocumentFormat::Logstream
    );

    // A single line is not a stream.
    assert_ne!(
        detect_document_format(b"INFO only one line\n", Limits::DEFAULT),
        DocumentFormat::Logstream
    );

    let caps = capabilities_for_format(DocumentFormat::Logstream);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for n in ["logstream-line", "logstream-field", "logstream-find"] {
        assert!(caps.native_selectors.contains(&n), "missing {n}");
    }
    assert!(caps.to_json().contains("\"format\":\"logstream\""));
}

#[test]
fn per_record_spans_terminators_blank_lines_and_bom() {
    use vole_document::adapter::logstream::{
        DIALECT_GENERIC, DIALECT_RFC3164, DIALECT_RFC5424, T_CRLF, T_LF, line_bytes, parse,
    };

    let m = parse(DOC, Limits::DEFAULT).unwrap();
    assert_eq!(m.records.len(), 3);
    assert_eq!(m.blank_lines, 1);
    assert_eq!(m.crlf_records, 1);
    assert!(m.trailing_newline);
    assert_eq!(m.bom_len, 0);
    assert_eq!(m.doc_len, DOC.len() as u64);

    let r0 = &m.records[0];
    assert_eq!(r0.dialect, DIALECT_RFC3164);
    assert_eq!(r0.terminator, T_CRLF);
    assert_eq!(r0.line_number, 0);
    assert_eq!(r0.line_start, 0);
    assert_eq!(
        line_bytes(DOC, r0).unwrap(),
        b"<34>Oct 11 22:14:15 mymachine su[123]: 'su root' failed\r\n"
    );

    // The blank line is counted; record 1 is physical line 2.
    assert_eq!(m.records[1].dialect, DIALECT_GENERIC);
    assert_eq!(m.records[1].terminator, T_LF);
    assert_eq!(m.records[1].line_number, 2);
    assert_eq!(m.records[2].dialect, DIALECT_RFC5424);
    assert_eq!(m.records[2].line_number, 3);

    // A final line with no terminator is `T_NONE`.
    let m = parse(b"INFO a\nINFO b", Limits::DEFAULT).unwrap();
    assert!(!m.trailing_newline);
    assert_eq!(m.records[1].terminator, 0);

    // A leading UTF-8 BOM is recorded and the first record starts after it.
    let bom = b"\xEF\xBB\xBFINFO a\nINFO b\n";
    let m = parse(bom, Limits::DEFAULT).unwrap();
    assert_eq!(m.bom_len, 3);
    assert_eq!(m.records[0].line_start, 3);
}

#[test]
fn native_and_common_selectors_resolve() {
    let mut fx = Fixture::new("selectors", DOC);
    let field_id = fx.report.field;

    // `logstream-line` — exact line content bytes (terminator excluded).
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field_id,
            line(0),
            Representation::ExactBytes,
        )),
        b"<34>Oct 11 22:14:15 mymachine su[123]: 'su root' failed"
    );
    let md = answer_json(&observe_ok(
        &mut fx.store,
        &field_id,
        line(0),
        Representation::Metadata,
    ));
    assert!(md.contains("\"dialect\":\"rfc3164\""), "{md}");
    assert!(md.contains("\"terminator\":\"crlf\""), "{md}");
    assert!(md.contains("\"pri\":34"), "{md}");
    assert!(md.contains("\"facility\":4"), "{md}");
    assert!(md.contains("\"severity\":2"), "{md}");

    // The RFC 5424 record's structured data, with escaping, and its message.
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field_id,
            field("2:structured-data"),
            Representation::ExactBytes,
        )),
        b"[ex@1 k=\"a b\"]"
    );
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field_id,
            field("2:sd-element"),
            Representation::ExactBytes,
        )),
        b"[ex@1 k=\"a b\"]"
    );
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field_id,
            field("2:sd-id"),
            Representation::ExactBytes,
        )),
        b"ex@1"
    );
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field_id,
            field("2:sd-param-value"),
            Representation::ExactBytes,
        )),
        b"\"a b\""
    );
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field_id,
            field("2:msg"),
            Representation::ExactBytes,
        )),
        b"hello <world>"
    );
    // The generic record's level and timestamp spelling.
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field_id,
            field("1:level"),
            Representation::Text,
        )),
        "INFO"
    );
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field_id,
            field("1:timestamp"),
            Representation::ExactBytes,
        )),
        b"2026-10-11T12:34:56Z"
    );
    // The RFC 3164 pid.
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field_id,
            field("0:pid"),
            Representation::ExactBytes,
        )),
        b"123"
    );

    // `logstream-find` reports the record index and role.
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field_id,
        Selector::LogstreamFind {
            pattern: "hello".to_string(),
        },
        Representation::Text,
    ));
    assert!(found.contains("\"record\":2"), "{found}");
    assert!(found.contains("\"role\":\"msg\""), "{found}");

    // Common metadata / text / search-match.
    let meta = answer_json(&observe_ok(
        &mut fx.store,
        &field_id,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(meta.contains("\"format\":\"logstream\""), "{meta}");
    assert!(meta.contains("\"records\":3"), "{meta}");
    assert!(meta.contains("\"blank_lines\":1"), "{meta}");
    assert!(meta.contains("\"crlf_records\":1"), "{meta}");
    assert!(meta.contains("\"trailing_newline\":true"), "{meta}");
    assert!(meta.contains("\"rfc5424\":1"), "{meta}");
    assert!(meta.contains("\"rfc3164\":1"), "{meta}");
    assert!(meta.contains("\"generic\":1"), "{meta}");
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field_id,
        Selector::Text,
        Representation::Text,
    ));
    assert_eq!(text.lines().count(), 3, "{text}");
    assert!(text.contains("hello <world>"), "{text}");
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field_id,
        Selector::SearchMatch("failed".to_string()),
        Representation::Text,
    ));
    assert!(found.contains("failed"), "{found}");

    // Missing records/roles and unknown roles/records decline typed.
    assert_eq!(
        observe_err(&mut fx.store, &field_id, line(9), Representation::Metadata),
        ErrorClass::UnsupportedFeature
    );
    // A role absent from a given record declines typed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field_id,
            field("1:hostname"),
            Representation::Metadata
        ),
        ErrorClass::UnsupportedFeature
    );
    // An unknown role or a malformed reference is a usage error.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field_id,
            field("0:nope"),
            Representation::Metadata
        ),
        ErrorClass::Usage
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field_id,
            field("no-index:msg"),
            Representation::Metadata
        ),
        ErrorClass::Usage
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field_id,
            Selector::Table(0),
            Representation::Text
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
    let src_path = root.join("doc.log");
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
        field("2:msg"),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"hello <world>");

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), DOC.len());
    assert_eq!(sha256(&exact), sha256(DOC));
    assert_eq!(exact, DOC);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn caps_decline_typed() {
    use vole_document::adapter::logstream::parse;

    // An over-cap line declines typed.
    let tight_line = Limits {
        max_logstream_line_bytes: 4,
        ..Limits::DEFAULT
    };
    assert!(!vole_document::adapter::logstream::detect(DOC, tight_line));
    assert_eq!(
        parse(DOC, tight_line).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );

    // A depth cap below structured-data nesting declines typed.
    let tight_depth = Limits {
        max_logstream_depth: 1,
        ..Limits::DEFAULT
    };
    assert_eq!(
        parse(DOC, tight_depth).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );

    // An over-cap record count declines typed.
    let tight_records = Limits {
        max_logstream_records: 2,
        ..Limits::DEFAULT
    };
    assert_eq!(
        parse(DOC, tight_records).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );

    // A field cap below the total declines typed.
    let tight_fields = Limits {
        max_logstream_fields: 3,
        ..Limits::DEFAULT
    };
    assert_eq!(
        parse(DOC, tight_fields).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );

    // The opaque floor still closes the source exactly.
    let fx = Fixture::new("caps", DOC);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn interleaved_fixtures_do_not_alias() {
    // ADR-0060: a source-reading node must carry its own source-identity input.
    // Two fixtures with distinct sources share one store, interleaved; each must
    // answer from its own spans (never the other document's bytes).
    let root = temp_dir("alias");
    let mut store = FieldStore::open(&root).unwrap();

    let a = b"[INFO] aaa tail\n[WARN] aaa tail\n".as_slice();
    let b = b"[INFO] bbb tail\n[WARN] bbb tail\n".as_slice();
    let da = opaque_descriptor(a);
    let db = opaque_descriptor(b);
    let fa = ingest_pdf(&mut store, &da, Limits::DEFAULT).unwrap().field;
    let fb = ingest_pdf(&mut store, &db, Limits::DEFAULT).unwrap().field;

    for (f, src, want) in [(fa, a, b"aaa tail".to_vec()), (fb, b, b"bbb tail".to_vec())] {
        let got = answer_text(&observe_ok(&mut store, &f, line(1), Representation::Text));
        assert_eq!(got.as_bytes(), &want[..], "aliased source for {f:?}");
        let field_handle = Field::open(&store, &f, Limits::DEFAULT).unwrap();
        assert_eq!(
            field_handle.materialize_exact(Limits::DEFAULT).unwrap(),
            src
        );
    }
    drop(store);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn logstream_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived log-stream model reads the source bytes, so its single
    // dependency must be the exact `DocumentExact` root (keyed by `sha256(source)`).
    // This is what prevents cross-field source aliasing.
    let fx = Fixture::new("adr0060", DOC);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let manifest = field.manifest().clone();
    let root = manifest.root_node;
    let root_node = load_node(fx.store.seeds(), &root).unwrap();
    assert_eq!(root_node.kind, NodeKind::DocumentExact);

    assert!(manifest.has_index());
    let istore = FsIndexStore::open(fx.store.root()).unwrap();
    let iroot = NodeId::from_bytes(manifest.index_root);
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_LOGSTREAM_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::LogstreamModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the log-stream model must depend on exactly the exact root"
    );
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0x0BAD_F00D_DEAD_BEEF;
    for _ in 0..128 {
        let mut buf = vec![0u8; 512];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        if let Ok(m) = vole_document::adapter::logstream::parse(&buf, Limits::STRICT) {
            let _ = vole_document::adapter::logstream::canonical_text(&m, &buf);
            let _ = vole_document::adapter::logstream::find(&m, &buf, "a", Limits::STRICT);
        }
        let _ = vole_document::adapter::logstream::build_logstream_model(&buf, Limits::STRICT);
    }
}
