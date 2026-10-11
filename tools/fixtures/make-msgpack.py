#!/usr/bin/env python3
# Phase 21.19 — deterministic MessagePack fixture generator (Python stdlib only).
#
# Emits a small, fixed family of MessagePack documents that exercise the surface the
# adapter claims: the exact format byte actually used (encoding width AND signedness:
# `0x17` fixint vs `0xcc` uint8 vs `0xd0` int8); `str` vs `bin` as distinct kinds;
# map order and duplicate keys; float width (float32/float64); extension type numbers
# and payload lengths (preserved, never interpreted); an unambiguous `map16` header;
# a map with non-text keys (no strict-JSON representation); non-finite floats (no
# strict-JSON representation for NaN); and a large document.
#
# The controls pin the detection boundaries:
#   * `strict.json`  — **strict JSON**. It must stay `Json` (never reclassified).
#   * `control.cbor` — a **CBOR** document. It must stay `Cbor` (coexistence: CBOR is
#     tried first, so MessagePack never steals it).
#   * `prose.txt`    — plain text. Must stay `Opaque`.
#   * `single.msgpack`, `scalar.msgpack` — structurally trivial; stay `Opaque`.
#   * `badc1.msgpack`, `badmap.msgpack`, `trailing.msgpack` — malformed; stay `Opaque`.
#   * `fixarray3.bin`, `fixmap2.bin` — the ambiguous short-container prefix overlap;
#     stay `Opaque` (never guessed).
#
#   python3 tools/fixtures/make-msgpack.py                 # write tools/fixtures/msgpack/
#   python3 tools/fixtures/make-msgpack.py --corpus DIR    # write DIR/, print TSV
#
# The TSV is `name<TAB>bytes<TAB>sha256` per fixture.

import hashlib
import os
import sys


def basic():
    # {"a": [1, 2, 3], "b": true}
    return bytes([0x82, 0xA1, 0x61, 0x93, 0x01, 0x02, 0x03, 0xA1, 0x62, 0xC3])


def widths():
    # [23 fixint, 23 uint8, 23 int8, 127, -1] — width + signedness must stay distinct.
    return bytes([0x95, 0x17, 0xCC, 0x17, 0xD0, 0x17, 0x7F, 0xFF])


def floats():
    # [1.0 (float32), 1.0 (float64)]
    return bytes([0x92, 0xCA, 0x3F, 0x80, 0x00, 0x00,
                  0xCB, 0x3F, 0xF0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])


def bytestext():
    # [bin'010203', "abc"] — bin vs str
    return bytes([0x92, 0xC4, 0x03, 0x01, 0x02, 0x03, 0xA3, 0x61, 0x62, 0x63])


def dupkeys():
    # {"a": 1, "a": 2, "b": 3} — duplicate keys
    return bytes([0x83, 0xA1, 0x61, 0x01, 0xA1, 0x61, 0x02, 0xA1, 0x62, 0x03])


def ext():
    # [fixext4(-1, 01020304), ext8(5, "AB")] — ext type + length preserved
    return bytes([0x92, 0xD6, 0xFF, 0x01, 0x02, 0x03, 0x04,
                  0xC7, 0x02, 0x05, 0x41, 0x42])


def map16():
    # map16 {"a": 1} — an unambiguous MessagePack-only head byte (CBOR rejects 0xde)
    return bytes([0xDE, 0x00, 0x01, 0xA1, 0x61, 0x01])


def nested():
    # {"a": {"b": [1, 2, 3]}}
    return bytes([0x81, 0xA1, 0x61, 0x81, 0xA1, 0x62, 0x93, 0x01, 0x02, 0x03])


def intkeys():
    # {"n": 1, "m": {1: "a", 2: "b"}} — an inner map with non-text (int) keys, which a
    # strict-JSON normalization cannot represent.
    return bytes([0x82, 0xA1, 0x6E, 0x01, 0xA1, 0x6D, 0x82, 0x01, 0xA1, 0x61,
                  0x02, 0xA1, 0x62])


def nonfinite():
    # [+Inf (float32), NaN (float64)] — NaN has no strict-JSON representation
    return bytes([0x92, 0xCA, 0x7F, 0x80, 0x00, 0x00,
                  0xCB, 0x7F, 0xF8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])


def large():
    # A definite array16(2000) of uint8 1.
    return bytes([0xDC, 0x07, 0xD0]) + (b"\xCC\x01" * 2000)


def strict_json():
    return b'{"a": 1, "b": [2, 3]}'


def control_cbor():
    return bytes([0xA3, 0x61, 0x61, 0x01, 0x61, 0x62, 0x82, 0x01, 0x02,
                  0x61, 0x63, 0xF5])


def prose():
    return (b"The quick brown fox jumps over the lazy dog.\n"
            b"Plain prose, not MessagePack.\n")


def single():
    return bytes([0x17])


def scalar():
    return bytes([0xCC, 0x17])


def badc1():
    return bytes([0xC1])


def badmap():
    return bytes([0x81, 0xA1, 0x61])


def trailing():
    return bytes([0x01, 0x02])


def fixarray3():
    return bytes([0x93, 0x01, 0x02, 0x03])


def fixmap2():
    return bytes([0x82, 0x01, 0x02, 0x03, 0x04])


FIXTURES = [
    ("basic.msgpack", basic),
    ("widths.msgpack", widths),
    ("floats.msgpack", floats),
    ("bytestext.msgpack", bytestext),
    ("dupkeys.msgpack", dupkeys),
    ("ext.msgpack", ext),
    ("map16.msgpack", map16),
    ("nested.msgpack", nested),
    ("intkeys.msgpack", intkeys),
    ("nonfinite.msgpack", nonfinite),
    ("large.msgpack", large),
    ("strict.json", strict_json),
    ("control.cbor", control_cbor),
    ("prose.txt", prose),
    ("single.msgpack", single),
    ("scalar.msgpack", scalar),
    ("badc1.msgpack", badc1),
    ("badmap.msgpack", badmap),
    ("trailing.msgpack", trailing),
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
    out = os.path.join(here, "msgpack")
    os.makedirs(out, exist_ok=True)
    for name, fn in FIXTURES:
        data = fn()
        with open(os.path.join(out, name), "wb") as f:
            f.write(data)
        print("%s\t%d bytes" % (name, len(data)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
