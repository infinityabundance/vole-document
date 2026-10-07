#!/usr/bin/env python3
# Structural probe for real100-v1 documents (runs in the pinned `realcorpus`
# Docker service). Read-only: it inspects the *downloaded bytes* with Poppler,
# qpdf and the Python stdlib and prints objective structural facts. Those facts
# justify the manifest's `structural_tags`; the probe never runs the codec and
# never consults a codec result, so a tag can never be influenced by a loss.
#
#   docker compose run --rm --no-TTY realcorpus \
#     python3 tools/realcorpus/probe.py real100-v1/documents/**/*.pdf
#
# Output: one TSV row per file (header first):
#   path format pages bytes text_chars images fonts objstm xref_stream
#   encrypted zip_members media_files spine tables headings max_heading
#   list_items text_chars2 suggested_tags

import json
import os
import re
import subprocess
import sys
import zipfile

IMAGE_EXT = (".png", ".jpg", ".jpeg", ".gif", ".tif", ".tiff", ".bmp",
             ".webp", ".svg", ".emf", ".wmf")


def run(cmd):
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=180)
        return r.stdout
    except Exception:  # noqa: BLE001
        return ""


def read_head(path, n=1 << 22):
    with open(path, "rb") as f:
        return f.read(n)


def probe_pdf(path):
    facts = {"format": "pdf", "pages": "", "text_chars": "", "images": "",
             "fonts": "", "objstm": "", "xref_stream": "", "encrypted": "",
             "producer": ""}
    info = run(["pdfinfo", path])
    m = re.search(r"^Pages:\s+(\d+)", info, re.M)
    if m:
        facts["pages"] = m.group(1)
    if re.search(r"^Encrypted:\s+yes", info, re.M):
        facts["encrypted"] = "yes"
    mp = re.search(r"^Producer:\s+(.*)$", info, re.M)
    if mp:
        facts["producer"] = mp.group(1).strip()[:80]
    txt = run(["pdftotext", "-q", path, "-"])
    facts["text_chars"] = str(len(re.sub(r"\s+", "", txt)))
    imgs = run(["pdfimages", "-list", path])
    n = 0
    for line in imgs.splitlines()[2:]:
        if line.strip():
            n += 1
    facts["images"] = str(n)
    fonts = run(["pdffonts", path])
    facts["fonts"] = str(max(0, len(fonts.splitlines()) - 2))
    # A PDF with a cross-reference *stream* (rather than a classic table) and/or
    # compressed object streams is the "complex" family the corpus must cover.
    # Scan the tail (xref lives near the end) plus a head slice.
    blob = read_head(path, 1 << 20)
    try:
        with open(path, "rb") as f:
            f.seek(max(0, os.path.getsize(path) - (1 << 20)))
            blob += f.read(1 << 20)
    except OSError:
        pass
    facts["objstm"] = "yes" if b"/ObjStm" in blob else "no"
    facts["xref_stream"] = "yes" if (b"/XRef" in blob or b"/Type/XRef" in blob) else "no"
    return facts


def probe_docx(path):
    facts = {"format": "docx", "zip_members": "", "media_files": "",
             "tables": "", "headings": "", "max_heading": "", "list_items": "",
             "text_chars2": ""}
    try:
        with zipfile.ZipFile(path) as z:
            names = z.namelist()
            facts["zip_members"] = str(len(names))
            facts["media_files"] = str(sum(1 for n in names
                                           if n.startswith("word/media/")
                                           and n.lower().endswith(IMAGE_EXT)))
            doc = z.read("word/document.xml") if "word/document.xml" in names else b""
    except (zipfile.BadZipFile, KeyError, OSError):
        return facts
    facts["tables"] = str(doc.count(b"<w:tbl>"))
    facts["list_items"] = str(doc.count(b"<w:numPr>"))
    heads = re.findall(rb'w:val="Heading(\d)"', doc)
    facts["headings"] = str(len(heads))
    facts["max_heading"] = str(max((int(h) for h in heads), default=0))
    text = re.sub(rb"<[^>]+>", b" ", doc)
    facts["text_chars2"] = str(len(re.sub(rb"\s+", b"", text)))
    return facts


def probe_epub(path):
    facts = {"format": "epub", "zip_members": "", "media_files": "",
             "spine": "", "nav": "", "text_chars2": ""}
    try:
        with zipfile.ZipFile(path) as z:
            names = z.namelist()
            facts["zip_members"] = str(len(names))
            facts["media_files"] = str(sum(1 for n in names
                                           if n.lower().endswith(IMAGE_EXT)))
            opf = None
            if "META-INF/container.xml" in names:
                c = z.read("META-INF/container.xml").decode("utf-8", "replace")
                m = re.search(r'full-path="([^"]+)"', c)
                if m:
                    opf = m.group(1)
            spine = 0
            nav = "no"
            total = 0
            if opf and opf in names:
                doc = z.read(opf).decode("utf-8", "replace")
                spine = len(re.findall(r"<itemref\b", doc))
                nav = "yes" if re.search(r'properties="[^"]*\bnav\b', doc) else nav
            facts["spine"] = str(spine)
            facts["nav"] = nav
            # sample text length across xhtml members (bounded)
            for n in names:
                if n.lower().endswith((".xhtml", ".html", ".htm")):
                    data = z.read(n)[:1 << 20]
                    total += len(re.sub(rb"\s+", b"", re.sub(rb"<[^>]+>", b" ", data)))
                    if total > (1 << 22):
                        break
            facts["text_chars2"] = str(total)
    except (zipfile.BadZipFile, OSError):
        return facts
    return facts


def detect(path):
    with open(path, "rb") as f:
        head = f.read(1024)
    # PDF may carry up to 1024 bytes of leading junk before `%PDF-`.
    if b"%PDF-" in head[:1024]:
        return "pdf"
    if head[:4] == b"PK\x03\x04":
        return _detect_opc(path)
    return None


def _detect_opc(path):
    try:
        with zipfile.ZipFile(path) as z:
            names = set(z.namelist())
            if "mimetype" in names and b"epub" in z.read("mimetype")[:64].lower():
                return "epub"
            if "META-INF/container.xml" in names or "word/document.xml" not in names:
                if "word/document.xml" not in names:
                    return "epub"
            if "word/document.xml" in names:
                return "docx"
            return "epub"
    except (zipfile.BadZipFile, OSError):
        return None


def suggest(f):
    tags = []
    fmt = f.get("format")

    def as_int(k):
        v = f.get(k, "")
        return int(v) if str(v).isdigit() else 0

    if fmt == "pdf":
        pages = as_int("pages")
        tc = as_int("text_chars")
        imgs = as_int("images")
        prod = (f.get("producer") or "").lower()
        # Scanned/legacy: a scan-conversion producer, or an image-heavy file
        # with no embedded fonts. Both are objective byte facts.
        if any(k in prod for k in ("capture", "scan", "ocr")):
            tags.append("scanned")
        elif pages and imgs and imgs >= pages and as_int("fonts") == 0:
            tags.append("scanned")
        if imgs and pages and imgs >= pages:
            tags.append("figure-heavy")
        if f.get("objstm") == "yes" or f.get("xref_stream") == "yes":
            tags.append("complex-xref")
        if pages and pages >= 300:
            tags.append("very-large")
    elif fmt == "docx":
        if as_int("tables") >= 3:
            tags.append("table-heavy")
        if as_int("media_files") >= 3:
            tags.append("image-heavy")
        if as_int("max_heading") >= 3:
            tags.append("deep-headings")
        if as_int("list_items") >= 10:
            tags.append("list-heavy")
    elif fmt == "epub":
        if as_int("spine") >= 10:
            tags.append("multi-chapter")
        if as_int("media_files") >= 20:
            tags.append("image-heavy")
        if f.get("nav") == "yes":
            tags.append("nav-heavy")
    return ";".join(tags)


def main():
    paths = sys.argv[1:]
    if not paths:
        print("usage: probe.py FILE...", file=sys.stderr)
        return 2
    fields = ["path", "format", "pages", "bytes", "text_chars", "images",
              "fonts", "objstm", "xref_stream", "encrypted", "zip_members",
              "media_files", "spine", "nav", "tables", "headings",
              "max_heading", "list_items", "text_chars2", "suggested_tags"]
    print("\t".join(fields))
    rc = 0
    for p in sorted(paths):
        try:
            fmt = detect(p)
            if fmt is None:
                print("%s\tUNKNOWN" % p)
                continue
            if fmt == "pdf":
                f = probe_pdf(p)
            elif fmt == "docx":
                f = probe_docx(p)
            else:
                f = probe_epub(p)
            b = str(os.path.getsize(p))
            f["bytes"] = b
            f["path"] = p
            f["suggested_tags"] = suggest(f)
            print("\t".join(str(f.get(k, "")) for k in fields))
        except Exception as e:  # noqa: BLE001
            print("%s\tERROR\t%s" % (p, e), file=sys.stderr)
            rc = 1
    return rc


if __name__ == "__main__":
    sys.exit(main())
