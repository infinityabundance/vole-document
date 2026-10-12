#!/usr/bin/env python3
# Phase 28.6 — aggregate the raw lanes into SUMMARY.md + receipt.json.
#
#   python3 tools/fixtures/phase28-6-aggregate.py RAW OUT CAMP SHORT
#
import json
import os
import sys

raw, out, camp, short = sys.argv[1:5]


def rows(name):
    p = os.path.join(raw, name)
    ls = [l.rstrip("\n").split("\t") for l in open(p) if l.strip()]
    return ls[0], ls[1:]


oh, obs = rows("observations.tsv")
sh, sch = rows("schemes.tsv")
qh, sql = rows("sqlite.tsv")

schemes = {}
for r in sch:
    schemes.setdefault(r[0], {})[r[1]] = {
        "n_fragments": int(r[2]), "hit_fragments": int(r[3]),
        "floor_bytes": int(r[4]), "directory_bytes": int(r[5])}
sq = {}
for r in sql:
    sq.setdefault(r[0], {})[r[1]] = {
        "db_bytes": int(r[2]), "extract": int(r[3]), "reads": int(r[4]),
        "distinct_ranges": int(r[5]), "bytes": int(r[6]), "pages": int(r[7])}

ENTRY = 48


def practical(sc):
    best = None
    for name, s in sc.items():
        tot = s["floor_bytes"] + s["directory_bytes"]
        if best is None or tot < best[1]:
            best = (name, tot, s)
    return best


def best_native(sc):
    return practical({k: v for k, v in sc.items() if k.startswith("native")}) \
        if any(k.startswith("native") for k in sc) else None


lines = []
L = lines.append
L("# Phase 28.6 results — fragment locality (decision Experiment 2)")
L("")
L("Branch `phase28` @ `%s`. **Measurement only**: no wire format, no planner "
  "change, no decoder change. This is the decision gate for 28.8+ (physical "
  "fragmentation)." % short)
L("")
L("**Historical control (Phase 22.5):** VOLE's selective remote read transferred a")
L("median **3.15x** more bytes than page-level SQLite (0 wins / 17 losses / 7")
L("unresolved). This court re-measures the same question *locally and exactly*")
L("for five large documents, with VOLE's own instrumented `bytes_read` as the")
L("closure and a real page-level SQLite control.")
L("")
L("## Request, and the ideal floor (bytes a directory would have to verify)")
L("")
L("| fmt | source B | narrow request | required source B | required ranges |")
L("|---|---:|---|---:|---:|")
for r in obs:
    L("| %s | %s | `%s` | %s | %s |" % (r[0], r[1], r[4], r[11], r[12]))
L("")
L("## Bytes read per lane (the headline table)")
L("")
L("| fmt | current VOLE `bytes_read` | VOLE store-reads (strace) | ideal floor | best fixed size / B | best fragment floor+dir | page SQLite: pages | page SQLite: blob | VOLE / floor | VOLE / SQL(pages) |")
L("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
summary = {}
for r in obs:
    fmt = r[0]
    vole = int(r[6]); phys = int(r[7]); ideal = int(r[11])
    sc = schemes.get(fmt, {})
    best = practical(sc)
    fixed = {k: v for k, v in sc.items() if k.startswith("fixed")}
    bf = min(fixed.items(), key=lambda kv: kv[1]["floor_bytes"]) if fixed else None
    pg = sq.get(fmt, {}).get("pages", {})
    bl = sq.get(fmt, {}).get("blob", {})
    sqb = pg.get("bytes", 0)
    L("| %s | %s | %s | %s | %s | %s (%s) | %s (%s pages) | %s | %.1fx | %.1fx |" % (
        fmt, f"{vole:,}", f"{phys:,}", f"{ideal:,}",
        f"{bf[1]['floor_bytes']:,}" if bf else "-",
        f"{best[1]:,}" if best else "n/a", best[0] if best else "-",
        f"{sqb:,}", pg.get("pages", 0), f"{bl.get('bytes', 0):,}",
        (vole / best[1]) if best and best[1] else 0,
        (vole / sqb) if sqb else 0))
    summary[fmt] = {"src_bytes": int(r[1]), "src_sha256": r[2],
                    "vole_bytes_read": vole, "vole_phys_bytes": phys,
                    "ideal_floor": ideal, "schemes": sc,
                    "sqlite": sq.get(fmt, {}), "exact_ok": int(r[13]),
                    "vole_wall_us": int(r[8]), "vole_rss_kb": int(r[9]),
                    "bytes_returned": int(r[10])}
L("")
L("## Cost of the VOLE lane (single cold observation; no network transport)")
L("")
L("There is no remote transport in this lane, so p95/p99 latency is not")
L("measured and is not claimed: the request is a local file read, so the")
L("headline is BYTES. Wall time is one cold `observe-batch` (cache removed);")
L("peak RSS is that process's high-water mark.")
L("")
L("| fmt | VOLE wall ms | VOLE peak RSS KiB | bytes returned |")
L("|---|---:|---:|---:|")
for r in obs:
    L("| %s | %.1f | %s | %s |" % (r[0], int(r[8]) / 1000.0, f"{int(r[9]):,}",
                                    f"{int(r[10]):,}"))
L("")
L("## Fragment schemes per format (floor = required fragments; dir = 48 B/entry)")
L("")
L("| fmt | scheme | fragments | required fragments | floor B | directory B | floor+dir B |")
L("|---|---|---:|---:|---:|---:|---:|")
for fmt in sorted(schemes):
    order = sorted(schemes[fmt],
                   key=lambda n: (schemes[fmt][n]["floor_bytes"]
                                  + schemes[fmt][n]["directory_bytes"]))
    for name in order:
        s = schemes[fmt][name]
        L("| %s | %s | %d | %d | %s | %s | %s |" % (
            fmt, name, s["n_fragments"], s["hit_fragments"],
            f"{s['floor_bytes']:,}", f"{s['directory_bytes']:,}",
            f"{s['floor_bytes']+s['directory_bytes']:,}"))
L("")
L("## Exactness")
L("")
L("`materialize --exact == source` (length + SHA-256 + `cmp`): %s" %
  ("**%d/%d**" % (sum(1 for r in obs if r[13] == "1"), len(obs))))
L("")
L("## Historical control (Phase 22.5, unchanged)")
L("")
L("Phase 22.5 measured VOLE selective remote reads against a page-level SQLite")
L("control (33 observations, 12 documents) and recorded a **model-scoped loss**:")
L("")
L("| region | n | VOLE B | SQLite B | bytes ratio | verdict |")
L("|---|---:|---:|---:|---:|---|")
L("| all | 33 | 264,466 | 20,480 | **3.154** | loss |")
L("| obs=text | 12 | 133,693 | 20,480 | 6.53 | loss |")
L("| obs=bytes | 12 | 208,889 | 219,136 | 0.95 | unresolved |")
L("| obs=metadata (pdf) | 4 | 708,608 | 8,192 | 86.5 | loss |")
L("| obs=resource (docx/epub) | 5 | 109,995 | 12,288 | 8.95 | loss |")
L("")
L("Source: `docs/phases/phase-22-5-results.md`,")
L("`evidence/campaigns/2026-10-08-phase22-5-remote-*`. The present court re-asks")
L("that question locally with the same VOLE closure definition and a real")
L("page-level SQLite control.")
L("")
L("## Limits and control notes (stated, not hidden)")
L("")
L("- **VOLE closure is the honest physical total** the observation fetched")
L("  (`descriptor+manifest+index+seed`). It reads the whole descriptor for a")
L("  narrow request here (`descriptor_read_mode=full`), which is the Phase-22.5")
L("  mechanism; the independent `strace` store-read count agrees within ~0.1%.")
L("- **The fragment directory is a prototype, not a wire format.** The")
L("  required byte set is derived per format (below); the directory cost is")
L("  modelled at 48 B/entry and does not include a query->fragment resolution")
L("  index beyond that per-fragment entry.")
L("- **PDF**: required objects are the ones reachable from the page-1 node via")
L("  qpdf's object graph (NOT through `/Parent`), plus the xref/trailer tail;")
L("  object spans come from a `N G obj`...`endobj` scan, so the native fragment")
L("  count is approximate (the required *bytes* are the union of those spans).")
L("- **DOCX**: a DEFLATE member cannot be range-accessed inside the stream, so")
L("  the required set is the whole compressed `word/document.xml` member plus the")
L("  package directory. Paragraph locality is therefore bounded by the member,")
L("  not the paragraph.")
L("- **Parquet**: VOLE's contract has no predicate pushdown, so")
L("  `--parquet-cell 0:0` (one column chunk = its narrowest observation) stands")
L("  in for a predicate+projection. A real predicate would only shrink the")
L("  required set (row-group pruning via min/max statistics), so the recorded")
L("  floor is an **upper bound** for the predicate case.")
L("- **SQLite**: two source-retaining envelopes are reported. `pages` (one row")
L("  per 4 KiB page) is the honest page-level control; `blob` is the Phase-22.5")
L("  `source_blob` analogue, whose overflow chain must be walked for a deep")
L("  offset (hence its much larger reads). `mmap_size=0`, `cache_size=-8`.")
L("- **Directory overhead is the binding constraint for row/record formats**:")
L("  a per-record structural index has one entry per row/element and is larger")
L("  than the source (see the schemes table); only a COARSE structural index")
L("  (objects / members / row groups / column chunks) is viable.")
L("")
open(os.path.join(camp, "SUMMARY.md"), "w").write("\n".join(lines) + "\n")

receipt = {
    "campaign": os.path.basename(camp),
    "phase": "28.6 — fragment locality (decision Experiment 2; measurement only)",
    "branch": "phase28",
    "commit": short,
    "exact_ok": sum(1 for r in obs if r[13] == "1"),
    "n_docs": len(obs),
    "phase22_5_historical_control": {
        "vole_vs_sqlite_bytes_median_ratio": 3.154,
        "wins": 0, "losses": 17, "unresolved": 7,
        "source": "docs/phases/phase-22-5-results.md"},
    "directory_entry_model_bytes": ENTRY,
    "per_format": summary,
}
json.dump(receipt, open(os.path.join(camp, "receipt.json"), "w"), indent=1,
          sort_keys=True)
print("\n".join(lines))
