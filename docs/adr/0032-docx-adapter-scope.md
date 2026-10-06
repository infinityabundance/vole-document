# ADR-0032 — DOCX adapter scope: OPC discovery, WML subset, stories, profiles

Status: accepted (Phase 12.0).
Extends ADR-0029/0030/0031. Relates: ADR-0009 (oracles), ADR-0024 (authority).
Cites plan §DEC-4, §DEC-5, §DEC-6, §19, §77; research C §1–§10, F §2, I §3.

## Context

A `.docx` is an OPC package on the ZIP layer (ADR-0030). Hardcoding
`/word/document.xml` is the classic correctness bug, because the main part is
identified **semantically** by the package relationship. WordprocessingML
(ISO/IEC 29500-1 clause 17) is far larger than Phase 12 can model, so the subset,
the story scoping, and the text-extraction semantics must all be explicit, or the
adapter is either wrong or vacuous (research J §5).

## Decision

* **OPC semantic main-part discovery, not a hardcoded path.** Parse `_rels/.rels`,
  select the officeDocument relationship, require internal `TargetMode`, resolve
  the target against the package base, and cross-check content type + root element
  `w:document`. Zero or >1 officeDocument rels, >1 main content type, or an
  external/ambiguous target → typed decline (`InvalidPackageStructure`). A
  `/word/document.xml` that is not the relationship target is **not** the main part.
* **Supported WordprocessingML subset** = clause-17 core: paragraphs/runs/text
  (`w:t`, `xml:space`), styles with **heading identity via resolved `outlineLvl`**
  (never locale name or `styleId` spelling), sections, headers/footers,
  footnotes/endnotes, comments, bookmarks, hyperlinks, fields, tracked changes,
  drawings/resources, numbering; MCE `Choice`/`Fallback` chosen deterministically,
  unknown namespaces **preserved and never interpreted**.
* **Stories are modelled explicitly** (`Main`, `Header{kind,n}`, `Footer`,
  `Footnote{id}`, `Endnote`, `Comment`, `TextBox`, `Glossary`) and text/find
  observations must be **scoped to one story and never silently mixed**; the
  default is a named, versioned profile, not an implicit union.
* **Tables are first-class** (`w:tbl`/`w:tr`/`w:tc`), exposing physical `(tr, tc)`
  and marking logical grid columns (under `gridSpan`/`vMerge`) as profile-dependent.
* **Versioned extraction profiles** (`DocxExtractProfile`): tracked changes
  Final/Original/All, fields Result/Code/Both, notes/comments/headers/textboxes
  include/exclude, hidden, `lastRenderedPageBreak`, MCE choice; profile identity is
  recorded in the answer and hashed into the canonical selector.
* **Unknown parts stay exactly recoverable**: `Part(ordinal)` with `ExactBytes`
  (raw compressed span, `Q_ref`), `DecodedBytes`/`Xml` (`Q_gen`); unsupported
  members are reported in a manifest with a reason. Declines are typed, never a
  silent drop.
* **XML is derived-only**: `quick-xml = "=0.42.0"` (`default-features = false`, no
  DTD/entities, UTF-8); exact XML bytes remain `Q_ref`. A non-UTF-8 part is
  preserved exactly and its semantic observation declined (no `encoding_rs`).

## Consequences

* The DOCX adapter is auditable: every answer names its story, profile and layer;
  a per-corpus **decline rate** and enumerated declined features are reported so
  "DOCX supported" is not vacuous.
* `materialize(descriptor) == original_bytes` holds for any `.docx`, including
  packages the adapter declines to interpret natively.

**Rejected:** a hardcoded main-part path; silent story mixing; flattening WML into a
lossy AST; hidden extraction defaults; following unknown relationship types;
fetching `TargetMode="External"` targets; adding `encoding_rs`.
