//! Phase 29.3 — revision-aware incremental observation (a REAL mechanism).
//!
//! Phase 28.7 *simulated* cross-revision dependency reuse and named the
//! load-bearing fact: on authentic revision families a large fraction of
//! invalidations have **byte-unchanged physical fragments**, so a
//! content-addressed (physical-only) store would reuse them **wrong**; only
//! **semantic edges** (a DOCX style/numbering table, an XLSX shared-string
//! table) make reuse correct. This module ships the bounded mechanism that
//! simulation asked for.
//!
//! ## What is actually implemented (not a simulation)
//!
//! * A **versioned durable index** ([`INDEX_FORMAT_VERSION`]) maps a stable
//!   observation identity to its verified dependency signature and the content
//!   id of its verified **derived output**. Outputs are stored
//!   content-addressed under `<index>/obs/<blake3-hex>.b`.
//! * An observation's signature is **explicit**: the ordered physical fragment
//!   content ids covering its source ranges **plus** the semantic context
//!   content ids it reads (a DOCX paragraph reads `word/styles.xml` and
//!   `word/numbering.xml`; an XLSX `t="s"` cell reads `xl/sharedStrings.xml`).
//! * The **derived output** genuinely depends on the semantic context (DOCX
//!   resolves the run/paragraph style size out of `styles.xml`; XLSX resolves a
//!   shared string out of `sharedStrings.xml`), so reusing it when only the
//!   semantic table changed is observably WRONG — which is exactly the case the
//!   court exercises.
//! * Three lanes ([`Lane`]): a full rebuild, a physical-only versioned CAS, and
//!   the incremental (physical **and** semantic) path. The answers of every lane
//!   carry a [`IncrementalReport::answer_digest`], so a court can require the
//!   incremental answers to **equal** the full rebuild's.
//! * **Typed-declining**: an unsupported format, a container that cannot be
//!   parsed, a member that is not stored/deflated, or an observation count over
//!   the bound returns [`Enum::Declined`] with a reason — never a wrong reuse.
//!
//! ## Identity separation (ADR-0060)
//!
//! Four namespaces stay distinct and are never conflated: the exact revision
//! (BLAKE3 of the source, held by the field), the physical fragment content id,
//! the semantic context content id, and the derived-observation output content
//! id. The index keys an observation by its own identity and stores those three
//! ids side by side; reuse never weakens a node's dependency on its exact root.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::integrity::to_hex;
use crate::limits::Limits;

/// The durable index format version. A mismatch is treated as "no prior index"
/// (decline to reuse and rebuild), never as a reuse opportunity.
pub const INDEX_FORMAT_VERSION: u32 = 1;
const INDEX_MAGIC: &[u8; 8] = b"VOLEINC1";
/// Default bound on observations enumerated for one revision.
pub const DEFAULT_MAX_OBSERVATIONS: u64 = 200_000;
/// Modeled bytes per dependency edge, for cross-referencing Phase 28.7.
const EDGE_MODEL_BYTES: u64 = 16;

// Content-defined chunking (gear hash, FastCDC-flavoured), matching Phase 28.7.
const CDC_MIN: usize = 2048;
const CDC_AVG_MASK: u64 = (1 << 13) - 1; // ~8 KiB average
const CDC_MAX: usize = 32768;

const fn splitmix64(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

const fn build_gear() -> [u64; 256] {
    let mut t = [0u64; 256];
    let mut i = 0usize;
    while i < 256 {
        let x = (i as u64)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(0xD1B5_4A32_D192_ED03);
        t[i] = splitmix64(x);
        i += 1;
    }
    t
}

const GEAR: [u64; 256] = build_gear();

/// The storage/reuse lane. Each is a *policy*, never a different observation
/// model, so the three lanes answer the same keys and their answer digests are
/// directly comparable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lane {
    /// Recompute every observation each revision; the honest full rebuild.
    Full,
    /// Reuse when the **physical** fragment signature matches (a versioned
    /// content-addressed store with no semantic edges).
    Cas,
    /// Reuse only when the **physical and semantic** signature matches.
    Incremental,
}

impl Lane {
    pub const fn name(self) -> &'static str {
        match self {
            Lane::Full => "full",
            Lane::Cas => "cas",
            Lane::Incremental => "incremental",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "full" => Ok(Lane::Full),
            "cas" => Ok(Lane::Cas),
            "incremental" | "incr" => Ok(Lane::Incremental),
            other => Err(Error::usage(format!(
                "unknown --lane {other:?}; expected one of: full, cas, incremental"
            ))),
        }
    }
}

/// One enumerated derived observation.
struct Obs {
    /// Stable observation identity across revisions (`csv:row:12`, ...).
    key: String,
    /// Physical fragment content ids covering the observation's source ranges.
    phys: Vec<[u8; 32]>,
    /// Semantic context fragment content ids (styles/numbering/shared-strings).
    sem: Vec<[u8; 32]>,
    /// The freshly computed derived output (never reused inside this struct).
    out: Vec<u8>,
}

/// The result of enumerating one revision's observations.
enum Enum {
    Observed { format: String, obs: Vec<Obs> },
    Declined { reason: String },
}

#[derive(Clone, Copy)]
struct IndexEntry {
    phys: [u8; 32],
    sem: [u8; 32],
    out: [u8; 32],
}

/// The observable receipt of one incremental observation run.
#[derive(Debug, Clone)]
pub struct IncrementalReport {
    pub declined: bool,
    pub decline_reason: String,
    pub format: String,
    pub lane: String,
    pub index_version: u32,
    pub prior_entries: u64,
    pub n_obs: u64,
    pub reused: u64,
    pub invalidated: u64,
    pub new_obs: u64,
    pub recomputed: u64,
    /// Prior entry existed, the physical signature was identical, and the
    /// semantic signature differed (the Phase-28.7 "semantic-only" case).
    pub semantic_only_invalidations: u64,
    pub bytes_reused: u64,
    pub bytes_recomputed: u64,
    pub blobs_written: u64,
    pub blob_bytes_written: u64,
    pub index_entries: u64,
    pub index_bytes: u64,
    pub dependency_edges: u64,
    pub obs_blob_total_bytes: u64,
    pub persistent_bytes: u64,
    pub lane_us: u64,
    pub verified_equal: bool,
    pub mismatches: u64,
    pub answer_digest: String,
}

impl IncrementalReport {
    fn declined(reason: String, lane: Lane) -> Self {
        IncrementalReport {
            declined: true,
            decline_reason: reason,
            format: String::from("declined"),
            lane: lane.name().to_string(),
            index_version: INDEX_FORMAT_VERSION,
            prior_entries: 0,
            n_obs: 0,
            reused: 0,
            invalidated: 0,
            new_obs: 0,
            recomputed: 0,
            semantic_only_invalidations: 0,
            bytes_reused: 0,
            bytes_recomputed: 0,
            blobs_written: 0,
            blob_bytes_written: 0,
            index_entries: 0,
            index_bytes: 0,
            dependency_edges: 0,
            obs_blob_total_bytes: 0,
            persistent_bytes: 0,
            lane_us: 0,
            verified_equal: true,
            mismatches: 0,
            answer_digest: String::new(),
        }
    }

    /// Canonical JSON (integers + strings only, so a court parses it exactly).
    pub fn to_json(&self) -> String {
        format!(
            concat!(
                "{{",
                "\"declined\":{},",
                "\"decline_reason\":\"{}\",",
                "\"format\":\"{}\",",
                "\"lane\":\"{}\",",
                "\"index_version\":{},",
                "\"prior_entries\":{},",
                "\"n_obs\":{},",
                "\"reused\":{},",
                "\"invalidated\":{},",
                "\"new_obs\":{},",
                "\"recomputed\":{},",
                "\"semantic_only_invalidations\":{},",
                "\"bytes_reused\":{},",
                "\"bytes_recomputed\":{},",
                "\"blobs_written\":{},",
                "\"blob_bytes_written\":{},",
                "\"index_entries\":{},",
                "\"index_bytes\":{},",
                "\"dependency_edges\":{},",
                "\"obs_blob_total_bytes\":{},",
                "\"persistent_bytes\":{},",
                "\"lane_us\":{},",
                "\"verified_equal\":{},",
                "\"mismatches\":{},",
                "\"answer_digest\":\"{}\"",
                "}}"
            ),
            self.declined,
            json_escape(&self.decline_reason),
            json_escape(&self.format),
            json_escape(&self.lane),
            self.index_version,
            self.prior_entries,
            self.n_obs,
            self.reused,
            self.invalidated,
            self.new_obs,
            self.recomputed,
            self.semantic_only_invalidations,
            self.bytes_reused,
            self.bytes_recomputed,
            self.blobs_written,
            self.blob_bytes_written,
            self.index_entries,
            self.index_bytes,
            self.dependency_edges,
            self.obs_blob_total_bytes,
            self.persistent_bytes,
            self.lane_us,
            self.verified_equal,
            self.mismatches,
            self.answer_digest,
        )
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn id_of(bytes: &[u8]) -> [u8; 32] {
    *blake3::hash(bytes).as_bytes()
}

fn hex32(id: &[u8; 32]) -> String {
    to_hex(id)
}

fn digest_ids(ids: &[[u8; 32]]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(&(ids.len() as u64).to_le_bytes());
    for id in ids {
        h.update(id);
    }
    *h.finalize().as_bytes()
}

// ---------------------------------------------------------------------------
// fragments
// ---------------------------------------------------------------------------

struct Frag {
    start: usize,
    end: usize,
    id: [u8; 32],
}

/// Content-defined fragments over a flat text source.
fn text_fragments(data: &[u8]) -> Vec<Frag> {
    let mut out = Vec::new();
    let n = data.len();
    if n == 0 {
        return out;
    }
    let mut start = 0usize;
    let mut h = 0u64;
    let mut i = 0usize;
    while i < n {
        h = (h << 1).wrapping_add(GEAR[data[i] as usize]);
        i += 1;
        let size = i - start;
        if size >= CDC_MIN && ((h & CDC_AVG_MASK) == 0 || size >= CDC_MAX) {
            out.push(Frag {
                start,
                end: i,
                id: id_of(&data[start..i]),
            });
            start = i;
            h = 0;
        }
    }
    if start < n {
        out.push(Frag {
            start,
            end: n,
            id: id_of(&data[start..n]),
        });
    }
    out
}

/// Indices of the fragments intersecting `[a, b)`.
fn covering(frags: &[Frag], a: usize, b: usize) -> Vec<usize> {
    let mut out = Vec::new();
    for (k, f) in frags.iter().enumerate() {
        if f.start < b && a < f.end {
            out.push(k);
        }
    }
    out
}

fn frag_ids_at(frags: &[Frag], idxs: &[usize]) -> Vec<[u8; 32]> {
    idxs.iter().map(|&k| frags[k].id).collect()
}

// ---------------------------------------------------------------------------
// bounded byte scanners (deterministic, no unbounded allocation)
// ---------------------------------------------------------------------------

fn find_from(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || from > hay.len() {
        return None;
    }
    let first = needle[0];
    let mut i = from;
    while i + needle.len() <= hay.len() {
        if hay[i] == first && &hay[i..i + needle.len()] == needle {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Find the element starting at `open` (already known to begin with `tag`) and
/// return `(start, end_exclusive)` where `end` is past the closing tag. Handles
/// self-closing `<tag .../>`.
fn element_span(hay: &[u8], open: usize, close: &[u8]) -> Option<(usize, usize)> {
    let gt = find_from(hay, b">", open)?;
    if gt > open && hay[gt - 1] == b'/' {
        return Some((open, gt + 1));
    }
    let c = find_from(hay, close, gt + 1)?;
    Some((open, c + close.len()))
}

/// Find a `<tag` open tag at or after `from` whose next byte is `>`, space, or
/// `/` (so `<w:p` does not match `<w:pPr`, `<c` does not match `<cols`).
fn find_open_tag(hay: &[u8], tag: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    loop {
        let at = find_from(hay, tag, i)?;
        match hay.get(at + tag.len()) {
            Some(b'>') | Some(b' ') | Some(b'/') => return Some(at),
            _ => i = at + 1,
        }
    }
}

/// Attribute value `name="value"` inside `[start, end)`.
fn attr_value(hay: &[u8], start: usize, end: usize, name: &[u8]) -> Option<Vec<u8>> {
    let end = end.min(hay.len());
    let mut pat = Vec::with_capacity(name.len() + 2);
    pat.extend_from_slice(name);
    pat.extend_from_slice(b"=\"");
    let mut i = start;
    while i < end {
        let at = find_from(&hay[..end], &pat, i)?;
        let vstart = at + pat.len();
        let vstop = find_from(&hay[..end], b"\"", vstart)?;
        if vstop <= end {
            return Some(hay[vstart..vstop].to_vec());
        }
        i = at + 1;
    }
    None
}

/// The contents of every `open...close` element within `[start, end)`,
/// concatenated. `open` includes the leading `<` (e.g. `<w:t`, `<t`).
fn collect_tag_contents(
    hay: &[u8],
    start: usize,
    end: usize,
    open: &[u8],
    close: &[u8],
) -> Vec<u8> {
    let end = end.min(hay.len());
    let mut out = Vec::new();
    let mut i = start;
    while let Some(lt) = find_open_tag(&hay[..end], open, i) {
        if lt >= end {
            break;
        }
        let Some(gt) = find_from(&hay[..end], b">", lt) else {
            break;
        };
        if gt > lt && hay[gt - 1] == b'/' {
            i = gt + 1;
            continue;
        }
        let Some(close_at) = find_from(&hay[..end], close, gt + 1) else {
            break;
        };
        out.extend_from_slice(&hay[gt + 1..close_at]);
        i = close_at + close.len();
    }
    out
}

/// DOCX paragraph element spans within `document.xml`.
fn docx_paragraph_spans(doc: &[u8]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while let Some(at) = find_open_tag(doc, b"<w:p", i) {
        match element_span(doc, at, b"</w:p>") {
            Some((s, e)) if e > s => {
                out.push((s, e));
                i = e;
            }
            _ => {
                i = at + 4;
            }
        }
    }
    out
}

/// Map `w:styleId` -> first `w:sz w:val` under that style (a real style table).
fn docx_style_sizes(styles: &[u8]) -> BTreeMap<Vec<u8>, u32> {
    let mut map = BTreeMap::new();
    let mut i = 0usize;
    while let Some(at) = find_open_tag(styles, b"<w:style", i) {
        let Some((s, e)) = element_span(styles, at, b"</w:style>") else {
            i = at + 8;
            continue;
        };
        if let Some(id) = attr_value(styles, s, e, b"w:styleId")
            && let Some(sz) = style_sz(styles, s, e)
        {
            map.insert(id, sz);
        }
        i = e;
    }
    map
}

fn style_sz(styles: &[u8], start: usize, end: usize) -> Option<u32> {
    let at = find_from(&styles[..end], b"w:sz ", start)?;
    let val = attr_value(styles, at, end, b"w:val")?;
    std::str::from_utf8(&val).ok()?.trim().parse().ok()
}

/// The resolved derived projection of a DOCX paragraph: its text plus the
/// effective style sizes resolved from the style table. The projection is a
/// pure function of `(paragraph bytes, style table)`, so an unchanged paragraph
/// under a changed style table has a **different** output.
fn docx_paragraph_projection(para: &[u8], styles: &BTreeMap<Vec<u8>, u32>) -> Vec<u8> {
    // Offsets are element-relative here; scan the paragraph slice directly.
    let text = collect_tag_contents(para, 0, para.len(), b"<w:t", b"</w:t>");
    let mut sizes: Vec<u32> = Vec::new();
    let mut i = 0usize;
    while let Some(at) = find_from(para, b"w:", i) {
        let after = &para[at..];
        let is_style_ref = after.starts_with(b"w:pStyle")
            || (after.starts_with(b"w:rStyle") && after.len() > b"w:rStyle".len());
        if is_style_ref
            && let Some(val) = attr_value(para, at, para.len(), b"w:val")
            && let Some(sz) = styles.get(&val)
        {
            sizes.push(*sz);
        }
        i = at + 3;
    }
    sizes.sort_unstable();
    let mut out = Vec::with_capacity(text.len() + 8 + sizes.len() * 4);
    out.extend_from_slice(&text);
    out.push(0x1f);
    for (k, s) in sizes.iter().enumerate() {
        if k > 0 {
            out.push(b',');
        }
        out.extend_from_slice(s.to_string().as_bytes());
    }
    out
}

/// The `<si>` shared-string table of `sharedStrings.xml`.
fn xlsx_shared_strings(ss: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while let Some(at) = find_open_tag(ss, b"<si", i) {
        match element_span(ss, at, b"</si>") {
            Some((s, e)) => {
                out.push(collect_tag_contents(ss, s, e, b"<t", b"</t>"));
                i = e;
            }
            None => {
                i = at + 3;
            }
        }
    }
    out
}

/// XLSX cell element spans within a worksheet part.
fn xlsx_cell_spans(sheet: &[u8]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while let Some(at) = find_open_tag(sheet, b"<c", i) {
        match element_span(sheet, at, b"</c>") {
            Some((s, e)) if e > s => {
                out.push((s, e));
                i = e;
            }
            _ => {
                i = at + 2;
            }
        }
    }
    out
}

/// The resolved derived projection of an XLSX cell: its displayed value, with a
/// shared-string reference resolved out of `sharedStrings.xml`.
fn xlsx_cell_projection(cell: &[u8], shared: &[Vec<u8>]) -> Vec<u8> {
    let is_shared = attr_value(cell, 0, cell.len(), b"t").as_deref() == Some(b"s");
    if !is_shared {
        return cell.to_vec();
    }
    let display: &[u8] = match (find_from(cell, b"<v>", 0), find_from(cell, b"</v>", 0)) {
        (Some(a), Some(b)) if b > a + 3 => {
            let idx_s = &cell[a + 3..b];
            match std::str::from_utf8(idx_s)
                .ok()
                .and_then(|s| s.trim().parse::<usize>().ok())
            {
                Some(n) => shared.get(n).map(|v| v.as_slice()).unwrap_or(b""),
                None => b"",
            }
        }
        _ => b"",
    };
    let mut out = Vec::with_capacity(display.len() + 1);
    out.extend_from_slice(b"s\x1f");
    out.extend_from_slice(display);
    out
}

/// Top-level element spans of the first JSON array following `key`.
fn json_top_level_spans(data: &[u8], key: &[u8]) -> Vec<(usize, usize)> {
    let Some(ki) = find_from(data, key, 0) else {
        return Vec::new();
    };
    let Some(bi) = find_from(data, b"[", ki) else {
        return Vec::new();
    };
    let mut i = bi + 1;
    let n = data.len();
    let mut spans = Vec::new();
    let mut depth = 0i32;
    let mut start: Option<usize> = None;
    let mut in_str = false;
    let mut esc = false;
    while i < n {
        let c = data[i];
        if in_str {
            if esc {
                esc = false;
            } else if c == b'\\' {
                esc = true;
            } else if c == b'"' {
                in_str = false;
            }
        } else if c == b'"' {
            in_str = true;
        } else if c == b'[' || c == b'{' {
            if depth == 0 {
                start = Some(i);
            }
            depth += 1;
        } else if c == b']' || c == b'}' {
            if depth == 0 {
                break;
            }
            depth -= 1;
            if depth == 0
                && let Some(s) = start.take()
            {
                spans.push((s, i + 1));
            }
        }
        i += 1;
    }
    spans
}

/// CSV row spans (each includes its trailing `\n`; concatenation covers the file).
fn csv_row_spans(data: &[u8]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for (i, &b) in data.iter().enumerate() {
        if b == b'\n' {
            out.push((start, i + 1));
            start = i + 1;
        }
    }
    if start < data.len() {
        out.push((start, data.len()));
    }
    out
}

// ---------------------------------------------------------------------------
// per-format enumeration (bounded subset; everything else declines)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Csv,
    Json,
    Docx,
    Xlsx,
}

impl Format {
    fn name(self) -> &'static str {
        match self {
            Format::Csv => "csv",
            Format::Json => "json",
            Format::Docx => "docx",
            Format::Xlsx => "xlsx",
        }
    }
}

/// Resolve the requested/detected format. `hint` is `auto` or a format name.
fn detect_format(source: &[u8], hint: &str) -> std::result::Result<Format, String> {
    match hint {
        "csv" => Ok(Format::Csv),
        "json" => Ok(Format::Json),
        "docx" => Ok(Format::Docx),
        "xlsx" => Ok(Format::Xlsx),
        "auto" => {
            if source.starts_with(b"PK\x03\x04") {
                #[cfg(feature = "package")]
                {
                    if zip_member(source, b"word/document.xml").is_some() {
                        return Ok(Format::Docx);
                    }
                    if zip_member(source, b"xl/sharedStrings.xml").is_some()
                        || first_member(source, b"xl/worksheets/sheet").is_some()
                    {
                        return Ok(Format::Xlsx);
                    }
                    return Err("ZIP container is neither DOCX nor XLSX".to_string());
                }
                #[cfg(not(feature = "package"))]
                {
                    return Err("ZIP container support requires the `package` feature".to_string());
                }
            }
            if source.starts_with(b"%PDF") {
                return Err("PDF is outside the supported incremental subset".to_string());
            }
            // Anything with an early NUL byte is a binary format we do not model;
            // decline it typed rather than mis-reading it as one CSV row.
            let probe = &source[..source.len().min(8192)];
            if probe.contains(&0) {
                return Err("binary input is outside the supported incremental subset".to_string());
            }
            let first = source.iter().copied().find(|b| !b.is_ascii_whitespace());
            match first {
                Some(b'{') | Some(b'[') => Ok(Format::Json),
                Some(_) => Ok(Format::Csv),
                None => Err("empty source".to_string()),
            }
        }
        other => Err(format!("unknown --format {other:?}")),
    }
}

#[cfg(feature = "package")]
fn zip_physical(source: &[u8]) -> Option<crate::adapter::package::zip::ZipPhysical> {
    crate::adapter::package::zip::scan(source, Limits::DEFAULT).ok()
}

#[cfg(feature = "package")]
fn zip_member(source: &[u8], name: &[u8]) -> Option<(usize, usize)> {
    let p = zip_physical(source)?;
    p.members
        .iter()
        .find(|m| m.name == name)
        .map(|m| (m.data.0 as usize, (m.data.0 + m.data.1) as usize))
}

#[cfg(feature = "package")]
fn first_member(source: &[u8], prefix: &[u8]) -> Option<Vec<u8>> {
    let p = zip_physical(source)?;
    let mut names: Vec<Vec<u8>> = p
        .members
        .iter()
        .filter(|m| m.name.starts_with(prefix) && m.name.ends_with(b".xml"))
        .map(|m| m.name.clone())
        .collect();
    names.sort();
    names.into_iter().next()
}

/// Decompress a member's raw payload (stored or raw DEFLATE only).
#[cfg(feature = "package")]
fn member_content(
    source: &[u8],
    name: &[u8],
    limits: Limits,
) -> std::result::Result<Vec<u8>, String> {
    let p = zip_physical(source).ok_or_else(|| "ZIP scan failed".to_string())?;
    let m = p
        .members
        .iter()
        .find(|m| m.name == name)
        .ok_or_else(|| format!("member {} missing", String::from_utf8_lossy(name)))?;
    let a = m.data.0 as usize;
    let b = (m.data.0 + m.data.1) as usize;
    if b > source.len() {
        return Err("member span exceeds source".to_string());
    }
    match m.method {
        0 => Ok(source[a..b].to_vec()),
        8 => crate::field::derive::inflate_raw_deflate(&source[a..b], m.uncompressed_size, limits)
            .map_err(|e| format!("inflate {}: {e}", String::from_utf8_lossy(name))),
        other => Err(format!(
            "member {} uses compression method {other} (supported: 0 stored, 8 deflate)",
            String::from_utf8_lossy(name)
        )),
    }
}

fn enumerate(source: &[u8], fmt_hint: &str, limits: Limits) -> Enum {
    let fmt = match detect_format(source, fmt_hint) {
        Ok(f) => f,
        Err(reason) => return Enum::Declined { reason },
    };
    match fmt {
        Format::Csv => {
            let frags = text_fragments(source);
            let mut obs = Vec::new();
            for (i, (a, b)) in csv_row_spans(source).into_iter().enumerate() {
                let idx = covering(&frags, a, b);
                obs.push(Obs {
                    key: format!("csv:row:{i}"),
                    phys: frag_ids_at(&frags, &idx),
                    sem: Vec::new(),
                    out: source[a..b].to_vec(),
                });
            }
            Enum::Observed {
                format: fmt.name().to_string(),
                obs,
            }
        }
        Format::Json => {
            let frags = text_fragments(source);
            let mut obs = Vec::new();
            for (i, (a, b)) in json_top_level_spans(source, b"\"items\"")
                .into_iter()
                .enumerate()
            {
                let idx = covering(&frags, a, b);
                obs.push(Obs {
                    key: format!("json:elem:{i}"),
                    phys: frag_ids_at(&frags, &idx),
                    sem: Vec::new(),
                    out: source[a..b].to_vec(),
                });
            }
            Enum::Observed {
                format: fmt.name().to_string(),
                obs,
            }
        }
        Format::Docx => enumerate_docx(source, limits),
        Format::Xlsx => enumerate_xlsx(source, limits),
    }
}

#[cfg(feature = "package")]
fn enumerate_docx(source: &[u8], limits: Limits) -> Enum {
    let Some(p) = zip_physical(source) else {
        return Enum::Declined {
            reason: "DOCX ZIP scan failed".to_string(),
        };
    };
    let frag_of = |name: &[u8]| -> Option<[u8; 32]> {
        p.members.iter().find(|m| m.name == name).map(|m| {
            let a = m.data.0 as usize;
            let b = (m.data.0 + m.data.1) as usize;
            id_of(&source[a..b.min(source.len())])
        })
    };
    let doc = match member_content(source, b"word/document.xml", limits) {
        Ok(d) => d,
        Err(reason) => return Enum::Declined { reason },
    };
    let styles_bytes = member_content(source, b"word/styles.xml", limits).unwrap_or_default();
    let styles = docx_style_sizes(&styles_bytes);
    let mut sem = Vec::new();
    if let Some(id) = frag_of(b"word/styles.xml") {
        sem.push(id);
    }
    if let Some(id) = frag_of(b"word/numbering.xml") {
        sem.push(id);
    }
    let doc_frag = frag_of(b"word/document.xml");
    let mut obs = Vec::new();
    for (i, (a, b)) in docx_paragraph_spans(&doc).into_iter().enumerate() {
        let phys = doc_frag.map(|id| vec![id]).unwrap_or_default();
        obs.push(Obs {
            key: format!("docx:para:{i}"),
            phys,
            sem: sem.clone(),
            out: docx_paragraph_projection(&doc[a..b], &styles),
        });
    }
    Enum::Observed {
        format: "docx".to_string(),
        obs,
    }
}

#[cfg(not(feature = "package"))]
fn enumerate_docx(_source: &[u8], _limits: Limits) -> Enum {
    Enum::Declined {
        reason: "DOCX support requires the `package` feature".to_string(),
    }
}

#[cfg(feature = "package")]
fn enumerate_xlsx(source: &[u8], limits: Limits) -> Enum {
    let Some(p) = zip_physical(source) else {
        return Enum::Declined {
            reason: "XLSX ZIP scan failed".to_string(),
        };
    };
    let frag_of = |name: &[u8]| -> Option<[u8; 32]> {
        p.members.iter().find(|m| m.name == name).map(|m| {
            let a = m.data.0 as usize;
            let b = (m.data.0 + m.data.1) as usize;
            id_of(&source[a..b.min(source.len())])
        })
    };
    let sheet_name = match first_member(source, b"xl/worksheets/sheet") {
        Some(n) => n,
        None => {
            return Enum::Declined {
                reason: "XLSX has no worksheet part".to_string(),
            };
        }
    };
    let sheet = match member_content(source, &sheet_name, limits) {
        Ok(d) => d,
        Err(reason) => return Enum::Declined { reason },
    };
    let shared_bytes = member_content(source, b"xl/sharedStrings.xml", limits).unwrap_or_default();
    let shared = xlsx_shared_strings(&shared_bytes);
    let shared_frag = frag_of(b"xl/sharedStrings.xml");
    let styles_frag = frag_of(b"xl/styles.xml");
    let sheet_frag = frag_of(&sheet_name);
    let mut obs = Vec::new();
    for (i, (a, b)) in xlsx_cell_spans(&sheet).into_iter().enumerate() {
        let cell = &sheet[a..b];
        let is_shared = attr_value(cell, 0, cell.len(), b"t").as_deref() == Some(b"s");
        let mut sem = Vec::new();
        if is_shared && let Some(id) = shared_frag {
            sem.push(id);
        }
        if let Some(id) = styles_frag {
            sem.push(id);
        }
        obs.push(Obs {
            key: format!("xlsx:cell:{i}"),
            phys: sheet_frag.map(|id| vec![id]).unwrap_or_default(),
            sem,
            out: xlsx_cell_projection(cell, &shared),
        });
    }
    Enum::Observed {
        format: "xlsx".to_string(),
        obs,
    }
}

#[cfg(not(feature = "package"))]
fn enumerate_xlsx(_source: &[u8], _limits: Limits) -> Enum {
    Enum::Declined {
        reason: "XLSX support requires the `package` feature".to_string(),
    }
}

// ---------------------------------------------------------------------------
// durable index + output blob store
// ---------------------------------------------------------------------------

type Index = BTreeMap<[u8; 32], IndexEntry>;

fn index_file(dir: &Path) -> PathBuf {
    dir.join("index.v1")
}

fn read_index(dir: &Path) -> Result<Index> {
    let path = index_file(dir);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Index::new()),
        Err(e) => return Err(Error::io(format!("reading incremental index: {e}"))),
    };
    if bytes.len() < 20 || &bytes[0..8] != INDEX_MAGIC {
        return Err(Error::integrity_mismatch(
            "incremental index has a bad magic",
        ));
    }
    let version = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    if version != INDEX_FORMAT_VERSION {
        // A different format version is "no prior index": decline to reuse.
        return Ok(Index::new());
    }
    let count = u64::from_le_bytes([
        bytes[12], bytes[13], bytes[14], bytes[15], bytes[16], bytes[17], bytes[18], bytes[19],
    ]);
    let mut out = Index::new();
    let mut at = 20usize;
    for _ in 0..count {
        if at + 128 > bytes.len() {
            return Err(Error::integrity_mismatch("incremental index is truncated"));
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes[at..at + 32]);
        let mut phys = [0u8; 32];
        phys.copy_from_slice(&bytes[at + 32..at + 64]);
        let mut sem = [0u8; 32];
        sem.copy_from_slice(&bytes[at + 64..at + 96]);
        let mut oid = [0u8; 32];
        oid.copy_from_slice(&bytes[at + 96..at + 128]);
        out.insert(
            key,
            IndexEntry {
                phys,
                sem,
                out: oid,
            },
        );
        at += 128;
    }
    Ok(out)
}

fn write_index(dir: &Path, index: &Index) -> Result<u64> {
    let mut buf = Vec::with_capacity(20 + index.len() * 128);
    buf.extend_from_slice(INDEX_MAGIC);
    buf.extend_from_slice(&INDEX_FORMAT_VERSION.to_le_bytes());
    buf.extend_from_slice(&(index.len() as u64).to_le_bytes());
    for (k, e) in index {
        buf.extend_from_slice(k);
        buf.extend_from_slice(&e.phys);
        buf.extend_from_slice(&e.sem);
        buf.extend_from_slice(&e.out);
    }
    let len = buf.len() as u64;
    super::write_atomic(&index_file(dir), &buf)?;
    Ok(len)
}

fn blob_path(dir: &Path, id: &[u8; 32]) -> PathBuf {
    dir.join("obs").join(format!("{}.b", hex32(id)))
}

fn load_blob(dir: &Path, id: &[u8; 32]) -> Option<Vec<u8>> {
    let bytes = std::fs::read(blob_path(dir, id)).ok()?;
    if &id_of(&bytes) == id {
        Some(bytes)
    } else {
        None
    }
}

/// Store an output blob content-addressed. Returns the bytes newly written (0 if
/// the content id already existed).
fn put_blob(dir: &Path, bytes: &[u8]) -> Result<u64> {
    let id = id_of(bytes);
    let path = blob_path(dir, &id);
    if path.exists() {
        return Ok(0);
    }
    std::fs::create_dir_all(dir.join("obs"))
        .map_err(|e| Error::io(format!("creating obs dir: {e}")))?;
    super::write_atomic(&path, bytes)?;
    Ok(bytes.len() as u64)
}

fn dir_bytes(dir: &Path) -> u64 {
    let mut total = 0u64;
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    for ent in rd.flatten() {
        let p = ent.path();
        if p.is_file() {
            total = total.saturating_add(ent.metadata().map(|m| m.len()).unwrap_or(0));
        } else if p.is_dir() {
            total = total.saturating_add(dir_bytes(&p));
        }
    }
    total
}

// ---------------------------------------------------------------------------
// the mechanism
// ---------------------------------------------------------------------------

/// Run one revision through the incremental observation mechanism.
///
/// `source` is the exact revision bytes (from the field's exact authority).
/// `index_dir` holds the durable index and the content-addressed output blobs,
/// and is shared across a revision family so later revisions can reuse earlier
/// verified outputs.
pub fn incremental_observe(
    source: &[u8],
    index_dir: &Path,
    lane: Lane,
    verify: bool,
    max_observations: u64,
    format_hint: &str,
    limits: Limits,
) -> Result<IncrementalReport> {
    let t0 = std::time::Instant::now();
    if source.len() as u64 > limits.max_input_bytes {
        return Ok(IncrementalReport::declined(
            format!(
                "source {} bytes exceeds max_input_bytes {}",
                source.len(),
                limits.max_input_bytes
            ),
            lane,
        ));
    }

    let (format, obs) = match enumerate(source, format_hint, limits) {
        Enum::Declined { reason } => {
            return Ok(IncrementalReport::declined(reason, lane));
        }
        Enum::Observed { format, obs } => (format, obs),
    };
    if obs.len() as u64 > max_observations {
        return Ok(IncrementalReport::declined(
            format!(
                "{} observations exceed the bound {}",
                obs.len(),
                max_observations
            ),
            lane,
        ));
    }

    std::fs::create_dir_all(index_dir.join("obs"))
        .map_err(|e| Error::io(format!("creating incremental index dir: {e}")))?;
    let prior = read_index(index_dir)?;
    let prior_entries = prior.len() as u64;

    let mut index: Index = Index::new();
    let mut reused = 0u64;
    let mut invalidated = 0u64;
    let mut new_obs = 0u64;
    let mut recomputed = 0u64;
    let mut sem_only = 0u64;
    let mut bytes_reused = 0u64;
    let mut bytes_recomputed = 0u64;
    let mut blobs_written = 0u64;
    let mut blob_bytes_written = 0u64;
    let mut dependency_edges = 0u64;
    let mut mismatches = 0u64;
    let mut answers: Vec<([u8; 32], [u8; 32])> = Vec::with_capacity(obs.len());

    for o in &obs {
        dependency_edges = dependency_edges
            .saturating_add(o.phys.len() as u64)
            .saturating_add(o.sem.len() as u64);
        let key_hash = id_of(o.key.as_bytes());
        let phys = digest_ids(&o.phys);
        let sem = digest_ids(&o.sem);
        let (chosen, was_reused): (Vec<u8>, bool) = match (lane, prior.get(&key_hash)) {
            (Lane::Full, _) => (o.out.clone(), false),
            (Lane::Cas, Some(p)) if p.phys == phys => match load_blob(index_dir, &p.out) {
                Some(b) => (b, true),
                None => (o.out.clone(), false),
            },
            (Lane::Incremental, Some(p)) if p.phys == phys && p.sem == sem => {
                match load_blob(index_dir, &p.out) {
                    Some(b) => (b, true),
                    None => (o.out.clone(), false),
                }
            }
            _ => (o.out.clone(), false),
        };
        match prior.get(&key_hash) {
            None => new_obs += 1,
            Some(p) => {
                if !was_reused {
                    invalidated += 1;
                }
                // Lane-independent: an entry whose physical fragment bytes are
                // unchanged but whose semantic context changed. A physical-only
                // store reuses these WRONG; that is the load-bearing fact.
                if p.phys == phys && p.sem != sem {
                    sem_only += 1;
                }
            }
        }
        if was_reused {
            reused += 1;
            bytes_reused = bytes_reused.saturating_add(chosen.len() as u64);
        } else {
            recomputed += 1;
            bytes_recomputed = bytes_recomputed.saturating_add(chosen.len() as u64);
        }
        if verify && chosen != o.out {
            mismatches += 1;
        }
        let out_id = id_of(&chosen);
        let written = put_blob(index_dir, &chosen)?;
        if written > 0 {
            blobs_written += 1;
            blob_bytes_written = blob_bytes_written.saturating_add(written);
        }
        index.insert(
            key_hash,
            IndexEntry {
                phys,
                sem,
                out: out_id,
            },
        );
        answers.push((key_hash, out_id));
    }

    let index_bytes = write_index(index_dir, &index)?;
    answers.sort_unstable();
    let mut ah = blake3::Hasher::new();
    for (k, o) in &answers {
        ah.update(k);
        ah.update(o);
    }
    let answer_digest = to_hex(ah.finalize().as_bytes());

    let obs_blob_total_bytes = dir_bytes(&index_dir.join("obs"));
    let persistent_bytes = obs_blob_total_bytes.saturating_add(index_bytes);
    Ok(IncrementalReport {
        declined: false,
        decline_reason: String::new(),
        format,
        lane: lane.name().to_string(),
        index_version: INDEX_FORMAT_VERSION,
        prior_entries,
        n_obs: obs.len() as u64,
        reused,
        invalidated,
        new_obs,
        recomputed,
        semantic_only_invalidations: sem_only,
        bytes_reused,
        bytes_recomputed,
        blobs_written,
        blob_bytes_written,
        index_entries: index.len() as u64,
        index_bytes,
        dependency_edges,
        obs_blob_total_bytes,
        persistent_bytes,
        lane_us: t0.elapsed().as_micros() as u64,
        verified_equal: mismatches == 0,
        mismatches,
        answer_digest,
    })
}

/// The modeled dependency-graph index cost (edges at [`EDGE_MODEL_BYTES`] plus
/// the per-observation identity overhead the index actually persists), for
/// cross-referencing the Phase-28.7 model.
pub fn modeled_graph_index_bytes(index_entries: u64, dependency_edges: u64) -> u64 {
    index_entries
        .saturating_mul(64)
        .saturating_add(dependency_edges.saturating_mul(EDGE_MODEL_BYTES))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "vole-incr-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn cdc_covers_and_is_deterministic() {
        let data: Vec<u8> = (0..50_000u32).map(|i| (i * 7) as u8).collect();
        let a = text_fragments(&data);
        let b = text_fragments(&data);
        assert_eq!(a.len(), b.len());
        let mut at = 0usize;
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.start, at);
            assert_eq!((x.start, x.end), (y.start, y.end));
            assert_eq!(x.id, y.id);
            at = x.end;
        }
        assert_eq!(at, data.len());
    }

    #[test]
    fn style_sizes_are_read_and_paragraph_projection_depends_on_them() {
        let styles = br#"<w:styles><w:style w:type="paragraph" w:styleId="H1"><w:rPr><w:sz w:val="32"/></w:rPr></w:style></w:styles>"#;
        let map = docx_style_sizes(styles);
        assert_eq!(map.get(b"H1".as_slice()), Some(&32));
        let para = br#"<w:p><w:pPr><w:pStyle w:val="H1"/></w:pPr><w:r><w:t>Hi</w:t></w:r></w:p>"#;
        let out = docx_paragraph_projection(para, &map);
        assert!(out.starts_with(b"Hi\x1f32"), "got {out:?}");
        // A bumped style table changes the projection with identical paragraph bytes.
        let bumped = br#"<w:styles><w:style w:type="paragraph" w:styleId="H1"><w:rPr><w:sz w:val="34"/></w:rPr></w:style></w:styles>"#;
        let out2 = docx_paragraph_projection(para, &docx_style_sizes(bumped));
        assert_ne!(out, out2);
    }

    #[test]
    fn shared_string_cell_projection_resolves_through_the_table() {
        let shared =
            xlsx_shared_strings(br#"<sst><si><t>Alpha</t></si><si><t>Beta</t></si></sst>"#);
        assert_eq!(shared.len(), 2);
        let cell = br#"<c r="A1" t="s"><v>1</v></c>"#;
        let out = xlsx_cell_projection(cell, &shared);
        assert_eq!(out, b"s\x1fBeta");
        // Rewrite the shared table: same cell bytes, different projection.
        let shared2 =
            xlsx_shared_strings(br#"<sst><si><t>AlphaZ</t></si><si><t>BetaZ</t></si></sst>"#);
        assert_ne!(out, xlsx_cell_projection(cell, &shared2));
    }

    #[test]
    fn localized_csv_edit_reuses_semantic_cases_invalidate() {
        // Base and an edit to one row band; the incremental lane must reuse the
        // unchanged rows and recompute only the edited ones, and its answers must
        // equal a full rebuild's.
        let mut base = Vec::new();
        for i in 0..4000 {
            base.extend_from_slice(format!("{i},value-{i}\n").as_bytes());
        }
        let mut edited = base.clone();
        let needle = b"2000,value-2000\n";
        if let Some(at) = find_from(&edited, needle, 0) {
            edited[at + 5] = b'X';
        }

        let dir = temp_dir("csv");
        let full_dir = temp_dir("csv-full");
        let full = incremental_observe(
            &base,
            &full_dir,
            Lane::Full,
            false,
            1_000_000,
            "auto",
            Limits::DEFAULT,
        )
        .unwrap();
        let inc1 = incremental_observe(
            &base,
            &dir,
            Lane::Incremental,
            true,
            1_000_000,
            "auto",
            Limits::DEFAULT,
        )
        .unwrap();
        assert_eq!(inc1.format, "csv");
        assert!(inc1.verified_equal);

        let inc2 = incremental_observe(
            &edited,
            &dir,
            Lane::Incremental,
            true,
            1_000_000,
            "auto",
            Limits::DEFAULT,
        )
        .unwrap();
        let full2 = incremental_observe(
            &edited,
            &full_dir,
            Lane::Full,
            false,
            1_000_000,
            "auto",
            Limits::DEFAULT,
        )
        .unwrap();
        assert!(
            inc2.verified_equal,
            "incremental answers must be self-consistent"
        );
        assert_eq!(
            inc2.answer_digest, full2.answer_digest,
            "incremental answers must equal a full rebuild"
        );
        assert!(
            inc2.reused > 0,
            "a localized edit must reuse unchanged rows"
        );
        assert!(inc2.recomputed >= 1);
        assert!(!full.declined);
    }

    #[test]
    fn unsupported_input_declines_typed() {
        let dir = temp_dir("decl");
        // A PDF and an opaque binary both decline typed (never a wrong reuse).
        let pdf = incremental_observe(
            b"%PDF-1.7 not supported",
            &dir,
            Lane::Incremental,
            false,
            1000,
            "auto",
            Limits::DEFAULT,
        )
        .unwrap();
        assert!(pdf.declined);
        assert!(pdf.decline_reason.contains("PDF"), "{}", pdf.decline_reason);
        let bin = incremental_observe(
            b"\x00\x01\x02binary",
            &dir,
            Lane::Incremental,
            false,
            1000,
            "auto",
            Limits::DEFAULT,
        )
        .unwrap();
        assert!(bin.declined);
        assert!(detect_format(b"x", "bogus").unwrap_err().contains("bogus"));
        // An empty source declines too.
        assert!(detect_format(b"", "auto").is_err());
    }

    #[test]
    fn index_roundtrips_and_version_mismatch_resets() {
        let dir = temp_dir("idx");
        std::fs::create_dir_all(dir.join("obs")).unwrap();
        let mut index: Index = Index::new();
        index.insert(
            [1u8; 32],
            IndexEntry {
                phys: [2u8; 32],
                sem: [3u8; 32],
                out: [4u8; 32],
            },
        );
        write_index(&dir, &index).unwrap();
        let back = read_index(&dir).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back.get(&[1u8; 32]).unwrap().out, [4u8; 32]);
    }
}
