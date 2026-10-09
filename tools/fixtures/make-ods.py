#!/usr/bin/env python3
"""Phase 21.3.1 — deterministic, self-authored ODS fixture generator.

Python **stdlib only** (`zipfile` + hand-written OpenDocument XML); no `odfpy`
and no external tool. Output is byte-deterministic: fixed member order (the
mandatory stored `mimetype` first), fixed `ZipInfo.date_time`, fixed compression
method per member. The generated `.ods` bytes are committed under
`tools/fixtures/ods/` and consumed by `tests/ods_adapter.rs` and
`tools/phase21-3-ods-court.sh`.

Nothing here is a conformance claim: each fixture exists to exercise one declared
part of the adapter (a basic multi-cell sheet, multi-sheet order/visibility,
formulas + typed values + repeated cells/rows, merged cells, named expressions,
cell comments, a styles/automatic-styles fixture, and a large
`table:number-rows-repeated` bomb to exercise the expansion bound).
"""

import os
import zipfile

MIMETYPE = "application/vnd.oasis.opendocument.spreadsheet"

OFFICE_NS = "urn:oasis:names:tc:opendocument:xmlns:office:1.0"
TEXT_NS = "urn:oasis:names:tc:opendocument:xmlns:text:1.0"
TABLE_NS = "urn:oasis:names:tc:opendocument:xmlns:table:1.0"
STYLE_NS = "urn:oasis:names:tc:opendocument:xmlns:style:1.0"
FO_NS = "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0"
NUMBER_NS = "urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0"
DC_NS = "http://purl.org/dc/elements/1.1/"
MANIFEST_NS = "urn:oasis:names:tc:opendocument:xmlns:manifest:1.0"

FIXED_TIME = (2026, 1, 1, 0, 0, 0)


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
    f'xmlns:text="{TEXT_NS}" xmlns:table="{TABLE_NS}" '
    f'xmlns:style="{STYLE_NS}" xmlns:fo="{FO_NS}" '
    f'xmlns:number="{NUMBER_NS}" xmlns:dc="{DC_NS}" office:version="1.2">'
)


def content_xml(automatic_styles, spreadsheet):
    return (
        CONTENT_HEADER
        + f"<office:automatic-styles>{automatic_styles}</office:automatic-styles>"
        + "<office:body>"
        + f"<office:spreadsheet>{spreadsheet}</office:spreadsheet>"
        + "</office:body>"
        + "</office:document-content>"
    ).encode("utf-8")


def styles_xml(styles="", automatic_styles=""):
    return (
        '<?xml version="1.0" encoding="UTF-8"?>'
        f'<office:document-styles xmlns:office="{OFFICE_NS}" '
        f'xmlns:text="{TEXT_NS}" xmlns:table="{TABLE_NS}" '
        f'xmlns:style="{STYLE_NS}" xmlns:fo="{FO_NS}" '
        f'xmlns:number="{NUMBER_NS}" office:version="1.2">'
        f"<office:styles>{styles}</office:styles>"
        f"<office:automatic-styles>{automatic_styles}</office:automatic-styles>"
        "</office:document-styles>"
    ).encode("utf-8")


def meta_xml():
    return (
        '<?xml version="1.0" encoding="UTF-8"?>'
        f'<office:document-meta xmlns:office="{OFFICE_NS}" xmlns:dc="{DC_NS}">'
        "<office:meta><dc:title>vole-ods-fixture</dc:title></office:meta>"
        "</office:document-meta>"
    ).encode("utf-8")


def write_ods(path, content, styles, extra_manifest=()):
    """Write one deterministic ODS: stored `mimetype` first, then manifest, then
    content/styles/meta."""
    if os.path.exists(path):
        os.remove(path)
    with zipfile.ZipFile(path, "w") as zf:
        # The mandatory `mimetype` member: first and `stored` (with no extra field).
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


def cell(value_type=None, value=None, formula=None, style=None, text=None, extra=""):
    attrs = ""
    if value_type:
        attrs += f' office:value-type="{value_type}"'
    if value is not None:
        attrs += f' office:value="{value}"'
    if formula:
        attrs += f' table:formula="{formula}"'
    if style:
        attrs += f' table:style-name="{style}"'
    body = "" if text is None else f"<text:p>{text}</text:p>"
    return f"<table:table-cell{attrs}{extra}>{body}</table:table-cell>"


# --- individual fixtures ----------------------------------------------------


def basic(path):
    rows = [
        "<table:table-row>"
        + cell(value_type="string", text="Name")
        + cell(value_type="float", value="42", text="42")
        + "</table:table-row>",
        "<table:table-row>"
        + cell(value_type="string", text="Pi")
        + cell(value_type="float", value="3.14", text="3.14")
        + "</table:table-row>",
    ]
    spreadsheet = '<table:table table:name="Data">' + "".join(rows) + "</table:table>"
    write_ods(path, content_xml("", spreadsheet), styles_xml())


def multi(path):
    s1 = (
        '<table:table table:name="First"><table:table-row>'
        + cell(value_type="string", text="a")
        + "</table:table-row></table:table>"
    )
    s2 = (
        '<table:table table:name="Second" table:display="false"><table:table-row>'
        + cell(value_type="string", text="hidden-cell")
        + "</table:table-row></table:table>"
    )
    s3 = (
        '<table:table table:name="Third"><table:table-row>'
        + cell(value_type="string", text="c")
        + "</table:table-row></table:table>"
    )
    write_ods(path, content_xml("", s1 + s2 + s3), styles_xml())


def values(path):
    auto = (
        '<style:style style:name="ceMoney" style:family="table-cell" '
        'style:data-style-name="N104"><style:table-cell-properties '
        'style:vertical-align="middle"/><style:text-properties fo:font-weight="bold"/>'
        "</style:style>"
        '<number:currency-style style:name="N104">'
        "<number:currency-symbol>$</number:currency-symbol>"
        "<number:number number:decimal-places=\"2\"/>"
        "</number:currency-style>"
    )
    row = (
        "<table:table-row>"
        + cell(value_type="float", value="1.5", text="1.5")
        + cell(
            value_type="float",
            value="3",
            formula="of:=SUM(A1:A1)",
            text="3",
        )
        + cell(value_type="boolean", extra=' office:boolean-value="true"', text="TRUE")
        + cell(value_type="date", extra=' office:date-value="2026-01-01"', text="2026-01-01")
        + "</table:table-row>"
    )
    repeated_cell = (
        '<table:table-cell table:number-columns-repeated="3" office:value-type="string">'
        "<text:p>x</text:p></table:table-cell>"
    )
    repeated_row = (
        '<table:table-row table:number-rows-repeated="2">'
        + cell(value_type="string", text="r")
        + "</table:table-row>"
    )
    spreadsheet = (
        '<table:table table:name="V">'
        + row
        + "<table:table-row>" + repeated_cell + "</table:table-row>"
        + repeated_row
        + "</table:table>"
    )
    write_ods(path, content_xml(auto, spreadsheet), styles_xml())


def merged(path):
    row = (
        "<table:table-row>"
        + cell(
            value_type="string",
            text="merged",
            extra=' table:number-columns-spanned="2" table:number-rows-spanned="1"',
        )
        + '<table:covered-table-cell/>'
        + "</table:table-row>"
    )
    write_ods(
        path,
        content_xml("", '<table:table table:name="M">' + row + "</table:table>"),
        styles_xml(),
    )


def named(path):
    spreadsheet = (
        '<table:table table:name="N"><table:table-row>'
        + cell(value_type="float", value="7", text="7")
        + "</table:table-row></table:table>"
        '<table:named-expressions>'
        '<table:named-range table:name="Rate" table:base-cell-address="N.A1" '
        'table:cell-range-address="N.A1:N.A1"/>'
        '<table:named-expression table:name="DoubleRate" '
        'table:base-cell-address="N.A1" table:expression="of:=N.A1*2"/>'
        "</table:named-expressions>"
    )
    write_ods(path, content_xml("", spreadsheet), styles_xml())


def comments(path):
    annotated = (
        '<table:table-cell office:value-type="string" table:style-name="ce1">'
        "<text:p>annotated</text:p>"
        '<office:annotation><dc:creator>Alice</dc:creator>'
        "<dc:date>2026-01-01T00:00:00</dc:date>"
        "<text:p>a note</text:p></office:annotation>"
        "</table:table-cell>"
    )
    row = "<table:table-row>" + annotated + "</table:table-row>"
    write_ods(
        path,
        content_xml("", '<table:table table:name="C">' + row + "</table:table>"),
        styles_xml(),
    )


def styles_fixture(path):
    named_style = (
        '<style:style style:name="ceBold" style:family="table-cell">'
        '<style:text-properties fo:font-weight="bold"/></style:style>'
    )
    auto_style = (
        '<style:style style:name="ceAuto" style:family="table-cell" '
        'style:parent-style-name="ceBold" style:data-style-name="N104">'
        '<style:table-cell-properties fo:background-color="#ffff00"/>'
        '<style:text-properties fo:color="#ff0000"/></style:style>'
        '<number:number-style style:name="N104"><number:number '
        'number:decimal-places="2"/></number:number-style>'
    )
    sheet = (
        '<table:table table:name="S"><table:table-row>'
        + cell(value_type="string", style="ceAuto", text="styled")
        + "</table:table-row></table:table>"
    )
    write_ods(
        path,
        content_xml(auto_style, sheet),
        styles_xml(styles=named_style),
    )


def bomb(path):
    # A row repeated far beyond any plausible bound: the adapter must decline
    # typed (resource limit) rather than allocate.
    bomb_row = (
        '<table:table-row table:number-rows-repeated="4000000">'
        + cell(value_type="string", text="b")
        + "</table:table-row>"
    )
    write_ods(
        path,
        content_xml("", '<table:table table:name="Bomb">' + bomb_row + "</table:table>"),
        styles_xml(),
    )


FIXTURES = {
    "basic.ods": basic,
    "multi.ods": multi,
    "values.ods": values,
    "merged.ods": merged,
    "named.ods": named,
    "comments.ods": comments,
    "styles.ods": styles_fixture,
    "bomb.ods": bomb,
}


# ===========================================================================
# Phase 21.3.2 — deterministic self-authored *corpus* mode (`--corpus DIR`).
#
# Emits a small corpus of varied ODS workbooks so the economic court has more
# than the eight tiny 21.3.1 fixtures to measure build/storage/query cost on:
# multi-sheet order/visibility, a large header+body sheet, typed values
# (float/string/boolean/date/currency/percentage), stored formulas with cached
# values, repeated cells/rows, merged cells, named ranges/expressions, cell
# comments, a rich styles fixture, an embedded image resource, and one
# several-thousand-row sheet. Every workbook is byte-deterministic (fixed member
# order, fixed `ZipInfo.date_time`, fixed per-member compression method); the
# numeric content uses a fixed-seed LCG (never `random`), so regeneration is
# reproducible. Python **stdlib only** (`zipfile` + hand-written OpenDocument
# XML).
#
# The corpus is deliberately NOT a real-world population: it is self-authored
# and small, and every claim the court makes is scoped to it.
# ===========================================================================

DRAW_NS = "urn:oasis:names:tc:opendocument:xmlns:drawing:1.0"
XLINK_NS = "http://www.w3.org/1999/xlink"

CORPUS_CONTENT_HEADER = (
    '<?xml version="1.0" encoding="UTF-8"?>'
    f'<office:document-content xmlns:office="{OFFICE_NS}" '
    f'xmlns:text="{TEXT_NS}" xmlns:table="{TABLE_NS}" '
    f'xmlns:style="{STYLE_NS}" xmlns:fo="{FO_NS}" '
    f'xmlns:number="{NUMBER_NS}" xmlns:dc="{DC_NS}" '
    f'xmlns:draw="{DRAW_NS}" xmlns:xlink="{XLINK_NS}" office:version="1.2">'
)

# A minimal, deterministic 1x1 RGBA PNG (real PNG signature + IHDR/IDAT/IEND),
# used as the one embedded resource the court asks about (Q9).
PNG_1X1 = bytes.fromhex(
    "89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c489"
    "0000000a49444154789c63000100000500010d0a2db40000000049454e44ae426082"
)


def _esc(text):
    return (
        str(text)
        .replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace('"', "&quot;")
    )


def _lcg(seed):
    """A deterministic 31-bit LCG (no `random`), for reproducible content."""
    state = [seed & 0x7FFFFFFF]

    def nxt():
        state[0] = (1103515245 * state[0] + 12345) & 0x7FFFFFFF
        return state[0]

    return nxt


def corpus_content(spreadsheet, automatic_styles=""):
    return (
        CORPUS_CONTENT_HEADER
        + f"<office:automatic-styles>{automatic_styles}</office:automatic-styles>"
        + "<office:body>"
        + f"<office:spreadsheet>{spreadsheet}</office:spreadsheet>"
        + "</office:body>"
        + "</office:document-content>"
    ).encode("utf-8")


def write_ods_ex(path, members, extra_manifest=()):
    """Write one deterministic ODS from an explicit member list:
    `members` = [(name, bytes, compress_type)] inserted after the manifest."""
    if os.path.exists(path):
        os.remove(path)
    with zipfile.ZipFile(path, "w") as zf:
        info = zipfile.ZipInfo("mimetype", FIXED_TIME)
        info.compress_type = zipfile.ZIP_STORED
        zf.writestr(info, MIMETYPE.encode("ascii"))
        info = zipfile.ZipInfo("META-INF/manifest.xml", FIXED_TIME)
        info.compress_type = zipfile.ZIP_DEFLATED
        zf.writestr(info, manifest_xml(extra_manifest))
        for name, data, comp in members:
            info = zipfile.ZipInfo(name, FIXED_TIME)
            info.compress_type = comp
            zf.writestr(info, data)


def ccell(
    value_type=None,
    value=None,
    formula=None,
    style=None,
    text=None,
    boolean=None,
    date=None,
    string_value=None,
    repeat=None,
    col_span=None,
    row_span=None,
    annotation=None,
    draw=None,
    covered=False,
):
    """A corpus cell element. `annotation` = {author,date,text}; `draw` =
    xlink:href of a `<draw:frame><draw:image>` placed in the cell."""
    attrs = ""
    if repeat and repeat > 1:
        attrs += f' table:number-columns-repeated="{repeat}"'
    if col_span and col_span > 1:
        attrs += f' table:number-columns-spanned="{col_span}"'
    if row_span and row_span > 1:
        attrs += f' table:number-rows-spanned="{row_span}"'
    if value_type:
        attrs += f' office:value-type="{value_type}"'
    if value is not None:
        attrs += f' office:value="{value}"'
    if boolean is not None:
        attrs += f' office:boolean-value="{"true" if boolean else "false"}"'
    if date is not None:
        attrs += f' office:date-value="{date}"'
    if string_value is not None:
        attrs += f' office:string-value="{_esc(string_value)}"'
    if formula:
        attrs += f' table:formula="{_esc(formula)}"'
    if style:
        attrs += f' table:style-name="{style}"'
    if covered:
        return f"<table:covered-table-cell{attrs}/>"
    body = ""
    if text is not None:
        body += f"<text:p>{_esc(text)}</text:p>"
    if draw:
        body += (
            f'<draw:frame draw:name="img1">'
            f'<draw:image xlink:href="{_esc(draw)}" xlink:type="simple"/>'
            "</draw:frame>"
        )
    if annotation:
        body += (
            "<office:annotation>"
            f"<dc:creator>{_esc(annotation['author'])}</dc:creator>"
            f"<dc:date>{annotation['date']}</dc:date>"
            f"<text:p>{_esc(annotation['text'])}</text:p>"
            "</office:annotation>"
        )
    return f"<table:table-cell{attrs}>{body}</table:table-cell>"


def crow(cells, repeat=None):
    attrs = f' table:number-rows-repeated="{repeat}"' if repeat and repeat > 1 else ""
    return f"<table:table-row{attrs}>" + "".join(cells) + "</table:table-row>"


def ctable(name, rows, display=None, style=None):
    attrs = f' table:name="{_esc(name)}"'
    if display is False:
        attrs += ' table:display="false"'
    if style:
        attrs += f' table:style-name="{style}"'
    return f"<table:table{attrs}>" + "".join(rows) + "</table:table>"


# --- corpus fixtures --------------------------------------------------------


def c01_basic(path):
    """A small header+body sheet with stored formulas and cached values."""
    rows = [
        crow(
            [
                ccell(value_type="string", text="Item"),
                ccell(value_type="string", text="Qty"),
                ccell(value_type="string", text="Price"),
                ccell(value_type="string", text="Total"),
            ]
        )
    ]
    for r in range(2, 14):
        qty = (r - 1) * 3
        price = 100 + (r - 2) * 7
        rows.append(
            crow(
                [
                    ccell(value_type="string", text=f"item-{r - 1}"),
                    ccell(value_type="float", value=str(qty), text=str(qty)),
                    ccell(value_type="float", value=str(price), text=str(price)),
                    ccell(
                        value_type="float",
                        value=str(qty * price),
                        formula=f"of:=[.B{r}]*[.C{r}]",
                        text=str(qty * price),
                    ),
                ]
            )
        )
    sheet = ctable("Data", rows)
    named = (
        "<table:named-expressions>"
        '<table:named-range table:name="Items" table:base-cell-address="Data.C2" '
        'table:cell-range-address="Data.C2:Data.C13"/>'
        "</table:named-expressions>"
    )
    members = [
        ("content.xml", corpus_content(sheet + named), zipfile.ZIP_DEFLATED),
        ("styles.xml", styles_xml(), zipfile.ZIP_DEFLATED),
        ("meta.xml", meta_xml(), zipfile.ZIP_STORED),
    ]
    write_ods_ex(path, members)


def c02_typed(path):
    """Every typed value the adapter models: float/string/boolean/date/currency/
    percentage, plus stored formulas with cached values."""
    auto = (
        '<number:currency-style style:name="NCur">'
        "<number:currency-symbol>$</number:currency-symbol>"
        '<number:number number:decimal-places="2"/></number:currency-style>'
        '<number:percentage-style style:name="NPct">'
        '<number:number number:decimal-places="0"/></number:percentage-style>'
        '<style:style style:name="ceCur" style:family="table-cell" '
        'style:data-style-name="NCur"/>'
        '<style:style style:name="cePct" style:family="table-cell" '
        'style:data-style-name="NPct"/>'
    )
    rows = []
    for i in range(20):
        rows.append(
            crow(
                [
                    ccell(value_type="float", value=str(i) + ".5", text=str(i) + ".5"),
                    ccell(value_type="string", text=f"label-{i}"),
                    ccell(value_type="boolean", boolean=(i % 2 == 0), text="TRUE" if i % 2 == 0 else "FALSE"),
                    ccell(value_type="date", date=f"2026-01-{i % 28 + 1:02d}", text=f"2026-01-{i % 28 + 1:02d}"),
                    ccell(value_type="currency", value=str(1000 + i), style="ceCur", text=f"${1000 + i}.00"),
                    ccell(value_type="percentage", value="0.25", style="cePct", text="25%"),
                    ccell(
                        value_type="float",
                        value=str(2 * i),
                        formula=f"of:=[.A{i + 1}]*2",
                        text=str(2 * i),
                    ),
                ]
            )
        )
    members = [
        ("content.xml", corpus_content(ctable("Types", rows), auto), zipfile.ZIP_DEFLATED),
        ("styles.xml", styles_xml(), zipfile.ZIP_DEFLATED),
        ("meta.xml", meta_xml(), zipfile.ZIP_STORED),
    ]
    write_ods_ex(path, members)


def c03_repeated(path):
    """Repeated cells and repeated rows (bounded, real expansion)."""
    rows = [
        crow([
            ccell(value_type="string", text="h"),
            ccell(value_type="string", text="x", repeat=3),
        ]),
        crow([ccell(value_type="string", text="r"), ccell(value_type="float", value="7", text="7")], repeat=2),
        crow([ccell(value_type="float", value="42", text="42")]),
    ]
    members = [
        ("content.xml", corpus_content(ctable("Rep", rows)), zipfile.ZIP_DEFLATED),
        ("styles.xml", styles_xml(), zipfile.ZIP_DEFLATED),
        ("meta.xml", meta_xml(), zipfile.ZIP_STORED),
    ]
    write_ods_ex(path, members)


def c04_merged(path):
    """Merged cells (column + row spans with covered continuation cells)."""
    rows = [
        crow([ccell(value_type="string", text="merged-2x1", col_span=2), ccell(covered=True)]),
        crow([ccell(value_type="string", text="merged-1x2", row_span=2), ccell(value_type="float", value="1", text="1")]),
        crow([ccell(covered=True), ccell(value_type="float", value="2", text="2")]),
    ]
    members = [
        ("content.xml", corpus_content(ctable("Merge", rows)), zipfile.ZIP_DEFLATED),
        ("styles.xml", styles_xml(), zipfile.ZIP_DEFLATED),
        ("meta.xml", meta_xml(), zipfile.ZIP_STORED),
    ]
    write_ods_ex(path, members)


def c05_named(path):
    """Two sheets (one hidden) with named ranges and named expressions that
    reference each sheet."""
    s0 = ctable(
        "First",
        [crow([ccell(value_type="float", value=str(i), text=str(i)) for i in range(4)]) for _ in range(6)],
    )
    s1 = ctable(
        "Second",
        [crow([ccell(value_type="float", value=str(i * 10), text=str(i * 10)) for i in range(3)]) for _ in range(4)],
        display=False,
    )
    named = (
        "<table:named-expressions>"
        '<table:named-range table:name="Block" table:base-cell-address="First.A1" '
        'table:cell-range-address="First.A1:First.B2"/>'
        '<table:named-range table:name="SecondCol" table:base-cell-address="Second.A1" '
        'table:cell-range-address="Second.A1:Second.A4"/>'
        '<table:named-expression table:name="Scale" table:base-cell-address="First.C1" '
        'table:expression="of:=[.C1]*10"/>'
        "</table:named-expressions>"
    )
    members = [
        ("content.xml", corpus_content(s0 + s1 + named), zipfile.ZIP_DEFLATED),
        ("styles.xml", styles_xml(), zipfile.ZIP_DEFLATED),
        ("meta.xml", meta_xml(), zipfile.ZIP_STORED),
    ]
    write_ods_ex(path, members)


def c06_comments(path):
    """Cell comments plus one embedded image resource (manifest-declared PNG
    member, referenced by a `<draw:frame>` in a cell)."""
    rows = []
    for r in range(6):
        cells = [
            ccell(value_type="float", value=str(r), text=str(r)),
            ccell(
                value_type="string",
                text=f"cell-{r}",
                annotation={"author": "Alice" if r % 2 == 0 else "Bob", "date": "2026-01-01T00:00:00", "text": f"note {r}"},
            ),
        ]
        if r == 0:
            cells.append(ccell(value_type="string", text="pic", draw="Pictures/image1.png"))
        rows.append(crow(cells))
    members = [
        ("content.xml", corpus_content(ctable("Notes", rows)), zipfile.ZIP_DEFLATED),
        ("styles.xml", styles_xml(), zipfile.ZIP_DEFLATED),
        ("meta.xml", meta_xml(), zipfile.ZIP_STORED),
        ("Pictures/image1.png", PNG_1X1, zipfile.ZIP_STORED),
    ]
    write_ods_ex(path, members, extra_manifest=[("Pictures/image1.png", "image/png")])


def c07_styles(path):
    """A rich styles fixture: automatic cell styles + number formats in the
    content part, named table-cell styles in the styles part."""
    auto = (
        '<number:number-style style:name="N2"><number:number number:decimal-places="2"/>'
        "</number:number-style>"
        '<style:style style:name="ceA" style:family="table-cell" style:data-style-name="N2">'
        '<style:table-cell-properties fo:background-color="#eeeeee"/>'
        '<style:text-properties fo:font-weight="bold"/></style:style>'
        '<style:style style:name="ceB" style:family="table-cell">'
        '<style:text-properties fo:color="#0000ff"/></style:style>'
    )
    named = (
        '<style:style style:name="ceP" style:family="table-cell" style:parent-style-name="ceA">'
        '<style:text-properties fo:font-style="italic"/></style:style>'
    )
    rows = []
    for r in range(30):
        rows.append(
            crow(
                [
                    ccell(value_type="float", value=str(r), style="ceA", text=str(r)),
                    ccell(value_type="float", value=str(r * 2), style="ceB", text=str(r * 2)),
                    ccell(value_type="float", value=str(r * 3), style="ceP", text=str(r * 3)),
                    ccell(value_type="string", text=f"s{r}"),
                ]
            )
        )
    members = [
        ("content.xml", corpus_content(ctable("Styled", rows), auto), zipfile.ZIP_DEFLATED),
        ("styles.xml", styles_xml(styles=named), zipfile.ZIP_DEFLATED),
        ("meta.xml", meta_xml(), zipfile.ZIP_STORED),
    ]
    write_ods_ex(path, members)


def c08_large(path):
    """One several-thousand-row sheet: the size/feature fixture (target ~0.5-3 MB)."""
    nxt = _lcg(21320)
    nrows = 6000
    ncols = 24
    rows = []
    for r in range(1, nrows + 1):
        cells = [ccell(value_type="float", value=str(r), text=str(r))]
        for _ in range(1, ncols):
            v = nxt() % 1000000
            cells.append(ccell(value_type="float", value=str(v), text=str(v)))
        rows.append(crow(cells))
    rows.append(
        crow(
            [ccell(value_type="float", value="0", formula="of:=SUM([.A1:.A6000])", text="0")]
            + [ccell(value_type="float", value="0", text="0") for _ in range(ncols - 1)]
        )
    )
    members = [
        ("content.xml", corpus_content(ctable("Big", rows)), zipfile.ZIP_DEFLATED),
        ("styles.xml", styles_xml(), zipfile.ZIP_DEFLATED),
        ("meta.xml", meta_xml(), zipfile.ZIP_STORED),
    ]
    write_ods_ex(path, members)


CORPUS = [
    ("c01-basic.ods", c01_basic),
    ("c02-typed.ods", c02_typed),
    ("c03-repeated.ods", c03_repeated),
    ("c04-merged.ods", c04_merged),
    ("c05-named.ods", c05_named),
    ("c06-comments.ods", c06_comments),
    ("c07-styles.ods", c07_styles),
    ("c08-large.ods", c08_large),
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
    out_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), "ods")
    os.makedirs(out_dir, exist_ok=True)
    for name, fn in sorted(FIXTURES.items()):
        fn(os.path.join(out_dir, name))
        print(f"wrote {os.path.join(out_dir, name)}")
    return 0


if __name__ == "__main__":
    main()
