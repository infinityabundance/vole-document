#!/usr/bin/env python3
# Read-only discovery for NASA NTRS documents (PDF).
#
# Queries the public NTRS search API (https://ntrs.nasa.gov/api/citations/search)
# for a set of terms and writes a *candidate* TSV: one row per NTRS citation that
# exposes at least one PDF download, with the pre-performance attributes the
# selection discipline allows (agency, series/stiType, center, year, report
# number, size unknown until fetch). It never records a document; acquisition is
# a separate, deliberate step through acquire.py.
#
#   docker compose run --rm --no-TTY realcorpus \
#     python3 tools/realcorpus/discover-ntrs.py \
#       --out real100-v1/sources/nasa_ntrs_candidates.tsv \
#       --query "NACA technical note" --query "technical memorandum" ...
#
# Output columns: id, year, sti_type, center, report_number, title,
#                 pdf_url, landing_page

import argparse
import json
import sys
import urllib.parse
import urllib.request

UA = "vole-document-realcorpus/1.0 (discovery helper)"
API = "https://ntrs.nasa.gov/api/citations/search"


def get(url):
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=60) as r:
        return json.loads(r.read())


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--out", required=True)
    p.add_argument("--query", action="append", required=True)
    p.add_argument("--page-size", type=int, default=100)
    args = p.parse_args()

    seen = {}
    for term in args.query:
        url = "%s?q=%s&page.size=%d" % (API, urllib.parse.quote(term),
                                        args.page_size)
        try:
            data = get(url)
        except Exception as e:  # noqa: BLE001
            print("warn: query %r failed: %s" % (term, e), file=sys.stderr)
            continue
        for r in data.get("results", []):
            rid = r.get("id")
            if not rid or rid in seen:
                continue
            pdf = None
            for dl in (r.get("downloads") or []):
                if dl.get("mimetype") == "application/pdf":
                    pdf = dl.get("links", {}).get("original")
                    break
            if not pdf:
                continue
            year = ""
            pubs = r.get("publications") or []
            if pubs and pubs[0].get("publicationDate"):
                year = pubs[0]["publicationDate"][:4]
            center = (r.get("center") or {}).get("name", "")
            reports = r.get("otherReportNumbers") or []
            rn = ""
            for x in reports:
                if not x.startswith("Report Number:"):
                    rn = x
                    break
            seen[rid] = (
                str(rid), str(year), str(r.get("stiType", "")), str(center), str(rn),
                str(r.get("title") or "").replace("\t", " ").replace("\n", " "),
                "https://ntrs.nasa.gov" + str(pdf),
                "https://ntrs.nasa.gov/citations/%s" % rid,
            )

    with open(args.out, "w", encoding="utf-8") as f:
        f.write("id\tyear\tsti_type\tcenter\treport_number\ttitle\tpdf_url\tlanding_page\n")
        for row in sorted(seen.values()):
            f.write("\t".join(row) + "\n")
    print("wrote %d candidate PDFs to %s" % (len(seen), args.out),
          file=sys.stderr)


if __name__ == "__main__":
    main()
