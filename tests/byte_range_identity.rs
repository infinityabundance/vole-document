//! Regression: source-dependent node identity must include the source.
//!
//! A `SourceSlice` node (`Selector::ByteRange`) reads `source[offset..offset+len]`
//! directly, so its span coordinates alone are not an identity. The `doc-baseline`
//! court and the derived cache are keyed on the `NodeId` alone, so before this was
//! fixed, two *different* sources in one store at the same `(offset, len)` aliased
//! and one document's bytes were served for the other.
//!
//! This test builds two distinct sources into one shared store and asserts the two
//! `--byte-range 0..4 --kind exact` answers are different and each matches its own
//! source's slice (the reachable wrong-answer path from the audit).
//!
//! Gated on `field`; a build without it compiles an empty target.

#![cfg(feature = "field")]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::field::ingest::ingest_pdf;
use vole_document::field::observe::{ObserveRequest, Representation, Selector, observe};
use vole_document::field::provenance::AnswerValue;
use vole_document::field::{FieldId, FieldStore};
use vole_document::integrity::sha256;
use vole_document::limits::Limits;

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-identity-{label}-{}-{}",
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
        format_basis: "opaque:identity-test".to_string(),
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

fn observe_slice(store: &mut FieldStore, field: &FieldId, offset: u64, len: u64) -> Vec<u8> {
    let req = ObserveRequest::new(
        Selector::ByteRange { offset, len },
        Representation::ExactBytes,
    );
    let (answer, _, _) = observe(store, field, &req, Limits::DEFAULT).unwrap();
    match answer.value {
        AnswerValue::Bytes(b) => b,
        other => panic!("expected exact bytes, got {other:?}"),
    }
}

#[test]
fn byte_range_of_two_sources_in_one_store_never_aliases() {
    let root = temp_dir("byte-range");
    let store_root = root.join("store");

    // Two DIFFERENT sources that share the same byte length and the same slice span.
    let a = b"AAAAaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_vec();
    let b = b"BBBBbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_vec();
    assert_eq!(a.len(), b.len());

    let mut store = FieldStore::open(&store_root).unwrap();
    let fa = ingest_pdf(&mut store, &opaque_descriptor(&a), Limits::DEFAULT)
        .unwrap()
        .field;
    let fb = ingest_pdf(&mut store, &opaque_descriptor(&b), Limits::DEFAULT)
        .unwrap()
        .field;

    // Observe A first: this persists A's `SourceSlice` in the shared cache, which
    // is exactly the interleaving that used to serve A's bytes for B.
    let slice_a = observe_slice(&mut store, &fa, 0, 4);
    let slice_b = observe_slice(&mut store, &fb, 0, 4);

    assert_eq!(slice_a, b"AAAA");
    assert_eq!(slice_b, b"BBBB");
    assert_ne!(
        sha256(&slice_a),
        sha256(&slice_b),
        "two sources must not share a byte-range answer"
    );
    // Each slice matches its own source's prefix.
    assert_eq!(slice_a, a[..4]);
    assert_eq!(slice_b, b[..4]);

    // A whole-source slice of the same store is likewise per-source.
    let whole_a = observe_slice(&mut store, &fa, 0, a.len() as u64);
    let whole_b = observe_slice(&mut store, &fb, 0, b.len() as u64);
    assert_eq!(whole_a, a);
    assert_eq!(whole_b, b);

    std::fs::remove_dir_all(&root).ok();
}

/// A field id must be a function of its source, never of the store's other
/// fields, so building the same source into a fresh store is *the same field*.
#[test]
fn field_identity_is_independent_of_store_contents() {
    let root = temp_dir("field-identity");
    let a = b"source-one-aaaaaaaaaaaaaaaaaaaaaaa".to_vec();

    let shared_root = root.join("shared");
    let fa_in_shared = {
        let mut store = FieldStore::open(&shared_root).unwrap();
        // A second, unrelated field first.
        let _other = ingest_pdf(
            &mut store,
            &opaque_descriptor(b"unrelated-bbbbbbbbbbbbbbbbbbbbbbbbb"),
            Limits::DEFAULT,
        )
        .unwrap();
        ingest_pdf(&mut store, &opaque_descriptor(&a), Limits::DEFAULT)
            .unwrap()
            .field
    };
    let fa_alone = {
        let mut store = FieldStore::open(root.join("alone")).unwrap();
        ingest_pdf(&mut store, &opaque_descriptor(&a), Limits::DEFAULT)
            .unwrap()
            .field
    };
    assert_eq!(
        fa_in_shared, fa_alone,
        "a field id must not depend on the store's other fields"
    );

    std::fs::remove_dir_all(&root).ok();
}
