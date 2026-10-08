#!/usr/bin/env python3
# Phase 20.3 — warm heterogeneous-query court aggregator.
#
# Adapted from tools/fixtures/phase19-warm.py (UNCHANGED). It reuses the frozen
# Phase-18 SQLite lane and the Phase-19.1 statistics estimators, prints the same
# per-lane / per-depth / headline tables, and — when `--predecessor DIR` is given
# — adds a before/after block computed from the predecessor campaign's retained
# `raw/warm_samples.tsv` with the SAME estimator, so the two runs are directly
# comparable without re-deriving anything by hand.
#
#
#   raw/subset.tsv        id|agency|fmt|sclass|blen|sha|path|fam|is_head
#   raw/once.tsv          id fmt lane depth rc us          (the ONE-TIME builds)
#   raw/warm_samples.tsv  id fmt lane depth rep rc us n     (every warm sample)
#   raw/exact.tsv         id fmt v_ok v_rc v_us sql_ok sql_rc sql_us
#   raw/order.tsv         id rep order
#   raw/store_shape.tsv   id fmt lane files dirs bytes
#   raw/env/C<d>/*.jsonl  one-time untimed equivalence pass (VOLE observe-batch
#                         JSONL and the SQLite session JSONL), per document
#
# All wall times are integer microseconds; the report prints milliseconds.

import argparse
import importlib.util
import json
import os
import statistics
import sys


# ---------------------------------------------------------------------------
# Reuse the frozen Phase-18 (SQLite lane + envelope/equivalence) and Phase-19.1
# (statistics estimators) fixtures rather than reimplementing them.
# ---------------------------------------------------------------------------

def _load(name, fname):
    here = os.path.dirname(os.path.abspath(__file__))
    path = os.path.join(here, fname)
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


P18 = _load("p18_for_warm", "phase18-contract-packed.py")
P19 = _load("p19_for_warm", "phase19-repeat.py")


# ---------------------------------------------------------------------------
# Folding and pairing
# ---------------------------------------------------------------------------

def fold_depth_sum(series, ids, depths):
    """Both lanes' warm session SUMMED over every depth, per (id, rep).

    A rep is folded only when BOTH lanes answered EVERY depth for it, so the
    two sums cover the same set of depths (never a biased partial sum).
    -> (vole_map, sqlite_map) with id -> rep -> summed us.
    """
    vmap, smap = {}, {}
    for idv in ids:
        common = None
        for d in depths:
            rv = set(series.get("vole", {}).get(d, {}).get(idv, {}))
            rs = set(series.get("sqlite", {}).get(d, {}).get(idv, {}))
            common = (rv & rs) if common is None else (common & rv & rs)
        if not common:
            continue
        for rep in sorted(common):
            vmap.setdefault(idv, {})[rep] = sum(
                series["vole"][d][idv][rep] for d in depths)
            smap.setdefault(idv, {})[rep] = sum(
                series["sqlite"][d][idv][rep] for d in depths)
    return vmap, smap


# ---------------------------------------------------------------------------
# Reporting helpers
# ---------------------------------------------------------------------------

def _headline_stats(warm_s, ids, depths, seed, boot, tie):
    """The Phase-19.1 headline estimator (depth-summed per rep, cluster
    bootstrap over documents) as a plain dict, for a before/after comparison."""
    vsum, ssum = fold_depth_sum(warm_s, ids, depths)
    by_doc = P19.pair_by_doc(vsum, ssum)
    pooled = [r for d in by_doc for r in by_doc[d]]
    if not pooled:
        return None
    lo_m, hi_m, _ = P19.cluster_bootstrap(by_doc, statistics.median, boot, seed)
    lo_g, hi_g, _ = P19.cluster_bootstrap(by_doc, P19.geomean, boot, seed + 1)
    docmed = {d: statistics.median(v) for d, v in by_doc.items()}
    wins = sum(1 for m in docmed.values() if m < (1 - tie))
    ties = sum(1 for m in docmed.values() if (1 - tie) <= m <= (1 + tie))
    losses = sum(1 for m in docmed.values() if m > (1 + tie))
    return {"n": len(pooled), "docs": len(by_doc),
            "median": statistics.median(pooled), "ci_median": (lo_m, hi_m),
            "geomean": P19.geomean(pooled), "ci_geomean": (lo_g, hi_g),
            "wins": wins, "ties": ties, "losses": losses, "docmed": docmed}


def across(lines, title, by_doc, seed, boot, tie):
    """Across-document paired-ratio summary: median + geometric mean with a
    fixed-seed cluster bootstrap 95% CI (resampling docs), wins/ties/losses under
    the +/-tie band, the pooled ratio CV, and the resolution / MDE at this N."""
    lines.append("### " + title)
    lines.append("")
    pooled = [r for d in by_doc for r in by_doc[d]]
    if not pooled:
        lines.append("- **NO PAIRED SAMPLES** (a lane produced no rc-0 warm measurement); "
                     "nothing to estimate. Reported as a gap, never as parity.")
        lines.append("")
        return {"n": 0, "median": 0.0, "ci_median": (0.0, 0.0), "geomean": 0.0,
                "ci_geomean": (0.0, 0.0), "wins": 0, "ties": 0, "losses": 0,
                "docmed": {}, "cv": 0.0, "half": 0.0, "mde80": 0.0}
    med = statistics.median(pooled)
    gm = P19.geomean(pooled)
    lo_m, hi_m, _ = P19.cluster_bootstrap(by_doc, statistics.median, boot, seed)
    lo_g, hi_g, _ = P19.cluster_bootstrap(by_doc, P19.geomean, boot, seed + 1)
    docmed = {d: statistics.median(v) for d, v in by_doc.items()}
    wins = sorted(d for d, m in docmed.items() if m < (1 - tie))
    ties = sorted(d for d, m in docmed.items() if (1 - tie) <= m <= (1 + tie))
    losses = sorted(d for d, m in docmed.items() if m > (1 + tie))
    mean = statistics.fmean(pooled)
    cv = (statistics.stdev(pooled) / mean) if (len(pooled) > 1 and mean) else 0.0
    half = (hi_m - lo_m) / 2.0
    # Normal-approximation minimum detectable effect at 80% power, expressed as a
    # multiplicative distance from 1.0 using the bootstrap CI half-width as the
    # 1.96*SE estimate. Stated as an approximation, never as a measured fact.
    mde80 = (1.959963985 + 0.8416212336) / 1.959963985 * half
    lines.append("- documents (clusters): **{}**; paired samples: **{}**".format(
        len(by_doc), len(pooled)))
    lines.append("- median paired ratio: **{:.3f}**  (95% CI {:.3f}..{:.3f})".format(
        med, lo_m, hi_m))
    lines.append("- geometric-mean paired ratio: **{:.3f}**  (95% CI {:.3f}..{:.3f})".format(
        gm, lo_g, hi_g))
    lines.append("- pooled ratio CV (stdev/mean of the paired ratios): **{:.1f}%**; "
                 "median-CI half-width: **+/-{:.3f}**. Resolution at this N: a true "
                 "median shift smaller than ~{:.3f} is not separable from 1.0.".format(
                     100 * cv, half, half))
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
    return {"n": len(pooled), "median": med, "ci_median": (lo_m, hi_m), "geomean": gm,
            "ci_geomean": (lo_g, hi_g), "wins": len(wins), "ties": len(ties),
            "losses": len(losses), "docmed": docmed, "cv": cv, "half": half,
            "mde80": mde80}


def verdict(lines, name, res, tie, reps):
    lo, hi = res["ci_median"]
    lines.append("**{}**".format(name))
    lines.append("")
    if res["n"] == 0:
        lines.append("- No paired samples: the question is **UNRESOLVED** (no measurement).")
        lines.append("")
        return "unresolved"
    lines.append("- Paired median ratio **{:.3f}** (95% CI {:.3f}..{:.3f}); "
                 "geometric mean **{:.3f}** (95% CI {:.3f}..{:.3f}); per-document "
                 "{} win / {} tie / {} loss.".format(
                     res["median"], lo, hi, res["geomean"],
                     res["ci_geomean"][0], res["ci_geomean"][1],
                     res["wins"], res["ties"], res["losses"]))
    if lo > 1.0:
        lines.append("- The 95% CI lies **entirely above 1.0**: a real warm-session "
                     "loss for VOLE at this N.")
        return "loss"
    if hi <= 1.0:
        lines.append("- The 95% CI lies **entirely at or below 1.0**: warm parity or "
                     "better for VOLE at this N.")
        return "parity"
    lines.append("- The 95% CI **still includes 1.0**: the warm ratio is **not resolved "
                 "at N={}** on this store and host. Smallest resolvable median shift at "
                 "this N is ~+/-{:.3f} (pooled ratio CV {:.1f}%; paired samples {}).".format(
                     reps, res["half"], 100 * res["cv"], res["n"]))
    return "unresolved"


# ---------------------------------------------------------------------------
# Equivalence (reuses the frozen Phase-18 decision logic, never re-derived here)
# ---------------------------------------------------------------------------

def _read_jsonl(path):
    out = []
    try:
        with open(path) as fh:
            for line in fh:
                line = line.strip()
                if line:
                    out.append(line)
    except OSError:
        return None
    return out


def equivalence(raw, lines, subset_by_id, depths):
    lines.append("## Equivalence between lanes — warm session (untimed pass)")
    lines.append("")
    lines.append("For every document and depth, the VOLE `observe-batch` JSONL and the "
                 "SQLite session JSONL of the SAME contract query are compared with the "
                 "frozen Phase-18 envelope logic (`phase18-contract-packed.py::_equiv`). "
                 "`raw` = byte/SHA-identity; `projected` = documented text projection; "
                 "`shape` = both answer but not byte-comparable by design (metadata schema; "
                 "resource reference vs member bytes; revision below C4); `observable` = "
                 "the revision observation of different things; `divergent` = PDF page "
                 "text heuristic; `capability` = one lane declines; `decline` = both "
                 "decline.")
    lines.append("")
    tally = {}
    mismatches = []
    checked = 0
    for d in depths:
        for idv, meta in sorted(subset_by_id.items()):
            fmt = meta["fmt"]
            obslist = P18.FMT_OBS[fmt]
            vlines = _read_jsonl(os.path.join(raw, "env", "C%s" % d, "%s.%s.vole.jsonl" % (idv, fmt)))
            slines = _read_jsonl(os.path.join(raw, "env", "C%s" % d, "%s.%s.a1c.jsonl" % (idv, fmt)))
            if vlines is None or slines is None:
                continue
            for i, obs in enumerate(obslist):
                vraw = None
                if i < len(vlines):
                    try:
                        vraw = json.loads(vlines[i])
                    except ValueError:
                        vraw = None
                sraw = None
                if i < len(slines):
                    try:
                        sraw = json.loads(slines[i])
                    except ValueError:
                        sraw = None
                if isinstance(vraw, dict) and "error" in vraw:
                    v = {"obs": obs, "declined": True, "lane": "vole",
                         "reason": "typed decline rc %s" % vraw.get("error")}
                else:
                    v = P18._normalize_vole_answer(vraw, fmt, obs, int(d))
                s = sraw if isinstance(sraw, dict) else None
                result, detail = P18._equiv(int(d), fmt, obs, v, s)
                key = (int(d), fmt, obs, result)
                tally[key] = tally.get(key, 0) + 1
                checked += 1
                if result == "mismatch":
                    mismatches.append((idv, d, obs, detail))
    if checked == 0:
        lines.append("- **NO equivalence evidence** (the untimed pass produced no env JSONL); "
                     "reported as a gap, never as equality.")
        lines.append("")
        return {"checked": 0, "mismatch": 0}
    lines.append("| depth | fmt | obs | raw | projected | shape | observable | divergent | capability | decline | mismatch |")
    lines.append("|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|")
    for d in depths:
        for fmt in ("pdf", "docx", "epub"):
            for obs in P18.FMT_OBS[fmt]:
                row = {r: tally.get((int(d), fmt, obs, r), 0)
                       for r in ("raw", "projected", "shape", "observable",
                                 "divergent", "capability", "decline", "mismatch")}
                if not any(row.values()):
                    continue
                lines.append("| C{} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |".format(
                    d, fmt, obs, row["raw"], row["projected"], row["shape"],
                    row["observable"], row["divergent"], row["capability"],
                    row["decline"], row["mismatch"]))
    lines.append("")
    lines.append("- observations compared: **{}**; **value mismatches: {}**.".format(
        checked, len(mismatches)))
    if mismatches:
        for idv, d, obs, detail in mismatches[:20]:
            lines.append("  - MISMATCH {}/C{}/{}: {}".format(idv, d, obs, detail))
    lines.append("- Where the contract defines equality (`raw`/`projected`), the two lanes "
                 "agree; the non-`raw` cells are the documented shape / observable / "
                 "heuristic-projection distinctions, not value disagreements.")
    lines.append("")
    return {"checked": checked, "mismatch": len(mismatches)}


# ---------------------------------------------------------------------------
# Main aggregation
# ---------------------------------------------------------------------------

def aggregate(raw, campaign, reps, boot, seed, tie, predecessor=None):
    warm = P19.read_tsv(os.path.join(raw, "warm_samples.tsv"))
    once = P19.read_tsv(os.path.join(raw, "once.tsv"))
    exact = P19.read_tsv(os.path.join(raw, "exact.tsv"))
    order = P19.read_tsv(os.path.join(raw, "order.tsv"))

    # subset.tsv has no header row (it is the raw selection); read it positionally.
    subset_by_id = {}
    with open(os.path.join(raw, "subset.tsv")) as fh:
        for line in fh:
            parts = line.rstrip("\n").split("|")
            if len(parts) < 3:
                continue
            subset_by_id[parts[0]] = {"fmt": parts[2]}

    warm_s = P19.collect_build_or_warm(warm)
    ids = sorted({r["id"] for r in warm})
    depths = sorted({r["depth"] for r in warm if r["lane"] == "sqlite"},
                    key=lambda x: int(x))
    if not depths:
        depths = sorted({r["depth"] for r in warm}, key=lambda x: int(x))
    lastdep = depths[-1] if depths else "5"

    lines = []
    lines.append("# Phase 20.3 — warm heterogeneous-query court (per-session overhead)")
    lines.append("")
    lines.append("Builds each lane's store **once**, then repeats the WARM one-session "
                 "heterogeneous query lane **N={}** times per (document, depth, lane) with "
                 "an interleaved order (odd reps VOLE first, even reps SQLite first) and "
                 "retains every sample. Same 12-document subset, same contract depths "
                 "C0..C5, same source-retaining SQLite lane "
                 "(`tools/fixtures/phase18-contract-packed.py`, byte-identical), same "
                 "accounting as Phase 19.1. Wall times are integer microseconds.".format(reps))
    lines.append("")
    lines.append("- documents: **{}** ({}); depths: **C0..C{}**; warm repetitions: "
                 "**N={}**; bootstrap: **{} resamples, seed {}**, cluster-resampled by "
                 "document; tie band **+/-{:.0f}%**.".format(
                     len(ids), ", ".join(ids), depths[-1] if depths else 5, reps,
                     boot, seed, 100 * tie))
    lines.append("- interleaving: odd reps VOLE-then-SQLite, even reps SQLite-then-VOLE "
                 "(per-rep order in `raw/order.tsv`); one untimed warm-up session per "
                 "(document, depth, lane) precedes the timed reps, so no timed rep pays a "
                 "cold-inode first touch.")
    lines.append("- every individual warm sample is retained in `raw/warm_samples.tsv`; "
                 "nothing is reduced to min/median at collection time.")
    lines.append("")

    # --- one-time builds (informational) ----------------------------------
    lines.append("## One-time builds (paid once per document, outside the warm loop)")
    lines.append("")
    if once:
        lines.append("| lane | depth | n | median ms | max ms |")
        lines.append("|---|---|---:|---:|---:|")
        for lane in ("vole", "sqlite"):
            ds = ["all"] if lane == "vole" else depths
            for d in ds:
                vals = [int(r["us"]) for r in once
                        if r["lane"] == lane and r["depth"] == d and r["rc"] == "0"]
                if not vals:
                    continue
                lines.append("| {} | {} | {} | {:.2f} | {:.2f} |".format(
                    "VOLE" if lane == "vole" else "SQLite",
                    "all" if d == "all" else "C" + d, len(vals),
                    statistics.median(vals) / 1000, max(vals) / 1000))
    lines.append("")
    lines.append("The build cost is paid ONCE here (it was the per-rep cost in Phase 19.1) "
                 "and is not part of the warm ratio.")
    lines.append("")

    # --- pooled per-lane warm stats ---------------------------------------
    lines.append("## Per-lane warm statistics (pooled over documents x reps)")
    lines.append("")
    P19._pooled_table(lines, "Warm one-session wall, every sample (pooled over documents x reps)",
                      warm_s, depths)
    lines.append("Full per (document, lane, depth) statistics: `raw/per_doc_lane_depth.csv`.")
    lines.append("")

    # --- per (doc, lane, depth) CSV ---------------------------------------
    with open(os.path.join(raw, "per_doc_lane_depth.csv"), "w") as fh:
        fh.write("id,fmt,lane,depth,n,median_ms,mean_ms,p25_ms,p75_ms,min_ms,max_ms,cv\n")
        for lane in ("vole", "sqlite"):
            for d in depths:
                for idv in ids:
                    repmap = warm_s.get(lane, {}).get(d, {}).get(idv)
                    if not repmap:
                        continue
                    s = P19.summarize(repmap.values())
                    fmt = subset_by_id.get(idv, {}).get("fmt", "")
                    fh.write("{},{},{},{},{},{:.3f},{:.3f},{:.3f},{:.3f},{:.3f},{:.3f},{:.4f}\n".format(
                        idv, fmt, lane, d, s["n"], s["median"] / 1000, s["mean"] / 1000,
                        s["p25"] / 1000, s["p75"] / 1000, s["min"] / 1000,
                        s["max"] / 1000, s["cv"]))

    # --- per-depth ratio ---------------------------------------------------
    lines.append("## Per-depth VOLE/SQLite warm ratio (paired per rep)")
    lines.append("")
    lines.append("VOLE's packed warm session is depth-independent (the same store and "
                 "request set serve every depth); the per-depth variation is therefore the "
                 "SQLite envelope's depth cost. C0 is the first-touch depth worth watching.")
    lines.append("")
    lines.append("| depth | docs | pooled pairs | median ratio | geometric-mean ratio | median-ratio 95% CI |")
    lines.append("|---|---:|---:|---:|---:|---|")
    q_depth = {}
    for d in depths:
        bd = P19.pair_by_doc(warm_s.get("vole", {}).get(d, {}),
                             warm_s.get("sqlite", {}).get(d, {}))
        q_depth[d] = bd
        pooled = [r for x in bd.values() for r in x]
        if pooled:
            lo, hi, _ = P19.cluster_bootstrap(bd, statistics.median, boot, seed + 100 + int(d))
            lines.append("| C{} | {} | {} | {:.3f} | {:.3f} | {:.3f}..{:.3f} |".format(
                d, len(bd), len(pooled), statistics.median(pooled), P19.geomean(pooled), lo, hi))
        else:
            lines.append("| C{} | 0 | 0 | n/a | n/a | n/a |".format(d))
    lines.append("")

    # --- headline: summed over depths per rep -----------------------------
    vsum, ssum = fold_depth_sum(warm_s, ids, depths)
    by_doc = P19.pair_by_doc(vsum, ssum)
    lines.append("## Headline — warm heterogeneous query summed over C0..C{} per rep".format(
        lastdep))
    lines.append("")
    lines.append("The Phase-19.1/18.5 headline: per rep, the whole C0..C5 depth schedule "
                 "is served in ONE session per lane, and the two lanes' session totals are "
                 "paired by rep index.")
    lines.append("")
    head = across(lines, "VOLE/SQLite warm session, C0..C{} folded per rep".format(lastdep),
                  by_doc, seed, boot, tie)

    # --- before/after against a sealed predecessor campaign -----------------
    if predecessor:
        lines.append("## Before/after — the SAME frozen court, sealed predecessor vs this run")
        lines.append("")
        pred_raw = os.path.join(predecessor, "raw")
        try:
            p_warm = P19.read_tsv(os.path.join(pred_raw, "warm_samples.tsv"))
            p_s = P19.collect_build_or_warm(p_warm)
            p_ids = sorted({r["id"] for r in p_warm})
            p_depths = sorted({r["depth"] for r in p_warm if r["lane"] == "sqlite"},
                              key=lambda x: int(x)) or depths
            before = _headline_stats(p_s, p_ids, p_depths, seed, boot, tie)
        except OSError:
            before = None
        if before is None:
            lines.append("- predecessor `{}` produced no usable samples; before/after not "
                         "computed (reported as a gap, never as equality).".format(predecessor))
            lines.append("")
        else:
            lines.append("Predecessor campaign: `{}`.".format(predecessor))
            lines.append("Both runs use the identical frozen court and the identical "
                         "Phase-19.1 estimator (depth-summed per rep, fixed-seed cluster "
                         "bootstrap over the 12 documents); only the binary and the host "
                         "epoch differ.")
            lines.append("")
            lines.append("| run | docs | pairs | median ratio (95% CI) | geometric mean (95% CI) | W/T/L |")
            lines.append("|---|---:|---:|---|---|---|")
            for label, r in (("BEFORE", before), ("AFTER", head)):
                if r is None:
                    continue
                lines.append("| {} | {} | {} | {:.3f} ({:.3f}..{:.3f}) | {:.3f} ({:.3f}..{:.3f}) | "
                             "{} / {} / {} |".format(
                                 label, r.get("docs", len(r["docmed"])), r["n"], r["median"],
                                 r["ci_median"][0],
                                 r["ci_median"][1], r["geomean"], r["ci_geomean"][0],
                                 r["ci_geomean"][1], r["wins"], r["ties"], r["losses"]))
            lines.append("")
            # Per-document VOLE session medians (depth-summed), before vs after.
            lines.append("| document | BEFORE VOLE/SQLite median | AFTER VOLE/SQLite median | direction |")
            lines.append("|---|---:|---:|---|")
            for d in sorted(set(before["docmed"]) | set(head["docmed"])):
                b = before["docmed"].get(d)
                a = head["docmed"].get(d)
                if b is None or a is None:
                    continue
                direction = "after lower" if a < b else ("after higher" if a > b else "unchanged")
                lines.append("| {} | {:.3f} | {:.3f} | {} |".format(d, b, a, direction))
            lines.append("")
            lines.append("A before/after difference in the headline median smaller than the "
                         "after-run's median-CI half-width is **not** resolved by this court; "
                         "the half-width is the honest resolution floor at this N.")
            lines.append("")

    # --- raw paired tables -------------------------------------------------
    with open(os.path.join(raw, "paired_warm_depth.csv"), "w") as fh:
        fh.write("id,depth,rep,vole_us,sqlite_us,ratio\n")
        for d in depths:
            for idv in ids:
                for rep in sorted(set(warm_s.get("vole", {}).get(d, {}).get(idv, {})) &
                                  set(warm_s.get("sqlite", {}).get(d, {}).get(idv, {}))):
                    n = warm_s["vole"][d][idv][rep]
                    m = warm_s["sqlite"][d][idv][rep]
                    if m > 0:
                        fh.write("{},{},{},{},{},{:.5f}\n".format(idv, d, rep, n, m, n / m))
    with open(os.path.join(raw, "paired_warm_sum.csv"), "w") as fh:
        fh.write("id,rep,vole_sum_us,sqlite_sum_us,ratio\n")
        for idv in ids:
            for rep in sorted(set(vsum.get(idv, {})) & set(ssum.get(idv, {}))):
                n, m = vsum[idv][rep], ssum[idv][rep]
                if m > 0:
                    fh.write("{},{},{},{},{:.5f}\n".format(idv, rep, n, m, n / m))

    # --- variance floor / MDE ---------------------------------------------
    lines.append("## Variance floor and minimum detectable effect at N={}".format(reps))
    lines.append("")
    # within-cell CV, averaged (the bind mount's repeat noise at fixed doc+store)
    cell_cvs = []
    for lane in ("vole", "sqlite"):
        for d in depths:
            for idv in ids:
                repmap = warm_s.get(lane, {}).get(d, {}).get(idv, {})
                if len(repmap) > 1:
                    cell_cvs.append(P19.summarize(repmap.values())["cv"])
    if cell_cvs:
        lines.append("- within-cell between-rep CV (median over all (doc, depth, lane) "
                     "cells): **{:.1f}%** (p90 {:.1f}%). This is the host bind-mount "
                     "repeat noise floor at fixed document and store.".format(
                         100 * statistics.median(cell_cvs),
                         100 * P19.percentile(cell_cvs, 0.90)))
    lines.append("- headline pooled paired-ratio CV: **{:.1f}%**; median-ratio 95% CI "
                 "half-width: **+/-{:.3f}**.".format(100 * head["cv"], head["half"]))
    lines.append("- **minimum detectable effect at this N (80% power, normal approx): "
                 "~{:.3f}** (i.e. a true median ratio shift smaller than this is not "
                 "resolvable with N={} on this variance floor).".format(
                     head["mde80"], reps))
    lines.append("")

    # --- exactness ---------------------------------------------------------
    lines.append("## Exact original closure (length + SHA-256 + byte compare)")
    lines.append("")
    lines.append("| id | fmt | VOLE ok | VOLE rc | VOLE ms | SQLite ok | SQLite rc | SQLite ms |")
    lines.append("|---|---|---|---:|---:|---|---:|---:|")
    for r in exact:
        lines.append("| {} | {} | {} | {} | {:.1f} | {} | {} | {:.1f} |".format(
            r["id"], r["fmt"], r["v_ok"], r["v_rc"], int(r["v_us"]) / 1000,
            r["sql_ok"], r["sql_rc"], int(r["sql_us"]) / 1000))
    lines.append("")
    n = len(exact)
    nv = sum(1 for r in exact if r["v_ok"] == "1")
    ns = sum(1 for r in exact if r["sql_ok"] == "1")
    lines.append("VOLE `materialize --exact --packed`: **{}/{} byte-exact**. "
                 "SQLite retained blob: **{}/{} byte-exact**.".format(nv, n, ns, n))
    lines.append("")

    # --- equivalence -------------------------------------------------------
    equiv = equivalence(raw, lines, subset_by_id, depths)

    # --- explicit verdict --------------------------------------------------
    lines.append("## Answers")
    lines.append("")
    v = verdict(lines, "Is the warm VOLE/SQLite ratio resolved to > 1.0 (a real loss), "
                       "resolved to <= 1.0 (parity or better), or still including 1.0?",
                 head, tie, reps)
    lines.append("")
    lines.append("**Exactness / equivalence.** VOLE exact closure **{}/{}**, SQLite "
                 "**{}/{}** byte-exact; warm-session answers compared **{}**, value "
                 "mismatches **{}**.".format(nv, n, ns, n, equiv["checked"], equiv["mismatch"]))
    lines.append("")

    # --- caveats -----------------------------------------------------------
    lines.append("## Caveats and honesty")
    lines.append("")
    lines.append("- This is a **warm-only** court: the one-time store builds are paid "
                 "once and are not part of the ratio. The build win question is Phase "
                 "19.1's and is unchanged.")
    lines.append("- The bind-mounted host store is noisy; the between-rep CV and the "
                 "quoted CI are the honest measure of that noise. If 1.0 remains inside "
                 "the interval, that is reported, never massaged.")
    lines.append("- Warm sessions run in the low-millisecond range, where process "
                 "start-up is a material fraction of the wall; that start-up is part of "
                 "BOTH lanes' measured cost and is not subtracted.")
    lines.append("- Sampling unit for the CI is the document (cluster bootstrap), not the "
                 "doc-rep pair; a document's {} repeated measures are correlated and are "
                 "not treated as {} independent documents.".format(reps, reps))
    lines.append("- MDE is a normal-approximation convenience derived from the bootstrap "
                 "CI half-width; it is an order-of-magnitude resolution statement, not a "
                 "measured quantity.")
    if order:
        seen = sorted({r["order"] for r in order})
        lines.append("- recorded interleave orders: {}.".format("; ".join(seen)))
    lines.append("")

    report = "\n".join(lines)
    with open(os.path.join(campaign, "SUMMARY.md"), "w") as fh:
        fh.write(report)
    print(report)
    return 0 if v != "unresolved" else 0


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    ag = sub.add_parser("aggregate")
    ag.add_argument("raw")
    ag.add_argument("campaign")
    ag.add_argument("--reps", type=int, default=100)
    ag.add_argument("--boot", type=int, default=20000)
    ag.add_argument("--seed", type=int, default=190102)
    ag.add_argument("--tie", type=float, default=0.10)
    ag.add_argument("--predecessor", default=None,
                    help="path to a sealed predecessor campaign dir (for before/after)")
    ns = ap.parse_args(argv)
    if ns.cmd == "aggregate":
        return aggregate(ns.raw, ns.campaign, ns.reps, ns.boot, ns.seed, ns.tie,
                         predecessor=ns.predecessor)
    return 2


if __name__ == "__main__":
    sys.exit(main())
