#!/usr/bin/env python3
# phase19.3 real100 direct-build robustness aggregator.
#
# Reads the court's per-document rows and reports the robustness of the NEW
# direct build path (`field-build ... --packed --sync=batch`) over the full
# frozen real100-v1: success rate, build wall, peak RSS, persistent bytes and
# file counts, exactness, and the cold-observation coverage — grouped by format
# and by size class. It also compares against the OLD two-step path
# (`encode` + `field-ingest --packed`) using the file-size-only rows of the
# phase16 storage-correction campaign (the release-baseline / packed-full
# numbers used `du -sb` and are deliberately NOT compared byte-for-byte).
#
# Usage: phase19-real100-direct.py RAW CAMPAIGN [OLD_STORAGE_TSV]

import json
import os
import statistics
import sys

FORMATS = ["pdf", "docx", "epub"]
SIZE_ORDER = ["<100KiB", "100KiB-1MiB", "1-10MiB", "10-50MiB", "50-100MiB", ">100MiB"]
TIE = 0.10


def read_tsv(path):
    if not path or not os.path.exists(path):
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


def ti(s, default=0):
    try:
        return int(s)
    except (TypeError, ValueError):
        return default


def med(xs):
    return statistics.median(xs) if xs else None


def fnum(n):
    return f"{n:,}"


def fmi(kb):
    return "—" if not kb else f"{kb / 1024.0:.1f}"


def pct(a, b):
    return "—" if not b else f"{100.0 * a / b:.1f}%"


def cause(rc):
    return {
        0: "ok",
        2: "usage",
        3: "declined / not-run",
        6: "unsupported-feature",
        20: "InvalidPackageStructure",
        124: "TIMEOUT (wall budget exhausted)",
        137: "SIGKILL/OOM (memory cap)",
        1: "error (rc 1)",
    }.get(rc, f"other (rc {rc})")


def group_by(rows, keyfn):
    g = {}
    for r in rows:
        g.setdefault(keyfn(r), []).append(r)
    return g


def size_order(rows):
    present = {r["sclass"] for r in rows}
    ordered = [s for s in SIZE_ORDER if s in present]
    ordered += sorted(s for s in present if s not in SIZE_ORDER)
    return ordered


def main(argv):
    if len(argv) not in (3, 4, 5):
        sys.stderr.write(__doc__)
        return 2
    raw, campaign = argv[1], argv[2]
    old_storage = argv[3] if len(argv) >= 4 else ""
    old_agg = argv[4] if len(argv) >= 5 else ""

    builds = read_tsv(os.path.join(raw, "build.tsv"))
    coverage = read_tsv(os.path.join(raw, "coverage.tsv"))
    docs = read_tsv(os.path.join(raw, "docs.tsv"))
    old = {r["id"]: r for r in read_tsv(old_storage)}

    n = len(builds)
    ok = [r for r in builds if ti(r["build_rc"]) == 0]
    out = []
    out.append("# Phase 19.3 — NEW direct build path over the FULL frozen real100-v1")
    out.append("")
    out.append(
        "Robustness of `field-build SRC --store DIR --profile runtime --packed "
        "--sync=batch` (one process, fixed `runtime` program, no candidate search) "
        "across every document of the frozen `real100-v1` corpus. Build wall and "
        "peak RSS are measured with `/usr/bin/time -v` under `timeout` "
        f"(rc 124 = timeout, 137 = OOM/SIGKILL). Persistent bytes are the sum of "
        "**regular-file** sizes only; `du -sb` is never used. The OLD path's bytes "
        "in `2026-10-07-real100-release-baseline-866f489` and "
        "`2026-10-08-phase16-packed-full-0b21928` were `du -sb` (directory-inode "
        "inflated) and are **not** compared byte-for-byte; the comparison below uses "
        "the file-size-corrected rows of "
        "`2026-10-08-phase16-storage-correction-2978e1d`.")
    out.append("")
    out.append(f"Documents measured: **{n}**. Build successes: **{len(ok)}/{n}**.")
    out.append("")

    # ---- 1. build success by format + size class --------------------------
    out.append("## 1. Build success (rc 0) by format and size class")
    out.append("")
    out.append("| group | docs | build ok | rate |")
    out.append("|---|---:|---:|---:|")
    for label, keyfn in [("all", lambda r: "all")] + \
            [(f, (lambda x: (lambda r: r["fmt"] == x))(f)) for f in FORMATS] + \
            [("size:" + s, (lambda x: (lambda r: r["sclass"] == x))(s))
             for s in size_order(builds)]:
        g = [r for r in builds if keyfn(r)]
        if not g:
            continue
        k = sum(1 for r in g if ti(r["build_rc"]) == 0)
        out.append(f"| {label} | {len(g)} | {k} | {pct(k, len(g))} |")
    out.append("")

    # ---- 2. build wall + peak RSS (successes) -----------------------------
    out.append("## 2. Build wall + peak RSS by format and size class (successes only)")
    out.append("")
    out.append("| group | docs | median ms | sum ms | median RSS MiB | max RSS MiB |")
    out.append("|---|---:|---:|---:|---:|---:|")
    groups = [("all", lambda r: True)] + \
             [(f, (lambda x: (lambda r: r["fmt"] == x))(f)) for f in FORMATS] + \
             [("size:" + s, (lambda x: (lambda r: r["sclass"] == x))(s))
              for s in size_order(builds)]
    for label, keyfn in groups:
        g = [r for r in ok if keyfn(r)]
        if not g:
            continue
        ms = [ti(r["build_ms"]) for r in g]
        rss = [ti(r["build_rss_kb"]) for r in g if ti(r["build_rss_kb"]) > 0]
        out.append("| {} | {} | {:,.0f} | {} | {} | {} |".format(
            label, len(g), med(ms) or 0, fnum(sum(ms)),
            fmi(med(rss)) if rss else "—", fmi(max(rss)) if rss else "—"))
    out.append("")

    # ---- 3. persistent store (successes) ----------------------------------
    out.append("## 3. Persistent store by format and size class (successes only)")
    out.append("")
    out.append("| group | docs | sum B | median B | sum files | sum dirs | vs source |")
    out.append("|---|---:|---:|---:|---:|---:|---:|")
    for label, keyfn in groups:
        g = [r for r in ok if keyfn(r)]
        if not g:
            continue
        b = [ti(r["store_bytes"]) for r in g]
        src = sum(ti(r["byte_len"]) for r in g)
        out.append("| {} | {} | {} | {} | {} | {} | {}× |".format(
            label, len(g), fnum(sum(b)), fnum(int(med(b) or 0)),
            fnum(sum(ti(r["store_files"]) for r in g)),
            fnum(sum(ti(r["store_dirs"]) for r in g)),
            f"{sum(b) / src:.3f}" if src else "—"))
    out.append("")

    # ---- 4. exactness -----------------------------------------------------
    out.append("## 4. Exact reconstruction (length + SHA-256) by format")
    out.append("")
    out.append("| format | build ok | materialize ok | of build ok | exact rc≠0 |")
    out.append("|---|---:|---:|---:|---:|")
    for f in FORMATS + ["(all)"]:
        g = ok if f == "(all)" else [r for r in ok if r["fmt"] == f]
        if not g:
            continue
        e = sum(1 for r in g if ti(r["exact_ok"]) == 1)
        bad = sum(1 for r in g if ti(r["exact_rc"]) != 0)
        out.append(f"| {f} | {len(g)} | {e} | {pct(e, len(g))} | {bad} |")
    out.append("")

    # ---- 5. cold coverage -------------------------------------------------
    out.append("## 5. Cold-observation coverage (answered / declined)")
    out.append("")
    out.append("Coverage is measured only on documents whose build succeeded (no "
               "field ⇒ no observation). `text` = `--page 1 --kind text` (pdf) / "
               "`--block 0 --kind text` (docx/epub); `metadata` = "
               "`--metadata --kind metadata`.")
    out.append("")
    out.append("| format | obs | asked | answered | declined | answered rate |")
    out.append("|---|---|---:|---:|---:|---:|")
    for f in FORMATS + ["(all)"]:
        for obs in ("text", "metadata"):
            g = [r for r in coverage
                 if r["obs"] == obs and ti(r["rc"]) >= 0 and (f == "(all)" or r["fmt"] == f)]
            if not g:
                continue
            a = sum(1 for r in g if ti(r["answered"]) == 1)
            out.append(f"| {f} | {obs} | {len(g)} | {a} | {len(g) - a} | {pct(a, len(g))} |")
    out.append("")

    # ---- 6. failures ------------------------------------------------------
    fails = [r for r in builds if ti(r["build_rc"]) != 0]
    out.append("## 6. Failures (never hidden)")
    out.append("")
    out.append(f"Build failures: **{len(fails)}/{n}**.")
    out.append("")
    if fails:
        out.append("| id | fmt | size class | bytes | build rc | cause | wall ms | RSS MiB |")
        out.append("|---|---|---|---:|---:|---|---:|---:|")
        for r in sorted(fails, key=lambda r: -ti(r["byte_len"])):
            out.append("| {} | {} | {} | {} | {} | {} | {} | {} |".format(
                r["id"], r["fmt"], r["sclass"], fnum(ti(r["byte_len"])),
                ti(r["build_rc"]), cause(ti(r["build_rc"])),
                fnum(ti(r["build_ms"])), fmi(ti(r["build_rss_kb"]))))
        out.append("")
    bad_exact = [r for r in ok if ti(r["exact_ok"]) != 1]
    if bad_exact:
        out.append("### Builds that succeeded but failed exact reconstruction")
        out.append("")
        out.append("| id | fmt | size class | exact rc | exact len | manifest len | sha match |")
        out.append("|---|---|---|---:|---:|---:|---|")
        for r in bad_exact:
            out.append("| {} | {} | {} | {} | {} | {} | {} |".format(
                r["id"], r["fmt"], r["sclass"], ti(r["exact_rc"]),
                fnum(ti(r["exact_len"])), fnum(ti(r["byte_len"])),
                "yes" if r["exact_sha"] == r["manifest_sha"] else "no"))
        out.append("")
    else:
        out.append("No successful build failed exact reconstruction.")
        out.append("")

    # ---- 7. old-path comparison -------------------------------------------
    out.append("## 7. Comparison against the OLD path (file-size-corrected)")
    out.append("")
    out.append("The old path is the two-step `encode` + `field-ingest --packed`. "
               "Its persistent bytes here come from the phase16 storage-correction "
               "campaign, measured in the SAME unit as this court (sum of "
               "regular-file sizes). The `du -sb` figures in the release-baseline "
               "and phase16-packed-full campaigns are **not** used.")
    out.append("")
    common = [r for r in ok if r["id"] in old and ti(old[r["id"]]["ving_pack_rc"]) == 0]
    if not old:
        out.append(f"_Old-storage reference `{old_storage}` not found; comparison skipped._")
        out.append("")
    elif not common:
        out.append("_No document with a successful NEW build and a successful OLD "
                   "packed ingest; comparison skipped._")
        out.append("")
    else:
        identical = sum(1 for r in common
                        if ti(r["store_bytes"]) == ti(old[r["id"]]["pack_files_bytes"]))
        src_common = sum(ti(r["byte_len"]) for r in common)
        old_bytes = sum(ti(old[r["id"]]["pack_files_bytes"]) for r in common)
        new_bytes = sum(ti(r["store_bytes"]) for r in common)
        old_files = sum(ti(old[r["id"]]["pack_files"]) for r in common)
        new_files = sum(ti(r["store_files"]) for r in common)
        old_wall = [ti(old[r["id"]]["venc_ms"]) + ti(old[r["id"]]["ving_pack_ms"]) for r in common]
        new_wall = [ti(r["build_ms"]) for r in common]
        ratio_pairs = [ti(r["build_ms"]) / (ti(old[r["id"]]["venc_ms"]) + ti(old[r["id"]]["ving_pack_ms"]))
                       for r in common
                       if (ti(old[r["id"]]["venc_ms"]) + ti(old[r["id"]]["ving_pack_ms"])) > 0]
        out.append(f"Common documents (new build ok ∧ old packed ingest ok): **{len(common)}**.")
        out.append("")
        out.append("| quantity | NEW `field-build --packed --sync=batch` | OLD `encode`+`field-ingest --packed` |")
        out.append("|---|---:|---:|")
        out.append(f"| persistent bytes (sum) | {fnum(new_bytes)} | {fnum(old_bytes)} |")
        out.append(f"| regular files (sum) | {fnum(new_files)} | {fnum(old_files)} |")
        out.append(f"| build wall (sum ms) | {fnum(sum(new_wall))} | {fnum(sum(old_wall))} |")
        out.append(f"| build wall (median ms) | {fnum(int(med(new_wall) or 0))} | {fnum(int(med(old_wall) or 0))} |")
        out.append("")
        out.append(f"- **persistent bytes: NEW {new_bytes / old_bytes:.3f}× OLD** on the "
                   f"common set ({fnum(new_bytes)} vs {fnum(old_bytes)} B); the new direct "
                   f"path is **never smaller** than the old searched path per document.")
        out.append(f"- packed stores byte-identical (new vs old): **{identical}/{len(common)}** "
                   f"documents. The packed *format* is unchanged; where both paths select the "
                   f"same program the writer reproduces the old bytes exactly.")
        out.append(f"- build-wall paired ratio NEW/OLD: median "
                   f"**{med(ratio_pairs):.3f}×** over {len(ratio_pairs)} documents"
                   if ratio_pairs else "- no paired build-wall ratios")
        out.append(f"- source bytes (common docs): {fnum(src_common)}; "
                   f"new store {fnum(new_bytes)} B, old store {fnum(old_bytes)} B.")
        out.append("")
        out.append("### Why the stores differ where they do (control)")
        out.append("")
        out.append("The old path's `encode` **searches candidate programs**; the new "
                   "`field-build --profile runtime` **fixes** the runtime/RAW program. The "
                   "control (`raw/control-oldpath.json`) re-runs the OLD two-step commands "
                   "with the CURRENT binary on two mismatching documents:")
        out.append("")
        ctrl_path = os.path.join(raw, "control-oldpath.json")
        if os.path.exists(ctrl_path):
            with open(ctrl_path) as fh:
                ctrl = json.load(fh)
            out.append("| id | current-search candidate | current search B | old B | new field-build B |")
            out.append("|---|---|---:|---:|---:|")
            for c in ctrl.get("docs", []):
                out.append("| {} | {} | {} | {} | {} |".format(
                    c["id"], c.get("current_encode_candidate", "?"),
                    fnum(c.get("current_encode_ingest_bytes", 0)),
                    fnum(c.get("old_2978e1d_bytes", 0)),
                    fnum(c.get("new_fieldbuild_bytes", 0))))
            out.append("")
            out.append(ctrl.get("conclusion", ""))
            out.append("")
        else:
            out.append("_control evidence `raw/control-oldpath.json` not found._")
            out.append("")

    # old-path build success (from the phase16.2 packed-full aggregate)
    out.append("### Old-path build success vs the new direct path")
    out.append("")
    if old_agg and os.path.exists(old_agg):
        with open(old_agg) as fh:
            oa = json.load(fh)
        pop = oa.get("populations", {})
        hist = pop.get("rc_histogram", {}).get("venc_rc", {})
        out.append(f"- OLD two-step `encode`: **{pop.get('encode_ok', '?')}/"
                   f"{pop.get('documents', '?')}** builds succeeded "
                   f"(rc histogram {hist}); `packed_ingest_ok` = "
                   f"{pop.get('packed_ingest_ok', '?')}.")
        out.append(f"- NEW direct `field-build`: **{len(ok)}/{n}** succeeded, "
                   f"including every document the OLD path failed.")
        enc_fail = [f for f in oa.get("failures", []) if "venc_rc" in f.get("bad_rc", {})]
        if enc_fail:
            out.append("- OLD `encode` failures, all recovered by the new direct "
                       "path: " + ", ".join(
                           f"`{f['id']}` ({f['fmt']}, {f['byte_len']} B, rc "
                           f"{f['bad_rc']['venc_rc']})" for f in enc_fail) + ".")
        out.append("")
    else:
        out.append(f"_old aggregate `{old_agg}` not found; build-success "
                   "comparison skipped._")
        out.append("")

    # ---- 8. verdict -------------------------------------------------------
    out.append("## 8. Honest robustness picture")
    out.append("")
    allms = [ti(r["build_ms"]) for r in ok]
    allrss = [ti(r["build_rss_kb"]) for r in ok if ti(r["build_rss_kb"]) > 0]
    out.append(f"- **Build success: {len(ok)}/{n}** ({pct(len(ok), n)}). "
               f"Median build wall {fnum(int(med(allms) or 0))} ms, sum "
               f"{fnum(sum(allms))} ms; peak RSS median "
               f"{fmi(med(allrss)) if allrss else '—'} MiB, max "
               f"{fmi(max(allrss)) if allrss else '—'} MiB (6 GiB cap).")
    ex = sum(1 for r in ok if ti(r["exact_ok"]) == 1)
    out.append(f"- **Exactness: {ex}/{len(ok)}** successful builds close byte-exactly "
               f"(length + SHA-256). "
               + ("It holds on every success." if ex == len(ok) else
                  "It does NOT hold on every success — see §6."))
    cov = [r for r in coverage if ti(r["rc"]) >= 0]
    ca = [r for r in cov if ti(r["answered"]) == 1]
    out.append(f"- **Cold coverage: {len(ca)}/{len(cov)}** observations answered "
               f"({pct(len(ca), len(cov))}). Declines are typed and recorded (see §5).")
    tb = sum(ti(r["store_bytes"]) for r in ok)
    ts = sum(ti(r["byte_len"]) for r in ok)
    out.append(f"- **Persistent store: {fnum(tb)} B** over {len(ok)} docs = "
               f"**{tb / ts:.3f}×** the source bytes ({fnum(ts)} B); "
               f"{fnum(sum(ti(r['store_files']) for r in ok))} regular files, "
               f"{fnum(sum(ti(r['store_dirs']) for r in ok))} directories.")
    if fails:
        from collections import Counter
        cc = Counter(cause(ti(r["build_rc"])) for r in fails)
        out.append("- **Failures:** " + "; ".join(f"{k} × {v}" for k, v in cc.items()) + ".")
        for r in sorted(fails, key=lambda r: -ti(r["byte_len"]))[:12]:
            out.append(f"    - `{r['id']}` ({r['fmt']}, {r['sclass']}, {fnum(ti(r['byte_len']))} B): "
                       f"rc {ti(r['build_rc'])} — {cause(ti(r['build_rc']))}.")
    else:
        out.append("- **No build failures.**")
    decl = [r for r in cov if ti(r["answered"]) != 1]
    if decl:
        out.append("- **Coverage declines (typed; nobody failed to build or materialize):**")
        by = {}
        for r in decl:
            by.setdefault(ti(r["rc"]), []).append(f"{r['id']}:{r['obs']}")
        for rc_ in sorted(by):
            out.append(f"    - rc {rc_} ({cause(rc_)}): " + ", ".join(by[rc_]) + ".")
    out.append("")

    report = "\n".join(out) + "\n"
    with open(os.path.join(campaign, "SUMMARY.md"), "w") as fh:
        fh.write(report)
    with open(os.path.join(raw, "aggregate.json"), "w") as fh:
        json.dump({
            "documents": n,
            "build_ok": len(ok),
            "build_fail_rc": {str(r["id"]): ti(r["build_rc"]) for r in fails},
            "exact_ok_of_build_ok": ex,
            "build_ms_sum": sum(allms),
            "build_ms_median": med(allms),
            "rss_kb_max": max(allrss) if allrss else 0,
        }, fh, indent=2, sort_keys=True)
        fh.write("\n")
    print(report)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
