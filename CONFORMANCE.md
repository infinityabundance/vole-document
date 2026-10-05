# Conformance and courts

## Current courts (`cargo test --all-features`, inside Docker)

| Court | File | Properties |
|---|---|---|
| Unit | `src/**` | framing, header, CRC, SHA, DRA ops/program/coverage, court, materialize |
| Exact | `tests/exact.rs` | `materialize(encode(X)) == X`, digest equal, deterministic encode, bounded overhead, empty/tiny/large/random/text/sequential corpora |
| Malformed | `tests/malformed.rs` | every single-byte flip detected; truncation rejected; unknown mandatory record fails closed; unknown optional record skipped; duplicate records rejected; future version and feature bits fail closed; expansion bounded; the court rejects inexact candidates |
| Conformance | `tests/conformance.rs` | canonical `serialize(parse(x)) == x`; model round-trip; universe-id binding; golden structural length; coverage authority split; verify report accuracy |
| Entropy | `tests/entropy.rs` | Phase-2 acceptance gates: byte-exact round-trip; complete cost (model bytes charged, no "free model"); negative controls (RAW on incompressible/tiny, RLE on runs, deterministic tie-break); determinism; honest record when rANS loses |
| Goldens | `tests/goldens.rs` | **reference-oracle parity** (a from-scratch, integer-only decoder agrees byte-for-byte with `entropy::rans::decode_channel`, both ending at the encoder lower bound with the payload fully consumed); frozen model/capsule/descriptor golden bytes; truncation and single-byte flips ⇒ typed error; model decode never panics |
| Property | `tests/property.rs` | deterministic mutation/round-trip fuzzing over descriptor/model/channel/DRA parsers: `decode(encode(x)) == x`; `parse(serialize(d)) == d`; random and mutated bytes never panic and never report an internal invariant; oversized claims are bounded; limits never change reconstructed bytes |
| Soak (script) | `tools/soak-fuzz.sh` | longer deterministic run (`VOLE_FUZZ_ITERS`, default `200000`) of the property, entropy, goldens, and malformed courts |
| PDF physical | `tests/pdf.rs` | Phase-3 gates: total contiguous coverage of every corpus PDF; forced physical materialization byte-exact; validated (not extension-based) detection; incremental revisions; literal/hex/stream traps never split objects; hostile random bytes never panic; the court honestly still prefers RAW in Phase 3 |
| PDF oracle (script) | `tools/pdf-oracle.sh` | qpdf 11.3 differential court over a deterministic corpus: `qpdf --check` valid, object-number set agreement, `pdfinfo` page count; qpdf is an oracle, never the byte authority |

Test counts (inside the pinned `dev` image): **228** with all features (the
default set), **196** with `--no-default-features` (the rANS-dependent
integration courts are skipped), of which **166** are library unit tests (with
all features).

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
7. **Reference-oracle parity**: an independent, from-scratch decoder reproduces
   `entropy::rans::decode_channel` byte-for-byte for every tested model, and both
   consume the payload exactly and return the decoder state to `RANS_BYTE_L`.
8. **Structural exact-consumption**: a channel stream is accepted only if it
   consumes its payload exactly and the decoder state returns to `RANS_BYTE_L`.
   This is a structural invariant, *not* a checksum: it rejects truncation and
   many corruptions, while content corruption is caught by the enclosing
   per-record CRC-32C and the whole-source SHA-256 — not by the channel check.
9. **Feature-set behaviour**: with the `rans` feature, channel-bearing
   descriptors materialize byte-for-byte; without it, channel-free descriptors
   still materialize exactly and channel-bearing ones fail closed with
   `UnsupportedFeature` (never a silent reinterpretation).

## Fuzz targets (added as parsers land)

The Phase-2 property/mutation court already exercises the `.voldoc`
header/record parser, the DRA parser/evaluator, the rANS model parser, the rANS
channel decoder, and the coverage certificate (`tests/property.rs`,
`tests/goldens.rs`, `tools/soak-fuzz.sh`). Phase 3 additionally exercises the PDF
lexical cover and physical scanner over hostile random bytes (`tests/pdf.rs`).
Still planned for later phases: an xref parser · a stream-boundary parser ·
DEFLATE replay wrapper · partial materializer.

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
