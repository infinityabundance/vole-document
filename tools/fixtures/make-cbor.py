#!/usr/bin/env python3
# Phase 21.18 — deterministic CBOR (RFC 8949) fixture generator (Python stdlib only).
#
# Emits a small, fixed family of CBOR documents that exercise the surface the
# adapter claims: every major type; the encoding width actually used (`0x17` vs
# `0x1817`) and signedness (uint vs negint); byte string vs text string as distinct
# kinds; tag numbers (never resolved); map order and duplicate keys; float width
# (half/single/double); definite vs indefinite-length items; a map with non-text
# keys (which a strict-JSON normalization cannot represent); non-finite floats (which
# a strict-JSON normalization cannot represent either); and a large document.
#
# The controls pin the detection boundaries:
#   * `strict.json`   — **strict JSON**. It must stay `Json` (never reclassified).
#   * `prose.txt`     — plain text. Must stay `Opaque`.
#   * `single.cbor`, `scalar.cbor` — structurally trivial; stay `Opaque`.
#   * `badmap.cbor`, `unterm.cbor`, `trailing.cbor` — malformed; stay `Opaque`.
#   * `fixarray3.bin`, `fixmap2.bin` — the ambiguous MessagePack/CBOR short-container
#     prefix overlap; stay `Opaque` (never guessed).
#
#   python3 tools/fixtures/make-cbor.py                 # write tools/fixtures/cbor/
#   python3 tools/fixtures/make-cbor.py --corpus DIR    # write DIR/, print TSV
#
# The TSV is `name<TAB>bytes<TAB>sha256` per fixture.

import hashlib
import os
import sys


def basic():
    # {"a": 1, "b": [1, 2], "c": true}
    return bytes([0xA3, 0x61, 0x61, 0x01, 0x61, 0x62, 0x82, 0x01, 0x02,
                  0x61, 0x63, 0xF5])


def widths():
    # [23 (0x17), 23 (0x1817), -1 (0x20), -25 (0x3818)] — width + signedness distinct.
    return bytes([0x84, 0x17, 0x18, 0x17, 0x20, 0x38, 0x18])


def floats():
    # [1.5 (half), 1.0 (single), 1.0 (double)]
    return bytes([0x83, 0xF9, 0x3E, 0x00, 0xFA, 0x3F, 0x80, 0x00, 0x00,
                  0xFB, 0x3F, 0xF0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])


def bytestext():
    # [h'010203', "abc"] — byte string vs text string
    return bytes([0x82, 0x43, 0x01, 0x02, 0x03, 0x63, 0x61, 0x62, 0x63])


def dupkeys():
    # {"a": 1, "a": 2} — duplicate keys
    return bytes([0xA2, 0x61, 0x61, 0x01, 0x61, 0x61, 0x02])


def tags():
    # 55799({"t": 1(0x514b6700), "n": 5}) — self-described tag + tag 1, never resolved
    return bytes([0xD9, 0xD9, 0xF7, 0xA2, 0x61, 0x74, 0xC1, 0x1A, 0x51, 0x4B,
                  0x67, 0x00, 0x61, 0x6E, 0x05])


def indef():
    # [_ "ab", {_ "x": 1}] — indefinite-length array/text/map
    return bytes([0x9F, 0x7F, 0x61, 0x61, 0x61, 0x62, 0xFF, 0xBF, 0x61, 0x78,
                  0x01, 0xFF, 0xFF])


def nested():
    # {"a": {"b": [1, 2, 3]}, "c": true}
    return bytes([0xA2, 0x61, 0x61, 0xA1, 0x61, 0x62, 0x83, 0x01, 0x02, 0x03,
                  0x61, 0x63, 0xF5])


def mapkeys():
    # {"n": 1, "m": {1: "a", 2: "b"}} — an inner map with non-text (uint) keys, which
    # a strict-JSON normalization cannot represent.
    return bytes([0xA2, 0x61, 0x6E, 0x01, 0x61, 0x6D, 0xA2, 0x01, 0x61, 0x61,
                  0x02, 0x61, 0x62])


def nonfinite():
    # [+Inf (half), NaN (half), -Inf (half)] — no strict-JSON representation for NaN
    return bytes([0x83, 0xF9, 0x7C, 0x00, 0xF9, 0x7E, 0x00, 0xF9, 0xFC, 0x00])


def large():
    # A definite array(2000) of uint 1 in the one-byte form (0x18 0x01).
    return bytes([0x99, 0x07, 0xD0]) + (b"\x18\x01" * 2000)


def strict_json():
    return b'{"a": 1, "b": [2, 3]}'


def prose():
    return (b"The quick brown fox jumps over the lazy dog.\n"
            b"Plain prose, not CBOR.\n")


def single():
    return bytes([0x17])


def scalar():
    return bytes([0x18, 0x17])


def badmap():
    return bytes([0xA1, 0x61, 0x61, 0x61])


def unterm():
    return bytes([0x9F, 0x01, 0x02, 0x03])


def trailing():
    return bytes([0x82, 0x01, 0x02, 0x00])


def fixarray3():
    return bytes([0x93, 0x01, 0x02, 0x03])


def fixmap2():
    return bytes([0x82, 0x01, 0x02, 0x03, 0x04])


FIXTURES = [
    ("basic.cbor", basic),
    ("widths.cbor", widths),
    ("floats.cbor", floats),
    ("bytestext.cbor", bytestext),
    ("dupkeys.cbor", dupkeys),
    ("tags.cbor", tags),
    ("indef.cbor", indef),
    ("nested.cbor", nested),
    ("mapkeys.cbor", mapkeys),
    ("nonfinite.cbor", nonfinite),
    ("large.cbor", large),
    ("strict.json", strict_json),
    ("prose.txt", prose),
    ("single.cbor", single),
    ("scalar.cbor", scalar),
    ("badmap.cbor", badmap),
    ("unterm.cbor", unterm),
    ("trailing.cbor", trailing),
    ("fixarray3.bin", fixarray3),
    ("fixmap2.bin", fixmap2),
]


def emit_corpus(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    manifest = []
    for name, fn in FIXTURES:
        data = fn()
        with open(os.path.join(out_dir, name), "wb") as f:
            f.write(data)
        manifest.append((name, len(data), hashlib.sha256(data).hexdigest()))
    for name, length, sha in manifest:
        print("%s\t%d\t%s" % (name, length, sha))
    return manifest


def main():
    if len(sys.argv) >= 3 and sys.argv[1] == "--corpus":
        emit_corpus(sys.argv[2])
        return 0
    here = os.path.dirname(os.path.abspath(__file__))
    out = os.path.join(here, "cbor")
    os.makedirs(out, exist_ok=True)
    for name, fn in FIXTURES:
        data = fn()
        with open(os.path.join(out, name), "wb") as f:
            f.write(data)
        print("%s\t%d bytes" % (name, len(data)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
