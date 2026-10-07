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

Receipt short names are the suffixes of `evidence/campaigns/2026-10-*<name>/`.

## Measurement reports

Human-readable reports kept alongside the machine receipts in this directory:

- [phase7-corpus-report.md](phase7-corpus-report.md) — Phase-7.0 producer-stratified Flate ratio.
- [phase7-partial-report.md](phase7-partial-report.md) — Phase-7.3 partial-materialization court.
- [phase8-seek-report.md](phase8-seek-report.md) — Phase-8.3 seek-based bytes-read court.
- [phase9-store-report.md](phase9-store-report.md) — Phase-9.3 cross-document store court.

## Reproducing

Every campaign is driven by a script under `tools/` and run inside a pinned
Docker service; see [Conformance](../reference/conformance.md) for the gate
suite and [the README](../../README.md) for the container commands.
