//! Length-delimited record framing.
//!
//! ```text
//! record := tag:u8 flags:u8 reserved:u16=0 length:u32 payload:[u8;length] crc32c:u32
//! ```
//!
//! The CRC covers the 8-byte record header and the payload. `reserved` must be
//! zero. The `flags` bit [`FLAG_OPTIONAL`] marks a record whose *tag* a decoder
//! may skip if unknown; unknown non-optional tags fail closed.

use crate::error::{Error, Result};
use crate::integrity::crc32c;
use crate::limits::Limits;

/// Bytes of per-record framing before the payload.
pub const RECORD_HEADER_LEN: usize = 8;
/// Bytes of per-record framing after the payload (the CRC).
pub const RECORD_TRAILER_LEN: usize = 4;
/// Total framing overhead per record.
pub const RECORD_OVERHEAD: usize = RECORD_HEADER_LEN + RECORD_TRAILER_LEN;

/// Flag: this record's tag may be skipped by a decoder that does not know it.
pub const FLAG_OPTIONAL: u8 = 0x01;

/// Known record classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RecordTag {
    /// Universe declaration (opcode/coder/limit semantics version).
    Universe = 0x01,
    /// Source-format descriptor.
    Format = 0x02,
    /// A raw byte object.
    Object = 0x10,
    /// The reconstruction graph (DRA program).
    Graph = 0x20,
    /// An entropy model (Phase 2+).
    Model = 0x30,
    /// An entropy channel capsule (Phase 2+).
    EntropyChannel = 0x40,
    /// A typed residual stream (later phases).
    Residual = 0x50,
    /// A random-access checkpoint (later phases).
    Checkpoint = 0x60,
    /// A physical/logical index (later phases).
    Index = 0x70,
    /// A reference to an external content-addressed object (Phase 9+).
    ExternalRef = 0x80,
    /// Whole-source integrity manifest.
    Integrity = 0xF0,
    /// Terminal record.
    Trailer = 0xFF,
}

impl RecordTag {
    /// Map a raw tag byte to a known tag, if any.
    pub const fn from_u8(b: u8) -> Option<RecordTag> {
        match b {
            0x01 => Some(RecordTag::Universe),
            0x02 => Some(RecordTag::Format),
            0x10 => Some(RecordTag::Object),
            0x20 => Some(RecordTag::Graph),
            0x30 => Some(RecordTag::Model),
            0x40 => Some(RecordTag::EntropyChannel),
            0x50 => Some(RecordTag::Residual),
            0x60 => Some(RecordTag::Checkpoint),
            0x70 => Some(RecordTag::Index),
            0x80 => Some(RecordTag::ExternalRef),
            0xF0 => Some(RecordTag::Integrity),
            0xFF => Some(RecordTag::Trailer),
            _ => None,
        }
    }

    /// Stable short name for diagnostics and receipts.
    pub const fn name(self) -> &'static str {
        match self {
            RecordTag::Universe => "UNIVERSE",
            RecordTag::Format => "FORMAT",
            RecordTag::Object => "OBJECT",
            RecordTag::Graph => "GRAPH",
            RecordTag::Model => "MODEL",
            RecordTag::EntropyChannel => "ENTROPY_CHANNEL",
            RecordTag::Residual => "RESIDUAL",
            RecordTag::Checkpoint => "CHECKPOINT",
            RecordTag::Index => "INDEX",
            RecordTag::ExternalRef => "EXTERNAL_REF",
            RecordTag::Integrity => "INTEGRITY",
            RecordTag::Trailer => "TRAILER",
        }
    }
}

/// A single decoded record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// Raw tag byte (kept raw so unknown optional tags survive a reparse).
    pub tag: u8,
    /// Raw flags byte.
    pub flags: u8,
    /// Payload bytes.
    pub payload: Vec<u8>,
}

impl Record {
    /// Construct a record with the given tag byte and payload.
    pub fn new(tag: u8, payload: Vec<u8>) -> Self {
        Record {
            tag,
            flags: 0,
            payload,
        }
    }

    /// Construct a record for a known tag.
    pub fn known(tag: RecordTag, payload: Vec<u8>) -> Self {
        Record {
            tag: tag as u8,
            flags: 0,
            payload,
        }
    }

    /// Whether the optional-skip flag is set.
    pub fn is_optional(&self) -> bool {
        self.flags & FLAG_OPTIONAL != 0
    }
}

/// Append an encoded record to `out`. `payload` must fit in `u32`.
pub fn write_record(out: &mut Vec<u8>, tag: u8, flags: u8, payload: &[u8]) -> Result<()> {
    let len = u32::try_from(payload.len())
        .map_err(|_| Error::resource_limit("record payload exceeds 4 GiB"))?;
    let start = out.len();
    out.push(tag);
    out.push(flags);
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(payload);
    let crc = crc32c(&out[start..]);
    out.extend_from_slice(&crc.to_le_bytes());
    Ok(())
}

/// An iterator over the records of a byte slice.
pub struct RecordReader<'a> {
    data: &'a [u8],
    pos: usize,
    limits: Limits,
    count: u32,
}

impl<'a> RecordReader<'a> {
    /// Create a reader starting at `offset` within `data`.
    pub fn new(data: &'a [u8], offset: usize, limits: Limits) -> Self {
        RecordReader {
            data,
            pos: offset,
            limits,
            count: 0,
        }
    }

    /// Current byte offset of the next record header.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Records successfully read so far.
    pub fn records_read(&self) -> u32 {
        self.count
    }

    /// Read the next record, or `None` at end of input.
    pub fn next_record(&mut self) -> Result<Option<Record>> {
        if self.pos == self.data.len() {
            return Ok(None);
        }
        if self.count >= self.limits.max_record_count {
            return Err(Error::resource_limit("record count limit exceeded"));
        }
        let remaining = self.data.len() - self.pos;
        if remaining < RECORD_OVERHEAD {
            return Err(Error::invalid_container(format!(
                "truncated record header: {remaining} bytes remain"
            )));
        }
        let hdr = &self.data[self.pos..self.pos + RECORD_HEADER_LEN];
        let tag = hdr[0];
        let flags = hdr[1];
        let reserved = u16::from_le_bytes([hdr[2], hdr[3]]);
        if reserved != 0 {
            return Err(Error::invalid_container(
                "record reserved field must be zero",
            ));
        }
        let len = u32::from_le_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]);
        if len > self.limits.max_record_len {
            return Err(Error::resource_limit(format!(
                "record payload length {len} exceeds limit {}",
                self.limits.max_record_len
            )));
        }
        let total = RECORD_HEADER_LEN
            .checked_add(len as usize)
            .and_then(|n| n.checked_add(RECORD_TRAILER_LEN))
            .ok_or_else(|| Error::invalid_container("record length overflow"))?;
        if total > remaining {
            return Err(Error::invalid_container(format!(
                "truncated record payload: need {total}, have {remaining}"
            )));
        }
        let body = &self.data[self.pos..self.pos + RECORD_HEADER_LEN + len as usize];
        let want_crc = u32::from_le_bytes([
            self.data[self.pos + RECORD_HEADER_LEN + len as usize],
            self.data[self.pos + RECORD_HEADER_LEN + len as usize + 1],
            self.data[self.pos + RECORD_HEADER_LEN + len as usize + 2],
            self.data[self.pos + RECORD_HEADER_LEN + len as usize + 3],
        ]);
        let got_crc = crc32c(body);
        if want_crc != got_crc {
            return Err(Error::invalid_container(format!(
                "record CRC32C mismatch at offset {}: declared {want_crc:#010x}, computed {got_crc:#010x}",
                self.pos
            )));
        }
        let payload = self.data
            [self.pos + RECORD_HEADER_LEN..self.pos + RECORD_HEADER_LEN + len as usize]
            .to_vec();
        self.pos += total;
        self.count += 1;
        Ok(Some(Record {
            tag,
            flags,
            payload,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_read_roundtrip() {
        let mut buf = Vec::new();
        write_record(&mut buf, RecordTag::Universe as u8, 0, b"hello").unwrap();
        write_record(&mut buf, RecordTag::Object as u8, 0, &[1, 2, 3]).unwrap();
        let mut r = RecordReader::new(&buf, 0, Limits::DEFAULT);
        let a = r.next_record().unwrap().unwrap();
        assert_eq!(a.tag, RecordTag::Universe as u8);
        assert_eq!(a.payload, b"hello");
        let b = r.next_record().unwrap().unwrap();
        assert_eq!(b.tag, RecordTag::Object as u8);
        assert_eq!(b.payload, vec![1, 2, 3]);
        assert!(r.next_record().unwrap().is_none());
    }

    #[test]
    fn detects_payload_corruption() {
        let mut buf = Vec::new();
        write_record(&mut buf, 0x10, 0, b"abcdef").unwrap();
        buf[9] ^= 0xFF; // corrupt a payload byte
        let mut r = RecordReader::new(&buf, 0, Limits::DEFAULT);
        let e = r.next_record().unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::InvalidContainer);
    }

    #[test]
    fn detects_truncation() {
        let mut buf = Vec::new();
        write_record(&mut buf, 0x10, 0, b"abcdef").unwrap();
        buf.truncate(buf.len() - 2);
        let mut r = RecordReader::new(&buf, 0, Limits::DEFAULT);
        let e = r.next_record().unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::InvalidContainer);
    }

    #[test]
    fn rejects_nonzero_reserved() {
        let mut buf = Vec::new();
        write_record(&mut buf, 0x10, 0, b"x").unwrap();
        buf[2] = 1; // reserved
        let mut r = RecordReader::new(&buf, 0, Limits::DEFAULT);
        let e = r.next_record().unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::InvalidContainer);
    }

    #[test]
    fn enforces_record_length_limit() {
        let mut buf = Vec::new();
        write_record(&mut buf, 0x10, 0, &[0u8; 100]).unwrap();
        let limits = Limits {
            max_record_len: 10,
            ..Limits::DEFAULT
        };
        let mut r = RecordReader::new(&buf, 0, limits);
        let e = r.next_record().unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::ResourceLimit);
    }
}
