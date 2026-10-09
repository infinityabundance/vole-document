#!/usr/bin/env python3
# Phase 21.2.3 — PPTX economic court: the source-retaining *SQLite* baseline and
# the campaign aggregator.
#
# ## Why this file exists
#
# Phase 21.2.3 completes subphase 21.2 (the PresentationML adapter shipped in
# 21.2.1). PPTX is **not** a tabular/analytical format, so there is no DuckDB
# comparator (unlike the XLSX court): the required comparator is a
# **source-retaining SQLite baseline** on contract-equivalent terms — the same
# eight questions (Q1..Q8) asked of both lanes.
#
# ## Honesty
#
# * The baseline extracts structure with Python **stdlib** only (`zipfile`,
#   `xml.etree.ElementTree`) and retains the original bytes as a BLOB. Exactness
#   (Q8) is a *validation* of the retained blob; it adds no new table.
# * `journal_mode=WAL` + `synchronous=NORMAL` is the durability contract: a
#   committed transaction survives a process crash, consistent with the
#   established A1 baseline (`tools/fixtures/phase12-baseline.py`,
#   `phase21-3-xlsx-baseline.py`). WAL does NOT make the store crash-proof
#   against media failure; it is the same guarantee the other baselines state.
# * Declines are typed: a question the lane cannot answer is `{"declined":true,
#   "decline":{"code":..,"detail":..}}`, never a silent empty answer.
#
# ## Subcommands
#
#   build       --source S --db D [--metrics M]
#   query       --db D --q Qn --plan JSON --out OUT
#   session     --db D --queries Q1,Q2,.. --plan JSON --out OUT
#   materialize --db D --out OUT
#   aggregate   --raw DIR --campaign DIR --env ENV_JSON

import argparse
import hashlib
import io
import json
import os
import posixpath
import sqlite3
import sys
import time
import zipfile
from xml.etree import ElementTree as ET

REL_NS = "{http://schemas.openxmlformats.org/package/2006/relationships}"
OD_REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
R_ID = "{" + OD_REL + "}id"
R_EMBED = "{" + OD_REL + "}embed"

# Provenance classes shared with the rest of the repo.
CLASS_VERBATIM = "verbatim"
CLASS_DERIVED = "derived"
CLASS_HEURISTIC = "heuristic"
CLASS_UNRESOLVED = "unresolved"

# Shape kinds counted by the container (mirrors `ShapeKind` in src/adapter/pptx.rs).
SHAPE_TAGS = ("sp", "pic", "graphicFrame", "grpSp", "cxnSp")


# ---------------------------------------------------------------------------
# Small helpers
# ---------------------------------------------------------------------------

def sha256_hex(b):
    return hashlib.sha256(b).hexdigest()


def now_us():
    return int(time.monotonic() * 1_000_000)


def _local(tag):
    return tag.split("}")[-1]


def _attr_local(el, name):
    """The value of an attribute by local name (namespace-insensitive), matching
    the adapter's local-name attribute handling."""
    for k, v in el.attrib.items():
        if k == name or k.endswith("}" + name):
            return v
    return None


def _ns_attr(el, name):
    """The value of a *namespaced* attribute by local name (e.g. `r:id`), which
    is what the packaging layer reads for relationship ids."""
    for k, v in el.attrib.items():
        if k.endswith("}" + name):
            return v
    return None


def _rels(xml_bytes):
    if xml_bytes is None:
        return []
    out = []
    root = ET.fromstring(xml_bytes)
    for rel in root:
        if rel.tag != REL_NS + "Relationship":
            continue
        out.append(
            {
                "id": rel.get("Id"),
                "type": rel.get("Type", ""),
                "target": rel.get("Target", ""),
                "external": rel.get("TargetMode") == "External",
            }
        )
    out.sort(key=lambda r: r["id"] or "")
    return out


def _resolve(owner, target):
    """Resolve an OPC relationship target to a canonical `/`-rooted part name."""
    if target.startswith("/"):
        return target
    base = posixpath.dirname(owner)
    p = posixpath.normpath(posixpath.join(base, target))
    return p if p.startswith("/") else "/" + p


def _local_header_span(buf, zinfo):
    """The raw byte span (data_offset, data_offset+compress_size) of a member's
    compressed record inside the source ZIP. Mirrors VOLE's `source_span` for the
    member raw node."""
    off = zinfo.header_offset
    if buf[off:off + 4] != b"PK\x03\x04":
        return None
    name_len = int.from_bytes(buf[off + 26:off + 28], "little")
    extra_len = int.from_bytes(buf[off + 28:off + 30], "little")
    data_off = off + 30 + name_len + extra_len
    return [data_off, data_off + zinfo.compress_size]


# ---------------------------------------------------------------------------
# Content types
# ---------------------------------------------------------------------------

def parse_content_types(data):
    defaults = {}
    overrides = {}
    if data is None:
        return defaults, overrides
    root = ET.fromstring(data)
    for el in root:
        lt = _local(el.tag)
        if lt == "Default":
            defaults[(el.get("Extension") or "").lower()] = el.get("ContentType")
        elif lt == "Override":
            overrides[(el.get("PartName") or "").lower()] = el.get("ContentType")
    return defaults, overrides


def content_type_for(name, defaults, overrides):
    ov = overrides.get(name.lower())
    if ov is not None:
        return ov
    if "." in name:
        ext = name.rsplit(".", 1)[1].lower()
        return defaults.get(ext)
    return None


# ---------------------------------------------------------------------------
# Shape-tree extraction (mirrors the adapter's local-name accumulation)
# ---------------------------------------------------------------------------

def _txbody_text(tx):
    """The paragraphs of a `<p:txBody>`/`<a:txBody>`, joined by `\\n`, where each
    paragraph is the concatenation of its run texts (`<a:t>`), with `<a:br>`
    contributing `\\n`. This mirrors `SlideParser` exactly (which appends every
    text event while the text target is a shape/cell)."""
    text = ""
    for p in tx:
        if _local(p.tag) != "p":
            continue
        seg = ""
        for c in p.iter():
            lt = _local(c.tag)
            if lt == "t":
                seg += c.text or ""
            elif lt == "br":
                seg += "\n"
        if text:
            text += "\n"
        text += seg
    return text


def _find_txbody(el):
    for c in el:
        if _local(c.tag) == "txBody":
            return c
    return None


def _parse_table(tbl):
    rows = []
    for tr in tbl:
        if _local(tr.tag) != "tr":
            continue
        cells = []
        for tc in tr:
            if _local(tc.tag) != "tc":
                continue
            tx = _find_txbody(tc)
            cells.append(
                {
                    "text": _txbody_text(tx) if tx is not None else "",
                    "grid_span": _int_attr(tc, "gridSpan", 1),
                    "row_span": _int_attr(tc, "rowSpan", 1),
                }
            )
        rows.append({"cells": cells})
    return {"rows": rows}


def _int_attr(el, name, default):
    v = _attr_local(el, name)
    if v is None:
        return default
    try:
        return int(v)
    except ValueError:
        return default


def _shape_from(el):
    kind = _local(el.tag)
    node = {
        "kind": kind,
        "name": None,
        "placeholder": None,
        "text": "",
        "media": None,
        "chart": None,
        "table": None,
        "children": [],
    }
    for c in el.iter():
        if _local(c.tag) == "cNvPr":
            node["name"] = c.get("name")
            break
    for c in el.iter():
        if _local(c.tag) == "ph":
            node["placeholder"] = c.get("type")
            break
    if kind == "sp":
        tx = _find_txbody(el)
        node["text"] = _txbody_text(tx) if tx is not None else ""
    elif kind == "pic":
        for c in el.iter():
            if _local(c.tag) == "blip":
                node["media"] = _ns_attr(c, "embed")
                break
    elif kind == "graphicFrame":
        for c in el.iter():
            lt = _local(c.tag)
            if lt == "tbl" and node["table"] is None:
                node["table"] = _parse_table(c)
            elif lt == "chart" and node["chart"] is None:
                node["chart"] = _ns_attr(c, "id")
    elif kind == "grpSp":
        node["children"] = [
            _shape_from(x) for x in el if _local(x.tag) in SHAPE_TAGS
        ]
    return node


def parse_slide_shapes(data):
    if data is None:
        return []
    root = ET.fromstring(data)
    sptree = None
    for el in root.iter():
        if _local(el.tag) == "spTree":
            sptree = el
            break
    if sptree is None:
        return []
    return [_shape_from(e) for e in sptree if _local(e.tag) in SHAPE_TAGS]


def slide_root_hidden(data):
    if data is None:
        return False
    root = ET.fromstring(data)
    if _local(root.tag) != "sld":
        return False
    return _attr_local(root, "show") == "0"


def _table_text(table):
    return "\n".join(
        "\t".join(c["text"] for c in row["cells"]) for row in table["rows"]
    )


def own_text(node):
    if node["table"] is not None:
        ttxt = _table_text(node["table"])
        if node["text"]:
            return node["text"] + "\n" + ttxt
        return ttxt
    return node["text"]


def _push_deep(node, out):
    own = own_text(node)
    if own:
        out.append(own)
    for c in node["children"]:
        _push_deep(c, out)


def text_deep(node):
    out = []
    _push_deep(node, out)
    return "\n".join(out)


def slide_text(shapes):
    out = []
    for s in shapes:
        _push_deep(s, out)
    return "\n".join(out)


def flatten(shapes):
    out = []

    def walk(s):
        out.append(s)
        for c in s["children"]:
            walk(c)

    for s in shapes:
        walk(s)
    return out


def shape_count(shapes):
    n = 0
    for s in shapes:
        n += 1 + shape_count(s["children"])
    return n


# ---------------------------------------------------------------------------
# Extraction
# ---------------------------------------------------------------------------

def extract(source):
    with open(source, "rb") as f:
        buf = f.read()
    doc = {
        "source_len": len(buf),
        "source_sha256": sha256_hex(buf),
        "presentation": None,
        "slides": [],
        "parts": [],
        "media": [],
        "relationships": [],
        "part_spans": {},
    }
    zf = zipfile.ZipFile(io.BytesIO(buf))
    members = {}
    for zi in zf.infolist():
        members[zi.filename] = zi
        doc["part_spans"]["/" + zi.filename] = _local_header_span(buf, zi)

    def read(name):
        if name is None:
            return None
        try:
            return zf.read(name.lstrip("/"))
        except KeyError:
            return None

    def rels_of(part):
        return _rels(
            read(
                posixpath.join(
                    posixpath.dirname(part),
                    "_rels",
                    posixpath.basename(part) + ".rels",
                )
            )
        )

    defaults, overrides = parse_content_types(read("[Content_Types].xml"))

    # Every member is an OPC part (mirrors build_opc_model).
    for name in zf.namelist():
        if name.endswith("/"):
            continue
        part = "/" + name
        ct = content_type_for(part, defaults, overrides)
        doc["parts"].append({"name": part, "content_type": ct})

    pkg_rels = _rels(read("_rels/.rels"))
    for r in pkg_rels:
        doc["relationships"].append(
            {"owner": None, "rel_id": r["id"], "rel_type": r["type"], "target": r["target"], "external": r["external"]}
        )
    pres_part = None
    for r in pkg_rels:
        if r["type"].endswith("/officeDocument") or r["type"].endswith("officeDocument"):
            pres_part = _resolve("/", r["target"])
            break
    if pres_part is None:
        raise ValueError("no officeDocument relationship (not a PPTX presentation)")
    doc["presentation"] = pres_part

    pres = ET.fromstring(read(pres_part))
    sld_ids = []
    for el in pres.iter():
        if _local(el.tag) == "sldId":
            rid = _ns_attr(el, "id")
            sld_ids.append(rid)
    pres_rels = rels_of(pres_part)
    for r in pres_rels:
        doc["relationships"].append(
            {"owner": pres_part, "rel_id": r["id"], "rel_type": r["type"], "target": r["target"], "external": r["external"]}
        )
    by_id = {r["id"]: r for r in pres_rels}

    for ord_i, rid in enumerate(sld_ids):
        r = by_id.get(rid)
        if r is None or r["external"]:
            continue
        part = _resolve(pres_part, r["target"])
        data = read(part)
        shapes = parse_slide_shapes(data)
        span = doc["part_spans"].get(part)
        s = {
            "ord": ord_i,
            "rel_id": rid,
            "part": part,
            "hidden": slide_root_hidden(data),
            "shape_count": shape_count(shapes),
            "text": slide_text(shapes),
            "shapes": flatten(shapes),
            "layout": None,
            "notes_part": None,
            "links": [],  # (kind, rel_id, part)
        }
        for lr in rels_of(part):
            doc["relationships"].append(
                {"owner": part, "rel_id": lr["id"], "rel_type": lr["type"], "target": lr["target"], "external": lr["external"]}
            )
            if lr["external"]:
                continue
            target = _resolve(part, lr["target"])
            ty = lr["type"]
            if ty.endswith("/slideLayout"):
                s["layout"] = target
                s["links"].append(("layout", lr["id"], target))
            elif ty.endswith("/notesSlide"):
                s["notes_part"] = target
                s["links"].append(("notes", lr["id"], target))
            elif ty.endswith("/image"):
                s["links"].append(("media", lr["id"], target))
            elif ty.endswith("/chart"):
                s["links"].append(("chart", lr["id"], target))
        doc["slides"].append(s)

    # Media parts (content type image/audio/video), name-sorted like VOLE.
    media_parts = [
        p
        for p in doc["parts"]
        if (p["content_type"] or "").startswith(("image/", "audio/", "video/"))
    ]
    media_parts.sort(key=lambda p: p["name"].lower())
    for ord_i, p in enumerate(media_parts):
        data = read(p["name"]) or b""
        doc["media"].append(
            {
                "ordinal": ord_i,
                "part": p["name"],
                "decoded_sha256": sha256_hex(data),
                "decoded_len": len(data),
                "payload": data,
            }
        )

    # Layouts / masters / themes / charts (part-name-sorted where VOLE is).
    def by_ct(suffix):
        return sorted(
            [p["name"] for p in doc["parts"] if (p["content_type"] or "").endswith(suffix)],
            key=lambda n: n.lower(),
        )

    doc["layouts"] = by_ct("presentationml.slideLayout+xml")
    doc["masters"] = by_ct("presentationml.slideMaster+xml")
    doc["themes"] = by_ct("officedocument.theme+xml")
    doc["charts"] = by_ct("drawingml.chart+xml")
    return doc


# ---------------------------------------------------------------------------
# Escalating SQLite contract store
# ---------------------------------------------------------------------------

def _schema():
    return """
PRAGMA journal_mode=WAL;
PRAGMA synchronous=NORMAL;
CREATE TABLE documents(
  doc_id INTEGER PRIMARY KEY, format TEXT NOT NULL, path TEXT NOT NULL,
  source_len INTEGER NOT NULL, source_sha256 TEXT NOT NULL, presentation TEXT);
CREATE TABLE parts(doc_id INTEGER, kind TEXT, ordinal INTEGER, name TEXT, content_type TEXT);
CREATE TABLE slides(
  doc_id INTEGER, ord INTEGER, part_ordinal INTEGER, part TEXT, rel_id TEXT,
  hidden INTEGER, shape_count INTEGER, layout TEXT, notes_part TEXT, text TEXT,
  span_start INTEGER, span_end INTEGER);
CREATE TABLE shapes(
  doc_id INTEGER, slide INTEGER, flat_index INTEGER, kind TEXT, name TEXT,
  placeholder TEXT, media_rel_id TEXT, chart_rel_id TEXT, text_deep TEXT);
CREATE TABLE tables(doc_id INTEGER, slide INTEGER, table_ordinal INTEGER, rows INTEGER, cells INTEGER, text TEXT);
CREATE TABLE cells(
  doc_id INTEGER, slide INTEGER, table_ordinal INTEGER, row INTEGER, col INTEGER,
  text TEXT, grid_span INTEGER, row_span INTEGER);
CREATE TABLE notes(doc_id INTEGER, slide INTEGER, part TEXT, text TEXT);
CREATE TABLE media(doc_id INTEGER, ordinal INTEGER, part TEXT, decoded_sha256 TEXT, decoded_len INTEGER, payload BLOB);
CREATE TABLE relationships(doc_id INTEGER, owner TEXT, rel_id TEXT, rel_type TEXT, target TEXT, external INTEGER);
CREATE TABLE slide_links(doc_id INTEGER, slide INTEGER, kind TEXT, rel_id TEXT, part TEXT);
CREATE INDEX idx_shapes ON shapes(doc_id, slide, flat_index);
CREATE INDEX idx_notes ON notes(doc_id, slide);
CREATE INDEX idx_media ON media(doc_id, ordinal);
CREATE INDEX idx_links ON slide_links(doc_id, slide);
CREATE TABLE provenance(doc_id INTEGER, kind TEXT, key INTEGER, class TEXT, part TEXT, span_start INTEGER, span_end INTEGER);
CREATE TABLE source_blob(doc_id INTEGER PRIMARY KEY, payload BLOB NOT NULL);
"""


def build(source, db_path):
    for suffix in ("", "-wal", "-shm"):
        try:
            os.remove(db_path + suffix)
        except FileNotFoundError:
            pass
    t0 = now_us()
    doc = extract(source)
    extract_us = now_us() - t0
    con = sqlite3.connect(db_path)
    con.executescript(_schema())
    cur = con.cursor()
    cur.execute(
        "INSERT INTO documents VALUES(1,'pptx',?,?,?,?)",
        (source, doc["source_len"], doc["source_sha256"], doc["presentation"]),
    )
    for kind, names in (
        ("layout", doc["layouts"]),
        ("master", doc["masters"]),
        ("theme", doc["themes"]),
        ("chart", doc["charts"]),
    ):
        for i, n in enumerate(names):
            cur.execute("INSERT INTO parts VALUES(1,?,?,?,NULL)", (kind, i, n))
    for m in doc["media"]:
        cur.execute(
            "INSERT INTO parts VALUES(1,'media',?,?,NULL)",
            (m["ordinal"], m["part"]),
        )
    for s in doc["slides"]:
        span = doc["part_spans"].get(s["part"])
        cur.execute(
            "INSERT INTO slides VALUES(1,?,?,?,?,?,?,?,?,?,?,?)",
            (
                s["ord"],
                None,
                s["part"],
                s["rel_id"],
                int(s["hidden"]),
                s["shape_count"],
                s["layout"],
                s["notes_part"],
                s["text"],
                span[0] if span else None,
                span[1] if span else None,
            ),
        )
        for fi, sh in enumerate(s["shapes"]):
            cur.execute(
                "INSERT INTO shapes VALUES(1,?,?,?,?,?,?,?,?)",
                (
                    s["ord"],
                    fi,
                    sh["kind"],
                    sh["name"],
                    sh["placeholder"],
                    sh["media"],
                    sh["chart"],
                    text_deep(sh),
                ),
            )
        to = 0
        for sh in s["shapes"]:
            if sh["table"] is None:
                continue
            tbl = sh["table"]
            cur.execute(
                "INSERT INTO tables VALUES(1,?,?,?,?,?)",
                (
                    s["ord"],
                    to,
                    len(tbl["rows"]),
                    sum(len(r["cells"]) for r in tbl["rows"]),
                    _table_text(tbl),
                ),
            )
            for ri, row in enumerate(tbl["rows"]):
                for ci, c in enumerate(row["cells"]):
                    cur.execute(
                        "INSERT INTO cells VALUES(1,?,?,?,?,?,?,?)",
                        (s["ord"], to, ri, ci, c["text"], c["grid_span"], c["row_span"]),
                    )
            to += 1
        if s["notes_part"]:
            ndata = zipfile.ZipFile(io.BytesIO(open(source, "rb").read())).read(
                s["notes_part"].lstrip("/")
            )
            nshapes = parse_slide_shapes(ndata)
            cur.execute(
                "INSERT INTO notes VALUES(1,?,?,?)",
                (s["ord"], s["notes_part"], slide_text(nshapes)),
            )
        for kind, rel_id, part in s["links"]:
            cur.execute("INSERT INTO slide_links VALUES(1,?,?,?,?)", (s["ord"], kind, rel_id, part))
        cur.execute(
            "INSERT INTO provenance VALUES(1,'slide',?,?,?,?,?)",
            (
                s["ord"],
                CLASS_DERIVED,
                s["part"],
                span[0] if span else None,
                span[1] if span else None,
            ),
        )
    for r in doc["relationships"]:
        cur.execute(
            "INSERT INTO relationships VALUES(1,?,?,?,?,?)",
            (r["owner"], r["rel_id"], r["rel_type"], r["target"], int(r["external"])),
        )
    for m in doc["media"]:
        cur.execute(
            "INSERT INTO media VALUES(1,?,?,?,?,?)",
            (m["ordinal"], m["part"], m["decoded_sha256"], m["decoded_len"], m["payload"]),
        )
    with open(source, "rb") as f:
        payload = f.read()
    cur.execute("INSERT INTO source_blob VALUES(1,?)", (payload,))
    cur.execute("ANALYZE")
    con.commit()
    con.close()
    return {
        "extract_us": extract_us,
        "db_bytes": os.path.getsize(db_path),
        "slides": len(doc["slides"]),
        "shapes": sum(len(s["shapes"]) for s in doc["slides"]),
        "media": len(doc["media"]),
        "layouts": len(doc["layouts"]),
        "masters": len(doc["masters"]),
        "themes": len(doc["themes"]),
        "charts": len(doc["charts"]),
    }


# ---------------------------------------------------------------------------
# Query envelopes
# ---------------------------------------------------------------------------

def _env(q, lane, value, *, declined=False, code=None, reason="", native=True, detail=None):
    e = {"q": q, "lane": lane, "declined": bool(declined), "native": native,
         "value": None if declined else value, "detail": detail or {}}
    e["decline"] = {"code": code, "detail": reason} if declined else None
    return e


def _slide_row(cur, slide):
    return cur.execute(
        "SELECT ord,part,rel_id,hidden,shape_count,layout,notes_part,text,span_start,span_end "
        "FROM slides WHERE doc_id=1 AND ord=?",
        (slide,),
    ).fetchone()


def _source_bytes(cur):
    row = cur.execute("SELECT payload FROM source_blob WHERE doc_id=1").fetchone()
    return row[0] if row else None


def q_answer(con, q, plan, source=None):
    cur = con.cursor()
    slide = int(plan.get("slide", 0))
    shape = int(plan.get("shape", 0))
    notes = int(plan.get("notes", 0))
    media = int(plan.get("media", 0))
    chart_slide = int(plan.get("chart_slide", slide))

    if q == "Q1":
        row = _slide_row(cur, slide)
        if row is None:
            return _env(q, "sqlite", None, declined=True, code="no-such-slide", reason=f"no slide {slide}")
        return _env(q, "sqlite", row[7], detail={"part": row[1], "shape_count": row[4]})
    if q == "Q2":
        row = cur.execute(
            "SELECT kind,name,text_deep FROM shapes WHERE doc_id=1 AND slide=? AND flat_index=?",
            (slide, shape),
        ).fetchone()
        if row is None:
            return _env(q, "sqlite", None, declined=True, code="no-such-shape", reason=f"slide {slide} has no shape {shape}")
        return _env(q, "sqlite", row[2], detail={"kind": row[0], "name": row[1]})
    if q == "Q3":
        row = cur.execute(
            "SELECT part,text FROM notes WHERE doc_id=1 AND slide=?", (notes,)
        ).fetchone()
        if row is None:
            return _env(q, "sqlite", None, declined=True, code="no-notes", reason=f"slide {notes} has no notes slide")
        return _env(q, "sqlite", row[1], detail={"part": row[0]})
    if q == "Q4":
        row = cur.execute(
            "SELECT part,decoded_sha256,decoded_len FROM media WHERE doc_id=1 AND ordinal=?",
            (media,),
        ).fetchone()
        if row is None:
            return _env(q, "sqlite", None, declined=True, code="no-such-media", reason=f"deck has no media {media}")
        return _env(q, "sqlite", {"media_decoded_sha256": row[1], "media_decoded_len": row[2]}, detail={"part": row[0]})
    if q == "Q5":
        rows = cur.execute(
            "SELECT chart_rel_id FROM shapes WHERE doc_id=1 AND slide=? AND chart_rel_id IS NOT NULL",
            (chart_slide,),
        ).fetchall()
        if not rows:
            return _env(q, "sqlite", None, declined=True, code="no-chart", reason=f"slide {chart_slide} references no chart")
        charts = sorted({r[0] for r in rows})
        parts = cur.execute(
            "SELECT rel_id,part FROM slide_links WHERE doc_id=1 AND slide=? AND kind='chart'", (chart_slide,)
        ).fetchall()
        return _env(q, "sqlite", charts, detail={"chart_parts": sorted(p for _, p in parts)})
    if q == "Q6":
        row = _slide_row(cur, slide)
        if row is None:
            return _env(q, "sqlite", None, declined=True, code="no-such-slide", reason=f"no slide {slide}")
        links = cur.execute(
            "SELECT kind,part FROM slide_links WHERE doc_id=1 AND slide=? ORDER BY kind,part", (slide,)
        ).fetchall()
        detail = {"layout": row[5], "media": sorted(p for k, p in links if k == "media"),
                  "notes": row[6]}
        return _env(q, "sqlite", row[1], detail=detail)
    if q == "Q7":
        row = cur.execute(
            "SELECT part,span_start,span_end FROM slides WHERE doc_id=1 AND ord=?", (slide,)
        ).fetchone()
        if row is None or row[2] is None:
            return _env(q, "sqlite", None, declined=True, code="no-span", reason=f"no source span for slide {slide}")
        return _env(q, "sqlite", [row[1], row[2]], detail={"part": row[0]})
    if q == "Q8":
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

    ns = ap.parse_args(argv)

    if ns.cmd == "build":
        metrics = build(ns.source, ns.db)
        if ns.metrics:
            with open(ns.metrics, "w") as f:
                json.dump(metrics, f, sort_keys=True)
        print(json.dumps(metrics, sort_keys=True))
        return 0

    if ns.cmd == "query":
        con = _open(ns.db)
        env = q_answer(con, ns.q, json.loads(ns.plan))
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
        who = "vole" if va.get("declined") else "sqlite"
        return "capability-gap", f"{who} declines"
    a, b = va.get("value"), vb.get("value")
    if q in ("Q1", "Q2", "Q3", "Q6"):
        return ("equal" if a == b else "mismatch"), "scalar"
    if q == "Q5":
        return ("equal" if sorted(a or []) == sorted(b or []) else "mismatch"), "set"
    if q == "Q7":
        return ("equal" if list(a or []) == list(b or []) else "mismatch"), "span"
    if q == "Q4":
        if isinstance(a, dict) and isinstance(b, dict):
            ok = (a.get("media_decoded_sha256") == b.get("media_decoded_sha256")
                  and a.get("media_decoded_len") == b.get("media_decoded_len"))
            return ("equal" if ok else "mismatch"), "media decoded sha+len"
        return "shape", "not dict"
    if q == "Q8":
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
    SEED = 21323
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
    lanes = ["vole", "sqlite"]
    qs = ["Q1", "Q2", "Q3", "Q4", "Q5", "Q6", "Q7", "Q8"]

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
    equiv = {}
    for fx in fixtures:
        for q in qs + ["Q8"]:
            row = {}
            for lane in lanes:
                p = os.path.join(raw, "qanswers", f"{fx}.{q}.{lane}.json")
                try:
                    with open(p) as f:
                        row[lane] = json.load(f)
                except (OSError, ValueError):
                    row[lane] = None
            qanswers.setdefault((fx, q), row)
    for fx in fixtures:
        for q in qs:
            row = qanswers.get((fx, q), {})
            res, _ = compare(q, row.get("vole"), row.get("sqlite"))
            equiv.setdefault((q, "sqlite"), {}).setdefault(res, 0)
            equiv[(q, "sqlite")][res] += 1

    exact_ok = sum(1 for r in exact_rows if r.get("vole_ok") in ("1", "true"))
    exact_n = len(exact_rows)

    lines = []
    lines.append("# Phase 21.2.3 — PPTX economic court")
    lines.append("")
    lines.append("**Question.** Against a source-retaining SQLite baseline, on "
                 "contract-equivalent terms, can VOLE answer the same eight questions "
                 "(Q1–Q8) it can answer, at comparable build/storage/cold/warm cost, "
                 "while closing the original presentation byte-exactly?")
    lines.append("")
    lines.append("**Method.** A deterministic self-authored PPTX corpus "
                 "(`tools/fixtures/make-pptx.py --corpus`) is regenerated at court time; "
                 "each fixture is ingested by two lanes (VOLE field CLI; a source-retaining "
                 "SQLite baseline), Q1–Q8 are asked of each, and build/storage/cold/warm are "
                 "measured. PPTX is not a tabular/analytical format, so there is **no DuckDB "
                 "comparator** (unlike the XLSX court). Persistent bytes are the **sum of "
                 "regular-file sizes** (`find -type f -printf '%s'`), never `du -sb` "
                 "(ADR-0049).")
    lines.append("")
    lines.append(f"Corpus: **{len(fixtures)} fixtures**; lanes **{', '.join(lanes)}**; "
                 f"questions **Q1–Q8**; bootstrap **{B} resamples, seed {SEED}**, "
                 f"cluster-resampled by fixture; tie band **+/-{int(TIE * 100)}%**.")
    lines.append("")
    prof = (env or {}).get("profile", "unknown")
    sub = (env or {}).get("vole_substrate", "unknown")
    bin_label = (env or {}).get("bin", "?")
    lines.append(f"VOLE lane: **{prof}** profile (`{bin_label}`); substrate: **{sub}**. The "
                 "comparator (SQLite C + Python) is unaffected by the Rust profile while the "
                 "entropyfs build is not, so the release default keeps the comparison fair to "
                 "VOLE. All wall times are recorded and reported in **microseconds (`us`)**.")
    lines.append("")
    lines.append("## Verdict")
    lines.append("")
    verdict = "PASS" if exact_ok == exact_n and exact_n > 0 else "FAIL"
    lines.append(f"- **VOLE exactness (Q8): {exact_ok}/{exact_n} byte-exact** "
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

    lines.append("## Paired ratios VOLE/sqlite (median + geometric mean, 95% CI by fixture)")
    lines.append("")
    lines.append("| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | "
                 "wins | ties | losses | ratio of sums |")
    lines.append("|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|")
    for label, series in (("build", build_us), ("storage", store_bytes), ("cold", cold_us), ("warm", warm_us)):
        other = "sqlite"
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
                 "repetitions** (best-of-N); storage is measured once after the last build. Every "
                 "raw sample is kept in `raw/build.tsv`, `raw/cold.tsv`, `raw/warm.tsv`. With "
                 f"only {len(fixtures)} fixture clusters the bootstrap is coarse and is stated as "
                 "such, not as a precise interval.")
    lines.append("")
    startup = (env or {}).get("python_startup_us") if isinstance(env, dict) else None
    lines.append("The SQLite cold path runs a fresh **Python** process per request, so its cold "
                 f"numbers include interpreter start-up as part of that lane's honest per-request "
                 f"cost (measured bare start-up {startup} us); VOLE's cold path is a native binary. "
                 "The cold ratio is therefore dominated by that constant and is reported for "
                 "completeness, not headlined.")
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
    lines.append("| Q | VOLE | SQLite (source-retaining) |")
    lines.append("|---|---|---|")
    qdesc = {
        "Q1": ("slide text (presentation-order index)", "slide text (sldIdLst order)"),
        "Q2": ("shape text (`--pptx-shape`, flattened pre-order)", "shape text (flattened pre-order)"),
        "Q3": ("notes text (`--pptx-notes`)", "notes text (via `notesSlide` relationship)"),
        "Q4": ("media decoded bytes (`--pptx-media --kind decoded`)", "media decoded bytes (extracted BLOB)"),
        "Q5": ("chart relationship id(s) on a slide (`--slide --kind structure`)",
               "chart relationship id(s) on a slide"),
        "Q6": ("slide part (relationship target)", "slide part + resolved layout/media/notes targets"),
        "Q7": ("slide member source span (`--slide --kind metadata` `source_span`)",
               "slide member raw span (`find` of the member record)"),
        "Q8": ("`materialize --exact` (byte-authority)", "retained source blob (byte-authority)"),
    }
    for q in qs:
        lines.append("| {} | {} | {} |".format(q, *qdesc[q]))
    lines.append("")

    lines.append("## Comparison normalization (contract-equivalence)")
    lines.append("")
    lines.append("Where two lanes answer the SAME question the comparable value is normalized so "
                 "the comparison is on the same contract and any projection is explicit (every "
                 "judgement call is listed so it can be audited):")
    lines.append("")
    lines.append("- **Q1/Q2/Q3 (text):** raw string equality of the projected text. The projection "
                 "is the adapter's exact rule: each shape's text is its runs concatenated within a "
                 "paragraph and paragraphs joined by `\\n`; a table frame contributes its cells "
                 "(`\\t` between cells, `\\n` between rows); `text_deep` joins a group's own text "
                 "with its descendants. The baseline mirrors that rule.")
    lines.append("- **Q2 (shape index):** the flattened pre-order shape index (groups counted as one "
                 "node before their children) — the same order `shape_by_flat_index` uses.")
    lines.append("- **Q3 (notes):** the corpus numbers its `notesSlideN.xml` parts so the "
                 "`N`-th notes part (name-sorted, as `--pptx-notes` resolves them) is exactly the "
                 "notes part linked from the `N`-th presentation slide; the baseline resolves the "
                 "same part through the slide's `notesSlide` relationship. This alignment is a "
                 "property of this self-authored corpus, stated as such.")
    lines.append("- **Q4 (media):** the SHA-256 of the DECODED media member bytes (`--kind decoded` "
                 "== `zipfile.read`); the media ordinal is the name-sorted part index (the adapter "
                 "sorts media parts by name).")
    lines.append("- **Q5 (chart reference):** the sorted set of chart relationship ids referenced "
                 "by the slide's shapes (`--slide --kind structure` exposes the shape `chart` field; "
                 "the baseline scans the slide's `<c:chart r:id>`); the resolved chart part name is "
                 "a lane detail. **This is not chart data.**")
    lines.append("- **Q6 (slide relationship):** the resolved slide part name in canonical "
                 "`/`-rooted form (`/ppt/slides/slideN.xml`); the layout/media/notes targets are "
                 "lane details.")
    lines.append("- **Q7 (source span):** `[start, end)` of the slide's ZIP member record in the "
                 "SOURCE (VOLE's raw-member `source_span` == the baseline's member-record span). "
                 "This is a byte-address observation of the source, not a decoded-XML digest. "
                 "**Judgement call:** the shipped VOLE surface exposes decoded member bytes only "
                 "for media (`--pptx-media --kind decoded`); it does **not** return a slide/shape's "
                 "decoded XML (the slide selector supports `text`/`structure`/`metadata` only). The "
                 "court therefore compares the contract-equivalent observable both lanes expose — "
                 "the slide member's source span — and records the *decoded slide XML digest* as a "
                 "VOLE capability gap it does not claim.")
    lines.append("- **Q8 (original bytes):** `{length, sha256}` of the whole presentation.")
    lines.append("")

    lines.append("## Scope (honest)")
    lines.append("")
    lines.append("- **Self-authored deterministic corpus, NOT a real-world population.** The eight "
                 "decks are generated by `tools/fixtures/make-pptx.py --corpus` (Python stdlib only; "
                 "no `python-pptx`; a fixed-state LCG grows each deck's PNG to a target size). Every "
                 "claim is scoped to these files; the aggregate carries a fixture-clustered CI and "
                 "is not extrapolated.")
    lines.append("- **Only Q8 is a byte-authority claim.** `materialize --exact == source` (length + "
                 "SHA-256 + `cmp`) and the retained blob reproduce the original bytes; that is the "
                 "archival invariant. **Every other observation (Q1–Q7) is a DERIVED projection** of "
                 "the PresentationML model. Semantic agreement is not archival equality.")
    lines.append("- **PPTX is not tabular.** No DuckDB/Parquet comparator is used here; the required "
                 "comparator is the source-retaining SQLite baseline. Chart *data* and image "
                 "*decoding* are out of scope for both lanes (Q5 is a chart reference, Q4 is opaque "
                 "resource bytes).")
    lines.append("- **VOLE capability gaps are recorded, never papered over.** Any question VOLE "
                 "declines is a typed decline (`rc` 6 / explicit code) and appears as a "
                 "`capability-gap` in the matrix; it is never claimed as equivalence. Concretely, "
                 "VOLE's shipped surface exposes no **decoded slide/shape XML digest** (only media "
                 "supports `--kind decoded`), so no such digest is compared; where a question's "
                 "feature is absent from both decks the row is a `both-decline`.")
    lines.append("- **Nothing here is run on the host.** Every command ran in the pinned "
                 "`doc-baseline` container (dev toolchain + python3 + sqlite3 + poppler).")
    lines.append("")

    matrix = []
    matrix.append("# Phase 21.2.3 — cross-lane Q1–Q8 answer matrix")
    matrix.append("")
    matrix.append("Derived / declined per lane, and the VOLE-vs-baseline equivalence. "
                  "`g` = answered (derived), `D` = typed decline, `-` = not applicable.")
    matrix.append("")
    matrix.append("| fixture | Q | VOLE | SQLite | VOLE↔SQLite |")
    matrix.append("|---|---|---|---|---|")
    for fx in fixtures:
        for q in qs:
            row = qanswers.get((fx, q), {})
            def mark(lane):
                e = row.get(lane)
                if e is None:
                    return "-"
                return "D" if e.get("declined") else "g"
            rs, _ = compare(q, row.get("vole"), row.get("sqlite"))
            matrix.append(f"| {fx} | {q} | {mark('vole')} | {mark('sqlite')} | {rs} |")
    matrix.append("")
    matrix.append("### Aggregate equivalence per Q")
    matrix.append("")
    matrix.append("| Q | comparator | equal | both-decline | capability-gap | mismatch | shape |")
    matrix.append("|---|---|---:|---:|---:|---:|---:|")
    for q in qs:
        c = equiv.get((q, "sqlite"), {})
        matrix.append("| {} | {} | {} | {} | {} | {} | {} |".format(
            q, "sqlite", c.get("equal", 0), c.get("both-decline", 0), c.get("capability-gap", 0),
            c.get("mismatch", 0), c.get("shape", 0)))
    matrix.append("")

    counts = []
    counts.append(f"fixtures {len(fixtures)}")
    counts.append(f"questions {len(qs)}")
    counts.append(f"exact_ok {exact_ok}")
    counts.append(f"exact_n {exact_n}")
    for q in qs:
        c = equiv.get((q, "sqlite"), {})
        counts.append(f"{q}.sqlite.equal {c.get('equal', 0)}")
        counts.append(f"{q}.sqlite.capability_gap {c.get('capability-gap', 0)}")
        counts.append(f"{q}.sqlite.mismatch {c.get('mismatch', 0)}")
        counts.append(f"{q}.sqlite.both_decline {c.get('both-decline', 0)}")
    counts.append(f"verdict {verdict}")

    with open(os.path.join(campaign, "SUMMARY.md"), "w") as f:
        f.write("\n".join(lines) + "\n")
    with open(os.path.join(campaign, "MATRIX.md"), "w") as f:
        f.write("\n".join(matrix) + "\n")
    with open(os.path.join(campaign, "counts.txt"), "w") as f:
        f.write("\n".join(counts) + "\n")

    receipt = {
        "campaign": campaign,
        "phase": "21.2.3 — PPTX economic court (VOLE vs source-retaining SQLite)",
        "verdict": verdict,
        "exact_ok": exact_ok,
        "exact_n": exact_n,
        "fixtures": fixtures,
        "lanes": lanes,
        "estimator": ("paired per-fixture ratio; median + geometric mean; fixed-seed cluster "
                      f"bootstrap by fixture ({B} resamples, seed {SEED}); tie band +/-{int(TIE*100)}%; "
                      "ratio of sums reported separately"),
        "equivalence": {f"{q}.sqlite": equiv.get((q, "sqlite"), {}) for q in qs},
        "environment": env,
    }
    with open(os.path.join(campaign, "receipt.json"), "w") as f:
        json.dump(receipt, f, indent=2, sort_keys=True)
    print("\n".join(lines))
    return 0 if verdict == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
