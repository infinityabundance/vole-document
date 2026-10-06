# Phase 8 — amendment: the seekable/blocked random-access baseline

> **Amendment to `report.md`; `results.json`, `report.md`, `environment.json` and
> `query-table.md` are unchanged.** New artifacts: `seekable.jsonl`,
> `seekable-table.md`, `seekable-environment.json`, this report. Drivers:
> `tools/seekable-baselines.sh`, `tools/bgzf-seek-probe.pl`,
> `tools/xz-seek-probe.pl`, `tools/xz-block-reframe.pl`, `tools/seekable-table.jq`.

## Why this amendment exists

The Phase-8.3 "bytes-read win" was measured against **non-seekable sequential**
`gzip`/`zstd`/`xz` streams — the deliberate Phase-7.3 baseline. That baseline is
honest for *sequential* decompression, but it is **not** the honest
random-access baseline. Against **seekable/blocked archive formats** VOLE does
not win generally. This amendment makes the seekable baseline reproducible and
restates the claim (see `docs/evidence/phase8-skeptic-review.md` and ADR-0019).

## Archives (whole-file, all built in the pinned `baseline` image)

| format | bytes | vs source | vs VOLE |
| --- | ---: | ---: | ---: |
| source `large.pdf` | 33,789,340 | 1.000 | — |
| `pixz` (16 MiB blocks) | 5,689,264 | 0.168 | 0.32× |
| `xz --block-size=4MiB` | 5,725,576 | 0.169 | 0.33× |
| `xz --block-size=1MiB` | 5,797,248 | 0.172 | 0.33× |
| `xz --block-size=64KiB` | 6,393,552 | 0.189 | 0.36× |
| `bgzip -l 9` (BGZF) | 7,995,600 | 0.237 | 0.46× |
| VOLE `PDF_DEFLATE_REPLAY_RANS_INDEXED` seek | 17,566,832 | 0.520 | 1.000 |

**BGZF is also smaller than VOLE's descriptor** (7,995,600 B vs 17,566,832 B), so
even the whole-file axis is lost to a *seekable* format, not just to xz.

## Query court (6 pre-registered byte ranges, 256 B each; all byte-exact)

Cost definition: the covering compressed block(s) **plus** the index/framing that
locates them (the `.gzi` file for BGZF; the xz Stream Footer + Index field for
xz/pixz). This is a *seek* cost; the xz/pixz blocks are decoded **in isolation**
(block independence demonstrated, not assumed), and every extracted slice is
`cmp`-identical to the source. `s` = user+sys decode seconds.

| query | a (B) | VOLE B | VOLE s | bgzip B | bgzip s | xz64k B | xz64k s | xz1m B | xz1m s | xz4m B | xz4m s | pixz B | pixz s |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| byte-0 | 0 | 439679 | 0 | 23332 | 0 | 14376 | 0 | 181412 | 0 | 716844 | 0.03 | 2838644 | 0.13 |
| byte-1048576 | 1048576 | 460861 | 0 | 23795 | 0 | 14860 | 0 | 184420 | 0 | 716844 | 0.03 | 2838644 | 0.13 |
| byte-8388608 | 8388608 | 461256 | 0 | 23802 | 0 | 15268 | 0 | 179568 | 0 | 709856 | 0.03 | 2838644 | 0.13 |
| byte-16777216 | 16777216 | 461345 | 0 | 23817 | 0 | 15160 | 0 | 179240 | 0 | 709320 | 0.03 | 2810832 | 0.13 |
| byte-25165824 | 25165824 | 461367 | 0 | 23791 | 0 | 14908 | 0 | 180208 | 0 | 709964 | 0.03 | 2810832 | 0.13 |
| byte-32505856 | 32505856 | 460713 | 0 | 23808 | 0 | 15344 | 0 | 179892 | 0 | 708612 | 0.03 | 2810832 | 0.13 |

These reproduce the independent reviewer's figures:
bgzip **23,808 B** (exact), xz 64 KiB **15,344 B** (reviewer 15,272),
xz 1 MiB **179,892 B** (reviewer 179,916), xz 4 MiB **708,612 B** (reviewer
708,632).

## What the late query (a = 32,505,856) now shows

| reader | bytes read | VOLE / reader |
| --- | ---: | ---: |
| VOLE seeked `view` | 460,713 | 1.00× |
| bgzip (BGZF) | 23,808 | **19.4× more** |
| xz --block-size=64KiB | 15,344 | **30.0× more** |
| xz --block-size=1MiB | 179,892 | **2.56× more** |
| xz --block-size=4MiB | 708,612 | 0.65× (VOLE reads *less*) |
| pixz (16 MiB blocks) | 2,810,832 | 0.16× (VOLE reads *less*) |

So VOLE reads **2.6–30× more** than a purpose-built seekable/blocked format for
the same late query (BGZF ~24 KB; blocked xz 15–180 KB), and it loses at offset
0 and at early queries too. It reads *less* than blocked xz with very large
blocks (4 MiB) and than pixz, whose default 16 MiB blocks make a byte-range seek
expensive — neither is the compact-block seekable regime.

## Verdict

**Not a general random-access-I/O win.** The Phase-8.3 result stands only in its
correctly scoped form: the seeked `view` reads a constant ~0.44–0.46 MB
(a floor dominated by GRAPH + OBSERVATION_INDEX + DIRECTORY), which beats
**non-seekable sequential** gzip/zstd/xz prefixes by 12–21× for late queries.
Against seekable/blocked formats it is a 2.6–30× loss on the same late query.
Whole-file size is 3.01× xz and 0.46× BGZF. Single locally generated corpus; no
population claim. `zstd --seekable` does not exist in zstd 1.5.4.
