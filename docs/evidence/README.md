# Evidence index

Sealed campaign receipts under `evidence/campaigns/<date>-<phase>-<gitsha>/` are
immutable: corrections are amendments, never rewrites. Each records its base
image digest, toolchain versions, `Cargo.lock` SHA-256, git commit and dirty
state, and the exact command line. Independent adversarial reviews live in
[reviews/](../reviews/).

## Campaigns

| Phase | Question | Result | Receipt | Review |
|---|---|---|---|---|
| 1 | exact container + DRA byte-exact? | exact court green (framing, integrity, bounds) | `phase1-7b5fad0` | — |
| 2 | order-0 entropy floor | core 464,474 → full 291,304 over 9 files | `phase2-f6af30b` | — |
| 3 | PDF physical authority exact? | 171 spans, `all_covered`/`all_exact`; `PDF_PHYSICAL` loses RAW 0/9 | `phase3-486aa17` | — |
| 4 | typed lexical channels beat `BYTE_RANS`? | no (`bigtext.pdf` 46,432 vs 38,142) | `phase4-3840bc4` | — |
| 5 | classic-xref layout beats RAW/`BYTE_RANS`? | correct prediction, loses on framing (0/7) | `phase5-7193001` | — |
| 5.7 | packed-framing threshold | beats RAW at scale (10,069 vs 10,215), loses `BYTE_RANS` | `phase5-4521778` | — |
| 5.8 | layout + rANS beat `BYTE_RANS`? | no (0 win / 8 lose / 3 decline) | `phase5-8-cf8048d` | — |
| 6 | exact DEFLATE replay | −13,189 B on `flate.pdf` (one composed sample) | `phase6-0d0bb79` | [phase6](../reviews/phase6-skeptic-review.md) |
| 6 | replay negative controls (two-run court) | controls hold; enabling condition bounded | `phase6-ec92c1a` | [phase6](../reviews/phase6-skeptic-review.md) |
| 7.0 | producer-stratified Flate census | 24/24 replay acceptance after the stream-boundary fix | `phase7-corpus-b-c4eb77e` (supersedes `phase7-corpus-f1f8d26`) | [phase7](../reviews/phase7-skeptic-review.md) |
| 7.0 | complete-cost court over the producer corpus | win 3 / lose 8 / decline 12 — all 3 wins self-authored | `phase7-court-99dc72e` | [phase7](../reviews/phase7-skeptic-review.md) |
| 7.0b | authoring-generator corpus | 87/87 acceptance; win 1 / lose 3; the Cairo "win" is a repeated-bytes artifact | `phase7-producers-e071250` | [phase7b](../reviews/phase7b-skeptic-review.md) |
| 7.0c | generic-compressor ladder | VOLE beats gzip/zstd/xz/brotli on 0/27 | `phase7-baselines-7b9f662` | [phase7b](../reviews/phase7b-skeptic-review.md) |
| 7.1 | coverage-guided fuzzing | 9/10 targets zero-crash; two upstream `preflate` findings | `phase7-fuzz-ca6a92b` | — |
| 7.3 | partial materialization (`view`) | 18/18 exact; decode-CPU win, no I/O win | `phase7-partial-a5764c9` | [phase7c](../reviews/phase7c-skeptic-review.md) |
| 8.3 | seek-based partial I/O | 18/18 exact; constant ~0.44 MB vs non-seekable only | `phase8-seek-08de2a9` | [phase8](../reviews/phase8-skeptic-review.md) |
| 9.3 | cross-document object store | `U = 3,369,900 B` loses to LZ and to CDC | `phase9-store-fdb2845` | [phase9](../reviews/phase9-skeptic-review.md) |
| 10.1 | encoder-only search governor | adds zero bytes (`fixed == exhaustive`) | `phase10-governor-d2b09c9` | — |
| 11 | field I/O, partial, descriptor-free | warm overhead + wall win; warm byte win withdrawn | `phase11-63f43fb`, `phase11-io-2e0a840`, `phase11-partial-b7de39d`, `phase11-desc-free-a8ad6f4` | [phase-11](../reviews/phase-11-skeptic-review.md) |
| 11 | EntropyFS seed store | verified optional adapter; `list`/`remove` decline | `phase11-entropyfs-0f809cc` | [phase-11](../reviews/phase-11-skeptic-review.md) |
| 11.12 | fair-baseline lifetime court | mixed; A1 wall crossover absent on 3/5 | `phase11-lifetime-5a7edd3` | [phase-11](../reviews/phase-11-skeptic-review.md) |
| 11.13 | LLM working set | 2 win / 2 tie / 2 loss vs page-local (pinned tokenizer) | `phase11-llm-tokens-824faa9` | [phase-11](../reviews/phase-11-skeptic-review.md) |
| 11.14 | fine-unit sharing | loses to CDC and to `tar` + `xz -9e` | `phase11-share-d9f818a` | [phase-11](../reviews/phase-11-skeptic-review.md) |
| 11.12 | immutable edit witness | shares by content id; cost recorded as a loss | `phase11-edit-8cceaac` | [phase-11](../reviews/phase-11-skeptic-review.md) |
| 11.13 | skeptic review | corrections F1–F8 | `phase11-skeptic-9bd766d` | [phase-11](../reviews/phase-11-skeptic-review.md) |
| 12.8 | cross-document state reuse | warm 0.339907 → 0.0 after `cache --clear` (`N3` negative) | `phase12-share-e7ef693`, `phase12-share-controls-dce2705` | [phase-12](../reviews/phase-12-skeptic-review.md) |
| 12.9 | cross-format equivalence | 96/96 (self-authored triplet) | `phase12-triplet-dc4d5a3` | [phase-12](../reviews/phase-12-skeptic-review.md) |
| 12.10 | source + descriptor removal | 38/38 byte-exact, fresh process | `phase12-removal-dc4d5a3` | [phase-12](../reviews/phase-12-skeptic-review.md) |
| 12.11 | mixed-format lifetime workload | small-doc win; A1 wins the large frontier | `phase12-lifetime-3eaf576` | [phase-12](../reviews/phase-12-skeptic-review.md) |
| 12.11b | ablation ladder | content adapters + persistent reuse; A9 a loss; A7/A8 not separable | `phase12-lifetime-ablations-06db12a` | [phase-12](../reviews/phase-12-skeptic-review.md) |
| 12.11 | FTS5 amendment | trigram ties `LIKE` but reads more | `phase12-fts5-amendment-22302f9` | [phase-12](../reviews/phase-12-skeptic-review.md) |
| 12.12 | mixed-format LLM working set | 1/8/3 vs page-local; 9/0/3 vs whole document | `phase12-llm-3eaf576` | [phase-12](../reviews/phase-12-skeptic-review.md) |
| 12.13 | security + hostile input | 315/315 over 45 fixtures; 8 fuzz targets `exit=0` | `phase12-security-33f6d04` | [phase-12](../reviews/phase-12-skeptic-review.md) |
| 12.14 | flagship demo | live output, exit 0 | `phase12-demo-fb8a592` | [phase-12](../reviews/phase-12-skeptic-review.md) |
| N6 | PDF no-regression | A2 vs A11: 32/32 exact, 0 regressions | `phase12-pdf-noregression-0d23a02` | [phase-12](../reviews/phase-12-skeptic-review.md) |
| 12.15 | skeptic review | findings F1–F16, close-out amendments | `phase12-skeptic-15b5729` | [phase-12](../reviews/phase-12-skeptic-review.md) |
| 13.1 | PDF `/Length`/revision as a size mechanism | byte-exact; 0 wins vs the ladder and vs generic | `phase13-pdf-length-revision-12fc84e` | — |
| 13.2 | PDF COS grammar/templates | byte-exact; scoped VOLE-ladder win, 0 wins vs generic | `phase13-pdf-grammar-dfba2a4` | — |
| 13.3 | ODT adapter | byte-exact + queryable after source + descriptor deletion, fresh process | `phase13-odt-95c486d` | — |
| 13.4 | byte-level partial-materialization checkpoints | exact + advisory, but redundant with the index (+bytes, identical op work) | `phase13-checkpoints-9d1306a` | — |
| N5 | package-index-only gate | package index answers 0/72 structural selectors, field 72/72 — falsified | `phase13-n5-21948bd` | [phase-12](../reviews/phase-12-skeptic-review.md) |
| real100 | frontier court over the frozen 100-doc NASA/NIST corpus | mixed: VOLE wins repeated observations + DOCX tables/metadata; loses cold lookups, >100 MiB PDFs. A real EPUB-content loss (XHTML `DOCTYPE`) was found and fixed (13.7) | `real100-frontier-8f10d00` | — |
| 13.7 | benign `DOCTYPE` accepted in the bounded-XML policy (real EPUB fix) | EPUB content declines 62 → 6; ADR-0040 | `real100-frontier-c14e06f` | [phase-13](../reviews/phase-13-skeptic-review.md) |
| 15.1 | release-mode baseline of the frozen `real100-v1` court, storage universes split | repaired: coverage/exactness identical to debug (deterministic); VOLE holds `pdf`/`text_repeat` + `docx`/`table`; 3/5 `>100 MiB` PDFs fail at `encode` | `real100-release-baseline-866f489` | — |
| 15.2 | resident `DocumentFieldSession` + `observe-batch` vs the cold per-observation lane | NEGATIVE/partial: wins only < ~1 MiB; cold 7 ms vs resident 9 ms aggregate; mechanism = lost `narrow_probe` short-circuit | `real100-release-resident-78f7ea8` | — |
| 15.3 | packed seed store (`fieldpack`) vs one-file-per-node | WIN (seed namespace only): file/directory count 0.009× (111× fewer) at **byte parity** (file bytes 1.007×), latency parity; identity + exactness unchanged; the "persistent bytes 0.719×" was a `du -sb` directory-inode artifact; 16.2/16.6 correct it | `phase15-packed-8c195e8` | — |
| 15.4 | bounded parallel ingest (`--workers {1..16}`) + determinism | speed-neutral (1.00× at 2/4/8; 0.99× at 16); determinism positive (field id + exact 10/10 across every count) | `phase15-workers-122c026` | — |
| 15.5 | DEFLATE backend ablation over real `real100-v1` members | `zlib-rs` meets the bar (1.58×, RSS-neutral, byte-identical; recommended, not adopted at 15.5 — **adopted as the shipped backend in 16.1**); `miniz-simd` enabled (1.11×); `zune-inflate` disqualified (255 mismatches) | `phase15-deflate-e676166` | — |
| 15.6 | adaptive procedural promotion (diversity frontier) | NEGATIVE: falsifier F1 — `v_on` never beats `sq_adapt` by >10% at any depth; F2 — promoted bytes cut durable bytes 0.0% | `phase15-diversity-4786f8e` | — |
| 15.6 | adaptive procedural promotion (revision retention) | NEGATIVE: falsifier F3 — best-lane retained cross-revision work +0.3% (< 20%) | `phase15-revision-4786f8e` | — |
| 15.7 | durable cross-root derivations (`N3` re-run, real families) | NEGATIVE: `N3` VIOLATED — cross-member derived reuse 0 nodes; only representation identity shared; no Rust change made | `phase15-crossroot-7429d61` | — |
| 16.1 | adopt `zlib-rs` inflate, end-to-end ingest impact | ingest total `0.917×` (`encode` control `0.996×` = labelled noise floor); field id identical, `materialize --exact` PASS | `phase16-zlib-ebb6636` | — |
| 16.2 | full `real100-v1` packed-store storage court (fs vs packed vs SQLite) | storage headline refuted by 16.6: `du -sb` inflated the fs store; corrected fs/SQLite **0.906×**, packed/SQLite **0.914×** — both VOLE backends at/below SQLite on file bytes | `phase16-packed-full-0b21928` | — |
| 16.3 | the `>100 MiB` PDF memory pathology (`propose_rle` allocation) | FIXED: `nasa-pdf-0001` (409 MB) completes byte-exactly; peak/input `17.7× → 8.9×`; 77/77 `.voldoc` SHA-256 unchanged | `phase16-largepdf-ce8af8f` | — |
| 16.4 | resident session + `narrow_probe` short-circuit | NEGATIVE: the probe removes per-observation cost but residency still wins only < ~1 MiB; residual = one-time session open (a full descriptor parse) | `phase16-resident-probe-5d331f2` | — |
| 16.5 | contract-equivalent heterogeneous-session court (C0..C5) | SQLite does NOT lose under the equal contract: VOLE is the storage winner (`0.47×` bytes) but SQLite wins build (`9.54×`), warm latency (`1.47×`), ties cold, and is the only lane answering C4/C5 | `phase16-contract-45d2c0e` | — |
| 16.6 | storage accounting correction (`du -sb` vs file-bytes-only) | CORRECTION: `du -sb` counts 4096 B/dir — fs store inflated ~52 %, packed 0.8 %, `.db` 0 %; the 15.3/16.2 byte headlines correct to parity (ADR-0049) | `phase16-storage-correction-2978e1d` | — |
| 17.1 | direct source → field build (`field-build --profile runtime` = fixed `RAW`, no search) | ADOPTED (ADR-0051): build wall sum 7,928 → 3,878 ms (**2.04×**), peak RSS median 51,792 → 15,228 KB (**3.4× smaller**), authority 1.007×, exactness **9/9**, observations **0 divergent**; caveat: PDF metadata `object_count`/`graph_ops` are encoder-dependent | `phase17-direct-field-d83ddba` | — |
| 17.2 | contract-equivalent court re-run with the PDF revision-lineage surface | SURFACE ADDED (ADR-0052): **C0–C3 satisfied**, **C4/C5 still do not close** — the contract's C4 tuple is the corpus family/member/head, external metadata PDF bytes cannot derive; storage 0.47×, build 10.74×; typed decline for non-PDF | `phase17-revision-1179386` | — |
| 18.1 | contract-equivalent court with the **direct** `field-build` build step (C4a/C4b split) | RECORDED: build **10.74× → 7.24×** (VOLE 11,905 vs SQLite 1,644 ms; the 17.1 2.04× and the 10.74× two-step gap are **not composable**), storage **0.49×**, cold sum **779 vs 772 ms (tie)**, warm **1.33×**. **C4a** document-native lineage is VOLE's answer (4/4 PDFs; typed unsupported 8/8 docx/epub) and byte-derivable from the baseline's retained blob — a work-location difference; **C4b** corpus/external lineage is harness-supplied metadata (baseline 12/12, VOLE 0/12) | `phase18-contract-direct-f2a34a5` | — |
| 18.2 | one-pass direct build (`FieldStore::ingest_verified`, `ingest_pdf_direct`/`ingest_package_direct`) | ADOPTED: wall **−6.1%** (sum 4012 → 3769 ms), peak RSS median **−28.8%** (15,184 → 10,812 KB); the contract gap held at **7.21×**; `encode`/`field-ingest` byte-for-byte unchanged; exactness 9/9 + 12/12, observations 0 divergent | `phase18-onepass-61b11350` | — |
| 18.3 | observation index attached before the single serialize (`Descriptor::with_observation_index`, `encode_with_observation_index`) | IMPLEMENTED: wall-neutral (control moved +3.8%), large-doc RSS **−14–16%**; `encode` still emits no index and the one-pass authority is byte-identical to `with_observation_index(encode_with(…))` (witness test); isolation: `nist-pdf-0017` is **9425/11,605 ms (81%)**, **6842 seed files**, **9.40 s** bind mount vs **1.39 s** `/tmp` | `phase18-inindex-b7046f0` | — |
| 18.4 | contract court with the packed seed substrate (`field-build --packed`) | PACKED is a storage-shape win only: build 7.08× (fs-direct 7.34×), because `insert` still `sync_data()`s per node; `observe-batch` rejects `--packed` (rc 6) | `phase18-contract-packed-fb23021` | — |
| 18.5 | batch packed durability syncs (`--sync=batch`, default) + `observe-batch --packed` | ADOPTED (ADR-0053): packed worst-doc wall 9392 → 1415 ms, `fdatasync` 6792 → 0; contract build ratio **7.08× → 0.82×** (VOLE now the build winner); storage unchanged 0.53×; warm `observe-batch --packed` now 1.09× SQLite; exactness 12/12 | `phase18-batched-sync-14a7e6f` | — |
| 18.5 | re-run contract court with batched packed durability (12 docs, best-of-3 min) | RECORDED: VOLE `field-build --packed` build sum **1552 ms vs SQLite 1893 ms = 0.82×** (fs-direct 6.27×); storage **0.53×**; cold sum 895 vs 933 ms; warm `--packed` **1.09×** SQLite over C0–C5; `nist-pdf-0017` 9187 → 1382 ms; exactness `materialize --exact --packed` **12/12** (fixture prose is stale; raw tables authoritative) | `phase18-contract-packed-14a7e6f` | — |
| 19.1 | paired, interleaved repeatability court (N=10) of the Phase-18.5 build/warm headline | BUILD **established < 1.0**: paired median **0.182** (95% CI 0.102–0.228), geometric mean 0.203 (0.128–0.395), 11 win / 0 tie / 1 loss. **Correction:** ratio-of-sums reads **~0.96** (not 0.82) — one heavy doc dominates VOLE's total; move is SQLite-lane host variance (1893 → 1582 ms), VOLE sum reproducible (1545.5 vs 1552 ms). WARM **not resolved**: paired median 1.077 (0.809–1.391, includes 1.0), 5/1/6. Best-of-3 (min) vs median-of-10: build 0.977 → 0.963, warm 1.218 → 1.194. Exactness 12/12 both. ADR-0054 | `phase19-repeat-d6c8c4c` | — |
| 19.2 | high-N warm-only repeat (N=100) | **Not resolved under the median estimator, resolved under the geometric mean**: pooled median paired ratio **1.292** (95% CI 0.994–1.658, includes 1.0), geometric mean **1.283** (95% CI 1.069–1.535); VOLE warm median 2.25–2.32 ms vs SQLite 1.54–1.65 ms (~0.7 ms slower). A **modest real loss (~1.29×), not parity**; N=10 was under-sampled. CV 5.4% (p90 9.4%); median-CI half-width ±0.332; MDE(80%) ≈0.474. Exactness 12/12; 480 envelopes, 0 mismatches. ADR-0054 | `phase19-warm-6b66eab` | — |
| 19.3 | NEW direct build (`field-build --profile runtime --packed --sync=batch`) over the full frozen `real100-v1` | **100/100 built, 100/100 exact** (pdf 60/60, docx 15/15, epub 25/25; rc histogram all 0, no 124/137), recovering the 3 docs the old two-step path failed (`nasa-pdf-0001`/`0002`/`0003`); wall median 64 ms, sum 221,408 ms; peak RSS median 30.9 MiB, max 2342 MiB — **memory is the next binding constraint**; persistent 2,829,898,049 B = 1.036× source. Cold coverage text 92/100, metadata 98/100 (typed declines). vs old path (file-size-corrected): 96 common docs median 61 ms vs 2,095 ms (paired 0.083×), **+5.3%** bytes, 15/96 byte-identical. **Storage population-dependent: ~0.96× SQLite on the full population, not 0.53× of the contract subset** | `phase19-real100-direct-954dbc2` | — |

Receipt short names are the suffixes of `evidence/campaigns/2026-10-*<name>/`.

## Measurement reports

Human-readable reports kept alongside the machine receipts in this directory:

- [phase7-corpus-report.md](phase7-corpus-report.md) — Phase-7.0 producer-stratified Flate ratio.
- [phase7-partial-report.md](phase7-partial-report.md) — Phase-7.3 partial-materialization court.
- [phase8-seek-report.md](phase8-seek-report.md) — Phase-8.3 seek-based bytes-read court.
- [phase9-store-report.md](phase9-store-report.md) — Phase-9.3 cross-document store court.
- [real100-frontier-report.md](real100-frontier-report.md) — the `real100-v1` frontier court over the frozen real NASA/NIST corpus.

## Reproducing

Every campaign is driven by a script under `tools/` and run inside a pinned
Docker service; see [Conformance](../reference/conformance.md) for the gate
suite and [the README](../../README.md) for the container commands.
