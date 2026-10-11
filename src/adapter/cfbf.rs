//! Bounded, representation-preserving CFBF / OLE2 (Compound File Binary Format)
//! physical-layer adapter (Phase 21.35).
//!
//! CFBF is the container behind legacy Word (`.doc`), Excel (`.xls`), PowerPoint
//! (`.ppt`), and Outlook message (`.msg`) files. Those *semantic* models are later
//! subphases; this adapter exposes the **compound-document tree faithfully** — never
//! un-OLE and re-OLE — so a source is preserved byte-for-byte while its header,
//! allocation tables, directory entries, and stream spans become queryable.
//!
//! ## What is authoritative
//!
//! Like every Wave-2 format, CFBF has **no *separate* package layer**: the exact leaf
//! is the whole source (a `DocumentExact`) and every derived observation is a bounded,
//! span-preserving (`Q_gen`) projection of it. Nothing is decoded, re-serialized, or
//! reassembled for exactness.
//!
//! ## What is parsed (MS-CFB)
//!
//! * the **512-byte header** at offset 0: the signature
//!   `D0 CF 11 E0 A1 B1 1A E1`, the minor/major version, the **byte order** (`FFFE`),
//!   the **sector shift** (512 / 4096) and the mini-sector shift, the number of
//!   FAT/DIFAT sectors, the first directory sector, the mini-stream cutoff, and the
//!   first MiniFAT/DIFAT sector with their declared counts;
//! * the **DIFAT** (the header's 109 entries followed by any chained DIFAT sectors)
//!   and the **FAT** array;
//! * the **directory** (128-byte entries): UTF-16LE names (preserved verbatim,
//!   including non-ASCII), object type (storage `1` / stream `2` / root `5`), color,
//!   the left/right/child sibling tree IDs, the start sector, and the stream size —
//!   with the red-black sibling/child tree reconstructed into an entry `parent` and
//!   `depth`;
//! * the **stream byte ranges**: streams ≥ the mini cutoff are FAT-chained regular
//!   sectors; smaller streams are MiniFAT-chained mini-sectors inside the **root
//!   entry's mini stream**; both resolve to an exact byte span (offset, length) in the
//!   original file;
//! * the **end-of-chain** markers (`ENDOFCHAIN` `FFFFFFFE`, `FREESECT` `FFFFFFFF`,
//!   `FATSECT` `FFFFFFFD`, `DIFSECT` `FFFFFFFC`).
//!
//! ## The boundary it cannot cross (honest)
//!
//! Detection is a **strong magic + header validation** test: the first 8 bytes must be
//! the CFBF signature **and** the header must be internally consistent — byte order
//! `FFFE`, a major version of 3 or 4, a sector shift of 9 or 12 consistent with the
//! major version, a mini-sector shift of 6, at least one FAT sector, and a first
//! directory sector that lies inside the file. A random signature-only blob, a
//! signature-prefixed blob whose header fields are zero/inconsistent, and prose all
//! stay [`crate::field::document_format::DocumentFormat::Opaque`]. A blob that carries
//! a *plausible header* but a malformed body is admitted (so its whole-source exact
//! leaf is unaffected) and its derived model then **declines typed**.
//!
//! ## Span caveat (honest)
//!
//! A stream's `data_span` is its exact `(offset, length)` when its sector chain is
//! **contiguous** (the overwhelmingly common case, and every fixture). When a chain is
//! *not* contiguous the model records the **bounding hull** with `contiguous == false`,
//! and `ExactBytes` for that stream declines typed rather than returning interleaved
//! foreign bytes. Nothing is silently guessed.

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model entries (defends the decoder against a hostile blob).
pub const MAX_MODEL_ENTRIES: u32 = 1 << 24;
/// Hard cap on a decoded name length (defends the decoder).
pub const MAX_MODEL_NAME_BYTES: u32 = 1 << 16;

/// The CFB signature at offset 0.
pub const SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

/// The header length (always 512 bytes of meaningful fields).
pub const HEADER_LEN: usize = 512;
/// One directory entry's length.
pub const DIR_ENTRY_LEN: usize = 128;
/// The number of FAT sector IDs held directly in the header.
pub const DIFAT_HEADER_ENTRIES: usize = 109;

/// The little-endian byte-order mark (`0xFFFE`).
pub const BYTE_ORDER_MARK: u16 = 0xFFFE;
/// The mini-sector shift this adapter accepts (64-byte mini sectors, mandated by MS-CFB).
pub const MINI_SECTOR_SHIFT: u16 = 6;

/// Sector marker: a free, unused sector.
pub const FREESECT: u32 = 0xFFFF_FFFF;
/// Sector marker: the end of a chain.
pub const ENDOFCHAIN: u32 = 0xFFFF_FFFE;
/// Sector marker: a FAT sector.
pub const FATSECT: u32 = 0xFFFF_FFFD;
/// Sector marker: a DIFAT sector.
pub const DIFSECT: u32 = 0xFFFF_FFFC;
/// The largest valid regular sector number.
pub const MAXREGSECT: u32 = 0xFFFF_FFFA;
/// Directory tree marker: no sibling/child.
pub const NO_STREAM: u32 = 0xFFFF_FFFF;
/// The parent sentinel for an entry not reached from the root tree.
pub const NO_PARENT: u32 = 0xFFFF_FFFF;

/// The recorded dialect: a CFB version-3 file (512-byte sectors).
pub const DIALECT_CFB3: u8 = 0;
/// The recorded dialect: a CFB version-4 file (4096-byte sectors).
pub const DIALECT_CFB4: u8 = 1;

/// Object type: an unallocated directory slot.
pub const OBJ_UNKNOWN: u8 = 0;
/// Object type: a storage (directory).
pub const OBJ_STORAGE: u8 = 1;
/// Object type: a stream (file).
pub const OBJ_STREAM: u8 = 2;
/// Object type: the root storage.
pub const OBJ_ROOT: u8 = 5;

/// Stable lower-case dialect name.
pub const fn dialect_name(dialect: u8) -> &'static str {
    match dialect {
        DIALECT_CFB4 => "cfb4",
        _ => "cfb3",
    }
}

/// A human-readable object-type name.
pub const fn obj_type_name(obj_type: u8) -> &'static str {
    match obj_type {
        OBJ_STORAGE => "storage",
        OBJ_STREAM => "stream",
        OBJ_ROOT => "root",
        _ => "unallocated",
    }
}

/// Match field: the entry name.
pub const FIELD_NAME: u8 = 0;

/// The stable match-field name.
pub const fn field_name(field: u8) -> &'static str {
    let _ = field;
    "name"
}

/// One directory entry with its verbatim metadata and exact source spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfbfEntry {
    /// Raw UTF-16LE name bytes (never decoded or normalized on the model path).
    pub name: Vec<u8>,
    /// `(offset, length)` of the name bytes in the source.
    pub name_span: (u64, u64),
    /// The declared name length in bytes (including the UTF-16 terminator).
    pub name_utf16_len: u16,
    /// The object type (`OBJ_*`).
    pub obj_type: u8,
    /// The red-black tree color (0 red / 1 black), verbatim.
    pub color: u8,
    /// The left sibling's directory index (or [`NO_STREAM`]).
    pub left: u32,
    /// The right sibling's directory index (or [`NO_STREAM`]).
    pub right: u32,
    /// The child sibling-tree root's directory index (or [`NO_STREAM`]).
    pub child: u32,
    /// The declared start sector (FAT sector, or mini-sector for a mini stream).
    pub start_sector: u32,
    /// The declared stream size in bytes.
    pub stream_size: u64,
    /// `(offset, length)` of this 128-byte directory entry in the source.
    pub entry_span: (u64, u64),
    /// `(offset, length)` of the resolved stream content (a bounding hull when
    /// `contiguous` is false); `(0, 0)` for a storage.
    pub data_span: (u64, u64),
    /// Whether the resolved chain's sectors are contiguous in the source.
    pub contiguous: bool,
    /// The number of sectors (or mini-sectors) the stream occupies.
    pub num_sectors: u32,
    /// The owning storage's directory index (or [`NO_PARENT`] for the root/orphans).
    pub parent: u32,
    /// The nesting depth (the root is depth 0).
    pub depth: u32,
}

/// The canonical derived CFBF model (the materialization of a `CfbfModel` node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfbfModel {
    /// The recorded dialect ([`DIALECT_CFB3`] or [`DIALECT_CFB4`]).
    pub dialect: u8,
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// `(offset, length)` of the header (`(0, 512)`).
    pub header_span: (u64, u64),
    /// The sector size in bytes (512 or 4096).
    pub sector_size: u32,
    /// The mini-sector size in bytes (64).
    pub mini_sector_size: u32,
    /// The mini-stream cutoff (streams smaller than this are MiniFAT-chained).
    pub mini_cutoff: u32,
    /// The minor version.
    pub minor: u16,
    /// The major version (3 or 4).
    pub major: u16,
    /// The declared FAT sector count.
    pub num_fat_sectors: u32,
    /// The declared MiniFAT sector count.
    pub num_minifat_sectors: u32,
    /// The declared DIFAT sector count.
    pub num_difat_sectors: u32,
    /// The first directory sector.
    pub first_dir_sector: u32,
    /// The first MiniFAT sector (or [`ENDOFCHAIN`]).
    pub first_minifat_sector: u32,
    /// The first DIFAT sector (or [`ENDOFCHAIN`]).
    pub first_difat_sector: u32,
    /// The ordered directory-entry list (directory index order).
    pub entries: Vec<CfbfEntry>,
}

impl CfbfModel {
    /// The directory entry at `index`, if present.
    pub fn entry(&self, index: u32) -> Option<&CfbfEntry> {
        self.entries.get(index as usize)
    }

    /// The number of stream entries (`OBJ_STREAM`) in the directory.
    pub fn stream_count(&self) -> u64 {
        self.entries
            .iter()
            .filter(|e| e.obj_type == OBJ_STREAM)
            .count() as u64
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let names: usize = self.entries.iter().map(|e| e.name.len()).sum();
        let mut out = Vec::with_capacity(64 + self.entries.len() * 96 + names);
        out.extend_from_slice(b"CFBF");
        out.push(MODEL_VERSION);
        out.push(self.dialect);
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&self.header_span.0.to_le_bytes());
        out.extend_from_slice(&self.header_span.1.to_le_bytes());
        out.extend_from_slice(&self.sector_size.to_le_bytes());
        out.extend_from_slice(&self.mini_sector_size.to_le_bytes());
        out.extend_from_slice(&self.mini_cutoff.to_le_bytes());
        out.extend_from_slice(&self.minor.to_le_bytes());
        out.extend_from_slice(&self.major.to_le_bytes());
        out.extend_from_slice(&self.num_fat_sectors.to_le_bytes());
        out.extend_from_slice(&self.num_minifat_sectors.to_le_bytes());
        out.extend_from_slice(&self.num_difat_sectors.to_le_bytes());
        out.extend_from_slice(&self.first_dir_sector.to_le_bytes());
        out.extend_from_slice(&self.first_minifat_sector.to_le_bytes());
        out.extend_from_slice(&self.first_difat_sector.to_le_bytes());
        out.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());
        for e in &self.entries {
            out.extend_from_slice(&(e.name.len() as u32).to_le_bytes());
            out.extend_from_slice(&e.name);
            out.extend_from_slice(&e.name_span.0.to_le_bytes());
            out.extend_from_slice(&e.name_span.1.to_le_bytes());
            out.extend_from_slice(&e.name_utf16_len.to_le_bytes());
            out.push(e.obj_type);
            out.push(e.color);
            out.extend_from_slice(&e.left.to_le_bytes());
            out.extend_from_slice(&e.right.to_le_bytes());
            out.extend_from_slice(&e.child.to_le_bytes());
            out.extend_from_slice(&e.start_sector.to_le_bytes());
            out.extend_from_slice(&e.stream_size.to_le_bytes());
            out.extend_from_slice(&e.entry_span.0.to_le_bytes());
            out.extend_from_slice(&e.entry_span.1.to_le_bytes());
            out.extend_from_slice(&e.data_span.0.to_le_bytes());
            out.extend_from_slice(&e.data_span.1.to_le_bytes());
            out.push(e.contiguous as u8);
            out.extend_from_slice(&e.num_sectors.to_le_bytes());
            out.extend_from_slice(&e.parent.to_le_bytes());
            out.extend_from_slice(&e.depth.to_le_bytes());
        }
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<CfbfModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"CFBF" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let dialect = r.u8()?;
        if dialect > DIALECT_CFB4 {
            return Err(corrupt("unknown dialect"));
        }
        let doc_len = r.u64()?;
        let header_span = (r.u64()?, r.u64()?);
        if header_span.0 > header_span.1 || header_span.1 > doc_len {
            return Err(corrupt("header span is outside the document"));
        }
        let sector_size = r.u32()?;
        let mini_sector_size = r.u32()?;
        let mini_cutoff = r.u32()?;
        let minor = r.u16()?;
        let major = r.u16()?;
        let num_fat_sectors = r.u32()?;
        let num_minifat_sectors = r.u32()?;
        let num_difat_sectors = r.u32()?;
        let first_dir_sector = r.u32()?;
        let first_minifat_sector = r.u32()?;
        let first_difat_sector = r.u32()?;
        if !matches!(major, 3 | 4) {
            return Err(corrupt("major version is not 3 or 4"));
        }
        if !matches!(sector_size, 512 | 4096) || mini_sector_size != 64 {
            return Err(corrupt("sector sizes are implausible"));
        }
        let count = r.u32()?;
        if count > MAX_MODEL_ENTRIES {
            return Err(corrupt("model entry count is implausible"));
        }
        let mut entries = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let name_len = r.u32()?;
            if name_len > MAX_MODEL_NAME_BYTES {
                return Err(corrupt("model name length is implausible"));
            }
            let name = r.bytes(name_len as usize)?.to_vec();
            if name.len() % 2 != 0 {
                return Err(corrupt("model name is not UTF-16LE aligned"));
            }
            let name_span = (r.u64()?, r.u64()?);
            let name_utf16_len = r.u16()?;
            let obj_type = r.u8()?;
            let color = r.u8()?;
            let left = r.u32()?;
            let right = r.u32()?;
            let child = r.u32()?;
            let start_sector = r.u32()?;
            let stream_size = r.u64()?;
            let entry_span = (r.u64()?, r.u64()?);
            let data_span = (r.u64()?, r.u64()?);
            let contiguous = r.u8()? != 0;
            let num_sectors = r.u32()?;
            let parent = r.u32()?;
            let depth = r.u32()?;
            if !matches!(obj_type, OBJ_UNKNOWN | OBJ_STORAGE | OBJ_STREAM | OBJ_ROOT) {
                return Err(corrupt("unknown object type"));
            }
            check_span(name_span, doc_len, "name span")?;
            check_span(entry_span, doc_len, "entry span")?;
            check_span(data_span, doc_len, "data span")?;
            if entry_span.1 != entry_span.0.saturating_add(DIR_ENTRY_LEN as u64) {
                return Err(corrupt("entry span is not one directory entry"));
            }
            entries.push(CfbfEntry {
                name,
                name_span,
                name_utf16_len,
                obj_type,
                color,
                left,
                right,
                child,
                start_sector,
                stream_size,
                entry_span,
                data_span,
                contiguous,
                num_sectors,
                parent,
                depth,
            });
        }
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        Ok(CfbfModel {
            dialect,
            doc_len,
            header_span,
            sector_size,
            mini_sector_size,
            mini_cutoff,
            minor,
            major,
            num_fat_sectors,
            num_minifat_sectors,
            num_difat_sectors,
            first_dir_sector,
            first_minifat_sector,
            first_difat_sector,
            entries,
        })
    }
}

fn check_span(span: (u64, u64), doc_len: u64, what: &'static str) -> Result<()> {
    if span.0 > span.1 || span.1 > doc_len {
        return Err(corrupt(format!("{what} is outside the document")));
    }
    Ok(())
}

/// The decoded name text of an entry (lossy UTF-16LE; for reports only).
pub fn entry_name_text(e: &CfbfEntry) -> String {
    let mut units: Vec<u16> = Vec::with_capacity(e.name.len() / 2);
    let mut j = 0usize;
    while j + 2 <= e.name.len() {
        units.push(u16::from_le_bytes([e.name[j], e.name[j + 1]]));
        j += 2;
    }
    String::from_utf16_lossy(&units)
}

/// The exact 128-byte directory entry bytes.
pub fn entry_bytes<'a>(source: &'a [u8], e: &CfbfEntry) -> Result<&'a [u8]> {
    slice(
        source,
        e.entry_span.0,
        e.entry_span.1,
        "entry span is outside the source",
    )
}

/// The exact stream content bytes. Declines typed for a non-contiguous chain (whose
/// model span is only a bounding hull) and for a non-stream entry.
pub fn stream_bytes<'a>(source: &'a [u8], e: &CfbfEntry) -> Result<&'a [u8]> {
    if e.obj_type != OBJ_STREAM && e.obj_type != OBJ_ROOT {
        return Err(Error::unsupported_feature(
            "a storage object has no stream bytes",
        ));
    }
    if !e.contiguous {
        return Err(Error::unsupported_feature(
            "a non-contiguous stream is not representable as a single exact span",
        ));
    }
    let end = e.data_span.0.saturating_add(e.stream_size);
    if e.stream_size == 0 {
        return Ok(&source[0..0]);
    }
    slice(
        source,
        e.data_span.0,
        end,
        "stream span is outside the source",
    )
}

/// The canonical `/`-joined path of an entry (built from the reconstructed tree).
pub fn entry_path(model: &CfbfModel, index: u32) -> String {
    let Some(e) = model.entry(index) else {
        return String::new();
    };
    if e.obj_type == OBJ_ROOT {
        return "/".to_string();
    }
    let mut parts: Vec<String> = Vec::new();
    let mut cur = index;
    let mut guard: u64 = 0;
    while let Some(c) = model.entry(cur) {
        parts.push(entry_name_text(c));
        let p = c.parent;
        if p == NO_PARENT || p as usize >= model.entries.len() {
            break;
        }
        cur = p;
        guard += 1;
        if guard > model.entries.len() as u64 {
            break;
        }
    }
    parts.reverse();
    format!("/{}", parts.join("/"))
}

/// A compact canonical text rendering of the directory (the common `text`
/// observation). Bounded by `max_bytes`.
pub fn canonical_text(model: &CfbfModel, _source: &[u8], max_bytes: u64) -> Result<String> {
    let mut out = String::new();
    for (i, e) in model.entries.iter().enumerate() {
        let kind = obj_type_name(e.obj_type);
        if e.obj_type == OBJ_UNKNOWN {
            out.push_str(&format!("{i}\t-\t{kind}\t0\n"));
        } else {
            let size = if e.obj_type == OBJ_STREAM {
                e.stream_size
            } else {
                0
            };
            out.push_str(&format!(
                "{i}\t{}\t{kind}\t{size}\n",
                entry_path(model, i as u32)
            ));
        }
        if out.len() as u64 > max_bytes {
            return Err(Error::resource_limit(format!(
                "cfbf canonical text exceeded the {max_bytes}-byte budget"
            )));
        }
    }
    Ok(out)
}

/// One lexical-match record from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfbfMatch {
    /// The directory entry index.
    pub entry: u32,
    /// [`FIELD_NAME`].
    pub field: u8,
    /// The match's exact source byte offset (the matched UTF-16 code units' span).
    pub start: u64,
    /// One past the match's last source byte.
    pub end: u64,
    /// The matched text.
    pub text: String,
}

/// A bounded, case-sensitive lexical search over every entry name.
pub fn find(model: &CfbfModel, pattern: &str, max_bytes: u64) -> Result<Vec<CfbfMatch>> {
    if pattern.is_empty() {
        return Ok(Vec::new());
    }
    let needle = pattern.as_bytes();
    let plen = pattern.encode_utf16().count() as u64;
    let mut out: Vec<CfbfMatch> = Vec::new();
    let mut estimated: u64 = 0;
    for (i, e) in model.entries.iter().enumerate() {
        if e.obj_type == OBJ_UNKNOWN {
            continue;
        }
        let name = entry_name_text(e);
        let bytes = name.as_bytes();
        let mut at = 0usize;
        while at + needle.len() <= bytes.len() {
            if &bytes[at..at + needle.len()] == needle {
                estimated = estimated.saturating_add(64 + needle.len() as u64);
                if estimated > max_bytes {
                    return Err(Error::resource_limit(format!(
                        "cfbf find exceeded the {max_bytes}-byte budget"
                    )));
                }
                let units = name[..at].encode_utf16().count() as u64;
                let start = e.name_span.0.saturating_add(units.saturating_mul(2));
                let end = start.saturating_add(plen.saturating_mul(2));
                out.push(CfbfMatch {
                    entry: i as u32,
                    field: FIELD_NAME,
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

/// The parsed header fields.
struct Header {
    minor: u16,
    major: u16,
    byte_order: u16,
    sector_shift: u16,
    mini_shift: u16,
    num_dir_sectors: u32,
    num_fat_sectors: u32,
    first_dir_sector: u32,
    mini_cutoff: u32,
    first_minifat_sector: u32,
    num_minifat_sectors: u32,
    first_difat_sector: u32,
    num_difat_sectors: u32,
    difat: [u32; DIFAT_HEADER_ENTRIES],
}

/// Parse the 512-byte header. Bounds-checked; requires `b.len() >= 512`.
fn parse_header(b: &[u8]) -> Result<Header> {
    if b.len() < HEADER_LEN {
        return Err(invalid("header is truncated"));
    }
    if !b.starts_with(&SIGNATURE) {
        return Err(invalid("signature mismatch"));
    }
    let mut difat = [0u32; DIFAT_HEADER_ENTRIES];
    for (i, slot) in difat.iter_mut().enumerate() {
        *slot = rd_u32(b, 76 + i * 4).ok_or_else(|| invalid("header is truncated"))?;
    }
    Ok(Header {
        minor: rd_u16(b, 24).ok_or_else(|| invalid("header is truncated"))?,
        major: rd_u16(b, 26).ok_or_else(|| invalid("header is truncated"))?,
        byte_order: rd_u16(b, 28).ok_or_else(|| invalid("header is truncated"))?,
        sector_shift: rd_u16(b, 30).ok_or_else(|| invalid("header is truncated"))?,
        mini_shift: rd_u16(b, 32).ok_or_else(|| invalid("header is truncated"))?,
        num_dir_sectors: rd_u32(b, 40).ok_or_else(|| invalid("header is truncated"))?,
        num_fat_sectors: rd_u32(b, 44).ok_or_else(|| invalid("header is truncated"))?,
        first_dir_sector: rd_u32(b, 48).ok_or_else(|| invalid("header is truncated"))?,
        mini_cutoff: rd_u32(b, 56).ok_or_else(|| invalid("header is truncated"))?,
        first_minifat_sector: rd_u32(b, 60).ok_or_else(|| invalid("header is truncated"))?,
        num_minifat_sectors: rd_u32(b, 64).ok_or_else(|| invalid("header is truncated"))?,
        first_difat_sector: rd_u32(b, 68).ok_or_else(|| invalid("header is truncated"))?,
        num_difat_sectors: rd_u32(b, 72).ok_or_else(|| invalid("header is truncated"))?,
        difat,
    })
}

/// The sector size implied by a shift, or `None` when the shift is not 9/12.
fn sector_size_of(shift: u16) -> Option<u32> {
    match shift {
        9 => Some(512),
        12 => Some(4096),
        _ => None,
    }
}

/// The number of complete sectors after the header.
fn total_sectors(len: u64, ss: u32) -> u64 {
    let ss = ss as u64;
    if len < ss { 0 } else { (len - ss) / ss }
}

/// Validate the header's internal consistency (the detection contract).
fn header_consistent(h: &Header, len: u64) -> bool {
    if h.byte_order != BYTE_ORDER_MARK {
        return false;
    }
    if !matches!(h.major, 3 | 4) {
        return false;
    }
    let Some(ss) = sector_size_of(h.sector_shift) else {
        return false;
    };
    // The sector shift must match the major version (v3 = 512, v4 = 4096).
    if (h.major == 3) != (ss == 512) {
        return false;
    }
    if h.mini_shift != MINI_SECTOR_SHIFT {
        return false;
    }
    if h.num_fat_sectors == 0 {
        return false;
    }
    // `num_dir_sectors` is 0 for v3; v4 declares a positive count.
    if h.major == 3 && h.num_dir_sectors != 0 {
        return false;
    }
    if h.major == 4 && h.num_dir_sectors == 0 {
        return false;
    }
    // The first directory sector must be a real sector inside the file.
    let total = total_sectors(len, ss);
    if h.first_dir_sector > MAXREGSECT || h.first_dir_sector as u64 >= total {
        return false;
    }
    true
}

/// Fast-fail CFBF detector: a CFBF signature plus a fully consistent header. Never
/// consults a file name.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.len() < HEADER_LEN || source.len() as u64 > limits.max_cfbf_document_bytes {
        return false;
    }
    let Ok(h) = parse_header(source) else {
        return false;
    };
    header_consistent(&h, source.len() as u64)
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse `source` into a [`CfbfModel`] under `limits`. Declines typed on any
/// malformed or over-large input; never panics.
pub fn parse(source: &[u8], limits: Limits) -> Result<CfbfModel> {
    let len = source.len() as u64;
    if len > limits.max_cfbf_document_bytes {
        return Err(Error::resource_limit(format!(
            "cfbf source is {len} bytes, above the {}-byte document cap",
            limits.max_cfbf_document_bytes
        )));
    }
    let h = parse_header(source)?;
    if !header_consistent(&h, len) {
        return Err(invalid("header is not internally consistent"));
    }
    let ss = sector_size_of(h.sector_shift).ok_or_else(|| invalid("unsupported sector shift"))?;
    if !h.mini_cutoff.is_power_of_two() || h.mini_cutoff == 0 {
        return Err(invalid("mini-stream cutoff is not a power of two"));
    }
    let mini_ss: u32 = 1 << h.mini_shift;
    let total = total_sectors(len, ss);
    let entries_per_sector = (ss / 4) as usize;

    // ---- FAT sector list (DIFAT) -----------------------------------------
    let want_fat = h.num_fat_sectors as usize;
    let fat_entries = want_fat
        .checked_mul(entries_per_sector)
        .ok_or_else(|| invalid("FAT size overflows"))?;
    if fat_entries as u64 > limits.max_cfbf_chain_sectors {
        return Err(Error::resource_limit(format!(
            "cfbf FAT of {fat_entries} entries exceeds max_cfbf_chain_sectors {}",
            limits.max_cfbf_chain_sectors
        )));
    }
    let mut used_sectors = vec![false; total as usize];
    let mut fat_ids: Vec<u32> = Vec::with_capacity(want_fat.min(4096));
    for &id in h.difat.iter() {
        if fat_ids.len() >= want_fat {
            break;
        }
        if id == FREESECT {
            break;
        }
        if id == ENDOFCHAIN || id == FATSECT || id == DIFSECT {
            return Err(invalid("invalid FAT sector ID in the header DIFAT"));
        }
        if id as u64 >= total {
            return Err(invalid("FAT sector ID is out of range"));
        }
        fat_ids.push(id);
    }
    if fat_ids.len() < want_fat {
        // Continue through the chained DIFAT sectors.
        if h.num_difat_sectors == 0 {
            return Err(invalid("FAT sectors are not covered by the DIFAT"));
        }
        let mut s = h.first_difat_sector;
        let mut seen: u64 = 0;
        while fat_ids.len() < want_fat {
            if seen >= h.num_difat_sectors as u64 || s == ENDOFCHAIN {
                return Err(invalid("DIFAT chain is truncated"));
            }
            if s > MAXREGSECT || s as u64 >= total {
                return Err(invalid("DIFAT sector ID is out of range"));
            }
            if used_sectors[s as usize] {
                return Err(invalid("cyclic DIFAT chain"));
            }
            used_sectors[s as usize] = true;
            let sect = sector_slice(source, s, ss, total)?;
            for j in 0..entries_per_sector.saturating_sub(1) {
                if fat_ids.len() >= want_fat {
                    break;
                }
                let id = u32::from_le_bytes([
                    sect[j * 4],
                    sect[j * 4 + 1],
                    sect[j * 4 + 2],
                    sect[j * 4 + 3],
                ]);
                if id == FREESECT {
                    break;
                }
                if id == ENDOFCHAIN || id == FATSECT || id == DIFSECT || id as u64 >= total {
                    return Err(invalid("invalid FAT sector ID in a DIFAT sector"));
                }
                fat_ids.push(id);
            }
            let last = (entries_per_sector - 1) * 4;
            s = u32::from_le_bytes([sect[last], sect[last + 1], sect[last + 2], sect[last + 3]]);
            seen += 1;
        }
    }
    if fat_ids.len() != want_fat {
        return Err(invalid("declared FAT sector count is inconsistent"));
    }

    // ---- Read the FAT ----------------------------------------------------
    let mut fat: Vec<u32> = Vec::with_capacity(fat_entries);
    for &id in &fat_ids {
        if used_sectors[id as usize] {
            return Err(invalid("overlapping FAT sector"));
        }
        used_sectors[id as usize] = true;
        let sect = sector_slice(source, id, ss, total)?;
        let mut j = 0usize;
        while j + 4 <= sect.len() {
            fat.push(u32::from_le_bytes([
                sect[j],
                sect[j + 1],
                sect[j + 2],
                sect[j + 3],
            ]));
            j += 4;
        }
    }

    // ---- Directory -------------------------------------------------------
    let max_chain = limits.max_cfbf_chain_sectors;
    let dir_chain = follow_until_end(
        h.first_dir_sector,
        &fat,
        total as usize,
        &mut used_sectors,
        max_chain,
        "directory",
    )?;
    if h.major == 4 && (dir_chain.len() as u32) < h.num_dir_sectors {
        return Err(invalid("directory chain is shorter than declared"));
    }
    let mut dir_bytes: Vec<u8> = Vec::with_capacity(dir_chain.len() * ss as usize);
    for &s in &dir_chain {
        dir_bytes.extend_from_slice(sector_slice(source, s, ss, total)?);
    }
    let entry_count = dir_bytes.len() / DIR_ENTRY_LEN;
    if entry_count == 0 {
        return Err(invalid("directory has no entries"));
    }
    if entry_count as u64 > limits.max_cfbf_entries {
        return Err(Error::resource_limit(format!(
            "cfbf directory has {entry_count} entries, exceeding max_cfbf_entries {}",
            limits.max_cfbf_entries
        )));
    }

    // The directory's absolute start offset: the first directory sector is contiguous
    // only if its chain is; derive each entry's exact span from the sector map.
    let dir_sector_off = dir_chain
        .iter()
        .map(|&s| sector_off(s, ss))
        .collect::<Vec<_>>();
    // A 128-byte entry never straddles a sector (ss is 512 or 4096; both divide by
    // 128), so an entry's index maps to exactly one directory sector.
    let entries_per_dir_sector = ss as usize / DIR_ENTRY_LEN;

    let mut entries: Vec<CfbfEntry> = Vec::with_capacity(entry_count);
    let mut name_bytes: u64 = 0;
    for i in 0..entry_count {
        let base = i * DIR_ENTRY_LEN;
        let slot = &dir_bytes[base..base + DIR_ENTRY_LEN];
        let dir_sector = i / entries_per_dir_sector;
        let intra = (i % entries_per_dir_sector) * DIR_ENTRY_LEN;
        let entry_off = dir_sector_off[dir_sector] + intra as u64;
        let obj_type = slot[66];
        if !matches!(obj_type, OBJ_UNKNOWN | OBJ_STORAGE | OBJ_STREAM | OBJ_ROOT) {
            return Err(invalid(format!(
                "directory entry {i} has an invalid object type {obj_type}"
            )));
        }
        let raw_len = u16::from_le_bytes([slot[64], slot[65]]);
        let (raw_name_len, name_used): (u16, usize) = if obj_type == OBJ_UNKNOWN {
            if raw_len <= 64 && raw_len.is_multiple_of(2) {
                (raw_len, raw_len.saturating_sub(2) as usize)
            } else {
                (0, 0)
            }
        } else {
            if raw_len > 64 || !raw_len.is_multiple_of(2) {
                return Err(invalid(format!(
                    "directory entry {i} has an invalid name length {raw_len}"
                )));
            }
            (
                raw_len,
                if raw_len >= 2 {
                    (raw_len - 2) as usize
                } else {
                    0
                },
            )
        };
        let name = if obj_type == OBJ_UNKNOWN {
            Vec::new()
        } else {
            slot[..name_used].to_vec()
        };
        name_bytes = name_bytes.saturating_add(name_utf16_length(raw_len) as u64);
        if name_bytes > limits.max_cfbf_name_bytes {
            return Err(Error::resource_limit(format!(
                "cfbf name bytes exceed max_cfbf_name_bytes {}",
                limits.max_cfbf_name_bytes
            )));
        }
        if name.len() as u64 > limits.max_cfbf_entry_name_bytes {
            return Err(Error::resource_limit(format!(
                "cfbf entry name is longer than max_cfbf_entry_name_bytes {}",
                limits.max_cfbf_entry_name_bytes
            )));
        }
        let name_span = (entry_off, entry_off + name_used as u64);
        entries.push(CfbfEntry {
            name,
            name_span,
            name_utf16_len: raw_name_len,
            obj_type,
            color: slot[67],
            left: u32::from_le_bytes([slot[68], slot[69], slot[70], slot[71]]),
            right: u32::from_le_bytes([slot[72], slot[73], slot[74], slot[75]]),
            child: u32::from_le_bytes([slot[76], slot[77], slot[78], slot[79]]),
            start_sector: u32::from_le_bytes([slot[116], slot[117], slot[118], slot[119]]),
            stream_size: stream_size_of(slot, h.major),
            entry_span: (entry_off, entry_off + DIR_ENTRY_LEN as u64),
            data_span: (0, 0),
            contiguous: true,
            num_sectors: 0,
            parent: NO_PARENT,
            depth: 0,
        });
    }
    if entries[0].obj_type != OBJ_ROOT {
        return Err(invalid("directory entry 0 is not the root storage"));
    }

    // ---- Reconstruct the red-black sibling/child tree --------------------
    build_tree(&mut entries, limits.max_cfbf_depth, max_chain)?;

    // ---- Resolve the mini stream and the MiniFAT -------------------------
    let (root_start, root_size) = (entries[0].start_sector, entries[0].stream_size);
    let (mini_span, mini_contig, mini_chain) = resolve_regular(
        source,
        root_start,
        root_size,
        ss,
        total,
        &fat,
        &mut used_sectors,
        max_chain,
        "mini-stream",
    )?;
    entries[0].data_span = mini_span;
    entries[0].contiguous = mini_contig;
    entries[0].num_sectors = mini_chain.len() as u32;
    let mini_stream_size = root_size;

    let minifat: Vec<u32> = if h.num_minifat_sectors == 0 {
        Vec::new()
    } else {
        let mf_chain = follow_chain(
            h.first_minifat_sector,
            h.num_minifat_sectors as usize,
            &fat,
            total as usize,
            &mut used_sectors,
            max_chain,
            "minifat",
        )?;
        if mf_chain.len() != h.num_minifat_sectors as usize {
            return Err(invalid("MiniFAT chain is shorter than declared"));
        }
        let mut mf = Vec::with_capacity(mf_chain.len() * entries_per_sector);
        for &s in &mf_chain {
            let sect = sector_slice(source, s, ss, total)?;
            let mut j = 0usize;
            while j + 4 <= sect.len() {
                mf.push(u32::from_le_bytes([
                    sect[j],
                    sect[j + 1],
                    sect[j + 2],
                    sect[j + 3],
                ]));
                j += 4;
            }
        }
        mf
    };
    let mut used_mini = vec![false; minifat.len()];

    // ---- Resolve every stream -------------------------------------------
    let mut stream_count: u64 = 0;
    for e in entries.iter_mut().skip(1) {
        if e.obj_type != OBJ_STREAM {
            continue;
        }
        stream_count += 1;
        if stream_count > limits.max_cfbf_streams {
            return Err(Error::resource_limit(format!(
                "cfbf stream count exceeds max_cfbf_streams {}",
                limits.max_cfbf_streams
            )));
        }
        let (start, size) = (e.start_sector, e.stream_size);
        if size < h.mini_cutoff as u64 {
            if minifat.is_empty() && size > 0 {
                return Err(invalid("a mini stream exists but there is no MiniFAT"));
            }
            let (span, contig, chain) = resolve_mini(
                source,
                start,
                size,
                mini_ss,
                &minifat,
                &mut used_mini,
                &mini_chain,
                ss,
                mini_stream_size,
                max_chain,
                "stream",
            )?;
            e.data_span = span;
            e.contiguous = contig;
            e.num_sectors = chain.len() as u32;
        } else {
            let (span, contig, chain) = resolve_regular(
                source,
                start,
                size,
                ss,
                total,
                &fat,
                &mut used_sectors,
                max_chain,
                "stream",
            )?;
            e.data_span = span;
            e.contiguous = contig;
            e.num_sectors = chain.len() as u32;
        }
    }

    let dialect = if h.major == 4 {
        DIALECT_CFB4
    } else {
        DIALECT_CFB3
    };

    Ok(CfbfModel {
        dialect,
        doc_len: len,
        header_span: (0, HEADER_LEN as u64),
        sector_size: ss,
        mini_sector_size: mini_ss,
        mini_cutoff: h.mini_cutoff,
        minor: h.minor,
        major: h.major,
        num_fat_sectors: h.num_fat_sectors,
        num_minifat_sectors: h.num_minifat_sectors,
        num_difat_sectors: h.num_difat_sectors,
        first_dir_sector: h.first_dir_sector,
        first_minifat_sector: h.first_minifat_sector,
        first_difat_sector: h.first_difat_sector,
        entries,
    })
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of a `CfbfModel` node).
pub fn build_cfbf_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits)?.encode())
}

/// The byte length of a declared UTF-16 name (excluding the terminator).
fn name_utf16_length(raw_len: u16) -> u16 {
    raw_len.saturating_sub(2)
}

/// The effective stream size (the upper 32 bits are ignored for a v3 file).
fn stream_size_of(slot: &[u8], major: u16) -> u64 {
    let lo = u32::from_le_bytes([slot[120], slot[121], slot[122], slot[123]]) as u64;
    if major == 3 {
        lo
    } else {
        lo | ((u32::from_le_bytes([slot[124], slot[125], slot[126], slot[127]]) as u64) << 32)
    }
}

/// Reconstruct the sibling/child tree into `parent`/`depth`, detecting cycles.
fn build_tree(entries: &mut [CfbfEntry], max_depth: u32, max_chain: u64) -> Result<()> {
    let n = entries.len();
    let mut visited = vec![false; n];
    visited[0] = true;
    entries[0].parent = NO_PARENT;
    entries[0].depth = 0;

    // A work queue of (storage index, child depth).
    let mut work: Vec<(u32, u32)> = Vec::new();
    work.push((0, 1));
    let mut guard: u64 = 0;
    while let Some((storage, depth)) = work.pop() {
        if depth > max_depth {
            return Err(Error::resource_limit(format!(
                "cfbf directory depth exceeds max_cfbf_depth {max_depth}"
            )));
        }
        let child = entries[storage as usize].child;
        if child == NO_STREAM {
            continue;
        }
        let mut pending: Vec<u32> = vec![child];
        while let Some(id) = pending.pop() {
            if id == NO_STREAM {
                continue;
            }
            guard += 1;
            if guard > max_chain {
                return Err(invalid("directory tree traversal exceeds the cap"));
            }
            if id as usize >= n {
                return Err(invalid("directory tree references an out-of-range entry"));
            }
            if visited[id as usize] {
                return Err(invalid("directory tree contains a cycle"));
            }
            visited[id as usize] = true;
            entries[id as usize].parent = storage;
            entries[id as usize].depth = depth;
            let e = &entries[id as usize];
            if e.obj_type == OBJ_STORAGE {
                work.push((id, depth + 1));
            }
            let (l, r) = (e.left, e.right);
            if l != NO_STREAM {
                pending.push(l);
            }
            if r != NO_STREAM {
                pending.push(r);
            }
        }
    }
    Ok(())
}

/// Follow a chain of `needed` sectors (or mini-sectors), declining on a special
/// sector, an out-of-range id, a cycle, or an overlap with an already-used sector.
fn follow_chain(
    start: u32,
    needed: usize,
    table: &[u32],
    bound: usize,
    used: &mut [bool],
    max_chain: u64,
    what: &'static str,
) -> Result<Vec<u32>> {
    let mut chain: Vec<u32> = Vec::with_capacity(needed.min(4096));
    let mut s = start;
    let mut guard: u64 = 0;
    while chain.len() < needed {
        if s == ENDOFCHAIN {
            break;
        }
        if s > MAXREGSECT {
            return Err(invalid(format!("{what}: special sector mid-chain")));
        }
        let idx = s as usize;
        if idx >= bound || idx >= table.len() || idx >= used.len() {
            return Err(invalid(format!("{what}: sector {s} is out of range")));
        }
        if used[idx] {
            return Err(invalid(format!("{what}: overlapping or cyclic chain")));
        }
        used[idx] = true;
        chain.push(s);
        guard += 1;
        if guard > max_chain {
            return Err(invalid(format!("{what}: chain exceeds the sector cap")));
        }
        s = table[idx];
    }
    Ok(chain)
}

/// Follow a chain to its `ENDOFCHAIN` terminator (bounded); used for the directory.
fn follow_until_end(
    start: u32,
    table: &[u32],
    bound: usize,
    used: &mut [bool],
    max_chain: u64,
    what: &'static str,
) -> Result<Vec<u32>> {
    let mut chain: Vec<u32> = Vec::new();
    let mut s = start;
    let mut guard: u64 = 0;
    while s != ENDOFCHAIN {
        if s > MAXREGSECT {
            return Err(invalid(format!("{what}: special sector mid-chain")));
        }
        let idx = s as usize;
        if idx >= bound || idx >= table.len() || idx >= used.len() {
            return Err(invalid(format!("{what}: sector {s} is out of range")));
        }
        if used[idx] {
            return Err(invalid(format!("{what}: overlapping or cyclic chain")));
        }
        used[idx] = true;
        chain.push(s);
        guard += 1;
        if guard > max_chain {
            return Err(invalid(format!("{what}: chain exceeds the sector cap")));
        }
        s = table[idx];
    }
    Ok(chain)
}

/// Resolve a regular (FAT-chained) stream to its exact span, contiguity, and chain.
#[allow(clippy::too_many_arguments)]
fn resolve_regular(
    _source: &[u8],
    start: u32,
    size: u64,
    ss: u32,
    total: u64,
    fat: &[u32],
    used: &mut [bool],
    max_chain: u64,
    what: &'static str,
) -> Result<((u64, u64), bool, Vec<u32>)> {
    if size == 0 {
        return Ok(((0, 0), true, Vec::new()));
    }
    let needed = size.div_ceil(ss as u64) as usize;
    let chain = follow_chain(start, needed, fat, total as usize, used, max_chain, what)?;
    if chain.len() != needed {
        return Err(invalid(format!(
            "{what}: chain is shorter than the stream size"
        )));
    }
    let (span, contig) = span_of_chain(&chain, ss, size, total);
    Ok((span, contig, chain))
}

/// Resolve a MiniFAT-chained stream inside the root mini stream.
#[allow(clippy::too_many_arguments)]
fn resolve_mini(
    _source: &[u8],
    start: u32,
    size: u64,
    mini_ss: u32,
    minifat: &[u32],
    used_mini: &mut [bool],
    mini_chain: &[u32],
    ss: u32,
    mini_stream_size: u64,
    max_chain: u64,
    what: &'static str,
) -> Result<((u64, u64), bool, Vec<u32>)> {
    if size == 0 {
        return Ok(((0, 0), true, Vec::new()));
    }
    let needed = size.div_ceil(mini_ss as u64) as usize;
    let chain = follow_chain(
        start,
        needed,
        minifat,
        minifat.len(),
        used_mini,
        max_chain,
        what,
    )?;
    if chain.len() != needed {
        return Err(invalid(format!(
            "{what}: mini chain is shorter than the stream size"
        )));
    }
    let mini_ss_u = mini_ss as u64;
    let ss_u = ss as u64;
    let mut first = 0u64;
    let mut min_off = u64::MAX;
    let mut max_end = 0u64;
    let mut prev_end: Option<u64> = None;
    let mut contiguous = true;
    let mut remaining = size;
    for (i, &m) in chain.iter().enumerate() {
        let logical = m as u64 * mini_ss_u;
        if logical + mini_ss_u > mini_stream_size {
            return Err(invalid(format!(
                "{what}: mini sector is outside the mini stream"
            )));
        }
        let k = (logical / ss_u) as usize;
        let intra = logical % ss_u;
        if k >= mini_chain.len() {
            return Err(invalid(format!(
                "{what}: mini sector maps outside the mini stream"
            )));
        }
        let abs = sector_off(mini_chain[k], ss) + intra;
        if i == 0 {
            first = abs;
        }
        min_off = min_off.min(abs);
        let clen = remaining.min(mini_ss_u);
        let end = abs + clen;
        if let Some(pe) = prev_end
            && pe != abs
        {
            contiguous = false;
        }
        prev_end = Some(end);
        max_end = max_end.max(end);
        remaining -= clen;
    }
    let span = if contiguous {
        (first, first + size)
    } else {
        (min_off, max_end)
    };
    Ok((span, contiguous, chain))
}

/// Compute the span and contiguity of a sector chain holding `size` bytes.
fn span_of_chain(chain: &[u32], ss: u32, size: u64, _total: u64) -> ((u64, u64), bool) {
    let ss_u = ss as u64;
    let mut first = 0u64;
    let mut min_off = u64::MAX;
    let mut max_end = 0u64;
    let mut prev_end: Option<u64> = None;
    let mut contiguous = true;
    let mut remaining = size;
    for (i, &s) in chain.iter().enumerate() {
        let abs = sector_off(s, ss);
        if i == 0 {
            first = abs;
        }
        min_off = min_off.min(abs);
        let clen = remaining.min(ss_u);
        let end = abs + clen;
        if let Some(pe) = prev_end
            && pe != abs
        {
            contiguous = false;
        }
        prev_end = Some(end);
        max_end = max_end.max(end);
        remaining -= clen;
    }
    let span = if contiguous {
        (first, first + size)
    } else {
        (min_off, max_end)
    };
    (span, contiguous)
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn sector_off(s: u32, ss: u32) -> u64 {
    (s as u64 + 1) * ss as u64
}

fn sector_slice(source: &[u8], s: u32, ss: u32, total: u64) -> Result<&[u8]> {
    if s > MAXREGSECT || s as u64 >= total {
        return Err(invalid("sector is out of range"));
    }
    let off = sector_off(s, ss);
    let end = off + ss as u64;
    slice(source, off, end, "sector is outside the source")
}

fn slice<'a>(source: &'a [u8], start: u64, end: u64, why: &'static str) -> Result<&'a [u8]> {
    let s = usize::try_from(start).map_err(|_| invalid(why))?;
    let e = usize::try_from(end).map_err(|_| invalid(why))?;
    source.get(s..e).ok_or_else(|| invalid(why))
}

fn rd_u16(b: &[u8], off: usize) -> Option<u16> {
    let s = b.get(off..off + 2)?;
    Some(u16::from_le_bytes([s[0], s[1]]))
}

fn rd_u32(b: &[u8], off: usize) -> Option<u32> {
    let s = b.get(off..off + 4)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn invalid(message: impl Into<String>) -> Error {
    Error::invalid_cfbf_structure(format!("cfbf: {}", message.into()))
}

fn corrupt(message: impl Into<String>) -> Error {
    Error::invalid_cfbf_structure(format!("cfbf model: {}", message.into()))
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
    fn dialect_and_kind_names_are_stable() {
        assert_eq!(dialect_name(DIALECT_CFB3), "cfb3");
        assert_eq!(dialect_name(DIALECT_CFB4), "cfb4");
        assert_eq!(obj_type_name(OBJ_STORAGE), "storage");
        assert_eq!(obj_type_name(OBJ_STREAM), "stream");
        assert_eq!(obj_type_name(OBJ_ROOT), "root");
        assert_eq!(obj_type_name(OBJ_UNKNOWN), "unallocated");
    }

    #[test]
    fn prose_is_not_cfbf() {
        assert!(!detect(
            b"The quick brown fox jumps over the lazy dog.\nPlain prose.\n",
            Limits::DEFAULT
        ));
    }

    #[test]
    fn signature_only_blob_is_not_cfbf() {
        let mut blob = SIGNATURE.to_vec();
        assert!(!detect(&blob, Limits::DEFAULT));
        blob.resize(HEADER_LEN, 0);
        assert!(!detect(&blob, Limits::DEFAULT));
    }

    #[test]
    fn bad_sector_shift_is_not_cfbf() {
        let mut blob = vec![0u8; HEADER_LEN];
        blob[..8].copy_from_slice(&SIGNATURE);
        blob[26..28].copy_from_slice(&3u16.to_le_bytes());
        blob[28..30].copy_from_slice(&BYTE_ORDER_MARK.to_le_bytes());
        blob[30..32].copy_from_slice(&7u16.to_le_bytes()); // sector shift 7
        blob[32..34].copy_from_slice(&MINI_SECTOR_SHIFT.to_le_bytes());
        blob[44..48].copy_from_slice(&1u32.to_le_bytes());
        assert!(!detect(&blob, Limits::DEFAULT));
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
        for _ in 0..2048 {
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
        // The signature-prefixed path is the most likely false-positive route.
        for _ in 0..1024 {
            let mut buf = SIGNATURE.to_vec();
            buf.resize(1024, 0);
            for b in buf[8..].iter_mut() {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                *b = (seed >> 33) as u8;
            }
            let _ = detect(&buf, Limits::DEFAULT);
            let _ = parse(&buf, Limits::STRICT);
        }
    }
}
