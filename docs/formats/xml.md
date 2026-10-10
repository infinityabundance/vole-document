# XML

XML is the **structured-tree** format of Phase 21 Wave 2 that carries namespaces,
attributes and a DTD policy. It is not a package — the whole source is the
document, and the exact leaf is the source. The adapter is gated behind the
**non-default** `xml = ["dep:quick-xml"]` feature.

## Authority boundary

Detection is byte-based and **conservative**: the source is not PDF/ZIP, not a
JSON/YAML/JSONL/TOML/CSV/Markdown document, and it parses as a single XML
document under the limits. A bare XML tree that only *mentions* `<html>` as a
non-root descendant stays Xml; a document-level HTML marker wins HTML (see
[HTML](html.md)). Malformed input, a `<-junk` fragment, and prose all stay
`Opaque` and round-trip exactly through the RAW lane.

## Representation preservation

A bounded, span-preserving scanner: elements, attributes, text, CDATA, comments,
processing instructions, DOCTYPE and namespace declarations, each with its
**exact source byte span**. Entity references are surfaced **literally**, never
resolved. Unknown namespaces are preserved, never interpreted.

## Supported observations

Common selectors: `metadata`, `text`, `find`. Native: `--xml-path P`,
`--xml-element`, `--xml-attr`, `--xml-namespaces`, `--xml-find PATTERN`.

## Unsupported / honest cost

`Page(n)` and byte-range-as-package are typed declines. A general XPath engine,
XML Schema/DTD validation, C14N, XInclude, and encoding switching beyond UTF-8
are not claimed. The economic court is measured on a **self-authored
deterministic corpus**.

## Security limits

A benign `<!DOCTYPE …>` (bare or `PUBLIC`/`SYSTEM`) is accepted and **never
fetched**; a declaration with an **internal subset is refused**, so no entity is
ever resolved — **no XXE and no billion-laughs expansion**. Bounded depth/node/
string/document caps; a source over any cap declines typed.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted XML,
including documents the adapter declines to interpret, and after the source
**and** descriptor are deleted in a fresh process (the 21.9.1 court and the 21.9
economic court, exactness **10/10** and **6/6**). The exact leaf is the whole
source; the derived model is never on the exactness path (ADR-0060: the model
node depends on the `DocumentExact` root keyed on `sha256(source)`).

## Known limitations

A bounded XML subset, not a validating processor. The economic court compares a
source-retaining SQLite baseline **and** a conventional `ElementTree`
XML→dict/text baseline; the conventional lane normalizes away spans and namespace
bindings and cannot reproduce the source bytes. Only exact closure is a
byte-authority claim.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.9.1 XML court: `tools/phase21-9-1-xml-court.sh` (exactness 10/10,
  6 typed declines, 4 opaque controls); campaign
  [2026-10-10-phase21-9-1-xml-4e3c3fbb](../../evidence/campaigns/2026-10-10-phase21-9-1-xml-4e3c3fbb/).
- Phase 21.9 economic court: `tools/phase21-9-xml-court.sh` (SQLite + a
  conventional `ElementTree` baseline; exactness 6/6); campaign
  [2026-10-10-phase21-9-xml-econ-4e3c3fbb](../../evidence/campaigns/2026-10-10-phase21-9-xml-econ-4e3c3fbb/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
