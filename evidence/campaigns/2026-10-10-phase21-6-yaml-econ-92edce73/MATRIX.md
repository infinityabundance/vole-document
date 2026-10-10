# Phase 21.6.2 — cross-lane Q1–Q8 answer matrix

`g` = answered (derived), `D` = typed decline, `-` = not applicable.

| fixture | Q | VOLE | SQLite | spanpy | VOLE<->sqlite | VOLE<->spanpy |
|---|---|---|---|---|---|---|
| anchors.yaml | Q1 | g | g | g | equal | equal |
| anchors.yaml | Q2 | g | D | g | capability-gap | equal |
| anchors.yaml | Q3 | g | D | g | capability-gap | equal |
| anchors.yaml | Q4 | g | D | g | capability-gap | equal |
| anchors.yaml | Q5 | g | g | g | equal | equal |
| anchors.yaml | Q6 | g | D | g | capability-gap | equal |
| anchors.yaml | Q7 | g | D | g | capability-gap | equal |
| anchors.yaml | Q8 | g | g | g | equal | equal |
| tags.yaml | Q1 | g | g | g | equal | equal |
| tags.yaml | Q2 | g | D | g | capability-gap | equal |
| tags.yaml | Q3 | D | D | D | both-decline | both-decline |
| tags.yaml | Q4 | g | D | g | capability-gap | equal |
| tags.yaml | Q5 | g | g | g | equal | equal |
| tags.yaml | Q6 | g | D | g | capability-gap | equal |
| tags.yaml | Q7 | D | D | D | both-decline | both-decline |
| tags.yaml | Q8 | g | g | g | equal | equal |
| multidoc.yaml | Q1 | g | g | g | equal | equal |
| multidoc.yaml | Q2 | g | D | g | capability-gap | equal |
| multidoc.yaml | Q3 | D | D | D | both-decline | both-decline |
| multidoc.yaml | Q4 | g | D | g | capability-gap | equal |
| multidoc.yaml | Q5 | g | g | g | equal | equal |
| multidoc.yaml | Q6 | g | D | g | capability-gap | equal |
| multidoc.yaml | Q7 | D | D | D | both-decline | both-decline |
| multidoc.yaml | Q8 | g | g | g | equal | equal |
| styles.yaml | Q1 | g | g | g | equal | equal |
| styles.yaml | Q2 | g | D | g | capability-gap | equal |
| styles.yaml | Q3 | D | D | D | both-decline | both-decline |
| styles.yaml | Q4 | g | D | g | capability-gap | equal |
| styles.yaml | Q5 | g | g | g | equal | equal |
| styles.yaml | Q6 | g | D | g | capability-gap | equal |
| styles.yaml | Q7 | D | D | D | both-decline | both-decline |
| styles.yaml | Q8 | g | g | g | equal | equal |
| comments.yaml | Q1 | g | g | g | equal | equal |
| comments.yaml | Q2 | g | D | g | capability-gap | equal |
| comments.yaml | Q3 | D | D | D | both-decline | both-decline |
| comments.yaml | Q4 | D | D | D | both-decline | both-decline |
| comments.yaml | Q5 | g | g | g | equal | equal |
| comments.yaml | Q6 | g | D | g | capability-gap | equal |
| comments.yaml | Q7 | D | D | D | both-decline | both-decline |
| comments.yaml | Q8 | g | g | g | equal | equal |
| dup.yaml | Q1 | g | g | g | mismatch | equal |
| dup.yaml | Q2 | g | D | g | capability-gap | equal |
| dup.yaml | Q3 | D | D | D | both-decline | both-decline |
| dup.yaml | Q4 | g | D | g | capability-gap | equal |
| dup.yaml | Q5 | g | g | g | equal | equal |
| dup.yaml | Q6 | g | D | g | capability-gap | equal |
| dup.yaml | Q7 | D | D | D | both-decline | both-decline |
| dup.yaml | Q8 | g | g | g | equal | equal |
| deep.yaml | Q1 | D | D | D | both-decline | both-decline |
| deep.yaml | Q2 | g | D | g | capability-gap | equal |
| deep.yaml | Q3 | D | D | D | both-decline | both-decline |
| deep.yaml | Q4 | g | D | g | capability-gap | equal |
| deep.yaml | Q5 | g | g | g | equal | equal |
| deep.yaml | Q6 | g | D | g | capability-gap | equal |
| deep.yaml | Q7 | D | D | D | both-decline | both-decline |
| deep.yaml | Q8 | g | g | g | equal | equal |
| large.yaml | Q1 | g | g | g | equal | equal |
| large.yaml | Q2 | g | D | g | capability-gap | equal |
| large.yaml | Q3 | D | D | D | both-decline | both-decline |
| large.yaml | Q4 | g | D | g | capability-gap | equal |
| large.yaml | Q5 | g | g | g | equal | equal |
| large.yaml | Q6 | g | D | g | capability-gap | equal |
| large.yaml | Q7 | D | D | D | both-decline | both-decline |
| large.yaml | Q8 | g | g | g | equal | equal |

### Aggregate equivalence per Q

| Q | comparator | equal | both-decline | capability-gap | mismatch | shape |
|---|---|---:|---:|---:|---:|---:|
| Q1 | sqlite | 6 | 1 | 0 | 1 | 0 |
| Q1 | spanpy | 7 | 1 | 0 | 0 | 0 |
| Q2 | sqlite | 0 | 0 | 8 | 0 | 0 |
| Q2 | spanpy | 8 | 0 | 0 | 0 | 0 |
| Q3 | sqlite | 0 | 7 | 1 | 0 | 0 |
| Q3 | spanpy | 1 | 7 | 0 | 0 | 0 |
| Q4 | sqlite | 0 | 1 | 7 | 0 | 0 |
| Q4 | spanpy | 7 | 1 | 0 | 0 | 0 |
| Q5 | sqlite | 8 | 0 | 0 | 0 | 0 |
| Q5 | spanpy | 8 | 0 | 0 | 0 | 0 |
| Q6 | sqlite | 0 | 0 | 8 | 0 | 0 |
| Q6 | spanpy | 8 | 0 | 0 | 0 | 0 |
| Q7 | sqlite | 0 | 7 | 1 | 0 | 0 |
| Q7 | spanpy | 1 | 7 | 0 | 0 | 0 |
| Q8 | sqlite | 8 | 0 | 0 | 0 | 0 |
| Q8 | spanpy | 8 | 0 | 0 | 0 | 0 |

