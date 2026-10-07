#!/usr/bin/env python3
# real100-v1 diversity-gate evaluator (stdlib only; runs in the pinned
# `realcorpus` Docker service, never on the host).
#
# Reads a corpus manifest.tsv (the canonical store) and evaluates every
# diversity gate from the real100-v1 specification, printing a compliance table.
# It measures *pre-performance* attributes only: agency, format, series, year,
# size class, structural tags, and cross-format / revision family identifiers.
# It never runs the codec and never reads a codec result.
#
# Complements used below (all derived from the manifest's structural_tags and
# document_type fields, and from the family ids):
#
#   scanned        = scanned | legacy-image
#   table          = table-heavy
#   figure/res     = figure-heavy | image-heavy
#   equation       = equation-heavy
#   appendix/ref   = appendix-heavy | reference-heavy | footnote-heavy
#   complex-xref   = object-stream | complex-xref | complex
#   rich-docx      = DOCX carrying any of table-heavy, deep-headings,
#                    image-heavy, list-heavy, form, procedure-heavy
#   center:<name>  = a NASA center tag (center:langley, center:glenn, ...)
#
# Usage:
#   docker compose run --rm --no-TTY realcorpus \
#     python3 tools/realcorpus/check-diversity.py --corpus real100-v1
#   ... --corpus real100-v1/pilot        # pilot view (informational)

import argparse
import os
import sys
from collections import Counter

FIELDS = [
    "id", "agency", "title", "publication_id", "format", "source_url",
    "landing_page", "retrieved_utc", "sha256", "byte_len", "publication_year",
    "publication_family", "document_type", "producer_or_origin_if_known",
    "size_class", "structural_tags", "cross_format_family_id",
    "revision_family_id", "rights_status", "redistributable",
]

SIZE_CLASSES = ["<100KiB", "100KiB-1MiB", "1-10MiB", "10-50MiB",
                "50-100MiB", ">100MiB"]


def read_rows(corpus):
    tsv = os.path.join(corpus, "manifest.tsv")
    if not os.path.exists(tsv):
        return []
    rows = []
    with open(tsv, "r", encoding="utf-8") as f:
        header = f.readline().rstrip("\n").split("\t")
        if header != FIELDS:
            raise SystemExit("manifest.tsv header does not match schema")
        for line in f:
            line = line.rstrip("\n")
            if not line:
                continue
            vals = line.split("\t")
            if len(vals) != len(FIELDS):
                raise SystemExit("bad manifest row: %r" % line[:100])
            rows.append(dict(zip(FIELDS, vals)))
    return rows


def tags(r):
    return set(t for t in r["structural_tags"].split(";") if t)


def year(r):
    v = r["publication_year"]
    return int(v) if v.isdigit() else None


def ntags(r, names):
    t = tags(r)
    return 1 if t & set(names) else 0


def count(rows, pred):
    return sum(1 for r in rows if pred(r))


# ---------------------------------------------------------------------------
# Gate table. Each gate: (group, label, lo, hi, measure(rows) -> int).
# hi=None means "at least lo". A gate PASSes when lo <= cur <= hi (or cur>=lo).
# ---------------------------------------------------------------------------
def cross_pairs(rows, agency, fa, fb):
    a = set(r["cross_format_family_id"] for r in rows
            if r["agency"] == agency and r["format"] == fa
            and r["cross_format_family_id"])
    b = set(r["cross_format_family_id"] for r in rows
            if r["agency"] == agency and r["format"] == fb
            and r["cross_format_family_id"])
    return len(a & b)


def gates(rows):
    nasa = [r for r in rows if r["agency"] == "nasa"]
    nist = [r for r in rows if r["agency"] == "nist"]
    npdf = [r for r in nasa if r["format"] == "pdf"]
    nep = [r for r in nasa if r["format"] == "epub"]
    xpdf = [r for r in nist if r["format"] == "pdf"]
    xdoc = [r for r in nist if r["format"] == "docx"]
    xep = [r for r in nist if r["format"] == "epub"]

    def yr_band(r, lo, hi):
        y = year(r)
        return 1 if (y is not None and lo <= y <= hi) else 0

    def rich_docx(r):
        return r["format"] == "docx" and bool(
            tags(r) & {"table-heavy", "deep-headings", "image-heavy",
                       "list-heavy", "form", "procedure-heavy"})

    G = []

    def add(group, label, lo, hi, fn):
        G.append((group, label, lo, hi, fn))

    # --- composition ------------------------------------------------------
    add("composition", "NASA PDF", 40, 40, lambda rs: len(npdf))
    add("composition", "NASA EPUB", 15, 15, lambda rs: len(nep))
    add("composition", "NIST PDF", 20, 20, lambda rs: len(xpdf))
    add("composition", "NIST DOCX", 15, 15, lambda rs: len(xdoc))
    add("composition", "NIST EPUB", 10, 10, lambda rs: len(xep))
    add("composition", "NASA total", 55, 55, lambda rs: len(nasa))
    add("composition", "NIST total", 45, 45, lambda rs: len(nist))
    add("composition", "PDF total", 60, 60, lambda rs: len(npdf) + len(xpdf))
    add("composition", "DOCX total", 15, 15, lambda rs: len(xdoc))
    add("composition", "EPUB total", 25, 25, lambda rs: len(nep) + len(xep))

    # --- NASA PDF eras ----------------------------------------------------
    add("nasa-era", "pre-1960", 5, 5, lambda rs: count(npdf, lambda r: yr_band(r, 0, 1959)))
    add("nasa-era", "1960-1979", 7, 7, lambda rs: count(npdf, lambda r: yr_band(r, 1960, 1979)))
    add("nasa-era", "1980-1999", 8, 8, lambda rs: count(npdf, lambda r: yr_band(r, 1980, 1999)))
    add("nasa-era", "2000-2014", 8, 8, lambda rs: count(npdf, lambda r: yr_band(r, 2000, 2014)))
    add("nasa-era", "2015-2024", 8, 8, lambda rs: count(npdf, lambda r: yr_band(r, 2015, 2024)))
    add("nasa-era", "2025-2026", 4, 4, lambda rs: count(npdf, lambda r: yr_band(r, 2025, 2026)))

    # --- NASA centers (soft: no single center dominating) -----------------
    centers = Counter()
    for r in nasa:
        for t in tags(r):
            if t.startswith("center:"):
                centers[t.split(":", 1)[1]] += 1
    distinct = len(centers)
    add("nasa-center", "distinct centers (>=4, none >40%)", 4, None, lambda rs: distinct)
    top = max(centers.values()) if centers else 0
    add("nasa-center", "max single-center share <=40%", 0, 0,
        lambda rs: 0 if (len(nasa) == 0 or top / max(len(nasa), 1) <= 0.4) else 1)

    # --- NASA doc types ---------------------------------------------------
    add("nasa-doctype", "TM", 8, 10, lambda rs: count(npdf, lambda r: r["document_type"] == "TM"))
    add("nasa-doctype", "TR", 8, 10, lambda rs: count(npdf, lambda r: r["document_type"] in ("TR", "TN")))
    add("nasa-doctype", "CR", 5, 7, lambda rs: count(npdf, lambda r: r["document_type"] == "CR"))
    add("nasa-doctype", "conference/research", 4, 6,
        lambda rs: count(npdf, lambda r: r["document_type"] in ("CONFERENCE", "CONFERENCE_PAPER", "RESEARCH")))
    add("nasa-doctype", "handbook/reference", 3, 5,
        lambda rs: count(npdf, lambda r: r["document_type"] in ("HANDBOOK", "REFERENCE", "SP")))
    add("nasa-doctype", "NACA/early", 5, None,
        lambda rs: count(npdf, lambda r: r["document_type"] == "NACA" or (year(r) or 9999) < 1960))

    # --- NASA PDF structure ----------------------------------------------
    add("nasa-struct", "scanned/legacy-image", 8, None, lambda rs: count(npdf, lambda r: ntags(r, ["scanned", "legacy-image"])))
    add("nasa-struct", "table-heavy", 8, None, lambda rs: count(npdf, lambda r: ntags(r, ["table-heavy"])))
    add("nasa-struct", "figure-heavy", 8, None, lambda rs: count(npdf, lambda r: ntags(r, ["figure-heavy"])))
    add("nasa-struct", "equation-heavy", 6, None, lambda rs: count(npdf, lambda r: ntags(r, ["equation-heavy"])))
    add("nasa-struct", "multi-column", 5, None, lambda rs: count(npdf, lambda r: ntags(r, ["multi-column"])))
    add("nasa-struct", "appendix/reference-heavy", 5, None, lambda rs: count(npdf, lambda r: ntags(r, ["appendix-heavy", "reference-heavy"])))
    add("nasa-struct", "very large", 3, None, lambda rs: count(npdf, lambda r: ntags(r, ["very-large"]) or r["size_class"] == ">100MiB"))
    add("nasa-struct", "simple born-digital", 4, None, lambda rs: count(npdf, lambda r: ntags(r, ["simple", "born-digital"])))

    # --- NASA EPUB --------------------------------------------------------
    add("nasa-epub", "long multi-chapter", 4, None, lambda rs: count(nep, lambda r: ntags(r, ["multi-chapter"])))
    add("nasa-epub", "image-heavy", 3, None, lambda rs: count(nep, lambda r: ntags(r, ["image-heavy"])))
    add("nasa-epub", "reference/footnote-heavy", 2, None, lambda rs: count(nep, lambda r: ntags(r, ["reference-heavy", "footnote-heavy"])))
    add("nasa-epub", "simple", 2, None, lambda rs: count(nep, lambda r: ntags(r, ["simple"])))
    add("nasa-epub", "nav-heavy", 2, None, lambda rs: count(nep, lambda r: ntags(r, ["nav-heavy"])))
    add("nasa-epub", "unusual/large", 2, None, lambda rs: count(nep, lambda r: ntags(r, ["unusual", "very-large"])))

    # --- NIST PDF families ------------------------------------------------
    add("nist-pdf", "SP", 5, None, lambda rs: count(xpdf, lambda r: r["publication_family"] == "SP"))
    add("nist-pdf", "NISTIR", 4, None, lambda rs: count(xpdf, lambda r: r["publication_family"] == "NISTIR"))
    add("nist-pdf", "Handbooks", 3, None, lambda rs: count(xpdf, lambda r: r["publication_family"] == "HANDBOOK"))
    add("nist-pdf", "TN", 3, None, lambda rs: count(xpdf, lambda r: r["publication_family"] == "TN"))
    add("nist-pdf", "FIPS", 2, None, lambda rs: count(xpdf, lambda r: r["publication_family"] == "FIPS"))
    add("nist-pdf", "other", 3, None, lambda rs: count(xpdf, lambda r: r["publication_family"] == "OTHER"))

    # --- NIST DOCX --------------------------------------------------------
    add("nist-docx", "table-heavy", 3, None, lambda rs: count(xdoc, lambda r: ntags(r, ["table-heavy"])))
    add("nist-docx", "procedure-heavy", 2, None, lambda rs: count(xdoc, lambda r: ntags(r, ["procedure-heavy"])))
    add("nist-docx", "deep headings", 2, None, lambda rs: count(xdoc, lambda r: ntags(r, ["deep-headings"])))
    add("nist-docx", "forms", 2, None, lambda rs: count(xdoc, lambda r: ntags(r, ["form"])))
    add("nist-docx", "glossary/index", 2, None, lambda rs: count(xdoc, lambda r: ntags(r, ["glossary", "index"])))
    add("nist-docx", "image-heavy", 2, None, lambda rs: count(xdoc, lambda r: ntags(r, ["image-heavy"])))
    add("nist-docx", "list-heavy", 1, None, lambda rs: count(xdoc, lambda r: ntags(r, ["list-heavy"])))
    add("nist-docx", "long regulatory", 1, None, lambda rs: count(xdoc, lambda r: ntags(r, ["long-regulatory"])))

    # --- NIST EPUB --------------------------------------------------------
    add("nist-epub", "long", 3, None, lambda rs: count(xep, lambda r: ntags(r, ["multi-chapter", "long"])))
    add("nist-epub", "table-heavy", 2, None, lambda rs: count(xep, lambda r: ntags(r, ["table-heavy"])))
    add("nist-epub", "figure-heavy", 2, None, lambda rs: count(xep, lambda r: ntags(r, ["figure-heavy", "image-heavy"])))
    add("nist-epub", "reference-heavy", 1, None, lambda rs: count(xep, lambda r: ntags(r, ["reference-heavy", "footnote-heavy"])))
    add("nist-epub", "simple", 1, None, lambda rs: count(xep, lambda r: ntags(r, ["simple"])))
    add("nist-epub", "unusual", 1, None, lambda rs: count(xep, lambda r: ntags(r, ["unusual"])))

    # --- cross-format pairs (agency originals, not local conversions) -----
    add("cross-format", "NASA PDF<->EPUB (10-12)", 10, 12, lambda rs: cross_pairs(rows, "nasa", "pdf", "epub"))
    add("cross-format", "NIST PDF<->DOCX (8-10)", 8, 10, lambda rs: cross_pairs(rows, "nist", "pdf", "docx"))
    add("cross-format", "NIST PDF<->EPUB (>=5)", 5, None, lambda rs: cross_pairs(rows, "nist", "pdf", "epub"))

    # --- revision families ------------------------------------------------
    add("revision", "docs in revision/publication families (15-20)", 15, 20,
        lambda rs: count(rows, lambda r: bool(r["revision_family_id"])))

    # --- size distribution ------------------------------------------------
    add("size", "<100KiB", 10, 10, lambda rs: count(rows, lambda r: r["size_class"] == "<100KiB"))
    add("size", "100KiB-1MiB", 20, 20, lambda rs: count(rows, lambda r: r["size_class"] == "100KiB-1MiB"))
    add("size", "1-10MiB", 30, 30, lambda rs: count(rows, lambda r: r["size_class"] == "1-10MiB"))
    add("size", "10-50MiB", 20, 20, lambda rs: count(rows, lambda r: r["size_class"] == "10-50MiB"))
    add("size", "50-100MiB", 10, 10, lambda rs: count(rows, lambda r: r["size_class"] == "50-100MiB"))
    add("size", ">100MiB", 5, 5, lambda rs: count(rows, lambda r: r["size_class"] == ">100MiB"))

    # --- whole-corpus minimums -------------------------------------------
    add("corpus-min", "tables (>=25)", 25, None, lambda rs: count(rows, lambda r: ntags(r, ["table-heavy"])))
    add("corpus-min", "figure/resource-heavy (>=25)", 25, None, lambda rs: count(rows, lambda r: ntags(r, ["figure-heavy", "image-heavy"])))
    add("corpus-min", "equations (>=15)", 15, None, lambda rs: count(rows, lambda r: ntags(r, ["equation-heavy"])))
    add("corpus-min", "appendix/reference-heavy (>=15)", 15, None, lambda rs: count(rows, lambda r: ntags(r, ["appendix-heavy", "reference-heavy", "footnote-heavy"])))
    add("corpus-min", "scanned PDFs (>=10)", 10, None, lambda rs: count(rows, lambda r: r["format"] == "pdf" and ntags(r, ["scanned", "legacy-image"])))
    add("corpus-min", "complex/object-stream PDFs (>=10)", 10, None, lambda rs: count(rows, lambda r: r["format"] == "pdf" and ntags(r, ["object-stream", "complex-xref", "complex"])))
    add("corpus-min", "multi-column (>=8)", 8, None, lambda rs: count(rows, lambda r: ntags(r, ["multi-column"])))
    add("corpus-min", "rich DOCX (>=8)", 8, None, lambda rs: count(xdoc, rich_docx))
    add("corpus-min", "unusual DOCX (>=6)", 6, None, lambda rs: count(xdoc, lambda r: ntags(r, ["unusual"])))
    add("corpus-min", "substantial-spine EPUBs (>=8)", 8, None, lambda rs: count(rows, lambda r: r["format"] == "epub" and ntags(r, ["multi-chapter"])))
    add("corpus-min", "image-heavy EPUBs (>=6)", 6, None, lambda rs: count(rows, lambda r: r["format"] == "epub" and ntags(r, ["image-heavy"])))
    add("corpus-min", "files >100MiB (>=5)", 5, None, lambda rs: count(rows, lambda r: r["size_class"] == ">100MiB"))

    return G


def status(cur, lo, hi):
    if cur >= lo and (hi is None or cur <= hi):
        return "PASS"
    if cur == 0:
        return "FAIL"
    return "PARTIAL"


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--corpus", default="real100-v1")
    p.add_argument("--only", default=None,
                   help="only print gates whose group contains this substring")
    args = p.parse_args()

    rows = read_rows(args.corpus)
    print("corpus: %s" % args.corpus)
    print("documents in manifest: %d" % len(rows))
    by = Counter((r["agency"], r["format"]) for r in rows)
    for k in sorted(by):
        print("  %-5s %-5s %d" % (k[0], k[1], by[k]))
    print()

    G = gates(rows)
    groups = []
    for g in G:
        if g[0] not in groups:
            groups.append(g[0])

    npass = npart = nfail = 0
    for group in groups:
        if args.only and args.only not in group:
            continue
        print("== %s ==" % group)
        print("  %-40s %-10s %6s  %s" % ("gate", "target", "now", "status"))
        for (grp, label, lo, hi, fn) in G:
            if grp != group:
                continue
            cur = fn(rows)
            if hi == 0 and lo == 0:
                # boolean-style gate: cur == 0 is PASS, anything else FAIL
                tgt = "=0"
                st = "PASS" if cur == 0 else "FAIL"
            else:
                tgt = ("%d" % lo) if hi is None else ("%d-%d" % (lo, hi))
                st = status(cur, lo, hi)
            if st == "PASS":
                npass += 1
            elif st == "PARTIAL":
                npart += 1
            else:
                nfail += 1
            print("  %-40s %-10s %6d  %s" % (label, tgt, cur, st))
        print()

    print("summary: %d PASS  %d PARTIAL  %d FAIL  (of %d gates)"
          % (npass, npart, nfail, npass + npart + nfail))
    return 0


if __name__ == "__main__":
    sys.exit(main())
