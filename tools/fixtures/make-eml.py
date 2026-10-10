#!/usr/bin/env python3
# Phase 21.13 — deterministic EML / MIME fixture generator (Python stdlib only).
#
# Emits a small, fixed family of RFC 5322 + MIME messages that exercise the surface
# the EML adapter claims: a simple text message; a `multipart/mixed` message with a
# quoted-printable text part and a base64 attachment; a `multipart/alternative`
# message; a nested `message/rfc822` message; a folded + duplicate-`Received`
# header message; and a multi-megabyte message. Everything is deterministic (no
# randomness, no clock).
#
# The control pins the detection boundary:
#   * `prose.txt` — a plain prose paragraph with no header block. It must stay
#                    `Opaque` (not EML, not any other structured format).
#
#   python3 tools/fixtures/make-eml.py                 # write tools/fixtures/eml/
#   python3 tools/fixtures/make-eml.py --corpus DIR    # write DIR/, print TSV
#
# The TSV is `name<TAB>bytes<TAB>sha256` per fixture, matching make-jsonl.py.

import base64
import hashlib
import os
import sys

CRLF = b"\r\n"


def _line(s):
    if isinstance(s, str):
        s = s.encode("utf-8")
    return s + CRLF


def _headers(pairs):
    return b"".join(_line("%s: %s" % (k, v)) for k, v in pairs)


# --- fixtures ---------------------------------------------------------------

def simple():
    return (
        _headers([
            ("From", "Alice <alice@example.com>"),
            ("To", "Bob <bob@example.com>"),
            ("Subject", "A simple message"),
            ("Date", "Mon, 01 Jan 2029 00:00:00 +0000"),
            ("Message-ID", "<simple@example.com>"),
        ])
        + CRLF
        + _line("Hello, this is a simple text message.")
    )


def mixed():
    # multipart/mixed: a quoted-printable text part and a base64 attachment.
    attachment = b"Report data: \x00\x01\x02 binary payload\n"
    b64 = base64.encodebytes(attachment).replace(b"\n", CRLF)
    return (
        _headers([
            ("From", "Alice <alice@example.com>"),
            ("To", "Bob <bob@example.com>"),
            ("Subject", "Mixed message"),
            ("Date", "Mon, 01 Jan 2029 00:00:00 +0000"),
            ("Message-ID", "<mixed@example.com>"),
            ("MIME-Version", "1.0"),
            ("Content-Type", 'multipart/mixed; boundary="MIXED"'),
        ])
        + CRLF
        + _line("This is the preamble and is ignored.")
        + _line("--MIXED")
        + _line("Content-Type: text/plain; charset=utf-8")
        + _line("Content-Transfer-Encoding: quoted-printable")
        + CRLF
        + _line("Caf=C3=A9 and tea for two.")
        + _line("--MIXED")
        + _line("Content-Type: application/octet-stream")
        + _line("Content-Transfer-Encoding: base64")
        + _line('Content-Disposition: attachment; filename="report.bin"')
        + CRLF
        + b64
        + _line("--MIXED--")
        + _line("This is the epilogue and is ignored.")
    )


def alternative():
    return (
        _headers([
            ("From", "Alice <alice@example.com>"),
            ("To", "Bob <bob@example.com>"),
            ("Subject", "Alternative message"),
            ("Date", "Mon, 01 Jan 2029 00:00:00 +0000"),
            ("Message-ID", "<alt@example.com>"),
            ("MIME-Version", "1.0"),
            ("Content-Type", 'multipart/alternative; boundary="ALT"'),
        ])
        + CRLF
        + _line("--ALT")
        + _line("Content-Type: text/plain; charset=utf-8")
        + CRLF
        + _line("Plain version.")
        + _line("--ALT")
        + _line("Content-Type: text/html; charset=utf-8")
        + CRLF
        + _line("<p>HTML version.</p>")
        + _line("--ALT--")
    )


def nested():
    inner = (
        _headers([
            ("From", "Inner <inner@example.com>"),
            ("To", "Bob <bob@example.com>"),
            ("Subject", "Forwarded note"),
            ("Date", "Sun, 31 Dec 2028 00:00:00 +0000"),
            ("Message-ID", "<inner@example.com>"),
        ])
        + CRLF
        + _line("This is the forwarded body.")
    )
    return (
        _headers([
            ("From", "Alice <alice@example.com>"),
            ("To", "Bob <bob@example.com>"),
            ("Subject", "Fwd: Forwarded note"),
            ("Date", "Mon, 01 Jan 2029 00:00:00 +0000"),
            ("Message-ID", "<nested@example.com>"),
            ("MIME-Version", "1.0"),
            ("Content-Type", "message/rfc822"),
        ])
        + CRLF
        + inner
    )


def folded():
    # Duplicate Received headers; a folded Subject; a folded X-Long header.
    return (
        _line("Received: from a.example (a.example [10.0.0.1])")
        + _line("\tby b.example with ESMTP id ABC123")
        + _line("Received: from c.example (c.example [10.0.0.2])")
        + _line("\tby d.example with ESMTP id DEF456")
        + _line("From: Alice <alice@example.com>")
        + _line("To: Bob <bob@example.com>")
        + _line("Subject: A folded")
        + _line("\tsubject line")
        + _line("Date: Mon, 01 Jan 2029 00:00:00 +0000")
        + _line("Message-ID: <folded@example.com>")
        + _line("X-Long: alpha")
        + _line("\tbeta gamma")
        + CRLF
        + _line("Body after folded headers.")
    )


def large():
    # ~2 MB message: a small text part and a large base64 attachment.
    n = 1_500_000
    payload = bytes(((i * 7 + 3) % 256) for i in range(n))
    b64 = base64.encodebytes(payload).replace(b"\n", CRLF)
    return (
        _headers([
            ("From", "Alice <alice@example.com>"),
            ("To", "Bob <bob@example.com>"),
            ("Subject", "Large message"),
            ("Date", "Mon, 01 Jan 2029 00:00:00 +0000"),
            ("Message-ID", "<large@example.com>"),
            ("MIME-Version", "1.0"),
            ("Content-Type", 'multipart/mixed; boundary="BIG"'),
        ])
        + CRLF
        + _line("--BIG")
        + _line("Content-Type: text/plain; charset=utf-8")
        + CRLF
        + _line("A large message with a big attachment.")
        + _line("--BIG")
        + _line("Content-Type: application/octet-stream")
        + _line("Content-Transfer-Encoding: base64")
        + _line('Content-Disposition: attachment; filename="big.bin"')
        + CRLF
        + b64
        + _line("--BIG--")
    )


def prose():
    # No header block: must stay Opaque.
    return (
        b"From here to there, the quick brown fox jumps over the lazy dog.\n"
        b"This paragraph has no header block and no blank-line separator.\n"
    )


FIXTURES = [
    ("simple.eml", simple),
    ("mixed.eml", mixed),
    ("alternative.eml", alternative),
    ("nested.eml", nested),
    ("folded.eml", folded),
    ("large.eml", large),
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
    out = os.path.join(here, "eml")
    os.makedirs(out, exist_ok=True)
    for name, fn in FIXTURES:
        path = os.path.join(out, name)
        data = fn()
        _write(path, data)
        print("%s\t%d bytes" % (name, len(data)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
