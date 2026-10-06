//! Physical I/O accounting for observations (Phase 11.9 review fix #1).
//!
//! An observation is only honest about its cost if every blob it makes the OS
//! read is counted. Before this module, [`ObserveStats`] counted only the
//! procedural *seed* universe while the observation silently also read the whole
//! descriptor blob and its manifest. This is a small, shared, per-process set of
//! byte counters — one per physical read class — so a caller can attribute the
//! bytes actually fetched to exactly one observation.
//!
//! The counters are interior-mutable ([`Cell`]) and shareable via [`Rc`], so a
//! store handle can hand a second handle to a sub-store (seed/index) and still
//! see every read. They are **not** `Sync`; a field store is a single-threaded
//! object, which is how the courts already use it.
//!
//! These counters measure bytes *fetched from the filesystem by this process*.
//! They do not measure page-cache hits, mmap'd libraries, or CPU time; those are
//! reported separately (or not at all) elsewhere.

use std::cell::Cell;
use std::rc::Rc;

/// A shared set of physical-read byte counters, one per class.
///
/// Cloning shares the same counters; use [`IoCounters::handle`] to make the
/// sharing explicit.
#[derive(Debug, Clone, Default)]
pub struct IoCounters {
    inner: Rc<IoInner>,
}

#[derive(Debug, Default)]
struct IoInner {
    descriptor_bytes: Cell<u64>,
    manifest_bytes: Cell<u64>,
    index_bytes: Cell<u64>,
    seed_bytes: Cell<u64>,
    descriptor_reads: Cell<u64>,
}

impl IoCounters {
    /// A fresh, independent counter set.
    pub fn new() -> Self {
        IoCounters {
            inner: Rc::new(IoInner::default()),
        }
    }

    /// A second handle that shares this counter set.
    pub fn handle(&self) -> Self {
        IoCounters {
            inner: Rc::clone(&self.inner),
        }
    }

    /// Note a descriptor blob read of `bytes` bytes.
    pub fn add_descriptor(&self, bytes: u64) {
        self.inner
            .descriptor_bytes
            .set(self.inner.descriptor_bytes.get().saturating_add(bytes));
        self.inner
            .descriptor_reads
            .set(self.inner.descriptor_reads.get().saturating_add(1));
    }

    /// Note a field manifest read of `bytes` bytes.
    pub fn add_manifest(&self, bytes: u64) {
        self.inner
            .manifest_bytes
            .set(self.inner.manifest_bytes.get().saturating_add(bytes));
    }

    /// Note a hierarchical index node read of `bytes` bytes.
    pub fn add_index(&self, bytes: u64) {
        self.inner
            .index_bytes
            .set(self.inner.index_bytes.get().saturating_add(bytes));
    }

    /// Note a seed node read of `bytes` bytes.
    pub fn add_seed(&self, bytes: u64) {
        self.inner
            .seed_bytes
            .set(self.inner.seed_bytes.get().saturating_add(bytes));
    }

    /// Number of descriptor blobs fetched so far (a read *count*, not bytes).
    pub fn descriptor_reads(&self) -> u64 {
        self.inner.descriptor_reads.get()
    }

    /// A consistent snapshot of every counter.
    pub fn snapshot(&self) -> IoSnapshot {
        IoSnapshot {
            descriptor_bytes: self.inner.descriptor_bytes.get(),
            manifest_bytes: self.inner.manifest_bytes.get(),
            index_bytes: self.inner.index_bytes.get(),
            seed_bytes: self.inner.seed_bytes.get(),
        }
    }
}

/// A point-in-time copy of [`IoCounters`]. Subtract two to attribute bytes to an
/// interval.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IoSnapshot {
    /// Descriptor-blob bytes read.
    pub descriptor_bytes: u64,
    /// Field-manifest bytes read.
    pub manifest_bytes: u64,
    /// Hierarchical-index-node bytes read.
    pub index_bytes: u64,
    /// Seed-node bytes read.
    pub seed_bytes: u64,
}

impl IoSnapshot {
    /// Bytes fetched in the interval `(self, later]`, saturating at zero so a
    /// counter reset can never underflow into a fabricated huge number.
    pub fn delta(&self, later: &IoSnapshot) -> IoSnapshot {
        IoSnapshot {
            descriptor_bytes: later.descriptor_bytes.saturating_sub(self.descriptor_bytes),
            manifest_bytes: later.manifest_bytes.saturating_sub(self.manifest_bytes),
            index_bytes: later.index_bytes.saturating_sub(self.index_bytes),
            seed_bytes: later.seed_bytes.saturating_sub(self.seed_bytes),
        }
    }

    /// The total physical bytes across every class.
    pub fn total(&self) -> u64 {
        self.descriptor_bytes
            .saturating_add(self.manifest_bytes)
            .saturating_add(self.index_bytes)
            .saturating_add(self.seed_bytes)
    }

    /// Add `other` class-by-class.
    pub fn plus(&self, other: &IoSnapshot) -> IoSnapshot {
        IoSnapshot {
            descriptor_bytes: self.descriptor_bytes.saturating_add(other.descriptor_bytes),
            manifest_bytes: self.manifest_bytes.saturating_add(other.manifest_bytes),
            index_bytes: self.index_bytes.saturating_add(other.index_bytes),
            seed_bytes: self.seed_bytes.saturating_add(other.seed_bytes),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_share_and_delta_sums_correctly() {
        let a = IoCounters::new();
        let b = a.handle();
        a.add_descriptor(100);
        b.add_manifest(10);
        b.add_index(7);
        b.add_seed(3);
        let s = a.snapshot();
        assert_eq!(s.descriptor_bytes, 100);
        assert_eq!(s.manifest_bytes, 10);
        assert_eq!(s.index_bytes, 7);
        assert_eq!(s.seed_bytes, 3);
        assert_eq!(s.total(), 120);
        assert_eq!(a.descriptor_reads(), 1);
        assert_eq!(b.descriptor_reads(), 1);
    }

    #[test]
    fn independent_counters_do_not_leak() {
        let a = IoCounters::new();
        let b = IoCounters::new();
        a.add_seed(5);
        assert_eq!(b.snapshot().total(), 0);
    }

    #[test]
    fn delta_is_non_negative_and_additive() {
        let c = IoCounters::new();
        let before = c.snapshot();
        c.add_descriptor(4);
        c.add_seed(9);
        let after = c.snapshot();
        let d = before.delta(&after);
        assert_eq!(d.descriptor_bytes, 4);
        assert_eq!(d.seed_bytes, 9);
        assert_eq!(d.total(), 13);
        // A reversed interval saturates rather than underflowing.
        assert_eq!(after.delta(&before).total(), 0);
        assert_eq!(d.plus(&d).total(), 26);
    }
}
