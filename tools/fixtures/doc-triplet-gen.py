#!/usr/bin/env python3
# Phase 12.9 — deterministic cross-format triplet generator.
#
# Emits the SAME known logical report as three formats plus a machine-readable
# ground truth, so tools/phase12-triplet-court.sh can assert that one common
# observation vocabulary returns the same logical answer with different native
# provenance. It uses only the Python standard library (no VOLE code) and is
# pinned by the `producers` image (python3 3.11.2 on debian:bookworm-slim).
#
#   report.pdf    a single-page classic-xref PDF whose heuristic text includes
#                 the title, headings, paragraphs, list, table row and link URL
#   report.docx   a WordprocessingML OPC package (styles/headings, hyperlink,
#                 an embedded PNG resource, an 8x2 table with B7 = bravo-seven)
#   report.epub   an OCF EPUB 3 package (XHTML headings/list/table/link/image)
#   ground_truth.json   the logical report + per-format length/sha256 oracle
#
# The bytes are deterministic (fixed ZIP timestamps, fixed datetime, no locale).
# PDF structure mirrors the classic-xref fixtures used by the Rust courts, so the
# Phase-11 physical scanner and the heuristic text-runs extractor accept it.
#
# Usage: python3 tools/fixtures/doc-triplet-gen.py OUTDIR

import hashlib
import io
import json
import os
import sys
import zipfile
import zlib

# ---------------------------------------------------------------------------
# The logical report (the canonical ground truth; both the DOCX/EPUB/PDF bytes
# and ground_truth.json are derived from these constants).
# ---------------------------------------------------------------------------

TITLE = "Phase Twelve Equivalence Report"
LANGUAGE = "en"
IDENTIFIER = "urn:uuid:8e0f2c4a-1b2d-4c3e-9f10-1234567890ab"

HEADINGS = [
    (1, "Introduction"),
    (2, "Method"),
    (3, "Findings"),
]
PARAGRAPHS = [
    "Alpha paragraph carries marker XF12A.",
    "Bravo paragraph carries marker XF12B.",
]
LIST_ITEMS = ["First step", "Second step", "Third step"]
TABLE_ROWS = [
    ["Key", "Value"],
    ["alpha", "one"],
    ["bravo", "two"],
    ["charlie", "three"],
    ["delta", "four"],
    ["echo", "five"],
    ["golf", "bravo-seven"],  # column B, row 7 -> the B7 cell
    ["hotel", "eight"],
]
B7 = TABLE_ROWS[6][1]  # 0-based row index 6 == 1-based row 7; column index 1 == B
LINK_TEXT = "external"
LINK_HREF = "https://example.com/phase12"
MARKERS = ["XF12A", "XF12B"]

W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
R_NS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
A_NS = "http://schemas.openxmlformats.org/drawingml/2006/main"
CT_NS = "http://schemas.openxmlformats.org/package/2006/content-types"
REL_NS = "http://schemas.openxmlformats.org/package/2006/relationships"
OPC_REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"

MAIN_CT = "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"
STYLES_CT = "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"
CORE_CT = "application/vnd.openxmlformats-package.core-properties+xml"


def shared_image() -> bytes:
    """A deterministic PNG-signature resource (the one shared image)."""
    payload = bytes((i * 7 + 11) & 0xFF for i in range(1024))
    return b"\x89PNG\r\n\x1a\n" + payload


IMAGE_BYTES = shared_image()
IMAGE_SHA256 = hashlib.sha256(IMAGE_BYTES).hexdigest()


# ---------------------------------------------------------------------------
# Deterministic ZIP writer (stdlib zipfile; fixed metadata; no timestamps).
# ---------------------------------------------------------------------------

def build_zip(entries):
    """entries: list of (name, bytes, compress_type). Order preserved."""
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", allowZip64=False) as z:
        for name, data, method in entries:
            zi = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            zi.compress_type = method
            zi.create_system = 3  # unix, deterministic
            zi.external_attr = 0
            z.writestr(zi, data)
    return buf.getvalue()


STORE = zipfile.ZIP_STORED
DEFLATE = zipfile.ZIP_DEFLATED


# ---------------------------------------------------------------------------
# DOCX
# ---------------------------------------------------------------------------

def _p(text, style=None):
    ppr = f'<w:pPr><w:pStyle w:val="{style}"/></w:pPr>' if style else ""
    return f"<w:p>{ppr}<w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p>"


def _table():
    out = ["<w:tbl>"]
    for row in TABLE_ROWS:
        out.append("<w:tr>")
        for cell in row:
            out.append(f"<w:tc><w:p><w:r><w:t>{cell}</w:t></w:r></w:p></w:tc>")
        out.append("</w:tr>")
    out.append("</w:tbl>")
    return "".join(out)


def build_docx():
    content_types = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<Types xmlns="{CT_NS}">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Default Extension="png" ContentType="image/png"/>'
        f'<Override PartName="/word/document.xml" ContentType="{MAIN_CT}"/>'
        f'<Override PartName="/word/styles.xml" ContentType="{STYLES_CT}"/>'
        f'<Override PartName="/docProps/core.xml" ContentType="{CORE_CT}"/>'
        "</Types>"
    )
    package_rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<Relationships xmlns="{REL_NS}">'
        f'<Relationship Id="rId1" Type="{OPC_REL}/officeDocument" Target="word/document.xml"/>'
        f'<Relationship Id="rIdCore" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>'
        "</Relationships>"
    )
    doc_rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<Relationships xmlns="{REL_NS}">'
        f'<Relationship Id="rIdStyles" Type="{OPC_REL}/styles" Target="styles.xml"/>'
        f'<Relationship Id="rIdHl" Type="{OPC_REL}/hyperlink" Target="{LINK_HREF}" TargetMode="External"/>'
        f'<Relationship Id="rIdImg" Type="{OPC_REL}/image" Target="media/image1.png"/>'
        "</Relationships>"
    )
    styles = (
        f'<w:styles xmlns:w="{W_NS}">'
        '<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:pPr><w:outlineLvl w:val="0"/></w:pPr></w:style>'
        '<w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:pPr><w:outlineLvl w:val="1"/></w:pPr></w:style>'
        '<w:style w:type="paragraph" w:styleId="Heading3"><w:name w:val="heading 3"/><w:pPr><w:outlineLvl w:val="2"/></w:pPr></w:style>'
        "</w:styles>"
    )
    core = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" '
        'xmlns:dc="http://purl.org/dc/elements/1.1/">'
        f"<dc:title>{TITLE}</dc:title><dc:language>{LANGUAGE}</dc:language>"
        "</cp:coreProperties>"
    )
    body = [
        _p(HEADINGS[0][1], "Heading1"),
        _p(PARAGRAPHS[0]),
        _p(HEADINGS[1][1], "Heading2"),
        _p(PARAGRAPHS[1]),
        _p(HEADINGS[2][1], "Heading3"),
    ]
    body += [_p(item) for item in LIST_ITEMS]
    body.append(_p(f'<w:hyperlink r:id="rIdHl"><w:r><w:t>{LINK_TEXT}</w:t></w:r></w:hyperlink>'))
    body.append(_table())
    body.append(
        '<w:p><w:r><w:drawing>'
        f'<a:blip xmlns:a="{A_NS}" r:embed="rIdImg"/>'
        "</w:drawing></w:r></w:p>"
    )
    document = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<w:document xmlns:w="{W_NS}" xmlns:r="{R_NS}" xmlns:a="{A_NS}">'
        f'<w:body>{"".join(body)}<w:sectPr/></w:body></w:document>'
    )
    return build_zip([
        ("[Content_Types].xml", content_types.encode(), DEFLATE),
        ("_rels/.rels", package_rels.encode(), DEFLATE),
        ("word/document.xml", document.encode(), DEFLATE),
        ("word/styles.xml", styles.encode(), DEFLATE),
        ("word/_rels/document.xml.rels", doc_rels.encode(), DEFLATE),
        ("docProps/core.xml", core.encode(), DEFLATE),
        ("word/media/image1.png", IMAGE_BYTES, STORE),
    ])


# ---------------------------------------------------------------------------
# EPUB
# ---------------------------------------------------------------------------

def build_epub():
    container = (
        '<?xml version="1.0" encoding="UTF-8"?>'
        '<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">'
        '<rootfiles><rootfile full-path="OEBPS/package.opf" media-type="application/oebps-package+xml"/></rootfiles>'
        "</container>"
    )
    opf = (
        '<?xml version="1.0" encoding="UTF-8"?>'
        '<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="pub-id">'
        '<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">'
        f'<dc:identifier id="pub-id">{IDENTIFIER}</dc:identifier>'
        f"<dc:title>{TITLE}</dc:title><dc:language>{LANGUAGE}</dc:language>"
        "</metadata>"
        "<manifest>"
        '<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>'
        '<item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>'
        '<item id="img" href="images/image1.png" media-type="image/png"/>'
        "</manifest>"
        '<spine><itemref idref="ch1"/></spine>'
        "</package>"
    )
    nav_items = "".join(
        f'<li><a href="chapter1.xhtml">{h}</a></li>' for _, h in HEADINGS
    )
    nav = (
        '<?xml version="1.0" encoding="UTF-8"?>'
        '<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">'
        "<head><title>Nav</title></head><body>"
        f'<nav epub:type="toc" id="toc"><ol>{nav_items}</ol></nav>'
        "</body></html>"
    )
    table = ["<table>"]
    for i, row in enumerate(TABLE_ROWS):
        table.append("<tr>")
        tag = "th" if i == 0 else "td"
        for cell in row:
            table.append(f"<{tag}>{cell}</{tag}>")
        table.append("</tr>")
    table.append("</table>")
    chapter = (
        '<?xml version="1.0" encoding="UTF-8"?>'
        '<html xmlns="http://www.w3.org/1999/xhtml"><head><title>One</title></head><body>'
        f"<h1>{HEADINGS[0][1]}</h1><p>{PARAGRAPHS[0]}</p>"
        f"<h2>{HEADINGS[1][1]}</h2><p>{PARAGRAPHS[1]}</p>"
        f"<h3>{HEADINGS[2][1]}</h3>"
        "<ol>" + "".join(f"<li>{i}</li>" for i in LIST_ITEMS) + "</ol>"
        + "".join(table) +
        f'<p>See <a href="{LINK_HREF}">{LINK_TEXT}</a>.</p>'
        '<img src="images/image1.png" alt="figure"/>'
        "</body></html>"
    )
    return build_zip([
        ("mimetype", b"application/epub+zip", STORE),
        ("META-INF/container.xml", container.encode(), DEFLATE),
        ("OEBPS/package.opf", opf.encode(), DEFLATE),
        ("OEBPS/nav.xhtml", nav.encode(), DEFLATE),
        ("OEBPS/chapter1.xhtml", chapter.encode(), DEFLATE),
        ("OEBPS/images/image1.png", IMAGE_BYTES, STORE),
    ])


# ---------------------------------------------------------------------------
# PDF (classic xref, one page, lone-Flate content stream of text runs)
# ---------------------------------------------------------------------------

def _pdf_content():
    # The PDF has no structural coordinates, so it carries the same logical
    # report as flat text runs: title, headings, paragraphs, list, the B7 value
    # and the link URL. Content matches the ground truth; no structure is implied.
    lines = [f"({TITLE}) Tj T*"]
    lines.append(f"({HEADINGS[0][1]}) Tj T*")
    lines.append(f"({PARAGRAPHS[0]}) Tj T*")
    lines.append(f"({HEADINGS[1][1]}) Tj T*")
    lines.append(f"({PARAGRAPHS[1]}) Tj T*")
    lines.append(f"({HEADINGS[2][1]}) Tj T*")
    for item in LIST_ITEMS:
        lines.append(f"({item}) Tj T*")
    lines.append(f"(Key: golf Value: {B7}) Tj T*")
    lines.append(f"(Link: {LINK_HREF}) Tj")
    return ("BT /F1 12 Tf 72 720 Td\n" + "\n".join(lines) + "\nET\n").encode()


def build_pdf():
    content = _pdf_content()
    encoded = zlib.compress(content, 9)
    objs = {}
    objs[1] = b"<< /Type /Catalog /Pages 2 0 R >>"
    objs[2] = b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>"
    objs[3] = (b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
               b"/Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>")
    objs[4] = (b"<< /Length " + str(len(encoded)).encode()
               + b" /Filter /FlateDecode >>\nstream\n" + encoded + b"\nendstream")
    objs[5] = b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"
    objs[6] = (b"<< /Title (" + TITLE.encode() + b") /Producer (doc-triplet-gen.py) >>")

    size = len(objs) + 1
    out = bytearray(b"%PDF-1.5\n")
    offsets = {}
    for number in sorted(objs):
        offsets[number] = len(out)
        out += f"{number} 0 obj\n".encode()
        out += objs[number]
        out += b"\nendobj\n"
    xref = len(out)
    out += f"xref\n0 {size}\n".encode()
    out += b"0000000000 65535 f \n"
    for number in range(1, size):
        out += f"{offsets[number]:010} 00000 n \n".encode()
    out += (f"trailer\n<< /Size {size} /Root 1 0 R /Info 6 0 R >>\n"
            f"startxref\n{xref}\n%%EOF\n").encode()
    return bytes(out)


# ---------------------------------------------------------------------------
# Emit
# ---------------------------------------------------------------------------

def sha256_hex(b):
    return hashlib.sha256(b).hexdigest()


def main():
    if len(sys.argv) != 2:
        print("usage: doc-triplet-gen.py OUTDIR", file=sys.stderr)
        return 2
    outdir = sys.argv[1]
    os.makedirs(outdir, exist_ok=True)

    pdf = build_pdf()
    docx = build_docx()
    epub = build_epub()

    artifacts = {
        "report.pdf": pdf,
        "report.docx": docx,
        "report.epub": epub,
        "shared-image.png": IMAGE_BYTES,
    }
    for name, data in artifacts.items():
        with open(os.path.join(outdir, name), "wb") as f:
            f.write(data)

    truth = {
        "logical_report": {
            "title": TITLE,
            "language": LANGUAGE,
            "identifier": IDENTIFIER,
            "headings": [h for _, h in HEADINGS],
            "paragraphs": PARAGRAPHS,
            "list_items": LIST_ITEMS,
            "table": {"rows": TABLE_ROWS, "cell_b7": B7},
            "link": {"text": LINK_TEXT, "href": LINK_HREF},
            "resource": {
                "href": "images/image1.png",
                "media_type": "image/png",
                "sha256": IMAGE_SHA256,
                "length": len(IMAGE_BYTES),
            },
            "markers": MARKERS,
        },
        "formats": {
            "pdf": {
                "file": "report.pdf",
                "length": len(pdf),
                "sha256": sha256_hex(pdf),
                "native_selectors_expected": ["document", "object", "stream",
                                              "revision", "page", "byte-range",
                                              "text-match"],
                # The PDF adapter exposes structural document metadata, not a
                # document title, and no heading/table/cell/resource/link
                # coordinate; those create typed capability errors.
                "common_supported": ["metadata", "text", "search-match"],
                "common_declined": ["heading", "block", "table", "cell",
                                    "resource", "link"],
            },
            "docx": {
                "file": "report.docx",
                "length": len(docx),
                "sha256": sha256_hex(docx),
                "common_supported": ["metadata", "text", "heading", "block",
                                     "table", "cell", "resource", "link",
                                     "search-match"],
                "common_declined": [],
                "cell_b7_ref": "0:6:1",
                "resource_rel": "rIdImg",
            },
            "epub": {
                "file": "report.epub",
                "length": len(epub),
                "sha256": sha256_hex(epub),
                "common_supported": ["metadata", "text", "heading", "block",
                                     "table", "cell", "resource", "link",
                                     "search-match"],
                "common_declined": [],
                "cell_b7_ref": "0:6:1",
                "resource_href": "images/image1.png",
                "resource_media_type": "image/png",
            },
        },
        "generator": {
            "name": "doc-triplet-gen.py",
            "python": sys.version.split()[0],
            "note": "stdlib only (zipfile/zlib/hashlib/json); pinned by the producers image",
        },
    }
    with open(os.path.join(outdir, "ground_truth.json"), "w") as f:
        json.dump(truth, f, indent=2, sort_keys=True)
        f.write("\n")

    print(json.dumps({
        "outdir": outdir,
        "pdf": {"length": len(pdf), "sha256": sha256_hex(pdf)},
        "docx": {"length": len(docx), "sha256": sha256_hex(docx)},
        "epub": {"length": len(epub), "sha256": sha256_hex(epub)},
        "image_sha256": IMAGE_SHA256,
    }, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
