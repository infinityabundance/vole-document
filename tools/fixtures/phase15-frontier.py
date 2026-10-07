#!/usr/bin/env python3
# Phase 15.6 diversity-court schedule generator + frontier aggregator (stdlib only).
#
# Two responsibilities, both pre-registered and frozen:
#
#   schedule  Emit an ORDERED op stream for one document from fixed constants
#             (SEED, K, TYPES, PER_TYPE). It never runs VOLE, never reads a lane
#             result, and never sees the corpus content: it is a pure function of
#             the format + these constants, so the emitted stream (and its
#             SHA-256) can be recorded before any measurement. Every lane
#             consumes the identical stream front-to-back; the popularity of an
#             index is only discoverable online (the "unforeseen" property).
#
#   frontier  Aggregate the court's raw TSV rows into a frontier map and an
#             explicit, machine-readable verdict against the design §5
#             falsifiers. Declines are counted, never hidden.
#
# Usage:
#   phase15-frontier.py schedule --format F --out R.json [--per-type N] [--k K]
#   phase15-frontier.py frontier --raw RAW --campaign CAMP [--x 10] [--y 20] [--z 20]

import argparse
import hashlib
import json
import os
import statistics
import sys

SEED = 20261007
K = 8  # index range for a type: indices are drawn over 0..K-1
PER_TYPE = 8  # requests per revealed type
# The pre-registered query surface, revealed in this order, one type per depth.
TYPES = ["text", "metadata", "heading", "table", "resource"]

LANES = ["v_off", "v_on", "sq_min", "sq_full", "sq_adapt"]
LANE_NAME = {
    "v_off": "VOLE (no promotion)",
    "v_on": "VOLE --promote",
    "sq_min": "SQLite minimal",
    "sq_full": "SQLite full",
    "sq_adapt": "SQLite adaptive",
}


def lcg(state):
    return (1103515245 * state + 12345) % (1 << 31)


def draw_indices(n, k, seed):
    """n indices over 0..k-1 with a squared-uniform (Zipf-like, low-index-heavy,
    long-tailed) popularity, from a fixed LCG. Repeats are expected and are the
    point (they create the reuse the lanes must learn online)."""
    s = seed
    out = []
    for _ in range(n):
        s = lcg(s)
        u = (s % 10000) / 10000.0
        out.append(int(k * u * u))
    return out


def vole_arg(fmt, t, i):
    """The per-observation argument (VOLE observe grammar, no --store/--field)."""
    if t == "text":
        return f"--page {i + 1} --kind text" if fmt == "pdf" else f"--block {i} --kind text"
    if t == "metadata":
        return "--metadata --kind metadata"
    if t == "heading":
        return f"--heading {i} --kind text"
    if t == "table":
        return f"--table {i} --kind text"
    if t == "resource":
        return f"--resource {i} --kind metadata"
    raise SystemExit(f"unknown type {t!r}")


def build_schedule(fmt, per_type, k):
    idx = draw_indices(per_type, k, SEED)  # same index sequence for every type
    reqs = []
    for t in TYPES:
        for i in idx:
            reqs.append({"type": t, "i": i, "arg": vole_arg(fmt, t, i)})
    doc = {
        "format": fmt,
        "seed": SEED,
        "k": k,
        "per_type": per_type,
        "types": TYPES,
        "requests": reqs,
    }
    canon = json.dumps(doc, sort_keys=True, separators=(",", ":")).encode()
    doc["schedule_sha256"] = hashlib.sha256(canon).hexdigest()
    return doc


def read_tsv(path):
    if not os.path.exists(path):
        return [], []
    with open(path) as fh:
        lines = [ln for ln in fh if ln.strip()]
    if not lines:
        return [], []
    hdr = lines[0].rstrip("\n").split("\t")
    rows = []
    for ln in lines[1:]:
        vals = ln.rstrip("\n").split("\t")
        if len(vals) != len(hdr):
            continue
        rows.append(dict(zip(hdr, vals)))
    return hdr, rows


def median(xs):
    xs = [x for x in xs if x is not None]
    return statistics.median(xs) if xs else None


def median_by_depth(rows, lane, field="total_ms"):
    """median over documents of a lane's metric at each depth."""
    by_depth = {}
    for r in rows:
        if r["lane"] != lane:
            continue
        try:
            d = int(r["depth"])
        except ValueError:
            continue
        v = r.get(field, "")
        if v == "" or v is None:
            continue
        by_depth.setdefault(d, []).append(float(v))
    return {d: median(v) for d, v in sorted(by_depth.items())}


def fmt_ms(x):
    return "—" if x is None else f"{x:.1f}"


def main(argv):
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)

    s = sub.add_parser("schedule")
    s.add_argument("--format", required=True)
    s.add_argument("--out", required=True)
    s.add_argument("--per-type", type=int, default=PER_TYPE)
    s.add_argument("--k", type=int, default=K)

    f = sub.add_parser("frontier")
    f.add_argument("--raw", required=True)
    f.add_argument("--campaign", required=True)
    f.add_argument("--x", type=float, default=10.0)
    f.add_argument("--y", type=float, default=20.0)
    f.add_argument("--z", type=float, default=20.0)

    a = ap.parse_args(argv)

    if a.cmd == "schedule":
        doc = build_schedule(a.format, a.per_type, a.k)
        with open(a.out, "w") as fh:
            json.dump(doc, fh, sort_keys=True, indent=1)
        print(json.dumps({"out": a.out, "requests": len(doc["requests"]),
                          "schedule_sha256": doc["schedule_sha256"]}))
        return 0

    # ---- frontier aggregation ---------------------------------------------
    _h, drows = read_tsv(os.path.join(a.raw, "diversity.tsv"))
    _h2, rrows = read_tsv(os.path.join(a.raw, "revision.tsv"))

    out = []
    verdict = {"falsifiers": {}}
    out.append("# Phase 15.6 — adaptive procedural promotion: court frontier")
    out.append("")
    out.append(f"Lanes: {', '.join(LANES)}. Falsifier thresholds: "
               f"X={a.x:g}%, Y={a.y:g}%, Z={a.z:g}% (design §5). "
               f"Metric = median over documents of cumulative wall ms "
               f"(ingest + prefix queries) at each diversity depth.")

    # ---------- diversity ----------
    if drows:
        docs = sorted({r["doc"] for r in drows})
        depths = sorted({int(r["depth"]) for r in drows})
        out.append("")
        out.append(f"## Diversity — {len(docs)} documents: {', '.join(docs)}")
        out.append("")
        out.append("| depth | " + " | ".join(LANE_NAME[l] for l in LANES) +
                   " | best | v_on vs sq_adapt |")
        out.append("|---|" + "---:|" * (len(LANES) + 2))
        med = {l: median_by_depth(drows, l, "total_ms") for l in LANES}
        ratio = {}
        for d in depths:
            row = [med[l].get(d) for l in LANES]
            present = [(l, med[l][d]) for l in LANES if l in med and med[l].get(d) is not None]
            best = min(present, key=lambda kv: kv[1]) if present else (None, None)
            vo = med["v_on"].get(d)
            sa = med["sq_adapt"].get(d)
            if vo is not None and sa not in (None, 0):
                r = (vo - sa) / sa * 100.0
                ratio[d] = r
                rr = f"{r:+.1f}%"
            else:
                rr = "—"
            out.append("| {} | {} | {} | {} |".format(
                d, " | ".join(fmt_ms(med[l].get(d)) for l in LANES),
                LANE_NAME.get(best[0], "—") if best[0] else "—", rr))
        out.append("")
        # Falsifier 1: governed promotion never beats SQLite-Adaptive by > X%.
        crossing = [d for d, r in ratio.items() if r < -a.x]
        never = len(ratio) > 0 and all(r >= -a.x for r in ratio.values())
        verdict["falsifiers"]["diversity_sq_adapt_within_X"] = bool(never)
        verdict["diversity_ratio_v_on_vs_sq_adapt_pct"] = {str(d): ratio[d] for d in sorted(ratio)}
        verdict["diversity_crossing_depths"] = crossing
        if never:
            out.append(f"**Falsifier 1 (diversity) REFUTES promotion:** `v_on` is never "
                       f"more than {a.x:g}% faster than `sq_adapt` at any depth "
                       f"(crossing depths: none).")
        else:
            out.append(f"Falsifier 1 (diversity): `v_on` beats `sq_adapt` by >{a.x:g}% "
                       f"at depth(s) {crossing}.")

        # ---------- promotion vs write-everything (v_on vs v_off) ----------
        v_prom = median_by_depth(drows, "v_on", "aux_bytes")
        vo_all = median_by_depth(drows, "v_off", "aux_bytes")
        v_on_cut = []
        for d in depths:
            p, o = v_prom.get(d), vo_all.get(d)
            if p is not None and o not in (None, 0):
                v_on_cut.append((d, (o - p) / o * 100.0))
        max_cut = max((c for _, c in v_on_cut), default=0.0)
        faster = all(
            (med["v_on"].get(d) is None or med["v_off"].get(d) is None
             or med["v_on"][d] <= med["v_off"][d] * 1.000001)
            for d in depths)
        refutes_mech = not (max_cut >= a.z and faster)
        verdict["falsifiers"]["promotion_beats_write_everything"] = (not refutes_mech)
        verdict["v_on_aux_byte_cut_pct_max"] = max_cut
        out.append("")
        out.append(f"Falsifier 2 (mechanism): promoted-store bytes cut total durable "
                   f"bytes by at most **{max_cut:.1f}%** (need ≥{a.z:g}% at "
                   f"equal-or-better latency). "
                   + ("**REFUTED** — the governor's durable store adds no byte "
                      "distinction." if refutes_mech else "not refuted."))
    else:
        out.append("")
        out.append("_No diversity rows._")

    # ---------- revision ----------
    if rrows:
        out.append("")
        out.append("## Revision — retained cross-revision work")
        out.append("")
        out.append("| family | lane | C(R2 cold) ms | C(R2 shared) ms | retained % | "
                   "nodes_id_shared | shared_resource_ids | seed_nodes_reused |")
        out.append("|---|---|---:|---:|---:|---:|---:|---:|")
        retained = {}
        for r in rrows:
            try:
                cold = float(r["r2_cold_ms"])
                shared = float(r["r2_shared_ms"])
            except (KeyError, ValueError):
                continue
            ret = (cold - shared) / cold * 100.0 if cold else 0.0
            retained.setdefault(r["lane"], []).append(ret)
            out.append("| {} | {} | {:.1f} | {:.1f} | {:+.1f} | {} | {} | {} |".format(
                r["family"], LANE_NAME.get(r["lane"], r["lane"]), cold, shared, ret,
                r.get("nodes_id_shared", ""), r.get("shared_resource_ids", ""),
                r.get("seed_nodes_reused", "")))
        med = {l: median(v) for l, v in retained.items()}
        best_lane = max(med, key=lambda k: med[k]) if med else None
        best_med = med.get(best_lane) if best_lane else None
        refutes_rev = best_med is None or best_med < a.y
        verdict["falsifiers"]["revision_retained_ge_Y"] = (not refutes_rev)
        verdict["revision_retained_pct_median"] = med
        out.append("")
        out.append(f"Median retained: " + ", ".join(
            f"{LANE_NAME.get(l, l)} {m:+.1f}%" for l, m in med.items()) + ".")
        if refutes_rev:
            out.append(f"**Falsifier 3 (revision) REFUTES the retention claim:** "
                       f"the best lane's median retained cross-revision work is "
                       f"{best_med:+.1f}% < Y={a.y:g}%." if best_med is not None
                       else f"**Falsifier 3 (revision) REFUTES:** no measurable retention.")
        else:
            out.append(f"Falsifier 3 (revision): best median retained "
                       f"{best_med:+.1f}% ≥ Y={a.y:g}% (not refuted).")
    else:
        out.append("")
        out.append("_No revision rows._")

    report = "\n".join(out) + "\n"
    with open(os.path.join(a.campaign, "SUMMARY.md"), "w") as fh:
        fh.write(report)
    with open(os.path.join(a.campaign, "verdict.json"), "w") as fh:
        json.dump(verdict, fh, sort_keys=True, indent=1)
    print(report)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
