//! The persistent procedural document field (Phase 11).
//!
//! A [`Field`] is the queryable, content-addressed procedural substrate behind a
//! document. It is opened from a [`FieldStore`] by a [`FieldId`] and can:
//!
//! * materialize the exact original source bytes ([`Field::materialize_exact`]),
//!   exactly as the standalone `.voldoc` form does, and
//! * serve derived/structured observations from the persisted seed DAG without
//!   reconstructing the whole document.
//!
//! The exact archival authority is the serialized `.voldoc` descriptor, stored as
//! one blob; the seed DAG and the hierarchical index are additional persisted
//! state. A field never weakens `materialize(root) == original_bytes` (ADR-0024).

pub mod cache;
pub mod dag;
pub mod derive;
pub mod edit;
pub mod explain;
pub mod index;
pub mod ingest;
#[cfg(feature = "package")]
pub mod ingest_package;
pub mod manifest;
pub mod node;
pub mod observe;
pub mod partial;
pub mod plan;
pub mod provenance;
pub mod share;

pub use manifest::{FieldId, FieldRoot};

use std::fs;
use std::path::{Path, PathBuf};

#[cfg(feature = "entropyfs-store")]
use std::sync::Arc;

#[cfg(feature = "entropyfs-store")]
use entropyfs::engine::BlobId;

use crate::container::ParsedDescriptor;
use crate::error::{Error, Result};
use crate::limits::Limits;
#[cfg(feature = "entropyfs-store")]
use crate::store::{EntropyFsStore, map_engine_error};
use crate::store::{FsSeedStore, Id, IoCounters, IoSnapshot, NodeId, SeedStore};

#[cfg(feature = "entropyfs-store")]
use self::manifest::FIELD_ROOT_DOMAIN;
use self::node::{NodeKind, SeedNode};

/// The canonical universe string for a Phase-11 field.
pub const FIELD_UNIVERSE: &str = "vole-document;universe;phase11;exact-bytes;dra-8;opaque+entropy+pdf+channels+offsets+packed+packed-channels+deflate-replay-preflate-0.7.6-experimental+observation-index-v1+seek-directory-v1+external-objects-v1+procedural-seed-field-v1+hier-index-v1";

/// The canonical universe string for a Phase-12 **package** (ZIP/OCF/OPC) field.
///
/// It is the Phase-11 universe plus an explicit `package-v1` marker. The marker is
/// recorded in the manifest's `universe_id`; it is **not** a new wire record and
/// does not change the exact `.voldoc` descriptor, which remains the ordinary
/// exact form.
#[cfg(feature = "package")]
pub const PACKAGE_UNIVERSE: &str = "vole-document;universe;phase11;exact-bytes;dra-8;opaque+entropy+pdf+channels+offsets+packed+packed-channels+deflate-replay-preflate-0.7.6-experimental+observation-index-v1+seek-directory-v1+external-objects-v1+procedural-seed-field-v1+hier-index-v1+package-v1";

/// A cheaply cloneable handle to a field's seed substrate.
///
/// Cloning shares the *same* underlying substrate and I/O counters, so an
/// observation can mint a detached seed handle without borrowing the
/// [`FieldStore`] that owns it (the evaluation core holds `&mut FieldStore`).
/// Every variant owns a reference-counted engine or a path — never a second lock:
/// an `entropyfs-store` field uses exactly **one** EntropyFS engine for its
/// descriptors, manifests, and seed nodes, so there is no per-observation reopen.
#[derive(Clone)]
pub(crate) enum SeedSubstrate {
    /// Plain files under `<root>/seed` ([`FsSeedStore`]).
    Fs { root: PathBuf, io: IoCounters },
    /// One engine blob per node, through the store's shared EntropyFS engine.
    #[cfg(feature = "entropyfs-store")]
    EntropyFs {
        store: Arc<EntropyFsStore>,
        io: IoCounters,
    },
}

impl SeedStore for SeedSubstrate {
    fn put_node(&mut self, canonical: &[u8]) -> Result<NodeId> {
        match self {
            SeedSubstrate::Fs { root, io } => {
                FsSeedStore::open_with_io(root, io.handle())?.put_node(canonical)
            }
            #[cfg(feature = "entropyfs-store")]
            SeedSubstrate::EntropyFs { store, .. } => store.seed_put(canonical),
        }
    }

    fn get_node(&self, id: &NodeId) -> Result<Vec<u8>> {
        match self {
            SeedSubstrate::Fs { root, io } => {
                FsSeedStore::open_with_io(root, io.handle())?.get_node(id)
            }
            #[cfg(feature = "entropyfs-store")]
            SeedSubstrate::EntropyFs { store, io } => {
                let bytes = store.seed_get(id)?;
                io.add_seed(bytes.len() as u64);
                Ok(bytes)
            }
        }
    }

    fn get_node_range(&self, id: &NodeId, offset: u64, len: u64) -> Result<Vec<u8>> {
        match self {
            SeedSubstrate::Fs { root, io } => {
                FsSeedStore::open_with_io(root, io.handle())?.get_node_range(id, offset, len)
            }
            #[cfg(feature = "entropyfs-store")]
            SeedSubstrate::EntropyFs { store, io } => {
                let bytes = store.seed_get_range(id, offset, len)?;
                io.add_seed(bytes.len() as u64);
                Ok(bytes)
            }
        }
    }

    fn contains_node(&self, id: &NodeId) -> Result<bool> {
        match self {
            SeedSubstrate::Fs { root, io } => {
                FsSeedStore::open_with_io(root, io.handle())?.contains_node(id)
            }
            #[cfg(feature = "entropyfs-store")]
            SeedSubstrate::EntropyFs { store, .. } => store.seed_contains(id),
        }
    }

    fn list_nodes(&self) -> Result<Vec<(NodeId, u64)>> {
        match self {
            SeedSubstrate::Fs { root, io } => {
                FsSeedStore::open_with_io(root, io.handle())?.list_nodes()
            }
            #[cfg(feature = "entropyfs-store")]
            SeedSubstrate::EntropyFs { store, .. } => store.seed_list(),
        }
    }
}

/// Where a field's descriptor and manifest blobs live.
///
/// The `FieldStore` API never leaks which backend is in use: both store and fetch
/// by content id (`Id` for descriptors, `FieldId` for manifests). Only [`stats`]
/// distinguish them.
///
/// [`stats`]: crate::field::observe::ObserveStats
enum BlobBackend {
    /// Plain files: `descriptor/` and `field/` under the store root.
    Fs,
    /// One engine: a descriptor is stored raw (so the engine's `BlobId` equals the
    /// descriptor `Id`), a manifest is stored domain-prefixed with
    /// [`FIELD_ROOT_DOMAIN`] (so the engine's `BlobId` equals the manifest
    /// `FieldId`). Both namespaces are content-addressed and cannot collide with a
    /// seed node, whose bytes carry a different domain prefix.
    #[cfg(feature = "entropyfs-store")]
    EntropyFs(Arc<EntropyFsStore>),
}

impl BlobBackend {
    fn descriptor_put(&self, root: &Path, bytes: &[u8]) -> Result<Id> {
        match self {
            BlobBackend::Fs => {
                let id = Id::of(bytes);
                let path = root.join("descriptor").join(id.to_hex());
                if !path.exists() {
                    write_atomic(&path, bytes)?;
                }
                Ok(id)
            }
            #[cfg(feature = "entropyfs-store")]
            BlobBackend::EntropyFs(store) => {
                let id = Id::of(bytes);
                let blob = store
                    .engine()
                    .put_blob(bytes)
                    .map_err(|e| map_engine_error("put_blob", &e))?;
                debug_assert_eq!(blob.as_bytes(), id.as_bytes());
                Ok(id)
            }
        }
    }

    fn descriptor_get(&self, root: &Path, id: &Id) -> Result<Vec<u8>> {
        match self {
            BlobBackend::Fs => {
                let path = root.join("descriptor").join(id.to_hex());
                fs::read(&path).map_err(|e| {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        Error::missing_external_object(format!(
                            "descriptor blob {id} is not present"
                        ))
                    } else {
                        Error::io(format!("reading descriptor blob {id}: {e}"))
                    }
                })
            }
            #[cfg(feature = "entropyfs-store")]
            BlobBackend::EntropyFs(store) => store
                .engine()
                .get_blob(BlobId::new(*id.as_bytes()))
                .map_err(|e| map_engine_error("get_blob", &e)),
        }
    }

    fn field_put(&self, root: &Path, manifest: &FieldRoot) -> Result<FieldId> {
        let bytes = manifest.encode_canonical();
        let id = FieldId::of_manifest(&bytes);
        match self {
            BlobBackend::Fs => {
                let path = root.join("field").join(id.to_hex());
                if !path.exists() {
                    write_atomic(&path, &bytes)?;
                }
                Ok(id)
            }
            #[cfg(feature = "entropyfs-store")]
            BlobBackend::EntropyFs(store) => {
                let mut prefixed = Vec::with_capacity(FIELD_ROOT_DOMAIN.len() + bytes.len());
                prefixed.extend_from_slice(FIELD_ROOT_DOMAIN);
                prefixed.extend_from_slice(&bytes);
                let blob = store
                    .engine()
                    .put_blob(&prefixed)
                    .map_err(|e| map_engine_error("put_blob", &e))?;
                debug_assert_eq!(blob.as_bytes(), id.as_bytes());
                Ok(id)
            }
        }
    }

    fn field_get(&self, root: &Path, id: &FieldId) -> Result<Vec<u8>> {
        match self {
            BlobBackend::Fs => {
                let path = root.join("field").join(id.to_hex());
                fs::read(&path).map_err(|e| {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        Error::missing_external_object(format!("field {id} is not present"))
                    } else {
                        Error::io(format!("reading field {id}: {e}"))
                    }
                })
            }
            #[cfg(feature = "entropyfs-store")]
            BlobBackend::EntropyFs(store) => {
                let blob = store
                    .engine()
                    .get_blob(BlobId::new(*id.as_bytes()))
                    .map_err(|e| map_engine_error("get_blob", &e))?;
                let rest = blob.strip_prefix(FIELD_ROOT_DOMAIN).ok_or_else(|| {
                    Error::integrity_mismatch(format!(
                        "field manifest {id} is missing its domain prefix"
                    ))
                })?;
                Ok(rest.to_vec())
            }
        }
    }
}

/// On-disk layout of a field store. Every namespace is content-addressed.
///
/// ```text
/// <root>/
///   descriptor/<64-hex>       serialized .voldoc descriptor blobs
///   field/<64-hex>            canonical field manifests
///   seed/<aa>/<bb>/<64-hex>   canonical seed nodes (FsSeedStore)
///   index/<64-hex>            hierarchical index nodes (11.3)
///   cache/                    derived observation cache (11.8)
/// ```
///
/// An EntropyFS-backed store ([`FieldStore::open_entropyfs`], feature
/// `entropyfs-store`) keeps the descriptor, manifest, and seed namespaces in
/// `entropyfs/` (one engine blob each) and `index/`/`cache/` as files. The seed
/// DAG is therefore persisted **one node per engine blob**, never as one coarse
/// blob; EntropyFS sees only opaque bytes and VOLE owns node semantics (ADR-0025).
/// Advisory engine accounting for an EntropyFS-backed field store.
///
/// These are EntropyFS's own numbers, not VOLE's; they are reported so a court can
/// witness that the seed DAG is many individual engine blobs rather than one. They
/// carry **no** decoder authority and no exactness meaning.
#[cfg(feature = "entropyfs-store")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineStoreStats {
    /// Files in the engine blob namespace (one per descriptor / manifest / node).
    pub blob_count: u64,
    /// Sum of materialized logical bytes across reachable inodes.
    pub logical_bytes: u64,
    /// Sum of segment-file lengths (physical store bytes).
    pub physical_used_bytes: u64,
}

pub struct FieldStore {
    root: PathBuf,
    backend: BlobBackend,
    seeds: SeedSubstrate,
    io: IoCounters,
}

impl std::fmt::Debug for FieldStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FieldStore")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl FieldStore {
    /// Create or open a field store rooted at `root` (the filesystem backend).
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("descriptor"))?;
        fs::create_dir_all(root.join("field"))?;
        fs::create_dir_all(root.join("index"))?;
        fs::create_dir_all(root.join("cache"))?;
        let io = IoCounters::new();
        let seeds = SeedSubstrate::Fs {
            root: root.clone(),
            io: io.handle(),
        };
        Ok(FieldStore {
            root,
            backend: BlobBackend::Fs,
            seeds,
            io,
        })
    }

    /// Create or open a field store whose descriptor, manifest, and seed
    /// namespaces are served by one embedded EntropyFS engine at
    /// `root/entropyfs` (feature `entropyfs-store`).
    ///
    /// The hierarchical observation index and the disposable derived cache remain
    /// files under `<root>/index` and `<root>/cache`. Exactly one engine is opened;
    /// the seed substrate and every observation share it by reference count, so a
    /// field never opens a second engine over the same directory (which would
    /// deadlock on the engine's exclusive lock).
    #[cfg(feature = "entropyfs-store")]
    pub fn open_entropyfs(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("index"))?;
        fs::create_dir_all(root.join("cache"))?;
        let engine_root = root.join("entropyfs");
        fs::create_dir_all(&engine_root)?;
        // An empty engine directory is a fresh store; anything else is an
        // existing one. (`Engine::open` cannot open a directory with no store.)
        let fresh = fs::read_dir(&engine_root)?.next().is_none();
        let engine = if fresh {
            EntropyFsStore::create(&engine_root)?
        } else {
            EntropyFsStore::open(&engine_root)?
        };
        let engine = Arc::new(engine);
        let io = IoCounters::new();
        let seeds = SeedSubstrate::EntropyFs {
            store: Arc::clone(&engine),
            io: io.handle(),
        };
        Ok(FieldStore {
            root,
            backend: BlobBackend::EntropyFs(engine),
            seeds,
            io,
        })
    }

    /// The physical-I/O counters shared by this store and its seed substrate.
    ///
    /// Every descriptor/manifest/index/seed read made through any handle derived
    /// from this store is attributed here; an observation snapshots it before and
    /// after to report its own physical bytes (review fix #1).
    pub fn io(&self) -> &IoCounters {
        &self.io
    }

    /// The store root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// A detached, cheaply cloned seed handle that shares this store's substrate
    /// and I/O counters (used by observations).
    pub(crate) fn seed_substrate(&self) -> SeedSubstrate {
        self.seeds.clone()
    }

    /// Whether the descriptor blob is a filesystem file supporting seek-based
    /// partial reads. The EntropyFS backend stores it as an engine blob, so the
    /// partial lane is unavailable and observations read the full descriptor.
    pub(crate) fn supports_partial_descriptor(&self) -> bool {
        matches!(self.backend, BlobBackend::Fs)
    }

    /// The seed store (for advanced callers and courts).
    pub fn seeds(&self) -> &dyn SeedStore {
        &self.seeds
    }

    /// Mutable seed store access.
    pub fn seeds_mut(&mut self) -> &mut dyn SeedStore {
        &mut self.seeds
    }

    /// Open the disposable derived observation cache (11.8) under this store.
    ///
    /// The cache is never normative: it can always be deleted and observations
    /// remain correct by recomputation (ADR-0027).
    pub fn cache(&self) -> Result<cache::DerivedCache> {
        cache::DerivedCache::open(self.root.join("cache"))
    }

    /// Make every acknowledged write power-durable before the handle is dropped.
    ///
    /// The EntropyFS engine acks a `put_blob` at rename but does not barrier, so a
    /// fresh open in another process can miss an unpublishied epoch. Calling this
    /// after a mutation closes that gap. The filesystem backend writes atomically
    /// (`tmp -> fsync -> rename`) and needs no barrier.
    pub fn sync(&self) -> Result<()> {
        match &self.backend {
            BlobBackend::Fs => Ok(()),
            #[cfg(feature = "entropyfs-store")]
            BlobBackend::EntropyFs(store) => store.sync(),
        }
    }

    /// Advisory engine accounting for an EntropyFS-backed store, or `None` for
    /// the filesystem backend.
    ///
    /// `blob_count` is the number of files in the engine's blob namespace: every
    /// descriptor, manifest, and seed node is exactly one blob, so a seed DAG of
    /// `n` nodes adds `n` blobs (never one coarse blob). This is a snapshot, not a
    /// claim that the engine understands procedural state (ADR-0025).
    #[cfg(feature = "entropyfs-store")]
    pub fn engine_stats(&self) -> Result<Option<EngineStoreStats>> {
        match &self.backend {
            BlobBackend::Fs => Ok(None),
            BlobBackend::EntropyFs(store) => {
                let m = store
                    .engine()
                    .metrics()
                    .map_err(|e| map_engine_error("metrics", &e))?;
                Ok(Some(EngineStoreStats {
                    blob_count: m.accounting.blob_count,
                    logical_bytes: m.accounting.logical_bytes,
                    physical_used_bytes: m.accounting.physical_used_bytes,
                }))
            }
        }
    }

    /// The filesystem path of a descriptor blob, or `None` when the backend keeps
    /// it as an engine blob (in which case the partial lane is unavailable).
    pub(crate) fn descriptor_path(&self, id: &Id) -> Option<PathBuf> {
        match self.backend {
            BlobBackend::Fs => Some(self.root.join("descriptor").join(id.to_hex())),
            #[cfg(feature = "entropyfs-store")]
            BlobBackend::EntropyFs(_) => None,
        }
    }

    /// Store a serialized `.voldoc` descriptor as a content-addressed blob.
    ///
    /// Returns the blob's [`Id`] (`BLAKE3-256(bytes)`), which is what the field
    /// manifest binds. Idempotent.
    pub fn put_descriptor(&mut self, bytes: &[u8]) -> Result<Id> {
        self.backend.descriptor_put(&self.root, bytes)
    }

    /// Fetch a descriptor blob, verifying its content id.
    pub fn get_descriptor(&self, id: &Id) -> Result<Vec<u8>> {
        let bytes = self.backend.descriptor_get(&self.root, id)?;
        let actual = Id::of(&bytes);
        if actual != *id {
            return Err(Error::integrity_mismatch(format!(
                "descriptor blob {id} hashes to {actual}"
            )));
        }
        self.io.add_descriptor(bytes.len() as u64);
        Ok(bytes)
    }

    /// Store a canonical field manifest.
    pub fn put_field(&mut self, manifest: &FieldRoot) -> Result<FieldId> {
        self.backend.field_put(&self.root, manifest)
    }

    /// Fetch a field manifest, verifying its content id and universe.
    pub fn get_field(&self, id: &FieldId) -> Result<FieldRoot> {
        let bytes = self.backend.field_get(&self.root, id)?;
        let manifest = FieldRoot::decode_canonical(&bytes)?;
        if manifest.content_id() != *id {
            return Err(Error::integrity_mismatch(format!(
                "field {id} manifest content id mismatch"
            )));
        }
        self.io.add_manifest(bytes.len() as u64);
        Ok(manifest)
    }

    /// List every stored field manifest id.
    ///
    /// The filesystem backend reads the `field/` directory. The EntropyFS backend
    /// **declines** with `UnsupportedFeature`: the engine exposes no per-blob
    /// enumeration, so a manifest cannot be listed — but any manifest remains
    /// openable by its `FieldId` (`get_field`).
    pub fn list_fields(&self) -> Result<Vec<FieldId>> {
        if !matches!(self.backend, BlobBackend::Fs) {
            return Err(Error::unsupported_feature(
                "EntropyFS exposes no per-blob enumeration; a field store backed by it \
                 cannot list its manifests (open a field by its FieldId instead)",
            ));
        }
        let dir = self.root.join("field");
        let mut out = Vec::new();
        for entry in fs::read_dir(&dir)?.flatten() {
            if let Some(name) = entry.file_name().to_str()
                && let Ok(id) = FieldId::from_hex(name)
            {
                out.push(id);
            }
        }
        out.sort_unstable();
        Ok(out)
    }

    /// Ingest a serialized `.voldoc` descriptor as a new field.
    ///
    /// Stage A (durable exact capture): store the descriptor blob and an exact
    /// `DocumentExact` root node, then write the manifest. Deeper Stage-B
    /// procedural nodes are added by [`crate::field::ingest`].
    pub fn ingest(&mut self, descriptor_bytes: &[u8], limits: Limits) -> Result<FieldId> {
        // The descriptor must parse and materialize exactly: ingest never invents
        // authority it cannot reproduce.
        let parsed = crate::container::Descriptor::parse(descriptor_bytes, limits)?;
        let source = crate::materialize::materialize(&parsed, limits)?;
        let descriptor_id = self.put_descriptor(descriptor_bytes)?;

        let root = SeedNode::new(
            NodeKind::DocumentExact,
            source.len() as u64,
            Vec::new(),
            Vec::new(),
            "field:document-exact",
        );
        let root_id = self.seeds.put_node(&root.encode_canonical())?;

        let manifest = FieldRoot {
            universe_id: crate::container::universe_id_from_str(FIELD_UNIVERSE),
            source_sha256: parsed.descriptor.source_sha256,
            source_len: parsed.descriptor.source_len,
            descriptor_id: *descriptor_id.as_bytes(),
            root_node: root_id,
            index_root: manifest::ABSENT_ROOT,
            node_count: 1,
            index_node_count: 0,
            provenance: format!("field:ingest;{}", parsed.descriptor.format_basis),
        };
        self.put_field(&manifest)
    }
}

/// Write bytes to `path` atomically (`tmp -> fsync -> rename`).
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let dir = path
        .parent()
        .ok_or_else(|| Error::internal_invariant("atomic write path has no parent"))?;
    fs::create_dir_all(dir)?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("blob");
    let tmp = dir.join(format!(".{name}.tmp-{}", std::process::id()));
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

/// An opened field: the manifest plus its parsed exact descriptor.
pub struct Field {
    store_root: PathBuf,
    manifest: FieldRoot,
    parsed: ParsedDescriptor,
    descriptor_bytes: Vec<u8>,
    /// A detached seed handle sharing the store's substrate, so a field opened
    /// from an EntropyFS-backed store materializes its seed nodes through the
    /// same engine (never a second opener).
    seeds: SeedSubstrate,
    /// The physical bytes this `open` fetched to load the manifest and the
    /// descriptor blob. An observation attributes exactly these to its own
    /// `descriptor_bytes_read`/`manifest_bytes_read` (review fix #1/#2).
    open_io: IoSnapshot,
}

impl std::fmt::Debug for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Field")
            .field("manifest", &self.manifest.content_id())
            .field("source_len", &self.manifest.source_len)
            .finish_non_exhaustive()
    }
}

impl Field {
    /// Open a field by id, loading and verifying its descriptor.
    pub fn open(store: &FieldStore, id: &FieldId, limits: Limits) -> Result<Field> {
        let io_before = store.io().snapshot();
        let manifest = store.get_field(id)?;
        Field::open_after_manifest(store, manifest, io_before, limits)
    }

    /// The body of [`Field::open`] from an already-read manifest. `io_before` is
    /// the snapshot `open_io` is measured from: pass one taken before the
    /// manifest read to charge it here (the ordinary path), or after it to charge
    /// it elsewhere (the narrow probe, which already counted it in `base_io`).
    pub(crate) fn open_after_manifest(
        store: &FieldStore,
        manifest: FieldRoot,
        io_before: IoSnapshot,
        limits: Limits,
    ) -> Result<Field> {
        let descriptor_bytes = store.get_descriptor(&Id::from_bytes(manifest.descriptor_id))?;
        let open_io = io_before.delta(&store.io().snapshot());
        let mut parsed = crate::container::Descriptor::parse(&descriptor_bytes, limits)?;
        parsed.universe_id = manifest.universe_id;
        // The manifest must agree with the descriptor it binds.
        if parsed.descriptor.source_len != manifest.source_len
            || parsed.descriptor.source_sha256 != manifest.source_sha256
        {
            return Err(Error::integrity_mismatch(
                "field manifest does not match its descriptor's declared source",
            ));
        }
        Ok(Field {
            store_root: store.root().to_path_buf(),
            manifest,
            parsed,
            descriptor_bytes,
            seeds: store.seed_substrate(),
            open_io,
        })
    }

    /// The physical bytes fetched to open this field (manifest + descriptor).
    pub(crate) fn open_io(&self) -> IoSnapshot {
        self.open_io
    }

    /// The field manifest.
    pub fn manifest(&self) -> &FieldRoot {
        &self.manifest
    }

    /// The field id.
    pub fn id(&self) -> FieldId {
        self.manifest.content_id()
    }

    /// The parsed exact descriptor.
    pub fn parsed(&self) -> &ParsedDescriptor {
        &self.parsed
    }

    /// The raw serialized descriptor bytes.
    pub fn descriptor_bytes(&self) -> &[u8] {
        &self.descriptor_bytes
    }

    /// The store root this field was opened from.
    pub fn store_root(&self) -> &Path {
        &self.store_root
    }

    /// Materialize the exact original source bytes.
    ///
    /// This is the archival authority: it is byte-identical to the standalone
    /// `.voldoc` materialization and is the only path that verifies the
    /// whole-source SHA-256.
    pub fn materialize_exact(&self, limits: Limits) -> Result<Vec<u8>> {
        crate::materialize::materialize(&self.parsed, limits)
    }

    /// Materialize a seed node's output by id, using the field's own seed handle
    /// (which for an EntropyFS-backed store is the shared engine, not a reopen).
    pub fn materialize_node(
        &self,
        id: &NodeId,
        limits: Limits,
        budget: &mut dag::EvalBudget,
    ) -> Result<Vec<u8>> {
        let node = dag::load_node(&self.seeds, id)?;
        dag::materialize_node(
            &self.parsed,
            &self.seeds,
            &node,
            limits,
            budget,
            node.limits.max_depth,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "vole-field-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        p
    }

    fn tiny_descriptor() -> Vec<u8> {
        use crate::container::{Descriptor, ObjectSource};
        use crate::dra::{Op, Program};
        let source = b"the exact field bytes";
        let d = Descriptor {
            universe: crate::container::UNIVERSE.to_string(),
            source_format: crate::SOURCE_FORMAT_OPAQUE,
            format_basis: "opaque:field-test".to_string(),
            models: vec![],
            channels: vec![],
            objects: vec![ObjectSource::Inline(source.to_vec())],
            program: Program::new(vec![Op::EmitObject { object_id: 0 }]),
            observation_index: None,
            seek_directory: false,
            source_sha256: crate::integrity::sha256(source),
            source_len: source.len() as u64,
        };
        d.serialize().unwrap().0
    }

    #[test]
    fn ingest_then_materialize_is_exact() {
        let root = temp_root("rt");
        let mut store = FieldStore::open(&root).unwrap();
        let bytes = tiny_descriptor();
        let id = store.ingest(&bytes, Limits::DEFAULT).unwrap();
        let field = Field::open(&store, &id, Limits::DEFAULT).unwrap();
        assert_eq!(
            field.materialize_exact(Limits::DEFAULT).unwrap(),
            b"the exact field bytes"
        );
        // The root node also materializes the exact source.
        let mut budget = dag::EvalBudget::default();
        let out = field
            .materialize_node(&field.manifest.root_node, Limits::DEFAULT, &mut budget)
            .unwrap();
        assert_eq!(out, b"the exact field bytes");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn field_id_is_stable_and_reopenable() {
        let root = temp_root("reopen");
        let bytes = tiny_descriptor();
        let id1 = {
            let mut store = FieldStore::open(&root).unwrap();
            store.ingest(&bytes, Limits::DEFAULT).unwrap()
        };
        let store = FieldStore::open(&root).unwrap();
        let m = store.get_field(&id1).unwrap();
        assert_eq!(m.content_id(), id1);
        assert_eq!(store.list_fields().unwrap(), vec![id1]);
        fs::remove_dir_all(&root).ok();
    }
}

/// The EntropyFS-backed field store: the seed DAG persisted one engine blob per
/// node, through the same engine as the descriptor and manifest namespaces.
#[cfg(all(test, feature = "entropyfs-store"))]
mod entropyfs_field_tests {
    use super::*;
    use crate::field::observe::{self, ObserveRequest, Representation, Selector};
    use crate::field::provenance::AnswerValue;
    use crate::store::seed_closure;

    fn temp_root(label: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "vole-field-entropyfs-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        p
    }

    fn tiny_descriptor() -> Vec<u8> {
        use crate::container::{Descriptor, ObjectSource};
        use crate::dra::{Op, Program};
        let source = b"the exact field bytes";
        let d = Descriptor {
            universe: crate::container::UNIVERSE.to_string(),
            source_format: crate::SOURCE_FORMAT_OPAQUE,
            format_basis: "opaque:field-entropyfs-test".to_string(),
            models: vec![],
            channels: vec![],
            objects: vec![ObjectSource::Inline(source.to_vec())],
            program: Program::new(vec![Op::EmitObject { object_id: 0 }]),
            observation_index: None,
            seek_directory: false,
            source_sha256: crate::integrity::sha256(source),
            source_len: source.len() as u64,
        };
        d.serialize().unwrap().0
    }

    #[test]
    fn ingest_observe_and_exact_materialize_through_the_engine() {
        let root = temp_root("rt");
        let descriptor = tiny_descriptor();
        let expected = b"the exact field bytes";
        let mut store = FieldStore::open_entropyfs(&root).unwrap();
        let id = store.ingest(&descriptor, Limits::DEFAULT).unwrap();
        let field = Field::open(&store, &id, Limits::DEFAULT).unwrap();
        assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), expected);
        // The DocumentExact seed node materializes through the engine itself.
        let mut budget = dag::EvalBudget::default();
        assert_eq!(
            field
                .materialize_node(&field.manifest().root_node, Limits::DEFAULT, &mut budget)
                .unwrap(),
            expected
        );
        // A narrow observation resolves against the engine-backed seed substrate.
        let req = ObserveRequest::new(
            Selector::ByteRange { offset: 4, len: 5 },
            Representation::ExactBytes,
        );
        let (answer, _stats, _) = observe::observe(&mut store, &id, &req, Limits::DEFAULT).unwrap();
        match answer.value {
            AnswerValue::Bytes(b) => assert_eq!(b, b"exact"),
            other => panic!("expected bytes, got {other:?}"),
        }
        // `list_fields` cannot enumerate the engine namespace, but the manifest is
        // openable by id.
        assert_eq!(
            store.list_fields().unwrap_err().class(),
            crate::ErrorClass::UnsupportedFeature
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn cross_process_reopen_reads_what_a_previous_handle_wrote() {
        let root = temp_root("reopen");
        let descriptor = tiny_descriptor();
        // The first handle (a first process) writes, then is dropped, releasing the
        // engine's exclusive lock.
        let (id, root_node) = {
            let mut store = FieldStore::open_entropyfs(&root).unwrap();
            let id = store.ingest(&descriptor, Limits::DEFAULT).unwrap();
            let node = store.get_field(&id).unwrap().root_node;
            // Barrier so the engine publishes the epoch before this handle drops.
            store.sync().unwrap();
            (id, node)
        };
        // A brand-new handle over the same directory (a second process) reads it.
        let store = FieldStore::open_entropyfs(&root).unwrap();
        let manifest = store.get_field(&id).unwrap();
        assert_eq!(manifest.content_id(), id);
        let field = Field::open(&store, &id, Limits::DEFAULT).unwrap();
        assert_eq!(
            field.materialize_exact(Limits::DEFAULT).unwrap(),
            b"the exact field bytes"
        );
        let mut budget = dag::EvalBudget::default();
        assert_eq!(
            field
                .materialize_node(&root_node, Limits::DEFAULT, &mut budget)
                .unwrap(),
            b"the exact field bytes"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn list_nodes_declines_so_gc_cannot_sweep() {
        let root = temp_root("gc");
        let mut store = FieldStore::open_entropyfs(&root).unwrap();
        let id = store.ingest(&tiny_descriptor(), Limits::DEFAULT).unwrap();
        let manifest = store.get_field(&id).unwrap();
        // A mark-and-sweep needs `list_nodes`, which the engine cannot provide.
        let e = store.seeds().list_nodes().unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::UnsupportedFeature);
        // The reachable closure is still derivable, because it descends by explicit
        // id and never enumerates the store.
        let reachable =
            seed_closure(store.seeds(), &[manifest.root_node], |_| Ok(Vec::new())).unwrap();
        assert!(reachable.contains(&manifest.root_node));
        std::fs::remove_dir_all(&root).ok();
    }
}
