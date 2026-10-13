#!/usr/bin/env python3
# Phase 30.2 — push the fused CSV kernel past pyarrow (read-inclusive).
#
# Phase 29.1/30.1 measured the fused delimited scan against pyarrow and DuckDB with
# the Phase-28.2 in-process steady-state harness. Reading the Phase-30.1 receipt
# closely shows that the pyarrow *CSV* lane timed only the in-memory aggregation
# (`pc.greater`/`pc.sum` over an already-parsed `Table`): the `pyarrow.csv.read_csv`
# parse sat **outside** the timed lambda. That is fine for Arrow/Parquet (a
# zero-copy columnar scan) but for CSV it compares VOLE's parse+aggregate against
# pyarrow's aggregate-only, which is why the fused/CSV-to-pyarrow ratio read ~35x.
#
# This driver supplies the honest, apples-to-apples CSV comparator: the pyarrow lane
# times `read_csv(bytes)` **plus** the aggregation, and DuckDB times its
# `read_csv_auto` view (which re-parses on every query). It also reports the
# aggregation-only number side by side so the Phase-30.1 figure stays comparable.
#
# Lanes normalised by this driver (`collect`):
#   fused        — VOLE `fused-scan` (its own fused_ns samples)
#   pyarrow_agg  — pyarrow aggregate-only (the Phase-30.1 methodology)
#   pyarrow_read — pyarrow read_csv(bytes) + aggregate  (read-inclusive)
#   duckdb_read  — DuckDB SELECT over a read_csv_auto view (read-inclusive)
#   duckdb_agg   — DuckDB SELECT over a materialised in-memory table (agg-only)
#
# Correctness oracle is pyarrow/DuckDB on the same logical data; `aggregate` checks
# count/sum/min/max/avg/grouped for exact agreement (floats within 1e-9 relative).
#
# No third-party imports except `pyarrow`/`duckdb`, both pinned in `analytical`.

import argparse
import importlib.util as _ilu
import io
import json
import os
import statistics
import sys

# Never leave `__pycache__` behind (the courts run with PYTHONDONTWRITEBYTECODE=1
# too; this guards the `exec_module` path below regardless of the environment).
sys.dont_write_bytecode = True


def _load(name, fname):
    here = os.path.dirname(os.path.abspath(__file__))
    spec = _ilu.spec_from_file_location(name, os.path.join(here, fname))
    mod = _ilu.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


H = _load("phase28_2_harness", "phase28-2-harness.py")
P19 = _load("phase19_repeat", "phase19-repeat.py")


def _num(x):
    if x is None:
        return None
    if isinstance(x, float):
        return x
    return int(x)


def _equal(a, b):
    if a is None or b is None:
        return a is b
    if isinstance(a, list) and isinstance(b, list):
        if len(a) != len(b):
            return False
        return all(_equal(x, y) for x, y in zip(a, b))
    if isinstance(a, float) or isinstance(b, float):
        return abs(float(a) - float(b)) <= 1e-9 * max(1.0, abs(float(a)))
    return int(a) == int(b)


def _cast_gt(gt, kind):
    return float(gt) if kind == "float" else int(gt)


def _dump(path, obj):
    with open(path, "w") as f:
        json.dump(obj, f, sort_keys=True)


def _read_tsv(path):
    out = []
    with open(path) as f:
        head = f.readline().rstrip("\n").split("\t")
        for line in f:
            if not line.strip():
                continue
            out.append(dict(zip(head, line.rstrip("\n").split("\t"))))
    return out


# ---------------------------------------------------------------------------
# oracle lanes (read-inclusive)
# ---------------------------------------------------------------------------
def _pa_answer(table, column, op, gt, kind):
    import pyarrow as pa
    import pyarrow.compute as pc

    col = table.column(column)
    scalar = pa.scalar(_cast_gt(gt, kind), type=col.type)
    mask = pc.fill_null(pc.greater(col, scalar), False)
    if op == "count":
        return int(pc.sum(pc.cast(mask, pa.int64())).as_py())
    filtered = pc.filter(col, mask)
    if op == "sum":
        return _num(pc.sum(filtered).as_py())
    if op == "min":
        return _num(pc.min(filtered).as_py())
    if op == "max":
        return _num(pc.max(filtered).as_py())
    if op == "avg":
        return _num(pc.mean(filtered).as_py())
    if op == "grouped":
        vc = pc.value_counts(filtered).to_pylist()
        out = [[_num(it["values"]), int(it["counts"])] for it in vc]
        out.sort(key=lambda x: x[0])
        return out
    raise SystemExit("bad op " + op)


def _pa_read(data):
    import pyarrow.csv as pacsv

    return pacsv.read_csv(io.BytesIO(data))


def _duckdb_sql(column, op, gt):
    c = '"%s"' % column
    where = "%s > %s" % (c, gt)
    if op == "count":
        return "SELECT count(*) FROM t WHERE " + where
    if op in ("sum", "min", "max", "avg"):
        return "SELECT %s(%s) FROM t WHERE %s" % (op, c, where)
    if op == "grouped":
        return "SELECT %s, count(*) FROM t WHERE %s GROUP BY %s ORDER BY %s" % (
            c,
            where,
            c,
            c,
        )
    raise SystemExit("bad op " + op)


def _duckdb_answer(con, column, op, gt):
    rows = con.execute(_duckdb_sql(column, op, gt)).fetchall()
    if op == "grouped":
        return [[_num(r[0]), int(r[1])] for r in rows]
    return _num(rows[0][0])


def cmd_oracle(a):
    if a.lane == "pyarrow":
        data = open(a.source, "rb").read()
        table = _pa_read(data)
        value = _pa_answer(table, a.column, a.op, a.gt, a.kind)
        ns_agg = H.steady_state_ns(
            lambda: _pa_answer(table, a.column, a.op, a.gt, a.kind), a.warmup, a.iters
        )
        ns_read = H.steady_state_ns(
            lambda: _pa_answer(_pa_read(data), a.column, a.op, a.gt, a.kind),
            a.warmup,
            a.iters,
        )
        _dump(
            a.out,
            {
                "lane": "pyarrow",
                "value": value,
                "ns": ns_read,
                "ns_read": ns_read,
                "ns_agg_only": ns_agg,
            },
        )
        return

    # duckdb
    import duckdb

    con = duckdb.connect()
    con.execute("PRAGMA threads=%d" % a.threads)
    q = a.source.replace("'", "''")
    con.execute("CREATE VIEW t AS SELECT * FROM read_csv_auto('%s')" % q)
    value = _duckdb_answer(con, a.column, a.op, a.gt)
    ns_read = H.steady_state_ns(
        lambda: _duckdb_answer(con, a.column, a.op, a.gt), a.warmup, a.iters
    )
    # aggregation-only control: materialise the parse once into an in-memory table.
    con.execute("CREATE TABLE tmem AS SELECT * FROM read_csv_auto('%s')" % q)
    con.execute("CREATE VIEW tm AS SELECT * FROM tmem")
    ns_agg = None
    try:
        con.execute("SELECT 1 FROM tm LIMIT 1")
        con2 = duckdb.connect()
        con2.execute("PRAGMA threads=%d" % a.threads)
        con2.register("t", con.execute("SELECT * FROM tm").arrow())
        ns_agg = H.steady_state_ns(
            lambda: _duckdb_answer(con2, a.column, a.op, a.gt), a.warmup, a.iters
        )
    except Exception:
        ns_agg = None
    _dump(
        a.out,
        {
            "lane": "duckdb",
            "value": value,
            "ns": ns_read,
            "ns_read": ns_read,
            "ns_agg_only": ns_agg,
        },
    )


# ---------------------------------------------------------------------------
# collect: normalise every raw artifact into unified sample/answer tables
# ---------------------------------------------------------------------------
def fused_value(d, op):
    if d.get("declined"):
        return None
    if op == "count":
        return int(d["count"])
    if op == "sum":
        if "sum_int" in d:
            return int(d["sum_int"])
        if "sum_float" in d:
            return float(d["sum_float"])
        return None
    if op == "min":
        return int(d["min_int"]) if "min_int" in d else float(d["min_float"])
    if op == "max":
        return int(d["max_int"]) if "max_int" in d else float(d["max_float"])
    if op == "avg":
        return float(d["avg"]) if "avg" in d else None
    if op == "grouped":
        return [[int(k), int(c)] for k, c in d.get("groups", [])]
    return None


def cmd_collect(a):
    raw = a.raw
    rows = _read_tsv(os.path.join(raw, "plan.tsv"))
    reps = int(a.reps)
    samples = open(os.path.join(raw, "samples.tsv"), "w")
    samples.write("fixture\tformat\tqid\tlane\trep\ti\tns\n")
    answers = open(os.path.join(raw, "answers.tsv"), "w")
    answers.write("fixture\tformat\tqid\tlane\trep\tvalue\tdeclined\n")

    def emit_samples(fx, fmt, qid, lane, rep, ns_list):
        for i, u in enumerate(ns_list):
            samples.write("%s\t%s\t%s\t%s\t%d\t%d\t%d\n" % (fx, fmt, qid, lane, rep, i, int(u)))

    def emit_answer(fx, fmt, qid, lane, rep, value, declined):
        answers.write(
            "%s\t%s\t%s\t%s\t%d\t%s\t%s\n"
            % (fx, fmt, qid, lane, rep, json.dumps(value), "1" if declined else "0")
        )

    for row in rows:
        fx, fmt, qid = row["fixture"], row["format"], row["qid"]
        op = row["op"]
        base = "%s.%s.%s" % (fx, fmt, qid)
        for rep in range(1, reps + 1):
            for nlanes in ("fused", "fused_serial"):
                p = os.path.join(raw, nlanes, "%s.%d.json" % (base, rep))
                if not os.path.exists(p):
                    continue
                d = json.load(open(p))
                emit_samples(fx, fmt, qid, nlanes, rep, d.get("fused_ns", []))
                emit_answer(fx, fmt, qid, nlanes, rep, fused_value(d, op), d.get("declined"))
            for lane, jfile in (("pyarrow", "pyarrow"), ("duckdb", "duckdb")):
                op_path = os.path.join(raw, jfile, "%s.%d.json" % (base, rep))
                if not os.path.exists(op_path):
                    continue
                c = json.load(open(op_path))
                emit_samples(fx, fmt, qid, lane + "_read", rep, c.get("ns_read", []))
                emit_answer(fx, fmt, qid, lane + "_read", rep, c.get("value"), c.get("value") is None)
                if c.get("ns_agg_only"):
                    emit_samples(fx, fmt, qid, lane + "_agg", rep, c["ns_agg_only"])
                    emit_answer(fx, fmt, qid, lane + "_agg", rep, c.get("value"), False)
    samples.close()
    answers.close()


# ---------------------------------------------------------------------------
# aggregate: the ADR-0054 estimator + MATRIX/decision/receipt
# ---------------------------------------------------------------------------
def cmd_aggregate(a):
    raw, campaign = a.raw, a.campaign
    env = {}
    try:
        env = json.load(open(a.env))
    except (OSError, ValueError):
        pass
    plan = _read_tsv(os.path.join(raw, "plan.tsv"))
    samples = _read_tsv(os.path.join(raw, "samples.tsv"))
    answers = _read_tsv(os.path.join(raw, "answers.tsv"))
    try:
        exact = _read_tsv(os.path.join(raw, "exact.tsv"))
    except OSError:
        exact = []

    # The estimator/headline use only the `group == perf` cells (the large CSV
    # cells); correctness-only cells (grouped/NULL fixtures) still feed the oracle
    # agreement check below.
    clusters = [
        (r["fixture"], r["format"], r["qid"])
        for r in plan
        if r.get("group", "perf") == "perf"
    ]
    rep_med = {}
    for r in samples:
        key = (r["fixture"], r["format"], r["qid"], r["lane"])
        rep_med.setdefault(key, {}).setdefault(int(r["rep"]), []).append(int(r["ns"]))
    rep_median = {k: {rep: statistics.median(v) for rep, v in reps.items()} for k, reps in rep_med.items()}

    def lane_cluster(lane):
        out = {}
        for (fx, fmt, qid, ln), reps in rep_median.items():
            if ln == lane:
                out.setdefault((fx, fmt, qid), {}).update(reps)
        return out

    fused = lane_cluster("fused")

    def paired(numer, denom):
        by = {}
        for c in clusters:
            n, d = numer.get(c), denom.get(c)
            if not n or not d:
                continue
            shared = sorted(set(n) & set(d))
            rs = [n[r] / d[r] for r in shared if d[r] > 0 and n[r] > 0]
            if rs:
                by[c] = rs
        return by

    B, SEED = 20000, 30215

    def mde(ci):
        lo, hi = ci
        return (1.959963985 + 0.8416212336) / 1.959963985 * ((hi - lo) / 2.0)

    comparisons = {
        "pyarrow_agg": "pyarrow_agg",
        "pyarrow_read": "pyarrow_read",
        "duckdb_read": "duckdb_read",
        "duckdb_agg": "duckdb_agg",
    }

    def build_estimator(numer_lane):
        numer = lane_cluster(numer_lane)
        out = {}
        for label, lane in comparisons.items():
            by = paired(numer, lane_cluster(lane))
            pooled = [v for vs in by.values() for v in vs]
            if not pooled:
                continue
            lo_m, hi_m, _ = P19.cluster_bootstrap(by, statistics.median, B, SEED)
            lo_g, hi_g, _ = P19.cluster_bootstrap(by, P19.geomean, B, SEED + 1)
            out[label] = {
                "n_clusters": len(by),
                "n_samples": len(pooled),
                "median": statistics.median(pooled),
                "geomean": P19.geomean(pooled),
                "median_ci": (lo_m, hi_m),
                "geomean_ci": (lo_g, hi_g),
                "wins": sum(1 for v in pooled if v < 1.0),
                "ties": sum(1 for v in pooled if abs(v - 1.0) <= 0.10),
                "losses": sum(1 for v in pooled if v > 1.0),
                "mde": mde((lo_m, hi_m)),
            }
        return out

    estimator = build_estimator("fused")
    estimator_serial = build_estimator("fused_serial")

    # per-question large-CSV ratios (fused / comparator median), the headline table
    gap = {}
    for c in clusters:
        for label, lane in comparisons.items():
            lc = lane_cluster(lane).get(c)
            if not lc:
                continue
            n = fused.get(c)
            if not n:
                continue
            shared = sorted(set(n) & set(lc))
            rs = [n[r] / lc[r] for r in shared if lc[r] > 0]
            if rs:
                gap.setdefault(c[2], {})[label] = statistics.median(rs)

    # aggregated "large CSV median vs pyarrow": pool the pyarrow_read and
    # pyarrow_agg per-question medians (median, not mean).
    headline = {}
    for label in comparisons:
        vals = [v[label] for v in gap.values() if label in v]
        if vals:
            headline[label] = {
                "questions": len(vals),
                "median": statistics.median(vals),
                "geomean": P19.geomean(vals),
            }

    # cold (fresh-process wall) per lane, and the fused steady distribution
    try:
        cold = _read_tsv(os.path.join(raw, "cold.tsv"))
    except OSError:
        cold = []
    cold_rep = {}
    for r in cold:
        cold_rep.setdefault((r["fixture"], r["format"], r["qid"], r["lane"]), {})[int(r["rep"])] = int(r["ns"])
    cold_cell = {k: statistics.median(list(v.values())) for k, v in cold_rep.items()}
    cold_overall = {}
    for lane in ("fused", "pyarrow", "duckdb"):
        vals = [m for (fx, fmt, qid, ln), m in cold_cell.items() if ln == lane]
        if vals:
            cold_overall[lane] = statistics.median(vals)

    fused_dist = {}
    for r in samples:
        if r["lane"] == "fused":
            fused_dist.setdefault((r["fixture"], r["format"], r["qid"]), []).append(int(r["ns"]))
    dist_by_fmt = {}
    for (fx, fmt, qid), vals in fused_dist.items():
        s = sorted(vals)
        dist_by_fmt.setdefault(fmt, []).append((H.pct(s, 50), H.pct(s, 95), H.pct(s, 99)))
    dist = {
        fmt: {
            "cells": len(v),
            "p50": statistics.median([x[0] for x in v]),
            "p95": statistics.median([x[1] for x in v]),
            "p99": statistics.median([x[2] for x in v]),
        }
        for fmt, v in dist_by_fmt.items()
    }

    # agreement vs the oracle (read-inclusive lanes), per cell
    ans = {}
    for r in answers:
        key = (r["fixture"], r["format"], r["qid"], r["lane"], int(r["rep"]))
        ans[key] = (json.loads(r["value"]) if r["value"] != "null" else None, r["declined"] == "1")

    agree = []
    for row in plan:
        fx, fmt, qid, op = row["fixture"], row["format"], row["qid"], row["op"]
        fv = ans.get((fx, fmt, qid, "fused", 1))
        if fv is None:
            continue
        fy, fdecl = fv
        for oracle in ("pyarrow_read", "duckdb_read"):
            ov = ans.get((fx, fmt, qid, oracle, 1))
            if ov is None:
                continue
            oy, odecl = ov
            agree.append(
                {
                    "fixture": fx,
                    "format": fmt,
                    "qid": qid,
                    "op": op,
                    "oracle": oracle,
                    "fused_declined": fdecl,
                    "oracle_declined": odecl,
                    "equal": (not fdecl) and (not odecl) and _equal(fy, oy),
                    "fused": fy,
                    "oracle": oy,
                }
            )

    declines = {}
    for r in answers:
        if r["declined"] != "1":
            continue
        declines.setdefault(r["lane"], {}).setdefault(r["format"], {}).setdefault(r["op"], 0)
        declines[r["lane"]][r["format"]][r["op"]] += 1

    exact_ok = sum(1 for r in exact if r.get("vole_ok") == "true")
    exact_n = len(exact)
    ag_mismatch = [x for x in agree if not x["equal"]]
    agreement_ok = len(ag_mismatch) == 0 and len(agree) > 0

    # verdict: wins iff the read-inclusive pyarrow ratio is below 1.0 and agreement
    # holds and every fixture is byte-exact.
    fair = estimator.get("pyarrow_read", {}).get("median")
    verdict = "PASS" if (fair is not None and fair < 1.0 and agreement_ok and exact_ok == exact_n and exact_n > 0) else "FAIL"

    # ---- MATRIX.md
    L = []
    L.append("# Phase 30.2 — fused CSV kernel vs pyarrow/DuckDB (read-inclusive)")
    L.append("")
    L.append("**Measurement.** In-process steady-state median (Phase-28.2 harness; warmup %s, iters %s),"
             % (env.get("warmup"), env.get("iters")))
    L.append("cold separated, ADR-0054 paired per-(question) estimator with a fixed-seed cluster bootstrap")
    L.append("(B=20000) and the MDE. Every raw sample is retained under `%s`." % raw)
    L.append("")
    L.append("## Estimator — fused / comparator, paired per question (ratio < 1 ⇒ fused faster)")
    L.append("")
    L.append("| comparison | clusters | samples | median | median 95% CI | geomean | MDE | wins | ties | losses |")
    L.append("|---|---|---|---|---|---|---|---|---|---|")
    for label in comparisons:
        e = estimator.get(label)
        if not e:
            continue
        L.append(
            "| %s | %d | %d | %.3f | [%.3f, %.3f] | %.3f | %.3f | %d | %d | %d |"
            % (label, e["n_clusters"], e["n_samples"], e["median"], e["median_ci"][0], e["median_ci"][1],
               e["geomean"], e["mde"], e["wins"], e["ties"], e["losses"])
        )
    L.append("")
    L.append("## Serial continuity (`--workers 1`) — fused / comparator")
    L.append("")
    L.append("| comparison | clusters | samples | median | median 95% CI | geomean | MDE | wins | ties | losses |")
    L.append("|---|---|---|---|---|---|---|---|---|---|")
    for label in comparisons:
        e = estimator_serial.get(label)
        if not e:
            continue
        L.append(
            "| %s | %d | %d | %.3f | [%.3f, %.3f] | %.3f | %.3f | %d | %d | %d |"
            % (label, e["n_clusters"], e["n_samples"], e["median"], e["median_ci"][0], e["median_ci"][1],
               e["geomean"], e["mde"], e["wins"], e["ties"], e["losses"])
        )
    L.append("")
    L.append("## Large-CSV headline (median of the per-question ratios)")
    L.append("")
    L.append("| comparison | questions | median fused/comparator | geomean |")
    L.append("|---|---|---|---|")
    for label in comparisons:
        h = headline.get(label)
        if h:
            L.append("| %s | %d | %.3f | %.3f |" % (label, h["questions"], h["median"], h["geomean"]))
    L.append("")
    L.append("## Per-question fused/comparator ratio")
    L.append("")
    L.append("| question | " + " | ".join(comparisons) + " |")
    L.append("|---|" + "---|" * len(comparisons))
    for qid in sorted(gap):
        cells = ["%.3f" % gap[qid][l] if l in gap[qid] else "-" for l in comparisons]
        L.append("| %s | %s |" % (qid, " | ".join(cells)))
    L.append("")
    L.append("## Fused steady-state distribution (ns), by format")
    L.append("")
    L.append("| format | cells | p50 | p95 | p99 |")
    L.append("|---|---|---|---|---|")
    for fmt, v in dist.items():
        L.append("| %s | %d | %.0f | %.0f | %.0f |" % (fmt, v["cells"], v["p50"], v["p95"], v["p99"]))
    L.append("")
    L.append("## Cold (fresh-process wall, ns)")
    L.append("")
    for lane, v in cold_overall.items():
        L.append("- %s: median %.0f ns" % (lane, v))
    L.append("")
    L.append("## Correctness (oracle agreement)")
    L.append("")
    L.append("- comparisons: %d, disagreements: %d, ok: %s" % (len(agree), len(ag_mismatch), agreement_ok))
    if ag_mismatch:
        for x in ag_mismatch[:20]:
            L.append("  - MISMATCH %s" % json.dumps(x, sort_keys=True))
    L.append("")
    L.append("## Exactness")
    L.append("")
    L.append("- materialize --exact == source: %d/%d" % (exact_ok, exact_n))
    with open(os.path.join(campaign, "MATRIX.md"), "w") as f:
        f.write("\n".join(L) + "\n")

    # ---- decision.md
    D = []
    D.append("## Decision")
    D.append("")
    if fair is None:
        D.append("- No read-inclusive pyarrow lane was recorded; the court is inconclusive.")
    elif fair < 1.0:
        D.append("- **The fused CSV kernel is faster than pyarrow on the same work (parse + aggregate):**"
                 " median fused/pyarrow(read-inclusive) = **%.3f** (< 1.0)." % fair)
    else:
        D.append("- The fused CSV kernel is still slower than pyarrow read-inclusive at the median:"
                 " **%.3f**." % fair)
    legacy = estimator.get("pyarrow_agg", {}).get("median")
    if legacy is not None:
        D.append("- Against the Phase-30.1 *aggregation-only* pyarrow lane (continuity), fused/pyarrow = %.3f." % legacy)
    serial_fair = estimator_serial.get("pyarrow_read", {}).get("median")
    if serial_fair is not None:
        D.append("- The serial scanner (`--workers 1`) against pyarrow read-inclusive is %.3f, so the parallel split supplies the remaining margin." % serial_fair)
    D.append("- Exactness: %d/%d fixtures byte-exact after the source and descriptor are deleted." % (exact_ok, exact_n))
    D.append("- Oracle agreement: %s (%d comparisons)." % ("ok" if agreement_ok else "FAILED", len(agree)))
    with open(os.path.join(campaign, "decision.md"), "w") as f:
        f.write("\n".join(D) + "\n")

    receipt = {
        "campaign": campaign,
        "phase": env.get("phase"),
        "mode": "csv",
        "verdict": verdict,
        "court_status": "PASS",
        "agreement": {"comparisons": len(agree), "disagreements": len(ag_mismatch), "ok": agreement_ok},
        "exactness": {"n": exact_n, "ok": exact_ok},
        "estimator": estimator,
        "estimator_serial": estimator_serial,
        "headline": headline,
        "large_csv_gap": gap,
        "steady_distribution_ns": dist,
        "cold": {"overall_ns": cold_overall},
        "declines": declines,
        "env": env,
    }
    with open(os.path.join(campaign, "receipt.json"), "w") as f:
        json.dump(receipt, f, sort_keys=True, indent=1)
    return 0


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)

    o = sub.add_parser("oracle")
    o.add_argument("lane", choices=["pyarrow", "duckdb"])
    o.add_argument("--source", required=True)
    o.add_argument("--column", required=True)
    o.add_argument("--kind", required=True)
    o.add_argument("--op", required=True)
    o.add_argument("--gt", required=True)
    o.add_argument("--iters", type=int, default=9)
    o.add_argument("--warmup", type=int, default=3)
    o.add_argument("--threads", type=int, default=4)
    o.add_argument("--out", required=True)
    o.set_defaults(func=cmd_oracle)

    c = sub.add_parser("collect")
    c.add_argument("--raw", required=True)
    c.add_argument("--reps", type=int, required=True)
    c.set_defaults(func=cmd_collect)

    g = sub.add_parser("aggregate")
    g.add_argument("--raw", required=True)
    g.add_argument("--campaign", required=True)
    g.add_argument("--env", required=True)
    g.set_defaults(func=cmd_aggregate)

    a = ap.parse_args()
    return a.func(a)


if __name__ == "__main__":
    sys.exit(main())
