//! Bounded, representation-preserving legacy PowerPoint 97–2003 (`.ppt`, MS-PPT)
//! **semantic** adapter (Phase 26.3), built on the CFBF/OLE2 physical layer (21.35).
//!
//! A `.ppt` is a CFBF compound file whose `PowerPoint Document` stream is a
//! **record stream**: a byte-packed sequence of `RecordHeader` (8 bytes) + payload
//! records, nested into containers. This adapter **reuses the CFBF resolver**
//! ([`crate::adapter::cfbf`]) to obtain that stream as an exact byte span — it never
//! writes a second OLE parser — and adds only a bounded semantic model over the
//! records.
//!
//! ## Record framing (MS-PPT `RecordHeader`)
//!
//! Each record is an 8-byte header followed by `recLen` payload bytes:
//!
//! ```text
//! w   : u16 little-endian  →  recVer      = w & 0x000F        (4 bits)
//!                             recInstance = (w >> 4) & 0x0FFF  (12 bits)
//! recType : u16 little-endian
//! recLen  : u32 little-endian
//! ```
//!
//! `recVer == 0xF` marks a **container** (its payload is a nested record sequence);
//! every other value is a leaf record. The `Document` record (`recType 1000`,
//! `recVer 0xF`) therefore opens with the bytes `0F 00 E8 03 …` on the wire. This is
//! the MS-PPT bit layout (`recVer` is the **low** nibble of the little-endian
//! `u16`); the *observable* claim the task states — the stream begins with a
//! `Document` record of `recVer 0xF`, `recType 1000` — holds for real files under
//! exactly this layout.
//!
//! ## What is authoritative
//!
//! Like every Wave-2 format, a `.ppt` has **no *separate* package layer**: the exact
//! leaf is the whole source (a `DocumentExact`) and every derived observation is a
//! bounded, span-preserving (`Q_gen`) projection of it. Nothing is re-encoded,
//! re-serialized, or re-OLE'd for exactness.
//!
//! ## What is parsed (the documented subset)
//!
//! * the **record stream** of the `PowerPoint Document` stream: every record's
//!   `recVer`/`recInstance`/`recType`/`recLen`, its **exact byte span** (and its
//!   offset within the stream), and its **nesting depth** (containers enclose their
//!   children). A truncated record header, a `recLen` that overruns the stream, and a
//!   record that overruns its enclosing container all decline typed;
//! * the **`Document`** record (`recType 1000`, `recVer 0xF`) at the stream start;
//! * the **`SlideListWithText`** containers (`recType 4080`, `recVer 0xF`) and their
//!   **`SlidePersistAtom`** entries (`recType 1011`): each atom's `persistIdRef` (its
//!   slide identity) and `cTexts`, giving the **slide order**. All four lists are
//!   modeled, distinguished by `recInstance`: `slides` (0), `masters` (1), `notes` (2),
//!   and `handout` (3); the slides list additionally defines the legacy slide/run
//!   projection ([`PptModel::slides`]/[`PptModel::runs`]);
//! * the **text runs**: for each `TextPersist` group, a **`TextHeaderAtom`**
//!   (`recType 3999`), then a **`TextCharsAtom`** (`recType 4000`, UTF-16LE) or a
//!   **`TextBytesAtom`** (`recType 4008`, CP1252). Each run records its kind, its
//!   slide, the **exact span of its text atom** (and of the preceding header atom),
//!   and the decoded text. A `TextCharsAtom` whose `recLen` is not a multiple of two
//!   declines typed; a text atom with no preceding `SlidePersistAtom` declines typed;
//! * the **`Current User`** stream's `UserEditAtom` offset when the stream exists and
//!   its declared offset resolves to a `UserEditAtom` (`recType 4085`) record; a
//!   present but unresolvable `Current User` stream declines typed.
//!
//! Slide order, text-run order, each run's exact span, and the decoded text are all
//! preserved; nothing is normalized.
//!
//! ## The boundary it cannot cross (honest)
//!
//! Detection is built **on top of** the CFBF detector: a source is claimed only when
//! the CFBF layer accepts it **and** its `PowerPoint Document` stream begins with a
//! `Document` record (`recVer 0xF`, `recType 1000`). A generic compound file stays
//! [`Cfbf`](crate::field::document_format::DocumentFormat::Cfbf); a `.pptx` (an OPC
//! ZIP) never reaches here.
//!
//! A `.ppt` that carries a plausible `Document` record but a malformed or
//! out-of-subset body (a `recLen` that overruns the stream, a truncated record
//! header, a record that overruns its container, an odd-length `TextCharsAtom`, a
//! text atom with no `SlidePersistAtom`, an out-of-range `Current User` offset, or a
//! missing `PowerPoint Document` stream) is admitted by detection only when the
//! leading `Document` record is well-framed; its derived model then **declines
//! typed** ([`crate::ErrorClass::InvalidPptStructure`]). Never a panic, never a
//! guess.

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model. Version 2 added the modeled
/// `SlideListWithText` list arena (all four instances); the legacy slides/runs
/// projection is derived from it.
pub const MODEL_VERSION: u8 = 2;
/// Hard cap on decoded model records (defends the decoder against a hostile blob).
pub const MAX_MODEL_RECORDS: u32 = 1 << 24;
/// Hard cap on decoded model slides.
pub const MAX_MODEL_SLIDES: u32 = 1 << 20;
/// Hard cap on decoded model text runs.
pub const MAX_MODEL_RUNS: u32 = 1 << 24;
/// Hard cap on decoded model `SlideListWithText` lists.
pub const MAX_MODEL_LISTS: u32 = 1 << 12;
/// Hard cap on decoded text code units.
pub const MAX_MODEL_TEXT_UNITS: u32 = 1 << 30;

/// The `recVer` value marking a container record.
pub const REC_VER_CONTAINER: u8 = 0x0F;

/// The `Document` container (`recType 1000`).
pub const REC_DOCUMENT: u16 = 1000;
/// A `SlideListWithText` container (`recType 4080`).
pub const REC_SLIDE_LIST_WITH_TEXT: u16 = 4080;
/// A `SlidePersistAtom` (`recType 1011`).
pub const REC_SLIDE_PERSIST_ATOM: u16 = 1011;
/// A `TextHeaderAtom` (`recType 3999`).
pub const REC_TEXT_HEADER_ATOM: u16 = 3999;
/// A `TextCharsAtom` (UTF-16LE, `recType 4000`).
pub const REC_TEXT_CHARS_ATOM: u16 = 4000;
/// A `TextBytesAtom` (CP1252, `recType 4008`).
pub const REC_TEXT_BYTES_ATOM: u16 = 4008;
/// A `UserEditAtom` (`recType 4085`).
pub const REC_USER_EDIT_ATOM: u16 = 4085;

/// The `SlideListWithText` instance selecting the **slides** list.
pub const SLIDE_LIST_SLIDES: u16 = 0;
/// The `SlideListWithText` instance selecting the **master** list.
pub const SLIDE_LIST_MASTER: u16 = 1;
/// The `SlideListWithText` instance selecting the **notes** list.
pub const SLIDE_LIST_NOTES: u16 = 2;
/// The `SlideListWithText` instance selecting the **handout** list.
pub const SLIDE_LIST_HANDOUT: u16 = 3;
/// The four `SlideListWithText` instances this adapter models, in wire order.
pub const SLIDE_LIST_KINDS: [u16; 4] = [
    SLIDE_LIST_SLIDES,
    SLIDE_LIST_MASTER,
    SLIDE_LIST_NOTES,
    SLIDE_LIST_HANDOUT,
];

/// Run kind: a `TextCharsAtom` (UTF-16LE).
pub const RUN_CHARS: u8 = 0;
/// Run kind: a `TextBytesAtom` (CP1252).
pub const RUN_BYTES: u8 = 1;

/// Match field: a decoded text run.
pub const FIELD_RUN: u8 = 0;

/// The `PowerPoint Document` stream name.
pub const STREAM_POWERPOINT_DOCUMENT: &str = "PowerPoint Document";
/// The `Current User` stream name.
pub const STREAM_CURRENT_USER: &str = "Current User";

/// A stable lowercase name for a run kind.
pub const fn run_kind_name(kind: u8) -> &'static str {
    match kind {
        RUN_BYTES => "bytes",
        _ => "chars",
    }
}

/// A stable name for a recognized record type.
pub const fn record_type_name(rec_type: u16) -> &'static str {
    match rec_type {
        REC_DOCUMENT => "document",
        REC_SLIDE_LIST_WITH_TEXT => "slide-list-with-text",
        REC_SLIDE_PERSIST_ATOM => "slide-persist-atom",
        REC_TEXT_HEADER_ATOM => "text-header-atom",
        REC_TEXT_CHARS_ATOM => "text-chars-atom",
        REC_TEXT_BYTES_ATOM => "text-bytes-atom",
        REC_USER_EDIT_ATOM => "user-edit-atom",
        _ => "unknown",
    }
}

/// A stable name for a `SlideListWithText` instance (`slides`, `masters`, `notes`,
/// `handout`; `other` for an unrecognized instance).
pub const fn slide_list_name(instance: u16) -> &'static str {
    match instance {
        SLIDE_LIST_SLIDES => "slides",
        SLIDE_LIST_MASTER => "masters",
        SLIDE_LIST_NOTES => "notes",
        SLIDE_LIST_HANDOUT => "handout",
        _ => "other",
    }
}

/// Resolve a `SlideListWithText` instance by its stable name (`slides`, `masters`,
/// `notes`, `handout`).
pub fn slide_list_from_name(name: &str) -> Option<u16> {
    SLIDE_LIST_KINDS
        .into_iter()
        .find(|&i| slide_list_name(i) == name)
}

/// The stable match-field name.
pub const fn field_name(field: u8) -> &'static str {
    let _ = field;
    "run"
}

/// One MS-PPT record with its exact source span and stream offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PptRecord {
    /// The record version nibble (low four bits of the first `u16`).
    pub rec_ver: u8,
    /// The record instance (high twelve bits of the first `u16`).
    pub rec_instance: u16,
    /// The record type.
    pub rec_type: u16,
    /// `(offset, length)` of the whole record (header + payload) in the source.
    pub span: (u64, u64),
    /// The record's offset within the `PowerPoint Document` stream.
    pub stream_off: u64,
    /// The payload length in bytes.
    pub len: u32,
    /// The nesting depth (a direct child of `Document` is depth 1).
    pub depth: u32,
}

impl PptRecord {
    /// Whether this record is a container (`recVer == 0xF`).
    pub const fn is_container(&self) -> bool {
        self.rec_ver == REC_VER_CONTAINER
    }

    /// `(offset, length)` of the record's payload in the source.
    pub fn payload_span(&self) -> (u64, u64) {
        (self.span.0 + 8, self.span.1)
    }
}

/// One slide entry (`SlidePersistAtom`) of a `SlideListWithText`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PptSlide {
    /// `persistIdRef`: the slide's persistent identity.
    pub persist_id_ref: u32,
    /// `cTexts`: the declared number of text records (informational).
    pub c_texts: u32,
    /// The enclosing `SlideListWithText` instance ([`SLIDE_LIST_SLIDES`] etc.).
    pub list_instance: u16,
    /// `(offset, length)` of the `SlidePersistAtom` record.
    pub sp_span: (u64, u64),
    /// The slide's run arena range `[run_start, run_end)`.
    pub run_start: u32,
    pub run_end: u32,
}

/// One text run (`TextCharsAtom`/`TextBytesAtom`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PptRun {
    /// The owning slide's index in the slide list.
    pub slide: u32,
    /// [`RUN_CHARS`] or [`RUN_BYTES`].
    pub kind: u8,
    /// The text atom's record type.
    pub rec_type: u16,
    /// `(offset, length)` of the preceding `TextHeaderAtom` (or `(0, 0)`).
    pub header_span: (u64, u64),
    /// `(offset, length)` of the text atom record (header + payload).
    pub span: (u64, u64),
    /// The decoded text (Unicode code units; exact).
    pub text: Vec<u16>,
}

/// One text run inside a `SlideListWithText` list: its kind, its exact atom/header
/// spans, and its decoded text. [`PptListRun::slide`] is the run's slide index within
/// the enclosing [`PptList`] (not the global slide arena).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PptListRun {
    /// The owning slide's index within the enclosing [`PptList`].
    pub slide: u32,
    /// [`RUN_CHARS`] or [`RUN_BYTES`].
    pub kind: u8,
    /// The text atom's record type.
    pub rec_type: u16,
    /// `(offset, length)` of the preceding `TextHeaderAtom` (or `(0, 0)`).
    pub header_span: (u64, u64),
    /// `(offset, length)` of the text atom record (header + payload).
    pub span: (u64, u64),
    /// The decoded text (Unicode code units; exact).
    pub text: Vec<u16>,
}

/// One slide entry (`SlidePersistAtom`) of a [`PptList`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PptListSlide {
    /// `persistIdRef`: the slide's persistent identity.
    pub persist_id_ref: u32,
    /// `cTexts`: the declared number of text records (informational).
    pub c_texts: u32,
    /// `(offset, length)` of the `SlidePersistAtom` record.
    pub sp_span: (u64, u64),
    /// The slide's run range `[run_start, run_end)` within the enclosing [`PptList`].
    pub run_start: u32,
    pub run_end: u32,
}

/// One modeled `SlideListWithText` list (`recType 4080`): its `recInstance`, the exact
/// span of the container record, and its ordered `SlidePersistAtom` slides with their
/// ordered text runs. All four lists (`slides`, `masters`, `notes`, `handout`) are
/// materialized; the slides list additionally defines [`PptModel::slides`] /
/// [`PptModel::runs`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PptList {
    /// The `SlideListWithText` instance ([`SLIDE_LIST_SLIDES`] etc.).
    pub instance: u16,
    /// `(offset, length)` of the `SlideListWithText` container record.
    pub span: (u64, u64),
    /// The slides, in `SlidePersistAtom` order.
    pub slides: Vec<PptListSlide>,
    /// The text-run arena (slide order, then record order), local to this list.
    pub runs: Vec<PptListRun>,
}

impl PptList {
    /// The slide at `index`, if present.
    pub fn slide(&self, index: u32) -> Option<&PptListSlide> {
        self.slides.get(index as usize)
    }

    /// The run arena slice of a slide.
    pub fn slide_runs(&self, slide: &PptListSlide) -> &[PptListRun] {
        let s = slide.run_start as usize;
        let e = slide.run_end as usize;
        self.runs.get(s..e).unwrap_or(&[])
    }

    /// The concatenated decoded text of the list: each slide's runs (newline-joined)
    /// with slides newline-separated.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for (i, slide) in self.slides.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            for (k, run) in self.slide_runs(slide).iter().enumerate() {
                if k > 0 {
                    out.push('\n');
                }
                out.push_str(&String::from_utf16_lossy(&run.text));
            }
        }
        out
    }
}

/// The recorded `Current User` stream (`UserEditAtom` offset).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PptCurrentUser {
    /// `(offset, length)` of the whole `Current User` stream in the source.
    pub span: (u64, u64),
    /// The declared offset (from the stream start) to the `UserEditAtom`.
    pub edit_offset: u32,
    /// `(offset, length)` of the resolved `UserEditAtom` region in the source.
    pub user_edit_span: (u64, u64),
}

/// The canonical derived legacy-PPT model (the materialization of a `PptModel` node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PptModel {
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// `(offset, length)` of the `PowerPoint Document` stream in the source.
    pub stream_span: (u64, u64),
    /// Every record of the stream, in order.
    pub records: Vec<PptRecord>,
    /// Every modeled `SlideListWithText` list, in record order (all four instances).
    pub lists: Vec<PptList>,
    /// The slides of the **slides** list, in `SlidePersistAtom` order (derived from
    /// [`Self::lists`]).
    pub slides: Vec<PptSlide>,
    /// The slides-list text-run arena (slide order, then record order; derived from
    /// [`Self::lists`]).
    pub runs: Vec<PptRun>,
    /// The `Current User` stream's `UserEditAtom` offset, if present.
    pub current_user: Option<PptCurrentUser>,
}

impl PptModel {
    /// The `SlideListWithText` list for `instance`, if the source carries one.
    pub fn list(&self, instance: u16) -> Option<&PptList> {
        self.lists.iter().find(|l| l.instance == instance)
    }

    /// The slide at `index`, if present.
    pub fn slide(&self, index: u32) -> Option<&PptSlide> {
        self.slides.get(index as usize)
    }

    /// The run at arena `index`, if present.
    pub fn run(&self, index: u32) -> Option<&PptRun> {
        self.runs.get(index as usize)
    }

    /// The run arena slice of a slide.
    pub fn slide_runs(&self, slide: &PptSlide) -> &[PptRun] {
        let s = slide.run_start as usize;
        let e = slide.run_end as usize;
        self.runs.get(s..e).unwrap_or(&[])
    }

    /// The number of `TextCharsAtom` runs.
    pub fn char_runs(&self) -> u64 {
        self.runs.iter().filter(|r| r.kind == RUN_CHARS).count() as u64
    }

    /// The number of `TextBytesAtom` runs.
    pub fn byte_runs(&self) -> u64 {
        self.runs.iter().filter(|r| r.kind == RUN_BYTES).count() as u64
    }

    /// The aggregate decoded text code-unit count (the byte budget the caller checks
    /// against `max_ppt_text_bytes`).
    pub fn text_units(&self) -> u64 {
        self.runs.iter().map(|r| r.text.len() as u64).sum()
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        out.extend_from_slice(b"PPTM");
        out.push(MODEL_VERSION);
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        w_span(&mut out, self.stream_span);
        // records
        out.extend_from_slice(&(self.records.len() as u32).to_le_bytes());
        for r in &self.records {
            out.push(r.rec_ver);
            out.extend_from_slice(&r.rec_instance.to_le_bytes());
            out.extend_from_slice(&r.rec_type.to_le_bytes());
            w_span(&mut out, r.span);
            out.extend_from_slice(&r.stream_off.to_le_bytes());
            out.extend_from_slice(&r.len.to_le_bytes());
            out.extend_from_slice(&r.depth.to_le_bytes());
        }
        // SlideListWithText lists (all four instances; slides/runs are derived on decode).
        out.extend_from_slice(&(self.lists.len() as u32).to_le_bytes());
        for l in &self.lists {
            out.extend_from_slice(&l.instance.to_le_bytes());
            w_span(&mut out, l.span);
            out.extend_from_slice(&(l.slides.len() as u32).to_le_bytes());
            for s in &l.slides {
                out.extend_from_slice(&s.persist_id_ref.to_le_bytes());
                out.extend_from_slice(&s.c_texts.to_le_bytes());
                w_span(&mut out, s.sp_span);
                out.extend_from_slice(&s.run_start.to_le_bytes());
                out.extend_from_slice(&s.run_end.to_le_bytes());
            }
            out.extend_from_slice(&(l.runs.len() as u32).to_le_bytes());
            for r in &l.runs {
                out.extend_from_slice(&r.slide.to_le_bytes());
                out.push(r.kind);
                out.extend_from_slice(&r.rec_type.to_le_bytes());
                w_span(&mut out, r.header_span);
                w_span(&mut out, r.span);
                w_units(&mut out, &r.text);
            }
        }
        // current user
        match &self.current_user {
            None => out.push(0),
            Some(cu) => {
                out.push(1);
                w_span(&mut out, cu.span);
                out.extend_from_slice(&cu.edit_offset.to_le_bytes());
                w_span(&mut out, cu.user_edit_span);
            }
        }
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<PptModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"PPTM" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let doc_len = r.u64()?;
        let stream_span = r.span()?;
        check_span(stream_span, doc_len, "PowerPoint Document stream span")?;
        let stream_len = stream_span.1 - stream_span.0;
        let n_records = r.u32()?;
        if n_records > MAX_MODEL_RECORDS {
            return Err(corrupt("model record count is implausible"));
        }
        let mut records = Vec::new();
        for _ in 0..n_records {
            let rec_ver = r.u8()?;
            let rec_instance = r.u16()?;
            let rec_type = r.u16()?;
            let span = r.span()?;
            check_span(span, doc_len, "record span")?;
            let stream_off = r.u64()?;
            let len = r.u32()?;
            let depth = r.u32()?;
            if span.1 - span.0 != 8 + len as u64 {
                return Err(corrupt("record span disagrees with its length"));
            }
            if stream_off + span.1 - span.0 > stream_len {
                return Err(corrupt(
                    "record lies outside the PowerPoint Document stream",
                ));
            }
            records.push(PptRecord {
                rec_ver,
                rec_instance,
                rec_type,
                span,
                stream_off,
                len,
                depth,
            });
        }
        let n_lists = r.u32()?;
        if n_lists > MAX_MODEL_LISTS {
            return Err(corrupt("model list count is implausible"));
        }
        let mut lists = Vec::new();
        let mut slides_total: u64 = 0;
        let mut runs_total: u64 = 0;
        let mut units_total: u64 = 0;
        for _ in 0..n_lists {
            let instance = r.u16()?;
            let span = r.span()?;
            check_span(span, doc_len, "SlideListWithText span")?;
            let n_slides = r.u32()?;
            slides_total = slides_total.saturating_add(n_slides as u64);
            if slides_total > MAX_MODEL_SLIDES as u64 {
                return Err(corrupt("model slide count is implausible"));
            }
            let mut slides = Vec::new();
            for _ in 0..n_slides {
                let persist_id_ref = r.u32()?;
                let c_texts = r.u32()?;
                let sp_span = r.span()?;
                check_span(sp_span, doc_len, "SlidePersistAtom span")?;
                let run_start = r.u32()?;
                let run_end = r.u32()?;
                slides.push(PptListSlide {
                    persist_id_ref,
                    c_texts,
                    sp_span,
                    run_start,
                    run_end,
                });
            }
            let n_runs = r.u32()?;
            runs_total = runs_total.saturating_add(n_runs as u64);
            if runs_total > MAX_MODEL_RUNS as u64 {
                return Err(corrupt("model run count is implausible"));
            }
            let mut runs = Vec::new();
            for _ in 0..n_runs {
                let slide = r.u32()?;
                let kind = r.u8()?;
                let rec_type = r.u16()?;
                let header_span = r.span()?;
                check_span(header_span, doc_len, "run header span")?;
                let span = r.span()?;
                check_span(span, doc_len, "run span")?;
                let text = r.units()?;
                units_total = units_total.saturating_add(text.len() as u64);
                if units_total > MAX_MODEL_TEXT_UNITS as u64 {
                    return Err(corrupt("model text is implausible"));
                }
                runs.push(PptListRun {
                    slide,
                    kind,
                    rec_type,
                    header_span,
                    span,
                    text,
                });
            }
            for s in &slides {
                if s.run_start > s.run_end || s.run_end as usize > runs.len() {
                    return Err(corrupt("a slide run range is out of bounds"));
                }
            }
            for run in &runs {
                if run.slide as usize >= slides.len() {
                    return Err(corrupt("a run names an out-of-range slide"));
                }
            }
            lists.push(PptList {
                instance,
                span,
                slides,
                runs,
            });
        }
        let (slides, runs) = derive_slides_runs(&lists);
        let current_user = if r.u8()? != 0 {
            let span = r.span()?;
            check_span(span, doc_len, "Current User stream span")?;
            let edit_offset = r.u32()?;
            let user_edit_span = r.span()?;
            check_span(user_edit_span, doc_len, "UserEditAtom span")?;
            Some(PptCurrentUser {
                span,
                edit_offset,
                user_edit_span,
            })
        } else {
            None
        };
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        Ok(PptModel {
            doc_len,
            stream_span,
            records,
            lists,
            slides,
            runs,
            current_user,
        })
    }
}

fn check_span(span: (u64, u64), bound: u64, what: &'static str) -> Result<()> {
    if span.0 > span.1 || span.1 > bound {
        return Err(corrupt(format!("{what} is outside its container")));
    }
    Ok(())
}

/// A lexical-match record from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PptMatch {
    /// [`FIELD_RUN`].
    pub field: u8,
    /// The run's arena index.
    pub index: u32,
    /// The run's first byte offset in the source.
    pub start: u64,
    /// One past the run's last byte in the source.
    pub end: u64,
    /// The matched run's decoded text.
    pub text: String,
}

/// A bounded, case-sensitive lexical search over each text run's decoded text.
pub fn find(model: &PptModel, pattern: &str, max_bytes: u64) -> Result<Vec<PptMatch>> {
    if pattern.is_empty() {
        return Ok(Vec::new());
    }
    let mut out: Vec<PptMatch> = Vec::new();
    let mut estimated: u64 = 0;
    for (i, r) in model.runs.iter().enumerate() {
        let text = String::from_utf16_lossy(&r.text);
        if text.contains(pattern) {
            estimated = estimated.saturating_add(64 + text.len() as u64);
            if estimated > max_bytes {
                return Err(Error::resource_limit(format!(
                    "ppt find exceeded the {max_bytes}-byte budget"
                )));
            }
            out.push(PptMatch {
                field: FIELD_RUN,
                index: i as u32,
                start: r.span.0,
                end: r.span.1,
                text,
            });
        }
    }
    Ok(out)
}

/// The canonical text rendering of the model: the slide-ordered run texts. Bounded by
/// `max_bytes`.
pub fn canonical_text(model: &PptModel, _source: &[u8], max_bytes: u64) -> Result<String> {
    let mut out = String::new();
    for (i, slide) in model.slides.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        for (k, run) in model.slide_runs(slide).iter().enumerate() {
            if k > 0 {
                out.push('\n');
            }
            out.push_str(&String::from_utf16_lossy(&run.text));
        }
        if out.len() as u64 > max_bytes {
            return Err(Error::resource_limit(format!(
                "ppt canonical text exceeded the {max_bytes}-byte budget"
            )));
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Fast-fail legacy-PPT detector, built **on top of** the CFBF detector: the source
/// must be a compound file the CFBF layer accepts and its `PowerPoint Document`
/// stream must begin with a `Document` record (`recVer 0xF`, `recType 1000`). Never
/// consults a file name.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.len() as u64 > limits.max_ppt_document_bytes {
        return false;
    }
    if !crate::adapter::cfbf::detect(source, limits) {
        return false;
    }
    let Ok(model) = crate::adapter::cfbf::parse(source, limits) else {
        return false;
    };
    let Some(entry) = find_named(&model, STREAM_POWERPOINT_DOCUMENT) else {
        return false;
    };
    // Read only the leading record header: a genuine `.ppt` whose `PowerPoint
    // Document` chain is **fragmented** is still a `.ppt`, and its first record
    // lives in the stream's first chunk. Detection therefore reads the prefix
    // rather than requiring the whole stream to be one contiguous span.
    let Ok(stream) = crate::adapter::cfbf::stream_leading(source, &model, entry, 8) else {
        return false;
    };
    if stream.len() < 8 {
        return false;
    }
    let w = u16::from_le_bytes([stream[0], stream[1]]);
    let rec_ver = (w & 0x000F) as u8;
    let rec_type = u16::from_le_bytes([stream[2], stream[3]]);
    rec_ver == REC_VER_CONTAINER && rec_type == REC_DOCUMENT
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse `source` into a [`PptModel`] under `limits`. Declines typed on any malformed
/// or over-large input; never panics.
pub fn parse(source: &[u8], limits: Limits) -> Result<PptModel> {
    let len = source.len() as u64;
    if len > limits.max_ppt_document_bytes {
        return Err(Error::resource_limit(format!(
            "ppt source is {len} bytes, above the {}-byte document cap",
            limits.max_ppt_document_bytes
        )));
    }
    let cfbf = crate::adapter::cfbf::parse(source, limits)?;
    let entry = find_named(&cfbf, STREAM_POWERPOINT_DOCUMENT)
        .ok_or_else(|| invalid("the compound file has no PowerPoint Document stream"))?;
    let stream = crate::adapter::cfbf::stream_bytes(source, entry)?;
    if stream.len() as u64 > limits.max_ppt_document_bytes {
        return Err(Error::resource_limit(format!(
            "the PowerPoint Document stream is {} bytes, above the {}-byte document cap",
            stream.len(),
            limits.max_ppt_document_bytes
        )));
    }
    let base = entry.data_span.0;
    let stream_span = (base, base + stream.len() as u64);

    let records = scan_records(stream, base, limits)?;
    let first = records
        .first()
        .ok_or_else(|| invalid("the PowerPoint Document stream is empty"))?;
    if first.rec_ver != REC_VER_CONTAINER || first.rec_type != REC_DOCUMENT {
        return Err(invalid(
            "the stream does not begin with a Document record (recVer 0xF, recType 1000)",
        ));
    }

    let lists = parse_lists(stream, &records, limits)?;

    let text_units: u64 = lists
        .iter()
        .flat_map(|l| l.runs.iter())
        .map(|r| r.text.len() as u64)
        .sum();
    if text_units > limits.max_ppt_text_bytes {
        return Err(Error::resource_limit(format!(
            "ppt derived text is above the {}-byte text cap",
            limits.max_ppt_text_bytes
        )));
    }

    let (slides, runs) = derive_slides_runs(&lists);
    let current_user = parse_current_user(source, &cfbf)?;

    Ok(PptModel {
        doc_len: len,
        stream_span,
        records,
        lists,
        slides,
        runs,
        current_user,
    })
}

/// Parse `source` and return its canonical derived model, encoded (the materialization
/// of a `PptModel` node).
pub fn build_ppt_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits)?.encode())
}

/// Scan the `PowerPoint Document` stream into its flat record list with exact spans and
/// nesting depths (a container's payload is its children, so the walk descends into a
/// container before resuming its siblings).
fn scan_records(stream: &[u8], base: u64, limits: Limits) -> Result<Vec<PptRecord>> {
    let mut out: Vec<PptRecord> = Vec::new();
    let mut off = 0usize;
    // A stack of enclosing container end offsets (stream-relative).
    let mut stack: Vec<usize> = Vec::new();
    while off < stream.len() {
        // Drop any containers we have already finished.
        while stack.last().is_some_and(|&e| off >= e) {
            stack.pop();
        }
        if off >= stream.len() {
            break;
        }
        if stream.len() - off < 8 {
            return Err(invalid("a record header is truncated"));
        }
        let w = u16::from_le_bytes([stream[off], stream[off + 1]]);
        let rec_ver = (w & 0x000F) as u8;
        let rec_instance = (w >> 4) & 0x0FFF;
        let rec_type = u16::from_le_bytes([stream[off + 2], stream[off + 3]]);
        let rec_len = u32::from_le_bytes([
            stream[off + 4],
            stream[off + 5],
            stream[off + 6],
            stream[off + 7],
        ]);
        let rec_end = off
            .checked_add(8)
            .and_then(|o| o.checked_add(rec_len as usize))
            .ok_or_else(|| invalid("a record length overflows"))?;
        if rec_end > stream.len() {
            return Err(invalid("a record extends past the stream"));
        }
        let parent_end = stack.last().copied().unwrap_or(stream.len());
        if rec_end > parent_end {
            return Err(invalid("a record extends past its enclosing container"));
        }
        let depth = stack.len() as u32;
        if depth > limits.max_ppt_depth {
            return Err(Error::resource_limit(format!(
                "the record nesting depth exceeds {}",
                limits.max_ppt_depth
            )));
        }
        if out.len() as u64 >= limits.max_ppt_records {
            return Err(Error::resource_limit(format!(
                "the PowerPoint Document stream has more than {} records",
                limits.max_ppt_records
            )));
        }
        out.push(PptRecord {
            rec_ver,
            rec_instance,
            rec_type,
            span: (base + off as u64, base + rec_end as u64),
            stream_off: off as u64,
            len: rec_len,
            depth,
        });
        if rec_ver == REC_VER_CONTAINER {
            // Descend into the container's children.
            stack.push(rec_end);
            off += 8;
        } else {
            off = rec_end;
        }
    }
    Ok(out)
}

/// Decode a text atom's payload into `(kind, text)` (`TextCharsAtom` is UTF-16LE;
/// `TextBytesAtom` is CP1252).
fn decode_text_atom(pl: &[u8], rec_type: u16) -> Result<(u8, Vec<u16>)> {
    if rec_type == REC_TEXT_CHARS_ATOM {
        if !pl.len().is_multiple_of(2) {
            return Err(invalid(
                "a TextCharsAtom payload length is not a multiple of two",
            ));
        }
        let text: Vec<u16> = pl
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        Ok((RUN_CHARS, text))
    } else {
        Ok((RUN_BYTES, pl.iter().map(|&b| cp1252(b)).collect()))
    }
}

/// Parse the `SlideListWithText` containers (all four instances) into their lists, in
/// record order. Every list is span-preserving: each `SlidePersistAtom` and each text
/// atom keeps its exact source span and stream-relative order, and each run is bound to
/// its `persistIdRef`. A text atom that precedes any `SlidePersistAtom` in a list, a
/// truncated `SlidePersistAtom`, and an odd-length `TextCharsAtom` all decline typed; a
/// `SlideListWithText` with no slides (and an absent one) is an empty list, not an error.
fn parse_lists(stream: &[u8], records: &[PptRecord], limits: Limits) -> Result<Vec<PptList>> {
    let mut lists: Vec<PptList> = Vec::new();
    let mut slides_total: u64 = 0;
    let mut runs_total: u64 = 0;
    for (i, r) in records.iter().enumerate() {
        if r.rec_type != REC_SLIDE_LIST_WITH_TEXT || r.rec_ver != REC_VER_CONTAINER {
            continue;
        }
        let container_end = r.stream_off + 8 + r.len as u64;
        let child_depth = r.depth + 1;
        let mut cur: Option<usize> = None;
        let mut header_span: (u64, u64) = (0, 0);
        let mut slides: Vec<PptListSlide> = Vec::new();
        let mut runs: Vec<PptListRun> = Vec::new();
        for c in &records[i + 1..] {
            if c.stream_off >= container_end {
                break;
            }
            if c.depth != child_depth {
                continue;
            }
            match c.rec_type {
                REC_SLIDE_PERSIST_ATOM => {
                    let pl = payload(stream, c);
                    if pl.len() < 8 {
                        return Err(invalid("a SlidePersistAtom is truncated"));
                    }
                    let persist_id_ref = u32::from_le_bytes([pl[0], pl[1], pl[2], pl[3]]);
                    let c_texts = u32::from_le_bytes([pl[4], pl[5], pl[6], pl[7]]);
                    slides_total = slides_total.saturating_add(1);
                    if slides_total > limits.max_ppt_slides {
                        return Err(Error::resource_limit(format!(
                            "the presentation has more than {} slides",
                            limits.max_ppt_slides
                        )));
                    }
                    cur = Some(slides.len());
                    slides.push(PptListSlide {
                        persist_id_ref,
                        c_texts,
                        sp_span: c.span,
                        run_start: runs.len() as u32,
                        run_end: runs.len() as u32,
                    });
                }
                REC_TEXT_HEADER_ATOM => header_span = c.span,
                REC_TEXT_CHARS_ATOM | REC_TEXT_BYTES_ATOM => {
                    let Some(si) = cur else {
                        return Err(invalid(
                            "a text atom appears before any SlidePersistAtom in a slide list",
                        ));
                    };
                    let pl = payload(stream, c);
                    let (kind, text) = decode_text_atom(pl, c.rec_type)?;
                    runs_total = runs_total.saturating_add(1);
                    if runs_total > limits.max_ppt_runs {
                        return Err(Error::resource_limit(format!(
                            "the presentation has more than {} text runs",
                            limits.max_ppt_runs
                        )));
                    }
                    runs.push(PptListRun {
                        slide: si as u32,
                        kind,
                        rec_type: c.rec_type,
                        header_span,
                        span: c.span,
                        text,
                    });
                    slides[si].run_end = runs.len() as u32;
                    header_span = (0, 0);
                }
                _ => {}
            }
        }
        lists.push(PptList {
            instance: r.rec_instance,
            span: r.span,
            slides,
            runs,
        });
    }
    Ok(lists)
}

/// Derive the legacy slides/runs projection (the **slides** list only) from the list
/// arena. The resulting arenas are byte-for-byte the Phase-26.3 slides/runs (the slides
/// list's slides in order, each with its runs in record order).
fn derive_slides_runs(lists: &[PptList]) -> (Vec<PptSlide>, Vec<PptRun>) {
    let mut slides: Vec<PptSlide> = Vec::new();
    let mut runs: Vec<PptRun> = Vec::new();
    for list in lists {
        if list.instance != SLIDE_LIST_SLIDES {
            continue;
        }
        let slide_base = slides.len() as u32;
        let run_base = runs.len() as u32;
        for s in &list.slides {
            slides.push(PptSlide {
                persist_id_ref: s.persist_id_ref,
                c_texts: s.c_texts,
                list_instance: list.instance,
                sp_span: s.sp_span,
                run_start: run_base.saturating_add(s.run_start),
                run_end: run_base.saturating_add(s.run_end),
            });
        }
        for r in &list.runs {
            runs.push(PptRun {
                slide: slide_base.saturating_add(r.slide),
                kind: r.kind,
                rec_type: r.rec_type,
                header_span: r.header_span,
                span: r.span,
                text: r.text.clone(),
            });
        }
    }
    (slides, runs)
}

/// Parse the `Current User` stream's `UserEditAtom` offset, if the stream exists.
fn parse_current_user(
    source: &[u8],
    cfbf: &crate::adapter::cfbf::CfbfModel,
) -> Result<Option<PptCurrentUser>> {
    let Some(entry) = find_named(cfbf, STREAM_CURRENT_USER) else {
        return Ok(None);
    };
    let s = crate::adapter::cfbf::stream_bytes(source, entry)?;
    if s.len() < 4 {
        return Err(invalid("the Current User stream is too short"));
    }
    let edit_offset = u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as u64;
    let edit_end = edit_offset
        .checked_add(8)
        .ok_or_else(|| invalid("the Current User UserEditAtom offset overflows"))?;
    if edit_end > s.len() as u64 {
        return Err(invalid(
            "the Current User UserEditAtom offset lies outside the stream",
        ));
    }
    let o = edit_offset as usize;
    let rec_type = u16::from_le_bytes([s[o + 2], s[o + 3]]);
    if rec_type != REC_USER_EDIT_ATOM {
        return Err(invalid(
            "the Current User stream does not point at a UserEditAtom record",
        ));
    }
    let base = entry.data_span.0;
    Ok(Some(PptCurrentUser {
        span: (base, base + s.len() as u64),
        edit_offset: edit_offset as u32,
        user_edit_span: (base + edit_offset, base + s.len() as u64),
    }))
}

/// The payload bytes of a record within the stream.
fn payload<'a>(stream: &'a [u8], r: &PptRecord) -> &'a [u8] {
    let s = r.stream_off as usize + 8;
    let e = s + r.len as usize;
    stream.get(s..e).unwrap_or(&[])
}

/// Map one CP1252 byte to its Unicode code unit.
fn cp1252(b: u8) -> u16 {
    // The five CP1252-undefined positions map to U+FFFD (a derived-text caveat only;
    // the exact bytes remain available through the run's source span).
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
    model.entries.iter().find(|e| {
        e.obj_type == crate::adapter::cfbf::OBJ_STREAM
            && crate::adapter::cfbf::entry_name_text(e) == name
    })
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn invalid(message: impl Into<String>) -> Error {
    Error::invalid_ppt_structure(format!("ppt: {}", message.into()))
}

fn corrupt(message: impl Into<String>) -> Error {
    Error::invalid_ppt_structure(format!("ppt model: {}", message.into()))
}

fn w_span(out: &mut Vec<u8>, s: (u64, u64)) {
    out.extend_from_slice(&s.0.to_le_bytes());
    out.extend_from_slice(&s.1.to_le_bytes());
}

fn w_units(out: &mut Vec<u8>, units: &[u16]) {
    out.extend_from_slice(&(units.len() as u32).to_le_bytes());
    for u in units {
        out.extend_from_slice(&u.to_le_bytes());
    }
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

    fn span(&mut self) -> Result<(u64, u64)> {
        Ok((self.u64()?, self.u64()?))
    }

    fn units(&mut self) -> Result<Vec<u16>> {
        let n = self.u32()?;
        if n as u64 > MAX_MODEL_TEXT_UNITS as u64 {
            return Err(corrupt("text length is implausible"));
        }
        let bytes = self.bytes(n as usize * 2)?;
        Ok(bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect())
    }

    fn at_end(&self) -> bool {
        self.at == self.b.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_and_kind_names_are_stable() {
        assert_eq!(record_type_name(REC_DOCUMENT), "document");
        assert_eq!(
            record_type_name(REC_SLIDE_LIST_WITH_TEXT),
            "slide-list-with-text"
        );
        assert_eq!(record_type_name(REC_TEXT_CHARS_ATOM), "text-chars-atom");
        assert_eq!(record_type_name(REC_TEXT_BYTES_ATOM), "text-bytes-atom");
        assert_eq!(run_kind_name(RUN_CHARS), "chars");
        assert_eq!(run_kind_name(RUN_BYTES), "bytes");
        assert_eq!(slide_list_name(SLIDE_LIST_SLIDES), "slides");
        assert_eq!(slide_list_name(SLIDE_LIST_MASTER), "masters");
        assert_eq!(slide_list_name(SLIDE_LIST_NOTES), "notes");
        assert_eq!(slide_list_name(SLIDE_LIST_HANDOUT), "handout");
        assert_eq!(slide_list_name(7), "other");
        assert_eq!(slide_list_from_name("masters"), Some(SLIDE_LIST_MASTER));
        assert_eq!(slide_list_from_name("handout"), Some(SLIDE_LIST_HANDOUT));
        assert_eq!(slide_list_from_name("nope"), None);
    }

    #[test]
    fn cp1252_maps_the_high_range() {
        assert_eq!(cp1252(0x41), 0x0041);
        assert_eq!(cp1252(0x80), 0x20AC); // euro sign
        assert_eq!(cp1252(0x99), 0x2122); // trade mark
        assert_eq!(cp1252(0x81), 0xFFFD); // undefined
    }

    #[test]
    fn prose_is_not_a_ppt() {
        assert!(!detect(
            b"The quick brown fox jumps over the lazy dog.\nPlain prose.\n",
            Limits::DEFAULT
        ));
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut seed: u64 = 0x2620_0003_5050_5456;
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
        let mut buf = vec![0u8; 4096];
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
