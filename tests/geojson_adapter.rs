//! Phase 21.22 court: the GeoJSON (RFC 7946) adapter.
//!
//! Gated on `field` + `geojson` (which implies `json`), so a build without the
//! feature compiles an empty target.
//!
//! GeoJSON's physical bytes are JSON, so its representation-preserving parser is the
//! **shared** JSON adapter (never a second JSON parser). The distinct claim is a
//! **bounded semantic sub-detection** run before the generic JSON detector. The court
//! checks:
//!
//! 1. **Byte-based detection and the boundaries**: a `FeatureCollection`/geometry →
//!    `Geojson`; a plain JSON document and a JSON document whose `"type"` is an
//!    unrelated string (or a GeoJSON type name with no consistent shape) stay `Json`;
//!    prose stays `Opaque`.
//! 2. **Representation preservation**: the exact `"type"` token; `coordinates`
//!    nesting with each number's exact span/literal spelling; `properties` order and
//!    duplicate keys; `id`/`bbox`/`geometry`/`features` order; and foreign members
//!    (reported, never dropped).
//! 3. **Selectors**: native `geojson-type`/`geojson-feature`/`geojson-geometry`/
//!    `geojson-coordinates`/`geojson-property`/`geojson-find` plus common `metadata`/
//!    `text`/`search-match`.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 5. **Fail closed**: malformed declines typed; random bytes never panic.
//! 6. **No cross-field aliasing** (ADR-0060): the model node depends on the exact
//!    `sha256(source)` root.

#![cfg(all(feature = "field", feature = "geojson"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_GEOJSON_MODEL, SelectorKey, lookup};
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
        "vole-geojson-{label}-{}-{}",
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
        format_basis: "opaque:geojson-test".to_string(),
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

/// A `FeatureCollection` with a `bbox`, two features (a `Point` with a duplicated
/// `properties` key and a foreign `vendor` member, and a `Polygon` with `null`
/// properties), and a foreign root member `title`.
const FC: &[u8] = br#"{
  "type": "FeatureCollection",
  "bbox": [100.0, 0.0, 105.0, 1.0],
  "features": [
    {"type": "Feature", "id": 1,
     "geometry": {"type": "Point", "coordinates": [1.25, -2.5e1]},
     "properties": {"name": "A", "name": "B", "n": 1e3},
     "vendor": "x"},
    {"type": "Feature", "id": 2,
     "geometry": {"type": "Polygon", "coordinates": [[[0, 0], [1, 0], [1, 1], [0, 0]]]},
     "properties": null}
  ],
  "title": "kept"
}"#;

/// A bare `Point` geometry.
const POINT: &[u8] = br#"{"type":"Point","coordinates":[30.0,10.0]}"#;
/// A bare `LineString` geometry with preserved numeric spelling.
const LINE: &[u8] = br#"{"type":"LineString","coordinates":[[30.0,10.0],[10.0,30.0],[40.0,40.0]]}"#;
/// A `GeometryCollection`.
const GC: &[u8] = br#"{"type":"GeometryCollection","geometries":[{"type":"Point","coordinates":[0,0]},{"type":"LineString","coordinates":[[0,0],[1,1]]}]}"#;

/// Plain JSON: not GeoJSON (stays `Json`).
const PLAIN_JSON: &[u8] = br#"{"a":1,"b":[2,3]}"#;
/// A JSON document with an unrelated `"type"` string (stays `Json`).
const UNRELATED_TYPE: &[u8] = br#"{"type":"object","properties":{}}"#;
/// A GeoJSON type name with no consistent shape (stays `Json`).
const SHAPELESS_TYPE: &[u8] = br#"{"type":"Point"}"#;
/// Plain prose: never GeoJSON (stays `Opaque`).
const PROSE: &[u8] = b"The quick brown fox jumps over the lazy dog.\nPlain prose, not GeoJSON.\n";

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_byte_based_and_conservative() {
    for (bytes, want) in [
        (FC, DocumentFormat::Geojson),
        (POINT, DocumentFormat::Geojson),
        (LINE, DocumentFormat::Geojson),
        (GC, DocumentFormat::Geojson),
        // The documented boundaries: plain JSON and a JSON doc with an unrelated
        // (or shape-inconsistent) `"type"` stay `Json`; prose stays `Opaque`.
        (PLAIN_JSON, DocumentFormat::Json),
        (UNRELATED_TYPE, DocumentFormat::Json),
        (SHAPELESS_TYPE, DocumentFormat::Json),
        (PROSE, DocumentFormat::Opaque),
    ] {
        assert_eq!(
            detect_document_format(bytes, Limits::DEFAULT),
            want,
            "detection boundary: {:?}",
            String::from_utf8_lossy(bytes)
        );
    }

    let caps = capabilities_for_format(DocumentFormat::Geojson);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for want in ["metadata", "text", "search-match"] {
        assert!(names.contains(&want), "missing common selector {want}");
    }
    let native = &caps.native_selectors;
    for want in [
        "geojson-type",
        "geojson-feature",
        "geojson-geometry",
        "geojson-coordinates",
        "geojson-property",
        "geojson-find",
    ] {
        assert!(native.contains(&want), "missing native selector {want}");
    }
}

#[test]
fn geometry_types_preserve_spelling_order_and_foreign_members() {
    use vole_document::adapter::geojson;

    // Every geometry type round-trips through the model with its exact type token.
    for (src, type_name) in [
        (POINT, "Point"),
        (LINE, "LineString"),
        (GC, "GeometryCollection"),
    ] {
        let m = geojson::parse(src, Limits::DEFAULT, true).unwrap();
        let tn = m.node(m.type_node).unwrap();
        assert_eq!(
            geojson::token_bytes(src, tn).unwrap(),
            format!("\"{type_name}\"").as_bytes()
        );
        assert_eq!(
            geojson::type_of_object(&m, src, m.root).unwrap().unwrap(),
            type_name
        );
    }

    let m = geojson::parse(FC, Limits::DEFAULT, true).unwrap();
    assert_eq!(m.class, geojson::G_FEATURE_COLLECTION);
    assert_eq!(m.feature_count(), 2);
    assert_eq!(m.geometry_count(), 2);

    // The Point's coordinates keep numeric spelling verbatim (never reparsed).
    let point = m.geometries[0];
    let coords = geojson::coordinates_node(&m, FC, point).unwrap().unwrap();
    let nums = geojson::coordinate_numbers(&m, coords, Limits::DEFAULT).unwrap();
    assert_eq!(nums.len(), 2);
    assert_eq!(
        geojson::token_bytes(FC, m.node(nums[0]).unwrap()).unwrap(),
        b"1.25"
    );
    assert_eq!(
        geojson::token_bytes(FC, m.node(nums[1]).unwrap()).unwrap(),
        b"-2.5e1"
    );

    // properties order and duplicate keys are preserved by the JSON model.
    let props = geojson::object_member(&m, FC, m.features[0], "properties")
        .unwrap()
        .unwrap();
    let keys: Vec<String> = geojson::object_members(&m, FC, props)
        .unwrap()
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    assert_eq!(keys, vec!["name", "name", "n"]);

    // Foreign members are reported, never dropped.
    let root_foreign = geojson::foreign_members(&m, FC, m.root).unwrap();
    assert_eq!(root_foreign.len(), 1);
    assert_eq!(root_foreign[0].0, "title");
    let feat_foreign = geojson::foreign_members(&m, FC, m.features[0]).unwrap();
    assert_eq!(feat_foreign.len(), 1);
    assert_eq!(feat_foreign[0].0, "vendor");

    // features are in document order.
    let f0 = m.node(m.features[0]).unwrap();
    let f1 = m.node(m.features[1]).unwrap();
    assert!(f0.start < f1.start);
}

#[test]
fn native_and_common_selectors_resolve() {
    let mut fx = Fixture::new("selectors", FC);
    let field = fx.report.field;
    assert_eq!(fx.report.format, DocumentFormat::Geojson);

    // Native: the exact `"type"` token and decoded text.
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::GeojsonType,
            Representation::Text
        )),
        "FeatureCollection"
    );
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            Selector::GeojsonType,
            Representation::ExactBytes
        )),
        b"\"FeatureCollection\""
    );

    // Native: a feature descriptor reports its type, geometry, id, and foreign member.
    let feat = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::GeojsonFeature { index: 0 },
        Representation::Metadata,
    ));
    assert!(feat.contains("\"type\":\"Feature\""), "{feat}");
    assert!(feat.contains("\"vendor\""), "{feat}");
    assert!(feat.contains("\"type\":\"Point\""), "{feat}");

    // Native: a geometry descriptor reports its type and coordinate count.
    let geom = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::GeojsonGeometry { index: 1 },
        Representation::Metadata,
    ));
    assert!(geom.contains("\"type\":\"Polygon\""), "{geom}");

    // Native: coordinates expose each number's span and literal spelling.
    let coords = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::GeojsonCoordinates { index: 0 },
        Representation::Metadata,
    ));
    assert!(coords.contains("\"count\":2"), "{coords}");
    assert!(coords.contains("\"spelling\":\"1.25\""), "{coords}");
    assert!(coords.contains("\"spelling\":\"-2.5e1\""), "{coords}");

    // Native: a property by name (Text decodes; ExactBytes is the token).
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::GeojsonProperty {
                feature: 0,
                name: "name".to_string()
            },
            Representation::Text
        )),
        "A"
    );

    // Native: geojson-find reuses the JSON match vocabulary (duplicate keys reported).
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::GeojsonFind {
            pattern: "name".to_string(),
        },
        Representation::Text,
    ));
    assert!(found.contains("\"role\":\"key\""), "{found}");

    // Common: metadata / text / search-match.
    let meta = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(meta.contains("\"format\":\"geojson\""), "{meta}");
    assert!(meta.contains("\"type\":\"FeatureCollection\""), "{meta}");
    assert!(meta.contains("\"features\":2"), "{meta}");
    assert!(meta.contains("\"geometries\":2"), "{meta}");
    let _ = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    let _ = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("Feature".to_string()),
        Representation::Text,
    ));
}

#[test]
fn exact_materialization_is_byte_identical() {
    for src in [FC, POINT, LINE, GC, PLAIN_JSON, UNRELATED_TYPE, PROSE] {
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
    let src_path = root.join("doc.geojson");
    let desc_path = root.join("doc.voldoc");
    std::fs::write(&src_path, FC).unwrap();
    std::fs::write(&desc_path, opaque_descriptor(FC)).unwrap();

    let store_root = root.join("store");
    let field_id = {
        let mut store = FieldStore::open(&store_root).unwrap();
        let report = ingest_pdf(&mut store, &opaque_descriptor(FC), Limits::DEFAULT).unwrap();
        report.field
        // store drops here: a fresh process reopens the directory.
    };

    std::fs::remove_file(&src_path).unwrap();
    std::fs::remove_file(&desc_path).unwrap();

    let mut store2 = FieldStore::open(&store_root).unwrap();
    let bytes = answer_bytes(&observe_ok(
        &mut store2,
        &field_id,
        Selector::GeojsonProperty {
            feature: 0,
            name: "n".to_string(),
        },
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"1e3");

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), FC.len());
    assert_eq!(sha256(&exact), sha256(FC));
    assert_eq!(exact, FC);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn malformed_inputs_decline_typed() {
    use vole_document::adapter::geojson;
    // Valid JSON, but not a structurally-consistent GeoJSON document.
    for src in [
        &b"{\"type\":\"object\"}"[..],
        &b"{\"type\":\"Point\"}"[..],
        &b"{\"type\":\"FeatureCollection\"}"[..],
        &b"[1,2,3]"[..],
    ] {
        let e = geojson::parse(src, Limits::DEFAULT, true).unwrap_err();
        assert_eq!(
            e.class(),
            ErrorClass::InvalidGeojsonStructure,
            "src={src:?}"
        );
    }
    // Malformed JSON is a typed decline too (never a panic).
    let e = geojson::parse(b"{", Limits::DEFAULT, true).unwrap_err();
    assert_eq!(e.class(), ErrorClass::InvalidGeojsonStructure);

    // Over the feature cap declines typed (resource limit, not invalid).
    let mut big = Vec::new();
    big.extend_from_slice(b"{\"type\":\"FeatureCollection\",\"features\":[");
    for i in 0..5000 {
        if i > 0 {
            big.push(b',');
        }
        big.extend_from_slice(b"{\"type\":\"Feature\",\"geometry\":null,\"properties\":null}");
    }
    big.extend_from_slice(b"]}");
    let e = geojson::parse(&big, Limits::STRICT, true).unwrap_err();
    assert_eq!(e.class(), ErrorClass::ResourceLimit);
    assert_ne!(
        detect_document_format(&big, Limits::STRICT),
        DocumentFormat::Geojson
    );
    // But it is still valid JSON, so it stays `Json`.
    assert_eq!(
        detect_document_format(&big, Limits::STRICT),
        DocumentFormat::Json
    );
}

#[test]
fn interleaved_fixtures_do_not_alias() {
    // ADR-0060: a source-reading node must carry its own source-identity input.
    let root = temp_dir("alias");
    let mut store = FieldStore::open(&root).unwrap();

    let fa = ingest_pdf(&mut store, &opaque_descriptor(POINT), Limits::DEFAULT)
        .unwrap()
        .field;
    let fb = ingest_pdf(&mut store, &opaque_descriptor(LINE), Limits::DEFAULT)
        .unwrap()
        .field;

    for (field, src, want) in [
        (fa, POINT, "Point".to_string()),
        (fb, LINE, "LineString".to_string()),
    ] {
        let got = answer_text(&observe_ok(
            &mut store,
            &field,
            Selector::GeojsonType,
            Representation::Text,
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
fn geojson_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived GeoJSON model reads the source bytes, so its single
    // dependency must be the exact `DocumentExact` root (keyed by `sha256(source)`).
    // This is what prevents cross-field source aliasing.
    let fx = Fixture::new("adr0060", FC);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let manifest = field.manifest().clone();
    let root = manifest.root_node;
    let root_node = load_node(fx.store.seeds(), &root).unwrap();
    assert_eq!(root_node.kind, NodeKind::DocumentExact);

    assert!(manifest.has_index());
    let istore = FsIndexStore::open(fx.store.root()).unwrap();
    let iroot = NodeId::from_bytes(manifest.index_root);
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_GEOJSON_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::GeojsonModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the GeoJSON model must depend on exactly the exact root"
    );
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0x6E01_E05E_2022_2200;
    for _ in 0..128 {
        let mut buf = vec![0u8; 512];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        if let Ok(m) = vole_document::adapter::geojson::parse(&buf, Limits::STRICT, true) {
            let _ = vole_document::adapter::geojson::build_geojson_model(&buf, Limits::STRICT);
            let _ = vole_document::adapter::geojson::canonical_text(&m, &buf);
        }
    }
    // JSON/GeoJSON-shaped hostile inputs must also never panic.
    for src in [
        b"{\"type\":\"".as_slice(),
        b"{\"type\":\"FeatureCollection\",\"features\":[".as_slice(),
        b"{\"type\":\"GeometryCollection\",\"geometries\":[{\"type\":\"".as_slice(),
    ] {
        let _ = detect_document_format(src, Limits::STRICT);
        let _ = vole_document::adapter::geojson::parse(src, Limits::STRICT, true);
    }
}
