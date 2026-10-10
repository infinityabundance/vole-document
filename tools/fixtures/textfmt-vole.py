#!/usr/bin/env python3
# Phase 21.20-21.24 (economic courts) — the VOLE lane's probe driver for the five
# text-family formats (config / feed / geojson / gis / notebook). Drives the shipped
# VOLE `observe` / `observe-batch` / `materialize` CLI for one fixture and turns the
# raw answers into the SAME Q1–Q12 envelope the conventional baselines emit, so the
# court can compare them. Cold probes are **fresh processes** (one per observation);
# warm probes run in **one resident `observe-batch` session**; every raw sample is
# retained.
#
#   run --format F --bin B --store S --field F --plan JSON --outdir D [--source SRC] [--packed]

import argparse
import hashlib
import json
import os
import subprocess
import sys
import time


def sha256_hex(b):
    return hashlib.sha256(b).hexdigest()


def envelope(q, value=None, *, declined=False, code=None, reason="", detail=None):
    e = {"q": q, "lane": "vole", "declined": bool(declined), "native": True,
         "value": None if declined else value, "detail": detail or {}}
    e["decline"] = {"code": code, "detail": reason} if declined else None
    return e


def clamp_canon_num(v):
    import math
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, float):
        if math.isnan(v):
            return "nan"
        if v.is_integer():
            return str(int(v))
        return repr(v)
    return str(v)


# --- probes (per format, per question) --------------------------------------

def probes_for(fmt, q, plan):
    if fmt == "config":
        if q == "Q1":
            return [(["--config-entry", str(plan["value_entry"])], "structure")]
        if q == "Q2":
            return [(["--config-entry", str(plan["span_entry"])], "structure")]
        if q == "Q3":
            return [(["--config-line", str(plan["kind_line"])], "structure")]
        if q == "Q4":
            return [(["--config-entry", str(plan["dup_entry"])], "structure")]
        if q == "Q5":
            if plan.get("section") is None:
                return []
            return [(["--config-section", plan["section"]], "structure")]
        if q == "Q7":
            return [(["--config-find", plan["find_pat"]], "text")]
        if q == "Q8":
            return [(["--config-entry", str(plan["token_entry"])], "exact")]
        if q == "Q9":
            return [(["--config-line", str(plan["comment_line"])], "structure")]
        if q == "Q10":
            return [(["--config-entry", str(plan["value_entry"])], "structure")]
        if q == "Q11":
            return [(["--metadata"], "metadata")]
        if q == "Q12":
            return [(["--config-entry", str(plan["value_entry"])], "structure")]
    if fmt == "feed":
        e = str(plan["entry"])
        f = plan["field"]
        cf = plan["cfield"]
        if q in ("Q1", "Q2", "Q3"):
            return [(["--feed-entry-field", e + ":" + f], "structure")]
        if q == "Q4":
            return [(["--feed-entry", e], "structure")]
        if q == "Q5":
            return [(["--feed-entry", e], "structure")]
        if q == "Q7":
            return [(["--feed-find", plan["find_pat"]], "text")]
        if q == "Q8":
            return [(["--feed-field", cf], "exact")]
        if q == "Q9":
            return [(["--feed-field", cf], "structure")]
        if q == "Q10":
            return [(["--feed-channel"], "structure")]
        if q == "Q11":
            return [(["--metadata"], "metadata")]
        if q == "Q12":
            return [(["--feed-entry", e], "structure")]
    if fmt == "geojson":
        f = str(plan["feature"])
        g = str(plan["geometry"])
        p = plan["prop"]
        if q in ("Q1", "Q2", "Q8"):
            rep = "exact" if q == "Q8" else "structure"
            return [(["--geojson-property", f + ":" + p], rep)]
        if q == "Q3":
            return [(["--geojson-geometry", g], "structure")]
        if q in ("Q4", "Q5", "Q10"):
            return [(["--geojson-feature", f], "structure")]
        if q == "Q7":
            return [(["--geojson-find", plan["find_pat"]], "text")]
        if q == "Q9":
            return [(["--geojson-coordinates", g], "structure")]
        if q == "Q11":
            return [(["--metadata"], "metadata")]
        if q == "Q12":
            return [(["--geojson-coordinates", g], "exact")]
    if fmt == "gis":
        r = str(plan["record"])
        rf = plan["record_field"]
        p = str(plan["point"])
        if q in ("Q1", "Q2"):
            return [(["--gis-record-field", r + ":" + rf], "structure")]
        if q == "Q3":
            return [(["--gis-point", p], "structure")]
        if q == "Q4":
            return [(["--gis-record", r], "structure")]
        if q == "Q5":
            return [(["--gis-record", r], "structure")]
        if q == "Q7":
            return [(["--gis-find", plan["find_pat"]], "text")]
        if q == "Q8":
            return [(["--gis-point", p], "exact")]
        if q in ("Q9", "Q10", "Q12"):
            return [(["--gis-point", p], "structure")]
        if q == "Q11":
            return [(["--metadata"], "metadata")]
    if fmt == "notebook":
        c = str(plan["cell"])
        if q == "Q1":
            return [(["--notebook-cell-type", c], "text")]
        if q == "Q2":
            return [(["--notebook-cell", c], "structure")]
        if q in ("Q3", "Q8", "Q10"):
            rep = "exact" if q == "Q8" else "structure"
            return [(["--notebook-cell-source", c], rep)]
        if q == "Q4":
            return [(["--notebook-nbformat"], "metadata")]
        if q == "Q5":
            return [(["--notebook-cell", c], "structure")]
        if q == "Q7":
            return [(["--notebook-find", plan["find_pat"]], "text")]
        if q == "Q9":
            return [(["--notebook-cell-output",
                      str(plan["out_cell"]) + ":" + str(plan["out_idx"])], "structure")]
        if q == "Q11":
            return [(["--metadata"], "metadata")]
        if q == "Q12":
            return [(["--metadata"], "metadata")]
    return []


# --- extraction (parsed observe JSON -> envelope) ---------------------------

def _ans(answers, rcs):
    if not answers or answers[0] is None:
        return None, envelope("?", declined=True, code="rc%d" % (rcs[0] if rcs else 6),
                              reason="observe declined")
    return answers[0], None


def answer(fmt, q, plan, answers, rcs):
    a, d = _ans(answers, rcs)
    if d is not None:
        d["q"] = q
        return d
    v = a.get("value") or {}
    try:
        if fmt == "config":
            if q == "Q1":
                return envelope(q, {"kind": "string", "value": v.get("value")})
            if q == "Q2":
                return envelope(q, a.get("source_span"))
            if q == "Q3":
                return envelope(q, v.get("kind"))
            if q == "Q4":
                n = v.get("same_key_entries")
                return envelope(q, {"exists": bool(n), "duplicate_count": n})
            if q == "Q5":
                return envelope(q, {"name": v.get("name"), "matches": v.get("matches")})
            if q == "Q7":
                ms = v.get("matches") or []
                out = sorted([[m.get("role"), m.get("text")] for m in ms])
                return envelope(q, out)
            if q == "Q8":
                return envelope(q, {"sha256": a.get("bytes_sha256"),
                                    "len": a.get("bytes_len")})
            if q == "Q9":
                m = v.get("marker")
                return envelope(q, {"kind": v.get("kind"),
                                    "marker": chr(m) if isinstance(m, int) else m})
            if q == "Q10":
                return envelope(q, {"key_span": v.get("key_span"),
                                    "sep_span": v.get("sep_span"),
                                    "marker": chr(v["marker"])})
            if q == "Q11":
                return envelope(q, v.get("dialect"))
            if q == "Q12":
                return envelope(q, {"export": v.get("export"),
                                    "single_quoted": v.get("single_quoted"),
                                    "double_quoted": v.get("double_quoted"),
                                    "continued": v.get("continued"),
                                    "inline_comment": v.get("inline_comment")})
        if fmt == "feed":
            if q == "Q1":
                return envelope(q, {"kind": "string", "value": v.get("text")})
            if q == "Q2":
                return envelope(q, a.get("source_span"))
            if q == "Q3":
                return envelope(q, v.get("name"))
            if q == "Q4":
                fields = v.get("fields") or []
                n = sum(1 for f in fields if f.get("name") == plan["dup_field"])
                return envelope(q, {"exists": n > 0, "duplicate_count": n})
            if q == "Q5":
                return envelope(q, {"index": v.get("index"),
                                    "fields": [f.get("name") for f in (v.get("fields") or [])]})
            if q == "Q7":
                ms = v.get("matches") or []
                out = sorted([[("*" if m.get("entry") is None else str(m.get("entry"))),
                               m.get("name"), m.get("text")] for m in ms])
                return envelope(q, out)
            if q == "Q8":
                return envelope(q, {"sha256": a.get("bytes_sha256"), "len": a.get("bytes_len")})
            if q == "Q9":
                attrs = v.get("attrs") or []
                return envelope(q, [[x.get("name"), x.get("value"), x.get("span")]
                                    for x in attrs])
            if q == "Q10":
                return envelope(q, [f.get("name") for f in (v.get("fields") or [])])
            if q == "Q11":
                return envelope(q, v.get("dialect"))
            if q == "Q12":
                return envelope(q, [f.get("span") for f in (v.get("fields") or [])])
        if fmt == "geojson":
            if q == "Q1":
                kind = v.get("kind")
                tok = v.get("token")
                val = _decode_token(kind, tok)
                return envelope(q, {"kind": _kind_name(kind), "value": val})
            if q == "Q2":
                return envelope(q, a.get("source_span"))
            if q == "Q3":
                return envelope(q, v.get("type"))
            if q == "Q4":
                feats = v.get("properties") or []
                n = sum(1 for x in feats if x == plan["key"])
                return envelope(q, {"exists": n > 0, "duplicate_count": n})
            if q == "Q5":
                return envelope(q, {"type": v.get("type"),
                                    "foreign": v.get("foreign")})
            if q == "Q7":
                ms = v.get("matches") or []
                return envelope(q, sorted([[m.get("role"), m.get("text")] for m in ms]))
            if q == "Q8":
                return envelope(q, {"sha256": a.get("bytes_sha256"), "len": a.get("bytes_len")})
            if q == "Q9":
                nums = v.get("numbers") or []
                return envelope(q, [n.get("spelling") for n in nums])
            if q == "Q10":
                return envelope(q, v.get("foreign"))
            if q == "Q11":
                return envelope(q, v.get("type"))
            if q == "Q12":
                return envelope(q, {"sha256": a.get("bytes_sha256"), "len": a.get("bytes_len")})
        if fmt == "gis":
            if q == "Q1":
                return envelope(q, {"kind": "string", "value": v.get("text")})
            if q == "Q2":
                return envelope(q, a.get("source_span"))
            if q == "Q3":
                return envelope(q, v.get("kind"))
            if q == "Q4":
                fields = v.get("fields") or []
                n = sum(1 for f in fields if f.get("name") == plan["field"])
                return envelope(q, {"exists": n > 0, "duplicate_count": n})
            if q == "Q5":
                return envelope(q, {"index": v.get("index"), "kind": v.get("kind"),
                                    "fields": [f.get("name") for f in (v.get("fields") or [])]})
            if q == "Q7":
                ms = v.get("matches") or []
                return envelope(q, sorted([[m.get("kind"), str(m.get("index")),
                                             m.get("name"), m.get("text")] for m in ms]))
            if q == "Q8":
                return envelope(q, {"sha256": a.get("bytes_sha256"), "len": a.get("bytes_len")})
            if q == "Q9":
                attrs = v.get("attrs") or []
                return envelope(q, [[x.get("name"), x.get("value"), x.get("span")]
                                    for x in attrs])
            if q == "Q10":
                return envelope(q, [f.get("name") for f in (v.get("fields") or [])])
            if q == "Q11":
                return envelope(q, v.get("dialect"))
            if q == "Q12":
                return envelope(q, [[f.get("name"), f.get("text")]
                                    for f in (v.get("fields") or [])])
        if fmt == "notebook":
            if q == "Q1":
                return envelope(q, {"kind": "string", "value": a.get("text")})
            if q == "Q2":
                return envelope(q, a.get("source_span"))
            if q == "Q3":
                return envelope(q, v.get("form"))
            if q == "Q4":
                return envelope(q, {"cells": v.get("cells"), "outputs": v.get("outputs")})
            if q == "Q5":
                return envelope(q, {"index": v.get("index"), "cell_type": v.get("cell_type"),
                                    "source_form": v.get("source_form"),
                                    "source_elements": v.get("source_elements"),
                                    "outputs": v.get("outputs")})
            if q == "Q7":
                ms = v.get("matches") or []
                return envelope(q, sorted([[m.get("role"), m.get("text")] for m in ms]))
            if q == "Q8":
                return envelope(q, {"sha256": a.get("bytes_sha256"), "len": a.get("bytes_len")})
            if q == "Q9":
                return envelope(q, {"output_type": v.get("output_type"),
                                    "name": v.get("name"), "text_form": v.get("text_form")})
            if q == "Q10":
                return envelope(q, {"elements": v.get("elements")})
            if q == "Q11":
                return envelope(q, v.get("nbformat"))
            if q == "Q12":
                return envelope(q, v.get("nbformat_minor"))
    except (KeyError, TypeError, ValueError) as e:
        return envelope(q, declined=True, code="extract-error", reason=str(e))
    return envelope(q, declined=True, code="unknown-question", reason=q)


def _kind_name(kind):
    return kind


def _decode_token(kind, tok):
    if kind == "string":
        return json.loads(tok)
    if kind == "number":
        return clamp_canon_num(json.loads(tok))
    if kind in ("true", "false", "null"):
        return kind
    return tok


# --- driver -----------------------------------------------------------------

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


def run(fmt, bin_path, store, field, plan, outdir, source, packed=False):
    os.makedirs(os.path.join(outdir, "qanswers"), exist_ok=True)
    os.makedirs(os.path.join(outdir, "warm"), exist_ok=True)
    qs = ["Q1", "Q2", "Q3", "Q4", "Q5", "Q7", "Q8", "Q9", "Q10", "Q11", "Q12"]
    cold_us = {}
    ok_probes = {}
    for q in qs:
        probes = probes_for(fmt, q, plan)
        answers, rcs, total, allok = [], [], 0, True
        for pr in probes:
            a, dt, rc = _observe(bin_path, store, field, _kind_args(pr), packed)
            answers.append(a)
            rcs.append(rc)
            total += dt
            if a is None:
                allok = False
        env = answer(fmt, q, plan, answers, rcs)
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
            lines.append(" ".join(_kind_args(pr)))
            layout.append((q, i))
    warm_us = {q: 0 for q in qs + ["Q6"]}
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
            env = answer(fmt, q, plan, objs, [0] * len(objs))
            with open(os.path.join(outdir, "warm", q + ".json"), "w") as f:
                json.dump(env, f, sort_keys=True)
        with open(os.path.join(outdir, "warm_wall_us.json"), "w") as f:
            json.dump({"batch_wall_us": batch_wall}, f, sort_keys=True)
    with open(os.path.join(outdir, "warm_us.json"), "w") as f:
        json.dump(warm_us, f, sort_keys=True)
    return 0


def _kind_args(probe):
    args, rep = probe
    return list(args) + ["--kind", rep]


def main(argv=None):
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run")
    r.add_argument("--format", required=True)
    r.add_argument("--bin", required=True)
    r.add_argument("--store", required=True)
    r.add_argument("--field", required=True)
    r.add_argument("--plan", default="{}")
    r.add_argument("--outdir", required=True)
    r.add_argument("--source", default=None)
    r.add_argument("--packed", action="store_true")
    ns = ap.parse_args(argv)
    if ns.cmd == "run":
        return run(ns.format, ns.bin, ns.store, ns.field, json.loads(ns.plan),
                   ns.outdir, ns.source, ns.packed)
    return 2


if __name__ == "__main__":
    sys.exit(main())
