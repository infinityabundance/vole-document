#!/usr/bin/env python3
"""Phase 22.5 — local HTTP range server + coalescing range client (Python stdlib).

## What this is, and what it is NOT

This is an explicitly-labelled **model** of remote selective reads, NOT a real
remote (S3) benchmark.  Real object storage is unavailable in the pinned lane, so
this fixture provides two things that ARE real and checkable on localhost:

  * a tiny `http.server`-based **byte-range file server** (`serve`) that honours
    `Range: bytes=a-b` with `206 Partial Content` + `Content-Range` and logs one
    JSON line per request; and
  * a pure-Python **coalescing range client** (`fetch`) that, given a list of
    `(path, offset, length)` ranges, merges neighbouring ranges on the same
    object under a gap budget and a per-request byte budget, issues real HTTP
    range GETs, and returns `(requests, bytes_transferred)`.

The byte counts fed to the client come from **real, local, deterministic
measurements** (VOLE's instrumented per-class `*_bytes_read` stats; SQLite's
`pread64` offsets captured under `strace`).  What is modelled is (a) the mapping
of those byte counts onto concrete ranges where the exact node->offset map is not
exposed, and (b) the latency/cost constants.  Both are stated in the court
receipt.  No network egress is used: everything is loopback.

## Subcommands

    layout   --dir STORE --img IMG --manifest M   concatenate a store into one
                                                   flat immutable object + layout
    derive   --manifest M --img-name IMG --descriptor N --field N --index N \
             --seed N --out PLAN                   class byte counts -> ranges
    trace2plan --trace FILE --db NAME --out PLAN  strace pread64 -> ranges
    coalesce --plan PLAN [--gap G] [--max-bytes N] --out PLAN2
    serve    --root R [--port P] --log L           HTTP range server (loopback)
    fetch    --root R --port P --plan PLAN --out OUT [--gap G] [--max-bytes N]
             [--no-verify]                         issue the coalesced requests

Plan format is a JSON list of `{"path","offset","length"}` objects.  `fetch`
prints a JSON object with `requests`, `bytes_transferred`, the merged ranges and
per-range verification results.
"""

import argparse
import hashlib
import http.client
import json
import os
import re
import shutil
import signal
import socket
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


# ---------------------------------------------------------------------------
# Coalescing planner
# ---------------------------------------------------------------------------

def coalesce(ranges, gap=0, max_bytes=None):
    """Merge `(path, offset, length)` ranges on the same object.

    Two ranges on the same path merge when the byte gap between them is
    `<= gap` AND the merged span stays within `max_bytes` (0/None = unlimited).
    `gap=0` therefore merges only ranges that touch/overlap: the byte-minimal
    plan.  Returns a new list sorted by `(path, offset)`.
    """
    by = {}
    for r in ranges:
        by.setdefault(r["path"], []).append((int(r["offset"]), int(r["length"])))
    out = []
    for path in sorted(by):
        cur = None
        for off, ln in sorted(by[path]):
            if ln <= 0:
                continue
            if cur is None:
                cur = [off, off + ln]
                continue
            gap_bytes = off - cur[1]
            span = max(cur[1], off + ln) - cur[0]
            mergeable = (gap_bytes <= gap) and (
                not max_bytes or span <= max_bytes)
            if mergeable:
                cur[1] = max(cur[1], off + ln)
            else:
                out.append({"path": path, "offset": cur[0],
                            "length": cur[1] - cur[0]})
                cur = [off, off + ln]
        if cur is not None:
            out.append({"path": path, "offset": cur[0],
                        "length": cur[1] - cur[0]})
    return out


def _sha256_slice(root, path, off, ln):
    with open(os.path.join(root, path), "rb") as f:
        f.seek(off)
        return hashlib.sha256(f.read(ln)).hexdigest()


def fetch(root, port, ranges, gap=0, max_bytes=None, verify=True,
          host="127.0.0.1", timeout=60.0):
    """Issue the coalesced range GETs against the local range server.

    Returns a dict with the real request count, the real bytes the server sent,
    the merged plan, and per-range verification (HTTP 206 + exact length +
    SHA-256 of the returned body vs the source object slice).
    """
    merged = coalesce(ranges, gap=gap, max_bytes=max_bytes)
    conn = http.client.HTTPConnection(host, port, timeout=timeout)
    requests = 0
    bytes_transferred = 0
    verified = 0
    mismatches = []
    for r in merged:
        path, off, ln = r["path"], r["offset"], r["length"]
        body = status = None
        for _ in range(3):
            try:
                conn.request("GET", "/" + path,
                             headers={"Range": "bytes=%d-%d" % (off, off + ln - 1)})
                resp = conn.getresponse()
                status = resp.status
                body = resp.read()
                break
            except Exception as exc:  # reconnect and retry
                conn.close()
                conn = http.client.HTTPConnection(host, port, timeout=timeout)
                status, body = None, None
        if body is None:
            mismatches.append({"path": path, "offset": off, "length": ln,
                               "error": "request_failed"})
            continue
        requests += 1
        bytes_transferred += len(body)
        ok = (status == 206 and len(body) == ln)
        detail = "ok" if ok else "status=%s len=%d want=%d" % (status, len(body), ln)
        if ok and verify:
            exp = _sha256_slice(root, path, off, ln)
            got = hashlib.sha256(body).hexdigest()
            if exp != got:
                ok = False
                detail = "sha256_mismatch"
        if ok:
            verified += 1
        else:
            mismatches.append({"path": path, "offset": off, "length": ln,
                               "detail": detail})
    try:
        conn.close()
    except Exception:
        pass
    return {"requests": requests, "bytes_transferred": bytes_transferred,
            "n_ranges": len(merged), "merged": merged,
            "verified": verified, "mismatches": mismatches}


# ---------------------------------------------------------------------------
# Layout / derivation
# ---------------------------------------------------------------------------

def build_layout(dir_, img, manifest):
    """Concatenate every regular file under `dir_` (sorted by path) into one
    flat immutable object `img`, and write the offset directory `manifest`."""
    files = []
    for base, _dirs, names in os.walk(dir_):
        for n in names:
            full = os.path.join(base, n)
            if os.path.isfile(full):
                rel = os.path.relpath(full, dir_).replace(os.sep, "/")
                files.append((rel, full))
    files.sort()
    off = 0
    recs = []
    with open(img, "wb") as out:
        for rel, full in files:
            ln = os.path.getsize(full)
            with open(full, "rb") as f:
                shutil.copyfileobj(f, out, 1 << 16)
            recs.append({"path": rel, "offset": off, "length": ln})
            off += ln
    json.dump({"files": recs, "total": off}, open(manifest, "w"), indent=1)
    return recs


def namespace_regions(manifest):
    """Contiguous flat region [start, end) per top-level namespace directory."""
    regs = {}
    for r in manifest["files"]:
        ns = r["path"].split("/", 1)[0]
        a, b = r["offset"], r["offset"] + r["length"]
        if ns in regs:
            regs[ns] = (min(regs[ns][0], a), max(regs[ns][1], b))
        else:
            regs[ns] = (a, b)
    return regs


def derive_plan(manifest, img_name, classes):
    """Map per-class byte counts onto contiguous prefixes of each class's
    namespace region in the flat image.

    Byte counts are exact (instrumented); the *locality* is an upper-bound model
    (a contiguous prefix of the class's namespace region), so the resulting
    request count is a lower bound on the true scattered read.
    """
    regs = namespace_regions(manifest)
    plan = []
    clamped = {}
    for ns, count in classes:
        count = int(count or 0)
        if count <= 0:
            continue
        a, b = regs.get(ns, (0, 0))
        region = b - a
        ln = min(count, region)
        if ln < count:
            clamped[ns] = count - region
        if ln > 0:
            plan.append({"path": img_name, "offset": a, "length": ln})
    return plan, clamped


# ---------------------------------------------------------------------------
# strace -> ranges
# ---------------------------------------------------------------------------

# `strace -yy` annotates the fd with the absolute path; long read buffers are
# elided with a trailing `...`, so the buffer field is matched greedily and the
# (length, offset, return) integers are anchored at the end of the line.
_PREAD = re.compile(
    r"pread64\((\d+)<([^>]*)>.*,\s*(\d+),\s*(\d+)\)\s*=\s*(-?\d+)\s*$")


def trace_ranges(trace, db):
    """Parse `strace -e trace=pread64 -yy` output for one DB object.

    Returns (raw_calls, distinct_ranges) where ranges are `(offset, length)`
    pread segments on the object whose annotated fd path ends with `db`.
    """
    calls = []
    with open(trace, errors="replace") as f:
        for line in f:
            m = _PREAD.search(line)
            if not m:
                continue
            path = m.group(2)
            # `-yy` annotates the fd with the absolute path (or "path (deleted)").
            # Keep only the target object and drop any unlinked temporary.
            if path.endswith(" (deleted)"):
                continue
            if not path.endswith(db):
                continue
            length, off, got = int(m.group(3)), int(m.group(4)), int(m.group(5))
            if got <= 0:
                continue
            calls.append((off, min(length, got)))
    distinct = sorted(set(calls))
    plan = [{"path": db, "offset": o, "length": l} for o, l in distinct]
    return len(calls), len(distinct), plan


# ---------------------------------------------------------------------------
# HTTP range server
# ---------------------------------------------------------------------------

class _RangeHandler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    root = "."
    logfile = None
    _lock = threading.Lock()

    def log_message(self, *args):  # silence the default stderr access log
        pass

    def _resolve(self, urlpath):
        rel = urlpath.split("?", 1)[0].lstrip("/")
        full = os.path.realpath(os.path.join(self.root, rel))
        root = os.path.realpath(self.root)
        if full != root and not full.startswith(root + os.sep):
            return None
        return full

    def do_HEAD(self):
        self._serve(head=True)

    def do_GET(self):
        self._serve(head=False)

    def _serve(self, head):
        urlpath = self.path.split("?", 1)[0]
        full = self._resolve(urlpath)
        if full is None or not os.path.isfile(full):
            self._log(urlpath, None, 0, 404, 0)
            self.send_error(404)
            return
        size = os.path.getsize(full)
        start, end, status = 0, size - 1, 200
        rng = self.headers.get("Range")
        if rng and rng.startswith("bytes="):
            spec = rng[len("bytes="):].split(",")[0].strip()
            a, _, b = spec.partition("-")
            try:
                if a == "":
                    n = int(b)
                    start = max(0, size - n)
                    end = size - 1
                else:
                    start = int(a)
                    end = int(b) if b else size - 1
            except ValueError:
                self._log(urlpath, None, 0, 400, 0)
                self.send_error(400)
                return
            if start >= size or start > end:
                self.send_response(416)
                self.send_header("Content-Range", "bytes */%d" % size)
                self.send_header("Content-Length", "0")
                self.end_headers()
                self._log(urlpath, start, 0, 416, 0)
                return
            end = min(end, size - 1)
            status = 206
        length = end - start + 1
        self.send_response(status)
        self.send_header("Content-Type", "application/octet-stream")
        self.send_header("Accept-Ranges", "bytes")
        self.send_header("Content-Length", str(length))
        if status == 206:
            self.send_header("Content-Range", "bytes %d-%d/%d" % (start, end, size))
        self.end_headers()
        sent = 0
        if not head:
            try:
                with open(full, "rb") as f:
                    f.seek(start)
                    remain = length
                    while remain > 0:
                        chunk = f.read(min(1 << 16, remain))
                        if not chunk:
                            break
                        self.wfile.write(chunk)
                        sent += len(chunk)
                        remain -= len(chunk)
            except (BrokenPipeError, ConnectionResetError):
                pass
        self._log(urlpath, start if status == 206 else 0, length, status,
                  sent if not head else length)

    def _log(self, path, start, length, status, sent):
        if not self.logfile:
            return
        rec = json.dumps({"path": path, "start": start, "length": length,
                          "status": status, "sent": sent})
        with self._lock:
            with open(self.logfile, "a") as f:
                f.write(rec + "\n")
                f.flush()


def cmd_serve(args):
    handler = type("RangeHandler", (_RangeHandler,), {})
    handler.root = os.path.realpath(args.root)
    handler.logfile = args.log
    httpd = ThreadingHTTPServer(("127.0.0.1", args.port), handler)
    httpd.daemon_threads = True
    port = httpd.server_address[1]
    print("LISTENING %d" % port, flush=True)
    stopping = {"v": False}

    def _stop(_sig, _frm):
        if not stopping["v"]:
            stopping["v"] = True
            threading.Thread(target=httpd.shutdown, daemon=True).start()
    signal.signal(signal.SIGTERM, _stop)
    signal.signal(signal.SIGINT, _stop)
    try:
        httpd.serve_forever(poll_interval=0.1)
    finally:
        httpd.server_close()


def wait_for_port(port, host="127.0.0.1", timeout=10.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            with socket.create_connection((host, port), timeout=0.5):
                return True
        except OSError:
            time.sleep(0.05)
    return False


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main(argv=None):
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)

    a = sub.add_parser("layout")
    a.add_argument("--dir", required=True)
    a.add_argument("--img", required=True)
    a.add_argument("--manifest", required=True)

    a = sub.add_parser("derive")
    a.add_argument("--manifest", required=True)
    a.add_argument("--img-name", default="store.img")
    a.add_argument("--descriptor", type=int, default=0)
    a.add_argument("--field", type=int, default=0)
    a.add_argument("--index", type=int, default=0)
    a.add_argument("--seed", type=int, default=0)
    a.add_argument("--out", required=True)

    a = sub.add_parser("trace2plan")
    a.add_argument("--trace", required=True)
    a.add_argument("--db", required=True)
    a.add_argument("--out", required=True)

    a = sub.add_parser("coalesce")
    a.add_argument("--plan", required=True)
    a.add_argument("--gap", type=int, default=0)
    a.add_argument("--max-bytes", type=int, default=0)
    a.add_argument("--out", required=True)

    a = sub.add_parser("serve")
    a.add_argument("--root", required=True)
    a.add_argument("--port", type=int, default=0)
    a.add_argument("--log", required=True)

    a = sub.add_parser("fetch")
    a.add_argument("--root", required=True)
    a.add_argument("--port", type=int, required=True)
    a.add_argument("--plan", required=True)
    a.add_argument("--gap", type=int, default=0)
    a.add_argument("--max-bytes", type=int, default=0)
    a.add_argument("--no-verify", action="store_true")
    a.add_argument("--out", default=None)

    ns = ap.parse_args(argv)

    if ns.cmd == "layout":
        recs = build_layout(ns.dir, ns.img, ns.manifest)
        print(json.dumps({"files": len(recs),
                          "total": sum(r["length"] for r in recs)}))
        return 0

    if ns.cmd == "derive":
        man = json.load(open(ns.manifest))
        plan, clamped = derive_plan(man, ns.img_name, [
            ("descriptor", ns.descriptor), ("field", ns.field),
            ("index", ns.index), ("fieldpack", ns.seed)])
        json.dump(plan, open(ns.out, "w"), indent=1)
        print(json.dumps({"ranges": len(plan), "clamped": clamped,
                          "bytes": sum(r["length"] for r in plan)}))
        return 0

    if ns.cmd == "trace2plan":
        raw, distinct, plan = trace_ranges(ns.trace, ns.db)
        json.dump(plan, open(ns.out, "w"), indent=1)
        print(json.dumps({"raw_preads": raw, "distinct_ranges": distinct}))
        return 0

    if ns.cmd == "coalesce":
        plan = json.load(open(ns.plan))
        merged = coalesce(plan, gap=ns.gap,
                          max_bytes=ns.max_bytes or None)
        json.dump(merged, open(ns.out, "w"), indent=1)
        print(json.dumps({"n_in": len(plan), "n_merged": len(merged),
                          "bytes": sum(r["length"] for r in merged)}))
        return 0

    if ns.cmd == "serve":
        cmd_serve(ns)
        return 0

    if ns.cmd == "fetch":
        plan = json.load(open(ns.plan))
        res = fetch(ns.root, ns.port, plan, gap=ns.gap,
                    max_bytes=ns.max_bytes or None, verify=not ns.no_verify)
        if ns.out:
            json.dump(res, open(ns.out, "w"), indent=1)
        print(json.dumps({k: res[k] for k in
                          ("requests", "bytes_transferred", "n_ranges",
                           "verified")}))
        return 0
    return 2


if __name__ == "__main__":
    sys.exit(main())
