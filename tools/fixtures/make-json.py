#!/usr/bin/env python3
# Phase 21.5.1 — deterministic JSON fixture generator (Python stdlib only).
#
# Emits a small, fixed family of JSON documents that exercise the
# representation-preserving surface the JSON adapter claims: nested
# objects/arrays, unicode escapes, large/small numbers with varied spelling,
# duplicate keys, deep nesting, whitespace variety, a top-level scalar, a
# multi-megabyte document, and one malformed control (which must NOT be detected
# as JSON). Everything is deterministic (no randomness, no clock).
#
#   python3 tools/fixtures/make-json.py                 # write tools/fixtures/json/
#   python3 tools/fixtures/make-json.py --corpus DIR    # write DIR/, print TSV
#
# The TSV is `name<TAB>bytes<TAB>sha256` per fixture, matching make-pptx.py.

import hashlib
import os
import sys

# --- fixtures ---------------------------------------------------------------


def basic():
    # Nested objects/arrays with varied whitespace (spaces, tabs, newlines).
    return (
        '{\n'
        '  "name": "basic",\n'
        '  "count": 3,\n'
        '  "nested": {"a": [1, 2, {"b": true, "c": null}],\n'
        '             "d": "x"},\n'
        '\t"list": [ 1,\t2 , 3 ],\n'
        '  "empty_obj": {},\n'
        '  "empty_arr": []\n'
        '}\n'
    ).encode("utf-8")


def unicode_doc():
    # Escape spelling is preserved: `\u00e9` is distinct from the literal `é`.
    return (
        '{"emoji":"\\ud83d\\ude00",'
        '"accent":"\\u00e9",'
        '"raw":"é",'
        '"tab":"a\\tb",'
        '"quote":"say \\"hi\\"",'
        '"solidus":"a\\/b"}'
    ).encode("utf-8")


def numbers():
    # Big/small numbers with varied spelling; never parsed into a binary float.
    return (
        '{"big":123456789012345678901234567890,'
        '"small":1e-10,'
        '"zero":-0,'
        '"float":1.0,'
        '"exp":1e3,'
        '"neg":-42.5,'
        '"cap":6.022e23,'
        '"frac":0.0001}'
    ).encode("utf-8")


def duplicates():
    # Duplicate keys are kept distinct (never dropped or overwritten), in order.
    return (
        '{"a":1,"b":2,"a":3,"c":{"x":1,"x":2},"a":4}'
    ).encode("utf-8")


def deep():
    # Container nesting depth 80: within the DEFAULT cap (256), over STRICT (64).
    depth = 80
    body = "0"
    for i in range(depth):
        if i % 2 == 0:
            body = "[" + body + "]"
        else:
            body = '{"k":' + body + "}"
    return body.encode("utf-8")


def scalar():
    # A top-level scalar (a number) — a whole document that is one value.
    return b"1234567890"


def large():
    # ~2 MB: a marker plus an array of objects with string and numeric payloads.
    rows = []
    target = 2 * 1024 * 1024
    size = 0
    i = 0
    while size < target:
        row = ('{"id":%d,"name":"item-%d","vals":[%d,%d,%d],'
               '"flag":%s}' % (i, i, i, i + 1, i + 2,
                                 "true" if i % 2 == 0 else "false"))
        rows.append(row)
        size += len(row) + 1
        i += 1
    body = ",\n".join(rows)
    return ('{"marker":"ZEBRA_MARKER_21_5","items":[' + body + "]}").encode("utf-8")


def malformed():
    # An unterminated array: NOT valid JSON (must detect as Opaque).
    return b'{"a": 1, "b": [1, 2'


# Files written by default (into tools/fixtures/json/) and by --corpus.
FIXTURES = [
    ("basic.json", basic),
    ("unicode.json", unicode_doc),
    ("numbers.json", numbers),
    ("dup.json", duplicates),
    ("deep.json", deep),
    ("scalar.json", scalar),
    ("large.json", large),
    ("malformed.json", malformed),
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
    out = os.path.join(here, "json")
    os.makedirs(out, exist_ok=True)
    for name, fn in FIXTURES:
        path = os.path.join(out, name)
        data = fn()
        _write(path, data)
        print("%s\t%d bytes" % (name, len(data)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
