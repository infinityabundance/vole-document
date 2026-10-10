#!/usr/bin/env python3
# Phase 21.11 — TOML economic court: the VOLE lane's probe driver.
#
# Drives the shipped VOLE `observe` / `observe-batch` / `materialize` CLI for one
# fixture and turns the raw answers into the SAME Q1–Q8 envelope the baselines
# emit, so the court can compare them. Cold probes are **fresh processes**; warm
# probes run in **one resident `observe-batch` session**; every raw sample is
# retained.
#
#   run --bin B --store S --field F --plan JSON --outdir D [--source SRC] [--packed]
#
# The plan keys are: str_path (Q1 value, Q2 span), table (Q3 keys), bool_path (Q4
# value), int_path (Q5 exact spelling), find (Q6 lexical find).

import argparse
import hashlib
import json
import os
import subprocess
import sys
import time


def sha256_hex(b):
    return hashlib.sha256(b).hexdigest()


def envelope(q, value=None, *, declined=False, code=None, reason="", detail=None, native=True):
    e = {"q": q, "lane": "vole", "declined": bool(declined), "native": native,
         "value": None if declined else value, "detail": detail or {}}
    e["decline"] = {"code": code, "detail": reason} if declined else None
    return e


def probes_for(q, plan):
    if q == "Q1":
        return [["--toml-path", plan.get("str_path", ""), "--kind", "text"]]
    if q == "Q2":
        return [["--toml-path", plan.get("str_path", ""), "--kind", "metadata"]]
    if q == "Q3":
        return [["--toml-table", plan.get("table", ""), "--kind", "text"]]
    if q == "Q4":
        return [["--toml-path", plan.get("bool_path", ""), "--kind", "text"]]
    if q == "Q5":
        return [["--toml-path", plan.get("int_path", ""), "--kind", "metadata"]]
    if q == "Q6":
        return [["--toml-find", plan.get("find", ""), "--kind", "metadata"]]
    if q == "Q7":
        return [["--toml-table", "", "--kind", "metadata"]]
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


def answer(q, plan, answers, rcs):
    if not answers:
        return envelope(q, declined=True, code="no-probe",
                        reason="no probe for this plan coordinate")
    for a in answers:
        if a is None:
            return envelope(q, declined=True, code="rc%d" % (rcs[0] if rcs else 6),
                            reason="observe declined")
    a0 = answers[0]
    if q == "Q1":
        return envelope(q, a0.get("text"))
    if q == "Q2":
        v = a0.get("value") or {}
        return envelope(q, v.get("span"), detail={"kind": v.get("kind")})
    if q == "Q3":
        return envelope(q, a0.get("text"))
    if q == "Q4":
        return envelope(q, a0.get("text"))
    if q == "Q5":
        v = a0.get("value") or {}
        return envelope(q, v.get("spelling"), detail={"kind": v.get("kind")})
    if q == "Q6":
        v = a0.get("value") or {}
        return envelope(q, len(v.get("matches") or []))
    if q == "Q7":
        v = a0.get("value") or {}
        return envelope(q, len(v.get("keys") or []))
    return envelope(q, declined=True, code="unknown-question", reason=q)


def run(bin_path, store, field, plan, outdir, source, packed=False):
    os.makedirs(os.path.join(outdir, "qanswers"), exist_ok=True)
    os.makedirs(os.path.join(outdir, "warm"), exist_ok=True)
    qs = ["Q1", "Q2", "Q3", "Q4", "Q5", "Q6", "Q7"]
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
        tmp = os.path.join(outdir, "q8.materialized")
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
            env = envelope("Q8", {"length": len(data), "sha256": sha256_hex(data)})
        else:
            env = envelope("Q8", declined=True, code="rc%d" % p.returncode,
                           reason="materialize declined")
        cold_us["Q8"] = dt
        with open(os.path.join(outdir, "qanswers", "Q8.json"), "w") as f:
            json.dump(env, f, sort_keys=True)

    with open(os.path.join(outdir, "cold_us.json"), "w") as f:
        json.dump(cold_us, f, sort_keys=True)

    lines = []
    layout = []
    for q in qs:
        for i, pr in enumerate(ok_probes.get(q, [])):
            lines.append(" ".join(pr))
            layout.append((q, i))
    requests = os.path.join(outdir, "requests.txt")
    with open(requests, "w") as f:
        f.write("\n".join(lines) + ("\n" if lines else ""))
    warm_us = {q: 0 for q in qs}
    warm_us["Q8"] = 0
    if lines:
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
        with open(os.path.join(outdir, "warm_us.json"), "w") as f:
            json.dump(warm_us, f, sort_keys=True)
        with open(os.path.join(outdir, "warm_wall_us.json"), "w") as f:
            json.dump({"batch_wall_us": batch_wall}, f, sort_keys=True)
    else:
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
        return run(ns.bin, ns.store, ns.field, json.loads(ns.plan), ns.outdir, ns.source,
                   ns.packed)
    return 2


if __name__ == "__main__":
    sys.exit(main())
