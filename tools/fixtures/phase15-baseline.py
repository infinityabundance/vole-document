#!/usr/bin/env python3
# Phase 15.6 — SQLite lanes for the diversity + revision courts (stdlib + sqlite3).
#
# Three honest, separately-labelled lanes over the SAME per-op cost model so the
# comparison is apples-to-apples:
#
#   min    Minimal materialization: a SQLite DB that retains ONLY the source blob
#          and precomputes nothing. Every request re-parses the source with the
#          frozen Phase-12 extractors (`phase12-baseline.py`). This is the
#          "compute from source, no precompute" pole. (It is deliberately as
#          expensive as A0 — that is what minimal means; the DB's only role is to
#          retain the bytes.)
#
#   adapt  Minimal + an online, governed `result_cache`: on the FIRST request for
#          a key the answer is computed from source and the answer is
#          materialized; later identical requests are served from the cache.
#          Budget-bounded with LRU eviction by `last_seen`. This is the crucial
#          competitor — it generalizes "materialize a view the first time it is
#          requested" and is the strongest fair online baseline.
#
#   full   Every supported view materialized eagerly: reuses the frozen
#          `phase12-baseline.py build` schema (leaves + FTS5) verbatim. Queries
#          are plain indexed SQL.
#
# The driver runs the WHOLE per-depth prefix in one process and reports the
# total; the court wraps it in `/usr/bin/time -v` for CPU + peak RSS and charges
# the build separately. Persistent bytes are the DB (+ wal/shm). Physical reads
# are read from `/proc/self/io` (`read_bytes`) — a *separate* universe from
# VOLE's logical `bytes_read`, never compared raw (ADR-0027 §Correction).
#
# Usage:
#   phase15-baseline.py min-build  --source S --db D
#   phase15-baseline.py run        --lane min|adapt|full --format F --source S \
#                                  --db D --requests R.json --out M.json \
#                                  [--budget N]

import argparse
import importlib.util
import json
import os
import sqlite3
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))

MIN_SCHEMA = """
PRAGMA journal_mode=WAL;
PRAGMA synchronous=NORMAL;
CREATE TABLE documents(
  doc_id INTEGER PRIMARY KEY, format TEXT NOT NULL, path TEXT NOT NULL);
CREATE TABLE source_blob(
  doc_id INTEGER PRIMARY KEY, payload BLOB NOT NULL);
CREATE TABLE result_cache(
  key TEXT PRIMARY KEY, kind TEXT, answer BLOB, hits INTEGER, bytes INTEGER,
  cost_units INTEGER, last_seen INTEGER);
"""

# composite weight (a table/heading answer costs the whole document parse, but
# the extraction time is measured, so a flat weight is honest).
KIND_WEIGHT = {"text": 1, "metadata": 1, "heading": 1, "table": 1, "resource": 1}


def load_phase12():
    spec = importlib.util.spec_from_file_location(
        "phase12_baseline", os.path.join(HERE, "phase12-baseline.py"))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def phys_read_bytes():
    try:
        with open("/proc/self/io") as fh:
            for ln in fh:
                if ln.startswith("read_bytes:"):
                    return int(ln.split()[1])
    except OSError:
        pass
    return None


def db_bytes(path):
    total = 0
    for suffix in ("", "-wal", "-shm"):
        try:
            total += os.path.getsize(path + suffix)
        except OSError:
            pass
    return total


def answer_from_doc(p12, doc, fmt, t, i):
    """Map a (type, index) request onto the frozen Phase-12 `answer_for` cases."""
    case = {"text": "block", "heading": "heading", "table": "table",
            "resource": "resource-meta", "metadata": "metadata"}[t]
    arg = "" if t == "metadata" else str(i)
    payload = p12.answer_for(doc, case, arg)
    if payload is None:
        payload = {"text": None}
    return json.dumps(payload, sort_keys=True).encode()


def full_sql(fmt, t, i):
    """The indexed SQL for the eagerly-materialized lane (mirrors the A1 lane)."""
    if t == "text":
        if fmt == "pdf":
            return f"SELECT text FROM blocks WHERE doc_id=1 AND unit='page' AND ordering={i + 1};"
        return f"SELECT text FROM blocks WHERE doc_id=1 AND block_id={i + 1};"
    if t == "heading":
        return (f"SELECT b.text FROM blocks b JOIN headings h ON h.block_id=b.block_id "
                f"WHERE h.doc_id=1 ORDER BY h.heading_id LIMIT 1 OFFSET {i};")
    if t == "table":
        return (f"SELECT text FROM blocks WHERE doc_id=1 AND kind='table' "
                f"ORDER BY block_id LIMIT 1 OFFSET {i};")
    if t == "resource":
        return (f"SELECT path||' '||member_bytes FROM resources "
                f"WHERE doc_id=1 AND resource_id={i + 1};")
    if t == "metadata":
        return "SELECT group_concat(key||'='||value,';') FROM metadata WHERE doc_id=1;"
    raise SystemExit(f"unknown type {t!r}")


def run_min(p12, fmt, source, reqs):
    wall = []
    for r in reqs:
        t0 = time.perf_counter()
        doc = p12.extract(fmt, source)
        answer_from_doc(p12, doc, fmt, r["type"], r["i"])
        wall.append((time.perf_counter() - t0) * 1e6)
    return wall, 0, 0


def run_adapt(p12, fmt, source, reqs, db, budget):
    con = sqlite3.connect(db)
    clock = 0
    cache_bytes = 0
    entries = 0
    wall = []
    for r in reqs:
        clock += 1
        t0 = time.perf_counter()
        key = f"{r['type']}:{r['i']}"
        row = con.execute("SELECT answer, bytes FROM result_cache WHERE key=?",
                          (key,)).fetchone()
        if row is not None:
            con.execute("UPDATE result_cache SET hits=hits+1, last_seen=? WHERE key=?",
                        (clock, key))
        else:
            doc = p12.extract(fmt, source)
            ans = answer_from_doc(p12, doc, fmt, r["type"], r["i"])
            micros = int((time.perf_counter() - t0) * 1e6)
            need = len(ans)
            if need <= budget:
                used = cache_bytes
                if used + need > budget:  # LRU evict until it fits
                    for (vkey, vbytes) in con.execute(
                            "SELECT key, bytes FROM result_cache ORDER BY last_seen"):
                        if used + need <= budget:
                            break
                        con.execute("DELETE FROM result_cache WHERE key=?", (vkey,))
                        used -= vbytes
                con.execute(
                    "INSERT OR REPLACE INTO result_cache VALUES(?,?,?,?,?,?,?)",
                    (key, r["type"], ans, 0, need,
                     KIND_WEIGHT.get(r["type"], 1) * need + micros, clock))
                cache_bytes = used + need
        con.commit()
        wall.append((time.perf_counter() - t0) * 1e6)
    entries = con.execute("SELECT COUNT(*), COALESCE(SUM(bytes),0) FROM result_cache").fetchone()
    con.close()
    return wall, entries[0], entries[1]


def run_full(fmt, reqs, db):
    con = sqlite3.connect(db)
    wall = []
    for r in reqs:
        t0 = time.perf_counter()
        con.execute(full_sql(fmt, r["type"], r["i"])).fetchall()
        wall.append((time.perf_counter() - t0) * 1e6)
    con.close()
    return wall, 0, 0


def main(argv=None):
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)

    b = sub.add_parser("min-build")
    b.add_argument("--source", required=True)
    b.add_argument("--db", required=True)

    r = sub.add_parser("run")
    r.add_argument("--lane", required=True, choices=["min", "adapt", "full"])
    r.add_argument("--format", required=True)
    r.add_argument("--source", required=True)
    r.add_argument("--db", required=True)
    r.add_argument("--requests", required=True)
    r.add_argument("--out", required=True)
    r.add_argument("--budget", type=int, default=1 << 20)

    a = ap.parse_args(argv)

    if a.cmd == "min-build":
        try:
            os.remove(a.db)
        except FileNotFoundError:
            pass
        con = sqlite3.connect(a.db)
        con.executescript(MIN_SCHEMA)
        with open(a.source, "rb") as fh:
            payload = fh.read()
        con.execute("INSERT OR REPLACE INTO documents VALUES(1,?,?)",
                    (os.path.splitext(a.source)[1].lstrip("."), a.source))
        con.execute("INSERT OR REPLACE INTO source_blob VALUES(1,?)", (payload,))
        con.commit()
        con.close()
        print(json.dumps({"db": a.db, "db_bytes": db_bytes(a.db)}))
        return 0

    p12 = load_phase12()
    with open(a.requests) as fh:
        reqs = json.load(fh)

    before = phys_read_bytes()
    t0 = time.perf_counter()
    if a.lane == "min":
        wall, entries, cbytes = run_min(p12, a.format, a.source, reqs)
    elif a.lane == "adapt":
        wall, entries, cbytes = run_adapt(p12, a.format, a.source, reqs, a.db, a.budget)
    else:
        wall, entries, cbytes = run_full(a.format, reqs, a.db)
    total_us = (time.perf_counter() - t0) * 1e6
    after = phys_read_bytes()

    metrics = {
        "lane": a.lane,
        "requests": len(reqs),
        "wall_us_total": total_us,
        "wall_us_max": max(wall) if wall else 0.0,
        "wall_us_per_request": [round(x, 2) for x in wall],
        "db_bytes": db_bytes(a.db),
        "cache_entries": entries,
        "cache_bytes": cbytes,
        "phys_read_bytes": (after - before) if (after is not None and before is not None) else None,
    }
    with open(a.out, "w") as fh:
        json.dump(metrics, fh, sort_keys=True)
    print(json.dumps(metrics, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
