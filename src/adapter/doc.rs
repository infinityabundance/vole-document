//! Bounded, representation-preserving legacy Word 97–2003 (`.doc`, MS-DOC)
//! **semantic** adapter (Phase 26.1), built on the CFBF/OLE2 physical layer (21.35).
//!
//! A `.doc` is a CFBF compound file whose `WordDocument` stream opens with the Word
//! **FIB** (File Information Block) and whose table stream (`0Table` or `1Table`,
//! selected by the FIB `fWhichTblStm` flag) carries the **piece table (CLX)** that
//! maps character positions (CPs) to text spans in the `WordDocument` stream. This
//! adapter **reuses the CFBF resolver** ([`crate::adapter::cfbf`]) to obtain those
//! two streams as exact byte spans — it never writes a second OLE parser — and adds
//! only a bounded semantic model over them.
//!
//! ## What is authoritative
//!
//! Like every Wave-2 format, a `.doc` has **no *separate* package layer**: the exact
//! leaf is the whole source (a `DocumentExact`) and every derived observation is a
//! bounded, span-preserving (`Q_gen`) projection of it. Nothing is re-encoded,
//! re-flowed, or re-OLE'd for exactness.
//!
//! ## What is parsed (the documented subset)
//!
//! * the **FIB** at the start of the `WordDocument` stream: `wIdent` (`0xA5EC`),
//!   `nFib` (restricted to a plausible Word 97–2003 range), the `fWhichTblStm` flag
//!   (bit 9 of the `flags` word, selecting `0Table`/`1Table`), the `fcMin`/`fcMac`
//!   legacy text-bound fields, the `ccp*` section counts (`ccpText`, `ccpFtn`,
//!   `ccpHdd`, `ccpAtn`, `ccpEdn`, `ccpTxbx`, `ccpHdrTxbx`), and the `fcClx`/`lcbClx`
//!   piece-table location (pair index 33 of `fibRgFcLcb`). The variable-length FIB is
//!   walked by its own `csw`/`cslw`/`cbRgFcLcb` counts, so an unusual layout still
//!   resolves;
//! * the **piece table (CLX)** in the table stream at `[fcClx, fcClx + lcbClx)`: the
//!   zero-or-more `Prc` (`0x01`) records, then the `Pcdt` (`0x02`) record with its
//!   `lcb`, then the `PlcPcd` — a CP array of `n+1` values followed by `n` 8-byte
//!   `PCD`s. For each piece the `fc` and its **compressed** bit (bit 30) are read: a
//!   **compressed** piece (bit set, `fc = (fcRaw & 0x3FFF_FFFF) / 2`) is CP1252
//!   single-byte text; an **uncompressed** piece is UTF-16LE. Each piece keeps its
//!   **exact byte span in the `WordDocument` stream** and its decoded text;
//! * the **sectioned document text**: the single contiguous CP space is partitioned by
//!   the cumulative `ccp*` counts into `main`/`footnote`/`header`/`annotation`/
//!   `endnote`/`textbox`/`hdrtextbox`, and each section is materialized as its own
//!   span-preserving (`Q_gen`) answer — its CP range, the ordered pieces clipped to it
//!   (each with the exact `WordDocument` byte span of that portion, the compressed flag,
//!   and the decoded text), and the section's concatenated decoded text. A section whose
//!   `ccp*` count is zero is a typed **empty** answer, never an error. The **main text
//!   over `[0, ccpText)`** is unchanged.
//!
//! ## The boundary it cannot cross (honest)
//!
//! Detection is built **on top of** the CFBF detector: a source is claimed only when
//! the CFBF layer accepts it **and** its `WordDocument` stream begins with the Word FIB
//! magic `0xA5EC` **and** its `nFib` is a plausible Word 97–2003 value **and** its
//! table stream (`0Table`/`1Table` per `fWhichTblStm`) exists. A generic compound file
//! without a Word FIB stays
//! [`Cfbf`](crate::field::document_format::DocumentFormat::Cfbf); a non-CFBF source is
//! unaffected. Word 6/95 (an older FIB, `nFib` below the range), Word 2007+ `.docx`
//! (an OPC ZIP, never a compound file), and every non-Word compound file are **not**
//! this format.
//!
//! A `.doc` that carries a plausible FIB but a malformed or out-of-subset body (a bad
//! `wIdent`, an implausible `nFib`, a missing table stream, an out-of-range
//! `fcClx`/`lcbClx`, a CLX past the table stream, a `Pcdt` that is absent or whose
//! `lcb` is not `12n+4`, a CP array that is not monotonic or does not start at 0, a
//! PCD `fc` whose piece falls outside the `WordDocument` stream, or a declared section
//! total that disagrees with the piece table) is admitted by detection (its
//! whole-source exact leaf is unaffected) and its derived model then **declines typed**
//! ([`crate::ErrorClass::InvalidDocStructure`]). Never a panic, never a guess.
//!
//! ## What stays out of the subset (recorded, Phase 30.5)
//!
//! Three MS-DOC surfaces are **deliberately outside** this bounded adapter and remain
//! recorded rather than guessed:
//!
//! * the **story → section mapping** — which header/footer *story* (`ccpFtn`,
//!   `ccpHdd`, `ccpTxbx`, …) a given CP range belongs to is defined not by the `ccp*`
//!   cumulative counts alone (those give a *sectioned* CP space, which this adapter
//!   exposes) but by the `PlcfHdd`/`PlcfBteChpx` story tables in the table stream,
//!   which this adapter does not model. The sectioned text is exact; the *story* label
//!   of a header/footer CP is not assigned.
//! * the **textbox / header-textbox anchors** — the `PlcfTxbxHdr`/`PlcfTxbxBxftn`
//!   anchor tables (and the `Spa`/`Txbx` records that bind a textbox CP to its shape)
//!   are not modelled, so textbox text is exposed as its section's CP range but not
//!   bound to an anchor.
//! * **piece-local / section-scoped `doc-find`** — `doc-find` is a **document-level**
//!   substring search over the decoded text **by contract**; it reports decoded-text
//!   matches with their CP/byte spans and is not scoped to a piece or a section.
//!   Section-scoped search is available by observing `--doc-section NAME` and searching
//!   the returned text at the call site.
//!
//! Each is a *typed decline or an explicitly unclaimed surface*, never a guess; the
//! exact leaf (`materialize == source`) is unaffected.

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model pieces (defends the decoder against a hostile blob).
pub const MAX_MODEL_PIECES: u32 = 1 << 24;
/// Hard cap on decoded text code units (defends the decoder against a hostile blob).
pub const MAX_MODEL_TEXT_UNITS: u32 = 1 << 30;

/// The Word FIB magic at offset 0 of the `WordDocument` stream.
pub const FIB_MAGIC: u16 = 0xA5EC;
/// The lowest plausible Word 97–2003 `nFib` (Word 97 = `0x00C1`).
pub const NFIB_MIN: u16 = 0x00C1;
/// The highest plausible Word 97–2003 `nFib` (Word 2003 = `0x010C`).
pub const NFIB_MAX: u16 = 0x010C;

/// The FIB `flags` bit selecting the `1Table` stream (else `0Table`).
pub const F_WHICH_TBL_STM: u16 = 0x0200;

/// The CLX (piece-table) record tag for a `Prc` record.
pub const CLXT_PRC: u8 = 0x01;
/// The CLX record tag for the terminating `Pcdt` record.
pub const CLXT_PCDT: u8 = 0x02;

/// The `Pcdt` `lcb` is always `12n + 4` (a `(n+1)`-CP array plus `n` 8-byte PCDs).
pub const PCDT_BASE: u32 = 4;
/// One PCD's length.
pub const PCD_LEN: usize = 8;
/// The PCD bit marking a compressed (CP1252 single-byte) piece.
pub const PCD_FCOMPRESSED: u32 = 0x4000_0000;
/// The PCD `fc` mask (the compressed bit removed).
pub const PCD_FCMASK: u32 = 0x3FFF_FFFF;

/// The `fibRgFcLcb` pair index of `fcClx`/`lcbClx`.
pub const CLX_PAIR_INDEX: usize = 33;
/// One `fibRgFcLcb` `(fc, lcb)` pair's length.
pub const FC_LCB_PAIR_LEN: usize = 8;
/// The minimum FIB byte length (through the `cbRgFcLcb` count field).
pub const FIB_MIN_LEN: usize = 154;
/// The FIB header (`FibBase`) length.
pub const FIB_BASE_LEN: usize = 32;

/// The `WordDocument` stream name (UTF-16LE, NUL-terminated in the directory).
pub const STREAM_WORD_DOCUMENT: &str = "WordDocument";
/// The `0Table` (or `1Table`) table stream name.
pub const STREAM_TABLE_0: &str = "0Table";
/// The `1Table` table stream name (selected when `fWhichTblStm` is set).
pub const STREAM_TABLE_1: &str = "1Table";

/// Match field: the document text.
pub const FIELD_TEXT: u8 = 0;

/// The stable match-field name.
pub const fn field_name(field: u8) -> &'static str {
    let _ = field;
    "text"
}

/// Whether an `nFib` value is a plausible Word 97–2003 version.
pub const fn n_fib_plausible(n_fib: u16) -> bool {
    n_fib >= NFIB_MIN && n_fib <= NFIB_MAX
}

/// A human-readable Word version name for an `nFib` value (Word 97–2003).
pub const fn version_name(n_fib: u16) -> &'static str {
    match n_fib {
        0x00C1 | 0x00C2 => "word97",
        0x00D9 | 0x00DA => "word2000",
        0x0101 | 0x0102 => "word2002",
        0x010C | 0x010D => "word2003",
        _ => "word97-2003",
    }
}

/// The parsed FIB fields the subset records (and depends on).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocFib {
    /// `wIdent` (`0xA5EC`).
    pub w_ident: u16,
    /// `nFib` (the format version).
    pub n_fib: u16,
    /// `nProduct`.
    pub n_product: u16,
    /// `lid` (the language / code-page hint; recorded, not used to select a code page).
    pub lid: u16,
    /// `pnNext` (the FIB chain's next page; normally 0).
    pub pn_next: i16,
    /// `flags` (the `FibBase` bit field; bit 9 is `fWhichTblStm`).
    pub flags: u16,
    /// `nFibBack`.
    pub n_fib_back: u16,
    /// `lKey`.
    pub l_key: u32,
    /// `envr`.
    pub envr: u8,
    /// `flags2`.
    pub flags2: u8,
    /// `fcMin`: the legacy text-begin file offset (the `FibBase` `reserved5` word).
    pub fc_min: u32,
    /// `fcMac`: the legacy text-end file offset (the `FibBase` `reserved6` word).
    pub fc_mac: u32,
    /// `cbMac`: the used byte length of the `WordDocument` stream.
    pub cb_mac: u32,
    /// `ccpText`: the main-document character count.
    pub ccp_text: u32,
    /// `ccpFtn`: the footnote character count.
    pub ccp_ftn: u32,
    /// `ccpHdd`: the header/footer character count.
    pub ccp_hdd: u32,
    /// `ccpAtn`: the annotation character count.
    pub ccp_atn: u32,
    /// `ccpEdn`: the endnote character count.
    pub ccp_edn: u32,
    /// `ccpTxbx`: the textbox character count.
    pub ccp_txbx: u32,
    /// `ccpHdrTxbx`: the header-textbox character count.
    pub ccp_hdr_txbx: u32,
    /// `fcClx`: the piece-table offset in the table stream.
    pub fc_clx: u32,
    /// `lcbClx`: the piece-table length in the table stream.
    pub lcb_clx: u32,
    /// `csw` (the `fibRgW` 16-bit count).
    pub csw: u16,
    /// `cslw` (the `fibRgLw` 32-bit count).
    pub cslw: u16,
    /// `cbRgFcLcb` (the `fibRgFcLcb` pair count).
    pub cb_rg_fc_lcb: u16,
}

impl DocFib {
    /// Whether the FIB selects the `1Table` stream.
    pub const fn table_1(&self) -> bool {
        self.flags & F_WHICH_TBL_STM != 0
    }

    /// The selected table stream name (`0Table` or `1Table`).
    pub const fn table_stream(&self) -> &'static str {
        if self.table_1() {
            STREAM_TABLE_1
        } else {
            STREAM_TABLE_0
        }
    }

    /// The CP range `[start, end)` of a section: the cumulative `ccp*` counts before
    /// it. An empty section (`ccp* == 0`) yields a zero-length range.
    pub fn section_range(&self, section: DocSection) -> (u64, u64) {
        let mut start = 0u64;
        for s in DocSection::ALL {
            let len = s.ccp(self) as u64;
            if s == section {
                return (start, start.saturating_add(len));
            }
            start = start.saturating_add(len);
        }
        (start, start)
    }

    /// The sum of every `ccp*` section count (the piece table's last CP).
    pub fn total_cp(&self) -> u64 {
        let mut total = 0u64;
        for c in [
            self.ccp_text,
            self.ccp_ftn,
            self.ccp_hdd,
            self.ccp_atn,
            self.ccp_edn,
            self.ccp_txbx,
            self.ccp_hdr_txbx,
        ] {
            total = total.saturating_add(c as u64);
        }
        total
    }
}

/// One of the seven MS-DOC character-position (CP) sections, in the order the piece
/// table lays them out (the cumulative `ccp*` counts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocSection {
    /// Main document text (`ccpText`), the CP range `[0, ccpText)`.
    Main,
    /// Footnote text (`ccpFtn`).
    Footnote,
    /// Header/footer text (`ccpHdd`).
    Header,
    /// Annotation (comment) text (`ccpAtn`).
    Annotation,
    /// Endnote text (`ccpEdn`).
    Endnote,
    /// Textbox text (`ccpTxbx`).
    Textbox,
    /// Header-textbox text (`ccpHdrTxbx`).
    HdrTextbox,
}

impl DocSection {
    /// Every section, in CP order.
    pub const ALL: [DocSection; 7] = [
        DocSection::Main,
        DocSection::Footnote,
        DocSection::Header,
        DocSection::Annotation,
        DocSection::Endnote,
        DocSection::Textbox,
        DocSection::HdrTextbox,
    ];

    /// The stable lower-case section name.
    pub const fn name(self) -> &'static str {
        match self {
            DocSection::Main => "main",
            DocSection::Footnote => "footnote",
            DocSection::Header => "header",
            DocSection::Annotation => "annotation",
            DocSection::Endnote => "endnote",
            DocSection::Textbox => "textbox",
            DocSection::HdrTextbox => "hdrtextbox",
        }
    }

    /// The FIB `ccp*` field this section's character count is read from.
    pub const fn fib_field(self) -> &'static str {
        match self {
            DocSection::Main => "ccpText",
            DocSection::Footnote => "ccpFtn",
            DocSection::Header => "ccpHdd",
            DocSection::Annotation => "ccpAtn",
            DocSection::Endnote => "ccpEdn",
            DocSection::Textbox => "ccpTxbx",
            DocSection::HdrTextbox => "ccpHdrTxbx",
        }
    }

    /// Resolve a section by its stable name (`main`, `footnote`, `header`,
    /// `annotation`, `endnote`, `textbox`, `hdrtextbox`).
    pub fn from_name(name: &str) -> Option<DocSection> {
        DocSection::ALL.into_iter().find(|s| s.name() == name)
    }

    /// This section's declared `ccp*` character count in `fib`.
    pub const fn ccp(self, fib: &DocFib) -> u32 {
        match self {
            DocSection::Main => fib.ccp_text,
            DocSection::Footnote => fib.ccp_ftn,
            DocSection::Header => fib.ccp_hdd,
            DocSection::Annotation => fib.ccp_atn,
            DocSection::Endnote => fib.ccp_edn,
            DocSection::Textbox => fib.ccp_txbx,
            DocSection::HdrTextbox => fib.ccp_hdr_txbx,
        }
    }
}

/// One piece of the piece table: a CP range mapped to an exact span in the
/// `WordDocument` stream, with its decoded text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocPiece {
    /// The piece's first character position (inclusive).
    pub cp_start: u32,
    /// The piece's last character position (exclusive).
    pub cp_end: u32,
    /// The resolved file offset of the piece in the `WordDocument` stream.
    pub fc: u32,
    /// Whether the piece is compressed (CP1252 single-byte; else UTF-16LE).
    pub compressed: bool,
    /// `(offset, length)` of the piece's bytes in the `WordDocument` stream.
    pub span: (u64, u64),
    /// The decoded text of the piece (UTF-16 code units; exact, never lossy).
    pub text: Vec<u16>,
}

impl DocPiece {
    /// The piece's character (CP) count.
    pub const fn cp_len(&self) -> u64 {
        (self.cp_end - self.cp_start) as u64
    }
}

/// One piece's contribution to a section: a CP range clipped to the section, the exact
/// byte span of that portion in the `WordDocument` stream, and its decoded text. A
/// piece that crosses a section boundary appears (clipped) in every section it
/// overlaps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocSectionPiece {
    /// The source piece's index in [`DocModel::pieces`].
    pub piece: u32,
    /// The clipped first character position inside the section (inclusive).
    pub cp_start: u32,
    /// One past the clipped last character position inside the section.
    pub cp_end: u32,
    /// Whether the piece is compressed (CP1252 single-byte; else UTF-16LE).
    pub compressed: bool,
    /// `(offset, length)` of the clipped portion's bytes in the `WordDocument` stream.
    pub span: (u64, u64),
    /// The decoded text of the clipped portion (UTF-16 code units; exact, never lossy).
    pub text: Vec<u16>,
}

impl DocSectionPiece {
    /// The clipped portion's character (CP) count.
    pub const fn cp_len(&self) -> u64 {
        (self.cp_end - self.cp_start) as u64
    }
}

/// The canonical derived legacy-DOC model (the materialization of a `DocModel` node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocModel {
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// `(offset, length)` of the `WordDocument` stream in the source.
    pub wd_span: (u64, u64),
    /// `(offset, length)` of the table stream in the source.
    pub tbl_span: (u64, u64),
    /// The parsed FIB fields.
    pub fib: DocFib,
    /// `(offset, length)` of the FIB in the `WordDocument` stream (`(0, fib_len)`).
    pub fib_span: (u64, u64),
    /// `(offset, length)` of the CLX in the table stream.
    pub clx_span: (u64, u64),
    /// The ordered piece list (piece order).
    pub pieces: Vec<DocPiece>,
}

impl DocModel {
    /// The piece at `index`, if present.
    pub fn piece(&self, index: u32) -> Option<&DocPiece> {
        self.pieces.get(index as usize)
    }

    /// The `(offset, length)` of a piece's bytes **in the source** (adding the
    /// `WordDocument` stream's source offset).
    pub fn piece_source_span(&self, p: &DocPiece) -> (u64, u64) {
        let base = self.wd_span.0;
        (base.saturating_add(p.span.0), base.saturating_add(p.span.1))
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let text_units: usize = self.pieces.iter().map(|p| p.text.len()).sum();
        let mut out = Vec::with_capacity(256 + self.pieces.len() * 64 + text_units * 2);
        out.extend_from_slice(b"DOCM");
        out.push(MODEL_VERSION);
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&self.wd_span.0.to_le_bytes());
        out.extend_from_slice(&self.wd_span.1.to_le_bytes());
        out.extend_from_slice(&self.tbl_span.0.to_le_bytes());
        out.extend_from_slice(&self.tbl_span.1.to_le_bytes());
        let f = &self.fib;
        out.extend_from_slice(&f.w_ident.to_le_bytes());
        out.extend_from_slice(&f.n_fib.to_le_bytes());
        out.extend_from_slice(&f.n_product.to_le_bytes());
        out.extend_from_slice(&f.lid.to_le_bytes());
        out.extend_from_slice(&f.pn_next.to_le_bytes());
        out.extend_from_slice(&f.flags.to_le_bytes());
        out.extend_from_slice(&f.n_fib_back.to_le_bytes());
        out.extend_from_slice(&f.l_key.to_le_bytes());
        out.push(f.envr);
        out.push(f.flags2);
        out.extend_from_slice(&f.fc_min.to_le_bytes());
        out.extend_from_slice(&f.fc_mac.to_le_bytes());
        out.extend_from_slice(&f.cb_mac.to_le_bytes());
        out.extend_from_slice(&f.ccp_text.to_le_bytes());
        out.extend_from_slice(&f.ccp_ftn.to_le_bytes());
        out.extend_from_slice(&f.ccp_hdd.to_le_bytes());
        out.extend_from_slice(&f.ccp_atn.to_le_bytes());
        out.extend_from_slice(&f.ccp_edn.to_le_bytes());
        out.extend_from_slice(&f.ccp_txbx.to_le_bytes());
        out.extend_from_slice(&f.ccp_hdr_txbx.to_le_bytes());
        out.extend_from_slice(&f.fc_clx.to_le_bytes());
        out.extend_from_slice(&f.lcb_clx.to_le_bytes());
        out.extend_from_slice(&f.csw.to_le_bytes());
        out.extend_from_slice(&f.cslw.to_le_bytes());
        out.extend_from_slice(&f.cb_rg_fc_lcb.to_le_bytes());
        out.extend_from_slice(&self.fib_span.0.to_le_bytes());
        out.extend_from_slice(&self.fib_span.1.to_le_bytes());
        out.extend_from_slice(&self.clx_span.0.to_le_bytes());
        out.extend_from_slice(&self.clx_span.1.to_le_bytes());
        out.extend_from_slice(&(self.pieces.len() as u32).to_le_bytes());
        for p in &self.pieces {
            out.extend_from_slice(&p.cp_start.to_le_bytes());
            out.extend_from_slice(&p.cp_end.to_le_bytes());
            out.extend_from_slice(&p.fc.to_le_bytes());
            out.push(p.compressed as u8);
            out.extend_from_slice(&p.span.0.to_le_bytes());
            out.extend_from_slice(&p.span.1.to_le_bytes());
            out.extend_from_slice(&(p.text.len() as u32).to_le_bytes());
            for u in &p.text {
                out.extend_from_slice(&u.to_le_bytes());
            }
        }
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<DocModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"DOCM" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let doc_len = r.u64()?;
        let wd_span = (r.u64()?, r.u64()?);
        let tbl_span = (r.u64()?, r.u64()?);
        check_span(wd_span, doc_len, "WordDocument span")?;
        check_span(tbl_span, doc_len, "table span")?;
        let w_ident = r.u16()?;
        let n_fib = r.u16()?;
        let n_product = r.u16()?;
        let lid = r.u16()?;
        let pn_next = r.u16()? as i16;
        let flags = r.u16()?;
        let n_fib_back = r.u16()?;
        let l_key = r.u32()?;
        let envr = r.u8()?;
        let flags2 = r.u8()?;
        let fc_min = r.u32()?;
        let fc_mac = r.u32()?;
        let cb_mac = r.u32()?;
        let ccp_text = r.u32()?;
        let ccp_ftn = r.u32()?;
        let ccp_hdd = r.u32()?;
        let ccp_atn = r.u32()?;
        let ccp_edn = r.u32()?;
        let ccp_txbx = r.u32()?;
        let ccp_hdr_txbx = r.u32()?;
        let fc_clx = r.u32()?;
        let lcb_clx = r.u32()?;
        let csw = r.u16()?;
        let cslw = r.u16()?;
        let cb_rg_fc_lcb = r.u16()?;
        if w_ident != FIB_MAGIC || !n_fib_plausible(n_fib) {
            return Err(corrupt("model FIB magic or nFib is implausible"));
        }
        let fib = DocFib {
            w_ident,
            n_fib,
            n_product,
            lid,
            pn_next,
            flags,
            n_fib_back,
            l_key,
            envr,
            flags2,
            fc_min,
            fc_mac,
            cb_mac,
            ccp_text,
            ccp_ftn,
            ccp_hdd,
            ccp_atn,
            ccp_edn,
            ccp_txbx,
            ccp_hdr_txbx,
            fc_clx,
            lcb_clx,
            csw,
            cslw,
            cb_rg_fc_lcb,
        };
        let fib_span = (r.u64()?, r.u64()?);
        let clx_span = (r.u64()?, r.u64()?);
        check_span(fib_span, wd_span.1, "FIB span")?;
        check_span(clx_span, tbl_span.1, "CLX span")?;
        let count = r.u32()?;
        if count > MAX_MODEL_PIECES {
            return Err(corrupt("model piece count is implausible"));
        }
        let mut pieces = Vec::with_capacity(count as usize);
        let mut text_units: u64 = 0;
        let mut prev_cp: u32 = 0;
        for i in 0..count {
            let cp_start = r.u32()?;
            let cp_end = r.u32()?;
            let fc = r.u32()?;
            let compressed = r.u8()? != 0;
            let span = (r.u64()?, r.u64()?);
            check_span(span, wd_span.1, "piece span")?;
            let units = r.u32()?;
            if units as u64 > MAX_MODEL_TEXT_UNITS as u64 {
                return Err(corrupt("piece text is implausible"));
            }
            if cp_start > cp_end || (i > 0 && cp_start < prev_cp) {
                return Err(corrupt("piece CPs are not monotonic"));
            }
            let expected_units = if compressed {
                span.1 - span.0
            } else {
                (span.1 - span.0) / 2
            };
            if units as u64 != expected_units || (!compressed && (span.1 - span.0) % 2 != 0) {
                return Err(corrupt("piece text length disagrees with its span"));
            }
            let mut text = Vec::with_capacity(units as usize);
            for _ in 0..units {
                text.push(r.u16()?);
            }
            text_units = text_units.saturating_add(units as u64);
            if text_units > MAX_MODEL_TEXT_UNITS as u64 {
                return Err(corrupt("model text is implausible"));
            }
            prev_cp = cp_end;
            pieces.push(DocPiece {
                cp_start,
                cp_end,
                fc,
                compressed,
                span,
                text,
            });
        }
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        Ok(DocModel {
            doc_len,
            wd_span,
            tbl_span,
            fib,
            fib_span,
            clx_span,
            pieces,
        })
    }
}

fn check_span(span: (u64, u64), bound: u64, what: &'static str) -> Result<()> {
    if span.0 > span.1 || span.1 > bound {
        return Err(corrupt(format!("{what} is outside its container")));
    }
    Ok(())
}

/// A lexical-match record from [`find`]. The span is **within the `WordDocument`
/// stream** (add [`DocModel::wd_span`]`.0` for the source offset).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocMatch {
    /// The piece index the match lies in.
    pub piece: u32,
    /// [`FIELD_TEXT`].
    pub field: u8,
    /// The match's first byte offset in the `WordDocument` stream.
    pub start: u64,
    /// One past the match's last byte in the `WordDocument` stream.
    pub end: u64,
    /// The matched text.
    pub text: String,
}

/// The exact bytes of a piece in the `WordDocument` stream, from the source.
pub fn piece_bytes<'a>(source: &'a [u8], model: &DocModel, p: &DocPiece) -> Result<&'a [u8]> {
    let (start, end) = model.piece_source_span(p);
    slice(source, start, end, "piece span is outside the source")
}

/// The decoded text of one piece (lossy for display only; the model stores the exact
/// UTF-16 code units).
pub fn piece_text(p: &DocPiece) -> String {
    String::from_utf16_lossy(&p.text)
}

/// The document text: the concatenation of the piece texts over the main-text range
/// `[0, ccpText)`.
pub fn main_text(model: &DocModel) -> String {
    let ccp = model.fib.ccp_text as u64;
    let mut units: Vec<u16> = Vec::new();
    for p in &model.pieces {
        if p.cp_start as u64 >= ccp {
            break;
        }
        let end = (p.cp_end as u64).min(ccp);
        let take = (end - p.cp_start as u64) as usize;
        units.extend_from_slice(&p.text[..take.min(p.text.len())]);
        if end >= ccp {
            break;
        }
    }
    String::from_utf16_lossy(&units)
}

/// The ordered pieces that fall in `section`, clipped to the section's CP range.
/// A piece that crosses a section boundary appears (clipped) in every section it
/// overlaps; a piece entirely outside the section is omitted. An empty section yields
/// no pieces (a typed empty answer, not an error).
pub fn section_pieces(model: &DocModel, section: DocSection) -> Vec<DocSectionPiece> {
    let (sec_start, sec_end) = model.fib.section_range(section);
    let mut out = Vec::new();
    if sec_start >= sec_end {
        return out;
    }
    for (i, p) in model.pieces.iter().enumerate() {
        let ps = p.cp_start as u64;
        let pe = p.cp_end as u64;
        if pe <= sec_start || ps >= sec_end {
            continue;
        }
        let lo = ps.max(sec_start);
        let hi = pe.min(sec_end);
        let left = lo - ps;
        let take = hi - lo;
        let unit: u64 = if p.compressed { 1 } else { 2 };
        let span_start = p.span.0.saturating_add(left.saturating_mul(unit));
        let span_end = span_start.saturating_add(take.saturating_mul(unit));
        let text = p
            .text
            .get(left as usize..(left + take) as usize)
            .map(<[u16]>::to_vec)
            .unwrap_or_default();
        out.push(DocSectionPiece {
            piece: i as u32,
            cp_start: lo as u32,
            cp_end: hi as u32,
            compressed: p.compressed,
            span: (span_start, span_end),
            text,
        });
    }
    out
}

/// The decoded text of one section's clipped piece (lossy for display only; the model
/// stores the exact UTF-16 code units).
pub fn section_piece_text(p: &DocSectionPiece) -> String {
    String::from_utf16_lossy(&p.text)
}

/// The concatenated decoded text of a section: its clipped pieces' texts in piece
/// order.
pub fn section_text(model: &DocModel, section: DocSection) -> String {
    let mut units: Vec<u16> = Vec::new();
    for p in &section_pieces(model, section) {
        units.extend_from_slice(&p.text);
    }
    String::from_utf16_lossy(&units)
}

/// The `(offset, length)` of a section piece's bytes **in the source** (adding the
/// `WordDocument` stream's source offset).
pub fn section_piece_source_span(model: &DocModel, p: &DocSectionPiece) -> (u64, u64) {
    let base = model.wd_span.0;
    (base.saturating_add(p.span.0), base.saturating_add(p.span.1))
}

/// The canonical text rendering of the model (the main document text).
pub fn canonical_text(model: &DocModel, _source: &[u8], max_bytes: u64) -> Result<String> {
    let text = main_text(model);
    if text.len() as u64 > max_bytes {
        return Err(Error::resource_limit(format!(
            "doc canonical text exceeded the {max_bytes}-byte budget"
        )));
    }
    Ok(text)
}

/// A bounded, case-sensitive lexical search over each piece's decoded text. A match is
/// reported only within one piece (a pattern that straddles a piece boundary is not
/// found — recorded honestly). The span is within the `WordDocument` stream.
pub fn find(model: &DocModel, pattern: &str, max_bytes: u64) -> Result<Vec<DocMatch>> {
    if pattern.is_empty() {
        return Ok(Vec::new());
    }
    let needle = pattern.as_bytes();
    let plen_units = pattern.encode_utf16().count() as u64;
    let mut out: Vec<DocMatch> = Vec::new();
    let mut estimated: u64 = 0;
    for (i, p) in model.pieces.iter().enumerate() {
        let text = piece_text(p);
        let bytes = text.as_bytes();
        let unit: u64 = if p.compressed { 1 } else { 2 };
        let base = p.fc as u64;
        let mut at = 0usize;
        while at + needle.len() <= bytes.len() {
            if &bytes[at..at + needle.len()] == needle {
                estimated = estimated.saturating_add(64 + needle.len() as u64);
                if estimated > max_bytes {
                    return Err(Error::resource_limit(format!(
                        "doc find exceeded the {max_bytes}-byte budget"
                    )));
                }
                let units = text[..at].encode_utf16().count() as u64;
                let start = base.saturating_add(units.saturating_mul(unit));
                let end = start.saturating_add(plen_units.saturating_mul(unit));
                out.push(DocMatch {
                    piece: i as u32,
                    field: FIELD_TEXT,
                    start,
                    end,
                    text: pattern.to_string(),
                });
                at += needle.len();
            } else {
                at += 1;
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Fast-fail legacy-DOC detector, built **on top of** the CFBF detector: the source
/// must be a compound file the CFBF layer accepts, its `WordDocument` stream must
/// begin with the Word FIB magic `0xA5EC` and an `nFib` in the plausible Word 97–2003
/// range, and its table stream (`0Table`/`1Table` per `fWhichTblStm`) must exist.
/// Never consults a file name.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.len() as u64 > limits.max_doc_document_bytes {
        return false;
    }
    if !crate::adapter::cfbf::detect(source, limits) {
        return false;
    }
    let Ok(model) = crate::adapter::cfbf::parse(source, limits) else {
        return false;
    };
    let Some(wd_entry) = find_named(&model, STREAM_WORD_DOCUMENT) else {
        return false;
    };
    // Read only the FIB prefix: a genuine `.doc` whose `WordDocument` chain is
    // **fragmented** (a non-contiguous sector chain) is still a `.doc`, and its
    // FIB lives in the stream's first chunk. Detection therefore reads the header
    // prefix rather than requiring the whole stream to be one contiguous span.
    let Ok(wd) = crate::adapter::cfbf::stream_leading(source, &model, wd_entry, FIB_BASE_LEN)
    else {
        return false;
    };
    if wd.len() < FIB_BASE_LEN {
        return false;
    }
    let w_ident = u16::from_le_bytes([wd[0], wd[1]]);
    if w_ident != FIB_MAGIC {
        return false;
    }
    let n_fib = u16::from_le_bytes([wd[2], wd[3]]);
    if !n_fib_plausible(n_fib) {
        return false;
    }
    let flags = u16::from_le_bytes([wd[10], wd[11]]);
    let table = if flags & F_WHICH_TBL_STM != 0 {
        STREAM_TABLE_1
    } else {
        STREAM_TABLE_0
    };
    find_named(&model, table).is_some()
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse `source` into a [`DocModel`] under `limits`. Declines typed on any malformed
/// or over-large input; never panics.
pub fn parse(source: &[u8], limits: Limits) -> Result<DocModel> {
    let len = source.len() as u64;
    if len > limits.max_doc_document_bytes {
        return Err(Error::resource_limit(format!(
            "doc source is {len} bytes, above the {}-byte document cap",
            limits.max_doc_document_bytes
        )));
    }
    let cfbf = crate::adapter::cfbf::parse(source, limits)?;

    let wd_entry = find_named(&cfbf, STREAM_WORD_DOCUMENT)
        .ok_or_else(|| invalid("the compound file has no WordDocument stream"))?;
    let wd = crate::adapter::cfbf::stream_bytes(source, wd_entry)?;
    if wd.len() as u64 > limits.max_doc_stream_bytes {
        return Err(Error::resource_limit(format!(
            "the WordDocument stream is {} bytes, above the {}-byte stream cap",
            wd.len(),
            limits.max_doc_stream_bytes
        )));
    }
    let fib = parse_fib(wd, limits)?;

    let table = fib.table_stream();
    let tbl_entry = find_named(&cfbf, table)
        .ok_or_else(|| invalid(format!("the compound file has no {table} stream")))?;
    let tbl = crate::adapter::cfbf::stream_bytes(source, tbl_entry)?;
    if tbl.len() as u64 > limits.max_doc_stream_bytes {
        return Err(Error::resource_limit(format!(
            "the {table} stream is {} bytes, above the {}-byte stream cap",
            tbl.len(),
            limits.max_doc_stream_bytes
        )));
    }

    let fc_clx = fib.fc_clx as u64;
    let lcb_clx = fib.lcb_clx as u64;
    let clx_end = fc_clx
        .checked_add(lcb_clx)
        .ok_or_else(|| invalid("the CLX span overflows"))?;
    if clx_end > tbl.len() as u64 {
        return Err(invalid("the CLX lies outside the table stream"));
    }
    let clx = &tbl[fc_clx as usize..clx_end as usize];
    let (raw_pieces, pcdt_off, pcdt_len) = parse_clx(clx, limits)?;

    // Resolve every piece to an exact span in the WordDocument stream and decode it.
    let mut pieces = Vec::with_capacity(raw_pieces.len());
    let mut text_bytes: u64 = 0;
    for rp in &raw_pieces {
        let cp_len = (rp.cp_end - rp.cp_start) as u64;
        let byte_len = if rp.compressed { cp_len } else { cp_len * 2 };
        let start = rp.fc as u64;
        let end = start
            .checked_add(byte_len)
            .ok_or_else(|| invalid("a piece span overflows"))?;
        if end > wd.len() as u64 {
            return Err(invalid("a piece falls outside the WordDocument stream"));
        }
        text_bytes = text_bytes.saturating_add(byte_len);
        if text_bytes > limits.max_doc_text_bytes {
            return Err(Error::resource_limit(format!(
                "doc text is above the {}-byte text cap",
                limits.max_doc_text_bytes
            )));
        }
        let bytes = &wd[start as usize..end as usize];
        let text = decode_piece(bytes, rp.compressed);
        pieces.push(DocPiece {
            cp_start: rp.cp_start,
            cp_end: rp.cp_end,
            fc: rp.fc,
            compressed: rp.compressed,
            span: (start, end),
            text,
        });
    }

    // The piece table's last CP must equal the declared sum of every section count.
    let total_cp = pieces.last().map(|p| p.cp_end as u64).unwrap_or(0);
    if total_cp != fib.total_cp() {
        return Err(invalid(
            "the piece table's last CP disagrees with the declared section totals",
        ));
    }

    let fib_span = (0, fib_declared_len(&fib));
    // `pcdt_off`/`pcdt_len` are relative to the CLX slice; make them relative to the
    // table stream (the model's `clx_span` contract).
    let clx_start = fc_clx.saturating_add(pcdt_off);
    let clx_span = (clx_start, clx_start.saturating_add(pcdt_len));
    Ok(DocModel {
        doc_len: len,
        wd_span: wd_entry.data_span,
        tbl_span: tbl_entry.data_span,
        fib,
        fib_span,
        clx_span,
        pieces,
    })
}

/// Parse `source` and return its canonical derived model, encoded (the materialization
/// of a `DocModel` node).
pub fn build_doc_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits)?.encode())
}

/// The declared FIB length (`38 + csw*2 + cslw*4 + cbRgFcLcb*8`).
fn fib_declared_len(fib: &DocFib) -> u64 {
    let mut len = 38u64;
    len = len.saturating_add(fib.csw as u64 * 2);
    len = len.saturating_add(fib.cslw as u64 * 4);
    len = len.saturating_add(fib.cb_rg_fc_lcb as u64 * FC_LCB_PAIR_LEN as u64);
    len
}

/// Parse the FIB from the start of the `WordDocument` stream.
fn parse_fib(wd: &[u8], limits: Limits) -> Result<DocFib> {
    if wd.len() < FIB_MIN_LEN {
        return Err(invalid("the FIB is truncated"));
    }
    let get16 = |off: usize| -> Result<u16> {
        rd_u16(wd, off).ok_or_else(|| invalid("the FIB is truncated"))
    };
    let get32 = |off: usize| -> Result<u32> {
        rd_u32(wd, off).ok_or_else(|| invalid("the FIB is truncated"))
    };
    let w_ident = get16(0)?;
    if w_ident != FIB_MAGIC {
        return Err(invalid(format!("bad FIB wIdent 0x{w_ident:04X}")));
    }
    let n_fib = get16(2)?;
    if !n_fib_plausible(n_fib) {
        return Err(invalid(format!("implausible nFib {n_fib}")));
    }
    let n_product = get16(4)?;
    let lid = get16(6)?;
    let pn_next = get16(8)? as i16;
    let flags = get16(10)?;
    let n_fib_back = get16(12)?;
    let l_key = get32(14)?;
    let envr = *wd.get(18).ok_or_else(|| invalid("the FIB is truncated"))?;
    let flags2 = *wd.get(19).ok_or_else(|| invalid("the FIB is truncated"))?;
    let fc_min = get32(24)?;
    let fc_mac = get32(28)?;
    if fc_min as u64 > wd.len() as u64 || fc_mac as u64 > wd.len() as u64 {
        return Err(invalid("fcMin or fcMac is outside the WordDocument stream"));
    }

    // Walk the variable-length FIB by its declared counts.
    let mut off = FIB_BASE_LEN;
    let csw = get16(off)? as u64;
    if csw > limits.max_doc_fib_fields {
        return Err(Error::resource_limit(format!(
            "FIB csw {csw} exceeds max_doc_fib_fields {}",
            limits.max_doc_fib_fields
        )));
    }
    off = off
        .checked_add(2)
        .and_then(|o| o.checked_add(csw as usize * 2))
        .ok_or_else(|| invalid("the FIB is truncated"))?;
    let cslw = get16(off)? as u64;
    if cslw > limits.max_doc_fib_fields {
        return Err(Error::resource_limit(format!(
            "FIB cslw {cslw} exceeds max_doc_fib_fields {}",
            limits.max_doc_fib_fields
        )));
    }
    let fib_rglw_off = off
        .checked_add(2)
        .ok_or_else(|| invalid("the FIB is truncated"))?;
    off = fib_rglw_off
        .checked_add(cslw as usize * 4)
        .ok_or_else(|| invalid("the FIB is truncated"))?;
    let cb_rg_fc_lcb = get16(off)? as u64;
    if cb_rg_fc_lcb > limits.max_doc_fib_fields {
        return Err(Error::resource_limit(format!(
            "FIB cbRgFcLcb {cb_rg_fc_lcb} exceeds max_doc_fib_fields {}",
            limits.max_doc_fib_fields
        )));
    }
    let fib_rg_fclcb_off = off
        .checked_add(2)
        .ok_or_else(|| invalid("the FIB is truncated"))?;
    off = fib_rg_fclcb_off
        .checked_add(cb_rg_fc_lcb as usize * FC_LCB_PAIR_LEN)
        .ok_or_else(|| invalid("the FIB is truncated"))?;
    if off > wd.len() {
        return Err(invalid("the FIB extends past the WordDocument stream"));
    }
    if cslw < 11 {
        return Err(invalid(
            "the FIB fibRgLw is too short for the section counts",
        ));
    }
    if cb_rg_fc_lcb <= CLX_PAIR_INDEX as u64 {
        return Err(invalid("the FIB has no fcClx pair"));
    }

    let rl = fib_rglw_off;
    let cb_mac = get32(rl)?;
    let ccp = |delta: usize| -> Result<u32> {
        let v = get32(rl + delta)?;
        if v > i32::MAX as u32 {
            Err(invalid("a FIB ccp count is negative"))
        } else {
            Ok(v)
        }
    };
    let ccp_text = ccp(12)?;
    let ccp_ftn = ccp(16)?;
    let ccp_hdd = ccp(20)?;
    let ccp_atn = ccp(28)?;
    let ccp_edn = ccp(32)?;
    let ccp_txbx = ccp(36)?;
    let ccp_hdr_txbx = ccp(40)?;

    let clx_pair = fib_rg_fclcb_off + CLX_PAIR_INDEX * FC_LCB_PAIR_LEN;
    let fc_clx = get32(clx_pair)?;
    let lcb_clx = get32(clx_pair + 4)?;

    Ok(DocFib {
        w_ident,
        n_fib,
        n_product,
        lid,
        pn_next,
        flags,
        n_fib_back,
        l_key,
        envr,
        flags2,
        fc_min,
        fc_mac,
        cb_mac,
        ccp_text,
        ccp_ftn,
        ccp_hdd,
        ccp_atn,
        ccp_edn,
        ccp_txbx,
        ccp_hdr_txbx,
        fc_clx,
        lcb_clx,
        csw: csw as u16,
        cslw: cslw as u16,
        cb_rg_fc_lcb: cb_rg_fc_lcb as u16,
    })
}

/// A raw (spanless) piece as read from the CLX.
struct RawPiece {
    cp_start: u32,
    cp_end: u32,
    fc: u32,
    compressed: bool,
}

/// Parse the CLX (the piece table) and return the pieces plus the `Pcdt` span
/// `(offset, length)` **within the CLX**.
fn parse_clx(clx: &[u8], limits: Limits) -> Result<(Vec<RawPiece>, u64, u64)> {
    let mut pos = 0usize;
    loop {
        let clxt = *clx
            .get(pos)
            .ok_or_else(|| invalid("the CLX has no Pcdt record"))?;
        pos += 1;
        match clxt {
            CLXT_PRC => {
                let b = clx
                    .get(pos..pos + 2)
                    .ok_or_else(|| invalid("a CLX Prc record is truncated"))?;
                let cb = i16::from_le_bytes([b[0], b[1]]);
                pos += 2;
                if cb < 0 {
                    return Err(invalid("a CLX Prc record has a negative length"));
                }
                pos = pos
                    .checked_add(cb as usize)
                    .ok_or_else(|| invalid("a CLX Prc record overflows"))?;
                if pos > clx.len() {
                    return Err(invalid("a CLX Prc record extends past the CLX"));
                }
            }
            CLXT_PCDT => {
                let pcdt_off = (pos - 1) as u64;
                let b = clx
                    .get(pos..pos + 4)
                    .ok_or_else(|| invalid("the CLX Pcdt is truncated"))?;
                let lcb = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                pos += 4;
                if lcb < PCDT_BASE || !(lcb - PCDT_BASE).is_multiple_of(12) {
                    return Err(invalid("the CLX Pcdt lcb is not 12n+4"));
                }
                let n = (lcb - PCDT_BASE) / 12;
                if n as u64 > limits.max_doc_pieces {
                    return Err(Error::resource_limit(format!(
                        "the piece table has {n} pieces, exceeding max_doc_pieces {}",
                        limits.max_doc_pieces
                    )));
                }
                let plc_end = pos
                    .checked_add(lcb as usize)
                    .ok_or_else(|| invalid("the CLX PlcPcd overflows"))?;
                if plc_end > clx.len() {
                    return Err(invalid("the CLX Pcdt extends past the CLX"));
                }
                let plc = &clx[pos..plc_end];
                let pieces = parse_plcpcd(plc, n as usize)?;
                return Ok((pieces, pcdt_off, 5 + lcb as u64));
            }
            other => {
                return Err(invalid(format!("unknown CLX record tag 0x{other:02X}")));
            }
        }
    }
}

/// Parse a `PlcPcd` (a `(n+1)`-CP array followed by `n` 8-byte PCDs).
fn parse_plcpcd(plc: &[u8], n: usize) -> Result<Vec<RawPiece>> {
    let cp_bytes = (n + 1)
        .checked_mul(4)
        .ok_or_else(|| invalid("the piece CP array overflows"))?;
    let pcd_bytes = n
        .checked_mul(PCD_LEN)
        .ok_or_else(|| invalid("the PCD array overflows"))?;
    if cp_bytes + pcd_bytes != plc.len() {
        return Err(invalid(
            "the PlcPcd length is inconsistent with its piece count",
        ));
    }
    let mut cps: Vec<u32> = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let off = i * 4;
        cps.push(u32::from_le_bytes([
            plc[off],
            plc[off + 1],
            plc[off + 2],
            plc[off + 3],
        ]));
    }
    if cps[0] != 0 {
        return Err(invalid("the piece CP array does not start at 0"));
    }
    for w in cps.windows(2) {
        if w[1] < w[0] {
            return Err(invalid("the piece CP array is not monotonic"));
        }
    }
    let pcd_base = cp_bytes;
    let mut pieces = Vec::with_capacity(n);
    for i in 0..n {
        let off = pcd_base + i * PCD_LEN;
        let fc_raw = u32::from_le_bytes([plc[off + 2], plc[off + 3], plc[off + 4], plc[off + 5]]);
        let compressed = fc_raw & PCD_FCOMPRESSED != 0;
        let fc = if compressed {
            (fc_raw & PCD_FCMASK) >> 1
        } else {
            fc_raw
        };
        pieces.push(RawPiece {
            cp_start: cps[i],
            cp_end: cps[i + 1],
            fc,
            compressed,
        });
    }
    Ok(pieces)
}

/// Decode a piece's bytes: CP1252 for a compressed piece, UTF-16LE otherwise.
fn decode_piece(bytes: &[u8], compressed: bool) -> Vec<u16> {
    if compressed {
        bytes.iter().map(|&b| cp1252(b)).collect()
    } else {
        bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect()
    }
}

/// Map one CP1252 byte to its Unicode code unit.
fn cp1252(b: u8) -> u16 {
    // The five CP1252-undefined positions map to U+FFFD (a derived-text caveat only;
    // the exact bytes remain available through the piece's source span).
    const HIGH: [u16; 32] = [
        0x20AC, 0xFFFD, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, // 0x80..0x87
        0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0xFFFD, 0x017D, 0xFFFD, // 0x88..0x8F
        0xFFFD, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, // 0x90..0x97
        0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0xFFFD, 0x017E, 0x0178, // 0x98..0x9F
    ];
    match b {
        0x80..=0x9F => HIGH[(b - 0x80) as usize],
        _ => b as u16,
    }
}

/// Find a directory stream entry by its verbatim name.
fn find_named<'a>(
    model: &'a crate::adapter::cfbf::CfbfModel,
    name: &str,
) -> Option<&'a crate::adapter::cfbf::CfbfEntry> {
    model
        .entries
        .iter()
        .find(|e| e.obj_type == crate::adapter::cfbf::OBJ_STREAM && entry_name_is(e, name))
}

/// Whether a directory entry's verbatim name equals `name`.
fn entry_name_is(e: &crate::adapter::cfbf::CfbfEntry, name: &str) -> bool {
    crate::adapter::cfbf::entry_name_text(e) == name
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn rd_u16(b: &[u8], off: usize) -> Option<u16> {
    let s = b.get(off..off + 2)?;
    Some(u16::from_le_bytes([s[0], s[1]]))
}

fn rd_u32(b: &[u8], off: usize) -> Option<u32> {
    let s = b.get(off..off + 4)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn slice<'a>(source: &'a [u8], start: u64, end: u64, why: &'static str) -> Result<&'a [u8]> {
    let s = usize::try_from(start).map_err(|_| invalid(why))?;
    let e = usize::try_from(end).map_err(|_| invalid(why))?;
    source.get(s..e).ok_or_else(|| invalid(why))
}

fn invalid(message: impl Into<String>) -> Error {
    Error::invalid_doc_structure(format!("doc: {}", message.into()))
}

fn corrupt(message: impl Into<String>) -> Error {
    Error::invalid_doc_structure(format!("doc model: {}", message.into()))
}

/// A minimal fail-closed little-endian reader for model decoding.
struct BinReader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> BinReader<'a> {
    fn new(b: &'a [u8]) -> Self {
        BinReader { b, at: 0 }
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| corrupt("length overflow"))?;
        let out = self
            .b
            .get(self.at..end)
            .ok_or_else(|| corrupt("unexpected end of model"))?;
        self.at = end;
        Ok(out)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        let b = self.bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Result<u64> {
        let b = self.bytes(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    fn at_end(&self) -> bool {
        self.at == self.b.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_names_are_stable() {
        assert_eq!(version_name(0x00C1), "word97");
        assert_eq!(version_name(0x00D9), "word2000");
        assert_eq!(version_name(0x0101), "word2002");
        assert_eq!(version_name(0x010C), "word2003");
        assert_eq!(version_name(0x0065), "word97-2003");
    }

    #[test]
    fn nfib_range_is_word_97_to_2003() {
        assert!(!n_fib_plausible(0x0065)); // Word 6/95
        assert!(n_fib_plausible(0x00C1));
        assert!(n_fib_plausible(0x010C));
        assert!(!n_fib_plausible(0x010D));
    }

    #[test]
    fn cp1252_maps_the_high_range() {
        assert_eq!(cp1252(0x41), 0x0041);
        assert_eq!(cp1252(0x80), 0x20AC);
        assert_eq!(cp1252(0xE9), 0x00E9);
        assert_eq!(cp1252(0xFF), 0x00FF);
        assert_eq!(cp1252(0x81), 0xFFFD);
    }

    #[test]
    fn prose_is_not_a_doc() {
        assert!(!detect(
            b"The quick brown fox jumps over the lazy dog.\nPlain prose.\n",
            Limits::DEFAULT
        ));
    }

    fn fib_with(ccp: [u32; 7]) -> DocFib {
        DocFib {
            w_ident: FIB_MAGIC,
            n_fib: 0x00C1,
            n_product: 0,
            lid: 0x0409,
            pn_next: 0,
            flags: 0,
            n_fib_back: 0x00C1,
            l_key: 0,
            envr: 0,
            flags2: 0,
            fc_min: 0,
            fc_mac: 0,
            cb_mac: 0,
            ccp_text: ccp[0],
            ccp_ftn: ccp[1],
            ccp_hdd: ccp[2],
            ccp_atn: ccp[3],
            ccp_edn: ccp[4],
            ccp_txbx: ccp[5],
            ccp_hdr_txbx: ccp[6],
            fc_clx: 0,
            lcb_clx: 0,
            csw: 14,
            cslw: 22,
            cb_rg_fc_lcb: 93,
        }
    }

    #[test]
    fn section_names_and_lookup_are_stable() {
        assert_eq!(DocSection::ALL.len(), 7);
        assert_eq!(DocSection::Main.name(), "main");
        assert_eq!(DocSection::Footnote.name(), "footnote");
        assert_eq!(DocSection::Header.name(), "header");
        assert_eq!(DocSection::Annotation.name(), "annotation");
        assert_eq!(DocSection::Endnote.name(), "endnote");
        assert_eq!(DocSection::Textbox.name(), "textbox");
        assert_eq!(DocSection::HdrTextbox.name(), "hdrtextbox");
        assert_eq!(DocSection::from_name("endnote"), Some(DocSection::Endnote));
        assert_eq!(DocSection::from_name("nope"), None);
        assert_eq!(DocSection::Main.fib_field(), "ccpText");
        assert_eq!(DocSection::HdrTextbox.fib_field(), "ccpHdrTxbx");
    }

    #[test]
    fn section_ranges_are_cumulative_and_empty_is_zero_length() {
        let fib = fib_with([6, 4, 3, 3, 3, 3, 2]);
        assert_eq!(fib.total_cp(), 24);
        for (s, want) in [
            (DocSection::Main, (0, 6)),
            (DocSection::Footnote, (6, 10)),
            (DocSection::Header, (10, 13)),
            (DocSection::Annotation, (13, 16)),
            (DocSection::Endnote, (16, 19)),
            (DocSection::Textbox, (19, 22)),
            (DocSection::HdrTextbox, (22, 24)),
        ] {
            assert_eq!(fib.section_range(s), want, "{}", s.name());
        }
        // A zero-count section is a zero-length range, not an error.
        let empty = fib_with([6, 0, 0, 0, 0, 0, 0]);
        assert_eq!(empty.section_range(DocSection::Footnote), (6, 6));
        assert_eq!(empty.section_range(DocSection::HdrTextbox), (6, 6));
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut seed: u64 = 0x1234_5678_9abc_def0;
        for _ in 0..1024 {
            let mut buf = [0u8; 1024];
            for b in buf.iter_mut() {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                *b = (seed >> 33) as u8;
            }
            let _ = detect(&buf, Limits::DEFAULT);
            let _ = parse(&buf, Limits::STRICT);
        }
        // The signature-prefixed CFBF path is the most likely false-positive route.
        let mut buf = vec![0u8; 2048];
        buf[..8].copy_from_slice(&crate::adapter::cfbf::SIGNATURE);
        for _ in 0..256 {
            for b in buf[8..].iter_mut() {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                *b = (seed >> 33) as u8;
            }
            let _ = detect(&buf, Limits::STRICT);
            let _ = parse(&buf, Limits::STRICT);
        }
    }
}
