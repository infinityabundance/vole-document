#!/usr/bin/env python3
# phase15-packed aggregator (Phase 15.3).
#
# Reads the packed court's per-document rows and reports, honestly, the packed
# seed store vs the one-file-per-node reference: the persistent-byte ratio, the
# file-count ratio, the cold-observation latency ratio, and byte-exact
# reconstruction for BOTH backends. It does NOT assert that packed is better.
#
# Usage: python3 tools/fixtures/phase15-packed.py RAW_DIR CAMPAIGN_DIR

import os
import statistics
import sys

TIE = 0.10  # within ±10% of 1.0 is reported as "≈parity"


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


def to_int(s):
    try:
        return int(s)
    except (TypeError, ValueError):
        return 0


def median(xs):
    return statistics.median(xs) if xs else None


def fmt_ratio(r):
    return "—" if r is None else f"{r:.3f}x"


def describe_ratio(r):
    if r is None:
        return "—"
    if abs(r - 1.0) <= TIE:
        return "≈parity"
    return "packed smaller" if r < 1 else "packed larger"


def main(argv):
    if len(argv) != 3:
        sys.stderr.write(__doc__)
        return 2
    raw, campaign = argv[1], argv[2]
    rows = read_tsv(os.path.join(raw, "packed.tsv"))
    strace = read_tsv(os.path.join(raw, "strace.tsv"))

    ok = [r for r in rows if to_int(r["encode_rc"]) == 0]

    byte_ratios, file_ratios, lat_ratios = [], [], []
    for r in ok:
        fb, pb = to_int(r["fs_bytes"]), to_int(r["packed_bytes"])
        ff, pf = to_int(r["fs_files"]), to_int(r["packed_files"])
        fm, pm = to_int(r["fs_cold_ms"]), to_int(r["packed_cold_ms"])
        if fb > 0:
            byte_ratios.append(pb / fb)
        if ff > 0:
            file_ratios.append(pf / ff)
        if fm > 0 and pm > 0:
            lat_ratios.append(pm / fm)

    out = []
    out.append("# phase15-packed — packed seed store vs one-file-per-node reference")
    out.append("")
    out.append(
        f"Documents: **{len(rows)}** (SUBSET of `real100-v1`, joined on the "
        f"`(format, size_class)` strata — **not** the frozen 100-document "
        f"population). Ratios are `packed / fs`; a ratio < 1 means packed is "
        f"smaller/faster, > 1 means the reverse. Within ±{int(TIE*100)}% of 1.0 is "
        f"labelled parity. This court makes **no** claim that packed is better."
    )
    out.append("")

    # ---- exactness + field identity -------------------------------------
    fsok = sum(1 for r in ok if to_int(r["fs_exact_ok"]) == 1)
    pkok = sum(1 for r in ok if to_int(r["packed_exact_ok"]) == 1)
    feq = sum(1 for r in ok if to_int(r["field_equal"]) == 1)
    out.append("## Correctness")
    out.append("")
    out.append("| check | pass | of |")
    out.append("|---|---:|---:|")
    out.append(f"| fs `materialize --exact` (sha256+len == manifest) | {fsok} | {len(ok)} |")
    out.append(f"| packed `materialize --exact` (sha256+len == manifest) | {pkok} | {len(ok)} |")
    out.append(f"| field id identical across the two stores | {feq} | {len(ok)} |")
    out.append("")

    # ---- ratios ----------------------------------------------------------
    tot_fs = sum(to_int(r["fs_bytes"]) for r in ok)
    tot_pk = sum(to_int(r["packed_bytes"]) for r in ok)
    tot_ff = sum(to_int(r["fs_files"]) for r in ok)
    tot_pf = sum(to_int(r["packed_files"]) for r in ok)
    tot_fs_ms = sum(to_int(r["fs_cold_ms"]) for r in ok if to_int(r["fs_cold_rc"]) == 0)
    tot_pk_ms = sum(to_int(r["packed_cold_ms"]) for r in ok if to_int(r["packed_cold_rc"]) == 0)
    out.append("## Ratios (packed / fs)")
    out.append("")
    out.append("| metric | sum(fs) | sum(packed) | sum ratio | median per-doc ratio | reading |")
    out.append("|---|---:|---:|---:|---:|---|")
    br = (tot_pk / tot_fs) if tot_fs else None
    fr = (tot_pf / tot_ff) if tot_ff else None
    lr = (tot_pk_ms / tot_fs_ms) if tot_fs_ms else None
    out.append(f"| persistent bytes (`du -sb`) | {tot_fs} | {tot_pk} | {fmt_ratio(br)} | "
               f"{fmt_ratio(median(byte_ratios))} | {describe_ratio(median(byte_ratios))} |")
    out.append(f"| file count (`find -type f`) | {tot_ff} | {tot_pf} | {fmt_ratio(fr)} | "
               f"{fmt_ratio(median(file_ratios))} | {describe_ratio(median(file_ratios))} |")
    out.append(f"| cold observation wall (ms) | {tot_fs_ms} | {tot_pk_ms} | {fmt_ratio(lr)} | "
               f"{fmt_ratio(median(lat_ratios))} | {describe_ratio(median(lat_ratios))} |")
    out.append("")
    out.append(f"Medians are over the {len(ok)} documents with a successful encode "
               f"(bytes/files over all of them; latency only over documents where "
               f"both cold reads returned rc=0).")
    out.append("")

    # ---- per document ----------------------------------------------------
    out.append("### Per document")
    out.append("")
    out.append("| id | fmt | size class | src B | fs B | packed B | B ratio | fs files | packed files | files ratio | fs cold ms | packed cold ms | lat ratio | fs exact | packed exact | field = |")
    out.append("|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---|---|")
    for r in rows:
        fb, pb = to_int(r["fs_bytes"]), to_int(r["packed_bytes"])
        ff, pf = to_int(r["fs_files"]), to_int(r["packed_files"])
        fm, pm = to_int(r["fs_cold_ms"]), to_int(r["packed_cold_ms"])
        out.append("| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |".format(
            r["id"], r["fmt"], r["sclass"], r["byte_len"], fb, pb,
            fmt_ratio(pb / fb if fb else None), ff, pf, fmt_ratio(pf / ff if ff else None),
            fm if to_int(r["fs_cold_rc"]) == 0 else "err",
            pm if to_int(r["packed_cold_rc"]) == 0 else "err",
            fmt_ratio(pm / fm if fm and pm else None),
            "yes" if to_int(r["fs_exact_ok"]) == 1 else "no",
            "yes" if to_int(r["packed_exact_ok"]) == 1 else "no",
            "yes" if to_int(r["field_equal"]) == 1 else "**no**"))
    out.append("")

    # ---- strace ----------------------------------------------------------
    out.append("### Syscall summary (`strace -c -f`, one representative document)")
    out.append("")
    out.append("The `doc-baseline` lane has `strace` but not `perf`; these are "
               "whole-process syscall totals for one cold observation per backend.")
    out.append("")
    if strace:
        out.append("| id | backend | total seconds | total syscalls | raw |")
        out.append("|---|---|---:|---:|---|")
        for r in strace:
            out.append(f"| {r['id']} | {r['backend']} | {r['seconds']} | {r['calls']} | "
                       f"`{r['id']}.{r['backend']}.strace.txt` |")
    else:
        out.append("_No strace rows recorded (representative document not reached)._")
    out.append("")

    report = "\n".join(out) + "\n"
    with open(os.path.join(campaign, "SUMMARY.md"), "w") as fh:
        fh.write(report)
    print(report)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
