#![cfg(feature = "store")]
//! Phase 9.1 court: the `ObjectStore` abstraction, `EmbeddedStore`, the
//! store-backed descriptor form (`EXTERNAL_REF`), and externalize/hydrate/GC.
//!
//! Every store-backed assertion still requires the exact profile's triple:
//! `materialized_length == source_length`, `SHA256(materialized) == SHA256(source)`,
//! and `byte_compare` equality.

use std::fs;
use std::path::PathBuf;

use vole_document::adapter::opaque::propose;
use vole_document::adapter::pdf::{large_pdf, sample_pdfs};
use vole_document::container::{Descriptor, ObjectSource, UNIVERSE};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::integrity::sha256;
use vole_document::limits::Limits;
use vole_document::materialize::{materialize, materialize_with};
use vole_document::store::{EmbeddedStore, ObjectStore, externalize, gc, hydrate};

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
        path.push(format!("vole-store-{tag}-{}-{nanos}", std::process::id()));
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

/// Serialize `d`, parse it back, and return the reparsed descriptor (so every
/// case exercises the on-disk canonical form, not just the in-memory model).
fn roundtrip(d: &Descriptor) -> Descriptor {
    let (bytes, cost) = d.serialize().unwrap();
    assert_eq!(cost.total(), bytes.len() as u64);
    Descriptor::parse(&bytes, DEFAULT).unwrap().descriptor
}

/// Externalize a standalone descriptor into a fresh store and return the store
/// plus the store-backed descriptor.
fn externalized(dir: &TempDir, source: &[u8]) -> (EmbeddedStore, Descriptor) {
    let d = roundtrip(&propose(source, DEFAULT).unwrap());
    let mut store = EmbeddedStore::open(dir.join("store")).unwrap();
    let mut backed = d.clone();
    externalize(&mut backed, &vole_document::store::NullResolver, &mut store).unwrap();
    assert!(
        backed
            .objects
            .iter()
            .all(|o| matches!(o, ObjectSource::External { .. })),
        "every object must be external after externalize"
    );
    (store, backed)
}

/// Assert the exact-profile triple plus the `INTEGRITY` digest equality.
fn assert_exact(out: &[u8], source: &[u8], label: &str) {
    assert_eq!(out.len(), source.len(), "[{label}] length");
    assert_eq!(out, source, "[{label}] bytes");
    assert_eq!(sha256(out), sha256(source), "[{label}] sha256");
}

fn store_backed_courts(source: &[u8], label: &str) {
    let dir = TempDir::new(label);
    let (store, backed) = externalized(&dir, source);

    // Store-backed serialization is exactly costed and emits EXTERNAL_REF.
    let (ebytes, ecost) = backed.serialize().unwrap();
    assert_eq!(ecost.total(), ebytes.len() as u64, "[{label}] cost total");
    assert_eq!(
        ecost.external_refs, 40,
        "[{label}] one 40-byte EXTERNAL_REF"
    );
    assert_eq!(ecost.objects, 0, "[{label}] no inline objects remain");

    let parsed = Descriptor::parse(&ebytes, DEFAULT).unwrap();
    let out = materialize_with(&parsed, &store, DEFAULT).unwrap();
    assert_exact(&out, source, label);

    // The whole-source SHA-256 court still holds and equals the standalone one.
    let standalone = propose(source, DEFAULT).unwrap();
    assert_eq!(parsed.descriptor.source_sha256, standalone.source_sha256);

    // With no resolver, the same descriptor fails closed, never guesses.
    let err = materialize(&parsed, DEFAULT).unwrap_err();
    assert_eq!(
        err.class(),
        ErrorClass::MissingExternalObject,
        "[{label}] standalone materialize of a store-backed descriptor"
    );

    // Hydrating back to standalone reproduces the exact original bytes.
    let mut hydrated = parsed.descriptor.clone();
    hydrate(&mut hydrated, &store).unwrap();
    assert!(
        hydrated.objects.iter().all(|o| o.as_inline().is_some()),
        "[{label}] every object must be inline after hydrate"
    );
    let (hbytes, _) = hydrated.serialize().unwrap();
    let hparsed = Descriptor::parse(&hbytes, DEFAULT).unwrap();
    let hout = materialize(&hparsed, DEFAULT).unwrap();
    assert_exact(&hout, source, label);
}

#[test]
fn externalize_materializes_exactly_on_flate_pdf() {
    store_backed_courts(&flate_pdf(), "flate");
}

#[test]
fn externalize_materializes_exactly_on_large_pdf() {
    let source = large_pdf(8, 16 * 1024);
    assert!(source.len() > 8 * 1024, "large_pdf must be non-trivial");
    store_backed_courts(&source, "large");
}

/// Identical inline objects externalize to ONE store entry at refcount 2 and
/// materialize back byte-exactly.
#[test]
fn identical_objects_dedup_to_one_entry() {
    let shared = b"shared-object-bytes".to_vec();
    let source: Vec<u8> = [shared.clone(), shared.clone()].concat();
    let d = Descriptor {
        universe: UNIVERSE.to_string(),
        source_format: vole_document::SOURCE_FORMAT_OPAQUE,
        format_basis: "test;dup".to_string(),
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
        source_sha256: sha256(&source),
        source_len: source.len() as u64,
    };

    let dir = TempDir::new("dedup");
    let mut store = EmbeddedStore::open(dir.join("store")).unwrap();
    let mut backed = roundtrip(&d);
    externalize(&mut backed, &vole_document::store::NullResolver, &mut store).unwrap();

    let ids: Vec<_> = backed
        .objects
        .iter()
        .map(|o| match o {
            ObjectSource::External { id, .. } => *id,
            ObjectSource::Inline(_) => panic!("all objects external"),
        })
        .collect();
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[0], ids[1], "equal bytes must share one content id");

    let stats = store.stats().unwrap();
    assert_eq!(stats.object_count, 1, "one stored object at refcount 2");
    assert_eq!(stats.total_bytes, source.len() as u64 / 2);
    assert_eq!(stats.stored_bytes, stats.total_bytes);

    let (ebytes, _) = backed.serialize().unwrap();
    let parsed = Descriptor::parse(&ebytes, DEFAULT).unwrap();
    let out = materialize_with(&parsed, &store, DEFAULT).unwrap();
    assert_exact(&out, &source, "dedup");
}

/// GC sweeps exactly the unreachable objects and keeps every reachable one.
#[test]
fn gc_sweeps_unreachable_and_keeps_reachable() {
    let dir = TempDir::new("gc");
    let mut store = EmbeddedStore::open(dir.join("store")).unwrap();

    let source_a = b"reachable-object-A".to_vec();
    let source_b = b"orphan-object-BB".to_vec();
    let mut a = roundtrip(&propose(&source_a, DEFAULT).unwrap());
    let mut b = roundtrip(&propose(&source_b, DEFAULT).unwrap());
    let null = vole_document::store::NullResolver;
    externalize(&mut a, &null, &mut store).unwrap();
    externalize(&mut b, &null, &mut store).unwrap();
    assert_eq!(store.stats().unwrap().object_count, 2);

    // Both roots reachable: nothing is swept, nothing dangles.
    let all = [a.clone(), b.clone()];
    let report = gc(&all, &store).unwrap();
    assert_eq!(report.reachable, 2);
    assert_eq!(report.swept, 0);
    assert_eq!(report.bytes_reclaimed, 0);
    assert!(report.dangling.is_empty());
    assert_eq!(store.stats().unwrap().object_count, 2);

    // Only root A: B's object is swept.
    let report = gc(&[a.clone()], &store).unwrap();
    assert_eq!(report.reachable, 1);
    assert_eq!(report.swept, 1);
    assert_eq!(report.bytes_reclaimed, source_b.len() as u64);
    assert!(report.dangling.is_empty());
    assert_eq!(store.stats().unwrap().object_count, 1);

    // A still materializes; B now fails closed as missing.
    let (abytes, _) = a.serialize().unwrap();
    let aparsed = Descriptor::parse(&abytes, DEFAULT).unwrap();
    assert_exact(
        &materialize_with(&aparsed, &store, DEFAULT).unwrap(),
        &source_a,
        "gc-keep",
    );
    let (bbytes, _) = b.serialize().unwrap();
    let bparsed = Descriptor::parse(&bbytes, DEFAULT).unwrap();
    assert_eq!(
        materialize_with(&bparsed, &store, DEFAULT)
            .unwrap_err()
            .class(),
        ErrorClass::MissingExternalObject
    );
}

/// A dangling reference (a root whose object is absent) is reported, not served.
#[test]
fn gc_reports_dangling_reference() {
    let dir = TempDir::new("dangling");
    let mut store = EmbeddedStore::open(dir.join("store")).unwrap();
    let mut a = roundtrip(&propose(b"dangling-bytes", DEFAULT).unwrap());
    externalize(&mut a, &vole_document::store::NullResolver, &mut store).unwrap();
    // Delete the only object out from under the root.
    let stored = store.list().unwrap();
    assert_eq!(stored.len(), 1);
    store.remove(&stored[0].0).unwrap();

    let report = gc(&[a.clone()], &store).unwrap();
    assert_eq!(report.reachable, 1);
    assert_eq!(report.dangling.len(), 1);
    assert_eq!(report.swept, 0);
}

/// `put`/`get`/`get_range`/`contains`/`len` behave and the content gate fires.
#[test]
fn store_ops_and_integrity_gate() {
    let dir = TempDir::new("ops");
    let mut store = EmbeddedStore::open(dir.join("store")).unwrap();

    let id = store.put(b"hello world").unwrap();
    assert_eq!(id.to_hex().len(), 64);
    assert_eq!(
        id,
        vole_document::store::Id::from_hex(&id.to_hex()).unwrap()
    );
    assert!(store.contains(&id).unwrap());
    assert_eq!(store.len(&id).unwrap(), 11);
    assert_eq!(store.get(&id).unwrap(), b"hello world");
    assert_eq!(store.get_range(&id, 6, 5).unwrap(), b"world");

    // Strict range: offset + len past the end is a typed error, never a clip.
    assert_eq!(
        store.get_range(&id, 6, 6).unwrap_err().class(),
        ErrorClass::IntegrityMismatch
    );

    // Idempotent put: identical bytes are stored once.
    assert_eq!(store.put(b"hello world").unwrap(), id);
    assert_eq!(store.stats().unwrap().object_count, 1);

    // Corrupting the stored bytes is caught by the re-hash gate.
    let hex = id.to_hex();
    let path = store
        .root()
        .join("objects")
        .join(&hex[0..2])
        .join(&hex[2..4])
        .join(&hex);
    fs::write(&path, b"not the object").unwrap();
    assert_eq!(
        store.get(&id).unwrap_err().class(),
        ErrorClass::IntegrityMismatch
    );

    // An absent id is MissingExternalObject.
    let missing = vole_document::store::Id::from_bytes([0xAB; 32]);
    assert_eq!(
        store.get(&missing).unwrap_err().class(),
        ErrorClass::MissingExternalObject
    );
}

/// Externalization is deterministic: identical inputs give identical ids,
/// stats, and serialized bytes.
#[test]
fn externalize_is_deterministic() {
    let source = flate_pdf();
    let dir = TempDir::new("det");
    let (store1, backed1) = externalized(&dir, &source);
    let (store2, backed2) = externalized(&dir, &source);

    let (b1, _) = backed1.serialize().unwrap();
    let (b2, _) = backed2.serialize().unwrap();
    assert_eq!(b1, b2, "store-backed serialization must be deterministic");
    assert_eq!(store1.stats().unwrap(), store2.stats().unwrap());
}

/// `externalize` is idempotent and `hydrate(externalize(d))` restores the exact
/// standalone bytes when `d` carries no directory.
#[test]
fn externalize_hydrate_roundtrips_bytes() {
    let source = b"round-trippable exact bytes".to_vec();
    let original = roundtrip(&propose(&source, DEFAULT).unwrap());
    let (obytes, _) = original.serialize().unwrap();

    let dir = TempDir::new("roundtrip");
    let mut store = EmbeddedStore::open(dir.join("store")).unwrap();
    let mut backed = original.clone();
    externalize(&mut backed, &vole_document::store::NullResolver, &mut store).unwrap();

    let mut hydrated = backed.clone();
    hydrate(&mut hydrated, &store).unwrap();
    let (hbytes, _) = hydrated.serialize().unwrap();
    assert_eq!(
        hbytes, obytes,
        "hydrate must restore the exact standalone descriptor bytes"
    );

    // Re-externalizing the hydrated descriptor reproduces the same ordered ids.
    let mut again = hydrated.clone();
    externalize(&mut again, &vole_document::store::NullResolver, &mut store).unwrap();
    let table = |d: &Descriptor| -> Vec<ObjectSource> { d.objects.clone() };
    assert_eq!(table(&again), table(&backed));
}
