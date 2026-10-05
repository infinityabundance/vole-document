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
| PDF channels | `tests/pdf_channels.rs` | Phase-4 gates: channel transposition (`join(split(x)) == x`); a forced typed-channel descriptor is byte-exact and its cost fully charged (models + payload non-zero, attribution sums to the serialized length); the honest `large_text_channels_compete` experiment forces RAW/BYTE_RANS/PDF_CHANNELS side by side and records the sizes, asserting only exactness and determinism — never that channels win |
| PDF oracle (script) | `tools/pdf-oracle.sh` | qpdf 11.3 differential court over a deterministic corpus: `qpdf --check` valid, object-number set agreement, `pdfinfo` page count; qpdf is an oracle, never the byte authority |
| Phase-4 ablation (script) | `tools/phase4-court.sh` | forced-candidate ablation via `encode --force KIND` (`raw`, `rle`, `byte-rans`, `pdf-physical`, `pdf-channels`) over the deterministic corpus: per-kind forced sizes, the cumulative ladder A0..A4, and the leave-one-out channel delta; every auto winner is `cmp`ed and `verify`ed byte-exact |
| PDF layout | `tests/pdf_layout.rs` | Phase-5 gates: the positional DRA ops (`MARK_OFFSET`/`EMIT_OFFSET`, DRA v4) round-trip and are bounded to 256 slots; a forced `PDF_LAYOUT` descriptor is byte-exact with fully charged cost; `classic.pdf` really marks positions and emits predicted offsets; a deliberately wrong source offset falls back to a literal and is never predicted; cross-reference streams, non-PDFs, and >255-object tables decline; identical input is deterministic; hostile corruption yields a typed error, never a panic; and the honest rejection gate asserts `PDF_LAYOUT` **loses** the complete-cost court on `classic.pdf` while the real winner round-trips exactly |
| Packed framing (Phase 5.7) | `src/dra/op.rs`, `src/adapter/pdf/layout.rs`, `tests/pdf_layout.rs` | `PACK_SEGMENTS` (opcode `0x08`, DRA v5) round-trips and rejects unknown tags, truncation, unmarked slots, bad widths, literal overruns, and unconsumed data; layout-v2 is one packed item table over one data object with adjacent literals coalesced; `many.pdf` (hundreds of xref entries) predicts ≥100 offsets and is byte-exact; forced layout-v2 beats RAW at scale but still loses to `BYTE_RANS`, asserted honestly |
| Phase-5.7 packed-framing court (script) | `tools/phase5-court.sh` | forced-candidate ablation (`encode --force KIND`) including `pdf-layout` (layout-v2) over the enlarged 11-file corpus: per-kind forced sizes, cumulative ladder A0..A5, leave-one-out layout delta, and the classic-xref subset; every auto winner is `cmp`ed and `verify`ed byte-exact, the qpdf oracle is re-checked, and the campaign `2026-10-05-phase5-4521778` is sealed |
| Packed channels (Phase 5.8) | `src/dra/op.rs`, `src/dra/program.rs`, `src/adapter/pdf/layout.rs` | `PACKED_CHANNELS` (opcode `0x09`, DRA v6) round-trips and rejects unknown item tags, truncation, unmarked slots, bad widths, literal overruns, missing channels, a declared-length mismatch, and unconsumed data; layout+rANS is one data channel plus one plan channel (serialized item table) with two models; `classic.pdf`/`bigtext.pdf`/`many.pdf` are byte-exact and deterministic, non-PDF and cross-reference-stream inputs decline, and the honest gate records that layout+rANS loses to `BYTE_RANS` |
| Phase-5.8 layout+rANS court (script) | `tools/phase5-8-court.sh` | forced-candidate ablation (`encode --force KIND`) including `pdf-layout-rans` over the 11-file corpus: per-kind forced sizes, cumulative ladder A0..A6, leave-one-out layout+rANS delta, and head-to-head win/lose/decline vs `BYTE_RANS`; every auto winner is `cmp`ed and `verify`ed byte-exact, the qpdf oracle is re-checked, and the campaign `2026-10-05-phase5-8-cf8048d` is sealed |
| PDF DEFLATE replay (Phase 6) | `tests/pdf_deflate.rs` | `DEFLATE_REPLAY` (DRA v8, `replay_codec`-tagged) and the two replay candidates: both replay variants are proposed for `flate.pdf` and every serialized descriptor reproduces the source byte-for-byte (length + digest + `cmp` + deep `verify`); shared plaintext is stored once (6 streams → strictly fewer unique plaintext channels); the rANS-plaintext lane beats the raw-plaintext lane; the unforced court on `flate.pdf` is byte-exact with an unknown winner tolerated; forcing a replay lane on a Flate-less PDF or a non-PDF is a typed `Usage` decline; replay proposal is deterministic; hostile correction blobs fail closed with a typed `CodecReplay`/`InvalidGraph` and never panic or silently reconstruct; a graph record naming an unknown `replay_codec` is rejected as `UnsupportedFeature`; a declared replay output above the static VOLE replay-profile admission limit (a policy bound; RFC 1951 permits unbounded empty non-final blocks) is rejected before the engine runs (ADR-0016); and the lexer stream-opacity change regresses no sample |
| Phase-6 replay court (script) | `tools/phase6-court.sh` | forced-candidate ablation (`encode --force KIND`) including `pdf-deflate-replay` and `pdf-deflate-replay-rans` over the 12-file corpus: per-kind forced sizes, cumulative ladder A0..A8, leave-one-out deltas for both replay mechanisms, and a head-to-head win/lose/decline vs `BYTE_RANS`; every auto winner is `cmp`ed and `verify`ed byte-exact, negative controls and typed declines are recorded verbatim, the qpdf oracle is re-checked, and the campaign `2026-10-05-phase6-0d0bb79` is sealed (the earlier DRA-v7 receipt `2026-10-05-phase6-ec92c1a` is retained as a historical amendment reference) |
| Flate correction-ratio harness (Phase 7.0) | `tests/deflate_stats.rs` | `deflate_stats` is diagnostics-only and never changes what the complete-cost court selects. On the real-zlib `flate.pdf`: exactly 6 `FlateDecode` streams, all replayed, aggregate `compressed_bytes` equals the sum of stream `data_len`, every replayed stream carries a non-empty plaintext, correction and rANS cost, and each correction is strictly smaller than its stream; the **deduplicated** shared-channel aggregate is strictly below the naive per-stream sum for a file whose plaintexts repeat; a non-PDF yields `is_pdf:false` with no streams and a zeroed summary (never an error); a PDF with no `FlateDecode` streams reports zero streams |
| Producer corpus + ratio campaign (Phase 7.0, script) | `tools/pdf-corpus.sh` | builds a locally-generated producer-stratified Flate corpus (Ghostscript `pdfwrite` at five `/PDFSETTINGS`, qpdf in four modes, a hand-written stored-block-zlib base, plus the Phase-3 synthetic set), validates every produced PDF with `qpdf --check`, records a provenance ledger (producer, version, exact command, SHA-256, `license:"locally-generated"`), and captures `deflate-stats` over every corpus PDF; every qpdf invocation passes `--deterministic-id`, so the qpdf corpus outputs are byte-reproducible across runs (Ghostscript output is not — it embeds a per-run `/ID` and timestamp); the campaign `2026-10-05-phase7-corpus-f1f8d26` is sealed and amended (not rewritten) by `2026-10-05-phase7-corpus-b-c4eb77e`, which re-measures after the lexer stream-boundary fix and records 24/24 replayed, 0 declined (acceptance 1.000) with the Phase-6 win geometry now also on qpdf transformer output (no new candidate adopted; every stream recorded verbatim) |
| Producer complete-cost court (Phase 7.0, script) | `tools/pdf-court.sh` | runs the real complete-cost court (`encode FILE OUT`) and four forced lanes (`encode --force raw|byte-rans|pdf-deflate-replay|pdf-deflate-replay-rans FILE OUT`) over every file in the locally-generated producer corpus, recording each lane's complete serialized `.voldoc` size; a forced kind the input does not propose is a typed `Usage` decline recorded as `null`; the auto winner is `verify`ed and `decode`d + `cmp`ed byte-exact; seals campaign `2026-10-05-phase7-court-99dc72e`: `PDF_DEFLATE_REPLAY_RANS` vs `BYTE_RANS` is win 3 / lose 8 / decline 12 over 23 files (the only real-producer transformer win is `qpdf-preserve-objectstreams.pdf` at −55,167 B); no new candidate adopted |

Test counts (inside the pinned `dev` image): **331** passing with `--all-features`
(0 failed, 1 ignored), of which **243** are library unit tests (plus 1 ignored);
**310** passing with the default feature set (`default = ["rans"]`, permissive-only;
the replay courts are skipped), of which **234** are library unit tests; and
**263** passing with `--no-default-features`, of which **212** are library unit
tests (the rANS- and replay-dependent integration courts are skipped).

`encode --force KIND` is the ablation surface: it runs the *same* complete-cost
court over a one-element candidate set (`KIND` in `raw`, `rle`, `byte-rans`,
`pdf-physical`, `pdf-channels`, `pdf-layout`, `pdf-layout-rans`,
`pdf-deflate-replay`, `pdf-deflate-replay-rans`). Forcing selects
a lane; it never bypasses serialization, decoding, or the byte-compare, and a kind
the input does not propose fails with a typed usage error (recorded as `null`), not
a fabricated result.

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
10. **Opt-in replay and its bounds**: the default build is permissive-only
    (`default = ["rans"]`); the DEFLATE replay stack is opt-in. At decode time a
    `DEFLATE_REPLAY` op names its semantics with a `replay_codec` tag (unknown id
    ⇒ `UnsupportedFeature`), rejects a declared output above the VOLE
    replay-profile admission limit `min(max_output_bytes, max_replay_bytes,
    2*P+1024)` for a `P`-byte plaintext *before* running the engine (ADR-0016) — a
    policy bound, since RFC 1951 permits unbounded empty non-final blocks and so
    gives no finite `f(decompressed_size)` bound — and bounds
    plaintext/corrections by `max_record_len`.

## Fuzz targets (added as parsers land)

The Phase-2 property/mutation court already exercises the `.voldoc`
header/record parser, the DRA parser/evaluator, the rANS model parser, the rANS
channel decoder, and the coverage certificate (`tests/property.rs`,
`tests/goldens.rs`, `tools/soak-fuzz.sh`). Phase 3 additionally exercises the PDF
lexical cover and physical scanner over hostile random bytes (`tests/pdf.rs`).
Phase 4 additionally exercises the typed-channel transposition and the
`INTERLEAVE_CHANNELS` evaluator, including corrupt, misaligned, and overrunning
channels (`tests/pdf_channels.rs`). Phase 5 additionally exercises the positional
DRA ops and the classic-xref layout builder, including unmarked-slot and
width-bound rejection, wrong-offset fallback, decline preconditions, and hostile
corruption (`tests/pdf_layout.rs`). Phase 5.7 additionally exercises the
`PACK_SEGMENTS` item-table parser and evaluator (unknown tags, truncation,
unmarked slots, bad widths, literal overruns, and unconsumed data) and the
coalesced layout-v2 builder (`tests/pdf_layout.rs`). Phase 5.8 additionally
exercises the `PACKED_CHANNELS` item-table-over-channels evaluator (declared-length
mismatch, missing channels, truncated plan, unconsumed data) and the layout+rANS
candidate builder (two-channel exactness, determinism, and decline) in
`src/dra/program.rs` and `src/adapter/pdf/layout.rs`. Phase 6 additionally
exercises the `DEFLATE_REPLAY` evaluator (declared-length mismatch, missing
objects, hostile correction blobs that must fail closed) in `src/dra/program.rs`,
the bounded replay wrapper in `src/codec/deflate.rs`, and the replay candidate
builders and lexer stream-opacity change over the real-zlib `flate.pdf` sample in
`tests/pdf_deflate.rs`.
Still planned for later phases: a stream-boundary parser · partial materializer.

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
