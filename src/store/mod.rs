//! Content-addressed object store (Phase 9).
//!
//! A `.voldoc` descriptor's object table is a single ordered sequence. Each
//! entry is either an inline byte object (`OBJECT`, tag `0x10`) or a reference
//! to an object held by an [`ObjectStore`] (`EXTERNAL_REF`, tag `0x80`). Inline
//! and external objects share one id space, so equal bytes are equal ids
//! regardless of which document they came from.
//!
//! ## Two digests, two roles
//!
//! | digest | role | authority |
//! |---|---|---|
//! | SHA-256 | whole reconstructed source; archival identity | parse/materialize/verify |
//! | BLAKE3-256 ([`Id`]) | one object's bytes; store namespace | relational/advisory |
//!
//! An [`Id`] is an *ephemeral, relational* name for a shareable object. It is
//! never a substitute for the source digest and never appears in `INTEGRITY`.
//!
//! ## Verification rule
//!
//! [`ObjectStore::get`] **must** re-hash the returned bytes and reject with
//! [`crate::ErrorClass::IntegrityMismatch`] unless `BLAKE3(bytes) == id`.
//! [`ObjectStore::get_range`] cannot re-verify the whole object; exactness for a
//! store-backed materialization therefore rests on the `EXTERNAL_REF` declared
//! length and the descriptor's `INTEGRITY` SHA-256 checked by `materialize`.

use core::fmt;
use std::collections::BTreeSet;

use crate::container::{Descriptor, ObjectSource};
use crate::error::{Error, Result};

#[cfg(feature = "store")]
mod embedded;
#[cfg(feature = "store")]
pub use embedded::{EmbeddedStore, STORE_FORMAT_VERSION, STORE_MAGIC};

/// Size accounting reported by a backend.
///
/// `total_bytes` is the sum of raw unique-object lengths (the backend-independent
/// headline); `stored_bytes` is what the backend physically writes (equal to
/// `total_bytes` for [`EmbeddedStore`], which stores raw; smaller for a
/// compressing backend).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StoreStats {
    /// Unique ids present.
    pub object_count: u64,
    /// Sum of object file lengths (raw).
    pub total_bytes: u64,
    /// On-disk bytes (`== total_bytes` for a raw backend).
    pub stored_bytes: u64,
}

/// Outcome of a mark-and-sweep GC pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GcReport {
    /// Number of distinct external ids reachable from the roots.
    pub reachable: u64,
    /// Number of stored objects removed.
    pub swept: u64,
    /// Raw bytes reclaimed by the sweep.
    pub bytes_reclaimed: u64,
    /// Reachable ids the store does not contain (must be empty for a valid
    /// closure; a non-empty list is a live `MissingExternalObject`).
    pub dangling: Vec<Id>,
}

/// `Id` is a newtype over the 32 raw bytes of `BLAKE3-256(object_bytes)`, with no
/// domain prefix. Hex is lower-case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id([u8; 32]);

impl Id {
    /// Wrap 32 raw digest bytes.
    pub const fn from_bytes(b: [u8; 32]) -> Self {
        Id(b)
    }

    /// The raw digest bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Content id of `bytes`: `BLAKE3-256(bytes)`.
    #[cfg(feature = "store")]
    pub fn of(bytes: &[u8]) -> Self {
        Id(*blake3::hash(bytes).as_bytes())
    }

    /// Lower-case hex rendering (64 characters).
    pub fn to_hex(&self) -> String {
        crate::integrity::to_hex(&self.0)
    }

    /// Parse exactly 64 lower- or upper-case hex characters.
    pub fn from_hex(s: &str) -> Result<Self> {
        let raw = s.as_bytes();
        if raw.len() != 64 {
            return Err(Error::usage(format!(
                "object id must be 64 hex characters, got {}",
                raw.len()
            )));
        }
        let nib = |c: u8| -> Option<u8> {
            match c {
                b'0'..=b'9' => Some(c - b'0'),
                b'a'..=b'f' => Some(c - b'a' + 10),
                b'A'..=b'F' => Some(c - b'A' + 10),
                _ => None,
            }
        };
        let mut out = [0u8; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            let hi =
                nib(raw[2 * i]).ok_or_else(|| Error::usage("object id has a non-hex character"))?;
            let lo = nib(raw[2 * i + 1])
                .ok_or_else(|| Error::usage("object id has a non-hex character"))?;
            *byte = (hi << 4) | lo;
        }
        Ok(Id(out))
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// A content-addressed store keyed by `BLAKE3-256` of the object bytes.
///
/// The object namespace is the descriptor's object table: the raw, uncompressed
/// bytes a DRA `EMIT_OBJECT`/`DEFLATE_REPLAY` op consumes. Inline and external
/// objects share one id space, so equal bytes are equal ids regardless of which
/// document they came from.
///
/// `put` takes `&mut self` (ids are content-derived, so retries are idempotent).
/// `list`/`remove` support mark-and-sweep GC; both decline by default so a
/// backend that exposes no enumeration or per-object delete (EntropyFS) fails
/// closed rather than reporting a fake sweep.
pub trait ObjectStore {
    /// Store `bytes`, returning its content id.
    ///
    /// Idempotent and at-least-once: identical bytes always return the same id
    /// and are stored once; a retry after a crash is a no-op. A returned id
    /// guarantees the bytes are durable under [`ObjectStore::get`].
    fn put(&mut self, bytes: &[u8]) -> Result<Id>;

    /// Fetch the exact bytes for `id`.
    ///
    /// MUST verify `BLAKE3(bytes) == id` before returning.
    /// - absent id        -> [`crate::ErrorClass::MissingExternalObject`]
    /// - bytes hash != id -> [`crate::ErrorClass::IntegrityMismatch`]
    fn get(&self, id: &Id) -> Result<Vec<u8>>;

    /// Fetch `len` bytes of `id` starting at `offset`.
    ///
    /// STRICT: a request for `offset + len > stored_len(id)` is a typed error,
    /// never a silent EOF clip. Range reads carry no whole-object hash gate.
    fn get_range(&self, id: &Id, offset: u64, len: u64) -> Result<Vec<u8>>;

    /// Whether `id` is present. `EmbeddedStore` overrides it with a stat.
    fn contains(&self, id: &Id) -> Result<bool> {
        let _ = id;
        Err(Error::unsupported_feature(
            "contains is not implemented by this backend",
        ))
    }

    /// Every stored `(id, raw_len)`, for closure checking and GC.
    ///
    /// The default declines, so a backend without enumeration never reports a
    /// fake closure.
    fn list(&self) -> Result<Vec<(Id, u64)>> {
        Err(Error::unsupported_feature(
            "object enumeration is not implemented by this backend",
        ))
    }

    /// Delete `id`, returning the raw bytes reclaimed (0 if absent).
    ///
    /// The default declines: a backend that exposes no per-object delete must
    /// not appear to have swept anything.
    fn remove(&self, id: &Id) -> Result<u64> {
        let _ = id;
        Err(Error::unsupported_feature(
            "per-object delete is not implemented by this backend",
        ))
    }
}

/// The resolver consumed by store-backed materialization. A blanket impl makes
/// every [`ObjectStore`] a resolver, so callers pass `&store`.
pub trait ObjectResolver {
    /// Fetch `id`, verifying that its length is exactly `len`.
    fn get(&self, id: &Id, len: u64) -> Result<Vec<u8>>;
}

impl<T: ObjectStore> ObjectResolver for T {
    fn get(&self, id: &Id, len: u64) -> Result<Vec<u8>> {
        let bytes = ObjectStore::get(self, id)?;
        if bytes.len() as u64 != len {
            return Err(Error::integrity_mismatch(format!(
                "external object {id} has {} bytes, EXTERNAL_REF declared {len}",
                bytes.len()
            )));
        }
        Ok(bytes)
    }
}

/// A resolver that resolves nothing; used by the standalone `materialize`.
pub struct NullResolver;

impl ObjectResolver for NullResolver {
    fn get(&self, id: &Id, _len: u64) -> Result<Vec<u8>> {
        Err(Error::missing_external_object(format!(
            "no store supplied to resolve object {id}"
        )))
    }
}

/// Inline → external: put every object's bytes into `store`, replacing it with
/// an [`ObjectSource::External`] reference.
///
/// Objects already external are resolved through `resolver` (so a reference the
/// resolver cannot satisfy fails closed) and re-put into `store`, so every
/// object ends up in the destination store. Identical objects collapse to one
/// store entry by content addressing (refcount > 1 is one stored object).
pub fn externalize<R: ObjectResolver + ?Sized>(
    d: &mut Descriptor,
    resolver: &R,
    store: &mut impl ObjectStore,
) -> Result<()> {
    let mut out: Vec<ObjectSource> = Vec::with_capacity(d.objects.len());
    for src in &d.objects {
        let bytes: Vec<u8> = match src {
            ObjectSource::Inline(bytes) => bytes.clone(),
            ObjectSource::External { id, len } => resolver.get(id, *len)?,
        };
        let len = bytes.len() as u64;
        let id = store.put(&bytes)?;
        out.push(ObjectSource::External { id, len });
    }
    d.objects = out;
    Ok(())
}

/// External → inline: resolve every reference through `resolver` (verifying id
/// and length) and replace it with [`ObjectSource::Inline`]. The external
/// feature bit clears automatically because `required_features()` is derived.
pub fn hydrate<R: ObjectResolver>(d: &mut Descriptor, resolver: &R) -> Result<()> {
    let mut out: Vec<ObjectSource> = Vec::with_capacity(d.objects.len());
    for src in &d.objects {
        match src {
            ObjectSource::Inline(bytes) => out.push(ObjectSource::Inline(bytes.clone())),
            ObjectSource::External { id, len } => {
                out.push(ObjectSource::Inline(resolver.get(id, *len)?));
            }
        }
    }
    d.objects = out;
    Ok(())
}

/// Mark-and-sweep garbage collection over a set of root descriptors.
///
/// *Mark*: the union of every root's external ids. *Sweep*: `stored \ mark`.
/// `dangling` is `mark \ stored` and must be empty for a valid closure.
pub fn gc<R: ObjectResolver + ObjectStore>(roots: &[Descriptor], store: &R) -> Result<GcReport> {
    let mut mark: BTreeSet<Id> = BTreeSet::new();
    for root in roots {
        for src in &root.objects {
            if let ObjectSource::External { id, .. } = src {
                mark.insert(*id);
            }
        }
    }

    let stored = store.list()?;
    let stored_ids: BTreeSet<Id> = stored.iter().map(|(id, _)| *id).collect();

    let mut dangling: Vec<Id> = mark
        .iter()
        .filter(|id| !stored_ids.contains(id))
        .copied()
        .collect();
    dangling.sort_unstable();

    let mut swept: u64 = 0;
    let mut bytes_reclaimed: u64 = 0;
    for (id, _len) in &stored {
        if !mark.contains(id) {
            bytes_reclaimed += store.remove(id)?;
            swept += 1;
        }
    }

    Ok(GcReport {
        reachable: mark.len() as u64,
        swept,
        bytes_reclaimed,
        dangling,
    })
}
