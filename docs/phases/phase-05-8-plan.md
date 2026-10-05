# Phase 5.8 — layout + rANS residual (phase5-8)

Isolate the last lever from ADR-0012: the layout candidate removes predictable
bytes (xref offsets, startxref) but stores the residual data object literally, so
an order-0 rANS lane still dominates it. This phase entropy-codes the residual:
a candidate that performs **structural layout prediction** (PACK_SEGMENTS plan)
**over rANS-coded channels** (the data object and the plan), and is measured
head-to-head against BYTE_RANS.

Execution rules (frozen):
- All compilation/testing in the pinned Docker `dev` service. Never on host.
- Subphases in order; each: implement, test in Docker, commit, push to `phase5-8`.
- Subagents one at a time, disjoint files.
- Adoption requires byte-exactness and a strict complete-cost win; otherwise it is
  recorded as a negative result. No mechanism is hidden.

## Subphases

| # | Subphase | Deliverable |
|---|---|---|
| 5.8.1 | `PACKED_CHANNELS` DRA op | reconstruct from a data channel + a plan channel (item table) with a declared output length validated at eval; DRA v6; universe → phase5-8 |
| 5.8.2 | Layout+rANS candidate | build the layout plan, rANS-code the data object and the plan as separate channels, one program op; measure vs BYTE_RANS |
| 5.8.3 | Court + evidence | ablation court with the new lane; campaign receipt; honest verdict |
| 5.8.4 | Docs + freeze + merge | ADR-0013; docs + version; merge `phase5-8` into `main` |

## Acceptance gates (predeclared)

1. **Exactness**: byte-for-byte materialization for every accepted descriptor.
2. **Prediction correctness**: predicted offsets equal the source or fall back to a
   literal residual; never a wrong byte.
3. **Complete cost**: channel models, payloads, and the plan are fully charged.
4. **Head-to-head**: the result against BYTE_RANS is reported plainly, win or lose.
5. **Hostile-safe**: declared-length mismatch, truncated channels, and malformed
   plans return typed errors; never panic.
6. **Determinism**: identical input ⇒ identical descriptor bytes.
