# Phase 18.2 — one-pass direct build (scan the original source; store + verify the authority)

Commit `61b11350` (branch `phase18`, dirty tree). All measurement in the
digest-pinned, hard-capped `doc-baseline` service; the gate in `dev`. No court
ran on the host.

## Where the redundant source → authority → source pass was

`field-build --profile runtime` already holds the exact source, yet the direct
ingest reconstructed it from the authority **twice more than necessary**:

1. `src/field/build.rs::direct_ingest` → `ingest_with`, which materialized the
   source from the authority **just to detect ZIP vs PDF**;
2. `src/field/ingest.rs::ingest_pdf_with` called `FieldStore::ingest` (which
   materializes once) **and** `Field::materialize_exact` (a second
   materialization) whose result was only handed to `scan()`;
3. `src/field/ingest_package.rs::ingest_package_with` called `materialize()` on
   the parsed observable purely to get the bytes to scan.

## What changed

* Added `FieldStore::ingest_verified(descriptor, source, limits)`: parse,
  materialize, **byte-compare to the caller's source** (the one remaining round
  trip — now a *verification*, not a discarded copy). `FieldStore::ingest`
  shares the same `ingest_parsed` tail, so the `field-ingest` CLI path is
  byte-for-byte unchanged.
* Split `ingest_pdf_with` into the unchanged materializing entry point and a
  `ingest_pdf_stage_b(source, manifest)` tail; added
  `pub(crate) ingest_pdf_direct(observable, source)` that verifies against and
  scans the caller's source.
* Split `ingest_package_with` the same way, adding
  `pub(crate) ingest_package_direct(observable, source)`.
* `build_field_with` now hands the **original source** and the already-enriched
  authority straight to the direct variants: one pass over the source for
  structure, one verified authority store. `encode` + `field-ingest` are
  untouched.

## Exactness (unimpaired)

9-document Phase-17 court, **before and after**, all three lanes:
`materialize --exact` length + SHA-256 + `cmp` = **9/9 current field, 9/9 direct
field, 9/9 direct authority decode**. The 12-document contract court reproduces
the exact closure **12/12** on both lanes. The observation schedule is
identical before/after: **53 equal, 46 decline-equal, 0 divergent** (99 total).
The stored authority blob (and its SHA-256, and the field id) is unchanged.

## Measurement (A/B, 9-document direct-field court)

The `current` control lane (`encode` + `field-ingest`) is untouched and agrees
between runs (7831 vs 7895 ms sum), so the A/B is fair.

| lane | direct sum ms | direct median ms | direct RSS median KB |
|---|---:|---:|---:|
| before | 4012 | 574 | 15184 |
| after  | 3769 | 524 | 10812 |
| Δ | **−6.1%** | −8.7% | **−28.8%** |

Peak RSS drops consistently on the large documents — `nasa-pdf-0007`
102160 → 72956 KB (−28.6%), `nasa-epub-0011` 94860 → 64496 KB (−32.0%),
`nasa-epub-0006` 99872 → 78748 KB (−21.2%).

**Honest reading:** the removed materializations were *cheap* — a RAW
literal-object decode is essentially a `memcpy` — so eliminating two of three
passes buys only ~6% wall. The robust win is peak RSS: the extra source-sized
buffers no longer coexist.

## Contract court (C0..C5, 12 documents)

| run | VOLE build sum ms | SQLite C5 build sum ms | build gap |
|---|---:|---:|---:|
| Phase 18.1 baseline | 11905 | 1644 | 7.24× |
| this change (clean re-run) | 11689 | 1621 | 7.21× |

**The gap does not move (7.24× → 7.21×).** The round trip was not the contract
court's bottleneck: its build wall is dominated by the descriptor encode (RAW
literal object) and the `with_observation_index` parse+reserialize, neither of
which this phase touches. Reported as measured; this phase narrows RSS, not the
contract build gap.

A first contract re-run (`TAG=onepass`, VOLE build sum 46679 ms) was **discarded
as contention**: four unrelated containers had been up 43–47 h (loadavg
~3.7/16) and inflated every single-process build wall (docx-0005 47 → 858 ms)
while the same binary measured ~4 MB / <10 ms immediately afterwards. The clean
`TAG=onepass2` re-run on the same binary/commit returned to 11689 ms.

## Gate

`cargo fmt --all --check` **pass**; `cargo clippy --all-targets --all-features --
-D warnings` **pass**; `cargo test --locked --all-features` **pass**;
`cargo test --locked --no-default-features` **pass**; `sh tools/phase1-court.sh`
**PASS** (8/8 corpus items `cmp=equal`).

## Residual risks

* `ingest_verified`'s byte-compare is a defence-in-depth guard; the encode court
  already proves `materialize(descriptor) == source` for the direct path.
* Wall results are single-run; the ~6% gain is within small-document noise. RSS
  is the trustworthy signal.
* The direct path scans the source for ZIP detection and then the package
  adapter scans it again — two ZIP scans, exactly as before (not consolidated).
* The ~7.2× contract build gap remains; the descriptor encode /
  observation-index reserialization is the untouched dominant cost.
