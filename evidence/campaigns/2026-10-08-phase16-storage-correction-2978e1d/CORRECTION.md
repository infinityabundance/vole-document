# CORRECTION — Phase-15.3 / Phase-16.2 persistent-storage accounting

**Campaign:** `2026-10-08-phase16-storage-correction-2978e1d` · commit `2978e1d` · release, `doc-baseline`.
**Status:** the two storage headlines were inflated by a measurement artifact and are **corrected** here.

## What was wrong

The Phase-15.3 (`tools/phase15-packed-court.sh`) and Phase-16.2
(`tools/phase16-packed-full-court.sh`) courts measured persistent bytes with
`du -sb`. On this ext4 bind mount `du -sb` is `--apparent-size`: it sums each
regular file's `st_size` **and each directory inode's own `st_size` (4096 B per
directory)**. The one-file-per-node `fs` store has one file per node under
`seed/aa/bb/` / `index/aa/bb/`, so it carries tens of thousands of directories;
the packed store has a handful; SQLite is a single file. The ratio therefore
mixed real document bytes with directory-inode accounting, inflating `fs` far
more than `packed` or the `.db`. The comparison was also asymmetric: the store's
whole directory tree was counted, but the `.db`'s containing directory was not.

## Measured directory overhead (96 successful store trees)

| substrate | file bytes | `du -sb` bytes | directory overhead | overhead / file bytes | dirs |
|---|---:|---:|---:|---:|---:|
| VOLE `fs` store | 1,723,650,951 | 2,620,072,839 | **896,421,888 B** | **52.0 %** | 218,853 |
| VOLE packed store | 1,738,386,483 | 1,752,767,539 | 14,381,056 B | 0.8 % | 3,511 |
| A1 SQLite `.db` | 1,902,919,680 | 1,902,919,680 | 0 B | 0.0 % | 0 |

Exactly 4096 B per directory. For the smallest doc (`nist-docx-0013`, 25 KB
source) the `fs` store is 31,292 real bytes but 387,644 under `du -sb` (87 dirs =
356,352 B of pure inode overhead) — a 12.4× overstatement driven entirely by
directory count.

## Old vs corrected numbers (file-size-only)

### Phase-15.3 subset (12 docs), packed / fs
| | old `du -sb` | corrected |
|---|---:|---:|
| packed / fs | **0.719×** | **1.007×** |

The "packed is 28 % smaller" result was a directory-inode artifact: in real file
bytes packed is marginally **larger** than fs (+0.7 % on the sums, +2.1 % on the
median per-document ratio).

### Phase-16.2 common success (95 docs), overall
| comparison | old `du -sb` | corrected (files) | "by how much" |
|---|---:|---:|---|
| VOLE fs / SQLite | **1.377×** | **0.906×** | −0.471× |
| VOLE packed / SQLite | **0.921×** | **0.914×** | −0.007× |
| VOLE packed / VOLE fs | **0.669×** | **1.009×** | +0.340× |

By format (corrected, file-size-only):

| format (n) | fs / SQLite | packed / SQLite | packed / fs |
|---|---:|---:|---:|
| pdf (57)  | 0.893× | 0.902× | 1.010× |
| docx (15) | 0.845× | 0.850× | 1.006× |
| epub (23) | 0.992× | 0.993× | 1.001× |

### Reconciliation with the 15.1 baseline
`tools/real100-court.sh` (Phase 15.1) also used `du -sb`
(`v_store_bytes = du -sb vstore`, `a1_bytes = du -sb a1.db`). Over its 95
common-success documents it reproduces `2,619,707,424 / 1,902,919,680 =
1.3767×` **byte-for-byte** with the 16.2 `du` sums. The 1.377× figure is
therefore a `du -sb` number and **moves to 0.906×** under file-size accounting.

## Does the conclusion change?

**Yes, materially.**

- **"VOLE fs is 1.377× SQLite" is refuted.** On real file bytes VOLE's `fs`
  store is **0.906× SQLite — already ~9 % smaller**, not 38 % larger.
- **"Packed CLOSES the gap vs SQLite (0.921×)" is numerically still true**
  (packed / SQLite = **0.914× ≤ 1.0**, margin 0.086×) — **but the framing is
  wrong**: there was no byte gap to close, because `fs` was already below
  SQLite. Packed's apparent 0.669× win over `fs` is **illusory**: in real bytes
  packed ≈ fs (1.009×, packed marginally larger), and this holds for every
  format (1.001–1.010×).
- **The real, surviving claim:** on file-size-only accounting, VOLE's persistent
  store (either backend) is at or below SQLite on the common-success population;
  the packed backend's benefit is **file/directory count** (3511 vs 218,853
  directories here) and the associated metadata/syscall cost, **not document
  bytes**. No byte-level "packed beats fs" or "fs is worse than SQLite" claim
  should be carried forward.

## Method / reproduction

New files only; no existing court, aggregator, manifest or `real100-v1` file was
changed. Court `tools/phase16-storage-correction-court.sh` records both
accountings (`*_files_bytes`, `*_du_bytes`) plus file and directory counts;
aggregator `tools/fixtures/phase16-storage-correction.py` re-derives both
comparisons on the **unchanged** populations (the 15.3 12-doc subset and the 16.2
95-doc common-success set, supplied as id lists). The re-run reproduces the
original `du -sb` sums exactly, so the only variable is the accounting unit.
