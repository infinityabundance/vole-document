#!/usr/bin/env python3
# real100-v1 frontier-map aggregator.
#
# Reads the court's raw rows and emits the frontier map: per (stratum, workload)
# it reports, for each lane, whether it answered and its median wall cost, and
# which lane is fastest. Declines are counted, never hidden.
#
#   V  = Phase-11/12 procedural DocumentField (frozen)
#   A1 = one-time-preprocessed source-retaining SQLite + FTS5
#   A0 = direct per-query tooling
#
# Usage: python3 tools/fixtures/real100-frontier.py RAW_DIR CAMPAIGN_DIR

import json
import os
import statistics
import sys

LANES = ["v", "a1", "a0"]
LANE_NAME = {"v": "VOLE", "a1": "SQLite/FTS", "a0": "direct tooling"}
WORKLOADS = ["text_once", "text_repeat", "heading", "table", "resource",
             "metadata", "exact"]
TIE = 0.10  # within ±10% of the fastest median is a tie


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


def cell_verdict(rows_by_lane):
    """rows_by_lane: {lane: [wall_ms ints]} for ANSWERED ops only.
    Returns {lane: 'win'|'loss'|'tie'|'decline'|'-'} plus the fastest lane."""
    med = {}
    for ln in LANES:
        xs = [int(x) for x in rows_by_lane.get(ln, [])]
        if xs:
            med[ln] = statistics.median(xs)
    if not med:
        return {ln: "-" for ln in LANES}, None
    fastest = min(med, key=lambda k: med[k])
    out = {}
    for ln in LANES:
        if ln not in med:
            out[ln] = "decline"
        elif med[ln] <= med[fastest] * (1.0 + TIE):
            out[ln] = "win" if ln == fastest else "tie"
        else:
            out[ln] = "loss"
    return out, fastest


def group(ops, keyfn):
    g = {}
    for r in ops:
        g.setdefault(keyfn(r), []).append(r)
    return g


def frontier_table(ops, keyfn, title, order=None):
    g = group(ops, keyfn)
    keys = order or sorted(g)
    lines = [f"### {title}", "",
             "| stratum | workload | VOLE | SQLite/FTS | direct tooling | fastest |",
             "|---|---|---|---|---|---|"]
    for k in keys:
        for w in WORKLOADS:
            cell = [r for r in g.get(k, []) if r["workload"] == w]
            if not cell:
                continue
            by_lane = {}
            for r in cell:
                if r["rc"] == "0":
                    by_lane.setdefault(r["lane"], []).append(r["wall_ms"])
                else:
                    by_lane.setdefault(r["lane"], [])
            verdict, fastest = cell_verdict(by_lane)
            lines.append(
                "| {} | {} | {} | {} | {} | {} |".format(
                    k, w, verdict["v"], verdict["a1"], verdict["a0"],
                    LANE_NAME.get(fastest, "—")))
    lines.append("")
    return "\n".join(lines)


def main(argv):
    if len(argv) != 3:
        sys.stderr.write(__doc__)
        return 2
    raw, campaign = argv[1], argv[2]
    ops = read_tsv(os.path.join(raw, "ops.tsv"))
    onetime = read_tsv(os.path.join(raw, "onetime.tsv"))
    exact = read_tsv(os.path.join(raw, "exact.tsv"))

    docs = sorted({r["id"] for r in ops})
    out = []
    out.append("# real100-v1 frontier map")
    out.append("")
    out.append(f"Documents measured: **{len(docs)}**. "
               f"Lanes: VOLE (frozen), SQLite/FTS (A1), direct tooling (A0). "
               f"Tie band: ±{int(TIE*100)}% of the fastest median. "
               f"`decline` = the lane has no such observation for the format / "
               f"returned a typed error.")
    out.append("")

    # declines per lane overall
    dec = {ln: sum(1 for r in ops if r["lane"] == ln and r["rc"] != "0")
           for ln in LANES}
    ans = {ln: sum(1 for r in ops if r["lane"] == ln and r["rc"] == "0")
           for ln in LANES}
    out.append("## Answered vs declined (all ops)")
    out.append("")
    out.append("| lane | answered | declined |")
    out.append("|---|---:|---:|")
    for ln in LANES:
        out.append(f"| {LANE_NAME[ln]} | {ans[ln]} | {dec[ln]} |")
    out.append("")

    out.append(frontier_table(ops, lambda r: "all", "Overall"))
    out.append(frontier_table(ops, lambda r: r["fmt"], "By format",
                              order=["pdf", "docx", "epub"]))
    out.append(frontier_table(ops, lambda r: r["sclass"], "By size class"))

    # one-time costs
    out.append("### One-time costs (build/ingest, per document)")
    out.append("")
    out.append("| id | format | size | VOLE encode ms | VOLE ingest ms | VOLE bytes | A1 build ms | A1 db bytes |")
    out.append("|---|---|---|---:|---:|---:|---:|---:|")
    for r in onetime:
        out.append("| {} | {} | {} | {} | {} | {} | {} | {} |".format(
            r["id"], r["fmt"], r["sclass"], r["venc_ms"], r["ving_ms"],
            r["v_bytes"], r["a1_build_ms"], r["a1_bytes"]))
    out.append("")

    # exactness
    vok = sum(1 for r in exact if r["v_ok"] == "1")
    a1ok = sum(1 for r in exact if r["a1_ok"] == "1")
    a0ok = sum(1 for r in exact if r["a0_ok"] == "1")
    n = len(exact)
    out.append("### Exact reconstruction (length + SHA-256 of materialized bytes)")
    out.append("")
    out.append(f"| lane | byte-exact | of |")
    out.append("|---|---:|---:|")
    out.append(f"| VOLE `materialize --exact` | {vok} | {n} |")
    out.append(f"| SQLite/FTS (retained source blob) | {a1ok} | {n} |")
    out.append(f"| direct tooling (the source file) | {a0ok} | {n} |")
    out.append("")

    report = "\n".join(out) + "\n"
    with open(os.path.join(campaign, "SUMMARY.md"), "w") as fh:
        fh.write(report)
    print(report)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
