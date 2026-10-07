# ODT

ODT (OpenDocument Text) enters through a bounded OpenDocument inverse compiler over
the shared byte-authoritative ZIP layer (ADR-0030/0038). ODF is **not** OPC, so —
like EPUB — the adapter reuses the ZIP physical layer and the bounded-XML policy but
does not route through the OPC graph.

## Authority boundary

The exact physical source is the ZIP member cover. The main content part is located
**semantically** from `META-INF/manifest.xml` (the `content.xml` file-entry, ODF 1.2
§2.2.1), never from a hardcoded path alone. Decode-time OpenDocument *conformance*
is a differential judgement reported as typed issues and never gates exactness: an
invalid or hostile ODT is still an exact archival object.

## Physical representation

`mimetype` (first entry, stored, no extra field, an OpenDocument text media type such
as `application/vnd.oasis.opendocument.text`), `META-INF/manifest.xml`,
`content.xml`, `styles.xml`, `meta.xml`. Members are identified by
`(archive ordinal, local-header offset)`; the exact leaf is the raw compressed span
(no unzip/rezip). A missing or malformed manifest is a typed decline
(`InvalidPackageStructure` / `InvalidXmlStructure`); encryption preserves exact bytes
and declines decode.

## Native inverse representation

The `office:body`/`office:text` content model: paragraphs (`text:p`), headings
(`text:h` + `text:outline-level`), spans (`text:span`), lists
(`text:list`/`text:list-item`), tables (`table:table`/`table:table-row`/
`table:table-cell` with `number-columns-spanned`/`number-rows-spanned` and
`covered-table-cell`), links (`text:a`), bookmarks, notes (`text:note`),
images/resources (`draw:image` → package member), tracked changes
(`text:change-start`/`-end`/`text:change` resolved through `text:changed-region`
kinds), and sections (`text:section`). Unknown elements are preserved in the exact
bytes and simply not interpreted.

## Supported observations

All common selectors: `metadata`, `text`, `heading`, `block`, `table`, `cell`,
`resource`, `link`, `find`. Native: `odt-part` (an ODF manifest part by
`manifest:full-path`), `odt-paragraph`, `odt-heading`, `odt-table`, `odt-cell`,
`odt-list`, `odt-find`, and raw or decoded members. Versioned extraction profiles
are hashed into the selector.

## Unsupported observations

`Page(n)` is a typed decline — OpenDocument has no intrinsic pagination and pages are
never synthesized. Generic OPC `package-part` is not offered (ODF has no
`[Content_Types].xml`); ODF package parts are reached through `odt-part`. A part
declared without a resolvable member, or a package with no resolvable content part,
is a typed decline, never an approximation.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any `.odt`, including
packages the adapter declines to interpret natively, and after the source and
descriptor are deleted in a fresh process (the 13.3 removal court).

## Security limits

XML is derived-only (`quick-xml`, no DTD/entities, no `encoding_rs`, UTF-8 only,
bounded depth/events/nodes/attributes/text). No `PathBuf` is ever built from a member
name; external targets are inert strings and never fetched; no script execution. See
[Security](../SECURITY.md).

## Known limitations

This is a **bounded** OpenDocument subset, not a full ODF 1.2 render. Point
`text:change` content is substituted only inside an open paragraph. Nested lists and
nested tables are flattened into their enclosing construct (nested tables contribute
their text). Whitespace-only text nodes are dropped (pretty-print indentation is not
significant in ODF). A `draw:image` reference is resolved by package-relative path,
not through a manifest relationship. Also, ODF conformance is a differential report,
not an exactness gate. All measurement corpora are locally generated.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0030](../adr/0030-zip-physical-layer.md),
[0031](../adr/0031-common-observation-model.md),
[0034](../adr/0034-cross-document-identity-sharing.md),
[0038](../adr/0038-odt-adapter-scope.md).

## Evidence

- Phase 13.3 ODT court: `tests/odt_adapter.rs` (detection; content; profiles;
  provenance; byte-exact materialization; source + descriptor deletion in a fresh
  process; malformed/missing-manifest typed decline).
- Campaign: `evidence/campaigns/2026-10-07-phase13-odt-95c486d/`.
- Results: [phase-13-results.md](../phases/phase-13-results.md).
