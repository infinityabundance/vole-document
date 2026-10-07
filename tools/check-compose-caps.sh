#!/bin/sh
# OOM-containment audit for compose.yaml.
#
# The host's swap is zram (RAM-backed), so an unbounded container is an
# unbounded host: a runaway build/ingest/query/fetch can drive the whole
# machine into OOM. Every service must therefore hard-cap itself:
#
#   mem_limit      hard RSS cap; exceeding it OOM-kills the *container*, never
#                  the host.
#   memswap_limit  must be present and equal to mem_limit, so the container
#                  cannot swap at all and cannot evade the cap. (memswap_limit
#                  must be >= mem_limit; setting them equal is the only way to
#                  disable swap. `0` means unlimited and is rejected.)
#   pids_limit     fork/thread-bomb bound, so a hostile input or runaway build
#                  cannot exhaust the host's PID space.
#
# `cpus` is required only on the long-running/saturation lanes and is not
# enforced here.
#
# This script exits 1 if any service in compose.yaml is missing a mandatory cap,
# if memswap_limit != mem_limit, or if mem_limit is 0 (unlimited), so a new
# uncapped lane cannot merge unnoticed.
#
#   sh tools/check-compose-caps.sh
#   docker compose run --rm --no-TTY tools sh tools/check-compose-caps.sh
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
compose="$root/compose.yaml"
[ -f "$compose" ] || { echo "check-compose-caps: compose.yaml not found" >&2; exit 1; }

awk '
  function audit() {
    if (svc == "") return
    n++
    if (!("mem_limit" in cap)) {
      bad = 1; printf "FAIL: %s: missing mem_limit\n", svc
    } else if (cap["mem_limit"] == "0") {
      bad = 1; printf "FAIL: %s: mem_limit is 0 (unlimited)\n", svc
    }
    if (!("memswap_limit" in cap)) {
      bad = 1; printf "FAIL: %s: missing memswap_limit\n", svc
    } else if (("mem_limit" in cap) && cap["memswap_limit"] != cap["mem_limit"]) {
      bad = 1
      printf "FAIL: %s: memswap_limit (%s) != mem_limit (%s)\n", svc, cap["memswap_limit"], cap["mem_limit"]
    }
    if (!("pids_limit" in cap)) {
      bad = 1; printf "FAIL: %s: missing pids_limit\n", svc
    }
  }
  /^services:[[:space:]]*$/ { insvc = 1; next }
  insvc && /^volumes:[[:space:]]*$/ { audit(); insvc = 0; svc = ""; next }
  insvc && /^  [A-Za-z0-9_.-]+:[[:space:]]*$/ {
    audit()
    svc = $1; sub(/:$/, "", svc)
    delete cap
    next
  }
  insvc && svc != "" && /^    [A-Za-z0-9_.-]+:[[:space:]]*/ {
    k = $1; sub(/:$/, "", k); cap[k] = $2
  }
  END {
    audit()
    if (bad) { printf "check-compose-caps: FAILED\n"; exit 1 }
    printf "OK: %d compose services all hard-capped (mem_limit == memswap_limit + pids_limit)\n", n
  }
' "$compose"
