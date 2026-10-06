# ADR-0021: Cross-document sharing does not beat generic chunk dedup (measured)

- **Status:** Accepted — recorded measurement (Phase 9.3)
- **Date:** 2026-10-06
- **Amends:** none. **Relates to:** ADR-0017 (whole-file loss), ADR-0020 (store
  contract and three accounting universes).
- **Campaign:** `evidence/campaigns/2026-10-05-phase9-store-fdb2845/`
  (branch `phase9` commit `fdb2845`).

## Context

ADR-0020 froze a content-addressed `ObjectStore`, a store-backed descriptor form
(`EXTERNAL_REF`), and three accounting universes that must never be conflated:

```text
S (standalone)        = Σ_i |serialize(d_i)|                       # whole file
U (unique reachable)  = Σ_i |serialize(e_i)| + Σ_{o ∈ O} len(o)    # shared once
A (amortized)         = Σ_i (|serialize(e_i)| + Σ_{o ∈ reach(i)} len(o)/refcount(o))
```

`S` is the only universe comparable to a per-file compressor. This ADR records
what the Phase-9.3 cohort court actually measured. The cohort is locally
generated (no third-party bytes): 37 files, 5,579,469 B source, 10 strata
(`p7` Phase-7 producer subset, `repeat-bin`, `repeat-pdf`, `shared-payload`,
`shared-bin`, `dup-resource`, `reexport`, `one-changed`, `incremental`,
`shifted`). Every standalone descriptor and every store-backed root was decoded
and `cmp`-ed byte-exact (37/37); `store account` closure reported zero dangling.

## Decision

### Measured result

```text
source_bytes                    5,579,469
standalone S                    3,762,694
unique reachable U              3,369,900   (== amortized A)
unique object bytes               786,501
store physical bytes              786,501
per-file min LZ (gzip/zstd/xz/brotli)   1,304,307
  gzip -9 1,495,074 | zstd -19 1,334,444 | xz -9e 1,307,612 | brotli -q11 1,306,498
generic CDC, borg 1.2.4, chunker 10,15,11,127, --compression none   771,383
generic CDC, same chunks, --compression zstd,19                     210,836
```

**The store does not win the cross-document axis on this cohort.** `U` (3.37 MB)
is **larger** than per-file min LZ (1.30 MB) and larger than the strongest
content-defined-chunk dedup (0.77 MB raw, 0.21 MB compressed). `U` also exceeds
its own object bytes by ~2.58 MB of root framing, because the roots carry the
document's entropy channels and program bytes. Object-granularity sharing beats
*per-file LZ* only where duplicate files make the whole-object dedup pay; it must
not be described as compression.

### Per-stratum verdict (`vs LZ` / `vs CDC raw` / `vs CDC+zstd`)

| stratum | files | S | U | LZ | CDC raw | CDC zstd | vs LZ | vs CDC |
|---|---:|---:|---:|---:|---:|---:|---|---|
| repeat-bin | 4 | 526,104 | 133,048 | 524,308 | 140,640 | 133,865 | WIN | WIN |
| dup-resource | 1 | 56,944 | 56,984 | 3,454 | 101,586 | 11,531 | LOSS | (WIN raw / LOSS zstd) |
| shared-bin | 5 | 657,670 | 657,870 | 655,425 | 147,253 | 138,450 | TIE | LOSS |
| shared-payload | 5 | 800,210 | 800,210 | 34,591 | 285,257 | 28,195 | LOSS | LOSS |
| one-changed | 2 | 320,109 | 320,109 | 14,040 | 270,984 | 27,019 | LOSS | LOSS |
| incremental | 5 | 246,454 | 246,454 | 6,793 | 51,192 | 7,079 | LOSS | LOSS |
| p7 | 6 | 362,526 | 362,577 | 25,042 | 226,918 | 31,867 | LOSS | LOSS |
| reexport | 3 | 173,863 | 173,863 | 15,184 | 96,854 | 17,856 | LOSS | LOSS |
| repeat-pdf | 4 | 298,720 | 298,720 | 11,536 | 82,106 | 9,198 | LOSS | LOSS |
| shifted | 2 | 320,094 | 320,094 | 13,934 | 288,854 | 28,867 | LOSS | LOSS |

- **The one genuine win is `repeat-bin`**: four byte-identical opaque binaries.
  A RAW-winner descriptor stores the whole file as one object, so the store keeps
  one copy (`U = 133,048 B`) where per-file LZ pays 524,308 B and borg's chunked
  dedup pays 140,640 B raw / 133,865 B compressed. Whole-object dedup edges out
  chunk dedup by ~0.6 % on this stratum.
- **`shared-payload` — the expected win fixture — loses.** Five documents embed
  the same ~200 KB stream, but the auto winner codes the bulk in entropy channels
  and program `INLINE` ops, not in the object table, so `externalize` has nothing
  to share (`U = S = 800,210 B`); CDC separates the payload (`285,257 B`).
- **`shared-bin` loses to CDC.** One 128 KB payload under five distinct 8-byte
  prefixes: the RAW whole-file objects differ, so the store shares nothing
  (`U = 657,870 B`) while CDC keeps the payload once (`147,253 B`).
- **`shifted` loses to CDC** (the pre-registered control): an inserted byte breaks
  whole-object equality, so `U = 320,094 B` while the rolling-hash chunker keeps
  `288,854 B`.
- **`repeat-pdf` does no dedup at all.** Identical copies of a channels-winner
  PDF externalize to zero objects, so `U = S = 298,720 B`; CDC keeps `82,106 B`.
- `dup-resource` beats *raw* CDC only because a single compressed descriptor
  (56,984 B) is smaller than the same file stored as uncompressed chunks
  (101,586 B); it loses to per-file LZ (3,454 B) and to compressed CDC (11,531 B).

### Cause (measured, not hypothetical)

`externalize` replaces exactly the entries of `Descriptor.objects`. The
auto-complete-cost winner for most PDFs is a channels/program candidate
(`BYTE_RANS`, `PDF_DEFLATE_REPLAY_RANS`) whose bulk lives in `ENTROPY_CHANNEL`
payloads and `Op::Inline` bytes inside the `GRAPH`, neither of which is an
object-table entry. The object table is therefore usually either empty (0 objects;
`repeat-pdf`, `shared-payload`, `one-changed`, `incremental`, `reexport`) or just
the whole file (one RAW object; `repeat-bin`, `shared-bin`). Sharing is
whole-object-granular and coarse; generic CDC operates at sub-object granularity
and therefore captures the same sharing (and near-duplicate sharing) that the
store misses.

## Consequences

- **Claim discipline (normative).** Cross-document sharing is reported as store
  *amortization*, never "compression"; `S`, `U`, and `A` are reported separately;
  a store root is never compared to a whole document; the losing strata and the
  fact that CDC captures the same sharing are stated wherever the Phase-9 result
  is cited.
- The measured Phase-9 store axis is **RECORDED (negative)**: it does not beat
  per-file LZ or generic CDC on this cohort. The `ObjectStore` mechanism and the
  exactness of the store-backed form are unaffected and remain correct (37/37
  byte-exact, closure valid).
- A future positive would require the *shareable unit* to be finer than a whole
  DRA object — e.g. externalizing channel payloads or sub-object chunks — which
  is a representation change, not implied by the current candidate set. Until
  such a change is measured, the honest result stands: **generic CDC is the
  stronger cross-document baseline.**

## References

- `evidence/campaigns/2026-10-05-phase9-store-fdb2845/` (`manifest.json`,
  `environment.json`, `cohort.json`, `results.json`, `report.md`,
  `cdc-sweep.json`, `perfile.jsonl`)
- `tools/store-cohort.sh`, `tools/store-court.sh`, `tools/chunk-dedup.sh`
- `docs/evidence/phase9-store-report.md`
- ADR-0017 (per-file LZ is the whole-file comparator), ADR-0020 (store contract)
