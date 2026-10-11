//! Phase 21.18 court: the CBOR (RFC 8949) binary structured-tree adapter.
//!
//! Gated on `cbor`, so a build without the feature compiles an empty target.
//!
//! CBOR has no package layer: the exact leaf is the whole source (a
//! `DocumentExact`), and every derived observation is a bounded,
//! representation-preserving projection of it. The court checks:
//!
//! 1. **Byte-based detection** and the boundaries: a container-rooted CBOR
//!    document → `Cbor`; the self-described-CBOR tag → `Cbor`; a strict JSON
//!    document stays `Json` (never reclassified); a lone scalar, a truncated item,
//!    a map key with no value, and plain prose stay `Opaque`.
//! 2. **Representation preservation**: the encoding width actually used (`0x17` vs
//!    `0x1817`), byte string vs text string as distinct kinds, tag numbers (never
//!    resolved), map order and duplicate keys, float width (half/single/double),
//!    and definite vs indefinite-length items.
//! 3. **Selectors**: native `cbor-pointer`/`cbor-node`/`cbor-find` plus common
//!    `metadata`/`text`/`search-match`.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 5. **Fail closed**: deep nesting declines typed (resource limit); random bytes
//!    never panic.
//! 6. **No cross-field aliasing** (ADR-0060): the model node depends on the exact
//!    `sha256(source)` root.

#![cfg(all(feature = "field", feature = "cbor"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_CBOR_MODEL, SelectorKey, lookup};
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
        "vole-cbor-{label}-{}-{}",
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
        format_basis: "opaque:cbor-test".to_string(),
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
    Selector::CborPointer {
        pointer: p.to_string(),
    }
}

/// `{"a": 1, "b": [1, 2], "c": true}` — a definite map with text keys.
const BASIC: &[u8] = &[
    0xa3, 0x61, b'a', 0x01, 0x61, b'b', 0x82, 0x01, 0x02, 0x61, b'c', 0xf5,
];
/// `[23, 23]` where the first 23 is the inline form (`0x17`) and the second uses
/// the one-byte form (`0x18 0x17`): the widths must stay distinct.
const WIDTHS: &[u8] = &[0x82, 0x17, 0x18, 0x17];
/// `[1.5h, 1.0f, 1.0d]` — half (`0xf9`), single (`0xfa`), and double (`0xfb`).
const FLOATS: &[u8] = &[
    0x83, 0xf9, 0x3e, 0x00, 0xfa, 0x3f, 0x80, 0x00, 0x00, 0xfb, 0x3f, 0xf0, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00,
];
/// `55799({"t": 1(<uint>)})` — the self-described tag wrapping a map wrapping tag 1.
const TAGS: &[u8] = &[
    0xd9, 0xd9, 0xf7, 0xa1, 0x61, b't', 0xc1, 0x1a, 0x51, 0x4b, 0x67, 0x00,
];
/// `[h'010203', "abc"]` — a byte string and a text string (distinct kinds).
const BYTES_TEXT: &[u8] = &[0x82, 0x43, 0x01, 0x02, 0x03, 0x63, b'a', b'b', b'c'];
/// `{"a": 1, "a": 2}` — a duplicate text key, kept distinct.
const DUPKEYS: &[u8] = &[0xa2, 0x61, b'a', 0x01, 0x61, b'a', 0x02];
/// `[_ "ab", {_ "x": 1 }]` — an indefinite array holding an indefinite text string
/// and an indefinite map (break-stop `0xff`).
const INDEF: &[u8] = &[
    0x9f, 0x7f, 0x61, b'a', 0x61, b'b', 0xff, 0xbf, 0x61, b'x', 0x01, 0xff, 0xff,
];
/// `[1, 2, 3]` as **MessagePack** `fixarray(3)` (`0x93`): a CBOR reader sees
/// `array(19)` (truncated), so it stays `Opaque` — the detectors do not guess.
const MSGPACK_FIXARRAY: &[u8] = &[0x93, 0x01, 0x02, 0x03];
/// `{1:2, 3:4}` as **MessagePack** `fixmap(2)` (`0x82`): a CBOR reader sees
/// `array(2) [1, 2]` followed by trailing bytes, so it stays `Opaque`.
const MSGPACK_FIXMAP: &[u8] = &[0x82, 0x01, 0x02, 0x03, 0x04];

/// Strict JSON: must stay `Json`.
const STRICT: &[u8] = br#"{"a": 1, "b": [2, 3]}"#;
/// Plain prose: never a container head byte, stays `Opaque`.
const PROSE: &[u8] = b"The quick brown fox jumps over the lazy dog.\nPlain prose, not CBOR.\n";
/// A lone inline scalar (`23`): structurally trivial, stays `Opaque`.
const SINGLE: &[u8] = &[0x17];
/// A lone one-byte scalar (`0x18 0x17`): a single item, stays `Opaque`.
const SCALAR: &[u8] = &[0x18, 0x17];
/// `{"a":` with no value: malformed, stays `Opaque`.
const BADMAP: &[u8] = &[0xa1, 0x61, b'a', 0x61];
/// An indefinite array with no break-stop: malformed, stays `Opaque`.
const UNTERMINATED: &[u8] = &[0x9f, 0x01, 0x02, 0x03];

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_byte_based_and_conservative() {
    for (bytes, want) in [
        (BASIC, DocumentFormat::Cbor),
        (WIDTHS, DocumentFormat::Cbor),
        (FLOATS, DocumentFormat::Cbor),
        (TAGS, DocumentFormat::Cbor),
        (BYTES_TEXT, DocumentFormat::Cbor),
        (DUPKEYS, DocumentFormat::Cbor),
        (INDEF, DocumentFormat::Cbor),
        (STRICT, DocumentFormat::Json),
        (PROSE, DocumentFormat::Opaque),
        (SINGLE, DocumentFormat::Opaque),
        (SCALAR, DocumentFormat::Opaque),
        (BADMAP, DocumentFormat::Opaque),
        (UNTERMINATED, DocumentFormat::Opaque),
        // The honest MessagePack boundary: a MessagePack source whose bytes are not
        // also a complete well-formed CBOR container stays `Opaque` (never guessed).
        (MSGPACK_FIXARRAY, DocumentFormat::Opaque),
        (MSGPACK_FIXMAP, DocumentFormat::Opaque),
    ] {
        assert_eq!(
            detect_document_format(bytes, Limits::DEFAULT),
            want,
            "{bytes:?}"
        );
    }

    let caps = capabilities_for_format(DocumentFormat::Cbor);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for n in ["cbor-pointer", "cbor-node", "cbor-find"] {
        assert!(caps.native_selectors.contains(&n), "missing {n}");
    }
    assert!(caps.to_json().contains("\"format\":\"cbor\""));
}

#[test]
fn model_preserves_width_order_duplicates_and_kinds() {
    let m = vole_document::adapter::cbor::parse(BASIC, Limits::DEFAULT, true).unwrap();
    assert_eq!(m.top_type, vole_document::adapter::cbor::K_MAP);
    let txt = vole_document::adapter::cbor::canonical_text(&m, BASIC).unwrap();
    // Member order is the source order, not sorted.
    assert!(
        txt.find("\"a\"").unwrap() < txt.find("\"b\"").unwrap(),
        "{txt}"
    );
    assert!(txt.contains("true"), "{txt}");

    // Width preservation: `0x17` (inline, info 23) vs `0x18 0x17` (info 24).
    let w = vole_document::adapter::cbor::parse(WIDTHS, Limits::DEFAULT, true).unwrap();
    let root = &w.nodes[w.root as usize];
    let n0 = &w.nodes[root.children[0] as usize];
    let n1 = &w.nodes[root.children[1] as usize];
    assert_eq!(n0.kind, vole_document::adapter::cbor::K_UINT);
    assert_eq!(n0.info, 23);
    assert_eq!(n0.arg, 23);
    assert_eq!(n1.kind, vole_document::adapter::cbor::K_UINT);
    assert_eq!(n1.info, 24);
    assert_eq!(n1.arg, 23);
    assert_ne!(n0.info, n1.info);

    // Byte string vs text string are distinct kinds.
    let bt = vole_document::adapter::cbor::parse(BYTES_TEXT, Limits::DEFAULT, true).unwrap();
    let rb = &bt.nodes[bt.root as usize];
    assert_eq!(
        bt.nodes[rb.children[0] as usize].kind,
        vole_document::adapter::cbor::K_BYTES
    );
    assert_eq!(
        bt.nodes[rb.children[1] as usize].kind,
        vole_document::adapter::cbor::K_TEXT
    );

    // Through the field: duplicate keys are reported, never collapsed; the first
    // matching value wins pointer resolution.
    let mut fx = Fixture::new("dup", DUPKEYS);
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &fx.report.field,
        pointer("/a"),
        Representation::Metadata,
    ));
    assert!(j.contains("\"matches\":2"), "{j}");
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &fx.report.field,
            pointer("/a"),
            Representation::ExactBytes,
        )),
        &[0x01]
    );
}

#[test]
fn preserves_tags_floats_and_indefinite_items() {
    // Tags are preserved verbatim and never resolved. `/t` steps through the
    // self-described tag and the map, resolving to the inner tag node.
    let mut fx = Fixture::new("tags", TAGS);
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &fx.report.field,
        pointer("/t"),
        Representation::Metadata,
    ));
    assert!(j.contains("\"kind\":\"tag\""), "{j}");
    assert!(j.contains("\"tag\":1"), "{j}");
    let root = answer_json(&observe_ok(
        &mut fx.store,
        &fx.report.field,
        pointer(""),
        Representation::Metadata,
    ));
    assert!(root.contains("\"tag\":55799"), "{root}");

    // Float width is preserved (half/single/double).
    let m = vole_document::adapter::cbor::parse(FLOATS, Limits::DEFAULT, true).unwrap();
    let r = &m.nodes[m.root as usize];
    let infos: Vec<u8> = r
        .children
        .iter()
        .map(|c| m.nodes[*c as usize].info)
        .collect();
    assert_eq!(infos, vec![25, 26, 27]);

    // Definite vs indefinite is recorded.
    let mi = vole_document::adapter::cbor::parse(INDEF, Limits::DEFAULT, true).unwrap();
    assert!(mi.indefinite >= 3);
    assert!(mi.nodes[mi.root as usize].indefinite);
    let mut fx2 = Fixture::new("indef", INDEF);
    let ij = answer_json(&observe_ok(
        &mut fx2.store,
        &fx2.report.field,
        Selector::CborNode {
            pointer: "/0".to_string(),
        },
        Representation::Structure,
    ));
    assert!(ij.contains("\"indefinite\":true"), "{ij}");
    assert!(ij.contains("\"kind\":\"text\""), "{ij}");
}

#[test]
fn pointer_resolves_indices_and_spans() {
    let mut fx = Fixture::new("ptr", BASIC);
    let field = fx.report.field;

    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            pointer("/a"),
            Representation::ExactBytes
        )),
        &[0x01]
    );
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            pointer("/b/1"),
            Representation::ExactBytes
        )),
        &[0x02]
    );
    // The reported span is the exact source span of the token.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        pointer("/c"),
        Representation::Metadata,
    ));
    let (start, end) = parse_span(&j);
    assert_eq!(&BASIC[start..end], &[0xf5]);

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
            pointer("/b/9"),
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
fn native_and_common_selectors_resolve() {
    let mut fx = Fixture::new("selectors", DUPKEYS);
    let field = fx.report.field;

    // `cbor-node`: the root map reports duplicate keys.
    let r = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::CborNode {
            pointer: String::new(),
        },
        Representation::Structure,
    ));
    assert!(r.contains("\"kind\":\"map\""), "{r}");
    assert!(r.contains("\"duplicate_keys\":[\"a\"]"), "{r}");

    // `cbor-find` reports text-key matches.
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::CborFind {
            pattern: "a".to_string(),
        },
        Representation::Text,
    ));
    assert!(found.contains("\"role\":\"key\""), "{found}");

    // Common metadata / text / search-match.
    let meta = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(meta.contains("\"format\":\"cbor\""), "{meta}");
    assert!(meta.contains("\"top_type\":\"map\""), "{meta}");
    assert!(meta.contains("\"duplicate_keys\":1"), "{meta}");
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert!(text.starts_with('{'), "{text}");
    let sm = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("a".to_string()),
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
    for src in [
        BASIC, WIDTHS, FLOATS, TAGS, BYTES_TEXT, DUPKEYS, INDEF, STRICT, PROSE,
    ] {
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
    let src_path = root.join("doc.cbor");
    let desc_path = root.join("doc.voldoc");
    std::fs::write(&src_path, BASIC).unwrap();
    std::fs::write(&desc_path, opaque_descriptor(BASIC)).unwrap();

    let store_root = root.join("store");
    let field_id = {
        let mut store = FieldStore::open(&store_root).unwrap();
        let report = ingest_pdf(&mut store, &opaque_descriptor(BASIC), Limits::DEFAULT).unwrap();
        report.field
        // store drops here: a fresh process reopens the directory.
    };

    std::fs::remove_file(&src_path).unwrap();
    std::fs::remove_file(&desc_path).unwrap();

    let mut store2 = FieldStore::open(&store_root).unwrap();
    let bytes = answer_bytes(&observe_ok(
        &mut store2,
        &field_id,
        pointer("/a"),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, &[0x01]);

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), BASIC.len());
    assert_eq!(sha256(&exact), sha256(BASIC));
    assert_eq!(exact, BASIC);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn deep_nesting_declines_typed() {
    // A 200-deep definite array: `0x81` * 200 then a scalar.
    let mut deep = vec![0x81u8; 200];
    deep.push(0x00);
    // STRICT caps the depth at 64, so a 200-deep item is not CBOR.
    assert_ne!(
        detect_document_format(&deep, Limits::STRICT),
        DocumentFormat::Cbor
    );
    // The parser declines typed (resource limit), never panics.
    let e = vole_document::adapter::cbor::parse(&deep, Limits::STRICT, true).unwrap_err();
    assert_eq!(e.class(), ErrorClass::ResourceLimit);
    // Exactness is independent of the derived decline.
    let fx = Fixture::new("deep", &deep);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn malformed_inputs_decline_typed() {
    for bad in [
        &[0x9f, 0x01, 0x02][..], // unterminated indefinite array
        &[0xa1, 0x61, b'a'][..], // map key with no value
        &[0x01, 0x02][..],       // trailing bytes
        &[0x1c][..],             // reserved additional information
        &[0xf8, 0x00][..],       // non-minimal simple value
        &[0x63, 0x61, 0x62][..], // truncated text string
        &[0x62, 0xff, 0xff][..], // text string that is not valid UTF-8
    ] {
        let e = vole_document::adapter::cbor::parse(bad, Limits::DEFAULT, true).unwrap_err();
        assert_eq!(e.class(), ErrorClass::InvalidCborStructure, "{bad:?}");
    }
}

#[test]
fn interleaved_fixtures_do_not_alias() {
    // ADR-0060: a source-reading node must carry its own source-identity input.
    let root = temp_dir("alias");
    let mut store = FieldStore::open(&root).unwrap();

    let a: &[u8] = &[0x83, 0x01, 0x02, 0x03];
    let b: &[u8] = &[0x83, 0x04, 0x05, 0x06];
    let fa = ingest_pdf(&mut store, &opaque_descriptor(a), Limits::DEFAULT)
        .unwrap()
        .field;
    let fb = ingest_pdf(&mut store, &opaque_descriptor(b), Limits::DEFAULT)
        .unwrap()
        .field;

    for (field, src, want) in [(fa, a, vec![0x01u8]), (fb, b, vec![0x04u8])] {
        let got = answer_bytes(&observe_ok(
            &mut store,
            &field,
            pointer("/0"),
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
fn cbor_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived CBOR model reads the source bytes, so its single
    // dependency must be the exact `DocumentExact` root (keyed by `sha256(source)`).
    // This is what prevents cross-field source aliasing.
    let fx = Fixture::new("adr0060", BASIC);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let manifest = field.manifest().clone();
    let root = manifest.root_node;
    let root_node = load_node(fx.store.seeds(), &root).unwrap();
    assert_eq!(root_node.kind, NodeKind::DocumentExact);

    assert!(manifest.has_index());
    let istore = FsIndexStore::open(fx.store.root()).unwrap();
    let iroot = NodeId::from_bytes(manifest.index_root);
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_CBOR_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::CborModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the CBOR model must depend on exactly the exact root"
    );
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0xC80B_5EED_1234_ABCD;
    for _ in 0..128 {
        let mut buf = vec![0u8; 512];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        if let Ok(m) = vole_document::adapter::cbor::parse(&buf, Limits::STRICT, true) {
            let _ = vole_document::adapter::cbor::canonical_text(&m, &buf);
            let _ = vole_document::adapter::cbor::build_cbor_model(&buf, Limits::STRICT);
        }
    }
}
