#!/usr/bin/env python3
# Phase 19.1 — repeatability court aggregator (paired, interleaved, N repetitions).
#
# ## Why this file exists
#
# The Phase-18 contract court (tools/phase18-contract-packed-court.sh) reported
# two headline numbers — a VOLE/SQLite **build** ratio of 0.82x and a warm
# heterogeneous-session ratio of 1.09x — from a SINGLE run reduced best-of-3
# (min). With no repetition and no paired/interleaved design those numbers are
# point estimates with no interval, and the host store is a noisy bind mount
# whose one-sided stalls the min reduction hides. This court replaces them with
# properly-estimated, qualified numbers.
#
# ## What this file is
#
# The *aggregator* plus a thin proxy to the phase-18 fixture. The SQLite lane
# (build / query SQL / the retained-blob materialization) is delegated byte for
# byte to tools/fixtures/phase18-contract-packed.py, so this court measures the
# SAME source-retaining baseline as Phase 18 (the "SAME SQLite lane" the phase
# requires) and only the repetition design and the statistics differ.
#
# Raw tables the court writes (all wall times are integer microseconds, `us`):
#
#   raw/build_samples.tsv : id fmt lane depth rep rc us
#       lane=vole   -> one-time `field-build --profile runtime --packed`, depth=all
#       lane=sqlite -> escalating build `--through D`, depth=0..5
#   raw/cold_samples.tsv  : id fmt lane depth rep obs rc us   (one process per obs)
#   raw/warm_samples.tsv  : id fmt lane depth rep rc us rss_kb n   (one session)
#   raw/exact.tsv         : id fmt v_ok v_rc v_us sql_ok sql_rc sql_us
#
# ## Estimators (stated, not implied)
#
# * Per (document, lane, depth) and per (lane, depth pooled): n, median, mean,
#   p25, p75, min, max, coefficient of variation (sample sd / mean).
# * Per paired rep: ratio = VOLE / SQLite for the same rep index. Odd reps run
#   VOLE first, even reps SQLite first (recorded in raw/order.tsv), so the two
#   lanes see the same cache/drift conditions up to order.
# * Across documents: the median and geometric-mean paired ratio over all
#   (document, rep) samples, with a **cluster bootstrap** 95% CI that resamples
#   the 12 documents (with replacement) and pools their per-rep ratios, fixed
#   seed, >= 10,000 resamples. Resampling documents (not doc-rep pairs) is the
#   conservative choice: it keeps a document's repeated measures together and
#   does not pretend 10 reps of one file are 10 independent files.
# * Wins / ties / losses are per DOCUMENT (its median paired ratio) under an
#   explicit +/-10% tie band (TIE): win = ratio < 1-TIE, loss = ratio > 1+TIE,
#   tie otherwise. A ratio < 1 favours VOLE (faster build / smaller query time).
# * The phase-18 estimator is also reported (ratio of SUMS, best-of-3 min vs
#   median-of-10) so the change from the old point estimate is visible.

import argparse
import importlib.util
import math
import os
import random
import statistics
import sys


# ---------------------------------------------------------------------------
# Phase-18 delegation (the SQLite lane and the VOLE store/query surface)
# ---------------------------------------------------------------------------

def _load_phase18():
    here = os.path.dirname(os.path.abspath(__file__))
    path = os.path.join(here, "phase18-contract-packed.py")
    spec = importlib.util.spec_from_file_location("phase18_contract_packed", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


P18 = _load_phase18()

DELEGATED = {"build", "query", "session", "materialize", "extract", "sqlgen"}


# ---------------------------------------------------------------------------
# Small statistics helpers
# ---------------------------------------------------------------------------

def read_tsv(path):
    if not os.path.exists(path):
        return []
    with open(path) as fh:
        hdr = fh.readline().rstrip("\n").split("\t")
        rows = []
        for line in fh:
            if not line.strip():
                continue
            vals = line.rstrip("\n").split("\t")
            if len(vals) != len(hdr):
                continue
            rows.append(dict(zip(hdr, vals)))
        return rows


def percentile(vals, p):
    v = sorted(float(x) for x in vals)
    if not v:
        return 0.0
    if len(v) == 1:
        return v[0]
    k = (len(v) - 1) * p
    f = math.floor(k)
    c = math.ceil(k)
    if f == c:
        return v[int(k)]
    return v[f] * (c - k) + v[c] * (k - f)


def geomean(vals):
    v = [float(x) for x in vals if float(x) > 0]
    if not v:
        return 0.0
    return math.exp(sum(math.log(x) for x in v) / len(v))


def summarize(vals):
    v = [float(x) for x in vals]
    n = len(v)
    if n == 0:
        return {"n": 0, "median": 0.0, "mean": 0.0, "p25": 0.0, "p75": 0.0,
                "min": 0.0, "max": 0.0, "cv": 0.0}
    mean = statistics.fmean(v)
    sd = statistics.stdev(v) if n > 1 else 0.0
    return {"n": n, "median": statistics.median(v), "mean": mean,
            "p25": percentile(v, 0.25), "p75": percentile(v, 0.75),
            "min": min(v), "max": max(v), "cv": (sd / mean if mean else 0.0)}


def cluster_bootstrap(by_doc, stat, B, seed):
    """Resample documents with replacement; pool their per-rep values; return
    (lo, hi) at the 2.5/97.5 percentiles of the statistic, plus the draws."""
    rng = random.Random(seed)
    docs = sorted(by_doc.keys())
    draws = []
    for _ in range(B):
        sample = []
        for _ in range(len(docs)):
            sample.extend(by_doc[rng.choice(docs)])
        draws.append(stat(sample))
    draws.sort()
    lo = draws[max(0, int(0.025 * B) - 1)]
    hi = draws[min(B - 1, int(0.975 * B))]
    return lo, hi, draws


# ---------------------------------------------------------------------------
# Collection
# ---------------------------------------------------------------------------

def collect_build_or_warm(rows):
    """rc-0 rows only: lane -> depth -> id -> rep -> us (last write wins)."""
    out = {}
    for r in rows:
        if r.get("rc") != "0":
            continue
        lane, depth, idv = r["lane"], r["depth"], r["id"]
        out.setdefault(lane, {}).setdefault(depth, {}).setdefault(idv, {})[int(r["rep"])] = int(r["us"])
    return out


def collect_cold(rows):
    """All rows: lane -> depth -> id -> rep -> sum(us) over observations."""
    out = {}
    for r in rows:
        lane, depth, idv = r["lane"], r["depth"], r["id"]
        rep = int(r["rep"])
        cell = out.setdefault(lane, {}).setdefault(depth, {}).setdefault(idv, {})
        cell[rep] = cell.get(rep, 0) + int(r["us"])
    return out


def flatten(series, lane, depths):
    """lane's chosen depths, summed per (id, rep). -> id -> rep -> us."""
    out = {}
    for d in depths:
        for idv, repmap in series.get(lane, {}).get(d, {}).items():
            for rep, us in repmap.items():
                out.setdefault(idv, {}).setdefault(rep, 0)
                out[idv][rep] += us
    return out


def pair_by_doc(numer, denom):
    """-> {id: [ratio per shared rep]}, only reps both lanes answered."""
    out = {}
    for idv in sorted(set(numer) & set(denom)):
        ratios = []
        for rep in sorted(set(numer[idv]) & set(denom[idv])):
            n, d = numer[idv][rep], denom[idv][rep]
            if d > 0 and n > 0:
                ratios.append(n / d)
        if ratios:
            out[idv] = ratios
    return out


def sum_reduced(flat, ids, reducer):
    tot = 0.0
    for idv in ids:
        vals = [flat[idv][r] for r in sorted(flat.get(idv, {}))]
        if vals:
            tot += reducer(vals)
    return tot


def reduce_min3(vals):
    return min(vals[:3]) if len(vals) >= 3 else min(vals)


def reduce_median(vals):
    return statistics.median(vals)


def gof(vals, denom):
    return (vals / denom) if denom else 0.0


# ---------------------------------------------------------------------------
# Report
# ---------------------------------------------------------------------------

def _pooled_table(lines, title, series, depths, lanes=("vole", "sqlite")):
    lines.append("### " + title)
    lines.append("")
    lines.append("| lane | depth | n | median ms | mean ms | p25 ms | p75 ms | min ms | max ms | CV |")
    lines.append("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|")
    for lane in lanes:
        for d in depths:
            cells = series.get(lane, {}).get(d, {})
            vals = [us for repmap in cells.values() for us in repmap.values()]
            if not vals:
                continue
            s = summarize(vals)
            dlabel = "all" if d == "all" else "C" + d
            lines.append("| {} | {} | {} | {:.3f} | {:.3f} | {:.3f} | {:.3f} | {:.3f} | {:.3f} | {:.1f}% |".format(
                "VOLE" if lane == "vole" else "SQLite", dlabel, s["n"],
                s["median"] / 1000, s["mean"] / 1000, s["p25"] / 1000,
                s["p75"] / 1000, s["min"] / 1000, s["max"] / 1000, 100.0 * s["cv"]))
    lines.append("")


def _across_table(lines, title, by_doc, seed, B, tie):
    lines.append("### " + title)
    lines.append("")
    pooled = [r for d in by_doc for r in by_doc[d]]
    if not pooled:
        lines.append("- **NO PAIRED SAMPLES** (a lane produced no rc-0 measurements); nothing to "
                     "estimate. This is reported as a gap, never as parity.")
        lines.append("")
        return {"median": 0.0, "ci_median": (0.0, 0.0), "geomean": 0.0,
                "ci_geomean": (0.0, 0.0), "wins": 0, "ties": 0, "losses": 0, "docmed": {}}
    med = statistics.median(pooled)
    gm = geomean(pooled)
    lo_m, hi_m, _ = cluster_bootstrap(by_doc, statistics.median, B, seed)
    lo_g, hi_g, _ = cluster_bootstrap(by_doc, geomean, B, seed + 1)
    docmed = {d: statistics.median(v) for d, v in by_doc.items()}
    ties = [d for d, m in docmed.items() if (1 - tie) <= m <= (1 + tie)]
    wins = [d for d, m in docmed.items() if m < (1 - tie)]
    losses = [d for d, m in docmed.items() if m > (1 + tie)]
    lines.append("- documents (clusters): **{}**; paired samples: **{}**".format(len(by_doc), len(pooled)))
    lines.append("- median paired ratio: **{:.3f}**  (95% CI {:.3f}..{:.3f})".format(med, lo_m, hi_m))
    lines.append("- geometric-mean paired ratio: **{:.3f}**  (95% CI {:.3f}..{:.3f})".format(gm, lo_g, hi_g))
    lines.append("- per-document tie band +/-{:.0f}%: **{} win / {} tie / {} loss** "
                 "(win = ratio < {:.2f}, VOLE favourable; loss = ratio > {:.2f})".format(
                     100 * tie, len(wins), len(ties), len(losses), 1 - tie, 1 + tie))
    lines.append("")
    lines.append("| document | median ratio | samples | outcome |")
    lines.append("|---|---:|---:|---|")
    for d in sorted(docmed):
        m = docmed[d]
        outcome = "win" if m < (1 - tie) else ("loss" if m > (1 + tie) else "tie")
        lines.append("| {} | {:.3f} | {} | {} |".format(d, m, len(by_doc[d]), outcome))
    lines.append("")
    return {"median": med, "ci_median": (lo_m, hi_m), "geomean": gm,
            "ci_geomean": (lo_g, hi_g), "wins": len(wins), "ties": len(ties),
            "losses": len(losses), "docmed": docmed}


def aggregate(raw, campaign, reps, boot, seed, tie):
    build = read_tsv(os.path.join(raw, "build_samples.tsv"))
    cold = read_tsv(os.path.join(raw, "cold_samples.tsv"))
    warm = read_tsv(os.path.join(raw, "warm_samples.tsv"))
    exact = read_tsv(os.path.join(raw, "exact.tsv"))
    order = read_tsv(os.path.join(raw, "order.tsv"))
    env = {}
    try:
        import json as _json
        env = _json.load(open(os.path.join(raw, "environment.json")))
    except (OSError, ValueError):
        pass

    build_s = collect_build_or_warm(build)
    warm_s = collect_build_or_warm(warm)
    cold_s = collect_cold(cold)

    ids = sorted({r["id"] for r in build})
    depths = sorted({int(r["depth"]) for r in build if r["lane"] == "sqlite"})
    dstr = [str(d) for d in depths]
    lastdep = str(depths[-1]) if depths else "5"

    lines = []
    lines.append("# Phase 19.1 — repeatability court (paired, interleaved, N={})".format(reps))
    lines.append("")
    lines.append("Replaces the two single-shot Phase-18.5 point estimates with paired, "
                 "interleaved, repeated measurements and a bootstrap interval. Same 12-document "
                 "subset, same contract depths C0–C5, same source-retaining SQLite lane "
                 "(`tools/fixtures/phase18-contract-packed.py`, byte-identical), same accounting "
                 "(persistent bytes = sum of regular-file sizes; never `du -sb`). Wall times are "
                 "integer microseconds.")
    lines.append("")
    lines.append("- documents: **{}** ({}); depths: **C0..C{}**; repetitions: **N={}**; "
                 "bootstrap: **{} resamples, seed {}**, cluster-resampled by document; "
                 "tie band **+/-{:.0f}%**.".format(
                     len(ids), ", ".join(ids), depths[-1] if depths else 5, reps,
                     boot, seed, 100 * tie))
    lines.append("- interleaving: odd reps VOLE-then-SQLite, even reps SQLite-then-VOLE "
                 "(per-rep order in `raw/order.tsv`); a per-rep warm-up precedes each lane's "
                 "timed queries so the first read of a freshly rewritten store is not timed.")
    lines.append("- every individual sample is retained in `raw/*_samples.tsv`; nothing is "
                 "reduced to min/median at collection time.")
    lines.append("")

    # --- pooled per (lane, depth) stats ----------------------------------
    lines.append("## Per-lane statistics")
    lines.append("")
    _pooled_table(lines, "One-time build wall (pooled over documents x reps)",
                  build_s, ["all"] + dstr)
    _pooled_table(lines, "Warm single-session wall, all observations (pooled over documents x reps)",
                  warm_s, dstr)
    _pooled_table(lines, "Cold per-observation wall, summed per rep (pooled over documents x reps)",
                  cold_s, dstr)

    # --- per (doc, lane, depth) CSV --------------------------------------
    per_csv = os.path.join(raw, "per_doc_lane_depth.csv")
    with open(per_csv, "w") as fh:
        fh.write("kind,id,lane,depth,n,median_ms,mean_ms,p25_ms,p75_ms,min_ms,max_ms,cv\n")
        for kind, series, dset in (("build", build_s, ["all"] + dstr),
                                   ("warm", warm_s, dstr), ("cold", cold_s, dstr)):
            for lane in ("vole", "sqlite"):
                for d in dset:
                    for idv in ids:
                        repmap = series.get(lane, {}).get(d, {}).get(idv)
                        if not repmap:
                            continue
                        s = summarize(repmap.values())
                        fh.write("{},{},{},{},{},{:.3f},{:.3f},{:.3f},{:.3f},{:.3f},{:.3f},{:.4f}\n".format(
                            kind, idv, lane, d, s["n"], s["median"] / 1000, s["mean"] / 1000,
                            s["p25"] / 1000, s["p75"] / 1000, s["min"] / 1000, s["max"] / 1000, s["cv"]))
    lines.append("Full per (document, lane, depth) statistics: `raw/per_doc_lane_depth.csv`.")
    lines.append("")

    # --- Q1: build ratio --------------------------------------------------
    b_vole = flatten(build_s, "vole", ["all"])
    q1_by_depth = {}
    for d in dstr:
        q1_by_depth[d] = pair_by_doc(b_vole, flatten(build_s, "sqlite", [d]))
    lines.append("## Q1 — VOLE/SQLite BUILD ratio (paired per rep)")
    lines.append("")
    lines.append("| SQLite depth | docs | pooled samples | median ratio | geometric-mean ratio |")
    lines.append("|---|---:|---:|---:|---:|")
    for d in dstr:
        bd = q1_by_depth[d]
        pooled = [r for x in bd.values() for r in x]
        lines.append("| C{} | {} | {} | {:.3f} | {:.3f} |".format(
            d, len(bd), len(pooled),
            statistics.median(pooled) if pooled else 0.0, geomean(pooled)))
    lines.append("")
    q1 = _across_table(lines, "Q1 headline — VOLE/SQLite build vs SQLite C{} (full contract)".format(lastdep),
                       q1_by_depth[lastdep], seed, boot, tie)
    # ratio-of-sums best-of-3 vs median-of-10
    sv = sum_reduced(b_vole, ids, reduce_min3); ss = sum_reduced(flatten(build_s, "sqlite", [lastdep]), ids, reduce_min3)
    mv = sum_reduced(b_vole, ids, reduce_median); ms = sum_reduced(flatten(build_s, "sqlite", [lastdep]), ids, reduce_median)
    lines.append("### Best-of-3 (min) vs median-of-{} — build (ratio of sums, the Phase-18 estimator)".format(reps))
    lines.append("")
    lines.append("| reduction | VOLE build sum ms | SQLite C{} build sum ms | ratio |".format(lastdep))
    lines.append("|---|---:|---:|---:|")
    lines.append("| phase-18 best-of-3 (min of first 3) | {:.1f} | {:.1f} | **{:.3f}** |".format(sv / 1000, ss / 1000, gof(sv, ss)))
    lines.append("| this court median-of-{} | {:.1f} | {:.1f} | **{:.3f}** |".format(reps, mv / 1000, ms / 1000, gof(mv, ms)))
    lines.append("")
    lines.append("The min reduction shrinks BOTH lanes' sums; because the heavy store-write tail lives "
                 "on the VOLE lane, min-of-3 is expected to flatter VOLE relative to median-of-{}.".format(reps))
    lines.append("")

    # --- Q2: warm ratio ---------------------------------------------------
    w_vole = flatten(warm_s, "vole", dstr)
    w_sql = flatten(warm_s, "sqlite", dstr)
    q2_depth = {}
    for d in dstr:
        q2_depth[d] = pair_by_doc(flatten(warm_s, "vole", [d]), flatten(warm_s, "sqlite", [d]))
    lines.append("## Q2 — VOLE/SQLite WARM heterogeneous-session ratio (paired per rep)")
    lines.append("")
    lines.append("| depth | docs | pooled samples | median ratio | geometric-mean ratio |")
    lines.append("|---|---:|---:|---:|---:|")
    for d in dstr:
        bd = q2_depth[d]
        pooled = [r for x in bd.values() for r in x]
        lines.append("| C{} | {} | {} | {:.3f} | {:.3f} |".format(
            d, len(bd), len(pooled),
            statistics.median(pooled) if pooled else 0.0, geomean(pooled)))
    lines.append("")
    q2 = _across_table(lines, "Q2 headline — VOLE/SQLite warm, summed over C0..C{} per rep".format(lastdep),
                       pair_by_doc(w_vole, w_sql), seed, boot, tie)
    wsv = sum_reduced(w_vole, ids, reduce_min3); wss = sum_reduced(w_sql, ids, reduce_min3)
    wmv = sum_reduced(w_vole, ids, reduce_median); wms = sum_reduced(w_sql, ids, reduce_median)
    lines.append("### Best-of-3 (min) vs median-of-{} — warm (ratio of sums, the Phase-18 estimator)".format(reps))
    lines.append("")
    lines.append("| reduction | VOLE warm sum ms | SQLite warm sum ms | ratio |")
    lines.append("|---|---:|---:|---:|")
    lines.append("| phase-18 best-of-3 (min of first 3) | {:.1f} | {:.1f} | **{:.3f}** |".format(wsv / 1000, wss / 1000, gof(wsv, wss)))
    lines.append("| this court median-of-{} | {:.1f} | {:.1f} | **{:.3f}** |".format(reps, wmv / 1000, wms / 1000, gof(wmv, wms)))
    lines.append("")

    # --- raw paired tables -------------------------------------------------
    for name, numer, denom in (("paired_build", b_vole, flatten(build_s, "sqlite", [lastdep])),
                               ("paired_warm", w_vole, w_sql)):
        with open(os.path.join(raw, name + ".csv"), "w") as fh:
            fh.write("id,rep,vole_us,sqlite_us,ratio\n")
            for idv in ids:
                for rep in sorted(set(numer.get(idv, {})) & set(denom.get(idv, {}))):
                    n, d = numer[idv][rep], denom[idv][rep]
                    if d > 0:
                        fh.write("{},{},{},{},{:.5f}\n".format(idv, rep, n, d, n / d))

    # --- exactness ---------------------------------------------------------
    lines.append("## Exact original closure (length + SHA-256 + byte compare)")
    lines.append("")
    lines.append("| id | fmt | VOLE ok | VOLE rc | VOLE ms | SQLite ok | SQLite rc | SQLite ms |")
    lines.append("|---|---|---|---:|---:|---|---:|---:|")
    for r in exact:
        lines.append("| {} | {} | {} | {} | {:.1f} | {} | {} | {:.1f} |".format(
            r["id"], r["fmt"], r["v_ok"], r["v_rc"], int(r["v_us"]) / 1000,
            r["sql_ok"], r["sql_rc"], int(r["sql_us"]) / 1000))
    n = len(exact)
    nv = sum(1 for r in exact if r["v_ok"] == "1")
    ns = sum(1 for r in exact if r["sql_ok"] == "1")
    lines.append("")
    lines.append("VOLE `materialize --exact --packed`: **{}/{} byte-exact**. "
                 "SQLite retained blob: **{}/{} byte-exact**.".format(nv, n, ns, n))
    lines.append("")

    # --- explicit answers --------------------------------------------------
    lines.append("## Answers")
    lines.append("")
    lines.append("**Q1 — is the VOLE/SQLite BUILD ratio < 1.0?**")
    lines.append("")
    lo, hi = q1["ci_median"]
    lines.append("- Paired median ratio **{:.3f}** (95% CI {:.3f}..{:.3f}); geometric mean "
                 "**{:.3f}** (95% CI {:.3f}..{:.3f}). Per-document: {} win / {} tie / {} loss."
                 .format(q1["median"], lo, hi, q1["geomean"], q1["ci_geomean"][0],
                         q1["ci_geomean"][1], q1["wins"], q1["ties"], q1["losses"]))
    if hi < 1.0:
        lines.append("- The 95% CI lies **entirely below 1.0**: on this subset VOLE builds the equal "
                     "contract faster than SQLite, and the effect is not within parity at these depths.")
    elif lo > 1.0:
        lines.append("- The 95% CI lies **entirely above 1.0**: VOLE's build is slower, not < 1.0.")
    else:
        lines.append("- The 95% CI **crosses 1.0**: the sign of the build ratio is NOT established by "
                     "this design; the point estimate is {} but parity is not excluded.".format(q1["median"]))
    lines.append("")
    lines.append("**Q2 — is the VOLE/SQLite WARM heterogeneous-session ratio > 1.0, or parity?**")
    lines.append("")
    lo2, hi2 = q2["ci_median"]
    lines.append("- Paired median ratio **{:.3f}** (95% CI {:.3f}..{:.3f}); geometric mean "
                 "**{:.3f}** (95% CI {:.3f}..{:.3f}). Per-document: {} win / {} tie / {} loss."
                 .format(q2["median"], lo2, hi2, q2["geomean"], q2["ci_geomean"][0],
                         q2["ci_geomean"][1], q2["wins"], q2["ties"], q2["losses"]))
    if lo2 > 1.0:
        lines.append("- The 95% CI lies **entirely above 1.0**: a real warm-session loss for VOLE.")
    elif hi2 < 1.0:
        lines.append("- The 95% CI lies **entirely below 1.0**: VOLE's warm session is faster.")
    else:
        lines.append("- The 95% CI **includes 1.0**: warm latency is **indistinguishable from parity** "
                     "at this sample size on this store and host — the Phase-18 1.09x is a point "
                     "estimate, not a resolved loss.")
    lines.append("")

    # --- caveats -----------------------------------------------------------
    lines.append("## Caveats and honesty")
    lines.append("")
    lines.append("- The bind-mounted host store is noisy; the per-document CVs in "
                 "`raw/per_doc_lane_depth.csv` quantify that noise and are the reason the intervals "
                 "are quoted at all. Wide intervals are reported as wide, not massaged.")
    lines.append("- Warm sessions run in the low-millisecond range, where process start-up is a "
                 "material fraction of the wall; that start-up is part of BOTH lanes' measured cost "
                 "and is not subtracted.")
    lines.append("- The previous 0.82x / 1.09x came from one run with a min-of-3 reduction; the "
                 "min-of-3 vs median-of-10 rows above show how much that reduction moved each lane.")
    lines.append("- Sampling unit for the CI is the document (cluster bootstrap), not the doc-rep "
                 "pair; a document's 10 repeated measures are correlated and are not treated as 10 "
                 "independent documents.")
    if order:
        seen = sorted({r["order"] for r in order})
        lines.append("- recorded interleave orders: {}.".format("; ".join(seen)))
    lines.append("")

    report = "\n".join(lines)
    with open(os.path.join(campaign, "SUMMARY.md"), "w") as fh:
        fh.write(report)
    print(report)
    return 0


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    if argv and argv[0] in DELEGATED:
        return P18.main(argv)
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    ag = sub.add_parser("aggregate")
    ag.add_argument("raw")
    ag.add_argument("campaign")
    ag.add_argument("--reps", type=int, default=10)
    ag.add_argument("--boot", type=int, default=20000)
    ag.add_argument("--seed", type=int, default=190019)
    ag.add_argument("--tie", type=float, default=0.10)
    ns = ap.parse_args(argv)
    if ns.cmd == "aggregate":
        return aggregate(ns.raw, ns.campaign, ns.reps, ns.boot, ns.seed, ns.tie)
    return 2


if __name__ == "__main__":
    sys.exit(main())
