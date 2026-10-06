# ADR-0026 — Observation semantics, provenance, and EXPLAIN

Status: accepted (Phase 11.0).
Relates: ADR-0018 (partial materialization), ADR-0023 (consolidated findings).

## Context

The field (ADR-0024) can serve many observations; an agent needs typed,
auditable answers, not a confident string with no basis. Database ergonomics
(`EXPLAIN`) are valuable, but a relational ontology must not become the
normative document model.

## Decision

### Observation algebra (typed Rust core; no SQL DSL)

```text
Observe { root, selector, representation, revision?, limits, output_budget, prefs }

selector:        Document | Page(n) | Region(page,rect) | Object(id) | Stream(id)
                 | Revision(id) | Resource(id) | TextMatch(pattern) | Table(id)
                 | Cell(table,row,col) | PhysicalByteRange(a..b) | ProceduralNode(id)
representation:  Metadata | Text | Structure | Table | Operators | ResourceManifest
                 | Preview | EncodedBytes | DecodedBytes | ExactBytes | FullDocument
```

Each represented observation resolves to an **exact source byte span**
(`Q_ref`), or is explicitly a derived, non-exact `Q_gen` answer
(`exactness: none`). `Q_ref` and `Q_gen` are never conflated.

### Provenance is part of every answer

```text
FieldAnswer<T> { value, basis, root, observation_scope, dependency_ids,
                 evidence, source_spans, integrity_scope }
```

`basis ∈ { authored, directly-observed, deterministically-derived, inferred,
heuristic, unresolved }`. An exact original byte range and a heuristically
inferred table boundary do not share an epistemic basis and are never blurred.

### EXPLAIN / EXPLAIN ANALYZE

`explain` is a **pure function of validated metadata** returning the intended
plan (selected frontier, index path, required nodes, reads planned, what will
and will not materialize). `explain --analyze` executes and reports actual work:
index/root bytes read, seed nodes fetched, dependencies traversed, inverse
operations executed, reused vs executed nodes, decoded/intermediate/final bytes,
CPU, wall, RSS, source bytes re-read, external tool invocations.

### Planner

Correctness is a precondition, never a cost: inadmissible shapes are
*eliminated*, then admissible shapes are costed with integer weights and a fixed
tie-break; a `full_materialize` fallback always exists. Q_ref cardinalities are
exact on the wire, so no histograms/statistics are needed initially.

### Non-relational boundary

`pages`/`tables`/`cells`/`resources` may appear only as **views** over the
procedural field, never as normative authority. The authoritative state is the
reconstructive DAG + residual/literal closure.

## Consequences

* Every answer is auditable; `integrity_verified == false` on partial views and
  is never promoted to archival authority.
* `EXPLAIN ANALYZE` becomes the central evidence surface for Phase-11 claims.
* A query language can grow above the typed core later without the core becoming
  a database.
