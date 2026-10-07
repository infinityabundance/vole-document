# Phase 15.6 — adaptive procedural promotion: court frontier

Lanes: v_off, v_on, sq_min, sq_full, sq_adapt. Falsifier thresholds: X=10%, Y=20%, Z=20% (design §5). Metric = median over documents of cumulative wall ms (ingest + prefix queries) at each diversity depth.

## Diversity — 3 documents: nist-docx-0005, nist-epub-0008, nist-pdf-0007

| depth | VOLE (no promotion) | VOLE --promote | SQLite minimal | SQLite full | SQLite adaptive | best | v_on vs sq_adapt |
|---|---:|---:|---:|---:|---:|---:|---:|
| 1 | 96.0 | 87.0 | 84.0 | 61.0 | 73.0 | SQLite full | +19.2% |
| 2 | 84.0 | 85.0 | 120.0 | 60.0 | 90.0 | SQLite full | -5.6% |
| 3 | 127.0 | 130.0 | 159.0 | 57.0 | 119.0 | SQLite full | +9.2% |
| 4 | 125.0 | 133.0 | 200.0 | 61.0 | 146.0 | SQLite full | -8.9% |
| 5 | 130.0 | 210.0 | 239.0 | 65.0 | 173.0 | SQLite full | +21.4% |

**Falsifier 1 (diversity) REFUTES promotion:** `v_on` is never more than 10% faster than `sq_adapt` at any depth (crossing depths: none).

Falsifier 2 (mechanism): promoted-store bytes cut total durable bytes by at most **0.0%** (need ≥20% at equal-or-better latency). **REFUTED** — the governor's durable store adds no byte distinction.

_No revision rows._
