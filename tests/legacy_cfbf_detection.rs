//! Phase 27.3 — detection-hardening regressions for the legacy CFBF family.
//!
//! Two real-world `.ppt`/`.xls` shapes that the v2 stratified court recorded as
//! `cfbf` (a finding) are fixed here:
//!
//! * a `.ppt`/`.xls` whose `PowerPoint Document`/`Workbook` stream chain is
//!   **fragmented** (its content is not one contiguous span, so the CFBF
//!   `stream_bytes` declines) — the detector now reads only the stream **header**
//!   from the first chunk (`cfbf::stream_leading`), which is where the `Document`
//!   record / BIFF `BOF` lives;
//! * a `.xls` whose workbook stream is named `WORKBOOK` (uppercase) — the
//!   well-known name is matched ASCII-case-insensitively.
//!
//! The fixtures are tiny synthetic compound files (v3, 512-byte sectors) whose
//! stream chain is deliberately fragmented by placing the directory sector in the
//! middle of the stream's sector list.

use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::limits::Limits;

const SS: usize = 512;
const ENDOFCHAIN: u32 = 0xFFFF_FFFE;
const FATSECT: u32 = 0xFFFF_FFFD;
const FREESECT: u32 = 0xFFFF_FFFF;
const NOSTREAM: u32 = 0xFFFF_FFFF;

/// A 128-byte directory entry (UTF-16LE name, NUL-terminated).
fn dir_entry(name: &str, obj_type: u8, child: u32, start: u32, size: u64) -> [u8; 128] {
    let mut e = [0u8; 128];
    let mut units: Vec<u16> = name.encode_utf16().collect();
    units.push(0);
    let bytes: Vec<u8> = units.iter().flat_map(|u| u.to_le_bytes()).collect();
    e[..bytes.len()].copy_from_slice(&bytes);
    e[64..66].copy_from_slice(&(bytes.len() as u16).to_le_bytes());
    e[66] = obj_type;
    e[67] = 1;
    // left/right/child default to NOSTREAM.
    e[68..72].copy_from_slice(&NOSTREAM.to_le_bytes());
    e[72..76].copy_from_slice(&NOSTREAM.to_le_bytes());
    e[76..80].copy_from_slice(&child.to_le_bytes());
    e[116..120].copy_from_slice(&start.to_le_bytes());
    e[120..128].copy_from_slice(&size.to_le_bytes());
    e
}

/// Build a minimal v3 compound file holding one regular stream `name` of `content`
/// whose sector chain is **fragmented**.
///
/// Layout (512-byte sectors): sector 0 = the stream's first sector, sector 1 = the
/// directory, sectors 2.. = the rest of the stream, then one FAT sector. The stream
/// size is exactly 4096 (`>=` the 4096 mini cutoff), so it is a regular stream; the
/// chain jumps `0 -> 2`, so `contiguous == false`.
fn build_fragmented(name: &str, head: &[u8; 8]) -> Vec<u8> {
    const LEN: usize = 4096;
    let need = LEN / SS; // 8 sectors
    // Stream sector indices: 0, then 2..need (sector 1 is the directory).
    let mut stream_sectors = vec![0u32];
    stream_sectors.extend(2u32..2 + (need as u32 - 1));
    let dir_sec = 1u32;
    let fat_sec = 2 + (need as u32 - 1); // one past the last stream sector
    let total = (fat_sec + 1) as usize;

    let mut fat = vec![FREESECT; SS / 4];
    for w in stream_sectors.windows(2) {
        fat[w[0] as usize] = w[1];
    }
    fat[*stream_sectors.last().unwrap() as usize] = ENDOFCHAIN;
    fat[dir_sec as usize] = ENDOFCHAIN;
    fat[fat_sec as usize] = FATSECT;

    let mut out = vec![0u8; SS];
    out[..8].copy_from_slice(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]);
    out[24..26].copy_from_slice(&0x003Eu16.to_le_bytes());
    out[26..28].copy_from_slice(&3u16.to_le_bytes());
    out[28..30].copy_from_slice(&0xFFFEu16.to_le_bytes());
    out[30..32].copy_from_slice(&9u16.to_le_bytes());
    out[32..34].copy_from_slice(&6u16.to_le_bytes());
    out[40..44].copy_from_slice(&0u32.to_le_bytes()); // num dir sectors (v3)
    out[44..48].copy_from_slice(&1u32.to_le_bytes()); // one FAT sector
    out[48..52].copy_from_slice(&dir_sec.to_le_bytes());
    out[56..60].copy_from_slice(&4096u32.to_le_bytes()); // mini cutoff
    out[60..64].copy_from_slice(&ENDOFCHAIN.to_le_bytes());
    out[64..68].copy_from_slice(&0u32.to_le_bytes());
    out[68..72].copy_from_slice(&ENDOFCHAIN.to_le_bytes());
    out[72..76].copy_from_slice(&0u32.to_le_bytes());
    out[76..80].copy_from_slice(&fat_sec.to_le_bytes()); // DIFAT[0]
    for i in 1..109 {
        out[76 + i * 4..80 + i * 4].copy_from_slice(&FREESECT.to_le_bytes());
    }

    let mut sectors: Vec<Vec<u8>> = vec![vec![0u8; SS]; total];
    // Stream content, written into its (fragmented) sectors in order.
    let mut chunk = vec![0u8; SS];
    chunk[..8].copy_from_slice(head);
    for (i, &s) in stream_sectors.iter().enumerate() {
        sectors[s as usize] = if i == 0 { chunk.clone() } else { vec![0u8; SS] };
    }
    // Directory sector.
    let root = dir_entry("Root Entry", 5, 1, ENDOFCHAIN, 0);
    let stream = dir_entry(name, 2, NOSTREAM, 0, LEN as u64);
    let mut d = vec![0u8; SS];
    d[..128].copy_from_slice(&root);
    d[128..256].copy_from_slice(&stream);
    sectors[dir_sec as usize] = d;
    // FAT sector.
    let mut f = vec![0u8; SS];
    for (i, &v) in fat.iter().enumerate() {
        f[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    sectors[fat_sec as usize] = f;

    for s in &sectors {
        out.extend_from_slice(s);
    }
    out
}

#[test]
fn fragmented_powerpoint_document_stream_is_detected() {
    // The `Document` record: recVer 0xF, recType 1000.
    let src = build_fragmented("PowerPoint Document", &[0x0F, 0x00, 0xE8, 0x03, 0, 0, 0, 0]);
    assert_eq!(
        detect_document_format(&src, Limits::DEFAULT),
        DocumentFormat::Ppt,
        "a fragmented PowerPoint Document stream must still detect as Ppt"
    );
}

#[test]
fn fragmented_workbook_stream_is_detected() {
    // A BIFF8 `BOF` (id 0x0809, version 0x0600, substream 0x0005).
    let src = build_fragmented(
        "Workbook",
        &[0x09, 0x08, 0x10, 0x00, 0x00, 0x06, 0x05, 0x00],
    );
    assert_eq!(
        detect_document_format(&src, Limits::DEFAULT),
        DocumentFormat::Xls,
        "a fragmented Workbook stream must still detect as Xls"
    );
}

#[test]
fn uppercase_workbook_stream_name_is_detected() {
    // Some writers uppercase the well-known stream name.
    let src = build_fragmented(
        "WORKBOOK",
        &[0x09, 0x08, 0x10, 0x00, 0x00, 0x06, 0x05, 0x00],
    );
    assert_eq!(
        detect_document_format(&src, Limits::DEFAULT),
        DocumentFormat::Xls,
        "a `WORKBOOK` stream with a BIFF8 BOF must detect as Xls"
    );
}

#[test]
fn a_fragmented_stream_without_the_header_still_declines() {
    // The same fragmented shape but with junk in the first chunk stays at the
    // generic CFBF physical layer: the header prefix is what is inspected.
    let src = build_fragmented("PowerPoint Document", &[0xDE, 0xAD, 0xBE, 0xEF, 0, 0, 0, 0]);
    assert_eq!(
        detect_document_format(&src, Limits::DEFAULT),
        DocumentFormat::Cfbf
    );
}
