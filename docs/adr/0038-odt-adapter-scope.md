# ADR-0038 — ODT adapter scope: ODF package, semantic content part, bounded OpenDocument

Status: accepted (Phase 13.3).
Extends ADR-0029/0030/0031. Relates: ADR-0009 (oracles), ADR-0024 (authority),
ADR-0032 (DOCX), ADR-0033 (EPUB).
Cites phase-13 plan §3; research C §1–§10.

## Context

An `.odt` is an **OpenDocument (ODF) package** on the ZIP layer (ADR-0030): a ZIP
whose first member is the mandatory stored `mimetype`
(`application/vnd.oasis.opendocument.text`), whose `META-INF/manifest.xml`
enumerates the package's files with their media types, and whose main document part
is the OpenDocument content stream `content.xml` (`office:document-content` →
`office:body` → `office:text`).

ODF is **not OPC**: it has no `[Content_Types].xml` and no `_rels/.rels`
`officeDocument` relationship, so a `.docx`-style OPC discovery cannot work. The
two easy mistakes are hardcoding `content.xml` from a file name (rather than
resolving it from the ODF manifest) and treating OpenDocument *conformance* as an
exactness gate. As with EPUB, exactness and interpretive success are separate
outcomes.

## Decision

* **ODF package discovery.** The exact physical source is the ZIP member cover.
  Detection/discovery is byte-based, never by extension: the mandatory stored
  `mimetype` (or `META-INF/manifest.xml`) must declare an OpenDocument **text**
  media type. The main content part is resolved **semantically** from the ODF
  manifest's `content.xml` file-entry (ODF 1.2 §2.2.1), else a non-root entry
  declaring a text media type — never from a hardcoded path string alone.
* **Mandatory manifest.** A missing or malformed `META-INF/manifest.xml` is a typed
  decline (`InvalidPackageStructure` / `InvalidXmlStructure`); the exact bytes
  remain recoverable regardless (`materialize == source` still holds).
* **Bounded OpenDocument content model** from `content.xml` in document order:
  paragraphs (`text:p`), headings (`text:h` + `text:outline-level`), spans
  (`text:span`), lists (`text:list`/`text:list-item`), tables
  (`table:table`/`table:table-row`/`table:table-cell` with
  `number-columns-spanned`/`number-rows-spanned` and `covered-table-cell`), links
  (`text:a`), bookmarks, notes (`text:note`), images/resources (`draw:image` →
  package member), tracked changes (`text:change-start`/`-end`/`text:change` with
  `text:changed-region` kinds), and sections (`text:section`).
* **Named versioned profile** (`OdtExtractProfile`): tracked changes
  Final/Original/All, footnotes/endnotes include/exclude, hidden
  (`text:display="none"`), `text:tab`, and `text:line-break`; the identity is
  recorded in every answer and hashed into the canonical selector.
* **Common + native observations.** The common vocabulary (`metadata`, `text`,
  `heading`, `block`, `table`, `cell`, `resource`, `link`, `find`) maps to the
  adapter's blocks, and native selectors (`odt-part`, `odt-paragraph`,
  `odt-heading`, `odt-table`, `odt-cell`, `odt-list`, `odt-find`) remain
  first-class peers. Generic OPC `package-part` is **not** offered for ODT (ODF has
  no `[Content_Types].xml`); ODF package parts are exposed via `odt-part`.
* **Progressive inversion.** Only the requested part is parsed, on demand; the
  canonical derived model is persisted in the disposable cache and reused. No eager
  full parse; the exact leaf stays the 12.2 member raw span.
* **XML is derived-only** (`quick-xml = "=0.42.0"`, `default-features = false`, no
  DTD/entities, UTF-8); external targets are inert strings and never fetched.

## Consequences

* An invalid or hostile ODT is still an exact archival object; only the *derived*
  observation is declined, and the `materialize == source` court still passes.
* The adapter is auditable: every answer names its part, profile, and layer, and
  the ODF manifest is the sole authority for part identity.
* The adapter reuses the byte-authoritative ZIP layer and the shared bounded-XML
  policy; it does not add a second ZIP parser, and it adds no decoder behavior
  (enabling `odt` never changes `.voldoc` bytes).

**Rejected:** a hardcoded main-part path; treating ODF conformance as an exactness
gate; routing ODT through the OPC graph; flattening OpenDocument into a lossy AST;
hidden extraction defaults; fetching external targets; parsing arbitrary encodings
via `encoding_rs`.
