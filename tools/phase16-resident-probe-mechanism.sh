#!/usr/bin/env bash
# Phase 16 item 4 support: capture the mechanism behind the resident verdict.
# Runs in the pinned `doc-baseline` service; writes into the sealed campaign.
set -uo pipefail
cd /work

OUT=evidence/campaigns/2026-10-08-phase16-resident-probe-5d331f2/raw
B=target/release/vole-document

{
  echo "# Isolating the session's one-time open from per-observation cost"
  echo "# doc: nasa-pdf-0004 (50-100MiB pdf), descriptor $(stat -c%s evidence/scratch/real100-resident-probe-work/nasa-pdf-0004/v.voldoc) bytes"
  echo "# cold observe --page 1 --kind text (narrow_probe, never opens the descriptor)"
  echo "# resident observe-batch (one session open = full Field::open, then probe hits)"
  D=evidence/scratch/real100-resident-probe-work/nasa-pdf-0004
  F=$(cat "$D/field")
  for r in 1 5; do
    for i in 1 2 3; do
      /usr/bin/time -f "observe-batch repeat=$r: %e s" \
        "$B" observe-batch --store "$D/vstore" --field "$F" \
        --requests "$D/vr.repeat.reqs" --repeat "$r" >/dev/null
    done
  done
  for i in 1 2 3; do
    /usr/bin/time -f "observe (cold, single): %e s" \
      "$B" observe --page 1 --kind text --store "$D/vstore" --field "$F" >/dev/null
  done
} >"$OUT/mechanism.txt" 2>&1

python3 - "$OUT" <<'PY' >"$OUT/answer-equality.txt" 2>&1
import json, os, sys
out = sys.argv[1]
work = 'evidence/scratch/real100-resident-probe-work'
eq = neq = missing = 0
hits = tot = 0
for d in sorted(os.listdir(work)):
    vf = os.path.join(work, d, 'v.text_once.json')
    rf = os.path.join(work, d, 'vr.repeat.jsonl')
    if not (os.path.exists(vf) and os.path.exists(rf)):
        missing += 1
        continue
    try:
        v = json.load(open(vf))
    except Exception:
        missing += 1
        continue
    lines = [json.loads(l) for l in open(rf) if l.strip()]
    if not lines:
        missing += 1
        continue
    r = lines[-1]
    if 'error' in v or 'error' in r:
        missing += 1
        continue
    key = lambda o: {k: x for k, x in o.items() if k != 'stats'}
    if key(v) == key(r):
        eq += 1
    else:
        neq += 1
    for ln in lines[1:]:
        tot += 1
        if ln['stats']['descriptor_bytes_read'] == 0 and ln['stats']['descriptor_read_mode'] == 'partial':
            hits += 1
print("Answer equality: cold `observe --page 1 --kind text` == resident `observe-batch` last line")
print(f"  equal: {eq}   mismatch: {neq}   skipped (decline / missing): {missing}")
print("Probe short-circuit in the resident session (observations 2..N of each batch):")
print(f"  cache-served hits (descriptor_bytes_read==0, mode==partial): {hits}/{tot}")
PY

echo "--- rustc/cargo/uname/Cargo.lock/git ---"
rustc --version
cargo --version
uname -m
sha256sum Cargo.lock
git rev-parse HEAD
git --no-pager status --porcelain | head
echo "--- done ---"
