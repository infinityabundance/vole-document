# Phase 22.3.0 — duplicate work across a heterogeneous batch (measurement only)

**Verdict: SESSION-ALREADY-CAPTURES.** Docs 12; determinism PASS. Store built once per document; every lane starts from a cold derived cache.

## The question

`observe_batch` is a plain loop. Before building a fused executor, is there ≥2× duplicate work in an independent batch, and does the *shipping* resident session already remove it? `indep` disables the typed-model memo AND the derived cache; `resident` is the default session.

## Headline (deterministic counters)

| fmt | docs | indep exec | resident exec | exec_dedup | idx_dedup | seed_bytes_dedup | cache_write_dedup |
|---|---:|---:|---:|---:|---:|---:|---:|
| docx | 4 | 148 | 36 | 4.1111 | 1.0000 | 4.5000 | 5.1248 |
| epub | 4 | 647 | 177 | 3.6554 | 1.0000 | 1.8090 | 21.0487 |
| pdf | 4 | 26 | 26 | 1.0000 | 1.0000 | 0.9913 | 1.0000 |

Pooled: indep_exec **821** → resident_exec **239** = exec_dedup **3.4351**; index_nodes indep **461** → resident **461** = idx_dedup **1.0000**.

## Reading

- **The duplication is large and is already captured by the shipping session.** On DOCX/EPUB an independent batch executes ~3.7–4.1× more seed nodes than the default resident session (typed-model memo + derived cache); the resident session's `seed_nodes_executed` falls to the per-document minimum. The `≥2×` target a fused executor was meant to hit is therefore *already met by shipped code* on the formats where duplication exists.
- **PDF has no duplicate work to fuse.** Its frozen schedule (page-text, byte-range, metadata, revision-lineage) is mutually disjoint: indep_exec == resident_exec and `struct_dup_proxy` ≈ 1.0.
- **The one axis the session never dedupes is the index/selector descent** (`idx_dedup` = 1.0 for every document: `index_nodes_read` is identical with and without the memo/cache). That is exactly the 22.2 candidate — index read+verify was 10.5 % of the warm session and a *perfect* selector directory was measured **sub-MDE** (implied shift ≈0.13 vs MDE ≈0.40).

## Caveats (why the earlier `CLEARS` was withdrawn)

- **`dependency_ids` is not the executed closure.** It omits dependency nodes and, for DOCX/EPUB, many observations report the *same* shared model nodes (the null-overlap control collapses to `struct_dup_proxy` = 2.0 for DOCX/EPUB because `--block 0` and `--block 1` share the model). Any metric of the form `indep_exec / |U|` is therefore unsound; it is retained here only as a clearly labelled secondary proxy.
- **Node counts are a proxy for work, not time or bytes.** The byte-weighted view is dominated by the (undeduped) index bytes, so the composite byte ratio is ≈1.0 while the node-execution ratio is ≈4×. A clean `≥2×` gate on decoded/materialized work is **not established**; the honest reading is "no shippable mechanism, insufficient resolution to claim a win".
- **DOCX resident lane is slower in wall than indep** (e.g. 39.8 ms vs 1.9 ms) because `--no-cache` disables cache *consultation* but not cache *writes*, and the first resident request writes the derived cache to disk. This court is a counter court; wall is recorded but not interpreted.

## Decision

**Do not build a fused executor as scoped (22.3.1).** The cross-request redundancy it targets is already removed by the shipping session's memo + cache; the only structurally unexploited axis is the index/selector descent, already rejected as sub-MDE in 22.2. Nothing is shipped. This is consistent with the Phase-22.3 scope's pre-registered negative and with the plan's "existing memoization captures most reuse" risk.

