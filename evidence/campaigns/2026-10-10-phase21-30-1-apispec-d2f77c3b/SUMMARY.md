# Phase 21.30.1 — API/specification adapter court

**Question.** Do API specifications — a JSON Schema, an OpenAPI 3.x document,
a Swagger 2.0 document, and an AsyncAPI document — close exactly and expose a
representation-preserving model (the recorded dialect, the exact spec-version
string, every object with its role and exact spans, and every `$ref` preserved
verbatim and never resolved) on top of the whole-source exact leaf, while keeping
a conservative content-only detection boundary (a plain JSON object stays `json`,
a JSON-Schema-shaped object with no `$schema` stays `json`, and prose stays
`opaque`)?

**Method.** Each self-authored fixture is generated in-court with GNU
`/usr/bin/printf`, ingested via `field-build`, observed, then — after the
**source file and the standalone descriptor are deleted** — rematerialized
exactly in a fresh process and compared with `length + SHA-256 + cmp`. An
out-of-range object/`$ref` and an unsupported common pair are required to
decline typed; the generic-JSON, JSON-Schema-shaped, and prose controls pin the
detection boundaries. The court runs in the pinned `dev` service using only
POSIX `sh`, coreutils, git, and the shipped binary (no python3, no jq).

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | sha256 match | cmp | exact | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: |
| `schema.json` | apispec | 273 | 273 | true | true | true | -1 |
| `openapi.json` | apispec | 200 | 200 | true | true | true | -1 |
| `swagger.json` | apispec | 186 | 186 | true | true | true | -1 |
| `asyncapi.json` | apispec | 201 | 201 | true | true | true | -1 |
| `generic.json` | json | 31 | 31 | true | true | true | 6 |
| `shape.json` | json | 55 | 55 | true | true | true | 6 |
| `prose.txt` | opaque | 104 | 104 | true | true | true | 6 |

## Counts

```
fixtures 7
apispec_fixtures 4
control_fixtures 3
exact_ok 7
exact_fail 0
surface_fail 0
boundary_ok 3
opaque_control_ok 1
json_control_ok 2
typed_declines_ok 1
binary c11c8edebf47b32a0ea17af08fe3f3d71d8b67e058430d59086fa6c778e1c9a0
```

## Per-fixture observation files (raw/)

- `asyncapi.json.common.search.json`
- `asyncapi.json.dialect.json`
- `asyncapi.json.find.json`
- `asyncapi.json.metadata.json`
- `asyncapi.json.object0.json`
- `asyncapi.json.text.json`
- `build.log`
- `generic.json.apispec-decline.json`
- `generic.json.metadata.json`
- `openapi.json.common.search.json`
- `openapi.json.dialect.json`
- `openapi.json.find.json`
- `openapi.json.metadata.json`
- `openapi.json.object0.json`
- `openapi.json.ref0.bin`
- `openapi.json.text.json`
- `openapi.json.version.bin`
- `prose.txt.metadata.json`
- `results.tsv`
- `schema.json.common.search.json`
- `schema.json.dialect.json`
- `schema.json.find.json`
- `schema.json.metadata.json`
- `schema.json.object0.json`
- `schema.json.ref0.bin`
- `schema.json.text.json`
- `schema.json.version.json`
- `shape.json.apispec-decline.json`
- `shape.json.metadata.json`
- `swagger.json.common.search.json`
- `swagger.json.dialect.json`
- `swagger.json.find.json`
- `swagger.json.metadata.json`
- `swagger.json.object0.json`
- `swagger.json.text.json`
- `swagger.json.version.json`

## Scope (honest)

- **Shipped here:** content-only, conservative API-spec detection (four recorded
  dialects); the bounded, span-preserving object/member/`$ref` model over the
  reused JSON parser; native `apispec-dialect`/`apispec-version`/`apispec-object`/
  `apispec-ref`/`apispec-find`; common `metadata`/`text`/`search-match`.
- **Precedence:** apispec is tried **before** the generic JSON detector, so a
  plain JSON value is never stolen.
- **Boundary (honest):** a JSON-Schema-*shaped* object with no `$schema` is
  indistinguishable from a plain JSON tree and stays `json`; a plain JSON object
  that merely has a `properties`/`paths`/`components` key without the marker also
  stays `json`. Detection never consults a file name.
- **Refs:** `$ref` targets are preserved verbatim and are **never** resolved,
  dereferenced, or fetched (dangling/external/cyclic references are recorded as
  written).
- **Exactness** is the whole source (a RAW-like `DocumentExact`): the API-spec
  model is derived (`Q_gen`) and never on the exactness path (ADR-0060: the model
  node depends on the `sha256(source)` root). Nothing is normalized, resolved, or
  re-serialized.
- **Not claimed here:** a schema validator, a `$ref` resolver, or any rendering.
- **Never run on the host:** every command above ran in the pinned `dev`
  container.
