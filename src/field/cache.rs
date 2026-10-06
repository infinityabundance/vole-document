//! Disposable, closure-keyed derived observation cache (Phase 11.8).
//!
//! A seed node's output is a pure function of its canonical bytes, hence of its
//! [`NodeId`]. A cache keyed by `NodeId` is therefore automatically
//! **closure-keyed**: a changed dependency yields a different dependency id,
//! hence a different node id, hence a miss; an unchanged closure yields a hit.
//! This is the exact analogue of rustc's red-green validation, and it needs no
//! invalidation pass (ADR-0025, ADR-0027).
//!
//! The cache is **disposable** and never normative. It is off the exactness path
//! and can never influence `materialize`. Because [`DerivedCache::get`] only has
//! the output bytes (it cannot re-derive the node), every entry carries a
//! **sidecar integrity digest**: a mismatch fails closed with
//! [`crate::ErrorClass::IntegrityMismatch`] rather than returning wrong bytes.
//! The DAG layer treats any cache error as a miss, so a corrupt or missing entry
//! simply falls back to recomputation.
//!
//! ## On-disk layout
//!
//! ```text
//! <root>/<64-hex>        output bytes, verbatim
//! <root>/<64-hex>.b3     BLAKE3-256 sidecar digest of those bytes
//! ```
//!
//! Writes are atomic (`tmp -> fsync -> rename`); a torn write cannot publish a
//! half-entry. Cache bytes are a fourth accounting universe and are never folded
//! into the descriptor or store universes (ADR-0027).

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::store::NodeId;

use super::dag::OutputCache;
use super::write_atomic;

/// A disposable, closure-keyed derived observation cache rooted under a field
/// store's `cache/` directory.
pub struct DerivedCache {
    root: PathBuf,
}

impl std::fmt::Debug for DerivedCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DerivedCache")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl DerivedCache {
    /// Open (creating if needed) a derived cache rooted at `root`.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;
        Ok(DerivedCache { root })
    }

    /// The cache root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn bytes_path(&self, id: &NodeId) -> PathBuf {
        self.root.join(id.to_hex())
    }

    fn digest_path(&self, id: &NodeId) -> PathBuf {
        self.root.join(format!("{}.b3", id.to_hex()))
    }

    /// Fetch a cached output, verifying the sidecar digest.
    ///
    /// * absent bytes or absent sidecar -> `Ok(None)` (a miss; disposable).
    /// * sidecar present but `BLAKE3(bytes) != sidecar` ->
    ///   [`crate::ErrorClass::IntegrityMismatch`] (fail closed; never return
    ///   wrong bytes).
    pub fn get(&self, id: &NodeId) -> Result<Option<Vec<u8>>> {
        let bytes = match fs::read(self.bytes_path(id)) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(Error::io(format!("reading cache entry {id}: {e}"))),
        };
        let sidecar = match fs::read(self.digest_path(id)) {
            Ok(b) => b,
            // A missing sidecar is an incomplete (hence disposable) entry.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(Error::io(format!("reading cache sidecar {id}: {e}"))),
        };
        if sidecar.as_slice() != blake3::hash(&bytes).as_bytes() {
            return Err(Error::integrity_mismatch(format!(
                "cache entry {id} does not match its sidecar digest"
            )));
        }
        Ok(Some(bytes))
    }

    /// Store an output atomically with its sidecar digest. Returns the number of
    /// output bytes written.
    pub fn put(&mut self, id: &NodeId, bytes: &[u8]) -> Result<u64> {
        write_atomic(&self.bytes_path(id), bytes)?;
        write_atomic(&self.digest_path(id), blake3::hash(bytes).as_bytes())?;
        Ok(bytes.len() as u64)
    }

    /// Whether an output file exists for `id` (the sidecar is not checked).
    pub fn contains(&self, id: &NodeId) -> Result<bool> {
        Ok(self.bytes_path(id).exists())
    }

    /// Total physical cache bytes (outputs plus sidecars), excluding temp files.
    pub fn total_bytes(&self) -> Result<u64> {
        let mut total = 0u64;
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let meta = entry.metadata()?;
            if meta.is_file() {
                total = total.saturating_add(meta.len());
            }
        }
        Ok(total)
    }

    /// Remove every cache entry, returning the bytes reclaimed. Idempotent.
    pub fn clear(&self) -> Result<u64> {
        let mut reclaimed = 0u64;
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_file() {
                if !entry.file_name().to_string_lossy().starts_with('.') {
                    reclaimed = reclaimed.saturating_add(entry.metadata()?.len());
                }
                fs::remove_file(&path)?;
            }
        }
        Ok(reclaimed)
    }
}

impl OutputCache for DerivedCache {
    fn get(&self, id: &NodeId) -> Result<Option<Vec<u8>>> {
        DerivedCache::get(self, id)
    }

    fn put(&mut self, id: &NodeId, bytes: &[u8]) -> Result<()> {
        DerivedCache::put(self, id, bytes).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "vole-cache-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        p
    }

    #[test]
    fn put_get_roundtrip_and_accounting() {
        let root = temp_root("rt");
        let mut cache = DerivedCache::open(&root).unwrap();
        let id = NodeId::from_bytes([7u8; 32]);
        assert!(!cache.contains(&id).unwrap());
        assert_eq!(cache.get(&id).unwrap(), None);
        assert_eq!(cache.put(&id, b"derived output").unwrap(), 14);
        assert!(cache.contains(&id).unwrap());
        assert_eq!(
            cache.get(&id).unwrap().as_deref(),
            Some(&b"derived output"[..])
        );
        // Physical bytes = output (14) + 32-byte sidecar.
        assert_eq!(cache.total_bytes().unwrap(), 14 + 32);
        let reclaimed = cache.clear().unwrap();
        assert_eq!(reclaimed, 14 + 32);
        assert_eq!(cache.total_bytes().unwrap(), 0);
        assert_eq!(cache.get(&id).unwrap(), None);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn poisoned_bytes_fail_closed_never_wrong() {
        let root = temp_root("poison");
        let mut cache = DerivedCache::open(&root).unwrap();
        let id = NodeId::from_bytes([3u8; 32]);
        cache.put(&id, b"correct bytes").unwrap();
        // Corrupt the output but keep the sidecar: get must fail closed.
        fs::write(cache.bytes_path(&id), b"wrong!!").unwrap();
        let err = cache.get(&id).unwrap_err();
        assert_eq!(err.class(), crate::ErrorClass::IntegrityMismatch);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn poisoned_sidecar_fail_closed_and_missing_sidecar_is_a_miss() {
        let root = temp_root("sidecar");
        let mut cache = DerivedCache::open(&root).unwrap();
        let id = NodeId::from_bytes([9u8; 32]);
        cache.put(&id, b"payload").unwrap();
        fs::write(cache.digest_path(&id), [0u8; 32]).unwrap();
        assert_eq!(
            cache.get(&id).unwrap_err().class(),
            crate::ErrorClass::IntegrityMismatch
        );
        // Dropping the sidecar makes the entry disposable, not fatal.
        fs::remove_file(cache.digest_path(&id)).unwrap();
        assert_eq!(cache.get(&id).unwrap(), None);
        fs::remove_dir_all(&root).ok();
    }
}
