//! Deterministic, byte-exact sample PDFs for the Phase-3 court.
//!
//! Every well-formed sample is assembled in memory with its own byte offsets as
//! it is written, so every direct `/Length` and every `startxref` target is
//! correct *by construction*: the corpus never depends on a PDF writer, a
//! canonicalizer, or a post-hoc fixup pass.
//!
//! Two entries are deliberate negative controls, listed in
//! [`NEGATIVE_CONTROLS`]: `malformed.pdf` is truncated with no `%%EOF`, and
//! `notpdf.bin` is plain non-PDF bytes. Both must be *rejected* by
//! [`super::detect`] while still round-tripping byte-for-byte through the opaque
//! RAW lane. `malformed.pdf` therefore carries a `.pdf` name but is not a
//! validated PDF: a file name is a hint, never authority.

/// One ordered sample: a fixed name and its exact bytes.
pub fn sample_pdfs() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("classic.pdf", classic()),
        ("xrefstream.pdf", xref_stream()),
        ("objstm.pdf", object_stream()),
        ("incremental.pdf", incremental()),
        ("mixedeol.pdf", mixed_eol()),
        ("traptext.pdf", trap_text()),
        ("trapstream.pdf", trap_stream()),
        ("bigtext.pdf", big_text_pdf()),
        ("many.pdf", many_objects_pdf()),
        ("flate.pdf", flate_pdf()),
        ("malformed.pdf", malformed()),
        ("notpdf.bin", not_pdf()),
    ]
}

/// Committed real-zlib fixture blobs (produced by zlib-rs). Regenerate with
/// `cargo test --all-features regenerate_flate_fixtures -- --ignored`. The
/// library carries no compressor dependency; these are opaque inputs.
const FLATE_P1_L0: &[u8] = include_bytes!("fixtures/p1_l0.zlib");
const FLATE_P1_L1: &[u8] = include_bytes!("fixtures/p1_l1.zlib");
const FLATE_P1_L6: &[u8] = include_bytes!("fixtures/p1_l6.zlib");
const FLATE_P1_L9: &[u8] = include_bytes!("fixtures/p1_l9.zlib");
const FLATE_P2_L6: &[u8] = include_bytes!("fixtures/p2_l6.zlib");
const FLATE_P3_L6: &[u8] = include_bytes!("fixtures/p3_l6.zlib");

/// The two entries that must *not* be detected as PDFs. `malformed.pdf` is the
/// only `.pdf`-named member of this set.
pub const NEGATIVE_CONTROLS: [&str; 2] = ["malformed.pdf", "notpdf.bin"];

/// Whether `name` is one of the deliberate non-PDF negative controls.
pub fn is_negative_control(name: &str) -> bool {
    NEGATIVE_CONTROLS.contains(&name)
}

/// Append-only PDF assembler that records each object's byte offset, so a
/// classic cross-reference table can be emitted with correct offsets.
struct Writer {
    buf: Vec<u8>,
    offsets: Vec<(u64, u64)>,
}

impl Writer {
    fn new() -> Self {
        Writer {
            buf: Vec::new(),
            offsets: Vec::new(),
        }
    }

    fn raw(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    fn text(&mut self, s: &str) {
        self.buf.extend_from_slice(s.as_bytes());
    }

    /// Offset of object `number`'s `N G obj` introducer.
    fn offset_of(&self, number: u64) -> u64 {
        self.offsets
            .iter()
            .find(|&&(n, _)| n == number)
            .map(|&(_, off)| off)
            .unwrap_or_else(|| panic!("object {number} was never written"))
    }

    /// Append `N G obj\n<body>\nendobj\n`, recording the introducer offset.
    fn obj(&mut self, number: u64, generation: u64, body: &[u8]) {
        self.offsets.push((number, self.buf.len() as u64));
        self.text(&format!("{number} {generation} obj\n"));
        self.raw(body);
        self.raw(b"\nendobj\n");
    }

    /// Append a stream object whose `/Length` is the exact payload length, with
    /// `extra_dict` (starting with a space) spliced into the leading dictionary.
    fn stream_obj(&mut self, number: u64, generation: u64, extra_dict: &str, data: &[u8]) {
        self.offsets.push((number, self.buf.len() as u64));
        self.text(&format!(
            "{number} {generation} obj\n<< /Length {}{extra_dict} >>\nstream\n",
            data.len()
        ));
        self.raw(data);
        self.raw(b"\nendstream\nendobj\n");
    }

    /// Append a classic `xref` table covering objects `0..size`, a `trailer`,
    /// `startxref`, and a terminating `%%EOF`. `trailer_extra` is spliced into
    /// the trailer dictionary. The `startxref` value is the table's own offset.
    fn classic_trailer(&mut self, size: u64, trailer_extra: &str) {
        let xref = self.buf.len() as u64;
        self.text(&format!("xref\n0 {size}\n"));
        self.raw(b"0000000000 65535 f \n");
        for number in 1..size {
            let off = self.offset_of(number);
            self.text(&format!("{off:010} 00000 n \n"));
        }
        self.text(&format!(
            "trailer\n<< /Size {size}{trailer_extra} >>\nstartxref\n{xref}\n%%EOF\n"
        ));
    }
}

/// A classic cross-reference PDF: header, three indirect objects, a classic
/// `xref` table, and a trailer whose `startxref` points at the table.
fn classic() -> Vec<u8> {
    let mut w = Writer::new();
    w.text("%PDF-1.4\n");
    w.obj(1, 0, b"<< /Type /Catalog /Pages 2 0 R >>");
    w.obj(2, 0, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.obj(
        3,
        0,
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
    );
    w.classic_trailer(4, " /Root 1 0 R");
    w.buf
}

/// A cross-reference-stream PDF. Object 3 is `/Type /XRef`; its 28-byte payload
/// is a genuine `W [1 4 2]` table with the correct offsets of objects 1, 2, and
/// 3 (including itself). `startxref` targets that object by construction.
fn xref_stream() -> Vec<u8> {
    let mut w = Writer::new();
    w.text("%PDF-1.5\n");
    w.obj(1, 0, b"<< /Type /Catalog /Pages 2 0 R >>");
    w.obj(2, 0, b"<< /Type /Pages /Kids [] /Count 0 >>");

    let off1 = w.offset_of(1);
    let off2 = w.offset_of(2);
    let off3 = w.buf.len() as u64;

    let mut data = Vec::with_capacity(28);
    // Object 0: free.
    data.push(0u8);
    data.extend_from_slice(&0u32.to_be_bytes());
    data.extend_from_slice(&65535u16.to_be_bytes());
    // Objects 1, 2, 3: in-use, field 2 is the byte offset, generation 0.
    for off in [off1, off2, off3] {
        data.push(1u8);
        data.extend_from_slice(&(off as u32).to_be_bytes());
        data.extend_from_slice(&0u16.to_be_bytes());
    }
    assert_eq!(data.len(), 28);

    w.stream_obj(3, 0, " /Type /XRef /Size 4 /Root 1 0 R /W [1 4 2]", &data);
    assert_eq!(w.offset_of(3), off3);
    w.text(&format!("startxref\n{off3}\n%%EOF\n"));
    w.buf
}

/// An object-stream PDF: object 2 is `/Type /ObjStm` and carries an opaque
/// payload of the form `N1 off1 N2 off2 <bodies>`.
fn object_stream() -> Vec<u8> {
    let mut w = Writer::new();
    w.text("%PDF-1.5\n");
    w.obj(1, 0, b"<< /Type /Catalog /Pages 3 0 R >>");
    w.stream_obj(2, 0, " /Type /ObjStm /N 2 /First 8", b"1 0 3 0 42");
    w.obj(3, 0, b"<< /Type /Pages /Kids [] /Count 0 >>");
    w.classic_trailer(4, " /Root 1 0 R");
    w.buf
}

/// An incrementally updated PDF: a base revision with a classic `xref`, then an
/// appended revision that adds object 3 and carries a trailer whose `/Prev`
/// points back at the base `xref` by construction.
fn incremental() -> Vec<u8> {
    let mut w = Writer::new();
    w.text("%PDF-1.4\n");
    w.obj(1, 0, b"<< /Type /Catalog /Pages 2 0 R >>");
    w.obj(2, 0, b"<< /Type /Pages /Kids [] /Count 0 >>");

    // Base revision's classic xref starts here; `/Prev` must name this offset.
    let xref1 = w.buf.len() as u64;
    w.classic_trailer(3, " /Root 1 0 R");

    // Appended revision.
    w.obj(
        3,
        0,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    );
    let prev_extra = format!(" /Prev {xref1} /Root 1 0 R");
    w.classic_trailer(4, &prev_extra);
    w.buf
}

/// A PDF that mixes CRLF and LF line endings, including a CRLF-terminated
/// stream boundary.
fn mixed_eol() -> Vec<u8> {
    let mut b: Vec<u8> = Vec::new();
    b.extend_from_slice(b"%PDF-1.4\r\n");

    let off1 = b.len() as u64;
    b.extend_from_slice(b"1 0 obj\r\n<< /Type /Catalog /Pages 2 0 R >>\r\nendobj\r\n");

    let off2 = b.len() as u64;
    b.extend_from_slice(b"2 0 obj\n<< /Length 5 >>\nstream\nhello\nendstream\nendobj\n");

    let xref = b.len() as u64;
    b.extend_from_slice(b"xref\r\n0 3\r\n");
    b.extend_from_slice(b"0000000000 65535 f \r\n");
    b.extend_from_slice(format!("{off1:010} 00000 n \n").as_bytes());
    b.extend_from_slice(format!("{off2:010} 00000 n \r\n").as_bytes());
    b.extend_from_slice(
        format!("trailer\r\n<< /Size 3 /Root 1 0 R >>\r\nstartxref\r\n{xref}\r\n%%EOF\r\n")
            .as_bytes(),
    );
    b
}

/// A PDF whose object 1 contains a literal string holding the spelling
/// `endobj` (and `stream`). Lexing must keep the literal opaque, so the object
/// is not split and the file remains a validated PDF.
fn trap_text() -> Vec<u8> {
    let mut w = Writer::new();
    w.text("%PDF-1.4\n");
    w.obj(
        1,
        0,
        b"<< /Type /Catalog /Pages 2 0 R /Title (trap endobj stream text) >>",
    );
    w.obj(2, 0, b"<< /Type /Pages /Kids [] /Count 0 >>");
    w.classic_trailer(3, " /Root 1 0 R");
    w.buf
}

/// A PDF whose stream payload contains the spellings `endobj` and `stream`.
/// The verified direct `/Length` must win, so the payload stays opaque and the
/// enclosing object is not split.
fn trap_stream() -> Vec<u8> {
    let mut w = Writer::new();
    w.text("%PDF-1.5\n");
    w.obj(1, 0, b"<< /Type /Catalog /Pages 2 0 R >>");
    w.stream_obj(2, 0, "", b"endobj stream bytes");
    w.classic_trailer(3, " /Root 1 0 R");
    w.buf
}

/// A larger, plain-content PDF (~64 KiB): a classic cross-reference PDF whose
/// single content stream is a long, deterministic, *uncompressed* sequence of
/// show-text operators.
///
/// The content is deliberately lexical: thousands of near-identical lines whose
/// per-kind byte distributions differ sharply (whitespace, names, regular
/// operators, and literal strings). That is exactly the regime the typed-channel
/// candidate is meant to exploit, so this sample is the scale at which
/// per-channel models can actually pay off — the complete-cost court still
/// decides whether they do.
fn big_text_pdf() -> Vec<u8> {
    const LINES: usize = 1000;
    let mut content = Vec::with_capacity(LINES * 64);
    for i in 0..LINES {
        content.extend_from_slice(
            format!("BT /F1 12 Tf 72 720 Td (Invoice line {i:06} amount 456.78) Tj ET\n")
                .as_bytes(),
        );
    }

    let mut w = Writer::new();
    w.text("%PDF-1.4\n");
    w.obj(1, 0, b"<< /Type /Catalog /Pages 2 0 R >>");
    w.obj(2, 0, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.obj(
        3,
        0,
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
    );
    w.stream_obj(4, 0, "", &content);
    w.obj(
        5,
        0,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    );
    w.classic_trailer(6, " /Root 1 0 R");
    w.buf
}

/// A classic cross-reference PDF with many tiny indirect objects: `OBJECTS`
/// bodies of the form `N 0 obj\n<< /K n >>\nendobj\n`, each written with its own
/// byte offset, followed by a matching classic `xref` table, trailer, and
/// `startxref`.
///
/// This is the scale sample for the layout candidate: with one xref entry per
/// object, per-entry framing dominates at small object counts, so this file is
/// where amortizing that framing over a single packed item table either pays off
/// or does not. `MANY_OBJECTS` is kept below the 255 markable-slot limit so
/// the candidate is genuinely accepted; the object count still exceeds the
/// 100-entry threshold the scaling test pins.
const MANY_OBJECTS: u64 = 200;
fn many_objects_pdf() -> Vec<u8> {
    let mut w = Writer::new();
    w.text("%PDF-1.4\n");
    for number in 1..=MANY_OBJECTS {
        w.obj(number, 0, format!("<< /K {number} >>").as_bytes());
    }
    w.classic_trailer(MANY_OBJECTS + 1, " /Root 1 0 R");
    w.buf
}

/// A PDF whose streams are real zlib-compressed `/FlateDecode` streams.
///
/// Object 4 is the page content stream (`p1`, level 9); object 5 is a font;
/// object 6 carries the *same* plaintext as object 4 at a different level (`p1`,
/// level 6) so shared-plaintext replay is exercised; object 7 is a distinct
/// graphics stream (`p2`, level 6); object 8 is a stored/level-0 stream (`p1`,
/// level 0), the weak-producer case where replay can genuinely pay. Every
/// `/Length` and offset is correct by construction.
fn flate_pdf() -> Vec<u8> {
    let mut w = Writer::new();
    w.text("%PDF-1.5\n");
    w.obj(1, 0, b"<< /Type /Catalog /Pages 2 0 R >>");
    w.obj(2, 0, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.obj(
        3,
        0,
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
    );
    w.stream_obj(4, 0, " /Filter /FlateDecode", FLATE_P1_L9);
    w.obj(
        5,
        0,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    );
    w.stream_obj(6, 0, " /Filter /FlateDecode", FLATE_P1_L6);
    w.stream_obj(7, 0, " /Filter /FlateDecode", FLATE_P2_L6);
    w.stream_obj(8, 0, " /Filter /FlateDecode", FLATE_P1_L0);
    w.stream_obj(9, 0, " /Filter /FlateDecode", FLATE_P1_L1);
    w.stream_obj(10, 0, " /Filter /FlateDecode", FLATE_P3_L6);
    w.classic_trailer(11, " /Root 1 0 R");
    w.buf
}

/// Truncated and without any `%%EOF`: the header and one complete object are
/// present, but the file ends mid-object. Detection must decline.
fn malformed() -> Vec<u8> {
    let mut b: Vec<u8> = Vec::new();
    b.extend_from_slice(b"%PDF-1.4\n");
    b.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    b.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Ki");
    b
}

/// A plain-text non-PDF control.
fn not_pdf() -> Vec<u8> {
    b"this is plain text, definitely not a PDF\n".to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::pdf::{detect, propose_pdf, scan};
    use crate::limits::Limits;
    use crate::materialize::decode_to_bytes;

    const EXPECTED_NAMES: [&str; 12] = [
        "classic.pdf",
        "xrefstream.pdf",
        "objstm.pdf",
        "incremental.pdf",
        "mixedeol.pdf",
        "traptext.pdf",
        "trapstream.pdf",
        "bigtext.pdf",
        "many.pdf",
        "flate.pdf",
        "malformed.pdf",
        "notpdf.bin",
    ];

    #[test]
    fn corpus_names_are_exactly_as_specified() {
        let names: Vec<&str> = sample_pdfs().into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, EXPECTED_NAMES);
    }

    #[test]
    fn corpus_is_deterministic() {
        let a = sample_pdfs();
        let b = sample_pdfs();
        assert_eq!(a.len(), b.len());
        for ((na, ba), (nb, bb)) in a.iter().zip(b.iter()) {
            assert_eq!(na, nb);
            assert_eq!(ba, bb, "{na} must be byte-identical across calls");
        }
    }

    #[test]
    fn every_sample_is_byte_exact_through_the_court() {
        for (name, bytes) in sample_pdfs() {
            let (encoded, report) = crate::encode::encode(&bytes, Limits::DEFAULT).unwrap();
            let (out, _) = decode_to_bytes(&encoded, Limits::DEFAULT).unwrap();
            assert_eq!(out, bytes, "{name} must materialize byte-exactly");
            assert_eq!(report.source_len, bytes.len() as u64, "{name} source_len");
        }
    }

    #[test]
    fn valid_pdfs_detect_scan_and_propose() {
        for (name, bytes) in sample_pdfs() {
            if is_negative_control(name) {
                continue;
            }
            assert!(name.ends_with(".pdf"), "{name} is expected to be a PDF");

            // Detection.
            assert!(detect(&bytes, Limits::DEFAULT), "{name} must be detected");

            // Scan covers exactly and finds structure.
            let physical = scan(&bytes, Limits::DEFAULT).unwrap();
            physical.validate(bytes.len() as u64).unwrap();
            assert_eq!(physical.total_len(), bytes.len() as u64, "{name} cover");
            assert!(physical.header.is_some(), "{name} must carry a header");
            assert!(!physical.objects.is_empty(), "{name} must have an object");
            assert!(
                !physical.revisions.is_empty(),
                "{name} must have a revision"
            );

            // The proposed candidate materializes byte-for-byte.
            let candidate = propose_pdf(&bytes, Limits::DEFAULT)
                .unwrap()
                .unwrap_or_else(|| panic!("{name} must propose a PDF candidate"));
            let (encoded, _) = candidate.descriptor.serialize().unwrap();
            let (out, _) = decode_to_bytes(&encoded, Limits::DEFAULT).unwrap();
            assert_eq!(out, bytes, "{name} PDF candidate materializes exactly");
        }
    }

    #[test]
    fn negative_controls_are_rejected_but_still_exact() {
        // `notpdf.bin` is the specified non-PDF control; `malformed.pdf` is the
        // `.pdf`-named truncated control. Neither may be detected, both must
        // still round-trip exactly through the opaque RAW lane.
        for (name, bytes) in sample_pdfs() {
            if !is_negative_control(name) {
                continue;
            }
            assert!(
                !detect(&bytes, Limits::DEFAULT),
                "{name} must not be detected as a PDF"
            );
            assert!(
                propose_pdf(&bytes, Limits::DEFAULT).unwrap().is_none(),
                "{name} must decline the PDF candidate"
            );
            let (encoded, report) = crate::encode::encode(&bytes, Limits::DEFAULT).unwrap();
            let (out, _) = decode_to_bytes(&encoded, Limits::DEFAULT).unwrap();
            assert_eq!(out, bytes, "{name} must still be exact via RAW");
            assert_eq!(report.kind.name(), "RAW", "{name} raw lane");
        }
        assert!(!detect(b"plain text, not a PDF", Limits::DEFAULT));
    }

    /// A stream whose payload is written immediately before `endstream` with no
    /// intervening EOL (the real-producer case: Ghostscript 10.00.0). The direct
    /// `/Length` is correct, so `scan` must resolve the exact payload span and the
    /// exact-replay court must admit the committed zlib fixture byte-for-byte.
    #[cfg(feature = "deflate-replay")]
    #[test]
    fn stream_without_eol_before_endstream_scans_and_replays() {
        use crate::codec::deflate::{replay_raw, try_replay};

        let payload: &[u8] = FLATE_P1_L6;
        let mut w = Writer::new();
        w.text("%PDF-1.5\n");
        w.obj(1, 0, b"<< /Type /Catalog /Pages 2 0 R >>");
        // Object 2: correct `/Length`, payload then `endstream` with no EOL.
        w.offsets.push((2, w.buf.len() as u64));
        w.text(&format!(
            "2 0 obj\n<< /Length {} /Filter /FlateDecode >>\nstream\n",
            payload.len()
        ));
        w.raw(payload);
        w.raw(b"endstream\nendobj\n");
        w.classic_trailer(3, " /Root 1 0 R");
        let pdf = w.buf;

        // The physical scanner covers exactly and finds the object and stream.
        let physical = scan(&pdf, Limits::DEFAULT).unwrap();
        physical.validate(pdf.len() as u64).unwrap();
        assert_eq!(
            physical.total_len(),
            pdf.len() as u64,
            "cover must be exact"
        );
        let obj = physical
            .objects
            .iter()
            .find(|o| o.number == 2)
            .expect("object 2 must be found");
        assert_eq!(obj.generation, 0);
        let stream = physical
            .streams
            .iter()
            .find(|s| s.object == 2)
            .expect("stream 2 must be found");
        assert_eq!(stream.data_len, payload.len() as u64, "payload length");
        assert_eq!(
            stream.length_source,
            crate::adapter::pdf::LengthSource::Direct
        );
        assert_eq!(
            &pdf[stream.data_start as usize..(stream.data_start + stream.data_len) as usize],
            payload,
            "stream span data must equal the payload exactly"
        );

        // The exact-replay court admits it and replays byte-for-byte.
        let plan = try_replay(payload, Limits::DEFAULT)
            .expect("committed zlib fixture must produce a replay plan");
        let mut rebuilt = Vec::new();
        rebuilt.extend_from_slice(&plan.header);
        rebuilt.extend_from_slice(&replay_raw(&plan.plaintext, &plan.corrections).unwrap());
        rebuilt.extend_from_slice(&plan.adler);
        assert_eq!(
            rebuilt, payload,
            "replay must reproduce the payload exactly"
        );
    }

    /// Regenerate the committed real-zlib fixture blobs. Ignored by default;
    /// run manually inside Docker:
    /// `cargo test --all-features regenerate_flate_fixtures -- --ignored`.
    #[test]
    #[ignore = "regenerates committed fixture blobs; run manually inside Docker"]
    fn regenerate_flate_fixtures() {
        use flate2::Compression;
        use flate2::write::ZlibEncoder;
        use std::io::Write;

        fn content_stream() -> Vec<u8> {
            let mut v = Vec::new();
            for i in 0..500u32 {
                v.extend_from_slice(
                    format!(
                        "BT /F1 12 Tf 72 {} Td (Invoice line {i:05} amount {}.{:02}) Tj ET\n",
                        720 - (i % 36) * 18,
                        100 + (i % 7) * 13,
                        (i * 37) % 100
                    )
                    .as_bytes(),
                );
            }
            v
        }
        fn graphics_stream() -> Vec<u8> {
            let mut v = Vec::new();
            for i in 0..400u32 {
                v.extend_from_slice(
                    format!(
                        "0 0 0 RG 1.5 w {} {} m {} {} l S\n",
                        i % 50,
                        (i * 3) % 400,
                        (i * 7) % 300,
                        (i * 11) % 350
                    )
                    .as_bytes(),
                );
            }
            v
        }
        fn incompressible(n: usize) -> Vec<u8> {
            let mut state = 0x9E37_79B9_7F4A_7C15u64;
            let mut out = Vec::with_capacity(n + 8);
            while out.len() < n {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                out.extend_from_slice(&state.to_le_bytes());
            }
            out.truncate(n);
            out
        }
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/adapter/pdf/fixtures");
        std::fs::create_dir_all(&dir).unwrap();
        let p1 = content_stream();
        let p2 = graphics_stream();
        let p3 = incompressible(8192);
        for (name, data, level) in [
            ("p1_l0.zlib", &p1, 0u32),
            ("p1_l1.zlib", &p1, 1),
            ("p1_l6.zlib", &p1, 6),
            ("p1_l9.zlib", &p1, 9),
            ("p2_l6.zlib", &p2, 6),
            ("p3_l6.zlib", &p3, 6),
        ] {
            let mut e = ZlibEncoder::new(Vec::new(), Compression::new(level));
            e.write_all(data).unwrap();
            std::fs::write(dir.join(name), e.finish().unwrap()).unwrap();
        }
    }
}
