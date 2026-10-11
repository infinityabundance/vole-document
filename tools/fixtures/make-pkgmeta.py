#!/usr/bin/env python3
# Phase 21.29 — deterministic package-metadata fixture generator for the ECONOMIC
# court (Python stdlib only). Exercises the surface the package-metadata adapter
# claims: npm `package.json`, `package-lock.json`, Cargo.toml, Cargo.lock and PEP
# 621 pyproject.toml — dialects, section roles, ordered entries and inline-table
# dependency specs. Controls pin the detection boundary (a generic JSON/TOML
# document stays `json`/`toml`, prose stays `opaque`).
#
#   python3 tools/fixtures/make-pkgmeta.py --corpus DIR   # write DIR/, print TSV

import hashlib
import os
import sys


def package_json():
    return (b'{"name":"demo","version":"1.0.0","scripts":{"build":"tsc"},'
            b'"dependencies":{"react":"^18.0.0","left-pad":"1.0.0"},'
            b'"devDependencies":{"typescript":"^5.0.0"}}\n')


def cargo_toml():
    return (b'[package]\n'
            b'name = "demo"\n'
            b'version = "0.1.0"\n'
            b'edition = "2021"\n'
            b'\n'
            b'[dependencies]\n'
            b'serde = { version = "1", features = ["derive"] }\n'
            b'regex = "1"\n')


def pyproject_toml():
    return (b'[build-system]\n'
            b'requires = ["setuptools>=61"]\n'
            b'build-backend = "setuptools.build_meta"\n'
            b'\n'
            b'[project]\n'
            b'name = "demo"\n'
            b'version = "1.0.0"\n'
            b'dependencies = ["requests>=2"]\n')


def cargo_lock():
    return (b'version = 3\n'
            b'\n'
            b'[[package]]\n'
            b'name = "demo"\n'
            b'version = "0.1.0"\n'
            b'dependencies = ["serde"]\n')


def package_lock_json():
    return (b'{"name":"demo","version":"1.0.0","lockfileVersion":3,'
            b'"packages":{"":{"name":"demo","version":"1.0.0"},'
            b'"node_modules/react":{"version":"18.0.0"}}}\n')


def large():
    out = [b'{"name":"demo","version":"1.0.0","dependencies":{']
    size = len(out[0])
    i = 0
    while size < 256 * 1024:
        item = ('"pkg-%06d":"^1.0.%d",' % (i, i % 10)).encode("utf-8")
        out.append(item)
        size += len(item)
        i += 1
    out.append(b'"tail":"1.0.0"}}\n')
    return b"".join(out)


# --- controls ---------------------------------------------------------------

def generic_json():
    return b'{"name":"x","version":"1.0.0"}\n'


def generic_toml():
    return b'name = "x"\nversion = "1.0.0"\n'


def prose():
    return (b"This is just prose text.\n"
            b"It has several lines, with punctuation,\n"
            b"but no manifest structure at all.\n")


LANE = [
    ("package.json", package_json),
    ("Cargo.toml", cargo_toml),
    ("pyproject.toml", pyproject_toml),
    ("Cargo.lock", cargo_lock),
    ("package-lock.json", package_lock_json),
    ("large.json", large),
]

CONTROLS = [
    ("generic.json", generic_json),
    ("generic.toml", generic_toml),
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
    print("usage: make-pkgmeta.py --corpus DIR", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
