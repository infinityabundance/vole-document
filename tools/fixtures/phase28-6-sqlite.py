#!/usr/bin/env python3
# Phase 28.6 — page-level SQLite control lane (the Phase-22.5 model, local).
#
# A source-retaining SQLite envelope: one row whose `blob` column is the exact
# source. A narrow request is answered by reading only the required source
# region: `SELECT substr(blob, start, len)`. The bytes SQLite physically reads
# are counted from `strace -e trace=pread64` on the database file (mmap_size=0,
# so every page read is a pread64 — a realistic remote-client visibility mode).
#
#   python3 tools/fixtures/phase28-6-sqlite.py build  --source S --db D
#   python3 tools/fixtures/phase28-6-sqlite.py query  --db D --start A --len L [--out F]
#   python3 tools/fixtures/phase28-6-sqlite.py trace2bytes --trace T --db D --page 4096
#
# `query` is meant to be run under strace by the court; it is a pure read.

import json
import os
import re
import sqlite3
import sys


def die(msg):
    sys.stderr.write("phase28-6-sqlite: %s\n" % msg)
    sys.exit(2)


PAGE = 4096


def cmd_build(source, db, layout):
    if os.path.exists(db):
        os.remove(db)
    with open(source, "rb") as f:
        blob = f.read()
    con = sqlite3.connect(db)
    con.execute("PRAGMA journal_mode=OFF")
    con.execute("PRAGMA synchronous=OFF")
    con.execute("PRAGMA page_size=%d" % PAGE)
    if layout == "pages":
        con.execute("CREATE TABLE pages(idx INTEGER PRIMARY KEY, data BLOB)")
        rows = [(i // PAGE, blob[i:i + PAGE]) for i in range(0, len(blob), PAGE)]
        con.executemany("INSERT INTO pages(idx, data) VALUES(?, ?)", rows)
    else:
        con.execute("CREATE TABLE src(id INTEGER PRIMARY KEY, blob BLOB)")
        con.execute("INSERT INTO src(id, blob) VALUES(1, ?)",
                    (sqlite3.Binary(blob),))
    con.commit()
    con.execute("VACUUM")
    con.commit()
    con.close()
    print(json.dumps({"db": db, "layout": layout,
                      "db_bytes": os.path.getsize(db),
                      "source_bytes": len(blob)}))


def _pages_for(rs, page):
    ps = set()
    for a, b in rs:
        for p in range(a // page, (max(a, b - 1)) // page + 1):
            ps.add(p)
    return sorted(ps)


def cmd_query(db, ranges, layout, out_prefix):
    con = sqlite3.connect(db)
    con.execute("PRAGMA mmap_size=0")      # every page read is a real pread64
    con.execute("PRAGMA cache_size=-8")    # tiny cache: no cheating from RAM
    with open(ranges) as f:
        rs = json.load(f)
    got = b""
    if layout == "pages":
        ps = _pages_for(rs, PAGE)
        acc = {}
        for i in range(0, len(ps), 900):
            chunk = ps[i:i + 900]
            q = "SELECT idx, data FROM pages WHERE idx IN (%s)" % \
                ",".join("?" * len(chunk))
            for idx, data in con.execute(q, chunk):
                acc[idx] = data
        for a, b in rs:
            lo, hi = a // PAGE, (max(a, b - 1)) // PAGE
            buf = b"".join(acc.get(p, b"") for p in range(lo, hi + 1))
            base = lo * PAGE
            got += buf[a - base:b - base]
    else:
        for a, b in rs:
            cur = con.execute("SELECT substr(blob, ?, ?) FROM src WHERE id=1",
                              (a + 1, b - a))
            row = cur.fetchone()
            got += row[0] if row and row[0] is not None else b""
    if out_prefix:
        with open(out_prefix, "wb") as f:
            f.write(got)
    print(json.dumps({"ranges": len(rs), "got": len(got)}))
    con.close()


def cmd_trace2bytes(trace, match, page):
    total = 0
    reads = 0
    ranges = []
    rx = re.compile(r"pread64\(\d+<([^>]*)>,\s*(?:\"[^\"]*\"|[^,]+),\s*(\d+),\s*(\d+)\)\s*=\s*(\d+)")
    rx2 = re.compile(r"read\(\d+<([^>]*)>,\s*(?:\"[^\"]*\"|[^,]+),\s*(\d+)\)\s*=\s*(\d+)")
    with open(trace, "r", errors="replace") as f:
        for line in f:
            if match not in line:
                continue
            m = rx.search(line)
            if m:
                path, off, ret = m.group(1), int(m.group(3)), int(m.group(4))
                if match in path and ret > 0:
                    total += ret
                    reads += 1
                    ranges.append((off, off + ret))
                continue
            m = rx2.search(line)
            if m:
                path, ret = m.group(1), int(m.group(3))
                if match in path and ret > 0:
                    total += ret
                    reads += 1
    ranges.sort()
    merged = []
    for a, b in ranges:
        if merged and a <= merged[-1][1]:
            merged[-1][1] = max(merged[-1][1], b)
        else:
            merged.append([a, b])
    pages = 0
    if page > 0:
        pages = len({p for a, b in merged
                     for p in range(a // page, (b + page - 1) // page)})
    print(json.dumps({"reads": reads, "distinct_ranges": len(merged),
                      "bytes": total, "pages": pages, "page_size": page,
                      "regions": merged}))


def main(argv):
    if len(argv) < 2:
        die("usage: build|query|trace2bytes")
    cmd = argv[1]
    vals = {}
    i = 2
    while i < len(argv) - 1:
        if argv[i].startswith("--"):
            vals[argv[i]] = argv[i + 1]
            i += 2
        else:
            i += 1
    if cmd == "build":
        cmd_build(vals["--source"], vals["--db"], vals.get("--layout", "pages"))
    elif cmd == "query":
        cmd_query(vals["--db"], vals["--ranges"], vals.get("--layout", "pages"),
                  vals.get("--out-prefix"))
    elif cmd == "trace2bytes":
        match = vals.get("--match")
        if not match:
            match = os.path.basename(vals["--db"])
        cmd_trace2bytes(vals["--trace"], match, int(vals.get("--page", "4096")))
    else:
        die("unknown command %s" % cmd)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
