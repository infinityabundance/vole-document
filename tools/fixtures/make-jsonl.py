#!/usr/bin/env python3
# Phase 21.12 — deterministic JSONL / NDJSON fixture generator (Python stdlib only).
#
# Emits a small, fixed family of newline-delimited JSON documents that exercise the
# per-line surface the JSONL adapter claims: records of varied shapes (objects,
# arrays, strings, numbers, booleans, null), nested shapes, duplicate keys **within**
# a record, numeric spelling (`1e3`, `-0`, `1.5e-3`), and unicode / escape spelling
# (`\u00e9`, a surrogate pair, a raw multi-byte character, `\t`/`\n`); CRLF
# terminators; blank and trailing lines; a multi-megabyte document; and three
# controls. Everything is deterministic (no randomness, no clock).
#
# The controls pin the detection boundaries:
#   * `single.json`   — **one** JSON value spread across several lines. It is NOT
#                       JSONL; it must stay `Json`.
#   * `malformed.ndjson` — a stream whose second line is not one JSON value. It must
#                       stay `Opaque`.
#   * `bag.ndjson`    — a bag of two JSON values on one line with a **non-newline**
#                       separator. It is NOT JSONL (the adapter declines it typed).
#
#   python3 tools/fixtures/make-jsonl.py                 # write tools/fixtures/jsonl/
#   python3 tools/fixtures/make-jsonl.py --corpus DIR    # write DIR/, print TSV
#
# The TSV is `name<TAB>bytes<TAB>sha256` per fixture, matching make-toml.py.

import hashlib
import os
import sys


def basic():
    return (
        '{"id":1,"name":"alpha","tags":["a","b"],"active":true}\n'
        '{"id":2,"name":"beta","tags":["c"],"active":false}\n'
        '{"id":3,"name":"gamma","tags":[],"active":true}\n'
        '{"note":"duplicate keys below","k":1,"k":2}\n'
        '{"esc":"line\\nbreak","uni":"\\u00e9","num":1e3}\n'
    ).encode("utf-8")


def shapes():
    # A record may be any JSON value, not only an object.
    return (
        '[1,2,3]\n'
        '[true,false,null]\n'
        '"a string"\n'
        '42\n'
        '-0\n'
        '1.5e-3\n'
        '{"nested":{"deep":[{"x":1},{"y":2}]}}\n'
    ).encode("utf-8")


def unicode():
    return (
        '{"s":"\\u00e9\\u00e8","emoji":"\\ud83d\\ude00"}\n'
        '{"raw":"caf\u00e9","tab":"a\\tb"}\n'
        '{"mixed":"a\\tb\\nc"}\n'
    ).encode("utf-8")


def crlf():
    # CRLF terminators, with a blank CRLF line between records.
    return b'{"a":1}\r\n\r\n{"b":2}\r\n{"c":3}\r\n'


def blank():
    # Blank and trailing lines around two records.
    return b'\n{"x":1}\n\n\n{"y":2}\n\n'


def large():
    # ~2 MB of records with strings, integers, floats, arrays, and booleans.
    out = []
    target = 2 * 1024 * 1024
    size = 0
    i = 0
    while size < target:
        row = (
            '{"id":%d,"name":"item-%d","value":%d,"ratio":%d.%02d,'
            '"tags":["t%d","t%d"],"ok":true}\n'
            % (i, i, i * 3, i, i % 100, i, i + 1)
        )
        out.append(row)
        size += len(row)
        i += 1
    return "".join(out).encode("utf-8")


def single_json():
    # One JSON value, pretty-printed across several lines. Must stay `Json`.
    return (
        '{\n'
        '  "a": 1,\n'
        '  "b": [1, 2, 3],\n'
        '  "c": "single"\n'
        '}\n'
    ).encode("utf-8")


def malformed():
    # The second physical line is not exactly one JSON value. Must stay `Opaque`.
    return b'{"a":1}\n[1,2\n'


def bag():
    # Two JSON values on one line, separated by nothing (no newline). Not JSONL.
    return b'{"a":1}{"b":2}\n'


FIXTURES = [
    ("basic.ndjson", basic),
    ("shapes.ndjson", shapes),
    ("unicode.ndjson", unicode),
    ("crlf.ndjson", crlf),
    ("blank.ndjson", blank),
    ("large.ndjson", large),
    ("single.json", single_json),
    ("malformed.ndjson", malformed),
    ("bag.ndjson", bag),
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
    out = os.path.join(here, "jsonl")
    os.makedirs(out, exist_ok=True)
    for name, fn in FIXTURES:
        path = os.path.join(out, name)
        data = fn()
        _write(path, data)
        print("%s\t%d bytes" % (name, len(data)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
