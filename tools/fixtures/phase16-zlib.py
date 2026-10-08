#!/usr/bin/env python3
# phase16 zlib-rs end-to-end aggregator (Phase 16.1).
#
# Reads the per-(document, arm, rep) rows written by
# `tools/phase16-zlib-endtoend-court.sh` and compares the OLD backend
# (`miniz_oxide`, the pre-Phase-16.1 shipped inflate) against the NEW backend
# (`zlib-rs`) on the SAME documents and the SAME lane. "arm" is `old` or `new`.
#
# What is compared: the MEDIAN `encode` wall ms, the MEDIAN `field-ingest` wall
# ms, their sum, and the peak `/usr/bin/time -v` RSS of each. `encode` never
# inflates (a stream's exact leaf is its raw compressed span), so it is the
# control column: any encode delta is lane noise, and it bounds how small an
# ingest delta this court can honestly resolve. The delta lives in `field-ingest`.
#
# Correctness gate (from `exactness.tsv`): every row's `materialize --exact` must
# reproduce the manifest SHA-256/byte length, and both arms must yield the same
# field id.
#
# The Phase-15.5 microbench measured inflate alone at 1.58x (miniz->zlib-rs); the
# end-to-end win is necessarily smaller because inflate is only part of ingest.
# That honest gap is the point of this court.
#
# Usage: python3 tools/fixtures/phase16-zlib.py RAW_DIR CAMPAIGN_DIR

import os
import statistics
import sys

HEADER = [
    "id", "arm", "rep", "fmt", "sclass", "byte_len",
    "enc_rc", "enc_wall_ms", "enc_rss_kb",
    "ing_rc", "ing_wall_ms", "ing_rss_kb", "field",
]


def read_tsv(path, header):
    rows = []
    try:
        with open(path) as fh:
            first = fh.readline().rstrip("\n").split("\t")
            if first != header:  # first line was data, not a header
                fh.seek(0)
            for line in fh:
                line = line.rstrip("\n")
                if not line:
                    continue
                cols = line.split("\t")
                if len(cols) == len(header):
                    rows.append(dict(zip(header, cols)))
    except OSError:
        return []
    return rows


def num(v):
    try:
        return int(v)
    except (TypeError, ValueError):
        return 0


def med(vals):
    return int(statistics.median(vals)) if vals else 0


def summarize(rows):
    """(id, arm) -> median encode ms, median ingest ms, peak RSS KB, rc ok count."""
    groups = {}
    for r in rows:
        groups.setdefault((r["id"], r["arm"]), []).append(r)
    out = {}
    for key, rs in groups.items():
        enc = [num(r["enc_wall_ms"]) for r in rs if r["enc_rc"] == "0"]
        ing = [num(r["ing_wall_ms"]) for r in rs if r["ing_rc"] == "0"]
        enc_rss = max((num(r["enc_rss_kb"]) for r in rs), default=0)
        ing_rss = max((num(r["ing_rss_kb"]) for r in rs), default=0)
        out[key] = {
            "enc_ms": med(enc),
            "ing_ms": med(ing),
            "enc_rss": enc_rss,
            "ing_rss": ing_rss,
            "reps": len(rs),
            "ok": len(enc) == len(rs) and len(ing) == len(rs),
            "fmt": rs[0]["fmt"],
            "sclass": rs[0]["sclass"],
            "byte_len": rs[0]["byte_len"],
        }
    return out


def ratio(new, old):
    if old <= 0:
        return "n/a"
    return "%.3fx" % (new / old)


def main(argv):
    if len(argv) != 3:
        sys.stderr.write(__doc__)
        return 2
    raw, campaign = argv[1], argv[2]
    rows = read_tsv(os.path.join(raw, "endtoend.tsv"), HEADER)
    exact_rows = read_tsv(os.path.join(raw, "exactness.tsv"),
                          ["id", "arm", "field", "exact"])
    g = summarize(rows)

    def arm_rows(arm):
        return {k: v for k, v in g.items() if k[1] == arm}

    old, new = arm_rows("old"), arm_rows("new")
    ids = sorted({k[0] for k in g})

    out = ["# phase16 zlib-rs inflate backend — end-to-end impact", ""]
    out.append("OLD = `miniz_oxide` (pre-16.1 inflate), NEW = `zlib-rs` (shipped "
               "backend). Same documents, same capped `doc-baseline` lane, same "
               "`--features docx,epub` build; one process per operation. Wall ms is "
               "the median over the court's reps. `encode` does not inflate and is "
               "the control; the delta is `field-ingest`.")
    out.append("")
    out.append("| id | fmt | class | bytes | arm | encode ms (med) | encode RSS MiB | "
               "ingest ms (med) | ingest RSS MiB |")
    out.append("|---|---|---|---:|---|---:|---:|---:|---:|")
    for i in ids:
        for arm in ("old", "new"):
            v = g.get((i, arm))
            if not v:
                continue
            out.append("| {} | {} | {} | {} | {} | {} | {:.1f} | {} | {:.1f} |".format(
                i, v["fmt"], v["sclass"], v["byte_len"], arm, v["enc_ms"],
                v["enc_rss"] / 1024.0, v["ing_ms"], v["ing_rss"] / 1024.0))
    out.append("")

    def total(d):
        return {
            "enc": sum(v["enc_ms"] for v in d.values()),
            "ing": sum(v["ing_ms"] for v in d.values()),
            "enc_rss": max((v["enc_rss"] for v in d.values()), default=0),
            "ing_rss": max((v["ing_rss"] for v in d.values()), default=0),
            "docs": len(d),
            "ok": sum(1 for v in d.values() if v["ok"]),
        }

    to, tn = total(old), total(new)
    out.append("## Aggregate (median per operation)")
    out.append("")
    out.append("| arm | docs | rc-ok | encode total ms | ingest total ms | total ms | "
               "peak encode RSS MiB | peak ingest RSS MiB |")
    out.append("|---|---:|---:|---:|---:|---:|---:|---:|")
    for name, t in (("old", to), ("new", tn)):
        out.append("| {} | {} | {}/{} | {} | {} | {} | {:.1f} | {:.1f} |".format(
            name, t["docs"], t["ok"], t["docs"], t["enc"], t["ing"],
            t["enc"] + t["ing"], t["enc_rss"] / 1024.0, t["ing_rss"] / 1024.0))
    out.append("")
    out.append("| quantity | old | new | new/old |")
    out.append("|---|---:|---:|---:|")
    out.append("| encode total ms | {} | {} | {} |".format(to["enc"], tn["enc"], ratio(tn["enc"], to["enc"])))
    out.append("| field-ingest total ms | {} | {} | {} |".format(to["ing"], tn["ing"], ratio(tn["ing"], to["ing"])))
    out.append("| encode+ingest total ms | {} | {} | {} |".format(
        to["enc"] + to["ing"], tn["enc"] + tn["ing"],
        ratio(tn["enc"] + tn["ing"], to["enc"] + to["ing"])))
    out.append("| peak ingest RSS MiB | {:.1f} | {:.1f} | {} |".format(
        to["ing_rss"] / 1024.0, tn["ing_rss"] / 1024.0, ratio(tn["ing_rss"], to["ing_rss"])))
    out.append("")

    # Correctness: exactness rows + field-id agreement.
    ex = {}
    for r in exact_rows:
        ex[(r["id"], r["arm"])] = r
    exact_ok = bool(ex) and all(r["exact"] == "1" for r in ex.values())
    f_old = {r["id"]: r["field"] for r in exact_rows if r["arm"] == "old"}
    f_new = {r["id"]: r["field"] for r in exact_rows if r["arm"] == "new"}
    agree = bool(f_old) and all(f_old.get(i) and f_old.get(i) == f_new.get(i) for i in f_old)
    out.append("Field-id agreement (old == new, per document): **{}**".format("yes" if agree else "NO"))
    out.append("Exactness gate (`materialize --exact` == manifest on every row): "
               "**{}**".format("PASS" if exact_ok else "FAIL"))
    out.append("")
    out.append("Caveat (labelled, not hidden): `encode` is backend-independent, so its "
               "new/old ratio is a measured NOISE FLOOR — an ingest delta smaller than "
               "that floor is not resolved by this court.")
    out.append("")
    out.append("Phase-15.5 microbench context: inflate alone was 1.58x GB/s "
               "(miniz_oxide scalar -> zlib-rs) and byte-identical. The end-to-end "
               "factor above is smaller because inflate is only one part of ingest; it "
               "is reported, not asserted.")

    report = "\n".join(out) + "\n"
    with open(os.path.join(campaign, "SUMMARY.md"), "w") as fh:
        fh.write(report)
    print(report)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
