# ADR-0040: A benign `DOCTYPE` is accepted and ignored in the bounded-XML policy

- **Status:** Accepted (Phase 13.7 — remediation of a `real100-v1` frontier finding)
- **Date:** 2026-10-07

## Context

The bounded-XML policy (Phase 12.3/12.5) shared by the OPC (DOCX), OCF (EPUB) and
ODF (ODT) adapters **refused every `<!DOCTYPE`** — the textbook defense against
XXE and billion-laughs. The `real100-v1` frontier court then exposed the cost
directly: **every NASA EPUB content document carries the standard XHTML
`<!DOCTYPE html …>`**, so the frozen EPUB adapter declined **all** content
observations (`text`/`heading`/`table`, typed `InvalidXmlStructure`) on real
books. The refusal was correct about the *dangerous* case (a DTD) and wrong about
the *common, inert* case (a bare declaration or a PUBLIC/SYSTEM identifier we
never fetch).

The distinction that was missing: a `DOCTYPE` is dangerous only when it can
**declare entities or reference resources we resolve**. A declaration with an
*internal subset* (`<!DOCTYPE x [ … ]>`) can do both; a bare
`<!DOCTYPE html>` or `<!DOCTYPE html PUBLIC "…" "…">` declares nothing we act on.

## Decision

- **Accept a benign declaration; refuse one with an internal subset.** The shared
  helper `accept_doctype(raw: &str)` accepts the declaration (and discards it)
  unless it contains `[`, in which case it returns the typed
  `InvalidXmlStructure` refusal. Every adapter's pull loop routes `Event::DocType`
  through it (OPC ×2, DOCX ×2, EPUB mod ×3, EPUB content ×1, ODT ×3). The
  `doctype_declined()` message is updated to "…with an internal subset…".
- **Still no DTD, no entity resolution, no external fetch.** The declaration is
  dropped; entities are never expanded and the external identifier is never
  fetched. Undeclared entity references in the body continue to be handled by the
  existing `entity_ref_text` path (fail-closed), and the
  `harden_xml` (UTF-8-only, NUL/UTF-16, size) checks are unchanged. This is a
  policy on *which declarations are inert*, not a relaxation of the no-DTD rule.
- **No decoder behavior, no wire change, no limit weakening.** The change is
  parse-side only (derived `Q_gen` state); `.voldoc` bytes are untouched, the
  universe string and `FORMAT_MINOR` do not move, and no cap is raised.

## Consequences

- **Real EPUB content parses.** On `real100-v1` the frozen adapter answers
  `block`/`heading`/`doc-text` on the NASA EPUBs (previously 62 typed declines);
  re-measured in the frontier court (see the receipt below).
- **The hostile cases still decline.** Both hostile fixtures use an internal
  subset (`docx_doctype.docx`: `<!DOCTYPE w:document [<!ENTITY x "boom">]>`;
  `epub_doctype_xhtml.epub`: `<!DOCTYPE html [<!ENTITY e "x">]>`), so the security
  court's tally is unchanged. The unit tests are updated to assert exactly the new
  line: benign accepted, subset refused (`doctype_policy` in `docx/wml.rs` and
  `epub/content.rs`; `tests/epub_adapter.rs` now distinguishes the benign-`DOCTYPE`
  container decline as `InvalidPackageStructure` from the subset
  `InvalidXmlStructure`).
- **Honest limits.** A `DOCTYPE` whose external identifier points at a *local*
  resource is still not fetched, so a document that relies on a *locally-defined*
  entity in the body will fail closed (unchanged). The heuristic (bit `[`) is
  deliberately conservative: any internal subset refuses, even a subset that
  declares no entities.

## References

- `src/adapter/package/xml.rs` (`accept_doctype`, `doctype_declined`)
- `src/adapter/package/opc.rs`, `src/adapter/docx/wml.rs`,
  `src/adapter/epub/{mod,content}.rs`, `src/adapter/odt.rs` (the `Event::DocType`
  arms)
- `tests/epub_adapter.rs`, `tests/opc_core.rs`,
  `tests/fixtures/phase12-hostile/` (unchanged hostile expectations)
- ADR-0029/0030/0031 (the DOCX/EPUB/package adapters whose XML policy this amends);
  `docs/evidence/real100-frontier-report.md` (the finding)
