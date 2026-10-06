#!/usr/bin/env python3
"""Phase-12.13 hostile corpus generator (deterministic, Python stdlib only).

Produces a small, locally-generated corpus of hostile DOCX/EPUB/ZIP/XML inputs
for the Phase-12 security court and the fuzz seeds. Every fixture is built from
the standard library alone (``struct``/``zlib``/``zipfile``) — **no third-party
document bytes** are committed (``fuzz/README.md``). Each input is deliberately
tiny (a few hundred bytes to a few KiB): the decoders must reject or decline
*before* inflating, so a fixture never needs the memory a successful expansion
would (research I §9).

The corpus mirrors the threat list frozen in
``research/subagents/phase-12/I-security.md`` §1–§4/§10 and plan §123. For each
fixture the manifest records a *research-derived* coarse disposition prediction:

  * ``reject``   — the byte cover / identity is broken, so the ZIP/OPC/OCF
                   scanner must return a typed error (never an approximation);
  * ``preserve`` — semantics/resource only: the exact bytes are always preserved
                   and the *derived* observation is declined, typed;
  * ``accept``   — a well-formed (if unusual) package the pipeline answers.

The prediction is a falsifiable claim, not a description of the code: the court
records the observed outcome and reports any divergence.

Usage (Docker only):
  docker compose run --rm --no-TTY doc-baseline \
      python3 tools/fixtures/phase12-hostile-gen.py <OUTDIR>
"""

import hashlib
import io
import json
import os
import struct
import sys
import zipfile
import zlib

STORE = 0
DEFLATE = 8

# ZIP flag bits we manipulate.
FLAG_ENCRYPTED = 0x0001
FLAG_DATA_DESCRIPTOR = 0x0008

LFH_SIG = 0x04034B50
CDH_SIG = 0x02014B50
EOCD_SIG = 0x06054B50

CT_NS = "http://schemas.openxmlformats.org/package/2006/content-types"
REL_NS = "http://schemas.openxmlformats.org/package/2006/relationships"
OPC_REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
R_NS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
CONTAINER_NS = "urn:oasis:names:tc:opendocument:xmlns:container"
OPF_MEDIA = "application/oebps-package+xml"
MAIN_CT = (
    "application/vnd.openxmlformats-officedocument."
    "wordprocessingml.document.main+xml"
)


def crc32(data: bytes) -> int:
    return zlib.crc32(data) & 0xFFFFFFFF


def deflate_raw(data: bytes) -> bytes:
    """RFC-1951 raw DEFLATE (no zlib header/trailer) — what ZIP stores."""
    c = zlib.compressobj(9, zlib.DEFLATED, -15)
    return c.compress(data) + c.flush()


# ---------------------------------------------------------------------------
# Minimal valid DOCX / EPUB bases (LOCALLY generated; no third-party bytes).
# ---------------------------------------------------------------------------


def docx_entries(document_xml: bytes, *, extra=None, main_rel=True,
                 package_rels_override=None, content_types_override=None,
                 story_names=None):
    """The members of a minimal valid DOCX opc package.

    ``story_names`` lets a caller add a second (duplicate / hazardous) part name.
    """
    content_types = content_types_override or (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<Types xmlns="{CT_NS}">'
        '<Default Extension="rels" '
        'ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        f'<Override PartName="/word/document.xml" ContentType="{MAIN_CT}"/>'
        "</Types>"
    ).encode()
    if package_rels_override is not None:
        package_rels = package_rels_override
    elif main_rel:
        package_rels = (
            '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
            f'<Relationships xmlns="{REL_NS}">'
            f'<Relationship Id="rId1" Type="{OPC_REL}/officeDocument" '
            'Target="word/document.xml"/>'
            "</Relationships>"
        ).encode()
    else:
        package_rels = (
            '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
            f'<Relationships xmlns="{REL_NS}">'
            f'<Relationship Id="rId1" Type="{OPC_REL}/styles" '
            'Target="word/styles.xml"/>'
            "</Relationships>"
        ).encode()
    out = [
        ("[Content_Types].xml", content_types, DEFLATE),
        ("_rels/.rels", package_rels, DEFLATE),
    ]
    names = story_names or ["word/document.xml"]
    for n in names:
        out.append((n, document_xml, DEFLATE))
    if extra:
        out.extend(extra)
    return out


def build_zip(entries) -> bytes:
    """Deterministic zipfile-based ZIP (fixed metadata, no timestamps)."""
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", allowZip64=True) as z:
        for name, data, method in entries:
            zi = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            zi.compress_type = method
            zi.create_system = 3
            zi.external_attr = 0
            z.writestr(zi, data)
    return buf.getvalue()


def epub_entries(chapter_xhtml: bytes, *, container_override=None,
                 opf_override=None, nav_xhtml=None, mimetype=b"application/epub+zip",
                 full_path="OEBPS/package.opf", rootfile_path=None, extra=None):
    # `rootfile_path` defaults to the member location; when it differs the
    # container points at a package document that is not present (a decline).
    if rootfile_path is None:
        rootfile_path = full_path
    container = container_override or (
        '<?xml version="1.0" encoding="UTF-8"?>'
        f'<container version="1.0" xmlns="{CONTAINER_NS}">'
        "<rootfiles>"
        f'<rootfile full-path="{rootfile_path}" media-type="{OPF_MEDIA}"/>'
        "</rootfiles></container>"
    ).encode()
    opf = opf_override or (
        '<?xml version="1.0" encoding="UTF-8"?>'
        '<package xmlns="http://www.idpf.org/2007/opf" version="3.0" '
        'unique-identifier="pub-id">'
        '<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">'
        '<dc:identifier id="pub-id">urn:uuid:hostile</dc:identifier>'
        "<dc:title>Hostile</dc:title><dc:language>en</dc:language>"
        "</metadata>"
        "<manifest>"
        '<item id="ch1" href="chapter1.xhtml" '
        'media-type="application/xhtml+xml"/>'
        "</manifest>"
        '<spine><itemref idref="ch1"/></spine>'
        "</package>"
    ).encode()
    nav = nav_xhtml or (
        '<?xml version="1.0" encoding="UTF-8"?>'
        '<html xmlns="http://www.w3.org/1999/xhtml" '
        'xmlns:epub="http://www.idpf.org/2007/ops">'
        "<head><title>n</title></head><body>"
        '<nav epub:type="toc" id="toc"><ol><li><a href="chapter1.xhtml">c</a>'
        "</li></ol></nav></body></html>"
    ).encode()
    out = [
        ("mimetype", mimetype, STORE),
        ("META-INF/container.xml", container, DEFLATE),
        (full_path, opf, DEFLATE),
        ("OEBPS/nav.xhtml", nav, DEFLATE),
        ("OEBPS/chapter1.xhtml", chapter_xhtml, DEFLATE),
    ]
    if extra:
        out.extend(extra)
    return out


# ---------------------------------------------------------------------------
# Raw ZIP assembler for structural faults (full byte control).
# ---------------------------------------------------------------------------


def raw_zip(entries, *, prepend=b"", append=b"", eocd=True, comment=b"",
            disk=0, cd_disk=0, entries_total=None, entries_this=None,
            cd_offset=None, cd_size=None, eocd_comment_len=None,
            omit_cd=False, truncate=0):
    """Assemble a ZIP with byte-level fault injection.

    ``entries`` is a list of dicts with keys: ``name`` (bytes/str), ``data``,
    ``method``, ``flags``, ``crc``, ``comp``, ``uncomp``, ``extra``,
    ``comment``, ``local_offset`` (override), ``cd_local_offset`` (override).
    """
    out = bytearray(prepend)
    local_offsets = []
    for e in entries:
        name = e["name"] if isinstance(e["name"], bytes) else e["name"].encode()
        data = e.get("data", b"")
        method = e.get("method", STORE)
        flags = e.get("flags", 0)
        crc = e.get("crc", crc32(data))
        comp = e.get("comp", len(data))
        uncomp = e.get("uncomp", len(data))
        extra = e.get("extra", b"")
        local_offsets.append(
            e["local_offset"] if e.get("local_offset") is not None else len(out)
        )
        lfh = struct.pack(
            "<IHHHHHIIIHH", LFH_SIG, 20, flags, method, 0, 0, crc, comp, uncomp,
            len(name), len(extra),
        )
        out += lfh + name + extra + data
    cd_start = len(out)
    if not omit_cd:
        for i, e in enumerate(entries):
            name = e["name"] if isinstance(e["name"], bytes) else e["name"].encode()
            data = e.get("data", b"")
            method = e.get("method", STORE)
            flags = e.get("flags", 0)
            crc = e.get("crc", crc32(data))
            comp = e.get("comp", len(data))
            uncomp = e.get("uncomp", len(data))
            extra = e.get("extra", b"")
            ecomment = e.get("comment", b"")
            off = e.get("cd_local_offset", local_offsets[i])
            cdh = struct.pack(
                "<IHHHHHHIIIHHHHHII", CDH_SIG, 20, 20, flags, method, 0, 0, crc,
                comp, uncomp, len(name), len(extra), len(ecomment), 0, 0, 0, off,
            )
            out += cdh + name + extra + ecomment
    cd_end = len(out)
    real_cd_size = cd_end - cd_start
    if eocd:
        et = entries_this if entries_this is not None else len(entries)
        tot = entries_total if entries_total is not None else len(entries)
        co = cd_offset if cd_offset is not None else cd_start
        cs = cd_size if cd_size is not None else real_cd_size
        cl = eocd_comment_len if eocd_comment_len is not None else len(comment)
        out += struct.pack(
            "<IHHHHIIH", EOCD_SIG, disk, cd_disk, et, tot, cs, co, cl
        ) + comment
    out += append
    if truncate:
        out = out[:-truncate]
    return bytes(out)


# ---------------------------------------------------------------------------
# Corpus.
# ---------------------------------------------------------------------------


def fixture_cases():
    """Return a list of (name, category, expect, bytes)."""
    cases = []

    def add(name, category, expect, data):
        cases.append((name, category, expect, data))

    # --- ZIP structural faults (cover / identity broken -> reject) ----------
    base = build_zip([("hello.txt", b"hello world\n", STORE)])
    add("z_truncated.zip", "zip-structure", "decline", base[:-8])
    add("z_no_eocd.zip", "zip-structure", "decline",
        b"PK\x03\x04" + b"\x00" * 64 + b"no eocd here")
    # EOCD whose declared comment length runs past EOF.
    add("z_bad_eocd_comment.zip", "zip-structure", "decline",
        raw_zip([dict(name="a.txt", data=b"a")], eocd_comment_len=0x4000))
    # EOCD points the central directory beyond EOF.
    add("z_bad_cd_offset.zip", "zip-structure", "decline",
        raw_zip([dict(name="a.txt", data=b"a")], cd_offset=1 << 30))
    # ZIP64 sentinel without the ZIP64 records.
    add("z_zip64_inconsistent.zip", "zip-structure", "decline",
        raw_zip([dict(name="a.txt", data=b"a")], entries_total=0xFFFF,
                entries_this=0xFFFF))
    # Multi-disk layout (not reconstructible from one file).
    add("z_multidisk.zip", "zip-structure", "decline",
        raw_zip([dict(name="a.txt", data=b"a")], disk=1, cd_disk=1))
    # Overlapping / impossible member offsets: two LFHs claim the same bytes.
    add("z_overlap.zip", "zip-structure", "decline",
        raw_zip([
            dict(name="a.txt", data=b"AAAAAAAA"),
            dict(name="b.txt", data=b"BBBBBBBB", local_offset=0,
                 cd_local_offset=0),
        ]))
    # A name that escapes the package root (traversal).
    add("z_traversal_name.zip", "zip-identity", "decline",
        raw_zip([dict(name="../../etc/passwd", data=b"x")]))
    # An absolute name.
    add("z_absolute_name.zip", "zip-identity", "decline",
        raw_zip([dict(name="/etc/passwd", data=b"x")]))
    # A Windows drive name.
    add("z_drive_name.zip", "zip-identity", "decline",
        raw_zip([dict(name="C:\\win\\evil", data=b"x")]))
    # A backslash separator.
    add("z_backslash_name.zip", "zip-identity", "decline",
        raw_zip([dict(name="a\\b", data=b"x")]))
    # A NUL byte in the name.
    add("z_nul_name.zip", "zip-identity", "decline",
        raw_zip([dict(name=b"a\x00b", data=b"x")]))
    # Duplicate member names (identity ambiguity -> typed decline, bytes exact).
    add("z_duplicate_names.zip", "zip-identity", "decline",
        raw_zip([dict(name="dup.txt", data=b"one"),
                 dict(name="dup.txt", data=b"two")]))

    # --- ZIP resource / semantics faults (bytes preserved, decode declined) --
    # A ratio bomb: tiny stored payload, huge declared uncompressed size.
    add("z_ratio_bomb.zip", "zip-resource", "decline",
        raw_zip([dict(name="bomb.bin", data=b"\x00" * 16, comp=16,
                      uncomp=1 << 30)]))
    # A declared expansion beyond every configured member cap (STRICT) / ratio
    # cap (DEFAULT): never inflated, rejected before allocation.
    add("z_huge_declared.zip", "zip-resource", "decline",
        raw_zip([dict(name="huge.bin", data=b"\x00" * 16, comp=16,
                      uncomp=1 << 31)]))
    # A stored member whose CRC disagrees with its bytes.
    add("z_bad_crc.zip", "zip-resource", "decline",
        raw_zip([dict(name="crc.txt", data=b"hello", crc=0xDEADBEEF)]))
    # An encrypted member (ZipCrypto general-purpose bit 0).
    add("z_encrypted.zip", "zip-resource", "decline",
        raw_zip([dict(name="secret.bin", data=b"\x01\x02\x03",
                      flags=FLAG_ENCRYPTED)]))
    # An unknown compression method (AES marker 99).
    add("z_unknown_method.zip", "zip-resource", "decline",
        raw_zip([dict(name="mystery.bin", data=b"\x01\x02\x03", method=99)]))
    # A malformed data descriptor (bit-3 set, sizes zero, garbage follows).
    add("z_bad_descriptor.zip", "zip-structure", "decline",
        raw_zip([dict(name="d.bin", data=b"payload" + b"\xff\xff\xff\xff",
                      flags=FLAG_DATA_DESCRIPTOR, crc=0, comp=0, uncomp=0)]))

    # --- ZIP benign-unusual (must be accepted exactly) ----------------------
    add("z_empty.zip", "zip-benign", "decline",
        raw_zip([], comment=b"empty archive comment"))
    add("z_prefix_stub.zip", "zip-benign", "decline",
        raw_zip([dict(name="p.txt", data=b"stub")],
                prepend=b"#!/bin/sh\n# self-extracting stub\n"))
    add("z_eps_stored.zip", "zip-benign", "decline", base)

    # --- DOCX XML/OPC faults ------------------------------------------------
    plain_doc = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<w:document xmlns:w="{W_NS}" xmlns:r="{R_NS}"><w:body>'
        "<w:p><w:r><w:t>hello hostile world</w:t></w:r></w:p>"
        "</w:body></w:document>"
    ).encode()

    # A well-formed DOCX baseline (control for the mutations below).
    add("docx_ok.docx", "opc", "accept", build_zip(docx_entries(plain_doc)))

    # DOCTYPE in the main document (no DTD may ever be resolved).
    doctype_doc = (
        b'<?xml version="1.0"?>'
        b'<!DOCTYPE w:document [<!ENTITY x "boom">]>'
        + f'<w:document xmlns:w="{W_NS}"><w:body><w:p><w:r><w:t>&x;</w:t>'.encode()
        + b"</w:r></w:p></w:body></w:document>"
    )
    add("docx_doctype.docx", "xml", "reject",
        build_zip(docx_entries(doctype_doc)))

    # A billion-laughs entity bomb (never expanded: DOCTYPE is refused).
    bomb = (
        b'<?xml version="1.0"?>'
        b"<!DOCTYPE lolz ["
        b'<!ENTITY lol "lol">'
        b'<!ENTITY lol2 "&lol;&lol;&lol;&lol;">'
        b'<!ENTITY lol3 "&lol2;&lol2;&lol2;&lol2;">'
        b'<!ENTITY lol4 "&lol3;&lol3;&lol3;&lol3;">'
        b"]>"
        + f'<w:document xmlns:w="{W_NS}"><w:body><w:p><w:r><w:t>&lol4;</w:t>'.encode()
        + b"</w:r></w:p></w:body></w:document>"
    )
    add("docx_entity_bomb.docx", "xml", "reject",
        build_zip(docx_entries(bomb)))

    # Deep nesting beyond the depth cap (tiny: ~1 KiB).
    depth = 300
    deep_doc = (
        b'<?xml version="1.0"?>'
        + f'<w:document xmlns:w="{W_NS}"><w:body>'.encode()
        + b"<w:p>" * depth
        + b"<w:r><w:t>deep</w:t></w:r>"
        + b"</w:p>" * depth
        + b"</w:body></w:document>"
    )
    add("docx_deep_nesting.docx", "xml", "reject",
        build_zip(docx_entries(deep_doc)))

    # A non-UTF-8 main part (bytes preserved, semantics declined).
    non_utf8 = b'<?xml version="1.0"?>\xff\xfe<w:document xmlns:w="' + \
        W_NS.encode() + b'"/>'
    add("docx_non_utf8.docx", "xml", "reject",
        build_zip(docx_entries(non_utf8)))

    # A NUL byte in the main part.
    nul_doc = (
        b'<?xml version="1.0"?>'
        + f'<w:document xmlns:w="{W_NS}"><w:body><w:p>\x00</w:p>'.encode()
        + b"</w:body></w:document>"
    )
    add("docx_nul.docx", "xml", "reject",
        build_zip(docx_entries(nul_doc)))

    # Malformed XML in [Content_Types].xml (parsed eagerly by the OPC core).
    add("docx_bad_content_types.docx", "opc", "reject",
        build_zip(docx_entries(
            plain_doc,
            content_types_override=b'<Types><Default Extension="xml"></Types>',
        )))

    # Missing main document relationship (main part is discovered, not guessed).
    add("docx_missing_main_rel.docx", "opc", "reject",
        build_zip(docx_entries(plain_doc, main_rel=False)))

    # Ambiguous main part: two officeDocument relationships.
    ambiguous_rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<Relationships xmlns="{REL_NS}">'
        f'<Relationship Id="rId1" Type="{OPC_REL}/officeDocument" '
        'Target="word/document.xml"/>'
        f'<Relationship Id="rId2" Type="{OPC_REL}/officeDocument" '
        'Target="word/other.xml"/>'
        "</Relationships>"
    ).encode()
    add("docx_ambiguous_main.docx", "opc", "reject",
        build_zip(docx_entries(
            plain_doc, package_rels_override=ambiguous_rels,
            extra=[("word/other.xml", plain_doc, DEFLATE)],
        )))

    # An external main-part relationship (inert identifier, never fetched).
    external_rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<Relationships xmlns="{REL_NS}">'
        f'<Relationship Id="rId1" Type="{OPC_REL}/officeDocument" '
        'Target="https://evil.example.com/doc.xml" TargetMode="External"/>'
        "</Relationships>"
    ).encode()
    add("docx_external_main.docx", "opc", "reject",
        build_zip(docx_entries(plain_doc, package_rels_override=external_rels)))

    # A duplicate part name (identity ambiguity -> typed decline).
    add("docx_duplicate_part.docx", "opc", "reject",
        build_zip(docx_entries(plain_doc,
                               story_names=["word/document.xml",
                                            "word/document.xml"])))

    # A part name that tries to traverse out of the package root.
    add("docx_traversal_part.docx", "opc", "reject",
        raw_zip([
            dict(name="[Content_Types].xml",
                 data=docx_entries(plain_doc)[0][1], method=STORE),
            dict(name="_rels/.rels", data=docx_entries(plain_doc)[1][1],
                 method=STORE),
            dict(name="../../etc/passwd", data=b"x", method=STORE),
        ]))

    # --- EPUB OCF/package faults -------------------------------------------
    chapter = (
        '<?xml version="1.0" encoding="UTF-8"?>'
        '<html xmlns="http://www.w3.org/1999/xhtml"><head><title>c</title>'
        "</head><body><h1>Hostile</h1><p>body text</p></body></html>"
    ).encode()
    add("epub_ok.epub", "epub", "accept", build_zip(epub_entries(chapter)))

    # Malformed container.xml.
    add("epub_bad_container.epub", "epub", "reject",
        build_zip(epub_entries(chapter,
                               container_override=b"<container><not xml")))

    # container.xml points at a package document that does not exist.
    add("epub_missing_opf.epub", "epub", "reject",
        build_zip(epub_entries(chapter, rootfile_path="OEBPS/absent.opf")))

    # malformed package document.
    add("epub_bad_opf.epub", "epub", "reject",
        build_zip(epub_entries(chapter, opf_override=b"<package><oops")))

    # A scripted XHTML content document (script is data, never executed).
    scripted = (
        b'<?xml version="1.0" encoding="UTF-8"?>'
        b'<html xmlns="http://www.w3.org/1999/xhtml"><head><title>s</title></head>'
        b'<body onload="evil()"><script>fetch("http://evil.example/x")</script>'
        b'<p>text</p></body></html>'
    )
    add("epub_scripted.epub", "epub", "accept",
        build_zip(epub_entries(scripted)))

    # A remote resource reference (inert string, never fetched).
    remote = (
        b'<?xml version="1.0" encoding="UTF-8"?>'
        b'<html xmlns="http://www.w3.org/1999/xhtml"><head><title>r</title></head>'
        b'<body><img src="https://evil.example/pixel.png"/>'
        b'<p>remote</p></body></html>'
    )
    add("epub_remote.epub", "epub", "accept",
        build_zip(epub_entries(remote)))

    # DOCTYPE in the XHTML content document.
    doctype_xhtml = (
        b'<?xml version="1.0"?><!DOCTYPE html [<!ENTITY e "x">]>'
        b'<html xmlns="http://www.w3.org/1999/xhtml"><head><title>d</title></head>'
        b'<body><p>&e;</p></body></html>'
    )
    add("epub_doctype_xhtml.epub", "epub", "reject",
        build_zip(epub_entries(doctype_xhtml)))

    # spine references a manifest id that is absent (typed inconsistency).
    bad_opf = (
        '<?xml version="1.0" encoding="UTF-8"?>'
        '<package xmlns="http://www.idpf.org/2007/opf" version="3.0" '
        'unique-identifier="pub-id">'
        '<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">'
        '<dc:identifier id="pub-id">urn:uuid:x</dc:identifier>'
        "<dc:title>t</dc:title><dc:language>en</dc:language></metadata>"
        "<manifest>"
        '<item id="ch1" href="chapter1.xhtml" '
        'media-type="application/xhtml+xml"/>'
        "</manifest>"
        '<spine><itemref idref="ghost"/></spine>'
        "</package>"
    ).encode()
    add("epub_spine_inconsistent.epub", "epub", "reject",
        build_zip(epub_entries(chapter, opf_override=bad_opf)))

    # mimetype is present but carries the wrong payload (conformance decline).
    add("epub_bad_mimetype.epub", "epub", "accept",
        build_zip(epub_entries(chapter, mimetype=b"text/plain")))

    # An encryption.xml annotation (encrypted/obfuscated resources preserved).
    encryption = (
        '<?xml version="1.0"?>'
        '<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container">'
        '<EncryptedData>'
        '<CipherData><CipherReference URI="OEBPS/chapter1.xhtml"/></CipherData>'
        "</EncryptedData></encryption>"
    ).encode()
    add("epub_encrypted.epub", "epub", "accept",
        build_zip(epub_entries(chapter, extra=[
            ("META-INF/encryption.xml", encryption, DEFLATE),
        ])))

    # An oversized declared text flood (declared, never allocated).
    huge_doc = (
        b'<?xml version="1.0"?>'
        + f'<w:document xmlns:w="{W_NS}"><w:body><w:p><w:r><w:t>'.encode()
        + b"A" * (1 << 20)
        + b"</w:t></w:r></w:p></w:body></w:document>"
    )
    add("docx_oversized_text.docx", "xml", "accept",
        build_zip(docx_entries(huge_doc)))

    return cases


def main():
    if len(sys.argv) != 2:
        print("usage: phase12-hostile-gen.py OUTDIR", file=sys.stderr)
        return 2
    outdir = sys.argv[1]
    os.makedirs(outdir, exist_ok=True)

    manifest = {"generator": "phase12-hostile-gen.py", "fixtures": {}}
    total = 0
    for name, category, expect, data in fixture_cases():
        path = os.path.join(outdir, name)
        with open(path, "wb") as f:
            f.write(data)
        manifest["fixtures"][name] = {
            "category": category,
            "expect": expect,
            "length": len(data),
            "sha256": hashlib.sha256(data).hexdigest(),
        }
        total += len(data)
        print(f"  {name:32s} {category:14s} {expect:8s} {len(data):6d} B")

    manifest["fixture_count"] = len(manifest["fixtures"])
    manifest["total_bytes"] = total
    with open(os.path.join(outdir, "manifest.json"), "w") as f:
        json.dump(manifest, f, indent=2, sort_keys=True)
        f.write("\n")
    print(f"wrote {len(manifest['fixtures'])} fixtures ({total} bytes) to {outdir}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
