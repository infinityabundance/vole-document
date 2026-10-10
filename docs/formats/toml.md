# TOML

TOML is a **structured-tree** format of Phase 21 Wave 2. It is not a package —
the whole source is the document, and the exact leaf is the source. The adapter
is gated behind the **non-default, dependency-free** `toml = []` feature.

## Authority boundary

Detection is byte-based and **conservative**: the *whole* source must parse as
TOML (tables, arrays of tables, dotted keys, inline tables, arrays, keys, values,
comments). The complete-parse signal is strong, so TOML is tried after
JSON/YAML and before CSV/Markdown/XML/HTML — a TOML comment (`#` at column 0)
would otherwise be misread as a Markdown ATX heading. Prose and malformed input
stay `Opaque` and round-trip exactly through the RAW lane.

## Representation preservation

A bounded, span-preserving parser that keeps **every scalar's exact spelling**
and **every key/value/table's exact source span**. Duplicate keys and table
redefinitions are not silently preserved or collapsed: they are typed declines
(exit 26). Comments are retained as spans.

## Supported observations

Common selectors: `metadata`, `text`. Native: `--toml-path P`, `--toml-table`,
`--toml-find PATTERN`.

## Unsupported / honest cost

`Page(n)` is a typed decline. Calendar validation of date-times and encodings
beyond UTF-8 are not claimed. The economic court is measured on a **self-authored
deterministic corpus**.

## Security limits

Bounded depth/node/string/document caps; a source over any cap declines typed.
No external reference is fetched.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted TOML,
and after the source **and** descriptor are deleted in a fresh process (the
21.11.1 court and the 21.11 economic court, exactness **10/10** and **7/7**). The
exact leaf is the whole source; the derived model is never on the exactness path
(ADR-0060: the model node depends on the `DocumentExact` root keyed on
`sha256(source)`).

## Known limitations

A bounded TOML subset, not a full parser with a datetime library. The economic
court compares a source-retaining SQLite baseline **and** a conventional
`tomllib` TOML→dict baseline; the conventional lane normalizes away spans and
scalar spelling and cannot reproduce the source. Only exact closure is a
byte-authority claim.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.11.1 TOML court: `tools/phase21-11-1-toml-court.sh` (exactness 10/10,
  7 typed declines, 3 opaque controls); campaign
  [2026-10-10-phase21-11-1-toml-eb045c17](../../evidence/campaigns/2026-10-10-phase21-11-1-toml-eb045c17/).
- Phase 21.11 economic court: `tools/phase21-11-toml-court.sh` (SQLite + a
  conventional `tomllib` baseline; exactness 7/7); campaign
  [2026-10-10-phase21-11-toml-econ-eb045c17](../../evidence/campaigns/2026-10-10-phase21-11-toml-econ-eb045c17/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
