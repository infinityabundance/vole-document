//! Phase 26.3 legacy PowerPoint 97–2003 (`.ppt`, MS-PPT) court fixture generator.
//!
//! Dependency-free: it writes small, deterministic `.ppt` compound files (a
//! `PowerPoint Document` record stream with a valid `Document` container, a
//! `SlideListWithText` with two `SlidePersistAtom` slides, a `TextCharsAtom` and a
//! `TextBytesAtom` run, a `Current User` stream, plus malformed and non-PowerPoint
//! controls) byte-for-byte, so the court needs no PowerPoint, LibreOffice, or OLE
//! tooling and no network.
//!
//! This file mirrors `examples/xls_fixtures.rs`: the CFBF v3 writer is identical, and
//! only the `PowerPoint Document` payload differs.
//!
//! ```sh
//! cargo run --locked --all-features --example ppt_fixtures -- CORPUS_DIR
//! ```

use std::path::Path;

// ---------------------------------------------------------------------------
// CFBF v3 writer (all streams live in the root mini stream)
// ---------------------------------------------------------------------------

const SIG: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const ENDOFCHAIN: u32 = 0xFFFF_FFFE;
const FATSECT: u32 = 0xFFFF_FFFD;
const FREESECT: u32 = 0xFFFF_FFFF;
const NOSTREAM: u32 = 0xFFFF_FFFF;
const MINI_SS: usize = 64;
const SECTOR: usize = 512;
const MINI_CUTOFF: u32 = 4096;

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

/// Build a CFBF v3 compound file whose streams all live in the root mini stream.
fn build_cfbf(streams: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let epf = SECTOR / 4;
    let epd = SECTOR / 128;

    let mut mini_stream: Vec<u8> = Vec::new();
    let mut starts: Vec<usize> = Vec::new();
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

    let mut out = vec![0u8; SECTOR];
    out[..8].copy_from_slice(&SIG);
    out[24..26].copy_from_slice(&0x003Eu16.to_le_bytes());
    out[26..28].copy_from_slice(&3u16.to_le_bytes());
    out[28..30].copy_from_slice(&0xFFFEu16.to_le_bytes());
    out[30..32].copy_from_slice(&9u16.to_le_bytes());
    out[32..34].copy_from_slice(&6u16.to_le_bytes());
    out[40..44].copy_from_slice(&0u32.to_le_bytes());
    out[44..48].copy_from_slice(&(fat_secs as u32).to_le_bytes());
    out[48..52].copy_from_slice(&(dir_start as u32).to_le_bytes());
    out[56..60].copy_from_slice(&MINI_CUTOFF.to_le_bytes());
    out[60..64].copy_from_slice(&(minifat_start as u32).to_le_bytes());
    out[64..68].copy_from_slice(&(minifat_secs as u32).to_le_bytes());
    out[68..72].copy_from_slice(&ENDOFCHAIN.to_le_bytes());
    out[72..76].copy_from_slice(&0u32.to_le_bytes());
    out[76..80].copy_from_slice(&(fat_start as u32).to_le_bytes());
    for i in 1..109 {
        out[76 + i * 4..80 + i * 4].copy_from_slice(&FREESECT.to_le_bytes());
    }

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

// ---------------------------------------------------------------------------
// MS-PPT record stream writer
// ---------------------------------------------------------------------------

const REC_VER_CONTAINER: u8 = 0x0F;
const REC_DOCUMENT: u16 = 1000;
const REC_END_DOCUMENT: u16 = 1001;
const REC_SLIDE_PERSIST_ATOM: u16 = 1011;
const REC_TEXT_HEADER_ATOM: u16 = 3999;
const REC_TEXT_CHARS_ATOM: u16 = 4000;
const REC_TEXT_BYTES_ATOM: u16 = 4008;
const REC_USER_EDIT_ATOM: u16 = 4085;
const REC_SLIDE_LIST_WITH_TEXT: u16 = 4080;

const CHARS_TEXT: &str = "Hello Ω";
/// CP1252 bytes for "Café €" (`é` = 0xE9, `€` = 0x80).
const BYTES_TEXT: &[u8] = b"Caf\xE9 \x80";

fn rec(out: &mut Vec<u8>, rec_ver: u8, rec_instance: u16, rec_type: u16, payload: &[u8]) {
    let w: u16 = (rec_ver as u16) | (rec_instance << 4);
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&rec_type.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
}

fn utf16_units(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for u in s.encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out
}

/// A `SlidePersistAtom` payload: `persistIdRef`, `cTexts`, then 8 reserved bytes.
fn slide_persist_atom(persist_id_ref: u32, c_texts: u32) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&persist_id_ref.to_le_bytes());
    p.extend_from_slice(&c_texts.to_le_bytes());
    p.extend_from_slice(&[0u8; 8]);
    p
}

/// The fixture options (all default to a *valid* presentation).
#[derive(Clone, Copy, Default)]
struct Opts {
    /// The first record is not a `Document` (recVer `0`, recType `1001`) → detection
    /// declines, so the source stays `cfbf`.
    bad_first_record: bool,
    /// The `Document` container's `recLen` is smaller than its children → a child
    /// overruns its container (a bad `Document` record).
    bad_document_len: bool,
    /// The `Document` container's `recLen` overruns the stream.
    overrun_document: bool,
    /// The `TextCharsAtom` payload length is odd (a text-atom/`recLen` mismatch).
    odd_chars_atom: bool,
    /// Append a 3-byte partial record header after the `Document` container.
    trailing_truncated: bool,
    /// The `Current User` stream's `UserEditAtom` offset is out of range.
    bad_current_user: bool,
    /// Omit the `Current User` stream entirely.
    no_current_user: bool,
}

/// Build the `PowerPoint Document` stream.
fn powerpoint_document(o: &Opts) -> Vec<u8> {
    // Document children.
    let mut doc_payload: Vec<u8> = Vec::new();

    // A single SlideListWithText container (recInstance 0 = the slides list).
    let mut slwt: Vec<u8> = Vec::new();
    // Slide 0 (persistIdRef 1): a TextCharsAtom run.
    rec(
        &mut slwt,
        0,
        0,
        REC_SLIDE_PERSIST_ATOM,
        &slide_persist_atom(1, 1),
    );
    rec(&mut slwt, 0, 0, REC_TEXT_HEADER_ATOM, &[0u8; 4]);
    let mut chars = utf16_units(CHARS_TEXT);
    if o.odd_chars_atom {
        chars.push(0);
    }
    rec(&mut slwt, 0, 0, REC_TEXT_CHARS_ATOM, &chars);
    // Slide 1 (persistIdRef 2): a TextBytesAtom run.
    rec(
        &mut slwt,
        0,
        0,
        REC_SLIDE_PERSIST_ATOM,
        &slide_persist_atom(2, 1),
    );
    rec(&mut slwt, 0, 0, REC_TEXT_HEADER_ATOM, &[0u8; 4]);
    rec(&mut slwt, 0, 0, REC_TEXT_BYTES_ATOM, BYTES_TEXT);

    rec(
        &mut doc_payload,
        REC_VER_CONTAINER,
        0,
        REC_SLIDE_LIST_WITH_TEXT,
        &slwt,
    );
    // EndDocument.
    rec(&mut doc_payload, 0, 0, REC_END_DOCUMENT, &[]);

    // The Document container.
    let mut stream: Vec<u8> = Vec::new();
    let (rec_ver, rec_type) = if o.bad_first_record {
        (0u8, REC_END_DOCUMENT)
    } else {
        (REC_VER_CONTAINER, REC_DOCUMENT)
    };
    let w: u16 = rec_ver as u16;
    let doc_len: u32 = if o.overrun_document {
        0xFFFF_FFFF
    } else if o.bad_document_len {
        4
    } else {
        doc_payload.len() as u32
    };
    stream.extend_from_slice(&w.to_le_bytes());
    stream.extend_from_slice(&rec_type.to_le_bytes());
    stream.extend_from_slice(&doc_len.to_le_bytes());
    stream.extend_from_slice(&doc_payload);
    if o.trailing_truncated {
        stream.extend_from_slice(&[0u8, 0u8, 0u8]);
    }
    stream
}

/// Build the `Current User` stream: a 4-byte offset, 4 bytes of padding, then a
/// `UserEditAtom` header.
fn current_user(bad: bool) -> Vec<u8> {
    let mut s: Vec<u8> = Vec::new();
    let offset: u32 = if bad { 1000 } else { 8 };
    s.extend_from_slice(&offset.to_le_bytes());
    s.extend_from_slice(&[0u8; 4]);
    // The UserEditAtom header at offset 8.
    let w: u16 = 0; // recVer 0
    s.extend_from_slice(&w.to_le_bytes());
    s.extend_from_slice(&REC_USER_EDIT_ATOM.to_le_bytes());
    s.extend_from_slice(&0u32.to_le_bytes());
    s
}

fn build_ppt(o: &Opts) -> Vec<u8> {
    let doc = powerpoint_document(o);
    if o.no_current_user {
        build_cfbf(&[("PowerPoint Document", doc)])
    } else {
        build_cfbf(&[
            ("PowerPoint Document", doc),
            ("Current User", current_user(o.bad_current_user)),
        ])
    }
}

/// A generic compound file with no `PowerPoint Document` stream.
fn build_generic_cfbf() -> Vec<u8> {
    build_cfbf(&[
        ("Data", b"not a powerpoint document".to_vec()),
        ("Meta", b"generic compound file".to_vec()),
    ])
}

fn write(dir: &str, name: &str, bytes: &[u8]) {
    let path = Path::new(dir).join(name);
    std::fs::write(&path, bytes).expect("write fixture");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = args.get(1).expect("usage: ppt_fixtures OUTDIR");
    std::fs::create_dir_all(dir).expect("create outdir");

    let valid = build_ppt(&Opts::default());
    write(dir, "ppt_valid.ppt", &valid);

    // A first record that is not a `Document` container: detection declines → `cfbf`.
    write(
        dir,
        "ppt_bad_first_record.ppt",
        &build_ppt(&Opts {
            bad_first_record: true,
            ..Opts::default()
        }),
    );

    // A `Document` container whose declared length is too small: a child overruns it.
    write(
        dir,
        "ppt_bad_document.ppt",
        &build_ppt(&Opts {
            bad_document_len: true,
            ..Opts::default()
        }),
    );

    // A `Document` container whose declared length overruns the stream.
    write(
        dir,
        "ppt_overrun.ppt",
        &build_ppt(&Opts {
            overrun_document: true,
            ..Opts::default()
        }),
    );

    // A `TextCharsAtom` with an odd payload length.
    write(
        dir,
        "ppt_text_mismatch.ppt",
        &build_ppt(&Opts {
            odd_chars_atom: true,
            ..Opts::default()
        }),
    );

    // A trailing partial record header.
    write(
        dir,
        "ppt_truncated.ppt",
        &build_ppt(&Opts {
            trailing_truncated: true,
            ..Opts::default()
        }),
    );

    // A `Current User` stream whose `UserEditAtom` offset is out of range.
    write(
        dir,
        "ppt_bad_current_user.ppt",
        &build_ppt(&Opts {
            bad_current_user: true,
            ..Opts::default()
        }),
    );

    write(dir, "generic_cfbf.bin", &build_generic_cfbf());
    write(
        dir,
        "prose.txt",
        b"The quick brown fox jumps over the lazy dog.\nPlain prose, not a presentation.\n",
    );

    println!(
        "wrote ppt fixtures (valid={} bytes) into {dir}",
        valid.len()
    );
}
