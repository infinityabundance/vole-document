//! The `EmbeddedStore` reference backend: a local content-addressed directory.
//!
//! Storage is raw (no compression) so its accounting is transparent and
//! backend-independent. Layout:
//!
//! ```text
//! STORE_ROOT/
//!   STORE                    # magic + backend tag + format version (10 bytes)
//!   objects/<aa>/<bb>/<64-hex-id>       # object payload, raw
//!   objects/<aa>/<bb>/<64-hex-id>.tmp-<pid>-<nanos>   # in-flight put
//! ```
//!
//! ## Atomic put invariant
//!
//! The final name exists ⇒ the object is complete and byte-exact. A put writes
//! a temp sibling, `write_all` + `sync_all`, then renames onto the final name
//! (atomic on one filesystem). A crash leaves at most a `.tmp-*` file, swept on
//! the next [`EmbeddedStore::open`]; only `.tmp-*` names are swept, foreign
//! entries are left untouched.

use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{Error, Result};
use crate::store::{Id, ObjectStore, StoreStats};

/// Store marker magic (`VOLDST` + 0x1A text-EOF + NUL).
pub const STORE_MAGIC: [u8; 8] = *b"VOLDST\x1A\x00";
/// Backend tag for the raw embedded store.
pub const STORE_BACKEND_EMBEDDED: u8 = 0x01;
/// Current on-disk store format version.
pub const STORE_FORMAT_VERSION: u8 = 1;

/// A content-addressed directory store.
#[derive(Debug, Clone)]
pub struct EmbeddedStore {
    root: PathBuf,
}

impl EmbeddedStore {
    /// Open (creating if necessary) a store rooted at `root`.
    ///
    /// Creates the directory tree, writes/validates the `STORE` marker, and
    /// sweeps any leftover `.tmp-*` files from a crashed put.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("objects"))?;
        let store = EmbeddedStore { root };
        store.ensure_marker()?;
        store.sweep_tmp()?;
        Ok(store)
    }

    /// The store root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The raw stored length of `id`, or a typed error if absent.
    pub fn len(&self, id: &Id) -> Result<u64> {
        match fs::metadata(self.object_path(id)) {
            Ok(m) if m.is_file() => Ok(m.len()),
            Ok(_) => Err(Error::missing_external_object(format!(
                "object {id} is not a file"
            ))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(
                Error::missing_external_object(format!("object {id} is not present in the store")),
            ),
            Err(e) => Err(Error::from(e)),
        }
    }

    /// Whether the store holds no objects.
    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.stats()?.object_count == 0)
    }

    /// Store statistics over every present object.
    pub fn stats(&self) -> Result<StoreStats> {
        let listed = self.list()?;
        let total_bytes: u64 = listed.iter().map(|(_, len)| *len).sum();
        Ok(StoreStats {
            object_count: listed.len() as u64,
            total_bytes,
            stored_bytes: total_bytes,
        })
    }

    fn marker_path(&self) -> PathBuf {
        self.root.join("STORE")
    }

    fn objects_dir(&self) -> PathBuf {
        self.root.join("objects")
    }

    fn object_path(&self, id: &Id) -> PathBuf {
        let hex = id.to_hex();
        self.objects_dir()
            .join(&hex[0..2])
            .join(&hex[2..4])
            .join(&hex)
    }

    fn ensure_marker(&self) -> Result<()> {
        let path = self.marker_path();
        if let Ok(bytes) = fs::read(&path) {
            if bytes.len() != 10 || bytes[0..8] != STORE_MAGIC {
                return Err(Error::invalid_container(
                    "store marker has a bad magic or length",
                ));
            }
            if bytes[8] != STORE_BACKEND_EMBEDDED {
                return Err(Error::unsupported_feature(format!(
                    "store backend tag {:#04x} is not the embedded backend",
                    bytes[8]
                )));
            }
            if bytes[9] > STORE_FORMAT_VERSION {
                return Err(Error::unsupported_version(format!(
                    "store format version {} is newer than this build ({STORE_FORMAT_VERSION})",
                    bytes[9]
                )));
            }
            return Ok(());
        }
        let mut marker = Vec::with_capacity(10);
        marker.extend_from_slice(&STORE_MAGIC);
        marker.push(STORE_BACKEND_EMBEDDED);
        marker.push(STORE_FORMAT_VERSION);
        write_atomic(&path, &marker)
    }

    /// Remove leftover `.tmp-*` files from a crashed put. Foreign entries (any
    /// name not containing the `.tmp-` infix) are left untouched.
    fn sweep_tmp(&self) -> Result<()> {
        let objects = self.objects_dir();
        for aa in read_dir_dirs(&objects)? {
            for bb in read_dir_dirs(&aa)? {
                for entry in fs::read_dir(&bb)? {
                    let entry = entry?;
                    if !entry.file_type()?.is_file() {
                        continue;
                    }
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if name.contains(".tmp-") {
                        let _ = fs::remove_file(entry.path());
                    }
                }
            }
        }
        Ok(())
    }
}

impl ObjectStore for EmbeddedStore {
    fn put(&mut self, bytes: &[u8]) -> Result<Id> {
        let id = Id::of(bytes);
        let path = self.object_path(&id);
        if path.is_file() {
            return Ok(id);
        }
        let parent = path
            .parent()
            .ok_or_else(|| Error::internal_invariant("object path has no parent"))?;
        fs::create_dir_all(parent)?;

        let hex = id.to_hex();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp = parent.join(format!(".{hex}.tmp-{}-{nanos}", std::process::id()));

        let write_result = (|| -> Result<()> {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(bytes)?;
            f.sync_all()?;
            Ok(())
        })();
        if let Err(e) = write_result {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }
        if let Err(e) = fs::rename(&tmp, &path) {
            let _ = fs::remove_file(&tmp);
            return Err(Error::from(e));
        }
        // Best-effort directory durability (Linux).
        if let Ok(dir) = fs::File::open(parent) {
            let _ = dir.sync_all();
        }
        Ok(id)
    }

    fn get(&self, id: &Id) -> Result<Vec<u8>> {
        let bytes = match fs::read(self.object_path(id)) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::missing_external_object(format!(
                    "object {id} is not present in the store"
                )));
            }
            Err(e) => return Err(Error::from(e)),
        };
        if Id::of(&bytes) != *id {
            return Err(Error::integrity_mismatch(format!(
                "stored object {id} does not hash to its content id"
            )));
        }
        Ok(bytes)
    }

    fn get_range(&self, id: &Id, offset: u64, len: u64) -> Result<Vec<u8>> {
        let mut f = match fs::File::open(self.object_path(id)) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::missing_external_object(format!(
                    "object {id} is not present in the store"
                )));
            }
            Err(e) => return Err(Error::from(e)),
        };
        let file_len = f.metadata()?.len();
        let end = offset
            .checked_add(len)
            .ok_or_else(|| Error::integrity_mismatch("object range offset+len overflow"))?;
        if end > file_len {
            return Err(Error::integrity_mismatch(format!(
                "object {id} has {file_len} bytes; range [{offset}, {end}) is out of bounds"
            )));
        }
        f.seek(SeekFrom::Start(offset))?;
        let mut out = vec![0u8; len as usize];
        f.read_exact(&mut out)?;
        Ok(out)
    }

    fn contains(&self, id: &Id) -> Result<bool> {
        Ok(self.object_path(id).is_file())
    }

    fn list(&self) -> Result<Vec<(Id, u64)>> {
        let mut out: Vec<(Id, u64)> = Vec::new();
        let objects = self.objects_dir();
        for aa in read_dir_dirs(&objects)? {
            for bb in read_dir_dirs(&aa)? {
                for entry in fs::read_dir(&bb)? {
                    let entry = entry?;
                    if !entry.file_type()?.is_file() {
                        continue;
                    }
                    let name = entry.file_name();
                    let Some(name) = name.to_str() else {
                        continue;
                    };
                    let Ok(id) = Id::from_hex(name) else {
                        continue;
                    };
                    out.push((id, entry.metadata()?.len()));
                }
            }
        }
        out.sort_unstable_by_key(|(id, _)| *id);
        Ok(out)
    }

    fn remove(&self, id: &Id) -> Result<u64> {
        let path = self.object_path(id);
        let len = match fs::metadata(&path) {
            Ok(m) if m.is_file() => m.len(),
            Ok(_) => return Ok(0),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(Error::from(e)),
        };
        fs::remove_file(&path)?;
        // Best-effort empty-shard cleanup; a non-empty shard is left in place.
        if let Some(bb) = path.parent() {
            let _ = fs::remove_dir(bb);
            if let Some(aa) = bb.parent() {
                let _ = fs::remove_dir(aa);
            }
        }
        Ok(len)
    }
}

/// Write `bytes` to `path` atomically: temp sibling, `sync_all`, rename, fsync
/// the parent directory. Used for the small `STORE` marker.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "STORE".to_string());
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = dir.join(format!(".{name}.tmp-{}-{nanos}", std::process::id()));

    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(Error::from(e));
    }
    if let Ok(d) = fs::File::open(&dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

/// Subdirectories of `dir` (missing `dir` yields an empty list), sorted for
/// deterministic traversal.
fn read_dir_dirs(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(Error::from(e)),
    };
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            out.push(entry.path());
        }
    }
    out.sort_unstable();
    Ok(out)
}
