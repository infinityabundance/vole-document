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

import os
import zipfile

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


def main():
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
