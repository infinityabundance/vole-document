#!/usr/bin/env python3
# Phase 21.20-21.26 (economic courts) — SHARED engine for the nine text-family
# courts (CONFIG / FEED / GEOJSON / GIS / NOTEBOOK / TABULAR / RST / ASCIIDOC /
# MDX). One module, parameterised by `--format`, so the courts cannot drift
# apart. Runs on Python stdlib only (the pinned `doc-baseline` service; no
# third-party module, no network) — except the `tabular` format's DuckDB lane,
# which runs in the `analytical` service (same base + hash-pinned DuckDB).
#
# Two conventional comparators per format (plus a third for `tabular` only):
#
#   * `sqlite` — a **source-retaining** baseline: it keeps the original bytes
#     verbatim (a `raw` BLOB, so it can answer `materialize` / byte authority, Q6)
#     AND a conventional extraction of the document (a parsed line/field/JSON view).
#     It preserves whatever the extraction preserves (duplicate keys where the
#     source text is retained, dialect, counts, values, kinds); it exposes **no**
#     exact source span, token spelling, attribute span, or native representation.
#   * `conv`   — a **conventional decode-to-host-values** load:
#     `configparser`-style for config, `xml.etree.ElementTree` for feed/gis,
#     `json` for geojson/notebook, a `csv`-module load (right delimiter) or a
#     manual fixed-width split for tabular, and a **line-based prose load**
#     (blank-line blocks, no typed structure) for rst/asciidoc/mdx. It keeps only a
#     derived host-value view: it drops the source bytes, every source offset, exact
#     spelling, duplicate keys, member/attribute order, adornments/markers, and the
#     recorded dialect/format, so it must decline typed on all of those.
#   * `duckdb` — the **DuckDB/Parquet analytical baseline** mandated by ADR-0059
#     for the tabular/analytical formats. Defined for the `tabular` format only:
#     it loads PSV with DuckDB's own CSV reader and fixed-width via DuckDB SQL,
#     persists a Parquet projection (ZSTD) and answers the columnar/tabular
#     questions with SQL. It keeps no exact source bytes/span, dialect or layout,
#     so those questions are typed declines, never silent answers.
#
#   plan      --format F
#   build     --format F --lane sqlite|conv|duckdb --source FILE --out DIR
#   query     --format F --lane ... --dir DIR --q Qn --plan JSON --out FILE
#   session   --format F --lane ... --dir DIR --queries Q1,... --plan JSON --out FILE
#   materialize --format F --lane sqlite --dir DIR --out FILE
#   version   --format F
#   aggregate --format F --raw DIR --campaign DIR --env ENV_JSON

import argparse
import csv
import hashlib
import io
import json
import math
import os
import re
import sys
import time
import xml.etree.ElementTree as ET

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)

FORMATS = ["config", "feed", "geojson", "gis", "notebook",
           "tabular", "rst", "asciidoc", "mdx",
           "mhtml", "logstream", "pkgmeta", "apispec", "bib", "rtf",
           "container", "hdf", "cfbf",
           "doc", "xls", "ppt", "msg"]

# Phase 21.27-21.35 (+27.4): the additional text/binary-document families. For
# these the per-format configuration is declared in the `..._NINE` tables below
# and the comparator/host-value logic lives in the `_new_*` section.
NINE = ["mhtml", "logstream", "pkgmeta", "apispec", "bib", "rtf",
        "container", "hdf", "cfbf",
        "doc", "xls", "ppt", "msg"]

# Lane C (the conventional decode-to-host-values load) has NO faithful Python-stdlib
# decoder for these formats, so it declines every question with the recorded code
# `comparator_absent`. `all` = the whole format; otherwise the fixture predicate is
# evaluated per fixture by `_conv_absent`. The four legacy CFBF-backed Office
# formats (27.4) have no faithful stdlib decoder at all, so their scope is `all`.
C_ABSENT_NOTE = {
    "cfbf": ("all", "CFBF/OLE2 has no faithful Python-stdlib decoder, so Lane C "
                     "declines every question as comparator_absent"),
    "hdf": ("hdf5", "HDF5 has no faithful Python-stdlib decoder (only the "
                     "NetCDF-classic part is decoded by Lane C), so Lane C "
                     "declines every HDF5-fixture question as comparator_absent"),
    "doc": ("all", "legacy Word 97–2003 (.doc, MS-DOC) has no faithful "
                    "Python-stdlib decoder, so Lane C declines every question as "
                    "comparator_absent"),
    "xls": ("all", "legacy Excel 97–2003 (.xls, BIFF8) has no faithful "
                    "Python-stdlib decoder, so Lane C declines every question as "
                    "comparator_absent"),
    "ppt": ("all", "legacy PowerPoint 97–2003 (.ppt, MS-PPT) has no faithful "
                    "Python-stdlib decoder, so Lane C declines every question as "
                    "comparator_absent"),
    "msg": ("all", "legacy Outlook message (.msg, MS-OXMSG) has no faithful "
                    "Python-stdlib decoder, so Lane C declines every question as "
                    "comparator_absent"),
}


def _conv_absent(fmt, name):
    spec = C_ABSENT_NOTE.get(fmt)
    if spec is None:
        return False
    kind = spec[0]
    if kind == "all":
        return True
    if kind == "hdf5":
        return name.startswith("hdf5_")
    return False


def sha256_hex(b):
    return hashlib.sha256(b).hexdigest()


def now_us():
    return int(time.monotonic() * 1_000_000)


def envelope(q, value=None, *, declined=False, code=None, reason="", detail=None):
    e = {"q": q, "lane": "?", "declined": bool(declined), "native": True,
         "value": None if declined else value, "detail": detail or {}}
    e["decline"] = {"code": code, "detail": reason} if declined else None
    return e


def decl(q, code, reason):
    return envelope(q, declined=True, code=code, reason=reason)


def canon_num(x):
    if isinstance(x, bool):
        return "true" if x else "false"
    if isinstance(x, float):
        if math.isnan(x):
            return "nan"
        if x.is_integer():
            return str(int(x))
        return repr(x)
    return str(x)


# ===========================================================================
# Per-format configuration: fixtures, controls, per-fixture plans, question text
# ===========================================================================

LANE_FIXTURES = {
    "config": ["ini-basic.ini", "ini-comments.ini", "ini-spaces.ini",
               "env-basic.env", "env-quotes.env", "props-basic.properties",
               "props-colon.properties", "dup.ini", "large.ini"],
    "feed": ["rss-basic.xml", "rss-attrs.xml", "rss-cdata.xml", "atom-basic.xml",
             "atom-multilink.xml", "dup-fields.xml", "large.xml"],
    "geojson": ["points.geojson", "feature.geojson", "coords.geojson",
                "foreign.geojson", "geomcoll.geojson", "dupkeys.geojson",
                "large.geojson"],
    "gis": ["placemarks.kml", "folder.kml", "dupfields.kml", "track.gpx",
            "routes.gpx", "large.kml"],
    "notebook": ["basic.ipynb", "string-src.ipynb", "lines-src.ipynb",
                 "outputs.ipynb", "raw.ipynb", "dupkeys.ipynb", "large.ipynb"],
    "tabular": ["psv-basic.psv", "psv-quotes.psv", "fw-basic.fw", "fw-three.fw",
                "fw-crlf.fw", "large.psv", "large.fw"],
    "rst": ["basic.rst", "explicit.rst", "lists.rst", "tables.rst",
            "inline.rst", "literal.rst", "large.rst"],
    "asciidoc": ["basic.adoc", "attributes.adoc", "delimited.adoc", "lists.adoc",
                 "tables.adoc", "inline.adoc", "large.adoc"],
    "mdx": ["esm.mdx", "jsx.mdx", "expr.mdx", "mixed.mdx", "surface.mdx",
            "large.mdx"],
    "mhtml": ["base.mhtml", "nostart.mhtml", "envelope.mhtml", "large.mhtml"],
    "logstream": ["generic.log", "rfc5424.log", "rfc3164.log", "crlf.log",
                  "blank.log", "large.log"],
    "pkgmeta": ["package.json", "Cargo.toml", "pyproject.toml", "Cargo.lock",
                "package-lock.json", "large.json"],
    "apispec": ["schema.json", "openapi.json", "swagger.json", "asyncapi.json",
                "large.json"],
    "bib": ["refs.bib", "refs.ris", "refs.json", "large.json"],
    "rtf": ["doc.rtf", "large.rtf"],
    "container": ["zip_stored.zip", "zip_multi.zip", "zip_nonutf8.zip",
                  "tar_ustar.tar", "tar_extended.tar"],
    "hdf": ["netcdf_cdf1.nc", "netcdf_cdf2.nc", "hdf5_basic.h5",
            "hdf5_userblock.h5"],
    "cfbf": ["cfbf_v3.cfb", "cfbf_v4.cfb"],
    "doc": ["doc_0table.doc", "doc_1table.doc", "doc_sections.doc"],
    "xls": ["xls_valid.xls"],
    "ppt": ["ppt_valid.ppt", "ppt_lists.ppt"],
    "msg": ["msg_valid.msg"],
}

# Controls -> the format class each MUST receive (checked at court time).
CONTROLS = {
    "config": {"strict.toml": "toml", "strict.json": "json",
               "overlap.env": "opaque", "prose.txt": "opaque",
               "script.sh": "opaque"},
    "feed": {"notfeed.xml": "xml", "strict.json": "json", "prose.txt": "opaque",
             "malformed.xml": "opaque"},
    "geojson": {"notgeojson.json": "json", "wrongtype.json": "json",
                "malformed.geojson": "opaque", "prose.txt": "opaque"},
    "gis": {"notgis.xml": "xml", "strict.json": "json", "prose.txt": "opaque",
            "malformed.kml": "opaque"},
    "notebook": {"notnotebook.json": "json", "missing-nbformat.json": "json",
                 "malformed.ipynb": "opaque", "prose.txt": "opaque"},
    "tabular": {"comma.csv": "csv", "markdown.md": "markdown",
                "prose.txt": "opaque", "spaced.txt": "opaque"},
    "rst": {"prose.txt": "opaque", "markdown.md": "markdown"},
    "asciidoc": {"prose.txt": "opaque", "markdown.md": "markdown",
                 "rest.rst": "rst", "spaced.adoc": "rst"},
    "mdx": {"prose.txt": "opaque", "markdown.md": "markdown",
            "markdown_code.md": "markdown", "inline_expr.md": "markdown",
            "html.html": "html", "braces.txt": "opaque"},
    "mhtml": {"prose.txt": "opaque", "plain.html": "html",
              "plain.eml": "eml"},
    "logstream": {"prose.txt": "opaque", "single.json": "json",
                  "stream.jsonl": "jsonl", "bsd_nocolon.log": "yaml"},
    "pkgmeta": {"generic.json": "json", "generic.toml": "toml",
                "prose.txt": "opaque"},
    "apispec": {"generic.json": "json", "shape.json": "json",
                "prose.txt": "opaque"},
    "bib": {"array.json": "json", "prose.txt": "opaque"},
    "rtf": {"junk.txt": "opaque", "prose.txt": "opaque",
            "rtfx.rtf": "opaque", "broken.rtf": "opaque"},
    "container": {"office.docx": "docx@20", "office.odt": "odt@20",
                  "pk_junk.bin": "opaque", "prose.txt": "opaque",
                  "bad_eocd.zip": "opaque", "bad_checksum.tar": "opaque",
                  "truncated.zip": "opaque"},
    "hdf": {"prose.txt": "opaque", "hdf5_badmagic.bin": "opaque",
            "hdf5_truncated.h5": "opaque", "hdf5_junk.bin": "opaque"},
    "cfbf": {"prose.txt": "opaque", "sig_only.bin": "opaque",
             "cfbf_badsig.cfb": "opaque", "cfbf_badshift.cfb": "opaque",
             "cfbf_bad_dir_sector.cfb": "opaque"},
    "doc": {"generic_cfbf.bin": "cfbf", "doc_bad_ident.doc": "cfbf",
            "doc_bad_nfib.doc": "cfbf", "prose.txt": "opaque"},
    "xls": {"generic_cfbf.bin": "cfbf", "xls_bad_bof.xls": "cfbf",
            "xls_bad_type.xls": "cfbf", "prose.txt": "opaque"},
    "ppt": {"generic_cfbf.bin": "cfbf", "ppt_bad_first_record.ppt": "cfbf",
            "prose.txt": "opaque"},
    "msg": {"generic_cfbf.bin": "cfbf", "msg_missing_props.msg": "cfbf",
            "prose.txt": "opaque"},
}

# The per-format plan (FROZEN). Slots are named per format; see the qdescs.
PLANS = {
    "config": {
        "ini-basic.ini": {"value_entry": 1, "span_entry": 0, "kind_line": 1,
                          "dup_entry": 0, "section": "db", "token_entry": 1,
                          "find_pat": "5432", "comment_line": 0},
        "ini-comments.ini": {"value_entry": 1, "span_entry": 0, "kind_line": 1,
                             "dup_entry": 0, "section": "s", "token_entry": 0,
                             "find_pat": "w", "comment_line": 2},
        "ini-spaces.ini": {"value_entry": 0, "span_entry": 1, "kind_line": 1,
                           "dup_entry": 0, "section": "ab", "token_entry": 1,
                           "find_pat": "spaces", "comment_line": 0},
        "env-basic.env": {"value_entry": 0, "span_entry": 1, "kind_line": 1,
                          "dup_entry": 0, "section": None, "token_entry": 2,
                          "find_pat": "bar", "comment_line": 0},
        "env-quotes.env": {"value_entry": 2, "span_entry": 1, "kind_line": 1,
                           "dup_entry": 0, "section": None, "token_entry": 0,
                           "find_pat": "plain", "comment_line": 0},
        "props-basic.properties": {"value_entry": 0, "span_entry": 1,
                                   "kind_line": 1, "dup_entry": 0, "section": None,
                                   "token_entry": 2, "find_pat": "v",
                                   "comment_line": 0},
        "props-colon.properties": {"value_entry": 2, "span_entry": 3,
                                   "kind_line": 1, "dup_entry": 0, "section": None,
                                   "token_entry": 4, "find_pat": "3",
                                   "comment_line": 0},
        "dup.ini": {"value_entry": 3, "span_entry": 1, "kind_line": 1,
                    "dup_entry": 0, "section": "a", "token_entry": 0,
                    "find_pat": "9", "comment_line": 0},
        "large.ini": {"value_entry": 5, "span_entry": 0, "kind_line": 0,
                      "dup_entry": 5, "section": "main", "token_entry": 5,
                      "find_pat": "value_10", "comment_line": 0},
    },
    "feed": {
        "rss-basic.xml": {"entry": 0, "field": "title", "dup_field": "title",
                          "cfield": "title", "find_pat": "First"},
        "rss-attrs.xml": {"entry": 0, "field": "guid", "dup_field": "category",
                          "cfield": "description", "find_pat": "alpha"},
        "rss-cdata.xml": {"entry": 0, "field": "title", "dup_field": "title",
                          "cfield": "description", "find_pat": "Plain"},
        "atom-basic.xml": {"entry": 0, "field": "title", "dup_field": "title",
                           "cfield": "link", "find_pat": "Solo"},
        "atom-multilink.xml": {"entry": 0, "field": "title", "dup_field": "title",
                               "cfield": "title", "find_pat": "E1"},
        "dup-fields.xml": {"entry": 0, "field": "title", "dup_field": "category",
                           "cfield": "title", "find_pat": "b"},
        "large.xml": {"entry": 5, "field": "title", "dup_field": "title",
                      "cfield": "title", "find_pat": "item-5"},
    },
    "geojson": {
        "points.geojson": {"feature": 0, "geometry": 0, "prop": "value",
                           "key": "a", "find_pat": "name"},
        "feature.geojson": {"feature": 0, "geometry": 0, "prop": "name",
                            "key": "name", "find_pat": "solo"},
        "coords.geojson": {"feature": 0, "geometry": 0, "prop": "n",
                           "key": "n", "find_pat": "n"},
        "foreign.geojson": {"feature": 0, "geometry": 0, "prop": "name",
                            "key": "name", "find_pat": "F"},
        "geomcoll.geojson": {"feature": 0, "geometry": 0, "prop": "name",
                             "key": "name", "find_pat": "G"},
        "dupkeys.geojson": {"feature": 0, "geometry": 0, "prop": "z",
                            "key": "k", "find_pat": "k"},
        "large.geojson": {"feature": 5, "geometry": 5, "prop": "name",
                          "key": "name", "find_pat": "item-5"},
    },
    "gis": {
        "placemarks.kml": {"record": 0, "record_field": "name", "point": 0,
                           "field": "name", "find_pat": "P1"},
        "folder.kml": {"record": 0, "record_field": "name", "point": 0,
                       "field": "name", "find_pat": "N1"},
        "dupfields.kml": {"record": 0, "record_field": "name", "point": 0,
                          "field": "name", "find_pat": "first"},
        "track.gpx": {"record": 0, "record_field": "name", "point": 0,
                      "field": "name", "find_pat": "W1"},
        "routes.gpx": {"record": 0, "record_field": "name", "point": 0,
                       "field": "name", "find_pat": "R1"},
        "large.kml": {"record": 5, "record_field": "name", "point": 5,
                      "field": "name", "find_pat": "p-5"},
    },
    "notebook": {
        "basic.ipynb": {"cell": 0, "out_cell": 1, "out_idx": 0, "find_pat": "hello"},
        "string-src.ipynb": {"cell": 0, "out_cell": 0, "out_idx": 0,
                             "find_pat": "Heading"},
        "lines-src.ipynb": {"cell": 0, "out_cell": 0, "out_idx": 0,
                            "find_pat": "a + 1"},
        "outputs.ipynb": {"cell": 0, "out_cell": 0, "out_idx": 0,
                          "find_pat": "warn"},
        "raw.ipynb": {"cell": 0, "out_cell": 0, "out_idx": 0, "find_pat": "raw"},
        "dupkeys.ipynb": {"cell": 0, "out_cell": 0, "out_idx": 0, "find_pat": "dup"},
        "large.ipynb": {"cell": 5, "out_cell": 0, "out_idx": 0, "find_pat": "v5"},
    },
    "tabular": {
        "psv-basic.psv": {"kind": "csv", "row": 1, "col": 1, "find_pat": "a"},
        "psv-quotes.psv": {"kind": "csv", "row": 1, "col": 0, "find_pat": "x"},
        "fw-basic.fw": {"kind": "fixedwidth", "row": 1, "col": 1, "find_pat": "A"},
        "fw-three.fw": {"kind": "fixedwidth", "row": 1, "col": 2, "find_pat": "B"},
        "fw-crlf.fw": {"kind": "fixedwidth", "row": 1, "col": 0,
                       "find_pat": "Alice"},
        "large.psv": {"kind": "csv", "row": 1, "col": 1, "find_pat": "name1"},
        "large.fw": {"kind": "fixedwidth", "row": 1, "col": 1,
                     "find_pat": "b1"},
    },
    "rst": {
        "basic.rst": {"heading": 1, "block": 0, "find_pat": "body"},
        "explicit.rst": {"heading": 0, "block": 0, "find_pat": "text."},
        "lists.rst": {"heading": 0, "block": 0, "find_pat": "closing"},
        "tables.rst": {"heading": 0, "block": 0, "find_pat": "closing"},
        "inline.rst": {"heading": 0, "block": 0, "find_pat": "strong"},
        "literal.rst": {"heading": 0, "block": 0, "find_pat": "paragraph"},
        "large.rst": {"heading": 1, "block": 0, "find_pat": "number 7 carries"},
    },
    "asciidoc": {
        "basic.adoc": {"heading": 1, "block": 0, "find_pat": "body"},
        "attributes.adoc": {"heading": 0, "block": 0, "find_pat": "literal"},
        "delimited.adoc": {"heading": 0, "block": 0, "find_pat": "closing"},
        "lists.adoc": {"heading": 0, "block": 0, "find_pat": "closing"},
        "tables.adoc": {"heading": 0, "block": 0, "find_pat": "closing"},
        "inline.adoc": {"heading": 0, "block": 0, "find_pat": "strong"},
        "large.adoc": {"heading": 1, "block": 0, "find_pat": "numbered 7 "},
    },
    "mdx": {
        "esm.mdx": {"heading": 0, "block": 0, "find_pat": "paragraph"},
        "jsx.mdx": {"heading": 0, "block": 0, "find_pat": "closing"},
        "expr.mdx": {"heading": 0, "block": 0, "find_pat": "inline"},
        "mixed.mdx": {"heading": 0, "block": 0, "find_pat": "before"},
        "surface.mdx": {"heading": 0, "block": 0, "find_pat": "emphasis"},
        "large.mdx": {"heading": 1, "block": 0, "find_pat": "numbered 7 "},
    },
    "mhtml": {
        "base.mhtml": {"res": 0, "find": "stylesheet"},
        "nostart.mhtml": {"res": 0, "find": "plain"},
        "envelope.mhtml": {"res": None, "find": "env"},
        "large.mhtml": {"res": 0, "find": "line 3999"},
    },
    "logstream": {
        "generic.log": {"rec": 0, "role": "msg", "find": "starting"},
        "rfc5424.log": {"rec": 0, "role": "msg", "find": "hello"},
        "rfc3164.log": {"rec": 0, "role": "msg", "find": "su root"},
        "crlf.log": {"rec": 0, "role": "msg", "find": "tail"},
        "blank.log": {"rec": 0, "role": "msg", "find": "first"},
        "large.log": {"rec": 0, "role": "msg", "find": "payload"},
    },
    "pkgmeta": {
        "package.json": {"section": 2, "sname": "dependencies", "entry": 6,
                         "find": "demo"},
        "Cargo.toml": {"section": 1, "sname": "package", "entry": 2,
                       "find": "serde"},
        "pyproject.toml": {"section": 3, "sname": "project", "entry": 4,
                           "find": "demo"},
        "Cargo.lock": {"section": 1, "sname": "package", "entry": 2,
                       "find": "demo"},
        "package-lock.json": {"section": 1, "sname": "packages", "entry": 4,
                              "find": "demo"},
        "large.json": {"section": 1, "sname": "dependencies", "entry": 3,
                       "find": "demo"},
    },
    "apispec": {
        "schema.json": {"object": 0, "ref": 0, "find": "Id"},
        "openapi.json": {"object": 0, "ref": 0, "find": "Demo"},
        "swagger.json": {"object": 0, "ref": None, "find": "Demo"},
        "asyncapi.json": {"object": 0, "ref": None, "find": "Demo"},
        "large.json": {"object": 0, "ref": None, "find": "Demo"},
    },
    "bib": {
        "refs.bib": {"entry": 0, "field": "title", "key": "smith2020",
                     "find": "Study"},
        "refs.ris": {"entry": 0, "field": "TI", "key": "",
                     "find": "Study"},
        "refs.json": {"entry": 0, "field": "title", "key": "doe2019",
                      "find": "Study"},
        "large.json": {"entry": 0, "field": "title", "key": "item000000",
                       "find": "Study"},
    },
    "rtf": {
        "doc.rtf": {"group": 1, "control": 0, "hex": 0, "unicode": 0,
                    "dest": "fonttbl", "text": 0, "find": "bold"},
        "large.rtf": {"group": 1, "control": 0, "hex": 0, "unicode": 0,
                      "dest": "fonttbl", "text": 0, "find": "bold"},
    },
    "container": {
        "zip_stored.zip": {"entry": 0, "find": "hello"},
        "zip_multi.zip": {"entry": 0, "find": "alpha"},
        "zip_nonutf8.zip": {"entry": 0, "find": "caf"},
        "tar_ustar.tar": {"entry": 0, "find": "nested"},
        "tar_extended.tar": {"entry": 0, "find": "overridden"},
    },
    "hdf": {
        "netcdf_cdf1.nc": {"object": 2, "dim": 0, "attr": 0, "find": "temp"},
        "netcdf_cdf2.nc": {"object": 2, "dim": 0, "attr": 0, "find": "temp"},
        "hdf5_basic.h5": {"object": 1, "dim": None, "attr": 0, "find": "dset"},
        "hdf5_userblock.h5": {"object": 1, "dim": None, "attr": 0,
                              "find": "dset"},
    },
    "cfbf": {
        "cfbf_v3.cfb": {"entry": 2, "stream": 2, "find": "Large"},
        "cfbf_v4.cfb": {"entry": 2, "stream": 2, "find": "caf"},
    },
    "doc": {
        "doc_0table.doc": {"piece": 0, "section": "main", "find": "Hello"},
        "doc_1table.doc": {"piece": 1, "section": "footnote", "find": "Hello"},
        "doc_sections.doc": {"piece": 0, "section": "header", "find": "AAAB"},
    },
    "xls": {
        "xls_valid.xls": {"sheet": 0, "cell": 0, "record": 1, "find": "Alpha"},
    },
    "ppt": {
        "ppt_valid.ppt": {"slide": 0, "run": 0, "record": 0, "list": "slides",
                          "find": "Hello"},
        "ppt_lists.ppt": {"slide": 0, "run": 1, "record": 1, "list": "masters",
                          "find": "Caf"},
    },
    "msg": {
        "msg_valid.msg": {"property": 1, "recipient": 0, "attachment": 0,
                          "find": "Hello"},
    },
}

# Question descriptions per format: (VOLE, sqlite, conv).
QDESC = {
    "config": {
        "Q1": ("a decoded entry value", "extracted `key=value` (value)", "host value"),
        "Q2": ("an entry's exact source span", "no source span -> typed decline",
               "no source span -> typed decline"),
        "Q3": ("a line's kind (section/entry/comment/blank)",
               "extracted line kind", "configparser has no line kinds -> decline"),
        "Q4": ("the duplicate-key count for an entry (same_key_entries)",
               "enumerated entries (duplicates preserved)", "duplicates collapsed -> decline"),
        "Q5": ("a section header (name + header count)",
               "section names (duplicates collapsed by name)", "section names"),
        "Q7": ("`config-find` over decoded keys/values (with spans)",
               "scan over extracted keys/values (no spans)", "scan over host values (no spans)"),
        "Q8": ("the exact raw line token bytes", "no source token -> typed decline",
               "no source token -> typed decline"),
        "Q9": ("a comment line's kind + marker", "extracted comment markers",
               "comments dropped -> typed decline"),
        "Q10": ("key/separator exact spans + marker",
                "no source span -> typed decline", "no source span -> typed decline"),
        "Q11": ("the recorded dialect (ini/env/properties)", "stored dialect",
                "dialect not recorded -> typed decline"),
        "Q12": ("quoting/export/continuation spelling flags",
                "no source spelling -> typed decline", "no source spelling -> typed decline"),
    },
    "feed": {
        "Q1": ("a decoded entry-field value", "extracted field text", "host value"),
        "Q2": ("an entry-field element's exact source span",
               "no source span -> typed decline", "no source span -> typed decline"),
        "Q3": ("a target field's local name", "extracted field name", "host tag name"),
        "Q4": ("the count of same-named fields in a record",
               "count of extracted same-named fields", "count of same-tag children"),
        "Q5": ("a record descriptor (index + ordered field names)",
               "extracted record (ordered fields)", "host record"),
        "Q7": ("`feed-find` over decoded field values (with spans)",
               "scan over extracted field text (no spans)", "host-structure walk (no spans)"),
        "Q8": ("a channel field's exact element bytes", "no source token -> decline",
               "no source token -> decline"),
        "Q9": ("a field element's attributes (order + spelling + spans)",
               "no attribute spelling -> typed decline", "no attribute spelling -> decline"),
        "Q10": ("channel field names in document order",
                "extracted channel field names", "host child names"),
        "Q11": ("the recorded dialect (rss/atom)", "stored dialect",
                "dialect not recorded -> typed decline"),
        "Q12": ("an entry's ordered field spans", "no source span -> typed decline",
                "no source span -> typed decline"),
    },
    "geojson": {
        "Q1": ("a `properties` scalar (kind + canonical value)",
               "`json_extract` value (normalized)", "host value"),
        "Q2": ("a `properties` value's exact source span",
               "no source span -> typed decline", "no source span -> typed decline"),
        "Q3": ("a geometry's `type`", "`json_extract` type", "host type"),
        "Q4": ("the duplicate-key count in `properties`",
               "`json_tree` duplicate enumeration", "duplicate keys collapsed -> decline"),
        "Q5": ("a feature descriptor (type + foreign member keys)",
               "normalized feature view", "host feature view"),
        "Q7": ("`geojson-find` over keys/strings (with spans)",
               "scan over keys/strings (no spans)", "host-structure walk (no spans)"),
        "Q8": ("a property value's exact token bytes", "re-serialized -> typed decline",
               "host value -> typed decline"),
        "Q9": ("each coordinate number's exact spelling",
               "normalized numbers -> typed decline", "host numbers -> typed decline"),
        "Q10": ("a feature's foreign (non-core) member keys",
                "`json_each` member keys", "host member keys"),
        "Q11": ("the root `type` (Feature/FeatureCollection)",
                "`json_extract` type", "host type"),
        "Q12": ("a geometry's exact coordinate token bytes",
                "no source token -> typed decline", "no source token -> typed decline"),
    },
    "gis": {
        "Q1": ("a decoded record-field value", "extracted field text", "host value"),
        "Q2": ("a record-field element's exact source span",
               "no source span -> typed decline", "no source span -> typed decline"),
        "Q3": ("a point's kind (Placemark/Point/wpt/trkpt/rtept)",
               "extracted point kind", "host point kind"),
        "Q4": ("the count of same-named fields in a record",
               "count of extracted same-named fields", "count of same-tag children"),
        "Q5": ("a record descriptor (kind + ordered field names)",
               "extracted record", "host record"),
        "Q7": ("`gis-find` over decoded field values (with spans)",
               "scan over extracted field text (no spans)", "host-structure walk (no spans)"),
        "Q8": ("a point's exact element bytes", "no source token -> typed decline",
               "no source token -> typed decline"),
        "Q9": ("a point's attributes (lat/lon spelling + spans)",
               "no attribute spelling -> typed decline",
               "no attribute spelling -> typed decline"),
        "Q10": ("a point's ordered field names", "extracted point field names",
                "host point child names"),
        "Q11": ("the recorded dialect (kml/gpx)", "stored dialect",
                "dialect not recorded -> typed decline"),
        "Q12": ("a point's ordered fields (name + text)",
                "extracted point fields", "host point fields"),
    },
    "notebook": {
        "Q1": ("a cell's `cell_type`", "`json_extract` cell_type", "host cell_type"),
        "Q2": ("a cell's source span", "no source span -> typed decline",
               "no source span -> typed decline"),
        "Q3": ("a cell source's form (string vs line array)",
               "`json_type` of `source`", "host form"),
        "Q4": ("derived counts (cells + outputs)", "`json_array_length` counts",
               "host counts"),
        "Q5": ("a cell descriptor (type + source form + counts)",
               "normalized cell view", "host cell view"),
        "Q7": ("`notebook-find` over keys/strings (with spans)",
               "scan over keys/strings (no spans)", "host-structure walk (no spans)"),
        "Q8": ("a cell source's exact token bytes", "re-serialized -> typed decline",
               "host value -> typed decline"),
        "Q9": ("an output descriptor (output_type + name + text form)",
               "`json_extract` output view", "host output view"),
        "Q10": ("a cell source's element count + byte length",
                "`json_array_length` + length", "host element count + length"),
        "Q11": ("the `nbformat` major", "`json_extract` nbformat", "host nbformat"),
        "Q12": ("the `nbformat_minor`", "`json_extract` nbformat_minor",
                "host nbformat_minor"),
    },
    "tabular": {
        "Q1": ("a decoded cell's text", "extracted cell text", "host cell text",
               "projected cell text (SQL)"),
        "Q2": ("a cell's exact source span", "no source span -> typed decline",
               "no source span -> typed decline", "no source span -> typed decline"),
        "Q3": ("a record's column count", "extracted column count", "host column count",
               "projected column count"),
        "Q4": ("the table's record count", "extracted row count", "host row count",
               "projected row count"),
        "Q5": ("a record descriptor (ordered cell texts)", "extracted record",
               "host record", "projected record (SQL projection)"),
        "Q7": ("`tabular-find` over cell text (with spans)",
               "scan over extracted cells (no spans)", "scan over host cells (no spans)",
               "scan over projected cells (no spans)"),
        "Q8": ("a cell's exact padded/raw bytes", "re-decoded -> typed decline",
               "host value -> typed decline", "no source bytes -> typed decline"),
        "Q9": ("a record's exact content bytes", "no source bytes -> typed decline",
               "host value -> typed decline", "no source bytes -> typed decline"),
        "Q10": ("the header row's cell names (ordered)", "extracted header names",
                "host header names", "projected header names"),
        "Q11": ("the recorded dialect (pipe vs fixedwidth)", "stored dialect",
                "dialect not recorded -> typed decline",
                "dialect not recorded -> typed decline"),
        "Q12": ("the recovered column layout / per-cell quoting spelling",
                "no layout/spelling -> typed decline",
                "no layout/spelling -> typed decline",
                "no layout/spelling -> typed decline"),
    },
    "rst": {
        "Q1": ("the whole canonical document text", "line-based re-read",
               "line-based prose load"),
        "Q2": ("a section title's exact source span", "no source span -> typed decline",
               "no source span -> typed decline"),
        "Q3": ("the section-title count (titles)", "stored title count",
               "no typed structure -> typed decline"),
        "Q4": ("the list-item count", "no typed list model -> typed decline",
               "no typed list model -> typed decline"),
        "Q5": ("the first block's content text", "extracted first block",
               "line-based first block"),
        "Q7": ("`rst-find` over block content (with spans)",
               "scan over extracted blocks (no spans)",
               "scan over prose blocks (no spans)"),
        "Q8": ("a block's exact source bytes", "no source bytes -> typed decline",
               "no source bytes -> typed decline"),
        "Q9": ("a title's exact adornment", "adornment stripped -> typed decline",
               "adornment stripped -> typed decline"),
        "Q10": ("the directive count", "stored directive count",
                "no typed structure -> typed decline"),
        "Q11": ("the recorded format (rst)", "stored format",
                "format not recorded -> typed decline"),
        "Q12": ("the inline-span count", "no inline model -> typed decline",
                "no inline model -> typed decline"),
    },
    "asciidoc": {
        "Q1": ("the whole canonical document text", "line-based re-read",
               "line-based prose load"),
        "Q2": ("a section heading's exact source span", "no source span -> typed decline",
               "no source span -> typed decline"),
        "Q3": ("the section count", "stored section count",
               "no typed structure -> typed decline"),
        "Q4": ("the list-item count", "no typed list model -> typed decline",
               "no typed list model -> typed decline"),
        "Q5": ("the first block's content text", "extracted first block",
               "line-based first block"),
        "Q7": ("`adoc-find` over block content (with spans)",
               "scan over extracted blocks (no spans)",
               "scan over prose blocks (no spans)"),
        "Q8": ("a block's exact source bytes", "no source bytes -> typed decline",
               "no source bytes -> typed decline"),
        "Q9": ("a heading's exact `=` marker (document-title flag)",
               "marker stripped -> typed decline", "marker stripped -> typed decline"),
        "Q10": ("the document-attribute count", "stored attribute count",
                "no typed structure -> typed decline"),
        "Q11": ("the recorded format (asciidoc)", "stored format",
                "format not recorded -> typed decline"),
        "Q12": ("the inline-span count", "no inline model -> typed decline",
                "no inline model -> typed decline"),
    },
    "mdx": {
        "Q1": ("the whole canonical document text", "line-based re-read",
               "line-based prose load"),
        "Q2": ("a heading's exact source span", "no source span -> typed decline",
               "no source span -> typed decline"),
        "Q3": ("the ATX heading count", "stored heading count",
               "no typed structure -> typed decline"),
        "Q4": ("the list-item count", "no typed list model -> typed decline",
               "no typed list model -> typed decline"),
        "Q5": ("the first block's content text", "extracted first block",
               "line-based first block"),
        "Q7": ("`mdx-find` over block content (with spans)",
               "scan over extracted blocks (no spans)",
               "scan over prose blocks (no spans)"),
        "Q8": ("a block's exact source bytes", "no source bytes -> typed decline",
               "no source bytes -> typed decline"),
        "Q9": ("a heading's ATX level", "level not derived -> typed decline",
               "level not derived -> typed decline"),
        "Q10": ("the ESM statement count", "stored ESM count",
                "no typed structure -> typed decline"),
        "Q11": ("the recorded format (mdx)", "stored format",
                "format not recorded -> typed decline"),
        "Q12": ("the inline-span count", "no inline model -> typed decline",
                "no inline model -> typed decline"),
    },
    "mhtml": {
        "Q1": ("the decoded root-part HTML text", "line-based re-read of the root",
               "decoded root body (email+quopri/base64)"),
        "Q2": ("the root part's exact entity span", "no source span -> typed decline",
               "no source span -> typed decline"),
        "Q3": ("the root part's media type", "extracted root media type",
               "host root media type"),
        "Q4": ("the MIME header-field count", "extracted header count",
               "host header count"),
        "Q5": ("the envelope locations (Snapshot-Content-Location + Content-Base)",
               "extracted envelope headers", "host envelope headers"),
        "Q7": ("`mhtml-find` over root text + headers (with part/span)",
               "no source span -> typed decline", "no source span -> typed decline"),
        "Q8": ("the root part's exact decoded byte hash",
               "re-decoded -> typed decline", "host value -> typed decline"),
        "Q9": ("a sub-resource's media type", "extracted sub-resource type",
               "host sub-resource type"),
        "Q10": ("a sub-resource's transfer-encoding spelling",
                "no source spelling -> typed decline",
                "no source spelling -> typed decline"),
        "Q11": ("the recorded format (mhtml)", "stored format",
                "format not recorded -> typed decline"),
        "Q12": ("the root part's exact body span", "no source span -> typed decline",
                "no source span -> typed decline"),
    },
    "logstream": {
        "Q1": ("a record's decoded message text", "line-based re-read",
               "line-based record load (generic dialect only)"),
        "Q2": ("a record's exact source span", "no source span -> typed decline",
               "no source span -> typed decline"),
        "Q3": ("a record's dialect (generic/rfc5424/rfc3164)", "stored dialect",
               "detected dialect"),
        "Q4": ("the record count", "stored record count", "non-blank line count"),
        "Q5": ("a record's ordered field roles", "stored field roles",
               "no field-role model -> typed decline"),
        "Q7": ("`logstream-find` over record text (with spans)",
               "no source span -> typed decline", "no source span -> typed decline"),
        "Q8": ("a field's exact bytes", "no source token -> typed decline",
               "no source token -> typed decline"),
        "Q9": ("a record's line terminator (lf/crlf)", "stored terminator",
               "line terminator"),
        "Q10": ("a record's byte length", "stored line length", "line length"),
        "Q11": ("the recorded format (logstream)", "stored format",
                "format not recorded -> typed decline"),
        "Q12": ("a record's exact raw bytes", "no source bytes -> typed decline",
                "no source bytes -> typed decline"),
    },
    "pkgmeta": {
        "Q1": ("a section's name", "extracted section name",
               "host section name"),
        "Q2": ("a section's exact source span", "no source span -> typed decline",
               "no source span -> typed decline"),
        "Q3": ("a section's role", "extracted section role", "host section role"),
        "Q4": ("a section's kind (root/table/array)", "extracted section kind",
               "host section kind"),
        "Q5": ("a section descriptor (name+role+kind)", "extracted descriptor",
               "host descriptor"),
        "Q7": ("`pkgmeta-find` over keys/values (with spans)",
               "no source span -> typed decline", "no source span -> typed decline"),
        "Q8": ("a section's entry count", "extracted entry count",
               "host entry count"),
        "Q9": ("the recorded dialect (npm/cargo/pyproject/lock)", "stored dialect",
               "dialect not recorded -> typed decline"),
        "Q10": ("a section's exact name span", "no source span -> typed decline",
                "no source span -> typed decline"),
        "Q11": ("the recorded format (pkgmeta)", "stored format",
                "format not recorded -> typed decline"),
        "Q12": ("a section entry's exact bytes", "no source bytes -> typed decline",
                "no source bytes -> typed decline"),
    },
    "apispec": {
        "Q1": ("the recorded version string", "extracted version", "host version"),
        "Q2": ("an object's exact source span", "no source span -> typed decline",
               "no source span -> typed decline"),
        "Q3": ("the recorded dialect (schema/openapi/swagger/asyncapi)",
               "stored dialect", "dialect not recorded -> typed decline"),
        "Q4": ("an object descriptor (role+member count)", "extracted descriptor",
               "host descriptor"),
        "Q5": ("a `$ref` target string", "extracted ref target", "host ref target"),
        "Q7": ("`apispec-find` over pointers/values (with spans)",
               "no source span -> typed decline", "no source span -> typed decline"),
        "Q8": ("a `$ref` value's exact bytes", "no source token -> typed decline",
               "no source token -> typed decline"),
        "Q9": ("the `$ref` count", "extracted ref count", "counted `$ref` keys"),
        "Q10": ("an object's role", "extracted role", "host role"),
        "Q11": ("the recorded format (apispec)", "stored format",
                "format not recorded -> typed decline"),
        "Q12": ("the version string's exact bytes", "no source token -> typed decline",
                "no source token -> typed decline"),
    },
    "bib": {
        "Q1": ("a field's decoded value", "extracted field value",
               "host field value"),
        "Q2": ("an entry's exact source span", "no source span -> typed decline",
               "no source span -> typed decline"),
        "Q3": ("the recorded dialect (bibtex/ris/csl_json)", "stored dialect",
               "dialect not recorded -> typed decline"),
        "Q4": ("an entry's duplicate-field count", "extracted duplicates",
               "counted duplicate fields"),
        "Q5": ("an entry descriptor (key/id + type)", "extracted descriptor",
               "host descriptor"),
        "Q7": ("`bib-find` over field values (with spans)",
               "no source span -> typed decline", "no source span -> typed decline"),
        "Q8": ("a field's exact value bytes", "no source token -> typed decline",
               "no source token -> typed decline"),
        "Q9": ("the record count", "extracted record count", "counted records"),
        "Q10": ("the total field count", "extracted field count", "counted fields"),
        "Q11": ("the recorded format (bib)", "stored format",
                "format not recorded -> typed decline"),
        "Q12": ("a field's exact source span", "no source span -> typed decline",
                "no source span -> typed decline"),
    },
    "rtf": {
        "Q1": ("the header control text", "extracted header text",
               "host header text"),
        "Q2": ("a group's exact source span", "no source span -> typed decline",
               "no source span -> typed decline"),
        "Q3": ("the RTF version", "extracted version", "host version"),
        "Q4": ("the fonttbl destination count", "extracted fonttbl count",
               "counted fonttbl destinations"),
        "Q5": ("the first control word's name", "extracted control name",
               "host control name"),
        "Q7": ("`rtf-find` over text runs (with spans)",
               "no source span -> typed decline", "no source span -> typed decline"),
        "Q8": ("a control word's exact bytes", "no source token -> typed decline",
               "no source token -> typed decline"),
        "Q9": ("the hex-escape (`\\'hh`) count", "extracted hex count",
               "counted hex escapes"),
        "Q10": ("the first unicode escape's code point", "extracted code",
                "host code"),
        "Q11": ("the recorded format (rtf)", "stored format",
                "format not recorded -> typed decline"),
        "Q12": ("a control word's exact span", "no source span -> typed decline",
                "no source span -> typed decline"),
    },
    "container": {
        "Q1": ("a member's raw name", "extracted member name",
               "host member name"),
        "Q2": ("a member's exact source span", "no source span -> typed decline",
               "no source span -> typed decline"),
        "Q3": ("the recorded dialect (zip/tar)", "stored dialect",
               "host dialect"),
        "Q4": ("the member count", "extracted member count", "host member count"),
        "Q5": ("a member descriptor (name+kind+method+sizes)",
               "extracted descriptor", "host descriptor"),
        "Q7": ("`container-find` over names (with spans)",
               "no source span -> typed decline", "no source span -> typed decline"),
        "Q8": ("a member's exact bytes", "no source bytes -> typed decline",
               "no source bytes -> typed decline"),
        "Q9": ("a member's CRC-32", "extracted CRC-32", "computed CRC-32"),
        "Q10": ("a member's compression method", "extracted method",
                "host method"),
        "Q11": ("the recorded format (container)", "stored format",
                "format not recorded -> typed decline"),
        "Q12": ("a member's exact data span", "no source span -> typed decline",
                "no source span -> typed decline"),
    },
    "hdf": {
        "Q1": ("an object's path", "extracted object path",
               "host object path (NetCDF only)"),
        "Q2": ("an object's exact source span", "no source span -> typed decline",
               "no source span -> typed decline"),
        "Q3": ("the recorded dialect (netcdf/hdf5)", "stored dialect",
               "host dialect (NetCDF only)"),
        "Q4": ("the object count", "extracted object count",
               "host object count (NetCDF only)"),
        "Q5": ("an object descriptor (path+kind+type+shape)",
               "extracted descriptor", "host descriptor (NetCDF only)"),
        "Q7": ("`hdf-find` over object/attribute names (with spans)",
               "no source span -> typed decline", "no source span -> typed decline"),
        "Q8": ("an object's data bytes", "no source bytes -> typed decline",
               "no source bytes -> typed decline"),
        "Q9": ("a dimension's name", "extracted dimension name",
               "host dimension name (NetCDF only)"),
        "Q10": ("an attribute's name", "no faithful attr order -> typed decline",
                "no faithful attr order -> typed decline"),
        "Q11": ("the recorded format (hdf)", "stored format",
                "format not recorded -> typed decline"),
        "Q12": ("an object's exact data span", "no source span -> typed decline",
                "no source span -> typed decline"),
    },
    "cfbf": {
        "Q1": ("an entry's path", "no faithful extraction -> typed decline",
               "comparator_absent"),
        "Q2": ("an entry's exact source span", "no source span -> typed decline",
               "comparator_absent"),
        "Q3": ("the recorded dialect (cfb3/cfb4)",
               "no faithful extraction -> typed decline", "comparator_absent"),
        "Q4": ("the entry count", "no faithful extraction -> typed decline",
               "comparator_absent"),
        "Q5": ("an entry/stream descriptor",
               "no faithful extraction -> typed decline", "comparator_absent"),
        "Q7": ("`cfbf-find` over entry/stream names (with spans)",
               "no source span -> typed decline", "comparator_absent"),
        "Q8": ("a stream's exact bytes", "no source bytes -> typed decline",
               "comparator_absent"),
        "Q9": ("the stream count", "no faithful extraction -> typed decline",
               "comparator_absent"),
        "Q10": ("an entry's object type (root/storage/stream)",
                "no faithful extraction -> typed decline", "comparator_absent"),
        "Q11": ("the recorded format (cfbf)", "stored format", "comparator_absent"),
        "Q12": ("a stream's exact data span", "no source span -> typed decline",
                "comparator_absent"),
    },
    "doc": {
        "Q1": ("a piece's decoded text", "no faithful extraction -> typed decline",
               "comparator_absent"),
        "Q2": ("a piece's exact source span", "no source span -> typed decline",
               "comparator_absent"),
        "Q3": ("the recorded dialect (Word version)",
               "no faithful extraction -> typed decline", "comparator_absent"),
        "Q4": ("the piece count", "no faithful extraction -> typed decline",
               "comparator_absent"),
        "Q5": ("the FIB descriptor (version/table/ccps/spans)",
               "no faithful extraction -> typed decline", "comparator_absent"),
        "Q7": ("`doc-find` over piece texts (with spans)",
               "no source span -> typed decline", "comparator_absent"),
        "Q8": ("a piece's exact `WordDocument` bytes",
               "no source bytes -> typed decline", "comparator_absent"),
        "Q9": ("a CP section's text", "no faithful extraction -> typed decline",
               "comparator_absent"),
        "Q10": ("a piece's compressed flag",
                "no faithful extraction -> typed decline", "comparator_absent"),
        "Q11": ("the recorded format (doc)", "stored format", "comparator_absent"),
        "Q12": ("a piece's exact stream byte span", "no source span -> typed decline",
                "comparator_absent"),
    },
    "xls": {
        "Q1": ("a sheet's name", "no faithful extraction -> typed decline",
               "comparator_absent"),
        "Q2": ("a sheet's exact source span", "no source span -> typed decline",
               "comparator_absent"),
        "Q3": ("the recorded dialect (BIFF version)",
               "no faithful extraction -> typed decline", "comparator_absent"),
        "Q4": ("the sheet count", "no faithful extraction -> typed decline",
               "comparator_absent"),
        "Q5": ("a sheet descriptor (name/pos/type/span)",
               "no faithful extraction -> typed decline", "comparator_absent"),
        "Q7": ("`xls-find` over sheet names/shared strings (with spans)",
               "no source span -> typed decline", "comparator_absent"),
        "Q8": ("a cell's exact record bytes", "no source bytes -> typed decline",
               "comparator_absent"),
        "Q9": ("a cell's stored value", "no faithful extraction -> typed decline",
               "comparator_absent"),
        "Q10": ("a cell descriptor (row/col/kind/span)",
                "no faithful extraction -> typed decline", "comparator_absent"),
        "Q11": ("the recorded format (xls)", "stored format", "comparator_absent"),
        "Q12": ("a BIFF record descriptor (id/len/span)",
                "no source span -> typed decline", "comparator_absent"),
    },
    "ppt": {
        "Q1": ("a slide's text", "no faithful extraction -> typed decline",
               "comparator_absent"),
        "Q2": ("a slide's exact source span", "no source span -> typed decline",
               "comparator_absent"),
        "Q3": ("the recorded dialect (PowerPoint version)",
               "no faithful extraction -> typed decline", "comparator_absent"),
        "Q4": ("the slide count", "no faithful extraction -> typed decline",
               "comparator_absent"),
        "Q5": ("a slide descriptor (persist/cTexts/list/span)",
               "no faithful extraction -> typed decline", "comparator_absent"),
        "Q7": ("`ppt-find` over text runs (with spans)",
               "no source span -> typed decline", "comparator_absent"),
        "Q8": ("a text run's exact atom bytes", "no source bytes -> typed decline",
               "comparator_absent"),
        "Q9": ("a `SlideListWithText` list's text",
               "no faithful extraction -> typed decline", "comparator_absent"),
        "Q10": ("a text run descriptor (slide/kind/type/span)",
                "no faithful extraction -> typed decline", "comparator_absent"),
        "Q11": ("the recorded format (ppt)", "stored format", "comparator_absent"),
        "Q12": ("an MS-PPT record descriptor (id/len/span)",
                "no source span -> typed decline", "comparator_absent"),
    },
    "msg": {
        "Q1": ("a property's name", "no faithful extraction -> typed decline",
               "comparator_absent"),
        "Q2": ("a property entry's exact source span",
               "no source span -> typed decline", "comparator_absent"),
        "Q3": ("the recorded dialect (MSG version)",
               "no faithful extraction -> typed decline", "comparator_absent"),
        "Q4": ("the property count", "no faithful extraction -> typed decline",
               "comparator_absent"),
        "Q5": ("a recipient/attachment descriptor",
               "no faithful extraction -> typed decline", "comparator_absent"),
        "Q7": ("`msg-find` over substream texts (with spans)",
               "no source span -> typed decline", "comparator_absent"),
        "Q8": ("a property entry's exact bytes", "no source bytes -> typed decline",
               "comparator_absent"),
        "Q9": ("the sub-stream count", "no faithful extraction -> typed decline",
               "comparator_absent"),
        "Q10": ("a property descriptor (tag/id/type/span)",
                "no faithful extraction -> typed decline", "comparator_absent"),
        "Q11": ("the recorded format (msg)", "stored format", "comparator_absent"),
        "Q12": ("a property entry's exact byte span", "no source span -> typed decline",
                "comparator_absent"),
    },
}


def cfg(fmt):
    return {"lane_fixtures": LANE_FIXTURES[fmt], "controls": CONTROLS[fmt],
            "plans": PLANS[fmt], "qdesc": QDESC[fmt]}


def plan_dump(fmt):
    c = cfg(fmt)
    return {"format": fmt, "lane_fixtures": c["lane_fixtures"],
            "controls": c["controls"], "plans": c["plans"], "qdesc": c["qdesc"]}


# ===========================================================================
# Pure-Python conventional extractors (shared by both comparators)
# ===========================================================================

def _cfg_lines(text):
    """Split a config-family source into logical lines.

    Returns (dialect, lines) where each line is a dict:
    {kind: 'entry'|'section'|'comment'|'blank', key, value, marker, raw}
    The properties dialect joins a trailing-`\\` continuation.
    """
    nl = text.split("\n")
    if nl and nl[-1] == "":
        nl = nl[:-1]
    # dialect detection
    dialect = None
    for ln in nl:
        s = ln.strip()
        if s.startswith("[") and s.endswith("]") and len(s) > 2:
            dialect = "ini"
            break
    if dialect is None:
        for ln in nl:
            s = ln.lstrip()
            if s.startswith("export ") or s.startswith("export\t"):
                dialect = "env"
                break
    if dialect is None:
        for ln in nl:
            if ln.rstrip().endswith("\\"):
                dialect = "properties"
                break
        if dialect is None:
            for ln in nl:
                if "\\u" in ln or "\\U" in ln:
                    dialect = "properties"
                    break
        if dialect is None:
            for ln in nl:
                s = ln.strip()
                if s and not s.startswith("#") and not s.startswith(";"):
                    # a `:` separator with content after it
                    body = s.split("\\", 1)[0]
                    if ":" in body and body.index(":") > 0:
                        dialect = "properties"
                        break
    lines = []
    i = 0
    n = len(nl)
    while i < n:
        raw = nl[i]
        stripped = raw.strip()
        if stripped == "":
            lines.append({"kind": "blank", "key": None, "value": None,
                          "marker": None, "raw": raw})
            i += 1
            continue
        c0 = stripped[0]
        if c0 == "#" or c0 == ";":
            lines.append({"kind": "comment", "key": None, "value": None,
                          "marker": c0, "raw": raw})
            i += 1
            continue
        if c0 == "[":
            close = raw.rfind("]")
            name = raw[raw.index("[") + 1:close] if close > raw.index("[") else ""
            lines.append({"kind": "section", "key": name.strip(), "value": None,
                          "marker": None, "raw": raw})
            i += 1
            continue
        # entry; possibly continued
        buf = raw
        while buf.rstrip().endswith("\\") and i + 1 < n:
            i += 1
            buf = buf.rstrip()[:-1] + "\n" + nl[i]
        body = buf
        work = body.lstrip()
        export = False
        if work.startswith("export ") or work.startswith("export\t"):
            export = True
            work = work[7:].lstrip()
        # find first unescaped = or :
        marker = None
        idx = -1
        j = 0
        while j < len(work):
            ch = work[j]
            if ch == "\\":
                j += 2
                continue
            if ch == "=" or ch == ":":
                marker = ch
                idx = j
                break
            j += 1
        if marker is None:
            key = work.strip()
            value = ""
        else:
            key = work[:idx].strip()
            value = work[idx + 1:].strip()
        value = _strip_inline_comment(value)
        lines.append({"kind": "entry", "key": key, "value": value,
                      "marker": marker, "raw": buf, "export": export})
        i += 1
    return dialect, lines


def _strip_inline_comment(value):
    """Drop an unquoted `;`/`#` inline comment from a config value."""
    q = None
    j = 0
    while j < len(value):
        ch = value[j]
        if q is not None:
            if ch == "\\":
                j += 2
                continue
            if ch == q:
                q = None
        elif ch in "\"'":
            q = ch
        elif (ch == ";" or ch == "#") and j > 0 and value[j - 1].isspace():
            return value[:j].rstrip()
        j += 1
    return value


def _cfg_decode_value(dialect, value):
    """Decode a conventional value (quotes stripped, continuation joined,
    \\uXXXX expanded) — the host-value view a conventional load exposes."""
    v = value
    if len(v) >= 2 and ((v[0] == '"' and v[-1] == '"') or
                        (v[0] == "'" and v[-1] == "'")):
        q = v[0]
        v = v[1:-1]
        if q == '"':
            v = v.replace('\\"', '"').replace("\\n", "\n").replace("\\t", "\t")
    # join a properties continuation: `\` + newline
    v = v.replace("\\\n", "")
    # expand \uXXXX
    def _u(m):
        try:
            return chr(int(m.group(1), 16))
        except ValueError:
            return m.group(0)
    v = re.sub(r"\\u([0-9a-fA-F]{4})", _u, v)
    return v


def _cfg_model(text):
    dialect, lines = _cfg_lines(text)
    entries = []
    comments = []
    sections = []
    kinds = []
    for ln in lines:
        kinds.append(ln["kind"])
        if ln["kind"] == "entry":
            entries.append({"key": ln["key"],
                            "value": _cfg_decode_value(dialect, ln["value"])})
        elif ln["kind"] == "comment":
            comments.append(ln["marker"])
        elif ln["kind"] == "section":
            sections.append(ln["key"])
    return {"dialect": dialect, "lines": lines, "kinds": kinds,
            "entries": entries, "comments": comments, "sections": sections}


def _xml_local(tag):
    return tag.rsplit("}", 1)[-1] if "}" in tag else tag


def _xml_text(elem):
    return "".join(elem.itertext())


def _xml_model(text, kind):
    root = ET.fromstring(text)
    lname = _xml_local(root.tag)
    if kind == "feed":
        dialect = "atom" if lname == "feed" else ("rss" if lname == "rss" else None)
        channel = None
        for ch in root:
            if _xml_local(ch.tag) in ("channel", "feed"):
                channel = ch
                break
        if channel is None:
            channel = root
        fields = []
        for ch in channel:
            l = _xml_local(ch.tag)
            if l in ("item", "entry"):
                continue
            fields.append({"name": l, "text": _xml_text(ch)})
        entries = []
        for ch in channel:
            l = _xml_local(ch.tag)
            if l not in ("item", "entry"):
                continue
            ef = []
            for e in ch:
                ef.append({"name": _xml_local(e.tag), "text": _xml_text(e)})
            entries.append(ef)
        return {"dialect": dialect, "channel": fields, "entries": entries}
    return _gis_model(text, lname)


KML_GEOM = {"Point", "LineString", "Polygon", "MultiGeometry", "LinearRing",
            "Model", "coordinates"}
GPX_REC = {"wpt", "rte", "trk"}
GPX_PT = {"wpt", "rtept", "trkpt"}
GPX_REC_SKIP = {"trkseg", "rtept", "trkpt"}


def _gis_model(text, lname):
    root = ET.fromstring(text)
    dialect = "kml" if lname == "kml" else ("gpx" if lname == "gpx" else None)
    records = []
    points = []
    for el in root.iter():
        l = _xml_local(el.tag)
        if dialect == "kml":
            if l == "Placemark":
                fields = [{"name": _xml_local(c.tag), "text": _xml_text(c)}
                          for c in el if _xml_local(c.tag) not in KML_GEOM]
                records.append({"kind": "Placemark", "fields": fields})
            elif l == "Point":
                fields = [{"name": _xml_local(c.tag), "text": _xml_text(c)}
                          for c in el]
                points.append({"kind": "Point", "attrs": [], "fields": fields})
        else:
            if l in GPX_REC:
                fields = [{"name": _xml_local(c.tag), "text": _xml_text(c)}
                          for c in el if _xml_local(c.tag) not in GPX_REC_SKIP]
                records.append({"kind": l, "fields": fields})
            if l in GPX_PT:
                attrs = [{"name": k, "value": el.attrib[k]}
                         for k in ("lat", "lon") if k in el.attrib]
                fields = [{"name": _xml_local(c.tag), "text": _xml_text(c)}
                          for c in el]
                points.append({"kind": l, "attrs": attrs, "fields": fields})
    return {"dialect": dialect, "records": records, "points": points}


# ===========================================================================
# Phase 21.25-21.26 conventional extraction (tabular + prose families)
# ===========================================================================

TAB_DIALECTS = {".psv": ("pipe", "|"), ".tsv": ("tab", "\t"),
                ".csv": ("comma", ",")}


def _tab_dialect(name):
    low = name.lower()
    for ext, pair in TAB_DIALECTS.items():
        if low.endswith(ext):
            return pair
    return TAB_DIALECTS[".csv"]


def _fw_split(line):
    """Split a fixed-width record into trimmed cells on 2+-space gaps.

    The record is right-padded to the uniform width, so trailing padding must be
    stripped before splitting (otherwise it yields a spurious empty cell).
    """
    return [c.strip() for c in re.split(r"  +", line.rstrip())]


def _tab_model(raw, kind, name):
    text = raw.decode("utf-8", "replace")
    canon = text.replace("\r\n", "\n").replace("\r", "\n")
    lines = canon.split("\n")
    if lines and lines[-1] == "":
        lines = lines[:-1]
    if kind == "csv":
        dname, dchar = _tab_dialect(name)
        records = list(csv.reader(io.StringIO(canon), delimiter=dchar))
        fmt = dname
    else:
        records = [_fw_split(ln) for ln in lines]
        fmt = "fixedwidth"
    return {"kind": kind, "format": fmt, "text": canon, "records": records,
            "rows": len(records), "header": records[0] if records else []}


def _prose_block(group, fmt):
    first = group[0]
    if fmt == "rst":
        if len(group) >= 2:
            a = group[1].strip()
            if len(a) >= 3 and set(a) == {a[0]} and a[0] in "=~`:'\"^-_*+#<>":
                return {"kind": "title", "text": first.strip()}
    elif fmt == "asciidoc":
        m = re.match(r"^(=+)\s+(.*)$", first)
        if m:
            kind = "doc-title" if len(m.group(1)) == 1 else "section"
            return {"kind": kind, "text": m.group(2).strip()}
    elif fmt == "mdx":
        m = re.match(r"^(#{1,6})\s+(.*)$", first)
        if m:
            return {"kind": "heading", "text": m.group(2).strip()}
    if re.match(r"^\s*([-*+]|\d+[.)])\s+", first):
        return {"kind": "list-item",
                "text": re.sub(r"^\s*([-*+]|\d+[.)])\s+", "", first).strip()}
    return {"kind": "paragraph", "text": "\n".join(group)}


def _prose_blocks(canon, fmt):
    lines = canon.split("\n")
    if lines and lines[-1] == "":
        lines = lines[:-1]
    groups = []
    cur = []
    for ln in lines:
        if ln.strip() == "":
            if cur:
                groups.append(cur)
                cur = []
        else:
            cur.append(ln)
    if cur:
        groups.append(cur)
    return [_prose_block(g, fmt) for g in groups]


def _prose_model(raw, fmt):
    text = raw.decode("utf-8", "replace")
    canon = text.replace("\r\n", "\n").replace("\r", "\n")
    blocks = _prose_blocks(canon, fmt)
    if fmt == "rst":
        heads = [b["text"] for b in blocks if b["kind"] == "title"]
        directives = sum(1 for ln in canon.split("\n")
                         if re.match(r"^\.\.\s+\S+::", ln))
        count = len(heads)
    elif fmt == "asciidoc":
        allh = [b["text"] for b in blocks if b["kind"] in ("doc-title", "section")]
        heads = [b["text"] for b in blocks if b["kind"] == "section"]
        directives = sum(1 for ln in canon.split("\n")
                         if re.match(r"^:[^:\s][^:]*:", ln))
        count = len(heads)
    else:  # mdx
        heads = [b["text"] for b in blocks if b["kind"] == "heading"]
        directives = sum(1 for ln in canon.split("\n")
                         if re.match(r"^(import|export)\b", ln))
        count = len(heads)
    return {"format": fmt, "text": canon, "blocks": blocks, "headings": heads,
            "counts": {"headings": count, "directives": directives}}


def _read_cmodel(lane, d):
    if lane == "sqlite":
        _, model = _read_sqlite(d)
        return model
    with open(os.path.join(d, "model.json")) as f:
        return json.load(f)


def _q_tabular(lane, d, q, plan):
    if lane == "duckdb":
        return _q_tabular_duckdb(d, q, plan)
    model = _read_cmodel(lane, d)
    recs = model.get("records") or []
    row = plan.get("row", 1)
    col = plan.get("col", 1)
    if q == "Q1":
        if row < len(recs) and col < len(recs[row]):
            return _laneless(q, recs[row][col])
        return decl(q, "missing", "no such cell")
    if q == "Q2":
        return decl(q, "no-source-span", "a conventional table load keeps no source span")
    if q == "Q3":
        if row < len(recs):
            return _laneless(q, len(recs[row]))
        return decl(q, "missing", "no such record")
    if q == "Q4":
        return _laneless(q, len(recs))
    if q == "Q5":
        if row < len(recs):
            return _laneless(q, list(recs[row]))
        return decl(q, "missing", "no such record")
    if q == "Q7":
        pat = plan["find_pat"]
        out = []
        for ri, r in enumerate(recs):
            for ci, c in enumerate(r):
                if pat in c:
                    out.append([_s(ri), _s(ci), c])
        return _laneless(q, out)
    if q == "Q8":
        return decl(q, "no-source-bytes", "a conventional table load keeps no source bytes")
    if q == "Q9":
        return decl(q, "no-source-bytes", "a conventional table load keeps no source bytes")
    if q == "Q10":
        return _laneless(q, list(model.get("header") or []))
    if q == "Q11":
        fmt = model.get("format")
        if lane == "conv" or fmt is None:
            return decl(q, "dialect-not-recorded",
                        "a conventional table load records no dialect")
        return _laneless(q, fmt)
    if q == "Q12":
        return decl(q, "no-layout-spelling",
                    "a conventional table load keeps no column layout or quoting spelling")
    return decl(q, "unknown-question", q)


def _q_prose(lane, d, q, plan):
    model = _read_cmodel(lane, d)
    blocks = model.get("blocks") or []
    if q == "Q1":
        return _laneless(q, model.get("text"))
    if q == "Q2":
        return decl(q, "no-source-span",
                    "a prose load re-flows and keeps no exact source span")
    if q == "Q3":
        c = model.get("counts")
        if not c:
            return decl(q, "no-typed-structure",
                        "a line-based prose load derives no typed heading count")
        return _laneless(q, c.get("headings"))
    if q == "Q4":
        return decl(q, "no-typed-list-model",
                    "a line-based prose load builds no list model")
    if q == "Q5":
        if blocks:
            return _laneless(q, blocks[0]["text"])
        return decl(q, "missing", "no blocks")
    if q == "Q7":
        pat = plan["find_pat"]
        return _laneless(q, [b["text"] for b in blocks if pat in b["text"]])
    if q == "Q8":
        return decl(q, "no-source-bytes", "a prose load keeps no exact source bytes")
    if q == "Q9":
        return decl(q, "no-adornment",
                    "a line-based prose load strips and forgets adornment/delimiters")
    if q == "Q10":
        c = model.get("counts")
        if not c:
            return decl(q, "no-typed-structure",
                        "a line-based prose load derives no typed structure count")
        return _laneless(q, c.get("directives"))
    if q == "Q11":
        fmt = model.get("format")
        if fmt is None:
            return decl(q, "format-not-recorded", "a prose load records no format")
        return _laneless(q, fmt)
    if q == "Q12":
        return decl(q, "no-inline-model",
                    "a line-based prose load builds no inline model")
    return decl(q, "unknown-question", q)


# ===========================================================================
# Build
# ===========================================================================

def build(fmt, lane, source_path, out_dir):
    os.makedirs(out_dir, exist_ok=True)
    if fmt in NINE:
        return _build_new(fmt, lane, source_path, out_dir)
    if lane == "duckdb":
        if fmt != "tabular":
            raise SystemExit("the duckdb lane is defined only for the tabular format")
        print(json.dumps(build_duckdb_tabular(source_path, out_dir), sort_keys=True))
        return 0
    with open(source_path, "rb") as f:
        raw = f.read()
    text = raw.decode("utf-8", "replace")
    if fmt == "config":
        model = _cfg_model(text)
    elif fmt in ("feed", "gis"):
        try:
            model = _xml_model(text, fmt)
        except ET.ParseError as e:
            model = {"dialect": None, "error": str(e)}
    else:  # geojson / notebook
        model = None

    if fmt in ("geojson", "notebook"):
        # source-retaining: store the original bytes AND the original JSON text.
        _write_sqlite(os.path.join(out_dir, "x.sqlite"), raw, {"j": text})
        if lane == "conv":
            try:
                parsed = json.loads(text)
            except ValueError:
                parsed = None
            with open(os.path.join(out_dir, "parsed.json"), "w") as f:
                json.dump({"value": parsed}, f)
        print(json.dumps({"ok": True, "lane": lane, "src_len": len(raw)},
                         sort_keys=True))
        return 0

    if fmt in ("tabular", "rst", "asciidoc", "mdx"):
        if fmt == "tabular":
            kind = "fixedwidth" if source_path.lower().endswith(".fw") else "csv"
            full = _tab_model(raw, kind, os.path.basename(source_path))
        else:
            full = _prose_model(raw, fmt)
        if lane == "sqlite":
            _write_sqlite(os.path.join(out_dir, "x.sqlite"), raw, full)
        else:
            # A conventional decode-to-host load keeps no source bytes.
            if fmt == "tabular":
                reduced = {"format": None, "text": full.get("text"),
                           "records": full.get("records"),
                           "header": full.get("header")}
            else:
                reduced = {"format": None, "text": full.get("text"),
                           "blocks": full.get("blocks")}
            with open(os.path.join(out_dir, "model.json"), "w") as f:
                json.dump(reduced, f)
        print(json.dumps({"ok": True, "lane": lane, "src_len": len(raw),
                          "format": full.get("format")}, sort_keys=True))
        return 0

    # config/feed/gis
    _write_sqlite(os.path.join(out_dir, "x.sqlite"), raw, model or {})
    if lane == "conv":
        with open(os.path.join(out_dir, "model.json"), "w") as f:
            json.dump(model or {}, f)
    print(json.dumps({"ok": True, "lane": lane, "src_len": len(raw),
                      "dialect": (model or {}).get("dialect")}, sort_keys=True))
    return 0


def _write_sqlite(path, raw, model):
    import sqlite3
    for suffix in ("", "-wal", "-shm", "-journal"):
        try:
            os.remove(path + suffix)
        except FileNotFoundError:
            pass
    con = sqlite3.connect(path)
    con.execute("PRAGMA journal_mode=DELETE")
    con.execute("CREATE TABLE doc(id INTEGER PRIMARY KEY, raw BLOB, model TEXT)")
    con.execute("INSERT INTO doc VALUES(1,?,?)",
                (sqlite3.Binary(raw), json.dumps(model)))
    con.commit()
    con.close()


def _read_sqlite(d):
    import sqlite3
    con = sqlite3.connect(os.path.join(d, "x.sqlite"))
    try:
        row = con.execute("SELECT raw, model FROM doc WHERE id=1").fetchone()
        return bytes(row[0]), json.loads(row[1])
    finally:
        con.close()


# ===========================================================================
# DuckDB/Parquet comparator (the tabular format only; ADR-0059)
# ===========================================================================
#
# ADR-0059 mandates a DuckDB/Parquet baseline alongside SQLite for the
# tabular/analytical formats: those engines already embody columnar projection,
# predicate pushdown, compressed pages and metadata indexes, so winning only
# against SQLite there could just mean the wrong competitor was chosen. DuckDB
# is compared as a columnar/tabular comparator only: the projection keeps no
# exact source bytes, span, dialect or layout, so those questions are typed
# declines, never silent answers.
#
# The whole load runs inside DuckDB. A delimited source (PSV — the CSV adapter's
# pipe dialect) uses DuckDB's own CSV reader (the real "conventional columnar
# load"). Fixed-width has no native DuckDB reader, so the raw lines are read by
# DuckDB's CSV reader and split on 2+-space gaps with DuckDB SQL — the same
# conventional fixed-width decode the `conv` lane uses, kept inside DuckDB. The
# logical table is persisted as a Parquet projection (ZSTD) and every answer is
# a SQL projection over it. `row 0` is the source header record, so the row/cell
# indices match the other lanes' record model.

def build_duckdb_tabular(source_path, out_dir):
    import duckdb
    os.makedirs(out_dir, exist_ok=True)
    name = os.path.basename(source_path)
    kind = "fixedwidth" if source_path.lower().endswith(".fw") else "csv"
    con = duckdb.connect()
    if kind == "csv":
        delim = _tab_dialect(name)[1]
        con.execute(
            "CREATE TABLE t AS SELECT row_number() OVER () - 1 AS __rn, * "
            "FROM read_csv(?, delim=?, header=false, null_padding=true, "
            "all_varchar=true, parallel=false, sample_size=-1)",
            (source_path, delim))
        cols = [r[0] for r in con.execute("DESCRIBE t").fetchall() if r[0] != "__rn"]
        loader = "duckdb native CSV reader"
    else:
        con.execute(
            "CREATE TABLE raw AS SELECT row_number() OVER () - 1 AS __rn, "
            "column0 AS line FROM read_csv(?, delim=?, header=false, quote='', "
            "escape='', all_varchar=true, sample_size=-1)",
            (source_path, "\t"))
        con.execute(
            "CREATE TABLE cells AS SELECT __rn, "
            "string_split_regex(rtrim(line, ' \t\r\n'), '  +') AS cells FROM raw")
        n = con.execute("SELECT max(len(cells)) FROM cells").fetchone()[0] or 0
        sel = ", ".join("list_extract(cells, %d) AS c%d" % (i + 1, i) for i in range(n))
        con.execute("CREATE TABLE t AS SELECT __rn, %s FROM cells" % sel)
        cols = ["c%d" % i for i in range(n)]
        loader = "duckdb reader + SQL 2+-space split (no native fixed-width reader)"
    nrows = int(con.execute("SELECT COUNT(*) FROM t").fetchone()[0])
    hdr = con.execute(
        "SELECT %s FROM t WHERE __rn = 0" % ", ".join('"%s"' % c for c in cols)).fetchone()
    header = ["" if v is None else str(v) for v in (hdr or [])]
    con.execute("COPY t TO '%s' (FORMAT PARQUET, COMPRESSION ZSTD)"
                % os.path.join(out_dir, "t.parquet"))
    con.close()
    with open(os.path.join(out_dir, "meta.json"), "w") as f:
        json.dump({"columns": cols, "header": header, "ncols": len(cols),
                   "nrows": nrows, "kind": kind, "loader": loader},
                  f, sort_keys=True)
    return {"ok": True, "lane": "duckdb", "src_len": os.path.getsize(source_path),
            "format": kind, "ncols": len(cols), "nrows": nrows}


def _dd_connect(d):
    import duckdb
    con = duckdb.connect()
    con.execute("CREATE TABLE t AS SELECT * FROM read_parquet(?)",
                [os.path.join(d, "t.parquet")])
    with open(os.path.join(d, "meta.json")) as f:
        meta = json.load(f)
    return con, meta


def _q_tabular_duckdb(d, q, plan):
    con, meta = _dd_connect(d)
    try:
        cols = meta["columns"]
        row = plan.get("row", 1)
        col = plan.get("col", 1)
        if q == "Q1":
            if col < 0 or col >= len(cols):
                return decl(q, "no-such-column", "column out of range")
            r = con.execute('SELECT CAST("%s" AS VARCHAR) FROM t WHERE __rn = ?'
                            % cols[col], (row,)).fetchone()
            if r is None:
                return decl(q, "no-such-row", "row out of range")
            return _laneless(q, r[0])
        if q == "Q2":
            return decl(q, "no-source-span",
                        "a columnar projection keeps no exact source span")
        if q == "Q3":
            return _laneless(q, len(cols))
        if q == "Q4":
            return _laneless(q, meta["nrows"])
        if q == "Q5":
            r = con.execute("SELECT %s FROM t WHERE __rn = ?"
                            % ", ".join('CAST("%s" AS VARCHAR)' % c for c in cols),
                            (row,)).fetchone()
            if r is None:
                return decl(q, "no-such-row", "row out of range")
            return _laneless(q, list(r))
        if q == "Q7":
            pat = plan["find_pat"]
            out = []
            for k, c in enumerate(cols):
                rs = con.execute('SELECT __rn, CAST("%s" AS VARCHAR) FROM t '
                                 'WHERE contains(CAST("%s" AS VARCHAR), ?) '
                                 'ORDER BY __rn' % (c, c), (pat,)).fetchall()
                for rn, val in rs:
                    out.append([str(rn), str(k), val])
            return _laneless(q, out)
        if q in ("Q8", "Q9"):
            return decl(q, "no-source-bytes",
                        "a Parquet projection keeps no exact raw cell/record bytes")
        if q == "Q10":
            return _laneless(q, list(meta["header"]))
        if q == "Q11":
            return decl(q, "dialect-not-recorded",
                        "a Parquet projection records no source dialect")
        if q == "Q12":
            return decl(q, "no-layout-spelling",
                        "a columnar projection keeps no column layout or quoting spelling")
        return decl(q, "unknown-question", q)
    finally:
        con.close()


# ===========================================================================
# Comparator queries
# ===========================================================================

def _laneless(q, value):
    e = envelope(q, value)
    return e


def _q_config(lane, d, q, plan):
    raw, model = _read_sqlite(d)
    dialect = model.get("dialect")
    entries = model.get("entries", [])
    kinds = model.get("kinds", [])
    comments = model.get("comments", [])
    sections = model.get("sections", [])
    if q == "Q1":
        i = plan["value_entry"]
        if lane == "conv" and dialect in ("ini",):
            # configparser-style: last wins; still a plain value
            pass
        if i < len(entries):
            return _laneless(q, {"kind": "string", "value": entries[i]["value"]})
        return decl(q, "missing", "no entry")
    if q == "Q2":
        return decl(q, "no-source-span", "conventional extraction keeps no source span")
    if q == "Q3":
        if lane == "conv":
            return decl(q, "no-line-kinds", "configparser exposes no line kinds")
        li = plan["kind_line"]
        if li < len(kinds):
            return _laneless(q, kinds[li])
        return decl(q, "missing", "no line")
    if q == "Q4":
        if lane == "conv":
            return decl(q, "duplicates-collapsed", "a conventional load collapses duplicates")
        i = plan["dup_entry"]
        if i < len(entries):
            k = entries[i]["key"]
            n = sum(1 for e in entries if e["key"] == k)
            return _laneless(q, {"exists": n > 0, "duplicate_count": n})
        return decl(q, "missing", "no entry")
    if q == "Q5":
        sec = plan.get("section")
        if sec is None:
            return decl(q, "no-sections", "this dialect has no sections")
        n = sum(1 for s in sections if s == sec)
        if n == 0:
            return decl(q, "missing", "no such section")
        return _laneless(q, {"name": sec, "matches": n})
    if q == "Q7":
        pat = plan["find_pat"]
        if lane == "conv":
            out = []
            for e in entries:
                if pat in e["key"]:
                    out.append(["key", e["key"]])
                if pat in e["value"]:
                    out.append(["value", e["value"]])
            out.sort()
            return _laneless(q, out)
        out = []
        for e in entries:
            if pat in e["key"]:
                out.append(["key", e["key"]])
            if pat in e["value"]:
                out.append(["value", e["value"]])
        out.sort()
        return _laneless(q, out)
    if q == "Q8":
        return decl(q, "no-source-token", "conventional extraction keeps no token bytes")
    if q == "Q9":
        if lane == "conv":
            return decl(q, "comments-dropped", "a conventional config load drops comments")
        li = plan["comment_line"]
        lines = model.get("lines", [])
        if li < len(lines):
            return _laneless(q, {"kind": lines[li]["kind"],
                                 "marker": lines[li]["marker"]})
        return decl(q, "missing", "no line")
    if q == "Q10":
        return decl(q, "no-source-span", "conventional extraction keeps no spans")
    if q == "Q11":
        if lane == "conv":
            return decl(q, "dialect-not-recorded", "a conventional load records no dialect")
        if dialect is None:
            return decl(q, "no-dialect", "no dialect detected")
        return _laneless(q, dialect)
    if q == "Q12":
        return decl(q, "no-source-spelling", "conventional extraction keeps no spelling")
    return decl(q, "unknown-question", q)


def _q_xml(fmt, lane, d, q, plan):
    raw, model = _read_sqlite(d)
    dialect = model.get("dialect")
    if fmt == "feed":
        channel = model.get("channel", [])
        entries = model.get("entries", [])
        if q == "Q1":
            e = plan["entry"]
            name = plan["field"]
            if e < len(entries):
                for f in entries[e]:
                    if f["name"] == name:
                        return _laneless(q, {"kind": "string", "value": f["text"]})
            return decl(q, "missing", "no such entry field")
        if q == "Q2":
            return decl(q, "no-source-span", "a conventional XML load keeps no span")
        if q == "Q3":
            e = plan["entry"]
            name = plan["field"]
            if e < len(entries):
                for f in entries[e]:
                    if f["name"] == name:
                        return _laneless(q, f["name"])
            return decl(q, "missing", "no such field")
        if q == "Q4":
            e = plan["entry"]
            name = plan["dup_field"]
            if e < len(entries):
                n = sum(1 for f in entries[e] if f["name"] == name)
                return _laneless(q, {"exists": n > 0, "duplicate_count": n})
            return decl(q, "missing", "no such entry")
        if q == "Q5":
            e = plan["entry"]
            if e < len(entries):
                return _laneless(q, {"index": e,
                                     "fields": [f["name"] for f in entries[e]]})
            return decl(q, "missing", "no such entry")
        if q == "Q7":
            pat = plan["find_pat"]
            out = []
            for ei, ef in enumerate(entries):
                for f in ef:
                    if pat in f["text"]:
                        out.append([_s(ei), f["name"], f["text"]])
            for f in channel:
                if pat in f["text"]:
                    out.append(["*", f["name"], f["text"]])
            out.sort()
            return _laneless(q, out)
        if q == "Q8":
            return decl(q, "no-source-token", "a conventional XML load keeps no token")
        if q == "Q9":
            return decl(q, "no-attribute-spelling",
                        "ElementTree keeps no attribute spelling or order")
        if q == "Q10":
            return _laneless(q, [f["name"] for f in channel])
        if q == "Q11":
            if lane == "conv":
                return decl(q, "dialect-not-recorded", "a conventional load records no dialect")
            if dialect is None:
                return decl(q, "no-dialect", "no dialect detected")
            return _laneless(q, dialect)
        if q == "Q12":
            return decl(q, "no-source-span", "a conventional XML load keeps no span")
        return decl(q, "unknown-question", q)
    else:  # gis
        records = model.get("records", [])
        points = model.get("points", [])
        if q == "Q1":
            r = plan["record"]
            name = plan["record_field"]
            if r < len(records):
                for f in records[r]["fields"]:
                    if f["name"] == name:
                        return _laneless(q, {"kind": "string", "value": f["text"]})
            return decl(q, "missing", "no such record field")
        if q == "Q2":
            return decl(q, "no-source-span", "a conventional XML load keeps no span")
        if q == "Q3":
            p = plan["point"]
            if p < len(points):
                return _laneless(q, points[p]["kind"])
            return decl(q, "missing", "no such point")
        if q == "Q4":
            r = plan["record"]
            name = plan["field"]
            if r < len(records):
                n = sum(1 for f in records[r]["fields"] if f["name"] == name)
                return _laneless(q, {"exists": n > 0, "duplicate_count": n})
            return decl(q, "missing", "no such record")
        if q == "Q5":
            r = plan["record"]
            if r < len(records):
                return _laneless(q, {"index": r, "kind": records[r]["kind"],
                                     "fields": [f["name"] for f in records[r]["fields"]]})
            return decl(q, "missing", "no such record")
        if q == "Q7":
            pat = plan["find_pat"]
            out = []
            for ri, rec in enumerate(records):
                for f in rec["fields"]:
                    if pat in f["text"]:
                        out.append(["record", _s(ri), f["name"], f["text"]])
            for pi, pt in enumerate(points):
                for f in pt["fields"]:
                    if pat in f["text"]:
                        out.append(["point", _s(pi), f["name"], f["text"]])
            out.sort()
            return _laneless(q, out)
        if q == "Q8":
            return decl(q, "no-source-token", "a conventional XML load keeps no token")
        if q == "Q9":
            return decl(q, "no-attribute-spelling",
                        "ElementTree keeps no attribute spelling or order")
        if q == "Q10":
            p = plan["point"]
            if p < len(points):
                return _laneless(q, [f["name"] for f in points[p]["fields"]])
            return decl(q, "missing", "no such point")
        if q == "Q11":
            if lane == "conv":
                return decl(q, "dialect-not-recorded", "a conventional load records no dialect")
            if dialect is None:
                return decl(q, "no-dialect", "no dialect detected")
            return _laneless(q, dialect)
        if q == "Q12":
            p = plan["point"]
            if p < len(points):
                return _laneless(q, [[f["name"], f["text"]] for f in points[p]["fields"]])
            return decl(q, "missing", "no such point")
        return decl(q, "unknown-question", q)


def _s(i):
    return str(i)


def _q_json(fmt, lane, d, q, plan):
    if lane == "conv":
        with open(os.path.join(d, "parsed.json")) as f:
            root = json.load(f)["value"]
        return _q_json_conv(fmt, q, plan, root)
    raw, model = _read_sqlite(d)
    j = model.get("j")
    return _q_json_sqlite(fmt, q, plan, j)


def _load_pairs(text):
    """Parse JSON preserving duplicate keys (list of pairs at every object)."""
    return json.loads(text, object_pairs_hook=lambda kv: _Obj(kv))


class _Obj(list):
    """A list of (key, value) pairs (duplicate-preserving object)."""

    def get(self, key, default=None):
        for k, v in self:
            if k == key:
                return v
        return default

    def keys(self):
        return [k for k, _ in self]

    def items(self):
        return list(self)


def _q_json_sqlite(fmt, q, plan, j):
    import sqlite3
    con = sqlite3.connect(":memory:")
    try:
        if fmt == "geojson":
            return _gj_sqlite(con, j, q, plan)
        return _nb_sqlite(con, j, q, plan)
    finally:
        con.close()


def _gj_root_type(j):
    return _Obj(_load_pairs(j)).get("type") if j else None


def _py_kind_value(v):
    if isinstance(v, bool):
        return ("true" if v else "false"), ("true" if v else "false")
    if v is None:
        return "null", "null"
    if isinstance(v, _Obj):
        return "object", None
    if isinstance(v, list):
        return "array", None
    if isinstance(v, str):
        return "string", v
    return "number", canon_num(v)


def _gj_feature(root, f):
    feats = root.get("features")
    if isinstance(feats, list) and f < len(feats):
        return feats[f]
    return root


GJ_CORE = {"type", "id", "bbox", "properties", "geometry"}


def _gj_foreign(feat):
    if isinstance(feat, _Obj):
        return [k for k, _ in feat if k not in GJ_CORE]
    return []


def _gj_sqlite(con, j, q, plan):
    root = _Obj(_load_pairs(j))
    feat = _gj_feature(root, plan["feature"])
    props = feat.get("properties") if isinstance(feat, _Obj) else None
    if q == "Q1":
        if not isinstance(props, _Obj):
            return decl(q, "missing", "no properties")
        vals = [v for k, v in props if k == plan["prop"]]
        if not vals:
            return decl(q, "missing", "no such property")
        kind, value = _py_kind_value(vals[0])
        return _laneless(q, {"kind": kind, "value": value})
    if q == "Q2":
        return decl(q, "no-source-span", "SQLite exposes no source span")
    if q == "Q3":
        g = feat.get("geometry") if isinstance(feat, _Obj) else None
        if not isinstance(g, _Obj):
            return decl(q, "missing", "no geometry")
        return _laneless(q, g.get("type"))
    if q == "Q4":
        if not isinstance(props, _Obj):
            return decl(q, "missing", "no properties")
        n = sum(1 for k, _ in props if k == plan["key"])
        return _laneless(q, {"exists": n > 0, "duplicate_count": n})
    if q == "Q5":
        return _laneless(q, {"type": feat.get("type"), "foreign": _gj_foreign(feat)})
    if q == "Q7":
        return _laneless(q, _gj_find_pairs(root, plan["find_pat"]))
    if q == "Q8":
        return decl(q, "re-serialized", "SQLite re-serializes; spelling not preserved")
    if q == "Q9":
        return decl(q, "normalized", "numbers are normalized; spelling not preserved")
    if q == "Q10":
        return _laneless(q, _gj_foreign(feat))
    if q == "Q11":
        return _laneless(q, root.get("type"))
    if q == "Q12":
        return decl(q, "no-source-token", "SQLite keeps no token bytes")
    return decl(q, "unknown-question", q)


def _gj_find_pairs(root, pat):
    out = []

    def walk(node):
        if isinstance(node, _Obj):
            for k, v in node:
                if pat in str(k):
                    out.append(["key", str(k)])
                walk(v)
        elif isinstance(node, list):
            for v in node:
                walk(v)
        elif isinstance(node, str):
            if pat in node:
                out.append(["value", node])

    walk(root)
    out.sort()
    return out


def _nb_sqlite(con, j, q, plan):
    root = _Obj(_load_pairs(j))
    cell = plan["cell"]

    def cc():
        return root.get("cells") or []
    if q == "Q1":
        cells = cc()
        if cell < len(cells):
            return _laneless(q, {"kind": "string", "value": cells[cell].get("cell_type")})
        return decl(q, "missing", "no such cell")
    if q == "Q2":
        return decl(q, "no-source-span", "SQLite exposes no source span")
    if q == "Q3":
        cells = cc()
        if cell < len(cells):
            src = cells[cell].get("source")
            return _laneless(q, "lines" if isinstance(src, list) else "string")
        return decl(q, "missing", "no such cell")
    if q == "Q4":
        cells = cc()
        outputs = sum(len(c.get("outputs") or []) for c in cells)
        return _laneless(q, {"cells": len(cells), "outputs": outputs})
    if q == "Q5":
        cells = cc()
        if cell < len(cells):
            src = cells[cell].get("source")
            form = "lines" if isinstance(src, list) else "string"
            elems = len(src) if isinstance(src, list) else 1
            return _laneless(q, {"index": cell, "cell_type": cells[cell].get("cell_type"),
                                 "source_form": form, "source_elements": elems,
                                 "outputs": len(cells[cell].get("outputs") or [])})
        return decl(q, "missing", "no such cell")
    if q == "Q7":
        return _nb_find(root, plan["find_pat"], sqlite=True)
    if q == "Q8":
        return decl(q, "re-serialized", "SQLite re-serializes; spelling not preserved")
    if q == "Q9":
        cells = cc()
        oc = plan["out_cell"]
        oi = plan["out_idx"]
        if oc < len(cells):
            outs = cells[oc].get("outputs") or []
            if oi < len(outs):
                o = outs[oi]
                has_text = any(k == "text" for k, _ in o) if isinstance(o, _Obj) \
                    else ("text" in o)
                if not has_text:
                    tf = None
                else:
                    tf = "lines" if isinstance(o.get("text"), list) else "string"
                return _laneless(q, {"output_type": o.get("output_type"),
                                     "name": o.get("name"), "text_form": tf})
        return decl(q, "missing", "no such output")
    if q == "Q10":
        cells = cc()
        if cell < len(cells):
            src = cells[cell].get("source")
            return _laneless(q, {"elements": len(src) if isinstance(src, list) else 1})
        return decl(q, "missing", "no such cell")
    if q == "Q11":
        return _laneless(q, root.get("nbformat"))
    if q == "Q12":
        return _laneless(q, root.get("nbformat_minor"))
    return decl(q, "unknown-question", q)


def _nb_find(root, pat, sqlite):
    out = []

    def walk(node, path):
        if isinstance(node, _Obj):
            for k, v in node:
                if pat in str(k):
                    out.append(["key", str(k)])
                walk(v, path + "/" + str(k))
        elif isinstance(node, list):
            for i, v in enumerate(node):
                walk(v, path + "/" + str(i))
        elif isinstance(node, str):
            if pat in node:
                out.append(["value", node])

    walk(root, "")
    out.sort()
    return _laneless("Q7", out)


# --- conv (json) ------------------------------------------------------------

def _q_json_conv(fmt, q, plan, root):
    if root is None:
        return decl(q, "unparseable", "the source is not valid JSON")
    if fmt == "geojson":
        return _gj_conv(q, plan, root)
    return _nb_conv(q, plan, root)


def _gj_conv(q, plan, root):
    feats = root.get("features") if isinstance(root, dict) else None
    f = plan["feature"]
    feat = feats[f] if isinstance(feats, list) and f < len(feats) else root
    if q == "Q1":
        props = feat.get("properties") or {}
        if plan["prop"] not in props:
            return decl(q, "missing", "no such property")
        v = props[plan["prop"]]
        if isinstance(v, bool):
            return _laneless(q, {"kind": "true" if v else "false",
                                 "value": "true" if v else "false"})
        if isinstance(v, str):
            return _laneless(q, {"kind": "string", "value": v})
        if v is None:
            return _laneless(q, {"kind": "null", "value": "null"})
        return _laneless(q, {"kind": "number", "value": canon_num(v)})
    if q == "Q2":
        return decl(q, "no-source-span", "a conventional JSON load keeps no span")
    if q == "Q3":
        g = feat.get("geometry") or {}
        return _laneless(q, g.get("type") if isinstance(g, dict) else None)
    if q == "Q4":
        return decl(q, "duplicates-collapsed", "a conventional load collapses duplicates")
    if q == "Q5":
        core = {"type", "id", "bbox", "properties", "geometry"}
        foreign = [k for k in feat.keys() if k not in core]
        return _laneless(q, {"type": feat.get("type"), "foreign": foreign})
    if q == "Q7":
        return _gj_find_conv(root, plan["find_pat"])
    if q == "Q8":
        return decl(q, "host-value", "a conventional load yields host values")
    if q == "Q9":
        return decl(q, "host-numbers", "a conventional load parses numbers, losing spelling")
    if q == "Q10":
        core = {"type", "id", "bbox", "properties", "geometry"}
        return _laneless(q, [k for k in feat.keys() if k not in core])
    if q == "Q11":
        return _laneless(q, root.get("type"))
    if q == "Q12":
        return decl(q, "host-value", "a conventional load yields host values")
    return decl(q, "unknown-question", q)


def _gj_find_conv(root, pat):
    out = []

    def walk(node, path):
        if isinstance(node, dict):
            for k, v in node.items():
                if pat in k:
                    out.append(["key", k])
                walk(v, path + "/" + k)
        elif isinstance(node, list):
            for i, v in enumerate(node):
                walk(v, path + "/" + str(i))
        elif isinstance(node, str):
            if pat in node:
                out.append(["value", node])

    walk(root, "")
    out.sort()
    return _laneless("Q7", out)


def _nb_conv(q, plan, root):
    cells = root.get("cells") or []
    cell = plan["cell"]
    if q == "Q1":
        if cell < len(cells):
            return _laneless(q, {"kind": "string", "value": cells[cell].get("cell_type")})
        return decl(q, "missing", "no such cell")
    if q == "Q2":
        return decl(q, "no-source-span", "a conventional JSON load keeps no span")
    if q == "Q3":
        if cell < len(cells):
            src = cells[cell].get("source")
            return _laneless(q, "lines" if isinstance(src, list) else "string")
        return decl(q, "missing", "no such cell")
    if q == "Q4":
        outputs = sum(len(c.get("outputs") or []) for c in cells)
        return _laneless(q, {"cells": len(cells), "outputs": outputs})
    if q == "Q5":
        if cell < len(cells):
            src = cells[cell].get("source")
            form = "lines" if isinstance(src, list) else "string"
            elems = len(src) if isinstance(src, list) else 1
            return _laneless(q, {"index": cell, "cell_type": cells[cell].get("cell_type"),
                                 "source_form": form, "source_elements": elems,
                                 "outputs": len(cells[cell].get("outputs") or [])})
        return decl(q, "missing", "no such cell")
    if q == "Q7":
        out = []

        def walk(node):
            if isinstance(node, dict):
                for k, v in node.items():
                    if plan["find_pat"] in k:
                        out.append(["key", k])
                    walk(v)
            elif isinstance(node, list):
                for v in node:
                    walk(v)
            elif isinstance(node, str):
                if plan["find_pat"] in node:
                    out.append(["value", node])

        walk(root)
        out.sort()
        return _laneless(q, out)
    if q == "Q8":
        return decl(q, "host-value", "a conventional load yields host values")
    if q == "Q9":
        oc, oi = plan["out_cell"], plan["out_idx"]
        if oc < len(cells):
            outs = cells[oc].get("outputs") or []
            if oi < len(outs):
                o = outs[oi]
                txt = o.get("text")
                if "text" not in o:
                    tf = None
                else:
                    tf = "lines" if isinstance(txt, list) else "string"
                return _laneless(q, {"output_type": o.get("output_type"),
                                     "name": o.get("name"), "text_form": tf})
        return decl(q, "missing", "no such output")
    if q == "Q10":
        if cell < len(cells):
            src = cells[cell].get("source")
            return _laneless(q, {"elements": len(src) if isinstance(src, list) else 1})
        return decl(q, "missing", "no such cell")
    if q == "Q11":
        return _laneless(q, root.get("nbformat"))
    if q == "Q12":
        return _laneless(q, root.get("nbformat_minor"))
    return decl(q, "unknown-question", q)


def run_query(fmt, lane, d, q, plan):
    if lane == "duckdb" and fmt != "tabular":
        return decl(q, "lane-not-supported",
                    "the duckdb lane is defined only for the tabular format")
    if q == "Q6":
        if lane == "sqlite":
            raw, _ = _read_sqlite(d)
            return envelope(q, {"length": len(raw), "sha256": sha256_hex(raw)})
        return decl(q, "no-source-bytes",
                    "a conventional load retains no source bytes")
    if fmt == "config":
        env = _q_config(lane, d, q, plan)
    elif fmt in ("feed", "gis"):
        env = _q_xml(fmt, lane, d, q, plan)
    elif fmt == "tabular":
        env = _q_tabular(lane, d, q, plan)
    elif fmt in ("rst", "asciidoc", "mdx"):
        env = _q_prose(lane, d, q, plan)
    elif fmt in NINE:
        env = _q_new(fmt, lane, d, q, plan)
    else:
        env = _q_json(fmt, lane, d, q, plan)
    env["lane"] = lane
    return env


def query(fmt, lane, d, q, plan, out):
    env = run_query(fmt, lane, d, q, plan)
    with open(out, "w") as f:
        json.dump(env, f, sort_keys=True)
    return 0


def session(fmt, lane, d, queries, plan, out):
    batch = []
    for q in queries.split(","):
        q = q.strip()
        if not q:
            continue
        t0 = now_us()
        env = run_query(fmt, lane, d, q, plan)
        dt = now_us() - t0
        batch.append({"q": q, "us": dt, "declined": env["declined"]})
    with open(out, "w") as f:
        json.dump({"batch": batch}, f, sort_keys=True)
    return 0


def materialize(fmt, lane, d, out):
    raw, _ = _read_sqlite(d)
    with open(out, "wb") as f:
        f.write(raw)
    return 0


def version(fmt):
    import sqlite3
    print(json.dumps({"module": "sqlite3", "sqlite_version": sqlite3.sqlite_version,
                      "format": fmt, "python": sys.version.split()[0]}, sort_keys=True))
    return 0


# ===========================================================================
# Aggregate
# ===========================================================================

def _load_p19():
    import importlib.util
    path = os.path.join(HERE, "phase19-repeat.py")
    spec = importlib.util.spec_from_file_location("phase19_repeat", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


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


TIE = 0.10
QS = ["Q1", "Q2", "Q3", "Q4", "Q5", "Q6", "Q7", "Q8", "Q9", "Q10", "Q11", "Q12"]


def _norm_find(v):
    if not isinstance(v, list):
        return v
    return sorted(tuple(x) if isinstance(x, list) else x for x in v)


def compare(q, va, vb):
    if va is None or vb is None:
        return "missing"
    if va.get("declined") and vb.get("declined"):
        return "both-decline"
    if va.get("declined") or vb.get("declined"):
        return "capability-gap"
    a, b = va.get("value"), vb.get("value")
    if q == "Q7":
        a, b = _norm_find(a), _norm_find(b)
    return "equal" if a == b else "mismatch"


def aggregate(fmt, raw, campaign, env_path=None):
    P19 = _load_p19()
    B = 10000
    SEED = 2120 + FORMATS.index(fmt)
    import statistics

    env = {}
    if env_path and os.path.exists(env_path):
        with open(env_path) as f:
            try:
                env = json.load(f)
            except ValueError:
                env = {}

    c = cfg(fmt)
    qdesc = c["qdesc"]
    docs = read_tsv(os.path.join(raw, "fixtures.tsv"))
    fixtures = [r["fixture"] for r in docs]
    lanes = ["vole", "sqlite", "conv"]
    comparators = ["sqlite", "conv"]
    if fmt == "tabular":
        # ADR-0059: the tabular format carries the mandatory DuckDB/Parquet
        # comparator as a third lane alongside the two conventional ones.
        lanes = ["vole", "sqlite", "conv", "duckdb"]
        comparators = ["sqlite", "conv", "duckdb"]

    build_rows = read_tsv(os.path.join(raw, "build.tsv"))
    storage_rows = read_tsv(os.path.join(raw, "storage.tsv"))
    cold_rows = read_tsv(os.path.join(raw, "cold.tsv"))
    warm_rows = read_tsv(os.path.join(raw, "warm.tsv"))
    exact_rows = read_tsv(os.path.join(raw, "exact.tsv"))

    def best_by_fixture(rows, lane, metric="us"):
        out = {}
        for r in rows:
            if r["lane"] != lane or r.get("rc") != "0":
                continue
            fx = r["fixture"]
            v = int(r[metric])
            if fx not in out or v < out[fx]:
                out[fx] = v
        return out

    build_us = {lane: best_by_fixture(build_rows, lane) for lane in lanes}
    store_bytes = {lane: {} for lane in lanes}
    store_files = {lane: {} for lane in lanes}
    for r in storage_rows:
        store_bytes[r["lane"]][r["fixture"]] = int(r["bytes"])
        store_files[r["lane"]][r["fixture"]] = int(r["files"])

    def cold_by_fixture(lane):
        perrep = {}
        for r in cold_rows:
            if r["lane"] != lane or r.get("rc") != "0":
                continue
            key = (r["fixture"], r["rep"])
            perrep[key] = perrep.get(key, 0) + int(r["us"])
        out = {}
        for (fx, rep), v in perrep.items():
            if fx not in out or v < out[fx]:
                out[fx] = v
        return out

    cold_us = {lane: cold_by_fixture(lane) for lane in lanes}
    warm_us = {lane: {} for lane in lanes}
    warm_perrep = {lane: {} for lane in lanes}
    for r in warm_rows:
        if r.get("rc") != "0":
            continue
        fx, lane, rep = r["fixture"], r["lane"], r["rep"]
        key = (fx, rep)
        warm_perrep[lane][key] = warm_perrep[lane].get(key, 0) + int(r["us"])
    for lane in lanes:
        for (fx, rep), v in warm_perrep[lane].items():
            if fx not in warm_us[lane] or v < warm_us[lane][fx]:
                warm_us[lane][fx] = v

    qanswers = {}
    for fx in fixtures:
        for q in QS:
            row = {}
            for lane in lanes:
                p = os.path.join(raw, "qanswers", "%s.%s.%s.json" % (fx, q, lane))
                try:
                    with open(p) as f:
                        row[lane] = json.load(f)
                except (OSError, ValueError):
                    row[lane] = None
            qanswers[(fx, q)] = row

    equiv = {}
    for fx in fixtures:
        for q in QS:
            row = qanswers.get((fx, q), {})
            for comp in comparators:
                res = compare(q, row.get("vole"), row.get(comp))
                equiv.setdefault(q, {}).setdefault(comp, {}).setdefault(res, 0)
                equiv[q][comp][res] += 1

    exact_ok = sum(1 for r in exact_rows if r.get("vole_ok") in ("1", "true"))
    exact_n = len(exact_rows)
    verdict = "PASS" if exact_ok == exact_n and exact_n > 0 else "FAIL"

    lines = []
    lines.append("# Phase %s — %s economic court" % (env.get("phase", fmt), fmt))
    lines.append("")
    if fmt == "tabular":
        lines.append("**Question.** Against three conventional comparators — a "
                     "source-retaining store that keeps the raw bytes plus a conventional "
                     "extraction, a conventional decode-to-host-values load, and the "
                     "mandatory **DuckDB/Parquet** analytical baseline (ADR-0059) — on "
                     "contract-equivalent terms, can VOLE answer the same question family "
                     "(Q1–Q12) it can answer, while closing the original %s byte-exactly, "
                     "and does it add value by **preserving representation** (source spans, "
                     "exact spelling, quoting markers, the recorded dialect and the "
                     "recovered column layout)?" % fmt)
    else:
        lines.append("**Question.** Against two conventional comparators — a "
                     "source-retaining store that keeps the raw bytes plus a conventional "
                     "extraction, and a conventional decode-to-host-values load — on "
                     "contract-equivalent terms, can VOLE answer the same question family "
                     "(Q1–Q12) it can answer, while closing the original %s byte-exactly, "
                     "and does it add value by **preserving representation** (source spans, "
                     "exact spelling, attribute/quote/continuation markers, duplicate keys, "
                     "member order)?" % fmt)
    if fmt in ("doc", "xls", "ppt", "msg"):
        lines.append("")
        lines.append("**Lane C is `comparator_absent` for %s.** There is no faithful "
                     "pure-Python/stdlib decoder for a legacy .%s file in the pinned image, "
                     "so rather than fabricate one, every Lane-C question is recorded as a "
                     "typed `comparator_absent` decline (exactly as the HDF5/CFBF courts do). "
                     "The head-to-head is therefore **limited to a single substantive "
                     "comparator** — the source-retaining SQLite store (byte authority) — "
                     "and the source-retaining store itself declines every *structural* "
                     "question for the same reason. Read the paired ratios below as "
                     "VOLE-vs-a-bytes-store, not as a full decode-vs-decode comparison."
                     % (fmt, fmt))
    lines.append("")
    lines.append("Corpus: **%d fixtures**; lanes **%s**; questions **Q1–Q12**; "
                 "bootstrap **%d resamples, seed %d**, cluster-resampled by fixture; "
                 "tie band **+/-%d%%**." % (len(fixtures), ", ".join(lanes), B, SEED,
                                             int(TIE * 100)))
    lines.append("")
    prof = (env or {}).get("profile", "unknown")
    sub = (env or {}).get("vole_substrate", "unknown")
    lines.append("VOLE lane: **%s** profile (`%s`); substrate: **%s**." %
                 (prof, env.get("bin", "?"), sub))
    lines.append("")
    lines.append("## Verdict")
    lines.append("")
    lines.append("- **VOLE exactness (Q6): %d/%d byte-exact** (length + SHA-256 + "
                 "`cmp`, after source + descriptor deletion in a fresh process)."
                 % (exact_ok, exact_n))
    lines.append("- **COURT VERDICT: %s**." % verdict)
    lines.append("")
    lines.append("## Build + storage (per lane, per fixture)")
    lines.append("")
    lines.append("Time columns are **microseconds (`us`)**.")
    lines.append("")
    lines.append("| fixture | src B | " + " | ".join("%s build us" % l for l in lanes) +
                 " | " + " | ".join("%s B" % l for l in lanes) + " |")
    lines.append("|---|---:|" + "".join("---:|" for _ in lanes) +
                 "".join("---:|" for _ in lanes))
    for r in docs:
        fx = r["fixture"]
        cells = [r["src_bytes"]]
        for lane in lanes:
            cells.append(str(build_us[lane].get(fx, "-")))
        for lane in lanes:
            cells.append(str(store_bytes[lane].get(fx, "-")))
        lines.append("| " + fx + " | " + " | ".join(cells) + " |")
    lines.append("")
    lines.append("`build us` is the best-of-N (min) of the retained repetitions.")
    lines.append("")
    lines.append("## Paired ratios VOLE/comparator (median + geometric mean, 95% CI by fixture)")
    lines.append("")
    lines.append("| metric | comparator | n | median | geomean | median 95% CI | "
                 "geomean 95% CI | wins | ties | losses | ratio of sums |")
    lines.append("|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|")
    ratio_report = {}
    for label, series in (("build", build_us), ("storage", store_bytes),
                          ("cold", cold_us), ("warm", warm_us)):
        for other in comparators:
            ratios = {}
            sums_v = sums_o = 0
            for fx in fixtures:
                v, o = series["vole"].get(fx), series[other].get(fx)
                if v is None or o in (None, 0):
                    continue
                ratios[fx] = float(v) / float(o)
                sums_v += v
                sums_o += o
            if not ratios:
                continue
            vals = list(ratios.values())
            bydoc = {fx: [rt] for fx, rt in ratios.items()}
            lo_m, hi_m, _ = P19.cluster_bootstrap(bydoc, statistics.median, B, SEED)
            lo_g, hi_g, _ = P19.cluster_bootstrap(bydoc, P19.geomean, B, SEED + 1)
            wins = sum(1 for x in vals if x < 1 - TIE)
            ties = sum(1 for x in vals if 1 - TIE <= x <= 1 + TIE)
            losses = sum(1 for x in vals if x > 1 + TIE)
            ratio_report.setdefault(label, {})[other] = {
                "n": len(vals), "median": statistics.median(vals),
                "geomean": P19.geomean(vals), "median_lo": lo_m, "median_hi": hi_m,
                "geomean_lo": lo_g, "geomean_hi": hi_g, "wins": wins, "ties": ties,
                "losses": losses, "ratio_of_sums": (sums_v / sums_o if sums_o else 0),
            }
            lines.append("| {} | {} | {} | {:.3f} | {:.3f} | {:.3f}..{:.3f} | "
                         "{:.3f}..{:.3f} | {} | {} | {} | {:.3f} |".format(
                             label, other, len(vals), statistics.median(vals),
                             P19.geomean(vals), lo_m, hi_m, lo_g, hi_g, wins, ties,
                             losses, (sums_v / sums_o if sums_o else 0)))
    lines.append("")
    lines.append("A ratio < 1 favours VOLE. The estimator is the **paired per-fixture "
                 "ratio**, summarised by the median and geometric mean with a fixed-seed "
                 "cluster bootstrap over fixtures; `ratio of sums` is reported separately "
                 "and named as such. Each per-fixture lane cost is the **minimum over the "
                 "retained repetitions** (best-of-N); storage is measured once after the "
                 "last build. Every raw sample is kept in `raw/build.tsv`, "
                 "`raw/storage.tsv`, `raw/cold.tsv`, `raw/warm.tsv`.")
    lines.append("")
    lines.append("## Per-lane totals (sum over fixtures; times in microseconds `us`)")
    lines.append("")
    lines.append("| lane | build us | storage B | cold us | warm us |")
    lines.append("|---|---:|---:|---:|---:|")
    for lane in lanes:
        lines.append("| {} | {} | {} | {} | {} |".format(
            lane,
            sum(build_us[lane].get(fx, 0) for fx in fixtures),
            sum(store_bytes[lane].get(fx, 0) for fx in fixtures),
            sum(cold_us[lane].get(fx, 0) for fx in fixtures),
            sum(warm_us[lane].get(fx, 0) for fx in fixtures)))
    lines.append("")
    lines.append("## What each lane derives and what it declines")
    lines.append("")
    if fmt == "tabular":
        lines.append("| Q | VOLE | source-retaining baseline | conventional load | DuckDB/Parquet |")
        lines.append("|---|---|---|---|---|")
    else:
        lines.append("| Q | VOLE | source-retaining baseline | conventional load |")
        lines.append("|---|---|---|---|")
    for q in QS:
        if q == "Q6":
            if fmt == "tabular":
                lines.append("| Q6 | `materialize --exact` (byte authority) | retained raw "
                             "BLOB (byte authority) | no source bytes -> typed decline | "
                             "not-native (no original bytes) -> typed decline |")
            else:
                lines.append("| Q6 | `materialize --exact` (byte authority) | retained raw "
                             "BLOB (byte authority) | no source bytes -> typed decline |")
            continue
        d = qdesc.get(q, ("", "", ""))
        if fmt == "tabular" and len(d) >= 4:
            lines.append("| {} | {} | {} | {} | {} |".format(q, d[0], d[1], d[2], d[3]))
        else:
            lines.append("| {} | {} | {} | {} |".format(q, d[0], d[1], d[2]))
    lines.append("")
    lines.append("## Scope (honest)")
    lines.append("")
    lines.append("- **Self-authored deterministic corpus, NOT a real-world population.** "
                 "The fixtures are generated by `tools/fixtures/make-%s.py` (Python stdlib "
                 "only). Every claim is scoped to these files." % fmt)
    lines.append("- **Only Q6 is a byte-authority claim.**")
    lines.append("- **The conventional load is deliberately the weaker comparator.** It "
                 "drops spans, exact spelling, duplicate keys, and member/attribute order; "
                 "a span-preserving loader could in principle match VOLE on those, and no "
                 "claim is made against one.")
    lines.append("- **VOLE capability gaps are recorded, never papered over** — any "
                 "question VOLE declines is shown as a `capability-gap`.")
    if fmt == "tabular":
        lines.append("- **The DuckDB/Parquet lane is the mandatory ADR-0059 analytical "
                     "comparator**, carried alongside SQLite. It answers the "
                     "columnar/tabular questions (Q1/Q3/Q4/Q5/Q7/Q10) from a Parquet "
                     "projection; exact source bytes/spans (Q2/Q8/Q9), the recorded "
                     "dialect (Q11) and the recovered column layout/quoting spelling "
                     "(Q12) are typed declines, never silent answers. Where it wins or "
                     "loses an axis, the paired-ratio table above records it.")
    if fmt in C_ABSENT_NOTE:
        lines.append("- **Lane C has NO faithful stdlib decoder for %s**: %s. "
                     "Every Lane-C answer for that scope is recorded as a typed "
                     "`comparator_absent` decline, never a silent or fabricated "
                     "answer; VOLE capability is shown as `capability-gap` against it."
                     % (fmt.upper(), C_ABSENT_NOTE[fmt][1]))
    if fmt in ("doc", "xls", "ppt", "msg"):
        lines.append("- **Cold is Python-startup-dominated.** Each comparator cold probe is "
                     "a *fresh* `python3` process, so the cold ratio mostly measures "
                     "interpreter startup, not query cost; the **warm** column (one resident "
                     "session) is the honest per-query comparison. Both are reported.")
        lines.append("- **The median and the ratio of sums can diverge** (the ratio of sums "
                     "weights by absolute cost; the median treats each fixture equally). "
                     "Both are reported **separately** and neither is presented as the "
                     "estimator: the paired per-fixture ratio summarised by the median + "
                     "geometric mean (with a fixed-seed cluster bootstrap 95% CI) is the "
                     "stated estimator, and `ratio of sums` is named as such.")
        lines.append("- **The single substantive comparator limits the comparison.** With "
                     "Lane C `comparator_absent`, only the source-retaining SQLite store is "
                     "a real counterpart, and it too stores the bytes rather than decoding "
                     "them, so the axes on which VOLE is *decisively* cheaper or costlier on "
                     "a faithful decode are not exercised here.")
        if len(fixtures) == 1:
            lines.append("- **Single-fixture corpus (n=1):** the fixed-seed cluster bootstrap "
                         "is **degenerate** (its 95% CI collapses to the point estimate), so "
                         "the paired ratio and every derived cost are point estimates over "
                         "one file, not a population estimate.")
    lines.append("- **Nothing here is run on the host**; every command ran in a pinned "
                 "container.")
    lines.append("")

    matrix = []
    matrix.append("# Phase %s — cross-lane Q1–Q12 answer matrix (%s)" % (env.get("phase", fmt), fmt))
    matrix.append("")
    matrix.append("`g` = answered (derived), `D` = typed decline, `-` = not applicable.")
    matrix.append("")
    if fmt == "tabular":
        matrix.append("| fixture | Q | VOLE | source-ret | conv | DuckDB | "
                      "VOLE<->source | VOLE<->conv | VOLE<->duck |")
        matrix.append("|---|---|---|---|---|---|---|---|---|")
    else:
        matrix.append("| fixture | Q | VOLE | source-ret | conv | VOLE<->source | VOLE<->conv |")
        matrix.append("|---|---|---|---|---|---|---|")
    for fx in fixtures:
        for q in QS:
            row = qanswers.get((fx, q), {})

            def mark(lane):
                e = row.get(lane)
                if e is None:
                    return "-"
                return "D" if e.get("declined") else "g"

            rs = compare(q, row.get("vole"), row.get("sqlite"))
            rc_ = compare(q, row.get("vole"), row.get("conv"))
            if fmt == "tabular":
                rd = compare(q, row.get("vole"), row.get("duckdb"))
                matrix.append("| %s | %s | %s | %s | %s | %s | %s | %s | %s |" % (
                    fx, q, mark("vole"), mark("sqlite"), mark("conv"), mark("duckdb"),
                    rs, rc_, rd))
            else:
                matrix.append("| %s | %s | %s | %s | %s | %s | %s |" % (
                    fx, q, mark("vole"), mark("sqlite"), mark("conv"), rs, rc_))
    matrix.append("")
    matrix.append("### Aggregate equivalence per Q")
    matrix.append("")
    matrix.append("| Q | comparator | equal | both-decline | capability-gap | mismatch |")
    matrix.append("|---|---|---:|---:|---:|---:|")
    for q in QS:
        for comp in comparators:
            c2 = equiv.get(q, {}).get(comp, {})
            matrix.append("| {} | {} | {} | {} | {} | {} |".format(
                q, comp, c2.get("equal", 0), c2.get("both-decline", 0),
                c2.get("capability-gap", 0), c2.get("mismatch", 0)))
    matrix.append("")

    counts = []
    counts.append("format %s" % fmt)
    counts.append("fixtures %d" % len(fixtures))
    counts.append("questions %d" % len(QS))
    counts.append("exact_ok %d" % exact_ok)
    counts.append("exact_n %d" % exact_n)
    for q in QS:
        for comp in comparators:
            c2 = equiv.get(q, {}).get(comp, {})
            counts.append("%s.%s.equal %d" % (q, comp, c2.get("equal", 0)))
            counts.append("%s.%s.capability_gap %d" % (q, comp, c2.get("capability-gap", 0)))
            counts.append("%s.%s.mismatch %d" % (q, comp, c2.get("mismatch", 0)))
            counts.append("%s.%s.both_decline %d" % (q, comp, c2.get("both-decline", 0)))
    counts.append("verdict %s" % verdict)

    with open(os.path.join(campaign, "SUMMARY.md"), "w") as f:
        f.write("\n".join(lines) + "\n")
    with open(os.path.join(campaign, "MATRIX.md"), "w") as f:
        f.write("\n".join(matrix) + "\n")
    with open(os.path.join(campaign, "counts.txt"), "w") as f:
        f.write("\n".join(counts) + "\n")

    receipt = {
        "campaign": campaign,
        "phase": env.get("phase", fmt),
        "format": fmt,
        "verdict": verdict,
        "exact_ok": exact_ok,
        "exact_n": exact_n,
        "fixtures": fixtures,
        "lanes": lanes,
        "estimator": ("paired per-fixture ratio; median + geometric mean; fixed-seed "
                      "cluster bootstrap by fixture (%d resamples, seed %d); tie band "
                      "+/-%d%%; ratio of sums reported separately"
                      % (B, SEED, int(TIE * 100))),
        "equivalence": {q: equiv.get(q, {}) for q in QS},
        "comparators": comparators,
        "ratios": ratio_report,
        "lane_c_absent": (list(C_ABSENT_NOTE[fmt]) if fmt in C_ABSENT_NOTE else None),
        "environment": env,
    }
    with open(os.path.join(campaign, "receipt.json"), "w") as f:
        json.dump(receipt, f, indent=2, sort_keys=True)
    print("\n".join(lines))
    return 0 if verdict == "PASS" else 1


# ===========================================================================
# Phase 21.27-21.35: the nine additional document families
# ===========================================================================
#
# Each family has a **conventional** Python-stdlib extractor (the model) that a
# real decoder would expose: `zipfile`/`tarfile` for container, `email` +
# `quopri`/`base64` + `html.parser` for MHTML, `json`/`tomllib` for
# pkgmeta/apispec/CSL-JSON, a line/tag reader for logstream/bib, a control-word
# scanner for RTF, and a small pure-Python **NetCDF-classic** reader for the
# NetCDF part of `hdf`. The source-retaining SQLite lane keeps the raw bytes too
# (byte authority) and stores the same model; the conventional `conv` lane keeps
# only the derived host-value view and records no dialect.
#
# Lane C has NO faithful stdlib decoder for **CFBF/OLE2** at all and for the
# **HDF5** part of `hdf`, so those decline every question with the recorded code
# `comparator_absent` (see `C_ABSENT_NOTE` / `_conv_absent`). Nothing is
# fabricated to stand in for a decoder that does not exist.

MHTML_DIALECT = "mhtml"


def _type_info(fmt):
    return None


def _new_model(fmt, raw, name):
    fn = {
        "mhtml": _model_mhtml, "logstream": _model_logstream,
        "pkgmeta": _model_pkgmeta, "apispec": _model_apispec,
        "bib": _model_bib, "rtf": _model_rtf, "container": _model_container,
        "hdf": _model_hdf, "cfbf": _model_cfbf,
        "doc": _model_doc, "xls": _model_xls, "ppt": _model_ppt, "msg": _model_msg,
    }[fmt]
    model = fn(raw, name)
    model["_name"] = name
    return model


def _new_reduced(lane, model):
    """The conventional decode-to-host-values view (a deep copy; the lane-specific
    declines for the recorded dialect/format are enforced per query)."""
    import copy
    return copy.deepcopy(model)


def _build_new(fmt, lane, source_path, out_dir):
    os.makedirs(out_dir, exist_ok=True)
    name = os.path.basename(source_path)
    with open(source_path, "rb") as f:
        raw = f.read()
    model = _new_model(fmt, raw, name)
    if lane == "sqlite":
        _write_sqlite(os.path.join(out_dir, "x.sqlite"), raw, model)
    else:
        with open(os.path.join(out_dir, "model.json"), "w") as f:
            json.dump(_new_reduced(lane, model), f)
    print(json.dumps({"ok": True, "lane": lane, "src_len": len(raw),
                      "format": model.get("_format")}, sort_keys=True))
    return 0


def _read_new_model(lane, d):
    if lane == "sqlite":
        _, model = _read_sqlite(d)
        return model
    with open(os.path.join(d, "model.json")) as f:
        return json.load(f)


# --- MHTML ------------------------------------------------------------------

def _model_mhtml(raw, name):
    import email
    msg = email.message_from_bytes(raw)
    top = list(msg.items())
    body = msg.get_payload() if msg.is_multipart() else []
    if not isinstance(body, list):
        body = []

    def cte(p):
        v = p.get("Content-Transfer-Encoding")
        return v if v else "7bit"

    def mtype(p):
        return (p.get_content_type() or "").lower()

    start = msg.get_param("start") if msg.get("Content-Type") else None
    root = None
    if start:
        for p in body:
            if (p.get("Content-ID") or "").strip() == start.strip():
                root = p
                break
    if root is None:
        for p in body:
            if mtype(p) == "text/html":
                root = p
                break
    if root is None and body:
        root = body[0]
    root_text = root_mt = root_cte = None
    if root is not None:
        data = root.get_payload(decode=True)
        if data is None:
            pl = root.get_payload()
            data = pl.encode("utf-8", "replace") if isinstance(pl, str) else b""
        root_text = data.decode("utf-8", "replace")
        root_mt = mtype(root)
        root_cte = cte(root)
    headers = len(top) + sum(len(p.items()) for p in body)
    resources = [{"media_type": mtype(p), "cte": cte(p)}
                 for p in body if p is not root]
    return {"_format": "mhtml", "dialect": "mhtml", "root_text": root_text,
            "root_media_type": root_mt, "root_cte": root_cte, "headers": headers,
            "locations": {"snapshot_content_location":
                          msg.get("Snapshot-Content-Location"),
                          "content_base": msg.get("Content-Base")},
            "resources": resources}


def _q_mhtml(lane, model, q, plan):
    if q == "Q1":
        return _laneless(q, {"kind": "string", "value": model.get("root_text")})
    if q == "Q2":
        return decl(q, "no-source-span", "a conventional MIME load keeps no entity span")
    if q == "Q3":
        return _laneless(q, model.get("root_media_type"))
    if q == "Q4":
        return _laneless(q, model.get("headers"))
    if q == "Q5":
        return _laneless(q, dict(model.get("locations", {})))
    if q == "Q7":
        return decl(q, "no-source-span", "a conventional MIME load keeps no part/span")
    if q == "Q8":
        return decl(q, "re-decoded",
                    "a conventional load re-decodes a derived view; no exact bytes")
    if q == "Q9":
        res = plan.get("res")
        resources = model.get("resources", [])
        if res is None or res >= len(resources):
            return decl(q, "missing", "no such sub-resource")
        return _laneless(q, resources[res]["media_type"])
    if q == "Q10":
        return decl(q, "no-source-spelling",
                    "a conventional load keeps no transfer-encoding spelling")
    if q == "Q11":
        if lane == "conv":
            return decl(q, "format-not-recorded", "a conventional load records no format")
        return _laneless(q, "mhtml")
    if q == "Q12":
        return decl(q, "no-source-span", "a conventional MIME load keeps no body span")
    return decl(q, "unknown-question", q)


# --- log-stream -------------------------------------------------------------

def _ls_records(text):
    records = []
    blanks = 0
    pos = 0
    n = len(text)
    while pos < n:
        nl = text.find("\n", pos)
        if nl == -1:
            content, term = text[pos:], ""
            pos = n
        else:
            seg = text[pos:nl]
            if seg.endswith("\r"):
                content, term = seg[:-1], "crlf"
            else:
                content, term = seg, "lf"
            pos = nl + 1
        if content.strip() == "":
            blanks += 1
        else:
            records.append({"content": content, "term": term})
    return records, blanks


def _ls_dialect(content):
    if re.match(r"^<\d{1,3}>\d+ ", content):
        return "rfc5424"
    if re.match(r"^<\d{1,3}>[A-Z][a-z]{2} ", content):
        return "rfc3164"
    return "generic"


def _ls_generic_msg(content):
    s = content
    if s.startswith("["):
        close = s.find("]")
        if close == -1:
            return None
        rest = s[close + 1:].lstrip()
    else:
        m = re.match(r"^(\S+)\s+(.*)$", s)
        if not m:
            return None
        tok, rest = m.group(1), m.group(2)
        if re.search(r"[-:T]", tok) and re.search(r"\d", tok):
            pass  # a timestamp token; rest begins with the level
        else:
            return (tok, rest)
    m = re.match(r"^(\S+)\s+(.*)$", rest)
    if not m:
        return (rest, "")
    return (m.group(1), m.group(2))


def _model_logstream(raw, name):
    text = raw.decode("utf-8", "replace")
    records, blanks = _ls_records(text)
    return {"_format": "logstream", "dialect": "logstream",
            "records": records, "blank_lines": blanks}


def _q_logstream(lane, model, q, plan):
    records = model.get("records", [])
    rec = plan.get("rec", 0)
    if q == "Q1":
        if rec >= len(records):
            return decl(q, "missing", "no such record")
        content = records[rec]["content"]
        if _ls_dialect(content) != "generic":
            return decl(q, "no-typed-message",
                        "a conventional line reader does not split this dialect's message")
        parsed = _ls_generic_msg(content)
        if parsed is None:
            return decl(q, "no-typed-message", "the line does not match the generic dialect")
        return _laneless(q, {"kind": "string", "value": parsed[1]})
    if q == "Q2":
        return decl(q, "no-source-span", "a conventional line load keeps no span")
    if q == "Q3":
        if rec >= len(records):
            return decl(q, "missing", "no such record")
        return _laneless(q, _ls_dialect(records[rec]["content"]))
    if q == "Q4":
        return _laneless(q, len(records))
    if q == "Q5":
        return decl(q, "no-field-role-model",
                    "a conventional line reader builds no field-role model")
    if q == "Q7":
        return decl(q, "no-source-span", "a conventional line load keeps no span")
    if q == "Q8":
        return decl(q, "no-source-token", "a conventional line load keeps no token bytes")
    if q == "Q9":
        if rec >= len(records):
            return decl(q, "missing", "no such record")
        return _laneless(q, records[rec]["term"])
    if q == "Q10":
        if rec >= len(records):
            return decl(q, "missing", "no such record")
        return _laneless(q, len(records[rec]["content"]))
    if q == "Q11":
        if lane == "conv":
            return decl(q, "format-not-recorded", "a conventional load records no format")
        return _laneless(q, "logstream")
    if q == "Q12":
        return decl(q, "no-source-bytes", "a conventional line load keeps no source bytes")
    return decl(q, "unknown-question", q)


# --- package metadata -------------------------------------------------------

_PKG_ROLE = {
    "npm_package": {"scripts": "scripts", "dependencies": "dependencies",
                    "devDependencies": "dev-dependencies",
                    "peerDependencies": "peer-dependencies",
                    "optionalDependencies": "optional-dependencies"},
    "cargo_manifest": {"package": "package", "dependencies": "dependencies",
                       "dev-dependencies": "dev-dependencies",
                       "build-dependencies": "build-dependencies"},
    "pyproject_manifest": {"project": "project", "build-system": "build-system",
                           "tool": "tool"},
    "cargo_lock": {"package": "lock-package", "dependencies": "dependencies"},
    "npm_lock": {},
}


def _pkgmeta_dialect(raw):
    import tomllib
    text = raw.decode("utf-8", "replace")
    if text.lstrip().startswith("{"):
        d = json.loads(text)
        return "npm_lock" if "lockfileVersion" in d else "npm_package"
    d = tomllib.loads(text)
    if isinstance(d.get("package"), list):
        return "cargo_lock"
    if "build-system" in d or "project" in d or "tool" in d:
        return "pyproject_manifest"
    return "cargo_manifest"


def _model_pkgmeta(raw, name):
    import tomllib
    text = raw.decode("utf-8", "replace")
    dialect = _pkgmeta_dialect(raw)
    parsed = json.loads(text) if dialect in ("npm_package", "npm_lock") \
        else tomllib.loads(text)
    return {"_format": "pkgmeta", "dialect": dialect, "parsed": parsed}


def _pkgmeta_section(model, sname):
    parsed = model.get("parsed", {})
    node = parsed.get(sname) if isinstance(parsed, dict) else None
    if isinstance(node, dict):
        kind = "table"
    elif isinstance(node, list):
        kind = "array"
    elif node is None:
        kind = None
    else:
        kind = "scalar"
    entries = len(node) if isinstance(node, dict) else 0
    role = _PKG_ROLE.get(model.get("dialect"), {}).get(sname, "other")
    return {"name": sname, "role": role, "kind": kind, "entries": entries}


def _q_pkgmeta(lane, model, q, plan):
    sec = _pkgmeta_section(model, plan.get("sname", ""))
    if q == "Q1":
        if sec["kind"] is None:
            return decl(q, "missing", "no such section")
        return _laneless(q, sec["name"])
    if q == "Q2":
        return decl(q, "no-source-span", "a conventional manifest load keeps no span")
    if q == "Q3":
        if sec["kind"] is None:
            return decl(q, "missing", "no such section")
        return _laneless(q, sec["role"])
    if q == "Q4":
        if sec["kind"] is None:
            return decl(q, "missing", "no such section")
        return _laneless(q, sec["kind"])
    if q == "Q5":
        if sec["kind"] is None:
            return decl(q, "missing", "no such section")
        return _laneless(q, {"name": sec["name"], "role": sec["role"],
                             "kind": sec["kind"]})
    if q == "Q7":
        return decl(q, "no-source-span", "a conventional manifest load keeps no span")
    if q == "Q8":
        if sec["kind"] is None:
            return decl(q, "missing", "no such section")
        return _laneless(q, sec["entries"])
    if q == "Q9":
        if lane == "conv":
            return decl(q, "dialect-not-recorded", "a conventional load records no dialect")
        return _laneless(q, model.get("dialect"))
    if q == "Q10":
        return decl(q, "no-source-span", "a conventional manifest load keeps no span")
    if q == "Q11":
        if lane == "conv":
            return decl(q, "format-not-recorded", "a conventional load records no format")
        return _laneless(q, "pkgmeta")
    if q == "Q12":
        return decl(q, "no-source-bytes",
                    "a conventional manifest load keeps no exact entry bytes")
    return decl(q, "unknown-question", q)


# --- API specification ------------------------------------------------------

def _model_apispec(raw, name):
    d = json.loads(raw.decode("utf-8", "replace"))
    if "$schema" in d:
        dialect, version = "json_schema", str(d["$schema"])
    elif "openapi" in d:
        dialect, version = "openapi", str(d["openapi"])
    elif "swagger" in d:
        dialect, version = "swagger", str(d["swagger"])
    elif "asyncapi" in d:
        dialect, version = "asyncapi", str(d["asyncapi"])
    else:
        dialect, version = None, None
    refs = []

    def walk(o):
        if isinstance(o, dict):
            for k, v in o.items():
                if k == "$ref":
                    refs.append(v)
                walk(v)
        elif isinstance(o, list):
            for v in o:
                walk(v)

    walk(d)
    members = len(d) if isinstance(d, dict) else 0
    return {"_format": "apispec", "dialect": dialect, "version": version,
            "refs": refs, "root_members": members}


def _q_apispec(lane, model, q, plan):
    if q == "Q1":
        if model.get("version") is None:
            return decl(q, "missing", "no version")
        return _laneless(q, {"kind": "string", "value": model.get("version")})
    if q == "Q2":
        return decl(q, "no-source-span", "a conventional JSON load keeps no span")
    if q == "Q3":
        if lane == "conv":
            return decl(q, "dialect-not-recorded", "a conventional load records no dialect")
        return _laneless(q, model.get("dialect"))
    if q == "Q4":
        return _laneless(q, {"role": "root", "members": model.get("root_members")})
    if q == "Q5":
        refs = model.get("refs", [])
        ref = plan.get("ref")
        if ref is None or ref >= len(refs):
            return decl(q, "missing", "no such $ref")
        return _laneless(q, {"kind": "string", "value": refs[ref]})
    if q == "Q7":
        return decl(q, "no-source-span", "a conventional JSON load keeps no span")
    if q == "Q8":
        return decl(q, "no-source-token", "a conventional JSON load keeps no token bytes")
    if q == "Q9":
        return _laneless(q, len(model.get("refs", [])))
    if q == "Q10":
        return _laneless(q, "root")
    if q == "Q11":
        if lane == "conv":
            return decl(q, "format-not-recorded", "a conventional load records no format")
        return _laneless(q, "apispec")
    if q == "Q12":
        return decl(q, "no-source-token", "a conventional JSON load keeps no token bytes")
    return decl(q, "unknown-question", q)


# --- bibliography -----------------------------------------------------------

def _bibtex_entries(text):
    out = []
    for m in re.finditer(r"@(\w+)\s*\{\s*([^,\s]*)\s*,", text):
        etype, key = m.group(1), m.group(2)
        start = m.end()
        depth = 1
        i = start
        while i < len(text) and depth:
            c = text[i]
            if c == "{":
                depth += 1
            elif c == "}":
                depth -= 1
            i += 1
        body = text[start:i - 1]
        fields = []
        for fm in re.finditer(r"(\w[\w-]*)\s*=\s*", body):
            fname = fm.group(1)
            j = fm.end()
            if j < len(body) and body[j] == "{":
                d = 1
                k = j + 1
                while k < len(body) and d:
                    if body[k] == "{":
                        d += 1
                    elif body[k] == "}":
                        d -= 1
                    k += 1
                raw = body[j:k]
            elif j < len(body) and body[j] == '"':
                k = j + 1
                while k < len(body) and body[k] != '"':
                    k += 1
                raw = body[j:k + 1]
            else:
                k = j
                while k < len(body) and body[k] not in ",\n":
                    k += 1
                raw = body[j:k].strip()
            fields.append((fname.lower(), raw))
        out.append({"type": etype, "key": key, "fields": fields})
    return out


def _bibtex_value(raw):
    r = raw.strip()
    if len(r) >= 2 and r[0] == "{" and r[-1] == "}":
        return r[1:-1]
    if len(r) >= 2 and r[0] == '"' and r[-1] == '"':
        return r[1:-1]
    return r


def _ris_entries(text):
    entries = []
    cur = None
    for line in text.split("\n"):
        if len(line) >= 6 and line[0:2].isupper() and line[2:4] == "  " \
                and line[4:6] == "- ":
            tag, val = line[0:2], line[6:]
            if tag == "TY":
                cur = {"type": val.strip(), "key": "", "fields": [(tag, val)]}
                entries.append(cur)
            elif tag == "ER":
                if cur is not None:
                    cur["fields"].append((tag, val))
                cur = None
            elif cur is not None:
                cur["fields"].append((tag, val))
        elif cur is not None and cur["fields"]:
            # a continuation line extends the last field's raw value
            tag, val = cur["fields"][-1]
            cur["fields"][-1] = (tag, val + "\n" + line)
    return entries


def _model_bib(raw, name):
    text = raw.decode("utf-8", "replace")
    head = text.lstrip()
    if head.startswith("[") or head.startswith("{"):
        data = json.loads(text)
        items = data if isinstance(data, list) else [data]
        return {"_format": "bib", "dialect": "csl_json", "items": items,
                "entries": [], "fields": 0, "records": len(items)}
    if re.search(r"^TY  - ", text, re.M):
        entries = _ris_entries(text)
        nfields = sum(len(e["fields"]) for e in entries)
        return {"_format": "bib", "dialect": "ris", "items": [],
                "entries": entries, "fields": nfields, "records": len(entries)}
    entries = _bibtex_entries(text)
    nfields = sum(len(e["fields"]) for e in entries)
    return {"_format": "bib", "dialect": "bibtex", "items": [],
            "entries": entries, "fields": nfields, "records": len(entries)}


def _bib_field(entry, fname):
    for nm, raw in entry.get("fields", []):
        if nm.lower() == fname.lower():
            return raw
    return None


def _q_bib(lane, model, q, plan):
    dialect = model.get("dialect")
    entry = plan.get("entry", 0)
    if q == "Q1":
        if dialect == "csl_json":
            items = model.get("items", [])
            if entry >= len(items):
                return decl(q, "missing", "no such item")
            v = items[entry].get(plan.get("field"))
            return decl(q, "missing", "no such field") if v is None \
                else _laneless(q, {"kind": "string", "value": v})
        entries = model.get("entries", [])
        if entry >= len(entries):
            return decl(q, "missing", "no such entry")
        raw = _bib_field(entries[entry], plan.get("field"))
        if raw is None:
            return decl(q, "missing", "no such field")
        if dialect == "ris":
            return _laneless(q, {"kind": "string", "value": raw})
        return _laneless(q, {"kind": "string", "value": _bibtex_value(raw)})
    if q == "Q2":
        return decl(q, "no-source-span", "a conventional bibliography load keeps no span")
    if q == "Q3":
        if lane == "conv":
            return decl(q, "dialect-not-recorded", "a conventional load records no dialect")
        return _laneless(q, dialect)
    if q == "Q4":
        if dialect == "csl_json":
            return decl(q, "no-duplicate-model",
                        "a CSL item view exposes no duplicate-field count")
        entries = model.get("entries", [])
        if entry >= len(entries):
            return decl(q, "missing", "no such entry")
        names = [nm.lower() for nm, _ in entries[entry].get("fields", [])]
        return _laneless(q, len(names) - len(set(names)))
    if q == "Q5":
        if dialect == "csl_json":
            items = model.get("items", [])
            if entry >= len(items):
                return decl(q, "missing", "no such item")
            return _laneless(q, {"key": items[entry].get("id"),
                                 "type": items[entry].get("type")})
        entries = model.get("entries", [])
        if entry >= len(entries):
            return decl(q, "missing", "no such entry")
        return _laneless(q, {"key": entries[entry].get("key"),
                             "type": entries[entry].get("type")})
    if q == "Q7":
        return decl(q, "no-source-span", "a conventional bibliography load keeps no span")
    if q == "Q8":
        return decl(q, "no-source-token", "a conventional bibliography load keeps no token")
    if q == "Q9":
        return _laneless(q, model.get("records"))
    if q == "Q10":
        return _laneless(q, model.get("fields"))
    if q == "Q11":
        if lane == "conv":
            return decl(q, "format-not-recorded", "a conventional load records no format")
        return _laneless(q, "bib")
    if q == "Q12":
        return decl(q, "no-source-span", "a conventional bibliography load keeps no span")
    return decl(q, "unknown-question", q)


# --- RTF --------------------------------------------------------------------

def _model_rtf(raw, name):
    b = raw
    if b.startswith(b"\xef\xbb\xbf"):
        b = b[3:]
    text = b.decode("latin-1")
    n = len(text)
    i = 0
    depth = 0
    version = None
    fonttbl = 0
    hexesc = 0
    first_control = None
    first_unicode = None
    header = []
    header_done = False
    while i < n:
        c = text[i]
        if c == "{":
            depth += 1
            if depth > 1:
                header_done = True
            i += 1
            continue
        if c == "}":
            depth -= 1
            i += 1
            continue
        if c == "\\":
            if i + 1 < n and text[i + 1] == "'":
                hexesc += 1
                i += 4
                continue
            j = i + 1
            name_start = j
            while j < n and text[j].isalpha():
                j += 1
            name = text[name_start:j]
            neg = False
            if j < n and text[j] == "-":
                neg = True
                j += 1
            dstart = j
            while j < n and text[j].isdigit():
                j += 1
            param = text[dstart:j]
            if not name:
                i = j if j > i + 1 else i + 2
                continue
            if first_control is None:
                first_control = name
            if name == "rtf" and param:
                version = int(("-" if neg else "") + param)
            if name == "fonttbl":
                fonttbl += 1
            if name == "u" and first_unicode is None and param:
                first_unicode = int(param)
            if not header_done and depth == 1:
                header.append(name + param)
            i = j
            continue
        i += 1
    return {"_format": "rtf", "dialect": "rtf", "header": " ".join(header),
            "version": version, "fonttbl": fonttbl, "first_control": first_control,
            "hex_escapes": hexesc, "first_unicode": first_unicode}


def _q_rtf(lane, model, q, plan):
    if q == "Q1":
        return _laneless(q, {"kind": "string", "value": model.get("header")})
    if q == "Q2":
        return decl(q, "no-source-span", "a conventional RTF load keeps no span")
    if q == "Q3":
        return _laneless(q, model.get("version"))
    if q == "Q4":
        return _laneless(q, model.get("fonttbl"))
    if q == "Q5":
        return _laneless(q, {"kind": "string", "value": model.get("first_control")})
    if q == "Q7":
        return decl(q, "no-source-span", "a conventional RTF load keeps no run span")
    if q == "Q8":
        return decl(q, "no-source-token", "a conventional RTF load keeps no token bytes")
    if q == "Q9":
        return _laneless(q, model.get("hex_escapes"))
    if q == "Q10":
        if model.get("first_unicode") is None:
            return decl(q, "missing", "no unicode escape in the document")
        return _laneless(q, model.get("first_unicode"))
    if q == "Q11":
        if lane == "conv":
            return decl(q, "format-not-recorded", "a conventional load records no format")
        return _laneless(q, "rtf")
    if q == "Q12":
        return decl(q, "no-source-span", "a conventional RTF load keeps no span")
    return decl(q, "unknown-question", q)


# --- archive container (ZIP/TAR) --------------------------------------------

def _zip_members(raw):
    import struct
    eocd = raw.rfind(b"PK\x05\x06")
    if eocd < 0:
        raise ValueError("no EOCD")
    count = struct.unpack_from("<H", raw, eocd + 10)[0]
    off = struct.unpack_from("<I", raw, eocd + 16)[0]
    members = []
    p = off
    for _ in range(count):
        if raw[p:p + 4] != b"PK\x01\x02":
            break
        flags = struct.unpack_from("<H", raw, p + 8)[0]
        method = struct.unpack_from("<H", raw, p + 10)[0]
        crc = struct.unpack_from("<I", raw, p + 16)[0]
        csize = struct.unpack_from("<I", raw, p + 20)[0]
        usize = struct.unpack_from("<I", raw, p + 24)[0]
        nlen = struct.unpack_from("<H", raw, p + 28)[0]
        elen = struct.unpack_from("<H", raw, p + 30)[0]
        clen = struct.unpack_from("<H", raw, p + 32)[0]
        name_bytes = raw[p + 46:p + 46 + nlen]
        name = name_bytes.decode("utf-8", "replace") if (flags & 0x800) \
            else name_bytes.decode("cp437", "replace")
        members.append({"name": name,
                        "kind": "stored" if method == 0 else "deflate",
                        "method": method, "crc32": crc,
                        "compressed_size": csize, "uncompressed_size": usize})
        p += 46 + nlen + elen + clen
    return members


def _tar_members(raw):
    import tarfile
    tf = tarfile.open(fileobj=io.BytesIO(raw))
    members = []
    for m in tf.getmembers():
        if m.isfile():
            kind = "file"
        elif m.isdir():
            kind = "directory"
        else:
            kind = "other"
        size = m.size if m.isfile() else 0
        members.append({"name": m.name, "kind": kind, "method": 0, "crc32": 0,
                        "compressed_size": size, "uncompressed_size": size})
    return members


def _model_container(raw, name):
    dialect = "zip" if raw[:2] == b"PK" else "tar"
    members = _zip_members(raw) if dialect == "zip" else _tar_members(raw)
    return {"_format": "container", "dialect": dialect, "members": members}


def _q_container(lane, model, q, plan):
    members = model.get("members", [])
    idx = plan.get("entry", 0)
    if q == "Q1":
        if idx >= len(members):
            return decl(q, "missing", "no such member")
        return _laneless(q, {"kind": "string", "value": members[idx]["name"]})
    if q == "Q2":
        return decl(q, "no-source-span", "a conventional container load keeps no span")
    if q == "Q3":
        return _laneless(q, model.get("dialect"))
    if q == "Q4":
        return _laneless(q, len(members))
    if q == "Q5":
        if idx >= len(members):
            return decl(q, "missing", "no such member")
        m = members[idx]
        return _laneless(q, {"name": m["name"], "kind": m["kind"],
                             "method": m["method"],
                             "compressed_size": m["compressed_size"],
                             "uncompressed_size": m["uncompressed_size"]})
    if q == "Q7":
        return decl(q, "no-source-span", "a conventional container load keeps no span")
    if q == "Q8":
        return decl(q, "no-source-bytes", "a conventional container load keeps no member bytes")
    if q == "Q9":
        if idx >= len(members):
            return decl(q, "missing", "no such member")
        return _laneless(q, members[idx]["crc32"])
    if q == "Q10":
        if idx >= len(members):
            return decl(q, "missing", "no such member")
        return _laneless(q, members[idx]["method"])
    if q == "Q11":
        if lane == "conv":
            return decl(q, "format-not-recorded", "a conventional load records no format")
        return _laneless(q, "container")
    if q == "Q12":
        return decl(q, "no-source-span", "a conventional container load keeps no span")
    return decl(q, "unknown-question", q)


# --- scientific container (NetCDF classic; HDF5 deliberately absent) --------

_NC_TYPES = {1: ("byte", 1), 2: ("char", 1), 3: ("int16", 2), 4: ("int32", 4),
             5: ("float32", 4), 6: ("float64", 8)}


def _model_netcdf(raw):
    import struct
    if raw[:4] not in (b"CDF\x01", b"CDF\x02"):
        return None
    b = raw
    off = 8 if raw[:4] == b"CDF\x02" else 4
    p = [4]

    def u32():
        v = struct.unpack_from(">I", b, p[0])[0]
        p[0] += 4
        return v

    def nonneg():
        if off == 8:
            v = struct.unpack_from(">Q", b, p[0])[0]
        else:
            v = struct.unpack_from(">I", b, p[0])[0]
        p[0] += off
        return v

    def name_str():
        n = u32()
        s = b[p[0]:p[0] + n]
        p[0] += n
        return s.rstrip(b"\x00").decode("utf-8", "replace")

    def read_tagged():
        # A list is `tag nelems` when present, or the single ZERO word when ABSENT.
        tag = u32()
        if tag in (0x0A, 0x0B, 0x0C):
            return tag, u32()
        return 0, 0

    numrecs = nonneg()
    dims = []
    tag, n = read_tagged()
    if tag == 0x0A:
        for _ in range(n):
            nm = name_str()
            dims.append({"name": nm, "length": nonneg()})
    tag, n = read_tagged()
    if tag == 0x0C:
        for _ in range(n):
            name_str()
            typ = u32()
            nv = nonneg()
            sz = _NC_TYPES.get(typ, ("?", 1))[1] * nv
            p[0] += (sz + 3) // 4 * 4
    variables = []
    tag, n = read_tagged()
    if tag == 0x0B:
        for _ in range(n):
            nm = name_str()
            nd = u32()
            dimids = [u32() for _ in range(nd)]
            atag = u32()
            if atag == 0x0C:
                an = u32()
                for _ in range(an):
                    name_str()
                    typ = u32()
                    nv = nonneg()
                    sz = _NC_TYPES.get(typ, ("?", 1))[1] * nv
                    p[0] += (sz + 3) // 4 * 4
            typ = u32()
            nonneg()  # vsize
            nonneg()  # begin
            shape = [dims[d]["length"] for d in dimids if d < len(dims)]
            variables.append({"name": nm, "type": _NC_TYPES.get(typ, ("?", 0))[0],
                              "shape": shape})
    return {"_format": "hdf", "dialect": "netcdf", "dims": dims,
            "objects": [{"path": "/" + v["name"], "kind": "dataset",
                         "type": v["type"], "shape": v["shape"]}
                        for v in variables]}


def _model_hdf(raw, name):
    nc = _model_netcdf(raw)
    if nc is not None:
        return nc
    dialect = "hdf5" if raw[:8] == b"\x89HDF\r\n\x1a\n" or raw[512:520] == b"\x89HDF\r\n\x1a\n" else None
    return {"_format": "hdf", "dialect": dialect, "dims": [], "objects": []}


def _q_hdf(lane, model, q, plan):
    if model.get("dialect") != "netcdf":
        return decl(q, "no-faithful-extraction",
                    "no faithful conventional extraction exists for this fixture")
    objects = model.get("objects", [])
    idx = plan.get("object", 0)
    if q == "Q1":
        if idx >= len(objects):
            return decl(q, "missing", "no such object")
        return _laneless(q, {"kind": "string", "value": objects[idx]["path"]})
    if q == "Q2":
        return decl(q, "no-source-span", "a conventional NetCDF load keeps no span")
    if q == "Q3":
        return _laneless(q, "netcdf")
    if q == "Q4":
        return _laneless(q, len(objects))
    if q == "Q5":
        if idx >= len(objects):
            return decl(q, "missing", "no such object")
        o = objects[idx]
        return _laneless(q, {"path": o["path"], "kind": o["kind"],
                             "type": o["type"], "shape": o["shape"]})
    if q == "Q7":
        return decl(q, "no-source-span", "a conventional NetCDF load keeps no span")
    if q == "Q8":
        return decl(q, "no-source-bytes", "a conventional NetCDF load keeps no data bytes")
    if q == "Q9":
        dim = plan.get("dim")
        dims = model.get("dims", [])
        if dim is None or dim >= len(dims):
            return decl(q, "missing", "no such dimension")
        return _laneless(q, {"kind": "string", "value": dims[dim]["name"]})
    if q == "Q10":
        return decl(q, "no-faithful-attr-order",
                    "a conventional NetCDF load does not reconstruct the adapter's attribute order")
    if q == "Q11":
        if lane == "conv":
            return decl(q, "format-not-recorded", "a conventional load records no format")
        return _laneless(q, "hdf")
    if q == "Q12":
        return decl(q, "no-source-span", "a conventional NetCDF load keeps no span")
    return decl(q, "unknown-question", q)


# --- CFBF/OLE2 (Lane C deliberately absent) ---------------------------------

def _model_cfbf(raw, name):
    return {"_format": "cfbf", "dialect": None}


def _q_cfbf(lane, model, q, plan):
    return decl(q, "no-faithful-extraction",
                "no faithful conventional extraction exists for a CFBF compound file")


# --- the four legacy CFBF-backed Office formats (Phase 27.4) -----------------
#
# Lane B (the source-retaining SQLite store) keeps the original bytes verbatim
# (byte authority, so it answers Q6) and records the detected format, but there
# is no faithful Python-stdlib decoder for `.doc`/`.xls`/`.ppt`/`.msg` in the
# pinned image, so every *structural* question is a typed decline. Lane C is
# `comparator_absent` (see `C_ABSENT_NOTE`).

def _model_doc(raw, name):
    return {"_format": "doc", "dialect": None}


def _q_doc(lane, model, q, plan):
    return _q_legacy(lane, q, "doc")


def _model_xls(raw, name):
    return {"_format": "xls", "dialect": None}


def _q_xls(lane, model, q, plan):
    return _q_legacy(lane, q, "xls")


def _model_ppt(raw, name):
    return {"_format": "ppt", "dialect": None}


def _q_ppt(lane, model, q, plan):
    return _q_legacy(lane, q, "ppt")


def _model_msg(raw, name):
    return {"_format": "msg", "dialect": None}


def _q_msg(lane, model, q, plan):
    return _q_legacy(lane, q, "msg")


def _q_legacy(lane, q, fmt):
    # The only cheaply recoverable fact from the retained bytes is the recorded
    # format; every structural question needs a faithful decoder that does not
    # exist in the pinned image. Never fabricate one.
    if q == "Q11":
        return _laneless(q, fmt)
    return decl(q, "no-faithful-extraction",
                "no faithful conventional extraction exists for a legacy %s file"
                % fmt)


_QNEW = {
    "mhtml": _q_mhtml, "logstream": _q_logstream, "pkgmeta": _q_pkgmeta,
    "apispec": _q_apispec, "bib": _q_bib, "rtf": _q_rtf,
    "container": _q_container, "hdf": _q_hdf, "cfbf": _q_cfbf,
    "doc": _q_doc, "xls": _q_xls, "ppt": _q_ppt, "msg": _q_msg,
}


def _q_new(fmt, lane, d, q, plan):
    model = _read_new_model(lane, d)
    if lane == "conv" and _conv_absent(fmt, model.get("_name", "")):
        return decl(q, "comparator_absent", C_ABSENT_NOTE[fmt][1])
    return _QNEW[fmt](lane, model, q, plan)


# ===========================================================================
# CLI
# ===========================================================================

def main(argv=None):
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)

    for name in ("plan", "version"):
        p = sub.add_parser(name)
        p.add_argument("--format", required=True, choices=FORMATS)

    b = sub.add_parser("build")
    b.add_argument("--format", required=True, choices=FORMATS)
    b.add_argument("--lane", required=True, choices=["sqlite", "conv", "duckdb"])
    b.add_argument("--source", required=True)
    b.add_argument("--out", required=True)

    q = sub.add_parser("query")
    q.add_argument("--format", required=True, choices=FORMATS)
    q.add_argument("--lane", required=True, choices=["sqlite", "conv", "duckdb"])
    q.add_argument("--dir", required=True)
    q.add_argument("--q", required=True)
    q.add_argument("--plan", default="{}")
    q.add_argument("--out", required=True)

    s = sub.add_parser("session")
    s.add_argument("--format", required=True, choices=FORMATS)
    s.add_argument("--lane", required=True, choices=["sqlite", "conv", "duckdb"])
    s.add_argument("--dir", required=True)
    s.add_argument("--queries", required=True)
    s.add_argument("--plan", default="{}")
    s.add_argument("--out", required=True)

    m = sub.add_parser("materialize")
    m.add_argument("--format", required=True, choices=FORMATS)
    m.add_argument("--lane", required=True, choices=["sqlite", "conv"])
    m.add_argument("--dir", required=True)
    m.add_argument("--out", required=True)

    a = sub.add_parser("aggregate")
    a.add_argument("--format", required=True, choices=FORMATS)
    a.add_argument("--raw", required=True)
    a.add_argument("--campaign", required=True)
    a.add_argument("--env", default=None)

    ns = ap.parse_args(argv)
    if ns.cmd == "plan":
        print(json.dumps(plan_dump(ns.format), sort_keys=True))
        return 0
    if ns.cmd == "version":
        return version(ns.format)
    if ns.cmd == "build":
        return build(ns.format, ns.lane, ns.source, ns.out)
    if ns.cmd == "query":
        return query(ns.format, ns.lane, ns.dir, ns.q, json.loads(ns.plan), ns.out)
    if ns.cmd == "session":
        return session(ns.format, ns.lane, ns.dir, ns.queries, json.loads(ns.plan), ns.out)
    if ns.cmd == "materialize":
        return materialize(ns.format, ns.lane, ns.dir, ns.out)
    if ns.cmd == "aggregate":
        return aggregate(ns.format, ns.raw, ns.campaign, ns.env)
    return 2


if __name__ == "__main__":
    sys.exit(main())
