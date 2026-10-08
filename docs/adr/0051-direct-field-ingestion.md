# ADR-0051: A direct source → field build path with a fixed, non-searched exactness program

- **Status:** Accepted — adopted (Phase 17.1)
- **Date:** 2026-10-08

## Context

The runtime path to build a persistent document field was two commands:
`encode SRC OUT.voldoc` (which prices the **whole candidate portfolio** — PDF
DEFLATE replay, `BYTE_RANS`, RLE, RAW, …) followed by
`field-ingest OUT.voldoc --store DIR`. The field is **not** a compression
product, and — the load-bearing fact — its observations are **not** derived
from the winning reconstruction program: both `ingest_pdf_with` (Stage B) and
`ingest_package_with` materialize the exact source from the descriptor and then
**re-scan the source bytes** into exact spans, object/stream/page nodes, package
members, resource blobs and the hierarchical index. The descriptor program is
consulted only for the advisory `OBSERVATION_INDEX` op table (a seek
optimization that falls back to the full descriptor path when it cannot apply)
and for the exactness check.

So the portfolio search is **pure overhead for a runtime ingest**: the same
source is scanned and the same observations are produced no matter which exact
program won the search. Phase 16 recorded the build cost as the field's largest
weakness under an equal-capability contract (VOLE build `~10×` slower than a
source-retaining SQLite baseline, [ADR-0050](0050-sqlite-as-substrate-question.md)),
so removing non-load-bearing work is a direct, measurable improvement.

## Decision

1. **Add a direct build path.** A new module `src/field/build.rs` and the CLI
   command

   ```
   field-build INPUT --store DIR [--profile runtime] [--workers N] [--voldoc OUT] [--entropyfs | --packed]
   ```

   build the exact authority and the field **in one process**. `field-build`
   reuses the existing `encode`/`field-ingest` machinery internally; those
   commands are untouched.

2. **A profile names a fixed function, never a search.** `--profile runtime`
   fixes the program to the literal **`RAW`** floor: one literal object and one
   `EMIT_OBJECT`. Exactly **one** candidate is priced — and it is still
   serialized, decoded and byte-compared by the **same complete-cost court** the
   searched path uses (`candidates_evaluated == 1`). A profile exists so the
   runtime CLI can state which deterministic program it chose; adding one is a
   deliberate act, and every profile must remain exact and must not reduce the
   observation surface.

3. **RAW preserves the observation surface because structure comes from the
   source, not the program.** Re-scanning the materialized source is what
   produces spans, object/stream/page nodes, package members, resources and the
   index, so a single deterministic exact program serves the same surface as the
   searched winner.

4. **The `--voldoc` authority is an ordinary `.voldoc`,** stored in the field's
   descriptor namespace and optionally written to disk. Feeding it back through
   `field-ingest` yields the same field id.

## Measured result (Phase 17.1)

Receipt:
[`2026-10-08-phase17-direct-field-d83ddba`](../../evidence/campaigns/2026-10-08-phase17-direct-field-d83ddba/);
[phase-17-results.md](../phases/phase-17-results.md), 17.1.

`doc-baseline` (6 GiB, cpus 8), 9 documents across pdf/docx/epub and four size
classes; current = `encode` + `field-ingest`, direct = `field-build`:

| quantity | current | direct | ratio |
|---|---:|---:|---:|
| build wall ms (sum) | 7,928 | 3,878 | **2.04×** (pdf 2.56×, epub 1.34×, docx 1.22×) |
| peak RSS KB (median) | 51,792 | 15,228 | **3.4× smaller** |
| authority bytes (sum) | 35,154,409 | 35,383,939 | **1.007×** |

Exactness **9/9** for each of three closures (current field, direct field, direct
authority decoded standalone; length + SHA-256 + `cmp`); observation equality
**53 equal / 46 decline-equal / 0 divergent** over an 11-entry schedule.

## Recorded caveat: the PDF metadata projection is not encoder-independent

For **PDF**, `Selector::Metadata` is the native *structural-descriptor*
projection and embeds the chosen program's `object_count` and `graph_ops`. A
fixed program reports its own; the searched winner reports its winner's
(e.g. `nist-pdf-0002` 29/1346 searched vs 1/1 direct; `nist-pdf-0004` 32/763 vs
1/1; `nasa-pdf-0007` 0/1 vs 1/1). Every other metadata field is byte-identical,
and DOCX/EPUB common metadata is format-model derived, so packages are
unaffected. This is a real finding — the shipped common PDF metadata projection
is not encoder-independent — and it is recorded rather than silently changed,
because altering a shipped observation's contract is out of scope for this
implementation task. A follow-up could make that projection report only
encoder-independent facts.

## Consequences

- The field's build cost drops materially (**2.04×** wall, **3.4×** lower peak
  RSS) **without** changing exactness or the observation surface. The build
  economics weakness recorded by Phase 16/ADR-0050 is reduced, not eliminated
  [SUPERSEDED: the direct build plus batched packed-store durability
  (ADR-0053) invert the equal-contract build position — Phase 19 measures VOLE at
  paired median **0.182** (95% CI 0.102–0.228; ratio-of-sums ~0.96) of SQLite on
  the same 12-document court; see `docs/phases/phase-18-results.md`,
  `docs/phases/phase-19-results.md`, ADR-0054. At the time of this ADR SQLite did
  still build faster than the two-step path] on the contract subset.
- **No wire byte, descriptor byte, or decode-path behaviour changes.** The exact
  authority is an ordinary `.voldoc`; `materialize(descriptor) == original_bytes`
  is the unchanged authority.
- **No cap was raised** and no validate-or-decline rule was weakened: the change
  removes search work, it does not bypass exactness. The complete-cost court still
  runs, over exactly one candidate.
- This is **not** a compression claim and not a claim that RAW is a good
  whole-file codec; RAW is the cheapest exact program, which is the point of the
  runtime profile.
- The remaining PDF metadata shape-counter difference is a documented limitation
  for any consumer that compares PDF metadata bytes across encoders.

## References

- `evidence/campaigns/2026-10-08-phase17-direct-field-d83ddba/`
- `docs/phases/phase-17-results.md` (17.1)
- `src/field/build.rs` (`BuildProfile::Runtime`), `src/field/ingest.rs` (Stage B)
- ADR-0050 (SQLite-as-substrate open question); ADR-0041 (large-PDF encode bound)
- ADR-0024/0026 (field authority, observation/provenance model);
  ADR-0001 (exact bytes only)
