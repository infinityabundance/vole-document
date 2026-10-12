## Verdict

- **Correctness: 60/60 fused answers equal the pyarrow/DuckDB oracle** across 30 (fixture, format, question) clusters.
- **fused vs pyarrow**: median **0.792**, geomean **1.992**, CI **0.664..3.661**, MDE **2.142** → not resolved faster (overall).
- **fused vs duckdb**: median **0.098**, geomean **0.075**, CI **0.057..0.118**, MDE **0.044** → resolved faster.
- **fused vs the current in-process observe path** (count only): median **0.005**, geomean **0.003**, CI **0.001..0.008**, MDE **0.005**.
- **Large-input gap to pyarrow (e_large)**: the worst fused/pyarrow ratio is **42.218** (csv/id:sum); the gap remains — pyarrow's vectorized kernel still wins there, reported honestly.

- Declines are **capability accounting**: a lane that declines is never scored as a win.
- Estimator: paired per-(fixture,format,question) ratio, one per rep (inner samples reduced to a median); median + geomean with a fixed-seed (29115) cluster bootstrap over 30 clusters, B=20000; MDE from the median-CI half-width (80% power).
