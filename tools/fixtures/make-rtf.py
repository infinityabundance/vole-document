#!/usr/bin/env python3
# Phase 21.32 — deterministic RTF fixture generator for the ECONOMIC court (Python
# stdlib only). Exercises the surface the RTF adapter claims: the `\rtf1` header,
# the font/colour/stylesheet/info destination tables, control words with numeric
# parameters, `\'hh` hex escapes and `\uN` unicode escapes, escapes/symbols, and
# nested groups. Controls pin the detection boundary (a leading-`{` junk blob, a
# `{\rtfX` header without a version digit, an unbalanced brace, and prose all stay
# `opaque`).
#
#   python3 tools/fixtures/make-rtf.py --corpus DIR   # write DIR/, print TSV

import hashlib
import os
import sys


def doc():
    return (b"{\\rtf1\\ansi\\deff0\n"
            b"{\\fonttbl{\\f0\\fnil Arial;}{\\f1\\fnil Times;}}\n"
            b"{\\colortbl;\\red0\\green0\\blue0;}\n"
            b"{\\stylesheet{\\s0 Normal;}}\n"
            b"{\\info{\\title Doc}{\\author Ada}}\n"
            b"Hello \\b bold\\b0 \\fs24 big\\fs20 and caf\\'e9\n"
            b"\\u233? \\uc0\\u8212\n"
            b"\\{x\\} \\\\ \\~ \\- \\_\n"
            b"{grouped text}\n"
            b"}")


def large():
    out = [b"{\\rtf1\\ansi\\deff0\n{\\fonttbl{\\f0\\fnil Arial;}}\n"]
    size = sum(len(x) for x in out)
    i = 0
    while size < 256 * 1024:
        run = ("Paragraph %d with \\b bold\\b0 and caf\\'e9 text. " % i).encode("ascii")
        out.append(run)
        size += len(run)
        i += 1
        if i % 8 == 0:
            out.append(b"\\par\n")
            size += 5
    out.append(b"}")
    return b"".join(out)


# --- controls ---------------------------------------------------------------

def junk():
    return b"{ not rtf }"


def prose():
    return b"This is just prose text.\nNo RTF structure at all.\n"


def rtfx():
    return b"{\\rtfX junk}"


def broken():
    return b"{\\rtf1\\ansi {x}"


LANE = [
    ("doc.rtf", doc),
    ("large.rtf", large),
]

CONTROLS = [
    ("junk.txt", junk),
    ("prose.txt", prose),
    ("rtfx.rtf", rtfx),
    ("broken.rtf", broken),
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
    print("usage: make-rtf.py --corpus DIR", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
