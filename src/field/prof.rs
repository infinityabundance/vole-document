//! Env-gated warm-session *stage* profiler (Phase 22.2 review item P1).
//!
//! Measurement scaffolding only: it never changes an observation, a byte, a
//! decision, or the on-disk layout, and it is **off by default**. The enable gate
//! is cached in a `OnceLock`, so a default run pays exactly one `getenv` per
//! process (plus a relaxed load per instrumentation site) and nothing else. When
//! `VOLE_PROFILE_OPEN` is set, a thread-local accumulator records the microseconds
//! spent in each stage of one observation so the harness can attribute the warm
//! cost.
//!
//! Stages (see `docs/phases/phase-22-2-results.md` for the interpretation):
//!
//! - `probe_us` — selector resolution: the cache-first narrow probe, including the
//!   index descent it performs (`lookup`).
//! - `index_read_us` — one index-node fetch: `fs::read` + BLAKE3 `NodeId` verify.
//! - `index_parse_us` — index-node framing/decode (`parse_node`).
//! - `index_nodes` — number of index nodes physically read.
//! - `lookup_calls` — number of index descents (`lookup`) performed.
//! - `dispatch_us` — the whole evaluation core (`Ctx::dispatch`), which contains
//!   materialization, typed-model access, and answer construction.
//! - `materialize_us` — the subset of `dispatch_us` spent decoding procedural seed
//!   nodes (and the typed models an adapter parses from them) via the DAG
//!   materializer.
//! - `observe_us` — the whole `observe_session` call (probe + dispatch).

use std::cell::Cell;
use std::sync::OnceLock;
use std::time::Instant;

/// Per-observation stage accumulators. All values are microseconds unless noted.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub(crate) struct Stages {
    pub probe_us: u64,
    pub index_read_us: u64,
    pub index_parse_us: u64,
    pub index_nodes: u64,
    pub lookup_calls: u64,
    pub dispatch_us: u64,
    pub materialize_us: u64,
    pub observe_us: u64,
}

impl Stages {
    const ZERO: Stages = Stages {
        probe_us: 0,
        index_read_us: 0,
        index_parse_us: 0,
        index_nodes: 0,
        lookup_calls: 0,
        dispatch_us: 0,
        materialize_us: 0,
        observe_us: 0,
    };
}

thread_local! {
    static STAGES: Cell<Stages> = const { Cell::new(Stages::ZERO) };
}

/// Whether the stage profiler is enabled (the `VOLE_PROFILE_OPEN` gate).
///
/// Cached in a `OnceLock`, so the default (disabled) path pays exactly ONE
/// `getenv` per process and thereafter a relaxed atomic load — not one env lookup
/// per instrumentation site. This keeps the measured binary's off-path cost
/// negligible.
#[inline]
pub(crate) fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(super::warm_prof_enabled)
}

fn update(f: impl FnOnce(&mut Stages)) {
    STAGES.with(|s| {
        let mut v = s.get();
        f(&mut v);
        s.set(v);
    });
}

/// Start a timed region only when the profiler is enabled; otherwise `None`
/// (and the matching `add_*` below is a no-op).
#[inline]
pub(crate) fn start() -> Option<Instant> {
    enabled().then(Instant::now)
}

/// Zero the accumulator. Called at the start of a profiled observation.
pub(crate) fn reset() {
    if enabled() {
        STAGES.with(|s| s.set(Stages::ZERO));
    }
}

/// Read and clear the accumulator (per-observation deltas).
pub(crate) fn take() -> Stages {
    STAGES.with(|s| {
        let v = s.get();
        s.set(Stages::ZERO);
        v
    })
}

#[inline]
pub(crate) fn add_probe(t: Option<Instant>) {
    if let Some(t) = t {
        update(|v| v.probe_us = v.probe_us.saturating_add(us(t)));
    }
}

#[inline]
pub(crate) fn add_index_read(t: Option<Instant>) {
    if let Some(t) = t {
        update(|v| v.index_read_us = v.index_read_us.saturating_add(us(t)));
    }
}

#[inline]
pub(crate) fn add_index_parse(t: Option<Instant>) {
    if let Some(t) = t {
        update(|v| v.index_parse_us = v.index_parse_us.saturating_add(us(t)));
    }
}

#[inline]
pub(crate) fn inc_index_node() {
    if enabled() {
        update(|v| v.index_nodes = v.index_nodes.saturating_add(1));
    }
}

#[inline]
pub(crate) fn inc_lookup() {
    if enabled() {
        update(|v| v.lookup_calls = v.lookup_calls.saturating_add(1));
    }
}

#[inline]
pub(crate) fn add_dispatch(t: Option<Instant>) {
    if let Some(t) = t {
        update(|v| v.dispatch_us = v.dispatch_us.saturating_add(us(t)));
    }
}

#[inline]
pub(crate) fn add_materialize(t: Option<Instant>) {
    if let Some(t) = t {
        update(|v| v.materialize_us = v.materialize_us.saturating_add(us(t)));
    }
}

#[inline]
pub(crate) fn add_observe(t: Option<Instant>) {
    if let Some(t) = t {
        update(|v| v.observe_us = v.observe_us.saturating_add(us(t)));
    }
}

#[inline]
fn us(t: Instant) -> u64 {
    t.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
}
