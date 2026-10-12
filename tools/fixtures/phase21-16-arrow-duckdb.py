#!/usr/bin/env python3
# Phase 28.2 — Arrow IPC economic court: the DuckDB analytical comparator.
#
# PHASE 28.2 CHANGE (the review's "28.0"). Before 28.2 this lane read a **Parquet
# projection** of the logical table, because the pinned DuckDB wheel exposes only
# the Arrow C data interface, not an Arrow IPC *file* reader. That made the "Arrow"
# comparison partly a Parquet comparison. The DuckDB wheel **does** integrate with
# real Arrow through the C data interface, so the lane now opens the `.arrow` file
# with `pyarrow.ipc.open_file(...).read_all()` and registers that Arrow table with
# DuckDB (`con.register("arrow_tbl", table)`); DuckDB then executes over the **real
# Arrow data** (zero-copy through the Arrow C data interface). This is the fair
# "Arrow" comparison the review demanded — not a Parquet surrogate.
#
# Allowed optimizations (recorded):
#   * column projection (only the referenced column is projected for Q1/Q5);
#   * predicate execution pushed into the Arrow scan (`WHERE "<col>" > K`);
#   * **prepared statements** reused across the steady-state iterations;
#   * tuned parallelism (`PRAGMA threads=4`, recorded in the receipt).
# Honest boundaries (declared, not hidden): the Arrow IPC format carries **no
# per-column statistics**, so there is no zonemap/row-group pruning to exploit
# (DuckDB's Arrow scanner cannot prune what Arrow does not record), and DuckDB
# exposes no exact source buffer span. Q3 therefore declines `not-retained`. DuckDB
# is not a source-retaining store, so Q6 declines `not-native`.
#
#   build      --source FILE.arrow --dir DD
#   query      --dir DD --q Qn --plan JSON --out FILE            (cold-process)
#   steady     --dir DD --plan JSON --out FILE [--iters R --warmup W]
#   coldcache  --dir DD --plan JSON --out FILE
#   session    --dir DD --queries Q1,... --plan JSON --out FILE  (legacy single-shot)
#   explain    --dir DD --plan JSON --out FILE                   (EXPLAIN + ANALYZE)

import argparse
import importlib.util as _ilu
import json
import os
import shutil
import sys
import time

import duckdb
import pyarrow.ipc as ipc

THREADS = 4

Q3_MSG = ("Arrow IPC carries no per-column min/max statistics and DuckDB exposes no "
          "exact source buffer span for an Arrow buffer; there is no zonemap to prune")
Q6_MSG = "DuckDB is not a source-retaining store: the original Arrow bytes are not recoverable"

QS = ["Q1", "Q2", "Q3", "Q4", "Q5", "Q6"]


def _load_harness():
    here = os.path.dirname(os.path.abspath(__file__))
    spec = _ilu.spec_from_file_location("phase28_2_harness",
                                        os.path.join(here, "phase28-2-harness.py"))
    mod = _ilu.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


H = _load_harness()


def now_us():
    return int(time.monotonic() * 1_000_000)


def envelope(q, value=None, *, declined=False, code=None, reason="", detail=None, native=True):
    e = {"q": q, "lane": "duckdb", "declined": bool(declined), "native": native,
         "value": None if declined else value, "detail": detail or {}}
    e["decline"] = {"code": code, "detail": reason} if declined else None
    return e


def arrow_file(d):
    return os.path.join(d, "data.arrow")


def _cell(v):
    return "null" if v is None else str(v)


class State:
    """A persistent DuckDB connection over the real Arrow table (warm state)."""

    def __init__(self, d):
        self.con = duckdb.connect()
        self.con.execute("PRAGMA threads=%d" % THREADS)
        self.reader = ipc.open_file(arrow_file(d))
        self.table = self.reader.read_all()
        # Zero-copy Arrow C data interface registration; DuckDB scans the Arrow data.
        self.con.register("arrow_tbl", self.table)
        self._prep = {}

    def close(self):
        try:
            self.con.close()
            self.reader.close()
        except Exception:
            pass

    def colname(self, i):
        return self.table.schema.field(i).name

    def sql_for(self, q, plan):
        if q == "Q1":
            return 'SELECT "%s" FROM arrow_tbl' % self.colname(plan["col_str"])
        if q == "Q2":
            return 'SELECT count(*) FROM arrow_tbl WHERE "%s" > %d' % (
                self.colname(plan["col_num"]), plan["gt"])
        if q == "Q5":
            return "SELECT count(*) FROM arrow_tbl WHERE \"%s\" LIKE '%%%s%%'" % (
                self.colname(plan["col_str"]), plan["substr"])
        return None

    def prepared(self, q, plan):
        # DuckDB's Python client exposes no statement handle, but DuckDB itself
        # supports SQL-level prepared statements; prepare once, EXECUTE repeatedly.
        if q not in self._prep:
            name = "p_" + q.lower()
            self.con.execute("PREPARE %s AS %s" % (name, self.sql_for(q, plan)))
            self._prep[q] = name
        return self._prep[q]

    def num_batches(self):
        return int(self.reader.num_record_batches)


def state_fresh(d):
    return State(d)


def answer(st, q, plan):
    if q in ("Q1", "Q2", "Q5"):
        stmt = st.prepared(q, plan)
        rows = st.con.execute("EXECUTE %s" % stmt).fetchall()
        if q == "Q1":
            return {"value": [_cell(r[0]) for r in rows]}
        return {"value": int(rows[0][0])}
    if q == "Q3":
        return envelope(q, declined=True, code="not-retained", reason=Q3_MSG, native=False)
    if q == "Q4":
        # The record-batch count of the real Arrow data DuckDB registered (the Arrow
        # container's batch structure is preserved through the C data interface).
        return {"value": st.num_batches()}
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
            json.dump({"lane": "duckdb", "version": duckdb.__version__,
                       "threads": THREADS, "arrow_integration": "arrow C data interface",
                       "prepared_statements": "SQL-level PREPARE/EXECUTE",
                       "build_us": t1 - t0}, f)
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
        stt = state_fresh(args.dir)
        out = {"lane": "duckdb", "threads": THREADS,
               "arrow_integration": "arrow C data interface (con.register)",
               "zonemap_pruning": "not available (Arrow IPC stores no per-column statistics)",
               "plans": {}}
        for qn in ("Q1", "Q2", "Q5"):
            sql = stt.sql_for(qn, plan)
            try:
                logical = stt.con.execute("EXPLAIN " + sql).fetchall()
                out["plans"][qn] = {"sql": sql,
                                    "explain": logical[0][1] if logical else ""}
            except Exception as exc:  # pragma: no cover
                out["plans"][qn] = {"sql": sql, "error": str(exc)}
        # Operator-level timing where cheap: EXPLAIN ANALYZE on the predicate query.
        try:
            sql2 = stt.sql_for("Q2", plan)
            analyzed = stt.con.execute("EXPLAIN ANALYZE " + sql2).fetchall()
            out["analyze_q2"] = analyzed[0][1] if analyzed else ""
        except Exception as exc:  # pragma: no cover
            out["analyze_q2"] = {"error": str(exc)}
        stt.close()
        with open(args.out, "w") as f:
            json.dump(out, f, sort_keys=True)
        return 0
    return 2


if __name__ == "__main__":
    sys.exit(main())
