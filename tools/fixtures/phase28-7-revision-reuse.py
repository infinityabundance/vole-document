#!/usr/bin/env python3
# Phase 28.7 — Experiment 3: cross-revision dependency reuse (MEASUREMENT ONLY).
#
# This is a SIMULATION. It ships NO wire format, no planner change, no decoder
# change, and touches NO production storage. It asks: on authentic document
# *revision families*, how much **verified work** is reusable under a
# fragment-level dependency graph, how much index memory that needs, and does it
# beat (a) a full rebuild and (b) a properly optimized versioned content-addressed
# store that dedups by content hash but has no semantic dependency edges?
#
# Subcommands
#   families --out DIR                     build revision families; print manifest TSV
#   digest   --families M --measured D --out OUT.json
#                                          fragment + build the dependency graph and
#                                          the three lanes' counters
#   aggregate --raw R --out OUT --campaign C --short S
#   decide   --receipt R
#
# Everything a decision needs is emitted as plain integers; the court seals them.
#
# Design notes (honesty):
#   * The exact authority is fragmented with CONTENT-DEFINED chunking (a gear
#     hash), the honest, load-bearing property of a "properly optimized"
#     content-addressed store: an insertion/deletion perturbs only the chunks
#     that actually changed, not every chunk after it.
#   * Content ids use BLAKE2b-256, a stand-in for the store's BLAKE3 (stdlib has
#     no BLAKE3). Dedup semantics are identical; the hash function name is stated.
#   * "Unchanged physical fragment" does NOT imply "unchanged observation": the
#     graph carries SEMANTIC edges (a DOCX style table, an XLSX shared-string
#     table) so an observation that reads no changed byte can still invalidate.
#   * Work is counted in abstract verified node executions (ADR-0027: reuse is
#     claimed by an execution counter, never wall-clock). Latency is MODELED from
#     measured full-rebuild latency and node counts; it is labelled as modeled.

import argparse
import hashlib
import json
import os
import re
import struct
import sys
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))  # repo root (/work)

# Index cost model (stated, never hidden).
CHUNK_ENTRY_BYTES = 48   # u64 offset + u32 length + 32 B content id, padded
OBS_NODE_BYTES = 48      # observation id + span, padded
EDGE_BYTES = 16          # (u32 obs, u32 dep, u32 kind, pad)
CTX_ENTRY_BYTES = 48     # semantic context part entry

# Content-defined chunking parameters (gear hash, FastCDC-flavoured).
CDC_MIN = 2048
CDC_AVG = 8192
CDC_MAX = 32768
CDC_MASK = (1 << 13) - 1  # ~8 KiB average


def die(msg):
    sys.stderr.write("phase28-7: %s\n" % msg)
    sys.exit(2)


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def sha256_bytes(b):
    return hashlib.sha256(b).hexdigest()


def cid(bs):
    """Content id of a chunk: BLAKE2b-256 (stand-in for the store's BLAKE3)."""
    return hashlib.blake2b(bs, digest_size=32).hexdigest()


def load_mod(name, path):
    import importlib.util
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


# ---------------------------------------------------------------------------
# content-defined chunking
# ---------------------------------------------------------------------------
def _gear_table():
    # Deterministic 256-entry 64-bit table (splitmix64 of the index); never
    # `random`, so regeneration is byte-identical.
    tbl = []
    for i in range(256):
        x = (i * 0x9E3779B97F4A7C15 + 0xD1B54A32D192ED03) & 0xFFFFFFFFFFFFFFFF
        x = (x ^ (x >> 30)) * 0xBF58476D1CE4E5B9 & 0xFFFFFFFFFFFFFFFF
        x = (x ^ (x >> 27)) * 0x94D049BB133111EB & 0xFFFFFFFFFFFFFFFF
        tbl.append(x ^ (x >> 31))
    return tbl


_GEAR = _gear_table()


def cdc(data):
    """Content-defined chunk boundaries -> [(start, end), ...] tiling [0, len)."""
    n = len(data)
    if n == 0:
        return []
    chunks = []
    start = 0
    h = 0
    i = 0
    while i < n:
        h = ((h << 1) + _GEAR[data[i]]) & 0xFFFFFFFFFFFFFFFF
        i += 1
        size = i - start
        if size >= CDC_MIN and ((h & CDC_MASK) == 0 or size >= CDC_MAX):
            chunks.append((start, i))
            start = i
            h = 0
    if start < n:
        chunks.append((start, n))
    return chunks


def chunk_hashes(data):
    """[(start, end, cid_hex), ...]"""
    return [(a, b, cid(data[a:b])) for a, b in cdc(data)]


def zip_fragment_spans(path):
    """Structural fragments of a ZIP container: one per member data span plus
    the central directory. Member-aligned, so a change inside one member cannot
    perturb the fragment of a neighbour (which whole-file CDC can do)."""
    parts = _zip_part_spans(path)
    n = os.path.getsize(path)
    frags = []
    ends = []
    for name in sorted(parts, key=lambda k: parts[k][0]):
        a, b, _ = parts[name]
        b = min(b, n)
        if b > a:
            frags.append((a, b))
            ends.append(b)
    cd = max(ends) if ends else 0
    if cd < n:
        frags.append((cd, n))
    return sorted(set(frags))


def fragment_list(path, fmt, data):
    """[(start, end, cid), ...] for the format's fragment scheme."""
    if fmt in ("docx", "xlsx"):
        spans = zip_fragment_spans(path)
    else:
        spans = cdc(data)
    return [(a, b, cid(data[a:b])) for a, b in spans]


def chunks_covering(chs, a, b):
    """Indices of chunks intersecting [a, b)."""
    out = []
    for k, (s, e, _) in enumerate(chs):
        if s < b and a < e:
            out.append(k)
    return out


def span_sig(chs, ranges):
    """Ordered content ids of the chunks covering the ranges (dedup, stable)."""
    idx = set()
    for a, b in ranges:
        idx.update(chunks_covering(chs, a, b))
    return tuple(chs[k][2] for k in sorted(idx))


# ---------------------------------------------------------------------------
# zip surgery: replace one member, preserving every other member's raw bytes
# ---------------------------------------------------------------------------
def zip_members(path):
    """[(name, info_dict)] in central-directory order, with raw compressed data."""
    data = open(path, "rb").read()
    z = zipfile.ZipFile(path)
    out = []
    for info in z.infolist():
        off = info.header_offset
        (sig, ver, flags, method, dtime, ddate, crc, csz, usz, nl, el) = struct.unpack(
            "<IHHHHHIIIHH", data[off:off + 30])
        name = data[off + 30:off + 30 + nl].decode("utf-8", "replace")
        extra = data[off + 30 + nl:off + 30 + nl + el]
        cstart = off + 30 + nl + el
        cdata = data[cstart:cstart + info.compress_size]
        out.append({
            "name": name, "flags": flags & ~0x08, "method": method,
            "dtime": dtime, "ddate": ddate, "crc": crc,
            "csz": info.compress_size, "usz": info.file_size,
            "extra": extra, "cdata": cdata, "isdir": info.is_dir(),
            # raw local header (30 + name + extra), preserved verbatim for
            # unchanged members so their bytes are bit-identical.
            "raw_lh": data[off:off + 30 + nl + el],
        })
    return out


def _recompress(newdata, method):
    if method == 8:
        import zlib
        co = zlib.compressobj(9, zlib.DEFLATED, -15)
        cdata = co.compress(newdata) + co.flush()
    else:
        method = 0
        cdata = newdata
    crc = zlib.crc32(newdata) & 0xFFFFFFFF
    return method, cdata, crc, len(newdata)


def zip_write(members, dst):
    with open(dst, "wb") as f:
        central = []
        for m in members:
            off = f.tell()
            if m.get("raw_lh"):
                f.write(m["raw_lh"])
            else:
                nb = m["name"].encode("utf-8")
                f.write(struct.pack("<IHHHHHIIIHH", 0x04034b50, 20, m["flags"], m["method"],
                                    m["dtime"], m["ddate"], m["crc"], m["csz"], m["usz"],
                                    len(nb), len(m["extra"])))
                f.write(nb)
                f.write(m["extra"])
            f.write(m["cdata"])
            central.append((off, m))
        cstart = f.tell()
        for off, m in central:
            nb = m["name"].encode("utf-8")
            f.write(struct.pack("<IHHHHHHIIIHHHHHII", 0x02014b50, 20, 20, m["flags"],
                                m["method"], m["dtime"], m["ddate"], m["crc"], m["csz"],
                                m["usz"], len(nb), len(m["extra"]), 0, 0, 0, 0, off))
            f.write(nb)
            f.write(m["extra"])
        csize = f.tell() - cstart
        f.write(struct.pack("<IHHHHIIH", 0x06054b50, 0, 0, len(central), len(central),
                            csize, cstart, 0))


def zip_replace_member(src, dst, member, newdata):
    members = zip_members(src)
    for m in members:
        if m["name"] == member:
            method, cdata, crc, usz = _recompress(newdata, m["method"])
            m["method"], m["cdata"], m["crc"], m["usz"] = method, cdata, crc, usz
            m["csz"] = len(cdata)
            # deflated size changed -> keep flags clean (no data descriptor) and
            # re-emit this member's local header (its raw form is now stale).
            m["flags"] = m["flags"] & ~0x08
            m.pop("raw_lh", None)
    zip_write(members, dst)


# ---------------------------------------------------------------------------
# families
# ---------------------------------------------------------------------------
def real_docx(doc_id):
    p = os.path.join(ROOT, "real100-v1/documents/nist/docx/%s.docx" % doc_id)
    return p


def gen_csv(target, variant):
    """Deterministic CSV; variant 1 = base, 2 = a LOCALIZED edit to one
    contiguous band of rows, 3 = a second localized band (the classic
    section-edit revision pattern)."""
    out = ["row_id,name,score,note\n"]
    size = len(out[0])
    i = 0
    while size < target:
        note = "n, %d" % i
        if variant >= 2 and 5000 <= i < 5200:
            note = "EDITED2 %d" % i
        if variant >= 3 and 60000 <= i < 60300:
            note = "EDITED3 %d" % i
        row = '%d,item-%d,%d,"%s"\n' % (i, i, (i * 37) % 1000, note)
        out.append(row)
        size += len(row)
        i += 1
    return "".join(out).encode("utf-8")


def gen_json(target, variant):
    rows = []
    size = 0
    i = 0
    while size < target:
        name = "item-%d" % i
        if variant >= 2 and i % 977 == 7:
            name = "ITEM-2-%d" % i
        if variant >= 3 and i % 977 == 13:
            name = "ITEM-3-%d" % i
        row = ('{"id":%d,"name":"%s","vals":[%d,%d,%d],"flag":%s}'
               % (i, name, i, i + 1, i + 2, "true" if i % 2 == 0 else "false"))
        rows.append(row)
        size += len(row) + 2
        i += 1
    body = ",\n".join(rows)
    return ('{"marker":"ZEBRA_MARKER_28_7","items":[' + body + "]}").encode("utf-8")


def _styles_variant(base_styles, bump):
    """Bump every w:sz w:val by `bump`; a real, semantic style-table change."""
    def sub(m):
        return 'w:sz w:val="%d"' % (int(m.group(1)) + bump)
    new, n = re.subn(r'w:sz w:val="(\d+)"', sub, base_styles.decode("utf-8", "replace"))
    return new.encode("utf-8"), n


def build_families(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    fams = []

    # --- real DOCX revision families (authentic published drafts) -------------
    for fam, ids in (("nist-sp800-34", ["nist-docx-0005", "nist-docx-0006",
                                        "nist-docx-0007", "nist-docx-0008"]),
                     ("nist-sp800-18", ["nist-docx-0001", "nist-docx-0002",
                                        "nist-docx-0003", "nist-docx-0004"])):
        revs = []
        for k, doc_id in enumerate(ids, start=1):
            src = real_docx(doc_id)
            if not os.path.exists(src):
                die("real corpus missing: %s (real100-v1 is gitignored; fetch it)" % src)
            dst = os.path.join(out_dir, "%s-r%d.docx" % (fam, k))
            if not os.path.exists(dst):
                with open(src, "rb") as fi, open(dst, "wb") as fo:
                    while True:
                        b = fi.read(1 << 20)
                        if not b:
                            break
                        fo.write(b)
            revs.append(dst)
        fams.append((fam, "docx", "real-draft-edit", revs))

    # --- DOCX style-table family (faithful: only word/styles.xml changes) -----
    base = real_docx("nist-docx-0005")
    if not os.path.exists(base):
        die("real corpus missing: %s" % base)
    with zipfile.ZipFile(base) as z:
        base_styles = z.read("word/styles.xml")
    s2, n2 = _styles_variant(base_styles, 2)
    s3, n3 = _styles_variant(base_styles, 4)
    if n2 == 0:
        die("docx-style family: no w:sz values to perturb in styles.xml")
    d1 = os.path.join(out_dir, "docx-style-r1.docx")
    with open(base, "rb") as fi, open(d1, "wb") as fo:
        fo.write(fi.read())
    d2 = os.path.join(out_dir, "docx-style-r2.docx")
    d3 = os.path.join(out_dir, "docx-style-r3.docx")
    zip_replace_member(base, d2, "word/styles.xml", s2)
    zip_replace_member(base, d3, "word/styles.xml", s3)
    fams.append(("docx-style", "docx", "style-table", [d1, d2, d3]))

    # --- XLSX shared-string family (faithful: only xl/sharedStrings.xml) ------
    xl = os.path.join(ROOT, "realformats-v1/documents/xlsx/poi-spreadsheet-58731.xlsx")
    if not os.path.exists(xl):
        xl = os.path.join(ROOT, "realformats-v1/documents/xlsx/poi-spreadsheet-TwoSheetsOneHidden.xlsx")
    if not os.path.exists(xl):
        die("xlsx sample missing under realformats-v1")
    with zipfile.ZipFile(xl) as z:
        names = z.namelist()
        ss = "xl/sharedStrings.xml"
        if ss not in names:
            die("xlsx sample has no sharedStrings.xml: %s" % xl)
        base_ss = z.read(ss).decode("utf-8", "replace")
    # Re-attribute EVERY shared string (a table-wide change), so the coarse
    # table-level semantic edge is accurate: every string cell's displayed
    # observation really does change, while the worksheet bytes are untouched.
    if "</t>" not in base_ss:
        die("xlsx sharedStrings.xml has no <t> element")
    x1 = os.path.join(out_dir, "xlsx-ss-r1.xlsx")
    with open(xl, "rb") as fi, open(x1, "wb") as fo:
        fo.write(fi.read())
    x2 = os.path.join(out_dir, "xlsx-ss-r2.xlsx")
    x3 = os.path.join(out_dir, "xlsx-ss-r3.xlsx")
    ss2 = _all_t(base_ss, "ZEBRA2:")
    ss3 = _all_t(base_ss, "ZEBRA3:")
    zip_replace_member(xl, x2, "xl/sharedStrings.xml", ss2.encode("utf-8"))
    zip_replace_member(xl, x3, "xl/sharedStrings.xml", ss3.encode("utf-8"))
    fams.append(("xlsx-ss", "xlsx", "shared-string", [x1, x2, x3]))

    # --- deterministic text families (faithful localized edits) ---------------
    csv_target = 4 * 1024 * 1024
    csv_revs = []
    for v in (1, 2, 3):
        p = os.path.join(out_dir, "csv-text-r%d.csv" % v)
        with open(p, "wb") as f:
            f.write(gen_csv(csv_target, v))
        csv_revs.append(p)
    fams.append(("csv-text", "csv", "text-edit", csv_revs))

    json_target = 4 * 1024 * 1024
    json_revs = []
    for v in (1, 2, 3):
        p = os.path.join(out_dir, "json-text-r%d.json" % v)
        with open(p, "wb") as f:
            f.write(gen_json(json_target, v))
        json_revs.append(p)
    fams.append(("json-text", "json", "text-edit", json_revs))

    print("family\tfmt\tchange\trev\tpath\tbytes\tsha256")
    for fam, fmt, change, revs in fams:
        for k, p in enumerate(revs, start=1):
            print("%s\t%s\t%s\t%d\t%s\t%d\t%s" % (
                fam, fmt, change, k, p, os.path.getsize(p), sha256_file(p)))


def _all_t(text, prefix):
    """Prefix every <t> string content; preserves <si> count and references."""
    return re.sub(r"(<t(?:[^>]*)>)([^<]*)(</t>)",
                  lambda m: m.group(1) + prefix + m.group(2) + m.group(3), text)



# ---------------------------------------------------------------------------
# structural observations (a "derived observation" population, per format)
# ---------------------------------------------------------------------------
def _zip_part_spans(path):
    """{member_name: (span_start, span_end, uncompressed_len)}."""
    with zipfile.ZipFile(path) as z:
        out = {}
        data = open(path, "rb").read()
        for info in z.infolist():
            off = info.header_offset
            nl, el = struct.unpack("<HH", data[off + 26:off + 30])
            end = off + 30 + nl + el + info.compress_size
            out[info.filename] = (off, end, info.file_size)
        return out


def json_array_elem_spans(data, key=b'"items"'):
    """Spans of the top-level elements of the first array following `key`."""
    i = data.find(key)
    if i < 0:
        return []
    j = data.find(b"[", i)
    if j < 0:
        return []
    i = j + 1
    n = len(data)
    spans = []
    depth = 0
    start = None
    in_str = False
    esc = False
    while i < n:
        c = data[i]
        if in_str:
            if esc:
                esc = False
            elif c == 0x5C:
                esc = True
            elif c == 0x22:
                in_str = False
        else:
            if c == 0x22:
                in_str = True
            elif c in (0x5B, 0x7B):
                if depth == 0:
                    start = i
                depth += 1
            elif c in (0x5D, 0x7D):
                if depth == 0:
                    break  # the array's own closing bracket
                depth -= 1
                if depth == 0 and start is not None:
                    spans.append((start, i + 1))
                    start = None
        i += 1
    return spans


def observations_for(fmt, path, frag):
    """[(kind, physical_ranges, semantic_kinds, id_hash)] . Ranges are in the
    *source* (compressed) coordinate space; semantic kinds are context member
    names the observation depends on but does not physically live in; id_hash is
    the observation's own content id (its node identity, so two observations
    that share a fragment but differ still dedup as distinct nodes)."""
    obs = []
    if fmt == "csv":
        schemes, _ = frag.native_schemes("csv", path)
        data = open(path, "rb").read()
        for a, b in schemes[0][1]:
            obs.append(("rec", [(a, b)], [], cid(data[a:b])))
    elif fmt == "json":
        data = open(path, "rb").read()
        spans = json_array_elem_spans(data)
        if not spans:
            schemes, _ = frag.native_schemes("json", path)
            spans = schemes[0][1]
        for a, b in spans:
            obs.append(("elem", [(a, b)], [], cid(data[a:b])))
    elif fmt == "docx":
        parts = _zip_part_spans(path)
        with zipfile.ZipFile(path) as z:
            doc = z.read("word/document.xml")
        paras = re.findall(rb"<w:p(?: [^>]*)?>.*?</w:p>", doc, re.S)
        if not paras:
            paras = [doc]
        dspan = [(parts["word/document.xml"][0], parts["word/document.xml"][1])]
        sem = [m for m in ("word/styles.xml", "word/numbering.xml") if m in parts]
        for p in paras:
            obs.append(("para", dspan, list(sem), cid(p)))
    elif fmt == "xlsx":
        parts = _zip_part_spans(path)
        sheet = next((nm for nm in sorted(parts)
                      if nm.startswith("xl/worksheets/sheet") and nm.endswith(".xml")), None)
        if sheet is None:
            die("xlsx has no worksheet part")
        with zipfile.ZipFile(path) as z:
            sxb = z.read(sheet)
        cells = re.findall(rb"<c(?: [^>]*)?/>|<c(?: [^>]*)?>.*?</c>", sxb, re.S)
        if not cells:
            cells = [sxb]
        sspan = [(parts[sheet][0], parts[sheet][1])]
        sem_ss = "xl/sharedStrings.xml" if "xl/sharedStrings.xml" in parts else None
        for c in cells:
            kinds = [sem_ss] if (sem_ss and b't="s"' in c) else []
            obs.append(("cell", sspan, kinds, cid(c)))
    else:
        die("no observation model for %s" % fmt)
    return obs


# ---------------------------------------------------------------------------
# digest: fragmentation + dependency graph + three lanes
# ---------------------------------------------------------------------------
def obs_output_bytes(fmt, path, obs):
    """Per-observation derived-output byte estimate. For record/element text
    this is the observation's own span; for a container part it is the part's
    uncompressed size divided across its observations."""
    if fmt in ("csv", "json"):
        return [sum(b - a for a, b in o[1]) for o in obs]
    if fmt == "docx":
        with zipfile.ZipFile(path) as z:
            tot = len(z.read("word/document.xml"))
    elif fmt == "xlsx":
        parts = _zip_part_spans(path)
        sheet = next((nm for nm in sorted(parts)
                      if nm.startswith("xl/worksheets/sheet") and nm.endswith(".xml")), None)
        if sheet is None:
            return [0] * len(obs)
        with zipfile.ZipFile(path) as z:
            tot = len(z.read(sheet))
    else:
        return [0] * len(obs)
    k = max(1, len(obs))
    return [tot // k] * len(obs)


def _load_measured(measured_dir, fam, rev):
    p = os.path.join(measured_dir, "%s.r%d.json" % (fam, rev))
    if not os.path.exists(p):
        return {}
    try:
        return json.load(open(p))
    except Exception:
        return {}


def digest(families_path, measured_dir, out_path):
    frag = load_mod("p286", os.path.join(HERE, "phase28-6-fragments.py"))
    rows = []
    with open(families_path) as f:
        for line in f:
            c = line.rstrip("\n").split("\t")
            if len(c) >= 7 and c[0] != "family":
                rows.append(c)
    fams = {}
    order = []
    for fam, fmt, change, rev, path, nbytes, sha in rows:
        if fam not in fams:
            fams[fam] = {"family": fam, "fmt": fmt, "change": change, "revs": []}
            order.append(fam)
        fams[fam]["revs"].append({"rev": int(rev), "path": path,
                                  "bytes": int(nbytes), "sha256": sha})

    result = []
    for fam in order:
        F = fams[fam]
        fmt = F["fmt"]
        revs = sorted(F["revs"], key=lambda r: r["rev"])
        # per-revision: raw bytes, chunks, observations, semantic context spans
        for r in revs:
            data = open(r["path"], "rb").read()
            r["data"] = data
            r["chs"] = fragment_list(r["path"], fmt, data)
            r["obs"] = observations_for(fmt, r["path"], frag)
            r["obs_out"] = obs_output_bytes(fmt, r["path"], r["obs"])
            # semantic context member spans -> signature over their chunks
            ctx = {}
            for o in r["obs"]:
                for k in o[2]:
                    ctx[k] = None
            r["ctx_parts"] = ctx
            r["ctx_sig"] = {}
            if ctx:
                parts = _zip_part_spans(r["path"])
                for k in ctx:
                    if k in parts:
                        r["ctx_sig"][k] = cid(data[parts[k][0]:parts[k][1]])
            # per-observation dependency signature (physical + semantic)
            sigs = []
            for oid, ranges, kinds, _idh in r["obs"]:
                phys = span_sig(r["chs"], ranges)
                sem = tuple(sorted((k, r["ctx_sig"].get(k, "")) for k in kinds))
                sigs.append((phys, sem))
            r["obs_sig"] = sigs
            m = _load_measured(measured_dir, fam, r["rev"])
            r["measured"] = m

        revs = [dict(r) for r in revs]
        for r in revs:
            del r["data"]

        # unique chunks across the family
        seen = {}
        for r in revs:
            for s, e, h in r["chs"]:
                seen.setdefault(h, e - s)
        unique_chunk_bytes = sum(seen.values())
        unique_chunks = len(seen)

        # per-revision chunk novelty
        known = set()
        for r in revs:
            new = [h for _, _, h in r["chs"] if h not in known]
            r["new_chunk_bytes"] = sum(dict((h, e - s) for s, e, h in r["chs"])[h] for h in new)
            for h in new:
                known.add(h)

        pairs = []
        obs_reused_total = 0
        obs_sem_only_total = 0
        for i in range(1, len(revs)):
            a, b = revs[i - 1], revs[i]
            b_phys = [sig[0] for sig in b["obs_sig"]]
            b_sem = [sig[1] for sig in b["obs_sig"]]
            a_phys = set(sig[0] for sig in a["obs_sig"])
            a_sem = set(sig[1] for sig in a["obs_sig"])
            reusable = 0
            sem_only = 0
            for k in range(len(a["obs_sig"])):
                if k >= len(b["obs_sig"]):
                    continue
                p_same = b_phys[k] in a_phys
                s_same = b_sem[k] in a_sem
                if p_same and s_same:
                    reusable += 1
                elif s_same and not p_same:
                    pass
                elif p_same and not s_same:
                    sem_only += 1
            n_obs = max(len(a["obs_sig"]), len(b["obs_sig"]))
            pairs.append({"from": a["rev"], "to": b["rev"],
                          "obs_total": n_obs, "reusable": reusable,
                          "invalid": n_obs - reusable,
                          "semantic_only_invalid": sem_only})
            obs_reused_total += reusable
            obs_sem_only_total += sem_only

        # --- lane counters (cumulative over the family, base = rev 1) --------
        # Content-addressed observation nodes: an observation whose dependency
        # signature (physical + semantic) repeats is ONE node. Its derived
        # output is a pure function of that signature, so identical signatures
        # share one stored output.
        n = len(revs)
        src_bytes = [r["bytes"] for r in revs]
        obs_counts = [len(r["obs"]) for r in revs]
        uniq_node = {}          # id_hash -> [out_bytes, n_deps]
        for r in revs:
            for k, (oid, ranges, kinds, idh) in enumerate(r["obs"]):
                if idh in uniq_node:
                    continue
                ndeps = len(kinds)
                for a, b in ranges:
                    ndeps += len(chunks_covering(r["chs"], a, b))
                if fmt in ("csv", "json"):
                    out_b = sum(b - a for a, b in ranges)
                else:
                    out_b = r["obs_out"][k]
                uniq_node[idh] = [out_b, ndeps]
        n_unique_obs = len(uniq_node)
        unique_obs_out_bytes = sum(v[0] for v in uniq_node.values())
        n_unique_edges = sum(v[1] for v in uniq_node.values())
        ctx_entries = sum(len(r["ctx_sig"]) for r in revs)

        # L1 full rebuild: everything rewritten each revision.
        build_us_total = sum((r["measured"].get("build_us") or 0) for r in revs)
        all_frag_writes = sum(len(r["chs"]) for r in revs)
        l1_work = sum(obs_counts) + all_frag_writes
        l1 = {
            "raw_bytes_written": sum(src_bytes),
            "raw_persistent_bytes": sum(src_bytes),
            "encoded_store_bytes": sum((r["measured"].get("store_bytes") or 0) for r in revs),
            "fragment_writes": all_frag_writes,
            "obs_execs": sum(obs_counts),
            "obs_total_available": sum(obs_counts),
            "work_units": l1_work,
            "latency_us_measured": build_us_total,
            "latency_us_modeled": build_us_total,
        }
        # L2 versioned CAS: physical dedup by content id; the observation
        # output store is deduped by content too, but with NO dependency edges
        # it cannot know an output is unchanged, so it must recompute every
        # observation each revision.
        chunk_index = unique_chunks * CHUNK_ENTRY_BYTES
        obs_node_index = n_unique_obs * OBS_NODE_BYTES
        l2_index = chunk_index
        l2 = {
            "raw_bytes_written": sum(r["new_chunk_bytes"] for r in revs),
            "persistent_bytes": unique_chunk_bytes + l2_index,
            "fragment_store_bytes": unique_chunk_bytes,
            "obs_store_bytes": 0,
            "index_bytes": l2_index,
            "obs_execs": sum(obs_counts),
            "obs_total_available": sum(obs_counts),
            "n_unique_fragments": unique_chunks,
            "n_unique_obs": n_unique_obs,
            "work_units": sum(obs_counts) + unique_chunks,
            "latency_us_modeled": int(build_us_total * (sum(obs_counts) + unique_chunks) / l1_work) if l1_work else 0,
        }
        # L3 dependency graph: L2 + dependency edges (physical + semantic) ->
        # an observation whose FULL closure is unchanged is REUSED (no execution).
        base_obs = obs_counts[0] if obs_counts else 0
        l3_recomputed = base_obs + sum(p["invalid"] for p in pairs)
        l3_reused = sum(p["reusable"] for p in pairs)
        edge_index = n_unique_edges * EDGE_BYTES
        ctx_index = ctx_entries * CTX_ENTRY_BYTES
        obs_identity_overhead = n_unique_obs * OBS_NODE_BYTES
        # L3 = L2 + the dependency graph itself. The MINIMAL graph index is one
        # fragment-directory entry per fragment plus one dependency-signature
        # edge per (observation, fragment) plus the semantic context entries.
        # Persisting a per-observation identity NODE is an ADDITIONAL overhead,
        # reported separately (obs_identity_overhead_bytes).
        l3_index = l2_index + edge_index + ctx_index
        l3 = {
            "raw_bytes_written": sum(r["new_chunk_bytes"] for r in revs),
            "persistent_bytes": unique_chunk_bytes + l3_index,
            "fragment_store_bytes": unique_chunk_bytes,
            "obs_store_bytes": 0,
            "index_bytes": l3_index,
            "obs_identity_overhead_bytes": obs_identity_overhead,
            "index_with_identity_bytes": l3_index + obs_identity_overhead,
            "obs_execs": l3_recomputed,
            "obs_reused": l3_reused,
            "obs_total_available": sum(obs_counts),
            "n_unique_fragments": unique_chunks,
            "n_unique_obs": n_unique_obs,
            "work_units": l3_recomputed + unique_chunks,
            "latency_us_modeled": int(build_us_total * (l3_recomputed + unique_chunks) / l1_work) if l1_work else 0,
            "index_components": {
                "chunk_entries": chunk_index,
                "dependency_edges": edge_index,
                "semantic_ctx_entries": ctx_index,
                "obs_identity_overhead": obs_identity_overhead,
                "n_unique_edges": n_unique_edges,
                "n_unique_obs": n_unique_obs,
                "edge_delta_over_cas": edge_index + ctx_index,
            },
        }
        result.append({
            "family": fam, "fmt": fmt, "change": F["change"], "n_revs": n,
            "revisions": [{"rev": r["rev"], "bytes": r["bytes"], "sha256": r["sha256"],
                           "chunks": len(r["chs"]), "new_chunk_bytes": r["new_chunk_bytes"],
                           "n_obs": len(r["obs"]),
                           "measured": r["measured"]} for r in revs],
            "unique_chunks": unique_chunks,
            "unique_chunk_bytes": unique_chunk_bytes,
            "pairs": pairs,
            "lanes": {"full": l1, "cas": l2, "graph": l3},
        })

    with open(out_path, "w") as f:
        json.dump({"families": result}, f, indent=1, sort_keys=True)
    print("digest: %d families -> %s" % (len(result), out_path))


# ---------------------------------------------------------------------------
# aggregate: SUMMARY.md + receipt.json
# ---------------------------------------------------------------------------
def _fmt(n):
    return "{:,}".format(int(n))


def _summ_row(fam):
    L = fam["lanes"]
    return (fam["family"], fam["fmt"], fam["change"],
            L["full"], L["cas"], L["graph"])


def aggregate(raw_dir, out_dir, campaign, short):
    os.makedirs(out_dir, exist_ok=True)
    d = json.load(open(os.path.join(raw_dir, "digest.json")))
    fams = d["families"]

    exact = json.load(open(os.path.join(raw_dir, "exactness.json"))) \
        if os.path.exists(os.path.join(raw_dir, "exactness.json")) else {}
    meta = {}
    mp = os.path.join(raw_dir, "metadata.json")
    if os.path.exists(mp):
        meta = json.load(open(mp))

    out = []
    A = out.append
    A("# Phase 28.7 — Experiment 3: cross-revision dependency reuse (measurement only)")
    A("")
    A("Simulation. No wire format, no planner/decoder change, no production-storage edit.")
    A("")
    A("Question: on authentic revision families, how much *verified work* is")
    A("reusable under a fragment-level dependency graph, what index does it need,")
    A("and does it beat a full rebuild and a versioned content-addressed store?")
    A("")
    A("Keys: **L1 full** = rebuild each revision from scratch; **L2 cas** =")
    A("fragment-level content-addressed store (content-defined chunking, dedup by")
    A("hash, no semantic edges -> must recompute every observation); **L3 graph** =")
    A("L2 + semantic dependency edges -> reuse an observation whose full closure is")
    A("unchanged. Work is counted in verified node executions; latency is modeled.")
    A("")
    A("Identities are kept separate (ADR-0060): the exact revision SHA-256, the")
    A("physical fragment content id, the semantic context signature, and the derived")
    A("observation content id are distinct namespaces. Context (Phase 15.6): the")
    A("existing content-addressed descriptor store retained ~0 cross-revision work,")
    A("because 97.2% of persistent bytes is the whole exact descriptor (Phase 22.6)")
    A("and any change invalidates it wholesale -- fragmenting the authority is what")
    A("localizes a revision.")
    A("")
    A("## Per-family reuse (cumulative over the family)")
    A("")
    A("  family        fmt   change          L1 raw B   L2 raw B   L3 raw B  L1 obs  L2 obs  L3 obs  L3 reused")
    tot = {"full": {}, "cas": {}, "graph": {}}
    for fam in fams:
        r = fam["family"]
        full = fam["lanes"]["full"]
        cas = fam["lanes"]["cas"]
        g = fam["lanes"]["graph"]
        A("  %-13s %-5s %-14s %10s %10s %10s %7s %7s %7s %10s" % (
            r, fam["fmt"], fam["change"],
            _fmt(full["raw_bytes_written"]), _fmt(cas["raw_bytes_written"]),
            _fmt(g["raw_bytes_written"]), _fmt(full["obs_execs"]),
            _fmt(cas["obs_execs"]), _fmt(g["obs_execs"]), _fmt(g["obs_reused"])))
        A("      persistent B: L1(raw)=%s L1(encoded store, measured)=%s  L2=%s  L3=%s  (L3 index=%s)" % (
            _fmt(full["raw_persistent_bytes"]), _fmt(full["encoded_store_bytes"]),
            _fmt(cas["persistent_bytes"]), _fmt(g["persistent_bytes"]),
            _fmt(g["index_bytes"])))
        A("      latency us:  L1(measured)=%s  L2(modeled)=%s  L3(modeled)=%s" % (
            _fmt(full["latency_us_measured"]), _fmt(cas["latency_us_modeled"]),
            _fmt(g["latency_us_modeled"])))
        for k in ("full", "cas", "graph"):
            for kk, vv in fam["lanes"][k].items():
                if isinstance(vv, (int, float)):
                    tot[k][kk] = tot[k].get(kk, 0) + vv
    A("")
    A("## Totals across families")
    A("")
    A("  metric                         L1 full          L2 cas          L3 graph")
    A("  raw bytes written          %14s %15s %15s" % (
        _fmt(tot["full"]["raw_bytes_written"]), _fmt(tot["cas"]["raw_bytes_written"]),
        _fmt(tot["graph"]["raw_bytes_written"])))
    A("  persistent bytes           %14s %15s %15s" % (
        _fmt(tot["full"]["raw_persistent_bytes"]), _fmt(tot["cas"]["persistent_bytes"]),
        _fmt(tot["graph"]["persistent_bytes"])))
    A("  verified obs executions    %14s %15s %15s" % (
        _fmt(tot["full"]["obs_execs"]), _fmt(tot["cas"]["obs_execs"]),
        _fmt(tot["graph"]["obs_execs"])))
    A("  obs available              %14s %15s %15s" % (
        _fmt(tot["full"]["obs_total_available"]), _fmt(tot["cas"]["obs_total_available"]),
        _fmt(tot["graph"]["obs_total_available"])))
    A("  index bytes                %14s %15s %15s" % (
        "0", _fmt(tot["cas"]["index_bytes"]), _fmt(tot["graph"]["index_bytes"])))
    A("  latency us                 %14s %15s %15s" % (
        _fmt(tot["full"]["latency_us_measured"]), _fmt(tot["cas"]["latency_us_modeled"]),
        _fmt(tot["graph"]["latency_us_modeled"])))
    A("")
    A("## Exactness")
    A("")
    A("  materialize --exact == source (length + SHA-256 + cmp), all revisions: "
      "%s" % (("%s/%s" % (exact.get("ok", "?"), exact.get("n", "?"))) if exact else "see exactness.json"))
    A("")
    open(os.path.join(out_dir, "SUMMARY.md"), "w").write("\n".join(out) + "\n")

    receipt = {
        "phase": "28.7",
        "court": "revision-reuse",
        "kind": "measurement-only simulation",
        "short": short,
        "metadata": meta,
        "exactness": exact,
        "families": fams,
        "totals": tot,
    }
    with open(os.path.join(campaign, "receipt.json"), "w") as f:
        json.dump(receipt, f, indent=1, sort_keys=True)
    with open(os.path.join(raw_dir, "aggregate_summary.json"), "w") as f:
        json.dump(receipt, f, indent=1, sort_keys=True)
    return receipt


# ---------------------------------------------------------------------------
# decide: a plain, quantified decision
# ---------------------------------------------------------------------------
def decide(receipt_path):
    r = json.load(open(receipt_path))
    fams = r["families"]
    tot = r["totals"]
    out = []
    L = out.append
    L("PHASE 28.7 DECISION — cross-revision dependency reuse (measurement only)")
    L("")
    L("Lanes: L1 full rebuild; L2 versioned content-addressed store (physical CDC,")
    L("no semantic edges); L3 fragment-level dependency graph (semantic edges).")
    L("")
    # per-family retained verified work (L3 vs L1) and index overhead
    for fam in fams:
        g = fam["lanes"]["graph"]
        cas = fam["lanes"]["cas"]
        avail = g["obs_total_available"]
        reused = g.get("obs_reused", 0)
        recomputed = g["obs_execs"]
        L("  %-13s %-5s %-14s obs %s: graph recomputes %s, reuses %s (%.1f%%);" % (
            fam["family"], fam["fmt"], fam["change"], _fmt(avail),
            _fmt(recomputed), _fmt(reused),
            (100.0 * reused / avail) if avail else 0.0))
        L("        L3 index %s B vs cas store %s B (graph-only edge+semantic delta %s B);" % (
            _fmt(g["index_bytes"]), _fmt(cas["persistent_bytes"]),
            _fmt(g["index_components"]["edge_delta_over_cas"])))
        sem_only = sum(p["semantic_only_invalid"] for p in fam["pairs"])
        L("        semantic-only invalidations (physical bytes unchanged, observation still invalid): %s" % _fmt(sem_only))
    L("")
    # headline ratios
    l1o = tot["full"]["obs_execs"]
    l2o = tot["cas"]["obs_execs"]
    l3o = tot["graph"]["obs_execs"]
    l3re = tot["graph"].get("obs_reused", 0)
    l1b = tot["full"]["raw_persistent_bytes"]
    l2b = tot["cas"]["persistent_bytes"]
    l3b = tot["graph"]["persistent_bytes"]
    indexb = tot["graph"]["index_bytes"]
    index_with_identity = tot["graph"].get("index_with_identity_bytes", indexb)
    chunkb = tot["graph"].get("fragment_store_bytes", 0)
    L("TOTALS")
    L("  verified obs executions  L1=%s  L2=%s  L3=%s  (L3 reuses %s)" % (
        _fmt(l1o), _fmt(l2o), _fmt(l3o), _fmt(l3re)))
    L("  persistent bytes         L1raw=%s  L2=%s  L3=%s  (L3 index=%s)" % (
        _fmt(l1b), _fmt(l2b), _fmt(l3b), _fmt(indexb)))
    L("")
    # The graph's advantage over CAS is compute (obs executions); its cost is
    # the index. The graph's advantage over L1 is both bytes and compute.
    work_saved_vs_cas = l2o - l3o
    bytes_saved_vs_l1 = l1b - l3b
    index_share = (float(indexb) / l1b) if l1b else 0.0
    base_total = sum(f["revisions"][0]["n_obs"] for f in fams if f["revisions"])
    subsequent = l1o - base_total
    retained_frac = (float(l3re) / subsequent) if subsequent else 0.0
    edge_delta = sum(f["lanes"]["graph"]["index_components"]["edge_delta_over_cas"] for f in fams)
    # The MINIMAL graph index is the dependency signatures (edges + ctx). If the
    # design also persists a per-observation identity node, the index grows by
    # obs_identity_overhead. Report both; the verdict is read from the minimal one
    # and the fragility is stated, never hidden.
    edges_only_index = sum(
        f["lanes"]["graph"]["index_components"]["chunk_entries"]
        + f["lanes"]["graph"]["index_components"]["dependency_edges"]
        + f["lanes"]["graph"]["index_components"]["semantic_ctx_entries"]
        for f in fams)
    sem_only_total = sum(sum(p["semantic_only_invalid"] for p in f["pairs"]) for f in fams)
    L("  graph vs CAS work saved: %s of %s obs executions (%.1f%% of the full-rebuild work)" % (
        _fmt(work_saved_vs_cas), _fmt(l1o), 100.0 * (work_saved_vs_cas / l1o if l1o else 0.0)))
    L("  next-revision observations reused: %s of %s (%.1f%%)" % (
        _fmt(l3re), _fmt(subsequent), 100.0 * retained_frac))
    L("  graph persistent bytes vs L1 raw: saved %s (L3 index is %.2f%% of L1 raw)" % (
        _fmt(bytes_saved_vs_l1), 100.0 * index_share))
    L("  graph-only index delta over CAS (chunk + edge + semantic entries): %s B" % _fmt(edge_delta))
    L("  index model: minimal dependency-signature index = %s B (%.2f%% of L1 raw);" % (
        _fmt(indexb), 100.0 * index_share))
    L("    if the design ALSO persists a %d B per-observation identity node it is %s B (%.2f%%)." % (
        OBS_NODE_BYTES, _fmt(index_with_identity),
        100.0 * index_with_identity / l1b if l1b else 0.0))
    L("  semantic-only invalidations (physical bytes unchanged, observation still invalid): %s" % _fmt(sem_only_total))
    L("")
    L("DECISION")
    # Falsifiable criterion, stated before the verdict (ADR-0054 discipline):
    #   POSITIVE iff the graph retains >=20% of next-revision observations,
    #   its whole index is <=10% of the L1 raw bytes, and the graph-only index
    #   delta is smaller than the bytes the physical CAS already saves vs L1.
    cas_saved = l1b - l2b
    positive = (retained_frac >= 0.20 and index_share <= 0.10 and edge_delta < cas_saved)
    L("  criterion: retained>=20%% [%s]  index<=10%% of L1 [%s]  graph delta < CAS byte saving [%s]" % (
        "yes" if retained_frac >= 0.20 else "no",
        "yes" if index_share <= 0.10 else "no",
        "yes" if edge_delta < cas_saved else "no"))
    if positive:
        L("  RESOLVED (positive, scoped, marginal): the minimal dependency-signature")
        L("  index does NOT consume the byte advantage. A fragment-level physical")
        L("  content-addressed store already captures the byte reuse (%s B saved vs a" % _fmt(cas_saved))
        L("  full rebuild, %.1f%% of L1 raw), and the graph adds only %s B of index" % (
            100.0 * (cas_saved / l1b if l1b else 0.0), _fmt(edge_delta)))
        L("  (%.1f%% of L1 raw) while reusing %.1f%% of next-revision verified" % (
            100.0 * index_share, 100.0 * retained_frac))
        L("  observations. The semantic edges are load-bearing: %s invalidations have" % _fmt(sem_only_total))
        L("  UNCHANGED physical bytes and would be reused WRONG by a physical-only store.")
        L("  FRAGILITY (stated, not hidden): the verdict is marginal on the 10%")
        L("  index criterion (%.1f%%). If the design also persists a %d B per-observation" % (
            100.0 * index_share, OBS_NODE_BYTES))
        L("  identity node the index becomes %s B (%.1f%%) and the verdict flips to" % (
            _fmt(index_with_identity), 100.0 * index_with_identity / l1b if l1b else 0.0))
        L("  NEGATIVE. SCOPE: the work win lives in LOCALIZED edits; on a whole-document")
        L("  redraft it is 0 by construction (and the byte baseline there is also ~0).")
    else:
        L("  NEGATIVE (the index is the binding cost): the dependency graph's minimal")
        L("  dependency-signature index is %s B (%.1f%% of the L1 raw bytes) to save" % (
            _fmt(indexb), 100.0 * index_share))
        L("  %.1f%% of next-revision observation executions; with per-observation" %
          (100.0 * retained_frac))
        L("  identity nodes it is %s B (%.1f%%). It is Theta(#observations) -- one" % (
            _fmt(index_with_identity), 100.0 * index_with_identity / l1b if l1b else 0.0))
        L("  node plus its dependency edges per observation -- so on record/element-")
        L("  dense formats the index rivals the source itself.")
        L("  ")
        L("  WHAT PAYS: the fragment-level PHYSICAL content-addressed store (L2). It")
        L("  keeps %s B vs a full rebuild (%.1f%% of L1 raw) with an index of only" % (
            _fmt(cas_saved), 100.0 * (cas_saved / l1b if l1b else 0.0)))
        L("  %s B. The byte win of revision reuse is a content-addressing win, not a" %
          _fmt(tot["cas"]["index_bytes"]))
        L("  dependency-graph win.")
        L("  ")
        L("  WHAT THE GRAPH AND ITS SEMANTIC EDGES STILL BUY: correctness of reuse.")
        L("  %s invalidations have UNCHANGED physical bytes (style-table / shared-" % _fmt(sem_only_total))
        L("  string changes) and would be reused WRONG by a physical-only store. A")
        L("  graph is REQUIRED if any observation is reused at all; it is simply not")
        L("  the mechanism that saves the bytes.")
        _by_idx = sorted(fams, key=lambda f: f["lanes"]["graph"]["index_bytes"])
        if _by_idx:
            lo, hi = _by_idx[0], _by_idx[-1]
            L("  its index is only affordable where observations are COARSE:")
            L("  %s (%s) index %s B vs %s (%s) index %s B." % (
                lo["family"], lo["fmt"], _fmt(lo["lanes"]["graph"]["index_bytes"]),
                hi["family"], hi["fmt"], _fmt(hi["lanes"]["graph"]["index_bytes"])))
    print("\n".join(out))


def main(argv):
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("families"); p.add_argument("--out", required=True)
    p = sub.add_parser("digest")
    p.add_argument("--families", required=True)
    p.add_argument("--measured", required=True)
    p.add_argument("--out", required=True)
    p = sub.add_parser("aggregate")
    p.add_argument("--raw", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--campaign", required=True)
    p.add_argument("--short", required=True)
    p = sub.add_parser("decide"); p.add_argument("--receipt", required=True)
    a = ap.parse_args(argv[1:])
    if a.cmd == "families":
        build_families(a.out); return 0
    if a.cmd == "digest":
        digest(a.families, a.measured, a.out); return 0
    if a.cmd == "aggregate":
        aggregate(a.raw, a.out, a.campaign, a.short); return 0
    if a.cmd == "decide":
        decide(a.receipt); return 0
    die("unknown command")


if __name__ == "__main__":
    sys.exit(main(sys.argv))
