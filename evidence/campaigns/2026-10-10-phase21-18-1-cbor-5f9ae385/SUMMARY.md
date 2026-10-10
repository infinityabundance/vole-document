# Phase 21.18.1 — CBOR (RFC 8949) court

**Question.** Does the binary structured-tree CBOR format close exactly and
expose a representation-preserving model (every major type, the encoding width
actually used, byte-vs-text strings, tag numbers never resolved, map order and
duplicate keys, float width, and definite/indefinite-length items) on top of the
whole-source exact leaf — while keeping the conservative no-magic-byte detection
boundary and declining malformed/ambiguous inputs typed?

**Method.** Each self-authored fixture is generated in-court with GNU
`/usr/bin/printf`, ingested via `field-build`, observed, then — after the
**source file and the standalone descriptor are deleted** — rematerialized
exactly in a fresh process and compared with `length + SHA-256 + cmp`. An
out-of-range pointer is required to decline typed; the strict-JSON,
lone-scalar, truncated, map-key-without-value, unterminated, MessagePack, and
prose controls pin the detection boundaries. The court runs in the pinned
`dev` service using only POSIX `sh`, coreutils, git, and the shipped binary
(no python3, no jq).

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `basic.cbor` | cbor | 12 | 12 | true | true | true | 6 | -1 |
| `widths.cbor` | cbor | 4 | 4 | true | true | true | 6 | -1 |
| `floats.cbor` | cbor | 18 | 18 | true | true | true | 6 | -1 |
| `tags.cbor` | cbor | 12 | 12 | true | true | true | 6 | -1 |
| `bytestext.cbor` | cbor | 9 | 9 | true | true | true | 6 | -1 |
| `dupkeys.cbor` | cbor | 7 | 7 | true | true | true | 6 | -1 |
| `indef.cbor` | cbor | 13 | 13 | true | true | true | 6 | -1 |
| `large.cbor` | cbor | 4003 | 4003 | true | true | true | 6 | -1 |
| `strict.json` | json | 21 | 21 | true | true | true | -1 | 6 |
| `single.cbor` | opaque | 1 | 1 | true | true | true | -1 | 6 |
| `scalar.cbor` | opaque | 2 | 2 | true | true | true | -1 | 6 |
| `badmap.cbor` | opaque | 4 | 4 | true | true | true | -1 | 6 |
| `unterm.cbor` | opaque | 4 | 4 | true | true | true | -1 | 6 |
| `msgpack_fixarray.bin` | opaque | 4 | 4 | true | true | true | -1 | 6 |
| `msgpack_fixmap.bin` | opaque | 5 | 5 | true | true | true | -1 | 6 |
| `prose.txt` | opaque | 68 | 68 | true | true | true | -1 | 6 |

## Counts

```
fixtures 16
normal 8
exact_ok 16
exact_fail 0
surface_fail 0
typed_declines_ok 8
json_control_ok 1
usage_ok 1
opaque_controls_ok 7
opaque_controls 7
binary 322678b8e53960cc751059fc283b5d5e73f439c12152c58127cb8190c1d1060a
```

## Per-fixture observation files (raw/)

- `badmap.cbor.metadata.json`
- `basic.cbor.find.json`
- `basic.cbor.metadata.json`
- `basic.cbor.node.json`
- `basic.cbor.ptr.bin`
- `basic.cbor.ptr.json`
- `basic.cbor.text.json`
- `bytestext.cbor.find.json`
- `bytestext.cbor.metadata.json`
- `bytestext.cbor.node.json`
- `bytestext.cbor.p0.json`
- `bytestext.cbor.p1.json`
- `bytestext.cbor.ptr.bin`
- `bytestext.cbor.ptr.json`
- `bytestext.cbor.text.json`
- `dupkeys.cbor.find.json`
- `dupkeys.cbor.metadata.json`
- `dupkeys.cbor.node.json`
- `dupkeys.cbor.ptr.bin`
- `dupkeys.cbor.ptr.json`
- `dupkeys.cbor.text.json`
- `floats.cbor.find.json`
- `floats.cbor.metadata.json`
- `floats.cbor.node.json`
- `floats.cbor.p0.json`
- `floats.cbor.p2.json`
- `floats.cbor.ptr.bin`
- `floats.cbor.ptr.json`
- `floats.cbor.text.json`
- `indef.cbor.find.json`
- `indef.cbor.metadata.json`
- `indef.cbor.node.json`
- `indef.cbor.ptr.bin`
- `indef.cbor.ptr.json`
- `indef.cbor.text.json`
- `large.cbor.find.json`
- `large.cbor.metadata.json`
- `large.cbor.node.json`
- `large.cbor.ptr.bin`
- `large.cbor.ptr.json`
- `large.cbor.text.json`
- `msgpack_fixarray.bin.metadata.json`
- `msgpack_fixmap.bin.metadata.json`
- `prose.txt.metadata.json`
- `results.tsv`
- `scalar.cbor.metadata.json`
- `single.cbor.metadata.json`
- `strict.json.cbor-decline.json`
- `tags.cbor.find.json`
- `tags.cbor.metadata.json`
- `tags.cbor.node.json`
- `tags.cbor.ptr.bin`
- `tags.cbor.ptr.json`
- `tags.cbor.text.json`
- `unterm.cbor.metadata.json`
- `widths.cbor.find.json`
- `widths.cbor.metadata.json`
- `widths.cbor.node.json`
- `widths.cbor.p0.json`
- `widths.cbor.p1.json`
- `widths.cbor.ptr.bin`
- `widths.cbor.ptr.json`
- `widths.cbor.text.json`

## Scope (honest)

- **Shipped here:** byte-based conservative CBOR detection; the bounded CBOR
  parser producing a representation-preserving arena (kind, exact span, encoding
  width, tag number, float width, definite/indefinite form, ordered children);
  native `cbor-pointer`/`cbor-node`/`cbor-find`; common
  `metadata`/`text`/`search-match`.
- **Precedence:** after the strong magic-byte binaries (PDF/ZIP/Parquet/Arrow)
  and the JSON family (JSON/JSON5/JSONL), before the textual heuristics.
- **Detection boundary:** CBOR has **no magic bytes**. An input is claimed only
  on the self-described-CBOR tag `55799`, or a full-input well-formed parse
  whose root is a container/tag reaching at least three nodes; a container head
  byte is always `>= 0x80`, so no pure-ASCII document is ever claimed. A lone
  scalar, a truncated item, a map key with no value, an unterminated item, a
  MessagePack source, and prose all stay `Opaque` rather than being guessed.
- **Recorded negative:** the whole-number/short-container prefix overlaps
  MessagePack's `fixint`/`fixarray` encodings; the two cannot always be told
  apart, so ambiguous inputs are not guessed. A Phase-21.19 MessagePack adapter
  must share this seam.
- **Declines:** a malformed head/value/map/string and an out-of-range pointer
  are typed (`InvalidCborStructure` rc 32, or unsupported-feature rc 6); such
  input stays `Opaque`.
- **Exactness** is the whole source (a RAW-like `DocumentExact`): the CBOR
  model is derived (`Q_gen`) and never on the exactness path (ADR-0060: the
  model node depends on the `sha256(source)` root).
- **Not claimed here:** the economic court (separate script).
- **Never run on the host:** every command above ran in the pinned `dev`
  container.
