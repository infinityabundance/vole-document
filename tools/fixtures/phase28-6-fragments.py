#!/usr/bin/env python3
# Phase 28.6 — measurement-only fragment-locality prototype.
#
# This is a SIDE ANALYSIS, not a persistent wire format. For each large source it
# builds candidate *fragmentations* (fixed byte sizes; format-native structural
# boundaries) and, for a pre-registered narrow request, computes the **minimum set
# of fragments whose verified bytes are actually required**: the fragments that
# intersect the request's required source byte range(s). It also reports the
# directory a fragment store would need, so the metadata overhead is explicit.
#
#   python3 tools/fixtures/phase28-6-fragments.py corpus   --out DIR
#   python3 tools/fixtures/phase28-6-fragments.py analyze  --fmt F --source S \
#       --answer A.json --out OUT.json
#
# Required source ranges (the floor's numerator) are derived per format:
#   json/parquet : the adapter's own `source_span` (exact source coordinates)
#   csv          : the union of the answer's per-cell spans (or `source_span`)
#   docx         : the compressed ZIP member span(s) of the observed part plus the
#                  package directory (a DEFLATE member cannot be range-accessed)
#   pdf          : the objects reachable from the page node (via qpdf's object
#                  graph, NOT traversing /Parent) plus the xref/trailer tail
#
# Directory model: one entry per fragment = u64 offset + u32 length + sha256 =
# 44 B, stated as 48 B with padding. The overhead is reported, never hidden.

import hashlib
import importlib.util
import json
import os
import re
import struct
import subprocess
import sys
import zipfile

ENTRY_BYTES = 48
FIXED_SIZES = [4096, 16384, 65536, 262144]
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))  # repo root (/work)


def die(msg):
    sys.stderr.write("phase28-6-fragments: %s\n" % msg)
    sys.exit(2)


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def load_mod(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


# ---------------------------------------------------------------------------
# corpus
# ---------------------------------------------------------------------------
def gen_json(target):
    rows = []
    size = 0
    i = 0
    while size < target:
        row = ('{"id":%d,"name":"item-%d","vals":[%d,%d,%d],"flag":%s}'
               % (i, i, i, i + 1, i + 2, "true" if i % 2 == 0 else "false"))
        rows.append(row)
        size += len(row) + 2
        i += 1
    body = ",\n".join(rows)
    return ('{"marker":"ZEBRA_MARKER_28_6","items":[' + body + "]}").encode("utf-8")


def real_row(manifest, doc_id):
    with open(manifest) as f:
        for line in f:
            c = line.rstrip("\n").split("\t")
            if c and c[0] == doc_id:
                # id agency title pubid format url landing utc sha256 bytes ...
                return {"sha256": c[8], "bytes": int(c[9]), "fmt": c[4], "agency": c[1]}
    die("id %s not in manifest" % doc_id)


def cmd_corpus(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    rows = []

    manifest = os.path.join(ROOT, "real100-v1/manifest.tsv")
    corpus_root = os.path.join(ROOT, "real100-v1/documents")
    for doc_id in ("nasa-pdf-0021", "nist-docx-0001"):
        r = real_row(manifest, doc_id)
        src = os.path.join(corpus_root, r["agency"], r["fmt"], "%s.%s" % (doc_id, r["fmt"]))
        if not os.path.exists(src):
            die("real corpus file missing: %s" % src)
        got = sha256_file(src)
        if got != r["sha256"]:
            die("real corpus SHA-256 mismatch for %s: %s != %s" % (doc_id, got, r["sha256"]))
        dst = os.path.join(out_dir, "real.%s" % r["fmt"])
        if not os.path.exists(dst):
            with open(src, "rb") as fi, open(dst, "wb") as fo:
                while True:
                    b = fi.read(1 << 20)
                    if not b:
                        break
                    fo.write(b)
        rows.append((r["fmt"], dst))

    # deterministic large JSON (same shape as make-json.py `large`, bigger)
    jp = os.path.join(out_dir, "large.json")
    with open(jp, "wb") as f:
        f.write(gen_json(16 * 1024 * 1024))
    rows.append(("json", jp))

    # deterministic CSV / Parquet via the shipped stdlib generators
    mkc = load_mod("mk_csv", os.path.join(HERE, "make-csv.py"))
    cp = os.path.join(out_dir, "large.csv")
    with open(cp, "wb") as f:
        f.write(mkc.large(16 * 1024 * 1024))
    rows.append(("csv", cp))

    mkp = load_mod("mk_parquet", os.path.join(HERE, "make-parquet.py"))
    pp = os.path.join(out_dir, "large.parquet")
    d = mkp.large(16 * 1024 * 1024)
    mkp.write_parquet(pp, d["columns"], 50000)
    rows.append(("parquet", pp))

    print("fmt\tpath\tbytes\tsha256")
    for fmt, path in rows:
        print("%s\t%s\t%d\t%s" % (fmt, path, os.path.getsize(path), sha256_file(path)))


# ---------------------------------------------------------------------------
# required source ranges
# ---------------------------------------------------------------------------
def merge_ranges(rs):
    rs = sorted((int(a), int(b)) for a, b in rs if b > a)
    out = []
    for a, b in rs:
        if out and a <= out[-1][1]:
            out[-1][1] = max(out[-1][1], b)
        else:
            out.append([a, b])
    return out


def pdf_object_spans(path):
    with open(path, "rb") as f:
        data = f.read()
    spans = {}
    for m in re.finditer(rb'(\d+)\s+(\d+)\s+obj', data):
        num = int(m.group(1))
        end = data.find(b"endobj", m.end())
        if end == -1:
            end = len(data)
        else:
            end += len(b"endobj")
        if num not in spans or m.start() < spans[num][0]:
            spans[num] = (m.start(), end)
    tail = data.rfind(b"startxref")
    tail = tail if tail != -1 else len(data)
    return spans, tail, len(data)


def pdf_required(path):
    with open(path, "rb") as f:
        data = f.read()
    spans, tail, n = pdf_object_spans(path)
    # qpdf object graph (v1). /Parent is NOT traversed: from the page node the
    # upward edge reaches the whole page tree, which is not required to render
    # page 0. Everything else reachable from the page object is.
    p = subprocess.run(["qpdf", "--json=1", "--json-key=objects", path],
                       capture_output=True)
    if p.returncode not in (0, 2, 3):
        die("qpdf failed: %s" % p.stderr.decode()[:200])
    objs = json.loads(p.stdout).get("objects", {})

    def onum(k):
        m = re.match(r"(\d+)", k)
        return int(m.group(1)) if m else None

    def ref(v):
        if isinstance(v, str):
            m = re.match(r"^(\d+)\s+\d+\s+R$", v.strip())
            return int(m.group(1)) if m else None
        return None

    def refs(v, out, skip):
        if isinstance(v, dict):
            for k, vv in v.items():
                if k in skip:
                    continue
                if k == "/Kids":
                    continue
                refs(vv, out, skip)
        elif isinstance(v, list):
            for vv in v:
                refs(vv, out, skip)
        else:
            r = ref(v)
            if r is not None:
                out.add(r)

    byid = {}
    for k, v in objs.items():
        num = onum(k)
        if num is not None:
            byid[num] = v
    pages = sorted(num for num, v in byid.items()
                   if isinstance(v, dict) and v.get("/Type") == "/Page")
    if not pages:
        die("no /Page in %s" % path)
    p0 = pages[0]
    seen = {p0}
    stack = [p0]
    while stack:
        o = stack.pop()
        rs = set()
        refs(byid[o], rs, {"/Parent"})
        for r in rs:
            if r not in seen and r in byid:
                seen.add(r)
                stack.append(r)
    ranges = [spans[o] for o in seen if o in spans]
    ranges.append((tail, n))  # xref/trailer tail
    ranges = merge_ranges(ranges)
    return ranges, {"page_object": p0, "page_count": len(pages),
                    "reachable_objects": len(seen), "total_objects": len(byid)}


def docx_required(path, answer):
    prov = answer.get("provenance", "") or ""
    m = re.search(r"part=([^;]+)", prov)
    part = m.group(1) if m else "/word/document.xml"
    part = part.lstrip("/")
    n = os.path.getsize(path)
    with zipfile.ZipFile(path) as z:
        infos = {i.filename: i for i in z.infolist()}
        if part not in infos:
            die("docx part %s not in package" % part)
        info = infos[part]
        hoff = info.header_offset
        with open(path, "rb") as f:
            f.seek(hoff)
            lh = f.read(30)
            if lh[:4] != b"PK\x03\x04":
                die("bad local header for %s" % part)
            nlen, elen = struct.unpack("<HH", lh[26:30])
        data_end = hoff + 30 + nlen + elen + info.compress_size
        member = (hoff, min(data_end, n))
        # package directory: the central directory + EOCD, i.e. everything after
        # the end of the last member's data.
        members = []
        for i in z.infolist():
            with open(path, "rb") as f:
                f.seek(i.header_offset)
                l2 = f.read(30)
                nl, el = struct.unpack("<HH", l2[26:30])
            members.append((i.header_offset, i.header_offset + 30 + nl + el + i.compress_size))
        members = merge_ranges(members)
        dir_start = min(n, members[-1][1]) if members else 0
        return merge_ranges([member, (dir_start, n)]), {
            "member": part, "member_span": list(member),
            "part_uncompressed": info.file_size,
            "part_compressed": info.compress_size}


def required_ranges(fmt, path, answer):
    detail = {}
    if fmt in ("json", "parquet"):
        sp = answer.get("source_span")
        return ([list(sp)] if sp else []), detail
    if fmt == "csv":
        v = answer.get("value")
        if isinstance(v, dict) and v.get("cells"):
            return merge_ranges([[c["span"][0], c["span"][1]] for c in v["cells"]]), detail
        sp = answer.get("source_span")
        return ([list(sp)] if sp else []), detail
    if fmt == "docx":
        rs, detail = docx_required(path, answer)
        return rs, detail
    if fmt == "pdf":
        rs, detail = pdf_required(path)
        return rs, detail
    die("unknown fmt %s" % fmt)


# ---------------------------------------------------------------------------
# fragmentations
# ---------------------------------------------------------------------------
def partition(units, n):
    """Turn disjoint (possibly adjacent) units into a partition of [0,n).

    Adjacent units are kept as SEPARATE fragments: that is exactly the point of a
    format-native directory (each row group / member / record is its own unit).
    """
    units = sorted((max(0, a), min(n, b)) for a, b in units if b > a)
    clean = []
    for a, b in units:
        if clean and a < clean[-1][1]:
            a = clean[-1][1]
        if b > a:
            clean.append((a, b))
    frags = []
    pos = 0
    for a, b in clean:
        if a > pos:
            frags.append((pos, a))
        frags.append((a, b))
        pos = b
    if pos < n:
        frags.append((pos, n))
    return frags


def native_schemes(fmt, path):
    """Return [(scheme_name, units)] of format-native fragmentation candidates."""
    if fmt == "pdf":
        spans, tail, n = pdf_object_spans(path)
        return [("native", [spans[k] for k in sorted(spans)])], n
    if fmt == "docx":
        n = os.path.getsize(path)
        with zipfile.ZipFile(path) as z:
            units = []
            for i in z.infolist():
                with open(path, "rb") as f:
                    f.seek(i.header_offset)
                    l2 = f.read(30)
                    nl, el = struct.unpack("<HH", l2[26:30])
                units.append((i.header_offset,
                              i.header_offset + 30 + nl + el + i.compress_size))
            return [("native", units)], n
    if fmt == "csv":
        with open(path, "rb") as f:
            data = f.read()
        units = []
        start = 0
        inq = False
        i = 0
        n = len(data)
        while i < n:
            c = data[i]
            if c == 0x22:
                if inq and i + 1 < n and data[i + 1] == 0x22:
                    i += 2
                    continue
                inq = not inq
            elif c == 0x0A and not inq:
                units.append((start, i + 1))
                start = i + 1
            i += 1
        if start < n:
            units.append((start, n))
        return [("native", units)], n
    if fmt == "json":
        with open(path, "rb") as f:
            data = f.read()
        return [("native", json_native_units(data))], len(data)
    if fmt == "parquet":
        with open(path, "rb") as f:
            data = f.read()
        n = len(data)
        footer = parquet_footer(data)
        return [("native_chunk", parquet_chunk_spans(data) + [footer]),
                ("native_rg", parquet_row_group_spans(data) + [footer])], n
    die("no native units for %s" % fmt)


def _skip_ws(data, j, n):
    while j < n and data[j] in b" \t\r\n":
        j += 1
    return j


def _skip_value(data, j, n):
    j = _skip_ws(data, j, n)
    if j >= n:
        return j
    c = data[j]
    if c == 0x22:  # string
        j += 1
        while j < n:
            if data[j] == 0x5C:
                j += 2
                continue
            if data[j] == 0x22:
                return j + 1
            j += 1
        return j
    if c in (0x7B, 0x5B):  # { [
        depth = 0
        while j < n:
            ch = data[j]
            if ch == 0x22:
                j += 1
                while j < n:
                    if data[j] == 0x5C:
                        j += 2
                        continue
                    if data[j] == 0x22:
                        j += 1
                        break
                    j += 1
                continue
            if ch in (0x7B, 0x5B):
                depth += 1
            elif ch in (0x7D, 0x5D):
                depth -= 1
                if depth == 0:
                    return j + 1
            j += 1
        return j
    while j < n and data[j] not in b",}] \t\r\n":
        j += 1
    return j


def _json_children(data, n, opener, start):
    """Direct children spans of the container whose opener is at `opener`.

    Each child owns the separator that precedes it; `start` is where the first
    child's span begins (0 for the root so leading bytes are covered).
    """
    out = []
    i = opener + 1
    pos = start
    while True:
        i = _skip_ws(data, i, n)
        if i >= n or data[i] in (0x7D, 0x5D):  # } ]
            out.append((pos, n))
            break
        if data[i] == 0x2C:
            i += 1
            continue
        ve = _skip_value(data, i, n)
        if ve <= i:
            i += 1
            continue
        out.append((pos, ve))
        pos = ve
        i = _skip_ws(data, ve, n)
    return [c for c in out if c[1] > c[0]]


def _first_opener(data, n):
    i = _skip_ws(data, 0, n)
    return i if i < n and data[i] in (0x7B, 0x5B) else None


def json_native_units(data):
    """Native JSON units: expand the root's direct containers one level so a
    top-level record array becomes one unit per record (the natural 'row')."""
    n = len(data)
    i = _first_opener(data, n)
    if i is None:
        return [(0, n)]
    units = []
    for (a, b) in _json_children(data, n, i, 0):
        k = a
        while k < b and data[k] in b" \t\r\n,":
            k += 1
        if k < b and data[k] in (0x7B, 0x5B):
            units.extend(_json_children(data, n, k, a))
        else:
            units.append((a, b))
    return units


class _TC(object):
    """A minimal Thrift-Compact reader (only what the Parquet footer needs)."""

    def __init__(self, b):
        self.b = b
        self.i = 0

    def _byte(self):
        v = self.b[self.i]
        self.i += 1
        return v

    def _varint(self):
        shift = 0
        out = 0
        while True:
            c = self._byte()
            out |= (c & 0x7F) << shift
            if not (c & 0x80):
                return out
            shift += 7

    def _zig(self):
        u = self._varint()
        return (u >> 1) ^ -(u & 1)

    def skip(self, t):
        if t in (1, 2):
            return t == 1
        if t == 3:
            return self._byte()
        if t in (4, 5, 6):
            return self._zig()
        if t == 7:
            self.i += 8
            return 0
        if t == 8:
            ln = self._varint()
            s = self.b[self.i:self.i + ln]
            self.i += ln
            return s
        if t in (9, 10):
            h = self._byte()
            size = (h & 0xF0) >> 4
            et = h & 0x0F
            if size == 15:
                size = self._varint()
            return [self.skip(et) for _ in range(size)]
        if t == 11:
            size = self._varint()
            if size == 0:
                return {}
            kt = self._byte()
            vt = kt & 0x0F
            kt = (kt & 0xF0) >> 4
            return {self.skip(kt): self.skip(vt) for _ in range(size)}
        if t == 12:
            return self.struct()
        raise ValueError("thrift type %d" % t)

    def struct(self):
        out = {}
        last = 0
        while True:
            h = self._byte()
            if h == 0:
                return out
            delta = (h & 0xF0) >> 4
            t = h & 0x0F
            fid = last + delta if delta else self._zig()
            last = fid
            out[fid] = self.skip(t)


def parquet_footer(data):
    n = len(data)
    flen = struct.unpack("<I", data[n - 8:n - 4])[0]
    return (n - 8 - flen, n)


def parquet_chunk_spans(data):
    """Per row-group, per-column chunk spans (the Parquet 'page' unit here)."""
    n = len(data)
    flen = struct.unpack("<I", data[n - 8:n - 4])[0]
    meta = _TC(data[n - 8 - flen:n - 8]).struct()
    spans = []
    for rg in meta.get(4, []):
        for col in rg.get(1, []):
            cm = col.get(3)
            if not cm:
                continue
            start = cm.get(11) or cm.get(9) or 0
            size = cm.get(7, 0)
            if size:
                spans.append((start, start + size))
    return spans


def parquet_row_group_spans(data):
    """Row-group spans from the Parquet footer metadata (one unit per group)."""
    n = len(data)
    flen = struct.unpack("<I", data[n - 8:n - 4])[0]
    meta = _TC(data[n - 8 - flen:n - 8]).struct()
    spans = []
    for rg in meta.get(4, []):  # 4 = row_groups
        lo = None
        hi = None
        for col in rg.get(1, []):  # 1 = columns
            cm = col.get(3)  # 3 = meta_data
            if not cm:
                continue
            start = cm.get(11) or cm.get(9) or 0  # dict_page_offset / data_page_offset
            size = cm.get(7, 0)  # total_compressed_size
            if not size:
                continue
            lo = start if lo is None else min(lo, start)
            hi = start + size if hi is None else max(hi, start + size)
        if lo is not None and hi is not None and hi > lo:
            spans.append((lo, hi))
    return spans


def floor(scheme, req):
    tot = 0
    cnt = 0
    for a, b in scheme:
        for ra, rb in req:
            if a < rb and ra < b:
                tot += b - a
                cnt += 1
                break
    return cnt, tot


def cmd_analyze(fmt, path, answer_path, out_path):
    n = os.path.getsize(path)
    with open(answer_path) as f:
        lines = [l for l in f if l.strip()]
    answer = json.loads(lines[-1]) if lines else {}
    rs, detail = required_ranges(fmt, path, answer)
    required_bytes = sum(b - a for a, b in rs)

    schemes = {}
    for k in FIXED_SIZES:
        frags = [(i, min(i + k, n)) for i in range(0, n, k)] or [(0, n)]
        c, b = floor(frags, rs)
        schemes["fixed_%d" % k] = {
            "n_fragments": len(frags), "hit_fragments": c,
            "floor_bytes": b, "directory_bytes": len(frags) * ENTRY_BYTES,
        }
    units_list, un = native_schemes(fmt, path)
    for name, units in units_list:
        frags = partition(units, n)
        c, b = floor(frags, rs)
        schemes[name] = {
            "n_fragments": len(frags), "hit_fragments": c,
            "floor_bytes": b, "directory_bytes": len(frags) * ENTRY_BYTES,
            "unit_count": len(units),
        }

    out = {
        "fmt": fmt, "source": path, "source_bytes": n,
        "required_ranges": rs, "required_bytes": required_bytes,
        "required_n_ranges": len(rs),
        "entry_bytes": ENTRY_BYTES,
        "schemes": schemes,
        "detail": detail,
        "answer_source_span": answer.get("source_span"),
        "answer_bytes_read": (answer.get("stats") or {}).get("bytes_read"),
    }
    with open(out_path, "w") as f:
        json.dump(out, f, indent=1, sort_keys=True)
    print(json.dumps({k: out[k] for k in
                      ("fmt", "source_bytes", "required_bytes",
                       "required_n_ranges")}))


def main(argv):
    if len(argv) < 2:
        die("usage: corpus|analyze")
    cmd = argv[1]
    if cmd == "corpus":
        out = argv[argv.index("--out") + 1]
        cmd_corpus(out)
        return 0
    if cmd == "analyze":
        vals = {}
        i = 2
        while i < len(argv):
            vals[argv[i]] = argv[i + 1]
            i += 2
        cmd_analyze(vals["--fmt"], vals["--source"], vals["--answer"], vals["--out"])
        return 0
    die("unknown command %s" % cmd)


if __name__ == "__main__":
    sys.exit(main(sys.argv))
