# Phase 21.7 — cross-lane Q1–Q8 answer matrix

`g` = answered (derived), `D` = typed decline, `-` = not applicable.

| fixture | Q | VOLE | SQLite | DuckDB | VOLE↔SQLite | VOLE↔DuckDB |
|---|---|---|---|---|---|---|
| basic.csv | Q1 | g | g | g | equal | equal |
| basic.csv | Q2 | g | D | D | capability-gap | capability-gap |
| basic.csv | Q3 | g | D | D | capability-gap | capability-gap |
| basic.csv | Q4 | g | g | g | equal | equal |
| basic.csv | Q5 | g | g | g | equal | equal |
| basic.csv | Q6 | g | g | g | equal | equal |
| basic.csv | Q7 | g | g | g | equal | equal |
| basic.csv | Q8 | g | g | D | equal | capability-gap |
| quoted.csv | Q1 | g | g | g | equal | equal |
| quoted.csv | Q2 | g | D | D | capability-gap | capability-gap |
| quoted.csv | Q3 | g | D | D | capability-gap | capability-gap |
| quoted.csv | Q4 | g | g | g | equal | equal |
| quoted.csv | Q5 | g | g | g | equal | equal |
| quoted.csv | Q6 | g | g | g | equal | equal |
| quoted.csv | Q7 | g | g | g | equal | equal |
| quoted.csv | Q8 | g | g | D | equal | capability-gap |
| crlf.csv | Q1 | g | g | g | equal | equal |
| crlf.csv | Q2 | g | D | D | capability-gap | capability-gap |
| crlf.csv | Q3 | g | D | D | capability-gap | capability-gap |
| crlf.csv | Q4 | g | g | g | equal | equal |
| crlf.csv | Q5 | g | g | g | equal | equal |
| crlf.csv | Q6 | g | g | g | equal | equal |
| crlf.csv | Q7 | g | g | g | equal | equal |
| crlf.csv | Q8 | g | g | D | equal | capability-gap |
| bom.csv | Q1 | g | g | g | equal | equal |
| bom.csv | Q2 | g | D | D | capability-gap | capability-gap |
| bom.csv | Q3 | g | D | D | capability-gap | capability-gap |
| bom.csv | Q4 | g | g | g | equal | equal |
| bom.csv | Q5 | g | g | g | equal | equal |
| bom.csv | Q6 | g | g | g | equal | equal |
| bom.csv | Q7 | g | g | g | equal | equal |
| bom.csv | Q8 | g | g | D | equal | capability-gap |
| ragged.csv | Q1 | g | g | g | equal | equal |
| ragged.csv | Q2 | g | D | D | capability-gap | capability-gap |
| ragged.csv | Q3 | g | D | D | capability-gap | capability-gap |
| ragged.csv | Q4 | g | g | g | equal | mismatch |
| ragged.csv | Q5 | g | g | g | equal | equal |
| ragged.csv | Q6 | g | g | g | equal | equal |
| ragged.csv | Q7 | g | g | g | equal | equal |
| ragged.csv | Q8 | g | g | D | equal | capability-gap |
| tsv.tsv | Q1 | g | g | g | equal | equal |
| tsv.tsv | Q2 | g | D | D | capability-gap | capability-gap |
| tsv.tsv | Q3 | g | D | D | capability-gap | capability-gap |
| tsv.tsv | Q4 | g | g | g | equal | equal |
| tsv.tsv | Q5 | g | g | g | equal | equal |
| tsv.tsv | Q6 | g | g | g | equal | equal |
| tsv.tsv | Q7 | g | g | g | equal | equal |
| tsv.tsv | Q8 | g | g | D | equal | capability-gap |

### Aggregate equivalence per Q

| Q | comparator | equal | both-decline | capability-gap | mismatch | shape |
|---|---|---:|---:|---:|---:|---:|
| Q1 | sqlite | 6 | 0 | 0 | 0 | 0 |
| Q1 | duckdb | 6 | 0 | 0 | 0 | 0 |
| Q2 | sqlite | 0 | 0 | 6 | 0 | 0 |
| Q2 | duckdb | 0 | 0 | 6 | 0 | 0 |
| Q3 | sqlite | 0 | 0 | 6 | 0 | 0 |
| Q3 | duckdb | 0 | 0 | 6 | 0 | 0 |
| Q4 | sqlite | 6 | 0 | 0 | 0 | 0 |
| Q4 | duckdb | 5 | 0 | 0 | 1 | 0 |
| Q5 | sqlite | 6 | 0 | 0 | 0 | 0 |
| Q5 | duckdb | 6 | 0 | 0 | 0 | 0 |
| Q6 | sqlite | 6 | 0 | 0 | 0 | 0 |
| Q6 | duckdb | 6 | 0 | 0 | 0 | 0 |
| Q7 | sqlite | 6 | 0 | 0 | 0 | 0 |
| Q7 | duckdb | 6 | 0 | 0 | 0 | 0 |
| Q8 | sqlite | 6 | 0 | 0 | 0 | 0 |
| Q8 | duckdb | 0 | 0 | 6 | 0 | 0 |

