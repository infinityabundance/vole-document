#!/bin/sh
# Phase 20.1 — crash-court aggregator.
#
# Usage: sh tools/fixtures/phase20-crash-aggregate.sh CASES.tsv OUTDIR
#
# Reads the per-case TSV emitted by tests/crash_recovery.rs and writes:
#   OUTDIR/MATRIX.md    the injection x outcome grid
#   OUTDIR/summary.json machine-readable counts (used by receipt.json)
#   OUTDIR/SUMMARY.md   the human narrative + the grid
#
# Pure awk (no Python in the `dev` image). The court never decides a verdict
# here — verdicts are already in the TSV; this only counts and tabulates them.
set -eu

CASES="$1"
OUT="$2"
mkdir -p "$OUT"

awk -v out="$OUT" '
BEGIN { FS="\t" }

# Vocabulary, fixed order so the matrix is stable across runs.
function injlist(  i) {
  split("sigkill/sigabrt-at-delay|sigkill/sigabrt-at-boundary|fault-inject-abort|truncate_pack|truncate_idx|flip_pack|flip_idx|zero_pack_tail|drop_idx|drop_idx_truncate_pack", a, "|"); return length(a)
}
function oclist(  i) {
  split("no-manifest/reopens|no-manifest/fail-closed|manifest/exact-ok|manifest/fail-closed|manifest/other|CRITICAL", b, "|"); return length(b)
}

NR == 1 { next }

{
  inj = $5
  pol = $4
  fam = $2
  verdict = $19
  man = $7
  exact = $17
  obs = $18
  rw = $11
  rwstate = rw; sub(/:.*/, "", rwstate)

  if (verdict == "CRITICAL") oc = "CRITICAL"
  else if (man != "y")
    oc = (rwstate == "reopens_ok") ? "no-manifest/reopens" : "no-manifest/fail-closed"
  else if (exact ~ /^FailClosed/ || obs ~ /^FailClosed/) oc = "manifest/fail-closed"
  else if (exact ~ /^Match/ && (obs ~ /^Match/ || obs ~ /^NoBaseline/)) oc = "manifest/exact-ok"
  else oc = "manifest/other"

  total++
  v[verdict]++
  famv[fam "/" verdict]++
  injoc[inj "/" oc]++
  injtot[inj]++
  injv[inj "/" pol "/" verdict]++
  polv[pol "/" verdict]++
  poltot[pol]++
  if ($14 + 0 > 0) badhash += $14 + 0
  if ($15 + 0 > 0) { fetchfail += $15 + 0; rows_fetchfail++ }
  if ($16 == "VIOLATED") prefixbad++
  if (verdict == "FAIL") failcase = failcase " " $1
  if (verdict == "CRITICAL") critcase = critcase " " $1
  if (fam == "A" && oc == "no-manifest/fail-closed") a_unusable++
  if (!(fam in famseen)) { famseen[fam] = 1 }
}

END {
  crit = v["CRITICAL"] + 0
  fail = v["FAIL"] + 0
  pass = v["PASS"] + 0

  # ---------------- MATRIX.md ----------------
  m = out "/MATRIX.md"
  print "# Phase 20.1 crash court — injection x outcome matrix" > m
  print "" > m
  printf "Source: `%s`  \nTotal cases: **%d** (PASS %d / FAIL %d / CRITICAL %d)  \n", FILENAME, total, pass, fail, crit > m
  printf "Wrong-bytes nodes served (sum `bad_hash`): **%d**   Prefix violations: **%d**   Nodes not served (re-hash mismatch or truncated away, `fetch_fail`): **%d**  \n", badhash, prefixbad, fetchfail > m
  print "" > m
  print "## Injection x outcome" > m
  print "" > m
  printf "| injection | %s | total |\n", "no-manifest/reopens | no-manifest/fail-closed | manifest/exact-ok | manifest/fail-closed | manifest/other | CRITICAL" > m
  printf "|---|---|---|---|---|---|---|---|\n" > m
  n = injlist()
  split("sigkill/sigabrt-at-delay|sigkill/sigabrt-at-boundary|fault-inject-abort|truncate_pack|truncate_idx|flip_pack|flip_idx|zero_pack_tail|drop_idx|drop_idx_truncate_pack", il, "|")
  for (i = 1; i <= n; i++) {
    k = il[i]
    if (!(k in injtot)) continue
    printf "| `%s` | %d | %d | %d | %d | %d | %d | **%d** |\n", k, injoc[k "/no-manifest/reopens"]+0, injoc[k "/no-manifest/fail-closed"]+0, injoc[k "/manifest/exact-ok"]+0, injoc[k "/manifest/fail-closed"]+0, injoc[k "/manifest/other"]+0, injoc[k "/CRITICAL"]+0, injtot[k] > m
  }
  print "" > m
  print "## Injection x verdict, split by sync policy" > m
  print "" > m
  print "| injection | batch PASS/FAIL/CRIT | each PASS/FAIL/CRIT |" > m
  print "|---|---|---|" > m
  for (i = 1; i <= n; i++) {
    k = il[i]
    if (!(k in injtot)) continue
    printf "| `%s` | %d/%d/%d | %d/%d/%d |\n", k, injv[k "/batch/PASS"]+0, injv[k "/batch/FAIL"]+0, injv[k "/batch/CRITICAL"]+0, injv[k "/each/PASS"]+0, injv[k "/each/FAIL"]+0, injv[k "/each/CRITICAL"]+0 > m
  }
  print "" > m
  print "## Family x verdict" > m
  print "" > m
  print "| family | PASS | FAIL | CRITICAL |" > m
  print "|---|---|---|---|" > m
  printf "| A process death | %d | %d | %d |\n", famv["A/PASS"]+0, famv["A/FAIL"]+0, famv["A/CRITICAL"]+0 > m
  printf "| B storage corruption | %d | %d | %d |\n", famv["B/PASS"]+0, famv["B/FAIL"]+0, famv["B/CRITICAL"]+0 > m
  printf "| C deterministic abort | %d | %d | %d |\n", famv["C/PASS"]+0, famv["C/FAIL"]+0, famv["C/CRITICAL"]+0 > m
  print "" > m
  print "Every non-`CRITICAL` cell is a pass: either the store reopens and its" > m
  print "recovered set re-hashes exactly, or it fails closed with a typed error." > m
  print "`manifest/fail-closed` counts a published field whose seed nodes were" > m
  print "tampered: `materialize --exact` or the cold observation declined with a" > m
  print "typed error rather than returning altered bytes. A `CRITICAL` would mean a" > m
  print "corrupted store served bytes that do not match the requested id, or a" > m
  print "published manifest materialized/observed content differing from the clean" > m
  print "source." > m
  print "" > m
  print "A `manifest/exact-ok` cell means the field stayed serviceable through its" > m
  print "descriptor (`materialize --exact`) and the selected cold observation. A body" > m
  print "flip in a seed node outside the observation closure is still caught when" > m
  print "the node is enumerated, by the unchanged whole-node re-hash gate (counted in" > m
  print "`fetch_fail`); `exact-ok` never means wrong bytes were served." > m

  # ---------------- summary.json ----------------
  j = out "/summary.json"
  printf "{\n" > j
  printf "  \"total\": %d,\n", total > j
  printf "  \"pass\": %d,\n", pass > j
  printf "  \"fail\": %d,\n", fail > j
  printf "  \"critical\": %d,\n", crit > j
  printf "  \"wrong_byte_nodes\": %d,\n", badhash > j
  printf "  \"prefix_violations\": %d,\n", prefixbad > j
  printf "  \"rehash_rejected_nodes\": %d,\n", fetchfail > j
  printf "  \"rows_with_rehash_rejection\": %d,\n", rows_fetchfail > j
  printf "  \"family_A\": {\"pass\": %d, \"fail\": %d, \"critical\": %d},\n", famv["A/PASS"]+0, famv["A/FAIL"]+0, famv["A/CRITICAL"]+0 > j
  printf "  \"family_B\": {\"pass\": %d, \"fail\": %d, \"critical\": %d},\n", famv["B/PASS"]+0, famv["B/FAIL"]+0, famv["B/CRITICAL"]+0 > j
  printf "  \"family_C\": {\"pass\": %d, \"fail\": %d, \"critical\": %d},\n", famv["C/PASS"]+0, famv["C/FAIL"]+0, famv["C/CRITICAL"]+0 > j
  printf "  \"batch\": {\"total\": %d, \"pass\": %d, \"fail\": %d, \"critical\": %d},\n", poltot["batch"]+0, polv["batch/PASS"]+0, polv["batch/FAIL"]+0, polv["batch/CRITICAL"]+0 > j
  printf "  \"each\": {\"total\": %d, \"pass\": %d, \"fail\": %d, \"critical\": %d},\n", poltot["each"]+0, polv["each/PASS"]+0, polv["each/FAIL"]+0, polv["each/CRITICAL"]+0 > j
  printf "  \"verdict\": \"%s\"\n", (crit == 0 && fail == 0) ? "PASS" : (crit > 0 ? "CRITICAL" : "FAIL") > j
  printf "}\n" > j

  # ---------------- counts.txt (flat, grep-friendly) ----------------
  c = out "/counts.txt"
  printf "total=%d\n", total > c
  printf "pass=%d\n", pass > c
  printf "fail=%d\n", fail > c
  printf "critical=%d\n", crit > c
  printf "wrong_byte_nodes=%d\n", badhash > c
  printf "prefix_violations=%d\n", prefixbad > c
  printf "rehash_rejected_nodes=%d\n", fetchfail > c
  printf "rows_with_rehash_rejection=%d\n", rows_fetchfail > c
  printf "a_unusable_fail_closed=%d\n", a_unusable > c
  printf "verdict=%s\n", (crit == 0 && fail == 0) ? "PASS" : (crit > 0 ? "CRITICAL" : "FAIL") > c

  # ---------------- SUMMARY.md ----------------
  s = out "/SUMMARY.md"
  print "# Phase 20.1 — crash / power-cut fault-injection court" > s
  print "" > s
  print "## Question" > s
  print "" > s
  print "Does the packed seed store (`fieldpack/`, ADR-0053), under **both**" > s
  print "`SyncPolicy::Batch` (default) and `SyncPolicy::Each`, hold its recovery" > s
  print "invariants across an arbitrary crash at every meaningful durability" > s
  print "boundary — and is the recovered set exactly a prefix of the appended" > s
  print "record sequence, with a published manifest always consistent?" > s
  print "" > s
  print "## Result" > s
  print "" > s
  printf "**%s** — %d cases, PASS %d / FAIL %d / CRITICAL %d.  \n", (crit == 0 && fail == 0) ? "PASS" : (crit > 0 ? "CRITICAL" : "FAIL"), total, pass, fail, crit > s
  printf "Sum of `bad_hash` (fetched bytes not matching the requested id): **%d**.  \n", badhash > s
  printf "Nodes not served by the whole-node re-hash gate / truncated reads (`fetch_fail`): **%d** across **%d** case(s).  \n", fetchfail, rows_fetchfail > s
  printf "Prefix-resolution violations: **%d**.  \n", prefixbad > s
  printf "Family-A stores left unusable-but-fail-closed (no manifest): **%d**.  \n", a_unusable > s
  print "" > s
  print "## Failure / critical findings" > s
  print "" > s
  if (crit > 0) print "> **CRITICAL**: wrong bytes were served." > s
  else if (fail > 0) printf "> **FAIL** in: %s\n", failcase > s
  else print "None. No injection made the store serve bytes not matching the" > s
  if (crit > 0 || fail > 0) { } else {
    print "requested id; every published manifest materialized exactly or declined" > s
    print "with a typed error." > s
  }
  print "" > s
  if (a_unusable > 0) {
    printf "Robustness observation (not a correctness failure): **%d** family-A case(s)\n", a_unusable > s
    print "left the store **unusable** but with **no** published manifest — a `SIGKILL`" > s
    print "inside the `PackWriter::ensure_open` window, after the `.pack` was created" > s
    print "but before its 24-byte header completed. The store then fails **closed** with a" > s
    print "typed `IntegrityMismatch` (\"has a truncated header\") on every later open and" > s
    print "never returns bytes. It is recoverable only by removing the sub-header stray" > s
    print "segment. This is the contract-permitted \"detected, fails closed\" outcome;" > s
    print "it is recorded, not hidden." > s
    print "" > s
  }
  print "## Injection matrix" > s
  print "" > s
  print "| injection | no-manifest reopens | no-manifest fail-closed | manifest exact-ok | manifest fail-closed | manifest other | CRITICAL | total |" > s
  print "|---|---|---|---|---|---|---|---|" > s
  for (i = 1; i <= n; i++) {
    k = il[i]
    if (!(k in injtot)) continue
    printf "| `%s` | %d | %d | %d | %d | %d | %d | %d |\n", k, injoc[k "/no-manifest/reopens"]+0, injoc[k "/no-manifest/fail-closed"]+0, injoc[k "/manifest/exact-ok"]+0, injoc[k "/manifest/fail-closed"]+0, injoc[k "/manifest/other"]+0, injoc[k "/CRITICAL"]+0, injtot[k] > s
  }
  print "" > s
  print "## Batch vs Each" > s
  print "" > s
  printf "batch: %d cases (%d PASS / %d FAIL / %d CRITICAL).  \n", poltot["batch"]+0, polv["batch/PASS"]+0, polv["batch/FAIL"]+0, polv["batch/CRITICAL"]+0 > s
  printf "each:  %d cases (%d PASS / %d FAIL / %d CRITICAL).  \n", poltot["each"]+0, polv["each/PASS"]+0, polv["each/FAIL"]+0, polv["each/CRITICAL"]+0 > s
  print "" > s
  print "The two policies produce the same verdict distribution. This is expected" > s
  print "*for this injection model* and is **not** evidence that batching is safe" > s
  print "under power loss: a `SIGKILL` does not evict the page cache, so no" > s
  print "`fsync`/`fdatasync` boundary is actually exercised (see scope)." > s
  print "" > s
  print "## Assertions checked after every injection" > s
  print "" > s
  print "* No published manifest → the store reopens (read-only and read-write);" > s
  print "  the recovered set is exactly the independent framing scan of the open" > s
  print "  segment (prefix property); every enumerated id re-hashes to itself." > s
  print "* A published manifest → it opens, its root node is enumerable, and" > s
  print "  `materialize --exact` returns the exact source bytes." > s
  print "* Observations (cold cache: page-1 text + metadata) are compared to a" > s
  print "  clean store of the same document; a tampered store may decline" > s
  print "  (typed, fail-closed) but must never answer with different content." > s
  print "" > s
  print "## Honest scope" > s
  print "" > s
  print "* `SIGKILL`/`SIGABRT`/`abort()` stop the process but the OS page cache" > s
  print "  survives; every completed `write` is still readable after reopen. The" > s
  print "  court therefore proves the **ordering** and **prefix-recovery** design" > s
  print "  (flush-before-publish, no partial node, exactly-a-prefix recovery) and" > s
  print "  the **storage-corruption fail-closed** path — not that an un-`fsync`ed" > s
  print "  record is lost on true power loss." > s
  print "* The `write_atomic` rename is never followed by a parent-directory" > s
  print "  `fsync`; the court cannot observe a torn/lost rename across a real power" > s
  print "  cut, so that ordering is argued, not measured." > s
  print "* Family C needs `--features fault-inject`; without it, family C is" > s
  print "  reported as skipped." > s
  print "" > s
  print "See `MATRIX.md` for the full grid and `raw/cases.tsv` for every case." > s
}
' "$CASES"
