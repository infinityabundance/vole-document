#!/usr/bin/env python3
# Phase 27.4 — deterministic legacy Word 97–2003 (`.doc`, MS-DOC) fixture
# generator for the ECONOMIC court. The bytes are produced by the *shipped*
# dependency-free example writer (`examples/doc_fixtures.rs`, the same generator
# the Phase-26 adapter court uses), so the economic corpus is byte-identical to
# the adapter corpus. This wrapper only builds/runs that example and prints the
# `name<TAB>bytes<TAB>sha` TSV the shared economic engine consumes (Python stdlib
# only otherwise).
#
#   python3 tools/fixtures/make-doc.py --corpus DIR   # write DIR/, print TSV

import hashlib
import os
import subprocess
import sys

PROFILE = os.environ.get("ECON_PROFILE", "release")


def build_and_run(out_dir):
    subprocess.run(
        ["cargo", "run", "--locked", "--all-features",
         "--profile", PROFILE, "--example", "doc_fixtures", "--", out_dir],
        check=True)


def emit_tsv(out_dir):
    for name in sorted(os.listdir(out_dir)):
        p = os.path.join(out_dir, name)
        if not os.path.isfile(p):
            continue
        data = open(p, "rb").read()
        print("%s\t%d\t%s" % (name, len(data), hashlib.sha256(data).hexdigest()))


def main():
    if len(sys.argv) >= 3 and sys.argv[1] == "--corpus":
        out = sys.argv[2]
        os.makedirs(out, exist_ok=True)
        build_and_run(out)
        emit_tsv(out)
        return 0
    print("usage: make-doc.py --corpus DIR", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
