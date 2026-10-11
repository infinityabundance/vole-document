//! Phase 21.17.1 court: the JSON5 / JSONC structured-extra adapter.
//!
//! Gated on `json5`, so a build without the feature compiles an empty target.
//!
//! JSON5 (and its JSONC subset) has no package layer: the exact leaf is the whole
//! source (a `DocumentExact`), and every derived observation is a bounded,
//! representation-preserving projection of it. The court checks:
//!
//! 1. **Byte-based detection** and the boundaries: a JSON5 document → `Json5`; a
//!    JSONC document → `Json5` (dialect `jsonc`); a strict JSON document stays
//!    `Json` (never reclassified); a malformed document and a plain non-JSON blob
//!    stay `Opaque`.
//! 2. **Representation preservation**: comments (exact spans, never dropped),
//!    unquoted keys, single quotes, trailing commas, numeric spelling (`1e3`,
//!    `.5`, `5.`, `0xFF`, `Infinity`, `NaN`), string-escape spelling (`\u00e9`,
//!    `\x41`), member order, and duplicate keys.
//! 3. **Dialect recording**: `jsonc` (comments/trailing commas only) vs `json5`.
//! 4. **Selectors**: native `json5-pointer`/`json5-node`/`json5-find`/`json5-comments`
//!    plus common `metadata`/`text`/`search-match`.
//! 5. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 6. **Fail closed**: deep nesting declines typed (resource limit); random bytes
//!    never panic.
//! 7. **No cross-field aliasing** (ADR-0060): the model node depends on the exact
//!    `sha256(source)` root.

#![cfg(all(feature = "field", feature = "json5"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_JSON5_MODEL, SelectorKey, lookup};
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
        "vole-json5-{label}-{}-{}",
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
        format_basis: "opaque:json5-test".to_string(),
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

fn pointer(p: &str) -> Selector {
    Selector::Json5Pointer {
        pointer: p.to_string(),
    }
}

/// A JSON5 object exercising comments, unquoted keys, single quotes, trailing
/// commas, hex/leading-dot/`Infinity`/`NaN` numbers, escape spelling, member order,
/// and duplicate keys.
const DOC: &[u8] = b"{\n  // a line comment\n  b: 1e3,\n  'a': .5,\n  a: -0,\n  c: '\\u00e9',\n  list: [true, false, null, 42,],\n  'a/b': 7,\n  'c~d': 8,\n  inf: Infinity,\n  nan: NaN,\n}";

/// A JSONC document: the only extensions are comments and a trailing comma.
const JSONC_DOC: &[u8] = b"{\n  // a comment\n  \"a\": 1,\n  \"b\": [1, 2,],\n}";

/// Strict JSON: must stay `Json`.
const STRICT: &[u8] = br#"{"a": 1, "b": [2, 3]}"#;

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_byte_based_and_conservative() {
    assert_eq!(
        detect_document_format(DOC, Limits::DEFAULT),
        DocumentFormat::Json5
    );
    assert_eq!(
        detect_document_format(JSONC_DOC, Limits::DEFAULT),
        DocumentFormat::Json5
    );
    // A strict JSON document is claimed by JSON and never reclassified.
    assert_eq!(
        detect_document_format(STRICT, Limits::DEFAULT),
        DocumentFormat::Json
    );
    assert_eq!(
        detect_document_format(b"42", Limits::DEFAULT),
        DocumentFormat::Json
    );
    // A malformed JSON5 source and a plain non-JSON blob are Opaque (not a guess).
    assert_eq!(
        detect_document_format(b"[1, 2", Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    assert_eq!(
        detect_document_format(
            b"The quick brown fox jumps over the lazy dog.\nThis is plain prose, not a value.\n",
            Limits::DEFAULT
        ),
        DocumentFormat::Opaque
    );
    for bad in [&b"[1, 2"[..], &b"{ unquoted: 1,, b: 2 }"[..]] {
        assert_ne!(
            detect_document_format(bad, Limits::DEFAULT),
            DocumentFormat::Json5,
            "{bad:?}"
        );
    }

    let caps = capabilities_for_format(DocumentFormat::Json5);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for n in [
        "json5-pointer",
        "json5-node",
        "json5-find",
        "json5-comments",
    ] {
        assert!(caps.native_selectors.contains(&n), "missing {n}");
    }
    assert!(caps.to_json().contains("\"format\":\"json5\""));
}

#[test]
fn model_preserves_comments_spelling_order_and_duplicate_keys() {
    let m = vole_document::adapter::json5::parse(DOC, Limits::DEFAULT, true).unwrap();
    assert_eq!(m.dialect, vole_document::adapter::json5::DIALECT_JSON5);
    let txt = vole_document::adapter::json5::canonical_text(&m, DOC).unwrap();
    assert!(txt.contains("b:1e3"), "{txt}");
    assert!(txt.contains("'a':.5"), "{txt}");
    assert!(txt.contains("a:-0"), "{txt}");
    assert!(txt.contains("\\u00e9"), "{txt}");
    assert!(txt.contains("inf:Infinity"), "{txt}");
    assert!(txt.contains("nan:NaN"), "{txt}");
    // Member order is the source order, not sorted.
    let b_pos = txt.find("b:").unwrap();
    let a_pos = txt.find("'a'").unwrap();
    assert!(b_pos < a_pos, "{txt}");
    // Comments are recorded, not dropped.
    assert_eq!(m.comments.len(), 1);
    assert_eq!(m.comments[0].kind, vole_document::adapter::json5::C_LINE);
    assert!(m.trailing_commas >= 2);

    // Through the field: duplicate keys are reported, never collapsed.
    let mut fx = Fixture::new("dup", DOC);
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &fx.report.field,
        pointer("/a"),
        Representation::Metadata,
    ));
    assert!(j.contains("\"matches\":2"), "{j}");
    assert!(j.contains("\"dialect\":\"json5\""), "{j}");
    // The first `a` wins pointer resolution: its token is `.5`.
    let bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &fx.report.field,
        pointer("/a"),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b".5");
}

#[test]
fn records_the_jsonc_and_json5_dialects() {
    // JSONC: comments + trailing comma only.
    let m = vole_document::adapter::json5::parse(JSONC_DOC, Limits::DEFAULT, true).unwrap();
    assert_eq!(m.dialect, vole_document::adapter::json5::DIALECT_JSONC);
    let mut fx = Fixture::new("jsonc", JSONC_DOC);
    let meta = answer_json(&observe_ok(
        &mut fx.store,
        &fx.report.field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(meta.contains("\"format\":\"json5\""), "{meta}");
    assert!(meta.contains("\"dialect\":\"jsonc\""), "{meta}");
    assert!(meta.contains("\"comments\":1"), "{meta}");

    // JSON5: an unquoted key / single quote → json5.
    let m5 = vole_document::adapter::json5::parse(DOC, Limits::DEFAULT, true).unwrap();
    assert_eq!(m5.dialect, vole_document::adapter::json5::DIALECT_JSON5);
}

#[test]
fn pointer_resolves_escapes_indices_and_spans() {
    let mut fx = Fixture::new("ptr", DOC);
    let field = fx.report.field;

    // Array index: /list/3 == 42.
    let bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        pointer("/list/3"),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"42");

    // `~1` decodes to `/`; `~0` to `~`; single-quoted keys resolve.
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            pointer("/a~1b"),
            Representation::ExactBytes,
        )),
        b"7"
    );
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            pointer("/c~0d"),
            Representation::ExactBytes,
        )),
        b"8"
    );

    // The reported span is the exact source span of the token.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        pointer("/c"),
        Representation::Metadata,
    ));
    let (start, end) = parse_span(&j);
    assert_eq!(&DOC[start..end], b"'\\u00e9'");

    // Missing members / out-of-range indices decline typed (never empty).
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            pointer("/nope"),
            Representation::Metadata
        ),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            pointer("/list/9"),
            Representation::Metadata
        ),
        ErrorClass::UnsupportedFeature
    );
    // A malformed pointer is a usage error.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            pointer("no-slash"),
            Representation::Metadata
        ),
        ErrorClass::Usage
    );
}

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

#[test]
fn native_node_comments_and_common_selectors_resolve() {
    let mut fx = Fixture::new("selectors", DOC);
    let field = fx.report.field;

    // `json5-node`: the root object reports unquoted-key kinds and duplicate keys.
    let r = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Json5Node {
            pointer: String::new(),
        },
        Representation::Structure,
    ));
    assert!(r.contains("\"kind\":\"object\""), "{r}");
    assert!(r.contains("\"key_kind\":\"unquoted-key\""), "{r}");
    assert!(r.contains("\"duplicate_keys\":[\"a\"]"), "{r}");

    // `json5-find` reports key and value matches.
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Json5Find {
            pattern: "a".to_string(),
        },
        Representation::Text,
    ));
    assert!(found.contains("\"role\":\"key\""), "{found}");
    assert!(found.contains("\"matches\":["), "{found}");

    // `json5-comments` lists the comment with its exact span.
    let cj = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Json5Comments,
        Representation::Metadata,
    ));
    assert!(cj.contains("\"kind\":\"line\""), "{cj}");
    assert!(cj.contains("\"comments\":1"), "{cj}");

    // Common metadata / text / search-match.
    let meta = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(meta.contains("\"format\":\"json5\""), "{meta}");
    assert!(meta.contains("\"top_type\":\"object\""), "{meta}");
    assert!(meta.contains("\"duplicate_keys\":1"), "{meta}");
    assert!(meta.contains("\"unquoted_keys\":"), "{meta}");
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert!(text.starts_with('{') && text.contains("b:1e3"), "{text}");
    let sm = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("inf".to_string()),
        Representation::Text,
    ));
    assert!(sm.contains("\"role\":\"key\""), "{sm}");

    // Unsupported common pairs decline typed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
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
    let src_path = root.join("doc.json5");
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
        pointer("/list/3"),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"42");

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), DOC.len());
    assert_eq!(sha256(&exact), sha256(DOC));
    assert_eq!(exact, DOC);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn deep_nesting_declines_typed() {
    let mut deep = vec![b'['; 200];
    deep.push(b'0');
    deep.extend(std::iter::repeat_n(b']', 200));
    // STRICT caps the depth at 64, so a 200-deep document is not JSON5.
    assert_ne!(
        detect_document_format(&deep, Limits::STRICT),
        DocumentFormat::Json5
    );
    // The parser declines typed (resource limit), never panics.
    let e = vole_document::adapter::json5::parse(&deep, Limits::STRICT, true).unwrap_err();
    assert_eq!(e.class(), ErrorClass::ResourceLimit);
    // Exactness is independent of the derived decline.
    let fx = Fixture::new("deep", &deep);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn interleaved_fixtures_do_not_alias() {
    // ADR-0060: a source-reading node must carry its own source-identity input.
    let root = temp_dir("alias");
    let mut store = FieldStore::open(&root).unwrap();

    let a = b"{ k: 'aaa', n: 1 }".as_slice();
    let b = b"{ k: 'bbb', n: 2 }".as_slice();
    let da = opaque_descriptor(a);
    let db = opaque_descriptor(b);
    let fa = ingest_pdf(&mut store, &da, Limits::DEFAULT).unwrap().field;
    let fb = ingest_pdf(&mut store, &db, Limits::DEFAULT).unwrap().field;

    for (field, src, want) in [(fa, a, b"'aaa'".to_vec()), (fb, b, b"'bbb'".to_vec())] {
        let got = answer_bytes(&observe_ok(
            &mut store,
            &field,
            pointer("/k"),
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
fn json5_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived JSON5 model reads the source bytes, so its single
    // dependency must be the exact `DocumentExact` root (keyed by
    // `sha256(source)`). This is what prevents cross-field source aliasing.
    let fx = Fixture::new("adr0060", DOC);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let manifest = field.manifest().clone();
    let root = manifest.root_node;
    let root_node = load_node(fx.store.seeds(), &root).unwrap();
    assert_eq!(root_node.kind, NodeKind::DocumentExact);

    assert!(manifest.has_index());
    let istore = FsIndexStore::open(fx.store.root()).unwrap();
    let iroot = NodeId::from_bytes(manifest.index_root);
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_JSON5_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::Json5Model);
    assert_eq!(
        model.deps,
        vec![root],
        "the JSON5 model must depend on exactly the exact root"
    );
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0x5157_5EED_1234_ABCD;
    for _ in 0..128 {
        let mut buf = vec![0u8; 512];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        if let Ok(m) = vole_document::adapter::json5::parse(&buf, Limits::STRICT, true) {
            let _ = vole_document::adapter::json5::canonical_text(&m, &buf);
            let _ = vole_document::adapter::json5::build_json5_model(&buf, Limits::STRICT);
        }
    }
}
