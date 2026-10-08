#!/usr/bin/env python3
# phase16-packed-full aggregator (Phase 16.2).
#
# Reads the full-population packed-store court's per-document rows and answers,
# on the COMMON-SUCCESS population (documents where the VOLE fs ingest, the VOLE
# packed ingest, and the A1 SQLite build ALL succeeded), whether the packed
# backend closes the persistent-storage gap against SQLite.
#
# It reports, explicitly:
#   * whole-population totals + the number of common-success documents;
#   * the common-success comparison (VOLE fs vs SQLite, VOLE packed vs SQLite,
#     packed vs fs) in absolute bytes and ratios, overall and by format;
#   * a clear verdict on whether packed/SQLite <= 1.0x.
#
# It does NOT assert that packed is better; ratios are measured, not claimed.
# Terminology: a ratio < 1 means the numerator (VOLE) is smaller.
#
# Usage: python3 tools/fixtures/phase16-packed-full.py RAW_DIR CAMPAIGN_DIR

import json
import os
import statistics
import sys

FORMATS = ["pdf", "docx", "epub"]
RC_TIMEOUT = 124
RC_OOM = 137


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
        return None


def rc_of(r, col):
    v = to_int(r.get(col, ""))
    return -1 if v is None else v


def enc_ok(r):
    return rc_of(r, "venc_rc") == 0


def fs_ok(r):
    return rc_of(r, "ving_fs_rc") == 0


def pk_ok(r):
    return rc_of(r, "ving_pack_rc") == 0


def a1_ok(r):
    return rc_of(r, "a1_build_rc") == 0


def common_fs_a1(r):
    return fs_ok(r) and a1_ok(r)


def common_all(r):
    return fs_ok(r) and pk_ok(r) and a1_ok(r)


def b(r, col):
    v = to_int(r.get(col, ""))
    return v if v is not None else 0


def ratio(num, den):
    if not den:
        return None
    return num / den


def fmt_ratio(r):
    return "—" if r is None else f"{r:.3f}×"


def fmt_int(n):
    return f"{n:,}"


def median(xs):
    return statistics.median(xs) if xs else None


def compare(rows, with_pack=True):
    """Absolute-byte sums and ratios for one population.

    With ``with_pack=False`` the pack terms are omitted entirely: a population
    that does not require a successful packed ingest must not carry pack sums
    (they would silently undercount rows where packed failed).
    """
    sfs = sum(b(r, "fs_bytes") for r in rows)
    sa1 = sum(b(r, "a1_bytes") for r in rows)
    per_fs_a1 = [b(r, "fs_bytes") / b(r, "a1_bytes")
                 for r in rows if b(r, "a1_bytes") > 0]
    out = {
        "documents": len(rows),
        "fs_bytes": sfs,
        "a1_bytes": sa1,
        "fs_over_a1": ratio(sfs, sa1),
        "median_fs_over_a1": median(per_fs_a1),
    }
    if with_pack:
        spk = sum(b(r, "pack_bytes") for r in rows)
        per_pk_a1 = [b(r, "pack_bytes") / b(r, "a1_bytes")
                     for r in rows if b(r, "a1_bytes") > 0 and b(r, "pack_bytes") > 0]
        per_pk_fs = [b(r, "pack_bytes") / b(r, "fs_bytes")
                     for r in rows if b(r, "fs_bytes") > 0 and b(r, "pack_bytes") > 0]
        out.update({
            "pack_bytes": spk,
            "pack_over_a1": ratio(spk, sa1),
            "pack_over_fs": ratio(spk, sfs),
            "median_pack_over_a1": median(per_pk_a1),
            "median_pack_over_fs": median(per_pk_fs),
        })
    return out


def comparison_table(rows, title, with_pack=True):
    out = [f"#### {title}", ""]
    c = compare(rows, with_pack)
    if with_pack:
        out.append("| comparison | VOLE (numerator) B | SQLite / fs (denominator) B | sum ratio | median per-doc ratio |")
        out.append("|---|---:|---:|---:|---:|")
        out.append(f"| VOLE fs vs SQLite | {fmt_int(c['fs_bytes'])} | {fmt_int(c['a1_bytes'])} | "
                   f"{fmt_ratio(c['fs_over_a1'])} | {fmt_ratio(c['median_fs_over_a1'])} |")
        out.append(f"| VOLE packed vs SQLite | {fmt_int(c['pack_bytes'])} | {fmt_int(c['a1_bytes'])} | "
                   f"{fmt_ratio(c['pack_over_a1'])} | {fmt_ratio(c['median_pack_over_a1'])} |")
        out.append(f"| VOLE packed vs VOLE fs | {fmt_int(c['pack_bytes'])} | {fmt_int(c['fs_bytes'])} | "
                   f"{fmt_ratio(c['pack_over_fs'])} | {fmt_ratio(c['median_pack_over_fs'])} |")
    else:
        out.append("| comparison | VOLE fs B | SQLite B | sum ratio | median per-doc ratio |")
        out.append("|---|---:|---:|---:|---:|")
        out.append(f"| VOLE fs vs SQLite | {fmt_int(c['fs_bytes'])} | {fmt_int(c['a1_bytes'])} | "
                   f"{fmt_ratio(c['fs_over_a1'])} | {fmt_ratio(c['median_fs_over_a1'])} |")
    out.append("")
    return "\n".join(out)


def main(argv):
    if len(argv) != 3:
        sys.stderr.write(__doc__)
        return 2
    raw, campaign = argv[1], argv[2]
    rows = read_tsv(os.path.join(raw, "storage.tsv"))

    total = len(rows)
    n_enc = sum(1 for r in rows if enc_ok(r))
    n_fs = sum(1 for r in rows if fs_ok(r))
    n_pk = sum(1 for r in rows if pk_ok(r))
    n_a1 = sum(1 for r in rows if a1_ok(r))
    n_feq = sum(1 for r in rows if to_int(r.get("field_equal", "")) == 1)
    cf_a1 = [r for r in rows if common_fs_a1(r)]
    c_all = [r for r in rows if common_all(r)]

    # --- counts of rcs by code (never hide a failure) -----------------------
    def rc_hist(col):
        h = {}
        for r in rows:
            code = rc_of(r, col)
            h[code] = h.get(code, 0) + 1
        return {str(k): v for k, v in sorted(h.items())}

    populations = {
        "documents": total,
        "encode_ok": n_enc,
        "fs_ingest_ok": n_fs,
        "packed_ingest_ok": n_pk,
        "a1_build_ok": n_a1,
        "field_equal": n_feq,
        "common_fs_a1": len(cf_a1),
        "common_all": len(c_all),
        "rc_histogram": {
            "venc_rc": rc_hist("venc_rc"),
            "ving_fs_rc": rc_hist("ving_fs_rc"),
            "ving_pack_rc": rc_hist("ving_pack_rc"),
            "a1_build_rc": rc_hist("a1_build_rc"),
        },
    }

    whole = {
        "documents": total,
        "fs_bytes_ok_only": sum(b(r, "fs_bytes") for r in rows if fs_ok(r)),
        "pack_bytes_ok_only": sum(b(r, "pack_bytes") for r in rows if pk_ok(r)),
        "a1_bytes_ok_only": sum(b(r, "a1_bytes") for r in rows if a1_ok(r)),
        "fs_documents": n_fs,
        "pack_documents": n_pk,
        "a1_documents": n_a1,
    }

    all_cmp = compare(c_all, True)
    verdict_closes = (all_cmp["pack_over_a1"] is not None
                      and all_cmp["pack_over_a1"] <= 1.0)
    verdict = {
        "common_success_documents": len(c_all),
        "pack_over_a1_sum": all_cmp["pack_over_a1"],
        "pack_over_a1_median": all_cmp["median_pack_over_a1"],
        "fs_over_a1_sum": all_cmp["fs_over_a1"],
        "closes_gap_packed_le_1x": bool(verdict_closes),
    }

    failures = []
    for r in rows:
        codes = {"venc_rc": rc_of(r, "venc_rc"), "ving_fs_rc": rc_of(r, "ving_fs_rc"),
                 "ving_pack_rc": rc_of(r, "ving_pack_rc"), "a1_build_rc": rc_of(r, "a1_build_rc")}
        bad = {k: v for k, v in codes.items() if v not in (0, -1)}
        skipped = {k: v for k, v in codes.items() if v == -1}
        if bad or skipped:
            failures.append({"id": r["id"], "fmt": r["fmt"], "sclass": r["sclass"],
                             "byte_len": to_int(r["byte_len"]), "bad_rc": bad, "skipped": list(skipped)})

    # --- write SUMMARY.md ---------------------------------------------------
    out = []
    out.append("# Phase 16.2 — full `real100-v1` packed-store storage court")
    out.append("")
    out.append(
        f"Documents in population: **{total}**. Profile release, "
        f"`doc-baseline` (6 GiB, cpus 8). Question: on the **common-success** "
        f"population, does the `--packed` backend close the persistent-storage "
        f"gap against the A1 SQLite db? Ratios are VOLE/SQLite (and pack/fs); "
        f"< 1 means VOLE is smaller. The established baseline "
        f"(`2026-10-07-real100-release-baseline-866f489`) reported fs/SQLite = "
        f"**1.377×** on 95 common-success documents."
    )
    out.append("")

    out.append("## Population and success")
    out.append("")
    out.append("| stage | succeeded |")
    out.append("|---|---:|")
    out.append(f"| encode (`venc_rc==0`) | {n_enc} / {total} |")
    out.append(f"| VOLE fs `field-ingest` | {n_fs} / {total} |")
    out.append(f"| VOLE packed `field-ingest --packed` | {n_pk} / {total} |")
    out.append(f"| A1 SQLite build | {n_a1} / {total} |")
    out.append(f"| field id identical (fs == packed) | {n_feq} / {min(n_fs, n_pk)} |")
    out.append("")
    out.append(f"* common success, fs + A1 (the 1.377× population shape): **{len(cf_a1)}**")
    out.append(f"* common success, fs + packed + A1 (**the head-to-head**): **{len(c_all)}**")
    out.append("")
    hist = populations["rc_histogram"]
    out.append("Exit-code histogram (`0` success, `124` timeout, `137` SIGKILL/OOM, "
               "`-1` not run because encode failed):")
    out.append("")
    out.append("| stage | rc -> count |")
    out.append("|---|---|")
    for k, h in hist.items():
        out.append(f"| {k} | " + ", ".join(f"`{rc}`×{n}" for rc, n in h.items()) + " |")
    out.append("")

    out.append("## Whole-population totals (successful footprints only)")
    out.append("")
    out.append("| substrate | docs | bytes |")
    out.append("|---|---:|---:|")
    out.append(f"| VOLE fs store | {n_fs} | {fmt_int(whole['fs_bytes_ok_only'])} |")
    out.append(f"| VOLE packed store | {n_pk} | {fmt_int(whole['pack_bytes_ok_only'])} |")
    out.append(f"| A1 SQLite db | {n_a1} | {fmt_int(whole['a1_bytes_ok_only'])} |")
    out.append("")
    out.append("_Whole-population totals are over different document sets per "
               "substrate and must never be read as a head-to-head._")
    out.append("")

    out.append("## Head-to-head — common success (fs + packed + A1)")
    out.append("")
    out.append(f"Population: **{len(c_all)}** documents.")
    out.append("")
    out.append(comparison_table(c_all, "Overall"))
    out.append("### By format")
    out.append("")
    for fmt in FORMATS:
        sub = [r for r in c_all if r["fmt"] == fmt]
        if sub:
            out.append(comparison_table(sub, f"{fmt} ({len(sub)} docs)"))
    other = [r for r in c_all if r["fmt"] not in FORMATS]
    if other:
        out.append(comparison_table(other, f"other ({len(other)} docs)"))
    out.append("")

    out.append("## Reconciliation — common success (fs + A1 only)")
    out.append("")
    out.append(f"Population: **{len(cf_a1)}** documents (the shape the 1.377× "
               f"baseline used).")
    out.append("")
    out.append(comparison_table(cf_a1, "Overall", with_pack=False))
    for fmt in FORMATS:
        sub = [r for r in cf_a1 if r["fmt"] == fmt]
        if sub:
            out.append(comparison_table(sub, f"{fmt} ({len(sub)} docs)", with_pack=False))
    out.append("")

    out.append("## Verdict")
    out.append("")
    if all_cmp["pack_over_a1"] is None:
        out.append("Insufficient common-success data to compute packed/SQLite.")
    else:
        poa_sum = all_cmp["pack_over_a1"]
        poa_med = all_cmp["median_pack_over_a1"]
        foa_sum = all_cmp["fs_over_a1"]
        margin = 1.0 - poa_sum
        if verdict_closes:
            out.append(
                f"**Packed CLOSES the gap**: packed/SQLite = **{fmt_ratio(poa_sum)}** "
                f"(sum, <= 1.0×), median per-doc {fmt_ratio(poa_med)} — the packed "
                f"store is {'smaller than' if poa_sum < 1 else 'at parity with'} the "
                f"SQLite db on the {len(c_all)} common-success documents "
                f"(margin {margin:+.3f}× below 1.0). For the same population "
                f"fs/SQLite = {fmt_ratio(foa_sum)}."
            )
        else:
            out.append(
                f"**Packed does NOT close the gap**: packed/SQLite = "
                f"**{fmt_ratio(poa_sum)}** (sum, > 1.0×), median per-doc "
                f"{fmt_ratio(poa_med)} — the packed store is still "
                f"{poa_sum:.3f}× the SQLite db on the {len(c_all)} common-success "
                f"documents. For the same population fs/SQLite = "
                f"{fmt_ratio(foa_sum)}, so packed improves on fs by "
                f"{all_cmp['pack_over_fs']:.3f}× but does not reach parity with SQLite."
            )
    out.append("")

    out.append("## Failures (never hidden)")
    out.append("")
    if not failures:
        out.append("None: every stage succeeded for every document.")
    else:
        out.append("| id | fmt | size class | bytes | failing rc | not run |")
        out.append("|---|---|---|---:|---|---|")
        for f in failures:
            bad = ", ".join(f"{k}={v}" for k, v in f["bad_rc"].items()) or "—"
            skip = ", ".join(f["skipped"]) or "—"
            out.append(f"| {f['id']} | {f['fmt']} | {f['sclass']} | "
                       f"{fmt_int(f['byte_len'] or 0)} | {bad} | {skip} |")
    out.append("")

    report = "\n".join(out) + "\n"
    with open(os.path.join(campaign, "SUMMARY.md"), "w") as fh:
        fh.write(report)

    aggregate = {
        "populations": populations,
        "whole_population": whole,
        "common_success_all": {"overall": compare(c_all, True),
                               "by_format": {f: compare([r for r in c_all if r["fmt"] == f], True)
                                             for f in FORMATS}},
        "common_success_fs_a1": {"overall": compare(cf_a1, False),
                                 "by_format": {f: compare([r for r in cf_a1 if r["fmt"] == f], False)
                                               for f in FORMATS}},
        "verdict": verdict,
        "failures": failures,
    }
    for subset in (aggregate["common_success_all"], aggregate["common_success_fs_a1"]):
        subset["by_format"] = {k: v for k, v in subset["by_format"].items() if v["documents"]}
    with open(os.path.join(raw, "aggregate.json"), "w") as fh:
        json.dump(aggregate, fh, indent=2, sort_keys=True)
        fh.write("\n")

    print(report)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
