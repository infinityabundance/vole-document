# Phase 7.1 — coverage-guided fuzzing campaign

Campaign: `2026-10-05-phase7-fuzz-ca6a92b`
Revision under test: `ca6a92b469a11e03e759305844a431e5ffd7a57b` (`phase7`, tracked tree clean)
Status: **PASS with two reported upstream findings**

## What was run

A bounded, coverage-guided libFuzzer campaign over every hostile
parser/reconstruction surface, inside the pinned `fuzz` Docker service:

```sh
docker compose run --rm --no-TTY fuzz sh tools/fuzz.sh
```

Toolchain (pinned; never floating):

- base image `rustlang/rust:nightly-bookworm-slim-2026-10-04@sha256:58f725c9459a637c19ddf24542d2aee7d93704684c9780b168e81504327ad297`
- toolchain `nightly-2026-10-04` — `rustc 1.101.0-nightly (db8f076d2 2026-10-03)`, `cargo 1.101.0-nightly (f3865b2a4 2026-09-29)`
- `cargo-fuzz 0.13.2`, `libfuzzer-sys 0.4.13`, address sanitizer
- policy: 60 s per target, `-rss_limit_mb=2048`, artifacts under `fuzz/artifacts/<target>/`

Ten targets: `voldoc_parse`, `voldoc_roundtrip`, `dra_program`, `rans_model`,
`rans_channel`, `pdf_lexer`, `pdf_scan`, `pdf_xref`, `deflate_replay`,
`materializer`. Seeds (5 committed inputs, `fuzz/seeds/`) are copied into each
target's corpus before the run.

## Results

| target | duration s | exit | crashes | coverage (cov/ft) | execs |
| --- | ---: | ---: | ---: | --- | ---: |
| voldoc_parse | 62 | 0 | 0 | 89 / 91 | 23,348,092 |
| voldoc_roundtrip | 61 | 0 | 0 | 1547 / 3588 | 179,596 |
| dra_program | 61 | 0 | 0 | 515 / 1190 | 2,626,569 |
| rans_model | 61 | 0 | 0 | 163 / 206 | 58,889,038 |
| rans_channel | 61 | 0 | 0 | 336 / 989 | 3,657,623 |
| pdf_lexer | 61 | 0 | 0 | 231 / 1167 | 7,636,556 |
| pdf_scan | 61 | 0 | 0 | 790 / 4260 | 1,551,184 |
| pdf_xref | 61 | 0 | 0 | 736 / 4229 | 1,470,572 |
| deflate_replay | 103 | 1 | 1 (OOM) | 1301 / 2012 | 2,361 |
| materializer | 62 | 0 | 0 | 83 / 85 | 19,481,521 |

Nine of ten targets finished with zero crashes. Coverage is the libFuzzer
inline-8-bit-counter count; it is scoped to this build, not a property of the
code.

## Findings

### F1 — preflate-rs 0.7.6 shift-overflow panic (mitigated, fail-closed)

Reconstructing from a hostile correction blob reaches an unchecked
`1 << params.window_bits` (`preflate-rs/src/hash_chain_holder.rs:325`) and panics
with *attempt to shift left with overflow* under debug assertions. libFuzzer's
panic hook aborts the process **before unwinding**, which would defeat
`replay_raw`'s deliberate `catch_unwind` isolation. The fuzz targets now install
a printing-but-non-aborting panic hook so the library's documented boundary is
exercised as in production; genuinely uncaught panics still abort via
libfuzzer-sys and are reported. Minimized fixture (35 B):
`tests/fixtures/deflate_replay_shift_overflow.bin`; regression test asserts
`replay_raw` returns `CodecReplay` (never unwinds into the caller).

### F2 — preflate-rs 0.7.6 unbounded reconstruction memory (upstream, reported)

A 33-byte hostile `(plaintext, corrections)` pair drives a multi-gigabyte
allocation inside `recreate_whole_deflate_stream` (peak RSS 2532 MiB at the
campaign's 2048 MiB policy, recorded as `oom-333c997d…`). `REPLAY_OUTPUT_RATIO`
policy bounds only the *declared output* length, not the third-party decoder's
internal allocation, and ADR-0016 already records that preflate-rs 0.7.6 offers
no bounded streaming reconstruction sink. There is no in-tree fix that avoids
`unsafe` (forbidden) or process isolation. Minimized fixture (33 B):
`tests/fixtures/deflate_replay_unbounded_alloc.bin`; the committed test asserts
the public analysis boundary declines it (fail-closed), and the in-process
reconstruction is deliberately not executed by CI.

## Scope and honesty

- This is a bounded 60 s-per-target campaign, not a proof of absence. Zero
  crashes on nine targets means no crash was observed in this budget on this
  build.
- The single crash is an upstream `preflate-rs` resource/robustness defect
  surfaced by our wrapper, not a clean-execution result.
- Fuzzing does not change the wire format, candidates, or any `.voldoc`
  semantics; the campaign only adds test evidence.
