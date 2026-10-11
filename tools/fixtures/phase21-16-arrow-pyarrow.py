#!/usr/bin/env python3
# Phase 26.6 — Arrow IPC economic court: the `pyarrow` comparator lane.
#
# `pyarrow` *is* the reference Arrow reader: it opens the Arrow IPC file directly
# (`pyarrow.ipc.open_file`) and answers the analytical questions with its own
# columnar kernels (`pyarrow.compute`), so Arrow is measured on its own terms
# rather than through the Parquet-projection surrogate the pinned DuckDB wheel
# forced. This lane has the SAME contract as VOLE: the same six questions, the same
# envelope shape, the same estimator (ADR-0054).
#
# Where pyarrow cannot answer a *representation* question it declines **typed**,
# exactly like the other lanes:
#   Q3 — a column's exact source buffer span and min/max *statistics*: the Arrow
#        IPC format stores no per-column statistics, and pyarrow exposes no file
#        offset for a decoded buffer, so this declines `not-retained`.
#   Q6 — `materialize --exact` byte-authority: pyarrow is a reader/projection
#        engine, not a source-retaining archival store; the lane's artifact is the
#        unmodified Arrow file, so reconstructing "the source" from it is identity,
#        not a recovery claim. It declines `not-native`.
#
#   build   --source FILE.arrow --dir PD     (copy the Arrow file into the lane)
#   query   --dir PD --q Qn --plan JSON --out FILE
#   session --dir PD --queries Q1,... --plan JSON --out FILE

import argparse
import json
import os
import shutil
import sys
import time

import pyarrow as pa
import pyarrow.compute as pc
import pyarrow.ipc as ipc

Q3_MSG = ("Arrow IPC stores no per-column min/max statistics, and pyarrow exposes "
          "no exact source offset for a decoded buffer span")
Q6_MSG = ("pyarrow is a reader over the Arrow IPC file, not a source-retaining "
          "archival store: the lane artifact is the unmodified source")


def now_us():
    return int(time.monotonic() * 1_000_000)


def envelope(q, value=None, *, declined=False, code=None, reason="", detail=None, native=True):
    e = {"q": q, "lane": "pyarrow", "declined": bool(declined), "native": native,
         "value": None if declined else value, "detail": detail or {}}
    e["decline"] = {"code": code, "detail": reason} if declined else None
    return e


def arrow_file(d):
    return os.path.join(d, "data.arrow")


def _cell(v):
    return "null" if v is None else str(v)


def _table(d):
    # Arrow IPC has no predicate pushdown into a local file beyond column
    # projection, so read the (bounded) table once and run its compute kernels.
    with ipc.open_file(arrow_file(d)) as r:
        return r.read_all()


def _batches(d):
    with ipc.open_file(arrow_file(d)) as r:
        return r.num_record_batches


def query(d, q, plan):
    if q == "Q1":
        t = _table(d)
        vals = t.column(plan["col_str"]).to_pylist()
        return {"value": [_cell(v) for v in vals]}
    if q == "Q2":
        t = _table(d)
        col = t.column(plan["col_num"])
        mask = pc.greater(col, pa.scalar(plan["gt"], type=col.type))
        return {"value": int(pc.sum(mask).as_py())}
    if q == "Q3":
        return envelope(q, declined=True, code="not-retained", reason=Q3_MSG, native=False)
    if q == "Q4":
        return {"value": int(_batches(d))}
    if q == "Q5":
        t = _table(d)
        col = t.column(plan["col_str"])
        mask = pc.match_substring(col, plan["substr"])
        return {"value": int(pc.sum(mask).as_py())}
    if q == "Q6":
        return envelope(q, declined=True, code="not-native", reason=Q6_MSG, native=False)
    raise RuntimeError("unknown question %s" % q)


QS = ["Q1", "Q2", "Q3", "Q4", "Q5", "Q6"]


def main(argv=None):
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    b = sub.add_parser("build")
    b.add_argument("--source", required=True)
    b.add_argument("--dir", required=True)
    q = sub.add_parser("query")
    q.add_argument("--dir", required=True)
    q.add_argument("--q", required=True)
    q.add_argument("--plan", default="{}")
    q.add_argument("--out", required=True)
    s = sub.add_parser("session")
    s.add_argument("--dir", required=True)
    s.add_argument("--queries", default="")
    s.add_argument("--plan", default="{}")
    s.add_argument("--out", required=True)
    args = ap.parse_args(argv)
    plan = json.loads(getattr(args, "plan", "{}") or "{}")

    if args.cmd == "build":
        t0 = now_us()
        if os.path.isdir(args.dir):
            shutil.rmtree(args.dir)
        os.makedirs(args.dir)
        shutil.copyfile(args.source, arrow_file(args.dir))
        t1 = now_us()
        with open(os.path.join(args.dir, "build.json"), "w") as f:
            json.dump({"lane": "pyarrow", "version": pa.__version__, "build_us": t1 - t0}, f)
        return 0
    if args.cmd == "query":
        env = query(args.dir, args.q, plan)
        with open(args.out, "w") as f:
            json.dump(env, f, sort_keys=True)
        return 0
    if args.cmd == "session":
        batch = []
        answers = {}
        for q in QS:
            t0 = now_us()
            env = query(args.dir, q, plan)
            t1 = now_us()
            answers[q] = env
            batch.append({"q": q, "us": t1 - t0})
        with open(args.out, "w") as f:
            json.dump({"batch": batch, "answers": answers}, f, sort_keys=True)
        return 0
    return 2


if __name__ == "__main__":
    sys.exit(main())
