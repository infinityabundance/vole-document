# Phase 5.7 — packed segment framing (Phase-6 prep)

Root cause from Phases 4 and 5: the reconstruction algebra charges per-segment
framing (one op + tag + 4-byte length per literal run), which exceeds the bytes
that structural prediction or typed channels save at document scale. This phase
amortizes that framing so structure can pay, then re-runs the Phase-4 and
Phase-5 courts to test — honestly — whether channels and layout become viable.

Execution rules (frozen):
- All compilation/testing in the pinned Docker `dev` service. Never on host.
- Subphases in order; each: implement, test in Docker, commit, push to `phase5-7`.
- Subagents one at a time, disjoint files.
- Adoption requires byte-exactness and a strict complete-cost win; otherwise it
  is recorded as a negative result.

## Subphases

| # | Subphase | Deliverable |
|---|---|---|
| 5.7.1 | `PACK_SEGMENTS` DRA op | one op carrying a compact item table (varint-length literal runs + mark/emit items) over a single data object; DRA v5; universe → phase6-prep |
| 5.7.2 | Layout-v2 candidate + scale sample | rebuild the PDF layout candidate on `PACK_SEGMENTS` (payload as one object, compact plan); add a large classic-xref PDF (hundreds of objects) to expose the scaling win |
| 5.7.3 | Court re-run + evidence | extend the ablation court with the layout-v2 lane over the enlarged corpus; cumulative ladder + leave-one-out; campaign receipt |
| 5.7.4 | Docs + freeze + merge | ADR (profitability threshold or honest negative); docs + version; merge `phase5-7` into `main` |

## Acceptance gates (predeclared)

1. **Exactness**: every accepted descriptor materializes byte-for-byte.
2. **Prediction correctness**: regenerated offsets equal the source or fall back to
   a literal residual — never a wrong byte.
3. **Complete cost**: packed plan bytes, data object, and residuals are fully
   charged; adoption requires a strict win.
4. **Honest result**: if packed framing still does not make channels/layout
   profitable, record it (ADR) with the measured threshold analysis.
5. **Hostile-safe**: malformed inputs never panic; the packed op never reads out of
   bounds or invents bytes.
6. **Determinism**: identical input ⇒ identical descriptor bytes.
