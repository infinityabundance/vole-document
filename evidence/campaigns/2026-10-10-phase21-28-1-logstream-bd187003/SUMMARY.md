# Phase 21.28.1 — syslog / log-stream adapter court

**Question.** Does a log stream — RFC 5424 syslog, RFC 3164 (BSD) syslog, or a
generic application log line — close exactly and expose a representation-
preserving per-record model (each record's recorded dialect, exact line span and
terminator, decoded priority, deepest structured-data nesting, and every field's
exact span, with messages/NILVALUE/timestamp/level spelling preserved) on top of
the whole-source exact leaf, while keeping a conservative detection boundary
(prose stays `opaque`, a JSON value stays `json`, a JSONL stream stays
`jsonl`, and a pure BSD `key: value` stream honestly stays `yaml`)?

**Method.** Each self-authored fixture is generated in-court with GNU
`/usr/bin/printf`, ingested via `field-build`, observed, then — after the
**source file and the standalone descriptor are deleted** — rematerialized
exactly in a fresh process and compared with `length + SHA-256 + cmp`. An
out-of-range record and an unsupported common pair are required to decline typed;
a malformed native field reference is a usage error; the prose, single-JSON,
JSONL, and BSD-no-colon controls pin the detection boundaries. The court runs in
the pinned `dev` service using only POSIX `sh`, coreutils, git, and the shipped
binary (no python3, no jq).

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | sha256 match | cmp | exact | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: |
| `generic.log` | logstream | 99 | 99 | true | true | true | -1 |
| `rfc5424.log` | logstream | 143 | 143 | true | true | true | -1 |
| `rfc3164.log` | logstream | 92 | 92 | true | true | true | -1 |
| `crlf.log` | logstream | 26 | 26 | true | true | true | -1 |
| `blank.log` | logstream | 25 | 25 | true | true | true | -1 |
| `prose.txt` | opaque | 94 | 94 | true | true | true | 6 |
| `single.json` | json | 8 | 8 | true | true | true | -1 |
| `stream.jsonl` | jsonl | 16 | 16 | true | true | true | 6 |
| `bsd_nocolon.log` | yaml | 74 | 74 | true | true | true | -1 |

## Counts

```
fixtures 9
logstream_fixtures 5
control_fixtures 4
exact_ok 9
exact_fail 0
surface_fail 0
boundary_ok 4
opaque_controls_ok 1
jsonl_control_ok 1
typed_declines_ok 1
usage_ok 1
binary 0cd3d729a9401c007927b6dcc639f4356cb345dd7b6b892b524b046afe5b7a54
```

## Per-fixture observation files (raw/)

- `blank.log.common.search.json`
- `blank.log.find.json`
- `blank.log.line.bin`
- `blank.log.line.json`
- `blank.log.metadata.json`
- `blank.log.text.json`
- `bsd_nocolon.log.metadata.json`
- `build.log`
- `crlf.log.common.search.json`
- `crlf.log.find.json`
- `crlf.log.line.bin`
- `crlf.log.line.json`
- `crlf.log.metadata.json`
- `crlf.log.text.json`
- `generic.log.common.search.json`
- `generic.log.find.json`
- `generic.log.level.bin`
- `generic.log.line.bin`
- `generic.log.line.json`
- `generic.log.metadata.json`
- `generic.log.text.json`
- `prose.txt.metadata.json`
- `results.tsv`
- `rfc3164.log.common.search.json`
- `rfc3164.log.find.json`
- `rfc3164.log.line.bin`
- `rfc3164.log.line.json`
- `rfc3164.log.metadata.json`
- `rfc3164.log.pid.bin`
- `rfc3164.log.text.json`
- `rfc5424.log.common.search.json`
- `rfc5424.log.find.json`
- `rfc5424.log.line.bin`
- `rfc5424.log.line.json`
- `rfc5424.log.metadata.json`
- `rfc5424.log.sd.bin`
- `rfc5424.log.text.json`
- `single.json.metadata.json`
- `stream.jsonl.logstream-decline.json`
- `stream.jsonl.metadata.json`

## Scope (honest)

- **Shipped here:** byte-based, conservative log-stream detection (three
  dialects); the bounded, span-preserving per-record model; native
  `logstream-line`/`logstream-field`/`logstream-find`; common `metadata`/
  `text`/`search-match`.
- **Precedence:** logstream is tried **after** every structured/tabular/prose
  format (so JSON/JSONL/YAML/TOML/CSV/config/prose are never stolen) and
  **before** the maximally ambiguous fixed-width heuristic.
- **Boundary (honest):** a prose file whose *every* line begins with a level
  word is indistinguishable from a log and is claimed; a single log-looking line
  inside prose is not claimed; and a pure BSD stream whose every message lacks a
  `: ` is a YAML mapping sequence and stays `yaml`.
- **Exactness** is the whole source (a RAW-like `DocumentExact`): the log-stream
  model is derived (`Q_gen`) and never on the exactness path (ADR-0060: the
  model node depends on the `sha256(source)` root).
- **Not claimed here:** an economic court, a syslog daemon, or any rendering.
- **Never run on the host:** every command above ran in the pinned `dev`
  container.
