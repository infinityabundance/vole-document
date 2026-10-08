//! High-level `.voldoc` descriptor: serialize and parse.
//!
//! A descriptor binds a source-format class, a universe declaration, raw byte
//! objects, a reconstruction program, and a whole-source integrity manifest
//! into one framed container. Parsing is structural and bounded; it does not
//! materialize bytes. Materialization is a separate, explicit step.

use crate::EXACTNESS_PROFILE_EXACT_BYTES;
use crate::accounting::CostBreakdown;
use crate::container::checkpoint::CheckpointTable;
use crate::container::directory::{
    DirectoryEntry, RecordSite, SEEK_DIRECTORY_ALL_SECTIONS, SeekDirectory,
};
use crate::container::header::{HEADER_LEN, Header, MAGIC};
use crate::container::observation::{
    ObservationIndex, OpEntry, SECTION_OP_TABLE, primary_dependency,
};
use crate::container::record::{FLAG_OPTIONAL, RECORD_OVERHEAD, RecordReader, RecordTag};
use crate::dra::Program;
use crate::entropy::codec::EntropyChannelDescriptor;
use crate::entropy::model::EntropyModel;
use crate::error::{Error, Result};
use crate::integrity::sha256;
use crate::limits::Limits;
use crate::store::Id;

/// Payload length of an `EXTERNAL_REF` record: `[u8; 32 id][u64 LE len]`.
pub const EXTERNAL_REF_PAYLOAD_LEN: usize = 40;

/// The current reconstruction universe declaration.
///
/// Changing any opcode, coder, limit semantic, or adapter meaning requires a
/// new universe string. The `universe_id` in the header is the first 16 bytes
/// of SHA-256 over this string.
///
/// Phase 7 adds an optional `OBSERVATION_INDEX` record; Phase 8 adds an optional
/// `DIRECTORY` record; Phase 9 adds the store-backed object form (`EXTERNAL_REF`
/// records, mandatory [`crate::container::header::FEATURE_EXTERNAL_OBJECTS`]) and
/// appends `+external-objects-v1`. The DRA graph stays at `dra-8`, the
/// `FORMAT_MINOR` does not move, and the exactness semantics are unchanged: a
/// decoder that ignores the optional records still fully materializes, while one
/// without the `store` feature fails closed on a store-backed descriptor.
pub const UNIVERSE: &str = "vole-document;universe;phase9;exact-bytes;dra-8;opaque+entropy+pdf+channels+offsets+packed+packed-channels+deflate-replay-preflate-0.7.6-experimental+observation-index-v1+seek-directory-v1+external-objects-v1";

/// First 16 bytes of SHA-256 over a universe declaration string.
pub fn universe_id_from_str(universe: &str) -> [u8; 16] {
    let full = sha256(universe.as_bytes());
    let mut id = [0u8; 16];
    id.copy_from_slice(&full[0..16]);
    id
}

/// Source of one object-table entry.
///
/// A descriptor's object table is a single ordered sequence; each entry is
/// either inline bytes or a reference to a content-addressed store object, in
/// object-table order, so the DRA's `object_id` continues to index it unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectSource {
    /// Bytes carried in an `OBJECT` (0x10) record.
    Inline(Vec<u8>),
    /// Bytes held by an [`crate::store::ObjectStore`] under `id`; `len` is the
    /// exact byte length.
    External { id: Id, len: u64 },
}

impl ObjectSource {
    /// Length available to the coverage certificate WITHOUT resolving.
    pub fn len(&self) -> u64 {
        match self {
            ObjectSource::Inline(b) => b.len() as u64,
            ObjectSource::External { len, .. } => *len,
        }
    }

    /// Whether this entry contributes zero bytes.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The inline bytes, if this entry is not an external reference.
    pub fn as_inline(&self) -> Option<&[u8]> {
        match self {
            ObjectSource::Inline(b) => Some(b),
            ObjectSource::External { .. } => None,
        }
    }
}

/// The in-memory model of a `.voldoc` descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Descriptor {
    /// Universe declaration string.
    pub universe: String,
    /// Source-format class selector.
    pub source_format: u8,
    /// Human-readable basis for the format decision (provenance, not trust).
    pub format_basis: String,
    /// Canonical entropy models referenced by channels.
    pub models: Vec<EntropyModel>,
    /// Typed entropy channels referenced by the program.
    pub channels: Vec<EntropyChannelDescriptor>,
    /// Raw byte objects referenced by the program.
    pub objects: Vec<ObjectSource>,
    /// The reconstruction program.
    pub program: Program,
    /// Optional advisory observation index (Phase 7.3).
    ///
    /// `None` is today's descriptor and fully materializes. When `Some`, the
    /// record is validated against the program at parse time; it is never
    /// authority.
    pub observation_index: Option<ObservationIndex>,
    /// Whether to emit an optional seek `DIRECTORY` record (Phase 8).
    ///
    /// `false` is today's descriptor and produces the exact Phase-7 record
    /// sequence (the universe string still carries the Phase-8 suffix). When
    /// `true`, `serialize` writes a two-pass `DIRECTORY` record as the first
    /// record; a directory requires an observation index to describe.
    pub seek_directory: bool,
    /// Optional advisory byte-level checkpoint table (Phase 13.4).
    ///
    /// `None` is today's descriptor and emits no `CHECKPOINT` record. When
    /// `Some`, `serialize` writes one `FLAG_OPTIONAL` `CHECKPOINT` record (which a
    /// decoder that ignores it skips, still fully materializing) and the seek
    /// `DIRECTORY` indexes it. A checkpoint is **advisory, never authority**: it
    /// requires a seek directory to be locatable, and `parse` re-derives every
    /// boundary from the program and rejects any contradiction.
    pub checkpoints: Option<CheckpointTable>,
    /// SHA-256 of the exact reconstructed source.
    pub source_sha256: [u8; 32],
    /// Exact reconstructed source length.
    pub source_len: u64,
}

/// A parsed descriptor plus its physical cost breakdown.
#[derive(Debug, Clone)]
pub struct ParsedDescriptor {
    /// The descriptor model.
    pub descriptor: Descriptor,
    /// Physical byte attribution of the serialized form.
    pub cost: CostBreakdown,
    /// Encode the universe identifier that was validated against the header.
    pub universe_id: [u8; 16],
}

/// A record payload staged during the first pass of [`Descriptor::serialize`].
struct PendingRecord {
    tag: RecordTag,
    flags: u8,
    payload: Vec<u8>,
}

impl PendingRecord {
    fn new(tag: RecordTag, flags: u8, payload: Vec<u8>) -> Self {
        PendingRecord {
            tag,
            flags,
            payload,
        }
    }
}

impl Descriptor {
    /// The header that this descriptor serializes to.
    pub fn header(&self) -> Header {
        let mut header = Header::new(
            universe_id_from_str(&self.universe),
            self.source_len,
            EXACTNESS_PROFILE_EXACT_BYTES,
            self.source_format,
        );
        header.mandatory_features = self.required_features();
        header.optional_features = self.optional_features();
        header
    }

    /// Mandatory feature bits implied by this descriptor's contents.
    ///
    /// Derived rather than stored, so no constructor can forget to declare a
    /// feature: a descriptor carrying a `DEFLATE_REPLAY` op always sets
    /// [`crate::container::header::FEATURE_DEFLATE_REPLAY`] in its header, and a
    /// build without that feature rejects it at header validation.
    pub fn required_features(&self) -> u32 {
        let mut bits = 0u32;
        for op in &self.program.ops {
            if matches!(op, crate::dra::Op::DeflateReplay { .. }) {
                bits |= crate::container::header::FEATURE_DEFLATE_REPLAY;
            }
        }
        if self
            .objects
            .iter()
            .any(|o| matches!(o, ObjectSource::External { .. }))
        {
            bits |= crate::container::header::FEATURE_EXTERNAL_OBJECTS;
        }
        bits
    }

    /// Optional feature bits implied by this descriptor's contents.
    ///
    /// Optional bits are ignorable: a decoder that does not understand them
    /// still materializes the source exactly. The observation-index bit records
    /// only that a partial-decode lane is available, and the seek-directory bit
    /// only that a seek-based lane is available; exactness never requires either.
    pub fn optional_features(&self) -> u32 {
        let mut bits = 0u32;
        if self.observation_index.is_some() {
            bits |= crate::container::header::FEATURE_OBSERVATION_INDEX;
        }
        if self.seek_directory {
            bits |= crate::container::header::FEATURE_SEEK_DIRECTORY;
        }
        if self.checkpoints.is_some() {
            bits |= crate::container::header::FEATURE_CHECKPOINTS;
        }
        bits
    }

    /// Return this descriptor with a minimal advisory observation-index op table
    /// attached, if it lacks one and the table is derivable from the (unchanged)
    /// reconstruction program.
    ///
    /// This is the pure, container-level form of the enrichment an ingest performs
    /// on a stored authority blob: the op table is exactly
    /// [`Program::analyze_ops`]' per-op output lengths plus each op's
    /// [`primary_dependency`], so it changes **no** reconstruction semantics — only
    /// the ignorable `OBSERVATION_INDEX` record and the optional feature bit that
    /// advertises it. It returns `self` unchanged when an index is already present,
    /// when [`Program::analyze_ops`] declines (a limit breach), or when an op's
    /// output length does not fit the index's `u32` field, exactly as the
    /// byte-level fallback does. Because it never parses or serializes, a caller
    /// that holds a [`Descriptor`] can produce the enriched, byte-identical
    /// authority with a **single** serialize pass.
    pub fn with_observation_index(mut self, limits: Limits) -> Self {
        if self.observation_index.is_some() {
            return self;
        }
        let object_lens: Vec<u64> = self.objects.iter().map(|o| o.len()).collect();
        let channel_lens: Vec<u64> = self.channels.iter().map(|c| c.decoded_length).collect();
        let per_op = match self
            .program
            .analyze_ops(&object_lens, &channel_lens, limits)
        {
            Ok(v) => v,
            Err(_) => return self,
        };
        let mut ops: Vec<OpEntry> = Vec::with_capacity(per_op.len());
        for (i, len) in per_op.iter().enumerate() {
            let Ok(out_len) = u32::try_from(*len) else {
                return self;
            };
            let (dep_kind, dep_id) = primary_dependency(&self.program.ops[i]);
            ops.push(OpEntry {
                out_len,
                dep_kind,
                dep_id,
            });
        }
        self.observation_index = Some(ObservationIndex {
            section_flags: SECTION_OP_TABLE,
            ops,
            selectors: Vec::new(),
            digests: Vec::new(),
        });
        self
    }

    /// Serialize to a complete `.voldoc` byte sequence plus cost attribution.
    ///
    /// When [`Descriptor::seek_directory`] is set this is a two-pass build: every
    /// record payload is encoded first, the seek directory's length is computed
    /// from counts alone (so there is no chicken-and-egg), then the records are
    /// emitted after the directory. A descriptor without a directory emits exactly
    /// the record sequence and bytes it emitted before this field existed, and
    /// `cost.directory == 0`.
    pub fn serialize(&self) -> Result<(Vec<u8>, CostBreakdown)> {
        if self.seek_directory && self.observation_index.is_none() {
            return Err(Error::invalid_container(
                "a seek directory requires an observation index",
            ));
        }
        if self.checkpoints.is_some() && !self.seek_directory {
            return Err(Error::invalid_container(
                "a checkpoint requires a seek directory to locate it",
            ));
        }

        let mut cost = CostBreakdown {
            header: HEADER_LEN as u64,
            ..Default::default()
        };

        // ----- Pass 1: encode every record payload; emit no bytes yet. -----
        let mut pending: Vec<PendingRecord> = Vec::new();

        // UNIVERSE
        pending.push(PendingRecord::new(
            RecordTag::Universe,
            0,
            self.universe.as_bytes().to_vec(),
        ));
        cost.universe = self.universe.len() as u64;

        // FORMAT: [class u8][basis_len u32 LE][basis bytes]
        let basis = self.format_basis.as_bytes();
        let basis_len = u32::try_from(basis.len())
            .map_err(|_| Error::resource_limit("format basis too long"))?;
        let mut fmt = Vec::with_capacity(5 + basis.len());
        fmt.push(self.source_format);
        fmt.extend_from_slice(&basis_len.to_le_bytes());
        fmt.extend_from_slice(basis);
        cost.format = fmt.len() as u64;
        pending.push(PendingRecord::new(RecordTag::Format, 0, fmt));

        // MODELS
        for model in &self.models {
            let encoded = model.encode()?;
            cost.models += encoded.len() as u64;
            pending.push(PendingRecord::new(RecordTag::Model, 0, encoded));
        }

        // ENTROPY CHANNELS
        for channel in &self.channels {
            let encoded = channel.encode()?;
            cost.entropy_payload += encoded.len() as u64;
            pending.push(PendingRecord::new(RecordTag::EntropyChannel, 0, encoded));
        }

        // OBJECTS: each entry is either an inline OBJECT record or an
        // EXTERNAL_REF record, in object-table order (position is the id).
        for obj in &self.objects {
            match obj {
                ObjectSource::Inline(bytes) => {
                    cost.objects += bytes.len() as u64;
                    pending.push(PendingRecord::new(RecordTag::Object, 0, bytes.clone()));
                }
                ObjectSource::External { id, len } => {
                    let mut payload = Vec::with_capacity(EXTERNAL_REF_PAYLOAD_LEN);
                    payload.extend_from_slice(id.as_bytes());
                    payload.extend_from_slice(&len.to_le_bytes());
                    cost.external_refs += payload.len() as u64;
                    pending.push(PendingRecord::new(RecordTag::ExternalRef, 0, payload));
                }
            }
        }

        // GRAPH
        let graph = self.program.encode()?;
        cost.graph = graph.len() as u64;
        pending.push(PendingRecord::new(RecordTag::Graph, 0, graph));

        // OBSERVATION_INDEX (optional, advisory). Written with the optional flag
        // so a decoder that ignores it still fully materializes.
        let mut index_records: u64 = 0;
        if let Some(index) = &self.observation_index {
            let payload = index.encode()?;
            index_records = 1;
            cost.index = payload.len() as u64 + RECORD_OVERHEAD as u64;
            pending.push(PendingRecord::new(
                RecordTag::ObservationIndex,
                FLAG_OPTIONAL,
                payload,
            ));
        }

        // CHECKPOINT (optional, advisory). Written with the optional flag so a
        // decoder that ignores it still fully materializes. The table was
        // provided by the caller (built from the program); its bytes are charged
        // to `cost.checkpoints`, not to `cost.index`.
        let mut checkpoint_records: u64 = 0;
        if let Some(checkpoint) = &self.checkpoints {
            let payload = checkpoint.encode()?;
            checkpoint_records = 1;
            cost.checkpoints = payload.len() as u64 + RECORD_OVERHEAD as u64;
            pending.push(PendingRecord::new(
                RecordTag::Checkpoint,
                FLAG_OPTIONAL,
                payload,
            ));
        }

        // INTEGRITY: [sha256 32][source_len u64 LE]
        let mut integ = Vec::with_capacity(40);
        integ.extend_from_slice(&self.source_sha256);
        integ.extend_from_slice(&self.source_len.to_le_bytes());
        cost.integrity = integ.len() as u64;
        pending.push(PendingRecord::new(RecordTag::Integrity, 0, integ));

        // ----- Seek directory (optional). Build it before emitting anything, so
        // the record offsets can account for its own (count-determined) length. -----
        let mut directory_payload: Option<Vec<u8>> = None;
        let mut directory_records: u64 = 0;
        if self.seek_directory {
            let channel_lengths: Vec<u64> =
                self.channels.iter().map(|c| c.decoded_length).collect();

            // Locators in file order: the DIRECTORY itself (offset 64), then every
            // pending record, then the TRAILER. Offsets after the directory are
            // filled once its record length is known.
            let mut entries: Vec<DirectoryEntry> = Vec::with_capacity(pending.len() + 2);
            entries.push(DirectoryEntry {
                tag: RecordTag::Directory as u8,
                offset: HEADER_LEN as u64,
                payload_len: 0,
            });
            for rec in &pending {
                let payload_len = u32::try_from(rec.payload.len())
                    .map_err(|_| Error::resource_limit("record payload exceeds u32"))?;
                entries.push(DirectoryEntry {
                    tag: rec.tag as u8,
                    offset: 0,
                    payload_len,
                });
            }
            entries.push(DirectoryEntry {
                tag: RecordTag::Trailer as u8,
                offset: 0,
                payload_len: 20,
            });
            let classes = crate::container::directory::class_index(&entries);

            // The provisional encode fixes the directory's own payload length:
            // offsets are placeholder-valued but fixed-width, so only the counts
            // determine the length. There is no circular dependency.
            let mut dir = SeekDirectory {
                section_flags: SEEK_DIRECTORY_ALL_SECTIONS,
                entries,
                classes,
                channel_lengths,
            };
            let dir_payload_len = dir.encode()?.len();
            dir.entries[0].payload_len = u32::try_from(dir_payload_len)
                .map_err(|_| Error::resource_limit("seek directory payload exceeds u32"))?;

            let mut off = (HEADER_LEN + RECORD_OVERHEAD + dir_payload_len) as u64;
            let last = dir.entries.len() - 1;
            for entry in &mut dir.entries[1..last] {
                entry.offset = off;
                off = off
                    .checked_add(RECORD_OVERHEAD as u64)
                    .and_then(|v| v.checked_add(u64::from(entry.payload_len)))
                    .ok_or_else(|| Error::invalid_container("seek directory offset overflow"))?;
            }
            dir.entries[last].offset = off;

            let payload = dir.encode()?;
            debug_assert_eq!(payload.len(), dir_payload_len);
            cost.directory = dir_payload_len as u64 + RECORD_OVERHEAD as u64;
            directory_records = 1;
            directory_payload = Some(payload);
        }

        // ----- Pass 2: emit. -----
        let mut out = Vec::new();
        out.extend_from_slice(&self.header().encode());

        if let Some(payload) = &directory_payload {
            crate::container::record::write_record(
                &mut out,
                RecordTag::Directory as u8,
                FLAG_OPTIONAL,
                payload,
            )?;
        }
        for rec in &pending {
            crate::container::record::write_record(
                &mut out,
                rec.tag as u8,
                rec.flags,
                &rec.payload,
            )?;
        }

        // TRAILER: [record_count u32][payload_bytes u64][MAGIC 8]
        // record_count includes the trailer itself and any directory record.
        let total_records = pending.len() as u64 + directory_records + 1;
        let total_records =
            u32::try_from(total_records).map_err(|_| Error::resource_limit("too many records"))?;
        let payload_bytes = (out.len() - HEADER_LEN) as u64;
        let mut trailer = Vec::with_capacity(20);
        trailer.extend_from_slice(&total_records.to_le_bytes());
        trailer.extend_from_slice(&payload_bytes.to_le_bytes());
        trailer.extend_from_slice(&MAGIC);
        crate::container::record::write_record(&mut out, RecordTag::Trailer as u8, 0, &trailer)?;
        cost.trailer = trailer.len() as u64;

        // Framing overhead for every record after the fixed header. The optional
        // index and directory records' framing is charged to `cost.index` and
        // `cost.directory` instead, so subtract their counts here to keep
        // `total()` exactly the serialized length (every category stays a real
        // byte).
        cost.record_framing = RECORD_OVERHEAD as u64
            * (u64::from(total_records) - index_records - directory_records - checkpoint_records);

        debug_assert_eq!(cost.total(), out.len() as u64);
        Ok((out, cost))
    }

    /// Parse a complete `.voldoc` byte sequence with structural validation.
    ///
    /// This validates framing, universe identity, version, mandatory features,
    /// record presence, the coverage certificate, and declared length. It does
    /// **not** materialize or hash the reconstructed source; call
    /// [`crate::materialize::materialize`] for that.
    pub fn parse(bytes: &[u8], limits: Limits) -> Result<ParsedDescriptor> {
        if bytes.len() as u64 > limits.max_input_bytes {
            return Err(Error::resource_limit(
                "input exceeds configured input limit",
            ));
        }
        let header = Header::decode(bytes)?;
        if !header.source_format_supported() {
            return Err(Error::unsupported_feature(format!(
                "source format class {} has no adapter in this build",
                header.source_format
            )));
        }

        let mut cost = CostBreakdown {
            header: HEADER_LEN as u64,
            ..Default::default()
        };

        let mut reader = RecordReader::new(bytes, HEADER_LEN, limits);
        let mut universe: Option<String> = None;
        let mut format: Option<(u8, String)> = None;
        let mut models: Vec<EntropyModel> = Vec::new();
        let mut channels: Vec<EntropyChannelDescriptor> = Vec::new();
        let mut objects: Vec<ObjectSource> = Vec::new();
        let mut program: Option<Program> = None;
        let mut observation_index: Option<ObservationIndex> = None;
        let mut source_sha256: Option<[u8; 32]> = None;
        let mut source_len: Option<u64> = None;
        let mut saw_trailer = false;
        let mut trailer_record_count: Option<u32> = None;
        let mut records_seen: u32 = 0;
        let mut index_records: u64 = 0;
        let mut directory_records: u64 = 0;
        let mut seek_directory: Option<SeekDirectory> = None;
        let mut checkpoint_records: u64 = 0;
        let mut checkpoints: Option<CheckpointTable> = None;
        let mut graph_payload: Vec<u8> = Vec::new();
        let mut sites: Vec<RecordSite> = Vec::new();

        while let Some(rec) = reader.next_record()? {
            records_seen += 1;
            let site = RecordSite {
                tag: rec.tag,
                offset: reader.position() as u64
                    - (RECORD_OVERHEAD as u64 + rec.payload.len() as u64),
                payload_len: u32::try_from(rec.payload.len())
                    .map_err(|_| Error::resource_limit("record payload exceeds u32"))?,
            };
            sites.push(site);
            if saw_trailer {
                return Err(Error::invalid_container("record found after TRAILER"));
            }
            match RecordTag::from_u8(rec.tag) {
                Some(RecordTag::Universe) => {
                    if universe.is_some() {
                        return Err(Error::invalid_container("duplicate UNIVERSE record"));
                    }
                    let payload_len = rec.payload.len();
                    let s = String::from_utf8(rec.payload)
                        .map_err(|_| Error::invalid_container("universe is not valid UTF-8"))?;
                    if universe_id_from_str(&s) != header.universe_id {
                        return Err(Error::invalid_container(
                            "universe declaration does not match its header identifier",
                        ));
                    }
                    universe = Some(s);
                    cost.universe = payload_len as u64;
                }
                Some(RecordTag::Format) => {
                    if format.is_some() {
                        return Err(Error::invalid_container("duplicate FORMAT record"));
                    }
                    if rec.payload.len() < 5 {
                        return Err(Error::invalid_container("truncated FORMAT payload"));
                    }
                    let class = rec.payload[0];
                    let blen = u32::from_le_bytes([
                        rec.payload[1],
                        rec.payload[2],
                        rec.payload[3],
                        rec.payload[4],
                    ]);
                    let blen = blen as usize;
                    if rec.payload.len() != 5 + blen {
                        return Err(Error::invalid_container("FORMAT payload length mismatch"));
                    }
                    let basis = String::from_utf8(rec.payload[5..].to_vec())
                        .map_err(|_| Error::invalid_container("format basis is not UTF-8"))?;
                    if class != header.source_format {
                        return Err(Error::invalid_container(
                            "FORMAT class disagrees with header source_format",
                        ));
                    }
                    format = Some((class, basis));
                    cost.format = rec.payload.len() as u64;
                }
                Some(RecordTag::Object) => {
                    if objects.len() as u32 >= limits.max_object_count {
                        return Err(Error::resource_limit("object count limit exceeded"));
                    }
                    cost.objects += rec.payload.len() as u64;
                    objects.push(ObjectSource::Inline(rec.payload));
                }
                Some(RecordTag::ExternalRef) => {
                    if objects.len() as u32 >= limits.max_object_count {
                        return Err(Error::resource_limit("object count limit exceeded"));
                    }
                    if rec.payload.len() != EXTERNAL_REF_PAYLOAD_LEN {
                        return Err(Error::invalid_container(
                            "EXTERNAL_REF payload must be 40 bytes",
                        ));
                    }
                    let mut id = [0u8; 32];
                    id.copy_from_slice(&rec.payload[0..32]);
                    let len = u64::from_le_bytes([
                        rec.payload[32],
                        rec.payload[33],
                        rec.payload[34],
                        rec.payload[35],
                        rec.payload[36],
                        rec.payload[37],
                        rec.payload[38],
                        rec.payload[39],
                    ]);
                    cost.external_refs += rec.payload.len() as u64;
                    objects.push(ObjectSource::External {
                        id: Id::from_bytes(id),
                        len,
                    });
                }
                Some(RecordTag::Model) => {
                    if models.len() as u32 >= limits.max_model_count {
                        return Err(Error::resource_limit("entropy model count limit exceeded"));
                    }
                    if rec.payload.len() as u32 > limits.max_entropy_model_bytes {
                        return Err(Error::resource_limit(format!(
                            "entropy model payload {} exceeds limit {}",
                            rec.payload.len(),
                            limits.max_entropy_model_bytes
                        )));
                    }
                    let model = EntropyModel::decode(&rec.payload)?;
                    cost.models += rec.payload.len() as u64;
                    models.push(model);
                }
                Some(RecordTag::EntropyChannel) => {
                    if channels.len() as u32 >= limits.max_channel_count {
                        return Err(Error::resource_limit(
                            "entropy channel count limit exceeded",
                        ));
                    }
                    let channel = EntropyChannelDescriptor::decode(&rec.payload, limits)?;
                    cost.entropy_payload += rec.payload.len() as u64;
                    channels.push(channel);
                }
                Some(RecordTag::Graph) => {
                    if program.is_some() {
                        return Err(Error::invalid_container("duplicate GRAPH record"));
                    }
                    let p = Program::decode(&rec.payload, limits)?;
                    cost.graph = rec.payload.len() as u64;
                    graph_payload = rec.payload.clone();
                    program = Some(p);
                }
                Some(RecordTag::ObservationIndex) => {
                    if observation_index.is_some() {
                        return Err(Error::invalid_container(
                            "duplicate OBSERVATION_INDEX record",
                        ));
                    }
                    let idx = ObservationIndex::decode(&rec.payload, limits)?;
                    cost.index = rec.payload.len() as u64 + RECORD_OVERHEAD as u64;
                    index_records = 1;
                    observation_index = Some(idx);
                }
                Some(RecordTag::Directory) => {
                    if seek_directory.is_some() {
                        return Err(Error::invalid_container("duplicate DIRECTORY record"));
                    }
                    if !rec.is_optional() {
                        return Err(Error::invalid_container(
                            "DIRECTORY record must carry FLAG_OPTIONAL",
                        ));
                    }
                    if sites.len() != 1 {
                        return Err(Error::invalid_container(
                            "DIRECTORY record must be the first record",
                        ));
                    }
                    let dir = SeekDirectory::decode(&rec.payload, limits)?;
                    cost.directory = rec.payload.len() as u64 + RECORD_OVERHEAD as u64;
                    directory_records = 1;
                    seek_directory = Some(dir);
                }
                Some(RecordTag::Integrity) => {
                    if source_sha256.is_some() {
                        return Err(Error::invalid_container("duplicate INTEGRITY record"));
                    }
                    if rec.payload.len() != 40 {
                        return Err(Error::invalid_container(
                            "INTEGRITY payload must be 40 bytes",
                        ));
                    }
                    let mut sha = [0u8; 32];
                    sha.copy_from_slice(&rec.payload[0..32]);
                    let len = u64::from_le_bytes([
                        rec.payload[32],
                        rec.payload[33],
                        rec.payload[34],
                        rec.payload[35],
                        rec.payload[36],
                        rec.payload[37],
                        rec.payload[38],
                        rec.payload[39],
                    ]);
                    source_sha256 = Some(sha);
                    source_len = Some(len);
                    cost.integrity = rec.payload.len() as u64;
                }
                Some(RecordTag::Trailer) => {
                    if rec.payload.len() != 20 {
                        return Err(Error::invalid_container("TRAILER payload must be 20 bytes"));
                    }
                    if rec.payload[12..20] != MAGIC {
                        return Err(Error::invalid_container("TRAILER magic mismatch"));
                    }
                    trailer_record_count = Some(u32::from_le_bytes([
                        rec.payload[0],
                        rec.payload[1],
                        rec.payload[2],
                        rec.payload[3],
                    ]));
                    cost.trailer = rec.payload.len() as u64;
                    saw_trailer = true;
                }
                Some(RecordTag::Checkpoint) => {
                    if checkpoints.is_some() {
                        return Err(Error::invalid_container("duplicate CHECKPOINT record"));
                    }
                    if !rec.is_optional() {
                        return Err(Error::invalid_container(
                            "CHECKPOINT record must carry FLAG_OPTIONAL",
                        ));
                    }
                    let cp = CheckpointTable::decode(&rec.payload, limits)?;
                    cost.checkpoints = rec.payload.len() as u64 + RECORD_OVERHEAD as u64;
                    checkpoint_records = 1;
                    checkpoints = Some(cp);
                }
                // Phase 2+ mandatory records have no meaning in this universe.
                Some(RecordTag::Residual) => {
                    if rec.is_optional() {
                        // Explicitly optional and unknown to this universe: skip.
                    } else {
                        return Err(Error::unsupported_feature(format!(
                            "record class {} requires a universe this build does not implement",
                            rec.tag
                        )));
                    }
                }
                None => {
                    if rec.is_optional() {
                        // Forward-compatible optional record: skip.
                    } else {
                        return Err(Error::unsupported_feature(format!(
                            "unknown mandatory record tag {:#04x}",
                            rec.tag
                        )));
                    }
                }
            }
        }

        let universe =
            universe.ok_or_else(|| Error::invalid_container("missing UNIVERSE record"))?;
        let (class, basis) =
            format.ok_or_else(|| Error::invalid_container("missing FORMAT record"))?;
        let program = program.ok_or_else(|| Error::invalid_container("missing GRAPH record"))?;
        let source_sha256 =
            source_sha256.ok_or_else(|| Error::invalid_container("missing INTEGRITY record"))?;
        let source_len =
            source_len.ok_or_else(|| Error::invalid_container("missing INTEGRITY record"))?;
        if !saw_trailer {
            return Err(Error::invalid_container("missing TRAILER record"));
        }
        if let Some(n) = trailer_record_count
            && n != records_seen
        {
            return Err(Error::invalid_container(format!(
                "TRAILER declares {n} records but {records_seen} were read"
            )));
        }
        if source_len != header.declared_source_len {
            return Err(Error::integrity_mismatch(format!(
                "INTEGRITY length {source_len} disagrees with header {}",
                header.declared_source_len
            )));
        }

        // Cross-validate every channel against the model it references. A
        // channel may not name a missing model, and its declared scale must
        // agree with that model.
        for (i, channel) in channels.iter().enumerate() {
            let model = models.get(channel.model_id as usize).ok_or_else(|| {
                Error::invalid_model(format!(
                    "entropy channel {i} references missing model {}",
                    channel.model_id
                ))
            })?;
            if channel.scale_bits != model.scale_bits {
                return Err(Error::invalid_model(format!(
                    "entropy channel {i} scale_bits {} disagrees with model {} scale_bits {}",
                    channel.scale_bits, channel.model_id, model.scale_bits
                )));
            }
        }

        // Coverage certificate: every source byte has exactly one authority and
        // the program's predicted length equals the declared length.
        let object_lens: Vec<u64> = objects.iter().map(|o| o.len()).collect();
        let channel_lens: Vec<u64> = channels.iter().map(|c| c.decoded_length).collect();
        let (predicted, coverage) = program.analyze(&object_lens, &channel_lens, limits)?;
        if predicted != source_len {
            return Err(Error::coverage_violation(format!(
                "reconstruction program predicts {predicted} bytes but {source_len} were declared"
            )));
        }
        coverage.validate(source_len)?;

        // The observation index is advisory: re-derive every claim from the
        // program and reject any contradiction. It is never authority.
        if let Some(index) = &observation_index {
            index.validate(&program, &object_lens, &channel_lens, limits)?;
        }

        // The seek directory is advisory too: it must be consistent with the
        // actual record framing, but the framing and the program remain the
        // authority. A malformed or inconsistent directory is rejected rather
        // than trusted.
        if let Some(dir) = &seek_directory {
            dir.validate(&sites, bytes.len() as u64, limits)?;
        }

        // The checkpoint is advisory too: its boundaries must be re-derivable
        // from the authoritative program (and bound to the GRAPH record), or it
        // is rejected. It is never authority.
        if let Some(cp) = &checkpoints {
            cp.validate(
                &program,
                &graph_payload,
                source_len,
                &object_lens,
                &channel_lens,
                limits,
            )?;
        }

        // As in `serialize`, the optional index and directory records' framing is
        // charged to `cost.index`/`cost.directory`, so exclude their counts from
        // the framing total.
        cost.record_framing = RECORD_OVERHEAD as u64
            * (records_seen as u64 - index_records - directory_records - checkpoint_records);

        Ok(ParsedDescriptor {
            descriptor: Descriptor {
                universe,
                source_format: class,
                format_basis: basis,
                models,
                channels,
                objects,
                program,
                observation_index,
                seek_directory: seek_directory.is_some(),
                checkpoints,
                source_sha256,
                source_len,
            },
            cost,
            universe_id: header.universe_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SOURCE_FORMAT_OPAQUE;
    use crate::dra::Op;
    use crate::integrity::sha256;

    fn sample(source: &[u8]) -> Descriptor {
        Descriptor {
            universe: UNIVERSE.to_string(),
            source_format: SOURCE_FORMAT_OPAQUE,
            format_basis: "opaque:test".to_string(),
            models: vec![],
            channels: vec![],
            objects: vec![ObjectSource::Inline(source.to_vec())],
            program: Program::new(vec![Op::EmitObject { object_id: 0 }]),
            observation_index: None,
            seek_directory: false,
            checkpoints: None,
            source_sha256: sha256(source),
            source_len: source.len() as u64,
        }
    }

    fn channel(model_id: u32, scale_bits: u8, decoded_length: u64) -> EntropyChannelDescriptor {
        EntropyChannelDescriptor {
            coder: crate::entropy::codec::CODER_ORDER0_BYTE_RANS,
            coder_version: crate::entropy::codec::CODER_VERSION_1,
            scale_bits,
            lane_count: 1,
            model_id,
            symbol_count: decoded_length,
            decoded_length,
            initial_state: 1,
            payload: vec![0u8; 4],
        }
    }

    #[test]
    fn model_and_channel_roundtrip() {
        let payload = b"channel bytes";
        let mut d = sample(payload);
        d.models = vec![EntropyModel::uniform(8).unwrap()];
        d.channels = vec![channel(0, 8, payload.len() as u64)];
        d.program = Program::new(vec![Op::DecodeChannel { channel_id: 0 }]);
        let (bytes, cost) = d.serialize().unwrap();
        assert_eq!(cost.total(), bytes.len() as u64);
        assert!(cost.models > 0);
        assert!(cost.entropy_payload > 0);
        let parsed = Descriptor::parse(&bytes, Limits::DEFAULT).unwrap();
        assert_eq!(parsed.descriptor, d);
        assert_eq!(parsed.cost.total(), bytes.len() as u64);
    }

    #[test]
    fn channel_with_missing_model_rejected() {
        let mut d = sample(b"abc");
        d.program = Program::new(vec![Op::DecodeChannel { channel_id: 0 }]);
        d.channels = vec![channel(3, 8, 3)];
        let (bytes, _) = d.serialize().unwrap();
        let e = Descriptor::parse(&bytes, Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::InvalidModel);
    }

    #[test]
    fn channel_scale_mismatch_rejected() {
        let mut d = sample(b"abc");
        d.models = vec![EntropyModel::uniform(8).unwrap()];
        d.program = Program::new(vec![Op::DecodeChannel { channel_id: 0 }]);
        d.channels = vec![channel(0, 12, 3)];
        let (bytes, _) = d.serialize().unwrap();
        let e = Descriptor::parse(&bytes, Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::InvalidModel);
    }

    #[test]
    fn serialize_parse_roundtrip() {
        let d = sample(b"hello, exact world");
        let (bytes, cost) = d.serialize().unwrap();
        assert_eq!(cost.total(), bytes.len() as u64);
        let parsed = Descriptor::parse(&bytes, Limits::DEFAULT).unwrap();
        assert_eq!(parsed.descriptor, d);
        assert_eq!(parsed.cost.total(), bytes.len() as u64);
    }

    #[test]
    fn trailing_bytes_after_trailer_rejected() {
        let d = sample(b"abc");
        let (mut bytes, _) = d.serialize().unwrap();
        bytes.push(0);
        let e = Descriptor::parse(&bytes, Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::InvalidContainer);
    }

    #[test]
    fn declared_length_mismatch_rejected() {
        // Build a descriptor whose declared length disagrees with the program.
        let mut d = sample(b"abcdef");
        d.source_len = 5;
        let (bytes, _) = d.serialize().unwrap();
        let e = Descriptor::parse(&bytes, Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::CoverageViolation);
    }

    fn replay_program() -> Program {
        Program::new(vec![Op::DeflateReplay {
            replay_codec: crate::dra::op::REPLAY_DEFLATE_PREFLATE_0_7_6,
            source_kind: crate::dra::op::DEFLATE_SOURCE_OBJECT,
            source_id: 0,
            corrections_object: 0,
            declared_output_len: 3,
        }])
    }

    #[test]
    fn plain_descriptor_declares_no_mandatory_features() {
        assert_eq!(sample(b"abc").required_features(), 0);
    }

    #[cfg(feature = "deflate-replay")]
    #[test]
    fn replay_op_declares_mandatory_feature() {
        let mut d = sample(b"abc");
        d.program = replay_program();
        assert_eq!(
            d.required_features(),
            crate::container::header::FEATURE_DEFLATE_REPLAY
        );
        // The declared bit survives a serialize/parse cycle.
        let (bytes, _) = d.serialize().unwrap();
        let parsed = Descriptor::parse(&bytes, Limits::DEFAULT).unwrap();
        assert_eq!(
            parsed.descriptor.required_features(),
            crate::container::header::FEATURE_DEFLATE_REPLAY
        );
    }

    #[cfg(not(feature = "deflate-replay"))]
    #[test]
    fn replay_descriptor_fails_closed_without_feature() {
        let mut d = sample(b"abc");
        d.program = replay_program();
        let (bytes, _) = d.serialize().unwrap();
        let e = Descriptor::parse(&bytes, Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::UnsupportedFeature);
    }

    use crate::container::observation::{
        DEP_NONE, DEP_OBJECT, ObservationDigest, ObservationSelector, OpEntry, SECTION_DIGESTS,
        SECTION_OP_TABLE, SECTION_PDF_SELECTORS, SELECTOR_OBJECT,
    };

    /// A two-op descriptor (`abc` literal + `de` inline) carrying a fully
    /// consistent observation index over its five output bytes.
    fn indexed_descriptor() -> Descriptor {
        let mut d = sample(b"");
        d.objects = vec![ObjectSource::Inline(b"abc".to_vec())];
        d.program = Program::new(vec![
            Op::EmitObject { object_id: 0 },
            Op::Inline {
                bytes: b"de".to_vec(),
            },
        ]);
        d.source_sha256 = sha256(b"abcde");
        d.source_len = 5;
        d.observation_index = Some(ObservationIndex {
            section_flags: SECTION_OP_TABLE | SECTION_PDF_SELECTORS | SECTION_DIGESTS,
            ops: vec![
                OpEntry {
                    out_len: 3,
                    dep_kind: DEP_OBJECT,
                    dep_id: 0,
                },
                OpEntry {
                    out_len: 2,
                    dep_kind: DEP_NONE,
                    dep_id: 0,
                },
            ],
            selectors: vec![ObservationSelector {
                kind: SELECTOR_OBJECT,
                number: 1,
                generation: 0,
                out_off: 0,
                out_len: 3,
            }],
            digests: vec![ObservationDigest {
                out_off: 3,
                out_len: 2,
                sha256: [7u8; 32],
            }],
        });
        d
    }

    #[test]
    fn observation_index_roundtrip_and_charge() {
        let d = indexed_descriptor();
        assert_eq!(
            d.optional_features(),
            crate::container::header::FEATURE_OBSERVATION_INDEX
        );
        let (bytes, cost) = d.serialize().unwrap();
        assert_eq!(
            cost.total(),
            bytes.len() as u64,
            "cost must be the byte length"
        );
        assert!(
            cost.index > 0,
            "the index payload + framing must be charged"
        );

        let parsed = Descriptor::parse(&bytes, Limits::DEFAULT).unwrap();
        assert_eq!(parsed.descriptor, d);
        assert_eq!(parsed.cost.total(), bytes.len() as u64);
        assert!(parsed.cost.index > 0);

        // Absent: no record, no charge, and the descriptor still round-trips.
        let plain = sample(b"nope");
        let (pbytes, pcost) = plain.serialize().unwrap();
        assert_eq!(pcost.total(), pbytes.len() as u64);
        assert_eq!(pcost.index, 0);
        assert_eq!(plain.optional_features(), 0);
        let reparsed = Descriptor::parse(&pbytes, Limits::DEFAULT).unwrap();
        assert!(reparsed.descriptor.observation_index.is_none());
    }

    #[test]
    fn inconsistent_observation_index_is_rejected_on_parse() {
        // A CRC-valid but self-contradictory index must be rejected, not trusted.
        let mut d = indexed_descriptor();
        d.observation_index.as_mut().unwrap().ops[0].out_len = 9;
        let (bytes, _) = d.serialize().unwrap();
        assert_eq!(
            Descriptor::parse(&bytes, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::CoverageViolation
        );

        let mut d = indexed_descriptor();
        let idx = d.observation_index.as_mut().unwrap();
        idx.ops[0].dep_kind = DEP_OBJECT;
        idx.ops[0].dep_id = 99;
        let (bytes, _) = d.serialize().unwrap();
        assert_eq!(
            Descriptor::parse(&bytes, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::CoverageViolation
        );

        let mut d = indexed_descriptor();
        let sel = &mut d.observation_index.as_mut().unwrap().selectors[0];
        sel.out_off = 4;
        sel.out_len = 9;
        let (bytes, _) = d.serialize().unwrap();
        assert_eq!(
            Descriptor::parse(&bytes, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::CoverageViolation
        );
    }

    // -----------------------------------------------------------------------
    // Phase 8 — the optional seek `DIRECTORY` record.
    // -----------------------------------------------------------------------

    /// A seekable descriptor: the two-op indexed descriptor plus the directory
    /// switch.
    fn seekable_descriptor() -> Descriptor {
        let mut d = indexed_descriptor();
        d.seek_directory = true;
        d
    }

    /// Rebuild a descriptor's bytes after mutating its decoded directory payload,
    /// recomputing the record CRC. The directory payload length is unchanged by
    /// offset/count mutations, so the trailer stays consistent. The result is a
    /// *CRC-valid* but potentially lying directory.
    fn rebuild_with_directory(bytes: &[u8], mut mutate: impl FnMut(&mut SeekDirectory)) -> Vec<u8> {
        use crate::container::record::{RecordReader, write_record};
        let header = &bytes[0..HEADER_LEN];
        let mut reader = RecordReader::new(bytes, HEADER_LEN, Limits::DEFAULT);
        let mut records = Vec::new();
        while let Some(r) = reader.next_record().unwrap() {
            records.push(r);
        }
        let mut out = header.to_vec();
        for r in &records {
            if r.tag == RecordTag::Directory as u8 {
                let mut dir = SeekDirectory::decode(&r.payload, Limits::DEFAULT).unwrap();
                mutate(&mut dir);
                write_record(&mut out, r.tag, r.flags, &dir.encode().unwrap()).unwrap();
            } else {
                write_record(&mut out, r.tag, r.flags, &r.payload).unwrap();
            }
        }
        out
    }

    #[test]
    fn seek_directory_roundtrips_materializes_and_charges() {
        let d = seekable_descriptor();
        assert_eq!(
            d.optional_features(),
            crate::container::header::FEATURE_OBSERVATION_INDEX
                | crate::container::header::FEATURE_SEEK_DIRECTORY
        );
        let (bytes, cost) = d.serialize().unwrap();
        assert_eq!(cost.total(), bytes.len() as u64, "cost must be the length");
        assert!(
            cost.directory > 0,
            "the directory payload + framing is charged"
        );

        let parsed = Descriptor::parse(&bytes, Limits::DEFAULT).unwrap();
        assert_eq!(parsed.descriptor, d, "seekable descriptor must round-trip");
        assert!(parsed.descriptor.seek_directory);
        assert_eq!(parsed.cost.total(), bytes.len() as u64);
        assert_eq!(parsed.cost.directory, cost.directory);

        // The normal materialize path is unaffected by the advisory directory.
        let out = crate::materialize::decode_to_bytes(&bytes, Limits::DEFAULT)
            .unwrap()
            .0;
        assert_eq!(out, b"abcde");
    }

    #[test]
    fn directory_is_the_first_record_and_is_optional() {
        let (bytes, _) = seekable_descriptor().serialize().unwrap();
        let mut r = RecordReader::new(&bytes, HEADER_LEN, Limits::DEFAULT);
        let first = r.next_record().unwrap().unwrap();
        assert_eq!(first.tag, RecordTag::Directory as u8);
        assert!(
            first.is_optional(),
            "the directory must carry FLAG_OPTIONAL"
        );
        // The directory is locatable at the fixed offset after the header.
        let mut r = RecordReader::new(&bytes, HEADER_LEN, Limits::DEFAULT);
        r.next_record().unwrap().unwrap();
        assert_eq!(
            r.position(),
            HEADER_LEN + RECORD_OVERHEAD + first.payload.len(),
            "the next record must begin right after the directory"
        );
    }

    #[test]
    fn non_seekable_descriptor_has_no_directory_cost() {
        let d = sample(b"no directory here");
        let (bytes, cost) = d.serialize().unwrap();
        assert_eq!(cost.directory, 0);
        assert_eq!(cost.total(), bytes.len() as u64);
        assert_eq!(d.optional_features(), 0);
        // The record sequence is unchanged: the first record is UNIVERSE, not a
        // DIRECTORY.
        let mut r = RecordReader::new(&bytes, HEADER_LEN, Limits::DEFAULT);
        assert_eq!(
            r.next_record().unwrap().unwrap().tag,
            RecordTag::Universe as u8
        );
        let parsed = Descriptor::parse(&bytes, Limits::DEFAULT).unwrap();
        assert!(!parsed.descriptor.seek_directory);
        assert_eq!(parsed.cost.directory, 0);
    }

    #[test]
    fn seek_directory_without_index_is_rejected() {
        let mut d = sample(b"abc");
        d.seek_directory = true;
        assert_eq!(
            d.serialize().unwrap_err().class(),
            crate::ErrorClass::InvalidContainer
        );
    }

    #[test]
    fn corrupted_directory_payload_is_rejected() {
        let (mut bytes, _) = seekable_descriptor().serialize().unwrap();
        // Flip a byte inside the directory payload (its first payload byte, the
        // version); the record CRC32C must catch it.
        bytes[HEADER_LEN + 8] ^= 0x01;
        assert_eq!(
            Descriptor::parse(&bytes, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidContainer
        );
    }

    #[test]
    fn lying_directory_is_rejected_on_parse() {
        let (bytes, _) = seekable_descriptor().serialize().unwrap();
        // Sanity: the honest directory parses.
        Descriptor::parse(&bytes, Limits::DEFAULT).unwrap();

        // A locator offset that does not match the framing.
        let lying = rebuild_with_directory(&bytes, |dir| dir.entries[1].offset += 1);
        assert_eq!(
            Descriptor::parse(&lying, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidContainer
        );

        // A class count that disagrees with a scan of the locators.
        let lying = rebuild_with_directory(&bytes, |dir| dir.classes[0].count += 1);
        assert_eq!(
            Descriptor::parse(&lying, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidContainer
        );

        // A directory record that does not carry FLAG_OPTIONAL would strand an
        // older decoder, so this build rejects it too.
        let mut reader = RecordReader::new(&bytes, HEADER_LEN, Limits::DEFAULT);
        let mut records = Vec::new();
        while let Some(r) = reader.next_record().unwrap() {
            records.push(r);
        }
        let mut out = bytes[0..HEADER_LEN].to_vec();
        for r in &records {
            let flags = if r.tag == RecordTag::Directory as u8 {
                0
            } else {
                r.flags
            };
            crate::container::record::write_record(&mut out, r.tag, flags, &r.payload).unwrap();
        }
        assert_eq!(
            Descriptor::parse(&out, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidContainer
        );
    }

    // -----------------------------------------------------------------------
    // Phase 13.4 — the optional CHECKPOINT record.
    // -----------------------------------------------------------------------

    /// The seekable descriptor plus a consistent checkpoint over its program.
    fn checkpointed_descriptor() -> Descriptor {
        let mut d = seekable_descriptor();
        let object_lens: Vec<u64> = d.objects.iter().map(|o| o.len()).collect();
        let channel_lens: Vec<u64> = d.channels.iter().map(|c| c.decoded_length).collect();
        let table = CheckpointTable::from_program(
            &d.program,
            &object_lens,
            &channel_lens,
            d.source_len,
            Limits::DEFAULT,
        )
        .unwrap();
        d.checkpoints = Some(table);
        d
    }

    /// Rebuild a descriptor's bytes after mutating its decoded checkpoint payload,
    /// recomputing the record CRC. The entry count (and so the record length) is
    /// unchanged, so the framing stays consistent; the result is a CRC-valid but
    /// potentially lying checkpoint.
    fn rebuild_with_checkpoint(
        bytes: &[u8],
        mut mutate: impl FnMut(&mut CheckpointTable),
    ) -> Vec<u8> {
        use crate::container::record::{RecordReader, write_record};
        let header = &bytes[0..HEADER_LEN];
        let mut reader = RecordReader::new(bytes, HEADER_LEN, Limits::DEFAULT);
        let mut records = Vec::new();
        while let Some(r) = reader.next_record().unwrap() {
            records.push(r);
        }
        let mut out = header.to_vec();
        for r in &records {
            if r.tag == RecordTag::Checkpoint as u8 {
                let mut cp = CheckpointTable::decode(&r.payload, Limits::DEFAULT).unwrap();
                mutate(&mut cp);
                write_record(&mut out, r.tag, r.flags, &cp.encode().unwrap()).unwrap();
            } else {
                write_record(&mut out, r.tag, r.flags, &r.payload).unwrap();
            }
        }
        out
    }

    #[test]
    fn checkpoint_roundtrips_materializes_and_charges() {
        let d = checkpointed_descriptor();
        assert_eq!(
            d.optional_features(),
            crate::container::header::FEATURE_OBSERVATION_INDEX
                | crate::container::header::FEATURE_SEEK_DIRECTORY
                | crate::container::header::FEATURE_CHECKPOINTS
        );
        let (bytes, cost) = d.serialize().unwrap();
        assert_eq!(cost.total(), bytes.len() as u64, "cost must be the length");
        assert!(
            cost.checkpoints > 0,
            "the checkpoint payload + framing is charged"
        );

        let parsed = Descriptor::parse(&bytes, Limits::DEFAULT).unwrap();
        assert_eq!(
            parsed.descriptor, d,
            "checkpointed descriptor must round-trip"
        );
        assert!(parsed.descriptor.checkpoints.is_some());
        assert_eq!(parsed.cost.total(), bytes.len() as u64);
        assert_eq!(parsed.cost.checkpoints, cost.checkpoints);

        // The advisory checkpoint cannot change the materialized bytes.
        let out = crate::materialize::decode_to_bytes(&bytes, Limits::DEFAULT)
            .unwrap()
            .0;
        assert_eq!(out, b"abcde");
    }

    #[test]
    fn checkpoint_without_directory_is_rejected() {
        let mut d = checkpointed_descriptor();
        d.seek_directory = false;
        assert_eq!(
            d.serialize().unwrap_err().class(),
            crate::ErrorClass::InvalidContainer
        );
    }

    #[test]
    fn absent_checkpoint_has_no_cost_and_no_feature_bit() {
        let d = seekable_descriptor();
        assert!(d.checkpoints.is_none());
        let (bytes, cost) = d.serialize().unwrap();
        assert_eq!(cost.checkpoints, 0);
        assert_eq!(cost.total(), bytes.len() as u64);
        assert_eq!(
            d.optional_features() & crate::container::header::FEATURE_CHECKPOINTS,
            0
        );
    }

    #[test]
    fn corrupt_or_non_optional_checkpoint_is_rejected() {
        let (mut bytes, _) = checkpointed_descriptor().serialize().unwrap();
        // Flip a byte inside the checkpoint payload; its record CRC32C must catch it.
        let mut reader =
            crate::container::record::RecordReader::new(&bytes, HEADER_LEN, Limits::DEFAULT);
        let mut at = None;
        loop {
            let p = reader.position();
            match reader.next_record().unwrap() {
                Some(r) if r.tag == RecordTag::Checkpoint as u8 => {
                    at = Some(p + 8);
                    break;
                }
                Some(_) => {}
                None => break,
            }
        }
        bytes[at.expect("a checkpoint record")] ^= 0x01;
        assert_eq!(
            Descriptor::parse(&bytes, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidContainer
        );

        // A checkpoint record that does not carry FLAG_OPTIONAL would strand an
        // older decoder, so this build rejects it too.
        let (bytes, _) = checkpointed_descriptor().serialize().unwrap();
        let mut reader =
            crate::container::record::RecordReader::new(&bytes, HEADER_LEN, Limits::DEFAULT);
        let mut records = Vec::new();
        while let Some(r) = reader.next_record().unwrap() {
            records.push(r);
        }
        let mut out = bytes[0..HEADER_LEN].to_vec();
        for r in &records {
            let flags = if r.tag == RecordTag::Checkpoint as u8 {
                0
            } else {
                r.flags
            };
            crate::container::record::write_record(&mut out, r.tag, flags, &r.payload).unwrap();
        }
        assert_eq!(
            Descriptor::parse(&out, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidContainer
        );
    }

    #[test]
    fn lying_checkpoint_is_rejected_on_parse() {
        let (bytes, _) = checkpointed_descriptor().serialize().unwrap();
        Descriptor::parse(&bytes, Limits::DEFAULT).unwrap();

        // A boundary whose length contradicts the program (internally consistent).
        let lying = rebuild_with_checkpoint(&bytes, |cp| {
            cp.entries[0].out_len = 4;
            cp.entries[1].out_start = 4;
            cp.entries[1].out_len = 1;
        });
        assert_eq!(
            Descriptor::parse(&lying, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::CoverageViolation
        );

        // A checkpoint not bound to this GRAPH record.
        let lying = rebuild_with_checkpoint(&bytes, |cp| cp.graph_crc32c ^= 0xFFFF_FFFF);
        assert_eq!(
            Descriptor::parse(&lying, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidContainer
        );

        // A checkpoint that declares a different source length.
        let lying = rebuild_with_checkpoint(&bytes, |cp| cp.source_len += 1);
        assert_eq!(
            Descriptor::parse(&lying, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::IntegrityMismatch
        );
    }
}
