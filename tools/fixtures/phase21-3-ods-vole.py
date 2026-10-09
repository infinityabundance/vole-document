#!/usr/bin/env python3
# Phase 21.3.2 — ODS economic court: the VOLE lane's probe driver.
#
# This helper drives the VOLE `observe` / `observe-batch` / `materialize` CLI for
# one fixture and turns the raw answers into the SAME Q1–Q10 envelope the other
# lanes emit, so the court can compare them. It runs the cold probes as **fresh
# processes** (one per observation) and the warm probes in **one resident
# `observe-batch` session**, recording every raw sample.
#
# It is deliberately dumb about VOLE internals: it only uses the shipped CLI
# surface named by the Phase-21.3 contract (`--ods-sheet`, `--ods-cell`,
# `--ods-styles`, `--ods-named-expressions`, `--ods-comments`, `--ods-find`, the
# common `--metadata`/`--doc-text`/`--table`/`--cell`/`--resource`, and
# `materialize --exact`) plus `--packed`, which the court passes so the VOLE lane
# measures the established packed seed substrate (`field-build --profile runtime
# --packed`).
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
    path = os.path.join(HERE, "phase21-3-ods-baseline.py")
    spec = importlib.util.spec_from_file_location("phase21_3_ods_baseline", path)
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
    if q in ("Q1", "Q2"):
        return [["--ods-cell", cell, "--ods-sheet", sheet, "--kind", "metadata"]]
    if q == "Q4":
        return [
            ["--ods-sheet", sheet, "--kind", "metadata"],
            ["--ods-named-expressions", "--kind", "metadata"],
        ]
    if q == "Q5":
        return [
            ["--ods-cell", cell, "--ods-sheet", sheet, "--kind", "metadata"],
            ["--ods-styles", "--kind", "metadata"],
        ]
    if q == "Q6":
        return [["--ods-sheet", sheet, "--kind", "metadata"]]
    if q == "Q7":
        return [
            ["--ods-sheet", sheet, "--kind", "metadata"],
            ["--ods-named-expressions", "--kind", "metadata"],
        ]
    if q == "Q8":
        return [
            ["--ods-cell", cell, "--ods-sheet", sheet, "--kind", "metadata"],
            ["--ods-cell", cell, "--ods-sheet", sheet, "--kind", "exact"],
        ]
    if q == "Q9":
        return [["--resource", "0", "--kind", "metadata"]]
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


def surface_probes(plan):
    """The full shipped ODS/common selector surface, exercised once per fixture
    (not all of it maps onto Q1–Q10; this records that the surface is driven)."""
    sheet = str(plan["sheet"])
    cell = plan.get("cell")
    find = plan.get("find", "a")
    return [
        ("metadata", ["--metadata", "--kind", "metadata"]),
        ("text", ["--doc-text", "--kind", "text"]),
        ("table", ["--table", "0", "--kind", "metadata"]),
        ("cell", ["--cell", "0:0:0", "--kind", "metadata"]),
        ("ods-sheet", ["--ods-sheet", sheet, "--kind", "metadata"]),
        ("ods-cell", ["--ods-cell", cell, "--ods-sheet", sheet, "--kind", "metadata"]),
        ("ods-cell-exact", ["--ods-cell", cell, "--ods-sheet", sheet, "--kind", "exact"]),
        ("ods-styles", ["--ods-styles", "--kind", "metadata"]),
        ("ods-named-expressions", ["--ods-named-expressions", "--kind", "metadata"]),
        ("ods-comments", ["--ods-comments", "--ods-sheet", sheet, "--kind", "metadata"]),
        ("ods-find", ["--ods-find", find, "--kind", "text"]),
        ("resource", ["--resource", "0", "--kind", "metadata"]),
    ]


def _digest(answer):
    if answer is None:
        return None
    if "bytes_sha256" in answer:
        return answer["bytes_sha256"]
    val = answer.get("value")
    if val is None and "text" in answer:
        val = answer["text"]
    return sha256_hex(json.dumps(val, sort_keys=True).encode())


def _resolve_style(styles_answer, style_name):
    if not style_name or styles_answer is None:
        return None
    val = styles_answer.get("value") or {}
    for scope in ("automatic_styles", "named_styles"):
        for st in val.get(scope, []):
            if st.get("name") == style_name:
                return st
    return None


def _named_containing(named_answer, sheet_name, cell):
    rc = BASE.parse_cell_position(cell) if cell else None
    if rc is None or named_answer is None:
        return None
    col, row = rc
    items = named_answer.get("value") or []
    names = sorted(
        n["name"] for n in items
        if n.get("kind") == "range" and BASE._range_contains(n.get("cell_range_address"), sheet_name, row, col)
    )
    return names


def _named_referencing(named_answer, sheet_name):
    if named_answer is None:
        return None
    items = named_answer.get("value") or []
    names = sorted(
        n["name"] for n in items
        if sheet_name in BASE._address_sheets(n.get("base_cell_address"))
        or sheet_name in BASE._address_sheets(n.get("cell_range_address"))
    )
    return names


def answer(q, plan, answers, rcs):
    """Build the Q envelope from a list of parsed observe answers (or None) and
    their exit codes. All probes must have succeeded, else the Q declines typed."""
    if q == "Q3":
        return envelope(q, declined=True, code="no-formula-eval",
                        reason="VOLE does not evaluate formulas and exposes no dependency graph")
    if q == "Q9":
        return envelope(q, declined=True, code="no-resource-selector",
                        reason="VOLE exposes no ODS-native resource/media selector (the common "
                               "`--resource` selector is not mapped for ODS)")
    if q == "Q10":
        return None  # handled by the court's exactness step
    for a in answers:
        if a is None:
            return envelope(q, declined=True, code="rc%d" % (rcs[0] if rcs else 6),
                            reason="observe declined")
    if q == "Q1":
        v = answers[0]["value"]
        return envelope(q, {"type": v.get("value_type"), "value": BASE._typed_value(v)},
                        detail={"ref": v.get("cell")})
    if q == "Q2":
        return envelope(q, answers[0]["value"]["formula"], detail={"ref": answers[0]["value"]["cell"]})
    if q == "Q4":
        name = answers[0]["value"].get("sheet")
        cell = plan.get("q4cell", plan.get("cell"))
        return envelope(q, _named_containing(answers[1], name, cell), detail={"cell": cell})
    if q == "Q5":
        v = answers[0]["value"]
        style_name = v.get("style")
        return envelope(q, _resolve_style(answers[1], style_name),
                        detail={"ref": v.get("cell"), "style_name": style_name})
    if q == "Q6":
        return envelope(q, answers[0]["value"]["part"],
                        detail={"sheet": answers[0]["value"]["sheet"]})
    if q == "Q7":
        name = answers[0]["value"].get("sheet")
        return envelope(q, _named_referencing(answers[1], name), detail={"sheet": name})
    if q == "Q8":
        meta, exact = answers[0], answers[1]
        return envelope(q, {"cell_xml_sha256": exact["bytes_sha256"], "cell_xml_len": exact["bytes_len"]},
                        detail={"part": meta["value"]["part"], "member_span": meta.get("source_span"),
                                "span_start": meta["value"].get("span_start"),
                                "span_len": meta["value"].get("span_len")})
    return envelope(q, declined=True, code="unknown-question", reason=q)


def run(bin_path, store, field, plan, outdir, source, packed=False):
    os.makedirs(os.path.join(outdir, "qanswers"), exist_ok=True)
    os.makedirs(os.path.join(outdir, "warm"), exist_ok=True)
    qs = ["Q1", "Q2", "Q3", "Q4", "Q5", "Q6", "Q7", "Q8", "Q9"]
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

    # ---- shipped surface (extra) ----------------------------------------------
    surface = {}
    for name, flags in surface_probes(plan):
        a, _dt, rc = _observe(bin_path, store, field, flags, packed)
        surface[name] = {"flags": " ".join(flags), "rc": rc, "ok": a is not None, "digest": _digest(a)}
    with open(os.path.join(outdir, "surface.json"), "w") as f:
        json.dump(surface, f, sort_keys=True)

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
    warm_us["Q10"] = 0
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
            line = line.strip()
            if not line:
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
        return run(ns.bin, ns.store, ns.field, json.loads(ns.plan), ns.outdir, ns.source, ns.packed)
    return 2


if __name__ == "__main__":
    sys.exit(main())
