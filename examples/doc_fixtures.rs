//! Dependency-free legacy Word 97–2003 (`.doc`, MS-DOC) fixture writer for the
//! Phase-26.1 court.
//!
//! Writes small, deterministic `.doc` compound files into a directory, so the legacy
//! Word court needs neither Microsoft Word, LibreOffice, `wvWare`, nor any network
//! access — only the shipped toolchain.
//!
//! ```sh
//! cargo run --locked --all-features --example doc_fixtures -- OUTDIR
//! ```
//!
//! Every fixture is a v3 (512-byte sector) CFBF compound file whose `WordDocument`,
//! `0Table`, and `1Table` streams live in the root mini stream (all are < 4096 bytes).
//! The generator emits:
//!
//! * a valid document whose `WordDocument` stream opens with a Word 97 FIB and whose
//!   piece table mixes a **compressed** (CP1252) piece, an **uncompressed** (UTF-16LE)
//!   piece, and a footnote piece, in **both** a `0Table` and a `1Table` variant (the
//!   non-selected table carries a decoy CLX so the `fWhichTblStm` flag is exercised);
//! * malformed controls: a bad `wIdent`, an implausible `nFib`, an out-of-range
//!   `fcClx`, a non-monotonic CP array, and a PCD `fc` outside the `WordDocument`
//!   stream;
//! * a generic compound file (no `WordDocument` stream) and prose.

#![cfg(feature = "doc")]
#![allow(dead_code)]

use std::path::Path;

const SIG: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const ENDOFCHAIN: u32 = 0xFFFF_FFFE;
const FATSECT: u32 = 0xFFFF_FFFD;
const FREESECT: u32 = 0xFFFF_FFFF;
const NOSTREAM: u32 = 0xFFFF_FFFF;
const MINI_SS: usize = 64;
const SECTOR: usize = 512;
const MINI_CUTOFF: u32 = 4096;

const FIB_MAGIC: u16 = 0xA5EC;
const NFIB_WORD97: u16 = 0x00C1;
const F_W_WHICH_TBL_STM: u16 = 0x0200;
const PCD_FCOMPRESSED: u32 = 0x4000_0000;

const FIB_LEN: usize = 898; // 38 + csw(14)*2 + cslw(22)*4 + cbRgFcLcb(93)*8
const TEXT_OFF: usize = FIB_LEN;
const CLX_OFF: u32 = 64; // the CLX sits after 64 bytes of table-stream filler

fn utf16le(name: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for u in name.encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

fn pad64(d: &[u8]) -> Vec<u8> {
    let mut v = d.to_vec();
    let r = v.len() % MINI_SS;
    if r != 0 {
        v.resize(v.len() + (MINI_SS - r), 0);
    }
    v
}

#[allow(clippy::too_many_arguments)]
fn dir_entry(
    name: &str,
    obj_type: u8,
    color: u8,
    left: u32,
    right: u32,
    child: u32,
    start: u32,
    size: u64,
) -> [u8; 128] {
    let mut e = [0u8; 128];
    let n = utf16le(name);
    assert!(n.len() <= 64);
    e[..n.len()].copy_from_slice(&n);
    e[64..66].copy_from_slice(&(n.len() as u16).to_le_bytes());
    e[66] = obj_type;
    e[67] = color;
    e[68..72].copy_from_slice(&left.to_le_bytes());
    e[72..76].copy_from_slice(&right.to_le_bytes());
    e[76..80].copy_from_slice(&child.to_le_bytes());
    e[116..120].copy_from_slice(&start.to_le_bytes());
    e[120..128].copy_from_slice(&size.to_le_bytes());
    e
}

/// One piece as placed in the fixture: `(character count, compressed, file offset)`.
#[derive(Clone, Copy)]
struct Piece {
    cp_len: u32,
    compressed: bool,
    fc: u32,
}

/// Build a valid CLX (a couple of `Prc` records, then the `Pcdt`).
fn clx_valid(pieces: &[Piece]) -> Vec<u8> {
    clx_with(pieces, false)
}

/// Build a CLX whose CP array is not monotonic.
fn clx_nonmonotonic(pieces: &[Piece]) -> Vec<u8> {
    clx_with(pieces, true)
}

fn clx_with(pieces: &[Piece], break_monotonic: bool) -> Vec<u8> {
    let mut out = Vec::new();
    // Two Prc records: an empty one and a 3-byte one.
    out.push(0x01);
    out.extend_from_slice(&0i16.to_le_bytes());
    out.push(0x01);
    out.extend_from_slice(&3i16.to_le_bytes());
    out.extend_from_slice(&[0xAA, 0xBB, 0xCC]);
    // The Pcdt.
    out.push(0x02);
    let n = pieces.len();
    let mut cps: Vec<u32> = Vec::with_capacity(n + 1);
    cps.push(0);
    let mut acc = 0u32;
    for p in pieces {
        acc += p.cp_len;
        cps.push(acc);
    }
    if break_monotonic && n >= 3 {
        // Swap entries 1 and 2 so the array decreases.
        cps.swap(1, 2);
    }
    let lcb = (4 * (n + 1) + 8 * n) as u32;
    out.extend_from_slice(&lcb.to_le_bytes());
    for cp in &cps {
        out.extend_from_slice(&cp.to_le_bytes());
    }
    for p in pieces {
        let mut pcd = [0u8; 8];
        let fc_raw = if p.compressed {
            (p.fc << 1) | PCD_FCOMPRESSED
        } else {
            p.fc
        };
        pcd[2..6].copy_from_slice(&fc_raw.to_le_bytes());
        out.extend_from_slice(&pcd);
    }
    out
}

/// Build a `WordDocument` stream: the FIB (with the given `fcClx`/`lcbClx` and section
/// counts) followed by the exact text bytes the pieces reference.
#[allow(clippy::too_many_arguments)]
fn build_word_document(
    ident: u16,
    n_fib: u16,
    table_1: bool,
    fc_clx: u32,
    lcb_clx: u32,
    ccp_text: u32,
    ccp_ftn: u32,
    text: &[u8],
) -> Vec<u8> {
    let mut wd = vec![0u8; FIB_LEN];
    // FibBase.
    wd[0..2].copy_from_slice(&ident.to_le_bytes());
    wd[2..4].copy_from_slice(&n_fib.to_le_bytes());
    wd[4..6].copy_from_slice(&0x00C1u16.to_le_bytes()); // nProduct
    wd[6..8].copy_from_slice(&0x0409u16.to_le_bytes()); // lid (en-US)
    wd[8..10].copy_from_slice(&0i16.to_le_bytes()); // pnNext
    let mut flags = 0u16;
    if table_1 {
        flags |= F_W_WHICH_TBL_STM;
    }
    wd[10..12].copy_from_slice(&flags.to_le_bytes());
    wd[12..14].copy_from_slice(&n_fib.to_le_bytes()); // nFibBack
    wd[24..28].copy_from_slice(&(TEXT_OFF as u32).to_le_bytes()); // fcMin
    wd[28..32].copy_from_slice(&((TEXT_OFF + text.len()) as u32).to_le_bytes()); // fcMac
    // csw = 14.
    wd[32..34].copy_from_slice(&14u16.to_le_bytes());
    // cslw = 22.
    wd[62..64].copy_from_slice(&22u16.to_le_bytes());
    // fibRgLw97 starts at 64: cbMac, reserved1, reserved2, ccpText, ccpFtn, ccpHdd, ...
    let rl = 64usize;
    wd[rl..rl + 4].copy_from_slice(&((TEXT_OFF + text.len()) as u32).to_le_bytes()); // cbMac
    wd[rl + 12..rl + 16].copy_from_slice(&ccp_text.to_le_bytes());
    wd[rl + 16..rl + 20].copy_from_slice(&ccp_ftn.to_le_bytes());
    // cbRgFcLcb = 93 (at 64 + 88 = 152).
    wd[152..154].copy_from_slice(&93u16.to_le_bytes());
    // fibRgFcLcb starts at 154; pair 33 is fcClx/lcbClx at 154 + 33*8 = 418.
    wd[418..422].copy_from_slice(&fc_clx.to_le_bytes());
    wd[422..426].copy_from_slice(&lcb_clx.to_le_bytes());
    // The text.
    wd.extend_from_slice(text);
    wd
}

/// The real (selected) and decoy text bytes and their piece offsets.
struct Layout {
    real_text: Vec<u8>,
    real_pieces: Vec<Piece>,
    decoy_text: Vec<u8>,
    decoy_piece: Piece,
    ccp_text: u32,
    ccp_ftn: u32,
}

/// The standard layout: a compressed piece, an uncompressed piece, a footnote, then
/// the decoy text.
fn layout() -> Layout {
    // The real text: "Hello, " (compressed) + "wörld\r" (uncompressed) + "footnote".
    let real_compressed = b"Hello, ";
    let real_uncompressed: Vec<u8> = "wörld\r"
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    let real_footnote = b"footnote";
    let mut real_text = Vec::new();
    let mut real_pieces = Vec::new();
    // piece 0 (compressed)
    let fc0 = TEXT_OFF as u32 + real_text.len() as u32;
    real_text.extend_from_slice(real_compressed);
    real_pieces.push(Piece {
        cp_len: real_compressed.len() as u32,
        compressed: true,
        fc: fc0,
    });
    // piece 1 (uncompressed)
    let fc1 = TEXT_OFF as u32 + real_text.len() as u32;
    real_text.extend_from_slice(&real_uncompressed);
    real_pieces.push(Piece {
        cp_len: (real_uncompressed.len() / 2) as u32,
        compressed: false,
        fc: fc1,
    });
    // piece 2 (compressed, the footnote section)
    let fc2 = TEXT_OFF as u32 + real_text.len() as u32;
    real_text.extend_from_slice(real_footnote);
    real_pieces.push(Piece {
        cp_len: real_footnote.len() as u32,
        compressed: true,
        fc: fc2,
    });
    let ccp_text = (real_compressed.len() + real_uncompressed.len() / 2) as u32;
    let ccp_ftn = real_footnote.len() as u32;

    // The decoy text (referenced only by the non-selected table stream).
    let decoy_text = b"DECOY!".to_vec();
    let decoy_piece = Piece {
        cp_len: decoy_text.len() as u32,
        compressed: true,
        fc: fc2 + real_footnote.len() as u32,
    };
    Layout {
        real_text,
        real_pieces,
        decoy_text,
        decoy_piece,
        ccp_text,
        ccp_ftn,
    }
}

/// Build a valid `.doc` fixture with the given memory layout.
fn build_valid(ident: u16, n_fib: u16, table_1: bool) -> Vec<u8> {
    let l = layout();
    let clx_real = clx_valid(&l.real_pieces);
    let clx_decoy = clx_valid(&[l.decoy_piece]);

    // Both text blocks live in the WordDocument stream (real then decoy).
    let mut text = l.real_text.clone();
    text.extend_from_slice(&l.decoy_text);
    let wd = build_word_document(
        ident,
        n_fib,
        table_1,
        CLX_OFF,
        clx_real.len() as u32,
        l.ccp_text,
        l.ccp_ftn,
        &text,
    );

    // The selected table carries the real CLX; the other carries the decoy.
    let mut real_table = vec![0u8; CLX_OFF as usize];
    real_table.extend_from_slice(&clx_real);
    let mut decoy_table = vec![0u8; CLX_OFF as usize];
    decoy_table.extend_from_slice(&clx_decoy);

    let (t0, t1) = if table_1 {
        (decoy_table, real_table)
    } else {
        (real_table, decoy_table)
    };
    build_cfbf(&[("WordDocument", wd), ("0Table", t0), ("1Table", t1)])
}

/// `fcClx` points past the end of the table stream.
fn build_bad_clx_range() -> Vec<u8> {
    let l = layout();
    let clx = clx_valid(&l.real_pieces);
    let mut text = l.real_text.clone();
    text.extend_from_slice(&l.decoy_text);
    // A huge fcClx (still a plausible u32).
    let wd = build_word_document(
        FIB_MAGIC,
        NFIB_WORD97,
        false,
        1_000_000,
        clx.len() as u32,
        l.ccp_text,
        l.ccp_ftn,
        &text,
    );
    build_cfbf(&[
        ("WordDocument", wd),
        ("0Table", table_stream(&clx)),
        ("1Table", table_stream(&clx)),
    ])
}

/// The CP array is not monotonic.
fn build_nonmonotonic() -> Vec<u8> {
    let l = layout();
    let clx = clx_nonmonotonic(&l.real_pieces);
    let mut text = l.real_text.clone();
    text.extend_from_slice(&l.decoy_text);
    let wd = build_word_document(
        FIB_MAGIC,
        NFIB_WORD97,
        false,
        CLX_OFF,
        clx.len() as u32,
        l.ccp_text,
        l.ccp_ftn,
        &text,
    );
    build_cfbf(&[
        ("WordDocument", wd),
        ("0Table", table_stream(&clx)),
        ("1Table", table_stream(&clx)),
    ])
}

/// A piece's `fc` lies outside the `WordDocument` stream.
fn build_piece_outside() -> Vec<u8> {
    let l = layout();
    // Move the footnote piece far past the end of the WordDocument stream.
    let mut pieces = l.real_pieces.clone();
    pieces[2].fc = 5_000_000;
    let clx = clx_valid(&pieces);
    let mut text = l.real_text.clone();
    text.extend_from_slice(&l.decoy_text);
    let wd = build_word_document(
        FIB_MAGIC,
        NFIB_WORD97,
        false,
        CLX_OFF,
        clx.len() as u32,
        l.ccp_text,
        l.ccp_ftn,
        &text,
    );
    build_cfbf(&[
        ("WordDocument", wd),
        ("0Table", table_stream(&clx)),
        ("1Table", table_stream(&clx)),
    ])
}

/// `fcMin`/`fcMac` (the legacy text bounds) lie outside the `WordDocument` stream.
fn build_fcmin_range() -> Vec<u8> {
    let l = layout();
    let clx = clx_valid(&l.real_pieces);
    let mut text = l.real_text.clone();
    text.extend_from_slice(&l.decoy_text);
    let mut wd = build_word_document(
        FIB_MAGIC,
        NFIB_WORD97,
        false,
        CLX_OFF,
        clx.len() as u32,
        l.ccp_text,
        l.ccp_ftn,
        &text,
    );
    // Corrupt `fcMin` (FibBase offset 24) to a value past the stream.
    wd[24..28].copy_from_slice(&9_000_000u32.to_le_bytes());
    build_cfbf(&[
        ("WordDocument", wd),
        ("0Table", table_stream(&clx)),
        ("1Table", table_stream(&clx)),
    ])
}

/// A table stream of `CLX_OFF` filler bytes followed by `clx`.
fn table_stream(clx: &[u8]) -> Vec<u8> {
    let mut t = vec![0u8; CLX_OFF as usize];
    t.extend_from_slice(clx);
    t
}

/// A generic compound file with no `WordDocument` stream.
fn build_generic_cfbf() -> Vec<u8> {
    build_cfbf(&[
        ("Data", b"not a word document".to_vec()),
        ("Meta", b"generic compound file".to_vec()),
    ])
}

/// Build a CFBF v3 compound file whose streams all live in the root mini stream.
fn build_cfbf(streams: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let epf = SECTOR / 4; // FAT entries per sector
    let epd = SECTOR / 128; // directory entries per sector

    // ---- mini stream layout -------------------------------------------------
    let mut mini_stream: Vec<u8> = Vec::new();
    let mut starts: Vec<usize> = Vec::new(); // mini-sector starts
    for (_, data) in streams {
        assert!(data.len() < MINI_CUTOFF as usize);
        starts.push(mini_stream.len() / MINI_SS);
        mini_stream.extend_from_slice(&pad64(data));
    }
    let mini_secs = mini_stream.len() / MINI_SS;
    let minifat_secs = mini_secs.div_ceil(epf);
    let mut minifat = vec![FREESECT; minifat_secs * epf];
    for (k, (_, data)) in streams.iter().enumerate() {
        let count = pad64(data).len() / MINI_SS;
        let s = starts[k];
        for j in 0..count {
            minifat[s + j] = if j + 1 < count {
                (s + j + 1) as u32
            } else {
                ENDOFCHAIN
            };
        }
    }

    let mini_stream_secs = mini_stream.len().div_ceil(SECTOR).max(1);
    let dir_count = 1 + streams.len();
    let dir_secs = dir_count.div_ceil(epd);
    let fat_secs = 1usize;

    let mini_stream_start = 0usize;
    let minifat_start = mini_stream_secs;
    let dir_start = minifat_start + minifat_secs;
    let fat_start = dir_start + dir_secs;
    let total = fat_start + fat_secs;
    assert!(total <= epf, "fixture is too large for one FAT sector");

    // ---- FAT ----------------------------------------------------------------
    let mut fat = vec![FREESECT; epf];
    for i in 0..mini_stream_secs {
        fat[mini_stream_start + i] = if i + 1 < mini_stream_secs {
            (mini_stream_start + i + 1) as u32
        } else {
            ENDOFCHAIN
        };
    }
    for i in 0..minifat_secs {
        fat[minifat_start + i] = if i + 1 < minifat_secs {
            (minifat_start + i + 1) as u32
        } else {
            ENDOFCHAIN
        };
    }
    for i in 0..dir_secs {
        fat[dir_start + i] = if i + 1 < dir_secs {
            (dir_start + i + 1) as u32
        } else {
            ENDOFCHAIN
        };
    }
    for i in 0..fat_secs {
        fat[fat_start + i] = FATSECT;
    }

    // ---- header -------------------------------------------------------------
    let mut out = vec![0u8; SECTOR];
    out[..8].copy_from_slice(&SIG);
    out[24..26].copy_from_slice(&0x003Eu16.to_le_bytes());
    out[26..28].copy_from_slice(&3u16.to_le_bytes()); // major 3
    out[28..30].copy_from_slice(&0xFFFEu16.to_le_bytes());
    out[30..32].copy_from_slice(&9u16.to_le_bytes()); // sector shift 9
    out[32..34].copy_from_slice(&6u16.to_le_bytes()); // mini shift 6
    out[40..44].copy_from_slice(&0u32.to_le_bytes()); // num dir sectors (v3 = 0)
    out[44..48].copy_from_slice(&(fat_secs as u32).to_le_bytes());
    out[48..52].copy_from_slice(&(dir_start as u32).to_le_bytes());
    out[56..60].copy_from_slice(&MINI_CUTOFF.to_le_bytes());
    out[60..64].copy_from_slice(&(minifat_start as u32).to_le_bytes());
    out[64..68].copy_from_slice(&(minifat_secs as u32).to_le_bytes());
    out[68..72].copy_from_slice(&ENDOFCHAIN.to_le_bytes()); // first DIFAT
    out[72..76].copy_from_slice(&0u32.to_le_bytes()); // num DIFAT sectors
    out[76..80].copy_from_slice(&(fat_start as u32).to_le_bytes()); // DIFAT[0]
    for i in 1..109 {
        out[76 + i * 4..80 + i * 4].copy_from_slice(&FREESECT.to_le_bytes());
    }

    // ---- sectors ------------------------------------------------------------
    let mut sectors: Vec<Vec<u8>> = Vec::new();
    for i in 0..mini_stream_secs {
        let mut s = vec![0u8; SECTOR];
        let off = i * SECTOR;
        let end = (off + SECTOR).min(mini_stream.len());
        s[..end - off].copy_from_slice(&mini_stream[off..end]);
        sectors.push(s);
    }
    for sec in 0..minifat_secs {
        let mut s = vec![0u8; SECTOR];
        for k in 0..epf {
            let idx = sec * epf + k;
            if idx >= minifat.len() {
                break;
            }
            s[k * 4..k * 4 + 4].copy_from_slice(&minifat[idx].to_le_bytes());
        }
        sectors.push(s);
    }

    // Directory: entry 0 is the root; the streams are a right-leaning chain child.
    let mut dirs: Vec<[u8; 128]> = Vec::new();
    dirs.push(dir_entry(
        "Root Entry",
        5,
        1,
        NOSTREAM,
        NOSTREAM,
        if streams.is_empty() { NOSTREAM } else { 1 },
        mini_stream_start as u32,
        mini_stream.len() as u64,
    ));
    for (k, (name, data)) in streams.iter().enumerate() {
        let right = if k + 1 < streams.len() {
            (k + 2) as u32
        } else {
            NOSTREAM
        };
        dirs.push(dir_entry(
            name,
            2,
            (k % 2) as u8,
            NOSTREAM,
            right,
            NOSTREAM,
            starts[k] as u32,
            data.len() as u64,
        ));
    }
    for sec in 0..dir_secs {
        let mut s = vec![0u8; SECTOR];
        for k in 0..epd {
            let idx = sec * epd + k;
            if idx >= dirs.len() {
                break;
            }
            let off = k * 128;
            s[off..off + 128].copy_from_slice(&dirs[idx]);
        }
        sectors.push(s);
    }
    for sec in 0..fat_secs {
        let mut s = vec![0u8; SECTOR];
        for k in 0..epf {
            let v = fat[sec * epf + k];
            s[k * 4..k * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        sectors.push(s);
    }
    for s in &sectors {
        out.extend_from_slice(s);
    }
    out
}

fn write(dir: &str, name: &str, bytes: &[u8]) {
    let path = Path::new(dir).join(name);
    std::fs::write(&path, bytes).expect("write fixture");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = args.get(1).expect("usage: doc_fixtures OUTDIR");
    std::fs::create_dir_all(dir).expect("create outdir");

    let doc0 = build_valid(FIB_MAGIC, NFIB_WORD97, false);
    let doc1 = build_valid(FIB_MAGIC, NFIB_WORD97, true);
    write(dir, "doc_0table.doc", &doc0);
    write(dir, "doc_1table.doc", &doc1);

    // A bad wIdent: detection declines (the source stays Opaque).
    write(
        dir,
        "doc_bad_ident.doc",
        &build_valid(0x0000, NFIB_WORD97, false),
    );

    // An implausible nFib (Word 6/95): detection declines.
    write(
        dir,
        "doc_bad_nfib.doc",
        &build_valid(FIB_MAGIC, 0x0065, false),
    );

    write(dir, "doc_bad_clx_range.doc", &build_bad_clx_range());
    write(dir, "doc_fcmin_range.doc", &build_fcmin_range());
    write(dir, "doc_nonmonotonic.doc", &build_nonmonotonic());
    write(dir, "doc_piece_outside.doc", &build_piece_outside());
    write(dir, "generic_cfbf.bin", &build_generic_cfbf());
    write(
        dir,
        "prose.txt",
        b"The quick brown fox jumps over the lazy dog.\nPlain prose, not a document.\n",
    );

    println!(
        "wrote doc fixtures (0table={} bytes, 1table={} bytes) into {dir}",
        doc0.len(),
        doc1.len()
    );
}
