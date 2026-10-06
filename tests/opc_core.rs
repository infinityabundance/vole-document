//! Phase 12.3 court: the generic OPC (Open Packaging Conventions) core.
//!
//! Gated on `field` + `package` + `opc` so `--no-default-features` (and the
//! default build without `opc`) compiles an empty target.
//!
//! Predeclared gates exercised here:
//!
//! 1. **Content types** — `Default`/`Override` precedence and extension matching.
//! 2. **Relationships** — package `_rels/.rels` and part-level `*.rels`, with
//!    internal targets resolved against the source part's base URI (RFC 3986).
//! 3. **External targets are inert** — never resolved, never fetched; asking for
//!    their bytes is a typed decline.
//! 4. **Discovery/fail-closed** — missing/ambiguous/duplicate identity and
//!    unknown ids are typed errors, never last-wins guesses.
//! 5. **Security** — malformed XML, DOCTYPE, non-UTF-8, and unbounded depth are
//!    typed declines with no panic; a traversal member name is clamped.
//! 6. **Exactness preserved** — the underlying package still materializes
//!    `materialize(field) == original_bytes` after any OPC decline.

#![cfg(all(feature = "field", feature = "package", feature = "opc"))]

use std::path::PathBuf;

use vole_document::adapter::package::scan;
use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::ingest_package::{PackageIngestReport, ingest_package};
use vole_document::field::observe::{
    ObserveRequest, ObserveStats, Representation, Selector, observe,
};
use vole_document::field::provenance::{AnswerValue, Basis, FieldAnswer};
use vole_document::field::{Field, FieldStore};
use vole_document::limits::Limits;

// ---------------------------------------------------------------------------
// Dependency-free ZIP writer (test ground truth), mirroring the 12.2 court.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Entry {
    name: Vec<u8>,
    content: Vec<u8>,
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
        crcs.push(vole_document::adapter::package::crc32_iso_hdlc(&e.content));
        let flags = e.flags | UTF8_FLAG;
        put_u32(&mut out, 0x0403_4b50);
        put_u16(&mut out, 20);
        put_u16(&mut out, flags);
        put_u16(&mut out, e.method);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u32(&mut out, *crcs.last().unwrap());
        put_u32(&mut out, e.compressed.len() as u32);
        put_u32(&mut out, e.content.len() as u32);
        put_u16(&mut out, e.name.len() as u16);
        put_u16(&mut out, 0);
        out.extend_from_slice(&e.name);
        out.extend_from_slice(&e.compressed);
    }

    let cd_start = out.len() as u32;
    for (i, e) in entries.iter().enumerate() {
        let flags = e.flags | UTF8_FLAG;
        put_u32(&mut out, 0x0201_4b50);
        put_u16(&mut out, 20);
        put_u16(&mut out, 20);
        put_u16(&mut out, flags);
        put_u16(&mut out, e.method);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u32(&mut out, crcs[i]);
        put_u32(&mut out, e.compressed.len() as u32);
        put_u32(&mut out, e.content.len() as u32);
        put_u16(&mut out, e.name.len() as u16);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u32(&mut out, 0);
        put_u32(&mut out, offsets[i]);
        out.extend_from_slice(&e.name);
    }
    let cd_end = out.len() as u32;

    put_u32(&mut out, 0x0605_4b50);
    put_u16(&mut out, 0);
    put_u16(&mut out, 0);
    put_u16(&mut out, entries.len() as u16);
    put_u16(&mut out, entries.len() as u16);
    put_u32(&mut out, cd_end - cd_start);
    put_u32(&mut out, cd_start);
    put_u16(&mut out, 0);
    out
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const CONTENT_TYPES: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
    r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
    r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>"#,
    r#"<Default Extension="xml" ContentType="application/xml"/>"#,
    r#"<Default Extension="png" ContentType="image/png"/>"#,
    r#"<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>"#,
    r#"</Types>"#
);

const PACKAGE_RELS: &str = concat!(
    r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>"#,
    r#"<Relationship Id="rIdExt" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com/x" TargetMode="External"/>"#,
    r#"</Relationships>"#
);

const PART_RELS: &str = concat!(
    r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    r#"<Relationship Id="rIdImg" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.png"/>"#,
    r#"</Relationships>"#
);

const DOCUMENT_XML: &str =
    r#"<?xml version="1.0"?><document xmlns="x"><body>hello</body></document>"#;

fn opc_entries() -> Vec<Entry> {
    vec![
        Entry::stored("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        Entry::stored("_rels/.rels", PACKAGE_RELS.as_bytes()),
        Entry::deflated("word/document.xml", DOCUMENT_XML.as_bytes()),
        Entry::stored("word/_rels/document.xml.rels", PART_RELS.as_bytes()),
        Entry::stored("media/image1.png", &[0x89, b'P', b'N', b'G', 0x00, 0xFF]),
    ]
}

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-opc-{label}-{}-{}",
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
        format_basis: "opaque:opc-test".to_string(),
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
    fn new(label: &str, entries: &[Entry], limits: Limits) -> Fixture {
        let root = temp_dir(label);
        let mut store = FieldStore::open(&root).unwrap();
        let source = build_zip(entries);
        let descriptor = opaque_descriptor(&source);
        let report = ingest_package(&mut store, &descriptor, limits).unwrap();
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
    field: &vole_document::field::FieldId,
    selector: Selector,
    representation: Representation,
) -> (FieldAnswer, ObserveStats) {
    let req = ObserveRequest::new(selector, representation);
    let (answer, stats, _) = observe(store, field, &req, Limits::DEFAULT).unwrap();
    (answer, stats)
}

fn observe_err(
    store: &mut FieldStore,
    field: &vole_document::field::FieldId,
    selector: Selector,
    representation: Representation,
    limits: Limits,
) -> ErrorClass {
    let req = ObserveRequest::new(selector, representation);
    observe(store, field, &req, limits).unwrap_err().class()
}

fn answer_bytes(answer: &FieldAnswer) -> Vec<u8> {
    match &answer.value {
        AnswerValue::Bytes(b) => b.clone(),
        other => panic!("expected bytes, got {other:?}"),
    }
}

fn answer_json(answer: &FieldAnswer) -> String {
    match &answer.value {
        AnswerValue::Json(j) => j.clone(),
        other => panic!("expected json, got {other:?}"),
    }
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

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn opc_parts_relationships_and_exactness() {
    let mut fx = Fixture::new("core", &opc_entries(), Limits::DEFAULT);
    assert_eq!(fx.report.member_count, 5);
    assert_eq!(fx.report.opc_model_nodes, 1);

    // Exactness unchanged by the OPC layer.
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), fx.source.len());
    assert_eq!(exact, fx.source);
    drop(field);

    // A part's exact bytes are its member raw span (Q_ref), found by part name.
    let (doc_raw, stats) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::PackagePart("/word/document.xml".to_string()),
        Representation::ExactBytes,
    );
    assert_eq!(answer_bytes(&doc_raw), member_raw(&fx.source, 2));
    assert_eq!(doc_raw.basis, Basis::DirectlyObserved);
    assert!(doc_raw.exact);
    assert_eq!(doc_raw.source_span, {
        let physical = scan(&fx.source, Limits::DEFAULT).unwrap();
        let (off, len) = physical.members[2].data;
        Some((off, off + len))
    });
    assert!(stats.seed_nodes_fetched > 0);

    // Decoded bytes are the known plaintext (deflated member).
    let (doc_dec, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::PackagePart("/word/document.xml".to_string()),
        Representation::DecodedBytes,
    );
    assert_eq!(answer_bytes(&doc_dec), DOCUMENT_XML.as_bytes());
    assert_eq!(doc_dec.basis, Basis::DeterministicallyDerived);
    assert!(!doc_dec.exact);

    // Part metadata reports the resolved content type + ordinal.
    let (meta, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::PackagePart("/WORD/DOCUMENT.XML".to_string()),
        Representation::Metadata,
    );
    let json = answer_json(&meta);
    assert!(json.contains("/word/document.xml"), "{json}");
    assert!(json.contains("\"ordinal\":2"), "{json}");
    assert!(
        json.contains("wordprocessingml.document.main+xml"),
        "{json}"
    );
    assert!(json.contains("\"relationships\":1"), "{json}");
}

#[test]
fn relationship_resolution_and_external_inertness() {
    let mut fx = Fixture::new("rels", &opc_entries(), Limits::DEFAULT);

    // Package relationship resolves against `/`.
    let (r1, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Relationship("rId1".to_string()),
        Representation::Metadata,
    );
    let json = answer_json(&r1);
    assert!(
        json.contains("\"resolved\":\"/word/document.xml\""),
        "{json}"
    );
    assert!(json.contains("\"target_mode\":\"internal\""), "{json}");
    assert!(json.contains("\"owner\":null"), "{json}");

    // Part relationship resolves against the source part's base URI (`/word/`).
    let (rimg, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Relationship("rIdImg".to_string()),
        Representation::Metadata,
    );
    let json = answer_json(&rimg);
    assert!(
        json.contains("\"resolved\":\"/media/image1.png\""),
        "{json}"
    );
    assert!(json.contains("\"owner\":2"), "{json}");

    // An internal relationship's bytes resolve to the target part's raw span.
    let (bytes, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Relationship("rIdImg".to_string()),
        Representation::ExactBytes,
    );
    assert_eq!(answer_bytes(&bytes), member_raw(&fx.source, 4));

    // External target: metadata is honest, bytes are a typed decline (never fetched).
    let (ext, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Relationship("rIdExt".to_string()),
        Representation::Metadata,
    );
    let json = answer_json(&ext);
    assert!(json.contains("\"target_mode\":\"external\""), "{json}");
    assert!(json.contains("\"resolved\":null"), "{json}");
    let class = observe_err(
        &mut fx.store,
        &fx.report.field,
        Selector::Relationship("rIdExt".to_string()),
        Representation::ExactBytes,
        Limits::DEFAULT,
    );
    assert_eq!(class, ErrorClass::InvalidPackageStructure);

    // Unknown ids and parts fail closed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &fx.report.field,
            Selector::Relationship("nope".to_string()),
            Representation::Metadata,
            Limits::DEFAULT,
        ),
        ErrorClass::InvalidPackageStructure
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &fx.report.field,
            Selector::PackagePart("/does/not/exist.xml".to_string()),
            Representation::Metadata,
            Limits::DEFAULT,
        ),
        ErrorClass::InvalidPackageStructure
    );
}

#[test]
fn plan_and_explain_are_honest_for_native_selectors() {
    use vole_document::field::explain::explain_analyze;
    let mut fx = Fixture::new("plan", &opc_entries(), Limits::DEFAULT);
    let req = ObserveRequest::new(
        Selector::PackagePart("/word/document.xml".to_string()),
        Representation::DecodedBytes,
    );
    let (plan, actual) =
        explain_analyze(&mut fx.store, &fx.report.field, &req, Limits::DEFAULT).unwrap();
    assert_eq!(plan.plan.shape.name(), "deepen_then_observe");
    assert!(
        plan.plan
            .will_materialize
            .iter()
            .any(|k| k == "PackageOpcModel")
    );
    assert!(
        plan.plan
            .will_materialize
            .iter()
            .any(|k| k == "PackageMemberDecoded")
    );
    // Frozen plan JSON key set is unchanged by 12.3 (only values differ).
    let mut keys = Vec::new();
    for k in [
        "selector",
        "representation",
        "shape",
        "index_reads",
        "required_nodes",
        "will_materialize",
        "will_not_materialize",
    ] {
        assert!(
            plan.json.contains(&format!("\"{k}\":")),
            "{k} missing: {}",
            plan.json
        );
        keys.push(k);
    }
    assert_eq!(keys.len(), 7);
    assert_eq!(actual.answer_basis, Basis::DeterministicallyDerived);
    assert!(!actual.exact);
}

#[test]
fn malformed_opc_declines_typed_and_keeps_exactness() {
    // Malformed content types XML.
    let mut entries = opc_entries();
    entries[0] = Entry::stored("[Content_Types].xml", b"<Types><Default");
    let mut fx = Fixture::new("malformed", &entries, Limits::DEFAULT);
    assert_eq!(
        observe_err(
            &mut fx.store,
            &fx.report.field,
            Selector::PackagePart("/word/document.xml".to_string()),
            Representation::Metadata,
            Limits::DEFAULT,
        ),
        ErrorClass::InvalidXmlStructure
    );
    // Exactness still holds after the semantic decline.
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn doctype_and_entities_are_refused() {
    let evil = concat!(
        r#"<?xml version="1.0"?>"#,
        r#"<!DOCTYPE Types [<!ENTITY xxe SYSTEM "file:///etc/passwd">]>"#,
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
        r#"<Default Extension="xml" ContentType="&xxe;"/></Types>"#
    );
    let mut entries = opc_entries();
    entries[0] = Entry::stored("[Content_Types].xml", evil.as_bytes());
    let mut fx = Fixture::new("doctype", &entries, Limits::DEFAULT);
    assert_eq!(
        observe_err(
            &mut fx.store,
            &fx.report.field,
            Selector::PackagePart("/word/document.xml".to_string()),
            Representation::Metadata,
            Limits::DEFAULT,
        ),
        ErrorClass::InvalidXmlStructure
    );
}

#[test]
fn non_utf8_and_deep_xml_decline() {
    // Non-UTF-8 content-types bytes.
    let mut entries = opc_entries();
    entries[0] = Entry::stored("[Content_Types].xml", &[b'<', 0xC3, 0x28, b'>']);
    let mut fx = Fixture::new("nonutf8", &entries, Limits::DEFAULT);
    assert_eq!(
        observe_err(
            &mut fx.store,
            &fx.report.field,
            Selector::PackagePart("/word/document.xml".to_string()),
            Representation::Metadata,
            Limits::DEFAULT,
        ),
        ErrorClass::InvalidXmlStructure
    );

    // Deep nesting, bounded by `max_xml_depth`.
    let deep = format!(
        r#"<Types xmlns="x">{}{}</Types>"#,
        "<a>".repeat(64),
        "</a>".repeat(64)
    );
    let mut entries2 = opc_entries();
    entries2[0] = Entry::stored("[Content_Types].xml", deep.as_bytes());
    let tight = Limits {
        max_xml_depth: 4,
        ..Limits::DEFAULT
    };
    let mut fx2 = Fixture::new("deep", &entries2, tight);
    assert_eq!(
        observe_err(
            &mut fx2.store,
            &fx2.report.field,
            Selector::PackagePart("/word/document.xml".to_string()),
            Representation::Metadata,
            tight,
        ),
        ErrorClass::InvalidXmlStructure
    );
}

#[test]
fn ambiguous_identity_and_traversal_clamp() {
    // Two case-equivalent part names make OPC identity ambiguous → typed decline.
    let mut entries = opc_entries();
    entries.push(Entry::stored("word/DOCUMENT.XML", b"dupe"));
    let mut fx = Fixture::new("ambiguous", &entries, Limits::DEFAULT);
    assert_eq!(
        observe_err(
            &mut fx.store,
            &fx.report.field,
            Selector::PackagePart("/word/document.xml".to_string()),
            Representation::Metadata,
            Limits::DEFAULT,
        ),
        ErrorClass::InvalidPackageStructure
    );
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);

    // A traversal member name is clamped (rejected) at the OPC boundary.
    let mut entries2 = opc_entries();
    entries2.push(Entry::stored("../evil.xml", b"x"));
    let mut fx2 = Fixture::new("traversal", &entries2, Limits::DEFAULT);
    assert_eq!(
        observe_err(
            &mut fx2.store,
            &fx2.report.field,
            Selector::PackagePart("/word/document.xml".to_string()),
            Representation::Metadata,
            Limits::DEFAULT,
        ),
        ErrorClass::InvalidPackageStructure
    );
}

#[test]
fn duplicate_relationship_ids_decline() {
    // The same id in two different `.rels` parts is ambiguous.
    let a = concat!(
        r#"<Relationships xmlns="x">"#,
        r#"<Relationship Id="dup" Type="t/a" Target="b.xml"/>"#,
        r#"</Relationships>"#
    );
    let b = concat!(
        r#"<Relationships xmlns="x">"#,
        r#"<Relationship Id="dup" Type="t/b" Target="a.xml"/>"#,
        r#"</Relationships>"#
    );
    let entries = vec![
        Entry::stored("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        Entry::stored("_rels/.rels", PACKAGE_RELS.as_bytes()),
        Entry::stored("a.xml", b"<a/>"),
        Entry::stored("b.xml", b"<b/>"),
        Entry::stored("_rels/a.xml.rels", a.as_bytes()),
        Entry::stored("_rels/b.xml.rels", b.as_bytes()),
    ];
    let mut fx = Fixture::new("dupids", &entries, Limits::DEFAULT);
    assert_eq!(
        observe_err(
            &mut fx.store,
            &fx.report.field,
            Selector::Relationship("dup".to_string()),
            Representation::Metadata,
            Limits::DEFAULT,
        ),
        ErrorClass::InvalidPackageStructure
    );
}
