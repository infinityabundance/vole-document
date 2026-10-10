# Phase 21.20.1 — config family court

**Question.** Does the config family (INI / `.env` / Java `.properties`) close
exactly and expose a representation-preserving key/value model (exact spans,
line order, the `export` marker and quoting, `properties` continuations,
`\uXXXX` spelling, and duplicate-key reports) on top of the whole-source exact
leaf — while keeping the conservative no-magic-byte detection boundary and
declining malformed/ambiguous inputs typed?

**Method.** Each self-authored fixture is generated in-court with GNU
`/usr/bin/printf`, ingested via `field-build`, observed, then — after the
**source file and the standalone descriptor are deleted** — rematerialized
exactly in a fresh process and compared with `length + SHA-256 + cmp`. An
out-of-range line/entry and an unknown section are required to decline typed;
the TOML/JSON/CSV controls, the pure `KEY=VALUE` overlap, prose, a `#!`
script, and a sectioned-but-malformed blob pin the detection boundaries. The
court runs in the pinned `dev` service using only POSIX `sh`, coreutils,
git, and the shipped binary (no python3, no jq).

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `ini.ini` | config | 44 | 44 | true | true | true | 6 | -1 |
| `env.env` | config | 38 | 38 | true | true | true | 6 | -1 |
| `props.properties` | config | 51 | 51 | true | true | true | 6 | -1 |
| `dup.ini` | config | 16 | 16 | true | true | true | 6 | -1 |
| `strict.json` | json | 21 | 21 | true | true | true | -1 | 6 |
| `table.csv` | csv | 8 | 8 | true | true | true | -1 | 6 |
| `doc.toml` | toml | 12 | 12 | true | true | true | -1 | 6 |
| `overlap.env` | opaque | 16 | 16 | true | true | true | -1 | 6 |
| `prose.txt` | opaque | 70 | 70 | true | true | true | -1 | 6 |
| `script.sh` | opaque | 25 | 25 | true | true | true | -1 | 6 |
| `badblock.ini` | opaque | 25 | 25 | true | true | true | -1 | 6 |

## Counts

```
fixtures 11
config_fixtures 4
exact_ok 11
exact_fail 0
surface_fail 0
typed_declines_ok 4
cross_format_controls_ok 3
usage_ok 1
opaque_controls_ok 4
opaque_controls 4
binary eb7c9dc84921ba19096dde6a4a8b116f49ce12e13dd0d8928343b0886d764151
```

## Per-fixture observation files (raw/)

- `badblock.ini.metadata.json`
- `doc.toml.config-decline.json`
- `dup.ini.e0.json`
- `dup.ini.metadata.json`
- `dup.ini.search.json`
- `dup.ini.text.json`
- `env.env.e1.json`
- `env.env.e1.txt.json`
- `env.env.metadata.json`
- `env.env.search.json`
- `env.env.text.json`
- `ini.ini.find.json`
- `ini.ini.line1.json`
- `ini.ini.metadata.json`
- `ini.ini.search.json`
- `ini.ini.section.json`
- `ini.ini.text.json`
- `overlap.env.metadata.json`
- `props.properties.e1.json`
- `props.properties.e2.json`
- `props.properties.metadata.json`
- `props.properties.search.json`
- `props.properties.text.json`
- `prose.txt.metadata.json`
- `results.tsv`
- `script.sh.metadata.json`
- `strict.json.config-decline.json`
- `table.csv.config-decline.json`

## Scope (honest)

- **Shipped here:** byte-based conservative config detection; one bounded
  parser covering INI, `.env`, and Java `.properties` with a recorded dialect;
  native `config-line`/`config-entry`/`config-section`/`config-find`; common
  `metadata`/`text`/`search-match`.
- **Precedence:** after the strong magic-byte binaries, the JSON family,
  CBOR/MessagePack, and TOML; before CSV/Markdown/XML/HTML. A source that is
  valid TOML is very often syntactically valid INI, so TOML wins (an
  INI-shaped TOML stays `Toml`).
- **Detection boundary:** the config family has **no magic bytes**. INI needs a
  `[section]` header; `properties` needs a strong `=`/`:` separator plus a
  properties-only construct; `env` needs an `export ` prefix. A plain-prose/
  `.txt`/Markdown/code blob and a pure `KEY=VALUE`/`#`-comment/`=`-only file stay
  `Opaque`.
- **Recorded negative (the overlap):** a file that is all `KEY=VALUE` lines,
  `=`-only, `#`-only comments, identifier keys, is byte-for-byte the same shape
  under `env` and Java `properties`. It is **not guessed** — it stays Opaque —
  and is claimed as a dialect only on a dialect-only signal (`export ` for env;
  `:`/whitespace/`!`/`\uXXXX`/continuation/non-identifier key for properties).
- **Declines:** a line that is not a valid construct of the detected dialect, a
  line/entry/section addressed out of range, a too-deep continuation, and a cap
  breach are typed (`InvalidConfigStructure` rc 34, unsupported-feature rc 6,
  or resource-limit rc 8); such input stays `Opaque` when detection declines.
- **Exactness** is the whole source (a RAW-like `DocumentExact`): the config
  model is derived (`Q_gen`) and never on the exactness path (ADR-0060: the
  model node depends on the `sha256(source)` root).
- **Duplicate keys** are preserved and reported (never collapsed, never a
  decline): the representation-preserving policy.
- **Not claimed here:** the economic court (separate script).
- **Never run on the host:** every command above ran in the pinned `dev`
  container.
