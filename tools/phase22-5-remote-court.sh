#!/usr/bin/env bash
# Phase 22.5 (review item P4) — remote selective materialization (MODEL court).
#
# ## Question
#
#   Bounded LOCAL reads exist (Phase 20.2). Can they extend to **remote**
#   storage? A remote field fetches a compact directory and only the necessary
#   immutable segments via byte-range reads, with a range planner that resolves
#   the dependency closure, maps nodes to offsets, coalesces neighbouring ranges,
#   batches within a byte/latency budget, fetches only required segments, verifies
#   the returned identities, and evaluates. The control is the tuned SQLite
#   envelope given a comparably optimized remote page cache + source-range
#   interface.
#
# ## THIS IS A MODEL, NOT A REAL S3 BENCHMARK
#
#   Real object storage is unavailable in the pinned lane. This court therefore
#   builds an explicitly-labelled **model** of remote selective reads on
#   **deterministic local measurements**:
#
#     * the BYTES a selective remote read must transfer are REAL: VOLE's
#       instrumented per-class physical bytes (`descriptor_bytes_read`,
#       `manifest_bytes_read`, `index_bytes_read`, `seed_bytes_read`) from the
#       `observe` JSON `stats`; SQLite's page-level bytes come from the actual
#       `pread64` offsets captured under `strace` (with `mmap_size=0`, the
#       honest remote page-cache interface);
#     * the REQUEST COUNT is REAL: a local HTTP range server
#       (`tools/fixtures/phase22-5-range-server.py`, stdlib `http.server`, HTTP
#       `Range`/`206`) is actually hit by a coalescing client, and the server's
#       per-request access log is cross-checked against the client's count;
#     * only the geometry (how the class byte counts map onto concrete ranges,
#       where the exact node->offset map is not exposed) and the latency/cost
#       constants are MODELLED. Both are stated below and in the receipt.
#
#   VOLE class bytes are mapped to a contiguous prefix of each class's namespace
#   region inside one flat immutable store image; the byte counts are exact and
#   the request count is therefore a LOWER BOUND on the true scattered read.
#
# ## Lane
#
#   Never the host:
#     docker compose run --rm --no-TTY doc-baseline bash tools/phase22-5-remote-court.sh
set -uo pipefail

cd /work
export LC_ALL=C
export PYTHONDONTWRITEBYTECODE=1

SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN=evidence/campaigns/${STAMP}-phase22-5-remote-${SHA}${TAG:+-$TAG}
RAW=$CAMPAIGN/raw
WORK=${WORK:-evidence/scratch/phase22-5-court}
rm -rf "$RAW"
mkdir -p "$RAW" "$RAW/plans" "$WORK"

MANIFEST=${MANIFEST:-real100-v1/manifest.tsv}
CORPUS=${CORPUS:-real100-v1/documents}
COMP=tools/fixtures/phase22-competitors.py
RANGE=tools/fixtures/phase22-5-range-server.py
AGG=tools/fixtures/phase22-5-remote.py
OP_TIMEOUT=${OP_TIMEOUT:-180}
CONFIG=${CONFIG:-full}
# Pre-registered narrow observations (frozen before running):
#   O1 text      : pdf `--page 1 --kind text`; docx/epub `--block 0 --kind text`
#   O2 bytes     : `--byte-range 0..64 --kind exact`
#   O3 resource  : docx/epub `--resource 0 --kind metadata`; pdf `--metadata
#                  --kind metadata` (the PDF contract has NO resource observation,
#                  so metadata substitutes; recorded, not silently swapped)
# Latency/cost model constants (stated, date 2026-10-08):
#   T = N_requests * RTT + B_transferred / BW + C_decode      (sequential, P=1)
#   cloud-same-region (PRIMARY): RTT 25 ms, BW 100 MB/s
#   edge-fast (sensitivity)    : RTT  5 ms, BW 1000 MB/s
#   wan-slow (sensitivity)     : RTT 80 ms, BW 25 MB/s
#   C_decode = measured local decode wall of the same observation.
DEFAULT_IDS="nist-pdf-0002 nist-pdf-0004 nist-pdf-0016 nist-pdf-0017 nist-docx-0005 nist-docx-0008 nist-docx-0009 nist-docx-0014 nist-epub-0003 nist-epub-0006 nist-epub-0008 nist-epub-0009"
IDS=${IDS:-$DEFAULT_IDS}

PROFILE=${PROFILE:-release}
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features";           BIN=${BIN:-${VOLE_BIN:-target/debug/vole-document}} ;;
esac

if [ -x "$BIN" ] && [ -n "${BIN_SKIP_BUILD:-}" ]; then
    echo "-- using prebuilt $BIN (BIN_SKIP_BUILD set)" >&2
else
    echo "== phase22.5 remote selective court (MODEL) ==" >&2
    echo "-- building ($PROFILE) binary" >&2
    # shellcheck disable=SC2086
    if ! cargo build $BUILD_ARGS >&2; then
        echo "phase22-5: build FAILED ($BUILD_ARGS); refusing to measure" >&2
        exit 1
    fi
fi
[ -x "$BIN" ] || { echo "phase22-5: $BIN missing; refusing to measure" >&2; exit 1; }

# Preflight: the binary must actually carry the multi-format adapters.
PREFLIGHT=""
for cand in real100-v1/documents/nist/docx/*.docx real100-v1/documents/nasa/epub/*.epub; do
    [ -f "$cand" ] && { PREFLIGHT="$cand"; break; }
done
if [ -n "$PREFLIGHT" ]; then
    expect=$(case "$PREFLIGHT" in *.docx) echo docx ;; *) echo epub ;; esac)
    if ! "$BIN" capabilities "$PREFLIGHT" 2>/dev/null | grep -q "\"adapter\":\"$expect\""; then
        echo "phase22-5: preflight failed — $BIN lacks $expect" >&2
        exit 1
    fi
fi

now_us() { echo $(( ${EPOCHREALTIME/./} )); }

# --- subset selection (identical IDs to Phase 22.2) -------------------------
python3 - "$MANIFEST" "$CORPUS" $IDS >"$RAW/subset.tsv" <<'PY'
import sys
manifest, corpus = sys.argv[1], sys.argv[2]
ids = sys.argv[3:]
EXT = {"pdf": "pdf", "docx": "docx", "epub": "epub"}
rows = []
with open(manifest) as f:
    hdr = f.readline().rstrip("\n").split("\t")
    for line in f:
        if line.strip():
            rows.append(dict(zip(hdr, line.rstrip("\n").split("\t"))))
by_id = {r["id"]: r for r in rows}
heads = {}
for r in rows:
    fam = r["revision_family_id"]
    if not fam:
        continue
    key = (int(r["byte_len"]),)
    cur = heads.get(fam)
    if cur is None or key > cur[0]:
        heads[fam] = (key, r["id"])
for i in ids:
    r = by_id.get(i)
    if not r:
        continue
    fmt = r["format"]
    path = "%s/%s/%s/%s.%s" % (corpus, r["agency"], fmt, i, EXT[fmt])
    fam = r["revision_family_id"]
    is_head = "1" if (not fam or heads.get(fam, (None, i))[1] == i) else "0"
    print("|".join([i, r["agency"], fmt, r["size_class"], r["byte_len"], r["sha256"],
                    path, fam or "", is_head]))
PY
echo "-- subset ($(wc -l <"$RAW/subset.tsv" | tr -d ' ') docs)" >&2

# --- verify the subset (read-only) ------------------------------------------
echo "-- verifying the subset (SHA-256 + length)" >&2
while IFS='|' read -r id _a _f _s _bl sha path _fam _head; do
    [ -n "$id" ] || continue
    if [ ! -f "$path" ]; then echo "   missing $path" >&2; exit 1; fi
    got_sha=$(sha256sum "$path" | cut -d' ' -f1)
    got_len=$(stat -c %s "$path")
    if [ "$got_sha" != "$sha" ] || [ "$got_len" != "$_bl" ]; then
        echo "   verify FAIL $id" >&2; exit 1
    fi
done <"$RAW/subset.tsv"

# --- answer SQL (frozen control's sql_for, depth C0 for narrow observations) --
python3 "$COMP" sqlgen --out "$RAW/sql" --depths "0" >"$RAW/sqlgen.json"

# --- raw tables -------------------------------------------------------------
HDR="id\tfmt\tsize_class\tsrc_bytes\tstore_bytes\tdb_bytes\tobs\tvole_rc\tD\tM\tI\tS\tvole_bytes\tvole_ret\tvole_decode_us\tvole_plan_ranges\tvole_clamp\tvole_fetch_requests\tvole_fetch_bytes\tvole_fetch_verified\tsql_rc\tsql_raw_preads\tsql_distinct\tsql_plan_ranges\tsql_page_bytes\tsql_fetch_requests\tsql_fetch_bytes\tsql_fetch_verified\tsql_decode_us\tsql_declined\twhole_store_requests\twhole_store_bytes\twhole_db_requests\twhole_db_bytes\tsrv_client_requests\tsrv_server_requests"
printf "$HDR\n" >"$RAW/observations.tsv"
printf 'id\tfmt\tvole_ok\tvole_rc\tsrc_len\tmat_len\tsha_match\tcmp_eq\n' >"$RAW/exact.tsv"

obslist_for() { case "$1" in pdf) echo "text bytes metadata" ;; *) echo "text bytes resource" ;; esac; }
vole_args_for() {
    case "$2" in
        text)    if [ "$1" = pdf ]; then echo "--page 1 --kind text"; else echo "--block 0 --kind text"; fi ;;
        bytes)   echo "--byte-range 0..64 --kind exact" ;;
        resource) echo "--resource 0 --kind metadata" ;;
        metadata) echo "--metadata --kind metadata" ;;
        *)       echo "" ;;
    esac
}

SRV_PID=""
cleanup() { [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null; }
trap cleanup EXIT

while IFS='|' read -r id agency fmt sclass blen sha path fam is_head; do
    [ -n "$id" ] || continue
    d="$WORK/$id"; rm -rf "$d"; mkdir -p "$d"
    echo "== $id ($fmt, $sclass, $blen B) ==" >&2

    # ---- store build (once) + flat image + SQLite full envelope (once) ------
    rm -rf "$d/store"
    timeout "$OP_TIMEOUT" "$BIN" field-build "$path" --store "$d/store" \
        --profile runtime --packed >"$d/build.json" 2>"$d/build.err"; brc=$?
    if [ "$brc" -ne 0 ]; then
        echo "   field-build rc=$brc; skipping" >&2
        continue
    fi
    field=$(python3 -c "import json;print(json.load(open('$d/build.json'))['ingest']['field'])")
    store_bytes=$(find "$d/store" -type f -printf '%s\n' | awk '{s+=$1} END{print s+0}')
    python3 "$RANGE" layout --dir "$d/store" --img "$d/store.img" --manifest "$d/layout.json" >"$d/layout.out"

    rm -f "$d/full.db" "$d/full.db-wal" "$d/full.db-shm"
    timeout "$OP_TIMEOUT" python3 "$COMP" build --config "$CONFIG" --format "$fmt" \
        --source "$path" --db "$d/full.db" --through 5 --durability "${DURABILITY:-normal}" \
        --family "$fam" --member "$id" --is-head "$is_head" \
        --metrics "$d/full.metrics.json" >"$d/full.build.json" 2>"$d/full.build.err"; sbrc=$?
    db_bytes=$(find "$d" -maxdepth 1 -name 'full.db*' -type f -printf '%s\n' 2>/dev/null | awk '{s+=$1} END{print s+0}')
    echo "   store=${store_bytes} B  db=${db_bytes} B  (sqlite build rc=$sbrc)" >&2

    # ---- range server (real HTTP, loopback) --------------------------------
    rm -f "$d/server.log"; : >"$d/server.log"
    python3 "$RANGE" serve --root "$d" --port 0 --log "$d/server.log" \
        >"$d/server.out" 2>"$d/server.err" &
    SRV_PID=$!
    for _ in $(seq 1 100); do grep -q LISTENING "$d/server.out" 2>/dev/null && break; sleep 0.05; done
    PORT=$(awk '/LISTENING/{print $2}' "$d/server.out")
    if [ -z "$PORT" ]; then
        echo "   range server failed to start" >&2; cat "$d/server.err" >&2; continue
    fi
    srvlines() { local n; n=$(wc -l <"$d/server.log" 2>/dev/null); echo "${n:-0}" | tr -d ' '; }
    srv_client=0; srv_server=0

    fetch_vole() { # $1 = plan file -> prints "requests bytes verified"
        local b a out
        b=$(srvlines)
        python3 "$RANGE" fetch --root "$d" --port "$PORT" --plan "$1" --gap 0 \
            --out "$d/.fetch.json" >/dev/null 2>"$d/.fetch.err"
        a=$(srvlines)
        srv_client=$((srv_client + $(python3 -c "import json;print(json.load(open('$d/.fetch.json'))['requests'])" 2>/dev/null || echo 0)))
        srv_server=$((srv_server + a - b))
    }

    for obs in $(obslist_for "$fmt"); do
        args=$(vole_args_for "$fmt" "$obs")
        sqlfile="$RAW/sql/C0/$fmt.$obs.sql"
        have_sql=0; [ -f "$sqlfile" ] && have_sql=1
        srv_client0=$srv_client; srv_server0=$srv_server

        # ---- VOLE: cold selective observation ---------------------------------
        t0=$(now_us)
        timeout "$OP_TIMEOUT" "$BIN" observe --store "$d/store" --packed --field "$field" \
            $args --no-cache >"$d/$obs.vole.json" 2>"$d/$obs.vole.err"; vrc=$?
        t1=$(now_us)
        D=0; M=0; I=0; S=0; VBYTES=0; VRET=0; VDEC=0
        if [ "$vrc" -eq 0 ]; then
            read D M I S VBYTES VRET VDEC <<EOF
$(python3 -c "import json;s=json.load(open('$d/$obs.vole.json'))['stats'];print(s['descriptor_bytes_read'],s['manifest_bytes_read'],s['index_bytes_read'],s['seed_bytes_read'],s['bytes_read'],s['bytes_returned'],s['wall_micros'])")
EOF
            python3 "$RANGE" derive --manifest "$d/layout.json" --img-name store.img \
                --descriptor "$D" --field "$M" --index "$I" --seed "$S" \
                --out "$RAW/plans/$id.$obs.vole.json" >"$d/$obs.derive.json"
            VPLAN=$(python3 -c "import json;print(len(json.load(open('$RAW/plans/$id.$obs.vole.json'))))")
            VCLAMP=$(python3 -c "import json;d=json.load(open('$d/$obs.derive.json'));print(json.dumps(d['clamped']))" 2>/dev/null || echo '{}')
            fetch_vole "$RAW/plans/$id.$obs.vole.json"
            VREQ=$(python3 -c "import json;print(json.load(open('$d/.fetch.json'))['requests'])")
            VXFER=$(python3 -c "import json;print(json.load(open('$d/.fetch.json'))['bytes_transferred'])")
            VVER=$(python3 -c "import json;print(json.load(open('$d/.fetch.json'))['verified'])")
        else
            VPLAN=0; VCLAMP="{}"; VREQ=0; VXFER=0; VVER=0
        fi

        # ---- SQLite: page-level remote cache (strace pread64, mmap_size=0) ----
        if [ "$have_sql" -eq 1 ]; then
            { printf 'PRAGMA mmap_size=0;\n'; cat "$sqlfile"; } >"$d/$obs.q.sql"
            strace -f -e trace=pread64 -yy -o "$d/$obs.strace" \
                sqlite3 -batch "$d/full.db" ".read $d/$obs.q.sql" >"$d/$obs.sql.out" 2>/dev/null
            SRC_RC=$?
            python3 "$RANGE" trace2plan --trace "$d/$obs.strace" --db full.db \
                --out "$RAW/plans/$id.$obs.sql.json" >"$d/$obs.trace.json"
            SQLPLAN=$(python3 -c "import json;print(len(json.load(open('$RAW/plans/$id.$obs.sql.json'))))")
            SRP=$(python3 -c "import json;print(json.load(open('$d/$obs.trace.json'))['raw_preads'])")
            SDIST=$(python3 -c "import json;print(json.load(open('$d/$obs.trace.json'))['distinct_ranges'])")
            fetch_vole "$RAW/plans/$id.$obs.sql.json"
            SREQ=$(python3 -c "import json;print(json.load(open('$d/.fetch.json'))['requests'])")
            SXFER=$(python3 -c "import json;print(json.load(open('$d/.fetch.json'))['bytes_transferred'])")
            SVER=$(python3 -c "import json;print(json.load(open('$d/.fetch.json'))['verified'])")
            SDECL=$(python3 -c "import json,sys;ls=[l for l in open('$d/$obs.sql.out') if l.startswith('{')];print(json.loads(ls[-1])['declined'] if ls else 1)" 2>/dev/null || echo 1)
            t0=$(now_us)
            timeout "$OP_TIMEOUT" sqlite3 -batch "$d/full.db" ".read $d/$obs.q.sql" >/dev/null 2>&1
            t1=$(now_us); SDEC_US=$(( t1 - t0 ))
        else
            SRC_RC=3; SQLPLAN=0; SRP=0; SDIST=0; SREQ=0; SXFER=0; SVER=0; SDECL=1; SDEC_US=0
        fi

        # ---- whole-object controls -------------------------------------------
        printf '[{"path":"store.img","offset":0,"length":%s}]' "$store_bytes" >"$d/whole_store.plan.json"
        fetch_vole "$d/whole_store.plan.json"
        WSREQ=$(python3 -c "import json;print(json.load(open('$d/.fetch.json'))['requests'])")
        WSXFER=$(python3 -c "import json;print(json.load(open('$d/.fetch.json'))['bytes_transferred'])")
        printf '[{"path":"full.db","offset":0,"length":%s}]' "$db_bytes" >"$d/whole_db.plan.json"
        fetch_vole "$d/whole_db.plan.json"
        WDREQ=$(python3 -c "import json;print(json.load(open('$d/.fetch.json'))['requests'])")
        WDXFER=$(python3 -c "import json;print(json.load(open('$d/.fetch.json'))['bytes_transferred'])")

        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
            "$id" "$fmt" "$sclass" "$blen" "$store_bytes" "$db_bytes" "$obs" \
            "$vrc" "$D" "$M" "$I" "$S" "$VBYTES" "$VRET" "$VDEC" "$VPLAN" "$VCLAMP" \
            "$VREQ" "$VXFER" "$VVER" \
            "$SRC_RC" "$SRP" "$SDIST" "$SQLPLAN" "$SXFER" "$SREQ" "$SXFER" "$SVER" "$SDEC_US" "$SDECL" \
            "$WSREQ" "$WSXFER" "$WDREQ" "$WDXFER" \
            "$((srv_client - srv_client0))" "$((srv_server - srv_server0))" \
            >>"$RAW/observations.tsv"
    done

    # ---- exact closure (per document, once) ---------------------------------
    v_ok=0; v_rc=3; mat_len=0
    timeout "$OP_TIMEOUT" "$BIN" materialize --store "$d/store" --field "$field" \
        --exact --output "$d/v.exact.bin" --packed >/dev/null 2>"$d/v.exact.err"; v_rc=$?
    if [ "$v_rc" -eq 0 ]; then
        mat_len=$(stat -c %s "$d/v.exact.bin")
        vs=$(sha256sum "$d/v.exact.bin" | cut -d' ' -f1)
        [ "$vs" = "$sha" ] && [ "$mat_len" = "$blen" ] && v_ok=1
    fi
    cmp_eq=no; [ "$v_ok" -eq 1 ] && cmp -s "$d/v.exact.bin" "$path" && cmp_eq=eq
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "$v_ok" "$v_rc" "$blen" "$mat_len" \
        "$([ "$v_ok" -eq 1 ] && echo yes || echo no)" "$cmp_eq" >>"$RAW/exact.tsv"

    kill "$SRV_PID" 2>/dev/null; wait "$SRV_PID" 2>/dev/null || true; SRV_PID=""
    rm -rf "$d"
    echo "   ok (materialize --exact $v_ok)" >&2
done <"$RAW/subset.tsv"

# --- provenance -------------------------------------------------------------
RUSTC_V=$(rustc --version 2>/dev/null | sed 's/"/\\"/g')
CARGO_V=$(cargo --version 2>/dev/null | sed 's/"/\\"/g')
ARCH=$(uname -m)
COMMIT=$(git rev-parse HEAD 2>/dev/null || echo unknown)
DIRTY=$(git --no-optional-locks status --porcelain 2>/dev/null | tr '\n' ';' | sed 's/"/\\"/g')
LOCK_SHA=$(sha256sum Cargo.lock 2>/dev/null | cut -d' ' -f1)
MANIFEST_SHA=$(sha256sum "$MANIFEST" | cut -d' ' -f1)
BIN_SHA=$(sha256sum "$BIN" 2>/dev/null | cut -d' ' -f1)
SQLITE_V=$(sqlite3 --version 2>/dev/null | cut -d' ' -f1)
PY_V=$(python3 --version 2>&1)
STRACE_V=$(strace --version 2>/dev/null | head -1 | sed 's/"/\\"/g')
BASE_STABLE=rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e
cat >"$RAW/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "22.5 — remote selective materialization (explicit MODEL, not real S3)",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_commit": "$COMMIT",
  "git_dirty": "$DIRTY",
  "arch": "$ARCH",
  "service": "doc-baseline",
  "base_image": "$BASE_STABLE",
  "rustc": "$RUSTC_V",
  "cargo": "$CARGO_V",
  "sqlite3": "$SQLITE_V",
  "python": "$PY_V",
  "strace": "$STRACE_V",
  "cargo_lock_sha256": "$LOCK_SHA",
  "manifest": "$MANIFEST",
  "manifest_sha256": "$MANIFEST_SHA",
  "bin": "$BIN",
  "bin_sha256": "$BIN_SHA",
  "profile": "$PROFILE",
  "subset": "$DEFAULT_IDS",
  "sqlite_config": "$CONFIG (through C5)",
  "observations": "O1 text; O2 bytes; O3 resource (docx/epub) / metadata (pdf, no resource capability)",
  "model": "remote-selective-v1: T = N_requests*RTT + B_transferred/BW + C_decode (sequential)",
  "cost_constants": {"primary": {"name": "cloud-same-region", "rtt_ms": 25, "bw_MBps": 100}, "fast": {"name": "edge-fast", "rtt_ms": 5, "bw_MBps": 1000}, "slow": {"name": "wan-slow", "rtt_ms": 80, "bw_MBps": 25}},
  "constant_date": "2026-10-08",
  "vole_bytes": "instrumented per-class physical bytes from observe stats (exact); mapped to contiguous prefixes of each class's flat-image namespace region (upper-bound locality; request count is a lower bound)",
  "sqlite_bytes": "page-level pread64 offsets from strace with PRAGMA mmap_size=0 (the remote page-cache interface); whole-.db download is the conservative control",
  "range_server": "tools/fixtures/phase22-5-range-server.py (stdlib http.server, Range/206), loopback only; per-request access log cross-checked against the client count",
  "accounting": "persistent bytes = sum of regular-file sizes (find -printf %s); du -sb never used",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF
cp "$RAW/environment.json" "$CAMPAIGN/environment.json"

cat >"$CAMPAIGN/commands.txt" <<'EOF'
# Phase 22.5 — remote selective materialization (explicit MODEL):
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase22-5-remote-court.sh
# Bounded / explicit population:
#   docker compose run --rm --no-TTY doc-baseline sh -c \
#     'IDS="nist-pdf-0017 nist-docx-0005" bash tools/phase22-5-remote-court.sh'
# The court builds the VOLE packed store once, measures the cold selective
# observation's instrumented per-class bytes, maps them onto ranges in a flat
# store image, hits a loopback HTTP range server with a coalescing client, and
# does the same for the SQLite page-level interface (strace pread64, mmap_size=0).
EOF

echo "-- aggregating" >&2
python3 "$AGG" "$RAW" "$CAMPAIGN" | tee "$RAW/summary.txt"

cat >"$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "22.5 — remote selective materialization (explicit MODEL, not real S3)",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "measured_commit": "$COMMIT",
  "tree_state": "$DIRTY",
  "profile": "$PROFILE",
  "bin": "$BIN",
  "bin_sha256": "$BIN_SHA",
  "base_image": "$BASE_STABLE",
  "service_image": "vole-document/doc-baseline:1.99.0",
  "service": "doc-baseline (mem_limit == memswap_limit == 6g, pids_limit 4096, cpus 8)",
  "rustc": "$RUSTC_V",
  "cargo": "$CARGO_V",
  "sqlite3": "$SQLITE_V",
  "strace": "$STRACE_V",
  "cargo_lock_sha256": "$LOCK_SHA",
  "manifest_sha256": "$MANIFEST_SHA",
  "court": "tools/phase22-5-remote-court.sh",
  "range_server": "tools/fixtures/phase22-5-range-server.py",
  "aggregator": "tools/fixtures/phase22-5-remote.py",
  "model": "remote-selective-v1 (T = N*RTT + B/BW + C_decode); PRIMARY cloud-same-region RTT 25 ms / BW 100 MB/s; sensitivity edge-fast 5 ms/1000 MB/s, wan-slow 80 ms/25 MB/s; constants dated 2026-10-08",
  "observations": "pre-registered O1 text, O2 bytes, O3 resource (docx/epub) / metadata (pdf)",
  "sqlite_control": "tools/fixtures/phase22-competitors.py config=full, through C5; page-level pread64 via strace + PRAGMA mmap_size=0",
  "accounting": "persistent bytes = sum of regular-file sizes (find -printf %s)",
  "is_a_model": "YES — real S3 is unavailable in the pinned lane; bytes/requests are real local measurements, geometry and cost constants are stated models",
  "rc_codes": "0 ok; 3 decline; 6 unsupported-feature; 124 timeout; 137 SIGKILL/OOM"
}
EOF

chown -R 1000:1000 "$CAMPAIGN" "$WORK" 2>/dev/null || true
echo "phase22.5 remote court: done — $CAMPAIGN"
