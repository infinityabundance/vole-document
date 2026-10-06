# ADR-0033 — EPUB adapter scope: OCF, package, spine-first order, bounded XHTML

Status: accepted (Phase 12.0).
Extends ADR-0029/0030/0031. Relates: ADR-0009 (oracles), ADR-0024 (authority).
Cites plan §DEC-5, §DEC-6, §22, §78; research D §1–§10, F §2, I §4.

## Context

An `.epub` is an OCF abstract container on the ZIP layer (ADR-0030). Two facts are
easy to get wrong: the package document is located **semantically** from
`container.xml` (not from a hardcoded `OEBPS/content.opf`), and reflowable EPUB has
**no intrinsic pages**. A third — EPUB **conformance is not byte preservation** —
is the central failure mode the adapter must avoid (research D §1).

## Decision

* **OCF container.** `mimetype` (first entry, stored, no extra field, exactly the
  20 bytes `application/epub+zip`) and `META-INF/container.xml` → one or more
  `rootfile`s (first is default; >1 is a recorded ambiguity, never a silent pick).
  Other `META-INF/*` members are preserved, never interpreted. Encryption /
  obfuscation **preserves exact bytes and declines decode**.
* **Package document located semantically**: `metadata` (Dublin Core + `refines`),
  `manifest`, `spine`, `nav`. Legacy NCX is exposed alongside nav and **never
  silently preferred**; disagreement is a typed issue.
* **Spine-first reading order.** `SpineItem(n)` is the format-native coordinate for
  all EPUB. **No intrinsic pages**: `Page(n)` exists only where a `page-list` nav,
  `epub:type="pagebreak"` markers, or a fixed-layout viewport defines it — never
  synthesized from viewport size or heuristics.
* **Bounded XHTML observations** from the retained XML tree in document order:
  headings, paragraphs, lists, tables (physical vs logical cells under
  `colspan`/`rowspan`), links (fragment / intra-container / external-inert),
  resources, fragment ids, semantic sections, SVG/MathML preserved, CSS as text
  plus edges. **No script execution, no remote fetch** — scripted content is
  flagged static-only and external targets are inert strings.
* **Named versioned profiles** (`EpubExtractProfile`): spine linear-only/all, nav,
  scripted static-only, hidden, semantics, text-match scope; identity recorded in
  the answer and hashed into the canonical selector.
* **Exact-byte preservation and EPUB-conformance are separate outcomes.** Every member
  is exactly recoverable regardless of validity; conformance is a **differential
  judgement** reported as typed issues and never gates exactness.

## Consequences

* An invalid or hostile EPUB is still an exact archival object; only the *derived*
  observation is declined, and the `materialize == source` court still passes.
* Per-publication reading order and capability gaps are explicit; no invented
  coordinates appear in any answer.

**Rejected:** a hardcoded package path; synthesising pages; executing scripts or
fetching remote resources; treating conformance as an exactness gate; last-wins
nav/NCX preference; parsing arbitrary encodings via `encoding_rs`.
