#!/usr/bin/env python3
# Phase 21.27 — deterministic MHTML (MIME HTML) fixture generator for the ECONOMIC
# court (Python stdlib only). Exercises the surface the MHTML adapter claims: the
# MIME `multipart/related` envelope (`Snapshot-Content-Location`, `Content-Base`),
# ordered parts, ordered sub-resources keyed by Content-Location/Content-ID, and a
# root HTML part selected by the `start=` Content-ID else the first `text/html`
# part. Controls pin the detection boundary (prose stays `opaque`, a plain HTML
# document stays `html`, a plain MIME message stays `eml`).
#
#   python3 tools/fixtures/make-mhtml.py --corpus DIR   # write DIR/, print TSV

import hashlib
import os
import sys

CRLF = b"\r\n"


def base():
    return CRLF.join([
        b'From: <Saved by Blink>',
        b'Subject: Example page',
        b'MIME-Version: 1.0',
        b'Content-Type: multipart/related; boundary="BOUND"; type="text/html"; start="<root@x>"',
        b'Snapshot-Content-Location: http://example.com/page.html',
        b'Content-Base: http://example.com/',
        b'',
        b'--BOUND',
        b'Content-Type: text/html; charset=utf-8',
        b'Content-Transfer-Encoding: quoted-printable',
        b'Content-Location: http://example.com/page.html',
        b'Content-ID: <root@x>',
        b'',
        b'<html><head><link rel=3D"stylesheet" href=3D"s.css"></head>'
        b'<body><p>Caf=C3=A9</p><img src=3D"i.png"></body></html>',
        b'--BOUND',
        b'Content-Type: text/css',
        b'Content-Transfer-Encoding: quoted-printable',
        b'Content-Location: http://example.com/s.css',
        b'',
        b'p { color: =23458; }',
        b'--BOUND',
        b'Content-Type: image/png',
        b'Content-Transfer-Encoding: base64',
        b'Content-Location: http://example.com/i.png',
        b'Content-ID: <img1@x>',
        b'',
        b'iVBORw0KGgo=',
        b'--BOUND--',
        b'',
    ])


def nostart():
    return CRLF.join([
        b'From: a@b',
        b'Subject: no start',
        b'MIME-Version: 1.0',
        b'Content-Type: multipart/related; boundary="N"',
        b'Snapshot-Content-Location: http://n/',
        b'',
        b'--N',
        b'Content-Type: text/html',
        b'',
        b'<html><body>plain</body></html>',
        b'--N',
        b'Content-Type: application/octet-stream',
        b'Content-Transfer-Encoding: base64',
        b'Content-ID: <blob@n>',
        b'',
        b'AAECAwQ=',
        b'--N--',
        b'',
    ])


def envelope():
    return CRLF.join([
        b'From: a@b',
        b'Subject: envelope signal',
        b'MIME-Version: 1.0',
        b'Content-Type: multipart/related; boundary="E"',
        b'',
        b'--E',
        b'Content-Type: text/html',
        b'',
        b'<h1>env</h1>',
        b'--E--',
        b'',
    ])


def large():
    out = [CRLF.join([
        b'From: a@b',
        b'Subject: large',
        b'MIME-Version: 1.0',
        b'Content-Type: multipart/related; boundary="L"',
        b'Snapshot-Content-Location: http://l/',
        b'',
        b'--L',
        b'Content-Type: text/html',
        b'',
        b'<html><body>',
    ])]
    size = sum(len(x) for x in out)
    i = 0
    while size < 256 * 1024:
        line = ("<p>line %d</p>\n" % i).encode("utf-8")
        out.append(line)
        size += len(line)
        i += 1
    out.append(CRLF.join([
        b'</body></html>',
        b'--L',
        b'Content-Type: application/octet-stream',
        b'Content-Transfer-Encoding: base64',
        b'Content-Location: http://l/blob.bin',
        b'',
        b'AAECAwQFBgc=',
        b'--L--',
        b'',
    ]))
    return b"".join(out)


# --- controls ---------------------------------------------------------------

def prose():
    return (b"This is just some prose text.\n"
            b"It has several lines, with punctuation,\n"
            b"but no MIME or HTML structure at all.\n")


def plain_html():
    return (b"<!doctype html>\n"
            b"<html><body><h1>Hi</h1></body></html>\n")


def plain_eml():
    return (b"From: a@b\r\n"
            b"Subject: hi\r\n"
            b"Date: Mon, 01 Jan 2029 00:00:00 +0000\r\n"
            b"\r\n"
            b"a plain message body\r\n")


LANE = [
    ("base.mhtml", base),
    ("nostart.mhtml", nostart),
    ("envelope.mhtml", envelope),
    ("large.mhtml", large),
]

CONTROLS = [
    ("prose.txt", prose),
    ("plain.html", plain_html),
    ("plain.eml", plain_eml),
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
    print("usage: make-mhtml.py --corpus DIR", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
