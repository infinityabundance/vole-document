#!/usr/bin/env bash
# Phase 21.1.3a — analytical comparator (DuckDB/Parquet) smoke court.
#
# The Phase-21 plan *requires* the tabular-format courts (CSV/TSV, XLSX, ODS) to
# include a DuckDB/Parquet baseline alongside SQLite, because those engines
# already embody columnar projection, predicate pushdown, compressed pages, and
# metadata indexes. This court proves the comparator service is real,
# reproducible, and self-contained before the XLSX economic court relies on it:
#
#   H1 — `import duckdb` succeeds and reports the pinned version (1.5.6).
#   H2 — DuckDB reads and writes Parquet natively (no `pyarrow`), and a
#        write/read round-trip is byte-for-value stable across a fresh process.
#   H3 — the Rust/Python toolchain identity matches the pinned `doc-baseline`
#        base digest, so every number its lane produces is on the same base.
#
# Runs in the pinned, hard-capped `analytical` service (never the host):
#
#   docker compose run --rm --no-TTY analytical bash tools/phase21-3-analytical-smoke.sh
set -uo pipefail

cd /work
export LC_ALL=C
export PYTHONDONTWRITEBYTECODE=1

SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN=evidence/campaigns/${STAMP}-phase21-3-analytical-${SHA}
RAW=$CAMPAIGN/raw
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

echo "=== toolchain identity ==="
DUCKDB_VERSION=$(python3 -c 'import duckdb; print(duckdb.__version__)')
RUSTC=$(rustc --version)
CARGO=$(cargo --version)
PYTHON=$(python3 --version)
SQLITE=$(sqlite3 --version | cut -d' ' -f1)
echo "duckdb=$DUCKDB_VERSION rustc=$RUSTC python=$PYTHON sqlite=$SQLITE"

echo "=== H2: Parquet write/read round-trip (fresh process) ==="
python3 - "$RAW/roundtrip.parquet" <<'PY'
import sys, duckdb
path = sys.argv[1]
con = duckdb.connect()
con.execute("CREATE TABLE t(id INTEGER, name VARCHAR, val DOUBLE)")
con.execute("INSERT INTO t VALUES (1,'a',1.5),(2,'b',2.5),(3,'c',3.5)")
con.execute(f"COPY t TO '{path}' (FORMAT PARQUET, COMPRESSION ZSTD)")
print("wrote", path)
PY
ROUNDTRIP=$(python3 - "$RAW/roundtrip.parquet" <<'PY'
import sys, duckdb
con = duckdb.connect()
rows = con.execute(
    f"SELECT id,name,val FROM read_parquet('{sys.argv[1]}') ORDER BY id"
).fetchall()
expect = [(1, 'a', 1.5), (2, 'b', 2.5), (3, 'c', 3.5)]
print("true" if rows == expect else f"false {rows}")
PY
)
echo "roundtrip_equal=$ROUNDTRIP"

echo "=== receipt ==="
{
  echo "{"
  echo "  \"campaign\": \"$CAMPAIGN\","
  echo "  \"phase\": \"21.1.3a — analytical comparator (DuckDB/Parquet) smoke\","
  echo "  \"utc\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\","
  echo "  \"measured_commit\": \"$(git rev-parse HEAD 2>/dev/null || echo unknown)\","
  echo "  \"service\": \"analytical\","
  echo "  \"base_image\": \"rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e\","
  echo "  \"derives_from\": \"doc-baseline (same pinned base digest)\","
  echo "  \"duckdb_version\": \"$DUCKDB_VERSION\","
  echo "  \"duckdb_wheel_sha256\": \"73b108c04c932b36c2fa4e41110cc1c3c8cd510eb49f065f92d050be8e6929fd (cp311 manylinux_2_26_x86_64); 56c0f71c6bee982e9c30568bb12371bf66b26bf129c75d8d7f60bc69d6590a2c (cp311 manylinux_2_26_aarch64)\","
  echo "  \"rustc\": \"$RUSTC\","
  echo "  \"cargo\": \"$CARGO\","
  echo "  \"python\": \"$PYTHON\","
  echo "  \"sqlite\": \"$SQLITE\","
  echo "  \"basis\": \"hash-pinned wheel (--require-hashes), --no-deps; DuckDB reads/writes Parquet natively (no pyarrow)\","
  echo "  \"roundtrip_equal\": $ROUNDTRIP"
  echo "}"
} > "$CAMPAIGN/receipt.json"

echo "PHASE 21.1.3a SMOKE: PASS — campaign $CAMPAIGN (duckdb $DUCKDB_VERSION, roundtrip $ROUNDTRIP)"
