# Phase 6 — independent adversarial review

- **Reviewer:** independent adversarial reviewer (Phase 6)
- **Scope:** ADR-0015 and the Phase-6 measured result, campaign
  `2026-10-05-phase6-ec92c1a`
- **Method:** all controls reproduced in the pinned Docker services
  (`docker compose run --rm --no-TTY dev` / `tools`), same complete-cost court as
  the campaign; no measured campaign number was altered.
- **Verdict:** the numeric/exactness headline **survives**; the causal/generality
  clause ("shared **or** weakly coded") is **falsified**; several documentation
  sentences were loose.

## Angles

| # | Angle | Verdict | Basis |
| --- | --- | --- | --- |
| 1 | Exactness: `materialize(descriptor) == original_bytes` (length + SHA-256 + `byte_compare`) | PASS | every forced replay descriptor and every auto winner round-trips byte-exactly; `all_exact=true` |
| 2 | Numeric headline: `PDF_DEFLATE_REPLAY_RANS` 36,068 vs `BYTE_RANS` 49,263 on `flate.pdf` (13,195 B win) | PASS | reproduced from the sealed campaign |
| 3 | Determinism: repeated encodes are byte-identical | PASS | repeated runs identical |
| 4 | Fail-closed: bounded replay, `declared_output_len`, `catch_unwind`, mandatory feature bit | PASS | no panic, no silent reconstruction; typed decline without the feature |
| 5 | Causal claim: the win needs plaintext "shared across streams **or** weakly coded" | **FALSIFIED** | four controls below: each conjunct alone loses |
| 6 | Descriptor/label accuracy: "three **shared** channels", "six **stored** bitstreams", levels "0/1 almost verbatim" | **FALSIFIED** | only `p1` is shared; `BYTE_RANS` order-0-codes the streams, it does not store them; level 1 is ~19%, not verbatim |
| 7 | Generality: the winning region is characterized by a population | **CAVEAT** | one synthetic composed fixture; no population, no third-party PDF |

## Falsifying negative controls (angles 5–6)

Each fixture is run through the same complete-cost court. "replay loses" means
`PDF_DEFLATE_REPLAY_RANS` is **larger** than `BYTE_RANS`.

| fixture | `BYTE_RANS` | replay + rANS | outcome |
| --- | ---: | ---: | --- |
| single unique level-9 stream | 4,476 | 19,958 | replay loses **4.46×** (unique, strongly coded) |
| single unique level-0 stream | 19,418 | 19,656 | replay loses slightly — **weak coding alone loses** |
| four identical level-9 streams (shared, strong) | 13,914 | 20,403 | replay loses — **sharing alone loses** |
| `flate.pdf` with the level-0 stream replaced by a level-6 stream | 29,375 | 36,027 | replay loses — sharing without a large/weak appearance |

These falsify any reading in which sharing or weak producer coding is
*individually* sufficient. The win requires **both** conjuncts simultaneously:
the plaintext must be shared across streams **and** at least one of its
appearances must be large/weakly coded, so that order-0 rANS of the shared
plaintext costs less than the sum of the compressed appearances it replaces.

## Descriptor corrections (angle 6)

The real `PDF_DEFLATE_REPLAY_RANS` descriptor for `flate.pdf` reports
`streams=6 replayed=6 channels=3 objects=6`. Only `p1` is shared (four streams at
levels 0/1/6/9); `p2` and `p3` are unique. Of `p1`'s four appearances exactly one
is weakly coded: level 0 is stored (~verbatim, 31,998 B); level 1 is ~19% of the
plaintext (6,197 B); levels 6 and 9 are strong (3,768 B / 3,410 B). The phrases
"three shared plaintext channels", "six stored bitstreams", and "levels 0/1
almost verbatim" were therefore inaccurate and have been corrected in the docs.

## Strongest remaining caveat

The positive result is demonstrated on **one synthetic composed fixture** at
commit `ec92c1a`. The boundary between the winning and losing regions is
characterized structurally by the controls above, **not** by a corpus
population, and no independent/third-party PDF with real producer streams
exercises the lane. The numeric headline is real and byte-exact; its
generalization is not established.
