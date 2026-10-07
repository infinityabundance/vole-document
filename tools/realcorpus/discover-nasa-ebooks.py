#!/usr/bin/env python3
# Read-only discovery for NASA official e-book assets.
#
# NASA's e-book index (https://www.nasa.gov/ebooks/) is JavaScript-rendered, but
# the site is WordPress and exposes every e-book landing page through the public
# WP search REST API. This helper enumerates those pages, fetches each one, and
# extracts the direct `.pdf` / `.epub` asset links. It writes a *candidate* TSV
# (never a manifest row): acquisition is a separate, deliberate step run through
# acquire.py with the operator's pre-performance metadata.
#
#   docker compose run --rm --no-TTY realcorpus \
#     python3 tools/realcorpus/discover-nasa-ebooks.py --out real100-v1/sources/nasa_ebook_assets.tsv
#
# Output columns: kind, page_title, landing_page, asset_url

import argparse
import json
import re
import sys
import urllib.parse
import urllib.request

UA = "vole-document-realcorpus/1.0 (discovery helper)"


def get(url):
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=45) as r:
        return r.read()


def search_ebooks():
    """Return [(title, url)] for e-book landing pages."""
    out = []
    seen = set()
    for term in ("ebook", "e-book", "stem-content", "history", "aeronautics"):
        url = ("https://www.nasa.gov/wp-json/wp/v2/search?search=%s&per_page=100"
               % urllib.parse.quote(term))
        try:
            data = json.loads(get(url))
        except Exception as e:  # noqa: BLE001
            print("warn: search %r failed: %s" % (term, e), file=sys.stderr)
            continue
        for item in data:
            u = item.get("url", "")
            if u and u not in seen:
                seen.add(u)
                out.append((item.get("title", ""), u))
    return out


ASSET_RE = re.compile(r"https?://[^\"'<>\s]+?\.(?:epub|pdf)(?:\?[^\"'<>\s]*)?",
                      re.IGNORECASE)


def assets_on(page_url):
    try:
        html = get(page_url).decode("utf-8", "replace")
    except Exception as e:  # noqa: BLE001
        print("warn: page %s failed: %s" % (page_url, e), file=sys.stderr)
        return []
    found = {}
    for m in ASSET_RE.findall(html):
        m = m.split("?")[0]
        low = m.lower()
        kind = "epub" if low.endswith(".epub") else "pdf"
        found[m] = kind
    return sorted(found.items())


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--out", required=True)
    args = p.parse_args()

    pages = search_ebooks()
    print("e-book candidate pages: %d" % len(pages), file=sys.stderr)
    rows = []
    for title, url in pages:
        for asset, kind in assets_on(url):
            rows.append((kind, title, url, asset))
    with open(args.out, "w", encoding="utf-8") as f:
        f.write("kind\tpage_title\tlanding_page\tasset_url\n")
        for kind, title, page, asset in sorted(rows):
            title = title.replace("\t", " ").replace("\n", " ")
            f.write("%s\t%s\t%s\t%s\n" % (kind, title, page, asset))
    print("wrote %d assets (%d epub, %d pdf) to %s"
          % (len(rows),
             sum(1 for r in rows if r[0] == "epub"),
             sum(1 for r in rows if r[0] == "pdf"),
             args.out), file=sys.stderr)


if __name__ == "__main__":
    main()
