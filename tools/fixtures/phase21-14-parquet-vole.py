#!/usr/bin/env python3
# Phase 21.14 — Parquet economic court: the VOLE lane driver.
#
# Drives the shipped CLI (`observe`, `materialize`) for the six contract questions
# and records the wall time (microseconds) of each. VOLE answers the analytical
# questions by **materializing bounded observations** of the archival field — it is
# not a query engine; a predicate count is a decoded column plus a count in this
# driver, which is exactly the point the economic court records.
#
#   run --bin BIN --store STORE --field HEX --plan JSON --outdir DIR --source SRC
#
# Plan keys: col_num, gt, col_str, substr.

import argparse
import hashlib
import json
import os
import subprocess
import sys
import time


def now_us():
    return int(time.monotonic() * 1_000_000)


def envelope(q, value=None, *, declined=False, code=None, reason="", detail=None, native=True):
    e = {"q": q, "lane": "vole", "declined": bool(declined), "native": native,
         "value": None if declined else value, "detail": detail or {}}
    e["decline"] = {"code": code, "detail": reason} if declined else None
    return e


class Vole:
    def __init__(self, bin_path, store, field, packed=True, source=None):
        self.bin = bin_path
        self.store = store
        self.field = field
        self.packed = packed
        self.source = source
        self.calls = {}

    def observe(self, flags, kind):
        cmd = [self.bin, "observe", "--store", self.store, "--field", self.field]
        if self.packed:
            cmd.append("--packed")
        cmd += flags + ["--kind", kind]
        p = subprocess.run(cmd, capture_output=True)
        if p.returncode != 0:
            raise RuntimeError("observe rc=%d: %s" % (p.returncode, p.stderr.decode()[:200]))
        return json.loads(p.stdout.decode())

    def materialize(self, out):
        cmd = [self.bin, "materialize", "--store", self.store, "--field", self.field,
               "--exact", "--output", out]
        if self.packed:
            cmd.append("--packed")
        p = subprocess.run(cmd, capture_output=True)
        if p.returncode != 0:
            raise RuntimeError("materialize rc=%d: %s" % (p.returncode, p.stderr.decode()[:200]))

    # -- the six questions ----------------------------------------------------

    def q1_column_values(self, plan):
        a = self.observe(["--parquet-column", str(plan["col_str"])], "metadata")
        return a["value"]["values"]

    def q2_predicate_count(self, plan):
        a = self.observe(["--parquet-column", str(plan["col_num"])], "text")
        n = 0
        for line in a["text"].split("\n"):
            if line == "":
                continue
            try:
                if int(line) > plan["gt"]:
                    n += 1
            except ValueError:
                continue
        return n

    def q3_span_stats(self, plan):
        a = self.observe(["--parquet-column", str(plan["col_num"])], "metadata")
        chunks = a["value"]["chunks"]
        if not chunks:
            raise RuntimeError("no chunks")
        c = chunks[0]
        return {"row_group": 0, "span": c["span"], "min": c["min"], "max": c["max"]}

    def q4_row_group_count(self, plan):
        a = self.observe(["--metadata"], "metadata")
        return a["value"]["row_groups"]

    def q5_find(self, plan):
        a = self.observe(["--parquet-column", str(plan["col_str"])], "text")
        sub = plan["substr"]
        return sum(1 for line in a["text"].split("\n") if sub in line)

    def q6_exact(self, plan, outdir):
        out = os.path.join(outdir, "materialized.bin")
        self.materialize(out)
        with open(out, "rb") as f:
            data = f.read()
        return {"length": len(data), "sha256": hashlib.sha256(data).hexdigest()}


QS = ["Q1", "Q2", "Q3", "Q4", "Q5", "Q6"]


def run(v, plan, outdir):
    qdir = os.path.join(outdir, "qanswers")
    os.makedirs(qdir, exist_ok=True)
    cold, warm = {}, {}
    answers = {}

    def once():
        return {
            "Q1": lambda: v.q1_column_values(plan),
            "Q2": lambda: v.q2_predicate_count(plan),
            "Q3": lambda: v.q3_span_stats(plan),
            "Q4": lambda: v.q4_row_group_count(plan),
            "Q5": lambda: v.q5_find(plan),
            "Q6": lambda: v.q6_exact(plan, outdir),
        }

    for pass_name, sink in (("cold", cold), ("warm", warm)):
        fns = once()
        for q in QS:
            t0 = now_us()
            try:
                value = fns[q]()
                env = envelope(q, value=value)
            except Exception as e:  # a typed decline or an infrastructure error
                env = envelope(q, declined=True, code="decline", reason=str(e))
            t1 = now_us()
            sink[q] = t1 - t0
            if pass_name == "cold":
                answers[q] = env
                with open(os.path.join(qdir, q + ".json"), "w") as f:
                    json.dump(env, f, sort_keys=True)

    with open(os.path.join(outdir, "cold_us.json"), "w") as f:
        json.dump(cold, f)
    with open(os.path.join(outdir, "warm_us.json"), "w") as f:
        json.dump(warm, f)


def main(argv=None):
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run")
    r.add_argument("--bin", required=True)
    r.add_argument("--store", required=True)
    r.add_argument("--field", required=True)
    r.add_argument("--plan", required=True)
    r.add_argument("--outdir", required=True)
    r.add_argument("--source", default=None)
    r.add_argument("--packed", action="store_true")
    args = ap.parse_args(argv)
    if args.cmd == "run":
        plan = json.loads(args.plan)
        v = Vole(args.bin, args.store, args.field, packed=args.packed, source=args.source)
        run(v, plan, args.outdir)
        return 0
    return 2


if __name__ == "__main__":
    sys.exit(main())
