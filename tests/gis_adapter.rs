//! Phase 21.23 court: the KML / GPX geospatial adapter.
//!
//! Gated on `field` + `gis` (which implies `xml`), so a build without the feature
//! compiles an empty target.
//!
//! KML and GPX are XML, so their physical bytes are shared with the standalone XML
//! adapter. The distinct claim is a **bounded semantic sub-detection** run before the
//! generic XML detector, plus one bounded adapter covering both dialects with the
//! dialect recorded. The court checks:
//!
//! 1. **Byte-based detection and the boundaries**: a KML `<kml
//!    xmlns="…/kml/2.2">` with a `Document` child → `Gis`; a GPX `<gpx
//!    xmlns="…/GPX/1/1">` with a `wpt`/`trk` child → `Gis`; a plain XML document
//!    stays `Xml`; a shaped-but-invalid `<kml>`/`<gpx>` (wrong/no namespace, or no
//!    structural child) stays `Xml`; prose stays `Opaque`.
//! 2. **Representation preservation**: the recorded dialect; exact element/attribute
//!    spans; element order; attribute spelling (KML `<coordinates>`, GPX
//!    `lat`/`lon`); and the namespace declaration.
//! 3. **Selectors**: native `gis-root`/`gis-field`/`gis-record`/`gis-record-field`/
//!    `gis-point`/`gis-find` plus common `metadata`/`text`/`search-match`.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 5. **Fail closed**: malformed declines typed; random bytes never panic.
//! 6. **No cross-field aliasing** (ADR-0060): the model node depends on the exact
//!    `sha256(source)` root.

#![cfg(all(feature = "field", feature = "gis"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_GIS_MODEL, SelectorKey, lookup};
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
        "vole-gis-{label}-{}-{}",
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
        format_basis: "opaque:gis-test".to_string(),
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

/// An OGC KML 2.2 document: the KML namespace, a `<Document>` with a `<name>`, two
/// `<Placemark>` features (a `<Point>` and a `<LineString>`, each with a
/// `<coordinates>`), and a `<styleUrl>`.
const KML: &[u8] = b"<?xml version=\"1.0\"?>\n\
<kml xmlns=\"http://www.opengis.net/kml/2.2\">\n<name>RootDoc</name>\n<Document>\n\
<name>Example</name>\n\
<Placemark>\n<name>First</name>\n<description>one</description>\n\
<styleUrl>#s1</styleUrl>\n<Point><coordinates>1.25,-2.5e1,0</coordinates></Point>\n\
</Placemark>\n<Placemark>\n<name>Second</name>\n\
<LineString><coordinates>0,0 1,1</coordinates></LineString>\n</Placemark>\n\
</Document>\n</kml>\n";

/// A GPX 1.1 document: the GPX namespace, `<metadata>`, a `<wpt>`, a `<rte>` with a
/// `<rtept>`, and a `<trk>`/`<trkseg>`/`<trkpt>` (with `lat`/`lon` attributes and
/// `<ele>`/`<time>` children).
const GPX: &[u8] = b"<?xml version=\"1.0\"?>\n\
<gpx xmlns=\"http://www.topografix.com/GPX/1/1\" version=\"1.1\" creator=\"test\">\n\
<metadata><name>Example</name></metadata>\n\
<wpt lat=\"1.0\" lon=\"2.0\"><ele>10</ele><time>2024-01-01T00:00:00Z</time><name>W</name></wpt>\n\
<rte><name>R</name><rtept lat=\"3.0\" lon=\"4.0\"><ele>20</ele></rtept></rte>\n\
<trk><name>T</name><trkseg><trkpt lat=\"5.0\" lon=\"6.0\"><ele>30</ele>\
<time>2024-01-02T00:00:00Z</time></trkpt></trkseg></trk>\n</gpx>\n";

/// A plain XML document: not geospatial (stays `Xml`).
const PLAIN_XML: &[u8] = b"<note><to>Tove</to><from>Jani</from></note>";
/// A `<kml>` root with no KML namespace: not geospatial (stays `Xml`).
const NO_NS_KML: &[u8] = b"<kml><Document><Placemark/></Document></kml>";
/// A `<kml>` root in the KML namespace with no structural child (stays `Xml`).
const NO_CHILD_KML: &[u8] = b"<kml xmlns=\"http://www.opengis.net/kml/2.2\"/>";
/// A `<gpx>` root with no GPX namespace: not geospatial (stays `Xml`).
const NO_NS_GPX: &[u8] = b"<gpx><wpt lat=\"0\" lon=\"0\"/></gpx>";
/// A `<gpx>` root in the GPX namespace with no structural child (stays `Xml`).
const NO_CHILD_GPX: &[u8] = b"<gpx xmlns=\"http://www.topografix.com/GPX/1/1\"/>";
/// Plain prose: never geospatial (stays `Opaque`).
const PROSE: &[u8] = b"The quick brown fox jumps over the lazy dog.\nPlain prose, not GIS.\n";

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_byte_based_and_conservative() {
    for (bytes, want) in [
        (KML, DocumentFormat::Gis),
        (GPX, DocumentFormat::Gis),
        // The documented boundaries: plain XML / shaped-but-invalid stay `Xml`;
        // prose stays `Opaque`. (The `xml` feature is implied by `gis`.)
        (PLAIN_XML, DocumentFormat::Xml),
        (NO_NS_KML, DocumentFormat::Xml),
        (NO_CHILD_KML, DocumentFormat::Xml),
        (NO_NS_GPX, DocumentFormat::Xml),
        (NO_CHILD_GPX, DocumentFormat::Xml),
        (PROSE, DocumentFormat::Opaque),
    ] {
        assert_eq!(
            detect_document_format(bytes, Limits::DEFAULT),
            want,
            "{bytes:?}"
        );
    }

    let caps = capabilities_for_format(DocumentFormat::Gis);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for n in [
        "gis-root",
        "gis-field",
        "gis-record",
        "gis-record-field",
        "gis-point",
        "gis-find",
    ] {
        assert!(caps.native_selectors.contains(&n), "missing {n}");
    }
    assert!(caps.to_json().contains("\"format\":\"gis\""));
    // The fresh exit code is stable and distinct.
    assert_eq!(ErrorClass::InvalidGisStructure.exit_code(), 37);
}

#[test]
fn kml_preserves_spans_order_and_geometry() {
    use vole_document::adapter::gis;

    let m = gis::parse(KML, Limits::DEFAULT, true).unwrap();
    assert_eq!(m.dialect, gis::DIALECT_KML);
    assert_eq!(m.record_count(), 2);
    assert_eq!(m.point_count(), 2);
    // Records and points are in document order.
    assert!(m.records[0] < m.records[1]);
    assert!(m.points[0] < m.points[1]);

    // The first placemark's name keeps its exact whole-element span and text.
    let fields = gis::record_field_nodes(&m, KML, 0).unwrap();
    let name = fields[0];
    assert_eq!(gis::field_name(&m, KML, name).unwrap(), "name");
    assert_eq!(
        gis::field_bytes(&m, KML, name).unwrap(),
        b"<name>First</name>"
    );
    assert_eq!(gis::field_text(&m, KML, name).unwrap(), "First");

    // The geometry keeps its `coordinates` text verbatim (never reparsed).
    let point = m.points[0];
    assert_eq!(gis::field_name(&m, KML, point).unwrap(), "Point");
    let coords = gis::point_field_nodes(&m, KML, 0).unwrap();
    assert_eq!(gis::field_name(&m, KML, coords[0]).unwrap(), "coordinates");
    assert_eq!(
        gis::field_text(&m, KML, coords[0]).unwrap(),
        "1.25,-2.5e1,0"
    );

    // The namespace declaration is preserved as an attribute on the root.
    let root = m.node(m.root).unwrap();
    let ns = m.xml.attr(root.attrs[0]).unwrap();
    assert_eq!(ns.is_ns, 1);
    assert_eq!(
        vole_document::adapter::xml::attr_value_bytes(KML, ns).unwrap(),
        gis::KML_NS.as_bytes()
    );
}

#[test]
fn gpx_preserves_lat_lon_attributes_and_ele_time() {
    use vole_document::adapter::gis;

    let m = gis::parse(GPX, Limits::DEFAULT, true).unwrap();
    assert_eq!(m.dialect, gis::DIALECT_GPX);
    // Records: one wpt, one rte, one trk.
    assert_eq!(m.record_count(), 3);
    // Points: wpt + rtept + trkpt = 3, in document order.
    assert_eq!(m.point_count(), 3);
    assert!(m.points[0] < m.points[1] && m.points[1] < m.points[2]);

    // The trkpt `lat`/`lon` spellings survive as attributes, in source order.
    let trkpt = m.points[2];
    assert_eq!(gis::field_name(&m, GPX, trkpt).unwrap(), "trkpt");
    assert_eq!(
        gis::attr_value_by_local(&m, GPX, trkpt, "lat")
            .unwrap()
            .unwrap(),
        "5.0"
    );
    assert_eq!(
        gis::attr_value_by_local(&m, GPX, trkpt, "lon")
            .unwrap()
            .unwrap(),
        "6.0"
    );
    let attrs = gis::field_attrs(&m, GPX, trkpt).unwrap();
    assert_eq!(attrs.len(), 2);
    assert_eq!(attrs[0].name, "lat");
    assert_eq!(attrs[0].value, "5.0");
    assert_eq!(attrs[1].name, "lon");
    assert_eq!(attrs[1].value, "6.0");

    // The trkpt `ele`/`time` are child elements with exact text.
    let fields = gis::point_field_nodes(&m, GPX, 2).unwrap();
    assert_eq!(fields.len(), 2);
    assert_eq!(gis::field_text(&m, GPX, fields[0]).unwrap(), "30");
    assert_eq!(
        gis::field_text(&m, GPX, fields[1]).unwrap(),
        "2024-01-02T00:00:00Z"
    );

    // The namespace declaration is preserved.
    let root = m.node(m.root).unwrap();
    let ns = m.xml.attr(root.attrs[0]).unwrap();
    assert_eq!(ns.is_ns, 1);
    assert_eq!(
        vole_document::adapter::xml::attr_value_bytes(GPX, ns).unwrap(),
        gis::GPX_NS.as_bytes()
    );
}

#[test]
fn native_and_common_selectors_resolve() {
    let mut fx = Fixture::new("selectors", KML);
    let field = fx.report.field;
    assert_eq!(fx.report.format, DocumentFormat::Gis);

    // `gis-root`: the root container descriptor (dialect + counts).
    let root = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::GisRoot,
        Representation::Structure,
    ));
    assert!(root.contains("\"dialect\":\"kml\""), "{root}");
    assert!(root.contains("\"records\":2"), "{root}");
    assert!(root.contains("\"points\":2"), "{root}");

    // `gis-field`: a root-level scalar metadata field.
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::GisField {
                name: "name".to_string()
            },
            Representation::Text
        )),
        "RootDoc"
    );

    // `gis-record`: one record (a Placemark).
    let rec = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::GisRecord { index: 0 },
        Representation::Structure,
    ));
    assert!(rec.contains("\"kind\":\"Placemark\""), "{rec}");
    assert!(rec.contains("\"index\":0"), "{rec}");
    assert!(rec.contains("First"), "{rec}");

    // `gis-record-field`: a field of a record; ExactBytes is the whole element span.
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::GisRecordField {
                record: 0,
                name: "name".to_string()
            },
            Representation::Text
        )),
        "First"
    );
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            Selector::GisRecordField {
                record: 0,
                name: "description".to_string()
            },
            Representation::ExactBytes
        )),
        b"<description>one</description>"
    );

    // `gis-point`: a point (KML geometry) descriptor reports its kind and coordinates.
    let pt = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::GisPoint { index: 0 },
        Representation::Structure,
    ));
    assert!(pt.contains("\"kind\":\"Point\""), "{pt}");
    assert!(pt.contains("1.25,-2.5e1,0"), "{pt}");

    // `gis-find`: a lexical search over recognized field values.
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::GisFind {
            pattern: "First".to_string(),
        },
        Representation::Text,
    ));
    assert!(found.contains("\"kind\":\"record\""), "{found}");
    assert!(found.contains("\"name\":\"name\""), "{found}");

    // Common metadata / text / search-match.
    let meta = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(meta.contains("\"format\":\"gis\""), "{meta}");
    assert!(meta.contains("\"dialect\":\"kml\""), "{meta}");
    assert!(meta.contains("\"records\":2"), "{meta}");
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert!(text.contains("First"), "{text}");
    assert!(text.contains("Second"), "{text}");
    let sm = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("First".to_string()),
        Representation::Text,
    ));
    assert!(sm.contains("\"kind\":\"record\""), "{sm}");

    // An unknown field, an out-of-range record/point, and an unsupported common pair
    // decline typed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::GisField {
                name: "nope".to_string()
            },
            Representation::Text
        ),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::GisRecord { index: 99 },
            Representation::Text
        ),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::GisPoint { index: 99 },
            Representation::Text
        ),
        ErrorClass::UnsupportedFeature
    );
    // A Placemark has no direct `coordinates` field (it lives on the geometry).
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::GisRecordField {
                record: 0,
                name: "coordinates".to_string()
            },
            Representation::Text
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
fn gpx_native_point_exposes_lat_lon() {
    let mut fx = Fixture::new("gpx-selectors", GPX);
    let field = fx.report.field;
    assert_eq!(fx.report.format, DocumentFormat::Gis);

    let pt = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::GisPoint { index: 2 },
        Representation::Structure,
    ));
    assert!(pt.contains("\"kind\":\"trkpt\""), "{pt}");
    assert!(pt.contains("\"lat\":\"5.0\""), "{pt}");
    assert!(pt.contains("\"lon\":\"6.0\""), "{pt}");
    assert!(pt.contains("30"), "{pt}");

    // The wpt is both a record and a point.
    let pt0 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::GisPoint { index: 0 },
        Representation::Structure,
    ));
    assert!(pt0.contains("\"kind\":\"wpt\""), "{pt0}");
    assert!(pt0.contains("\"lat\":\"1.0\""), "{pt0}");
}

#[test]
fn exact_materialization_is_byte_identical() {
    for src in [KML, GPX, PLAIN_XML, NO_CHILD_KML, PROSE] {
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
    let src_path = root.join("doc.gpx");
    let desc_path = root.join("doc.voldoc");
    std::fs::write(&src_path, GPX).unwrap();
    std::fs::write(&desc_path, opaque_descriptor(GPX)).unwrap();

    let store_root = root.join("store");
    let field_id = {
        let mut store = FieldStore::open(&store_root).unwrap();
        let report = ingest_pdf(&mut store, &opaque_descriptor(GPX), Limits::DEFAULT).unwrap();
        report.field
        // store drops here: a fresh process reopens the directory.
    };

    std::fs::remove_file(&src_path).unwrap();
    std::fs::remove_file(&desc_path).unwrap();

    let mut store2 = FieldStore::open(&store_root).unwrap();
    let bytes = answer_bytes(&observe_ok(
        &mut store2,
        &field_id,
        Selector::GisRecordField {
            record: 0,
            name: "name".to_string(),
        },
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"<name>W</name>");

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), GPX.len());
    assert_eq!(sha256(&exact), sha256(GPX));
    assert_eq!(exact, GPX);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn malformed_inputs_decline_typed() {
    // Not well-formed XML.
    let e =
        vole_document::adapter::gis::parse(b"<kml><Document>", Limits::DEFAULT, true).unwrap_err();
    assert_eq!(e.class(), ErrorClass::InvalidGisStructure);
    // Well-formed XML, but not geospatial.
    let e = vole_document::adapter::gis::parse(b"<a><b/></a>", Limits::DEFAULT, true).unwrap_err();
    assert_eq!(e.class(), ErrorClass::InvalidGisStructure);
    // A KML root in the KML namespace with no structural child declines.
    let e = vole_document::adapter::gis::parse(
        b"<kml xmlns=\"http://www.opengis.net/kml/2.2\"/>",
        Limits::DEFAULT,
        true,
    )
    .unwrap_err();
    assert_eq!(e.class(), ErrorClass::InvalidGisStructure);
    // A GPX root in the GPX namespace with no structural child declines.
    let e = vole_document::adapter::gis::parse(
        b"<gpx xmlns=\"http://www.topografix.com/GPX/1/1\"/>",
        Limits::DEFAULT,
        true,
    )
    .unwrap_err();
    assert_eq!(e.class(), ErrorClass::InvalidGisStructure);

    // A KML over the STRICT record cap declines typed (resource limit, not invalid).
    let mut big = String::from("<kml xmlns=\"http://www.opengis.net/kml/2.2\"><Document>");
    for _ in 0..5000 {
        big.push_str("<Placemark/>");
    }
    big.push_str("</Document></kml>");
    let e = vole_document::adapter::gis::parse(big.as_bytes(), Limits::STRICT, true).unwrap_err();
    assert_eq!(e.class(), ErrorClass::ResourceLimit);
    assert_ne!(
        detect_document_format(big.as_bytes(), Limits::STRICT),
        DocumentFormat::Gis
    );
    // But it is still well-formed XML, so it stays `Xml`.
    assert_eq!(
        detect_document_format(big.as_bytes(), Limits::STRICT),
        DocumentFormat::Xml
    );
}

#[test]
fn interleaved_fixtures_do_not_alias() {
    // ADR-0060: a source-reading node must carry its own source-identity input.
    let root = temp_dir("alias");
    let mut store = FieldStore::open(&root).unwrap();

    let fa = ingest_pdf(&mut store, &opaque_descriptor(KML), Limits::DEFAULT)
        .unwrap()
        .field;
    let fb = ingest_pdf(&mut store, &opaque_descriptor(GPX), Limits::DEFAULT)
        .unwrap()
        .field;

    for (field, src, want) in [(fa, KML, "kml".to_string()), (fb, GPX, "gpx".to_string())] {
        let got = answer_json(&observe_ok(
            &mut store,
            &field,
            Selector::GisRoot,
            Representation::Structure,
        ));
        assert!(
            got.contains(&format!("\"dialect\":\"{want}\"")),
            "aliased source for {field:?}: {got}"
        );
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
fn gis_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived GIS model reads the source bytes, so its single
    // dependency must be the exact `DocumentExact` root (keyed by `sha256(source)`).
    // This is what prevents cross-field source aliasing.
    let fx = Fixture::new("adr0060", KML);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let manifest = field.manifest().clone();
    let root = manifest.root_node;
    let root_node = load_node(fx.store.seeds(), &root).unwrap();
    assert_eq!(root_node.kind, NodeKind::DocumentExact);

    assert!(manifest.has_index());
    let istore = FsIndexStore::open(fx.store.root()).unwrap();
    let iroot = NodeId::from_bytes(manifest.index_root);
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_GIS_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::GisModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the GIS model must depend on exactly the exact root"
    );
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0x6153_2020_23DE_AD00;
    for _ in 0..128 {
        let mut buf = vec![0u8; 512];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        if let Ok(m) = vole_document::adapter::gis::parse(&buf, Limits::STRICT, true) {
            let _ = vole_document::adapter::gis::canonical_text(&m, &buf, 1 << 20);
            let _ = vole_document::adapter::gis::build_gis_model(&buf, Limits::STRICT);
        }
    }
    // The XML-shaped hostile inputs must also never panic.
    for src in [
        b"<kml".as_slice(),
        b"<kml xmlns=\"http://www.opengis.net/kml/2.2\">".as_slice(),
        b"<gpx xmlns=\"http://www.topografix.com/GPX/1/1\"><trk>".as_slice(),
    ] {
        let _ = detect_document_format(src, Limits::STRICT);
        let _ = vole_document::adapter::gis::parse(src, Limits::STRICT, true);
    }
}
