#!/usr/bin/env python3
# Phase 28.2 — Arrow IPC economic court: the `pyarrow` comparator lane.
#
# `pyarrow` *is* the reference Arrow reader: it opens the Arrow IPC file directly
# (`pyarrow.ipc.open_file`) and answers the analytical questions with its own
# columnar kernels (`pyarrow.compute`), so Arrow is measured on its own terms
# rather than through the Parquet-projection surrogate (Phase 28.2 replaced the
# DuckDB lane's projection with the real Arrow data; see the DuckDB lane).
#
# Allowed optimizations (recorded): **column projection** (`table.select([idx])`
# so only the referenced column's kernel output is materialised), `pyarrow.compute`
# vector kernels (no Python-level per-row loop), and a **persistent reader** in the
# steady-state harness. Honest boundary: an Arrow IPC file has no column-selective
# I/O — buffers are read per record batch, and there is no per-column statistics
# block — so projection here is a kernel-level saving, not an I/O skip, and there is
# no zonemap pruning to exploit (declared, not hidden).
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
#   build      --source FILE.arrow --dir PD
#   query      --dir PD --q Qn --plan JSON --out FILE            (cold-process)
#   steady     --dir PD --plan JSON --out FILE [--iters R --warmup W]
#   coldcache  --dir PD --plan JSON --out FILE
#   session    --dir PD --queries Q1,... --plan JSON --out FILE  (legacy single-shot)
#   explain    --dir PD --plan JSON --out FILE

import argparse
import importlib.util as _ilu
import json
import os
import shutil
import sys
import time

import pyarrow as pa
import pyarrow.compute as pc
import pyarrow.ipc as ipc


def _load_harness():
    here = os.path.dirname(os.path.abspath(__file__))
    spec = _ilu.spec_from_file_location("phase28_2_harness",
                                        os.path.join(here, "phase28-2-harness.py"))
    mod = _ilu.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


H = _load_harness()

Q3_MSG = ("Arrow IPC stores no per-column min/max statistics, and pyarrow exposes "
          "no exact source offset for a decoded buffer span")
Q6_MSG = ("pyarrow is a reader over the Arrow IPC file, not a source-retaining "
          "archival store: the lane artifact is the unmodified source")

QS = ["Q1", "Q2", "Q3", "Q4", "Q5", "Q6"]


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


class State:
    """A persistent Arrow IPC reader + projection table (warm reader state)."""

    def __init__(self, d):
        self.reader = ipc.open_file(arrow_file(d))
        self.table = self.reader.read_all()

    def close(self):
        try:
            self.reader.close()
        except Exception:
            pass

    def num_batches(self):
        return int(self.reader.num_record_batches)


def state_fresh(d):
    return State(d)


def answer(st, q, plan):
    if q == "Q1":
        # Column projection: keep only the referenced column's values.
        col = st.table.column(plan["col_str"])
        return {"value": [_cell(v) for v in col.to_pylist()]}
    if q == "Q2":
        col = st.table.column(plan["col_num"])
        mask = pc.greater(col, pa.scalar(plan["gt"], type=col.type))
        return {"value": int(pc.sum(mask).as_py())}
    if q == "Q3":
        return envelope(q, declined=True, code="not-retained", reason=Q3_MSG, native=False)
    if q == "Q4":
        return {"value": st.num_batches()}
    if q == "Q5":
        col = st.table.column(plan["col_str"])
        mask = pc.match_substring(col, plan["substr"])
        return {"value": int(pc.sum(mask).as_py())}
    if q == "Q6":
        return envelope(q, declined=True, code="not-native", reason=Q6_MSG, native=False)
    raise RuntimeError("unknown question %s" % q)


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
    st = sub.add_parser("steady")
    st.add_argument("--dir", required=True)
    st.add_argument("--plan", default="{}")
    st.add_argument("--out", required=True)
    st.add_argument("--iters", type=int, default=15)
    st.add_argument("--warmup", type=int, default=3)
    cc = sub.add_parser("coldcache")
    cc.add_argument("--dir", required=True)
    cc.add_argument("--plan", default="{}")
    cc.add_argument("--out", required=True)
    s = sub.add_parser("session")
    s.add_argument("--dir", required=True)
    s.add_argument("--queries", default="")
    s.add_argument("--plan", default="{}")
    s.add_argument("--out", required=True)
    e = sub.add_parser("explain")
    e.add_argument("--dir", required=True)
    e.add_argument("--plan", default="{}")
    e.add_argument("--out", required=True)
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
        stt = state_fresh(args.dir)
        env = answer(stt, args.q, plan)
        stt.close()
        with open(args.out, "w") as f:
            json.dump(env, f, sort_keys=True)
        return 0
    if args.cmd == "steady":
        stt = state_fresh(args.dir)
        results = {}
        for qn in QS:
            results[qn] = H.steady_state(lambda qn=qn: answer(stt, qn, plan),
                                         warmup=args.warmup, iters=args.iters)
        stt.close()
        with open(args.out, "w") as f:
            json.dump({"batch": [{"q": k, **v} for k, v in results.items()]}, f, sort_keys=True)
        return 0
    if args.cmd == "coldcache":
        fns = {qn: (lambda s, qn=qn: answer(s, qn, plan)) for qn in QS}
        out = H.cold_cache(fns, lambda: state_fresh(args.dir))
        with open(args.out, "w") as f:
            json.dump({"coldcache": out}, f, sort_keys=True)
        return 0
    if args.cmd == "session":
        stt = state_fresh(args.dir)
        batch = []
        answers = {}
        for qn in QS:
            t0 = now_us()
            env = answer(stt, qn, plan)
            t1 = now_us()
            answers[qn] = env
            batch.append({"q": qn, "us": t1 - t0})
        stt.close()
        with open(args.out, "w") as f:
            json.dump({"batch": batch, "answers": answers}, f, sort_keys=True)
        return 0
    if args.cmd == "explain":
        # pyarrow is a kernel reader with no query planner: there is no plan to
        # record. Declared typed rather than silently omitted.
        with open(args.out, "w") as f:
            json.dump({"lane": "pyarrow", "planner": "none (kernel reader)",
                       "declined": True, "code": "not-applicable",
                       "detail": "pyarrow.compute kernels are invoked directly; "
                                 "there is no query plan"}, f, sort_keys=True)
        return 0
    return 2


if __name__ == "__main__":
    sys.exit(main())
