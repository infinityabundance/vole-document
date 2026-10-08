# Phase-16 storage-correction — file-size-only vs `du -sb` accounting

Re-measures the Phase-15.3 and Phase-16.2 persistent-storage courts with **file-size-only** bytes (`find DIR -type f -printf '%s\n'`, directories contribute 0) instead of `du -sb`, on unchanged populations. `du -sb` is `--apparent-size`, so it adds each directory inode's own `st_size` (4096 B on this ext4 bind mount): inflating the one-file-per-node `fs` store (tens of thousands of dirs) far more than the packed store or the single SQLite `.db`.

Documents measured in this run: **96** (union of the 15.3 subset and the 16.2 common-success set). Git `2978e1d`. Profile release.

## 1. Directory-inode distortion (measured)

Over the successful store directories in this run:

| substrate | file bytes | `du -sb` bytes | directory overhead (B) | overhead / file bytes | dirs | B per dir |
|---|---:|---:|---:|---:|---:|---:|
| VOLE fs store | 1,723,650,951 | 2,620,072,839 | 896,421,888 | 52.0% | 218,853 | 4096 |
| VOLE packed store | 1,738,386,483 | 1,752,767,539 | 14,381,056 | 0.8% | 3,511 | 4096 |
| A1 SQLite `.db` | 1,902,919,680 | 1,902,919,680 | 0 | 0.0% | 0 | — |

Worked example (smallest and largest successful document):

| id | fmt | src B | fs file B | fs du B | fs dirs | fs overhead B | pack file B | pack du B | pack dirs | pack overhead B |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nist-docx-0013 | docx | 25,400 | 31,292 | 387,644 | 87 | 356,352 | 35,536 | 68,304 | 8 | 32,768 |
| nasa-pdf-0020 | pdf | 178,050,443 | 178,289,590 | 188,918,710 | 2,595 | 10,629,120 | 178,443,894 | 178,607,734 | 40 | 163,840 |

## 2. Corrected Phase-15.3 subset (packed / fs)

Population: **12** documents (the 15.3 12-document subset).

| accounting | sum fs B | sum packed B | sum ratio packed/fs | median per-doc |
|---|---:|---:|---:|---:|
| old `du -sb` | 352,671,955 | 253,737,799 | 0.719× | 0.447× |
| **file-size-only** | 250,333,395 | 252,046,151 | **1.007×** | 1.021× |

## 3. Corrected Phase-16.2 common-success (fs vs packed vs SQLite)

Population: **95** documents (16.2 common success (fs+packed+A1)).

#### Overall — files accounting

| comparison | numerator B | denominator B | sum ratio | median per-doc ratio |
|---|---:|---:|---:|---:|
| VOLE fs vs SQLite | 1,723,609,120 | 1,902,919,680 | 0.906× | 0.815× |
| VOLE packed vs SQLite | 1,738,340,616 | 1,902,919,680 | 0.914× | 0.824× |
| VOLE packed vs VOLE fs | 1,738,340,616 | 1,723,609,120 | 1.009× | 1.015× |

### By format (file-size-only)

#### pdf (57 docs) — files accounting

| comparison | numerator B | denominator B | sum ratio | median per-doc ratio |
|---|---:|---:|---:|---:|
| VOLE fs vs SQLite | 1,456,387,670 | 1,631,068,160 | 0.893× | 0.828× |
| VOLE packed vs SQLite | 1,470,712,070 | 1,631,068,160 | 0.902× | 0.847× |
| VOLE packed vs VOLE fs | 1,470,712,070 | 1,456,387,670 | 1.010× | 1.016× |

#### docx (15 docs) — files accounting

| comparison | numerator B | denominator B | sum ratio | median per-doc ratio |
|---|---:|---:|---:|---:|
| VOLE fs vs SQLite | 14,282,449 | 16,900,096 | 0.845× | 0.271× |
| VOLE packed vs SQLite | 14,364,101 | 16,900,096 | 0.850× | 0.283× |
| VOLE packed vs VOLE fs | 14,364,101 | 14,282,449 | 1.006× | 1.035× |

#### epub (23 docs) — files accounting

| comparison | numerator B | denominator B | sum ratio | median per-doc ratio |
|---|---:|---:|---:|---:|
| VOLE fs vs SQLite | 252,939,001 | 254,951,424 | 0.992× | 0.705× |
| VOLE packed vs SQLite | 253,264,445 | 254,951,424 | 0.993× | 0.706× |
| VOLE packed vs VOLE fs | 253,264,445 | 252,939,001 | 1.001× | 1.002× |

### Same population under the old `du -sb` accounting (for comparison)

#### Overall — du accounting

| comparison | numerator B | denominator B | sum ratio | median per-doc ratio |
|---|---:|---:|---:|---:|
| VOLE fs vs SQLite | 2,619,707,424 | 1,902,919,680 | 1.377× | 1.342× |
| VOLE packed vs SQLite | 1,752,688,904 | 1,902,919,680 | 0.921× | 0.846× |
| VOLE packed vs VOLE fs | 1,752,688,904 | 2,619,707,424 | 0.669× | 0.486× |

### `du -sb` by format (old unit)

#### pdf (57 docs) — du accounting

| comparison | numerator B | denominator B | sum ratio | median per-doc ratio |
|---|---:|---:|---:|---:|
| VOLE fs vs SQLite | 2,312,656,470 | 1,631,068,160 | 1.418× | 1.767× |
| VOLE packed vs SQLite | 1,483,577,606 | 1,631,068,160 | 0.910× | 0.861× |
| VOLE packed vs VOLE fs | 1,483,577,606 | 2,312,656,470 | 0.642× | 0.454× |

#### docx (15 docs) — du accounting

| comparison | numerator B | denominator B | sum ratio | median per-doc ratio |
|---|---:|---:|---:|---:|
| VOLE fs vs SQLite | 22,138,577 | 16,900,096 | 1.310× | 1.213× |
| VOLE packed vs SQLite | 14,855,621 | 16,900,096 | 0.879× | 0.476× |
| VOLE packed vs VOLE fs | 14,855,621 | 22,138,577 | 0.671× | 0.286× |

#### epub (23 docs) — du accounting

| comparison | numerator B | denominator B | sum ratio | median per-doc ratio |
|---|---:|---:|---:|---:|
| VOLE fs vs SQLite | 284,912,377 | 254,951,424 | 1.118× | 0.961× |
| VOLE packed vs SQLite | 254,255,677 | 254,951,424 | 0.997× | 0.710× |
| VOLE packed vs VOLE fs | 254,255,677 | 284,912,377 | 0.892× | 0.812× |

## 4. Verdict

Under **file-size-only** accounting, on the 95 common-success documents: fs/SQLite = **0.906×** (old du: 1.377×), packed/SQLite = **0.914×** (old du: 0.921×), packed/fs = **1.009×** (old du: 0.669×). Packed **CLOSES** the gap vs SQLite (margin +0.086× vs 1.0).

