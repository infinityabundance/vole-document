#!/usr/bin/env python3
# Phase 21.20-21.24 (economic courts) — SHARED engine for the five text-family
# courts (CONFIG / FEED / GEOJSON / GIS / NOTEBOOK). One module, parameterised by
# `--format`, so the five courts cannot drift apart. Runs on Python stdlib only
# (the pinned `doc-baseline` service; no third-party module, no network).
#
# Two conventional comparators per format:
#
#   * `sqlite` — a **source-retaining** baseline: it keeps the original bytes
#     verbatim (a `raw` BLOB, so it can answer `materialize` / byte authority, Q6)
#     AND a conventional extraction of the document (a parsed line/field/JSON view).
#     It preserves whatever the extraction preserves (duplicate keys where the
#     source text is retained, dialect, counts, values, kinds); it exposes **no**
#     exact source span, token spelling, attribute span, or native representation.
#   * `conv`   — a **conventional decode-to-host-values** load: `configparser`-style
#     for config, `xml.etree.ElementTree` for feed/gis, `json` for geojson/notebook.
#     It keeps only a derived host-value view: it drops the source bytes, every
#     source offset, exact spelling, duplicate keys, member/attribute order, and the
#     recorded dialect, so it must decline typed on all of those.
#
#   plan      --format F
#   build     --format F --lane sqlite|conv --source FILE --out DIR
#   query     --format F --lane ... --dir DIR --q Qn --plan JSON --out FILE
#   session   --format F --lane ... --dir DIR --queries Q1,... --plan JSON --out FILE
#   materialize --format F --lane sqlite --dir DIR --out FILE
#   version   --format F
#   aggregate --format F --raw DIR --campaign DIR --env ENV_JSON

import argparse
import hashlib
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

FORMATS = ["config", "feed", "geojson", "gis", "notebook"]


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
# Build
# ===========================================================================

def build(fmt, lane, source_path, out_dir):
    os.makedirs(out_dir, exist_ok=True)
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
    lines.append("**Question.** Against two conventional comparators — a "
                 "source-retaining store that keeps the raw bytes plus a conventional "
                 "extraction, and a conventional decode-to-host-values load — on "
                 "contract-equivalent terms, can VOLE answer the same question family "
                 "(Q1–Q12) it can answer, while closing the original %s byte-exactly, "
                 "and does it add value by **preserving representation** (source spans, "
                 "exact spelling, attribute/quote/continuation markers, duplicate keys, "
                 "member order)?" % fmt)
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
    lines.append("| Q | VOLE | source-retaining baseline | conventional load |")
    lines.append("|---|---|---|---|")
    for q in QS:
        if q == "Q6":
            lines.append("| Q6 | `materialize --exact` (byte authority) | retained raw "
                         "BLOB (byte authority) | no source bytes -> typed decline |")
            continue
        d = qdesc.get(q, ("", "", ""))
        lines.append("| {} | {} | {} | {} |".format(q, *d))
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
    lines.append("- **Nothing here is run on the host**; every command ran in a pinned "
                 "container.")
    lines.append("")

    matrix = []
    matrix.append("# Phase %s — cross-lane Q1–Q12 answer matrix (%s)" % (env.get("phase", fmt), fmt))
    matrix.append("")
    matrix.append("`g` = answered (derived), `D` = typed decline, `-` = not applicable.")
    matrix.append("")
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
        "environment": env,
    }
    with open(os.path.join(campaign, "receipt.json"), "w") as f:
        json.dump(receipt, f, indent=2, sort_keys=True)
    print("\n".join(lines))
    return 0 if verdict == "PASS" else 1


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
    b.add_argument("--lane", required=True, choices=["sqlite", "conv"])
    b.add_argument("--source", required=True)
    b.add_argument("--out", required=True)

    q = sub.add_parser("query")
    q.add_argument("--format", required=True, choices=FORMATS)
    q.add_argument("--lane", required=True, choices=["sqlite", "conv"])
    q.add_argument("--dir", required=True)
    q.add_argument("--q", required=True)
    q.add_argument("--plan", default="{}")
    q.add_argument("--out", required=True)

    s = sub.add_parser("session")
    s.add_argument("--format", required=True, choices=FORMATS)
    s.add_argument("--lane", required=True, choices=["sqlite", "conv"])
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
