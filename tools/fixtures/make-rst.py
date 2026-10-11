#!/usr/bin/env python3
# Phase 21.26.1 — deterministic reStructuredText (reST) fixture generator (Python
# stdlib only). Exercises the surface the reST adapter claims: section titles with
# their exact underline adornment, paragraphs, explicit markup (comments,
# hyperlink targets, directives, footnotes, substitution definitions), field/
# option/definition lists, literal (`::`) and doctest blocks, bullet/enumerated
# lists with nesting, inline markup, and grid/simple tables. Controls pin the
# detection boundary: plain prose stays `opaque` and a Markdown document stays
# `markdown`.
#
#   python3 tools/fixtures/make-rst.py --corpus DIR   # write DIR/, print TSV

import hashlib
import os
import sys


def basic():
    return (b"Title\n"
            b"=====\n"
            b"\n"
            b"A paragraph with **strong**, *emphasis*, ``literal``, and `role`:code: text.\n"
            b"\n"
            b"Section Two\n"
            b"^^^^^^^^^^^\n"
            b"\n"
            b"A body paragraph.\n")


def explicit():
    return (b"Explicit\n"
            b"========\n"
            b"\n"
            b".. a comment line\n"
            b"\n"
            b".. _tgt: https://example.org\n"
            b"\n"
            b".. note:: directive body\n"
            b"\n"
            b".. [1] footnote body\n"
            b"\n"
            b".. |sub| replace:: replacement\n"
            b"\n"
            b"body text.\n")


def lists():
    return (b"Lists\n"
            b"=====\n"
            b"\n"
            b"- alpha\n"
            b"- beta\n"
            b"\n"
            b"1. one\n"
            b"2. two\n"
            b"\n"
            b"- outer\n"
            b"  - inner\n"
            b"\n"
            b"Term\n"
            b"  definition body\n"
            b"\n"
            b":author: me\n"
            b"\n"
            b"-a, --all  do all\n"
            b"\n"
            b"A closing paragraph.\n")


def tables():
    return (b"Tables\n"
            b"======\n"
            b"\n"
            b"+---+---+\n"
            b"| a | b |\n"
            b"+---+---+\n"
            b"| 1 | 2 |\n"
            b"+---+---+\n"
            b"\n"
            b"=====  =====\n"
            b"colA   colB\n"
            b"=====  =====\n"
            b"1      2\n"
            b"=====  =====\n"
            b"\n"
            b"A closing paragraph.\n")


def inline():
    return (b"Inline\n"
            b"======\n"
            b"\n"
            b"**strong** *emphasis* ``literal`` `role`:code: |sub| [1]_ `lbl`_ and name_\n")


def literal():
    return (b"Literal\n"
            b"=======\n"
            b"\n"
            b"A paragraph ending here::\n"
            b"\n"
            b"    indented literal body\n"
            b"\n"
            b">>> print(1)\n"
            b"1\n")


def large():
    out = [b"Big\n", b"===\n", b"\n",
           b"Alpha\n", b"^^^^^\n", b"\n"]
    size = sum(len(x) for x in out)
    i = 0
    while size < 256 * 1024:
        para = ("Paragraph number %d carries some ordinary words and "
                "punctuation, long enough to be a realistic block of prose."
                % i).encode("utf-8")
        out.append(para)
        out.append(b"\n\n")
        size += len(para) + 2
        i += 1
    return b"".join(out)


# --- controls ---------------------------------------------------------------

def prose():
    return (b"This is just some prose text.\n"
            b"It has several lines, with punctuation,\n"
            b"but no reST directive, adornment, table, or field at all.\n")


def markdown_md():
    return (b"# Heading\n"
            b"\n"
            b"A paragraph with [link](http://x).\n"
            b"\n"
            b"| a | b |\n"
            b"| - | - |\n"
            b"| 1 | 2 |\n")


LANE = [
    ("basic.rst", basic),
    ("explicit.rst", explicit),
    ("lists.rst", lists),
    ("tables.rst", tables),
    ("inline.rst", inline),
    ("literal.rst", literal),
    ("large.rst", large),
]

CONTROLS = [
    ("prose.txt", prose),
    ("markdown.md", markdown_md),
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
    print("usage: make-rst.py --corpus DIR", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
