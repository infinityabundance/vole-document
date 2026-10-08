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

FIXED_TIME = (2026, 1, 1, 0, 0, 0)


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


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    out = os.path.join(here, "xlsx")
    os.makedirs(out, exist_ok=True)
    write_xlsx(os.path.join(out, "single.xlsx"), single())
    write_xlsx(os.path.join(out, "multi.xlsx"), multi())
    for name in ("single.xlsx", "multi.xlsx"):
        p = os.path.join(out, name)
        print(f"{name}\t{os.path.getsize(p)} bytes")


if __name__ == "__main__":
    main()
