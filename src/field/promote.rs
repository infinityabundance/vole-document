//! Adaptive procedural promotion (Phase 15.6) — **mechanism only**.
//!
//! The field DAG is already lazy and `materialize_inner`
//! already persists *every* node output (intermediates included) into the
//! disposable, content-addressed `cache/` (Phase 11.8). The gap this module
//! closes is a **budgeted, durable, policy-governed** promotion layer on top of
//! that: a bounded subset of the reused intermediates is written to a second,
//! durable, content-addressed store under `<root>/promoted/`.
//!
//! Promotion is **opt-in** ([`PromotePolicy::enabled`] defaults to `false`) and
//! is **never consulted for exact materialization**: this store is just another
//! [`OutputCache`], off the exactness path. A content-addressed [`NodeId`] binds
//! a node's full dependency closure, so an unchanged closure hits and a changed
//! dependency misses — there is **no invalidation pass and no revision rewrite**
//! (§3). A promoted entry for a changed id is unreachable, never wrong.
//!
//! ## On-disk layout (identical to [`DerivedCache`])
//!
//! ```text
//! <root>/promoted/<64-hex>        node output bytes, verbatim
//! <root>/promoted/<64-hex>.b3     BLAKE3-256 sidecar digest of those bytes
//! ```
//!
//! Reads re-hash against the sidecar (mirroring `cache.rs`); a missing or
//! mismatched sidecar is a miss / fail-closed error, never wrong bytes.
//!
//! ## Bounding
//!
//! Total promoted **output** bytes are capped at [`PromotePolicy::budget_bytes`];
//! on overflow the least-recently-seen promoted entries are evicted
//! ([`PromotedStore::remove`]) until the new entry fits. A single entry larger
//! than [`PromotePolicy::max_node_bytes`] is *never* promoted, which excludes the
//! giant final answers (`DocumentExact`, `Concat`, whole-document text) from ever
//! becoming candidates. The host swap is zram (RAM-backed), so an unbounded
//! promotion would risk OOM; the byte cap is mandatory, not a convenience.
//!
//! The governor's hit/cost counters are **in-memory** (`Rc<RefCell<..>>`, exactly
//! like `ModelMemo`) and shared for the life of a store handle (one session/process). There is no on-disk ledger in this mechanism
//! step; on first use the durable byte count is reconciled from
//! [`PromotedStore::output_bytes`], and a store that already exceeds the budget
//! is cleared wholesale (the same coarse clear-on-overflow bound `ModelMemo`
//! uses). Cross-process LRU over a persistent ledger is deferred (see the design
//! §2.2/§3 and Phase 15.7).

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::error::{Error, Result};
use crate::store::NodeId;

use super::cache::DerivedCache;
use super::dag::{CacheEffect, CacheNote, OutputCache};
use super::node::NodeKind;
use super::write_atomic;

/// Default durable promoted-store byte budget (output bytes).
pub const DEFAULT_PROMOTE_BUDGET_BYTES: u64 = 256 * 1024 * 1024;
/// Default cap on a single promotable node output; a larger output is never
/// promoted (this is what keeps giant final answers out of the durable store).
pub const DEFAULT_PROMOTE_MAX_NODE_BYTES: u64 = 8 * 1024 * 1024;
/// Default horizon (in observation events) the governor extrapolates demand over.
pub const DEFAULT_PROMOTE_HORIZON: u64 = 64;
/// Default storage rent, work-units per output byte per horizon.
pub const DEFAULT_PROMOTE_RENT_PER_BYTE: u64 = 1;
/// Default reuse events required before promotion is considered.
pub const DEFAULT_PROMOTE_MIN_HITS: u32 = 2;

/// The pre-registered promotion policy knobs (recorded verbatim in a receipt).
#[derive(Debug, Clone, Copy)]
pub struct PromotePolicy {
    /// Master switch; `false` (the default) makes the whole layer a no-op.
    pub enabled: bool,
    /// Durable store byte budget (promoted output bytes).
    pub budget_bytes: u64,
    /// A bigger output is never promoted (no giant final answers).
    pub max_node_bytes: u64,
    /// Observation events the governor extrapolates future demand over.
    pub horizon: u64,
    /// Storage rent, work-units per output byte per horizon.
    pub rent_per_byte: u64,
    /// Reuse events required before promotion is considered.
    pub min_hits: u32,
}

impl Default for PromotePolicy {
    fn default() -> Self {
        PromotePolicy {
            enabled: false,
            budget_bytes: DEFAULT_PROMOTE_BUDGET_BYTES,
            max_node_bytes: DEFAULT_PROMOTE_MAX_NODE_BYTES,
            horizon: DEFAULT_PROMOTE_HORIZON,
            rent_per_byte: DEFAULT_PROMOTE_RENT_PER_BYTE,
            min_hits: DEFAULT_PROMOTE_MIN_HITS,
        }
    }
}

/// A durable, content-addressed, budgeted store of promoted intermediates.
///
/// Same atomic layout as [`DerivedCache`] but under `<root>/promoted/`: a
/// `<64-hex>` output file plus a `<64-hex>.b3` BLAKE3 sidecar, verified on read.
pub struct PromotedStore {
    root: PathBuf,
}

impl std::fmt::Debug for PromotedStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PromotedStore")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl PromotedStore {
    /// Open (creating if needed) a promoted store rooted at `root`.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        crate::store::durable::create_dir_all(&root)?;
        Ok(PromotedStore { root })
    }

    /// The store root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn bytes_path(&self, id: &NodeId) -> PathBuf {
        self.root.join(id.to_hex())
    }

    fn digest_path(&self, id: &NodeId) -> PathBuf {
        self.root.join(format!("{}.b3", id.to_hex()))
    }

    /// Fetch a promoted output, verifying the sidecar digest.
    ///
    /// * absent bytes or absent sidecar -> `Ok(None)` (a miss).
    /// * sidecar present but not `BLAKE3(bytes)` -> [`crate::ErrorClass::IntegrityMismatch`]
    ///   (fail closed; never return wrong bytes). The DAG treats the error as a
    ///   miss and recomputes, so a poisoned entry cannot change an answer.
    pub fn get(&self, id: &NodeId) -> Result<Option<Vec<u8>>> {
        let bytes = match fs::read(self.bytes_path(id)) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(Error::io(format!("reading promoted entry {id}: {e}"))),
        };
        let sidecar = match fs::read(self.digest_path(id)) {
            Ok(b) => b,
            // A missing sidecar is an incomplete (hence disposable) entry.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(Error::io(format!("reading promoted sidecar {id}: {e}"))),
        };
        if sidecar.as_slice() != blake3::hash(&bytes).as_bytes() {
            return Err(Error::integrity_mismatch(format!(
                "promoted entry {id} does not match its sidecar digest"
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

    /// Remove one promoted entry (output plus sidecar), returning the physical
    /// bytes reclaimed. Idempotent: an absent entry reclaims nothing.
    pub fn remove(&mut self, id: &NodeId) -> Result<u64> {
        let mut reclaimed = 0u64;
        for path in [self.bytes_path(id), self.digest_path(id)] {
            match fs::metadata(&path) {
                Ok(meta) => reclaimed = reclaimed.saturating_add(meta.len()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(Error::io(format!("stat promoted entry {id}: {e}"))),
            }
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(Error::io(format!("removing promoted entry {id}: {e}"))),
            }
        }
        Ok(reclaimed)
    }

    /// Whether an output file exists for `id` (the sidecar is not checked).
    pub fn contains(&self, id: &NodeId) -> Result<bool> {
        Ok(self.bytes_path(id).exists())
    }

    /// Total physical promoted bytes (outputs plus sidecars), excluding temp files.
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

    /// Sum of **output** file lengths only (excluding the `.b3` sidecars). This is
    /// the unit the promotion byte budget is expressed in.
    pub fn output_bytes(&self) -> Result<u64> {
        let mut total = 0u64;
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || name.ends_with(".b3") {
                continue;
            }
            let meta = entry.metadata()?;
            if meta.is_file() {
                total = total.saturating_add(meta.len());
            }
        }
        Ok(total)
    }

    /// Remove every promoted entry, returning the physical bytes reclaimed.
    /// Idempotent.
    pub fn clear(&mut self) -> Result<u64> {
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

/// One work-unit is a microsecond-equivalent. Monotone, bounded, integer-only.
fn kind_weight(kind: NodeKind) -> u64 {
    match kind {
        NodeKind::PdfStreamDecoded | NodeKind::PackageMemberDecoded => 4, // inflate
        NodeKind::PackageOpcModel
        | NodeKind::DocxModel
        | NodeKind::DocxStory
        | NodeKind::EpubModel
        | NodeKind::EpubContent
        | NodeKind::OdtModel
        | NodeKind::OdtContent => 8, // XML parse
        NodeKind::ContentOperators | NodeKind::TextRuns | NodeKind::PagePreview => 2,
        _ => 1,
    }
}

/// The estimated cost of materializing a node output: the (reserved) measured
/// time floored by the deterministic `bytes x kind_weight` estimate.
fn cost_of(kind: NodeKind, bytes: u64, measured_micros: u64) -> u64 {
    measured_micros.max(bytes.saturating_mul(kind_weight(kind)))
}

/// One tracked node in the governor. The content-addressed `NodeId` is the map
/// key, so it is not duplicated here; the kind's cost weight is folded into
/// `cost_units` at record time.
#[derive(Debug, Clone, Copy)]
struct PromoEntry {
    /// Output length in bytes.
    bytes: u64,
    /// Estimated materialization cost in work-units.
    cost_units: u64,
    /// Reuse events observed.
    hits: u32,
    /// The event clock at which this entry was first seen.
    born: u64,
    /// The event clock at which this entry was last seen.
    last_seen: u64,
    /// Whether this entry currently lives in the durable store.
    promoted: bool,
}

#[derive(Default)]
struct GovernorInner {
    policy: PromotePolicy,
    /// Monotone event clock: +1 on every observed cache event.
    now: u64,
    entries: HashMap<NodeId, PromoEntry>,
    /// Output bytes currently held in the durable store.
    promoted_bytes: u64,
    /// Whether the durable byte count has been reconciled from disk once.
    initialized: bool,
}

/// The promotion policy engine (Phase 15.6).
///
/// `Rc<RefCell<..>>` exactly like `ModelMemo` (`observe.rs`), so one store can
/// share a single governor across many observations.
#[derive(Clone, Default)]
pub struct Governor {
    inner: Rc<RefCell<GovernorInner>>,
}

impl std::fmt::Debug for Governor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = self.inner.borrow();
        f.debug_struct("Governor")
            .field("policy", &inner.policy)
            .field("now", &inner.now)
            .field("entries", &inner.entries.len())
            .field("promoted_bytes", &inner.promoted_bytes)
            .finish()
    }
}

impl Governor {
    /// A governor with an explicit policy.
    pub fn with_policy(policy: PromotePolicy) -> Self {
        let g = Governor::default();
        g.inner.borrow_mut().policy = policy;
        g
    }

    /// The active policy.
    pub fn policy(&self) -> PromotePolicy {
        self.inner.borrow().policy
    }

    /// Replace the active policy (a caller opting in after the store was opened).
    pub fn set_policy(&self, policy: PromotePolicy) {
        self.inner.borrow_mut().policy = policy;
    }

    /// Whether promotion is enabled at all.
    pub fn enabled(&self) -> bool {
        self.inner.borrow().policy.enabled
    }

    /// Whether the durable byte count has been reconciled from disk once.
    fn begin(&self) -> bool {
        let mut inner = self.inner.borrow_mut();
        if inner.initialized {
            false
        } else {
            inner.initialized = true;
            true
        }
    }

    fn set_promoted_bytes(&self, n: u64) {
        self.inner.borrow_mut().promoted_bytes = n;
    }

    fn promoted_bytes(&self) -> u64 {
        self.inner.borrow().promoted_bytes
    }

    fn entry(&self, id: &NodeId) -> Option<PromoEntry> {
        self.inner.borrow().entries.get(id).copied()
    }

    fn mark_promoted(&self, id: &NodeId) {
        let mut inner = self.inner.borrow_mut();
        let Some(e) = inner.entries.get(id) else {
            return;
        };
        if e.promoted {
            return;
        }
        let bytes = e.bytes;
        if let Some(e) = inner.entries.get_mut(id) {
            e.promoted = true;
        }
        inner.promoted_bytes = inner.promoted_bytes.saturating_add(bytes);
    }

    fn mark_evicted(&self, id: &NodeId) {
        let mut inner = self.inner.borrow_mut();
        let Some(e) = inner.entries.get(id) else {
            return;
        };
        if !e.promoted {
            return;
        }
        let bytes = e.bytes;
        if let Some(e) = inner.entries.get_mut(id) {
            e.promoted = false;
        }
        inner.promoted_bytes = inner.promoted_bytes.saturating_sub(bytes);
    }

    /// Record that a node was computed and stored (a cache miss). The event clock
    /// advances; the entry's reuse count is preserved if it already exists.
    fn record_stored(&self, kind: NodeKind, id: &NodeId, bytes: u64, wall_micros: u64) {
        let mut inner = self.inner.borrow_mut();
        inner.now = inner.now.saturating_add(1);
        let now = inner.now;
        let cost = cost_of(kind, bytes, wall_micros);
        let entry = inner.entries.entry(*id).or_insert(PromoEntry {
            bytes,
            cost_units: cost,
            hits: 0,
            born: now,
            last_seen: now,
            promoted: false,
        });
        entry.bytes = bytes;
        entry.cost_units = cost;
        entry.last_seen = now;
    }

    /// Record that a node was served from the cache (a hit). Returns the updated
    /// entry so the caller can evaluate `should_promote` without a second lock.
    fn credit_hit(&self, id: &NodeId) -> Option<PromoEntry> {
        let mut inner = self.inner.borrow_mut();
        inner.now = inner.now.saturating_add(1);
        let now = inner.now;
        let entry = inner.entries.get_mut(id)?;
        entry.hits = entry.hits.saturating_add(1);
        entry.last_seen = now;
        Some(*entry)
    }

    /// The policy function: promote iff the expected future saved work strictly
    /// exceeds the materialization cost plus the storage rent over the horizon.
    fn should_promote(&self, e: &PromoEntry) -> bool {
        let inner = self.inner.borrow();
        should_promote(&inner.policy, e, inner.now)
    }

    /// Plan the least-recently-seen promoted entries to evict so that `need_bytes`
    /// more can be stored within budget.
    fn eviction_plan(&self, need_bytes: u64) -> Vec<NodeId> {
        let inner = self.inner.borrow();
        let budget = inner.policy.budget_bytes;
        let mut used = inner.promoted_bytes;
        if used.saturating_add(need_bytes) <= budget {
            return Vec::new();
        }
        let mut victims: Vec<(&NodeId, u64)> = inner
            .entries
            .iter()
            .filter(|(_, e)| e.promoted)
            .map(|(id, e)| (id, e.last_seen))
            .collect();
        victims.sort_by_key(|(_, seen)| *seen);
        let mut plan = Vec::new();
        for (id, _) in victims {
            if used.saturating_add(need_bytes) <= budget {
                break;
            }
            plan.push(*id);
            used = used.saturating_sub(inner.entries.get(id).map_or(0, |e| e.bytes));
        }
        plan
    }
}

/// Promote iff expected future saved work **strictly** exceeds materialization
/// cost plus storage rent over the horizon. Saturating integer arithmetic.
fn should_promote(p: &PromotePolicy, e: &PromoEntry, now: u64) -> bool {
    if !p.enabled {
        return false;
    }
    if e.bytes > p.max_node_bytes {
        return false; // never a giant final
    }
    if e.hits < p.min_hits {
        return false; // require real reuse
    }
    let age = now.saturating_sub(e.born).max(1);
    let future_hits = (e.hits as u64).saturating_mul(p.horizon) / age;
    let saved = future_hits.saturating_mul(e.cost_units);
    let rent = e
        .bytes
        .saturating_mul(p.rent_per_byte)
        .saturating_mul(p.horizon);
    saved > e.cost_units.saturating_add(rent)
}

/// An [`OutputCache`] decorator: the durable promoted store is consulted first,
/// then the mutable disposable [`DerivedCache`]. Promotion is decided in
/// [`OutputCache::note`].
pub struct GovernedCache {
    ephemeral: DerivedCache,
    promoted: PromotedStore,
    gov: Governor,
}

impl std::fmt::Debug for GovernedCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GovernedCache")
            .field("promoted", &self.promoted)
            .field("gov", &self.gov)
            .finish_non_exhaustive()
    }
}

impl GovernedCache {
    /// Open the governed cache over a store root: the disposable `cache/` plus the
    /// durable `promoted/`, sharing `gov`. On the first open of a governor the
    /// durable byte count is reconciled from disk; a store already over budget is
    /// cleared wholesale (the coarse bound `ModelMemo` uses).
    pub fn open(store_root: impl AsRef<Path>, gov: Governor) -> Result<Self> {
        let root = store_root.as_ref();
        let ephemeral = DerivedCache::open(root.join("cache"))?;
        let mut promoted = PromotedStore::open(root.join("promoted"))?;
        if gov.begin() {
            let on_disk = promoted.output_bytes()?;
            if on_disk > gov.policy().budget_bytes {
                promoted.clear()?;
                gov.set_promoted_bytes(0);
            } else {
                gov.set_promoted_bytes(on_disk);
            }
        }
        Ok(GovernedCache {
            ephemeral,
            promoted,
            gov,
        })
    }

    /// The durable promoted store (for courts and stats).
    pub fn promoted(&self) -> &PromotedStore {
        &self.promoted
    }

    /// Attempt to promote one entry, if the policy decides and it fits the budget.
    /// Best-effort: any I/O failure leaves the ephemeral cache (and the answer)
    /// untouched. The verified bytes are read back from the disposable cache.
    fn maybe_promote(&mut self, id: &NodeId) {
        let Some(entry) = self.gov.entry(id) else {
            return;
        };
        if entry.promoted || entry.bytes > self.gov.policy().max_node_bytes {
            return;
        }
        if !self.gov.should_promote(&entry) {
            return;
        }
        let Ok(Some(bytes)) = self.ephemeral.get(id) else {
            return;
        };
        let need = bytes.len() as u64;
        if need > self.gov.policy().budget_bytes {
            return;
        }
        for victim in self.gov.eviction_plan(need) {
            if self.promoted.remove(&victim).is_ok() {
                self.gov.mark_evicted(&victim);
            }
        }
        if self.gov.promoted_bytes().saturating_add(need) > self.gov.policy().budget_bytes {
            return;
        }
        if self.promoted.put(id, &bytes).is_ok() {
            self.gov.mark_promoted(id);
        }
    }
}

impl OutputCache for GovernedCache {
    fn get(&self, id: &NodeId) -> Result<Option<Vec<u8>>> {
        // Durable first. A promoted `get` error fails closed (the DAG treats any
        // error as a miss); it never returns wrong bytes.
        match self.promoted.get(id) {
            Ok(Some(bytes)) => return Ok(Some(bytes)),
            Ok(None) => {}
            Err(e) => return Err(e),
        }
        self.ephemeral.get(id)
    }

    fn put(&mut self, id: &NodeId, bytes: &[u8]) -> Result<()> {
        // The disposable cache is always filled, exactly as before.
        self.ephemeral.put(id, bytes)?;
        Ok(())
    }

    fn note(&mut self, note: CacheNote<'_>) {
        if !self.gov.enabled() {
            return;
        }
        match note.effect {
            CacheEffect::Stored => {
                self.gov
                    .record_stored(note.kind, note.id, note.bytes, note.wall_micros)
            }
            CacheEffect::Hit => {
                self.gov.credit_hit(note.id);
                self.maybe_promote(note.id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::dag::{CacheEffect, CacheNote, OutputCache};

    fn temp_root(label: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "vole-promote-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        p
    }

    fn stored(kind: NodeKind, id: &NodeId, bytes: u64) -> CacheNote<'_> {
        CacheNote {
            effect: CacheEffect::Stored,
            kind,
            id,
            bytes,
            wall_micros: 0,
        }
    }

    fn hit(kind: NodeKind, id: &NodeId, bytes: u64) -> CacheNote<'_> {
        CacheNote {
            effect: CacheEffect::Hit,
            kind,
            id,
            bytes,
            wall_micros: 0,
        }
    }

    fn promoted_policy(min_hits: u32, budget_bytes: u64) -> PromotePolicy {
        PromotePolicy {
            enabled: true,
            budget_bytes,
            min_hits,
            rent_per_byte: 0,
            ..PromotePolicy::default()
        }
    }

    #[test]
    fn promotion_is_disabled_by_default_writes_nothing() {
        let root = temp_root("off");
        let gov = Governor::default();
        assert!(!gov.enabled());
        let mut cache = GovernedCache::open(&root, gov).unwrap();
        let id = NodeId::from_bytes([3u8; 32]);
        cache.put(&id, b"bytes").unwrap();
        cache.note(stored(NodeKind::Literal, &id, 5));
        cache.note(hit(NodeKind::Literal, &id, 5));
        // Nothing durable was written, but the disposable cache still works.
        assert_eq!(cache.promoted.total_bytes().unwrap(), 0);
        assert_eq!(cache.get(&id).unwrap().as_deref(), Some(&b"bytes"[..]));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_promoted_hit_is_served_after_the_ephemeral_cache_is_cleared() {
        let root = temp_root("serve");
        let gov = Governor::with_policy(promoted_policy(1, 1 << 20));
        let mut cache = GovernedCache::open(&root, gov).unwrap();
        let id = NodeId::from_bytes([7u8; 32]);
        let cold = b"promoted intermediate output".to_vec();
        cache.put(&id, &cold).unwrap();
        cache.note(stored(
            NodeKind::PackageMemberDecoded,
            &id,
            cold.len() as u64,
        ));
        // One reuse event is enough under this policy: the entry is promoted.
        cache.note(hit(NodeKind::PackageMemberDecoded, &id, cold.len() as u64));
        assert!(cache.promoted.contains(&id).unwrap());
        // Drop the disposable cache entirely; only the durable store remains.
        cache.ephemeral.clear().unwrap();
        assert_eq!(cache.ephemeral.get(&id).unwrap(), None);
        assert_eq!(cache.get(&id).unwrap(), Some(cold));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_promoted_entry_for_a_changed_id_is_a_miss() {
        let root = temp_root("changed");
        let gov = Governor::with_policy(promoted_policy(1, 1 << 20));
        let mut cache = GovernedCache::open(&root, gov).unwrap();
        let id = NodeId::from_bytes([1u8; 32]);
        cache.put(&id, b"aaa").unwrap();
        cache.note(stored(NodeKind::DocxStory, &id, 3));
        cache.note(hit(NodeKind::DocxStory, &id, 3));
        assert!(cache.promoted.contains(&id).unwrap());
        // A changed dependency yields a different NodeId: simply absent.
        let other = NodeId::from_bytes([2u8; 32]);
        assert_eq!(cache.get(&other).unwrap(), None);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn eviction_respects_the_byte_budget() {
        let root = temp_root("evict");
        // Budget holds one 3-byte output but not two.
        let gov = Governor::with_policy(promoted_policy(1, 5));
        let mut cache = GovernedCache::open(&root, gov).unwrap();
        let a = NodeId::from_bytes([1u8; 32]);
        let b = NodeId::from_bytes([2u8; 32]);
        cache.put(&a, b"aaa").unwrap();
        cache.note(stored(NodeKind::Literal, &a, 3));
        cache.note(hit(NodeKind::Literal, &a, 3));
        assert!(cache.promoted.contains(&a).unwrap());
        cache.put(&b, b"bbb").unwrap();
        cache.note(stored(NodeKind::Literal, &b, 3));
        cache.note(hit(NodeKind::Literal, &b, 3));
        // `a` is the least-recently-seen promoted entry: it is evicted to fit `b`.
        assert!(cache.promoted.contains(&b).unwrap());
        assert!(!cache.promoted.contains(&a).unwrap());
        assert!(cache.promoted.output_bytes().unwrap() <= 5);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn oversized_nodes_are_never_promoted() {
        let root = temp_root("oversize");
        let gov = Governor::with_policy(PromotePolicy {
            max_node_bytes: 4,
            ..promoted_policy(1, 1 << 20)
        });
        let mut cache = GovernedCache::open(&root, gov).unwrap();
        let id = NodeId::from_bytes([5u8; 32]);
        cache.put(&id, b"giant final answer").unwrap();
        cache.note(stored(NodeKind::DocumentExact, &id, 18));
        cache.note(hit(NodeKind::DocumentExact, &id, 18));
        assert_eq!(cache.promoted.total_bytes().unwrap(), 0);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn poisoned_promoted_bytes_fail_closed_never_wrong() {
        let root = temp_root("poison");
        let mut store = PromotedStore::open(&root).unwrap();
        let id = NodeId::from_bytes([9u8; 32]);
        store.put(&id, b"correct bytes").unwrap();
        // Corrupt the output but keep the sidecar: get must fail closed.
        fs::write(store.bytes_path(&id), b"wrong!!").unwrap();
        assert_eq!(
            store.get(&id).unwrap_err().class(),
            crate::ErrorClass::IntegrityMismatch
        );
        fs::remove_dir_all(&root).ok();
    }
}
