# Findings

Consolidated results of the VOLE-Document programme, superseding the per-phase
narratives. Every number links to a sealed receipt under `evidence/campaigns/`
and to an ADR. Decision record: [ADR-0023](../adr/0023-consolidated-findings.md).
Phase-11/12 results: [phase-11-results.md](../phases/phase-11-results.md),
[phase-12-results.md](../phases/phase-12-results.md).

Scope, stated once: VOLE-Document is a reversible representation stack for a
document's bytes. It is **not a compressor** (whole-file size loses to generic
lossless tools) and **not a database** (a source-retaining SQLite+FTS5 baseline
wins the large-document frontier). Every corpus is locally generated and
deterministic; nothing here is a population claim. Exactness is stated as the
invariant, never as a competitive win.

## Current conclusions

The current VOLE representation stack does not beat purpose-built baselines on
any measured axis. The durable outputs are byte-exactness, an auditable/typed
reconstruction representation, and a complete, receipted record of the
negatives.

Two scoped results survive, and both are bounded:

1. A small partial-decode / bytes-read win on large documents for late
   random-access queries, versus **non-seekable sequential** codecs only
   (ADR-0018/0019). It never beats zstd's decoder, loses to fine-block seekable
   formats, and carries an offset-independent ~440 KB floor.
2. Whole-object dedup of identical opaque files in the content-addressed store
   (ADR-0021) — real but marginal versus CDC (`repeat-bin` `U = 133,048 B` vs
   CDC+zstd `133,863 B`), and large only versus per-file LZ (the wrong
   comparison for a sharing axis).

There is one under-claimed qualitative capability, stated as a capability and
not a bytes-read win: a single `.voldoc` artifact simultaneously provides full
byte-exact archival materialization and structural observation
(`--pdf-object`/`--pdf-stream`/`--pdf-revision`), which byte-range-seekable formats
cannot without first reconstructing and re-parsing the document.

Phase 11 adds a persistent procedural field on which scoped wins are measured
(warm observation overhead + wall; exactness after source removal), without
overturning the verdict. Phase 12 extends the field to PDF + DOCX + EPUB; its
courts are deliberately mixed and its one durable-work claim is a recorded
negative. Details below.

## Positive results

| Result | Scope / baseline | Receipt | ADR |
|---|---|---|---|
| Exactness holds (all phases) | every admitted input returns byte-identical bytes (length + SHA-256 + `cmp`) | all campaigns | [0001](../adr/0001-exact-bytes-only.md) |
| Warm narrow-observation overhead + wall | `0` descriptor bytes read, 8.4–8.9 KB overhead (~99 µs) — an *overhead-only* figure | `2026-10-06-phase11-desc-free-a8ad6f4` | [0027](../adr/0027-cost-accounting.md) |
| Partial-view decode CPU | ~2–5× faster than gzip, ~4–13× than xz on late queries; never beats zstd's decoder; RSS a loss | `2026-10-05-phase7-partial-a5764c9` | [0018](../adr/0018-partial-materialization.md) |
| Random-access bytes read | 12–21× fewer bytes than non-seekable sequential gzip/zstd/xz prefixes (late region) | `2026-10-05-phase8-seek-08de2a9` | [0019](../adr/0019-seek-based-io.md) |
| Whole-object dedup of identical opaque files | `repeat-bin` `U = 133,048 B` — marginal vs CDC+zstd `133,863 B` | `2026-10-05-phase9-store-fdb2845` | [0021](../adr/0021-cross-document-sharing-result.md) |
| Cross-format exactness + representation identity | all three formats byte-exact after source+descriptor deletion (removal 38/38, triplet 96/96); one blob shared DOCX↔EPUB | `2026-10-06-phase12-removal-dc4d5a3`, `…-triplet-dc4d5a3`, `…-share-e7ef693` | [0034](../adr/0034-cross-document-identity-sharing.md) |
| Small-document lifetime frontier | beats A0 on the small-document frontier and the cold one-time comparison, on a self-authored 841 B–61 KB corpus | `2026-10-06-phase12-lifetime-ablations-06db12a` | [0035](../adr/0035-phase12-lifetime-benchmark.md) |
| Package-index-only cannot explain the small-document win (`N5` — falsified) | package index (`unzip -p`/`zipfile.read` + byte substring) | answers **0/72** structural selectors vs the field's **72/72**; only 59/96 answers are even byte-reachable | `2026-10-07-phase13-n5-21948bd` | [0035](../adr/0035-phase12-lifetime-benchmark.md) |
| A fourth format (ODT) enters exactly and queryably | ODF package over the shared ZIP layer (not OPC) | `len`+SHA-256+`cmp` after source **and** descriptor deletion in a fresh process; typed declines on a bad manifest | `2026-10-07-phase13-odt-95c486d` | [0038](../adr/0038-odt-adapter-scope.md) |
| Packed seed store (`fieldpack`) | one-file-per-node reference; identity (`NodeId`) unchanged | file bytes `250,333,395 → 252,046,151` (**1.007×** — parity; the "0.719× persistent bytes" was a `du -sb` directory-inode artifact, ADR-0049), file count `25,574 → 237` (**0.009×**, 111× fewer), directory count `218,853 → 3,511`, cold wall parity (**0.991×**); field id identical 12/12, byte-exact 12/12 both | `2026-10-07-phase15-packed-8c195e8`; corrected by `2026-10-08-phase16-storage-correction-2978e1d` | [0043](../adr/0043-packed-seed-store.md) |
| `zlib-rs` inflate backend | shipped `miniz_oxide` scalar; bar ≥1.25× GB/s **and** ≤1.10× RSS, byte-identical | **1.58×** GB/s at **1.00×** RSS, 0 mismatches over 52,498 real members — meets the bar, **adopted** as the shipped backend in 16.1 (end-to-end field-ingest **0.917×**, byte-identical) | `2026-10-07-phase15-deflate-e676166`; adopted `2026-10-08-phase16-zlib-ebb6636` | [0045](../adr/0045-deflate-backend-ablation.md) |
| Direct build + batched durability (equal-contract subset) | a source-retaining SQLite Full baseline, on the same escalating C0–C5 contract, 12 docs | **build: paired median 0.182 (95% CI 0.102–0.228)** — 11 win / 0 tie / 1 loss; ratio-of-sums reads ~0.96 (one heavy doc dominates VOLE's total). **storage 0.53×**, **cold a tie**, **warm ~1.29× (modest loss, marginal)**. Exactness 12/12. **Superseded as competitor statements by Phase 22.1** (see corrected-claims row): against the tuned equal-contract envelope storage is **0.762× `full` / 0.805× `adaptive`**, build **0.219× `full`**, cold **0.812× `full`**, warm a **loss** (**1.211× `full`**) | `2026-10-08-phase19-repeat-d6c8c4c`, `…-phase19-warm-6b66eab`, `…-phase18-batched-sync-14a7e6f` | [0051](../adr/0051-direct-field-ingestion.md), [0053](../adr/0053-batched-packed-sync.md), [0054](../adr/0054-repeatability-and-paired-measurement.md) |
| Direct build over the full frozen population | `real100-v1`, 100 documents, `field-build --profile runtime --packed --sync=batch` | **100/100 built** (the old two-step path was 97/100; the direct path recovered all three large PDFs), ≤87.5 s each, peak RSS ≤2,342 MiB; **exactness 100/100**; bytes 1.036× source; +5.3% bytes vs the searching path (it fixes RAW instead of searching). On this population VOLE bytes are ~0.96× the SQLite db, **not** the 0.53× of the subset | `2026-10-08-phase19-real100-direct-954dbc2` | [0051](../adr/0051-direct-field-ingestion.md) |
| Adversarial durability of the packed seed store | 1,300-case fault-injection court over `fieldpack/` under **both** `SyncPolicy::Batch` and `Each` (process death, storage corruption, deterministic abort) | **1,300 PASS / 0 FAIL / 0 CRITICAL**; `bad_hash` **0**; prefix violations **0**; whole-node re-hash gate rejected **212,812** nodes across **110** cases; corruption fails closed typed. **Scope: proves ordering / no-partial-node / prefix recovery / fail-closed — not true power loss or torn rename** (page cache survives SIGKILL; `write_atomic` never dir-fsyncs) | `2026-10-08-phase20-crash-47acde7` | [0053](../adr/0053-batched-packed-sync.md) |
| Large-source memory bound (direct build) | the encode court's decode-before-commit proof held **six** source-sized buffers; drop a redundant copy (move, not clone), **no wire change** | RSS/source **5.998× → 3.997×** (−33 %); `nasa-pdf-0001` **2342 → 1563 MiB**; 1 GiB synthetic **rc 137 @6113 MiB → rc 0 @4099 MiB** (~1 GiB now fits); exactness **16/16**; descriptor SHA-256 identical **15/15**; wall unchanged; no cap raised | `2026-10-08-phase20-memory-7b897ba` | — |
| C4b closed by a typed external layer | corpus/external lineage supplied **equally** to both lanes as an explicit `ExternalContext` stored beside the field (basis `external-metadata`, `exact=false`); never in the seed DAG, index, manifest, or exactness authority | plain `--metadata` and `materialize --exact` **byte-identical** with/without the layer; both lanes answer C4b **12/12**, tuples match **12/12**; sidecar **1001 B** / attach **26 ms** / query **8 ms** vs SQLite **19 ms** | `2026-10-08-phase20-c4b-5ab2e76` | [0055](../adr/0055-external-context-typed-external-metadata.md) |

## Negative results

| Lost axis | Against | Why | Receipt | ADR |
|---|---|---|---|---|
| Whole-file size | gzip/zstd/xz/brotli | the DRA + typed-residual + entropy representation is coarser than LZ77; structural prediction of plain syntax removes fewer bytes than its per-site/model overhead adds | `2026-10-05-phase7-baselines-7b9f662` (0/27) | [0017](../adr/0017-generic-lossless-baselines.md) |
| Random-access I/O | fine-block seekable/blocked formats | the index+directory+graph floor is read together; compact block formats read only the covering block(s)+index | `2026-10-05-phase8-seek-08de2a9` (bgzip 23,808 B; xz-64KiB 15,344 B) | [0019](../adr/0019-seek-based-io.md) |
| Cross-document sharing (object granularity) | per-file LZ and generic CDC | sharing is whole-object-granular; `externalize` emits 0–1 objects/file, so sub-object sharing is missed | `2026-10-05-phase9-store-fdb2845` (`U = 3,369,900 B` vs LZ 1,304,307 vs CDC 771,383) | [0021](../adr/0021-cross-document-sharing-result.md) |
| Cross-document sharing (finer-than-object) | strongest CDC and `tar | xz -9e` | fine-unit unique lower bound `208,001 B` loses to CDC `160,668 B` and `tar|xz` `92,752 B`; per-stratum loss | `2026-10-06-phase11-share-d9f818a` | [0028](../adr/0028-finer-than-object-sharing.md) |
| Cross-document durable work reuse (`N3`) | the reuse fraction itself | warm `0.339907` falls to `0.0` after `cache --clear` (in-process and fresh process); only representation identity is shared | `2026-10-06-phase12-share-controls-dce2705` | [0034](../adr/0034-cross-document-identity-sharing.md) |
| Encoder-only parametric search | fixed heuristic vs exhaustive grid | the fixed heuristic already attains the exhaustive minimum on every workload, so the search adds zero bytes | `2026-10-05-phase10-governor-d2b09c9` | [0022](../adr/0022-encoder-only-search-governance.md) |
| Large-document lifetime frontier | source-retaining SQLite+FTS5 (A1) | A1 wins the large-document byte frontier (N=10–1000) and wall/CPU at N=1000 | `2026-10-06-phase12-lifetime-3eaf576` | [0035](../adr/0035-phase12-lifetime-benchmark.md) |
| Partial-view memory | sequential gzip | peak RSS ~38 MB vs gzip ~1.2 MB (it is a decode-CPU win, not an allocation win) | `2026-10-05-phase7-partial-a5764c9` | [0018](../adr/0018-partial-materialization.md) |
| PDF `/Length`/revision as a *size* mechanism | the current VOLE ladder and generic compressors | regenerating a field costs a `Mark`+`Emit` per site, more than the digits removed (0 wins / 21 losses) | `2026-10-07-phase13-pdf-length-revision-12fc84e` | [0036](../adr/0036-pdf-length-revision-size.md) |
| PDF COS grammar/templates as a whole-file win | generic compressors | a bounded syntax grammar beats an order-0 lane but not LZ (0 wins vs brotli/xz/zstd on the 7 files where proposed) | `2026-10-07-phase13-pdf-grammar-dfba2a4` | [0037](../adr/0037-pdf-grammar-templates.md) |
| Byte-level partial-materialization checkpoints | the observation index | the checkpoint record is redundant and *larger* than the index it replaces (16 B/op vs 9 B/op): every query reads more bytes, op work identical | `2026-10-07-phase13-checkpoints-9d1306a` | [0039](../adr/0039-partial-materialization-checkpoints.md) |
| Resident runtime (repeated observation) | the cold per-observation lane | residency wins only `<100KiB`/`100KiB-1MiB`; loses `1-10MiB`/`10-50MiB`/`50-100MiB`; aggregate `text_repeat` cold **7 ms** vs resident **9 ms** — the cold `narrow_probe` short-circuit is lost above ~1 MiB | `2026-10-07-real100-release-resident-78f7ea8` | [0042](../adr/0042-resident-session-negative.md) |
| Adaptive procedural promotion | the SQLite-Minimal/Full/Adaptive frontier | all three pre-registered falsifiers fire; promoted bytes cut durable bytes **0.0%** (bar 20%); `sq_full` fastest at every depth; mechanism ships opt-in/default-off | `2026-10-07-phase15-diversity-4786f8e`, `…-revision-4786f8e` | [0046](../adr/0046-adaptive-promotion-refuted.md) |
| Warm one-session query latency (durable) | the resident session's own descriptor parse | the session pays a one-time full `Descriptor::parse` (~1.7 ns/B) + `Field::open` that decoder authority requires; no safe lever exists (a lazy/partial open cannot help this contract; no redundant re-read; relaxing a validation would weaken decoder authority) | ~1.29× loss; profiling is the mechanism; paired N=100 before/after is a **NULL** (1.292 → 1.322, CI half-width ±0.311, MDE ≈0.44) — recorded **durable** | `2026-10-08-phase20-warm-5ed5957` | [0054](../adr/0054-repeatability-and-paired-measurement.md) |
| Durable cross-root derived work (`N3`, canonical derivation identity) | the reuse fraction itself | cross-member derived reuse **0 nodes**; post-`cache --clear` reuse above the floor **0**; only representation identity shared (44 nodes, 4 resources); borg CDC saved 32,158,196 bytes vs **0** derived bytes; not built | `2026-10-07-phase15-crossroot-7429d61` | [0047](../adr/0047-durable-cross-root-derivations-negative.md) |
| Large-PDF `encode` (on the release binary, re-measured — **superseded by 16.3 and 19.3**) | the 6 GiB lane cap | 3 of 5 `>100 MiB` PDFs failed at the time: `nasa-pdf-0001` OOM-killed (rc 137), `nasa-pdf-0002`/`0003` timed out (rc 124); 2 succeed (`0020`, `0024`). **Superseded:** 16.3 fixed the `propose_rle` memory pathology (`nasa-pdf-0001` byte-exact, peak/input `17.7× → 8.9×`; `0002`/`0003` became a **wall**, not memory, limit), and the direct build built **100/100 exact** (19.3); the old two-step `encode` still times out on `0002`/`0003` | `2026-10-07-real100-release-baseline-866f489`; correction `2026-10-08-phase16-largepdf-ce8af8f`, `2026-10-08-phase19-real100-direct-954dbc2` | [0041](../adr/0041-large-pdf-encode-bound.md), [0051](../adr/0051-direct-field-ingestion.md) |

Exactness is not a win in itself: byte-exactness is shared with any lossless
compressor. It is the floor, not an advantage.

### Per-mechanism ablation (relative to the weak order-0 `BYTE_RANS` lane)

These are ablation results, not compression claims (ADR-0017).

| Candidate | Phase | Result | ADR |
|---|---|---|---|
| `PDF_CHANNELS` | 4 | loses to `BYTE_RANS` (`bigtext.pdf` 46,432 vs 38,142 B) | [0010](../adr/0010-typed-channels-rejected.md) |
| `PDF_LAYOUT` | 5 | loses on DRA framing (0/7 classic-xref files) | [0011](../adr/0011-layout-prediction-framing.md) |
| layout-v2 packed | 5.7 | beats RAW at scale (`many.pdf` 10,069 vs 10,215) but loses to `BYTE_RANS` (5,181) | [0012](../adr/0012-packed-framing-threshold.md) |
| `PDF_LAYOUT_RANS` | 5.8 | 0 win / 8 lose / 3 decline | [0013](../adr/0013-layout-rans-not-profitable.md) |
| `PDF_DEFLATE_REPLAY_RANS` | 6 | wins on one synthetic fixture (`flate.pdf` 36,102 vs 49,291 B) when plaintext is both shared and large/weakly coded; loses/declines on every genuinely transformed producer output | [0015](../adr/0015-deflate-replay-result.md) |

## Corrected claims

Every over-stated claim was corrected by an independent adversarial reviewer or
by a later sealed measurement; the correction is preserved rather than rewritten,
and `results.json` numbers were left untouched (corrections are prose/amendments).

| Withdrawn claim | Why it was false | Review / correction record |
|---|---|---|
| Phase-6 "win reproduces on qpdf transformer output" | the "qpdf win" is a `--object-streams=preserve` copy of our own fixture's two byte-identical raw streams (99.93% inherited); every genuinely transformed producer output loses or declines (win 3 / lose 8 / decline 12, all 3 self-authored) | [phase7-skeptic-review.md](../reviews/phase7-skeptic-review.md) |
| Phase-7.0b "Cairo: first authoring-generator witness" | the generator repeated one identical page six times; Cairo emitted six byte-identical streams, a region generic LZ compresses ~2× better (gzip 17,382 B; xz-9e 16,852 B) than the 34,574 B "win" over the weak order-0 lane | [phase7b-skeptic-review.md](../reviews/phase7b-skeptic-review.md) |
| Phase-6 win region = "shared or weakly coded" | each conjunct alone loses (four negative controls); the win requires shared **and** large/weakly-coded plaintext | [phase6-skeptic-review.md](../reviews/phase6-skeptic-review.md) |
| Phase-6 descriptor labels ("three *shared* plaintext channels", "six stored bitstreams", "levels 0/1 almost verbatim") | the real descriptor is `streams=6 replayed=6 channels=3 objects=6`, only `p1` shared; `BYTE_RANS` order-0-codes rather than stores; level 1 is ~19% (6,197 B), only level 0 is verbatim | [phase6-skeptic-review.md](../reviews/phase6-skeptic-review.md) |
| Phase-7c primary metric = `descriptor_bytes_traversed + entropy_bytes_decoded` | double-counts the decoded channels; superseded by `descriptor_bytes_traversed` alone (412,161–433,694 B) | [phase7c-skeptic-review.md](../reviews/phase7c-skeptic-review.md) |
| Phase-7.0 `correction/compressed` p50 = 0.004518 | misattributed subset median; the corpus-wide p50 is 0.014716 | [phase7-skeptic-review.md](../reviews/phase7-skeptic-review.md) |
| Phase-8 "bytes-read win" as a general random-access-I/O result | holds only vs non-seekable sequential codecs; vs fine-block seekable formats VOLE reads 2.6–30× more, and the "reads only the 64-byte header…" wording hid a ~440 KB floor | [phase8-skeptic-review.md](../reviews/phase8-skeptic-review.md) |
| Phase-9 "the only win is byte-identical opaque repeats"; fixed CDC-zstd 210,836 B | forcing `PDF_DEFLATE_REPLAY` wins `shared-payload` over raw CDC; compressed-CDC is non-deterministic (210,835–210,840 B) | [phase9-skeptic-review.md](../reviews/phase9-skeptic-review.md) |
| Phase-7.3 partial "allocation" win / early boundary ≤ 1–8 MiB | it is a decode-CPU win; peak RSS is a loss; the early boundary is ≤ ~8–16 MiB | [phase7c-skeptic-review.md](../reviews/phase7c-skeptic-review.md) |
| Phase-11 warm byte win vs A1; "357 B working set"; cross-process reuse as recomputation | the 8.4–8.9 KB is overhead-only (the warm process re-reads the cached answer); the seed class is 449 B but the honest cold total is 232–364 KB; reuse is served by the derived cache (re-executes after `cache --clear`) | [phase-11-skeptic-review.md](../reviews/phase-11-skeptic-review.md) |
| Phase-12 hostile-input class tally; "FTS5/BM25 reported separately"; ablation ladder run; `N6` receipt; demo commit; `N3` controls | tally corrected to 16 reject / 22 decline / 7 accept; FTS5 now measured (trigram ties `LIKE` but reads more); ladder then run (A4→A5, A5→A6); `N6` closed (32/32); `N3` violated; demo re-attributed and sealed | [phase-12-skeptic-review.md](../reviews/phase-12-skeptic-review.md) |
| `real100-v1` frontier storage = one footprint, and the numbers were debug-only | the earlier court folded the transient `.voldoc` into the persistent footprint and ran an unoptimized debug binary; Phase 15.1 re-runs the frozen court on the **release** binary and reports **persistent store / optional standalone descriptor / transient ingest** separately (never folded); coverage/exactness are unchanged (deterministic), so only the accounting was corrected | [phase-15-results.md](../phases/phase-15-results.md) 15.1 |
| Phase-15.3/16.2 "VOLE persistent footprint is 1.377× SQLite's" and "`--packed` cuts persistent bytes to 0.719× / closes the gap" | measured with `du -sb`, which on the bind-mounted host is `--apparent-size`: it counts **4096 B per directory inode**. The one-file-per-node `fs` store carries ~2 directories per file (`218,853` here), so its `du -sb` was inflated **~52 %** (896,421,888 B of pure directory overhead), the packed store **0.8 %**, the single `.db` **0 %**. File-bytes-only accounting gives fs/SQLite **0.906×**, packed/SQLite **0.914×**, packed/fs **1.009×** — both VOLE backends are at/below SQLite on file bytes and packed ≈ fs; packed's surviving benefit is **file/directory count** (open/syscall economics), not bytes | [CORRECTION.md](../../evidence/campaigns/2026-10-08-phase16-storage-correction-2978e1d/CORRECTION.md), [0049](../adr/0049-storage-accounting-correction.md) |
| Phase-16.5 "VOLE builds **`~10×` slower** than a source-retaining SQLite baseline" under the equal contract | superseded: the direct `field-build` (17.1) plus batched packed-store durability (18.5) **invert** it. Phase 19 re-reads it with paired, interleaved repetitions: paired per-rep median **0.182** (95% CI **0.102–0.228**, entirely < 1; 11 win / 0 tie / 1 loss) — VOLE builds **faster**, though the ratio-of-sums estimator reads **~0.96** because one heavy document dominates VOLE's total, so the magnitude is estimator-dependent | [phase-18-results.md](../phases/phase-18-results.md), [phase-19-results.md](../phases/phase-19-results.md), [0053](../adr/0053-batched-packed-sync.md), [0054](../adr/0054-repeatability-and-paired-measurement.md) |
| Phase-18.5 single-run "build **0.82×**" and "warm **1.09×**" point estimates | re-read by Phase 19 with paired, interleaved repeats and fixed-seed bootstrap intervals: the build **win stands** (paired median 0.182, CI below 1.0) but its size is estimator-dependent (**~0.96** by ratio of sums, one heavy document dominating); the warm lane is a **modest ~1.29× loss** (pooled median 1.292, 95% CI 0.994–1.658, includes 1.0; geometric mean 1.283, 95% CI 1.069–1.535) — the N=10 warm estimate was under-sampled | [phase-19-results.md](../phases/phase-19-results.md), [0054](../adr/0054-repeatability-and-paired-measurement.md) |
| Phase-18.5 / Phase-19 "storage **0.53×**", "build **0.82×**", "warm **1.09×**" stated against "SQLite" unqualified | those figures were all measured against the Phase-18 **historical-control** configuration, which carried the baseline's **contract-dead** FTS/trigram indexes (the contract has **no search observation**). Phase 22.1 strengthened the competitor **first** (six purpose-tuned SQLite configurations) and re-measured. Against the tuned equal-contract envelope VOLE storage is **0.762× `full` / 0.805× `adaptive`** (not 0.53×; the dead index is ≈**+44%** bytes / 4.5 MB over 12 docs), build **0.219× `full`** (0.194× `hist` — the advantage **survives but shrinks**), cold **0.812× `full`**, and warm is a **loss** (**1.211× `full`**, 95% CI 1.006–1.483; 1.253× `hist`). **No capability gap remains** | [phase-22-results.md](../phases/phase-22-results.md) 22.1 |

## Open hypotheses

Concrete, falsifiable next steps, each with an explicit prior that it may also
lose — see [Roadmap](roadmap.md) for the full list and the Phase-13 proposals.

1. A structural model stronger than LZ, removing structure without paying a
   per-site plan. Prior: loses.
2. Producer-population corpora, to test whether the one structural win's enabling
   condition exists in the wild. Prior: loses/unknown.
3. A like-for-like lifetime frontier on the common answered set (`N1`/`N2`).

Finer-than-object shareable units were hypothesis 1 of the older list and are now
measured (a recorded negative, ADR-0028).

## Evidence index

- [Evidence index](../evidence/README.md) — campaigns and measurement reports.
- [Status and mechanism ledger](status.md) — the single authoritative status table.
- [Conformance](../reference/conformance.md) — courts, invariants, fuzzing.
- Reviews: [phase-11](../reviews/phase-11-skeptic-review.md),
  [phase-12](../reviews/phase-12-skeptic-review.md), and the Phase-6/7/7b/7c/8/9
  reviews under [reviews/](../reviews/).
