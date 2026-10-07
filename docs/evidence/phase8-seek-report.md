# Phase 8.3 — seek-based partial I/O: bytes-read result

> **Separately scoped measurement — nothing in `phase7-partial-report.md` is
> rewritten.** Receipt: `evidence/campaigns/2026-10-05-phase8-seek-08de2a9/`;
> drivers `tools/seek-court.sh`, `tools/seek-table.jq`. The DRA, candidates, and
> entropy semantics are unchanged from Phase 7; Phase 8.1/8.2 added the optional
> seek `DIRECTORY` record and the `Read + Seek` reader.

Phase 7.3 (ADR-0018) measured a partial-materialization **decode-CPU** win but
`view` read and parsed the whole framed descriptor, so on-disk I/O was **not**
reduced and there was no bytes-read win. This campaign closes that gap: the CLI
now peeks only the 64-byte header and serves via the seek reader, and it measures
the actual bytes read.

## Corpus and descriptor

`large.pdf` = 33,789,340 B (32.22 MiB), SHA-256 `983915ae…12c205a` — regenerated
under Phase 8 and **byte-identical** to the Phase-7 corpus (800 streams, 1603
indirect objects; `qpdf --check` rc 0; Ghostscript renders; deflate census 800
replayed / 0 declined) — so the two campaigns are directly comparable.

`large.seek.voldoc` (`encode --force pdf-deflate-replay-rans-indexed`) =
**17,566,832 B**. It is **174,101 B** larger than the non-seek indexed base:
the `OBSERVATION_INDEX` (146,711 B) + the new `DIRECTORY` (27,390 B = 12 B
framing + 27,378 B payload, 1,608 locators, at fixed offset 64, `FLAG_OPTIONAL`).

## Complete cost (still a loss, kept on a separate axis)

| candidate | bytes | ratio |
| --- | ---: | ---: |
| source | 33,789,340 | 1.000 |
| **xz -9e** | **5,841,896** | 0.173 |
| brotli -q 11 | 6,101,311 | 0.181 |
| zstd -19 --long=27 | 6,184,771 | 0.183 |
| gzip -9 | 9,999,924 | 0.296 |
| best VOLE (`PDF_DEFLATE_REPLAY_RANS`) | 17,392,731 | 0.515 |
| `PDF_DEFLATE_REPLAY_RANS_INDEXED` seek (forced) | 17,566,832 | 0.520 |

The best VOLE lane is **3.01×** xz. Non-seek lanes re-base +18 B vs Phase 7 only
from the `+seek-directory-v1` UNIVERSE suffix.

## Query court — a bytes-read win **versus non-seekable sequential codecs**

18 pre-registered queries (6 byte-ranges at 0/1/8/16/24/31 MiB + 8 `--pdf-stream`
+ 4 `--pdf-object`), all **18/18 byte-exact**. Start/middle/end (`compRead` =
compressed bytes the decoder must read to the same offset, `pv`):

| query | a (B) | VOLE bytes_read | VOLE syscalls | VOLE CPU s | VOLE RSS kB | gzip compRead | gzip CPU s | zstd compRead | xz compRead |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| byte-0 (start) | 0 | 439,679 | 14 | 0.00 | 3,872 | 327,680 | 0.00 | 724,992 | 73,728 |
| byte-16 MiB (mid) | 16,777,216 | 461,345 | 23 | 0.00 | 3,804 | 5,308,416 | 0.05 | 3,739,648 | 2,965,504 |
| stream-1525 (end) | 32,069,777 | 460,875 | 23 | 0.00 | 3,908 | 9,764,864 | 0.11 | 6,184,771 | 5,627,904 |

The `strace` cross-check, attributed to the descriptor file, equals the
instrumented count **+ exactly 64 B** (the CLI header peek) for every query — no
hidden whole-file read. The floor at `a = 0` is exactly header 64 + DIRECTORY
27,390 + GRAPH 265,462 + OBSERVATION_INDEX 146,711 + INTEGRITY 52 = **439,679 B**;
a channel-bearing query adds one referenced channel + model.

**Wins (versus non-seekable sequential codecs only).** H1 met **18/18**:
439,679–461,367 B is ≤ 2.6 % of the descriptor (threshold 5 % / 878,342 B) for
every query, including `a = 0`. H2 met **8/8** in the late region: at 31 MiB
460,713 B vs gzip's 9,764,864 B (**4.7 %**, ~21× fewer) and ~12–13× fewer than
zstd/xz. At ≥ 8 MiB it beats gzip 17/18, zstd 18/18, xz 14/18. CPU is ~0.00 s vs
gzip 0.11 s / xz 0.26 s late; peak RSS is **3.6–4.0 MB**, down from Phase 7's
~38 MB.

**The honest random-access baseline (Phase 8.4 amendment).** The win above is
only against **non-seekable sequential** codecs. Against **seekable/blocked**
formats, for the same late query (`--byte-range=32505856:256`; VOLE 460,713 B):
**bgzip (BGZF) 23,808 B** (~19× fewer), **xz --block-size=64KiB 15,344 B**
(~30× fewer) and **1 MiB 179,892 B** (~2.6× fewer) all read **less**; only
blocked xz with **4 MiB** blocks (708,612 B) and **pixz** (2,810,832 B; 16 MiB
default blocks) read more. BGZF's archive is also **7,995,600 B — smaller than
the 17,566,832 B seekable descriptor**. So this is **not** a general
random-access-I/O win. Full table: `seekable-table.md`; amendment report
`seekable-report.md` in the same receipt.

**The read is a floor, not a small read.** `view` touches the header and the
DIRECTORY, GRAPH, OBSERVATION_INDEX, INTEGRITY record *classes* plus the one
referenced object/channel/model — but the constant floor is dominated by
GRAPH + OBSERVATION_INDEX + DIRECTORY (~439 KB), so a 256-byte request still
incurs ~440 KB (~1,700× the requested bytes).

**Validator caveat.** A **referenced** channel's `decoded_length` is
cross-checked against its record (a disagreement is `InvalidContainer`); an
**unreferenced** `CHANNEL_LENGTHS` entry is not validated. This is benign —
`analyze_ops` never uses an unreferenced length, so it cannot change a served
byte. It is a gap in the checks, not in the bytes.

**Losses (recorded).** At `a = 0` the constant floor exceeds gzip's first bytes
(327,680 B) and xz's (73,728 B); at early queries (≤ ~1.7 MiB) it loses to xz's
tiny compressed prefix (253,952–368,640 B); and versus seekable/blocked formats
it loses across the late region (above). The floor is constant in the offset, so
it would dominate a descriptor smaller than ~9 MB — a large-document mechanism,
not a small-file one. The codecs' `compRead` is measured at a pipe and includes
bounded read-ahead (an upper bound); OS page cache changes wall time, not read()
byte counts.

**Verdict: a scoped bytes-read win versus non-seekable sequential codecs** for
mid/late queries on one locally generated 33.8 MB PDF, alongside CPU and RSS
reductions. It is **not** a general random-access-I/O win: a purpose-built
seekable/blocked format reads 2.6–30× less for the same late query, and it loses
at the start and early queries. No population claim. ADR-0019;
`docs/reviews/phase8-skeptic-review.md`.
