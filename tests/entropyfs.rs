#![cfg(feature = "entropyfs-store")]
//! Phase 9.2 court: the optional `EntropyFsStore` adapter.
//!
//! Proves that a descriptor externalized into the reference `EmbeddedStore`
//! materializes byte-exactly when served by the EntropyFS engine: the two
//! backends share one content id space (`BLAKE3-256`), so the same
//! `EXTERNAL_REF` table resolves unchanged. Every store-backed assertion still
//! requires the exact-profile triple (`length`, `SHA256`, `byte_compare`).

use std::fs;
use std::path::PathBuf;

use vole_document::adapter::opaque::propose;
use vole_document::adapter::pdf::sample_pdfs;
use vole_document::container::{Descriptor, ObjectSource, UNIVERSE};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::integrity::sha256;
use vole_document::limits::Limits;
use vole_document::materialize::{materialize, materialize_with};
use vole_document::store::{EmbeddedStore, EntropyFsStore, Id, ObjectStore, externalize};

const DEFAULT: Limits = Limits::DEFAULT;

/// A self-cleaning temporary directory (no `tempfile` dependency).
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut path = std::env::temp_dir();
        path.push(format!("vole-efs-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        TempDir { path }
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn flate_pdf() -> Vec<u8> {
    sample_pdfs()
        .into_iter()
        .find(|(n, _)| *n == "flate.pdf")
        .map(|(_, b)| b)
        .expect("flate.pdf is in the sample corpus")
}

fn roundtrip(d: &Descriptor) -> Descriptor {
    let (bytes, cost) = d.serialize().unwrap();
    assert_eq!(cost.total(), bytes.len() as u64);
    Descriptor::parse(&bytes, DEFAULT).unwrap().descriptor
}

fn assert_exact(out: &[u8], source: &[u8], label: &str) {
    assert_eq!(out.len(), source.len(), "[{label}] length");
    assert_eq!(out, source, "[{label}] bytes");
    assert_eq!(sha256(out), sha256(source), "[{label}] sha256");
}

/// An `EmbeddedStore`-externalized descriptor materializes byte-exactly through
/// `EntropyFsStore`; the ids are identical, so the object table needs no rewrite.
#[test]
fn embedded_externalized_descriptor_materializes_through_entropyfs() {
    let dir = TempDir::new("roundtrip");
    let source = flate_pdf();
    let standalone = roundtrip(&propose(&source, DEFAULT).unwrap());

    // Externalize into the reference embedded store.
    let mut embedded = EmbeddedStore::open(dir.join("embedded")).unwrap();
    let mut backed = standalone.clone();
    externalize(
        &mut backed,
        &vole_document::store::NullResolver,
        &mut embedded,
    )
    .unwrap();
    assert!(
        backed
            .objects
            .iter()
            .all(|o| matches!(o, ObjectSource::External { .. })),
        "every object is external after externalize"
    );

    // Create an EntropyFS store and copy every object across by content id.
    let mut efs = EntropyFsStore::create(dir.join("efs")).unwrap();
    let mut object_bytes: Vec<(Id, Vec<u8>)> = Vec::new();
    for src in &backed.objects {
        let ObjectSource::External { id, len } = src else {
            panic!("all objects external");
        };
        let bytes = embedded.get(id).unwrap();
        assert_eq!(bytes.len() as u64, *len, "declared length");
        let put_id = efs.put(&bytes).unwrap();
        assert_eq!(put_id, *id, "BLAKE3-256 ids must match across backends");
        assert_eq!(Id::of(&bytes), *id, "Id::of is the shared content id");
        object_bytes.push((*id, bytes));
    }

    // Materialize the SAME store-backed descriptor through the EntropyFS engine.
    let (ebytes, ecost) = backed.serialize().unwrap();
    assert_eq!(ecost.total(), ebytes.len() as u64);
    let parsed = Descriptor::parse(&ebytes, DEFAULT).unwrap();
    let out = materialize_with(&parsed, &efs, DEFAULT).unwrap();
    assert_exact(&out, &source, "entropyfs");

    // Standalone materialization fails closed (no resolver) as for any backend.
    assert_eq!(
        materialize(&parsed, DEFAULT).unwrap_err().class(),
        ErrorClass::MissingExternalObject
    );

    // get / get_range / contains all agree, and get_range is strict.
    for (id, bytes) in &object_bytes {
        assert!(efs.contains(id).unwrap());
        assert_eq!(&efs.get(id).unwrap(), bytes);
        let mid = bytes.len() as u64 / 2;
        assert_eq!(efs.get_range(id, 0, bytes.len() as u64).unwrap(), *bytes);
        assert_eq!(
            efs.get_range(id, mid, bytes.len() as u64 - mid).unwrap(),
            bytes[mid as usize..]
        );
        // offset + len past the end is a typed error, never a silent EOF clip.
        assert_eq!(
            efs.get_range(id, mid, bytes.len() as u64 - mid + 1)
                .unwrap_err()
                .class(),
            ErrorClass::IntegrityMismatch
        );
    }

    // An absent id is MissingExternalObject.
    let missing = Id::from_bytes([0x5A; 32]);
    assert_eq!(
        efs.get(&missing).unwrap_err().class(),
        ErrorClass::MissingExternalObject
    );

    // EntropyFS has no per-blob enumeration or delete: both decline.
    assert_eq!(
        efs.list().unwrap_err().class(),
        ErrorClass::UnsupportedFeature
    );
    let some_id = object_bytes[0].0;
    assert_eq!(
        efs.remove(&some_id).unwrap_err().class(),
        ErrorClass::UnsupportedFeature
    );
}

/// Identical bytes externalized from a two-object descriptor collapse to one
/// EntropyFS blob at refcount 2, and materialize byte-exactly.
#[test]
fn identical_objects_dedup_to_one_entropyfs_blob() {
    let shared = b"shared-object-bytes".to_vec();
    let source: Vec<u8> = [shared.clone(), shared.clone()].concat();
    let d = Descriptor {
        universe: UNIVERSE.to_string(),
        source_format: vole_document::SOURCE_FORMAT_OPAQUE,
        format_basis: "test;efs-dup".to_string(),
        models: vec![],
        channels: vec![],
        objects: vec![
            ObjectSource::Inline(shared.clone()),
            ObjectSource::Inline(shared),
        ],
        program: Program::new(vec![
            Op::EmitObject { object_id: 0 },
            Op::EmitObject { object_id: 1 },
        ]),
        observation_index: None,
        seek_directory: false,
        checkpoints: None,
        source_sha256: sha256(&source),
        source_len: source.len() as u64,
    };

    let dir = TempDir::new("dedup");
    let mut efs = EntropyFsStore::create(dir.join("efs")).unwrap();
    let mut backed = roundtrip(&d);
    externalize(&mut backed, &vole_document::store::NullResolver, &mut efs).unwrap();

    let ids: Vec<Id> = backed
        .objects
        .iter()
        .map(|o| match o {
            ObjectSource::External { id, .. } => *id,
            ObjectSource::Inline(_) => panic!("all objects external"),
        })
        .collect();
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[0], ids[1], "equal bytes share one content id");
    assert!(efs.contains(&ids[0]).unwrap());

    let (ebytes, _) = backed.serialize().unwrap();
    let parsed = Descriptor::parse(&ebytes, DEFAULT).unwrap();
    assert_exact(
        &materialize_with(&parsed, &efs, DEFAULT).unwrap(),
        &source,
        "efs-dedup",
    );
}

/// A store created by [`EntropyFsStore::create`] is re-openable by
/// [`EntropyFsStore::open`] and still serves its blobs.
#[test]
fn create_then_open_round_trips_a_blob() {
    let dir = TempDir::new("reopen");
    let bytes = b"durable across reopen".to_vec();
    let id = {
        let mut efs = EntropyFsStore::create(dir.join("efs")).unwrap();
        let id = efs.put(&bytes).unwrap();
        efs.sync().unwrap();
        id
    };
    let efs = EntropyFsStore::open(dir.join("efs")).unwrap();
    assert!(efs.contains(&id).unwrap());
    assert_eq!(efs.get(&id).unwrap(), bytes);
}
