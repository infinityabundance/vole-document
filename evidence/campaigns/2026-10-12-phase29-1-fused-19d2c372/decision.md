## Verdict

- **Correctness: 228/228 fused answers equal the pyarrow/DuckDB oracle** across 114 (fixture, format, question) clusters.
- **fused vs pyarrow**: median **0.077**, geomean **0.263**, CI **0.064..0.344**, MDE **0.201** → resolved faster.
- **fused vs duckdb**: median **0.003**, geomean **0.007**, CI **0.002..0.010**, MDE **0.006** → resolved faster.
- **fused vs the current in-process observe path** (count only): median **0.034**, geomean **0.024**, CI **0.029..0.039**, MDE **0.007**.
- **Large-input gap to pyarrow (e_large)**: the worst fused/pyarrow ratio is **52.229** (csv/score:avg); the gap remains — pyarrow's vectorized kernel still wins there, reported honestly.

- Declines are **capability accounting**: a lane that declines is never scored as a win.
- Estimator: paired per-(fixture,format,question) ratio, one per rep (inner samples reduced to a median); median + geomean with a fixed-seed (29115) cluster bootstrap over 114 clusters, B=20000; MDE from the median-CI half-width (80% power).
