# Phase 4 — PDF lexical and structural channels (subphase plan)

Goal: split the exact PDF byte stream into **typed channels** (token kinds,
lengths, per-kind payloads, lexical residuals), entropy-code each channel with
its own model, and reconstruct the exact bytes — then let the complete-cost court
accept or reject each channel split. Reject any split whose model overhead
exceeds its savings.

Execution rules (frozen):
- All compilation/testing in the pinned Docker `dev` service; PDF oracles in
  `tools`. Never on host.
- Subphases in order; each: implement, test in Docker, commit, push to `phase4`.
- Subagents one at a time, disjoint files, testing via
  `docker compose run --rm --no-TTY dev cargo test`.
- A candidate is admitted only if byte-exact and strictly cheaper than the best
  fallback on complete serialized cost (per-channel model bytes charged).

## Subphases

| # | Subphase | Deliverable |
|---|---|---|
| 4.1 ✅ | Channel transposition | map lexer spans to typed token kinds; split into kind sequence + lengths + per-kind payload streams; exact, reversible `join`; pure logic + tests |
| 4.2 ✅ | `INTERLEAVE_CHANNELS` DRA op (v3) | a bounded DRA op that reconstructs from kind/length/payload channels; universe → phase-4; materializer support |
| 4.3 ✅ | PDF-token candidate | encode each channel with rANS (own model), build the descriptor, charge all model+payload bytes, wire into the court |
| 4.4 ✅ | Court + reject gates | compete vs RAW/RLE/BYTE_RANS; reject splits whose model overhead loses; determinism; negative controls |
| 4.5 ✅ | Lexical refinement channels | whitespace classes, keyword ids, name references, with lexical residuals; compete vs the coarse split |
| 4.6 ✅ | Corpus exactness + ablation | text-heavy + structural PDF corpus; cumulative and leave-one-out channel ablations |
| 4.7 ✅ | Phase-4 evidence campaign | coverage/exactness, per-channel attribution, accepted/rejected splits, oracle re-check |
| 4.8 ✅ | Docs + freeze + merge | SPEC/PROJECT_STATE/README/CHANGELOG/ADR updated; merge `phase4` into `main` |

## Acceptance gates (predeclared)

1. **Exactness**: every accepted descriptor materializes byte-for-byte (length +
   SHA-256 + byte compare).
2. **Reversibility**: `join(split(x)) == x` for every corpus item.
3. **Complete cost**: every channel model and payload byte is charged; a split is
   adopted only if strictly smaller than the best existing lane.
4. **Honest rejection**: splits whose model overhead exceeds savings lose the
   court and are recorded, not hidden.
5. **Hostile-safe**: malformed PDFs and hostile channel descriptors never panic.
6. **Determinism**: identical input ⇒ identical descriptor bytes.

## Measured outcome (campaign `2026-10-05-phase4-3840bc4`)

Verdict **PASS**: every file round-trips byte-exactly through its auto-winning
lane (`cmp` + `verify`), and the qpdf oracle re-check passes. Receipt under
`evidence/campaigns/2026-10-05-phase4-3840bc4/`.

Cumulative ladder over the deterministic 10-file corpus (serialized `.voldoc`
bytes):

| Rung | Mechanism added | Total bytes | Step delta |
|---|---|---:|---:|
| A0 | RAW | 71,036 | — |
| A1 | + RLE | 71,036 | 0 |
| A2 | + BYTE_RANS | 43,297 | −27,739 |
| A3 | + PDF_PHYSICAL | 43,297 | 0 |
| A4 | + PDF_CHANNELS | 43,297 | 0 |

Leave-one-out channel delta = **0**. Auto winners: RAW = 8, `BYTE_RANS` = 2,
`PDF_PHYSICAL` = 0, `PDF_CHANNELS` = 0.

Forced per-mechanism sizes on the text-heavy scale sample `bigtext.pdf`
(65,549 B):

| Lane | Forced `.voldoc` bytes |
|---|---:|
| RAW | 65,871 |
| BYTE_RANS | 38,142 |
| PDF_CHANNELS | 46,432 |

The compact v2 model reduced per-channel **model** overhead from 7,224 B to
1,981 B, but `PDF_CHANNELS` still loses to `BYTE_RANS` by ~8,290 B.

### Recorded negative result

Coarse lexical transposition plus per-channel order-0 models does **not** beat a
monolithic order-0 `BYTE_RANS` on this corpus. Typed channels beat RAW (~21.6% on
`bigtext.pdf`) but are **rejected by the complete-cost court**; `PDF_CHANNELS` is
kept implemented and available but is not adopted. Recovering a win requires
*conditioning and ordering* (contextual models) rather than more per-kind
marginal models — Phase 5/8 work. See ADR-0010.
