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
| 4.1 | Channel transposition | map lexer spans to typed token kinds; split into kind sequence + lengths + per-kind payload streams; exact, reversible `join`; pure logic + tests |
| 4.2 | `INTERLEAVE_CHANNELS` DRA op (v3) | a bounded DRA op that reconstructs from kind/length/payload channels; universe → phase-4; materializer support |
| 4.3 | PDF-token candidate | encode each channel with rANS (own model), build the descriptor, charge all model+payload bytes, wire into the court |
| 4.4 | Court + reject gates | compete vs RAW/RLE/BYTE_RANS; reject splits whose model overhead loses; determinism; negative controls |
| 4.5 | Lexical refinement channels | whitespace classes, keyword ids, name references, with lexical residuals; compete vs the coarse split |
| 4.6 | Corpus exactness + ablation | text-heavy + structural PDF corpus; cumulative and leave-one-out channel ablations |
| 4.7 | Phase-4 evidence campaign | coverage/exactness, per-channel attribution, accepted/rejected splits, oracle re-check |
| 4.8 | Docs + freeze + merge | SPEC/PROJECT_STATE/README/CHANGELOG/ADR updated; merge `phase4` into `main` |

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
