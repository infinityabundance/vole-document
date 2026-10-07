# DOCX

DOCX enters through a WordprocessingML inverse compiler over the shared
byte-authoritative ZIP layer (ADR-0030/0032).

## Authority boundary

The exact physical source is the ZIP member cover — local headers, raw compressed
member spans, data descriptors, central directory, ZIP64 and EOCD. That cover,
not a decoded member, is normative. The main part is discovered **semantically**
from `_rels/.rels` (the officeDocument relationship) and cross-checked against
content type and root element; a hardcoded `/word/document.xml` is never
authority. Ambiguous or external targets are typed declines.

## Physical representation

Members are identified by `(archive ordinal, local-header offset)`, distinct from
the advisory logical part name; duplicate names are never normalized. The exact
leaf for a member is its raw compressed span — no unzip/rezip.
`ErrorClass::InvalidZipStructure` (exit 18) covers a broken physical cover.

## Native inverse representation

A bounded WordprocessingML subset: paragraphs/runs/text, styles with heading
identity via resolved `outlineLvl`, sections, headers/footers, notes, comments,
bookmarks, hyperlinks, fields, tracked changes, drawings/resources, numbering.
MCE `Choice`/`Fallback` is chosen deterministically and unknown namespaces are
preserved, not interpreted. Stories are explicit (`Main`, `Header`, `Footer`,
`Footnote`, `Endnote`, `Comment`, `TextBox`, `Glossary`); tables are first-class.

## Supported observations

All common selectors: `metadata`, `text`, `heading`, `block`, `table`, `cell`,
`resource`, `link`, `find`. Native: story, package part/relationship, and raw or
decoded members. Versioned extraction profiles are hashed into the selector.

## Unsupported observations

`Page(n)` is a typed decline — DOCX has no intrinsic pagination and pages are
never synthesized. Parts outside the bounded subset are preserved opaque with a
typed reason, not approximated. A non-UTF-8 part is preserved exactly and its
semantic observation declined.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any `.docx`,
including packages the adapter declines to interpret natively, and after the
source and descriptor are deleted in a fresh process (part of the 38/38 removal
court).

## Security limits

XML is derived-only (`quick-xml`, no DTD/entities, network fetches disabled).
No `PathBuf` is ever built from a member name. No macros or embedded executables
are executed. See [Security](../SECURITY.md).

## Known limitations

Byte-level cross-document sharing of compressed members is a recorded negative
(per-entry local-header metadata defeats it); only decoded members and exactly
identical resources share. All measurement corpora are locally generated. ODT is
not a DOCX concern but is likewise unbuilt.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0030](../adr/0030-zip-physical-layer.md),
[0031](../adr/0031-common-observation-model.md),
[0032](../adr/0032-docx-adapter-scope.md),
[0034](../adr/0034-cross-document-identity-sharing.md).

## Evidence

- Removal / triplet exactness: `evidence/campaigns/2026-10-06-phase12-removal-dc4d5a3/`,
  `evidence/campaigns/2026-10-06-phase12-triplet-dc4d5a3/`.
- Sharing (DOCX↔EPUB identity): `evidence/campaigns/2026-10-06-phase12-share-e7ef693/`.
- Security/fuzz: `evidence/campaigns/2026-10-06-phase12-security-33f6d04/`.
- Results: [phase-12-results.md](../phases/phase-12-results.md).
