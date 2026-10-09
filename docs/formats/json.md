# JSON

JSON is the first **structured-tree** format (Phase 21 Wave 2). Unlike the office
formats it is not a package/container: the whole source is the document, and the
exact leaf is the source itself. The adapter is gated behind the **non-default,
dependency-free** `json = []` feature.

## Authority boundary

Detection is byte-based and **conservative**: the source is not PDF and not a ZIP
family, and the *whole* source parses as exactly one JSON value under the limits
(bounded depth, node count, string bytes, document bytes). Anything else —
including malformed JSON, or an input that merely starts with `[` — stays
`Opaque` and still round-trips exactly through the RAW lane. JSON decoding is a
derived (`Q_gen`) judgement and never gates exactness.

## Representation preservation (the point of the format)

A conventional "JSON → value" pipeline destroys representation. This adapter
keeps, for every token, its **exact source byte span**, and preserves:

* **object member order** (not re-sorted);
* **numeric spelling** — `1e3`, `1.0`, `-0` are kept as the literal text, not
  normalized to a float;
* **string escape spelling** — `"\u00e9"` and `"é"` are distinct tokens;
* **duplicate keys** — kept distinct (a pointer reports how many members match),
  never silently collapsed.

## Native inverse representation

The parsed value tree: object / array / string / number / true / false / null,
with each node's kind, exact source span, and (for object members) the key token
and the key/value spans; parent/child relations and bounded depth/nodes. The
model is canonical and bounded; no `unwrap`/panic on untrusted input.

## Supported observations

Common selectors: `metadata` (top-level type + counts), `text` (a deterministic
canonical rendering), `find`. Native: `--json-pointer P` (RFC 6901, including
`~0`/`~1` escapes and array indices) returning the node's kind, exact source span,
and exact token bytes; `--json-node`; `--json-find PATTERN` (lexical search over
keys and strings).

## Unsupported observations

`Page(n)` and byte-range-as-package are typed declines — JSON has no pagination
and no member structure. A pointer into a non-existent path declines typed, never
a silent empty answer.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted JSON,
including documents whose tree the adapter declines to interpret, and after the
source **and** descriptor are deleted in a fresh process (the 21.5.1 court and the
21.5 economic court, exactness **8/8** and **7/7**). The exact leaf is the whole
source; the derived model is never on the exactness path (ADR-0060: the model node
depends on the `DocumentExact` root keyed on `sha256(source)`).

## Security limits

Bounded depth (`max_json_depth`), node count (`max_json_nodes`), string bytes
(`max_json_string_bytes`), and document bytes (`max_json_document_bytes`); a
document over any cap declines typed (`resource_limit`), and a structural error is
`InvalidJsonStructure` (exit 21). No external reference is ever fetched.

## Known limitations

This is a **bounded** JSON subset (RFC 8259), not a JSON5/JSONC superset. It does
not evaluate JSON Schema, JSONPath beyond RFC 6901 pointers, or duplicate-key
resolution policy (it surfaces duplicates rather than choosing). The economic
court is measured on a **self-authored deterministic corpus**; only exact closure
is a byte-authority claim.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.5.1 JSON court: `tests/json_adapter.rs`; campaign
  `evidence/campaigns/2026-10-09-phase21-5-1-json-0d94667/`.
- Phase 21.5.2 economic court: `tools/phase21-5-json-court.sh`;
  campaign `evidence/campaigns/2026-10-09-phase21-5-json-econ-0d94667/`.
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
