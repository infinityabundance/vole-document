# Phase 21.5.1 — cross-lane Q1–Q8 answer matrix

`g` = answered (derived), `D` = typed decline, `-` = not applicable.

| fixture | Q | VOLE | SQLite | spanpy | VOLE<->sqlite | VOLE<->spanpy |
|---|---|---|---|---|---|---|
| basic.json | Q1 | g | g | g | equal | equal |
| basic.json | Q2 | g | D | g | capability-gap | equal |
| basic.json | Q3 | g | g | g | equal | equal |
| basic.json | Q4 | g | g | g | equal | equal |
| basic.json | Q5 | g | g | g | equal | equal |
| basic.json | Q6 | g | g | g | equal | equal |
| basic.json | Q7 | g | g | g | equal | equal |
| basic.json | Q8 | g | g | g | equal | equal |
| unicode.json | Q1 | g | g | g | equal | equal |
| unicode.json | Q2 | g | D | g | capability-gap | equal |
| unicode.json | Q3 | g | g | g | equal | equal |
| unicode.json | Q4 | g | g | g | equal | equal |
| unicode.json | Q5 | D | D | D | both-decline | both-decline |
| unicode.json | Q6 | g | g | g | equal | equal |
| unicode.json | Q7 | g | g | g | equal | equal |
| unicode.json | Q8 | g | g | g | mismatch | equal |
| numbers.json | Q1 | g | g | g | equal | equal |
| numbers.json | Q2 | g | D | g | capability-gap | equal |
| numbers.json | Q3 | g | g | g | equal | equal |
| numbers.json | Q4 | g | g | g | equal | equal |
| numbers.json | Q5 | D | D | D | both-decline | both-decline |
| numbers.json | Q6 | g | g | g | equal | equal |
| numbers.json | Q7 | g | g | g | equal | equal |
| numbers.json | Q8 | g | g | g | equal | equal |
| dup.json | Q1 | g | g | g | equal | equal |
| dup.json | Q2 | g | D | g | capability-gap | equal |
| dup.json | Q3 | g | g | g | equal | equal |
| dup.json | Q4 | g | g | g | equal | equal |
| dup.json | Q5 | D | D | D | both-decline | both-decline |
| dup.json | Q6 | g | g | g | equal | equal |
| dup.json | Q7 | g | g | g | equal | equal |
| dup.json | Q8 | g | g | g | equal | equal |
| deep.json | Q1 | g | g | g | equal | equal |
| deep.json | Q2 | g | D | g | capability-gap | equal |
| deep.json | Q3 | g | g | g | equal | equal |
| deep.json | Q4 | g | g | g | equal | equal |
| deep.json | Q5 | g | g | g | equal | equal |
| deep.json | Q6 | g | g | g | equal | equal |
| deep.json | Q7 | g | g | g | equal | equal |
| deep.json | Q8 | g | g | g | equal | equal |
| scalar.json | Q1 | g | g | g | equal | equal |
| scalar.json | Q2 | g | D | g | capability-gap | equal |
| scalar.json | Q3 | g | g | g | equal | equal |
| scalar.json | Q4 | g | g | g | equal | equal |
| scalar.json | Q5 | g | g | g | equal | equal |
| scalar.json | Q6 | g | g | g | equal | equal |
| scalar.json | Q7 | g | g | g | equal | equal |
| scalar.json | Q8 | g | g | g | equal | equal |
| large.json | Q1 | g | g | g | equal | equal |
| large.json | Q2 | g | D | g | capability-gap | equal |
| large.json | Q3 | g | g | g | equal | equal |
| large.json | Q4 | g | g | g | equal | equal |
| large.json | Q5 | g | g | g | equal | equal |
| large.json | Q6 | g | g | g | equal | equal |
| large.json | Q7 | g | g | g | equal | equal |
| large.json | Q8 | g | g | g | equal | equal |

### Aggregate equivalence per Q

| Q | comparator | equal | both-decline | capability-gap | mismatch | shape |
|---|---|---:|---:|---:|---:|---:|
| Q1 | sqlite | 7 | 0 | 0 | 0 | 0 |
| Q1 | spanpy | 7 | 0 | 0 | 0 | 0 |
| Q2 | sqlite | 0 | 0 | 7 | 0 | 0 |
| Q2 | spanpy | 7 | 0 | 0 | 0 | 0 |
| Q3 | sqlite | 7 | 0 | 0 | 0 | 0 |
| Q3 | spanpy | 7 | 0 | 0 | 0 | 0 |
| Q4 | sqlite | 7 | 0 | 0 | 0 | 0 |
| Q4 | spanpy | 7 | 0 | 0 | 0 | 0 |
| Q5 | sqlite | 4 | 3 | 0 | 0 | 0 |
| Q5 | spanpy | 4 | 3 | 0 | 0 | 0 |
| Q6 | sqlite | 7 | 0 | 0 | 0 | 0 |
| Q6 | spanpy | 7 | 0 | 0 | 0 | 0 |
| Q7 | sqlite | 7 | 0 | 0 | 0 | 0 |
| Q7 | spanpy | 7 | 0 | 0 | 0 | 0 |
| Q8 | sqlite | 6 | 0 | 0 | 1 | 0 |
| Q8 | spanpy | 7 | 0 | 0 | 0 | 0 |

