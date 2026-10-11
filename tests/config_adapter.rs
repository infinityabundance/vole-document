//! Phase 21.20 court: the config-family adapter (INI / `.env` / Java
//! `.properties`).
//!
//! Gated on `field` + `config`, so a build without the feature compiles an empty
//! target.
//!
//! The config family has no package layer: the exact leaf is the whole source (a
//! `DocumentExact`), and every derived observation is a bounded,
//! representation-preserving projection of it. The court checks:
//!
//! 1. **Byte-based detection** and the boundaries: an INI document (section +
//!    entries) → `Config`; an `export`-bearing `.env` → `Config`; a
//!    properties-only construct → `Config`; a strict TOML document stays `Toml`;
//!    a JSON document stays `Json`; a CSV table stays `Csv`; the pure
//!    `KEY=VALUE` env/properties overlap, prose, and a `#!` script stay `Opaque`.
//! 2. **Representation preservation**: exact spans for sections, keys, values, and
//!    comments; line order; the `export` marker and quoting style; the
//!    `properties` trailing-`\` continuation; and `\uXXXX` escapes preserved as
//!    spelling.
//! 3. **Selectors**: native `config-line`/`config-entry`/`config-section`/
//!    `config-find` plus common `metadata`/`text`/`search-match`, and the
//!    duplicate-key report.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 5. **Fail closed**: malformed declines typed; random bytes never panic.
//! 6. **No cross-field aliasing** (ADR-0060): the model node depends on the exact
//!    `sha256(source)` root.

#![cfg(all(feature = "field", feature = "config"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_CONFIG_MODEL, SelectorKey, lookup};
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
        "vole-config-{label}-{}-{}",
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
        format_basis: "opaque:config-test".to_string(),
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
    Selector::ConfigLine { index }
}

fn entry(index: u32) -> Selector {
    Selector::ConfigEntry { index }
}

/// An INI document: a `[section]` header, `=`/`:` entries, a `;` inline comment.
const INI: &[u8] = b"[db]\nhost = localhost ; the host\nport: 5432\n";
/// An `.env` document: an `export` signal (the `env`-only construct), quoting.
const ENV: &[u8] = b"# app\nFOO=bar\nexport BAZ=\"a b\"\nEMPTY=\n";
/// A Java `.properties` document: a `:` separator, a trailing-`\` continuation,
/// and a `\uXXXX` escape.
const PROPS: &[u8] = b"# note\ncolon: v\nmulti=one\\\n  two\nunicode=gr\\u00FCn\n";
/// The pure `KEY=VALUE` overlap between env and Java properties: never guessed.
const OVERLAP: &[u8] = b"FOO=bar\nBAZ=qux\n";
/// Plain prose: never a config-family document.
const PROSE: &[u8] = b"The quick brown fox jumps over the lazy dog.\nPlain prose, not config.\n";
/// A `#!` script: a shebang declines.
const SCRIPT: &[u8] = b"#!/bin/sh\nexport FOO=bar\n";

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_byte_based_and_conservative() {
    for (bytes, want) in [
        (INI, DocumentFormat::Config),
        (ENV, DocumentFormat::Config),
        (PROPS, DocumentFormat::Config),
        // The documented boundaries: the overlap, prose, and a script stay Opaque.
        (OVERLAP, DocumentFormat::Opaque),
        (PROSE, DocumentFormat::Opaque),
        (SCRIPT, DocumentFormat::Opaque),
    ] {
        assert_eq!(
            detect_document_format(bytes, Limits::DEFAULT),
            want,
            "{bytes:?}"
        );
    }

    // A document already claimed by another format stays claimed by it. These
    // controls only apply when the earlier adapter is compiled into this build.
    #[cfg(feature = "toml")]
    assert_eq!(
        detect_document_format(b"a = 1\nb = 2\n", Limits::DEFAULT),
        DocumentFormat::Toml
    );
    #[cfg(feature = "json")]
    assert_eq!(
        detect_document_format(b"{\"a\": 1, \"b\": [2, 3]}", Limits::DEFAULT),
        DocumentFormat::Json
    );
    #[cfg(feature = "csv")]
    assert_eq!(
        detect_document_format(b"a,b\nc,d\n", Limits::DEFAULT),
        DocumentFormat::Csv
    );

    let caps = capabilities_for_format(DocumentFormat::Config);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for n in [
        "config-line",
        "config-entry",
        "config-section",
        "config-find",
    ] {
        assert!(caps.native_selectors.contains(&n), "missing {n}");
    }
    assert!(caps.to_json().contains("\"format\":\"config\""));
}

#[test]
fn ini_preserves_sections_keys_values_and_comments() {
    let m = vole_document::adapter::config::parse(INI, Limits::DEFAULT, true).unwrap();
    assert_eq!(m.dialect, vole_document::adapter::config::DIALECT_INI);
    assert_eq!(m.lines.len(), 3);
    let sec = &m.lines[0];
    assert_eq!(sec.kind, vole_document::adapter::config::L_SECTION);
    assert_eq!(
        vole_document::adapter::config::key_bytes(INI, sec).unwrap(),
        b"db"
    );
    let host = &m.lines[1];
    assert_eq!(host.kind, vole_document::adapter::config::L_ENTRY);
    assert_eq!(
        vole_document::adapter::config::key_bytes(INI, host).unwrap(),
        b"host"
    );
    assert_eq!(
        vole_document::adapter::config::value_bytes(INI, host).unwrap(),
        b"localhost"
    );
    assert!(
        host.flags & vole_document::adapter::config::F_INLINE_COMMENT != 0,
        "the inline comment must be recorded"
    );
    // The `:` separator is preserved as the marker.
    assert_eq!(m.lines[2].marker, b':');

    // Through the field: the section resolves by name and the entries observe.
    let mut fx = Fixture::new("ini", INI);
    let field = fx.report.field;
    assert_eq!(fx.report.format, DocumentFormat::Config);
    let sec_json = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::ConfigSection {
            name: "db".to_string(),
        },
        Representation::Structure,
    ));
    assert!(sec_json.contains("\"name\":\"db\""), "{sec_json}");
    assert!(sec_json.contains("\"matches\":1"), "{sec_json}");
    // An unknown section declines typed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::ConfigSection {
                name: "nope".to_string()
            },
            Representation::Structure
        ),
        ErrorClass::UnsupportedFeature
    );
}

#[test]
fn env_preserves_export_quoting_and_order() {
    let m = vole_document::adapter::config::parse(ENV, Limits::DEFAULT, true).unwrap();
    assert_eq!(m.dialect, vole_document::adapter::config::DIALECT_ENV);
    assert_eq!(m.lines[0].kind, vole_document::adapter::config::L_COMMENT);
    let baz = &m.lines[2];
    assert!(baz.flags & vole_document::adapter::config::F_EXPORT != 0);
    assert!(baz.flags & vole_document::adapter::config::F_DOUBLE_QUOTED != 0);
    assert_eq!(
        vole_document::adapter::config::value_bytes(ENV, baz).unwrap(),
        b"\"a b\""
    );
    assert_eq!(
        vole_document::adapter::config::decode_value(m.dialect, ENV, baz).unwrap(),
        "a b"
    );
    assert!(
        vole_document::adapter::config::value_bytes(ENV, &m.lines[3])
            .unwrap()
            .is_empty()
    );

    // Through the field: entry text is the decoded `key=value`; the `export` fact
    // is reported in the structure projection.
    let mut fx = Fixture::new("env", ENV);
    let field = fx.report.field;
    let meta = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(meta.contains("\"dialect\":\"env\""), "{meta}");
    let e2 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        entry(1),
        Representation::Structure,
    ));
    assert!(e2.contains("\"export\":true"), "{e2}");
    assert!(e2.contains("\"double_quoted\":true"), "{e2}");
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            entry(1),
            Representation::Text
        )),
        "BAZ=a b"
    );
}

#[test]
fn properties_preserves_continuation_and_unicode_spelling() {
    let m = vole_document::adapter::config::parse(PROPS, Limits::DEFAULT, true).unwrap();
    assert_eq!(
        m.dialect,
        vole_document::adapter::config::DIALECT_PROPERTIES
    );
    // `colon: v` uses the `:` separator.
    assert_eq!(m.lines[1].marker, b':');
    // The continued logical line is one line whose exact span holds the raw
    // backslashes and embedded newline.
    let multi = &m.lines[2];
    assert!(multi.flags & vole_document::adapter::config::F_CONTINUED != 0);
    assert_eq!(
        vole_document::adapter::config::value_bytes(PROPS, multi).unwrap(),
        b"one\\\n  two"
    );
    assert_eq!(
        vole_document::adapter::config::decode_value(m.dialect, PROPS, multi).unwrap(),
        "onetwo"
    );
    // `\uXXXX` is preserved as spelling, never expanded.
    assert_eq!(
        vole_document::adapter::config::decode_value(m.dialect, PROPS, &m.lines[3]).unwrap(),
        "gr\\u00FCn"
    );

    // Through the field: the continuation fact is reported and the dialect token
    // is recorded in provenance.
    let mut fx = Fixture::new("props", PROPS);
    let field = fx.report.field;
    let e2 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        entry(1),
        Representation::Structure,
    ));
    assert!(e2.contains("\"continued\":true"), "{e2}");
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            line(2),
            Representation::ExactBytes
        )),
        b"multi=one\\\n  two"
    );
}

#[test]
fn duplicate_keys_are_reported_not_collapsed() {
    let src = b"[a]\nk = 1\nk = 2\n";
    let mut fx = Fixture::new("dup", src);
    let field = fx.report.field;
    let e0 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        entry(0),
        Representation::Structure,
    ));
    assert!(e0.contains("\"same_key_entries\":2"), "{e0}");
    assert!(e0.contains("\"key\":\"k\""), "{e0}");
    let e1 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        entry(1),
        Representation::Structure,
    ));
    assert!(e1.contains("\"same_key_entries\":2"), "{e1}");
}

#[test]
fn native_and_common_selectors_resolve() {
    let mut fx = Fixture::new("selectors", INI);
    let field = fx.report.field;

    // `config-line`: line 0 is the section header.
    let l0 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        line(0),
        Representation::Structure,
    ));
    assert!(l0.contains("\"kind\":\"section\""), "{l0}");
    assert!(l0.contains("\"dialect\":\"ini\""), "{l0}");

    // `config-find` reports key and value roles with spans.
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::ConfigFind {
            pattern: "host".to_string(),
        },
        Representation::Text,
    ));
    assert!(found.contains("\"role\":\"key\""), "{found}");
    assert!(found.contains("\"role\":\"value\""), "{found}");

    // Common metadata / text / search-match.
    let meta = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(meta.contains("\"format\":\"config\""), "{meta}");
    assert!(meta.contains("\"entries\":2"), "{meta}");
    assert!(meta.contains("\"sections\":1"), "{meta}");
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert!(text.contains("host = localhost"), "{text}");
    let sm = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("5432".to_string()),
        Representation::Text,
    ));
    assert!(sm.contains("\"role\":\"value\""), "{sm}");

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
    for src in [INI, ENV, PROPS, OVERLAP, PROSE] {
        let fx = Fixture::new("exact", src);
        let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
        let out = field.materialize_exact(Limits::DEFAULT).unwrap();
        assert_eq!(out.len(), fx.source.len());
        assert_eq!(sha256(&out), sha256(&fx.source));
        assert_eq!(out, fx.source);
    }
}

#[test]
fn removal_then_fresh_process_is_queryable_and_exact() {
    let root = temp_dir("removal");
    let src_path = root.join("doc.env");
    let desc_path = root.join("doc.voldoc");
    std::fs::write(&src_path, ENV).unwrap();
    std::fs::write(&desc_path, opaque_descriptor(ENV)).unwrap();

    let store_root = root.join("store");
    let field_id = {
        let mut store = FieldStore::open(&store_root).unwrap();
        let report = ingest_pdf(&mut store, &opaque_descriptor(ENV), Limits::DEFAULT).unwrap();
        report.field
        // store drops here: a fresh process reopens the directory.
    };

    std::fs::remove_file(&src_path).unwrap();
    std::fs::remove_file(&desc_path).unwrap();

    let mut store2 = FieldStore::open(&store_root).unwrap();
    let bytes = answer_bytes(&observe_ok(
        &mut store2,
        &field_id,
        entry(1),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"export BAZ=\"a b\"");

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), ENV.len());
    assert_eq!(sha256(&exact), sha256(ENV));
    assert_eq!(exact, ENV);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn malformed_inputs_decline_typed() {
    // A line that is neither a comment, section, nor entry declines typed.
    let e = vole_document::adapter::config::parse(
        b"[a]\nthis is not an entry\n",
        Limits::DEFAULT,
        true,
    )
    .unwrap_err();
    assert_eq!(e.class(), ErrorClass::InvalidConfigStructure);
    // An out-of-range line / entry / section addressed by a selector declines typed.
    let mut fx = Fixture::new("declines", INI);
    let field = fx.report.field;
    assert_eq!(
        observe_err(&mut fx.store, &field, line(99), Representation::Metadata),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(&mut fx.store, &field, entry(99), Representation::Metadata),
        ErrorClass::UnsupportedFeature
    );
}

#[test]
fn interleaved_fixtures_do_not_alias() {
    // ADR-0060: a source-reading node must carry its own source-identity input.
    let root = temp_dir("alias");
    let mut store = FieldStore::open(&root).unwrap();

    let fa = ingest_pdf(&mut store, &opaque_descriptor(INI), Limits::DEFAULT)
        .unwrap()
        .field;
    let fb = ingest_pdf(&mut store, &opaque_descriptor(ENV), Limits::DEFAULT)
        .unwrap()
        .field;

    for (field, src, want) in [(fa, INI, b"[db]".to_vec()), (fb, ENV, b"# app".to_vec())] {
        let got = answer_bytes(&observe_ok(
            &mut store,
            &field,
            line(0),
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
fn config_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived config model reads the source bytes, so its single
    // dependency must be the exact `DocumentExact` root (keyed by `sha256(source)`).
    // This is what prevents cross-field source aliasing.
    let fx = Fixture::new("adr0060", INI);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let manifest = field.manifest().clone();
    let root = manifest.root_node;
    let root_node = load_node(fx.store.seeds(), &root).unwrap();
    assert_eq!(root_node.kind, NodeKind::DocumentExact);

    assert!(manifest.has_index());
    let istore = FsIndexStore::open(fx.store.root()).unwrap();
    let iroot = NodeId::from_bytes(manifest.index_root);
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_CONFIG_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::ConfigModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the config model must depend on exactly the exact root"
    );
}

#[test]
fn deep_continuation_declines_typed() {
    // A `properties` logical line 200 physical lines deep: STRICT caps the
    // continuation depth at 64, so it is not detected and the parser declines.
    let mut deep = b"k=a\\\n".to_vec();
    for _ in 0..200 {
        deep.extend_from_slice(b" \\\n");
    }
    deep.extend_from_slice(b" z\n");
    assert_ne!(
        detect_document_format(&deep, Limits::STRICT),
        DocumentFormat::Config
    );
    let e = vole_document::adapter::config::parse(&deep, Limits::STRICT, true).unwrap_err();
    assert_eq!(e.class(), ErrorClass::ResourceLimit);
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0xC0FE_2020_21DE_AD00;
    for _ in 0..128 {
        let mut buf = vec![0u8; 512];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        if let Ok(m) = vole_document::adapter::config::parse(&buf, Limits::STRICT, true) {
            let _ = vole_document::adapter::config::canonical_text(&m, &buf, 1 << 20);
            let _ = vole_document::adapter::config::build_config_model(&buf, Limits::STRICT);
        }
    }
}
