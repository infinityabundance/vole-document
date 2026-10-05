//! High-level `.voldoc` descriptor: serialize and parse.
//!
//! A descriptor binds a source-format class, a universe declaration, raw byte
//! objects, a reconstruction program, and a whole-source integrity manifest
//! into one framed container. Parsing is structural and bounded; it does not
//! materialize bytes. Materialization is a separate, explicit step.

use crate::EXACTNESS_PROFILE_EXACT_BYTES;
use crate::accounting::CostBreakdown;
use crate::container::header::{HEADER_LEN, Header, MAGIC};
use crate::container::record::{RECORD_OVERHEAD, RecordReader, RecordTag};
use crate::dra::Program;
use crate::entropy::codec::EntropyChannelDescriptor;
use crate::entropy::model::EntropyModel;
use crate::error::{Error, Result};
use crate::integrity::sha256;
use crate::limits::Limits;

/// The Phase-2 reconstruction universe declaration.
///
/// Changing any opcode, coder, limit semantic, or adapter meaning requires a
/// new universe string. The `universe_id` in the header is the first 16 bytes
/// of SHA-256 over this string.
pub const UNIVERSE: &str = "vole-document;universe;phase5-8;exact-bytes;dra-6;opaque+entropy+pdf+channels+offsets+packed+packed-channels";

/// First 16 bytes of SHA-256 over a universe declaration string.
pub fn universe_id_from_str(universe: &str) -> [u8; 16] {
    let full = sha256(universe.as_bytes());
    let mut id = [0u8; 16];
    id.copy_from_slice(&full[0..16]);
    id
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
    pub objects: Vec<Vec<u8>>,
    /// The reconstruction program.
    pub program: Program,
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

impl Descriptor {
    /// The header that this descriptor serializes to.
    pub fn header(&self) -> Header {
        Header::new(
            universe_id_from_str(&self.universe),
            self.source_len,
            EXACTNESS_PROFILE_EXACT_BYTES,
            self.source_format,
        )
    }

    /// Serialize to a complete `.voldoc` byte sequence plus cost attribution.
    pub fn serialize(&self) -> Result<(Vec<u8>, CostBreakdown)> {
        let mut cost = CostBreakdown {
            header: HEADER_LEN as u64,
            ..Default::default()
        };
        let mut out = Vec::new();

        let header = self.header();
        out.extend_from_slice(&header.encode());

        let mut records: u64 = 0;
        let mut write = |out: &mut Vec<u8>, tag: RecordTag, payload: &[u8]| -> Result<()> {
            crate::container::record::write_record(out, tag as u8, 0, payload)?;
            records += 1;
            Ok(())
        };

        // UNIVERSE
        write(&mut out, RecordTag::Universe, self.universe.as_bytes())?;
        cost.universe = self.universe.len() as u64;

        // FORMAT: [class u8][basis_len u32 LE][basis bytes]
        let basis = self.format_basis.as_bytes();
        let basis_len = u32::try_from(basis.len())
            .map_err(|_| Error::resource_limit("format basis too long"))?;
        let mut fmt = Vec::with_capacity(5 + basis.len());
        fmt.push(self.source_format);
        fmt.extend_from_slice(&basis_len.to_le_bytes());
        fmt.extend_from_slice(basis);
        write(&mut out, RecordTag::Format, &fmt)?;
        cost.format = fmt.len() as u64;

        // MODELS
        for model in &self.models {
            let encoded = model.encode()?;
            write(&mut out, RecordTag::Model, &encoded)?;
            cost.models += encoded.len() as u64;
        }

        // ENTROPY CHANNELS
        for channel in &self.channels {
            let encoded = channel.encode()?;
            write(&mut out, RecordTag::EntropyChannel, &encoded)?;
            cost.entropy_payload += encoded.len() as u64;
        }

        // OBJECTS
        for obj in &self.objects {
            write(&mut out, RecordTag::Object, obj)?;
            cost.objects += obj.len() as u64;
        }

        // GRAPH
        let graph = self.program.encode()?;
        write(&mut out, RecordTag::Graph, &graph)?;
        cost.graph = graph.len() as u64;

        // INTEGRITY: [sha256 32][source_len u64 LE]
        let mut integ = Vec::with_capacity(40);
        integ.extend_from_slice(&self.source_sha256);
        integ.extend_from_slice(&self.source_len.to_le_bytes());
        write(&mut out, RecordTag::Integrity, &integ)?;
        cost.integrity = integ.len() as u64;

        // TRAILER: [record_count u32][payload_bytes u64][MAGIC 8]
        // record_count includes the trailer itself.
        let total_records =
            u32::try_from(records + 1).map_err(|_| Error::resource_limit("too many records"))?;
        let payload_bytes = (out.len() - HEADER_LEN) as u64;
        let mut trailer = Vec::with_capacity(20);
        trailer.extend_from_slice(&total_records.to_le_bytes());
        trailer.extend_from_slice(&payload_bytes.to_le_bytes());
        trailer.extend_from_slice(&MAGIC);
        crate::container::record::write_record(&mut out, RecordTag::Trailer as u8, 0, &trailer)?;
        cost.trailer = trailer.len() as u64;

        // Framing overhead for every record after the fixed header.
        cost.record_framing = RECORD_OVERHEAD as u64 * total_records as u64;

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
        let mut objects: Vec<Vec<u8>> = Vec::new();
        let mut program: Option<Program> = None;
        let mut source_sha256: Option<[u8; 32]> = None;
        let mut source_len: Option<u64> = None;
        let mut saw_trailer = false;
        let mut trailer_record_count: Option<u32> = None;
        let mut records_seen: u32 = 0;

        while let Some(rec) = reader.next_record()? {
            records_seen += 1;
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
                    objects.push(rec.payload);
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
                    program = Some(p);
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
                // Phase 2+ mandatory records have no meaning in this universe.
                Some(RecordTag::Residual)
                | Some(RecordTag::Checkpoint)
                | Some(RecordTag::Index)
                | Some(RecordTag::ExternalRef) => {
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
        let object_lens: Vec<u64> = objects.iter().map(|o| o.len() as u64).collect();
        let channel_lens: Vec<u64> = channels.iter().map(|c| c.decoded_length).collect();
        let (predicted, coverage) = program.analyze(&object_lens, &channel_lens, limits)?;
        if predicted != source_len {
            return Err(Error::coverage_violation(format!(
                "reconstruction program predicts {predicted} bytes but {source_len} were declared"
            )));
        }
        coverage.validate(source_len)?;

        cost.record_framing = RECORD_OVERHEAD as u64 * records_seen as u64;

        Ok(ParsedDescriptor {
            descriptor: Descriptor {
                universe,
                source_format: class,
                format_basis: basis,
                models,
                channels,
                objects,
                program,
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
            objects: vec![source.to_vec()],
            program: Program::new(vec![Op::EmitObject { object_id: 0 }]),
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
}
