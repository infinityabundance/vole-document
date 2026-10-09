#!/usr/bin/env python3
# Phase 21.1.3 — XLSX economic court: the VOLE lane's probe driver.
#
# This helper drives the VOLE `observe` / `observe-batch` / `materialize` CLI for
# one fixture and turns the raw answers into the SAME Q1–Q10 envelope the other
# lanes emit, so the court can compare them. It runs the cold probes as **fresh
# processes** (one per observation) and the warm probes in **one resident
# `observe-batch` session**, recording every raw sample.
#
# It is deliberately dumb about VOLE internals: it only uses the shipped CLI
# surface named by the Phase-21.1 contract (`--xlsx-cell`, `--xlsx-tables`,
# `--xlsx-drawing`, `--sheet`, `--kind`, `materialize --exact`) plus `--packed`,
# which the court passes so the VOLE lane measures the established packed seed
# substrate (`field-build --profile runtime --packed`).
#
#   run --bin B --store S --field F --plan JSON --outdir D [--source SRC]

import argparse
import json
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))


def _load_baseline():
    import importlib.util
    path = os.path.join(HERE, "phase21-3-xlsx-baseline.py")
    spec = importlib.util.spec_from_file_location("phase21_3_xlsx_baseline", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


BASE = _load_baseline()


def sha256_hex(b):
    import hashlib
    return hashlib.sha256(b).hexdigest()


def envelope(q, value=None, *, declined=False, code=None, reason="", detail=None, native=True):
    e = {"q": q, "lane": "vole", "declined": bool(declined), "native": native,
         "value": None if declined else value, "detail": detail or {}}
    e["decline"] = {"code": code, "detail": reason} if declined else None
    return e


def probes_for(q, plan):
    """The observe flag lists (one fresh process each) that answer ``q`` cold."""
    sheet = str(plan["sheet"])
    cell = plan.get("cell")
    if q in ("Q1", "Q2", "Q5"):
        return [["--xlsx-cell", cell, "--sheet", sheet, "--kind", "metadata"]]
    if q == "Q4":
        return [["--xlsx-tables", "--sheet", sheet, "--kind", "metadata"]]
    if q == "Q6":
        return [["--sheet", sheet, "--kind", "metadata"]]
    if q == "Q8":
        return [
            ["--xlsx-cell", cell, "--sheet", sheet, "--kind", "metadata"],
            ["--xlsx-cell", cell, "--sheet", sheet, "--kind", "exact"],
        ]
    if q == "Q9":
        return [["--xlsx-drawing", "--sheet", sheet, "--kind", "decoded"]]
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


def _tables_containing(value, cell):
    rc = BASE.a1_to_rowcol(cell)
    names = []
    for t in value.get("tables", []):
        ref = t.get("ref")
        if not ref:
            continue
        try:
            rect = BASE._rect(ref)
        except Exception:
            continue
        if BASE._contains(rect, rc[0], rc[1]):
            names.append(t.get("name"))
    return sorted(names)


def answer(q, plan, answers, rcs):
    """Build the Q envelope from a list of parsed observe answers (or None) and
    their exit codes. All probes must have succeeded, else the Q declines typed."""
    if q in ("Q3", "Q7"):
        code = "no-formula-eval" if q == "Q3" else "no-chart-linkage"
        reason = ("VOLE does not evaluate formulas and exposes no dependency graph"
                  if q == "Q3" else
                  "VOLE exposes the drawing's chart references but not the chart's data "
                  "references, so a chart->table link is not derivable")
        return envelope(q, declined=True, code=code, reason=reason)
    if q == "Q10":
        return None  # handled by the court's exactness step
    for a in answers:
        if a is None:
            return envelope(q, declined=True, code="rc%d" % (rcs[0] if rcs else 6),
                            reason="observe declined")
    if q == "Q1":
        return envelope(q, answers[0]["value"]["value"], detail={"ref": answers[0]["value"]["cell"]})
    if q == "Q2":
        return envelope(q, answers[0]["value"]["formula"], detail={"ref": answers[0]["value"]["cell"]})
    if q == "Q4":
        cell = plan.get("q4cell", plan.get("cell"))
        return envelope(q, _tables_containing(answers[0]["value"], cell), detail={"cell": cell})
    if q == "Q5":
        style = answers[0]["value"].get("style")
        idx = style.get("index") if isinstance(style, dict) else None
        return envelope(q, style, detail={"ref": answers[0]["value"]["cell"], "style_index": idx})
    if q == "Q6":
        return envelope(q, answers[0]["value"]["part"],
                        detail={"sheet": answers[0]["value"]["sheet"], "state": answers[0]["value"]["state"]})
    if q == "Q8":
        meta, exact = answers[0], answers[1]
        return envelope(q, {"cell_xml_sha256": exact["bytes_sha256"], "cell_xml_len": exact["bytes_len"]},
                        detail={"part": meta["value"]["part"], "member_span": meta.get("source_span")})
    if q == "Q9":
        return envelope(q, {"drawing_decoded_sha256": answers[0]["bytes_sha256"],
                            "drawing_decoded_len": answers[0]["bytes_len"]})
    return envelope(q, declined=True, code="unknown-question", reason=q)


def run(bin_path, store, field, plan, outdir, source, packed=False):
    os.makedirs(os.path.join(outdir, "qanswers"), exist_ok=True)
    os.makedirs(os.path.join(outdir, "warm"), exist_ok=True)
    qs = ["Q1", "Q2", "Q3", "Q4", "Q5", "Q6", "Q7", "Q8", "Q9"]
    cold_us = {}
    ok_probes = {}  # q -> list of flag lists that succeeded (for the warm batch)
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

    # Q10 (exact closure) is measured by the court's deletion step, but a
    # length+sha answer is recorded here too (source still present).
    if source:
        tmp = os.path.join(outdir, "q10.materialized")
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
            env = envelope("Q10", {"length": len(data), "sha256": sha256_hex(data)})
            cold_us["Q10"] = dt
        else:
            env = envelope("Q10", declined=True, code="rc%d" % p.returncode, reason="materialize declined")
            cold_us["Q10"] = dt
        with open(os.path.join(outdir, "qanswers", "Q10.json"), "w") as f:
            json.dump(env, f, sort_keys=True)

    with open(os.path.join(outdir, "cold_us.json"), "w") as f:
        json.dump(cold_us, f, sort_keys=True)

    # ---- warm: one observe-batch session over all successful probes ----------
    lines = []      # ordered flag-lines
    layout = []     # (q, probe_index) per line
    for q in qs:
        for i, pr in enumerate(ok_probes.get(q, [])):
            lines.append(" ".join(pr))
            layout.append((q, i))
    requests = os.path.join(outdir, "requests.txt")
    with open(requests, "w") as f:
        f.write("\n".join(lines) + ("\n" if lines else ""))
    warm_us = {q: 0 for q in qs}
    warm_us["Q10"] = 0
    warm_answers = {}
    batch_raw = []
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
        got = {}
        for line in p.stdout.splitlines():
            line = line.strip()
            if not line:
                continue
            try:
                obj = json.loads(line)
            except ValueError:
                continue
            batch_raw.append(obj)
            # match by selector to the ordered layout
        # map by index (one output line per input line, in order)
        outputs = [json.loads(l) for l in p.stdout.splitlines() if l.strip()]
        per_q = {}
        for idx, (q, i) in enumerate(layout):
            if idx >= len(outputs):
                break
            obj = outputs[idx]
            us = obj.get("stats", {}).get("wall_micros", 0)
            warm_us[q] = warm_us.get(q, 0) + us
            per_q.setdefault(q, []).append(obj)
        for q, objs in per_q.items():
            warm_answers[q] = answer(q, plan, objs, [0] * len(objs))
            with open(os.path.join(outdir, "warm", q + ".json"), "w") as f:
                json.dump(warm_answers[q], f, sort_keys=True)
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
        return run(ns.bin, ns.store, ns.field, json.loads(ns.plan), ns.outdir, ns.source, ns.packed)
    return 2


if __name__ == "__main__":
    sys.exit(main())
