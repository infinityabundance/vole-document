//! Phase 26.1 court: the legacy Word 97–2003 (`.doc`, MS-DOC) semantic adapter built
//! on the CFBF/OLE2 physical layer.
//!
//! Gated on `doc`, so a build without the feature compiles an empty target.
//!
//! A `.doc` has no package layer: the exact leaf is the whole source (a
//! `DocumentExact`) and every derived observation is a bounded, representation-
//! preserving projection of it. The court checks:
//!
//! 1. **Byte-based detection** and the boundaries: a `.doc` → `Doc`; a generic
//!    compound file → `Cfbf`; prose → `Opaque`.
//! 2. **Representation preservation**: the FIB fields, the piece table (a mix of
//!    compressed and uncompressed pieces), each piece's exact `WordDocument`-stream
//!    byte span and compressed flag, the decoded main text, and the **sectioned**
//!    footnote/header/annotation/endnote/textbox/hdrtextbox text with pieces that cross
//!    section boundaries (each clipped piece's exact span, flag, and decoded text) — in
//!    both a `0Table` and a `1Table` variant (the flag is exercised by a decoy in the
//!    other table).
//! 3. **Selectors**: native `doc-fib`/`doc-piece`/`doc-text`/`doc-section`/`doc-find`
//!    plus common `metadata`/`text`/`search-match`.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted.
//! 5. **Fail closed**: malformed inputs decline typed (bad `wIdent`, out-of-range
//!    `fcClx`, non-monotonic CP array, piece outside the stream); random bytes never
//!    panic.
//! 6. **No cross-field aliasing** (ADR-0060): the model node depends on the exact
//!    `sha256(source)` root.

#![cfg(all(feature = "field", feature = "doc"))]

use std::path::PathBuf;

use vole_document::adapter::doc::DocSection;
use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_DOC_MODEL, SelectorKey, lookup};
use vole_document::field::ingest::{IngestReport, ingest_pdf};
use vole_document::field::node::NodeKind;
use vole_document::field::observe::{ObserveRequest, Representation, Selector, observe};
use vole_document::field::provenance::{AnswerValue, FieldAnswer};
use vole_document::field::{Field, FieldId, FieldStore};
use vole_document::integrity::sha256;
use vole_document::limits::Limits;
use vole_document::store::NodeId;

// ---------------------------------------------------------------------------
// Dependency-free `.doc` writer (test ground truth)
// ---------------------------------------------------------------------------

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

/// The FIB length (`38 + csw(14)*2 + cslw(22)*4 + cbRgFcLcb(93)*8`).
const FIB_LEN: usize = 898;
/// The text begins right after the FIB.
const TEXT_OFF: usize = FIB_LEN;
/// The CLX sits after 64 bytes of table-stream filler.
const CLX_OFF: u32 = 64;

/// The compressed main-text piece: `Hello, ` (one byte per character).
const PIECE0_TEXT: &[u8] = b"Hello, ";
/// The uncompressed main-text piece: `wörld\r` (UTF-16LE).
const PIECE1_TEXT: &str = "wörld\r";
/// The compressed footnote piece.
const PIECE2_TEXT: &[u8] = b"footnote";
/// The decoy text in the non-selected table stream.
const DECOY_TEXT: &[u8] = b"DECOY!";
/// The expected main document text.
const MAIN_TEXT: &str = "Hello, wörld\r";

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

#[derive(Clone, Copy)]
struct Piece {
    cp_len: u32,
    compressed: bool,
    fc: u32,
}

/// The document layout the writer produces.
struct Layout {
    text: Vec<u8>,
    pieces: Vec<Piece>,
    ccp: [u32; 7],
}

fn standard_layout() -> Layout {
    let uncompressed: Vec<u8> = PIECE1_TEXT
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    let mut text = Vec::new();
    let mut pieces = Vec::new();
    let mut base = TEXT_OFF as u32;
    for (bytes, cp_len, compressed) in [
        (PIECE0_TEXT.to_vec(), PIECE0_TEXT.len() as u32, true),
        (uncompressed.clone(), (uncompressed.len() / 2) as u32, false),
        (PIECE2_TEXT.to_vec(), PIECE2_TEXT.len() as u32, true),
    ] {
        let fc = base;
        base += bytes.len() as u32;
        text.extend_from_slice(&bytes);
        pieces.push(Piece {
            cp_len,
            compressed,
            fc,
        });
    }
    let ccp_text = pieces[0].cp_len + pieces[1].cp_len;
    let ccp_ftn = pieces[2].cp_len;
    // The decoy text is appended and referenced only by the non-selected table.
    text.extend_from_slice(DECOY_TEXT);
    Layout {
        text,
        pieces,
        ccp: [ccp_text, ccp_ftn, 0, 0, 0, 0, 0],
    }
}

/// The seven CP sections' character counts for the all-sections fixture:
/// main 6, footnote 4, header 3, annotation 3, endnote 3, textbox 3, hdrtextbox 0.
const SECTIONS_CCP: [u32; 7] = [6, 4, 3, 3, 3, 3, 0];

/// The pieces of the all-sections fixture: `(bytes, cp_len, compressed)`, in piece
/// order. Piece 1 (`BBF1`) is uncompressed and crosses the main/footnote boundary;
/// pieces 2 and 3 cross the footnote/header and header/annotation boundaries.
fn section_piece_specs() -> Vec<(Vec<u8>, u32, bool)> {
    let utf16: Vec<u8> = "BBF1"
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    vec![
        (b"AAAB".to_vec(), 4, true),
        (utf16, 4, false),
        (b"F2H".to_vec(), 3, true),
        (b"DRC".to_vec(), 3, true),
        (b"MT".to_vec(), 2, true),
        (b"END".to_vec(), 3, true),
        (b"TBX".to_vec(), 3, true),
    ]
}

/// A valid `.doc` whose piece table covers every CP section, with pieces that cross
/// section boundaries.
fn build_sections_doc() -> Vec<u8> {
    let mut text = Vec::new();
    let mut pieces = Vec::new();
    let mut base = TEXT_OFF as u32;
    for (bytes, cp_len, compressed) in section_piece_specs() {
        let fc = base;
        base += bytes.len() as u32;
        text.extend_from_slice(&bytes);
        pieces.push(Piece {
            cp_len,
            compressed,
            fc,
        });
    }
    let clx_bytes = clx(&pieces, false);
    let wd = build_word_document(
        FIB_MAGIC,
        NFIB_WORD97,
        false,
        CLX_OFF,
        clx_bytes.len() as u32,
        SECTIONS_CCP,
        &text,
    );
    let mut real_table = vec![0u8; CLX_OFF as usize];
    real_table.extend_from_slice(&clx_bytes);
    build_cfbf(&[
        ("WordDocument", wd),
        ("0Table", real_table.clone()),
        ("1Table", real_table),
    ])
}

fn clx(pieces: &[Piece], break_monotonic: bool) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(0x01);
    out.extend_from_slice(&0i16.to_le_bytes());
    out.push(0x01);
    out.extend_from_slice(&3i16.to_le_bytes());
    out.extend_from_slice(&[0xAA, 0xBB, 0xCC]);
    out.push(0x02);
    let n = pieces.len();
    let mut cps = vec![0u32];
    let mut acc = 0u32;
    for p in pieces {
        acc += p.cp_len;
        cps.push(acc);
    }
    if break_monotonic && n >= 3 {
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

/// The kinds of fixture the writer can produce.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Valid,
    BadIdent,
    BadNfib,
    BadClxRange,
    FcminRange,
    NonMonotonic,
    PieceOutside,
}

fn build(kind: Kind, table_1: bool) -> Vec<u8> {
    let l = standard_layout();
    let decoy = {
        let mut t = vec![0u8; CLX_OFF as usize];
        let decoy_piece = Piece {
            cp_len: DECOY_TEXT.len() as u32,
            compressed: true,
            fc: TEXT_OFF as u32 + (l.text.len() - DECOY_TEXT.len()) as u32,
        };
        t.extend_from_slice(&clx(&[decoy_piece], false));
        t
    };

    let clx_real = clx(&l.pieces, false);
    let (ident, n_fib) = match kind {
        Kind::BadIdent => (0x0000, NFIB_WORD97),
        Kind::BadNfib => (FIB_MAGIC, 0x0065),
        _ => (FIB_MAGIC, NFIB_WORD97),
    };
    let fc_clx = match kind {
        Kind::BadClxRange => 1_000_000,
        _ => CLX_OFF,
    };
    let mut pieces = l.pieces.clone();
    if kind == Kind::PieceOutside {
        pieces[2].fc = 5_000_000;
    }
    let clx_bytes = if kind == Kind::NonMonotonic {
        clx(&pieces, true)
    } else if kind == Kind::PieceOutside {
        clx(&pieces, false)
    } else {
        clx_real.clone()
    };
    let mut real_table = vec![0u8; CLX_OFF as usize];
    real_table.extend_from_slice(&clx_bytes);

    let mut wd = build_word_document(
        ident,
        n_fib,
        table_1,
        fc_clx,
        clx_bytes.len() as u32,
        l.ccp,
        &l.text,
    );
    if kind == Kind::FcminRange {
        // Corrupt `fcMin` (FibBase offset 24) to a value past the stream.
        wd[24..28].copy_from_slice(&9_000_000u32.to_le_bytes());
    }
    let (t0, t1) = if table_1 {
        (decoy, real_table)
    } else {
        (real_table, decoy)
    };
    build_cfbf(&[("WordDocument", wd), ("0Table", t0), ("1Table", t1)])
}

#[allow(clippy::too_many_arguments)]
fn build_word_document(
    ident: u16,
    n_fib: u16,
    table_1: bool,
    fc_clx: u32,
    lcb_clx: u32,
    ccp: [u32; 7],
    text: &[u8],
) -> Vec<u8> {
    let mut wd = vec![0u8; FIB_LEN];
    wd[0..2].copy_from_slice(&ident.to_le_bytes());
    wd[2..4].copy_from_slice(&n_fib.to_le_bytes());
    wd[4..6].copy_from_slice(&0x00C1u16.to_le_bytes());
    wd[6..8].copy_from_slice(&0x0409u16.to_le_bytes());
    let mut flags = 0u16;
    if table_1 {
        flags |= F_W_WHICH_TBL_STM;
    }
    wd[10..12].copy_from_slice(&flags.to_le_bytes());
    wd[12..14].copy_from_slice(&n_fib.to_le_bytes());
    wd[24..28].copy_from_slice(&(TEXT_OFF as u32).to_le_bytes());
    wd[28..32].copy_from_slice(&((TEXT_OFF + text.len()) as u32).to_le_bytes());
    wd[32..34].copy_from_slice(&14u16.to_le_bytes()); // csw
    wd[62..64].copy_from_slice(&22u16.to_le_bytes()); // cslw
    let rl = 64usize;
    wd[rl..rl + 4].copy_from_slice(&((TEXT_OFF + text.len()) as u32).to_le_bytes());
    // fibRgLw97 section counts: ccpText, ccpFtn, ccpHdd, (reserved3), ccpAtn, ccpEdn,
    // ccpTxbx, ccpHdrTxbx.
    for (delta, v) in [
        (12usize, ccp[0]),
        (16, ccp[1]),
        (20, ccp[2]),
        (28, ccp[3]),
        (32, ccp[4]),
        (36, ccp[5]),
        (40, ccp[6]),
    ] {
        wd[rl + delta..rl + delta + 4].copy_from_slice(&v.to_le_bytes());
    }
    wd[152..154].copy_from_slice(&93u16.to_le_bytes()); // cbRgFcLcb
    wd[418..422].copy_from_slice(&fc_clx.to_le_bytes()); // fcClx (pair 33)
    wd[422..426].copy_from_slice(&lcb_clx.to_le_bytes());
    wd.extend_from_slice(text);
    wd
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
        "vole-doc-{label}-{}-{}",
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
        format_basis: "opaque:doc-test".to_string(),
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
    // A real `.doc` is `Doc`.
    for table_1 in [false, true] {
        assert_eq!(
            detect_document_format(&build(Kind::Valid, table_1), Limits::DEFAULT),
            DocumentFormat::Doc
        );
    }
    // A `.doc` with a bad `wIdent` is still a valid compound file → `Cfbf`.
    assert_eq!(
        detect_document_format(&build(Kind::BadIdent, false), Limits::DEFAULT),
        DocumentFormat::Cfbf
    );
    // A `.doc` with an implausible `nFib` (Word 6/95) → `Cfbf`.
    assert_eq!(
        detect_document_format(&build(Kind::BadNfib, false), Limits::DEFAULT),
        DocumentFormat::Cfbf
    );
    // A generic compound file (no `WordDocument` stream) → `Cfbf`.
    let generic = build_cfbf(&[("Data", b"not a word document".to_vec())]);
    assert_eq!(
        detect_document_format(&generic, Limits::DEFAULT),
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
}

#[test]
fn fib_pieces_spans_and_text_are_preserved() {
    for (label, table_1, want_table) in [("0table", false, "0Table"), ("1table", true, "1Table")] {
        let src = build(Kind::Valid, table_1);
        let mut fx = Fixture::new(label, &src);
        let field = fx.report.field;

        // Common metadata names the format and the FIB version.
        let common = answer_json(&observe_ok(
            &mut fx.store,
            &field,
            Selector::Metadata,
            Representation::Metadata,
        ));
        assert!(common.contains("\"format\":\"doc\""), "{common}");
        assert!(common.contains("\"nFib\":193"), "{common}");
        assert!(common.contains("\"version\":\"word97\""), "{common}");
        assert!(
            common.contains(&format!("\"table\":\"{want_table}\"")),
            "{common}"
        );
        assert!(common.contains("\"pieces\":3"), "{common}");
        // The CLX span is table-stream-relative: the `Pcdt` sits after two `Prc`
        // records (9 bytes) at `CLX_OFF`, and the whole Pcdt is 5 + 40 bytes.
        assert!(common.contains("\"clx_span\":[73,118]"), "{common}");

        // The native FIB selector reports the recorded version.
        assert_eq!(
            answer_text(&observe_ok(
                &mut fx.store,
                &field,
                Selector::DocFib,
                Representation::Text,
            )),
            "word97"
        );

        // The document text is the main-text pieces only — never the decoy.
        let text = answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::Text,
            Representation::Text,
        ));
        assert_eq!(text, MAIN_TEXT, "{label}");
        assert!(!text.contains("DECOY"), "{label}");

        // The native `doc-text` selector agrees with the common text.
        assert_eq!(
            answer_text(&observe_ok(
                &mut fx.store,
                &field,
                Selector::DocText,
                Representation::Text,
            )),
            MAIN_TEXT
        );

        // Piece 0 is a compressed CP1252 piece with its exact stream span.
        let p0 = answer_json(&observe_ok(
            &mut fx.store,
            &field,
            Selector::DocPiece { index: 0 },
            Representation::Metadata,
        ));
        assert!(p0.contains("\"compressed\":true"), "{p0}");
        assert!(p0.contains("\"cp_start\":0"), "{p0}");
        assert!(
            p0.contains(&format!("\"cp_end\":{}", PIECE0_TEXT.len())),
            "{p0}"
        );
        assert!(p0.contains("\"fc\":898"), "{p0}");
        assert!(
            p0.contains(&format!("\"span\":[898,{}]", 898 + PIECE0_TEXT.len())),
            "{p0}"
        );
        assert_eq!(
            answer_text(&observe_ok(
                &mut fx.store,
                &field,
                Selector::DocPiece { index: 0 },
                Representation::Text,
            )),
            "Hello, "
        );
        // The piece's exact bytes are the CP1252 text bytes.
        assert_eq!(
            answer_bytes(&observe_ok(
                &mut fx.store,
                &field,
                Selector::DocPiece { index: 0 },
                Representation::ExactBytes,
            )),
            PIECE0_TEXT
        );

        // Piece 1 is uncompressed (UTF-16LE).
        let p1 = answer_json(&observe_ok(
            &mut fx.store,
            &field,
            Selector::DocPiece { index: 1 },
            Representation::Metadata,
        ));
        assert!(p1.contains("\"compressed\":false"), "{p1}");
        assert!(p1.contains("\"units\":6"), "{p1}");
        assert_eq!(
            answer_text(&observe_ok(
                &mut fx.store,
                &field,
                Selector::DocPiece { index: 1 },
                Representation::Text,
            )),
            PIECE1_TEXT
        );
        let p1_bytes = answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            Selector::DocPiece { index: 1 },
            Representation::ExactBytes,
        ));
        assert_eq!(p1_bytes.len(), 12);
        assert_eq!(
            p1_bytes,
            PIECE1_TEXT
                .encode_utf16()
                .flat_map(|u| u.to_le_bytes())
                .collect::<Vec<u8>>()
        );

        // Piece 2 is the compressed footnote section.
        let p2 = answer_json(&observe_ok(
            &mut fx.store,
            &field,
            Selector::DocPiece { index: 2 },
            Representation::Metadata,
        ));
        assert!(p2.contains("\"compressed\":true"), "{p2}");
        assert_eq!(
            answer_text(&observe_ok(
                &mut fx.store,
                &field,
                Selector::DocPiece { index: 2 },
                Representation::Text,
            )),
            "footnote"
        );

        // An out-of-range piece index declines typed.
        assert_eq!(
            observe_err(
                &mut fx.store,
                &field,
                Selector::DocPiece { index: 99 },
                Representation::Metadata
            ),
            ErrorClass::UnsupportedFeature
        );

        // `doc-find` finds a piece-local match.
        let found = answer_json(&observe_ok(
            &mut fx.store,
            &field,
            Selector::DocFind {
                pattern: "Hello".to_string(),
            },
            Representation::Metadata,
        ));
        assert!(found.contains("\"piece\":0"), "{found}");
        assert!(found.contains("\"matches\":[{"), "{found}");
        // The search-match common selector declines here only for an empty pattern.
        let common_find = answer_json(&observe_ok(
            &mut fx.store,
            &field,
            Selector::SearchMatch("wörld".to_string()),
            Representation::Text,
        ));
        assert!(common_find.contains("\"piece\":1"), "{common_find}");
    }
}

#[test]
fn sectioned_answers_preserve_ranges_pieces_and_text() {
    let src = build_sections_doc();
    let mut fx = Fixture::new("sections", &src);
    let field = fx.report.field;

    // The main-text answer is unchanged: exactly the first section's text, and it
    // agrees with the native `doc-text` selector and the `main` section answer.
    let main = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert_eq!(main, "AAABBB");
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::DocText,
            Representation::Text,
        )),
        main
    );
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::DocSection {
                section: DocSection::Main
            },
            Representation::Text,
        )),
        main
    );

    // Per section: the CP range, the ordered clipped pieces (piece index, clipped CP
    // range, compressed flag, exact WordDocument-stream byte span, decoded text), and
    // the concatenated decoded text. `span` is relative to the WordDocument stream.
    #[allow(clippy::type_complexity)]
    let cases: [(
        &str,
        u32,
        u32,
        &[(u32, u32, u32, bool, u64, u64, &str)],
        &str,
    ); 6] = [
        (
            "main",
            0,
            6,
            &[
                (0, 0, 4, true, 898, 902, "AAAB"),
                (1, 4, 6, false, 902, 906, "BB"),
            ],
            "AAABBB",
        ),
        (
            "footnote",
            6,
            10,
            &[
                (1, 6, 8, false, 906, 910, "F1"),
                (2, 8, 10, true, 910, 912, "F2"),
            ],
            "F1F2",
        ),
        (
            "header",
            10,
            13,
            &[
                (2, 10, 11, true, 912, 913, "H"),
                (3, 11, 13, true, 913, 915, "DR"),
            ],
            "HDR",
        ),
        (
            "annotation",
            13,
            16,
            &[
                (3, 13, 14, true, 915, 916, "C"),
                (4, 14, 16, true, 916, 918, "MT"),
            ],
            "CMT",
        ),
        (
            "endnote",
            16,
            19,
            &[(5, 16, 19, true, 918, 921, "END")],
            "END",
        ),
        (
            "textbox",
            19,
            22,
            &[(6, 19, 22, true, 921, 924, "TBX")],
            "TBX",
        ),
    ];
    for (name, cp_start, cp_end, want_pieces, want_text) in cases {
        let section = DocSection::from_name(name).expect("known section");
        let answer = observe_ok(
            &mut fx.store,
            &field,
            Selector::DocSection { section },
            Representation::Metadata,
        );
        assert!(
            answer.provenance.contains(&format!("section={name}")),
            "{name}"
        );
        let json = answer_json(&answer);
        assert!(json.contains("\"format\":\"doc\""), "{name}: {json}");
        assert!(
            json.contains(&format!("\"section\":\"{name}\"")),
            "{name}: {json}"
        );
        assert!(
            json.contains(&format!("\"cp_start\":{cp_start},\"cp_end\":{cp_end}")),
            "{name}: {json}"
        );
        assert!(json.contains("\"empty\":false"), "{name}: {json}");
        assert!(
            json.contains(&format!("\"pieces\":{}", want_pieces.len())),
            "{name}: {json}"
        );
        assert!(
            json.contains(&format!("\"text\":\"{want_text}\"")),
            "{name}: {json}"
        );
        for (piece, ps, pe, comp, sp0, sp1, ptext) in want_pieces {
            let frag = format!(
                "{{\"piece\":{piece},\"cp_start\":{ps},\"cp_end\":{pe},\"compressed\":{comp},\"units\":{},\"span\":[{sp0},{sp1}]",
                pe - ps
            );
            assert!(json.contains(&frag), "{name}: missing {frag} in {json}");
            assert!(
                json.contains(&format!("\"text\":\"{ptext}\"}}")),
                "{name}: missing piece text {ptext} in {json}"
            );
        }
        // The `Text` representation is the section's concatenated decoded text.
        assert_eq!(
            answer_text(&observe_ok(
                &mut fx.store,
                &field,
                Selector::DocSection { section },
                Representation::Text,
            )),
            want_text,
            "{name}"
        );
        // The `Structure` representation carries the same section descriptor.
        let structure = answer_json(&observe_ok(
            &mut fx.store,
            &field,
            Selector::DocSection { section },
            Representation::Structure,
        ));
        assert!(
            structure.contains(&format!("\"cp_start\":{cp_start},\"cp_end\":{cp_end}")),
            "{name}: {structure}"
        );
    }

    // A section whose `ccp*` count is zero is a typed **empty** answer: not an error,
    // not an unsupported feature, and with an empty decoded text.
    let empty = observe_ok(
        &mut fx.store,
        &field,
        Selector::DocSection {
            section: DocSection::HdrTextbox,
        },
        Representation::Metadata,
    );
    let empty_json = answer_json(&empty);
    assert!(
        empty_json.contains("\"section\":\"hdrtextbox\""),
        "{empty_json}"
    );
    assert!(
        empty_json.contains("\"cp_start\":22,\"cp_end\":22"),
        "{empty_json}"
    );
    assert!(empty_json.contains("\"empty\":true"), "{empty_json}");
    assert!(empty_json.contains("\"pieces\":0"), "{empty_json}");
    assert!(empty_json.contains("\"text\":\"\""), "{empty_json}");
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::DocSection {
                section: DocSection::HdrTextbox
            },
            Representation::Text,
        )),
        ""
    );

    // A section with an unknown name never resolves.
    assert!(DocSection::from_name("bogus").is_none());

    // The existing `doc-piece` answers are unchanged: piece 1 is the uncompressed
    // `BBF1` crossing the main/footnote boundary.
    let p1 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::DocPiece { index: 1 },
        Representation::Metadata,
    ));
    assert!(p1.contains("\"compressed\":false"), "{p1}");
    assert!(p1.contains("\"units\":4"), "{p1}");
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::DocPiece { index: 1 },
            Representation::Text,
        )),
        "BBF1"
    );
}

#[test]
fn malformed_inputs_decline_typed() {
    // An out-of-range `fcClx`: detects as `doc`, declines on parse.
    for (kind, label) in [
        (Kind::BadClxRange, "clx-range"),
        (Kind::FcminRange, "fcmin-range"),
        (Kind::NonMonotonic, "nonmonotonic"),
        (Kind::PieceOutside, "piece-outside"),
    ] {
        let src = build(kind, false);
        assert_eq!(
            detect_document_format(&src, Limits::DEFAULT),
            DocumentFormat::Doc,
            "{label}"
        );
        let e = vole_document::adapter::doc::parse(&src, Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), ErrorClass::InvalidDocStructure, "{label}");
    }

    // A bad `wIdent` / `nFib` is not detected as `doc` at all.
    for kind in [Kind::BadIdent, Kind::BadNfib] {
        let src = build(kind, false);
        assert_ne!(
            detect_document_format(&src, Limits::DEFAULT),
            DocumentFormat::Doc
        );
    }
}

#[test]
fn exact_materialization_is_byte_identical() {
    for (label, table_1) in [("exact-0", false), ("exact-1", true)] {
        let src = build(Kind::Valid, table_1);
        let fx = Fixture::new(label, &src);
        let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
        let out = field.materialize_exact(Limits::DEFAULT).unwrap();
        assert_eq!(out.len(), src.len(), "{label}");
        assert_eq!(sha256(&out), sha256(&src), "{label}");
        assert_eq!(out, fx.source, "{label}");
    }
}

#[test]
fn removal_then_fresh_process_is_queryable_and_exact() {
    let root = temp_dir("removal");
    let src_path = root.join("doc.doc");
    let desc_path = root.join("doc.voldoc");
    let source = build(Kind::Valid, false);
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
            Selector::DocText,
            Representation::Text,
        )),
        MAIN_TEXT
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
fn doc_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived DOC model reads the source bytes, so its single
    // dependency must be the exact `DocumentExact` root (keyed by `sha256(source)`).
    // The sectioned answers are derived from that same node, so the invariant must
    // hold for both the ordinary and the all-sections fixtures.
    for (label, source) in [
        ("adr0060-valid", build(Kind::Valid, false)),
        ("adr0060-sections", build_sections_doc()),
    ] {
        let fx = Fixture::new(label, &source);
        let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
        let manifest = field.manifest().clone();
        let root = manifest.root_node;
        let root_node = load_node(fx.store.seeds(), &root).unwrap();
        assert_eq!(root_node.kind, NodeKind::DocumentExact, "{label}");

        assert!(manifest.has_index(), "{label}");
        let istore = FsIndexStore::open(fx.store.root()).unwrap();
        let iroot = NodeId::from_bytes(manifest.index_root);
        let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_DOC_MODEL, 0)).unwrap();
        assert_eq!(entries.len(), 1, "{label}");
        let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
        assert_eq!(model.kind, NodeKind::DocModel, "{label}");
        assert_eq!(
            model.deps,
            vec![root],
            "{label}: the doc model must depend on exactly the exact root"
        );
    }
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0xD0C0_FFEE_2610_5566;
    for _ in 0..256 {
        let mut buf = vec![0u8; 4096];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        let _ = vole_document::adapter::doc::detect(&buf, Limits::STRICT);
        let _ = vole_document::adapter::doc::parse(&buf, Limits::STRICT);
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
        let _ = vole_document::adapter::doc::parse(&buf, Limits::STRICT);
    }
}
