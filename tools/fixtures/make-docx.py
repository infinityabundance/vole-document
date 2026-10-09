#!/usr/bin/env python3
# Phase 21.5.3 (FIX 3) — deterministic minimal WordprocessingML (DOCX) fixture
# generator (Python stdlib only). The identity court needs one real OPC package of
# each kind; the other formats already have generators (make-xlsx/pptx/ods/odp/
# json/yaml) and PDF comes from `pdf-make-samples`. DOCX did not, so this fills the
# gap with the same OPC shape the detector requires: a valid ZIP carrying
# `[Content_Types].xml` with the WordprocessingML main content type, a package
# `officeDocument` relationship targeting `word/document.xml`, and that part.
#
#   python3 tools/fixtures/make-docx.py                 # write tools/fixtures/docx/
#   python3 tools/fixtures/make-docx.py --corpus DIR    # write DIR/, print TSV

import hashlib
import os
import sys
import zipfile

CT_NS = "http://schemas.openxmlformats.org/package/2006/content-types"
REL_NS = "http://schemas.openxmlformats.org/package/2006/relationships"
OD_REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
CT_DOCX_MAIN = "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"
FIXED_TIME = (1980, 1, 1, 0, 0, 0)


def content_types():
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<Types xmlns="{CT_NS}">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        f'<Override PartName="/word/document.xml" ContentType="{CT_DOCX_MAIN}"/>'
        "</Types>"
    ).encode("utf-8")


def package_rels():
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<Relationships xmlns="{REL_NS}">'
        f'<Relationship Id="rId1" Type="{OD_REL}/officeDocument" Target="word/document.xml"/>'
        "</Relationships>"
    ).encode("utf-8")


def document_xml(text="Hello DOCX"):
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<w:document xmlns:w="{W_NS}"><w:body>'
        f'<w:p><w:r><w:t>{text}</w:t></w:r></w:p>'
        "</w:body></w:document>"
    ).encode("utf-8")


def basic():
    return [
        ("[Content_Types].xml", content_types(), zipfile.ZIP_STORED),
        ("_rels/.rels", package_rels(), zipfile.ZIP_DEFLATED),
        ("word/document.xml", document_xml(), zipfile.ZIP_DEFLATED),
    ]


FIXTURES = [("basic.docx", basic)]


def write_docx(path, members):
    with zipfile.ZipFile(path, "w") as z:
        for name, data, comp in members:
            zi = zipfile.ZipInfo(name, date_time=FIXED_TIME)
            zi.compress_type = comp
            zi.external_attr = 0o600 << 16
            z.writestr(zi, data)


def emit_corpus(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    for name, fn in FIXTURES:
        path = os.path.join(out_dir, name)
        write_docx(path, fn())
        data = open(path, "rb").read()
        print("%s\t%d\t%s" % (name, len(data), hashlib.sha256(data).hexdigest()))


def main():
    if len(sys.argv) >= 3 and sys.argv[1] == "--corpus":
        emit_corpus(sys.argv[2])
        return 0
    here = os.path.dirname(os.path.abspath(__file__))
    out = os.path.join(here, "docx")
    os.makedirs(out, exist_ok=True)
    for name, fn in FIXTURES:
        path = os.path.join(out, name)
        write_docx(path, fn())
        print("%s\t%d bytes" % (name, os.path.getsize(path)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
