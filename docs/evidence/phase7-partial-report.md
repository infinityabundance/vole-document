# Phase 7.3 — partial materialization / observation views: query-cost result

> **Separately scoped measurement — nothing in `phase7-corpus-report.md` is
> rewritten.** Receipt: `evidence/campaigns/2026-10-05-phase7-partial-a5764c9/`;
> drivers `tools/partial-court.sh`, `tools/partial-table.jq`. No wire format or
> candidate semantics changed; the only new encode-time code is the
> `pdf-make-large` corpus generator.

Phase 7.0c (ADR-0017) retired VOLE's whole-file *compression* claim: the best
VOLE lane lost to gzip/zstd/xz/brotli on **0/27** files. This campaign measures
the pivoted axis — **random-access / query cost** — on a large, multi-object PDF.

## Corpus

`large.pdf` = 33,789,340 B (32.22 MiB), SHA-256 `983915ae…12c205a`, from
`pdf-make-large DIR 800`: **800** `FlateDecode` streams, **1603** indirect
objects, each stream distinct. `qpdf --check` rc 0; Ghostscript renders; deflate
census 800 replayed / 0 declined. Streams are level-0 (stored) zlib — the
weak-producer regime — so this is a random-access stress corpus, not a survey of
strong real-world DEFLATE.

## Complete cost (still a loss, recorded on the same corpus)

| candidate | bytes | ratio |
| --- | ---: | ---: |
| source | 33,789,340 | 1.000 |
| **xz -9e** | **5,841,896** | 0.173 |
| brotli -q 11 | 6,101,311 | 0.181 |
| zstd -19 --long=27 | 6,184,771 | 0.183 |
| gzip -9 | 9,999,924 | 0.296 |
| best VOLE (`PDF_DEFLATE_REPLAY_RANS`) | 17,392,713 | 0.515 |
| `PDF_DEFLATE_REPLAY_RANS_INDEXED` | 17,539,424 | 0.519 |
| `BYTE_RANS` | 17,773,281 | 0.526 |
| `PDF_PHYSICAL` | 33,829,848 | 1.001 |

The best VOLE lane is **2.98×** xz; the index record costs **146,711 B** extra.

## Query court — a scoped positive on decode work, no I/O win

18 pre-registered queries (6 byte-ranges at 0/1/8/16/24/31 MiB + 8 `--pdf-stream`
+ 4 `--pdf-object`), all **18/18 byte-exact**. Start/middle/end
(`VOLE bytes` = `descriptor_bytes_traversed + entropy_bytes_decoded`):

| query | a (B) | VOLE bytes | VOLE CPU s | VOLE RSS kB | gzip infl | gzip CPU s | zstd CPU s | xz CPU s |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| byte-0 (start) | 0 | 412,161 | 0.02 | 37,812 | 256 | 0.00 | 0.00 | 0.00 |
| byte-16 MiB (mid) | 16,777,216 | 455,155 | 0.03 | 38,036 | 16,777,472 | 0.05 | 0.00 | 0.13 |
| stream-1525 (end) | 32,069,777 | 454,215 | 0.02 | 38,116 | 32,111,758 | 0.11 | 0.01 | 0.26 |

**Wins.** For any query at ≥ ~2 MiB, VOLE touches ~0.41–0.46 MB regardless of
offset while gzip inflates `a+len`; in the late region (≥ 50 % in) that is
**1.4–2.3 %** of gzip's bytes (~20–70× less) and VOLE is **~2–5× faster than
gzip** and **~4–13× faster than xz** on CPU. Only 1 of 800 channels is decoded
and 1–3 of 9621 ops evaluated per mid/late query.

**Losses (recorded).** In the early region (≤ ~8 MiB) the constant ~0.02–0.03 s
/ ~38 MB descriptor parse loses to codecs that process 256 B–8 MB; **zstd's raw
decoder is faster than VOLE at every point** (≤ 0.01 s); VOLE's peak RSS is
~38 MB versus gzip's ~1.2 MB; and whole-file size is still 2.98× xz.

**The v1 caveat.** `view` reads and parses the **whole** framed descriptor
(`fs::read`), so on-disk I/O is not reduced: at 31 MiB gzip reads a ~9.8 MB
compressed prefix while VOLE reads its entire **17.5 MB** `.voldoc`.
`descriptor_bytes_traversed` is a **CPU-side approximation**, not a bytes-read
figure. **There is no I/O win yet**, and an mmap/seek reader is the prerequisite
for any such claim.

**Verdict: scoped positive (decode CPU/allocation), no whole-file or I/O win.**
H1 is met vs gzip and xz for the late region (8/8 on bytes and CPU) and not met
vs zstd or in the early region. ADR-0018.
