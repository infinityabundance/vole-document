//! Phase 29.2 — the coarse structural index court.
//!
//! The index is an *optional side structure*: it partitions the verbatim
//! authority object into format-native addressable units and serves a narrow
//! observation from real `pread` reads of those units, instead of the current
//! whole-descriptor closure. These tests cover the claims the phase rests on:
//!
//! * **Directory build** for PDF (objects), ZIP/OPC (members + central
//!   directory) and Parquet (column chunks + footer) coarse units.
//! * **A materially narrower real closure**: a page-1 text request served from
//!   the index reads a small fraction of the whole-descriptor control and
//!   returns byte-identical answers.
//! * **Exactness is unchanged**: `materialize --exact` still reproduces the
//!   source byte-for-byte after the index is built.
//! * **Typed declines** on truncated/hostile sources, and **no panic** on
//!   random bytes.
//!
//! The whole file is gated on the `field` feature so `--no-default-features`
//! builds an empty (compiling) test target.

#![cfg(feature = "field")]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::field::coarse::{self, CoarseIndex, CoarseUnit, UnitKind, build_units};
use vole_document::field::ingest as field_ingest;
use vole_document::field::observe::{ObserveRequest, Representation, Selector, observe};
use vole_document::field::{Field, FieldStore};
use vole_document::limits::Limits;

const SMALL_PARQUET: &[u8] = include_bytes!("../tools/fixtures/parquet/small_plain.parquet");
const MULTI_RG_PARQUET: &[u8] = include_bytes!("../tools/fixtures/parquet/multi_rg.parquet");

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-coarse-{label}-{}-{}",
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

/// A classic PDF with `pages` pages, each with a sizeable content stream, so a
/// page-1 observation is a small fraction of the whole source.
fn large_pdf(pages: u64) -> Vec<u8> {
    let mut w = PdfBuilder::new();
    w.text("%PDF-1.5\n");
    let pages_id = 2;
    let font_id = 3 + 2 * pages;
    w.obj(1, b"<< /Type /Catalog /Pages 2 0 R >>");
    let kids: Vec<String> = (0..pages).map(|i| format!("{} 0 R", 3 + 2 * i)).collect();
    w.obj(
        pages_id,
        format!(
            "<< /Type /Pages /Kids [{}] /Count {pages} >>",
            kids.join(" ")
        )
        .as_bytes(),
    );
    for i in 0..pages {
        let page_id = 3 + 2 * i;
        let content_id = 4 + 2 * i;
        w.obj(
            page_id,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
                 /Resources << /Font << /F1 {font_id} 0 R >> >> /Contents {content_id} 0 R >>"
            )
            .as_bytes(),
        );
        let mut content = format!(
            "BT /F1 12 Tf 72 720 Td (Page {} of the coarse index court) Tj ET\n",
            i + 1
        );
        // Pad the content stream so each page's stream is a few KiB.
        content.push_str("BT /F1 12 Tf 72 700 Td (");
        content.push_str(&"x".repeat(4096));
        content.push_str(") Tj ET\n");
        w.stream_obj(
            content_id,
            " /Filter /FlateDecode",
            &zlib_stored(content.as_bytes()),
        );
    }
    w.obj(
        font_id,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    );
    w.classic_trailer(font_id + 1, " /Root 1 0 R");
    w.buf
}

fn opaque_descriptor(source: &[u8]) -> Vec<u8> {
    let d = Descriptor {
        universe: vole_document::container::UNIVERSE.to_string(),
        source_format: vole_document::SOURCE_FORMAT_OPAQUE,
        format_basis: "opaque:coarse-index-test".to_string(),
        models: vec![],
        channels: vec![],
        objects: vec![ObjectSource::Inline(source.to_vec())],
        program: Program::new(vec![Op::EmitObject { object_id: 0 }]),
        observation_index: None,
        seek_directory: false,
        checkpoints: None,
        source_sha256: vole_document::integrity::sha256(source),
        source_len: source.len() as u64,
    };
    d.serialize().unwrap().0
}

// ---------------------------------------------------------------------------
// Minimal stored-mode ZIP (OPC-shaped), used only for `build_units` (no CRC or
// adapter check is involved there).
// ---------------------------------------------------------------------------

fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    const UTF8_FLAG: u16 = 0x0800;
    let mut out: Vec<u8> = Vec::new();
    let mut offsets: Vec<u32> = Vec::new();

    for (name, content) in entries {
        offsets.push(out.len() as u32);
        put_u32(&mut out, 0x0403_4b50);
        put_u16(&mut out, 20);
        put_u16(&mut out, UTF8_FLAG);
        put_u16(&mut out, 0); // stored
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u32(&mut out, 0); // crc (not checked by the coarse parser)
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
        put_u32(&mut out, 0);
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

fn count_kind(units: &[CoarseUnit], kind: UnitKind) -> usize {
    units.iter().filter(|u| u.kind == kind).count()
}

// ---------------------------------------------------------------------------
// Directory build
// ---------------------------------------------------------------------------

#[test]
fn pdf_directory_units_are_objects_and_cover_the_source() {
    let source = large_pdf(8);
    let (format, units) = build_units(&source);
    assert_eq!(format, 1, "a %PDF- source is recognised as PDF");
    assert!(
        count_kind(&units, UnitKind::PdfObject) >= 8,
        "there must be at least one object unit per page plus catalog/pages/font"
    );
    assert_eq!(count_kind(&units, UnitKind::PdfTail), 1);
    // The units are an ascending partition of [0, source_len).
    let mut cursor = 0u64;
    for u in &units {
        assert_eq!(u.offset, cursor, "units must be contiguous and ascending");
        cursor += u64::from(u.len);
    }
    assert_eq!(
        cursor,
        source.len() as u64,
        "units must cover the whole source"
    );
}

#[test]
fn zip_directory_units_are_members_plus_central_directory() {
    let big = vec![b'A'; 32 * 1024];
    let source = build_zip(&[
        ("[Content_Types].xml", b"<Types/>"),
        ("_rels/.rels", b"<Relationships/>"),
        ("word/document.xml", &big),
    ]);
    let (format, units) = build_units(&source);
    assert_eq!(format, 2, "a PK\\x03\\x04 source is recognised as ZIP/OPC");
    assert_eq!(count_kind(&units, UnitKind::ZipMember), 3);
    assert_eq!(count_kind(&units, UnitKind::ZipDirectory), 1);
    let mut cursor = 0u64;
    for u in &units {
        assert_eq!(u.offset, cursor);
        cursor += u64::from(u.len);
    }
    assert_eq!(cursor, source.len() as u64);
}

#[test]
fn parquet_directory_units_are_column_chunks_plus_footer() {
    for source in [SMALL_PARQUET, MULTI_RG_PARQUET] {
        let (format, units) = build_units(source);
        assert_eq!(format, 3, "a PAR1..PAR1 source is recognised as Parquet");
        assert!(
            count_kind(&units, UnitKind::ParquetColumnChunk) >= 1,
            "at least one column-chunk unit"
        );
        assert_eq!(count_kind(&units, UnitKind::ParquetFooter), 1);
        let mut cursor = 0u64;
        for u in &units {
            assert_eq!(u.offset, cursor);
            cursor += u64::from(u.len);
        }
        assert_eq!(cursor, source.len() as u64);
    }
}

// ---------------------------------------------------------------------------
// The narrow real closure through the store + CLI-equivalent lane
// ---------------------------------------------------------------------------

#[test]
fn narrow_read_reads_materially_fewer_bytes_than_the_whole_descriptor() {
    let dir = temp_dir("narrow");
    let source = large_pdf(24);
    let desc = opaque_descriptor(&source);
    let mut store = FieldStore::open(&dir).unwrap();
    let id = field_ingest::ingest_pdf(&mut store, &desc, Limits::DEFAULT)
        .unwrap()
        .field;

    // Build and persist the coarse directory.
    let rep = coarse::build(&dir, false, &id.to_hex(), Limits::DEFAULT).unwrap();
    assert_eq!(rep.format, "pdf");
    assert!(rep.native_units >= 24, "at least one object unit per page");

    // The whole-descriptor control closure for the same narrow request.
    let mut req = ObserveRequest::new(Selector::Page(1), Representation::Text);
    req.use_cache = false;
    let (control, control_stats, _) = observe(&mut store, &id, &req, Limits::DEFAULT).unwrap();
    let control_bytes = control_stats.bytes_read;
    assert!(
        control_bytes >= source.len() as u64,
        "the control closure must read the whole descriptor ({control_bytes} vs source {})",
        source.len()
    );

    // The coarse lane: discovery (records the map) then replay (`--use-map`).
    let requests = vec![("--page 1 --kind text".to_string(), req.clone())];
    let rows = coarse::read(&dir, false, &id.to_hex(), &requests, false, Limits::DEFAULT).unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.outcome, "planned", "reason: {}", row.fallback_reason);
    assert!(
        row.matches_full,
        "the coarse answer must equal the full answer"
    );
    assert!(row.preads >= 1, "the coarse lane must make real preads");
    let coarse_total =
        row.unit_bytes + row.dir_bytes + row.manifest_bytes + row.index_bytes + row.seed_bytes;
    assert!(
        coarse_total * 4 < control_bytes,
        "coarse closure {coarse_total} must be materially (<1/4) below the control {control_bytes}"
    );

    // `--use-map` replays the persisted plan and must reach the same answer.
    let rows_map =
        coarse::read(&dir, false, &id.to_hex(), &requests, true, Limits::DEFAULT).unwrap();
    assert_eq!(rows_map[0].outcome, "planned");
    assert!(rows_map[0].matches_full);
    assert_eq!(
        rows_map[0].units_read, row.units_read,
        "the persisted map must cover exactly the discovered units"
    );

    // The stored answer is byte-identical to the control.
    assert_eq!(
        rows[0].answer_bytes, control_stats.bytes_returned,
        "the coarse answer must be the same size as the control answer"
    );
    let _ = control;
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn coarse_index_build_does_not_change_exactness() {
    let dir = temp_dir("exact");
    let source = large_pdf(6);
    let desc = opaque_descriptor(&source);
    let mut store = FieldStore::open(&dir).unwrap();
    let id = field_ingest::ingest_pdf(&mut store, &desc, Limits::DEFAULT)
        .unwrap()
        .field;
    let _ = coarse::build(&dir, false, &id.to_hex(), Limits::DEFAULT).unwrap();

    let field = Field::open(&store, &id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), source.len());
    assert_eq!(
        exact, source,
        "materialize --exact must still equal the source"
    );
    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// Hostile / truncated inputs decline typed; random bytes never panic
// ---------------------------------------------------------------------------

#[test]
fn truncated_and_hostile_sources_decline_to_the_authority_leaf() {
    // Truncated ZIP (the whole end-of-central-directory is gone) is not a
    // supported ZIP: it falls back to a leaf.
    let mut zip = build_zip(&[("a", b"hello")]);
    zip.truncate(zip.len() - 25);
    let (format, units) = build_units(&zip);
    assert_eq!(format, 0);
    assert_eq!(units.len(), 1);
    assert_eq!(units[0].kind, UnitKind::AuthorityLeaf);

    // Truncated Parquet (bad footer length) falls back to a leaf.
    let mut pq = SMALL_PARQUET.to_vec();
    let n = pq.len();
    pq[n - 5] = 0x7f; // corrupt the little-endian footer length's high byte
    let (format, units) = build_units(&pq);
    assert_eq!(format, 0);
    assert_eq!(units[0].kind, UnitKind::AuthorityLeaf);

    // A deeply-nested Thrift-Compact footer must decline typed rather than
    // recurse the parser into a stack overflow.
    let depth = 100_000usize;
    let mut nested = b"PAR1".to_vec();
    nested.extend(std::iter::repeat_n(0x1Cu8, depth)); // field delta 1, type 12 (struct)
    nested.extend(std::iter::repeat_n(0x00u8, depth * 2 + 8)); // close every level
    let flen = (nested.len() - 4) as u32;
    nested.extend_from_slice(&flen.to_le_bytes());
    nested.extend_from_slice(b"PAR1");
    let (format, units) = build_units(&nested);
    assert_eq!(format, 0, "an over-deep footer must decline, not overflow");
    assert_eq!(units[0].kind, UnitKind::AuthorityLeaf);
}

#[test]
fn random_bytes_never_panic() {
    // A deterministic LCG: random bytes are never a valid PDF/ZIP/Parquet and
    // must never panic the builder or the index decoder.
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (state >> 33) as u8
    };
    for len in [0usize, 1, 3, 4, 11, 12, 13, 64, 257, 4096, 65536] {
        let bytes: Vec<u8> = (0..len).map(|_| next()).collect();
        let (_format, units) = build_units(&bytes);
        assert!(!units.is_empty() || len == 0);
        // The decoder must return, always, for arbitrary input.
        let _ = CoarseIndex::decode(&bytes, Limits::DEFAULT);
    }
    // A well-formed index encodes and decodes to itself.
    let idx = CoarseIndex {
        format: 1,
        source_len: 10,
        source_sha256: [7u8; 32],
        object_base: 128,
        object_count: 1,
        graph_ops: 1,
        units: vec![CoarseUnit {
            kind: UnitKind::AuthorityLeaf,
            id: 0,
            offset: 0,
            len: 10,
        }],
    };
    let enc = idx.encode();
    let dec = CoarseIndex::decode(&enc, Limits::DEFAULT).unwrap();
    assert_eq!(dec.source_len, 10);
    assert_eq!(dec.units.len(), 1);

    // A forged index with a non-partition unit set must be rejected typed.
    let mut bad = enc.clone();
    // The header is 65 bytes (magic+format+counts+lens+sha+unit-count); the
    // first unit entry is kind(1)+id(4)+offset(8)+len(4), so the offset's first
    // byte is at 65 + 5. Forcing it non-zero breaks the ascending partition.
    bad[65 + 5] = 0xff;
    assert!(
        CoarseIndex::decode(&bad, Limits::DEFAULT).is_err(),
        "a non-partition unit set must be a typed decode error"
    );
    // Trailing garbage is rejected.
    let mut trailing = enc.clone();
    trailing.push(0);
    assert!(CoarseIndex::decode(&trailing, Limits::DEFAULT).is_err());
}
