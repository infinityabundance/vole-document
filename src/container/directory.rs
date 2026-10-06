//! The optional `DIRECTORY` record: a bounded, advisory seek directory.
//!
//! The directory is a self-describing, little-endian, versioned payload written
//! as the **first** record (fixed offset 64) with [`FLAG_OPTIONAL`]. It lets a
//! seek reader jump to the records a query needs instead of reading the whole
//! descriptor. It is **advisory, never authority**: every locator is
//! cross-checked against the record framing, and a decoder that ignores the
//! record still fully materializes the source exactly.
//!
//! ```text
//! seek_directory_v1 :=
//!     version:u8 = 1
//!     section_flags:u8            # bit0 LOCATORS, bit1 CLASS_INDEX, bit2 CHANNEL_LENGTHS
//!     entry_count:u32 LE          # if bit0
//!     entry[entry_count]          # tag:u8 | offset:u64 | payload_len:u32
//!     class_count:u8              # if bit1
//!     class_entry[class_count]    # tag:u8 | first:u32 | count:u32
//!     channel_count:u32 LE        # if bit2
//!     channel_decoded_len[channel_count]:u64 LE
//! ```
//!
//! `LOCATORS` lists every record in file order, including the `DIRECTORY` itself
//! (entry 0, at offset [`HEADER_LEN`]), so offset contiguity is checkable against
//! the actual framing. `CLASS_INDEX` gives the `first`/`count` of each repeating
//! class (`OBJECT`, `ENTROPY_CHANNEL`, `MODEL`) and each singleton (`GRAPH`,
//! `OBSERVATION_INDEX`, `INTEGRITY`, `TRAILER`) into `entry[]`. `CHANNEL_LENGTHS`
//! carries each entropy channel's decoded length in table order.
//!
//! Unknown version or section flags fail closed. All arithmetic is checked.

use crate::container::header::HEADER_LEN;
use crate::container::record::{RECORD_OVERHEAD, RecordTag};
use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the seek directory implemented by this build.
pub const SEEK_DIRECTORY_VERSION: u8 = 1;

/// Section flag: the locator table (bit 0) is present.
pub const SECTION_LOCATORS: u8 = 0x01;
/// Section flag: the class index (bit 1) is present.
pub const SECTION_CLASS_INDEX: u8 = 0x02;
/// Section flag: the channel decoded-length table (bit 2) is present.
pub const SECTION_CHANNEL_LENGTHS: u8 = 0x04;

/// Every section flag this version understands. Any other bit fails closed.
pub const SEEK_DIRECTORY_KNOWN_SECTION_FLAGS: u8 =
    SECTION_LOCATORS | SECTION_CLASS_INDEX | SECTION_CHANNEL_LENGTHS;

/// The section set emitted by this build's encoder.
pub const SEEK_DIRECTORY_ALL_SECTIONS: u8 = SEEK_DIRECTORY_KNOWN_SECTION_FLAGS;

/// Encoded size of one locator entry: `tag u8 | offset u64 | payload_len u32`.
pub const DIRECTORY_ENTRY_LEN: usize = 1 + 8 + 4;
/// Encoded size of one class entry: `tag u8 | first u32 | count u32`.
pub const CLASS_ENTRY_LEN: usize = 1 + 4 + 4;

/// The record classes that receive a `CLASS_INDEX` entry, in emission order.
///
/// Repeating classes first, then singletons. The `DIRECTORY` tag is deliberately
/// absent: it is always entry 0 and locatable at a constant offset.
pub const CLASS_INDEX_TAGS: [RecordTag; 7] = [
    RecordTag::Model,
    RecordTag::EntropyChannel,
    RecordTag::Object,
    RecordTag::Graph,
    RecordTag::ObservationIndex,
    RecordTag::Integrity,
    RecordTag::Trailer,
];

/// One record locator: where a record's tag byte sits and its declared payload
/// length. `offset` is absolute from the start of the descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectoryEntry {
    /// Raw record tag byte.
    pub tag: u8,
    /// Absolute file offset of the record's tag byte.
    pub offset: u64,
    /// Declared payload length of the record.
    pub payload_len: u32,
}

/// One class index entry: the `first` locator index of `tag` and how many
/// consecutive locators carry it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassEntry {
    /// Raw record tag byte this entry indexes.
    pub tag: u8,
    /// Index into [`SeekDirectory::entries`] of the first record of this class.
    pub first: u32,
    /// Number of records of this class.
    pub count: u32,
}

/// A decoded `DIRECTORY` record payload.
///
/// Field vectors correspond to the sections named by `section_flags`; vectors for
/// absent sections are empty.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SeekDirectory {
    /// Section flags present in the encoded record.
    pub section_flags: u8,
    /// Record locators in file order, when [`SECTION_LOCATORS`] is set.
    pub entries: Vec<DirectoryEntry>,
    /// Per-class index, when [`SECTION_CLASS_INDEX`] is set.
    pub classes: Vec<ClassEntry>,
    /// Per-`ENTROPY_CHANNEL` decoded length in table order, when
    /// [`SECTION_CHANNEL_LENGTHS`] is set.
    pub channel_lengths: Vec<u64>,
}

/// The on-disk location of one record, used to cross-check a directory against
/// the actual framing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordSite {
    /// Raw record tag byte.
    pub tag: u8,
    /// Absolute file offset of the record's tag byte.
    pub offset: u64,
    /// Declared payload length of the record.
    pub payload_len: u32,
}

impl SeekDirectory {
    /// Encode the payload to canonical bytes.
    ///
    /// A section's flag and its vector must agree: a set flag with an empty
    /// vector is encoded as a count of zero (valid), but a *clear* flag with a
    /// non-empty vector is an error rather than a silently dropped section.
    pub fn encode(&self) -> Result<Vec<u8>> {
        if self.section_flags & !SEEK_DIRECTORY_KNOWN_SECTION_FLAGS != 0 {
            return Err(Error::invalid_container(
                "seek directory has unknown section flags",
            ));
        }
        if self.section_flags & SECTION_LOCATORS == 0 && !self.entries.is_empty() {
            return Err(Error::invalid_container(
                "seek directory carries locators without the locator section flag",
            ));
        }
        if self.section_flags & SECTION_CLASS_INDEX == 0 && !self.classes.is_empty() {
            return Err(Error::invalid_container(
                "seek directory carries classes without the class-index section flag",
            ));
        }
        if self.section_flags & SECTION_CHANNEL_LENGTHS == 0 && !self.channel_lengths.is_empty() {
            return Err(Error::invalid_container(
                "seek directory carries channel lengths without the channel-lengths section flag",
            ));
        }

        let mut out = Vec::new();
        out.push(SEEK_DIRECTORY_VERSION);
        out.push(self.section_flags);

        if self.section_flags & SECTION_LOCATORS != 0 {
            let count = u32::try_from(self.entries.len())
                .map_err(|_| Error::resource_limit("seek directory entry_count exceeds u32"))?;
            out.extend_from_slice(&count.to_le_bytes());
            for e in &self.entries {
                out.push(e.tag);
                out.extend_from_slice(&e.offset.to_le_bytes());
                out.extend_from_slice(&e.payload_len.to_le_bytes());
            }
        }
        if self.section_flags & SECTION_CLASS_INDEX != 0 {
            let count = u8::try_from(self.classes.len())
                .map_err(|_| Error::resource_limit("seek directory class_count exceeds u8"))?;
            out.push(count);
            for c in &self.classes {
                out.push(c.tag);
                out.extend_from_slice(&c.first.to_le_bytes());
                out.extend_from_slice(&c.count.to_le_bytes());
            }
        }
        if self.section_flags & SECTION_CHANNEL_LENGTHS != 0 {
            let count = u32::try_from(self.channel_lengths.len())
                .map_err(|_| Error::resource_limit("seek directory channel_count exceeds u32"))?;
            out.extend_from_slice(&count.to_le_bytes());
            for &len in &self.channel_lengths {
                out.extend_from_slice(&len.to_le_bytes());
            }
        }
        Ok(out)
    }

    /// Decode and structurally bound a payload.
    ///
    /// This checks the size bound, the version, the section flags, and that every
    /// section fits its declared count. It does **not** check agreement with the
    /// actual record framing; call [`SeekDirectory::validate`] for that.
    pub fn decode(bytes: &[u8], limits: Limits) -> Result<SeekDirectory> {
        if bytes.len() as u64 > u64::from(limits.max_directory_bytes) {
            return Err(Error::resource_limit(format!(
                "seek directory payload {} exceeds limit {}",
                bytes.len(),
                limits.max_directory_bytes
            )));
        }
        if bytes.len() < 2 {
            return Err(Error::invalid_container("truncated SEEK_DIRECTORY header"));
        }
        let version = bytes[0];
        if version != SEEK_DIRECTORY_VERSION {
            return Err(Error::unsupported_version(format!(
                "seek directory version {version} is not supported"
            )));
        }
        let section_flags = bytes[1];
        if section_flags & !SEEK_DIRECTORY_KNOWN_SECTION_FLAGS != 0 {
            return Err(Error::invalid_container(
                "SEEK_DIRECTORY has unknown section flags",
            ));
        }

        let mut p = 2usize;

        let mut entries = Vec::new();
        if section_flags & SECTION_LOCATORS != 0 {
            let count = read_u32(bytes, &mut p)?;
            if count > limits.max_record_count {
                return Err(Error::resource_limit(format!(
                    "seek directory entry_count {count} exceeds limit {}",
                    limits.max_record_count
                )));
            }
            let count = count as usize;
            require(bytes, p, count, DIRECTORY_ENTRY_LEN)?;
            entries.reserve(count);
            for _ in 0..count {
                entries.push(DirectoryEntry {
                    tag: read_u8(bytes, &mut p)?,
                    offset: read_u64(bytes, &mut p)?,
                    payload_len: read_u32(bytes, &mut p)?,
                });
            }
        }

        let mut classes = Vec::new();
        if section_flags & SECTION_CLASS_INDEX != 0 {
            let count = read_u8(bytes, &mut p)? as usize;
            require(bytes, p, count, CLASS_ENTRY_LEN)?;
            classes.reserve(count);
            for _ in 0..count {
                classes.push(ClassEntry {
                    tag: read_u8(bytes, &mut p)?,
                    first: read_u32(bytes, &mut p)?,
                    count: read_u32(bytes, &mut p)?,
                });
            }
        }

        let mut channel_lengths = Vec::new();
        if section_flags & SECTION_CHANNEL_LENGTHS != 0 {
            let count = read_u32(bytes, &mut p)?;
            if count > limits.max_channel_count {
                return Err(Error::resource_limit(format!(
                    "seek directory channel_count {count} exceeds limit {}",
                    limits.max_channel_count
                )));
            }
            let count = count as usize;
            require(bytes, p, count, 8)?;
            channel_lengths.reserve(count);
            for _ in 0..count {
                channel_lengths.push(read_u64(bytes, &mut p)?);
            }
        }

        if p != bytes.len() {
            return Err(Error::invalid_container(format!(
                "SEEK_DIRECTORY has {} trailing bytes",
                bytes.len() - p
            )));
        }

        Ok(SeekDirectory {
            section_flags,
            entries,
            classes,
            channel_lengths,
        })
    }

    /// Validate the directory against the actual record framing.
    ///
    /// Runs the internal structural checks and then requires that [`Self::entries`]
    /// agrees byte-for-byte with `records` (tag, offset, payload length). This is
    /// the advisory cross-check: the directory is a claim, and the framing is the
    /// authority.
    pub fn validate(&self, records: &[RecordSite], file_len: u64, limits: Limits) -> Result<()> {
        self.validate_structural(file_len, limits)?;

        if self.section_flags & SECTION_LOCATORS == 0 {
            return Err(Error::invalid_container(
                "seek directory omits the locator section",
            ));
        }
        if self.entries.len() != records.len() {
            return Err(Error::invalid_container(format!(
                "seek directory lists {} records but {} were framed",
                self.entries.len(),
                records.len()
            )));
        }
        for (i, (entry, site)) in self.entries.iter().zip(records).enumerate() {
            if entry.tag != site.tag
                || entry.offset != site.offset
                || entry.payload_len != site.payload_len
            {
                return Err(Error::invalid_container(format!(
                    "seek directory locator {i} disagrees with the record framing"
                )));
            }
        }
        Ok(())
    }

    /// Validate the directory's internal consistency and its coverage of the file.
    ///
    /// This needs only the directory and the file length, so a seek reader can run
    /// it without reading the whole record set.
    pub fn validate_structural(&self, file_len: u64, limits: Limits) -> Result<()> {
        if self.section_flags & !SEEK_DIRECTORY_KNOWN_SECTION_FLAGS != 0 {
            return Err(Error::invalid_container(
                "seek directory has unknown section flags",
            ));
        }
        if self.entries.len() > limits.max_record_count as usize {
            return Err(Error::resource_limit(
                "seek directory entry_count exceeds the record-count limit",
            ));
        }
        if self.channel_lengths.len() > limits.max_channel_count as usize {
            return Err(Error::resource_limit(
                "seek directory channel_count exceeds the channel-count limit",
            ));
        }

        if self.section_flags & SECTION_LOCATORS != 0 {
            if self.entries.is_empty() {
                return Err(Error::invalid_container("seek directory has no locators"));
            }
            // Entry 0 is the directory itself, at the fixed end of the header.
            let first = self.entries[0];
            if first.tag != RecordTag::Directory as u8 {
                return Err(Error::invalid_container(
                    "seek directory locator 0 is not the DIRECTORY record",
                ));
            }
            if first.offset != HEADER_LEN as u64 {
                return Err(Error::invalid_container(
                    "seek directory locator 0 is not at the end of the header",
                ));
            }

            // Locators must be strictly ascending and contiguous, and the last
            // record must end exactly at `file_len`.
            let mut prev_end: Option<u64> = None;
            for (i, e) in self.entries.iter().enumerate() {
                let end = e
                    .offset
                    .checked_add(RECORD_OVERHEAD as u64)
                    .and_then(|v| v.checked_add(u64::from(e.payload_len)))
                    .ok_or_else(|| {
                        Error::invalid_container(format!(
                            "seek directory locator {i} end overflows"
                        ))
                    })?;
                if let Some(pe) = prev_end
                    && e.offset != pe
                {
                    return Err(Error::invalid_container(format!(
                        "seek directory locator {i} is not contiguous with the previous record"
                    )));
                }
                prev_end = Some(end);
            }
            let last = self.entries.last().expect("non-empty");
            if last.tag != RecordTag::Trailer as u8 {
                return Err(Error::invalid_container(
                    "seek directory does not end at the TRAILER record",
                ));
            }
            match prev_end {
                Some(end) if end == file_len => {}
                Some(end) => {
                    return Err(Error::invalid_container(format!(
                        "seek directory ends at offset {end} but the file is {file_len} bytes"
                    )));
                }
                None => unreachable!("entries is non-empty"),
            }

            // Singleton classes may appear at most once.
            for tag in [
                RecordTag::Graph,
                RecordTag::ObservationIndex,
                RecordTag::Integrity,
                RecordTag::Trailer,
            ] {
                let n = self.entries.iter().filter(|e| e.tag == tag as u8).count();
                if n > 1 {
                    return Err(Error::invalid_container(format!(
                        "seek directory lists the singleton {} {n} times",
                        tag.name()
                    )));
                }
            }
        }

        // The class index must agree with a linear scan of the locators.
        for (i, c) in self.classes.iter().enumerate() {
            if c.count == 0 {
                return Err(Error::invalid_container(format!(
                    "seek directory class entry {i} has a zero count"
                )));
            }
            if self.classes[..i].iter().any(|o| o.tag == c.tag) {
                return Err(Error::invalid_container(format!(
                    "seek directory has duplicate class entry for tag {:#04x}",
                    c.tag
                )));
            }
            let first = c.first as usize;
            let count = c.count as usize;
            let end = first.checked_add(count).ok_or_else(|| {
                Error::invalid_container(format!("seek directory class entry {i} range overflows"))
            })?;
            if end > self.entries.len() {
                return Err(Error::invalid_container(format!(
                    "seek directory class entry {i} range {first}..{end} exceeds {} locators",
                    self.entries.len()
                )));
            }
            if self.entries[first..end].iter().any(|e| e.tag != c.tag) {
                return Err(Error::invalid_container(format!(
                    "seek directory class entry {i} spans a foreign tag"
                )));
            }
            let scan_first = self.entries.iter().position(|e| e.tag == c.tag);
            let scan_count = self.entries.iter().filter(|e| e.tag == c.tag).count();
            if scan_first != Some(first) || scan_count != count {
                return Err(Error::invalid_container(format!(
                    "seek directory class entry {i} disagrees with a scan of the locators"
                )));
            }
        }

        if self.section_flags & SECTION_CHANNEL_LENGTHS != 0 {
            let channels = self
                .entries
                .iter()
                .filter(|e| e.tag == RecordTag::EntropyChannel as u8)
                .count();
            if channels != self.channel_lengths.len() {
                return Err(Error::invalid_container(format!(
                    "seek directory lists {} channel lengths for {channels} channel records",
                    self.channel_lengths.len()
                )));
            }
        }

        Ok(())
    }
}

/// Build a canonical class index from a locator table: one entry per class in
/// [`CLASS_INDEX_TAGS`] that appears, with its first index and total count.
pub fn class_index(entries: &[DirectoryEntry]) -> Vec<ClassEntry> {
    let mut classes = Vec::new();
    for tag in CLASS_INDEX_TAGS {
        let count = entries.iter().filter(|e| e.tag == tag as u8).count();
        if count == 0 {
            continue;
        }
        let first = entries
            .iter()
            .position(|e| e.tag == tag as u8)
            .expect("count > 0 implies a first occurrence");
        classes.push(ClassEntry {
            tag: tag as u8,
            first: first as u32,
            count: count as u32,
        });
    }
    classes
}

/// Ensure `count` entries of `entry_len` bytes fit in `bytes` from `p`.
fn require(bytes: &[u8], p: usize, count: usize, entry_len: usize) -> Result<()> {
    let need = count
        .checked_mul(entry_len)
        .ok_or_else(|| Error::invalid_container("SEEK_DIRECTORY section length overflow"))?;
    let available = bytes
        .len()
        .checked_sub(p)
        .ok_or_else(|| Error::invalid_container("SEEK_DIRECTORY cursor past end of payload"))?;
    if need > available {
        return Err(Error::invalid_container(format!(
            "SEEK_DIRECTORY section needs {need} bytes but only {available} remain"
        )));
    }
    Ok(())
}

fn read_u8(bytes: &[u8], p: &mut usize) -> Result<u8> {
    let v = *bytes
        .get(*p)
        .ok_or_else(|| Error::invalid_container("truncated SEEK_DIRECTORY payload"))?;
    *p += 1;
    Ok(v)
}

fn read_u32(bytes: &[u8], p: &mut usize) -> Result<u32> {
    let end = p
        .checked_add(4)
        .ok_or_else(|| Error::invalid_container("SEEK_DIRECTORY cursor overflow"))?;
    let slice = bytes
        .get(*p..end)
        .ok_or_else(|| Error::invalid_container("truncated SEEK_DIRECTORY payload"))?;
    *p = end;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_u64(bytes: &[u8], p: &mut usize) -> Result<u64> {
    let end = p
        .checked_add(8)
        .ok_or_else(|| Error::invalid_container("SEEK_DIRECTORY cursor overflow"))?;
    let slice = bytes
        .get(*p..end)
        .ok_or_else(|| Error::invalid_container("truncated SEEK_DIRECTORY payload"))?;
    *p = end;
    Ok(u64::from_le_bytes([
        slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(tag: RecordTag, offset: u64, payload_len: u32) -> DirectoryEntry {
        DirectoryEntry {
            tag: tag as u8,
            offset,
            payload_len,
        }
    }

    /// A directory whose locators describe a synthetic three-record file:
    /// DIRECTORY at 64, INTEGRITY at 100, TRAILER at 152 (each 12 B framing).
    fn sample_dir() -> SeekDirectory {
        let entries = vec![
            entry(RecordTag::Directory, HEADER_LEN as u64, 30),
            entry(RecordTag::Integrity, 106, 40),
            entry(RecordTag::Trailer, 158, 20),
        ];
        let classes = class_index(&entries);
        SeekDirectory {
            section_flags: SEEK_DIRECTORY_ALL_SECTIONS,
            entries,
            classes,
            channel_lengths: Vec::new(),
        }
    }

    #[test]
    fn roundtrip() {
        let dir = sample_dir();
        let bytes = dir.encode().unwrap();
        let decoded = SeekDirectory::decode(&bytes, Limits::DEFAULT).unwrap();
        assert_eq!(decoded, dir);
    }

    #[test]
    fn encoded_length_is_the_documented_sum() {
        let dir = sample_dir();
        let bytes = dir.encode().unwrap();
        let expected = 2
            + 4
            + DIRECTORY_ENTRY_LEN * dir.entries.len()
            + 1
            + CLASS_ENTRY_LEN * dir.classes.len()
            + 4
            + 8 * dir.channel_lengths.len();
        assert_eq!(bytes.len(), expected);
    }

    #[test]
    fn unknown_version_and_flags_fail_closed() {
        let dir = sample_dir();
        let mut bytes = dir.encode().unwrap();
        bytes[0] = 2;
        assert_eq!(
            SeekDirectory::decode(&bytes, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::UnsupportedVersion
        );

        let mut bytes = dir.encode().unwrap();
        bytes[1] |= 0x80;
        assert_eq!(
            SeekDirectory::decode(&bytes, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidContainer
        );
    }

    #[test]
    fn truncated_section_is_rejected() {
        let dir = sample_dir();
        let bytes = dir.encode().unwrap();
        for cut in 0..bytes.len() {
            // The 2-byte header alone decodes but names sections that are absent.
            if cut < 2 {
                continue;
            }
            let e = SeekDirectory::decode(&bytes[..cut], Limits::DEFAULT);
            assert!(e.is_err(), "cut {cut} was accepted");
        }
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let dir = sample_dir();
        let mut bytes = dir.encode().unwrap();
        bytes.push(0);
        assert_eq!(
            SeekDirectory::decode(&bytes, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidContainer
        );
    }

    #[test]
    fn over_limit_size_is_declined_before_allocation() {
        let dir = sample_dir();
        let bytes = dir.encode().unwrap();
        let tiny = Limits {
            max_directory_bytes: 4,
            ..Limits::DEFAULT
        };
        assert_eq!(
            SeekDirectory::decode(&bytes, tiny).unwrap_err().class(),
            crate::ErrorClass::ResourceLimit
        );
    }

    #[test]
    fn structural_validation_accepts_a_contiguous_directory() {
        let dir = sample_dir();
        // entry ends: dir 64+12+30=106; integrity 106+12+40=158; trailer 158+12+20=190.
        dir.validate_structural(190, Limits::DEFAULT).unwrap();
    }

    #[test]
    fn structural_validation_rejects_bad_geometry() {
        // A non-contiguous locator.
        let mut dir = sample_dir();
        dir.entries[2].offset += 1;
        assert_eq!(
            dir.validate_structural(190, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidContainer
        );

        // The last record does not reach the end of the file.
        let dir = sample_dir();
        assert_eq!(
            dir.validate_structural(191, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidContainer
        );

        // A class index whose count disagrees with a scan.
        let mut dir = sample_dir();
        dir.classes[0].count += 1;
        assert_eq!(
            dir.validate_structural(190, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidContainer
        );
    }

    #[test]
    fn validate_cross_checks_the_record_framing() {
        let dir = sample_dir();
        let sites = vec![
            RecordSite {
                tag: RecordTag::Directory as u8,
                offset: HEADER_LEN as u64,
                payload_len: 30,
            },
            RecordSite {
                tag: RecordTag::Integrity as u8,
                offset: 106,
                payload_len: 40,
            },
            RecordSite {
                tag: RecordTag::Trailer as u8,
                offset: 158,
                payload_len: 20,
            },
        ];
        dir.validate(&sites, 190, Limits::DEFAULT).unwrap();

        // A framing that disagrees with the locator must be rejected.
        let mut wrong = sites.clone();
        wrong[1].payload_len = 41;
        assert_eq!(
            dir.validate(&wrong, 190, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidContainer
        );
    }
}
