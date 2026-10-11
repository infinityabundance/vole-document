//! Phase 21.33 court fixture generator.
//!
//! Writes a small, deterministic set of ZIP + TAR fixtures (plus office-format and
//! malformed controls) into a directory, so the container court needs neither
//! python3, `zip`, nor a committed binary blob — only the shipped toolchain. The
//! generator is dependency-free: it writes the ZIP and TAR structures byte-for-byte
//! (the ground truth is therefore known exactly).
//!
//! ```sh
//! cargo run --locked --all-features --example container_fixtures -- OUTDIR
//! ```

#![cfg(feature = "container")]

use std::path::Path;
use std::process::ExitCode;

use vole_document::adapter::package::crc32_iso_hdlc;

fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// A valid single-block raw DEFLATE stream that stores `data` uncompressed.
fn stored_deflate(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 5);
    out.push(0x01); // BFINAL=1, BTYPE=00 (stored)
    let len = data.len() as u16;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&(!len).to_le_bytes());
    out.extend_from_slice(data);
    out
}

/// Build a ZIP from `(name, content, method)` triples (method 0 stored, 8 deflate).
fn build_zip(entries: &[(&[u8], &[u8], u16)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut offsets: Vec<u32> = Vec::new();
    let mut crcs: Vec<u32> = Vec::new();
    let mut payloads: Vec<Vec<u8>> = Vec::new();
    for (name, content, method) in entries {
        offsets.push(out.len() as u32);
        crcs.push(crc32_iso_hdlc(content));
        let payload = if *method == 8 {
            stored_deflate(content)
        } else {
            content.to_vec()
        };
        payloads.push(payload.clone());
        let mut flags = 0u16;
        if name.iter().any(|&b| b >= 0x80) {
            flags |= 0x0800;
        }
        put_u32(&mut out, 0x0403_4b50);
        put_u16(&mut out, 20);
        put_u16(&mut out, flags);
        put_u16(&mut out, *method);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0x0021);
        put_u32(&mut out, crcs[offsets.len() - 1]);
        put_u32(&mut out, payload.len() as u32);
        put_u32(&mut out, content.len() as u32);
        put_u16(&mut out, name.len() as u16);
        put_u16(&mut out, 0);
        out.extend_from_slice(name);
        out.extend_from_slice(&payload);
    }
    let cd_offset = out.len() as u32;
    for (i, (name, content, method)) in entries.iter().enumerate() {
        let mut flags = 0u16;
        if name.iter().any(|&b| b >= 0x80) {
            flags |= 0x0800;
        }
        put_u32(&mut out, 0x0201_4b50);
        put_u16(&mut out, 20);
        put_u16(&mut out, 20);
        put_u16(&mut out, flags);
        put_u16(&mut out, *method);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0x0021);
        put_u32(&mut out, crcs[i]);
        put_u32(&mut out, payloads[i].len() as u32);
        put_u32(&mut out, content.len() as u32);
        put_u16(&mut out, name.len() as u16);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u32(&mut out, 0);
        put_u32(&mut out, offsets[i]);
        out.extend_from_slice(name);
    }
    let cd_size = out.len() as u32 - cd_offset;
    put_u32(&mut out, 0x0605_4b50);
    put_u16(&mut out, 0);
    put_u16(&mut out, 0);
    put_u16(&mut out, entries.len() as u16);
    put_u16(&mut out, entries.len() as u16);
    put_u32(&mut out, cd_size);
    put_u32(&mut out, cd_offset);
    put_u16(&mut out, 0);
    out
}

fn put_octal(field: &mut [u8], v: u64) {
    let digits = field.len() - 1;
    let s = format!("{v:0>digits$o}");
    let bytes = s.as_bytes();
    field[..digits].copy_from_slice(&bytes[bytes.len() - digits..]);
    field[digits] = 0;
}

fn tar_header(
    name: &[u8],
    mode: u64,
    size: u64,
    typeflag: u8,
    linkname: &[u8],
    ustar: bool,
) -> Vec<u8> {
    let mut h = vec![0u8; 512];
    let n = name.len().min(100);
    h[..n].copy_from_slice(&name[..n]);
    put_octal(&mut h[100..108], mode);
    put_octal(&mut h[108..116], 0);
    put_octal(&mut h[116..124], 0);
    put_octal(&mut h[124..136], size);
    put_octal(&mut h[136..148], 0);
    for b in &mut h[148..156] {
        *b = b' ';
    }
    h[156] = typeflag;
    let ln = linkname.len().min(100);
    h[157..157 + ln].copy_from_slice(&linkname[..ln]);
    if ustar {
        h[257..263].copy_from_slice(b"ustar\0");
        h[263..265].copy_from_slice(b"00");
    }
    let sum: u64 = h.iter().map(|&b| b as u64).sum();
    let s = format!("{sum:06o}");
    h[148..154].copy_from_slice(s.as_bytes());
    h[154] = 0;
    h[155] = b' ';
    h
}

fn padded(size: u64) -> u64 {
    size.saturating_add(511) & !511
}

fn push_tar_member(
    out: &mut Vec<u8>,
    name: &[u8],
    typeflag: u8,
    content: &[u8],
    linkname: &[u8],
    ustar: bool,
) {
    out.extend_from_slice(&tar_header(
        name,
        if typeflag == b'5' { 0o755 } else { 0o644 },
        content.len() as u64,
        typeflag,
        linkname,
        ustar,
    ));
    out.extend_from_slice(content);
    let pad = padded(content.len() as u64) as usize - content.len();
    out.extend(std::iter::repeat_n(0u8, pad));
}

fn pax_record(key: &str, val: &str) -> Vec<u8> {
    let rest = format!(" {key}={val}\n");
    let mut len = rest.len() + 1;
    loop {
        let total = len.to_string().len() + rest.len();
        if total == len {
            break;
        }
        len = total;
    }
    let mut out = len.to_string().into_bytes();
    out.extend_from_slice(rest.as_bytes());
    out
}

fn write(dir: &Path, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(dir.join(name), bytes)
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let out_dir = args
        .next()
        .unwrap_or_else(|| "evidence/scratch/phase21-33/corpus".to_string());
    let dir = Path::new(&out_dir);
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("cannot create {out_dir}: {e}");
        return ExitCode::FAILURE;
    }

    // --- ZIP: one stored member ------------------------------------------------
    let zip_stored = build_zip(&[(b"hello.txt", b"hello zip world\n", 0)]);
    // --- ZIP: a stored + a deflated member -------------------------------------
    let zip_multi = build_zip(&[(b"a.txt", b"alpha\n", 0), (b"b.bin", &[7u8; 300], 8)]);
    // --- ZIP: a non-UTF-8 raw name (bytes >= 0x80; flag bit 11 set) ------------
    let zip_nonutf8 = build_zip(&[(b"caf\xe9/n\xf1.txt", b"body\n", 0)]);
    // --- ZIP: the empty archive ------------------------------------------------
    let zip_empty = build_zip(&[]);
    // --- ZIP: a bad EOCD (signature destroyed) ---------------------------------
    let mut bad_eocd = build_zip(&[(b"a.txt", b"a\n", 0)]);
    let n = bad_eocd.len();
    bad_eocd[n - 22..n - 18].copy_from_slice(b"XXXX");
    // --- ZIP: a truncated member (the file cut in half) ------------------------
    let good = build_zip(&[(b"a.txt", b"the payload bytes here", 0)]);
    let truncated_zip = good[..good.len() / 2].to_vec();

    // --- TAR: ustar with a file, a directory, and a nested file ----------------
    let mut tar_ustar = Vec::new();
    push_tar_member(
        &mut tar_ustar,
        b"hello.txt",
        b'0',
        b"hello tar world\n",
        b"",
        true,
    );
    push_tar_member(&mut tar_ustar, b"dir/", b'5', b"", b"", true);
    push_tar_member(
        &mut tar_ustar,
        b"dir/nested.bin",
        b'0',
        &[0u8, 1, 2, 3, 4, 5],
        b"",
        true,
    );
    tar_ustar.extend(std::iter::repeat_n(0u8, 1024));

    // --- TAR: GNU long name (`L`) + PAX (`x`) path + a symlink -----------------
    let long = b"this/is/a/very/long/member/name/that/does/not/fit/in/one/hundred/bytes/somewhere/deeper/still/further.txt";
    let mut tar_extended = Vec::new();
    {
        let mut namez = long.to_vec();
        namez.push(0);
        push_tar_member(&mut tar_extended, b"././@LongLink", b'L', &namez, b"", true);
    }
    push_tar_member(
        &mut tar_extended,
        &long[..100],
        b'0',
        b"payload-L\n",
        b"",
        true,
    );
    {
        let rec = pax_record("path", "pax/overridden-name.txt");
        push_tar_member(&mut tar_extended, b"paxheader", b'x', &rec, b"", true);
    }
    push_tar_member(
        &mut tar_extended,
        b"short-name",
        b'0',
        b"payload-X\n",
        b"",
        true,
    );
    push_tar_member(
        &mut tar_extended,
        b"link",
        b'2',
        b"",
        b"pax/overridden-name.txt",
        true,
    );
    tar_extended.extend(std::iter::repeat_n(0u8, 1024));

    // --- TAR: a bad header checksum (one non-checksum byte flipped) ------------
    let mut bad_checksum = tar_ustar.clone();
    bad_checksum[0] ^= 0x01;
    // --- TAR: a truncated header block -----------------------------------------
    let truncated_tar = tar_ustar[..600].to_vec();

    // --- Office controls (office ZIP family must be detected first) ------------
    let docx = build_zip(&[
        (
            b"[Content_Types].xml",
            br#"<Types><Override ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml" PartName="/word/document.xml"/></Types>"#,
            0,
        ),
        (
            b"_rels/.rels",
            br#"<Relationships><Relationship Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
            0,
        ),
        (b"word/document.xml", b"<w:document/>", 0),
    ]);
    let odt = build_zip(&[
        (b"mimetype", b"application/vnd.oasis.opendocument.text", 0),
        (b"content.xml", b"<office:document-content/>", 0),
    ]);

    // --- Opaque controls -------------------------------------------------------
    let mut pk_junk = b"PK\x03\x04".to_vec();
    pk_junk.extend(std::iter::repeat_n(0x41u8, 4096));
    let prose =
        b"The quick brown fox jumps over the lazy dog.\nPlain prose, not an archive.\n".to_vec();

    let fixtures: &[(&str, &[u8])] = &[
        ("zip_stored.zip", &zip_stored),
        ("zip_multi.zip", &zip_multi),
        ("zip_nonutf8.zip", &zip_nonutf8),
        ("zip_empty.zip", &zip_empty),
        ("tar_ustar.tar", &tar_ustar),
        ("tar_extended.tar", &tar_extended),
        ("office.docx", &docx),
        ("office.odt", &odt),
        ("pk_junk.bin", &pk_junk),
        ("prose.txt", &prose),
        ("bad_eocd.zip", &bad_eocd),
        ("bad_checksum.tar", &bad_checksum),
        ("truncated.zip", &truncated_zip),
        ("truncated.tar", &truncated_tar),
    ];
    for (name, bytes) in fixtures {
        if let Err(e) = write(dir, name, bytes) {
            eprintln!("cannot write {name}: {e}");
            return ExitCode::FAILURE;
        }
    }
    println!("wrote {} fixtures to {}", fixtures.len(), out_dir);
    ExitCode::SUCCESS
}
