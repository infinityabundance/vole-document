# Phase 21.11 — cross-lane Q1–Q8 answer matrix

`g` = answered (derived), `D` = typed decline, `-` = not applicable.

| fixture | Q | VOLE | SQLite | conv | VOLE↔SQLite | VOLE↔conv |
|---|---|---|---|---|---|---|
| basic.toml | Q1 | g | g | g | equal | equal |
| basic.toml | Q2 | g | D | D | capability-gap | capability-gap |
| basic.toml | Q3 | g | g | g | equal | equal |
| basic.toml | Q4 | g | g | g | equal | equal |
| basic.toml | Q5 | g | D | D | capability-gap | capability-gap |
| basic.toml | Q6 | g | g | g | equal | equal |
| basic.toml | Q7 | g | g | g | equal | equal |
| basic.toml | Q8 | g | g | D | equal | capability-gap |
| tables.toml | Q1 | g | g | g | equal | equal |
| tables.toml | Q2 | g | D | D | capability-gap | capability-gap |
| tables.toml | Q3 | g | g | g | equal | equal |
| tables.toml | Q4 | g | g | g | equal | equal |
| tables.toml | Q5 | g | D | D | capability-gap | capability-gap |
| tables.toml | Q6 | g | g | g | equal | equal |
| tables.toml | Q7 | g | g | g | equal | equal |
| tables.toml | Q8 | g | g | D | equal | capability-gap |
| arrays.toml | Q1 | g | g | g | equal | equal |
| arrays.toml | Q2 | g | D | D | capability-gap | capability-gap |
| arrays.toml | Q3 | g | g | g | equal | equal |
| arrays.toml | Q4 | g | g | g | equal | equal |
| arrays.toml | Q5 | g | D | D | capability-gap | capability-gap |
| arrays.toml | Q6 | g | g | g | equal | equal |
| arrays.toml | Q7 | g | g | g | equal | equal |
| arrays.toml | Q8 | g | g | D | equal | capability-gap |
| inline.toml | Q1 | g | g | g | equal | equal |
| inline.toml | Q2 | g | D | D | capability-gap | capability-gap |
| inline.toml | Q3 | g | g | g | equal | equal |
| inline.toml | Q4 | g | g | g | equal | equal |
| inline.toml | Q5 | g | D | D | capability-gap | capability-gap |
| inline.toml | Q6 | g | g | g | equal | equal |
| inline.toml | Q7 | g | g | g | equal | equal |
| inline.toml | Q8 | g | g | D | equal | capability-gap |
| scalars.toml | Q1 | g | g | g | equal | equal |
| scalars.toml | Q2 | g | D | D | capability-gap | capability-gap |
| scalars.toml | Q3 | g | g | g | equal | equal |
| scalars.toml | Q4 | g | g | g | equal | equal |
| scalars.toml | Q5 | g | D | D | capability-gap | capability-gap |
| scalars.toml | Q6 | g | g | g | equal | equal |
| scalars.toml | Q7 | g | g | g | equal | equal |
| scalars.toml | Q8 | g | g | D | equal | capability-gap |
| comments.toml | Q1 | g | g | g | equal | equal |
| comments.toml | Q2 | g | D | D | capability-gap | capability-gap |
| comments.toml | Q3 | g | g | g | equal | equal |
| comments.toml | Q4 | g | g | g | equal | equal |
| comments.toml | Q5 | g | D | D | capability-gap | capability-gap |
| comments.toml | Q6 | g | g | g | equal | equal |
| comments.toml | Q7 | g | g | g | equal | equal |
| comments.toml | Q8 | g | g | D | equal | capability-gap |
| large.toml | Q1 | g | g | g | equal | equal |
| large.toml | Q2 | g | D | D | capability-gap | capability-gap |
| large.toml | Q3 | g | g | g | equal | equal |
| large.toml | Q4 | g | g | g | equal | equal |
| large.toml | Q5 | g | D | D | capability-gap | capability-gap |
| large.toml | Q6 | g | g | g | equal | equal |
| large.toml | Q7 | g | g | g | equal | equal |
| large.toml | Q8 | g | g | D | equal | capability-gap |

### Aggregate equivalence per Q

| Q | comparator | equal | both-decline | capability-gap | mismatch | shape |
|---|---|---:|---:|---:|---:|---:|
| Q1 | sqlite | 7 | 0 | 0 | 0 | 0 |
| Q1 | conv | 7 | 0 | 0 | 0 | 0 |
| Q2 | sqlite | 0 | 0 | 7 | 0 | 0 |
| Q2 | conv | 0 | 0 | 7 | 0 | 0 |
| Q3 | sqlite | 7 | 0 | 0 | 0 | 0 |
| Q3 | conv | 7 | 0 | 0 | 0 | 0 |
| Q4 | sqlite | 7 | 0 | 0 | 0 | 0 |
| Q4 | conv | 7 | 0 | 0 | 0 | 0 |
| Q5 | sqlite | 0 | 0 | 7 | 0 | 0 |
| Q5 | conv | 0 | 0 | 7 | 0 | 0 |
| Q6 | sqlite | 7 | 0 | 0 | 0 | 0 |
| Q6 | conv | 7 | 0 | 0 | 0 | 0 |
| Q7 | sqlite | 7 | 0 | 0 | 0 | 0 |
| Q7 | conv | 7 | 0 | 0 | 0 | 0 |
| Q8 | sqlite | 7 | 0 | 0 | 0 | 0 |
| Q8 | conv | 0 | 0 | 7 | 0 | 0 |

