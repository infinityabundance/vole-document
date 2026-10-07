# Roadmap

Open work that is `PROPOSED` or not yet measured. Every item carries an explicit
prior that it may also lose, because every measured attempt so far has.

## Phase-13 open proposals

Phase 13 closes the remaining Phase-12 proposals and the last open gate. Plan:
[phase-13-plan.md](../phases/phase-13-plan.md).

| Item | Question | Prior |
|---|---|---|
| PDF `/Length`/revision proceduralization as a *size* mechanism | Does persisting revision/xref structural redundancy beat the RAW/`BYTE_RANS`/generic ladders once framing is charged? (Phase 11 persists revisions as *observation* nodes only, never as a size candidate.) | Expected negative |
| PDF grammar/templates | Can a bounded grammar/template candidate that pays its definition cost beat a whole-file order-0 lane? | Expected negative |
| ODT adapter | ODT is an OPC/ZIP package, so it reuses the 12.x ZIP + OPC layers with an OpenDocument content model. Does a fourth format enter exactly and queryably? | Unknown |
| Byte-level partial-materialization checkpoints | The random-access `view`/seek lane was measured (ADR-0018/0019), but literal byte-level checkpoint records were never built. Do they pay their framing cost? | Unknown |
| `N5` package-index-only gate | Is the small-document win reproducible by `unzip -p` + `substr` at the same boundary? The A3/A4 ladder rungs are evidence against it; the mechanical check remains open. | Evidence suggests the field adds value |

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
(ADR-0034, `N3`); encoder-only parametric search as a byte win (ADR-0022). See
[Findings](findings.md).
