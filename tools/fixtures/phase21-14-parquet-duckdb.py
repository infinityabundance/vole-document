#!/usr/bin/env python3
# Phase 21.14 / 28.2 — Parquet economic court: the DuckDB/Parquet analytical comparator.
#
# DuckDB reads the very same Parquet bytes natively: projection, predicate
# execution, compressed pages, and metadata statistics (row-group min/max zonemaps
# for pruning) are all built in. It is the **mandatory** comparator for this
# analytical format, and the court records plainly where it wins. It is **not** a
# source-retaining store: it cannot reproduce the original Parquet bytes, so the
# exact-closure question is a typed `not-native` decline.
#
# PHASE 28.2 — allowed optimizations (recorded): column projection; predicate
# execution pushed into the Parquet scan with **zonemap/statistics pruning**;
# **SQL-level prepared statements** (`PREPARE`/`EXECUTE`) reused across the
# steady-state iterations; the **Parquet object cache** (`enable_object_cache`), so
# the footer/metadata is read once and reused; and tuned parallelism
# (`PRAGMA threads=4`). The `EXPLAIN ANALYZE` output (operator-level timing and the
# pruned row groups) is recorded.
#
#   build      --source FILE --dir DD
#   query      --dir DD --q Qn --plan JSON --out FILE            (cold-process)
#   steady     --dir DD --plan JSON --out FILE [--iters R --warmup W]
#   coldcache  --dir DD --plan JSON --out FILE
#   session    --dir DD --queries Q1,... --plan JSON --out FILE  (legacy single-shot)
#   explain    --dir DD --plan JSON --out FILE                    (EXPLAIN + ANALYZE)

import argparse
import importlib.util as _ilu
import json
import os
import shutil
import sys
import time

import duckdb

THREADS = 4

Q6_MSG = "DuckDB/Parquet is not a source-retaining store: the original Parquet bytes are not recoverable"

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


def parquet_file(d):
    return os.path.join(d, "data.parquet")


def _cell(v):
    return "null" if v is None else str(v)


class State:
    """A persistent DuckDB connection over the Parquet file (warm state)."""

    def __init__(self, d):
        self.path = parquet_file(d)
        self.con = duckdb.connect()
        self.con.execute("PRAGMA threads=%d" % THREADS)
        self.con.execute("PRAGMA enable_object_cache=true")
        rows = self.con.execute(
            "DESCRIBE SELECT * FROM read_parquet('%s')" % self.path).fetchall()
        self.cols = [r[0] for r in rows]
        self._prep = {}

    def close(self):
        try:
            self.con.close()
        except Exception:
            pass

    def sql_for(self, q, plan):
        p = self.path
        if q == "Q1":
            return 'SELECT "%s" FROM read_parquet(\'%s\')' % (self.cols[plan["col_str"]], p)
        if q == "Q2":
            return 'SELECT count(*) FROM read_parquet(\'%s\') WHERE "%s" > %d' % (
                p, self.cols[plan["col_num"]], plan["gt"])
        if q == "Q5":
            return "SELECT count(*) FROM read_parquet('%s') WHERE \"%s\" LIKE '%%%s%%'" % (
                p, self.cols[plan["col_str"]], plan["substr"])
        return None

    def prepared(self, q, plan):
        if q not in self._prep:
            name = "p_" + q.lower()
            self.con.execute("PREPARE %s AS %s" % (name, self.sql_for(q, plan)))
            self._prep[q] = name
        return self._prep[q]

    def meta_query(self, plan):
        return self.con.execute(
            "SELECT data_page_offset, dictionary_page_offset, total_compressed_size, "
            "stats_min_value, stats_max_value "
            "FROM parquet_metadata('%s') WHERE row_group_id = 0 AND column_id = %d"
            % (self.path, plan["col_num"])).fetchall()


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
        rows = st.meta_query(plan)
        if not rows:
            raise RuntimeError("no metadata row")
        data_off, dict_off, size, mn, mx = rows[0]
        start = int(data_off)
        if dict_off is not None and int(dict_off) > 0 and int(dict_off) < start:
            start = int(dict_off)
        return {"value": {"row_group": 0, "span": [start, start + int(size)],
                          "min": None if mn is None else str(mn),
                          "max": None if mx is None else str(mx)}}
    if q == "Q4":
        rows = st.con.execute("SELECT count(DISTINCT row_group_id) FROM parquet_metadata('%s')"
                              % st.path).fetchall()
        return {"value": int(rows[0][0])}
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
        shutil.copyfile(args.source, parquet_file(args.dir))
        t1 = now_us()
        with open(os.path.join(args.dir, "build.json"), "w") as f:
            json.dump({"lane": "duckdb", "version": duckdb.__version__,
                       "threads": THREADS, "object_cache": True,
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
        out = {"lane": "duckdb", "threads": THREADS, "object_cache": True,
               "plans": {}}
        for qn in ("Q1", "Q2", "Q5"):
            sql = stt.sql_for(qn, plan)
            try:
                logical = stt.con.execute("EXPLAIN " + sql).fetchall()
                out["plans"][qn] = {"sql": sql, "explain": logical[0][1] if logical else ""}
            except Exception as exc:  # pragma: no cover
                out["plans"][qn] = {"sql": sql, "error": str(exc)}
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
