#!/usr/bin/env python3
# Phase 21.9 — deterministic XML fixture generator (Python stdlib only).
#
# Emits a small, fixed family of XML documents that exercise the
# representation-preserving surface the XML adapter claims: nested elements,
# attributes (single- and double-quoted), namespaces (default + prefixed),
# CDATA, comments, processing instructions, an XML declaration, a benign
# `<!DOCTYPE>`, entity references left literal, a multi-megabyte document, and
# the hostile/opaque controls that must NOT be detected as XML: an internal-subset
# DOCTYPE (billion-laughs / XXE surface), non-XML `<`-junk, and plain prose.
# Everything is deterministic (no randomness, no clock).
#
#   python3 tools/fixtures/make-xml.py                 # write tools/fixtures/xml/
#   python3 tools/fixtures/make-xml.py --corpus DIR    # write DIR/, print TSV
#
# The TSV is `name<TAB>bytes<TAB>sha256` per fixture, matching make-json.py.

import hashlib
import os
import sys


def basic():
    # Nested elements with attributes and varied whitespace.
    return (
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"
        "<doc>\n"
        "  <name>basic</name>\n"
        "  <count>3</count>\n"
        "  <nested>\n"
        "    <a id=\"1\"><b>deep</b></a>\n"
        "    <a id='2'><b>deeper</b></a>\n"
        "  </nested>\n"
        "  <empty attr=\"e\"/>\n"
        "</doc>\n"
    ).encode("utf-8")


def namespaces():
    # Two in-scope namespace declarations and a prefixed child element.
    return (
        "<root xmlns=\"urn:default\" xmlns:p=\"urn:p\">\n"
        "  <child>plain</child>\n"
        "  <p:child p:attr=\"pv\">prefixed</p:child>\n"
        "</root>\n"
    ).encode("utf-8")


def mixed():
    # XML declaration, comment, processing instruction, CDATA, and literal entities.
    return (
        "<?xml version=\"1.0\"?>\n"
        "<!-- a comment -->\n"
        "<?target some data?>\n"
        "<r>\n"
        "  <cdata><![CDATA[a < b && c > d]]></cdata>\n"
        "  <ents>a &amp; b &lt; c</ents>\n"
        "</r>\n"
    ).encode("utf-8")


def attrs():
    # Single- and double-quoted attribute values, entities left literal.
    return (
        "<r a=\"1\" b='2' c=\"a&amp;b\" d=\"\">\n"
        "  <x id='x1' flag=\"yes\"/>\n"
        "</r>\n"
    ).encode("utf-8")


def dtd():
    # A benign external-identifier DOCTYPE: accepted and ignored (never fetched),
    # and NO entity is resolved.
    return (
        "<!DOCTYPE root PUBLIC \"-//VOLE//DTD test//EN\" \"http://example.invalid/test.dtd\">\n"
        "<root><child>dtd</child></root>\n"
    ).encode("utf-8")


def large():
    # ~2 MB of record elements with attributes and nested text.
    rows = []
    target = 2 * 1024 * 1024
    size = 0
    i = 0
    while size < target:
        row = ('  <rec id="%d" flag="%s"><name>item-%d</name>'
               '<vals><v>%d</v><v>%d</v></vals></rec>\n'
               % (i, "on" if i % 2 == 0 else "off", i, i, i + 1))
        rows.append(row)
        size += len(row)
        i += 1
    body = "".join(rows)
    return ('<root marker="ZEBRA_MARKER_21_9">\n' + body + "</root>\n").encode("utf-8")


def xxe():
    # An internal-subset DOCTYPE declaring an external entity: the internal subset
    # is refused, so this is NOT detected as XML (the document stays Opaque) and
    # the entity is never resolved.
    return (
        "<!DOCTYPE foo [<!ENTITY xxe SYSTEM \"file:///etc/passwd\">]>\n"
        "<foo>&xxe;</foo>\n"
    ).encode("utf-8")


def billion_laughs():
    # An internal-subset DOCTYPE declaring exponential entities: refused, never
    # expanded. Not detected as XML.
    return (
        "<!DOCTYPE lolz [\n"
        "  <!ENTITY lol \"lol\">\n"
        "  <!ENTITY lol2 \"&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;\">\n"
        "  <!ENTITY lol3 \"&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;\">\n"
        "]>\n"
        "<lolz>&lol3;</lolz>\n"
    ).encode("utf-8")


def junk():
    # `<-prefixed` junk that is not well-formed XML: must stay Opaque.
    return b"<<< not xml at all >>> <a <b> <<"


def prose():
    # Plain prose: no `<` start, must stay Opaque.
    return b"This is a plain prose paragraph. It has no <tags> and no markup.\n"


FIXTURES = [
    ("basic.xml", basic),
    ("namespaces.xml", namespaces),
    ("mixed.xml", mixed),
    ("attrs.xml", attrs),
    ("dtd.xml", dtd),
    ("large.xml", large),
    ("xxe.xml", xxe),
    ("billion.xml", billion_laughs),
    ("junk.xml", junk),
    ("prose.txt", prose),
]


def _write(path, data):
    with open(path, "wb") as f:
        f.write(data)


def emit_corpus(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    manifest = []
    for name, fn in FIXTURES:
        path = os.path.join(out_dir, name)
        data = fn()
        _write(path, data)
        manifest.append((name, len(data), hashlib.sha256(data).hexdigest()))
    for name, length, sha in manifest:
        print("%s\t%d\t%s" % (name, length, sha))
    return manifest


def main():
    if len(sys.argv) >= 3 and sys.argv[1] == "--corpus":
        emit_corpus(sys.argv[2])
        return 0
    here = os.path.dirname(os.path.abspath(__file__))
    out = os.path.join(here, "xml")
    os.makedirs(out, exist_ok=True)
    for name, fn in FIXTURES:
        path = os.path.join(out, name)
        data = fn()
        _write(path, data)
        print("%s\t%d bytes" % (name, len(data)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
