#!/usr/bin/env python3
# Phase 21.14 — bound oversized raw evidence.
#
# Repository convention (Phase 21.7.1): a full/verbatim dump of a large fixture is
# never committed. For every JSON file under the given roots that exceeds a
# threshold, move it aside to `<name>.full.json` (gitignored) and leave a bounded
# receipt `<name>` holding the byte length, the SHA-256, and a bounded prefix.
#
#   python3 tools/fixtures/phase21-14-bound.py DIR [DIR ...]
#   BOUND_BYTES=262144 python3 tools/fixtures/phase21-14-bound.py DIR

import hashlib
import json
import os
import sys

LIMIT = int(os.environ.get("BOUND_BYTES", str(256 * 1024)))
PREFIX = 4096


def main(argv):
    for root in argv:
        for dirpath, _dirs, files in os.walk(root):
            for name in files:
                if name.endswith(".full.json") or not name.endswith(".json"):
                    continue
                p = os.path.join(dirpath, name)
                if os.path.getsize(p) <= LIMIT:
                    continue
                with open(p, "rb") as f:
                    data = f.read()
                full = p + ".full.json"
                os.replace(p, full)
                rec = {
                    "bounded": True,
                    "length": len(data),
                    "sha256": hashlib.sha256(data).hexdigest(),
                    "prefix": data[:PREFIX].decode("utf-8", "replace"),
                    "note": "the unbounded dump is gitignored as %s" % os.path.basename(full),
                }
                with open(p, "w") as f:
                    json.dump(rec, f, sort_keys=True)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
