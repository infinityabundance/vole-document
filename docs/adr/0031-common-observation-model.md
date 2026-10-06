# ADR-0031 — A shared observation vocabulary, not a shared schema

Status: accepted (Phase 12.0).
Extends ADR-0026 (observation algebra, provenance, EXPLAIN). Relates: ADR-0029
(layers). Cites plan §DEC-2, §DEC-5, §DEC-6, §77–§78; research A §8.4, C §9,
D §9, J §1.

## Context

Phase 11 landed only 7 selectors and 9 representations, all PDF-flavoured
(`src/field/observe.rs`). A consumer needs one way to ask a question across PDF,
DOCX and EPUB, but the shared unit must not become a lossy schema that erases
format-native structure. Folding native concepts into one ontology is exactly the
universal-AST mistake rejected in ADR-0029.

## Decision

The common layer is a **small vocabulary plus native escape hatches**:

* A shared selector/representation set (`metadata`, `text`, `heading`, `block`,
  `table`, `cell`, `resource`, `link`, `find`) is added additively to the existing
  enums. Native selectors remain **first-class**: `Selector::{Pdf,Docx,Epub}` and
  their per-format variants (ADR-0032/0033) are peers, not fallbacks.
* Widening the enums is additive; the dispatch and planner already **fail closed**
  on unknown pairs, returning a typed `unsupported_feature`, never an empty answer.
* **Provenance is preserved on every answer**: `Basis`, `source_span`, and
  `IntegrityScope` carry through. Derived/common observations are
  `DeterministicallyDerived` or `Heuristic`; only `Authored`/`DirectlyObserved`
  are exact. `source_span` is the exact physical span (e.g. compressed ZIP member);
  an offset inside a decoded part is `Q_gen` and never conflated with it.
* **Capability discovery** (`capabilities ROOT`, machine-readable) reports the
  supported selectors/representations per format; an unsupported observation
  returns a typed capability error.
* **No invented coordinates**: reflowable EPUB has no intrinsic pages; DOCX has no
  intrinsic pagination; `Page(n)` is meaningful only where the format or publisher
  defines it (ADR-0033). Never fabricate `Page(n)`.
* The extraction **profile identity** (ADR-0032/0033) is recorded in the answer and
  hashed into the canonical selector so two profiles never collide.

## Consequences

* One API serves three formats while every answer names its native basis and layer;
  a common observation is a *projection*, never the document model.
* Per-format capability gaps are explicit and auditable rather than silently
  approximated.
* The PDF lane keeps the same numbers (ADR-0029); shared dispatch adds a
  widening, not a behaviour change.

**Rejected:** a single lowest-common-denominator AST; implicit unions (silently
mixing DOCX stories or EPUB spine items); synthesising `Page(n)`; returning an
empty or best-effort answer for an unsupported pair; surfacing a derived
observation with an exact basis.
