//! Phase 21.13 court: the EML / MIME messaging adapter (the messaging Wave-2
//! format).
//!
//! Gated on `eml`, so a build without the feature compiles an empty target.
//!
//! EML has no package layer: the exact leaf is the whole source (a
//! `DocumentExact`), and every derived observation is a bounded,
//! representation-preserving projection of it. The court checks:
//!
//! 1. **Byte-based conservative detection** and the Opaque boundary: a genuine
//!    RFC 5322 message is EML; prose without a header block, a colon-bearing note
//!    without a body separator, and a JSON/YAML document stay their own format.
//! 2. **Representation preservation**: header order and duplicate headers kept,
//!    folded header spans, the resolved `multipart/*` tree, and the exact
//!    base64/quoted-printable decoded constituent bytes.
//! 3. **Attachments**: an attachment's exact decoded bytes and metadata.
//! 4. **Selectors**: native `eml-header`/`eml-part`/`eml-attachments`/`eml-body`/
//!    `eml-find` plus common `metadata`/`text`/`resource`/`search-match`.
//! 5. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 6. **Fail closed**: an over-deep tree and an over-cap decode decline typed;
//!    random bytes never panic.
//! 7. **No cross-field aliasing** (ADR-0060): two EML fixtures interleaved in one
//!    store each answer from their own source spans, and the model node depends on
//!    the exact `sha256(source)` root.

#![cfg(all(feature = "field", feature = "eml"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_EML_MODEL, SelectorKey, lookup};
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
        "vole-eml-{label}-{}-{}",
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
        format_basis: "opaque:eml-test".to_string(),
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

fn header(name: &str) -> Selector {
    Selector::EmlHeader {
        name: name.to_string(),
    }
}

/// A MIME message with: two `Received:` headers (duplicates), a folded `Subject`,
/// a `multipart/mixed` body with a quoted-printable text part and a base64
/// attachment, and a nested `message/rfc822` part.
const DOC: &[u8] = b"Received: from a.example by b.example\r\nReceived: from c.example by d.example\r\nFrom: Alice <alice@example.com>\r\nTo: Bob <bob@example.com>\r\nSubject: Hello\r\n there\r\nDate: Mon, 01 Jan 2029 00:00:00 +0000\r\nMessage-ID: <doc@example.com>\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"MIX\"\r\n\r\npreamble text\r\n--MIX\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\nCaf=C3=A9 and tea\r\n--MIX\r\nContent-Type: application/octet-stream\r\nContent-Transfer-Encoding: base64\r\nContent-Disposition: attachment; filename=\"report.bin\"\r\n\r\nUmVwb3J0IGRhdGE=\r\n--MIX--\r\nepilogue text\r\n";

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_byte_based_and_conservative() {
    assert_eq!(
        detect_document_format(DOC, Limits::DEFAULT),
        DocumentFormat::Eml
    );
    // A minimal message is EML.
    assert_eq!(
        detect_document_format(b"From: a@b\r\n\r\nbody\r\n", Limits::DEFAULT),
        DocumentFormat::Eml
    );
    // A MIME-Version header alone is a positive signal.
    assert_eq!(
        detect_document_format(b"MIME-Version: 1.0\r\n\r\nbody\r\n", Limits::DEFAULT),
        DocumentFormat::Eml
    );
    // Prose without a header block stays Opaque (no structural mark under any
    // feature set).
    assert_eq!(
        detect_document_format(b"just a paragraph of prose.\n", Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    // A colon-bearing note without a body separator, and a header block without a
    // From/Date/Message-ID/MIME-Version signal, are never EML.
    for prose in [
        &b"From: Alpha to Beta\n"[..],
        &b"X-Other: 1\nX-More: 2\n\nbody\n"[..],
        &b"{\"a\":1}\n"[..],
        &b"# Heading\n\nSome *prose*.\n"[..],
    ] {
        assert_ne!(
            detect_document_format(prose, Limits::DEFAULT),
            DocumentFormat::Eml,
            "{prose:?}"
        );
    }

    let caps = capabilities_for_format(DocumentFormat::Eml);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "resource", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for n in [
        "eml-header",
        "eml-part",
        "eml-attachments",
        "eml-body",
        "eml-find",
    ] {
        assert!(caps.native_selectors.contains(&n), "missing {n}");
    }
    assert!(caps.to_json().contains("\"format\":\"eml\""));
}

#[test]
fn header_order_and_duplicates_and_folding_are_preserved() {
    use vole_document::adapter::eml::parse;

    let m = parse(DOC, Limits::DEFAULT).unwrap();
    let root = m.part(0).unwrap();
    // Header order is the source order, and the two `Received:` headers stay
    // distinct.
    let names: Vec<&str> = root.headers.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names[0], "Received");
    assert_eq!(names[1], "Received");
    let received: Vec<&str> = root
        .headers
        .iter()
        .filter(|h| h.name_is("received"))
        .map(|h| h.value.as_str())
        .collect();
    assert_eq!(
        received,
        vec!["from a.example by b.example", "from c.example by d.example"]
    );
    // The folded Subject keeps its exact full span (both lines) and unfolds.
    let subj = root.header("Subject").unwrap();
    assert_eq!(subj.value, "Hello there");
    assert_eq!(
        &DOC[subj.full_start as usize..subj.full_end as usize],
        b"Subject: Hello\r\n there\r\n"
    );
    // The exact raw value span is retained (`Hello` + folding whitespace + `there`).
    assert_eq!(
        &DOC[subj.value_start as usize..subj.value_end as usize],
        b" Hello\r\n there"
    );
}

#[test]
fn mime_tree_and_exact_decoded_bytes() {
    use vole_document::adapter::eml::{CTE_BASE64, CTE_QP, K_MULTIPART, K_TEXT, parse};

    let m = parse(DOC, Limits::DEFAULT).unwrap();
    assert_eq!(m.parts.len(), 3);
    let root = m.part(0).unwrap();
    assert_eq!(root.kind, K_MULTIPART);
    assert_eq!(root.boundary.as_deref(), Some("MIX"));
    assert_eq!(root.children, vec![1, 2]);
    assert_eq!(m.multipart_parts, 1);
    assert_eq!(m.text_parts, 1);
    assert_eq!(m.attachment_count, 1);
    assert!(m.has_from);
    assert!(m.has_mime_version);

    let text = m.part(1).unwrap();
    assert_eq!(text.kind, K_TEXT);
    assert_eq!(text.cte, CTE_QP);
    // Exact quoted-printable decoded bytes (`Café and tea`).
    assert_eq!(
        vole_document::adapter::eml::decode_body(DOC, text, Limits::DEFAULT).unwrap(),
        "Café and tea".as_bytes()
    );

    let att = m.part(2).unwrap();
    assert_eq!(att.cte, CTE_BASE64);
    assert_eq!(att.filename.as_deref(), Some("report.bin"));
    assert_eq!(
        vole_document::adapter::eml::decode_body(DOC, att, Limits::DEFAULT).unwrap(),
        b"Report data"
    );
    assert_eq!(m.attachment_indices(), vec![2]);

    // The message body text resolves to the text/plain part.
    assert_eq!(
        vole_document::adapter::eml::body_text(&m, DOC, Limits::DEFAULT).unwrap(),
        "Café and tea"
    );
}

#[test]
fn native_and_common_selectors_resolve() {
    let mut fx = Fixture::new("selectors", DOC);
    let field = fx.report.field;

    // `eml-header` — the duplicate `Received:` headers, each with exact spans.
    let hj = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        header("Received"),
        Representation::Metadata,
    ));
    assert!(hj.contains("\"part\":0"), "{hj}");
    assert!(
        hj.contains("from a.example by b.example") && hj.contains("from c.example by d.example"),
        "expected two Received headers: {hj}"
    );
    // ExactBytes returns the first match's raw value bytes.
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            header("Received"),
            Representation::ExactBytes,
        )),
        b" from a.example by b.example"
    );

    // `eml-part` — the attachment part's exact decoded bytes and its descriptor.
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            Selector::EmlPart { index: 2 },
            Representation::ExactBytes,
        )),
        b"Report data"
    );
    let pj = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::EmlPart { index: 2 },
        Representation::Metadata,
    ));
    assert!(pj.contains("\"kind\":\"other\""), "{pj}");
    assert!(pj.contains("\"cte\":\"base64\""), "{pj}");
    assert!(pj.contains("\"filename\":\"report.bin\""), "{pj}");
    let pj = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::EmlPart { index: 0 },
        Representation::Structure,
    ));
    assert!(pj.contains("\"kind\":\"multipart\""), "{pj}");
    assert!(pj.contains("\"boundary\":\"MIX\""), "{pj}");

    // `eml-attachments` — one attachment with its decoded size.
    let aj = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::EmlAttachments,
        Representation::Metadata,
    ));
    assert!(aj.contains("\"filename\":\"report.bin\""), "{aj}");
    assert!(aj.contains("\"decoded_len\":11"), "{aj}");

    // `eml-body` — the message body text.
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::EmlBody,
            Representation::Text,
        )),
        "Café and tea"
    );

    // `eml-find` — a lexical search over headers and the body.
    let fj = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::EmlFind {
            pattern: "Alice".to_string(),
        },
        Representation::Text,
    ));
    assert!(fj.contains("\"location\":\"header-value\""), "{fj}");
    assert!(fj.contains("\"name\":\"From\""), "{fj}");

    // Common metadata / text / resource.
    let meta = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(meta.contains("\"format\":\"eml\""), "{meta}");
    assert!(meta.contains("\"parts\":3"), "{meta}");
    assert!(meta.contains("\"attachments\":1"), "{meta}");
    assert!(meta.contains("\"has_mime_version\":true"), "{meta}");
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert_eq!(text, "Café and tea");
    // Common `resource 0` — the attachment's exact decoded bytes.
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            Selector::Resource(0),
            Representation::ExactBytes,
        )),
        b"Report data"
    );
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("example".to_string()),
        Representation::Text,
    ));
    assert!(found.contains("example"), "{found}");

    // Missing headers/parts/attachments decline typed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            header("X-Nope"),
            Representation::Metadata
        ),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::EmlPart { index: 99 },
            Representation::Metadata
        ),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::Resource(9),
            Representation::ExactBytes
        ),
        ErrorClass::UnsupportedFeature
    );
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
    let src_path = root.join("doc.eml");
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
    let bytes = answer_bytes(&observe_ok(
        &mut store2,
        &field_id,
        Selector::EmlPart { index: 2 },
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"Report data");

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), DOC.len());
    assert_eq!(sha256(&exact), sha256(DOC));
    assert_eq!(exact, DOC);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn bombs_and_caps_decline_typed() {
    use vole_document::adapter::eml::{decode_body, parse};

    // A deeply nested message/rfc822 tree declines typed under STRICT.
    let mut deep = b"body\r\n".to_vec();
    for _ in 0..20 {
        let mut outer = Vec::new();
        outer.extend_from_slice(
            b"From: a@b\r\nDate: x\r\nMessage-ID: <m>\r\nContent-Type: message/rfc822\r\n\r\n",
        );
        outer.extend_from_slice(&deep);
        deep = outer;
    }
    assert_eq!(
        parse(&deep, Limits::STRICT).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );

    // An over-cap base64 decode declines typed.
    let big = b"From: a@b\r\nDate: x\r\nMessage-ID: <m>\r\nContent-Transfer-Encoding: base64\r\n\r\nQUJDREVGR0g=\r\n"; // "ABCDEFGH"
    let m = parse(big, Limits::DEFAULT).unwrap();
    let tight = Limits {
        max_eml_decoded_bytes: 2,
        ..Limits::DEFAULT
    };
    assert_eq!(
        decode_body(big, m.part(0).unwrap(), tight)
            .unwrap_err()
            .class(),
        ErrorClass::ResourceLimit
    );

    // An over-cap header count declines typed.
    let mut many = Vec::new();
    for i in 0..40 {
        many.extend_from_slice(format!("X-H{i}: v\r\n").as_bytes());
    }
    many.extend_from_slice(b"From: a@b\r\n\r\nbody\r\n");
    let tight_h = Limits {
        max_eml_headers: 8,
        ..Limits::DEFAULT
    };
    assert_eq!(
        parse(&many, tight_h).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );

    // The opaque floor still closes the deep source exactly.
    let fx = Fixture::new("deep", &deep);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn interleaved_fixtures_do_not_alias() {
    // ADR-0060: a source-reading node must carry its own source-identity input.
    let root = temp_dir("alias");
    let mut store = FieldStore::open(&root).unwrap();

    let a = b"From: a@b\r\nDate: x\r\nMessage-ID: <a>\r\nSubject: AAA\r\n\r\nbody A\r\n".as_slice();
    let b = b"From: c@d\r\nDate: y\r\nMessage-ID: <b>\r\nSubject: BBB\r\n\r\nbody B\r\n".as_slice();
    let da = opaque_descriptor(a);
    let db = opaque_descriptor(b);
    let fa = ingest_pdf(&mut store, &da, Limits::DEFAULT).unwrap().field;
    let fb = ingest_pdf(&mut store, &db, Limits::DEFAULT).unwrap().field;

    for (field, src, want) in [(fa, a, "AAA".to_string()), (fb, b, "BBB".to_string())] {
        let got = answer_json(&observe_ok(
            &mut store,
            &field,
            header("Subject"),
            Representation::Metadata,
        ));
        assert!(got.contains(&want), "aliased source for {field:?}: {got}");
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
fn eml_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived EML model reads the source bytes, so its single
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
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_EML_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::EmlModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the EML model must depend on exactly the exact root"
    );
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0x1357_9BDF_2468_ACE0;
    for _ in 0..128 {
        let mut buf = vec![0u8; 512];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        if let Ok(m) = vole_document::adapter::eml::parse(&buf, Limits::STRICT) {
            let _ = vole_document::adapter::eml::body_text(&m, &buf, Limits::STRICT);
            let _ = vole_document::adapter::eml::find(&m, &buf, "a", Limits::STRICT);
            for p in &m.parts {
                let _ = vole_document::adapter::eml::decode_body(&buf, p, Limits::STRICT);
            }
        }
        let _ = vole_document::adapter::eml::build_eml_model(&buf, Limits::STRICT);
    }
}
