#!/usr/bin/env python3
# Phase 21.25 — deterministic PSV (pipe) + fixed-width (column-position) fixture
# generator (Python stdlib only). Exercises the two tabular-extra dialects the
# adapters claim: the CSV adapter's **pipe** delimiter and the fixed-width
# adapter's inferred per-column layout. Controls pin the detection boundaries
# (a comma table stays `csv`, a GFM pipe table stays `markdown`, prose and an
# ambiguous single-space aligned blob stay `opaque`).
#
#   python3 tools/fixtures/make-tabular.py --corpus DIR   # write DIR/, print TSV

import hashlib
import os
import sys


def psv_basic():
    return b"name|age\nalice|30\nbob|25\n"


def psv_quotes():
    # An embedded delimiter inside a quoted field (the pipe parser must not
    # split it) and a plain field.
    return b'label|flag\n"x|y"|on\nplain|off\n'


def fw_basic():
    # Three records of identical width 10; columns [0,5) and [7,10).
    return b"Name   Age\nAlice   30\nBob     25\n"


def fw_three():
    # Three records of width 13; columns [0,3), [5,8), [10,13).
    return b"ID   Nm   Age\n001  Ali  030\n002  Bob  025\n"


def fw_crlf():
    return b"Name   Age\r\nAlice   30\r\nBob     25\r\n"


def large_psv():
    out = [b"id|name|value\n"]
    i = 0
    size = len(out[0])
    while size < 256 * 1024:
        row = "r%d|name%d|v%d\n" % (i, i, i)
        out.append(row.encode("utf-8"))
        size += len(row)
        i += 1
    return b"".join(out)


def large_fw():
    # Uniform width 20; the header fills both columns so the recovered layout is
    # [0,8) and [10,20) and every value fits without truncation.
    out = [("%-8s  %-10s\n" % ("ColA0000", "ColB000000")).encode("utf-8")]
    i = 0
    size = len(out[0])
    while size < 256 * 1024:
        row = "%-8s  %-10s\n" % ("a%d" % i, "b%d" % i)
        out.append(row.encode("utf-8"))
        size += len(row)
        i += 1
    return b"".join(out)


# --- controls ---------------------------------------------------------------

def comma_csv():
    return b"name,age\nalice,30\nbob,25\n"


def markdown_md():
    return b"| a | b |\n| --- | --- |\n| c | d |\n"


def prose():
    return b"The quick brown fox\njumps over the lazy\ndog near the river\n"


def spaced():
    return b"abcd efgh\nijkl mnop\nqrst uvwx\n"


LANE = [
    ("psv-basic.psv", psv_basic),
    ("psv-quotes.psv", psv_quotes),
    ("fw-basic.fw", fw_basic),
    ("fw-three.fw", fw_three),
    ("fw-crlf.fw", fw_crlf),
    ("large.psv", large_psv),
    ("large.fw", large_fw),
]

CONTROLS = [
    ("comma.csv", comma_csv),
    ("markdown.md", markdown_md),
    ("prose.txt", prose),
    ("spaced.txt", spaced),
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
    print("usage: make-tabular.py --corpus DIR", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
