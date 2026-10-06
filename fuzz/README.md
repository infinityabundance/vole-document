# VOLE-Document coverage-guided fuzz targets

This is a standalone [`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz)
package (excluded from `cargo package` by the root `Cargo.toml`) that fuzzes the
hostile parser/reconstruction surfaces of `vole-document` with coverage-guided
libFuzzer.

## Toolchain (pinned, never floating)

Everything runs in the `fuzz` Docker service. The base is a **dated** nightly,
pinned by index digest in `Dockerfile`:

- image: `rustlang/rust:nightly-bookworm-slim-2026-10-04`
- digest: `sha256:58f725c9459a637c19ddf24542d2aee7d93704684c9780b168e81504327ad297`
- toolchain: `nightly-2026-10-04` (rustc `1.101.0-nightly (db8f076d2 2026-10-03)`)
- `cargo-fuzz 0.13.2`, `libfuzzer-sys 0.4.13`

The service sets `RUSTUP_TOOLCHAIN=nightly-2026-10-04` to override the root
`rust-toolchain.toml` (which requests stable 1.99.0).

## Targets

| Target | Surface |
| --- | --- |
| `voldoc_parse` | `.voldoc` header/record parse + materialize |
| `voldoc_roundtrip` | `encode` → `decode` byte-exactness + determinism |
| `dra_program` | DRA `Program::decode` + `analyze_inputs` + `eval` |
| `rans_model` | `EntropyModel::decode` + canonical re-encode stability |
| `rans_channel` | `decode_channel` over a fuzz-derived model |
| `pdf_lexer` | PDF lexical cover (`lex`) invariant |
| `pdf_scan` | PDF physical scanner cover invariant |
| `pdf_xref` | xref/revision + `/Prev` handling via `scan` |
| `deflate_replay` | `codec::deflate::replay_raw` (plaintext, corrections) |
| `materializer` | `materialize` / `verify` on arbitrary bytes |
| `zip_scan` | Phase-12 ZIP physical scanner cover invariant |
| `zip_decode` | bounded ZIP member inflate + stored CRC (12.2) |
| `opc_rels` | OPC content types / relationships / part-name resolution (12.3) |
| `docx_wml` | WordprocessingML story/styles extraction (12.4) |
| `epub_package` | OCF/Package/nav discovery (12.5) |
| `epub_content` | bounded XHTML content model (12.6) |
| `xml_part` | shared bounded-XML part policy (12.5) |
| `common_observe` | format detection + capability self-consistency (12.7) |

## Build

```sh
docker compose run --rm --no-TTY fuzz cargo fuzz build
```

## Run a bounded campaign

```sh
tools/fuzz.sh            # 60s per target by default (see the script for knobs)
```

## Seeds and corpus

`fuzz/seeds/` holds five tiny committed inputs (`classic.pdf`, `malformed.pdf`,
`notpdf.bin`, `notpdf.voldoc`, `text.voldoc`); the generated corpus
(`fuzz/corpus/`), campaign logs (`fuzz/campaign/`), and crash/OOM artifacts
(`fuzz/artifacts/`) are gitignored. `tools/fuzz.sh` copies the seeds into each
target's corpus before running.

To regenerate the seeds, from the `dev` service:

```sh
cargo run --locked --bin vole-document -- pdf-make-samples /tmp/samples
cargo run --locked --bin vole-document -- encode /tmp/samples/notpdf.bin /work/fuzz/seeds/notpdf.voldoc
printf 'the quick brown fox jumps over the lazy dog\n' > /tmp/text.txt
cargo run --locked --bin vole-document -- encode /tmp/text.txt /work/fuzz/seeds/text.voldoc
cp /tmp/samples/notpdf.bin /tmp/samples/malformed.pdf /tmp/samples/classic.pdf /work/fuzz/seeds/
```

The committed seeds are small, deterministic, and regenerable; no third-party
document bytes are stored here.

## Findings and regressions

The first campaign (`2026-10-05-phase7-fuzz-ca6a92b`) found two upstream
`preflate-rs` 0.7.6 issues in `deflate_replay`. Their minimized inputs live in
`tests/fixtures/` and `tests/fuzz_regressions.rs`:

- `deflate_replay_shift_overflow.bin` (35 B) — a shift-overflow panic; the fuzz
targets install a non-aborting panic hook so `replay_raw`'s `catch_unwind`
isolation works, and the test asserts a typed `CodecReplay` error.
- `deflate_replay_unbounded_alloc.bin` (33 B) — a multi-gigabyte reconstruction
allocation; an unresolved upstream resource limitation (ADR-0016). The fixture
is committed but deliberately not executed in-process by CI.

The Phase-12 campaign (`FUZZ_SECONDS=10`, all 18 targets; receipt
`evidence/campaigns/2026-10-06-phase12-security-33f6d04/fuzz-campaign/`) added the
eight targets above and found **no new crash, hang, or resource amplification**.
The only artifact was the known `deflate_replay` OOM (F2 above). The hostile
seeds in `fuzz/seeds/hostile*` are copied from the committed Phase-12 corpus
(`tests/fixtures/phase12-hostile/`).
