# Phase 30.2 — fused CSV kernel vs pyarrow/DuckDB (read-inclusive)

**Measurement.** In-process steady-state median (Phase-28.2 harness; warmup 3, iters 9),
cold separated, ADR-0054 paired per-(question) estimator with a fixed-seed cluster bootstrap
(B=20000) and the MDE. Every raw sample is retained under `evidence/campaigns/2026-10-13-phase30-2-csv-8ed3e80b/raw`.

## Estimator — fused / comparator, paired per question (ratio < 1 ⇒ fused faster)

| comparison | clusters | samples | median | median 95% CI | geomean | MDE | wins | ties | losses |
|---|---|---|---|---|---|---|---|---|---|
| pyarrow_agg | 10 | 50 | 9.710 | [5.300, 12.104] | 9.028 | 4.863 | 0 | 0 | 50 |
| pyarrow_read | 10 | 50 | 0.362 | [0.312, 0.392] | 0.360 | 0.058 | 49 | 0 | 1 |
| duckdb_read | 10 | 50 | 0.033 | [0.032, 0.034] | 0.035 | 0.001 | 50 | 0 | 0 |
| duckdb_agg | 10 | 50 | 0.835 | [0.805, 0.913] | 0.856 | 0.078 | 38 | 15 | 12 |

## Serial continuity (`--workers 1`) — fused / comparator

| comparison | clusters | samples | median | median 95% CI | geomean | MDE | wins | ties | losses |
|---|---|---|---|---|---|---|---|---|---|
| pyarrow_agg | 10 | 50 | 23.797 | [14.186, 30.275] | 21.160 | 11.499 | 0 | 0 | 50 |
| pyarrow_read | 10 | 50 | 0.805 | [0.742, 0.914] | 0.843 | 0.123 | 36 | 11 | 14 |
| duckdb_read | 10 | 50 | 0.083 | [0.077, 0.089] | 0.083 | 0.008 | 50 | 0 | 0 |
| duckdb_agg | 10 | 50 | 2.015 | [1.909, 2.138] | 2.005 | 0.164 | 0 | 0 | 50 |

## Large-CSV headline (median of the per-question ratios)

| comparison | questions | median fused/comparator | geomean |
|---|---|---|---|
| pyarrow_agg | 10 | 9.554 | 8.465 |
| pyarrow_read | 10 | 0.366 | 0.350 |
| duckdb_read | 10 | 0.033 | 0.033 |
| duckdb_agg | 10 | 0.825 | 0.825 |

## Per-question fused/comparator ratio

| question | pyarrow_agg | pyarrow_read | duckdb_read | duckdb_agg |
|---|---|---|---|---|
| id:avg | 12.140 | 0.384 | 0.034 | 0.957 |
| id:count | 9.365 | 0.366 | 0.033 | 0.907 |
| id:max | 11.227 | 0.385 | 0.033 | 0.818 |
| id:min | 14.359 | 0.394 | 0.037 | 0.920 |
| id:sum | 13.141 | 0.454 | 0.032 | 0.916 |
| score:avg | 9.072 | 0.294 | 0.033 | 0.813 |
| score:count | 8.204 | 0.288 | 0.028 | 0.615 |
| score:max | 2.436 | 0.298 | 0.028 | 0.790 |
| score:min | 4.443 | 0.313 | 0.033 | 0.831 |
| score:sum | 9.742 | 0.365 | 0.043 | 0.743 |

## Fused steady-state distribution (ns), by format

| format | cells | p50 | p95 | p99 |
|---|---|---|---|---|
| csv | 26 | 145342 | 150943 | 155352 |

## Cold (fresh-process wall, ns)

- fused: median 3088644 ns
- pyarrow: median 76271908 ns
- duckdb: median 206848406 ns

## Correctness (oracle agreement)

- comparisons: 52, disagreements: 0, ok: True

## Exactness

- materialize --exact == source: 3/3
