# HTML

HTML is the **error-recovering markup** format of Phase 21 Wave 2. It is not a
package — the whole source is the document, and the exact leaf is the source.
The adapter is gated behind the **non-default, dependency-free** `html = []`
feature.

## Authority boundary

Detection is byte-based and **conservative**. A document-level HTML marker
(`<!doctype html>` or an `<html>` root) **wins HTML over the generic XML
fallback**, so well-formed XHTML is detected as HTML, not XML; a bare XML tree
that only mentions `<html>` as a non-root descendant stays XML. A UTF-16 BOM, a
NUL byte, a non-UTF-8 byte string, or input that is not markup stays `Opaque`
and round-trips exactly through the RAW lane.

## Representation preservation

A bounded, span-preserving, **error-recovering** scanner that keeps exact source
byte spans for elements, attributes (double/single/unquoted/boolean quoting),
text, comments and DOCTYPE, and captures raw `script`/`style` content as bytes.
Entity references are surfaced **literally**, never resolved. The canonical
derived model is a tree with per-node spans.

## Supported observations

Common selectors: `metadata`, `text`, `heading`, `link`, `find`. Native:
`--html-path P`, `--html-element`, `--html-attr`, `--html-scripts`, `--html-find PATTERN`.

## Unsupported / honest cost

`Page(n)` is a typed decline. The full HTML **tree-construction recovery
algorithm** (adoption agency / foster parenting), CSS/JS interpretation, and
encodings beyond UTF-8 are not claimed; `script`/`style` content is captured as
raw bytes and never executed or parsed as markup.

## Security limits

A DOCTYPE with an internal subset is **refused**, so no entity is ever resolved.
UTF-8 only. Bounded depth/node/attribute/string/document caps; a source over any
cap declines typed. No remote fetch, ever.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted HTML,
including documents the adapter declines to interpret, and after the source
**and** descriptor are deleted in a fresh process (the 21.10.1 court and the
21.10 economic court, exactness **10/10** and **6/6**). The exact leaf is the
whole source; the derived model is never on the exactness path (ADR-0060: the
model node depends on the `DocumentExact` root keyed on `sha256(source)`).

## Known limitations

A bounded error-recovering scanner, not a browser engine and not a full
tree-construction implementation. The economic court compares a source-retaining
SQLite baseline **and** a conventional `html.parser` HTML→view baseline; the
conventional lane normalizes away spans and raw script bytes and cannot
reproduce the source. Only exact closure is a byte-authority claim.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.10.1 HTML court: `tools/phase21-10-1-html-court.sh` (exactness 10/10,
  6 typed declines, 3 opaque controls); campaign
  [2026-10-10-phase21-10-1-html-f04d7545](../../evidence/campaigns/2026-10-10-phase21-10-1-html-f04d7545/).
- Phase 21.10 economic court: `tools/phase21-10-html-court.sh` (SQLite + a
  conventional `html.parser` baseline; exactness 6/6); campaign
  [2026-10-10-phase21-10-html-econ-f04d7545](../../evidence/campaigns/2026-10-10-phase21-10-html-econ-f04d7545/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
