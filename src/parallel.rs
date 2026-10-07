//! Bounded CPU parallelism for the naturally independent ingest work
//! (Phase 15.4).
//!
//! The pool is compiled **only** with the non-default `parallel` feature (which
//! implies `field`). Without that feature [`WorkerPool`] is a zero-sized marker
//! that is never constructed, and every ingest site takes its serial path; the
//! exact core therefore keeps no new dependency and no behavior change.
//!
//! ## The one architectural rule
//!
//! A worker may only run a **pure** map over immutable inputs. It must never
//! touch a [`FieldStore`](crate::field::FieldStore), its `Rc`-based
//! [`IoCounters`](crate::store::IoCounters), a derived-cache memo, or the disk.
//! The store's single-threaded accounting is the fuse that keeps parallelism
//! honest: the parallel phase computes owned canonical bytes and [`NodeId`]s,
//! and a *serial* fold merges them in physical order.
//!
//! Determinism is therefore structural, not a tuning property. Indexed
//! `par_iter().collect()` preserves physical order, and every order-dependent
//! gate (the running `MAX_TOTAL_DECODED` / `MAX_OBJSTM*` caps, the existence
//! probes and counters) is replayed in that same serial fold — so the parallel
//! path yields byte-identical nodes, ids, index entries, and limits enforcement
//! to the serial path.
//!
//! [`NodeId`]: crate::store::NodeId

#[cfg(feature = "parallel")]
use crate::error::{Error, Result};

/// A bounded worker pool. `workers == 1` is a valid (serial) pool, though the
/// ingest sites take their serial branch directly rather than paying to build it.
#[cfg(feature = "parallel")]
pub struct WorkerPool {
    pool: rayon::ThreadPool,
    workers: usize,
}

#[cfg(feature = "parallel")]
impl WorkerPool {
    /// Build a pool of exactly `workers` threads, or return a typed error.
    ///
    /// `workers == 0` is rejected here: the "auto" policy (`available_parallelism`)
    /// is resolved explicitly by the CLI, never silently by the library.
    pub fn new(workers: usize) -> Result<Self> {
        if workers == 0 {
            return Err(Error::usage("a worker pool needs at least one worker"));
        }
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .thread_name(|i| format!("vole-worker-{i}"))
            .build()
            .map_err(|e| Error::internal_invariant(format!("worker pool build failed: {e}")))?;
        Ok(WorkerPool { pool, workers })
    }

    /// The number of threads in the pool.
    pub fn workers(&self) -> usize {
        self.workers
    }

    /// Run `f` on the pool and block until it returns. The rayon thread-local
    /// context is scoped to `f`.
    pub fn install<R: Send>(&self, f: impl FnOnce() -> R + Send) -> R {
        self.pool.install(f)
    }
}

/// Without the `parallel` feature this is a zero-sized marker that is never
/// constructed; the ingest sites keep their serial path (`pool` is always `None`).
#[cfg(not(feature = "parallel"))]
pub struct WorkerPool;
