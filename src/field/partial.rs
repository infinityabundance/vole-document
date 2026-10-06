//! Seek-based partial descriptor loading (Phase 11.12, review priority #2).
//!
//! [`PartialDescriptor`] serves exact source byte ranges without reading the
//! whole `.voldoc` blob. It walks the file's record framing once, reading only
//! the small records a query needs (`UNIVERSE`, `FORMAT`, `GRAPH`,
//! `OBSERVATION_INDEX`, `INTEGRITY`, `TRAILER`) and **seeking over** the payloads
//! of every `OBJECT`/`ENTROPY_CHANNEL`/`MODEL` record, remembering their on-disk
//! offsets. When a range is served it reads **only** the records the selected ops
//! reference, and every payload it materializes is CRC32C-verified.
//!
//! ## Requires an observation index
//!
//! The lazy lane needs an `OBSERVATION_INDEX` op table to learn each op's output
//! length without decoding every channel. [`PartialDescriptor::open`] returns
//! `Ok(None)` when the descriptor has no op table, references external objects,
//! or is otherwise ineligible; the caller then falls back to the full
//! [`crate::container::Descriptor::parse`] path (recorded honestly as a full
//! descriptor read).
//!
//! ## Integrity is honest
//!
//! A partial read cannot recompute the whole-source SHA-256, and it does not
//! re-hash the blob against its content id; it verifies each *record's* CRC32C as
//! the record is read (including every payload it materializes) and cross-checks
//! the op table against the program's own op shapes. A corrupted unneeded record
//! is not read, so it does not break a narrow observation. The delivered bytes
//! are an *observation*, never a verified archival read.

use std::cell::RefCell;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::container::directory::RecordSite;
use crate::container::header::{HEADER_LEN, Header};
use crate::container::observation::{ObservationIndex, SECTION_OP_TABLE, primary_dependency};
use crate::container::record::{RECORD_HEADER_LEN, RECORD_TRAILER_LEN, RecordTag, read_record_at};
use crate::dra::Program;
use crate::entropy::codec::EntropyChannelDescriptor;
use crate::entropy::model::EntropyModel;
use crate::error::{Error, Result};
use crate::limits::Limits;
use crate::materialize::observation::{
    select_ops_from_lengths, selection_references, serve_selection,
};
use crate::materialize::seek::CountingReader;

use super::dag::SourceServer;

/// A descriptor opened for seek-based partial reads.
///
/// Holds the small records read up front plus the on-disk locators of every
/// object/channel/model record. Every read goes through an internal
/// [`CountingReader`], so `bytes_read` is a real, deterministic I/O figure.
pub struct PartialDescriptor {
    reader: RefCell<CountingReader<File>>,
    source_len: u64,
    source_sha256: [u8; 32],
    object_sites: Vec<RecordSite>,
    channel_sites: Vec<RecordSite>,
    model_sites: Vec<RecordSite>,
    program: Program,
    /// Per-op output lengths, in program order, from the validated op table.
    per_op: Vec<u64>,
}

impl std::fmt::Debug for PartialDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PartialDescriptor")
            .field("source_len", &self.source_len)
            .field("objects", &self.object_sites.len())
            .field("channels", &self.channel_sites.len())
            .field("models", &self.model_sites.len())
            .field("graph_ops", &self.program.ops.len())
            .finish_non_exhaustive()
    }
}

/// The outcome of inspecting a descriptor for lazy serving.
pub enum PartialLoad {
    /// The descriptor can be served lazily.
    Ready(Box<PartialDescriptor>),
    /// The descriptor is ineligible; `bytes_read` is the (small) number of bytes
    /// the inspection fetched before declining.
    Ineligible { bytes_read: u64 },
}

impl PartialDescriptor {
    /// Walk the framing of the descriptor at `path`, reading only the small
    /// records. Returns [`PartialLoad::Ineligible`] when the descriptor cannot be
    /// served lazily (no op table or external objects): the caller must then use
    /// the full path.
    pub fn open(path: &Path, limits: Limits) -> Result<PartialLoad> {
        let file = File::open(path).map_err(|e| Error::io(format!("opening descriptor: {e}")))?;
        let mut reader = CountingReader::new(file);

        let mut hdr = [0u8; HEADER_LEN];
        reader
            .seek(SeekFrom::Start(0))
            .map_err(|e| Error::io(format!("seek to header failed: {e}")))?;
        reader
            .read_exact(&mut hdr)
            .map_err(|_| Error::invalid_container("truncated header"))?;
        let header = Header::decode(&hdr)?;

        let mut universe: Option<String> = None;
        let mut graph: Option<Program> = None;
        let mut index: Option<ObservationIndex> = None;
        let mut source_sha256: Option<[u8; 32]> = None;
        let mut source_len: Option<u64> = None;
        let mut object_sites: Vec<RecordSite> = Vec::new();
        let mut channel_sites: Vec<RecordSite> = Vec::new();
        let mut model_sites: Vec<RecordSite> = Vec::new();

        let file_len = reader
            .seek(SeekFrom::End(0))
            .map_err(|e| Error::io(format!("seek to end failed: {e}")))?;
        let mut pos: u64 = HEADER_LEN as u64;
        while pos < file_len {
            reader
                .seek(SeekFrom::Start(pos))
                .map_err(|e| Error::io(format!("seek to record failed: {e}")))?;
            let mut rec_hdr = [0u8; RECORD_HEADER_LEN];
            reader
                .read_exact(&mut rec_hdr)
                .map_err(|_| Error::invalid_container("truncated record header"))?;
            let tag = rec_hdr[0];
            let _flags = rec_hdr[1];
            let reserved = u16::from_le_bytes([rec_hdr[2], rec_hdr[3]]);
            if reserved != 0 {
                return Err(Error::invalid_container(
                    "record reserved field must be zero",
                ));
            }
            let len = u32::from_le_bytes([rec_hdr[4], rec_hdr[5], rec_hdr[6], rec_hdr[7]]);
            if len > limits.max_record_len {
                return Err(Error::resource_limit(format!(
                    "record payload length {len} exceeds limit {}",
                    limits.max_record_len
                )));
            }
            let total = RECORD_HEADER_LEN as u64 + u64::from(len) + RECORD_TRAILER_LEN as u64;
            let end = pos
                .checked_add(total)
                .ok_or_else(|| Error::invalid_container("record length overflow"))?;
            if end > file_len {
                return Err(Error::invalid_container("truncated record"));
            }

            let site = RecordSite {
                tag,
                offset: pos,
                payload_len: len,
            };
            match RecordTag::from_u8(tag) {
                Some(RecordTag::Universe) => {
                    let rec = read_record_at(&mut reader, pos, limits)?;
                    universe = Some(
                        String::from_utf8(rec.payload)
                            .map_err(|_| Error::invalid_container("UNIVERSE is not UTF-8"))?,
                    );
                }
                Some(RecordTag::Graph) => {
                    let rec = read_record_at(&mut reader, pos, limits)?;
                    graph = Some(Program::decode(&rec.payload, limits)?);
                }
                Some(RecordTag::ObservationIndex) => {
                    let rec = read_record_at(&mut reader, pos, limits)?;
                    index = Some(ObservationIndex::decode(&rec.payload, limits)?);
                }
                Some(RecordTag::Integrity) => {
                    let rec = read_record_at(&mut reader, pos, limits)?;
                    if rec.payload.len() != 40 {
                        return Err(Error::invalid_container(
                            "INTEGRITY payload must be 40 bytes",
                        ));
                    }
                    let mut sha = [0u8; 32];
                    sha.copy_from_slice(&rec.payload[0..32]);
                    source_sha256 = Some(sha);
                    source_len = Some(u64::from_le_bytes([
                        rec.payload[32],
                        rec.payload[33],
                        rec.payload[34],
                        rec.payload[35],
                        rec.payload[36],
                        rec.payload[37],
                        rec.payload[38],
                        rec.payload[39],
                    ]));
                }
                Some(RecordTag::Object) => object_sites.push(site),
                Some(RecordTag::EntropyChannel) => channel_sites.push(site),
                Some(RecordTag::Model) => model_sites.push(site),
                Some(RecordTag::ExternalRef) => {
                    // The lazy lane has no resolver; decline rather than serve
                    // wrong bytes.
                    return Ok(PartialLoad::Ineligible {
                        bytes_read: reader.bytes_read(),
                    });
                }
                Some(RecordTag::Trailer) => {
                    // Verify the trailer's CRC and stop; it is the last record.
                    let _ = read_record_at(&mut reader, pos, limits)?;
                    break;
                }
                // FORMAT, DIRECTORY, RESIDUAL, CHECKPOINT, unknown: skipped.
                _ => {}
            }
            pos = end;
        }

        // The slow lane is only admissible with the small records that define the
        // source and the program/index that define op geometry.
        let (Some(universe), Some(program), Some(index), Some(source_sha256), Some(source_len)) =
            (universe, graph, index, source_sha256, source_len)
        else {
            return Ok(PartialLoad::Ineligible {
                bytes_read: reader.bytes_read(),
            });
        };
        if header.universe_id != crate::container::universe_id_from_str(&universe) {
            return Err(Error::invalid_container(
                "UNIVERSE record does not match the header universe id",
            ));
        }
        if index.section_flags & SECTION_OP_TABLE == 0 || index.ops.len() != program.ops.len() {
            // No usable op table: fall back to the full path.
            return Ok(PartialLoad::Ineligible {
                bytes_read: reader.bytes_read(),
            });
        }

        let object_lens: Vec<u64> = object_sites
            .iter()
            .map(|s| u64::from(s.payload_len))
            .collect();
        let per_op = validate_op_table(&program, &index, &object_lens, source_len)?;

        Ok(PartialLoad::Ready(Box::new(PartialDescriptor {
            reader: RefCell::new(reader),
            source_len,
            source_sha256,
            object_sites,
            channel_sites,
            model_sites,
            program,
            per_op,
        })))
    }

    /// Real bytes fetched from the descriptor file by this loader.
    pub fn bytes_read(&self) -> u64 {
        self.reader.borrow().bytes_read()
    }

    /// Declared source length (from the `INTEGRITY` record).
    pub fn source_len(&self) -> u64 {
        self.source_len
    }

    /// Declared whole-source SHA-256 (from the `INTEGRITY` record).
    pub fn source_sha256(&self) -> [u8; 32] {
        self.source_sha256
    }

    /// Number of raw object records in the descriptor.
    pub fn object_count(&self) -> usize {
        self.object_sites.len()
    }

    /// Number of instructions in the reconstruction program.
    pub fn graph_ops(&self) -> usize {
        self.program.ops.len()
    }

    fn read_site(&self, site: &RecordSite, limits: Limits) -> Result<Vec<u8>> {
        let mut reader = self.reader.borrow_mut();
        let rec = read_record_at(&mut *reader, site.offset, limits)?;
        if rec.tag != site.tag || rec.payload.len() as u64 != u64::from(site.payload_len) {
            return Err(Error::invalid_container(format!(
                "record framing at offset {} disagrees with the walk (tag {:#04x}, len {})",
                site.offset, site.tag, site.payload_len
            )));
        }
        Ok(rec.payload)
    }
}

impl SourceServer for PartialDescriptor {
    fn serve_range(&self, offset: u64, len: u64, limits: Limits) -> Result<Vec<u8>> {
        let end = offset
            .checked_add(len)
            .ok_or_else(|| Error::usage("source slice end overflows"))?;
        if end > self.source_len {
            return Err(Error::usage(format!(
                "source slice {offset}..{end} exceeds source length {}",
                self.source_len
            )));
        }
        let window = select_ops_from_lengths(&self.program, &self.per_op, offset, end)?;
        let (objects_used, channels_used) = selection_references(
            &window.ops,
            self.object_sites.len(),
            self.channel_sites.len(),
        );

        let mut objects: Vec<Vec<u8>> = vec![Vec::new(); self.object_sites.len()];
        for (id, used) in objects_used.iter().enumerate() {
            if *used {
                objects[id] = self.read_site(&self.object_sites[id], limits)?;
            }
        }

        let mut channels: Vec<EntropyChannelDescriptor> =
            vec![placeholder_channel(); self.channel_sites.len()];
        let mut models_needed = vec![false; self.model_sites.len()];
        for (id, used) in channels_used.iter().enumerate() {
            if *used {
                let payload = self.read_site(&self.channel_sites[id], limits)?;
                let channel = EntropyChannelDescriptor::decode(&payload, limits)?;
                if channel.model_id as usize >= self.model_sites.len() {
                    return Err(Error::invalid_model(format!(
                        "entropy channel {id} references missing model {}",
                        channel.model_id
                    )));
                }
                models_needed[channel.model_id as usize] = true;
                channels[id] = channel;
            }
        }
        let mut models: Vec<EntropyModel> = vec![placeholder_model(); self.model_sites.len()];
        for (id, used) in models_needed.iter().enumerate() {
            if *used {
                let payload = self.read_site(&self.model_sites[id], limits)?;
                models[id] = EntropyModel::decode(&payload)?;
            }
        }

        let served = serve_selection(
            &objects,
            &channels,
            &models,
            window,
            &objects_used,
            &channels_used,
            offset,
            end,
            limits,
        )?;
        Ok(served.bytes)
    }

    fn serve_document(&self, _limits: Limits) -> Result<Vec<u8>> {
        // A partial loader deliberately cannot reconstruct the whole source
        // without reading every record; whole-document observations use the full
        // path (which also verifies the source SHA-256).
        Err(Error::unsupported_feature(
            "partial descriptor cannot serve the whole document",
        ))
    }
}

/// Cross-check the observation-index op table against the program's own op
/// shapes, and return the per-op lengths it claims.
///
/// This re-checks the entries the loader can derive without channel payloads
/// (op count, literal/object/replay lengths, the labelled dependency, and the
/// total), so a blob whose op table disagrees with its CRC-checked program is
/// rejected rather than trusted.
fn validate_op_table(
    program: &Program,
    index: &ObservationIndex,
    object_lens: &[u64],
    source_len: u64,
) -> Result<Vec<u64>> {
    use crate::dra::op::{DEFLATE_SOURCE_CHANNEL, Op};

    let mut per_op: Vec<u64> = Vec::with_capacity(index.ops.len());
    let mut total: u64 = 0;
    for (i, entry) in index.ops.iter().enumerate() {
        let op = &program.ops[i];
        let claimed = u64::from(entry.out_len);
        let (dep_kind, dep_id) = primary_dependency(op);
        if entry.dep_kind != dep_kind
            || (dep_kind != crate::container::observation::DEP_NONE && entry.dep_id != dep_id)
        {
            return Err(Error::invalid_container(format!(
                "observation index op {i} dependency disagrees with the program"
            )));
        }
        let derived: Option<u64> = match op {
            Op::EmitObject { object_id } => {
                Some(*object_lens.get(*object_id as usize).ok_or_else(|| {
                    Error::invalid_graph(format!("graph references missing object {object_id}"))
                })?)
            }
            Op::Inline { bytes } => Some(bytes.len() as u64),
            Op::MarkOffset { .. } => Some(0),
            Op::EmitOffset { width, .. } => Some(u64::from(*width)),
            Op::DeflateReplay {
                source_kind,
                declared_output_len,
                ..
            } => {
                if *source_kind == DEFLATE_SOURCE_CHANNEL {
                    None
                } else {
                    Some(u64::from(*declared_output_len))
                }
            }
            _ => None,
        };
        if let Some(expected) = derived
            && expected != claimed
        {
            return Err(Error::invalid_container(format!(
                "observation index op {i} out_len {claimed} disagrees with the program's {expected}"
            )));
        }
        total = total
            .checked_add(claimed)
            .ok_or_else(|| Error::resource_limit("observation op length overflow"))?;
        per_op.push(claimed);
    }
    if total != source_len {
        return Err(Error::invalid_container(format!(
            "observation index op table totals {total} bytes but the source declares {source_len}"
        )));
    }
    Ok(per_op)
}

/// A never-dereferenced placeholder for an unread channel slot.
fn placeholder_channel() -> EntropyChannelDescriptor {
    use crate::entropy::{CODER_ORDER0_BYTE_RANS, CODER_VERSION_1};
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

/// A never-dereferenced placeholder for an unread model slot.
fn placeholder_model() -> EntropyModel {
    EntropyModel {
        scale_bits: 0,
        frequencies: Vec::new(),
    }
}
