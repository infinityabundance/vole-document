#!/usr/bin/env python3
# Phase 21.20 — deterministic INI / .env / Java-properties (config family) fixture
# generator (Python stdlib only). Exercises the surface the config adapter claims:
# `[section]` headers; `=` and `:` separators; `;`/`#` full-line and inline comments;
# `export` markers; single/double/empty quoting; the Java `.properties` trailing-`\`
# line continuation and `\uXXXX` escapes preserved as spelling; duplicate keys; and
# a multi-hundred-kilobyte document. Controls pin the detection boundaries.
#
# Every lane fixture starts with exactly one full-line comment (line 0), so the
# frozen plans in tools/fixtures/textfmt_econ.py address a stable layout.
#
#   python3 tools/fixtures/make-config.py --corpus DIR   # write DIR/, print TSV

import hashlib
import os
import sys


def ini_basic():
    return (b"; header\n"
            b"[db]\n"
            b"host = localhost ; the host\n"
            b"port: 5432\n"
            b"[app]\n"
            b"name = demo\n")


def ini_comments():
    return (b"; a leading comment\n"
            b"[s]\n"
            b"# another comment\n"
            b"k = v\n"
            b"; trailing\n"
            b"other = w\n")


def ini_spaces():
    return (b"; spaces\n"
            b"[ab]\n"
            b"key = a value with spaces\n"
            b"q = \"kept \\\"q\\\"\"\n")


def env_basic():
    return (b"# app\n"
            b"FOO=bar\n"
            b"export BAZ=\"a b\"\n"
            b"EMPTY=\n")


def env_quotes():
    return (b"# q\n"
            b"export A='single'\n"
            b"export B=\"double\"\n"
            b"export C=plain\n")


def props_basic():
    return (b"# note\n"
            b"colon: v\n"
            b"multi=one\\\n  two\n"
            b"unicode=gr\\u00FCn\n")


def props_colon():
    return (b"# c\n"
            b"a: 1\n"
            b"b : 2\n"
            b"c=3\n"
            b"unicode=x\\u00e9\n")


def dup_ini():
    return (b"; dup\n"
            b"[a]\n"
            b"k = 1\n"
            b"k = 2\n"
            b"k = 3\n"
            b"b = 9\n")


def large_ini():
    out = [b"; generated\n[main]\n"]
    size = 0
    target = 256 * 1024
    i = 0
    while size < target:
        row = "key_%d = value_%d ; note %d\n" % (i, i, i)
        out.append(row.encode("utf-8"))
        size += len(row)
        i += 1
    return b"".join(out)


# --- controls ---------------------------------------------------------------

def strict_toml():
    return b"a = 1\nb = 2\n"


def strict_json():
    return b'{"a": 1, "b": [2, 3]}\n'


def overlap_env():
    return b"FOO=bar\nBAZ=qux\n"


def prose():
    return (b"The quick brown fox jumps over the lazy dog.\n"
            b"Plain prose, not config.\n")


def script():
    return b"#!/bin/sh\nexport FOO=bar\n"


LANE = [
    ("ini-basic.ini", ini_basic),
    ("ini-comments.ini", ini_comments),
    ("ini-spaces.ini", ini_spaces),
    ("env-basic.env", env_basic),
    ("env-quotes.env", env_quotes),
    ("props-basic.properties", props_basic),
    ("props-colon.properties", props_colon),
    ("dup.ini", dup_ini),
    ("large.ini", large_ini),
]

CONTROLS = [
    ("strict.toml", strict_toml),
    ("strict.json", strict_json),
    ("overlap.env", overlap_env),
    ("prose.txt", prose),
    ("script.sh", script),
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
    print("usage: make-config.py --corpus DIR", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
