#!/usr/bin/env python3
# Phase 21.16 — Arrow IPC economic court: the DuckDB analytical comparator.
#
# The pinned DuckDB wheel exposes only the Arrow **C data interface** scanners
# (`arrow_scan`/`arrow_scan_dumb`), not an Arrow IPC file reader. So the comparator
# reads a **Parquet projection of the same logical table** (written by
# `make-arrow.py --econ` alongside the `.arrow` fixture): projection, predicate
# execution, compressed pages, and metadata statistics are all built in. This is
# recorded plainly in the court's matrix. DuckDB is **not** a source-retaining
# store: it cannot reproduce the original Arrow bytes, so the exact-closure question
# is a typed `not-native` decline.
#
#   build   --source PROJECTION.parquet --dir DD
#   query   --dir DD --q Qn --plan JSON --out FILE
#   session --dir DD --queries Q1,... --plan JSON --out FILE

import argparse
import json
import os
import shutil
import sys
import time

import duckdb

Q6_MSG = "DuckDB is not a source-retaining store: the original Arrow bytes are not recoverable"


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


def col_names(d):
    p = parquet_file(d)
    rows = duckdb.sql("DESCRIBE SELECT * FROM read_parquet('%s')" % p).fetchall()
    return [r[0] for r in rows]


def query(d, q, plan):
    p = parquet_file(d)
    if q == "Q1":
        nm = col_names(d)[plan["col_str"]]
        rows = duckdb.sql('SELECT "%s" FROM read_parquet(\'%s\')' % (nm, p)).fetchall()
        return {"value": [_cell(r[0]) for r in rows]}
    if q == "Q2":
        nm = col_names(d)[plan["col_num"]]
        rows = duckdb.sql('SELECT count(*) FROM read_parquet(\'%s\') WHERE "%s" > %d'
                          % (p, nm, plan["gt"])).fetchall()
        return {"value": int(rows[0][0])}
    if q == "Q3":
        rows = duckdb.sql(
            "SELECT data_page_offset, dictionary_page_offset, total_compressed_size, "
            "stats_min_value, stats_max_value "
            "FROM parquet_metadata('%s') WHERE row_group_id = 0 AND column_id = %d"
            % (p, plan["col_num"])).fetchall()
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
        rows = duckdb.sql("SELECT count(DISTINCT row_group_id) FROM parquet_metadata('%s')"
                          % p).fetchall()
        return {"value": int(rows[0][0])}
    if q == "Q5":
        nm = col_names(d)[plan["col_str"]]
        rows = duckdb.sql('SELECT count(*) FROM read_parquet(\'%s\') WHERE "%s" LIKE \'%%%s%%\''
                          % (p, nm, plan["substr"])).fetchall()
        return {"value": int(rows[0][0])}
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
        shutil.copyfile(args.source, parquet_file(args.dir))
        t1 = now_us()
        with open(os.path.join(args.dir, "build.json"), "w") as f:
            json.dump({"lane": "duckdb", "build_us": t1 - t0}, f)
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
