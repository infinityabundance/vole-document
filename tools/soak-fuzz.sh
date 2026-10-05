#!/bin/sh
# Longer-running deterministic soak run of the property/mutation courts.
#
# The `rans` feature is not in `default`, so every cargo invocation must enable
# it (`--all-features`); the crate does not compile without it.
set -eu
cd /work
: "${VOLE_FUZZ_ITERS:=200000}"
export VOLE_FUZZ_ITERS
echo "soak: VOLE_FUZZ_ITERS=$VOLE_FUZZ_ITERS"
cargo test --locked --all-features --test property -- --nocapture
cargo test --locked --all-features --test entropy
cargo test --locked --all-features --test goldens
cargo test --locked --all-features --test malformed
