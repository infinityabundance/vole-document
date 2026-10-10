//! Bounded, representation-preserving INI / `.env` / Java `.properties` adapter
//! (Phase 21.20).
//!
//! The **config family** is the key/value line format. One adapter covers three
//! dialects — `ini`, `env`, and `properties` — and records the dialect in the
//! model, exactly as the CSV/TSV adapter records its delimiter/terminator
//! dialect. Like every other Wave-2 format it is *not* a package: there is no
//! OPC/ZIP layer, so the exact leaf is the **whole source** (a `DocumentExact`,
//! a RAW-like authority) and everything this module produces is a bounded,
//! deterministic (`Q_gen`) projection that never sits on the exactness path.
//!
//! ## Why a bespoke parser
//!
//! The point is to preserve **representation**, not merely values:
//!
//! * every line records its exact **byte span** (its terminator excluded, and
//!   the terminator spelling kept), so line order and inter-line bytes are
//!   preserved;
//! * an entry records exact spans for its **key**, its **separator** (`=`,
//!   `:`, or the whitespace run), and its **value** — quoting, inline comments,
//!   and continuations are never dropped;
//! * **duplicate keys are preserved and reported** (see below), never collapsed;
//! * a `properties` logical line that uses a trailing `\` **continuation** keeps
//!   the exact joined span (the backslash, the newline, and the continuation's
//!   leading whitespace all remain inside the span);
//! * a `properties` `\uXXXX` escape (and every other escape) is surfaced as its
//!   **spelling**, never expanded — [`decode_value`] joins continuations but does
//!   not interpret escapes.
//!
//! ## Duplicate-key policy (chosen: preserve-and-report)
//!
//! INI/`.env`/`.properties` have no normative duplicate-key rule (most readers
//! take "last wins", but they disagree). This adapter therefore takes the
//! **representation-preserving** policy used by JSON/YAML/CBOR: every entry is
//! kept in source order and duplicates are *reported* ([`ConfigModel::entry_count`]
//! plus per-key visibility through the selectors), never collapsed and never a
//! decline. That keeps the model a faithful projection of the source instead of
//! imposing a merging rule the source did not state.
//!
//! ## Detection (conservative — this is the risk)
//!
//! The config family has **no magic bytes**, and a key/value line is easy to
//! confuse with prose. Detection is therefore deliberately conservative and
//! requires a **dialect-distinguishing signal**:
//!
//! * **`ini`** — at least one `[section]` header, at least one entry, and every
//!   non-blank line a valid INI construct (`;`/`#` whole-line comments, `;`
//!   inline comments, `=`/`:` entries, optional quoting).
//! * **`properties`** — no sections, at least two entries, at least one entry
//!   with a **`=` or `:` separator** (a strong signal absent from prose), *and*
//!   at least one properties-only construct: a `:`/whitespace separator, a `!`
//!   comment, a `\uXXXX` escape, a line continuation, or a key outside the
//!   shell-identifier charset.
//! * **`env`** — no sections, at least two entries, every non-blank line a
//!   `KEY=VALUE` (optional `export `) entry with a shell-identifier key, *and*
//!   at least one `export ` prefix (the one `env`-only signal).
//!
//! Everything else falls back to
//! [`Opaque`](crate::field::document_format::DocumentFormat::Opaque). This
//! deliberately leaves the **pure `KEY=VALUE` overlap** opaque: a file that is
//! all `KEY=VALUE` lines, with `=` as the only separator, `#` the only comment
//! marker, and shell-identifier keys, is byte-for-byte the same shape under both
//! `env` and Java `properties` and is **not guessed** — it stays `Opaque`. Such a
//! file is claimed as a dialect only when it carries a dialect-only signal
//! (`export ` for `env`; `:`/whitespace/`!`/`\uXXXX`/continuation/non-identifier
//! key for `properties`). A plain-prose/`.txt`/Markdown/code blob stays `Opaque`
//! (a whitespace-separated prose line never satisfies the strong-separator
//! requirement), and a document already claimed by another format
//! (JSON/YAML/TOML/CSV/…) stays claimed by it.
//!
//! ### What the detector cannot distinguish (recorded honestly)
//!
//! * **`env` vs `properties`**: the pure `=`-only/`#`-only/identifier-key shape
//!   is shared verbatim; it is left `Opaque` rather than guessed.
//! * **`properties` vs prose**: whitespace-separated `key value` pairs are
//!   indistinguishable from prose in isolation; the detector requires a strong
//!   `=`/`:` separator somewhere, so a file that uses *only* the whitespace
//!   separator stays `Opaque` even though the adapter can represent it.
//! * **A shell script** that is only `#` comments and `export KEY=VALUE` lines
//!   would be classified `env`; a leading `#!` shebang declines instead.

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model lines (defends the decoder against a hostile blob).
pub const MAX_MODEL_LINES: u32 = 1 << 24;
/// Hard cap on decoded model entries (defends the decoder against a hostile blob).
pub const MAX_MODEL_ENTRIES: u64 = 1 << 24;

/// Dialect tag: INI (`[section]` + `key = value`).
pub const DIALECT_INI: u8 = 0;
/// Dialect tag: `.env` (`KEY=VALUE`, optional `export`).
pub const DIALECT_ENV: u8 = 1;
/// Dialect tag: Java `.properties` (`key=value` / `key:value` / `key value`,
/// `#`/`!` comments, trailing-`\` continuations, `\uXXXX` escapes).
pub const DIALECT_PROPERTIES: u8 = 2;

/// Line kind: a blank (whitespace-only, possibly empty) line.
pub const L_BLANK: u8 = 0;
/// Line kind: a whole-line comment (its marker byte is recorded in `marker`).
pub const L_COMMENT: u8 = 1;
/// Line kind: an INI `[section]` header.
pub const L_SECTION: u8 = 2;
/// Line kind: a key/value entry.
pub const L_ENTRY: u8 = 3;

/// Terminator tag: no terminator (the final line of a file with no trailing
/// newline).
pub const T_NONE: u8 = 0;
/// Terminator tag: `\n`.
pub const T_LF: u8 = 1;
/// Terminator tag: `\r\n`.
pub const T_CRLF: u8 = 2;
/// Terminator tag: a bare `\r`.
pub const T_CR: u8 = 3;

/// Entry flag: the `env` `export ` prefix was present.
pub const F_EXPORT: u8 = 1 << 0;
/// Entry flag: the value was single-quoted (`'…'`).
pub const F_SINGLE_QUOTED: u8 = 1 << 1;
/// Entry flag: the value was double-quoted (`"…"`).
pub const F_DOUBLE_QUOTED: u8 = 1 << 2;
/// Entry flag: a `properties` logical line used a trailing-`\` continuation.
pub const F_CONTINUED: u8 = 1 << 3;
/// Entry flag: an unquoted value was cut by an inline comment.
pub const F_INLINE_COMMENT: u8 = 1 << 4;

/// The set of admissible entry flags.
const FLAG_MASK: u8 = F_EXPORT | F_SINGLE_QUOTED | F_DOUBLE_QUOTED | F_CONTINUED | F_INLINE_COMMENT;

/// Match role: a key match (from [`find`]).
pub const ROLE_KEY: u8 = 0;
/// Match role: a value match (from [`find`]).
pub const ROLE_VALUE: u8 = 1;

/// Stable name for a dialect tag.
pub const fn dialect_name(d: u8) -> &'static str {
    match d {
        DIALECT_ENV => "env",
        DIALECT_PROPERTIES => "properties",
        _ => "ini",
    }
}

/// Stable name for a line kind.
pub const fn line_kind_name(k: u8) -> &'static str {
    match k {
        L_COMMENT => "comment",
        L_SECTION => "section",
        L_ENTRY => "entry",
        _ => "blank",
    }
}

/// Stable name for a match role.
pub const fn role_name(r: u8) -> &'static str {
    match r {
        ROLE_VALUE => "value",
        _ => "key",
    }
}

/// Stable name for a terminator tag.
pub const fn terminator_name(t: u8) -> &'static str {
    match t {
        T_CRLF => "crlf",
        T_CR => "cr",
        T_NONE => "none",
        _ => "lf",
    }
}

/// One parsed config line.
///
/// `start..end` is the line's exact content span (its terminator excluded; for a
/// continued `properties` logical line this spans the merged physical lines,
/// including the backslash and the embedded newline). `key_start..key_end` is
/// the key token (or the inner section name), `sep_start..sep_end` the separator
/// token (`=`/`:` plus any following whitespace run), and `value_start..value_end`
/// the value token (quotes included when quoted).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CfgLine {
    /// The line kind (`L_*`).
    pub kind: u8,
    /// The terminator tag (`T_*`) that ended the line.
    pub terminator: u8,
    /// The comment marker byte (`#`/`;`/`!`) for a comment line, the primary
    /// separator byte (`=`/`:`/whitespace) for an entry, else `0`.
    pub marker: u8,
    /// The entry flags (`F_*`).
    pub flags: u8,
    /// The line's first source byte.
    pub start: u64,
    /// One past the line's last content byte.
    pub end: u64,
    /// The key token's first source byte.
    pub key_start: u64,
    /// One past the key token.
    pub key_end: u64,
    /// The separator token's first source byte.
    pub sep_start: u64,
    /// One past the separator token.
    pub sep_end: u64,
    /// The value token's first source byte.
    pub value_start: u64,
    /// One past the value token.
    pub value_end: u64,
}

/// The canonical derived config-family model (the materialization of a
/// `ConfigModel` node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigModel {
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// The recorded dialect (`DIALECT_*`).
    pub dialect: u8,
    /// Whether the source ended with a line terminator.
    pub trailing_terminator: bool,
    /// The line arena, in physical (line) order.
    pub lines: Vec<CfgLine>,
}

impl ConfigModel {
    /// The line at `index`, if present.
    pub fn line(&self, index: u32) -> Option<&CfgLine> {
        self.lines.get(index as usize)
    }

    /// The total number of entries in the model.
    pub fn entry_count(&self) -> u64 {
        self.lines.iter().filter(|l| l.kind == L_ENTRY).count() as u64
    }

    /// The total number of section headers in the model.
    pub fn section_count(&self) -> u64 {
        self.lines.iter().filter(|l| l.kind == L_SECTION).count() as u64
    }

    /// The `index`-th (0-based) entry line, in source order.
    pub fn nth_entry(&self, index: u32) -> Option<&CfgLine> {
        self.lines
            .iter()
            .filter(|l| l.kind == L_ENTRY)
            .nth(index as usize)
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(24 + self.lines.len() * 68);
        out.extend_from_slice(b"CFGM");
        out.push(MODEL_VERSION);
        out.push(self.dialect);
        out.push(u8::from(self.trailing_terminator));
        out.push(0); // reserved
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&(self.lines.len() as u32).to_le_bytes());
        for l in &self.lines {
            out.push(l.kind);
            out.push(l.terminator);
            out.push(l.marker);
            out.push(l.flags);
            out.extend_from_slice(&l.start.to_le_bytes());
            out.extend_from_slice(&l.end.to_le_bytes());
            out.extend_from_slice(&l.key_start.to_le_bytes());
            out.extend_from_slice(&l.key_end.to_le_bytes());
            out.extend_from_slice(&l.sep_start.to_le_bytes());
            out.extend_from_slice(&l.sep_end.to_le_bytes());
            out.extend_from_slice(&l.value_start.to_le_bytes());
            out.extend_from_slice(&l.value_end.to_le_bytes());
        }
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<ConfigModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"CFGM" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let dialect = r.u8()?;
        if dialect > DIALECT_PROPERTIES {
            return Err(corrupt("unknown dialect"));
        }
        let flags = r.u8()?;
        if flags & !1 != 0 {
            return Err(corrupt("unknown model flags"));
        }
        let _reserved = r.u8()?;
        let doc_len = r.u64()?;
        let count = r.u32()?;
        if count > MAX_MODEL_LINES {
            return Err(corrupt("model line count is implausible"));
        }
        let mut lines = Vec::with_capacity(count as usize);
        let mut entries: u64 = 0;
        let mut prev_end: u64 = 0;
        for i in 0..count {
            let kind = r.u8()?;
            if kind > L_ENTRY {
                return Err(corrupt("unknown line kind"));
            }
            let terminator = r.u8()?;
            if terminator > T_CR {
                return Err(corrupt("unknown terminator"));
            }
            let marker = r.u8()?;
            if marker >= 0x80 {
                return Err(corrupt("implausible marker byte"));
            }
            let lflags = r.u8()?;
            if lflags & !FLAG_MASK != 0 {
                return Err(corrupt("unknown entry flags"));
            }
            let start = r.u64()?;
            let end = r.u64()?;
            let key_start = r.u64()?;
            let key_end = r.u64()?;
            let sep_start = r.u64()?;
            let sep_end = r.u64()?;
            let value_start = r.u64()?;
            let value_end = r.u64()?;
            if start > end || end > doc_len {
                return Err(corrupt("line span is outside the document"));
            }
            if i > 0 && start < prev_end {
                return Err(corrupt("line spans are not in source order"));
            }
            if key_start > key_end || key_end > doc_len {
                return Err(corrupt("key span is outside the document"));
            }
            if sep_start > sep_end || sep_end > doc_len {
                return Err(corrupt("separator span is outside the document"));
            }
            if value_start > value_end || value_end > doc_len {
                return Err(corrupt("value span is outside the document"));
            }
            match kind {
                L_ENTRY => {
                    if key_start < start || key_end > sep_start || sep_end > value_start {
                        return Err(corrupt("entry spans are not ordered"));
                    }
                    if value_end > end {
                        return Err(corrupt("entry value runs past the line"));
                    }
                    entries += 1;
                    if entries > MAX_MODEL_ENTRIES {
                        return Err(corrupt("model entry count is implausible"));
                    }
                }
                L_SECTION if key_start < start || key_end > end => {
                    return Err(corrupt("section name span is outside the line"));
                }
                _ => {}
            }
            prev_end = end;
            lines.push(CfgLine {
                kind,
                terminator,
                marker,
                flags: lflags,
                start,
                end,
                key_start,
                key_end,
                sep_start,
                sep_end,
                value_start,
                value_end,
            });
        }
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        Ok(ConfigModel {
            doc_len,
            dialect,
            trailing_terminator: flags & 1 != 0,
            lines,
        })
    }
}

/// One lexical match from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfgMatch {
    /// The 0-based line index.
    pub line: u32,
    /// The 0-based entry ordinal of the matching entry.
    pub entry: u32,
    /// The match role (`ROLE_KEY`/`ROLE_VALUE`).
    pub role: u8,
    /// The exact source span of the matching token.
    pub start: u64,
    /// One past the matching token.
    pub end: u64,
    /// The decoded token text.
    pub text: String,
}

/// Byte-based config-family detector. See the module docs for the exact,
/// conservative heuristic.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    classify(source, limits).is_ok()
}

/// Classify `source` as one config dialect, or decline typed when it is not a
/// config-family document under the conservative heuristic.
pub fn classify(source: &[u8], limits: Limits) -> Result<u8> {
    let probe = probe(source, limits)?;
    resolve(&probe).ok_or_else(|| {
        corrupt("source is not an INI/env/properties document under the conservative heuristic")
    })
}

/// Parse `source` into a [`ConfigModel`]. `build` selects whether the line arena
/// is populated (detection runs with `build = false` to stay bounded in memory).
pub fn parse(source: &[u8], limits: Limits, build: bool) -> Result<ConfigModel> {
    let probe = probe(source, limits)?;
    let dialect =
        resolve(&probe).ok_or_else(|| corrupt("source is not an INI/env/properties document"))?;
    let lines = if build {
        build_lines(source, dialect, limits)?
    } else {
        Vec::new()
    };
    Ok(ConfigModel {
        doc_len: source.len() as u64,
        dialect,
        trailing_terminator: probe.trailing_terminator,
        lines,
    })
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of a `ConfigModel` node).
pub fn build_config_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits, true)?.encode())
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// The dialect-shape signals gathered by [`probe`].
struct Probe {
    all_ini: bool,
    all_env: bool,
    all_prop: bool,
    has_section: bool,
    has_env_only: bool,
    has_prop_only: bool,
    has_strong_sep: bool,
    sections: u64,
    content_lines: u64,
    prop_entries: u64,
    trailing_terminator: bool,
}

fn resolve(p: &Probe) -> Option<u8> {
    if p.has_section {
        // A section header is only a valid INI construct. If every non-blank line
        // is a valid INI construct with at least one section and one entry, admit
        // INI; otherwise the document is not a config family at all.
        if p.all_ini && p.sections >= 1 && p.content_lines >= 1 {
            return Some(DIALECT_INI);
        }
        return None;
    }
    // No sections: `properties` needs a strong `=`/`:` separator plus a
    // properties-only signal; `env` needs an `export ` prefix. The pure
    // `KEY=VALUE` overlap satisfies neither and stays opaque.
    if p.all_prop && p.has_strong_sep && p.has_prop_only && p.prop_entries >= 2 {
        return Some(DIALECT_PROPERTIES);
    }
    if p.all_env && p.has_env_only && p.content_lines >= 2 {
        return Some(DIALECT_ENV);
    }
    None
}

fn probe(source: &[u8], limits: Limits) -> Result<Probe> {
    if source.is_empty() {
        return Err(corrupt("empty input is not a config document"));
    }
    if source.len() as u64 > limits.max_config_document_bytes {
        return Err(Error::resource_limit(format!(
            "config source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_config_document_bytes
        )));
    }
    if source.contains(&0) {
        return Err(corrupt("config source contains a NUL byte"));
    }
    if source.starts_with(b"#!") {
        return Err(corrupt("a `#!` shebang is a script, not a config document"));
    }
    let start = bom_len(source);
    let mut p = Probe {
        all_ini: true,
        all_env: true,
        all_prop: true,
        has_section: false,
        has_env_only: false,
        has_prop_only: false,
        has_strong_sep: false,
        sections: 0,
        content_lines: 0,
        prop_entries: 0,
        trailing_terminator: source.last().is_some_and(|b| *b == b'\n' || *b == b'\r'),
    };

    // Pass 1: INI and env shapes over physical lines (no continuation).
    let mut at = start;
    let mut line_count: u64 = 0;
    while let Some((s, e, _t, next)) = physical_line(source, at) {
        line_count += 1;
        if line_count > limits.max_config_lines as u64 {
            return Err(Error::resource_limit(format!(
                "config source exceeds the {}-line cap",
                limits.max_config_lines
            )));
        }
        let c = &source[s..e];
        check_line_bytes(e - s, limits)?;
        match first_non_ws(c) {
            None => {}
            Some(i) if c[i] == b'#' => {}
            Some(i) if c[i] == b';' => {
                // INI whole-line comment; env does not use `;`.
                p.all_env = false;
            }
            Some(i) if c[i] == b'!' => {
                p.all_ini = false;
                p.all_env = false;
            }
            Some(i) if is_ini_section(c, i) => {
                p.has_section = true;
                p.sections += 1;
                p.all_env = false;
            }
            _ => {
                p.content_lines += 1;
                if parse_entry_ini(source, s, e).is_none() {
                    p.all_ini = false;
                }
                match parse_entry_env(source, s, e) {
                    Some(ent) => {
                        if ent.flags & F_EXPORT != 0 {
                            p.has_env_only = true;
                        }
                    }
                    None => p.all_env = false,
                }
            }
        }
        at = next;
    }

    // Pass 2: properties shape over logical lines (with continuation).
    let mut at = start;
    let mut in_cont = false;
    let mut cont_depth: u32 = 0;
    while let Some((s, e, _t, next)) = physical_line(source, at) {
        let c = &source[s..e];
        if in_cont {
            cont_depth += 1;
            if cont_depth > limits.max_config_depth {
                return Err(Error::resource_limit(format!(
                    "config logical line exceeds the {}-continuation depth cap",
                    limits.max_config_depth
                )));
            }
            in_cont = ends_with_continuation(c);
            if !in_cont {
                cont_depth = 0;
            }
            at = next;
            continue;
        }
        match first_non_ws(c) {
            None => {}
            Some(i) if c[i] == b'#' => {}
            Some(i) if c[i] == b'!' => {
                p.has_prop_only = true;
            }
            _ => {
                // An `export KEY=VALUE` line is an `env`-only construct: it must not
                // masquerade as a `properties` entry with `export` as the key and a
                // whitespace separator, or the two dialects would be conflated.
                let is_env_export =
                    parse_entry_env(source, s, e).is_some_and(|ent| ent.flags & F_EXPORT != 0);
                if is_env_export {
                    if ends_with_continuation(c) {
                        in_cont = true;
                        cont_depth = 1;
                        p.has_prop_only = true;
                    }
                    at = next;
                    continue;
                }
                match parse_entry_prop(source, s, e) {
                    Some(ent) => {
                        p.prop_entries += 1;
                        if ent.marker == b'=' {
                            p.has_strong_sep = true;
                        } else if ent.marker == b':' {
                            p.has_strong_sep = true;
                            p.has_prop_only = true;
                        } else {
                            // a whitespace separator
                            p.has_prop_only = true;
                        }
                        if !is_env_identifier(&source[ent.key.0..ent.key.1]) {
                            p.has_prop_only = true;
                        }
                        if contains_unicode_escape(c) {
                            p.has_prop_only = true;
                        }
                    }
                    None => p.all_prop = false,
                }
            }
        }
        if ends_with_continuation(c) {
            in_cont = true;
            cont_depth = 1;
            p.has_prop_only = true;
        }
        at = next;
    }

    Ok(p)
}

// ---------------------------------------------------------------------------
// Building the model
// ---------------------------------------------------------------------------

fn build_lines(source: &[u8], dialect: u8, limits: Limits) -> Result<Vec<CfgLine>> {
    let start = bom_len(source);
    let mut lines: Vec<CfgLine> = Vec::new();
    let mut at = start;
    let mut entries: u64 = 0;
    while let Some((s, e, t, next)) = physical_line(source, at) {
        // Merge a `properties` logical line across trailing-`\` continuations.
        let (end, term, cont, after) = if dialect == DIALECT_PROPERTIES {
            logical_line(source, s, e, t, next, limits)?
        } else {
            (e, t, false, next)
        };
        check_line_bytes(end - s, limits)?;
        let c = &source[s..end];
        let mut line = CfgLine {
            kind: L_BLANK,
            terminator: term,
            marker: 0,
            flags: if cont { F_CONTINUED } else { 0 },
            start: s as u64,
            end: end as u64,
            key_start: end as u64,
            key_end: end as u64,
            sep_start: end as u64,
            sep_end: end as u64,
            value_start: end as u64,
            value_end: end as u64,
        };
        match dialect {
            DIALECT_ENV => {
                if let Some(i) = first_non_ws(c) {
                    if c[i] == b'#' {
                        line.kind = L_COMMENT;
                        line.marker = b'#';
                    } else if let Some(ent) = parse_entry_env(source, s, end) {
                        fill_entry(&mut line, &ent);
                    } else {
                        return Err(corrupt("env line is not a comment or KEY=VALUE entry"));
                    }
                }
            }
            DIALECT_INI => {
                if let Some(i) = first_non_ws(c) {
                    if c[i] == b';' || c[i] == b'#' {
                        line.kind = L_COMMENT;
                        line.marker = c[i];
                    } else if is_ini_section(c, i) {
                        let (ns, ne) = ini_section_name(source, s, end, i);
                        line.kind = L_SECTION;
                        line.key_start = ns as u64;
                        line.key_end = ne as u64;
                    } else if let Some(ent) = parse_entry_ini(source, s, end) {
                        fill_entry(&mut line, &ent);
                    } else {
                        return Err(corrupt("INI line is not a comment, section, or entry"));
                    }
                }
            }
            _ => {
                // properties (continuations already merged into `end`)
                if let Some(i) = first_non_ws(c) {
                    if c[i] == b'#' || c[i] == b'!' {
                        line.kind = L_COMMENT;
                        line.marker = c[i];
                    } else if let Some(ent) = parse_entry_prop(source, s, end) {
                        fill_entry(&mut line, &ent);
                    } else {
                        return Err(corrupt("properties line is not a comment or entry"));
                    }
                }
            }
        }
        if line.kind == L_ENTRY {
            entries += 1;
            if entries > MAX_MODEL_ENTRIES || entries > limits.max_config_entries as u64 {
                return Err(Error::resource_limit(format!(
                    "config model exceeds the {}-entry cap",
                    limits.max_config_entries
                )));
            }
            if line.key_end - line.key_start > limits.max_config_key_bytes {
                return Err(Error::resource_limit(format!(
                    "config key exceeds the {}-byte cap",
                    limits.max_config_key_bytes
                )));
            }
            if line.value_end - line.value_start > limits.max_config_value_bytes {
                return Err(Error::resource_limit(format!(
                    "config value exceeds the {}-byte cap",
                    limits.max_config_value_bytes
                )));
            }
        }
        if lines.len() as u64 >= limits.max_config_nodes as u64 {
            return Err(Error::resource_limit(format!(
                "config source exceeds the {}-node cap",
                limits.max_config_nodes
            )));
        }
        lines.push(line);
        at = after;
    }
    Ok(lines)
}

fn fill_entry(line: &mut CfgLine, ent: &EntrySpans) {
    line.kind = L_ENTRY;
    line.marker = ent.marker;
    line.flags |= ent.flags;
    line.key_start = ent.key.0 as u64;
    line.key_end = ent.key.1 as u64;
    line.sep_start = ent.sep.0 as u64;
    line.sep_end = ent.sep.1 as u64;
    line.value_start = ent.value.0 as u64;
    line.value_end = ent.value.1 as u64;
}

/// Resolve one `properties` logical line: merge physical lines while each ends
/// with an odd number of backslashes. Returns `(content_end, terminator,
/// continued, next_at)` where `next_at` is the offset after the last consumed
/// physical line.
fn logical_line(
    source: &[u8],
    s: usize,
    e: usize,
    t: u8,
    next: usize,
    limits: Limits,
) -> Result<(usize, u8, bool, usize)> {
    let mut end = e;
    let mut term = t;
    let mut nxt = next;
    let mut cont = false;
    let mut phys: u32 = 1;
    loop {
        if !ends_with_continuation(&source[s..end]) {
            break;
        }
        let Some((_ns, ne, nt, nn)) = physical_line(source, nxt) else {
            // A trailing backslash with no following line is not a continuation.
            break;
        };
        cont = true;
        phys += 1;
        if phys > limits.max_config_depth {
            return Err(Error::resource_limit(format!(
                "config logical line exceeds the {}-continuation depth cap",
                limits.max_config_depth
            )));
        }
        end = ne;
        term = nt;
        nxt = nn;
    }
    Ok((end, term, cont, nxt))
}

// ---------------------------------------------------------------------------
// Physical lines
// ---------------------------------------------------------------------------

/// Read one physical line at `at`: returns `(content_start, content_end,
/// terminator, next_at)`. The content span excludes the terminator.
fn physical_line(b: &[u8], at: usize) -> Option<(usize, usize, u8, usize)> {
    if at >= b.len() {
        return None;
    }
    let s = at;
    let mut i = at;
    while i < b.len() && b[i] != b'\n' && b[i] != b'\r' {
        i += 1;
    }
    let e = i;
    let (term, next) = if i >= b.len() {
        (T_NONE, i)
    } else if b[i] == b'\r' {
        if b.get(i + 1) == Some(&b'\n') {
            (T_CRLF, i + 2)
        } else {
            (T_CR, i + 1)
        }
    } else {
        (T_LF, i + 1)
    };
    Some((s, e, term, next))
}

fn bom_len(b: &[u8]) -> usize {
    if b.starts_with(&[0xEF, 0xBB, 0xBF]) {
        3
    } else {
        0
    }
}

fn first_non_ws(c: &[u8]) -> Option<usize> {
    let mut i = 0;
    while i < c.len() && (c[i] == b' ' || c[i] == b'\t') {
        i += 1;
    }
    if i >= c.len() { None } else { Some(i) }
}

fn check_line_bytes(len: usize, limits: Limits) -> Result<()> {
    if len as u64 > limits.max_config_line_bytes {
        return Err(Error::resource_limit(format!(
            "config line exceeds the {}-byte cap",
            limits.max_config_line_bytes
        )));
    }
    Ok(())
}

/// Count trailing backslashes; a continuation is an **odd** count.
fn ends_with_continuation(c: &[u8]) -> bool {
    let mut n = 0usize;
    let mut i = c.len();
    while i > 0 && c[i - 1] == b'\\' {
        n += 1;
        i -= 1;
    }
    n % 2 == 1
}

fn contains_unicode_escape(c: &[u8]) -> bool {
    let mut i = 0;
    while i + 5 < c.len() {
        if c[i] == b'\\'
            && c[i + 1] == b'u'
            && c[i + 2..i + 6].iter().all(|b| b.is_ascii_hexdigit())
        {
            return true;
        }
        i += 1;
    }
    false
}

// ---------------------------------------------------------------------------
// Entry parsers
// ---------------------------------------------------------------------------

struct EntrySpans {
    key: (usize, usize),
    sep: (usize, usize),
    value: (usize, usize),
    flags: u8,
    marker: u8,
}

fn parse_entry_ini(source: &[u8], s: usize, e: usize) -> Option<EntrySpans> {
    let c = &source[s..e];
    let mut i = 0;
    while i < c.len() && (c[i] == b' ' || c[i] == b'\t') {
        i += 1;
    }
    if i >= c.len() || c[i] == b'[' {
        return None;
    }
    let ks = i;
    let mut j = i;
    while j < c.len() && c[j] != b'=' && c[j] != b':' {
        j += 1;
    }
    if j >= c.len() {
        return None;
    }
    let mut ke = j;
    while ke > ks && (c[ke - 1] == b' ' || c[ke - 1] == b'\t') {
        ke -= 1;
    }
    if ke == ks {
        return None;
    }
    let marker = c[j];
    let ss = j;
    let mut se = j + 1;
    while se < c.len() && (c[se] == b' ' || c[se] == b'\t') {
        se += 1;
    }
    let vs = se;
    let mut ve = c.len();
    while ve > vs && (c[ve - 1] == b' ' || c[ve - 1] == b'\t') {
        ve -= 1;
    }
    let mut flags = 0u8;
    if ve > vs && (c[vs] == b'"' || c[vs] == b'\'') {
        let q = c[vs];
        if ve > vs + 1 && c[ve - 1] == q {
            flags |= if q == b'"' {
                F_DOUBLE_QUOTED
            } else {
                F_SINGLE_QUOTED
            };
        }
    }
    if flags == 0 {
        // Unquoted: an inline comment is introduced by whitespace + `;`.
        let mut k = vs;
        while k < ve {
            if c[k] == b';' && k > vs && (c[k - 1] == b' ' || c[k - 1] == b'\t') {
                let mut cut = k;
                while cut > vs && (c[cut - 1] == b' ' || c[cut - 1] == b'\t') {
                    cut -= 1;
                }
                ve = cut;
                flags |= F_INLINE_COMMENT;
                break;
            }
            k += 1;
        }
    }
    Some(EntrySpans {
        key: (s + ks, s + ke),
        sep: (s + ss, s + se),
        value: (s + vs, s + ve),
        flags,
        marker,
    })
}

fn parse_entry_env(source: &[u8], s: usize, e: usize) -> Option<EntrySpans> {
    let c = &source[s..e];
    let mut i = 0;
    while i < c.len() && (c[i] == b' ' || c[i] == b'\t') {
        i += 1;
    }
    let mut flags = 0u8;
    if c[i..].starts_with(b"export") {
        let after = i + 6;
        if after < c.len() && (c[after] == b' ' || c[after] == b'\t') {
            flags |= F_EXPORT;
            i = after;
            while i < c.len() && (c[i] == b' ' || c[i] == b'\t') {
                i += 1;
            }
        } else {
            return None;
        }
    }
    let ks = i;
    if i >= c.len() || !(c[i].is_ascii_alphabetic() || c[i] == b'_') {
        return None;
    }
    i += 1;
    while i < c.len() && (c[i].is_ascii_alphanumeric() || c[i] == b'_') {
        i += 1;
    }
    let ke = i;
    if i >= c.len() || c[i] != b'=' {
        return None;
    }
    let ss = i;
    let se = i + 1;
    let vs = i + 1;
    let mut ve = c.len();
    while ve > vs && (c[ve - 1] == b' ' || c[ve - 1] == b'\t') {
        ve -= 1;
    }
    if ve > vs && (c[vs] == b'"' || c[vs] == b'\'') {
        let q = c[vs];
        if ve > vs + 1 && c[ve - 1] == q {
            flags |= if q == b'"' {
                F_DOUBLE_QUOTED
            } else {
                F_SINGLE_QUOTED
            };
        }
    }
    Some(EntrySpans {
        key: (s + ks, s + ke),
        sep: (s + ss, s + se),
        value: (s + vs, s + ve),
        flags,
        marker: b'=',
    })
}

fn parse_entry_prop(source: &[u8], s: usize, e: usize) -> Option<EntrySpans> {
    let c = &source[s..e];
    let mut i = 0;
    while i < c.len() && (c[i] == b' ' || c[i] == b'\t') {
        i += 1;
    }
    let ks = i;
    let mut j = i;
    while j < c.len() {
        let b = c[j];
        if b == b'\\' {
            j += 1;
            if j < c.len() {
                if c[j] == b'u' {
                    j += 1;
                    let mut h = 0;
                    while h < 4 && j < c.len() && c[j].is_ascii_hexdigit() {
                        j += 1;
                        h += 1;
                    }
                } else {
                    j += 1;
                }
            }
            continue;
        }
        if is_prop_key_byte(b) {
            j += 1;
            continue;
        }
        break;
    }
    let ke = j;
    if ke == ks || j >= c.len() {
        return None;
    }
    let marker = c[j];
    if marker != b'=' && marker != b':' && marker != b' ' && marker != b'\t' {
        return None;
    }
    let ss = j;
    let mut se = j;
    if marker == b'=' || marker == b':' {
        se = j + 1;
    }
    while se < c.len() && (c[se] == b' ' || c[se] == b'\t') {
        se += 1;
    }
    let vs = se;
    let mut ve = c.len();
    while ve > vs && (c[ve - 1] == b' ' || c[ve - 1] == b'\t') {
        ve -= 1;
    }
    Some(EntrySpans {
        key: (s + ks, s + ke),
        sep: (s + ss, s + se),
        value: (s + vs, s + ve),
        flags: 0,
        marker,
    })
}

fn is_prop_key_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-')
}

fn is_env_identifier(k: &[u8]) -> bool {
    if k.is_empty() {
        return false;
    }
    let first = k[0];
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return false;
    }
    k[1..]
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

fn is_ini_section(c: &[u8], i: usize) -> bool {
    if c[i] != b'[' {
        return false;
    }
    let mut j = i + 1;
    while j < c.len() && c[j] != b']' {
        if c[j] == b'[' {
            return false;
        }
        j += 1;
    }
    if j >= c.len() || j <= i + 1 {
        return false;
    }
    // only trailing whitespace may follow `]`
    let mut k = j + 1;
    while k < c.len() && (c[k] == b' ' || c[k] == b'\t') {
        k += 1;
    }
    k == c.len()
}

fn ini_section_name(source: &[u8], s: usize, e: usize, i: usize) -> (usize, usize) {
    let c = &source[s..e];
    let mut j = i + 1;
    while j < c.len() && c[j] != b']' {
        j += 1;
    }
    (s + i + 1, s + j)
}

// ---------------------------------------------------------------------------
// Token access and decoding
// ---------------------------------------------------------------------------

fn slice(source: &[u8], start: u64, end: u64) -> Result<&[u8]> {
    let s = usize::try_from(start).map_err(|_| corrupt("span overflow"))?;
    let e = usize::try_from(end).map_err(|_| corrupt("span overflow"))?;
    source
        .get(s..e)
        .ok_or_else(|| corrupt("span is outside the source"))
}

/// The exact source bytes of a line's content.
pub fn line_bytes<'a>(source: &'a [u8], line: &CfgLine) -> Result<&'a [u8]> {
    slice(source, line.start, line.end)
}

/// The exact source bytes of a line's key token (or section name).
pub fn key_bytes<'a>(source: &'a [u8], line: &CfgLine) -> Result<&'a [u8]> {
    slice(source, line.key_start, line.key_end)
}

/// The exact source bytes of a line's value token (quotes included when quoted).
pub fn value_bytes<'a>(source: &'a [u8], line: &CfgLine) -> Result<&'a [u8]> {
    slice(source, line.value_start, line.value_end)
}

/// The exact source bytes of a line's separator token.
pub fn sep_bytes<'a>(source: &'a [u8], line: &CfgLine) -> Result<&'a [u8]> {
    slice(source, line.sep_start, line.sep_end)
}

/// Decode a key token: strip a surrounding pair of matching quotes if present;
/// otherwise the token bytes interpreted lossily as UTF-8.
pub fn decode_key(source: &[u8], line: &CfgLine) -> Result<String> {
    let tok = key_bytes(source, line)?;
    if tok.len() >= 2 && (tok[0] == b'"' || tok[0] == b'\'') && tok[tok.len() - 1] == tok[0] {
        return Ok(String::from_utf8_lossy(&tok[1..tok.len() - 1]).into_owned());
    }
    Ok(String::from_utf8_lossy(tok).into_owned())
}

/// Decode a value token for a dialect.
///
/// * `env` — a quoted value drops its outer quotes and its escape sequences are
///   resolved (`\\`, `\"`, `\'`, `\n`, `\t`, `\r`); an unquoted value is returned
///   verbatim.
/// * `ini` — a value wrapped in a matching pair of `'`/`"` drops the quotes
///   (no escape interpretation); otherwise verbatim.
/// * `properties` — trailing-`\` continuations are joined (the backslash, the
///   embedded newline, and the continuation's leading whitespace are removed),
///   but **every escape is preserved as its spelling** (`\uXXXX` is never
///   expanded). The exact bytes remain authoritative.
pub fn decode_value(dialect: u8, source: &[u8], line: &CfgLine) -> Result<String> {
    let tok = value_bytes(source, line)?;
    match dialect {
        DIALECT_ENV => {
            if line.flags & F_DOUBLE_QUOTED != 0 && tok.len() >= 2 {
                let inner = &tok[1..tok.len() - 1];
                Ok(String::from_utf8_lossy(&unescape_env(inner)).into_owned())
            } else if line.flags & F_SINGLE_QUOTED != 0 && tok.len() >= 2 {
                let inner = &tok[1..tok.len() - 1];
                Ok(String::from_utf8_lossy(inner).into_owned())
            } else {
                Ok(String::from_utf8_lossy(tok).into_owned())
            }
        }
        DIALECT_INI => {
            if tok.len() >= 2 && (tok[0] == b'"' || tok[0] == b'\'') && tok[tok.len() - 1] == tok[0]
            {
                Ok(String::from_utf8_lossy(&tok[1..tok.len() - 1]).into_owned())
            } else {
                Ok(String::from_utf8_lossy(tok).into_owned())
            }
        }
        _ => {
            let mut out: Vec<u8> = Vec::with_capacity(tok.len());
            let mut i = 0usize;
            while i < tok.len() {
                if tok[i] == b'\\' {
                    // Join a `\`+newline continuation, dropping the continuation's
                    // leading whitespace.
                    if let Some((nl, skip)) = continuation_at(tok, i) {
                        i = nl + skip;
                        while i < tok.len() && (tok[i] == b' ' || tok[i] == b'\t') {
                            i += 1;
                        }
                        continue;
                    }
                    // Otherwise copy the escape spelling verbatim.
                    out.push(b'\\');
                    i += 1;
                } else {
                    out.push(tok[i]);
                    i += 1;
                }
            }
            Ok(String::from_utf8_lossy(&out).into_owned())
        }
    }
}

/// If a `\` at `i` is immediately followed by a line terminator, return
/// `(newline_start, newline_len)`; else `None`.
fn continuation_at(tok: &[u8], i: usize) -> Option<(usize, usize)> {
    match tok.get(i + 1) {
        Some(b'\n') => Some((i + 1, 1)),
        Some(b'\r') => {
            if tok.get(i + 2) == Some(&b'\n') {
                Some((i + 1, 2))
            } else {
                Some((i + 1, 1))
            }
        }
        _ => None,
    }
}

fn unescape_env(inner: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(inner.len());
    let mut i = 0usize;
    while i < inner.len() {
        let b = inner[i];
        if b == b'\\' {
            match inner.get(i + 1) {
                Some(b'\\') => {
                    out.push(b'\\');
                    i += 2;
                }
                Some(b'"') => {
                    out.push(b'"');
                    i += 2;
                }
                Some(b'\'') => {
                    out.push(b'\'');
                    i += 2;
                }
                Some(b'n') => {
                    out.push(b'\n');
                    i += 2;
                }
                Some(b't') => {
                    out.push(b'\t');
                    i += 2;
                }
                Some(b'r') => {
                    out.push(b'\r');
                    i += 2;
                }
                Some(&c) => {
                    out.push(b'\\');
                    out.push(c);
                    i += 2;
                }
                None => {
                    out.push(b'\\');
                    i += 1;
                }
            }
        } else {
            out.push(b);
            i += 1;
        }
    }
    out
}

/// Render a deterministic canonical text projection: every line's exact bytes,
/// terminators normalized to `\n`, a trailing `\n` when the source ended with a
/// terminator. Declines typed if the text would exceed `max_out` bytes.
pub fn canonical_text(model: &ConfigModel, source: &[u8], max_out: u64) -> Result<String> {
    let mut out = String::new();
    for (i, line) in model.lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&String::from_utf8_lossy(line_bytes(source, line)?));
        if out.len() as u64 > max_out {
            return Err(Error::resource_limit(format!(
                "config text projection exceeds the {max_out}-byte budget"
            )));
        }
    }
    if model.trailing_terminator {
        out.push('\n');
    }
    Ok(out)
}

/// A bounded, case-sensitive lexical search over decoded keys and values.
/// Returns matches in source order. Declines typed if the match list would
/// exceed `max_out` bytes (an approximate bound).
pub fn find(
    model: &ConfigModel,
    source: &[u8],
    pattern: &str,
    max_out: u64,
) -> Result<Vec<CfgMatch>> {
    let mut out: Vec<CfgMatch> = Vec::new();
    let mut estimated: u64 = 0;
    let mut entry: u32 = 0;
    for (i, line) in model.lines.iter().enumerate() {
        if line.kind != L_ENTRY {
            continue;
        }
        let key = decode_key(source, line)?;
        let value = decode_value(model.dialect, source, line)?;
        if key.contains(pattern) {
            estimated = estimated.saturating_add(48 + key.len() as u64);
            if estimated > max_out {
                return Err(Error::resource_limit(format!(
                    "config find exceeded the {max_out}-byte budget"
                )));
            }
            out.push(CfgMatch {
                line: i as u32,
                entry,
                role: ROLE_KEY,
                start: line.key_start,
                end: line.key_end,
                text: key.clone(),
            });
        }
        if value.contains(pattern) {
            estimated = estimated.saturating_add(48 + value.len() as u64);
            if estimated > max_out {
                return Err(Error::resource_limit(format!(
                    "config find exceeded the {max_out}-byte budget"
                )));
            }
            out.push(CfgMatch {
                line: i as u32,
                entry,
                role: ROLE_VALUE,
                start: line.value_start,
                end: line.value_end,
                text: value,
            });
        }
        entry += 1;
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn corrupt(msg: &str) -> Error {
    Error::invalid_config_structure(format!("malformed config: {msg}"))
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
            .ok_or_else(|| corrupt("reader overflow"))?;
        let s = self
            .b
            .get(self.at..end)
            .ok_or_else(|| corrupt("reader underrun"))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn dialect(source: &[u8]) -> u8 {
        classify(source, Limits::DEFAULT).unwrap()
    }

    #[test]
    fn detects_the_three_dialects() {
        // INI needs a section header.
        let ini = b"[db]\nhost = localhost\nport = 5432\n";
        assert_eq!(dialect(ini), DIALECT_INI);
        // env needs an `export` signal.
        let env = b"# app\nFOO=bar\nexport BAZ=\"a b\"\n";
        assert_eq!(dialect(env), DIALECT_ENV);
        // properties needs a properties-only signal (`:` separator).
        let prop = b"# note\napp.name: myapp\napp.port=8080\n";
        assert_eq!(dialect(prop), DIALECT_PROPERTIES);
    }

    #[test]
    fn pure_env_properties_overlap_stays_opaque() {
        // `KEY=VALUE`, `=` only, `#`-only comments, identifier keys: the exact
        // shape shared by env and Java properties. Never guessed.
        let overlap = b"FOO=bar\nBAZ=qux\n";
        assert!(!detect(overlap, Limits::DEFAULT));
        assert!(classify(overlap, Limits::DEFAULT).is_err());
    }

    #[test]
    fn prose_and_json_and_csv_are_not_config() {
        assert!(!detect(
            b"The quick brown fox jumps over the lazy dog.\nPlain prose, not config.\n",
            Limits::DEFAULT
        ));
        assert!(!detect(b"{\"a\":1,\"b\":2}", Limits::DEFAULT));
        assert!(!detect(b"a,b\nc,d\n", Limits::DEFAULT));
        // A shebang is a script.
        assert!(!detect(b"#!/bin/sh\nexport FOO=bar\n", Limits::DEFAULT));
    }

    #[test]
    fn ini_preserves_spans_and_inline_comments() {
        let src = b"[db]\nhost = localhost ; the host\nport: 5432\n";
        let m = parse(src, Limits::DEFAULT, true).unwrap();
        assert_eq!(m.dialect, DIALECT_INI);
        assert_eq!(m.lines.len(), 3);
        assert_eq!(m.lines[0].kind, L_SECTION);
        assert_eq!(key_bytes(src, &m.lines[0]).unwrap(), b"db");
        assert_eq!(m.lines[1].kind, L_ENTRY);
        assert_eq!(key_bytes(src, &m.lines[1]).unwrap(), b"host");
        assert_eq!(value_bytes(src, &m.lines[1]).unwrap(), b"localhost");
        assert!(m.lines[1].flags & F_INLINE_COMMENT != 0);
        assert_eq!(m.lines[2].marker, b':');
    }

    #[test]
    fn env_preserves_export_quoting_and_order() {
        let src = b"# app\nexport FOO=\"a b\"\nBAR='x'\nEMPTY=\n";
        let m = parse(src, Limits::DEFAULT, true).unwrap();
        assert_eq!(m.dialect, DIALECT_ENV);
        assert_eq!(m.lines[0].kind, L_COMMENT);
        let foo = &m.lines[1];
        assert!(foo.flags & F_EXPORT != 0);
        assert!(foo.flags & F_DOUBLE_QUOTED != 0);
        assert_eq!(value_bytes(src, foo).unwrap(), b"\"a b\"");
        assert_eq!(decode_value(m.dialect, src, foo).unwrap(), "a b");
        assert_eq!(decode_value(m.dialect, src, &m.lines[2]).unwrap(), "x");
        assert!(value_bytes(src, &m.lines[3]).unwrap().is_empty());
    }

    #[test]
    fn properties_preserves_continuation_and_unicode_spelling() {
        let src = b"multi=one\\\n  two\\\n  three\nunicode=gr\\u00FCn\ncolon: v\n";
        let m = parse(src, Limits::DEFAULT, true).unwrap();
        assert_eq!(m.dialect, DIALECT_PROPERTIES);
        let multi = &m.lines[0];
        assert!(multi.flags & F_CONTINUED != 0);
        // The value span holds the raw continuation bytes (backslashes + newlines).
        assert_eq!(value_bytes(src, multi).unwrap(), b"one\\\n  two\\\n  three");
        assert_eq!(decode_value(m.dialect, src, multi).unwrap(), "onetwothree");
        // `\uXXXX` is preserved as spelling, never expanded.
        assert_eq!(
            decode_value(m.dialect, src, &m.lines[1]).unwrap(),
            "gr\\u00FCn"
        );
        assert_eq!(m.lines[2].marker, b':');
    }

    #[test]
    fn duplicate_keys_are_preserved() {
        let src = b"[a]\nk = 1\nk = 2\n";
        let m = parse(src, Limits::DEFAULT, true).unwrap();
        assert_eq!(m.entry_count(), 2);
        let a = m.nth_entry(0).unwrap();
        let b = m.nth_entry(1).unwrap();
        assert_eq!(decode_key(src, a).unwrap(), "k");
        assert_eq!(decode_key(src, b).unwrap(), "k");
        assert_ne!(a.value_start, b.value_start);
    }

    #[test]
    fn model_roundtrips_and_fails_closed() {
        let src = b"[db]\nhost = localhost\n";
        let m = parse(src, Limits::DEFAULT, true).unwrap();
        let enc = m.encode();
        assert_eq!(ConfigModel::decode(&enc).unwrap(), m);
        let mut bad = enc.clone();
        bad[0] = b'X';
        assert!(ConfigModel::decode(&bad).is_err());
        let mut truncated = enc;
        truncated.truncate(truncated.len() - 1);
        assert!(ConfigModel::decode(&truncated).is_err());
    }

    #[test]
    fn find_reports_keys_and_values() {
        let src = b"[db]\nhost = localhost\n";
        let m = parse(src, Limits::DEFAULT, true).unwrap();
        let hits = find(&m, src, "host", 1 << 20).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].role, ROLE_KEY);
        assert_eq!(hits[1].role, ROLE_VALUE);
    }

    #[test]
    fn caps_decline_typed() {
        let src = b"[db]\nhost = localhost\nport: 5432\n";
        // A line cap below the physical line count declines typed.
        let tight = Limits {
            max_config_lines: 2,
            ..Limits::DEFAULT
        };
        let e = parse(src, tight, true).unwrap_err();
        assert_eq!(e.class(), crate::error::ErrorClass::ResourceLimit);
        // A value-bytes cap below the value length declines typed.
        let tight = Limits {
            max_config_value_bytes: 2,
            ..Limits::DEFAULT
        };
        let e = parse(src, tight, true).unwrap_err();
        assert_eq!(e.class(), crate::error::ErrorClass::ResourceLimit);
        // A continuation-depth cap declines typed.
        let deep = b"k=a\\\n b\\\n c\n";
        let tight = Limits {
            max_config_depth: 1,
            ..Limits::DEFAULT
        };
        assert!(parse(deep, tight, true).is_err());
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
            let _ = detect(&buf, Limits::DEFAULT);
            if let Ok(m) = parse(&buf, Limits::STRICT, true) {
                let _ = canonical_text(&m, &buf, 1 << 20);
                let _ = build_config_model(&buf, Limits::STRICT);
            }
        }
    }
}
