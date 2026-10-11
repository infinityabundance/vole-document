//! Bounded, representation-preserving **bibliography** adapter (Phase 21.31).
//!
//! The bibliography family is one adapter covering three dialects — **BibTeX**
//! (`.bib`), **RIS**, and **CSL-JSON** — and records the dialect in the model,
//! exactly as the CSV/config/feed/pkgmeta/apispec adapters record theirs. Like every
//! other Wave-2 format it is *not* a package: there is no OPC/ZIP layer, so the exact
//! leaf is the **whole source** (a `DocumentExact`, a RAW-like authority) and
//! everything this module produces is a bounded, deterministic (`Q_gen`) projection
//! that never sits on the exactness path.
//!
//! ## Dialects
//!
//! * **BibTeX** ([`D_BIBTEX`]) — a sequence of `@type{…}` entries. Three entry
//!   shapes are recognized: a **normal** entry `@type{key, field = value, …}`
//!   (retaining the entry key, the entry type *as written*, the field order, the
//!   field names *as written*, and every value's exact spelling and delimiters),
//!   `@string{name = value, …}`, `@preamble{value}` (with `#` concatenation), and
//!   `@comment{…}` (raw brace-balanced content). Four value forms are distinguished
//!   by [`BibField::value_kind`]: a brace-balanced `{…}`, a double-quoted `"…"`, a
//!   bare number, and a bare macro identifier; a `#`-concatenation is a single value
//!   span whose kind is the first segment's and which carries [`FF_CONCAT`].
//!   **Duplicate fields are preserved and reported** (the later occurrence carries
//!   [`FF_DUPLICATE`]); nothing is collapsed. Whitespace and `%` comments **between**
//!   entries are preserved verbatim because they are simply not inside any span.
//! * **RIS** ([`D_RIS`]) — `TY  - TYPE` … `ER  - ` entries. Every tagged line
//!   (`XX  - value`, the two-space separator preserved) is a field; a line that is
//!   not a tag line **continues** the previous value (the joined multi-line span,
//!   including the embedded newline, is retained and the field carries
//!   [`FF_CONTINUED`]). Entry order is preserved.
//! * **CSL-JSON** ([`D_CSL_JSON`]) — a JSON **array** of item objects (or a single
//!   item object), each carrying a string `id` and a string `type`. The physical
//!   bytes are JSON, so this dialect **reuses** the shared [`crate::adapter::json`]
//!   parser/arena (never a second JSON parser): member order, duplicate keys, numeric
//!   and string-escape spelling, key quoting, and every token's exact byte span carry
//!   the identical guarantees they carry in the base format.
//!
//! ## Detection (conservative; content-only; never a file name)
//!
//! The bibliography family has **no magic bytes** for its text dialects, so detection
//! is deliberately conservative and claims only on a strong signal:
//!
//! * **BibTeX** — the first non-whitespace byte (after an optional BOM and `%`
//!   comments) is `@`, a line-anchored `@type{` entry begins there, the whole source
//!   parses as one or more entries with only whitespace/`%` comments/`@comment`
//!   entries between them, and every entry is brace-balanced with a closing `}`.
//! * **RIS** — the first non-blank line is a `TY  - ` tag line, the source parses as
//!   one or more entries, **and every entry is terminated by an `ER  - ` line**.
//! * **CSL-JSON** — the source parses as exactly one JSON value whose root is an
//!   **array** whose every element is an object with a string `id` and a string
//!   `type`, **or** a single object with a string `id` and a string `type`.
//!
//! ### What the detector cannot distinguish (recorded honestly)
//!
//! * A **BibTeX entry type that is not `string`/`preamble`/`comment`** is treated as a
//!   normal entry; a `@type{key}` with no fields is a valid normal entry.
//! * A **JSON array of objects that each carry a string `id` and a string `type`**
//!   but are *not* bibliographic (for example an array of GeoJSON `Feature` objects,
//!   which RFC 7946 does not admit as a document anyway) is byte-for-byte
//!   indistinguishable from CSL-JSON and **is** claimed as `csl_json`. A *single*
//!   GeoJSON `Feature`/`FeatureCollection` object is claimed by the GeoJSON detector
//!   first.
//! * A plain JSON array, a generic JSON object, and prose are **not** claimed: they
//!   stay `Json`/`Opaque`. A JSON array of objects lacking a string `id` or a string
//!   `type` stays `Json`.
//! * A RIS **continuation line** that happens to begin with two uppercase
//!   alphanumerics and the `  - ` separator is read as a new tagged line (the
//!   inherent RIS ambiguity), so the field boundary there is the reading the grammar
//!   states.
//! * A BibTeX value that is a bare token is recorded as a `macro` or a `number`
//!   purely lexically; the adapter never resolves a macro against `@string`.
//!
//! ## Bounds and no panics
//!
//! Untrusted input can only ever yield a typed decline, never a panic or an unbounded
//! allocation: the entry/item count by [`Limits::max_bib_entries`], the field/member
//! count by [`Limits::max_bib_fields`], one value's byte length by
//! [`Limits::max_bib_value_bytes`], the reused JSON arena node count by
//! [`Limits::max_bib_nodes`], the structural recursion/brace nesting by
//! [`Limits::max_bib_depth`], and the source length by
//! [`Limits::max_bib_document_bytes`].

use crate::adapter::json::{self, JNode, JsonMatch, JsonModel, K_ARRAY, K_OBJECT, K_STRING};
use crate::error::{Error, ErrorClass, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded text entries (defends the decoder against a hostile blob).
pub const MAX_MODEL_ENTRIES: u32 = 1 << 24;
/// Hard cap on decoded text fields (defends the decoder against a hostile blob).
pub const MAX_MODEL_FIELDS: u64 = 1 << 26;
/// Hard cap on decoded CSL-JSON items (defends the decoder against a hostile blob).
pub const MAX_MODEL_ITEMS: u32 = 1 << 24;

/// Sentinel for an absent index (no field / no JSON node).
pub const NONE: u32 = u32::MAX;

/// Dialect: BibTeX (`.bib`).
pub const D_BIBTEX: u8 = 0;
/// Dialect: RIS.
pub const D_RIS: u8 = 1;
/// Dialect: CSL-JSON.
pub const D_CSL_JSON: u8 = 2;

/// The stable dialect name.
pub const fn dialect_name(d: u8) -> &'static str {
    match d {
        D_BIBTEX => "bibtex",
        D_RIS => "ris",
        D_CSL_JSON => "csl_json",
        _ => "unknown",
    }
}

/// Entry kind: a normal BibTeX entry (or an RIS entry).
pub const E_ENTRY: u8 = 0;
/// Entry kind: a BibTeX `@string` entry.
pub const E_STRING: u8 = 1;
/// Entry kind: a BibTeX `@preamble` entry.
pub const E_PREAMBLE: u8 = 2;
/// Entry kind: a BibTeX `@comment` entry.
pub const E_COMMENT: u8 = 3;

/// The stable entry-kind name.
pub const fn entry_kind_name(k: u8) -> &'static str {
    match k {
        E_ENTRY => "entry",
        E_STRING => "string",
        E_PREAMBLE => "preamble",
        E_COMMENT => "comment",
        _ => "unknown",
    }
}

/// Value kind: a brace-balanced `{…}` value.
pub const V_BRACED: u8 = 0;
/// Value kind: a double-quoted `"…"` value.
pub const V_QUOTED: u8 = 1;
/// Value kind: a bare numeric value.
pub const V_NUMBER: u8 = 2;
/// Value kind: a bare macro identifier.
pub const V_MACRO: u8 = 3;
/// Value kind: raw (an `@preamble`/`@comment` body, or an RIS value).
pub const V_RAW: u8 = 4;

/// The stable value-kind name.
pub const fn value_kind_name(k: u8) -> &'static str {
    match k {
        V_BRACED => "braced",
        V_QUOTED => "quoted",
        V_NUMBER => "number",
        V_MACRO => "macro",
        V_RAW => "raw",
        _ => "unknown",
    }
}

/// Field flag: the field name already appeared earlier in the same entry (duplicate
/// preserved and reported, never collapsed).
pub const FF_DUPLICATE: u8 = 1 << 0;
/// Field flag: a RIS value spanned more than one physical line (continuation).
pub const FF_CONTINUED: u8 = 1 << 1;
/// Field flag: a BibTeX value used `#` concatenation (the span covers every segment;
/// the kind is the first segment's).
pub const FF_CONCAT: u8 = 1 << 2;

/// The admissible field flags.
const FF_MASK: u8 = FF_DUPLICATE | FF_CONTINUED | FF_CONCAT;

/// Line terminator tag: no terminator (EOF).
pub const T_NONE: u8 = 0;
/// Line terminator tag: a bare `\n`.
pub const T_LF: u8 = 1;
/// Line terminator tag: a `\r\n` pair.
pub const T_CRLF: u8 = 2;

/// The stable line-terminator name.
pub const fn terminator_name(t: u8) -> &'static str {
    match t {
        T_LF => "lf",
        T_CRLF => "crlf",
        _ => "none",
    }
}

/// Match role: an entry type token (BibTeX) or a `TY` value (RIS).
pub const R_ENTRY_TYPE: u8 = 0;
/// Match role: a BibTeX entry key.
pub const R_ENTRY_KEY: u8 = 1;
/// Match role: a field name (BibTeX) or a tag (RIS).
pub const R_FIELD_NAME: u8 = 2;
/// Match role: a field value (BibTeX or RIS).
pub const R_FIELD_VALUE: u8 = 3;
/// Match role: a CSL-JSON object member key.
pub const R_MEMBER_KEY: u8 = 4;
/// Match role: a CSL-JSON object member string value.
pub const R_MEMBER_VALUE: u8 = 5;

/// The stable match-role name.
pub const fn role_name(r: u8) -> &'static str {
    match r {
        R_ENTRY_TYPE => "entry-type",
        R_ENTRY_KEY => "entry-key",
        R_FIELD_NAME => "field-name",
        R_FIELD_VALUE => "field-value",
        R_MEMBER_KEY => "member-key",
        _ => "member-value",
    }
}

/// One recorded bibliography entry (BibTeX/RIS) with exact spans.
///
/// `start..end` is the entry's exact content span (the closure `}` for BibTeX, the
/// `ER` line's content end for RIS); `type_start..type_end` the entry-type token
/// (BibTeX) or the `TY` value (RIS); `key_start..key_end` the BibTeX entry key (empty
/// for RIS and the special entries).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BibEntry {
    /// The entry kind (`E_*`).
    pub kind: u8,
    /// The terminator that followed the entry (`T_*`).
    pub terminator: u8,
    /// The entry's first source byte.
    pub start: u64,
    /// One past the entry's last content byte.
    pub end: u64,
    /// The entry-type token's first source byte.
    pub type_start: u64,
    /// One past the entry-type token.
    pub type_end: u64,
    /// The entry-key token's first source byte (`== key_end` when there is no key).
    pub key_start: u64,
    /// One past the entry-key token.
    pub key_end: u64,
    /// The entry's fields, in source order.
    pub fields: Vec<BibField>,
}

impl BibEntry {
    /// The field at `index` of this entry, if present.
    pub fn field(&self, index: u32) -> Option<&BibField> {
        self.fields.get(index as usize)
    }

    /// The number of fields that repeat an earlier field name in this entry.
    pub fn duplicate_field_count(&self) -> u64 {
        self.fields
            .iter()
            .filter(|f| f.flags & FF_DUPLICATE != 0)
            .count() as u64
    }
}

/// One recorded field (BibTeX `name = value`, or RIS `XX  - value`) with exact spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BibField {
    /// The value spelling kind (`V_*`).
    pub value_kind: u8,
    /// The field flags (`FF_*`).
    pub flags: u8,
    /// The field-name (or tag) token's first source byte (`== name_end` when unnamed).
    pub name_start: u64,
    /// One past the field-name token.
    pub name_end: u64,
    /// The value's first source byte (delimiters included).
    pub value_start: u64,
    /// One past the value.
    pub value_end: u64,
}

/// One recorded CSL-JSON item (an array element object, or the single root object).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BibItem {
    /// The item object node (arena index).
    pub node: u32,
    /// The `id` string value node (arena index), or [`NONE`].
    pub id_node: u32,
    /// The `type` string value node (arena index), or [`NONE`].
    pub type_node: u32,
    /// The item object's first source byte.
    pub start: u64,
    /// One past the item object.
    pub end: u64,
}

/// The canonical derived bibliography model (the materialization of a `BibModel`
/// node). For the text dialects it is the entry/field arena; for CSL-JSON it embeds
/// the reused JSON arena and the item projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BibModel {
    /// The recorded dialect (`D_*`).
    pub dialect: u8,
    /// The length in bytes of a leading UTF-8 BOM (`0` or `3`) for a text dialect.
    pub bom_len: u32,
    /// Whether a text dialect's source ended with a line terminator.
    pub trailing_newline: bool,
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// The text entries (BibTeX/RIS), in source order.
    pub entries: Vec<BibEntry>,
    /// The CSL-JSON items, in document order.
    pub items: Vec<BibItem>,
    /// The embedded representation-preserving JSON arena (CSL-JSON only).
    pub json: Option<JsonModel>,
}

impl BibModel {
    /// The recorded dialect name.
    pub fn dialect_name(&self) -> &'static str {
        dialect_name(self.dialect)
    }

    /// Whether this model is the CSL-JSON dialect.
    pub fn is_json(&self) -> bool {
        self.json.is_some()
    }

    /// The text entry at `index`, if present.
    pub fn entry(&self, index: u32) -> Option<&BibEntry> {
        self.entries.get(index as usize)
    }

    /// The CSL-JSON item at `index`, if present.
    pub fn item(&self, index: u32) -> Option<&BibItem> {
        self.items.get(index as usize)
    }

    /// The JSON node at `index`, if present.
    pub fn node(&self, index: u32) -> Option<&JNode> {
        self.json.as_ref().and_then(|j| j.node(index))
    }

    /// The number of recorded entries or items.
    pub fn record_count(&self) -> u64 {
        if self.is_json() {
            self.items.len() as u64
        } else {
            self.entries.len() as u64
        }
    }

    /// The total number of recorded fields/members across every record.
    pub fn field_count(&self) -> u64 {
        self.entries.iter().map(|e| e.fields.len() as u64).sum()
    }

    /// The exact source length the spans are relative to.
    pub fn doc_len(&self) -> u64 {
        self.doc_len
    }

    /// The number of arena nodes (CSL-JSON), else `0`.
    pub fn arena_len(&self) -> u32 {
        self.json.as_ref().map_or(0, |j| j.nodes.len() as u32)
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let fields: usize = self.entries.iter().map(|e| e.fields.len()).sum();
        let json_bytes = self.json.as_ref().map(|j| j.encode());
        let mut out =
            Vec::with_capacity(40 + self.entries.len() * 60 + fields * 40 + self.items.len() * 48);
        out.extend_from_slice(b"BIBM");
        out.push(MODEL_VERSION);
        out.push(self.dialect);
        let mut flags = 0u8;
        if self.trailing_newline {
            flags |= 1;
        }
        if self.json.is_some() {
            flags |= 2;
        }
        out.push(flags);
        out.push(0); // reserved
        out.extend_from_slice(&self.bom_len.to_le_bytes());
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.items.len() as u32).to_le_bytes());
        out.extend_from_slice(&(fields as u32).to_le_bytes());
        for e in &self.entries {
            out.push(e.kind);
            out.push(e.terminator);
            out.extend_from_slice(&[0, 0]); // reserved
            out.extend_from_slice(&e.start.to_le_bytes());
            out.extend_from_slice(&e.end.to_le_bytes());
            out.extend_from_slice(&e.type_start.to_le_bytes());
            out.extend_from_slice(&e.type_end.to_le_bytes());
            out.extend_from_slice(&e.key_start.to_le_bytes());
            out.extend_from_slice(&e.key_end.to_le_bytes());
            out.extend_from_slice(&(e.fields.len() as u32).to_le_bytes());
            for f in &e.fields {
                out.push(f.value_kind);
                out.push(f.flags);
                out.extend_from_slice(&[0, 0]); // reserved
                out.extend_from_slice(&f.name_start.to_le_bytes());
                out.extend_from_slice(&f.name_end.to_le_bytes());
                out.extend_from_slice(&f.value_start.to_le_bytes());
                out.extend_from_slice(&f.value_end.to_le_bytes());
            }
        }
        for it in &self.items {
            out.extend_from_slice(&it.node.to_le_bytes());
            out.extend_from_slice(&it.id_node.to_le_bytes());
            out.extend_from_slice(&it.type_node.to_le_bytes());
            out.extend_from_slice(&[0, 0, 0, 0]); // reserved
            out.extend_from_slice(&it.start.to_le_bytes());
            out.extend_from_slice(&it.end.to_le_bytes());
        }
        if let Some(jb) = json_bytes {
            out.extend_from_slice(&(jb.len() as u32).to_le_bytes());
            out.extend_from_slice(&jb);
        }
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<BibModel> {
        let mut r = Reader::new(bytes);
        if r.bytes(4)? != b"BIBM" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let dialect = r.u8()?;
        if dialect > D_CSL_JSON {
            return Err(corrupt("unknown dialect"));
        }
        let flags = r.u8()?;
        if flags & !3 != 0 {
            return Err(corrupt("unknown model flags"));
        }
        let _reserved = r.u8()?;
        let bom_len = r.u32()?;
        let doc_len = r.u64()?;
        if bom_len as u64 > doc_len {
            return Err(corrupt("BOM runs past the document"));
        }
        let entry_count = r.u32()?;
        let item_count = r.u32()?;
        let field_total = r.u32()?;
        if entry_count > MAX_MODEL_ENTRIES {
            return Err(corrupt("entry count is implausible"));
        }
        if item_count > MAX_MODEL_ITEMS {
            return Err(corrupt("item count is implausible"));
        }
        if field_total as u64 > MAX_MODEL_FIELDS {
            return Err(corrupt("field count is implausible"));
        }
        let mut entries = Vec::with_capacity(entry_count as usize);
        let mut seen_fields: u64 = 0;
        let mut prev_start: u64 = 0;
        for i in 0..entry_count {
            let kind = r.u8()?;
            if kind > E_COMMENT {
                return Err(corrupt("unknown entry kind"));
            }
            let terminator = r.u8()?;
            if terminator > T_CRLF {
                return Err(corrupt("unknown entry terminator"));
            }
            if r.bytes(2)? != [0, 0] {
                return Err(corrupt("unknown entry flags"));
            }
            let start = r.u64()?;
            let end = r.u64()?;
            let type_start = r.u64()?;
            let type_end = r.u64()?;
            let key_start = r.u64()?;
            let key_end = r.u64()?;
            let field_count = r.u32()?;
            if start > end || end > doc_len {
                return Err(corrupt("entry span is outside the document"));
            }
            if i > 0 && start < prev_start {
                return Err(corrupt("entry spans are not in source order"));
            }
            for (lo, hi) in [(type_start, type_end), (key_start, key_end)] {
                if lo > hi || hi > end || lo < start {
                    return Err(corrupt("entry token span is outside the entry"));
                }
            }
            if field_count as u64 > MAX_MODEL_FIELDS {
                return Err(corrupt("entry field count is implausible"));
            }
            let mut fields = Vec::with_capacity((field_count as usize).min(4096));
            for _ in 0..field_count {
                let value_kind = r.u8()?;
                if value_kind > V_RAW {
                    return Err(corrupt("unknown value kind"));
                }
                let fflags = r.u8()?;
                if fflags & !FF_MASK != 0 {
                    return Err(corrupt("unknown field flags"));
                }
                if r.bytes(2)? != [0, 0] {
                    return Err(corrupt("unknown field reserved bytes"));
                }
                let name_start = r.u64()?;
                let name_end = r.u64()?;
                let value_start = r.u64()?;
                let value_end = r.u64()?;
                if name_start > name_end || name_end > value_start || value_end > end {
                    return Err(corrupt("field spans are not ordered inside the entry"));
                }
                if value_start > value_end || value_start < start {
                    return Err(corrupt("field value span is outside the entry"));
                }
                fields.push(BibField {
                    value_kind,
                    flags: fflags,
                    name_start,
                    name_end,
                    value_start,
                    value_end,
                });
            }
            seen_fields += fields.len() as u64;
            if seen_fields > MAX_MODEL_FIELDS {
                return Err(corrupt("model field count is implausible"));
            }
            prev_start = start;
            entries.push(BibEntry {
                kind,
                terminator,
                start,
                end,
                type_start,
                type_end,
                key_start,
                key_end,
                fields,
            });
        }
        let mut items = Vec::with_capacity(item_count as usize);
        for _ in 0..item_count {
            let node = r.u32()?;
            let id_node = r.u32()?;
            let type_node = r.u32()?;
            if r.bytes(4)? != [0, 0, 0, 0] {
                return Err(corrupt("unknown item flags"));
            }
            let start = r.u64()?;
            let end = r.u64()?;
            if start > end || end > doc_len {
                return Err(corrupt("item span is outside the document"));
            }
            items.push(BibItem {
                node,
                id_node,
                type_node,
                start,
                end,
            });
        }
        let has_json = flags & 2 != 0;
        let json = if has_json {
            let json_len = r.u32()?;
            let json_bytes = r.bytes(json_len as usize)?;
            let j = JsonModel::decode(json_bytes)?;
            if j.doc_len != doc_len {
                return Err(corrupt("model doc_len disagrees with the JSON model"));
            }
            Some(j)
        } else {
            None
        };
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        if has_json {
            let j = json.as_ref().unwrap();
            let nnodes = j.nodes.len() as u32;
            if nnodes == 0 {
                return Err(corrupt("model has no arena"));
            }
            for it in &items {
                if it.node >= nnodes {
                    return Err(corrupt("item node is out of range"));
                }
                if it.id_node != NONE && it.id_node >= nnodes {
                    return Err(corrupt("item id node is out of range"));
                }
                if it.type_node != NONE && it.type_node >= nnodes {
                    return Err(corrupt("item type node is out of range"));
                }
            }
        } else if !items.is_empty() {
            return Err(corrupt("a text dialect has recorded items"));
        }
        Ok(BibModel {
            dialect,
            bom_len,
            trailing_newline: flags & 1 != 0,
            doc_len,
            entries,
            items,
            json,
        })
    }
}

/// A lexical match from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BibMatch {
    /// The 0-based entry (or item) index, or [`NONE`] for a CSL-JSON match.
    pub entry: u32,
    /// The 0-based field index within the entry, or [`NONE`].
    pub field: u32,
    /// The match role (`R_*`).
    pub role: u8,
    /// The exact source span of the matching bytes.
    pub start: u64,
    /// One past the matching bytes.
    pub end: u64,
    /// The canonical RFC 6901 pointer (CSL-JSON matches), else the empty string.
    pub pointer: String,
    /// The matched bytes, decoded lossily for reporting.
    pub text: String,
}

/// Byte-based, conservative bibliography detector. See the module docs for the exact
/// boundary.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.len() as u64 > limits.max_bib_document_bytes {
        return false;
    }
    parse(source, limits, false).is_ok()
}

/// Classify `source` as one bibliography dialect, or decline typed when it is not a
/// recognized document under the conservative test.
pub fn classify(source: &[u8], limits: Limits) -> Result<u8> {
    Ok(parse(source, limits, false)?.dialect)
}

/// Parse `source` into a [`BibModel`]. `build` selects whether the text field arena is
/// populated (detection runs with `build = false`). The CSL-JSON arena is always built
/// because the semantic test walks it.
pub fn parse(source: &[u8], limits: Limits, build: bool) -> Result<BibModel> {
    if source.len() as u64 > limits.max_bib_document_bytes {
        return Err(Error::resource_limit(format!(
            "bibliography source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_bib_document_bytes
        )));
    }
    let bom = bom_len(source);
    // BibTeX and RIS are line-anchored text dialects; CSL-JSON reuses the JSON parser.
    if first_significant(source, bom) == Some(b'@') {
        return parse_bibtex(source, bom, limits, build);
    }
    if ris_starts(source, bom) {
        return parse_ris(source, bom, limits, build);
    }
    if json::detect(source, limits) {
        return parse_csl(source, limits, build);
    }
    Err(invalid_bib(
        "source is not BibTeX, RIS, or CSL-JSON under the conservative test",
    ))
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of a `BibModel` node).
pub fn build_bib_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits, true)?.encode())
}

// ---------------------------------------------------------------------------
// Detection helpers
// ---------------------------------------------------------------------------

/// The length in bytes of a leading UTF-8 BOM (`0` or `3`).
fn bom_len(source: &[u8]) -> u32 {
    if source.starts_with(&[0xEF, 0xBB, 0xBF]) {
        3
    } else {
        0
    }
}

/// Skip ASCII whitespace and `%`-to-end-of-line comments, starting at `mut i`.
fn skip_inter(source: &[u8], mut i: usize) -> usize {
    loop {
        while i < source.len() && source[i].is_ascii_whitespace() {
            i += 1;
        }
        if i < source.len() && source[i] == b'%' {
            while i < source.len() && source[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        return i;
    }
}

/// The first significant byte (skipping a BOM, ASCII whitespace, and `%` comments).
fn first_significant(source: &[u8], bom: u32) -> Option<u8> {
    let i = skip_inter(source, bom as usize);
    source.get(i).copied()
}

/// Whether `source` begins (after a BOM) with a `TY  - ` RIS tag line.
fn ris_starts(source: &[u8], bom: u32) -> bool {
    let start = bom as usize;
    if source.len() < start + 6 {
        return false;
    }
    ris_tag(&source[start..]).is_some_and(|(tag, _)| tag == b"TY")
}

/// A RIS tag line: two uppercase-alphanumeric tag bytes then the `  - ` separator.
/// Returns the tag and the index one past the separator (relative to `content`).
fn ris_tag(content: &[u8]) -> Option<(&[u8], usize)> {
    if content.len() >= 6
        && is_ris_tag_byte(content[0])
        && is_ris_tag_byte(content[1])
        && content[2] == b' '
        && content[3] == b' '
        && content[4] == b'-'
        && content[5] == b' '
    {
        Some((&content[0..2], 6))
    } else {
        None
    }
}

fn is_ris_tag_byte(b: u8) -> bool {
    b.is_ascii_uppercase() || b.is_ascii_digit()
}

// ---------------------------------------------------------------------------
// Line iteration
// ---------------------------------------------------------------------------

/// One physical line.
#[derive(Debug, Clone, Copy)]
struct Line {
    /// First byte of the line.
    start: usize,
    /// One past the line's content (the terminator excluded).
    content_end: usize,
    /// The terminator tag (`T_*`).
    term: u8,
}

/// A bounded physical-line splitter over the whole source, starting at `at`.
struct LineIter<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> LineIter<'a> {
    fn new(b: &'a [u8], at: usize) -> Self {
        LineIter { b, at }
    }

    fn next_line(&mut self) -> Option<Line> {
        if self.at >= self.b.len() {
            return None;
        }
        let start = self.at;
        let mut i = self.at;
        while i < self.b.len() && self.b[i] != b'\n' {
            i += 1;
        }
        let (content_end, end, term) = if i < self.b.len() {
            let end = i + 1;
            if i > start && self.b[i - 1] == b'\r' {
                (i - 1, end, T_CRLF)
            } else {
                (i, end, T_LF)
            }
        } else {
            (self.b.len(), self.b.len(), T_NONE)
        };
        self.at = end;
        Some(Line {
            start,
            content_end,
            term,
        })
    }
}

fn is_blank(content: &[u8]) -> bool {
    content.iter().all(|b| b.is_ascii_whitespace())
}

// ---------------------------------------------------------------------------
// BibTeX parsing
// ---------------------------------------------------------------------------

/// The lexical character class of an entry-type token byte.
fn is_type_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b':'
}

/// The lexical character class of a field-name or macro-token byte.
fn is_name_byte(b: u8) -> bool {
    !b.is_ascii_whitespace()
        && !matches!(
            b,
            b',' | b'{' | b'}' | b'(' | b')' | b'=' | b'#' | b'"' | b'@' | b'%'
        )
}

/// Skip ASCII whitespace.
fn skip_ws(source: &[u8], mut i: usize) -> usize {
    while i < source.len() && source[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

/// Parse the whole source as a sequence of BibTeX entries.
fn parse_bibtex(source: &[u8], bom: u32, limits: Limits, build: bool) -> Result<BibModel> {
    let mut i = skip_inter(source, bom as usize);
    let mut entries: Vec<BibEntry> = Vec::new();
    let mut total_fields: u64 = 0;
    let mut max_depth: u32 = 1;
    while i < source.len() {
        if source[i] != b'@' {
            return Err(invalid_bib(format!(
                "expected `@` at the start of a BibTeX entry at byte {i}"
            )));
        }
        let entry_start = i;
        i += 1;
        let type_start = i;
        while i < source.len() && is_type_byte(source[i]) {
            i += 1;
        }
        if i == type_start {
            return Err(invalid_bib("BibTeX entry has an empty type name"));
        }
        let entry_type = &source[type_start..i];
        let type_end = i;
        i = skip_ws(source, i);
        if i >= source.len() || source[i] != b'{' {
            return Err(invalid_bib("BibTeX entry type must be followed by `{`"));
        }
        i += 1;
        if entries.len() as u64 >= limits.max_bib_entries as u64 {
            return Err(Error::resource_limit(format!(
                "bibliography exceeds the {}-entry cap",
                limits.max_bib_entries
            )));
        }
        let (entry, next, depth) = parse_bibtex_body(
            source,
            i,
            entry_start,
            entry_type,
            type_start,
            type_end,
            limits,
            build,
            &mut total_fields,
        )?;
        if depth > max_depth {
            max_depth = depth;
        }
        entries.push(entry);
        i = skip_inter(source, next);
    }
    if entries.is_empty() {
        return Err(invalid_bib("no BibTeX entries were found"));
    }
    if max_depth > limits.max_bib_depth {
        return Err(Error::resource_limit(format!(
            "bibliography brace nesting exceeds the {}-level cap",
            limits.max_bib_depth
        )));
    }
    Ok(BibModel {
        dialect: D_BIBTEX,
        bom_len: bom,
        trailing_newline: source.last() == Some(&b'\n'),
        doc_len: source.len() as u64,
        entries,
        items: Vec::new(),
        json: None,
    })
}

/// Parse one BibTeX entry body (starting just after the opening `{`), returning the
/// entry, the index of the byte just past its closing `}`, and the brace depth seen.
#[allow(clippy::too_many_arguments)]
fn parse_bibtex_body(
    source: &[u8],
    start: usize,
    entry_start: usize,
    entry_type: &[u8],
    type_start: usize,
    type_end: usize,
    limits: Limits,
    build: bool,
    total_fields: &mut u64,
) -> Result<(BibEntry, usize, u32)> {
    if eq_ignore_ascii_case(entry_type, b"comment") {
        let content_start = start;
        let (close, depth) = scan_balanced(source, start, b'{', b'}', limits)?;
        let mut entry = BibEntry {
            kind: E_COMMENT,
            terminator: terminator_at(source, close + 1),
            start: entry_start as u64,
            end: (close + 1) as u64,
            type_start: type_start as u64,
            type_end: type_end as u64,
            key_start: type_end as u64,
            key_end: type_end as u64,
            fields: Vec::new(),
        };
        if build {
            entry.fields.push(BibField {
                value_kind: V_RAW,
                flags: 0,
                name_start: content_start as u64,
                name_end: content_start as u64,
                value_start: content_start as u64,
                value_end: close as u64,
            });
            *total_fields += 1;
        }
        return Ok((entry, close + 1, depth));
    }
    if eq_ignore_ascii_case(entry_type, b"preamble") {
        let mut i = skip_ws(source, start);
        let (value_start, value_end, kind, concat, ni, depth) = parse_value(source, i, limits, 0)?;
        i = skip_ws(source, ni);
        if i >= source.len() || source[i] != b'}' {
            return Err(invalid_bib("unterminated BibTeX @preamble entry"));
        }
        let mut entry = BibEntry {
            kind: E_PREAMBLE,
            terminator: terminator_at(source, i + 1),
            start: entry_start as u64,
            end: (i + 1) as u64,
            type_start: type_start as u64,
            type_end: type_end as u64,
            key_start: type_end as u64,
            key_end: type_end as u64,
            fields: Vec::new(),
        };
        if build {
            entry.fields.push(BibField {
                value_kind: kind,
                flags: if concat { FF_CONCAT } else { 0 },
                name_start: value_start as u64,
                name_end: value_start as u64,
                value_start: value_start as u64,
                value_end: value_end as u64,
            });
            *total_fields += 1;
            check_field_cap(total_fields, limits)?;
        }
        return Ok((entry, i + 1, depth));
    }
    let string_entry = eq_ignore_ascii_case(entry_type, b"string");
    let mut i = skip_ws(source, start);
    let mut key_start = i;
    let mut key_end = i;
    if !string_entry {
        key_start = i;
        while i < source.len() && source[i] != b',' && source[i] != b'}' {
            i += 1;
        }
        key_end = trim_end_ws(source, key_start, i);
        if i < source.len() && source[i] == b',' {
            i += 1;
        }
    }
    let mut fields: Vec<BibField> = Vec::new();
    let mut seen: Vec<Vec<u8>> = Vec::new();
    let mut depth_max = 1u32;
    loop {
        i = skip_ws(source, i);
        if i >= source.len() {
            return Err(invalid_bib("unterminated BibTeX entry body"));
        }
        if source[i] == b'}' {
            break;
        }
        let name_start = i;
        while i < source.len() && is_name_byte(source[i]) {
            i += 1;
        }
        if i == name_start {
            return Err(invalid_bib("BibTeX field is missing a name"));
        }
        let name_end = i;
        i = skip_ws(source, i);
        if i >= source.len() || source[i] != b'=' {
            return Err(invalid_bib("BibTeX field name must be followed by `=`"));
        }
        i += 1;
        let (value_start, value_end, kind, concat, ni, depth) = parse_value(source, i, limits, 0)?;
        i = ni;
        if depth > depth_max {
            depth_max = depth;
        }
        let mut flags = if concat { FF_CONCAT } else { 0 };
        if !string_entry {
            // Duplicate field names are preserved and reported (case-insensitive, as
            // BibTeX field names are case-insensitive).
            let name = &source[name_start..name_end];
            if build {
                if seen.iter().any(|s| eq_ignore_ascii_case(s, name)) {
                    flags |= FF_DUPLICATE;
                } else {
                    seen.push(name.to_vec());
                }
            }
        }
        if build {
            fields.push(BibField {
                value_kind: kind,
                flags,
                name_start: name_start as u64,
                name_end: name_end as u64,
                value_start: value_start as u64,
                value_end: value_end as u64,
            });
            *total_fields += 1;
            check_field_cap(total_fields, limits)?;
        }
        i = skip_ws(source, i);
        if i >= source.len() {
            return Err(invalid_bib("unterminated BibTeX entry body"));
        }
        if source[i] == b',' {
            i += 1;
            continue;
        }
        if source[i] == b'}' {
            break;
        }
        return Err(invalid_bib("BibTeX entry body has an unexpected byte"));
    }
    let entry = BibEntry {
        kind: if string_entry { E_STRING } else { E_ENTRY },
        terminator: terminator_at(source, i + 1),
        start: entry_start as u64,
        end: (i + 1) as u64,
        type_start: type_start as u64,
        type_end: type_end as u64,
        key_start: key_start as u64,
        key_end: key_end as u64,
        fields,
    };
    Ok((entry, i + 1, depth_max))
}

fn check_field_cap(total: &u64, limits: Limits) -> Result<()> {
    if *total > limits.max_bib_fields {
        return Err(Error::resource_limit(format!(
            "bibliography exceeds the {}-field cap",
            limits.max_bib_fields
        )));
    }
    Ok(())
}

/// Parse a BibTeX value expression (one or more `#`-concatenated segments). Returns
/// the joined span, the first segment's kind, whether concatenation was used, the
/// index just past the expression, and the deepest brace nesting seen.
fn parse_value(
    source: &[u8],
    mut i: usize,
    limits: Limits,
    depth_in: u32,
) -> Result<(usize, usize, u8, bool, usize, u32)> {
    i = skip_ws(source, i);
    if i >= source.len() {
        return Err(invalid_bib("BibTeX value is missing"));
    }
    let (value_start, mut value_end, kind, mut depth_max) =
        parse_value_segment(source, i, limits, depth_in)?;
    let mut next = value_end;
    let mut concat = false;
    loop {
        let j = skip_ws(source, next);
        if j < source.len() && source[j] == b'#' {
            concat = true;
            let k = skip_ws(source, j + 1);
            if k >= source.len() {
                return Err(invalid_bib("BibTeX concatenation is missing a segment"));
            }
            let (_, seg_end, _, seg_depth) = parse_value_segment(source, k, limits, depth_in)?;
            if seg_depth > depth_max {
                depth_max = seg_depth;
            }
            value_end = seg_end;
            next = seg_end;
            continue;
        }
        next = j;
        break;
    }
    Ok((value_start, value_end, kind, concat, next, depth_max))
}

/// Parse one BibTeX value segment. Returns the span, the kind, the index just past
/// it, and the deepest brace nesting seen (base `depth_in`).
fn parse_value_segment(
    source: &[u8],
    i: usize,
    limits: Limits,
    depth_in: u32,
) -> Result<(usize, usize, u8, u32)> {
    let b = *source
        .get(i)
        .ok_or_else(|| invalid_bib("BibTeX value is missing"))?;
    if b == b'{' {
        let (close, depth) = scan_balanced(source, i, b'{', b'}', limits)?;
        enforce_value_bytes(i, close + 1, limits)?;
        let total = depth_in.saturating_add(depth);
        if total > limits.max_bib_depth {
            return Err(Error::resource_limit(format!(
                "bibliography brace nesting exceeds the {}-level cap",
                limits.max_bib_depth
            )));
        }
        return Ok((i, close + 1, V_BRACED, depth));
    }
    if b == b'"' {
        let close = scan_quoted(source, i, limits)?;
        enforce_value_bytes(i, close + 1, limits)?;
        return Ok((i, close + 1, V_QUOTED, 1));
    }
    let start = i;
    let mut j = i;
    while j < source.len() && is_name_byte(source[j]) {
        j += 1;
    }
    if j == start {
        return Err(invalid_bib(
            "BibTeX value is not a brace, quote, number, or macro",
        ));
    }
    enforce_value_bytes(start, j, limits)?;
    let all_digits = source[start..j].iter().all(|c| c.is_ascii_digit());
    Ok((start, j, if all_digits { V_NUMBER } else { V_MACRO }, 1))
}

fn enforce_value_bytes(start: usize, end: usize, limits: Limits) -> Result<()> {
    let len = (end - start) as u64;
    if len > limits.max_bib_value_bytes {
        return Err(Error::resource_limit(format!(
            "bibliography value is {len} bytes, above the {}-byte value cap",
            limits.max_bib_value_bytes
        )));
    }
    Ok(())
}

/// Scan a brace-balanced run beginning at `open` (the opening byte), returning the
/// index of the matching closing byte and the maximum nesting depth.
fn scan_balanced(
    source: &[u8],
    open_at: usize,
    open: u8,
    close: u8,
    limits: Limits,
) -> Result<(usize, u32)> {
    let mut depth: u32 = 1;
    let mut max_depth: u32 = 1;
    let mut i = open_at + 1;
    while i < source.len() {
        let c = source[i];
        if c == open {
            depth += 1;
            if depth > max_depth {
                max_depth = depth;
            }
            if depth > limits.max_bib_depth {
                return Err(Error::resource_limit(format!(
                    "bibliography brace nesting exceeds the {}-level cap",
                    limits.max_bib_depth
                )));
            }
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Ok((i, max_depth));
            }
        }
        i += 1;
    }
    Err(invalid_bib("unbalanced BibTeX brace"))
}

/// Scan a double-quoted BibTeX value beginning at the opening `"`, returning the
/// index of the closing `"`. A `"` at brace depth > 0 does not close the value.
fn scan_quoted(source: &[u8], open_at: usize, limits: Limits) -> Result<usize> {
    let mut depth: u32 = 0;
    let mut i = open_at + 1;
    while i < source.len() {
        match source[i] {
            b'{' => {
                depth += 1;
                if depth > limits.max_bib_depth {
                    return Err(Error::resource_limit(format!(
                        "bibliography brace nesting exceeds the {}-level cap",
                        limits.max_bib_depth
                    )));
                }
            }
            b'}' => depth = depth.saturating_sub(1),
            b'"' if depth == 0 => return Ok(i),
            _ => {}
        }
        i += 1;
    }
    Err(invalid_bib("unterminated BibTeX quoted value"))
}

/// The terminator tag immediately at `at`.
fn terminator_at(source: &[u8], at: usize) -> u8 {
    match source.get(at) {
        Some(&b'\n') => T_LF,
        Some(&b'\r') if source.get(at + 1) == Some(&b'\n') => T_CRLF,
        _ => T_NONE,
    }
}

fn trim_end_ws(source: &[u8], start: usize, end: usize) -> usize {
    let mut e = end;
    while e > start && source[e - 1].is_ascii_whitespace() {
        e -= 1;
    }
    e
}

fn eq_ignore_ascii_case(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .all(|(x, y)| x.eq_ignore_ascii_case(y))
}

// ---------------------------------------------------------------------------
// RIS parsing
// ---------------------------------------------------------------------------

/// Parse the whole source as a sequence of RIS entries.
fn parse_ris(source: &[u8], bom: u32, limits: Limits, build: bool) -> Result<BibModel> {
    let mut entries: Vec<BibEntry> = Vec::new();
    let mut total_fields: u64 = 0;
    let mut trailing_newline = false;
    let mut it = LineIter::new(source, bom as usize);
    // In-progress entry, as (start, type_start, type_end, fields, end, terminator).
    let mut cur: Option<(usize, usize, usize, Vec<BibField>, usize, u8)> = None;
    while let Some(line) = it.next_line() {
        trailing_newline = line.term != T_NONE;
        let content = &source[line.start..line.content_end];
        if is_blank(content) {
            if cur.is_some() {
                return Err(invalid_bib("blank line inside a RIS entry"));
            }
            continue;
        }
        if let Some((tag, vstart)) = ris_tag(content) {
            let is_er = tag == b"ER";
            let name_start = line.start;
            let value_start = line.start + vstart;
            let value_end = line.content_end;
            if cur.is_none() {
                if tag != b"TY" {
                    return Err(invalid_bib("a RIS entry must begin with a `TY  - ` line"));
                }
                check_entry_cap(entries.len() as u64, limits)?;
                let mut fields = Vec::new();
                if build {
                    fields.push(BibField {
                        value_kind: V_RAW,
                        flags: 0,
                        name_start: name_start as u64,
                        name_end: (name_start + 2) as u64,
                        value_start: value_start as u64,
                        value_end: value_end as u64,
                    });
                    total_fields += 1;
                    check_field_cap(&total_fields, limits)?;
                }
                cur = Some((
                    line.start,
                    value_start,
                    value_end,
                    fields,
                    value_end,
                    line.term,
                ));
            } else {
                {
                    let state = cur.as_mut().unwrap();
                    if build {
                        state.3.push(BibField {
                            value_kind: V_RAW,
                            flags: 0,
                            name_start: name_start as u64,
                            name_end: (name_start + 2) as u64,
                            value_start: value_start as u64,
                            value_end: value_end as u64,
                        });
                        total_fields += 1;
                        check_field_cap(&total_fields, limits)?;
                    }
                    state.4 = value_end;
                    state.5 = line.term;
                }
                if is_er {
                    let (start, type_start, type_end, fields, end, term) = cur.take().unwrap();
                    entries.push(BibEntry {
                        kind: E_ENTRY,
                        terminator: term,
                        start: start as u64,
                        end: end as u64,
                        type_start: type_start as u64,
                        type_end: type_end as u64,
                        key_start: type_end as u64,
                        key_end: type_end as u64,
                        fields,
                    });
                }
            }
        } else {
            // A continuation line extends the previous field's value span.
            match cur.as_mut() {
                Some(state) => {
                    if build {
                        let last = state.3.last_mut().ok_or_else(|| {
                            invalid_bib("RIS continuation with no preceding field")
                        })?;
                        last.value_end = line.content_end as u64;
                        last.flags |= FF_CONTINUED;
                    }
                    state.4 = line.content_end;
                    state.5 = line.term;
                }
                None => return Err(invalid_bib("a non-tag line appeared between RIS entries")),
            }
        }
    }
    if cur.is_some() {
        return Err(invalid_bib("unterminated RIS entry (missing `ER  - `)"));
    }
    if entries.is_empty() {
        return Err(invalid_bib("no RIS entries were found"));
    }
    Ok(BibModel {
        dialect: D_RIS,
        bom_len: bom,
        trailing_newline,
        doc_len: source.len() as u64,
        entries,
        items: Vec::new(),
        json: None,
    })
}

fn check_entry_cap(count: u64, limits: Limits) -> Result<()> {
    if count >= limits.max_bib_entries as u64 {
        return Err(Error::resource_limit(format!(
            "bibliography exceeds the {}-entry cap",
            limits.max_bib_entries
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// CSL-JSON parsing
// ---------------------------------------------------------------------------

/// Parse the whole source as CSL-JSON (an array of items, or a single item).
fn parse_csl(source: &[u8], limits: Limits, build: bool) -> Result<BibModel> {
    let json = json::parse(source, limits, true).map_err(|e| match e.class() {
        ErrorClass::ResourceLimit => e,
        _ => invalid_bib(format!(
            "CSL-JSON source is not a single JSON value: {}",
            e.message()
        )),
    })?;
    if json.nodes.len() as u64 > limits.max_bib_nodes as u64 {
        return Err(Error::resource_limit(format!(
            "bibliography exceeds the {}-node cap",
            limits.max_bib_nodes
        )));
    }
    if json.max_depth > limits.max_bib_depth {
        return Err(Error::resource_limit(format!(
            "bibliography nesting exceeds the {}-level depth cap",
            limits.max_bib_depth
        )));
    }
    let root = json
        .node(json.root)
        .ok_or_else(|| corrupt("root index is out of range"))?;
    let mut items: Vec<BibItem> = Vec::new();
    match root.kind {
        K_ARRAY => {
            let children = root.children.clone();
            if children.is_empty() {
                return Err(invalid_bib("a CSL-JSON array must carry at least one item"));
            }
            if children.len() as u64 > limits.max_bib_entries as u64 {
                return Err(Error::resource_limit(format!(
                    "bibliography exceeds the {}-item cap",
                    limits.max_bib_entries
                )));
            }
            for el in children {
                let n = json
                    .node(el)
                    .ok_or_else(|| corrupt("array element index is out of range"))?;
                if n.kind != K_OBJECT {
                    return Err(invalid_bib("each CSL-JSON array element must be an object"));
                }
                items.push(classify_item(&json, source, el, limits)?);
            }
        }
        K_OBJECT => {
            items.push(classify_item(&json, source, json.root, limits)?);
        }
        _ => {
            return Err(invalid_bib(
                "a CSL-JSON root must be an item object or an array of item objects",
            ));
        }
    }
    let _ = build;
    Ok(BibModel {
        dialect: D_CSL_JSON,
        bom_len: 0,
        trailing_newline: false,
        doc_len: json.doc_len,
        entries: Vec::new(),
        items,
        json: Some(json),
    })
}

/// Classify one CSL-JSON item object: it must carry a string `id` **and** a string
/// `type`.
fn classify_item(json: &JsonModel, source: &[u8], obj: u32, limits: Limits) -> Result<BibItem> {
    let node = json
        .node(obj)
        .ok_or_else(|| corrupt("item node index is out of range"))?;
    if node.kind != K_OBJECT {
        return Err(invalid_bib("a CSL-JSON item must be an object"));
    }
    let id_node = object_member_node(json, source, obj, "id")?;
    let type_node = object_member_node(json, source, obj, "type")?;
    let id_node = match id_node {
        Some(n) if json.node(n).map(|x| x.kind) == Some(K_STRING) => n,
        _ => return Err(invalid_bib("a CSL-JSON item must carry a string `id`")),
    };
    let type_node = match type_node {
        Some(n) if json.node(n).map(|x| x.kind) == Some(K_STRING) => n,
        _ => return Err(invalid_bib("a CSL-JSON item must carry a string `type`")),
    };
    let idn = json
        .node(id_node)
        .ok_or_else(|| corrupt("item id node is out of range"))?;
    let tyn = json
        .node(type_node)
        .ok_or_else(|| corrupt("item type node is out of range"))?;
    for n in [idn, tyn] {
        if (n.end - n.start) > limits.max_bib_value_bytes {
            return Err(Error::resource_limit(format!(
                "bibliography value is {} bytes, above the {}-byte value cap",
                n.end - n.start,
                limits.max_bib_value_bytes
            )));
        }
    }
    Ok(BibItem {
        node: obj,
        id_node,
        type_node,
        start: node.start,
        end: node.end,
    })
}

/// The value node of the **first** object member named `key`, if any.
fn object_member_node(json: &JsonModel, source: &[u8], obj: u32, key: &str) -> Result<Option<u32>> {
    let node = json
        .node(obj)
        .ok_or_else(|| corrupt("object index is out of range"))?;
    if node.kind != K_OBJECT {
        return Err(corrupt("node is not a JSON object"));
    }
    let mut i = 0usize;
    while i + 1 < node.children.len() {
        let k = node.children[i];
        let v = node.children[i + 1];
        i += 2;
        let kn = json
            .node(k)
            .ok_or_else(|| corrupt("object key index is out of range"))?;
        if kn.kind == K_STRING && json::decode_string(source, kn)? == key {
            return Ok(Some(v));
        }
    }
    Ok(None)
}

// ---------------------------------------------------------------------------
// Semantic projections
// ---------------------------------------------------------------------------

/// A text field's exact value bytes (`[value_start, value_end)`, delimiters included).
pub fn field_value_bytes<'a>(source: &'a [u8], field: &BibField) -> Result<&'a [u8]> {
    slice(source, field.value_start, field.value_end)
}

/// A text field's name/tag bytes (`[name_start, name_end)`, empty when unnamed).
pub fn field_name_bytes<'a>(source: &'a [u8], field: &BibField) -> Result<&'a [u8]> {
    slice(source, field.name_start, field.name_end)
}

/// A text field's decoded value text: the inner text for a braced/quoted value, else
/// the exact bytes rendered lossily.
pub fn field_value_text(source: &[u8], field: &BibField) -> Result<String> {
    let bytes = field_value_bytes(source, field)?;
    let inner = match field.value_kind {
        V_BRACED | V_QUOTED => {
            if bytes.len() >= 2 {
                &bytes[1..bytes.len() - 1]
            } else {
                bytes
            }
        }
        _ => bytes,
    };
    Ok(String::from_utf8_lossy(inner).into_owned())
}

/// A text entry's exact type-token bytes.
pub fn entry_type_bytes<'a>(source: &'a [u8], entry: &BibEntry) -> Result<&'a [u8]> {
    slice(source, entry.type_start, entry.type_end)
}

/// A text entry's decoded type text.
pub fn entry_type_text(source: &[u8], entry: &BibEntry) -> Result<String> {
    Ok(String::from_utf8_lossy(entry_type_bytes(source, entry)?).into_owned())
}

/// A text entry's decoded key text (empty when there is no key).
pub fn entry_key_text(source: &[u8], entry: &BibEntry) -> Result<String> {
    Ok(String::from_utf8_lossy(entry_key_bytes(source, entry)?).into_owned())
}

/// A text field's decoded name/tag text.
pub fn field_name_text(source: &[u8], field: &BibField) -> Result<String> {
    Ok(String::from_utf8_lossy(field_name_bytes(source, field)?).into_owned())
}

/// A CSL-JSON arena node's exact token bytes.
pub fn node_token_bytes<'a>(model: &BibModel, source: &'a [u8], node: u32) -> Result<&'a [u8]> {
    let n = model
        .node(node)
        .ok_or_else(|| corrupt("node index is out of range"))?;
    json::token_bytes(source, n)
}

/// A CSL-JSON arena node's value text: the decoded string for a string node, else the
/// exact token bytes rendered lossily.
pub fn node_value_text(model: &BibModel, source: &[u8], node: u32) -> Result<String> {
    let n = model
        .node(node)
        .ok_or_else(|| corrupt("node index is out of range"))?;
    if n.kind == K_STRING {
        json::decode_string(source, n)
    } else {
        Ok(String::from_utf8_lossy(json::token_bytes(source, n)?).into_owned())
    }
}

/// A text entry's exact key bytes (empty when there is no key).
pub fn entry_key_bytes<'a>(source: &'a [u8], entry: &BibEntry) -> Result<&'a [u8]> {
    slice(source, entry.key_start, entry.key_end)
}

/// An entry's exact content bytes (`[start, end)`).
pub fn entry_bytes<'a>(source: &'a [u8], entry: &BibEntry) -> Result<&'a [u8]> {
    slice(source, entry.start, entry.end)
}

/// A CSL-JSON item's exact object bytes.
pub fn item_bytes<'a>(model: &BibModel, source: &'a [u8], item: &BibItem) -> Result<&'a [u8]> {
    let _ = model;
    slice(source, item.start, item.end)
}

/// A CSL-JSON item's decoded `id` string.
pub fn item_id_text(model: &BibModel, source: &[u8], item: &BibItem) -> Result<String> {
    decode_string_node(model, source, item.id_node)
}

/// A CSL-JSON item's decoded `type` string.
pub fn item_type_text(model: &BibModel, source: &[u8], item: &BibItem) -> Result<String> {
    decode_string_node(model, source, item.type_node)
}

fn decode_string_node(model: &BibModel, source: &[u8], node: u32) -> Result<String> {
    if node == NONE {
        return Ok(String::new());
    }
    let n = model
        .node(node)
        .ok_or_else(|| corrupt("item string node is out of range"))?;
    json::decode_string(source, n)
}

/// The value node of the first member named `key` of the CSL-JSON item at `index`.
pub fn item_member_node(
    model: &BibModel,
    source: &[u8],
    index: u32,
    key: &str,
) -> Result<Option<u32>> {
    let item = model
        .item(index)
        .ok_or_else(|| corrupt("item index is out of range"))?;
    let json = model
        .json
        .as_ref()
        .ok_or_else(|| corrupt("item model has no arena"))?;
    object_member_node(json, source, item.node, key)
}

/// The index of the first field of the entry at `index` whose name matches `name`.
/// BibTeX field names are matched case-insensitively; RIS tags exactly.
pub fn entry_field_index(
    model: &BibModel,
    source: &[u8],
    index: u32,
    name: &str,
) -> Result<Option<u32>> {
    let entry = model
        .entry(index)
        .ok_or_else(|| corrupt("entry index is out of range"))?;
    for (fi, f) in entry.fields.iter().enumerate() {
        let nb = field_name_bytes(source, f)?;
        let hit = if model.dialect == D_BIBTEX {
            nb.eq_ignore_ascii_case(name.as_bytes())
        } else {
            nb == name.as_bytes()
        };
        if hit {
            return Ok(Some(fi as u32));
        }
    }
    Ok(None)
}

/// The index of the first entry whose citation key (BibTeX) or `ID` field (RIS) or
/// `id` string (CSL-JSON) equals `key`.
pub fn entry_key_index(model: &BibModel, source: &[u8], key: &str) -> Result<Option<u32>> {
    if model.is_json() {
        for (i, item) in model.items.iter().enumerate() {
            if item_id_text(model, source, item)? == key {
                return Ok(Some(i as u32));
            }
        }
        return Ok(None);
    }
    for (i, entry) in model.entries.iter().enumerate() {
        if model.dialect == D_BIBTEX {
            let kb = entry_key_bytes(source, entry)?;
            if !kb.is_empty() && kb == key.as_bytes() {
                return Ok(Some(i as u32));
            }
        } else {
            if let Some(fi) = entry_field_index(model, source, i as u32, "ID")? {
                let f = &entry.fields[fi as usize];
                if field_value_text(source, f)? == key {
                    return Ok(Some(i as u32));
                }
            }
        }
    }
    Ok(None)
}

/// A bounded, case-sensitive lexical search. For the text dialects it searches every
/// entry's type/key, every field name, and every field value; for CSL-JSON it reuses
/// the shared JSON [`json::find`] over the embedded arena.
pub fn find(
    model: &BibModel,
    source: &[u8],
    pattern: &str,
    limits: Limits,
) -> Result<Vec<BibMatch>> {
    if let Some(json) = &model.json {
        let matches: Vec<JsonMatch> = json::find(json, source, pattern, limits)?;
        return Ok(matches
            .into_iter()
            .map(|m| BibMatch {
                entry: NONE,
                field: NONE,
                role: match m.role {
                    json::MatchRole::Key => R_MEMBER_KEY,
                    json::MatchRole::Value => R_MEMBER_VALUE,
                },
                start: m.start,
                end: m.end,
                pointer: m.pointer,
                text: m.text,
            })
            .collect());
    }
    let mut out: Vec<BibMatch> = Vec::new();
    for (ei, entry) in model.entries.iter().enumerate() {
        let ty = entry_type_bytes(source, entry)?;
        if contains(ty, pattern.as_bytes()) {
            out.push(mk_match(
                ei as u32,
                NONE,
                R_ENTRY_TYPE,
                entry.type_start,
                entry.type_end,
                ty,
            ));
        }
        let kb = entry_key_bytes(source, entry)?;
        if !kb.is_empty() && contains(kb, pattern.as_bytes()) {
            out.push(mk_match(
                ei as u32,
                NONE,
                R_ENTRY_KEY,
                entry.key_start,
                entry.key_end,
                kb,
            ));
        }
        for (fi, f) in entry.fields.iter().enumerate() {
            let nb = field_name_bytes(source, f)?;
            if !nb.is_empty() && contains(nb, pattern.as_bytes()) {
                out.push(mk_match(
                    ei as u32,
                    fi as u32,
                    R_FIELD_NAME,
                    f.name_start,
                    f.name_end,
                    nb,
                ));
            }
            let vb = field_value_bytes(source, f)?;
            if contains(vb, pattern.as_bytes()) {
                out.push(mk_match(
                    ei as u32,
                    fi as u32,
                    R_FIELD_VALUE,
                    f.value_start,
                    f.value_end,
                    vb,
                ));
            }
        }
    }
    Ok(out)
}

fn mk_match(entry: u32, field: u32, role: u8, start: u64, end: u64, bytes: &[u8]) -> BibMatch {
    BibMatch {
        entry,
        field,
        role,
        start,
        end,
        pointer: String::new(),
        text: String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Render a deterministic canonical text projection: for the text dialects, each
/// entry's exact bytes, one entry per line; for CSL-JSON, the shared JSON canonical
/// text. A derived, lossy view — the exact bytes stay available.
pub fn canonical_text(model: &BibModel, source: &[u8], max_out: u64) -> Result<String> {
    if let Some(json) = &model.json {
        return json::canonical_text(json, source);
    }
    let mut out = String::new();
    for (i, entry) in model.entries.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&String::from_utf8_lossy(entry_bytes(source, entry)?));
        if out.len() as u64 > max_out {
            return Err(Error::resource_limit(format!(
                "bibliography text projection exceeds the {max_out}-byte budget"
            )));
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > hay.len() {
        return false;
    }
    hay.windows(needle.len()).any(|w| w == needle)
}

fn slice(source: &[u8], start: u64, end: u64) -> Result<&[u8]> {
    let s = usize::try_from(start).map_err(|_| corrupt("span overflow"))?;
    let e = usize::try_from(end).map_err(|_| corrupt("span overflow"))?;
    source
        .get(s..e)
        .ok_or_else(|| corrupt("span is outside the source"))
}

fn corrupt(msg: &str) -> Error {
    Error::invalid_model(format!("corrupt bibliography model: {msg}"))
}

fn invalid_bib(msg: impl Into<String>) -> Error {
    Error::invalid_bib_structure(msg)
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(b: &'a [u8]) -> Reader<'a> {
        Reader { b, at: 0 }
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| corrupt("reader overflow"))?;
        let s = self
            .b
            .get(self.at..end)
            .ok_or_else(|| corrupt("unexpected end of model"))?;
        self.at = end;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const BIBTEX: &[u8] = br#"@article{smith2020,
  author = {Smith, John and {Doe}, Jane},
  title = "A {Study} of Things",
  year = 2020,
  month = jan,
  note = {first},
  note = {second}
}

@string{ acm = "Association for Computing Machinery" }

@preamble{ "\newcommand{\noopsort}[1]{}" }

@comment{ this is a comment }
"#;

    const RIS: &[u8] = b"TY  - JOUR\nAU  - Smith, John\nAU  - Doe, Jane\nTI  - A long title\n      continued here\nPY  - 2020\nER  - \n";

    const CSL: &[u8] = br#"[
  {"id":"smith2020","type":"article-journal","title":"A Study","author":[{"family":"Smith","given":"John"}]},
  {"id":"doe2019","type":"book","title":"Things"}
]
"#;

    #[test]
    fn detects_the_three_dialects_and_rejects_others() {
        assert_eq!(
            parse(BIBTEX, Limits::DEFAULT, false).unwrap().dialect,
            D_BIBTEX
        );
        assert_eq!(parse(RIS, Limits::DEFAULT, false).unwrap().dialect, D_RIS);
        assert_eq!(
            parse(CSL, Limits::DEFAULT, false).unwrap().dialect,
            D_CSL_JSON
        );
        assert!(detect(BIBTEX, Limits::DEFAULT));
        assert!(detect(RIS, Limits::DEFAULT));
        assert!(detect(CSL, Limits::DEFAULT));

        // Boundaries: plain JSON, a JSON array of non-CSL objects, and prose decline.
        assert!(parse(br#"{"a":1}"#, Limits::DEFAULT, false).is_err());
        assert!(parse(br#"[{"a":1},{"b":2}]"#, Limits::DEFAULT, false).is_err());
        assert!(parse(br#"[1,2,3]"#, Limits::DEFAULT, false).is_err());
        assert!(parse(b"just prose\nwith lines\n", Limits::DEFAULT, false).is_err());
        assert!(!detect(b"just prose\nwith lines\n", Limits::DEFAULT));
    }

    #[test]
    fn bibtex_preserves_spans_types_order_and_duplicates() {
        let m = parse(BIBTEX, Limits::DEFAULT, true).unwrap();
        assert_eq!(m.dialect, D_BIBTEX);
        assert_eq!(m.entries.len(), 4);
        let e = &m.entries[0];
        assert_eq!(e.kind, E_ENTRY);
        assert_eq!(
            entry_type_bytes(BIBTEX, e).unwrap(),
            b"article" // case as written
        );
        assert_eq!(entry_key_bytes(BIBTEX, e).unwrap(), b"smith2020");
        assert_eq!(e.fields.len(), 6);
        // Four value forms are distinct.
        assert_eq!(e.fields[0].value_kind, V_BRACED);
        assert_eq!(e.fields[1].value_kind, V_QUOTED);
        assert_eq!(e.fields[2].value_kind, V_NUMBER);
        assert_eq!(e.fields[3].value_kind, V_MACRO);
        assert_eq!(e.fields[4].value_kind, V_BRACED);
        // Duplicate field is reported on the later occurrence.
        assert_eq!(e.fields[4].flags & FF_DUPLICATE, 0);
        assert_eq!(e.fields[5].flags & FF_DUPLICATE, FF_DUPLICATE);
        assert_eq!(e.duplicate_field_count(), 1);
        // Special entries.
        assert_eq!(m.entries[1].kind, E_STRING);
        assert_eq!(m.entries[2].kind, E_PREAMBLE);
        assert_eq!(m.entries[3].kind, E_COMMENT);
    }

    #[test]
    fn ris_preserves_tags_order_and_continuation() {
        let m = parse(RIS, Limits::DEFAULT, true).unwrap();
        assert_eq!(m.dialect, D_RIS);
        assert_eq!(m.entries.len(), 1);
        let e = &m.entries[0];
        assert_eq!(entry_type_bytes(RIS, e).unwrap(), b"JOUR");
        assert_eq!(e.fields.len(), 6);
        assert_eq!(field_name_bytes(RIS, &e.fields[0]).unwrap(), b"TY");
        assert_eq!(field_name_bytes(RIS, &e.fields[5]).unwrap(), b"ER");
        // The TI field's value spans the continuation line.
        assert_eq!(e.fields[3].flags & FF_CONTINUED, FF_CONTINUED);
        let v = field_value_text(RIS, &e.fields[3]).unwrap();
        assert!(v.contains("A long title"));
        assert!(v.contains("continued here"));
    }

    #[test]
    fn csl_preserves_items_and_strings() {
        let m = parse(CSL, Limits::DEFAULT, true).unwrap();
        assert_eq!(m.dialect, D_CSL_JSON);
        assert_eq!(m.items.len(), 2);
        assert_eq!(item_id_text(&m, CSL, &m.items[0]).unwrap(), "smith2020");
        assert_eq!(
            item_type_text(&m, CSL, &m.items[0]).unwrap(),
            "article-journal"
        );
        assert_eq!(item_id_text(&m, CSL, &m.items[1]).unwrap(), "doe2019");
        // A member lookup is by key.
        assert!(item_member_node(&m, CSL, 0, "title").unwrap().is_some());
        assert!(item_member_node(&m, CSL, 0, "nope").unwrap().is_none());
    }

    #[test]
    fn model_roundtrips_and_fails_closed() {
        for src in [BIBTEX, RIS, CSL] {
            let m = parse(src, Limits::DEFAULT, true).unwrap();
            let enc = m.encode();
            let dec = BibModel::decode(&enc).unwrap();
            assert_eq!(dec, m);
        }
        // Corruption fails closed.
        let mut enc = parse(CSL, Limits::DEFAULT, true).unwrap().encode();
        enc[0] = b'X';
        assert!(BibModel::decode(&enc).is_err());
        let enc = parse(BIBTEX, Limits::DEFAULT, true).unwrap().encode();
        assert!(BibModel::decode(&enc[..enc.len() - 1]).is_err());
    }

    #[test]
    fn find_reports_fields_and_values() {
        let m = parse(BIBTEX, Limits::DEFAULT, true).unwrap();
        let hits = find(&m, BIBTEX, "smith2020", Limits::DEFAULT).unwrap();
        assert!(hits.iter().any(|h| h.role == R_ENTRY_KEY));
        let hits = find(&m, BIBTEX, "Study", Limits::DEFAULT).unwrap();
        assert!(hits.iter().any(|h| h.role == R_FIELD_VALUE));

        let m = parse(CSL, Limits::DEFAULT, true).unwrap();
        let hits = find(&m, CSL, "smith2020", Limits::DEFAULT).unwrap();
        assert!(hits.iter().any(|h| h.role == R_MEMBER_VALUE));
    }

    #[test]
    fn caps_decline_typed() {
        let tight = Limits {
            max_bib_entries: 1,
            ..Limits::DEFAULT
        };
        assert_eq!(
            parse(BIBTEX, tight, true).unwrap_err().class(),
            ErrorClass::ResourceLimit
        );
        let tight = Limits {
            max_bib_fields: 1,
            ..Limits::DEFAULT
        };
        assert_eq!(
            parse(BIBTEX, tight, true).unwrap_err().class(),
            ErrorClass::ResourceLimit
        );
        let tight = Limits {
            max_bib_value_bytes: 2,
            ..Limits::DEFAULT
        };
        assert_eq!(
            parse(BIBTEX, tight, true).unwrap_err().class(),
            ErrorClass::ResourceLimit
        );
        let tight = Limits {
            max_bib_nodes: 1,
            ..Limits::DEFAULT
        };
        assert_eq!(
            parse(CSL, tight, true).unwrap_err().class(),
            ErrorClass::ResourceLimit
        );
        let tight = Limits {
            max_bib_depth: 1,
            ..Limits::DEFAULT
        };
        assert_eq!(
            parse(CSL, tight, true).unwrap_err().class(),
            ErrorClass::ResourceLimit
        );
        let tight = Limits {
            max_bib_document_bytes: 4,
            ..Limits::DEFAULT
        };
        assert_eq!(
            parse(BIBTEX, tight, true).unwrap_err().class(),
            ErrorClass::ResourceLimit
        );
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut x: u64 = 0x1234_5678_9ABC_DEF0;
        for _ in 0..128 {
            let mut buf = vec![0u8; 512];
            for b in buf.iter_mut() {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                *b = (x & 0xFF) as u8;
            }
            let _ = detect(&buf, Limits::STRICT);
            if let Ok(m) = parse(&buf, Limits::STRICT, true) {
                let _ = canonical_text(&m, &buf, 1 << 20);
                let _ = find(&m, &buf, "a", Limits::STRICT);
            }
            let _ = build_bib_model(&buf, Limits::STRICT);
            let _ = BibModel::decode(&buf);
        }
    }
}
