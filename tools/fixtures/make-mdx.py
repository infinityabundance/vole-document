#!/usr/bin/env python3
# Phase 21.26.3 — deterministic MDX (Markdown + JSX/ESM) fixture generator
# (Python stdlib only). Exercises the surface the MDX adapter claims on top of the
# reused Markdown model: ESM `import`/`export` statements, JSX elements and
# fragments with attributes and nested children, MDX `{ … }` expressions (block,
# inline, comment), and the full Markdown surface. Controls pin the boundary:
# plain prose stays `opaque`, a brace-bearing blob with no Markdown structural
# mark stays `opaque`, Markdown stays `markdown` (including when inline code or an
# inline expression merely looks like MDX), and HTML stays `html`.
#
#   python3 tools/fixtures/make-mdx.py --corpus DIR   # write DIR/, print TSV

import hashlib
import os
import sys


def esm():
    return ('import React, {\n'
            '  useState,\n'
            '} from "react"\n'
            'export default function App() { return null }\n'
            'export const VERSION = "1.0"\n'
            '\n'
            '# ESM\n'
            '\n'
            'Body paragraph.\n').encode("utf-8")


def jsx():
    return ('# JSX\n'
            '\n'
            '<Widget name="w" count={3}>\n'
            '  <Item.Child />\n'
            '  {value}\n'
            '</Widget>\n'
            '\n'
            '<>\n'
            'frag\n'
            '</>\n'
            '\n'
            'A closing paragraph.\n').encode("utf-8")


def expr():
    return ('import config from "./c"\n'
            '\n'
            '# Expr\n'
            '\n'
            '{frontmatter.title}\n'
            '\n'
            '{/* a comment */}\n'
            '\n'
            'Text with {inline.value} inside.\n').encode("utf-8")


def mixed():
    return ('# Mixed\n'
            '\n'
            'A paragraph before.\n'
            '\n'
            '<Card title={x}>\n'
            '  **bold** child text\n'
            '</Card>\n'
            '\n'
            '{summary}\n'
            '\n'
            'After.\n').encode("utf-8")


def surface():
    return ('# Surface\n'
            '\n'
            'A paragraph with *emphasis*, `code`, and [a link](http://ex).\n'
            '\n'
            '- item one\n'
            '- item two\n'
            '\n'
            '```rust\n'
            'let x = 1;\n'
            '```\n'
            '\n'
            '| a | b |\n'
            '| - | - |\n'
            '| 1 | 2 |\n'
            '\n'
            '<Meta />\n'
            '\n'
            '[ref]: http://ref\n'
            '\n'
            'see [text][ref].\n'
            '\n'
            '[^n]: a note\n').encode("utf-8")


def large():
    out = ['import React from "react"\n', '\n', '# Big\n', '\n', '## Alpha\n', '\n']
    size = sum(len(x) for x in out)
    i = 0
    while size < 256 * 1024:
        para = ("A paragraph numbered %d with ordinary words, punctuation, and "
                "enough length to read as prose.\n\n" % i)
        out.append(para)
        size += len(para)
        i += 1
    return "".join(out).encode("utf-8")


# --- controls ---------------------------------------------------------------

def prose():
    return ('This is just some prose text.\n'
            'It has several lines, with punctuation,\n'
            'but no heading, list, or any markup at all.\n').encode("utf-8")


def markdown_md():
    return ('# Heading\n'
            '\n'
            'A paragraph with [link](http://x).\n').encode("utf-8")


def markdown_code():
    return '# Heading\n\nuse `<Foo />` here\n'.encode("utf-8")


def inline_expr():
    return '# Heading\n\nvalue is {x} here\n'.encode("utf-8")


def html():
    return ('<!doctype html>\n'
            '<html><body><h1>Hi</h1></body></html>\n').encode("utf-8")


def braces():
    return '{"a":1}\n{oops}\n'.encode("utf-8")


LANE = [
    ("esm.mdx", esm),
    ("jsx.mdx", jsx),
    ("expr.mdx", expr),
    ("mixed.mdx", mixed),
    ("surface.mdx", surface),
    ("large.mdx", large),
]

CONTROLS = [
    ("prose.txt", prose),
    ("markdown.md", markdown_md),
    ("markdown_code.md", markdown_code),
    ("inline_expr.md", inline_expr),
    ("html.html", html),
    ("braces.txt", braces),
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
    print("usage: make-mdx.py --corpus DIR", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
