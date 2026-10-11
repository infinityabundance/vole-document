#!/usr/bin/env python3
# Phase 21.26.2 — deterministic AsciiDoc (Asciidoctor input language) fixture
# generator (Python stdlib only). Exercises the surface the AsciiDoc adapter
# claims: a level-0 document title and `==`+ sections, document attributes and
# literal `{ref}` references, delimited blocks (listing/literal/example/sidebar/
# quote/open/passthrough), lists, `|===` tables with `[cols=...]`, admonitions,
# and inline markup plus macros. Controls pin the boundary: plain prose stays
# `opaque`, Markdown stays `markdown`, reStructuredText stays `rst`, and the
# spaced-attribute form is a reST field list (claimed by reST, tried first).
#
#   python3 tools/fixtures/make-asciidoc.py --corpus DIR   # write DIR/, print TSV

import hashlib
import os
import sys


def basic():
    return (b"= AsciiDoc Primer\n"
            b"\n"
            b"A paragraph with *strong*, _emphasis_, and `mono` text.\n"
            b"\n"
            b"== First Section\n"
            b"\n"
            b"A body paragraph with +pass+ and ^sup^ and ~sub~ and #mark#.\n"
            b"\n"
            b"=== Subsection\n"
            b"\n"
            b"More body text.\n")


def attributes():
    return (b"= Attributes\n"
            b"\n"
            b":author:Jane Doe\n"
            b":version:1.0\n"
            b"\n"
            b"The attribute {author} stays literal.\n")


def delimited():
    return (b"= Delimited\n"
            b"\n"
            b"----\n"
            b"listing one\n"
            b"listing two\n"
            b"----\n"
            b"\n"
            b"....\n"
            b"literal one\n"
            b"literal two\n"
            b"....\n"
            b"\n"
            b"====\n"
            b"example one\n"
            b"example two\n"
            b"====\n"
            b"\n"
            b"****\n"
            b"sidebar one\n"
            b"sidebar two\n"
            b"****\n"
            b"\n"
            b"____\n"
            b"quote one\n"
            b"quote two\n"
            b"____\n"
            b"\n"
            b"--\n"
            b"open one\n"
            b"open two\n"
            b"--\n"
            b"\n"
            b"++++\n"
            b"passthrough one\n"
            b"passthrough two\n"
            b"++++\n"
            b"\n"
            b"A closing paragraph.\n")


def lists():
    return (b"= Lists\n"
            b"\n"
            b"* item one\n"
            b"* item two\n"
            b"\n"
            b". first\n"
            b". second\n"
            b"\n"
            b"term:: a definition body\n"
            b"\n"
            b"[source,rust]\n"
            b"----\n"
            b"fn main() {}\n"
            b"----\n"
            b"\n"
            b"A closing paragraph.\n")


def tables():
    return (b"= Tables\n"
            b"\n"
            b"[cols=\"2\"]\n"
            b"|===\n"
            b"| Name | Value\n"
            b"| alpha | 1\n"
            b"| beta | 2\n"
            b"|===\n"
            b"\n"
            b"A closing paragraph.\n")


def inline():
    return (b"= Inline\n"
            b"\n"
            b"Text with *strong*, _emphasis_, `mono`, +pass+, ^sup^, ~sub~, #mark#, and ##marked##.\n"
            b"\n"
            b"See link:https://example.org[Example] and image:logo.png[Logo] and "
            b"include::chapter.adoc[] and xref:sec-1[One] and "
            b"https://bare.example[Site] and {name}.\n")


def large():
    out = [b"= Big\n", b"\n", b"== Alpha\n", b"\n"]
    size = sum(len(x) for x in out)
    i = 0
    while size < 256 * 1024:
        para = ("A paragraph numbered %d with ordinary words, punctuation, and "
                "enough length to read as prose.\n\n" % i).encode("utf-8")
        out.append(para)
        size += len(para)
        i += 1
    return b"".join(out)


# --- controls ---------------------------------------------------------------

def prose():
    return (b"This is just some prose text.\n"
            b"It has several lines, with punctuation,\n"
            b"but no AsciiDoc title, section, table, or delimited block at all.\n")


def markdown_md():
    return (b"# Heading\n"
            b"\n"
            b"A paragraph with [link](http://x).\n"
            b"\n"
            b"| a | b |\n"
            b"| - | - |\n"
            b"| 1 | 2 |\n")


def rest():
    return b"Title\n=====\n\n.. note:: a directive body\n"


def attributes_spaced():
    return (b"= Attributes\n"
            b"\n"
            b":author: Jane Doe\n"
            b"\n"
            b"The attribute {author} stays literal.\n")


LANE = [
    ("basic.adoc", basic),
    ("attributes.adoc", attributes),
    ("delimited.adoc", delimited),
    ("lists.adoc", lists),
    ("tables.adoc", tables),
    ("inline.adoc", inline),
    ("large.adoc", large),
]

CONTROLS = [
    ("prose.txt", prose),
    ("markdown.md", markdown_md),
    ("rest.rst", rest),
    ("spaced.adoc", attributes_spaced),
]

FIXTURES = LANE + CONTROLS


def emit_corpus(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    for name, fn in FIXTURES:
        data = fn()
        with open(os.path.join(out_dir, name), "wb") as f:
            f.write(data)
        print("%s\t%d\t%s" % (name, len(data), hashlib.sha256(data).hexdigest()))


def main():
    if len(sys.argv) >= 3 and sys.argv[1] == "--corpus":
        emit_corpus(sys.argv[2])
        return 0
    print("usage: make-asciidoc.py --corpus DIR", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
