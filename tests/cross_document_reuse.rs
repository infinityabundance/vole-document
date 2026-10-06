//! Phase 12.8 court: cross-document procedural state under exact content identity.
//!
//! Two documents of **different formats** (a DOCX and an EPUB) that embed the
//! byte-identical resource resolve to **one** content-addressed
//! [`ResourceBlob`](vole_document::field::node::NodeKind::ResourceBlob) — one
//! node id, one persisted blob — so the second ingest writes strictly fewer seed
//! bytes and the shared decoded state is reused across a document boundary. A
//! no-sharing control (a structurally identical document embedding *different*
//! bytes) shares nothing.
//!
//! Sharing is scoped to the formats that actually extract embedded resources:
//! the package adapters (DOCX/EPUB). The Phase-11 PDF adapter does **not**
//! extract embedded images as resource blobs, so a PDF carrying the same bytes
//! joins the store but shares no resource node (asserted as a reported zero);
//! PDF↔DOCX resource sharing is therefore out of scope in Phase 12, and a
//! genuine cross-format share is demonstrated DOCX↔EPUB instead.
//!
//! Exactness is asserted for every root: `materialize_exact(root) == source`.
//! The whole file is gated on the features it needs so `--no-default-features`
//! builds an empty, compiling target.

#![cfg(all(
    feature = "field",
    feature = "package",
    feature = "docx",
    feature = "epub"
))]

use std::path::PathBuf;

use vole_document::adapter::package::{crc32_iso_hdlc, scan};
use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::field::cache::DerivedCache;
use vole_document::field::dag::{
    EvalBudget, InverseWork, load_node, materialize_node_cached, retained_inverse_work_fraction,
};
use vole_document::field::index::{FsIndexStore, SEL_PACKAGE_MEMBER_DECODED, SelectorKey, lookup};
use vole_document::field::ingest::ingest;
use vole_document::field::observe::{
    ObserveRequest, ObserveStats, Representation, Selector, observe,
};
use vole_document::field::provenance::AnswerValue;
use vole_document::field::resource::{is_shareable_resource, resource_blob_node};
use vole_document::field::{Field, FieldStore};
use vole_document::limits::Limits;

/// The resource embedded verbatim in the shared cohort.
const SHARED_PNG: &[u8] = b"\x89PNG\r\n\x1a\nSHARED-RESOURCE-PAYLOAD-0123456789";
/// A control resource of **identical length** but different bytes.
const CONTROL_PNG: &[u8] = b"\x89PNG\r\n\x1a\nCONTROL-RESOURCE-PAYLOAD-012345678";

// ---------------------------------------------------------------------------
// Dependency-free ZIP / PDF writers (test ground truth)
// ---------------------------------------------------------------------------

fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Build a ZIP with stored members (order preserved), UTF-8 flag set.
fn build_zip(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    const UTF8_FLAG: u16 = 0x0800;
    let mut out: Vec<u8> = Vec::new();
    let mut offsets: Vec<u32> = Vec::new();
    let mut crcs: Vec<u32> = Vec::new();
    for (name, content) in entries {
        offsets.push(out.len() as u32);
        crcs.push(crc32_iso_hdlc(content));
        put_u32(&mut out, 0x0403_4b50);
        put_u16(&mut out, 20);
        put_u16(&mut out, UTF8_FLAG);
        put_u16(&mut out, 0); // stored
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u32(&mut out, *crcs.last().unwrap());
        put_u32(&mut out, content.len() as u32);
        put_u32(&mut out, content.len() as u32);
        put_u16(&mut out, name.len() as u16);
        put_u16(&mut out, 0);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(content);
    }
    let cd_start = out.len() as u32;
    for (i, (name, content)) in entries.iter().enumerate() {
        put_u32(&mut out, 0x0201_4b50);
        put_u16(&mut out, 20);
        put_u16(&mut out, 20);
        put_u16(&mut out, UTF8_FLAG);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u32(&mut out, crcs[i]);
        put_u32(&mut out, content.len() as u32);
        put_u32(&mut out, content.len() as u32);
        put_u16(&mut out, name.len() as u16);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u32(&mut out, 0);
        put_u32(&mut out, offsets[i]);
        out.extend_from_slice(name.as_bytes());
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

/// A minimal OPC DOCX embedding `png` at `word/media/image1.png`.
fn build_docx(png: &[u8]) -> Vec<u8> {
    let content_types = br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_vec();
    let rels = br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_vec();
    let document = br#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Cross-document resource court</w:t></w:r></w:p></w:body></w:document>"#.to_vec();
    build_zip(&[
        ("[Content_Types].xml".to_string(), content_types),
        ("_rels/.rels".to_string(), rels),
        ("word/document.xml".to_string(), document),
        ("word/media/image1.png".to_string(), png.to_vec()),
    ])
}

/// A minimal OCF EPUB embedding `png` at `OEBPS/images/shared.png`.
fn build_epub(png: &[u8]) -> Vec<u8> {
    let container = br#"<?xml version="1.0" encoding="UTF-8"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/package.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#.to_vec();
    let opf = br#"<?xml version="1.0" encoding="UTF-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">urn:uuid:court</dc:identifier><dc:title>Resource Court</dc:title><dc:language>en</dc:language></metadata><manifest><item id="c1" href="chapter.xhtml" media-type="application/xhtml+xml"/><item id="img" href="images/shared.png" media-type="image/png"/></manifest><spine><itemref idref="c1"/></spine></package>"#.to_vec();
    let chapter = br#"<?xml version="1.0" encoding="UTF-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>c</title></head><body><p>cross-document resource</p></body></html>"#.to_vec();
    build_zip(&[
        ("mimetype".to_string(), b"application/epub+zip".to_vec()),
        ("META-INF/container.xml".to_string(), container),
        ("OEBPS/package.opf".to_string(), opf),
        ("OEBPS/chapter.xhtml".to_string(), chapter),
        ("OEBPS/images/shared.png".to_string(), png.to_vec()),
    ])
}

/// A classic-xref PDF whose object 5 is an **unfiltered** image XObject carrying
/// `png` verbatim. The PDF adapter extracts no resource blob from it, so this is
/// an exactness-only participant.
fn build_pdf(png: &[u8]) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    let mut offsets: Vec<(u64, u64)> = Vec::new();
    let obj = |buf: &mut Vec<u8>, offsets: &mut Vec<(u64, u64)>, n: u64, body: &[u8]| {
        offsets.push((n, buf.len() as u64));
        buf.extend_from_slice(format!("{n} 0 obj\n").as_bytes());
        buf.extend_from_slice(body);
    };
    buf.extend_from_slice(b"%PDF-1.5\n");
    obj(
        &mut buf,
        &mut offsets,
        1,
        b"<< /Type /Catalog /Pages 2 0 R >>",
    );
    buf.extend_from_slice(b"\nendobj\n");
    obj(
        &mut buf,
        &mut offsets,
        2,
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    );
    buf.extend_from_slice(b"\nendobj\n");
    obj(
        &mut buf,
        &mut offsets,
        3,
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /XObject << /Im0 5 0 R >> >> /Contents 4 0 R >>",
    );
    buf.extend_from_slice(b"\nendobj\n");
    // An unfiltered content stream (its bytes are not a resource signature).
    let content = b"q 1 0 0 1 0 0 cm /Im0 Do Q";
    obj(
        &mut buf,
        &mut offsets,
        4,
        format!("<< /Length {} >>\nstream\n", content.len()).as_bytes(),
    );
    buf.extend_from_slice(content);
    buf.extend_from_slice(b"\nendstream\nendobj\n");
    // The image XObject: uncompressed, so its encoded bytes are `png` exactly.
    obj(
        &mut buf,
        &mut offsets,
        5,
        format!(
            "<< /Type /XObject /Subtype /Image /Width 4 /Height 4 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length {} >>\nstream\n",
            png.len()
        )
        .as_bytes(),
    );
    buf.extend_from_slice(png);
    buf.extend_from_slice(b"\nendstream\nendobj\n");
    // Classic xref trailer.
    let xref = buf.len() as u64;
    buf.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
    for n in 1..6u64 {
        let off = offsets.iter().find(|&&(num, _)| num == n).unwrap().1;
        buf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    buf.extend_from_slice(
        format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
    );
    buf
}

fn opaque_descriptor(source: &[u8]) -> Vec<u8> {
    let d = Descriptor {
        universe: vole_document::container::UNIVERSE.to_string(),
        source_format: vole_document::SOURCE_FORMAT_OPAQUE,
        format_basis: "opaque:phase12-share".to_string(),
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

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-xdoc-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn answer_bytes(answer: &vole_document::field::provenance::FieldAnswer) -> Vec<u8> {
    match &answer.value {
        AnswerValue::Bytes(b) => b.clone(),
        other => panic!("expected bytes, got {other:?}"),
    }
}

/// The content id of the shared resource blob, from the library itself.
fn shared_blob_id() -> vole_document::store::NodeId {
    resource_blob_node(SHARED_PNG).content_id()
}

// ---------------------------------------------------------------------------
// Cross-document sharing
// ---------------------------------------------------------------------------

#[test]
fn cross_document_shared_resource_is_one_content_addressed_blob() {
    let limits = Limits::DEFAULT;
    let shared_dir = temp_dir("identity-shared");
    let control_dir = temp_dir("identity-control");

    let docx_src = build_docx(SHARED_PNG);
    let epub_src = build_epub(SHARED_PNG);
    let control_src = build_epub(CONTROL_PNG);
    // A PDF carrying the same bytes verbatim participates in exactness only: the
    // Phase-11 PDF adapter extracts no embedded resource blob.
    let pdf_src = build_pdf(SHARED_PNG);

    assert!(is_shareable_resource(SHARED_PNG));
    assert!(is_shareable_resource(CONTROL_PNG));

    // Store A: a DOCX and an EPUB embedding the *same* resource bytes. DOCX first:
    // it pays the blob's byte cost; the EPUB then shares it by content id.
    let mut store = FieldStore::open(&shared_dir).unwrap();
    let docx = match ingest(&mut store, &opaque_descriptor(&docx_src), limits).unwrap() {
        vole_document::field::ingest::IngestOutcome::Package(r) => r,
        other => panic!("expected package, got {other:?}"),
    };
    let epub = match ingest(&mut store, &opaque_descriptor(&epub_src), limits).unwrap() {
        vole_document::field::ingest::IngestOutcome::Package(r) => r,
        other => panic!("expected package, got {other:?}"),
    };
    let pdf = match ingest(&mut store, &opaque_descriptor(&pdf_src), limits).unwrap() {
        vole_document::field::ingest::IngestOutcome::Pdf(r) => r,
        other => panic!("expected PDF, got {other:?}"),
    };

    // Store B: the same DOCX, and a structurally identical EPUB whose only
    // difference is the resource bytes (the no-sharing control).
    let mut control_store = FieldStore::open(&control_dir).unwrap();
    let _control_docx =
        match ingest(&mut control_store, &opaque_descriptor(&docx_src), limits).unwrap() {
            vole_document::field::ingest::IngestOutcome::Package(r) => r,
            other => panic!("expected package, got {other:?}"),
        };
    let control =
        match ingest(&mut control_store, &opaque_descriptor(&control_src), limits).unwrap() {
            vole_document::field::ingest::IngestOutcome::Package(r) => r,
            other => panic!("expected package, got {other:?}"),
        };

    // One content id, one physically stored blob, shared across the DOCX/EPUB
    // boundary by exact byte identity (not by format or member name).
    let blob = shared_blob_id();
    assert!(
        store.seeds().contains_node(&blob).unwrap(),
        "the shared resource blob must be in the store"
    );

    // The DOCX writes the blob; the EPUB shares it by content id and writes zero
    // bytes for it, and shares the content-addressed decoded member node too.
    assert_eq!(
        docx.resource_blob_nodes, 1,
        "DOCX registers one blob: {docx:?}"
    );
    assert_eq!(
        docx.shared_resource_ids, 0,
        "DOCX pays for the blob: {docx:?}"
    );
    assert_eq!(docx.shared_resource_bytes, 0, "DOCX pays: {docx:?}");
    assert_eq!(
        epub.resource_blob_nodes, 1,
        "EPUB registers one blob: {epub:?}"
    );
    assert_eq!(
        epub.shared_resource_ids, 1,
        "EPUB must share the DOCX's resource blob: {epub:?}"
    );
    assert_eq!(
        epub.shared_resource_bytes,
        SHARED_PNG.len() as u64,
        "EPUB deduplicates exactly the resource bytes: {epub:?}"
    );
    // Two id-shared nodes: the resource blob and the decoded member for it.
    assert_eq!(
        epub.nodes_id_shared, 2,
        "EPUB shares blob + decoded: {epub:?}"
    );

    // The no-sharing control (different bytes) shares no resource, and deduplicated
    // zero resource bytes.
    assert_eq!(control.resource_blob_nodes, 1, "control: {control:?}");
    assert_eq!(
        control.shared_resource_ids, 0,
        "control shares no resource: {control:?}"
    );
    assert_eq!(control.shared_resource_bytes, 0, "control: {control:?}");
    // The control's only difference from the sharing EPUB is the resource bytes;
    // in its own store it shares no node id at all (a true zero-reuse control).
    assert_eq!(
        control.nodes_id_shared, 0,
        "control shares no node id: {control:?}"
    );

    // Same container structure and same DOCX baseline in both stores; the only
    // difference is the resource bytes, so the sharing EPUB writes strictly fewer
    // seed bytes than the control.
    assert!(
        epub.seed_bytes_written < control.seed_bytes_written,
        "shared EPUB seed bytes {} must be < control {}",
        epub.seed_bytes_written,
        control.seed_bytes_written
    );

    // The PDF adapter extracts no embedded resource blob (Phase-11 limitation),
    // so PDF↔DOCX resource sharing does not exist and is a reported zero.
    assert_eq!(
        pdf.resource_blob_nodes, 0,
        "PDF extracts no resource: {pdf:?}"
    );
    assert_eq!(
        pdf.shared_resource_ids, 0,
        "PDF shares no resource: {pdf:?}"
    );

    // Exactness holds for every root, independent of any sharing.
    for (s, id, src) in [
        (&store, docx.field, docx_src.as_slice()),
        (&store, epub.field, epub_src.as_slice()),
        (&store, pdf.field, pdf_src.as_slice()),
        (&control_store, control.field, control_src.as_slice()),
    ] {
        let field = Field::open(s, &id, limits).unwrap();
        assert_eq!(field.materialize_exact(limits).unwrap(), src);
    }

    std::fs::remove_dir_all(&shared_dir).ok();
    std::fs::remove_dir_all(&control_dir).ok();
}

#[test]
fn shared_decoded_state_is_reused_across_documents() {
    let dir = temp_dir("reuse");
    let mut store = FieldStore::open(&dir).unwrap();
    let limits = Limits::DEFAULT;

    let docx_src = build_docx(SHARED_PNG);
    let epub_src = build_epub(SHARED_PNG);

    let docx = match ingest(&mut store, &opaque_descriptor(&docx_src), limits).unwrap() {
        vole_document::field::ingest::IngestOutcome::Package(r) => r,
        other => panic!("expected package, got {other:?}"),
    };
    let epub = match ingest(&mut store, &opaque_descriptor(&epub_src), limits).unwrap() {
        vole_document::field::ingest::IngestOutcome::Package(r) => r,
        other => panic!("expected package, got {other:?}"),
    };

    // The DOCX and EPUB decoded-member nodes for the resource are the *same* node:
    // content identity, not per-source coordinates.
    let docx_png_ordinal = png_ordinal(&docx_src);
    let docx_decoded = decoded_node_id(&store, &docx.field, docx_png_ordinal);
    let epub_png_ordinal = png_ordinal(&epub_src);
    let epub_decoded = decoded_node_id(&store, &epub.field, epub_png_ordinal);
    assert_eq!(
        docx_decoded, epub_decoded,
        "the shared resource must decode to one content-addressed node"
    );

    // Materialize the shared node through the *DOCX* field, warming the persisted
    // derived cache for that content id.
    warm_shared_node(&store, &docx.field, &docx_decoded, limits);

    // Observe the EPUB's resource decoded bytes: the cold run re-executes, the warm
    // run is served from the state the DOCX materialization persisted.
    let selector = Selector::EpubResource("OEBPS/images/shared.png".to_string());
    let cold = observe_stats(&mut store, &epub.field, &selector, false, limits);
    assert_eq!(
        cold.seed_nodes_reused, 0,
        "a cold run cannot reuse: {cold:?}"
    );
    assert!(cold.seed_nodes_executed > 0, "cold run executes: {cold:?}");

    let (warm_answer, warm) = observe_answer(&mut store, &epub.field, &selector, true, limits);
    assert_eq!(
        answer_bytes(&warm_answer),
        SHARED_PNG,
        "shared bytes must be exact"
    );
    assert!(
        warm.seed_nodes_reused >= 1,
        "the shared decoded state must be reused across documents: {warm:?}"
    );

    // The retained-inverse-work fraction, from receipted integers only.
    let cold_work = InverseWork::new(cold.seed_nodes_executed, cold.bytes_read);
    let warm_work = InverseWork::new(warm.seed_nodes_executed, warm.bytes_read);
    let fraction = retained_inverse_work_fraction(cold_work, warm_work);
    assert!(
        fraction > 0.0 && fraction <= 1.0,
        "fraction {fraction} must be in (0,1]; cold={cold_work:?} warm={warm_work:?}"
    );
    // Consistency: the fraction is exactly the receipted reuse over cold work.
    let expected = warm_work_reused(cold_work, warm_work) as f64 / cold_work.units() as f64;
    assert!((fraction - expected).abs() < 1e-12);

    // `nodes_id_shared` (representation) is reported and distinct from
    // `nodes_reused` (work): it comes from the manifest, not this observation.
    assert!(
        warm.nodes_id_shared >= 1,
        "ingest recorded content-id sharing: {warm:?}"
    );

    // Exactness for both roots after all sharing and reuse.
    for (id, src) in [
        (docx.field, docx_src.as_slice()),
        (epub.field, epub_src.as_slice()),
    ] {
        let field = Field::open(&store, &id, limits).unwrap();
        assert_eq!(field.materialize_exact(limits).unwrap(), src);
    }

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn different_resource_bytes_never_share() {
    let dir = temp_dir("control");
    let mut store = FieldStore::open(&dir).unwrap();
    let limits = Limits::DEFAULT;

    let a = build_epub(SHARED_PNG);
    let b = build_epub(CONTROL_PNG);
    let ra = match ingest(&mut store, &opaque_descriptor(&a), limits).unwrap() {
        vole_document::field::ingest::IngestOutcome::Package(r) => r,
        other => panic!("expected package, got {other:?}"),
    };
    let rb = match ingest(&mut store, &opaque_descriptor(&b), limits).unwrap() {
        vole_document::field::ingest::IngestOutcome::Package(r) => r,
        other => panic!("expected package, got {other:?}"),
    };

    assert_eq!(ra.shared_resource_ids, 0);
    assert_eq!(rb.shared_resource_ids, 0, "different bytes must not share");
    assert_eq!(rb.shared_resource_bytes, 0);
    // The two decoded-member nodes are distinct content ids.
    assert_ne!(
        decoded_node_id(&store, &ra.field, png_ordinal(&a)),
        decoded_node_id(&store, &rb.field, png_ordinal(&b)),
    );

    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn png_ordinal(source: &[u8]) -> u32 {
    let physical = scan(source, Limits::DEFAULT).unwrap();
    physical
        .members
        .iter()
        .find(|m| m.name.ends_with(b".png"))
        .expect("a .png member")
        .id
        .ordinal
}

fn decoded_node_id(
    store: &FieldStore,
    field: &vole_document::field::FieldId,
    ordinal: u32,
) -> vole_document::store::NodeId {
    let manifest = store.get_field(field).unwrap();
    let istore = FsIndexStore::open(store.root()).unwrap();
    let root = vole_document::store::NodeId::from_bytes(manifest.index_root);
    lookup(
        &istore,
        &root,
        &SelectorKey::new(SEL_PACKAGE_MEMBER_DECODED, ordinal),
    )
    .unwrap()
    .first()
    .expect("decoded member entry")
    .node_id
}

/// Materialize a node through a field's source and the persisted derived cache,
/// so the shared content id is warm for a later observation of another document.
fn warm_shared_node(
    store: &FieldStore,
    field: &vole_document::field::FieldId,
    id: &vole_document::store::NodeId,
    limits: Limits,
) {
    let opened = Field::open(store, field, limits).unwrap();
    let node = load_node(store.seeds(), id).unwrap();
    let mut cache = DerivedCache::open(store.root().join("cache")).unwrap();
    let mut budget = EvalBudget::default();
    let mut reuse = vole_document::field::dag::ReuseStats::default();
    let out = materialize_node_cached(
        opened.parsed(),
        store.seeds(),
        &mut cache,
        &node,
        limits,
        &mut budget,
        node.limits.max_depth,
        &mut reuse,
    )
    .unwrap();
    assert_eq!(out, SHARED_PNG);
}

fn observe_answer(
    store: &mut FieldStore,
    field: &vole_document::field::FieldId,
    selector: &Selector,
    use_cache: bool,
    limits: Limits,
) -> (vole_document::field::provenance::FieldAnswer, ObserveStats) {
    let mut req = ObserveRequest::new(selector.clone(), Representation::DecodedBytes);
    req.use_cache = use_cache;
    let (answer, stats, _) = observe(store, field, &req, limits).unwrap();
    (answer, stats)
}

fn observe_stats(
    store: &mut FieldStore,
    field: &vole_document::field::FieldId,
    selector: &Selector,
    use_cache: bool,
    limits: Limits,
) -> ObserveStats {
    observe_answer(store, field, selector, use_cache, limits).1
}

fn warm_work_reused(cold: InverseWork, warm: InverseWork) -> u64 {
    cold.node_executions
        .saturating_sub(warm.node_executions)
        .saturating_add(cold.input_bytes.saturating_sub(warm.input_bytes))
}
