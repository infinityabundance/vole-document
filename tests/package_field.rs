//! Phase 12.2 court: a ZIP as a first-class field root with procedural members.
//!
//! The whole file is gated on the `package` (and `field`) features so
//! `--no-default-features` builds an empty, compiling test target, and the default
//! build (no `package`) does not run it.
//!
//! Predeclared gates exercised here:
//!
//! 1. **Exactness** — the field materializes the exact original ZIP bytes
//!    (length + SHA-256 + byte equality), independent of any member decode.
//! 2. **Raw leaf** — a member's `EncodedBytes` equal its exact compressed span.
//! 3. **Progressive decode** — a deflated member's `DecodedBytes` equal the known
//!    plaintext; a stored member decodes by identity.
//! 4. **Identity by ordinal** — duplicate names are distinct; `lookup` finds
//!    members by their central-directory ordinal.
//! 5. **Persistence** — a repeated query reuses the derived cache (no re-decode);
//!    after `cache --clear` and in a **new process**, it re-decodes correctly.
//! 6. **Hostile safety** — a bad DEFLATE member and an unsupported method are typed
//!    declines, never a panic; the exact raw bytes remain available.
//! 7. **Bounded work** — a tiny member observation materializes a bounded number of
//!    seed nodes and never decodes the whole archive.

#![cfg(all(feature = "field", feature = "package"))]

use std::path::PathBuf;
use std::process::Command;

use vole_document::adapter::package::{crc32_iso_hdlc, scan};
use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::cache::DerivedCache;
use vole_document::field::explain::explain_analyze;
use vole_document::field::index::{
    FsIndexStore, SEL_PACKAGE_MEMBER_DECODED, SEL_PACKAGE_MEMBER_RAW, SelectorKey, lookup,
};
use vole_document::field::ingest_package::{PackageIngestReport, ingest_package};
use vole_document::field::observe::{
    ObserveRequest, ObserveStats, Representation, Selector, observe,
};
use vole_document::field::plan::plan;
use vole_document::field::provenance::{AnswerValue, Basis, FieldAnswer};
use vole_document::field::{Field, FieldId, FieldStore};
use vole_document::limits::Limits;
use vole_document::store::NodeId;

// ---------------------------------------------------------------------------
// Dependency-free ZIP writer (test ground truth)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Entry {
    name: Vec<u8>,
    /// Logical content (drives CRC, uncompressed size, and the plaintext).
    content: Vec<u8>,
    /// Bytes physically written as the member payload.
    compressed: Vec<u8>,
    method: u16,
    flags: u16,
}

impl Entry {
    fn stored(name: &str, content: &[u8]) -> Entry {
        Entry {
            name: name.as_bytes().to_vec(),
            content: content.to_vec(),
            compressed: content.to_vec(),
            method: 0,
            flags: 0,
        }
    }

    fn deflated(name: &str, content: &[u8]) -> Entry {
        Entry {
            name: name.as_bytes().to_vec(),
            content: content.to_vec(),
            compressed: raw_stored_deflate(content),
            method: 8,
            flags: 0,
        }
    }

    /// A method-8 member whose payload is deliberately not valid DEFLATE.
    fn bad_deflate(name: &str, content: &[u8]) -> Entry {
        Entry {
            name: name.as_bytes().to_vec(),
            content: content.to_vec(),
            compressed: vec![0xFF; 16],
            method: 8,
            flags: 0,
        }
    }

    /// A member with an unimplemented compression method.
    fn method(name: &str, content: &[u8], method: u16) -> Entry {
        Entry {
            name: name.as_bytes().to_vec(),
            content: content.to_vec(),
            compressed: content.to_vec(),
            method,
            flags: 0,
        }
    }
}

/// A valid single-block raw DEFLATE stream that stores `data` uncompressed.
fn raw_stored_deflate(data: &[u8]) -> Vec<u8> {
    assert!(
        data.len() <= 0xFFFF,
        "test deflate helper is single-block only"
    );
    let mut out = Vec::with_capacity(data.len() + 5);
    out.push(0x01); // BFINAL=1, BTYPE=00 (stored)
    let len = data.len() as u16;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&(!len).to_le_bytes());
    out.extend_from_slice(data);
    out
}

fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn build_zip(entries: &[Entry]) -> Vec<u8> {
    const UTF8_FLAG: u16 = 0x0800;
    let mut out: Vec<u8> = Vec::new();
    let mut offsets: Vec<u32> = Vec::new();
    let mut crcs: Vec<u32> = Vec::new();

    for e in entries {
        offsets.push(out.len() as u32);
        crcs.push(crc32_iso_hdlc(&e.content));
        let flags = e.flags | UTF8_FLAG;
        put_u32(&mut out, 0x0403_4b50); // local file header
        put_u16(&mut out, 20); // version needed
        put_u16(&mut out, flags);
        put_u16(&mut out, e.method);
        put_u16(&mut out, 0); // mod time
        put_u16(&mut out, 0); // mod date
        put_u32(&mut out, *crcs.last().unwrap());
        put_u32(&mut out, e.compressed.len() as u32);
        put_u32(&mut out, e.content.len() as u32);
        put_u16(&mut out, e.name.len() as u16);
        put_u16(&mut out, 0); // extra len
        out.extend_from_slice(&e.name);
        out.extend_from_slice(&e.compressed);
    }

    let cd_start = out.len() as u32;
    for (i, e) in entries.iter().enumerate() {
        let flags = e.flags | UTF8_FLAG;
        put_u32(&mut out, 0x0201_4b50); // central file header
        put_u16(&mut out, 20); // version made by
        put_u16(&mut out, 20); // version needed
        put_u16(&mut out, flags);
        put_u16(&mut out, e.method);
        put_u16(&mut out, 0); // mod time
        put_u16(&mut out, 0); // mod date
        put_u32(&mut out, crcs[i]);
        put_u32(&mut out, e.compressed.len() as u32);
        put_u32(&mut out, e.content.len() as u32);
        put_u16(&mut out, e.name.len() as u16);
        put_u16(&mut out, 0); // extra len
        put_u16(&mut out, 0); // comment len
        put_u16(&mut out, 0); // disk start
        put_u16(&mut out, 0); // internal attrs
        put_u32(&mut out, 0); // external attrs
        put_u32(&mut out, offsets[i]);
        out.extend_from_slice(&e.name);
    }
    let cd_end = out.len() as u32;

    put_u32(&mut out, 0x0605_4b50); // EOCD
    put_u16(&mut out, 0);
    put_u16(&mut out, 0);
    put_u16(&mut out, entries.len() as u16);
    put_u16(&mut out, entries.len() as u16);
    put_u32(&mut out, cd_end - cd_start);
    put_u32(&mut out, cd_start);
    put_u16(&mut out, 0); // comment len
    out
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-pkg-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// An exact opaque descriptor for `source` (a single inline object emitted whole).
fn opaque_descriptor(source: &[u8]) -> Vec<u8> {
    let d = Descriptor {
        universe: vole_document::container::UNIVERSE.to_string(),
        source_format: vole_document::SOURCE_FORMAT_OPAQUE,
        format_basis: "opaque:package-test".to_string(),
        models: vec![],
        channels: vec![],
        objects: vec![ObjectSource::Inline(source.to_vec())],
        program: Program::new(vec![Op::EmitObject { object_id: 0 }]),
        observation_index: None,
        seek_directory: false,
        source_sha256: vole_document::integrity::sha256(source),
        source_len: source.len() as u64,
    };
    d.serialize().unwrap().0
}

struct Fixture {
    root: PathBuf,
    store: FieldStore,
    report: PackageIngestReport,
    source: Vec<u8>,
}

impl Fixture {
    fn new(label: &str, entries: &[Entry]) -> Fixture {
        let root = temp_dir(label);
        let mut store = FieldStore::open(&root).unwrap();
        let source = build_zip(entries);
        let descriptor = opaque_descriptor(&source);
        let report = ingest_package(&mut store, &descriptor, Limits::DEFAULT).unwrap();
        Fixture {
            root,
            store,
            report,
            source,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

fn observe_eq(
    store: &mut FieldStore,
    field: &FieldId,
    selector: Selector,
    representation: Representation,
) -> (FieldAnswer, ObserveStats) {
    let req = ObserveRequest::new(selector, representation);
    let (answer, stats, _) = observe(store, field, &req, Limits::DEFAULT).unwrap();
    (answer, stats)
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

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// The exact raw span of member `ordinal`, from an independent scan.
fn member_raw(source: &[u8], ordinal: u32) -> Vec<u8> {
    let physical = scan(source, Limits::DEFAULT).unwrap();
    let m = physical
        .members
        .iter()
        .find(|m| m.id.ordinal == ordinal)
        .expect("member ordinal present");
    let (off, len) = m.data;
    source[off as usize..(off + len) as usize].to_vec()
}

fn answer_bytes(answer: &FieldAnswer) -> Vec<u8> {
    match &answer.value {
        AnswerValue::Bytes(b) => b.clone(),
        other => panic!("expected bytes, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Exactness + raw/decode
// ---------------------------------------------------------------------------

#[test]
fn package_field_is_exact_and_members_resolve() {
    let stored = b"stored payload bytes".to_vec();
    let deflated = b"deflated payload bytes, a little longer than stored".to_vec();
    let mut fx = Fixture::new(
        "exact",
        &[
            Entry::stored("a.txt", &stored),
            Entry::deflated("b.bin", &deflated),
        ],
    );
    assert_eq!(fx.report.member_count, 2);
    assert_eq!(fx.report.raw_nodes, 2);
    assert_eq!(fx.report.decoded_nodes, 2);
    assert_eq!(fx.report.declined_decodes, 0);
    assert!(fx.report.index_root.is_some());

    // 1. Exactness: length + SHA-256 + byte equality.
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), fx.source.len());
    assert_eq!(
        vole_document::integrity::sha256(&exact),
        vole_document::integrity::sha256(&fx.source)
    );
    assert_eq!(exact, fx.source);

    // The `PackageRoot` seed node itself materializes the exact source.
    let mut budget = vole_document::field::dag::EvalBudget::default();
    let root_bytes = field
        .materialize_node(&fx.report.root_node, Limits::DEFAULT, &mut budget)
        .unwrap();
    assert_eq!(root_bytes, fx.source);

    // 4. The hierarchical index resolves members by ordinal.
    let index_root = NodeId::from_bytes(field.manifest().index_root);
    drop(field);
    let istore = FsIndexStore::open(&fx.root).unwrap();
    let raw_entries = lookup(
        &istore,
        &index_root,
        &SelectorKey::new(SEL_PACKAGE_MEMBER_RAW, 1),
    )
    .unwrap();
    assert_eq!(raw_entries.len(), 1);
    let (off, len) = {
        let physical = scan(&fx.source, Limits::DEFAULT).unwrap();
        physical.members[1].data
    };
    assert_eq!(raw_entries[0].out_off, off);
    assert_eq!(raw_entries[0].out_len, len);
    assert_eq!(
        lookup(
            &istore,
            &index_root,
            &SelectorKey::new(SEL_PACKAGE_MEMBER_DECODED, 1)
        )
        .unwrap()
        .len(),
        1
    );

    // 2. Raw leaf equals the exact compressed span.
    let (raw0, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Member(0),
        Representation::EncodedBytes,
    );
    assert_eq!(answer_bytes(&raw0), member_raw(&fx.source, 0));
    assert_eq!(raw0.basis, Basis::DirectlyObserved);
    assert!(raw0.exact);

    // 3. Decoded bytes equal the known plaintext (identity for stored, inflate
    //    for deflated).
    let (dec0, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Member(0),
        Representation::DecodedBytes,
    );
    assert_eq!(answer_bytes(&dec0), stored);
    assert_eq!(dec0.basis, Basis::DeterministicallyDerived);
    assert!(!dec0.exact);
    let (dec1, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Member(1),
        Representation::DecodedBytes,
    );
    assert_eq!(answer_bytes(&dec1), deflated);
}

#[test]
fn duplicate_names_are_distinct_by_ordinal() {
    let first = b"the first duplicate".to_vec();
    let second = b"a different second duplicate payload".to_vec();
    let mut fx = Fixture::new(
        "dupes",
        &[
            Entry::stored("dup.txt", &first),
            Entry::deflated("dup.txt", &second),
        ],
    );
    assert_eq!(fx.report.member_count, 2);
    assert_eq!(
        answer_bytes(
            &observe_eq(
                &mut fx.store,
                &fx.report.field,
                Selector::Member(0),
                Representation::DecodedBytes
            )
            .0
        ),
        first
    );
    assert_eq!(
        answer_bytes(
            &observe_eq(
                &mut fx.store,
                &fx.report.field,
                Selector::Member(1),
                Representation::DecodedBytes
            )
            .0
        ),
        second
    );
    // The raw bytes of the two same-named members are genuinely different spans.
    assert_ne!(
        answer_bytes(
            &observe_eq(
                &mut fx.store,
                &fx.report.field,
                Selector::Member(0),
                Representation::EncodedBytes
            )
            .0
        ),
        answer_bytes(
            &observe_eq(
                &mut fx.store,
                &fx.report.field,
                Selector::Member(1),
                Representation::EncodedBytes
            )
            .0
        )
    );
}

#[test]
fn non_zip_input_is_a_typed_rejection() {
    let root = temp_dir("nonzip");
    let mut store = FieldStore::open(&root).unwrap();
    let source = b"this is not a zip archive".to_vec();
    let err = ingest_package(&mut store, &opaque_descriptor(&source), Limits::DEFAULT).unwrap_err();
    assert_eq!(err.class(), ErrorClass::InvalidZipStructure);
    std::fs::remove_dir_all(&root).ok();
}

// ---------------------------------------------------------------------------
// Hostile members
// ---------------------------------------------------------------------------

#[test]
fn hostile_members_decline_typed_without_panic() {
    let plain = b"expected plaintext".to_vec();
    let mut fx = Fixture::new(
        "hostile",
        &[
            Entry::bad_deflate("bad.bin", &plain),
            Entry::method("bzip.bin", b"unsupported method payload", 12),
        ],
    );
    assert_eq!(fx.report.member_count, 2);
    assert_eq!(fx.report.declined_decodes, 1); // method 12 has no decoded node

    // The exact raw bytes remain available for both members.
    let _ = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Member(0),
        Representation::EncodedBytes,
    );
    let _ = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Member(1),
        Representation::EncodedBytes,
    );

    // A bad DEFLATE payload is a typed error, not a panic.
    let class = observe_err(
        &mut fx.store,
        &fx.report.field,
        Selector::Member(0),
        Representation::DecodedBytes,
    );
    assert!(
        matches!(
            class,
            ErrorClass::ReconstructionMismatch | ErrorClass::Usage
        ),
        "unexpected class {class:?}"
    );

    // An unsupported method has no decoded entry: a typed decline.
    let class = observe_err(
        &mut fx.store,
        &fx.report.field,
        Selector::Member(1),
        Representation::DecodedBytes,
    );
    assert_eq!(class, ErrorClass::UnsupportedFeature);
}

// ---------------------------------------------------------------------------
// Bounded work
// ---------------------------------------------------------------------------

#[test]
fn tiny_member_query_does_not_decode_all_members() {
    let entries: Vec<Entry> = (0..8)
        .map(|i| {
            Entry::deflated(
                &format!("m{i}.bin"),
                format!("member {i} payload").as_bytes(),
            )
        })
        .collect();
    let mut fx = Fixture::new("bounded", &entries);
    assert_eq!(fx.report.decoded_nodes, 8);

    let (answer, stats) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Member(3),
        Representation::DecodedBytes,
    );
    assert_eq!(answer_bytes(&answer), b"member 3 payload");
    // Exactly the decoded node and its raw dependency are executed.
    assert!(
        stats.seed_nodes_executed <= 2,
        "one member decode executed {} seed nodes",
        stats.seed_nodes_executed
    );
    assert!(
        stats.seed_nodes_materialized <= 2,
        "one member decode materialized {} seed nodes",
        stats.seed_nodes_materialized
    );
    assert!(
        stats.seed_nodes_executed < fx.report.decoded_nodes,
        "a tiny query must not decode all {} members",
        fx.report.decoded_nodes
    );
}

// ---------------------------------------------------------------------------
// Persistence (cache reuse + cache clear + a genuinely new process)
// ---------------------------------------------------------------------------

/// Child entry point: with `VOLE_PKG_CHILD_ROOT` set, observe one member's decoded
/// bytes in this (fresh) process and print `MEMBER_HEX=<hex>`.
#[test]
fn child_observe_member_decoded() {
    let Some(root) = std::env::var_os("VOLE_PKG_CHILD_ROOT") else {
        return;
    };
    let field_hex = std::env::var("VOLE_PKG_CHILD_FIELD").expect("child field hex");
    let ordinal: u32 = std::env::var("VOLE_PKG_CHILD_ORDINAL")
        .expect("child ordinal")
        .parse()
        .unwrap();
    let mut store = FieldStore::open(PathBuf::from(root)).unwrap();
    let field = FieldId::from_hex(&field_hex).unwrap();
    let (answer, _stats) = observe_eq(
        &mut store,
        &field,
        Selector::Member(ordinal),
        Representation::DecodedBytes,
    );
    println!("MEMBER_HEX={}", hex(&answer_bytes(&answer)));
}

#[test]
fn member_decode_persists_and_survives_cache_clear_in_a_new_process() {
    let plaintext = b"persisted member payload".to_vec();
    let root = temp_dir("persist");
    let source = build_zip(&[
        Entry::stored("a.txt", b"unrelated"),
        Entry::deflated("b.bin", &plaintext),
    ]);
    let descriptor = opaque_descriptor(&source);

    let report;
    {
        let mut store = FieldStore::open(&root).unwrap();
        report = ingest_package(&mut store, &descriptor, Limits::DEFAULT).unwrap();

        // First decode fills the derived cache.
        let (first, _) = observe_eq(
            &mut store,
            &report.field,
            Selector::Member(1),
            Representation::DecodedBytes,
        );
        assert_eq!(answer_bytes(&first), plaintext);

        // Second decode is served whole from the cache: no re-execution.
        let (second, stats) = observe_eq(
            &mut store,
            &report.field,
            Selector::Member(1),
            Representation::DecodedBytes,
        );
        assert_eq!(answer_bytes(&second), plaintext);
        assert!(
            stats.seed_nodes_reused >= 1,
            "a warm query must reuse the cached decode: {stats:?}"
        );
        assert_eq!(
            stats.seed_nodes_executed, 0,
            "a warm query must not re-decode: {stats:?}"
        );
    }

    // `cache --clear` through a fresh handle forces a re-decode.
    DerivedCache::open(root.join("cache"))
        .unwrap()
        .clear()
        .unwrap();
    {
        let mut store = FieldStore::open(&root).unwrap();
        let (redecoded, stats) = observe_eq(
            &mut store,
            &report.field,
            Selector::Member(1),
            Representation::DecodedBytes,
        );
        assert_eq!(answer_bytes(&redecoded), plaintext);
        assert!(
            stats.seed_nodes_executed >= 1,
            "after cache --clear the decode must re-run: {stats:?}"
        );
    }

    // A **new process** clears nothing and re-decodes from the persisted field.
    DerivedCache::open(root.join("cache"))
        .unwrap()
        .clear()
        .unwrap();
    let exe = std::env::current_exe().unwrap();
    let out = Command::new(exe)
        .args(["--exact", "child_observe_member_decoded", "--nocapture"])
        .env("VOLE_PKG_CHILD_ROOT", &root)
        .env("VOLE_PKG_CHILD_FIELD", report.field.to_hex())
        .env("VOLE_PKG_CHILD_ORDINAL", "1")
        .output()
        .expect("spawn child test process");
    assert!(
        out.status.success(),
        "child process failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let expected = format!("MEMBER_HEX={}", hex(&plaintext));
    assert!(
        stdout.contains(&expected),
        "new process did not re-decode correctly; stdout:\n{stdout}"
    );

    std::fs::remove_dir_all(&root).ok();
}

// ---------------------------------------------------------------------------
// Plan / explain
// ---------------------------------------------------------------------------

#[test]
fn member_plan_and_explain_are_honest() {
    let mut fx = Fixture::new(
        "plan",
        &[
            Entry::stored("a.txt", b"hello"),
            Entry::deflated("b.bin", b"world, decoded"),
        ],
    );

    let raw_req = ObserveRequest::new(Selector::Member(0), Representation::EncodedBytes);
    let dec_req = ObserveRequest::new(Selector::Member(0), Representation::DecodedBytes);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let raw_plan = plan(field.manifest(), &fx.store, &raw_req).unwrap();
    assert_eq!(raw_plan.shape.name(), "index_lookup");
    assert!(
        raw_plan
            .will_materialize
            .iter()
            .any(|s| s == "PackageMemberRaw")
    );
    let dec_plan = plan(field.manifest(), &fx.store, &dec_req).unwrap();
    assert_eq!(dec_plan.shape.name(), "node_materialize");
    assert!(
        dec_plan
            .will_materialize
            .iter()
            .any(|s| s == "PackageMemberDecoded")
    );
    drop(field);

    let (plan_out, actual) =
        explain_analyze(&mut fx.store, &fx.report.field, &dec_req, Limits::DEFAULT).unwrap();
    assert_eq!(plan_out.plan.shape.name(), "node_materialize");
    assert_eq!(actual.answer_basis.name(), "deterministically-derived");
    assert!(!actual.exact);
    assert_eq!(actual.stats.descriptor_read_mode.name(), "partial");
    let json = actual.to_json();
    // The frozen actual-key set is unchanged by Phase 12.2.
    for key in [
        "\"basis\":\"deterministically-derived\"",
        "\"bytes_returned\":5",
        "\"descriptor_read_mode\":\"partial\"",
    ] {
        assert!(json.contains(key), "explain json missing {key}: {json}");
    }
}

// ---------------------------------------------------------------------------
// EntropyFS: the same field through the SeedStore abstraction
// ---------------------------------------------------------------------------

#[cfg(feature = "entropyfs-store")]
#[test]
fn package_field_through_entropyfs_engine() {
    let root = temp_dir("entropyfs");
    let payload = b"entropyfs-backed deflated member".to_vec();
    let entries = [
        Entry::stored("a.txt", b"stored"),
        Entry::deflated("b.bin", &payload),
    ];
    let source = build_zip(&entries);
    let descriptor = opaque_descriptor(&source);

    let mut store = FieldStore::open_entropyfs(&root).unwrap();
    let report = ingest_package(&mut store, &descriptor, Limits::DEFAULT).unwrap();
    let field = Field::open(&store, &report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), source);
    drop(field);

    let (answer, _stats) = observe_eq(
        &mut store,
        &report.field,
        Selector::Member(1),
        Representation::DecodedBytes,
    );
    assert_eq!(answer_bytes(&answer), payload);
    std::fs::remove_dir_all(&root).ok();
}
