# Phase 21.6 — cross-lane Q1–Q8 answer matrix

`g` = answered (derived), `D` = typed decline, `-` = not applicable.

| fixture | Q | VOLE | SQLite | VOLE<->SQLite |
|---|---|---|---|---|
| anchors.yaml | Q1 | g | g | equal |
| anchors.yaml | Q2 | g | D | capability-gap |
| anchors.yaml | Q3 | g | D | capability-gap |
| anchors.yaml | Q4 | g | D | capability-gap |
| anchors.yaml | Q5 | g | g | equal |
| anchors.yaml | Q6 | g | D | capability-gap |
| anchors.yaml | Q7 | g | D | capability-gap |
| anchors.yaml | Q8 | g | g | equal |
| tags.yaml | Q1 | g | g | equal |
| tags.yaml | Q2 | g | D | capability-gap |
| tags.yaml | Q3 | D | D | both-decline |
| tags.yaml | Q4 | g | D | capability-gap |
| tags.yaml | Q5 | g | g | equal |
| tags.yaml | Q6 | g | D | capability-gap |
| tags.yaml | Q7 | D | D | both-decline |
| tags.yaml | Q8 | g | g | equal |
| multidoc.yaml | Q1 | g | g | equal |
| multidoc.yaml | Q2 | g | D | capability-gap |
| multidoc.yaml | Q3 | D | D | both-decline |
| multidoc.yaml | Q4 | g | D | capability-gap |
| multidoc.yaml | Q5 | g | g | equal |
| multidoc.yaml | Q6 | g | D | capability-gap |
| multidoc.yaml | Q7 | D | D | both-decline |
| multidoc.yaml | Q8 | g | g | equal |
| styles.yaml | Q1 | g | g | equal |
| styles.yaml | Q2 | g | D | capability-gap |
| styles.yaml | Q3 | D | D | both-decline |
| styles.yaml | Q4 | g | D | capability-gap |
| styles.yaml | Q5 | g | g | equal |
| styles.yaml | Q6 | g | D | capability-gap |
| styles.yaml | Q7 | D | D | both-decline |
| styles.yaml | Q8 | g | g | equal |
| comments.yaml | Q1 | g | g | equal |
| comments.yaml | Q2 | g | D | capability-gap |
| comments.yaml | Q3 | D | D | both-decline |
| comments.yaml | Q4 | g | D | capability-gap |
| comments.yaml | Q5 | g | g | equal |
| comments.yaml | Q6 | g | D | capability-gap |
| comments.yaml | Q7 | D | D | both-decline |
| comments.yaml | Q8 | g | g | equal |
| dup.yaml | Q1 | g | g | mismatch |
| dup.yaml | Q2 | g | D | capability-gap |
| dup.yaml | Q3 | D | D | both-decline |
| dup.yaml | Q4 | g | D | capability-gap |
| dup.yaml | Q5 | g | g | equal |
| dup.yaml | Q6 | g | D | capability-gap |
| dup.yaml | Q7 | D | D | both-decline |
| dup.yaml | Q8 | g | g | equal |
| deep.yaml | Q1 | D | D | both-decline |
| deep.yaml | Q2 | g | D | capability-gap |
| deep.yaml | Q3 | D | D | both-decline |
| deep.yaml | Q4 | g | D | capability-gap |
| deep.yaml | Q5 | g | g | equal |
| deep.yaml | Q6 | g | D | capability-gap |
| deep.yaml | Q7 | D | D | both-decline |
| deep.yaml | Q8 | g | g | equal |
| large.yaml | Q1 | g | g | equal |
| large.yaml | Q2 | g | D | capability-gap |
| large.yaml | Q3 | D | D | both-decline |
| large.yaml | Q4 | g | D | capability-gap |
| large.yaml | Q5 | g | g | equal |
| large.yaml | Q6 | g | D | capability-gap |
| large.yaml | Q7 | D | D | both-decline |
| large.yaml | Q8 | g | g | equal |

### Aggregate equivalence per Q

| Q | comparator | equal | both-decline | capability-gap | mismatch | shape |
|---|---|---:|---:|---:|---:|---:|
| Q1 | sqlite | 6 | 1 | 0 | 1 | 0 |
| Q2 | sqlite | 0 | 0 | 8 | 0 | 0 |
| Q3 | sqlite | 0 | 7 | 1 | 0 | 0 |
| Q4 | sqlite | 0 | 0 | 8 | 0 | 0 |
| Q5 | sqlite | 8 | 0 | 0 | 0 | 0 |
| Q6 | sqlite | 0 | 0 | 8 | 0 | 0 |
| Q7 | sqlite | 0 | 7 | 1 | 0 | 0 |
| Q8 | sqlite | 8 | 0 | 0 | 0 | 0 |

