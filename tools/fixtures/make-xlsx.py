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


# ===========================================================================
# Phase 21.1.3 — deterministic self-authored *corpus* mode (`--corpus DIR`).
#
# Emits a small corpus of varied, realistic-shape XLSX workbooks so the
# economic court has more than three tiny fixtures to measure build/storage/
# query cost on. Every workbook is byte-deterministic (fixed member order,
# fixed `ZipInfo.date_time`, fixed per-member compression method); the numeric
# content uses a fixed-seed LCG (never `random`), so regeneration is
# reproducible. `zipfile` + hand-written SpreadsheetML XML only.
#
# The corpus is deliberately NOT a real-world population: it is self-authored
# and small, and every claim the court makes is scoped to it.
# ===========================================================================


def esc(text):
    return (
        str(text)
        .replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace('"', "&quot;")
    )


def col_letter(c):
    """0-based column index -> A1 column letters (A, B, ..., Z, AA, ...)."""
    s = ""
    c += 1
    while c:
        c, r = divmod(c - 1, 26)
        s = chr(65 + r) + s
    return s


def _a1(ref):
    """Return (row0, col0) for an A1 reference (no range semantics)."""
    i = 0
    while i < len(ref) and ref[i].isalpha():
        i += 1
    col = 0
    for ch in ref[:i]:
        col = col * 26 + (ord(ch.upper()) - 64)
    return int(ref[i:]) - 1, col - 1


def _lcg(seed):
    """A deterministic 31-bit LCG (no `random`), for reproducible content."""
    state = [seed & 0x7FFFFFFF]

    def nxt():
        state[0] = (1103515245 * state[0] + 12345) & 0x7FFFFFFF
        return state[0]

    return nxt


# --- cell XML helpers -------------------------------------------------------


def _cell_inline(ref, text):
    return f'<c r="{ref}" t="inlineStr"><is><t>{esc(text)}</t></is></c>'


def _cell_sst(ref, idx, s=None):
    sa = f' s="{s}"' if s is not None else ""
    return f'<c r="{ref}" t="s"{sa}><v>{idx}</v></c>'


def _cell_num(ref, val, s=None):
    sa = f' s="{s}"' if s is not None else ""
    return f'<c r="{ref}"{sa}><v>{val}</v></c>'


def _cell_bool(ref, v, s=None):
    sa = f' s="{s}"' if s is not None else ""
    return f'<c r="{ref}" t="b"{sa}><v>{1 if v else 0}</v></c>'


def _cell_err(ref, code, s=None):
    sa = f' s="{s}"' if s is not None else ""
    return f'<c r="{ref}" t="e"{sa}><v>{esc(code)}</v></c>'


def _cell_formula(ref, formula, cached, s=None):
    sa = f' s="{s}"' if s is not None else ""
    return f'<c r="{ref}"{sa}><f>{esc(formula)}</f><v>{cached}</v></c>'


# --- part XML helpers -------------------------------------------------------


def _rels_xml(rels):
    """rels: list of (id, type, target, external)."""
    items = []
    for rid, rtype, target, external in rels:
        mode = ' TargetMode="External"' if external else ""
        items.append(
            f'<Relationship Id="{rid}" Type="{rtype}" Target="{esc(target)}"{mode}/>'
        )
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<Relationships xmlns="{REL_NS}">' + "".join(items) + "</Relationships>"
    ).encode("utf-8")


def _font_xml(f):
    parts = []
    if f.get("bold"):
        parts.append("<b/>")
    if f.get("italic"):
        parts.append("<i/>")
    if f.get("size"):
        parts.append(f'<sz val="{esc(f["size"])}"/>')
    if f.get("name"):
        parts.append(f'<name val="{esc(f["name"])}"/>')
    return "<font>" + "".join(parts) + "</font>"


def _fill_xml(f):
    pattern = f.get("pattern", "none")
    inner = ""
    if f.get("fg"):
        inner += f'<fgColor rgb="{esc(f["fg"])}"/>'
    if f.get("bg"):
        inner += f'<bgColor rgb="{esc(f["bg"])}"/>'
    return f'<fill><patternFill patternType="{esc(pattern)}">{inner}</patternFill></fill>'


def _xf_xml(x):
    attrs = (
        f'numFmtId="{x.get("numFmtId", 0)}" fontId="{x.get("fontId", 0)}" '
        f'fillId="{x.get("fillId", 0)}"'
    )
    align = x.get("align")
    if align:
        aa = []
        if align.get("horizontal"):
            aa.append(f'horizontal="{esc(align["horizontal"])}"')
        if align.get("vertical"):
            aa.append(f'vertical="{esc(align["vertical"])}"')
        if align.get("wrapText"):
            aa.append('wrapText="1"')
        return f'<xf {attrs} applyAlignment="1"><alignment {" ".join(aa)}/></xf>'
    return f'<xf {attrs}/>'


def styles_xml_custom(num_fmts, fonts, fills, xfs):
    nf = "".join(f'<numFmt numFmtId="{i}" formatCode="{esc(c)}"/>' for i, c in num_fmts)
    fnts = "".join(_font_xml(f) for f in fonts)
    fls = "".join(_fill_xml(f) for f in fills)
    xf = "".join(_xf_xml(x) for x in xfs)
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<styleSheet xmlns="{SSML}">'
        f'<numFmts count="{len(num_fmts)}">{nf}</numFmts>'
        f'<fonts count="{len(fonts)}">{fnts}</fonts>'
        f'<fills count="{len(fills)}">{fls}</fills>'
        '<borders count="1"><border/></borders>'
        '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0"/></cellStyleXfs>'
        f'<cellXfs count="{len(xfs)}">{xf}</cellXfs>'
        '<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles>'
        "</styleSheet>"
    ).encode("utf-8")


def _comments_xml(comments):
    authors = []
    for c in comments:
        a = c.get("author") or ""
        if a not in authors:
            authors.append(a)
    items = []
    for c in comments:
        aid = authors.index(c.get("author") or "")
        items.append(
            f'<comment ref="{c["cell"]}" authorId="{aid}">'
            f'<text><r><t>{esc(c["text"])}</t></r></text></comment>'
        )
    au = "".join(f"<author>{esc(a)}</author>" for a in authors)
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<comments xmlns="{SSML}"><authors>{au}</authors>'
        f'<commentList>{"".join(items)}</commentList></comments>'
    ).encode("utf-8")


def _vml_xml(cell_ref):
    row, col = _a1(cell_ref)
    sid = 1025 + col
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<xml xmlns:v="{VML_NS}" xmlns:x="{EXCEL_NS}">'
        f'<v:shape id="_x0000_s{sid}" type="#_x0000_t202" style="visibility:hidden">'
        '<v:textbox><div/></v:textbox>'
        '<x:ClientData ObjectType="Note"><x:MoveWithCells/><x:SizeWithCells/>'
        '<x:Anchor>1, 15, 0, 2, 2, 79, 4, 8</x:Anchor>'
        f'<x:Row>{row}</x:Row><x:Column>{col}</x:Column>'
        "</x:ClientData></v:shape></xml>"
    ).encode("utf-8")


def _table_xml(t):
    cols = "".join(
        f'<tableColumn id="{i + 1}" name="{esc(n)}"/>' for i, n in enumerate(t["columns"])
    )
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<table xmlns="{SSML}" id="{t.get("id", 1)}" name="{esc(t["name"])}" '
        f'displayName="{esc(t.get("displayName", t["name"]))}" ref="{t["ref"]}">'
        f'<autoFilter ref="{t["ref"]}"/>'
        f'<tableColumns count="{len(t["columns"])}">{cols}</tableColumns>'
        '<tableStyleInfo name="TableStyleMedium2" showRowStripes="1"/></table>'
    ).encode("utf-8")


def _drawing_xml(d):
    parts = [
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>',
        f'<xdr:wsDr xmlns:xdr="{XDR_NS}" xmlns:a="{A_NS}" xmlns:c="{C_NS}" '
        f'xmlns:r="{OD_REL}">',
    ]
    if d.get("chart"):
        parts.append(
            '<xdr:twoCellAnchor><xdr:graphicFrame><a:graphic><a:graphicData>'
            '<c:chart r:id="rId1"/></a:graphicData></a:graphic></xdr:graphicFrame>'
            "</xdr:twoCellAnchor>"
        )
    if d.get("image"):
        parts.append(
            '<xdr:oneCellAnchor><xdr:pic><xdr:blipFill><a:blip r:embed="rId2"/>'
            "</xdr:blipFill></xdr:pic></xdr:oneCellAnchor>"
        )
    parts.append("</xdr:wsDr>")
    return "".join(parts).encode("utf-8")


def _chart_xml(ref):
    if ref:
        body = (
            "<c:chart><c:plotArea><c:barChart><c:ser>"
            '<c:idx val="0"/><c:order val="0"/>'
            f'<c:val><c:numRef><c:f>{esc(ref)}</c:f></c:numRef></c:val>'
            "</c:ser></c:barChart></c:plotArea></c:chart>"
        )
    else:
        body = "<c:chart/>"
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<c:chartSpace xmlns:c="{C_NS}">{body}</c:chartSpace>'
    ).encode("utf-8")


def _ext_link_xml():
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<externalLink xmlns="{SSML}" xmlns:r="{OD_REL}">'
        '<externalBook r:id="rId1"><sheetNames><sheetName val="Other"/></sheetNames></externalBook>'
        "</externalLink>"
    ).encode("utf-8")


def worksheet_full(dimension, rows, merges, hyperlinks, drawing_rid, legacy_rid, table_rids):
    dim = f'<dimension ref="{dimension}"/>' if dimension else ""
    body = "".join(f'<row r="{r}">{"".join(cells)}</row>' for r, cells in rows)
    mc = ""
    if merges:
        mc = (
            f'<mergeCells count="{len(merges)}">'
            + "".join(f'<mergeCell ref="{m}"/>' for m in merges)
            + "</mergeCells>"
        )
    hl = ""
    if hyperlinks:
        hl = "<hyperlinks>" + "".join(hyperlinks) + "</hyperlinks>"
    dr = f'<drawing r:id="{drawing_rid}"/>' if drawing_rid else ""
    lg = f'<legacyDrawing r:id="{legacy_rid}"/>' if legacy_rid else ""
    tp = ""
    if table_rids:
        tp = (
            f'<tableParts count="{len(table_rids)}">'
            + "".join(f'<tablePart r:id="{r}"/>' for r in table_rids)
            + "</tableParts>"
        )
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<worksheet xmlns="{SSML}" xmlns:r="{OD_REL}">'
        f'{dim}<sheetData>{body}</sheetData>{mc}{hl}{dr}{lg}{tp}</worksheet>'
    ).encode("utf-8")


def corpus_workbook(spec):
    """Assemble one deterministic corpus workbook's ordered member list."""
    sst = spec.get("sst") or []
    styles_bytes = spec.get("styles")
    defined_names = spec.get("defined_names") or []
    sheets = spec["sheets"]

    defaults = []
    if any(s.get("comment") for s in sheets):
        defaults.append(("vml", CT_VML))
    if any((s.get("drawing") or {}).get("image") for s in sheets):
        defaults.append(("png", CT_PNG))
    overrides = []

    # workbook relationships (sheets, then styles, sharedStrings, externalLink)
    wb_rels = []
    sheet_rel_ids = []
    for i in range(len(sheets)):
        rid = f"rId{i + 1}"
        sheet_rel_ids.append(rid)
        wb_rels.append((rid, OD_REL + "/worksheet", f"worksheets/sheet{i + 1}.xml", False))
    nxt = len(sheets) + 1
    if styles_bytes is not None:
        wb_rels.append((f"rId{nxt}", OD_REL + "/styles", "styles.xml", False))
        nxt += 1
    if sst:
        wb_rels.append((f"rId{nxt}", OD_REL + "/sharedStrings", "sharedStrings.xml", False))
        nxt += 1
    if spec.get("ext_link"):
        wb_rels.append(
            (f"rId{nxt}", OD_REL + "/externalLink", "externalLinks/externalLink1.xml", False)
        )
        nxt += 1

    wb_sheets = []
    for i, (s, rid) in enumerate(zip(sheets, sheet_rel_ids)):
        state = s.get("state")
        attr = f' state="{state}"' if state and state != "visible" else ""
        sid = s.get("sheetId", i + 1)
        wb_sheets.append(
            f'<sheet name="{esc(s["name"])}" sheetId="{sid}" r:id="{rid}"{attr}/>'
        )
    dn = ""
    if defined_names:
        parts = []
        for d in defined_names:
            ls = (
                f' localSheetId="{d["localSheetId"]}"'
                if d.get("localSheetId") is not None
                else ""
            )
            hd = ' hidden="1"' if d.get("hidden") else ""
            parts.append(
                f'<definedName name="{esc(d["name"])}"{ls}{hd}>{esc(d["refersTo"])}</definedName>'
            )
        dn = f'<definedNames>{"".join(parts)}</definedNames>'
    workbook = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<workbook xmlns="{SSML}" xmlns:r="{OD_REL}">'
        f'<sheets>{"".join(wb_sheets)}</sheets>{dn}</workbook>'
    ).encode("utf-8")
    overrides.append(("/xl/workbook.xml", CT_SHEET_MAIN))

    parts = [
        ("_rels/.rels", package_rels(), zipfile.ZIP_DEFLATED),
        ("xl/workbook.xml", workbook, zipfile.ZIP_DEFLATED),
        ("xl/_rels/workbook.xml.rels", _rels_xml(wb_rels), zipfile.ZIP_DEFLATED),
    ]
    if styles_bytes is not None:
        parts.append(("xl/styles.xml", styles_bytes, zipfile.ZIP_DEFLATED))
        overrides.append(("/xl/styles.xml", CT_STYLES))
    if sst:
        parts.append(("xl/sharedStrings.xml", shared_strings_xml(sst), zipfile.ZIP_DEFLATED))
        overrides.append(("/xl/sharedStrings.xml", CT_SST))

    for i, s in enumerate(sheets):
        srel = []
        hrid = 1
        hyper_xml = []
        for h in s.get("hyperlinks", []):
            if h.get("external"):
                rid = f"rId{hrid}"
                hrid += 1
                srel.append((rid, OD_REL + "/hyperlink", h["target"], True))
                hyper_xml.append(
                    f'<hyperlink ref="{h["ref"]}" r:id="{rid}" display="{esc(h.get("display", ""))}"/>'
                )
            else:
                hyper_xml.append(
                    f'<hyperlink ref="{h["ref"]}" location="{esc(h["location"])}" '
                    f'display="{esc(h.get("display", ""))}"/>'
                )
        legacy_rid = None
        drawing_rid = None
        table_rids = []
        comments = s.get("comment")
        if comments:
            rid = f"rId{hrid}"
            hrid += 1
            srel.append((rid, OD_REL + "/comments", f"../comments{i + 1}.xml", False))
            parts.append((f"xl/comments{i + 1}.xml", _comments_xml(comments), zipfile.ZIP_DEFLATED))
            overrides.append((f"/xl/comments{i + 1}.xml", CT_COMMENTS))
            vrid = f"rId{hrid}"
            hrid += 1
            srel.append((vrid, OD_REL + "/vmlDrawing", f"../drawings/vmlDrawing{i + 1}.vml", False))
            parts.append(
                (f"xl/drawings/vmlDrawing{i + 1}.vml", _vml_xml(comments[0]["cell"]), zipfile.ZIP_DEFLATED)
            )
            legacy_rid = vrid
        table = s.get("table")
        if table:
            rid = f"rId{hrid}"
            hrid += 1
            srel.append((rid, OD_REL + "/table", f"../tables/table{i + 1}.xml", False))
            parts.append((f"xl/tables/table{i + 1}.xml", _table_xml(table), zipfile.ZIP_DEFLATED))
            overrides.append((f"/xl/tables/table{i + 1}.xml", CT_TABLE))
            table_rids.append(rid)
        drawing = s.get("drawing")
        if drawing:
            rid = f"rId{hrid}"
            hrid += 1
            srel.append((rid, OD_REL + "/drawing", f"../drawings/drawing{i + 1}.xml", False))
            drawing_rid = rid
            parts.append(
                (f"xl/drawings/drawing{i + 1}.xml", _drawing_xml(drawing), zipfile.ZIP_DEFLATED)
            )
            overrides.append((f"/xl/drawings/drawing{i + 1}.xml", CT_DRAWING))
            drels = []
            if drawing.get("chart"):
                drels.append(("rId1", OD_REL + "/chart", f"../charts/chart{i + 1}.xml", False))
                parts.append(
                    (
                        f"xl/charts/chart{i + 1}.xml",
                        _chart_xml(drawing.get("chart_ref")),
                        zipfile.ZIP_DEFLATED,
                    )
                )
                overrides.append((f"/xl/charts/chart{i + 1}.xml", CT_CHART))
            if drawing.get("image"):
                drels.append(("rId2", OD_REL + "/image", f"../media/image{i + 1}.png", False))
                parts.append((f"xl/media/image{i + 1}.png", PNG_1X1, zipfile.ZIP_STORED))
            parts.append(
                (
                    f"xl/drawings/_rels/drawing{i + 1}.xml.rels",
                    _rels_xml(drels),
                    zipfile.ZIP_DEFLATED,
                )
            )
        sheet_xml = worksheet_full(
            s.get("dimension"),
            s["rows"],
            s.get("merges", []),
            hyper_xml,
            drawing_rid,
            legacy_rid,
            table_rids,
        )
        parts.append((f"xl/worksheets/sheet{i + 1}.xml", sheet_xml, zipfile.ZIP_DEFLATED))
        overrides.append((f"/xl/worksheets/sheet{i + 1}.xml", CT_WS))
        if srel:
            parts.append(
                (
                    f"xl/worksheets/_rels/sheet{i + 1}.xml.rels",
                    _rels_xml(srel),
                    zipfile.ZIP_DEFLATED,
                )
            )

    if spec.get("ext_link"):
        parts.append(("xl/externalLinks/externalLink1.xml", _ext_link_xml(), zipfile.ZIP_DEFLATED))
        parts.append(
            (
                "xl/externalLinks/_rels/externalLink1.xml.rels",
                _rels_xml([("rId1", OD_REL + "/externalLinkPath", "file:///C:/tmp/other.xlsx", True)]),
                zipfile.ZIP_DEFLATED,
            )
        )
        overrides.append(("/xl/externalLinks/externalLink1.xml", CT_EXT_LINK))

    members = [("[Content_Types].xml", content_types_custom(defaults, overrides), zipfile.ZIP_STORED)]
    members.extend(parts)
    return members


# --- shared style tables ----------------------------------------------------

BASE_FONTS = [
    {"bold": False, "italic": False, "size": "11", "name": "Calibri"},
    {"bold": True, "italic": False, "size": "14", "name": "Arial"},
    {"bold": False, "italic": True, "size": "10", "name": "Courier New"},
    {"bold": True, "italic": True, "size": "12", "name": "Georgia"},
]
BASE_FILLS = [
    {"pattern": "none"},
    {"pattern": "gray125"},
    {"pattern": "solid", "fg": "FFFF0000", "bg": "FF000000"},
    {"pattern": "solid", "fg": "FF00CC00", "bg": "FF000000"},
]
BASE_NUMFMTS = [(164, "0.00"), (165, "0%"), (166, "#,##0"), (167, "yyyy-mm-dd")]
BASE_XFS = [
    {"numFmtId": 0, "fontId": 0, "fillId": 0},
    {"numFmtId": 164, "fontId": 1, "fillId": 0},
    {"numFmtId": 164, "fontId": 2, "fillId": 2, "align": {"horizontal": "center", "vertical": "top", "wrapText": True}},
    {"numFmtId": 165, "fontId": 0, "fillId": 0},
    {"numFmtId": 166, "fontId": 3, "fillId": 3, "align": {"horizontal": "right"}},
]
STYLES_RICH = styles_xml_custom(BASE_NUMFMTS, BASE_FONTS, BASE_FILLS, BASE_XFS)


def wb_basic():
    sst = ["Item", "Qty", "Price", "Total", "Notes", "Alpha", "Beta", "Gamma", "Delta"]
    rows = [(1, [_cell_sst(f"{col_letter(c)}1", c, 1) for c in range(5)])]
    for r in range(2, 13):
        item = 5 + ((r - 2) % 4)
        qty = (r - 1) * 3
        price = 100 + (r - 2) * 7
        rows.append(
            (
                r,
                [
                    _cell_sst(f"A{r}", item),
                    _cell_num(f"B{r}", qty),
                    _cell_num(f"C{r}", price, 1),
                    _cell_formula(f"D{r}", f"B{r}*C{r}", qty * price, 1),
                    _cell_inline(f"E{r}", f"note-{r}"),
                ],
            )
        )
    rows.append((13, [_cell_inline("A13", "Merged"), _cell_bool("C13", True), _cell_err("D13", "#DIV/0!")]))
    return {
        "sst": sst,
        "styles": STYLES_RICH,
        "defined_names": [
            {"name": "TaxRate", "localSheetId": 0, "refersTo": "Data!$C$1"},
            {"name": "HiddenName", "hidden": True, "refersTo": "Data!$A$1"},
        ],
        "sheets": [
            {
                "name": "Data",
                "dimension": "A1:E13",
                "rows": rows,
                "merges": ["A13:B13"],
                "hyperlinks": [
                    {"ref": "A1", "external": True, "target": "https://example.com/", "display": "example"},
                    {"ref": "A2", "external": False, "location": "Data!C1", "display": "go"},
                ],
                "comment": [{"cell": "B2", "author": "Alice", "text": "Check this"}],
            }
        ],
    }


def wb_shared_table():
    nxt = _lcg(20211)
    sst = [f"cat-{i:02d}" for i in range(24)]
    rows = [(1, [_cell_sst(f"{col_letter(c)}1", c, 1) for c in range(8)])]
    for r in range(2, 201):
        cells = [_cell_sst(f"A{r}", nxt() % 24)]
        for c in range(1, 7):
            cells.append(_cell_num(f"{col_letter(c)}{r}", (nxt() % 100000) / 100.0, 1))
        cells.append(_cell_formula(f"H{r}", f"B{r}+C{r}+D{r}", "0", 1))
        rows.append((r, cells))
    return {
        "sst": sst,
        "styles": STYLES_RICH,
        "ext_link": True,
        "sheets": [
            {
                "name": "Data",
                "dimension": "A1:H200",
                "rows": rows,
                "table": {
                    "name": "Table1",
                    "displayName": "Table1",
                    "ref": "A1:H200",
                    "columns": [col_letter(c) for c in range(8)],
                },
                "drawing": {"chart": True, "image": True, "chart_ref": "Data!$A$1:$H$200"},
                "hyperlinks": [
                    {"ref": "A1", "external": True, "target": "https://example.org/data", "display": "src"}
                ],
            }
        ],
    }


def wb_multisheet():
    nxt = _lcg(2103)
    sst = ["Alpha", "Beta", "Gamma", "Delta", "Epsilon"]

    def sheet(name, nrows, ncols, table=None, comment=None, link=None):
        rows = [(1, [_cell_sst(f"{col_letter(c)}1", c, 1) for c in range(ncols)])]
        for r in range(2, nrows + 1):
            cells = []
            for c in range(ncols):
                if c == 0:
                    cells.append(_cell_sst(f"A{r}", nxt() % 5))
                else:
                    cells.append(_cell_num(f"{col_letter(c)}{r}", nxt() % 1000, 1 if c % 2 else 0))
            rows.append((r, cells))
        s = {
            "name": name,
            "dimension": f"A1:{col_letter(ncols - 1)}{nrows}",
            "rows": rows,
        }
        if table:
            s["table"] = {
                "name": table,
                "displayName": table,
                "ref": f"A1:{col_letter(ncols - 1)}{nrows}",
                "columns": [col_letter(c) for c in range(ncols)],
            }
        if comment:
            s["comment"] = comment
        if link:
            s["hyperlinks"] = link
        return s

    return {
        "sst": sst,
        "styles": STYLES_RICH,
        "defined_names": [
            {"name": "FirstCell", "localSheetId": 0, "refersTo": "First!$A$1"},
            {"name": "GlobalHidden", "hidden": True, "refersTo": "Second!$B$2"},
        ],
        "sheets": [
            sheet(
                "First",
                30,
                4,
                table="FirstTable",
                link=[{"ref": "A1", "external": True, "target": "https://example.net/", "display": "net"}],
            ),
            sheet("Second", 20, 4, comment=[{"cell": "B2", "author": "Bob", "text": "second sheet"}]),
            {
                "name": "Hidden",
                "state": "veryHidden",
                "sheetId": 3,
                "dimension": "A1:C10",
                "rows": [(r, [_cell_inline(f"A{r}", f"h{r}")]) for r in range(1, 11)],
            },
        ],
    }


def wb_styles():
    nxt = _lcg(2104)
    rows = [(1, [_cell_inline(f"{col_letter(c)}1", f"col{c}") for c in range(6)])]
    for r in range(2, 31):
        cells = []
        for c in range(6):
            style = c % 5
            if c == 2:
                cells.append(_cell_num(f"C{r}", nxt() % 100, style))
            elif c == 3:
                cells.append(_cell_formula(f"D{r}", f"C{r}*2", (nxt() % 100) * 2, style))
            else:
                cells.append(_cell_num(f"{col_letter(c)}{r}", (nxt() % 100000) / 100.0, style))
        rows.append((r, cells))
    return {
        "styles": STYLES_RICH,
        "sheets": [{"name": "Data", "dimension": "A1:F30", "rows": rows}],
    }


def wb_wide():
    nxt = _lcg(2105)
    ncols = 60
    nrows = 40
    rows = []
    for r in range(1, nrows + 1):
        rows.append((r, [_cell_num(f"{col_letter(c)}{r}", nxt() % 10000, 0) for c in range(ncols)]))
    rows.append((nrows + 1, [_cell_formula(f"{col_letter(c)}{nrows + 1}", f"SUM({col_letter(c)}1:{col_letter(c)}{nrows})", "0") for c in range(ncols)]))
    last = col_letter(ncols - 1)
    return {
        "styles": STYLES_RICH,
        "sheets": [
            {
                "name": "Grid",
                "dimension": f"A1:{last}{nrows + 1}",
                "rows": rows,
                "table": {
                    "name": "GridTable",
                    "displayName": "GridTable",
                    "ref": f"A1:{last}{nrows}",
                    "columns": [col_letter(c) for c in range(ncols)],
                },
                "drawing": {"chart": True, "chart_ref": f"Grid!$A$1:${col_letter(1)}${nrows}"},
            }
        ],
    }


def wb_large():
    nxt = _lcg(2106)
    ncols = 12
    nrows = 6000
    rows = []
    for r in range(1, nrows + 1):
        cells = [_cell_num(f"A{r}", r, 0)]
        for c in range(1, ncols):
            cells.append(_cell_num(f"{col_letter(c)}{r}", nxt() % 100000000, 0))
        rows.append((r, cells))
    last = col_letter(ncols - 1)
    return {
        "styles": STYLES_RICH,
        "sheets": [
            {
                "name": "Big",
                "dimension": f"A1:{last}{nrows}",
                "rows": rows,
                "table": {
                    "name": "BigTable",
                    "displayName": "BigTable",
                    "ref": f"A1:{last}{nrows}",
                    "columns": [col_letter(c) for c in range(ncols)],
                },
            }
        ],
    }


def wb_merges():
    rows = [(1, [_cell_inline("A1", "MergedHeader")])]
    for r in range(2, 21):
        cells = [_cell_inline(f"A{r}", f"row-{r}")]
        cells.append(_cell_num(f"B{r}", r * 2))
        cells.append(_cell_err(f"C{r}", "#VALUE!") if r % 5 == 0 else _cell_num(f"C{r}", r * 3)) 
        cells.append(_cell_bool(f"D{r}", r % 2 == 0))
        rows.append((r, cells))
    return {
        "styles": STYLES_RICH,
        "sheets": [
            {
                "name": "Data",
                "dimension": "A1:D20",
                "rows": rows,
                "merges": ["A1:D1", "A3:C3", "A5:C5", "A7:C7"],
                "comment": [
                    {"cell": "B2", "author": "Alice", "text": "first note"},
                    {"cell": "B3", "author": "Bob", "text": "second note"},
                ],
                "hyperlinks": [
                    {"ref": "A2", "external": False, "location": "Data!B2", "display": "jump"}
                ],
            }
        ],
    }


def wb_external():
    rows = [(1, [_cell_sst(f"{col_letter(c)}1", c, 1) for c in range(3)])]
    for r in range(2, 11):
        rows.append((r, [_cell_sst(f"A{r}", r % 3), _cell_num(f"B{r}", r * 10, 1), _cell_num(f"C{r}", r * 100, 1)]))
    return {
        "sst": ["X", "Y", "Z"],
        "styles": STYLES_RICH,
        "ext_link": True,
        "defined_names": [{"name": "Rate", "localSheetId": 0, "refersTo": "Data!$C$1"}],
        "sheets": [
            {
                "name": "Data",
                "dimension": "A1:C10",
                "rows": rows,
                "table": {"name": "ExtTable", "displayName": "ExtTable", "ref": "A1:C5", "columns": ["X", "Y", "Z"]},
                "drawing": {"chart": True, "chart_ref": "Data!$A$1:$C$5"},
                "hyperlinks": [
                    {"ref": "A1", "external": True, "target": "https://example.com/ext", "display": "ext"}
                ],
            }
        ],
    }


CORPUS = [
    ("c01-basic.xlsx", wb_basic),
    ("c02-shared-table.xlsx", wb_shared_table),
    ("c03-multisheet.xlsx", wb_multisheet),
    ("c04-styles.xlsx", wb_styles),
    ("c05-wide.xlsx", wb_wide),
    ("c06-large.xlsx", wb_large),
    ("c07-merges.xlsx", wb_merges),
    ("c08-external.xlsx", wb_external),
]


def emit_corpus(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    manifest = []
    for name, fn in CORPUS:
        path = os.path.join(out_dir, name)
        write_xlsx(path, corpus_workbook(fn()))
        with open(path, "rb") as f:
            data = f.read()
        import hashlib

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
    out = os.path.join(here, "xlsx")
    os.makedirs(out, exist_ok=True)
    write_xlsx(os.path.join(out, "single.xlsx"), single())
    write_xlsx(os.path.join(out, "multi.xlsx"), multi())
    write_xlsx(os.path.join(out, "semantic.xlsx"), semantic())
    for name in ("single.xlsx", "multi.xlsx", "semantic.xlsx"):
        p = os.path.join(out, name)
        print(f"{name}\t{os.path.getsize(p)} bytes")
    return 0


if __name__ == "__main__":
    main()
