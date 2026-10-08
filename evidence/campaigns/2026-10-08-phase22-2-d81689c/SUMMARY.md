# Phase 22.2 — warm-session profiling gate + the tuned-`full` warm court

**Verdict: the index/selector layer is NOT the warm-session bottleneck; nothing was
shipped structurally.** The dominant term is the one-time full `Descriptor::parse`
at field open (44.7% of the pooled warm session; 78–96% of the large-PDF sessions
and 33–65% of the large EPUB/DOCX sessions). Selector resolution is 0.4% and
index-node traversal is 10.6% pooled — and that 10.6% is itself dominated by
re-reading one immutable content-addressed leaf. A *perfect* selector directory
could remove at most 10.6% of the session, an implied shift of ~0.13 on the warm
median ratio, against a court MDE of ~0.40: **below resolution**. It would also add
persistent bytes. It is therefore not built.

This campaign holds two artefacts: the **profiling gate** (`raw/profile/`) and the
**paired before/after warm court** against the Phase-22.1 tuned `full` SQLite
envelope (`raw/court-summary.md`, `raw/warm_samples.tsv`, `raw/store_shape.tsv`).

## 1. Profiling gate — method

`tools/phase22-2-profile.sh` (NEW; read-only to every frozen court) builds each
document's **packed** store once, warms it, then attributes the warm
`observe-batch` session with five independent instruments:

1. the env-gated internal stage profiler (**extended** in
   `src/field/prof.rs`, `VOLE_PROFILE_OPEN=1`, off by default; the disabled path
   costs one `getenv` per process plus a `OnceLock` load per site) — open /
   manifest / descriptor read+parse / probe / index-read / index-decode /
   dispatch / materialize / serialize, with index node and descent counts;
2. `/usr/bin/time -v` — wall (fine `EPOCHREALTIME`), user, sys, peak RSS,
   minor/major page faults;
3. `strace -c -f` — syscall-time attribution; `strace -e trace=openat` — the
   physical open count per index node;
4. an allocation-counting `LD_PRELOAD` shim
   (`tools/fixtures/phase22-2-malloc-count.c`) — the crate forbids `unsafe`, so it
   cannot install a counting global allocator; exact allocation **count** and
   requested **bytes**;
5. the observation JSON byte classes (`bytes_read`, `index_bytes_read`, …).

## 2. Stage attribution (packed store, one warm session per document)

Wall/session figures are the last of five in-process runs (µs); the allocation,
RSS and fault columns are whole-process. Full table:
`raw/profile/attribution.md`; raw per-document stderr: `raw/profile/*.prof.err`.

| document | session µs | open % | `Descriptor::parse` % | loop % | probe % | index-read % | dispatch % | materialize % | serialize % | idx opens/reads | distinct idx nodes |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nist-pdf-0002 | 728 | 82.7 | 63.5 | 17.3 | 3.85 | 3.6 | 6.2 | 1.9 | 1.4 | 4/4 | 3 |
| nist-pdf-0004 | 2431 | 95.6 | 78.0 | 4.4 | 0.99 | 0.9 | 1.4 | 0.5 | 0.3 | 4/4 | 3 |
| nist-pdf-0016 | 737 | 88.3 | 71.5 | 11.7 | 2.99 | 2.2 | 3.0 | 0.9 | 1.1 | 4/4 | 3 |
| nist-pdf-0017 | 3250 | 94.4 | 77.5 | 5.6 | 0.77 | 1.0 | 3.2 | 0.9 | 0.3 | 4/4 | 3 |
| nist-docx-0005 | 605 | 27.3 | 16.7 | 72.7 | 0.00 | 9.9 | 38.7 | 5.5 | 18.8 | 24/24 | 1 |
| nist-docx-0008 | 388 | 27.3 | 13.9 | 72.7 | 0.00 | 15.7 | 36.6 | 5.2 | 11.1 | 24/24 | 1 |
| nist-docx-0009 | 1031 | 32.9 | 25.1 | 67.1 | 0.00 | 10.0 | 41.6 | 4.4 | 14.3 | 24/24 | 1 |
| nist-docx-0014 | 3013 | 65.3 | 53.8 | 34.7 | 0.00 | 4.2 | 22.3 | 2.1 | 10.3 | 24/24 | 1 |
| nist-epub-0003 | 2346 | 9.7 | 6.7 | 90.3 | 0.00 | 22.9 | 70.8 | 15.5 | 16.4 | 114/114 | 1 |
| nist-epub-0006 | 1654 | 15.9 | 11.2 | 84.1 | 0.00 | 24.1 | 70.3 | 16.6 | 10.0 | 90/90 | 1 |
| nist-epub-0008 | 840 | 15.2 | 8.2 | 84.8 | 0.00 | 23.6 | 71.5 | 18.8 | 6.3 | 60/60 | 1 |
| nist-epub-0009 | 5196 | 48.0 | 40.3 | 52.0 | 0.00 | 15.0 | 38.7 | 7.3 | 11.9 | 117/117 | 1 |

Pooled (sum of one warm session per document, **22,219 µs**):

| stage | µs | share |
|---|---:|---:|
| open (manifest + descriptor read + `Descriptor::parse`) | 12,334 | **55.5%** |
| — of which `Descriptor::parse` | 9,941 | **44.7%** |
| request loop | 9,885 | 44.5% |
| — dispatch (evaluation core) | 7,120 | 32.0% |
| — serialize (answer JSON) | 1,868 | 8.4% |
| — materialize (procedural node + typed model decode) | 1,397 | 6.3% |
| — probe (selector resolution) | 99 | **0.4%** |
| index node read + BLAKE3 verify | 2,327 | 10.5% |
| index node decode | 33 | 0.1% |

Whole-process resources (packed, `raw/profile/*.timev.txt`, `*.malloc.txt`,
`*.strace.txt`): wall 0–5 ms; user/sys < 0.01 s; peak RSS 3.7–7.1 MB; minor faults
202–1079; **major faults 0**; 12–15 syscall classes; allocations **433** (PDF) to
**18,727** (epub-0009), 0.5–9.2 MB requested. Process-start floor: `/bin/true`
0.6 MB RSS, the VOLE binary with no work 2.2 MB; startup is a material fraction of a
1–5 ms session and is charged to both lanes in the court.

Physical bytes read by class (summed from the retained warm `observe-batch` JSON,
`raw/env/C0`): descriptor **5,806,893 B**, index **1,625,456 B**, manifest
**3,650 B**, seed **49,806 B** — total **7,485,805 B**. The descriptor is read once
per session; the index bytes are dominated by the repeated leaf re-reads of §3
(**493** physical node reads across the 12 profiling sessions, §2).

## 3. Why the index term exists at all — redundant re-reads of one immutable leaf

**493 index-node opens for 20 distinct index-node files across the 12 sessions.**
`strace -e trace=openat` shows the text lanes open the *same* file once per
descent:

| document | index `openat` calls | distinct index files | the file |
|---|---:|---:|---|
| nist-epub-0009 | 117 | **1** | `index/85/06/85065a…` (the root leaf) |
| nist-docx-0008 | 24 | **1** | `index/33/69/3369a3…` (the root leaf) |
| nist-pdf-0017 | 4 | 3 | 3 distinct leaves |

`FsIndexStore` persists hash-addressed nodes as individual files
(`src/field/index.rs:349-356`) and `get` does `fs::read` + a BLAKE3 `NodeId`
verify per read (`:265-282`). Because these trees are depth-0 (a single leaf is
the root), every selector descent re-opens, re-reads and re-hashes the *same*
immutable node. The 10.6% index term is dominated by this redundancy, not by
lookup work itself (`parse_node` is 0.1%).

## 4. Headroom a perfect selector directory could remove

- pooled index share **10.6%**; median per-document share **10.0%**;
  per-lane: PDF **0.9–3.6%**, DOCX **4.2–15.7%**, EPUB **15.0–24.1%**.
- a *perfect* directory (offsets resolved with zero opens, zero re-hash) removes at
  most that share; offsets still have to be read, and it touches none of the 44.7%
  parse, the 32.0% dispatch, or the 8.4% serialize.
- implied headline shift: `1.293 × 0.106 ≈ 0.137` (pooled) / `0.129` (median doc).
- the court's **minimum detectable effect at N=100 is ≈ 0.399** (half-width
  ±0.279). A perfect selector directory is therefore **~3× below the MDE** and
  **cannot be resolved** by this court.

## 5. Paired before/after warm court vs the tuned `full` envelope

`tools/phase22-2-court.sh` (NEW; a copy of the frozen Phase-20.3 court with the
SQLite lane swapped to `tools/fixtures/phase22-competitors.py --config full`; the
answer SQL is the frozen control's `sql_for`, so only the physics change). Stores
built once per lane; the C0..C5 schedule served as one session per lane per rep;
N=100 paired, interleaved reps per (document, depth, lane); every sample retained.
Aggregation reuses the UNCHANGED Phase-20.3 / Phase-19.1 estimator
(`tools/fixtures/phase20-warm.py`; `raw/court-summary.md`).

| run | docs | pairs | median ratio (95% CI) | geo-mean (95% CI) | W/T/L |
|---|---:|---:|---|---|---|
| BEFORE — Phase 22.1 (`2026-10-08-phase22-competitors-86d9312`) | 12 | 1200 | 1.211 (1.006–1.483) | — | — |
| AFTER — this run | 12 | 1200 | **1.293 (1.002–1.561)** | **1.246 (1.048–1.488)** | 2 / 4 / 6 |

**No structural change was shipped**, so the AFTER binary is the BEFORE binary
plus the off-by-default profiler extension (no wire byte, no on-disk layout, no
answer changes). The two independent runs agree within their intervals (both
CIs include the other's median; the 0.08 difference is ≪ the ±0.279 half-width).
The warm position is a **real loss vs the tuned envelope, unchanged** — a
**null**, and the honest reading is that the selector layout is not the lever.

## 6. Integrity, semantics, provenance

- **Exact closure:** VOLE `materialize --exact --packed` **12/12** byte-exact;
  the tuned SQLite `full` retained blob **12/12** byte-exact (source length +
  SHA-256, `raw/exact.tsv`).
- **Equivalence:** 480 warm-session observation answers compared with the frozen
  Phase-18 `_equiv` logic; **0 value mismatches**.
- **Persistent bytes (ADR-0049, sum of regular-file sizes, measured immediately
  after build):** VOLE **7,778,087 B** vs SQLite `full` **10,199,040 B** →
  **0.763×** — reproducing Phase 22.1's 0.762× storage headline, and byte-for-byte
  the same VOLE store sizes as the profiling harness (`raw/store_shape.tsv`). The
  profiler extension changed no layout byte.
- **Provenance:** `environment.json`, `receipt.json` (base image digest, rustc /
  cargo / sqlite3 versions, `Cargo.lock` SHA-256, commit, dirty tree, binary
  SHA-256); `commands.txt`.

## 7. Decision and residual risks

- **Shipped: no structural layout.** The measured target is the one-time
  `Descriptor::parse` (44.7%), which lives in `src/container/` and is outside the
  physical-layout surface; Phase 20.3 already recorded that a lazy/partial parse
  cannot help this contract. The index is 10.6% and sub-MDE.
- **Recorded, not built:** the cheapest mechanism is not a persistent directory at
  all but an in-session verified-node memo (one immutable leaf re-read 24–117×); it
  would remove most of the 10.6% for zero persistent bytes. It is still ~3× below
  the MDE, so it cannot be credited by this court; it is left as a candidate for a
  future phase with a higher-resolution court.
- **Residual risk:** the stage split separates *open* from *loop* cleanly, but
  `dispatch` (32%) is a single bucket — typed-model access vs answer construction
  inside it are not separately timed, so if a future phase targets dispatch it must
  be further instrumented. The profiling subset is the 12-document warm court
  subset; the numbers are specific to it.
