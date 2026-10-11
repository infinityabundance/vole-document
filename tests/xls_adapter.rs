//! Phase 26.2 court: the legacy Excel 97–2003 (`.xls`, BIFF8) semantic adapter built on
//! the CFBF/OLE2 physical layer.
//!
//! Gated on `xls`, so a build without the feature compiles an empty target.
//!
//! A `.xls` has no package layer: the exact leaf is the whole source (a
//! `DocumentExact`) and every derived observation is a bounded, representation-
//! preserving projection of it. The court checks:
//!
//! 1. **Byte-based detection** and the boundaries: a `.xls` → `Xls`; a generic
//!    compound file → `Cfbf`; a `.xls` with a bad `BOF` version or an unrecognized
//!    substream type → `Cfbf`; prose → `Opaque`.
//! 2. **Representation preservation**: the BIFF8 `BOF`/version, the ordered sheets
//!    (`BOUNDSHEET` name + `BOF` stream position), the `SST` (including a
//!    `CONTINUE`-split string), and each cell of every supported kind
//!    (`LABELSST`/`NUMBER`/`RK`/`BOOLERR`/`FORMULA`+`STRING`/`BLANK`/`MULBLANK`) with
//!    its row/column, exact record span, and exact stored value spelling (the `RK`
//!    compact numeric encoding is decoded).
//! 3. **Selectors**: native `xls-biff`/`xls-sheet`/`xls-cell`/`xls-record`/`xls-find`
//!    plus common `metadata`/`text`/`search-match`.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted.
//! 5. **Fail closed**: malformed inputs decline typed (a truncated record, an `SST`
//!    count mismatch, an out-of-range `BOUNDSHEET` position, a malformed `CONTINUE`);
//!    random bytes never panic.
//! 6. **No cross-field aliasing** (ADR-0060): the model node depends on the exact
//!    `sha256(source)` root.

#![cfg(all(feature = "field", feature = "xls"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_XLS_MODEL, SelectorKey, lookup};
use vole_document::field::ingest::{IngestReport, ingest_pdf};
use vole_document::field::node::NodeKind;
use vole_document::field::observe::{ObserveRequest, Representation, Selector, observe};
use vole_document::field::provenance::{AnswerValue, FieldAnswer};
use vole_document::field::{Field, FieldId, FieldStore};
use vole_document::integrity::sha256;
use vole_document::limits::Limits;
use vole_document::store::NodeId;

// ---------------------------------------------------------------------------
// Dependency-free `.xls` (BIFF8-in-CFBF) writer (test ground truth)
// ---------------------------------------------------------------------------

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

/// The shared-string-table strings (in order).
const SST0: &str = "Alpha";
const SST1: &str = "Ωmega";
const SST2: &str = "Continued";
/// The cached `FORMULA` string result.
const FORMULA_TEXT: &str = "calc";

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
    p.push(0);
    p.push(0);
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

fn sst_records(declared_unique: u32, malformed: bool) -> (Vec<u8>, Vec<u8>) {
    let mut sst = Vec::new();
    sst.extend_from_slice(&3u32.to_le_bytes());
    sst.extend_from_slice(&declared_unique.to_le_bytes());
    push_xl_string(&mut sst, &units(SST0), true);
    push_xl_string(&mut sst, &units(SST1), false);
    let s2: Vec<u8> = SST2.bytes().collect();
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
    p.extend_from_slice(&0u32.to_le_bytes());
    p.extend_from_slice(&10u32.to_le_bytes());
    p.extend_from_slice(&0u16.to_le_bytes());
    p.extend_from_slice(&5u16.to_le_bytes());
    p.extend_from_slice(&0u16.to_le_bytes());
    p
}

fn format_rec() -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&0u16.to_le_bytes());
    push_xl_string(&mut p, &units("General"), true);
    p
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Valid,
    BadBof,
    BadType,
    Truncated,
    BadSstCount,
    BadBoundsheet,
    MalformedContinue,
}

fn workbook(kind: Kind) -> Vec<u8> {
    let mut w: Vec<u8> = Vec::new();
    let (bof_version, bof_subtype) = match kind {
        Kind::BadBof => (0x0500, ST_GLOBALS),
        Kind::BadType => (BIFF8_VERSION, 0x0001),
        _ => (BIFF8_VERSION, ST_GLOBALS),
    };
    push_rec(&mut w, REC_BOF, &bof_payload(bof_version, bof_subtype));
    let alpha_bs = w.len();
    push_rec(&mut w, REC_BOUNDSHEET, &boundsheet(0, "Alpha", true));
    let beta_bs = w.len();
    push_rec(&mut w, REC_BOUNDSHEET, &boundsheet(0, "Beta", true));
    let declared = if kind == Kind::BadSstCount { 5 } else { 3 };
    let (sst, cont) = sst_records(declared, kind == Kind::MalformedContinue);
    push_rec(&mut w, REC_SST, &sst);
    push_rec(&mut w, REC_CONTINUE, &cont);
    push_rec(&mut w, REC_XF, &[0u8; 20]);
    push_rec(&mut w, REC_FORMAT, &format_rec());
    push_rec(&mut w, REC_EOF, &[]);

    let p1 = w.len() as u32;
    push_rec(&mut w, REC_BOF, &bof_payload(BIFF8_VERSION, ST_WORKSHEET));
    push_rec(&mut w, REC_DIMENSIONS, &dimensions());
    push_rec(&mut w, REC_LABELSST, &labelsst(0, 0, 0));
    push_rec(&mut w, REC_NUMBER, &number(1, 0, 2.5));
    push_rec(&mut w, REC_RK, &rk(2, 0, (7u32 << 2) | 0x02));
    push_rec(&mut w, REC_BOOLERR, &boolerr(3, 0, 1, false));
    push_rec(&mut w, REC_FORMULA, &formula_string(4, 0));
    push_rec(&mut w, REC_STRING, &string_rec(FORMULA_TEXT));
    push_rec(&mut w, REC_BLANK, &blank(5, 0));
    push_rec(&mut w, REC_MULBLANK, &mulblank(6, &[1, 2]));
    push_rec(&mut w, REC_ROW, &[0u8; 16]);
    push_rec(&mut w, REC_EOF, &[]);

    let p2 = w.len() as u32;
    push_rec(&mut w, REC_BOF, &bof_payload(BIFF8_VERSION, ST_WORKSHEET));
    push_rec(&mut w, REC_LABELSST, &labelsst(0, 0, 1));
    push_rec(&mut w, REC_RK, &rk(1, 0, 0x3FF8_0000));
    push_rec(&mut w, REC_LABELSST, &labelsst(2, 0, 2));
    let last_eof = w.len();
    push_rec(&mut w, REC_EOF, &[]);

    let pos_a = if kind == Kind::BadBoundsheet {
        999_999
    } else {
        p1
    };
    let pos_b = if kind == Kind::BadBoundsheet {
        999_999
    } else {
        p2
    };
    w[alpha_bs + 4..alpha_bs + 8].copy_from_slice(&pos_a.to_le_bytes());
    w[beta_bs + 4..beta_bs + 8].copy_from_slice(&pos_b.to_le_bytes());
    if kind == Kind::Truncated {
        w[last_eof + 2..last_eof + 4].copy_from_slice(&200u16.to_le_bytes());
    }
    w
}

fn build(kind: Kind) -> Vec<u8> {
    build_cfbf(&[("Workbook", workbook(kind))])
}

fn build_generic_cfbf() -> Vec<u8> {
    build_cfbf(&[("Data", b"not an excel workbook".to_vec())])
}

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
    assert!(total <= epf);

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
// Harness
// ---------------------------------------------------------------------------

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-xls-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn opaque_descriptor(source: &[u8]) -> Vec<u8> {
    let d = Descriptor {
        universe: vole_document::container::UNIVERSE.to_string(),
        source_format: vole_document::SOURCE_FORMAT_OPAQUE,
        format_basis: "opaque:xls-test".to_string(),
        models: vec![],
        channels: vec![],
        objects: vec![ObjectSource::Inline(source.to_vec())],
        program: Program::new(vec![Op::EmitObject { object_id: 0 }]),
        observation_index: None,
        seek_directory: false,
        checkpoints: None,
        source_sha256: sha256(source),
        source_len: source.len() as u64,
    };
    d.serialize().unwrap().0
}

struct Fixture {
    root: PathBuf,
    store: FieldStore,
    report: IngestReport,
    source: Vec<u8>,
}

impl Fixture {
    fn new(label: &str, source: &[u8]) -> Fixture {
        let root = temp_dir(label);
        let mut store = FieldStore::open(&root).unwrap();
        let descriptor = opaque_descriptor(source);
        let report = ingest_pdf(&mut store, &descriptor, Limits::DEFAULT).unwrap();
        Fixture {
            root,
            store,
            report,
            source: source.to_vec(),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

fn observe_ok(
    store: &mut FieldStore,
    field: &FieldId,
    selector: Selector,
    representation: Representation,
) -> FieldAnswer {
    let req = ObserveRequest::new(selector, representation);
    observe(store, field, &req, Limits::DEFAULT).unwrap().0
}

fn observe_err(
    store: &mut FieldStore,
    field: &FieldId,
    selector: Selector,
    representation: Representation,
) -> ErrorClass {
    let req = ObserveRequest::new(selector, representation);
    observe(store, field, &req, Limits::DEFAULT)
        .unwrap_err()
        .class()
}

fn answer_text(answer: &FieldAnswer) -> String {
    match &answer.value {
        AnswerValue::Text(t) => t.clone(),
        other => panic!("expected text, got {other:?}"),
    }
}

fn answer_json(answer: &FieldAnswer) -> String {
    match &answer.value {
        AnswerValue::Json(j) => j.clone(),
        other => panic!("expected json, got {other:?}"),
    }
}

fn answer_bytes(answer: &FieldAnswer) -> Vec<u8> {
    match &answer.value {
        AnswerValue::Bytes(b) => b.clone(),
        other => panic!("expected bytes, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_byte_based_and_conservative() {
    // A real `.xls` is `Xls`.
    assert_eq!(
        detect_document_format(&build(Kind::Valid), Limits::DEFAULT),
        DocumentFormat::Xls
    );
    // A bad BOF version (BIFF5) → a valid compound file → `Cfbf`.
    assert_eq!(
        detect_document_format(&build(Kind::BadBof), Limits::DEFAULT),
        DocumentFormat::Cfbf
    );
    // An unrecognized BOF substream type → `Cfbf`.
    assert_eq!(
        detect_document_format(&build(Kind::BadType), Limits::DEFAULT),
        DocumentFormat::Cfbf
    );
    // A generic compound file (no Workbook stream) → `Cfbf`.
    assert_eq!(
        detect_document_format(&build_generic_cfbf(), Limits::DEFAULT),
        DocumentFormat::Cfbf
    );
    // Prose → `Opaque`.
    assert_eq!(
        detect_document_format(
            b"The quick brown fox jumps over the lazy dog.\nPlain prose.\n",
            Limits::DEFAULT
        ),
        DocumentFormat::Opaque
    );
    // A `.xlsx` (an OPC ZIP) never reaches the CFBF detector.
    let zip = b"PK\x03\x04\x14\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
    assert_ne!(
        detect_document_format(zip, Limits::DEFAULT),
        DocumentFormat::Xls
    );
}

#[test]
fn biff_header_sheets_sst_and_cells_are_preserved() {
    let src = build(Kind::Valid);
    let mut fx = Fixture::new("surface", &src);
    let field = fx.report.field;

    // Common metadata names the format and the BIFF version.
    let common = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(common.contains("\"format\":\"xls\""), "{common}");
    assert!(common.contains("\"version\":\"biff8\""), "{common}");
    assert!(common.contains("\"biff_version\":1536"), "{common}");
    assert!(common.contains("\"stream\":\"Workbook\""), "{common}");
    assert!(
        common.contains("\"root_substream\":\"globals\""),
        "{common}"
    );
    assert!(common.contains("\"sheets\":2"), "{common}");
    assert!(common.contains("\"sst_unique\":3"), "{common}");
    assert!(common.contains("\"sst_strings\":3"), "{common}");
    assert!(common.contains("\"cells\":11"), "{common}");

    // The native BIFF selector reports the recorded version.
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::XlsBiff,
            Representation::Text,
        )),
        "biff8"
    );

    // Sheet order and names are preserved. `Alpha` is sheet 0, `Beta` sheet 1.
    let s0 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::XlsSheet { index: 0 },
        Representation::Metadata,
    ));
    assert!(s0.contains("\"name\":\"Alpha\""), "{s0}");
    assert!(s0.contains("\"type\":\"worksheet\""), "{s0}");
    assert!(s0.contains("\"cells\":[0,8]"), "{s0}");
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::XlsSheet { index: 1 },
            Representation::Text,
        )),
        "Beta"
    );

    // Cell 0: `LABELSST` resolving SST string 0 ("Alpha"), at row 0 col 0.
    let c0 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::XlsCell { index: 0 },
        Representation::Metadata,
    ));
    assert!(c0.contains("\"kind\":\"labelsst\""), "{c0}");
    assert!(c0.contains("\"row\":0"), "{c0}");
    assert!(c0.contains("\"col\":0"), "{c0}");
    assert!(c0.contains(&format!("\"text\":\"{SST0}\"")), "{c0}");

    // Cell 1: `NUMBER` 2.5, at row 1.
    let c1 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::XlsCell { index: 1 },
        Representation::Metadata,
    ));
    assert!(c1.contains("\"kind\":\"number\""), "{c1}");
    assert!(c1.contains("\"row\":1"), "{c1}");
    assert!(c1.contains("\"num\":2.5"), "{c1}");

    // Cell 2: `RK` decoding a 30-bit signed integer (7), at row 2.
    let c2 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::XlsCell { index: 2 },
        Representation::Metadata,
    ));
    assert!(c2.contains("\"kind\":\"rk\""), "{c2}");
    assert!(c2.contains("\"row\":2"), "{c2}");
    assert!(c2.contains("\"rk\":30"), "{c2}");
    assert!(c2.contains("\"value\":7"), "{c2}");

    // Cell 3: `BOOLERR` boolean TRUE.
    let c3 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::XlsCell { index: 3 },
        Representation::Metadata,
    ));
    assert!(c3.contains("\"kind\":\"boolerr\""), "{c3}");
    assert!(c3.contains("\"value\":1"), "{c3}");
    assert!(c3.contains("\"error\":false"), "{c3}");

    // Cell 4: `FORMULA` with its cached `STRING` result.
    let c4 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::XlsCell { index: 4 },
        Representation::Metadata,
    ));
    assert!(c4.contains("\"kind\":\"formula\""), "{c4}");
    assert!(c4.contains(&format!("\"text\":\"{FORMULA_TEXT}\"")), "{c4}");

    // Cells 5/6: `BLANK` and the two `MULBLANK` columns.
    let c5 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::XlsCell { index: 5 },
        Representation::Metadata,
    ));
    assert!(c5.contains("\"kind\":\"blank\""), "{c5}");
    let c6 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::XlsCell { index: 6 },
        Representation::Metadata,
    ));
    assert!(c6.contains("\"kind\":\"mulblank\""), "{c6}");
    assert!(c6.contains("\"col\":1"), "{c6}");
    let c7 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::XlsCell { index: 7 },
        Representation::Metadata,
    ));
    assert!(c7.contains("\"col\":2"), "{c7}");

    // Cell 10: the `CONTINUE`-split shared string (index 2) resolves correctly.
    let c10 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::XlsCell { index: 10 },
        Representation::Metadata,
    ));
    assert!(c10.contains(&format!("\"text\":\"{SST2}\"")), "{c10}");

    // An out-of-range cell/sheet/record index declines typed.
    for sel in [
        Selector::XlsCell { index: 99 },
        Selector::XlsSheet { index: 9 },
        Selector::XlsRecord { index: 9_999 },
    ] {
        assert_eq!(
            observe_err(&mut fx.store, &field, sel, Representation::Metadata),
            ErrorClass::UnsupportedFeature
        );
    }

    // The native `xls-record` selector exposes the exact BIFF record ids and spans.
    let rec0 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::XlsRecord { index: 0 },
        Representation::Metadata,
    ));
    assert!(rec0.contains("\"id_hex\":\"0x0809\""), "{rec0}");
    // Record 3 is the `SST`.
    let rec3 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::XlsRecord { index: 3 },
        Representation::Metadata,
    ));
    assert!(rec3.contains("\"id_hex\":\"0x00FC\""), "{rec3}");
    let rec_bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        Selector::XlsRecord { index: 0 },
        Representation::ExactBytes,
    ));
    assert_eq!(rec_bytes.len(), 4 + 16);
    assert_eq!(&rec_bytes[..2], &REC_BOF.to_le_bytes());

    // `xls-find` finds a sheet name and a shared string.
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::XlsFind {
            pattern: "Alpha".to_string(),
        },
        Representation::Metadata,
    ));
    assert!(found.contains("\"field\":\"sheet\""), "{found}");
    assert!(found.contains("\"field\":\"string\""), "{found}");

    // The common text carries the sheet names and their strings.
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert!(text.contains("Alpha"), "{text}");
    assert!(text.contains(SST1), "{text}");
    assert!(text.contains(SST2), "{text}");

    // The common search-match agrees with the native find.
    let common_find = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("Alpha".to_string()),
        Representation::Text,
    ));
    assert!(common_find.contains("\"matches\":[{"), "{common_find}");
}

#[test]
fn malformed_inputs_decline_typed() {
    for (kind, label) in [
        (Kind::Truncated, "truncated"),
        (Kind::BadSstCount, "bad-sst-count"),
        (Kind::BadBoundsheet, "bad-boundsheet"),
        (Kind::MalformedContinue, "malformed-continue"),
    ] {
        let src = build(kind);
        assert_eq!(
            detect_document_format(&src, Limits::DEFAULT),
            DocumentFormat::Xls,
            "{label}"
        );
        let e = vole_document::adapter::xls::parse(&src, Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), ErrorClass::InvalidXlsStructure, "{label}");
    }

    // A bad BOF / substream type is not detected as `xls` at all.
    for kind in [Kind::BadBof, Kind::BadType] {
        assert_ne!(
            detect_document_format(&build(kind), Limits::DEFAULT),
            DocumentFormat::Xls
        );
    }
}

#[test]
fn exact_materialization_is_byte_identical() {
    let src = build(Kind::Valid);
    let fx = Fixture::new("exact", &src);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let out = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(out.len(), src.len());
    assert_eq!(sha256(&out), sha256(&src));
    assert_eq!(out, fx.source);
}

#[test]
fn removal_then_fresh_process_is_queryable_and_exact() {
    let root = temp_dir("removal");
    let src_path = root.join("book.xls");
    let desc_path = root.join("book.voldoc");
    let source = build(Kind::Valid);
    std::fs::write(&src_path, &source).unwrap();
    let descriptor = opaque_descriptor(&source);
    std::fs::write(&desc_path, &descriptor).unwrap();

    let store_root = root.join("store");
    let field_id = {
        let mut store = FieldStore::open(&store_root).unwrap();
        let report = ingest_pdf(&mut store, &descriptor, Limits::DEFAULT).unwrap();
        report.field
    };

    std::fs::remove_file(&src_path).unwrap();
    std::fs::remove_file(&desc_path).unwrap();

    let mut store2 = FieldStore::open(&store_root).unwrap();
    assert_eq!(
        answer_text(&observe_ok(
            &mut store2,
            &field_id,
            Selector::XlsSheet { index: 1 },
            Representation::Text,
        )),
        "Beta"
    );
    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), source.len());
    assert_eq!(sha256(&exact), sha256(&source));
    assert_eq!(exact, source);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn xls_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived XLS model reads the source bytes, so its single dependency
    // must be the exact `DocumentExact` root (keyed by `sha256(source)`).
    let source = build(Kind::Valid);
    let fx = Fixture::new("adr0060", &source);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let manifest = field.manifest().clone();
    let root = manifest.root_node;
    let root_node = load_node(fx.store.seeds(), &root).unwrap();
    assert_eq!(root_node.kind, NodeKind::DocumentExact);

    assert!(manifest.has_index());
    let istore = FsIndexStore::open(fx.store.root()).unwrap();
    let iroot = NodeId::from_bytes(manifest.index_root);
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_XLS_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::XlsModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the xls model must depend on exactly the exact root"
    );
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0x584C_5326_2056_4F4C;
    for _ in 0..256 {
        let mut buf = vec![0u8; 4096];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        let _ = vole_document::adapter::xls::detect(&buf, Limits::STRICT);
        let _ = vole_document::adapter::xls::parse(&buf, Limits::STRICT);
    }
    // The CFBF-signature-prefixed path is the most likely false-positive route.
    for _ in 0..256 {
        let mut buf = SIG.to_vec();
        buf.resize(4096, 0);
        for b in buf[8..].iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        let _ = vole_document::adapter::xls::parse(&buf, Limits::STRICT);
    }
}
