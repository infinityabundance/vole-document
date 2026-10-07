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
| 15.1 Court repair | Re-run the frozen `real100-v1` court on the **release** binary with the storage universes split | **Repaired** (measurement): coverage/exactness identical to the debug court (deterministic); VOLE holds `pdf`/`text_repeat` + `docx`/`table`, loses the rest; persistent `2,667,668,262 B` / descriptor `1,704,524,849 B` / A1 db `2,891,784,192 B`; 3 of 5 `>100 MiB` PDFs still fail at `encode` (`2026-10-07-real100-release-baseline-866f489`) |
| 15.2 Resident runtime | Does a resident session + `observe-batch` beat the cold lane? | **Measured — recorded negative/partial** (ADR-0042): wins only below ~1 MiB; cold 7 ms vs resident 9 ms aggregate; mechanism = lost `narrow_probe` short-circuit |
| 15.3 Packed seed store | Does a packed `fieldpack` backend beat one-file-per-node? | **Measured — adopted (seed namespace only)** (ADR-0043): persistent bytes 0.719×, file count 0.009×, latency parity, identity unchanged |
| 15.4 Parallel ingest | Does a bounded worker pool speed up ingest deterministically? | **Measured — implemented, non-default** (ADR-0044): speed-neutral (1.00× at 2/4/8); determinism positive (10/10 across every worker count) |
| 15.5 DEFLATE ablation | Which correct safe-Rust inflate is fastest? | **Measured — recommendation** (ADR-0045): `miniz-simd` enabled (1.11×); `zlib-rs` meets the bar (1.58×, RSS-neutral, byte-identical) and is recommended but **not adopted**; `zune-inflate` disqualified (255 mismatches) |
| 15.6 Adaptive promotion | Does an adaptive promotion governor beat SQLite on diversity/revision? | **Measured — recorded negative** (ADR-0046): all three pre-registered falsifiers fire; mechanism ships opt-in/default-off |
| 15.7 Durable cross-root derivations | Does canonical derived-work identity satisfy `N3`? | **Measured — recorded negative** (ADR-0047): `N3` violated again, 0 cross-member reuse; no Rust change made |
| 15.8 CUDA batch lane | Does a GPU batch inflate lane pay? | **Deferred, not measured** (ADR-0048): gate unopened; the pinned Docker lanes cannot see the GPU; `nvCOMP` proprietary; Docker-only evidence required |

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
