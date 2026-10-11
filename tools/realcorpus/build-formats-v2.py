#!/usr/bin/env python3
# Phase 26.5 — build the stratified real-world population v2 (manifest + candidates).
#
# Two modes, both Docker-only in the capped `realcorpus` lane:
#
#   # 1. select candidate files from a captured discovery snapshot (read-only):
#   docker compose run --rm --no-TTY realcorpus python3 tools/realcorpus/build-formats-v2.py \
#       select --discovery realformats-v1/.cache/discovery/all.tsv \
#                --discovery realformats-v1/.cache/discovery/new.tsv \
#       > realformats-v1/.cache/formats-v2-candidates.tsv
#
#   # 2. download + hash the candidates into a pinned manifest:
#   docker compose run --rm --no-TTY realcorpus python3 tools/realcorpus/build-formats-v2.py \
#       build --candidates tools/realcorpus/sources/formats-v2-candidates.tsv \
#       > realformats-v1/.cache/formats-manifest-v2.tsv
#
# `select` is deterministic: for every rule it filters the discovery rows by path
# regex, size window and count, then spreads the picks across the size range so the
# size stratum is represented. `build` downloads every candidate, fails hard on any
# non-2xx/empty body, records the true byte length + SHA-256, and prints the final
# manifest (id, format, bytes, sha256, url, source, license, redistributable, notes,
# complexity). The manifest is what `fetch-formats.sh` verifies; the network is
# never trusted, only the recorded length + hash are.

import argparse
import hashlib
import os
import sys
import urllib.request

# ---------------------------------------------------------------------------
# Selection rules. Each rule picks up to `n` files whose path matches `regex`
# and whose size is in [lo, hi], spread across the size range. `format` is the
# DECLARED (most-specific, semantically correct) format of the sample, which the
# court later compares against the detector's choice.
# ---------------------------------------------------------------------------
RULES = [
    # --- Apache POI: the OOXML + legacy binary office family -----------------
    ("apache/poi", "REL_5_4_1", r"^(?!.*clusterfuzz).*test-data/spreadsheet/.*\.xlsx$", "xlsx", "poi", 8, 1000, 400000,
     "Apache POI (tag REL_5_4_1)", "Apache-2.0", "true", "nested"),
    ("apache/poi", "REL_5_4_1", r"^(?!.*clusterfuzz).*test-data/spreadsheet/.*\.xls$", "xls", "poi", 8, 1000, 400000,
     "Apache POI (tag REL_5_4_1)", "Apache-2.0", "true", "nested"),
    ("apache/poi", "REL_5_4_1", r"^(?!.*clusterfuzz).*test-data/document/.*\.docx$", "docx", "poi", 8, 1000, 400000,
     "Apache POI (tag REL_5_4_1)", "Apache-2.0", "true", "nested"),
    ("apache/poi", "REL_5_4_1", r"^(?!.*clusterfuzz).*test-data/document/.*\.doc$", "doc", "poi", 8, 1000, 400000,
     "Apache POI (tag REL_5_4_1)", "Apache-2.0", "true", "nested"),
    ("apache/poi", "REL_5_4_1", r"^(?!.*clusterfuzz).*test-data/slideshow/.*\.pptx$", "pptx", "poi", 6, 1000, 600000,
     "Apache POI (tag REL_5_4_1)", "Apache-2.0", "true", "nested"),
    ("apache/poi", "REL_5_4_1", r"^(?!.*clusterfuzz).*test-data/slideshow/.*\.ppt$", "ppt", "poi", 6, 1000, 600000,
     "Apache POI (tag REL_5_4_1)", "Apache-2.0", "true", "nested"),
    ("apache/poi", "REL_5_4_1", r"^test-data/hsmf/.*\.msg$", "msg", "poi", 6, 1000, 200000,
     "Apache POI (tag REL_5_4_1)", "Apache-2.0", "true", "nested"),
    ("apache/poi", "REL_5_4_1", r"^test-data/.*\.rtf$", "rtf", "poi", 1, 100, 400000,
     "Apache POI (tag REL_5_4_1)", "Apache-2.0", "true", "structured"),
    ("apache/poi", "REL_5_4_1", r"\.properties$", "config", "poi", 2, 300, 200000,
     "Apache POI (tag REL_5_4_1)", "Apache-2.0", "true", "simple"),
    ("apache/poi", "REL_5_4_1", r"^test-data/(publisher|hpsf)/.*\.(pub|vsd|mpp)$", "cfbf", "poi", 4, 3000, 400000,
     "Apache POI (tag REL_5_4_1)", "Apache-2.0", "true", "nested"),
    ("apache/poi", "REL_5_4_1", r"^test-data/poifs/.*\.ole2$", "cfbf", "poi-ole", 1, 1000, 100000,
     "Apache POI (tag REL_5_4_1)", "Apache-2.0", "true", "nested"),
    ("apache/poi", "REL_5_4_1", r"^test-data/openxml4j/.*\.zip$", "container", "poi", 1, 500, 200000,
     "Apache POI (tag REL_5_4_1)", "Apache-2.0", "true", "nested"),
    ("apache/poi", "REL_5_4_1", r"^test-data/.*\.xml$", "xml", "poi", 2, 100, 200000,
     "Apache POI (tag REL_5_4_1)", "Apache-2.0", "true", "structured"),

    # --- odfpy: the OpenDocument family --------------------------------------
    ("eea/odfpy", "2eda5eea2ae3ec2cf65ac417fb8b360eb84f4b11", r"^tests/examples/.*\.odt$", "odt", "odfpy", 7, 1000, 300000,
     "odfpy (commit 2eda5ee)", "Apache-2.0", "true", "nested"),
    ("eea/odfpy", "2eda5eea2ae3ec2cf65ac417fb8b360eb84f4b11", r"^tests/examples/.*\.ods$", "ods", "odfpy", 4, 1000, 300000,
     "odfpy (commit 2eda5ee)", "Apache-2.0", "true", "nested"),
    ("eea/odfpy", "2eda5eea2ae3ec2cf65ac417fb8b360eb84f4b11", r"^tests/examples/.*\.odp$", "odp", "odfpy", 2, 1000, 300000,
     "odfpy (commit 2eda5ee)", "Apache-2.0", "true", "nested"),

    # --- vega-datasets: tabular + JSON ---------------------------------------
    ("vega/vega-datasets", "main", r"^data/.*\.json$", "json", "vega", 7, 1000, 400000,
     "vega-datasets (main pin)", "BSD-3-Clause", "true", "structured"),
    ("vega/vega-datasets", "main", r"^data/.*\.csv$", "csv", "vega", 7, 200, 300000,
     "vega-datasets (main pin)", "BSD-3-Clause", "true", "simple"),
    ("vega/vega-datasets", "main", r"^data/.*\.tsv$", "csv", "vega-tsv", 1, 100, 300000,
     "vega-datasets (main pin)", "BSD-3-Clause", "true", "simple"),
    ("vega/vega-datasets", "main", r"^package\.json$", "pkgmeta", "vega", 1, 100, 200000,
     "vega-datasets (main pin)", "BSD-3-Clause", "true", "structured"),

    # --- natural-earth: geospatial + archives --------------------------------
    ("nvkelso/natural-earth-vector", "v5.1.2", r"^geojson/.*\.geojson$", "geojson", "ne", 7, 1000, 400000,
     "Natural Earth v5.1.2", "Public-Domain", "true", "structured"),
    ("nvkelso/natural-earth-vector", "v5.1.2", r"^.*\.zip$", "container", "ne-zip", 4, 10000, 2000000,
     "Natural Earth v5.1.2", "Public-Domain", "true", "nested"),
    ("nvkelso/natural-earth-vector", "v5.1.2", r"^.*\.rtf$", "rtf", "ne", 2, 100, 400000,
     "Natural Earth v5.1.2", "Public-Domain", "true", "structured"),

    # --- analytical: parquet + arrow ----------------------------------------
    ("apache/parquet-testing", "master", r"^data/.*\.parquet$", "parquet", "parquet", 7, 500, 400000,
     "parquet-testing (master pin)", "Apache-2.0", "true", "nested"),
    ("apache/arrow-testing", "master", r"\.arrow_file$", "arrow", "arrow-file", 4, 100, 400000,
     "arrow-testing (master pin)", "Apache-2.0", "true", "nested"),
    ("apache/arrow-testing", "master", r"\.stream$", "arrow", "arrow-stream", 4, 100, 400000,
     "arrow-testing (master pin)", "Apache-2.0", "true", "nested"),

    # --- CPython: prose, mail, text configs ----------------------------------
    ("python/cpython", "v3.12.5", r"^Doc/.*\.rst$", "rst", "cpython", 6, 500, 100000,
     "CPython (tag v3.12.5)", "PSF-2.0", "true", "structured"),
    ("python/cpython", "v3.12.5", r"^Doc/.*\.pdf$", "pdf", "cpython", 1, 500, 5000000,
     "CPython (tag v3.12.5)", "PSF-2.0", "true", "nested"),
    ("python/cpython", "v3.12.5", r"^Lib/test/test_email/data/(?!.*(msg_35|msg_19)).*\.txt$", "eml", "cpython", 6, 100, 100000,
     "CPython (tag v3.12.5)", "PSF-2.0", "true", "structured"),
    ("python/cpython", "v3.12.5", r"^\.github/workflows/.*\.yml$", "yaml", "cpython", 5, 500, 100000,
     "CPython (tag v3.12.5)", "PSF-2.0", "true", "structured"),
    ("python/cpython", "v3.12.5", r"^Modules/_xxtestfuzz/fuzz_json_loads_corpus/.*\.json$", "json", "cpython", 3, 3, 100000,
     "CPython (tag v3.12.5)", "PSF-2.0", "true", "structured"),
    ("python/cpython", "v3.12.5", r"^Lib/test/.*\.ini$", "config", "cpython", 2, 100, 100000,
     "CPython (tag v3.12.5)", "PSF-2.0", "true", "simple"),
    ("python/cpython", "v3.12.5", r"^Lib/test/.*\.zip$", "container", "cpython", 3, 100, 100000,
     "CPython (tag v3.12.5)", "PSF-2.0", "true", "nested"),
    ("python/cpython", "v3.12.5", r"^Lib/test/.*\.tar$", "container", "cpython", 1, 100, 2000000,
     "CPython (tag v3.12.5)", "PSF-2.0", "true", "nested"),
    ("python/cpython", "v3.12.5", r"^.*\.md$", "markdown", "cpython", 3, 500, 100000,
     "CPython (tag v3.12.5)", "PSF-2.0", "true", "simple"),

    # --- toml-test: canonical TOML valid set ---------------------------------
    ("BurntSushi/toml-test", "main", r"^tests/valid/.*\.toml$", "toml", "toml", 8, 20, 100000,
     "toml-test (main pin)", "MIT", "true", "structured"),

    # --- asciidoctor: AsciiDoc + XML -----------------------------------------
    ("asciidoctor/asciidoctor", "main", r"\.adoc$", "asciidoc", "asciidoctor", 7, 200, 200000,
     "asciidoctor (main pin)", "MIT", "true", "structured"),

    # --- MDX ------------------------------------------------------------------
    ("mdx-js/mdx", "main", r"\.mdx$", "mdx", "mdx", 7, 50, 200000,
     "mdx-js (main pin)", "MIT", "true", "structured"),
    ("mdx-js/mdx", "main", r"^renovate\.json5$", "json5", "mdx-js", 1, 20, 100000,
     "mdx-js (main pin)", "MIT", "true", "structured"),

    # --- Jupyter notebooks + package metadata --------------------------------
    ("jupyter/notebook", "main", r"\.ipynb$", "notebook", "jupyter", 7, 100, 400000,
     "jupyter/notebook (main pin)", "BSD-3-Clause", "true", "structured"),
    ("jupyter/notebook", "main", r"^package\.json$", "pkgmeta", "jupyter", 1, 100, 200000,
     "jupyter/notebook (main pin)", "BSD-3-Clause", "true", "structured"),
    ("jupyter/notebook", "main", r"^pyproject\.toml$", "pkgmeta", "jupyter-py", 1, 100, 200000,
     "jupyter/notebook (main pin)", "BSD-3-Clause", "true", "structured"),

    # --- libcbor: CBOR -------------------------------------------------------
    ("PJK/libcbor", "master", r"\.cbor$", "cbor", "cbor", 7, 1, 100000,
     "libcbor (master pin)", "MIT", "true", "nested"),

    # --- OpenAPI Specification: API/spec documents ---------------------------
    ("OAI/OpenAPI-Specification", "main", r"^examples/v3\.0/.*\.json$", "apispec", "oai", 7, 100, 300000,
     "OpenAPI-Specification (main pin)", "Apache-2.0", "true", "structured"),

    # --- feedvalidator: syndication + HTML -----------------------------------
    ("w3c/feedvalidator", "main", r"^testcases/atom/1\.1/extensive-.*\.xml$", "feed", "feed", 2, 100, 100000,
     "feedvalidator (main pin)", "W3C", "true", "structured"),
    ("w3c/feedvalidator", "main", r"^testcases/atom/3\.3/.*\.xml$", "feed", "feed-a33", 4, 100, 100000,
     "feedvalidator (main pin)", "W3C", "true", "structured"),
    ("w3c/feedvalidator", "main", r"^news/archives/.*\.html$", "html", "feedval", 4, 500, 200000,
     "feedvalidator (main pin)", "W3C", "true", "structured"),

    # --- togeojson: KML / GPX ------------------------------------------------
    ("tmcw/togeojson", "main", r"^test/data/.*\.(kml|gpx)$", "gis", "togeojson", 9, 100, 400000,
     "togeojson (main pin)", "BSD-2-Clause", "true", "structured"),

    # --- netcdf-c: scientific containers -------------------------------------
    ("Unidata/netcdf-c", "v4.9.2", r"\.nc$", "hdf", "netcdf", 6, 500, 200000,
     "netcdf-c (tag v4.9.2)", "BSD-3-Clause", "true", "nested"),
    ("Unidata/netcdf-c", "v4.9.2", r"\.h5$", "hdf", "netcdf-h5", 2, 500, 200000,
     "netcdf-c (tag v4.9.2)", "BSD-3-Clause", "true", "nested"),
    ("Unidata/netcdf-c", "v4.9.2", r"\.pdf$", "pdf", "netcdf", 1, 500, 5000000,
     "netcdf-c (tag v4.9.2)", "BSD-3-Clause", "true", "nested"),
    ("asciidoctor/asciidoctor", "main", r"\.pdf$", "pdf", "asciidoctor", 1, 500, 5000000,
     "asciidoctor (main pin)", "MIT", "true", "nested"),

    # --- Apache Tika: MHTML + EPUB (its unique contribution) -----------------
    # Tika's test corpus mixes provenance; kept NON-redistributable so only the
    # length + SHA-256 are pinned, never the bytes.
    ("apache/tika", "2.9.2", r"\.mhtml$", "mhtml", "tika", 2, 100, 200000,
     "Apache Tika test corpus (tag 2.9.2)", "Apache-2.0 (test corpus; provenance varies)", "false", "nested"),
    ("apache/tika", "2.9.2", r"\.epub$", "epub", "tika", 2, 100, 200000,
     "Apache Tika test corpus (tag 2.9.2)", "Apache-2.0 (test corpus; provenance varies)", "false", "nested"),

    # --- bibtex-parser: BibTeX (the two large, clean, real bibliographies) ----
    ("retorquere/bibtex-parser", "master", r"^(strings\.bib|build/.*\.bib)$", "bib", "bibtex", 2, 200, 500000,
     "bibtex-parser (master pin)", "MIT", "true", "structured"),

    # --- loghub: real log streams (provenance varies; not redistributed) -----
    ("logpai/loghub", "master", r"^[A-Za-z]+/[A-Za-z0-9]+_2k\.log$", "logstream", "loghub", 6, 1000, 2000000,
     "loghub (master pin)", "research dataset (provenance varies)", "false", "simple"),

    # --- JSON Lines ----------------------------------------------------------
    ("jsonlines/guide", "master", r"^datagov100\.json$", "jsonl", "jsonlines", 1, 100, 2000000,
     "jsonlines/guide (master pin)", "MIT", "true", "structured"),
    ("openai/openai-cookbook", "main", r"^examples/data/.*\.jsonl$", "jsonl", "oai", 4, 1000, 900000,
     "openai-cookbook (main pin)", "MIT", "true", "structured"),
]


# v1 rows whose declared format was the coarse physical encoding; v2 declares the
# genuinely more-specific format the detector (and RFC 7946 / package-manifest
# shape) names. This is a curation correction, not a detection finding.
FORMAT_FIXUP = {}
for _i in ("natural-earth-countries.geojson", "natural-earth-places.geojson",
           "ne-regions.geojson", "ne-lakes.geojson", "ne-bbox.geojson"):
    FORMAT_FIXUP[_i] = "geojson"
for _i in ("serde-cargo.toml", "cargo-cargo.toml"):
    FORMAT_FIXUP[_i] = "pkgmeta"


# Phase 26.5 hostile stratum v2: each row is derived deterministically from a
# pinned v2 manifest row (truncation, wrong-magic, dropped tail) and pinned by
# SHA-256, so the derivation is falsifiable. The derived bytes never enter the
# repository; the recipe + hash are the record.
# (id, base_id, transform, notes)
HOSTILE_RECIPES = [
    ("hostile2-trunc-xls", "poi-spreadsheet-48325.xls", "truncate:1500", "truncated BIFF8 .xls (CFBF sectors cut) -> Opaque/CFBF floor"),
    ("hostile2-magic-doc", "poi-document-Bug46817.doc", "replace4:D0CF", "wrong-magic .doc (CFBF signature clobbered) -> Opaque"),
    ("hostile2-trunc-ppt", "poi-slideshow-basic_test_ppt_file.ppt", "truncate:8000", "truncated .ppt (CFBF sectors cut)"),
    ("hostile2-trunc-msg", "poi-hsmf-simple_test_msg.msg", "truncate:1500", "truncated .msg (property streams cut)"),
    ("hostile2-magic-docx", "poi-document-58618.docx", "replace4:XXXX", "wrong-magic OPC/DOCX (zip signature clobbered) -> Opaque"),
    ("hostile2-trunc-docx", "poi-document-Bug60341.docx", "truncate:4000", "truncated OPC/DOCX (zip) -> Opaque"),
    ("hostile2-trunc-odt", "odfpy-examples-simpletable.odt", "truncate:3000", "truncated ODF/ODT (zip) -> Opaque"),
    ("hostile2-trunc-parquet", "parquet-alltypes-plain.parquet", "truncate:800", "truncated Parquet (footer gone) -> Opaque"),
    ("hostile2-trunc-cbor", "cbor-rfc8949-good.cbor", "truncate:40", "truncated CBOR stream -> Opaque"),
    ("hostile2-trunc-hdf", "netcdf-ncdump-ref_no_ncproperty.nc", "truncate:600", "truncated NetCDF classic -> Opaque"),
    ("hostile2-magic-arrow", "arrow-file-cpp-21.0.0-generated_null_trivial.arrow_file", "replace4:XXXX", "wrong-magic Arrow (ARROW1 clobbered) -> Opaque"),
    ("hostile2-trunc-gis", "togeojson-data-point_extra_coordinates.kml", "truncate:200", "truncated KML (unclosed root) -> Opaque"),
    ("hostile2-trunc-notebook", "jupyter-notebooks-simple.ipynb", "truncate:400", "truncated nbformat JSON -> Opaque"),
    ("hostile2-trunc-mhtml", "tika-test-documents-testMHTMLFirefox.mhtml", "truncate:1000", "truncated MHTML -> Opaque"),
    ("hostile2-trunc-container", "ne-zip-zip-ne_50m_graticules_15.zip", "truncate:5000", "truncated plain ZIP (EOCD gone) -> Opaque"),
    ("hostile2-trunc-rtf", "poi-quick-contents-message.rtf", "truncate:1000", "truncated RTF (unbalanced braces) -> Opaque"),
    ("hostile2-trunc-bib", "bibtex-strings.bib", "truncate:800", "truncated BibTeX entry stream -> Opaque/other"),
    ("hostile2-magic-epub", "tika-test-documents-testEPUB.epub", "replace4:XXXX", "wrong-magic OCF/EPUB -> Opaque"),
    ("hostile2-trunc-xlsx", "poi-49609.xlsx", "truncate:15000", "truncated OPC/XLSX (zip) -> Opaque"),
    ("hostile2-trunc-pptx", "poi-Divino.pptx", "truncate:120000", "truncated OPC/PPTX (zip) -> Opaque"),
    ("hostile2-magic-json", "vega-data-burtin.json", "replace4:XXXX", "wrong-magic JSON -> Opaque"),
    ("hostile2-trunc-yaml", "prometheus-ci.yml", "truncate:4000", "truncated YAML stream -> Opaque/other"),
    ("hostile2-magic-toml", "toml-valid-implicit-and-explicit-before.toml", "replace4:XXXX", "wrong-magic TOML -> Opaque/other"),
    ("hostile2-trunc-geojson", "natural-earth-places.geojson", "truncate:2000", "truncated GeoJSON mid-structure -> Opaque"),
]


def apply_transform(data, transform):
    kind, arg = transform.split(":", 1)
    if kind == "truncate":
        return data[: int(arg)]
    if kind == "replace4":
        return arg.encode("ascii") + data[4:]
    if kind == "drop-last":
        return data[: len(data) - int(arg)]
    raise ValueError("unknown transform %r" % transform)


def read_tsv(path):
    rows = []
    with open(path) as f:
        hdr = f.readline().rstrip("\n").split("\t")
        for line in f:
            if not line.strip():
                continue
            vals = line.rstrip("\n").split("\t")
            if len(vals) != len(hdr):
                continue
            rows.append(dict(zip(hdr, vals)))
    return rows


def load_discovery(paths):
    rows = []
    for p in paths:
        try:
            rows.extend(read_tsv(p))
        except OSError as e:
            sys.stderr.write("WARN cannot read %s: %s\n" % (p, e))
    return rows


def parse_repo_dir(repo, ref, path):
    """The discovery `path` is repo-relative; return the last path component and
    the immediate parent directory (used to build a readable id)."""
    name = path.rsplit("/", 1)[-1]
    parent = path.rsplit("/", 2)[-2] if "/" in path else ""
    return name, parent


def sanitize(s):
    out = []
    for ch in s:
        out.append(ch if (ch.isalnum() or ch in ".-_") else "-")
    return "".join(out).strip("-")


def pick(rows, regex_rule, lo, hi, n):
    import re
    rx = re.compile(regex_rule)
    cand = [r for r in rows if rx.search(r["path"])]
    sel = []
    for r in cand:
        try:
            sz = int(r["size"])
        except (ValueError, KeyError):
            continue
        if lo <= sz <= hi:
            sel.append((sz, r["path"], r))
    sel.sort()
    if len(sel) <= n:
        return [s[2] for s in sel]
    # spread evenly across the (size-sorted) range
    out = []
    for i in range(n):
        idx = round(i * (len(sel) - 1) / (n - 1)) if n > 1 else 0
        out.append(sel[idx][2])
    # de-duplicate while preserving order
    seen = set()
    dedup = []
    for r in out:
        if r["path"] in seen:
            continue
        seen.add(r["path"])
        dedup.append(r)
    return dedup


def cmd_select(ns):
    rows = load_discovery(ns.discovery)
    by_repo = {}
    for r in rows:
        by_repo.setdefault((r["repo"], r["ref"]), []).append(r)
    print("id\tformat\turl\tsource\tlicense\tredistributable\tnotes\tcomplexity")
    used_ids = set()
    for (repo, ref, regex_rule, fmt, prefix, n, lo, hi, source, lic, redis, cx) in RULES:
        pool = by_repo.get((repo, ref), [])
        picked = pick(pool, regex_rule, lo, hi, n)
        for r in picked:
            name, parent = parse_repo_dir(repo, ref, r["path"])
            base = sanitize("%s-%s-%s" % (prefix, parent, name)) if parent else sanitize("%s-%s" % (prefix, name))
            base = base[:80]
            sid = base
            k = 2
            while sid in used_ids:
                sid = "%s-%d" % (base, k)
                k += 1
            used_ids.add(sid)
            print("%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s" % (
                sid, fmt, r["download_url"], source, lic, redis,
                sanitize(r["path"])[:120], cx))
    return 0


def cmd_merge(ns):
    # The v1 control rows first (re-declared where the v1 label was the coarse
    # physical encoding), then the new candidates. Both share the candidate schema
    # (id, format, url, source, license, redistributable, notes, complexity).
    print("id\tformat\turl\tsource\tlicense\tredistributable\tnotes\tcomplexity")
    for r in read_tsv(ns.old):
        fmt = FORMAT_FIXUP.get(r["id"], r["format"])
        print("\t".join([r["id"], fmt, r["url"], r["source"], r["license"],
                         r["redistributable"], r["notes"], r["complexity"]]))
    for r in read_tsv(ns.new):
        print("\t".join([r["id"], r["format"], r["url"], r["source"], r["license"],
                         r["redistributable"], r["notes"], r["complexity"]]))
    return 0


def cmd_hostile(ns):
    import hashlib as _h
    manifest = read_tsv(ns.manifest)
    by_id = {r["id"]: r for r in manifest}
    print("id\tbase_id\ttransform\tbytes\tsha256\tnotes")
    bad = 0
    for hid, base, transform, notes in HOSTILE_RECIPES:
        row = by_id.get(base)
        if row is None:
            sys.stderr.write("FAIL %s: base %s not in manifest\n" % (hid, base))
            bad += 1
            continue
        src = os.path.join(ns.corpus, "documents", row["format"], base)
        if not os.path.exists(src):
            sys.stderr.write("FAIL %s: base bytes absent: %s\n" % (hid, src))
            bad += 1
            continue
        raw = open(src, "rb").read()
        if len(raw) != int(row["bytes"]) or _h.sha256(raw).hexdigest() != row["sha256"]:
            sys.stderr.write("FAIL %s: base bytes mismatch: %s\n" % (hid, base))
            bad += 1
            continue
        derived = apply_transform(raw, transform)
        print("%s\t%s\t%s\t%d\t%s\t%s" % (
            hid, base, transform, len(derived), _h.sha256(derived).hexdigest(), notes))
    sys.stderr.write("hostile: %d ok, %d failed\n" % (len(HOSTILE_RECIPES) - bad, bad))
    return 1 if bad else 0


def cmd_build(ns):
    rows = read_tsv(ns.candidates)
    print("id\tformat\tbytes\tsha256\turl\tsource\tlicense\tredistributable\tnotes\tcomplexity")
    ok = bad = 0
    for r in rows:
        url = r["url"].replace(" ", "%20").replace("#", "%23")
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "vole-realcorpus/1.0"})
            with urllib.request.urlopen(req, timeout=180) as resp:
                data = resp.read()
        except Exception as e:  # noqa: BLE001
            sys.stderr.write("FAIL %s <- %s: %s\n" % (r["id"], url, e))
            bad += 1
            continue
        if not data:
            sys.stderr.write("FAIL %s: empty body\n" % r["id"])
            bad += 1
            continue
        print("%s\t%s\t%d\t%s\t%s\t%s\t%s\t%s\t%s\t%s" % (
            r["id"], r["format"], len(data), hashlib.sha256(data).hexdigest(),
            url, r["source"], r["license"], r["redistributable"], r["notes"], r["complexity"]))
        ok += 1
    sys.stderr.write("build-formats-v2: %d ok, %d failed\n" % (ok, bad))
    return 1 if bad else 0


def main(argv=None):
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    ps = sub.add_parser("select")
    ps.add_argument("--discovery", action="append", required=True)
    ps.set_defaults(func=cmd_select)
    pb = sub.add_parser("build")
    pb.add_argument("--candidates", required=True)
    pb.set_defaults(func=cmd_build)
    pm = sub.add_parser("merge")
    pm.add_argument("--old", required=True)
    pm.add_argument("--new", required=True)
    pm.set_defaults(func=cmd_merge)
    ph = sub.add_parser("hostile")
    ph.add_argument("--manifest", required=True)
    ph.add_argument("--corpus", default="realformats-v1")
    ph.set_defaults(func=cmd_hostile)
    ns = ap.parse_args(argv)
    return ns.func(ns)


if __name__ == "__main__":
    sys.exit(main())
