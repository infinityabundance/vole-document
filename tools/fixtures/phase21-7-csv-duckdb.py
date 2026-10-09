#!/usr/bin/env python3
# Phase 21.7 — CSV/TSV economic court: the DuckDB/Parquet **analytical comparator**.
#
# ## Why this lane exists
#
# The Phase-21 plan *mandates* a DuckDB/Parquet baseline alongside SQLite for the
# tabular formats (CSV/TSV, XLSX, ODS): those engines already embody columnar
# projection, predicate pushdown, compressed pages and metadata indexes, so
# winning only against SQLite there could just mean the wrong competitor was
# chosen. This lane is that comparator, loaded with DuckDB's own CSV reader (the
# real "conventional columnar load").
#
# ## What it honestly is
#
# DuckDB answers **columnar/tabular** questions (cell value, header names, a range
# projection, row count, a lexical find) from a Parquet projection with SQL. It
# does **not** provide exact-source closure or per-answer provenance, and its
# projection is lossy:
#
#   * Q2 (exact raw field token): typed DECLINE (CSV fields are unquoted into
#     columns; no raw token/span is kept).
#   * Q3 (exact record bytes): typed DECLINE (no record bytes/span).
#   * Q8 (exact original bytes): typed DECLINE as `not-native` — Parquet carries no
#     original bytes.
#
# ## Subcommands
#
#   build     --source S --dir D [--metrics M]
#   query     --dir D --q Qn --plan JSON --out OUT
#   session   --dir D --queries Q1,.. --plan JSON --out OUT
#   read-cell --dir D --row R --col C

import argparse
import json
import os
import sys
import time

import duckdb  # pinned 1.5.6, hash-verified wheel (see Dockerfile)


def now_us():
    return int(time.monotonic() * 1_000_000)


def _sniff_delim(path):
    with open(path, "rb") as f:
        head = f.readline()
    if head[:3] == b"\xef\xbb\xbf":
        head = head[3:]
    return "\t" if head.count(b"\t") > head.count(b",") else ","


def build(source, out_dir):
    os.makedirs(out_dir, exist_ok=True)
    delim = _sniff_delim(source)
    con = duckdb.connect()
    # DuckDB's own CSV reader: a single streaming pass, with an explicit 0-based
    # row id so range/point queries align with the other lanes.
    con.execute(
        "CREATE TABLE t AS SELECT row_number() OVER () - 1 AS __rn, * FROM read_csv(?, "
        "delim=?, header=true, null_padding=true, parallel=false, sample_size=-1)",
        (source, delim),
    )
    cols = [row[0] for row in con.execute("DESCRIBE t").fetchall()]
    cols = [c.lstrip("\ufeff") for c in cols if c != "__rn"]
    nrows = con.execute("SELECT COUNT(*) FROM t").fetchone()[0]
    con.execute(f"COPY t TO '{out_dir}/t.parquet' (FORMAT PARQUET, COMPRESSION ZSTD)")
    con.close()
    with open(os.path.join(out_dir, "meta.json"), "w") as f:
        json.dump({"columns": cols, "nrows": int(nrows), "delimiter": delim}, f, sort_keys=True)
    return {"columns": len(cols), "rows": int(nrows), "source_len": os.path.getsize(source)}


def _connect(out_dir):
    con = duckdb.connect()
    con.execute(f"CREATE TABLE t AS SELECT * FROM read_parquet('{out_dir}/t.parquet')")
    with open(os.path.join(out_dir, "meta.json")) as f:
        meta = json.load(f)
    return con, meta


def _env(q, value, *, declined=False, code=None, reason="", native=True, detail=None):
    e = {"q": q, "lane": "duckdb", "declined": bool(declined), "native": native,
         "value": None if declined else value, "detail": detail or {}}
    e["decline"] = {"code": code, "detail": reason} if declined else None
    return e


def _col(meta, c):
    cols = meta["columns"]
    if c < 0 or c >= len(cols):
        return None
    return cols[c]


def q_answer(con, meta, q, plan):
    if q == "Q1":
        col = _col(meta, plan["col"])
        if col is None:
            return _env(q, None, declined=True, code="no-such-column", reason="column out of range")
        r = con.execute(
            f'SELECT CAST("{col}" AS VARCHAR) FROM t WHERE __rn = ?', (plan["row"],)
        ).fetchone()
        if r is None:
            return _env(q, None, declined=True, code="no-such-row", reason="row out of range")
        return _env(q, r[0])
    if q == "Q2":
        return _env(q, None, declined=True, code="no-raw-token",
                    reason="CSV fields are unquoted into columns; no raw token/span is kept")
    if q == "Q3":
        return _env(q, None, declined=True, code="no-record-bytes",
                    reason="the columnar projection keeps no exact record bytes or span")
    if q == "Q4":
        return _env(q, list(meta["columns"]))
    if q == "Q5":
        col = _col(meta, plan["col"])
        if col is None:
            return _env(q, None, declined=True, code="no-such-column", reason="column out of range")
        rows = con.execute(
            f'SELECT CAST("{col}" AS VARCHAR) FROM t WHERE __rn BETWEEN ? AND ? ORDER BY __rn',
            (plan["r1"], plan["r2"]),
        ).fetchall()
        return _env(q, [r[0] for r in rows])
    if q == "Q6":
        r = con.execute("SELECT COUNT(*) FROM t").fetchone()
        return _env(q, int(r[0]))
    if q == "Q7":
        col = _col(meta, plan["col"])
        if col is None:
            return _env(q, None, declined=True, code="no-such-column", reason="column out of range")
        r = con.execute(
            f'SELECT COUNT(*) FROM t WHERE contains(CAST("{col}" AS VARCHAR), ?)', (plan["pattern"],)
        ).fetchone()
        return _env(q, int(r[0]))
    if q == "Q8":
        return _env(q, None, declined=True, code="not-native", native=False,
                    reason="Parquet carries no original bytes; there is no exact-source closure")
    return _env(q, None, declined=True, code="unknown-question", reason=q)


def read_cell(out_dir, row, col):
    con, meta = _connect(out_dir)
    name = _col(meta, col)
    t0 = now_us()
    r = con.execute(f'SELECT CAST("{name}" AS VARCHAR) FROM t WHERE __rn = ?', (row,)).fetchone()
    us = now_us() - t0
    con.close()
    print(json.dumps({"ok": r is not None, "value": r[0] if r else None, "us": us}, sort_keys=True))
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    b = sub.add_parser("build")
    b.add_argument("--source", required=True)
    b.add_argument("--dir", required=True)
    b.add_argument("--metrics", default=None)
    q = sub.add_parser("query")
    q.add_argument("--dir", required=True)
    q.add_argument("--q", required=True)
    q.add_argument("--plan", default="{}")
    q.add_argument("--out", required=True)
    s = sub.add_parser("session")
    s.add_argument("--dir", required=True)
    s.add_argument("--queries", required=True)
    s.add_argument("--plan", default="{}")
    s.add_argument("--out", required=True)
    rd = sub.add_parser("read-cell")
    rd.add_argument("--dir", required=True)
    rd.add_argument("--row", type=int, required=True)
    rd.add_argument("--col", type=int, required=True)
    ns = ap.parse_args(argv)
    if ns.cmd == "build":
        m = build(ns.source, ns.dir)
        if ns.metrics:
            with open(ns.metrics, "w") as f:
                json.dump(m, f, sort_keys=True)
        print(json.dumps(m, sort_keys=True))
        return 0
    if ns.cmd == "query":
        con, meta = _connect(ns.dir)
        env = q_answer(con, meta, ns.q, json.loads(ns.plan))
        env["q"] = ns.q
        con.close()
        with open(ns.out, "w") as f:
            json.dump(env, f, sort_keys=True)
        return 0
    if ns.cmd == "session":
        con, meta = _connect(ns.dir)
        batch = []
        for one in [x for x in ns.queries.split(",") if x]:
            t0 = now_us()
            env = q_answer(con, meta, one, json.loads(ns.plan))
            env["q"] = one
            batch.append({"q": one, "us": now_us() - t0, "env": env})
        con.close()
        with open(ns.out, "w") as f:
            json.dump({"batch": batch}, f, sort_keys=True)
        return 0
    if ns.cmd == "read-cell":
        return read_cell(ns.dir, ns.row, ns.col)
    return 2


if __name__ == "__main__":
    sys.exit(main())
