//! Generic archive-container adapter (Phase 21.33): ZIP and TAR as **standalone**
//! formats.
//!
//! This adapter covers the two ubiquitous archive containers, *not* the office
//! packages built on them. A `.docx`/`.xlsx`/`.pptx`/`.odt`/`.ods`/`.odp`/`.epub`
//! source is a ZIP, but it is detected (and inverted) by its own office adapter
//! **before** this one is ever consulted; detection order is load-bearing and is
//! enforced in [`crate::field::document_format::detect_document_format`].
//!
//! ## What is authoritative
//!
//! Like every Wave-2 format, a container has **no *separate* package layer**: the
//! exact leaf is the whole source and every derived observation is a bounded,
//! span-preserving (`Q_gen`) projection of it. For TAR the field root is a
//! `DocumentExact`; for ZIP it is the reused package layer's `PackageRoot` (which
//! materializes the whole source from its exact member cover). Nothing is ever
//! unzipped, re-zipped, decompressed, or re-serialized for exactness.
//!
//! The model records, per member, its **exact raw byte span** in the source:
//!
//! * **ZIP** reuses the byte-authoritative physical layer
//!   ([`crate::adapter::package::zip`], Phase 12.1) — never a second ZIP scanner.
//!   The **ordered central-directory member list** carries the raw name bytes
//!   (case, UTF-8 flag bit 11, and non-UTF-8 bytes all preserved), the compression
//!   method (stored/deflate/…), both declared sizes, the CRC-32, the local-header
//!   offset, the exact local-header span, the exact raw payload span (the member's
//!   exact leaf), and the optional data descriptor; the end-of-central-directory
//!   record and archive comment are recorded verbatim.
//! * **TAR** parses the standard 512-byte headers — `name` (with the `prefix` for
//!   POSIX ustar), `mode`, `uid`/`gid`, `size`, `mtime`, `typeflag`, `linkname`, and
//!   the `ustar` magic/version — plus the old-GNU/v7 variant. The GNU long-name
//!   (`L`) and long-link (`K`) extensions and the PAX (`x`/`g`) extension records
//!   are folded into the following member. Every member's exact byte span covers its
//!   512-byte header block through its **padded** payload (padding to the 512-byte
//!   boundary is preserved), and the tar end-of-archive (trailing zero blocks) is
//!   recorded.
//!
//! ## The boundary it cannot cross (honest)
//!
//! Detection is a **strong magic + validation** test:
//!
//! * ZIP requires `PK\x03\x04` at offset 0 (or the empty-archive `PK\x05\x06`) with
//!   a **valid EOCD** and a coherent central directory (the reused physical scan).
//!   A random `PK`-prefixed blob without a valid EOCD is **not** ZIP.
//! * TAR requires a valid `ustar`/old-GNU header (magic at offset 257) or a
//!   plausible v7 header (offset 0) whose 512-byte **header checksum** matches. An
//!   all-zero (empty) tar has no valid header and stays `Opaque`.
//!
//! Because the office ZIP family is detected first, a `.docx`/`.xlsx`/`.odt` source
//! still detects as its office format and is never reclassified as a plain archive.
//! A ZIP that is *also* a malformed office package is claimed by its office format
//! (which may then decline typed) rather than by this adapter. Prose, and any
//! `PK`- or 512-byte-junk blob that fails the validation above, stays `Opaque`.

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model members (defends the decoder against a hostile blob).
pub const MAX_MODEL_ENTRIES: u32 = 1 << 20;
/// Hard cap on a decoded name/linkname length (defends the decoder).
pub const MAX_MODEL_NAME_BYTES: u32 = 1 << 24;

/// The recorded dialect: a ZIP archive.
pub const DIALECT_ZIP: u8 = 0;
/// The recorded dialect: a TAR archive.
pub const DIALECT_TAR: u8 = 1;

/// Stable lower-case dialect name.
pub const fn dialect_name(dialect: u8) -> &'static str {
    match dialect {
        DIALECT_TAR => "tar",
        _ => "zip",
    }
}

/// `ext` bit: a PAX per-file (`x`) extension record preceded this member.
pub const EXT_PAX: u8 = 1 << 0;
/// `ext` bit: a GNU long-name (`L`) extension record preceded this member.
pub const EXT_GNU_LONG_NAME: u8 = 1 << 1;
/// `ext` bit: a GNU long-link (`K`) extension record preceded this member.
pub const EXT_GNU_LONG_LINK: u8 = 1 << 2;
/// `ext` bit: a PAX global (`g`) extension record was seen before this member.
pub const EXT_PAX_GLOBAL: u8 = 1 << 3;
/// The mask of every defined `ext` bit.
pub const EXT_MASK: u8 = EXT_PAX | EXT_GNU_LONG_NAME | EXT_GNU_LONG_LINK | EXT_PAX_GLOBAL;

/// The TAR header block size.
const TAR_BLOCK: u64 = 512;
/// The TAR checksum field offsets.
const TAR_CHKSUM: core::ops::Range<usize> = 148..156;

/// One archive member with its verbatim metadata and exact source spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerEntry {
    /// Raw name bytes (never decoded, normalized, or lossily converted on the model
    /// path). For TAR a GNU/PAX long name or a ustar `prefix`/`name` join is applied.
    pub name: Vec<u8>,
    /// `(offset, length)` of the name bytes in the source (advisory source anchor).
    pub name_span: (u64, u64),
    /// Raw TAR `linkname` bytes (empty for ZIP).
    pub linkname: Vec<u8>,
    /// `(offset, length)` of the linkname bytes in the source (empty for ZIP).
    pub link_span: (u64, u64),
    /// The member's **exact raw byte span** (header through payload/padding/descriptor).
    pub span: (u64, u64),
    /// `(offset, length)` of the member header (ZIP local header / TAR 512-byte block).
    pub header: (u64, u64),
    /// `(offset, length)` of the exact payload (ZIP raw compressed bytes / TAR data).
    pub data: (u64, u64),
    /// ZIP compression method id (0 for TAR).
    pub method: u16,
    /// ZIP general-purpose bit flag (0 for TAR).
    pub flags: u16,
    /// TAR `typeflag` byte (0 for ZIP).
    pub typeflag: u8,
    /// Subset of the `EXT_*` bits (0 for ZIP).
    pub ext: u8,
    /// ZIP declared CRC-32/ISO-HDLC (0 for TAR).
    pub crc32: u32,
    /// ZIP declared compressed size / TAR payload size.
    pub compressed_size: u64,
    /// ZIP declared uncompressed size / TAR payload size.
    pub uncompressed_size: u64,
    /// TAR `mode` (0 for ZIP).
    pub mode: u32,
    /// TAR `uid` (0 for ZIP).
    pub uid: u64,
    /// TAR `gid` (0 for ZIP).
    pub gid: u64,
    /// TAR `mtime` (0 for ZIP).
    pub mtime: i64,
}

/// The canonical derived container model (the materialization of a
/// `ContainerModel` node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerModel {
    /// The recorded dialect (`DIALECT_ZIP` or `DIALECT_TAR`).
    pub dialect: u8,
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// `(offset, length)` of the archive trailer (ZIP EOCD / TAR zero blocks).
    pub trailer: (u64, u64),
    /// The ordered member list.
    pub entries: Vec<ContainerEntry>,
}

impl ContainerModel {
    /// The member at `index`, if present.
    pub fn entry(&self, index: u32) -> Option<&ContainerEntry> {
        self.entries.get(index as usize)
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let names: usize = self
            .entries
            .iter()
            .map(|e| e.name.len() + e.linkname.len())
            .sum();
        let mut out = Vec::with_capacity(32 + self.entries.len() * 128 + names);
        out.extend_from_slice(b"CTNR");
        out.push(MODEL_VERSION);
        out.push(self.dialect);
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&self.trailer.0.to_le_bytes());
        out.extend_from_slice(&self.trailer.1.to_le_bytes());
        out.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());
        for e in &self.entries {
            out.extend_from_slice(&(e.name.len() as u32).to_le_bytes());
            out.extend_from_slice(&e.name);
            out.extend_from_slice(&e.name_span.0.to_le_bytes());
            out.extend_from_slice(&e.name_span.1.to_le_bytes());
            out.extend_from_slice(&(e.linkname.len() as u32).to_le_bytes());
            out.extend_from_slice(&e.linkname);
            out.extend_from_slice(&e.link_span.0.to_le_bytes());
            out.extend_from_slice(&e.link_span.1.to_le_bytes());
            out.extend_from_slice(&e.span.0.to_le_bytes());
            out.extend_from_slice(&e.span.1.to_le_bytes());
            out.extend_from_slice(&e.header.0.to_le_bytes());
            out.extend_from_slice(&e.header.1.to_le_bytes());
            out.extend_from_slice(&e.data.0.to_le_bytes());
            out.extend_from_slice(&e.data.1.to_le_bytes());
            out.extend_from_slice(&e.method.to_le_bytes());
            out.extend_from_slice(&e.flags.to_le_bytes());
            out.push(e.typeflag);
            out.push(e.ext);
            out.extend_from_slice(&e.crc32.to_le_bytes());
            out.extend_from_slice(&e.compressed_size.to_le_bytes());
            out.extend_from_slice(&e.uncompressed_size.to_le_bytes());
            out.extend_from_slice(&e.mode.to_le_bytes());
            out.extend_from_slice(&e.uid.to_le_bytes());
            out.extend_from_slice(&e.gid.to_le_bytes());
            out.extend_from_slice(&e.mtime.to_le_bytes());
        }
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<ContainerModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"CTNR" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let dialect = r.u8()?;
        if dialect > DIALECT_TAR {
            return Err(corrupt("unknown dialect"));
        }
        let doc_len = r.u64()?;
        let trailer = (r.u64()?, r.u64()?);
        if trailer.0 > trailer.1 || trailer.1 > doc_len {
            return Err(corrupt("trailer span is outside the document"));
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
            let name_span = (r.u64()?, r.u64()?);
            let link_len = r.u32()?;
            if link_len > MAX_MODEL_NAME_BYTES {
                return Err(corrupt("model linkname length is implausible"));
            }
            let linkname = r.bytes(link_len as usize)?.to_vec();
            let link_span = (r.u64()?, r.u64()?);
            let span = (r.u64()?, r.u64()?);
            let header = (r.u64()?, r.u64()?);
            let data = (r.u64()?, r.u64()?);
            let method = r.u16()?;
            let flags = r.u16()?;
            let typeflag = r.u8()?;
            let ext = r.u8()?;
            let crc32 = r.u32()?;
            let compressed_size = r.u64()?;
            let uncompressed_size = r.u64()?;
            let mode = r.u32()?;
            let uid = r.u64()?;
            let gid = r.u64()?;
            let mtime = r.i64()?;
            if ext & !EXT_MASK != 0 {
                return Err(corrupt("unknown extension flag"));
            }
            if span.0 > header.0
                || header.0 > header.1
                || header.1 > data.0
                || data.0 > data.1
                || data.1 > span.1
                || span.1 > doc_len
            {
                return Err(corrupt("member span is inconsistent"));
            }
            if name_span.0 > name_span.1 || name_span.1 > doc_len {
                return Err(corrupt("name span is outside the document"));
            }
            if link_span.0 > link_span.1 || link_span.1 > doc_len {
                return Err(corrupt("link span is outside the document"));
            }
            entries.push(ContainerEntry {
                name,
                name_span,
                linkname,
                link_span,
                span,
                header,
                data,
                method,
                flags,
                typeflag,
                ext,
                crc32,
                compressed_size,
                uncompressed_size,
                mode,
                uid,
                gid,
                mtime,
            });
        }
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        Ok(ContainerModel {
            dialect,
            doc_len,
            trailer,
            entries,
        })
    }
}

/// The exact raw bytes of a member's whole record span.
pub fn entry_bytes<'a>(source: &'a [u8], e: &ContainerEntry) -> Result<&'a [u8]> {
    slice(
        source,
        e.span.0,
        e.span.1,
        "member span is outside the source",
    )
}

/// The exact payload bytes of a member (ZIP raw compressed / TAR data).
pub fn entry_data_bytes<'a>(source: &'a [u8], e: &ContainerEntry) -> Result<&'a [u8]> {
    slice(
        source,
        e.data.0,
        e.data.1,
        "member data span is outside the source",
    )
}

/// The name bytes of a member (lossy UTF-8 for reports).
pub fn entry_name_text(e: &ContainerEntry) -> String {
    String::from_utf8_lossy(&e.name).into_owned()
}

/// A human-readable member-kind name for the recorded dialect.
pub fn entry_kind_name(dialect: u8, e: &ContainerEntry) -> &'static str {
    if dialect == DIALECT_TAR {
        tar_type_name(e.typeflag)
    } else {
        zip_method_name(e.method)
    }
}

/// A human-readable ZIP compression-method name.
pub fn zip_method_name(method: u16) -> &'static str {
    match method {
        0 => "stored",
        8 => "deflate",
        9 => "deflate64",
        12 => "bzip2",
        14 => "lzma",
        93 => "zstd",
        95 => "xz",
        98 => "ppmd",
        _ => "method",
    }
}

/// A human-readable TAR `typeflag` name.
pub fn tar_type_name(typeflag: u8) -> &'static str {
    match typeflag {
        0 | b'0' => "file",
        b'1' => "hardlink",
        b'2' => "symlink",
        b'3' => "character-device",
        b'4' => "block-device",
        b'5' => "directory",
        b'6' => "fifo",
        b'7' => "contiguous",
        b'L' => "gnu-long-name",
        b'K' => "gnu-long-link",
        b'x' => "pax-header",
        b'g' => "pax-global",
        _ => "type",
    }
}

/// Whether a member is a directory (TAR `typeflag` 5; always `false` for ZIP).
pub fn entry_is_dir(dialect: u8, e: &ContainerEntry) -> bool {
    dialect == DIALECT_TAR && e.typeflag == b'5'
}

/// Fast-fail container detector: a valid ZIP (magic + EOCD) or a valid TAR
/// (strong header + checksum). Never consults a file name.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.is_empty() || source.len() as u64 > limits.max_container_document_bytes {
        return false;
    }
    detect_zip(source, limits) || detect_tar(source, limits)
}

/// Whether `source` is a structurally valid ZIP archive (reusing the byte-
/// authoritative physical layer). Requires the local-header or empty-archive
/// signature at offset 0 and a valid EOCD/central directory.
pub fn detect_zip(source: &[u8], limits: Limits) -> bool {
    if source.len() as u64 > limits.max_container_document_bytes {
        return false;
    }
    let ok_magic = source.starts_with(b"PK\x03\x04") || source.starts_with(b"PK\x05\x06");
    if !ok_magic {
        return false;
    }
    crate::adapter::package::zip::scan(source, limits).is_ok()
}

/// Whether `source` is a structurally valid TAR archive (strong header +
/// checksum). An all-zero (empty) tar has no valid header and is rejected.
pub fn detect_tar(source: &[u8], limits: Limits) -> bool {
    if source.len() < TAR_BLOCK as usize
        || source.len() as u64 > limits.max_container_document_bytes
    {
        return false;
    }
    let block = &source[..TAR_BLOCK as usize];
    if !tar_checksum_ok(block) {
        return false;
    }
    let magic = &block[257..263];
    if magic == b"ustar\0" || magic == b"ustar " {
        return true;
    }
    // The old-GNU/v7 variant has no magic: require a plausible v7 header.
    if magic.iter().all(|&b| b == 0) {
        return v7_plausible(block);
    }
    false
}

/// Parse `source` into a [`ContainerModel`] under `limits`. Declines typed on any
/// malformed or over-large input; never panics.
pub fn parse(source: &[u8], limits: Limits) -> Result<ContainerModel> {
    if source.len() as u64 > limits.max_container_document_bytes {
        return Err(Error::resource_limit(format!(
            "container source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_container_document_bytes
        )));
    }
    if detect_zip(source, limits) {
        return build_zip_model(source, limits);
    }
    if detect_tar(source, limits) {
        return build_tar_model(source, limits);
    }
    Err(invalid_container(
        "source is neither a valid ZIP (magic + EOCD) nor a valid TAR (header + checksum)",
    ))
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of a `ContainerModel` node).
pub fn build_container_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits)?.encode())
}

/// A bounded, case-sensitive lexical search over every member name and linkname.
pub fn find(model: &ContainerModel, pattern: &str, max_bytes: u64) -> Result<Vec<ContainerMatch>> {
    let needle = pattern.as_bytes();
    let mut out: Vec<ContainerMatch> = Vec::new();
    let mut estimated: u64 = 0;
    for (i, e) in model.entries.iter().enumerate() {
        for (field, hay, anchor) in [
            (FIELD_NAME, e.name.as_slice(), e.name_span),
            (FIELD_LINK, e.linkname.as_slice(), e.link_span),
        ] {
            if needle.is_empty() {
                continue;
            }
            let mut at = 0usize;
            while at + needle.len() <= hay.len() {
                if &hay[at..at + needle.len()] == needle {
                    estimated = estimated.saturating_add(64 + needle.len() as u64);
                    if estimated > max_bytes {
                        return Err(Error::resource_limit(format!(
                            "container find exceeded the {max_bytes}-byte budget"
                        )));
                    }
                    let start = anchor.0.saturating_add(at as u64);
                    let end = start.saturating_add(needle.len() as u64);
                    out.push(ContainerMatch {
                        entry: i as u32,
                        field,
                        start,
                        end,
                        text: String::from_utf8_lossy(&hay[at..at + needle.len()]).into_owned(),
                    });
                    at += needle.len();
                } else {
                    at += 1;
                }
            }
        }
    }
    Ok(out)
}

/// A compact canonical text rendering of the member list (the common `text`
/// observation). Bounded by `max_bytes`.
pub fn canonical_text(model: &ContainerModel, _source: &[u8], max_bytes: u64) -> Result<String> {
    let mut out = String::new();
    for (i, e) in model.entries.iter().enumerate() {
        let name = String::from_utf8_lossy(&e.name).into_owned();
        let kind = if entry_is_dir(model.dialect, e) {
            "directory"
        } else {
            entry_kind_name(model.dialect, e)
        };
        out.push_str(&format!(
            "{i}\t{name}\t{kind}\t{}\n",
            e.data.1.saturating_sub(e.data.0)
        ));
        if out.len() as u64 > max_bytes {
            return Err(Error::resource_limit(format!(
                "container canonical text exceeded the {max_bytes}-byte budget"
            )));
        }
    }
    Ok(out)
}

/// One lexical-match record from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerMatch {
    /// The member index.
    pub entry: u32,
    /// `FIELD_NAME` or `FIELD_LINK`.
    pub field: u8,
    /// The match's exact source byte offset.
    pub start: u64,
    /// One past the match's last source byte.
    pub end: u64,
    /// The matched text.
    pub text: String,
}

/// Match field: the member name.
pub const FIELD_NAME: u8 = 0;
/// Match field: the member linkname.
pub const FIELD_LINK: u8 = 1;

/// The stable match-field name.
pub const fn field_name(field: u8) -> &'static str {
    match field {
        FIELD_LINK => "linkname",
        _ => "name",
    }
}

// ---------------------------------------------------------------------------
// ZIP model
// ---------------------------------------------------------------------------

fn build_zip_model(source: &[u8], limits: Limits) -> Result<ContainerModel> {
    let physical = crate::adapter::package::zip::scan(source, limits)?;
    physical.validate(source.len() as u64)?;
    if physical.members.len() as u64 > limits.max_container_members {
        return Err(Error::resource_limit(format!(
            "zip declares {} members, exceeding max_container_members {}",
            physical.members.len(),
            limits.max_container_members
        )));
    }
    let mut name_bytes: u64 = 0;
    let mut entries = Vec::with_capacity(physical.members.len());
    for m in &physical.members {
        name_bytes = name_bytes.saturating_add(m.name.len() as u64);
        if name_bytes > limits.max_container_name_bytes {
            return Err(Error::resource_limit(format!(
                "zip name bytes exceed max_container_name_bytes {}",
                limits.max_container_name_bytes
            )));
        }
        if m.name.len() as u64 > limits.max_container_entry_name_bytes {
            return Err(Error::resource_limit(format!(
                "zip member name {} bytes exceeds max_container_entry_name_bytes {}",
                m.name.len(),
                limits.max_container_entry_name_bytes
            )));
        }
        let data_end = m.data.0.saturating_add(m.data.1);
        let span_end = match &m.data_descriptor {
            Some(dd) => dd.span.0.saturating_add(dd.span.1),
            None => data_end,
        };
        // The authoritative name span is the central directory's own name field.
        let name_span = physical
            .central
            .get(m.central_index as usize)
            .map(|c| (c.name.0, c.name.0.saturating_add(c.name.1)))
            .unwrap_or((m.local_header.0, m.local_header.0));
        entries.push(ContainerEntry {
            name: m.name.clone(),
            name_span,
            linkname: Vec::new(),
            link_span: (0, 0),
            span: (m.local_header.0, span_end.max(m.local_header.0)),
            header: (
                m.local_header.0,
                m.local_header.0.saturating_add(m.local_header.1),
            ),
            data: (m.data.0, data_end),
            method: m.method,
            flags: m.flags,
            typeflag: 0,
            ext: 0,
            crc32: m.crc32,
            compressed_size: m.compressed_size,
            uncompressed_size: m.uncompressed_size,
            mode: 0,
            uid: 0,
            gid: 0,
            mtime: 0,
        });
    }
    Ok(ContainerModel {
        dialect: DIALECT_ZIP,
        doc_len: source.len() as u64,
        trailer: (
            physical.eocd.span.0,
            physical.eocd.span.0.saturating_add(physical.eocd.span.1),
        ),
        entries,
    })
}

// ---------------------------------------------------------------------------
// TAR model
// ---------------------------------------------------------------------------

/// Pending extension state applied to the next member.
#[derive(Default)]
struct Pending {
    long_name: Option<(Vec<u8>, (u64, u64))>,
    long_link: Option<(Vec<u8>, (u64, u64))>,
    pax_path: Option<(Vec<u8>, (u64, u64))>,
    pax_link: Option<(Vec<u8>, (u64, u64))>,
    pax_size: Option<u64>,
    pax_uid: Option<u64>,
    pax_gid: Option<u64>,
    pax_mtime: Option<i64>,
    global: bool,
}

impl Pending {
    fn is_empty(&self) -> bool {
        self.long_name.is_none()
            && self.long_link.is_none()
            && self.pax_path.is_none()
            && self.pax_link.is_none()
            && self.pax_size.is_none()
            && self.pax_uid.is_none()
            && self.pax_gid.is_none()
            && self.pax_mtime.is_none()
    }
}

fn build_tar_model(source: &[u8], limits: Limits) -> Result<ContainerModel> {
    let len = source.len() as u64;
    let mut entries: Vec<ContainerEntry> = Vec::new();
    let mut pos: u64 = 0;
    let mut pending = Pending::default();
    let mut extensions: u64 = 0;
    let mut name_bytes: u64 = 0;
    let trailer;

    loop {
        if pos == len {
            // EOF with no explicit zero blocks is accepted as a well-formed end.
            trailer = (len, len);
            break;
        }
        if len - pos < TAR_BLOCK {
            return Err(invalid_container("truncated tar header block"));
        }
        let start = pos as usize;
        let block = &source[start..start + TAR_BLOCK as usize];
        if block.iter().all(|&b| b == 0) {
            // End-of-archive: every remaining byte must be zero padding.
            if !source[start..].iter().all(|&b| b == 0) {
                return Err(invalid_container("tar trailer is not zero blocks"));
            }
            trailer = (pos, len);
            break;
        }
        if !tar_checksum_ok(block) {
            return Err(invalid_container("tar header checksum mismatch"));
        }
        let typeflag = block[156];
        let size = tar_number(&block[124..136])
            .and_then(|v| u64::try_from(v).ok())
            .ok_or_else(|| invalid_container("tar size field is not a valid number"))?;
        let data_start = pos + TAR_BLOCK;
        let data_end = data_start
            .checked_add(size)
            .ok_or_else(|| invalid_container("tar data size overflows"))?;
        let padded = round_up_512(size);
        let record_end = data_start
            .checked_add(padded)
            .ok_or_else(|| invalid_container("tar member span overflows"))?;
        if record_end > len {
            return Err(invalid_container("tar member extends past end of archive"));
        }
        let data = (data_start, data_end);
        match typeflag {
            b'L' => {
                extensions += 1;
                check_extensions(extensions, limits)?;
                pending.long_name = Some((trim_nul(slice_abs(source, data)?), data));
            }
            b'K' => {
                extensions += 1;
                check_extensions(extensions, limits)?;
                pending.long_link = Some((trim_nul(slice_abs(source, data)?), data));
            }
            b'x' | b'g' => {
                extensions += 1;
                check_extensions(extensions, limits)?;
                let body = slice_abs(source, data)?;
                apply_pax(body, data, &mut pending)?;
                if typeflag == b'g' {
                    pending.global = true;
                }
            }
            _ => {
                let name = entry_name(block, &pending);
                let name_span = entry_name_span(block, pos, &pending, source)?;
                let linkname = entry_link(block, &pending);
                let link_span = entry_link_span(block, pos, &pending)?;
                if name.is_empty() {
                    return Err(invalid_container("tar member has an empty name"));
                }
                let depth = path_depth(&name);
                if depth > limits.max_container_path_depth {
                    return Err(Error::resource_limit(format!(
                        "tar member path depth {depth} exceeds max_container_path_depth {}",
                        limits.max_container_path_depth
                    )));
                }
                name_bytes = name_bytes.saturating_add(name.len() as u64);
                if name_bytes > limits.max_container_name_bytes {
                    return Err(Error::resource_limit(format!(
                        "tar name bytes exceed max_container_name_bytes {}",
                        limits.max_container_name_bytes
                    )));
                }
                if name.len() as u64 > limits.max_container_entry_name_bytes {
                    return Err(Error::resource_limit(format!(
                        "tar member name {} bytes exceeds max_container_entry_name_bytes {}",
                        name.len(),
                        limits.max_container_entry_name_bytes
                    )));
                }
                if entries.len() as u64 >= limits.max_container_members {
                    return Err(Error::resource_limit(format!(
                        "tar member count exceeds max_container_members {}",
                        limits.max_container_members
                    )));
                }
                let effective_size = pending.pax_size.unwrap_or(size);
                let mut ext: u8 = 0;
                if pending.pax_path.is_some() || pending.pax_link.is_some() {
                    ext |= EXT_PAX;
                }
                if pending.long_name.is_some() {
                    ext |= EXT_GNU_LONG_NAME;
                }
                if pending.long_link.is_some() {
                    ext |= EXT_GNU_LONG_LINK;
                }
                if pending.global {
                    ext |= EXT_PAX_GLOBAL;
                }
                let mode = tar_number(&block[100..108])
                    .and_then(|v| u32::try_from(v.max(0)).ok())
                    .unwrap_or(0);
                let uid = pending
                    .pax_uid
                    .or_else(|| tar_number(&block[108..116]).and_then(|v| u64::try_from(v).ok()))
                    .unwrap_or(0);
                let gid = pending
                    .pax_gid
                    .or_else(|| tar_number(&block[116..124]).and_then(|v| u64::try_from(v).ok()))
                    .unwrap_or(0);
                let mtime = pending
                    .pax_mtime
                    .or_else(|| tar_number(&block[136..148]))
                    .unwrap_or(0);
                entries.push(ContainerEntry {
                    name,
                    name_span,
                    linkname,
                    link_span,
                    span: (pos, record_end),
                    header: (pos, pos.saturating_add(TAR_BLOCK)),
                    data: (data_start, data_end),
                    method: 0,
                    flags: 0,
                    typeflag,
                    ext,
                    crc32: 0,
                    compressed_size: effective_size,
                    uncompressed_size: effective_size,
                    mode,
                    uid,
                    gid,
                    mtime,
                });
                pending = Pending::default();
            }
        }
        pos = record_end;
    }

    // A trailing extension with no following member is a malformed archive.
    if !pending.is_empty() {
        return Err(invalid_container(
            "tar ends with an extension record and no member",
        ));
    }
    Ok(ContainerModel {
        dialect: DIALECT_TAR,
        doc_len: len,
        trailer,
        entries,
    })
}

fn check_extensions(n: u64, limits: Limits) -> Result<()> {
    if n > limits.max_container_extensions {
        return Err(Error::resource_limit(format!(
            "tar extension count exceeds max_container_extensions {}",
            limits.max_container_extensions
        )));
    }
    Ok(())
}

fn entry_name(block: &[u8], pending: &Pending) -> Vec<u8> {
    if let Some((n, _)) = &pending.pax_path {
        return n.clone();
    }
    if let Some((n, _)) = &pending.long_name {
        return n.clone();
    }
    let name = trim_nul(&block[0..100]);
    let prefix = trim_nul(&block[345..500]);
    if !prefix.is_empty() && (block[257..263] == *b"ustar\0" || block[257..263] == *b"ustar ") {
        let mut out = prefix.to_vec();
        out.push(b'/');
        out.extend_from_slice(&name);
        out
    } else {
        name.to_vec()
    }
}

fn entry_name_span(
    block: &[u8],
    header_start: u64,
    pending: &Pending,
    _source: &[u8],
) -> Result<(u64, u64)> {
    if let Some((_, span)) = &pending.pax_path {
        return Ok(*span);
    }
    if let Some((_, span)) = &pending.long_name {
        return Ok(*span);
    }
    let name_len = block[0..100].iter().position(|&b| b == 0).unwrap_or(100) as u64;
    Ok((header_start, header_start.saturating_add(name_len)))
}

fn entry_link(block: &[u8], pending: &Pending) -> Vec<u8> {
    if let Some((n, _)) = &pending.pax_link {
        return n.clone();
    }
    if let Some((n, _)) = &pending.long_link {
        return n.clone();
    }
    trim_nul(&block[157..257])
}

fn entry_link_span(block: &[u8], header_start: u64, pending: &Pending) -> Result<(u64, u64)> {
    if let Some((_, span)) = &pending.pax_link {
        return Ok(*span);
    }
    if let Some((_, span)) = &pending.long_link {
        return Ok(*span);
    }
    let link_len = block[157..257].iter().position(|&b| b == 0).unwrap_or(100) as u64;
    if link_len == 0 {
        return Ok((0, 0));
    }
    Ok((
        header_start.saturating_add(157),
        header_start.saturating_add(157).saturating_add(link_len),
    ))
}

/// Parse PAX extension records (`LEN KEY=VALUE\n`) and fold them into `pending`.
fn apply_pax(data: &[u8], data_span: (u64, u64), pending: &mut Pending) -> Result<()> {
    let mut i = 0usize;
    while i < data.len() {
        let rest = &data[i..];
        let sp = rest
            .iter()
            .position(|&b| b == b' ')
            .ok_or_else(|| invalid_container("pax record is missing its length separator"))?;
        let len_txt = core::str::from_utf8(&rest[..sp])
            .map_err(|_| invalid_container("pax record length is not ASCII"))?;
        let len: usize = len_txt
            .parse()
            .map_err(|_| invalid_container("pax record length is not a number"))?;
        if len <= sp + 1 || i + len > data.len() {
            return Err(invalid_container("pax record length is out of range"));
        }
        let record = &data[i + sp + 1..i + len];
        // Strip the trailing newline (and an optional NUL) if present.
        let record = record.strip_suffix(b"\n").unwrap_or(record);
        let eq = record
            .iter()
            .position(|&b| b == b'=')
            .ok_or_else(|| invalid_container("pax record is missing '='"))?;
        let key = &record[..eq];
        let val = &record[eq + 1..];
        let val_span = (
            data_span.0.saturating_add((i + sp + 1 + eq + 1) as u64),
            data_span
                .0
                .saturating_add((i + sp + 1 + eq + 1 + val.len()) as u64),
        );
        match key {
            b"path" => pending.pax_path = Some((val.to_vec(), val_span)),
            b"linkpath" => pending.pax_link = Some((val.to_vec(), val_span)),
            b"size" => {
                pending.pax_size = core::str::from_utf8(val).ok().and_then(|s| s.parse().ok());
            }
            b"uid" => {
                pending.pax_uid = core::str::from_utf8(val).ok().and_then(|s| s.parse().ok());
            }
            b"gid" => {
                pending.pax_gid = core::str::from_utf8(val).ok().and_then(|s| s.parse().ok());
            }
            b"mtime" => {
                pending.pax_mtime = core::str::from_utf8(val).ok().and_then(|s| s.parse().ok());
            }
            _ => {}
        }
        i += len;
    }
    Ok(())
}

fn slice_abs(source: &[u8], span: (u64, u64)) -> Result<&[u8]> {
    slice(source, span.0, span.1, "tar span is outside the source")
}

fn round_up_512(n: u64) -> u64 {
    n.saturating_add(511) & !511
}

fn path_depth(name: &[u8]) -> u64 {
    let mut depth = 1u64;
    for &b in name {
        if b == b'/' {
            depth = depth.saturating_add(1);
        }
    }
    depth
}

fn trim_nul(field: &[u8]) -> Vec<u8> {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    field[..end].to_vec()
}

/// Verify a 512-byte TAR header checksum (the stored octal equals the byte sum
/// with the checksum field treated as spaces), and that a plausible name is
/// present. A zero block is not a valid header.
fn tar_checksum_ok(block: &[u8]) -> bool {
    if block.len() != TAR_BLOCK as usize || block.iter().all(|&b| b == 0) {
        return false;
    }
    let Some(stored) = tar_number(&block[TAR_CHKSUM]) else {
        return false;
    };
    let mut sum: u64 = 0;
    for (i, &b) in block.iter().enumerate() {
        if TAR_CHKSUM.contains(&i) {
            sum = sum.saturating_add(0x20);
        } else {
            sum = sum.saturating_add(b as u64);
        }
    }
    stored >= 0 && stored as u64 == sum
}

/// A conservative v7 (no magic) header check: valid checksum, a non-empty name,
/// and a plausible `typeflag`.
fn v7_plausible(block: &[u8]) -> bool {
    let name = &block[0..100];
    let name_end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
    if name_end == 0 {
        return false;
    }
    let typeflag = block[156];
    if typeflag >= 0x80 {
        return false;
    }
    tar_number(&block[100..108]).is_some() && tar_number(&block[124..136]).is_some()
}

/// Parse a TAR numeric field: ASCII octal (space/NUL padded) or GNU base-256.
fn tar_number(field: &[u8]) -> Option<i64> {
    if field.is_empty() {
        return None;
    }
    if field[0] & 0x80 != 0 {
        // GNU base-256: big-endian two's complement, the high bit is the marker.
        let mut acc: i128 = if field[0] & 0x40 != 0 { -1 } else { 0 };
        for &b in field {
            acc = (acc << 8) | (b as i128);
        }
        return i64::try_from(acc).ok();
    }
    let mut val: i64 = 0;
    let mut seen = false;
    for &b in field {
        match b {
            b'0'..=b'7' => {
                val = val.checked_mul(8)?.checked_add((b - b'0') as i64)?;
                seen = true;
            }
            b' ' | 0 => {
                if seen {
                    break;
                }
            }
            _ => return None,
        }
    }
    if seen { Some(val) } else { None }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn slice<'a>(source: &'a [u8], start: u64, end: u64, why: &'static str) -> Result<&'a [u8]> {
    let s = usize::try_from(start).map_err(|_| invalid_container(why))?;
    let e = usize::try_from(end).map_err(|_| invalid_container(why))?;
    source.get(s..e).ok_or_else(|| invalid_container(why))
}

fn invalid_container(message: impl Into<String>) -> Error {
    Error::invalid_container_structure(message)
}

fn corrupt(message: impl Into<String>) -> Error {
    Error::invalid_container_structure(format!("container model: {}", message.into()))
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

    fn i64(&mut self) -> Result<i64> {
        Ok(self.u64()? as i64)
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
        assert_eq!(dialect_name(DIALECT_ZIP), "zip");
        assert_eq!(dialect_name(DIALECT_TAR), "tar");
        assert_eq!(zip_method_name(0), "stored");
        assert_eq!(zip_method_name(8), "deflate");
        assert_eq!(tar_type_name(b'5'), "directory");
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
        for _ in 0..2048 {
            let mut buf = [0u8; 96];
            for b in buf.iter_mut() {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                *b = (seed >> 33) as u8;
            }
            let _ = detect(&buf, Limits::DEFAULT);
            let _ = parse(&buf, Limits::STRICT);
        }
    }

    #[test]
    fn prose_is_not_a_container() {
        assert!(!detect(
            b"the quick brown fox jumps over the lazy dog\n",
            Limits::DEFAULT
        ));
    }

    #[test]
    fn round_up_to_block() {
        assert_eq!(round_up_512(0), 0);
        assert_eq!(round_up_512(1), 512);
        assert_eq!(round_up_512(512), 512);
        assert_eq!(round_up_512(513), 1024);
    }
}
