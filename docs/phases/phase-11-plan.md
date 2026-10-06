# Phase 11 — Persistent procedural document field

Branch: `phase11`. Base: `main` @ `be69026` (`v0.1.0-alpha.12`).
Status: **IN PROGRESS**.

Phase 10 keeps its existing role (DSFB encoder-only search governance; recorded
**negative**: 0‰ measurable benefit, ADR-0022). Phase 11 does not renumber or
absorb it. Phase 11's default search policy is **deterministic fixed policy**;
Phase 10's governor *mechanism* (a suggestion is only ever another candidate
that must independently pass the coverage / exactness / complete-cost courts) is
reused only if a candidate space large enough to need bounding appears. There is
no DSFB crate in the decode dependency path.

## The two laws

> **Inverse-proceduralize as early as possible. Persist the recovered procedural
> state. Materialize as late as possible and only as far as the requested
> observation requires.**

> **Never throw away expensive recovered structure merely because one baked
> observation has been produced from it.**

`materialize(root) == original_bytes` (length + SHA-256 + byte-compare) remains
absolutely exact. The PDF is *an* observation surface, not the privileged
working representation.

## Frozen decisions (synthesis of research A–J)

**DEC-1 — EntropyFS is a blob substrate, not a procedural engine.** EntropyFS
0.7.17's `Engine` is a content-addressed blob store (BLAKE3-256 ids) with
`put_blob`/`get_blob`/`read_blob_range`/`contains`/`sync`; the entropy
representation algebra is a *closed enum* whose only procedural variant is the
`UniformXofV1` **negative control**, and the `Encoder` is not injectable. VOLE
therefore owns its own canonical procedural-node serialization and stores each
node as an individual content-addressed blob. We never claim EntropyFS natively
stores VOLE procedural state. `FsSeedStore` (plain files under the store root,
atomic tmp→sync→rename, range reads) is the reference seed substrate;
`EntropyFsSeedStore` (one blob per node) is the optional adapter behind the
existing `entropyfs-store` feature.

**DEC-2 — Node identity is a domain-separated content hash.** `NodeId =
BLAKE3-256("VOLE:PSEED:v1" || canonical_node_state || canonical dependency ids)`,
distinct from the object table's un-prefixed `Id`. Node ids never appear in the
whole-source `INTEGRITY` manifest and never substitute for the source SHA-256.

**DEC-3 — The seed DAG is immutable, cycle-free, and dynamically keyed.** A node
binds canonical state **and** the canonical ids of the dependencies it actually
read (the dynamic read set). Changing one dependency yields a *different* id;
there is no mutation, no "valid" flag, no invalidation pass. Green = present with
a complete id-matching closure; red = absent. This is strictly stronger than
rustc's red-green validation: the content id *is* the fingerprint, so early
cutoff is exact and free.

**DEC-4 — Indexes accelerate; they never define truth.** The hierarchical
observation index is advisory and re-derivable. A lying, corrupt, cyclic,
out-of-closure, or oversized index node is rejected fail-closed; every served
slice carries `integrity_verified == false`. A missing index falls back to the
honest prefix path with unchanged exactness.

**DEC-5 — Caches are off-wire, closure-keyed, disposable, and byte-counted.**
Derived observation caches (page preview, page text projection, table
structure, resource manifest) are keyed by `SCOPED_CLOSURE` (default) or
`COARSE_ROOT`. They are never normative, never required, and are reported as a
**fourth accounting universe** separate from the descriptor/store/source
universes.

**DEC-6 — Observation surfaces and trajectory are zero-authority.** Agent verbs
(`query`/`observe`/`find`/`explain`/`preview`/`compare`) are read-only
observations returning a typed `FieldAnswer` with `basis`
(authored / directly-observed / deterministically-derived / inferred /
heuristic / unresolved), scope, dependency ids, source spans, and integrity
scope. No agent/LLM/model/search is ever on the decode path.

**DEC-7 — Promotion is additive and explicit.** Deepening a coarse/literal
region to a richer procedural representation adds a new root (new
universe minor + feature bit + promotion receipt). The old root keeps
materializing. No silent upgrade; unknown mandatory features/universe changes
fail closed.

**DEC-8 — Every observation surface resolves to a byte span.** To preserve
coverage and exactness, each represented observation maps to an exact source
byte span (or is explicitly typed as a derived, non-exact `Q_gen` answer with
`exactness: none`). `Q_ref` (referential, exact) and `Q_gen` (generative,
derived) are labelled distinctly.

**DEC-9 — The planner is deterministic and correctness-first.** Correctness is a
*precondition*, never a cost. Shape enumeration eliminates inadmissible shapes,
then costs the rest with integer weights and a fixed tie-break; a mandatory
`full_materialize` fallback always exists. Q_ref cardinalities are exact on the
wire, so no histograms/skew learning are needed; learning, if any, applies only
to Q_gen predicates.

**DEC-10 — Do not serialize known structure just to parse it again.** If decoded
stream state or content operators already exist as procedural nodes, an
observation must reuse them; re-baking DEFLATE then re-inflating, or
re-serializing an operator stream then re-parsing it, is forbidden inside VOLE
unless a court explicitly tests that boundary.

## What Phase 11 is, and is not

It is: a persistent procedural document substrate with queryable observations —
an inverse-compiled, content-addressed document field whose procedural state can
be queried directly and selectively materialized into exact or structured
observations on demand.

It is not: a relational database (relational rows are never the normative
document model), a RAG pipeline (embeddings are advisory at most), a cache (the
normative state is the recipe + residual closure), PDF linearization (which
still targets a baked PDF), a monolithic document AST (nodes must be
independently fetchable), or an object store (it answers observations, not just
"give me blob X").

## Architecture

```text
source PDF
   │  ingest (11.4, progressive)
   ▼
FIELD ROOT  (FieldRoot manifest, content-addressed, small)
   ├── exact authority : the existing .voldoc descriptor (DRA + objects +
   │                     channels), stored as one content-addressed blob
   ├── seed DAG        : fine-grained procedural nodes, one blob each (11.2)
   ├── hier index      : bounded observation index (11.3)
   └── provenance      : universe, versions, producer, digests
   │
   ▼
query/observe (11.5)  →  planner (11.6)  →  minimal dependency closure
   │                                            │
   │                                            ▼
   │                                     late materialization frontier
   ▼                                            │
FieldAnswer<T>{ value, basis, scope, evidence, provenance }   ← (11.5)
   │
   ├── derived cache (off-wire, closure-keyed)  ← (11.8)
   └── EXPLAIN / EXPLAIN ANALYZE                ← (11.5)
```

### Module layout (one production crate; internal modules only)

```text
src/field/mod.rs         FieldRoot, FieldStore, open/ingest/query entry points
src/field/node.rs        SeedNode, NodeKind, canonical encode/decode, NodeId, bounds
src/field/dag.rs         closure traversal, resolution, resource bounds, cycle rejection
src/field/index.rs       hierarchical observation index (build/lookup/validate)
src/field/ingest.rs      progressive inverse compiler (Stage A/B)
src/field/observe.rs     selectors, representations, frontier resolution
src/field/plan.rs        deterministic planner + cost model + shapes
src/field/explain.rs     ExplainPlan / ExplainActual
src/field/provenance.rs  Basis, FieldAnswer, source spans
src/field/cache.rs       derived observation cache (off-wire, closure-keyed)
src/store/seed.rs        SeedStore trait + FsSeedStore + EntropyFsSeedStore
src/adapter/pdf/text.rs  content-operator → text-run projection (bounded subset)
```

### Wire changes (minimal, optional, fail-closed)

* New optional record `HIER_INDEX = 0x72` (skippable; a decoder without it still
  fully materializes via the DRA). It is a locator + bound, not authority.
* New universe string `vole-document;universe;phase11;exact-bytes;dra-8;…+procedural-seed-field-v1+hier-index-v1`.
  `dra-8` and `FORMAT_MINOR` do not move; exactness semantics are unchanged.
* The seed DAG and the field manifest live in the **store**, referenced by
  content id. The descriptor remains the exact archival authority.

## Subphases (in order; commit + push each)

- **11.0** research + architectural freeze — this file + ADRs 0024–0027. Research
  reports A–J under `research/subagents/phase-11/` (gitignored).
- **11.1** consume Phase-10 governance: freeze the deterministic default policy;
  document why the DSFB crate is declined (ADR-0022) and what the reusable
  zero-authority governor contract is. Prove decode semantics are identical with
  the machinery absent.
- **11.2** persistent procedural entropy seed DAG: canonical `SeedNode`,
  `NodeId`, `SeedStore` + `FsSeedStore` + `EntropyFsSeedStore`, `FieldRoot`
  manifest, bounds, cycle rejection.
- **11.3** hierarchical observation index: bounded nodes, fanout/depth caps,
  build + lookup + validate-or-decline; `floor-vs-n` and `lying-index` courts.
- **11.4** progressive early inverse-proceduralization: Stage A durable exact
  capture; Stage B cheap eager inversion (revision chain, physical spans,
  objects, streams, decoded streams, page tree, resources); demand-driven deeper
  inversion with persistent promotion.
- **11.5** observation query engine: typed Rust API + CLI (`query`/`observe`/
  `find`/`explain`/`preview`/`materialize`), provenance, `EXPLAIN` /
  `EXPLAIN ANALYZE`.
- **11.6** selective late materialization: per-component frontiers; narrow
  observations must not trigger whole-document reconstruction (instrumented).
- **11.7** scrubbing / page-local preview: a deterministic structured page view
  (supported vector + text subset; unsupported semantics explicitly typed).
- **11.8** persistent computation reuse: cross-process reuse proven with
  execution counters; survives full bake; cache accounting separate.
- **11.9** fair baseline courts: A0 raw PDF tooling, A1 preprocessed
  SQLite/DuckDB, A2/A3/A4 pre-Phase-11 VOLE, A5–A11 Phase-11 ablations.
- **11.10** LLM working-set court: pinned tokenizers, baseline classes, no
  "tokens saved" without naming the tokenizer.
- **11.11** resilience + source-removal court: remove source, restart process,
  query, scrub, fully materialize, verify exactness.
- **11.12** optional immutable edit witness (declared narrow subset only).
- **11.13** skeptic review + release: independent adversarial review, corrected
  claims, sealed evidence, tag `v0.1.0-alpha.13`, publish, delete branch.
- **11.14** (added) finer-than-object shareable units: externalize channel
  payloads / sub-object chunks as their own nodes; measure against
  content-defined chunking. Explicit prior: may also lose.

## Acceptance gates

Exactness (no regression, byte-identical on the exact corpus); source
independence (source removed, new process, queries + scrub + full rematerialize
work); genuine fine-grained procedural state through the seed store (not one
giant blob, not the PDF as one blob + side index); early partial inversion
(opaque region → inverse-process → persist → reuse); late partial
materialization (narrow observation does not rebuild the document, proven by
instrumentation); scrubbing (bounded per-observation working set); work survives
baking; database-like typed query API; `EXPLAIN ANALYZE`; fair database
comparison; LLM working set measured; complete cost reported.

## Negative controls and honest losses

Tiny one-page PDF; random opaque binary; image-heavy document; already-efficient
page-local source; full-document request; first-ever access to an unproceduralized
region; queries SQLite/DuckDB should win. Where a conventional tool wins (page
text via Poppler, render via MuPDF, raw ranges via BGZF, first page via
linearization, narrow indexed SQL), **record it**. The research question is
whether a meaningful repeated agent/document workload has a superior *lifetime*
systems frontier, not whether VOLE beats everything.

## Working rules (unchanged)

Docker only (never the host). One `phaseN` branch; commit + push per subphase.
One subagent at a time, one at a time only, research subagents read-only.
Claim discipline: exactness is shared with lossless compressors; no
"compression" for sharing; no population claims; a seed is not free
information; semantic equality is not archival equality. Each subphase's
headline is attacked by an independent skeptic that tries to falsify it.
