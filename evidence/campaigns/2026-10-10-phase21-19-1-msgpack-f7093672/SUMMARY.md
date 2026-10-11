# Phase 21.19.1 — MessagePack court

**Question.** Does the binary structured-tree MessagePack format close exactly
and expose a representation-preserving model (the exact format byte actually
used — encoding width AND signedness, `str` vs `bin`, map order and duplicate
keys, float width, and extension type/length) on top of the whole-source exact
leaf — while keeping the conservative no-magic-byte detection boundary,
coexisting honestly with CBOR, and declining malformed/ambiguous inputs typed?

**Method.** Each self-authored fixture is generated in-court with GNU
`/usr/bin/printf`, ingested via `field-build`, observed, then — after the
**source file and the standalone descriptor are deleted** — rematerialized
exactly in a fresh process and compared with `length + SHA-256 + cmp`. An
out-of-range pointer is required to decline typed; the strict-JSON, **CBOR**
(coexistence), lone-scalar, 0xc1-byte, map-key-without-value, trailing-bytes,
ambiguous fixarray(3)/fixmap(2), and prose controls pin the detection
boundaries. The court runs in the pinned `dev` service using only POSIX `sh`,
coreutils, git, and the shipped binary (no python3, no jq).

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `basic.msgpack` | msgpack | 10 | 10 | true | true | true | 6 | -1 |
| `widths.msgpack` | msgpack | 8 | 8 | true | true | true | 6 | -1 |
| `floats.msgpack` | msgpack | 15 | 15 | true | true | true | 6 | -1 |
| `bytestext.msgpack` | msgpack | 10 | 10 | true | true | true | 6 | -1 |
| `dupkeys.msgpack` | msgpack | 10 | 10 | true | true | true | 6 | -1 |
| `ext.msgpack` | msgpack | 12 | 12 | true | true | true | 6 | -1 |
| `map16.msgpack` | msgpack | 6 | 6 | true | true | true | 6 | -1 |
| `strict.json` | json | 21 | 21 | true | true | true | -1 | 6 |
| `control.cbor` | cbor | 12 | 12 | true | true | true | -1 | 6 |
| `single.msgpack` | opaque | 1 | 1 | true | true | true | -1 | 6 |
| `scalar.msgpack` | opaque | 2 | 2 | true | true | true | -1 | 6 |
| `badc1.msgpack` | opaque | 1 | 1 | true | true | true | -1 | 6 |
| `badmap.msgpack` | opaque | 3 | 3 | true | true | true | -1 | 6 |
| `trailing.msgpack` | opaque | 2 | 2 | true | true | true | -1 | 6 |
| `fixarray3.bin` | opaque | 4 | 4 | true | true | true | -1 | 6 |
| `fixmap2.bin` | opaque | 5 | 5 | true | true | true | -1 | 6 |
| `prose.txt` | opaque | 75 | 75 | true | true | true | -1 | 6 |

## Counts

```
fixtures 17
normal 7
exact_ok 17
exact_fail 0
surface_fail 0
typed_declines_ok 7
json_control_ok 1
cbor_control_ok 1
usage_ok 1
opaque_controls_ok 8
opaque_controls 8
binary 188daf1d0619ec783a9472580f90354fcec94d13c651b3e92e21fa9d08e65acc
```

## Per-fixture observation files (raw/)

- `badc1.msgpack.metadata.json`
- `badmap.msgpack.metadata.json`
- `basic.msgpack.find.json`
- `basic.msgpack.metadata.json`
- `basic.msgpack.node.json`
- `basic.msgpack.ptr.bin`
- `basic.msgpack.ptr.json`
- `basic.msgpack.text.json`
- `bytestext.msgpack.find.json`
- `bytestext.msgpack.metadata.json`
- `bytestext.msgpack.node.json`
- `bytestext.msgpack.p0.json`
- `bytestext.msgpack.p1.json`
- `bytestext.msgpack.ptr.bin`
- `bytestext.msgpack.ptr.json`
- `bytestext.msgpack.text.json`
- `control.cbor.msgpack-decline.json`
- `dupkeys.msgpack.find.json`
- `dupkeys.msgpack.metadata.json`
- `dupkeys.msgpack.node.json`
- `dupkeys.msgpack.ptr.bin`
- `dupkeys.msgpack.ptr.json`
- `dupkeys.msgpack.text.json`
- `ext.msgpack.find.json`
- `ext.msgpack.metadata.json`
- `ext.msgpack.node.json`
- `ext.msgpack.ptr.bin`
- `ext.msgpack.ptr.json`
- `ext.msgpack.text.json`
- `fixarray3.bin.metadata.json`
- `fixmap2.bin.metadata.json`
- `floats.msgpack.find.json`
- `floats.msgpack.metadata.json`
- `floats.msgpack.node.json`
- `floats.msgpack.p0.json`
- `floats.msgpack.p1.json`
- `floats.msgpack.ptr.bin`
- `floats.msgpack.ptr.json`
- `floats.msgpack.text.json`
- `map16.msgpack.find.json`
- `map16.msgpack.metadata.json`
- `map16.msgpack.node.json`
- `map16.msgpack.ptr.bin`
- `map16.msgpack.ptr.json`
- `map16.msgpack.text.json`
- `prose.txt.metadata.json`
- `results.tsv`
- `scalar.msgpack.metadata.json`
- `single.msgpack.metadata.json`
- `strict.json.msgpack-decline.json`
- `trailing.msgpack.metadata.json`
- `widths.msgpack.find.json`
- `widths.msgpack.metadata.json`
- `widths.msgpack.node.json`
- `widths.msgpack.p0.json`
- `widths.msgpack.p1.json`
- `widths.msgpack.ptr.bin`
- `widths.msgpack.ptr.json`
- `widths.msgpack.text.json`

## Scope (honest)

- **Shipped here:** byte-based conservative MessagePack detection; the bounded
  MessagePack parser producing a representation-preserving arena (kind, exact
  span, exact format byte, extension type, ordered children); native
  `msgpack-pointer`/`msgpack-node`/`msgpack-find`; common
  `metadata`/`text`/`search-match`.
- **Precedence:** after the strong magic-byte binaries (PDF/ZIP/Parquet/Arrow),
  the JSON family (JSON/JSON5/JSONL), and CBOR; before the textual heuristics.
- **Detection boundary:** MessagePack has **no magic bytes**. An input is claimed
  only on a full-input well-formed parse whose root is a container reaching at
  least three items and eight bytes, or the same with an unambiguous
  MessagePack-only head byte (`0xdc..=0xdf`, which CBOR rejects). A container
  head byte is always `>= 0x80`, so no pure-ASCII document is ever claimed. A
  lone scalar, the `0xc1` byte, a map key with no value, trailing bytes, the
  ambiguous fixarray(3)/fixmap(2) sources, and prose all stay `Opaque` rather
  than being guessed.
- **Coexistence:** CBOR is tried **before** MessagePack (its self-described tag
  is the strongest binary signal), so a CBOR document stays `Cbor` and is never
  stolen. The two encodings cannot always be told apart for a source well-formed
  under both grammars; the ordering makes that an honest `Cbor` classification,
  never a MessagePack guess.
- **Recorded negative:** the whole-number/short-container prefix overlaps
  MessagePack's `fixint`/short containers; ambiguous inputs are not guessed.
- **Declines:** a malformed head (the never-used `0xc1`), a truncated item,
  trailing bytes, a map key with no value, an over-long declared length, a
  non-UTF-8 `str`, and an out-of-range pointer are typed
  (`InvalidMsgpackStructure` rc 33, or unsupported-feature rc 6); such input
  stays `Opaque`.
- **Exactness** is the whole source (a RAW-like `DocumentExact`): the MessagePack
  model is derived (`Q_gen`) and never on the exactness path (ADR-0060: the
  model node depends on the `sha256(source)` root).
- **Not claimed here:** the economic court (separate script).
- **Never run on the host:** every command above ran in the pinned `dev`
  container.
