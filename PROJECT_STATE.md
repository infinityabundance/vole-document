# Project state and mechanism ledger

This is the single authoritative status table. "Implemented" and "proven" are
**not** interchangeable: a mechanism is `ADOPTED` only after its predeclared gate
passes and a sealed campaign exists. Failed hypotheses stay in the history — they
are evidence.

## Status vocabulary

`PROPOSED` → `PROTOTYPED` → `IMPLEMENTED` → `MEASURED` → `ADOPTED`
(or `RECORDED` / `REJECTED` / `STOPPED`).

## Ledger

| Mechanism | Phase | Status | Notes |
|---|---|---|---|
| Byte-exact invariant (`materialize(D) == X`) | 1 | ADOPTED | the only normative profile |
| Length-delimited record container | 1 | ADOPTED | framing CRC-32C; unknown-mandatory fails closed |
| Typed errors + stable exit codes | 1 | ADOPTED | `src/error.rs` |
| Centralized resource limits | 1 | ADOPTED | `Limits::{DEFAULT,STRICT}` |
| SHA-256 whole-source identity | 1 | ADOPTED | durable receipt |
| Literal DRA (`EMIT_OBJECT`, `INLINE`, `REPEAT_LAST`) | 1 | ADOPTED | bounded, non-Turing-complete |
| Coverage certificate (checked invariant) | 1 | ADOPTED | rejects gaps/overlaps before allocation |
| RAW exact opaque adapter | 1 | ADOPTED | correctness floor for every file type |
| Complete-cost court + decode-before-commit | 1 | ADOPTED | winner priced from serialized bytes only |
| Native rANS floor (order-0 / typed channels) | 2 | PROPOSED | substrate beneath structure, not the model |
| Entropy-seed **capsule** (full decoder-entry state) | 2 | PROPOSED | never a scalar "magic seed" |
| PDF byte-authoritative physical scanner | 3 | PROPOSED | sees Phase 0 research synthesis |
| PDF lexical/structural channels | 4 | PROPOSED | — |
| PDF xref/`startxref`/`/Length`/revision proceduralization | 5 | PROPOSED | high-value target |
| Exact DEFLATE replay (`preflate-rs`) | 6 | PROPOSED | candidate, per-stream, exactness first |
| Nested PDF content proceduralization | 7 | PROPOSED | the clearest embodiment of the thesis |
| PDF grammar/templates | 8 | PROPOSED | must pay definition cost |
| EntropyFS store-backed form | 9 | PROPOSED | optional substrate; `engine::Engine` |
| DSFB encoder-only search governance | 10 | PROPOSED | **zero** decode authority |
| Partial materialization | 11 | PROPOSED | checkpoints cost bytes |
| Fuzzing / property courts | 1+ | PROPOSED | targets listed in `CONFORMANCE.md` |
| Cross-document proceduralization | 12+ | PROPOSED | — |
| Non-PDF adapters (DOCX/ODT/EPUB/…) | later | PROPOSED | adapters over the same core |

## Phase 0 research (frozen into this repo)

The prior-art paper is the architectural authority. Independent Phase-0 subagent
findings were produced under the (gitignored) `research/subagents/phase-00/` and
are frozen into ADRs and this ledger. Key verified facts adopted:

- `ryg-rans-rs` 0.5.1 is `ryg_rans_rs::byte::*`; safe manual decode via
  `rans_byte_dec_init` / `rans_byte_dec_advance_symbol` / `rans_byte_dec_renorm`
  returning `Result`; the `alloc_utils::decode` convenience path **panics** and is
  banned from the normative hostile-input path. See ADR-0006.
- `preflate-rs` 0.7.6 exposes `preflate_whole_deflate_stream` /
  `recreate_whole_deflate_stream` (plus a streaming pair); raw DEFLATE only;
  correction state is an opaque, version-coupled bitcode+CABAC blob; reconstruction
  can panic on hostile state and must be isolated and bounded. See ADR-0007.
- `entropyfs` 0.7.17 exposes an embeddable `engine::Engine` (BLAKE3 `BlobId`,
  `put_blob`/`get_blob`/`read_blob_range`), usable with `default-features=false`;
  `dsfb` is a hard (non-optional) dependency; a store directory + exclusive lock
  is required. See ADR-0008.
- PDF xref entries are literal file byte offsets; incremental updates are an
  append-only revision chain; `/Size` never decreases; object-stream membership is
  physical. qpdf JSON omits offsets and transparently decrypts, so it is an oracle,
  never the authority. See ADR-0009.

## Freeze policy

The wire format is **not** frozen. It becomes v1 only when the exact core, bounds,
integrity, and the PDF golden path are stable and a hostile-input format court is
green (see `docs/adr/0004`). Until then, all format changes bump the universe
string and/or the header minor version.
