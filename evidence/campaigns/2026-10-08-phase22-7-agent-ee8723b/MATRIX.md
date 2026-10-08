# MATRIX — per-region cost per correct grounded task

Cost per correct grounded task = total workload cost / #(correct AND grounded) tasks. Prices in summary.json; date 2026-10-08.

| region | backend | docs | tasks | correct | grounded | corr+grnd | tokens/task | tokens/corr+grnd | cost total (USD) | cost/corr+grnd (USD) |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| docx | vole | 3 | 9 | 7 | 7 | 7 | 311.2 | 400.1 | 0.008555175 | 0.001222168 |
| docx | baseline | 3 | 9 | 9 | 9 | 9 | 345.7 | 345.7 | 0.009372696 | 0.001041411 |
| epub | vole | 3 | 9 | 9 | 9 | 9 | 386.6 | 386.6 | 0.010529225 | 0.001169914 |
| epub | baseline | 3 | 9 | 9 | 9 | 9 | 378.7 | 378.7 | 0.010280912 | 0.001142324 |
| pdf | vole | 3 | 9 | 0 | 0 | 0 | 166.6 | undef | 0.004932490 | undef |
| pdf | baseline | 3 | 9 | 9 | 9 | 9 | 328.1 | 328.1 | 0.008935503 | 0.000992834 |

| region | ratio VOLE/baseline | verdict |
|---|---:|---|
| docx | 1.1736 | VOLE loss |
| epub | 1.0242 | VOLE loss |
| pdf | inf | VOLE loss (no correct grounded task) |

