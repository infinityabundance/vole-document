//! The optional `EntropyFsStore` backend: a thin `ObjectStore` adapter over the
//! embeddable EntropyFS `engine::Engine`.
//!
//! ## Why this is optional and never required
//!
//! Exactness never depends on a store. The standalone `.voldoc` form materializes
//! without any backend, and `EmbeddedStore` is the transparent reference backend
//! used by the measured campaign. This adapter exists so the same object table can
//! also be served by EntropyFS; it is behind the non-default `entropyfs-store`
//! feature because the engine pulls a large dependency tree (a **non-optional**
//! `dsfb`, plus clap/serde/rustix/rand) even with `default-features = false`.
//!
//! ## Identity
//!
//! `entropyfs::engine::BlobId` is `BLAKE3-256(blob_bytes)`, byte-for-byte the same
//! value as [`crate::store::Id`]; [`EntropyFsStore::put`] asserts the equivalence.
//! Equal logical bytes therefore share one store entry across both backends, and a
//! descriptor externalized into an [`EmbeddedStore`](crate::store::EmbeddedStore)
//! resolves unchanged through this adapter.
//!
//! ## Verification and bounds
//!
//! * [`ObjectStore::get`] delegates to `Engine::get_blob`, which applies the full
//!   *whole-blob* BLAKE3 gate (a mismatch is a typed integrity error, never a
//!   silent return).
//! * [`ObjectStore::get_range`] delegates to `Engine::read_blob_range`, which has
//!   `pread` EOF-clipping semantics and no whole-blob gate. Our contract is
//!   **strict**, so the adapter rejects a short read rather than silently clipping.
//! * `list`/`remove` **decline** with [`crate::ErrorClass::UnsupportedFeature`]:
//!   the engine facade exposes no per-blob enumeration or delete, so mark-and-sweep
//!   [`crate::store::gc`] cannot reclaim through this adapter. Reclamation is left
//!   to EntropyFS's own reachability GC and compaction policy; the reachable mark
//!   is still derivable from the roots, but a sweep here would be a lie.

use std::fs;
use std::path::{Path, PathBuf};

use entropyfs::engine::{BlobId, Engine, EngineError, EngineOpenOptions};

use crate::error::{Error, Result};
use crate::store::{Id, ObjectStore};

/// An `ObjectStore` served by an embedded EntropyFS engine.
///
/// Construct with [`EntropyFsStore::create`] (a fresh store) or
/// [`EntropyFsStore::open`] (an existing one). One engine per store: EntropyFS
/// takes an exclusive lock on the store directory.
pub struct EntropyFsStore {
    engine: Engine,
    root: PathBuf,
}

impl std::fmt::Debug for EntropyFsStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntropyFsStore")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl EntropyFsStore {
    /// Create a fresh EntropyFS store rooted at `root` and return an open engine.
    pub fn create(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;
        let engine = Engine::create(&root, &EngineOpenOptions::default())
            .map_err(|e| map_engine_error("create", &e))?;
        Ok(EntropyFsStore { engine, root })
    }

    /// Open an existing EntropyFS store rooted at `root`.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let engine = Engine::open(&root, &EngineOpenOptions::default())
            .map_err(|e| map_engine_error("open", &e))?;
        Ok(EntropyFsStore { engine, root })
    }

    /// The store root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The wrapped engine (for durability/compaction lifecycle operations).
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Make every acknowledged `put` power-durable.
    pub fn sync(&self) -> Result<()> {
        self.engine.sync().map_err(|e| map_engine_error("sync", &e))
    }
}

impl ObjectStore for EntropyFsStore {
    fn put(&mut self, bytes: &[u8]) -> Result<Id> {
        let blob = self
            .engine
            .put_blob(bytes)
            .map_err(|e| map_engine_error("put_blob", &e))?;
        let id = Id::from_bytes(*blob.as_bytes());
        // EntropyFS's BlobId is BLAKE3-256 of the blob bytes; so is our Id.
        debug_assert_eq!(
            id,
            Id::of(bytes),
            "EntropyFS BlobId must equal Id::of(bytes)"
        );
        Ok(id)
    }

    fn get(&self, id: &Id) -> Result<Vec<u8>> {
        self.engine
            .get_blob(blob_id(id))
            .map_err(|e| map_engine_error("get_blob", &e))
    }

    fn get_range(&self, id: &Id, offset: u64, len: u64) -> Result<Vec<u8>> {
        let out = self
            .engine
            .read_blob_range(
                blob_id(id),
                offset,
                usize::try_from(len).unwrap_or(usize::MAX),
            )
            .map_err(|e| map_engine_error("read_blob_range", &e))?;
        // The engine clips at EOF (pread semantics); our contract is strict.
        if out.len() as u64 != len {
            return Err(Error::integrity_mismatch(format!(
                "object {id} range [{offset}, {}) is out of bounds (engine returned {} bytes)",
                offset.saturating_add(len),
                out.len()
            )));
        }
        Ok(out)
    }

    fn contains(&self, id: &Id) -> Result<bool> {
        self.engine
            .contains(blob_id(id))
            .map_err(|e| map_engine_error("contains", &e))
    }

    fn list(&self) -> Result<Vec<(Id, u64)>> {
        Err(Error::unsupported_feature(
            "EntropyFS exposes no per-blob enumeration; a mark-and-sweep closure \
             cannot be computed through this adapter",
        ))
    }

    fn remove(&self, _id: &Id) -> Result<u64> {
        Err(Error::unsupported_feature(
            "EntropyFS exposes no per-blob delete; GC cannot reclaim through this \
             adapter (reclamation is EntropyFS's own reachability-GC policy)",
        ))
    }
}

/// Wrap our [`Id`] as the engine's `BlobId` (identical 32 bytes).
fn blob_id(id: &Id) -> BlobId {
    BlobId::new(*id.as_bytes())
}

/// Map an `EngineError` onto the crate's typed error classes.
///
/// `message` is never parsed by programs; the stable class is `code`.
fn map_engine_error(context: &str, e: &EngineError) -> Error {
    use entropyfs::engine::ErrorCode;
    let message = format!("entropyfs {context}: {e}");
    match e.code {
        ErrorCode::NotFound => Error::missing_external_object(message),
        ErrorCode::CorruptStore => Error::integrity_mismatch(message),
        ErrorCode::InvalidArgument => Error::usage(message),
        ErrorCode::ResourceLimit => Error::resource_limit(message),
        ErrorCode::Unsupported => Error::unsupported_feature(message),
        ErrorCode::IncompatibleFormat => Error::unsupported_version(message),
        ErrorCode::Busy => Error::io(message),
        ErrorCode::Internal => Error::internal_invariant(message),
        ErrorCode::Closed => Error::internal_invariant(message),
        _ => Error::io(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `BlobId` and `Id` are the same value for the same bytes.
    #[test]
    fn blob_id_matches_id() {
        let bytes = b"identity equivalence";
        assert_eq!(
            *blob_id(&Id::of(bytes)).as_bytes(),
            *Id::of(bytes).as_bytes()
        );
    }
}
