# Conformance and courts

## Current courts (`cargo test --all-features`, inside Docker)

| Court | File | Properties |
|---|---|---|
| Unit | `src/**` | framing, header, CRC, SHA, DRA ops/program/coverage, court, materialize |
| Exact | `tests/exact.rs` | `materialize(encode(X)) == X`, digest equal, deterministic encode, bounded overhead, empty/tiny/large/random/text/sequential corpora |
| Malformed | `tests/malformed.rs` | every single-byte flip detected; truncation rejected; unknown mandatory record fails closed; unknown optional record skipped; duplicate records rejected; future version and feature bits fail closed; expansion bounded; the court rejects inexact candidates |
| Conformance | `tests/conformance.rs` | canonical `serialize(parse(x)) == x`; model round-trip; universe-id binding; golden structural length; coverage authority split; verify report accuracy |

Run everything:

```sh
docker compose run --rm --no-TTY dev cargo test  --all-features
docker compose run --rm --no-TTY dev cargo clippy --all-targets --all-features -- -D warnings
docker compose run --rm --no-TTY dev cargo fmt --all --check
```

## Standing invariants

1. `materialize(encode(X)) == X` byte-for-byte for every admitted input.
2. Any single-byte mutation of a valid descriptor is detected (every byte is
   covered by the header CRC or a record CRC).
3. Unknown mandatory semantics fail closed; only explicitly-optional records are
   skipped.
4. Resource bounds are enforced before allocation.
5. Encoding is deterministic for a pinned universe and feature set.
6. `serialize(parse(x)) == x` for canonical descriptors.

## Fuzz targets (planned, added as parsers land)

`.voldoc` header/record parser · DRA parser/evaluator · rANS model parser · rANS
channel decoder · PDF physical scanner · PDF lexical parser · xref parser ·
stream-boundary parser · DEFLATE replay wrapper · coverage certificate · partial
materializer.

Useful properties: never panic · bounded failure · round trip · descriptor
parse/serialize stability · materialized length bound · RAW fallback preserves
bytes · mutated `voldoc` either verifies or returns a typed error. Maintain a
minimized regression fixture for every discovered crash/hang/invariant violation.

## Compatibility policy (provisional)

The format is **pre-1.0** and not a stability commitment. A descriptor's meaning
is pinned by its universe string and header version. Once v1 freezes, old streams
must decode exactly; new semantics require a new optional feature, a new
mandatory feature, a new universe, or a new major format — never a silent
reinterpretation. Golden `.voldoc` fixtures will be added at freeze time.

## Evidence

Every sealed run lives under `evidence/campaigns/<date>-<phase>-<gitsha>/` with a
`manifest.json`, `environment.json`, and results. Receipts are immutable; new runs
get new directories. Corrections are amendments, not rewrites.
