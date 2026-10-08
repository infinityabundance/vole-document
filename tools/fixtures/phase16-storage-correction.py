#!/usr/bin/env python3
# phase16 storage-correction aggregator.
#
# Re-computes the Phase-15.3 (subset packed/fs) and Phase-16.2 (full-population
# common-success fs/packed/SQLite) storage ratios from the correction court's
# per-document rows, under TWO accountings:
#
#   files = sum of regular-file st_size over the store tree (directories = 0)
#   du    = `du -sb` of the store tree (GNU --apparent-size; counts dir inodes)
#
# and quantifies the directory-inode distortion between them.  It reads the
# original id lists (15.3 subset, 16.2 common-success) so the populations are
# unchanged; only the unit of accounting changes.
#
# Usage: phase16-storage-correction.py RAW CAMPAIGN [SUBSET_IDS [COMMON_IDS [LABEL]]]

import json
import os
import statistics
import sys

FORMATS = ["pdf", "docx", "epub"]


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


def read_ids(path):
    if not path or not os.path.exists(path):
        return None
    with open(path) as fh:
        return [ln.strip() for ln in fh if ln.strip()]


def to_int(s):
    try:
        return int(s)
    except (TypeError, ValueError):
        return None


def rc(r, col):
    v = to_int(r.get(col, ""))
    return -1 if v is None else v


def ok(r, col):
    return rc(r, col) == 0


def B(r, col):
    v = to_int(r.get(col, ""))
    return v if v is not None else 0


def ratio(num, den):
    return None if not den else num / den


def fr(r):
    return "—" if r is None else f"{r:.3f}×"


def fi(n):
    return f"{n:,}"


def median(xs):
    return statistics.median(xs) if xs else None


def compare(rows, prefix):
    """Sum + median ratios for one population under one accounting prefix."""
    if prefix == "files":
        fs = sum(B(r, "fs_files_bytes") for r in rows)
        pk = sum(B(r, "pack_files_bytes") for r in rows)
        a1 = sum(B(r, "a1_files_bytes") for r in rows)
    else:
        fs = sum(B(r, "fs_du_bytes") for r in rows)
        pk = sum(B(r, "pack_du_bytes") for r in rows)
        a1 = sum(B(r, "a1_du_bytes") for r in rows)
    per_fs_a1 = [B(r, "fs_files_bytes") / B(r, "a1_files_bytes") if prefix == "files"
                 else B(r, "fs_du_bytes") / B(r, "a1_du_bytes")
                 for r in rows
                 if (B(r, "a1_files_bytes") if prefix == "files" else B(r, "a1_du_bytes")) > 0]
    per_pk_a1 = [(B(r, "pack_files_bytes") if prefix == "files" else B(r, "pack_du_bytes")) /
                 (B(r, "a1_files_bytes") if prefix == "files" else B(r, "a1_du_bytes"))
                 for r in rows
                 if (B(r, "a1_files_bytes") if prefix == "files" else B(r, "a1_du_bytes")) > 0
                 and (B(r, "pack_files_bytes") if prefix == "files" else B(r, "pack_du_bytes")) > 0]
    per_pk_fs = [(B(r, "pack_files_bytes") if prefix == "files" else B(r, "pack_du_bytes")) /
                 (B(r, "fs_files_bytes") if prefix == "files" else B(r, "fs_du_bytes"))
                 for r in rows
                 if (B(r, "fs_files_bytes") if prefix == "files" else B(r, "fs_du_bytes")) > 0
                 and (B(r, "pack_files_bytes") if prefix == "files" else B(r, "pack_du_bytes")) > 0]
    return {
        "documents": len(rows),
        "fs_bytes": fs, "pack_bytes": pk, "a1_bytes": a1,
        "fs_over_a1": ratio(fs, a1), "pack_over_a1": ratio(pk, a1), "pack_over_fs": ratio(pk, fs),
        "median_fs_over_a1": median(per_fs_a1),
        "median_pack_over_a1": median(per_pk_a1),
        "median_pack_over_fs": median(per_pk_fs),
    }


def overhead(rows, key):
    """du - file-size bytes and directory counts for one store column prefix."""
    sfiles = sum(B(r, f"{key}_files_bytes") for r in rows)
    sdu = sum(B(r, f"{key}_du_bytes") for r in rows)
    nd = sum(B(r, f"{key}_dirs") for r in rows)
    return {"file_bytes": sfiles, "du_bytes": sdu, "dirs": nd,
            "overhead_bytes": sdu - sfiles,
            "overhead_frac_of_files": (sdu - sfiles) / sfiles if sfiles else None,
            "bytes_per_dir": (sdu - sfiles) / nd if nd else None}


def table(title, rows, prefix):
    c = compare(rows, prefix)
    out = [f"#### {title} — {prefix} accounting", "",
           "| comparison | numerator B | denominator B | sum ratio | median per-doc ratio |",
           "|---|---:|---:|---:|---:|"]
    out.append(f"| VOLE fs vs SQLite | {fi(c['fs_bytes'])} | {fi(c['a1_bytes'])} | {fr(c['fs_over_a1'])} | {fr(c['median_fs_over_a1'])} |")
    out.append(f"| VOLE packed vs SQLite | {fi(c['pack_bytes'])} | {fi(c['a1_bytes'])} | {fr(c['pack_over_a1'])} | {fr(c['median_pack_over_a1'])} |")
    out.append(f"| VOLE packed vs VOLE fs | {fi(c['pack_bytes'])} | {fi(c['fs_bytes'])} | {fr(c['pack_over_fs'])} | {fr(c['median_pack_over_fs'])} |")
    out.append("")
    return "\n".join(out), c


def main(argv):
    if len(argv) < 3:
        sys.stderr.write(__doc__)
        return 2
    raw, campaign = argv[1], argv[2]
    subset_ids = read_ids(argv[3]) if len(argv) > 3 else None
    common_ids = read_ids(argv[4]) if len(argv) > 4 else None
    common_label = argv[5] if len(argv) > 5 else "common success"

    rows = read_tsv(os.path.join(raw, "storage.tsv"))
    by_id = {r["id"]: r for r in rows}

    # ---- populations, unchanged from the originals ------------------------
    def sub(ids, require_a1=False):
        out = []
        for i in ids or []:
            r = by_id.get(i)
            if r is None:
                continue
            if not (ok(r, "venc_rc") and ok(r, "ving_fs_rc") and ok(r, "ving_pack_rc")):
                continue
            if require_a1 and not ok(r, "a1_build_rc"):
                continue
            out.append(r)
        return out

    # ---- directory-inode distortion --------------------------------------
    succ = [r for r in rows if ok(r, "ving_fs_rc") and ok(r, "ving_pack_rc")]
    ov_fs = overhead(succ, "fs")
    ov_pk = overhead(succ, "pack")
    ov_a1 = overhead([r for r in rows if ok(r, "a1_build_rc")], "a1")

    # example small + large documents (by source byte_len)
    ordered = sorted(succ, key=lambda r: B(r, "byte_len"))
    examples = []
    if ordered:
        for r in (ordered[0], ordered[-1]):
            examples.append({
                "id": r["id"], "fmt": r["fmt"], "src_bytes": B(r, "byte_len"),
                "fs_du": B(r, "fs_du_bytes"), "fs_files": B(r, "fs_files_bytes"),
                "fs_dirs": B(r, "fs_dirs"),
                "fs_overhead": B(r, "fs_du_bytes") - B(r, "fs_files_bytes"),
                "pack_du": B(r, "pack_du_bytes"), "pack_files": B(r, "pack_files_bytes"),
                "pack_dirs": B(r, "pack_dirs"),
                "pack_overhead": B(r, "pack_du_bytes") - B(r, "pack_files_bytes"),
            })

    # ---- (a) 15.3 subset --------------------------------------------------
    a_rows = sub(subset_ids)
    a_files = compare(a_rows, "files")
    a_du = compare(a_rows, "du")
    # ---- (b) 16.2 common success -----------------------------------------
    b_rows = sub(common_ids, require_a1=True)
    b_files_all = compare(b_rows, "files")
    b_du_all = compare(b_rows, "du")
    b_files_fmt = {f: compare([r for r in b_rows if r["fmt"] == f], "files") for f in FORMATS}
    b_du_fmt = {f: compare([r for r in b_rows if r["fmt"] == f], "du") for f in FORMATS}

    # ---- SUMMARY.md -------------------------------------------------------
    out = []
    out.append("# Phase-16 storage-correction — file-size-only vs `du -sb` accounting")
    out.append("")
    out.append(
        "Re-measures the Phase-15.3 and Phase-16.2 persistent-storage courts with "
        "**file-size-only** bytes (`find DIR -type f -printf '%s\\n'`, directories "
        "contribute 0) instead of `du -sb`, on unchanged populations. `du -sb` is "
        "`--apparent-size`, so it adds each directory inode's own `st_size` "
        "(4096 B on this ext4 bind mount): inflating the one-file-per-node `fs` "
        "store (tens of thousands of dirs) far more than the packed store or the "
        "single SQLite `.db`."
    )
    out.append("")
    out.append(f"Documents measured in this run: **{len(rows)}** "
               f"(union of the 15.3 subset and the 16.2 common-success set). "
               f"Git `{os.environ.get('CORRECTION_COMMIT','?')}`. Profile release.")
    out.append("")

    out.append("## 1. Directory-inode distortion (measured)")
    out.append("")
    out.append("Over the successful store directories in this run:")
    out.append("")
    out.append("| substrate | file bytes | `du -sb` bytes | directory overhead (B) | overhead / file bytes | dirs | B per dir |")
    out.append("|---|---:|---:|---:|---:|---:|---:|")
    for name, ov in (("VOLE fs store", ov_fs), ("VOLE packed store", ov_pk), ("A1 SQLite `.db`", ov_a1)):
        of = "—" if ov["overhead_frac_of_files"] is None else f"{ov['overhead_frac_of_files']*100:.1f}%"
        bd = "—" if ov["bytes_per_dir"] is None else f"{ov['bytes_per_dir']:.0f}"
        out.append(f"| {name} | {fi(ov['file_bytes'])} | {fi(ov['du_bytes'])} | {fi(ov['overhead_bytes'])} | {of} | {fi(ov['dirs'])} | {bd} |")
    out.append("")
    out.append("Worked example (smallest and largest successful document):")
    out.append("")
    out.append("| id | fmt | src B | fs file B | fs du B | fs dirs | fs overhead B | pack file B | pack du B | pack dirs | pack overhead B |")
    out.append("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
    for e in examples:
        out.append(f"| {e['id']} | {e['fmt']} | {fi(e['src_bytes'])} | {fi(e['fs_files'])} | {fi(e['fs_du'])} | "
                   f"{fi(e['fs_dirs'])} | {fi(e['fs_overhead'])} | {fi(e['pack_files'])} | {fi(e['pack_du'])} | "
                   f"{fi(e['pack_dirs'])} | {fi(e['pack_overhead'])} |")
    out.append("")

    out.append("## 2. Corrected Phase-15.3 subset (packed / fs)")
    out.append("")
    out.append(f"Population: **{a_files['documents']}** documents (the 15.3 12-document subset).")
    out.append("")
    out.append("| accounting | sum fs B | sum packed B | sum ratio packed/fs | median per-doc |")
    out.append("|---|---:|---:|---:|---:|")
    out.append(f"| old `du -sb` | {fi(a_du['fs_bytes'])} | {fi(a_du['pack_bytes'])} | {fr(a_du['pack_over_fs'])} | {fr(a_du['median_pack_over_fs'])} |")
    out.append(f"| **file-size-only** | {fi(a_files['fs_bytes'])} | {fi(a_files['pack_bytes'])} | **{fr(a_files['pack_over_fs'])}** | {fr(a_files['median_pack_over_fs'])} |")
    out.append("")

    out.append("## 3. Corrected Phase-16.2 common-success (fs vs packed vs SQLite)")
    out.append("")
    out.append(f"Population: **{b_files_all['documents']}** documents ({common_label}).")
    out.append("")
    t, _ = table("Overall", b_rows, "files")
    out.append(t)
    out.append("### By format (file-size-only)")
    out.append("")
    for f in FORMATS:
        if b_files_fmt[f]["documents"]:
            t, _ = table(f"{f} ({b_files_fmt[f]['documents']} docs)", [r for r in b_rows if r["fmt"] == f], "files")
            out.append(t)
    out.append("### Same population under the old `du -sb` accounting (for comparison)")
    out.append("")
    t, _ = table("Overall", b_rows, "du")
    out.append(t)
    out.append("### `du -sb` by format (old unit)")
    out.append("")
    for f in FORMATS:
        if b_du_fmt[f]["documents"]:
            t, _ = table(f"{f} ({b_du_fmt[f]['documents']} docs)", [r for r in b_rows if r["fmt"] == f], "du")
            out.append(t)

    out.append("## 4. Verdict")
    out.append("")
    poa_files = b_files_all["pack_over_a1"]
    poa_du = b_du_all["pack_over_a1"]
    foa_files = b_files_all["fs_over_a1"]
    foa_du = b_du_all["fs_over_a1"]
    if poa_files is not None:
        closes = poa_files <= 1.0
        margin = 1.0 - poa_files
        out.append(
            f"Under **file-size-only** accounting, on the {b_files_all['documents']} "
            f"common-success documents: fs/SQLite = **{fr(foa_files)}** (old du: {fr(foa_du)}), "
            f"packed/SQLite = **{fr(poa_files)}** (old du: {fr(poa_du)}), "
            f"packed/fs = **{fr(b_files_all['pack_over_fs'])}** (old du: {fr(b_du_all['pack_over_fs'])}). "
            f"Packed **{'CLOSES' if closes else 'does NOT close'}** the gap vs SQLite "
            f"(margin {margin:+.3f}× vs 1.0)."
        )
    out.append("")

    report = "\n".join(out) + "\n"
    with open(os.path.join(campaign, "SUMMARY.md"), "w") as fh:
        fh.write(report)
    print(report)

    aggregate = {
        "distortion": {"fs": ov_fs, "packed": ov_pk, "a1": ov_a1, "examples": examples},
        "phase15_3_subset": {"documents": a_files["documents"],
                             "files": a_files, "du": a_du},
        "phase16_2_common": {"label": common_label, "documents": b_files_all["documents"],
                             "files_overall": b_files_all, "du_overall": b_du_all,
                             "files_by_format": b_files_fmt, "du_by_format": b_du_fmt},
    }
    with open(os.path.join(raw, "aggregate.json"), "w") as fh:
        json.dump(aggregate, fh, indent=2, sort_keys=True)
        fh.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
