# Phase 16 item 4 — resident session + `narrow_probe` short-circuit

**Verdict: NEGATIVE.** Giving the resident `DocumentFieldSession` the cold path's
cache-first short-circuit removes the per-observation cost but does **not** make
residency beat cold on this corpus. Under the frozen court's rule (fastest median,
±10% tie band) resident wins only below ~1 MiB — exactly the strata it already won
in `2026-10-07-real100-release-resident-78f7ea8`.

## The seam

`src/field/observe.rs`:

- `probe_eligible` — the shared eligibility test (identical to the old inline
  check: `use_cache` ∧ `store.supports_partial_descriptor()` ∧ `(Page,
  Text|Preview|Structure) | (Stream, DecodedBytes|Operators)`).
- `narrow_probe_core` — the probe body, refactored to take an already-parsed
  `&FieldRoot` manifest and an already-open `&FsIndexStore`.
- `narrow_probe` — the **cold** wrapper (unchanged behaviour): reads the manifest,
  opens the index store, then calls the core.
- `narrow_probe_open` — the probe from a manifest/index the caller already holds.
- `observe_session` — the resident entry point (replaces
  `observe_with_field_memo`). On a **hit** it serves the derived node from the
  disposable cache via a trip-wire `NO_SOURCE` view, never opening the
  descriptor. On a **miss** or an **ineligible** request it falls through to the
  ordinary evaluation core against the already-open `Field`.
- `open_session_index` — opens the session's index store once.

`src/field/session.rs`: `DocumentFieldSession` gained an `index: FsIndexStore`
field, opened once in `open()`. `observe()` is:

```rust
let open_io = self.pending_open_io.take().unwrap_or_default();
observe_session(&mut self.store, &self.field, &self.index, open_io, req, limits, self.models.clone())
```

**Hoisting:** the probe uses the manifest of the already-open `Field` (no manifest
re-read) and a single `FsIndexStore` kept open across the whole session (no
per-observation index re-open). `Ctx.istore` became a borrow so the evaluation
core reuses that same open store. The cold `observe` path is untouched.

## Answer equality

- Unit test `session_probe_matches_narrow_probe_short_circuit`: against a warm
  cache, cold `narrow_probe` and the resident session return the same
  `FieldAnswer`, and both report `descriptor_read_mode == Partial`.
  `session_answers_equal_cold_process_answers` and
  `session_reads_descriptor_once` still pass.
- Real corpus (`raw/answer-equality.txt`): cold `observe --page 1 --kind text`
  vs resident `observe-batch` — **90 equal / 0 mismatch** (10 declines/missing).

## The measurement

Trimmed copy of the frozen court (`tools/phase16-resident-probe-court.sh`) — same
100-doc population, same encode + field-ingest, same cold-vs-resident
`text_repeat` semantics. `v` = 5 separate cold processes; `v_r` = one
`observe-batch` doing 5 observations. Frozen court files unmodified.

Wall ms, answered docs (rc==0). Verdict uses the frozen court's rule (fastest
median, ±10% tie band); "cold wins" is over documents.

| size class | n | cold `v` med | resident med | cold sum | resident sum | cold wins | verdict |
|---|---:|---:|---:|---:|---:|---:|---|
| <100KiB | 8 | 6.5 | 1.0 | 55 | 9 | 0/8 | resident |
| 100KiB-1MiB | 15 | 7.0 | 2.0 | 119 | 26 | 0/15 | resident |
| 1-10MiB | 38 | 6.0 | 8.0 | 740 | 350 | 16/38 | cold by median; resident by sum |
| 10-50MiB | 16 | 6.0 | 41.0 | 2103 | 790 | 9/16 | cold by median; resident by sum |
| 50-100MiB | 10 | 6.0 | 128.5 | 59 | 1280 | 10/10 | cold |
| >100MiB | 3 | 5.0 | 370.0 | 16 | 1439 | 3/3 | cold |
| **all** | **90** | **6.0** | **9.0** | **3092** | **3894** | **38/90** | **cold** |

The 1-10MiB and 10-50MiB strata are genuinely mixed: the resident session wins
those strata **by sum** (and on the EPUB documents, whose `--block 0` text is
*not* probe-eligible, so residency memoizes the full evaluation across 5
observations), while the many small PDFs drag the *median* to cold. Overall the
resident lane is faster on 52/90 documents but cold wins the median and the sum,
because the two largest strata (50-100MiB, >100MiB — all PDFs, where the probe is
eligible) are pure cold wins.

Against the previous receipt (`...-resident-78f7ea8`), the resident lane improved
slightly on the probe-eligible large-PDF strata — 50-100MiB median 136.5 → 128.5,
10-50MiB sum 816 → 790, 1-10MiB sum 363 → 350 — but **no class flipped**. The
`>100MiB` median rose (353.5 → 370.0) but `n` changed 2 → 3 (phase16.3 fixed that
pathology), so it is not comparable.

## Why the probe did not flip it

The probe works exactly as intended: observations 2..N of each resident batch
report `descriptor_bytes_read = 0`, `descriptor_read_mode = "partial"`, ~25 µs
each (208/360 such hits).

The residual cost is the **one-time session open**. `raw/mechanism.txt`, on
`nasa-pdf-0004` (76,790,177-byte descriptor):

```
observe-batch repeat=1: 0.14 s      observe-batch repeat=5: 0.14 s
observe (cold, single): 0.00 s
```

A one-observation batch costs the same as a five-observation batch: the whole
cost is `DocumentFieldSession::open` → `Field::open`, which reads *and parses the
entire descriptor*. The cold path's `narrow_probe` never opens the descriptor at
all. One full parse of a tens-of-MB descriptor exceeds five probe-only process
spawns.

## Identified, out of scope

A lazy session open (open only the manifest at session start; load the descriptor
only when a probe misses) is the change that could make residency pay for large
descriptors. The user scoped this experiment to giving the session the
short-circuit and said not to reopen the broader residency thesis, so it is
recorded here and not attempted.

## Gates

`cargo fmt --all --check` pass · `cargo clippy --all-targets --all-features --
-D warnings` pass · `cargo test --locked --all-features` pass (468 lib +
integration, 0 failed) · `cargo test --locked --no-default-features` pass.
