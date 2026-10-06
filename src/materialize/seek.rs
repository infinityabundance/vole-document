//! Seek-based partial materialization (Phase 8.2).
//!
//! [`materialize_observation_seeked`] serves the same narrow byte range as
//! [`crate::materialize::observation::materialize_observation`], but from a
//! `Read + Seek` source and by reading **only the records the query needs**. It
//! does this by reading the optional `DIRECTORY` record (written as the first
//! record at offset 64), which locates every record class, and then seeking
//! directly to the `GRAPH`, `OBSERVATION_INDEX`, `INTEGRITY`, and the referenced
//! `OBJECT`/`ENTROPY_CHANNEL`/`MODEL` records.
//!
//! ## Decline, never guess
//!
//! A descriptor whose header does not advertise the seek feature, or whose first
//! record is not a `DIRECTORY`, is declined with
//! [`crate::ErrorClass::UnsupportedFeature`]; the reader never silently falls
//! back to reading the whole file. Op selection, lazy channel decoding, and the
//! evaluated slice are shared verbatim with the Phase-7 in-memory path
//! ([`crate::materialize::observation::select_ops`] / `serve_selection`), so the
//! two readers cannot diverge.
//!
//! ## The directory is advisory, never authority
//!
//! Every locator is cross-checked against the record it points at (tag byte and
//! payload length), every record's own CRC32C is verified when it is read, the
//! directory's internal geometry is validated by
//! [`SeekDirectory::validate_structural`], and the `ObservationIndex` is
//! re-derived against the directory-derived object/channel lengths with
//! [`ObservationIndex::validate`]. A lying directory is rejected, never trusted.
//!
//! ## Integrity is honest
//!
//! A partial read cannot recompute the whole-source SHA-256, so a served slice is
//! an *observation* consistent with the descriptor's own validated
//! program/index/directory — **not** a verified archival read. The returned
//! [`ObservationStats`] carries `integrity_verified == false` and a real
//! `bytes_read` measured by the internal [`CountingReader`]. Only
//! `materialize`/`decode`/`verify` check `INTEGRITY` and are the archival
//! authority.

use std::io::{self, Read, Seek, SeekFrom};

use crate::container::directory::{
    DirectoryEntry, SECTION_CHANNEL_LENGTHS, SECTION_LOCATORS, SeekDirectory,
};
use crate::container::header::{FEATURE_SEEK_DIRECTORY, HEADER_LEN, Header};
use crate::container::observation::ObservationIndex;
use crate::container::record::{FLAG_OPTIONAL, RECORD_OVERHEAD, Record, RecordTag, read_record_at};
use crate::dra::Program;
use crate::entropy::codec::EntropyChannelDescriptor;
use crate::entropy::model::EntropyModel;
use crate::entropy::{CODER_ORDER0_BYTE_RANS, CODER_VERSION_1};
use crate::error::{Error, Result};
use crate::limits::Limits;
use crate::materialize::observation::{
    ObservationReport, ObservationSelector, ObservationStats, resolve_selector, select_ops,
    selection_references, serve_selection,
};

/// A `Read + Seek` wrapper that counts the bytes actually returned by `read` and
/// the number of `read`/`seek` calls.
///
/// This is the primary instrument for the Phase-8 bytes-read claim: it is exact,
/// deterministic, attributes bytes to VOLE (not to a loader or pipe read-ahead),
/// and is immune to page-cache effects. The library wraps one internally in
/// [`materialize_observation_seeked`] and reports `bytes_read`; it is exposed so
/// callers and tests can measure I/O directly.
#[derive(Debug)]
pub struct CountingReader<R> {
    inner: R,
    bytes_read: u64,
    read_calls: u32,
    seeks: u32,
}

impl<R> CountingReader<R> {
    /// Wrap `inner`, starting all counters at zero.
    pub fn new(inner: R) -> Self {
        CountingReader {
            inner,
            bytes_read: 0,
            read_calls: 0,
            seeks: 0,
        }
    }

    /// Total payload bytes returned by `read` so far.
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    /// Number of `read` calls (each may return fewer bytes than requested).
    pub fn read_calls(&self) -> u32 {
        self.read_calls
    }

    /// Number of `seek` calls.
    pub fn seeks(&self) -> u32 {
        self.seeks
    }

    /// Unwrap the inner reader.
    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.bytes_read += n as u64;
        self.read_calls += 1;
        Ok(n)
    }
}

impl<R: Seek> Seek for CountingReader<R> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.seeks += 1;
        self.inner.seek(pos)
    }
}

/// Serve one observation from a seekable `.voldoc` source by reading only the
/// records the query needs.
///
/// The returned bytes equal `materialize(parsed)[a..b]` for the resolved range.
/// A source without a seek directory is declined (never silently fully read).
/// See the module documentation for the validation and integrity rules.
pub fn materialize_observation_seeked<R: Read + Seek>(
    reader: R,
    selector: ObservationSelector,
    limits: Limits,
) -> Result<ObservationReport> {
    let mut reader = CountingReader::new(reader);
    let file_len = reader
        .seek(SeekFrom::End(0))
        .map_err(|e| Error::io(format!("seek to end failed: {e}")))?;

    // (a) Header, then the fixed-offset DIRECTORY record.
    let header = read_header(&mut reader)?;
    if header.optional_features & FEATURE_SEEK_DIRECTORY == 0 {
        return Err(Error::unsupported_feature(
            "descriptor has no seek directory (the header does not declare the seek feature)",
        ));
    }
    let dir_rec = read_record_at(&mut reader, HEADER_LEN as u64, limits)?;
    if dir_rec.tag != RecordTag::Directory as u8 {
        return Err(Error::unsupported_feature(
            "descriptor has no seek directory record at offset 64",
        ));
    }
    if dir_rec.flags & FLAG_OPTIONAL == 0 {
        return Err(Error::invalid_container(
            "DIRECTORY record must carry FLAG_OPTIONAL",
        ));
    }
    let dir = SeekDirectory::decode(&dir_rec.payload, limits)?;
    dir.validate_structural(file_len, limits)?;
    if dir.section_flags & SECTION_LOCATORS == 0 {
        return Err(Error::unsupported_feature(
            "seek directory omits the locator section",
        ));
    }
    if dir.entries[0].payload_len != dir_rec.payload.len() as u32 {
        return Err(Error::invalid_container(
            "seek directory locator 0 payload length disagrees with the record framing",
        ));
    }

    let object_entries = class_entries(&dir, RecordTag::Object)?;
    let channel_entries = class_entries(&dir, RecordTag::EntropyChannel)?;
    let model_entries = class_entries(&dir, RecordTag::Model)?;
    if !channel_entries.is_empty() && dir.section_flags & SECTION_CHANNEL_LENGTHS == 0 {
        return Err(Error::unsupported_feature(
            "seek directory omits the channel-lengths section",
        ));
    }
    if dir.channel_lengths.len() != channel_entries.len() {
        return Err(Error::invalid_container(
            "seek directory channel-length table disagrees with the channel locators",
        ));
    }
    let object_lens: Vec<u64> = object_entries
        .iter()
        .map(|e| u64::from(e.payload_len))
        .collect();
    let channel_lens: Vec<u64> = dir.channel_lengths.clone();

    // (b) GRAPH and OBSERVATION_INDEX (the reader learns op->output and deps
    // only from these), plus INTEGRITY for the declared source length.
    let graph_site = class_entries(&dir, RecordTag::Graph)?
        .first()
        .ok_or_else(|| {
            Error::unsupported_feature("seek directory does not locate a GRAPH record")
        })?;
    let index_site = class_entries(&dir, RecordTag::ObservationIndex)?
        .first()
        .ok_or_else(|| {
            Error::unsupported_feature("seek directory does not locate an OBSERVATION_INDEX record")
        })?;
    let integrity_site = class_entries(&dir, RecordTag::Integrity)?
        .first()
        .ok_or_else(|| {
            Error::unsupported_feature("seek directory does not locate an INTEGRITY record")
        })?;

    let graph_rec = read_checked(&mut reader, graph_site, limits)?;
    let program = Program::decode(&graph_rec.payload, limits)?;
    let index_rec = read_checked(&mut reader, index_site, limits)?;
    let index = ObservationIndex::decode(&index_rec.payload, limits)?;
    let integrity_rec = read_checked(&mut reader, integrity_site, limits)?;
    if integrity_rec.payload.len() != 40 {
        return Err(Error::invalid_container(
            "INTEGRITY payload must be 40 bytes",
        ));
    }
    let declared_len = u64::from_le_bytes([
        integrity_rec.payload[32],
        integrity_rec.payload[33],
        integrity_rec.payload[34],
        integrity_rec.payload[35],
        integrity_rec.payload[36],
        integrity_rec.payload[37],
        integrity_rec.payload[38],
        integrity_rec.payload[39],
    ]);
    if declared_len != header.declared_source_len {
        return Err(Error::integrity_mismatch(format!(
            "INTEGRITY length {declared_len} disagrees with header {}",
            header.declared_source_len
        )));
    }

    // The decisive cross-check: the index, re-derived over the *directory-derived*
    // lengths, must reproduce the program's op table and the CRC-framed index.
    index.validate(&program, &object_lens, &channel_lens, limits)?;

    // (c) Resolve the selector and compute the minimal op set/prefix.
    let (a, b) = resolve_selector(&index, selector, declared_len)?;
    let window = select_ops(&program, &object_lens, &channel_lens, a, b, limits)?;
    let (objects_used, channels_used) =
        selection_references(&window.ops, object_entries.len(), channel_entries.len());

    // (d) Read ONLY the referenced OBJECT records.
    let mut objects: Vec<Vec<u8>> = vec![Vec::new(); object_entries.len()];
    for (id, used) in objects_used.iter().enumerate() {
        if *used {
            objects[id] = read_checked(&mut reader, &object_entries[id], limits)?.payload;
        }
    }

    // (d) Read ONLY the referenced ENTROPY_CHANNEL records, then the MODEL records
    // they name.
    let mut channels: Vec<EntropyChannelDescriptor> =
        vec![placeholder_channel(); channel_entries.len()];
    let mut models_needed = vec![false; model_entries.len()];
    for (id, used) in channels_used.iter().enumerate() {
        if *used {
            let rec = read_checked(&mut reader, &channel_entries[id], limits)?;
            let channel = EntropyChannelDescriptor::decode(&rec.payload, limits)?;
            if channel.decoded_length != channel_lens[id] {
                return Err(Error::invalid_container(format!(
                    "seek directory channel-length {id} disagrees with the channel record"
                )));
            }
            if channel.model_id as usize >= model_entries.len() {
                return Err(Error::invalid_model(format!(
                    "entropy channel {id} references missing model {}",
                    channel.model_id
                )));
            }
            models_needed[channel.model_id as usize] = true;
            channels[id] = channel;
        }
    }
    let mut models: Vec<EntropyModel> = vec![placeholder_model(); model_entries.len()];
    for (id, used) in models_needed.iter().enumerate() {
        if *used {
            let rec = read_checked(&mut reader, &model_entries[id], limits)?;
            models[id] = EntropyModel::decode(&rec.payload)?;
        }
    }

    // (e) Evaluate the selected ops and slice `[a, b)` exactly as Phase 7 does.
    let served = serve_selection(
        &objects,
        &channels,
        &models,
        window,
        &objects_used,
        &channels_used,
        a,
        b,
        limits,
    )?;

    // `descriptor_bytes_traversed` stays comparable with the Phase-7 path: graph
    // payload + index payload (+ framing) + referenced object/channel payloads.
    let descriptor_bytes_traversed = graph_rec.payload.len() as u64
        + index_rec.payload.len() as u64
        + RECORD_OVERHEAD as u64
        + served.referenced_object_bytes
        + served.referenced_channel_bytes;

    let stats = ObservationStats {
        ops_evaluated: served.ops_evaluated,
        ops_total: served.ops_total,
        objects_fetched: served.objects_fetched,
        objects_total: object_entries.len(),
        channels_decoded: served.channels_decoded,
        channels_total: channel_entries.len(),
        entropy_bytes_decoded: served.entropy_bytes_decoded,
        descriptor_bytes_traversed,
        output_bytes: served.bytes.len() as u64,
        // (f) Real I/O, distinct from the Phase-7 CPU-side approximation.
        bytes_read: reader.bytes_read(),
        integrity_verified: false,
    };

    Ok(ObservationReport {
        range: (a, b),
        bytes: served.bytes,
        stats,
    })
}

/// Read and decode the fixed 64-byte header from the start of the source.
fn read_header<R: Read + Seek>(reader: &mut R) -> Result<Header> {
    reader
        .seek(SeekFrom::Start(0))
        .map_err(|e| Error::io(format!("seek to header failed: {e}")))?;
    let mut buf = [0u8; HEADER_LEN];
    reader
        .read_exact(&mut buf)
        .map_err(|_| Error::invalid_container("truncated header"))?;
    Header::decode(&buf)
}

/// Read the record at `site` and require its framing to match the locator.
fn read_checked<R: Read + Seek>(
    reader: &mut R,
    site: &DirectoryEntry,
    limits: Limits,
) -> Result<Record> {
    let rec = read_record_at(reader, site.offset, limits)?;
    if rec.tag != site.tag || rec.payload.len() as u64 != u64::from(site.payload_len) {
        return Err(Error::invalid_container(format!(
            "DIRECTORY locator for tag {:#04x} disagrees with the record framing at offset {}",
            site.tag, site.offset
        )));
    }
    Ok(rec)
}

/// The locators of `tag`, using the directory's `CLASS_INDEX` and re-checking it
/// against a linear scan of the locator table (defence in depth;
/// [`SeekDirectory::validate_structural`] has already required agreement).
fn class_entries(dir: &SeekDirectory, tag: RecordTag) -> Result<&[DirectoryEntry]> {
    let (first, count) = match dir.classes.iter().find(|c| c.tag == tag as u8) {
        Some(c) => (c.first as usize, c.count as usize),
        None => (0, 0),
    };
    let scan_first = dir.entries.iter().position(|e| e.tag == tag as u8);
    let scan_count = dir.entries.iter().filter(|e| e.tag == tag as u8).count();
    let expected_first = if count == 0 { None } else { Some(first) };
    if scan_first != expected_first || scan_count != count {
        return Err(Error::invalid_container(
            "seek directory class index disagrees with the locator table",
        ));
    }
    if count == 0 {
        return Ok(&[]);
    }
    let end = first
        .checked_add(count)
        .ok_or_else(|| Error::invalid_container("seek directory class range overflow"))?;
    dir.entries
        .get(first..end)
        .ok_or_else(|| Error::invalid_container("seek directory class range out of bounds"))
}

/// A never-dereferenced placeholder occupying an unreferenced channel slot, so
/// index positions in the partial vectors stay aligned with the descriptor's
/// own tables.
fn placeholder_channel() -> EntropyChannelDescriptor {
    EntropyChannelDescriptor {
        coder: CODER_ORDER0_BYTE_RANS,
        coder_version: CODER_VERSION_1,
        scale_bits: 0,
        lane_count: 1,
        model_id: 0,
        symbol_count: 0,
        decoded_length: 0,
        initial_state: 0,
        payload: Vec::new(),
    }
}

/// A never-dereferenced placeholder occupying an unreferenced model slot.
fn placeholder_model() -> EntropyModel {
    EntropyModel {
        scale_bits: 0,
        frequencies: Vec::new(),
    }
}
