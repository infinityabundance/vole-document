#!/usr/bin/env python3
# Phase 12.11 — pre-registered lifetime-court schedule.
#
# Reads the corpus ground truth (tools/fixtures/phase12-corpus-gen.py output) and
# freezes the **query schedule before any measurement**: the ordered case list
# per document, the deterministic expansion rule to N in {1,10,100,1000}, and the
# expected answer of every case. Committing this file *before* running the court
# is what makes the schedule pre-registered (plan ADR-0035; research F §3).
#
# The expansion rule is pure round-robin over the ordered case list:
#
#     case(i) = cases[(i - 1) mod len(cases)],  i = 1..N
#
# so every case is exercised with equal frequency and the order cannot be tuned
# after seeing any result. "Hot" rows are the cases that recur every pass; "cold
# randoms" are the first occurrence of each case before its derived cache exists.
#
# Usage: python3 tools/fixtures/phase12-schedule.py CORPUS_DIR OUT.json

import hashlib
import json
import os
import sys

NS = [1, 10, 100, 1000]
PASSES = 2

# The required ablation ladder (plan §105, ADR-0035), pre-registered here *before*
# measurement. `status` is one of:
#   baseline        a reference system (A0/A1)
#   measured        a separable rung with its own switch, run as its own lane
#   not separable   the landed architecture gates it by another rung's feature;
#                   recorded honestly (never fabricated) with the closest proxy
LADDER = [
    {"rung": "A0", "status": "baseline",
     "switch": "lane a0: direct per-query tooling (pdftotext/pdfinfo; stdlib zipfile+XML)",
     "note": "no persistence"},
    {"rung": "A1", "status": "baseline",
     "switch": "lane a1: python3 phase12-baseline.py build/query",
     "note": "source-retaining SQLite+FTS5 acceptance gate"},
    {"rung": "A1b", "status": "measured",
     "switch": "lane a1b: A1 + result_cache materialized views (one SQL source of truth)",
     "note": "A1 plus cached popular results"},
    {"rung": "A2", "status": "measured",
     "switch": "lane a2: build --no-default-features --features rans,store,field",
     "note": "Phase-11 field, PDF only: DOCX/EPUB semantic surfaces decline"},
    {"rung": "A3", "status": "measured",
     "switch": "lane a3: features rans,store,field,package (no opc/docx/epub)",
     "note": "ZIP physical members only; no OPC/OCF graph, so common semantic selectors decline"},
    {"rung": "A4", "status": "measured",
     "switch": "lane a4: features rans,store,field,package,opc,docx,epub with --no-cache",
     "note": "native package graph + progressive semantic inversion, no persistent reuse"},
    {"rung": "A5", "status": "not separable",
     "switch": "same as A4",
     "note": "progressive semantic inversion is the only implemented mode (model nodes are "
             "registered at ingest and parsed on first materialization); there is no eager "
             "arm, so A5 == A4 and is measured on the A4 lane"},
    {"rung": "A6", "status": "measured",
     "switch": "lane a6: features rans,store,field,package,opc,docx,epub (DerivedCache on)",
     "note": "A4/A5 plus persistent semantic reuse"},
    {"rung": "A7", "status": "not separable",
     "switch": "n/a (proxied by A4/A6)",
     "note": "the common observation vocabulary *is* the dispatch (Ctx::common_dispatch); "
             "there is no native-only CLI path, so removing it is a capability-only "
             "(definitional) difference, not a cost rung"},
    {"rung": "A8", "status": "not separable",
     "switch": "n/a (proxied by A3/A4)",
     "note": "the hierarchical index is built unconditionally by package/PDF ingest and "
             "every narrow observation resolves through it (indexed_exact); there is no "
             "non-indexed observation build"},
    {"rung": "A9", "status": "measured",
     "switch": "lane a9: all-features with --entropyfs",
     "note": "EntropyFS fine-grained range access (entropyfs-store backend); the narrow "
             "cache short-circuit is disabled by design on this backend"},
    {"rung": "A10", "status": "not separable (per-document court)",
     "switch": "ingest-side proxy: all documents into one shared store",
     "note": "the per-document lifetime lanes use one store per document by construction, "
             "so cross-document sharing cannot manifest there; measured as the shared-store "
             "ingest delta; the reuse-work court is the 12.8 share receipt"},
    {"rung": "A11", "status": "measured",
     "switch": "lane a11: all-features, filesystem backend, DerivedCache on",
     "note": "full Phase-12 system; identical to the prior court's V lane"},
]

# The per-format ordered case list. Each entry:
#   id, kind (text|metadata|exact), arg, expected_kind, expected
# `expected_kind` is one of: equals (exact string), contains (substring),
#   sha256 (of the answer bytes), nonempty.
DOCX_CASES = [
    ("narrow-text", "text", "block:1", "equals", "block:1"),
    ("heading-section", "text", "heading:0", "equals", "heading:0"),
    ("paragraph-context", "text", "block:3", "equals", "block:3"),
    ("table-cell", "text", "cell:0:{r}:{c}", "equals", "cell"),
    ("expand-table", "text", "table:0", "contains", "table-first-cell"),
    ("resource", "metadata", "resource:0", "nonempty", "resource"),
    ("metadata", "metadata", "metadata", "contains", "format"),
    ("adjacent-region", "text", "block:2", "equals", "block:2"),
    ("search", "text", "search:{marker}", "contains", "marker"),
    ("full-source", "exact", "full-source", "sha256", "source-sha"),
    # VOLE declines DOCX resource *decoded* bytes (native member bytes are not
    # exposed); A0/A1 can answer it, and the court records the V decline.
    ("exact-member", "exact", "resource-bytes:0", "sha256", "image-sha"),
]

EPUB_CASES = [
    ("narrow-text", "text", "block:1", "equals", "block:1"),
    ("heading-section", "text", "heading:0", "equals", "heading:0"),
    ("paragraph-context", "text", "block:3", "equals", "block:3"),
    ("table-cell", "text", "cell:0:{r}:{c}", "equals", "cell"),
    ("expand-table", "text", "table:0", "contains", "table-first-cell"),
    ("resource", "metadata", "resource:0", "nonempty", "resource"),
    ("metadata", "metadata", "metadata", "contains", "spine"),
    ("adjacent-region", "text", "block:2", "equals", "block:2"),
    ("search", "text", "search:{marker}", "contains", "marker"),
    ("full-source", "exact", "full-source", "sha256", "source-sha"),
    ("exact-member", "exact", "resource-bytes:0", "sha256", "image-sha"),
]

NATIVE_PROVENANCE = ("native-provenance", "text", "block:0", "nonempty", "provenance")
# PDF's heuristic page/search text carries no native provenance (the 12.9 court
# recorded the empty string); the case is kept in the schedule and recorded as an
# honest VOLE gap, not asserted.
PDF_NATIVE_PROVENANCE = ("native-provenance", "text", "page:1", "declined", "")

PDF_CASES = [
    ("narrow-text", "text", "page:1", "contains", "marker"),
    ("metadata", "metadata", "metadata", "nonempty", "metadata-nonempty"),
    ("adjacent-region", "text", "page:1", "contains", "marker"),
    ("search", "text", "search:{marker}", "contains", "marker"),
    ("exact-member", "exact", "byte-range:0:64", "sha256", "prefix-sha"),
    ("full-source", "exact", "full-source", "sha256", "source-sha"),
]


def canonical_blocks(v, fmt):
    """The canonical text-block order (matches the generator and the adapters).

    DOCX keeps one block per list item; EPUB (bounded XHTML) emits one block per
    list container with items joined by newlines — the adapters' real models."""
    blocks = []
    h = v["headings"]
    ps = v["paragraphs"]
    for i in range(max(len(h), len(ps))):
        if i < len(h):
            blocks.append(h[i][1])
        if i < len(ps):
            blocks.append(ps[i])
    if fmt == "epub":
        blocks.append("\n".join(v["list_items"]))
    else:
        blocks += list(v["list_items"])
    if v["link"]:
        blocks.append(v["link"][0])
    blocks.append("__table__")
    blocks.append("")
    return blocks


def read_bytes(path, off, length):
    with open(path, "rb") as f:
        f.seek(off)
        return f.read(length)


def resolve(v, gt, corpus, key, fmt, case, path):
    cid, kind, arg, expkind, expref = case
    cr = v["cell_ref"]
    blocks = canonical_blocks(v, fmt)
    if expref == "block:1":
        exp = blocks[1]
    elif expref == "block:2":
        exp = blocks[2]
    elif expref == "block:3":
        exp = blocks[3]
    elif expref == "heading:0":
        exp = v["headings"][0][1]
    elif expref == "cell":
        exp = v["table_rows"][cr[1]][cr[2]]
    elif expref == "table-first-cell":
        exp = v["table_rows"][1][0]
    elif expref == "image1":
        exp = "image1.png"
    elif expref == "image-sha":
        exp = gt["resource"]["sha256"]
    elif expref == "resource":
        exp = ""
    elif expref == "metadata-nonempty":
        exp = ""
    elif expref == "format":
        exp = fmt
    elif expref == "spine":
        exp = "spine"
    elif expref == "marker":
        exp = v["markers"][0]
    elif expref == "source-sha":
        exp = gt["formats"][key][fmt]["sha256"]
    elif expref == "prefix-sha":
        prefix = read_bytes(path, 0, 64)
        exp = hashlib.sha256(prefix).hexdigest()
    elif expref == "provenance":
        exp = "format=" + fmt
    else:
        exp = ""
    arg = (arg.replace("{r}", str(cr[1])).replace("{c}", str(cr[2]))
           .replace("{marker}", v["markers"][0]))
    return {"id": cid, "kind": kind, "arg": arg,
            "expected_kind": expkind, "expected": exp}


def main():
    if len(sys.argv) != 3:
        print("usage: phase12-schedule.py CORPUS_DIR OUT.json", file=sys.stderr)
        return 2
    corpus, out = sys.argv[1], sys.argv[2]
    gt = json.load(open(os.path.join(corpus, "ground_truth.json")))
    docs = []
    for d in gt["documents"]:
        key, fmt, name = d["variant"], d["format"], d["name"]
        v = gt["variants"][key]
        path = os.path.join(corpus, name)
        if fmt == "pdf":
            base = PDF_CASES
        elif fmt == "docx":
            base = DOCX_CASES
        else:
            base = EPUB_CASES
        cases = [resolve(v, gt, corpus, key, fmt, c, path) for c in base]
        if fmt == "pdf":
            cases.append(resolve(v, gt, corpus, key, fmt, PDF_NATIVE_PROVENANCE, path))
        else:
            cases.append(resolve(v, gt, corpus, key, fmt, NATIVE_PROVENANCE, path))
        docs.append({
            "name": name, "variant": key, "format": fmt,
            "source": name, "length": gt["formats"][key][fmt]["length"],
            "sha256": gt["formats"][key][fmt]["sha256"],
            "cell_ref": v["cell_ref"], "markers": v["markers"],
            "cases": cases,
        })
    schedule = {
        "campaign": "phase12-lifetime-court",
        "pre_registered": True,
        "rule": "case(i) = cases[(i-1) mod len(cases)], i = 1..N",
        "ns": NS,
        "passes": PASSES,
        "corpus": corpus,
        "corpus_generator": "tools/fixtures/phase12-corpus-gen.py",
        "documents": docs,
        "ladder": LADDER,
        "notes": [
            "Order is frozen before measurement; the court refuses to run without this file.",
            "Each case is a (document, surface) point named by id; the full schedule for all",
            "systems is this same ordered list expanded by the rule above.",
            "A0 = direct per-query tooling; A1 = one-time preprocessed source-retaining",
            "SQLite+FTS5; A1b = A1 + cached popular results; A2-A11 = the required Phase-12",
            "ablation ladder (see `ladder`); A11 is the full system (the prior court's V lane).",
            "No system may reorder the schedule.",
        ],
    }
    with open(out, "w") as f:
        json.dump(schedule, f, indent=2, sort_keys=True)
        f.write("\n")
    print(json.dumps({"out": out, "documents": len(docs),
                      "cases_per_doc": {d["name"]: len(d["cases"]) for d in docs},
                      "ladder_rungs": [r["rung"] for r in LADDER]},
                     sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
