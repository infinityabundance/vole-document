# Observations and provenance

The field answers typed, auditable observations instead of a confident string
with no basis. Database ergonomics (`EXPLAIN`) are valuable, but a relational
ontology never becomes the normative document model (ADR-0026).

## Observation algebra

Typed Rust core; no SQL DSL.

```text
Observe { root, selector, representation, revision?, limits, output_budget, prefs }

selector:       Document | Page(n) | Region(page,rect) | Object(id) | Stream(id)
                | Revision(id) | Resource(id) | TextMatch(pattern) | Table(id)
                | Cell(table,row,col) | PhysicalByteRange(a..b) | ProceduralNode(id)
representation: Metadata | Text | Structure | Table | Operators | ResourceManifest
                | Preview | EncodedBytes | DecodedBytes | ExactBytes | FullDocument
```

Each represented observation resolves to an exact source byte span (`Q_ref`), or
is explicitly a derived, non-exact answer (`Q_gen`, `exactness: none`). `Q_ref`
and `Q_gen` are never conflated.

## Provenance on every answer

```text
FieldAnswer<T> { value, basis, root, observation_scope, dependency_ids,
                 evidence, source_spans, integrity_scope }
```

`basis ∈ { authored, directly-observed, deterministically-derived, inferred,
heuristic, unresolved }`. An exact original byte range and a heuristically
inferred table boundary do not share an epistemic basis and are never blurred.
Common observations are `DeterministicallyDerived` or `Heuristic`; only
`Authored`/`DirectlyObserved` are exact (ADR-0031).

## EXPLAIN / EXPLAIN ANALYZE

- `explain` is a pure function of validated metadata: the intended plan
  (selected frontier, index path, required nodes, planned reads, what will and
  will not materialize).
- `explain --analyze` executes and reports the actual work: index/root bytes
  read, seed nodes fetched and reused, dependencies traversed, inverse ops
  executed, decoded/intermediate/final bytes, CPU, wall, RSS, source bytes
  re-read, and whether the whole source was materialized.

The planner eliminates inadmissible shapes, then costs admissible shapes with
integer weights and a fixed tie-break; a `full_materialize` fallback always
exists. A query is answered by its minimum dependency closure, not an implicit
full parse.

## Partial materialization (`view`)

An optional advisory `OBSERVATION_INDEX` record (tag `0x70`) lets `view` serve
one byte range, object, stream, or revision by evaluating only the ops
intersecting `[a,b)` and decoding only the referenced channels. It is re-derived
from the authoritative program at parse and never trusted; every served slice is
`cmp`'d against the full materialization (ADR-0018).

Result. Scoped decode-CPU win. On a 32.2 MiB, 800-stream PDF, 18/18 pre-registered
queries are byte-exact; mid/late queries touch ~0.41–0.43 MB
(`descriptor_bytes_traversed` alone) regardless of offset, versus gzip inflating
`a + len` — late region 1.4–2.3% of gzip's bytes, ~2–5× faster than gzip and
~4–13× than xz on CPU.

Limitations. It never beats zstd's decoder; it loses in the early region (≤ ~8–16
MiB); peak RSS is ~38 MB vs gzip's ~1.2 MB; and in v1 `view` reads the whole
descriptor, so on-disk I/O is **not** reduced. `entropy_bytes_decoded` is a subset
already counted in `descriptor_bytes_traversed` and must not be added.

Receipt: `evidence/campaigns/2026-10-05-phase7-partial-a5764c9/`. ADR-0018.

## Seek-based partial I/O

Phase 8 adds an optional seek `DIRECTORY` record (tag `0x71`, first record at
fixed offset 64) and a `Read + Seek` reader; `view` peeks only the 64-byte header
and fetches only the record classes a query needs. The directory is advisory and
never authority: every locator is cross-checked against record framing and the
class index against a linear scan; a lying directory is rejected, and a missing
or oversized one declines with `unsupported-feature` rather than reading the
whole file (ADR-0019).

Result. Scoped bytes-read win **versus non-seekable sequential codecs only**. On
the same 32.2 MiB corpus the seekable descriptor is 17,566,832 B; the seeked
`view` reads a constant 439,679–461,367 B, ≤ 2.6% of the descriptor for every
query, and 4.7%–~21× fewer bytes than gzip's compressed prefix in the late
region. CPU drops to ~0.00 s and peak RSS to ~3.8 MB.

Limitations. Against **fine-block** seekable/blocked formats it reads 2.6–30×
more (bgzip 23,808 B, xz-64KiB 15,344 B), and BGZF is smaller whole-file. It
loses at `a = 0` and early (≤ ~1.7 MiB) and never beats xz's tiny prefix. The
~440 KB floor is offset-independent, so it would dominate a descriptor smaller
than ~9 MB. Every served slice is byte-exact.

Receipts: `evidence/campaigns/2026-10-05-phase8-seek-08de2a9/`; reports
[phase8-seek-report.md](../evidence/phase8-seek-report.md),
[phase8-skeptic-review.md](../reviews/phase8-skeptic-review.md).

## Capability discovery

`capabilities ROOT` detects `ROOT`'s format from bytes and prints the supported
selectors/representations per format. An unsupported observation returns a typed
capability error, never an empty or best-effort answer (ADR-0031).

## Non-relational boundary

`pages`/`tables`/`cells`/`resources` may appear only as **views** over the
procedural field, never as normative authority. The authoritative state is the
reconstructive DAG plus its residual/literal closure. A query language can grow
above the typed core later without the core becoming a database.

## Relevant ADRs

[0018](../adr/0018-partial-materialization.md),
[0019](../adr/0019-seek-based-io.md),
[0024](../adr/0024-document-field-authority.md),
[0026](../adr/0026-observation-query-provenance.md),
[0027](../adr/0027-cost-accounting.md),
[0031](../adr/0031-common-observation-model.md).
