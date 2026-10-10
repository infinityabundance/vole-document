# MessagePack

MessagePack is the **binary structured-tree** format of Phase 21 Wave 2
(subphase 21.19), the sibling of CBOR. Like JSON it is not a package — the whole
source is the document, and the exact leaf is the source. The adapter is gated
behind the **non-default** `msgpack = ["json"]` feature (dependency-free; it
reuses the shared JSON parser's match vocabulary).

## Authority boundary

MessagePack has **no magic bytes**, so detection is a documented, conservative
heuristic. It is tried **immediately after CBOR** and before the remaining
textual heuristics; the strong magic-byte binaries (PDF/ZIP/Parquet/Arrow) and
the JSON family run above it and are never reconsidered. An input is claimed only
on a full-input well-formed parse whose root is a container (array/map) reaching
at least three items and eight bytes, or the same with an unambiguous
MessagePack-only head byte (`0xdc..=0xdf`, which CBOR's grammar rejects). A
container head byte is always `>= 0x80`, so no pure-ASCII document is ever
claimed. A lone scalar, the never-used `0xc1` byte, a map key with no value,
trailing bytes, the ambiguous `fixarray(3)`/`fixmap(2)` sources, and prose all
stay `Opaque` and round-trip exactly through the RAW lane.

**What it cannot distinguish.** The whole-number/short-container prefix
**overlaps CBOR**'s encodings, so the two cannot always be told apart for a
source well-formed under both grammars. **Placement is load-bearing:** CBOR is
tried first (its self-described tag is the strongest binary signal), so such an
input is an honest `Cbor` classification and is never a MessagePack guess; a CBOR
document stays `Cbor` and is never stolen.

## Representation preservation (the point of the format)

A bounded MessagePack parser produces a representation-preserving arena
preserving the **exact format byte** actually used (encoding width **and
signedness**: `0x17` positive-fixint 23 vs `0xcc 0x17` uint8 23 vs `0xd0 0x17`
int8 23), `str` vs `bin` as **distinct kinds**, map key order and **duplicate
keys**, float width (`float32`/`float64`) with the IEEE-754 bits, and extension
type numbers + payload lengths (preserved verbatim, **never interpreted**) — each
with its exact source span.

## Supported observations

Common selectors: `metadata`, `text`, `search-match`. Native: `msgpack-pointer`
(RFC 6901), `msgpack-node`, `msgpack-find`.

## Unsupported / honest cost

Extension semantics are preserved but never interpreted; a general MessagePack
schema validator is not claimed. A malformed head (the never-used `0xc1`), a
truncated item, trailing bytes, a map key with no value, an over-long declared
length, a non-UTF-8 `str`, and an out-of-range pointer are typed declines
(`InvalidMsgpackStructure`, exit 33; `unsupported-feature`, exit 6); `Page(n)` is
a typed decline.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted
MessagePack, and after the source **and** descriptor are deleted in a fresh
process. The exact leaf is the whole source; the derived model is never on the
exactness path (ADR-0060: the model node depends on the `DocumentExact` root
keyed on `sha256(source)`). Adapter-court H1 exactness **17/17** (7 MessagePack
fixtures + the strict-JSON control + the **CBOR coexistence** control + 8 opaque
controls); 7 typed declines.

## Economic court

Measured by `tools/phase21-19-msgpack-court.sh` (a thin wrapper over the shared
`binfmt-court.sh` engine) against two conventional comparators — a
**source-retaining SQLite** store that must first normalize the binary source to
*strict* JSON, and a conventional **MessagePack → host-value load**. Corpus 11
fixtures, questions Q1–Q12, exactness **21/21**. Estimator = paired per-fixture
ratio VOLE/comparator, median + geometric mean with a fixed-seed,
fixture-clustered 95 % CI; ratio-of-sums reported separately. A ratio < 1 favours
VOLE.

| metric | comparator | median | geomean | median 95% CI | geomean 95% CI | W/T/L | ratio-of-sums |
|---|---|---:|---:|---|---|---:|---:|
| build | sqlite | 1.037 | 1.033 | 1.024..1.045 | 1.014..1.051 | 0/11/0 | 1.033 |
| build | conv | 1.432 | 1.423 | 1.398..1.469 | 1.392..1.449 | 0/0/11 | 1.422 |
| storage | sqlite | 0.571 | 0.602 | 0.570..0.572 | 0.570..0.670 | 10/1/0 | 0.630 |
| storage | conv | 36.315 | 28.353 | 33.511..38.779 | 16.572..37.983 | 0/0/11 | 8.105 |
| cold | sqlite | 0.031 | 0.031 | 0.030..0.032 | 0.030..0.032 | 11/0/0 | 0.031 |
| warm | sqlite | 0.483 | 0.571 | 0.451..0.764 | 0.468..0.724 | 9/1/1 | 0.661 |
| warm | conv | 4.107 | 4.020 | 3.567..4.625 | 3.719..4.343 | 0/0/11 | 3.704 |

Build is ~parity-to-slightly-slower than SQLite (median 1.037); storage is ~0.57×
SQLite and ~36× smaller than the conventional host-value dump; warm loses to the
conventional in-process load but wins ~0.48× vs SQLite. Cold is dominated by the
comparators' fresh-Python-process start-up.

## Honest negatives

* The **strict-JSON lane cannot represent a map with a non-text key**, nor `NaN`,
  so those sources cannot be normalized at all (recorded, e.g. the `intkeys` and
  `nonfinite` fixtures).
* **Duplicate keys** are a recorded mismatch: VOLE resolves a pointer to the
  **first** matching member and lexical find reports **two** matches, while the
  conventional load collapses to the **last** (counted as `mismatch`).
* The conventional load is deliberately the weaker comparator (it drops spans,
  encoding width/signedness, byte-vs-text, duplicate keys, extension type, and
  float width); a *representation-preserving* decoder is not built here.
* The economic court is measured on a **self-authored deterministic corpus**;
  only exact closure (Q6) is a byte-authority claim.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.19.1 MessagePack adapter court: `tools/phase21-19-1-msgpack-court.sh`
  (exactness 17/17); campaign
  [2026-10-10-phase21-19-1-msgpack-f7093672](../../evidence/campaigns/2026-10-10-phase21-19-1-msgpack-f7093672/).
- Phase 21.19 economic court: `tools/phase21-19-msgpack-court.sh` (SQLite +
  conventional MessagePack load; exactness 21/21); campaign
  [2026-10-10-phase21-19-msgpack-econ-6aecd8c5](../../evidence/campaigns/2026-10-10-phase21-19-msgpack-econ-6aecd8c5/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
