# Phase 9 — Cross-document content-addressed store

Branch: `phase9`. Base: `main` @ `d3c43c7` (`v0.1.0-alpha.10`).
Status: **IN PROGRESS**.

## Why

Every axis measured so far loses to a purpose-built baseline: whole-file size to
generic LZ (ADR-0017), random-access bytes to seekable/blocked formats (Phase 8).
The one axis a *single-file* compressor structurally cannot serve is **sharing
across documents**: a content-addressed store can hold one copy of an object that
many documents reference. This phase tests that axis honestly.

Prime directive unchanged: `materialize(descriptor) == original_bytes`. A
store-backed descriptor must materialize the same bytes as its standalone form.

## Three accounting universes (kept permanently distinct)

- **Standalone** — every `.voldoc` self-contained; sum over the cohort.
- **Unique reachable** — the root descriptors plus every **unique** external
  object reachable from them, counted once each.
- **Amortized cohort** — shared objects distributed across the declared cohort
  according to an explicit, written rule (e.g. charged once to the cohort, or
  fractionally by reference count). The rule is part of the contract.

Never compare a store root reference against a whole file. Report all three, and
compare the standalone universe against **per-file** `gzip`/`zstd`/`xz`/`brotli`.

## Honest comparison set (the skeptic's baseline)

Because a store deduplicates, the fair baseline is **not only** per-file LZ but
also a **generic content-defined-chunk dedup** over the same cohort (BGZF/bup/
borg/casync-style rolling-hash chunking). The court reports both; if generic
chunk dedup captures the same sharing, that is the honest result.

## Deliverables

- `ObjectStore` abstraction (`put`/`get`/`get_range`, content-addressed by
  BLAKE3) with `EmbeddedStore` (a local content-addressed directory) as the
  reference backend; `EntropyFsStore` optional (feature `entropyfs-store`, the
  embeddable engine, `default-features = false`) — never required for standalone.
- A **store-backed descriptor form**: objects may be inline or an external
  content id (`EXTERNAL_REF`), resolved through an `ObjectResolver` at
  materialization. Standalone ↔ store-backed conversion, with both proven to
  materialize identical bytes.
- Reference-closure/GC correctness and unique-reachable accounting.
- A **cohort** (the producer corpus plus deliberately repeated / near-duplicate
  documents so sharing exists), measured in all three universes against per-file
  LZ and the chunk-dedup baseline; a sealed receipt and an ADR.

## Subphases

- **9.0** plan + research/design freeze (this file + an independent design
  subagent; ADR for the store contract and accounting rule).
- **9.1** `ObjectStore` + `EmbeddedStore` + store-backed descriptor form +
  conversion + closure/GC tests + universe bump.
- **9.2** optional `EntropyFsStore` (if the embeddable engine is viable in the
  pinned toolchain; otherwise record why and keep `EmbeddedStore`).
- **9.3** cohort + measurement campaign (three universes vs per-file LZ and
  chunk-dedup) + ADR + report.
- **9.4** skeptic + docs + release.

## Non-goals

Beating per-file LZ on a single document; calling cross-document sharing
"compression" (it is store amortization, reported separately); any claim that a
store root substitutes for a whole file.
