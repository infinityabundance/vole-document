#!/usr/bin/env python3
"""Phase 21.1.1 — deterministic, self-authored XLSX fixture generator.

Python **stdlib only** (`zipfile` + hand-written SpreadsheetML XML); no
`openpyxl` and no external tool. Output is byte-deterministic: fixed member
order, fixed `ZipInfo.date_time`, fixed compression method per member. The
generated `.xlsx` bytes are committed under `tools/fixtures/xlsx/` and consumed
by `tests/xlsx_adapter.rs` and `tools/phase21-1-xlsx-court.sh`.

The fixtures are deliberately tiny and label themselves self-authored (they
carry a `dc:creator`/`docProps`-free, minimal SpreadsheetML surface). Nothing
here is a conformance claim: each fixture exists to exercise one declared part
of the adapter (shared strings, inline strings, numbers/booleans/errors, a
stored formula with a cached value, a minimal style, a merged range, and
multi-sheet order/visibility).
"""

import os
import zipfile

CT_NS = "http://schemas.openxmlformats.org/package/2006/content-types"
REL_NS = "http://schemas.openxmlformats.org/package/2006/relationships"
OD_REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
SSML = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"

CT_SHEET_MAIN = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"
CT_STYLES = "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"
CT_SST = "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"
CT_WS = "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"
# Phase 21.1.2 content types.
CT_COMMENTS = "application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml"
CT_DRAWING = "application/vnd.openxmlformats-officedocument.drawing+xml"
CT_CHART = "application/vnd.openxmlformats-officedocument.drawingml.chart+xml"
CT_TABLE = "application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml"
CT_VML = "application/vnd.openxmlformats-officedocument.vmlDrawing"
CT_EXT_LINK = "application/vnd.openxmlformats-officedocument.spreadsheetml.externalLink+xml"
CT_PNG = "image/png"

# DrawingML / chart namespaces.
XDR_NS = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing"
A_NS = "http://schemas.openxmlformats.org/drawingml/2006/main"
C_NS = "http://schemas.openxmlformats.org/drawingml/2006/chart"
VML_NS = "urn:schemas-microsoft-com:vml"
EXCEL_NS = "urn:schemas-microsoft-com:office:excel"

# A minimal self-authored 1x1 PNG (bytes only; never interpreted).
PNG_1X1 = bytes.fromhex(
    "89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c489"
    "0000000d4944415478da63fcffff3f030005fe02fea72d5e270000000049454e44ae426082"
)

FIXED_TIME = (2026, 1, 1, 0, 0, 0)


def content_types_custom(defaults, overrides):
    """Content types with extra `Default` entries before the `Override` list."""
    lines = [
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>',
        f'<Types xmlns="{CT_NS}">',
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>',
        '<Default Extension="xml" ContentType="application/xml"/>',
    ]
    for ext, ct in defaults:
        lines.append(f'<Default Extension="{ext}" ContentType="{ct}"/>')
    for part, ct in overrides:
        lines.append(f'<Override PartName="{part}" ContentType="{ct}"/>')
    lines.append("</Types>")
    return "".join(lines).encode("utf-8")


def content_types(overrides):
    lines = [
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>',
        f'<Types xmlns="{CT_NS}">',
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>',
        '<Default Extension="xml" ContentType="application/xml"/>',
    ]
    for part, ct in overrides:
        lines.append(f'<Override PartName="{part}" ContentType="{ct}"/>')
    lines.append("</Types>")
    return "".join(lines).encode("utf-8")


def package_rels(target="xl/workbook.xml"):
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<Relationships xmlns="{REL_NS}">'
        f'<Relationship Id="rId1" Type="{OD_REL}/officeDocument" Target="{target}"/>'
        "</Relationships>"
    ).encode("utf-8")


def workbook_rels(sheet_count, with_styles, with_sst):
    rels = []
    for i in range(sheet_count):
        rels.append(
            f'<Relationship Id="rId{i + 1}" Type="{OD_REL}/worksheet" '
            f'Target="worksheets/sheet{i + 1}.xml"/>'
        )
    nxt = sheet_count + 1
    if with_styles:
        rels.append(
            f'<Relationship Id="rId{nxt}" Type="{OD_REL}/styles" Target="styles.xml"/>'
        )
        nxt += 1
    if with_sst:
        rels.append(
            f'<Relationship Id="rId{nxt}" Type="{OD_REL}/sharedStrings" Target="sharedStrings.xml"/>'
        )
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<Relationships xmlns="{REL_NS}">' + "".join(rels) + "</Relationships>"
    ).encode("utf-8")


def workbook_xml(sheets):
    # sheets: list of (name, sheet_id, rel_id, state)
    parts = []
    for name, sid, rid, state in sheets:
        attr = f' state="{state}"' if state else ""
        parts.append(
            f'<sheet name="{name}" sheetId="{sid}" r:id="{rid}"{attr}/>'
        )
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<workbook xmlns="{SSML}" xmlns:r="{OD_REL}">'
        f"<sheets>{''.join(parts)}</sheets>"
        "</workbook>"
    ).encode("utf-8")


def shared_strings_xml(strings):
    si = "".join(f"<si><t>{s}</t></si>" for s in strings)
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<sst xmlns="{SSML}" count="{len(strings)}" uniqueCount="{len(strings)}">'
        f"{si}</sst>"
    ).encode("utf-8")


def styles_xml():
    # One custom number format (numFmtId 164) and two cellXfs entries: the
    # default and one that references the custom format.
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<styleSheet xmlns="{SSML}">'
        '<numFmts count="1"><numFmt numFmtId="164" formatCode="0.00"/></numFmts>'
        '<fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts>'
        '<fills count="2"><fill><patternFill patternType="none"/></fill>'
        '<fill><patternFill patternType="gray125"/></fill></fills>'
        '<borders count="1"><border/></borders>'
        '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0"/></cellStyleXfs>'
        '<cellXfs count="2">'
        '<xf numFmtId="0" fontId="0" fillId="0"/>'
        '<xf numFmtId="164" fontId="0" fillId="0"/>'
        "</cellXfs>"
        '<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles>'
        "</styleSheet>"
    ).encode("utf-8")


def worksheet_xml(rows, dimension=None, merges=()):
    dim = f'<dimension ref="{dimension}"/>' if dimension else ""
    body = []
    for r, cells in rows:
        cs = "".join(cells)
        body.append(f'<row r="{r}">{cs}</row>')
    mc = ""
    if merges:
        mc = (
            f'<mergeCells count="{len(merges)}">'
            + "".join(f'<mergeCell ref="{m}"/>' for m in merges)
            + "</mergeCells>"
        )
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<worksheet xmlns="{SSML}" xmlns:r="{OD_REL}">'
        f"{dim}<sheetData>{''.join(body)}</sheetData>{mc}</worksheet>"
    ).encode("utf-8")


def write_xlsx(path, members):
    """members: ordered list of (name, bytes, compression)."""
    with zipfile.ZipFile(path, "w") as z:
        for name, data, comp in members:
            zi = zipfile.ZipInfo(name, date_time=FIXED_TIME)
            zi.compress_type = comp
            zi.external_attr = 0o600 << 16
            z.writestr(zi, data)


def single():
    members = [
        (
            "[Content_Types].xml",
            content_types(
                [
                    ("/xl/workbook.xml", CT_SHEET_MAIN),
                    ("/xl/styles.xml", CT_STYLES),
                    ("/xl/sharedStrings.xml", CT_SST),
                    ("/xl/worksheets/sheet1.xml", CT_WS),
                ]
            ),
            zipfile.ZIP_STORED,
        ),
        ("_rels/.rels", package_rels(), zipfile.ZIP_DEFLATED),
        ("xl/workbook.xml", workbook_xml([("Sheet1", 1, "rId1", None)]), zipfile.ZIP_DEFLATED),
        ("xl/_rels/workbook.xml.rels", workbook_rels(1, True, True), zipfile.ZIP_DEFLATED),
        ("xl/sharedStrings.xml", shared_strings_xml(["Hello", "World"]), zipfile.ZIP_DEFLATED),
        ("xl/styles.xml", styles_xml(), zipfile.ZIP_DEFLATED),
        (
            "xl/worksheets/sheet1.xml",
            worksheet_xml(
                [
                    (1, ['<c r="A1" t="inlineStr"><is><t>Inline</t></is></c>']),
                    (
                        2,
                        [
                            '<c r="A2"><v>42</v></c>',
                            '<c r="B2" t="b"><v>1</v></c>',
                            '<c r="C2" t="e"><v>#DIV/0!</v></c>',
                        ],
                    ),
                    (
                        3,
                        [
                            '<c r="A3" t="s"><v>0</v></c>',
                            '<c r="B3"><f>SUM(A2:A2)</f><v>42</v></c>',
                            '<c r="C3" t="s" s="1"><v>1</v></c>',
                        ],
                    ),
                ],
                dimension="A1:C3",
                merges=["A1:B1"],
            ),
            zipfile.ZIP_DEFLATED,
        ),
    ]
    return members


def multi():
    members = [
        (
            "[Content_Types].xml",
            content_types(
                [
                    ("/xl/workbook.xml", CT_SHEET_MAIN),
                    ("/xl/sharedStrings.xml", CT_SST),
                    ("/xl/worksheets/sheet1.xml", CT_WS),
                    ("/xl/worksheets/sheet2.xml", CT_WS),
                    ("/xl/worksheets/sheet3.xml", CT_WS),
                ]
            ),
            zipfile.ZIP_STORED,
        ),
        ("_rels/.rels", package_rels(), zipfile.ZIP_DEFLATED),
        (
            "xl/workbook.xml",
            workbook_xml(
                [
                    ("First", 1, "rId1", None),
                    ("Second", 2, "rId2", None),
                    ("Hidden", 3, "rId3", "hidden"),
                ]
            ),
            zipfile.ZIP_DEFLATED,
        ),
        ("xl/_rels/workbook.xml.rels", workbook_rels(3, False, True), zipfile.ZIP_DEFLATED),
        ("xl/sharedStrings.xml", shared_strings_xml(["Alpha", "Beta", "Gamma"]), zipfile.ZIP_DEFLATED),
        ("xl/worksheets/sheet1.xml", worksheet_xml([(1, ['<c r="A1" t="s"><v>0</v></c>'])]), zipfile.ZIP_DEFLATED),
        ("xl/worksheets/sheet2.xml", worksheet_xml([(1, ['<c r="A1" t="s"><v>1</v></c>'])]), zipfile.ZIP_DEFLATED),
        ("xl/worksheets/sheet3.xml", worksheet_xml([(1, ['<c r="A1" t="s"><v>2</v></c>'])]), zipfile.ZIP_DEFLATED),
    ]
    return members


def semantic():
    """Phase 21.1.2: styles + number formats, merges, a comment (+VML), internal
    and external hyperlinks, a defined name, a table, a drawing/chart/media graph,
    an external workbook link, and a formula whose cached value differs from its
    number-format rendering. Byte-deterministic."""
    sheet = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<worksheet xmlns="{SSML}" xmlns:r="{OD_REL}">'
        '<dimension ref="A1:C5"/>'
        "<sheetData>"
        '<row r="1">'
        '<c r="A1" s="1"><v>1234.5</v></c>'
        '<c r="B1" s="3"><v>0.5</v></c>'
        '<c r="C1" s="2"><v>7</v></c>'
        "</row>"
        '<row r="2">'
        '<c r="A2" t="inlineStr"><is><t>Total</t></is></c>'
        '<c r="B2" s="1"><f>A1*2</f><v>2469</v></c>'
        "</row>"
        '<row r="3"><c r="A3"><v>3</v></c><c r="B3"><v>4</v></c><c r="C3"><v>5</v></c></row>'
        '<row r="5"><c r="A5" t="inlineStr"><is><t>Merged</t></is></c></row>'
        "</sheetData>"
        '<mergeCells count="1"><mergeCell ref="A5:B5"/></mergeCells>'
        "<hyperlinks>"
        '<hyperlink ref="A1" r:id="rId1" display="example"/>'
        '<hyperlink ref="A2" location="Sheet1!C1" display="go"/>'
        "</hyperlinks>"
        '<drawing r:id="rId5"/>'
        '<legacyDrawing r:id="rId3"/>'
        '<tableParts count="1"><tablePart r:id="rId4"/></tableParts>'
        "</worksheet>"
    ).encode("utf-8")
    sheet_rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<Relationships xmlns="{REL_NS}">'
        f'<Relationship Id="rId1" Type="{OD_REL}/hyperlink" Target="https://example.com/" TargetMode="External"/>'
        f'<Relationship Id="rId2" Type="{OD_REL}/comments" Target="../comments1.xml"/>'
        f'<Relationship Id="rId3" Type="{OD_REL}/vmlDrawing" Target="../drawings/vmlDrawing1.vml"/>'
        f'<Relationship Id="rId4" Type="{OD_REL}/table" Target="../tables/table1.xml"/>'
        f'<Relationship Id="rId5" Type="{OD_REL}/drawing" Target="../drawings/drawing1.xml"/>'
        "</Relationships>"
    ).encode("utf-8")
    workbook = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<workbook xmlns="{SSML}" xmlns:r="{OD_REL}">'
        '<sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets>'
        "<definedNames>"
        '<definedName name="TaxRate" localSheetId="0">Data!$C$1</definedName>'
        '<definedName name="HiddenName" hidden="1">Data!$A$1</definedName>'
        "</definedNames>"
        "</workbook>"
    ).encode("utf-8")
    workbook_rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<Relationships xmlns="{REL_NS}">'
        f'<Relationship Id="rId1" Type="{OD_REL}/worksheet" Target="worksheets/sheet1.xml"/>'
        f'<Relationship Id="rId2" Type="{OD_REL}/styles" Target="styles.xml"/>'
        f'<Relationship Id="rId3" Type="{OD_REL}/externalLink" Target="externalLinks/externalLink1.xml"/>'
        "</Relationships>"
    ).encode("utf-8")
    styles = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<styleSheet xmlns="{SSML}">'
        '<numFmts count="2"><numFmt numFmtId="164" formatCode="0.00"/>'
        '<numFmt numFmtId="165" formatCode="0%"/></numFmts>'
        '<fonts count="3">'
        '<font><sz val="11"/><name val="Calibri"/></font>'
        '<font><b/><sz val="14"/><name val="Arial"/></font>'
        '<font><b val="0"/><i/><sz val="10"/><name val="Courier New"/></font>'
        "</fonts>"
        '<fills count="2"><fill><patternFill patternType="none"/></fill>'
        '<fill><patternFill patternType="solid"><fgColor rgb="FFFF0000"/><bgColor indexed="64"/></patternFill></fill></fills>'
        '<borders count="1"><border/></borders>'
        '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0"/></cellStyleXfs>'
        '<cellXfs count="4">'
        '<xf numFmtId="0" fontId="0" fillId="0"/>'
        '<xf numFmtId="164" fontId="1" fillId="0"/>'
        '<xf numFmtId="164" fontId="2" fillId="1" applyAlignment="1">'
        '<alignment horizontal="center" vertical="top" wrapText="1"/></xf>'
        '<xf numFmtId="165" fontId="0" fillId="0"/>'
        "</cellXfs>"
        '<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles>'
        "</styleSheet>"
    ).encode("utf-8")
    comments = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<comments xmlns="{SSML}"><authors><author>Alice</author></authors>'
        '<commentList><comment ref="B2" authorId="0"><text><r><t>Check this</t></r></text></comment></commentList>'
        "</comments>"
    ).encode("utf-8")
    vml = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<xml xmlns:v="{VML_NS}" xmlns:x="{EXCEL_NS}">'
        '<v:shape id="_x0000_s1025" type="#_x0000_t202" style="visibility:hidden">'
        '<v:textbox><div/></v:textbox>'
        '<x:ClientData ObjectType="Note"><x:MoveWithCells/><x:SizeWithCells/>'
        '<x:Anchor>1, 15, 0, 2, 2, 79, 4, 8</x:Anchor><x:Row>1</x:Row><x:Column>1</x:Column>'
        "</x:ClientData></v:shape></xml>"
    ).encode("utf-8")
    table = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<table xmlns="{SSML}" id="1" name="Table1" displayName="Table1" ref="A1:C3">'
        '<autoFilter ref="A1:C3"/>'
        '<tableColumns count="3"><tableColumn id="1" name="One"/>'
        '<tableColumn id="2" name="Two"/><tableColumn id="3" name="Three"/></tableColumns>'
        '<tableStyleInfo name="TableStyleMedium2" showRowStripes="1"/>'
        "</table>"
    ).encode("utf-8")
    drawing = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<xdr:wsDr xmlns:xdr="{XDR_NS}" xmlns:a="{A_NS}" xmlns:c="{C_NS}" xmlns:r="{OD_REL}">'
        '<xdr:twoCellAnchor><xdr:graphicFrame><a:graphic><a:graphicData>'
        '<c:chart r:id="rId1"/></a:graphicData></a:graphic></xdr:graphicFrame></xdr:twoCellAnchor>'
        '<xdr:oneCellAnchor><xdr:pic><xdr:blipFill><a:blip r:embed="rId2"/>'
        "</xdr:blipFill></xdr:pic></xdr:oneCellAnchor></xdr:wsDr>"
    ).encode("utf-8")
    drawing_rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<Relationships xmlns="{REL_NS}">'
        f'<Relationship Id="rId1" Type="{OD_REL}/chart" Target="../charts/chart1.xml"/>'
        f'<Relationship Id="rId2" Type="{OD_REL}/image" Target="../media/image1.png"/>'
        "</Relationships>"
    ).encode("utf-8")
    chart = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<c:chartSpace xmlns:c="{C_NS}"><c:chart/></c:chartSpace>'
    ).encode("utf-8")
    ext_link = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<externalLink xmlns="{SSML}" xmlns:r="{OD_REL}">'
        '<externalBook r:id="rId1"><sheetNames><sheetName val="Other"/></sheetNames></externalBook>'
        "</externalLink>"
    ).encode("utf-8")
    ext_link_rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<Relationships xmlns="{REL_NS}">'
        f'<Relationship Id="rId1" Type="{OD_REL}/externalLinkPath" '
        'Target="file:///C:/tmp/other.xlsx" TargetMode="External"/>'
        "</Relationships>"
    ).encode("utf-8")
    members = [
        (
            "[Content_Types].xml",
            content_types_custom(
                [("vml", CT_VML), ("png", CT_PNG)],
                [
                    ("/xl/workbook.xml", CT_SHEET_MAIN),
                    ("/xl/styles.xml", CT_STYLES),
                    ("/xl/worksheets/sheet1.xml", CT_WS),
                    ("/xl/comments1.xml", CT_COMMENTS),
                    ("/xl/drawings/drawing1.xml", CT_DRAWING),
                    ("/xl/charts/chart1.xml", CT_CHART),
                    ("/xl/tables/table1.xml", CT_TABLE),
                    ("/xl/externalLinks/externalLink1.xml", CT_EXT_LINK),
                ],
            ),
            zipfile.ZIP_STORED,
        ),
        ("_rels/.rels", package_rels(), zipfile.ZIP_DEFLATED),
        ("xl/workbook.xml", workbook, zipfile.ZIP_DEFLATED),
        ("xl/_rels/workbook.xml.rels", workbook_rels, zipfile.ZIP_DEFLATED),
        ("xl/styles.xml", styles, zipfile.ZIP_DEFLATED),
        ("xl/worksheets/sheet1.xml", sheet, zipfile.ZIP_DEFLATED),
        ("xl/worksheets/_rels/sheet1.xml.rels", sheet_rels, zipfile.ZIP_DEFLATED),
        ("xl/comments1.xml", comments, zipfile.ZIP_DEFLATED),
        ("xl/drawings/vmlDrawing1.vml", vml, zipfile.ZIP_DEFLATED),
        ("xl/drawings/drawing1.xml", drawing, zipfile.ZIP_DEFLATED),
        ("xl/drawings/_rels/drawing1.xml.rels", drawing_rels, zipfile.ZIP_DEFLATED),
        ("xl/charts/chart1.xml", chart, zipfile.ZIP_DEFLATED),
        ("xl/tables/table1.xml", table, zipfile.ZIP_DEFLATED),
        ("xl/media/image1.png", PNG_1X1, zipfile.ZIP_STORED),
        ("xl/externalLinks/externalLink1.xml", ext_link, zipfile.ZIP_DEFLATED),
        ("xl/externalLinks/_rels/externalLink1.xml.rels", ext_link_rels, zipfile.ZIP_DEFLATED),
    ]
    return members


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    out = os.path.join(here, "xlsx")
    os.makedirs(out, exist_ok=True)
    write_xlsx(os.path.join(out, "single.xlsx"), single())
    write_xlsx(os.path.join(out, "multi.xlsx"), multi())
    write_xlsx(os.path.join(out, "semantic.xlsx"), semantic())
    for name in ("single.xlsx", "multi.xlsx", "semantic.xlsx"):
        p = os.path.join(out, name)
        print(f"{name}\t{os.path.getsize(p)} bytes")


if __name__ == "__main__":
    main()
