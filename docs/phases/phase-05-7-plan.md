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
| 5.7.1 ✅ | `PACK_SEGMENTS` DRA op | one op carrying a compact item table (varint-length literal runs + mark/emit items) over a single data object; DRA v5; universe → phase6-prep |
| 5.7.2 ✅ | Layout-v2 candidate + scale sample | rebuild the PDF layout candidate on `PACK_SEGMENTS` (payload as one object, compact plan); add a large classic-xref PDF (hundreds of objects) to expose the scaling win |
| 5.7.2b ✅ | Literal coalescing | merge adjacent literal runs so consecutive literals collapse to one item (`many.pdf` items 1,413 → 805) |
| 5.7.3 ✅ | Court re-run + evidence | extend the ablation court with the layout-v2 lane over the enlarged corpus; cumulative ladder + leave-one-out; campaign receipt |
| 5.7.4 ✅ | Docs + freeze | ADR (profitability threshold or honest negative); docs + version (`0.1.0-alpha.6`); merge `phase5-7` into `main` |

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

## Outcome (Phase 5.7)

All subphases complete; campaign `2026-10-05-phase5-4521778` (verdict PASS) over
an 11-file corpus. Every auto winner round-trips byte-exactly (`cmp` + `verify`).

```text
A0 RAW            = 81371
A2 + BYTE_RANS    = 48598
A5 + PDF_LAYOUT   = 48598     leave-one-out layout delta = 0

many.pdf     layout-v2 10069   RAW 10215   BYTE_RANS 5181   (layout beats RAW by 146 B)
classic.pdf  layout      711   RAW   663
bigtext.pdf  layout    65929   RAW 65883   BYTE_RANS 38154
```

Literal coalescing (5.7.2b) cut the `many.pdf` (200 objects, 9,881 B) item table
from **1,413 to 805** items.

**Conclusion.** Packed framing (`PACK_SEGMENTS`, DRA v5) does what it was built to
do: it amortizes per-segment framing and lets structural layout prediction **beat
RAW at scale** (`many.pdf` 10,069 vs 10,215), which the per-segment Phase-5 lanes
never managed. It is nevertheless **not adopted**: layout still loses to
`BYTE_RANS` (5,181), wins 0 of 8 classic-xref samples, and the leave-one-out
delta is 0, so it is never the auto winner. The reason is now precise: the
framing is fixed, but the **residual data object is stored literally**, so any
order-0 entropy lane dominates it. The next lever is to entropy-code the residual
— structural prediction **composed with** rANS on the residual, the paper's
layered model — not to pack the literals further. Recorded as a measured, partial
positive (ADR-0012).

Receipt: `evidence/campaigns/2026-10-05-phase5-4521778/`.
