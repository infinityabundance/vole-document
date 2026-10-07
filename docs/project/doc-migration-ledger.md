# Documentation migration ledger

The documentation-architecture refactor (Phase 13, branch `phase13`) moved,
split, and editorialized the root documents without discarding information.
This ledger maps **every substantive section of the pre-refactor root docs** to
its destination, so nothing disappears silently.

Columns: *existing content* → *destination file* → *destination heading* →
*action*. Actions:

- **preserved verbatim** — text carried over unchanged (pure move).
- **rewritten w/o loss** — re-expressed for the new home; every fact retained.
- **merged** — folded into another section that now owns the claim once.
- **superseded + linked** — replaced by a canonical statement elsewhere and
  linked, with the historical text kept where it is a receipt/history record.

Sources of truth for the present-tense content are
[`phases/phase-13-plan.md`](../phases/phase-13-plan.md),
[`project/status.md`](status.md), [`project/findings.md`](findings.md),
[`reviews/phase-11-skeptic-review.md`](../reviews/phase-11-skeptic-review.md),
[`reviews/phase-12-skeptic-review.md`](../reviews/phase-12-skeptic-review.md), and
ADRs 0029–0035.

## Root file moves (pure rename)

| Existing content | Destination file | Destination heading | Action |
|---|---|---|---|
| `CHANGELOG.md` (whole file) | [project/changelog.md](changelog.md) | all | preserved verbatim |
| `CONFORMANCE.md` (whole file) | [reference/conformance.md](../reference/conformance.md) | all | preserved verbatim |
| `FINDINGS.md` (whole file) | [project/findings.md](findings.md) | all (then restructured in commit 3) | preserved verbatim, then rewritten w/o loss |
| `PROJECT_STATE.md` (whole file) | [project/status.md](status.md) | all | preserved verbatim |
| `SPEC.md` (whole file) | [reference/specification.md](../reference/specification.md) | all | preserved verbatim |
| `SECURITY.md` (whole file) | [SECURITY.md](../SECURITY.md) | all | preserved verbatim |
| `docs/evidence/phase{6,7,7b,7c,8,9}-skeptic-review.md` | [reviews/](../reviews/) | all | preserved verbatim (pure move) |
| `docs/security/README.md` | docs/security/README.md | all | preserved verbatim (retained; format/phase security notes) |

## Old `README.md` (1053 lines → ≤ 300)

| Existing content | Destination file | Destination heading | Action |
|---|---|---|---|
| L1–27 title, one-line description, governing invariant, "compression is not the product" note | [../README.md](../../README.md) | intro + Exactness invariant | rewritten w/o loss |
| L22–26 pointers to `SPEC.md` / `PROJECT_STATE.md` / `docs/` | [docs/README.md](../README.md) | (index) | superseded + linked |
| L28–53 "📌 Findings" callout (Phase 12 headline) | [project/status.md](status.md), [project/findings.md](findings.md) | Current conclusions | merged (phase chronology removed from README) |
| L55–158 Status narrative (Phases 9–13 prose) | [project/status.md](status.md), [../reviews/](../reviews/) | Status vocabulary | rewritten w/o loss |
| L161–231 "Area / State / Evidence" capability table (70+ rows) | [reference/format-support.md](../reference/format-support.md), [project/status.md](status.md) | capability matrix / Ledger | merged (row-level ledger preserved in status.md) |
| L239–243 Phase-13 in-progress note | [project/status.md](status.md), [project/roadmap.md](roadmap.md) | Phase 13 | rewritten w/o loss |
| L245–265 Phase 2 measured results | [architecture/inverse-proceduralization.md](../architecture/inverse-proceduralization.md) | Entropy floor | rewritten w/o loss |
| L276–310 Phase 3 measured results | [formats/pdf.md](../formats/pdf.md), [architecture/inverse-proceduralization.md](../architecture/inverse-proceduralization.md) | Physical authority | rewritten w/o loss |
| L312–344 Phase 4 measured results | [architecture/inverse-proceduralization.md](../architecture/inverse-proceduralization.md) | Typed channels (rejected) | rewritten w/o loss |
| L346–382 Phase 5 measured results | [architecture/inverse-proceduralization.md](../architecture/inverse-proceduralization.md) | Layout prediction (rejected) | rewritten w/o loss |
| L384–424 Phase 5.7 packed framing | [architecture/inverse-proceduralization.md](../architecture/inverse-proceduralization.md) | Packed framing | rewritten w/o loss |
| L426–461 Phase 5.8 layout + rANS | [architecture/inverse-proceduralization.md](../architecture/inverse-proceduralization.md) | Layout + rANS (rejected) | rewritten w/o loss |
| L463–517 Phase 6 DEFLATE replay | [architecture/inverse-proceduralization.md](../architecture/inverse-proceduralization.md) | Exact DEFLATE replay | rewritten w/o loss |
| L519–602 Phase 7.0 producer corpus | [formats/pdf.md](../formats/pdf.md), [project/findings.md](findings.md) | Flate ratio / Corrected claims | rewritten w/o loss |
| L604–658 Phase 7.0b generator corpus (Cairo correction) | [project/findings.md](findings.md) | Corrected claims | rewritten w/o loss |
| L660–689 Phase 7.0c generic-compressor ladder | [project/findings.md](findings.md) | Negative results | rewritten w/o loss |
| L691–719 Phase 7.3 partial materialization | [architecture/observations-and-provenance.md](../architecture/observations-and-provenance.md) | Partial materialization | rewritten w/o loss |
| L721–773 Phase 8.3 seek I/O | [architecture/observations-and-provenance.md](../architecture/observations-and-provenance.md), [architecture/persistence-and-caching.md](../architecture/persistence-and-caching.md) | Seek reader | rewritten w/o loss |
| L775–813 Phase 10.1 governor | [project/findings.md](findings.md), [architecture/persistence-and-caching.md](../architecture/persistence-and-caching.md) | Negative results | rewritten w/o loss |
| L815–870 Phase 11 field results | [architecture/document-field.md](../architecture/document-field.md), [project/findings.md](findings.md) | The field / Current conclusions | rewritten w/o loss |
| L872–907 Phase 12 field results | [architecture/multi-format-adapters.md](../architecture/multi-format-adapters.md), [project/status.md](status.md) | Multi-format field | rewritten w/o loss |
| L909–994 "Quick start (Docker only)" + CLI surface block | [../README.md](../../README.md), [reference/cli.md](../reference/cli.md) | Quick start / CLI | rewritten w/o loss |
| L996–1024 "Fuzzing" | [reference/conformance.md](../reference/conformance.md) | Fuzz targets | merged |
| L1026–1040 "Repository layout" | [../README.md](../../README.md), [docs/README.md](../README.md) | Repository layout | rewritten w/o loss |
| L1042–1053 "Licensing" + LGPL note | [../README.md](../../README.md) | License | preserved (short) |

## Old `PROJECT_STATE.md` → `project/status.md`

| Existing content | Destination file | Destination heading | Action |
|---|---|---|---|
| L1–7 intro + status-vocabulary rule | [project/status.md](status.md) | Status vocabulary | preserved verbatim |
| L8–160 phase narrative (Phases 8–13) | [project/status.md](status.md) | Status vocabulary | preserved verbatim |
| L160–232 mechanism Ledger table (60+ rows) | [project/status.md](status.md) | Ledger | preserved verbatim |
| L234–264 PDF-lane narrative | [architecture/inverse-proceduralization.md](../architecture/inverse-proceduralization.md) | Structural courts | rewritten w/o loss |
| L266–325 Phase 6 scope | [architecture/inverse-proceduralization.md](../architecture/inverse-proceduralization.md) | Exact DEFLATE replay | rewritten w/o loss |
| L326–353 Phase 5.7 scope | [architecture/inverse-proceduralization.md](../architecture/inverse-proceduralization.md) | Packed framing | rewritten w/o loss |
| L354–395 Phase 5.8 scope | [architecture/inverse-proceduralization.md](../architecture/inverse-proceduralization.md) | Layout + rANS | rewritten w/o loss |
| L396–429 Phase 5 scope | [architecture/inverse-proceduralization.md](../architecture/inverse-proceduralization.md) | Layout prediction | rewritten w/o loss |
| L430–454 Phase 4 scope | [architecture/inverse-proceduralization.md](../architecture/inverse-proceduralization.md) | Typed channels | rewritten w/o loss |
| L455–475 Phase 3 scope | [formats/pdf.md](../formats/pdf.md) | Physical authority | rewritten w/o loss |
| L476–490 Phase 2 scope | [architecture/inverse-proceduralization.md](../architecture/inverse-proceduralization.md) | Entropy floor | rewritten w/o loss |
| L491–513 Phase 0 research (frozen facts) | [architecture/overview.md](../architecture/overview.md) | Prior art | rewritten w/o loss |
| L514–519 Freeze policy | [reference/specification.md](../reference/specification.md) | Feature policy | merged (SPEC owns the wire format) |

## Old `FINDINGS.md` → `project/findings.md`

| Existing content | Destination file | Destination heading | Action |
|---|---|---|---|
| §1 What this is | [architecture/overview.md](../architecture/overview.md) | The idea | rewritten w/o loss |
| §2 What was built (phase/tag table) | [project/status.md](status.md) | Ledger | merged |
| §3 What was measured (axes + receipts) | [project/findings.md](findings.md) | Positive results / Negative results | rewritten w/o loss (tables kept) |
| §3.3 internal per-mechanism court | [project/findings.md](findings.md) | Negative results | rewritten w/o loss |
| §4 What won (scoped) | [project/findings.md](findings.md) | Positive results | rewritten w/o loss |
| §5 What lost, and why | [project/findings.md](findings.md) | Negative results | rewritten w/o loss |
| §6 Falsified-claims log | [project/findings.md](findings.md) | Corrected claims | rewritten w/o loss |
| §7 What would change the conclusion | [project/roadmap.md](roadmap.md) | Open hypotheses | rewritten w/o loss |
| §8 Reproducibility | [../README.md](../../README.md), [reference/conformance.md](../reference/conformance.md) | Reproducibility / Gate suite | merged |
| §9 Phase-12 addendum | [project/status.md](status.md), [reviews/phase-12-skeptic-review.md](../reviews/phase-12-skeptic-review.md) | Phase 12 | superseded + linked (receipts/amendments owned by the review) |

## Old `SPEC.md` and `CONFORMANCE.md`

| Existing content | Destination file | Destination heading | Action |
|---|---|---|---|
| `SPEC.md` (all sections) | [reference/specification.md](../reference/specification.md) | all | preserved verbatim |
| `CONFORMANCE.md` (all sections) | [reference/conformance.md](../reference/conformance.md) | all | preserved verbatim |
| `CONFORMANCE.md` CLI/court summary | [reference/cli.md](../reference/cli.md) | CLI | merged (surface extracted) |

## New documents created by this refactor

| New file | Purpose |
|---|---|
| [docs/README.md](../README.md) | navigational index |
| [architecture/overview.md](../architecture/overview.md) | pipeline + invariant, evergreen |
| [architecture/authority-and-exactness.md](../architecture/authority-and-exactness.md) | layered authority model |
| [architecture/inverse-proceduralization.md](../architecture/inverse-proceduralization.md) | DRA + structural courts |
| [architecture/document-field.md](../architecture/document-field.md) | persistent field |
| [architecture/observations-and-provenance.md](../architecture/observations-and-provenance.md) | observation algebra |
| [architecture/persistence-and-caching.md](../architecture/persistence-and-caching.md) | DAG, stores, indexes, cache |
| [architecture/multi-format-adapters.md](../architecture/multi-format-adapters.md) | ZIP + PDF/DOCX/EPUB |
| [formats/pdf.md](../formats/pdf.md), [docx.md](../formats/docx.md), [epub.md](../formats/epub.md) | per-format detail |
| [reference/cli.md](../reference/cli.md) | CLI surface |
| [reference/format-support.md](../reference/format-support.md) | capability matrix |
| [project/roadmap.md](roadmap.md) | open `PROPOSED` items |
| [evidence/README.md](../evidence/README.md) | evidence index |
| [project/doc-migration-ledger.md](doc-migration-ledger.md) | this ledger |

## Verification

After the refactor, every substantive section above is accounted for. The
documentation checker [`tools/check-docs.sh`](../../tools/check-docs.sh) enforces
that all internal Markdown links resolve, that required docs exist, that ADR
references resolve and numbers are unique, that no root-level reader doc other
than `README.md` exists, and that `README.md` is ≤ 300 lines.

### Update log

- Commit 1 (`docs(layout)`): the moves above, the index
  ([docs/README.md](../README.md)), and this ledger. All internal Markdown links
  repointed.
- Commit 3 (`docs(editorial)`): the `docs/architecture/*`, `docs/formats/*`,
  `docs/reference/cli.md`, `docs/reference/format-support.md` and
  `docs/project/roadmap.md` documents created; `docs/project/findings.md`
  restructured into canonical sections. Stale code-span references to the moved
  skeptic reviews in living docs (status, conformance, ADRs 0015/0019/0021, two
  measurement reports) updated to `docs/reviews/`. Recorded history
  (`docs/project/changelog.md`) is left as written; its hyperlinks were already
  repointed in commit 1.
- Commit 4 (`docs(validation)`): [docs/evidence/README.md](../evidence/README.md)
  and [`tools/check-docs.sh`](../../tools/check-docs.sh) added, and the checks made
  to pass.
