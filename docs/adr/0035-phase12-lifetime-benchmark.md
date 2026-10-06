# ADR-0035 — Phase-12 lifetime benchmark: pre-registered methodology

Status: accepted (Phase 12.0).
Extends ADR-0027 (four accounting universes, lifetime-cost headline). Relates:
ADR-0017 (lossless baselines), ADR-0021/0028 (sharing negatives). Cites plan
§58–§65, §88, §91, §105–107, §117–124; research F §1–§7, G §1–§8, J §2, J §6.

> **Post-review amendment (Phase 12.15 skeptic, `15b5729`; updated by 12.11b,
> `862715c`).** The pre-registered ablation ladder of this ADR (`A1b`, `A2`–`A10`,
> §106 eager-vs-progressive, §107 raw-vs-decoded) was **not measured** in the first
> sealed receipt (`evidence/campaigns/2026-10-06-phase12-lifetime-3eaf576/`, A0/A1/V
> only). It **has now been run** in
> [`evidence/campaigns/2026-10-06-phase12-lifetime-ablations-862715c/`](../../evidence/campaigns/2026-10-06-phase12-lifetime-ablations-862715c/):
> `A1b`, `A2`, `A3`, `A4`/`A5`, `A6`, `A9`, `A11` are measured lanes; **`A5`, `A7`,
> `A8` are not separable** in the landed architecture (single feature gates — no
> eager arm, no native-only dispatch path, no non-indexed build) and **`A10` cannot
> manifest in a per-document lifetime court** (measured ingest-side instead); each is
> recorded with its reason, never fabricated. §106 is not separable (progressive
> inversion is the only mode); §107 is proxied by the `A4` (`--no-cache`) vs `A6`
> (cache) lanes. Attribution: the **native package graph** (A3→A4) enables the
> semantic surfaces and **persistent reuse** (A4→A6) trades per-query bytes for CPU.
> The failed gate `N6` (PDF no regression) still has **no receipt**. See
> [`docs/phases/phase-12-results.md`](../phases/phase-12-results.md) and
> [`docs/reviews/phase-12-skeptic-review.md`](../reviews/phase-12-skeptic-review.md).

## Context

Plan §22 is explicit: success is **not** "we parsed three formats" but a
**lifetime-cost** result with a competent SQLite+FTS baseline as an acceptance
gate. Phase 11's byte court was a loss, and two traps are pre-identified: the
measurement-boundary shift (instrumented VOLE bytes vs process `read`/`pread64`,
ADR-0027 correction) and warm-vs-cold reuse confusion. The methodology must be
frozen before it is run (research J §6).

## Decision

* **Pre-registered ablations** (plan §105): `A0` direct tooling per query; `A1`
  one-time preprocessed **source-retaining** SQLite+FTS (acceptance gate); `A1b`
  `A1` + result cache / materialized views (opt-in, only for pre-registered
  repeated expensive queries); `A2` Phase-11 field; `A3` ZIP physical only; `A4`
  +package graph; `A5` +progressive semantic inversion; `A6` +persistent reuse;
  `A7` +common observation layer; `A8` +hierarchical indexes; `A9` +EntropyFS
  range access; `A10` +cross-document sharing; `A11` full Phase 12. Plus
  eager-vs-progressive (§106) and raw-compressed-vs-decoded-persisted (§107).
* **Complete accounting boundary.** Report the four ADR-0027 universes separately,
  never summed, plus the baseline **derivative footprint**:
  `retained source (in .db) + .db + -wal + -shm + cache`, asserting
  `db_bytes >= retained_source_bytes`. Never sum VOLE-instrumented bytes with
  process `read`/`pread64`; reduce any win/loss to **one boundary** first. A
  cache-served row counts the cached-answer payload it physically re-reads.
* **Falsifiable headline** (J §6): on a pre-registered, generator-varied mixed
  PDF+DOCX+EPUB corpus, in fresh OS processes, the shared field answers a frozen
  observation set byte-exactly and with lower cumulative lifetime cost (process
  `read`+`pread64`+peak RSS+wall, one-time costs amortized over
  `N ∈ {1,10,100,1000}`) than a competent one-time-preprocessed SQLite+FTS5
  baseline that retains its source in full, on **at least one pre-registered
  surface per format** — with every loss to `A0`/`A1` recorded in the same table.
* **Go/no-go.** Phase 12 is recorded **negative** if any of: `N1` empty frontier
  (`A1` ≥ VOLE on every surface at every N); `N2` definitional (capability-only)
  wins; `N3` reuse ≈0 or below strongest CDC on reuse work; `N4` ETL illusion
  (decline rate above the pre-registered threshold); `N5` package-index-only
  (reproducible by `unzip -p` + `substr` at ≤ the same boundary); `N6` PDF
  regression. A negative result is a successful phase (ADR-0023/0028 discipline).
* **The DB baseline is an acceptance gate, not optional.** The court must be able
  to return "`A1` wins the lifetime frontier" and must return it whenever `A1`'s
  complete footprint beats VOLE at every N; a headline that never loses is not
  credible (Phase-11 precedent).

## Consequences

* Every claim names corpus, workload, ingest, the profile and the four universes;
  "never within N ≤ 1000" is a first-class verdict.
* Capability surfaces `A1` cannot answer are excluded from the lifetime frontier
  and reported separately, or the frontier is rigged.

**Rejected:** a single scalar lifetime number; excluding the retained source or the
cache payload from `A1`; comparing instrumented and process byte counters; deriving
reuse from source size; tuning the query schedule after seeing results.
