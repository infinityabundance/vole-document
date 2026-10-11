//! Dependency-free CFBF/OLE2 fixture writer for the Phase-21.35 court.
//!
//! Writes small, deterministic compound files into a directory, so the
//! compound-file court needs neither `olevba`, `oletools`, LibreOffice, nor any
//! network access — only the shipped toolchain.
//!
//! ```sh
//! cargo run --locked --all-features --example cfbf_fixtures -- OUTDIR
//! ```
//!
//! The generator emits a valid v3 (512-byte sector) and v4 (4096-byte sector)
//! compound file with:
//!
//! * a **root storage** (entry 0) whose child is a red-black sibling tree;
//! * a **sub-storage** (`Sub`) with its own child stream;
//! * a **large stream** (`Large`, 5000 bytes) that is **FAT-chained** regular
//!   sectors (>= the 4096-byte mini cutoff);
//! * two **small streams** (`café` — a verbatim **non-ASCII** UTF-16LE name — and
//!   `Sub/Inner`) that are **MiniFAT-chained** mini-sectors inside the root mini
//!   stream;
//! * plus malformed controls (bad signature, bad sector shift, out-of-range directory
//!   sector, a cyclic FAT chain, a truncated stream chain, an invalid directory object
//!   type, a signature-only blob, and prose).

#![cfg(feature = "cfbf")]
#![allow(dead_code)]

use std::path::Path;

const SIG: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const ENDOFCHAIN: u32 = 0xFFFF_FFFE;
const FATSECT: u32 = 0xFFFF_FFFD;
const FREESECT: u32 = 0xFFFF_FFFF;
const NOSTREAM: u32 = 0xFFFF_FFFF;
const MINI_SS: usize = 64;

fn utf16le(name: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for u in name.encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out.extend_from_slice(&0u16.to_le_bytes()); // terminator
    out
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

fn pad64(d: &[u8]) -> Vec<u8> {
    let mut v = d.to_vec();
    let r = v.len() % MINI_SS;
    if r != 0 {
        v.resize(v.len() + (MINI_SS - r), 0);
    }
    v
}

/// Build the standard fixture with the given sector size (512 or 4096).
pub fn build(ss: u32) -> Vec<u8> {
    assert!(ss == 512 || ss == 4096);
    let shift: u16 = if ss == 512 { 9 } else { 12 };
    let major: u16 = if ss == 512 { 3 } else { 4 };
    let epf = (ss / 4) as usize; // FAT entries per sector
    let epd = (ss / 128) as usize; // directory entries per sector

    let large_data: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
    let small_data: Vec<u8> = (0..100u32).map(|i| (i * 7 % 253) as u8).collect();
    let inner_data: Vec<u8> = (0..40u32).map(|i| (i * 13 % 251) as u8).collect();

    let small_padded = pad64(&small_data);
    let inner_padded = pad64(&inner_data);
    let small_secs = small_padded.len() / MINI_SS;
    let inner_start = small_secs as u32;
    let mut mini_stream = Vec::new();
    mini_stream.extend_from_slice(&small_padded);
    mini_stream.extend_from_slice(&inner_padded);

    let mut minifat = vec![FREESECT; epf];
    for (i, slot) in minifat.iter_mut().enumerate().take(small_secs) {
        *slot = if i + 1 < small_secs {
            (i + 1) as u32
        } else {
            ENDOFCHAIN
        };
    }
    minifat[small_secs] = ENDOFCHAIN; // the single-sector `Inner` stream

    let large_secs = large_data.len().div_ceil(ss as usize);
    let dir_entries = 5usize;
    let dir_secs = dir_entries.div_ceil(epd);

    let large_start = 0usize;
    let minifat_start = large_secs;
    let mini_stream_start = large_secs + 1;
    let dir_start = large_secs + 2;
    let fat_start = dir_start + dir_secs;
    let total = fat_start + 1;

    let mut fat = vec![FREESECT; epf];
    for i in 0..large_secs {
        fat[large_start + i] = if i + 1 < large_secs {
            (large_start + i + 1) as u32
        } else {
            ENDOFCHAIN
        };
    }
    fat[minifat_start] = ENDOFCHAIN;
    fat[mini_stream_start] = ENDOFCHAIN;
    for i in 0..dir_secs {
        fat[dir_start + i] = if i + 1 < dir_secs {
            (dir_start + i + 1) as u32
        } else {
            ENDOFCHAIN
        };
    }
    fat[fat_start] = FATSECT;

    // ---- header (one sector) --------------------------------------------
    let mut out = vec![0u8; ss as usize];
    out[..8].copy_from_slice(&SIG);
    out[24..26].copy_from_slice(&0x003Eu16.to_le_bytes());
    out[26..28].copy_from_slice(&major.to_le_bytes());
    out[28..30].copy_from_slice(&0xFFFEu16.to_le_bytes());
    out[30..32].copy_from_slice(&shift.to_le_bytes());
    out[32..34].copy_from_slice(&6u16.to_le_bytes());
    let ndir = if major == 3 { 0u32 } else { dir_secs as u32 };
    out[40..44].copy_from_slice(&ndir.to_le_bytes());
    out[44..48].copy_from_slice(&1u32.to_le_bytes()); // num FAT sectors
    out[48..52].copy_from_slice(&(dir_start as u32).to_le_bytes());
    out[52..56].copy_from_slice(&0u32.to_le_bytes());
    out[56..60].copy_from_slice(&4096u32.to_le_bytes()); // mini-stream cutoff
    out[60..64].copy_from_slice(&(minifat_start as u32).to_le_bytes());
    out[64..68].copy_from_slice(&1u32.to_le_bytes()); // num MiniFAT sectors
    out[68..72].copy_from_slice(&ENDOFCHAIN.to_le_bytes()); // first DIFAT
    out[72..76].copy_from_slice(&0u32.to_le_bytes()); // num DIFAT sectors
    out[76..80].copy_from_slice(&(fat_start as u32).to_le_bytes()); // DIFAT[0]
    for i in 1..109 {
        out[76 + i * 4..80 + i * 4].copy_from_slice(&FREESECT.to_le_bytes());
    }

    // ---- sectors --------------------------------------------------------
    let mut sectors: Vec<Vec<u8>> = Vec::with_capacity(total);
    for i in 0..large_secs {
        let mut s = vec![0u8; ss as usize];
        let off = i * ss as usize;
        let end = (off + ss as usize).min(large_data.len());
        s[..end - off].copy_from_slice(&large_data[off..end]);
        sectors.push(s);
    }
    {
        let mut s = vec![0u8; ss as usize];
        for (i, &v) in minifat.iter().enumerate() {
            s[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        sectors.push(s);
    }
    {
        let mut s = vec![0u8; ss as usize];
        s[..mini_stream.len()].copy_from_slice(&mini_stream);
        sectors.push(s);
    }
    let dirs = [
        dir_entry(
            "Root Entry",
            5,
            1,
            NOSTREAM,
            NOSTREAM,
            2,
            mini_stream_start as u32,
            mini_stream.len() as u64,
        ),
        dir_entry("Sub", 1, 0, NOSTREAM, NOSTREAM, 4, ENDOFCHAIN, 0),
        dir_entry(
            "Large",
            2,
            1,
            1,
            3,
            NOSTREAM,
            large_start as u32,
            large_data.len() as u64,
        ),
        dir_entry(
            "caf\u{e9}",
            2,
            0,
            NOSTREAM,
            NOSTREAM,
            NOSTREAM,
            0,
            small_data.len() as u64,
        ),
        dir_entry(
            "Inner",
            2,
            1,
            NOSTREAM,
            NOSTREAM,
            NOSTREAM,
            inner_start,
            inner_data.len() as u64,
        ),
    ];
    for sec in 0..dir_secs {
        let mut s = vec![0u8; ss as usize];
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
    {
        let mut s = vec![0u8; ss as usize];
        for (i, &v) in fat.iter().enumerate() {
            s[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
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
    let dir = args.get(1).expect("usage: cfbf_fixtures OUTDIR");
    std::fs::create_dir_all(dir).expect("create outdir");

    let v3 = build(512);
    let v4 = build(4096);
    write(dir, "cfbf_v3.cfb", &v3);
    write(dir, "cfbf_v4.cfb", &v4);

    // v3 layout constants (see `build`): large = 10 sectors; directory sector 12;
    // FAT sector 14 (byte offset (14+1)*512 = 7680); MiniFAT sector 10.
    let dir_off = 13 * 512usize; // sector 12
    let fat_off = 15 * 512usize; // sector 14
    let _ = fat_off;

    // An invalid directory object type on entry 1 (`Sub`).
    let mut badtype = v3.clone();
    badtype[dir_off + 128 + 66] = 3;
    write(dir, "cfbf_badtype.cfb", &badtype);

    // A cyclic FAT chain for the `Large` stream (FAT[0] = 0).
    let mut cyclic = v3.clone();
    cyclic[fat_off..fat_off + 4].copy_from_slice(&0u32.to_le_bytes());
    write(dir, "cfbf_cyclic.cfb", &cyclic);

    // A truncated `Large` chain (ends after 3 of 10 required sectors).
    let mut trunc = v3.clone();
    trunc[fat_off..fat_off + 4].copy_from_slice(&1u32.to_le_bytes());
    trunc[fat_off + 4..fat_off + 8].copy_from_slice(&2u32.to_le_bytes());
    trunc[fat_off + 8..fat_off + 12].copy_from_slice(&ENDOFCHAIN.to_le_bytes());
    write(dir, "cfbf_truncated_stream.cfb", &trunc);

    // An out-of-range first directory sector (detection boundary).
    let mut bad_dir = v3.clone();
    bad_dir[48..52].copy_from_slice(&9999u32.to_le_bytes());
    write(dir, "cfbf_bad_dir_sector.cfb", &bad_dir);

    // A bad signature.
    let mut badsig = v3.clone();
    badsig[0] = 0x00;
    write(dir, "cfbf_badsig.cfb", &badsig);

    // A bad sector shift.
    let mut badshift = v3.clone();
    badshift[30..32].copy_from_slice(&7u16.to_le_bytes());
    write(dir, "cfbf_badshift.cfb", &badshift);

    // A signature-only blob (a zero header).
    let mut sig_only = vec![0u8; 512];
    sig_only[..8].copy_from_slice(&SIG);
    write(dir, "sig_only.bin", &sig_only);

    write(
        dir,
        "prose.txt",
        b"The quick brown fox jumps over the lazy dog.\nPlain prose, not a compound file.\n",
    );

    // The retained source copies for exactness comparison are those the court copies
    // into its own `ref/` directory; nothing else is needed here.
    println!(
        "wrote cfbf fixtures (v3={} bytes, v4={} bytes) into {dir}",
        v3.len(),
        v4.len()
    );
}
