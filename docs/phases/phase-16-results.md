# Phase 16 results

Branch: `phase16`. Base: `main` @ `ebb6636` (`v0.1.0-alpha.20`, Phase 15).

Phase 16 follows Phase 15: it **adopts the backend Phase 15 only recommended**,
finishes the packed-storage court on the **full** `real100-v1` population, fixes
the **large-PDF encode pathology** that court surfaced, continues the residency
line, and — decisively — tests whether **SQLite loses under an equal capability
contract**. It closes with a **measurement correction** that removed a false VOLE
storage advantage. Every number links to a sealed receipt under
[`evidence/campaigns/`](../../evidence/campaigns/); negatives are recorded, not
buried. No claim here is a population claim: every corpus is frozen.

## What Phase 16 established

- **`zlib-rs` is adopted** as the shipped inflate backend (16.1): one helper owns
  every inflate, RFC 1950/1951 selected by a wrapper, **byte-identical** to the
  `miniz_oxide` reference — and a real end-to-end **~8 % `field-ingest` win**
  (`0.917×`), not the 1.58× microbench rate (inflate is only a fraction of
  ingest).
- **The >100 MiB PDF encode pathology is fixed** (16.3): a single encoder-side
  allocation in `propose_rle` was the cause; `nasa-pdf-0001` (409 MB) now
  completes **byte-exactly** (~40 s, rc 0) and peak/input fell **17.7× → 8.9×**.
- **Residency with the probe short-circuit is still a negative** (16.4): giving
  the resident session the cold path's `narrow_probe` works as intended but does
  **not** flip any size class; the isolated lever is a **lazy session open**.
- **SQLite does not lose under an equal capability contract** (16.5): forced to
  satisfy the same escalating contract, the baseline builds **~10×** faster, [SUPERSEDED for build cost: Phase 18.5 re-ran this court with the direct packed build and batched durability and measured VOLE **0.82×** SQLite (i.e. faster); the ~10× figure is retained as the historical two-step-path result. See `docs/phases/phase-18-results.md` and ADR-0053.]
  serves the warm session **~1.5×** faster, and is the **only** lane that answers
  revision lineage — VOLE declines C4/C5 because its CLI has no revision surface.
- **A false VOLE storage advantage was removed** (16.6): `du -sb` counted 4096 B
  per directory inode; under file-bytes-only accounting VOLE `fs`/SQLite goes
  **1.377× → 0.906×** and packed/`fs` **0.719× → 1.007×** — both VOLE backends
  are **at or below** SQLite on file bytes, and the packed win is **file/directory
  count**, not bytes ([ADR-0049](../adr/0049-storage-accounting-correction.md)).

## Adopt `zlib-rs` (16.1)

**Question.** Phase 15.5 recommended `zlib-rs` (1.58× GB/s, byte-identical,
RSS-neutral) but did not adopt it. Adopt it as the shipped inflate backend and
measure the **end-to-end** effect, not the microbench rate.

**Receipt.** [`2026-10-08-phase16-zlib-ebb6636`](../../evidence/campaigns/2026-10-08-phase16-zlib-ebb6636/).

**Result — ADOPTED, byte-identical, ~8 % end-to-end `field-ingest` win.**

One helper, `src/field/inflate.rs`, now owns every inflate: RFC 1950 (zlib) vs
RFC 1951 (raw DEFLATE) is selected by a **`Wrapper`** rather than by call site.

| quantity (median) | old (`miniz_oxide`) | new (`zlib-rs`) | new/old |
|---|---:|---:|---:|
| `encode` total ms (control) | 2,510 | 2,500 | **0.996×** |
| `field-ingest` total ms | 1,200 | 1,100 | **0.917×** |
| `encode`+ingest total ms | 3,710 | 3,600 | **0.970×** |
| peak ingest RSS MiB | 390.7 | 390.6 | **1.000×** |

- **Byte-identity witness.** `tests/deflate_backend_equivalence.rs` compares
  **2,075 real members across 17 documents** against the `miniz_oxide` reference:
  **byte-identical**. Field-id agreement **yes**; `materialize --exact` **PASS**
  on every row.
- **Court.** `tools/phase16-zlib-endtoend-court.sh`, `doc-baseline` lane
  (6 GiB, cpus 8), release, `--features docx,epub`, one process per operation,
  3 reps, **3 documents** (one per size class: `nasa-epub-0006`,
  `nasa-pdf-0029`, `nist-docx-0013`).

**Why ~8 % and not 1.58×.** `encode` does not inflate and is the **control**; its
`0.996×` is a measured **noise floor**, so the ingest delta is real but small
because inflate is only a fraction of `field-ingest` work. The 15.5 1.58×
microbench rate and this 0.917× end-to-end ratio are both reported; the smaller
number is the honest one.

**Regression found and fixed (during 16.1).** The first adoption cut
`field-ingest` by roughly half: the length-learning pass preallocated its **32 MiB
ceiling** unconditionally. It now grows from **2× input**, restoring the win. The
regression is recorded because it is exactly the kind a faster backend can hide.

**Interpretation.** A backend swap guarded by a byte-identity witness is a safe,
self-contained way to bank a real (if modest) ingest improvement without touching
any wire byte, descriptor, or decoder behavior.

**Limitation.** A 3-document end-to-end court; the `encode` control is a noise
floor, so deltas smaller than it are unresolved. No population claim.

## Full `real100-v1` packed court (16.2)

**Question.** On the **common-success** population of the full `real100-v1`
corpus, does the optional `--packed` seed backend close the persistent-storage
gap against the A1 SQLite db?

**Receipt.** [`2026-10-08-phase16-packed-full-0b21928`](../../evidence/campaigns/2026-10-08-phase16-packed-full-0b21928/).

> **Byte numbers superseded.** Every ratio in this receipt was taken with
> `du -sb`; the storage figures are corrected by **16.6** (ADR-0049). The court
> itself — population, success, exit codes — stands.

**Result — court complete; head-to-head on 95 documents; byte figures corrected
by 16.6.**

| stage | succeeded |
|---|---:|
| `encode` | 97 / 100 |
| VOLE `fs` `field-ingest` | 97 / 100 |
| VOLE packed `field-ingest --packed` | 97 / 100 |
| A1 SQLite build | 98 / 100 |
| field id identical (`fs` == packed) | 97 / 97 |

Common success (`fs` + packed + A1, the head-to-head): **95** documents.

**Failures (never hidden).** `nasa-pdf-0001` encode rc 137 (OOM — since fixed in
16.3); `nasa-pdf-0002`/`0003` rc 124 (timeout); two A1 builds rc 1
(`nasa-epub-0010`, `nist-epub-0008`) — pre-existing **baseline XML parse
errors**, not VOLE failures.

**Corrected head-to-head (file-bytes-only, 95 docs; see 16.6).**

| comparison | sum ratio | median per-doc |
|---|---:|---:|
| VOLE `fs` / SQLite | **0.906×** | 0.815× |
| VOLE packed / SQLite | **0.914×** | 0.824× |
| VOLE packed / VOLE `fs` | **1.009×** | 1.015× |

**Interpretation.** The packed backend's court completes on the real population
with byte-level parity to `fs` and both backends at or below SQLite on file
bytes. The *original* 16.2 framing ("packed closes the gap", `0.921×`) was a
`du -sb` artifact; there was no byte gap. The real remaining packed win is
file/directory **count** (3,511 vs 218,853 directories — 16.6).

**Limitation.** Whole-population totals in the receipt are over different
document sets per substrate and must never be read as head-to-head; only the
95-document common-success set is comparable. Its byte figures are superseded.

## Large-PDF encode pathology fixed (16.3)

**Question.** Phase 15 left the `>100 MiB` PDF encode failures (rc 137 OOM, rc 124
timeout) as "the known Phase-14 bound". 16.2 reproduced them. Attribute the
memory and fix it if it is a bug.

**Receipt.** [`2026-10-08-phase16-largepdf-ce8af8f`](../../evidence/campaigns/2026-10-08-phase16-largepdf-ce8af8f/).

**Result — FIXED; `nasa-pdf-0001` (409 MB) now completes byte-exactly.**

**Root cause.** `propose_rle` built `runs: Vec<(u8, u64)>` (16 B per maximal run,
≈16× input for near-incompressible data) **before** its `runs.len()*2 >
max_graph_ops` decline check. A 409 MB file needs ~16.4 GB for that `Vec` and is
SIGKILLed at the 6 GiB cap before it can decline. Direct proof: `0001 --force raw`
completes (rc 0, exact) while `0001 --force rle` OOMs.

**Fix.** Two streaming passes: pass 1 counts maximal runs and the longest run in
**O(1) memory** and performs both decline checks there; pass 2 materializes `ops`
only once admitted (bounded by `max_graph_ops/2`). Same ops, same descriptor
bytes, same verdict for every input.

| id | bytes | peak before | peak after | wall before → after | rc before → after |
|---|---:|---:|---:|---|:--:|
| nasa-pdf-0001 | 408,854,600 | 6,281,620 KB (OOM) | **3,558,292 KB** | 2.4 s → 40.0 s | 137 → **0** |
| nasa-pdf-0002 | 308,803,168 | 5,356,860 KB | **2,696,872 KB** | 260 s → 228 s | 0 → 0 |
| nasa-pdf-0003 | 217,666,672 | 3,758,292 KB | **1,906,148 KB** | 243 s → 233 s | 0 → 0 |

Peak/input ratio: **17.7× → 8.9×**. Forced `rle` on `0001`: 6.28 GiB OOM →
**401,524 KB** (1.0×, declines cleanly).

**Exactness preserved.** Before/after binary differential, byte-identical
`.voldoc` SHA-256: **73/73** corpus documents <20 MB (all formats) and **4/4**
large PDFs — **77/77 identical**. Gates all pass (`fmt`, `clippy -D warnings`,
`test --all-features`, `test --no-default-features`, `tools/phase1-court.sh`).

**Interpretation / limitation.** This is a genuine encoder bug fix, not a
cap raise: no cap was touched and no validate-or-decline rule was weakened.
`0002`/`0003` still exceed the **180 s op budget** (rc 124) — now a **wall** limit
on the slow `BYTE_RANS` lane, **not** memory (2.57 GiB / 1.82 GiB, comfortably
under the cap); the budget was **not** raised. The remaining ~8.9× peak is the
court's whole-file decode-before-commit round-trip plus the input buffer; no claim
that the residual is irreducible.

## Resident session + `narrow_probe` (16.4)

**Question.** Phase 15.2's resident session lost because the cold `observe` path
has a `narrow_probe` short-circuit the session lacked. Give the session that
short-circuit and re-measure.

**Receipt.** [`2026-10-08-phase16-resident-probe-5d331f2`](../../evidence/campaigns/2026-10-08-phase16-resident-probe-5d331f2/).

**Result — NEGATIVE; the probe works, residency still does not beat cold.**

`narrow_probe` was refactored into a shared core; a new `observe_session` keeps
the index store open and probes with the **already-open** `Field` manifest. The
probe works **exactly as intended**: observations 2..N of each batch report
`descriptor_bytes_read = 0`, `descriptor_read_mode = "partial"`, **~25 µs** each
(208/360 such hits), with **90 equal / 0 mismatch** answer equality against the
cold lane on the real corpus.

| size class (`text_repeat`) | n | cold `v` | resident `v_r` | winner |
|---|---:|---:|---:|---|
| <100KiB | 8 | 6.5 | 1.0 | resident |
| 100KiB-1MiB | 15 | 7.0 | 2.0 | resident |
| 1-10MiB | 38 | 6.0 | 8.0 | cold |
| 10-50MiB | 16 | 6.0 | 41.0 | mixed (epub resident; pdf cold) |
| 50-100MiB | 10 | 6.0 | 128.5 | cold 10/10 |
| >100MiB | 3 | 5.0 | 370.0 | cold 3/3 |
| **all** | **90** | **6.0** | **9.0** | **cold** |

Sums: cold **3,092 ms** vs resident **3,894 ms**. **No size-class verdict
flips** versus the 15.2 receipt.

**Mechanism.** The residual cost is the **one-time full `Field::open` descriptor
parse**: on `nasa-pdf-0004` a 1-observation batch costs the same as a
5-observation batch (`0.14 s == 0.14 s`), and the cold `narrow_probe` never opens
the descriptor at all. One full parse of a tens-of-MB descriptor exceeds five
probe-only process spawns.

**Interpretation / limitation.** This is the **second** independent residency
negative (15.2, 16.4); the mechanism is now isolated. The lever that could make
residency pay for large descriptors is a **lazy session open** (open only the
manifest at session start; load the descriptor only when a probe misses) —
**recorded, out of scope**, not attempted. One frozen corpus; no population claim.

## Contract-equivalent court (16.5)

**Question.** Phase 12–15 compared VOLE to a SQLite/FTS baseline on the axes VOLE
chose. Force a source-retaining SQLite baseline to satisfy the **same escalating
contract** (C0 value → C1 native coordinate → C2 provenance/span → C3 exact
closure → C4 revision lineage → C5 mixed batch) and ask whether SQLite still
loses.

**Receipt.** [`2026-10-08-phase16-contract-45d2c0e`](../../evidence/campaigns/2026-10-08-phase16-contract-45d2c0e/).

**Result — NEGATIVE for the VOLE-preferable framing: SQLite does NOT lose under
the equal contract.**

Subset: **12 documents** (docx, epub, pdf). Equality holds for both lanes at
**C0–C3** (docx/epub text byte-identical; PDF page text is a *heuristic* layout
projection — recorded as `divergent`, never equality). Both lanes reproduce the
source exactly: VOLE `materialize --exact` **12/12**, SQLite retained blob
**12/12**.

| depth | VOLE cold ms | SQLite cold ms | VOLE warm ms | SQLite warm ms | VOLE B | SQLite B | VOLE satisfies? |
|---|---:|---:|---:|---:|---:|---:|---|
| C0 | 127 | 116 | 32 | 24 | 6,913,777 | 14,270,464 | yes |
| C1 | 123 | 120 | 33 | 19 | 6,913,777 | 14,376,960 | yes |
| C2 | 126 | 117 | 35 | 20 | 6,913,777 | 14,622,720 | yes |
| C3 | 126 | 125 | 35 | 23 | 6,913,777 | 14,622,720 | yes |
| C4 | 123 | 118 | 33 | 25 | 6,913,777 | 14,721,024 | **no (declines)** |
| C5 | 121 | 120 | 34 | 26 | 6,913,777 | 14,721,024 | **no (declines)** |

**Readings.**

- **SQLite is the build, warm-latency and full-contract winner at every depth.**
  It builds **~10×** faster (VOLE encode+ingest 20,847 ms vs SQLite C0 1,956 ms)
  and serves the warm session **~1.47×** faster.
- **Escalating the contract costs SQLite only ~+3 % persistent bytes** (C0 →
  C4), because the retained source blob it already carries dominates the store;
  its query cost is **flat** across depths. VOLE's query cost is depth-independent
  too (its store already carries coord/provenance/exact).
- **The decisive depth is C4.** Once the contract demands revision lineage, VOLE
  **cannot answer at any cost** — its CLI exposes no revision query surface
  (`--revision N` returns `unsupported observation`) — while the baseline answers
  from a `revisions` table costing +1.1 % bytes over C2. That is a real product
  decision, not an artifact.
- **VOLE's lone edge is storage** — 6.9 MB vs 14.3–14.7 MB (~0.47×) on this
  12-document subset — and **16.6 shrinks even that** to ~0.9× on the full
  population under corrected accounting.

**Interpretation.** This is the strongest negative in Phase 16 and it is
architectural, not incidental: under an **equal capability contract** SQLite
wins on build, latency and coverage, and loses only on storage. It directly
motivates the open question recorded in
[ADR-0050](../adr/0050-sqlite-as-substrate-question.md) — whether the field's
conceptual invention should keep competing with an embedded DB or **use one as
part of its physical substrate**.

**Limitation.** A 12-document subset; no population claim. The baseline carries
FTS surfaces the contract does not require (a faithful upper bound on its cost).
PDF page text is a heuristic projection, so equality there is shape-only.

## Storage-accounting correction (16.6)

**Question.** The 15.1/15.3/16.2 storage headlines were taken with `du -sb`. Is
that the right accounting unit for a one-file-per-node store versus a single
`.db`?

**Receipt.** [`2026-10-08-phase16-storage-correction-2978e1d`](../../evidence/campaigns/2026-10-08-phase16-storage-correction-2978e1d/);
[ADR-0049](../adr/0049-storage-accounting-correction.md).

**Result — CORRECTION: the "VOLE is 1.377× SQLite" and "packed closes the gap"
headlines were accounting artifacts.**

On this ext4 bind mount `du -sb` is `--apparent-size`: it sums each regular
file's `st_size` **and each directory inode's own `st_size` (4096 B)**. The
one-file-per-node `fs` store carries tens of thousands of directories; packed a
handful; SQLite none. The comparison was asymmetric *and* inflation-dominated by
directory count.

| substrate | file bytes | `du -sb` bytes | directory overhead | overhead / file bytes | dirs |
|---|---:|---:|---:|---:|---:|
| VOLE `fs` store | 1,723,650,951 | 2,620,072,839 | **896,421,888 B** | **52.0 %** | 218,853 |
| VOLE packed store | 1,738,386,483 | 1,752,767,539 | 14,381,056 B | 0.8 % | 3,511 |
| A1 SQLite `.db` | 1,902,919,680 | 1,902,919,680 | 0 B | 0.0 % | 0 |

**Corrected headlines (file-bytes-only):**

| comparison | old `du -sb` | corrected |
|---|---:|---:|
| 15.3 subset (12 docs), packed / `fs` | **0.719×** | **1.007×** (parity; packed marginally larger) |
| 16.2 common success (95 docs), VOLE `fs` / SQLite | **1.377×** | **0.906×** |
| 16.2 common success (95 docs), VOLE packed / SQLite | 0.921× | **0.914×** |
| 16.2 common success (95 docs), VOLE packed / `fs` | 0.669× | **1.009×** |

By format (16.2, file-bytes-only): `fs`/SQLite pdf `0.893×`, docx `0.845×`, epub
`0.992×`; packed/SQLite pdf `0.902×`, docx `0.850×`, epub `0.993×`.

- **Refuted:** "VOLE persistent footprint is 1.377× SQLite's" (it is **0.906×**,
  ~9 % *below*) and "packed closes the gap" (there was no byte gap to close).
- **The surviving packed claim** is file/directory **count** (**3,511 vs
  218,853** directories over the 96-tree re-measurement) — open and syscall
  economics, **not** document bytes, where packed and `fs` are at parity
  (1.001–1.010× by format).
- **Exactness is untouched:** no descriptor, manifest, index or wire byte
  changed; the correction is measurement-only, and the re-run reproduces the
  original `du -sb` sums byte-for-byte, so the only variable is the accounting
  unit.

**Limitation.** The distortion is not `du`-specific: any apparent-size accounting
on a filesystem that charges directory inodes a non-zero `st_size` carries it.
The correction is a union of the 15.3 subset and the 16.2 common-success set
(**96** documents).

## Cross-cutting notes

- Every exactness statement in Phase 16 is the same invariant:
  `materialize(descriptor) == original_bytes` (length + SHA-256 + `cmp`). The
  backend swap (16.1) and the encoder fix (16.3) both carry a byte-identity
  differential; neither changes a `.voldoc` byte.
- **No cap was raised** and no validate-or-decline rule was weakened to make a
  run pass (16.3 fixed a bug, not a bound).
- No corpus or schedule was tuned against any measurement; the `real100-v1`
  manifest is frozen by SHA-256.
- The two storage courts keep their original `du -sb` numbers and gain a
  correction pointer (amendment, not rewrite — ADR-0003 discipline, ADR-0049).
