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
unique reachable U (auto)       3,369,900   (== amortized A)
unique object bytes               786,501
store physical bytes              786,501
per-file min LZ (gzip/zstd/xz/brotli)   1,304,307
  gzip -9 1,495,074 | zstd -19 1,334,444 | xz -9e 1,307,612 | brotli -q11 1,306,498
generic CDC, borg 1.2.4, chunker 10,15,11,127, --compression none   771,383
  (deterministic: run1 == run2)
generic CDC, same chunks, --compression zstd,19   210,835-210,840
  (NON-deterministic across runs; not a stable citation)
U forced, --force pdf-deflate-replay            2,360,054
U, per-stratum candidate oracle (approx.)       ~2,537,730
```

**The store loses the cross-document axis on this cohort.** The negative is
**robust and reproducible** — it survives forcing the finest candidate the current
set contains and a per-stratum oracle — but its *size* is **partly an artifact of
candidate selection and externalization granularity, not of the mechanism's
limits**. The auto complete-cost winner codes most PDF bulk in `ENTROPY_CHANNEL`
payloads and `GRAPH` inlines, which `externalize` (it replaces only
`Descriptor.objects`) does not touch; the auto winner therefore has 0-1 objects per
file and `U = 3,369,900 B`. Forcing `PDF_DEFLATE_REPLAY` — a candidate **in the
current set** — emits one object per deflate stream and lowers the global `U` to
**2,360,054 B**; the best per-stratum candidate choice (oracle) is ~**2,537,730 B**.
Both still lose to per-file min LZ (1.30 MB) and to raw CDC (0.77 MB), and the
store still loses to LZ and compressed CDC on `shared-payload` — so the verdict
stands — but the earlier claim that no current candidate could share finer than a
whole file was **false** (see Consequences). `U` also exceeds its own object bytes
by ~2.58 MB of root framing, because the roots carry the document's entropy
channels and program bytes. Object-granularity sharing beats *per-file LZ* only
where duplicate files make whole-object dedup pay; it must not be described as
compression.

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

- **`repeat-bin` — the only auto-candidate win**: four byte-identical opaque
  binaries. A RAW-winner descriptor stores the whole file as one object, so the
  store keeps one copy (`U = 133,048 B`, -74.6 % vs LZ) where per-file LZ pays
  524,308 B and borg's chunked dedup pays 140,640 B raw (-5.4 %) / 133,865 B
  compressed (-0.6 %, ~0.8 KB — real but marginal). Whole-object dedup edges out
  chunk dedup by ~0.6 % on this stratum.
- **A second win appears only when a finer-object candidate is forced.** Under
  `--force pdf-deflate-replay`, `shared-payload` drops from `U = 800,210 B` to
  `264,139 B` (2 unique objects) against CDC raw `285,257 B` — a win over **raw**
  CDC — though it still loses to per-file LZ (`34,591 B`) and CDC+zstd
  (`28,195 B`). That candidate is in the *current* set; the auto winner simply
  never selects it. `shared-bin` (`657,870` vs `147,253`) remains a pure
  granularity loss.
- **`shared-payload` — the expected win fixture — loses in the auto run.** Five
  documents embed the same ~200 KB stream, but the auto winner codes the bulk in
  entropy channels and program `INLINE` ops, not in the object table, so
  `externalize` has nothing to share (`U = S = 800,210 B`); CDC separates the
  payload (`285,257 B`). Forcing `PDF_DEFLATE_REPLAY` (one object per deflate
  stream) **flips this stratum** to `264,139 B`, beating raw CDC but not LZ or
  compressed CDC (above).
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
store misses. The finest shareable unit the current candidate set can *emit* is
therefore the whole (per-file) deflate stream: `PDF_DEFLATE_REPLAY` produces one
object per stream — still not a sub-stream chunk, and never more than one object
per file for any current candidate. Finer sharing (individual channel payloads,
arbitrary sub-object chunks) remains a representation change.

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
- **The current candidate set already contains a finer-object candidate.**
  `PDF_DEFLATE_REPLAY` is in the set and, when forced, emits one object per
  deflate stream instead of one per file: that lowers the global `U` from
  3,369,900 B to 2,360,054 B and flips `shared-payload` to a win over **raw** CDC
  (`264,139` vs `285,257` B, 2 unique objects). The earlier claim that a finer
  shareable unit was "not implied by the current candidate set" is therefore
  **false**. What the current set does *not* contain is any candidate that emits
  **more than one object per file**: the finest existing shareable unit is the
  whole deflate stream. Finer sharing still (externalizing individual
  `ENTROPY_CHANNEL` payloads or arbitrary sub-object chunks) remains a
  representation change. The negative survives both candidate selection and
  granularity: even the best forced choice (2,360,054 B) and a per-stratum oracle
  (~2,537,730 B) lose to per-file min LZ (1,304,307 B) and raw CDC (771,383 B),
  and the store still loses to LZ and compressed CDC on `shared-payload`. Until a
  finer representation is measured, the honest result stands: **generic CDC is
  the stronger cross-document baseline.**
- **CDC-compressed determinism caveat.** The raw CDC figure (`--compression none`,
  771,383 B) is deterministic (run1 == run2) and is the primary comparison. The
  *compressed* CDC figure is **not** stable across runs (observed 210,835-210,840
  B); it is cited (where at all) only as a non-deterministic range, never as a
  fixed number. This does not change any verdict.

## References

- `evidence/campaigns/2026-10-05-phase9-store-fdb2845/` (`manifest.json`,
  `environment.json`, `cohort.json`, `results.json`, `report.md`,
  `cdc-sweep.json`, `perfile.jsonl`)
- `tools/store-cohort.sh`, `tools/store-court.sh`, `tools/chunk-dedup.sh`
- `docs/evidence/phase9-store-report.md`, `docs/evidence/phase9-skeptic-review.md`
- ADR-0017 (per-file LZ is the whole-file comparator), ADR-0020 (store contract)
