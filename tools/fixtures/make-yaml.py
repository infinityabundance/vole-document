#!/usr/bin/env python3
# Phase 21.6.1 — deterministic YAML fixture generator (Python stdlib only).
#
# Emits a small, fixed family of YAML documents that exercise the
# representation-preserving surface the YAML adapter claims: anchors & aliases,
# merge keys, tags, multiple documents, every scalar style, comments, duplicate
# keys, deep nesting, a plain-text control (which must NOT be detected as YAML)
# and a malformed control. Everything is deterministic (no randomness, no clock).
#
#   python3 tools/fixtures/make-yaml.py                 # write tools/fixtures/yaml/
#   python3 tools/fixtures/make-yaml.py --corpus DIR    # write DIR/, print TSV
#
# The TSV is `name<TAB>bytes<TAB>sha256` per fixture, matching make-json.py.

import hashlib
import os
import sys

# --- fixtures ---------------------------------------------------------------


def anchors():
    # Anchors, aliases, and merge keys, with a full-line and a trailing comment.
    return (
        "# connection defaults\n"
        "defaults: &defaults\n"
        "  adapter: postgres\n"
        "  host: localhost\n"
        "  port: 5432\n"
        "development:\n"
        "  <<: *defaults\n"
        "  database: dev\n"
        "test:\n"
        "  <<: *defaults  # inherits\n"
        "  database: test\n"
        "primary: *defaults\n"
    ).encode("utf-8")


def tags():
    # Tags: the secondary handle (`!!str`), a verbatim tag (`!<...>`), and a
    # local tag (`!mytype`).
    return (
        "version: !!str 1.0\n"
        "enabled: !!bool true\n"
        "widget: !<tag:example.com,2024:widget>\n"
        "  name: thing\n"
        "  size: 3\n"
        "payload: !mytype raw-bytes\n"
    ).encode("utf-8")


def multidoc():
    # Three explicit documents; the third is not closed by an end marker.
    return (
        "---\n"
        "name: first\n"
        "value: 1\n"
        "---\n"
        "name: second\n"
        "value: 2\n"
        "---\n"
        "name: third\n"
        "value: 3\n"
    ).encode("utf-8")


def styles():
    # Every scalar style kept distinct: plain, single, double, literal, folded.
    return (
        "plain: a plain scalar\n"
        "single: 'single quoted'\n"
        "double: \"double quoted\"\n"
        "literal: |\n"
        "  line one\n"
        "  line two\n"
        "folded: >\n"
        "  folded one\n"
        "  folded two\n"
        "url: http://example.com/path\n"
        "empty:\n"
    ).encode("utf-8")


def comments():
    # Full-line and trailing comments; their spans must be preserved.
    return (
        "# header comment\n"
        "a: 1  # trailing on a\n"
        "\n"
        "# a comment between entries\n"
        "b: 2\n"
        "c:\n"
        "  - x  # element comment\n"
        "  - y\n"
        "# footer\n"
    ).encode("utf-8")


def duplicates():
    # Duplicate keys are kept as distinct members, in order.
    return (
        "a: 1\n"
        "b: 2\n"
        "a: 3\n"
        "c:\n"
        "  x: 1\n"
        "  x: 2\n"
        "a: 4\n"
    ).encode("utf-8")


def deep():
    # Nesting depth 80: within DEFAULT (256), over STRICT (64).
    depth = 80
    out = []
    for i in range(depth):
        out.append("  " * i + ("- " if i % 2 == 0 else "k: "))
    out.append("  " * depth + "0")
    return ("\n".join(out) + "\n").encode("utf-8")


def large():
    # ~2 MB: a marker plus a block sequence of mappings with scalar payloads.
    rows = []
    target = 2 * 1024 * 1024
    size = 0
    i = 0
    while size < target:
        row = ("  - id: %d\n"
               "    name: item-%d\n"
               "    vals: [%d, %d, %d]\n"
               "    flag: %s\n") % (i, i, i, i + 1, i + 2,
                                    "true" if i % 2 == 0 else "false")
        rows.append(row)
        size += len(row)
        i += 1
    body = "".join(rows)
    return ("marker: ZEBRA_MARKER_21_6\nitems:\n" + body).encode("utf-8")


def plain():
    # Plain prose with no colon: not a mapping or a sequence, so NOT YAML.
    return (
        "This is just some prose text.\n"
        "It has several lines but no colon anywhere\n"
        "and therefore is not a YAML mapping or sequence.\n"
    ).encode("utf-8")


def malformed():
    # An unterminated flow mapping: NOT valid YAML (must detect as Opaque).
    return b"a: {b: 1\n"


# Files written by default (into tools/fixtures/yaml/) and by --corpus.
FIXTURES = [
    ("anchors.yaml", anchors),
    ("tags.yaml", tags),
    ("multidoc.yaml", multidoc),
    ("styles.yaml", styles),
    ("comments.yaml", comments),
    ("dup.yaml", duplicates),
    ("deep.yaml", deep),
    ("large.yaml", large),
    ("plain.yaml", plain),
    ("malformed.yaml", malformed),
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
    out = os.path.join(here, "yaml")
    os.makedirs(out, exist_ok=True)
    for name, fn in FIXTURES:
        path = os.path.join(out, name)
        data = fn()
        _write(path, data)
        print("%s\t%d bytes" % (name, len(data)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
