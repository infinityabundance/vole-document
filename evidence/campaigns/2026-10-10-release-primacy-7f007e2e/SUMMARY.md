# Release primacy — exactness + cross-field identity courts on the release commit

**Commit under test:** `7f007e2e11d1240f638fd10d8ebd3df2751dea33`
(short `7f007e2e`) = branch `main`, tag `v0.1.0-alpha.33`.
**Binary:** `target/debug/vole-document` sha256
`dcfad49a927c2f96c47515c5ec00fdde7b116fc563373483fee754af4af92cd1`
(built `--locked --all-features`; every court below ran against this same hash).

**Overall verdict: `PASS`** — every exactness court passed, and the ADR-0060
cross-field identity court passed 12/12, on the released code (not a
feature-branch commit).

The primacy invariant asserted by every exactness court here is:

```text
materialize(descriptor) == original_bytes      # exactness n/n
materialize(field_root) == original_bytes
```

after **source file AND standalone descriptor deletion**, in a **fresh process**,
checked as `materialized_length == source_length` **and**
`SHA256(materialized) == SHA256(source)` **and** `byte_compare == equal`.

## Courts

| court | service | verdict | exactness |
|---|---|---:|---|
| ADR-0060 permanent cross-field identity court | `doc-baseline` | PASS | 12/12 checks |
| Phase 12.10 source-removal / restart court (PDF, DOCX, EPUB) | `doc-baseline` | PASS | 38/38 assertions |
| Phase 21.1.1 XLSX (SpreadsheetML) | `doc-baseline` | PASS | 2/2 fixtures |
| Phase 21.3.1 ODS (OpenDocument spreadsheet) | `doc-baseline` | PASS | 8/8 fixtures |
| Phase 21.4.1 ODP (OpenDocument presentation) | `doc-baseline` | PASS | 6/6 fixtures |
| Phase 21.5.1 JSON | `doc-baseline` | PASS | 8/8 fixtures |
| Phase 21.6.1 YAML | `doc-baseline` | PASS | 10/10 fixtures |
| Phase 21.7.1 CSV / TSV | `doc-baseline` | PASS | 10/10 fixtures |
| Phase 21.5.3 stratified real-world multi-format smoke | `doc-baseline` | PASS | 12/12 samples (0 skipped) |

Per-fixture byte-exactness (length + SHA-256 + `cmp` after removal):
adapters total **44/44**; the removal court adds **3/3** formats
(PDF/DOCX/EPUB) across its 38 assertions; the smoke court adds **12/12**
independently sourced real documents.

## Identity court cases (ADR-0060)

Invariant: *an observation's output is a pure function of the field's content id
(NodeId), independent of the store it lives in and of what else the store
contains.*

| case | what it asserts |
|---|---|
| interleave | PDF + DOCX/XLSX/PPTX/ODS/ODP + JSON + YAML built & observed interleaved in ONE store; every observation equals the same document built alone, and field ids do not depend on store contents |
| programs | identical source via direct `field-build --profile runtime` vs searching `encode`/`field-ingest` sharing a store: identical observations, no aliasing of a second document |
| edit | `field-edit` one PDF page: the original field is unchanged and NO other field is affected |
| cache-clear | `cache --clear` then re-observe: the same answers |
| crash | SIGKILL a build mid-way, reopen the store: no wrong-document bytes served, post-crash rebuild exact |

Checks (12/12 PASS): `cache_clear_materialize_exact`,
`cache_clear_same_answers`, `crash_recovery_build_exact`,
`crash_store_reopens_no_wrong_bytes`, `edit_new_field_shows_marker`,
`edit_no_other_field_affected`, `edit_original_field_unchanged`,
`interleave_field_ids_independent`, `interleave_observations_equal`,
`programs_identical_observations`, `programs_materialize_exact_source`,
`programs_no_aliasing_other_document`.

## Environment

| item | value |
|---|---|
| measured commit | `7f007e2e11d1240f638fd10d8ebd3df2751dea33` |
| branch / tag | `main` / `v0.1.0-alpha.33` |
| base image (dev / doc-baseline chain) | `rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e` |
| built image (doc-baseline) | `vole-document/doc-baseline:1.99.0` = `sha256:12de62c9ef3e4a292021358dec5755c68e2477a443de8cc1dc7cc7b6bf22f981` |
| built image (dev) | `vole-document/dev:1.99.0` = `sha256:79f3ea2ad94b8f0b4e58692cbaf3e53eb2ef484add34a7b6e96004bb2e48b2c0` |
| rustc | `rustc 1.99.0 (b940084d7 2026-09-28)` |
| cargo | `cargo 1.99.0 (5f94df478 2026-08-27)` |
| python (courts) | `Python 3.11.2` |
| arch | `x86_64` |
| `Cargo.lock` sha256 | `7a36c1cbcddffca16d2bf86ed46e36b18d054e987da81fe13e007968ea423927` |
| binary sha256 | `dcfad49a927c2f96c47515c5ec00fdde7b116fc563373483fee754af4af92cd1` |
| env affecting semantics | `LC_ALL=C` |

All work ran in the pinned, hard-capped Docker services (`dev`, `doc-baseline`);
nothing ran on the host except `git` and `docker`.

## Finding (honest, non-blocking for exactness)

A **release-commit performance defect** was observed while running the CSV court;
it is **not** an exactness failure and does not falsify the primacy invariant, but
it is recorded so the release is not overclaimed:

- `src/field/observe.rs`, `csv_common_metadata` (lines ~10220–10230) computes the
  modal column count with, for every record, a full re-scan of all records
  (`.filter(...).count()` inside a `for r in &model.records` loop) — **O(records²)**.
- The 50 MiB `large.csv` fixture has **1,471,685** records, so the single
  `observe --metadata` call is ~2.2×10¹² field-length comparisons.
- Measured scaling (release binary, `doc-baseline`, same code path):
  `50k → 1.047 s`, `100k → 4.060 s`, `200k → 16.028 s` — a clean **~4× per
  doubling** (quadratic). Extrapolated to 1.47M records this one observation
  costs ~15 min; the court therefore only completes after a long single-thread
  burn. A first 15-minute-bounded run did not terminate; a re-run with a larger
  bound completed and passed 10/10.
- Recommendation (not applied here — it would change the measured binary):
  replace the per-record full scan with a single pass tallying a field-count
  histogram, reducing the cost to O(records).

Because the release binary is what the courts must measure, no source change was
made; the receipt records the defect rather than patching around it.

## Scope (honest)

- The adapter courts are **fixed, self-authored** fixture sets (one or more files
  per format); the smoke court is a small pinned real-world sample (12 documents).
  These are regression courts, not coverage claims about real-world populations.
- The removal court's byte-exactness covers PDF, DOCX and EPUB; the adapter courts
  extend it to XLSX/ODS/ODP/JSON/YAML/CSV.
- The tree was left dirty deliberately (new `evidence/campaigns/*`), as requested;
  nothing was committed or pushed.
