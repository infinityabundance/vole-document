//! Phase 21.29 court: the package-metadata adapter (the manifest Wave-2 format).
//!
//! Gated on `field` + `pkgmeta` (which implies `json` and `toml`), so a build without
//! the feature compiles an empty target.
//!
//! A manifest's physical bytes are JSON or TOML, so the representation-preserving
//! parsers are the **shared** JSON and TOML adapters (never a second parser). The
//! distinct claim is a **bounded semantic sub-detection** run before the generic JSON
//! and TOML detectors, plus a span-preserving section/entry projection. The court
//! checks:
//!
//! 1. **Byte-based detection and the boundaries**: an npm `package.json`, a Cargo
//!    `Cargo.toml`, a Python `pyproject.toml`, a Cargo `Cargo.lock`, and an npm
//!    `package-lock.json` → `Pkgmeta` with the right recorded dialect; a generic JSON
//!    `{"name":…,"version":…}` stays `Json`; a generic TOML `name`/`version` stays
//!    `Toml`; prose stays `Opaque`.
//! 2. **Representation preservation**: the exact section name/key/value spans, member
//!    order, duplicate keys/members, numeric and string spelling, inline-table and
//!    array-valued dependency specs, and array-of-tables (`[[package]]`/`[[bin]]`)
//!    elements — nothing normalized or re-serialized.
//! 3. **Selectors**: native `pkgmeta-section`/`pkgmeta-entry`/`pkgmeta-key`/
//!    `pkgmeta-find` plus common `metadata`/`text`/`search-match`.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 5. **Fail closed**: a non-manifest JSON/TOML document declines typed; out-of-range
//!    sections/entries and an absent key decline typed; a malformed key reference is a
//!    usage error; random bytes never panic.
//! 6. **No cross-field aliasing** (ADR-0060): the model node depends on the exact
//!    `sha256(source)` root.

#![cfg(all(feature = "field", feature = "pkgmeta"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_PKGMETA_MODEL, SelectorKey, lookup};
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
        "vole-pkgmeta-{label}-{}-{}",
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
        format_basis: "opaque:pkgmeta-test".to_string(),
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

fn section(index: u32) -> Selector {
    Selector::PkgmetaSection { index }
}

fn entry(index: u32) -> Selector {
    Selector::PkgmetaEntry { index }
}

fn key(spec: &str) -> Selector {
    Selector::PkgmetaKey {
        spec: spec.to_string(),
    }
}

const NPM: &[u8] = br#"{
  "name": "demo",
  "version": "1.0.0",
  "description": "A demo package.",
  "main": "index.js",
  "scripts": { "build": "tsc", "test": "jest" },
  "dependencies": { "react": "^18.0.0", "left-pad": "1.0.0" },
  "devDependencies": { "typescript": "^5.0.0" },
  "engines": { "node": ">=18" }
}
"#;

const CARGO: &[u8] = br#"[package]
name = "demo"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"] }
regex = "1"

[dev-dependencies]
tempfile = "3"

[features]
default = ["std"]
std = []

[[bin]]
name = "demo"
path = "src/main.rs"

[profile.release]
opt-level = 3
"#;

const PYPROJECT: &[u8] = br#"[build-system]
requires = ["setuptools>=61"]
build-backend = "setuptools.build_meta"

[project]
name = "demo"
version = "1.0.0"
dependencies = ["requests>=2"]

[tool.poetry]
name = "demo"
"#;

const CARGO_LOCK: &[u8] = br#"version = 3

[[package]]
name = "demo"
version = "0.1.0"
dependencies = ["serde"]
"#;

const NPM_LOCK: &[u8] = br#"{"name":"demo","version":"1.0.0","lockfileVersion":3,"packages":{"":{"name":"demo","version":"1.0.0"},"node_modules/react":{"version":"18.0.0"}}}"#;

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_content_only_and_bounded() {
    use vole_document::adapter::pkgmeta::{
        D_CARGO_LOCK, D_CARGO_MANIFEST, D_NPM_LOCK, D_NPM_PACKAGE, D_PYPROJECT_MANIFEST, parse,
    };

    assert_eq!(
        detect_document_format(NPM, Limits::DEFAULT),
        DocumentFormat::Pkgmeta
    );
    assert_eq!(
        detect_document_format(CARGO, Limits::DEFAULT),
        DocumentFormat::Pkgmeta
    );
    assert_eq!(
        detect_document_format(PYPROJECT, Limits::DEFAULT),
        DocumentFormat::Pkgmeta
    );
    assert_eq!(
        detect_document_format(CARGO_LOCK, Limits::DEFAULT),
        DocumentFormat::Pkgmeta
    );
    assert_eq!(
        detect_document_format(NPM_LOCK, Limits::DEFAULT),
        DocumentFormat::Pkgmeta
    );

    assert_eq!(
        parse(NPM, Limits::DEFAULT, false).unwrap().dialect,
        D_NPM_PACKAGE
    );
    assert_eq!(
        parse(CARGO, Limits::DEFAULT, false).unwrap().dialect,
        D_CARGO_MANIFEST
    );
    assert_eq!(
        parse(PYPROJECT, Limits::DEFAULT, false).unwrap().dialect,
        D_PYPROJECT_MANIFEST
    );
    assert_eq!(
        parse(CARGO_LOCK, Limits::DEFAULT, false).unwrap().dialect,
        D_CARGO_LOCK
    );
    assert_eq!(
        parse(NPM_LOCK, Limits::DEFAULT, false).unwrap().dialect,
        D_NPM_LOCK
    );

    // The recorded boundary: a generic JSON `name`+`version` stays Json; a generic
    // TOML `name`/`version` (no `[package]` table) stays Toml.
    assert_eq!(
        detect_document_format(br#"{"name":"x","version":"1.0.0"}"#, Limits::DEFAULT),
        DocumentFormat::Json
    );
    assert_eq!(
        detect_document_format(b"name = \"x\"\nversion = \"1.0.0\"\n", Limits::DEFAULT),
        DocumentFormat::Toml
    );
    // A JSON document with a third package-specific key IS a manifest.
    assert_eq!(
        detect_document_format(
            br#"{"name":"x","version":"1.0.0","scripts":{"build":"tsc"}}"#,
            Limits::DEFAULT
        ),
        DocumentFormat::Pkgmeta
    );
    // A JSON object that merely has `version` and `packages` but no `lockfileVersion`
    // stays Json (never guessed to be a lockfile).
    assert_eq!(
        detect_document_format(
            br#"{"name":"x","version":"1.0.0","packages":{}}"#,
            Limits::DEFAULT
        ),
        DocumentFormat::Json
    );
    // Prose stays Opaque.
    assert_eq!(
        detect_document_format(
            b"This is just prose.\nWith a second line.\n",
            Limits::DEFAULT
        ),
        DocumentFormat::Opaque
    );

    let caps = capabilities_for_format(DocumentFormat::Pkgmeta);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for n in [
        "pkgmeta-section",
        "pkgmeta-entry",
        "pkgmeta-key",
        "pkgmeta-find",
    ] {
        assert!(caps.native_selectors.contains(&n), "missing {n}");
    }
    assert!(caps.to_json().contains("\"format\":\"pkgmeta\""));
}

#[test]
fn npm_package_preserves_order_duplicates_and_spelling() {
    use vole_document::adapter::pkgmeta::{
        NONE, entry_key_text, entry_value_bytes, parse, section_name,
    };

    let m = parse(NPM, Limits::DEFAULT, true).unwrap();
    assert_eq!(m.sections.len(), 5);
    assert_eq!(m.entries.len(), 14);

    // Sections: root, scripts, dependencies, devDependencies, engines.
    let names: Vec<String> = m
        .sections
        .iter()
        .map(|s| section_name(&m, NPM, s).unwrap())
        .collect();
    assert_eq!(
        names,
        vec!["", "scripts", "dependencies", "devDependencies", "engines"]
    );
    assert_eq!(m.sections[0].name_node, NONE);
    assert_eq!(
        m.sections[2].role,
        vole_document::adapter::pkgmeta::ROLE_DEPENDENCIES
    );

    // Every dependency member keeps its exact key and value token, in order.
    let s = &m.sections[2]; // dependencies
    let first = s.first_entry as usize;
    let mut pairs: Vec<(String, String)> = Vec::new();
    for e in &m.entries[first..first + s.entry_count as usize] {
        pairs.push((
            entry_key_text(&m, NPM, e).unwrap(),
            String::from_utf8_lossy(entry_value_bytes(&m, NPM, e).unwrap()).into_owned(),
        ));
    }
    assert_eq!(
        pairs,
        vec![
            ("react".to_string(), "\"^18.0.0\"".to_string()),
            ("left-pad".to_string(), "\"1.0.0\"".to_string()),
        ]
    );

    // Duplicate members are preserved verbatim.
    let dup = br#"{"name":"demo","version":"1.0.0","dependencies":{"react":"^18.0.0","react":"^19.0.0"}}"#;
    let m = parse(dup, Limits::DEFAULT, true).unwrap();
    let s = &m.sections[1]; // dependencies
    assert_eq!(s.entry_count, 2);
    let a = entry_value_bytes(&m, dup, &m.entries[s.first_entry as usize]).unwrap();
    let b = entry_value_bytes(&m, dup, &m.entries[s.first_entry as usize + 1]).unwrap();
    assert_eq!(a, b"\"^18.0.0\"");
    assert_eq!(b, b"\"^19.0.0\"");
}

#[test]
fn cargo_manifest_preserves_inline_tables_and_arrays() {
    use vole_document::adapter::pkgmeta::{entry_value_bytes, parse};

    let m = parse(CARGO, Limits::DEFAULT, true).unwrap();
    assert_eq!(m.dialect_name(), "cargo_manifest");
    assert_eq!(m.sections.len(), 13);
    assert_eq!(m.entries.len(), 20);

    // The inline-table dependency spec is preserved byte-for-byte (never re-serialized).
    let deps = m.sections.iter().find(|s| {
        vole_document::adapter::pkgmeta::section_name(&m, CARGO, s).unwrap() == "dependencies"
    });
    let deps = deps.expect("dependencies section");
    let first = deps.first_entry as usize;
    assert_eq!(
        entry_value_bytes(&m, CARGO, &m.entries[first]).unwrap(),
        b"{ version = \"1\", features = [\"derive\"] }"
    );
    // A `[[bin]]` element table is a recorded section with its own entries.
    let bin_el = m
        .sections
        .iter()
        .find(|s| s.kind == vole_document::adapter::pkgmeta::S_ELEMENT)
        .expect("[[bin]] element");
    assert_eq!(bin_el.entry_count, 2);
    // `[profile.release]` nests: a table section under `profile`.
    assert!(m.sections.iter().any(|s| {
        vole_document::adapter::pkgmeta::section_name(&m, CARGO, s).unwrap() == "release"
    }));
}

#[test]
fn pyproject_and_lockfiles_detect_and_record_dialect() {
    use vole_document::adapter::pkgmeta::parse;

    let m = parse(PYPROJECT, Limits::DEFAULT, true).unwrap();
    assert_eq!(m.dialect_name(), "pyproject_manifest");
    assert_eq!(m.sections.len(), 7);
    assert_eq!(m.entries.len(), 10);

    let m = parse(CARGO_LOCK, Limits::DEFAULT, true).unwrap();
    assert_eq!(m.dialect_name(), "cargo_lock");
    assert_eq!(m.sections.len(), 4);
    assert_eq!(m.entries.len(), 5);
    // Root scalar `version = 3` keeps its exact (integer) token, never parsed.
    assert_eq!(
        vole_document::adapter::pkgmeta::entry_value_bytes(&m, CARGO_LOCK, &m.entries[0]).unwrap(),
        b"3"
    );
    // The `[[package]]` element's `name` keeps its exact (quoted) spelling.
    let name_entry = m
        .entries
        .iter()
        .find(|e| {
            vole_document::adapter::pkgmeta::entry_key_text(&m, CARGO_LOCK, e).unwrap() == "name"
        })
        .expect("a `name` entry");
    assert_eq!(
        vole_document::adapter::pkgmeta::entry_value_bytes(&m, CARGO_LOCK, name_entry).unwrap(),
        b"\"demo\""
    );

    let m = parse(NPM_LOCK, Limits::DEFAULT, true).unwrap();
    assert_eq!(m.dialect_name(), "npm_lock");
    assert_eq!(m.sections.len(), 4);
    assert_eq!(m.entries.len(), 9);
}

#[test]
fn native_and_common_selectors_resolve() {
    let mut fx = Fixture::new("selectors-npm", NPM);
    let field_id = fx.report.field;

    // `pkgmeta-section 0` is the root.
    let md = answer_json(&observe_ok(
        &mut fx.store,
        &field_id,
        section(0),
        Representation::Metadata,
    ));
    assert!(md.contains("\"role\":\"root\""), "{md}");
    assert!(md.contains("\"kind\":\"root\""), "{md}");
    // `pkgmeta-section 2` is `dependencies`.
    let md = answer_json(&observe_ok(
        &mut fx.store,
        &field_id,
        section(2),
        Representation::Metadata,
    ));
    assert!(md.contains("\"dialect\":\"npm_package\""), "{md}");
    assert!(md.contains("\"role\":\"dependencies\""), "{md}");
    assert!(md.contains("\"name\":\"dependencies\""), "{md}");

    // `pkgmeta-key dependencies:react` returns the exact value bytes.
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field_id,
            key("dependencies:react"),
            Representation::ExactBytes,
        )),
        b"\"^18.0.0\""
    );
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field_id,
            key("dependencies:react"),
            Representation::Text,
        )),
        "^18.0.0"
    );
    // A role name works as the section reference too.
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field_id,
            key("dev-dependencies:typescript"),
            Representation::ExactBytes,
        )),
        b"\"^5.0.0\""
    );

    // `pkgmeta-entry` addressing by index.
    let md = answer_json(&observe_ok(
        &mut fx.store,
        &field_id,
        entry(1),
        Representation::Metadata,
    ));
    assert!(md.contains("\"key\":\"version\""), "{md}");
    assert!(md.contains("\"value\":\"1.0.0\""), "{md}");

    // `pkgmeta-find` reports the section index and role.
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field_id,
        Selector::PkgmetaFind {
            pattern: "left-pad".to_string(),
        },
        Representation::Text,
    ));
    assert!(found.contains("\"role\":\"key\""), "{found}");
    assert!(found.contains("left-pad"), "{found}");

    // Common metadata / text / search-match.
    let meta = answer_json(&observe_ok(
        &mut fx.store,
        &field_id,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(meta.contains("\"format\":\"pkgmeta\""), "{meta}");
    assert!(meta.contains("\"dialect\":\"npm_package\""), "{meta}");
    assert!(meta.contains("\"sections\":5"), "{meta}");
    assert!(meta.contains("\"entries\":14"), "{meta}");
    assert!(meta.contains("\"root_entries\":8"), "{meta}");
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field_id,
        Selector::Text,
        Representation::Text,
    ));
    assert!(text.contains("left-pad"), "{text}");
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field_id,
        Selector::SearchMatch("jest".to_string()),
        Representation::Text,
    ));
    assert!(found.contains("jest"), "{found}");

    // TOML manifest selectors.
    let mut cx = Fixture::new("selectors-cargo", CARGO);
    let cf = cx.report.field;
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut cx.store,
            &cf,
            key("package:version"),
            Representation::ExactBytes,
        )),
        b"\"0.1.0\""
    );
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut cx.store,
            &cf,
            key("dependencies:serde"),
            Representation::ExactBytes,
        )),
        b"{ version = \"1\", features = [\"derive\"] }"
    );
    let meta = answer_json(&observe_ok(
        &mut cx.store,
        &cf,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(meta.contains("\"dialect\":\"cargo_manifest\""), "{meta}");
    assert!(meta.contains("\"tables\":7"), "{meta}");
    assert!(meta.contains("\"arrays\":4"), "{meta}");
}

#[test]
fn declines_are_typed_never_silent() {
    let mut fx = Fixture::new("declines", NPM);
    let f = fx.report.field;

    // Out-of-range section and entry decline typed.
    assert_eq!(
        observe_err(&mut fx.store, &f, section(99), Representation::Metadata),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(&mut fx.store, &f, entry(9999), Representation::Metadata),
        ErrorClass::UnsupportedFeature
    );
    // An absent section or key declines typed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &f,
            key("nope:react"),
            Representation::Metadata
        ),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &f,
            key("dependencies:nope"),
            Representation::Metadata
        ),
        ErrorClass::UnsupportedFeature
    );
    // A malformed reference is a usage error.
    assert_eq!(
        observe_err(&mut fx.store, &f, key("no-colon"), Representation::Metadata),
        ErrorClass::Usage
    );
    assert_eq!(
        observe_err(&mut fx.store, &f, key(":react"), Representation::Metadata),
        ErrorClass::Usage
    );
    // An unsupported common pair declines typed.
    assert_eq!(
        observe_err(&mut fx.store, &f, Selector::Table(0), Representation::Text),
        ErrorClass::UnsupportedFeature
    );
}

#[test]
fn malformed_and_non_manifest_decline_typed() {
    use vole_document::adapter::pkgmeta::{detect, parse};

    // A JSON value that is not a manifest shape declines typed.
    assert!(!detect(br#"{"a":1,"b":2}"#, Limits::DEFAULT));
    assert_eq!(
        parse(br#"{"a":1,"b":2}"#, Limits::DEFAULT, true)
            .unwrap_err()
            .class(),
        ErrorClass::InvalidPkgmetaStructure
    );
    assert_eq!(
        parse(b"[1,2,3]", Limits::DEFAULT, true)
            .unwrap_err()
            .class(),
        ErrorClass::InvalidPkgmetaStructure
    );
    // A TOML value with no package table declines typed.
    assert_eq!(
        parse(b"a = 1\nb = 2\n", Limits::DEFAULT, true)
            .unwrap_err()
            .class(),
        ErrorClass::InvalidPkgmetaStructure
    );
    // A malformed TOML source is not claimed (falls to Opaque/Toml elsewhere).
    assert!(!detect(b"[package]\nname = \n", Limits::DEFAULT));
    assert_eq!(
        parse(b"[package]\nname = \n", Limits::DEFAULT, true)
            .unwrap_err()
            .class(),
        ErrorClass::InvalidPkgmetaStructure
    );
}

#[test]
fn exact_materialization_is_byte_identical() {
    for src in [NPM, CARGO, PYPROJECT, CARGO_LOCK, NPM_LOCK] {
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
    let src_path = root.join("Cargo.toml");
    let desc_path = root.join("doc.voldoc");
    std::fs::write(&src_path, CARGO).unwrap();
    let descriptor = opaque_descriptor(CARGO);
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
        key("dependencies:serde"),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"{ version = \"1\", features = [\"derive\"] }");

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), CARGO.len());
    assert_eq!(sha256(&exact), sha256(CARGO));
    assert_eq!(exact, CARGO);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn pkgmeta_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived package-metadata model reads the source bytes, so its
    // single dependency must be the exact `DocumentExact` root (keyed by
    // `sha256(source)`). This is what prevents cross-field source aliasing.
    let fx = Fixture::new("adr0060", CARGO);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let manifest = field.manifest().clone();
    let root = manifest.root_node;
    let root_node = load_node(fx.store.seeds(), &root).unwrap();
    assert_eq!(root_node.kind, NodeKind::DocumentExact);

    assert!(manifest.has_index());
    let istore = FsIndexStore::open(fx.store.root()).unwrap();
    let iroot = NodeId::from_bytes(manifest.index_root);
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_PKGMETA_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::PkgmetaModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the package-metadata model must depend on exactly the exact root"
    );
}

#[test]
fn interleaved_fixtures_do_not_alias() {
    // ADR-0060: a source-reading node must carry its own source-identity input.
    let root = temp_dir("alias");
    let mut store = FieldStore::open(&root).unwrap();

    let a = br#"{"name":"a","version":"1.0.0","dependencies":{"only":"aaa"}}"#.as_slice();
    let b = br#"{"name":"b","version":"2.0.0","dependencies":{"only":"bbb"}}"#.as_slice();
    let da = opaque_descriptor(a);
    let db = opaque_descriptor(b);
    let fa = ingest_pdf(&mut store, &da, Limits::DEFAULT).unwrap().field;
    let fb = ingest_pdf(&mut store, &db, Limits::DEFAULT).unwrap().field;

    for (f, src, want) in [(fa, a, b"\"aaa\"".to_vec()), (fb, b, b"\"bbb\"".to_vec())] {
        let got = answer_bytes(&observe_ok(
            &mut store,
            &f,
            key("dependencies:only"),
            Representation::ExactBytes,
        ));
        assert_eq!(got, want, "aliased source for {f:?}");
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
fn caps_decline_typed() {
    use vole_document::adapter::pkgmeta::parse;

    let tight_sections = Limits {
        max_pkgmeta_sections: 1,
        ..Limits::DEFAULT
    };
    assert!(!vole_document::adapter::pkgmeta::detect(
        NPM,
        tight_sections
    ));
    assert_eq!(
        parse(NPM, tight_sections, true).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );

    let tight_entries = Limits {
        max_pkgmeta_entries: 1,
        ..Limits::DEFAULT
    };
    assert_eq!(
        parse(NPM, tight_entries, true).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );

    let tight_nodes = Limits {
        max_pkgmeta_nodes: 1,
        ..Limits::DEFAULT
    };
    assert_eq!(
        parse(NPM, tight_nodes, true).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );

    let tight_depth = Limits {
        max_pkgmeta_depth: 1,
        ..Limits::DEFAULT
    };
    assert_eq!(
        parse(CARGO, tight_depth, true).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );

    // The opaque floor still closes the source exactly.
    let fx = Fixture::new("caps", NPM);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn random_bytes_never_panic() {
    use vole_document::adapter::pkgmeta::{
        PkgmetaModel, build_pkgmeta_model, canonical_text, detect, find, parse,
    };

    let mut x: u64 = 0x2545_F491_4F6C_DD1D;
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
            let _ = canonical_text(&m, &buf);
            let _ = find(&m, &buf, "a", Limits::STRICT);
        }
        let _ = build_pkgmeta_model(&buf, Limits::STRICT);
        let _ = PkgmetaModel::decode(&buf);
    }
}
