# Phase 21.9 — cross-lane Q1–Q8 answer matrix

`g` = answered (derived), `D` = typed decline, `-` = not applicable.

| fixture | Q | VOLE | SQLite | conv | VOLE↔SQLite | VOLE↔conv |
|---|---|---|---|---|---|---|
| basic.xml | Q1 | g | g | g | equal | equal |
| basic.xml | Q2 | g | D | D | capability-gap | capability-gap |
| basic.xml | Q3 | g | D | D | capability-gap | capability-gap |
| basic.xml | Q4 | g | g | g | equal | equal |
| basic.xml | Q5 | D | D | D | both-decline | both-decline |
| basic.xml | Q6 | g | g | g | equal | equal |
| basic.xml | Q7 | g | g | g | equal | equal |
| basic.xml | Q8 | g | g | D | equal | capability-gap |
| namespaces.xml | Q1 | g | g | g | equal | equal |
| namespaces.xml | Q2 | g | D | D | capability-gap | capability-gap |
| namespaces.xml | Q3 | g | D | D | capability-gap | capability-gap |
| namespaces.xml | Q4 | g | g | g | equal | equal |
| namespaces.xml | Q5 | g | g | D | equal | capability-gap |
| namespaces.xml | Q6 | g | g | g | equal | equal |
| namespaces.xml | Q7 | g | g | g | equal | equal |
| namespaces.xml | Q8 | g | g | D | equal | capability-gap |
| mixed.xml | Q1 | g | g | g | equal | equal |
| mixed.xml | Q2 | g | D | D | capability-gap | capability-gap |
| mixed.xml | Q3 | D | D | D | both-decline | both-decline |
| mixed.xml | Q4 | D | D | D | both-decline | both-decline |
| mixed.xml | Q5 | D | D | D | both-decline | both-decline |
| mixed.xml | Q6 | g | g | g | equal | equal |
| mixed.xml | Q7 | g | g | g | equal | equal |
| mixed.xml | Q8 | g | g | D | equal | capability-gap |
| attrs.xml | Q1 | g | g | g | equal | equal |
| attrs.xml | Q2 | g | D | D | capability-gap | capability-gap |
| attrs.xml | Q3 | g | D | D | capability-gap | capability-gap |
| attrs.xml | Q4 | g | g | g | equal | equal |
| attrs.xml | Q5 | D | D | D | both-decline | both-decline |
| attrs.xml | Q6 | g | g | g | equal | equal |
| attrs.xml | Q7 | g | g | g | equal | equal |
| attrs.xml | Q8 | g | g | D | equal | capability-gap |
| dtd.xml | Q1 | g | g | g | equal | equal |
| dtd.xml | Q2 | g | D | D | capability-gap | capability-gap |
| dtd.xml | Q3 | D | D | D | both-decline | both-decline |
| dtd.xml | Q4 | D | D | D | both-decline | both-decline |
| dtd.xml | Q5 | D | D | D | both-decline | both-decline |
| dtd.xml | Q6 | g | g | g | equal | equal |
| dtd.xml | Q7 | g | g | g | equal | equal |
| dtd.xml | Q8 | g | g | D | equal | capability-gap |
| large.xml | Q1 | g | g | g | equal | equal |
| large.xml | Q2 | g | D | D | capability-gap | capability-gap |
| large.xml | Q3 | g | D | D | capability-gap | capability-gap |
| large.xml | Q4 | g | g | g | equal | equal |
| large.xml | Q5 | D | D | D | both-decline | both-decline |
| large.xml | Q6 | g | g | g | equal | equal |
| large.xml | Q7 | g | g | g | equal | equal |
| large.xml | Q8 | g | g | D | equal | capability-gap |

### Aggregate equivalence per Q

| Q | comparator | equal | both-decline | capability-gap | mismatch | shape |
|---|---|---:|---:|---:|---:|---:|
| Q1 | sqlite | 6 | 0 | 0 | 0 | 0 |
| Q1 | conv | 6 | 0 | 0 | 0 | 0 |
| Q2 | sqlite | 0 | 0 | 6 | 0 | 0 |
| Q2 | conv | 0 | 0 | 6 | 0 | 0 |
| Q3 | sqlite | 0 | 2 | 4 | 0 | 0 |
| Q3 | conv | 0 | 2 | 4 | 0 | 0 |
| Q4 | sqlite | 4 | 2 | 0 | 0 | 0 |
| Q4 | conv | 4 | 2 | 0 | 0 | 0 |
| Q5 | sqlite | 1 | 5 | 0 | 0 | 0 |
| Q5 | conv | 0 | 5 | 1 | 0 | 0 |
| Q6 | sqlite | 6 | 0 | 0 | 0 | 0 |
| Q6 | conv | 6 | 0 | 0 | 0 | 0 |
| Q7 | sqlite | 6 | 0 | 0 | 0 | 0 |
| Q7 | conv | 6 | 0 | 0 | 0 | 0 |
| Q8 | sqlite | 6 | 0 | 0 | 0 | 0 |
| Q8 | conv | 0 | 0 | 6 | 0 | 0 |

