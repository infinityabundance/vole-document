#!/usr/bin/env python3
# Phase 21.1.3 — XLSX economic court: the DuckDB/Parquet **analytical comparator**.
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
# DuckDB answers **columnar/tabular** questions (cell value, stored formula,
# containing table, cell style, worksheet part, lexical dependents) from a
# Parquet projection with SQL. It does **not** provide exact-source closure or
# per-answer provenance, and its projection is lossy:
#
#   * Q7 (chart -> table linkage): typed DECLINE (no chart data in the columnar
#     model).
#   * Q8 (exact XML span of a cell): typed DECLINE (the span lives in the
#     SpreadsheetML member, not in a column).
#   * Q9 (embedded resource bytes): typed DECLINE (no embedded members are kept
#     in the columnar model).
#   * Q10 (exact original bytes): typed DECLINE as `not-native` — the lane keeps
#     the source bytes only as a **labelled passthrough** so its storage is
#     source-retaining and comparable; that passthrough is explicitly NOT counted
#     as a native columnar capability.
#
# The extraction is shared byte-for-byte with the SQLite baseline
# (`phase21-3-xlsx-baseline.py:extract`) so the only difference between the two
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
    path = os.path.join(here, "phase21-3-xlsx-baseline.py")
    spec = importlib.util.spec_from_file_location("phase21_3_xlsx_baseline", path)
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
    for si, s in enumerate(doc["sheets"]):
        for c in s.get("cells", []):
            cell_rows.append((si, c["row"], c["col"], c["ref"], c["type"], c["formula"], c["value"], c["style"]))
    styles = doc.get("styles") or {"num_fmts": {}, "fonts": [], "fills": [], "cell_xfs": []}
    style_rows = []
    for xid, x in enumerate(styles["cell_xfs"]):
        if x["num_fmt_id"] in styles["num_fmts"]:
            code = styles["num_fmts"][x["num_fmt_id"]]
        else:
            code = base.BUILTIN_FORMATS.get(x["num_fmt_id"])
        font = styles["fonts"][x["font_id"]] if x["font_id"] < len(styles["fonts"]) else {}
        fill = styles["fills"][x["fill_id"]] if x["fill_id"] < len(styles["fills"]) else {}
        a = x.get("alignment") or {}
        style_rows.append((
            xid, x["num_fmt_id"], code,
            int(bool(font.get("bold"))), int(bool(font.get("italic"))), font.get("size"), font.get("name"),
            fill.get("pattern_type"), fill.get("fg"), fill.get("bg"),
            a.get("horizontal"), a.get("vertical"), int(bool(a.get("wrap_text"))) if a else None,
        ))
    sheet_rows = [(si, s["name"], s["state"], s.get("part")) for si, s in enumerate(doc["sheets"])]
    table_rows = []
    for t in doc["tables"]:
        r = t.get("rect") or (None, None, None, None)
        table_rows.append((t["sheet"], t["name"], t["ref"], r[0], r[1], r[2], r[3]))
    merge_rows = [(m["sheet"], m["ref"]) for m in doc["merges"]]

    with open(source, "rb") as f:
        blob = f.read()

    # Bulk-load each table from a temporary TSV and let DuckDB's CSV reader
    # build it in one pass (row-by-row `executemany` would measure this driver's
    # Python loop, not DuckDB). Empty fields are NULL (`nullstr=''`).
    import shutil
    tmp = os.path.join(out_dir, ".ingest")
    os.makedirs(tmp, exist_ok=True)

    def _tsv(name, header, rows):
        path = os.path.join(tmp, name + ".tsv")
        with open(path, "w", newline="") as f:
            w = _csv.writer(f, delimiter="\t", quoting=_csv.QUOTE_NONE)
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

    _load("cells", ["sheet", "row", "col", "ref", "type", "formula", "cached_value", "style_id"], cell_rows,
          "{'sheet':'BIGINT','row':'BIGINT','col':'BIGINT','ref':'VARCHAR','type':'VARCHAR',"
          "'formula':'VARCHAR','cached_value':'VARCHAR','style_id':'BIGINT'}")
    _load("styles", ["style_id", "num_fmt_id", "format_code", "bold", "italic", "size", "name",
                     "pattern_type", "fg", "bg", "horizontal", "vertical", "wrap_text"], style_rows,
          "{'style_id':'BIGINT','num_fmt_id':'BIGINT','format_code':'VARCHAR','bold':'BIGINT',"
          "'italic':'BIGINT','size':'VARCHAR','name':'VARCHAR','pattern_type':'VARCHAR','fg':'VARCHAR',"
          "'bg':'VARCHAR','horizontal':'VARCHAR','vertical':'VARCHAR','wrap_text':'BIGINT'}")
    _load("sheets", ["ord", "name", "state", "part"], sheet_rows,
          "{'ord':'BIGINT','name':'VARCHAR','state':'VARCHAR','part':'VARCHAR'}")
    _load("tables", ["sheet", "name", "ref", "r0", "c0", "r1", "c1"], table_rows,
          "{'sheet':'BIGINT','name':'VARCHAR','ref':'VARCHAR','r0':'BIGINT','c0':'BIGINT',"
          "'r1':'BIGINT','c1':'BIGINT'}")
    _load("merges", ["sheet", "ref"], merge_rows, "{'sheet':'BIGINT','ref':'VARCHAR'}")

    con.execute("CREATE TABLE documents(source BLOB, source_len BIGINT, source_sha256 VARCHAR)")
    con.execute("INSERT INTO documents VALUES (?,?,?)", (blob, len(blob), _sha256(blob)))
    con.execute(f"COPY documents TO '{out_dir}/documents.parquet' (FORMAT PARQUET, COMPRESSION ZSTD)")
    con.close()
    shutil.rmtree(tmp, ignore_errors=True)
    return {
        "cells": len(cell_rows),
        "styles": len(style_rows),
        "sheets": len(sheet_rows),
        "tables": len(table_rows),
        "source_len": len(blob),
    }


def _connect(out_dir):
    # Materialise each Parquet projection into an in-memory DuckDB table once per
    # process: this is a fair "maximise the competitor" warm path (the natural
    # columnar usage), while a cold process still pays the load.
    con = duckdb.connect()
    for t in ("cells", "styles", "sheets", "tables", "merges", "documents"):
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
    sheet = int(plan.get("sheet", 0))
    cell = plan.get("cell")
    dep = plan.get("dep")
    q4cell = plan.get("q4cell", cell)

    if q == "Q1":
        r = con.execute("SELECT cached_value FROM cells WHERE sheet=? AND ref=?", (sheet, cell)).fetchone()
        if r is None:
            return _env(q, None, declined=True, code="no-such-cell", reason=f"{cell} not on sheet {sheet}")
        return _env(q, r[0], detail={"ref": cell})
    if q == "Q2":
        r = con.execute("SELECT formula FROM cells WHERE sheet=? AND ref=?", (sheet, cell)).fetchone()
        if r is None:
            return _env(q, None, declined=True, code="no-such-cell", reason=f"{cell} not on sheet {sheet}")
        return _env(q, r[0], detail={"ref": cell})
    if q == "Q3":
        if dep is None:
            return _env(q, None, declined=True, code="no-target", reason="no dependency target")
        rows = con.execute(
            "SELECT ref, formula FROM cells WHERE sheet=? AND formula IS NOT NULL", (sheet,)
        ).fetchall()
        pat = re.compile(r"(?<![A-Za-z0-9_$])" + re.escape(dep) + r"(?![0-9])")
        found = sorted(r[0] for r in rows if pat.search(r[1]))
        return _env(q, found, detail={"method": "lexical", "target": dep})
    if q == "Q4":
        rc = _load_baseline().a1_to_rowcol(q4cell) if q4cell else None
        if rc is None:
            return _env(q, None, declined=True, code="no-cell", reason="no cell")
        rows = con.execute(
            "SELECT name FROM tables WHERE sheet=? AND r0 IS NOT NULL AND r0<=? AND r1>=? AND c0<=? AND c1>=?",
            (sheet, rc[0], rc[0], rc[1], rc[1]),
        ).fetchall()
        return _env(q, sorted(r[0] for r in rows), detail={"cell": q4cell})
    if q == "Q5":
        r = con.execute(
            "SELECT c.style_id, s.num_fmt_id, s.format_code, s.bold, s.italic, s.size, s.name, "
            "s.pattern_type, s.fg, s.bg, s.horizontal, s.vertical, s.wrap_text "
            "FROM cells c LEFT JOIN styles s ON s.style_id=c.style_id WHERE c.sheet=? AND c.ref=?",
            (sheet, cell),
        ).fetchone()
        if r is None:
            return _env(q, None, declined=True, code="no-such-cell", reason=f"{cell} not on sheet {sheet}")
        if r[0] is None:
            return _env(q, None, detail={"ref": cell, "style_index": None})
        value = {
            "index": r[0], "numFmtId": r[1], "formatCode": r[2],
            "font": {"bold": bool(r[3]), "italic": bool(r[4]), "size": r[5], "name": r[6]},
            "fill": {"patternType": r[7], "fgColor": r[8], "bgColor": r[9]},
            "alignment": ({"horizontal": r[10], "vertical": r[11], "wrapText": bool(r[12])}
                          if (r[10] or r[11] or r[12]) else None),
        }
        return _env(q, value, detail={"ref": cell, "style_index": r[0]})
    if q == "Q6":
        r = con.execute("SELECT part, name, state FROM sheets WHERE ord=?", (sheet,)).fetchone()
        if r is None:
            return _env(q, None, declined=True, code="no-such-sheet", reason=f"no sheet {sheet}")
        return _env(q, r[0], detail={"sheet": r[1], "state": r[2]})
    if q == "Q7":
        return _env(q, None, declined=True, code="no-chart-linkage",
                    reason="the columnar projection carries no chart or chart data reference")
    if q == "Q8":
        return _env(q, None, declined=True, code="no-exact-span",
                    reason="the exact SpreadsheetML cell span is not in the columnar projection")
    if q == "Q9":
        return _env(q, None, declined=True, code="no-embedded-resource",
                    reason="no embedded package member bytes are kept in the columnar projection")
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
