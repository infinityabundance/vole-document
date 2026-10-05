# AGENTS.md — working rules for contributors and agents

## Prime directive

For the exact profile the only acceptable outcome is:

```text
materialize(descriptor) == original_bytes
```

Parsing, semantic equality, canonicalization, rendering, and re-saving are not
substitutes. Every exact court must require all three:

```text
materialized_length == source_length
SHA256(materialized) == SHA256(source)
byte_compare(materialized, source) == equal
```

## Docker only

Never run `cargo`, `rustc`, `rustfmt`, `clippy`, tests, benches, fuzzing, `qpdf`,
Poppler, MuPDF, Ghostscript, or corpus generation on the host. Use the pinned
services in `compose.yaml`:

```sh
docker compose run --rm --no-TTY dev  <command>
docker compose run --rm --no-TTY msrv <command>
docker compose run --rm --no-TTY tools <command>
```

Every evidence receipt must record the base image name and digest, `rustc`/`cargo`
versions, `Cargo.lock` SHA-256, git commit and dirty state, CPU architecture,
external oracle versions, the exact command line, and any environment variables
that affect semantics. GitHub Actions must invoke the same Docker paths.

## Subagent protocol

The orchestrator owns integration. Major phases begin with independent research
subagents; no phase is designed, implemented, and self-approved by one reasoning
path. Recurring roles:

```text
A specification / prior-art researcher
B PDF / document-format forensic specialist
C entropy / rANS specialist
D reconstruction-format / DRA architect
E EntropyFS integration specialist
F DSFB / residual-search specialist
G hostile-input / security skeptic
H evidence / benchmarking specialist
I independent adversarial reviewer
```

Rules:

- Subagents must not concurrently edit the same files. Research subagents are
  **read-only**; durable findings are written under the gitignored
  `research/subagents/phase-XX/`.
- Always include a skeptic whose job is to try to **falsify** the phase's headline
  claim.
- A subagent's confidence is not evidence. Empirical courts decide.
- Freeze a phase contract before implementing; have a *different* subagent audit
  the implementation against the frozen contract afterward.

## Claim discipline

Do not write or imply: "the true generating program was discovered"; "residuals
are irreducible"; "one seed stores the document"; "rANS state alone reconstructs
arbitrary data"; "we beat information theory"; "dedup is compression"; "a
pretrained model is free"; "semantic equality is archival equality"; "Rust means
the parser is secure". State measured, scoped, reproducible claims instead.

## Engineering attitude

For every design ask: does this preserve exactness? Does it encode a real
distinction? Can it be bounded, independently tested, and can it fall back? Does
it pay its own byte cost? Does it contaminate decoder authority? Can the claim be
falsified? When a simpler mechanism wins, use the simpler mechanism; when it
loses, preserve the evidence and change the design.
