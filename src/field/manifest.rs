//! The field manifest: a small, canonical, content-addressed root (Phase 11).
//!
//! A [`FieldRoot`] ties together the exact archival authority (the serialized
//! `.voldoc` descriptor), the procedural seed DAG root, and the hierarchical
//! observation index root. It is stored as one blob in the seed store and its id
//! is the handle a caller uses to open the field.

use crate::error::{Error, Result};
use crate::integrity::to_hex;
use crate::store::{Id, NodeId};

/// Domain-separation prefix for a field root id.
pub const FIELD_ROOT_DOMAIN: &[u8] = b"VOLE:VFIELD:v1";
/// Canonical field-manifest format version.
pub const FIELD_FORMAT_VERSION: u8 = 1;
/// Magic bytes at the head of a canonical manifest.
pub const FIELD_MAGIC: &[u8; 8] = b"VOLDFLD1";

/// All-zero id used for an absent optional root (e.g. no index).
pub const ABSENT_ROOT: [u8; 32] = [0u8; 32];

/// A content-addressed handle to a field manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FieldId([u8; 32]);

impl FieldId {
    /// Wrap 32 raw digest bytes.
    pub const fn from_bytes(b: [u8; 32]) -> Self {
        FieldId(b)
    }
    /// Raw digest bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
    /// Content id of canonical manifest bytes.
    pub fn of_manifest(canonical: &[u8]) -> Self {
        let mut h = blake3::Hasher::new();
        h.update(FIELD_ROOT_DOMAIN);
        h.update(canonical);
        FieldId(*h.finalize().as_bytes())
    }
    /// Lower-case hex.
    pub fn to_hex(&self) -> String {
        to_hex(&self.0)
    }
    /// Parse 64 hex chars.
    pub fn from_hex(s: &str) -> Result<Self> {
        Id::from_hex(s).map(|id| FieldId(*id.as_bytes()))
    }
}

impl core::fmt::Display for FieldId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// The field manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldRoot {
    /// Universe declaration this field was produced under.
    pub universe_id: [u8; 16],
    /// SHA-256 of the exact reconstructed source.
    pub source_sha256: [u8; 32],
    /// Exact reconstructed source length.
    pub source_len: u64,
    /// Content id (`Id`) of the serialized `.voldoc` descriptor blob.
    pub descriptor_id: [u8; 32],
    /// Root seed node (exact document closure).
    pub root_node: NodeId,
    /// Hierarchical observation index root, or all-zero when absent.
    pub index_root: [u8; 32],
    /// Number of seed nodes in the closure of `root_node` at ingest time.
    pub node_count: u64,
    /// Number of index nodes.
    pub index_node_count: u64,
    /// Human-readable basis/provenance (advisory).
    pub provenance: String,
}

impl FieldRoot {
    /// Canonical encoding (never includes the field id).
    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(256);
        out.extend_from_slice(FIELD_MAGIC);
        out.push(FIELD_FORMAT_VERSION);
        out.push(0); // reserved
        out.extend_from_slice(&self.universe_id);
        out.extend_from_slice(&self.source_sha256);
        out.extend_from_slice(&self.source_len.to_le_bytes());
        out.extend_from_slice(&self.descriptor_id);
        out.extend_from_slice(self.root_node.as_bytes());
        out.extend_from_slice(&self.index_root);
        out.extend_from_slice(&self.node_count.to_le_bytes());
        out.extend_from_slice(&self.index_node_count.to_le_bytes());
        let prov = self.provenance.as_bytes();
        out.extend_from_slice(&(prov.len() as u32).to_le_bytes());
        out.extend_from_slice(prov);
        out
    }

    /// Parse a canonical manifest.
    pub fn decode_canonical(bytes: &[u8]) -> Result<FieldRoot> {
        if bytes.len() < 8 || &bytes[0..8] != FIELD_MAGIC {
            return Err(Error::unsupported_version("field manifest: bad magic"));
        }
        // Fixed prefix: magic(8) ver(1) res(1) universe(16) sha(32) len(8)
        //               descriptor(32) root(32) index(32) nodes(8) inodes(8) = 178
        const FIXED: usize = 8 + 1 + 1 + 16 + 32 + 8 + 32 + 32 + 32 + 8 + 8;
        if bytes.len() < FIXED + 4 {
            return Err(Error::usage("truncated field manifest"));
        }
        if bytes[8] != FIELD_FORMAT_VERSION {
            return Err(Error::unsupported_version(format!(
                "field manifest version {} is not supported",
                bytes[8]
            )));
        }
        let mut at = 10;
        let mut universe_id = [0u8; 16];
        universe_id.copy_from_slice(&bytes[at..at + 16]);
        at += 16;
        let mut source_sha256 = [0u8; 32];
        source_sha256.copy_from_slice(&bytes[at..at + 32]);
        at += 32;
        let source_len = u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
        at += 8;
        let mut descriptor_id = [0u8; 32];
        descriptor_id.copy_from_slice(&bytes[at..at + 32]);
        at += 32;
        let mut root = [0u8; 32];
        root.copy_from_slice(&bytes[at..at + 32]);
        at += 32;
        let mut index_root = [0u8; 32];
        index_root.copy_from_slice(&bytes[at..at + 32]);
        at += 32;
        let node_count = u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
        at += 8;
        let index_node_count = u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
        at += 8;
        let prov_len = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        at += 4;
        let end = at
            .checked_add(prov_len)
            .ok_or_else(|| Error::usage("field manifest provenance overflow"))?;
        if end != bytes.len() {
            return Err(Error::usage(
                "field manifest provenance length does not match the buffer",
            ));
        }
        let provenance = core::str::from_utf8(&bytes[at..end])
            .map_err(|_| Error::usage("field manifest provenance is not UTF-8"))?
            .to_string();
        Ok(FieldRoot {
            universe_id,
            source_sha256,
            source_len,
            descriptor_id,
            root_node: NodeId::from_bytes(root),
            index_root,
            node_count,
            index_node_count,
            provenance,
        })
    }

    /// Content id of this manifest.
    pub fn content_id(&self) -> FieldId {
        FieldId::of_manifest(&self.encode_canonical())
    }

    /// Whether the manifest declares an index.
    pub fn has_index(&self) -> bool {
        self.index_root != ABSENT_ROOT
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrity::sha256;

    fn sample() -> FieldRoot {
        FieldRoot {
            universe_id: [3u8; 16],
            source_sha256: sha256(b"source"),
            source_len: 6,
            descriptor_id: [4u8; 32],
            root_node: NodeId::from_bytes([5u8; 32]),
            index_root: ABSENT_ROOT,
            node_count: 12,
            index_node_count: 0,
            provenance: "pdf:test".into(),
        }
    }

    #[test]
    fn manifest_roundtrips_and_ids() {
        let m = sample();
        let enc = m.encode_canonical();
        let back = FieldRoot::decode_canonical(&enc).unwrap();
        assert_eq!(m, back);
        assert_eq!(m.content_id(), back.content_id());
        assert_eq!(m.encode_canonical(), enc);
        assert!(!m.has_index());
    }

    #[test]
    fn bad_magic_and_version_fail_closed() {
        let mut enc = sample().encode_canonical();
        enc[0] = b'X';
        assert!(FieldRoot::decode_canonical(&enc).is_err());
        let mut enc2 = sample().encode_canonical();
        enc2[8] = 0x7F;
        assert!(FieldRoot::decode_canonical(&enc2).is_err());
    }

    #[test]
    fn truncation_and_trailing_fail_closed() {
        let enc = sample().encode_canonical();
        assert!(FieldRoot::decode_canonical(&enc[..enc.len() - 1]).is_err());
        let mut longer = enc.clone();
        longer.push(0);
        assert!(FieldRoot::decode_canonical(&longer).is_err());
    }
}
