#!/usr/bin/env python3
"""Phase 21.2.1 — deterministic, self-authored PPTX fixture generator.

Python **stdlib only** (`zipfile` + hand-written PresentationML XML); no
`python-pptx` and no external tool. Output is byte-deterministic: fixed member
order, fixed `ZipInfo.date_time`, fixed compression method per member. The
generated `.pptx` bytes are committed under `tools/fixtures/pptx/` and consumed
by `tests/pptx_adapter.rs` and `tools/phase21-2-pptx-court.sh`.

The fixtures are deliberately tiny and label themselves self-authored. Nothing
here is a conformance claim: each fixture exists to exercise one declared part of
the adapter (a multi-slide title+body deck, a run-level text, an embedded table,
a picture in `ppt/media/`, a notes slide, and a deck whose `slideN.xml` file names
are *not* in presentation order — proving the order comes from `p:sldIdLst`).
"""

import binascii
import os
import struct
import zipfile
import zlib

CT_NS = "http://schemas.openxmlformats.org/package/2006/content-types"
REL_NS = "http://schemas.openxmlformats.org/package/2006/relationships"
OD_REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
P_NS = "http://schemas.openxmlformats.org/presentationml/2006/main"
A_NS = "http://schemas.openxmlformats.org/drawingml/2006/main"

CT_PRES_MAIN = (
    "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"
)
CT_SLIDE = "application/vnd.openxmlformats-officedocument.presentationml.slide+xml"
CT_MASTER = "application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"
CT_LAYOUT = "application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"
CT_NOTES_MASTER = (
    "application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml"
)
CT_NOTES_SLIDE = (
    "application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml"
)
CT_THEME = "application/vnd.openxmlformats-officedocument.theme+xml"
CT_PNG = "image/png"

XMLNS = f'xmlns:p="{P_NS}" xmlns:a="{A_NS}" xmlns:r="{OD_REL}"'

# A minimal self-authored 1x1 PNG (bytes only; never interpreted).
PNG_1X1 = bytes.fromhex(
    "89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c489"
    "0000000d4944415478da63fcffff3f030005fe02fea72d5e270000000049454e44ae426082"
)

FIXED_TIME = (2026, 1, 1, 0, 0, 0)


def content_types(slide_names, overrides=()):
    lines = [
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>',
        f'<Types xmlns="{CT_NS}">',
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>',
        '<Default Extension="xml" ContentType="application/xml"/>',
        f'<Default Extension="png" ContentType="{CT_PNG}"/>',
        f'<Override PartName="/ppt/presentation.xml" ContentType="{CT_PRES_MAIN}"/>',
        f'<Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="{CT_MASTER}"/>',
        f'<Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="{CT_LAYOUT}"/>',
        f'<Override PartName="/ppt/theme/theme1.xml" ContentType="{CT_THEME}"/>',
    ]
    for name in slide_names:
        lines.append(f'<Override PartName="/ppt/slides/{name}" ContentType="{CT_SLIDE}"/>')
    for part, ct in overrides:
        lines.append(f'<Override PartName="{part}" ContentType="{ct}"/>')
    lines.append("</Types>")
    return "".join(lines).encode("utf-8")


def package_rels():
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<Relationships xmlns="{REL_NS}">'
        f'<Relationship Id="rId1" Type="{OD_REL}/officeDocument" Target="ppt/presentation.xml"/>'
        "</Relationships>"
    ).encode("utf-8")


def presentation_xml(slide_rids):
    sld_ids = "".join(
        f'<p:sldId id="{256 + i}" r:id="{rid}"/>' for i, rid in enumerate(slide_rids)
    )
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f"<p:presentation {XMLNS}>"
        '<p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>'
        f"<p:sldIdLst>{sld_ids}</p:sldIdLst>"
        '<p:sldSz cx="9144000" cy="6858000"/>'
        '<p:notesSz cx="6858000" cy="9144000"/>'
        "</p:presentation>"
    ).encode("utf-8")


def presentation_rels(slide_targets_by_rid, with_notes_master=False):
    """slide_targets_by_rid: ordered list of (rid, target)."""
    rels = ['<Relationship Id="rId1" Type="%s/slideMaster" Target="slideMasters/slideMaster1.xml"/>' % OD_REL]
    for rid, target in slide_targets_by_rid:
        rels.append(f'<Relationship Id="{rid}" Type="{OD_REL}/slide" Target="{target}"/>')
    if with_notes_master:
        rels.append(
            f'<Relationship Id="rIdN" Type="{OD_REL}/notesMaster" Target="notesMasters/notesMaster1.xml"/>'
        )
    rels.append(f'<Relationship Id="rIdT" Type="{OD_REL}/theme" Target="theme/theme1.xml"/>')
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<Relationships xmlns="{REL_NS}">' + "".join(rels) + "</Relationships>"
    ).encode("utf-8")


def title_body_slide(title, bullets, name="Title 1"):
    body = "".join(
        f'<a:p><a:r><a:rPr lang="en-US"/><a:t>{esc(b)}</a:t></a:r></a:p>' for b in bullets
    )
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f"<p:sld {XMLNS}>"
        "<p:cSld><p:spTree>"
        '<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>'
        "<p:grpSpPr/>"
        "<p:sp>"
        f'<p:nvSpPr><p:cNvPr id="2" name="{esc(name)}"/><p:cNvSpPr/><p:nvPr>'
        '<p:ph type="title"/></p:nvPr></p:nvSpPr>'
        "<p:spPr/>"
        f'<p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>{esc(title)}</a:t></a:r></a:p></p:txBody>'
        "</p:sp>"
        "<p:sp>"
        '<p:nvSpPr><p:cNvPr id="3" name="Body 1"/><p:cNvSpPr/><p:nvPr>'
        '<p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr>'
        "<p:spPr/>"
        f"<p:txBody><a:bodyPr/><a:lstStyle/>{body}</p:txBody>"
        "</p:sp>"
        "</p:spTree></p:cSld>"
        '<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>'
        "</p:sld>"
    ).encode("utf-8")


def text_slide(text, name="Text 1"):
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f"<p:sld {XMLNS}>"
        "<p:cSld><p:spTree>"
        '<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>'
        "<p:grpSpPr/>"
        "<p:sp>"
        f'<p:nvSpPr><p:cNvPr id="2" name="{esc(name)}"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>'
        "<p:spPr/>"
        f'<p:txBody><a:bodyPr/><a:p><a:r><a:t>{esc(text)}</a:t></a:r></a:p></p:txBody>'
        "</p:sp>"
        "</p:spTree></p:cSld>"
        "</p:sld>"
    ).encode("utf-8")


def table_slide(rows):
    trs = []
    for row in rows:
        tcs = []
        for cell in row:
            tcs.append(
                "<a:tc><a:txBody><a:bodyPr/><a:p><a:r><a:t>"
                + esc(cell)
                + "</a:t></a:r></a:p></a:txBody><a:tcPr/></a:tc>"
            )
        trs.append("<a:tr>" + "".join(tcs) + "</a:tr>")
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f"<p:sld {XMLNS}>"
        "<p:cSld><p:spTree>"
        '<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>'
        "<p:grpSpPr/>"
        "<p:graphicFrame>"
        '<p:nvGraphicFramePr><p:cNvPr id="4" name="Table 1"/><p:cNvGraphicFramePr/>'
        "<p:nvPr/></p:nvGraphicFramePr>"
        "<p:xfrm/>"
        "<a:graphic><a:graphicData>"
        "<a:tbl><a:tblPr/><a:tblGrid/>" + "".join(trs) + "</a:tbl>"
        "</a:graphicData></a:graphic>"
        "</p:graphicFrame>"
        "</p:spTree></p:cSld>"
        "</p:sld>"
    ).encode("utf-8")


def picture_slide(name="Picture 1"):
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f"<p:sld {XMLNS}>"
        "<p:cSld><p:spTree>"
        '<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>'
        "<p:grpSpPr/>"
        "<p:pic>"
        f'<p:nvPicPr><p:cNvPr id="5" name="{esc(name)}"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr>'
        '<p:blipFill><a:blip r:embed="rId2"/></p:blipFill>'
        "<p:spPr/>"
        "</p:pic>"
        "</p:spTree></p:cSld>"
        "</p:sld>"
    ).encode("utf-8")


def notes_slide(text):
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f"<p:notes {XMLNS}>"
        "<p:cSld><p:spTree>"
        '<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>'
        "<p:grpSpPr/>"
        "<p:sp>"
        '<p:nvSpPr><p:cNvPr id="2" name="Notes Placeholder"/><p:cNvSpPr/><p:nvPr>'
        '<p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr>'
        "<p:spPr/>"
        f'<p:txBody><a:bodyPr/><a:p><a:r><a:t>{esc(text)}</a:t></a:r></a:p></p:txBody>'
        "</p:sp>"
        "</p:spTree></p:cSld>"
        "</p:notes>"
    ).encode("utf-8")


def master_xml():
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f"<p:sldMaster {XMLNS}>"
        "<p:cSld><p:spTree>"
        '<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>'
        "<p:grpSpPr/>"
        "</p:spTree></p:cSld>"
        "</p:sldMaster>"
    ).encode("utf-8")


def layout_xml():
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f"<p:sldLayout {XMLNS} type=\"title\">"
        "<p:cSld><p:spTree>"
        '<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>'
        "<p:grpSpPr/>"
        "</p:spTree></p:cSld>"
        "</p:sldLayout>"
    ).encode("utf-8")


def theme_xml():
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<a:theme xmlns:a="{A_NS}" name="Office Theme"><a:themeElements/></a:theme>'
    ).encode("utf-8")


def notes_master_xml():
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f"<p:notesMaster {XMLNS}><p:cSld><p:spTree>"
        '<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>'
        "<p:grpSpPr/></p:spTree></p:cSld></p:notesMaster>"
    ).encode("utf-8")


def slide_rels(extra=()):
    rels = [
        f'<Relationship Id="rId1" Type="{OD_REL}/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>'
    ]
    for rid, typ, target in extra:
        rels.append(f'<Relationship Id="{rid}" Type="{OD_REL}/{typ}" Target="{target}"/>')
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<Relationships xmlns="{REL_NS}">' + "".join(rels) + "</Relationships>"
    ).encode("utf-8")


def master_rels():
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<Relationships xmlns="{REL_NS}">'
        f'<Relationship Id="rId1" Type="{OD_REL}/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>'
        f'<Relationship Id="rId2" Type="{OD_REL}/theme" Target="../theme/theme1.xml"/>'
        "</Relationships>"
    ).encode("utf-8")


def esc(s):
    return (
        s.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace('"', "&quot;")
    )


def write_pptx(path, members):
    """members: ordered list of (name, bytes, compression)."""
    with zipfile.ZipFile(path, "w") as z:
        for name, data, comp in members:
            zi = zipfile.ZipInfo(name, date_time=FIXED_TIME)
            zi.compress_type = comp
            zi.external_attr = 0o600 << 16
            z.writestr(zi, data)


STORED = zipfile.ZIP_STORED
DEFLATED = zipfile.ZIP_DEFLATED


def basic():
    slide1 = title_body_slide("Quarterly Report", ["Revenue up", "Costs flat"])
    slide2 = text_slide("Thank you")
    rids = [("rId2", "slides/slide1.xml"), ("rId3", "slides/slide2.xml")]
    members = [
        ("[Content_Types].xml", content_types(["slide1.xml", "slide2.xml"]), STORED),
        ("_rels/.rels", package_rels(), DEFLATED),
        ("ppt/presentation.xml", presentation_xml(["rId2", "rId3"]), DEFLATED),
        ("ppt/_rels/presentation.xml.rels", presentation_rels(rids), DEFLATED),
        ("ppt/theme/theme1.xml", theme_xml(), DEFLATED),
        ("ppt/slideMasters/slideMaster1.xml", master_xml(), DEFLATED),
        ("ppt/slideMasters/_rels/slideMaster1.xml.rels", master_rels(), DEFLATED),
        ("ppt/slideLayouts/slideLayout1.xml", layout_xml(), DEFLATED),
        ("ppt/slides/slide1.xml", slide1, DEFLATED),
        ("ppt/slides/_rels/slide1.xml.rels", slide_rels(), DEFLATED),
        ("ppt/slides/slide2.xml", slide2, DEFLATED),
        ("ppt/slides/_rels/slide2.xml.rels", slide_rels(), DEFLATED),
    ]
    return members


def table():
    slide1 = table_slide([["Name", "Qty"], ["Widget", "3"]])
    rids = [("rId2", "slides/slide1.xml")]
    members = [
        ("[Content_Types].xml", content_types(["slide1.xml"]), STORED),
        ("_rels/.rels", package_rels(), DEFLATED),
        ("ppt/presentation.xml", presentation_xml(["rId2"]), DEFLATED),
        ("ppt/_rels/presentation.xml.rels", presentation_rels(rids), DEFLATED),
        ("ppt/theme/theme1.xml", theme_xml(), DEFLATED),
        ("ppt/slideMasters/slideMaster1.xml", master_xml(), DEFLATED),
        ("ppt/slideMasters/_rels/slideMaster1.xml.rels", master_rels(), DEFLATED),
        ("ppt/slideLayouts/slideLayout1.xml", layout_xml(), DEFLATED),
        ("ppt/slides/slide1.xml", slide1, DEFLATED),
        ("ppt/slides/_rels/slide1.xml.rels", slide_rels(), DEFLATED),
    ]
    return members


def picture():
    slide1 = picture_slide()
    rids = [("rId2", "slides/slide1.xml")]
    members = [
        ("[Content_Types].xml", content_types(["slide1.xml"]), STORED),
        ("_rels/.rels", package_rels(), DEFLATED),
        ("ppt/presentation.xml", presentation_xml(["rId2"]), DEFLATED),
        ("ppt/_rels/presentation.xml.rels", presentation_rels(rids), DEFLATED),
        ("ppt/theme/theme1.xml", theme_xml(), DEFLATED),
        ("ppt/slideMasters/slideMaster1.xml", master_xml(), DEFLATED),
        ("ppt/slideMasters/_rels/slideMaster1.xml.rels", master_rels(), DEFLATED),
        ("ppt/slideLayouts/slideLayout1.xml", layout_xml(), DEFLATED),
        ("ppt/slides/slide1.xml", slide1, DEFLATED),
        (
            "ppt/slides/_rels/slide1.xml.rels",
            slide_rels([("rId2", "image", "../media/image1.png")]),
            DEFLATED,
        ),
        ("ppt/media/image1.png", PNG_1X1, DEFLATED),
    ]
    return members


def notes():
    slide1 = text_slide("Body text")
    notes1 = notes_slide("Speaker notes here")
    overrides = [
        ("/ppt/notesSlides/notesSlide1.xml", CT_NOTES_SLIDE),
        ("/ppt/notesMasters/notesMaster1.xml", CT_NOTES_MASTER),
    ]
    rids = [("rId2", "slides/slide1.xml")]
    members = [
        ("[Content_Types].xml", content_types(["slide1.xml"], overrides), STORED),
        ("_rels/.rels", package_rels(), DEFLATED),
        (
            "ppt/presentation.xml",
            presentation_xml(["rId2"]),
            DEFLATED,
        ),
        (
            "ppt/_rels/presentation.xml.rels",
            presentation_rels(rids, with_notes_master=True),
            DEFLATED,
        ),
        ("ppt/theme/theme1.xml", theme_xml(), DEFLATED),
        ("ppt/slideMasters/slideMaster1.xml", master_xml(), DEFLATED),
        ("ppt/slideMasters/_rels/slideMaster1.xml.rels", master_rels(), DEFLATED),
        ("ppt/slideLayouts/slideLayout1.xml", layout_xml(), DEFLATED),
        ("ppt/notesMasters/notesMaster1.xml", notes_master_xml(), DEFLATED),
        ("ppt/slides/slide1.xml", slide1, DEFLATED),
        (
            "ppt/slides/_rels/slide1.xml.rels",
            slide_rels([("rId2", "notesSlide", "../notesSlides/notesSlide1.xml")]),
            DEFLATED,
        ),
        ("ppt/notesSlides/notesSlide1.xml", notes1, DEFLATED),
    ]
    return members


def order():
    """The `slideN.xml` file names are NOT in presentation order: `sldIdLst`
    lists rId3 (→ slide2.xml) first, then rId4 (→ slide1.xml)."""
    slide1 = text_slide("FIRST FILE")
    slide2 = text_slide("SECOND FILE")
    rids = [("rId3", "slides/slide2.xml"), ("rId4", "slides/slide1.xml")]
    members = [
        ("[Content_Types].xml", content_types(["slide1.xml", "slide2.xml"]), STORED),
        ("_rels/.rels", package_rels(), DEFLATED),
        ("ppt/presentation.xml", presentation_xml(["rId3", "rId4"]), DEFLATED),
        ("ppt/_rels/presentation.xml.rels", presentation_rels(rids), DEFLATED),
        ("ppt/theme/theme1.xml", theme_xml(), DEFLATED),
        ("ppt/slideMasters/slideMaster1.xml", master_xml(), DEFLATED),
        ("ppt/slideMasters/_rels/slideMaster1.xml.rels", master_rels(), DEFLATED),
        ("ppt/slideLayouts/slideLayout1.xml", layout_xml(), DEFLATED),
        ("ppt/slides/slide1.xml", slide1, DEFLATED),
        ("ppt/slides/_rels/slide1.xml.rels", slide_rels(), DEFLATED),
        ("ppt/slides/slide2.xml", slide2, DEFLATED),
        ("ppt/slides/_rels/slide2.xml.rels", slide_rels(), DEFLATED),
    ]
    return members


FIXTURES = [
    ("basic.pptx", basic),
    ("table.pptx", table),
    ("picture.pptx", picture),
    ("notes.pptx", notes),
    ("order.pptx", order),
]


# ===========================================================================
# Phase 21.2.3 deterministic corpus
#
#   python3 tools/fixtures/make-pptx.py --corpus DIR
#
# Emits `name<TAB>bytes<TAB>sha256` lines for 6-8 decks of varied, realistic
# shape: multi-slide decks, run-level text, embedded tables, `ppt/media/*.png`
# pictures, speaker notes, layouts/masters/themes, a deck whose `slideN.xml`
# file names are NOT in `p:sldIdLst` order, and a few-hundred-shape deck. All
# generated by Python stdlib only (no `python-pptx`); no external tool.
# ===========================================================================

CT_CHART = "application/vnd.openxmlformats-officedocument.drawingml.chart+xml"


class _LCG:
    """A tiny fixed-state LCG so pixel noise (and therefore every deck's byte
    length and SHA-256) is reproducible with no external dependency."""

    def __init__(self, seed):
        self.state = seed & 0x7FFFFFFF

    def byte(self):
        self.state = (1103515245 * self.state + 12345) & 0x7FFFFFFF
        return (self.state >> 16) & 0xFF


def make_png(w, h, seed):
    """A valid 8-bit RGB PNG whose pixel bytes are deterministic pseudo-noise
    (incompressible), so a deck can be grown to a target size without any
    external asset. VOLE never interprets the image; the bytes are opaque."""
    rng = _LCG(seed)
    raw = bytearray()
    for _ in range(h):
        raw.append(0)  # PNG filter type 0 (None)
        raw.extend(rng.byte() for _ in range(w * 3))

    def chunk(typ, payload):
        return (
            struct.pack(">I", len(payload))
            + typ
            + payload
            + struct.pack(">I", binascii.crc32(typ + payload) & 0xFFFFFFFF)
        )

    ihdr = struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + chunk(b"IDAT", zlib.compress(bytes(raw), 6))
        + chunk(b"IEND", b"")
    )


def c_content_types(slides, notes, layouts, masters, themes, charts):
    lines = [
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>',
        f'<Types xmlns="{CT_NS}">',
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>',
        '<Default Extension="xml" ContentType="application/xml"/>',
        f'<Default Extension="png" ContentType="{CT_PNG}"/>',
        f'<Override PartName="/ppt/presentation.xml" ContentType="{CT_PRES_MAIN}"/>',
    ]
    for n in masters:
        lines.append(f'<Override PartName="/ppt/slideMasters/{n}" ContentType="{CT_MASTER}"/>')
    for n in layouts:
        lines.append(f'<Override PartName="/ppt/slideLayouts/{n}" ContentType="{CT_LAYOUT}"/>')
    for n in themes:
        lines.append(f'<Override PartName="/ppt/theme/{n}" ContentType="{CT_THEME}"/>')
    for n in slides:
        lines.append(f'<Override PartName="/ppt/slides/{n}" ContentType="{CT_SLIDE}"/>')
    if notes:
        lines.append(
            '<Override PartName="/ppt/notesMasters/notesMaster1.xml" '
            f'ContentType="{CT_NOTES_MASTER}"/>'
        )
        for n in notes:
            lines.append(f'<Override PartName="/ppt/notesSlides/{n}" ContentType="{CT_NOTES_SLIDE}"/>')
    for n in charts:
        lines.append(f'<Override PartName="/ppt/charts/{n}" ContentType="{CT_CHART}"/>')
    lines.append("</Types>")
    return "".join(lines).encode("utf-8")


def c_rels(entries):
    """entries: list of (rid, type, target, external)."""
    parts = []
    for rid, typ, target, external in entries:
        mode = ' TargetMode="External"' if external else ""
        parts.append(f'<Relationship Id="{rid}" Type="{typ}" Target="{target}"{mode}/>')
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<Relationships xmlns="{REL_NS}">' + "".join(parts) + "</Relationships>"
    ).encode("utf-8")


def chart_xml(series_ref):
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" '
        'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">'
        '<c:chart><c:plotArea><c:layout/>'
        '<c:barChart><c:barDir val="col"/><c:ser><c:idx val="0"/><c:order val="0"/>'
        f'<c:val><c:numRef><c:f>{esc(series_ref)}</c:f></c:numRef></c:val>'
        '</c:ser></c:barChart></c:plotArea></c:chart></c:chartSpace>'
    ).encode("utf-8")


def _c_shape_xml(sh, ctx):
    kind = sh["kind"]
    sid = sh["id"]
    name = esc(sh.get("name", ""))
    if kind == "text":
        paras = "".join(
            "<a:p>"
            + "".join(
                f'<a:r><a:rPr lang="en-US"/><a:t>{esc(r)}</a:t></a:r>' for r in para
            )
            + "</a:p>"
            for para in sh["paras"]
        )
        ph = ""
        if sh.get("placeholder"):
            ph = f'<p:ph type="{sh["placeholder"]}"'
            if sh.get("idx") is not None:
                ph += f' idx="{sh["idx"]}"'
            ph += "/>"
        return (
            f'<p:sp><p:nvSpPr><p:cNvPr id="{sid}" name="{name}"/><p:cNvSpPr/>'
            f'<p:nvPr>{ph}</p:nvPr></p:nvSpPr><p:spPr/>'
            f'<p:txBody><a:bodyPr/><a:lstStyle/>{paras}</p:txBody></p:sp>'
        )
    if kind == "table":
        trs = ""
        for row in sh["rows"]:
            trs += "<a:tr>" + "".join(
                '<a:tc><a:txBody><a:bodyPr/><a:p><a:r><a:t>'
                + esc(c)
                + "</a:t></a:r></a:p></a:txBody><a:tcPr/></a:tc>"
                for c in row
            ) + "</a:tr>"
        return (
            f'<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="{sid}" name="{name}"/>'
            f'<p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm/>'
            f'<a:graphic><a:graphicData><a:tbl><a:tblPr/><a:tblGrid/>{trs}</a:tbl>'
            '</a:graphicData></a:graphic></p:graphicFrame>'
        )
    if kind == "pic":
        rid = ctx["media_rel"][sh["embed"]]
        return (
            f'<p:pic><p:nvPicPr><p:cNvPr id="{sid}" name="{name}"/><p:cNvPicPr/>'
            f'<p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="{rid}"/></p:blipFill>'
            '<p:spPr/></p:pic>'
        )
    if kind == "chart":
        rid = ctx["chart_rel"][sh["chart"]]
        return (
            f'<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="{sid}" name="{name}"/>'
            f'<p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm/>'
            '<a:graphic><a:graphicData '
            'uri="http://schemas.openxmlformats.org/drawingml/2006/chart">'
            '<c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" '
            f'r:id="{rid}"/></a:graphicData></a:graphic></p:graphicFrame>'
        )
    if kind == "group":
        inner = "".join(_c_shape_xml(c, ctx) for c in sh["children"])
        return (
            f'<p:grpSp><p:nvGrpSpPr><p:cNvPr id="{sid}" name="{name}"/><p:cNvGrpSpPr/>'
            f'<p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{inner}</p:grpSp>'
        )
    raise ValueError(f"unknown corpus shape kind {kind!r}")


def _c_slide(spec_slide, notes_file):
    """Return (slide_xml_bytes, rels_bytes) for one slide, assigning its media/
    chart/notes relationship ids deterministically (rId1 = slide layout)."""
    media_names = []
    chart_nums = []

    def scan(shapes):
        for sh in shapes:
            if sh["kind"] == "pic" and sh["embed"] not in media_names:
                media_names.append(sh["embed"])
            elif sh["kind"] == "chart" and sh["chart"] not in chart_nums:
                chart_nums.append(sh["chart"])
            elif sh["kind"] == "group":
                scan(sh["children"])

    scan(spec_slide["shapes"])
    rels = [("rId1", OD_REL + "/slideLayout", f"../slideLayouts/{spec_slide['layout']}", False)]
    media_rel = {}
    chart_rel = {}
    n = 2
    for m in media_names:
        rid = f"rId{n}"
        n += 1
        media_rel[m] = rid
        rels.append((rid, OD_REL + "/image", f"../media/{m}", False))
    for c in chart_nums:
        rid = f"rId{n}"
        n += 1
        chart_rel[c] = rid
        rels.append((rid, OD_REL + "/chart", f"../charts/chart{c}.xml", False))
    if notes_file:
        rels.append((f"rId{n}", OD_REL + "/notesSlide", f"../notesSlides/{notes_file}", False))
        n += 1

    ctx = {"media_rel": media_rel, "chart_rel": chart_rel}
    shapes_xml = "".join(_c_shape_xml(sh, ctx) for sh in spec_slide["shapes"])
    show = ' show="0"' if spec_slide.get("hidden") else ""
    slide = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<p:sld {XMLNS}{show}><p:cSld><p:spTree>'
        '<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>'
        "<p:grpSpPr/>" + shapes_xml +
        "</p:spTree></p:cSld>"
        '<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>'
    )
    return slide.encode("utf-8"), c_rels(rels)


def _c_master_rels(layouts, themes):
    rels = []
    n = 1
    for l in layouts:
        rels.append((f"rId{n}", OD_REL + "/slideLayout", f"../slideLayouts/{l}", False))
        n += 1
    for t in themes:
        rels.append((f"rId{n}", OD_REL + "/theme", f"../theme/{t}", False))
        n += 1
    return c_rels(rels)


def build_corpus_deck(spec):
    """Assemble one deterministic corpus deck's ordered member list."""
    masters = spec.get("masters") or ["slideMaster1.xml"]
    layouts = spec.get("layouts") or ["slideLayout1.xml"]
    themes = spec.get("themes") or ["theme1.xml"]
    slides = spec["slides"]  # presentation order
    media = spec.get("media") or []  # (name, w, h, seed)
    charts = spec.get("charts") or []  # chart numbers

    pres = [("rId1", OD_REL + "/slideMaster", f"slideMasters/{masters[0]}", False)]
    slide_rids = []
    for i, s in enumerate(slides):
        rid = f"rId{i + 2}"
        slide_rids.append(rid)
        pres.append((rid, OD_REL + "/slide", f"slides/{s['file']}", False))
    has_notes = bool(spec.get("notes")) and all(s.get("notes") for s in slides)
    nxt = len(slides) + 2
    if has_notes:
        pres.append((f"rId{nxt}", OD_REL + "/notesMaster", "notesMasters/notesMaster1.xml", False))
        nxt += 1
    pres.append((f"rId{nxt}", OD_REL + "/theme", f"theme/{themes[0]}", False))

    for i, s in enumerate(slides):
        if "layout" not in s:
            s["layout"] = layouts[i % len(layouts)]

    slide_files = [s["file"] for s in slides]
    notes_files = [f"notesSlide{i + 1}.xml" for i in range(len(slides))] if has_notes else []
    chart_files = [f"chart{c}.xml" for c in charts]

    members = [
        (
            "[Content_Types].xml",
            c_content_types(slide_files, notes_files, layouts, masters, themes, chart_files),
            STORED,
        ),
        ("_rels/.rels", package_rels(), DEFLATED),
        ("ppt/presentation.xml", presentation_xml(slide_rids), DEFLATED),
        ("ppt/_rels/presentation.xml.rels", c_rels(pres), DEFLATED),
    ]
    for t in themes:
        members.append((f"ppt/theme/{t}", theme_xml(), DEFLATED))
    for m in masters:
        members.append((f"ppt/slideMasters/{m}", master_xml(), DEFLATED))
        members.append((f"ppt/slideMasters/_rels/{m}.rels", _c_master_rels(layouts, themes), DEFLATED))
    for l in layouts:
        members.append((f"ppt/slideLayouts/{l}", layout_xml(), DEFLATED))
    for name, w, h, seed in media:
        members.append((f"ppt/media/{name}", make_png(w, h, seed), DEFLATED))
    for i, s in enumerate(slides):
        notes_file = notes_files[i] if has_notes else None
        sx, rx = _c_slide(s, notes_file)
        members.append((f"ppt/slides/{s['file']}", sx, DEFLATED))
        members.append((f"ppt/slides/_rels/{s['file']}.rels", rx, DEFLATED))
        if notes_file:
            members.append((f"ppt/notesSlides/{notes_file}", notes_slide(s["notes"]), DEFLATED))
    for c in charts:
        members.append(
            (
                f"ppt/charts/chart{c}.xml",
                chart_xml(spec.get("chart_ref", "Sheet1!$A$1:$A$5")),
                DEFLATED,
            )
        )
    if has_notes:
        members.append(("ppt/notesMasters/notesMaster1.xml", notes_master_xml(), DEFLATED))
    return members


def _slide(file, shapes, notes=None, layout=None, hidden=False):
    s = {"file": file, "shapes": shapes}
    if notes is not None:
        s["notes"] = notes
    if layout is not None:
        s["layout"] = layout
    if hidden:
        s["hidden"] = True
    return s


def _title(sid, text):
    return {"kind": "text", "id": sid, "name": "Title 1", "placeholder": "title", "paras": [[text]]}


def _body(sid, paras, name="Body 1"):
    return {
        "kind": "text",
        "id": sid,
        "name": name,
        "placeholder": "body",
        "idx": 1,
        "paras": paras,
    }


def deck_basic():
    return {
        "layouts": ["slideLayout1.xml", "slideLayout2.xml"],
        "media": [("image1.png", 420, 400, 21101)],
        "slides": [
            _slide(
                "slide1.xml",
                [_title(2, "Quarterly Report"), _body(3, [["Revenue ", "up 12%"], ["Costs ", "flat"]])],
            ),
            _slide("slide2.xml", [_title(2, "Details"), _body(3, [["Alpha"], ["Beta"], ["Gamma"]])]),
            _slide("slide3.xml", [_title(2, "Runs"), _body(3, [["run-", "level-", "text"], ["second ", "paragraph"]])]),
            _slide("slide4.xml", [_title(2, "Media"), {"kind": "pic", "id": 4, "name": "Picture 1", "embed": "image1.png"}]),
        ],
    }


def deck_tables():
    notes = [f"Speaker notes for slide {i + 1}" for i in range(3)]
    return {
        "layouts": ["slideLayout1.xml", "slideLayout2.xml"],
        "media": [("image1.png", 400, 420, 21202)],
        "notes": True,
        "slides": [
            _slide(
                "slide1.xml",
                [
                    _title(2, "Table"),
                    {"kind": "table", "id": 4, "name": "Table 1", "rows": [["Name", "Qty"], ["Widget", "3"], ["Gadget", "7"]]},
                ],
                notes=notes[0],
            ),
            _slide(
                "slide2.xml",
                [
                    _title(2, "More"),
                    {"kind": "table", "id": 4, "name": "Table 1", "rows": [["A", "B"], ["1", "2"]]},
                    _body(5, [["with a note"]]),
                ],
                notes=notes[1],
            ),
            _slide(
                "slide3.xml",
                [_title(2, "Picture"), {"kind": "pic", "id": 4, "name": "Picture 1", "embed": "image1.png"}],
                notes=notes[2],
            ),
        ],
    }


def deck_media():
    notes = [f"Notes {i + 1}" for i in range(4)]
    return {
        "slides": [
            _slide("slide1.xml", [_title(2, "Media A"), {"kind": "pic", "id": 4, "name": "Picture 1", "embed": "image1.png"}], notes=notes[0]),
            _slide("slide2.xml", [_title(2, "Media B"), {"kind": "pic", "id": 4, "name": "Picture 2", "embed": "image2.png"}], notes=notes[1]),
            _slide("slide3.xml", [_title(2, "Media C"), {"kind": "pic", "id": 4, "name": "Picture 3", "embed": "image3.png"}], notes=notes[2]),
            _slide("slide4.xml", [_title(2, "Runs"), _body(3, [["a", "b", "c"], ["d", "e"]])], notes=notes[3]),
        ],
        "media": [
            ("image1.png", 400, 300, 21301),
            ("image2.png", 300, 400, 21302),
            ("image3.png", 420, 400, 21303),
        ],
    }


def deck_notes():
    notes = [f"Detailed speaker notes for slide {i + 1}: context and talking points." for i in range(5)]
    return {
        "layouts": ["slideLayout1.xml", "slideLayout2.xml", "slideLayout3.xml"],
        "masters": ["slideMaster1.xml"],
        "themes": ["theme1.xml"],
        "media": [("image1.png", 500, 500, 21401)],
        "notes": True,
        "slides": [
            _slide("slide1.xml", [_title(2, "Notes 1"), _body(3, [["intro"]])], notes=notes[0]),
            _slide("slide2.xml", [_title(2, "Notes 2"), _body(3, [["detail"]])], notes=notes[1]),
            _slide("slide3.xml", [_title(2, "Notes 3"), _body(3, [["more"]])], notes=notes[2]),
            _slide(
                "slide4.xml",
                [
                    _title(2, "Group"),
                    {
                        "kind": "group",
                        "id": 4,
                        "name": "Group 1",
                        "children": [
                            _body(5, [["inner one"]], name="Inner 1"),
                            _body(6, [["inner two"]], name="Inner 2"),
                        ],
                    },
                ],
                notes=notes[3],
            ),
            _slide("slide5.xml", [_title(2, "Notes 5"), {"kind": "pic", "id": 4, "name": "Picture 1", "embed": "image1.png"}], notes=notes[4]),
        ],
    }


def deck_charts():
    notes = [f"Chart notes {i + 1}" for i in range(3)]
    return {
        "charts": [1, 2],
        "chart_ref": "Data!$A$1:$A$5",
        "media": [("image1.png", 420, 400, 21501)],
        "notes": True,
        "slides": [
            _slide("slide1.xml", [_title(2, "Chart A"), {"kind": "chart", "id": 4, "name": "Chart 1", "chart": 1}], notes=notes[0]),
            _slide("slide2.xml", [_title(2, "Chart B"), {"kind": "chart", "id": 4, "name": "Chart 2", "chart": 2}, _body(5, [["caption"]])], notes=notes[1]),
            _slide("slide3.xml", [_title(2, "Text"), _body(3, [["no chart here"]]), {"kind": "pic", "id": 4, "name": "Picture 1", "embed": "image1.png"}], notes=notes[2]),
        ],
    }


def deck_order():
    """`slideN.xml` file names are NOT in `p:sldIdLst` order: presentation order is
    slide2, slide1, slide4, slide3."""
    notes = [f"Order notes {i + 1}" for i in range(4)]
    return {
        "media": [("image1.png", 440, 400, 21601)],
        "notes": True,
        "slides": [
            _slide("slide2.xml", [_title(2, "SECOND FILE"), _body(3, [["two"]])], notes=notes[0]),
            _slide("slide1.xml", [_title(2, "FIRST FILE"), _body(3, [["one"]])], notes=notes[1]),
            _slide("slide4.xml", [_title(2, "FOURTH FILE"), _body(3, [["four"]])], notes=notes[2]),
            _slide("slide3.xml", [_title(2, "THIRD FILE"), {"kind": "pic", "id": 4, "name": "Picture 1", "embed": "image1.png"}], notes=notes[3]),
        ],
    }


def deck_many():
    """A few-hundred-shape deck: 6 slides x (1 title + 44 body shapes) = 270 shapes."""
    slides = []
    for i in range(6):
        shapes = [_title(2, f"Many {i + 1}")]
        sid = 3
        for j in range(44):
            shapes.append(_body(sid, [[f"shape {j} on slide {i}", f" run {j}"]], name=f"Text {j}"))
            sid += 1
        slides.append(_slide(f"slide{i + 1}.xml", shapes, notes=f"Notes for many slide {i + 1}"))
    return {
        "media": [("image1.png", 700, 700, 21701)],
        "notes": True,
        "slides": slides,
    }


def deck_mixed():
    notes = [f"Mixed notes {i + 1}" for i in range(5)]
    return {
        "charts": [1],
        "chart_ref": "Data!$B$2:$B$6",
        "layouts": ["slideLayout1.xml", "slideLayout2.xml"],
        "media": [("image1.png", 900, 800, 21801), ("image2.png", 300, 300, 21802)],
        "notes": True,
        "slides": [
            _slide("slide1.xml", [_title(2, "Mixed"), _body(3, [["intro ", "text"], ["second"]])], notes=notes[0]),
            _slide("slide2.xml", [_title(2, "Table"), {"kind": "table", "id": 4, "name": "Table 1", "rows": [["A", "B"], ["1", "2"], ["3", "4"]]}], notes=notes[1]),
            _slide("slide3.xml", [_title(2, "Picture"), {"kind": "pic", "id": 4, "name": "Picture 1", "embed": "image1.png"}], notes=notes[2]),
            _slide("slide4.xml", [_title(2, "Chart"), {"kind": "chart", "id": 4, "name": "Chart 1", "chart": 1}], notes=notes[3]),
            _slide("slide5.xml", [_title(2, "Group"), {"kind": "group", "id": 4, "name": "Group 1", "children": [_body(5, [["nested ", "text"]], name="Inner")]}, {"kind": "pic", "id": 6, "name": "Picture 2", "embed": "image2.png"}], notes=notes[4]),
        ],
    }


CORPUS_DECKS = [
    ("c01-basic.pptx", deck_basic),
    ("c02-tables.pptx", deck_tables),
    ("c03-media.pptx", deck_media),
    ("c04-notes.pptx", deck_notes),
    ("c05-charts.pptx", deck_charts),
    ("c06-order.pptx", deck_order),
    ("c07-many.pptx", deck_many),
    ("c08-mixed.pptx", deck_mixed),
]


def emit_corpus(out_dir):
    import hashlib

    os.makedirs(out_dir, exist_ok=True)
    manifest = []
    for name, fn in CORPUS_DECKS:
        path = os.path.join(out_dir, name)
        write_pptx(path, build_corpus_deck(fn()))
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
    here = os.path.dirname(os.path.abspath(__file__))
    out = os.path.join(here, "pptx")
    os.makedirs(out, exist_ok=True)
    for name, fn in FIXTURES:
        path = os.path.join(out, name)
        write_pptx(path, fn())
        print(f"{name}\t{os.path.getsize(path)} bytes")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
