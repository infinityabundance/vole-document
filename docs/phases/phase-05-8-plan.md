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
| ✅ 5.8.1 | `PACKED_CHANNELS` DRA op | reconstruct from a data channel + a plan channel (item table) with a declared output length validated at eval; DRA v6; universe → phase5-8 |
| ✅ 5.8.2 | Layout+rANS candidate | build the layout plan, rANS-code the data object and the plan as separate channels, one program op; measure vs BYTE_RANS |
| ✅ 5.8.3 | Court + evidence | ablation court with the new lane; campaign receipt; honest verdict |
| ✅ 5.8.4 | Docs + freeze + merge | ADR-0013; docs + version; merge `phase5-8` into `main` |

## Acceptance gates (predeclared)

1. **Exactness**: byte-for-byte materialization for every accepted descriptor.
2. **Prediction correctness**: predicted offsets equal the source or fall back to a
   literal residual; never a wrong byte.
3. **Complete cost**: channel models, payloads, and the plan are fully charged.
4. **Head-to-head**: the result against BYTE_RANS is reported plainly, win or lose.
5. **Hostile-safe**: declared-length mismatch, truncated channels, and malformed
   plans return typed errors; never panic.
6. **Determinism**: identical input ⇒ identical descriptor bytes.

## Results (measured)

All subphases are complete. Campaign `2026-10-05-phase5-8-cf8048d` (verdict PASS)
runs the forced-candidate ablation (`encode --force KIND`, including
`pdf-layout-rans`) over the 11-file corpus; every auto winner round-trips
byte-exactly (`cmp` + `verify`).

Cumulative ladder (serialized `.voldoc` bytes):

```text
A0 RAW               = 81591
A2 + BYTE_RANS       = 48818
A6 + PDF_LAYOUT_RANS = 48818     leave-one-out layout+rANS delta = 0
```

Forced sizes:

```text
classic.pdf  RAW 683   BYTE_RANS 728   PDF_LAYOUT 731   PDF_LAYOUT_RANS 883
bigtext.pdf  RAW 65903 BYTE_RANS 38174 PDF_LAYOUT_RANS 38341
many.pdf     RAW 10235 BYTE_RANS 5201  PDF_LAYOUT 10089 PDF_LAYOUT_RANS 5914
             (data 7877, plan 1815, models 645, payload 4775)
```

Head-to-head against `BYTE_RANS`: **win 0, lose 8, decline 3** (classic +155,
bigtext +167, many +713). `PDF_LAYOUT_RANS` is never the auto winner and the A6
rung does not move below A5.

**Conclusion (honest negative).** `PACKED_CHANNELS` is exact, bounded, and
implemented (DRA v6), and `PDF_LAYOUT_RANS` is byte-exact — but layout+rANS does
**not** beat `BYTE_RANS`. Channel 0 entropy-codes nearly the whole file (the same
job `BYTE_RANS` does with one channel), while the plan channel (1,815 B on
`many.pdf`) plus a second model are added metadata `BYTE_RANS` never pays; the
predictable bytes saved (~7 B per xref entry after framing) are smaller than the
plan cost at this scale. Three phases (4, 5, 5.7) plus this one converge: at the
tested scale, PDF structural proceduralization does not beat a whole-file order-0
rANS lane. Recorded as ADR-0013; the mechanism stays available, not adopted.
An in-repo ADR (`docs/adr/0013-layout-rans-not-profitable.md`) and the campaign
receipt under `evidence/campaigns/2026-10-05-phase5-8-cf8048d/` preserve the
result.
