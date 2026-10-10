# Phase 21.13 — cross-lane Q1–Q6 answer matrix

`g` = answered (derived), `D` = typed decline, `-` = not applicable.

| fixture | Q | VOLE | SQLite | conv | VOLE↔SQLite | VOLE↔conv |
|---|---|---|---|---|---|---|
| simple.eml | Q1 | g | g | g | equal | equal |
| simple.eml | Q2 | g | g | g | equal | equal |
| simple.eml | Q3 | D | D | D | both-decline | both-decline |
| simple.eml | Q4 | g | g | g | equal | equal |
| simple.eml | Q5 | g | g | g | equal | equal |
| simple.eml | Q6 | g | g | D | equal | capability-gap |
| mixed.eml | Q1 | g | g | g | equal | equal |
| mixed.eml | Q2 | g | g | g | equal | equal |
| mixed.eml | Q3 | g | g | g | equal | equal |
| mixed.eml | Q4 | g | g | g | equal | equal |
| mixed.eml | Q5 | g | g | g | equal | equal |
| mixed.eml | Q6 | g | g | D | equal | capability-gap |
| alternative.eml | Q1 | g | g | g | equal | equal |
| alternative.eml | Q2 | g | g | g | equal | equal |
| alternative.eml | Q3 | D | D | D | both-decline | both-decline |
| alternative.eml | Q4 | g | g | g | equal | equal |
| alternative.eml | Q5 | g | g | g | equal | equal |
| alternative.eml | Q6 | g | g | D | equal | capability-gap |
| nested.eml | Q1 | g | g | g | equal | equal |
| nested.eml | Q2 | g | g | g | equal | equal |
| nested.eml | Q3 | D | D | D | both-decline | both-decline |
| nested.eml | Q4 | g | g | g | equal | equal |
| nested.eml | Q5 | g | g | g | equal | equal |
| nested.eml | Q6 | g | g | D | equal | capability-gap |
| folded.eml | Q1 | g | g | g | equal | equal |
| folded.eml | Q2 | g | g | g | equal | equal |
| folded.eml | Q3 | D | D | D | both-decline | both-decline |
| folded.eml | Q4 | g | g | g | equal | equal |
| folded.eml | Q5 | g | g | g | equal | equal |
| folded.eml | Q6 | g | g | D | equal | capability-gap |
| large.eml | Q1 | g | g | g | equal | equal |
| large.eml | Q2 | g | g | g | equal | equal |
| large.eml | Q3 | g | g | g | equal | equal |
| large.eml | Q4 | g | g | g | equal | equal |
| large.eml | Q5 | g | g | g | equal | equal |
| large.eml | Q6 | g | g | D | equal | capability-gap |

### Aggregate equivalence per Q

| Q | comparator | equal | both-decline | capability-gap | mismatch |
|---|---|---:|---:|---:|---:|
| Q1 | sqlite | 6 | 0 | 0 | 0 |
| Q1 | conv | 6 | 0 | 0 | 0 |
| Q2 | sqlite | 6 | 0 | 0 | 0 |
| Q2 | conv | 6 | 0 | 0 | 0 |
| Q3 | sqlite | 2 | 4 | 0 | 0 |
| Q3 | conv | 2 | 4 | 0 | 0 |
| Q4 | sqlite | 6 | 0 | 0 | 0 |
| Q4 | conv | 6 | 0 | 0 | 0 |
| Q5 | sqlite | 6 | 0 | 0 | 0 |
| Q5 | conv | 6 | 0 | 0 | 0 |
| Q6 | sqlite | 6 | 0 | 0 | 0 |
| Q6 | conv | 0 | 0 | 6 | 0 |

