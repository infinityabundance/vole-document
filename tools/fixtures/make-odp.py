#!/usr/bin/env python3
"""Phase 21.4.1 — deterministic, self-authored ODP fixture generator.

Python **stdlib only** (`zipfile` + hand-written OpenDocument XML); no `odfpy`
and no external tool. Output is byte-deterministic: fixed member order (the
mandatory stored `mimetype` first), fixed `ZipInfo.date_time`, fixed compression
method per member. The generated `.odp` bytes are committed under
`tools/fixtures/odp/` and consumed by `tests/odp_adapter.rs` and
`tools/phase21-4-odp-court.sh`.

Nothing here is a conformance claim: each fixture exists to exercise one declared
part of the adapter (a multi-slide deck, an embedded table, a picture/media
reference, a notes page, and a deck whose `draw:page` document order differs from
any page-name order).
"""

import os
import zipfile

MIMETYPE = "application/vnd.oasis.opendocument.presentation"

OFFICE_NS = "urn:oasis:names:tc:opendocument:xmlns:office:1.0"
DRAW_NS = "urn:oasis:names:tc:opendocument:xmlns:drawing:1.0"
TEXT_NS = "urn:oasis:names:tc:opendocument:xmlns:text:1.0"
PRESENTATION_NS = "urn:oasis:names:tc:opendocument:xmlns:presentation:1.0"
STYLE_NS = "urn:oasis:names:tc:opendocument:xmlns:style:1.0"
FO_NS = "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0"
TABLE_NS = "urn:oasis:names:tc:opendocument:xmlns:table:1.0"
SVG_NS = "urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0"
XLINK_NS = "http://www.w3.org/1999/xlink"
DC_NS = "http://purl.org/dc/elements/1.1/"
MANIFEST_NS = "urn:oasis:names:tc:opendocument:xmlns:manifest:1.0"

FIXED_TIME = (2026, 1, 1, 0, 0, 0)

# A minimal, deterministic 1x1 RGBA PNG (real PNG signature + IHDR/IDAT/IEND).
PNG_1X1 = bytes.fromhex(
    "89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c489"
    "0000000a49444154789c63000100000500010d0a2db40000000049454e44ae426082"
)


# ---------------------------------------------------------------------------
# Package parts
# ---------------------------------------------------------------------------

def manifest_xml(extra_entries=()):
    lines = [
        '<?xml version="1.0" encoding="UTF-8"?>',
        f'<manifest:manifest xmlns:manifest="{MANIFEST_NS}" manifest:version="1.2">',
        f'<manifest:file-entry manifest:full-path="/" manifest:version="1.2" manifest:media-type="{MIMETYPE}"/>',
        '<manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>',
        '<manifest:file-entry manifest:full-path="styles.xml" manifest:media-type="text/xml"/>',
        '<manifest:file-entry manifest:full-path="meta.xml" manifest:media-type="text/xml"/>',
    ]
    for path, mt in extra_entries:
        lines.append(
            f'<manifest:file-entry manifest:full-path="{path}" manifest:media-type="{mt}"/>'
        )
    lines.append("</manifest:manifest>")
    return "".join(lines).encode("utf-8")


CONTENT_HEADER = (
    '<?xml version="1.0" encoding="UTF-8"?>'
    f'<office:document-content xmlns:office="{OFFICE_NS}" '
    f'xmlns:draw="{DRAW_NS}" xmlns:text="{TEXT_NS}" '
    f'xmlns:presentation="{PRESENTATION_NS}" xmlns:style="{STYLE_NS}" '
    f'xmlns:fo="{FO_NS}" xmlns:table="{TABLE_NS}" xmlns:svg="{SVG_NS}" '
    f'xmlns:xlink="{XLINK_NS}" xmlns:dc="{DC_NS}" office:version="1.2">'
)


def content_xml(presentation, automatic_styles=""):
    return (
        CONTENT_HEADER
        + f"<office:automatic-styles>{automatic_styles}</office:automatic-styles>"
        + "<office:body>"
        + f"<office:presentation>{presentation}</office:presentation>"
        + "</office:body>"
        + "</office:document-content>"
    ).encode("utf-8")


def styles_xml(master_pages="", styles=""):
    return (
        '<?xml version="1.0" encoding="UTF-8"?>'
        f'<office:document-styles xmlns:office="{OFFICE_NS}" '
        f'xmlns:draw="{DRAW_NS}" xmlns:text="{TEXT_NS}" '
        f'xmlns:presentation="{PRESENTATION_NS}" xmlns:style="{STYLE_NS}" '
        f'xmlns:fo="{FO_NS}" xmlns:svg="{SVG_NS}" office:version="1.2">'
        f"<office:master-styles>{master_pages}</office:master-styles>"
        f"<office:styles>{styles}</office:styles>"
        "</office:document-styles>"
    ).encode("utf-8")


def meta_xml():
    return (
        '<?xml version="1.0" encoding="UTF-8"?>'
        f'<office:document-meta xmlns:office="{OFFICE_NS}" xmlns:dc="{DC_NS}">'
        "<office:meta><dc:title>vole-odp-fixture</dc:title></office:meta>"
        "</office:document-meta>"
    ).encode("utf-8")


def write_odp(path, content, styles, extra_members=(), extra_manifest=()):
    """Write one deterministic ODP: stored `mimetype` first, then manifest, then
    content/styles/meta, then any extra members (e.g. `Pictures/*`)."""
    if os.path.exists(path):
        os.remove(path)
    with zipfile.ZipFile(path, "w") as zf:
        info = zipfile.ZipInfo("mimetype", FIXED_TIME)
        info.compress_type = zipfile.ZIP_STORED
        zf.writestr(info, MIMETYPE.encode("ascii"))
        info = zipfile.ZipInfo("META-INF/manifest.xml", FIXED_TIME)
        info.compress_type = zipfile.ZIP_DEFLATED
        zf.writestr(info, manifest_xml(extra_manifest))
        info = zipfile.ZipInfo("content.xml", FIXED_TIME)
        info.compress_type = zipfile.ZIP_DEFLATED
        zf.writestr(info, content)
        info = zipfile.ZipInfo("styles.xml", FIXED_TIME)
        info.compress_type = zipfile.ZIP_DEFLATED
        zf.writestr(info, styles)
        info = zipfile.ZipInfo("meta.xml", FIXED_TIME)
        info.compress_type = zipfile.ZIP_STORED
        zf.writestr(info, meta_xml())
        for name, data, comp in extra_members:
            info = zipfile.ZipInfo(name, FIXED_TIME)
            info.compress_type = comp
            zf.writestr(info, data)


DEFAULT_MASTER = '<style:master-page style:name="Default" style:page-layout-name="pm1"/>'
DEFAULT_STYLES = '<style:style style:name="Standard" style:family="presentation"/>'


# ---------------------------------------------------------------------------
# Element builders
# ---------------------------------------------------------------------------

def esc(text):
    return (
        str(text)
        .replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace('"', "&quot;")
    )


def frame_text(name, paragraphs, placeholder=None):
    ph = (
        f'<presentation:placeholder presentation:object="{esc(placeholder)}"/>'
        if placeholder
        else ""
    )
    body = "".join(f"<text:p>{esc(p)}</text:p>" for p in paragraphs)
    return (
        f'<draw:frame draw:name="{esc(name)}" svg:x="1cm" svg:y="1cm" '
        f'svg:width="10cm" svg:height="2cm">{ph}'
        f"<draw:text-box>{body}</draw:text-box></draw:frame>"
    )


def frame_image(name, href):
    return (
        f'<draw:frame draw:name="{esc(name)}" svg:x="1cm" svg:y="5cm" '
        f'svg:width="2cm" svg:height="2cm">'
        f'<draw:image xlink:href="{esc(href)}" xlink:type="simple" '
        f'xlink:show="embed" xlink:actuate="onLoad"/></draw:frame>'
    )


def frame_table(name, rows):
    row_xml = ""
    for row in rows:
        cells = "".join(
            f'<table:table-cell office:value-type="string"><text:p>{esc(c)}</text:p>'
            f"</table:table-cell>"
            for c in row
        )
        row_xml += f"<table:table-row>{cells}</table:table-row>"
    return (
        f'<draw:frame draw:name="{esc(name)}" svg:x="1cm" svg:y="8cm" '
        f'svg:width="12cm" svg:height="4cm">'
        f'<table:table table:name="{esc(name)}">{row_xml}</table:table></draw:frame>'
    )


def page(name, frames, notes=None, visibility=None, master="Default"):
    attrs = f'draw:name="{esc(name)}"'
    if master:
        attrs += f' draw:master-page-name="{esc(master)}"'
    if visibility:
        attrs += f' presentation:visibility="{esc(visibility)}"'
    body = "".join(frames)
    if notes is not None:
        body += (
            "<presentation:notes>"
            + frame_text("Notes", [notes])
            + "</presentation:notes>"
        )
    return f"<draw:page {attrs}>{body}</draw:page>"


# ---------------------------------------------------------------------------
# Individual fixtures
# ---------------------------------------------------------------------------

def basic(path):
    slides = (
        page("Slide 1", [frame_text("Title 1", ["Hello"], placeholder="title"),
                         frame_text("Body 1", ["World", "Second paragraph"])])
        + page("Slide 2", [frame_text("Title 2", ["Second"], placeholder="title"),
                           frame_text("Body 2", ["Body text"])])
        + page("Slide 3", [frame_text("Title 3", ["Third"], placeholder="title")])
    )
    write_odp(
        path,
        content_xml(slides),
        styles_xml(master_pages=DEFAULT_MASTER, styles=DEFAULT_STYLES),
    )


def table(path):
    rows = [["a", "b"], ["c", "d"]]
    slides = page("Slide 1", [frame_text("Title 1", ["Table"], placeholder="title"),
                              frame_table("Table 1", rows)])
    write_odp(
        path,
        content_xml(slides),
        styles_xml(master_pages=DEFAULT_MASTER, styles=DEFAULT_STYLES),
    )


def picture(path):
    slides = page("Slide 1", [frame_text("Title 1", ["Picture"], placeholder="title"),
                              frame_image("Image 1", "Pictures/image1.png")])
    write_odp(
        path,
        content_xml(slides),
        styles_xml(master_pages=DEFAULT_MASTER, styles=DEFAULT_STYLES),
        extra_members=[("Pictures/image1.png", PNG_1X1, zipfile.ZIP_STORED)],
        extra_manifest=[("Pictures/image1.png", "image/png")],
    )


def notes(path):
    slides = page(
        "Slide 1",
        [frame_text("Body 1", ["Body text"])],
        notes="Speaker notes here",
    )
    write_odp(
        path,
        content_xml(slides),
        styles_xml(master_pages=DEFAULT_MASTER, styles=DEFAULT_STYLES),
    )


def order(path):
    # The FIRST page in the document carries the page name "Slide 2"; the SECOND
    # page is named "Slide 1". Document order (never page-name order) therefore
    # yields "SECOND FILE" then "FIRST FILE".
    slides = (
        page("Slide 2", [frame_text("Title", ["SECOND FILE"], placeholder="title")])
        + page("Slide 1", [frame_text("Title", ["FIRST FILE"], placeholder="title")])
    )
    write_odp(
        path,
        content_xml(slides),
        styles_xml(master_pages=DEFAULT_MASTER, styles=DEFAULT_STYLES),
    )


def bomb(path):
    # A table-row repeated far beyond any plausible bound: the adapter must decline
    # typed (resource limit) rather than allocate the declared grid.
    row = (
        '<table:table-row table:number-rows-repeated="4000000">'
        '<table:table-cell office:value-type="string"><text:p>b</text:p>'
        "</table:table-cell></table:table-row>"
    )
    table = (
        '<draw:frame draw:name="Table 1"><table:table table:name="Bomb">'
        + row
        + "</table:table></draw:frame>"
    )
    slides = page("Slide 1", [frame_text("Title", ["Bomb"], placeholder="title"), table])
    write_odp(
        path,
        content_xml(slides),
        styles_xml(master_pages=DEFAULT_MASTER, styles=DEFAULT_STYLES),
    )


FIXTURES = {
    "basic.odp": basic,
    "table.odp": table,
    "picture.odp": picture,
    "notes.odp": notes,
    "order.odp": order,
    "bomb.odp": bomb,
}


# ===========================================================================
# Phase 21.4.2 — deterministic self-authored *corpus* mode (`--corpus DIR`).
#
# Emits a small corpus of varied ODP decks so the economic court has more than
# the tiny 21.4.1 fixtures to measure build/storage/query cost on. Every deck is
# byte-deterministic (fixed member order, fixed `ZipInfo.date_time`, fixed
# per-member compression method); numeric content uses a fixed-seed LCG (never
# `random`). Python **stdlib only**.
#
# The corpus is deliberately NOT a real-world population: it is self-authored
# and small, and every claim the court makes is scoped to it.
# ===========================================================================

def _lcg(seed):
    state = [seed & 0x7FFFFFFF]

    def nxt():
        state[0] = (1103515245 * state[0] + 12345) & 0x7FFFFFFF
        return state[0]

    return nxt


def png_blob(n, seed):
    """A deterministic PNG-signature-prefixed blob of exactly `n` bytes (>= 8)."""
    n = max(8, n)
    nxt = _lcg(seed)
    out = bytearray(b"\x89PNG\r\n\x1a\n")
    while len(out) < n:
        v = nxt()
        out.append(v & 0xFF)
        out.append((v >> 8) & 0xFF)
        out.append((v >> 16) & 0xFF)
        out.append((v >> 24) & 0xFF)
    return bytes(out[:n])


def _corpus_deck(path, n_slides, seed, media_bytes=0, with_tables=False, with_notes=False):
    nxt = _lcg(seed)
    slides = []
    for s in range(n_slides):
        frames = [frame_text(f"Title {s + 1}", [f"Slide {s + 1}"], placeholder="title")]
        paras = [f"bullet-{s}-{j}: {nxt() % 100000}" for j in range(4)]
        frames.append(frame_text(f"Body {s + 1}", paras))
        if with_tables and s % 4 == 0:
            rows = [[str(nxt() % 1000) for _ in range(4)] for _ in range(3)]
            frames.append(frame_table(f"Table {s}", rows))
        if media_bytes and s == 0:
            frames.append(frame_image("Image 1", "Pictures/big.png"))
        notes = f"notes for slide {s + 1}" if with_notes else None
        slides.append(page(f"Slide {s + 1}", frames, notes=notes))
    extra_members = []
    extra_manifest = []
    if media_bytes:
        extra_members.append(("Pictures/big.png", png_blob(media_bytes, seed), zipfile.ZIP_STORED))
        extra_manifest.append(("Pictures/big.png", "image/png"))
    write_odp(
        path,
        content_xml("".join(slides)),
        styles_xml(master_pages=DEFAULT_MASTER, styles=DEFAULT_STYLES),
        extra_members=extra_members,
        extra_manifest=extra_manifest,
    )


def c01_basic(path):
    _corpus_deck(path, 12, 21401, media_bytes=512 * 1024)


def c02_tables(path):
    _corpus_deck(path, 16, 21402, media_bytes=560 * 1024, with_tables=True)


def c03_media(path):
    _corpus_deck(path, 10, 21403, media_bytes=900 * 1024)


def c04_notes(path):
    _corpus_deck(path, 14, 21404, media_bytes=620 * 1024, with_notes=True)


def c05_order(path):
    # A deck whose page names are reversed relative to document order.
    slides = []
    for i in range(12):
        n = 12 - i
        slides.append(page(f"Slide {n}", [frame_text("Title", [f"page-{n}"], placeholder="title")]))
    write_odp(
        path,
        content_xml("".join(slides)),
        styles_xml(master_pages=DEFAULT_MASTER, styles=DEFAULT_STYLES),
        extra_members=[("Pictures/big.png", png_blob(720 * 1024, 21405), zipfile.ZIP_STORED)],
        extra_manifest=[("Pictures/big.png", "image/png")],
    )


def c06_many(path):
    _corpus_deck(path, 60, 21406, media_bytes=1200 * 1024)


def c07_mixed(path):
    _corpus_deck(path, 20, 21407, media_bytes=1700 * 1024, with_tables=True, with_notes=True)


def c08_large(path):
    # A larger deck (many slides + a stored media member) targeting ~1–3 MB.
    _corpus_deck(path, 220, 21408, media_bytes=2600 * 1024, with_tables=True)


CORPUS = [
    ("c01-basic.odp", c01_basic),
    ("c02-tables.odp", c02_tables),
    ("c03-media.odp", c03_media),
    ("c04-notes.odp", c04_notes),
    ("c05-order.odp", c05_order),
    ("c06-many.odp", c06_many),
    ("c07-mixed.odp", c07_mixed),
    ("c08-large.odp", c08_large),
]


def emit_corpus(out_dir):
    import hashlib

    os.makedirs(out_dir, exist_ok=True)
    manifest = []
    for name, fn in CORPUS:
        path = os.path.join(out_dir, name)
        fn(path)
        with open(path, "rb") as f:
            data = f.read()
        manifest.append((name, len(data), hashlib.sha256(data).hexdigest()))
    for name, length, sha in manifest:
        print(f"{name}\t{length}\t{sha}")
    return manifest


def main():
    import sys

    if len(sys.argv) >= 3 and sys.argv[1] == "--corpus":
        emit_corpus(sys.argv[2])
        return 0
    out_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), "odp")
    os.makedirs(out_dir, exist_ok=True)
    for name, fn in sorted(FIXTURES.items()):
        fn(os.path.join(out_dir, name))
        print(f"wrote {os.path.join(out_dir, name)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
