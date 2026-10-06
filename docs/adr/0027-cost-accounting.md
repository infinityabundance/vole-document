# ADR-0027 — Cost accounting for the procedural field

Status: accepted (Phase 11.0).
Relates: ADR-0017 (lossless baselines), ADR-0019 (seek I/O), ADR-0023
(consolidated findings).

## Context

Phases 8–10 were careful about accounting boundaries: a store root must never be
compared to a whole file; `entropy_bytes_decoded` must not be summed into
`descriptor_bytes_traversed`; a partial read must not claim whole-source
verification. Phase 11 adds ingest work, procedural state, indexes, derived
caches, and query work, each of which is easy to hide.

## Decision

### Four accounting universes, always reported separately

1. **source** — the original document bytes (archival reality);
2. **descriptor + store** — the exact `.voldoc` plus externalized objects
   (normative reconstruction state);
3. **procedural field** — seed DAG nodes + hierarchical index bytes (normative
   for observations, advisory for reconstruction);
4. **derived cache** — disposable observation caches (never normative).

No byte is counted twice; no universe is folded into another.

### Multi-objective cost vector (never one scalar)

`persistent explanatory bytes`, `residual/literal bytes`, `index bytes`,
`dependency bytes`, `physical backing bytes`, `inverse-compilation work`,
`materialization work`, `observation latency`, `bytes read`, `bytes moved`,
`peak memory`, `LLM-facing bytes/tokens`, `dependency depth`. Explicit profiles
(`archive`, `agent-interactive`, `scrubbing`, `low-storage`, `low-latency`,
`forensic`) select weights; no profile is claimed universal. Phase 11's primary
court is **agent-interactive**.

### Defined Phase-11 metrics

```text
materialization_fraction = (intermediate + output bytes produced)
                         / (bytes of the conventional full-materialization path)
seed_read_fraction       = procedural/index bytes fetched / total procedural bytes
source_retouch_fraction  = source bytes read after ingest / source size
inverse_reuse_fraction   = reused inverse nodes / inverse nodes required
observation_token_fraction = VOLE tokens exposed / baseline tokens exposed
proceduralization_depth  = recovered structured layers / declared available layers
```

Each denominator is stated precisely; percentages with differing boundaries are
never compared.

### Ingest is not free and is never hidden

Ingest records wall/CPU/peak RSS, source bytes read, seed bytes written, index
bytes written, inverse candidates tried, Phase-10 decisions where applicable,
residual bytes, and node counts; amortization is measured over 1/10/100/1,000
queries and the crossover (or its absence) is reported.

### The principal equation is lifetime cost

`VOLE lifetime = capture + inverse-proceduralization + procedural state +
observations + edits + caches + requested bakes`, versus
`conventional lifetime = baked source + extracted DB/index + repeated parsing +
repeated decompression + repeated structure recovery + page caches + extracted
text/tables + agent-token expansion + repeated rebaking`. Both are measured
honestly, including the fair preprocessed SQLite/DuckDB baseline with its
one-time extraction and warm materialized views.

## Consequences

* Every Phase-11 claim names its universe, profile, corpus, and baseline class.
* A cache-heavy or index-heavy win must pay for its cache/index bytes.
* The headline is a *lifetime systems frontier*, not `VOLE < xz`.
