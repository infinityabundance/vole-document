#!/usr/bin/env python3
# Phase 21.17 (economic court) — the VOLE lane's probe driver for JSON5 / JSONC.
#
# Drives the shipped VOLE `observe` / `observe-batch` / `materialize` CLI for one
# fixture and turns the raw answers into the SAME Q1–Q12 envelope the two
# conventional baselines emit, so the court can compare them. Cold probes are
# **fresh processes** (one per observation); warm probes run in **one resident
# `observe-batch` session**; every raw sample is retained.
#
# The shipped surface used is the Phase-21.17.1 contract: `--json5-pointer`,
# `--json5-node`, `--json5-find`, `--json5-comments`, the common `--metadata`, and
# `materialize --exact`, plus `--packed` (the court passes it so the VOLE lane
# measures the established packed seed substrate,
# `field-build --profile runtime --packed`).
#
# Q1/Q5 return `(kind, canonical value)`: strings are decoded and numbers are
# canonicalized (`0xFF`->`255`, `.5`->`0.5`, `Infinity`->`inf`) with the same
# vendored loader the baselines use, so a *value* answer is compared on value while
# the exact *spelling* stays a separate question (Q8/Q12).
#
#   run --bin B --store S --field F --plan JSON --outdir D [--source SRC] [--packed]

import argparse
import hashlib
import json
import os
import subprocess
import sys
import time

import json5mini


def sha256_hex(b):
    return hashlib.sha256(b).hexdigest()


def envelope(q, value=None, *, declined=False, code=None, reason="", detail=None):
    e = {"q": q, "lane": "vole", "declined": bool(declined), "native": True,
         "value": None if declined else value, "detail": detail or {}}
    e["decline"] = {"code": code, "detail": reason} if declined else None
    return e


def _ptr(flag, val):
    return flag + "=" + val


def probes_for(q, plan):
    if q == "Q1":
        return [["--json5-pointer=" + plan["value_ptr"], "--kind", "metadata"]]
    if q == "Q2":
        return [["--json5-pointer=" + plan["span_ptr"], "--kind", "metadata"]]
    if q == "Q3":
        return [["--json5-pointer=" + plan["kind_ptr"], "--kind", "metadata"]]
    if q == "Q4":
        return [["--json5-pointer=" + plan["dup_ptr"], "--kind", "metadata"]]
    if q == "Q5":
        return [["--json5-pointer=" + plan["array_ptr"], "--kind", "metadata"]]
    if q == "Q7":
        return [["--json5-find=" + plan["find_pat"], "--kind", "text"]]
    if q == "Q8":
        return [["--json5-pointer=" + plan["token_ptr"], "--kind", "exact"]]
    if q == "Q9":
        return [["--json5-comments", "--kind", "metadata"]]
    if q == "Q10":
        return [["--json5-node=" + plan["keyspan_ptr"], "--kind", "structure"]]
    if q == "Q11":
        return [["--metadata", "--kind", "metadata"]]
    if q == "Q12":
        return [["--json5-pointer=" + plan["num_ptr"], "--kind", "exact"]]
    return []


def _observe(bin_path, store, field, args, packed=False):
    cmd = [bin_path, "observe", "--store", store, "--field", field]
    if packed:
        cmd.append("--packed")
    cmd += args
    t0 = time.monotonic()
    p = subprocess.run(cmd, capture_output=True, text=True)
    dt = int((time.monotonic() - t0) * 1_000_000)
    if p.returncode != 0:
        return None, dt, p.returncode
    try:
        return json.loads(p.stdout), dt, 0
    except ValueError:
        return None, dt, 1


def canon_value(kind, token):
    if kind == "string":
        return json5mini.loads(token)
    if kind == "number":
        return json5mini.canon_number(token)
    if kind in ("true", "false", "null"):
        return kind
    return token


def answer(q, plan, answers, rcs):
    if q == "Q6":
        return None  # handled by the court's exactness step
    if not answers or answers[0] is None:
        return envelope(q, declined=True, code="rc%d" % (rcs[0] if rcs else 6),
                        reason="observe declined")
    a0 = answers[0]
    if q in ("Q1", "Q5"):
        v = a0.get("value") or {}
        kind = v.get("kind")
        token = v.get("token")
        if kind is None or token is None:
            return envelope(q, declined=True, code="no-value", reason="node not found")
        try:
            val = canon_value(kind, token)
        except json5mini.JSON5Error as e:
            return envelope(q, declined=True, code="bad-token", reason=str(e))
        return envelope(q, {"kind": kind, "value": val}, detail={"token": token})
    if q == "Q2":
        span = a0.get("source_span")
        if span is None:
            return envelope(q, declined=True, code="no-span", reason="node has no source span")
        return envelope(q, list(span), detail={})
    if q == "Q3":
        v = a0.get("value") or {}
        kind = v.get("kind")
        if kind is None:
            return envelope(q, declined=True, code="no-kind", reason="no node at path")
        return envelope(q, kind, detail={})
    if q == "Q4":
        v = a0.get("value") or {}
        m = v.get("matches")
        if m is None:
            return envelope(q, declined=True, code="no-values", reason="no node at path")
        return envelope(q, {"exists": bool(m), "duplicate_count": m},
                        detail={"kind": v.get("kind")})
    if q == "Q7":
        v = a0.get("value") or {}
        ms = v.get("matches") or []
        out = [{"pointer": m.get("pointer"), "role": m.get("role"), "text": m.get("text")}
               for m in ms]
        return envelope(q, out, detail={"count": len(out)})
    if q in ("Q8", "Q12"):
        return envelope(q, {"sha256": a0.get("bytes_sha256"), "len": a0.get("bytes_len")},
                        detail={})
    if q == "Q9":
        v = a0.get("value") or {}
        spans = [{"kind": s.get("kind"), "span": s.get("span")}
                 for s in (v.get("spans") or [])]
        return envelope(q, {"comments": v.get("comments"), "spans": spans},
                        detail={"dialect": v.get("dialect")})
    if q == "Q10":
        v = a0.get("value") or {}
        members = [{"key": m.get("key"), "key_kind": m.get("key_kind"),
                    "key_span": m.get("key_span")} for m in (v.get("members") or [])]
        return envelope(q, {"members": members, "duplicate_keys": v.get("duplicate_keys")},
                        detail={})
    if q == "Q11":
        v = a0.get("value") or {}
        return envelope(q, v.get("dialect"), detail={"format": v.get("format")})
    return envelope(q, declined=True, code="unknown-question", reason=q)


def run(bin_path, store, field, plan, outdir, source, packed=False):
    os.makedirs(os.path.join(outdir, "qanswers"), exist_ok=True)
    os.makedirs(os.path.join(outdir, "warm"), exist_ok=True)
    qs = ["Q1", "Q2", "Q3", "Q4", "Q5", "Q7", "Q8", "Q9", "Q10", "Q11", "Q12"]
    cold_us = {}
    ok_probes = {}
    for q in qs:
        probes = probes_for(q, plan)
        answers, rcs, total = [], [], 0
        allok = True
        for pr in probes:
            a, dt, rc = _observe(bin_path, store, field, pr, packed)
            answers.append(a)
            rcs.append(rc)
            total += dt
            if a is None:
                allok = False
        env = answer(q, plan, answers, rcs)
        cold_us[q] = total
        with open(os.path.join(outdir, "qanswers", q + ".json"), "w") as f:
            json.dump(env, f, sort_keys=True)
        if probes and allok:
            ok_probes[q] = probes

    if source:
        tmp = os.path.join(outdir, "q6.materialized")
        t0 = time.monotonic()
        mcmd = [bin_path, "materialize", "--store", store, "--field", field, "--exact"]
        if packed:
            mcmd.append("--packed")
        mcmd += ["--output", tmp]
        p = subprocess.run(mcmd, capture_output=True, text=True)
        dt = int((time.monotonic() - t0) * 1_000_000)
        if p.returncode == 0 and os.path.exists(tmp):
            with open(tmp, "rb") as f:
                data = f.read()
            env = envelope("Q6", {"length": len(data), "sha256": sha256_hex(data)})
            cold_us["Q6"] = dt
        else:
            env = envelope("Q6", declined=True, code="rc%d" % p.returncode,
                           reason="materialize declined")
            cold_us["Q6"] = dt
        with open(os.path.join(outdir, "qanswers", "Q6.json"), "w") as f:
            json.dump(env, f, sort_keys=True)

    with open(os.path.join(outdir, "cold_us.json"), "w") as f:
        json.dump(cold_us, f, sort_keys=True)

    # ---- warm: one observe-batch session over all successful probes ----------
    lines = []
    layout = []
    for q in qs:
        for i, pr in enumerate(ok_probes.get(q, [])):
            lines.append(" ".join(pr))
            layout.append((q, i))
    qs_all = qs + ["Q6"]
    warm_us = {q: 0 for q in qs_all}
    if lines:
        requests = os.path.join(outdir, "requests.txt")
        with open(requests, "w") as f:
            f.write("\n".join(lines) + "\n")
        t0 = time.monotonic()
        bcmd = [bin_path, "observe-batch", "--store", store, "--field", field]
        if packed:
            bcmd.append("--packed")
        bcmd += ["--requests", requests]
        p = subprocess.run(bcmd, capture_output=True, text=True)
        batch_wall = int((time.monotonic() - t0) * 1_000_000)
        with open(os.path.join(outdir, "batch_raw.txt"), "w") as f:
            f.write(p.stdout)
        outputs = []
        for line in p.stdout.splitlines():
            if not line.strip():
                continue
            try:
                outputs.append(json.loads(line))
            except ValueError:
                continue
        per_q = {}
        for idx, (q, i) in enumerate(layout):
            if idx >= len(outputs):
                break
            obj = outputs[idx]
            us = obj.get("stats", {}).get("wall_micros", 0)
            warm_us[q] = warm_us.get(q, 0) + us
            per_q.setdefault(q, []).append(obj)
        for q, objs in per_q.items():
            env = answer(q, plan, objs, [0] * len(objs))
            with open(os.path.join(outdir, "warm", q + ".json"), "w") as f:
                json.dump(env, f, sort_keys=True)
        with open(os.path.join(outdir, "warm_wall_us.json"), "w") as f:
            json.dump({"batch_wall_us": batch_wall}, f, sort_keys=True)
    with open(os.path.join(outdir, "warm_us.json"), "w") as f:
        json.dump(warm_us, f, sort_keys=True)
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run")
    r.add_argument("--bin", required=True)
    r.add_argument("--store", required=True)
    r.add_argument("--field", required=True)
    r.add_argument("--plan", default="{}")
    r.add_argument("--outdir", required=True)
    r.add_argument("--source", default=None)
    r.add_argument("--packed", action="store_true")
    ns = ap.parse_args(argv)
    if ns.cmd == "run":
        return run(ns.bin, ns.store, ns.field, json.loads(ns.plan), ns.outdir,
                   ns.source, ns.packed)
    return 2


if __name__ == "__main__":
    sys.exit(main())
