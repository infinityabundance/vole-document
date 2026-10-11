# syslog / log streams

A syslog / log stream is the **line/event-stream** format of Phase 21 Wave 2
(subphase 21.28). Like the other Wave-2 formats it is **not a package** — the
whole source is the document and the exact leaf is the source. The adapter is
gated behind the **non-default, dependency-free** `logstream = []` feature (a
log line is *not* JSON, so reusing the JSON parser would add no representation
value; the physical-line split is implemented directly).

## Authority boundary

Log streams have **no magic bytes**, so detection is a documented, conservative
byte-based test: at least two non-blank lines, and **every** non-blank line must
match one of three **recorded dialects** — RFC 5424 syslog
(`<PRI>VERSION … STRUCTURED-DATA [SP MSG]`), RFC 3164 (BSD) syslog
(`<PRI>Mmm dd hh:mm:ss HOSTNAME TAG[pid]: MSG`), or a generic application log
line (an optional leading timestamp and/or level token followed by the message).
Precedence is explicit: logstream is tried **after** every structured/tabular/
prose format (so a JSON/JSONL/YAML/TOML/CSV/config/Markdown/reST/AsciiDoc
document is never stolen) and **before** the maximally ambiguous fixed-width
heuristic (which could otherwise read a column-aligned log stream as a
column-position table).

**What it cannot distinguish.** A prose file whose **every** line begins with a
level word is indistinguishable from a log and **is claimed** (recorded); a
single log-looking line inside prose is not claimed; and a pure BSD stream whose
every message lacks a `: ` is a YAML mapping sequence and **stays `yaml`**. A
JSON value stays `Json` and a JSONL stream stays `Jsonl` (both tried first); a
single-JSON control declines logstream, and a JSONL control declines it typed.

## Representation preservation (the point of the format)

Each record keeps its recorded dialect, its exact line span and terminator
(LF/CRLF/none), its physical line number, its decoded priority, the deepest
structured-data nesting, and every field's exact span. Structured-data elements,
NILVALUE (`-`), and timestamp/level spelling are preserved **verbatim**; a
message is never normalized.

## Supported observations

Common selectors: `metadata`, `text`, `search-match`. Native: `logstream-line`,
`logstream-field`, `logstream-find`.

## Unsupported / honest cost

An out-of-range record and an unsupported common pair decline typed (exit 6); a
malformed native field reference is a usage error (exit 2); plain prose stays
`Opaque` (exit 6). A syslog daemon, a real-world log population, and any
rendering are **not claimed**.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted log
stream, and after the source **and** descriptor are deleted in a fresh process.
Exactness is the whole source (a RAW-like `DocumentExact`): the log-stream model
is derived (`Q_gen`) and never on the exactness path (ADR-0060: the model node
depends on the `sha256(source)` root).

**Adapter-court H1:** **9/9 byte-exact** (5 logstream fixtures — `generic`,
`rfc5424`, `rfc3164`, `crlf`, `blank` — + 4 detection-boundary controls: prose,
single JSON, JSONL, BSD-without-colon; 1 typed decline; 1 usage row).

## Economic court

Measured by `tools/phase21-28-logstream-econ-court.sh` (a thin wrapper over the
shared `textfmt-court.sh` engine) against two comparators — a **source-retaining
SQLite store** with conventional extraction, and a conventional
**decode-to-host-values load** (`conv`). Corpus 6 fixtures, questions Q1–Q12,
exactness **10/10**. Estimator = paired per-fixture ratio VOLE/comparator, median
+ geometric mean with a fixed-seed (2130), fixture-clustered 10000-resample 95 %
CI, tie band ±10 %; ratio-of-sums reported separately. A ratio < 1 favours VOLE.

| metric | comparator | median | geomean | median 95% CI | geomean 95% CI | W/T/L | ratio-of-sums |
|---|---|---:|---:|---|---|---:|---:|
| build | sqlite | 0.811 | 0.839 | 0.779..0.945 | 0.781..0.930 | 5/1/0 | 0.849 |
| build | conv | 1.040 | 1.043 | 1.021..1.070 | 1.028..1.060 | 0/6/0 | 1.041 |
| storage | sqlite | 0.593 | 0.604 | 0.576..0.646 | 0.582..0.637 | 6/0/0 | 0.682 |
| storage | conv | 14.432 | 10.036 | 7.101..18.853 | 3.995..17.238 | 0/1/5 | 1.097 |
| cold | sqlite | 0.025 | 0.029 | 0.023..0.048 | 0.024..0.042 | 6/0/0 | 0.033 |
| cold | conv | 0.026 | 0.031 | 0.024..0.051 | 0.025..0.044 | 6/0/0 | 0.034 |
| warm | sqlite | 0.082 | 0.107 | 0.078..0.244 | 0.079..0.181 | 6/0/0 | 0.301 |
| warm | conv | 0.566 | 0.618 | 0.534..0.789 | 0.545..0.726 | 6/0/0 | 0.543 |

Build is ~0.81× SQLite but ~parity with the conventional load; storage is
~0.59× SQLite but a **recorded loss vs the conventional load**; cold and warm are
wins on both lanes.

## Honest negatives

* **Boundary (recorded):** a prose file whose **every** line begins with a level
  word **is claimed** as a log; a single log-looking line inside prose is not; and
  a pure BSD stream whose every message lacks a `: ` **stays `yaml`**.
* **Storage-vs-`conv` is a recorded loss** (the conventional lane stores far
  less because it drops spans and spelling).
* Only exact closure (Q6) is a byte-authority claim; the **conventional load is
  deliberately the weaker comparator**, and a span-preserving loader could in
  principle match VOLE on the representation axes.
* The **cold ratios** are dominated by the comparators' fresh-process **Python
  start-up**.
* The economic court is a **self-authored deterministic corpus** (by
  `tools/fixtures/make-logstream.py`), not a real-world log population.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.28.1 log-stream adapter court (exactness 9/9):
  [2026-10-10-phase21-28-1-logstream-bd187003](../../evidence/campaigns/2026-10-10-phase21-28-1-logstream-bd187003/).
- Phase 21.28 log-stream economic court (exactness 10/10):
  [2026-10-11-phase21-28-logstream-econ-372a86e0](../../evidence/campaigns/2026-10-11-phase21-28-logstream-econ-372a86e0/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
