#!/usr/bin/env python3
# Phase 21.24 — deterministic Jupyter notebook (.ipynb) fixture generator (Python
# stdlib only). Exercises the surface the notebook adapter claims: nbformat
# major/minor; `code`/`markdown`/`raw` cells; `source` as a **string** vs a
# **line array**; `execution_count` present/absent/null; the four output kinds
# (`stream`/`execute_result`/`display_data`/`error`); cell order; duplicate members;
# and a multi-hundred-kilobyte document. Controls pin the detection boundaries.
#
#   python3 tools/fixtures/make-notebook.py --corpus DIR   # write DIR/, print TSV

import hashlib
import os
import sys


def basic():
    return (
        '{\n'
        ' "cells": [\n'
        '  {"cell_type": "markdown", "metadata": {}, '
        '"source": ["# Title\\n", "text\\n"]},\n'
        '  {"cell_type": "code", "execution_count": 1, "metadata": {},\n'
        '   "outputs": [{"output_type": "stream", "name": "stdout", "text": "hello\\n"}],\n'
        '   "source": "print(1)\\n"}\n'
        ' ],\n'
        ' "metadata": {"kernelspec": {"name": "python3"}},\n'
        ' "nbformat": 4,\n'
        ' "nbformat_minor": 5\n'
        '}\n'
    ).encode("utf-8")


def string_src():
    return (
        '{"cells": [\n'
        '  {"cell_type": "code", "execution_count": null, "metadata": {}, "outputs": [], '
        '"source": "a = 1\\n"},\n'
        '  {"cell_type": "markdown", "metadata": {}, "source": "## Heading\\n"}\n'
        '], "metadata": {}, "nbformat": 4, "nbformat_minor": 4}\n'
    ).encode("utf-8")


def lines_src():
    return (
        '{"cells": [\n'
        '  {"cell_type": "code", "execution_count": 2, "metadata": {}, '
        '"outputs": [{"output_type": "execute_result", "execution_count": 2, '
        '"metadata": {}, "data": {"text/plain": "2"}}], '
        '"source": ["a = 1\\n", "a + 1\\n"]}\n'
        '], "metadata": {}, "nbformat": 4, "nbformat_minor": 5}\n'
    ).encode("utf-8")


def outputs():
    return (
        '{"cells": [\n'
        '  {"cell_type": "code", "execution_count": 3, "metadata": {}, "outputs": [\n'
        '    {"output_type": "stream", "name": "stderr", "text": ["warn\\n"]},\n'
        '    {"output_type": "display_data", "metadata": {}, '
        '"data": {"image/png": "AAAA"}},\n'
        '    {"output_type": "error", "ename": "ValueError", "evalue": "bad", '
        '"traceback": ["Traceback\\n", "ValueError: bad\\n"]}\n'
        '  ], "source": "raise ValueError()\\n"}\n'
        '], "metadata": {}, "nbformat": 4, "nbformat_minor": 5}\n'
    ).encode("utf-8")


def raw_cell():
    return (
        '{"cells": [\n'
        '  {"cell_type": "raw", "metadata": {"format": "text/plain"}, '
        '"source": ["raw one\\n", "raw two\\n"]}\n'
        '], "metadata": {"language_info": {"name": "python"}}, '
        '"nbformat": 4, "nbformat_minor": 3}\n'
    ).encode("utf-8")


def dupkeys():
    return (
        '{"cells": [\n'
        '  {"cell_type": "markdown", "metadata": {"a": 1, "a": 2}, '
        '"source": "dup\\n"}\n'
        '], "metadata": {}, "nbformat": 4, "nbformat_minor": 5}\n'
    ).encode("utf-8")


def large():
    cells = []
    target = 256 * 1024
    size = 0
    i = 0
    while size < target:
        row = ('{"cell_type": "code", "execution_count": %d, "metadata": {}, '
               '"outputs": [], "source": ["v%d = %d\\n"]}' % (i, i, i))
        cells.append(row)
        size += len(row)
        i += 1
    body = ('{"cells": [' + ",".join(cells) + '], "metadata": {}, '
            '"nbformat": 4, "nbformat_minor": 5}\n')
    return body.encode("utf-8")


# --- controls ---------------------------------------------------------------

def not_notebook():
    return b'{"a": 1, "b": [2, 3]}\n'


def missing_nbformat():
    return b'{"cells": [], "metadata": {}}\n'


def malformed():
    return b'{"cells": [{"cell_type": "code"}'


def prose():
    return b"Plain prose, not a Jupyter notebook at all.\n"


LANE = [
    ("basic.ipynb", basic),
    ("string-src.ipynb", string_src),
    ("lines-src.ipynb", lines_src),
    ("outputs.ipynb", outputs),
    ("raw.ipynb", raw_cell),
    ("dupkeys.ipynb", dupkeys),
    ("large.ipynb", large),
]

CONTROLS = [
    ("notnotebook.json", not_notebook),
    ("missing-nbformat.json", missing_nbformat),
    ("malformed.ipynb", malformed),
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
    print("usage: make-notebook.py --corpus DIR", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
