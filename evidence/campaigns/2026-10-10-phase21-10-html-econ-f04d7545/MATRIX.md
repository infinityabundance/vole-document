# Phase 21.10 — cross-lane Q1–Q8 answer matrix

`g` = answered (derived), `D` = typed decline, `-` = not applicable.

| fixture | Q | VOLE | SQLite | conv | VOLE↔SQLite | VOLE↔conv |
|---|---|---|---|---|---|---|
| basic.html | Q1 | g | g | g | equal | equal |
| basic.html | Q2 | g | D | D | capability-gap | capability-gap |
| basic.html | Q3 | g | D | D | capability-gap | capability-gap |
| basic.html | Q4 | g | g | g | equal | equal |
| basic.html | Q5 | g | g | g | equal | equal |
| basic.html | Q6 | g | g | g | equal | equal |
| basic.html | Q7 | g | g | g | equal | equal |
| basic.html | Q8 | g | g | D | equal | capability-gap |
| elements.html | Q1 | g | g | g | equal | equal |
| elements.html | Q2 | g | D | D | capability-gap | capability-gap |
| elements.html | Q3 | g | D | D | capability-gap | capability-gap |
| elements.html | Q4 | g | g | g | equal | equal |
| elements.html | Q5 | g | g | g | equal | equal |
| elements.html | Q6 | g | g | g | equal | equal |
| elements.html | Q7 | g | g | g | equal | equal |
| elements.html | Q8 | g | g | D | equal | capability-gap |
| rawtext.html | Q1 | g | g | g | equal | equal |
| rawtext.html | Q2 | g | D | D | capability-gap | capability-gap |
| rawtext.html | Q3 | g | D | D | capability-gap | capability-gap |
| rawtext.html | Q4 | g | g | g | equal | equal |
| rawtext.html | Q5 | g | g | g | equal | equal |
| rawtext.html | Q6 | g | g | g | equal | equal |
| rawtext.html | Q7 | g | g | g | equal | equal |
| rawtext.html | Q8 | g | g | D | equal | capability-gap |
| entities.html | Q1 | g | g | g | equal | equal |
| entities.html | Q2 | g | D | D | capability-gap | capability-gap |
| entities.html | Q3 | g | D | D | capability-gap | capability-gap |
| entities.html | Q4 | g | g | g | equal | equal |
| entities.html | Q5 | g | g | g | equal | equal |
| entities.html | Q6 | g | g | g | equal | equal |
| entities.html | Q7 | g | g | g | equal | equal |
| entities.html | Q8 | g | g | D | equal | capability-gap |
| malformed.html | Q1 | g | g | g | equal | equal |
| malformed.html | Q2 | g | D | D | capability-gap | capability-gap |
| malformed.html | Q3 | g | D | D | capability-gap | capability-gap |
| malformed.html | Q4 | g | g | g | equal | equal |
| malformed.html | Q5 | g | g | g | equal | equal |
| malformed.html | Q6 | g | g | g | equal | equal |
| malformed.html | Q7 | g | g | g | equal | equal |
| malformed.html | Q8 | g | g | D | equal | capability-gap |
| large.html | Q1 | g | g | g | equal | equal |
| large.html | Q2 | g | D | D | capability-gap | capability-gap |
| large.html | Q3 | g | D | D | capability-gap | capability-gap |
| large.html | Q4 | g | g | g | equal | equal |
| large.html | Q5 | g | g | g | equal | equal |
| large.html | Q6 | g | g | g | equal | equal |
| large.html | Q7 | g | g | g | equal | equal |
| large.html | Q8 | g | g | D | equal | capability-gap |

### Aggregate equivalence per Q

| Q | comparator | equal | both-decline | capability-gap | mismatch | shape |
|---|---|---:|---:|---:|---:|---:|
| Q1 | sqlite | 6 | 0 | 0 | 0 | 0 |
| Q1 | conv | 6 | 0 | 0 | 0 | 0 |
| Q2 | sqlite | 0 | 0 | 6 | 0 | 0 |
| Q2 | conv | 0 | 0 | 6 | 0 | 0 |
| Q3 | sqlite | 0 | 0 | 6 | 0 | 0 |
| Q3 | conv | 0 | 0 | 6 | 0 | 0 |
| Q4 | sqlite | 6 | 0 | 0 | 0 | 0 |
| Q4 | conv | 6 | 0 | 0 | 0 | 0 |
| Q5 | sqlite | 6 | 0 | 0 | 0 | 0 |
| Q5 | conv | 6 | 0 | 0 | 0 | 0 |
| Q6 | sqlite | 6 | 0 | 0 | 0 | 0 |
| Q6 | conv | 6 | 0 | 0 | 0 | 0 |
| Q7 | sqlite | 6 | 0 | 0 | 0 | 0 |
| Q7 | conv | 6 | 0 | 0 | 0 | 0 |
| Q8 | sqlite | 6 | 0 | 0 | 0 | 0 |
| Q8 | conv | 0 | 0 | 6 | 0 | 0 |

