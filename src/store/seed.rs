//! The procedural seed store (Phase 11).
//!
//! Phase 9 externalized *coarse* object-table entries and lost to per-file LZ
//! and to content-defined chunking: the granularity was wrong and most
//! representation state stayed embedded. Phase 11 instead persists **fine-
//! grained procedural seed nodes**, one blob per node, keyed by a
//! domain-separated content id.
//!
//! ## Identity
//!
//! A node's id is `BLAKE3-256("VOLE:PSEED:v1" || canonical_node_bytes)`. It is
//! **domain-separated** from the object table's un-prefixed
//! [`crate::store::Id`](crate::store::Id), so a seed node can never collide with,
//! or be mistaken for, a raw object. A node id is a relational/advisory identity:
//! it never appears in the whole-source `INTEGRITY` manifest and is never a
//! substitute for the source SHA-256.
//!
//! ## Backends
//!
//! * [`FsSeedStore`] — the reference substrate: plain files under the store
//!   root, written atomically (`tmp -> rename`), read with `pread`-style range
//!   reads. Range reads carry no whole-node hash gate; only [`SeedStore::get`]
//!   re-hashes.
//! * An `EntropyFsStore` also implements [`SeedStore`], storing one node per
//!   blob through the embeddable engine. Nodes are opaque bytes to EntropyFS; we
//!   never claim EntropyFS natively stores VOLE procedural semantics (ADR-0025).

use core::fmt;
use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

use super::IoCounters;

/// Domain-separation prefix for a procedural seed node's content id.
pub const SEED_NODE_DOMAIN: &[u8] = b"VOLE:PSEED:v1";

/// Canonical seed-node format version.
pub const SEED_FORMAT_VERSION: u8 = 1;

/// `NodeId` is a newtype over the 32 raw bytes of
/// `BLAKE3-256("VOLE:PSEED:v1" || canonical_node_bytes)`. Hex is lower-case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId([u8; 32]);

impl NodeId {
    /// Wrap 32 raw digest bytes.
    pub const fn from_bytes(b: [u8; 32]) -> Self {
        NodeId(b)
    }

    /// The raw digest bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Content id of canonical node bytes: `BLAKE3-256(domain || bytes)`.
    pub fn of_node(canonical: &[u8]) -> Self {
        let mut h = blake3::Hasher::new();
        h.update(SEED_NODE_DOMAIN);
        h.update(canonical);
        NodeId(*h.finalize().as_bytes())
    }

    /// Lower-case hex rendering (64 characters).
    pub fn to_hex(&self) -> String {
        crate::integrity::to_hex(&self.0)
    }

    /// Parse exactly 64 lower- or upper-case hex characters.
    pub fn from_hex(s: &str) -> Result<Self> {
        crate::store::Id::from_hex(s).map(|id| NodeId(*id.as_bytes()))
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// Physical accounting of a seed store.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SeedStoreStats {
    /// Number of stored nodes.
    pub node_count: u64,
    /// Sum of canonical node payload lengths.
    pub total_bytes: u64,
}

/// A content-addressed store of canonical procedural seed nodes.
///
/// `put` takes `&mut self` (ids are content-derived, so retries are idempotent).
pub trait SeedStore {
    /// Store the canonical bytes of one node, returning its content id.
    ///
    /// Idempotent and at-least-once: identical bytes always return the same id
    /// and are stored once. A returned id guarantees the bytes are durable under
    /// [`SeedStore::get_node`].
    fn put_node(&mut self, canonical: &[u8]) -> Result<NodeId>;

    /// Fetch the exact canonical bytes for `id`.
    ///
    /// MUST verify `NodeId::of_node(bytes) == id` before returning.
    fn get_node(&self, id: &NodeId) -> Result<Vec<u8>>;

    /// Fetch `len` bytes of `id` starting at `offset` (strict; no EOF clip).
    fn get_node_range(&self, id: &NodeId, offset: u64, len: u64) -> Result<Vec<u8>>;

    /// Whether `id` is present.
    fn contains_node(&self, id: &NodeId) -> Result<bool>;

    /// Every stored `(id, canonical_len)`, for closure checking and GC.
    fn list_nodes(&self) -> Result<Vec<(NodeId, u64)>>;
}

/// The reference seed substrate: one file per node under `root/seed`.
///
/// Layout: `root/seed/<aa>/<bb>/<64-hex>` where `aa`/`bb` are the first two
/// bytes of the id in hex. Writes are atomic (`tmp -> rename`); reads are
/// hash-verified by [`SeedStore::get_node`] and strict-range by
/// [`SeedStore::get_node_range`].
pub struct FsSeedStore {
    root: PathBuf,
    io: IoCounters,
}

impl std::fmt::Debug for FsSeedStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FsSeedStore")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl FsSeedStore {
    /// Open (creating if needed) a seed store rooted at `root`, with a fresh,
    /// private I/O counter set.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_io(root, IoCounters::new())
    }

    /// Open a seed store that accounts every read against `io`. A caller that
    /// also holds a [`crate::field::FieldStore`] shares its counter handle here,
    /// so a seed read made through either handle is attributed to one universe.
    pub fn open_with_io(root: impl AsRef<Path>, io: IoCounters) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("seed"))?;
        Ok(FsSeedStore { root, io })
    }

    /// The seed-store root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn node_path(&self, id: &NodeId) -> PathBuf {
        let hex = id.to_hex();
        self.root
            .join("seed")
            .join(&hex[0..2])
            .join(&hex[2..4])
            .join(&hex)
    }
}

impl SeedStore for FsSeedStore {
    fn put_node(&mut self, canonical: &[u8]) -> Result<NodeId> {
        let id = NodeId::of_node(canonical);
        let path = self.node_path(&id);
        if path.exists() {
            return Ok(id);
        }
        let dir = path
            .parent()
            .ok_or_else(|| Error::internal_invariant("seed node path has no parent"))?;
        crate::store::durable::create_dir_all(dir)?;
        // Atomic publish: write a sibling tmp file, fsync, rename over the target,
        // then fsync the directory so the rename is durable across a power cut
        // (Phase 23, GAP 1).
        let tmp = dir.join(format!(".{}.tmp-{}", id.to_hex(), std::process::id()));
        {
            let mut f = crate::store::durable::create_file(&tmp)?;
            crate::store::durable::write_all(&mut f, &tmp, canonical)?;
            crate::store::durable::sync_all(&f, &tmp)?;
        }
        crate::store::durable::rename(&tmp, &path)?;
        crate::store::durable::sync_dir(dir)?;
        Ok(id)
    }

    fn get_node(&self, id: &NodeId) -> Result<Vec<u8>> {
        let path = self.node_path(id);
        let bytes = fs::read(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::missing_external_object(format!("seed node {id} is not present"))
            } else {
                Error::io(format!("reading seed node {id}: {e}"))
            }
        })?;
        let actual = NodeId::of_node(&bytes);
        if actual != *id {
            return Err(Error::integrity_mismatch(format!(
                "seed node {id} content hashes to {actual}"
            )));
        }
        self.io.add_seed(bytes.len() as u64);
        Ok(bytes)
    }

    fn get_node_range(&self, id: &NodeId, offset: u64, len: u64) -> Result<Vec<u8>> {
        let path = self.node_path(id);
        let mut f = fs::File::open(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::missing_external_object(format!("seed node {id} is not present"))
            } else {
                Error::io(format!("opening seed node {id}: {e}"))
            }
        })?;
        let file_len = f.metadata()?.len();
        if offset.checked_add(len).is_none_or(|end| end > file_len) {
            return Err(Error::integrity_mismatch(format!(
                "seed node {id} range [{offset}, {}) exceeds stored length {file_len}",
                offset.saturating_add(len)
            )));
        }
        f.seek(SeekFrom::Start(offset))?;
        let mut out = vec![0u8; usize::try_from(len).unwrap_or(usize::MAX)];
        f.read_exact(&mut out)?;
        self.io.add_seed(out.len() as u64);
        Ok(out)
    }

    fn contains_node(&self, id: &NodeId) -> Result<bool> {
        Ok(self.node_path(id).exists())
    }

    fn list_nodes(&self) -> Result<Vec<(NodeId, u64)>> {
        let seed = self.root.join("seed");
        let mut out: Vec<(NodeId, u64)> = Vec::new();
        let mut stack = vec![seed];
        while let Some(dir) = stack.pop() {
            let entries = match fs::read_dir(&dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if name.len() != 64 {
                    continue;
                }
                let Ok(id) = NodeId::from_hex(name) else {
                    continue;
                };
                let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
                out.push((id, len));
            }
        }
        out.sort_unstable();
        Ok(out)
    }
}

/// Collect the transitive closure of `roots` in a seed store, verifying that no
/// dependency is missing. `deps_of` maps a node's canonical bytes to its
/// dependency ids; it must be deterministic.
///
/// Returns the reachable id set in ascending order. A missing dependency is a
/// live `MissingExternalObject` (never a silent partial closure).
pub fn closure(
    store: &dyn SeedStore,
    roots: &[NodeId],
    deps_of: impl Fn(&[u8]) -> Result<Vec<NodeId>>,
) -> Result<BTreeSet<NodeId>> {
    let mut seen: BTreeSet<NodeId> = BTreeSet::new();
    let mut stack: Vec<NodeId> = roots.to_vec();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let bytes = store.get_node(&id).map_err(|_| {
            Error::missing_external_object(format!("seed closure missing node {id}"))
        })?;
        for dep in deps_of(&bytes)? {
            if !seen.contains(&dep) {
                stack.push(dep);
            }
        }
    }
    Ok(seen)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "vole-seed-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn put_get_roundtrip_and_idempotence() {
        let root = temp_root("rt");
        let mut store = FsSeedStore::open(&root).unwrap();
        let bytes = b"canonical node bytes";
        let id1 = store.put_node(bytes).unwrap();
        let id2 = store.put_node(bytes).unwrap();
        assert_eq!(id1, id2);
        assert_eq!(store.get_node(&id1).unwrap(), bytes);
        assert!(store.contains_node(&id1).unwrap());
        assert_eq!(store.list_nodes().unwrap(), vec![(id1, bytes.len() as u64)]);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn range_reads_are_strict() {
        let root = temp_root("range");
        let mut store = FsSeedStore::open(&root).unwrap();
        let bytes = b"0123456789";
        let id = store.put_node(bytes).unwrap();
        assert_eq!(store.get_node_range(&id, 2, 3).unwrap(), b"234");
        let e = store.get_node_range(&id, 8, 5).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::IntegrityMismatch);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn node_ids_are_domain_separated_from_object_ids() {
        let bytes = b"same bytes";
        assert_ne!(
            *NodeId::of_node(bytes).as_bytes(),
            *crate::store::Id::of(bytes).as_bytes()
        );
    }
}
