#!/usr/bin/env python3
# Phase 21.3.2 — ODS economic court: the source-retaining *SQLite* baseline and
# the campaign aggregator.
#
# ## Why this file exists
#
# The Phase-21 plan requires the ODS economic court to compare VOLE against a
# **source-retaining SQLite baseline** on *contract-equivalent terms*: the same
# escalating capability contract (C0..C5, extended for spreadsheet coordinates)
# and the same ten questions (Q1..Q10). This file implements that lane plus the
# one aggregator the court needs, so a single stdlib-only helper serves both the
# baseline and the statistics.
#
# ## The escalating contracts (ODS)
#
#   C0  the tabular core: documents, sheets, cells (value + stored formula).
#   C1  C0 + the native spreadsheet coordinate (the sheet/row/col identity).
#   C2  C1 + provenance: a basis class and a part/span for a derived answer.
#   C3  C2 + *exact original closure*: the whole workbook reconstructs to the
#       original bytes (length + SHA-256 + byte compare) from the retained blob.
#   C4  C3 + the spreadsheet semantic model: cell styles + number formats
#       (automatic and named), named ranges/expressions, merges, comments,
#       embedded resources, the ODF manifest.
#   C5  C4 + a one-session heterogeneous batch (a `session` serving many
#       observations in one resident process).
#
# ## Honesty
#
# * The baseline extracts structure with Python **stdlib** only (`zipfile` + a
#   hand-rolled byte scanner over the OpenDocument XML — no `odfpy`, no
#   third-party parser) and retains the original bytes as a BLOB. Exactness
#   (Q10) is a *validation* of the retained blob; it adds no new table.
# * Cell spans are recorded as **decoded-part byte offsets** in the main content
#   member, computed by the same tag-scan rule the VOLE adapter uses (`<`
#   through the matching `>` of the cell element), so Q8 compares the same
#   decoded bytes on both lanes.
# * `journal_mode=WAL` + `synchronous=NORMAL` is the durability contract: a
#   committed transaction survives a process crash, consistent with the
#   established A1 baseline (`tools/fixtures/phase12-baseline.py`). WAL does NOT
#   make the store crash-proof against media failure; it is the same guarantee
#   the other baselines state.
# * Declines are typed: a question the lane cannot answer is `{"declined":true,
#   "decline":{"code":..,"detail":..}}`, never a silent empty answer.
#
# ## Subcommands
#
#   build       --source S --db D --through D [--metrics M]
#   query       --db D --q Qn --plan JSON --out OUT
#   session     --db D --queries Q1,Q2,.. --plan JSON --out OUT
#   materialize --db D --out OUT
#   aggregate   --raw DIR --campaign DIR --env ENV_JSON
#   sqlgen      --out DIR

import argparse
import hashlib
import io
import json
import os
import posixpath
import re
import sqlite3
import sys
import time
import zipfile
from xml.etree import ElementTree as ET

ODS_MIMETYPE = "application/vnd.oasis.opendocument.spreadsheet"
ODS_MIMETYPE_TEMPLATE = "application/vnd.oasis.opendocument.spreadsheet-template"
CONTENT_MEMBER = "content.xml"
STYLES_MEMBER = "styles.xml"
META_MEMBER = "meta.xml"
MANIFEST_MEMBER = "META-INF/manifest.xml"
MANIFEST_NS = "{urn:oasis:names:tc:opendocument:xmlns:manifest:1.0}"

# Provenance classes shared with the rest of the repo.
CLASS_VERBATIM = "verbatim"
CLASS_DERIVED = "derived"
CLASS_HEURISTIC = "heuristic"
CLASS_UNRESOLVED = "unresolved"


# ---------------------------------------------------------------------------
# Small helpers
# ---------------------------------------------------------------------------

def sha256_hex(b):
    return hashlib.sha256(b).hexdigest()


def canonical_json_sha(obj):
    return sha256_hex(json.dumps(obj, sort_keys=True, separators=(",", ":")).encode())


def now_us():
    return int(time.monotonic() * 1_000_000)


def _a1(col, row):
    """0-based (col, row) -> A1 reference."""
    n = col + 1
    letters = ""
    while n:
        n, r = divmod(n - 1, 26)
        letters = chr(65 + r) + letters
    return f"{letters}{row + 1}"


def a1_to_rowcol(ref):
    """A1-style reference -> (row0, col0)."""
    ref = ref.replace("$", "")
    i = 0
    while i < len(ref) and ref[i].isalpha():
        i += 1
    if i == 0 or i == len(ref):
        return None
    col = 0
    for ch in ref[:i]:
        col = col * 26 + (ord(ch.upper()) - 64)
    try:
        row = int(ref[i:])
    except ValueError:
        return None
    return row - 1, col - 1


def parse_cell_position(s):
    """Mirror `parse_cell_position` in `src/adapter/ods.rs`: `row:col` (0-based)
    or A1-style. Returns (col, row) or None."""
    if ":" in s:
        r, c = s.split(":", 1)
        try:
            return int(c), int(r)
        except ValueError:
            pass
    rc = a1_to_rowcol(s)
    if rc is None:
        return None
    return rc[1], rc[0]


def _split_address(addr):
    """`Sheet.A1` (or `$Sheet.$A$1`) -> (sheet_name, (row0, col0)); None on fail."""
    if not addr:
        return None
    addr = addr.replace("$", "")
    if "." not in addr:
        return None
    sheet, cell = addr.rsplit(".", 1)
    rc = a1_to_rowcol(cell)
    if rc is None:
        return None
    return sheet, rc


def _range_contains(addr, sheet_name, row, col):
    """True if the named-range address (a `A.x:B.y` string) is on `sheet_name` and
    contains (row, col)."""
    if not addr:
        return False
    addr = addr.replace("$", "")
    if ":" in addr:
        a, b = addr.split(":", 1)
    else:
        a = b = addr
    pa = _split_address(a)
    pb = _split_address(b)
    if pa is None or pb is None:
        return False
    if pa[0] != sheet_name or pb[0] != sheet_name:
        return False
    (ra, ca), (rb, cb) = pa[1], pb[1]
    return min(ra, rb) <= row <= max(ra, rb) and min(ca, cb) <= col <= max(ca, cb)


def _ref_in_sheet(addr, sheet_name):
    """True if the address names `sheet_name` (base cell or range)."""
    return sheet_name in _address_sheets(addr)


def _address_sheets(addr):
    """Every sheet name an address (`Sheet.A1` or `A.x:B.y`) names, in order."""
    if not addr:
        return []
    out = []
    for part in addr.replace("$", "").split(":"):
        sa = _split_address(part)
        if sa is not None and sa[0] not in out:
            out.append(sa[0])
    return out


def _range_bounds(addr):
    """`A.x:B.y` -> (sheet_name, r0, c0, r1, c1) inclusive, or None."""
    if not addr:
        return None
    addr = addr.replace("$", "")
    parts = addr.split(":")
    if len(parts) == 1:
        parts = [parts[0], parts[0]]
    pa = _split_address(parts[0])
    pb = _split_address(parts[1])
    if pa is None or pb is None or pa[0] != pb[0]:
        return None
    (ra, ca), (rb, cb) = pa[1], pb[1]
    return (pa[0], min(ra, rb), min(ca, cb), max(ra, rb), max(ca, cb))


def _typed_value(cell):
    """The format-native typed value the adapter models: for the value-type, the
    corresponding `office:*` attribute (or the string cell's text)."""
    vt = cell.get("value_type")
    if vt in ("float", "percentage", "currency"):
        return cell.get("value")
    if vt == "boolean":
        return cell.get("boolean_value")
    if vt in ("date", "time"):
        return cell.get("date_value")
    if vt == "string":
        sv = cell.get("string_value")
        return sv if sv is not None else cell.get("text")
    return None


# ---------------------------------------------------------------------------
# OpenDocument XML byte scanner (mirrors the adapter's spans + model)
# ---------------------------------------------------------------------------

_ENT = {"amp": "&", "lt": "<", "gt": ">", "quot": '"', "apos": "'"}


def _unescape(s):
    if "&" not in s:
        return s
    out = []
    i = 0
    while i < len(s):
        if s[i] == "&":
            j = s.find(";", i)
            if j != -1:
                ent = s[i + 1:j]
                if ent in _ENT:
                    out.append(_ENT[ent])
                    i = j + 1
                    continue
                if ent.startswith("#x") or ent.startswith("#X"):
                    try:
                        out.append(chr(int(ent[2:], 16)))
                        i = j + 1
                        continue
                    except ValueError:
                        pass
                elif ent.startswith("#"):
                    try:
                        out.append(chr(int(ent[1:])))
                        i = j + 1
                        continue
                    except ValueError:
                        pass
        out.append(s[i])
        i += 1
    return "".join(out)


def _local(name):
    return name.rsplit(":", 1)[-1]


def _parse_attrs(blob):
    out = []
    i, n = 0, len(blob)
    while i < n:
        while i < n and blob[i:i + 1] in (b" ", b"\t", b"\r", b"\n", b"/"):
            i += 1
        if i >= n:
            break
        j = i
        while j < n and blob[j:j + 1] != b"=":
            j += 1
        if j >= n:
            break
        name = blob[i:j].decode("utf-8", "replace").strip()
        k = j + 1
        while k < n and blob[k:k + 1] in (b" ", b"\t", b"\r", b"\n"):
            k += 1
        if k >= n or blob[k:k + 1] not in (b'"', b"'"):
            i = j + 1
            continue
        quote = blob[k:k + 1]
        e = blob.find(quote, k + 1)
        if e < 0:
            e = n
        val = blob[k + 1:e].decode("utf-8", "replace")
        if name:
            out.append((_local(name), _unescape(val)))
        i = e + 1
    return out


def _iter_events(data):
    """Yield text chunks (``("text", str)``) and tag events
    (``("tag", start, end, name, attrs, kind)``). `start` is the offset of the
    opening ``<`` and `end` the offset just past the closing ``>``."""
    i, n = 0, len(data)
    while i < n:
        lt = data.find(b"<", i)
        if lt < 0:
            chunk = data[i:]
            if chunk:
                yield ("text", chunk.decode("utf-8", "replace"))
            return
        if lt > i:
            yield ("text", data[i:lt].decode("utf-8", "replace"))
        nxt = data[lt + 1:lt + 2]
        if nxt == b"!":
            if data[lt + 1:lt + 3] == b"!--":
                e = data.find(b"-->", lt + 3)
                i = (e + 3) if e >= 0 else n
            else:
                e = data.find(b">", lt + 2)
                i = (e + 1) if e >= 0 else n
            continue
        if nxt == b"?":
            e = data.find(b"?>", lt + 2)
            i = (e + 2) if e >= 0 else n
            continue
        j = lt + 1
        is_end = data[j:j + 1] == b"/"
        if is_end:
            j += 1
        k = j
        while k < n and data[k:k + 1] not in (b" ", b"\t", b"\r", b"\n", b">", b"/"):
            k += 1
        name = data[j:k].decode("utf-8", "replace")
        m = k
        q = 0
        while m < n:
            c = data[m:m + 1]
            if q:
                if c == q:
                    q = 0
            elif c in (b'"', b"'"):
                q = c
            elif c == b">":
                break
            m += 1
        tag_end = m + 1
        self_close = (not is_end) and data[m - 1:m] == b"/"
        kind = "end" if is_end else ("empty" if self_close else "start")
        yield ("tag", lt, tag_end, name, _parse_attrs(data[k:m]), kind)
        i = tag_end


def _parse_part(data, mode):
    """Parse one OpenDocument part (content or styles) into the model."""
    model = {
        "sheets": [],
        "styles": [],
        "number_formats": [],
        "named_expressions": [],
        "comments": [],
        "merges": 0,
    }
    stack = []

    def in_cell():
        return any(k == "cell" for k, _ in stack)

    def nearest_style():
        for k, o in reversed(stack):
            if k == "style":
                return o
        return None

    def nearest_para():
        for k, o in reversed(stack):
            if k == "para":
                return o
        return None

    def nearest_annotation():
        for k, o in reversed(stack):
            if k == "annotation":
                return o
        return None

    def nearest_cell():
        for k, o in reversed(stack):
            if k == "cell":
                return o
        return None

    meta_kind = None
    meta_buf = []
    cur_table = None
    cur_row = None
    cur_cell = None
    cur_annotation = None

    def push_text(s):
        if not s.strip():
            return
        ann = nearest_annotation()
        para = nearest_para()
        cell = nearest_cell()
        if para is not None:
            para["text"] += s
        elif ann is not None:
            ann["raw"] += s
        elif cell is not None:
            cell["raw"] += s

    def start(name, attrs, pos_start):
        nonlocal meta_kind, meta_buf, cur_table, cur_row, cur_cell, cur_annotation
        local = _local(name)
        a = dict(attrs)
        if local in ("table-cell", "covered-table-cell") and stack and stack[-1][0] == "row":
            cur_cell = {
                "created": True,
                "covered": local == "covered-table-cell",
                "repeat": _int(a.get("number-columns-repeated"), 1),
                "col_span": max(_int(a.get("number-columns-spanned"), 1), 1),
                "row_span": max(_int(a.get("number-rows-spanned"), 1), 1),
                "value_type": a.get("value-type"),
                "value": a.get("value"),
                "boolean_value": a.get("boolean-value"),
                "date_value": a.get("date-value"),
                "string_value": a.get("string-value"),
                "formula": a.get("formula"),
                "style_name": a.get("style-name"),
                "paragraphs": [],
                "raw": "",
                "span_start": pos_start,
                "annotation": None,
            }
            if cur_cell["col_span"] > 1 or cur_cell["row_span"] > 1:
                model["merges"] += 1
            stack.append(("cell", cur_cell))
        elif local == "spreadsheet":
            stack.append(("spreadsheet", None))
        elif local == "table" and stack and stack[-1][0] == "spreadsheet":
            cur_table = {
                "name": a.get("name", ""),
                "display": a.get("display") != "false",
                "style_name": a.get("style-name"),
                "rows": [],
            }
            stack.append(("table", cur_table))
        elif local == "table-row" and stack and stack[-1][0] == "table":
            cur_row = {"repeat": _int(a.get("number-rows-repeated"), 1), "cells": []}
            stack.append(("row", cur_row))
        elif local == "annotation" and in_cell():
            cur_annotation = {"author": None, "date": None, "paragraphs": [], "raw": ""}
            stack.append(("annotation", cur_annotation))
        elif local in ("creator", "date"):
            meta_kind = local
            meta_buf = []
            stack.append(("meta", None))
        elif local == "named-expressions":
            stack.append(("namedexprs", None))
        elif local in ("named-range", "named-expression") and mode == "content":
            model["named_expressions"].append(
                {
                    "name": a.get("name", ""),
                    "kind": "range" if local == "named-range" else "expression",
                    "base_cell_address": a.get("base-cell-address"),
                    "cell_range_address": a.get("cell-range-address"),
                    "expression": a.get("expression"),
                }
            )
            stack.append(("other", None))
        elif local == "style" and a.get("family") == "table-cell":
            stack.append(("style", {
                "name": a.get("name", ""),
                "family": "table-cell",
                "parent": a.get("parent-style-name"),
                "data_style": a.get("data-style-name"),
                "table_cell_properties": {},
                "text_properties": {},
            }))
        elif local in ("table-cell-properties", "text-properties"):
            st = nearest_style()
            if st is not None:
                key = "table_cell_properties" if local == "table-cell-properties" else "text_properties"
                st[key] = dict(attrs)
            stack.append(("other", None))
        elif local in (
            "number-style", "date-style", "time-style", "currency-style",
            "percentage-style", "boolean-style", "text-style",
        ):
            model["number_formats"].append(
                {"name": a.get("name", ""), "kind": local[: -len("-style")]}
            )
            stack.append(("other", None))
        elif local in ("automatic-styles", "styles"):
            stack.append(("autostyles", None))
        elif local in ("p", "h"):
            stack.append(("para", {"text": ""}))
        else:
            stack.append(("other", None))

    def end(local, pos_after):
        nonlocal cur_table, cur_row, cur_cell, cur_annotation, meta_kind
        if local in ("table-cell", "covered-table-cell"):
            top = stack.pop() if stack else ("other", None)
            cell = top[1] if top[0] == "cell" else None
            if cell is None:
                return
            cell["span_len"] = max(pos_after - cell["span_start"], 0)
            if cur_row is not None:
                for _ in range(cell["repeat"]):
                    grid_col = len(cur_row["cells"])
                    cur_row["cells"].append(_finish_cell(cell, grid_col))
            cur_cell = None
        elif local == "table-row":
            top = stack.pop() if stack else ("other", None)
            row = top[1] if top[0] == "row" else None
            if row is not None and cur_table is not None:
                for _ in range(row["repeat"]):
                    idx = len(cur_table["rows"])
                    cur_table["rows"].append({"index": idx, "cells": list(row["cells"])})
            cur_row = None
        elif local == "table":
            top = stack.pop() if stack else ("other", None)
            tbl = top[1] if top[0] == "table" else None
            if tbl is not None:
                model["sheets"].append(tbl)
            cur_table = None
        elif local in ("p", "h"):
            top = stack.pop() if stack else ("other", None)
            para = top[1] if top[0] == "para" else None
            if para is not None:
                ann = nearest_annotation()
                if ann is not None:
                    ann["paragraphs"].append(para["text"])
                else:
                    cell = nearest_cell()
                    if cell is not None:
                        cell["paragraphs"].append(para["text"])
        elif local == "annotation":
            top = stack.pop() if stack else ("other", None)
            ann = top[1] if top[0] == "annotation" else None
            cell = nearest_cell()
            if ann is not None and cell is not None:
                cell["annotation"] = ann
            cur_annotation = None
        elif local in ("creator", "date"):
            top = stack.pop() if stack else ("other", None)
            if meta_kind is not None and cur_annotation is not None:
                txt = "".join(meta_buf).strip()
                if meta_kind == "creator":
                    cur_annotation["author"] = txt or None
                else:
                    cur_annotation["date"] = txt or None
            meta_kind = None
        elif local == "style":
            top = stack.pop() if stack else ("other", None)
            if top[0] == "style" and top[1] is not None:
                model["styles"].append(top[1])
        else:
            if stack:
                stack.pop()

    for ev in _iter_events(data):
        if ev[0] == "text":
            s = ev[1]
            if meta_kind is not None:
                meta_buf.append(s)
            else:
                push_text(s)
            continue
        _, start_pos, end_pos, name, attrs, kind = ev
        local = _local(name)
        if kind == "start":
            start(name, attrs, start_pos)
        elif kind == "empty":
            start(name, attrs, start_pos)
            end(local, end_pos)
        else:
            end(local, end_pos)

    # finalize cell text
    for sh in model["sheets"]:
        for row in sh["rows"]:
            for c in row["cells"]:
                _finalize_text(c)

    # Comments are hoisted by `extract` once every cell has its A1 ref.
    return model


def _int(v, default):
    try:
        n = int(v)
    except (TypeError, ValueError):
        return default
    return n if n >= 1 else default


def _finalize_text(cell):
    if cell["paragraphs"]:
        cell["text"] = "\n".join(cell["paragraphs"])
    else:
        cell["text"] = cell["raw"]


def _annotation_text(ann):
    if ann.get("paragraphs"):
        return "\n".join(ann["paragraphs"])
    return ann.get("raw", "")


def _finish_cell(src, grid_col):
    c = {
        "covered": src["covered"],
        "col_span": src["col_span"],
        "row_span": src["row_span"],
        "value_type": src["value_type"],
        "value": src["value"],
        "boolean_value": src["boolean_value"],
        "date_value": src["date_value"],
        "string_value": src["string_value"],
        "formula": src["formula"],
        "style_name": src["style_name"],
        "text": "",
        "raw": src["raw"],
        "paragraphs": list(src["paragraphs"]),
        "span_start": src["span_start"],
        "span_len": src["span_len"],
        "annotation": src["annotation"],
    }
    return c


# ---------------------------------------------------------------------------
# Extraction (stdlib zipfile + the scanner above)
# ---------------------------------------------------------------------------

def _local_header_span(buf, zinfo):
    """The raw byte span (data_offset, data_offset+compress_size) of a member
    inside the source ZIP (mirrors VOLE's `source_span` for a raw member)."""
    off = zinfo.header_offset
    if buf[off:off + 4] != b"PK\x03\x04":
        return None
    name_len = int.from_bytes(buf[off + 26:off + 28], "little")
    extra_len = int.from_bytes(buf[off + 28:off + 30], "little")
    data_off = off + 30 + name_len + extra_len
    return [data_off, data_off + zinfo.compress_size]


def parse_manifest(data):
    entries = []
    if data is None:
        return entries
    root = ET.fromstring(data)
    for e in root:
        if e.tag != MANIFEST_NS + "file-entry":
            continue
        entries.append(
            {
                "full_path": e.get(MANIFEST_NS + "full-path", ""),
                "media_type": e.get(MANIFEST_NS + "media-type", ""),
                "version": e.get(MANIFEST_NS + "version"),
            }
        )
    return entries


def extract(source):
    """Parse an ODS package into the baseline's structured model."""
    with open(source, "rb") as f:
        buf = f.read()
    zf = zipfile.ZipFile(io.BytesIO(buf))
    infos = zf.infolist()
    ordinal = {}
    part_spans = {}
    raw = {}
    for i, zi in enumerate(infos):
        ordinal[zi.filename] = i
        raw[zi.filename] = zi
        part_spans[zi.filename] = _local_header_span(buf, zi)

    def read(name):
        try:
            return zf.read(name)
        except KeyError:
            return None

    manifest = parse_manifest(read(MANIFEST_MEMBER))
    for e in manifest:
        fp = e["full_path"].lstrip("/")
        e["ordinal"] = ordinal.get(fp, None)

    # Semantic main-part discovery, mirroring the adapter: the manifest's
    # `content.xml` entry, else a non-root OpenDocument-spreadsheet entry.
    content_member = None
    for e in manifest:
        if e["full_path"].lstrip("/") == CONTENT_MEMBER:
            content_member = CONTENT_MEMBER
            break
    if content_member is None:
        for e in manifest:
            mt = e["media_type"]
            if e["full_path"] != "/" and mt in (ODS_MIMETYPE, ODS_MIMETYPE_TEMPLATE):
                content_member = e["full_path"].lstrip("/")
                break
    if content_member is None:
        raise ValueError("ODF package has no resolvable spreadsheet content part")
    if content_member not in ordinal:
        raise ValueError("ODF content part does not resolve to a package member")

    styles_member = STYLES_MEMBER if STYLES_MEMBER in ordinal else None
    meta_member = META_MEMBER if META_MEMBER in ordinal else None

    mimetype_bytes = read("mimetype")
    mimetype = mimetype_bytes.decode("ascii", "replace") if mimetype_bytes is not None else None

    content_bytes = read(content_member)
    content_model = _parse_part(content_bytes, "content")
    styles_model = _parse_part(read(styles_member), "styles") if styles_member else {
        "sheets": [], "styles": [], "number_formats": [], "named_expressions": [], "comments": [], "merges": 0,
    }

    # name every cell (grid -> A1) and flatten
    sheets = []
    for si, sh in enumerate(content_model["sheets"]):
        rows = []
        for row in sh["rows"]:
            cells = []
            for c in row["cells"]:
                c = dict(c)
                c["ref"] = _a1(c.get("grid_col", len(cells)), row["index"])
                c["row"] = row["index"]
                c["col"] = c.get("grid_col", len(cells))
                _finalize_text(c)
                cells.append(c)
            rows.append({"index": row["index"], "cells": cells})
        sheets.append(
            {
                "index": si,
                "name": sh["name"],
                "display": sh["display"],
                "style_name": sh["style_name"],
                "rows": rows,
                "part": content_member,
                "part_ordinal": ordinal[content_member],
            }
        )

    # comments: re-hoist with final refs
    comments = []
    for sh in sheets:
        for row in sh["rows"]:
            for c in row["cells"]:
                ann = c.get("annotation")
                if ann is not None:
                    comments.append(
                        {
                            "sheet": sh["index"],
                            "ref": c["ref"],
                            "row": c["row"],
                            "col": c["col"],
                            "author": ann.get("author"),
                            "date": ann.get("date"),
                            "text": _annotation_text(ann),
                        }
                    )

    # embedded resources: manifest entries with a binary (non-XML) media type
    resources = []
    for e in manifest:
        fp = e["full_path"].lstrip("/")
        mt = e["media_type"]
        if e["full_path"] == "/":
            continue
        if mt.startswith("image/"):
            data = read(fp) or b""
            resources.append(
                {
                    "ordinal": e.get("ordinal"),
                    "name": fp,
                    "media_type": mt,
                    "sha256": sha256_hex(data),
                    "len": len(data),
                }
            )

    content_media_type = next(
        (e["media_type"] for e in manifest if e["full_path"].lstrip("/") == content_member), "text/xml"
    )

    return {
        "source_len": len(buf),
        "source_sha256": sha256_hex(buf),
        "mimetype": mimetype,
        "manifest": manifest,
        "content_part": content_member,
        "content_ordinal": ordinal[content_member],
        "content_media_type": content_media_type,
        "styles_part": styles_member,
        "meta_part": meta_member,
        "sheets": sheets,
        "styles_auto": content_model["styles"],
        "styles_named": styles_model["styles"],
        "number_formats": content_model["number_formats"] + styles_model["number_formats"],
        "named_expressions": content_model["named_expressions"],
        "comments": comments,
        "merges": content_model["merges"],
        "resources": resources,
        "part_spans": {"/" + k: v for k, v in part_spans.items()},
        "content_bytes": content_bytes,
    }


# ---------------------------------------------------------------------------
# Escalating SQLite contract store
# ---------------------------------------------------------------------------

def _schema(through):
    s = """
PRAGMA journal_mode=WAL;
PRAGMA synchronous=NORMAL;
CREATE TABLE documents(
  doc_id INTEGER PRIMARY KEY, format TEXT NOT NULL, path TEXT NOT NULL,
  source_len INTEGER NOT NULL, source_sha256 TEXT NOT NULL,
  mimetype TEXT, content_part TEXT, content_ordinal INTEGER, content_media_type TEXT);
CREATE TABLE sheets(
  sheet_id INTEGER PRIMARY KEY, doc_id INTEGER NOT NULL, ord INTEGER NOT NULL,
  name TEXT, display INTEGER, style_name TEXT, rows INTEGER, cells INTEGER,
  part TEXT, part_ordinal INTEGER);
CREATE TABLE cells(
  cell_id INTEGER PRIMARY KEY, doc_id INTEGER NOT NULL, sheet INTEGER NOT NULL,
  row INTEGER NOT NULL, col INTEGER NOT NULL, ref TEXT NOT NULL, covered INTEGER,
  value_type TEXT, value TEXT, boolean_value TEXT, date_value TEXT, string_value TEXT,
  style_name TEXT, stored_formula TEXT, text TEXT, span_start INTEGER, span_len INTEGER);
CREATE INDEX idx_cells_sheet ON cells(doc_id, sheet, row, col);
CREATE INDEX idx_cells_ref ON cells(doc_id, sheet, ref);
CREATE TABLE merges(doc_id INTEGER, sheet INTEGER, ref TEXT);
"""
    if through >= 2:
        s += """
CREATE TABLE provenance(
  doc_id INTEGER, kind TEXT, key TEXT, class TEXT, part TEXT, span_start INTEGER, span_end INTEGER);
CREATE INDEX idx_prov ON provenance(doc_id, kind, key);
"""
    if through >= 4:
        s += """
CREATE TABLE styles(
  doc_id INTEGER, scope TEXT, name TEXT, family TEXT, parent TEXT, data_style TEXT,
  tcp_json TEXT, tp_json TEXT);
CREATE TABLE number_formats(doc_id INTEGER, name TEXT, kind TEXT);
CREATE TABLE named_expressions(
  doc_id INTEGER, name TEXT, kind TEXT, base_cell_address TEXT, cell_range_address TEXT, expression TEXT);
CREATE TABLE comments(
  doc_id INTEGER, sheet INTEGER, ref TEXT, row INTEGER, col INTEGER, author TEXT, date TEXT, text TEXT);
CREATE TABLE resources(
  doc_id INTEGER, ordinal INTEGER, name TEXT, media_type TEXT, member_sha256 TEXT, member_len INTEGER);
"""
    if through >= 3:
        s += "CREATE TABLE source_blob(doc_id INTEGER PRIMARY KEY, payload BLOB NOT NULL);\n"
    return s


def build(source, db_path, through):
    for suffix in ("", "-wal", "-shm"):
        try:
            os.remove(db_path + suffix)
        except FileNotFoundError:
            pass
    t0 = now_us()
    doc = extract(source)
    extract_us = now_us() - t0
    con = sqlite3.connect(db_path)
    con.executescript(_schema(through))
    cur = con.cursor()
    cur.execute(
        "INSERT INTO documents VALUES(1,'ods',?,?,?,?,?,?,?)",
        (source, doc["source_len"], doc["source_sha256"], doc["mimetype"], doc["content_part"],
         doc["content_ordinal"], doc["content_media_type"]),
    )
    for s in doc["sheets"]:
        cur.execute(
            "INSERT INTO sheets VALUES(?,1,?,?,?,?,?,?,?,?)",
            (s["index"] + 1, s["index"], s["name"], int(s["display"]), s["style_name"],
             len(s["rows"]), sum(len(r["cells"]) for r in s["rows"]), s["part"], s["part_ordinal"]),
        )
        for row in s["rows"]:
            for c in row["cells"]:
                cur.execute(
                    "INSERT INTO cells(doc_id,sheet,row,col,ref,covered,value_type,value,"
                    "boolean_value,date_value,string_value,style_name,stored_formula,text,"
                    "span_start,span_len) VALUES(1,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                    (s["index"], c["row"], c["col"], c["ref"], int(c["covered"]), c["value_type"],
                     c["value"], c["boolean_value"], c["date_value"], c["string_value"],
                     c["style_name"], c["formula"], c["text"], c["span_start"], c["span_len"]),
                )
        for row in s["rows"]:
            for c in row["cells"]:
                if c["col_span"] > 1 or c["row_span"] > 1:
                    r1 = c["row"] + c["row_span"] - 1
                    c1 = c["col"] + c["col_span"] - 1
                    cur.execute(
                        "INSERT INTO merges VALUES(1,?,?)",
                        (s["index"], f"{_a1(c['col'], c['row'])}:{_a1(c1, r1)}"),
                    )
    if through >= 2:
        for s in doc["sheets"]:
            span = doc["part_spans"].get("/" + (s["part"] or ""))
            cur.execute(
                "INSERT INTO provenance VALUES(1,'sheet',?,?,?,?,?)",
                (str(s["index"]), CLASS_DERIVED, s["part"], span[0] if span else None,
                 span[1] if span else None),
            )
    if through >= 4:
        for scope, styles in (("auto", doc["styles_auto"]), ("named", doc["styles_named"])):
            for st in styles:
                cur.execute(
                    "INSERT INTO styles VALUES(1,?,?,?,?,?,?,?)",
                    (scope, st["name"], st["family"], st["parent"], st["data_style"],
                     json.dumps(st["table_cell_properties"], sort_keys=True),
                     json.dumps(st["text_properties"], sort_keys=True)),
                )
        for nf in doc["number_formats"]:
            cur.execute("INSERT INTO number_formats VALUES(1,?,?)", (nf["name"], nf["kind"]))
        for ne in doc["named_expressions"]:
            cur.execute(
                "INSERT INTO named_expressions VALUES(1,?,?,?,?,?)",
                (ne["name"], ne["kind"], ne["base_cell_address"], ne["cell_range_address"],
                 ne["expression"]),
            )
        for c in doc["comments"]:
            cur.execute(
                "INSERT INTO comments VALUES(1,?,?,?,?,?,?,?)",
                (c["sheet"], c["ref"], c["row"], c["col"], c["author"], c["date"], c["text"]),
            )
        for r in doc["resources"]:
            cur.execute(
                "INSERT INTO resources VALUES(1,?,?,?,?,?)",
                (r["ordinal"], r["name"], r["media_type"], r["sha256"], r["len"]),
            )
    if through >= 3:
        with open(source, "rb") as f:
            payload = f.read()
        cur.execute("INSERT INTO source_blob VALUES(1,?)", (payload,))
    cur.execute("ANALYZE")
    con.commit()
    con.close()
    return {
        "extract_us": extract_us,
        "db_bytes": os.path.getsize(db_path),
        "through": through,
        "sheets": len(doc["sheets"]),
        "cells": sum(sum(len(r["cells"]) for r in s["rows"]) for s in doc["sheets"]),
        "named_expressions": len(doc["named_expressions"]),
        "comments": len(doc["comments"]),
        "resources": len(doc["resources"]),
        "merges": doc["merges"],
    }


# ---------------------------------------------------------------------------
# Query envelopes
# ---------------------------------------------------------------------------

def _env(q, lane, value, *, declined=False, code=None, reason="", native=True, detail=None):
    e = {"q": q, "lane": lane, "declined": bool(declined), "native": native,
         "value": None if declined else value, "detail": detail or {}}
    e["decline"] = {"code": code, "detail": reason} if declined else None
    return e


def _cell_row(cur, sheet, ref):
    return cur.execute(
        "SELECT row,col,ref,covered,value_type,value,boolean_value,date_value,string_value,"
        "style_name,stored_formula,text,span_start,span_len FROM cells "
        "WHERE doc_id=1 AND sheet=? AND ref=?", (sheet, ref)
    ).fetchone()


def _style_object(cur, style_name):
    if not style_name:
        return None
    row = cur.execute(
        "SELECT scope,name,family,parent,data_style,tcp_json,tp_json FROM styles "
        "WHERE doc_id=1 AND name=? ORDER BY CASE scope WHEN 'auto' THEN 0 ELSE 1 END LIMIT 1",
        (style_name,),
    ).fetchone()
    if row is None:
        return None
    return {
        "name": row[1],
        "family": row[2],
        "parent": row[3],
        "data_style": row[4],
        "table_cell_properties": json.loads(row[5]),
        "text_properties": json.loads(row[6]),
    }


def _source_bytes(cur):
    row = cur.execute("SELECT payload FROM source_blob WHERE doc_id=1").fetchone()
    return row[0] if row else None


def _sheet_name(cur, sheet):
    row = cur.execute("SELECT name,part FROM sheets WHERE doc_id=1 AND ord=?", (sheet,)).fetchone()
    return (row[0], row[1]) if row else (None, None)


def _decoded_cell(cur, span_start, span_len):
    payload = _source_bytes(cur)
    if payload is None:
        return None
    part = cur.execute("SELECT content_part FROM documents WHERE doc_id=1").fetchone()[0]
    zf = zipfile.ZipFile(io.BytesIO(payload))
    try:
        data = zf.read(part)
    except KeyError:
        return None
    return data[span_start:span_start + span_len]


def q_answer(con, q, plan):
    cur = con.cursor()
    sheet = int(plan.get("sheet", 0))
    cell = plan.get("cell")
    dep = plan.get("dep")
    q4cell = plan.get("q4cell", cell)
    has = lambda t: con.execute(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?", (t,)
    ).fetchone()[0] > 0

    if q == "Q1":
        row = _cell_row(cur, sheet, cell)
        if row is None:
            return _env(q, "sqlite", None, declined=True, code="no-such-cell",
                        reason=f"{cell} not on sheet {sheet}")
        c = {"value_type": row[4], "value": row[5], "boolean_value": row[6],
             "date_value": row[7], "string_value": row[8], "text": row[11]}
        return _env(q, "sqlite", {"type": row[4], "value": _typed_value(c)},
                    detail={"ref": row[2]})
    if q == "Q2":
        row = _cell_row(cur, sheet, cell)
        if row is None:
            return _env(q, "sqlite", None, declined=True, code="no-such-cell",
                        reason=f"{cell} not on sheet {sheet}")
        return _env(q, "sqlite", row[10], detail={"ref": row[2]})
    if q == "Q3":
        if dep is None:
            return _env(q, "sqlite", None, declined=True, code="no-target",
                        reason="no dependency target")
        rows = cur.execute(
            "SELECT ref,stored_formula FROM cells WHERE doc_id=1 AND sheet=? AND stored_formula IS NOT NULL",
            (sheet,),
        ).fetchall()
        pat = re.compile(r"(?<![A-Za-z0-9_])" + re.escape(dep) + r"(?![0-9])")
        found = sorted(r[0] for r in rows if pat.search(r[1]))
        return _env(q, "sqlite", found, detail={"method": "lexical", "target": dep})
    if q == "Q4":
        rc = parse_cell_position(q4cell) if q4cell else None
        name = _sheet_name(cur, sheet)[0]
        if rc is None or name is None:
            return _env(q, "sqlite", None, declined=True, code="no-cell", reason="no cell")
        col, rowno = rc
        rows = cur.execute(
            "SELECT name,cell_range_address FROM named_expressions WHERE doc_id=1 AND kind='range'"
        ).fetchall()
        names = sorted(n for n, addr in rows if _range_contains(addr, name, rowno, col))
        return _env(q, "sqlite", names, detail={"cell": q4cell})
    if q == "Q5":
        row = _cell_row(cur, sheet, cell)
        if row is None:
            return _env(q, "sqlite", None, declined=True, code="no-such-cell",
                        reason=f"{cell} not on sheet {sheet}")
        return _env(q, "sqlite", _style_object(cur, row[9]),
                    detail={"ref": row[2], "style_name": row[9]})
    if q == "Q6":
        row = cur.execute("SELECT name,part FROM sheets WHERE doc_id=1 AND ord=?", (sheet,)).fetchone()
        if row is None:
            return _env(q, "sqlite", None, declined=True, code="no-such-sheet",
                        reason=f"no sheet {sheet}")
        return _env(q, "sqlite", row[1], detail={"sheet": row[0]})
    if q == "Q7":
        name = _sheet_name(cur, sheet)[0]
        if name is None:
            return _env(q, "sqlite", None, declined=True, code="no-such-sheet",
                        reason=f"no sheet {sheet}")
        rows = cur.execute(
            "SELECT name,base_cell_address,cell_range_address FROM named_expressions WHERE doc_id=1"
        ).fetchall()
        names = sorted(
            n for n, base, rng in rows
            if _ref_in_sheet(base, name) or _ref_in_sheet(rng, name)
        )
        return _env(q, "sqlite", names, detail={"sheet": name})
    if q == "Q8":
        row = _cell_row(cur, sheet, cell)
        if row is None:
            return _env(q, "sqlite", None, declined=True, code="no-such-cell",
                        reason=f"{cell} not on sheet {sheet}")
        span_start, span_len = row[12], row[13]
        data = _decoded_cell(cur, span_start, span_len)
        if data is None:
            return _env(q, "sqlite", None, declined=True, code="no-cell-xml",
                        reason=f"no decoded cell for {cell}")
        part = cur.execute("SELECT part FROM sheets WHERE doc_id=1 AND ord=?", (sheet,)).fetchone()[0]
        span = cur.execute(
            "SELECT span_start,span_end FROM provenance WHERE doc_id=1 AND kind='sheet' AND key=?",
            (str(sheet),),
        ).fetchone()
        return _env(q, "sqlite", {"cell_xml_sha256": sha256_hex(data), "cell_xml_len": len(data)},
                    detail={"part": part, "member_span": list(span) if span else None,
                            "span_start": span_start, "span_len": span_len})
    if q == "Q9":
        row = cur.execute(
            "SELECT name,media_type,member_sha256,member_len FROM resources WHERE doc_id=1 "
            "ORDER BY ordinal LIMIT 1"
        ).fetchone()
        if row is None:
            return _env(q, "sqlite", None, declined=True, code="no-embedded-resource",
                        reason="sheet has no embedded resource")
        return _env(q, "sqlite", {"resource_sha256": row[2], "resource_len": row[3]},
                    detail={"name": row[0], "media_type": row[1]})
    if q == "Q10":
        if not has("source_blob"):
            return _env(q, "sqlite", None, declined=True, code="depth-too-low",
                        reason="source blob not in contract")
        payload = _source_bytes(cur)
        if payload is None:
            return _env(q, "sqlite", None, declined=True, code="no-source", reason="no retained source")
        return _env(q, "sqlite", {"length": len(payload), "sha256": sha256_hex(payload)})
    return _env(q, "sqlite", None, declined=True, code="unknown-question", reason=q)


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def _open(db):
    return sqlite3.connect(db)


def main(argv=None):
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)

    b = sub.add_parser("build")
    b.add_argument("--source", required=True)
    b.add_argument("--db", required=True)
    b.add_argument("--through", type=int, default=5)
    b.add_argument("--metrics", default=None)

    q = sub.add_parser("query")
    q.add_argument("--db", required=True)
    q.add_argument("--q", required=True)
    q.add_argument("--plan", default="{}")
    q.add_argument("--out", required=True)

    s = sub.add_parser("session")
    s.add_argument("--db", required=True)
    s.add_argument("--queries", required=True)
    s.add_argument("--plan", default="{}")
    s.add_argument("--out", required=True)

    m = sub.add_parser("materialize")
    m.add_argument("--db", required=True)
    m.add_argument("--out", required=True)

    ag = sub.add_parser("aggregate")
    ag.add_argument("--raw", required=True)
    ag.add_argument("--campaign", required=True)
    ag.add_argument("--env", default=None)

    sg = sub.add_parser("sqlgen")
    sg.add_argument("--out", required=True)

    ns = ap.parse_args(argv)

    if ns.cmd == "build":
        metrics = build(ns.source, ns.db, ns.through)
        if ns.metrics:
            with open(ns.metrics, "w") as f:
                json.dump(metrics, f, sort_keys=True)
        print(json.dumps(metrics, sort_keys=True))
        return 0

    if ns.cmd == "query":
        plan = json.loads(ns.plan)
        con = _open(ns.db)
        env = q_answer(con, ns.q, plan)
        con.close()
        with open(ns.out, "w") as f:
            json.dump(env, f, sort_keys=True)
        print(json.dumps({"q": ns.q, "declined": env["declined"]}))
        return 0

    if ns.cmd == "session":
        plan = json.loads(ns.plan)
        qs = [x for x in ns.queries.split(",") if x]
        con = _open(ns.db)
        batch = []
        for one in qs:
            t0 = now_us()
            env = q_answer(con, one, plan)
            us = now_us() - t0
            batch.append({"q": one, "us": us, "env": env})
        con.close()
        with open(ns.out, "w") as f:
            json.dump({"batch": batch}, f, sort_keys=True)
        print(json.dumps({"observations": len(batch)}))
        return 0

    if ns.cmd == "materialize":
        con = _open(ns.db)
        row = con.execute("SELECT payload FROM source_blob WHERE doc_id=1").fetchone()
        con.close()
        if row is None:
            return 3
        with open(ns.out, "wb") as f:
            f.write(row[0])
        return 0

    if ns.cmd == "aggregate":
        return aggregate(ns.raw, ns.campaign, ns.env)

    if ns.cmd == "sqlgen":
        os.makedirs(ns.out, exist_ok=True)
        with open(os.path.join(ns.out, "queries.txt"), "w") as f:
            for q in ("Q1", "Q2", "Q3", "Q4", "Q5", "Q6", "Q7", "Q8", "Q9", "Q10"):
                f.write(q + "\n")
        print(json.dumps({"ok": True}))
        return 0

    return 2


# ---------------------------------------------------------------------------
# Aggregator
# ---------------------------------------------------------------------------

def _load_p19():
    import importlib.util
    here = os.path.dirname(os.path.abspath(__file__))
    path = os.path.join(here, "phase19-repeat.py")
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


def compare(q, va, vb):
    """Cross-lane comparison of two Q envelopes -> (result, detail)."""
    if va is None or vb is None:
        return "missing", "no envelope"
    if va.get("declined") and vb.get("declined"):
        return "both-decline", "both typed declines"
    if va.get("declined") or vb.get("declined"):
        who = "sqlite/duckdb" if va.get("declined") else "vole"
        return "capability-gap", f"{who} declines"
    a, b = va.get("value"), vb.get("value")
    if q in ("Q4", "Q7"):
        return ("equal" if sorted(a or []) == sorted(b or []) else "mismatch"), "set"
    if q == "Q3":
        return ("equal" if sorted(a or []) == sorted(b or []) else "mismatch"), "lexical set"
    if q == "Q8":
        if isinstance(a, dict) and isinstance(b, dict):
            ok = a.get("cell_xml_sha256") == b.get("cell_xml_sha256")
            return ("equal" if ok else "mismatch"), "cell xml sha"
        return "shape", "not dict"
    if q == "Q9":
        if isinstance(a, dict) and isinstance(b, dict):
            ok = a.get("resource_sha256") == b.get("resource_sha256")
            return ("equal" if ok else "mismatch"), "resource sha"
        return "shape", "not dict"
    if q == "Q10":
        if isinstance(a, dict) and isinstance(b, dict):
            ok = a.get("sha256") == b.get("sha256") and a.get("length") == b.get("length")
            return ("equal" if ok else "mismatch"), "length+sha256"
        return "shape", "not dict"
    return ("equal" if a == b else "mismatch"), "default"


def _paired(by_fixture, other):
    out = {}
    for fx, vals in by_fixture.items():
        v, o = vals.get("vole"), vals.get(other)
        if v is None or o in (None, 0):
            continue
        out[fx] = float(v) / float(o)
    return out


def _geomean(vals):
    import math
    v = [float(x) for x in vals if float(x) > 0]
    if not v:
        return 0.0
    return math.exp(sum(math.log(x) for x in v) / len(v))


TIE = 0.10


def aggregate(raw, campaign, env_path=None):
    P19 = _load_p19()
    B = 10000
    SEED = 21320
    import statistics

    env = {}
    if env_path and os.path.exists(env_path):
        with open(env_path) as f:
            try:
                env = json.load(f)
            except ValueError:
                env = {}

    docs = read_tsv(os.path.join(raw, "fixtures.tsv"))
    fixtures = [r["fixture"] for r in docs]
    lanes = ["vole", "sqlite", "duckdb"]
    qs = ["Q1", "Q2", "Q3", "Q4", "Q5", "Q6", "Q7", "Q8", "Q9", "Q10"]

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
        warm_perrep[lane][(fx, rep)] = warm_perrep[lane].get((fx, rep), 0) + int(r["us"])
    for lane in lanes:
        for (fx, rep), v in warm_perrep[lane].items():
            if fx not in warm_us[lane] or v < warm_us[lane][fx]:
                warm_us[lane][fx] = v

    qanswers = {}
    equiv = {}
    for fx in fixtures:
        for q in qs:
            row = {}
            for lane in lanes:
                p = os.path.join(raw, "qanswers", f"{fx}.{q}.{lane}.json")
                try:
                    with open(p) as f:
                        row[lane] = json.load(f)
                except (OSError, ValueError):
                    row[lane] = None
            qanswers[(fx, q)] = row
            for lane in ("sqlite", "duckdb"):
                res, _ = compare(q, row.get("vole"), row.get(lane))
                equiv.setdefault((q, lane), {}).setdefault(res, 0)
                equiv[(q, lane)][res] += 1

    exact_ok = sum(1 for r in exact_rows if r.get("vole_ok") in ("1", "true"))
    exact_n = len(exact_rows)

    lines = []
    lines.append("# Phase 21.3.2 — ODS economic court")
    lines.append("")
    lines.append("**Question.** Against a source-retaining SQLite baseline *and* a "
                 "DuckDB/Parquet analytical baseline, on contract-equivalent terms "
                 "(the C0–C5 capability contract extended for spreadsheet coordinates), "
                 "can VOLE answer the same ten questions (Q1–Q10) it can answer, at "
                 "comparable build/storage/cold/warm cost, while closing the original "
                 "workbook byte-exactly?")
    lines.append("")
    lines.append("**Method.** A deterministic self-authored ODS corpus "
                 "(`tools/fixtures/make-ods.py --corpus`) is regenerated at court time; "
                 "each fixture is ingested by three lanes (VOLE field CLI; a source-retaining "
                 "SQLite baseline; a DuckDB/Parquet baseline), Q1–Q10 are asked of each, and "
                 "build/storage/cold/warm are measured. Persistent bytes are the **sum of "
                 "regular-file sizes** (`find -type f -printf '%s'`), never `du -sb` "
                 "(ADR-0049).")
    lines.append("")
    lines.append(f"Corpus: **{len(fixtures)} fixtures**; lanes **{', '.join(lanes)}**; "
                 f"questions **Q1–Q10**; bootstrap **{B} resamples, seed {SEED}**, "
                 f"cluster-resampled by fixture; tie band **+/-{int(TIE * 100)}%**.")
    lines.append("")
    prof = (env or {}).get("profile", "unknown")
    sub = (env or {}).get("vole_substrate", "unknown")
    bin_label = (env or {}).get("bin", "?")
    lines.append(f"VOLE lane: **{prof}** profile (`{bin_label}`); substrate: **{sub}**. The "
                 "comparators (SQLite C, DuckDB, Python) are unaffected by the Rust profile "
                 "while the entropyfs build is not, so the release default keeps the "
                 "comparison fair to VOLE. All wall times are recorded and reported in "
                 "**microseconds (`us`)**.")
    lines.append("")
    lines.append("## Verdict")
    lines.append("")
    verdict = "PASS" if exact_ok == exact_n and exact_n > 0 else "FAIL"
    lines.append(f"- **VOLE exactness (Q10): {exact_ok}/{exact_n} byte-exact** "
                 "(length + SHA-256 + `cmp`, after source + descriptor deletion in a fresh "
                 "process).")
    lines.append(f"- **COURT VERDICT: {verdict}** (fails unless exactness is 100 % on the "
                 "VOLE lane for every fixture).")
    lines.append("")

    lines.append("## Build + storage (per lane, per fixture)")
    lines.append("")
    lines.append("Time columns are **microseconds (`us`)** "
                 "(the raw TSVs carry `us`; nothing is relabelled or rescaled).")
    lines.append("")
    lines.append("| fixture | src B | " + " | ".join(f"{l} build us" for l in lanes) +
                 " | " + " | ".join(f"{l} B" for l in lanes) + " |")
    lines.append("|---|---:|" + "".join("---:|" for _ in lanes) + "".join("---:|" for _ in lanes))
    for r in docs:
        fx = r["fixture"]
        cells = [r["src_bytes"]]
        for lane in lanes:
            cells.append(str(build_us[lane].get(fx, "-")))
        for lane in lanes:
            cells.append(str(store_bytes[lane].get(fx, "-")))
        lines.append("| " + fx + " | " + " | ".join(cells) + " |")
    lines.append("")
    lines.append("`build us` is the best-of-N (min) of the retained repetitions "
                 "(microseconds).")
    lines.append("")

    lines.append("## Paired ratios VOLE/other (median + geometric mean, 95% CI by fixture)")
    lines.append("")
    lines.append("| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | "
                 "wins | ties | losses | ratio of sums |")
    lines.append("|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|")
    for label, series in (("build", build_us), ("storage", store_bytes), ("cold", cold_us), ("warm", warm_us)):
        for other in ("sqlite", "duckdb"):
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
            lines.append("| {} | {} | {} | {:.3f} | {:.3f} | {:.3f}..{:.3f} | {:.3f}..{:.3f} | {} | {} | {} | {:.3f} |".format(
                label, other, len(vals), statistics.median(vals), P19.geomean(vals),
                lo_m, hi_m, lo_g, hi_g, wins, ties, losses, (sums_v / sums_o if sums_o else 0)))
    lines.append("")
    lines.append("A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, "
                 "summarised by the median and geometric mean with a fixed-seed cluster bootstrap "
                 "over fixtures; `ratio of sums` is reported separately and named as such. Each "
                 "per-fixture lane cost (build, cold, warm) is the **minimum over the retained "
                 "repetitions** (best-of-N, as for build); storage is measured once after the last "
                 "build. Every raw sample is kept in `raw/build.tsv`, `raw/cold.tsv`, "
                 "`raw/warm.tsv`. With "
                 f"only {len(fixtures)} fixture clusters the bootstrap is coarse and is stated as "
                 "such, not as a precise interval.")
    lines.append("")
    startup = (env or {}).get("python_startup_us") if isinstance(env, dict) else None
    lines.append("The SQLite and DuckDB cold paths run a fresh **Python** process per request, so "
                 "their cold numbers include the interpreter start-up as part of that lane's honest "
                 f"per-request cost (measured bare start-up {startup} us); VOLE's cold path is a "
                 "native binary. The cold ratio is therefore dominated by that constant and is "
                 "reported for completeness, not headlined.")
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
    lines.append("| Q | VOLE | SQLite (source-retaining) | DuckDB (columnar) |")
    lines.append("|---|---|---|---|")
    qdesc = {
        "Q1": ("typed cell value (metadata)", "typed value (indexed)", "typed value (SQL)"),
        "Q2": ("stored formula text", "stored formula (indexed)", "stored formula (SQL)"),
        "Q3": ("**DECLINE** (no formula evaluation / dependency graph)",
               "lexical dependents (stored-formula text scan)",
               "lexical dependents (SQL scan)"),
        "Q4": ("containing named range(s)", "containing named range(s)", "containing named range(s) (join)"),
        "Q5": ("resolved cell style (automatic/named)", "style via styles join", "style via SQL join"),
        "Q6": ("content part holding the sheet", "content part", "content part"),
        "Q7": ("named expressions referencing the sheet", "named expressions referencing the sheet",
               "named expressions referencing the sheet (SQL)"),
        "Q8": ("exact decoded cell XML bytes + member span", "exact decoded cell sha256 + raw span",
               "**DECLINE** (no exact span)"),
        "Q9": ("**DECLINE** (no ODS-native resource/media selector)",
               "embedded image member bytes (from the retained package)",
               "**DECLINE** (no embedded members)"),
        "Q10": ("`materialize --exact` (byte-authority)", "retained source blob (byte-authority)",
                "**DECLINE** `not-native` (labelled blob passthrough only)"),
    }
    for q in qs:
        lines.append("| {} | {} | {} | {} |".format(q, *qdesc[q]))
    lines.append("")

    surface_rows = read_tsv(os.path.join(raw, "vole_surface.tsv"))
    if surface_rows:
        lines.append("## VOLE ODS/common surface exercised (per fixture, cold)")
        lines.append("")
        lines.append("Every shipped ODS/common selector is driven once per fixture; the "
                     "`ok` column is `rc == 0`. The common `--resource` selector is expected "
                     "to decline (ODS exposes no resource/media selector), recorded as a "
                     "capability gap, not a court failure; `--ods-find` needs `--kind text`.")
        lines.append("")
        names = sorted({r["surface"] for r in surface_rows})
        lines.append("| surface | ok | decline |")
        lines.append("|---|---:|---:|")
        for nm in names:
            okn = sum(1 for r in surface_rows if r["surface"] == nm and r["ok"] == "1")
            tot = sum(1 for r in surface_rows if r["surface"] == nm)
            lines.append(f"| {nm} | {okn} | {tot - okn} |")
        lines.append("")

    lines.append("## Comparison normalization (contract-equivalence)")
    lines.append("")
    lines.append("Where two lanes answer the SAME question the comparable value is "
                 "normalized so the comparison is on the same contract and any projection is "
                 "explicit (every judgement call is listed so it can be audited):")
    lines.append("")
    lines.append("- **Q1 (typed value):** the object `{type, value}` where `type` is "
                 "`office:value-type` and `value` is the type's matching attribute "
                 "(`office:value` for float/percentage/currency, `office:boolean-value` for "
                 "boolean, `office:date-value` for date/time, `office:string-value` (or the "
                 "cell text) for string). A cell with no `office:value-type` reads `{null, "
                 "null}`.")
    lines.append("- **Q2 (formula):** the raw `table:formula` text; a missing formula is `null` "
                 "on every lane.")
    lines.append("- **Q4 (containing named range):** the sorted set of named-range names whose "
                 "`table:cell-range-address` is on the same sheet and contains the cell.")
    lines.append("- **Q5 (style):** a normalized object `{name, family, parent, data_style, "
                 "table_cell_properties, text_properties}`, resolved by the cell's "
                 "`table:style-name` against the combined automatic + named table-cell styles "
                 "(automatic preferred on a name clash); no style name reads `null`.")
    lines.append("- **Q6 (content part):** the resolved main content part name (in ODS every "
                 "sheet lives in the single main content part, so this is `content.xml` — the "
                 "honest ODS structure, not a per-sheet part).")
    lines.append("- **Q7 (named-expression reference):** the sorted set of named-expression names "
                 "whose `table:base-cell-address` or `table:cell-range-address` names the sheet.")
    lines.append("- **Q8 (exact cell span):** the SHA-256 of the DECODED `<table:table-cell>` "
                 "element bytes; both lanes scan the same decompressed content member with the "
                 "same `<` … matching-`>` rule; the raw member span is a lane detail.")
    lines.append("- **Q9 (embedded resource):** the SHA-256 of the decoded embedded image member "
                 "(the first manifest entry with an `image/*` media type).")
    lines.append("- **Q10 (original bytes):** `{length, sha256}` of the whole workbook.")
    lines.append("")

    lines.append("## Scope (honest)")
    lines.append("")
    lines.append("- **Self-authored deterministic corpus, NOT a real-world population.** The "
                 "eight workbooks are generated by `tools/fixtures/make-ods.py --corpus` "
                 "(Python stdlib only, fixed-seed LCG). Every claim is scoped to these files; "
                 "the aggregate carries a fixture-clustered CI and is not extrapolated.")
    lines.append("- **Only Q10 is a byte-authority claim.** `materialize --exact == source` (length + "
                 "SHA-256 + `cmp`) and the retained blob reproduce the original bytes; that is the "
                 "archival invariant. **Every other observation (Q1–Q9) is a DERIVED projection** of "
                 "the OpenDocument spreadsheet model — the typed value, the stored formula (never "
                 "evaluated), the style/named-range/comment/resource views. Semantic agreement is not "
                 "archival equality.")
    lines.append("- **Q3 and Q9 are recorded VOLE capability gaps, never equivalences.** VOLE does "
                 "not evaluate formulas and exposes no ODS-native resource/media selector; the "
                 "SQLite baseline answers both (lexically / from the retained package).")
    lines.append("- **Baseline durability contract.** The source-retaining SQLite baseline runs "
                 "`journal_mode=WAL` with `synchronous=NORMAL`: a committed transaction survives a "
                 "process crash, but WAL is not a media-failure guarantee. The DuckDB lane writes "
                 "Parquet files directly (no WAL); both are destroyed and re-created at court time.")
    lines.append("- **DuckDB is a comparator for columnar/tabular questions.** It answers Q1/Q2/Q4/"
                 "Q5/Q6/Q7 (and Q3 lexically) but provides no exact-source closure or provenance; "
                 "Q8/Q9/Q10 are typed declines (Q10 `not-native`), and it is not compared as "
                 "though it carried exactness.")
    lines.append("- **Nothing here is run on the host.** Every command ran in the pinned `analytical` "
                 "container (dev toolchain + python3 + sqlite3 + hash-pinned DuckDB 1.5.6).")
    lines.append("")

    matrix = []
    matrix.append("# Phase 21.3.2 — cross-lane Q1–Q10 answer matrix")
    matrix.append("")
    matrix.append("Derived / declined per lane, and the VOLE-vs-comparator equivalence. "
                  "`g` = answered (derived), `D` = typed decline, `-` = not applicable.")
    matrix.append("")
    matrix.append("| fixture | Q | VOLE | SQLite | DuckDB | VOLE↔SQLite | VOLE↔DuckDB |")
    matrix.append("|---|---|---|---|---|---|---|")
    for fx in fixtures:
        for q in qs:
            row = qanswers[(fx, q)]

            def mark(lane):
                e = row.get(lane)
                if e is None:
                    return "-"
                return "D" if e.get("declined") else "g"

            rs, _ = compare(q, row.get("vole"), row.get("sqlite"))
            rd, _ = compare(q, row.get("vole"), row.get("duckdb"))
            matrix.append(f"| {fx} | {q} | {mark('vole')} | {mark('sqlite')} | {mark('duckdb')} | {rs} | {rd} |")
    matrix.append("")
    matrix.append("### Aggregate equivalence per Q")
    matrix.append("")
    matrix.append("| Q | comparator | equal | both-decline | capability-gap | mismatch | shape |")
    matrix.append("|---|---|---:|---:|---:|---:|---:|")
    for q in qs:
        for other in ("sqlite", "duckdb"):
            c = equiv.get((q, other), {})
            matrix.append("| {} | {} | {} | {} | {} | {} | {} |".format(
                q, other, c.get("equal", 0), c.get("both-decline", 0), c.get("capability-gap", 0),
                c.get("mismatch", 0), c.get("shape", 0)))
    matrix.append("")

    counts = []
    counts.append(f"fixtures {len(fixtures)}")
    counts.append(f"questions {len(qs)}")
    counts.append(f"exact_ok {exact_ok}")
    counts.append(f"exact_n {exact_n}")
    for q in qs:
        for other in ("sqlite", "duckdb"):
            c = equiv.get((q, other), {})
            counts.append(f"{q}.{other}.equal {c.get('equal', 0)}")
            counts.append(f"{q}.{other}.capability_gap {c.get('capability-gap', 0)}")
            counts.append(f"{q}.{other}.mismatch {c.get('mismatch', 0)}")
            counts.append(f"{q}.{other}.both_decline {c.get('both-decline', 0)}")
    counts.append(f"verdict {verdict}")

    with open(os.path.join(campaign, "SUMMARY.md"), "w") as f:
        f.write("\n".join(lines) + "\n")
    with open(os.path.join(campaign, "MATRIX.md"), "w") as f:
        f.write("\n".join(matrix) + "\n")
    with open(os.path.join(campaign, "counts.txt"), "w") as f:
        f.write("\n".join(counts) + "\n")

    receipt = {
        "campaign": campaign,
        "phase": "21.3.2 — ODS economic court (VOLE vs SQLite vs DuckDB)",
        "verdict": verdict,
        "exact_ok": exact_ok,
        "exact_n": exact_n,
        "fixtures": fixtures,
        "lanes": lanes,
        "estimator": ("paired per-fixture ratio; median + geometric mean; fixed-seed cluster "
                      f"bootstrap by fixture ({B} resamples, seed {SEED}); tie band +/-{int(TIE*100)}%; "
                      "ratio of sums reported separately"),
        "equivalence": {f"{q}.{o}": equiv.get((q, o), {}) for q in qs for o in ("sqlite", "duckdb")},
        "environment": env,
    }
    with open(os.path.join(campaign, "receipt.json"), "w") as f:
        json.dump(receipt, f, indent=2, sort_keys=True)
    print("\n".join(lines))
    return 0 if verdict == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
