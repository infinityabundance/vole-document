//! Phase 21.34 court: the scientific-container adapter (NetCDF classic + HDF5 subset).
//!
//! Gated on `hdf`, so a build without the feature compiles an empty target.
//!
//! A scientific container has no package layer: the exact leaf is the whole source
//! (a `DocumentExact`) and every derived observation is a bounded, representation-
//! preserving projection of it. The court checks:
//!
//! 1. **Byte-based detection** and the boundaries: `CDF\x01`/`CDF\x02` → `Hdf`, a
//!    valid HDF5 superblock (offset 0 or 512) → `Hdf`; prose, a truncated superblock,
//!    and a bad-version superblock stay `Opaque`.
//! 2. **Representation preservation**: the NetCDF dimension/attribute/variable model
//!    (incl. the unlimited dimension and 64-bit CDF-2 offsets) and the HDF5
//!    group/dataset/attribute model.
//! 3. **Selectors**: native `hdf-dialect`/`hdf-object`/`hdf-dimension`/`hdf-attribute`/
//!    `hdf-find` plus common `metadata`/`text`/`search-match`.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and descriptor are deleted (fresh process).
//! 5. **Fail closed**: malformed inputs decline typed; out-of-subset HDF5 constructs
//!    (a filtered dataset, a v2 object header) decline typed; random bytes never panic.
//! 6. **No cross-field aliasing** (ADR-0060): the model node depends on the exact
//!    `sha256(source)` root.

#![cfg(all(feature = "field", feature = "hdf"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_HDF_MODEL, SelectorKey, lookup};
use vole_document::field::ingest::{IngestReport, ingest_pdf};
use vole_document::field::node::NodeKind;
use vole_document::field::observe::{ObserveRequest, Representation, Selector, observe};
use vole_document::field::provenance::{AnswerValue, FieldAnswer};
use vole_document::field::{Field, FieldId, FieldStore};
use vole_document::integrity::sha256;
use vole_document::limits::Limits;
use vole_document::store::NodeId;

// The dependency-free NetCDF/HDF5 fixture writer is shared with the court example.
#[path = "../examples/hdf_fixtures.rs"]
mod fixtures;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-hdf-{label}-{}-{}",
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
        format_basis: "opaque:hdf-test".to_string(),
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

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_byte_based_and_conservative() {
    for src in [fixtures::build_netcdf(1), fixtures::build_netcdf(2)] {
        assert_eq!(
            detect_document_format(&src, Limits::DEFAULT),
            DocumentFormat::Hdf
        );
    }
    let h5 = fixtures::build_hdf5(0, false, false);
    assert_eq!(
        detect_document_format(&h5, Limits::DEFAULT),
        DocumentFormat::Hdf
    );
    let h5_ub = fixtures::build_hdf5(512, false, false);
    assert_eq!(
        detect_document_format(&h5_ub, Limits::DEFAULT),
        DocumentFormat::Hdf
    );

    // A random `\x89HDF`-prefixed blob with a bad superblock is NOT detected.
    let mut bad = fixtures::build_hdf5(0, false, false);
    bad[8] = 7; // invalid superblock version
    assert_eq!(
        detect_document_format(&bad, Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    // A signature-only blob and an all-zero-sizes blob are NOT detected.
    assert_eq!(
        detect_document_format(b"\x89HDF\r\n\x1a\n", Limits::DEFAULT),
        DocumentFormat::Opaque
    );

    // Prose stays Opaque.
    assert_eq!(
        detect_document_format(
            b"The quick brown fox jumps over the lazy dog.\nPlain prose, not a container.\n",
            Limits::DEFAULT
        ),
        DocumentFormat::Opaque
    );
}

#[test]
fn netcdf_cdf1_model_preserves_dimensions_attributes_variables() {
    let src = fixtures::build_netcdf(1);
    let mut fx = Fixture::new("cdf1", &src);
    let field = fx.report.field;

    let dialect = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::HdfDialect,
        Representation::Text,
    ));
    assert_eq!(dialect, "netcdf");

    // Dimensions: `time` is the unlimited/record dimension.
    let d0 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::HdfDimension { index: 0 },
        Representation::Metadata,
    ));
    assert!(d0.contains("\"name\":\"time\""), "{d0}");
    assert!(d0.contains("\"unlimited\":true"), "{d0}");
    let d1 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::HdfDimension { index: 1 },
        Representation::Metadata,
    ));
    assert!(d1.contains("\"name\":\"lat\""), "{d1}");
    assert!(d1.contains("\"length\":3"), "{d1}");

    // Global attributes.
    let a0 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::HdfAttribute { index: 0 },
        Representation::Metadata,
    ));
    assert!(a0.contains("\"name\":\"title\""), "{a0}");
    assert!(a0.contains("\"type\":\"char\""), "{a0}");

    // Variables: `temp` (index 2) is a record variable with shape [0,3,4].
    let v2 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::HdfObject { index: 2 },
        Representation::Metadata,
    ));
    assert!(v2.contains("\"name\":\"temp\""), "{v2}");
    assert!(v2.contains("\"is_record\":true"), "{v2}");
    assert!(v2.contains("\"shape\":[0,3,4]"), "{v2}");
    assert!(v2.contains("\"type\":\"float32\""), "{v2}");

    // An out-of-range index declines typed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::HdfObject { index: 99 },
            Representation::Metadata
        ),
        ErrorClass::UnsupportedFeature
    );

    // Find resolves dimension/variable names; the common surface answers.
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::HdfFind {
            pattern: "lat".to_string(),
        },
        Representation::Metadata,
    ));
    assert!(found.contains("\"matches\":["), "{found}");
    assert!(found.contains("\"field\":\"dimension\""), "{found}");
    let common = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(common.contains("\"format\":\"hdf\""), "{common}");
    assert!(common.contains("\"dialect\":\"netcdf\""), "{common}");
    assert!(common.contains("\"numrecs\":2"), "{common}");
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert!(text.contains("temp"), "{text}");
}

#[test]
fn netcdf_cdf2_uses_64_bit_offsets() {
    let src = fixtures::build_netcdf(2);
    let mut fx = Fixture::new("cdf2", &src);
    let field = fx.report.field;
    let common = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(common.contains("\"dialect\":\"netcdf\""), "{common}");
    assert!(common.contains("\"objects\":3"), "{common}");
    // The record variable's data offset is a real 64-bit `begin`.
    let v2 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::HdfObject { index: 2 },
        Representation::Metadata,
    ));
    assert!(v2.contains("\"is_record\":true"), "{v2}");
}

#[test]
fn hdf5_basic_group_dataset_and_attribute() {
    let src = fixtures::build_hdf5(0, false, false);
    let mut fx = Fixture::new("h5basic", &src);
    let field = fx.report.field;

    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::HdfDialect,
            Representation::Text,
        )),
        "hdf5"
    );

    // Object 0 is the root group; object 1 is the dataset `dset`.
    let root = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::HdfObject { index: 0 },
        Representation::Metadata,
    ));
    assert!(root.contains("\"kind\":\"group\""), "{root}");
    assert!(root.contains("\"path\":\"/\""), "{root}");

    let dset = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::HdfObject { index: 1 },
        Representation::Metadata,
    ));
    assert!(dset.contains("\"kind\":\"dataset\""), "{dset}");
    assert!(dset.contains("\"path\":\"/dset\""), "{dset}");
    assert!(dset.contains("\"type\":\"int32\""), "{dset}");
    assert!(dset.contains("\"shape\":[4]"), "{dset}");
    assert!(dset.contains("\"attributes\":1"), "{dset}");

    // The dataset's exact data bytes are the exact source span (all-zero payload).
    let data = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        Selector::HdfObject { index: 1 },
        Representation::ExactBytes,
    ));
    assert_eq!(data.len(), 16);
    assert!(
        src.windows(data.len()).any(|w| w == data.as_slice()),
        "the dataset's exact bytes must be a source span"
    );

    // A group has no data bytes: ExactBytes declines typed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::HdfObject { index: 0 },
            Representation::ExactBytes
        ),
        ErrorClass::UnsupportedFeature
    );

    // The nested subgroup and its dataset are present.
    let grp = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::HdfObject { index: 2 },
        Representation::Metadata,
    ));
    assert!(grp.contains("\"path\":\"/grp\""), "{grp}");
    let inner = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::HdfObject { index: 3 },
        Representation::Metadata,
    ));
    assert!(inner.contains("\"path\":\"/grp/inner\""), "{inner}");
    assert!(inner.contains("\"depth\":2"), "{inner}");

    // The root-group attribute is addressable.
    let attr = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::HdfAttribute { index: 0 },
        Representation::Metadata,
    ));
    assert!(attr.contains("\"name\":\"root_attr\""), "{attr}");

    // HDF5 has no global dimension table: the dimension selector declines typed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::HdfDimension { index: 0 },
            Representation::Metadata
        ),
        ErrorClass::UnsupportedFeature
    );
}

#[test]
fn hdf5_userblock_signature_is_detected_and_parsed() {
    let src = fixtures::build_hdf5(512, false, false);
    assert_eq!(
        detect_document_format(&src, Limits::DEFAULT),
        DocumentFormat::Hdf
    );
    let mut fx = Fixture::new("h5ub", &src);
    let field = fx.report.field;
    let common = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(common.contains("\"dialect\":\"hdf5\""), "{common}");
    assert!(common.contains("\"datasets\":2"), "{common}");
}

#[test]
fn hdf5_out_of_subset_constructs_decline_typed() {
    // A filtered (chunked/compressed) dataset is outside the subset.
    let filtered = fixtures::build_hdf5(0, true, false);
    assert_eq!(
        detect_document_format(&filtered, Limits::DEFAULT),
        DocumentFormat::Hdf
    );
    let e = vole_document::adapter::hdf::parse(&filtered, Limits::DEFAULT).unwrap_err();
    assert_eq!(e.class(), ErrorClass::UnsupportedFeature);

    // A v2 object header is outside the subset.
    let ohv2 = fixtures::build_hdf5(0, false, true);
    assert_eq!(
        detect_document_format(&ohv2, Limits::DEFAULT),
        DocumentFormat::Hdf
    );
    let e = vole_document::adapter::hdf::parse(&ohv2, Limits::DEFAULT).unwrap_err();
    assert_eq!(e.class(), ErrorClass::UnsupportedFeature);
}

#[test]
fn malformed_inputs_decline_typed() {
    // A truncated NetCDF header: detected, model declines structurally.
    let mut trunc = b"CDF\x01".to_vec();
    trunc.extend_from_slice(&[0x00, 0x00]);
    assert_eq!(
        detect_document_format(&trunc, Limits::DEFAULT),
        DocumentFormat::Hdf
    );
    let e = vole_document::adapter::hdf::parse(&trunc, Limits::DEFAULT).unwrap_err();
    assert_eq!(e.class(), ErrorClass::InvalidHdfStructure);

    // A NetCDF with a bad dimension-list tag.
    let mut junk = b"CDF\x01".to_vec();
    junk.extend_from_slice(&0u32.to_be_bytes());
    junk.extend_from_slice(&0xdead_beefu32.to_be_bytes());
    let e = vole_document::adapter::hdf::parse(&junk, Limits::DEFAULT).unwrap_err();
    assert_eq!(e.class(), ErrorClass::InvalidHdfStructure);

    // An out-of-range declared NetCDF data offset declines structurally.
    let bad = fixtures::build_netcdf_bad_offset();
    assert_eq!(
        detect_document_format(&bad, Limits::DEFAULT),
        DocumentFormat::Hdf
    );
    let e = vole_document::adapter::hdf::parse(&bad, Limits::DEFAULT).unwrap_err();
    assert_eq!(e.class(), ErrorClass::InvalidHdfStructure);

    // A truncated HDF5 superblock (signature only) is not even detected.
    assert_eq!(
        detect_document_format(b"\x89HDF\r\n\x1a\n", Limits::DEFAULT),
        DocumentFormat::Opaque
    );
}

#[test]
fn exact_materialization_is_byte_identical() {
    for (label, src) in [
        ("exact-cdf1", fixtures::build_netcdf(1)),
        ("exact-cdf2", fixtures::build_netcdf(2)),
        ("exact-h5", fixtures::build_hdf5(0, false, false)),
        ("exact-h5ub", fixtures::build_hdf5(512, false, false)),
    ] {
        let fx = Fixture::new(label, &src);
        let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
        let out = field.materialize_exact(Limits::DEFAULT).unwrap();
        assert_eq!(out.len(), src.len(), "{label}");
        assert_eq!(sha256(&out), sha256(&src), "{label}");
        assert_eq!(out, src, "{label}");
    }
}

#[test]
fn removal_then_fresh_process_is_queryable_and_exact() {
    let root = temp_dir("removal");
    let src_path = root.join("data.nc");
    let desc_path = root.join("data.voldoc");
    let source = fixtures::build_netcdf(1);
    std::fs::write(&src_path, &source).unwrap();
    let descriptor = opaque_descriptor(&source);
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
    let dialect = answer_text(&observe_ok(
        &mut store2,
        &field_id,
        Selector::HdfDialect,
        Representation::Text,
    ));
    assert_eq!(dialect, "netcdf");

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), source.len());
    assert_eq!(sha256(&exact), sha256(&source));
    assert_eq!(exact, source);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn hdf_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived model reads the source bytes, so its single dependency
    // must be the exact `DocumentExact` root (keyed by `sha256(source)`).
    let source = fixtures::build_hdf5(0, false, false);
    let fx = Fixture::new("adr0060", &source);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let manifest = field.manifest().clone();
    let root = manifest.root_node;
    let root_node = load_node(fx.store.seeds(), &root).unwrap();
    assert_eq!(root_node.kind, NodeKind::DocumentExact);

    assert!(manifest.has_index());
    let istore = FsIndexStore::open(fx.store.root()).unwrap();
    let iroot = NodeId::from_bytes(manifest.index_root);
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_HDF_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::HdfModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the scientific-container model must depend on exactly the exact root"
    );
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    for _ in 0..256 {
        let mut buf = vec![0u8; 512];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        let _ = vole_document::adapter::hdf::detect(&buf, Limits::STRICT);
        let _ = vole_document::adapter::hdf::parse(&buf, Limits::STRICT);
    }
    // The magic-prefixed paths are the most likely false-positive paths.
    for sig in [&b"\x89HDF\r\n\x1a\n"[..], &b"CDF\x01"[..], &b"CDF\x02"[..]] {
        for _ in 0..256 {
            let mut buf = sig.to_vec();
            buf.resize(512, 0);
            for b in buf.iter_mut().skip(sig.len()) {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                *b = (x & 0xFF) as u8;
            }
            let _ = detect_document_format(&buf, Limits::DEFAULT);
            let _ = vole_document::adapter::hdf::parse(&buf, Limits::STRICT);
        }
    }
}
