//! Phase 21.30 court: the API/specification adapter (the API/spec Wave-2 format).
//!
//! Gated on `field` + `apispec` (which implies `json`), so a build without the feature
//! compiles an empty target.
//!
//! An API spec's physical bytes are JSON, so the representation-preserving parser is the
//! **shared** JSON adapter (never a second parser). The distinct claim is a **bounded
//! semantic sub-detection** run before the generic JSON detector, plus a span-preserving
//! object/member/`$ref` projection and a recorded dialect. The court checks:
//!
//! 1. **Byte-based detection and the boundaries**: a JSON Schema, an OpenAPI 3 document,
//!    a Swagger 2.0 document, and an AsyncAPI document → `Apispec` with the right
//!    recorded dialect; a plain JSON object stays `Json`; a JSON-Schema-**shaped**
//!    object with no `$schema` stays `Json`; prose stays `Opaque`.
//! 2. **Representation preservation**: exact object/member/`$ref` spans, member order,
//!    duplicate keys, keyword spelling, and the exact spec-version string — nothing
//!    normalized, resolved, or re-serialized.
//! 3. **Selectors**: native `apispec-dialect`/`apispec-version`/`apispec-object`/
//!    `apispec-ref`/`apispec-find` plus common `metadata`/`text`/`search-match`.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 5. **Fail closed**: a non-spec JSON document declines typed; out-of-range
//!    objects/`$ref`s decline typed; random bytes never panic.
//! 6. **No cross-field aliasing** (ADR-0060): the model node depends on the exact
//!    `sha256(source)` root.

#![cfg(all(feature = "field", feature = "apispec"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_APISPEC_MODEL, SelectorKey, lookup};
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
        "vole-apispec-{label}-{}-{}",
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
        format_basis: "opaque:apispec-test".to_string(),
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

fn object(index: u32) -> Selector {
    Selector::ApispecObject { index }
}

fn ref_(index: u32) -> Selector {
    Selector::ApispecRef { index }
}

const SCHEMA: &[u8] = br##"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "Person",
  "type": "object",
  "properties": {
    "name": { "type": "string" },
    "age": { "type": "integer", "minimum": 0 }
  },
  "required": ["name"],
  "enum": ["a", "b"],
  "$defs": { "Id": { "type": "string", "format": "uuid" } },
  "definitions": { "Legacy": { "$ref": "#/$defs/Id" } },
  "allOf": [{ "type": "object" }]
}
"##;

const OPENAPI: &[u8] = br##"{
  "openapi": "3.1.0",
  "info": { "title": "Demo", "version": "1.0.0" },
  "servers": [{ "url": "https://api.example.com" }],
  "paths": {
    "/pets": {
      "get": {
        "operationId": "listPets",
        "parameters": [{ "name": "limit", "in": "query" }],
        "responses": { "200": { "description": "ok" } }
      }
    }
  },
  "components": { "schemas": { "Pet": { "$ref": "#/components/schemas/Pet" } } },
  "tags": [{ "name": "pets" }],
  "security": [{ "apiKey": [] }]
}
"##;

const SWAGGER: &[u8] = br##"{
  "swagger": "2.0",
  "info": { "title": "Demo", "version": "1.0.0" },
  "basePath": "/v1",
  "paths": { "/pets": { "get": { "responses": { "200": { "description": "ok" } } } } },
  "definitions": { "Pet": { "type": "object" } },
  "parameters": { "Limit": { "name": "limit", "in": "query" } },
  "responses": { "NotFound": { "description": "nope" } }
}
"##;

const ASYNCAPI: &[u8] = br##"{
  "asyncapi": "2.6.0",
  "info": { "title": "Demo", "version": "1.0.0" },
  "channels": { "user/signedup": { "subscribe": { "message": { "$ref": "#/components/messages/User" } } } },
  "operations": { "sendUser": { "action": "send" } },
  "components": { "messages": { "User": { "name": "User" } } },
  "servers": { "prod": { "url": "broker.example.com" } }
}
"##;

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_content_only_and_bounded() {
    use vole_document::adapter::apispec::{D_ASYNCAPI, D_JSON_SCHEMA, D_OPENAPI, D_SWAGGER, parse};

    assert_eq!(
        detect_document_format(SCHEMA, Limits::DEFAULT),
        DocumentFormat::Apispec
    );
    assert_eq!(
        detect_document_format(OPENAPI, Limits::DEFAULT),
        DocumentFormat::Apispec
    );
    assert_eq!(
        detect_document_format(SWAGGER, Limits::DEFAULT),
        DocumentFormat::Apispec
    );
    assert_eq!(
        detect_document_format(ASYNCAPI, Limits::DEFAULT),
        DocumentFormat::Apispec
    );

    assert_eq!(
        parse(SCHEMA, Limits::DEFAULT, false).unwrap().dialect,
        D_JSON_SCHEMA
    );
    assert_eq!(
        parse(OPENAPI, Limits::DEFAULT, false).unwrap().dialect,
        D_OPENAPI
    );
    assert_eq!(
        parse(SWAGGER, Limits::DEFAULT, false).unwrap().dialect,
        D_SWAGGER
    );
    assert_eq!(
        parse(ASYNCAPI, Limits::DEFAULT, false).unwrap().dialect,
        D_ASYNCAPI
    );

    // The recorded boundaries: a plain JSON object stays Json; a JSON-Schema-*shaped*
    // object with no `$schema` is an honest ambiguity and also stays Json; an unrelated
    // `openapi`/`swagger` value stays Json; prose stays Opaque.
    assert_eq!(
        detect_document_format(br#"{"a":1,"b":2}"#, Limits::DEFAULT),
        DocumentFormat::Json
    );
    assert_eq!(
        detect_document_format(
            br#"{"type":"object","properties":{"a":{"type":"string"}}}"#,
            Limits::DEFAULT
        ),
        DocumentFormat::Json
    );
    assert_eq!(
        detect_document_format(br#"{"properties":{},"paths":{}}"#, Limits::DEFAULT),
        DocumentFormat::Json
    );
    assert_eq!(
        detect_document_format(br#"{"openapi":"2.0","paths":{}}"#, Limits::DEFAULT),
        DocumentFormat::Json
    );
    assert_eq!(
        detect_document_format(
            b"This is just prose.\nWith a second line.\n",
            Limits::DEFAULT
        ),
        DocumentFormat::Opaque
    );

    let caps = capabilities_for_format(DocumentFormat::Apispec);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for n in [
        "apispec-dialect",
        "apispec-version",
        "apispec-object",
        "apispec-ref",
        "apispec-find",
    ] {
        assert!(caps.native_selectors.contains(&n), "missing {n}");
    }
    assert!(caps.to_json().contains("\"format\":\"apispec\""));
}

#[test]
fn schema_preserves_keywords_refs_and_version() {
    use vole_document::adapter::apispec::{
        object_members, parse, ref_target, version_text, version_token,
    };

    let m = parse(SCHEMA, Limits::DEFAULT, true).unwrap();
    assert_eq!(m.dialect_name(), "json_schema");
    // The exact spec-version token and text are preserved verbatim.
    assert_eq!(
        version_token(&m, SCHEMA).unwrap(),
        br#""https://json-schema.org/draft/2020-12/schema""#
    );
    assert_eq!(
        version_text(&m, SCHEMA).unwrap(),
        "https://json-schema.org/draft/2020-12/schema"
    );
    // Keyword spelling preserved.
    let keys: Vec<String> = object_members(&m, SCHEMA, m.root)
        .unwrap()
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    for k in [
        "$defs",
        "definitions",
        "required",
        "enum",
        "allOf",
        "properties",
    ] {
        assert!(keys.contains(&k.to_string()), "missing {k}: {keys:?}");
    }
    // The `$ref` target is preserved verbatim (never resolved).
    assert_eq!(m.refs.len(), 1);
    assert_eq!(ref_target(&m, SCHEMA, &m.refs[0]).unwrap(), "#/$defs/Id");
    // The property tree is walkable: the `properties` object has two member keys.
    use vole_document::adapter::apispec::{NONE, decode_string};
    let props = m
        .objects
        .iter()
        .find(|o| {
            o.name_node != NONE
                && decode_string(SCHEMA, m.node(o.name_node).unwrap()).unwrap() == "properties"
        })
        .expect("a properties object");
    assert!(props.member_count >= 2);
}

#[test]
fn duplicate_keys_and_order_are_preserved() {
    use vole_document::adapter::apispec::{member_key_text, parse};

    let dup = br#"{"$schema":"https://json-schema.org/draft-07/schema#","title":"a","title":"b"}"#;
    let m = parse(dup, Limits::DEFAULT, true).unwrap();
    let titles: Vec<String> = m
        .members
        .iter()
        .filter(|mm| member_key_text(&m, dup, mm).unwrap() == "title")
        .map(|mm| {
            String::from_utf8_lossy(&dup[mm.value_start as usize..mm.value_end as usize])
                .into_owned()
        })
        .collect();
    assert_eq!(titles, vec!["\"a\"", "\"b\""]);
}

#[test]
fn openapi_paths_operations_components_are_recorded() {
    use vole_document::adapter::apispec::{parse, role_name, version_text};

    let m = parse(OPENAPI, Limits::DEFAULT, true).unwrap();
    assert_eq!(m.dialect_name(), "openapi");
    assert_eq!(version_text(&m, OPENAPI).unwrap(), "3.1.0");
    for role in [
        "path-item",
        "operation",
        "components",
        "schemas",
        "servers",
        "security",
        "parameters",
        "responses",
    ] {
        assert!(
            m.objects.iter().any(|o| role_name(o.role) == role),
            "missing role {role}"
        );
    }
    assert_eq!(m.refs.len(), 1);
}

#[test]
fn swagger_and_asyncapi_dialects_are_recorded() {
    use vole_document::adapter::apispec::{parse, role_name, version_text};

    let m = parse(SWAGGER, Limits::DEFAULT, true).unwrap();
    assert_eq!(m.dialect_name(), "swagger");
    assert_eq!(version_text(&m, SWAGGER).unwrap(), "2.0");
    for role in ["definitions", "parameters", "responses", "operation"] {
        assert!(
            m.objects.iter().any(|o| role_name(o.role) == role),
            "missing {role}"
        );
    }
    // `basePath` is a scalar member: its exact key spelling is preserved.
    assert!(m.members.iter().any(|mm| mm.key_start < mm.key_end
        && &SWAGGER[mm.key_start as usize..mm.key_end as usize] == b"\"basePath\""));

    let m = parse(ASYNCAPI, Limits::DEFAULT, true).unwrap();
    assert_eq!(m.dialect_name(), "asyncapi");
    for role in [
        "channels",
        "channel-item",
        "operations",
        "servers",
        "components",
    ] {
        assert!(
            m.objects.iter().any(|o| role_name(o.role) == role),
            "missing {role}"
        );
    }
    assert_eq!(m.refs.len(), 1);
}

#[test]
fn native_and_common_selectors_resolve() {
    let mut fx = Fixture::new("selectors-openapi", OPENAPI);
    let field = fx.report.field;

    // `apispec-dialect` (text) and metadata.
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::ApispecDialect,
            Representation::Text,
        )),
        "openapi"
    );
    let md = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::ApispecDialect,
        Representation::Metadata,
    ));
    assert!(md.contains("\"format\":\"apispec\""), "{md}");
    assert!(md.contains("\"dialect\":\"openapi\""), "{md}");
    assert!(md.contains("\"version\":\"3.1.0\""), "{md}");

    // `apispec-version` exact bytes keep the quotes.
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            Selector::ApispecVersion,
            Representation::ExactBytes,
        )),
        b"\"3.1.0\""
    );

    // `apispec-ref 0` returns the exact target bytes (never resolved).
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            ref_(0),
            Representation::ExactBytes,
        )),
        b"\"#/components/schemas/Pet\""
    );
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            ref_(0),
            Representation::Text,
        )),
        "#/components/schemas/Pet"
    );

    // `apispec-object 0` is the root.
    let md = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        object(0),
        Representation::Metadata,
    ));
    assert!(md.contains("\"role\":\"root\""), "{md}");

    // `apispec-find` reports matches.
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::ApispecFind {
            pattern: "listPets".to_string(),
        },
        Representation::Text,
    ));
    assert!(found.contains("\"matches\":["), "{found}");
    assert!(found.contains("listPets"), "{found}");

    // Common metadata / text / search-match.
    let meta = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(meta.contains("\"format\":\"apispec\""), "{meta}");
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert!(text.contains("listPets"), "{text}");
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("limit".to_string()),
        Representation::Text,
    ));
    assert!(found.contains("limit"), "{found}");

    // Swagger native selectors.
    let mut sx = Fixture::new("selectors-swagger", SWAGGER);
    let sf = sx.report.field;
    assert_eq!(
        answer_text(&observe_ok(
            &mut sx.store,
            &sf,
            Selector::ApispecVersion,
            Representation::Text,
        )),
        "2.0"
    );
}

#[test]
fn declines_are_typed_never_silent() {
    let mut fx = Fixture::new("declines", OPENAPI);
    let f = fx.report.field;

    assert_eq!(
        observe_err(&mut fx.store, &f, object(9999), Representation::Metadata),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(&mut fx.store, &f, ref_(9999), Representation::Metadata),
        ErrorClass::UnsupportedFeature
    );
    // An unsupported common pair declines typed.
    assert_eq!(
        observe_err(&mut fx.store, &f, Selector::Table(0), Representation::Text),
        ErrorClass::UnsupportedFeature
    );
}

#[test]
fn malformed_and_non_spec_decline_typed() {
    use vole_document::adapter::apispec::{detect, parse};

    assert!(!detect(br#"{"a":1,"b":2}"#, Limits::DEFAULT));
    assert_eq!(
        parse(br#"{"a":1,"b":2}"#, Limits::DEFAULT, true)
            .unwrap_err()
            .class(),
        ErrorClass::InvalidApispecStructure
    );
    assert_eq!(
        parse(b"[1,2,3]", Limits::DEFAULT, true)
            .unwrap_err()
            .class(),
        ErrorClass::InvalidApispecStructure
    );
    assert_eq!(
        parse(b"{", Limits::DEFAULT, true).unwrap_err().class(),
        ErrorClass::InvalidApispecStructure
    );
    // A schema-shaped object with no marker is not claimed.
    assert!(!detect(
        br#"{"type":"object","properties":{}}"#,
        Limits::DEFAULT
    ));
}

#[test]
fn exact_materialization_is_byte_identical() {
    for src in [SCHEMA, OPENAPI, SWAGGER, ASYNCAPI] {
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
    let src_path = root.join("openapi.json");
    let desc_path = root.join("doc.voldoc");
    std::fs::write(&src_path, OPENAPI).unwrap();
    let descriptor = opaque_descriptor(OPENAPI);
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
        ref_(0),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"\"#/components/schemas/Pet\"");

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), OPENAPI.len());
    assert_eq!(sha256(&exact), sha256(OPENAPI));
    assert_eq!(exact, OPENAPI);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn apispec_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived API-spec model reads the source bytes, so its single
    // dependency must be the exact `DocumentExact` root (keyed by `sha256(source)`).
    let fx = Fixture::new("adr0060", OPENAPI);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let manifest = field.manifest().clone();
    let root = manifest.root_node;
    let root_node = load_node(fx.store.seeds(), &root).unwrap();
    assert_eq!(root_node.kind, NodeKind::DocumentExact);

    assert!(manifest.has_index());
    let istore = FsIndexStore::open(fx.store.root()).unwrap();
    let iroot = NodeId::from_bytes(manifest.index_root);
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_APISPEC_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::ApispecModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the API-spec model must depend on exactly the exact root"
    );
}

#[test]
fn caps_decline_typed() {
    use vole_document::adapter::apispec::{detect, parse};

    let tight = Limits {
        max_apispec_objects: 1,
        ..Limits::DEFAULT
    };
    assert!(!detect(OPENAPI, tight));
    assert_eq!(
        parse(OPENAPI, tight, true).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );
    let tight = Limits {
        max_apispec_members: 1,
        ..Limits::DEFAULT
    };
    assert_eq!(
        parse(OPENAPI, tight, true).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );
    let tight = Limits {
        max_apispec_depth: 1,
        ..Limits::DEFAULT
    };
    assert_eq!(
        parse(OPENAPI, tight, true).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );
    let tight = Limits {
        max_apispec_nodes: 1,
        ..Limits::DEFAULT
    };
    assert_eq!(
        parse(OPENAPI, tight, true).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );

    // The opaque floor still closes the source exactly.
    let fx = Fixture::new("caps", OPENAPI);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn random_bytes_never_panic() {
    use vole_document::adapter::apispec::{
        ApispecModel, build_apispec_model, canonical_text, detect, find, parse, version_text,
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
            let _ = version_text(&m, &buf);
        }
        let _ = build_apispec_model(&buf, Limits::STRICT);
        let _ = ApispecModel::decode(&buf);
    }
}
