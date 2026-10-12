#!/usr/bin/env python3
# Phase 29.1 — decision Experiment 2: the generalized fused kernel across formats.
#
# This is the analytical-lane side of the experiment. It supplies the two
# competitor lanes (pyarrow reading the real Arrow/Parquet/CSV file directly, and
# DuckDB reading the real data), normalizes every lane's raw artifacts into unified
# sample/answer tables (`collect`), and reduces them with the ADR-0054 estimator
# (`aggregate`): paired per-(fixture,format,question) ratios, median + geometric
# mean, a fixed-seed cluster bootstrap 95% CI, and the MDE. Correctness is checked
# against pyarrow/DuckDB as the oracle: fused count/sum/min/max/avg/grouped must
# match exactly (floats within 1e-9 relative).
#
# The fused kernel itself is measured by the `fused-scan` subcommand of the VOLE
# binary (in-process steady-state, interleaved with the shipped whole-column text
# path for attribution); this driver only reads what the court recorded.
#
# No third-party imports except `pyarrow` / `duckdb`, both pinned in the
# `analytical` service.

import argparse
import importlib.util as _ilu
import json
import os
import statistics
import subprocess
import sys
import time

OPS = ["count", "sum", "min", "max", "avg", "grouped"]
AGGS = {"count", "sum", "min", "max", "avg", "grouped"}


def _load(name, fname):
    here = os.path.dirname(os.path.abspath(__file__))
    spec = _ilu.spec_from_file_location(name, os.path.join(here, fname))
    mod = _ilu.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


H = _load("phase28_2_harness", "phase28-2-harness.py")
P19 = _load("phase19_repeat", "phase19-repeat.py")


def now_ns():
    return H.now_ns()


def steady(fn, warmup, iters):
    return H.steady_state_ns(fn, warmup=warmup, iters=iters)


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
        return a == b
    if isinstance(a, float) or isinstance(b, float):
        return abs(float(a) - float(b)) <= 1e-9 * max(1.0, abs(float(a)))
    return int(a) == int(b)


def _cast_gt(gt, kind):
    return float(gt) if kind == "float" else int(gt)


def _dump(path, obj):
    with open(path, "w") as f:
        json.dump(obj, f, sort_keys=True)


# ---------------------------------------------------------------------------
# pyarrow lane
# ---------------------------------------------------------------------------
def _pa_table(path, fmt):
    if fmt == "arrow":
        import pyarrow.ipc as ipc

        return ipc.open_file(path).read_all()
    if fmt == "parquet":
        import pyarrow.parquet as pq

        return pq.read_table(path)
    import pyarrow.csv as pacsv

    return pacsv.read_csv(path)


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


def cmd_pyarrow(a):
    table = _pa_table(a.source, a.format)
    value = _pa_answer(table, a.column, a.op, a.gt, a.kind)
    ns = steady(
        lambda: _pa_answer(table, a.column, a.op, a.gt, a.kind),
        a.warmup,
        a.iters,
    )
    _dump(a.out, {"lane": "pyarrow", "ns": ns, "value": value})


# ---------------------------------------------------------------------------
# DuckDB lane
# ---------------------------------------------------------------------------
def _duckdb_con(fmt, path, threads):
    import duckdb

    con = duckdb.connect()
    con.execute("PRAGMA threads=%d" % threads)
    q = path.replace("'", "''")
    if fmt == "parquet":
        con.execute("CREATE VIEW t AS SELECT * FROM read_parquet('%s')" % q)
    elif fmt == "csv":
        con.execute("CREATE VIEW t AS SELECT * FROM read_csv_auto('%s')" % q)
    else:
        import pyarrow.ipc as ipc

        con.register("t", ipc.open_file(path).read_all())
    return con


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
    v = rows[0][0]
    return _num(v)


def cmd_duckdb(a):
    con = _duckdb_con(a.format, a.source, a.threads)
    value = _duckdb_answer(con, a.column, a.op, a.gt)
    ns = steady(
        lambda: _duckdb_answer(con, a.column, a.op, a.gt),
        a.warmup,
        a.iters,
    )
    _dump(a.out, {"lane": "duckdb", "ns": ns, "value": value})


# ---------------------------------------------------------------------------
# collect: normalize every raw artifact into unified tables
# ---------------------------------------------------------------------------
def _read_tsv(path):
    out = []
    with open(path) as f:
        head = f.readline().rstrip("\n").split("\t")
        for line in f:
            if not line.strip():
                continue
            parts = line.rstrip("\n").split("\t")
            out.append(dict(zip(head, parts)))
    return out


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

    def emit_samples(fx, fmt, qid, lane, rep, us_list):
        for i, u in enumerate(us_list):
            samples.write(
                "%s\t%s\t%s\t%s\t%d\t%d\t%d\n" % (fx, fmt, qid, lane, rep, i, int(u))
            )

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
            # fused (its own samples + the interleaved in-process current path)
            p = os.path.join(raw, "fused", "%s.%d.json" % (base, rep))
            d = json.load(open(p))
            emit_samples(fx, fmt, qid, "fused", rep, d.get("fused_ns", []))
            emit_answer(fx, fmt, qid, "fused", rep, fused_value(d, op), d.get("declined"))
            if d.get("current_available") and not d.get("current_declined"):
                emit_samples(fx, fmt, qid, "current", rep, d.get("current_ns", []))
                emit_answer(fx, fmt, qid, "current", rep, d.get("current_count"), False)
            for lane in ("pyarrow", "duckdb"):
                p = os.path.join(raw, lane, "%s.%d.json" % (base, rep))
                if not os.path.exists(p):
                    continue
                c = json.load(open(p))
                emit_samples(fx, fmt, qid, lane, rep, c.get("ns", []))
                emit_answer(fx, fmt, qid, lane, rep, c.get("value"), c.get("value") is None)
    samples.close()
    answers.close()


# ---------------------------------------------------------------------------
# aggregate: the ADR-0054 estimator + reports
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
    exact = _read_tsv(os.path.join(raw, "exact.tsv"))

    clusters = [(r["fixture"], r["format"], r["qid"]) for r in plan]
    # (fixture, format, qid, lane) -> {rep: median us over samples}
    rep_med = {}
    for r in samples:
        key = (r["fixture"], r["format"], r["qid"], r["lane"])
        rep_med.setdefault(key, {}).setdefault(int(r["rep"]), []).append(int(r["ns"]))
    rep_median = {
        k: {rep: statistics.median(v) for rep, v in reps.items()}
        for k, reps in rep_med.items()
    }

    def lane_cluster(lane):
        out = {}
        for (fx, fmt, qid, ln), reps in rep_median.items():
            if ln == lane:
                out.setdefault((fx, fmt, qid), {}).update(reps)
        return out

    fused = lane_cluster("fused")
    fused_fixture = {}
    for (fx, fmt, qid), reps in fused.items():
        fused_fixture.setdefault((fx, fmt, qid), statistics.median(list(reps.values())))

    def paired(numer, denom):
        """cluster -> [ratios per shared rep]; paired per-(fixture,format,qid)."""
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

    B, SEED = 20000, 29115

    def mde(ci):
        lo, hi = ci
        return (1.959963985 + 0.8416212336) / 1.959963985 * ((hi - lo) / 2.0)

    estimator = {}
    for comp in ("pyarrow", "duckdb", "current"):
        by = paired(fused, lane_cluster(comp))
        pooled = [v for vs in by.values() for v in vs]
        if not pooled:
            continue
        lo_m, hi_m, _ = P19.cluster_bootstrap(by, statistics.median, B, SEED)
        lo_g, hi_g, _ = P19.cluster_bootstrap(by, P19.geomean, B, SEED + 1)
        estimator[comp] = {
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

    # the large-input gap to pyarrow, per question
    large = {}
    for c in clusters:
        if c[0] != "e_large":
            continue
        for comp in ("pyarrow", "duckdb"):
            n = fused.get(c)
            d = lane_cluster(comp).get(c)
            if not n or not d:
                continue
            shared = sorted(set(n) & set(d))
            rs = [n[r] / d[r] for r in shared if d[r] > 0]
            if rs:
                large.setdefault((c[1], c[2]), {})[comp] = statistics.median(rs)

    # cold (fresh-process wall) per cell, and the fused steady distribution
    try:
        cold = _read_tsv(os.path.join(raw, "cold.tsv"))
    except OSError:
        cold = []
    cold_rep = {}
    for r in cold:
        cold_rep.setdefault(
            (r["fixture"], r["format"], r["qid"], r["lane"]), {}
        )[int(r["rep"])] = int(r["ns"])
    cold_cell = {k: statistics.median(list(v.values())) for k, v in cold_rep.items()}

    def cold_lane(l):
        return {
            (fx, fmt, qid): m
            for (fx, fmt, qid, ln), m in cold_cell.items()
            if ln == l
        }

    cold_est = {}
    for comp in ("pyarrow", "duckdb"):
        f, d = cold_lane("fused"), cold_lane(comp)
        rs = [f[c] / d[c] for c in f if c in d and d[c] > 0]
        if rs:
            cold_est[comp] = {
                "cells": len(rs),
                "median": statistics.median(rs),
                "geomean": P19.geomean(rs),
            }
    cold_overall = {}
    for lane in ("fused", "pyarrow", "duckdb"):
        vals = list(cold_lane(lane).values())
        if vals:
            cold_overall[lane] = statistics.median(vals)

    fused_dist = {}
    for r in samples:
        if r["lane"] == "fused":
            fused_dist.setdefault((r["fixture"], r["format"], r["qid"]), []).append(
                int(r["ns"])
            )
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

    # answer agreement vs the oracle (pyarrow / duckdb), per (fmt, op)
    ans = {}
    for r in answers:
        key = (r["fixture"], r["format"], r["qid"], r["lane"], int(r["rep"]))
        ans[key] = (json.loads(r["value"]) if r["value"] != "null" else None, r["declined"] == "1")

    agree = []
    for row in plan:
        fx, fmt, qid, op = row["fixture"], row["format"], row["qid"], row["op"]
        c = (fx, fmt, qid)
        fv = ans.get((fx, fmt, qid, "fused", 1))
        if fv is None:
            continue
        fy, fdecl = fv
        for oracle in ("pyarrow", "duckdb"):
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

    # capability accounting: declines per (lane, format, op)
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

    # ---- MATRIX.md --------------------------------------------------------
    def med(c, lane):
        return fused_fixture.get(c) if lane == "fused" else None

    L = []
    L.append("# Phase 29.1 — generalized fused kernel: matrix")
    L.append("")
    L.append("Steady-state medians (ns). Ratio is **fused / lane**; **< 1 favours fused**.")
    L.append("")
    L.append("| fixture | format | question | fused | pyarrow | duckdb | current | fused/pyarrow | fused/duckdb |")
    L.append("|---|---|---|---:|---:|---:|---:|---:|---:|")

    def lane_med(c, lane):
        reps = rep_median.get((c[0], c[1], c[2], lane))
        return statistics.median(list(reps.values())) if reps else None

    for c in clusters:
        fm = fused_fixture.get(c)
        pa = lane_med(c, "pyarrow")
        dd = lane_med(c, "duckdb")
        cu = lane_med(c, "current")
        rp = "%.3f" % (fm / pa) if fm and pa else "-"
        rd = "%.3f" % (fm / dd) if fm and dd else "-"
        L.append(
            "| %s | %s | %s | %s | %s | %s | %s | %s | %s |"
            % (
                c[0],
                c[1],
                c[2],
                "%d" % fm if fm else "-",
                "%d" % pa if pa else "-",
                "%d" % dd if dd else "-",
                "%d" % cu if cu else "-",
                rp,
                rd,
            )
        )
    L.append("")
    L.append("## Paired ratios fused/comparator (median + geomean, 95%% CI by cluster, MDE)")
    L.append("")
    L.append("| comparator | clusters | samples | median | geomean | median 95% CI | geomean 95% CI | MDE | W/T/L |")
    L.append("|---|---:|---:|---:|---:|---|---|---:|---|")
    for name, e in estimator.items():
        L.append(
            "| %s | %d | %d | %.4f | %.4f | %.4f..%.4f | %.4f..%.4f | %.4f | %d/%d/%d |"
            % (
                name,
                e["n_clusters"],
                e["n_samples"],
                e["median"],
                e["geomean"],
                e["median_ci"][0],
                e["median_ci"][1],
                e["geomean_ci"][0],
                e["geomean_ci"][1],
                e["mde"],
                e["wins"],
                e["ties"],
                e["losses"],
            )
        )
    L.append("")
    L.append("## Large-input gap (e_large) — fused / comparator")
    L.append("")
    L.append("| format | question | fused/pyarrow | fused/duckdb |")
    L.append("|---|---|---:|---:|")
    for (fmt, qid), m in sorted(large.items()):
        L.append(
            "| %s | %s | %s | %s |"
            % (
                fmt,
                qid,
                "%.3f" % m["pyarrow"] if "pyarrow" in m else "-",
                "%.3f" % m["duckdb"] if "duckdb" in m else "-",
            )
        )
    L.append("")
    L.append("## Fused steady-state distribution (median of the per-cell percentiles, ns)")
    L.append("")
    L.append("| format | cells | p50 | p95 | p99 |")
    L.append("|---|---:|---:|---:|---:|")
    for fmt, d in sorted(dist.items()):
        L.append(
            "| %s | %d | %.0f | %.0f | %.0f |"
            % (fmt, d["cells"], d["p50"], d["p95"], d["p99"])
        )
    L.append("")
    L.append("## Cold (fresh process per question, same warmup+iters; open + read + op), median ns")
    L.append("")
    L.append("Cold total process wall, so it folds in field open, source materialization, and")
    L.append("the reader startup that the steady median deliberately excludes.")
    L.append("")
    L.append("| lane | median ns |")
    L.append("|---|---:|")
    for lane, m in sorted(cold_overall.items()):
        L.append("| %s | %.0f |" % (lane, m))
    L.append("")
    L.append("| comparator | cells | fused/comparator cold median | cold geomean |")
    L.append("|---|---:|---:|---:|")
    for name, e in sorted(cold_est.items()):
        L.append("| %s | %d | %.4f | %.4f |" % (name, e["cells"], e["median"], e["geomean"]))
    L.append("")
    L.append("## Correctness: fused vs the oracle (pyarrow/DuckDB), per format × operator")
    L.append("")
    L.append("| format | operator | comparisons | disagreements |")
    L.append("|---|---|---:|---:|")
    per_fmt_op = {}
    for x in agree:
        k = (x["format"], x["op"])
        a_, b_ = per_fmt_op.get(k, (0, 0))
        per_fmt_op[k] = (a_ + 1, b_ + (0 if x["equal"] else 1))
    for k, (n, bad) in sorted(per_fmt_op.items()):
        L.append("| %s | %s | %d | %d |" % (k[0], k[1], n, bad))
    L.append("")
    L.append("## Capability accounting: declines (never a wrong answer)")
    L.append("")
    L.append("| lane | format | operator | declines |")
    L.append("|---|---|---|---:|")
    for lane in sorted(declines):
        for fmt in sorted(declines[lane]):
            for op in sorted(declines[lane][fmt]):
                L.append("| %s | %s | %s | %d |" % (lane, fmt, op, declines[lane][fmt][op]))
    if not declines:
        L.append("| — | — | — | 0 (no addressed column fell outside any lane's subset) |")
    L.append("")
    with open(os.path.join(campaign, "MATRIX.md"), "w") as f:
        f.write("\n".join(L) + "\n")

    # ---- decision.md ------------------------------------------------------
    D = []
    D.append("## Verdict")
    D.append("")
    if not agreement_ok:
        D.append(
            "- **Correctness FAILED**: %d fused-vs-oracle disagreements "
            "(see MATRIX.md). The kernel is not admissible." % len(ag_mismatch)
        )
    else:
        D.append(
            "- **Correctness: %d/%d fused answers equal the pyarrow/DuckDB oracle** "
            "across %d (fixture, format, question) clusters." % (len(agree), len(agree), len(clusters))
        )
    if "pyarrow" in estimator:
        e = estimator["pyarrow"]
        resolved = e["median_ci"][1] < 1.0
        D.append(
            "- **fused vs pyarrow**: median **%.3f**, geomean **%.3f**, CI **%.3f..%.3f**, "
            "MDE **%.3f** → %s."
            % (
                e["median"],
                e["geomean"],
                e["median_ci"][0],
                e["median_ci"][1],
                e["mde"],
                "resolved faster" if resolved else "not resolved faster (overall)",
            )
        )
    if "duckdb" in estimator:
        e = estimator["duckdb"]
        resolved = e["median_ci"][1] < 1.0
        D.append(
            "- **fused vs duckdb**: median **%.3f**, geomean **%.3f**, CI **%.3f..%.3f**, "
            "MDE **%.3f** → %s."
            % (
                e["median"],
                e["geomean"],
                e["median_ci"][0],
                e["median_ci"][1],
                e["mde"],
                "resolved faster" if resolved else "not resolved faster (overall)",
            )
        )
    if "current" in estimator:
        e = estimator["current"]
        D.append(
            "- **fused vs the current in-process observe path** (count only): median "
            "**%.3f**, geomean **%.3f**, CI **%.3f..%.3f**, MDE **%.3f**."
            % (e["median"], e["geomean"], e["median_ci"][0], e["median_ci"][1], e["mde"])
        )
    # the large-input honesty
    lp = [(m.get("pyarrow"), k) for k, m in large.items() if "pyarrow" in m]
    if lp:
        worst = max(lp)
        D.append(
            "- **Large-input gap to pyarrow (e_large)**: the worst fused/pyarrow ratio is "
            "**%.3f** (%s); %s."
            % (
                worst[0],
                "%s/%s" % (worst[1][0], worst[1][1]),
                "the gap is closed or reversed"
                if worst[0] <= 1.0
                else "the gap remains — pyarrow's vectorized kernel still wins there, reported honestly",
            )
        )
    D.append("")
    D.append("- Declines are **capability accounting**: a lane that declines is never scored as a win.")
    D.append(
        "- Estimator: paired per-(fixture,format,question) ratio, one per rep (inner samples reduced "
        "to a median); median + geomean with a fixed-seed (%d) cluster bootstrap over %d clusters, "
        "B=%d; MDE from the median-CI half-width (80%% power)." % (SEED, len(clusters), B)
    )
    with open(os.path.join(campaign, "decision.md"), "w") as f:
        f.write("\n".join(D) + "\n")

    receipt = {
        "campaign": campaign,
        "phase": "29.1 — generalized fused kernel (Arrow/Parquet/CSV) decision Experiment 2",
        "env": env,
        "clusters": len(clusters),
        "exactness": {"ok": exact_ok, "n": exact_n},
        "agreement": {
            "comparisons": len(agree),
            "disagreements": len(ag_mismatch),
            "ok": agreement_ok,
        },
        "estimator": {
            k: {
                "n_clusters": v["n_clusters"],
                "n_samples": v["n_samples"],
                "median": v["median"],
                "geomean": v["geomean"],
                "median_ci": list(v["median_ci"]),
                "geomean_ci": list(v["geomean_ci"]),
                "mde": v["mde"],
                "wins": v["wins"],
                "ties": v["ties"],
                "losses": v["losses"],
            }
            for k, v in estimator.items()
        },
        "large_input_gap": {
            "%s/%s" % k: v for k, v in sorted(large.items())
        },
        "cold": {
            "overall_ns": cold_overall,
            "ratios": cold_est,
        },
        "steady_distribution_ns": dist,
        "declines": declines,
        "verdict": "PASS" if (agreement_ok and exact_ok == exact_n and exact_n > 0) else "FAIL",
    }
    _dump(os.path.join(campaign, "receipt.json"), receipt)
    print("PHASE 29.1 FUSED ECON: %s — %s" % (receipt["verdict"], campaign))
    print(json.dumps(receipt["estimator"], sort_keys=True)[:400])
    return 0 if receipt["verdict"] == "PASS" else 1


def main(argv=None):
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("pyarrow")
    p.add_argument("--source", required=True)
    p.add_argument("--format", required=True)
    p.add_argument("--column", required=True)
    p.add_argument("--kind", required=True)
    p.add_argument("--op", required=True)
    p.add_argument("--gt", required=True)
    p.add_argument("--iters", type=int, default=9)
    p.add_argument("--warmup", type=int, default=3)
    p.add_argument("--out", required=True)
    p.set_defaults(func=cmd_pyarrow)

    d = sub.add_parser("duckdb")
    d.add_argument("--source", required=True)
    d.add_argument("--format", required=True)
    d.add_argument("--column", required=True)
    d.add_argument("--kind", required=True)
    d.add_argument("--op", required=True)
    d.add_argument("--gt", required=True)
    d.add_argument("--threads", type=int, default=4)
    d.add_argument("--iters", type=int, default=9)
    d.add_argument("--warmup", type=int, default=3)
    d.add_argument("--out", required=True)
    d.set_defaults(func=cmd_duckdb)

    c = sub.add_parser("collect")
    c.add_argument("--raw", required=True)
    c.add_argument("--reps", type=int, default=5)
    c.set_defaults(func=cmd_collect)

    g = sub.add_parser("aggregate")
    g.add_argument("--raw", required=True)
    g.add_argument("--campaign", required=True)
    g.add_argument("--env", default=None)
    g.set_defaults(func=cmd_aggregate)

    args = ap.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
