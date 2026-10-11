//! Phase 21.19 court: the MessagePack binary structured-tree adapter.
//!
//! Gated on `msgpack`, so a build without the feature compiles an empty target.
//!
//! MessagePack has no package layer: the exact leaf is the whole source (a
//! `DocumentExact`), and every derived observation is a bounded,
//! representation-preserving projection of it. The court checks:
//!
//! 1. **Byte-based detection** and the boundaries: a container-rooted MessagePack
//!    document → `Msgpack`; an unambiguous MessagePack-only head → `Msgpack`; a
//!    strict JSON document stays `Json` (never reclassified); a lone scalar, a
//!    truncated item, a map key with no value, the never-used `0xc1` byte, the
//!    ambiguous `fixarray(3)`/`fixmap(2)` overlap fixtures, and plain prose stay
//!    `Opaque`; and a **CBOR** document still detects as `Cbor` (coexistence).
//! 2. **Representation preservation**: the exact format byte actually used (`0x17`
//!    vs `0xcc 0x17` vs `0xd0 0x17`), `str` vs `bin` as distinct kinds, map order
//!    and duplicate keys, float width (`float32`/`float64`), and extension type
//!    numbers + payload lengths (preserved, never interpreted).
//! 3. **Selectors**: native `msgpack-pointer`/`msgpack-node`/`msgpack-find` plus
//!    common `metadata`/`text`/`search-match`.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 5. **Fail closed**: deep nesting declines typed (resource limit); random bytes
//!    never panic.
//! 6. **No cross-field aliasing** (ADR-0060): the model node depends on the exact
//!    `sha256(source)` root.

#![cfg(all(feature = "field", feature = "msgpack"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_MSGPACK_MODEL, SelectorKey, lookup};
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
        "vole-msgpack-{label}-{}-{}",
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
        format_basis: "opaque:msgpack-test".to_string(),
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
    Selector::MsgpackPointer {
        pointer: p.to_string(),
    }
}

/// `{"a": [1, 2, 3], "b": true}` — a definite map with a text key and a nested
/// array (detected: 10 bytes, 8 items).
const BASIC: &[u8] = &[0x82, 0xa1, b'a', 0x93, 0x01, 0x02, 0x03, 0xa1, b'b', 0xc3];
/// `[23 (0x17), 23 (0xcc 0x17), 23 (0xd0 0x17), 127, -1]`: the three encodings of
/// the same value must stay distinct (width **and** signedness preserved).
const WIDTHS: &[u8] = &[0x95, 0x17, 0xcc, 0x17, 0xd0, 0x17, 0x7f, 0xff];
/// `[1.0f32, 1.0f64]` — the float width actually used must be preserved.
const FLOATS: &[u8] = &[
    0x92, 0xca, 0x3f, 0x80, 0x00, 0x00, 0xcb, 0x3f, 0xf0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];
/// `[h'010203', "abc"]` — a byte string and a text string (distinct kinds).
const BYTES_TEXT: &[u8] = &[0x92, 0xc4, 0x03, 0x01, 0x02, 0x03, 0xa3, b'a', b'b', b'c'];
/// `{"a": 1, "a": 2, "b": 3}` — a duplicate text key, kept distinct.
const DUPKEYS: &[u8] = &[0x83, 0xa1, b'a', 0x01, 0xa1, b'a', 0x02, 0xa1, b'b', 0x03];
/// `[fixext4(-1, 0x01020304), ext8(5, "AB")]` — extension type numbers and payload
/// lengths preserved, never interpreted.
const EXT: &[u8] = &[
    0x92, 0xd6, 0xff, 0x01, 0x02, 0x03, 0x04, 0xc7, 0x02, 0x05, 0x41, 0x42,
];
/// `map16 { "a": 1 }` — an unambiguous MessagePack-only head byte (`0xde`, which
/// CBOR's grammar rejects), so it is claimed even below the byte threshold.
const MAP16_SMALL: &[u8] = &[0xde, 0x00, 0x01, 0xa1, b'a', 0x01];

/// `[1, 2, 3]` as **MessagePack** `fixarray(3)` (`0x93`): a CBOR reader sees
/// `array(19)` (truncated), and this adapter leaves it `Opaque` (below the byte
/// threshold) — the detectors do not guess.
const AMBIG_FIXARRAY: &[u8] = &[0x93, 0x01, 0x02, 0x03];
/// `{1:2, 3:4}` as **MessagePack** `fixmap(2)` (`0x82`): ambiguous and small, so
/// `Opaque`.
const AMBIG_FIXMAP: &[u8] = &[0x82, 0x01, 0x02, 0x03, 0x04];

/// Strict JSON: must stay `Json`.
const STRICT: &[u8] = br#"{"a": 1, "b": [2, 3]}"#;
/// Plain prose: never a container head byte, stays `Opaque`.
const PROSE: &[u8] =
    b"The quick brown fox jumps over the lazy dog.\nPlain prose, not MessagePack.\n";
/// A lone inline scalar (`23`): structurally trivial, stays `Opaque`.
const SINGLE: &[u8] = &[0x17];
/// A lone one-byte scalar (`uint8 23`): a single item, stays `Opaque`.
const SCALAR: &[u8] = &[0xcc, 0x17];
/// `{"a":` with no value: malformed, stays `Opaque`.
const BADMAP: &[u8] = &[0x81, 0xa1, b'a'];
/// The never-used `0xc1` head byte: malformed, stays `Opaque`.
const BAD_C1: &[u8] = &[0xc1];
/// Trailing bytes after one item: malformed, stays `Opaque`.
const TRAILING: &[u8] = &[0x01, 0x02];

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_byte_based_and_conservative() {
    for (bytes, want) in [
        (BASIC, DocumentFormat::Msgpack),
        (WIDTHS, DocumentFormat::Msgpack),
        (FLOATS, DocumentFormat::Msgpack),
        (BYTES_TEXT, DocumentFormat::Msgpack),
        (DUPKEYS, DocumentFormat::Msgpack),
        (EXT, DocumentFormat::Msgpack),
        (MAP16_SMALL, DocumentFormat::Msgpack),
        (STRICT, DocumentFormat::Json),
        (PROSE, DocumentFormat::Opaque),
        (SINGLE, DocumentFormat::Opaque),
        (SCALAR, DocumentFormat::Opaque),
        (BADMAP, DocumentFormat::Opaque),
        (BAD_C1, DocumentFormat::Opaque),
        (TRAILING, DocumentFormat::Opaque),
        // The honest boundary: the ambiguous whole-number/short-container overlap
        // fixtures stay `Opaque` (never guessed).
        (AMBIG_FIXARRAY, DocumentFormat::Opaque),
        (AMBIG_FIXMAP, DocumentFormat::Opaque),
    ] {
        assert_eq!(
            detect_document_format(bytes, Limits::DEFAULT),
            want,
            "{bytes:?}"
        );
    }

    let caps = capabilities_for_format(DocumentFormat::Msgpack);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for n in ["msgpack-pointer", "msgpack-node", "msgpack-find"] {
        assert!(caps.native_selectors.contains(&n), "missing {n}");
    }
    assert!(caps.to_json().contains("\"format\":\"msgpack\""));
}

/// The coexistence control: CBOR documents are never reclassified as MessagePack,
/// because CBOR's detector runs first (and its self-described tag is the strongest
/// binary signal).
#[cfg(feature = "cbor")]
#[test]
fn cbor_documents_still_detect_as_cbor() {
    // {"a": 1, "b": [1, 2], "c": true}
    const CBOR_BASIC: &[u8] = &[
        0xa3, 0x61, b'a', 0x01, 0x61, b'b', 0x82, 0x01, 0x02, 0x61, b'c', 0xf5,
    ];
    // 55799({"t": 1(<uint>)}) — the self-described-CBOR tag.
    const CBOR_SELF_DESCRIBED: &[u8] = &[
        0xd9, 0xd9, 0xf7, 0xa1, 0x61, b't', 0xc1, 0x1a, 0x51, 0x4b, 0x67, 0x00,
    ];
    for doc in [CBOR_BASIC, CBOR_SELF_DESCRIBED] {
        assert_eq!(
            detect_document_format(doc, Limits::DEFAULT),
            DocumentFormat::Cbor,
            "{doc:?}"
        );
    }
}

#[test]
fn model_preserves_width_order_duplicates_and_kinds() {
    let m = vole_document::adapter::msgpack::parse(BASIC, Limits::DEFAULT, true).unwrap();
    assert_eq!(m.top_type, vole_document::adapter::msgpack::K_MAP);
    let txt = vole_document::adapter::msgpack::canonical_text(&m, BASIC).unwrap();
    // Member order is the source order, not sorted.
    assert!(
        txt.find("\"a\"").unwrap() < txt.find("\"b\"").unwrap(),
        "{txt}"
    );
    assert!(txt.contains("true"), "{txt}");

    // Width and signedness preservation: `0x17` (fixint), `0xcc 0x17` (uint8), and
    // `0xd0 0x17` (int8) are all 23 but keep distinct heads.
    let w = vole_document::adapter::msgpack::parse(WIDTHS, Limits::DEFAULT, true).unwrap();
    let root = &w.nodes[w.root as usize];
    let n0 = &w.nodes[root.children[0] as usize];
    let n1 = &w.nodes[root.children[1] as usize];
    let n2 = &w.nodes[root.children[2] as usize];
    assert_eq!(n0.kind, vole_document::adapter::msgpack::K_UINT);
    assert_eq!(n0.head, 0x17);
    assert_eq!(n0.arg, 23);
    assert_eq!(n1.kind, vole_document::adapter::msgpack::K_UINT);
    assert_eq!(n1.head, 0xcc);
    assert_eq!(n1.arg, 23);
    assert_eq!(n2.kind, vole_document::adapter::msgpack::K_UINT);
    assert_eq!(n2.head, 0xd0);
    assert_eq!(n2.arg, 23);
    let n4 = &w.nodes[root.children[4] as usize];
    assert_eq!(n4.kind, vole_document::adapter::msgpack::K_NEGINT);
    assert_eq!(n4.arg, 0);
    assert_eq!(
        vole_document::adapter::msgpack::canonical_text(&w, WIDTHS)
            .unwrap()
            .matches("23")
            .count(),
        3
    );

    // Byte string vs text string are distinct kinds.
    let bt = vole_document::adapter::msgpack::parse(BYTES_TEXT, Limits::DEFAULT, true).unwrap();
    let rb = &bt.nodes[bt.root as usize];
    assert_eq!(
        bt.nodes[rb.children[0] as usize].kind,
        vole_document::adapter::msgpack::K_BIN
    );
    assert_eq!(
        bt.nodes[rb.children[1] as usize].kind,
        vole_document::adapter::msgpack::K_STR
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
fn preserves_float_width_str_bin_and_ext() {
    // Float width is preserved (float32 vs float64).
    let m = vole_document::adapter::msgpack::parse(FLOATS, Limits::DEFAULT, true).unwrap();
    let r = &m.nodes[m.root as usize];
    let heads: Vec<u8> = r
        .children
        .iter()
        .map(|c| m.nodes[*c as usize].head)
        .collect();
    assert_eq!(heads, vec![0xca, 0xcb]);

    let mut fx = Fixture::new("floats", FLOATS);
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &fx.report.field,
        pointer("/0"),
        Representation::Metadata,
    ));
    assert!(j.contains("\"float\":\"single\""), "{j}");
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &fx.report.field,
        pointer("/1"),
        Representation::Metadata,
    ));
    assert!(j.contains("\"float\":\"double\""), "{j}");

    // Extension type number and payload length are preserved, never interpreted.
    let e = vole_document::adapter::msgpack::parse(EXT, Limits::DEFAULT, true).unwrap();
    let er = &e.nodes[e.root as usize];
    let e0 = &e.nodes[er.children[0] as usize];
    let e1 = &e.nodes[er.children[1] as usize];
    assert_eq!(e0.kind, vole_document::adapter::msgpack::K_EXT);
    assert_eq!(e0.ext_type, 0xff);
    assert_eq!(e0.arg, 4);
    assert_eq!(
        vole_document::adapter::msgpack::payload_bytes(EXT, e0).unwrap(),
        &[0x01, 0x02, 0x03, 0x04]
    );
    assert_eq!(e1.kind, vole_document::adapter::msgpack::K_EXT);
    assert_eq!(e1.ext_type, 0x05);
    assert_eq!(e1.arg, 2);

    let mut fx = Fixture::new("ext", EXT);
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &fx.report.field,
        pointer("/0"),
        Representation::Metadata,
    ));
    assert!(j.contains("\"kind\":\"ext\""), "{j}");
    assert!(j.contains("\"ext_type\":255"), "{j}");
    assert!(j.contains("\"arg\":4"), "{j}");

    // A `str` is `Text`, a `bin` is `ExactBytes` round-tripping the raw bytes.
    let mut fx = Fixture::new("strbin", BYTES_TEXT);
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &fx.report.field,
        pointer("/1"),
        Representation::Text,
    ));
    assert_eq!(text, "abc");
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &fx.report.field,
            pointer("/0"),
            Representation::ExactBytes,
        )),
        &[0xc4, 0x03, 0x01, 0x02, 0x03]
    );
}

#[test]
fn pointer_resolves_indices_and_spans() {
    let mut fx = Fixture::new("ptr", BASIC);
    let field = fx.report.field;

    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            pointer("/b"),
            Representation::ExactBytes
        )),
        &[0xc3]
    );
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            pointer("/a/1"),
            Representation::ExactBytes
        )),
        &[0x02]
    );
    // The reported span is the exact source span of the token.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        pointer("/b"),
        Representation::Metadata,
    ));
    let (start, end) = parse_span(&j);
    assert_eq!(&BASIC[start..end], &[0xc3]);

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
            pointer("/a/9"),
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

    // `msgpack-node`: the root map reports duplicate keys.
    let r = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::MsgpackNode {
            pointer: String::new(),
        },
        Representation::Structure,
    ));
    assert!(r.contains("\"kind\":\"map\""), "{r}");
    assert!(r.contains("\"duplicate_keys\":[\"a\"]"), "{r}");

    // `msgpack-find` reports text-key matches.
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::MsgpackFind {
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
    assert!(meta.contains("\"format\":\"msgpack\""), "{meta}");
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
        BASIC, WIDTHS, FLOATS, BYTES_TEXT, DUPKEYS, EXT, STRICT, PROSE,
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
    let src_path = root.join("doc.msgpack");
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
        pointer("/b"),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, &[0xc3]);

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
    // A 200-deep definite array: `0x91` * 200 then a scalar.
    let mut deep = vec![0x91u8; 200];
    deep.push(0x00);
    // STRICT caps the depth at 64, so a 200-deep item is not MessagePack.
    assert_ne!(
        detect_document_format(&deep, Limits::STRICT),
        DocumentFormat::Msgpack
    );
    // The parser declines typed (resource limit), never panics.
    let e = vole_document::adapter::msgpack::parse(&deep, Limits::STRICT, true).unwrap_err();
    assert_eq!(e.class(), ErrorClass::ResourceLimit);
    // Exactness is independent of the derived decline.
    let fx = Fixture::new("deep", &deep);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn malformed_inputs_decline_typed() {
    for bad in [
        &[0x93, 0x01, 0x02][..], // truncated array
        &[0x81, 0xa1, b'a'][..], // map key with no value
        &[0x01, 0x02][..],       // trailing bytes
        &[0xc1][..],             // the never-used 0xc1 byte
        &[0xc4, 0x10, 0x00][..], // over-long declared bin length
        &[0xa3, b'a', b'b'][..], // truncated fixstr
        &[0xcc][..],             // truncated uint8
        &[0x62, 0xff, 0xff][..], // str that is not valid UTF-8
    ] {
        let e = vole_document::adapter::msgpack::parse(bad, Limits::DEFAULT, true).unwrap_err();
        assert_eq!(e.class(), ErrorClass::InvalidMsgpackStructure, "{bad:?}");
    }
}

#[test]
fn interleaved_fixtures_do_not_alias() {
    // ADR-0060: a source-reading node must carry its own source-identity input.
    let root = temp_dir("alias");
    let mut store = FieldStore::open(&root).unwrap();

    let a: &[u8] = &[0x98, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
    let b: &[u8] = &[0x98, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12];
    let fa = ingest_pdf(&mut store, &opaque_descriptor(a), Limits::DEFAULT)
        .unwrap()
        .field;
    let fb = ingest_pdf(&mut store, &opaque_descriptor(b), Limits::DEFAULT)
        .unwrap()
        .field;

    for (field, src, want) in [(fa, a, vec![0x01u8]), (fb, b, vec![0x0bu8])] {
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
fn msgpack_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived MessagePack model reads the source bytes, so its single
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
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_MSGPACK_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::MsgpackModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the MessagePack model must depend on exactly the exact root"
    );
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0x4D50_C0DE_1234_ABCD;
    for _ in 0..128 {
        let mut buf = vec![0u8; 512];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        if let Ok(m) = vole_document::adapter::msgpack::parse(&buf, Limits::STRICT, true) {
            let _ = vole_document::adapter::msgpack::canonical_text(&m, &buf);
            let _ = vole_document::adapter::msgpack::build_msgpack_model(&buf, Limits::STRICT);
        }
    }
}
