#!/usr/bin/env python3
# Phase 21.11 — deterministic TOML fixture generator (Python stdlib only).
#
# Emits a small, fixed family of TOML documents that exercise the
# representation-preserving surface the TOML adapter claims: basic and literal
# strings, integers with `_`/`0x`/`0o`/`0b`, floats including `inf`/`nan`,
# booleans, offset/local date-times, arrays, inline tables, dotted keys, tables,
# arrays of tables, and comments; a multi-megabyte document; and two controls that
# must NOT be detected as TOML (plain prose, and a duplicate-key document that
# violates TOML's redefinition rules). Everything is deterministic (no randomness,
# no clock).
#
#   python3 tools/fixtures/make-toml.py                 # write tools/fixtures/toml/
#   python3 tools/fixtures/make-toml.py --corpus DIR    # write DIR/, print TSV
#
# The TSV is `name<TAB>bytes<TAB>sha256` per fixture, matching make-html.py.

import hashlib
import os
import sys


def basic():
    return (
        '# basic configuration\n'
        'title = "basic"\n'
        'count = 1_000\n'
        'ratio = 3.14\n'
        'enabled = true\n'
        'created = 1979-05-27T07:32:00Z\n'
        'hex = 0x1F\n'
        'list = [1, 2, 3]\n'
        '\n'
        '[server]\n'
        'host = "localhost"\n'
        'port = 8080\n'
        'tags = ["a", "b"]\n'
    ).encode("utf-8")


def tables():
    # Nested tables and dotted keys create the tables they name.
    return (
        'name = "tables"\n'
        'a.b.c = 1\n'
        'a.b.d = 2\n'
        '\n'
        '[owner]\n'
        'name = "Ada"\n'
        '\n'
        '[owner.address]\n'
        'city = "London"\n'
        'zip = "N1"\n'
        '\n'
        '[database]\n'
        'enabled = true\n'
        'ports = [8000, 8001, 8002]\n'
        'data = [["delta", "phi"], [3.14, 2.71]]\n'
    ).encode("utf-8")


def arrays():
    # Arrays of tables, including a nested one.
    out = ['name = "arrays"\n', "total = 7\n", "active = true\n"]
    out.append("\n[meta]\n")
    out.append('kind = "arrays"\n')
    for i in range(3):
        out.append("\n[[products]]\n")
        out.append('name = "product-%d"\n' % i)
        out.append("sku = %d\n" % (1000 + i))
        out.append('colors = ["red", "green"]\n')
    out.append("\n[[products.variants]]\n")
    out.append('name = "variant-0"\n')
    out.append("\n[[products.variants]]\n")
    out.append('name = "variant-1"\n')
    return "".join(out).encode("utf-8")


def inline():
    # Inline tables, including a dotted key inside one.
    return (
        'name = "inline"\n'
        'marker = "inline-marker"\n'
        'active = true\n'
        'point = { x = 1, y = 2 }\n'
        'nested = { a.b = 1, a.c = 2, d = true }\n'
        'empty = {}\n'
        'servers = [{ host = "a", port = 1 }, { host = "b", port = 2 }]\n'
    ).encode("utf-8")


def scalars():
    # Every scalar type with varied, preserved spelling.
    return (
        'name = "scalars"\n'
        'basic = "a\\tb\\nc"\n'
        'literal = \'C:\\Users\\nodejs\'\n'
        'multiline = """\nline one\nline two"""\n'
        'multiline_literal = \'\'\'\nraw \\n not escaped\'\'\'\n'
        'dec = 1_000_000\n'
        'hex = 0xDEAD_beef\n'
        'oct = 0o755\n'
        'bin = 0b1010_1010\n'
        'neg = -17\n'
        'pos = +99\n'
        'float = 1.0\n'
        'exp = 1e3\n'
        'frac = 0.0001\n'
        'special = nan\n'
        'big = inf\n'
        'nbig = -inf\n'
        'yes = true\n'
        'no = false\n'
        'odt = 1979-05-27T07:32:00Z\n'
        'ldt = 1979-05-27T07:32:00\n'
        'ld = 1979-05-27\n'
        'lt = 07:32:00\n'
        '\n'
        '[info]\n'
        'note = "ok"\n'
    ).encode("utf-8")


def comments():
    return (
        '# leading comment\n'
        'name = "comments"  # trailing comment\n'
        'ok = true\n'
        '\n'
        '# a whole-line comment between entries\n'
        'value = 42 # another\n'
        '\n'
        '[section] # header comment\n'
        'k = "v" # value comment\n'
    ).encode("utf-8")


def large():
    # ~2 MB of table rows with strings, integers, and comments.
    rows = []
    target = 2 * 1024 * 1024
    size = 0
    i = 0
    while size < target:
        row = ('\n[[items]]\n'
               'id = %d # row %d\n'
               'name = "item-%d"\n'
               'value = %d_%03d\n'
               'ratio = %d.%d\n'
               'tags = ["t%d", "t%d"]\n'
               % (i, i, i, 1000 + i, i % 1000, i, i % 100, i, i + 1))
        rows.append(row)
        size += len(row)
        i += 1
    body = "".join(rows)
    return ('# large TOML fixture\n'
            'title = "large"\n'
            'count = 10_000\n'
            'active = true\n'
            'marker = "ZEBRA_MARKER_21_11"\n'
            '\n'
            '[meta]\n'
            'kind = "large"\n'
            + body).encode("utf-8")


def prose():
    # Plain prose: no assignment, must stay Opaque (not TOML).
    return b"This is a plain prose paragraph. It has no key-value pairs at all.\n"


def dup():
    # A duplicate key violates TOML's rules and must NOT be detected as TOML.
    return b'name = "dup"\na = 1\na = 2\n'


def junk():
    # `<-prefixed` junk / a bare word are not TOML.
    return b"<- not a document at all ->\n"


FIXTURES = [
    ("basic.toml", basic),
    ("tables.toml", tables),
    ("arrays.toml", arrays),
    ("inline.toml", inline),
    ("scalars.toml", scalars),
    ("comments.toml", comments),
    ("large.toml", large),
    ("prose.txt", prose),
    ("dup.toml", dup),
    ("junk.toml", junk),
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
    out = os.path.join(here, "toml")
    os.makedirs(out, exist_ok=True)
    for name, fn in FIXTURES:
        path = os.path.join(out, name)
        data = fn()
        _write(path, data)
        print("%s\t%d bytes" % (name, len(data)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
