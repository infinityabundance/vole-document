# INI / .env / Java properties

The config family (INI / `.env` / Java `.properties`) is the key/value-line
format of Phase 21 Wave 2 (subphase 21.20). Like JSON it is not a package — the
whole source is the document, and the exact leaf is the source. One bounded
adapter covers all three dialects and is gated behind the **non-default,
dependency-free** `config = []` feature.

## Authority boundary

The config family has **no magic bytes**, so detection is a documented,
conservative heuristic that requires a **dialect-distinguishing signal**, and is
tried **after** the strong, fully-parsed tree formats (JSON/JSON5/JSONL/YAML/TOML)
and **before** CSV/Markdown/XML/HTML. It precedes CSV deliberately (a config
value containing commas, e.g. `A=x,y`, would otherwise be stolen by the CSV
detector). INI needs a `[section]` header; `properties` needs a strong `=`/`:`
separator plus a properties-only construct (`:`/whitespace/`!`/`\uXXXX`/
continuation/non-identifier key); `env` needs an `export ` prefix. A source that
is valid TOML is very often syntactically valid INI, so **TOML wins** and an
INI-shaped TOML stays `Toml`.

**What it cannot distinguish.** A file that is all plain `KEY=VALUE` lines with
identifier keys, `#`-only comments, and `=`-only separators is byte-for-byte the
same shape under `env` and Java `properties`. It is **not guessed** — it stays
`Opaque`. A dialect is claimed only on a dialect-only signal. Plain prose, a
`.txt`/Markdown/code blob, a two-column `a b` blob, and a `#!` script all stay
`Opaque` and round-trip exactly through the RAW lane.

## Representation preservation (the point of the format)

A bounded, line-based parser preserves the recorded dialect; exact
section/key/separator/value/comment spans; line order; the `export` marker,
quoting style, and inline comments; `properties` trailing-`\` continuations; and
Java `\uXXXX` escapes **as spelling** (never expanded). **Duplicate keys** are
preserved and reported (`same_key_entries`), never collapsed.

## Supported observations

Common selectors: `metadata`, `text`, `search-match`. Native: `config-line`,
`config-entry`, `config-section`, `config-find`.

## Unsupported / honest cost

A general config-format library, cross-file interpolation, and include resolution
are not claimed. A line that is not a valid construct of the detected dialect, an
out-of-range line/entry/section, a too-deep continuation, and a cap breach are
typed declines (`InvalidConfigStructure`, exit 34; `unsupported-feature`, exit 6;
`resource_limit`, exit 8); `Page(n)` is a typed decline.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted
config source, and after the source **and** descriptor are deleted in a fresh
process. The exact leaf is the whole source; the derived model is never on the
exactness path (ADR-0060: the model node depends on the `DocumentExact` root
keyed on `sha256(source)`). Adapter-court H1 exactness **11/11** (4 config
fixtures + TOML/JSON/CSV cross-format controls + 4 opaque controls); 4 typed
declines.

## Economic court

Measured by `tools/phase21-20-config-court.sh` (a thin wrapper over the shared
`textfmt-court.sh` engine) against two conventional comparators — a
**source-retaining store** plus conventional extraction, and a conventional
**decode-to-host-values load**. Corpus 9 fixtures, questions Q1–Q12, exactness
**14/14**. Estimator = paired per-fixture ratio VOLE/comparator, median +
geometric mean with a fixed-seed, fixture-clustered 95 % CI; ratio-of-sums
reported separately. A ratio < 1 favours VOLE.

| metric | comparator | median | geomean | median 95% CI | geomean 95% CI | W/T/L | ratio-of-sums |
|---|---|---:|---:|---|---|---:|---:|
| build | sqlite | 0.953 | 0.886 | 0.686..0.994 | 0.787..0.971 | 2/7/0 | 0.865 |
| build | conv | 0.969 | 0.876 | 0.725..1.043 | 0.727..1.002 | 2/7/0 | 0.824 |
| storage | sqlite | 0.583 | 0.542 | 0.579..0.587 | 0.468..0.584 | 9/0/0 | 0.313 |
| storage | conv | 0.537 | 0.471 | 0.528..0.542 | 0.361..0.539 | 9/0/0 | 0.172 |
| cold | sqlite | 0.028 | 0.030 | 0.027..0.029 | 0.027..0.036 | 9/0/0 | 0.032 |
| warm | sqlite | 0.083 | 0.079 | 0.068..0.088 | 0.072..0.086 | 9/0/0 | 0.095 |
| warm | conv | 0.082 | 0.081 | 0.071..0.088 | 0.074..0.089 | 9/0/0 | 0.103 |

Build is ~parity/slightly faster (median 0.95–0.97, mostly ties); storage is
~0.54–0.58× SQLite and ~0.47–0.54× the conventional load; warm is ~0.08× both
(a very large win on this small-file corpus). Cold is dominated by the
comparators' fresh-process start-up.

## Honest negatives

* The **env-vs-properties overlap is not distinguished** and stays `Opaque` — an
  honest, recorded limitation, not a guess.
* A TOML document stays `Toml`; an INI-shaped TOML is never reclassified.
* The conventional load is deliberately the weaker comparator (it drops spans,
  exact spelling, duplicate keys, and order).
* The economic court is measured on a **self-authored deterministic corpus**;
  only exact closure (Q6) is a byte-authority claim.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.20.1 config adapter court: `tools/phase21-20-1-config-court.sh`
  (exactness 11/11); campaign
  [2026-10-10-phase21-20-1-config-646e812b](../../evidence/campaigns/2026-10-10-phase21-20-1-config-646e812b/).
- Phase 21.20 economic court: `tools/phase21-20-config-court.sh` (exactness
  14/14); campaign
  [2026-10-10-phase21-20-config-econ-735d9b69](../../evidence/campaigns/2026-10-10-phase21-20-config-econ-735d9b69/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
