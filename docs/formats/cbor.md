# CBOR

CBOR (RFC 8949) is the **binary structured-tree** format of Phase 21 Wave 2
(subphase 21.18). Like JSON it is not a package — the whole source is the
document, and the exact leaf is the source. The adapter is gated behind the
**non-default** `cbor = ["json"]` feature (dependency-free; it reuses the shared
JSON parser's match vocabulary).

## Authority boundary

CBOR has **no magic bytes**, so detection is a documented, conservative
heuristic. It is tried **after** the strong magic-byte binaries
(PDF/ZIP/Parquet/Arrow) and the JSON family (JSON/JSON5/JSONL), and **before**
the remaining textual heuristics. An input is claimed only on the
self-described-CBOR tag `55799` (`0xd9 0xd9 0xf7`) or a full-input well-formed
parse whose root is a container/tag and which reaches at least three nodes; a
container head byte is always `>= 0x80`, so no pure-ASCII document is ever
claimed. A lone scalar, a truncated item, a map key with no value, an
unterminated item, a MessagePack source, and prose all stay `Opaque` and
round-trip exactly through the RAW lane.

**What it cannot distinguish.** The whole-number/short-container prefix
**overlaps MessagePack's** `fixint`/`fixarray` encodings; the two cannot always
be told apart, so ambiguous inputs are not guessed (they stay `Opaque`). The
Phase-21.19 MessagePack adapter shares this seam by ordering CBOR first.

## Representation preservation (the point of the format)

A bounded CBOR parser produces a representation-preserving arena preserving
every major type, the **encoding width actually used** (`0x17` vs `0x1817`),
byte string vs text string as **distinct kinds**, tag numbers (preserved,
**never resolved**), map order and **duplicate keys**, float width
(half/single/double), and definite vs indefinite-length items — each with its
exact source span.

## Supported observations

Common selectors: `metadata`, `text`, `search-match`. Native: `cbor-pointer`
(RFC 6901), `cbor-node`, `cbor-find`.

## Unsupported / honest cost

Tag/extension semantics are preserved but never interpreted; a general CBOR
schema or CDDL validator is not claimed. A malformed head/value/map/string and an
out-of-range pointer are typed declines (`InvalidCborStructure`, exit 32;
`unsupported-feature`, exit 6); `Page(n)` is a typed decline.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted CBOR,
including documents the adapter declines to interpret, and after the source
**and** descriptor are deleted in a fresh process. The exact leaf is the whole
source; the derived model is never on the exactness path (ADR-0060: the model
node depends on the `DocumentExact` root keyed on `sha256(source)`). Adapter-court
H1 exactness **16/16** (8 CBOR fixtures + the strict-JSON control + 7 opaque
controls); 8 typed declines.

## Economic court

Measured by `tools/phase21-18-cbor-court.sh` (a thin wrapper over the shared
`binfmt-court.sh` engine) against two conventional comparators — a
**source-retaining SQLite** store that must first normalize the binary source to
*strict* JSON, and a conventional **CBOR → host-value load**. Corpus 11 fixtures,
questions Q1–Q12, exactness **20/20**. Estimator = paired per-fixture ratio
VOLE/comparator, median + geometric mean with a fixed-seed, fixture-clustered
95 % CI; ratio-of-sums reported separately. A ratio < 1 favours VOLE.

| metric | comparator | median | geomean | median 95% CI | geomean 95% CI | W/T/L | ratio-of-sums |
|---|---|---:|---:|---|---|---:|---:|
| build | sqlite | 1.042 | 1.036 | 1.023..1.071 | 1.009..1.062 | 0/10/1 | 1.035 |
| build | conv | 1.433 | 1.427 | 1.391..1.479 | 1.399..1.454 | 0/0/11 | 1.426 |
| storage | sqlite | 0.570 | 0.601 | 0.569..0.571 | 0.569..0.669 | 10/1/0 | 0.629 |
| storage | conv | 35.406 | 27.933 | 33.440..38.333 | 16.369..37.415 | 0/0/11 | 8.069 |
| cold | sqlite | 0.031 | 0.031 | 0.030..0.032 | 0.031..0.032 | 11/0/0 | 0.031 |
| warm | sqlite | 0.467 | 0.573 | 0.433..0.824 | 0.462..0.746 | 9/1/1 | 0.686 |
| warm | conv | 3.750 | 3.996 | 3.625..4.346 | 3.742..4.308 | 0/0/11 | 3.845 |

Build is ~parity-to-slightly-slower than SQLite (median 1.042); storage is ~0.57×
SQLite and ~35× smaller than the conventional host-value dump (which carries no
source bytes); warm loses to the conventional in-process load (it keeps the
parsed host value in memory) but wins ~0.47× vs SQLite. Cold is dominated by the
comparators' fresh-Python-process start-up and is reported for completeness.

## Honest negatives

* The **strict-JSON lane is a real comparator limitation**: a store fronted by
  strict JSON cannot represent a **map with a non-text key**, nor `NaN`, so those
  sources cannot be normalized at all (recorded, e.g. the `mapkeys` and
  `nonfinite` fixtures).
* **Duplicate keys** are a recorded mismatch: VOLE resolves a pointer to the
  **first** matching member and lexical find reports **two** matches, while the
  conventional load collapses to the **last** (counted as `mismatch`, never
  hidden).
* The conventional load is deliberately the weaker comparator (it drops spans,
  encoding width/signedness, byte-vs-text, duplicate keys, tag numbers, and float
  width); a *representation-preserving* decoder is not built here.
* The economic court is measured on a **self-authored deterministic corpus**;
  only exact closure (Q6) is a byte-authority claim.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.18.1 CBOR adapter court: `tools/phase21-18-1-cbor-court.sh`
  (exactness 16/16); campaigns
  [2026-10-10-phase21-18-1-cbor-f7093672](../../evidence/campaigns/2026-10-10-phase21-18-1-cbor-f7093672/)
  (canonical, at the adapter commit) and the earlier
  [2026-10-10-phase21-18-1-cbor-5f9ae385](../../evidence/campaigns/2026-10-10-phase21-18-1-cbor-5f9ae385/).
- Phase 21.18 economic court: `tools/phase21-18-cbor-court.sh` (SQLite +
  conventional CBOR load; exactness 20/20); campaign
  [2026-10-10-phase21-18-cbor-econ-6aecd8c5](../../evidence/campaigns/2026-10-10-phase21-18-cbor-econ-6aecd8c5/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
