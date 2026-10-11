#!/usr/bin/env python3
# Phase 26.5 — read-only discovery helper for the stratified real-world population v2.
#
# Enumerates candidate sample files in a set of pinned public repositories via the
# GitHub **git-trees** API (one recursive call per repository), so the
# `formats-manifest-v2.tsv` URL set is curated from observed reality rather than
# guessed. It never downloads sample bytes (that is `fetch-formats.sh`) and never
# writes the manifest; it only prints a candidate TSV to stdout:
#
#   repo  ref  path  size  download_url
#
# Read-only; runs in the capped, network-capable `realcorpus` lane:
#
#   docker compose run --rm --no-TTY realcorpus \
#       python3 tools/realcorpus/discover-formats-v2.py [--only repo_substr] [--ext .xlsx,.doc]
#
# The GitHub API is unauthenticated (60 req/h); one recursive tree call per repo
# keeps the whole discovery well inside that budget.

import argparse
import json
import sys
import urllib.request

# (repo, ref) — a short list of stable, permissively licensed repositories with
# abundant real test fixtures. `ref` is a tag or commit whenever possible.
REPOS = [
    ("apache/poi", "REL_5_4_1"),
    ("eea/odfpy", "2eda5eea2ae3ec2cf65ac417fb8b360eb84f4b11"),
    ("vega/vega-datasets", "main"),
    ("nvkelso/natural-earth-vector", "v5.1.2"),
    ("apache/parquet-testing", "master"),
    ("apache/arrow-testing", "master"),
    ("python/cpython", "v3.12.5"),
    ("BurntSushi/toml-test", "main"),
    ("asciidoctor/asciidoctor", "main"),
    ("mdx-js/mdx", "main"),
    ("jupyter/notebook", "main"),
    ("PJK/libcbor", "master"),
    ("OAI/OpenAPI-Specification", "main"),
    ("w3c/feedvalidator", "main"),
    ("tmcw/togeojson", "main"),
    ("Unidata/netcdf-c", "v4.9.2"),
    ("w3c/epub-tests", "main"),
    ("msgpack/msgpack-c", "master"),
    ("apache/tika", "2.9.2"),
    ("IDPF/epub3-samples", "main"),
    ("retorquere/bibtex-parser", "master"),
    ("openai/openai-cookbook", "main"),
    ("jsonlines/guide", "master"),
    ("logpai/loghub", "master"),
    ("kawanet/msgpack-test-suite", "master"),
    ("simplepie/simplepie", "1.8.0"),
]


def api(url):
    req = urllib.request.Request(url, headers={"User-Agent": "vole-realcorpus/1.0",
                                               "Accept": "application/vnd.github+json"})
    with urllib.request.urlopen(req, timeout=120) as r:
        return json.load(r)


def tree(repo, ref):
    url = "https://api.github.com/repos/%s/git/trees/%s?recursive=1" % (repo, ref)
    try:
        return api(url)
    except Exception as e:  # noqa: BLE001 - discovery is best-effort
        sys.stderr.write("WARN %s@%s -> %s\n" % (repo, ref, e))
        return {"tree": []}


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--only", default="")
    ap.add_argument("--ext", default="",
                    help="comma-separated extension filter, e.g. .xlsx,.doc")
    ns = ap.parse_args(argv)
    exts = tuple(x.strip().lower() for x in ns.ext.split(",") if x.strip())
    print("repo\tref\tpath\tsize\tdownload_url")
    for repo, ref in REPOS:
        if ns.only and ns.only not in repo:
            continue
        t = tree(repo, ref)
        for e in t.get("tree", []):
            if e.get("type") != "blob":
                continue
            p = e["path"]
            if exts and not p.lower().endswith(exts):
                continue
            dl = "https://raw.githubusercontent.com/%s/%s/%s" % (repo, ref, p)
            print("%s\t%s\t%s\t%s\t%s" % (repo, ref, p, e.get("size", ""), dl))
    return 0


if __name__ == "__main__":
    sys.exit(main())
