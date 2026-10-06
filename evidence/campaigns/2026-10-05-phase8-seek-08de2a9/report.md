# Phase 8.3 — seek-based partial I/O: bytes-read court (2026-10-05, `08de2a9`)

**Scope.** The decisive measurement of Phase 8: does a seeked `view` become a
**bytes-read** win, not just a decode-CPU win? Phase 7.3 (`ADR-0018`) served
partial views but the CLI `fs::read` the whole 17.5 MB descriptor, so
`descriptor_bytes_traversed` was a CPU-side approximation and on-disk I/O was
**not** reduced. Phase 8.1 added an optional seek `DIRECTORY` record and Phase
8.2 a `Read + Seek` reader; Phase 8.3 makes the CLI actually seek (peek the
64-byte header, then hand the handle to the seek reader) and measures.

**Receipt.** `evidence/campaigns/2026-10-05-phase8-seek-08de2a9/`
(`environment.json`, `corpus.json`, `results.json`, `queries.jsonl`,
`query-table.md`, `large-baselines.json`, this report). Drivers:
`tools/seek-court.sh`, `tools/seek-table.jq`.

## Corpus and descriptor (locally generated, no third-party bytes)

`large.pdf` = 33,789,340 B (32.22 MiB), SHA-256
`983915ae…12c205a` — **byte-identical** to the Phase-7 corpus, so the campaigns
are directly comparable. 800 pages, 800 lone-`FlateDecode` streams, 1603
indirect objects; `qpdf --check` rc 0, Ghostscript renders; deflate census 800
replayed / 0 declined.

`large.seek.voldoc` (`encode --force pdf-deflate-replay-rans-indexed`) =
**17,566,832 B**. It adds **174,101 B** over the non-seek indexed base —
exactly the `OBSERVATION_INDEX` (146,711 B) + the new `DIRECTORY` (27,390 B =
12 B framing + 27,378 B payload, 1,608 locators). The directory sits at fixed
offset 64, is `FLAG_OPTIONAL`, and every locator is cross-checked against the
record framing; a lying directory is rejected, never trusted.

## Whole-file (complete-cost) — still a loss, kept on a separate axis

| candidate | bytes | vs source |
| --- | ---: | ---: |
| source | 33,789,340 | 1.000 |
| **xz -9e** | **5,841,896** | 0.173 |
| brotli -q 11 | 6,101,311 | 0.181 |
| zstd -19 --long=27 | 6,184,771 | 0.183 |
| gzip -9 | 9,999,924 | 0.296 |
| best VOLE (`PDF_DEFLATE_REPLAY_RANS`) | 17,392,731 | 0.515 |
| `PDF_DEFLATE_REPLAY_RANS_INDEXED` seek (forced) | **17,566,832** | 0.520 |

The best VOLE lane is **3.01×** the best generic (xz). Non-seek lanes are +18 B
versus Phase 7 solely from the `+seek-directory-v1` UNIVERSE suffix. The
directory buys query capability, not size.

## Query court (pre-registered, 18 points, all byte-exact)

18 queries: 6 byte-ranges (256 B at 0/1/8/16/24/31 MiB) + 8 `--pdf-stream` +
4 `--pdf-object`. Frozen before measuring. Every served slice is `cmp`-identical
to the source. `bytes_read` is the reader's instrumented `CountingReader`;
`strace B` is the descriptor-file-attributed read() cross-check.

| query | a (B) | len | VOLE bytes_read | VOLE strace B | VOLE syscalls | VOLE CPU s | VOLE RSS kB | gzip compRead | gzip CPU s | gzip RSS kB | zstd compRead | zstd CPU s | zstd RSS kB | xz compRead | xz CPU s | xz RSS kB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| byte-0 (start) | 0 | 256 | 439679 | 439743 | 14 | 0 | 3872 | 327680 | 0 | 972 | 724992 | 0 | 2944 | 73728 | 0 | 1284 |
| byte-1048576 | 1048576 | 256 | 460861 | 460925 | 23 | 0 | 3788 | 589824 | 0 | 1248 | 921600 | 0 | 5036 | 253952 | 0 | 2292 |
| byte-8388608 | 8388608 | 256 | 461256 | 461320 | 23 | 0 | 3880 | 2686976 | 0.02 | 1112 | 2297856 | 0 | 11152 | 1515520 | 0.07 | 10732 |
| byte-16777216 (mid) | 16777216 | 256 | 461345 | 461409 | 23 | 0 | 3804 | 5308416 | 0.05 | 1244 | 3739648 | 0.01 | 20740 | 2965504 | 0.13 | 18192 |
| byte-25165824 | 25165824 | 256 | 461367 | 461431 | 23 | 0 | 3860 | 7667712 | 0.08 | 1272 | 5312512 | 0.01 | 29640 | 4423680 | 0.2 | 27124 |
| byte-32505856 (end) | 32505856 | 256 | 460713 | 460777 | 23 | 0 | 3864 | 9764864 | 0.11 | 1272 | 6184771 | 0.01 | 36392 | 5693440 | 0.26 | 33204 |
| stream-1525 (end) | 32069777 | 41981 | 460875 | 460939 | 23 | 0 | 3908 | 9764864 | 0.11 | 1092 | 6184771 | 0.01 | 35952 | 5627904 | 0.26 | 34036 |
| object-84 (start) | 1694184 | 127 | 439679 | 439743 | 14 | 0 | 3868 | 589824 | 0 | 1236 | 987136 | 0 | 5068 | 360448 | 0.01 | 4364 |
| object-1332 (end) | 28019346 | 131 | 439679 | 439743 | 14 | 0 | 3676 | 8454144 | 0.09 | 972 | 5836800 | 0.01 | 31636 | 4915200 | 0.22 | 30692 |

Full 18-row table: `query-table.md`. `compRead` = compressed bytes the decoder
must read to reach the same output offset (`pv`, includes bounded read-ahead);
CPU = user+sys.

## The floor is exactly GRAPH + INDEX + DIRECTORY + framing

At `a = 0` VOLE reads **439,679 B** = header 64 + DIRECTORY 27,390 + GRAPH
265,462 + OBSERVATION_INDEX 146,711 + INTEGRITY 52. A query that needs no
channel reads only this floor; a query that needs a channel adds that channel's
one referenced record plus its model (e.g. byte-31 MiB = 439,679 + object 40 +
channel 20,896 + model 98 = **460,713 B**). This floor is **constant** in the
offset and does not shrink with output size.

## Verdict — a bytes-read win for mid/late queries; explicitly loses early

- **H1 (bytes_read ≪ descriptor):** met for **all 18** queries — 439,679–461,367 B
  is ≤ 2.6 % of the 17,566,832 B descriptor, far under the 5 % (878,342 B)
  threshold even at `a = 0`. 1–3 of 9,621 ops evaluated; 0–1 of 800 channels
  decoded.
- **H2 (beat gzip prefix at ≥ 50 % in):** met for **8/8** late queries. At
  31 MiB the seeked view reads **460,713 B vs gzip's 9,764,864 B** (4.7 %, ~21×
  fewer) and vs zstd 6,184,771 B / xz 5,693,440 B (~12–13× fewer). At ≥ 8 MiB it
  beats gzip on bytes for **17/18** queries, zstd for **18/18**, xz for 14/18.
- **CPU and RSS also improve.** CPU is ≤ 0.005 s (reported 0.00) at every
  offset versus gzip 0.11 s / xz 0.26 s at 31 MiB. Peak RSS is **3.6–4.0 MB**,
  down from Phase 7's ~38 MB (which read and parsed the whole descriptor), now
  comparable to gzip's ~1.2 MB and far below xz/zstd's ~33–36 MB late.
- **Where it loses (recorded, not hidden).**
  - `a = 0`: VOLE 439,679 B **loses to gzip** (327,680 B) and xz (73,728 B):
    the constant floor exceeds the first bytes the decoders read.
  - Early queries (≤ ~1.7 MiB): VOLE **loses to xz** (byte-1 MiB, stream-85,
    object-84; xz prefix 253,952–368,640 B) because xz's compressed prefix is
    tiny and its read-ahead minimal. VOLE still beats early zstd here.
  - **Small descriptors (not tested here):** the floor is constant, so for a
    descriptor smaller than ~9 MB the GRAPH+INDEX+DIRECTORY floor would exceed
    the descriptor's own useful prefix; the seek form is a large-document
    mechanism, not a small-file one. `max_directory_bytes` (1 MiB) bounds the
    directory's growth on record-heavy descriptors.
  - **Page-cache caveat:** `strace` and `pv` count bytes returned by
    `read()`, which the page cache does not change; they change latency. The
    strace total equals the instrumented count + exactly 64 B (the CLI's
    deliberate header peek), confirming no hidden whole-file read.

**Claim.** On one locally generated 33.8 MB / 800-object PDF, the seeked `view`
reduces **bytes read** to serve a narrow late observation from the full
17.5 MB descriptor to a constant ~0.46 MB — a real on-disk-I/O win over
sequential gzip/zstd/xz decompression, alongside the CPU and RSS reductions.
It loses on bytes at the start of the file and versus xz's tiny early
compressed prefix, exactly as the 8.0 contract pre-registered. No population
claim; the corpus is the weak-producer stress regime.
