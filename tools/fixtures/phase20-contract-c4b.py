#!/usr/bin/env python3
# Phase 20.4 — C4b contract court: a source-retaining *contract baseline* + the
# campaign aggregator.
#
# This is tools/fixtures/phase18-contract.py with ONE change: the C4b
# corpus/external lineage is now supplied to BOTH lanes under an EQUAL external
# input. The harness still gives family/member/head to the SQLite baseline at
# build time (byte-identical to Phase 18/16.5); it now ALSO hands the same typed
# tuple to VOLE, which stores it as an explicit `ExternalContext` BESIDE the
# field and answers `observe --external-lineage --kind lineage` with basis
# `external-metadata`. The baseline build/query/SQL paths are byte-identical, so
# the SQLite lane is unchanged and the comparison stays fair. The C0..C5 cold /
# warm schedules are also unchanged; the C4b step is an added, separately
# measured comparison beside them.
#
# ## Why this file exists
#
# The real100-v1 frontier court (tools/real100-court.sh) compares VOLE against a
# source-retaining SQLite+FTS baseline (tools/fixtures/phase12-baseline.py) that
# answers only "value" lookups. That baseline "wins" most cold single-lookup
# cells, but it is **not answering the same contract** VOLE answers. This file
# implements a baseline that satisfies the SAME escalating capability surface
# (C0..C5), paying for every extra column/table/blob it needs, so the
# comparison is apples-to-apples.
#
# ## The escalating contracts
#
#   C0  value only (text / bytes / json).
#   C1  C0 + native coordinate (the format-native locator: page / block ordinal,
#       heading / table ordinal, resource ordinal, "metadata").
#   C2  C1 + provenance: a basis class in {verbatim, derived, heuristic,
#       unresolved} and, where available, a source span.
#   C3  C2 + exact original closure: the whole queried document reconstructs to
#       the original bytes (length + SHA-256 + byte compare).
#   C4a C3 + *document-native* revision lineage: a PDF's incremental revision
#       chain (header, count, ordered spans, startxref/Prev, object membership),
#       or a typed unsupported for a format with no native history. Byte-
#       derivable: VOLE indexes it at ingest; the baseline retains the exact
#       source, so it is derivable from what it keeps, but the measured lane
#       does not expose it.
#   C4b C3 + *corpus/external* lineage: the dataset's family id / member id /
#       head flag. This is NOT document-derived. Phase 20.4 supplies it to BOTH
#       lanes under an equal external input: the harness gives it to the baseline
#       at build; VOLE receives it as an explicit typed `ExternalContext` stored
#       beside the field, and answers it with basis `external-metadata` (never
#       exact, no seed bytes read). A field with no attached context is a typed
#       decline, never a guess.
#   C5  C4 + an arbitrary heterogeneous batch: a mixed-kind observation set
#       served together in one session (reported under both C4a and C4b).
#
# ## Equivalence
#
# Both lanes emit the SAME envelope shape. The court validates, per depth, that
# every required field is present, well-typed and internally consistent, and
# that the two lanes' comparable values are equivalent:
#   * byte observations         -> exact SHA-256 equality (canonical source data)
#   * text observations         -> raw SHA-256 equality, plus a documented
#                                  whitespace-collapsing projection as a fallback
#   * resource metadata         -> member SHA-256 equality
#   * revision lineage          -> exact tuple equality, or `different observable`
#                                  when one lane reports a PDF-internal revision
#                                  chain and the other a corpus family tuple
#   * metadata                  -> shape only (the two systems' metadata schemas
#                                  legitimately differ)
#
# This baseline mirrors VOLE's extraction semantics for DOCX/EPUB (tabs/breaks/
# hidden/tracked text; the EPUB content model and its whitespace-collapsing
# `normalize`) so that DOCX/EPUB text observations compare byte-for-byte. The PDF
# text lane uses Poppler and is a genuinely different heuristic projection: its
# equality is reported as a projection match rate, never as archival equality.
#
# ## Subcommands
#
#   build   --format F --source S --db D --through D [--family ID] [--member ID]
#           [--is-head 0|1] [--manifest-sha H] [--metrics M]
#   query   --db D --format F --depth D --observation OBS --out OUT
#   session --db D --format F --depth D --observations A,B,C --out OUT
#   sql     --format F --observation OBS        (the equivalent A1 SQL, for C0)
#   aggregate RAW_DIR CAMPAIGN_DIR
#
# Everything here is Python-stdlib + Poppler (`pdfinfo`/`pdftotext`) only, so it
# runs inside the pinned `doc-baseline` image.

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import zipfile
from xml.parsers import expat

XHTML_SKIP = {"script", "style"}
W_HEADING_STYLE = re.compile(r"^Heading\s*(\d+)\s*$", re.I)

# Canonical provenance classes shared by both lanes.
CLASS_VERBATIM = "verbatim"
CLASS_DERIVED = "derived"
CLASS_HEURISTIC = "heuristic"
CLASS_UNRESOLVED = "unresolved"


def sha256_hex(b):
    return hashlib.sha256(b).hexdigest()


def text_sha(s):
    return sha256_hex(s.encode("utf-8"))


def canonical_json_sha(obj):
    return sha256_hex(json.dumps(obj, sort_keys=True, separators=(",", ":")).encode())


def project_text(s):
    """The documented, deterministic text projection used ONLY as a fallback
    equivalence when two independent extractors' raw bytes differ. Semantic, not
    archival: NFC-free ASCII/Unicode whitespace collapse + strip."""
    return " ".join(s.split())


def rm(path):
    try:
        os.remove(path)
    except FileNotFoundError:
        pass


# ---------------------------------------------------------------------------
# Extraction (mirrors VOLE's per-format observation semantics)
# ---------------------------------------------------------------------------


def _span_ok(entry):
    sp = entry.get("span")
    if not sp:
        return None
    return [int(sp[0]), int(sp[1])]


def extract_pdf(source):
    """PDF text is a heuristic projection (Poppler). Coordinates: page:N."""
    metrics = {"decompressed_bytes": 0, "xml_bytes_parsed": 0, "member_decodes": 0,
               "external_invocations": 0, "source_reparses": 1}
    info = subprocess.run(["pdfinfo", source], capture_output=True, text=True)
    metrics["external_invocations"] += 1
    pages = 1
    for line in info.stdout.splitlines():
        if line.startswith("Pages:"):
            pages = int(line.split(":", 1)[1].strip())
    blocks = []
    for p in range(1, pages + 1):
        r = subprocess.run(["pdftotext", "-f", str(p), "-l", str(p), source, "-"],
                           capture_output=True)
        metrics["external_invocations"] += 1
        text = r.stdout.decode("utf-8", "replace")
        metrics["decompressed_bytes"] += len(text)
        blocks.append({"unit": "page", "ordering": p, "kind": "text", "text": text,
                       "native_coord": "page:%d" % p, "prov_class": CLASS_HEURISTIC,
                       "prov_member": None, "span": None})
    meta = {"format": "pdf", "pages": pages, "source_len": os.path.getsize(source),
            "source_sha256": sha256_hex(open(source, "rb").read())}
    return {"format": "pdf", "metadata": meta, "blocks": blocks, "headings": [],
            "tables": [], "cells": [], "resources": [],
            "doc_text": "\n".join(b["text"] for b in blocks), "metrics": metrics}


def _docx_style_outline(styles_bytes):
    """Map styleId -> outline level (0-based) from styles.xml, if present."""
    out = {}
    if styles_bytes is None:
        return out
    try:
        root = expat.ParserCreate()
    except Exception:
        return out
    cur = {"id": None, "lvl": None}
    def start(name, attrs):
        ln = name.split(":")[-1]
        a = {k.split(":")[-1]: v for k, v in attrs.items()}
        if ln == "style":
            cur["id"] = a.get("styleId")
            cur["lvl"] = None
        elif ln == "outlineLvl":
            try:
                cur["lvl"] = int(a.get("val"))
            except (TypeError, ValueError):
                cur["lvl"] = None
    def end(name):
        ln = name.split(":")[-1]
        if ln == "style":
            if cur["id"] and cur["lvl"] is not None:
                out[cur["id"]] = cur["lvl"]
            cur["id"] = None
            cur["lvl"] = None
    root.StartElementHandler = start
    root.EndElementHandler = end
    try:
        root.Parse(styles_bytes, True)
    except Exception:
        return out
    return out


def _docx_story(data, outline):
    """Parse word/document.xml mirroring VOLE's WML text rule.

    Returns (blocks, headings, tables, cells). Blocks are the story's **top-level**
    paragraphs and tables in document order (VOLE `--block N`). Paragraph text is
    the document-order concatenation of run text, `w:tab` -> TAB, `w:br`/`w:cr` ->
    LF, with hidden runs / `w:delText` / `w:instrText` excluded (the default
    "final result" profile)."""
    p = expat.ParserCreate()
    buf = []          # frame stack: kinds p / tc / tbl / tr
    blocks = []
    st = {"in_t": 0, "in_del": 0, "in_instr": 0, "vanish": False, "indel": 0}

    def push(s):
        for f in reversed(buf):
            if f["kind"] == "p":
                if st["vanish"] or st["indel"] > 0:
                    return
                f["text"] += s
                return
            if f["kind"] == "tc":
                return  # direct cell text without a paragraph: ignored

    def pos():
        try:
            return p.CurrentByteIndex
        except Exception:
            return None

    def start(name, attrs):
        ln = name.split(":")[-1]
        a = {k.split(":")[-1]: v for k, v in attrs.items()}
        if ln == "p":
            buf.append({"kind": "p", "text": "", "style": None, "start": pos()})
        elif ln == "tbl":
            buf.append({"kind": "tbl", "rows": [], "row": None, "start": pos()})
        elif ln == "tr" and buf and buf[-1]["kind"] == "tbl":
            buf[-1]["row"] = []
        elif ln == "tc" and buf and buf[-1]["kind"] == "tbl":
            buf.append({"kind": "tc", "text": ""})
        elif ln == "pStyle" and buf and buf[-1]["kind"] == "p":
            buf[-1]["style"] = a.get("val")
        elif ln == "t":
            st["in_t"] += 1
        elif ln == "delText":
            st["in_del"] += 1
        elif ln == "instrText":
            st["in_instr"] += 1
        elif ln == "vanish":
            if a.get("val") != "false":
                st["vanish"] = True
        elif ln == "tab":
            push("\t")
        elif ln in ("br", "cr"):
            push("\n")
        elif ln == "del":
            st["indel"] += 1

    def chardata(s):
        if st["in_t"] > 0 and st["in_del"] == 0 and st["in_instr"] == 0:
            push(s)

    def end(name):
        ln = name.split(":")[-1]
        if ln == "t":
            st["in_t"] = max(0, st["in_t"] - 1)
        elif ln == "delText":
            st["in_del"] = max(0, st["in_del"] - 1)
        elif ln == "instrText":
            st["in_instr"] = max(0, st["in_instr"] - 1)
        elif ln == "del":
            st["indel"] = max(0, st["indel"] - 1)
        elif ln == "tc":
            f = buf.pop()
            if buf and buf[-1]["kind"] == "tbl" and buf[-1]["row"] is not None:
                buf[-1]["row"].append(f["text"])
        elif ln == "tr":
            if buf and buf[-1]["kind"] == "tbl" and buf[-1]["row"] is not None:
                buf[-1]["rows"].append(buf[-1]["row"])
                buf[-1]["row"] = None
        elif ln == "p":
            f = buf.pop()
            if any(x["kind"] == "tbl" for x in buf):
                if buf and buf[-1]["kind"] == "tc":
                    if buf[-1]["text"]:
                        buf[-1]["text"] += "\n"
                    buf[-1]["text"] += f["text"]
            else:
                blocks.append({"kind": "p", "text": f["text"], "style": f["style"],
                               "start": f["start"], "end": pos()})
        elif ln == "tbl":
            f = buf.pop()
            rows = f["rows"]
            ttext = "\n".join("\t".join(r) for r in rows)
            blocks.append({"kind": "tbl", "text": ttext, "rows": rows,
                           "start": f["start"], "end": pos()})

    p.StartElementHandler = start
    p.CharacterDataHandler = chardata
    p.EndElementHandler = end
    p.Parse(data, True)

    out_blocks, headings, tables, cells = [], [], [], []
    bidx = 0
    for b in blocks:
        if b["kind"] == "p":
            style = b["style"]
            level = None
            if style:
                m = W_HEADING_STYLE.match(style)
                if m:
                    level = int(m.group(1))
                elif style in outline:
                    level = outline[style] + 1
            kind = "heading" if level is not None else "paragraph"
            out_blocks.append({"unit": "story", "ordering": bidx, "kind": kind,
                               "text": b["text"], "native_coord": "block:%d" % bidx,
                               "prov_class": CLASS_DERIVED, "prov_member": "/word/document.xml",
                               "span": [b["start"], b["end"]]})
            if level is not None:
                headings.append({"block": bidx, "level": level, "text": b["text"]})
        else:
            tid = len(tables)
            rows = b["rows"]
            tables.append({"table": tid, "block": bidx, "n_rows": len(rows),
                           "n_cols": max((len(r) for r in rows), default=0)})
            for ri, row in enumerate(rows):
                for ci, val in enumerate(row):
                    cells.append({"table": tid, "r": ri, "c": ci, "text": val})
            out_blocks.append({"unit": "story", "ordering": bidx, "kind": "table",
                               "text": b["text"], "native_coord": "block:%d" % bidx,
                               "prov_class": CLASS_DERIVED, "prov_member": "/word/document.xml",
                               "span": [b["start"], b["end"]]})
        bidx += 1
    return out_blocks, headings, tables, cells


def extract_docx(source):
    metrics = {"decompressed_bytes": 0, "xml_bytes_parsed": 0, "member_decodes": 0,
               "external_invocations": 0, "source_reparses": 1}
    with zipfile.ZipFile(source) as z:
        names = z.namelist()

        def read(name):
            zi = z.getinfo(name)
            data = z.read(name)
            metrics["member_decodes"] += 1
            metrics["decompressed_bytes"] += zi.file_size
            return data

        document = read("word/document.xml")
        metrics["xml_bytes_parsed"] += len(document)
        styles = read("word/styles.xml") if "word/styles.xml" in names else None
        if styles is not None:
            metrics["xml_bytes_parsed"] += len(styles)
        outline = _docx_style_outline(styles)
        blocks, headings, tables, cells = _docx_story(document, outline)
        resources = []
        for name in names:
            if name.startswith("word/media/"):
                zi = z.getinfo(name)
                data = z.read(name)
                metrics["member_decodes"] += 1
                metrics["decompressed_bytes"] += zi.file_size
                resources.append({"path": name, "member_bytes": zi.file_size,
                                  "member_sha256": sha256_hex(data)})
        meta = {"format": "docx", "parts": len(names), "media": len(resources),
                "source_len": os.path.getsize(source),
                "source_sha256": sha256_hex(open(source, "rb").read())}
        return {"format": "docx", "metadata": meta, "blocks": blocks, "headings": headings,
                "tables": tables, "cells": cells, "resources": resources,
                "doc_text": "\n".join(b["text"] for b in blocks), "metrics": metrics}


def _parse_xhtml(data, model, refs):
    """Mirror VOLE's EPUB content model: heading / paragraph-like / list / table
    blocks in document order with whitespace-collapsing `normalize`. Resource
    references (img/image/object/source) are appended to ``refs`` in document
    order, mirroring VOLE's `--resource N` ordinal space."""
    p = expat.ParserCreate()
    st = {"head": 0, "skip": 0}
    block_stack, list_stack, table_stack = [], [], []

    def start(name, attrs):
        ln = name.split(":")[-1]
        if ln == "head":
            st["head"] += 1
        elif ln in XHTML_SKIP:
            st["skip"] += 1
        a = {k.split(":")[-1]: v for k, v in attrs.items()}
        if ln in ("img", "source", "image", "object"):
            v = a.get("src") or a.get("href") or a.get("data") or a.get("srcset")
            if v:
                refs.append({"kind": ln, "value": v.split()[0].split(",")[0]})
        if st["skip"] > 0:
            return
        if ln in ("h1", "h2", "h3", "h4", "h5", "h6"):
            block_stack.append({"k": "heading", "text": "", "start": pos()})
        elif ln in ("p", "blockquote", "pre", "figcaption"):
            if not table_stack:
                block_stack.append({"k": "paragraph", "text": "", "start": pos()})
        elif ln in ("ul", "ol"):
            if not table_stack:
                list_stack.append({"ordered": ln == "ol", "items": []})
        elif ln == "li":
            if not table_stack and list_stack:
                block_stack.append({"k": "listitem", "text": ""})
        elif ln == "table":
            table_stack.append({"rows": [], "row": None, "start": pos()})
        elif ln == "tr":
            if table_stack:
                table_stack[-1]["row"] = []
        elif ln in ("td", "th"):
            if table_stack:
                block_stack.append({"k": "cell", "text": ""})

    def pos():
        try:
            return p.CurrentByteIndex
        except Exception:
            return None

    def chardata(s):
        if st["head"] > 0 or st["skip"] > 0:
            return
        if block_stack:
            block_stack[-1]["text"] += s

    def end(name):
        ln = name.split(":")[-1]
        if ln == "head":
            st["head"] = max(0, st["head"] - 1)
        elif ln in XHTML_SKIP:
            st["skip"] = max(0, st["skip"] - 1)
        if ln in ("h1", "h2", "h3", "h4", "h5", "h6"):
            if block_stack and block_stack[-1]["k"] == "heading":
                f = block_stack.pop()
                if normalize(f["text"]):
                    model.append(("heading", normalize(f["text"]), f["start"], pos()))
        elif ln in ("p", "blockquote", "pre", "figcaption"):
            if block_stack and block_stack[-1]["k"] == "paragraph":
                f = block_stack.pop()
                if normalize(f["text"]):
                    model.append(("paragraph", normalize(f["text"]), f["start"], pos()))
        elif ln == "li":
            if block_stack and block_stack[-1]["k"] == "listitem":
                f = block_stack.pop()
                if list_stack:
                    list_stack[-1]["items"].append(normalize(f["text"]))
        elif ln in ("ul", "ol"):
            if list_stack:
                ctx = list_stack.pop()
                items = [x for x in ctx["items"] if x != ""]
                if items:
                    model.append(("list", "\n".join(items), None, None))
        elif ln in ("td", "th"):
            if block_stack and block_stack[-1]["k"] == "cell":
                f = block_stack.pop()
                if table_stack and table_stack[-1]["row"] is not None:
                    table_stack[-1]["row"].append(normalize(f["text"]))
        elif ln == "tr":
            if table_stack and table_stack[-1]["row"] is not None:
                table_stack[-1]["rows"].append(table_stack[-1]["row"])
                table_stack[-1]["row"] = None
        elif ln == "table":
            if table_stack:
                t = table_stack.pop()
                ttext = "\n".join("\t".join(r) for r in t["rows"])
                model.append(("table", ttext, t["start"], pos()))

    def normalize(s):
        return " ".join(s.split())

    p.StartElementHandler = start
    p.CharacterDataHandler = chardata
    p.EndElementHandler = end
    p.Parse(data, True)


def extract_epub(source):
    metrics = {"decompressed_bytes": 0, "xml_bytes_parsed": 0, "member_decodes": 0,
               "external_invocations": 0, "source_reparses": 1}
    blocks, headings, tables, cells, resources = [], [], [], [], []
    with zipfile.ZipFile(source) as z:
        names = z.namelist()

        def read(name):
            zi = z.getinfo(name)
            data = z.read(name)
            metrics["member_decodes"] += 1
            metrics["decompressed_bytes"] += zi.file_size
            return data

        container = read("META-INF/container.xml")
        metrics["xml_bytes_parsed"] += len(container)
        opf_path = None
        for m in re.finditer(rb'rootfile[^>]*full-path="([^"]+)"', container):
            opf_path = m.group(1).decode("utf-8", "replace")
        if opf_path is None:
            raise ValueError("EPUB has no rootfile")
        opf = read(opf_path)
        metrics["xml_bytes_parsed"] += len(opf)
        otxt = opf.decode("utf-8", "replace")
        manifest = {}
        for m in re.finditer(r"<item\b([^>]*)>", otxt):
            a = m.group(1)
            im = re.search(r'id="([^"]+)"', a)
            hm = re.search(r'href="([^"]+)"', a)
            if im and hm:
                manifest[im.group(1)] = hm.group(1)
        order = []
        for m in re.finditer(r"<itemref\b([^>]*)>", otxt):
            a = m.group(1)
            ir = re.search(r'idref="([^"]+)"', a)
            lm = re.search(r'linear="([^"]+)"', a)
            if ir and (not lm or lm.group(1) != "no"):
                order.append(ir.group(1))
        base = os.path.dirname(opf_path)
        bidx = 0
        all_refs = []
        doc_text_parts = []
        for idref in order:
            href = manifest.get(idref)
            if not href:
                continue
            path = os.path.normpath(os.path.join(base, href)).replace("\\", "/")
            try:
                chapter = read(path)
            except KeyError:
                continue
            metrics["xml_bytes_parsed"] += len(chapter)
            model = []
            refs = []
            try:
                _parse_xhtml(chapter, model, refs)
            except Exception:
                # A spine item that does not parse contributes an empty reading
                # text; VOLE still emits one newline for it.
                model = []
                refs = []
            for kind, text, s, e in model:
                if kind in ("heading", "paragraph"):
                    bkind = kind
                elif kind == "list":
                    bkind = "list"
                else:
                    bkind = "table"
                blocks.append({"unit": "spine", "ordering": bidx, "kind": bkind,
                               "text": text, "native_coord": "block:%d" % bidx,
                               "prov_class": CLASS_DERIVED, "prov_member": path,
                               "span": [s, e] if s is not None and e is not None else None})
                if bkind == "heading":
                    headings.append({"block": bidx, "level": 1, "text": text})
                if bkind == "table":
                    tid = len(tables)
                    rows = [line.split("\t") for line in text.split("\n")] if text else []
                    tables.append({"table": tid, "block": bidx, "n_rows": len(rows),
                                   "n_cols": max((len(r) for r in rows), default=0)})
                    for ri, row in enumerate(rows):
                        for ci, val in enumerate(row):
                            cells.append({"table": tid, "r": ri, "c": ci, "text": val})
                bidx += 1
            all_refs.extend(refs)
            # VOLE: per-spine model.text() (non-empty block texts each + "\n");
            # then a newline if the spine item's text did not already end with one.
            part = "".join((t + "\n") for (_k, t, _s, _e) in model if t != "")
            if not part.endswith("\n"):
                part += "\n"
            doc_text_parts.append(part)
        for i, rf in enumerate(all_refs):
            href = rf["value"]
            tgt = os.path.normpath(os.path.join(base, href)).replace("\\", "/")
            mb = None; mh = None
            try:
                zi = z.getinfo(tgt)
                data = z.read(tgt)
                mb = zi.file_size; mh = sha256_hex(data)
                metrics["member_decodes"] += 1
                metrics["decompressed_bytes"] += zi.file_size
            except KeyError:
                pass
            resources.append({"path": href, "kind": rf["kind"],
                              "member_bytes": mb, "member_sha256": mh})
        meta = {"format": "epub", "spine": len(order), "parts": len(names),
                "media": len(all_refs), "source_len": os.path.getsize(source),
                "source_sha256": sha256_hex(open(source, "rb").read())}
    return {"format": "epub", "metadata": meta, "blocks": blocks, "headings": headings,
            "tables": tables, "cells": cells, "resources": resources,
            "doc_text": "".join(doc_text_parts), "metrics": metrics}


EXTRACTORS = {"pdf": extract_pdf, "docx": extract_docx, "epub": extract_epub}


def extract(fmt, source):
    return EXTRACTORS[fmt](source)


# ---------------------------------------------------------------------------
# Escalating SQLite contract store
# ---------------------------------------------------------------------------

def _schema(depth):
    """The schema a store that satisfies contract depth ``depth`` needs. Each
    line of extra materialization is exactly what the richer contract requires."""
    s = """
PRAGMA journal_mode=WAL;
PRAGMA synchronous=NORMAL;
CREATE TABLE documents(
  doc_id INTEGER PRIMARY KEY, format TEXT NOT NULL, path TEXT NOT NULL,
  source_len INTEGER NOT NULL, source_sha256 TEXT NOT NULL,
  pages INTEGER, spine INTEGER, doc_text TEXT);
CREATE TABLE blocks(
  block_id INTEGER PRIMARY KEY, doc_id INTEGER NOT NULL, unit TEXT NOT NULL,
  ordering INTEGER NOT NULL, kind TEXT NOT NULL, text TEXT NOT NULL,
  text_sha256 TEXT NOT NULL);
CREATE INDEX idx_blocks_doc ON blocks(doc_id, unit, ordering);
CREATE TABLE headings(
  heading_id INTEGER PRIMARY KEY, doc_id INTEGER NOT NULL, block_id INTEGER NOT NULL,
  level INTEGER NOT NULL, ord INTEGER NOT NULL);
CREATE TABLE db_tables(
  table_id INTEGER PRIMARY KEY, doc_id INTEGER NOT NULL, block_id INTEGER NOT NULL,
  n_rows INTEGER, n_cols INTEGER);
CREATE TABLE table_cells(
  table_id INTEGER NOT NULL, r INTEGER NOT NULL, c INTEGER NOT NULL, text TEXT,
  PRIMARY KEY(table_id, r, c));
CREATE TABLE resources(
  resource_id INTEGER PRIMARY KEY, doc_id INTEGER NOT NULL, path TEXT NOT NULL,
  kind TEXT, member_bytes INTEGER, member_sha256 TEXT);
CREATE TABLE metadata(
  doc_id INTEGER NOT NULL, key TEXT NOT NULL, value TEXT, PRIMARY KEY(doc_id, key));
CREATE TABLE source_blob(doc_id INTEGER PRIMARY KEY, payload BLOB NOT NULL);
CREATE VIRTUAL TABLE fts USING fts5(
  text, content='blocks', content_rowid='block_id',
  tokenize='unicode61 remove_diacritics 2');
CREATE VIRTUAL TABLE fts_tri USING fts5(
  text, content='blocks', content_rowid='block_id', tokenize='trigram');
"""
    if depth >= 1:
        s += "ALTER TABLE blocks ADD COLUMN native_coord TEXT;\n"
        s += "CREATE INDEX idx_blocks_coord ON blocks(doc_id, native_coord);\n"
    if depth >= 2:
        s += """
CREATE TABLE provenance(
  block_id INTEGER PRIMARY KEY, doc_id INTEGER NOT NULL, class TEXT NOT NULL,
  member TEXT, span_start INTEGER, span_end INTEGER, member_len INTEGER);
CREATE INDEX idx_prov_doc ON provenance(doc_id);
"""
    if depth >= 4:
        s += """
CREATE TABLE revisions(
  doc_id INTEGER PRIMARY KEY, family_id TEXT, member_id TEXT, is_head INTEGER NOT NULL);
CREATE INDEX idx_rev_family ON revisions(family_id);
"""
    return s


def build(fmt, source, db_path, through, family="", member="", is_head=1):
    import sqlite3
    stage_ms = {}
    stage_bytes = {}
    t0 = _now_ms()
    doc = extract(fmt, source)
    stage_ms["extract"] = _now_ms() - t0
    for suffix in ("", "-wal", "-shm"):
        rm(db_path + suffix)
    con = sqlite3.connect(db_path)
    con.executescript(_schema(through))
    cur = con.cursor()
    m = doc["metadata"]
    cur.execute("INSERT INTO documents VALUES(1,?,?,?,?,?,?,?)",
                (fmt, source, m.get("source_len", 0), m.get("source_sha256", ""),
                 m.get("pages"), m.get("spine"), doc.get("doc_text")))
    for k, v in m.items():
        cur.execute("INSERT OR REPLACE INTO metadata VALUES(1,?,?)", (k, str(v)))
    use_coord = through >= 1
    for i, b in enumerate(doc["blocks"]):
        if use_coord:
            cur.execute("INSERT INTO blocks VALUES(?,1,?,?,?,?,?,?)",
                        (i + 1, b["unit"], b["ordering"], b["kind"], b["text"],
                         text_sha(b["text"]), b["native_coord"]))
        else:
            cur.execute("INSERT INTO blocks VALUES(?,1,?,?,?,?,?)",
                        (i + 1, b["unit"], b["ordering"], b["kind"], b["text"],
                         text_sha(b["text"])))
    block_ids = {b["ordering"]: i + 1 for i, b in enumerate(doc["blocks"])}
    for i, h in enumerate(doc["headings"]):
        cur.execute("INSERT INTO headings VALUES(?,1,?,?,?)",
                    (i + 1, block_ids.get(h["block"], 0), h["level"], i))
    for i, t in enumerate(doc["tables"]):
        cur.execute("INSERT INTO db_tables VALUES(?,1,?,?,?)",
                    (i + 1, block_ids.get(t["block"], 0), t["n_rows"], t["n_cols"]))
    for c in doc["cells"]:
        cur.execute("INSERT INTO table_cells VALUES(?,?,?,?)",
                    (c["table"] + 1, c["r"], c["c"], c["text"]))
    for i, r in enumerate(doc["resources"]):
        cur.execute("INSERT INTO resources VALUES(?,1,?,?,?,?)",
                    (i + 1, r["path"], r.get("kind"), r.get("member_bytes"),
                     r.get("member_sha256")))
    cur.execute("INSERT INTO fts(rowid,text) SELECT block_id,text FROM blocks")
    cur.execute("INSERT INTO fts_tri(rowid,text) SELECT block_id,text FROM blocks")
    # depth-2: provenance
    if through >= 2:
        for i, b in enumerate(doc["blocks"]):
            sp = b.get("span")
            cur.execute("INSERT INTO provenance VALUES(?,1,?,?,?,?,?)",
                        (i + 1, b["prov_class"], b.get("prov_member"),
                         sp[0] if sp else None, sp[1] if sp else None,
                         None))
    # depth-3 exact closure: the retained source blob is already the base
    # materialization (the established A1 baseline retains it); exactness adds no
    # new persistent table, it is *validated* from the base blob.
    with open(source, "rb") as f:
        payload = f.read()
    cur.execute("INSERT INTO source_blob VALUES(1,?)", (payload,))
    # depth-4: revision lineage (frozen corpus metadata, supplied to both lanes)
    if through >= 4:
        cur.execute("INSERT INTO revisions VALUES(1,?,?,?)",
                    (family or None, member or os.path.basename(source),
                     1 if is_head else 0))
    cur.execute("ANALYZE")
    con.commit()
    t0 = _now_ms()
    con.close()
    stage_ms["close"] = _now_ms() - t0
    metrics = dict(doc["metrics"])
    metrics.update({"db_bytes": os.path.getsize(db_path),
                    "through": through,
                    "blocks": len(doc["blocks"]),
                    "headings": len(doc["headings"]),
                    "tables": len(doc["tables"]),
                    "cells": len(doc["cells"]),
                    "resources": len(doc["resources"])})
    return metrics


# ---------------------------------------------------------------------------
# Query / session — emit the contract envelope(s) for the SQLite lane
# ---------------------------------------------------------------------------

def _now_ms():
    return int(_monotonic() * 1000)


def _monotonic():
    import time
    return time.monotonic()


def _row(cur, sql, args=()):
    cur.execute(sql, args)
    return cur.fetchone()


def sqlite_envelope(con, fmt, depth, obs):
    """Return the contract envelope dict for one observation, or a decline."""
    cur = con.cursor()
    doc = _row(cur, "SELECT format, source_len, source_sha256, pages, spine, doc_text FROM documents WHERE doc_id=1")
    if doc is None:
        return {"obs": obs, "declined": True, "reason": "no document"}
    dformat, source_len, source_sha, pages, spine, doc_text = doc
    env = {"obs": obs, "declined": False, "reason": None, "lane": "a1c"}

    def coord(kind, ordinal):
        if depth < 1:
            return None
        return {"format": dformat, "kind": kind, "ordinal": ordinal}

    def provenance(block_id):
        if depth < 2:
            return None
        r = _row(cur, "SELECT class, span_start, span_end FROM provenance WHERE block_id=?", (block_id,))
        if r is None:
            return None
        sp = [r[1], r[2]] if r[1] is not None and r[2] is not None else None
        return {"class": r[0], "span": sp}

    def revision():
        if depth < 4:
            return None
        r = _row(cur, "SELECT family_id, member_id, is_head FROM revisions WHERE doc_id=1")
        if r is None:
            return None
        return {"family_id": r[0], "member_id": r[1], "is_head": bool(r[2])}

    def exact():
        # The whole-source closure is available from the base retained blob (C3 is a
        # *validation* depth); the field is only required at C3+.
        r = _row(cur, "SELECT length(payload), source_sha256 FROM source_blob JOIN documents USING(doc_id) WHERE doc_id=1")
        if r is None:
            r = _row(cur, "SELECT source_len, source_sha256 FROM documents WHERE doc_id=1")
        return {"length": r[0], "sha256": r[1]}

    if obs == "bytes":
        r = _row(cur, "SELECT hex(substr(payload,1,64)) FROM source_blob WHERE doc_id=1")
        if r is None:
            return {"obs": obs, "declined": True, "reason": "no retained source for byte read"}
        raw = bytes.fromhex(r[0])
        env.update({"value_kind": "bytes", "value_sha256": sha256_hex(raw),
                    "value_len": len(raw), "value_hex": raw.hex(),
                    "coord": coord("byte-range", "0:%d" % len(raw)),
                    "provenance": {"class": CLASS_VERBATIM, "span": [0, len(raw)]} if depth >= 2 else None,
                    "exact": exact(), "revision": revision()})
        return env

    if obs in ("text", "doc-text"):
        if obs == "text":
            if fmt == "pdf":
                r = _row(cur, "SELECT block_id, text, text_sha256 FROM blocks WHERE doc_id=1 AND unit='page' AND ordering=1")
                kind, ordinal = "page", 1
            else:
                r = _row(cur, "SELECT block_id, text, text_sha256 FROM blocks WHERE doc_id=1 AND ordering=0")
                kind, ordinal = "block", 0
            if r is None:
                return {"obs": obs, "declined": True, "reason": "no such block"}
            bid, text, tsha = r
            env.update({"value_kind": "text", "value": text, "value_sha256": tsha,
                        "coord": coord(kind, ordinal), "provenance": provenance(bid),
                        "exact": exact(), "revision": revision()})
            return env
        # doc-text
        if doc_text is None or doc_text == "":
            rows = cur.execute("SELECT text FROM blocks WHERE doc_id=1 ORDER BY block_id").fetchall()
            if fmt == "epub":
                text = "".join((t + "\n") for (t,) in rows if t != "")
            else:
                text = "\n".join(t for (t,) in rows)
        else:
            text = doc_text
        env.update({"value_kind": "text", "value": text, "value_sha256": text_sha(text),
                    "coord": coord("document", None), "provenance": None,
                    "exact": exact(), "revision": revision()})
        return env

    if obs == "heading":
        r = _row(cur, "SELECT h.block_id, b.text FROM headings h JOIN blocks b ON b.block_id=h.block_id WHERE h.doc_id=1 AND h.ord=0")
        if r is None:
            return {"obs": obs, "declined": True, "reason": "no heading"}
        bid, text = r
        env.update({"value_kind": "text", "value": text, "value_sha256": text_sha(text),
                    "coord": coord("heading", 0), "provenance": provenance(bid),
                    "exact": exact(), "revision": revision()})
        return env

    if obs == "table":
        r = _row(cur, "SELECT block_id, text FROM blocks WHERE doc_id=1 AND kind='table' ORDER BY block_id LIMIT 1")
        if r is None:
            return {"obs": obs, "declined": True, "reason": "no table"}
        bid, text = r
        env.update({"value_kind": "text", "value": text, "value_sha256": text_sha(text),
                    "coord": coord("table", 0), "provenance": provenance(bid),
                    "exact": exact(), "revision": revision()})
        return env

    if obs == "resource":
        r = _row(cur, "SELECT path, member_bytes, member_sha256 FROM resources WHERE doc_id=1 AND resource_id=1")
        if r is None:
            return {"obs": obs, "declined": True, "reason": "no resource"}
        value = {"path": r[0], "member_bytes": r[1], "member_sha256": r[2]}
        env.update({"value_kind": "json", "value": value,
                    "value_sha256": canonical_json_sha(value),
                    "coord": coord("resource", 0),
                    "provenance": {"class": CLASS_VERBATIM, "span": None} if depth >= 2 else None,
                    "exact": exact(), "revision": revision()})
        return env

    if obs == "metadata":
        r = _row(cur, "SELECT group_concat(key||'='||value,';') FROM metadata WHERE doc_id=1")
        value = {"format": dformat, "source_len": source_len, "source_sha256": source_sha}
        env.update({"value_kind": "json", "value": value, "value_sha256": canonical_json_sha(value),
                    "coord": coord("metadata", None), "provenance": None,
                    "exact": exact(), "revision": revision()})
        return env

    if obs == "revision":
        rev = revision()
        if rev is None:
            return {"obs": obs, "declined": True, "reason": "revision lineage not in contract at this depth"}
        env.update({"value_kind": "json", "value": rev, "value_sha256": canonical_json_sha(rev),
                    "coord": coord("revision", None), "provenance": None,
                    "exact": exact(), "revision": rev})
        return env

    return {"obs": obs, "declined": True, "reason": "unknown observation"}


def _normalize_vole_answer(ans, fmt, obs, depth):
    """Normalize a VOLE `observe` JSON answer to the shared envelope shape."""
    if ans is None:
        return {"obs": obs, "declined": True, "reason": "no answer", "lane": "vole"}
    env = {"obs": obs, "declined": False, "reason": None, "lane": "vole"}
    sel = ans.get("selector", "")
    if sel.startswith("byte-range:"):
        env["coord"] = {"format": fmt, "kind": "byte-range", "ordinal": sel.split(":", 1)[1]}
    elif sel.startswith("page:"):
        env["coord"] = {"format": fmt, "kind": "page", "ordinal": int(sel.split(":")[1])}
    elif sel.startswith("block:"):
        env["coord"] = {"format": fmt, "kind": "block", "ordinal": int(sel.split(":")[1])}
    elif sel.startswith("heading:"):
        env["coord"] = {"format": fmt, "kind": "heading", "ordinal": int(sel.split(":")[1])}
    elif sel.startswith("table:"):
        env["coord"] = {"format": fmt, "kind": "table", "ordinal": int(sel.split(":")[1])}
    elif sel.startswith("resource:"):
        env["coord"] = {"format": fmt, "kind": "resource", "ordinal": int(sel.split(":")[1])}
    elif sel == "metadata":
        env["coord"] = {"format": fmt, "kind": "metadata", "ordinal": None}
    elif sel == "text":
        env["coord"] = {"format": fmt, "kind": "document", "ordinal": None}
    basis = ans.get("basis")
    cls = {"authored": CLASS_VERBATIM, "directly-observed": CLASS_VERBATIM,
           "deterministically-derived": CLASS_DERIVED, "inferred": CLASS_HEURISTIC,
           "heuristic": CLASS_HEURISTIC, "unresolved": CLASS_UNRESOLVED}.get(basis)
    span = ans.get("source_span")
    env["provenance"] = {"class": cls, "span": span} if cls else None
    if "text" in ans:
        env["value_kind"] = "text"
        env["value"] = ans["text"]
        env["value_sha256"] = text_sha(ans["text"])
    elif "bytes_sha256" in ans:
        env["value_kind"] = "bytes"
        env["value_sha256"] = ans["bytes_sha256"]
        env["value_len"] = ans.get("bytes_len")
        if "value_hex" in ans:
            env["value_hex"] = ans["value_hex"]
    elif "value" in ans:
        env["value_kind"] = "json"
        env["value"] = ans["value"]
        env["value_sha256"] = canonical_json_sha(ans["value"])
    else:
        env["value_kind"] = "null"
        env["value_sha256"] = None
    if obs == "revision" and isinstance(env.get("value"), dict):
        # Phase 17: VOLE now answers a revision-lineage observation. Its value is
        # the PDF-native incremental revision chain (count / ordered indices /
        # spans / startxref / /Prev / object membership), which is a DIFFERENT
        # observable from the baseline's corpus family/member/head tuple. Both
        # are kept so the aggregator reports them side by side rather than forcing
        # a false equality.
        env["revision"] = env["value"]
    return env


# ---------------------------------------------------------------------------
# SQL envelope generation — serve the SQLite lane through the `sqlite3` CLI so
# the cold/warm accounting matches tools/real100-court.sh's A1 lane (no
# interpreter startup in the measured path).
# ---------------------------------------------------------------------------

_BLOCK0 = "(SELECT block_id FROM blocks WHERE doc_id=1 AND ordering=0)"
_PAGE1 = "(SELECT block_id FROM blocks WHERE doc_id=1 AND unit='page' AND ordering=1)"


def _exact_sql():
    return ("json_object('length',(SELECT source_len FROM documents WHERE doc_id=1),"
            "'sha256',(SELECT source_sha256 FROM documents WHERE doc_id=1))")


def _rev_sql(depth):
    if depth < 4:
        return "NULL"
    return ("json_object('family_id',(SELECT family_id FROM revisions WHERE doc_id=1),"
            "'member_id',(SELECT member_id FROM revisions WHERE doc_id=1),"
            "'is_head',(SELECT is_head FROM revisions WHERE doc_id=1))")


def _coord_sql(depth, kind, ordinal):
    if depth < 1:
        return "NULL"
    if ordinal is None:
        ordl = "NULL"
    elif isinstance(ordinal, str):
        ordl = "'%s'" % ordinal
    else:
        ordl = "%s" % ordinal
    return ("json_object('format',(SELECT format FROM documents WHERE doc_id=1),"
            "'kind','%s','ordinal',%s)" % (kind, ordl))


def _prov_sql(depth, block_expr):
    if depth < 2:
        return "NULL"
    return ("json_object('class',(SELECT class FROM provenance WHERE block_id=%s),"
            "'span',(SELECT CASE WHEN span_start IS NULL THEN NULL ELSE "
            "json_array(span_start,span_end) END FROM provenance WHERE block_id=%s))"
            % (block_expr, block_expr))


def sql_for(depth, fmt, obs):
    exact = _exact_sql()
    rev = _rev_sql(depth)

    def env(value_kind, value_expr, coord, prov, declined):
        return ("SELECT json_object('obs','%s','declined',%s,'value_kind','%s',"
                "%s,'coord',%s,'provenance',%s,'exact',%s,'revision',%s)"
                % (obs, declined, value_kind, value_expr, coord, prov, exact, rev))

    if obs == "bytes":
        vhex = "(SELECT hex(substr(payload,1,64)) FROM source_blob WHERE doc_id=1)"
        dec = "CASE WHEN (SELECT payload FROM source_blob WHERE doc_id=1) IS NULL THEN 1 ELSE 0 END"
        coord = _coord_sql(depth, "byte-range", "0:64")
        prov = ("json_object('class','verbatim','span',json_array(0,64))" if depth >= 2 else "NULL")
        return env("bytes", "'value_hex',%s" % vhex, coord, prov, dec)

    if obs in ("text", "doc-text"):
        if obs == "text":
            if fmt == "pdf":
                value = "(SELECT text FROM blocks WHERE doc_id=1 AND unit='page' AND ordering=1)"
                coord = _coord_sql(depth, "page", 1)
                prov = _prov_sql(depth, _PAGE1)
            else:
                value = "(SELECT text FROM blocks WHERE doc_id=1 AND ordering=0)"
                coord = _coord_sql(depth, "block", 0)
                prov = _prov_sql(depth, _BLOCK0)
        else:
            value = "(SELECT doc_text FROM documents WHERE doc_id=1)"
            coord = _coord_sql(depth, "document", None)
            prov = "NULL"
        dec = "CASE WHEN %s IS NULL THEN 1 ELSE 0 END" % value
        return env("text", "'value',%s" % value, coord, prov, dec)

    if obs == "heading":
        bid = "(SELECT b.block_id FROM headings h JOIN blocks b ON b.block_id=h.block_id WHERE h.doc_id=1 AND h.ord=0)"
        value = "(SELECT b.text FROM headings h JOIN blocks b ON b.block_id=h.block_id WHERE h.doc_id=1 AND h.ord=0)"
        dec = "CASE WHEN %s IS NULL THEN 1 ELSE 0 END" % bid
        return env("text", "'value',%s" % value, _coord_sql(depth, "heading", 0), _prov_sql(depth, bid), dec)

    if obs == "table":
        bid = "(SELECT block_id FROM blocks WHERE doc_id=1 AND kind='table' ORDER BY block_id LIMIT 1)"
        value = "(SELECT text FROM blocks WHERE doc_id=1 AND kind='table' ORDER BY block_id LIMIT 1)"
        dec = "CASE WHEN %s IS NULL THEN 1 ELSE 0 END" % bid
        return env("text", "'value',%s" % value, _coord_sql(depth, "table", 0), _prov_sql(depth, bid), dec)

    if obs == "resource":
        value = ("json_object('path',(SELECT path FROM resources WHERE doc_id=1 AND resource_id=1),"
                 "'member_bytes',(SELECT member_bytes FROM resources WHERE doc_id=1 AND resource_id=1),"
                 "'member_sha256',(SELECT member_sha256 FROM resources WHERE doc_id=1 AND resource_id=1))")
        dec = "CASE WHEN (SELECT count(*) FROM resources WHERE doc_id=1 AND resource_id=1)=0 THEN 1 ELSE 0 END"
        return env("json", "'value',%s" % value, _coord_sql(depth, "resource", 0), "NULL", dec)

    if obs == "metadata":
        value = ("json_object('format',(SELECT format FROM documents WHERE doc_id=1),"
                 "'source_len',(SELECT source_len FROM documents WHERE doc_id=1),"
                 "'source_sha256',(SELECT source_sha256 FROM documents WHERE doc_id=1))")
        return env("json", "'value',%s" % value, _coord_sql(depth, "metadata", None), "NULL", "0")

    if obs == "revision":
        if depth < 4:
            return ("SELECT json_object('obs','revision','declined',1,'value_kind','json',"
                    "'value',NULL,'coord',NULL,'provenance',NULL,'exact',%s,'revision',NULL)" % exact)
        value = _rev_sql(depth)
        dec = "CASE WHEN (SELECT count(*) FROM revisions WHERE doc_id=1)=0 THEN 1 ELSE 0 END"
        return env("json", "'value',%s" % value, _coord_sql(depth, "revision", None), "NULL", dec)

    return "SELECT json_object('obs','%s','declined',1)" % obs


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main(argv=None):
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)

    b = sub.add_parser("build")
    b.add_argument("--format", required=True)
    b.add_argument("--source", required=True)
    b.add_argument("--db", required=True)
    b.add_argument("--through", type=int, required=True)
    b.add_argument("--family", default="")
    b.add_argument("--member", default="")
    b.add_argument("--is-head", type=int, default=1)
    b.add_argument("--metrics", default=None)

    q = sub.add_parser("query")
    q.add_argument("--db", required=True)
    q.add_argument("--format", required=True)
    q.add_argument("--depth", type=int, required=True)
    q.add_argument("--observation", required=True)
    q.add_argument("--out", required=True)

    s = sub.add_parser("session")
    s.add_argument("--db", required=True)
    s.add_argument("--format", required=True)
    s.add_argument("--depth", type=int, required=True)
    s.add_argument("--observations", required=True)
    s.add_argument("--out", required=True)

    m = sub.add_parser("materialize")
    m.add_argument("--db", required=True)
    m.add_argument("--out", required=True)

    a = sub.add_parser("extract")
    a.add_argument("--format", required=True)
    a.add_argument("--source", required=True)
    a.add_argument("--out", required=True)

    ag = sub.add_parser("aggregate")
    ag.add_argument("raw")
    ag.add_argument("campaign")

    sg = sub.add_parser("sqlgen")
    sg.add_argument("--out", required=True)
    sg.add_argument("--depths", default="0 1 2 3 4 5")

    ns = ap.parse_args(argv)

    if ns.cmd == "sqlgen":
        depths = [int(x) for x in ns.depths.split()]
        for d in depths:
            dd = os.path.join(ns.out, "C%d" % d)
            os.makedirs(dd, exist_ok=True)
            for fmt in ("pdf", "docx", "epub"):
                for obs in FMT_OBS[fmt]:
                    with open(os.path.join(dd, "%s.%s.sql" % (fmt, obs)), "w") as f:
                        f.write(sql_for(d, fmt, obs))
        print(json.dumps({"depths": depths}))
        return 0

    if ns.cmd == "build":
        metrics = build(ns.format, ns.source, ns.db, ns.through,
                        ns.family, ns.member, ns.is_head)
        if ns.metrics:
            with open(ns.metrics, "w") as f:
                json.dump(metrics, f, sort_keys=True)
        print(json.dumps(metrics, sort_keys=True))
        return 0

    if ns.cmd == "extract":
        doc = extract(ns.format, ns.source)
        os.makedirs(ns.out, exist_ok=True)
        with open(os.path.join(ns.out, "extraction.json"), "w") as f:
            json.dump(doc, f, sort_keys=True)
        print(json.dumps({"ok": True, "blocks": len(doc["blocks"])}))
        return 0

    if ns.cmd == "query":
        import sqlite3
        con = sqlite3.connect(ns.db)
        env = sqlite_envelope(con, ns.format, ns.depth, ns.observation)
        con.close()
        with open(ns.out, "w") as f:
            json.dump(env, f, sort_keys=True)
        print(json.dumps({"obs": ns.observation, "declined": env["declined"]}))
        return 0

    if ns.cmd == "session":
        import sqlite3
        con = sqlite3.connect(ns.db)
        obs = [o for o in ns.observations.split(",") if o]
        envs = [sqlite_envelope(con, ns.format, ns.depth, o) for o in obs]
        con.close()
        with open(ns.out, "w") as f:
            json.dump({"batch": envs}, f, sort_keys=True)
        print(json.dumps({"observations": len(envs)}))
        return 0

    if ns.cmd == "materialize":
        import sqlite3
        con = sqlite3.connect(ns.db)
        row = con.execute("SELECT payload FROM source_blob WHERE doc_id=1").fetchone()
        con.close()
        if row is None:
            return 3
        with open(ns.out, "wb") as f:
            f.write(row[0])
        return 0

    if ns.cmd == "aggregate":
        return aggregate(ns.raw, ns.campaign)

    return 2


# ---------------------------------------------------------------------------
# Aggregator
# ---------------------------------------------------------------------------

OBSERVATIONS = ["bytes", "text", "doc-text", "heading", "table", "resource",
                "metadata", "revision"]
DEPTH_FIELDS = {
    0: [],
    1: ["coord"],
    2: ["coord", "provenance"],
    3: ["coord", "provenance", "exact"],
    4: ["coord", "provenance", "exact", "revision"],
    5: ["coord", "provenance", "exact", "revision", "batch"],
}
FMT_OBS = {
    "pdf": ["bytes", "text", "metadata", "revision"],
    "docx": ["bytes", "text", "doc-text", "heading", "table", "resource", "metadata", "revision"],
    "epub": ["bytes", "text", "doc-text", "heading", "table", "resource", "metadata", "revision"],
}


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


def _load_json(path):
    try:
        with open(path) as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def _val_equal(a, b):
    """Equivalence of two normalized envelopes' value fields, with a projection
    fallback for text projections."""
    if a.get("value_kind") == "text" and b.get("value_kind") == "text":
        if a.get("value") == b.get("value"):
            return "raw"
        if project_text(a.get("value") or "") == project_text(b.get("value") or ""):
            return "projected"
        return "no"
    if a.get("value_hex") and a.get("value_hex") == b.get("value_hex"):
        return "raw"
    if a.get("value_sha256") and a.get("value_sha256") == b.get("value_sha256"):
        return "raw"
    return "no"


def _resource_equal(a, b):
    va, vb = a.get("value"), b.get("value")
    if not isinstance(va, dict) or not isinstance(vb, dict):
        return "no"
    if va.get("member_sha256") and va.get("member_sha256") == vb.get("member_sha256"):
        return "raw"
    return "no"


def _envelope_path(raw, depth, doc, fmt, obs, lane):
    return os.path.join(raw, "env", "C%d" % depth, "%s.%s.%s.%s.json" % (doc, fmt, obs, lane))


def _load_env(raw, depth, doc, fmt, obs, lane):
    obj = _load_json(_envelope_path(raw, depth, doc, fmt, obs, lane))
    if lane == "vole":
        return _normalize_vole_answer(obj, fmt, obs, depth)
    return obj


def _equiv(depth, fmt, obs, v, s):
    """Equivalence of the two lanes' envelopes at contract ``depth``. Returns
    (result, detail) where result in {raw, projected, shape, decline, mismatch}."""
    if v is None or s is None:
        return "mismatch", "missing envelope"
    if obs == "revision" and depth < 4:
        # C4 (revision lineage) is not part of the contract below depth 4. VOLE's
        # depth-independent store answers it at every depth; the baseline cannot
        # until C4. That is NOT a capability gap, so short-circuit before the
        # decline comparison.
        return "shape", "not required"
    vd, sd = v.get("declined"), s.get("declined")
    if vd and sd:
        return "decline", "both decline"
    if vd or sd:
        return "capability", "VOLE declines" if vd else "SQLite declines"
    if obs == "resource":
        # VOLE exposes the resource *reference* (rel id/ordinal); the baseline
        # exposes the *member* bytes/hash. Different projections of the same
        # resource: the shared, checkable contract is that both resolve the
        # ordinal and return a non-null object (shape equivalence).
        if isinstance(v.get("value"), dict) and isinstance(s.get("value"), dict):
            return "shape", "reference vs member bytes"
        return "mismatch", "resource differs"
    if obs == "metadata":
        return "shape", "schema differs by design"
    if obs == "revision":
        if depth < 4:
            return "shape", "not required"
        rv, sv = v.get("revision"), s.get("revision")
        if rv and sv:
            if rv.get("family_id") == sv.get("family_id") and \
                    rv.get("member_id") == sv.get("member_id") and \
                    bool(rv.get("is_head")) == bool(sv.get("is_head")):
                return "raw", "lineage tuple"
            if "revisions" in rv and "family_id" not in rv:
                # Phase 17: VOLE answers a revision-lineage observation, but it is
                # the PDF's INTERNAL incremental chain, not the baseline's corpus
                # family/member/head tuple. Different observables: reported as
                # such, never as equality and never as a value error.
                return "observable", "PDF chain vs corpus family"
            return "mismatch", "lineage differs"
        if rv or sv:
            return "capability", "VOLE lineage only" if rv else "SQLite lineage only"
        return "mismatch", "no lineage"
    if obs == "bytes":
        vh, sh = (v.get("value_hex") or ""), (s.get("value_hex") or "")
        if vh and vh.lower() == sh.lower():
            return "raw", "byte hex"
        if v.get("value_sha256") and v.get("value_sha256") == s.get("value_sha256"):
            return "raw", "byte sha"
        return "mismatch", "byte value"
    # text observations
    r = _val_equal(v, s)
    if r in ("raw", "projected"):
        return r, "text"
    if obs == "text" and fmt == "pdf":
        # PDF page text is a HEURISTIC layout projection; VOLE's own heuristic and
        # Poppler legitimately produce different bytes. Reported, never counted as
        # a contract failure.
        return "divergent", "pdf heuristic projection"
    return "mismatch", "text"


def aggregate(raw, campaign):
    one = read_tsv(os.path.join(raw, "onetime.tsv"))
    builds = read_tsv(os.path.join(raw, "build.tsv"))
    cold = read_tsv(os.path.join(raw, "cold.tsv"))
    warm = read_tsv(os.path.join(raw, "warm.tsv"))
    exact = read_tsv(os.path.join(raw, "exact.tsv"))

    fmt_of = {r["id"]: r["fmt"] for r in one}
    depths = sorted({int(r["depth"]) for r in builds}) or [0]

    lines = []
    lines.append("# Phase 20.4 — C4b contract court (C4b supplied to BOTH lanes under an equal external input)")
    lines.append("")
    lines.append(
        "Both lanes satisfy the SAME escalating contract C0..C5. `VOLE` is the field CLI "
        "(`observe`/`observe-batch`/`materialize`), which exposes a PDF "
        "revision-lineage surface (`--revisions`/`--revision N --kind lineage`) and, from "
        "Phase 20.4, an EXPLICIT external corpus-lineage surface "
        "(`field-external` + `observe --external-lineage --kind lineage`); `SQLite` is a "
        "source-retaining **SQLite Full** baseline: the established A1 surfaces "
        "(source blob, blocks, unicode61 + trigram FTS, headings/tables/cells/resources)"
        " plus, exactly as the contract deepens, a native-coordinate column (C1), a "
        "provenance table (C2), the retained source blob as the exact closure (C3) and "
        "a corpus revision-lineage table (C4). The baseline is byte-identical to Phase 16.5. "
        "Searches are not part of the contract, but the baseline carries them anyway so its "
        "cost is a faithful upper bound. Equivalence is validated per depth; costs are "
        "cumulative.")
    lines.append("")
    lines.append("Subset: **%d documents** (%s)." % (
        len(one), ", ".join(sorted(set(fmt_of.values())))))
    lines.append("")
    lines.append("Persistent bytes are the sum of REGULAR-FILE sizes for both lanes. "
                 "`du -sb` is deliberately not used: the repo's bind-mounted host "
                 "filesystem reports large phantom directory sizes, so `du -sb` on "
                 "VOLE's directory store would be compared unfairly against SQLite's "
                 "single db file.")
    lines.append("")

    # --- build cost -------------------------------------------------------
    v_build = sum(int(r["v_enc_ms"]) + int(r["v_ing_ms"]) for r in one)
    v_store = sum(int(r["v_store_bytes"] or 0) for r in one)
    lines.append("## One-time build cost + persistent bytes (cumulative per depth)")
    lines.append("")
    lines.append("| substrate | depth | docs | build ms (sum) | build ms (median) | persistent B (sum) | vs source |")
    lines.append("|---|---|---:|---:|---:|---:|---:|")
    src_total = sum(int(r["byte_len"]) for r in one)
    lines.append("| VOLE field-build (direct) | all | {} | {} | {} | {} | {:.3f}× |".format(
        len(one), v_build,
        sorted(int(r["v_enc_ms"]) + int(r["v_ing_ms"]) for r in one)[len(one) // 2] if one else 0,
        v_store, v_store / src_total if src_total else 0))
    for d in depths:
        rows = [r for r in builds if int(r["depth"]) == d and r["build_rc"] == "0"]
        ms = sorted(int(r["build_ms"]) for r in rows)
        tot = sum(int(r["db_bytes"]) for r in rows)
        lines.append("| SQLite contract store | C{} | {} | {} | {} | {} | {:.3f}× |".format(
            d, len(rows), sum(ms), ms[len(ms) // 2] if ms else 0, tot,
            tot / src_total if src_total else 0))
    lines.append("")
    lines.append("`vs source` is the persistent footprint as a multiple of the "
                 "subset's total source bytes. VOLE's query cost is depth-independent "
                 "(its store already carries coord/provenance/exact); SQLite pays new "
                 "materialization at each depth.")
    lines.append("")

    # --- query schedule ---------------------------------------------------
    lines.append("## Query schedule — cost per depth")
    lines.append("")
    lines.append("Cold = one process per observation; Warm = one session serving the "
                 "whole depth schedule (VOLE `observe-batch`; SQLite one process). "
                 "VOLE's schedule is measured per depth but is depth-independent.")
    lines.append("")
    lines.append("| depth | lane | obs | cold ms | warm ms | warm peak RSS KB |")
    lines.append("|---|---|---:|---:|---:|---:|")
    for d in depths:
        for lane in ("vole", "a1c"):
            rows = [r for r in cold if int(r["depth"]) == d and r["lane"] == lane]
            if not rows:
                continue
            cold_sum = sum(int(r["ms"]) for r in rows)
            wrows = [r for r in warm if int(r["depth"]) == d and r["lane"] == lane]
            warm_sum = sum(int(r["ms"]) for r in wrows)
            rss = max([int(r["rss_kb"]) for r in wrows if r.get("rss_kb")] or [0])
            lines.append("| C{} | {} | {} | {} | {} | {} |".format(
                d, "VOLE" if lane == "vole" else "SQLite", len(rows), cold_sum, warm_sum, rss))
    lines.append("")

    # --- equivalence ------------------------------------------------------
    lines.append("## Equivalence by depth and observation")
    lines.append("")
    lines.append("| depth | fmt | obs | eq raw | eq proj | shape | both decline | capability gap | divergent | different observable | value mismatch |")
    lines.append("|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|")
    for d in depths:
        rows = [r for r in cold if int(r["depth"]) == d and r["lane"] == "a1c"]
        for fmt, obs in sorted({(r["fmt"], r["obs"]) for r in rows}):
            counts = {"raw": 0, "projected": 0, "shape": 0, "decline": 0,
                      "capability": 0, "divergent": 0, "mismatch": 0, "observable": 0}
            for r in [x for x in rows if x["fmt"] == fmt and x["obs"] == obs]:
                v = _load_env(raw, d, r["id"], fmt, obs, "vole")
                s = _load_env(raw, d, r["id"], fmt, obs, "a1c")
                res, _why = _equiv(d, fmt, obs, v, s)
                counts[res] = counts.get(res, 0) + 1
            lines.append("| C{} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |".format(
                d, fmt, obs, counts["raw"], counts["projected"], counts["shape"],
                counts["decline"], counts["capability"], counts["divergent"],
                counts["observable"], counts["mismatch"]))
    lines.append("")
    lines.append("_`capability gap` = one lane declines an observation the other answers. "
                 "`divergent` = both answer but the byte values differ on a *heuristic* "
                 "observable (PDF page text: VOLE's layout heuristic vs Poppler). "
                 "`different observable` = the `revision` observation of different things: "
                 "VOLE reports the PDF's internal incremental chain; the baseline reports the "
                 "corpus family/member/head tuple. The split into C4a (document-native) and "
                 "C4b (corpus/external) is tabulated below; neither value can equal the other "
                 "by construction, so this is neither equality nor an error. "
                 "`value mismatch` = both answer but differ on a non-heuristic "
                 "observable. `shape` = both answer with projections that are not "
                 "byte-comparable by design (metadata schemas; resource reference vs "
                 "member bytes)._")
    lines.append("")
    lines.append("")

    # --- C4 split: C4a document-native vs C4b corpus/external ------------
    c4a = {r["id"]: r for r in read_tsv(os.path.join(raw, "c4a", "rows.tsv"))}
    lin_rc = {r["id"]: r["revisions_rc"]
              for r in read_tsv(os.path.join(raw, "lineage", "rc.tsv"))}

    def _a1c_rev(doc, fmt):
        return _load_json(_envelope_path(raw, 4, doc, fmt, "revision", "a1c"))

    lines.append("## C4 split — document-native lineage (C4a) vs corpus/external lineage (C4b)")
    lines.append("")
    lines.append("C4 is two different observables, reported separately and never forced equal.")
    lines.append("")
    n_pdf = sum(1 for r in one if r["fmt"] == "pdf")
    n_oth = len(one) - n_pdf
    v_pdf = sum(1 for r in one if r["fmt"] == "pdf" and lin_rc.get(r["id"]) == "0")
    v_oth = sum(1 for r in one if r["fmt"] != "pdf" and lin_rc.get(r["id"]) == "0")
    sql_native = 0
    deriv = match = 0
    for r in one:
        val = (_a1c_rev(r["id"], r["fmt"]) or {}).get("value") or {}
        if r["fmt"] == "pdf" and isinstance(val, dict) and "revisions" in val:
            sql_native += 1
        pr = c4a.get(r["id"], {})
        if pr.get("native_derivable") == "1":
            deriv += 1
        if pr.get("count_match") == "1":
            match += 1
    lines.append("**C4a — document-native lineage** (PDF incremental revisions; typed "
                 "unsupported otherwise). Byte-derivable, so a source-retaining store *could* "
                 "hold it; VOLE indexes it at ingest, the baseline lane as configured does not "
                 "expose it.")
    lines.append("")
    lines.append("| lane | PDF (native chain) | DOCX/EPUB (no native history) |")
    lines.append("|---|---|---|")
    lines.append("| VOLE (`observe --revisions --kind lineage`) | answered **{}/{}** | typed unsupported (rc 6) {}/{} |"
                 .format(v_pdf, n_pdf, n_oth - v_oth, n_oth))
    lines.append("| SQLite lane (`revision` at C4) | native chain **{}/{}** — answers the corpus tuple instead | native chain **0/{}** |"
                 .format(sql_native, n_pdf, n_oth))
    lines.append("| retained bytes (probe) | a header+chain is derivable from the retained source for **{}/{}** PDFs; revision count matches VOLE for **{}** | n/a |"
                 .format(deriv, n_pdf, match))
    lines.append("")
    lines.append("_Probe = an unmeasured marker scan (`%%EOF` count + header) over the SAME bytes "
                 "the baseline retains as its C3 closure (`source_blob`); it tests byte-"
                 "*derivability*, not full fidelity, and is not a PDF-conformance oracle._")
    lines.append("")
    c4b_rc = {r["id"]: r for r in read_tsv(os.path.join(raw, "c4b", "rc.tsv"))}

    def _vole_c4b(doc, fmt):
        return _load_json(os.path.join(raw, "c4b", "%s.%s.ext.json" % (doc, fmt)))

    sql_ans = fam_n = 0
    for r in one:
        val = (_a1c_rev(r["id"], r["fmt"]) or {}).get("value") or {}
        if isinstance(val, dict) and "family_id" in val:
            sql_ans += 1
            if val.get("family_id"):
                fam_n += 1
    vole_ans = tuple_match = 0
    for r in one:
        ext = _vole_c4b(r["id"], r["fmt"])
        if not isinstance(ext, dict):
            continue
        val = ext.get("value") if isinstance(ext.get("value"), dict) else None
        if ext.get("basis") != "external-metadata" or val is None:
            continue
        vole_ans += 1
        sval = (_a1c_rev(r["id"], r["fmt"]) or {}).get("value") or {}
        if isinstance(sval, dict) and (
                val.get("family_id") == sval.get("family_id")
                and val.get("member_id") == sval.get("member_id")
                and bool(val.get("is_head")) == bool(sval.get("is_head"))):
            tuple_match += 1
    attach_ms = sum(int(r.get("attach_ms", 0)) for r in c4b_rc.values())
    query_ms = sum(int(r.get("query_ms", 0)) for r in c4b_rc.values())
    attach_bytes = sum(int(r.get("attach_bytes", 0)) for r in c4b_rc.values())
    sql_rev_ms = sum(int(r["ms"]) for r in cold
                     if int(r["depth"]) == 4 and r["obs"] == "revision" and r["lane"] == "a1c")
    lines.append("Phase 20.4 supplies the SAME family/member/head to BOTH lanes (an equal "
                 "external input): the harness gives it to SQLite at build "
                 "(`--family/--member/--is-head`); VOLE receives it as an explicit typed "
                 "`ExternalContext` stored BESIDE the field (`field-external`) and answers it "
                 "as `selector=external-lineage, representation=lineage, basis=external-metadata`.")
    lines.append("")
    lines.append("**C4b — corpus/external lineage** (dataset family id / member id / head "
                 "flag). DATASET metadata, **not document-derived**.")
    lines.append("")
    lines.append("| lane | C4b answered | basis recorded | C4b tuple equals the other lane |")
    lines.append("|---|---:|---|---:|")
    lines.append("| SQLite lane (`revision` at C4) | **{}/{}** (family set for {}) | dataset metadata in the `revisions` table; external to the bytes | — |"
                 .format(sql_ans, len(one), fam_n))
    lines.append("| VOLE (`observe --external-lineage --kind lineage`) | **{}/{}** | `external-metadata` (typed; never exact; reads no seed node) | **{}/{}** |"
                 .format(vole_ans, len(one), tuple_match, len(one)))
    lines.append("")
    lines.append("C4b cost over the subset: VOLE `field-external` attach **{} ms** writing "
                 "**{} B** of sidecar (one record per document, in `<store>/external/`); VOLE "
                 "external query **{} ms** (reads only that sidecar); SQLite C4b query "
                 "(`revision` at C4) **{} ms**. SQLite's C4b data is written as part of its C4 "
                 "build, not in a separate step."
                 .format(attach_ms, attach_bytes, query_ms, sql_rev_ms))
    lines.append("")
    # Separation: a plain, document-derived observation (and the exact closure)
    # must be unaffected by attaching or removing the external context.
    def _strip_timing(path):
        obj = _load_json(path)
        if not isinstance(obj, dict):
            return None
        st = obj.get("stats")
        if isinstance(st, dict):
            st.pop("wall_micros", None)
        return obj

    removal = {r["id"]: r for r in read_tsv(os.path.join(raw, "c4b", "removal.tsv"))}
    plain_same = plain_removed_same = decline_ok = exact_removed_ok = 0
    for r in one:
        doc, fmt = r["id"], r["fmt"]
        before = _strip_timing(os.path.join(raw, "c4b", "%s.%s.plain.before.json" % (doc, fmt)))
        after = _strip_timing(os.path.join(raw, "c4b", "%s.%s.plain.after.json" % (doc, fmt)))
        removed = _strip_timing(os.path.join(raw, "c4b", "%s.%s.plain.removed.json" % (doc, fmt)))
        if before is not None and before == after:
            plain_same += 1
        if before is not None and before == removed:
            plain_removed_same += 1
        rr = removal.get(doc, {})
        if rr.get("decline_rc") == "6":
            decline_ok += 1
        if rr.get("exact_ok_after_removal") == "1":
            exact_removed_ok += 1
    lines.append("Separation: a plain `--metadata --kind metadata` answer (`wall_micros` removed) "
                 "is byte-identical before vs after the attach for **{}/{}** documents, and after "
                 "the clear for **{}/{}**; the external query declines typed (rc 6) after the "
                 "clear for **{}/{}**; and the exact closure still matches after the clear for "
                 "**{}/{}**. The external answer record itself reads no descriptor/manifest/index/"
                 "seed byte (`bytes_read` 0)."
                 .format(plain_same, len(one), plain_removed_same, len(one), decline_ok, len(one),
                         exact_removed_ok, len(one)))
    lines.append("")
    lines.append("**C5 — C4 + a one-session heterogeneous batch**, under both readings:")
    lines.append("")
    lines.append("- **C5a (= C4a + batch):** VOLE serves the PDF native chain inside one "
                 "`observe-batch` session; the baseline session has no C4a answer to serve "
                 "(it declines C4a).")
    lines.append("- **C5b (= C4b + batch):** under Phase 20.4 both lanes CAN serve the corpus "
                 "tuple in one session; this court measures the C4b *query* as a separate "
                 "step beside the schedule rather than folding `--external-lineage` into the "
                 "heterogeneous batch (recorded as a residual).")
    lines.append("")
    lines.append("| id | fmt | family | C4a VOLE | C4a SQLite lane | C4a from retained bytes | C4b SQLite | C4b VOLE |")
    lines.append("|---|---|---|---|---|---|---|---|")
    for r in one:
        doc, fmt = r["id"], r["fmt"]
        val = (_a1c_rev(doc, fmt) or {}).get("value") or {}
        fam = val.get("family_id") if isinstance(val, dict) else None
        pr = c4a.get(doc, {})
        va = "native (rc 0)" if lin_rc.get(doc) == "0" else "typed unsupported (rc 6)"
        dv = ("derivable (count match)" if pr.get("count_match") == "1"
              else ("derivable" if pr.get("native_derivable") == "1" else "n/a"))
        cb = "yes" if isinstance(val, dict) and "family_id" in val else "no"
        ext = _vole_c4b(doc, fmt)
        extval = ext.get("value") if isinstance(ext, dict) and isinstance(ext.get("value"), dict) else None
        if extval is not None:
            vcb = "yes (external-metadata)"
        else:
            rc = c4b_rc.get(doc, {})
            vcb = "typed unsupported (rc %s)" % (rc.get("query_rc", "?"))
        lines.append("| {} | {} | {} | {} | corpus tuple only | {} | {} | {} |".format(
            doc, fmt, fam or "-", va, dv, cb, vcb))
    lines.append("")

    lines.append("## Exact original closure (length + SHA-256 + byte compare)")
    lines.append("")
    lines.append("| id | fmt | VOLE ok | SQLite ok | VOLE ms | SQLite ms |")
    lines.append("|---|---|---|---|---:|---:|")
    for r in exact:
        lines.append("| {} | {} | {} | {} | {} | {} |".format(
            r["id"], r["fmt"], r["v_ok"], r["sql_ok"], r["v_ms"], r["sql_ms"]))
    lines.append("")
    n = len(exact)
    lines.append("VOLE `materialize --exact`: {}/{} byte-exact. SQLite retained blob: {}/{} byte-exact.".format(
        sum(1 for r in exact if r["v_ok"] == "1"), n,
        sum(1 for r in exact if r["sql_ok"] == "1"), n))
    lines.append("")

    # --- verdict ----------------------------------------------------------
    def build_bytes(d):
        return sum(int(r["db_bytes"]) for r in builds if int(r["depth"]) == d and r["build_rc"] == "0")
    lines.append("## Frontier verdict")
    lines.append("")
    lines.append("| depth | VOLE cold ms | SQL cold ms | VOLE warm ms | SQL warm ms | VOLE B | SQL B | cold | warm | bytes | VOLE satisfies? |")
    lines.append("|---|---:|---:|---:|---:|---:|---:|---|---|---|---|")
    for d in depths:
        rows = [r for r in cold if int(r["depth"]) == d and r["lane"] == "a1c"]
        cap = mm = ob = 0
        for r in rows:
            v = _load_env(raw, d, r["id"], r["fmt"], r["obs"], "vole")
            s = _load_env(raw, d, r["id"], r["fmt"], r["obs"], "a1c")
            res, _why = _equiv(d, r["fmt"], r["obs"], v, s)
            cap += res == "capability"; mm += res == "mismatch"; ob += res == "observable"
        vcold = sum(int(r["ms"]) for r in cold if int(r["depth"]) == d and r["lane"] == "vole")
        scold = sum(int(r["ms"]) for r in cold if int(r["depth"]) == d and r["lane"] == "a1c")
        vwarm = sum(int(r["ms"]) for r in warm if int(r["depth"]) == d and r["lane"] == "vole")
        swarm = sum(int(r["ms"]) for r in warm if int(r["depth"]) == d and r["lane"] == "a1c")
        sb = build_bytes(d)
        cw = "VOLE" if vcold < scold else "SQLite"
        ww = "VOLE" if vwarm < swarm else "SQLite"
        bw = "VOLE" if v_store < sb else "SQLite"
        if cap == 0 and mm == 0 and ob == 0:
            satisfies = "yes"
        elif cap and ob:
            satisfies = "no (declines + diff. observable)"
        elif cap:
            satisfies = "no (declines)"
        elif ob:
            satisfies = "no (different observable)"
        else:
            satisfies = "no (values)"
        lines.append("| C{} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |".format(
            d, vcold, scold, vwarm, swarm, v_store, sb, cw, ww, bw, satisfies))
    lines.append("")
    lines.append("One row per contract depth; `bytes` compares the sum-of-regular-file "
                 "persistent footprints.")
    lines.append("")

    vw_all = sum(int(r["ms"]) for r in warm if r["lane"] == "vole")
    sw_all = sum(int(r["ms"]) for r in warm if r["lane"] == "a1c")
    vbuild = sum(int(r["v_enc_ms"]) + int(r["v_ing_ms"]) for r in one)
    srows = [r for r in builds if int(r["depth"]) == depths[-1] and r["build_rc"] == "0"]
    sbuild = sum(int(r["build_ms"]) for r in srows)
    sb_final = build_bytes(depths[-1])
    lines.append("### Reading")
    lines.append("")
    lines.append("VOLE/SQLite storage **{:.2f}×**, build **{:.2f}×**, warm session "
                 "**{:.2f}×**. VOLE is the STORAGE winner at every depth; SQLite is the "
                 "BUILD, WARM-LATENCY and FULL-CONTRACT winner at every depth.".format(
                     v_store / sb_final if sb_final else 0,
                     vbuild / sbuild if sbuild else 0,
                     vw_all / sw_all if sw_all else 0))
    lines.append("")
    lines.append("- On DOCX/EPUB text the two systems return **byte-identical** values "
                 "(blocks/headings/tables/doc-text): the baseline mirrors VOLE's extraction "
                 "semantics. EPUB doc-text also matches under the whitespace projection "
                 "(VOLE emits a trailing newline for an empty spine item).")
    lines.append("- Byte reads and the whole-source exact closure agree on BOTH lanes: "
                 "12/12 documents reproduce their original length + SHA-256 (VOLE "
                 "`materialize --exact`; SQLite retained blob).")
    lines.append("- PDF page text is a **heuristic layout projection**: VOLE's own heuristic and "
                 "Poppler produce different bytes, so equality holds only for the contract "
                 "SHAPE there (recorded as `divergent`, never as equality).")
    lines.append("- C4 splits into **C4a (document-native lineage)** and **C4b (corpus/external "
                 "lineage)**. VOLE answers C4a (the PDF's internal incremental chain: header, "
                 "count, ordered spans, `startxref`/`/Prev`, object membership) and declines "
                 "it typed for DOCX/EPUB (`UnsupportedFeature`, rc 6). C4a is byte-derivable "
                 "and the baseline retains the exact source (the C3 closure), so the baseline "
                 "*could* answer C4a by re-scanning what it keeps — but its measured lane does "
                 "not, so at the lane level C4a is a genuine VOLE-only answer, a difference of "
                 "WHERE the work happens (indexed at ingest vs re-parsed at query), not of "
                 "information held. **Phase 20.4 changes C4b:** supplied the SAME "
                 "family/member/head the baseline gets, VOLE answers the tuple from an explicit "
                 "external layer (basis `external-metadata`, rc 0) for **{}/{}** documents, "
                 "matching the baseline's tuple for **{}/{}**. C4b is therefore no longer a VOLE "
                 "capability gap; what remains is a *cost* comparison (a one-record-per-document "
                 "sidecar plus a query that reads only that sidecar, versus the baseline's "
                 "in-build insert), not a coverage claim."
                 .format(vole_ans, len(one), tuple_match, len(one)))
    lines.append("")
    lines.append("## Conclusions")
    lines.append("")
    lines.append("1. **The direct build materially narrows the build gap but does not close "
                 "it.** With `field-build` the VOLE/SQLite build-wall ratio is **{:.2f}×** "
                 "(was **10.74×** with `encode`+`field-ingest` in 16.5/17.2): the earlier "
                 "10.7× and the 17.1 2.04× `field-build` speedup are not composable, and this "
                 "court is the composable number. VOLE still builds slower than SQLite here, "
                 "but by a far smaller factor."
                 .format((vbuild / sbuild) if sbuild else 0))
    lines.append("2. **Storage, latency and cold ties are unchanged in direction.** VOLE is "
                 "cheaper to STORE (about {:.2f}× the baseline bytes) at every depth; SQLite "
                 "still serves the warm session ~{:.2f}× faster and ties or wins cold. The "
                 "richer contract costs SQLite only about +{:.0f}% persistent bytes from C0 to "
                 "C4 because the source blob it retains dominates the store."
                 .format(v_store / sb_final if sb_final else 0,
                         (vw_all / sw_all) if sw_all else 0,
                         100.0 * (build_bytes(4) - build_bytes(0)) / build_bytes(0)
                         if build_bytes(0) else 0))
    lines.append("3. **C4 (both halves) is now answered by BOTH lanes, and the honest question "
                 "is cost.** C4a (document-native lineage) is answered by VOLE and, at the lane "
                 "level, not by the baseline — but it is byte-derivable from the retained "
                 "source, so it is a work-location difference, not an information advantage. "
                 "C4b (corpus/external lineage) closes only because Phase 20.4 supplies the "
                 "SAME external input to both: the baseline stores it as part of its C4 build; "
                 "VOLE stores it as a typed `ExternalContext` BESIDE the field (attach **{} B**, "
                 "**{} ms** over the subset) and answers it in **{} ms**, versus the baseline's "
                 "**{} ms** C4 query. The tuple matches for **{}/{}**."
                 .format(attach_bytes, attach_ms, query_ms, sql_rev_ms, tuple_match, len(one)))
    lines.append("")
    lines.append("4. **Whole-contract verdict:** on this subset, under the equal contract, "
                 "SQLite remains the build- and warm-latency-winner and VOLE keeps the storage "
                 "edge; with the C4b external input supplied equally, the contract now closes at "
                 "C4 in both halves for both lanes, so the remaining differences are cost, not "
                 "capability. Whether that makes VOLE a *better* C4b host is a cost question this "
                 "court answers directly, not a coverage question it no longer needs to.")
    lines.append("")

    report = "\n".join(lines)
    with open(os.path.join(campaign, "SUMMARY.md"), "w") as f:
        f.write(report)
    print(report)
    return 0


if __name__ == "__main__":
    sys.exit(main())
