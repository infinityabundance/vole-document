#!/usr/bin/env python3
# Phase 21.16 — Arrow IPC economic court: the source-retaining SQLite baseline.
#
# The comparator retains the original **Arrow** bytes verbatim (a `raw` BLOB), so it
# can answer the byte-authority (exact-closure) question. SQLite has no Arrow reader,
# so a conventional relational load is performed by DuckDB reading a **Parquet
# projection** of the same logical table (documented, never hidden): the SQLite lane
# then measures the **row-store query cost**, not the Arrow decode cost. It cannot
# answer the Arrow-metadata questions (a column's exact buffer span and min/max, the
# record-batch count) — those decline typed.
#
#   build       --source ARROW --table-source PARQUET --db DB
#   query       --db DB --q Qn --plan JSON --out FILE
#   session     --db DB --queries Q1,... --plan JSON --out FILE
#   materialize --db DB --out FILE
#   aggregate   --raw DIR --campaign DIR --env ENV_JSON

import argparse
import hashlib
import json
import os
import sqlite3
import sys
import time

import duckdb

Q3_MSG = "SQLite has no Arrow reader: a column's exact buffer span/statistics are not retained"
Q4_MSG = "SQLite has no Arrow reader: the record-batch count is not retained"


def now_us():
    return int(time.monotonic() * 1_000_000)


def envelope(q, value=None, *, declined=False, code=None, reason="", detail=None, native=True):
    e = {"q": q, "lane": "sqlite", "declined": bool(declined), "native": native,
         "value": None if declined else value, "detail": detail or {}}
    e["decline"] = {"code": code, "detail": reason} if declined else None
    return e


def _cell(v):
    return "null" if v is None else str(v)


def build(source, table_source, db):
    if os.path.exists(db):
        os.remove(db)
    raw = open(source, "rb").read()
    conn = sqlite3.connect(db)
    conn.execute("CREATE TABLE src(bytes BLOB)")
    conn.execute("INSERT INTO src VALUES (?)", (raw,))
    ncols, rows = _load(conn, table_source)
    conn.execute("CREATE TABLE meta(key TEXT, value TEXT)")
    conn.execute("INSERT INTO meta VALUES ('ncols', ?)", (str(ncols),))
    conn.execute("INSERT INTO meta VALUES ('nrows', ?)", (str(len(rows)),))
    conn.commit()
    conn.close()


def _load(conn, table_source):
    """A conventional relational load, using the available Parquet reader."""
    desc = duckdb.sql("DESCRIBE SELECT * FROM read_parquet('%s')" % table_source).fetchall()
    ncols = len(desc)
    rows = duckdb.sql("SELECT * FROM read_parquet('%s')" % table_source).fetchall()
    cols = ",".join("c%d" % i for i in range(ncols))
    conn.execute("CREATE TABLE t(%s)" % cols)
    ph = ",".join("?" for _ in range(ncols))
    conn.executemany("INSERT INTO t VALUES (%s)" % ph, rows)
    return ncols, rows


def query(db, q, plan):
    conn = sqlite3.connect(db)
    try:
        if q == "Q1":
            rows = conn.execute("SELECT c%d FROM t" % plan["col_str"]).fetchall()
            return {"value": [_cell(r[0]) for r in rows]}
        if q == "Q2":
            rows = conn.execute("SELECT count(*) FROM t WHERE c%d > ?" % plan["col_num"],
                                (plan["gt"],)).fetchall()
            return {"value": int(rows[0][0])}
        if q == "Q3":
            return envelope(q, declined=True, code="not-retained", reason=Q3_MSG, native=False)
        if q == "Q4":
            return envelope(q, declined=True, code="not-retained", reason=Q4_MSG, native=False)
        if q == "Q5":
            rows = conn.execute("SELECT count(*) FROM t WHERE c%d LIKE ?" % plan["col_str"],
                                ("%" + plan["substr"] + "%",)).fetchall()
            return {"value": int(rows[0][0])}
        if q == "Q6":
            row = conn.execute("SELECT bytes FROM src").fetchone()
            b = bytes(row[0])
            return {"value": {"length": len(b), "sha256": hashlib.sha256(b).hexdigest()}}
        raise RuntimeError("unknown question %s" % q)
    finally:
        conn.close()


QS = ["Q1", "Q2", "Q3", "Q4", "Q5", "Q6"]


def materialize(db, out):
    conn = sqlite3.connect(db)
    row = conn.execute("SELECT bytes FROM src").fetchone()
    conn.close()
    with open(out, "wb") as f:
        f.write(bytes(row[0]))


# --- aggregate --------------------------------------------------------------

def read_tsv(path):
    if not os.path.exists(path):
        return []
    with open(path) as f:
        lines = f.read().splitlines()
    if not lines:
        return []
    head = lines[0].split("\t")
    out = []
    for ln in lines[1:]:
        if ln == "":
            continue
        parts = ln.split("\t")
        out.append(dict(zip(head, parts)))
    return out


def _load_p19():
    import importlib.util
    here = os.path.dirname(os.path.abspath(__file__))
    path = os.path.join(here, "phase19-repeat.py")
    spec = importlib.util.spec_from_file_location("phase19_repeat", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


TIE = 0.10


def _envelope_value(path):
    if not os.path.exists(path):
        return None
    with open(path) as f:
        return json.load(f)


def compare(q, va, vb):
    if va is None or vb is None:
        return "missing", "no envelope"
    if va.get("declined") and vb.get("declined"):
        return "both-decline", "both typed declines"
    if va.get("declined") or vb.get("declined"):
        who = "sqlite/duckdb" if va.get("declined") else "vole"
        return "capability-gap", "%s declines" % who
    a, b = va.get("value"), vb.get("value")
    if q == "Q3":
        # The exact span convention can differ between readers; min/max statistics
        # are the byte-authority-free comparison, so compare those (recorded).
        a = {"min": a.get("min"), "max": a.get("max")}
        b = {"min": b.get("min"), "max": b.get("max")}
    if q == "Q6":
        if isinstance(a, dict) and isinstance(b, dict):
            ok = a.get("sha256") == b.get("sha256") and a.get("length") == b.get("length")
            return ("equal" if ok else "mismatch"), "length+sha256"
    return ("equal" if a == b else "mismatch"), "value"


QDESC = {
    "Q1": "a column's decoded values",
    "Q2": "a predicate/filtered count (numeric column > K)",
    "Q3": "a column's exact buffer span + min/max statistics",
    "Q4": "the record-batch count",
    "Q5": "a lexical find (rows in a string column containing S)",
    "Q6": "`materialize --exact` (byte-authority)",
}


def aggregate(raw, campaign, env_path=None):
    P19 = _load_p19()
    B = 10000
    SEED = 21616
    import statistics

    env = {}
    if env_path and os.path.exists(env_path):
        with open(env_path) as f:
            try:
                env = json.load(f)
            except ValueError:
                env = {}

    fixtures = [r["fixture"] for r in read_tsv(os.path.join(raw, "fixtures.tsv"))]
    lanes = ["vole", "sqlite", "duckdb"]

    build_rows = read_tsv(os.path.join(raw, "build.tsv"))
    storage_rows = read_tsv(os.path.join(raw, "storage.tsv"))
    cold_rows = read_tsv(os.path.join(raw, "cold.tsv"))
    warm_rows = read_tsv(os.path.join(raw, "warm.tsv"))
    exact_rows = read_tsv(os.path.join(raw, "exact.tsv"))

    def best_by_fixture(rows, lane):
        out = {}
        for r in rows:
            if r["lane"] != lane or r.get("rc") != "0":
                continue
            fx, v = r["fixture"], int(r["us"])
            if fx not in out or v < out[fx]:
                out[fx] = v
        return out

    def cold_by_fixture(lane):
        perrep = {}
        for r in cold_rows:
            if r["lane"] != lane or r.get("rc") != "0":
                continue
            key = (r["fixture"], r["rep"])
            perrep[key] = perrep.get(key, 0) + int(r["us"])
        out = {}
        for (fx, _), v in perrep.items():
            if fx not in out or v < out[fx]:
                out[fx] = v
        return out

    build_us = {lane: best_by_fixture(build_rows, lane) for lane in lanes}
    store_bytes = {lane: {} for lane in lanes}
    for r in storage_rows:
        store_bytes[r["lane"]][r["fixture"]] = int(r["bytes"])
    cold_us = {lane: cold_by_fixture(lane) for lane in lanes}
    warm_us = {lane: {} for lane in lanes}
    warm_perrep = {lane: {} for lane in lanes}
    for r in warm_rows:
        if r.get("rc") != "0":
            continue
        lane = r["lane"]
        key = (r["fixture"], r["rep"])
        warm_perrep[lane][key] = warm_perrep[lane].get(key, 0) + int(r["us"])
    for lane in lanes:
        for (fx, _), v in warm_perrep[lane].items():
            if fx not in warm_us[lane] or v < warm_us[lane][fx]:
                warm_us[lane][fx] = v

    # --- cross-lane answer agreement ---------------------------------------
    qcompare = {}
    for q in QS:
        counts = {}
        for other in ("sqlite", "duckdb"):
            n_equal = n_gap = n_both = n_mismatch = 0
            for fx in fixtures:
                va = _envelope_value(os.path.join(raw, "qanswers", "%s.%s.vole.json" % (fx, q)))
                vb = _envelope_value(os.path.join(raw, "qanswers", "%s.%s.%s.json" % (fx, q, other)))
                if va is None or vb is None:
                    continue
                status, _ = compare(q, va, vb)
                if status == "equal":
                    n_equal += 1
                elif status == "capability-gap":
                    n_gap += 1
                elif status == "both-decline":
                    n_both += 1
                elif status == "mismatch":
                    n_mismatch += 1
            counts[other] = {"equal": n_equal, "capability_gap": n_gap,
                             "both_decline": n_both, "mismatch": n_mismatch}
        qcompare[q] = counts

    exact_ok = sum(1 for r in exact_rows if r.get("vole_ok") == "true")
    exact_n = len(exact_rows)

    # --- MATRIX.md ----------------------------------------------------------
    lines = []
    lines.append("# Phase 21.16 Arrow IPC economic court — matrix")
    lines.append("")
    lines.append("Self-authored corpus; only **Q6 (exact closure)** is a byte-authority claim.")
    lines.append("Ratios are VOLE / comparator; **< 1 favours VOLE**. Wall time is microseconds.")
    lines.append("")
    lines.append("## Exactness (VOLE, after source + descriptor deletion)")
    lines.append("")
    lines.append("| fixture | vole_ok | len_ok | sha_ok | cmp_ok |")
    lines.append("|---|---|---|---|---|")
    for r in exact_rows:
        lines.append("| `%s` | %s | %s | %s | %s |" % (
            r["fixture"], r.get("vole_ok"), r.get("len_ok"), r.get("sha_ok"), r.get("cmp_ok")))
    lines.append("")
    lines.append("## Cross-lane answer agreement (per fixture, VOLE vs comparator)")
    lines.append("")
    lines.append("| Q | question | vs sqlite (equal / gap / both-decline / mismatch) | "
                 "vs duckdb (equal / gap / both-decline / mismatch) |")
    lines.append("|---|---|---|---|")
    for q in QS:
        c = qcompare[q]
        lines.append("| %s | %s | %d / %d / %d / %d | %d / %d / %d / %d |" % (
            q, QDESC[q],
            c["sqlite"]["equal"], c["sqlite"]["capability_gap"], c["sqlite"]["both_decline"],
            c["sqlite"]["mismatch"],
            c["duckdb"]["equal"], c["duckdb"]["capability_gap"], c["duckdb"]["both_decline"],
            c["duckdb"]["mismatch"]))
    lines.append("")
    lines.append("## Paired ratios VOLE/other (median + geometric mean, 95% CI by fixture)")
    lines.append("")
    lines.append("| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | "
                 "wins | ties | losses | ratio of sums |")
    lines.append("|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|")
    for metric, key in (("build", build_us), ("storage", store_bytes), ("cold", cold_us), ("warm", warm_us)):
        for other in ("sqlite", "duckdb"):
            ratios = {}
            sums_v = sums_o = 0
            for fx in fixtures:
                v = key["vole"].get(fx)
                o = key[other].get(fx)
                if not v or not o:
                    continue
                ratios[fx] = v / o
                sums_v += v
                sums_o += o
            if not ratios:
                continue
            vals = list(ratios.values())
            bydoc = {fx: [rt] for fx, rt in ratios.items()}
            lo_m, hi_m, _ = P19.cluster_bootstrap(bydoc, statistics.median, B, SEED)
            lo_g, hi_g, _ = P19.cluster_bootstrap(bydoc, P19.geomean, B, SEED + 1)
            wins = sum(1 for x in vals if x < 1 - TIE)
            ties = sum(1 for x in vals if 1 - TIE <= x <= 1 + TIE)
            losses = len(vals) - wins - ties
            lines.append("| %s | %s | %d | %.3f | %.3f | %.3f..%.3f | %.3f..%.3f | %d | %d | %d | %.3f |"
                         % (metric, other, len(vals), statistics.median(vals), P19.geomean(vals),
                            lo_m, hi_m, lo_g, hi_g, wins, ties, losses,
                            (sums_v / sums_o if sums_o else 0)))
    lines.append("")
    lines.append("A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, "
                 "summarised by the median and geometric mean with a fixed-seed cluster bootstrap "
                 "over fixtures; `ratio of sums` (a size-weighted pooled view) is reported "
                 "separately. With only %d fixture clusters the bootstrap is coarse and is stated "
                 "as such." % len(fixtures))
    lines.append("")
    lines.append("**Read `cold` with care.** VOLE's `cold` is a single in-process observation "
                 "(the driver excludes its own Python startup), while the SQLite and DuckDB "
                 "`cold` samples include a fresh-process Python + `duckdb` import (tens of "
                 "milliseconds). `warm` and `build`/`storage` are the fairer comparisons; the "
                 "`cold` ratio is inflated by process startup and is retained only for completeness.")
    lines.append("")
    lines.append("**Recorded plainly:** DuckDB wins the analytical axes (columnar projection, "
                 "predicate execution, metadata statistics) — but note that the DuckDB lane reads "
                 "a **Parquet projection** of the same logical table, because the pinned DuckDB "
                 "wheel exposes only the Arrow **C data interface** scanners (`arrow_scan*`), not "
                 "an Arrow IPC file reader. VOLE is an archival field with typed observations, not "
                 "a query engine; its predicate/find counts are a decoded column plus a count in "
                 "the driver. Only Q6 (exact closure) is a byte-authority claim, and only VOLE and "
                 "the source-retaining SQLite lane can make it.")
    lines.append("")

    with open(os.path.join(campaign, "MATRIX.md"), "w") as f:
        f.write("\n".join(lines))

    # --- summary.json -------------------------------------------------------
    total_mismatch = sum(qcompare[q][o]["mismatch"] for q in QS for o in ("sqlite", "duckdb"))
    verdict = "PASS"
    if exact_ok != exact_n or exact_n == 0 or total_mismatch > 0:
        verdict = "FAIL"
    summary = {
        "phase": "21.16 — Arrow IPC analytical economic court",
        "fixtures": fixtures,
        "lanes": lanes,
        "exact_ok": exact_ok,
        "exact_n": exact_n,
        "qcompare": qcompare,
        "total_mismatch": total_mismatch,
        "estimator": "paired per-fixture ratio; median + geometric mean; fixed-seed cluster "
                     "bootstrap (%d resamples, seed %d); tie band +/-%d%%" % (B, SEED, int(TIE * 100)),
        "storage_accounting": "sum of regular-file sizes (find -type f -printf '%s')",
        "verdict": verdict,
    }
    with open(os.path.join(campaign, "summary.json"), "w") as f:
        json.dump(summary, f, indent=2, sort_keys=True)
    return 0 if verdict == "PASS" else 1


def main(argv=None):
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    b = sub.add_parser("build")
    b.add_argument("--source", required=True)
    b.add_argument("--table-source", required=True)
    b.add_argument("--db", required=True)
    q = sub.add_parser("query")
    q.add_argument("--db", required=True)
    q.add_argument("--q", required=True)
    q.add_argument("--plan", default="{}")
    q.add_argument("--out", required=True)
    s = sub.add_parser("session")
    s.add_argument("--db", required=True)
    s.add_argument("--queries", default="")
    s.add_argument("--plan", default="{}")
    s.add_argument("--out", required=True)
    m = sub.add_parser("materialize")
    m.add_argument("--db", required=True)
    m.add_argument("--out", required=True)
    a = sub.add_parser("aggregate")
    a.add_argument("--raw", required=True)
    a.add_argument("--campaign", required=True)
    a.add_argument("--env", default=None)
    args = ap.parse_args(argv)
    plan = json.loads(getattr(args, "plan", "{}") or "{}")

    if args.cmd == "build":
        build(args.source, args.table_source, args.db)
        return 0
    if args.cmd == "query":
        env = query(args.db, args.q, plan)
        with open(args.out, "w") as f:
            json.dump(env, f, sort_keys=True)
        return 0
    if args.cmd == "session":
        batch = []
        answers = {}
        for q in QS:
            t0 = now_us()
            env = query(args.db, q, plan)
            t1 = now_us()
            answers[q] = env
            batch.append({"q": q, "us": t1 - t0})
        with open(args.out, "w") as f:
            json.dump({"batch": batch, "answers": answers}, f, sort_keys=True)
        return 0
    if args.cmd == "materialize":
        materialize(args.db, args.out)
        return 0
    if args.cmd == "aggregate":
        return aggregate(args.raw, args.campaign, args.env)
    return 2


if __name__ == "__main__":
    sys.exit(main())
