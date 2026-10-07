# EPUB

EPUB enters through a bounded-XHTML/OCF inverse compiler over the shared
byte-authoritative ZIP layer (ADR-0030/0033).

## Authority boundary

The exact physical source is the ZIP member cover (OCF abstract container). The
package document is located **semantically** from `META-INF/container.xml`, never
from a hardcoded `OEBPS/content.opf`. Decode-time EPUB *conformance* is a
differential judgement reported as typed issues and never gates exactness: an
invalid or hostile EPUB is still an exact archival object.

## Physical representation

`mimetype` (first entry, stored, no extra field, exactly the 20 bytes
`application/epub+zip`) and `META-INF/container.xml` → one or more `rootfile`s
(first is default; more than one is a recorded ambiguity). Other `META-INF/*`
members are preserved, never interpreted. Encryption/obfuscation preserves exact
bytes and declines decode.

## Native inverse representation

Package `metadata` (Dublin Core + `refines`), `manifest`, `spine`, and `nav`.
Legacy NCX is exposed alongside nav and never silently preferred. Spine-first
reading order: `SpineItem(n)` is the format-native coordinate. Bounded XHTML
observations (headings, paragraphs, lists, tables, links, resources, fragment
ids, semantic sections) are derived from the retained XML tree; SVG/MathML are
preserved; CSS is text plus edges.

## Supported observations

All common selectors: `metadata`, `text`, `heading`, `block`, `table`, `cell`,
`resource`, `link`, `find`. Native: `spine-item`, package part/relationship,
manifest, and raw or decoded members.

## Unsupported observations

`Page(n)` exists only where a page-list nav, `epub:type="pagebreak"` markers, or a
fixed-layout viewport defines it. Reflowable EPUB has **no intrinsic pages**, and
`Page(n)` is never synthesized from viewport size or heuristics. Scripted content
is flagged static-only and external targets are inert strings (not fetched).

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any `.epub`,
including publications the adapter declines to interpret, and after the source
and descriptor are deleted in a fresh process (part of the 38/38 removal court).

## Security limits

No script execution and no remote fetch. XML is parsed with external entities and
network fetches disabled. One shared content-addressed blob can be shared with a
DOCX by exact content identity ([Persistence and caching](../architecture/persistence-and-caching.md)).

## Known limitations

No intrinsic pagination; reading order and capability gaps are publication-
specific and reported explicitly. Cross-document byte-level member sharing is a
recorded negative. All corpora are locally generated; reflowable EPUB is a
genuine representation gap, not a defect to patch.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0030](../adr/0030-zip-physical-layer.md),
[0031](../adr/0031-common-observation-model.md),
[0033](../adr/0033-epub-adapter-scope.md),
[0034](../adr/0034-cross-document-identity-sharing.md).

## Evidence

- Removal / triplet exactness: `evidence/campaigns/2026-10-06-phase12-removal-dc4d5a3/`,
  `evidence/campaigns/2026-10-06-phase12-triplet-dc4d5a3/`.
- Sharing (DOCX↔EPUB identity): `evidence/campaigns/2026-10-06-phase12-share-e7ef693/`.
- Security/fuzz: `evidence/campaigns/2026-10-06-phase12-security-33f6d04/`.
- Results: [phase-12-results.md](../phases/phase-12-results.md).
