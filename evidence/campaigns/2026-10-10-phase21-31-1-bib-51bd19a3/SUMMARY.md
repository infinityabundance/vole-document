# Phase 21.31.1 — bibliography adapter court

**Question.** Do bibliographies — BibTeX, RIS, and CSL-JSON — close exactly and
expose a representation-preserving model (the recorded dialect; each entry/item;
each field's exact value span with the BibTeX brace/quoted/number/macro form kept
distinct and duplicate fields reported; the RIS two-space separator and a
multi-line continuation; each CSL-JSON item's string `id` and `type`) on top of
the whole-source exact leaf, while keeping a conservative content-only detection
boundary (a plain JSON array stays `json`, and prose stays `opaque`)?

**Method.** Each self-authored fixture is generated in-court with GNU
`/usr/bin/printf`, ingested via `field-build`, observed, then — after the
**source file and the standalone descriptor are deleted** — rematerialized
exactly in a fresh process and compared with `length + SHA-256 + cmp`. An
out-of-range entry, an absent field/key, and an unsupported common pair are
required to decline typed; the plain-JSON-array and prose controls pin the
detection boundaries. The court runs in the pinned `dev` service using only
POSIX `sh`, coreutils, git, and the shipped binary (no python3, no jq).

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | sha256 match | cmp | exact | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: |
| `refs.bib` | bib | 158 | 158 | true | true | true | -1 |
| `refs.ris` | bib | 82 | 82 | true | true | true | -1 |
| `refs.json` | bib | 112 | 112 | true | true | true | -1 |
| `array.json` | json | 18 | 18 | true | true | true | 6 |
| `prose.txt` | opaque | 103 | 103 | true | true | true | 6 |

## Counts

```
fixtures 5
bib_fixtures 3
control_fixtures 2
exact_ok 5
exact_fail 0
surface_fail 0
boundary_ok 2
opaque_control_ok 1
json_control_ok 1
typed_declines_ok 1
binary bae2ee1cb3c56b663918ddae8fd97a0eb2fa0c78c29944b63d3f877438f3dae4
```

## Per-fixture observation files (raw/)

- `array.json.bib-decline.json`
- `array.json.metadata.json`
- `build.log`
- `prose.txt.metadata.json`
- `refs.bib.common.search.json`
- `refs.bib.dialect.json`
- `refs.bib.entry0.dup.json`
- `refs.bib.entry0.json`
- `refs.bib.field.bin`
- `refs.bib.find.json`
- `refs.bib.key.json`
- `refs.bib.metadata.json`
- `refs.bib.text.json`
- `refs.json.common.search.json`
- `refs.json.dialect.json`
- `refs.json.entry0.json`
- `refs.json.field.bin`
- `refs.json.find.json`
- `refs.json.key.json`
- `refs.json.metadata.json`
- `refs.json.text.json`
- `refs.ris.common.search.json`
- `refs.ris.dialect.json`
- `refs.ris.entry0.json`
- `refs.ris.field.bin`
- `refs.ris.find.json`
- `refs.ris.metadata.json`
- `refs.ris.text.json`
- `results.tsv`

## Scope (honest)

- **Shipped here:** content-only, conservative bibliography detection (three
  recorded dialects); the bounded, span-preserving entry/field/item model over the
  reused JSON parser; native `bib-dialect`/`bib-entry`/`bib-field`/`bib-key`/
  `bib-find`; common `metadata`/`text`/`search-match`.
- **Precedence:** bibliography detection is tried **before** the generic JSON
  detector, so a CSL-JSON array is never stolen by JSON.
- **Boundary (honest):** a JSON array of objects that each carry a string `id` and
  a string `type` but are not bibliographic is indistinguishable from CSL-JSON and
  stays claimed as `csl_json`; a single GeoJSON Feature/FeatureCollection object is
  claimed by GeoJSON first. A BibTeX macro is never resolved against `@string`.
