//! Phase 29.2 — a persistent **coarse structural index**.
//!
//! Phase 28.6 measured the closure for a narrow request as the *whole encoded
//! descriptor* and decided a **coarse** index (PDF objects/pages, ZIP members,
//! Parquet row groups/column chunks) — not per-record and not tiny fixed
//! fragments. This module builds that index and uses it to serve a narrow
//! observation from real `pread` range reads of the authority object.
//!
//! ## What is stored
//!
//! For each supported source the index records a partition of `[0, source_len)`
//! into **format-native addressable units** ([`CoarseUnit`]) with their byte
//! ranges in the authority. The authority here is the descriptor's single
//! `OBJECT` payload (`EMIT_OBJECT(0)`), which — when its SHA-256 equals the
//! declared source SHA-256 — is byte-identical to the source, so a source range
//! `[a,b)` is the descriptor range `[object_base+a, object_base+b)`. That
//! precondition is *checked*, never assumed: a descriptor whose authority is not
//! a verbatim single object is declined and the caller falls back to the full
//! read.
//!
//! ## Exactness and the fallback
//!
//! The index narrows *where the bytes come from*; it never changes the bytes.
//! [`CoarseSource`] serves source ranges out of whole coarse units. If a request
//! ever needs a range the plan did not cover, the read is a **typed decline**
//! (`UnsupportedFeature`) that the caller turns into a full descriptor read — a
//! missing/malformed index, an unknown format, or an uncovered unit must degrade
//! degraded to the full read, never to a wrong answer. A field without the index is
//! unaffected: `materialize --exact` and every other lane do not touch it.
//!
//! ## Realized granularity
//!
//! The index is exercised wherever an observation reads the source through
//! `serve_range` — a byte range, a PDF object/stream, a ZIP/OPC member, a Parquet
//! column-chunk span. A format *model* node that instead materializes the whole
//! exact root (the DOCX/ODT/Parquet reading projections) still reads the whole
//! source, so for those requests the units are built but inert. Phase 29.2's
//! court measures both shapes and reports the difference rather than assuming
//! the index narrows every request.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::container::header::{HEADER_LEN, Header};
use crate::container::record::{RECORD_HEADER_LEN, RECORD_TRAILER_LEN, RecordTag, read_record_at};
use crate::dra::Program;
use crate::dra::op::Op;
use crate::error::{Error, Result};
use crate::limits::Limits;
use crate::store::Id;

use super::FieldStore;
use super::dag::SourceServer;
use super::manifest::{FieldId, FieldRoot};
use super::observe::{ObserveRequest, ObserveStats};

/// Wire magic + version of the persisted index.
const COARSE_MAGIC: [u8; 4] = *b"VCI1";
/// A bound on the unit count so a hostile/malformed index cannot allocate.
const MAX_UNITS: u64 = 1 << 20;
/// Unit entry length: kind(1) + id(4) + offset(8) + len(4).
const ENTRY_LEN: usize = 17;

/// The native kind of a coarse unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum UnitKind {
    /// A byte range no native unit covers (framing, xref tables, padding).
    Gap = 0,
    /// The whole authority object (a format with no finer units).
    AuthorityLeaf = 1,
    /// A PDF indirect object (`N G obj` … `endobj`).
    PdfObject = 2,
    /// The PDF xref/trailer tail.
    PdfTail = 3,
    /// A ZIP/OPC container member (local header + compressed data).
    ZipMember = 4,
    /// The ZIP central directory + EOCD region.
    ZipDirectory = 5,
    /// A Parquet column chunk (its narrowest addressable unit).
    ParquetColumnChunk = 6,
    /// The Parquet footer.
    ParquetFooter = 7,
}

impl UnitKind {
    fn code(self) -> u8 {
        self as u8
    }

    fn from_code(c: u8) -> Result<Self> {
        Ok(match c {
            0 => Self::Gap,
            1 => Self::AuthorityLeaf,
            2 => Self::PdfObject,
            3 => Self::PdfTail,
            4 => Self::ZipMember,
            5 => Self::ZipDirectory,
            6 => Self::ParquetColumnChunk,
            7 => Self::ParquetFooter,
            _ => return Err(Error::invalid_container("unknown coarse unit kind")),
        })
    }
}

/// One format-native addressable unit: a byte range of the source/authority.
#[derive(Debug, Clone, Copy)]
pub struct CoarseUnit {
    /// Native kind.
    pub kind: UnitKind,
    /// Ordinal within its kind (an object number, a member index, …).
    pub id: u32,
    /// Start offset in the source.
    pub offset: u64,
    /// Length in bytes.
    pub len: u32,
}

/// The persisted coarse structural directory for one field.
#[derive(Debug, Clone)]
pub struct CoarseIndex {
    /// `0` unsupported (no native units), `1` PDF, `2` ZIP, `3` Parquet.
    pub format: u8,
    /// Exact source length.
    pub source_len: u64,
    /// Exact source SHA-256.
    pub source_sha256: [u8; 32],
    /// Offset of the authority object payload in the descriptor.
    pub object_base: u64,
    /// The descriptor's object count (for the observation view).
    pub object_count: u32,
    /// The descriptor's graph op count (for the observation view).
    pub graph_ops: u32,
    /// A partition of `[0, source_len)`.
    pub units: Vec<CoarseUnit>,
}

/// The detected format of a coarse index.
pub fn format_name(code: u8) -> &'static str {
    match code {
        1 => "pdf",
        2 => "zip",
        3 => "parquet",
        _ => "unsupported",
    }
}

impl CoarseIndex {
    /// Encode the index into its bounded, versioned wheel.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(69 + self.units.len() * ENTRY_LEN);
        out.extend_from_slice(&COARSE_MAGIC);
        out.push(self.format);
        out.extend_from_slice(&self.object_count.to_le_bytes());
        out.extend_from_slice(&self.graph_ops.to_le_bytes());
        out.extend_from_slice(&self.source_len.to_le_bytes());
        out.extend_from_slice(&self.object_base.to_le_bytes());
        out.extend_from_slice(&self.source_sha256);
        out.extend_from_slice(&(self.units.len() as u32).to_le_bytes());
        for u in &self.units {
            out.push(u.kind.code());
            out.extend_from_slice(&u.id.to_le_bytes());
            out.extend_from_slice(&u.offset.to_le_bytes());
            out.extend_from_slice(&u.len.to_le_bytes());
        }
        out
    }

    /// Decode and structurally validate a persisted index.
    pub fn decode(bytes: &[u8], limits: Limits) -> Result<Self> {
        let mut p = 0usize;
        let magic = bytes
            .get(p..p + 4)
            .ok_or_else(|| Error::invalid_container("coarse index truncated (magic)"))?;
        if magic != COARSE_MAGIC {
            return Err(Error::invalid_container("coarse index magic mismatch"));
        }
        p += 4;
        let format = *bytes
            .get(p)
            .ok_or_else(|| Error::invalid_container("coarse index truncated (format)"))?;
        p += 1;
        let object_count = read_u32(bytes, &mut p)?;
        let graph_ops = read_u32(bytes, &mut p)?;
        let source_len = read_u64(bytes, &mut p)?;
        let object_base = read_u64(bytes, &mut p)?;
        let sha = bytes
            .get(p..p + 32)
            .ok_or_else(|| Error::invalid_container("coarse index truncated (sha)"))?;
        let mut source_sha256 = [0u8; 32];
        source_sha256.copy_from_slice(sha);
        p += 32;
        let count = u64::from(read_u32(bytes, &mut p)?);
        if count > MAX_UNITS || count > u64::from(limits.max_record_len) {
            return Err(Error::resource_limit(
                "coarse index unit count exceeds limit",
            ));
        }
        let need = (count as usize)
            .checked_mul(ENTRY_LEN)
            .ok_or_else(|| Error::resource_limit("coarse index size overflow"))?;
        if bytes.len().saturating_sub(p) < need {
            return Err(Error::invalid_container("coarse index truncated (units)"));
        }
        let mut units = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let kind = UnitKind::from_code(bytes[p])?;
            p += 1;
            let id = read_u32(bytes, &mut p)?;
            let offset = read_u64(bytes, &mut p)?;
            let len = read_u32(bytes, &mut p)?;
            units.push(CoarseUnit {
                kind,
                id,
                offset,
                len,
            });
        }
        if p != bytes.len() {
            return Err(Error::invalid_container("coarse index has trailing bytes"));
        }
        let index = CoarseIndex {
            format,
            source_len,
            source_sha256,
            object_base,
            object_count,
            graph_ops,
            units,
        };
        index.validate()?;
        Ok(index)
    }

    /// Confirm the units are a well-formed, in-bounds, ascending partition.
    fn validate(&self) -> Result<()> {
        let mut cursor = 0u64;
        for u in &self.units {
            let end = u
                .offset
                .checked_add(u64::from(u.len))
                .ok_or_else(|| Error::invalid_container("coarse unit length overflow"))?;
            if u.offset != cursor || end > self.source_len {
                return Err(Error::invalid_container(
                    "coarse units are not an ascending partition of the source",
                ));
            }
            cursor = end;
        }
        if cursor != self.source_len {
            return Err(Error::invalid_container(
                "coarse units do not cover the whole source",
            ));
        }
        Ok(())
    }

    /// The units intersecting `[offset, end)`, in ascending order.
    #[allow(dead_code)]
    fn covering(&self, offset: u64, end: u64) -> Vec<usize> {
        let mut out = Vec::new();
        for (i, u) in self.units.iter().enumerate() {
            let ue = u.offset + u64::from(u.len);
            if u.offset < end && offset < ue {
                out.push(i);
            }
        }
        out
    }

    /// Bytes the encoded index occupies.
    pub fn encoded_len(&self) -> u64 {
        self.encode().len() as u64
    }
}

fn read_u32(bytes: &[u8], p: &mut usize) -> Result<u32> {
    let s = bytes
        .get(*p..*p + 4)
        .ok_or_else(|| Error::invalid_container("coarse index truncated"))?;
    *p += 4;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn read_u64(bytes: &[u8], p: &mut usize) -> Result<u64> {
    let s = bytes
        .get(*p..*p + 8)
        .ok_or_else(|| Error::invalid_container("coarse index truncated"))?;
    *p += 8;
    Ok(u64::from_le_bytes([
        s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
    ]))
}

// ---------------------------------------------------------------------------
// Unit construction
// ---------------------------------------------------------------------------

fn push(units: &mut Vec<CoarseUnit>, kind: UnitKind, id: u32, offset: u64, len: u64, total: u64) {
    if len == 0 || offset >= total || u32::try_from(len).is_err() {
        return;
    }
    let capped = len.min(total - offset);
    units.push(CoarseUnit {
        kind,
        id,
        offset,
        len: capped as u32,
    });
}

/// Turn native spans into a partition of `[0, total)` by inserting gap units and
/// resolving overlaps (finer spans win, coarser duplicates are dropped).
fn partition(mut units: Vec<CoarseUnit>, total: u64) -> Vec<CoarseUnit> {
    units.retain(|u| u.len > 0 && u.offset < total);
    units.sort_by(|a, b| a.offset.cmp(&b.offset).then(a.len.cmp(&b.len)));
    let mut out: Vec<CoarseUnit> = Vec::new();
    let mut cursor = 0u64;
    for u in units {
        let start = u.offset.max(cursor);
        let end = u.offset + u64::from(u.len);
        if end <= cursor {
            continue;
        }
        if start > cursor {
            out.push(CoarseUnit {
                kind: UnitKind::Gap,
                id: 0,
                offset: cursor,
                len: (start - cursor) as u32,
            });
        }
        let len = (end - start) as u32;
        out.push(CoarseUnit {
            kind: u.kind,
            id: u.id,
            offset: start,
            len,
        });
        cursor = end;
    }
    if cursor < total {
        out.push(CoarseUnit {
            kind: UnitKind::Gap,
            id: 0,
            offset: cursor,
            len: (total - cursor) as u32,
        });
    }
    out
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    let first = needle[0];
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        if hay[i] == first && &hay[i..i + needle.len()] == needle {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn rfind(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    let mut i = hay.len() - needle.len();
    loop {
        if &hay[i..i + needle.len()] == needle {
            return Some(i);
        }
        if i == 0 {
            return None;
        }
        i -= 1;
    }
}

fn obj_header_start(d: &[u8], objp: usize) -> Option<usize> {
    let mut i = objp;
    while i > 0 && d[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    let g_end = i;
    while i > 0 && d[i - 1].is_ascii_digit() {
        i -= 1;
    }
    if i == g_end {
        return None;
    }
    while i > 0 && d[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    let n_end = i;
    while i > 0 && d[i - 1].is_ascii_digit() {
        i -= 1;
    }
    if i == n_end {
        return None;
    }
    Some(i)
}

fn pdf_units(data: &[u8], units: &mut Vec<CoarseUnit>) {
    let n = data.len();
    let total = n as u64;
    let mut pos = 0usize;
    let mut id = 0u32;
    while pos + 3 <= n {
        if &data[pos..pos + 3] == b"obj"
            && (pos + 3 == n || !is_word(data[pos + 3]))
            && let Some(start) = obj_header_start(data, pos)
        {
            match find(&data[pos + 3..], b"endobj") {
                Some(e) => {
                    let end = pos + 3 + e + b"endobj".len();
                    push(
                        units,
                        UnitKind::PdfObject,
                        id,
                        start as u64,
                        (end - start) as u64,
                        total,
                    );
                    id += 1;
                    pos = end;
                    continue;
                }
                // No `endobj` remains anywhere after `pos`, so no later candidate
                // can pair either: stop scanning instead of rescanning the tail
                // once per `obj` (a hostile all-`obj` blob is otherwise O(n^2)).
                None => break,
            }
        }
        pos += 1;
    }
    if let Some(x) = rfind(data, b"startxref") {
        push(units, UnitKind::PdfTail, 0, x as u64, (n - x) as u64, total);
    }
}

fn u16le(d: &[u8], i: usize) -> Option<u16> {
    let s = d.get(i..i + 2)?;
    Some(u16::from_le_bytes([s[0], s[1]]))
}

fn u32le(d: &[u8], i: usize) -> Option<u32> {
    let s = d.get(i..i + 4)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// Build ZIP/OPC member units and the central-directory unit. Declines (returns
/// `Err`) on ZIP64 / multi-disk / malformed geometry so the caller falls back.
fn zip_units(data: &[u8], units: &mut Vec<CoarseUnit>) -> Result<()> {
    let n = data.len();
    let total = n as u64;
    let search_from = n.saturating_sub(65_557);
    let eocd = rfind(&data[search_from..], b"PK\x05\x06")
        .map(|i| i + search_from)
        .ok_or_else(|| Error::invalid_container("ZIP: no end-of-central-directory"))?;
    let disk = u16le(data, eocd + 4).unwrap_or(1);
    let entries_disk = u16le(data, eocd + 8).unwrap_or(0);
    let entries_total = u16le(data, eocd + 10).unwrap_or(0);
    let cd_size = u32le(data, eocd + 12).unwrap_or(0);
    let cd_off = u32le(data, eocd + 16).unwrap_or(0);
    if disk != 0 || entries_disk != entries_total {
        return Err(Error::unsupported_feature(
            "ZIP: multi-disk archives declined",
        ));
    }
    if entries_total == u16::MAX || cd_size == u32::MAX || cd_off == u32::MAX {
        return Err(Error::unsupported_feature("ZIP64 archives declined"));
    }
    let cd_off = cd_off as usize;
    if cd_off > n {
        return Err(Error::invalid_container(
            "ZIP: central directory offset out of range",
        ));
    }
    let mut p = cd_off;
    for i in 0..entries_total {
        if data.get(p..p + 4) != Some(b"PK\x01\x02") {
            return Err(Error::invalid_container("ZIP: bad central-directory entry"));
        }
        let method = u16le(data, p + 10).unwrap_or(0);
        let comp = u32le(data, p + 20).unwrap_or(0);
        let name_len = u16le(data, p + 28).unwrap_or(0) as usize;
        let extra_len = u16le(data, p + 30).unwrap_or(0) as usize;
        let comment_len = u16le(data, p + 32).unwrap_or(0) as usize;
        let lo_raw = u32le(data, p + 42).unwrap_or(0);
        let _ = method;
        if comp == u32::MAX || lo_raw == u32::MAX {
            return Err(Error::unsupported_feature("ZIP64 member declined"));
        }
        let lo = lo_raw as usize;
        // Local header carries the authoritative name/extra lengths.
        if data.get(lo..lo + 4) != Some(b"PK\x03\x04") {
            return Err(Error::invalid_container("ZIP: bad local header"));
        }
        let ln = u16le(data, lo + 26).unwrap_or(0) as usize;
        let le = u16le(data, lo + 28).unwrap_or(0) as usize;
        let start = lo as u64;
        let end = (lo as u64)
            .saturating_add(30)
            .saturating_add(ln as u64)
            .saturating_add(le as u64)
            .saturating_add(comp as u64);
        if end > total || end < start {
            return Err(Error::invalid_container("ZIP: member span out of range"));
        }
        units.push(CoarseUnit {
            kind: UnitKind::ZipMember,
            id: i as u32,
            offset: start,
            len: (end - start) as u32,
        });
        p = p
            .checked_add(46 + name_len + extra_len + comment_len)
            .ok_or_else(|| Error::invalid_container("ZIP: central directory overflow"))?;
    }
    push(
        units,
        UnitKind::ZipDirectory,
        0,
        cd_off as u64,
        total.saturating_sub(cd_off as u64),
        total,
    );
    Ok(())
}

/// A minimal Thrift-Compact value, only as much as the Parquet footer needs.
#[derive(Debug)]
enum TVal {
    Bool,
    Int(i64),
    Bytes,
    List(Vec<TVal>),
    Struct(Vec<(i16, TVal)>),
    Other,
}

/// A bound on Thrift-Compact nesting, so a hostile footer cannot recurse the
/// parser into a stack overflow. Real Parquet footers nest a handful of levels.
const MAX_THRIFT_DEPTH: u32 = 64;

struct TC<'a> {
    b: &'a [u8],
    i: usize,
    depth: u32,
}

impl<'a> TC<'a> {
    fn byte(&mut self) -> Result<u8> {
        let v = *self
            .b
            .get(self.i)
            .ok_or_else(|| Error::invalid_container("thrift: truncated"))?;
        self.i += 1;
        Ok(v)
    }

    fn varint(&mut self) -> Result<u64> {
        let mut shift = 0u32;
        let mut out = 0u64;
        loop {
            let c = self.byte()?;
            out |= u64::from(c & 0x7F) << shift;
            if c & 0x80 == 0 {
                return Ok(out);
            }
            shift += 7;
            if shift > 63 {
                return Err(Error::invalid_container("thrift: varint overflow"));
            }
        }
    }

    fn zig(&mut self) -> Result<i64> {
        let u = self.varint()?;
        Ok(((u >> 1) as i64) ^ -((u & 1) as i64))
    }

    fn skip(&mut self, t: u8) -> Result<TVal> {
        Ok(match t {
            1 | 2 => TVal::Bool,
            3 => TVal::Int(i64::from(self.byte()?)),
            4..=6 => TVal::Int(self.zig()?),
            7 => {
                self.i = self
                    .i
                    .checked_add(8)
                    .ok_or_else(|| Error::invalid_container("thrift: overflow"))?;
                TVal::Other
            }
            8 => {
                let ln = self.varint()? as usize;
                if self.b.get(self.i..self.i + ln).is_none() {
                    return Err(Error::invalid_container("thrift: truncated binary"));
                }
                self.i += ln;
                TVal::Bytes
            }
            9 | 10 => {
                let h = self.byte()?;
                let mut size = u64::from((h & 0xF0) >> 4);
                let et = h & 0x0F;
                if size == 15 {
                    size = self.varint()?;
                }
                if size > MAX_UNITS {
                    return Err(Error::resource_limit("thrift: list too long"));
                }
                self.descend()?;
                let mut items = Vec::with_capacity(size as usize);
                for _ in 0..size {
                    items.push(self.skip(et)?);
                }
                self.ascend();
                TVal::List(items)
            }
            11 => {
                let size = self.varint()?;
                if size == 0 {
                    TVal::Other
                } else {
                    let kv = self.byte()?;
                    self.descend()?;
                    for _ in 0..size {
                        self.skip((kv & 0xF0) >> 4)?;
                        self.skip(kv & 0x0F)?;
                    }
                    self.ascend();
                    TVal::Other
                }
            }
            12 => {
                self.descend()?;
                let s = TVal::Struct(self.read_struct()?);
                self.ascend();
                s
            }
            _ => return Err(Error::invalid_container("thrift: unknown type")),
        })
    }

    fn descend(&mut self) -> Result<()> {
        self.depth += 1;
        if self.depth > MAX_THRIFT_DEPTH {
            return Err(Error::invalid_container("thrift: nesting too deep"));
        }
        Ok(())
    }

    fn ascend(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    fn read_struct(&mut self) -> Result<Vec<(i16, TVal)>> {
        let mut out = Vec::new();
        let mut last: i16 = 0;
        loop {
            let h = self.byte()?;
            if h == 0 {
                return Ok(out);
            }
            let delta = i16::from((h & 0xF0) >> 4);
            let t = h & 0x0F;
            let fid = if delta == 0 {
                self.zig()? as i16
            } else {
                last + delta
            };
            last = fid;
            let v = self.skip(t)?;
            out.push((fid, v));
        }
    }
}

fn struct_field(fields: &[(i16, TVal)], id: i16) -> Option<&TVal> {
    fields.iter().find(|(k, _)| *k == id).map(|(_, v)| v)
}

fn as_list(v: Option<&TVal>) -> &[TVal] {
    match v {
        Some(TVal::List(l)) => l,
        _ => &[],
    }
}

fn as_i64(v: Option<&TVal>) -> Option<i64> {
    match v {
        Some(TVal::Int(i)) => Some(*i),
        _ => None,
    }
}

/// Build Parquet column-chunk units + the footer unit. Declines on a malformed
/// footer so the caller falls back.
fn parquet_units(data: &[u8], units: &mut Vec<CoarseUnit>) -> Result<()> {
    let n = data.len();
    let total = n as u64;
    let flen =
        u32le(data, n - 8).ok_or_else(|| Error::invalid_container("parquet: short"))? as usize;
    if flen + 8 > n || flen == 0 {
        return Err(Error::invalid_container("parquet: bad footer length"));
    }
    let fstart = n - 8 - flen;
    let mut tc = TC {
        b: &data[fstart..n - 8],
        i: 0,
        depth: 0,
    };
    let meta = tc.read_struct()?;
    let rgs = as_list(struct_field(&meta, 4));
    let mut id = 0u32;
    for rg in rgs {
        let TVal::Struct(rg_fields) = rg else {
            continue;
        };
        for col in as_list(struct_field(rg_fields, 1)) {
            let TVal::Struct(col_fields) = col else {
                continue;
            };
            let Some(TVal::Struct(cm)) = struct_field(col_fields, 3) else {
                continue;
            };
            let start = as_i64(struct_field(cm, 11))
                .or_else(|| as_i64(struct_field(cm, 9)))
                .unwrap_or(0);
            let size = as_i64(struct_field(cm, 7)).unwrap_or(0);
            if start < 0 || size <= 0 {
                continue;
            }
            let start = start as u64;
            let size = size as u64;
            if start + size > total || size > u64::from(u32::MAX) {
                return Err(Error::invalid_container(
                    "parquet: column chunk out of range",
                ));
            }
            units.push(CoarseUnit {
                kind: UnitKind::ParquetColumnChunk,
                id,
                offset: start,
                len: size as u32,
            });
            id += 1;
        }
    }
    push(
        units,
        UnitKind::ParquetFooter,
        0,
        fstart as u64,
        (8 + flen) as u64,
        total,
    );
    Ok(())
}

/// Detect the format and build the native unit set for `source`.
pub fn build_units(source: &[u8]) -> (u8, Vec<CoarseUnit>) {
    let n = source.len() as u64;
    let mut units: Vec<CoarseUnit> = Vec::new();
    let format = if source.starts_with(b"%PDF-") {
        pdf_units(source, &mut units);
        1
    } else if source.starts_with(b"PK\x03\x04") && zip_units(source, &mut units).is_ok() {
        2
    } else if source.len() >= 12
        && source.starts_with(b"PAR1")
        && source.ends_with(b"PAR1")
        && parquet_units(source, &mut units).is_ok()
    {
        3
    } else {
        units.clear();
        0
    };
    if format == 0 {
        units.clear();
        units.push(CoarseUnit {
            kind: UnitKind::AuthorityLeaf,
            id: 0,
            offset: 0,
            len: u32::try_from(n).unwrap_or(u32::MAX),
        });
    }
    (format, partition(units, n))
}

// ---------------------------------------------------------------------------
// Authority discovery
// ---------------------------------------------------------------------------

/// The descriptor's verbatim authority object, when the descriptor is exactly
/// `EMIT_OBJECT(0)` over one object whose bytes equal the source.
pub struct Authority {
    /// Offset of the object payload in the descriptor file.
    pub object_base: u64,
    /// The object payload (byte-identical to the source).
    pub payload: Vec<u8>,
    /// Object count from the descriptor.
    pub object_count: u32,
    /// Graph op count from the descriptor.
    pub graph_ops: u32,
}

/// Walk the descriptor framing (read-only) and return the single authority
/// object **iff** it is the verbatim source under `EMIT_OBJECT(0)`.
///
/// Returns `Ok(None)` for any other shape — external objects, a multi-object
/// program, a transformed program, or a payload whose SHA-256 is not the
/// declared source SHA-256. The caller then falls back to the full read.
pub fn discover_authority(
    descriptor_path: &Path,
    manifest: &FieldRoot,
    limits: Limits,
) -> Result<Option<Authority>> {
    let mut file =
        File::open(descriptor_path).map_err(|e| Error::io(format!("opening descriptor: {e}")))?;
    let mut hdr = [0u8; HEADER_LEN];
    file.read_exact(&mut hdr)
        .map_err(|_| Error::invalid_container("truncated header"))?;
    let _header = Header::decode(&hdr)?;
    let file_len = file
        .seek(SeekFrom::End(0))
        .map_err(|e| Error::io(format!("seek to end failed: {e}")))?;
    let mut object_sites: Vec<(u64, u32)> = Vec::new();
    let mut graph: Option<Vec<u8>> = None;
    let mut pos = HEADER_LEN as u64;
    while pos < file_len {
        let mut rec_hdr = [0u8; RECORD_HEADER_LEN];
        file.seek(SeekFrom::Start(pos))
            .map_err(|e| Error::io(format!("seek failed: {e}")))?;
        file.read_exact(&mut rec_hdr)
            .map_err(|_| Error::invalid_container("truncated record header"))?;
        let tag = rec_hdr[0];
        let len = u32::from_le_bytes([rec_hdr[4], rec_hdr[5], rec_hdr[6], rec_hdr[7]]);
        let total = RECORD_HEADER_LEN as u64 + u64::from(len) + RECORD_TRAILER_LEN as u64;
        let end = pos
            .checked_add(total)
            .ok_or_else(|| Error::invalid_container("record length overflow"))?;
        if end > file_len {
            return Err(Error::invalid_container("truncated record"));
        }
        match RecordTag::from_u8(tag) {
            Some(RecordTag::Object) => {
                object_sites.push((pos + RECORD_HEADER_LEN as u64, len));
            }
            Some(RecordTag::Graph) => {
                let rec = read_record_at(&mut file, pos, limits)?;
                graph = Some(rec.payload);
            }
            Some(RecordTag::ExternalRef) => return Ok(None),
            Some(RecordTag::Trailer) => break,
            _ => {}
        }
        pos = end;
    }
    if object_sites.len() != 1 {
        return Ok(None);
    }
    let Some(graph) = graph else {
        return Ok(None);
    };
    let program = Program::decode(&graph, limits)?;
    if program.ops.len() != 1 || !matches!(program.ops[0], Op::EmitObject { object_id: 0 }) {
        return Ok(None);
    }
    let (object_base, object_len) = object_sites[0];
    let mut payload = vec![0u8; object_len as usize];
    file.read_exact_at(&mut payload, object_base)
        .map_err(|e| Error::io(format!("reading authority object: {e}")))?;
    let mut h = Sha256::new();
    h.update(&payload);
    let digest: [u8; 32] = h.finalize().into();
    if digest != manifest.source_sha256 || payload.len() as u64 != manifest.source_len {
        return Ok(None);
    }
    Ok(Some(Authority {
        object_base,
        payload,
        object_count: 1,
        graph_ops: 1,
    }))
}

// ---------------------------------------------------------------------------
// Persistent side structure
// ---------------------------------------------------------------------------

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0F) as usize] as char);
    }
    s
}

fn coarse_dir(root: &Path) -> PathBuf {
    root.join("coarse")
}

/// The persisted index path for a descriptor id.
pub fn index_path(root: &Path, descriptor_id: &[u8; 32]) -> PathBuf {
    coarse_dir(root).join(format!("{}.idx", hex(descriptor_id)))
}

/// The dependency-map path for a descriptor id.
pub fn map_path(root: &Path, descriptor_id: &[u8; 32]) -> PathBuf {
    coarse_dir(root).join(format!("{}.map", hex(descriptor_id)))
}

/// Persist or load a dependency map (request line → unit ordinals).
pub fn load_map(path: &Path) -> BTreeMap<String, Vec<usize>> {
    let mut out = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return out;
    };
    for line in text.lines() {
        if let Some((k, v)) = line.split_once('\t') {
            let units: Vec<usize> = v
                .split(',')
                .filter_map(|s| s.parse::<usize>().ok())
                .collect();
            out.insert(k.to_string(), units);
        }
    }
    out
}

fn save_map(path: &Path, map: &BTreeMap<String, Vec<usize>>) {
    let mut text = String::new();
    for (k, v) in map {
        let joined: Vec<String> = v.iter().map(usize::to_string).collect();
        text.push_str(k);
        text.push('\t');
        text.push_str(&joined.join(","));
        text.push('\n');
    }
    let _ = super::write_atomic(path, text.as_bytes());
}

// ---------------------------------------------------------------------------
// The narrow reader
// ---------------------------------------------------------------------------

/// A [`SourceServer`] that serves source ranges out of whole coarse units of the
/// verbatim authority object, reading each unit with a real `pread`.
pub struct CoarseSource {
    file: File,
    object_base: u64,
    source_len: u64,
    units: Vec<CoarseUnit>,
    loaded: RefCell<Vec<Option<Vec<u8>>>>,
    expand: bool,
    reads: Cell<u64>,
    bytes_read: Cell<u64>,
    misses: Cell<u64>,
    covered_lo: Cell<u64>,
    covered_hi: Cell<u64>,
}

impl CoarseSource {
    fn new(descriptor_path: &Path, index: &CoarseIndex, expand: bool) -> Result<Self> {
        let file = File::open(descriptor_path)
            .map_err(|e| Error::io(format!("opening descriptor: {e}")))?;
        let n = index.units.len();
        Ok(CoarseSource {
            file,
            object_base: index.object_base,
            source_len: index.source_len,
            units: index.units.clone(),
            loaded: RefCell::new(vec![None; n]),
            expand,
            reads: Cell::new(0),
            bytes_read: Cell::new(0),
            misses: Cell::new(0),
            covered_lo: Cell::new(u64::MAX),
            covered_hi: Cell::new(0),
        })
    }

    /// Real descriptor payload bytes fetched by this reader.
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read.get()
    }

    /// Number of `pread` calls made.
    pub fn reads(&self) -> u64 {
        self.reads.get()
    }

    /// Number of coverage misses (should be 0 for an exact, complete closure).
    pub fn misses(&self) -> u64 {
        self.misses.get()
    }

    /// Unit ordinals actually fetched.
    pub fn touched_units(&self) -> Vec<usize> {
        self.loaded
            .borrow()
            .iter()
            .enumerate()
            .filter(|(_, v)| v.is_some())
            .map(|(i, _)| i)
            .collect()
    }

    /// The covered byte span (for the receipt).
    pub fn covered_span(&self) -> Option<(u64, u64)> {
        if self.covered_hi.get() <= self.covered_lo.get() {
            None
        } else {
            Some((self.covered_lo.get(), self.covered_hi.get()))
        }
    }

    fn fetch(&self, i: usize, loaded: &mut [Option<Vec<u8>>]) -> Result<()> {
        if loaded[i].is_some() {
            return Ok(());
        }
        if !self.expand {
            self.misses.set(self.misses.get() + 1);
            return Err(Error::unsupported_feature(
                "coarse plan did not cover a required unit; falling back to the full read",
            ));
        }
        self.read_unit(i, loaded)
    }

    /// Read one unit unconditionally (a real `pread`), regardless of `expand`.
    fn read_unit(&self, i: usize, loaded: &mut [Option<Vec<u8>>]) -> Result<()> {
        if loaded[i].is_some() {
            return Ok(());
        }
        let u = self.units[i];
        let mut buf = vec![0u8; u.len as usize];
        self.file
            .read_exact_at(&mut buf, self.object_base + u.offset)
            .map_err(|e| Error::io(format!("coarse pread failed: {e}")))?;
        self.bytes_read
            .set(self.bytes_read.get() + buf.len() as u64);
        self.reads.set(self.reads.get() + 1);
        self.covered_lo.set(self.covered_lo.get().min(u.offset));
        self.covered_hi
            .set(self.covered_hi.get().max(u.offset + u64::from(u.len)));
        loaded[i] = Some(buf);
        Ok(())
    }

    /// Prefetch a planned set of units so the closure is exactly that set.
    ///
    /// The plan is authoritative: its units are read unconditionally. `expand`
    /// only governs an *unplanned* unit requested during serving (a coverage
    /// miss falls back to the full read, never to discovery).
    fn prefetch(&self, ordinals: &[usize]) -> Result<()> {
        let mut loaded = self.loaded.borrow_mut();
        for &i in ordinals {
            if i < self.units.len() {
                self.read_unit(i, &mut loaded)?;
            }
        }
        Ok(())
    }
}

impl SourceServer for CoarseSource {
    fn serve_range(&self, offset: u64, len: u64, _limits: Limits) -> Result<Vec<u8>> {
        let end = offset
            .checked_add(len)
            .ok_or_else(|| Error::usage("source slice end overflows"))?;
        if end > self.source_len {
            return Err(Error::usage(format!(
                "source slice {offset}..{end} exceeds source length {}",
                self.source_len
            )));
        }
        if len == 0 {
            return Ok(Vec::new());
        }
        // Units are a partition, so the intersection is non-empty and ordered.
        let needed = {
            let mut v = Vec::new();
            for (i, u) in self.units.iter().enumerate() {
                let ue = u.offset + u64::from(u.len);
                if u.offset < end && offset < ue {
                    v.push(i);
                }
            }
            v
        };
        if needed.is_empty() {
            return Err(Error::internal_invariant(
                "coarse units do not cover a requested source range",
            ));
        }
        let mut loaded = self.loaded.borrow_mut();
        for &i in &needed {
            self.fetch(i, &mut loaded)?;
        }
        let mut out = Vec::with_capacity((end - offset) as usize);
        let mut cursor = offset;
        for &i in &needed {
            let u = self.units[i];
            let ue = u.offset + u64::from(u.len);
            let s = cursor.max(u.offset);
            let e = end.min(ue);
            if e > s {
                if let Some(buf) = loaded[i].as_ref() {
                    let a = (s - u.offset) as usize;
                    let b = (e - u.offset) as usize;
                    out.extend_from_slice(&buf[a..b]);
                }
                cursor = e;
            }
        }
        if cursor != end {
            return Err(Error::internal_invariant(
                "coarse units left a gap in a requested source range",
            ));
        }
        Ok(out)
    }

    fn serve_document(&self, _limits: Limits) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; self.source_len as usize];
        self.file
            .read_exact_at(&mut buf, self.object_base)
            .map_err(|e| Error::io(format!("coarse whole-object pread failed: {e}")))?;
        self.bytes_read
            .set(self.bytes_read.get() + buf.len() as u64);
        self.reads.set(self.reads.get() + 1);
        self.covered_lo.set(0);
        self.covered_hi.set(self.source_len);
        Ok(buf)
    }
}

// ---------------------------------------------------------------------------
// Public entry points (called by the CLI; all `pub(crate)` guts stay here)
// ---------------------------------------------------------------------------

/// Outcome of building the index for one field.
pub struct BuildReport {
    /// Detected format name.
    pub format: &'static str,
    /// Number of units (a partition of the source).
    pub units: usize,
    /// Native units (excluding gap fillers).
    pub native_units: usize,
    /// Encoded index bytes.
    pub index_bytes: u64,
    /// Whole-store regular-file bytes after building.
    pub store_bytes: u64,
    /// `index_bytes / store_bytes`.
    pub fraction: f64,
    /// Persisted index path.
    pub path: String,
}

fn dir_bytes(path: &Path) -> u64 {
    let mut total = 0u64;
    let Ok(rd) = std::fs::read_dir(path) else {
        return 0;
    };
    for entry in rd.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            total += dir_bytes(&entry.path());
        } else if let Ok(md) = entry.metadata() {
            total += md.len();
        }
    }
    total
}

/// Open the store, discover the authority, build and persist the coarse index.
pub fn build(
    store_dir: &Path,
    packed: bool,
    field_hex: &str,
    limits: Limits,
) -> Result<BuildReport> {
    let store = if packed {
        FieldStore::open_packed(store_dir)?
    } else {
        FieldStore::open(store_dir)?
    };
    let field_id = FieldId::from_hex(field_hex)?;
    let manifest = store.get_field(&field_id)?;
    let descriptor_path = store
        .descriptor_path(&Id::from_bytes(manifest.descriptor_id))
        .ok_or_else(|| Error::unsupported_feature("descriptor is not a filesystem file"))?;
    let authority = discover_authority(&descriptor_path, &manifest, limits)?.ok_or_else(|| {
        Error::unsupported_feature(
            "descriptor is not a verbatim single authority object; coarse index not built",
        )
    })?;
    let (format, units) = build_units(&authority.payload);
    let native_units = units
        .iter()
        .filter(|u| u.kind != UnitKind::Gap && u.kind != UnitKind::AuthorityLeaf)
        .count();
    let index = CoarseIndex {
        format,
        source_len: manifest.source_len,
        source_sha256: manifest.source_sha256,
        object_base: authority.object_base,
        object_count: authority.object_count,
        graph_ops: authority.graph_ops,
        units,
    };
    let encoded = index.encode();
    let path = index_path(store.root(), &manifest.descriptor_id);
    super::write_atomic(&path, &encoded)?;
    let store_bytes = dir_bytes(store.root());
    let index_bytes = encoded.len() as u64;
    Ok(BuildReport {
        format: format_name(format),
        units: index.units.len(),
        native_units,
        index_bytes,
        store_bytes,
        fraction: if store_bytes == 0 {
            0.0
        } else {
            index_bytes as f64 / store_bytes as f64
        },
        path: path.display().to_string(),
    })
}

/// One row of the coarse-read receipt.
pub struct ReadRow {
    /// The request line.
    pub request: String,
    /// `planned` (served from the index), `fallback` (full descriptor read), or
    /// `declined` (the observation itself declined).
    pub outcome: &'static str,
    /// Why a fallback happened, when it did.
    pub fallback_reason: String,
    /// Units fetched by the coarse source.
    pub units_read: usize,
    /// Real `pread` calls made by the coarse source.
    pub preads: u64,
    /// Real descriptor payload bytes fetched by the coarse source.
    pub unit_bytes: u64,
    /// The covered source span, if any.
    pub covered: Option<(u64, u64)>,
    /// Manifest bytes the observation fetched.
    pub manifest_bytes: u64,
    /// Hierarchical-index bytes the observation fetched.
    pub index_bytes: u64,
    /// Seed bytes the observation fetched.
    pub seed_bytes: u64,
    /// Persisted index file size (the directory the read pays).
    pub dir_bytes: u64,
    /// Honest total physical bytes for this lane.
    pub closure_bytes: u64,
    /// Bytes returned by the observation.
    pub answer_bytes: u64,
    /// Whether the answer matched the full-read answer.
    pub matches_full: bool,
}

fn same_answer(a: &super::provenance::FieldAnswer, b: &super::provenance::FieldAnswer) -> bool {
    a.value == b.value && a.basis == b.basis
}

/// Run each request through the coarse lane, falling back to the full read when
/// the plan cannot cover it. When `use_map` is set, the persisted dependency map
/// supplies the unit plan and a coverage miss falls back; otherwise the reader
/// discovers the closure and records it into the map.
pub fn read(
    store_dir: &Path,
    packed: bool,
    field_hex: &str,
    requests: &[(String, ObserveRequest)],
    use_map: bool,
    limits: Limits,
) -> Result<Vec<ReadRow>> {
    let mut store = if packed {
        FieldStore::open_packed(store_dir)?
    } else {
        FieldStore::open(store_dir)?
    };
    let field_id = FieldId::from_hex(field_hex)?;
    let manifest = store.get_field(&field_id)?;
    let descriptor_path = store
        .descriptor_path(&Id::from_bytes(manifest.descriptor_id))
        .ok_or_else(|| Error::unsupported_feature("descriptor is not a filesystem file"))?;
    let idx_path = index_path(store.root(), &manifest.descriptor_id);
    let map_path = map_path(store.root(), &manifest.descriptor_id);
    let mut map = load_map(&map_path);
    let index = std::fs::read(&idx_path)
        .ok()
        .and_then(|b| CoarseIndex::decode(&b, limits).ok());
    let dir_bytes = std::fs::metadata(&idx_path).map(|m| m.len()).unwrap_or(0);

    let mut rows = Vec::with_capacity(requests.len());
    for (line, req) in requests {
        let mut row = ReadRow {
            request: line.clone(),
            outcome: "fallback",
            fallback_reason: String::new(),
            units_read: 0,
            preads: 0,
            unit_bytes: 0,
            covered: None,
            manifest_bytes: 0,
            index_bytes: 0,
            seed_bytes: 0,
            dir_bytes,
            closure_bytes: 0,
            answer_bytes: 0,
            matches_full: false,
        };
        // The full-read answer is the ground truth every coarse lane is checked
        // against; it also measures the control closure.
        let mut cold = req.clone();
        cold.use_cache = false;
        let (full_answer, full_stats, _) =
            super::observe::observe(&mut store, &field_id, &cold, limits)?;

        let plan = match (&index, use_map) {
            (Some(_), true) => map.get(line).cloned(),
            _ => None,
        };
        let expand = !(use_map && plan.is_some());
        let outcome: std::result::Result<(), String> = match &index {
            None => Err("no coarse index for this field".into()),
            Some(ix) if ix.format == 0 => {
                Err("coarse index declines this format (no native units)".into())
            }
            Some(ix) => {
                let src = CoarseSource::new(&descriptor_path, ix, expand)?;
                if let Some(units) = &plan {
                    src.prefetch(units)?;
                }
                let res = super::observe::observe_with_source(
                    &mut store,
                    &manifest,
                    field_id,
                    &src,
                    (ix.object_count, ix.graph_ops),
                    &cold,
                    limits,
                );
                match res {
                    Ok((ans, stats, _)) if src.misses() == 0 => {
                        row.outcome = "planned";
                        row.units_read = src.touched_units().len();
                        row.preads = src.reads();
                        row.unit_bytes = src.bytes_read();
                        row.covered = src.covered_span();
                        row.manifest_bytes = stats.manifest_bytes_read;
                        row.index_bytes = stats.index_bytes_read;
                        row.seed_bytes = stats.seed_bytes_read;
                        row.answer_bytes = stats.bytes_returned;
                        row.closure_bytes = dir_bytes
                            .saturating_add(src.bytes_read())
                            .saturating_add(stats.manifest_bytes_read)
                            .saturating_add(stats.index_bytes_read)
                            .saturating_add(stats.seed_bytes_read);
                        row.matches_full = same_answer(&ans, &full_answer);
                        if !use_map {
                            map.insert(line.clone(), src.touched_units());
                        }
                        Ok(())
                    }
                    Ok(_) => Err("coverage miss (uncovered unit)".into()),
                    Err(e) => Err(format!("coarse lane declined: {e}")),
                }
            }
        };
        if let Err(reason) = outcome {
            // Honest fallback: the full read already produced the answer above.
            row.outcome = "fallback";
            row.fallback_reason = reason;
            row.manifest_bytes = full_stats.manifest_bytes_read;
            row.index_bytes = full_stats.index_bytes_read;
            row.seed_bytes = full_stats.seed_bytes_read;
            row.answer_bytes = full_stats.bytes_returned;
            row.closure_bytes = full_stats.bytes_read;
            row.matches_full = true;
        }
        rows.push(row);
    }
    if !use_map {
        save_map(&map_path, &map);
    }
    Ok(rows)
}

/// The control closure (whole-descriptor read) for the same requests.
pub fn full_read(
    store_dir: &Path,
    packed: bool,
    field_hex: &str,
    requests: &[(String, ObserveRequest)],
    limits: Limits,
) -> Result<Vec<ObserveStats>> {
    let mut store = if packed {
        FieldStore::open_packed(store_dir)?
    } else {
        FieldStore::open(store_dir)?
    };
    let field_id = FieldId::from_hex(field_hex)?;
    let mut out = Vec::with_capacity(requests.len());
    for (_, req) in requests {
        let mut cold = req.clone();
        cold.use_cache = false;
        let (_, stats, _) = super::observe::observe(&mut store, &field_id, &cold, limits)?;
        out.push(stats);
    }
    Ok(out)
}
