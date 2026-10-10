#!/usr/bin/env python3
# Phase 21.8.1 — deterministic Markdown fixture generator (Python stdlib only).
#
# Emits a small, fixed family of Markdown documents that exercise the
# representation-preserving surface the Markdown adapter claims: ATX headings (a
# heading tree), paragraphs, ordered/unordered/nested lists, fenced code blocks with
# language tags, indented code, blockquotes, GFM tables, inline/reference links and
# images, reference definitions, footnotes, YAML and TOML front matter, and a
# ~3 MB file (to exercise bounded memory). Two controls must stay Opaque: plain
# prose (a Markdown paragraph, but with no structural mark) and a second prose
# blob. Everything is deterministic (no randomness, no clock).
#
#   python3 tools/fixtures/make-markdown.py                 # write tools/fixtures/markdown/
#   python3 tools/fixtures/make-markdown.py --corpus DIR    # write DIR/, print TSV
#
# The TSV is `name<TAB>bytes<TAB>sha256` per fixture, matching make-csv.py.

import hashlib
import os
import sys


def basic():
    return (
        "# Top Heading\n"
        "\n"
        "A first paragraph with *emphasis*, **strong**, and `code`.\n"
        "\n"
        "## Second Heading\n"
        "\n"
        "Another paragraph.\n"
        "\n"
        "### Third Heading\n"
        "\n"
        "Final paragraph.\n"
    ).encode("utf-8")


def lists():
    # A heading, an unordered list (first item carries a unique marker token), an
    # ordered list that restarts at 1, and a nested list (two-space indent).
    return (
        "# Lists\n"
        "\n"
        "- alpha LISTITEM_UNIQUE\n"
        "- beta\n"
        "\n"
        "1. one\n"
        "2. two\n"
        "\n"
        "- outer\n"
        "  - inner one\n"
        "  - inner two\n"
    ).encode("utf-8")


def code():
    return (
        "# Code\n"
        "\n"
        "```rust\n"
        "fn main() {\n"
        "    println!(\"hi\");\n"
        "}\n"
        "```\n"
        "\n"
        "```python\n"
        "print(\"hello\")\n"
        "```\n"
        "\n"
        "    indented code line\n"
        "    second line\n"
    ).encode("utf-8")


def table():
    return (
        "# Table\n"
        "\n"
        "| name | score |\n"
        "| --- | ---: |\n"
        "| alice | 10 |\n"
        "| bob | 20 |\n"
    ).encode("utf-8")


def links():
    return (
        "# Links\n"
        "\n"
        "An inline [link](https://example.com \"Example\") here.\n"
        "\n"
        "A reference [ref link][r1] and an image ![alt](img.png \"Img\").\n"
        "\n"
        "[r1]: https://example.org \"Ref Title\"\n"
    ).encode("utf-8")


def blockquotes():
    return (
        "# Quotes\n"
        "\n"
        "> first line\n"
        "> second line\n"
        "> > nested\n"
        "\n"
        "Done.\n"
    ).encode("utf-8")


def footnotes():
    return (
        "# Footnotes\n"
        "\n"
        "A claim[^1] and another[^note].\n"
        "\n"
        "[^1]: The first note.\n"
        "[^note]: The second note.\n"
    ).encode("utf-8")


def frontmatter():
    return (
        "---\n"
        "title: Front Matter\n"
        "author: tester\n"
        "---\n"
        "\n"
        "# After Front Matter\n"
        "\n"
        "Body text.\n"
    ).encode("utf-8")


def toml_frontmatter():
    return (
        "+++\n"
        "title = \"TOML Front Matter\"\n"
        "+++\n"
        "\n"
        "# TOML Body\n"
        "\n"
        "Text.\n"
    ).encode("utf-8")


def large(target=3 * 1024 * 1024):
    # A ~3 MB Markdown document: repeated heading + prose + code + link sections.
    out = ["# Large Fixture\n\n"]
    size = len(out[0])
    i = 0
    while size < target:
        block = (
            "## Section %d\n\n"
            "Paragraph number %d with some ordinary prose words to fill space.\n\n"
            "```text\n"
            "line %d\n"
            "```\n\n"
            "A [link](https://example.com/%d) and `code`.\n\n"
        ) % (i, i, i, i)
        out.append(block)
        size += len(block)
        i += 1
    return "".join(out).encode("utf-8")


def plain():
    # Plain prose: no heading, fence, table, reference definition, or footnote
    # definition -> NOT detected as Markdown -> Opaque.
    return (
        "This is just some prose text.\n"
        "It has several lines, with punctuation,\n"
        "but no heading, code fence, table, or link definition at all.\n"
    ).encode("utf-8")


def prose_control():
    # A second prose control (with a hyphen and a comma) that still has no
    # structural mark, so it must stay Opaque too. It deliberately avoids a
    # colon-followed-by-space, which YAML would read as a mapping.
    return (
        "Notes on nothing in particular.\n"
        "A hyphen - appears here, and no colon at all,\n"
        "so this is not a Markdown document.\n"
    ).encode("utf-8")


FIXTURES = [
    ("basic.md", basic),
    ("lists.md", lists),
    ("code.md", code),
    ("table.md", table),
    ("links.md", links),
    ("blockquotes.md", blockquotes),
    ("footnotes.md", footnotes),
    ("frontmatter.md", frontmatter),
    ("toml_frontmatter.md", toml_frontmatter),
    ("large.md", large),
    ("plain.txt", plain),
    ("prose.md", prose_control),
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
    out = os.path.join(here, "markdown")
    os.makedirs(out, exist_ok=True)
    for name, fn in FIXTURES:
        path = os.path.join(out, name)
        data = fn()
        _write(path, data)
        print("%s\t%d bytes" % (name, len(data)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
