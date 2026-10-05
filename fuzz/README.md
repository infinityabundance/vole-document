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

## Build

```sh
docker compose run --rm --no-TTY fuzz cargo fuzz build
```

## Run a bounded campaign

```sh
tools/fuzz.sh            # 60s per target by default (see the script for knobs)
```

## Seeds and corpus

`fuzz/seeds/` holds a handful of tiny committed inputs; the generated corpus
(`fuzz/corpus/`) and crash artifacts (`fuzz/artifacts/`) are gitignored.
`tools/fuzz.sh` copies the seeds into each target's corpus before running.

To regenerate the seeds, from the `dev` service:

```sh
docker compose run --rm --no-TTY dev sh -c '
  cargo run --locked --bin vole-document -- pdf-make-samples /tmp/samples &&
  cargo run --locked --bin vole-document -- encode /tmp/samples/notpdf.bin /tmp/notpdf.voldoc &&
  ...'
```

The committed seeds are small, deterministic, and regenerable; no third-party
document bytes are stored here.
