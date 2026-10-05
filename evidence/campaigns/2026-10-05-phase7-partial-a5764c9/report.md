# Phase 7.3 — partial-materialization query court (2026-10-05, `a5764c9`)

**Scope.** This is the decisive measurement of the partial-materialization pivot:
random-access / query cost against generic compressors on a large multi-object
PDF — not whole-file size. Nothing in the wire format or candidate semantics
changed; the only new encode-time code is the `pdf-make-large` corpus generator.

**Receipt.** `evidence/campaigns/2026-10-05-phase7-partial-a5764c9/`
(`environment.json`, `corpus.json`, `results.json`, `queries.jsonl`,
`query-table.md`, this report). Drivers: `tools/partial-court.sh`,
`tools/partial-table.jq`.

## Corpus (locally generated, no third-party bytes)

`large.pdf` = 33,789,340 B (32.22 MiB), SHA-256
`983915aef64bfce2beed5bc000822a2aceb9a8c3beb0ca760de0a95ad12c205a`,
produced by `pdf-make-large DIR 800`: **800** pages, **800** lone-`FlateDecode`
streams, **1603** indirect objects, every `/Length` and xref offset correct by
construction. `qpdf --check` = rc 0 ("No syntax or stream encoding errors
found"); Ghostscript renders. Deflate census: 800 streams, **800 replayed / 0
declined**. Each stream is a real zlib stream with distinct per-stream content
(no shared plaintext), so the replay lane is live and the cheap index path
applies.

> **Corpus limitation.** Streams are level-0 (stored-block) DEFLATE, the
> weak-producer regime where replay is exact and corrections are 28 B/stream
> (deduplicated to one object). This is a large multi-object *random-access*
> stress corpus, not a survey of strong real-world DEFLATE.

## Whole-file (complete-cost) — a loss, recorded so the axes are never conflated

The complete file must losslessly recover the source, so all framing/model/index
overhead is charged. `tools/baselines.sh`, round-trip verified:

| candidate | bytes | vs source |
| --- | ---: | ---: |
| source | 33,789,340 | 1.000 |
| **xz -9e** | **5,841,896** | 0.173 |
| brotli -q 11 | 6,101,311 | 0.181 |
| zstd -19 --long=27 | 6,184,771 | 0.183 |
| gzip -9 | 9,999,924 | 0.296 |
| best VOLE (`PDF_DEFLATE_REPLAY_RANS`) | 17,392,713 | 0.515 |
| `PDF_DEFLATE_REPLAY_RANS_INDEXED` (forced) | 17,539,424 | 0.519 |
| `BYTE_RANS` (forced) | 17,773,281 | 0.526 |
| `PDF_PHYSICAL` (forced) | 33,829,848 | 1.001 |

The best VOLE lane is **2.98×** the best generic (xz). The indexed lane adds
**146,711 B** (the observation-index record) over the non-indexed lane: it buys
query capability, not size. The auto winner stays the non-indexed lane because
auto is size-based. **Complete-cost loss confirmed on this corpus; prior wins
were relative to `BYTE_RANS`.**

## Query court (pre-registered, 18 points, all byte-exact)

18 queries: 6 byte-ranges (256 B at 0/1/8/16/24/31 MiB) + 8 `--pdf-stream` +
4 `--pdf-object` spans distributed across the file. Frozen before measuring.
All 18 served slices are `cmp`-identical to the source; the descriptor `verify`s
(length + SHA-256). Full table: `query-table.md` (its `VOLE bytes` column is
`descriptor_bytes_traversed + entropy_bytes_decoded` as originally generated — the
pre-correction sum retained in the sealed raw table; the honest metric is
`descriptor_bytes_traversed` alone, shown below). Start/middle/end:

| query | a (B) | VOLE bytes | VOLE CPU s | VOLE RSS kB | gzip infl | gzip CPU s | gzip RSS kB | zstd infl | zstd CPU s | zstd RSS kB | xz infl | xz CPU s | xz RSS kB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| byte-0 (start) | 0 | 412,161 | 0.02 | 37,812 | 256 | 0.00 | 1,224 | 256 | 0.00 | 4,932 | 256 | 0.00 | 1,196 |
| byte-16 MiB (mid) | 16,777,216 | 433,672 | 0.03 | 38,036 | 16,777,472 | 0.05 | 1,280 | 16,777,472 | 0.00 | 21,656 | 16,777,472 | 0.13 | 18,924 |
| stream-1525 (end) | 32,069,777 | 433,202 | 0.02 | 38,116 | 32,111,758 | 0.11 | 1,288 | 32,111,758 | 0.01 | 35,980 | 32,111,758 | 0.26 | 33,728 |

`VOLE bytes` = `descriptor_bytes_traversed` (the primary decode-side metric); its
`entropy_bytes_decoded` breakdown is a **subset** already counted inside that
field, so the two must **not** be added. `* infl` = decompressed bytes the codec
must produce to reach the range; CPU = user+sys.

### Where VOLE wins

- **Decode work (primary metric):** for any query at ≥ ~2 MiB, VOLE touches
  ~0.41–0.43 MB regardless of offset, while gzip must inflate `a + len`. At the
  end (31 MiB / stream-1525) that is **~1.4% of gzip's bytes** (~70× fewer); in
  the late region (≥ 50% in) it is 1.4%–2.3% on **8/8** queries.
- Only **1 of 800** channels is decoded and **1–3 of 9621** ops evaluated per
  mid/late query (`work_amplification` ≈ 0.0001); early byte-0 evaluates 8
  inline ops and decodes no channel.
- **CPU time vs gzip/xz:** VOLE is ~constant 0.02–0.03 s; gzip crosses it near
  8–16 MiB and reaches 0.11 s at 31 MiB, xz 0.26 s. VOLE is **~2–5× faster**
  than gzip and **~4–13×** faster than xz on the late queries.

### Where VOLE loses

- **Early region (≤ ~8–16 MiB):** gzip/xz/zstd finish in ~0.00–0.03 s and process
  only 256 B–8 MB, while VOLE pays a constant ~0.02–0.03 s and ~38 MB just to
  read/parse the descriptor. At offset 0 VOLE processes *more* bytes (412 KB)
  than a 256 B request.
- **vs zstd:** zstd's raw decoder reaches every offset in ≤ 0.01 s, so VOLE is
  **not** faster than zstd at any point — though zstd's RSS grows to ~36 MB,
  comparable to VOLE's ~38 MB.
- **Peak RSS:** VOLE is ~38 MB constant versus gzip's ~1.2 MB; it only reaches
  parity with xz/zstd at late offsets.
- **On-disk I/O (the v1 caveat):** `view` reads and parses the **whole** framed
  descriptor (`fs::read`), so bytes read do not shrink. At 31 MiB gzip reads a
  ~9.8 MB compressed prefix while VOLE reads its entire **17.5 MB** `.voldoc`.
  `descriptor_bytes_traversed` is a **CPU-side approximation**, not a byte-read
  figure. There is therefore **no I/O win yet**.

## Verdict — a scoped positive, not an unqualified win

Random-access decode work is genuinely reduced: for mid/late queries the indexed
lane processes ~20–70× fewer bytes and is ~2–5× faster (CPU) than gzip/xz, byte
exact on all 18 points. **This is a real query-cost result.** But: it **loses in
the early region**, **loses to zstd on wall time everywhere**, **loses on peak
RSS** (38 MB vs gzip's 1.2 MB), **still loses whole-file size ~3× to xz**, and
**does not reduce on-disk I/O at all** in v1. The measured v1 win is
**decode CPU**, exactly as the contract pre-registered.

H1 (≤ 25% of gzip's bytes and ≥ 2× faster, output ≤ 1 MiB at ≥ 50% into a
≥ 32 MiB doc) is **MET vs gzip and xz** for the late region (8/8 on both
criteria) and **NOT met vs zstd** or in the early region.

**Adopt the mechanism; claim only decode work, not I/O.** The next required step
is an mmap/seek descriptor reader so `descriptor_bytes_traversed` becomes a real
byte-read figure — only then can a "bytes read" win be claimed.
