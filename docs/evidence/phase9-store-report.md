# Phase 9.3 — cross-document content-addressed store: measured result

Campaign: `evidence/campaigns/2026-10-05-phase9-store-fdb2845/` (branch `phase9`,
commit `fdb2845`). ADR: [0021](../adr/0021-cross-document-sharing-result.md).
Independent adversarial review: [phase9-skeptic-review.md](../reviews/phase9-skeptic-review.md).

## Question

Does the cross-document `EmbeddedStore` (ADR-0020) beat, per the store contract,
(a) per-file generic LZ and (b) a generic **content-defined-chunk** dedup, over a
reproducible cohort that deliberately contains sharing? The pre-registered
falsifier is the `shifted` stratum, where CDC should win and the store must lose.

## Method

All measurements ran in the pinned Docker services (`compose.yaml`); nothing ran
on the host. Cohort: 37 locally generated files (5,579,469 B, 10 strata) built by
`tools/store-cohort.sh` from our own deterministic inputs (Phase-7 producer subset
copied from `tools/pdf-corpus.sh`; no third-party bytes; bytes gitignored, ledger
committed as `cohort.json`). `tools/store-court.sh`:

1. `encode` each file (auto complete-cost winner) → standalone descriptor;
2. `store put` every root into one `EmbeddedStore` (externalize the object table);
3. `store account` over the whole cohort and per stratum → the three universes;
4. `tools/baselines.sh` for the per-file LZ ladder;
5. `tools/chunk-dedup.sh` for the strongest CDC (borg, swept params).

Exactness: every standalone descriptor (`verify` + `decode` + `cmp`) and every
store-backed root (`decode --store` + `cmp`) was byte-exact (37/37); `store
account` closure reported `dangling = 0`. (`verify` has no resolver and is run on
the standalone form only; the store-backed form is gated by `decode --store`,
which re-checks the whole-source `INTEGRITY` SHA-256.)

## The CDC baseline

Pinned `borgbackup 1.2.4-1` (Debian bookworm), `--compression none` so dedup is
isolated from chunk compression. A coarse chunker would be a strawman, so the
court sweeps fixed parameter sets and keeps the smallest unique bytes:

| chunker params (min_exp,max_exp,mask,window) | unique bytes |
|---|---:|
| 19,23,21,4095 (borg default; ~512 KiB min chunk) | 4,272,988 |
| 11,16,13,4095 | 1,116,709 |
| 9,16,11,4095 | 874,857 |
| 10,15,11,1023 | 779,351 |
| 10,15,11,511 | 772,612 |
| **10,15,11,127 (kept)** | **771,359–771,383** |

Determinism was verified by running each configuration twice and comparing
`unique_csize` (identical) **for `--compression none`**. The same winning params
with chunk compression on (`--compression zstd,19`) are **not deterministic**
across runs (observed 210,835–210,840 B), so that figure is cited only as a
non-deterministic range and never as a fixed number. The primary comparison uses
the deterministic raw number.

## Totals (bytes)

| metric | value |
|---|---:|
| source_bytes | 5,579,469 |
| standalone S | 3,762,694 |
| unique reachable U | 3,369,900 |
| amortized A (= U) | 3,369,900 |
| unique object bytes | 786,501 |
| store physical bytes | 786,501 |
| gzip -9 sum | 1,495,074 |
| zstd -19 sum | 1,334,444 |
| xz -9e sum | 1,307,612 |
| brotli -q11 sum | 1,306,498 |
| per-file min LZ sum | 1,304,307 |
| CDC raw (none, deterministic) | 771,383 |
| CDC (zstd,19 chunks, non-deterministic) | 210,835–210,840 |

**Verdict: LOSS on the store axis.** U loses to per-file min LZ, to raw CDC, and
to compressed CDC; `U` is even ~2.58 MB larger than its own object bytes because
the store roots carry the documents' entropy channels and `GRAPH` bytes. The
negative is **robust** — it survives forcing the finest candidate in the current
set (`--force pdf-deflate-replay`, `U = 2,360,054 B`) and a per-stratum candidate
oracle (~`2,537,730 B`), both still losses vs LZ and raw CDC — but its size is
**partly an artifact of candidate selection and externalization granularity, not
of the mechanism's limits**: the auto winner emits 0–1 objects per file, so
`externalize` has almost nothing to share. Forcing `PDF_DEFLATE_REPLAY` emits one
object per deflate stream and flips `shared-payload` to `U = 264,139 B` (2 unique
objects), a win over **raw** CDC (`285,257 B`) that still loses to per-file LZ
(`34,591 B`) and CDC+zstd (`28,195 B`). The earlier claim that no current candidate
could share finer than a whole file is **false**; the finest unit the current set
can emit is the whole deflate stream. See `phase9-skeptic-review.md`.

## Per stratum (bytes)

| stratum | files | S | U | LZ | CDC raw | CDC zstd | vs LZ | vs CDC | vs CDC+zstd |
|---|---:|---:|---:|---:|---:|---:|---|---|---|
| repeat-bin | 4 | 526,104 | 133,048 | 524,308 | 140,640 | 133,863 | WIN | WIN | WIN |
| dup-resource | 1 | 56,944 | 56,984 | 3,454 | 101,586 | 11,531 | LOSS | WIN | LOSS |
| shared-bin | 5 | 657,670 | 657,870 | 655,425 | 147,253 | 138,450 | TIE | LOSS | LOSS |
| shared-payload | 5 | 800,210 | 800,210 | 34,591 | 285,257 | 28,195 | LOSS | LOSS | LOSS |
| one-changed | 2 | 320,109 | 320,109 | 14,040 | 270,984 | 27,019 | LOSS | LOSS | LOSS |
| incremental | 5 | 246,454 | 246,454 | 6,793 | 51,192 | 7,079 | LOSS | LOSS | LOSS |
| p7 | 6 | 362,526 | 362,577 | 25,042 | 226,918 | 31,867 | LOSS | LOSS | LOSS |
| reexport | 3 | 173,863 | 173,863 | 15,184 | 96,854 | 17,856 | LOSS | LOSS | LOSS |
| repeat-pdf | 4 | 298,720 | 298,720 | 11,536 | 82,106 | 9,198 | LOSS | LOSS | LOSS |
| shifted | 2 | 320,094 | 320,094 | 13,934 | 288,854 | 28,867 | LOSS | LOSS | LOSS |

WIN/TIE = U below / within 2 % of the baseline; a store root is never compared to
a whole file (`S` is the whole-file universe). These columns are the **auto**
candidate. Under `--force pdf-deflate-replay` the global `U` falls to 2,360,054 B
and `shared-payload` flips to `264,139 B` (a win over *raw* CDC only); see the
verdict above.

## What happened, mechanically

`externalize` replaces exactly `Descriptor.objects`. The auto winner for most
PDFs is a channels/program candidate (`BYTE_RANS`, `PDF_DEFLATE_REPLAY_RANS`)
whose bulk is in `ENTROPY_CHANNEL` payloads and `Op::Inline` bytes in the `GRAPH`
— neither is an object-table entry. So the object table is empty (0 objects) for
`shared-payload`, `shared-bin`'s siblings, `repeat-pdf`, `one-changed`,
`incremental`, `reexport`, or is a single whole-file RAW object for `repeat-bin`
and `shared-bin`. Sharing is therefore whole-object-granular and coarse:

- **repeat-bin** wins: one whole-file object is shared by four identical files,
  beating both per-file LZ (-74.6 %) and CDC (raw -5.4 %; compressed -0.6 %,
  ~0.8 KB — real but marginal).
- **shared-payload** (the fixture that was *expected* to win) loses in the auto
  run: the shared stream is coded in channels, so nothing is externalized; CDC
  keeps the payload once. Forcing `PDF_DEFLATE_REPLAY` emits one object per
  deflate stream and flips it to a win over raw CDC (`264,139` vs `285,257 B`),
  still losing to LZ and compressed CDC.
- **No current candidate emits more than one object per file.** The finest
  shareable unit in the set is the whole deflate stream; finer sharing
  (individual `ENTROPY_CHANNEL` payloads, sub-object chunks) is a representation
  change.
- **shared-bin** loses to CDC: distinct 8-byte prefixes change the whole-file RAW
  object, so nothing is shared.
- **shifted** loses to CDC, exactly as pre-registered: one inserted byte breaks
  whole-object equality; the rolling-hash chunker resynchronizes.
- **repeat-pdf** does no dedup at all: identical channels-winner PDFs externalize
  to zero objects.

## Honest comparison caveats

- `cdc_unique_raw` is dedup-only (no chunk compression), so it *understates* a
  compressing chunk store; the `zstd,19` number is reported alongside as a
  **non-deterministic range** (210,835–210,840 B). The VOLE loss is identical
  against both.
- The compressed CDC figure is not reproducible run-to-run, so the deterministic
  raw CDC number (`771,383 B`, run1 == run2) is the primary comparison; the
  compressed figure is marked non-deterministic wherever it appears.
- borg's repository index/metadata is not counted in `unique_csize`, whereas the
  VOLE `U` includes every store root's framing; this asymmetry favours VOLE and
  the loss stands regardless.
- `dup-resource` and `repeat-bin` are the only strata where VOLE is not behind on
  some axis; both are reported above rather than generalised.
- The cohort is one locally generated set with deliberate sharing; this is a
  scoped measurement, not a population claim.

## Reproduction

```sh
docker compose run --rm --no-TTY dev      cargo build --locked --all-features
docker compose run --rm --no-TTY tools    sh tools/pdf-corpus.sh
docker compose run --rm --no-TTY tools    sh tools/store-cohort.sh
docker compose run --rm --no-TTY baseline sh tools/store-court.sh \
    evidence/corpus/phase9 \
    evidence/campaigns/2026-10-05-phase9-store-fdb2845
```

Full artifacts: `manifest.json`, `environment.json`, `cohort.json`,
`results.json`, `report.md`, `cdc-sweep.json`, `perfile.jsonl`, `commands.txt`.
