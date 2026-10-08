#!/bin/sh
# Isolation probe (2026-10-08, phase18.3 / inindex): why does the contract court's
# VOLE build sum appear dominated by one document?
#
# The probe shows nist-pdf-0017's `field-build` wall is a *store-filesystem* effect,
# not descriptor encode / observation-index work: the same 6842-seed-node store
# takes ~9.5 s on the bind-mounted `/work` tree and ~1.4 s in `/tmp`, while the
# descriptor encode and the natural-language-free field ingest are sub-3 s.
#
# Run:  docker compose run --rm --no-TTY doc-baseline sh <this script>
set -u
BIN=target/release/vole-document
F=real100-v1/documents/nist/pdf/nist-pdf-0017.pdf
P=/tmp/probe-p17
rm -rf "$P"; mkdir -p "$P"

echo "# pdf-0017 isolation probe"
echo "# bin: $BIN"
echo "# source: $F ($(stat -c %s "$F") bytes)"
echo

echo "## encode (whole candidate court, forced-nothing) -> /tmp"
/usr/bin/time -f "wall=%e s  maxrss=%M KB" "$BIN" encode "$F" "$P/p17.voldoc" >"$P/enc.json" 2>"$P/enc.time"
cat "$P/enc.time"
python3 -c 'import json;d=json.load(open("/tmp/probe-p17/enc.json"));print("winner=%s encoded_len=%s graph_ops=%s"%(d["candidate"],d["encoded_len"],d["graph_ops"]))'
echo

echo "## field-build --profile runtime, store on the bind mount (/work)"
rm -rf evidence/scratch/probe-p17-bind
/usr/bin/time -f "wall=%e s  maxrss=%M KB" "$BIN" field-build "$F" --store evidence/scratch/probe-p17-bind --profile runtime >"$P/bind.json" 2>"$P/bind.time"
cat "$P/bind.time"
echo "seed files on bind store: $(find evidence/scratch/probe-p17-bind -type f | wc -l | tr -d ' ')"
echo

echo "## field-build --profile runtime, store in /tmp"
rm -rf "$P/store"
/usr/bin/time -f "wall=%e s  maxrss=%M KB" "$BIN" field-build "$F" --store "$P/store" --profile runtime >"$P/tmp.json" 2>"$P/tmp.time"
cat "$P/tmp.time"
echo "seed files in /tmp store: $(find "$P/store" -type f | wc -l | tr -d ' ')"
echo

echo "## field-ingest of the already-encoded authority, store in /tmp"
rm -rf "$P/store2"
/usr/bin/time -f "wall=%e s  maxrss=%M KB" "$BIN" field-ingest "$P/p17.voldoc" --store "$P/store2" >"$P/ing.json" 2>"$P/ing.time"
cat "$P/ing.time"
echo

echo "## contract-court contribution of pdf-0017 (from raw/phase18-contract/onetime.tsv)"
echo "v_enc_ms = 9425 of the 11605 ms VOLE build sum (81%); the other 11 docs sum to 2180 ms."
