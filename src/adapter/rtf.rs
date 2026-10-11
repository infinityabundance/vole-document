//! Bounded, representation-preserving RTF (Rich Text Format) adapter (Phase 21.32).
//!
//! RTF is a **word-processor markup** Wave-2 format: a text, group-brace grammar with
//! control words, structurally closer to a bounded tree (like XML) than to the flat
//! line formats. Like every Wave-2 format it is *not* a package: there is no OPC/ZIP
//! layer, no `mimetype`, and no relationship graph. The exact leaf is therefore the
//! **whole source** (a `DocumentExact`, a RAW-like authority), and everything this
//! module produces is a bounded, deterministic (`Q_gen`) projection that never sits on
//! the exactness path.
//!
//! ## Why a bespoke, span-preserving parser
//!
//! A conventional RTF reader derives **rendered text** and drops the *representation*:
//! it discards group boundaries, control-word spelling, hex/Unicode escape spelling,
//! destination names, and every source offset. This adapter does the opposite. For
//! every construct it records the exact **byte span** in the source, so it preserves
//! and can report:
//!
//! * the **header** (`{\rtf1\ansi\deffN …}`) with the exact `\rtf` version control
//!   word and its numeric version, the charset control word (`\ansi`/`\mac`/`\pc`/
//!   `\pca`), and `\deffN`;
//! * every **group** `{ … }` with its exact brace span (`[start, end)` includes both
//!   braces), its parent, and its nesting depth (the outermost group is depth 1);
//! * every **control word** (`\word` with its optional numeric parameter, e.g. `\fs24`)
//!   and **control symbol** (`\\`, `\{`, `\}`, `\~`, `\-`, `\_`, `\*`, and any other
//!   single non-alphabetic character) with the exact spelling, the name span, the
//!   parameter span, and whether a terminating delimiter space followed;
//! * every **hex escape** `\'hh` (the `\'hh` spelling is preserved together with the
//!   decoded byte, which is decoded to a scalar under the declared charset for derived
//!   observations);
//! * every **Unicode escape** `\uN` together with the **`\ucN` fallback count** active
//!   when it appeared (the literal `\uN` form and the fallback bytes that follow it in
//!   the source are preserved — the fallback is **never** dropped);
//! * every **destination** — the `\*` ignore destination, `\fonttbl`, `\colortbl`,
//!   `\stylesheet`, `\info`, and the `\pict`/`\object` binary groups — preserved
//!   verbatim with its exact name span and the enclosing group;
//! * every **plain text run** between control words, preserved **byte-exactly** (RTF
//!   uses a space to terminate a control word, so no whitespace is ever normalized).
//!
//! ## Bounds and no panics
//!
//! The parser is a bounded single-pass scanner. Untrusted input can only ever yield a
//! typed decline, never a panic or an unbounded allocation: the group count by
//! [`Limits::max_rtf_groups`]; the group nesting depth by [`Limits::max_rtf_depth`];
//! the control word / symbol / hex count by [`Limits::max_rtf_control_words`] and
//! [`Limits::max_rtf_hex_escapes`]; the accumulated plain-text bytes by
//! [`Limits::max_rtf_text_bytes`]; and the source length by
//! [`Limits::max_rtf_document_bytes`]. An over-long control word name, an over-long
//! numeric parameter, a trailing backslash, an unbalanced brace, and any
//! non-padding bytes after the root group are all typed declines. (Trailing NUL
//! and ASCII-whitespace *padding* after the balanced root group is tolerated,
//! because real writers routinely append a final CRLF and Word emits a trailing
//! NUL; see `is_rtf_trailing_pad`.) There is no `unwrap`, `expect`,
//! `panic!`, or indexing that is not span-checked.
//!
//! ## Detection
//!
//! [`detect`] is deliberately conservative and *strong-magic based*: the source must
//! begin (after an optional UTF-8 BOM) with `{` `\` `r` `t` `f` followed by an ASCII
//! **digit** — the RTF header magic `{\rtf1` — **and** its group braces must be
//! balanced under a bounded scan that respects the `\\`/`\{`/`\}` control symbols, the
//! `\'hh` hex spelling, control words (with their optional numeric parameter), and
//! `\binN` raw-binary data. The outermost group must close exactly at end of input.
//! A plain text source, a leading-`{` blob, and any other format are therefore never
//! stolen; they stay
//! [`Opaque`](crate::field::document_format::DocumentFormat::Opaque).
//!
//! ## Explicitly declined (honest boundary)
//!
//! Detection is a **magic + brace-balance** test, not a full-grammar test: a byte
//! string that merely begins with `{\rtf<digit>` and has balanced braces is admitted
//! as RTF even if its control words are otherwise meaningless, and the derived model
//! may then decline typed. The adapter does not render (no final text layout), does not
//! resolve `\fonttbl`/`\stylesheet`/`\info` semantics, does not execute any control
//! word, and does not map the `\mac`/`\pc`/`\pca` code pages beyond reporting the raw
//! decoded byte (only the `\ansi` 0x80–0x9F window is remapped, to Windows-1252). None
//! of these is guessed at.

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model groups (defends the decoder against a hostile blob).
pub const MAX_MODEL_GROUPS: u32 = 1 << 22;
/// Hard cap on decoded model lexical elements (defends the decoder against a hostile blob).
pub const MAX_MODEL_ELEMENTS: u64 = 1 << 24;

/// The sentinel "no parent group" index.
pub const NO_GROUP: u32 = u32::MAX;

/// Charset: `\ansi` (Windows-1252).
pub const CS_ANSI: u8 = 0;
/// Charset: `\mac` (MacRoman).
pub const CS_MAC: u8 = 1;
/// Charset: `\pc` (IBM PC code page 437).
pub const CS_PC: u8 = 2;
/// Charset: `\pca` (IBM PC code page 850).
pub const CS_PCA: u8 = 3;
/// Charset: no declared charset control word (recorded honestly, never guessed).
pub const CS_UNKNOWN: u8 = 4;

/// Stable lower-case charset name for reports and JSON output.
pub const fn charset_name(c: u8) -> &'static str {
    match c {
        CS_ANSI => "ansi",
        CS_MAC => "mac",
        CS_PC => "pc",
        CS_PCA => "pca",
        _ => "unknown",
    }
}

/// Control kind: a control word (`\word` with an optional numeric parameter).
pub const C_WORD: u8 = 0;
/// Control kind: a control symbol (a single non-alphabetic character after `\`).
pub const C_SYMBOL: u8 = 1;

/// Stable lower-case control kind name for reports.
pub const fn control_kind_name(k: u8) -> &'static str {
    match k {
        C_WORD => "word",
        _ => "symbol",
    }
}

/// Destination kind: not a destination group.
pub const D_NONE: u8 = 0;
/// Destination kind: the font table (`\fonttbl`).
pub const D_FONT_TABLE: u8 = 1;
/// Destination kind: the color table (`\colortbl`).
pub const D_COLOR_TABLE: u8 = 2;
/// Destination kind: the stylesheet (`\stylesheet`).
pub const D_STYLESHEET: u8 = 3;
/// Destination kind: the document info group (`\info`).
pub const D_INFO: u8 = 4;
/// Destination kind: a picture group (`\pict`).
pub const D_PICT: u8 = 5;
/// Destination kind: an embedded object group (`\object`).
pub const D_OBJECT: u8 = 6;
/// Destination kind: an ignore destination (`\*`).
pub const D_IGNORE: u8 = 7;

/// Stable lower-case destination name for reports and JSON output.
pub const fn destination_name(d: u8) -> &'static str {
    match d {
        D_FONT_TABLE => "fonttbl",
        D_COLOR_TABLE => "colortbl",
        D_STYLESHEET => "stylesheet",
        D_INFO => "info",
        D_PICT => "pict",
        D_OBJECT => "object",
        D_IGNORE => "ignore",
        _ => "none",
    }
}

/// The destination kind for a control-word name, if it names a recorded destination.
pub fn destination_for_name(name: &[u8]) -> Option<u8> {
    Some(match name {
        b"fonttbl" => D_FONT_TABLE,
        b"colortbl" => D_COLOR_TABLE,
        b"stylesheet" => D_STYLESHEET,
        b"info" => D_INFO,
        b"pict" => D_PICT,
        b"object" => D_OBJECT,
        _ => return None,
    })
}

/// The destination kind for an ASCII destination name (as used by the `rtf-dest`
/// selector), including the synthetic `ignore` name for a `\*` ignore destination.
pub fn destination_kind_for_name(name: &str) -> Option<u8> {
    Some(match name {
        "fonttbl" => D_FONT_TABLE,
        "colortbl" => D_COLOR_TABLE,
        "stylesheet" => D_STYLESHEET,
        "info" => D_INFO,
        "pict" => D_PICT,
        "object" => D_OBJECT,
        "ignore" => D_IGNORE,
        _ => return None,
    })
}

/// Maximum alphabetic control-word name length admitted (RTF specifies 32; a longer
/// name is a typed decline rather than an unbounded scan).
const MAX_CONTROL_NAME: usize = 64;
/// Maximum numeric-parameter digit count admitted (including a leading sign).
const MAX_PARAM_DIGITS: usize = 12;
/// The RTF default `\ucN` fallback count before any `\ucN` is seen.
const DEFAULT_UC: u32 = 1;

/// The parsed RTF header (`{\rtf1\ansi\deffN …}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtfHeader {
    /// The `\rtf` control word span start (the backslash).
    pub rtf_start: u64,
    /// One past the `\rtf` control word's numeric version.
    pub rtf_end: u64,
    /// The `\rtf` version number.
    pub version: i32,
    /// The version digits span start.
    pub version_start: u64,
    /// One past the version digits.
    pub version_end: u64,
    /// One of the `CS_*` tags.
    pub charset: u8,
    /// The charset control word span start (the backslash), or `0`.
    pub charset_start: u64,
    /// One past the charset control word name, or `0`.
    pub charset_end: u64,
    /// Whether a `\deffN` control word was present.
    pub has_deff: bool,
    /// The `\deffN` value (`0` when absent).
    pub deff: i32,
    /// The `\deff` control word span start, or `0`.
    pub deff_start: u64,
    /// One past the `\deff` control word's numeric value, or `0`.
    pub deff_end: u64,
}

impl RtfHeader {
    /// The `\deffN` value, or `None` when no `\deff` control word was present.
    pub fn deff_value(&self) -> Option<i32> {
        if self.has_deff { Some(self.deff) } else { None }
    }
}

/// One parsed RTF group `{ … }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtfGroup {
    /// The opening brace's source offset.
    pub start: u64,
    /// One past the closing brace.
    pub end: u64,
    /// The enclosing group index ([`NO_GROUP`] for the outermost group).
    pub parent: u32,
    /// The nesting depth (the outermost group is depth 1).
    pub depth: u32,
    /// The destination kind (`D_*`), or [`D_NONE`].
    pub dest: u8,
}

/// One parsed RTF control word or control symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtfControl {
    /// The backslash's source offset.
    pub start: u64,
    /// One past the control word/symbol (excluding a terminating delimiter space).
    pub end: u64,
    /// The name span start (one past the backslash).
    pub name_start: u64,
    /// One past the alphabetic name (for a symbol, `name_start + 1`).
    pub name_end: u64,
    /// The numeric parameter span start (`== name_end` when absent).
    pub param_start: u64,
    /// One past the numeric parameter (`== name_end` when absent).
    pub param_end: u64,
    /// One of the `C_*` tags.
    pub kind: u8,
    /// Whether a numeric parameter was present.
    pub has_param: bool,
    /// Whether a terminating delimiter space was consumed.
    pub delimited: bool,
    /// The enclosing group index.
    pub group: u32,
}

/// One parsed RTF plain text run (a maximal run of non-`\`/`{`/`}` bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtfSpan {
    /// The run's first source byte.
    pub start: u64,
    /// One past the run's last source byte.
    pub end: u64,
    /// The enclosing group index.
    pub group: u32,
}

/// One parsed RTF hex escape `\'hh`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtfHex {
    /// The backslash's source offset.
    pub start: u64,
    /// One past the second hex digit.
    pub end: u64,
    /// The decoded byte.
    pub byte: u8,
    /// The enclosing group index.
    pub group: u32,
}

/// One parsed RTF Unicode escape `\uN`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtfUnicode {
    /// The backslash's source offset.
    pub start: u64,
    /// One past the numeric value.
    pub end: u64,
    /// The value as written (may be negative: RTF's signed 16-bit convention).
    pub code: i32,
    /// The `\ucN` fallback count active when the escape appeared.
    pub uc: u32,
    /// The enclosing group index.
    pub group: u32,
}

/// One parsed RTF destination (a group whose first token names a destination).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtfDestination {
    /// The destination kind (`D_*`).
    pub kind: u8,
    /// The enclosing group index.
    pub group: u32,
    /// The destination name span start (the backslash).
    pub name_start: u64,
    /// One past the destination name.
    pub name_end: u64,
}

/// The canonical derived RTF model (the materialization of an `RtfModel` node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtfModel {
    /// The parsed header.
    pub header: RtfHeader,
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// The group arena, in document order (index 0 is the outermost group).
    pub groups: Vec<RtfGroup>,
    /// The control word/symbol arena, in document order.
    pub controls: Vec<RtfControl>,
    /// The plain-text-run arena, in document order.
    pub text_runs: Vec<RtfSpan>,
    /// The hex-escape arena, in document order.
    pub hex: Vec<RtfHex>,
    /// The Unicode-escape arena, in document order.
    pub unicode: Vec<RtfUnicode>,
    /// The destination arena, in document order.
    pub destinations: Vec<RtfDestination>,
}

impl RtfModel {
    /// The group at `index`, if present.
    pub fn group(&self, index: u32) -> Option<&RtfGroup> {
        self.groups.get(index as usize)
    }

    /// The control at `index`, if present.
    pub fn control(&self, index: u32) -> Option<&RtfControl> {
        self.controls.get(index as usize)
    }

    /// The text run at `index`, if present.
    pub fn text_run(&self, index: u32) -> Option<&RtfSpan> {
        self.text_runs.get(index as usize)
    }

    /// The hex escape at `index`, if present.
    pub fn hex_escape(&self, index: u32) -> Option<&RtfHex> {
        self.hex.get(index as usize)
    }

    /// The Unicode escape at `index`, if present.
    pub fn unicode_escape(&self, index: u32) -> Option<&RtfUnicode> {
        self.unicode.get(index as usize)
    }

    /// The destination at `index`, if present.
    pub fn destination(&self, index: u32) -> Option<&RtfDestination> {
        self.destinations.get(index as usize)
    }

    /// The observed maximum group nesting depth (0 when there are no groups).
    pub fn max_depth(&self) -> u32 {
        self.groups.iter().map(|g| g.depth).max().unwrap_or(0)
    }

    /// Indices of destinations of `kind`, in document order.
    pub fn destinations_of_kind(&self, kind: u8) -> Vec<u32> {
        self.destinations
            .iter()
            .enumerate()
            .filter(|(_, d)| d.kind == kind)
            .map(|(i, _)| i as u32)
            .collect()
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(
            96 + self.groups.len() * 29
                + self.controls.len() * 55
                + self.text_runs.len() * 20
                + self.hex.len() * 25
                + self.unicode.len() * 28
                + self.destinations.len() * 21,
        );
        out.extend_from_slice(b"RTFM");
        out.push(MODEL_VERSION);
        let h = &self.header;
        out.extend_from_slice(&h.rtf_start.to_le_bytes());
        out.extend_from_slice(&h.rtf_end.to_le_bytes());
        out.extend_from_slice(&h.version.to_le_bytes());
        out.extend_from_slice(&h.version_start.to_le_bytes());
        out.extend_from_slice(&h.version_end.to_le_bytes());
        out.push(h.charset);
        out.extend_from_slice(&h.charset_start.to_le_bytes());
        out.extend_from_slice(&h.charset_end.to_le_bytes());
        out.push(u8::from(h.has_deff));
        out.extend_from_slice(&h.deff.to_le_bytes());
        out.extend_from_slice(&h.deff_start.to_le_bytes());
        out.extend_from_slice(&h.deff_end.to_le_bytes());
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&(self.groups.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.controls.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.text_runs.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.hex.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.unicode.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.destinations.len() as u32).to_le_bytes());
        for g in &self.groups {
            out.extend_from_slice(&g.start.to_le_bytes());
            out.extend_from_slice(&g.end.to_le_bytes());
            out.extend_from_slice(&g.parent.to_le_bytes());
            out.extend_from_slice(&g.depth.to_le_bytes());
            out.push(g.dest);
        }
        for c in &self.controls {
            out.push(c.kind);
            out.push(u8::from(c.has_param) | (u8::from(c.delimited) << 1));
            out.extend_from_slice(&c.start.to_le_bytes());
            out.extend_from_slice(&c.end.to_le_bytes());
            out.extend_from_slice(&c.name_start.to_le_bytes());
            out.extend_from_slice(&c.name_end.to_le_bytes());
            out.extend_from_slice(&c.param_start.to_le_bytes());
            out.extend_from_slice(&c.param_end.to_le_bytes());
            out.extend_from_slice(&c.group.to_le_bytes());
        }
        for t in &self.text_runs {
            out.extend_from_slice(&t.start.to_le_bytes());
            out.extend_from_slice(&t.end.to_le_bytes());
            out.extend_from_slice(&t.group.to_le_bytes());
        }
        for h in &self.hex {
            out.extend_from_slice(&h.start.to_le_bytes());
            out.extend_from_slice(&h.end.to_le_bytes());
            out.push(h.byte);
            out.extend_from_slice(&h.group.to_le_bytes());
        }
        for u in &self.unicode {
            out.extend_from_slice(&u.start.to_le_bytes());
            out.extend_from_slice(&u.end.to_le_bytes());
            out.extend_from_slice(&u.code.to_le_bytes());
            out.extend_from_slice(&u.uc.to_le_bytes());
            out.extend_from_slice(&u.group.to_le_bytes());
        }
        for d in &self.destinations {
            out.push(d.kind);
            out.extend_from_slice(&d.group.to_le_bytes());
            out.extend_from_slice(&d.name_start.to_le_bytes());
            out.extend_from_slice(&d.name_end.to_le_bytes());
        }
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<RtfModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"RTFM" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let header = RtfHeader {
            rtf_start: r.u64()?,
            rtf_end: r.u64()?,
            version: r.i32()?,
            version_start: r.u64()?,
            version_end: r.u64()?,
            charset: r.u8()?,
            charset_start: r.u64()?,
            charset_end: r.u64()?,
            has_deff: r.u8()? != 0,
            deff: r.i32()?,
            deff_start: r.u64()?,
            deff_end: r.u64()?,
        };
        if header.charset > CS_UNKNOWN {
            return Err(corrupt("unknown charset tag"));
        }
        let doc_len = r.u64()?;
        let gc = r.u32()?;
        let cc = r.u32()?;
        let tc = r.u32()?;
        let hc = r.u32()?;
        let uc = r.u32()?;
        let dc = r.u32()?;
        if gc > MAX_MODEL_GROUPS {
            return Err(corrupt("model group count is implausible"));
        }
        let total = gc as u64 + cc as u64 + tc as u64 + hc as u64 + uc as u64 + dc as u64;
        if total > MAX_MODEL_ELEMENTS {
            return Err(corrupt("model element count is implausible"));
        }
        let mut groups = Vec::with_capacity(gc as usize);
        for _ in 0..gc {
            let start = r.u64()?;
            let end = r.u64()?;
            let parent = r.u32()?;
            let depth = r.u32()?;
            let dest = r.u8()?;
            if start > end || end > doc_len || depth == 0 || depth > MAX_MODEL_GROUPS {
                return Err(corrupt("group span is outside the source"));
            }
            if parent != NO_GROUP && parent >= gc {
                return Err(corrupt("group parent is out of range"));
            }
            if dest > D_IGNORE {
                return Err(corrupt("unknown destination kind"));
            }
            groups.push(RtfGroup {
                start,
                end,
                parent,
                depth,
                dest,
            });
        }
        let mut controls = Vec::with_capacity(cc as usize);
        for _ in 0..cc {
            let kind = r.u8()?;
            if kind > C_SYMBOL {
                return Err(corrupt("unknown control kind"));
            }
            let flags = r.u8()?;
            if flags & !0b11 != 0 {
                return Err(corrupt("unknown control flags"));
            }
            let start = r.u64()?;
            let end = r.u64()?;
            let name_start = r.u64()?;
            let name_end = r.u64()?;
            let param_start = r.u64()?;
            let param_end = r.u64()?;
            let group = r.u32()?;
            if start > name_start
                || name_start > name_end
                || name_end > param_start
                || param_start > param_end
                || param_end > end
                || end > doc_len
            {
                return Err(corrupt("control span is inconsistent"));
            }
            if group >= gc {
                return Err(corrupt("control group is out of range"));
            }
            controls.push(RtfControl {
                start,
                end,
                name_start,
                name_end,
                param_start,
                param_end,
                kind,
                has_param: flags & 1 != 0,
                delimited: flags & 2 != 0,
                group,
            });
        }
        let mut text_runs = Vec::with_capacity(tc as usize);
        for _ in 0..tc {
            let start = r.u64()?;
            let end = r.u64()?;
            let group = r.u32()?;
            if start > end || end > doc_len || group >= gc {
                return Err(corrupt("text-run span is outside the source"));
            }
            text_runs.push(RtfSpan { start, end, group });
        }
        let mut hex = Vec::with_capacity(hc as usize);
        for _ in 0..hc {
            let start = r.u64()?;
            let end = r.u64()?;
            let byte = r.u8()?;
            let group = r.u32()?;
            if start > end || end > doc_len || group >= gc {
                return Err(corrupt("hex span is outside the source"));
            }
            hex.push(RtfHex {
                start,
                end,
                byte,
                group,
            });
        }
        let mut unicode = Vec::with_capacity(uc as usize);
        for _ in 0..uc {
            let start = r.u64()?;
            let end = r.u64()?;
            let code = r.i32()?;
            let uc_n = r.u32()?;
            let group = r.u32()?;
            if start > end || end > doc_len || group >= gc {
                return Err(corrupt("unicode span is outside the source"));
            }
            unicode.push(RtfUnicode {
                start,
                end,
                code,
                uc: uc_n,
                group,
            });
        }
        let mut destinations = Vec::with_capacity(dc as usize);
        for _ in 0..dc {
            let kind = r.u8()?;
            let group = r.u32()?;
            let name_start = r.u64()?;
            let name_end = r.u64()?;
            if kind == D_NONE || kind > D_IGNORE {
                return Err(corrupt("unknown destination kind"));
            }
            if group >= gc || name_start > name_end || name_end > doc_len {
                return Err(corrupt("destination span is outside the source"));
            }
            destinations.push(RtfDestination {
                kind,
                group,
                name_start,
                name_end,
            });
        }
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        Ok(RtfModel {
            header,
            doc_len,
            groups,
            controls,
            text_runs,
            hex,
            unicode,
            destinations,
        })
    }
}

/// Byte-based RTF detector. A strong-magic, brace-balanced test: the source must begin
/// (after an optional UTF-8 BOM) with `{\rtf<digit>` and its group braces must balance
/// under a bounded scan. See the module docs for the boundary it cannot cross.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.is_empty() || source.len() as u64 > limits.max_rtf_document_bytes {
        return false;
    }
    let bom = bom_len(source);
    if !magic_at(source, bom) {
        return false;
    }
    scan_balanced(source, bom, limits)
}

/// Whether the strong RTF header magic `{\rtf<digit>` is present at `at`.
fn magic_at(source: &[u8], at: usize) -> bool {
    if source.len() < at + 6 {
        return false;
    }
    &source[at..at + 5] == b"{\\rtf" && source[at + 5].is_ascii_digit()
}

/// Inter-document padding tolerated after the balanced root group: NUL and ASCII
/// whitespace only (`\0`, tab, LF, form feed, CR, space). Real writers append a
/// final CRLF, and Word emits a trailing NUL; nothing else may follow the root
/// group, so this remains a strong boundary and never admits loose prose.
fn is_rtf_trailing_pad(b: u8) -> bool {
    matches!(b, 0x00 | b'\t' | b'\n' | 0x0c | b'\r' | b' ')
}

/// An optional UTF-8 BOM length (RTF producers occasionally emit one).
fn bom_len(source: &[u8]) -> usize {
    if source.starts_with(&[0xEF, 0xBB, 0xBF]) {
        3
    } else {
        0
    }
}

/// A bounded brace-balance scan over an RTF source. It respects the `\\`/`\{`/`\}`
/// control symbols, the `\'hh` hex spelling, control words (with their optional
/// numeric parameter), and `\binN` raw-binary data, and requires the outermost group to
/// close exactly at end of input.
fn scan_balanced(source: &[u8], bom: usize, limits: Limits) -> bool {
    let len = source.len();
    let mut i = bom;
    let mut depth: u32 = 0;
    while i < len {
        match source[i] {
            b'{' => {
                depth += 1;
                if depth > limits.max_rtf_depth {
                    return false;
                }
                i += 1;
            }
            b'}' => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
                i += 1;
                if depth == 0 {
                    // Tolerate the trailing NUL/ASCII-whitespace padding that real
                    // writers append after the root group; anything else declines.
                    while i < len && is_rtf_trailing_pad(source[i]) {
                        i += 1;
                    }
                    if i != len {
                        return false;
                    }
                }
            }
            b'\\' => {
                if i + 1 >= len {
                    return false;
                }
                let n = source[i + 1];
                if n == b'\'' {
                    if i + 3 >= len
                        || hexval(source[i + 2]).is_none()
                        || hexval(source[i + 3]).is_none()
                    {
                        return false;
                    }
                    i += 4;
                } else if n.is_ascii_alphabetic() {
                    let mut j = i + 1;
                    while j < len && source[j].is_ascii_alphabetic() {
                        j += 1;
                    }
                    let name = &source[i + 1..j];
                    let mut k = j;
                    if k < len && source[k] == b'-' {
                        k += 1;
                    }
                    let dstart = k;
                    while k < len && source[k].is_ascii_digit() {
                        k += 1;
                    }
                    let has_digits = k > dstart;
                    if name == b"bin" && has_digits {
                        let n = parse_uint(&source[dstart..k]);
                        let mut m = k;
                        if m < len && source[m] == b' ' {
                            m += 1;
                        }
                        let n = match n.and_then(|v| usize::try_from(v).ok()) {
                            Some(v) => v,
                            None => return false,
                        };
                        if n > len - m {
                            return false;
                        }
                        i = m + n;
                    } else {
                        i = if has_digits { k } else { j };
                        if i < len && source[i] == b' ' {
                            i += 1;
                        }
                    }
                } else {
                    i += 2;
                }
            }
            _ => i += 1,
        }
    }
    depth == 0
}

/// Parse `source` into an [`RtfModel`] under `limits`. Declines typed on any malformed
/// or over-large input; never panics.
pub fn parse(source: &[u8], limits: Limits) -> Result<RtfModel> {
    if source.len() as u64 > limits.max_rtf_document_bytes {
        return Err(Error::resource_limit(format!(
            "RTF source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_rtf_document_bytes
        )));
    }
    let bom = bom_len(source);
    if !magic_at(source, bom) {
        return Err(invalid_rtf(
            "RTF source must begin with the header magic {\\rtf<digit>",
        ));
    }
    let mut p = Parser::new(source, bom, limits);
    p.run()?;
    let header = p
        .header
        .ok_or_else(|| invalid_rtf("RTF header (\\rtf control word) is missing"))?;
    Ok(RtfModel {
        header,
        doc_len: source.len() as u64,
        groups: p.groups,
        controls: p.controls,
        text_runs: p.text_runs,
        hex: p.hex,
        unicode: p.unicode,
        destinations: p.destinations,
    })
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of an `RtfModel` node).
pub fn build_rtf_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits)?.encode())
}

/// The exact source bytes of a group's whole span (`[start, end)`, braces included).
pub fn group_bytes<'a>(source: &'a [u8], g: &RtfGroup) -> Result<&'a [u8]> {
    slice(source, g.start, g.end, "group span is outside the source")
}

/// The exact source bytes of a group's content span (between the braces).
pub fn group_content_bytes<'a>(source: &'a [u8], g: &RtfGroup) -> Result<&'a [u8]> {
    let s = g.start.saturating_add(1);
    let e = g.end.saturating_sub(1);
    if e < s {
        return Ok(&[]);
    }
    slice(source, s, e, "group content span is outside the source")
}

/// The exact source bytes of a control word/symbol's whole span.
pub fn control_bytes<'a>(source: &'a [u8], c: &RtfControl) -> Result<&'a [u8]> {
    slice(source, c.start, c.end, "control span is outside the source")
}

/// The control word/symbol's name bytes (no backslash).
pub fn control_name_bytes<'a>(source: &'a [u8], c: &RtfControl) -> Result<&'a [u8]> {
    slice(
        source,
        c.name_start,
        c.name_end,
        "control name span is outside the source",
    )
}

/// The exact source bytes of a plain text run.
pub fn text_run_bytes<'a>(source: &'a [u8], t: &RtfSpan) -> Result<&'a [u8]> {
    slice(
        source,
        t.start,
        t.end,
        "text-run span is outside the source",
    )
}

/// The exact source bytes of a hex escape (`\'hh`).
pub fn hex_bytes<'a>(source: &'a [u8], h: &RtfHex) -> Result<&'a [u8]> {
    slice(source, h.start, h.end, "hex span is outside the source")
}

/// The exact source bytes of a Unicode escape (`\uN`).
pub fn unicode_bytes<'a>(source: &'a [u8], u: &RtfUnicode) -> Result<&'a [u8]> {
    slice(source, u.start, u.end, "unicode span is outside the source")
}

/// The exact source bytes of a destination's whole group span.
pub fn destination_bytes<'a>(
    source: &'a [u8],
    model: &RtfModel,
    d: &RtfDestination,
) -> Result<&'a [u8]> {
    let g = model
        .group(d.group)
        .ok_or_else(|| corrupt("destination group is out of range"))?;
    group_bytes(source, g)
}

/// The numeric parameter of a control word as an `i64`, or `None` when it has none or
/// the spelling is not a canonical integer. Never panics on a hostile span.
pub fn control_param(source: &[u8], c: &RtfControl) -> Option<i64> {
    if !c.has_param {
        return None;
    }
    let bytes = source.get(c.param_start as usize..c.param_end as usize)?;
    std::str::from_utf8(bytes).ok()?.parse::<i64>().ok()
}

/// Decode a `\'hh` byte to a scalar under the declared charset. Only the `\ansi`
/// 0x80–0x9F window is remapped (to Windows-1252); every other byte and charset is
/// reported as its Latin-1 scalar, and the raw byte is always available separately.
pub fn hex_char(charset: u8, byte: u8) -> char {
    if charset == CS_ANSI && (0x80..=0x9F).contains(&byte) {
        const CP1252: [char; 32] = [
            '\u{20AC}', '\u{81}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}',
            '\u{2021}', '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{8D}',
            '\u{017D}', '\u{8F}', '\u{90}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}',
            '\u{2022}', '\u{2013}', '\u{2014}', '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}',
            '\u{0153}', '\u{9D}', '\u{017E}', '\u{0178}',
        ];
        return CP1252[(byte - 0x80) as usize];
    }
    char::from(byte)
}

/// The Unicode scalar for a `\uN` value: RTF's signed 16-bit convention maps a negative
/// value `n` to `n + 65536`. Returns `None` for an unpaired surrogate or out of range.
pub fn unicode_char(code: i32) -> Option<char> {
    let scalar: u32 = if code < 0 {
        (code as i64 + 65536) as u32
    } else {
        code as u32
    };
    char::from_u32(scalar)
}

/// A deterministic canonical text projection: the exact source bytes, lossily
/// decoded. RTF is **not** rendered, so the canonical text is the source itself.
/// Declines typed if it would exceed `max_out`.
pub fn canonical_text(source: &[u8], _limits: Limits, max_out: u64) -> Result<String> {
    if source.len() as u64 > max_out {
        return Err(Error::resource_limit(format!(
            "RTF text projection exceeds the {max_out}-byte budget"
        )));
    }
    Ok(String::from_utf8_lossy(source).into_owned())
}

/// One lexical-match record from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtfMatch {
    /// The 0-based text-run index the match lies in.
    pub run: u32,
    /// The text run's enclosing group index.
    pub group: u32,
    /// The exact source span of the whole text run.
    pub start: u64,
    /// One past the text run's span.
    pub end: u64,
    /// The text run's exact content text.
    pub text: String,
}

/// A bounded, case-sensitive lexical search over each plain text run (never over
/// control words). Returns matches in document order. Declines typed if the match list
/// would exceed `max_out` bytes (an approximate bound).
pub fn find(source: &[u8], model: &RtfModel, pattern: &str, max_out: u64) -> Result<Vec<RtfMatch>> {
    if pattern.is_empty() {
        return Err(Error::usage("RTF find pattern must not be empty"));
    }
    let needle = pattern.as_bytes();
    let mut out: Vec<RtfMatch> = Vec::new();
    let mut estimated: u64 = 0;
    for (i, t) in model.text_runs.iter().enumerate() {
        let content = text_run_bytes(source, t)?;
        if contains(content, needle) {
            estimated = estimated.saturating_add(32 + content.len() as u64);
            if estimated > max_out {
                return Err(Error::resource_limit(format!(
                    "RTF find exceeded the {max_out}-byte budget"
                )));
            }
            out.push(RtfMatch {
                run: i as u32,
                group: t.group,
                start: t.start,
                end: t.end,
                text: String::from_utf8_lossy(content).into_owned(),
            });
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Frame {
    group: u32,
    fresh: bool,
    uc: u32,
}

struct Parser<'a> {
    b: &'a [u8],
    limits: Limits,
    at: usize,
    groups: Vec<RtfGroup>,
    controls: Vec<RtfControl>,
    text_runs: Vec<RtfSpan>,
    hex: Vec<RtfHex>,
    unicode: Vec<RtfUnicode>,
    destinations: Vec<RtfDestination>,
    stack: Vec<Frame>,
    text_bytes: u64,
    header: Option<RtfHeader>,
}

impl<'a> Parser<'a> {
    fn new(b: &'a [u8], bom: usize, limits: Limits) -> Self {
        Parser {
            b,
            limits,
            at: bom,
            groups: Vec::new(),
            controls: Vec::new(),
            text_runs: Vec::new(),
            hex: Vec::new(),
            unicode: Vec::new(),
            destinations: Vec::new(),
            stack: Vec::new(),
            text_bytes: 0,
            header: None,
        }
    }

    fn len(&self) -> usize {
        self.b.len()
    }

    fn cur_group(&self) -> Result<u32> {
        self.stack
            .last()
            .map(|f| f.group)
            .ok_or_else(|| invalid_rtf("token outside the root group"))
    }

    fn run(&mut self) -> Result<()> {
        while self.at < self.len() {
            match self.b[self.at] {
                b'{' => self.open_group()?,
                b'}' => self.close_group()?,
                b'\\' => self.control()?,
                _ => self.text_run()?,
            }
        }
        if !self.stack.is_empty() {
            return Err(invalid_rtf("unbalanced braces (unclosed group)"));
        }
        Ok(())
    }

    fn open_group(&mut self) -> Result<()> {
        let parent = self.stack.last().map(|f| f.group).unwrap_or(NO_GROUP);
        let depth = self.stack.len() as u32 + 1;
        if self.groups.len() as u32 >= self.limits.max_rtf_groups {
            return Err(Error::resource_limit(format!(
                "RTF group count exceeds the {} group cap",
                self.limits.max_rtf_groups
            )));
        }
        if depth > self.limits.max_rtf_depth {
            return Err(Error::resource_limit(format!(
                "RTF group nesting exceeds the {} level cap",
                self.limits.max_rtf_depth
            )));
        }
        let uc = self.stack.last().map(|f| f.uc).unwrap_or(DEFAULT_UC);
        let idx = self.groups.len() as u32;
        self.groups.push(RtfGroup {
            start: self.at as u64,
            end: 0,
            parent,
            depth,
            dest: D_NONE,
        });
        self.stack.push(Frame {
            group: idx,
            fresh: true,
            uc,
        });
        self.at += 1;
        Ok(())
    }

    fn close_group(&mut self) -> Result<()> {
        let frame = self
            .stack
            .pop()
            .ok_or_else(|| invalid_rtf("unbalanced '}' (no open group)"))?;
        let g = &mut self.groups[frame.group as usize];
        g.end = self.at as u64 + 1;
        self.at += 1;
        if self.stack.is_empty() {
            // Tolerate the trailing NUL/ASCII-whitespace padding that real writers
            // append after the root group; consume it so the run loop stops here.
            while self.at < self.len() && is_rtf_trailing_pad(self.b[self.at]) {
                self.at += 1;
            }
            if self.at != self.len() {
                return Err(invalid_rtf("trailing bytes after the RTF root group"));
            }
        }
        Ok(())
    }

    fn note_token(&mut self) {
        if let Some(f) = self.stack.last_mut() {
            f.fresh = false;
        }
    }

    fn push_text(&mut self, start: usize, end: usize) -> Result<()> {
        if end <= start {
            return Ok(());
        }
        let n = (end - start) as u64;
        self.text_bytes = self.text_bytes.saturating_add(n);
        if self.text_bytes > self.limits.max_rtf_text_bytes {
            return Err(Error::resource_limit(format!(
                "RTF text bytes exceed the {} byte cap",
                self.limits.max_rtf_text_bytes
            )));
        }
        let group = self.cur_group()?;
        self.text_runs.push(RtfSpan {
            start: start as u64,
            end: end as u64,
            group,
        });
        Ok(())
    }

    fn text_run(&mut self) -> Result<()> {
        let start = self.at;
        let mut j = start;
        while j < self.len() && !matches!(self.b[j], b'{' | b'}' | b'\\') {
            j += 1;
        }
        self.note_token();
        self.push_text(start, j)?;
        self.at = j;
        Ok(())
    }

    /// Record a group's destination from a fresh-group first token.
    fn set_destination(&mut self, kind: u8, name_start: usize, name_end: usize) -> Result<()> {
        let group = self.cur_group()?;
        let g = &mut self.groups[group as usize];
        g.dest = kind;
        self.destinations.push(RtfDestination {
            kind,
            group,
            name_start: name_start as u64,
            name_end: name_end as u64,
        });
        if let Some(f) = self.stack.last_mut() {
            f.fresh = false;
        }
        Ok(())
    }

    fn control(&mut self) -> Result<()> {
        let start = self.at;
        if start + 1 >= self.len() {
            return Err(invalid_rtf("trailing backslash at end of input"));
        }
        let n = self.b[start + 1];
        if n == b'\'' {
            return self.hex_escape(start);
        }
        if !n.is_ascii_alphabetic() {
            return self.control_symbol(start, n);
        }
        // A control word.
        let name_start = start + 1;
        let mut j = name_start;
        while j < self.len() && self.b[j].is_ascii_alphabetic() {
            j += 1;
        }
        if j - name_start > MAX_CONTROL_NAME {
            return Err(invalid_rtf("control word name is too long"));
        }
        let mut k = j;
        let mut has_param = false;
        let mut param_start = j;
        if k < self.len() && (self.b[k] == b'-' || self.b[k].is_ascii_digit()) {
            let pstart = k;
            if self.b[k] == b'-' {
                k += 1;
            }
            let dstart = k;
            while k < self.len() && self.b[k].is_ascii_digit() {
                k += 1;
            }
            if k > dstart {
                if k - pstart > MAX_PARAM_DIGITS {
                    return Err(invalid_rtf("control word parameter is too long"));
                }
                has_param = true;
                param_start = pstart;
            } else {
                k = j;
            }
        }
        if self.controls.len() as u64 >= self.limits.max_rtf_control_words {
            return Err(Error::resource_limit(format!(
                "RTF control-word count exceeds the {} cap",
                self.limits.max_rtf_control_words
            )));
        }
        let mut end = if has_param { k } else { j };
        let mut delimited = false;
        if end < self.len() && self.b[end] == b' ' {
            delimited = true;
            end += 1;
        }
        let group = self.cur_group()?;
        let name = self.b[name_start..j].to_vec();
        self.controls.push(RtfControl {
            start: start as u64,
            end: end as u64,
            name_start: name_start as u64,
            name_end: j as u64,
            param_start: param_start as u64,
            param_end: (if has_param { k } else { j }) as u64,
            kind: C_WORD,
            has_param,
            delimited,
            group,
        });
        self.at = end;

        // Header capture (root group only).
        if self.stack.len() == 1 {
            self.capture_header(&name, has_param, j, k, start)?;
        }
        // Destination classification (fresh group's first token).
        if self.stack.last().map(|f| f.fresh).unwrap_or(false) {
            if let Some(d) = destination_for_name(&name) {
                self.set_destination(d, start, j)?;
            } else {
                self.note_token();
            }
        } else {
            self.note_token();
        }
        // \ucN sets the current fallback count; \uN records a Unicode escape.
        if has_param {
            let value = param_value(&self.b[param_start..k]);
            if name == b"uc" {
                if let Some(f) = self.stack.last_mut() {
                    f.uc = value.max(0) as u32;
                }
            } else if name == b"u" {
                let uc = self.stack.last().map(|f| f.uc).unwrap_or(DEFAULT_UC);
                self.unicode.push(RtfUnicode {
                    start: start as u64,
                    end: (if has_param { k } else { j }) as u64,
                    code: value as i32,
                    uc,
                    group,
                });
            } else if name == b"bin" {
                if value < 0 {
                    return Err(invalid_rtf("\\bin has a negative byte count"));
                }
                let nb =
                    usize::try_from(value).map_err(|_| invalid_rtf("\\bin count is too large"))?;
                if nb > self.len() - self.at {
                    return Err(invalid_rtf("\\bin data overruns the source"));
                }
                let bstart = self.at;
                let bend = self.at + nb;
                self.push_text(bstart, bend)?;
                self.at = bend;
            }
        }
        Ok(())
    }

    fn capture_header(
        &mut self,
        name: &[u8],
        has_param: bool,
        name_end: usize,
        param_end: usize,
        start: usize,
    ) -> Result<()> {
        let mut h = self.header.take().unwrap_or(RtfHeader {
            rtf_start: 0,
            rtf_end: 0,
            version: 0,
            version_start: 0,
            version_end: 0,
            charset: CS_UNKNOWN,
            charset_start: 0,
            charset_end: 0,
            has_deff: false,
            deff: 0,
            deff_start: 0,
            deff_end: 0,
        });
        match name {
            b"rtf" => {
                if !has_param {
                    return Err(invalid_rtf("\\rtf has no version parameter"));
                }
                h.rtf_start = start as u64;
                h.rtf_end = param_end as u64;
                h.version = param_value(&self.b[name_end..param_end]) as i32;
                h.version_start = name_end as u64;
                h.version_end = param_end as u64;
            }
            b"ansi" | b"mac" | b"pc" | b"pca" => {
                h.charset = match name {
                    b"ansi" => CS_ANSI,
                    b"mac" => CS_MAC,
                    b"pc" => CS_PC,
                    _ => CS_PCA,
                };
                h.charset_start = start as u64;
                h.charset_end = name_end as u64;
            }
            b"deff" if has_param => {
                h.has_deff = true;
                h.deff = param_value(&self.b[name_end..param_end]) as i32;
                h.deff_start = start as u64;
                h.deff_end = param_end as u64;
            }
            _ => {}
        }
        self.header = Some(h);
        Ok(())
    }

    fn hex_escape(&mut self, start: usize) -> Result<()> {
        if start + 3 >= self.len() {
            return Err(invalid_rtf("truncated hex escape"));
        }
        let hi = hexval(self.b[start + 2]).ok_or_else(|| invalid_rtf("bad hex escape"))?;
        let lo = hexval(self.b[start + 3]).ok_or_else(|| invalid_rtf("bad hex escape"))?;
        if self.hex.len() as u64 >= self.limits.max_rtf_hex_escapes {
            return Err(Error::resource_limit(format!(
                "RTF hex-escape count exceeds the {} cap",
                self.limits.max_rtf_hex_escapes
            )));
        }
        let group = self.cur_group()?;
        self.note_token();
        self.hex.push(RtfHex {
            start: start as u64,
            end: (start + 4) as u64,
            byte: (hi << 4) | lo,
            group,
        });
        self.at = start + 4;
        Ok(())
    }

    fn control_symbol(&mut self, start: usize, n: u8) -> Result<()> {
        if self.controls.len() as u64 >= self.limits.max_rtf_control_words {
            return Err(Error::resource_limit(format!(
                "RTF control-word count exceeds the {} cap",
                self.limits.max_rtf_control_words
            )));
        }
        let group = self.cur_group()?;
        self.controls.push(RtfControl {
            start: start as u64,
            end: (start + 2) as u64,
            name_start: (start + 1) as u64,
            name_end: (start + 2) as u64,
            param_start: (start + 2) as u64,
            param_end: (start + 2) as u64,
            kind: C_SYMBOL,
            has_param: false,
            delimited: false,
            group,
        });
        if n == b'*' && self.stack.last().map(|f| f.fresh).unwrap_or(false) {
            self.set_destination(D_IGNORE, start, start + 2)?;
        } else {
            self.note_token();
        }
        self.at = start + 2;
        Ok(())
    }
}

/// Parse an unsigned integer from ASCII digits with saturation to `u64::MAX`.
fn parse_uint(digits: &[u8]) -> Option<u64> {
    if digits.is_empty() {
        return None;
    }
    let mut v: u64 = 0;
    for &d in digits {
        if !d.is_ascii_digit() {
            return None;
        }
        v = v.saturating_mul(10).saturating_add((d - b'0') as u64);
    }
    Some(v)
}

/// Parse a signed numeric parameter (`-?digits`); `0` when empty.
fn param_value(bytes: &[u8]) -> i64 {
    let (neg, digits) = match bytes.first() {
        Some(b'-') => (true, &bytes[1..]),
        _ => (false, bytes),
    };
    let mut v: i64 = 0;
    for &d in digits {
        if !d.is_ascii_digit() {
            break;
        }
        v = v.saturating_mul(10).saturating_add((d - b'0') as i64);
    }
    if neg { -v } else { v }
}

fn hexval(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn slice<'a>(source: &'a [u8], s: u64, e: u64, msg: &str) -> Result<&'a [u8]> {
    let s = usize::try_from(s).map_err(|_| corrupt("span overflow"))?;
    let e = usize::try_from(e).map_err(|_| corrupt("span overflow"))?;
    source.get(s..e).ok_or_else(|| corrupt(msg))
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || needle.len() > hay.len() {
        return false;
    }
    hay.windows(needle.len()).any(|w| w == needle)
}

fn corrupt(msg: &str) -> Error {
    Error::invalid_model(format!("corrupt RTF model: {msg}"))
}

fn invalid_rtf(msg: &str) -> Error {
    Error::invalid_rtf_structure(msg)
}

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

    fn u32(&mut self) -> Result<u32> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn i32(&mut self) -> Result<i32> {
        let b = self.bytes(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
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

    fn model(src: &[u8]) -> RtfModel {
        parse(src, Limits::DEFAULT).unwrap()
    }

    #[test]
    fn detects_magic_and_rejects_junk() {
        assert!(detect(b"{\\rtf1\\ansi Hello}", Limits::DEFAULT));
        // BOM tolerated.
        assert!(detect(b"\xEF\xBB\xBF{\\rtf1\\ansi Hello}", Limits::DEFAULT));
        // A plain text/`{`-junk source is never claimed.
        assert!(!detect(b"{ not rtf }", Limits::DEFAULT));
        assert!(!detect(b"{\\rtfX}", Limits::DEFAULT));
        assert!(!detect(b"just prose", Limits::DEFAULT));
        assert!(!detect(b"", Limits::DEFAULT));
        // Balanced-brace requirement: the outermost group must close at EOF.
        assert!(!detect(b"{\\rtf1 x} trailing", Limits::DEFAULT));
        assert!(!detect(b"{\\rtf1 { unbalanced", Limits::DEFAULT));
        // Trailing NUL / ASCII-whitespace padding after the root group is tolerated
        // (real writers append a final CRLF; Word appends a trailing NUL), but only
        // padding: any other trailing byte still declines.
        assert!(detect(b"{\\rtf1\\ansi Hello}\r\n", Limits::DEFAULT));
        assert!(detect(b"{\\rtf1\\ansi Hello}\n", Limits::DEFAULT));
        assert!(detect(b"{\\rtf1\\ansi Hello}\0", Limits::DEFAULT));
        assert!(!detect(b"{\\rtf1 x}\r\nmore", Limits::DEFAULT));
        assert!(!detect(b"{\\rtf1 x}\0{not a group}", Limits::DEFAULT));
        // Escaped braces do not count toward balance.
        assert!(detect(b"{\\rtf1 \\{ \\} ok}", Limits::DEFAULT));
    }

    #[test]
    fn trailing_padding_parses_and_keeps_the_root_group_span() {
        // A real-world RTF with a trailing CRLF and a trailing NUL still parses, and
        // the root group span ends at its closing brace (the padding is not part of
        // any group).
        let src = b"{\\rtf1\\ansi Hi}\r\n\0";
        let m = model(src);
        assert_eq!(m.groups.len(), 1);
        assert_eq!(m.groups[0].start, 0);
        assert_eq!(m.groups[0].end as usize, src.len() - 3);
        assert_eq!(m.doc_len as usize, src.len());
    }

    #[test]
    fn header_and_nesting_are_exact() {
        let src = b"{\\rtf1\\ansi\\deff2{\\fonttbl{\\f0 Arial;}}Hello}";
        let m = model(src);
        assert_eq!(m.header.version, 1);
        assert_eq!(m.header.charset, CS_ANSI);
        assert_eq!(m.header.deff_value(), Some(2));
        assert_eq!(
            &src[m.header.rtf_start as usize..m.header.rtf_end as usize],
            b"\\rtf1"
        );
        assert_eq!(m.groups.len(), 3);
        assert_eq!(m.groups[0].depth, 1);
        assert_eq!(m.groups[0].dest, D_NONE);
        assert_eq!(m.groups[1].depth, 2);
        assert_eq!(m.groups[2].depth, 3);
        assert_eq!(m.groups[2].parent, 1);
        assert_eq!(m.groups[1].dest, D_FONT_TABLE);
    }

    #[test]
    fn control_words_symbols_and_text_roundtrip() {
        let src = b"{\\rtf1\\ansi Hello\\b\\fs24 world\\par \\{x\\} \\\\ \\~ \\- \\_ }";
        let m = model(src);
        // There is a text run for the exact plain-text bytes (the control-word
        // delimiter spaces are consumed, never folded into text).
        let runs: Vec<&[u8]> = m
            .text_runs
            .iter()
            .map(|t| text_run_bytes(src, t).unwrap())
            .collect();
        assert!(runs.contains(&&b"Hello"[..]), "{runs:?}");
        assert!(runs.contains(&&b"world"[..]), "{runs:?}");
        assert!(runs.contains(&&b"x"[..]), "{runs:?}");
        // \fs24 keeps its numeric parameter span.
        let fs = m
            .controls
            .iter()
            .find(|c| control_name_bytes(src, c).unwrap() == b"fs")
            .unwrap();
        assert!(fs.has_param);
        assert_eq!(&src[fs.param_start as usize..fs.param_end as usize], b"24");
        // Control symbols are recorded with their exact spelling.
        let symbols: Vec<&[u8]> = m
            .controls
            .iter()
            .filter(|c| c.kind == C_SYMBOL)
            .map(|c| control_name_bytes(src, c).unwrap())
            .collect();
        for want in [
            &b"{"[..],
            &b"}"[..],
            &b"\\"[..],
            &b"~"[..],
            &b"-"[..],
            &b"_"[..],
        ] {
            assert!(symbols.contains(&want), "missing symbol {want:?}");
        }
    }

    #[test]
    fn hex_and_unicode_preserve_spelling_and_fallback() {
        let src = b"{\\rtf1\\ansi caf\\'e9 \\u233?\\uc0 \\u8212\\'97}";
        let m = model(src);
        assert_eq!(m.hex.len(), 2);
        assert_eq!(m.hex[0].byte, 0xE9);
        assert_eq!(hex_bytes(src, &m.hex[0]).unwrap(), b"\\'e9");
        assert_eq!(hex_char(CS_ANSI, 0xE9), 'é');
        assert_eq!(m.unicode.len(), 2);
        assert_eq!(m.unicode[0].code, 233);
        assert_eq!(m.unicode[0].uc, 1);
        assert_eq!(unicode_char(m.unicode[0].code), Some('é'));
        assert_eq!(m.unicode[1].code, 8212);
        assert_eq!(m.unicode[1].uc, 0);
        assert_eq!(unicode_bytes(src, &m.unicode[0]).unwrap(), b"\\u233");
        // The fallback text is preserved verbatim (never dropped).
        let all: Vec<u8> = m
            .text_runs
            .iter()
            .flat_map(|t| text_run_bytes(src, t).unwrap().to_vec())
            .collect();
        let all = String::from_utf8_lossy(&all);
        assert!(all.contains('?'), "fallback '?' dropped: {all:?}");
    }

    #[test]
    fn destinations_are_recorded_verbatim() {
        let src = b"{\\rtf1\\ansi{\\colortbl;\\red0;}{\\stylesheet{\\s0 x;}}{\\info{\\title T}}{\\pict\\pngblip \\'89PNG}{\\*\\generator X}Body}";
        let m = model(src);
        let kinds: Vec<u8> = m.destinations.iter().map(|d| d.kind).collect();
        for want in [
            D_FONT_TABLE,
            D_COLOR_TABLE,
            D_STYLESHEET,
            D_INFO,
            D_PICT,
            D_IGNORE,
        ] {
            if want == D_FONT_TABLE {
                continue; // not present in this fixture
            }
            assert!(kinds.contains(&want), "missing destination {want}");
        }
        let pict = m.destinations.iter().find(|d| d.kind == D_PICT).unwrap();
        let bytes = destination_bytes(src, &m, pict).unwrap();
        assert!(bytes.starts_with(b"{\\pict"), "{bytes:?}");
        assert!(bytes.ends_with(b"}"));
    }

    #[test]
    fn model_roundtrips_and_fails_closed() {
        let src = b"{\\rtf1\\ansi{\\fonttbl{\\f0 A;}}Hi\\b there\\par}";
        let m = model(src);
        let bytes = m.encode();
        let back = RtfModel::decode(&bytes).unwrap();
        assert_eq!(m, back);
        // Corrupt magic / truncation fail closed.
        assert!(RtfModel::decode(b"XXXX").is_err());
        assert!(RtfModel::decode(&bytes[..bytes.len() - 1]).is_err());
    }

    #[test]
    fn declines_are_typed() {
        assert!(matches!(
            parse(b"{\\rtf1\\ansi x", Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::error::ErrorClass::InvalidRtfStructure
        ));
        // A depth bomb declines typed under STRICT.
        let mut deep = Vec::new();
        deep.extend_from_slice(b"{\\rtf1\\ansi");
        deep.extend_from_slice(&[b'{'; 100]);
        deep.extend_from_slice(b"x}");
        assert!(parse(&deep, Limits::STRICT).is_err());
        // An over-long control word declines typed.
        let mut long = Vec::new();
        long.extend_from_slice(b"{\\rtf1\\ansi\\");
        long.extend_from_slice(&[b'a'; 200]);
        long.extend_from_slice(b" x}");
        assert!(parse(&long, Limits::DEFAULT).is_err());
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut x: u64 = 0x1234_5678_9ABC_DEF0;
        for _ in 0..256 {
            let mut buf = vec![0u8; 256];
            for b in buf.iter_mut() {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                *b = (x & 0xFF) as u8;
            }
            let _ = detect(&buf, Limits::DEFAULT);
            if let Ok(m) = parse(&buf, Limits::STRICT) {
                let _ = m.encode();
                let _ = find(&buf, &m, "a", 1 << 20);
            }
            let _ = RtfModel::decode(&buf);
        }
    }
}
