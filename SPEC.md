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
| `0x60` | `CHECKPOINT` | reserved (later) |
| `0x70` | `INDEX` | reserved (later) |
| `0x80` | `EXTERNAL_REF` | reserved (Phase 9+) |
| `0xF0` | `INTEGRITY` | `sha256:[u8;32]`, `source_len:u64` |
| `0xFF` | `TRAILER` | `record_count:u32`, `payload_bytes:u64`, `magic:[u8;8]` |

Rule: **unknown record whose `flags` lacks `FLAG_OPTIONAL` (`0x01`) fails closed**
with `UnsupportedFeature`. Unknown explicitly-optional records are skipped.

Requirements enforced by `Descriptor::parse`:

- exactly one `UNIVERSE`, `FORMAT`, `GRAPH`, `INTEGRITY`, `TRAILER`;
- `SHA-256(universe)[..16] == header.universe_id`;
- `FORMAT.source_format == header.source_format`;
- `INTEGRITY.source_len == header.declared_source_len`;
- `TRAILER.record_count` equals the number of records actually read;
- no record after `TRAILER`.

## Graph (reconstruction program)

```text
graph := version:u8=2 op_count:u32 op*
op    := EMIT_OBJECT(0x01) u32_object_id
       | INLINE(0x02)      u32_len [u8;len]
       | REPEAT_LAST(0x03) u32_count
       | DECODE_CHANNEL(0x04) u32_channel_id
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

The Phase-3 universe string is:

```text
vole-document;universe;phase-3;exact-bytes;dra-2;opaque+entropy+pdf
```

The header's `universe_id` is the first 16 bytes of `SHA-256` over this string.
Any change to an opcode, coder, limit semantic, adapter meaning, or hash semantic
requires a new universe string. This supersedes the Phase-2 string
(`vole-document;universe;phase-2;exact-bytes;dra-2;opaque+entropy`).

## Entropy records (Phase 2)

Phase 2 introduces exactly two entropy records and one graph op. They are
implemented and measured but remain **PROVISIONAL** (the wire format is not
frozen v1).

### `MODEL` (`0x30`)

A canonical, self-describing frequency table over the fixed 256-symbol byte
alphabet. The payload is the dense canonical model, exactly **516 bytes**:

```text
model := version:u8=1 scale_bits:u8 count:u16=256 freq:[u16;256]
```

- `freq` entries are little-endian and **must sum to exactly `1 << scale_bits`**.
- `scale_bits` is in `1..=15` (frequencies are stored as `u16`).
- Normalization from observed counts is integer-only, deterministic, and
  tie-broken by lower symbol index; symbols seen zero times get frequency zero.
- A channel's `scale_bits` must equal the `scale_bits` of the model it names, and
  a channel may not name a missing model; both are checked during `parse`.

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
are all required to reconstruct bytes (see [`docs/adr/0006-rans-substrate.md`](docs/adr/0006-rans-substrate.md)).

## Feature policy

- `default = ["rans"]`: the native scalar entropy decoder (`ryg-rans-rs`
  `=0.5.1`, **safe manual** API only) is present by default.
- Built with `--no-default-features`, the exact RAW/RLE floor still compiles and
  materializes channel-free descriptors byte-for-byte.
- A descriptor that declares `MODEL`/`ENTROPY_CHANNEL` records but is decoded
  without the `rans` feature returns an explicit `UnsupportedFeature`
  (`ErrorClass` exit code 6). It is never silently reinterpreted and never
  partially materialized.

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
