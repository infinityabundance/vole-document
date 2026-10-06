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
//!
//! ## Procedural seed nodes (Phase 11)
//!
//! Behind the `field` feature this adapter also implements [`SeedStore`], so a
//! [`crate::field::FieldStore`] can persist its fine-grained procedural seed DAG
//! **one node per engine blob** rather than as one coarse blob.
//!
//! **EntropyFS sees only opaque bytes.** It has no notion of a VOLE seed node, a
//! node kind, a dependency edge, or a materializer; it is a content-addressed
//! blob store. VOLE owns the canonical node format and its interpretation entirely
//! (ADR-0025). We therefore never claim EntropyFS natively understands procedural
//! nodes, and we never claim this substrate is faster — the point is that the DAG
//! is genuinely *persisted* through the engine, one blob per node, with each node
//! independently fetchable and range-readable.
//!
//! Identity uses the engine's own BLAKE3 addressing. A VOLE [`NodeId`] is
//! `BLAKE3-256(SEED_NODE_DOMAIN || canonical_node_bytes)`, so the adapter stores the
//! **domain-prefixed** canonical bytes. The engine's `BlobId` is then exactly the
//! `NodeId`, which makes [`SeedStore::get_node`]/`contains`/range a direct engine
//! lookup and makes the engine's whole-blob BLAKE3 gate the node-identity gate.
//!
//! Like [`ObjectStore::list`], [`SeedStore::list_nodes`] **declines** with
//! `UnsupportedFeature`: the engine exposes no enumeration, so a seed-store
//! mark-and-sweep closure cannot be computed through this substrate. Dependency
//! closure itself (which descends by explicit ids, never by scanning) still works.

use std::fs;
use std::path::{Path, PathBuf};

use entropyfs::engine::{BlobId, Engine, EngineError, EngineOpenOptions};

use crate::error::{Error, Result};
use crate::store::{Id, ObjectStore};
#[cfg(feature = "field")]
use crate::store::{NodeId, SEED_NODE_DOMAIN, SeedStore};

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
pub(crate) fn map_engine_error(context: &str, e: &EngineError) -> Error {
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

// ---------------------------------------------------------------------------
// Procedural seed nodes (Phase 11, `field` feature).
// ---------------------------------------------------------------------------

/// The engine blob bytes for a canonical node: `SEED_NODE_DOMAIN || canonical`.
///
/// Prefixing the domain makes the engine's `BlobId` (BLAKE3 of the blob) equal to
/// the VOLE [`NodeId`] (BLAKE3 of the domain-prefixed canonical bytes), so a node
/// is addressed by its own content id and the engine's whole-blob hash gate is the
/// node-identity gate. EntropyFS is oblivious to the prefix and the payload.
#[cfg(feature = "field")]
fn seed_blob_bytes(canonical: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(SEED_NODE_DOMAIN.len() + canonical.len());
    out.extend_from_slice(SEED_NODE_DOMAIN);
    out.extend_from_slice(canonical);
    out
}

/// Strip the domain prefix from an engine blob, or `None` if it is absent.
#[cfg(feature = "field")]
fn strip_seed_domain(blob: &[u8]) -> Option<&[u8]> {
    blob.strip_prefix(SEED_NODE_DOMAIN)
}

#[cfg(feature = "field")]
impl EntropyFsStore {
    /// Store one canonical seed node as one engine blob (idempotent).
    pub(crate) fn seed_put(&self, canonical: &[u8]) -> Result<NodeId> {
        let id = NodeId::of_node(canonical);
        let blob = self
            .engine
            .put_blob(&seed_blob_bytes(canonical))
            .map_err(|e| map_engine_error("put_blob", &e))?;
        debug_assert_eq!(
            blob.as_bytes(),
            id.as_bytes(),
            "engine BlobId must equal the domain-prefixed NodeId"
        );
        Ok(id)
    }

    /// Fetch one canonical node, verifying the domain prefix and content id.
    pub(crate) fn seed_get(&self, id: &NodeId) -> Result<Vec<u8>> {
        let blob = self
            .engine
            .get_blob(BlobId::new(*id.as_bytes()))
            .map_err(|e| map_engine_error("get_blob", &e))?;
        let canonical = strip_seed_domain(&blob).ok_or_else(|| {
            Error::integrity_mismatch(format!("seed blob {id} is missing its domain prefix"))
        })?;
        let actual = NodeId::of_node(canonical);
        if actual != *id {
            return Err(Error::integrity_mismatch(format!(
                "seed node {id} content hashes to {actual}"
            )));
        }
        Ok(canonical.to_vec())
    }

    /// Fetch `len` canonical node bytes at `offset` (strict; no EOF clip).
    ///
    /// The range is shifted past the domain prefix, so the engine's EOF clip
    /// coincides exactly with the canonical node boundary: an out-of-bounds or
    /// short read returns fewer bytes than requested and is rejected.
    pub(crate) fn seed_get_range(&self, id: &NodeId, offset: u64, len: u64) -> Result<Vec<u8>> {
        let start = offset
            .checked_add(SEED_NODE_DOMAIN.len() as u64)
            .ok_or_else(|| Error::integrity_mismatch("seed node range start overflows"))?;
        let out = self
            .engine
            .read_blob_range(
                BlobId::new(*id.as_bytes()),
                start,
                usize::try_from(len).unwrap_or(usize::MAX),
            )
            .map_err(|e| map_engine_error("read_blob_range", &e))?;
        if out.len() as u64 != len {
            return Err(Error::integrity_mismatch(format!(
                "seed node {id} range [{offset}, {}) is out of bounds (engine returned {} bytes)",
                offset.saturating_add(len),
                out.len()
            )));
        }
        Ok(out)
    }

    /// Whether `id` is present in the engine.
    pub(crate) fn seed_contains(&self, id: &NodeId) -> Result<bool> {
        self.engine
            .contains(BlobId::new(*id.as_bytes()))
            .map_err(|e| map_engine_error("contains", &e))
    }

    /// Enumerate seed nodes. Declines: the engine has no enumeration API.
    pub(crate) fn seed_list(&self) -> Result<Vec<(NodeId, u64)>> {
        Err(Error::unsupported_feature(
            "EntropyFS exposes no per-blob enumeration; seed nodes are one blob each \
             but a mark-and-sweep closure cannot be computed through this substrate",
        ))
    }
}

/// `SeedStore` for the engine: one canonical node per blob.
///
/// EntropyFS is a blob store here, not a procedural-node store: every node is an
/// opaque, domain-prefixed blob and VOLE owns all node semantics.
#[cfg(feature = "field")]
impl SeedStore for EntropyFsStore {
    fn put_node(&mut self, canonical: &[u8]) -> Result<NodeId> {
        self.seed_put(canonical)
    }

    fn get_node(&self, id: &NodeId) -> Result<Vec<u8>> {
        self.seed_get(id)
    }

    fn get_node_range(&self, id: &NodeId, offset: u64, len: u64) -> Result<Vec<u8>> {
        self.seed_get_range(id, offset, len)
    }

    fn contains_node(&self, id: &NodeId) -> Result<bool> {
        self.seed_contains(id)
    }

    fn list_nodes(&self) -> Result<Vec<(NodeId, u64)>> {
        self.seed_list()
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

/// Seed-store behaviour through the engine, one blob per node.
#[cfg(all(test, feature = "field"))]
mod seed_tests {
    use super::*;
    use crate::ErrorClass;
    use std::path::PathBuf;

    fn temp_root(label: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "vole-entropyfs-seed-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        p
    }

    #[test]
    fn put_get_range_roundtrip_through_the_engine() {
        let root = temp_root("rt");
        let mut store = EntropyFsStore::create(&root).unwrap();
        let canonical = b"canonical seed node bytes";
        let id1 = store.put_node(canonical).unwrap();
        let id2 = store.put_node(canonical).unwrap();
        assert_eq!(id1, id2, "put_node must be idempotent");
        assert_eq!(store.get_node(&id1).unwrap(), canonical);
        assert!(store.contains_node(&id1).unwrap());
        assert_eq!(store.get_node_range(&id1, 0, 9).unwrap(), b"canonical");
        // A range past the canonical end is a strict error, not a silent clip.
        let e = store.get_node_range(&id1, 100, 4).unwrap_err();
        assert_eq!(e.class(), ErrorClass::IntegrityMismatch);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn node_ids_are_engine_blob_ids() {
        let root = temp_root("ids");
        let mut store = EntropyFsStore::create(&root).unwrap();
        let canonical = b"one node one blob";
        let id = store.put_node(canonical).unwrap();
        // The domain prefix makes the engine's BlobId exactly the NodeId.
        assert!(
            store
                .engine()
                .contains(BlobId::new(*id.as_bytes()))
                .unwrap()
        );
        // And exactly one blob was written for the one node.
        let m = store.engine().metrics().unwrap();
        assert_eq!(
            m.accounting.blob_count, 1,
            "one canonical node must be one engine blob"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn list_nodes_declines_with_a_typed_error() {
        let root = temp_root("list");
        let mut store = EntropyFsStore::create(&root).unwrap();
        store.put_node(b"a node").unwrap();
        let e = store.list_nodes().unwrap_err();
        assert_eq!(e.class(), ErrorClass::UnsupportedFeature);
        std::fs::remove_dir_all(&root).ok();
    }
}
