#!/usr/bin/env python3
# Phase 21.28 — deterministic log-stream (syslog-style) fixture generator for the
# ECONOMIC court (Python stdlib only). Exercises the surface the log-stream
# adapter claims: a `generic` timestamp/level/message dialect, RFC 5424
# (HEADER + NILVALUE/structured-data), RFC 3164 BSD, CRLF terminators and blank
# lines. Controls pin the detection boundary (prose stays `opaque`, a single JSON
# value stays `json`, a JSON stream stays `jsonl`, a colon-less BSD stream stays
# `yaml`).
#
#   python3 tools/fixtures/make-logstream.py --corpus DIR   # write DIR/, print TSV

import hashlib
import os
import sys


def generic():
    return (b"2026-10-11T12:34:56Z INFO starting up\n"
            b"[2026-10-11 12:00:00] ERROR boom: x\n"
            b"DEBUG raw remainder kept\n")


def rfc5424():
    return (b'<165>1 2026-10-11T12:34:56.789Z host app 1234 ID47 '
            b'[ex@1 k="a b" x="q\\"r"] hello world\n'
            b'<165>1 2026-10-11T12:34:57Z host app 1234 ID48 - second\n')


def rfc3164():
    return (b"<34>Oct 11 22:14:15 mymachine su[123]: su root failed: reason\n"
            b"<34>Oct  1 01:02:03 h x: y: z\n")


def crlf():
    return b"INFO a tail\r\nWARN b tail\r\n"


def blank():
    return b"\nINFO first\n\n\nERROR last\n"


def large():
    # A colon-free level+message dialect: a timestamped/bracketed line would be
    # claimed by the higher-precedence config (Java-properties `:`) detector, so the
    # large fixture uses the `LEVEL message` generic form only.
    out = [b"INFO boot\n"]
    size = len(out[0])
    i = 0
    while size < 256 * 1024:
        line = ("INFO line %d payload token%d\n" % (i, i)).encode("utf-8")
        out.append(line)
        size += len(line)
        i += 1
    return b"".join(out)


# --- controls ---------------------------------------------------------------

def prose():
    return (b"This is just prose text.\n"
            b"It has several lines, with punctuation,\n"
            b"but no log structure at all.\n")


def single_json():
    return b'{"a":1}\n'


def stream_jsonl():
    return b'{"a":1}\n{"b":2}\n'


def bsd_nocolon():
    return (b"<34>Oct 11 22:14:15 host app[1]: aaa\n"
            b"<34>Oct 11 22:14:16 host app[2]: bbb\n")


LANE = [
    ("generic.log", generic),
    ("rfc5424.log", rfc5424),
    ("rfc3164.log", rfc3164),
    ("crlf.log", crlf),
    ("blank.log", blank),
    ("large.log", large),
]

CONTROLS = [
    ("prose.txt", prose),
    ("single.json", single_json),
    ("stream.jsonl", stream_jsonl),
    ("bsd_nocolon.log", bsd_nocolon),
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
    print("usage: make-logstream.py --corpus DIR", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
