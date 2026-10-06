# ADR-0029 — Three representational layers for a multi-format field

Status: accepted (Phase 12.0).
Extends ADR-0024 (field authority), ADR-0026 (observation algebra). Relates:
ADR-0001 (exact bytes), ADR-0009 (PDF byte authority). Cites plan §DEC-1,
§DEC-2, §DEC-5, §DEC-10, §109; research A §8, B §2, J §1, J §7.

## Context

The Phase-11 field is PDF-shaped and its authority model (ADR-0024) is layered.
Phase 12 adds DOCX (OPC/ZIP) and EPUB (OCF/ZIP) through native inverse compilers.
The tempting shortcut is one **universal AST** or a lowest-common-denominator
schema so every format becomes "the same document". Research J §1 (steelman) shows
why that fails: a DOCX `w:p`, an EPUB `<p>` and a PDF page are not the same object,
and a shared Rust `Selector` enum is **interface reuse, not shared semantics**.

## Decision

Three representational layers stay **distinct**; they reference each other and are
never conflated:

1. **Exact physical source state** — PDF spans; ZIP local headers / compressed
   member spans / data descriptors / central directory / ZIP64 / EOCD / comments;
   XML member bytes. Normative; reconstructs `original_bytes`.
2. **Format-native procedural state** — PDF object/stream/revision; OPC
   part/relationship/WordprocessingML paragraph/table/story; EPUB
   package/manifest/spine/nav/XHTML. Normative for *native observations*;
   deterministic; versioned.
3. **Shared observation vocabulary** — a small common selector/representation set
   (ADR-0031) plus per-format escape hatches. Never a lossy universal AST.

Additional boundaries:

* Format capabilities are **explicit** (plan §DEC-5): formats do not share
  coordinates; `Selector::{Pdf,Docx,Epub}` remain first-class; capability discovery
  is machine-readable and unsupported observations are typed errors, never invented.
* The exactness contract and the zero-decode-authority boundary (`SourceServer`,
  ADR-0024) are **unchanged**: only physical source spans are `Q_ref`; every
  derived layer is `Q_gen` and can never feed an exact node.
* **PDF no regression** (plan §109): the widened universal surface must not slow
  or change the PDF lane; a cross-format win paid for by PDF regression is a loss.

## Consequences

* `materialize(descriptor) == original_bytes` (length + SHA-256 + `cmp`, source
  removed, new process) remains the only integrity authority for all three formats.
* New capability axes become measurable per format and per native layer; a
  cross-format claim must be stated per format, never averaged (J §4).
* A shared API is not shared semantics: any "common" claim is scoped to the
  vocabulary layer, with native layers kept explicit.

**Rejected:** one universal AST / lowest-common-denominator schema (destroys byte
authority and the one-way flattening guarantee); a cross-format canonical semantic
model (unproven and unsafe, research H §3); reusing any derived projection as
exact state.
