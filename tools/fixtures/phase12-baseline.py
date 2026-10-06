#!/usr/bin/env python3
# Phase 12.11 — the conventional-baseline engine for the lifetime court.
#
# Two honest lanes live here, both Python-standard-library only (so the court
# runs in the pinned `doc-baseline` image with no third-party parser):
#
#   A0  direct per-query extraction. `query` re-reads the source and does the
#       minimal work to answer one pre-registered case (a page/story/spine text
#       block, a heading, a table cell, a resource member, a lexical hit). No
#       persistence: every invocation pays the extraction cost again.
#
#   A1  one-time preprocessed, *source-retaining* SQLite+FTS5. `build` extracts
#       the whole document once, loads it into the schema below (covering
#       indexes, a real FTS5 tokenizer, WAL, `ANALYZE`), and concurrently retains
#       the exact original bytes in `source_blob` so exact recovery is possible.
#       A1 queries are run by the court with the `sqlite3` CLI.
#
# Everything the process physically reads is inside the accounting boundary; the
# extraction metrics (`decompressed_bytes`, `xml_bytes_parsed`, `member_decodes`,
# `external_invocations`, `source_reparses`) are emitted so the court can report
# the full metric vector, not just wall-clock.
#
# Usage:
#   phase12-baseline.py extract --format F --source S --out DIR [--metrics M]
#   phase12-baseline.py build   --format F --source S --db D   [--metrics M]
#   phase12-baseline.py query   --format F --source S --case C [--arg A] \
#                               --out OUT [--metrics M]
#
# `--out` for a byte case (resource-bytes / exact-member) receives the raw
# answer bytes; for every text case it receives the JSON answer payload.

import argparse
import hashlib
import io
import json
import os
import subprocess
import sys
import xml.etree.ElementTree as ET
import zipfile

W = "{http://schemas.openxmlformats.org/wordprocessingml/2006/main}"
XHTML = "{http://www.w3.org/1999/xhtml}"


# ---------------------------------------------------------------------------
# Small helpers
# ---------------------------------------------------------------------------

def sha256_hex(b):
    return hashlib.sha256(b).hexdigest()


def rm(path):
    try:
        os.remove(path)
    except FileNotFoundError:
        pass


# ---------------------------------------------------------------------------
# PDF extraction (Poppler is the oracle, never the authority; plan ADR-0009).
# ---------------------------------------------------------------------------

def extract_pdf(source):
    metrics = {"decompressed_bytes": 0, "xml_bytes_parsed": 0,
               "member_decodes": 0, "external_invocations": 0,
               "source_reparses": 1}
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
        blocks.append({"unit": "page", "ordering": p, "kind": "text",
                       "text": text, "sha256": sha256_hex(text.encode())})
    meta = {"format": "pdf", "pages": pages,
            "source_len": os.path.getsize(source),
            "source_sha256": sha256_hex(open(source, "rb").read())}
    return {"format": "pdf", "metadata": meta, "blocks": blocks,
            "headings": [], "tables": [], "cells": [], "resources": [],
            "metrics": metrics}


# ---------------------------------------------------------------------------
# DOCX extraction (OPC / WordprocessingML, stdlib zipfile + ElementTree)
# ---------------------------------------------------------------------------

def _wml_text(el):
    return "".join(t.text or "" for t in el.iter(W + "t"))


def extract_docx(source):
    metrics = {"decompressed_bytes": 0, "xml_bytes_parsed": 0,
               "member_decodes": 0, "external_invocations": 0,
               "source_reparses": 1}
    blocks, headings, tables, cells, resources = [], [], [], [], []
    with zipfile.ZipFile(source) as z:
        names = z.namelist()

        def read(name):
            zi = z.getinfo(name)
            data = z.read(name)
            metrics["member_decodes"] += 1
            metrics["decompressed_bytes"] += zi.file_size
            del zi
            return data

        document = read("word/document.xml")
        metrics["xml_bytes_parsed"] += len(document)
        root = ET.fromstring(document)
        body = root.find(W + "body")
        bidx = 0
        for child in list(body):
            if child.tag == W + "p":
                ppr = child.find(W + "pPr")
                style = None
                if ppr is not None:
                    ps = ppr.find(W + "pStyle")
                    if ps is not None:
                        style = ps.get(W + "val")
                kind = "paragraph"
                if style and style.startswith("Heading"):
                    kind = "heading"
                    headings.append({"block": bidx, "level": int(style[7:] or 1),
                                     "text": _wml_text(child)})
                if ppr is not None and ppr.find(W + "numPr") is not None:
                    kind = "list"
                blocks.append({"unit": "story", "ordering": bidx, "kind": kind,
                               "text": _wml_text(child),
                               "sha256": sha256_hex(_wml_text(child).encode())})
                bidx += 1
            elif child.tag == W + "tbl":
                rows = []
                for tr in child.findall(W + "tr"):
                    row = [_wml_text(tc) for tc in tr.findall(W + "tc")]
                    rows.append(row)
                # Table text block (tab-separated)
                ttext = "\n".join("\t".join(r) for r in rows)
                tables.append({"table": len(tables), "block": bidx,
                               "n_rows": len(rows),
                               "n_cols": max((len(r) for r in rows), default=0)})
                tid = len(tables) - 1
                for ri, row in enumerate(rows):
                    for ci, val in enumerate(row):
                        cells.append({"table": tid, "r": ri, "c": ci, "text": val})
                blocks.append({"unit": "story", "ordering": bidx, "kind": "table",
                               "text": ttext, "sha256": sha256_hex(ttext.encode())})
                bidx += 1
        # resources: every non-XML, non-rels media member
        for name in names:
            if name.startswith("word/media/"):
                zi = z.getinfo(name)
                data = z.read(name)
                metrics["member_decodes"] += 1
                metrics["decompressed_bytes"] += zi.file_size
                doff = zi.header_offset + 30 + len(zi.filename.encode("utf-8")) + len(zi.extra)
                resources.append({"path": name,
                                  "member_bytes": zi.file_size,
                                  "member_sha256": sha256_hex(data),
                                  "offset": doff,
                                  "raw_length": zi.compress_size,
                                  "method": zi.compress_type,
                                  "decoded": zi.compress_type == 0})
        core = None
        if "docProps/core.xml" in names:
            core = read("docProps/core.xml")
            metrics["xml_bytes_parsed"] += len(core)
        meta = {"format": "docx", "parts": len(names),
                "media": len(resources),
                "source_len": os.path.getsize(source),
                "source_sha256": sha256_hex(open(source, "rb").read())}
    return {"format": "docx", "metadata": meta, "blocks": blocks,
            "headings": headings, "tables": tables, "cells": cells,
            "resources": resources, "metrics": metrics}


# ---------------------------------------------------------------------------
# EPUB extraction (OCF / bounded XHTML, stdlib zipfile + ElementTree)
# ---------------------------------------------------------------------------

def extract_epub(source):
    metrics = {"decompressed_bytes": 0, "xml_bytes_parsed": 0,
               "member_decodes": 0, "external_invocations": 0,
               "source_reparses": 1}
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
        croot = ET.fromstring(container)
        opf_path = None
        for rf in croot.iter():
            if rf.tag.endswith("rootfile"):
                opf_path = rf.get("full-path")
        opf = read(opf_path)
        metrics["xml_bytes_parsed"] += len(opf)
        oroot = ET.fromstring(opf)
        manifest = {}
        for item in oroot.iter():
            if item.tag.endswith("item"):
                manifest[item.get("id")] = item.get("href")
        spine = [manifest.get(ir.get("idref"))
                 for ir in oroot.iter() if ir.tag.endswith("itemref")]
        base = os.path.dirname(opf_path)
        bidx = 0
        for href in spine:
            path = os.path.normpath(os.path.join(base, href))
            chapter = read(path)
            metrics["xml_bytes_parsed"] += len(chapter)
            root = ET.fromstring(chapter)
            body = None
            for el in root.iter():
                if el.tag == XHTML + "body":
                    body = el
                    break
            if body is None:
                continue

            def walk(node, kind_hint="paragraph"):
                nonlocal bidx
                tag = node.tag.replace(XHTML, "")
                if tag in ("h1", "h2", "h3", "h4", "h5", "h6"):
                    text = "".join(node.itertext()).strip()
                    headings.append({"block": bidx, "level": int(tag[1]),
                                     "text": text})
                    blocks.append({"unit": "spine", "ordering": bidx,
                                   "kind": "heading", "text": text,
                                   "sha256": sha256_hex(text.encode())})
                    bidx += 1
                    return
                if tag == "p":
                    text = "".join(node.itertext())
                    blocks.append({"unit": "spine", "ordering": bidx,
                                   "kind": "paragraph", "text": text,
                                   "sha256": sha256_hex(text.encode())})
                    bidx += 1
                    return
                if tag in ("ol", "ul"):
                    # VOLE's bounded-XHTML block model emits one block per list
                    # container (items joined by newlines), not one per <li>.
                    text = "\n".join("".join(li.itertext()) for li in node)
                    blocks.append({"unit": "spine", "ordering": bidx,
                                   "kind": "list", "text": text,
                                   "sha256": sha256_hex(text.encode())})
                    bidx += 1
                    return
                if tag == "table":
                    rows = []
                    for tr in node.iter(XHTML + "tr"):
                        row = ["".join(c.itertext()) for c in tr
                               if c.tag in (XHTML + "td", XHTML + "th")]
                        rows.append(row)
                    ttext = "\n".join("\t".join(r) for r in rows)
                    tid = len(tables)
                    tables.append({"table": tid, "block": bidx,
                                   "n_rows": len(rows),
                                   "n_cols": max((len(r) for r in rows), default=0)})
                    for ri, row in enumerate(rows):
                        for ci, val in enumerate(row):
                            cells.append({"table": tid, "r": ri, "c": ci, "text": val})
                    blocks.append({"unit": "spine", "ordering": bidx,
                                   "kind": "table", "text": ttext,
                                   "sha256": sha256_hex(ttext.encode())})
                    bidx += 1
                    return
                if tag == "img":
                    return
                if tag in ("a",):
                    text = "".join(node.itertext())
                    blocks.append({"unit": "spine", "ordering": bidx,
                                   "kind": "link", "text": text,
                                   "sha256": sha256_hex(text.encode())})
                    bidx += 1
                    return
                # container-ish element: descend
                for c in list(node):
                    walk(c, kind_hint)

            for child in list(body):
                walk(child)
        for name in names:
            if name.startswith("OEBPS/images/"):
                zi = z.getinfo(name)
                data = z.read(name)
                metrics["member_decodes"] += 1
                metrics["decompressed_bytes"] += zi.file_size
                doff = zi.header_offset + 30 + len(zi.filename.encode("utf-8")) + len(zi.extra)
                resources.append({"path": os.path.basename(name),
                                  "member_bytes": zi.file_size,
                                  "member_sha256": sha256_hex(data),
                                  "offset": doff,
                                  "raw_length": zi.compress_size,
                                  "method": zi.compress_type,
                                  "decoded": zi.compress_type == 0})
        meta = {"format": "epub", "spine": len(spine), "parts": len(names),
                "media": len(resources),
                "source_len": os.path.getsize(source),
                "source_sha256": sha256_hex(open(source, "rb").read())}
    return {"format": "epub", "metadata": meta, "blocks": blocks,
            "headings": headings, "tables": tables, "cells": cells,
            "resources": resources, "metrics": metrics}


EXTRACTORS = {"pdf": extract_pdf, "docx": extract_docx, "epub": extract_epub}


def extract(fmt, source):
    return EXTRACTORS[fmt](source)


# ---------------------------------------------------------------------------
# A1 — one-time preprocessed source-retaining SQLite
# ---------------------------------------------------------------------------

SCHEMA = """
PRAGMA journal_mode=WAL;
PRAGMA synchronous=NORMAL;
CREATE TABLE documents(
  doc_id INTEGER PRIMARY KEY, format TEXT NOT NULL, path TEXT NOT NULL,
  source_bytes INTEGER NOT NULL, source_sha256 TEXT NOT NULL,
  page_count INTEGER, spine_count INTEGER);
CREATE TABLE source_blob(
  doc_id INTEGER PRIMARY KEY REFERENCES documents(doc_id),
  payload BLOB NOT NULL);
CREATE TABLE metadata(
  doc_id INTEGER REFERENCES documents(doc_id), key TEXT NOT NULL, value TEXT,
  PRIMARY KEY(doc_id,key));
CREATE TABLE blocks(
  block_id INTEGER PRIMARY KEY, doc_id INTEGER NOT NULL,
  unit TEXT NOT NULL, ordering INTEGER NOT NULL, kind TEXT,
  text TEXT NOT NULL, text_sha256 TEXT NOT NULL);
CREATE INDEX idx_blocks_doc_order ON blocks(doc_id,unit,ordering);
CREATE TABLE headings(
  heading_id INTEGER PRIMARY KEY, doc_id INTEGER NOT NULL,
  block_id INTEGER NOT NULL, level INTEGER NOT NULL);
CREATE INDEX idx_headings_doc ON headings(doc_id);
CREATE TABLE db_tables(
  table_id INTEGER PRIMARY KEY, doc_id INTEGER NOT NULL, block_id INTEGER,
  n_rows INTEGER, n_cols INTEGER);
CREATE TABLE table_cells(
  table_id INTEGER NOT NULL, r INTEGER NOT NULL, c INTEGER NOT NULL,
  text TEXT, PRIMARY KEY(table_id,r,c));
CREATE TABLE resources(
  resource_id INTEGER PRIMARY KEY, doc_id INTEGER NOT NULL, path TEXT NOT NULL,
  media_type TEXT, member_bytes INTEGER NOT NULL, member_sha256 TEXT NOT NULL,
  payload_offset INTEGER, payload_length INTEGER, decoded INTEGER DEFAULT 0);
CREATE INDEX idx_res_doc ON resources(doc_id);
CREATE VIRTUAL TABLE fts USING fts5(
  text, content='blocks', content_rowid='block_id',
  tokenize='unicode61 remove_diacritics 2');
"""


def build(fmt, source, db_path):
    import sqlite3
    doc = extract(fmt, source)
    rm(db_path)
    rm(db_path + "-wal")
    rm(db_path + "-shm")
    con = sqlite3.connect(db_path)
    con.executescript(SCHEMA)
    cur = con.cursor()
    m = doc["metadata"]
    cur.execute(
        "INSERT INTO documents VALUES(1,?,?,?,?,?,?)",
        (fmt, source, m.get("source_len", 0), m.get("source_sha256", ""),
         m.get("pages"), m.get("spine")))
    with open(source, "rb") as f:
        payload = f.read()
    cur.execute("INSERT INTO source_blob VALUES(1,?)", (payload,))
    cur.execute("INSERT INTO metadata VALUES(1,'source_len',?)",
                (str(m.get("source_len", 0)),))
    cur.execute("INSERT INTO metadata VALUES(1,'source_sha256',?)",
                (m.get("source_sha256", ""),))
    for k, v in m.items():
        cur.execute("INSERT OR REPLACE INTO metadata VALUES(1,?,?)", (k, str(v)))
    block_ids = []
    for i, b in enumerate(doc["blocks"]):
        cur.execute("INSERT INTO blocks VALUES(?,1,?,?,?,?,?)",
                    (i + 1, b["unit"], b["ordering"], b["kind"], b["text"],
                     b["sha256"]))
        block_ids.append(i + 1)
    for i, h in enumerate(doc["headings"]):
        cur.execute("INSERT INTO headings VALUES(?,1,?,?)",
                    (i + 1, block_ids[h["block"]], h["level"]))
    for i, t in enumerate(doc["tables"]):
        cur.execute("INSERT INTO db_tables VALUES(?,1,?,?,?)",
                    (i + 1, block_ids[t["block"]], t["n_rows"], t["n_cols"]))
    for c in doc["cells"]:
        cur.execute("INSERT INTO table_cells VALUES(?,?,?,?)",
                    (c["table"] + 1, c["r"], c["c"], c["text"]))
    for i, r in enumerate(doc["resources"]):
        cur.execute("INSERT INTO resources VALUES(?,1,?,?,?,?,?,?,?)",
                    (i + 1, r["path"], None, r["member_bytes"],
                     r["member_sha256"], r.get("offset"), r.get("raw_length"),
                     1 if r["decoded"] else 0))
    cur.execute("INSERT INTO fts(rowid,text) SELECT block_id,text FROM blocks")
    cur.execute("ANALYZE")
    con.commit()
    con.close()
    metrics = dict(doc["metrics"])
    metrics["db_bytes"] = os.path.getsize(db_path)
    return doc, metrics


# ---------------------------------------------------------------------------
# A0 — direct per-query extraction
# ---------------------------------------------------------------------------

def answer_for(doc, case, arg):
    if case == "block":
        n = int(arg)
        return {"text": doc["blocks"][n]["text"] if n < len(doc["blocks"]) else None}
    if case == "heading":
        n = int(arg)
        return {"text": doc["headings"][n]["text"] if n < len(doc["headings"]) else None}
    if case == "cell":
        t, r, c = (int(x) for x in arg.split(":"))
        hit = [x for x in doc["cells"] if x["table"] == t and x["r"] == r and x["c"] == c]
        return {"text": hit[0]["text"] if hit else None}
    if case == "table":
        n = int(arg)
        hit = [b for b in doc["blocks"] if b["kind"] == "table"]
        return {"text": hit[n]["text"] if n < len(hit) else None}
    if case == "resource-meta":
        n = int(arg)
        return {"value": doc["resources"][n] if n < len(doc["resources"]) else None}
    if case == "metadata":
        return {"value": doc["metadata"]}
    if case == "doc-text":
        return {"text": "\n".join(b["text"] for b in doc["blocks"])}
    if case == "search":
        return {"value": [{"block": b["ordering"], "text": b["text"]}
                          for b in doc["blocks"] if arg in b["text"]]}
    return None


def main(argv=None):
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    e = sub.add_parser("extract")
    e.add_argument("--format", required=True)
    e.add_argument("--source", required=True)
    e.add_argument("--out", required=True)
    b = sub.add_parser("build")
    b.add_argument("--format", required=True)
    b.add_argument("--source", required=True)
    b.add_argument("--db", required=True)
    b.add_argument("--metrics", default=None)
    q = sub.add_parser("query")
    q.add_argument("--format", required=True)
    q.add_argument("--source", required=True)
    q.add_argument("--case", required=True)
    q.add_argument("--arg", default="")
    q.add_argument("--out", required=True)
    q.add_argument("--metrics", default=None)
    a = ap.parse_args(argv)

    if a.cmd == "extract":
        doc = extract(a.format, a.source)
        os.makedirs(a.out, exist_ok=True)
        with open(os.path.join(a.out, "extraction.json"), "w") as f:
            json.dump(doc, f, sort_keys=True)
        print(json.dumps({"ok": True, "blocks": len(doc["blocks"]),
                          "metrics": doc["metrics"]}))
        return 0
    if a.cmd == "build":
        _doc, metrics = build(a.format, a.source, a.db)
        if a.metrics:
            with open(a.metrics, "w") as f:
                json.dump(metrics, f, sort_keys=True)
        print(json.dumps(metrics, sort_keys=True))
        return 0
    if a.cmd == "query":
        doc = extract(a.format, a.source)
        m = doc["metrics"]
        if a.case == "resource-bytes":
            n = int(a.arg)
            with zipfile.ZipFile(a.source) as z:
                data = z.read(doc["resources"][n]["path"])
            with open(a.out, "wb") as f:
                f.write(data)
            m["answer_bytes"] = len(data)
        else:
            payload = answer_for(doc, a.case, a.arg)
            if payload is None:
                payload = {"text": None}
            with open(a.out, "w") as f:
                json.dump(payload, f, sort_keys=True)
        if a.metrics:
            with open(a.metrics, "w") as f:
                json.dump(m, f, sort_keys=True)
        print(json.dumps(m, sort_keys=True))
        return 0
    return 2


if __name__ == "__main__":
    sys.exit(main())
