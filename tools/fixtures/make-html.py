#!/usr/bin/env python3
# Phase 21.10 — deterministic HTML fixture generator (Python stdlib only).
#
# Emits a small, fixed family of HTML documents that exercise the
# representation-preserving, error-recovering surface the HTML adapter claims:
# a full document with a DOCTYPE, nested elements, attributes (double-quoted,
# single-quoted, unquoted, and boolean), void elements, comments, raw
# `<script>`/`<style>` content, entity references left literal, a multi-megabyte
# document, and a malformed-but-recoverable document that must parse (never a
# panic). It also emits the controls that must NOT be detected as HTML: plain
# prose, non-HTML `<`-junk, a DOCTYPE internal subset (an entity surface), and a
# fully well-formed XHTML document (which precedence keeps as XML).
# Everything is deterministic (no randomness, no clock).
#
#   python3 tools/fixtures/make-html.py                 # write tools/fixtures/html/
#   python3 tools/fixtures/make-html.py --corpus DIR    # write DIR/, print TSV
#
# The TSV is `name<TAB>bytes<TAB>sha256` per fixture, matching make-xml.py.

import hashlib
import os
import sys

HEAD = '<!DOCTYPE html>\n<html lang="en">\n<head>\n<meta charset="utf-8">\n<title>%s</title>\n</head>\n'


def basic():
    return (""
            + HEAD % "basic"
            + '<body id="main">\n'
            + "<h1>Basic</h1>\n"
            + '<p class="lead">hello<br>world</p>\n'
            + "<!-- a comment -->\n"
            + '<a href="http://example.invalid/basic">link text</a>\n'
            + '<input type="text" disabled>\n'
            + "<script>var basic = 1;</script><style>body{color:red}</style>\n"
            + "</body>\n</html>\n").encode("utf-8")


def elements():
    return (""
            + HEAD % "elements"
            + '<body id="body" data-role=main hidden>\n'
            + "<h1>Elements</h1>\n"
            + '<div class="a"><span>x</span><p>y<br>z</p></div>\n'
            + '<div class="b"><a href="http://example.invalid/elements">link</a></div>\n'
            + "<script>var elements = 2;</script><style>.a{color:blue}</style>\n"
            + "</body>\n</html>\n").encode("utf-8")


def rawtext():
    # A `<script>` whose content contains markup-like text: it must be captured
    # raw and never parsed as an element or executed. `</div>` and `<b>` inside the
    # script must NOT close/open anything.
    return (""
            + HEAD % "rawtext"
            + '<body id="r">\n'
            + "<h1>Rawtext</h1>\n"
            + '<script>var s = "</div><b>no</b>"; if (1 < 2 && 3 > 2) { alert(s); }</script>'
            + "<style>.x{content:'</div>'}</style>\n"
            + '<a href="http://example.invalid/rawtext">r</a>\n'
            + "<p>after</p>\n"
            + "</body>\n</html>\n").encode("utf-8")


def entities():
    # Entity references are surfaced literally (never expanded).
    return (""
            + HEAD % "entities"
            + '<body id="e">\n'
            + "<h1>Entities</h1>\n"
            + "<p>a &amp; b &lt; c &nbsp; d</p>\n"
            + '<a href="http://example.invalid/entities">e</a>\n'
            + "<script>var e = 3;</script><style>.e{color:green}</style>\n"
            + "</body>\n</html>\n").encode("utf-8")


def malformed():
    # Malformed but recoverable: an unclosed block, implicit `<li>` closing, a
    # stray end tag, and a document that never closes. Must parse, never panic.
    return (""
            + HEAD % "malformed"
            + '<body id="m">\n'
            + "<h1>Malformed</h1>\n"
            + "<ul><li>one<li>two<li>three\n"
            + "<div><p>x</span>y\n"
            + '<a href="http://example.invalid/malformed">m</a>\n'
            + "<script>var m = 4;</script><style>.m{color:black}</style>\n").encode("utf-8")


def large():
    # ~2 MB of record rows with attributes and nested text.
    rows = []
    target = 2 * 1024 * 1024
    size = 0
    i = 0
    while size < target:
        row = ('<div class="row" data-i="%d"><h2>item-%d</h2><p>value %d</p>'
               '<br></div>\n' % (i, i, i))
        rows.append(row)
        size += len(row)
        i += 1
    body = "".join(rows)
    return (""
            + '<!DOCTYPE html>\n<html lang="en"><head><meta charset="utf-8">'
            + "<title>large</title></head>\n"
            + '<body id="L" data-marker="ZEBRA_MARKER_21_10">\n'
            + "<h1>Large</h1>\n"
            + '<a href="http://example.invalid/large">L</a>\n'
            + body
            + "<script>var L = 5;</script><style>.L{color:gray}</style>\n"
            + "</body></html>\n").encode("utf-8")


def xhtml():
    # A fully well-formed XHTML document: XML is tried before HTML, so this stays
    # XML (precedence). It is therefore NOT detected as HTML.
    return ('<?xml version="1.0"?>\n'
            '<html xmlns="http://www.w3.org/1999/xhtml">\n'
            "<head><title>xhtml</title></head>\n"
            "<body><p>fully closed</p></body>\n"
            "</html>\n").encode("utf-8")


def prose():
    # Plain prose: no structure at all, must stay Opaque (not HTML).
    return b"This is a plain prose paragraph. It has no markup and no tags.\n"


def junk():
    # `<-prefixed` junk that carries only a couple of tag-like fragments: must stay
    # Opaque (the preponderance gate is not met).
    return b"<<< not html at all >>> <a <b> <<"


def xxe():
    # A DOCTYPE carrying an internal subset (where entities are declared) is refused
    # outright: the document is NOT detected as HTML and no entity is ever expanded.
    return ('<!DOCTYPE html [<!ENTITY xxe "boom">]>\n'
            "<html><body><p>&xxe;</p></body></html>\n").encode("utf-8")


FIXTURES = [
    ("basic.html", basic),
    ("elements.html", elements),
    ("rawtext.html", rawtext),
    ("entities.html", entities),
    ("malformed.html", malformed),
    ("large.html", large),
    ("xhtml.html", xhtml),
    ("prose.txt", prose),
    ("junk.html", junk),
    ("xxe.html", xxe),
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
    out = os.path.join(here, "html")
    os.makedirs(out, exist_ok=True)
    for name, fn in FIXTURES:
        path = os.path.join(out, name)
        data = fn()
        _write(path, data)
        print("%s\t%d bytes" % (name, len(data)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
