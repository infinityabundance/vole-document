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

pub mod dag;
pub mod derive;
pub mod index;
pub mod ingest;
pub mod manifest;
pub mod node;

pub use manifest::{FieldId, FieldRoot};

use std::fs;
use std::path::{Path, PathBuf};

use crate::container::ParsedDescriptor;
use crate::error::{Error, Result};
use crate::limits::Limits;
use crate::store::{FsSeedStore, Id, NodeId, SeedStore};

use self::node::{NodeKind, SeedNode};

/// The canonical universe string for a Phase-11 field.
pub const FIELD_UNIVERSE: &str = "vole-document;universe;phase11;exact-bytes;dra-8;opaque+entropy+pdf+channels+offsets+packed+packed-channels+deflate-replay-preflate-0.7.6-experimental+observation-index-v1+seek-directory-v1+external-objects-v1+procedural-seed-field-v1+hier-index-v1";

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
pub struct FieldStore {
    root: PathBuf,
    seeds: FsSeedStore,
}

impl std::fmt::Debug for FieldStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FieldStore")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl FieldStore {
    /// Create or open a field store rooted at `root`.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("descriptor"))?;
        fs::create_dir_all(root.join("field"))?;
        fs::create_dir_all(root.join("index"))?;
        fs::create_dir_all(root.join("cache"))?;
        let seeds = FsSeedStore::open(&root)?;
        Ok(FieldStore { root, seeds })
    }

    /// The store root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The seed store (for advanced callers and courts).
    pub fn seeds(&self) -> &FsSeedStore {
        &self.seeds
    }

    /// Mutable seed store access.
    pub fn seeds_mut(&mut self) -> &mut FsSeedStore {
        &mut self.seeds
    }

    fn descriptor_path(&self, id: &Id) -> PathBuf {
        self.root.join("descriptor").join(id.to_hex())
    }

    fn field_path(&self, id: &FieldId) -> PathBuf {
        self.root.join("field").join(id.to_hex())
    }

    /// Store a serialized `.voldoc` descriptor as a content-addressed blob.
    ///
    /// Returns the blob's [`Id`] (`BLAKE3-256(bytes)`), which is what the field
    /// manifest binds. Idempotent.
    pub fn put_descriptor(&mut self, bytes: &[u8]) -> Result<Id> {
        let id = Id::of(bytes);
        let path = self.descriptor_path(&id);
        if path.exists() {
            return Ok(id);
        }
        write_atomic(&path, bytes)?;
        Ok(id)
    }

    /// Fetch a descriptor blob, verifying its content id.
    pub fn get_descriptor(&self, id: &Id) -> Result<Vec<u8>> {
        let path = self.descriptor_path(id);
        let bytes = fs::read(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::missing_external_object(format!("descriptor blob {id} is not present"))
            } else {
                Error::io(format!("reading descriptor blob {id}: {e}"))
            }
        })?;
        let actual = Id::of(&bytes);
        if actual != *id {
            return Err(Error::integrity_mismatch(format!(
                "descriptor blob {id} hashes to {actual}"
            )));
        }
        Ok(bytes)
    }

    /// Store a canonical field manifest.
    pub fn put_field(&mut self, manifest: &FieldRoot) -> Result<FieldId> {
        let bytes = manifest.encode_canonical();
        let id = FieldId::of_manifest(&bytes);
        let path = self.field_path(&id);
        if !path.exists() {
            write_atomic(&path, &bytes)?;
        }
        Ok(id)
    }

    /// Fetch a field manifest, verifying its content id and universe.
    pub fn get_field(&self, id: &FieldId) -> Result<FieldRoot> {
        let bytes = fs::read(self.field_path(id)).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::missing_external_object(format!("field {id} is not present"))
            } else {
                Error::io(format!("reading field {id}: {e}"))
            }
        })?;
        let manifest = FieldRoot::decode_canonical(&bytes)?;
        if manifest.content_id() != *id {
            return Err(Error::integrity_mismatch(format!(
                "field {id} manifest content id mismatch"
            )));
        }
        Ok(manifest)
    }

    /// List every stored field manifest id.
    pub fn list_fields(&self) -> Result<Vec<FieldId>> {
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
        let manifest = store.get_field(id)?;
        let descriptor_bytes = store.get_descriptor(&Id::from_bytes(manifest.descriptor_id))?;
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
        })
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

    /// Materialize a seed node's output by id, using a fresh seed store handle.
    pub fn materialize_node(
        &self,
        id: &NodeId,
        limits: Limits,
        budget: &mut dag::EvalBudget,
    ) -> Result<Vec<u8>> {
        let seeds = FsSeedStore::open(&self.store_root)?;
        let node = dag::load_node(&seeds, id)?;
        dag::materialize_node(
            &self.parsed,
            &seeds,
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
