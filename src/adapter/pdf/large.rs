//! Shared `pdf-make-large` corpus generator.
//!
//! This is the single deterministic generator behind the `pdf-make-large`
//! subcommand and the disk-durability court (`tests/observation_disk.rs`), so
//! both exercise byte-identical sources. It assembles a valid classic-xref PDF
//! in memory, recording each object's byte offset as it is written, so every
//! `/Length` and xref entry is correct by construction.
//!
//! It carries no compressor dependency: each stream is a real zlib (RFC 1950)
//! stream built from DEFLATE *stored* blocks, which `preflate` parses and the
//! replay lane admits. Every stream's plaintext is distinct (an index-seeded
//! LCG), so the shared-plaintext replay lanes cannot deduplicate them and the
//! random-access query court stays non-degenerate.

/// Assemble a large, deterministic, multi-object classic-xref PDF.
///
/// `objects` is the number of pages (and therefore `FlateDecode` streams); it
/// must be at least 1. `target_bytes` is a source-size floor: the per-stream
/// plaintext length is scaled (at least 1024 B) so the result reaches it
/// regardless of the object count. Object `1` is the catalog, `2` the page tree,
/// `3` a shared font; page/content pairs occupy objects `4..`.
pub fn large_pdf(objects: u64, target_bytes: u64) -> Vec<u8> {
    debug_assert!(objects >= 1, "large_pdf requires at least one object");
    let per_stream = target_bytes.div_ceil(objects).max(1024) as usize;
    let mut w = LargeWriter::new();
    w.text("%PDF-1.5\n");
    // Objects: 1 catalog, 2 pages, 3 shared font; then page/content pairs from 4.
    let mut kids = String::with_capacity(objects as usize * 10);
    for i in 0..objects {
        let page = 4 + 2 * i;
        kids.push_str(&format!("{page} 0 R "));
    }
    w.obj(1, 0, b"<< /Type /Catalog /Pages 2 0 R >>");
    w.obj(
        2,
        0,
        format!("<< /Type /Pages /Kids [{kids}] /Count {objects} >>").as_bytes(),
    );
    w.obj(
        3,
        0,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    );
    for i in 0..objects {
        let page = 4 + 2 * i;
        let content = page + 1;
        w.obj(
            page,
            0,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents {content} 0 R >>"
            )
            .as_bytes(),
        );
        let plaintext = distinct_stream_content(i, per_stream);
        w.stream_obj(
            content,
            0,
            " /Filter /FlateDecode",
            &zlib_stored(&plaintext),
        );
    }
    w.classic_trailer(4 + 2 * objects, " /Root 1 0 R");
    w.buf
}

/// Deterministic, stream-indexed content-stream plaintext.
///
/// The bytes come from an LCG seeded by the stream index, so no two streams share
/// a plaintext (the shared-plaintext replay lanes cannot deduplicate them) while
/// the content stays text-like and compressible. The result is at least
/// `min_len` bytes.
fn distinct_stream_content(index: u64, min_len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(min_len + 64);
    out.extend_from_slice(b"BT /F1 12 Tf 72 720 Td\n");
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15
        ^ index
            .wrapping_mul(0xD1B5_4A32_D192_ED03)
            .wrapping_add(0x0123_4567_89AB_CDEF);
    let mut line: u64 = 0;
    while out.len() < min_len {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let value = (state >> 32) as u32;
        out.extend_from_slice(format!("({index:06}:{line:06}:{value:08x}) Tj\n").as_bytes());
        line += 1;
    }
    out.extend_from_slice(b"ET\n");
    out
}

/// Wrap `data` in a real zlib (RFC 1950) stream using DEFLATE *stored* blocks.
///
/// Stored blocks keep the generator free of any compressor dependency and make
/// every stream trivially and exactly reproducible, while still being a
/// standards-valid zlib stream that `preflate` parses and the replay lane admits.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let block_overhead = (data.len() / 65_535 + 1) * 5;
    let mut out = Vec::with_capacity(data.len() + block_overhead + 6);
    out.push(0x78); // CMF: CM=8 (deflate), CINFO=7 (32 KiB window)
    out.push(0x01); // FLG: FLEVEL=0, FCHECK makes (0x78<<8|0x01) % 31 == 0
    if data.is_empty() {
        out.push(0x01); // BFINAL=1, BTYPE=00 (stored)
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0xffffu16.to_le_bytes());
    } else {
        let mut chunks = data.chunks(65_535).peekable();
        while let Some(chunk) = chunks.next() {
            let last = chunks.peek().is_none();
            out.push(if last { 0x01 } else { 0x00 });
            let len = chunk.len() as u16;
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&(!len).to_le_bytes());
            out.extend_from_slice(chunk);
        }
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

/// RFC 1950 Adler-32 checksum of `data`.
fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    // Largest run that cannot overflow a u32 accumulator before reduction.
    const NMAX: usize = 5_552;
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for chunk in data.chunks(NMAX) {
        for &byte in chunk {
            a += u32::from(byte);
            b += a;
        }
        a %= MOD;
        b %= MOD;
    }
    (b << 16) | a
}

/// Minimal append-only classic-xref PDF assembler that records each object's byte
/// offset, so every `/Length`, xref entry, and `startxref` is correct by
/// construction.
struct LargeWriter {
    buf: Vec<u8>,
    offsets: Vec<(u64, u64)>,
}

impl LargeWriter {
    fn new() -> Self {
        LargeWriter {
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

    fn offset_of(&self, number: u64) -> u64 {
        self.offsets
            .iter()
            .find(|&&(n, _)| n == number)
            .map(|&(_, off)| off)
            .unwrap_or_else(|| panic!("object {number} was never written"))
    }

    fn obj(&mut self, number: u64, generation: u64, body: &[u8]) {
        self.offsets.push((number, self.buf.len() as u64));
        self.text(&format!("{number} {generation} obj\n"));
        self.raw(body);
        self.raw(b"\nendobj\n");
    }

    fn stream_obj(&mut self, number: u64, generation: u64, extra_dict: &str, data: &[u8]) {
        self.offsets.push((number, self.buf.len() as u64));
        self.text(&format!(
            "{number} {generation} obj\n<< /Length {}{extra_dict} >>\nstream\n",
            data.len()
        ));
        self.raw(data);
        self.raw(b"\nendstream\nendobj\n");
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_zlib_round_trips_through_flate2() {
        use flate2::read::ZlibDecoder;
        use std::io::Read;
        for len in [0usize, 1, 100, 70_000, 140_000] {
            let data: Vec<u8> = (0..len).map(|i| (i * 31 + 7) as u8).collect();
            let z = zlib_stored(&data);
            let mut d = ZlibDecoder::new(&z[..]);
            let mut out = Vec::new();
            d.read_to_end(&mut out).unwrap();
            assert_eq!(out, data, "stored zlib round-trip failed at len={len}");
        }
    }

    #[test]
    fn adler32_matches_known_vector() {
        assert_eq!(adler32(b""), 1);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn large_pdf_is_a_well_formed_classic_pdf() {
        let pdf = large_pdf(4, 64 * 1024);
        assert!(pdf.starts_with(b"%PDF-1.5\n"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        assert!(pdf.windows(6).any(|w| w == b"stream"));
        // Distinct per-stream plaintext: streams must not be byte-identical.
        let a = distinct_stream_content(0, 4096);
        let b = distinct_stream_content(1, 4096);
        assert_ne!(a, b);
    }
}
