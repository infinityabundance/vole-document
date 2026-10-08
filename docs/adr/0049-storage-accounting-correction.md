# ADR-0049: Storage receipts must state their byte-accounting method (`du -sb` counts directory inodes)

- **Status:** Accepted — correction (Phase 16.6)
- **Date:** 2026-10-08

## Context

The Phase-15.3 (`tools/phase15-packed-court.sh`) and Phase-16.2
(`tools/phase16-packed-full-court.sh`) storage courts — and the Phase-15.1
baseline (`tools/real100-court.sh`) — measured persistent bytes with `du -sb`.
On this ext4 bind mount `du -sb` is `--apparent-size`: it sums each regular
file's `st_size` **and each directory inode's own `st_size`**, which this
filesystem reports as **4096 B per directory**. The one-file-per-node `fs` store
writes one file per seed node under `seed/aa/bb/` / `index/aa/bb/`, so it carries
tens of thousands of directories (218,853 across the 96 measured store trees);
the packed store has a few thousand (3,511); SQLite is a single file (0).

The comparison was therefore asymmetric: the `fs` store's `du -sb` was inflated
**~52 %** (896,421,888 B of pure directory-inode overhead over 1,723,650,951 B of
real file bytes), the packed store by **0.8 %**, and the `.db` not at all. The
"packed is 28 % smaller" and "VOLE `fs` is 1.377× SQLite" headlines were artifacts
of the accounting unit, not of the stored bytes. The store's whole directory tree
was counted, but the `.db`'s containing directory was not.

## Decision

1. **Persistent-storage receipts state their byte-accounting method.** A receipt
   reporting store size names the unit (`du -sb` apparent size vs sum-of-regular-
   file bytes) and, for a one-file-per-node tree, reports file and directory
   counts alongside bytes.
2. **Storage comparisons use sum-of-regular-file bytes** —
   `find DIR -type f -printf '%s\n' | awk '{s+=$1} END{print s}'` — so that
   directory inodes contribute 0. When whole-tree cost matters, directory count
   is reported separately, never folded into document bytes.
3. **Correct affected headlines by amendment, not rewrite.** The Phase-15.3 and
   Phase-16.2 receipts keep their original `du -sb` numbers and gain a
   `superseded_by` / `correction` pointer to the correction campaign; corrected
   figures are published alongside (ADR-0003 amendment discipline).

## Corrected numbers (file-bytes-only)

| comparison | old `du -sb` | corrected |
|---|---:|---:|
| 15.3 subset (12 docs), packed / fs | **0.719×** | **1.007×** (parity; packed marginally larger) |
| 16.2 common success (95 docs), VOLE fs / SQLite | **1.377×** | **0.906×** |
| 16.2 common success (95 docs), VOLE packed / SQLite | 0.921× | **0.914×** |
| 16.2 common success (95 docs), VOLE packed / fs | 0.669× | **1.009×** |

By format (16.2, packed / SQLite): pdf 0.902×, docx 0.850×, epub 0.993×;
(fs / SQLite): pdf 0.893×, docx 0.845×, epub 0.992×.

## Consequences

- **"VOLE persistent footprint is 1.377× SQLite's" is refuted.** On file bytes the
  `fs` store is *smaller* than SQLite (**0.906×**, ~9 % below). **"Packed closes the
  gap" is mis-framed**: there was no byte gap to close — both VOLE backends are at
  or below SQLite on file bytes.
- **The packed store's surviving benefit is file/directory count** (`25,574 → 237`
  files on the 15.3 subset; `218,853 → 3,511` directories over the 96-tree
  re-measurement) — i.e. open and syscall economics, **not** document bytes, where
  packed and `fs` are at parity (1.001–1.010× by format).
- **Exactness is untouched.** No descriptor, manifest, index, or wire byte changed;
  the correction is measurement-only, and the re-run reproduces the original
  `du -sb` sums byte-for-byte, so the only variable is the accounting unit.
- The distortion is not `du`-specific: any apparent-size accounting on a filesystem
  that charges inodes a non-zero `st_size` carries it.

## References

- `evidence/campaigns/2026-10-08-phase16-storage-correction-2978e1d/`
  (`CORRECTION.md`, `SUMMARY.md`, `receipt.json`)
- `tools/phase16-storage-correction-court.sh`,
  `tools/fixtures/phase16-storage-correction.py`
- ADR-0043 (packed seed store); ADR-0027 (cost-accounting universes)
- `docs/phases/phase-15-results.md` (15.1, 15.3)
- Superseded receipts: `2026-10-07-real100-release-baseline-866f489`,
  `2026-10-07-phase15-packed-8c195e8`, `2026-10-08-phase16-packed-full-0b21928`
