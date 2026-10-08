//! Phase 17: the direct source → field path equals the searched path.
//!
//! This is the *ablation* court for the direct ingest path: it builds the same
//! PDF source twice — once through the ordinary portfolio `encode` + `ingest`,
//! once through [`vole_document::field::build`] with the fixed `runtime` profile
//! — and requires (a) both to materialize the source byte-exactly, (b) the
//! recovered structure to be identical (same root and index node ids), and (c)
//! every observation in a mixed schedule to answer with the same value (or the
//! same typed decline). The whole file is gated on the `field` feature so
//! `--no-default-features` builds an empty (compiling) test target.

#![cfg(feature = "field")]

use std::path::PathBuf;

use vole_document::field::build::{BuildProfile, DirectIngest, build_field};
use vole_document::field::ingest as field_ingest;
use vole_document::field::observe::{ObserveRequest, Representation, Selector, observe};
use vole_document::field::provenance::AnswerValue;
use vole_document::field::{Field, FieldStore};
use vole_document::limits::Limits;

#[cfg(feature = "package")]
use vole_document::adapter::package::crc32_iso_hdlc;
#[cfg(feature = "package")]
use vole_document::field::ingest::IngestOutcome;

fn temp_root(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-phase17-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let chunks: Vec<&[u8]> = data.chunks(0xFFFF).collect();
    for (i, chunk) in chunks.iter().enumerate() {
        out.push(u8::from(i + 1 == chunks.len()));
        let len = chunk.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(chunk);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

struct PdfBuilder {
    buf: Vec<u8>,
    offsets: Vec<(u64, u64)>,
}

impl PdfBuilder {
    fn new() -> Self {
        PdfBuilder {
            buf: Vec::new(),
            offsets: Vec::new(),
        }
    }
    fn text(&mut self, s: &str) {
        self.buf.extend_from_slice(s.as_bytes());
    }
    fn raw(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }
    fn obj(&mut self, number: u64, body: &[u8]) {
        self.offsets.push((number, self.buf.len() as u64));
        self.text(&format!("{number} 0 obj\n"));
        self.raw(body);
        self.text("\nendobj\n");
    }
    fn stream_obj(&mut self, number: u64, extra: &str, data: &[u8]) {
        self.offsets.push((number, self.buf.len() as u64));
        self.text(&format!(
            "{number} 0 obj\n<< /Length {}{extra} >>\nstream\n",
            data.len()
        ));
        self.raw(data);
        self.text("\nendstream\nendobj\n");
    }
    fn offset_of(&self, number: u64) -> u64 {
        self.offsets
            .iter()
            .find(|&&(n, _)| n == number)
            .map(|&(_, off)| off)
            .unwrap()
    }
    fn classic_trailer(&mut self, size: u64, extra: &str) {
        let xref = self.buf.len() as u64;
        self.text(&format!("xref\n0 {size}\n"));
        self.raw(b"0000000000 65535 f \n");
        for number in 1..size {
            let off = self.offset_of(number);
            self.text(&format!("{off:010} 00000 n \n"));
        }
        self.text(&format!(
            "trailer\n<< /Size {size}{extra} >>\nstartxref\n{xref}\n%%EOF\n"
        ));
    }
}

/// A one-page PDF with a FlateDecode content stream, a font, and classic xref.
fn fixture_pdf() -> Vec<u8> {
    let content = b"BT /F1 12 Tf 72 720 Td (Hello) Tj ET\n";
    let encoded = zlib_stored(content);
    let mut w = PdfBuilder::new();
    w.text("%PDF-1.5\n");
    w.obj(1, b"<< /Type /Catalog /Pages 2 0 R >>");
    w.obj(2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.obj(
        3,
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
    );
    w.stream_obj(4, " /Filter /FlateDecode", &encoded);
    w.obj(5, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
    w.classic_trailer(6, " /Root 1 0 R");
    w.buf
}

fn observe_value(
    store: &mut FieldStore,
    id: &vole_document::field::FieldId,
    selector: &Selector,
    representation: Representation,
) -> Result<AnswerValue, String> {
    let req = ObserveRequest::new(selector.clone(), representation);
    match observe(store, id, &req, Limits::DEFAULT) {
        Ok((answer, _stats, _id)) => Ok(answer.value),
        Err(e) => Err(format!("{:?}", e.class())),
    }
}

/// Drop the two descriptor-shape counters the common document-metadata
/// projection exposes (`object_count`, `graph_ops`). Those describe the chosen
/// reconstruction program, not the document; the searched path reports the
/// winner's program shape and a fixed profile reports its own. They are the ONE
/// observable difference Phase 17 records (see the test below).
fn strip_shape_counters(json: &str) -> String {
    let inner = json.trim_start_matches('{').trim_end_matches('}');
    let kept: Vec<&str> = inner
        .split(',')
        .filter(|f| !f.starts_with("\"object_count\":") && !f.starts_with("\"graph_ops\":"))
        .collect();
    format!("{{{}}}", kept.join(","))
}

#[test]
fn direct_runtime_build_matches_the_searched_path() {
    let source = fixture_pdf();

    // --- current path: portfolio encode, then ingest -----------------------
    let (auto_descriptor, auto_report) =
        vole_document::encode::encode(&source, Limits::DEFAULT).unwrap();
    assert!(
        auto_report.candidates_evaluated > 1,
        "the portfolio must price more than one candidate for this to be a real ablation"
    );
    let a_root = temp_root("searched");
    let mut store_a = FieldStore::open(&a_root).unwrap();
    let a_ingest =
        field_ingest::ingest_pdf(&mut store_a, &auto_descriptor, Limits::DEFAULT).unwrap();
    store_a.sync().unwrap();

    // --- direct path: one fixed program, no search -------------------------
    let b_root = temp_root("direct");
    let mut store_b = FieldStore::open(&b_root).unwrap();
    let direct = build_field(
        &source,
        &mut store_b,
        Limits::DEFAULT,
        BuildProfile::Runtime,
    )
    .unwrap();
    store_b.sync().unwrap();
    assert_eq!(direct.profile, "runtime");
    assert_eq!(direct.candidate, "RAW");
    assert_eq!(
        direct.candidates_evaluated, 1,
        "the direct path must price exactly one candidate"
    );

    let b_field = direct.ingest.field_id();
    let (b_source_len, b_root_node, b_index_root, b_node_count) = match &direct.ingest {
        DirectIngest::Pdf(r) => (r.source_len, r.root_node, r.index_root, r.node_count),
        #[cfg(feature = "package")]
        DirectIngest::Package(r) => (r.source_len, r.root_node, r.index_root, r.node_count),
    };

    // --- exactness: length + SHA-256 + byte compare ------------------------
    assert_eq!(b_source_len, source.len() as u64);
    let field_a = Field::open(&store_a, &a_ingest.field, Limits::DEFAULT).unwrap();
    let field_b = Field::open(&store_b, &b_field, Limits::DEFAULT).unwrap();
    assert_eq!(field_a.materialize_exact(Limits::DEFAULT).unwrap(), source);
    assert_eq!(field_b.materialize_exact(Limits::DEFAULT).unwrap(), source);
    // The report's source digest is the court's own hex digest of the source.
    assert_eq!(
        direct.source_sha256,
        vole_document::integrity::to_hex(&vole_document::integrity::sha256(&source))
    );

    // --- structure equality: same recovered root and index -----------------
    assert_eq!(
        a_ingest.root_node, b_root_node,
        "both paths must recover the same exact root node"
    );
    assert_eq!(
        a_ingest.index_root, b_index_root,
        "both paths must recover the same hierarchical index root"
    );
    assert_eq!(
        a_ingest.node_count, b_node_count,
        "both paths must recover the same node count"
    );

    // --- observation equality over a mixed schedule ------------------------
    // The document-level surface must be *identical*.
    let schedule: Vec<(Selector, Representation)> = vec![
        (Selector::Text, Representation::Text),
        (Selector::Page(1), Representation::Text),
        (Selector::Page(1), Representation::Structure),
        (Selector::Page(1), Representation::Operators),
        (Selector::Page(1), Representation::Preview),
        (
            Selector::ByteRange { offset: 0, len: 16 },
            Representation::ExactBytes,
        ),
        (Selector::Document, Representation::FullDocument),
        (
            Selector::SearchMatch("Hello".to_string()),
            Representation::Text,
        ),
    ];

    let mut answered = 0usize;
    for (selector, representation) in &schedule {
        let a = observe_value(&mut store_a, &a_ingest.field, selector, *representation);
        let b = observe_value(&mut store_b, &b_field, selector, *representation);
        assert_eq!(
            a, b,
            "observation differed for {selector:?}/{representation:?}: \
             searched={a:?} direct={b:?}"
        );
        if a.is_ok() {
            answered += 1;
        }
    }
    assert!(
        answered >= 4,
        "the schedule must actually exercise the observation surface (got {answered})"
    );

    // --- the ONE known difference: structural-descriptor metadata counters --
    // `Selector::Metadata` for a PDF is *native structural descriptor* metadata
    // (there is no common PDF title/author projection), so it embeds the chosen
    // program's `object_count`/`graph_ops`. A fixed profile cannot reproduce the
    // searched winner's counters; everything else in the metadata must match.
    let am = observe_value(
        &mut store_a,
        &a_ingest.field,
        &Selector::Metadata,
        Representation::Metadata,
    );
    let bm = observe_value(
        &mut store_b,
        &b_field,
        &Selector::Metadata,
        Representation::Metadata,
    );
    match (am, bm) {
        (Ok(AnswerValue::Json(ja)), Ok(AnswerValue::Json(jb))) => {
            assert_eq!(
                strip_shape_counters(&ja),
                strip_shape_counters(&jb),
                "metadata must match on every document/field fact: a={ja} b={jb}"
            );
            // The direct profile reports its own fixed program's shape (RAW: one
            // object, one op). Recorded, not hidden.
            assert!(jb.contains("\"object_count\":1"), "direct metadata: {jb}");
            assert!(jb.contains("\"graph_ops\":1"), "direct metadata: {jb}");
        }
        other => panic!("expected metadata JSON on both lanes, got {other:?}"),
    }

    std::fs::remove_dir_all(&a_root).ok();
    std::fs::remove_dir_all(&b_root).ok();
}

#[test]
fn direct_build_accepts_arbitrary_bytes_and_stays_exact() {
    // A non-document input exercises the opaque floor (scan declines, no index),
    // and must still be exact with zero search.
    let source = b"not a document, just bytes\n".repeat(128);
    let root = temp_root("opaque");
    let mut store = FieldStore::open(&root).unwrap();
    let report = build_field(&source, &mut store, Limits::DEFAULT, BuildProfile::Runtime).unwrap();
    assert_eq!(report.candidates_evaluated, 1);
    let field_id = report.ingest.field_id();
    let field = Field::open(&store, &field_id, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), source);
    std::fs::remove_dir_all(&root).ok();
}

#[cfg(feature = "package")]
fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

#[cfg(feature = "package")]
fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// A ZIP with stored members (order preserved), UTF-8 flag set. Ground truth for
/// the package adapter; no compression, so the bytes are trivially exact.
#[cfg(feature = "package")]
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

/// The direct package build must scan the **original** ZIP bytes: it must recover
/// exactly the same members/index/nodes as the searched `encode` + `ingest` path,
/// answer the same observations, and materialize the source byte-exactly. This is
/// the ZIP analogue of `direct_runtime_build_matches_the_searched_path` and the
/// regression guard for the Phase-18.2 `ingest_package_direct` path.
#[cfg(feature = "package")]
#[test]
fn direct_package_build_matches_the_searched_path() {
    let source = build_zip(&[
        ("mimetype".to_string(), b"application/epub+zip".to_vec()),
        (
            "META-INF/container.xml".to_string(),
            b"<container>meta</container>".to_vec(),
        ),
        (
            "OEBPS/chapter.xhtml".to_string(),
            b"<html><body><p>one pass, scanned from the original bytes</p></body></html>".to_vec(),
        ),
        (
            "OEBPS/data.bin".to_string(),
            (0u8..=255).cycle().take(4096).collect(),
        ),
    ]);

    // --- searched path: portfolio encode, then package ingest --------------
    let (auto_descriptor, auto_report) =
        vole_document::encode::encode(&source, Limits::DEFAULT).unwrap();
    assert!(
        auto_report.candidates_evaluated >= 1,
        "the searched lane must price at least one candidate"
    );
    let a_root = temp_root("pkg-searched");
    let mut store_a = FieldStore::open(&a_root).unwrap();
    let a_ingest = field_ingest::ingest(&mut store_a, &auto_descriptor, Limits::DEFAULT).unwrap();
    let (a_field, a_root_node, a_index_root, a_node_count) = match &a_ingest {
        IngestOutcome::Package(r) => (r.field, r.root_node, r.index_root, r.node_count),
        IngestOutcome::Pdf(r) => panic!("a ZIP must ingest as a package, got {r:?}"),
    };
    store_a.sync().unwrap();

    // --- direct path: one fixed program, no search, original bytes scanned --
    let b_root = temp_root("pkg-direct");
    let mut store_b = FieldStore::open(&b_root).unwrap();
    let direct = build_field(
        &source,
        &mut store_b,
        Limits::DEFAULT,
        BuildProfile::Runtime,
    )
    .unwrap();
    store_b.sync().unwrap();
    assert_eq!(direct.profile, "runtime");
    assert_eq!(direct.candidates_evaluated, 1);
    let (b_field, b_root_node, b_index_root, b_node_count, b_members) = match &direct.ingest {
        DirectIngest::Package(r) => (
            r.field,
            r.root_node,
            r.index_root,
            r.node_count,
            r.member_count,
        ),
        DirectIngest::Pdf(r) => panic!("a ZIP must build as a package, got {r:?}"),
    };
    let a_members = match &a_ingest {
        IngestOutcome::Package(r) => r.member_count,
        IngestOutcome::Pdf(_) => unreachable!(),
    };

    // --- exactness: length + SHA-256 + byte compare ------------------------
    let field_a = Field::open(&store_a, &a_field, Limits::DEFAULT).unwrap();
    let field_b = Field::open(&store_b, &b_field, Limits::DEFAULT).unwrap();
    let a_bytes = field_a.materialize_exact(Limits::DEFAULT).unwrap();
    let b_bytes = field_b.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(a_bytes.len(), source.len());
    assert_eq!(b_bytes.len(), source.len());
    assert_eq!(
        vole_document::integrity::to_hex(&vole_document::integrity::sha256(&b_bytes)),
        vole_document::integrity::to_hex(&vole_document::integrity::sha256(&source))
    );
    assert_eq!(
        b_bytes, source,
        "direct field must materialize the source byte-exactly"
    );
    assert_eq!(
        direct.source_sha256,
        vole_document::integrity::to_hex(&vole_document::integrity::sha256(&source))
    );

    // --- structure equality: same root, index, node count, members ---------
    assert_eq!(a_root_node, b_root_node);
    assert_eq!(a_index_root, b_index_root);
    assert_eq!(a_node_count, b_node_count);
    assert_eq!(a_members, b_members);

    // --- observation equality over a mixed schedule ------------------------
    let schedule: Vec<(Selector, Representation)> = vec![
        (Selector::Text, Representation::Text),
        (Selector::Document, Representation::FullDocument),
        (
            Selector::ByteRange { offset: 0, len: 24 },
            Representation::ExactBytes,
        ),
    ];
    for (selector, representation) in &schedule {
        let a = observe_value(&mut store_a, &a_field, selector, *representation);
        let b = observe_value(&mut store_b, &b_field, selector, *representation);
        assert_eq!(
            a, b,
            "package observation differed for {selector:?}/{representation:?}: \
             searched={a:?} direct={b:?}"
        );
    }

    std::fs::remove_dir_all(&a_root).ok();
    std::fs::remove_dir_all(&b_root).ok();
}
