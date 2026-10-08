#!/bin/sh
# Phase 23 — power-loss-proxy aggregator.
#
# Usage: sh tools/fixtures/phase23-powerloss-aggregate.sh PROXY.tsv TIMING.tsv OUTDIR
#
# PROXY.tsv columns:
#   1 case  2 backend  3 policy  4 dir_sync  5 arm  6 inject  7 manifests
#   8 state 9 nodes    10 bad_hash  11 exact  12 observe  13 verdict  14 note
# TIMING.tsv columns:
#   1 backend  2 dir_sync  3 filesystem  4 reps  5 median_ms  6 min_ms  7 max_ms
#
# Writes OUTDIR/MATRIX.md, OUTDIR/summary.json, OUTDIR/counts.txt. Pure awk (the
# `dev` image has no Python). The court decides nothing here; it counts and
# tabulates the verdicts already in the TSV.
set -eu

PROXY="$1"
TIMING="$2"
OUT="$3"
mkdir -p "$OUT"

awk -v out="$OUT" -v proxyname="$(basename "$PROXY")" '
BEGIN { FS="\t" }

# ---------- first file: the GAP-1 timing table ----------
NR == FNR {
  if (FNR == 1) next
  tmed[$1 "/" $3 "/" $2] = $5
  treps[$1 "/" $3 "/" $2] = $4
  next
}

# ---------- second file: the proxy cases ----------
FNR == 1 { next }

{
  backend = $2; policy = $3; dirsync = $4; arm = $5; inject = $6; verdict = $13
  key = backend "/" policy "/" dirsync "/" arm
  tot[key]++
  v[key "/" verdict]++
  alltot++
  if (verdict == "CRITICAL") {
    allcrit++
    if (arm == "model-drop") modeldrop_crit++; else shipped_crit++
    critcase = critcase " " $1
  }
  if (verdict == "FAIL") { allfail++; failcase = failcase " " $1 }
  if ($10 + 0 > 0) badhash += $10 + 0
  if (arm == "complete" && dirsync == "safe" && $7 + 0 > 0) complete_safe_manifest++
  if (arm == "complete" && dirsync == "safe") complete_safe_n++
  if (arm == "complete" && dirsync == "off" && $7 + 0 == 0) complete_off_lost++
  if (arm == "complete" && dirsync == "off") complete_off_n++
  if (arm == "cut" && $6 == "flush.before_sync" && backend == "packed")
    flushev[policy "/" $8]++
}

END {
  total = alltot + 0
  crit = allcrit + 0
  fail = allfail + 0
  pass = total - crit - fail

  # ------------- MATRIX.md -------------
  m = out "/MATRIX.md"
  print "# Phase 23 — power-loss proxy: durability matrix (model)" > m
  print "" > m
  printf "Source: `%s`. Cases: **%d** (PASS %d / FAIL %d / CRITICAL %d).  \n", proxyname, total, pass, fail, crit > m
  printf "Shipped-arm CRITICAL: **%d** (must be 0)   Counterfactual (`model-drop`) CRITICAL: **%d** (expected, proves sensitivity).  \n", shipped_crit + 0, modeldrop_crit + 0 > m
  printf "Wrong-byte nodes served (sum `bad_hash`): **%d**.  \n", badhash + 0 > m
  print "" > m
  print "## Grid (backend x policy x dir_sync x arm)" > m
  print "" > m
  print "| backend | policy | dir_sync | arm | PASS | CRITICAL | total |" > m
  print "|---|---|---|---|---|---|---|" > m
  nb = split("fs|packed", backs, "|")
  np = split("batch|each", pols, "|")
  nd = split("safe|off", dss, "|")
  na = split("complete|model-drop|cut", arms, "|")
  for (bi = 1; bi <= nb; bi++)
    for (pi = 1; pi <= np; pi++)
      for (di = 1; di <= nd; di++)
        for (ai = 1; ai <= na; ai++) {
          k = backs[bi] "/" pols[pi] "/" dss[di] "/" arms[ai]
          if (!(k in tot)) continue
          printf "| %s | %s | %s | %s | %d | %d | %d |\n", backs[bi], pols[pi], dss[di], arms[ai], v[k "/PASS"] + 0, v[k "/CRITICAL"] + 0, tot[k] > m
        }
  print "" > m
  print "`complete` and `cut` are the shipped arms. `model-drop` arms are" > m
  print "counterfactual model faults (a directory barrier is discarded) and must be" > m
  print "CRITICAL, or the model is not sensitive to the GAP-1 class of bug." > m
  print "" > m
  print "## GAP 1 — directory-fsync cost (median ms; bind = /work, tmp = /tmp)" > m
  print "" > m
  print "| backend | dir_sync | bind median | tmp median | reps |" > m
  print "|---|---|---|---|---|" > m
  for (bi = 1; bi <= nb; bi++)
    for (di = 1; di <= nd; di++) {
      k = backs[bi] "/bind/" dss[di]
      kt = backs[bi] "/tmp/" dss[di]
      if (!(k in tmed) && !(kt in tmed)) continue
      printf "| %s | %s | %s | %s | %s |\n", backs[bi], dss[di], tmed[k], tmed[kt], treps[k] > m
    }
  print "" > m
  print "## Batch vs Each at `flush.before_sync` (packed, open state per policy)" > m
  print "" > m
  print "| policy | store state at the cut |" > m
  print "|---|---|" > m
  npol = split("batch|each", pl, "|")
  for (i = 1; i <= npol; i++) {
    st = ""
    for (s in flushev) {
      split(s, a, "/")
      if (a[1] == pl[i]) st = st " " a[2] "x" flushev[s]
    }
    printf "| %s | %s |\n", pl[i], st > m
  }

  # ------------- counts.txt -------------
  c = out "/counts.txt"
  printf "total=%d\n", total > c
  printf "pass=%d\n", pass > c
  printf "fail=%d\n", fail > c
  printf "critical=%d\n", crit > c
  printf "shipped_critical=%d\n", shipped_crit + 0 > c
  printf "modeldrop_critical=%d\n", modeldrop_crit + 0 > c
  printf "wrong_byte_nodes=%d\n", badhash + 0 > c
  printf "complete_safe_with_manifest=%d\n", complete_safe_manifest + 0 > c
  printf "complete_safe_total=%d\n", complete_safe_n + 0 > c
  printf "complete_off_manifest_lost=%d\n", complete_off_lost + 0 > c
  printf "complete_off_total=%d\n", complete_off_n + 0 > c
  printf "verdict=%s\n", (shipped_crit == 0 && fail == 0) ? "PASS" : (shipped_crit > 0 ? "CRITICAL" : "FAIL") > c

  # ------------- summary.json -------------
  j = out "/summary.json"
  printf "{\n" > j
  printf "  \"total\": %d,\n", total > j
  printf "  \"pass\": %d,\n", pass > j
  printf "  \"fail\": %d,\n", fail > j
  printf "  \"critical\": %d,\n", crit > j
  printf "  \"shipped_critical\": %d,\n", shipped_crit + 0 > j
  printf "  \"modeldrop_critical\": %d,\n", modeldrop_crit + 0 > j
  printf "  \"wrong_byte_nodes\": %d,\n", badhash + 0 > j
  printf "  \"complete_safe_with_manifest\": {\"with\": %d, \"total\": %d},\n", complete_safe_manifest + 0, complete_safe_n + 0 > j
  printf "  \"complete_off_manifest_lost\": {\"lost\": %d, \"total\": %d},\n", complete_off_lost + 0, complete_off_n + 0 > j
  printf "  \"verdict\": \"%s\"\n", (shipped_crit == 0 && fail == 0) ? "PASS" : (shipped_crit > 0 ? "CRITICAL" : "FAIL") > j
  printf "}\n" > j

  if (shipped_crit > 0) printf "SHIPPED CRITICAL:%s\n", critcase > "/dev/stderr"
  if (fail > 0) printf "FAIL:%s\n", failcase > "/dev/stderr"
}
' "$TIMING" "$PROXY"
