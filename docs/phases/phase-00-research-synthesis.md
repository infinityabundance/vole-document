# Phase 0 — research synthesis (frozen)

This note freezes the Phase-0 findings that shaped the implementation. The raw
subagent reports live under the gitignored `research/subagents/phase-00/`; here we
record the adopted conclusions and the decisions they produced.

## Method

Four independent research subagents ran read-only, each writing one findings file
under `research/subagents/phase-00/`. Per the project protocol, a fifth pass acted
as a falsifier by re-checking the entropy claim against the facade/core sources.
Confidence was not treated as evidence: each adopted fact is marked VERIFIED
against a fetched source in the subagent report and is cross-checked by at least
one court in this repository.

## Findings

### Entropy / rANS (`entropy-rans-findings.md`)

- The crate is a thin facade; all core symbols are reached as
  `ryg_rans_rs::byte::*` (no root flattening).
- Safe manual decode: `rans_byte_dec_init`, `rans_byte_dec_advance_symbol`,
  `rans_byte_dec_renorm`, each returning `Result<_, DecodeError>`. The
  `alloc_utils::decode` wrapper **panics** on truncation and is banned on the
  hostile path.
- Models are plain public structs; there is **no** serialization API, so VOLE
  must define its own model descriptor and normalization.
- Adopted by ADR-0006. Consequence: Phase 2 must own the model format and the
  capsule descriptor.

### Exact DEFLATE replay (`preflate-findings.md`)

- Raw-DEFLATE entry points: `preflate_whole_deflate_stream` /
  `recreate_whole_deflate_stream`, plus a streaming pair for large inputs.
- Correction state is a private, version-coupled bitcode+CABAC blob; store it
  **opaque** and version-stamp it.
- Reconstruction can panic on hostile stored state; isolate and bound it, and
  enforce external time/memory caps. `plain_text_limit` is not checked for stored
  (BTYPE=0) blocks.
- Adopted by ADR-0007.

### EntropyFS (`entropyfs-findings.md`)

- An embeddable store API exists: `entropyfs::engine::Engine`, explicitly usable
  with `default-features = false` (base build drops fuse/ublk/uring/tracing).
- Concrete struct, no `ObjectStore` trait: `put_blob`/`get_blob`/`read_blob_range`,
  BLAKE3 `BlobId([u8;32])`. VOLE defines its own abstraction + adapter.
- `dsfb` is a hard (non-optional) dependency, and all deps are exact-pinned.
- A store directory and an exclusive lock are required; no daemon/root/FUSE.
- Adopted by ADR-0008.

### PDF forensics and the Presse methodology (`pdf-presse-findings.md`)

- xref entries are literal file byte offsets; incremental updates are an
  append-only revision chain; `/Size` never decreases; compressed-object
  membership is physical.
- qpdf's JSON omits offsets, flattens object streams, and transparently decrypts;
  it is not a byte authority.
- The output document is the unit under test; own a strict validator and a
  known-bad control; numbering/`max_id`/`/Size`/xref size are one coupled system.
- Adopted by ADR-0009.

## What this changes about the plan

1. The entropy layer must own its model format and a full capsule descriptor —
   there is nothing to reuse from the substrate crate's serialization.
2. Codec-replay candidates must be treated as hostile and bounded, with a store-raw
   fallback always present.
3. The store is a Phase-9 concern and must remain optional.
4. PDF work starts from an owned physical scanner, never a writer or qpdf JSON.

## Open questions carried into later phases

- Which typed channels actually beat monolithic byte coding after model bytes
  (Phase 4/5 courts)?
- Where does exact DEFLATE replay lose (Phase 6 negative records)?
- Does the PDF xref/`startxref`/`/Length` proceduralization pay its residual cost
  broadly, or only for particular producers (Phase 5)?
