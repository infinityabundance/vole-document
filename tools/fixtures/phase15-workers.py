#!/usr/bin/env python3
# phase15-workers aggregator (Phase 15.4).
#
# Reads the worker-count sweep rows and reports the speedup curve of
# `field-ingest --workers N` (N in 1,2,4,8,16) relative to serial (N=1), plus the
# determinism witness: the field id is identical across every worker count and
# every count reproduces the source byte-for-byte under `materialize --exact`.
#
# Usage: python3 tools/fixtures/phase15-workers.py RAW_DIR CAMPAIGN_DIR

import os
import statistics
import sys

LANE_CPUS = 8  # the `doc-baseline` lane cap (compose.yaml)


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


def main(argv):
    if len(argv) != 3:
        sys.stderr.write(__doc__)
        return 2
    raw, campaign = argv[1], argv[2]
    workers = read_tsv(os.path.join(raw, "workers.tsv"))
    det = read_tsv(os.path.join(raw, "determinism.tsv"))

    # Wall per (id, workers) for successful, timed runs.
    wall = {}
    for r in workers:
        n = to_int(r["workers"])
        if to_int(r["rc"]) == 0 and to_int(r["wall_ms"]) > 0:
            wall[(r["id"], n)] = to_int(r["wall_ms"])
    counts = sorted({to_int(r["workers"]) for r in workers})
    ids = sorted({r["id"] for r in workers})

    out = []
    out.append("# phase15-workers — worker-count sweep + determinism witness")
    out.append("")
    out.append(
        f"Documents: **{len(ids)}** (SUBSET of `real100-v1` — includes a large PDF "
        f"and package formats; **not** the frozen 100-document population). "
        f"`field-ingest` was run with `--workers ` in {{{', '.join(str(c) for c in counts)}}} "
        f"after a single `encode` per document. "
    )
    out.append(
        f"**Lane caveat:** the `doc-baseline` lane is capped at `cpus: {LANE_CPUS}`; "
        f"`--workers {LANE_CPUS}` saturates it and any count above that "
        f"(here 16) OVERSUBSCRIBES it, so those wall times are informational, "
        f"not a clean scaling point."
    )
    out.append("")

    # ---- speedup curve ---------------------------------------------------
    out.append("## Speedup curve (vs serial `--workers 1`)")
    out.append("")
    out.append("| workers | timed docs | median wall ms | median speedup vs 1 | note |")
    out.append("|---:|---:|---:|---:|---|")
    for n in counts:
        xs = [wall[(i, n)] for i in ids if (i, n) in wall]
        med = median(xs)
        per_doc = []
        for i in ids:
            if (i, 1) in wall and (i, n) in wall and wall[(i, n)] > 0:
                per_doc.append(wall[(i, 1)] / wall[(i, n)])
        sp = median(per_doc)
        note = ""
        if n > LANE_CPUS:
            note = f"oversubscribed (> {LANE_CPUS} cpus)"
        elif n == LANE_CPUS:
            note = "saturates the lane"
        out.append("| {} | {} | {} | {} | {} |".format(
            n, len(xs),
            f"{med:.0f}" if med is not None else "—",
            f"{sp:.2f}x" if sp is not None else "—",
            note))
    out.append("")
    out.append("Median speedup is over documents with a successful timed run at both "
               "worker counts; `—` means no comparable pair. A value below 1.0 means "
               "the parallel run was SLOWER (recorded, not hidden).")
    out.append("")

    # ---- per document ----------------------------------------------------
    out.append("### Per document (wall ms by worker count)")
    out.append("")
    hdr = "| id | " + " | ".join(f"w{n}" for n in counts) + " |"
    sep = "|---|" + "---:|" * len(counts)
    out.append(hdr)
    out.append(sep)
    for i in ids:
        cells = []
        for n in counts:
            cells.append(str(wall[(i, n)]) if (i, n) in wall else "—")
        out.append("| {} | {} |".format(i, " | ".join(cells)))
    out.append("")

    # ---- determinism -----------------------------------------------------
    feq = sum(1 for r in det if to_int(r["field_equal"]) == 1)
    aex = sum(1 for r in det if to_int(r["all_exact"]) == 1)
    out.append("## Determinism witness")
    out.append("")
    out.append("| check | pass | of |")
    out.append("|---|---:|---:|")
    out.append(f"| field id identical across every worker count | {feq} | {len(det)} |")
    out.append(f"| `materialize --exact` == source for EVERY worker count | {aex} | {len(det)} |")
    out.append("")
    out.append("| id | distinct field ids | field equal | w1 | w2 | w4 | w8 | w16 | all exact |")
    out.append("|---|---:|---|---|---|---|---|---|---|")
    for r in det:
        out.append("| {} | {} | {} | {} | {} | {} | {} | {} | {} |".format(
            r["id"], r["fields_seen"],
            "yes" if to_int(r["field_equal"]) == 1 else "**no**",
            r.get("exact_1", "?"), r.get("exact_2", "?"), r.get("exact_4", "?"),
            r.get("exact_8", "?"), r.get("exact_16", "?"),
            "yes" if to_int(r["all_exact"]) == 1 else "**no**"))
    out.append("")
    out.append("A `1` in an `exact_*` column means that worker count's store "
               "materialized byte-identically to the manifest `sha256`/`byte_len`.")
    out.append("")

    report = "\n".join(out) + "\n"
    with open(os.path.join(campaign, "SUMMARY.md"), "w") as fh:
        fh.write(report)
    print(report)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
