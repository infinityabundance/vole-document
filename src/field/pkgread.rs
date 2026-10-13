//! Phase 30.4 — unit-addressed package/file model construction.
//!
//! Phase 29.2 found the coarse structural index narrowed the natural *reading*
//! request for PDF only: the DOCX/ODT/Parquet format-model nodes materialize the
//! whole exact `DocumentExact` source, so their coarse units were built but
//! inert. This module makes those reading paths unit-addressed.
//!
//! ## Mechanism
//!
//! The coarse lane's [`SourceServer`] can serve two extra things the ordinary
//! sources cannot: the byte range of the ZIP central directory (`[cd_offset,
//! source_len)`) and *fine-grained* exact range reads. This module uses them to
//! reconstruct a **sparse image** of the source — a buffer of the source's own
//! length in which only the bytes a package reader actually consults are filled:
//!
//! * the whole ZIP central directory + end-of-central-directory record;
//! * every member's local file header (the authoritative name/extra lengths and
//!   the local data offset);
//! * the payload bytes of the few members the model decodes (the OPC metadata
//!   parts, or the ODF `mimetype`/`META-INF/manifest.xml`).
//!
//! Every other byte is left zero and is never read by the model builder — a
//! member the model does not decode contributes only its declared span, parsed
//! from the directory, not its data. The filled bytes are the source's own
//! bytes, so the derived model is *byte-identical* to the whole-source one; the
//! existing slice-based builders ([`crate::field::opc::build_opc_model`],
//! [`crate::adapter::odt::build_odt_model`]) are then called unchanged.
//!
//! ## Exactness and declination
//!
//! This path is never on the exactness path: `materialize --exact` does not call
//! it, and it adds no seed node (ADR-0060 unchanged). It is *bounded* and
//! *typed-declining*: anything outside the supported subset (ZIP64, multi-disk,
//! an inconsistent directory, a member larger than the reader's caps, a builder
//! error) returns `Ok(None)` and the caller falls back to the whole-source path,
//! which is always available. A decoded member's answer is additionally checked
//! against the full-read answer by the court (`matches_full`).

use crate::error::Result;
use crate::field::dag::SourceServer;
use crate::limits::Limits;

/// The ZIP end-of-central-directory signature.
const SIG_EOCD: &[u8; 4] = b"PK\x05\x06";
/// The ZIP central-file-header signature.
const SIG_CENTRAL: &[u8; 4] = b"PK\x01\x02";
/// Fixed local file header size before name/extra.
const LOCAL_HEADER_FIXED: u64 = 30;

/// One central-directory entry, reduced to what a sparse image needs.
struct CdEntry {
    name: Vec<u8>,
    local_offset: u64,
    compressed_size: u64,
}

/// Build the OPC model from the coarse source's units, or `Ok(None)` to fall back.
pub fn opc_model_from_units(source: &dyn SourceServer, limits: Limits) -> Result<Option<Vec<u8>>> {
    #[cfg(feature = "opc")]
    {
        let Some(image) = sparse_image(source, limits, is_opc_metadata)? else {
            return Ok(None);
        };
        Ok(crate::field::opc::build_opc_model(&image, limits).ok())
    }
    #[cfg(not(feature = "opc"))]
    {
        let _ = (source, limits);
        Ok(None)
    }
}

/// Build the ODT (ODF) discovery model from the coarse source's units, or
/// `Ok(None)` to fall back.
pub fn odt_model_from_units(source: &dyn SourceServer, limits: Limits) -> Result<Option<Vec<u8>>> {
    #[cfg(feature = "odt")]
    {
        let Some(image) = sparse_image(source, limits, is_odf_part)? else {
            return Ok(None);
        };
        Ok(crate::adapter::odt::build_odt_model(&image, limits).ok())
    }
    #[cfg(not(feature = "odt"))]
    {
        let _ = (source, limits);
        Ok(None)
    }
}

/// Whether a member is an OPC metadata part: `[Content_Types].xml`, `_rels/.rels`,
/// or a `*/_rels/*.rels` part-relationships member. Mirrors the predicate in
/// [`crate::field::opc::build_opc_model`].
fn is_opc_metadata(name: &[u8]) -> bool {
    let Ok(s) = core::str::from_utf8(name) else {
        return false;
    };
    s.eq_ignore_ascii_case("[Content_Types].xml")
        || s.eq_ignore_ascii_case("_rels/.rels")
        || crate::field::opc::part_rels_owner_member(s).is_some()
}

/// Whether a member is one of the two ODF members the discovery model decodes.
fn is_odf_part(name: &[u8]) -> bool {
    name == b"mimetype" || name == b"META-INF/manifest.xml"
}

/// Reconstruct a sparse but byte-faithful image of the source.
///
/// `None` means the source is not unit-addressed over a ZIP, or the archive is
/// outside the supported subset; the caller falls back to the whole-source path.
fn sparse_image(
    source: &dyn SourceServer,
    limits: Limits,
    select: fn(&[u8]) -> bool,
) -> Result<Option<Vec<u8>>> {
    let Some(len) = source.exact_len() else {
        return Ok(None);
    };
    if len == 0 || len > limits.max_input_bytes {
        return Ok(None);
    }
    let Ok(len_usize) = usize::try_from(len) else {
        return Ok(None);
    };
    let Some((cd_off, dir)) = source.zip_directory(limits)? else {
        return Ok(None);
    };
    // The directory region must be exactly `[cd_off, len)`.
    if cd_off.saturating_add(dir.len() as u64) != len {
        return Ok(None);
    }
    let entries = match parse_directory(&dir, cd_off) {
        Some(e) => e,
        None => return Ok(None),
    };

    let mut image = vec![0u8; len_usize];
    image[cd_off as usize..].copy_from_slice(&dir);

    for e in &entries {
        // The local header is authoritative for the data offset.
        let Some(head) = source.read_exact(e.local_offset, LOCAL_HEADER_FIXED, limits)? else {
            return Ok(None);
        };
        if head.len() != LOCAL_HEADER_FIXED as usize || &head[0..4] != b"PK\x03\x04" {
            return Ok(None);
        }
        let name_len = u64::from(u16le(&head, 26).unwrap_or(u16::MAX));
        let extra_len = u64::from(u16le(&head, 28).unwrap_or(u16::MAX));
        let Some(lfh_len) = LOCAL_HEADER_FIXED
            .checked_add(name_len)
            .and_then(|v| v.checked_add(extra_len))
        else {
            return Ok(None);
        };
        let Some(data_off) = e.local_offset.checked_add(lfh_len) else {
            return Ok(None);
        };
        let Some(data_end) = data_off.checked_add(e.compressed_size) else {
            return Ok(None);
        };
        if data_end > cd_off || data_end > len {
            return Ok(None);
        }
        // Fill the local header (and, for members the model decodes, the payload).
        let fill_end = if select(&e.name) { data_end } else { data_off };
        let Some(span) = source.read_exact(e.local_offset, fill_end - e.local_offset, limits)?
        else {
            return Ok(None);
        };
        if span.len() as u64 != fill_end - e.local_offset {
            return Ok(None);
        }
        image[e.local_offset as usize..fill_end as usize].copy_from_slice(&span);
    }
    Ok(Some(image))
}

/// Parse the central directory (and the EOCD that follows it) from the directory
/// region. `None` for ZIP64/multi-disk/inconsistent geometry.
fn parse_directory(dir: &[u8], cd_off: u64) -> Option<Vec<CdEntry>> {
    let eocd = rfind_eocd(dir)?;
    let entries = u16le(dir, eocd + 10)?;
    let cd_size = u32le(dir, eocd + 12)?;
    let cd_off_field = u32le(dir, eocd + 16)?;
    if entries == u16::MAX || cd_size == u32::MAX || cd_off_field == u32::MAX {
        return None;
    }
    if u64::from(cd_off_field) != cd_off {
        return None;
    }
    let cd = dir.get(0..cd_size as usize)?;
    let mut out = Vec::new();
    let mut p = 0usize;
    while p < cd.len() {
        if cd.get(p..p + 4)? != SIG_CENTRAL {
            return None;
        }
        let comp = u32le(cd, p + 20)?;
        let uncomp = u32le(cd, p + 24)?;
        let name_len = u16le(cd, p + 28)? as usize;
        let extra_len = u16le(cd, p + 30)? as usize;
        let comment_len = u16le(cd, p + 32)? as usize;
        let lo = u32le(cd, p + 42)?;
        if comp == u32::MAX || uncomp == u32::MAX || lo == u32::MAX {
            return None;
        }
        let name = cd.get(p + 46..p + 46 + name_len)?.to_vec();
        out.push(CdEntry {
            name,
            local_offset: u64::from(lo),
            compressed_size: u64::from(comp),
        });
        p = p.checked_add(46 + name_len + extra_len + comment_len)?;
    }
    if out.len() != entries as usize {
        return None;
    }
    Some(out)
}

/// Offset of the last end-of-central-directory signature in `dir`.
fn rfind_eocd(dir: &[u8]) -> Option<usize> {
    if dir.len() < 22 {
        return None;
    }
    (0..=dir.len() - SIG_EOCD.len())
        .rev()
        .find(|&i| &dir[i..i + SIG_EOCD.len()] == SIG_EOCD)
}

fn u16le(d: &[u8], i: usize) -> Option<u16> {
    let s = d.get(i..i + 2)?;
    Some(u16::from_le_bytes([s[0], s[1]]))
}

fn u32le(d: &[u8], i: usize) -> Option<u32> {
    let s = d.get(i..i + 4)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}
