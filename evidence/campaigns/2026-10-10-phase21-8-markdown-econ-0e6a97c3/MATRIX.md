# Phase 21.8 — cross-lane Q1–Q8 answer matrix

`g` = answered (derived), `D` = typed decline, `-` = not applicable.

| fixture | Q | VOLE | SQLite | render | VOLE↔SQLite | VOLE↔render |
|---|---|---|---|---|---|---|
| basic.md | Q1 | g | g | g | equal | equal |
| basic.md | Q2 | g | D | D | capability-gap | capability-gap |
| basic.md | Q3 | D | D | D | both-decline | both-decline |
| basic.md | Q4 | D | D | D | both-decline | both-decline |
| basic.md | Q5 | D | D | D | both-decline | both-decline |
| basic.md | Q6 | g | g | g | equal | equal |
| basic.md | Q7 | g | g | g | equal | equal |
| basic.md | Q8 | g | g | D | equal | capability-gap |
| lists.md | Q1 | g | g | g | equal | equal |
| lists.md | Q2 | g | D | D | capability-gap | capability-gap |
| lists.md | Q3 | D | D | D | both-decline | both-decline |
| lists.md | Q4 | D | D | D | both-decline | both-decline |
| lists.md | Q5 | g | g | g | equal | equal |
| lists.md | Q6 | g | g | g | equal | equal |
| lists.md | Q7 | g | g | g | equal | equal |
| lists.md | Q8 | g | g | D | equal | capability-gap |
| code.md | Q1 | g | g | g | equal | equal |
| code.md | Q2 | g | D | D | capability-gap | capability-gap |
| code.md | Q3 | g | D | D | capability-gap | capability-gap |
| code.md | Q4 | D | D | D | both-decline | both-decline |
| code.md | Q5 | D | D | D | both-decline | both-decline |
| code.md | Q6 | g | g | g | equal | equal |
| code.md | Q7 | g | g | g | equal | equal |
| code.md | Q8 | g | g | D | equal | capability-gap |
| table.md | Q1 | g | g | g | equal | equal |
| table.md | Q2 | g | D | D | capability-gap | capability-gap |
| table.md | Q3 | D | D | D | both-decline | both-decline |
| table.md | Q4 | D | D | D | both-decline | both-decline |
| table.md | Q5 | D | D | D | both-decline | both-decline |
| table.md | Q6 | g | g | g | equal | equal |
| table.md | Q7 | g | g | g | equal | equal |
| table.md | Q8 | g | g | D | equal | capability-gap |
| links.md | Q1 | g | g | g | equal | equal |
| links.md | Q2 | g | D | D | capability-gap | capability-gap |
| links.md | Q3 | D | D | D | both-decline | both-decline |
| links.md | Q4 | g | g | g | equal | equal |
| links.md | Q5 | D | D | D | both-decline | both-decline |
| links.md | Q6 | g | g | g | equal | equal |
| links.md | Q7 | g | g | g | equal | equal |
| links.md | Q8 | g | g | D | equal | capability-gap |
| blockquotes.md | Q1 | g | g | g | equal | equal |
| blockquotes.md | Q2 | g | D | D | capability-gap | capability-gap |
| blockquotes.md | Q3 | D | D | D | both-decline | both-decline |
| blockquotes.md | Q4 | D | D | D | both-decline | both-decline |
| blockquotes.md | Q5 | D | D | D | both-decline | both-decline |
| blockquotes.md | Q6 | g | g | g | mismatch | mismatch |
| blockquotes.md | Q7 | g | g | g | equal | equal |
| blockquotes.md | Q8 | g | g | D | equal | capability-gap |
| footnotes.md | Q1 | g | g | g | equal | equal |
| footnotes.md | Q2 | g | D | D | capability-gap | capability-gap |
| footnotes.md | Q3 | D | D | D | both-decline | both-decline |
| footnotes.md | Q4 | D | D | D | both-decline | both-decline |
| footnotes.md | Q5 | D | D | D | both-decline | both-decline |
| footnotes.md | Q6 | g | g | g | equal | equal |
| footnotes.md | Q7 | g | g | g | equal | equal |
| footnotes.md | Q8 | g | g | D | equal | capability-gap |
| frontmatter.md | Q1 | g | g | g | equal | equal |
| frontmatter.md | Q2 | g | D | D | capability-gap | capability-gap |
| frontmatter.md | Q3 | D | D | D | both-decline | both-decline |
| frontmatter.md | Q4 | D | D | D | both-decline | both-decline |
| frontmatter.md | Q5 | D | D | D | both-decline | both-decline |
| frontmatter.md | Q6 | g | g | g | equal | equal |
| frontmatter.md | Q7 | g | g | g | equal | equal |
| frontmatter.md | Q8 | g | g | D | equal | capability-gap |
| toml_frontmatter.md | Q1 | g | g | g | equal | equal |
| toml_frontmatter.md | Q2 | g | D | D | capability-gap | capability-gap |
| toml_frontmatter.md | Q3 | D | D | D | both-decline | both-decline |
| toml_frontmatter.md | Q4 | D | D | D | both-decline | both-decline |
| toml_frontmatter.md | Q5 | D | D | D | both-decline | both-decline |
| toml_frontmatter.md | Q6 | g | g | g | equal | equal |
| toml_frontmatter.md | Q7 | g | g | g | equal | equal |
| toml_frontmatter.md | Q8 | g | g | D | equal | capability-gap |

### Aggregate equivalence per Q

| Q | comparator | equal | both-decline | capability-gap | mismatch | shape |
|---|---|---:|---:|---:|---:|---:|
| Q1 | sqlite | 9 | 0 | 0 | 0 | 0 |
| Q1 | render | 9 | 0 | 0 | 0 | 0 |
| Q2 | sqlite | 0 | 0 | 9 | 0 | 0 |
| Q2 | render | 0 | 0 | 9 | 0 | 0 |
| Q3 | sqlite | 0 | 8 | 1 | 0 | 0 |
| Q3 | render | 0 | 8 | 1 | 0 | 0 |
| Q4 | sqlite | 1 | 8 | 0 | 0 | 0 |
| Q4 | render | 1 | 8 | 0 | 0 | 0 |
| Q5 | sqlite | 1 | 8 | 0 | 0 | 0 |
| Q5 | render | 1 | 8 | 0 | 0 | 0 |
| Q6 | sqlite | 8 | 0 | 0 | 1 | 0 |
| Q6 | render | 8 | 0 | 0 | 1 | 0 |
| Q7 | sqlite | 9 | 0 | 0 | 0 | 0 |
| Q7 | render | 9 | 0 | 0 | 0 | 0 |
| Q8 | sqlite | 9 | 0 | 0 | 0 | 0 |
| Q8 | render | 0 | 0 | 9 | 0 | 0 |

