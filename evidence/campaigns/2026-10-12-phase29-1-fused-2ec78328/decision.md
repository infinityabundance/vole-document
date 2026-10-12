## Verdict

- **Correctness: 228/228 fused answers equal the pyarrow/DuckDB oracle** across 114 (fixture, format, question) clusters.
- **fused vs pyarrow**: median **0.046**, geomean **0.168**, CI **0.037..0.126**, MDE **0.064** → resolved faster.
- **fused vs duckdb**: median **0.002**, geomean **0.005**, CI **0.001..0.009**, MDE **0.006** → resolved faster.
- **fused vs the current in-process observe path** (count only): median **0.023**, geomean **0.013**, CI **0.008..0.027**, MDE **0.013**.
- **Large-input gap to pyarrow (e_large)**: the worst fused/pyarrow ratio is **42.362** (csv/id:sum); the gap remains — pyarrow's vectorized kernel still wins there, reported honestly.

- Declines are **capability accounting**: a lane that declines is never scored as a win.
- Estimator: paired per-(fixture,format,question) ratio, one per rep (inner samples reduced to a median); median + geomean with a fixed-seed (29115) cluster bootstrap over 114 clusters, B=20000; MDE from the median-CI half-width (80% power).
