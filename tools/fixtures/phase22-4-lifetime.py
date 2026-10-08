#!/usr/bin/env python3
"""Phase 22.4 aggregator — unknown-query lifetime frontier per region.

Reads the raw tables retained by `tools/phase22-4-lifetime-court.sh` and reports,
per region (format / size class / contract depth), the paired VOLE/SQLite
cumulative-session ratio with a fixed-seed cluster bootstrap 95% CI over the 12
documents, the ADR-0054 tie band, W/T/L and the MDE — plus the build / persistent
bytes / peak-RSS frontier axes. It NEVER reports a single averaged headline.

Statistics are the frozen Phase-19.1 primitives (`tools/fixtures/phase19-repeat.py`:
`pair_by_doc`, `cluster_bootstrap`, `geomean`), the same estimators the Phase-20.3
warm aggregator uses. `phase20-warm.py::aggregate` itself is NOT used verbatim
because its headline folds the per-depth values by SUMMING them (C0..C5 are
independent capability sessions there); here the schedule is a single hidden
sequence and the regions are reported separately, which is exactly what the
Phase-22.4 method note requires ("do not report a single averaged headline").
"""

import argparse
import json
import os
import statistics
import sys
import importlib.util


def _load(name, fname):
    here = os.path.dirname(os.path.abspath(__file__))
    spec = importlib.util.spec_from_file_location(name, os.path.join(here, fname))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


P19 = _load("p19_for_2214", "phase19-repeat.py")

# A region may only be called win/loss when its cluster bootstrap actually resamples
# more than a tiny handful of documents; otherwise it is reported as low-n.
MIN_DOCS = 3


def est(by_doc, seed, boot, tie):
    pooled = [r for d in by_doc for r in by_doc[d]]
    out = {"n": len(pooled), "docs": len(by_doc)}
    if not pooled:
        out.update({"median": 0.0, "lo": 0.0, "hi": 0.0, "geomean": 0.0,
                    "gl": 0.0, "gh": 0.0, "half": 0.0, "mde80": 0.0,
                    "wins": 0, "ties": 0, "losses": 0, "cv": 0.0, "outcome": "no-samples"})
        return out
    med = statistics.median(pooled)
    gm = P19.geomean(pooled)
    lo, hi, _ = P19.cluster_bootstrap(by_doc, statistics.median, boot, seed)
    gl, gh, _ = P19.cluster_bootstrap(by_doc, P19.geomean, boot, seed + 1)
    docmed = {d: statistics.median(v) for d, v in by_doc.items()}
    wins = sum(1 for m in docmed.values() if m < (1 - tie))
    ties = sum(1 for m in docmed.values() if (1 - tie) <= m <= (1 + tie))
    losses = sum(1 for m in docmed.values() if m > (1 + tie))
    mean = statistics.fmean(pooled)
    cv = (statistics.stdev(pooled) / mean) if (len(pooled) > 1 and mean) else 0.0
    half = (hi - lo) / 2.0
    mde80 = (1.959963985 + 0.8416212336) / 1.959963985 * half
    if lo > 1.0:
        outcome = "loss"
    elif hi < 1.0:
        outcome = "win"
    elif (lo > 1 - tie) and (hi < 1 + tie):
        outcome = "tie"
    else:
        outcome = "unresolved"
    if out["docs"] < MIN_DOCS:
        outcome = "low-n"
    out.update({"median": med, "lo": lo, "hi": hi, "geomean": gm, "gl": gl, "gh": gh,
                "half": half, "mde80": mde80, "wins": wins, "ties": ties,
                "losses": losses, "cv": cv, "outcome": outcome, "docmed": docmed})
    return out


def pair_region(vd, sd, ids):
    """Pair VOLE (vd) with a SQLite lane (sd), restricted to `ids`."""
    bd = {}
    for idv in ids:
        if idv not in vd or idv not in sd:
            continue
        rs = []
        for rep in sorted(set(vd[idv]) & set(sd[idv])):
            n, m = vd[idv][rep], sd[idv][rep]
            if m > 0 and n > 0:
                rs.append(n / m)
        if rs:
            bd[idv] = rs
    return bd


def ratio_by_doc(num, den, ids):
    """One ratio per document from per-doc scalars (build/bytes/RSS axes)."""
    bd = {}
    for idv in ids:
        n, m = num.get(idv), den.get(idv)
        if n and m and m > 0:
            bd[idv] = [n / m]
    return bd


def f3(x):
    return "%.3f" % x


def region_table(lines, title, vseries, lanes, depths, ids, seed, boot, tie):
    lines.append("### " + title)
    lines.append("")
    lines.append("| lane | depth | docs | pairs | median (95% CI) | geomean (95% CI) | W/T/L | MDE | outcome |")
    lines.append("|---|---|---:|---:|---|---|---|---:|---|")
    res = {}
    for lane in lanes:
        for d in depths:
            bd = pair_region(vseries.get("vole", {}).get(d, {}),
                             vseries.get(lane, {}).get(d, {}), ids)
            e = est(bd, seed + 1000 * depths.index(d) + len(title), boot, tie)
            res[(lane, d)] = e
            if e["n"] == 0:
                lines.append("| {} | C{} | 0 | 0 | n/a | n/a | n/a | n/a | no-samples |".format(lane, d))
                continue
            lines.append("| {} | C{} | {} | {} | {} ({}..{}) | {} ({}..{}) | {}/{}/{} | +/-{} | {} |".format(
                lane, d, e["docs"], e["n"], f3(e["median"]), f3(e["lo"]), f3(e["hi"]),
                f3(e["geomean"]), f3(e["gl"]), f3(e["gh"]),
                e["wins"], e["ties"], e["losses"], f3(e["half"]), e["outcome"]))
    lines.append("")
    return res


def main(raw, camp, reps, boot, seed, tie):
    warm = P19.read_tsv(os.path.join(raw, "warm_samples.tsv"))
    builds = P19.read_tsv(os.path.join(raw, "build_bytes.tsv"))
    adapt = P19.read_tsv(os.path.join(raw, "adapt.tsv"))
    shape = P19.read_tsv(os.path.join(raw, "store_shape.tsv"))
    exact = P19.read_tsv(os.path.join(raw, "exact.tsv"))
    answered = P19.read_tsv(os.path.join(raw, "answered.tsv"))
    equiv = P19.read_tsv(os.path.join(raw, "equiv.tsv"))

    subset_by_id = {}
    with open(os.path.join(raw, "subset.tsv")) as fh:
        for line in fh:
            p = line.rstrip("\n").split("|")
            if len(p) >= 5:
                subset_by_id[p[0]] = {"agency": p[1], "fmt": p[2], "sclass": p[3],
                                      "blen": int(p[4])}

    ids = sorted({r["id"] for r in warm})
    lanes = [l for l in ("full", "adaptive", "hist")]
    depths = sorted({int(r["depth"]) for r in warm}, key=int)
    if not depths:
        depths = [0]

    # VOLE is depth-independent: collect_build_or_warm keyed by depth works.
    # Normalize the depth keys (they arrive as strings) to ints.
    raw_series = P19.collect_build_or_warm(warm)
    series = {lane: {int(d): m for d, m in dmap.items()}
              for lane, dmap in raw_series.items()}

    # ---- schedules (frozen) ----
    sched_dir = os.path.join(raw, "sched")
    schedules = {}
    for idv in ids:
        p = os.path.join(sched_dir, idv + ".schedule.json")
        try:
            with open(p) as f:
                schedules[idv] = json.load(f)
        except OSError:
            pass
    sched_ids = sorted(schedules)
    sched0 = schedules[sched_ids[0]] if sched_ids else {}

    # ---- answered/declined + common-answered counts (per id,depth,lane) ----
    ans = {}
    for r in answered:
        key = (r["id"], int(r["depth"]), r["lane"])
        a = ans.setdefault(key, {"v_ans": 0, "s_ans": 0, "common": 0, "n": 0})
        a["v_ans"] += int(r["v_ans"])
        a["s_ans"] += int(r["s_ans"])
        a["common"] += int(r["answered"])
        a["n"] += 1

    # ---- per-document scalars for the build / bytes / rss axes ----
    def lane_build(idv, lane):
        for r in builds:
            if r["id"] == idv and r["lane"] == lane and r["rc"] == "0":
                return int(r["us"])
        return 0

    def lane_adapt_us(idv, lane, upto_d=None):
        """The adaptive lane's charged lazy materialization for capability <= upto_d
        (all depths when None). Other lanes never lazily materialize."""
        if lane != "adaptive":
            return 0
        return sum(int(r["us"]) for r in adapt
                   if r["id"] == idv and r["rc"] == "0"
                   and (upto_d is None or int(r["depth"]) <= upto_d))

    def lane_bytes(idv, lane):
        # final (max) persistent bytes measured for that lane
        best = 0
        for r in shape:
            if r["id"] == idv and r["lane"] == lane:
                best = max(best, int(r["bytes"]))
        return best

    def lane_rss_med(idv, lane, d):
        vals = [int(r["rss_kb"]) for r in warm
                if r["id"] == idv and r["lane"] == lane and int(r["depth"]) == d
                and r["rc"] == "0" and int(r["rss_kb"]) > 0]
        return statistics.median(vals) if vals else 0

    # =====================================================================
    # Report
    # =====================================================================
    L = []
    L.append("# Phase 22.4 — unknown-query lifetime frontier (measurement court)")
    L.append("")
    L.append("On a **pre-registered, seeded hidden schedule** the encoder never saw, "
             "per **equivalent successful observation** (session cost / |observations "
             "answered by BOTH lanes|). Declines are excluded from numerator and "
             "denominator identically. VOLE/SQLite ratio **< 1 favours VOLE**. Every "
             "adaptation is charged. **No single averaged headline is reported:** every "
             "number below is a named region.")
    L.append("")
    L.append("- documents: **{}**; lanes: VOLE-packed vs {{{}}}; depths: **{}**; "
             "reps: **{}**; bootstrap: **{} resamples, seed {}**, cluster-resampled by "
             "document; tie band **+/-{:.0f}%**.".format(
                 len(ids), ", ".join(lanes), ", ".join("C%d" % d for d in depths),
                 reps, boot, seed, 100 * tie))
    L.append("- **binding prior (ADR-0046):** the Phase-15.6 adaptive-promotion "
             "experiment LOST; a VOLE win is **not** presumed likely. The "
             "pre-registered expectation is no advantage outside a narrow region, if any.")
    if schedules:
        L.append("- hidden schedule: seed **{}**, length **{}**, one per document "
                 "(recipe frozen in `tools/fixtures/phase22-4-schedule.py`); counts:".format(
                     sched0.get("seed"), sched0.get("length")))
        seen = {}
        for s in schedules.values():
            for o, c in s.get("counts", {}).items():
                seen[o] = seen.get(o, 0) + c
        L.append("- pooled schedule counts over {} docs: {}".format(
            len(schedules), ", ".join("{}={}".format(k, seen[k]) for k in sorted(seen))))
    L.append("")

    # ---- headline: primary competitor `full`, per region ----
    L.append("## Primary frontier — VOLE vs the tuned `full` envelope, region by region")
    L.append("")
    prim = region_table(L, "Pooled over documents, by depth (primary `full`)",
                        series, ["full"], depths, ids, seed, boot, tie)
    by_fmt = {}
    for idv in ids:
        by_fmt.setdefault(subset_by_id[idv]["fmt"], []).append(idv)
    for fmt in sorted(by_fmt):
        region_table(L, "Format `{}` ({} docs), by depth (primary `full`)".format(
            fmt, len(by_fmt[fmt])), series, ["full"], depths, by_fmt[fmt], seed, boot, tie)
    by_size = {}
    for idv in ids:
        by_size.setdefault(subset_by_id[idv]["sclass"], []).append(idv)
    for sc in sorted(by_size):
        region_table(L, "Size class `{}` ({} docs), by depth (primary `full`)".format(
            sc, len(by_size[sc])), series, ["full"], depths, by_size[sc], seed, boot, tie)

    # ---- secondary competitors ----
    L.append("## Secondary comparators — `adaptive` (all adaptation charged) and `hist`")
    L.append("")
    region_table(L, "Pooled over documents, by depth (secondary lanes)",
                 series, ["adaptive", "hist"], depths, ids, seed, boot, tie)

    # ---- build / bytes / RSS frontier axes ----
    L.append("## Frontier axes (VOLE/SQLite; <1 favours VOLE), pooled over documents")
    L.append("")
    L.append("| axis | lane | docs | median (95% CI) | MDE | outcome |")
    L.append("|---|---|---:|---|---:|---|")
    axes = {}
    for lane in lanes:
        num = {idv: lane_build(idv, "vole") for idv in ids}
        den = {idv: lane_build(idv, lane) for idv in ids}
        e = est(ratio_by_doc(num, den, ids), seed + 11, boot, tie)
        axes[("build", lane)] = e
        L.append("| build us | {} | {} | {} ({}..{}) | +/-{} | {} |".format(
            lane, e["docs"], f3(e["median"]), f3(e["lo"]), f3(e["hi"]), f3(e["half"]), e["outcome"]))
        num = {idv: lane_bytes(idv, "vole") for idv in ids}
        den = {idv: lane_bytes(idv, lane) for idv in ids}
        e = est(ratio_by_doc(num, den, ids), seed + 12, boot, tie)
        axes[("bytes", lane)] = e
        L.append("| store bytes | {} | {} | {} ({}..{}) | +/-{} | {} |".format(
            lane, e["docs"], f3(e["median"]), f3(e["lo"]), f3(e["hi"]), f3(e["half"]), e["outcome"]))
    for d in depths:
        for lane in lanes:
            num = {idv: lane_rss_med(idv, "vole", d) for idv in ids}
            den = {idv: lane_rss_med(idv, lane, d) for idv in ids}
            e = est(ratio_by_doc(num, den, ids), seed + 20 + d, boot, tie)
            axes[("rss", lane, d)] = e
            L.append("| peak RSS C{} | {} | {} | {} ({}..{}) | +/-{} | {} |".format(
                d, lane, e["docs"], f3(e["median"]), f3(e["lo"]), f3(e["hi"]),
                f3(e["half"]), e["outcome"]))
    L.append("")
    L.append("### Build and store-bytes frontier per format region")
    L.append("")
    L.append("| axis | format | lane | docs | median (95% CI) | MDE | outcome |")
    L.append("|---|---|---|---:|---|---:|---|")
    for fmt, fids in sorted(by_fmt.items()):
        for lane in lanes:
            num = {idv: lane_build(idv, "vole") for idv in fids}
            den = {idv: lane_build(idv, lane) for idv in fids}
            e = est(ratio_by_doc(num, den, fids), seed + 41, boot, tie)
            axes[("build", lane, fmt)] = e
            L.append("| build us | {} | {} | {} | {} ({}..{}) | +/-{} | {} |".format(
                fmt, lane, e["docs"], f3(e["median"]), f3(e["lo"]), f3(e["hi"]),
                f3(e["half"]), e["outcome"]))
            num = {idv: lane_bytes(idv, "vole") for idv in fids}
            den = {idv: lane_bytes(idv, lane) for idv in fids}
            e = est(ratio_by_doc(num, den, fids), seed + 42, boot, tie)
            axes[("bytes", lane, fmt)] = e
            L.append("| store bytes | {} | {} | {} | {} ({}..{}) | +/-{} | {} |".format(
                fmt, lane, e["docs"], f3(e["median"]), f3(e["lo"]), f3(e["hi"]),
                f3(e["half"]), e["outcome"]))
    L.append("")

    # ---- lifetime composite (build + adaptation + one query session) ----
    L.append("## Lifetime composite per region — build + charged adaptation(<=Cd) + N schedule passes")
    L.append("")
    L.append("Per document: VOLE `build + N x one-session`; SQLite `build + sum(ensure for "
             "capability <= Cd) + N x one-session`. The adaptation is a one-time cost and "
             "is amortized over **N schedule passes**. At N=1 the lifetime sees the "
             "schedule once (adaptation dominates the SQLite side); at N={} (the measured "
             "rep count) the one-time terms amortize and the ratio approaches the query "
             "region. Paired VOLE/SQLite ratio; per-equivalent-observation normalization "
             "cancels in the ratio.".format(reps))
    L.append("")
    life = {}
    for horizon in (1, reps):
        L.append("### Horizon N={} schedule pass(es)".format(horizon))
        L.append("")
        L.append("| lane | depth | docs | median (95% CI) | MDE | outcome |")
        L.append("|---|---|---:|---|---:|---|")
        for lane in lanes:
            for d in depths:
                num = {}
                den = {}
                for idv in ids:
                    vq = series.get("vole", {}).get(d, {}).get(idv, {})
                    sq = series.get(lane, {}).get(d, {}).get(idv, {})
                    if not vq or not sq:
                        continue
                    vs = statistics.median(vq.values())
                    ss = statistics.median(sq.values())
                    num[idv] = lane_build(idv, "vole") + horizon * vs
                    den[idv] = (lane_build(idv, lane) + lane_adapt_us(idv, lane, d)
                                + horizon * ss)
                e = est(ratio_by_doc(num, den, ids), seed + 30 + d + horizon, boot, tie)
                life[(lane, d, horizon)] = e
                L.append("| {} | C{} | {} | {} ({}..{}) | +/-{} | {} |".format(
                    lane, d, e["docs"], f3(e["median"]), f3(e["lo"]), f3(e["hi"]),
                    f3(e["half"]), e["outcome"]))
        L.append("")

    # ---- adaptation / storage-growth / reads ----
    L.append("## Adaptation, storage growth and reads (all charged)")
    L.append("")
    L.append("| id | lane | depth | ensure us | materialized | bytes after |")
    L.append("|---|---|---|---:|---|---:|")
    for r in adapt:
        L.append("| {} | {} | C{} | {} | {} | {} |".format(
            r["id"], r["lane"], r["depth"], r["us"], r["materialized"], r["bytes_after"]))
    L.append("")
    L.append("VOLE charges its own derived-cache writes inside its measured session "
             "(the `observe-batch` `stats.cache_bytes_written`), so no separate VOLE "
             "adaptation row exists. The adaptive lane's `ensure` rows above are its "
             "lazy materialization, timed in full.")
    L.append("")

    # ---- declines / equivalence ----
    L.append("## Declines and answer equivalence")
    L.append("")
    tally = {}
    for r in equiv:
        tally[r["equiv"]] = tally.get(r["equiv"], 0) + 1
    mism = [r for r in equiv if r["equiv"] == "mismatch"]
    L.append("- equivalence results (VOLE vs the primary `full` lane, every depth, "
             "every schedule slot): {}".format(
                 ", ".join("{}={}".format(k, tally[k]) for k in sorted(tally)) or "none"))
    L.append("- **value mismatches: {}**".format(len(mism)))
    for r in mism[:20]:
        L.append("  - MISMATCH {}/{}/C{}/{}: {}".format(
            r["id"], r["fmt"], r["depth"], r["lane"], r["mismatch_detail"]))
    L.append("")
    L.append("| depth | lane | slots | VOLE answered | SQLite answered | common |")
    L.append("|---|---|---:|---:|---:|---:|")
    for d in depths:
        for lane in lanes:
            n = sum(v["n"] for k, v in ans.items() if k[1] == d and k[2] == lane)
            va = sum(v["v_ans"] for k, v in ans.items() if k[1] == d and k[2] == lane)
            sa = sum(v["s_ans"] for k, v in ans.items() if k[1] == d and k[2] == lane)
            co = sum(v["common"] for k, v in ans.items() if k[1] == d and k[2] == lane)
            L.append("| C{} | {} | {} | {} | {} | {} |".format(d, lane, n, va, sa, co))
    L.append("")
    L.append("`common` = schedule observations answered by BOTH VOLE and that lane; the "
             "per-equivalent-observation cost divides each lane's session by this same "
             "number (which cancels in the paired ratio). At C0..C3 the `full`/`adaptive`/"
             "`hist` lanes decline `revision` (a C4 capability) while VOLE's depth-independent "
             "store answers it; at C4..C5 VOLE declines a few observations the SQLite lanes "
             "answer. That is a contract capability difference, recorded, not a value error.")
    L.append("")

    # ---- exactness ----
    L.append("## Exact original closure (length + SHA-256 + byte compare)")
    L.append("")
    L.append("| id | fmt | lane | ok | rc | sha match | len | cmp vs source | cmp vs VOLE |")
    L.append("|---|---|---|---:|---:|---|---:|---|---|")
    for r in exact:
        L.append("| {} | {} | {} | {} | {} | {} | {} | {} | {} |".format(
            r["id"], r["fmt"], r["lane"], r["ok"], r["rc"],
            r["sha"], r["len"], r.get("cmp_src", ""), r.get("cmp_vole", "")))
    L.append("")
    for lane in ["vole"] + lanes:
        rows = [r for r in exact if r["lane"] == lane]
        ok = sum(1 for r in rows if r["ok"] == "1")
        L.append("- `{}`: **{}/{}** byte-exact (length + SHA-256 + `cmp` vs source).".format(
            lane, ok, len(rows)))
    L.append("")

    # ---- verdict ----
    L.append("## Verdict (deterministic)")
    L.append("")
    L.append("A **VOLE win** is claimed ONLY where the 95% CI of the paired median "
             "ratio excludes 1.0 in VOLE's favour (upper bound < 1.0) in a named "
             "region of the primary `full` comparator. Everything else is a recorded "
             "tie / loss / insufficient-resolution.")
    L.append("")
    wins, losses, ties_, unres = [], [], [], []
    for (lane, d), e in prim.items():
        tag = "full@C{}".format(d)
        if e["outcome"] == "win":
            wins.append(tag)
        elif e["outcome"] == "loss":
            losses.append(tag)
        elif e["outcome"] == "tie":
            ties_.append(tag)
        else:
            unres.append(tag)
    for fmt, lids in sorted(by_fmt.items()):
        for d in depths:
            bd = pair_region(series.get("vole", {}).get(d, {}),
                             series.get("full", {}).get(d, {}), lids)
            e = est(bd, seed + 77 + d, boot, tie)
            tag = "full@{}@C{}".format(fmt, d)
            (wins if e["outcome"] == "win" else
             losses if e["outcome"] == "loss" else
             ties_ if e["outcome"] == "tie" else unres).append(tag)
    if wins and losses:
        verdict = "MIXED[win:" + ",".join(sorted(wins)) + "][loss:" + ",".join(sorted(losses)) + "]"
    elif wins:
        verdict = "VOLE-WIN[" + ",".join(sorted(wins)) + "]"
    elif losses:
        verdict = "VOLE-LOSS[" + ",".join(sorted(losses)) + "]"
    else:
        verdict = "NO-WIN"
    L.append("- **wins ({})**: {}".format(len(wins), ", ".join(sorted(wins)) or "none"))
    L.append("- **losses ({})**: {}".format(len(losses), ", ".join(sorted(losses)) or "none"))
    L.append("- **ties ({})**: {}".format(len(ties_), ", ".join(sorted(ties_)) or "none"))
    L.append("- **insufficient resolution ({})**: {}".format(
        len(unres), ", ".join(sorted(unres)) or "none"))
    L.append("")
    L.append("**Verdict: {}.**".format(verdict))
    L.append("")

    # ---- caveats ----
    L.append("## What this does not prove")
    L.append("")
    L.append("- No production code changed and **nothing ships**; this is a "
             "measurement court on the shipping binary.")
    L.append("- Regions whose CI still includes 1.0 are **unresolved at this N**, never "
             "parity; the MDE column is the honest resolution floor.")
    L.append("- The `sqlite3` CLI exposes no sub-millisecond per-statement wall, so the "
             "**paired** comparison is the cumulative session cost per equivalent "
             "successful observation; the VOLE per-request cumulative curve "
             "(`raw/cumulative_vole.csv`) is descriptive only and unpaired.")
    L.append("- Persistent bytes are measured on the store directories; VOLE's derived "
             "cache (written during the schedule) and the SQLite WAL are reported "
             "separately and are NOT part of the `store bytes` axis.")
    L.append("- The adaptive lane is materialized once per depth before the timed reps "
             "and charged in full; its steady-state queries run on the fully adapted "
             "store (this FAVOURS the competitor).")
    L.append("- The hidden schedule is one pre-registered draw per document; a different "
             "seed is a different frozen schedule. A separate, clearly-labelled seed "
             "robustness check (`evidence/campaigns/*-phase22-4-lifetime-*-seedcheck/`) "
             "was run to test whether the region split survives re-drawing the schedule; "
             "it is robustness evidence, not part of this pre-registered receipt.")
    L.append("")

    report = "\n".join(L) + "\n"
    with open(os.path.join(camp, "SUMMARY.md"), "w") as fh:
        fh.write(report)

    # ---- MATRIX.md ----
    M = ["# Phase 22.4 — raw region matrix (VOLE/SQLite paired session ratios)",
         "",
         "| scope | lane | depth | docs | pairs | median | 95% CI | geomean | W/T/L | MDE | outcome |",
         "|---|---|---|---:|---:|---:|---|---:|---|---:|---|"]
    for lane in lanes:
        for d in depths:
            for scope_name, scope_ids in (
                    [("all", ids)] + [("fmt:" + k, v) for k, v in sorted(by_fmt.items())]
                    + [("size:" + k, v) for k, v in sorted(by_size.items())]):
                bd = pair_region(series.get("vole", {}).get(d, {}),
                                 series.get(lane, {}).get(d, {}), scope_ids)
                e = est(bd, seed + 500 + d + len(scope_name), boot, tie)
                if e["n"] == 0:
                    continue
                M.append("| {} | {} | C{} | {} | {} | {} | {}..{} | {} | {}/{}/{} | +/-{} | {} |".format(
                    scope_name, lane, d, e["docs"], e["n"], f3(e["median"]),
                    f3(e["lo"]), f3(e["hi"]), f3(e["geomean"]),
                    e["wins"], e["ties"], e["losses"], f3(e["half"]), e["outcome"]))
    M.append("")
    M.append("Per-document build bytes / store bytes / median session us:")
    M.append("")
    M.append("| id | fmt | size | lane | build us | store bytes | median session us |")
    M.append("|---|---|---|---|---:|---:|---:|")
    for idv in ids:
        for lane in ["vole"] + lanes:
            meds = []
            for d in depths:
                vals = list(series.get(lane, {}).get(d, {}).get(idv, {}).values())
                if vals:
                    meds.append(statistics.median(vals))
            med = statistics.median(meds) if meds else 0
            M.append("| {} | {} | {} | {} | {} | {} | {:.1f} |".format(
                idv, subset_by_id[idv]["fmt"], subset_by_id[idv]["sclass"], lane,
                lane_build(idv, lane) if lane != "vole" else lane_build(idv, "vole"),
                lane_bytes(idv, lane), med))
    M.append("")
    with open(os.path.join(camp, "MATRIX.md"), "w") as fh:
        fh.write("\n".join(M) + "\n")

    # ---- summary.json / counts.txt ----
    fmt_depth, size_depth = {}, {}
    for fmt, lids in sorted(by_fmt.items()):
        for d in depths:
            bd = pair_region(series.get("vole", {}).get(d, {}),
                             series.get("full", {}).get(d, {}), lids)
            e = est(bd, seed + 77 + d, boot, tie)
            fmt_depth["full@%s@C%d" % (fmt, d)] = {
                k: e[k] for k in ("n", "docs", "median", "lo", "hi", "geomean",
                                 "half", "mde80", "wins", "ties", "losses", "outcome")}
    for sc, sids in sorted(by_size.items()):
        for d in depths:
            bd = pair_region(series.get("vole", {}).get(d, {}),
                             series.get("full", {}).get(d, {}), sids)
            e = est(bd, seed + 88 + d, boot, tie)
            size_depth["full@%s@C%d" % (sc, d)] = {
                k: e[k] for k in ("n", "docs", "median", "lo", "hi", "geomean",
                                 "half", "mde80", "wins", "ties", "losses", "outcome")}
    summary = {
        "phase": "22.4",
        "verdict": verdict,
        "docs": len(ids),
        "depths": depths,
        "primary_lane": "full",
        "schedule": {
            "seed": sched0.get("seed") if schedules else None,
            "length": sched0.get("length") if schedules else None,
            "per_doc": {k: v.get("counts") for k, v in schedules.items()},
        },
        "regions": {
            "primary_by_depth": {("full@C%d" % d): {k: prim[("full", d)][k]
                                 for k in ("n", "docs", "median", "lo", "hi",
                                           "geomean", "half", "mde80", "wins",
                                           "ties", "losses", "outcome")}
                                 for d in depths if ("full", d) in prim},
            "primary_by_format_depth": fmt_depth,
            "primary_by_size_depth": size_depth,
        },
        "frontier_axes": {
            "/".join(str(x) for x in k): {kk: axes[k][kk] for kk in
                                        ("n", "docs", "median", "lo", "hi", "half", "outcome")}
            for k in axes},
        "lifetime_composite": {("life@%s@C%d@N%d" % (k[0], k[1], k[2])): {
            kk: life[k][kk] for kk in ("n", "docs", "median", "lo", "hi", "half", "outcome")}
            for k in life},
        "wins": sorted(wins), "losses": sorted(losses),
        "ties": sorted(ties_), "insufficient_resolution": sorted(unres),
        "equivalence": tally,
        "value_mismatches": len(mism),
        "exactness": {lane: "{}/{}".format(
            sum(1 for r in exact if r["lane"] == lane and r["ok"] == "1"),
            sum(1 for r in exact if r["lane"] == lane))
            for lane in ["vole"] + lanes},
    }
    with open(os.path.join(camp, "summary.json"), "w") as fh:
        json.dump(summary, fh, indent=2, sort_keys=True)
        fh.write("\n")

    with open(os.path.join(camp, "counts.txt"), "w") as fh:
        fh.write("docs=%d\n" % len(ids))
        fh.write("verdict=%s\n" % verdict)
        fh.write("primary_wins=%s\n" % (",".join(sorted(wins)) or "none"))
        fh.write("primary_losses=%s\n" % (",".join(sorted(losses)) or "none"))
        fh.write("primary_ties=%s\n" % (",".join(sorted(ties_)) or "none"))
        fh.write("primary_unresolved=%s\n" % (",".join(sorted(unres)) or "none"))
        fh.write("value_mismatches=%d\n" % len(mism))
        fh.write("schedule_seed=%s\n" % (sched0.get("seed") if schedules else "none"))
        fh.write("schedule_length=%s\n" % (sched0.get("length") if schedules else "none"))

    # stdout
    print("phase22.4 lifetime court")
    print("  docs=%d depths=%s reps=%d" % (len(ids), ",".join("C%d" % d for d in depths), reps))
    for d in depths:
        e = prim.get(("full", d))
        if not e or e["n"] == 0:
            continue
        print("  full@C%d: median=%s (95%% CI %s..%s) MDE=+/-%s W/T/L=%d/%d/%d %s" % (
            d, f3(e["median"]), f3(e["lo"]), f3(e["hi"]), f3(e["half"]),
            e["wins"], e["ties"], e["losses"], e["outcome"]))
    print("  wins=%s losses=%s ties=%s unresolved=%s" % (
        len(wins), len(losses), len(ties_), len(unres)))
    print("  verdict=%s" % verdict)
    return 0


def main_cli(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("raw")
    ap.add_argument("campaign")
    ap.add_argument("--reps", type=int, default=25)
    ap.add_argument("--boot", type=int, default=20000)
    ap.add_argument("--seed", type=int, default=220400)
    ap.add_argument("--tie", type=float, default=0.10)
    ap.add_argument("--min-docs", type=int, default=3)
    ns = ap.parse_args(argv)
    global MIN_DOCS
    MIN_DOCS = ns.min_docs
    return main(ns.raw, ns.campaign, ns.reps, ns.boot, ns.seed, ns.tie)


if __name__ == "__main__":
    sys.exit(main_cli())
