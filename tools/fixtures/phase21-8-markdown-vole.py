#!/usr/bin/env python3
# Phase 21.8 — Markdown economic court: the VOLE lane's probe driver.
#
# Drives the shipped VOLE `observe` / `observe-batch` / `materialize` CLI for one
# fixture and turns the raw answers into the SAME Q1–Q8 envelope the baselines
# emit, so the court can compare them. Cold probes are **fresh processes**; warm
# probes run in **one resident `observe-batch` session**; every raw sample is
# retained.
#
#   run --bin B --store S --field F --plan JSON --outdir D [--source SRC] [--packed]

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
    h = plan.get("heading", 0)
    if q == "Q1":
        return [["--md-heading", str(h), "--kind", "text"]]
    if q == "Q2":
        return [["--md-block", str(plan.get("block", 0)), "--kind", "metadata"]]
    if q == "Q3":
        if "code" not in plan:
            return []
        return [["--md-code", str(plan["code"]), "--kind", "metadata"]]
    if q == "Q4":
        if "link" not in plan:
            return []
        return [["--md-link", str(plan["link"]), "--kind", "metadata"]]
    if q == "Q6":
        return [["--md-find", plan.get("pattern", ""), "--kind", "metadata"]]
    if q == "Q7":
        return [["--metadata", "--kind", "metadata"]]
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
        return envelope(q, a0.get("text"), detail={"selector": a0.get("selector")})
    if q == "Q2":
        v = a0.get("value") or {}
        return envelope(q, v.get("span"), detail={"kind": v.get("kind")})
    if q == "Q3":
        v = a0.get("value") or {}
        return envelope(q, {"language": v.get("language"), "content_span": v.get("content_span"),
                            "bytes": v.get("bytes")}, detail={"kind": v.get("kind")})
    if q == "Q4":
        v = a0.get("value") or {}
        return envelope(q, v.get("target"), detail={"kind": v.get("kind")})
    if q == "Q6":
        v = a0.get("value") or {}
        return envelope(q, len(v.get("matches") or []))
    if q == "Q7":
        v = a0.get("value") or {}
        return envelope(q, v.get("headings"))
    return envelope(q, declined=True, code="unknown-question", reason=q)


def run(bin_path, store, field, plan, outdir, source, packed=False):
    os.makedirs(os.path.join(outdir, "qanswers"), exist_ok=True)
    os.makedirs(os.path.join(outdir, "warm"), exist_ok=True)
    qs = ["Q1", "Q2", "Q3", "Q4", "Q5", "Q6", "Q7"]
    cold_us = {}
    ok_probes = {}
    for q in qs:
        if q == "Q5":
            # The first list item's text: locate the block by a unique marker token
            # (a lexical find), then read that block's content.
            token = plan.get("item", "")
            answers, rcs, total = [], [], 0
            allok = True
            if token:
                a, dt, rc = _observe(bin_path, store, field,
                                     ["--md-find", token, "--kind", "metadata"], packed)
                answers.append(a)
                rcs.append(rc)
                total += dt
                if a is None:
                    allok = False
                else:
                    matches = (a.get("value") or {}).get("matches") or []
                    if not matches:
                        allok = False
                        answers.append(None)
                    else:
                        bi = matches[0].get("block")
                        a2, dt2, rc2 = _observe(bin_path, store, field,
                                                ["--md-block", str(bi), "--kind", "text"], packed)
                        answers.append(a2)
                        rcs.append(rc2)
                        total += dt2
                        if a2 is None:
                            allok = False
            else:
                allok = False
                answers.append(None)
            if allok and answers and answers[-1] is not None:
                env = envelope(q, answers[-1].get("text"), detail={"block": "found"})
            else:
                env = envelope(q, declined=True, code="no-list-item",
                               reason="no list item located")
            cold_us[q] = total
            with open(os.path.join(outdir, "qanswers", q + ".json"), "w") as f:
                json.dump(env, f, sort_keys=True)
            continue
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
