# Phase 21.29 — package metadata economic court — cross-lane Q1–Q12 answer matrix (pkgmeta)

`g` = answered (derived), `D` = typed decline, `-` = not applicable.

| fixture | Q | VOLE | source-ret | conv | VOLE<->source | VOLE<->conv |
|---|---|---|---|---|---|---|
| package.json | Q1 | g | g | g | equal | equal |
| package.json | Q2 | g | D | D | capability-gap | capability-gap |
| package.json | Q3 | g | g | g | equal | equal |
| package.json | Q4 | g | g | g | equal | equal |
| package.json | Q5 | g | g | g | equal | equal |
| package.json | Q6 | g | g | D | equal | capability-gap |
| package.json | Q7 | g | D | D | capability-gap | capability-gap |
| package.json | Q8 | g | g | g | equal | equal |
| package.json | Q9 | g | g | D | equal | capability-gap |
| package.json | Q10 | g | D | D | capability-gap | capability-gap |
| package.json | Q11 | g | g | D | equal | capability-gap |
| package.json | Q12 | g | D | D | capability-gap | capability-gap |
| Cargo.toml | Q1 | g | g | g | equal | equal |
| Cargo.toml | Q2 | g | D | D | capability-gap | capability-gap |
| Cargo.toml | Q3 | g | g | g | equal | equal |
| Cargo.toml | Q4 | g | g | g | equal | equal |
| Cargo.toml | Q5 | g | g | g | equal | equal |
| Cargo.toml | Q6 | g | g | D | equal | capability-gap |
| Cargo.toml | Q7 | g | D | D | capability-gap | capability-gap |
| Cargo.toml | Q8 | g | g | g | equal | equal |
| Cargo.toml | Q9 | g | g | D | equal | capability-gap |
| Cargo.toml | Q10 | g | D | D | capability-gap | capability-gap |
| Cargo.toml | Q11 | g | g | D | equal | capability-gap |
| Cargo.toml | Q12 | g | D | D | capability-gap | capability-gap |
| pyproject.toml | Q1 | g | g | g | equal | equal |
| pyproject.toml | Q2 | g | D | D | capability-gap | capability-gap |
| pyproject.toml | Q3 | g | g | g | equal | equal |
| pyproject.toml | Q4 | g | g | g | equal | equal |
| pyproject.toml | Q5 | g | g | g | equal | equal |
| pyproject.toml | Q6 | g | g | D | equal | capability-gap |
| pyproject.toml | Q7 | g | D | D | capability-gap | capability-gap |
| pyproject.toml | Q8 | g | g | g | equal | equal |
| pyproject.toml | Q9 | g | g | D | equal | capability-gap |
| pyproject.toml | Q10 | g | D | D | capability-gap | capability-gap |
| pyproject.toml | Q11 | g | g | D | equal | capability-gap |
| pyproject.toml | Q12 | g | D | D | capability-gap | capability-gap |
| Cargo.lock | Q1 | g | g | g | equal | equal |
| Cargo.lock | Q2 | g | D | D | capability-gap | capability-gap |
| Cargo.lock | Q3 | g | g | g | equal | equal |
| Cargo.lock | Q4 | g | g | g | equal | equal |
| Cargo.lock | Q5 | g | g | g | equal | equal |
| Cargo.lock | Q6 | g | g | D | equal | capability-gap |
| Cargo.lock | Q7 | g | D | D | capability-gap | capability-gap |
| Cargo.lock | Q8 | g | g | g | equal | equal |
| Cargo.lock | Q9 | g | g | D | equal | capability-gap |
| Cargo.lock | Q10 | g | D | D | capability-gap | capability-gap |
| Cargo.lock | Q11 | g | g | D | equal | capability-gap |
| Cargo.lock | Q12 | g | D | D | capability-gap | capability-gap |
| package-lock.json | Q1 | g | g | g | equal | equal |
| package-lock.json | Q2 | g | D | D | capability-gap | capability-gap |
| package-lock.json | Q3 | g | g | g | equal | equal |
| package-lock.json | Q4 | g | g | g | equal | equal |
| package-lock.json | Q5 | g | g | g | equal | equal |
| package-lock.json | Q6 | g | g | D | equal | capability-gap |
| package-lock.json | Q7 | g | D | D | capability-gap | capability-gap |
| package-lock.json | Q8 | g | g | g | equal | equal |
| package-lock.json | Q9 | g | g | D | equal | capability-gap |
| package-lock.json | Q10 | g | D | D | capability-gap | capability-gap |
| package-lock.json | Q11 | g | g | D | equal | capability-gap |
| package-lock.json | Q12 | g | D | D | capability-gap | capability-gap |
| large.json | Q1 | g | g | g | equal | equal |
| large.json | Q2 | g | D | D | capability-gap | capability-gap |
| large.json | Q3 | g | g | g | equal | equal |
| large.json | Q4 | g | g | g | equal | equal |
| large.json | Q5 | g | g | g | equal | equal |
| large.json | Q6 | g | g | D | equal | capability-gap |
| large.json | Q7 | g | D | D | capability-gap | capability-gap |
| large.json | Q8 | g | g | g | equal | equal |
| large.json | Q9 | g | g | D | equal | capability-gap |
| large.json | Q10 | g | D | D | capability-gap | capability-gap |
| large.json | Q11 | g | g | D | equal | capability-gap |
| large.json | Q12 | g | D | D | capability-gap | capability-gap |

### Aggregate equivalence per Q

| Q | comparator | equal | both-decline | capability-gap | mismatch |
|---|---|---:|---:|---:|---:|
| Q1 | sqlite | 6 | 0 | 0 | 0 |
| Q1 | conv | 6 | 0 | 0 | 0 |
| Q2 | sqlite | 0 | 0 | 6 | 0 |
| Q2 | conv | 0 | 0 | 6 | 0 |
| Q3 | sqlite | 6 | 0 | 0 | 0 |
| Q3 | conv | 6 | 0 | 0 | 0 |
| Q4 | sqlite | 6 | 0 | 0 | 0 |
| Q4 | conv | 6 | 0 | 0 | 0 |
| Q5 | sqlite | 6 | 0 | 0 | 0 |
| Q5 | conv | 6 | 0 | 0 | 0 |
| Q6 | sqlite | 6 | 0 | 0 | 0 |
| Q6 | conv | 0 | 0 | 6 | 0 |
| Q7 | sqlite | 0 | 0 | 6 | 0 |
| Q7 | conv | 0 | 0 | 6 | 0 |
| Q8 | sqlite | 6 | 0 | 0 | 0 |
| Q8 | conv | 6 | 0 | 0 | 0 |
| Q9 | sqlite | 6 | 0 | 0 | 0 |
| Q9 | conv | 0 | 0 | 6 | 0 |
| Q10 | sqlite | 0 | 0 | 6 | 0 |
| Q10 | conv | 0 | 0 | 6 | 0 |
| Q11 | sqlite | 6 | 0 | 0 | 0 |
| Q11 | conv | 0 | 0 | 6 | 0 |
| Q12 | sqlite | 0 | 0 | 6 | 0 |
| Q12 | conv | 0 | 0 | 6 | 0 |

