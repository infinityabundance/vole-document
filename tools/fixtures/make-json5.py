#!/usr/bin/env python3
# Phase 21.17.1 — deterministic JSON5 / JSONC fixture generator (Python stdlib only).
#
# Emits a small, fixed family of JSON5 and JSONC documents that exercise the surface
# the adapter claims: `//` line and `/* … */` block comments; unquoted IdentifierName
# keys (including `$`, `_`, and Unicode letters); single-quoted strings; trailing
# commas in objects and arrays; numbers with a leading `+`, a leading/trailing
# decimal point (`.5`, `5.`), hexadecimal (`0xFF`), `Infinity`, `-Infinity`, `NaN`;
# multi-line strings via `\` line continuation and the extra escapes (`\x`, `\0`,
# `\v`); the extended JSON5 whitespace set (NBSP and friends); a JSONC document
# (comments + trailing commas only); a multi-hundred-kilobyte document; and three
# controls.
#
# The controls pin the detection boundaries:
#   * `strict.json`    — **strict JSON**. It must stay `Json` (never reclassified).
#   * `malformed.json5` — a malformed JSON5 source. It must stay `Opaque`.
#   * `prose.txt`      — a plain non-JSON text blob. It must stay `Opaque`.
#
#   python3 tools/fixtures/make-json5.py                 # write tools/fixtures/json5/
#   python3 tools/fixtures/make-json5.py --corpus DIR    # write DIR/, print TSV
#
# The TSV is `name<TAB>bytes<TAB>sha256` per fixture.

import hashlib
import os
import sys


def basic():
    # Comments, unquoted keys, single quotes, trailing commas.
    return (
        "{\n"
        "  // a line comment\n"
        "  unquoted: 'single',\n"
        "  hex: 0xFF,\n"
        "  /* a block comment */ trailing: [1, 2,],\n"
        "  $dollar: 1,\n"
        "  _under: 2,\n"
        "}\n"
    ).encode("utf-8")


def jsonc():
    # The ONLY extensions are comments and a trailing comma: dialect jsonc.
    return (
        "{\n"
        "  // tsconfig-style JSON with comments\n"
        "  \"compilerOptions\": {\n"
        "    \"strict\": true, /* a block comment */\n"
        "    \"target\": \"ES2020\",\n"
        "  },\n"
        "}\n"
    ).encode("utf-8")


def numbers():
    return (
        "{ plus: +1, dot: .5, trail: 5., hex: 0xff, "
        "inf: Infinity, ninf: -Infinity, nan: NaN, exp: 1e3, negz: -0 }\n"
    ).encode("utf-8")


def strings():
    # Single-quoted, `\x`, `\0`, `\u`, and a `\` line continuation (multi-line).
    return (
        "{ single: 'a\\tb', hex: '\\x41', nul: '\\0', "
        "uni: '\\u00e9', cont: 'line\\\nbreak' }\n"
    ).encode("utf-8")


def unicode_keys():
    # Unicode letters as unquoted keys, a Unicode escape, and NBSP whitespace.
    return (
        "{ caf\u00e9: 1, \u043a\u043b\u044e\u0447: 'x', esc: '\\u00e9',\u00a0nb: 2 }\n"
    ).encode("utf-8")


def comments():
    # Comments in many positions; `//` inside a string is not a comment.
    return (
        "// leading comment\n"
        "{ a: 1 /* mid */, b: '// not a comment', /* c: 3 */ d: 4 }\n"
        "// trailing comment\n"
    ).encode("utf-8")


def large():
    # ~1 MB of objects with unquoted keys and JSON5 numbers.
    out = []
    target = 1 * 1024 * 1024
    size = 0
    i = 0
    out.append("[")
    while size < target:
        row = (
            "{ id: %d, name: 'item-%d', value: 0x%X, ratio: .%02d, "
            "tags: ['t%d', 't%d',], ok: true },\n"
            % (i, i, i, i % 100, i, i + 1)
        )
        out.append(row)
        size += len(row)
        i += 1
    out.append("]\n")
    return "".join(out).encode("utf-8")


def strict_json():
    # Strict JSON: must stay `Json`.
    return (
        '{\n'
        '  "a": 1,\n'
        '  "b": [2, 3]\n'
        '}\n'
    ).encode("utf-8")


def malformed():
    # An unterminated array: not JSON5 (and not JSON/YAML/CSV/…). Must stay Opaque.
    return b"[1, 2"


def prose():
    # Plain prose: no structural mark. Must stay Opaque.
    return (
        "The quick brown fox jumps over the lazy dog.\n"
        "This is plain prose, not a structured document.\n"
    ).encode("utf-8")


FIXTURES = [
    ("basic.json5", basic),
    ("jsonc.jsonc", jsonc),
    ("numbers.json5", numbers),
    ("strings.json5", strings),
    ("unicode.json5", unicode_keys),
    ("comments.json5", comments),
    ("large.json5", large),
    ("strict.json", strict_json),
    ("malformed.json5", malformed),
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
    out = os.path.join(here, "json5")
    os.makedirs(out, exist_ok=True)
    for name, fn in FIXTURES:
        path = os.path.join(out, name)
        data = fn()
        _write(path, data)
        print("%s\t%d bytes" % (name, len(data)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
