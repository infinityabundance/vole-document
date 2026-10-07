#!/usr/bin/env python3
# phase15 DEFLATE-ablation aggregator (Phase 15.5).
#
# Each candidate writes a JSONL file (one line PER DOCUMENT) via
# `--out`, plus a final aggregate JSON on stdout and a `/usr/bin/time -v`
# report. This sums the per-document lines and takes peak RSS from the
# `/usr/bin/time -v` "Maximum resident set size" (falling back to the harness's
# own `peak_rss_kib`), then compares decoded GB/s and RSS against the miniz_oxide
# reference, gated on byte-identical output over the whole corpus.
#
# Adoption bar is pre-registered (15.5 design): a non-miniz candidate would be
# adopted only at >= 1.25x GB/s AND peak RSS <= 1.10x. This court MEASURES; it
# does not switch the shipped inflater.
#
# Usage: python3 tools/fixtures/phase15-deflate.py RAW_DIR CAMPAIGN_DIR

import json
import os
import re
import sys

REF = "miniz"
BADOPT = 1.25
BADRSS = 1.10
ORDER = ["miniz", "miniz-simd", "zlib-rs", "zune-inflate"]


def peak_rss_kib(raw, tag):
    tpath = os.path.join(raw, f"{tag}.time")
    try:
        with open(tpath) as fh:
            m = re.search(r"Maximum resident set size \(kbytes\):\s*(\d+)", fh.read())
            if m:
                return int(m.group(1))
    except OSError:
        pass
    # Fall back to the harness's own final stdout line.
    spath = os.path.join(raw, f"{tag}.stdout")
    try:
        with open(spath) as fh:
            lines = [ln for ln in fh if ln.strip().startswith("{")]
        if lines:
            return int(json.loads(lines[-1]).get("peak_rss_kib", 0))
    except OSError:
        pass
    return 0


def load(raw, tag):
    path = os.path.join(raw, f"{tag}.json")
    agg = {"members": 0, "compressed_bytes": 0, "decoded_bytes": 0,
           "wall_ms": 0.0, "mismatches": 0, "declined": 0, "docs": 0, "config": "?"}
    try:
        with open(path) as fh:
            for line in fh:
                line = line.strip()
                if not line:
                    continue
                r = json.loads(line)
                agg["docs"] += 1
                agg["config"] = r.get("config", agg["config"])
                for k in ("members", "compressed_bytes", "decoded_bytes",
                          "mismatches", "declined"):
                    agg[k] += r.get(k, 0)
                agg["wall_ms"] += r.get("wall_ms", 0.0)
    except OSError:
        return None
    if agg["wall_ms"] > 0:
        agg["gbps"] = agg["decoded_bytes"] / (agg["wall_ms"] / 1000.0) / 1e9
    else:
        agg["gbps"] = 0.0
    agg["peak_rss_kib"] = peak_rss_kib(raw, tag)
    return agg


def main(argv):
    if len(argv) != 3:
        sys.stderr.write(__doc__)
        return 2
    raw, campaign = argv[1], argv[2]
    rows = {}
    for tag in ORDER:
        r = load(raw, tag)
        if r is not None:
            rows[tag] = r
    tags = [t for t in ORDER if t in rows] + sorted(set(rows) - set(ORDER))

    out = []
    out.append("# phase15 deflate-ablation — real `real100-v1` compressed members")
    out.append("")
    out.append("Each candidate decodes the SAME extracted members (PDF `FlateDecode` "
               "zlib streams + ZIP method-8 DEFLATE members); every output must equal the "
               "`miniz_oxide` reference byte-for-byte (`mismatches == 0`). Wall time "
               "measures inflate only (extraction happens before timing). One candidate per "
               "process, so peak RSS is a clean per-candidate number.")
    out.append("")

    ref = rows.get(REF)
    out.append("| candidate | config | docs | members | compressed MiB | decoded MiB | wall ms | GB/s | GB/s vs miniz | mismatches | peak RSS MiB | RSS vs miniz | meets 1.25x/1.10 bar |")
    out.append("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|")
    for t in tags:
        r = rows[t]
        rss_mib = r["peak_rss_kib"] / 1024.0
        if ref and t != REF and ref["gbps"] > 0 and r["peak_rss_kib"]:
            gr = r["gbps"] / ref["gbps"]
            rr = r["peak_rss_kib"] / ref["peak_rss_kib"]
            bar = "yes" if (gr >= BADOPT and rr <= BADRSS) else "no"
            gr_s, rr_s = f"{gr:.2f}x", f"{rr:.2f}x"
        else:
            gr_s, rr_s, bar = "reference", "reference", "—"
        out.append(
            "| {} | {} | {} | {} | {:.1f} | {:.1f} | {:.1f} | {:.3f} | {} | {} | {:.1f} | {} | {} |".format(
                t, r["config"], r["docs"], r["members"],
                r["compressed_bytes"] / 1048576.0, r["decoded_bytes"] / 1048576.0,
                r["wall_ms"], r["gbps"], gr_s, r["mismatches"], rss_mib, rr_s, bar))
    out.append("")

    total_mismatch = sum(r["mismatches"] for r in rows.values())
    if rows and total_mismatch == 0:
        out.append("**Correctness gate: PASS** — every candidate decoded the corpus "
                   "byte-identically to the miniz_oxide reference (0 mismatches).")
    else:
        out.append(f"**Correctness gate: FAIL** — {total_mismatch} mismatched members. "
                   "No candidate may be adopted.")
    out.append("")
    out.append(f"Adoption bar (pre-registered): >= {BADOPT}x GB/s AND <= {BADRSS}x peak RSS "
               "vs miniz_oxide. A `yes` means a candidate MEETS the bar; it does not mean "
               "the shipped inflater was changed — this court measures only.")
    out.append("")

    report = "\n".join(out) + "\n"
    with open(os.path.join(campaign, "SUMMARY.md"), "w") as fh:
        fh.write(report)
    print(report)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
