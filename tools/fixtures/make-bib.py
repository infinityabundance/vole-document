#!/usr/bin/env python3
# Phase 21.31 — deterministic bibliography fixture generator for the ECONOMIC court
# (Python stdlib only). Exercises the surface the bibliography adapter claims:
# BibTeX (@article with braced/quoted/number/macro fields and a duplicate field),
# RIS (a TY..ER entry with a multi-line continuation) and CSL-JSON (a JSON array of
# items carrying `id` + `type`). Controls pin the detection boundary (a plain JSON
# array of non-CSL objects stays `json`; prose stays `opaque`).
#
#   python3 tools/fixtures/make-bib.py --corpus DIR   # write DIR/, print TSV

import hashlib
import os
import sys


def refs_bib():
    return (b"@article{smith2020,\n"
            b"  author = {Smith, John},\n"
            b'  title = "A Study",\n'
            b"  year = 2020,\n"
            b"  month = jan,\n"
            b"  note = {first},\n"
            b"  note = {second}\n"
            b"}\n")


def refs_ris():
    return (b"TY  - JOUR\n"
            b"AU  - Smith, John\n"
            b"TI  - A Study\n"
            b"      continued here\n"
            b"PY  - 2020\n"
            b"ER  - \n")


def refs_json():
    return (b'[{"id":"smith2020","type":"article-journal",'
            b'"title":"A Study"},{"id":"doe2019","type":"book",'
            b'"title":"Things"}]\n')


def large():
    out = [b"["]
    size = len(out[0])
    i = 0
    first = True
    while size < 256 * 1024:
        item = ('' if first else ',') + \
            ('{"id":"item%06d","type":"article-journal","title":"Study %d"}' % (i, i))
        first = False
        out.append(item.encode("utf-8"))
        size += len(item)
        i += 1
    out.append(b"]\n")
    return b"".join(out)


# --- controls ---------------------------------------------------------------

def array_json():
    return b'[{"a":1},{"b":2}]\n'


def prose():
    return (b"This is just prose text.\n"
            b"It has several lines, with punctuation,\n"
            b"but no bibliography structure at all.\n")


LANE = [
    ("refs.bib", refs_bib),
    ("refs.ris", refs_ris),
    ("refs.json", refs_json),
    ("large.json", large),
]

CONTROLS = [
    ("array.json", array_json),
    ("prose.txt", prose),
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
    print("usage: make-bib.py --corpus DIR", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
