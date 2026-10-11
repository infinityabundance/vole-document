# Phase 21.28 — log stream economic court — cross-lane Q1–Q12 answer matrix (logstream)

`g` = answered (derived), `D` = typed decline, `-` = not applicable.

| fixture | Q | VOLE | source-ret | conv | VOLE<->source | VOLE<->conv |
|---|---|---|---|---|---|---|
| generic.log | Q1 | g | g | g | equal | equal |
| generic.log | Q2 | g | D | D | capability-gap | capability-gap |
| generic.log | Q3 | g | g | g | equal | equal |
| generic.log | Q4 | g | g | g | equal | equal |
| generic.log | Q5 | g | D | D | capability-gap | capability-gap |
| generic.log | Q6 | g | g | D | equal | capability-gap |
| generic.log | Q7 | g | D | D | capability-gap | capability-gap |
| generic.log | Q8 | g | D | D | capability-gap | capability-gap |
| generic.log | Q9 | g | g | g | equal | equal |
| generic.log | Q10 | g | g | g | equal | equal |
| generic.log | Q11 | g | g | D | equal | capability-gap |
| generic.log | Q12 | g | D | D | capability-gap | capability-gap |
| rfc5424.log | Q1 | g | D | D | capability-gap | capability-gap |
| rfc5424.log | Q2 | g | D | D | capability-gap | capability-gap |
| rfc5424.log | Q3 | g | g | g | equal | equal |
| rfc5424.log | Q4 | g | g | g | equal | equal |
| rfc5424.log | Q5 | g | D | D | capability-gap | capability-gap |
| rfc5424.log | Q6 | g | g | D | equal | capability-gap |
| rfc5424.log | Q7 | g | D | D | capability-gap | capability-gap |
| rfc5424.log | Q8 | g | D | D | capability-gap | capability-gap |
| rfc5424.log | Q9 | g | g | g | equal | equal |
| rfc5424.log | Q10 | g | g | g | equal | equal |
| rfc5424.log | Q11 | g | g | D | equal | capability-gap |
| rfc5424.log | Q12 | g | D | D | capability-gap | capability-gap |
| rfc3164.log | Q1 | g | D | D | capability-gap | capability-gap |
| rfc3164.log | Q2 | g | D | D | capability-gap | capability-gap |
| rfc3164.log | Q3 | g | g | g | equal | equal |
| rfc3164.log | Q4 | g | g | g | equal | equal |
| rfc3164.log | Q5 | g | D | D | capability-gap | capability-gap |
| rfc3164.log | Q6 | g | g | D | equal | capability-gap |
| rfc3164.log | Q7 | g | D | D | capability-gap | capability-gap |
| rfc3164.log | Q8 | g | D | D | capability-gap | capability-gap |
| rfc3164.log | Q9 | g | g | g | equal | equal |
| rfc3164.log | Q10 | g | g | g | equal | equal |
| rfc3164.log | Q11 | g | g | D | equal | capability-gap |
| rfc3164.log | Q12 | g | D | D | capability-gap | capability-gap |
| crlf.log | Q1 | g | g | g | equal | equal |
| crlf.log | Q2 | g | D | D | capability-gap | capability-gap |
| crlf.log | Q3 | g | g | g | equal | equal |
| crlf.log | Q4 | g | g | g | equal | equal |
| crlf.log | Q5 | g | D | D | capability-gap | capability-gap |
| crlf.log | Q6 | g | g | D | equal | capability-gap |
| crlf.log | Q7 | g | D | D | capability-gap | capability-gap |
| crlf.log | Q8 | g | D | D | capability-gap | capability-gap |
| crlf.log | Q9 | g | g | g | equal | equal |
| crlf.log | Q10 | g | g | g | equal | equal |
| crlf.log | Q11 | g | g | D | equal | capability-gap |
| crlf.log | Q12 | g | D | D | capability-gap | capability-gap |
| blank.log | Q1 | g | g | g | equal | equal |
| blank.log | Q2 | g | D | D | capability-gap | capability-gap |
| blank.log | Q3 | g | g | g | equal | equal |
| blank.log | Q4 | g | g | g | equal | equal |
| blank.log | Q5 | g | D | D | capability-gap | capability-gap |
| blank.log | Q6 | g | g | D | equal | capability-gap |
| blank.log | Q7 | g | D | D | capability-gap | capability-gap |
| blank.log | Q8 | g | D | D | capability-gap | capability-gap |
| blank.log | Q9 | g | g | g | equal | equal |
| blank.log | Q10 | g | g | g | equal | equal |
| blank.log | Q11 | g | g | D | equal | capability-gap |
| blank.log | Q12 | g | D | D | capability-gap | capability-gap |
| large.log | Q1 | g | g | g | equal | equal |
| large.log | Q2 | g | D | D | capability-gap | capability-gap |
| large.log | Q3 | g | g | g | equal | equal |
| large.log | Q4 | g | g | g | equal | equal |
| large.log | Q5 | g | D | D | capability-gap | capability-gap |
| large.log | Q6 | g | g | D | equal | capability-gap |
| large.log | Q7 | g | D | D | capability-gap | capability-gap |
| large.log | Q8 | g | D | D | capability-gap | capability-gap |
| large.log | Q9 | g | g | g | equal | equal |
| large.log | Q10 | g | g | g | equal | equal |
| large.log | Q11 | g | g | D | equal | capability-gap |
| large.log | Q12 | g | D | D | capability-gap | capability-gap |

### Aggregate equivalence per Q

| Q | comparator | equal | both-decline | capability-gap | mismatch |
|---|---|---:|---:|---:|---:|
| Q1 | sqlite | 4 | 0 | 2 | 0 |
| Q1 | conv | 4 | 0 | 2 | 0 |
| Q2 | sqlite | 0 | 0 | 6 | 0 |
| Q2 | conv | 0 | 0 | 6 | 0 |
| Q3 | sqlite | 6 | 0 | 0 | 0 |
| Q3 | conv | 6 | 0 | 0 | 0 |
| Q4 | sqlite | 6 | 0 | 0 | 0 |
| Q4 | conv | 6 | 0 | 0 | 0 |
| Q5 | sqlite | 0 | 0 | 6 | 0 |
| Q5 | conv | 0 | 0 | 6 | 0 |
| Q6 | sqlite | 6 | 0 | 0 | 0 |
| Q6 | conv | 0 | 0 | 6 | 0 |
| Q7 | sqlite | 0 | 0 | 6 | 0 |
| Q7 | conv | 0 | 0 | 6 | 0 |
| Q8 | sqlite | 0 | 0 | 6 | 0 |
| Q8 | conv | 0 | 0 | 6 | 0 |
| Q9 | sqlite | 6 | 0 | 0 | 0 |
| Q9 | conv | 6 | 0 | 0 | 0 |
| Q10 | sqlite | 6 | 0 | 0 | 0 |
| Q10 | conv | 6 | 0 | 0 | 0 |
| Q11 | sqlite | 6 | 0 | 0 | 0 |
| Q11 | conv | 0 | 0 | 6 | 0 |
| Q12 | sqlite | 0 | 0 | 6 | 0 |
| Q12 | conv | 0 | 0 | 6 | 0 |

