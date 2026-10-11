#!/usr/bin/env python3
# Phase 21.18 / 21.19 (economic courts) — the VOLE lane's probe driver for the
# binary structured-tree formats (CBOR, MessagePack). Shared via `--format`.
#
# Drives the shipped VOLE `observe` / `observe-batch` / `materialize` CLI for one
# fixture and turns the raw answers into the SAME Q1–Q12 envelope the two
# conventional baselines emit, so the court can compare them. Cold probes are
# **fresh processes** (one per observation); warm probes run in **one resident
# `observe-batch` session**; every raw sample is retained.
#
# The shipped surface used is the Phase-21.18/21.19 contract: `--cbor-pointer` /
# `--msgpack-pointer`, `--<fmt>-find`, the common `--metadata`, and `materialize
# --exact`, plus `--packed` (the court passes it so the VOLE lane measures the
# established packed seed substrate, `field-build --profile runtime --packed`).
#
# Q1/Q5 return `(class, canonical value)`: the node's exact token bytes (from
# `--kind exact`) are decoded with the SAME vendored decoder the baselines use, so a
# *value* answer is compared on value while the exact token bytes stay Q8, the
# encoding width Q9, the byte-vs-text kind Q10, the tag/ext type Q11, and the float
# width Q12.
#
#   run --format cbor|msgpack --bin B --store S --field F --plan JSON --outdir D
#       [--source SRC] [--packed]

import argparse
import hashlib
import json
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)

import cbor_min
import msgpack_min

FMT = {
    "cbor": {"mod": cbor_min, "pointer_flag": "cbor-pointer", "find_flag": "cbor-find",
             "width_field": "info", "type_field": "tag"},
    "msgpack": {"mod": msgpack_min, "pointer_flag": "msgpack-pointer",
                "find_flag": "msgpack-find", "width_field": "head", "type_field": "ext_type"},
}

KIND_CLASS = {
    "cbor": {"uint": "number", "negint": "number", "float": "number",
             "simple": "number", "text": "string", "bytes": "string",
             "array": "array", "map": "object", "true": "bool", "false": "bool",
             "null": "null", "undefined": "null", "tag": "tag"},
    "msgpack": {"uint": "number", "negint": "number", "float": "number",
                "str": "string", "bin": "string", "array": "array", "map": "object",
                "true": "bool", "false": "bool", "nil": "null", "ext": "string"},
}


def sha256_hex(b):
    return hashlib.sha256(b).hexdigest()


def envelope(q, value=None, *, declined=False, code=None, reason="", detail=None):
    e = {"q": q, "lane": "vole", "declined": bool(declined), "native": True,
         "value": None if declined else value, "detail": detail or {}}
    e["decline"] = {"code": code, "detail": reason} if declined else None
    return e


def ptr_arg(fmt, ptr):
    return "--%s=%s" % (FMT[fmt]["pointer_flag"], ptr)


def probes_for(fmt, q, plan):
    if q == "Q1":
        return [[ptr_arg(fmt, plan["value_ptr"]), "--kind", "exact"]]
    if q == "Q2":
        return [[ptr_arg(fmt, plan["span_ptr"]), "--kind", "metadata"]]
    if q == "Q3":
        return [[ptr_arg(fmt, plan["kind_ptr"]), "--kind", "metadata"]]
    if q == "Q4":
        return [[ptr_arg(fmt, plan["dup_ptr"]), "--kind", "metadata"]]
    if q == "Q5":
        return [[ptr_arg(fmt, plan["array_ptr"]), "--kind", "exact"]]
    if q == "Q7":
        return [["--%s=%s" % (FMT[fmt]["find_flag"], plan["find_pat"]), "--kind", "text"]]
    if q == "Q8":
        return [[ptr_arg(fmt, plan["token_ptr"]), "--kind", "exact"]]
    if q == "Q9":
        return [[ptr_arg(fmt, plan["width_ptr"]), "--kind", "metadata"]]
    if q == "Q10":
        return [[ptr_arg(fmt, plan["bytetext_ptr"]), "--kind", "metadata"]]
    if q == "Q11":
        return [[ptr_arg(fmt, plan["tag_ptr"]), "--kind", "metadata"]]
    if q == "Q12":
        return [[ptr_arg(fmt, plan["float_ptr"]), "--kind", "metadata"]]
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


def _value(a0, key):
    if not isinstance(a0, dict):
        return None
    return a0.get("value", {}).get(key)


def answer(fmt, q, plan, answers, rcs):
    mod = FMT[fmt]["mod"]
    if q == "Q6":
        return None
    if not answers or answers[0] is None:
        return envelope(q, declined=True, code="rc%d" % (rcs[0] if rcs else 6),
                        reason="observe declined")
    a0 = answers[0]
    if q in ("Q1", "Q5"):
        hexs = a0.get("value_hex")
        if not hexs:
            return envelope(q, declined=True, code="no-token", reason="node not found")
        try:
            v = mod.loads(bytes.fromhex(hexs))
        except ValueError as e:
            return envelope(q, declined=True, code="bad-token", reason=str(e))
        return envelope(q, {"class": mod.host_class(v), "value": mod.canon_value(v)},
                        detail={"token_hex": hexs})
    if q == "Q2":
        span = _value(a0, "span")
        if span is None:
            return envelope(q, declined=True, code="no-span",
                            reason="node has no source span")
        return envelope(q, list(span), detail={})
    if q == "Q3":
        kind = _value(a0, "kind")
        if kind is None:
            return envelope(q, declined=True, code="no-kind", reason="no node at path")
        return envelope(q, KIND_CLASS[fmt].get(kind, kind), detail={"fine_kind": kind})
    if q == "Q4":
        m = _value(a0, "matches")
        if m is None:
            return envelope(q, declined=True, code="no-values", reason="no node at path")
        return envelope(q, {"exists": bool(m), "duplicate_count": m},
                        detail={"method": "pointer match count"})
    if q == "Q7":
        ms = _value(a0, "matches") or []
        out = [{"pointer": m.get("pointer"), "role": m.get("role"), "text": m.get("text")}
               for m in ms]
        return envelope(q, out, detail={"count": len(out)})
    if q == "Q8":
        return envelope(q, {"sha256": a0.get("bytes_sha256"), "len": a0.get("bytes_len")},
                        detail={})
    if q == "Q9":
        w = _value(a0, FMT[fmt]["width_field"])
        if w is None:
            return envelope(q, declined=True, code="no-width", reason="no node at path")
        return envelope(q, w, detail={})
    if q == "Q10":
        kind = _value(a0, "kind")
        if kind is None:
            return envelope(q, declined=True, code="no-kind", reason="no node at path")
        return envelope(q, kind, detail={})
    if q == "Q11":
        t = _value(a0, FMT[fmt]["type_field"])
        if t is None:
            return envelope(q, declined=True, code="no-type", reason="no node at path")
        return envelope(q, t, detail={})
    if q == "Q12":
        f = _value(a0, "float")
        if f is None:
            return envelope(q, declined=True, code="no-float", reason="no node at path")
        return envelope(q, f, detail={})
    return envelope(q, declined=True, code="unknown-question", reason=q)


def run(fmt, bin_path, store, field, plan, outdir, source, packed=False):
    os.makedirs(os.path.join(outdir, "qanswers"), exist_ok=True)
    os.makedirs(os.path.join(outdir, "warm"), exist_ok=True)
    qs = ["Q1", "Q2", "Q3", "Q4", "Q5", "Q7", "Q8", "Q9", "Q10", "Q11", "Q12"]
    cold_us = {}
    ok_probes = {}
    for q in qs:
        probes = probes_for(fmt, q, plan)
        answers, rcs, total = [], [], 0
        allok = True
        for pr in probes:
            a, dt, rc = _observe(bin_path, store, field, pr, packed)
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
            env = answer(fmt, q, plan, objs, [0] * len(objs))
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
    r.add_argument("--format", required=True, choices=["cbor", "msgpack"])
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
