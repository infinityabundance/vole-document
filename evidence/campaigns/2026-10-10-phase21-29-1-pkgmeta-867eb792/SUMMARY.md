# Phase 21.29.1 — package-metadata adapter court

**Question.** Do package manifests — an npm `package.json` / `package-lock.json`,
a Cargo `Cargo.toml` / `Cargo.lock`, and a Python `pyproject.toml` — close exactly
and expose a representation-preserving section/entry model (the recorded dialect,
every section's exact name/value spans, and every key/value entry's exact spans,
with member order, duplicate keys, numeric/string spelling, and inline-table /
array-valued dependency specs preserved verbatim) on top of the whole-source exact
leaf, while keeping a conservative content-only detection boundary (a generic JSON
`name`+`version` stays `json`, a generic TOML `name`/`version` stays `toml`, and
prose stays `opaque`)?

**Method.** Each self-authored fixture is generated in-court with GNU
`/usr/bin/printf`, ingested via `field-build`, observed, then — after the
**source file and the standalone descriptor are deleted** — rematerialized
exactly in a fresh process and compared with `length + SHA-256 + cmp`. An
out-of-range section/entry, an absent key, and an unsupported common pair are
required to decline typed; a malformed native key reference is a usage error; the
generic-JSON, generic-TOML, and prose controls pin the detection boundaries. The
court runs in the pinned `dev` service using only POSIX `sh`, coreutils, git, and
the shipped binary (no python3, no jq).

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | sha256 match | cmp | exact | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: |
| `package.json` | pkgmeta | 156 | 156 | true | true | true | -1 |
| `Cargo.toml` | pkgmeta | 136 | 136 | true | true | true | -1 |
| `pyproject.toml` | pkgmeta | 159 | 159 | true | true | true | -1 |
| `Cargo.lock` | pkgmeta | 82 | 82 | true | true | true | -1 |
| `package-lock.json` | pkgmeta | 146 | 146 | true | true | true | -1 |
| `generic.json` | json | 31 | 31 | true | true | true | 6 |
| `generic.toml` | toml | 29 | 29 | true | true | true | 6 |
| `prose.txt` | opaque | 99 | 99 | true | true | true | 6 |

## Counts

```
fixtures 8
pkgmeta_fixtures 5
control_fixtures 3
exact_ok 8
exact_fail 0
surface_fail 0
boundary_ok 3
opaque_control_ok 1
json_control_ok 1
toml_control_ok 1
typed_declines_ok 1
usage_ok 1
binary c0ea1fed83c89746deb75f377ed93b5c946a73bef23fe4d8689e85e3050d9ee7
```

## Per-fixture observation files (raw/)

- `Cargo.lock.common.search.json`
- `Cargo.lock.entry0.bin`
- `Cargo.lock.find.json`
- `Cargo.lock.metadata.json`
- `Cargo.lock.section0.json`
- `Cargo.lock.text.json`
- `Cargo.toml.common.search.json`
- `Cargo.toml.find.json`
- `Cargo.toml.metadata.json`
- `Cargo.toml.section0.json`
- `Cargo.toml.serde.bin`
- `Cargo.toml.text.json`
- `Cargo.toml.version.bin`
- `build.log`
- `generic.json.metadata.json`
- `generic.json.pkgmeta-decline.json`
- `generic.toml.metadata.json`
- `generic.toml.pkgmeta-decline.json`
- `package-lock.json.common.search.json`
- `package-lock.json.find.json`
- `package-lock.json.metadata.json`
- `package-lock.json.section0.json`
- `package-lock.json.text.json`
- `package.json.common.search.json`
- `package.json.dep.bin`
- `package.json.find.json`
- `package.json.metadata.json`
- `package.json.script.json`
- `package.json.section0.json`
- `package.json.section2.json`
- `package.json.text.json`
- `prose.txt.metadata.json`
- `pyproject.toml.common.search.json`
- `pyproject.toml.find.json`
- `pyproject.toml.metadata.json`
- `pyproject.toml.name.bin`
- `pyproject.toml.section0.json`
- `pyproject.toml.text.json`
- `results.tsv`

## Scope (honest)

- **Shipped here:** content-only, conservative manifest detection (five recorded
  dialects); the bounded, span-preserving section/entry model over the reused JSON
  and TOML parsers; native `pkgmeta-section`/`pkgmeta-entry`/`pkgmeta-key`/
  `pkgmeta-find`; common `metadata`/`text`/`search-match`.
- **Precedence:** pkgmeta is tried **before** the generic JSON detector (and
  therefore before the generic TOML detector), so a plain JSON/TOML value is never
  stolen.
- **Boundary (honest):** a generic JSON `name`+`version` with fewer than two
  package-specific keys is indistinguishable from a minimal `package.json` and
  stays `json`; a generic TOML `name`/`version` with no `[package]` table stays
  `toml`; a virtual Cargo workspace manifest carrying only `[workspace]` stays
  `toml`. Detection never consults a file name.
- **Exactness** is the whole source (a RAW-like `DocumentExact`): the manifest
  model is derived (`Q_gen`) and never on the exactness path (ADR-0060: the model
  node depends on the `sha256(source)` root). Nothing is normalized or
  re-serialized.
- **Not claimed here:** an economic court, a package resolver, or any rendering.
- **Never run on the host:** every command above ran in the pinned `dev`
  container.
