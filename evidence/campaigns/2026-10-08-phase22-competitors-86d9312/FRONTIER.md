# Phase 22.1 — Pareto frontier (VOLE vs the SQLite envelope)

Per depth, the covering configurations' pooled medians. `bytes` is the sum over the 12 documents of regular-file bytes.

## C0

| lane | build ms | bytes | cold ms | warm ms | Pareto |
|---|---:|---:|---:|---:|:--:|
| vole | 13.2 | 7778087 | 8.68 | 2.40 | yes |
| adaptive | 57.5 | 9661209 | 35.23 | 1.75 | yes |
| full | 58.5 | 9751273 | 10.59 | 1.78 | yes |
| structural | 58.8 | 9661233 | 10.54 | 1.72 | yes |
| minimal | 59.2 | 9661198 | 10.78 | 1.74 | yes |
| hybrid | 59.2 | 9751297 | 10.64 | 1.80 |  |
| fts | 60.8 | 10123999 | 10.64 | 1.77 |  |
| hist | 68.5 | 14273274 | 10.79 | 1.78 |  |

## C1

| lane | build ms | bytes | cold ms | warm ms | Pareto |
|---|---:|---:|---:|---:|:--:|
| vole | 13.2 | 7778087 | 8.56 | 2.40 | yes |
| structural | 58.1 | 9857841 | 10.64 | 1.81 | yes |
| adaptive | 58.2 | 9661209 | 69.02 | 1.87 | yes |
| full | 59.0 | 9857770 | 10.64 | 1.83 | yes |
| hybrid | 60.3 | 9857794 | 10.67 | 1.82 | yes |

## C2

| lane | build ms | bytes | cold ms | warm ms | Pareto |
|---|---:|---:|---:|---:|:--:|
| vole | 13.2 | 7778087 | 8.39 | 2.41 | yes |
| adaptive | 57.5 | 9661210 | 68.23 | 1.97 | yes |
| full | 58.5 | 10103530 | 10.81 | 1.95 | yes |
| hybrid | 59.2 | 10103554 | 10.81 | 1.95 |  |

## C3

| lane | build ms | bytes | cold ms | warm ms | Pareto |
|---|---:|---:|---:|---:|:--:|
| vole | 13.2 | 7778087 | 8.43 | 2.39 | yes |
| adaptive | 58.0 | 9661209 | 68.43 | 1.95 | yes |
| full | 58.8 | 10103530 | 10.80 | 1.95 | yes |
| hybrid | 60.4 | 10103554 | 10.80 | 1.93 | yes |

## C4

| lane | build ms | bytes | cold ms | warm ms | Pareto |
|---|---:|---:|---:|---:|:--:|
| vole | 13.2 | 7778087 | 8.44 | 2.37 | yes |
| adaptive | 58.1 | 9661209 | 67.66 | 2.03 | yes |
| full | 59.5 | 10201834 | 10.88 | 2.02 | yes |
| hybrid | 60.0 | 10201858 | 10.94 | 2.01 | yes |

## C5

| lane | build ms | bytes | cold ms | warm ms | Pareto |
|---|---:|---:|---:|---:|:--:|
| vole | 13.2 | 7778087 | 8.44 | 2.49 | yes |
| adaptive | 58.2 | 9661209 | 68.63 | 2.01 | yes |
| hybrid | 59.4 | 10201857 | 10.92 | 2.03 | yes |
| full | 59.8 | 10201834 | 10.92 | 2.00 | yes |
| hist | 68.5 | 14723834 | 11.10 | 1.94 | yes |
