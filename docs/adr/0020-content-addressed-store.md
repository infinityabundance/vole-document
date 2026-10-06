# ADR-0020: A content-addressed object store; three accounting universes

- **Status:** Accepted (design frozen in `research/subagents/phase-09/store-contract.md`,
  Phase 9.0; implementation 9.1/9.2; the cohort measurement is Phase 9.3, so no
  numeric size claim is made here)
- **Date:** 2026-10-06

## Context

Every axis measured so far loses to a purpose-built baseline: whole-file size to
generic lossless compressors (ADR-0017), random-access bytes read to
seekable/blocked formats (ADR-0019). The axis a *single-file* compressor
structurally cannot serve is **sharing across documents**: a content-addressed
store can hold one copy of an object that many documents reference.

Exactness is unchanged and non-negotiable: a store-backed descriptor must
materialize the exact same bytes as its standalone form
(`materialize(descriptor) == original_bytes`). A store also makes it tempting to
report a small root reference as a small document, which would be a category
error. This ADR freezes the store contract and the accounting rules so the
eventual measurement cannot quietly conflate them.

## Decision

### Two digests, two roles — never interchangeable

| digest | role | authority |
|---|---|---|
| SHA-256 | whole reconstructed source; **archival identity** | parse/materialize/verify |
| BLAKE3-256 (`Id`) | one object's raw bytes; **store namespace** | relational/advisory |

An `Id` is an *ephemeral, relational* name for a shareable object. It is never a
substitute for the source digest and never appears in `INTEGRITY`. A store-backed
descriptor is still identified, verified, and receipted by the whole-source
SHA-256.

### The store-backed descriptor form

The object table is one ordered sequence; each entry is either an inline `OBJECT`
(`0x10`) record or an `EXTERNAL_REF` (`0x80`) record naming a store object. The
entry's **position** is its `object_id`, so the DRA is untouched and continues to
consume a plain vector of object bytes.

```text
EXTERNAL_REF (0x80) := id:[u8;32] | len:u64LE        # exactly 40 bytes
```

- **Parsing never needs the store.** `len` supplies the object length for the
  coverage certificate without resolving anything.
- **The feature bit is mandatory.** A descriptor with any `EXTERNAL_REF` sets
  `FEATURE_EXTERNAL_OBJECTS` (`1 << 1`); a decoder built without the `store`
  cargo feature fails closed with `UnsupportedFeature` (exit 6) at header
  validation rather than materializing a partial document.
- **Verification.** `ObjectStore::get` MUST re-hash the bytes and reject with
  `IntegrityMismatch` unless `BLAKE3(bytes) == id` and the length matches `len`.
  `get_range` carries no whole-object gate, so exactness for a store-backed
  materialization rests on the declared `EXTERNAL_REF` length plus the
  whole-source `INTEGRITY` SHA-256 checked by `materialize`.
- **Universe.** The string is re-based to `phase9` with
  `+external-objects-v1` appended; `dra-8` is unchanged and `FORMAT_MINOR` does
  not move (the mandatory bit carries fail-closed compatibility, the universe
  suffix is the versioned semantic marker).

`ObjectSource` is the in-memory mirror: `Inline(Vec<u8>)` or
`External { id: Id, len: u64 }`.

### Standalone ↔ store-backed conversion

Both directions are provided and must materialize identical bytes:

- `externalize(d, resolver, store)` — inline → external: put every object's bytes
  into `store` and replace it with an `EXTERNAL_REF`. Objects already external are
  resolved through `resolver` and re-put, so every object ends in the destination
  store; a reference the resolver cannot satisfy fails closed. Identical objects
  collapse to one store entry by content addressing (refcount > 1 is one stored
  object).
- `hydrate(d, resolver)` — external → inline: resolve every reference (verifying
  id and length) and replace it with `Inline`. The external feature bit clears
  automatically because `required_features()` is derived.

### Reference closure and GC

Objects are opaque leaves (no object→object edges), so reachability is one level:
the union of the roots' external ids. `gc(roots, store)` marks that union and
sweeps `stored \ mark`; `dangling = mark \ stored` must be empty for a valid
closure (a non-empty list is a live `MissingExternalObject`). After a sweep every
root must still materialize byte-exactly and closure must report zero dangling.

### Three accounting universes (kept permanently distinct)

Notation: cohort `C = {r_0..r_{n-1}}`; `d_i` is the standalone descriptor, `e_i`
its store-backed form, `reach(i)` its external ids, `O = ⋃_i reach(i)` the unique
reachable set, `refcount(o) = |{i : o ∈ reach(i)}|`.

```text
Standalone          S = Σ_i |serialize(d_i)|                       # whole-file
Unique reachable    U = Σ_i |serialize(e_i)| + Σ_{o ∈ O} len(o)    # shared once
Amortized cohort    A = Σ_i ( |serialize(e_i)| + Σ_{o ∈ reach(i)} len(o)/refcount(o) )
```

- **`S` is the only universe comparable to a per-file compressor.** A store root
  alone is never a whole document and is never compared to one.
- **`U` is the backend-independent headline.** The backend's physical bytes are
  recorded separately (`stored_bytes`): for the raw `EmbeddedStore`
  `stored_bytes == Σ len(o)`; a compressing backend reports less.
- **The amortized rule is fractional by reference count** (the frozen rule R2):
  each shared object is charged to the members that reference it, split by
  `refcount`. The per-file decomposition is integerized by largest remainder so
  that `Σ_i A_i == U` **exactly, with no rounding drift**. The headline amortized
  number is the cohort total, and it telescopes: `A == U` by construction, so
  "charged once to the cohort" (R1) is not a distinct fourth universe. With no
  sharing (`refcount == 1` everywhere) each `A_i` reduces to that root's
  standalone bytes plus the `EXTERNAL_REF`/`UNIVERSE` framing delta.

### Backends

- **`EmbeddedStore` is the reference and measured backend**: a local
  content-addressed directory (`objects/<aa>/<bb>/<64-hex-id>`), raw (no
  compression) so its accounting is transparent, with write-then-rename atomic
  `put`, a strict `get_range`, per-object `remove`, and a `STORE` backend marker.
  It is the default `store` feature and has no heavy dependency.
- **`EntropyFsStore` is optional and heavy.** A thin adapter over the embeddable
  `entropyfs = "=0.7.17"` `engine::Engine` (`default-features = false`), behind
  the non-default `entropyfs-store` feature. `put → put_blob`, `get → get_blob`
  (full whole-blob BLAKE3 gate), `get_range → read_blob_range` **plus an explicit
  `offset + len <= stored_len` check** (the engine clips at EOF; our contract is
  strict), `contains → contains`. The engine's `BlobId` is `BLAKE3-256` of the
  blob bytes, identical to our `Id`, so an `EmbeddedStore`-externalized descriptor
  resolves unchanged through it. It is **viable, not incompatible**, but it is not
  in the default build and not required for the standalone form: it pulls a
  **non-optional** `dsfb` (still zero decode authority, ADR-0008) and a ~40-crate
  tree, needs a dedicated store directory with an exclusive lock, and exposes
  **no per-blob enumeration or delete**, so `list`/`remove` decline with
  `UnsupportedFeature` and mark-and-sweep GC cannot reclaim through this adapter
  (reclamation is EntropyFS's own reachability-GC policy).

## Consequences

- `blake3 = "=1.8.7"` is a **default** dependency via the new `store` feature, so
  the default build is still permissive-only; `--no-default-features` builds the
  bare exact core with no store dependency.
- The mandatory `FEATURE_EXTERNAL_OBJECTS` bit and the `+external-objects-v1`
  universe suffix re-base every descriptor's `universe_id` and add ~21 B to the
  `UNIVERSE` record (as Phase 8's suffix did); standalone exactness is unaffected.
- The CLI gains `store put`, `store account`, `store gc`, and `decode --store
  STORE_DIR`. `store put` writes `STORE_DIR/<input-stem>.voldoc`; `store account`/
  `store gc` read store-backed roots in place.
- **No size or win is claimed here.** Whether object-granularity sharing beats
  per-file LZ and generic content-defined-chunk dedup is the Phase 9.3 cohort
  measurement; the expected negative (generic chunking may capture the same
  sharing) is pre-registered in the store contract and will be reported, not
  hidden.

## References

- `research/subagents/phase-09/store-contract.md` (frozen design and protocol)
- `docs/phases/phase-09-plan.md`
- `src/store/{mod.rs,embedded.rs,entropyfs.rs,account.rs}`,
  `src/container/{descriptor.rs,header.rs}`, `src/materialize/mod.rs`,
  `src/main.rs`
- ADR-0008 (EntropyFS is optional), ADR-0017 (generic lossless baselines),
  ADR-0019 (seek-based I/O)
