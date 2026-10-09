#!/usr/bin/env python3
# Phase 21.7.1 — deterministic CSV/TSV fixture generator (Python stdlib only).
#
# Emits a small, fixed family of comma- and tab-separated tables that exercise the
# representation-preserving surface the CSV adapter claims: quoted fields with
# embedded delimiters/newlines/quotes (`""`), CRLF vs LF, a UTF-8 BOM, a header
# row, ragged rows, a TSV under the same feature, a ~50 MB file (to exercise
# streaming/bounded memory), and two controls that must stay Opaque (plain text
# and a malformed file). Everything is deterministic (no randomness, no clock).
#
#   python3 tools/fixtures/make-csv.py                 # write tools/fixtures/csv/
#   python3 tools/fixtures/make-csv.py --corpus DIR    # write DIR/, print TSV
#
# The TSV is `name<TAB>bytes<TAB>sha256` per fixture, matching make-json.py /
# make-yaml.py.

import hashlib
import os
import sys


def basic():
    # A plain comma table with a header row and three data rows (LF).
    return (
        "id,name,age\n"
        "1,alice,30\n"
        "2,bob,25\n"
        "3,carol,41\n"
    ).encode("utf-8")


def quoted():
    # Quoted fields: an embedded comma, an embedded newline, and `""` escapes.
    return (
        'name,note,age\n'
        'alice,"a,b",30\n'
        'bob,"x\ny",25\n'
        '"q""q",plain,7\n'
    ).encode("utf-8")


def crlf():
    # The same shape with CRLF terminators and a quoted multi-line field.
    return (
        "name,note,age\r\n"
        "alice,\"line1\r\nline2\",30\r\n"
        "bob,plain,25\r\n"
    ).encode("utf-8")


def bom():
    # A UTF-8 BOM followed by a plain LF table.
    return b"\xef\xbb\xbf" + (
        "a,b\n"
        "1,2\n"
        "3,4\n"
    ).encode("utf-8")


def ragged():
    # A header of 3 columns; most rows agree, one has 2 and one has 5 fields.
    return (
        "a,b,c\n"
        "1,2,3\n"
        "4,5\n"
        "6,7,8\n"
        "9,10,11,12,13\n"
        "14,15,16\n"
    ).encode("utf-8")


def tsv():
    # A tab-delimited table (the same feature covers CSV and TSV).
    return (
        "id\tname\tvalue\n"
        "1\talpha\t100\n"
        "2\tbeta\t200\n"
        "3\tgamma\t300\n"
    ).encode("utf-8")


def large(target=50 * 1024 * 1024):
    # A ~50 MB comma table: a header plus fixed-shaped rows. Deterministic.
    out = ["row_id,name,score,note\n"]
    size = len(out[0])
    i = 0
    while size < target:
        row = "%d,item-%d,%d,\"n, %d\"\n" % (i, i, (i * 37) % 1000, i)
        out.append(row)
        size += len(row)
        i += 1
    return "".join(out).encode("utf-8")


def plain():
    # Plain prose: one column, no stable delimiter -> NOT a table -> Opaque.
    return (
        "This is just some prose text.\n"
        "It has several lines, with commas even,\n"
        "but no consistent tabular framing at all.\n"
    ).encode("utf-8")


def malformed():
    # An unterminated quoted field: NOT valid CSV -> Opaque.
    return b'"a,b\nc,d\n'


def onecol():
    # A single-column blob: indistinguishable from text -> Opaque.
    return (
        "alpha\n"
        "beta\n"
        "gamma\n"
    ).encode("utf-8")


# Files written by default (into tools/fixtures/csv/) and by --corpus.
FIXTURES = [
    ("basic.csv", basic),
    ("quoted.csv", quoted),
    ("crlf.csv", crlf),
    ("bom.csv", bom),
    ("ragged.csv", ragged),
    ("tsv.tsv", tsv),
    ("large.csv", large),
    ("plain.txt", plain),
    ("malformed.csv", malformed),
    ("onecol.csv", onecol),
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
    out = os.path.join(here, "csv")
    os.makedirs(out, exist_ok=True)
    for name, fn in FIXTURES:
        path = os.path.join(out, name)
        data = fn()
        _write(path, data)
        print("%s\t%d bytes" % (name, len(data)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
