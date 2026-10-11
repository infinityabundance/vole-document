//! Phase 21.31 court: the bibliography adapter (BibTeX / RIS / CSL-JSON).
//!
//! Gated on `field` + `bib` (which implies `json`), so a build without the feature
//! compiles an empty target.
//!
//! One bounded adapter covers three dialects and records the dialect in the model. The
//! court checks:
//!
//! 1. **Byte-based detection and the boundaries**: a BibTeX entry sequence, an RIS
//!    `TY`/`ER` sequence, and a CSL-JSON item array/object → `Bib` with the right
//!    recorded dialect; a plain JSON array stays `Json`; a JSON array of non-CSL
//!    objects stays `Json`; a generic JSON object stays `Json`; prose stays `Opaque`.
//! 2. **Representation preservation**: exact entry/field spans, entry key, entry type
//!    and field names *as written*, field order, every value's exact spelling and
//!    delimiters (brace/quoted/number/macro distinct), duplicate-field reporting,
//!    `@string`/`@preamble`/`@comment`, the RIS two-space separator and multi-line
//!    continuation span, and CSL-JSON item/`id`/`type` — nothing normalized.
//! 3. **Selectors**: native `bib-dialect`/`bib-entry`/`bib-field`/`bib-key`/`bib-find`
//!    plus common `metadata`/`text`/`search-match`.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 5. **Fail closed**: a malformed/ambiguous input declines typed; out-of-range
//!    entries/fields lead to typed declines; random bytes never panic.
//! 6. **No cross-field aliasing** (ADR-0060): the model node depends on the exact
//!    `sha256(source)` root.

#![cfg(all(feature = "field", feature = "bib"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_BIB_MODEL, SelectorKey, lookup};
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
        "vole-bib-{label}-{}-{}",
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
        format_basis: "opaque:bib-test".to_string(),
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

const BIBTEX: &[u8] = br#"@article{smith2020,
  author = {Smith, John and {Doe}, Jane},
  title = "A {Study} of Things",
  year = 2020,
  month = jan,
  note = {first},
  note = {second}
}

@string{ acm = "Association for Computing Machinery" }

@comment{ a comment }
"#;

const RIS: &[u8] = b"TY  - JOUR\nAU  - Smith, John\nTI  - A long title\n      continued here\nPY  - 2020\nER  - \n";

const CSL: &[u8] = br#"[
  {"id":"smith2020","type":"article-journal","title":"A Study"},
  {"id":"doe2019","type":"book","title":"Things"}
]
"#;

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_content_only_and_bounded() {
    use vole_document::adapter::bib::{D_BIBTEX, D_CSL_JSON, D_RIS, parse};

    assert_eq!(
        detect_document_format(BIBTEX, Limits::DEFAULT),
        DocumentFormat::Bib
    );
    assert_eq!(
        detect_document_format(RIS, Limits::DEFAULT),
        DocumentFormat::Bib
    );
    assert_eq!(
        detect_document_format(CSL, Limits::DEFAULT),
        DocumentFormat::Bib
    );

    assert_eq!(
        parse(BIBTEX, Limits::DEFAULT, false).unwrap().dialect,
        D_BIBTEX
    );
    assert_eq!(parse(RIS, Limits::DEFAULT, false).unwrap().dialect, D_RIS);
    assert_eq!(
        parse(CSL, Limits::DEFAULT, false).unwrap().dialect,
        D_CSL_JSON
    );

    // The recorded boundaries: a plain JSON array stays Json; a JSON array of non-CSL
    // objects stays Json; a generic JSON object stays Json; prose stays Opaque.
    assert_eq!(
        detect_document_format(br#"[{"a":1},{"b":2}]"#, Limits::DEFAULT),
        DocumentFormat::Json
    );
    assert_eq!(
        detect_document_format(br#"[{"id":"x"}]"#, Limits::DEFAULT),
        DocumentFormat::Json
    );
    assert_eq!(
        detect_document_format(br#"{"id":"x","type":"y"}"#, Limits::DEFAULT),
        DocumentFormat::Bib
    );
    assert_eq!(
        detect_document_format(br#"{"name":"x","version":"1.0.0"}"#, Limits::DEFAULT),
        DocumentFormat::Json
    );
    assert_eq!(
        detect_document_format(
            b"This is just prose.\nWith a second line.\n",
            Limits::DEFAULT
        ),
        DocumentFormat::Opaque
    );

    let caps = capabilities_for_format(DocumentFormat::Bib);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for n in [
        "bib-dialect",
        "bib-entry",
        "bib-field",
        "bib-key",
        "bib-find",
    ] {
        assert!(caps.native_selectors.contains(&n), "missing {n}");
    }
    assert!(caps.to_json().contains("\"format\":\"bib\""));
}

#[test]
fn bibtex_preserves_value_forms_order_and_duplicates() {
    use vole_document::adapter::bib::{
        E_COMMENT, E_ENTRY, E_STRING, FF_DUPLICATE, V_BRACED, V_MACRO, V_NUMBER, V_QUOTED,
        entry_key_text, entry_type_text, field_value_text, parse,
    };

    let m = parse(BIBTEX, Limits::DEFAULT, true).unwrap();
    assert_eq!(m.entries.len(), 3);
    let e = &m.entries[0];
    assert_eq!(e.kind, E_ENTRY);
    assert_eq!(entry_type_text(BIBTEX, e).unwrap(), "article");
    assert_eq!(entry_key_text(BIBTEX, e).unwrap(), "smith2020");
    assert_eq!(e.fields.len(), 6);
    // Each value form is distinct.
    assert_eq!(e.fields[0].value_kind, V_BRACED);
    assert_eq!(e.fields[1].value_kind, V_QUOTED);
    assert_eq!(e.fields[2].value_kind, V_NUMBER);
    assert_eq!(e.fields[3].value_kind, V_MACRO);
    // Nested braces are retained inside the value span.
    let author = field_value_text(BIBTEX, &e.fields[0]).unwrap();
    assert!(author.contains("{Doe}"));
    // Duplicate field is preserved and reported on the later occurrence.
    assert_eq!(e.fields[4].flags & FF_DUPLICATE, 0);
    assert_eq!(e.fields[5].flags & FF_DUPLICATE, FF_DUPLICATE);
    assert_eq!(e.duplicate_field_count(), 1);
    // @string and @comment entries.
    assert_eq!(m.entries[1].kind, E_STRING);
    assert_eq!(m.entries[2].kind, E_COMMENT);
}

#[test]
fn ris_preserves_tags_order_and_continuation() {
    use vole_document::adapter::bib::{FF_CONTINUED, field_name_text, field_value_text, parse};

    let m = parse(RIS, Limits::DEFAULT, true).unwrap();
    assert_eq!(m.entries.len(), 1);
    let e = &m.entries[0];
    assert_eq!(e.fields.len(), 5);
    assert_eq!(field_name_text(RIS, &e.fields[0]).unwrap(), "TY");
    assert_eq!(field_name_text(RIS, &e.fields[4]).unwrap(), "ER");
    // The TI value spans its continuation line.
    assert_eq!(e.fields[2].flags & FF_CONTINUED, FF_CONTINUED);
    let v = field_value_text(RIS, &e.fields[2]).unwrap();
    assert!(v.contains("A long title"));
    assert!(v.contains("continued here"));
}

#[test]
fn csl_preserves_items_and_strings() {
    use vole_document::adapter::bib::{item_id_text, item_member_node, item_type_text, parse};

    let m = parse(CSL, Limits::DEFAULT, true).unwrap();
    assert_eq!(m.items.len(), 2);
    assert_eq!(item_id_text(&m, CSL, &m.items[0]).unwrap(), "smith2020");
    assert_eq!(
        item_type_text(&m, CSL, &m.items[0]).unwrap(),
        "article-journal"
    );
    assert_eq!(item_id_text(&m, CSL, &m.items[1]).unwrap(), "doe2019");
    // A member lookup is by key; membership order is preserved.
    assert!(item_member_node(&m, CSL, 0, "title").unwrap().is_some());
    assert!(item_member_node(&m, CSL, 0, "nope").unwrap().is_none());
}

#[test]
fn native_and_common_selectors_resolve() {
    let mut fx = Fixture::new("selectors", BIBTEX);
    let field = fx.report.field;

    // Native dialect.
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::BibDialect,
            Representation::Text,
        )),
        "bibtex"
    );

    // Native entry: index 0 is the @article entry, Text is its citation key.
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::BibEntry { index: 0 },
            Representation::Text,
        )),
        "smith2020"
    );
    assert!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            Selector::BibEntry { index: 0 },
            Representation::ExactBytes,
        ))
        .starts_with(b"@article{smith2020,")
    );

    // Native field: `0:title` ExactBytes is the quoted value (delimiters included).
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            Selector::BibField {
                spec: "0:title".to_string(),
            },
            Representation::ExactBytes,
        )),
        b"\"A {Study} of Things\""
    );
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::BibField {
                spec: "0:title".to_string(),
            },
            Representation::Text,
        )),
        "A {Study} of Things"
    );

    // Native key: resolve by citation key.
    assert!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            Selector::BibKey {
                spec: "smith2020".to_string(),
            },
            Representation::ExactBytes,
        ))
        .starts_with(b"@article{smith2020,")
    );

    // Native find.
    let find = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::BibFind {
            pattern: "Study".to_string(),
        },
        Representation::Text,
    ));
    assert!(find.contains("\"matches\":["), "{find}");

    // Common metadata / text / search-match.
    let md = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(md.contains("\"format\":\"bib\""), "{md}");
    assert!(md.contains("\"dialect\":\"bibtex\""), "{md}");
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert!(text.contains("@article{smith2020,"), "{text}");
    let sm = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("smith2020".to_string()),
        Representation::Text,
    ));
    assert!(sm.contains("\"matches\":["), "{sm}");
}

#[test]
fn declines_are_typed_never_silent() {
    let mut fx = Fixture::new("declines", BIBTEX);
    let field = fx.report.field;
    // An out-of-range entry declines typed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::BibEntry { index: 99 },
            Representation::Metadata,
        ),
        ErrorClass::UnsupportedFeature
    );
    // An absent field declines typed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::BibField {
                spec: "0:nope".to_string(),
            },
            Representation::Text,
        ),
        ErrorClass::UnsupportedFeature
    );
    // An absent key declines typed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::BibKey {
                spec: "nokey2020".to_string(),
            },
            Representation::Text,
        ),
        ErrorClass::UnsupportedFeature
    );
    // A malformed field reference is a usage error.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::BibField {
                spec: "notaref".to_string(),
            },
            Representation::Text,
        ),
        ErrorClass::Usage
    );
    // An unsupported common pair (table) declines typed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::Table(0),
            Representation::Text,
        ),
        ErrorClass::UnsupportedFeature
    );
}

#[test]
fn malformed_and_non_bibliography_decline_typed() {
    use vole_document::adapter::bib::{detect, parse};

    // A prose blob is not a bibliography.
    assert!(!detect(b"just prose\nwith lines\n", Limits::DEFAULT));
    assert_eq!(
        parse(b"just prose\nwith lines\n", Limits::DEFAULT, true)
            .unwrap_err()
            .class(),
        ErrorClass::InvalidBibStructure
    );
    // A JSON array of non-CSL objects is not CSL-JSON (it stays Json).
    assert_eq!(
        parse(br#"[{"a":1},{"b":2}]"#, Limits::DEFAULT, true)
            .unwrap_err()
            .class(),
        ErrorClass::InvalidBibStructure
    );
    // An unterminated BibTeX entry declines.
    assert_eq!(
        parse(b"@article{k, title = {x}\n", Limits::DEFAULT, true)
            .unwrap_err()
            .class(),
        ErrorClass::InvalidBibStructure
    );
    // An RIS stream without an `ER  - ` terminator declines.
    assert_eq!(
        parse(b"TY  - JOUR\nAU  - Smith\n", Limits::DEFAULT, true)
            .unwrap_err()
            .class(),
        ErrorClass::InvalidBibStructure
    );
    // A complete RIS entry followed by an unterminated one declines too.
    assert_eq!(
        parse(
            b"TY  - JOUR\nER  - \nTY  - BOOK\nAU  - X\n",
            Limits::DEFAULT,
            true
        )
        .unwrap_err()
        .class(),
        ErrorClass::InvalidBibStructure
    );
}

#[test]
fn exact_materialization_is_byte_identical() {
    for src in [BIBTEX, RIS, CSL] {
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
    let src_path = root.join("refs.bib");
    let desc_path = root.join("doc.voldoc");
    std::fs::write(&src_path, BIBTEX).unwrap();
    let descriptor = opaque_descriptor(BIBTEX);
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
        Selector::BibKey {
            spec: "smith2020".to_string(),
        },
        Representation::ExactBytes,
    ));
    assert!(bytes.starts_with(b"@article{smith2020,"));

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), BIBTEX.len());
    assert_eq!(sha256(&exact), sha256(BIBTEX));
    assert_eq!(exact, BIBTEX);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn bib_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived bibliography model reads the source bytes, so its single
    // dependency must be the exact `DocumentExact` root (keyed by `sha256(source)`).
    let fx = Fixture::new("adr0060", CSL);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let manifest = field.manifest().clone();
    let root = manifest.root_node;
    let root_node = load_node(fx.store.seeds(), &root).unwrap();
    assert_eq!(root_node.kind, NodeKind::DocumentExact);

    assert!(manifest.has_index());
    let istore = FsIndexStore::open(fx.store.root()).unwrap();
    let iroot = NodeId::from_bytes(manifest.index_root);
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_BIB_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::BibModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the bibliography model must depend on exactly the exact root"
    );
}

#[test]
fn caps_decline_typed() {
    use vole_document::adapter::bib::{detect, parse};

    let tight = Limits {
        max_bib_entries: 1,
        ..Limits::DEFAULT
    };
    assert!(!detect(BIBTEX, tight));
    assert_eq!(
        parse(BIBTEX, tight, true).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );
    let tight = Limits {
        max_bib_fields: 1,
        ..Limits::DEFAULT
    };
    assert_eq!(
        parse(BIBTEX, tight, true).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );
    let tight = Limits {
        max_bib_value_bytes: 2,
        ..Limits::DEFAULT
    };
    assert_eq!(
        parse(BIBTEX, tight, true).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );
    let tight = Limits {
        max_bib_nodes: 1,
        ..Limits::DEFAULT
    };
    assert_eq!(
        parse(CSL, tight, true).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );
    let tight = Limits {
        max_bib_depth: 1,
        ..Limits::DEFAULT
    };
    assert_eq!(
        parse(CSL, tight, true).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );

    // The opaque floor still closes the source exactly.
    let fx = Fixture::new("caps", BIBTEX);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn random_bytes_never_panic() {
    use vole_document::adapter::bib::{
        BibModel, build_bib_model, canonical_text, detect, find, parse,
    };

    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    for _ in 0..128 {
        let mut buf = vec![0u8; 512];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect(&buf, Limits::DEFAULT);
        if let Ok(m) = parse(&buf, Limits::STRICT, true) {
            let _ = canonical_text(&m, &buf, 1 << 20);
            let _ = find(&m, &buf, "a", Limits::STRICT);
        }
        let _ = build_bib_model(&buf, Limits::STRICT);
        let _ = BibModel::decode(&buf);
    }
}
