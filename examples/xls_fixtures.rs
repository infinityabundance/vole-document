//! Phase 26.2 legacy Excel 97–2003 (`.xls`, BIFF8) court fixture generator.
//!
//! Writes small, deterministic `.xls` compound files byte-for-byte (no Excel,
//! LibreOffice, or OLE tooling, and no network): a `Workbook` stream with a valid BIFF8
//! `BOF`, a `BOUNDSHEET` inventory (two sheets), an `SST` with a `CONTINUE`-split
//! string, cells of each supported kind (`LABELSST`/`NUMBER`/`RK`/`BOOLERR`/
//! `FORMULA`+`STRING`/`BLANK`/`MULBLANK`), plus malformed controls (a bad `BOF`
//! version, an unrecognized substream type, a truncated record, an `SST` count mismatch,
//! an out-of-range `BOUNDSHEET` position, a malformed `CONTINUE`), a generic compound
//! file, and prose.
//!
//! Usage: `xls_fixtures OUTDIR`

use std::path::Path;

const SIG: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const ENDOFCHAIN: u32 = 0xFFFF_FFFE;
const FATSECT: u32 = 0xFFFF_FFFD;
const FREESECT: u32 = 0xFFFF_FFFF;
const NOSTREAM: u32 = 0xFFFF_FFFF;
const MINI_SS: usize = 64;
const SECTOR: usize = 512;
const MINI_CUTOFF: u32 = 4096;

const REC_EOF: u16 = 0x000A;
const REC_FORMULA: u16 = 0x0006;
const REC_CONTINUE: u16 = 0x003C;
const REC_BOUNDSHEET: u16 = 0x0085;
const REC_MULBLANK: u16 = 0x00BE;
const REC_XF: u16 = 0x00E0;
const REC_SST: u16 = 0x00FC;
const REC_LABELSST: u16 = 0x00FD;
const REC_DIMENSIONS: u16 = 0x0200;
const REC_BLANK: u16 = 0x0201;
const REC_NUMBER: u16 = 0x0203;
const REC_BOOLERR: u16 = 0x0205;
const REC_STRING: u16 = 0x0207;
const REC_ROW: u16 = 0x0208;
const REC_RK: u16 = 0x027E;
const REC_FORMAT: u16 = 0x041E;
const REC_BOF: u16 = 0x0809;

const BIFF8_VERSION: u16 = 0x0600;
const ST_GLOBALS: u16 = 0x0005;
const ST_WORKSHEET: u16 = 0x0010;

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

fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn push_rec(out: &mut Vec<u8>, id: u16, payload: &[u8]) {
    out.extend_from_slice(&id.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    out.extend_from_slice(payload);
}

fn bof_payload(version: u16, subtype: u16) -> Vec<u8> {
    let mut p = vec![0u8; 16];
    p[0..2].copy_from_slice(&version.to_le_bytes());
    p[2..4].copy_from_slice(&subtype.to_le_bytes());
    p[4..6].copy_from_slice(&0x0DBBu16.to_le_bytes());
    p[6..8].copy_from_slice(&0x07CCu16.to_le_bytes());
    p[12..14].copy_from_slice(&BIFF8_VERSION.to_le_bytes());
    p
}

fn boundsheet(pos: u32, name: &str, compressed: bool) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&pos.to_le_bytes());
    p.push(0); // hsState: visible
    p.push(0); // dt: worksheet
    let u = units(name);
    p.push(u.len() as u8);
    p.push(if compressed { 0x00 } else { 0x01 });
    for x in u {
        if compressed {
            p.push(x as u8);
        } else {
            p.extend_from_slice(&x.to_le_bytes());
        }
    }
    p
}

fn push_xl_string(out: &mut Vec<u8>, text: &[u16], compressed: bool) {
    out.extend_from_slice(&(text.len() as u16).to_le_bytes());
    out.push(if compressed { 0x00 } else { 0x01 });
    for &x in text {
        if compressed {
            out.push(x as u8);
        } else {
            out.extend_from_slice(&x.to_le_bytes());
        }
    }
}

/// The `SST` record payload plus its `CONTINUE` payload. The third string is
/// `CONTINUE`-split: its character array begins in the `SST` and continues in the
/// `CONTINUE` (whose first byte is the string's option flag).
fn sst_records(declared_unique: u32, malformed: bool) -> (Vec<u8>, Vec<u8>) {
    let mut sst = Vec::new();
    sst.extend_from_slice(&3u32.to_le_bytes()); // cstTotal
    sst.extend_from_slice(&declared_unique.to_le_bytes()); // cstUnique
    push_xl_string(&mut sst, &units("Alpha"), true);
    push_xl_string(&mut sst, &units("Ωmega"), false);
    let s2: Vec<u8> = b"Continued".to_vec();
    sst.extend_from_slice(&(s2.len() as u16).to_le_bytes());
    sst.push(0x00);
    sst.extend_from_slice(&s2[..4]);
    let mut cont = Vec::new();
    cont.push(0x00);
    if malformed {
        cont.extend_from_slice(&s2[4..6]);
    } else {
        cont.extend_from_slice(&s2[4..]);
    }
    (sst, cont)
}

fn labelsst(row: u16, col: u16, isst: u32) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&row.to_le_bytes());
    p.extend_from_slice(&col.to_le_bytes());
    p.extend_from_slice(&0u16.to_le_bytes());
    p.extend_from_slice(&isst.to_le_bytes());
    p
}

fn number(row: u16, col: u16, v: f64) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&row.to_le_bytes());
    p.extend_from_slice(&col.to_le_bytes());
    p.extend_from_slice(&0u16.to_le_bytes());
    p.extend_from_slice(&v.to_bits().to_le_bytes());
    p
}

fn rk(row: u16, col: u16, raw: u32) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&row.to_le_bytes());
    p.extend_from_slice(&col.to_le_bytes());
    p.extend_from_slice(&0u16.to_le_bytes());
    p.extend_from_slice(&raw.to_le_bytes());
    p
}

fn boolerr(row: u16, col: u16, value: u8, is_error: bool) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&row.to_le_bytes());
    p.extend_from_slice(&col.to_le_bytes());
    p.extend_from_slice(&0u16.to_le_bytes());
    p.push(value);
    p.push(is_error as u8);
    p
}

fn blank(row: u16, col: u16) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&row.to_le_bytes());
    p.extend_from_slice(&col.to_le_bytes());
    p.extend_from_slice(&0u16.to_le_bytes());
    p
}

fn mulblank(row: u16, cols: &[u16]) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&row.to_le_bytes());
    p.extend_from_slice(&cols[0].to_le_bytes());
    p.extend_from_slice(&0u16.to_le_bytes());
    for (i, &c) in cols.iter().enumerate() {
        let v = if i + 1 == cols.len() { c | 0x0100 } else { c };
        p.extend_from_slice(&v.to_le_bytes());
    }
    p
}

fn formula_string(row: u16, col: u16) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&row.to_le_bytes());
    p.extend_from_slice(&col.to_le_bytes());
    p.extend_from_slice(&0u16.to_le_bytes());
    p.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF]);
    p.extend_from_slice(&0u16.to_le_bytes());
    p.extend_from_slice(&0u32.to_le_bytes());
    p
}

fn string_rec(s: &str) -> Vec<u8> {
    let u = units(s);
    let mut p = Vec::new();
    p.extend_from_slice(&(u.len() as u16).to_le_bytes());
    p.push(0x00);
    for x in u {
        p.push(x as u8);
    }
    p
}

fn dimensions() -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&0u32.to_le_bytes()); // rwMic
    p.extend_from_slice(&10u32.to_le_bytes()); // rwMac
    p.extend_from_slice(&0u16.to_le_bytes()); // colMic
    p.extend_from_slice(&5u16.to_le_bytes()); // colMac
    p.extend_from_slice(&0u16.to_le_bytes()); // reserved
    p
}

fn row_rec() -> Vec<u8> {
    vec![0u8; 16]
}

fn format_rec() -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&0u16.to_le_bytes()); // ifmt
    push_xl_string(&mut p, &units("General"), true);
    p
}

/// The workbook fixture options.
struct Opts {
    bof_version: u16,
    bof_subtype: u16,
    declared_unique: u32,
    bad_boundsheet_pos: bool,
    truncate: bool,
    malformed_continue: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            bof_version: BIFF8_VERSION,
            bof_subtype: ST_GLOBALS,
            declared_unique: 3,
            bad_boundsheet_pos: false,
            truncate: false,
            malformed_continue: false,
        }
    }
}

/// Build the `Workbook` stream.
fn workbook(o: &Opts) -> Vec<u8> {
    let mut w: Vec<u8> = Vec::new();
    // workbook globals
    push_rec(&mut w, REC_BOF, &bof_payload(o.bof_version, o.bof_subtype));
    let alpha_bs = w.len();
    push_rec(&mut w, REC_BOUNDSHEET, &boundsheet(0, "Alpha", true));
    let beta_bs = w.len();
    push_rec(&mut w, REC_BOUNDSHEET, &boundsheet(0, "Beta", true));
    let (sst, cont) = sst_records(o.declared_unique, o.malformed_continue);
    push_rec(&mut w, REC_SST, &sst);
    push_rec(&mut w, REC_CONTINUE, &cont);
    push_rec(&mut w, REC_XF, &[0u8; 20]);
    push_rec(&mut w, REC_FORMAT, &format_rec());
    push_rec(&mut w, REC_EOF, &[]);

    // sheet 1: Alpha
    let p1 = w.len() as u32;
    push_rec(&mut w, REC_BOF, &bof_payload(BIFF8_VERSION, ST_WORKSHEET));
    push_rec(&mut w, REC_DIMENSIONS, &dimensions());
    push_rec(&mut w, REC_LABELSST, &labelsst(0, 0, 0));
    push_rec(&mut w, REC_NUMBER, &number(1, 0, 2.5));
    push_rec(&mut w, REC_RK, &rk(2, 0, (7u32 << 2) | 0x02));
    push_rec(&mut w, REC_BOOLERR, &boolerr(3, 0, 1, false));
    push_rec(&mut w, REC_FORMULA, &formula_string(4, 0));
    push_rec(&mut w, REC_STRING, &string_rec("calc"));
    push_rec(&mut w, REC_BLANK, &blank(5, 0));
    push_rec(&mut w, REC_MULBLANK, &mulblank(6, &[1, 2]));
    push_rec(&mut w, REC_ROW, &row_rec());
    push_rec(&mut w, REC_EOF, &[]);

    // sheet 2: Beta
    let p2 = w.len() as u32;
    push_rec(&mut w, REC_BOF, &bof_payload(BIFF8_VERSION, ST_WORKSHEET));
    push_rec(&mut w, REC_LABELSST, &labelsst(0, 0, 1));
    push_rec(&mut w, REC_RK, &rk(1, 0, 0x3FF8_0000));
    // References the `CONTINUE`-split shared string (index 2).
    push_rec(&mut w, REC_LABELSST, &labelsst(2, 0, 2));
    let last_eof = w.len();
    push_rec(&mut w, REC_EOF, &[]);

    let pos_a = if o.bad_boundsheet_pos { 999_999 } else { p1 };
    let pos_b = if o.bad_boundsheet_pos { 999_999 } else { p2 };
    w[alpha_bs + 4..alpha_bs + 8].copy_from_slice(&pos_a.to_le_bytes());
    w[beta_bs + 4..beta_bs + 8].copy_from_slice(&pos_b.to_le_bytes());
    if o.truncate {
        // Declare the final record longer than the stream holds.
        w[last_eof + 2..last_eof + 4].copy_from_slice(&200u16.to_le_bytes());
    }
    w
}

fn build_xls(o: &Opts) -> Vec<u8> {
    build_cfbf(&[("Workbook", workbook(o))])
}

/// A generic compound file with no `Workbook`/`Book` stream.
fn build_generic_cfbf() -> Vec<u8> {
    build_cfbf(&[
        ("Data", b"not an excel workbook".to_vec()),
        ("Meta", b"generic compound file".to_vec()),
    ])
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

fn write(dir: &str, name: &str, bytes: &[u8]) {
    let path = Path::new(dir).join(name);
    std::fs::write(&path, bytes).expect("write fixture");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = args.get(1).expect("usage: xls_fixtures OUTDIR");
    std::fs::create_dir_all(dir).expect("create outdir");

    let valid = build_xls(&Opts::default());
    write(dir, "xls_valid.xls", &valid);

    // A bad BOF version (BIFF5): detection declines.
    write(
        dir,
        "xls_bad_bof.xls",
        &build_xls(&Opts {
            bof_version: 0x0500,
            ..Opts::default()
        }),
    );

    // An unrecognized substream type: detection declines.
    write(
        dir,
        "xls_bad_type.xls",
        &build_xls(&Opts {
            bof_subtype: 0x0001,
            ..Opts::default()
        }),
    );

    // A leading globals BOF but a truncated record: parses typed decline.
    write(
        dir,
        "xls_truncated.xls",
        &build_xls(&Opts {
            truncate: true,
            ..Opts::default()
        }),
    );

    // An SST whose declared unique count disagrees with its data.
    write(
        dir,
        "xls_bad_sst_count.xls",
        &build_xls(&Opts {
            declared_unique: 5,
            ..Opts::default()
        }),
    );

    // An out-of-range BOUNDSHEET position.
    write(
        dir,
        "xls_bad_boundsheet.xls",
        &build_xls(&Opts {
            bad_boundsheet_pos: true,
            ..Opts::default()
        }),
    );

    // A `CONTINUE`-split string whose continuation is too short.
    write(
        dir,
        "xls_malformed_continue.xls",
        &build_xls(&Opts {
            malformed_continue: true,
            ..Opts::default()
        }),
    );

    write(dir, "generic_cfbf.bin", &build_generic_cfbf());
    write(
        dir,
        "prose.txt",
        b"The quick brown fox jumps over the lazy dog.\nPlain prose, not a spreadsheet.\n",
    );

    println!(
        "wrote xls fixtures (valid={} bytes) into {dir}",
        valid.len()
    );
}
