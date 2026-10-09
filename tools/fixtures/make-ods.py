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


def main():
    out_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), "ods")
    os.makedirs(out_dir, exist_ok=True)
    for name, fn in sorted(FIXTURES.items()):
        fn(os.path.join(out_dir, name))
        print(f"wrote {os.path.join(out_dir, name)}")


if __name__ == "__main__":
    main()
