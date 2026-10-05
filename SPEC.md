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
| 21 | 1 | `source_format` | `0 = OPAQUE` |
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
| `0x30` | `MODEL` | reserved (Phase 2+) |
| `0x40` | `ENTROPY_CHANNEL` | reserved (Phase 2+) |
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
graph := version:u8=1 op_count:u32 op*
op    := EMIT_OBJECT(0x01) u32_object_id
       | INLINE(0x02)      u32_len [u8;len]
       | REPEAT_LAST(0x03) u32_count
```

Semantics:

- `EMIT_OBJECT` appends the referenced object's bytes (authority: **Literal**).
- `INLINE` appends inline bytes (authority: **Literal**).
- `REPEAT_LAST` repeats the bytes produced by the *immediately preceding literal
  instruction* `count` more times (authority: **Generated**). Consecutive
  `REPEAT_LAST` and a leading `REPEAT_LAST` are invalid.

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
