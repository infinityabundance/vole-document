//! Phase 21.27 court: the MHTML (MIME HTML) web-archive adapter.
//!
//! Gated on `mhtml`, so a build without the feature compiles an empty target.
//!
//! MHTML is a MIME `multipart/related` document, so this adapter **reuses the EML
//! adapter's bounded MIME layer** for the envelope/parts/headers/spans and the
//! **HTML adapter's** bounded, error-recovering scanner for the root part. MHTML has
//! no package layer: the exact leaf is the whole source (a `DocumentExact`), and every
//! derived observation is a bounded, representation-preserving projection of it. The
//! court checks:
//!
//! 1. **Byte-based, conservative, MHTML-specific detection**: a `multipart/related`
//!    document with an MHTML signal is `Mhtml`; a plain `multipart/related` email, a
//!    plain MIME message, and a plain HTML document stay their own format.
//! 2. **Representation preservation**: the MIME envelope and ordered parts with exact
//!    spans, the ordered sub-resources keyed by `Content-Location`/`Content-ID`, the
//!    root HTML part parsed by the reused HTML scanner, and quoted-printable/base64
//!    parts decoded for observations while the raw encoded bytes stay in the source.
//! 3. **`start=` selection**: the `multipart/related` `start=` `Content-ID` picks the
//!    root part.
//! 4. **Selectors**: native `mhtml-root`/`mhtml-resource`/`mhtml-location`/
//!    `mhtml-find` and common `metadata`/`text`/`resource`/`search-match`.
//! 5. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 6. **Fail closed**: a resource bomb declines typed; malformed inputs decline typed;
//!    random bytes never panic.
//! 7. **No cross-field aliasing** (ADR-0060): two MHTML fixtures interleaved in one
//!    store each answer from their own source, and the model node depends on the exact
//!    `sha256(source)` root.

#![cfg(all(feature = "field", feature = "mhtml"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_MHTML_MODEL, SelectorKey, lookup};
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
        "vole-mhtml-{label}-{}-{}",
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
        format_basis: "opaque:mhtml-test".to_string(),
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

/// An MHTML archive with a quoted-printable HTML root (with `Content-ID: <root@x>`
/// referenced by `start=`), a quoted-printable CSS sub-resource, and a base64 PNG
/// sub-resource with a `Content-ID`.
const DOC: &[u8] = b"From: <Saved by Blink>\r\nSubject: Example page\r\nDate: Mon, 01 Jan 2029 00:00:00 +0000\r\nMIME-Version: 1.0\r\nContent-Type: multipart/related; boundary=\"BOUND\"; type=\"text/html\"; start=\"<root@x>\"\r\nSnapshot-Content-Location: http://example.com/page.html\r\nContent-Base: http://example.com/\r\n\r\n--BOUND\r\nContent-Type: text/html; charset=utf-8\r\nContent-Transfer-Encoding: quoted-printable\r\nContent-Location: http://example.com/page.html\r\nContent-ID: <root@x>\r\n\r\n<html><head><link rel=3D\"stylesheet\" href=3D\"s.css\"></head><body><p>Caf=C3=A9</p><img src=3D\"i.png\"></body></html>\r\n--BOUND\r\nContent-Type: text/css\r\nContent-Transfer-Encoding: quoted-printable\r\nContent-Location: http://example.com/s.css\r\n\r\np { color: =23458; }\r\n--BOUND\r\nContent-Type: image/png\r\nContent-Transfer-Encoding: base64\r\nContent-Location: http://example.com/i.png\r\nContent-ID: <img1@x>\r\n\r\niVBORw0KGgo=\r\n--BOUND--\r\n";

fn mhtml_root() -> Selector {
    Selector::MhtmlRoot
}

fn mhtml_resource(ordinal: u32) -> Selector {
    Selector::MhtmlResource { ordinal }
}

fn mhtml_location() -> Selector {
    Selector::MhtmlLocation
}

fn mhtml_find(pattern: &str) -> Selector {
    Selector::MhtmlFind {
        pattern: pattern.to_string(),
    }
}

#[test]
fn detection_is_byte_based_and_conservative() {
    assert_eq!(
        detect_document_format(DOC, Limits::DEFAULT),
        DocumentFormat::Mhtml
    );
    // A `multipart/related` document with a `Snapshot-Content-Location` and no
    // `From`/`Subject` is still MHTML (the strongest signal).
    let snapshot_only: &[u8] = b"MIME-Version: 1.0\r\nContent-Type: multipart/related; boundary=\"x\"\r\nSnapshot-Content-Location: http://e/\r\n\r\n--x\r\nContent-Type: text/html\r\n\r\n<p>hi</p>\r\n--x--\r\n";
    assert_eq!(
        detect_document_format(snapshot_only, Limits::DEFAULT),
        DocumentFormat::Mhtml
    );
    // A `From`+`Subject` envelope with a `text/html` part is MHTML.
    let envelope: &[u8] = b"From: a@b\r\nSubject: hi\r\nMIME-Version: 1.0\r\nContent-Type: multipart/related; boundary=\"x\"\r\n\r\n--x\r\nContent-Type: text/html\r\n\r\n<p>hi</p>\r\n--x--\r\n";
    assert_eq!(
        detect_document_format(envelope, Limits::DEFAULT),
        DocumentFormat::Mhtml
    );

    // A plain `multipart/related` email (no MHTML markers, no text/html part) stays
    // Eml — never mislabeled MHTML.
    let plain_email: &[u8] = b"From: a@b\r\nSubject: hi\r\nMIME-Version: 1.0\r\nContent-Type: multipart/related; boundary=\"x\"\r\n\r\n--x\r\nContent-Type: text/plain\r\n\r\nyo\r\n--x--\r\n";
    assert_eq!(
        detect_document_format(plain_email, Limits::DEFAULT),
        DocumentFormat::Eml
    );
    // A `multipart/related` with a `text/html` part but no `From`+`Subject` and no
    // Snapshot/Content-Base stays Eml (the MHTML envelope test is the signal).
    let related_no_marker: &[u8] = b"MIME-Version: 1.0\r\nContent-Type: multipart/related; boundary=\"x\"\r\n\r\n--x\r\nContent-Type: text/html\r\n\r\n<p>hi</p>\r\n--x--\r\n";
    assert_eq!(
        detect_document_format(related_no_marker, Limits::DEFAULT),
        DocumentFormat::Eml
    );
    // A plain MIME message stays Eml.
    assert_eq!(
        detect_document_format(b"From: a@b\r\n\r\nbody\r\n", Limits::DEFAULT),
        DocumentFormat::Eml
    );
    // A plain HTML document stays Html (no MIME envelope).
    assert_eq!(
        detect_document_format(
            b"<!doctype html><html><body><h1>Hi</h1></body></html>\n",
            Limits::DEFAULT
        ),
        DocumentFormat::Html
    );
    // Prose stays Opaque.
    assert_eq!(
        detect_document_format(b"just a paragraph of prose.\n", Limits::DEFAULT),
        DocumentFormat::Opaque
    );

    let caps = capabilities_for_format(DocumentFormat::Mhtml);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "resource", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for n in [
        "mhtml-root",
        "mhtml-resource",
        "mhtml-location",
        "mhtml-find",
    ] {
        assert!(caps.native_selectors.contains(&n), "missing {n}");
    }
    assert!(caps.to_json().contains("\"format\":\"mhtml\""));
}

#[test]
fn envelope_parts_and_subresources_are_preserved() {
    use vole_document::adapter::mhtml::parse;

    let m = parse(DOC, Limits::DEFAULT, true).unwrap();
    // The MIME envelope is reused verbatim: four parts (the root `multipart/related`
    // plus the HTML root and the two sub-resources).
    assert_eq!(m.eml.parts.len(), 4);
    assert_eq!(m.eml.parts[0].media_type, "multipart/related");
    assert!(m.has_snapshot_location());
    assert!(m.has_content_base());
    assert_eq!(
        m.snapshot_location.as_deref(),
        Some("http://example.com/page.html")
    );
    assert_eq!(m.content_base.as_deref(), Some("http://example.com/"));
    // `start=` selects the HTML part (part 1), not the first part.
    assert_eq!(m.start_param.as_deref(), Some("<root@x>"));
    assert_eq!(m.root_part, 1);
    // Two sub-resources, in document order, keyed by Content-Location/Content-ID.
    assert_eq!(m.resources.len(), 2);
    assert_eq!(
        m.resources[0].location.as_deref(),
        Some("http://example.com/s.css")
    );
    assert_eq!(
        m.resources[1].location.as_deref(),
        Some("http://example.com/i.png")
    );
    assert_eq!(m.resources[1].content_id.as_deref(), Some("<img1@x>"));
    // The exact header span of a resource's `Content-Location` is in the source.
    let (s, e) = m.resources[1].location_span.unwrap();
    assert_eq!(&DOC[s as usize..e as usize], b" http://example.com/i.png");
    // The exact raw body span of the base64 resource is the encoded base64 text.
    let (bs, be) = (m.resources[1].body_start, m.resources[1].body_end);
    assert_eq!(&DOC[bs as usize..be as usize], b"iVBORw0KGgo=");
    // The decoded length of the base64 payload (8 bytes) is recorded.
    assert_eq!(m.resources[1].decoded_len, 8);
    // The embedded HTML model parses the decoded QP root body.
    assert!(m.html.nodes.len() >= 3);
}

#[test]
fn selectors_resolve_and_decode_for_observations() {
    let mut fx = Fixture::new("selectors", DOC);
    let field = fx.report.field;

    // Native `mhtml-location` reports the envelope locations.
    let loc = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        mhtml_location(),
        Representation::Metadata,
    ));
    assert!(loc.contains("http://example.com/page.html"), "{loc}");
    assert!(loc.contains("http://example.com/"), "{loc}");
    let loc_text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        mhtml_location(),
        Representation::Text,
    ));
    assert_eq!(loc_text, "http://example.com/page.html");

    // Native `mhtml-root` returns the decoded HTML body text.
    let root_text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        mhtml_root(),
        Representation::Text,
    ));
    assert!(root_text.contains("<html>"), "{root_text}");
    // The QP escapes are decoded (`=` -> `=` and `Caf=C3=A9` -> `Café`).
    assert!(root_text.contains("Café"), "{root_text}");
    assert!(root_text.contains("rel=\"stylesheet\""), "{root_text}");
    let root_json = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        mhtml_root(),
        Representation::Metadata,
    ));
    assert!(root_json.contains("\"root_part\":1"), "{root_json}");
    assert!(
        root_json.contains("\"media_type\":\"text/html\""),
        "{root_json}"
    );

    // Native `mhtml-resource` decodes the base64 PNG to its exact bytes.
    let png = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        mhtml_resource(1),
        Representation::ExactBytes,
    ));
    assert_eq!(png, vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
    let css = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        mhtml_resource(0),
        Representation::Text,
    ));
    assert_eq!(css, "p { color: #458; }");
    let res_json = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        mhtml_resource(1),
        Representation::Metadata,
    ));
    assert!(
        res_json.contains("\"location\":\"http://example.com/i.png\""),
        "{res_json}"
    );
    assert!(
        res_json.contains("\"content_id\":\"<img1@x>\""),
        "{res_json}"
    );

    // Native `mhtml-find` searches the whole document (headers + decoded HTML text).
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        mhtml_find("stylesheet"),
        Representation::Text,
    ));
    assert!(found.contains("\"matches\":["), "{found}");
    assert!(found.contains("stylesheet"), "{found}");

    // Common selectors.
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert!(text.contains("<html>"), "{text}");
    let meta = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(meta.contains("\"format\":\"mhtml\""), "{meta}");
    assert!(meta.contains("\"resources\":2"), "{meta}");
    assert!(meta.contains("\"has_snapshot_location\":true"), "{meta}");
    let common_res = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Resource(1),
        Representation::ExactBytes,
    ));
    assert_eq!(common_res, png);
    let search = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("Café".to_string()),
        Representation::Text,
    ));
    assert!(search.contains("\"matches\":["), "{search}");
}

#[test]
fn start_parameter_selects_a_non_first_html_root() {
    // The first part is an image; `start=` names the second (HTML) part.
    const D: &[u8] = b"From: a@b\r\nSubject: s\r\nMIME-Version: 1.0\r\nContent-Type: multipart/related; boundary=\"x\"; start=\"<h@x>\"\r\n\r\n--x\r\nContent-Type: image/gif\r\nContent-Location: http://e/a.gif\r\n\r\nGIF89a\r\n--x\r\nContent-Type: text/html\r\nContent-ID: <h@x>\r\nContent-Location: http://e/\r\n\r\n<h1>Hi</h1>\r\n--x--\r\n";
    assert_eq!(
        detect_document_format(D, Limits::DEFAULT),
        DocumentFormat::Mhtml
    );
    use vole_document::adapter::mhtml::parse;
    let m = parse(D, Limits::DEFAULT, true).unwrap();
    // Part 1 is the image; the root must be part 2 (the HTML part named by `start=`).
    assert_eq!(m.eml.parts[1].media_type, "image/gif");
    assert_eq!(m.root_part, 2);
    assert_eq!(m.resources.len(), 1);
    assert_eq!(m.resources[0].index, 1);
}

#[test]
fn malformed_inputs_decline_typed() {
    use vole_document::adapter::mhtml::parse;

    // No MIME header block at all.
    assert!(parse(b"<!doctype html><html></html>", Limits::DEFAULT, true).is_err());
    // A `multipart/related` without a boundary cannot be modelled.
    let no_boundary: &[u8] = b"From: a@b\r\nSubject: s\r\nMIME-Version: 1.0\r\nContent-Type: multipart/related\r\n\r\nbody\r\n";
    assert!(parse(no_boundary, Limits::DEFAULT, true).is_err());
    // A `multipart/related` with no MHTML signal.
    let no_signal: &[u8] = b"MIME-Version: 1.0\r\nContent-Type: multipart/related; boundary=\"x\"\r\n\r\n--x\r\nContent-Type: text/html\r\n\r\n<p>hi</p>\r\n--x--\r\n";
    assert!(parse(no_signal, Limits::DEFAULT, true).is_err());
    // A plain MIME message (not multipart/related).
    assert!(parse(b"From: a@b\r\n\r\nbody\r\n", Limits::DEFAULT, true).is_err());
    // An out-of-range native resource declines typed (rc 6).
    let mut fx = Fixture::new("declines", DOC);
    let field = fx.report.field;
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            mhtml_resource(99),
            Representation::Metadata,
            Limits::DEFAULT
        ),
        ErrorClass::UnsupportedFeature
    );
    // An unsupported common selector (a table) declines typed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::Table(0),
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
    let src_path = root.join("doc.mhtml");
    let desc_path = root.join("doc.voldoc");
    std::fs::write(&src_path, DOC).unwrap();
    let descriptor = opaque_descriptor(DOC);
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
    let loc = answer_text(&observe_ok(
        &mut store2,
        &field_id,
        mhtml_location(),
        Representation::Text,
    ));
    assert_eq!(loc, "http://example.com/page.html");

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), DOC.len());
    assert_eq!(sha256(&exact), sha256(DOC));
    assert_eq!(exact, DOC);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn resource_bomb_declines_typed_but_exactness_holds() {
    // More sub-resources than the STRICT resource/part cap. Detection and exactness are
    // unaffected (they run under DEFAULT); the model build declines typed under STRICT.
    let mut bomb = String::from(
        "From: a@b\r\nSubject: s\r\nMIME-Version: 1.0\r\nContent-Type: multipart/related; boundary=\"x\"\r\nSnapshot-Content-Location: http://e/\r\n\r\n--x\r\nContent-Type: text/html\r\n\r\n<p>hi</p>\r\n",
    );
    for i in 0..20_000 {
        bomb.push_str("--x\r\nContent-Type: text/css\r\n");
        bomb.push_str(&format!(
            "Content-Location: http://e/{i}.css\r\n\r\nbody{}\r\n",
            i % 10
        ));
    }
    bomb.push_str("--x--\r\n");
    let bomb = bomb.into_bytes();
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
    let f = Field::open(&fx.store, &field, Limits::DEFAULT).unwrap();
    assert_eq!(f.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn interleaved_fixtures_do_not_alias() {
    // ADR-0060: a source-reading node must carry its own source-identity input.
    let root = temp_dir("alias");
    let mut store = FieldStore::open(&root).unwrap();

    let a: &[u8] = b"From: a@b\r\nSubject: alpha\r\nMIME-Version: 1.0\r\nContent-Type: multipart/related; boundary=\"x\"\r\nSnapshot-Content-Location: http://alpha/\r\n\r\n--x\r\nContent-Type: text/html\r\n\r\n<p>alpha</p>\r\n--x--\r\n";
    let b: &[u8] = b"From: c@d\r\nSubject: beta\r\nMIME-Version: 1.0\r\nContent-Type: multipart/related; boundary=\"y\"\r\nSnapshot-Content-Location: http://beta/\r\n\r\n--y\r\nContent-Type: text/html\r\n\r\n<p>beta</p>\r\n--y--\r\n";
    let da = opaque_descriptor(a);
    let db = opaque_descriptor(b);
    let fa = ingest_pdf(&mut store, &da, Limits::DEFAULT).unwrap().field;
    let fb = ingest_pdf(&mut store, &db, Limits::DEFAULT).unwrap().field;

    for (field, src, want) in [(fa, a, "alpha"), (fb, b, "beta")] {
        let got = answer_text(&observe_ok(
            &mut store,
            &field,
            mhtml_root(),
            Representation::Text,
        ));
        assert!(got.contains(want), "aliased source for {field:?}: {got}");
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
fn mhtml_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived MHTML model reads the source bytes, so its single
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
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_MHTML_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::MhtmlModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the MHTML model must depend on exactly the exact root"
    );
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0x0021_2700_5EED_C0DE;
    for _ in 0..160 {
        let mut buf = vec![0u8; 640];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        if let Ok(m) = vole_document::adapter::mhtml::parse(&buf, Limits::STRICT, true) {
            let _ = m.encode();
            let _ = vole_document::adapter::mhtml::find(&buf, &m, "a", Limits::STRICT);
        }
    }
}
