# `.voldoc` wire format (provisional)

> **Status: PROVISIONAL, not frozen v1.** The layout below is implemented and
> tested, but it is not a stability commitment. Version identity is carried both
> in the header and in the universe declaration string, and the universe string
> changes whenever any opcode, coder, limit semantic, adapter meaning, or hash
> semantic changes. See `PROJECT_STATE.md` and `docs/adr/` for the freeze policy.

All multi-byte integers are little-endian. Offsets are byte offsets from the
start of the file.

## File

```text
file    := header record*
header  := 64 bytes (fixed, see below)
record  := tag:u8 flags:u8 reserved:u16=0 length:u32 payload:[u8;length] crc32c:u32
```

`crc32c` is CRC-32C (Castagnoli) over the 8-byte record header *and* the payload.
`reserved` must be zero. The final record is always `TRAILER`.

## Header (64 bytes)

| Offset | Len | Field | Notes |
|---|---|---|---|
| 0 | 8 | `magic` | `56 4F 4C 44 4F 43 1A 00` = ASCII `VOLDOC` + `0x1A` + `0x00` (source of truth: `MAGIC`) |
| 8 | 2 | `major` | format major version (this build: `0`) |
| 10 | 2 | `minor` | format minor version (this build: `1`) |
| 12 | 4 | `mandatory_features` | unknown bits fail closed |
| 16 | 4 | `optional_features` | may be ignored |
| 20 | 1 | `exactness_profile` | `0 = EXACT_BYTES` (only normative value) |
| 21 | 1 | `source_format` | `0 = OPAQUE`, `1 = PDF` (Phase 3); unknown fails closed |
| 22 | 2 | `reserved_a` | must be `0` |
| 24 | 16 | `universe_id` | first 16 bytes of `SHA-256(universe_string)` |
| 40 | 8 | `declared_source_len` | exact reconstructed length |
| 48 | 12 | `reserved_b` | must be `0` |
| 60 | 4 | `header_crc32c` | CRC-32C over bytes `[0,60)` |

The magic constant is defined once in `src/container/header.rs` (`MAGIC`); this
document defers to that source of truth.

## Records

| Tag | Name | Payload |
|---|---|---|
| `0x01` | `UNIVERSE` | UTF-8 universe declaration string |
| `0x02` | `FORMAT` | `source_format:u8`, `basis_len:u32`, `basis:[u8]` |
| `0x10` | `OBJECT` | raw object bytes |
| `0x20` | `GRAPH` | encoded reconstruction program |
| `0x30` | `MODEL` | canonical dense entropy model (see [Entropy records](#entropy-records-phase-2)) |
| `0x40` | `ENTROPY_CHANNEL` | typed channel capsule: 33-byte header + renorm payload |
| `0x50` | `RESIDUAL` | reserved (later) |
| `0x60` | `CHECKPOINT` | optional, advisory per-op output-boundary table, bound to the GRAPH record (Phase 13.4) |
| `0x70` | `OBSERVATION_INDEX` | optional, advisory op/selector/digest map (Phase 7.3) |
| `0x80` | `EXTERNAL_REF` | mandatory-when-present: `id:[u8;32]`, `len:u64` of an object held by an `ObjectStore` (Phase 9) |
| `0xF0` | `INTEGRITY` | `sha256:[u8;32]`, `source_len:u64` |
| `0xFF` | `TRAILER` | `record_count:u32`, `payload_bytes:u64`, `magic:[u8;8]` |

Rule: **unknown record whose `flags` lacks `FLAG_OPTIONAL` (`0x01`) fails closed**
with `UnsupportedFeature`. Unknown explicitly-optional records are skipped.

Requirements enforced by `Descriptor::parse`:

- exactly one `UNIVERSE`, `FORMAT`, `GRAPH`, `INTEGRITY`, `TRAILER`;
- at most one `OBSERVATION_INDEX`;
- every object-table entry appears in order: an `OBJECT`/`EXTERNAL_REF` is an
  entry, and its **position** is its `object_id` (there is no explicit id on the
  wire). Exactly one of `OBJECT`/`EXTERNAL_REF` is written per entry;
- a descriptor with at least one `EXTERNAL_REF` declares the mandatory
  `FEATURE_EXTERNAL_OBJECTS` bit; a decoder without `store` support fails closed
  at header validation;
- `SHA-256(universe)[..16] == header.universe_id`;
- `FORMAT.source_format == header.source_format`;
- `INTEGRITY.source_len == header.declared_source_len`;
- `TRAILER.record_count` equals the number of records actually read;
- no record after `TRAILER`;
- if an `OBSERVATION_INDEX` is present, every claim it makes is re-derived from
  the program and any disagreement is rejected with `CoverageViolation`. The
  index is **never authority**;
- at most one `CHECKPOINT`; it must carry `FLAG_OPTIONAL`, must be located by a
  `DIRECTORY` (so `serialize` rejects a checkpoint without a seek directory), and
  every boundary it declares is re-derived from the program and rejected on any
  disagreement (`CoverageViolation`/`InvalidContainer`). The checkpoint is
  **never authority**.

### `OBSERVATION_INDEX` (`0x70`, optional, Phase 7.3)

An advisory, checked map from reconstruction output to instructions, PDF
selectors, and output-block digests. It is written with `FLAG_OPTIONAL`; a
decoder that ignores it still materializes the source **byte-for-byte**, because
the reconstruction program alone is complete. It carries no authority and
cannot change reconstructed bytes.

```text
observation_index_v1 :=
    version:u8 = 1
    section_flags:u8            # bit0 OP_TABLE, bit1 PDF_SELECTORS, bit2 DIGESTS
    if bit0: op_count:u32, op_entry[op_count]
    if bit1: selector_count:u32, selector[selector_count]
    if bit2: digest_count:u32, digest[digest_count]

op_entry := out_len:u32 | dep_kind:u8 | dep_id:u32
selector := kind:u8 | number:u32 | generation:u32 | out_off:u64 | out_len:u64
digest   := out_off:u64 | out_len:u64 | sha256:[u8;32]
```

All integers are little-endian. `dep_kind` is `0` none / `1` object / `2`
channel; `kind` is `0` object / `1` encoded stream / `2` revision. The header
advertises an optional feature bit `FEATURE_OBSERVATION_INDEX` (`1 << 0`) when
the record is present. Validation re-derives each `out_len` via `analyze_ops`,
checks each dependency id is in range, and requires every selector and digest
range to lie within `[0, total)`. Sections are independent and unknown
`section_flags` bits or an unknown `version` fail closed.

### `CHECKPOINT` (`0x60`, optional, Phase 13.4)

An advisory, checked per-op output-boundary table. It is written with
`FLAG_OPTIONAL`; a decoder that ignores it still materializes the source
**byte-for-byte**, because the reconstruction program alone is complete. It
carries no authority and cannot change reconstructed bytes. A checkpoint is
locatable only through a `DIRECTORY` (its `CLASS_INDEX` gains the `CHECKPOINT`
class), so it requires a seek directory; the header advertises the optional bit
`FEATURE_CHECKPOINTS` (`1 << 2`) when the record is present.

```text
checkpoint_v1 :=
    version:u8 = 1
    kind:u8 = 1                 # OP_BOUNDARIES
    reserved:u16 = 0
    entry_count:u32
    source_len:u64
    graph_crc32c:u32            # CRC-32C of the GRAPH record payload
    entry[entry_count]          # out_start:u64 | out_len:u64
```

All integers are little-endian. The entries are the per-op output spans in
program order. Validation requires `entry_count == program.ops.len()`, every
`out_len` to equal `analyze_ops`, the spans to be contiguous and to cover
`source_len` exactly, and `graph_crc32c` to match the `GRAPH` record; a
contradiction is rejected. The seek reader may consume a validated checkpoint in
place of the `OBSERVATION_INDEX` for a raw byte range and otherwise falls back to
the index lane (ADR-0039 — the mechanism is byte-exact but a recorded negative).

### `EXTERNAL_REF` (`0x80`, mandatory-when-present, Phase 9)

The descriptor's object table is a single ordered sequence. Each entry is either
an inline `OBJECT` (`0x10`) record or an `EXTERNAL_REF` (`0x80`) record that names
an object held by an external content-addressed store. The entry's **position** in
the table is its `object_id`, exactly as for `OBJECT`; there is no explicit id on
the wire, so the DRA's `u32_object_id` indexes the table unchanged.

```text
EXTERNAL_REF (0x80) := id:[u8;32] | len:u64LE        # exactly 40 bytes
```

`id` is `BLAKE3-256` of the object's **raw** (uncompressed) bytes; `len` is the
exact byte length. This is the store namespace, **not** the archival identity: the
whole reconstructed source is still identified by the `INTEGRITY` SHA-256, and an
`id` never appears in `INTEGRITY`.

* **Parsing never needs the store.** `len` supplies the object length for the
  coverage certificate without resolving anything; a descriptor can be parsed
  with no I/O and no `blake3`.
* **The bit is mandatory, not optional.** A descriptor with at least one
  `EXTERNAL_REF` sets `FEATURE_EXTERNAL_OBJECTS` (`1 << 1`) in the header. Because
  an external reference is load-bearing, a decoder built without the `store`
  feature fails closed with `UnsupportedFeature` at header validation rather than
  materializing a partial document.
* **Resolution.** `materialize` resolves each reference through an
  `ObjectStore`, which MUST re-hash the returned bytes and reject with
  `IntegrityMismatch` unless `BLAKE3(bytes) == id` and the length matches `len`.
  The `INTEGRITY` SHA-256 of the whole source remains the archival backstop. The
  DRA is unchanged: it still consumes a plain vector of object bytes.
* **Two forms, one materialization.** Standalone (all `OBJECT`) and store-backed
  (all `EXTERNAL_REF`) descriptors of the same source materialize identical
  bytes; conversion is provided by `externalize` (inline → external) and
  `hydrate` (external → inline). Unlike `OBJECT`, the cost of an `EXTERNAL_REF` is
  charged to `CostBreakdown::external_refs` (40 B payload; framing to
  `record_framing`).

The in-memory model mirrors this one ordered table:

```rust
enum ObjectSource {
    Inline(Vec<u8>),
    External { id: Id, len: u64 },
}
```

`ObjectSource::len()` returns the length available to the coverage certificate
without resolving (`len` for `External`); `Id` is a newtype over the 32 raw
`BLAKE3-256` bytes with lower-case hex rendering.

### Mandatory feature bits

| Bit | Name | Meaning |
|---|---|---|
| `1 << 0` | `FEATURE_DEFLATE_REPLAY` | the program contains a `DEFLATE_REPLAY` op |
| `1 << 1` | `FEATURE_EXTERNAL_OBJECTS` | the object table contains at least one `EXTERNAL_REF` |

Unknown mandatory bits fail closed at header validation with
`UnsupportedFeature`; the bits this build supports depend on its cargo features
(`deflate-replay`, `store`).

## Graph (reconstruction program)

```text
graph := version:u8=7 op_count:u32 op*
op    := EMIT_OBJECT(0x01)        u32_object_id
       | INLINE(0x02)             u32_len [u8;len]
       | REPEAT_LAST(0x03)        u32_count
       | DECODE_CHANNEL(0x04)     u32_channel_id
       | INTERLEAVE_CHANNELS(0x05) u32_kinds_channel u32_lengths_channel \
                                 u32_first_payload_channel u8_payload_channel_count
       | MARK_OFFSET(0x06)        u8_slot
       | EMIT_OFFSET(0x07)        u8_slot u8_width
       | PACK_SEGMENTS(0x08)       u32_data_object u32_item_count item*
       | PACKED_CHANNELS(0x09)     u32_data_channel u32_plan_channel u64_declared_output_len
       | DEFLATE_REPLAY(0x0A)     u8_source_kind u32_source_id u32_corrections_object \
                                 u32_declared_output_len

item  := LITERAL(0x01)  u32_leb128_len
       | MARK(0x02)      u8_slot
       | EMIT(0x03)      u8_slot u8_width
```

Semantics:

- `EMIT_OBJECT` appends the referenced object's bytes (authority: **Literal**).
- `INLINE` appends inline bytes (authority: **Literal**).
- `REPEAT_LAST` repeats the bytes produced by the *immediately preceding literal
  instruction* `count` more times (authority: **Generated**). Consecutive
  `REPEAT_LAST` and a leading `REPEAT_LAST` are invalid.
- `DECODE_CHANNEL` appends the exact decoded bytes of the referenced entropy
  channel (authority: **EntropyChannel**). The channel's decoded length is known
  statically from its descriptor, so the op's output length is still bounded at
  parse time.
- `INTERLEAVE_CHANNELS` (introduced in DRA version 3) reconstructs a byte string
  from a **kind channel**, a **length channel**, and a contiguous run of
  **payload channels** (authority: **EntropyChannel**):

  - `kinds_channel` holds one kind byte per token, in file order;
  - `lengths_channel` holds one little-endian `u32` per token, aligned with the
    kind stream (its decoded length must be exactly `4 * token_count`);
  - payload channel `first_payload_channel + k` carries the concatenated bytes
    of every token whose kind is `k`, in file order, for
    `k in 0..payload_channel_count`.

  Evaluation walks the kind/length sequence, and for each token appends the next
  `length` bytes of the payload channel named by its kind, advancing a per-kind
  cursor. All indices, the kind range, and cursor bounds are validated before
  allocation; a kind outside the payload range, misaligned kind/length counts,
  a length overrun, or unconsumed payload bytes is rejected as
  `InvalidGraph`/`CoverageViolation`. The op is bounded and non-Turing-complete
  like the rest of the DRA.
- `MARK_OFFSET` (introduced in DRA version 4) records the current output position
  (a `u64`) into the named slot and emits **no** bytes (authority: **Generated**,
  zero-length). `slot` is a `u8`, so every value `0..=255` is in range; the
  program models [`MAX_OFFSET_SLOTS`](../../src/dra/program.rs) = **256** slots. Slot
  `255` is **reserved for the most recent classic `xref` section start**; slots
  `0..=254` are available to the layout builder for indirect-object introducer
  offsets. Marking a slot does not disturb the pending `REPEAT_LAST` block.
- `EMIT_OFFSET` (introduced in DRA version 4) emits the decimal form of the
  position most recently recorded in `slot`, left zero-padded with ASCII `0` to
  exactly `width` bytes (authority: **Generated**). `width` is in `1..=20`. The
  value is decoded only at materialization time, so analysis charges exactly
  `width` output bytes; at materialization a value that needs more than `width`
  digits is rejected as `InvalidGraph`. A slot must have been marked by an
  earlier `MARK_OFFSET` in program order, or the op is rejected during analysis
  (before allocation) with `InvalidGraph`. Prediction is deterministic and never
  invents bytes: the layout builder emits an `EMIT_OFFSET` only when it has
  verified that the marked position reproduces the source digits, and otherwise
  falls back to a literal `INLINE`.
- `PACK_SEGMENTS` (introduced in DRA version 5) reconstructs output from a
  **compact item table over one data object** (authority: **Literal** for
  `Literal` items, **Generated** for `Mark`/`Emit`), amortizing per-segment op
  framing. `data_object` is an index into the descriptor's object table; the
  table holds exactly `item_count` items:

  - `LITERAL(0x01) { len }` copies the next `len` contiguous bytes of the data
    object and advances the data cursor; `len` is a `u32` LEB128 varint;
  - `MARK(0x02) { slot }` records the current output position (a `u64`) into
    `slot` and emits no bytes, exactly like `MARK_OFFSET`;
  - `EMIT(0x03) { slot, width }` emits the marked decimal position, left
    zero-padded to `width` bytes, exactly like `EMIT_OFFSET`.

  The item table is bounded by the same limits as the graph and is validated
  before allocation: an unknown item tag, a truncation, a slot that was never
  marked, a `width` beyond `1..=20`, a literal run that overruns the data object,
  or a data object not consumed **exactly** is rejected (`InvalidGraph` /
  `CoverageViolation`). Like every other op it is deterministic and
  non-Turing-complete, and it never invents bytes: an `Emit` is only present when
  the builder verified the marked position reproduces the source digits.
- `PACKED_CHANNELS` (introduced in DRA version 6) reconstructs output from a
  **data entropy channel** interpreted by a serialized item table carried in a
  **plan entropy channel** (authority: **EntropyChannel** for both).
  `data_channel` is the index of the channel holding the literal data object;
  `plan_channel` is the index of the channel holding exactly
  [`encode_items`](../../src/dra/op.rs) of the item table (the same
  `Literal`/`Mark`/`Emit` item codec as `PACK_SEGMENTS`); `declared_output_len`
  is the exact expected output length. Evaluation decodes both channels, runs the
  item table over the data (the data object must be consumed **exactly**), and
  rejects a produced length other than `declared_output_len`, a missing channel, an
  unknown item tag, a truncation, an unmarked slot, a bad width, a literal overrun,
  or unconsumed data (`InvalidGraph` / `CoverageViolation`). Like every other op it
  is deterministic and non-Turing-complete, and it never invents bytes.
- `DEFLATE_REPLAY` (introduced in DRA version 7; the `replay_codec` tag added in
  DRA version 8) begins with a `replay_codec: u8` that names the exact
  reconstruction semantics bound to the `corrections` blob. The only value this
  build implements is `REPLAY_DEFLATE_PREFLATE_0_7_6` (`1`), which binds the blob
  to the **experimental, version-coupled** `preflate` 0.7.6 bitcode+CABAC layout;
  any other value fails closed with `UnsupportedFeature` (never `InvalidGraph`),
  so the private representation can never be silently promoted to a stable
  standard. The op then emits exactly
  `recreate_whole_deflate_stream(plaintext, corrections)` — the **raw** DEFLATE
  bitstream (RFC 1951, with no zlib header and no Adler-32 trailer) — so it
  reconstructs *producer* entropy-coded bytes rather than storing them literally
  (authority: **Generated**, reprising the original producer coding).
  `source_kind` selects where the plaintext comes from: `0` = the `source_id`-th
  **object**; `1` = the `source_id`-th **entropy channel** (decoded exactly as for
  `DECODE_CHANNEL`). `corrections_object` indexes the descriptor's object table;
  `declared_output_len` is the exact expected output length. Analysis charges it
  statically and rejects a value above the VOLE replay-profile admission limit
  `min(max_output_bytes, max_replay_bytes, 2*P + 1024)` for a `P`-byte plaintext
  (a policy bound: RFC 1951 gives no finite `f(decompressed_size)` bound, since
  arbitrarily many empty non-final blocks are legal) *before* the replay engine
  runs; evaluation additionally bounds the
  plaintext and corrections inputs by `max_record_len`. Reconstruction is isolated
  with `catch_unwind`: an out-of-range source, an unknown `source_kind`, a wrong
  `declared_output_len`, or an `Err`/panic from the replay engine is rejected with
  a typed `CodecReplay`/`InvalidGraph` — never a panic or a silent reconstruction.
  A successful reconstruction is still accepted only by the enclosing whole-source
  SHA-256 court, and the encoder emits the op only after the replay reproduced the
  exact source stream bytes. The op is deterministic and non-Turing-complete, and a
  descriptor containing it declares the mandatory `FEATURE_DEFLATE_REPLAY` bit.

## Coverage certificate (checked invariant, not stored bytes)

The coverage map is **derived** deterministically from the program and object
lengths during `parse`, and validated before any byte materialization:

```text
union(spans) == [0, declared_source_len)
spans are contiguous (no gaps, no overlapping authorities)
```

A descriptor whose predicted length disagrees with `declared_source_len`, or
whose spans are not contiguous, is rejected with `CoverageViolation` **before**
allocation. This catches the dangerous "the parser forgot a source distinction"
class of bugs at the representation boundary.

## Universe declaration

The Phase-9 universe string is:

```text
vole-document;universe;phase9;exact-bytes;dra-8;opaque+entropy+pdf+channels+offsets+packed+packed-channels+deflate-replay-preflate-0.7.6-experimental+observation-index-v1+seek-directory-v1+external-objects-v1
```

The header's `universe_id` is the first 16 bytes of `SHA-256` over this string.
Any change to an opcode, coder, limit semantic, adapter meaning, or hash semantic
(including adding an optional record meaning) requires a new universe string.
Phase 9 re-bases the prefix to `phase9` and appends `+external-objects-v1` for the
store-backed object form (`EXTERNAL_REF`); `dra-8` is unchanged and `FORMAT_MINOR`
does not move (the mandatory feature bit carries fail-closed compatibility).
This supersedes the Phase-8 string
(`vole-document;universe;phase8;exact-bytes;dra-8;opaque+entropy+pdf+channels+offsets+packed+packed-channels+deflate-replay-preflate-0.7.6-experimental+observation-index-v1+seek-directory-v1`),
which superseded the Phase-7 string
(`vole-document;universe;phase7;exact-bytes;dra-8;opaque+entropy+pdf+channels+offsets+packed+packed-channels+deflate-replay-preflate-0.7.6-experimental+observation-index-v1`),
which superseded the Phase-6 string
(`vole-document;universe;phase6;exact-bytes;dra-8;opaque+entropy+pdf+channels+offsets+packed+packed-channels+deflate-replay-preflate-0.7.6-experimental`),
which superseded the Phase-5.8 string
(`vole-document;universe;phase5-8;exact-bytes;dra-6;opaque+entropy+pdf+channels+offsets+packed+packed-channels`),
which superseded the Phase-5.7 (Phase-6
preparation) string
(`vole-document;universe;phase6-prep;exact-bytes;dra-5;opaque+entropy+pdf+channels+offsets`),
which superseded the Phase-5 string
(`vole-document;universe;phase-5;exact-bytes;dra-4;opaque+entropy+pdf+channels+offsets`),
which superseded the Phase-4 string
(`vole-document;universe;phase-4;exact-bytes;dra-3;opaque+entropy+pdf+channels`).

**Dated note (Phase 11/12, 2026-10-06).** This `.voldoc` *descriptor* universe is
unchanged through Phase 12 — `dra-8` and `FORMAT_MINOR` do not move and the wire
stays byte-compatible. The Phase-11 persistent field and the Phase-12 multi-format
package field carry their **own**, separate universe strings
(`…;phase11;…+procedural-seed-field-v1+hier-index-v1` and `…+package-v1`; see
`src/field/mod.rs`), so the descriptor universe string should not be read as
tracking the release number.

## Entropy records (Phase 2, extended in Phase 4)

Phase 2 introduces two entropy records and one graph op (`MODEL`,
`ENTROPY_CHANNEL`, `DECODE_CHANNEL`). Phase 4 extends the `MODEL` wire to
version 2 (sparse/dense) and adds the `INTERLEAVE_CHANNELS` op and the typed
PDF-channel candidate layout. All of it is implemented and measured but remains
**PROVISIONAL** (the wire format is not frozen v1).

### `MODEL` (`0x30`)

A canonical, self-describing frequency table over the fixed 256-symbol byte
alphabet. Two wire versions are accepted on decode; **version 2** is what this
build emits:

```text
model_v2 := version:u8=2 form:u8 scale_bits:u8 payload
form = 0 (SPARSE): present_count:u16 (symbol:u8 freq:u16)*
form = 1 (DENSE):  count:u16=256    freq:[u16;256]

model_v1 := version:u8=1 scale_bits:u8 count:u16=256 freq:[u16;256]   (legacy, dense)
```

- **v2 form selection.** The encoder serializes to whichever form is *strictly*
  smaller; on a tie it picks the dense form, so the mapping from model to bytes
  stays deterministic. Sparse symbols must be strictly ascending and unique with
  `freq >= 1`; the dense form carries all 256 little-endian `u16` frequencies.
- **Legacy v1.** The original dense payload (`[1][scale_bits][count=256][u16 x
  256]`, exactly 516 bytes) is still decodable, so older descriptors remain
  readable.
- `freq` entries are little-endian and **must sum to exactly `1 << scale_bits`**
  in both versions; trailing or truncated payloads are rejected.
- `scale_bits` is in `1..=15` (frequencies are stored as `u16`).
- Normalization from observed counts is integer-only, deterministic, and
  tie-broken by lower symbol index; symbols seen zero times get frequency zero.
- A channel's `scale_bits` must equal the `scale_bits` of the model it names, and
  a channel may not name a missing model; both are checked during `parse`.

### PDF typed-channel candidate layout (Phase 4)

The `PDF_CHANNELS` candidate (`source_format = 1`) transposes the Phase-3 lexical
cover into a fixed set of channels — the indices below are a wire contract of
this candidate. With `KIND_COUNT = 12`:

```text
channel 0            kinds: one kind byte per token, in file order
channel 1            lengths: four little-endian bytes per token, aligned with kinds
channels 2..2+KIND_COUNT   payloads[k]: bytes of every token of kind k, in file order
                           (with KIND_COUNT = 12 this is channels 2..13)
```

Each channel is independently order-0 byte-rANS coded with its own `MODEL`
(a model id per channel, in that order), and reconstruction is a single
`INTERLEAVE_CHANNELS` op with `kinds_channel = 0`, `lengths_channel = 1`,
`first_payload_channel = 2`, `payload_channel_count = KIND_COUNT`. Every model
and payload byte is charged in the complete-cost court; the candidate is
**proposed and measured but not adopted** — it loses to `BYTE_RANS` on this
corpus (see `PROJECT_STATE.md`, ADR-0010). This section is **PROVISIONAL**.

### `ENTROPY_CHANNEL` (`0x40`)

One typed channel capsule. The payload is a fixed **33-byte header** followed by
the renormalization payload; the total record payload length must equal
`33 + payload_len` exactly (no trailing bytes, no truncation).

| Offset | Len | Field | Notes |
|---|---|---|---|
| 0 | 1 | `coder` | `1 = CODER_ORDER0_BYTE_RANS` |
| 1 | 2 | `coder_version` | `1` |
| 3 | 1 | `scale_bits` | must match the referenced model |
| 4 | 1 | `lane_count` | `1` (single-lane only; else `UnsupportedFeature`) |
| 5 | 4 | `model_id` | index into the descriptor's `MODEL` table |
| 9 | 8 | `symbol_count` | number of symbols encoded |
| 17 | 8 | `decoded_length` | exact decoded bytes |
| 25 | 4 | `initial_state` | scalar decoder entry state |
| 29 | 4 | `payload_len` | length of the following payload |
| 33 | `payload_len` | `payload` | renormalization bytes in forward decoder-consumption order |

A channel is never a bare seed: the model, decoder state, payload, and counts
are all required to reconstruct bytes (see [`docs/adr/0006-rans-substrate.md`](../adr/0006-rans-substrate.md)).

## PDF layout candidate (Phase 5, rebuilt on packed framing in Phase 5.7, entropy-coded in Phase 5.8)

The `PDF_LAYOUT` candidate (`source_format = 1`) applies only to PDFs with a
classic cross-reference section and **no** cross-reference stream, and with at
most 255 indirect objects; anything else declines. Since Phase 5.7 it persists
`objects = [data]` (one packed data object), `models` and `channels` empty, and
reconstructs entirely from **one `PACK_SEGMENTS` op** whose item table interleaves
literals and predictions:

- every literal byte is appended to the single data object and referenced by a
  `LITERAL { len }` item, with adjacent literal runs **coalesced** into one item;
- a `MARK { slot }` (slot = the object's index in the physical object table)
  before each indirect object's introducer bytes, and a `MARK { slot: 255 }` at
  the xref section start;
- an `EMIT { slot, width: 10 }` (followed by a `LITERAL` for the generation/status
  field) for each xref entry whose 10-digit offset equals the marked offset of its
  target object, and a literal item otherwise;
- an `EMIT { slot: 255, width }` for the `startxref` value when the marked xref
  start reproduces it, and a literal fallback otherwise.

This replaced the per-segment `MARK_OFFSET`/`EMIT_OFFSET` program of the original
Phase-5 lane (kept in the DRA as `0x06`/`0x07` but no longer emitted by this
candidate) to amortize framing. The candidate is byte-exact by construction (the
builder verifies serialize → parse → materialize → byte-compare and declines on
any mismatch) and its analysis-only metadata (`pdf-layout;objects=…;
xref_predicted=…;xref_literal=…;startxref_predicted=…`) is deterministic.

It is **recorded, not adopted**. Packed framing plus coalescing makes it **beat
RAW at scale** (`many.pdf` 10,069 vs 10,215), which the per-segment lane never did,
but it is still dominated by `BYTE_RANS` (5,181) because the residual data object
is stored literally; layout wins 0 of the 8 classic-xref samples and the
leave-one-out layout delta is 0 (campaign `2026-10-05-phase5-4521778`). See
`PROJECT_STATE.md` and ADR-0012.

The Phase-5.8 variant `PDF_LAYOUT_RANS` keeps the same `LayoutPlan` but carries it
through the `PACKED_CHANNELS` op (`0x09`): channel 0 holds the plan's literal data
object and channel 1 holds `encode_items` of the item table, each order-0
byte-rANS coded with its own `MODEL` (model ids 0 and 1, `scale_bits` 12). It is
likewise byte-exact and is recorded, not adopted — head-to-head against
`BYTE_RANS` it wins 0, loses 8, and is declined by 3 files, because channel 0
codes nearly the whole file while the plan channel (1,815 B on `many.pdf`) and the
second model are added metadata `BYTE_RANS` never pays (campaign
`2026-10-05-phase5-8-cf8048d`, ADR-0013). This section is **PROVISIONAL**.

## PDF DEFLATE replay candidate (Phase 6)

The `DEFLATE_REPLAY` op (`0x0A`, DRA v8, with the `replay_codec` tag
`REPLAY_DEFLATE_PREFLATE_0_7_6`) reconstructs the **original** DEFLATE bitstream
of a producer-entropy-coded stream from `(plaintext, corrections)`. The
correction blob is an **experimental**, version-coupled `preflate` 0.7.6
representation, not a frozen v1 format; the codec tag makes that explicit and an
unknown tag fails closed. The
stream discovery is VOLE's own: the byte-authoritative physical scanner finds each
stream span and classifies its `/Filter`, and only a stream whose data begins at an
opaque `stream`+EOL span and whose dictionary is a lone `/FlateDecode` (a bare name
or a single-element array) is eligible. `preflate` never discovers streams. For an
eligible zlib stream the 2-byte header and 4-byte Adler-32 trailer are stripped and
re-emitted as literal bytes, and only the raw DEFLATE middle is replayed. The
encoder requires `recreate_whole_deflate_stream(...)` to reproduce the exact source
span bytes before admitting the stream to the complete-cost court; analysis and
reconstruction are both bounded and `catch_unwind`-isolated.

Two candidates use the op:

- `PDF_DEFLATE_REPLAY` (physical span order): each eligible stream span becomes
  `INLINE(zlib header) · DEFLATE_REPLAY · INLINE(Adler-32)`, with plaintexts and
  correction blobs stored as content-deduplicated `OBJECT`s and all other spans
  left literal.
- `PDF_DEFLATE_REPLAY_RANS`: identical, except each **unique** plaintext is coded
  as its own order-0 byte-rANS `ENTROPY_CHANNEL` and referenced (shared) by every
  stream that produces it; the materializer decodes each shared channel once.

Both are byte-exact by construction (the builder gates the candidate on
serialize → parse → materialize → byte-compare) and are proposed only for inputs
with at least one eligible lone-`FlateDecode` stream. On the measured
`2026-10-05-phase6-0d0bb79` campaign the rANS variant is the first PDF structural
candidate to **beat `BYTE_RANS`** (`flate.pdf` 36,102 vs 49,291 B, a 13,189 B win,
because 6 streams share only 3 unique plaintexts), while the raw-plaintext
variant loses (56,736 B). The result is scoped to one composed sample at commit
`0d0bb79`: the winning region is a shared plaintext that *also* has a
large/weakly-coded appearance (neither sharing alone nor weak coding alone wins),
and the losing region is unique, strongly-compressed plaintext (ADR-0015). This
section is **PROVISIONAL**.

## Feature policy

- `default = ["rans", "store", "field"]`: the native scalar entropy decoder
  (`ryg-rans-rs` `=0.5.1`, **safe manual** API only), the content-addressed
  object store (`Id = BLAKE3-256`, `blake3` `=1.8.7`) and the Phase-11 persistent
  field are present by default. (This bullet previously read `["rans", "store"]`;
  `field` was added in Phase 11.) The default build is **permissive-only** and
  pulls no copyleft dependency.
- The exact DEFLATE replay engine (`preflate-rs` `=0.7.6`) is **opt-in** via
  `--features deflate-replay` (or `--all-features`); it transitively pulls the
  `cabac` crate, licensed LGPL-3.0-or-later (ADR-0014).
- The optional `EntropyFsStore` adapter is **opt-in and never default** via
  `--features entropyfs-store` (which implies `store`). It pulls the embeddable
  EntropyFS engine (`entropyfs` `=0.7.17`, `default-features = false`) and hence a
  non-optional `dsfb` and a large dependency tree. It is a backend choice only:
  the standalone form and the reference `EmbeddedStore` do not need it (ADR-0008,
  ADR-0020).
- Built with `--no-default-features`, the exact RAW/RLE floor still compiles and
  materializes channel-free descriptors byte-for-byte.
- A descriptor that declares `MODEL`/`ENTROPY_CHANNEL` records but is decoded
  without the `rans` feature returns an explicit `UnsupportedFeature`
  (`ErrorClass` exit code 6). Likewise a descriptor whose program contains
  `DEFLATE_REPLAY` declares the mandatory `FEATURE_DEFLATE_REPLAY` bit, and decoding
  it without the `deflate-replay` feature returns `UnsupportedFeature`. Neither is
  ever silently reinterpreted or partially materialized.
- A store-backed descriptor (any `EXTERNAL_REF`) declares the mandatory
  `FEATURE_EXTERNAL_OBJECTS` bit; decoding it without the `store` feature returns
  `UnsupportedFeature` (exit 6) at header validation, never a partial document.
- The `deflate-replay` feature transitively pulls the `cabac` crate, licensed
  LGPL-3.0-or-later; see ADR-0014. Build with
  `--no-default-features --features rans` for an artifact without it.

## Canonical descriptor encoding

The *source document* is never canonicalized. The *descriptor* is canonical: a
given `Descriptor` always serializes to the same bytes, and
`serialize(parse(x)) == x` holds for every descriptor this implementation
produces (verified by the conformance court). This makes receipts and hashes
stable across runs and implementations.

## Integrity levels

| Level | Mechanism |
|---|---|
| framing | per-record CRC-32C; header CRC-32C |
| whole source | `INTEGRITY` SHA-256 of the reconstructed bytes |

A deep verify (`vole-document verify`) materializes the source and checks the
whole-source digest. During development, `byte_compare` is the court authority.

## PDF source format (Phase 3)

The header's `source_format` selector gains a second normative value:

```text
source_format := 0 = OPAQUE
               | 1 = PDF    (Phase 3)
```

An unknown class still fails closed with `UnsupportedFeature`; it is never
silently reinterpreted as opaque.

A PDF descriptor (`source_format = 1`) is produced by the byte-authoritative
physical scanner. The scanner lexes the input into a contiguous span cover and
classifies each span structurally (`%PDF-` header, `obj`/`endobj`,
`stream`/`endstream`, `xref`, `trailer`, `startxref`, `%%EOF`, comments,
whitespace, and raw stream data), resolves direct/indirect `/Length`, and builds
an append-only revision map delimited by `%%EOF` with `/Prev` links.

The Phase-3 candidate persists the exact physical partition as **literal-span DRA
ops**: one `INLINE` instruction per physical span, in ascending offset order,
with no structural compression. Because the cover is contiguous and
non-overlapping, concatenating those spans reconstructs the source byte-for-byte
by construction. The per-span **kinds are deterministic analysis metadata** —
`scan` can recompute them at any time, they are not stored as trusted semantics,
and the literal bytes are the authority.

`PDF_PHYSICAL` competes in the complete-cost court like every other candidate and
currently loses to RAW (the expected Phase-3 outcome; structural compression is
Phase 5+). This section remains **PROVISIONAL**; the wire layout is not frozen
v1.
