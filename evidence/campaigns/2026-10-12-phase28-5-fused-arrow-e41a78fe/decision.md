## Decision

- Primary gate — fused vs **current-VOLE in-process** (same process, same field, the
  shipped `observe` path with the derived cache off): median **0.0253**, geomean **0.0328**,
  median 95% CI **0.0000..0.0423**, MDE **0.0302** → **resolved faster** beyond the MDE.
- Deployed shape — fused vs the **current-VOLE CLI** (fresh process per question):
  median **0.0046**, geomean **0.0057**, CI **0.0000..0.0107**, MDE **0.0077** → **resolved faster**.
- vs **pyarrow**: median **0.6000**, geomean **1.9948**, CI **0.0000..6.4694** — fused LOSES on aggregate (geomean > 1).
- vs **duckdb**: median **0.0077**, geomean **0.0358**, CI **0.0000..0.1657** — fused wins on aggregate (geomean CI < 1).
  pyarrow's vectorized kernel beats the fused scalar scan on the large
  column (`e_large.arrow`); the fused kernel wins on the small fixtures.
- Evidence variant vs pyarrow coordinates (Q2E): median **0.6000**, geomean **1.0312**
  (VOLE returns bounded exact source spans; pyarrow returns matching indices).

- **Verdict: RESOLVED positive.** The fused kernel is a resolved end-to-end
  improvement over the shipped VOLE path beyond the MDE, with exact agreement on the count.
  It does **not** claim to beat pyarrow everywhere: it still loses to pyarrow's
  vectorized kernel on the large fixture (see the ratio table).
