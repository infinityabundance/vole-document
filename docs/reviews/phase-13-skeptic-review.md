# Phase-13 skeptic review

Adversarial review of Phase 13 (subphases 13.1–13.5) and the `real100-v1`
frontier court. It tries to falsify each headline against the sealed raw
receipts, not to confirm it.

## Independence caveat (recorded, not papered over)

The project requires an independent skeptic whose reasoning path differs from the
implementer's. For this phase the intended independent skeptic subagent was
**canceled by the operator before it ran**, so this review is an **in-session
adversarial pass by the same reasoning path that produced the work**. It is
therefore *not* an independent review, and the phase's
independent-adversarial-review gate is only **partially** satisfied. Findings
below are still checked against `evidence/campaigns/2026-10-07-phase13-*/raw/`,
but the reviewer is not independent of the author. Treat F1–F4 as author
self-checks, not third-party findings.

## Findings

| id | claim | verdict | counter-evidence / note |
|---|---|---|---|
| F1 | 13.1/13.2 "totals" (`new_total` vs `ladder_total_excl`/`generic_total`) | **overstated (presentational)** | the numerator sums only the *proposed* files (21 in 13.1, 7 in 13.2) while the denominators sum all *28* (including the 33.6 MB decline). In 13.1 this reads as ~19× smaller; it is not. The per-file verdicts (all losses / ties) are correct. **Do not quote the totals as a ratio.** |
| F2 | 13.5 "field 92/96; structural 72/72" | **confirmed** | the 4 non-passes are the pre-existing DOCX `resource:N --kind decoded` decline (`V:declined` in 12.11, `a2..a11:declined` in 12.11b), already disclosed; the `N5`-relevant number is the structural 72/72, and the package lane resolves 0/72. |
| F3 | `real100-v1` "VOLE wins repeated observations and DOCX tables/metadata" | **confirmed with caveats** | per-cell verdicts are computed over the docs where a lane answered (cells mix answered/declined) and every op spawns a process, so tiny walls are startup-dominated; the ratio, not the millisecond, is the signal. Both caveats are stated in `docs/evidence/real100-frontier-report.md`. |
| F4 | 13.3 ODT evidence id | **stale reference (fixed)** | `docs/project/status.md` cited `…-odt-9d1306a`; the real campaign is `…-odt-95c486d`. Corrected on the branch. |
| F5 | Phase 13 independent-review gate | **partial / not satisfied** | see the independence caveat above. |

## Claims re-verified against raw (survived)

- **13.1** — 28 files, 7 declines, 21 proposed, `exact_ok 21/21`; win 0 / tie 0 /
  loss 21 vs the ladder and vs generic (`raw/exactness.json`, `raw/results.json`).
- **13.2** — 7 proposed, `exact_ok 7/7`; win 0 / tie 4 / loss 3 vs the ladder and
  win 0 / loss 7 vs generic; `cairo-vector.pdf` 34,633 → 19,411 B (4-file
  auto-winner gain 34,505 B) but loses to brotli 16,670 B (`raw/results.json`).
- **13.4** — the raw court table shows constant `delta` 259 / 439 B at 20/40
  objects (and 1,159 B at 120) with `ops_eval` identical between the checkpoint
  and index lanes (`raw/court.txt`).
- **13.5** — `raw/assertions.tsv` (field 72/72 structural) and
  `raw/n5-control.json` (package lane 0/72 structural) agree with the headline.
- **real100 court** — `raw/ops.tsv` (700 lane-ops), `raw/exact.tsv` (VOLE 97/100,
  A1 98/100, A0 100/100), `raw/onetime.tsv` (VOLE one-time footprint ≥ A1 for
  PDFs) agree with the report.

## Corrections applied

- F1: recorded here and in `docs/phases/phase-13-results.md` — the 13.1/13.2
  aggregate totals are **not comparable across file sets** and must not be quoted
  as a size ratio. The campaigns are **not** rewritten (receipts are immutable).
- F4: the stale ODT campaign id in `docs/project/status.md` is corrected.

## Open items carried forward

- `N4` (decline-rate threshold) remains **not evaluated** (no pre-registered
  threshold), unchanged from Phase 12.
- The two `real100` failure regions are the concrete backlog the map produced:
  the EPUB XHTML `DOCTYPE` policy and the >100 MiB encode bound (cap/OOM).
- A genuinely independent review of Phase 13 is still wanted before the phase's
  independence gate can be called closed.
