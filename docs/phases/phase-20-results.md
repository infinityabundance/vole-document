# Phase 20 results

Branch: `phase20`. Measured at commits `47acde7` (20.1), `7b897ba` (20.2),
`5ed5957` (20.3), and `5ab2e76` (20.4); base `main` @ `v0.1.0-alpha.24`
(Phase 19).

Phase 20 is a **hardening-and-economics phase**. Phase 19 measured the system
honestly but left four ranked problems open: the Phase-18 **durability win was
unproven under faults**, **memory was the next binding constraint** for sources
≳1 GiB, the warm lane was a **modest real loss**, and **C4b corpus/external
lineage remained open**. Phase 20 attacks them in risk order — a correctness
court over the store the exactness path depends on runs first. Every figure
below links to a sealed receipt under
[`evidence/campaigns/`](../../evidence/campaigns/); negatives and partial
results are recorded, never buried. Exactness is the same invariant throughout:
`materialize(descriptor) == original_bytes` (length + SHA-256 + `cmp`).

## What Phase 20 established

- **The packed seed store's durability is now adversarially attacked, not merely
  argued.** A fault-injection court ran **1,300 cases** across process death,
  on-disk corruption, and deterministic in-code aborts, under **both**
  `SyncPolicy::Batch` and `SyncPolicy::Each`, with **1,300 PASS / 0 FAIL / 0
  CRITICAL**: no injection ever made the store serve bytes not matching the
  requested id (`bad_hash = 0`), recovery was always exactly a prefix
  (0 prefix violations), and every published field materialized exactly or
  failed closed with a typed error. The court's **scope is stated honestly**:
  it proves the flush-before-publish ordering, the no-partial-node property,
  prefix-exact recovery, and the corruption fail-closed path — it does **not**
  prove durability under true power loss or a torn/lost rename, because
  `SIGKILL` does not evict the OS page cache and `write_atomic` never
  dir-fsyncs (20.1). **Phase 23 closes and models this** (directory `fsync`,
  default `Safe`; a barrier-log power-loss proxy) so only a **true hardware
  power cut** remains unproven — see
  [ADR-0057](../adr/0057-directory-fsync-and-power-loss-proxy.md) and
  [phase-23-results.md](phase-23-results.md).
- **Large-source memory improved by 33% and a ~1 GiB source now ingests.** The
  direct build's peak RSS fell from **5.998×** to **3.997×** source on the
  `real100-v1` span; `nasa-pdf-0001` dropped **2342 → 1563 MiB**, and a 1 GiB
  synthetic that **OOM-killed (rc 137) at 6113 MiB** now completes **rc 0 at
  4099 MiB** — so a ~1 GiB source fits with ~2 GiB headroom. The change is
  **wire-neutral**: descriptor SHA-256 identical **15/15** (20.2).
- **The warm loss is diagnosed and recorded as durable, not tuned away.** The
  dominant cost is the one-time full `Descriptor::parse` + `Field::open` that
  decoder authority requires; a lazy/partial session open cannot help this
  contract because the frozen schedule issues non-probe-eligible observations
  on its very next request, so the total parse cost is unchanged. The paired
  before/after is a **null** (median 1.292 → 1.322, CI half-width ±0.311, MDE
  ≈0.44) — the park is below the court's resolution (20.3).
- **C4b is closed honestly by a typed external layer.** Corpus/external
  metadata enters as an explicit, removable `ExternalContext` stored *beside*
  the document-derived field, answered with an explicit `external-metadata`
  basis that is never exact and never touches decoder authority. Supplied
  equally to both lanes, both answer **12/12** and VOLE's tuple equals
  SQLite's **12/12** (20.4, [ADR-0055](../adr/0055-external-context-typed-external-metadata.md)).

## 20.1 Crash / power-cut fault-injection court

**Question.** Does the packed seed store (`fieldpack/`, ADR-0053), under
**both** `SyncPolicy::Batch` (default) and `SyncPolicy::Each`, hold its
recovery invariants across an arbitrary crash at every meaningful durability
boundary — and is the recovered set *exactly a prefix* of the appended record
sequence, with a published manifest always consistent?

**Receipt.**
[`2026-10-08-phase20-crash-47acde7`](../../evidence/campaigns/2026-10-08-phase20-crash-47acde7/);
court `tests/crash_recovery.rs` driven by `tools/phase20-crash-court.sh`. Base
image `rust:1.99.0-slim-bookworm@sha256:452176c0…`, service `dev`
(`mem_limit == memswap_limit == 8g`, `pids_limit 4096`), `rustc 1.99.0`,
`cargo 1.99.0`, `Cargo.lock` SHA-256 `3e8d445f…`. Two representative PDFs
(`nist-pdf-0017`, `nist-pdf-0002`), `reps=2`.

**Result — PASS: 1,300 cases, 1,300 PASS / 0 FAIL / 0 CRITICAL.**

Three injection families, under both sync policies:

| family | injection | cases | verdict |
|---|---|---:|---|
| A process death | `SIGKILL`/`SIGABRT` delay sweep + manifest/idx-boundary kills | 1,024 | 1,024 PASS |
| B storage corruption | truncate `.pack`/`.idx`, bit flips, zeroed tail, dropped `.idx` | 236 | 236 PASS |
| C deterministic abort | `fault-inject` in-code abort at 10 named writer points (non-default feature) | 40 | 40 PASS |

Injection × outcome:

| injection | no-manifest reopens | no-manifest fail-closed | manifest exact-ok | manifest fail-closed | CRITICAL | total |
|---|---:|---:|---:|---:|---:|---:|
| `sigkill/sigabrt-at-delay` | 67 | 1 | 860 | 0 | 0 | 928 |
| `sigkill/sigabrt-at-boundary` | 0 | 0 | 96 | 0 | 0 | 96 |
| `fault-inject-abort` | 28 | 0 | 12 | 0 | 0 | 40 |
| `truncate_pack` | 0 | 0 | 4 | 60 | 0 | 64 |
| `truncate_idx` | 0 | 0 | 0 | 44 | 0 | 44 |
| `flip_pack` | 0 | 0 | 32 | 0 | 0 | 32 |
| `flip_idx` | 0 | 0 | 10 | 34 | 0 | 44 |
| `zero_pack_tail` | 0 | 0 | 20 | 0 | 0 | 20 |
| `drop_idx` | 0 | 0 | 4 | 0 | 0 | 4 |
| `drop_idx_truncate_pack` | 0 | 0 | 4 | 24 | 0 | 28 |

- **`bad_hash` (fetched bytes not matching the requested id) = 0.** No injection
  ever served a wrong-bytes node.
- **Prefix-resolution violations = 0.** Every no-manifest reopen recovered
  exactly the independent framing scan of the open segment.
- The whole-node re-hash gate **rejected 212,812 enumerated nodes across 110
  cases** (tampered bodies outside the observation closure are caught when the
  node is enumerated). Corruption consistently **fails closed** with a typed
  error; `manifest/fail-closed` counts a published field whose seed nodes were
  tampered, which declined `materialize --exact`/observation typed rather than
  returning altered bytes.

**Batch vs Each — identical, and why that is not a durability proof.**

| policy | cases | PASS | FAIL | CRITICAL |
|---|---:|---:|---:|---:|
| `Batch` | 650 | 650 | 0 | 0 |
| `Each` | 650 | 650 | 0 | 0 |

The policies produce the same verdict distribution, **expected for this
injection model** and **not** evidence that batching is power-safe: a `SIGKILL`
does not evict the OS page cache, so no `fsync`/`fdatasync` boundary is
actually exercised.

**One robustness observation (not a correctness failure).** A `SIGKILL` inside
`PackWriter::ensure_open`, after the `.pack` was created but before its 24-byte
header completed, leaves the store **unusable but with no published manifest**;
every later open fails closed with a typed `IntegrityMismatch` ("has a
truncated header") and never returns bytes. This is the contract-permitted
"detected, fails closed" outcome, recorded rather than hidden; it is
recoverable by removing the sub-header stray segment.

**Interpretation.** The packed store's crash story holds under a hostile,
pre-registered fault matrix: flush-before-publish ordering is respected, no
partial node is ever fetchable, recovery is exactly a prefix, and corruption
fails closed. The mechanism is falsified only if a case serves a wrong-bytes
node, exposes a partial node, publishes a manifest referencing a missing node,
fails to materialize exactly, or violates the prefix property — none occurred.

**Limitation.** `SIGKILL`/`SIGABRT`/`abort()` stop the process but the OS page
cache survives, so the court does **not** measure loss of un-`fsync`ed records
under true power loss. The `write_atomic` rename is never followed by a parent
directory `fsync`, so a torn/lost rename across a real power cut is **argued,
not measured**. **Closed by Phase 23**: every atomic publish now dir-fsyncs
(default `DirSyncPolicy::Safe`) and a barrier-log model reconstructs the
post-power-loss store
([ADR-0057](../adr/0057-directory-fsync-and-power-loss-proxy.md);
[phase-23-results.md](phase-23-results.md)); only a true hardware power cut is
unproven. Family C (deterministic abort) requires the non-default
`fault-inject` feature; without it family C reports as skipped.

## 20.2 Large-source memory architecture for the direct build

**Question.** The measured `nasa-pdf-0001` ceiling was a 409 MiB source →
**2342 MiB** peak RSS (~5.7×; Phase 19.3) against the 6 GiB lane cap. Can peak
memory be **bounded** for sources ≳1 GiB while exactness and the observation
surface are unchanged?

**Receipt.**
[`2026-10-08-phase20-memory-7b897ba`](../../evidence/campaigns/2026-10-08-phase20-memory-7b897ba/);
court `tools/phase20-memory-court.sh`. Service `doc-baseline`
(`mem_limit == memswap_limit == 6g`, `pids_limit 4096`, `cpus 8`); **no cap was
raised**. `before` numbers are the identical court against `git stash`-ed
`src/`.

**Result — bisected dominance; the peak is the encode court's decode-before-commit proof.**

Per-stage RSS instrumentation (`VOLE_MEM_TRACE`) on `nasa-pdf-0001` shows the
whole peak is the encode court's decode-before-commit proof holding **six**
source-sized buffers at once (source, propose object copy, serialize `pending`
clone, serialized authority, parse object copy, `resolve_objects` clone, eval
output); it traced to exactly **6.007 S**. **Stage B never dominates:** after
Stage A the build sits at 2 S, and the scan/index/node-write structures never
define the high-water mark.

Four files changed, **no wire change**:

- `src/encode/court.rs` — `Court::offer` drops the candidate descriptor
  immediately after `serialize()` (the tie-break `work` value is captured
  first, so selection is unchanged).
- `src/materialize/mod.rs` — `materialize_in_place`/`take_objects` **move**
  inline object bytes (`std::mem::take`) instead of cloning; a regression test
  pins equality with `materialize`.
- `src/field/mod.rs` — `ingest_verified` uses `materialize_in_place`.
- `src/field/ingest_package.rs` — `ingest_package_direct` uses it for the
  DOCX/EPUB/ZIP path.

RSS-vs-source curve (peak RSS, `/usr/bin/time -v`; least-squares over sources
≥10 MiB, `peak_KiB = C + k·source_bytes`):

| source | bytes | before peak | before ×src | after peak | after ×src |
|---|---:|---:|---:|---:|---:|
| `nasa-pdf-0003` | 207.6 MiB | 1248.5 MiB | 6.02 | 833.4 MiB | 4.02 |
| `nasa-pdf-0002` | 294.5 MiB | 1769.9 MiB | 6.01 | 1181.1 MiB | 4.01 |
| `nasa-pdf-0001` | 389.9 MiB | **2342.2 MiB** | **6.007** | **1562.6 MiB** | **4.008** |
| `synth-opaque-1024m` | 1.0 GiB | **rc 137 (OOM), 6113 MiB** | 5.97 | **rc 0, 4099 MiB** | **4.003** |
| | | `k` = **5.998×** | `C` = 3.4 MiB | `k` = **3.997×** | `C` = 5.3 MiB |

- **RSS/source 5.998× → 3.997× — a 33% cut** on large sources. Wall is
  unchanged within noise (`nasa-pdf-0001` 4.7 s before and after).
- **A ~1 GiB source now fits the 6 GiB cap** with ~2 GiB headroom; the pre-change
  code OOM-killed at 6113 MiB. The cap boundary moves ~1.0 GiB → **~1.5 GiB**.
- **Exactness 16/16** after (15/16 before — the 1 GiB synthetic OOM-killed).
- **`.voldoc` bytes unchanged:** descriptor SHA-256 identical **15/15** across
  the 15 real `real100-v1` sources (PDF + DOCX + EPUB).
- Package ingest keeps a higher constant (**5.30×** at 10 MiB after vs 6.32×
  before; the large-source slope is dominated by the PDF/opaque path).

**Interpretation.** The memory ceiling was not "the build"; it was a specific,
redundant source-sized copy in the enforce-the-exactness check. Removing the
copy is output-preserving and caps the slope at ~4× source. This is a real
bound, not a raised limit.

**Limitation.** The floor with the current architecture is **4 source-sized
copies** {source, serialized authority, parsed inline object, reconstructed
output}; reaching 3 needs a `Descriptor` whose object payloads borrow from the
authority bytes (a **lifetime-parameterized descriptor** — a wire-type change
this phase does not make), so a ~2 GiB source still does not fit. The curve is a
single run per source on a bind-mounted host; the ratio is stable but not
interval-bounded. `materialize_in_place` consumes the parsed object bytes and is
a footgun for future callers (both current call sites use a fresh parse).

## 20.3 Warm heterogeneous query — profiling and a recorded durable loss

**Question.** Is the Phase-19.2 warm loss (~1.29×, ~0.7 ms per session) a
closeable constant, or a durable property of the resident session?

**Receipt.**
[`2026-10-08-phase20-warm-5ed5957`](../../evidence/campaigns/2026-10-08-phase20-warm-5ed5957/);
courts `tools/phase20-warm-court.sh` and the read-only
`tools/phase20-warm-profile.sh`. Service `doc-baseline`; same frozen
12-document C0–C5 contract court and the Phase-19.1 estimator as 19.2 (N=100,
bootstrap 20,000, seed 200103, cluster-resampled by document).

**Result — the loss is the descriptor parse; no safe lever; the change is a null.**

Breakdown of one warm VOLE session (`observe-batch`, packed store, warm cache;
median of 5, µs). The dominant term is **`Descriptor::parse`** — a linear
re-framing + full cross-validation of the `.voldoc` blob (~**1.7 ns/byte**,
≈**590 MB/s**) and the **entire `Field::open`**:

| document | descriptor B | descriptor read+hash | **descriptor parse** | session open |
|---|---:|---:|---:|---:|
| `nist-docx-0008` | 29,264 | 13 | **58** | 121 |
| `nist-docx-0014` | 948,903 | 254 | **1621** | 1915 |
| `nist-pdf-0004` | 1,105,542 | 318 | **1861** | 2248 |
| `nist-pdf-0017` | 1,466,727 | 517 | **2589** | 3264 |
| `nist-epub-0009` | 1,237,736 | 438 | **2201** | 2720 |

The parse is 1.6–2.6 ms (0.9–1.5 MB descriptors) and ~45–50% of a *losing*
session; for the winning 29 KB document it is 58 µs. Syscall time is a small
fraction (`strace -c` ~0.1–1.8 ms/session) — the cost is CPU, not I/O. This is
the same one-time full-parse mechanism Phase 16.4 found in the fs-direct world.

**No change shipped, with proof.** Each candidate lever was checked, not
assumed:

- **Lazy/partial session open cannot help *this* contract.** `probe_eligible`
  covers only `(Page, Text|Preview|Structure)` and `(Stream, Decoded|Operators)`,
  while the frozen schedule also issues `metadata`, `revision(s)`, and
  (docx/epub) `block`/`doc-text`/`heading`/`table`/`resource` — none
  probe-eligible. The first such request forces the full parse, so the **total
  parse cost is unchanged** (docx/epub force it on request #2, pdf on #3).
- **No redundant manifest/descriptor re-read or re-hash:** the session opens the
  field once and reuses the already-open `Field`, its parsed manifest, and one
  `FsIndexStore` across every request.
- **Per-request setup is already hoisted;** per-request dispatch is real but
  bounded (63 µs pdf → 2.7 ms epub) and not the gap driver.
- **Changing the parse itself** (lazy advisory records / relaxing a validation)
  lives in `src/container/` and would weaken decoder authority or change the
  observation contract — **rejected, not attempted**.

Paired interleaved N=100 before/after (the SAME frozen court):

| run | docs | pairs | median ratio (95% CI) | geometric mean (95% CI) | W/T/L |
|---|---:|---:|---|---|---|
| BEFORE (19.2) | 12 | 1200 | 1.292 (0.989..1.657) | 1.283 (1.068..1.538) | 2 / 3 / 7 |
| AFTER | 12 | 1200 | 1.322 (1.008..1.630) | 1.282 (1.072..1.531) | 2 / 3 / 7 |

**NULL.** The court's median-ratio 95% CI half-width is **±0.311** and the MDE at
N=100 is **≈0.44**; even fully realising the parse removal moves the median by
≈0.3 (≈0.7 ms on a ≈2.3 ms session) — **below the MDE**. Only the env-gated
profiler `VOLE_PROFILE_OPEN` (off by default; a single `getenv` on the default
path) was kept.

**Exactness / equivalence.** VOLE `materialize --exact --packed` **12/12**;
SQLite retained blob **12/12**; warm-session answers compared **480**, **0 value
mismatches**.

**Interpretation.** The warm deficit is a genuine property of the resident
session under this contract — the descriptor parse that decoder authority
requires — and it is **recorded as durable**, not tuned away. The honest
statement is the Phase-19.2 one: ~1.29×, marginal under the median and resolved
under the geometric mean.

**Limitation.** Warm-only: the one-time builds are paid once and excluded. The
bind-mounted host store is noisy; if 1.0 stays inside the interval that is
reported, never massaged. The parse cost is what the court can see on this
subset; no population claim.

## 20.4 C4b — `ExternalContext` as a typed external layer

**Question.** Can corpus/external lineage be admitted as an **explicit, typed
external layer** (`basis: ExternalMetadata`) *beside* the document-derived
field — never contaminating the core field to satisfy a benchmark?

**Receipt.**
[`2026-10-08-phase20-c4b-5ab2e76`](../../evidence/campaigns/2026-10-08-phase20-c4b-5ab2e76/);
court `tools/phase20-contract-c4b-court.sh`. Service `doc-baseline`; SQLite
baseline byte-identical to Phase 16.5/18.1. Decision:
[ADR-0055](../adr/0055-external-context-typed-external-metadata.md).

**Result — C4b closes under an equal external input, from a separate typed layer.**

`ExternalContext { dataset_id, lineage { family, member, head, revision_family },
origin (closed enum harness|operator|catalog), source }` is canonically encoded
(`VOLECTX1`) at `<store>/external/<FieldId>`. It is **disjoint** from
`descriptor/field/index/cache/seed|fieldpack/promoted` and is **never** in the
seed DAG, index, manifest, or exactness authority; removal is one `unlink`.
A new `Selector::ExternalLineage` (`external-lineage`) is answered for
`--kind lineage`; CLI `observe --external-lineage --kind lineage` and
`field-external --store --field (--lineage FAMILY:MEMBER:HEAD | --clear)`.

The answer carries a typed basis `Basis::ExternalMetadata` with
`is_exact() == false`; the answer record reports `basis=external-metadata`,
`exact=false`, empty `dependency_ids`, `integrity_scope=none`, `bytes_read=0`,
`provenance=external-context;origin=…;source=…`. **No context is a typed
decline** (`UnsupportedFeature`, rc 6) — never a guess.

Separation proven (Phase 20.4 re-run with the external input supplied to
**both** lanes):

| assertion | result |
|---|---|
| plain `--metadata` answer byte-identical before-attach / after-attach / after-clear | **12/12** |
| `materialize --exact` matches attached **and** after removal | **12/12** |
| external query declines typed (rc 6) after clear | **12/12** |
| C4b answered by both lanes under an equal external input | **12/12** both |
| VOLE's C4b tuple equals SQLite's | **12/12** |

Cost over the subset: VOLE `field-external` attach **26 ms**, sidecar **1001 B**;
external query **8 ms** (reads only the sidecar) vs SQLite's **19 ms** C4 query
(folded into its C4 build). Frontier otherwise unchanged from 18.1/19: storage
**0.49×**, build **7.34×**, warm **1.38×** SQLite.

**Interpretation.** C4b is no longer a VOLE capability gap. It closes **because
the same external fact is supplied to both lanes** — SQLite stores it inside its
C4 build, VOLE stores it as an explicit typed sidecar beside the field — so what
remains is a *cost* comparison, not a coverage claim. The core field's
observations and exactness are **byte-identical** whether or not the layer is
attached, which is exactly the property a benchmark-driven contamination would
have broken.

**Limitation.** C5b (folding `--external-lineage` into the heterogeneous batch)
is **not measured** — the court measures the C4b query as a separate step beside
the schedule. The layer is caller-supplied: it answers a fact a single document
cannot derive, and the provenance says so (`basis=external-metadata`,
`exact=false`), so it must never be cited as a document-derived answer.

## Cross-cutting notes

- **No wire byte, decode path, or `encode` output changed in Phase 20.** 20.1 is
  a court plus a non-default `fault-inject` feature; 20.2 removes redundant
  in-memory copies (descriptor SHA-256 identical 15/15); 20.3 ships only an
  env-gated, default-off profiler; 20.4 adds a store-adjacent sidecar that is
  never on the decode path.
- **Exactness is untouched throughout:** `materialize --exact` 16/16 (20.2),
  12/12 + 480 envelopes / 0 mismatches (20.3), 12/12 both lanes (20.4).
- **Claim discipline.** The durability court's scope is stated with it
  (ordering/prefix/fail-closed **proven**; the directory-`fsync`/torn-rename gap
  is **closed and modelled in Phase 23**, [ADR-0057](../adr/0057-directory-fsync-and-power-loss-proxy.md),
  leaving only a true hardware power cut unproven); the memory bound is a measured `C + k·source` with a stated
  floor and no raised cap; the warm loss is recorded durable rather than tuned
  away; and C4b is closed by a typed external layer whose basis is never exact.
