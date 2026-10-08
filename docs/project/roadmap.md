# Roadmap

Open work that is `PROPOSED` or not yet measured. Every item carries an explicit
prior that it may also lose, because every measured attempt so far has.

## Phase-13 open proposals

Phase 13 closes the remaining Phase-12 proposals and the last open gate. Plan:
[phase-13-plan.md](../phases/phase-13-plan.md).

| Item | Question | Prior | Outcome |
|---|---|---|---|
| PDF `/Length`/revision proceduralization as a *size* mechanism | Does persisting revision/xref structural redundancy beat the RAW/`BYTE_RANS`/generic ladders once framing is charged? (Phase 11 persists revisions as *observation* nodes only, never as a size candidate.) | Expected negative | **Measured 13.1 — recorded negative** (ADR-0036): byte-exact, 0 wins vs the ladder and 0 vs generic |
| PDF grammar/templates | Can a bounded grammar/template candidate that pays its definition cost beat a whole-file order-0 lane? | Expected negative | **Measured 13.2 — mixed** (ADR-0037): byte-exact; best VOLE lane on 4/28 (+34,505 B auto-winner) but 0 wins vs generic |
| ODT adapter | ODT is ODF packaged in ZIP, so it reuses the 12.x ZIP layer (not the OPC graph) with an OpenDocument content model. Does a fourth format enter exactly and queryably? | Unknown | **Measured 13.3 — ADOPTED** (ADR-0038): byte-exact (`len`+SHA-256+`cmp`) and queryable after source + descriptor deletion in a fresh process; missing/malformed manifest declines typed with exactness preserved |
| Byte-level partial-materialization checkpoints | The random-access `view`/seek lane was measured (ADR-0018/0019), but literal byte-level checkpoint records were never built. Do they pay their framing cost? | Unknown | **Measured 13.4 — recorded negative** (ADR-0039): byte-exact and advisory (lying/corrupt checkpoints rejected, reader falls back), but the per-op table is redundant with the observation index and *larger* than the index record it replaces (+259 to +1,159 B per byte-range query, identical op work) |
| `N5` package-index-only gate | Is the small-document win reproducible by `unzip -p` + `substr` at the same boundary? The A3/A4 ladder rungs are evidence against it; the mechanical check remains open. | Evidence suggests the field adds value | **Measured 13.5 — falsified** (gate closed): the literal mechanical control (`zipfile.read` + byte substring) answers **0/72** structural selectors while the field answers **72/72** (`2026-10-07-phase13-n5-21948bd`) |

## Phase-15 outcomes

Phase 15 is a performance programme: repair the measurement, then measure the
frontier. Plan: [phase-15-plan.md](../phases/phase-15-plan.md); results:
[phase-15-results.md](../phases/phase-15-results.md).

| Item | Question | Outcome |
|---|---|---|
| 15.1 Court repair | Re-run the frozen `real100-v1` court on the **release** binary with the storage universes split | **Repaired** (measurement): coverage/exactness identical to the debug court (deterministic); VOLE holds `pdf`/`text_repeat` + `docx`/`table`, loses the rest; persistent `2,667,668,262 B` / descriptor `1,704,524,849 B` / A1 db `2,891,784,192 B` (whole-population `du -sb` aggregates; the store figure is method-inflated — ADR-0049); 3 of 5 `>100 MiB` PDFs still fail at `encode` (`2026-10-07-real100-release-baseline-866f489`) |
| 15.2 Resident runtime | Does a resident session + `observe-batch` beat the cold lane? | **Measured — recorded negative/partial** (ADR-0042): wins only below ~1 MiB; cold 7 ms vs resident 9 ms aggregate; mechanism = lost `narrow_probe` short-circuit |
| 15.3 Packed seed store | Does a packed `fieldpack` backend beat one-file-per-node? | **Measured — adopted (seed namespace only)** (ADR-0043, corrected by ADR-0049): file/directory count 0.009× / 111× fewer at **byte parity** (file bytes 1.007×), latency parity, identity unchanged; the earlier "persistent bytes 0.719×" was a `du -sb` directory-inode artifact |
| 15.4 Parallel ingest | Does a bounded worker pool speed up ingest deterministically? | **Measured — implemented, non-default** (ADR-0044): speed-neutral (1.00× at 2/4/8); determinism positive (10/10 across every worker count) |
| 15.5 DEFLATE ablation | Which correct safe-Rust inflate is fastest? | **Measured — recommendation** (ADR-0045): `miniz-simd` enabled (1.11×); `zlib-rs` meets the bar (1.58×, RSS-neutral, byte-identical) and is recommended but **not adopted**; `zune-inflate` disqualified (255 mismatches) |
| 15.6 Adaptive promotion | Does an adaptive promotion governor beat SQLite on diversity/revision? | **Measured — recorded negative** (ADR-0046): all three pre-registered falsifiers fire; mechanism ships opt-in/default-off |
| 15.7 Durable cross-root derivations | Does canonical derived-work identity satisfy `N3`? | **Measured — recorded negative** (ADR-0047): `N3` violated again, 0 cross-member reuse; no Rust change made |
| 15.8 CUDA batch lane | Does a GPU batch inflate lane pay? | **Deferred, not measured** (ADR-0048): gate unopened; the pinned Docker lanes cannot see the GPU; `nvCOMP` proprietary; Docker-only evidence required |

## Phase-16 outcomes

Phase 16 follows Phase 15: adopt the recommended backend, finish the storage
court, fix the large-PDF pathology, continue the residency line, and test
whether SQLite loses under an equal capability contract. Results:
[phase-16-results.md](../phases/phase-16-results.md); ADRs 0049–0050.

| Item | Question | Outcome |
|---|---|---|
| 16.1 `zlib-rs` adoption | Adopt the backend 15.5 only recommended; what is the **end-to-end** effect? | **Adopted** (ADR-0045 fulfilled): one `src/field/inflate.rs` helper owns every inflate; byte-identical (**2,075 members / 17 docs**); `field-ingest` **0.917×** (~8 % faster), `encode` `0.996×` (control), peak RSS `1.000×` (`2026-10-08-phase16-zlib-ebb6636`) |
| 16.2 Full packed court | Does `--packed` beat the A1 SQLite db on the full population? | **Court complete** (95 common-success docs; `fs`/packed field id identical 97/97). Court `2026-10-08-phase16-packed-full-0b21928`; its `du -sb` byte numbers are **superseded by 16.6** |
| 16.3 Large-PDF encode pathology | Why do the `>100 MiB` PDFs fail at `encode`? | **Fixed** (ADR-0041 extended): `propose_rle` pre-allocated ~16× input before its decline; two streaming O(1)-memory passes. `nasa-pdf-0001` now completes byte-exactly; peak/input **17.7× → 8.9×**; **77/77** byte-identical (`.voldoc` SHA-256); `0002`/`0003` remain a **wall** limit, not memory (`2026-10-08-phase16-largepdf-ce8af8f`) |
| 16.4 Resident session + `narrow_probe` | Does giving the session the cold path's short-circuit make residency pay? | **Recorded negative** (ADR-0042 extended): probe works (0 descriptor bytes, ~25 µs, 90/0 equality) but no class flips — cold 6 ms vs resident 9 ms; one-time full `Field::open` dominates. Lever isolated: **lazy session open** (`2026-10-08-phase16-resident-probe-5d331f2`) |
| 16.5 Contract-equivalent court | Does SQLite lose under an **equal capability contract** (C0–C5)? | **Recorded negative for VOLE** (ADR-0050): equality at C0–C3; **VOLE declines C4/C5** (no revision surface); SQLite builds **~10×** faster, warm **~1.47×** faster, +**~3 %** bytes C0 → C4. VOLE's lone edge is storage (`2026-10-08-phase16-contract-45d2c0e`) |
| 16.6 Storage correction | Is `du -sb` the right unit for a one-file-per-node store vs a `.db`? | **Corrected** (ADR-0049): `du -sb` counted 4096 B/dir, inflating `fs` **52 %**. File-bytes-only: `fs`/SQLite **1.377× → 0.906×**, packed/SQLite **0.921× → 0.914×**, packed/`fs` **0.669× → 1.009×**; **"VOLE is 1.377× SQLite" / "packed closes the gap" refuted**; packed's win is file/directory count (`2026-10-08-phase16-storage-correction-2978e1d`) |

### Recorded open question (Phase 16)

The Phase-16.5/16.6 result — **SQLite does not lose under an equal capability
contract**, VOLE's storage edge corrects to **~0.9×**, and VOLE cannot answer
revision lineage — raises an architectural question the user asked to **record,
not resolve**: should the conceptual invention keep competing with SQLite, or
**use an embedded DB as part of its physical substrate** for materialized
observation state? **No switch is decided.** The measurements that would settle
it are enumerated in [ADR-0050](../adr/0050-sqlite-as-substrate-question.md).

## Phase-17 outcomes

Phase 17 follows Phase 16: it removes the search overhead from the runtime build
path and adds the revision-lineage surface Phase 16 recorded as missing, then
re-runs the contract court. Results:
[phase-17-results.md](../phases/phase-17-results.md); ADRs 0051–0052.

| Item | Question | Outcome |
|---|---|---|
| 17.1 Direct source → field ingestion | Can the runtime build path skip the candidate portfolio search without changing exactness or the observation surface? | **Adopted** (ADR-0051): `field-build --profile runtime` fixes `RAW` (`candidates_evaluated == 1`) and builds the authority + field in one process; build wall sum **7,928 → 3,878 ms (2.04×)**, peak RSS median **51,792 → 15,228 KB (3.4× smaller)**, authority **1.007×**, exactness **9/9**, observations **0 divergent**. Recorded caveat: the PDF metadata projection's `object_count`/`graph_ops` are encoder-dependent (`2026-10-08-phase17-direct-field-d83ddba`) |
| 17.2 Revision-lineage surface | Does adding the missing revision surface close C4/C5? | **Surface added; contract still open** (ADR-0052): `Selector::Revisions`/`Representation::Lineage` computed once at ingest, indexed, **O(depth)**, typed decline for non-PDF; the court re-run satisfies **C0–C3** but **C4/C5 still do not close** because the contract's C4 tuple is the corpus family/member/head — external metadata PDF bytes cannot derive (`different observable`) (`2026-10-08-phase17-revision-1179386`) |

### Recorded open question (Phase 17)

The C4 finding re-frames the open question: it is a **contract-definition**
question, not a missing surface. Either the contract's C4 tuple is the right
definition — in which case a single-document exact field is the wrong tool for C4
by construction — or VOLE should **ingest corpus/revision-family metadata as an
explicit external input** and answer the tuple as a derived observation. **No
decision is taken**; see [ADR-0052](../adr/0052-revision-lineage-surface.md).
This complements, and does not supersede, the Phase-16 substrate question
([ADR-0050](../adr/0050-sqlite-as-substrate-question.md)).

## Phase-18 outcomes

Phase 18 is a **build-cost programme**: starting from Phase 17's direct
`field-build`, it removes the remaining unnecessary work from the runtime build,
one mechanism at a time, and re-measures the equal-contract court. Results:
[phase-18-results.md](../phases/phase-18-results.md); ADR-0053.

| Item | Question | Outcome |
|---|---|---|
| 18.1 Contract court with the direct build | Re-run the 12-document C0–C5 court changing only VOLE's build step to `field-build --profile runtime`; what is the composable gap, and what is C4? | **Recorded** — build **10.74× → 7.24×** (the 17.1 2.04× and the 10.74× two-step gap are **not composable**); storage **0.49×**; cold sum **779 vs 772 ms (tie)**; warm **1.33×**. C4 splits: **C4a** document-native lineage is VOLE's answer (4/4 PDFs; typed unsupported for 8/8 docx/epub) and byte-derivable from the baseline's retained blob (a work-location difference); **C4b** corpus/external lineage is harness-supplied metadata (baseline 12/12, VOLE 0/12). `2026-10-08-phase18-contract-direct-f2a34a5` |
| 18.2 One-pass direct build | Can the source → authority → source round trip be removed without changing bytes? | **Adopted** — `ingest_verified` verifies against the caller's source; `ingest_pdf_direct`/`ingest_package_direct` scan the original input; `encode`/`field-ingest` unchanged. Wall **−6.1%**, peak RSS median **−28.8%**; contract gap held at **7.21×**; exactness 9/9 + 12/12. `2026-10-08-phase18-onepass-61b11350` |
| 18.3 Observation index before the single serialize | Can the redundant parse+re-serialize of the authority be removed? | **Adopted** — `Descriptor::with_observation_index` is a pure method; the index is attached before the court's single serialize; `encode` still emits no index and the one-pass authority is byte-identical to `with_observation_index(encode_with(…))`. Wall-neutral; large-doc RSS **−14–16%**. Isolation: `nist-pdf-0017` is 9425/11,605 ms (81%), 6842 files, 9.40 s on the bind mount vs 1.39 s `/tmp`. `2026-10-08-phase18-inindex-b7046f0` |
| 18.4 Packed as an ingest-write optimization | Does the packed file-count collapse also collapse the build term? | **Falsified** — packed 11,312 ms vs same-run fs-direct 11,715 ms vs SQLite 1,597 ms (7.08× vs 7.34×); files collapse but `insert` syncs **per node** (6792 `fdatasync` vs 6842 `fsync`); the term is sync **latency**, not file count. `2026-10-08-phase18-contract-packed-fb23021` |
| 18.5 Batched packed-store durability | If the term is a per-node sync, can the packed store sync once per segment — and can `observe-batch` serve it? | **Adopted** (ADR-0053) — `SyncPolicy { Batch (default), Each }`; `put_field` flushes before publishing a manifest; prefix recovery + no partial node + re-hash gate; `observe-batch --packed` now works. `nist-pdf-0017` wall **9392 → 1415 ms**, `fdatasync` **6792 → 0**; 12-doc contract build **1552 vs 1893 ms = 0.82×** (VOLE builds **~1.22× faster**); storage **0.53×**; warm **1.09×**; exactness **12/12**. `2026-10-08-phase18-batched-sync-14a7e6f`, `2026-10-08-phase18-contract-packed-14a7e6f` |

The build position recorded by Phase 16.5 as VOLE's decisive loss now reads: on the
equal-contract 12-document subset VOLE **builds ~1.22× faster** than the
source-retaining SQLite baseline, **stores ~0.53×** the bytes, **ties** on cold
queries, and is **~1.09×** slower on the warm one-session lane. **C4 is not a
closed contract:** C4a is VOLE's genuine native-lineage observable, C4b remains
open because it is external metadata.

### Recorded open question (Phase 18)

The C4b half is a **contract-definition / external-input** question, unchanged by
the build-cost programme: either the C4 tuple (corpus family/member/head) is not a
single-document derivable fact — in which case a single-document exact field is the
wrong tool for C4b by construction — or VOLE should ingest corpus/revision-family
metadata as an **explicit external input** and answer it as a derived observation.
**No decision is taken;** this remains the Phase-17 question
([ADR-0052](../adr/0052-revision-lineage-surface.md)) and composes with the
Phase-16 substrate question ([ADR-0050](../adr/0050-sqlite-as-substrate-question.md)).

## Phase-19 outcomes

Phase 19 is a **measurement-discipline phase**: it replaces Phase 18's two
single-run point estimates with paired, interleaved, repeated measurements and a
fixed-seed bootstrap interval (19.1), extends the warm lane to N=100 (19.2), and
runs the new direct build over the full frozen `real100-v1` population (19.3).
Results: [phase-19-results.md](../phases/phase-19-results.md);
[ADR-0054](../adr/0054-repeatability-and-paired-measurement.md).

| Item | Question | Outcome |
|---|---|---|
| 19.1 Repeatability court (paired, interleaved, N=10) | Is the Phase-18.5 build ratio < 1.0 and the warm ratio > 1.0, once both lanes are measured repeatedly and interleaved? | **Build established < 1.0** — paired median **0.182** (95% CI **0.102–0.228**), geometric mean 0.203 (0.128–0.395), **11 win / 0 tie / 1 loss** (`nist-pdf-0017` at 4.440×). **Correction:** the ratio-of-sums estimator reads **~0.96**, not 0.82, because one heavy document dominates VOLE's total; the move is SQLite-lane host variance (1893 → 1582 ms), VOLE's sum reproducible (1545.5 vs 1552 ms). **Warm not resolved:** paired median **1.077** (0.809–1.391, includes 1.0), 5/1/6. Best-of-3 (min) vs median-of-10: build 0.977 → 0.963, warm 1.218 → 1.194. Exactness 12/12 both. `2026-10-08-phase19-repeat-d6c8c4c` |
| 19.2 High-N warm repeat (N=100) | Is the warm lane a resolved loss, resolved parity, or still undecided at high N? | **Not resolved under the median estimator, resolved under the geometric mean** — pooled median paired ratio **1.292** (95% CI **0.994–1.658**, includes 1.0); geometric mean **1.283** (95% CI **1.069–1.535**, excludes 1.0); pooled warm medians VOLE 2.25–2.32 ms vs SQLite 1.54–1.65 ms (~0.7 ms slower). A **modest real loss (~1.29×), not parity**; the N=10 estimate was under-sampled. Variance floor CV 5.4% (p90 9.4%); median-CI half-width ±0.332; MDE(80%) ≈0.474. Exactness 12/12; 480 envelopes, 0 mismatches. `2026-10-08-phase19-warm-6b66eab` |
| 19.3 Direct build over the full `real100-v1` | Is the new direct build robust across the whole frozen population, and where is its next limit? | **100/100 built, 100/100 exact** (pdf 60/60, docx 15/15, epub 25/25; rc histogram all 0), recovering the three docs the old two-step path failed (`nasa-pdf-0001`/`0002`/`0003`); wall median **64 ms**; peak RSS median 30.9 MiB / max 2342 MiB — **memory, not wall, is the next binding constraint**. Cold coverage text 92/100, metadata 98/100 (typed declines). vs the old path (file-size-corrected) median **61 ms** vs 2,095 ms (paired 0.083×) but **+5.3%** bytes. **Storage is population-dependent: ~0.96× SQLite on the full population, not the 0.53× of the contract subset.** `2026-10-08-phase19-real100-direct-954dbc2` |

The Phase-18 **0.82×** build and **1.09×** warm headlines are superseded as point
estimates: the build win stands (paired median 0.18, CI below 1.0) but its size is
estimator-dependent (~0.96× by ratio of sums), and the warm lane is a modest
~1.29× loss that the median estimator cannot separate from 1.0 at N=100 while the
geometric mean resolves it.

## Phase-20 outcomes

Phase 20 is a **hardening-and-economics phase** that attacks, in risk order, the
four open problems Phase 19 left: prove (or bound) the packed store's durability
under faults, bound large-source memory, diagnose the warm loss, and decide C4b.
All four plan items are now **complete and sealed**. Results:
[phase-20-results.md](../phases/phase-20-results.md); plan:
[phase-20-plan.md](../phases/phase-20-plan.md) (complete); ADR-0055 and the
Phase-20.1 amendment to ADR-0053.

| Item | Question | Outcome |
|---|---|---|
| 20.1 Crash / power-cut fault-injection court | Does the packed store hold its recovery invariants across an arbitrary crash at every durability boundary, under both sync policies, with recovery exactly a prefix? | **Complete — 1,300 PASS / 0 FAIL / 0 CRITICAL** (families A 1,024, B 236, C 40; Batch 650/650, Each 650/650); `bad_hash` **0**; prefix violations **0**; whole-node re-hash gate rejected **212,812** nodes across **110** cases; corruption fails closed typed. **Scope:** proves ordering / no-partial-node / prefix recovery / fail-closed, **not** true power loss or torn rename (page cache survives SIGKILL; `write_atomic` never dir-fsyncs). `2026-10-08-phase20-crash-47acde7` |
| 20.2 Large-source memory architecture | Can peak memory be bounded for sources ≳1 GiB without changing exactness or the wire? | **Complete — adopted:** RSS/source **5.998× → 3.997×** (−33 %); `nasa-pdf-0001` **2342 → 1563 MiB**; 1 GiB synthetic **rc 137 @6113 MiB → rc 0 @4099 MiB** (≈1 GiB now fits, ~2 GiB headroom; cap boundary ~1.0 → ~1.5 GiB; **no cap raised**). Exactness **16/16**; descriptor SHA-256 identical **15/15**. Residual floor 4 copies (3 needs a wire-type change). `2026-10-08-phase20-memory-7b897ba` |
| 20.3 Warm heterogeneous query | Is the ~1.29× warm loss a closeable constant or durable? | **Complete — no change shipped, with proof.** Dominant term `Descriptor::parse` (~1.7 ns/B) + full `Field::open`; no safe lever (lazy/partial open cannot help this contract; no redundant re-read; relaxing a validation would weaken decoder authority). Paired N=100 before/after is a **NULL** (median 1.292 → 1.322; CI half-width ±0.311; MDE ≈0.44). Loss recorded **durable**. `2026-10-08-phase20-warm-5ed5957` |
| 20.4 C4b / `ExternalContext` | Can corpus/external lineage be admitted as an explicit typed layer beside the field without contaminating it? | **Complete — C4b closes** under an equal external input. Typed, removable `ExternalContext` at `<store>/external/<FieldId>`; `Basis::ExternalMetadata` (`exact=false`); plain `--metadata` byte-identical before/after attach/clear **12/12**; `materialize --exact` **12/12**; external query declines rc 6 after clear **12/12**; both lanes answer and tuples match **12/12**. ADR-0055. `2026-10-08-phase20-c4b-5ab2e76` |

The warm loss is now **diagnosed and durable**; the packed store's durability is
**adversarially attacked with its scope stated**; large sources up to ~1 GiB
**fit**; and **C4a and C4b are both answered**. The Phase-16 substrate question
([ADR-0050](../adr/0050-sqlite-as-substrate-question.md)) is unchanged and no
option of it is chosen.

## Unmeasured gates

- `N4` (decline-rate threshold): no pre-registered threshold exists, so it is
  **not evaluated**.

## Open hypotheses from the consolidated findings

These are concrete, falsifiable next steps. Finer-than-object shareable units
item 1 has since been measured (a recorded negative, ADR-0028).

1. A structural model stronger than LZ — one that removes structure without
   paying a per-site plan (e.g. canonical/parametric layout, nested content
   proceduralization). Prior: loses — four independent plain-syntax
   proceduralizations lost on framing/model overhead.
2. Producer-population corpora — the enabling condition of the one structural win
   (shared plaintext that is also large/weakly coded) has never been observed in
   genuinely transformed producer output, only in our own fixtures. Prior:
   loses/unknown.
3. A like-for-like lifetime frontier on the common answered set (removing
   declined cases from both numerator and denominator), per the Phase-12 skeptic's
   `N1`/`N2` concerns.

## Recorded negatives (do not re-attempt without new evidence)

Whole-file compression (0/27, ADR-0017); seek bytes-read versus fine-block
formats (ADR-0019); cross-document sharing at object granularity (ADR-0021) and
finer-than-object granularity (ADR-0028); cross-document durable work reuse
(ADR-0034/0035, `N3`; re-violated with canonical derivation identity in Phase
15.7, ADR-0047); encoder-only parametric search as a byte win (ADR-0022);
adaptive procedural promotion on the tested corpus (ADR-0046); residency as a
repeated-observation win above ~1 MiB (ADR-0042). See
[Findings](findings.md).

## real100-v1 corpus (frozen, 2026-10-07)

The `real100-v1` real NASA/NIST corpus is frozen at **100 documents**
(branch `real100-v1`; bytes gitignored, manifest + `SHA256SUMS` committed).
Diversity is **66 PASS / 13 PARTIAL / 1 FAIL** of 80 pre-performance gates. See
[the corpus README](../../real100-v1/README.md) and the close-out note
[phase real100-v1 corpus](../phases/real100-v1-corpus.md).

The remaining PARTIAL/FAIL gates are **corpus-spec over-constraints given
published material**, recorded rather than fabricated:

| Quota | Status | Why it cannot be met |
|---|---|---|
| NIST DOCX `long regulatory` | FAIL | NIST publishes no long regulatory Word document; Handbooks 44/130/133/105 are PDF-only |
| NIST `PDF<->DOCX` 8-10 | PARTIAL (1) | Only 5 NIST SP families publish DOCX at all, and the SP slot count is fixed at 5; max is 5, and they compete with `PDF<->EPUB` |
| `glossary/index` DOCX | PARTIAL (1) | NIST publishes a single index-bearing Word document |
| size `<100KiB` / `100KiB-1MiB` / `1-10MiB` | PARTIAL | Their exact targets plus the three satisfied large bands sum to 95, impossible at 100 documents |
| NASA `TR` 8-10 / `handbook/ref` 3-5, `simple born-digital`, `appendix/ref-heavy` | PARTIAL | Technical-doctype minimums sum to 33 > the 30 non-e-book NASA PDFs |

This is a corpus deliverable, not a codec claim; it does not itself assert any
VOLE result.
