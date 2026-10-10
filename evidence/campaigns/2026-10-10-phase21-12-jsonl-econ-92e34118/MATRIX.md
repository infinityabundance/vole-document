# Phase 21.12 — cross-lane Q1–Q8 answer matrix

`g` = answered (derived), `D` = typed decline, `-` = not applicable.

| fixture | Q | VOLE | SQLite | conv | VOLE↔SQLite | VOLE↔conv |
|---|---|---|---|---|---|---|
| basic.ndjson | Q1 | g | g | g | equal | equal |
| basic.ndjson | Q2 | g | D | D | capability-gap | capability-gap |
| basic.ndjson | Q3 | g | g | g | equal | equal |
| basic.ndjson | Q4 | g | g | g | equal | equal |
| basic.ndjson | Q5 | g | D | D | capability-gap | capability-gap |
| basic.ndjson | Q6 | g | g | g | equal | equal |
| basic.ndjson | Q7 | g | g | g | equal | equal |
| basic.ndjson | Q8 | g | g | D | equal | capability-gap |
| shapes.ndjson | Q1 | g | g | g | equal | equal |
| shapes.ndjson | Q2 | g | D | D | capability-gap | capability-gap |
| shapes.ndjson | Q3 | g | g | g | equal | equal |
| shapes.ndjson | Q4 | g | g | g | equal | equal |
| shapes.ndjson | Q5 | g | D | D | capability-gap | capability-gap |
| shapes.ndjson | Q6 | g | g | g | equal | equal |
| shapes.ndjson | Q7 | g | g | g | equal | equal |
| shapes.ndjson | Q8 | g | g | D | equal | capability-gap |
| unicode.ndjson | Q1 | g | g | g | equal | equal |
| unicode.ndjson | Q2 | g | D | D | capability-gap | capability-gap |
| unicode.ndjson | Q3 | g | g | g | equal | equal |
| unicode.ndjson | Q4 | g | g | g | equal | equal |
| unicode.ndjson | Q5 | g | D | D | capability-gap | capability-gap |
| unicode.ndjson | Q6 | g | g | g | equal | equal |
| unicode.ndjson | Q7 | g | g | g | equal | equal |
| unicode.ndjson | Q8 | g | g | D | equal | capability-gap |
| crlf.ndjson | Q1 | g | g | g | equal | equal |
| crlf.ndjson | Q2 | g | D | D | capability-gap | capability-gap |
| crlf.ndjson | Q3 | g | g | g | equal | equal |
| crlf.ndjson | Q4 | g | g | g | equal | equal |
| crlf.ndjson | Q5 | g | D | D | capability-gap | capability-gap |
| crlf.ndjson | Q6 | g | g | g | equal | equal |
| crlf.ndjson | Q7 | g | g | g | equal | equal |
| crlf.ndjson | Q8 | g | g | D | equal | capability-gap |
| blank.ndjson | Q1 | g | g | g | equal | equal |
| blank.ndjson | Q2 | g | D | D | capability-gap | capability-gap |
| blank.ndjson | Q3 | g | g | g | equal | equal |
| blank.ndjson | Q4 | g | g | g | equal | equal |
| blank.ndjson | Q5 | g | D | D | capability-gap | capability-gap |
| blank.ndjson | Q6 | g | g | g | equal | equal |
| blank.ndjson | Q7 | g | g | g | equal | equal |
| blank.ndjson | Q8 | g | g | D | equal | capability-gap |
| large.ndjson | Q1 | g | g | g | equal | equal |
| large.ndjson | Q2 | g | D | D | capability-gap | capability-gap |
| large.ndjson | Q3 | g | g | g | equal | equal |
| large.ndjson | Q4 | g | g | g | equal | equal |
| large.ndjson | Q5 | g | D | D | capability-gap | capability-gap |
| large.ndjson | Q6 | g | g | g | equal | equal |
| large.ndjson | Q7 | g | g | g | equal | equal |
| large.ndjson | Q8 | g | g | D | equal | capability-gap |

### Aggregate equivalence per Q

| Q | comparator | equal | both-decline | capability-gap | mismatch | shape |
|---|---|---:|---:|---:|---:|---:|
| Q1 | sqlite | 6 | 0 | 0 | 0 | 0 |
| Q1 | conv | 6 | 0 | 0 | 0 | 0 |
| Q2 | sqlite | 0 | 0 | 6 | 0 | 0 |
| Q2 | conv | 0 | 0 | 6 | 0 | 0 |
| Q3 | sqlite | 6 | 0 | 0 | 0 | 0 |
| Q3 | conv | 6 | 0 | 0 | 0 | 0 |
| Q4 | sqlite | 6 | 0 | 0 | 0 | 0 |
| Q4 | conv | 6 | 0 | 0 | 0 | 0 |
| Q5 | sqlite | 0 | 0 | 6 | 0 | 0 |
| Q5 | conv | 0 | 0 | 6 | 0 | 0 |
| Q6 | sqlite | 6 | 0 | 0 | 0 | 0 |
| Q6 | conv | 6 | 0 | 0 | 0 | 0 |
| Q7 | sqlite | 6 | 0 | 0 | 0 | 0 |
| Q7 | conv | 6 | 0 | 0 | 0 | 0 |
| Q8 | sqlite | 6 | 0 | 0 | 0 | 0 |
| Q8 | conv | 0 | 0 | 6 | 0 | 0 |

