#!/usr/bin/env python3
# Phase 30.4 — coarse *reading* court fixtures.
#
# Reuses the Phase-29.2 deterministic corpus and unit-range recovery verbatim
# (identical bytes, so the 29.2 numbers remain a like-for-like historical
# control), and adds a Phase-30.4 aggregator whose headline is the natural
# *reading* request.
#
#   python3 tools/fixtures/phase30-4-coarse-reading.py corpus      --out DIR
#   python3 tools/fixtures/phase30-4-coarse-reading.py unit-ranges --idx I --map M --request LINE
#   python3 tools/fixtures/phase30-4-coarse-reading.py aggregate   --raw R --out O --camp C --short SHA

import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import importlib.util


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


P292 = load("phase29_2_coarse", os.path.join(HERE, "phase29-2-coarse.py"))

# Phase 29.2 historical control (SAME corpus bytes; that receipt is the control).
HISTORICAL_29_2 = {
    "pdf":     {"reading_control": 3245913, "reading_closure": 46545,   "verdict": "WIN"},
    "docx":    {"reading_control": 3290209, "reading_closure": 3296460, "verdict": "LOSS"},
    "odt":     {"reading_control": 3289667, "reading_closure": 3295639, "verdict": "LOSS"},
    "parquet": {"reading_control": 4196116, "reading_closure": 8390673, "verdict": "LOSS"},
}


def cmd_corpus(out_dir):
    P292.cmd_corpus(out_dir)


def cmd_unit_ranges(idx, map_path, request):
    P292.cmd_unit_ranges(idx, map_path, request)


def read_tsv(path):
    return P292.read_tsv(path)


def num(d, k):
    return P292.num(d, k)


def fmt_b(n):
    return str(n)


def cmd_aggregate(raw, out, camp, short):
    obs = read_tsv(os.path.join(raw, "observations.tsv"))
    by_fmt = {}
    for r in obs:
        by_fmt.setdefault(r["fmt"], {})[r["shape"]] = r

    lines = []
    lines.append("# Phase 30.4 — unit-addressed coarse *reading* (real bytes read)\n")
    lines.append("Phase 29.2 found the coarse structural index narrowed the natural")
    lines.append("*reading* request (page-1 / block-0 / parquet-cell text) for PDF only:")
    lines.append("the DOCX/ODT/Parquet format-model nodes materialized the whole exact")
    lines.append("`DocumentExact` source, so their units were built but inert. Phase 30.4")
    lines.append("serves the reading request from the coarse member spans (the ZIP member /")
    lines.append("column chunk) plus the ZIP central directory, so its closure is the unit +")
    lines.append("directory, not the whole document. Corpus and reconstruction are unchanged")
    lines.append("from 29.2, so those numbers are the historical control.\n")
    lines.append("`closure` = real `pread` bytes (units + directory + manifest/index/seed);")
    lines.append("`units B` = the coarse-lane physical bytes; `dir B` = persisted index file;")
    lines.append("control = `observe-batch` cold whole-descriptor read. `page-SQLite` is the")
    lines.append("Phase-28.6 page control over the request's required regions.\n")
    lines.append("| fmt | shape | source B | control B | units B | dir B | closure B | page-SQLite B | closure/control | vs control | closure/SQLite | matches | exact |")
    lines.append("|---|---|---:|---:|---:|---:|---:|---:|---:|:--:|---:|:--:|:--:|")

    exact_ok = 0
    reading_wins = 0
    reading_sqlite_wins = 0
    reading_sqlite_measured = 0
    span_wins = 0
    for fmt in ["pdf", "docx", "odt", "parquet"]:
        q = by_fmt.get(fmt)
        if not q:
            continue
        rate = q.get("span") or q.get("reading")
        if num(rate, "exact_ok"):
            exact_ok += 1
        for shape in ["span", "reading"]:
            r = q.get(shape)
            if not r:
                continue
            src = num(r, "src_bytes")
            control = num(r, "control_bytes")
            unit_b = num(r, "coarse_unit_bytes")
            closure = num(r, "coarse_closure_bytes")
            dir_b = num(r, "coarse_index_bytes")
            sql = num(r, "sqlite_page_bytes")
            matches = num(r, "coarse_matches_full")
            ex = num(r, "exact_ok")
            vs_c = ("%.2fx" % (control / closure)) if closure else "n/a"
            vs_s = ("%.2fx" % (sql / closure)) if (sql and closure) else "n/a"
            vc = "WIN" if (control > 0 and closure < control) else "LOSS"
            if shape == "span":
                if control > 0 and closure < control:
                    span_wins += 1
            else:
                if control > 0 and closure < control:
                    reading_wins += 1
                if sql > 0:
                    reading_sqlite_measured += 1
                    if closure < sql:
                        reading_sqlite_wins += 1
            lines.append(
                "| %s | %s | %d | %d | %d | %d | %d | %s | %s | %s | %s | %s | %s |"
                % (fmt, shape, src, control, unit_b, dir_b, closure,
                   sql if sql else "n/m", vs_c, vc, vs_s,
                   "yes" if matches else "NO", "yes" if ex else "NO"))

    lines.append("")
    lines.append("## Phase 29.2 historical control (same corpus bytes; reading request)\n")
    lines.append("| fmt | 29.2 control B | 29.2 closure B | 29.2 verdict |")
    lines.append("|---|---:|---:|:--:|")
    for fmt in ["pdf", "docx", "odt", "parquet"]:
        d = HISTORICAL_29_2.get(fmt)
        if not d:
            continue
        lines.append("| %s | %d | %d | %s |"
                     % (fmt, d["reading_control"], d["reading_closure"], d["verdict"]))

    lines.append("")
    lines.append("## Directory overhead\n")
    lines.append("The persisted VCI1 index (the directory the read pays) is 30461 B for PDF")
    lines.append("(~34 B/object unit), 218 B for DOCX/ODT (8 members + directory) and 201 B")
    lines.append("for Parquet. It is charged once per observation in `closure`.")
    lines.append("")
    lines.append("## Remaining loss\n")
    lines.append("- **parquet** reading: still 2.0x the control. The `ParquetCell` projection")
    lines.append("  slices the containing column chunk out of a whole-source buffer")
    lines.append("  (`parquet_source()` materializes the `DocumentExact` root) and the")
    lines.append("  `ParquetModel` node reads the whole source; making the cell unit-addressed")
    lines.append("  needs a range source threaded through the Parquet value decoders, which this")
    lines.append("  subphase does not do. A footer-only model would not flip it on its own")
    lines.append("  (the whole-source read in `parquet_source()` remains).\n")
    lines.append("## Verdict\n")
    lines.append("- exactness held: %d/%d formats `materialize --exact == source`."
                 % (exact_ok, len(by_fmt)))
    lines.append("- **reading** requests: the coarse closure is now narrower than the")
    lines.append("  whole-descriptor control in %d/%d formats (Phase 29.2: 1/4), and beats the"
                 % (reading_wins, len(by_fmt)))
    lines.append("  page-SQLite control in %d/%d measured."
                 % (reading_sqlite_wins, reading_sqlite_measured))
    lines.append("- **span** requests remain narrower than the control in %d/%d formats."
                 % (span_wins, len(by_fmt)))
    lines.append("")
    lines.append("A format whose closure is not below the control is a *loss*, reported as such.")

    with open(os.path.join(camp, "SUMMARY.md"), "w") as f:
        f.write("\n".join(lines) + "\n")
    with open(os.path.join(out, "SUMMARY.md"), "w") as f:
        f.write("\n".join(lines) + "\n")

    receipt = {
        "phase": "30.4",
        "short": short,
        "n_docs": len(by_fmt),
        "exact_ok": exact_ok,
        "reading_wins_over_control": reading_wins,
        "reading_wins_over_sqlite_closure": reading_sqlite_wins,
        "reading_sqlite_measured": reading_sqlite_measured,
        "span_wins_over_control": span_wins,
        "rows": obs,
        "historical_29_2": HISTORICAL_29_2,
    }
    with open(os.path.join(camp, "receipt.json"), "w") as f:
        json.dump(receipt, f, indent=2, sort_keys=True)
    print("aggregate: %d formats, exact %d/%d, reading control wins %d/%d, "
          "reading sqlite wins %d/%d, span control wins %d/%d"
          % (len(by_fmt), exact_ok, len(by_fmt), reading_wins, len(by_fmt),
             reading_sqlite_wins, reading_sqlite_measured, span_wins, len(by_fmt)))


def main(argv):
    if len(argv) < 2:
        P292.die("usage: corpus|unit-ranges|aggregate ...")
    cmd = argv[1]
    args = {}
    i = 2
    while i < len(argv):
        key = argv[i].lstrip("-").replace("-", "_")
        args[key] = argv[i + 1]
        i += 2
    if cmd == "corpus":
        cmd_corpus(args["out"])
    elif cmd == "unit-ranges":
        cmd_unit_ranges(args["idx"], args["map"], args["request"])
    elif cmd == "aggregate":
        cmd_aggregate(args["raw"], args["out"], args["camp"], args["short"])
    else:
        P292.die("unknown command %r" % cmd)


if __name__ == "__main__":
    main(sys.argv)
