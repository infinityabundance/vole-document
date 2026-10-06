#!/usr/bin/env python3
# Phase 12.11 — deterministic mixed PDF/DOCX/EPUB lifetime-court corpus.
#
# Emits a >=8-document mixed corpus (3 logical variants x 3 formats = 9
# documents) plus a machine-readable ground truth, so
# tools/phase12-lifetime-court.sh can assert every answer and measure the
# lifetime cost of the same pre-registered workload against A0 (direct tooling),
# A1 (one-time-preprocessed source-retaining SQLite+FTS5) and V (the Phase-12
# VOLE field).
#
# The alpha variant *is* the 12.9 canonical triplet: it calls the exact builders
# in tools/fixtures/doc-triplet-gen.py, so those bytes are byte-identical to the
# cross-format equivalence court's fixtures. The bravo/charlie variants reuse the
# same deterministic ZIP writer, PDF structure and namespace constants from that
# module but parameterise the logical report (more/fewer headings, paragraphs,
# list items, a wider table, different markers). Only the Python standard library
# is used and the bytes are fixed-by-construction (fixed ZIP timestamp, fixed
# datetime, no locale), so the corpus is reproducible from source alone.
#
# Usage: python3 tools/fixtures/phase12-corpus-gen.py OUTDIR

import hashlib
import hashlib
import importlib.util
import json
import os
import sys

_HERE = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location(
    "doc_triplet_gen", os.path.join(_HERE, "doc-triplet-gen.py")
)
g = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(g)

# ---------------------------------------------------------------------------
# The three logical variants. `alpha` delegates to the 12.9 builders verbatim.
# ---------------------------------------------------------------------------

VARIANT_PARAMS = {
    "bravo": {
        "title": "Phase Twelve Lifetime Corpus Bravo",
        "markers": ["XF13A", "XF13B", "XF13C"],
        "headings": [
            (1, "Overview"),
            (2, "Scope"),
            (3, "Structure"),
            (4, "Measurements"),
            (5, "Conclusion"),
        ],
        "paragraphs": [
            "Alpha record carries marker XF13A.",
            "Bravo record carries marker XF13B.",
            "Charlie record carries marker XF13C.",
            "Delta record has no marker.",
            "Echo record has no marker.",
            "Foxtrot record concludes the report.",
        ],
        "list_items": ["Plan", "Build", "Measure", "Report", "Review"],
        "table_rows": [["Item", "Metric", "Value"]]
        + [[f"row{n:02d}", f"metric{n}", f"value{n}"] for n in range(1, 15)],
        "link": ("reference", "https://example.com/bravo"),
        "cell_ref": (0, 10, 2),  # table 0, 0-based row 10, col 2
    },
    "charlie": {
        "title": "Phase Twelve Lifetime Corpus Charlie",
        "markers": ["XF14A"],
        "headings": [(1, "Preamble"), (2, "Details")],
        "paragraphs": ["Solo record carries marker XF14A."],
        "list_items": ["One", "Two"],
        "table_rows": [["Key", "Value"], ["k1", "v1"], ["k2", "v2"], ["k3", "v3"]],
        "link": None,
        "cell_ref": (0, 3, 1),
    },
}

# A deliberately LARGE variant: 1500 paragraphs and a 1001-row x 3-column
# table, each body record salted with a deterministic incompressible hex token
# so the compressed source (and therefore VOLE's descriptor, which retains the
# exact member structure) is genuinely large. This is where the source-retaining
# SQLite baseline's covering-index point lookups are expected to beat VOLE's
# per-observation full-descriptor read — the court must be able to lose.
def _filler(n, prefix):
    return prefix + hashlib.sha256(f"delta-{n}".encode()).hexdigest()[:24]


VARIANT_PARAMS["delta"] = {
    "title": "Phase Twelve Lifetime Corpus Delta",
    "markers": ["XF15A", "XF15B", "XF15C"],
    "headings": [
        (1, "Delta Overview"),
        (2, "Delta Scope"),
        (3, "Delta Structure"),
        (4, "Delta Measurements"),
        (5, "Delta Method"),
        (6, "Delta Results"),
        (7, "Delta Discussion"),
        (8, "Delta Conclusion"),
    ],
    "paragraphs": [
        "Delta record 000 carries marker XF15A.",
        "Delta record 001 carries marker XF15B.",
        "Delta record 002 carries marker XF15C.",
    ] + [f"Delta body record {n:04d} {_filler(n, 'tok')}." for n in range(3, 1500)],
    "list_items": [f"Delta step {n:03d} {_filler(n, 'step')}" for n in range(16)],
    "table_rows": [["Item", "Metric", "Value"]]
    + [[f"d-row{n:04d}", f"d-metric{n} {_filler(n, 'm')}", f"d-value{n} {_filler(n, 'v')}"]
       for n in range(1000)],
    "link": ("delta-reference", "https://example.com/delta"),
    "cell_ref": (0, 600, 2),
}


def _p(text, style=None):
    return g._p(text, style)


def _table(rows):
    out = ["<w:tbl>"]
    for row in rows:
        out.append("<w:tr>")
        for cell in row:
            out.append(f"<w:tc><w:p><w:r><w:t>{cell}</w:t></w:r></w:p></w:tc>")
        out.append("</w:tr>")
    out.append("</w:tbl>")
    return "".join(out)


def _heading_styles(levels):
    out = [f'<w:styles xmlns:w="{g.W_NS}">']
    for lvl in range(1, max(levels) + 1):
        out.append(
            f'<w:style w:type="paragraph" w:styleId="Heading{lvl}">'
            f'<w:name w:val="heading {lvl}"/>'
            f'<w:pPr><w:outlineLvl w:val="{lvl - 1}"/></w:pPr></w:style>'
        )
    out.append("</w:styles>")
    return "".join(out)


def _docx_body_blocks(p):
    """Canonical block order (must match the adapters' document order)."""
    blocks = []
    h = p["headings"]
    ps = p["paragraphs"]
    for i in range(max(len(h), len(ps))):
        if i < len(h):
            blocks.append(("heading", h[i][1]))
        if i < len(ps):
            blocks.append(("paragraph", ps[i]))
    for item in p["list_items"]:
        blocks.append(("list", item))
    if p["link"]:
        blocks.append(("link", p["link"][0]))
    blocks.append(("table", "table"))
    blocks.append(("image", "image"))
    return blocks


def build_docx_variant(p):
    ct = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<Types xmlns="{g.CT_NS}">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Default Extension="png" ContentType="image/png"/>'
        f'<Override PartName="/word/document.xml" ContentType="{g.MAIN_CT}"/>'
        f'<Override PartName="/word/styles.xml" ContentType="{g.STYLES_CT}"/>'
        f'<Override PartName="/docProps/core.xml" ContentType="{g.CORE_CT}"/>'
        "</Types>"
    )
    package_rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<Relationships xmlns="{g.REL_NS}">'
        f'<Relationship Id="rId1" Type="{g.OPC_REL}/officeDocument" Target="word/document.xml"/>'
        '<Relationship Id="rIdCore" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>'
        "</Relationships>"
    )
    rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<Relationships xmlns="{g.REL_NS}">'
        f'<Relationship Id="rIdStyles" Type="{g.OPC_REL}/styles" Target="styles.xml"/>'
    )
    if p["link"]:
        rels += (
            f'<Relationship Id="rIdHl" Type="{g.OPC_REL}/hyperlink" '
            f'Target="{p["link"][1]}" TargetMode="External"/>'
        )
    rels += (
        f'<Relationship Id="rIdImg" Type="{g.OPC_REL}/image" Target="media/image1.png"/>'
        "</Relationships>"
    )
    core = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" '
        'xmlns:dc="http://purl.org/dc/elements/1.1/">'
        f'<dc:title>{p["title"]}</dc:title><dc:language>en</dc:language>'
        "</cp:coreProperties>"
    )
    body = []
    h = p["headings"]
    ps = p["paragraphs"]
    for i in range(max(len(h), len(ps))):
        if i < len(h):
            body.append(_p(h[i][1], f"Heading{h[i][0]}"))
        if i < len(ps):
            body.append(_p(ps[i]))
    body += [_p(item) for item in p["list_items"]]
    if p["link"]:
        body.append(
            f'<w:p><w:hyperlink r:id="rIdHl"><w:r><w:t>{p["link"][0]}</w:t></w:r></w:hyperlink></w:p>'
        )
    body.append(_table(p["table_rows"]))
    body.append(
        '<w:p><w:r><w:drawing>'
        f'<a:blip xmlns:a="{g.A_NS}" r:embed="rIdImg"/>'
        "</w:drawing></w:r></w:p>"
    )
    document = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<w:document xmlns:w="{g.W_NS}" xmlns:r="{g.R_NS}" xmlns:a="{g.A_NS}">'
        f'<w:body>{"".join(body)}<w:sectPr/></w:body></w:document>'
    )
    return g.build_zip(
        [
            ("[Content_Types].xml", ct.encode(), g.DEFLATE),
            ("_rels/.rels", package_rels.encode(), g.DEFLATE),
            ("word/document.xml", document.encode(), g.DEFLATE),
            ("word/styles.xml", _heading_styles([lvl for lvl, _ in h]).encode(), g.DEFLATE),
            ("word/_rels/document.xml.rels", rels.encode(), g.DEFLATE),
            ("docProps/core.xml", core.encode(), g.DEFLATE),
            ("word/media/image1.png", g.IMAGE_BYTES, g.STORE),
        ]
    )


def build_epub_variant(p):
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
        f'<dc:identifier id="pub-id">{g.IDENTIFIER}</dc:identifier>'
        f'<dc:title>{p["title"]}</dc:title><dc:language>en</dc:language>'
        "</metadata>"
        "<manifest>"
        '<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>'
        '<item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>'
        '<item id="img" href="images/image1.png" media-type="image/png"/>'
        "</manifest>"
        '<spine><itemref idref="ch1"/></spine>'
        "</package>"
    )
    nav_items = "".join(f'<li><a href="chapter1.xhtml">{t}</a></li>' for _, t in p["headings"])
    nav = (
        '<?xml version="1.0" encoding="UTF-8"?>'
        '<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">'
        "<head><title>Nav</title></head><body>"
        f'<nav epub:type="toc" id="toc"><ol>{nav_items}</ol></nav>'
        "</body></html>"
    )
    h = p["headings"]
    ps = p["paragraphs"]
    body = []
    for i in range(max(len(h), len(ps))):
        if i < len(h):
            body.append(f"<h{h[i][0]}>{h[i][1]}</h{h[i][0]}>")
        if i < len(ps):
            body.append(f"<p>{ps[i]}</p>")
    body.append("<ol>" + "".join(f"<li>{i}</li>" for i in p["list_items"]) + "</ol>")
    table = ["<table>"]
    for i, row in enumerate(p["table_rows"]):
        table.append("<tr>")
        tag = "th" if i == 0 else "td"
        for cell in row:
            table.append(f"<{tag}>{cell}</{tag}>")
        table.append("</tr>")
    table.append("</table>")
    body.append("".join(table))
    if p["link"]:
        body.append(f'<p>See <a href="{p["link"][1]}">{p["link"][0]}</a>.</p>')
    body.append('<img src="images/image1.png" alt="figure"/>')
    chapter = (
        '<?xml version="1.0" encoding="UTF-8"?>'
        '<html xmlns="http://www.w3.org/1999/xhtml"><head><title>One</title></head><body>'
        + "".join(body)
        + "</body></html>"
    )
    return g.build_zip(
        [
            ("mimetype", b"application/epub+zip", g.STORE),
            ("META-INF/container.xml", container.encode(), g.DEFLATE),
            ("OEBPS/package.opf", opf.encode(), g.DEFLATE),
            ("OEBPS/nav.xhtml", nav.encode(), g.DEFLATE),
            ("OEBPS/chapter1.xhtml", chapter.encode(), g.DEFLATE),
            ("OEBPS/images/image1.png", g.IMAGE_BYTES, g.STORE),
        ]
    )


def build_pdf_variant(p):
    lines = [f'({p["title"]}) Tj T*']
    for _, t in p["headings"]:
        lines.append(f"({t}) Tj T*")
    for para in p["paragraphs"]:
        lines.append(f"({para}) Tj T*")
    for item in p["list_items"]:
        lines.append(f"({item}) Tj T*")
    # The table's first data cell and the cell_ref value, as flat text.
    tr = p["table_rows"]
    lines.append(f'({tr[1][0]}: {tr[1][1]}) Tj T*')
    cr = p["cell_ref"]
    lines.append(f'({tr[cr[1]][cr[2]]}) Tj T*')
    if p["link"]:
        lines.append(f'({p["link"][1]}) Tj')
    content = ("BT /F1 12 Tf 72 720 Td\n" + "\n".join(lines) + "\nET\n").encode()
    import zlib

    encoded = zlib.compress(content, 9)
    objs = {
        1: b"<< /Type /Catalog /Pages 2 0 R >>",
        2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        3: (
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
            b"/Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>"
        ),
        4: (
            b"<< /Length " + str(len(encoded)).encode() + b" /Filter /FlateDecode >>\nstream\n"
            + encoded + b"\nendstream"
        ),
        5: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        6: (
            b"<< /Title (" + p["title"].encode()
            + b") /Producer (phase12-corpus-gen.py) >>"
        ),
    }
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
    out += (
        f"trailer\n<< /Size {size} /Root 1 0 R /Info 6 0 R >>\nstartxref\n{xref}\n%%EOF\n"
    ).encode()
    return bytes(out)


def sha256_hex(b):
    return hashlib.sha256(b).hexdigest()


def variant_files(key):
    if key == "alpha":
        return {
            "pdf": g.build_pdf(),
            "docx": g.build_docx(),
            "epub": g.build_epub(),
        }
    p = VARIANT_PARAMS[key]
    return {
        "pdf": build_pdf_variant(p),
        "docx": build_docx_variant(p),
        "epub": build_epub_variant(p),
    }


def variant_truth(key):
    if key == "alpha":
        p = {
            "title": g.TITLE,
            "markers": list(g.MARKERS),
            "headings": [[lvl, t] for lvl, t in g.HEADINGS],
            "paragraphs": list(g.PARAGRAPHS),
            "list_items": list(g.LIST_ITEMS),
            "table_rows": [list(r) for r in g.TABLE_ROWS],
            "link": [g.LINK_TEXT, g.LINK_HREF],
            "cell_ref": [0, 6, 1],
        }
    else:
        p = VARIANT_PARAMS[key]
        p = {
            "title": p["title"],
            "markers": list(p["markers"]),
            "headings": [[lvl, t] for lvl, t in p["headings"]],
            "paragraphs": list(p["paragraphs"]),
            "list_items": list(p["list_items"]),
            "table_rows": [list(r) for r in p["table_rows"]],
            "link": list(p["link"]) if p["link"] else None,
            "cell_ref": list(p["cell_ref"]),
        }
    return p


def main():
    if len(sys.argv) != 2:
        print("usage: phase12-corpus-gen.py OUTDIR", file=sys.stderr)
        return 2
    outdir = sys.argv[1]
    os.makedirs(outdir, exist_ok=True)

    variants = {
        "alpha": variant_truth("alpha"),
        "bravo": variant_truth("bravo"),
        "charlie": variant_truth("charlie"),
        "delta": variant_truth("delta"),
    }

    files = {}
    formats = {}
    order = ("alpha", "bravo", "charlie", "delta")
    for key in order:
        fs = variant_files(key)
        files[key] = fs
        for fmt, data in fs.items():
            name = f"{key}.{fmt}"
            with open(os.path.join(outdir, name), "wb") as f:
                f.write(data)
        formats[key] = {
            fmt: {"file": f"{key}.{fmt}", "length": len(data), "sha256": sha256_hex(data)}
            for fmt, data in fs.items()
        }

    truth = {
        "corpus": "phase12-lifetime-mixed",
        "generator": "tools/fixtures/phase12-corpus-gen.py",
        "reuses": "tools/fixtures/doc-triplet-gen.py (alpha is byte-identical to the 12.9 triplet)",
        "python": sys.version.split()[0],
        "variants": variants,
        "formats": formats,
        "documents": [
            {"name": f"{key}.{fmt}", "variant": key, "format": fmt}
            for key in order
            for fmt in ("pdf", "docx", "epub")
        ],
        "resource": {
            "href": "images/image1.png",
            "media_type": "image/png",
            "sha256": g.IMAGE_SHA256,
            "length": len(g.IMAGE_BYTES),
        },
    }
    with open(os.path.join(outdir, "ground_truth.json"), "w") as f:
        json.dump(truth, f, indent=2, sort_keys=True)
        f.write("\n")

    summary = {
        "outdir": outdir,
        "documents": len(truth["documents"]),
        "per_format": {
            fmt: {
                key: formats[key][fmt]["length"] for key in formats
            }
            for fmt in ("pdf", "docx", "epub")
        },
    }
    print(json.dumps(summary, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
