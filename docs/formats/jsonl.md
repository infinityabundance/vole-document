# JSONL / NDJSON

JSONL (newline-delimited JSON) is the **line/event-stream** format of Phase 21
Wave 2. It is not a package — the whole source is the document, and the exact
leaf is the source. The adapter is gated behind the **non-default** feature
`jsonl = ["json"]` (it reuses the shared JSON parser).

## Authority boundary

Detection is byte-based and **conservative**, and pins the Json-vs-Jsonl
boundary: JSON is tried first (a single value, even across several lines, stays
Json); JSONL is tried immediately after JSON and before YAML/TOML/CSV/Markdown/
XML/HTML, because each non-blank line must parse as **exactly one JSON value** —
the most specific signal for a newline-separated stream. Fewer than two records,
a malformed line, an over-cap line/record/node budget, and a non-newline-
separated bag are typed declines; such input stays `Opaque` and round-trips
exactly through the RAW lane.

## Representation preservation

A bounded, per-line span-preserving model. Each record is parsed by the **shared
JSON parser** (never a second JSON parser), so every record keeps its exact line
span and every token its exact source span — numeric/escape spelling, member
order, and duplicate keys are preserved exactly as for [JSON](json.md).

## Supported observations

Common selectors: `metadata`, `text`. Native: `--jsonl-line N`,
`--jsonl-pointer P`, `--jsonl-find PATTERN`.

## Unsupported / honest cost

`Page(n)` is a typed decline. The whole stream must be admitted; a single
malformed line declines the document rather than skipping it silently. The
economic court is measured on a **self-authored deterministic corpus**.

## Security limits

Bounded line/record/node/string/document caps; a source over any cap declines
typed (`resource_limit`). No external reference is ever fetched.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted
JSONL, and after the source **and** descriptor are deleted in a fresh process
(the 21.12.1 court and the 21.12 economic court, exactness **9/9** and **6/6**).
The exact leaf is the whole source; the derived model is never on the exactness
path (ADR-0060: the model node depends on the `DocumentExact` root keyed on
`sha256(source)`).

## Known limitations

A line-oriented stream format, not a nested document; a stream whose lines are
not each one JSON value declines. The economic court compares a source-retaining
SQLite baseline **and** a conventional per-line JSON load baseline; the
conventional lane drops source spans and cannot reproduce the source. Only exact
closure is a byte-authority claim.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.12.1 JSONL court: `tools/phase21-12-1-jsonl-court.sh` (exactness 9/9,
  6 typed declines, 1 JSON control, 2 opaque controls); campaign
  [2026-10-10-phase21-12-1-jsonl-92e34118](../../evidence/campaigns/2026-10-10-phase21-12-1-jsonl-92e34118/).
- Phase 21.12 economic court: `tools/phase21-12-jsonl-court.sh` (SQLite + a
  conventional baseline; exactness 6/6); campaign
  [2026-10-10-phase21-12-jsonl-econ-92e34118](../../evidence/campaigns/2026-10-10-phase21-12-jsonl-econ-92e34118/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
