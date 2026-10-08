# Phase 22.6 results — compact structural representation

Branch `staging`. Measured at commit `85eee7c` (dirty tree: the Phase-22.6 change
set, recorded in the receipt). Base `main` @ `v0.1.0-alpha.28` (Phase 25).

**Verdict: `NO WIN`.** The typed/structural bytes (`seed/` + `index/` + `field/`)
are only **2.77 %** of the persistent footprint — the **exact authority
(`descriptor/`) is 97.2 %** — so **no candidate compact encoding clears the bar at
full population** (best combined saving **1.166 %**). The mechanisms are real and
byte-exact (round-trip **100/100**), and compaction **does** pay in the region of
small, structure-dense documents (~7.5 % of a `<1 MiB` document's own footprint),
but that region is byte-negligible at population scale. A publishable negative.

## Method (measurement-only — no `src/`, no wire change)

On the frozen `real100-v1` population (100 docs), build the fs stores
(`field-build … --profile runtime`, one file per node so node bytes are directly
measurable) and attribute persistent bytes by namespace and by `SeedNode`
`NodeKind`. Then apply candidate compact encodings, **in the harness only**, to the
**actual canonical node bytes**, requiring a **byte-exact round-trip** and
**observation invariance**.

## Byte composition (full population, 2,808,956,109 B)

| namespace | bytes | share |
|---|---:|---:|
| `descriptor/` (exact authority) | 2,731,290,421 | **97.235 %** |
| `seed/` | 60,349,064 | 2.148 % |
| `index/` | 17,273,125 | 0.615 % |
| `field/` (manifest) | 43,499 | 0.002 % |

Typed structural bytes = **77,665,688 B = 2.765 %**. Top `seed/` kinds:
**`ResourceBlob` 34,860,100 B (57.76 % of seed — exact image/font payloads, not
structure)**, `PdfObject` 10,253,760, `PdfStreamEncoded` 6,291,480,
`PdfStreamDecoded` 4,611,452, `PdfRevisionLineage` 2,472,124, `PageContent`
1,319,852. Seed internals: header 11.24 MB, deps 2.01 MB, params 41.52 MB, prov
5.58 MB.

## Candidate encodings (actual bytes; round-trip 100/100)

| candidate | class | orig B | compact B | saving | % of footprint | rt |
|---|---|---:|---:|---:|---:|---|
| **D interning** | 32-B content-ids → dense varint ordinals (ids recomputed by topological content-addressing) | 12,077,760 | 834,670 | 11,243,090 | 0.4003 % | 100/100 |
| **E header-fold** | 36-B seed header → flag + varints | 11,244,132 | 1,865,284 | 9,378,848 | 0.3339 % | 100/100 |
| **A delta+varint** | index numeric columns + fixed-width seed params | 48,716,489 | 41,846,425 | 6,870,064 | 0.2446 % | 100/100 |
| **B dictionary** | provenance strings | 5,596,339 | 338,218 | 5,258,121 | 0.1872 % | 100/100 |
| **C bitmap+rank** | sparse number sets | 312,660 | 354,576 | **loses** | — | 100/100 |

The typed nodes are internally **~42 % redundant** (77.67 MB → 44.90 MB), but that
is only a **1.166 %** cut of the total footprint. `C` (rank/select bitmap) is
**worse** than a sorted delta list on this population — the mechanism the plan
suggested does not pay here.

**Observation invariance** (labelled subset, 3 docs): the store rebuilt from the
decoded compact bytes is **byte-identical 3/3**, and `observe-batch` answers
(stateful `stats` stripped) match **3/3**.

## Headroom vs the bar

Bar = **≥5 % of total persistent bytes at zero observation loss**. **Combined
full-population saving = 32,762,851 B = 1.166 % → no candidate clears it.**

| size class | docs | total B | typed frac | saving / footprint |
|---|---:|---:|---:|---:|
| `<100 KiB` | 8 | 478,234 | 14.81 % | **7.63 %** |
| `100 KiB–1 MiB` | 17 | 6,558,598 | 12.80 % | **7.45 %** |
| `1–10 MiB` | 40 | 171,426,717 | 5.07 % | 3.21 % |
| `10–100 MiB` | 30 | 1,332,504,954 | 3.90 % | 1.09 % |
| `≥100 MiB` | 5 | 1,297,987,606 | 1.24 % | 0.94 % |

27 documents individually clear 5 %, but they cover only **1.43 %** of the
population's persistent bytes — the byte mass is in large files where typed
overhead is negligible.

## What this does and does not prove

- **Proves (measured).** Compacting an existing typed node **cannot** yield a
  significant full-population persistent-byte win: the total possible saving is
  bounded by the typed fraction (~2.77 %), of which more than half is
  incompressible exact `ResourceBlob` payload. The best single mechanism
  (content-id interning, 11.2 MB) is real and safe but ~0.4 % of the footprint.
- **Does not prove** that no future encoding helps in the small-document region
  (it does, ~7.5 % there), nor anything about cross-document sharing (out of scope,
  refuted). The aggregate is mass-weighted by source bytes, which is exactly why
  the full-population result differs from the small-doc stratum.

## Decision

Recorded as a **measurement-scoped negative**: no shipped mechanism. If intra-node
compaction is ever pursued, the honest target region is **small, structure-dense
documents**, and the largest single lever is **content-id interning** (a scoped
proposal, not built). See [phase-22-plan.md](phase-22-plan.md) §22.6 and, for the
refuted results this must not reopen, ADR-0021/0028 and ADR-0046.
