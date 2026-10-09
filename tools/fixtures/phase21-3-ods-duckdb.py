#!/usr/bin/env python3
# Phase 21.3.2 — ODS economic court: the DuckDB/Parquet **analytical comparator**.
#
# ## Why this lane exists
#
# The Phase-21 plan *mandates* a DuckDB/Parquet baseline alongside SQLite for the
# tabular formats (CSV/TSV, XLSX, ODS): those engines already embody columnar
# projection, predicate pushdown, compressed pages and metadata indexes, so
# winning only against SQLite there could just mean the wrong competitor was
# chosen (the Phase-22 "maximize the competitor first" rule). This lane is that
# comparator.
#
# ## What it honestly is
#
# DuckDB answers **columnar/tabular** questions (typed cell value, stored
# formula, containing named range, cell style, content part, named-expression
# references) from a Parquet projection with SQL. It does **not** provide
# exact-source closure or per-answer provenance, and its projection is lossy:
#
#   * Q8 (exact decoded cell span): typed DECLINE (the span lives in the
#     OpenDocument content member, not in a column).
#   * Q9 (embedded resource bytes): typed DECLINE (no embedded member bytes are
#     kept in the columnar model).
#   * Q10 (exact original bytes): typed DECLINE as `not-native` — the lane keeps
#     the source bytes only as a **labelled passthrough** so its storage is
#     source-retaining and comparable; that passthrough is explicitly NOT counted
#     as a native columnar capability.
#
# The extraction is shared byte-for-byte with the SQLite baseline
# (`phase21-3-ods-baseline.py:extract`) so the only difference between the two
# comparators is the *storage and query engine*, never the parse.
#
# ## Subcommands
#
#   build   --source S --dir D [--metrics M]
#   query   --dir D --q Qn --plan JSON --out OUT
#   session --dir D --queries Q1,Q2,.. --plan JSON --out OUT

import argparse
import csv as _csv
import importlib.util
import json
import os
import re
import sys
import time

import duckdb  # pinned 1.5.6, hash-verified wheel (see Dockerfile)

B = None  # the baseline module (extract + helpers), loaded lazily


def _load_baseline():
    global B
    if B is not None:
        return B
    here = os.path.dirname(os.path.abspath(__file__))
    path = os.path.join(here, "phase21-3-ods-baseline.py")
    spec = importlib.util.spec_from_file_location("phase21_3_ods_baseline", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    B = mod
    return B


def now_us():
    return int(time.monotonic() * 1_000_000)


def _sha256(b):
    import hashlib
    return hashlib.sha256(b).hexdigest()


def build(source, out_dir):
    base = _load_baseline()
    os.makedirs(out_dir, exist_ok=True)
    doc = base.extract(source)

    cell_rows = []
    for s in doc["sheets"]:
        for row in s["rows"]:
            for c in row["cells"]:
                cell_rows.append((
                    s["index"], c["row"], c["col"], c["ref"], int(c["covered"]),
                    c["value_type"], c["value"], c["boolean_value"], c["date_value"],
                    c["string_value"], c["style_name"], c["formula"], c["text"],
                ))
    style_rows = []
    for scope, styles in (("auto", doc["styles_auto"]), ("named", doc["styles_named"])):
        for st in styles:
            style_rows.append((
                scope, st["name"], st["family"], st["parent"], st["data_style"],
                json.dumps(st["table_cell_properties"], sort_keys=True),
                json.dumps(st["text_properties"], sort_keys=True),
            ))
    sheet_rows = [
        (s["index"], s["name"], int(s["display"]), s["part"], s["part_ordinal"])
        for s in doc["sheets"]
    ]
    named_rows = []
    ref_rows = []
    for ne in doc["named_expressions"]:
        rb = base._range_bounds(ne["cell_range_address"])
        named_rows.append((
            ne["name"], ne["kind"], ne["base_cell_address"], ne["cell_range_address"],
            ne["expression"],
            rb[0] if rb else None, rb[1] if rb else None, rb[2] if rb else None,
            rb[3] if rb else None, rb[4] if rb else None,
        ))
        for sh in list(dict.fromkeys(base._address_sheets(ne["base_cell_address"])
                                    + base._address_sheets(ne["cell_range_address"]))):
            ref_rows.append((ne["name"], sh))
    comment_rows = [
        (c["sheet"], c["ref"], c["row"], c["col"], c["author"], c["date"], c["text"])
        for c in doc["comments"]
    ]

    with open(source, "rb") as f:
        blob = f.read()

    import shutil
    tmp = os.path.join(out_dir, ".ingest")
    os.makedirs(tmp, exist_ok=True)

    def _tsv(name, header, rows):
        path = os.path.join(tmp, name + ".tsv")
        with open(path, "w", newline="") as f:
            w = _csv.writer(f, delimiter="\t", quoting=_csv.QUOTE_MINIMAL)
            w.writerow(header)
            for r in rows:
                w.writerow(["" if v is None else v for v in r])
        return path

    con = duckdb.connect()

    def _load(name, header, rows, columns):
        path = _tsv(name, header, rows)
        con.execute(
            f"CREATE TABLE {name} AS SELECT * FROM read_csv('{path}', delim='\t', "
            f"header=true, nullstr='', columns={columns})"
        )
        con.execute(f"COPY {name} TO '{out_dir}/{name}.parquet' (FORMAT PARQUET, COMPRESSION ZSTD)")

    _load("cells", ["sheet", "row", "col", "ref", "covered", "value_type", "value",
                    "boolean_value", "date_value", "string_value", "style_name",
                    "stored_formula", "text"], cell_rows,
          "{'sheet':'BIGINT','row':'BIGINT','col':'BIGINT','ref':'VARCHAR','covered':'BIGINT',"
          "'value_type':'VARCHAR','value':'VARCHAR','boolean_value':'VARCHAR','date_value':'VARCHAR',"
          "'string_value':'VARCHAR','style_name':'VARCHAR','stored_formula':'VARCHAR','text':'VARCHAR'}")
    _load("styles", ["scope", "name", "family", "parent", "data_style", "tcp_json", "tp_json"],
          style_rows,
          "{'scope':'VARCHAR','name':'VARCHAR','family':'VARCHAR','parent':'VARCHAR',"
          "'data_style':'VARCHAR','tcp_json':'VARCHAR','tp_json':'VARCHAR'}")
    _load("sheets", ["ord", "name", "display", "part", "part_ordinal"], sheet_rows,
          "{'ord':'BIGINT','name':'VARCHAR','display':'BIGINT','part':'VARCHAR','part_ordinal':'BIGINT'}")
    _load("named_expressions", ["name", "kind", "base_cell_address", "cell_range_address",
                                "expression", "sheet_name", "r0", "c0", "r1", "c1"], named_rows,
          "{'name':'VARCHAR','kind':'VARCHAR','base_cell_address':'VARCHAR',"
          "'cell_range_address':'VARCHAR','expression':'VARCHAR','sheet_name':'VARCHAR',"
          "'r0':'BIGINT','c0':'BIGINT','r1':'BIGINT','c1':'BIGINT'}")
    _load("named_refs", ["name", "sheet_name"], ref_rows,
          "{'name':'VARCHAR','sheet_name':'VARCHAR'}")
    _load("comments", ["sheet", "ref", "row", "col", "author", "date", "text"], comment_rows,
          "{'sheet':'BIGINT','ref':'VARCHAR','row':'BIGINT','col':'BIGINT','author':'VARCHAR',"
          "'date':'VARCHAR','text':'VARCHAR'}")

    con.execute("CREATE TABLE documents(source BLOB, source_len BIGINT, source_sha256 VARCHAR)")
    con.execute("INSERT INTO documents VALUES (?,?,?)", (blob, len(blob), _sha256(blob)))
    con.execute(f"COPY documents TO '{out_dir}/documents.parquet' (FORMAT PARQUET, COMPRESSION ZSTD)")
    con.close()
    shutil.rmtree(tmp, ignore_errors=True)
    return {
        "cells": len(cell_rows),
        "styles": len(style_rows),
        "sheets": len(sheet_rows),
        "named_expressions": len(named_rows),
        "comments": len(comment_rows),
        "source_len": len(blob),
    }


def _connect(out_dir):
    # Materialise each Parquet projection into an in-memory DuckDB table once per
    # process: this is a fair "maximise the competitor" warm path (the natural
    # columnar usage), while a cold process still pays the load.
    con = duckdb.connect()
    for t in ("cells", "styles", "sheets", "named_expressions", "named_refs", "comments", "documents"):
        p = os.path.join(out_dir, t + ".parquet")
        if os.path.exists(p):
            con.execute(f"CREATE TABLE {t} AS SELECT * FROM read_parquet('{p}')")
    return con


def _env(q, value, *, declined=False, code=None, reason="", native=True, detail=None):
    e = {"q": q, "lane": "duckdb", "declined": bool(declined), "native": native,
         "value": None if declined else value, "detail": detail or {}}
    e["decline"] = {"code": code, "detail": reason} if declined else None
    return e


def q_answer(con, q, plan):
    base = _load_baseline()
    sheet = int(plan.get("sheet", 0))
    cell = plan.get("cell")
    dep = plan.get("dep")
    q4cell = plan.get("q4cell", cell)

    if q == "Q1":
        r = con.execute(
            "SELECT value_type, value, boolean_value, date_value, string_value, text "
            "FROM cells WHERE sheet=? AND ref=?", (sheet, cell)
        ).fetchone()
        if r is None:
            return _env(q, None, declined=True, code="no-such-cell", reason=f"{cell} not on sheet {sheet}")
        c = {"value_type": r[0], "value": r[1], "boolean_value": r[2], "date_value": r[3],
             "string_value": r[4], "text": r[5]}
        return _env(q, {"type": r[0], "value": base._typed_value(c)}, detail={"ref": cell})
    if q == "Q2":
        r = con.execute("SELECT stored_formula FROM cells WHERE sheet=? AND ref=?", (sheet, cell)).fetchone()
        if r is None:
            return _env(q, None, declined=True, code="no-such-cell", reason=f"{cell} not on sheet {sheet}")
        return _env(q, r[0], detail={"ref": cell})
    if q == "Q3":
        if dep is None:
            return _env(q, None, declined=True, code="no-target", reason="no dependency target")
        rows = con.execute(
            "SELECT ref, stored_formula FROM cells WHERE sheet=? AND stored_formula IS NOT NULL",
            (sheet,),
        ).fetchall()
        pat = re.compile(r"(?<![A-Za-z0-9_])" + re.escape(dep) + r"(?![0-9])")
        found = sorted(r[0] for r in rows if pat.search(r[1]))
        return _env(q, found, detail={"method": "lexical", "target": dep})
    if q == "Q4":
        rc = base.parse_cell_position(q4cell) if q4cell else None
        if rc is None:
            return _env(q, None, declined=True, code="no-cell", reason="no cell")
        col, row = rc
        srow = con.execute("SELECT name FROM sheets WHERE ord=?", (sheet,)).fetchone()
        if srow is None:
            return _env(q, None, declined=True, code="no-such-sheet", reason=f"no sheet {sheet}")
        rows = con.execute(
            "SELECT name FROM named_expressions WHERE kind='range' AND sheet_name=? "
            "AND r0 IS NOT NULL AND r0<=? AND r1>=? AND c0<=? AND c1>=?",
            (srow[0], row, row, col, col),
        ).fetchall()
        return _env(q, sorted(r[0] for r in rows), detail={"cell": q4cell})
    if q == "Q5":
        r = con.execute(
            "SELECT c.style_name, s.scope, s.name, s.family, s.parent, s.data_style, s.tcp_json, s.tp_json "
            "FROM cells c LEFT JOIN styles s ON s.name=c.style_name "
            "WHERE c.sheet=? AND c.ref=? "
            "ORDER BY CASE s.scope WHEN 'auto' THEN 0 ELSE 1 END LIMIT 1",
            (sheet, cell),
        ).fetchone()
        if r is None:
            return _env(q, None, declined=True, code="no-such-cell", reason=f"{cell} not on sheet {sheet}")
        if r[0] is None or r[2] is None:
            return _env(q, None, detail={"ref": cell, "style_name": r[0]})
        value = {
            "name": r[2], "family": r[3], "parent": r[4], "data_style": r[5],
            "table_cell_properties": json.loads(r[6]),
            "text_properties": json.loads(r[7]),
        }
        return _env(q, value, detail={"ref": cell, "style_name": r[0]})
    if q == "Q6":
        r = con.execute("SELECT part, name FROM sheets WHERE ord=?", (sheet,)).fetchone()
        if r is None:
            return _env(q, None, declined=True, code="no-such-sheet", reason=f"no sheet {sheet}")
        return _env(q, r[0], detail={"sheet": r[1]})
    if q == "Q7":
        srow = con.execute("SELECT name FROM sheets WHERE ord=?", (sheet,)).fetchone()
        if srow is None:
            return _env(q, None, declined=True, code="no-such-sheet", reason=f"no sheet {sheet}")
        rows = con.execute(
            "SELECT DISTINCT name FROM named_refs WHERE sheet_name=?", (srow[0],)
        ).fetchall()
        return _env(q, sorted(r[0] for r in rows), detail={"sheet": srow[0]})
    if q == "Q8":
        return _env(q, None, declined=True, code="no-exact-span",
                    reason="the exact OpenDocument cell span is not in the columnar projection")
    if q == "Q9":
        return _env(q, None, declined=True, code="no-embedded-resource",
                    reason="no embedded member bytes are kept in the columnar projection")
    if q == "Q10":
        r = con.execute("SELECT source_len, source_sha256 FROM documents").fetchone()
        return _env(
            q, None, declined=True, code="not-native", native=False,
            reason="columnar projection has no exact-source closure; the retained source blob "
                   "is a labelled passthrough and is NOT counted as native capability",
            detail={"stored_blob_len": r[0], "stored_blob_sha256": r[1]} if r else {},
        )
    return _env(q, None, declined=True, code="unknown-question", reason=q)


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

    ns = ap.parse_args(argv)

    if ns.cmd == "build":
        m = build(ns.source, ns.dir)
        if ns.metrics:
            with open(ns.metrics, "w") as f:
                json.dump(m, f, sort_keys=True)
        print(json.dumps(m, sort_keys=True))
        return 0
    if ns.cmd == "query":
        plan = json.loads(ns.plan)
        con = _connect(ns.dir)
        env = q_answer(con, ns.q, plan)
        con.close()
        with open(ns.out, "w") as f:
            json.dump(env, f, sort_keys=True)
        print(json.dumps({"q": ns.q, "declined": env["declined"]}))
        return 0
    if ns.cmd == "session":
        plan = json.loads(ns.plan)
        qs = [x for x in ns.queries.split(",") if x]
        con = _connect(ns.dir)
        batch = []
        for one in qs:
            t0 = now_us()
            env = q_answer(con, one, plan)
            us = now_us() - t0
            batch.append({"q": one, "us": us, "env": env})
        con.close()
        with open(ns.out, "w") as f:
            json.dump({"batch": batch}, f, sort_keys=True)
        print(json.dumps({"observations": len(batch)}))
        return 0
    return 2


if __name__ == "__main__":
    sys.exit(main())
