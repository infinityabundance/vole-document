# Phase 21.30 — API specification economic court — cross-lane Q1–Q12 answer matrix (apispec)

`g` = answered (derived), `D` = typed decline, `-` = not applicable.

| fixture | Q | VOLE | source-ret | conv | VOLE<->source | VOLE<->conv |
|---|---|---|---|---|---|---|
| schema.json | Q1 | g | g | g | equal | equal |
| schema.json | Q2 | g | D | D | capability-gap | capability-gap |
| schema.json | Q3 | g | g | D | equal | capability-gap |
| schema.json | Q4 | g | g | g | equal | equal |
| schema.json | Q5 | g | g | g | equal | equal |
| schema.json | Q6 | g | g | D | equal | capability-gap |
| schema.json | Q7 | g | D | D | capability-gap | capability-gap |
| schema.json | Q8 | g | D | D | capability-gap | capability-gap |
| schema.json | Q9 | g | g | g | equal | equal |
| schema.json | Q10 | g | g | g | equal | equal |
| schema.json | Q11 | g | g | D | equal | capability-gap |
| schema.json | Q12 | g | D | D | capability-gap | capability-gap |
| openapi.json | Q1 | g | g | g | equal | equal |
| openapi.json | Q2 | g | D | D | capability-gap | capability-gap |
| openapi.json | Q3 | g | g | D | equal | capability-gap |
| openapi.json | Q4 | g | g | g | equal | equal |
| openapi.json | Q5 | g | g | g | equal | equal |
| openapi.json | Q6 | g | g | D | equal | capability-gap |
| openapi.json | Q7 | g | D | D | capability-gap | capability-gap |
| openapi.json | Q8 | g | D | D | capability-gap | capability-gap |
| openapi.json | Q9 | g | g | g | equal | equal |
| openapi.json | Q10 | g | g | g | equal | equal |
| openapi.json | Q11 | g | g | D | equal | capability-gap |
| openapi.json | Q12 | g | D | D | capability-gap | capability-gap |
| swagger.json | Q1 | g | g | g | equal | equal |
| swagger.json | Q2 | g | D | D | capability-gap | capability-gap |
| swagger.json | Q3 | g | g | D | equal | capability-gap |
| swagger.json | Q4 | g | g | g | equal | equal |
| swagger.json | Q5 | D | D | D | both-decline | both-decline |
| swagger.json | Q6 | g | g | D | equal | capability-gap |
| swagger.json | Q7 | g | D | D | capability-gap | capability-gap |
| swagger.json | Q8 | D | D | D | both-decline | both-decline |
| swagger.json | Q9 | g | g | g | equal | equal |
| swagger.json | Q10 | g | g | g | equal | equal |
| swagger.json | Q11 | g | g | D | equal | capability-gap |
| swagger.json | Q12 | g | D | D | capability-gap | capability-gap |
| asyncapi.json | Q1 | g | g | g | equal | equal |
| asyncapi.json | Q2 | g | D | D | capability-gap | capability-gap |
| asyncapi.json | Q3 | g | g | D | equal | capability-gap |
| asyncapi.json | Q4 | g | g | g | equal | equal |
| asyncapi.json | Q5 | D | D | D | both-decline | both-decline |
| asyncapi.json | Q6 | g | g | D | equal | capability-gap |
| asyncapi.json | Q7 | g | D | D | capability-gap | capability-gap |
| asyncapi.json | Q8 | D | D | D | both-decline | both-decline |
| asyncapi.json | Q9 | g | g | g | equal | equal |
| asyncapi.json | Q10 | g | g | g | equal | equal |
| asyncapi.json | Q11 | g | g | D | equal | capability-gap |
| asyncapi.json | Q12 | g | D | D | capability-gap | capability-gap |
| large.json | Q1 | g | g | g | equal | equal |
| large.json | Q2 | g | D | D | capability-gap | capability-gap |
| large.json | Q3 | g | g | D | equal | capability-gap |
| large.json | Q4 | g | g | g | equal | equal |
| large.json | Q5 | D | D | D | both-decline | both-decline |
| large.json | Q6 | g | g | D | equal | capability-gap |
| large.json | Q7 | g | D | D | capability-gap | capability-gap |
| large.json | Q8 | D | D | D | both-decline | both-decline |
| large.json | Q9 | g | g | g | equal | equal |
| large.json | Q10 | g | g | g | equal | equal |
| large.json | Q11 | g | g | D | equal | capability-gap |
| large.json | Q12 | g | D | D | capability-gap | capability-gap |

### Aggregate equivalence per Q

| Q | comparator | equal | both-decline | capability-gap | mismatch |
|---|---|---:|---:|---:|---:|
| Q1 | sqlite | 5 | 0 | 0 | 0 |
| Q1 | conv | 5 | 0 | 0 | 0 |
| Q2 | sqlite | 0 | 0 | 5 | 0 |
| Q2 | conv | 0 | 0 | 5 | 0 |
| Q3 | sqlite | 5 | 0 | 0 | 0 |
| Q3 | conv | 0 | 0 | 5 | 0 |
| Q4 | sqlite | 5 | 0 | 0 | 0 |
| Q4 | conv | 5 | 0 | 0 | 0 |
| Q5 | sqlite | 2 | 3 | 0 | 0 |
| Q5 | conv | 2 | 3 | 0 | 0 |
| Q6 | sqlite | 5 | 0 | 0 | 0 |
| Q6 | conv | 0 | 0 | 5 | 0 |
| Q7 | sqlite | 0 | 0 | 5 | 0 |
| Q7 | conv | 0 | 0 | 5 | 0 |
| Q8 | sqlite | 0 | 3 | 2 | 0 |
| Q8 | conv | 0 | 3 | 2 | 0 |
| Q9 | sqlite | 5 | 0 | 0 | 0 |
| Q9 | conv | 5 | 0 | 0 | 0 |
| Q10 | sqlite | 5 | 0 | 0 | 0 |
| Q10 | conv | 5 | 0 | 0 | 0 |
| Q11 | sqlite | 5 | 0 | 0 | 0 |
| Q11 | conv | 0 | 0 | 5 | 0 |
| Q12 | sqlite | 0 | 0 | 5 | 0 |
| Q12 | conv | 0 | 0 | 5 | 0 |

