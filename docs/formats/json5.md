# JSON5 / JSONC

JSON5/JSONC is the **structured-extra** format of Phase 21 Wave 2 (subphase
21.17). Like JSON it is not a package — the whole source is the document, and the
exact leaf is the source. The adapter is gated behind the **non-default**
`json5 = ["json"]` feature (dependency-free; it reuses the shared JSON parser).

## Authority boundary

Detection is byte-based and **conservative**: strict JSON is tried first and is
never reclassified, so JSON5/JSONC is tried **immediately after JSON** and before
JSONL/YAML/TOML/CSV/Markdown/XML/HTML. The source is claimed only when it is a
strict-JSON *superset* form carrying at least one JSON5/JSONC-only construct (a
comment, a trailing comma, an unquoted key, a single-quoted string, a
hex/leading-dot/`Infinity`/`NaN` number, a string continuation, or the extended
whitespace set) and it parses under the caps. Malformed input and prose stay
`Opaque` and round-trip exactly through the RAW lane.

**What it cannot distinguish.** The recorded dialect is `jsonc` when the only
extensions are comments/trailing commas, else `json5`; the two are a recorded
property, not a separately detected format. The exact ECMAScript ID_Start/
ID_Continue identifier tables are **approximated** with
`char::is_alphabetic`/`is_alphanumeric` (a bounded, deterministic, no-dependency
approximation), not a byte-exact port.

## Representation preservation (the point of the format)

The bounded JSON5 parser reuses the JSON adapter's node arena and span policy, so
for every token it keeps the **exact source byte span** and preserves: comments
(with exact spans, never dropped), unquoted keys, single-quoted strings, trailing
commas, hex/leading-dot/`Infinity`/`NaN` numbers, string continuations, member
order, **duplicate keys** (kept distinct), numeric/escape spelling, and the
recorded dialect.

## Supported observations

Common selectors: `metadata`, `text`, `search-match`. Native:
`json5-pointer` (RFC 6901), `json5-node`, `json5-find`, `json5-comments`.

## Unsupported / honest cost

A general JSON5 evaluator, JavaScript expression evaluation, and comment
preservation across a re-serialization are not claimed. An out-of-range pointer,
an over-cap node/comment/string/depth/document budget, and a malformed source are
typed declines (`InvalidJson5Structure`, exit 31; `resource_limit`, exit 8);
`Page(n)` is a typed decline.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted
JSON5/JSONC, and after the source **and** descriptor are deleted in a fresh
process. The exact leaf is the whole source; the derived model is never on the
exactness path (ADR-0060: the model node depends on the `DocumentExact` root
keyed on `sha256(source)`). Adapter-court H1 exactness **10/10** (7 JSON5/JSONC
fixtures + the strict-JSON control + 2 opaque controls); 7 typed declines.

## Economic court

Measured by `tools/phase21-17-json5-court.sh` against two conventional
comparators — a **source-retaining SQLite** store that must first normalize JSON5
to *strict* JSON, and a conventional **JSON5 → object load**. Corpus 9 fixtures,
questions Q1–Q12, exactness **9/9**. The estimator is the paired per-fixture
ratio VOLE/comparator, summarised by the median and geometric mean with a
fixed-seed, fixture-clustered 95 % CI; the ratio-of-sums is reported separately.
A ratio < 1 favours VOLE.

| metric | comparator | median | geomean | median 95% CI | geomean 95% CI | W/T/L | ratio-of-sums |
|---|---|---:|---:|---|---|---:|---:|
| build | sqlite | 1.049 | 0.877 | 0.990..1.097 | 0.606..1.067 | 1/7/1 | 0.598 |
| build | conv | 1.445 | 1.188 | 1.380..1.477 | 0.795..1.466 | 1/0/8 | 0.760 |
| storage | sqlite | 0.590 | 0.637 | 0.579..0.689 | 0.586..0.723 | 8/1/0 | 1.009 |
| storage | conv | 31.548 | 21.606 | 9.696..42.147 | 10.570..36.126 | 0/0/9 | 1.878 |
| cold | sqlite | 0.032 | 0.041 | 0.032..0.034 | 0.032..0.068 | 9/0/0 | 0.067 |
| warm | sqlite | 0.074 | 0.100 | 0.069..0.114 | 0.072..0.171 | 9/0/0 | 0.614 |
| warm | conv | 1.042 | 0.966 | 0.906..1.115 | 0.799..1.103 | 1/6/2 | 0.507 |

The build **median** only slightly favours SQLite on the small fixtures while the
ratio-of-sums (0.598) favours VOLE on the dominant `large.json5`; the storage
median (0.590) favours VOLE but its ratio-of-sums (1.009) does not, because the
large fixture dominates the sum. Warm vs the conventional load is ~parity
(median 1.042). Cold is dominated by the comparators' fresh-Python-process
start-up and is reported for completeness, not headlined.

## Honest negatives

* The **strict-JSON lane cannot represent `NaN`/`Infinity`** at all: `Infinity`
  is mapped to `1e999` and a source with `NaN` cannot be normalized, so that
  comparator lane declines every JSON question for those fixtures (recorded).
* The conventional load is deliberately the weaker comparator (it drops spans,
  comments, duplicate keys, key spans, the dialect, and spelling); a
  *span-preserving* JSON5 loader is not built here and no claim is made against
  one.
* The economic court is measured on a **self-authored deterministic corpus**;
  only exact closure (Q6) is a byte-authority claim. Every other observation is a
  derived (`Q_gen`) projection.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.17.1 JSON5/JSONC adapter court: `tools/phase21-17-1-json5-court.sh`
  (exactness 10/10); campaign
  [2026-10-10-phase21-17-1-json5-88b30730](../../evidence/campaigns/2026-10-10-phase21-17-1-json5-88b30730/).
- Phase 21.17 economic court: `tools/phase21-17-json5-court.sh` (SQLite +
  conventional JSON5 load; exactness 9/9); campaign
  [2026-10-10-phase21-17-json5-econ-10f5b898](../../evidence/campaigns/2026-10-10-phase21-17-json5-econ-10f5b898/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
