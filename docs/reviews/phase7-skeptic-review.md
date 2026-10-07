# Phase 7.0 — independent adversarial review

- **Reviewer:** independent adversarial reviewer (Phase 7.0)
- **Scope:** the Phase-7.0 producer-stratified corpus, the ratio diagnostic
  (`2026-10-05-phase7-corpus-b-c4eb77e`) and the complete-cost court
  (`2026-10-05-phase7-court-99dc72e`), and the claim that the Phase-6 win
  "reproduces on qpdf transformer output".
- **Method:** all controls reproduced in the pinned Docker services
  (`docker compose run --rm --no-TTY dev` / `tools`), the same complete-cost
  court and the same `deflate-stats` harness as the campaigns; no measured
  campaign number was altered.
- **Verdict:** the **numeric and exactness results SURVIVE** (all 23 auto winners
  are byte-exact; the court sizes are as sealed). The **producer framing is
  FALSIFIED**: the one "qpdf win" is a `--object-streams=preserve` copy of our
  own fixture. One statistic was **misattributed**.

## Angles

| # | Angle | Verdict | Basis |
| --- | --- | --- | --- |
| 1 | Numeric/exactness: sealed court sizes; every auto winner round-trips (`verify` + `decode`/`cmp`) | PASS | reproduced from the sealed campaign; 23/23 byte-exact |
| 2 | Determinism: qpdf corpus outputs byte-reproducible | PASS | `--deterministic-id` derives `/ID` from a content hash; two builds identical |
| 3 | `--deterministic-id` semantic effect: is it load-bearing for the win? | PASS (with caveat) | it is a `/ID`-only normalization: 55,165 B no-flag vs 55,167 B flagged; it neither creates nor destroys the win, but the qpdf file must not be presented as an unmodified producer artifact |
| 4 | Producer framing: "the only real-producer transformer output that wins" / "now reproduces on qpdf transformer output" | **FALSIFIED** | provenance below: the qpdf file is a preserve-copy of `hand-base2.pdf` |
| 5 | Attribution: 99.93% of the qpdf win is inherited from our fixture | **FALSIFIED** (as a producer result) | qpdf adds only +41 B of margin; the fixture already wins 112,011 → 56,885 B |
| 6 | Statistic: `correction/compressed` corpus-wide p50 | **FALSIFIED** (misattribution) | corpus-wide p50 is **0.014716**; the previously cited 0.004518 is the `pdf-make-samples` subset median only |

## Provenance finding (angles 4–5)

`tools/pdf-corpus.sh` writes `hand-base2.pdf` objects 5 and 6 as **two copies of
the same** hand-built stored-block zlib payload, then runs
`qpdf --deterministic-id --object-streams=preserve hand-base2.pdf
qpdf-preserve-objectstreams.pdf`. `--object-streams=preserve` **copies and
renumbers** the objects; it does not re-compress or re-author them. Confirmed in
the pinned `tools` image:

```text
qpdf --show-object=5 --raw-stream-data hand-base2.pdf                  | sha256sum
qpdf --show-object=6 --raw-stream-data hand-base2.pdf                  | sha256sum
qpdf --show-object=4 --raw-stream-data qpdf-preserve-objectstreams.pdf | sha256sum
qpdf --show-object=5 --raw-stream-data qpdf-preserve-objectstreams.pdf | sha256sum
→ all four = ec028dc1cb5be9722d57fbefb0c112fda11edbf1cc9206bdbea0bd7a033b1590
```

All four raw streams are **byte-identical** (`ec028dc1…`). The fixture already
wins complete cost 112,011 → 56,885 B (−55,126 B); the qpdf file wins
112,147 → 56,980 B (−55,167 B). The difference is **+41 B**, so
`55,126 / 55,167 = 99.93%` of the reported qpdf win is inherited from the
fixture we authored. qpdf produced the *container*, not the winning geometry.

## Which producer outputs actually exhibit the region

| corpus file | producer | truly transformed? | exhibits shared-plaintext region? | replay vs `BYTE_RANS` |
| --- | --- | --- | --- | --- |
| `hand-base2.pdf` | hand (VOLE shell writer) | **no (our fixture)** | **yes** | win −55,126 |
| `_synthetic/flate.pdf` | `pdf-make-samples` | **no (our fixture)** | **yes** | win −13,189 |
| `qpdf-preserve-objectstreams.pdf` | qpdf `--object-streams=preserve` | **copy of fixture** | yes (inherited) | win −55,167 (99.93% inherited) |
| `gs-default/-ebook/-prepress/-printer/-screen` | Ghostscript 10.00.0 | yes | no | **lose** |
| `qpdf-compress`, `qpdf-linearize` | qpdf 11.3.0 | yes | no | **lose** |
| `hand-base1.pdf` | hand (our fixture) | no | no | lose |
| `qpdf-nocompress` + 11 synthetic | qpdf / fixtures | mixed | no replay lane | **decline** |

**Every genuinely transformed producer output loses or declines.** The court is
**win 3 / lose 8 / decline 12, and all 3 wins are self-authored** (two fixtures
plus a preserve-copy of one).

## Corrected statistic

`correction/compressed` over all 24 replayed streams:
**p10 = 0.000320, p50 = 0.014716, p90 = 0.097360** (nearest rank; corpus-wide).
The value **0.004518** — previously cited as if corpus-wide — is the
`pdf-make-samples` **subset** median (6 streams, all in `_synthetic/flate.pdf`).
Every per-producer row in the corpus report is a subset statistic and must be
labelled as such. The sealed `results.json` values are correct: they list
`analysis.overall.raw_p50 = 0.014716` and, separately,
`analysis.per_producer[pdf-make-samples].raw_p50 = 0.004518`.

## Strongest remaining caveat

The Phase-6 win is real and byte-exact (angle 1), and the boundary of the win
region is structurally characterized by the Phase-6 controls. What is **not**
established — and was overclaimed — is that a real producer *creates* the
enabling condition (a plaintext shared across streams). On this locally generated
corpus it does not. Closing that gap is exactly the task of Phase 7.2 (nested
content proceduralization of the plaintext itself); until then the candidate's
`ADOPTED` status means "implemented and can win when shared plaintext is
present", not "shown to win on real producer output".
