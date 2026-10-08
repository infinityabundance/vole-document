# Phase 16.3 — the >100 MiB PDF memory pathology

**Verdict: fixed.** The encode OOM on the largest PDFs was a single
encoder-side allocation bug in `propose_rle` (`src/encode/candidates.rs`).
`nasa-pdf-0001` (409 MB) now completes **byte-exactly**; peak RSS on the
217–308 MB PDFs drops ~2×. All gates pass and no `.voldoc` byte changed.

Lane: `doc-baseline` (mem_limit == memswap_limit == 6 GiB, cpus 8), release
binary, `/usr/bin/time -v`. Pre-fix binary SHA-256 `fb5544f7…` is byte-identical
to the one in `2026-10-08-phase16-packed-full-0b21928`, so the rcs below are a
faithful reproduction of that campaign.

## 1. Reproduced (pre-fix)

| id | bytes | op budget | peak RSS | rc | exact |
|---|---:|---:|---:|---:|---|
| nasa-pdf-0001 | 408,854,600 | 180 s | 6,281,452 KB | **137** (OOM) | no |
| nasa-pdf-0002 | 308,803,168 | 180 s | — | **124** (timeout) | no |
| nasa-pdf-0003 | 217,666,672 | 180 s | — | **124** (timeout) | no |

At a 600 s budget (to see the true peak): 0002 = 5,356,860 KB (17.8× input),
0003 = 3,758,292 KB (17.7×). 0001 OOMs in **2.4 s**.

## 2. Bisected allocation — `propose_rle`

Per-candidate peak RSS on `nasa-pdf-0024` (168,513,117 B), `encode --force KIND`:

| kind | peak RSS | rc |
|---|---:|---:|
| rle | **2,788,964 KB** | 2 (declined) |
| auto (portfolio) | 2,954,692 KB | 0 |
| pdf-channels | 1,011,824 KB | 0 |
| byte-rans / pdf-physical / raw | ~0.99 GB | 0 |
| layout / length-revision / cos-template | ~180 MB | 2 |

`propose_rle` built `runs: Vec<(u8,u64)>` — one entry per maximal run, 16 bytes
each — **before** its `runs.len()*2 > max_graph_ops` (1<<20) decline check. For
near-incompressible input that is ~16 bytes per input byte. Arithmetic matches
exactly: (2,624,401 KB − input) / 16 B = 167,961,664 runs for 168,513,117 bytes
(mean run length 1.003). A 409 MB file needs ~16.4 GB for that `Vec` and is
SIGKILLed at the 6 GiB cap before it can decline.

Direct proof: 0001 `--force raw` completes (rc 0, 2.29 GiB, exact) while
0001 `--force rle` OOMs (rc 137, 6.28 GiB, 0.54 s).

## 3. Fix

Two streaming passes: pass 1 counts maximal runs and the longest run in O(1)
memory and performs both decline checks there; pass 2 materializes `ops` only
once admitted (bounded by `max_graph_ops/2`). Same ops, same descriptor bytes,
same verdict for every input, including the empty input and single-byte runs.

## 4. Before / after (600 s budget, `encode` auto)

| id | bytes | peak before | peak after | wall before | wall after | rc before → after | exact |
|---|---:|---:|---:|---:|---:|---|:--:|
| nasa-pdf-0001 | 408,854,600 | 6,281,620 KB (OOM) | **3,558,292 KB** | 2.4 s | 40.0 s | 137 → **0** | yes |
| nasa-pdf-0002 | 308,803,168 | 5,356,860 KB | **2,696,872 KB** | 260 s | 228 s | 0 → 0 | yes |
| nasa-pdf-0003 | 217,666,672 | 3,758,292 KB | **1,906,148 KB** | 243 s | 233 s | 0 → 0 | yes |

Peak/input ratio: **17.7× → 8.9×**. Forced `rle` on 0001: 6.28 GiB OOM →
401,524 KB (1.0×, declines cleanly).

**`nasa-pdf-0001` now completes** (40 s, well under 180 s). At the campaign's
180 s budget: 0001 rc 137 → **rc 0**; 0002/0003 stay **rc 124** — now a wall
(via the slow `BYTE_RANS` lane), **not** a memory, limit. Memory for those two
is 2.57 GiB / 1.82 GiB, comfortably under the cap.

## 5. Exactness preserved

Before-vs-after binary differential, byte-identical `.voldoc` SHA-256:
- 73/73 corpus documents < 20 MB (all formats), 0 mismatches.
- 4/4 large PDFs (`0024`, `0020`, `0003`, `0002`), 0 mismatches.
- **77/77 identical.**

## 6. Gates (all PASS)

`cargo fmt --all --check`; `cargo clippy --all-targets --all-features -- -D
warnings`; `cargo test --locked --all-features`; `cargo test --locked
--no-default-features`; `sh tools/phase1-court.sh` (receipt folded under
`raw/phase1-court/`).

## Residual risk / open items

- 0002/0003 still exceed the 180 s **op budget** (encode speed, not memory);
  the budget was **not** raised.
- The remaining ~8.9× peak is the court's whole-file decode-before-commit
  round-trip (serialize → parse → materialize) plus the input buffer; bounding
  it further would need streaming materialization/compare, out of scope here.
- Attributed and fixed only the dominant term; no claim is made that the
  residual is irreducible.
