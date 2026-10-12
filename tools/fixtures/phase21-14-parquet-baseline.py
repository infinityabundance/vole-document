#!/usr/bin/env python3
# Phase 21.14 / 28.2 — Parquet economic court: the source-retaining SQLite baseline.
#
# The comparator retains the original Parquet bytes verbatim (a `raw` BLOB), so it
# can answer the byte-authority (exact-closure) question. SQLite has **no** Parquet
# reader, so a conventional relational load is performed by the same DuckDB reader
# the analytical comparator uses (documented, never hidden): the SQLite lane then
# measures the **row-store query cost**, not the Parquet decode cost. It cannot
# answer the Parquet-metadata questions (a column chunk's exact source span and
# min/max statistics) or a row-group count — those decline typed.
#
# PHASE 28.2 — the same competitor envelope as the Arrow court: typed columns, a
# covering predicate index, a tuned FTS5 external-content **trigram** index for the
# substring question (with an `instr()` cross-check and `case_sensitive_like=ON`), a
# realistic `cache_size`, memory-mapped I/O, and an in-process steady-state harness
# reported separately from cold-process/cold-cache. `WITHOUT ROWID` is declined for
# the same reason as the Arrow court (PK order != insertion order breaks Q1's
# positional value comparison).
#
#   build       --source FILE --db DB --plan JSON
#   query       --db DB --q Qn --plan JSON --out FILE          (cold-process)
#   steady      --db DB --plan JSON --out FILE [--iters R --warmup W]
#   coldcache   --db DB --plan JSON --out FILE
#   session     --db DB --queries Q1,... --plan JSON --out FILE  (legacy single-shot)
#   explain     --db DB --plan JSON --out FILE                 (EXPLAIN QUERY PLAN)
#   materialize --db DB --out FILE
#   aggregate   --raw DIR --campaign DIR --env ENV_JSON

import argparse
import hashlib
import importlib.util as _ilu
import json
import os
import sqlite3
import sys
import time

import duckdb

Q6_MSG = None  # retained: SQLite answers exact closure from the raw BLOB
Q3_MSG = "SQLite has no Parquet reader: a column chunk's exact source span/statistics are not retained"
Q4_MSG = "SQLite has no Parquet reader: the row-group count is not retained"

QS = ["Q1", "Q2", "Q3", "Q4", "Q5", "Q6"]

CACHE_SIZE_KIB = -65536  # 64 MiB page cache (negative = KiB)
MMAP_SIZE = 268435456    # 256 MiB memory-mapped I/O


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
    e = {"q": q, "lane": "sqlite", "declined": bool(declined), "native": native,
         "value": None if declined else value, "detail": detail or {}}
    e["decline"] = {"code": code, "detail": reason} if declined else None
    return e


def _cell(v):
    return "null" if v is None else str(v)


def _affinity(dtype):
    d = dtype.upper()
    if any(d.startswith(k) for k in ("TINYINT", "SMALLINT", "INTEGER", "BIGINT",
                                     "HUGEINT", "UTINYINT", "USMALLINT", "UINTEGER",
                                     "UBIGINT", "UHUGEINT")):
        return "INTEGER"
    if any(d.startswith(k) for k in ("REAL", "FLOAT", "DOUBLE", "DECIMAL", "NUMERIC")):
        return "REAL"
    if d.startswith("BLOB") or d.startswith("BINARY") or d.startswith("VARBINARY"):
        return "BLOB"
    return "TEXT"


def open_db(db):
    conn = sqlite3.connect(db)
    conn.execute("PRAGMA cache_size=%d" % CACHE_SIZE_KIB)
    conn.execute("PRAGMA mmap_size=%d" % MMAP_SIZE)
    conn.execute("PRAGMA temp_store=MEMORY")
    conn.execute("PRAGMA query_only=ON")
    conn.execute("PRAGMA case_sensitive_like=ON")
    return conn


def build(source, db, plan):
    if os.path.exists(db):
        os.remove(db)
    raw = open(source, "rb").read()
    conn = sqlite3.connect(db)
    conn.execute("CREATE TABLE src(bytes BLOB)")
    conn.execute("INSERT INTO src VALUES (?)", (raw,))
    ncols, rows, types = _load(conn, source)
    conn.execute("CREATE TABLE meta(key TEXT, value TEXT)")
    conn.execute("INSERT INTO meta VALUES ('ncols', ?)", (str(ncols),))
    conn.execute("INSERT INTO meta VALUES ('nrows', ?)", (str(len(rows)),))

    opt = {"typed_columns": [_affinity(t) for t in types], "indexes": []}
    pred_col = plan.get("col_num")
    str_col = plan.get("col_str")
    aff = [_affinity(t) for t in types]
    if isinstance(pred_col, int) and 0 <= pred_col < ncols:
        conn.execute("CREATE INDEX ix_num ON t(c%d)" % pred_col)
        opt["indexes"].append("ix_num(t.c%d) covering index for the range predicate" % pred_col)
    if isinstance(str_col, int) and 0 <= str_col < ncols and aff[str_col] == "TEXT":
        try:
            conn.execute(
                "CREATE VIRTUAL TABLE fts USING fts5(c%d, content='t', "
                "content_rowid='rowid', tokenize='trigram')" % str_col)
            conn.execute("INSERT INTO fts(fts) VALUES('rebuild')")
            opt["indexes"].append(
                "fts(c%d) external-content FTS5 trigram index for the substring question" % str_col)
            opt["q5_path"] = "fts5-trigram"
        except sqlite3.OperationalError as exc:
            opt["q5_path"] = "instr-scan"
            opt["fts5_error"] = str(exc)
    else:
        opt["q5_path"] = "instr-scan"
        opt["fts5_skipped"] = "the Q5 column is not a TEXT column"
    conn.commit()
    conn.close()
    with open(db + ".build.json", "w") as f:
        json.dump({"lane": "sqlite", "ncols": ncols, "nrows": len(rows),
                   "sqlite_version": sqlite3.sqlite_version,
                   "pragmas": {"cache_size_kib": CACHE_SIZE_KIB, "mmap_size": MMAP_SIZE,
                               "temp_store": "MEMORY", "case_sensitive_like": "ON"},
                   "optimizations": opt}, f, indent=2, sort_keys=True)


def _load(conn, source):
    """A conventional relational load, using the available Parquet reader."""
    desc = duckdb.sql("DESCRIBE SELECT * FROM read_parquet('%s')" % source).fetchall()
    ncols = len(desc)
    types = [d[1] for d in desc]
    rows = duckdb.sql("SELECT * FROM read_parquet('%s')" % source).fetchall()
    cols = ",".join("c%d %s" % (i, _affinity(types[i])) for i in range(ncols))
    conn.execute("CREATE TABLE t(%s)" % cols)
    ph = ",".join("?" for _ in range(ncols))
    conn.executemany("INSERT INTO t VALUES (%s)" % ph, rows)
    return ncols, rows, types


def answer(conn, q, plan, crosscheck=False):
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
        sub = plan["substr"]
        try:
            n_fts = int(conn.execute("SELECT count(*) FROM fts WHERE c%d LIKE ?" % plan["col_str"],
                                     ("%" + sub + "%",)).fetchone()[0])
            detail = {"path": "fts5-trigram"}
        except sqlite3.OperationalError:
            n_fts = int(conn.execute("SELECT count(*) FROM t WHERE instr(c%d, ?) > 0" % plan["col_str"],
                                     (sub,)).fetchone()[0])
            detail = {"path": "instr-scan"}
        if crosscheck:
            n_instr = int(conn.execute("SELECT count(*) FROM t WHERE instr(c%d, ?) > 0" % plan["col_str"],
                                       (sub,)).fetchone()[0])
            detail["instr_count"] = n_instr
            detail["agree"] = (n_instr == n_fts)
        return {"value": n_fts, "detail": detail}
    if q == "Q6":
        row = conn.execute("SELECT bytes FROM src").fetchone()
        b = bytes(row[0])
        return {"value": {"length": len(b), "sha256": hashlib.sha256(b).hexdigest()}}
    raise RuntimeError("unknown question %s" % q)


def query(db, q, plan):
    conn = open_db(db)
    try:
        return answer(conn, q, plan, crosscheck=True)
    finally:
        conn.close()


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

# Historical control: the pre-28.2 (Phase 21.14) campaign for this court. Its `warm`
# was a single-shot session.
PRE_28_2_CAMPAIGN = "evidence/campaigns/2026-10-10-phase21-14-parquet-econ-bfe32a33"
PRE_28_2_NOTE = ("pre-28.2 (21.14): `warm` was a single-shot session with no percentiles; "
                 "`cold` was fresh-process-per-question; the SQLite lane had no typed columns or "
                 "indexes. A historical control, not a like-for-like delta")


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
    "Q3": "a column chunk's exact span + min/max statistics",
    "Q4": "the row-group count",
    "Q5": "a lexical find (rows in a string column containing S)",
    "Q6": "`materialize --exact` (byte-authority)",
}


def aggregate(raw, campaign, env_path=None):
    P19 = _load_p19()
    B = 10000
    SEED = 21721
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
    steady_rows = read_tsv(os.path.join(raw, "steady.tsv"))
    coldcache_rows = read_tsv(os.path.join(raw, "coldcache.tsv"))
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

    def per_q(rows, lane, col, require_rc0=False):
        out = {}
        for r in rows:
            if r["lane"] != lane:
                continue
            if require_rc0 and r.get("rc") not in (None, "0"):
                continue
            key = (r["fixture"], r["q"])
            v = float(r[col])
            out[key] = v if key not in out else min(out[key], v)
        return out

    def answered_set(lane, fx):
        s = set()
        for q in QS:
            v = _envelope_value(os.path.join(raw, "qanswers", "%s.%s.%s.json" % (fx, q, lane)))
            if v is not None and not v.get("declined"):
                s.add(q)
        return s

    def ratio_metric(table, other):
        # Equal contract: sum only the questions BOTH lanes answer.
        ratios = {}
        sums_v = sums_o = 0.0
        for fx in fixtures:
            qs = sorted(answered_set("vole", fx) & answered_set(other, fx))
            v = o = 0.0
            ok = bool(qs)
            for q in qs:
                vv = table["vole"].get((fx, q))
                oo = table[other].get((fx, q))
                if vv is None or oo is None:
                    ok = False
                    break
                v += vv
                o += oo
            if not ok or o == 0:
                continue
            ratios[fx] = v / o
            sums_v += v
            sums_o += o
        return ratios, sums_v, sums_o

    build_us = {lane: best_by_fixture(build_rows, lane) for lane in lanes}
    store_bytes = {lane: {} for lane in lanes}
    for r in storage_rows:
        store_bytes[r["lane"]][r["fixture"]] = int(r["bytes"])
    cold_q = {lane: per_q(cold_rows, lane, "us", require_rc0=True) for lane in lanes}
    steady_q = {lane: per_q(steady_rows, lane, "median") for lane in lanes}
    coldcache_q = {lane: per_q(coldcache_rows, lane, "us") for lane in lanes}

    others = ("sqlite", "duckdb")

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

    # Capability accounting per lane: answered vs typed-declined (by code) and answered
    # vs VOLE (equal / capability-gap / both-decline / mismatch).
    cap = {}
    for lane in ("vole", "sqlite", "duckdb"):
        d = {"answered": 0, "declined": 0, "declined_codes": {}, "vs_vole": {}}
        if lane != "vole":
            d["vs_vole"] = {"equal": 0, "capability_gap": 0, "both_decline": 0, "mismatch": 0}
        for fx in fixtures:
            for q in QS:
                v = _envelope_value(os.path.join(raw, "qanswers", "%s.%s.%s.json" % (fx, q, lane)))
                if v is None:
                    continue
                if v.get("declined"):
                    d["declined"] += 1
                    code = (v.get("decline") or {}).get("code", "unknown")
                    d["declined_codes"][code] = d["declined_codes"].get(code, 0) + 1
                else:
                    d["answered"] += 1
                if lane != "vole":
                    va = _envelope_value(os.path.join(raw, "qanswers", "%s.%s.vole.json" % (fx, q)))
                    status, _ = compare(q, va, v)
                    key = {"equal": "equal", "capability-gap": "capability_gap",
                           "both-decline": "both_decline", "mismatch": "mismatch"}.get(status)
                    if key:
                        d["vs_vole"][key] += 1
        cap[lane] = d

    exact_ok = sum(1 for r in exact_rows if r.get("vole_ok") == "true")
    exact_n = len(exact_rows)

    # --- MATRIX.md ----------------------------------------------------------
    lines = []
    lines.append("# Phase 21.14 Parquet economic court — matrix")
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
    lines.append("## Capability accounting (per lane, over %d questions x %d fixtures)" % (len(QS), len(fixtures)))
    lines.append("")
    lines.append("A lane that **declines** what another answers has not won an equal-contract")
    lines.append("comparison; every typed decline is counted, never hidden.")
    lines.append("")
    lines.append("| lane | answered | declined | decline codes | vs VOLE equal / gap / both-decline / mismatch |")
    lines.append("|---|---:|---:|---|---|")
    for lane in ("vole", "sqlite", "duckdb"):
        d = cap[lane]
        codes = ", ".join("%s=%d" % (k, v) for k, v in sorted(d["declined_codes"].items())) or "-"
        if lane == "vole":
            vs = "(reference)"
        else:
            v = d["vs_vole"]
            vs = "%d / %d / %d / %d" % (v["equal"], v["capability_gap"], v["both_decline"], v["mismatch"])
        lines.append("| %s | %d | %d | %s | %s |" % (lane, d["answered"], d["declined"], codes, vs))
    lines.append("")
    lines.append("## Paired ratios VOLE/other (median + geometric mean, 95% CI by fixture)")
    lines.append("")
    lines.append("| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | "
                 "wins | ties | losses | ratio of sums |")
    lines.append("|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|")
    metrics = (("build", build_us, False), ("storage", store_bytes, False),
               ("steady", steady_q, True), ("cold-process", cold_q, True),
               ("cold-cache", coldcache_q, True))
    for metric, key, per_question in metrics:
        for other in others:
            if per_question:
                ratios, sums_v, sums_o = ratio_metric(key, other)
            else:
                ratios, sums_v, sums_o = {}, 0.0, 0.0
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
    lines.append("A ratio < 1 favours VOLE. The **headline is `steady`** (in-process, warm")
    lines.append("connection/state; per-question median). Question-level metrics (steady,")
    lines.append("cold-process, cold-cache) are summed per fixture over the **common answer set**")
    lines.append("(only questions BOTH lanes answer), so a lane is never charged for work its")
    lines.append("comparator declines. `build`/`storage` are whole-artifact. The estimator is the")
    lines.append("**paired per-fixture ratio**, summarised by the median and geometric mean with a")
    lines.append("fixed-seed cluster bootstrap over fixtures; `ratio of sums` (a size-weighted pooled")
    lines.append("view) is reported separately. With only %d fixture clusters the bootstrap is coarse" % len(fixtures))
    lines.append("and is stated as such.")
    lines.append("")
    lines.append("## Steady-state percentiles per question (us)")
    lines.append("")
    lines.append("| fixture | lane | Q | min | p50 | p95 | p99 | n |")
    lines.append("|---|---|---|---:|---:|---:|---:|---:|")
    for r in steady_rows:
        lines.append("| `%s` | %s | %s | %s | %s | %s | %s | %s |" % (
            r["fixture"], r["lane"], r["q"], r["min"], r["p50"], r["p95"], r["p99"], r["n"]))
    lines.append("")
    lines.append("**Cold vs steady.** `cold-process` includes a fresh Python + `duckdb` import for the")
    lines.append("Python lanes; the steady-state headline removes it. **Cold-cache honesty:** the")
    lines.append("container is unprivileged, so the OS page cache cannot be dropped; `cold-cache`")
    lines.append("differs from `cold-process` only by the resident interpreter.")
    lines.append("")
    lines.append("**Recorded plainly:** DuckDB wins the analytical axes (projection, predicate")
    lines.append("execution, compressed pages, selective reads via metadata statistics). VOLE is")
    lines.append("an archival field with typed observations, not a query engine; its predicate")
    lines.append("counts are a decoded column plus a count in the driver. Only Q6 (exact closure)")
    lines.append("is a byte-authority claim, and only VOLE and the source-retaining SQLite lane")
    lines.append("can make it.")
    lines.append("")
    lines.append("**Envelope (28.2).** SQLite gets typed columns, a covering predicate index, and a")
    lines.append("tuned FTS5 external-content trigram index for the substring question; DuckDB reuses")
    lines.append("SQL-level prepared statements and its Parquet zonemap/statistics pruning. Where a")
    lines.append("strengthened competitor now wins more, it is recorded here plainly.")
    lines.append("")
    lines.append("**Historical control.** %s. Campaign `%s`." % (PRE_28_2_NOTE, PRE_28_2_CAMPAIGN))
    lines.append("")

    with open(os.path.join(campaign, "MATRIX.md"), "w") as f:
        f.write("\n".join(lines))

    # --- summary.json -------------------------------------------------------
    total_mismatch = sum(qcompare[q][o]["mismatch"] for q in QS for o in ("sqlite", "duckdb"))
    verdict = "PASS"
    if exact_ok != exact_n or exact_n == 0 or total_mismatch > 0:
        verdict = "FAIL"
    summary = {
        "phase": "28.2 — Parquet analytical economic court (competitor envelope)",
        "fixtures": fixtures,
        "lanes": lanes,
        "exact_ok": exact_ok,
        "exact_n": exact_n,
        "qcompare": qcompare,
        "capability_accounting": cap,
        "total_mismatch": total_mismatch,
        "headline_metric": "steady (in-process, warm connection/state; per-question median; equal-contract common answer set)",
        "separate_metrics": ["cold-process (fresh OS process per question)",
                             "cold-cache (fresh connection/reader, warm interpreter)"],
        "estimator": "paired per-fixture ratio over the common answer set; median + geometric mean; "
                     "fixed-seed cluster bootstrap (%d resamples, seed %d); tie band +/-%d%%"
                     % (B, SEED, int(TIE * 100)),
        "storage_accounting": "sum of regular-file sizes (find -type f -printf '%s')",
        "historical_control": {"campaign": PRE_28_2_CAMPAIGN, "note": PRE_28_2_NOTE},
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
    b.add_argument("--db", required=True)
    b.add_argument("--plan", default="{}")
    q = sub.add_parser("query")
    q.add_argument("--db", required=True)
    q.add_argument("--q", required=True)
    q.add_argument("--plan", default="{}")
    q.add_argument("--out", required=True)
    st = sub.add_parser("steady")
    st.add_argument("--db", required=True)
    st.add_argument("--plan", default="{}")
    st.add_argument("--out", required=True)
    st.add_argument("--iters", type=int, default=15)
    st.add_argument("--warmup", type=int, default=3)
    cc = sub.add_parser("coldcache")
    cc.add_argument("--db", required=True)
    cc.add_argument("--plan", default="{}")
    cc.add_argument("--out", required=True)
    s = sub.add_parser("session")
    s.add_argument("--db", required=True)
    s.add_argument("--queries", default="")
    s.add_argument("--plan", default="{}")
    s.add_argument("--out", required=True)
    e = sub.add_parser("explain")
    e.add_argument("--db", required=True)
    e.add_argument("--plan", default="{}")
    e.add_argument("--out", required=True)
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
        build(args.source, args.db, plan)
        return 0
    if args.cmd == "query":
        env = query(args.db, args.q, plan)
        with open(args.out, "w") as f:
            json.dump(env, f, sort_keys=True)
        return 0
    if args.cmd == "steady":
        conn = open_db(args.db)
        results = {}
        for qn in QS:
            results[qn] = H.steady_state(lambda qn=qn: answer(conn, qn, plan),
                                         warmup=args.warmup, iters=args.iters)
        conn.close()
        with open(args.out, "w") as f:
            json.dump({"batch": [{"q": k, **v} for k, v in results.items()]}, f, sort_keys=True)
        return 0
    if args.cmd == "coldcache":
        fns = {qn: (lambda c, qn=qn: answer(c, qn, plan)) for qn in QS}
        out = H.cold_cache(fns, lambda: open_db(args.db))
        with open(args.out, "w") as f:
            json.dump({"coldcache": out}, f, sort_keys=True)
        return 0
    if args.cmd == "session":
        conn = open_db(args.db)
        batch = []
        answers = {}
        for qn in QS:
            t0 = now_us()
            env = answer(conn, qn, plan)
            t1 = now_us()
            answers[qn] = env
            batch.append({"q": qn, "us": t1 - t0})
        conn.close()
        with open(args.out, "w") as f:
            json.dump({"batch": batch, "answers": answers}, f, sort_keys=True)
        return 0
    if args.cmd == "explain":
        conn = open_db(args.db)
        out = {"lane": "sqlite", "sqlite_version": sqlite3.sqlite_version,
               "pragmas": {"cache_size_kib": CACHE_SIZE_KIB, "mmap_size": MMAP_SIZE,
                           "case_sensitive_like": "ON"},
               "plans": {}}
        q2 = "SELECT count(*) FROM t WHERE c%d > %d" % (plan["col_num"], plan["gt"])
        out["plans"]["Q2"] = {"sql": q2,
                              "eqp": conn.execute("EXPLAIN QUERY PLAN " + q2).fetchall()}
        try:
            q5 = "SELECT count(*) FROM fts WHERE c%d LIKE '%%%s%%'" % (plan["col_str"], plan["substr"])
            out["plans"]["Q5"] = {"sql": q5,
                                  "eqp": conn.execute("EXPLAIN QUERY PLAN " + q5).fetchall()}
        except sqlite3.OperationalError as exc:
            out["plans"]["Q5"] = {"error": str(exc)}
        q1 = "SELECT c%d FROM t" % plan["col_str"]
        out["plans"]["Q1"] = {"sql": q1,
                              "eqp": conn.execute("EXPLAIN QUERY PLAN " + q1).fetchall()}
        conn.close()
        with open(args.out, "w") as f:
            json.dump(out, f, sort_keys=True)
        return 0
    if args.cmd == "materialize":
        materialize(args.db, args.out)
        return 0
    if args.cmd == "aggregate":
        return aggregate(args.raw, args.campaign, args.env)
    return 2


if __name__ == "__main__":
    sys.exit(main())
