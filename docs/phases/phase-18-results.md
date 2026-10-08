# Phase 18 results

Branch: `phase18`. Base: `main` @ `f2a34a5` (`v0.1.0-alpha.22`, Phase 17).

Phase 18 is a **build-cost programme**. Phase 17 removed the compression candidate
search from the runtime build path (17.1) and added a PDF revision-lineage surface
(17.2); it also showed that the two-step `encode` + `field-ingest` build was
**10.74×** SQLite under the equal contract. Phase 18 removes the remaining
*unnecessary* work from the direct build, one mechanism at a time — a redundant
source materialization (18.2), a redundant re-serialize of the authority (18.3),
and finally the **per-node durability sync** (18.4 falsifies the file-count
hypothesis; 18.5 adopts batched syncs). It closes with the equal-contract build
position at **0.82× SQLite** — VOLE now builds the subset *faster* than SQLite —
while VOLE stays at ~0.5× SQLite's storage and the cold queries remain a tie.
[SUPERSEDED as competitor statements by [Phase 22.1](phase-22-results.md):
against the *tuned* equal-contract envelope the storage advantage is
**~0.76–0.80×** (not ~0.5×, which was vs the historical control's contract-dead
trigram index), build **0.219× `full`**, cold **0.812× `full`**, and warm a
**loss** (**1.211× `full`**).]
Every number links to a sealed receipt under
[`evidence/campaigns/`](../../evidence/campaigns/); negatives are recorded, not
buried. No claim here is a population claim: every corpus is frozen.

> **Competitor note (Phase 22.1).** Every "SQLite" figure in Phase 18 (and 19) is
> measured against the Phase-18 **historical-control** configuration, which
> carried the baseline's **contract-dead** FTS/trigram indexes (the contract has
> no search observation). Phase 22.1 strengthened the competitor **first** (a
> six-configuration SQLite envelope) and re-measured: the storage advantage is
> **0.762× `full` / 0.805× `adaptive`** (not 0.53×), build **0.219× `full`**,
> cold **0.812× `full`**, and warm is a **loss** (**1.211× `full`**). See
> [phase-22-results.md](phase-22-results.md).

## What Phase 18 established

- **The equal-contract build position inverted** (18.1 → 18.5). With the direct
  `field-build` the two-step **10.74×** gap first falls to **7.24×** (18.1), and
  after batched packed-store durability it falls to **0.82×** — VOLE builds the
  12-document equal-contract subset **1552 ms vs SQLite's 1893 ms (~1.22×
  faster)**. **The win is the deletion of unnecessary durability syncs, not
  parallelism and not a codec.**
- **The reductions are stepwise and each is attributed** (18.2–18.5). Removing
  two of the three source materializations cut wall **~6.1%** and peak RSS
  **~28.8%** (18.2); generating the observation index before the single serialize
  was **wall-neutral** but cut large-document RSS **14–16%** (18.3); the packed
  *file-count* collapse did **not** move the build term (18.4, **falsified**),
  because the term was a **per-node sync**; batching that sync is what collapsed
  it (18.5).
- **The C4 decline is now two observables, reported separately** (18.1). **C4a
  document-native lineage** (the PDF internal incremental chain) is a genuine VOLE
  answer the measured baseline lane does not give; it is byte-derivable from the
  blob the baseline retains, so it is a **where-the-work-happens** difference, not
  hidden information. **C4b corpus/external lineage** (family/member/head) is
  dataset metadata the harness supplies to the baseline; VOLE is not given it and
  a single-document field cannot derive it.
- **Storage ~0.5×, cold a tie, warm ~1.1×** (18.1, 18.5). VOLE's regular-file
  storage is **0.49–0.53×** SQLite's; its cold observation sums tie the baseline
  (779 vs 772 ms in 18.1); its warm one-session lane is **1.09×** (packed, 18.5)
  to **1.33×** (fs, 18.1) SQLite. **[SUPERSEDED as competitor statements by
  [Phase 22.1](phase-22-results.md): against the tuned equal-contract envelope
  storage is **~0.76–0.80×** (not ~0.5×), cold **0.812× `full`**, and warm a
  **loss** (**1.211× `full`**); the ~0.5× was inflated by the historical
  control's contract-dead trigram index.]**
- **Exactness is untouched throughout:** every build still leaves
  `materialize(descriptor) == original_bytes` (length + SHA-256 + `cmp`) — 12/12 on
  the contract subset for the packed store (18.5), and 9/9 for each of three
  closures on the direct court, plus 12/12 on the contract court (18.2, 18.3).

## 18.1 The contract court with the direct build (C4a/C4b split)

**Question.** Phase 17 made the runtime build one-pass (`field-build --profile
runtime`) but did not re-run the equal-contract court. Re-run the SAME
12-document C0–C5 contract court changing only VOLE's build step to the direct
build: what is the *composable* build gap, and what does the C4 decline actually
consist of?

**Receipt.**
[`2026-10-08-phase18-contract-direct-f2a34a5`](../../evidence/campaigns/2026-10-08-phase18-contract-direct-f2a34a5/).

**Result — RECORDED; the build gap is 7.24×; C4 splits into C4a and C4b.**
Changing only the build step, the two-step **10.74×** gap (16.5/17.2) becomes
**7.24×** (VOLE 11,905 ms vs SQLite 1,644 ms over 12 documents). The 17.1
`field-build` **2.04×** self-speedup and the 10.74× two-step gap are **not
composable**; this court's number is the composable one. Cost vs SQLite:

| quantity | VOLE (direct) | SQLite (C5) | ratio |
|---|---:|---:|---:|
| build ms (sum) | 11,905 | 1,644 | **7.24×** |
| persistent B (regular files) | 7,168,131 | 14,721,024 | **0.49×** |
| cold ms (sum over C0–C5) | 779 | 772 | **tie** |
| warm ms (sum over C0–C5) | 220 | 165 | **1.33×** |

Persistent bytes are the sum of **regular-file** sizes for both lanes (`du -sb`
is deliberately not used — it reports phantom bind-mount directory sizes).
VOLE's query cost is depth-independent (its store already carries
coord/provenance/exact); SQLite pays new materialization at each depth.

**C4 splits into two observables, never forced equal:**

| half | observable | VOLE | measured SQLite lane |
|---|---|---|---|
| **C4a** | document-native lineage (PDF internal incremental chain) | answered **4/4** PDFs; typed unsupported (`rc 6`) **8/8** docx/epub | native chain **0/4** — answers the corpus tuple instead |
| **C4b** | corpus/external lineage (family/member/head) | **0/12** (not given the external input) | **12/12** (harness-supplied dataset metadata) |

C4a is byte-derivable: an unmeasured marker probe over the exact source the
baseline retains at C3 derives a header+chain for **4/4** PDFs (count matches
VOLE for 4). It is therefore a **where-the-work-happens** difference (indexed at
ingest vs re-parsed at query), **not** a hidden-information advantage. C4b is
**dataset metadata external to the bytes**, so VOLE's 0/12 is not a decline of a
document-derivable fact and cannot be closed by a single-document field.

**Interpretation.** The direct build removes most, not all, of the build penalty.
Storage, cold-tie, and SQLite's warm-latency edge are unchanged in direction. The
C4 frontier is now stated in two parts so that the two different claims cannot be
conflated: C4a is VOLE's genuine native-lineage observable, C4b is external
metadata.

**Limitation.** A 12-document subset; wall times single-run; the SQLite lane is a
faithful baseline *configuration*, not a claim about all SQLite deployments. No
population claim.

## 18.2 One-pass direct build (scan the original source)

**Question.** The direct build already held the exact source, yet it reconstructed
it from the authority twice more than necessary (a materialization just to detect
ZIP-vs-PDF, and a second one inside `ingest_pdf_with` only to hand the bytes to
`scan()`). Can that source → authority → source round trip be removed without
changing any stored byte or the exactness closure?

**Receipt.**
[`2026-10-08-phase18-onepass-61b11350`](../../evidence/campaigns/2026-10-08-phase18-onepass-61b11350/).

**Result — ADOPTED; wall −6.1%, peak RSS median −28.8%; the contract gap held at
7.21×.** `FieldStore::ingest_verified(descriptor, source, limits)` parses,
materializes, and **byte-compares to the caller's source** (the one remaining
round trip is now a *verification*, not a discarded copy); new
`ingest_pdf_direct` / `ingest_package_direct` verify against and scan the
**original** input. `encode` and `field-ingest` are byte-for-byte unchanged
(they share the same `ingest_parsed` tail).

A/B on the 9-document direct-field court (the untouched `current` control agreed
within ~1%: 7831 vs 7895 ms):

| lane | direct sum ms | direct median ms | direct RSS median KB |
|---|---:|---:|---:|
| before | 4,012 | 574 | 15,184 |
| after | 3,769 | 524 | 10,812 |
| Δ | **−6.1%** | −8.7% | **−28.8%** |

Contract court: VOLE build sum **11,905 → 11,689 ms** and the gap **7.24× →
7.21×** — i.e. **it did not move**. Exactness 9/9 for each of the three closures
(current field / direct field / direct authority decode) and **12/12** on the
contract court; the observation schedule is identical (**53 equal / 46
decline-equal / 0 divergent**); the stored authority blob, its SHA-256, and the
field id are unchanged.

**Interpretation.** The removed materializations were *cheap* — a `RAW`
literal-object decode is essentially a `memcpy` — so eliminating two of three
passes buys only ~6% wall. The robust win is peak RSS: the extra source-sized
buffers no longer coexist (large docs −21 to −32%). The contract build wall is
dominated by the descriptor encode and the `with_observation_index` re-serialize,
neither of which this item touches.

**Limitation.** Wall is single-run and the ~6% is within small-document noise; RSS
is the trustworthy signal. Two ZIP scans (detection + adapter) remain. The ~7.2×
contract gap remains.

## 18.3 Observation index before the single serialize

**Question.** `field-build --profile runtime` produced the authority in two steps:
`encode_with(Raw)` serialized the descriptor, then `with_observation_index`
**parsed that blob and re-serialized it whole** just to append the ignorable
`OBSERVATION_INDEX` record — a source-sized parse plus a source-sized serialize on
the largest inputs. Can the index be attached *before* the court's single
serialize?

**Receipt.**
[`2026-10-08-phase18-inindex-b7046f0`](../../evidence/campaigns/2026-10-08-phase18-inindex-b7046f0/).

**Result — ADOPTED; wall-neutral, large-document RSS −14–16%.**
`Descriptor::with_observation_index(limits)` is now a **pure method** of the
descriptor; `encode_with_observation_index` enriches the forced candidate
**before** the court serializes it, so the court's single
serialize/parse/materialize round trip covers the enriched authority. `encode`
still emits **no** index, and the witness test asserts
`encode_with_observation_index(…) == with_observation_index(encode_with(…))`
**byte for byte**, so the stored authority is identical (and descriptor `parse`
is unchanged, so older descriptors still decode identically).

| lane | direct sum ms | direct median ms |
|---|---:|---:|
| before (18.2 HEAD) | 3,723 | 524 |
| after | 3,617 | 491 |
| Δ | −2.8% | −6.3% |

The untouched `current` control moved **+3.8%** between the two runs (7800 → 8099
ms), so the wall delta is **within the noise floor**. The robust signal is RSS on
the large documents: `nasa-pdf-0007` 73,476 → 62,032 KB (−15.6%), `nasa-epub-0006`
78,408 → 67,292 KB (−14.2%), `nist-docx-0001` 19,816 → 16,944 KB (−14.5%). The
contract court VOLE sum is flat (11,689 → 11,605 ms, −0.7%) and the *ratio* rose to
7.52× only because SQLite's C5 sum fell that run (1621 → 1544 ms).

**Key isolation (the finding that sets up 18.4/18.5).** The contract VOLE sum is
dominated by one document: **`nist-pdf-0017` is 9425 of the 11,605 ms (81%)**; the
other 11 documents sum to 2180 ms. Its `field-build` writes **6842 seed-node
files**: the same build is **9.40 s** on the bind-mounted `/work` store but
**1.39 s** into `/tmp` (identical 6842 files). So the dominant contract-build term
is a **small-file store-write effect on the bind mount**, not the descriptor encode
or the observation-index reserialize.

**Interpretation.** Removing two full passes over the source buys only ~3% wall;
the encode court's own serialize+parse+materialize, the Stage-B scan, and the store
write still dominate. The one-pass form's clear win is one fewer source-sized
buffer. A phase that wants to move the gap must attack **seed-store write
amplification**, not the descriptor encode.

**Limitation.** Wall A/B is single-run and within the control's own variance; only
the RSS reduction is trustworthy. The observe-lane archive identity is established
by the witness test; if `with_observation_index` and the enriched serialize ever
diverge, that test is the tripwire. The dominant store-write term is untouched and
out of this item's scope.

## 18.4 The packed store as an ingest-write optimization — FALSIFIED

**Question.** 18.3 isolated the contract-build term to a store write that emits
6842 files for `nist-pdf-0017`. The packed seed store collapses that to a handful
of files. Does packing therefore collapse the build term too?

**Receipt.**
[`2026-10-08-phase18-contract-packed-fb23021`](../../evidence/campaigns/2026-10-08-phase18-contract-packed-fb23021/).

**Result — FALSIFIED. Packed is a storage-shape win only.** Same-run contract
court (best-of-3 min):

| VOLE build lane | build ms (sum) | vs SQLite (1597) |
|---|---:|---:|
| direct `field-build` (fs) | 11,715 | 7.34× |
| direct `field-build --packed` | 11,312 | **7.08×** |

The packed substrate moves the gap by **−0.25×** only — within noise; and
essentially all of the 10.74× → 7.2× reduction is the direct `field-build` path
(18.1), not the packed store. The file count *did* collapse (below), but the build
term did not move, because `PackedSeedStore::insert` called `f.sync_data()` **once
per seed node**, so the packed store issued **6792 `fdatasync`** — the same count
as the fs store's **6842 `fsync`**. The term is sync **latency**, not file count.
`observe-batch` also rejected `--packed` with a typed `UnsupportedFeature` (rc 6),
so the packed store could not serve the warm session.

| VOLE backend | files | directories | regular-file B (sum) | vs fs files |
|---|---:|---:|---:|---:|
| fs-direct (`seed/`) | 8,621 | 9,618 | 7,168,131 | 1.00× |
| packed (`fieldpack/`) | 120 | 202 | 7,778,087 | **0.014×** |

Storage stays VOLE's edge (packed = **0.53×** SQLite), but packed regular-file
bytes are **1.09×** the fs store (block alignment + index), so the packed win is a
*shape* win (inode/directory pressure), not a byte win. [SUPERSEDED as a
competitor statement by [Phase 22.1](phase-22-results.md): against a tuned
equal-contract SQLite the storage advantage is **~0.76–0.80×**, not 0.53×.]

**Interpretation.** A file-count collapse is **not** an ingest-write win when the
durability barrier is still per node. The named hypothesis — the packed store
collapses the bind-mount store-write term — is falsified for this tool.

**Limitation.** The bind-mount sync latency is a host-filesystem effect; the
`/tmp` packed build was ~1.4 s both before and after, so the residual is real work.
Best-of-3 on a bind mount whose variance is up to ~2×.

## 18.5 Batched packed-store durability — the win

**Question.** If the contract-build term is a per-node durability sync, can the
append-only packed store sync **once per segment** instead of once per record,
without weakening exactness or crash consistency — and can `observe-batch` then
serve the packed store?

**Receipts.**
[`2026-10-08-phase18-batched-sync-14a7e6f`](../../evidence/campaigns/2026-10-08-phase18-batched-sync-14a7e6f/)
(the mechanism + same-run court) and
[`2026-10-08-phase18-contract-packed-14a7e6f`](../../evidence/campaigns/2026-10-08-phase18-contract-packed-14a7e6f/)
(the re-run contract court; its fixture prose is stale — the raw tables are
authoritative). Decision: [ADR-0053](../adr/0053-batched-packed-sync.md).

**Result — ADOPTED; the build gap folds 7.08× → 0.82× (VOLE now builds faster).**
`SyncPolicy { Batch (default), Each }`: `Batch` appends records to the open segment
and syncs once per segment — at **seal** (which also publishes the immutable
`.idx`) and at an explicit **flush** — instead of once per record; `Each` restores
the pre-18.5 per-record `fdatasync`. `FieldStore::put_field` **flushes before
publishing a manifest**, so a durable manifest never references a non-durable node.
`observe-batch` gained `--packed` (the rc-6 rejection is removed).

Worst document `nist-pdf-0017`, bind-mounted store (mechanism probe):

| lane | before | after |
|---|---:|---:|
| packed wall | 9,392 ms | **1,415 ms** |
| packed `fdatasync` | 6,792 | **0** |
| packed `fsync` | 52 | 54 |
| fs-direct wall | 10,181 ms | 9,963 ms (unchanged) |

Policy probe (best-of-3 min) on the same document: `Batch` **2644 ms** vs `Each`
**9230 ms**. Same-run 12-document contract court (best-of-3 min):

| VOLE build lane | build ms (sum) | vs SQLite (1893) |
|---|---:|---:|
| direct `field-build --packed` | **1,552** | **0.82×** |
| direct `field-build` (fs) | 11,874 | 6.27× |
| SQLite C5 | 1,893 | 1.00× |

`nist-pdf-0017` moves **9187 → 1382 ms**. Storage is unchanged (**7,778,087 B =
0.53×** SQLite; 120 vs 8621 files), confirming batch changed no stored byte. The
warm one-session lane with `--packed` now exists: VOLE **226 ms** vs SQLite
**208 ms** over C0–C5 (**1.09×**; **1.11×** at steady state C1–C5), versus the
fs-direct warm reference — the substrate does not change the query path. Cold ties
sum 895 vs 933 ms. [SUPERSEDED as competitor statements by
[Phase 22.1](phase-22-results.md): the **0.53×** and **1.09×** figures are against
the historical control; against a tuned equal-contract SQLite storage is
**0.762× `full`**, cold **0.812× `full`**, and warm is a **loss** (**1.211×
`full`**).] Exactness `materialize --exact --packed` is **12/12** (length +
SHA-256; worst doc len 1,466,246 = 1,466,246, `byte_compare = equal`).

**Crash-consistency semantics (precise).** The packed store is append-only and
self-describing (a `u32` length prefix + body per record), so recovery is a
**prefix** recovery: the framing scan stops at the first prefix that is absent,
zero, oversized, or whose body does not fit, and every record before that point is
recovered whole. The recovered set is **always a prefix** of the appended sequence;
the torn tail is discarded (truncated on a read-write reopen, ignored on a
read-only one); **no partial node is ever observable**, and every fetched node is
re-hashed against its id; a sealed segment is never rewritten. With `Batch`, a
crash may lose records not yet forced to stable storage (at most those appended
since the last seal/flush) — but never a published field, because a manifest is
flushed first. Unit tests pin prefix recovery
(`batch_policy_recovers_a_complete_prefix_after_a_torn_tail`) and policy-independent
bytes (`sync_policy_does_not_change_stored_bytes`).

**Interpretation.** The build-position inversion came from **deleting unnecessary
durability syncs** — not from parallelism and not from a codec. `Batch` is the
default because it preserves every correctness invariant and only moves *when* a
record becomes power-durable; the stronger per-`put_node` barrier remains available
as `--sync=each`.

**Limitation.** `Batch` can lose an unflushed suffix on power loss; the
flush-before-manifest ordering keeps every *published* field consistent, enforced
by construction rather than by a power-loss test. Bind-mount wall variance is up to
~2×; build figures are best-of-3 mins while the mechanism probes are single-shot.
Packed warm RSS is not measured. The court's 12-document subset is not the frozen
population.

## Cross-cutting notes

- Every exactness statement in Phase 18 is the same invariant:
  `materialize(descriptor) == original_bytes` (length + SHA-256 + `cmp`). 18.2 and
  18.3 carry the three-way direct closure (current field / direct field / direct
  authority decode) and the 12/12 contract closure; 18.5 adds the packed closure.
- **No cap was raised and no validate-or-decline rule was weakened.** 18.2 and 18.3
  remove *materialization* work; 18.4 records a falsified file-count hypothesis;
  18.5 removes *durability* work with an explicit, typed policy
  (`--sync=batch|each`) and a documented crash-consistency rule. No wire byte, the
  decode path, or `encode` output changed.
- No corpus or schedule was tuned against any measurement; the `real100-v1`
  manifest is frozen by SHA-256.
- The one mechanism that actually moved the build gap is **sync batching**. The
  direct build (18.1/18.2/18.3) removed most of the penalty; the packed store
  (18.4) removed none of it until its syncs were batched (18.5).
