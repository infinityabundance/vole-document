//! Byte-authoritative ZIP physical classifier (Phase 12.1).
//!
//! This is the ZIP analogue of [`crate::adapter::pdf::physical`]: it partitions
//! the whole input `[0, len)` into structural [`ZipSpan`]s that form a complete,
//! ordered, non-overlapping cover, and it records the physical identity and
//! verbatim metadata of every member. The cover is the authority;
//! `materialize(descriptor) == original_bytes` follows because the cover is an
//! exact partition of the source (see [`ZipPhysical::reemits`]).
//!
//! ## Non-negotiables
//!
//! * **Physical identity is `(central-directory ordinal, local-header offset)`**
//!   ([`PhysicalMemberId`]), never the member name. Duplicate names are never
//!   collapsed or normalized; the decoded name is advisory.
//! * **Nothing is normalized.** Signatures, flags, method ids, MS-DOS time/date,
//!   CRC, both declared size fields, name bytes (case + UTF-8 flag), every extra
//!   field in order, comments, prefix/trailing bytes, descriptor presence, the
//!   ZIP64 vs 32-bit layout, and central-directory order are all preserved
//!   verbatim in the cover. The scanner never re-serializes structure.
//! * **Reject vs preserve (plan §DEC-9).** A broken *cover/identity* — overlap,
//!   impossible offsets, ZIP64 contradictions, a malformed descriptor, a
//!   multi-disk layout, an ambiguous EOCD — is a typed rejection. A
//!   *semantics/resource* concern — a bomb ratio, an unknown method, encryption,
//!   a CRC fault — leaves the exact bytes intact and only declines later decode.
//!   A hostile *name* never breaks the physical cover; it is rejected at the
//!   semantic boundary by [`require_safe_name`] (and never becomes a `PathBuf`).
//! * **No decompression happens here** (12.1): the only CRC work is an optional,
//!   bounded verification of `stored` (method 0) members via
//!   [`verify_stored_crc`]. DEFLATE decode is a later subphase.

use crate::error::{Error, Result};
use crate::limits::Limits;

// PKWARE APPNOTE 6.3.x record signatures (little-endian u32 values).
const SIG_LOCAL_HEADER: u32 = 0x0403_4b50;
const SIG_DATA_DESCRIPTOR: u32 = 0x0807_4b50;
const SIG_ARCHIVE_EXTRA_DATA: u32 = 0x0806_4b50;
const SIG_CENTRAL_HEADER: u32 = 0x0201_4b50;
const SIG_EOCD64: u32 = 0x0606_4b50;
const SIG_EOCD64_LOCATOR: u32 = 0x0706_4b50;
const SIG_EOCD: u32 = 0x0605_4b50;

/// General-purpose flag bit 3: sizes/CRC follow the member data in a descriptor.
const FLAG_DATA_DESCRIPTOR: u16 = 0x0008;

/// The ZIP64 extended-information extra-field header id.
const EXTRA_ZIP64: u16 = 0x0001;

/// Whether a data descriptor carried the optional `0x08074b50` signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescriptorStyle {
    /// The descriptor begins with the `0x08074b50` signature.
    Signature,
    /// The descriptor omits the signature (the leading field is the CRC).
    NoSignature,
}

/// The physical class of a byte span in a ZIP archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZipSpanKind {
    /// Bytes before the first structural record (e.g. a self-extracting stub).
    Prefix,
    /// A local file header (`0x04034b50`) plus its name and extra field.
    LocalHeader,
    /// A member's raw compressed/stored payload (the exact leaf for a member).
    MemberData,
    /// A data descriptor following a bit-3 member.
    DataDescriptor(DescriptorStyle),
    /// The central directory region (`0x02014b50` entries and any interleaving).
    CentralDirectory,
    /// A ZIP64 end-of-central-directory record (`0x06064b50`).
    Zip64Eocd,
    /// A ZIP64 end-of-central-directory locator (`0x07064b50`).
    Zip64EocdLocator,
    /// The end-of-central-directory record (`0x06054b50`) plus archive comment.
    Eocd,
    /// An archive extra data record (`0x08064b50`).
    ArchiveExtraData,
    /// Bytes after the EOCD record.
    Trailing,
    /// Bytes not confidently classified (residual authority; never dropped).
    Unclassified,
}

/// One physical span: a half-open byte range `[start, start + len)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZipSpan {
    /// Offset of the first byte of the span.
    pub start: u64,
    /// Number of bytes in the span (always > 0 in a valid cover).
    pub len: u64,
    /// The physical class of the span.
    pub kind: ZipSpanKind,
}

impl ZipSpan {
    /// Byte offset just past the span.
    pub fn end(&self) -> u64 {
        self.start.saturating_add(self.len)
    }
}

/// Physical member identity: `(central-directory ordinal, local-header offset)`.
///
/// This is the authority; the decoded name is advisory lookup data only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalMemberId {
    /// Zero-based position of the member's central-directory entry.
    pub ordinal: u32,
    /// File offset of the member's local file header.
    pub local_header_offset: u64,
}

/// The data descriptor following a bit-3 member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataDescriptor {
    /// `(offset, length)` of the descriptor bytes.
    pub span: (u64, u64),
    /// Whether the optional signature was present.
    pub style: DescriptorStyle,
}

/// One member discovered from the central directory, with verbatim metadata.
///
/// The declared sizes are **as declared**; no decompression or reinterpretation
/// is performed in this subphase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipMember {
    /// Physical identity.
    pub id: PhysicalMemberId,
    /// Index of the originating central-directory entry (equals `id.ordinal`).
    pub central_index: u32,
    /// Raw name bytes (case and UTF-8 flag preserved; never normalized).
    pub name: Vec<u8>,
    /// File offset of the local file header.
    pub local_header_offset: u64,
    /// `(offset, length)` of the local file header (signature through extra).
    pub local_header: (u64, u64),
    /// `(offset, length)` of the raw compressed/stored payload (the exact leaf).
    pub data: (u64, u64),
    /// Compression method id.
    pub method: u16,
    /// General-purpose bit flag.
    pub flags: u16,
    /// Declared CRC-32/ISO-HDLC.
    pub crc32: u32,
    /// Declared compressed size.
    pub compressed_size: u64,
    /// Declared uncompressed size.
    pub uncompressed_size: u64,
    /// `(offset, length)` spans of the local and central extra fields, in order.
    pub extra_spans: Vec<(u64, u64)>,
    /// The data descriptor, when flag bit 3 is set.
    pub data_descriptor: Option<DataDescriptor>,
    /// `(offset, length)` of the central per-entry comment.
    pub comment: (u64, u64),
}

/// One parsed central-directory entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CentralEntry {
    /// Zero-based position in the central directory.
    pub ordinal: u32,
    /// `(offset, length)` of the whole central-file-header record.
    pub span: (u64, u64),
    /// `(offset, length)` of the name bytes.
    pub name: (u64, u64),
    /// `(offset, length)` of the extra field.
    pub extra: (u64, u64),
    /// `(offset, length)` of the per-entry comment.
    pub comment: (u64, u64),
    /// Declared local-header offset (ZIP64-resolved).
    pub local_header_offset: u64,
    /// Compression method id.
    pub method: u16,
    /// General-purpose bit flag.
    pub flags: u16,
    /// Declared CRC-32/ISO-HDLC.
    pub crc32: u32,
    /// Declared compressed size (ZIP64-resolved).
    pub compressed_size: u64,
    /// Declared uncompressed size (ZIP64-resolved).
    pub uncompressed_size: u64,
    /// MS-DOS modification time (preserved verbatim in the cover).
    pub dos_time: u16,
    /// MS-DOS modification date (preserved verbatim in the cover).
    pub dos_date: u16,
}

/// The classic end-of-central-directory record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EocdRecord {
    /// `(offset, length)` of the record plus archive comment.
    pub span: (u64, u64),
    /// This disk number.
    pub disk_number: u16,
    /// Disk on which the central directory starts.
    pub cd_start_disk: u16,
    /// Entries on this disk.
    pub entries_this_disk: u16,
    /// Total entries (may be the `0xFFFF` ZIP64 sentinel).
    pub total_entries: u16,
    /// Central-directory size (may be the `0xFFFFFFFF` ZIP64 sentinel).
    pub cd_size: u32,
    /// Central-directory offset (may be the `0xFFFFFFFF` ZIP64 sentinel).
    pub cd_offset: u32,
    /// `(offset, length)` of the archive comment.
    pub comment: (u64, u64),
}

/// The ZIP64 end-of-central-directory record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Zip64Eocd {
    /// `(offset, length)` of the record.
    pub span: (u64, u64),
    /// Record size (the extent of the record beyond the signature+size fields).
    pub size_of_record: u64,
    /// Version made by.
    pub version_made_by: u16,
    /// Version needed to extract.
    pub version_needed: u16,
    /// This disk number.
    pub disk_number: u32,
    /// Disk on which the central directory starts.
    pub cd_start_disk: u32,
    /// Entries on this disk.
    pub entries_this_disk: u64,
    /// Total entries.
    pub total_entries: u64,
    /// Central-directory size.
    pub cd_size: u64,
    /// Central-directory offset.
    pub cd_offset: u64,
}

/// The ZIP64 end-of-central-directory locator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Zip64EocdLocator {
    /// `(offset, length)` of the locator.
    pub span: (u64, u64),
    /// Disk holding the ZIP64 EOCD record.
    pub disk_with_eocd: u32,
    /// Offset of the ZIP64 EOCD record.
    pub eocd_offset: u64,
    /// Total number of disks.
    pub total_disks: u32,
}

/// The ZIP64 EOCD record and its locator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Zip64Records {
    /// The ZIP64 end-of-central-directory record.
    pub eocd: Zip64Eocd,
    /// The locator that precedes the classic EOCD.
    pub locator: Zip64EocdLocator,
}

/// The physical summary of a ZIP input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipPhysical {
    /// A complete, ordered, non-overlapping cover of `[0, file_len)`.
    pub spans: Vec<ZipSpan>,
    /// Members in central-directory order.
    pub members: Vec<ZipMember>,
    /// Central-directory entries in order.
    pub central: Vec<CentralEntry>,
    /// The classic EOCD record.
    pub eocd: EocdRecord,
    /// The ZIP64 records, when present.
    pub zip64: Option<Zip64Records>,
    /// `(offset, length)` of the archive comment.
    pub archive_comment: (u64, u64),
    /// `(offset, length)` of the leading prefix, if any.
    pub prefix: Option<(u64, u64)>,
    /// `(offset, length)` of the trailing bytes, if any.
    pub trailing: Option<(u64, u64)>,
    /// Total input length.
    pub file_len: u64,
}

impl ZipPhysical {
    /// Sum of all span lengths. Saturates rather than panicking.
    pub fn total_len(&self) -> u64 {
        self.spans
            .iter()
            .fold(0u64, |acc, s| acc.saturating_add(s.len))
    }

    /// Require a contiguous cover of exactly `[0, declared_len)` and that every
    /// member range lies within the file.
    ///
    /// Returns [`crate::ErrorClass::CoverageViolation`] for a gap, overlap, wrong
    /// total, or length overflow, and
    /// [`crate::ErrorClass::InvalidZipStructure`] for a degenerate zero-length
    /// span or an out-of-range member range. This is the internal self-check that
    /// turns a scanner bug into a loud classified failure instead of silent loss.
    pub fn validate(&self, declared_len: u64) -> Result<()> {
        let mut cursor: u64 = 0;
        for (i, span) in self.spans.iter().enumerate() {
            if span.len == 0 {
                return Err(Error::invalid_zip_structure(format!(
                    "zip cover span {i} has zero length at offset {}",
                    span.start
                )));
            }
            if span.start != cursor {
                let why = if span.start < cursor {
                    "overlap"
                } else {
                    "gap"
                };
                return Err(Error::coverage_violation(format!(
                    "zip cover span {i} {why}: expected start {cursor}, found {}",
                    span.start
                )));
            }
            cursor = cursor.checked_add(span.len).ok_or_else(|| {
                Error::coverage_violation("zip cover span lengths overflow the address space")
            })?;
        }
        if cursor != declared_len {
            return Err(Error::coverage_violation(format!(
                "zip cover ends at {cursor}, declared length is {declared_len}"
            )));
        }
        for (i, m) in self.members.iter().enumerate() {
            let (hs, hl) = m.local_header;
            let hend = hs.checked_add(hl).ok_or_else(|| {
                Error::coverage_violation(format!("member {i} local header overflows"))
            })?;
            let (ds, dl) = m.data;
            let dend = ds.checked_add(dl).ok_or_else(|| {
                Error::coverage_violation(format!("member {i} data span overflows"))
            })?;
            if hend > declared_len || dend > declared_len {
                return Err(Error::coverage_violation(format!(
                    "member {i} range lies outside the file"
                )));
            }
        }
        Ok(())
    }

    /// Prove the cover re-emits `input` byte-for-byte.
    ///
    /// This validates the partition and then concatenates the covered slices in
    /// order, requiring the result to equal `input` exactly. On success the
    /// exactness triple holds trivially: length, digest, and bytes all agree.
    pub fn reemits(&self, input: &[u8]) -> Result<()> {
        self.validate(input.len() as u64)?;
        let mut out = Vec::with_capacity(input.len());
        for span in &self.spans {
            let start = usize::try_from(span.start)
                .map_err(|_| Error::coverage_violation("zip span start exceeds address space"))?;
            let end = usize::try_from(span.end())
                .map_err(|_| Error::coverage_violation("zip span end exceeds address space"))?;
            let part = input.get(start..end).ok_or_else(|| {
                Error::coverage_violation("zip cover span lies outside the input")
            })?;
            out.extend_from_slice(part);
        }
        if out != input {
            return Err(Error::reconstruction_mismatch(
                "zip cover does not re-emit the source bytes",
            ));
        }
        Ok(())
    }

    /// The exact raw compressed/stored bytes of `member`.
    ///
    /// This is the member's exact leaf: no unzip, no rezip, no recompression.
    pub fn member_raw<'a>(&self, member: &ZipMember, input: &'a [u8]) -> Result<&'a [u8]> {
        let start = usize::try_from(member.data.0).map_err(|_| {
            Error::invalid_zip_structure("member data offset exceeds address space")
        })?;
        let len = usize::try_from(member.data.1).map_err(|_| {
            Error::invalid_zip_structure("member data length exceeds address space")
        })?;
        let end = start
            .checked_add(len)
            .ok_or_else(|| Error::invalid_zip_structure("member data span overflows"))?;
        input
            .get(start..end)
            .ok_or_else(|| Error::invalid_zip_structure("member data span lies outside the input"))
    }

    /// Look up a member by physical identity.
    pub fn member_by_id(&self, id: PhysicalMemberId) -> Option<&ZipMember> {
        self.members.iter().find(|m| m.id == id)
    }
}

/// A hostile property of a member name.
///
/// A hazard never affects the physical cover (which stays exact); it is declined
/// at the semantic/lookup boundary. No `PathBuf` is ever built from a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameHazard {
    /// A NUL byte is present.
    Nul,
    /// A control byte (`< 0x20`) is present.
    Control,
    /// The name is absolute (leading `/` or `\`, or a UNC `\\` prefix).
    Absolute,
    /// The name carries a Windows drive prefix (`X:`).
    Drive,
    /// A backslash separator is present.
    Backslash,
    /// A `..` path component is present.
    Traversal,
}

/// Classify a raw member name for hostile properties (advisory; never normalizes).
pub fn classify_name(name: &[u8]) -> Option<NameHazard> {
    if name.contains(&0) {
        return Some(NameHazard::Nul);
    }
    if name.iter().any(|&b| b < 0x20) {
        return Some(NameHazard::Control);
    }
    if name.first() == Some(&b'/') || name.first() == Some(&b'\\') {
        return Some(NameHazard::Absolute);
    }
    if name.len() >= 2 && name[1] == b':' && name[0].is_ascii_alphabetic() {
        return Some(NameHazard::Drive);
    }
    if name.contains(&b'\\') {
        return Some(NameHazard::Backslash);
    }
    if name
        .split(|&b| b == b'/')
        .any(|component| component == b"..")
    {
        return Some(NameHazard::Traversal);
    }
    None
}

/// Reject a hostile member name with a typed error.
///
/// This is the semantic-boundary rejection (never applied while building the
/// physical cover): the archive itself remains exact and wrappable, but any
/// operation that would *use* an unsafe name is declined.
pub fn require_safe_name(name: &[u8]) -> Result<()> {
    match classify_name(name) {
        Some(hazard) => Err(Error::invalid_zip_structure(format!(
            "unsafe member name rejected ({hazard:?})"
        ))),
        None => Ok(()),
    }
}

/// CRC-32/ISO-HDLC (`crc32fast`), the polynomial the ZIP format mandates.
///
/// This is **not** CRC-32C/Castagnoli.
pub fn crc32_iso_hdlc(bytes: &[u8]) -> u32 {
    crc32fast::hash(bytes)
}

/// Verify the declared CRC-32 of a `stored` (method 0) member against its raw
/// bytes. This is the only decompression-free integrity check performed in 12.1;
/// it is bounded by the caller's already-materialized slice.
pub fn verify_stored_crc(member: &ZipMember, raw: &[u8]) -> Result<()> {
    if member.method != 0 {
        return Err(Error::unsupported_feature(format!(
            "CRC verification is only defined for stored (method 0) members, found method {}",
            member.method
        )));
    }
    if raw.len() as u64 != member.uncompressed_size {
        return Err(Error::integrity_mismatch(format!(
            "stored member declared {} uncompressed bytes, found {}",
            member.uncompressed_size,
            raw.len()
        )));
    }
    let computed = crc32_iso_hdlc(raw);
    if computed != member.crc32 {
        return Err(Error::integrity_mismatch(format!(
            "stored member CRC mismatch: declared {:08x}, computed {:08x}",
            member.crc32, computed
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Scanning
// ---------------------------------------------------------------------------

/// Scan `input` into a byte-authoritative [`ZipPhysical`] under `limits`.
///
/// On success the returned cover is a valid partition of `[0, input.len())`, so
/// exact reconstruction is the source itself. Structural/cover ambiguity is a
/// typed rejection; a semantics/resource concern is preserved exactly.
pub fn scan(input: &[u8], limits: Limits) -> Result<ZipPhysical> {
    let file_len = input.len() as u64;
    if file_len > limits.max_input_bytes {
        return Err(Error::resource_limit(format!(
            "zip input {file_len} bytes exceeds max_input_bytes {}",
            limits.max_input_bytes
        )));
    }

    let eocd_off = locate_eocd(input, file_len)
        .ok_or_else(|| Error::invalid_zip_structure("no valid end-of-central-directory record"))?;
    let eocd = parse_eocd(input, eocd_off)?;

    if eocd.disk_number != 0 || eocd.cd_start_disk != 0 {
        return Err(Error::unsupported_feature(
            "multi-disk (spanned) ZIP archives are not supported",
        ));
    }
    if eocd.comment.1 > u64::from(limits.max_zip_archive_comment_bytes) {
        return Err(Error::resource_limit(format!(
            "archive comment {} bytes exceeds max_zip_archive_comment_bytes {}",
            eocd.comment.1, limits.max_zip_archive_comment_bytes
        )));
    }

    let mut core: Vec<ZipSpan> = Vec::new();
    let zip64 = detect_zip64(input, eocd_off, &mut core)?;

    let (total_entries, cd_size, cd_offset) = if let Some(z) = &zip64 {
        (z.eocd.total_entries, z.eocd.cd_size, z.eocd.cd_offset)
    } else {
        let sentinel = eocd.total_entries == 0xFFFF
            || eocd.entries_this_disk == 0xFFFF
            || eocd.cd_size == 0xFFFF_FFFF
            || eocd.cd_offset == 0xFFFF_FFFF;
        if sentinel {
            return Err(Error::invalid_zip_structure(
                "ZIP64 sentinel present without a ZIP64 end-of-central-directory record",
            ));
        }
        (
            u64::from(eocd.total_entries),
            u64::from(eocd.cd_size),
            u64::from(eocd.cd_offset),
        )
    };

    if total_entries > u64::from(limits.max_zip_members) {
        return Err(Error::resource_limit(format!(
            "archive declares {total_entries} members, exceeding max_zip_members {}",
            limits.max_zip_members
        )));
    }
    if cd_size > limits.max_zip_central_dir_bytes {
        return Err(Error::resource_limit(format!(
            "central directory {cd_size} bytes exceeds max_zip_central_dir_bytes {}",
            limits.max_zip_central_dir_bytes
        )));
    }

    let anchor = zip64.as_ref().map_or(eocd.span.0, |z| z.eocd.span.0);
    let cd_end = cd_offset
        .checked_add(cd_size)
        .ok_or_else(|| Error::invalid_zip_structure("central directory offset overflows"))?;
    if cd_end > anchor {
        return Err(Error::invalid_zip_structure(
            "central directory extends past the end-of-central-directory record",
        ));
    }

    let (central, members) =
        parse_central_directory(input, cd_offset, cd_end, total_entries, limits, &mut core)?;

    core.push(ZipSpan {
        start: eocd.span.0,
        len: eocd.span.1,
        kind: ZipSpanKind::Eocd,
    });

    let spans = assemble_cover(core, input, file_len, limits)?;
    let physical = ZipPhysical {
        prefix: spans
            .first()
            .filter(|s| s.kind == ZipSpanKind::Prefix)
            .map(|s| (s.start, s.len)),
        trailing: spans
            .last()
            .filter(|s| s.kind == ZipSpanKind::Trailing)
            .map(|s| (s.start, s.len)),
        spans,
        members,
        central,
        eocd,
        zip64,
        archive_comment: eocd.comment,
        file_len,
    };
    physical.validate(file_len)?;
    Ok(physical)
}

/// Locate the classic EOCD: scan backwards from EOF for a signature whose
/// comment length fits, preferring candidates whose central directory is
/// consistent (or that are accompanied by a ZIP64 locator).
fn locate_eocd(input: &[u8], file_len: u64) -> Option<u64> {
    if file_len < 22 {
        return None;
    }
    let mut o = file_len - 22;
    loop {
        if read_u32(input, o) == Some(SIG_EOCD)
            && let Some(comment_len) = read_u16(input, o + 20)
        {
            let end = o + 22 + u64::from(comment_len);
            if end <= file_len {
                let locator_present =
                    o >= 20 && read_u32(input, o - 20) == Some(SIG_EOCD64_LOCATOR);
                let sentinel = matches!(read_u32(input, o + 12), Some(0xFFFF_FFFF))
                    || matches!(read_u32(input, o + 16), Some(0xFFFF_FFFF))
                    || matches!(read_u16(input, o + 10), Some(0xFFFF));
                let cd_ok = match (read_u32(input, o + 16), read_u32(input, o + 12)) {
                    (Some(cd_off), Some(cd_size)) => u64::from(cd_off)
                        .checked_add(u64::from(cd_size))
                        .is_some_and(|e| e <= o),
                    _ => false,
                };
                if locator_present || sentinel || cd_ok {
                    return Some(o);
                }
            }
        }
        if o == 0 {
            return None;
        }
        o -= 1;
    }
}

/// Parse the classic EOCD record at `off`.
fn parse_eocd(input: &[u8], off: u64) -> Result<EocdRecord> {
    let bad = || Error::invalid_zip_structure("truncated end-of-central-directory record");
    let disk_number = read_u16(input, off + 4).ok_or_else(bad)?;
    let cd_start_disk = read_u16(input, off + 6).ok_or_else(bad)?;
    let entries_this_disk = read_u16(input, off + 8).ok_or_else(bad)?;
    let total_entries = read_u16(input, off + 10).ok_or_else(bad)?;
    let cd_size = read_u32(input, off + 12).ok_or_else(bad)?;
    let cd_offset = read_u32(input, off + 16).ok_or_else(bad)?;
    let comment_len = read_u16(input, off + 20).ok_or_else(bad)?;
    Ok(EocdRecord {
        span: (off, 22 + u64::from(comment_len)),
        disk_number,
        cd_start_disk,
        entries_this_disk,
        total_entries,
        cd_size,
        cd_offset,
        comment: (off + 22, u64::from(comment_len)),
    })
}

/// Detect and parse ZIP64 records immediately before the classic EOCD.
fn detect_zip64(
    input: &[u8],
    eocd_off: u64,
    core: &mut Vec<ZipSpan>,
) -> Result<Option<Zip64Records>> {
    if eocd_off < 20 {
        return Ok(None);
    }
    let loc_off = eocd_off - 20;
    if read_u32(input, loc_off) != Some(SIG_EOCD64_LOCATOR) {
        return Ok(None);
    }
    let bad = || Error::invalid_zip_structure("malformed ZIP64 end-of-central-directory locator");
    let disk_with_eocd = read_u32(input, loc_off + 4).ok_or_else(bad)?;
    let eocd64_off = read_u64(input, loc_off + 8).ok_or_else(bad)?;
    let total_disks = read_u32(input, loc_off + 16).ok_or_else(bad)?;
    if disk_with_eocd != 0 || total_disks > 1 {
        return Err(Error::unsupported_feature(
            "multi-disk (spanned) ZIP64 archives are not supported",
        ));
    }
    if eocd64_off.checked_add(12).is_none_or(|e| e > loc_off) {
        return Err(Error::invalid_zip_structure(
            "ZIP64 EOCD offset is inconsistent with its locator",
        ));
    }
    if read_u32(input, eocd64_off) != Some(SIG_EOCD64) {
        return Err(Error::invalid_zip_structure("ZIP64 EOCD signature missing"));
    }
    let size = read_u64(input, eocd64_off + 4)
        .ok_or_else(|| Error::invalid_zip_structure("truncated ZIP64 EOCD size field"))?;
    if size < 44 {
        return Err(Error::invalid_zip_structure(
            "ZIP64 EOCD record is shorter than the fixed fields",
        ));
    }
    let record_end = eocd64_off + 12 + size;
    if record_end != loc_off {
        return Err(Error::invalid_zip_structure(
            "ZIP64 EOCD record does not abut its locator",
        ));
    }
    let bad = || Error::invalid_zip_structure("truncated ZIP64 EOCD record");
    let version_made_by = read_u16(input, eocd64_off + 12).ok_or_else(bad)?;
    let version_needed = read_u16(input, eocd64_off + 14).ok_or_else(bad)?;
    let disk_number = read_u32(input, eocd64_off + 16).ok_or_else(bad)?;
    let cd_start_disk = read_u32(input, eocd64_off + 20).ok_or_else(bad)?;
    if disk_number != 0 || cd_start_disk != 0 {
        return Err(Error::unsupported_feature(
            "multi-disk (spanned) ZIP64 archives are not supported",
        ));
    }
    let entries_this_disk = read_u64(input, eocd64_off + 24).ok_or_else(bad)?;
    let total_entries = read_u64(input, eocd64_off + 32).ok_or_else(bad)?;
    let cd_size = read_u64(input, eocd64_off + 40).ok_or_else(bad)?;
    let cd_offset = read_u64(input, eocd64_off + 48).ok_or_else(bad)?;
    if cd_offset
        .checked_add(cd_size)
        .is_none_or(|end| end > eocd64_off)
    {
        return Err(Error::invalid_zip_structure(
            "ZIP64 central directory extends past the ZIP64 EOCD record",
        ));
    }
    core.push(ZipSpan {
        start: eocd64_off,
        len: 12 + size,
        kind: ZipSpanKind::Zip64Eocd,
    });
    core.push(ZipSpan {
        start: loc_off,
        len: 20,
        kind: ZipSpanKind::Zip64EocdLocator,
    });
    Ok(Some(Zip64Records {
        eocd: Zip64Eocd {
            span: (eocd64_off, 12 + size),
            size_of_record: size,
            version_made_by,
            version_needed,
            disk_number,
            cd_start_disk,
            entries_this_disk,
            total_entries,
            cd_size,
            cd_offset,
        },
        locator: Zip64EocdLocator {
            span: (loc_off, 20),
            disk_with_eocd,
            eocd_offset: eocd64_off,
            total_disks,
        },
    }))
}

/// Parse the central directory and build members plus their cover spans.
fn parse_central_directory(
    input: &[u8],
    cd_offset: u64,
    cd_end: u64,
    total_entries: u64,
    limits: Limits,
    core: &mut Vec<ZipSpan>,
) -> Result<(Vec<CentralEntry>, Vec<ZipMember>)> {
    let mut central: Vec<CentralEntry> = Vec::new();
    let mut members: Vec<ZipMember> = Vec::new();
    let mut pos = cd_offset;
    let mut ordinal: u32 = 0;
    let mut aggregate: u64 = 0;

    while pos < cd_end {
        if read_u32(input, pos) != Some(SIG_CENTRAL_HEADER) {
            break;
        }
        let (raw, next) = parse_central_entry(input, pos, limits)?;
        if next > cd_end {
            return Err(Error::invalid_zip_structure(
                "central-directory entry overruns the directory",
            ));
        }
        let (member, spans) = build_member(input, ordinal, &raw, cd_offset, limits)?;
        aggregate = aggregate.saturating_add(member.uncompressed_size);
        if aggregate > limits.max_zip_aggregate_uncompressed {
            return Err(Error::resource_limit(format!(
                "aggregate declared uncompressed size {aggregate} exceeds \
                 max_zip_aggregate_uncompressed {}",
                limits.max_zip_aggregate_uncompressed
            )));
        }
        core.extend_from_slice(&spans);
        central.push(CentralEntry {
            ordinal,
            span: raw.span,
            name: raw.name_span,
            extra: raw.extra_span,
            comment: raw.comment_span,
            local_header_offset: member.local_header_offset,
            method: member.method,
            flags: member.flags,
            crc32: member.crc32,
            compressed_size: member.compressed_size,
            uncompressed_size: member.uncompressed_size,
            dos_time: raw.dos_time,
            dos_date: raw.dos_date,
        });
        members.push(member);
        ordinal += 1;
        pos = next;
    }

    if u64::from(ordinal) != total_entries {
        return Err(Error::invalid_zip_structure(format!(
            "central directory declares {total_entries} entries, found {ordinal}"
        )));
    }
    if cd_end > cd_offset {
        core.push(ZipSpan {
            start: cd_offset,
            len: cd_end - cd_offset,
            kind: ZipSpanKind::CentralDirectory,
        });
    }
    Ok((central, members))
}

/// Intermediate parsed central-directory fields before member assembly.
struct CdRaw {
    span: (u64, u64),
    name_span: (u64, u64),
    extra_span: (u64, u64),
    comment_span: (u64, u64),
    flags: u16,
    method: u16,
    dos_time: u16,
    dos_date: u16,
    crc32: u32,
    compressed_size: u64,
    uncompressed_size: u64,
    local_header_offset: u64,
}

/// Parse one central-file-header at `pos`; returns the fields and the offset of
/// the next entry.
fn parse_central_entry(input: &[u8], pos: u64, limits: Limits) -> Result<(CdRaw, u64)> {
    let bad = || Error::invalid_zip_structure("truncated central-directory entry");
    let flags = read_u16(input, pos + 8).ok_or_else(bad)?;
    let method = read_u16(input, pos + 10).ok_or_else(bad)?;
    let dos_time = read_u16(input, pos + 12).ok_or_else(bad)?;
    let dos_date = read_u16(input, pos + 14).ok_or_else(bad)?;
    let crc32 = read_u32(input, pos + 16).ok_or_else(bad)?;
    let compressed32 = read_u32(input, pos + 20).ok_or_else(bad)?;
    let uncompressed32 = read_u32(input, pos + 24).ok_or_else(bad)?;
    let name_len = u64::from(read_u16(input, pos + 28).ok_or_else(bad)?);
    let extra_len = u64::from(read_u16(input, pos + 30).ok_or_else(bad)?);
    let comment_len = u64::from(read_u16(input, pos + 32).ok_or_else(bad)?);
    let disk_start = read_u16(input, pos + 34).ok_or_else(bad)?;
    let local_header32 = read_u32(input, pos + 42).ok_or_else(bad)?;

    if name_len > u64::from(limits.max_zip_name_bytes) {
        return Err(Error::resource_limit(format!(
            "member name {name_len} bytes exceeds max_zip_name_bytes {}",
            limits.max_zip_name_bytes
        )));
    }
    if extra_len > u64::from(limits.max_zip_extra_bytes) {
        return Err(Error::resource_limit(format!(
            "member extra field {extra_len} bytes exceeds max_zip_extra_bytes {}",
            limits.max_zip_extra_bytes
        )));
    }
    if comment_len > u64::from(limits.max_zip_entry_comment_bytes) {
        return Err(Error::resource_limit(format!(
            "member comment {comment_len} bytes exceeds max_zip_entry_comment_bytes {}",
            limits.max_zip_entry_comment_bytes
        )));
    }

    let name_span = (pos + 46, name_len);
    let extra_span = (name_span.0 + name_len, extra_len);
    let comment_span = (extra_span.0 + extra_len, comment_len);
    let entry_len = 46 + name_len + extra_len + comment_len;

    let (uncompressed_size, compressed_size, local_header_offset, _disk) = resolve_zip64_cd(
        input,
        extra_span,
        uncompressed32,
        compressed32,
        local_header32,
        disk_start,
    )?;

    let raw = CdRaw {
        span: (pos, entry_len),
        name_span,
        extra_span,
        comment_span,
        flags,
        method,
        dos_time,
        dos_date,
        crc32,
        compressed_size,
        uncompressed_size,
        local_header_offset,
    };
    Ok((raw, pos + entry_len))
}

/// Resolve the ZIP64 conditional fields of a central-directory entry.
///
/// The ZIP64 extended-information extra field (`0x0001`) carries, in fixed
/// order, the uncompressed size, compressed size, local-header offset, and disk
/// start, each present *only* when the corresponding 4/2-byte field holds the
/// sentinel. Parsing is driven by the sentinel set, not by the extra alone.
fn resolve_zip64_cd(
    input: &[u8],
    extra: (u64, u64),
    uncompressed32: u32,
    compressed32: u32,
    local_header32: u32,
    disk_start: u16,
) -> Result<(u64, u64, u64, u16)> {
    let need_uncompressed = uncompressed32 == 0xFFFF_FFFF;
    let need_compressed = compressed32 == 0xFFFF_FFFF;
    let need_offset = local_header32 == 0xFFFF_FFFF;
    let need_disk = disk_start == 0xFFFF;
    if !(need_uncompressed || need_compressed || need_offset || need_disk) {
        return Ok((
            u64::from(uncompressed32),
            u64::from(compressed32),
            u64::from(local_header32),
            disk_start,
        ));
    }

    let extra_end = extra.0.saturating_add(extra.1);
    let mut cursor = extra.0;
    let mut zip64: Option<(u64, u64)> = None;
    while cursor.checked_add(4).is_some_and(|end| end <= extra_end) {
        let id = read_u16(input, cursor)
            .ok_or_else(|| Error::invalid_zip_structure("truncated extra field"))?;
        let size = u64::from(
            read_u16(input, cursor + 2)
                .ok_or_else(|| Error::invalid_zip_structure("truncated extra field"))?,
        );
        let data = cursor + 4;
        let data_end = data.saturating_add(size);
        if data_end > extra_end {
            break;
        }
        if id == EXTRA_ZIP64 {
            zip64 = Some((data, size));
            break;
        }
        cursor = data_end;
    }

    let (mut q, z_size) = zip64.ok_or_else(|| {
        Error::invalid_zip_structure("ZIP64 sentinel present without a 0x0001 extra field")
    })?;
    let z_end = q.saturating_add(z_size);
    let short = || Error::invalid_zip_structure("ZIP64 extra field is too short");
    let mut uncompressed = u64::from(uncompressed32);
    let mut compressed = u64::from(compressed32);
    let mut offset = u64::from(local_header32);
    let mut disk = disk_start;
    if need_uncompressed {
        if q.checked_add(8).is_none_or(|e| e > z_end) {
            return Err(short());
        }
        uncompressed = read_u64(input, q).ok_or_else(short)?;
        q += 8;
    }
    if need_compressed {
        if q.checked_add(8).is_none_or(|e| e > z_end) {
            return Err(short());
        }
        compressed = read_u64(input, q).ok_or_else(short)?;
        q += 8;
    }
    if need_offset {
        if q.checked_add(8).is_none_or(|e| e > z_end) {
            return Err(short());
        }
        offset = read_u64(input, q).ok_or_else(short)?;
        q += 8;
    }
    if need_disk {
        if q.checked_add(4).is_none_or(|e| e > z_end) {
            return Err(short());
        }
        disk = read_u16(input, q).unwrap_or(disk_start);
    }
    Ok((uncompressed, compressed, offset, disk))
}

/// Build a [`ZipMember`] (and its cover spans) from a parsed central entry.
fn build_member(
    input: &[u8],
    ordinal: u32,
    raw: &CdRaw,
    cd_offset: u64,
    limits: Limits,
) -> Result<(ZipMember, Vec<ZipSpan>)> {
    let lfh_offset = raw.local_header_offset;
    if lfh_offset.checked_add(30).is_none_or(|end| end > cd_offset) {
        return Err(Error::invalid_zip_structure(format!(
            "member {ordinal} local header offset {lfh_offset} is outside the local region"
        )));
    }
    if read_u32(input, lfh_offset) != Some(SIG_LOCAL_HEADER) {
        return Err(Error::invalid_zip_structure(format!(
            "member {ordinal} local header signature missing"
        )));
    }
    let lfh_name_len = u64::from(
        read_u16(input, lfh_offset + 26)
            .ok_or_else(|| Error::invalid_zip_structure("truncated local file header"))?,
    );
    let lfh_extra_len = u64::from(
        read_u16(input, lfh_offset + 28)
            .ok_or_else(|| Error::invalid_zip_structure("truncated local file header"))?,
    );
    if lfh_name_len > u64::from(limits.max_zip_name_bytes) {
        return Err(Error::resource_limit(format!(
            "member {ordinal} local name {lfh_name_len} bytes exceeds max_zip_name_bytes"
        )));
    }
    if lfh_extra_len > u64::from(limits.max_zip_extra_bytes) {
        return Err(Error::resource_limit(format!(
            "member {ordinal} local extra {lfh_extra_len} bytes exceeds max_zip_extra_bytes"
        )));
    }

    let lfh_len = 30 + lfh_name_len + lfh_extra_len;
    let data_start = lfh_offset + lfh_len;
    let data_end = data_start
        .checked_add(raw.compressed_size)
        .ok_or_else(|| Error::invalid_zip_structure("member data span overflows"))?;
    if data_end > cd_offset {
        return Err(Error::invalid_zip_structure(format!(
            "member {ordinal} data extends into the central directory"
        )));
    }

    if raw.compressed_size > limits.max_zip_member_compressed {
        return Err(Error::resource_limit(format!(
            "member {ordinal} declared compressed size {} exceeds max_zip_member_compressed {}",
            raw.compressed_size, limits.max_zip_member_compressed
        )));
    }
    if raw.uncompressed_size > limits.max_zip_member_uncompressed {
        return Err(Error::resource_limit(format!(
            "member {ordinal} declared uncompressed size {} exceeds max_zip_member_uncompressed {}",
            raw.uncompressed_size, limits.max_zip_member_uncompressed
        )));
    }
    if raw.compressed_size == 0 {
        if raw.uncompressed_size > 0 {
            return Err(Error::resource_limit(format!(
                "member {ordinal} declares {0} compressed bytes for {} uncompressed bytes",
                raw.uncompressed_size
            )));
        }
    } else if raw.uncompressed_size
        > raw
            .compressed_size
            .saturating_mul(u64::from(limits.max_zip_compression_ratio))
    {
        return Err(Error::resource_limit(format!(
            "member {ordinal} compression ratio exceeds max_zip_compression_ratio {}",
            limits.max_zip_compression_ratio
        )));
    }

    let data_descriptor = if raw.flags & FLAG_DATA_DESCRIPTOR != 0 {
        let (dl, style) = parse_descriptor(
            input,
            data_end,
            raw.crc32,
            raw.compressed_size,
            raw.uncompressed_size,
        )
        .ok_or_else(|| {
            Error::invalid_zip_structure(format!(
                "member {ordinal} sets the data-descriptor flag but no valid descriptor follows"
            ))
        })?;
        let dd_end = data_end
            .checked_add(dl)
            .ok_or_else(|| Error::invalid_zip_structure("data descriptor overflow"))?;
        if dd_end > cd_offset {
            return Err(Error::invalid_zip_structure(format!(
                "member {ordinal} data descriptor extends into the central directory"
            )));
        }
        Some(DataDescriptor {
            span: (data_end, dl),
            style,
        })
    } else {
        None
    };

    let name = read_bytes(input, raw.name_span)
        .ok_or_else(|| Error::invalid_zip_structure("member name lies outside the input"))?
        .to_vec();

    let mut extra_spans = Vec::new();
    if lfh_extra_len > 0 {
        extra_spans.push((lfh_offset + 30 + lfh_name_len, lfh_extra_len));
    }
    if raw.extra_span.1 > 0 {
        extra_spans.push(raw.extra_span);
    }

    let mut spans = Vec::with_capacity(3);
    spans.push(ZipSpan {
        start: lfh_offset,
        len: lfh_len,
        kind: ZipSpanKind::LocalHeader,
    });
    if raw.compressed_size > 0 {
        spans.push(ZipSpan {
            start: data_start,
            len: raw.compressed_size,
            kind: ZipSpanKind::MemberData,
        });
    }
    if let Some(dd) = &data_descriptor {
        spans.push(ZipSpan {
            start: dd.span.0,
            len: dd.span.1,
            kind: ZipSpanKind::DataDescriptor(dd.style),
        });
    }

    let member = ZipMember {
        id: PhysicalMemberId {
            ordinal,
            local_header_offset: lfh_offset,
        },
        central_index: ordinal,
        name,
        local_header_offset: lfh_offset,
        local_header: (lfh_offset, lfh_len),
        data: (data_start, raw.compressed_size),
        method: raw.method,
        flags: raw.flags,
        crc32: raw.crc32,
        compressed_size: raw.compressed_size,
        uncompressed_size: raw.uncompressed_size,
        extra_spans,
        data_descriptor,
        comment: raw.comment_span,
    };
    Ok((member, spans))
}

/// Parse a data descriptor at `at`, matching it against the declared fields.
///
/// Tries the signed layouts first (the signature is optional and a real CRC may
/// equal `0x08074b50`), then the unsigned layouts; 32-bit sizes before 64-bit.
fn parse_descriptor(
    input: &[u8],
    at: u64,
    crc: u32,
    compressed: u64,
    uncompressed: u64,
) -> Option<(u64, DescriptorStyle)> {
    let first = read_u32(input, at)?;
    let fits32 = compressed <= u64::from(u32::MAX) && uncompressed <= u64::from(u32::MAX);
    if first == SIG_DATA_DESCRIPTOR {
        if fits32
            && read_u32(input, at + 4) == Some(crc)
            && read_u32(input, at + 8) == Some(compressed as u32)
            && read_u32(input, at + 12) == Some(uncompressed as u32)
        {
            return Some((16, DescriptorStyle::Signature));
        }
        if read_u32(input, at + 4) == Some(crc)
            && read_u64(input, at + 8) == Some(compressed)
            && read_u64(input, at + 16) == Some(uncompressed)
        {
            return Some((24, DescriptorStyle::Signature));
        }
    }
    if fits32
        && first == crc
        && read_u32(input, at + 4) == Some(compressed as u32)
        && read_u32(input, at + 8) == Some(uncompressed as u32)
    {
        return Some((12, DescriptorStyle::NoSignature));
    }
    if first == crc
        && read_u64(input, at + 4) == Some(compressed)
        && read_u64(input, at + 12) == Some(uncompressed)
    {
        return Some((20, DescriptorStyle::NoSignature));
    }
    None
}

/// Assemble the final cover from classified core spans, filling gaps.
fn assemble_cover(
    mut core: Vec<ZipSpan>,
    input: &[u8],
    file_len: u64,
    limits: Limits,
) -> Result<Vec<ZipSpan>> {
    core.retain(|s| s.len > 0);
    core.sort_by_key(|a| (a.start, a.len));

    let mut cover: Vec<ZipSpan> = Vec::with_capacity(core.len() + 2);
    let mut cursor: u64 = 0;
    for span in core {
        if span.start < cursor {
            return Err(Error::invalid_zip_structure(format!(
                "overlapping ZIP spans at offset {} (cover cursor {cursor})",
                span.start
            )));
        }
        if span.start > cursor {
            emit_gap(input, cursor, span.start, limits, &mut cover)?;
        }
        cover.push(span);
        cursor = span.end();
    }
    if cursor < file_len {
        let len = file_len - cursor;
        if len > limits.max_zip_trailing_bytes {
            return Err(Error::resource_limit(format!(
                "trailing bytes {len} exceeds max_zip_trailing_bytes {}",
                limits.max_zip_trailing_bytes
            )));
        }
        cover.push(ZipSpan {
            start: cursor,
            len,
            kind: ZipSpanKind::Trailing,
        });
    }
    Ok(cover)
}

/// Classify the bytes of a gap: the leading gap is a prefix; interior gaps are
/// scanned for archive-extra-data records and otherwise left `Unclassified`.
fn emit_gap(
    input: &[u8],
    start: u64,
    end: u64,
    limits: Limits,
    cover: &mut Vec<ZipSpan>,
) -> Result<()> {
    if start == 0 {
        let len = end;
        if len > limits.max_zip_prefix_bytes {
            return Err(Error::resource_limit(format!(
                "prefix {len} bytes exceeds max_zip_prefix_bytes {}",
                limits.max_zip_prefix_bytes
            )));
        }
        cover.push(ZipSpan {
            start: 0,
            len,
            kind: ZipSpanKind::Prefix,
        });
        return Ok(());
    }
    let mut p = start;
    while p < end {
        if let Some(size) = read_u32(input, p + 4)
            && read_u32(input, p) == Some(SIG_ARCHIVE_EXTRA_DATA)
        {
            let total = 8 + u64::from(size);
            if p.checked_add(total).is_some_and(|e| e <= end) {
                cover.push(ZipSpan {
                    start: p,
                    len: total,
                    kind: ZipSpanKind::ArchiveExtraData,
                });
                p += total;
                continue;
            }
        }
        cover.push(ZipSpan {
            start: p,
            len: end - p,
            kind: ZipSpanKind::Unclassified,
        });
        p = end;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Bounded little-endian readers
// ---------------------------------------------------------------------------

fn read_bytes(input: &[u8], span: (u64, u64)) -> Option<&[u8]> {
    let start = usize::try_from(span.0).ok()?;
    let len = usize::try_from(span.1).ok()?;
    input.get(start..start.checked_add(len)?)
}

fn read_u16(input: &[u8], off: u64) -> Option<u16> {
    let off = usize::try_from(off).ok()?;
    let bytes = input.get(off..off.checked_add(2)?)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(input: &[u8], off: u64) -> Option<u32> {
    let off = usize::try_from(off).ok()?;
    let bytes = input.get(off..off.checked_add(4)?)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u64(input: &[u8], off: u64) -> Option<u64> {
    let off = usize::try_from(off).ok()?;
    let bytes = input.get(off..off.checked_add(8)?)?;
    Some(u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_end_saturates() {
        let s = ZipSpan {
            start: u64::MAX - 1,
            len: 10,
            kind: ZipSpanKind::Unclassified,
        };
        assert_eq!(s.end(), u64::MAX);
    }

    #[test]
    fn classify_name_detects_hazards() {
        assert_eq!(classify_name(b"word/document.xml"), None);
        assert_eq!(classify_name(b"../etc/passwd"), Some(NameHazard::Traversal));
        assert_eq!(classify_name(b"/abs/x"), Some(NameHazard::Absolute));
        assert_eq!(classify_name(b"C:\\win"), Some(NameHazard::Drive));
        assert_eq!(classify_name(b"a\\b"), Some(NameHazard::Backslash));
        assert_eq!(classify_name(b"a\0b"), Some(NameHazard::Nul));
        assert_eq!(classify_name(b"a\x01b"), Some(NameHazard::Control));
        assert_eq!(classify_name(b"a/../b"), Some(NameHazard::Traversal));
        assert_eq!(classify_name(b"a..b"), None);
    }

    #[test]
    fn require_safe_name_is_typed() {
        let e = require_safe_name(b"../x").unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::InvalidZipStructure);
        assert!(require_safe_name(b"mimetype").is_ok());
    }

    #[test]
    fn empty_input_is_typed_error() {
        let e = scan(b"", Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::InvalidZipStructure);
    }

    #[test]
    fn crc_matches_known_vector() {
        // CRC-32/ISO-HDLC of "123456789" is 0xCBF43926.
        assert_eq!(crc32_iso_hdlc(b"123456789"), 0xCBF4_3926);
    }
}
