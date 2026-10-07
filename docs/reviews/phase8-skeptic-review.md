# Phase 8 — independent adversarial review

- **Reviewer:** independent adversarial reviewer (Phase 8)
- **Scope:** the Phase-8.1/8.2 seek `DIRECTORY` + `Read + Seek` reader
  (`ADR-0019`), the Phase-8.3 bytes-read court
  (`2026-10-05-phase8-seek-08de2a9`), and the headline framing that the seeked
  `view` is a bytes-read win.
- **Method:** all controls reproduced in the pinned Docker services
  (`docker compose run --rm --no-TTY dev|tools|baseline`); a new
  **seekable/blocked** baseline court was built and sealed as an amendment
  (`tools/seekable-baselines.sh`, `tools/bgzf-seek-probe.pl`,
  `tools/xz-seek-probe.pl`, `tools/xz-block-reframe.pl`); no measured campaign
  number was altered.
- **Verdict:** the **numeric and exactness results SURVIVE** (18/18 slices
  byte-exact, the floor arithmetic is exact, the instrumented `bytes_read` is
  cross-checked by `strace`). The **"bytes-read win" framing is FALSIFIED as a
  general random-access-I/O result**: it holds only against *non-seekable
  sequential* codecs. Two overclaims are corrected, and one validator gap is
  stated honestly.

## Angles

| # | Angle | Verdict | Basis |
| --- | --- | --- | --- |
| 1 | Seeked `view` exactness: every served slice `cmp`-identical to the full materialization | PASS | reproduced 18/18 (start/middle/end byte-ranges + `--pdf-object`/`--pdf-stream`) in `dev`; no slice disagrees |
| 2 | The primary metric is real I/O, not a model | PASS | the instrumented `bytes_read` equals `strace -P <descriptor>` read() bytes minus exactly 64 B (the CLI header peek) for every query; `strace` attributes reads to the descriptor file, never the loader/pipe |
| 3 | The floor arithmetic | PASS | `a=0` reads exactly 64 (header) + 27,390 (DIRECTORY) + 265,462 (GRAPH) + 146,711 (OBSERVATION_INDEX) + 52 (INTEGRITY) = 439,679 B; a channel-bearing query adds one referenced channel + model (460,713 B at 31 MiB) |
| 4 | **Headline: is it a bytes-read win?** | **FALSIFIED (as a general random-access-I/O win)** | the win was measured only against **non-seekable sequential** gzip/zstd/xz. Against seekable/blocked formats VOLE **loses**: for the late query it reads 2.6–30× **more** (below) |
| 5 | Overclaim: "reads only the 64-byte header, the DIRECTORY, GRAPH, OBSERVATION_INDEX, INTEGRITY, and the one referenced object/channel/model" | **FALSIFIED (as worded)** | those are *record classes*, and the read is a ~440 KB **floor**; a 256-byte request still incurs it (~1,700× the request) |
| 6 | Validator: are directory channel lengths checked? | PASS with caveat | a **referenced** channel's `decoded_length` is cross-checked against its record; an **unreferenced** `CHANNEL_LENGTHS` entry is not — benign, because `analyze_ops` never uses an unreferenced length |

## Angle 4 — the seekable/blocked comparison (the falsifying evidence)

Reproduced with the pinned `baseline` image (bgzip = htslib 1.16 via the `tabix`
package; pixz 1.0.7; xz 5.4.1). Late query `--byte-range=32505856:256`
(VOLE seeked `view` = 460,713 B); bytes read is the covering compressed block(s)
plus the index/framing that locates them. xz/pixz covering blocks were decoded
**in isolation** (block independence demonstrated) and every extracted slice was
`cmp`-identical to the source:

| reader | bytes read | VOLE / reader |
| --- | ---: | ---: |
| VOLE seeked `view` | 460,713 | 1.00× |
| bgzip (BGZF, 64 KiB blocks) | 23,808 | **19.4× more** |
| xz --block-size=64KiB | 15,344 | **30.0× more** |
| xz --block-size=1MiB | 179,892 | **2.56× more** |
| xz --block-size=4MiB | 708,612 | 0.65× (VOLE reads *less*) |
| pixz (16 MiB blocks) | 2,810,832 | 0.16× (VOLE reads *less*) |

Whole-file size compounds the loss: bgzip's archive is **7,995,600 B** — smaller
than VOLE's 17,566,832 B seekable descriptor — as are all three blocked-xz
streams (5.7–6.4 MB) and pixz (5.69 MB). `zstd --seekable` does not exist in
zstd 1.5.4. VOLE wins only against blocked xz with 4 MiB blocks and against
pixz, whose default 16 MiB blocks make a byte-range seek expensive; neither is
the compact-block seekable regime.

## Angle 5 — the overclaim in plain terms

The prose "reads only the 64-byte header, the DIRECTORY, GRAPH,
OBSERVATION_INDEX, INTEGRITY, and the one referenced object/channel/model"
describes the *record classes* touched, not a small read. The constant floor is
dominated by GRAPH + OBSERVATION_INDEX + DIRECTORY (~439 KB), so a 256-byte
request costs ~1,700× the requested bytes even when no channel is referenced.
The corrected statement lists the record classes **and** the floor.

## Angle 6 — the channel-length validator gap (stated honestly)

`materialize_observation_seeked` builds `channel_lens` from the directory's
`CHANNEL_LENGTHS` table and re-derives the observation index over it
(`ObservationIndex::validate`). When a channel is actually read, its
`decoded_length` is cross-checked against `channel_lens[id]` and a disagreement
is `InvalidContainer`. An **unreferenced** channel's `channel_lengths[id]` entry
is therefore never compared against that channel's record. This is benign:
`analyze_ops` derives each op's output length from the channels that op
references, so a lie in an unused length cannot change any served byte. It is a
gap in the *checks*, not in the *bytes*.

## Required restatement

> On one locally generated 33.8 MB PDF, the seeked `view` reads a constant
> ~0.44–0.46 MB (a floor dominated by GRAPH + OBSERVATION_INDEX + DIRECTORY),
> beating **non-seekable sequential** gzip/zstd/xz prefixes by 12–21× for late
> queries; a purpose-built seekable/blocked format (BGZF ~24 KB; blocked xz
> 15–180 KB) reads **2.6–30× less** for the same late query, so this is **not** a
> general random-access-I/O win. It loses at offset 0 and early queries;
> whole-file size is 3.01× xz.

## Strongest remaining caveat

The seek mechanism is real, byte-exact, and genuinely reduces on-disk I/O
relative to a *sequential* decoder — that part survives. What does **not** is
the framing that it is a general random-access-I/O win: measured against
purpose-built seekable/blocked formats, VOLE reads **more** for compact-block
formats and only wins where the block size is large (4 MiB xz, 16 MiB pixz).
The claim must be scoped to "beats non-seekable sequential prefixes" and must
record where it loses. Single locally generated corpus; no population claim.
