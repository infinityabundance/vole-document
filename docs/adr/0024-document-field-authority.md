# ADR-0024 — The document field and its authority model

Status: accepted (Phase 11.0).
Supersedes nothing. Extends ADR-0001 (exact bytes), ADR-0009 (PDF byte
authority), ADR-0018 (partial materialization), ADR-0020 (content-addressed
store).

## Context

Through Phase 10, `vole-document` was evaluated mostly as a document
*compressor*: whole-file size (loss, ADR-0017), raw random access (loss,
ADR-0019), cross-document sharing (robust loss, ADR-0021), partial-decode CPU
(scoped win, never over zstd, ADR-0018). The measured verdicts are consolidated
in `FINDINGS.md` + ADR-0023. Phase 11 stops optimizing whole-file bytes and
changes what the persistent document *is*.

## Decision

A persisted document becomes a **field**: a persistent, content-addressed
procedural substrate that can serve many typed observations, of which the exact
PDF bytes are exactly one.

Authority is layered and never confused:

| plane | examples | authority |
|---|---|---|
| exact reconstruction | DRA program + objects + channels, `INTEGRITY` | **normative**; `materialize(root) == original_bytes` |
| procedural state | seed DAG nodes (revisions, objects, streams, content, text) | normative for *observations*; deterministic; versioned |
| indexes | hierarchical observation index, seek directory | **advisory**; accelerate only; validate-or-decline |
| derived caches | page preview, text projection | **disposable**; off-wire; closure-keyed |
| trajectory / agent metadata | provenance, comparison, agent verbs | **zero authority** |

The document root is exact only through the DRA. The seed DAG never becomes a
substitute for `INTEGRITY`; a `NodeId` never replaces the source SHA-256. An
agent/LLM/search never sits on the decode path.

## Consequences

* The exactness contract and every existing gate are untouched. With all
  Phase-11 machinery deleted, `materialize(descriptor) == original_bytes` still
  holds byte-exactly (guard test).
* New capability axes become measurable: partial materialization fraction,
  inverse-reuse fraction, source-retouch fraction, bytes read, materialized
  bytes, LLM working set.
* The PDF is one observation surface (`FullExactDocument`), not a prerequisite.
  A query is answered by the minimum dependency closure, not by an implicit full
  parse.
* Every new surface must pay its own byte cost; a surface whose framing cost
  exceeds its benefit is not admitted (the Phase-8 ~440 KB floor lesson).
