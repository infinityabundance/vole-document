#!/usr/bin/env python3
# Phase 29.2 — coarse structural index court fixtures.
#
# Deterministic large fixtures (PDF / DOCX / ODT / Parquet) plus the two helpers
# the court needs: recover the exact source ranges a narrow request's coarse
# units cover (for the page-level SQLite control), and aggregate the raw tables
# into a sealed receipt.
#
#   python3 tools/fixtures/phase29-2-coarse.py corpus       --out DIR
#   python3 tools/fixtures/phase29-2-coarse.py unit-ranges  --idx I --map M --request LINE
#   python3 tools/fixtures/phase29-2-coarse.py aggregate    --raw R --out O --camp C --short SHA
#
# Nothing here is a wire format or a planner: it only *measures* what the
# persisted coarse index reads versus the whole-descriptor closure.

import hashlib
import importlib.util
import json
import os
import struct
import sys
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))


def die(msg):
    sys.stderr.write("phase29-2-coarse: %s\n" % msg)
    sys.exit(2)


def sha256_bytes(b):
    return hashlib.sha256(b).hexdigest()


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


def lcg_bytes(n, seed=0x9E3779B97F4A7C15):
    """Deterministic pseudo-random bytes (no OS entropy, reproducible)."""
    out = bytearray(n)
    s = seed & 0xFFFFFFFFFFFFFFFF
    for i in range(n):
        s = (s * 6364136223846793005 + 1442695040888963407) & 0xFFFFFFFFFFFFFFFF
        out[i] = (s >> 33) & 0xFF
    return bytes(out)


# ---------------------------------------------------------------------------
# corpus
# ---------------------------------------------------------------------------
def gen_pdf(target_bytes):
    """A valid classic PDF with many pages, each with a multi-KiB content
    stream. A page-1 text request needs one page's objects."""
    # ~7 KiB per page.
    per_page = 7000
    pages = max(2, target_bytes // (per_page + 60))
    out = bytearray(b"%PDF-1.5\n")
    offsets = {}

    def add(num, body):
        offsets[num] = len(out)
        out.extend(("%d 0 obj\n" % num).encode())
        out.extend(body)
        out.extend(b"\nendobj\n")

    font_id = 3 + 2 * pages
    add(1, b"<< /Type /Catalog /Pages 2 0 R >>")
    kids = " ".join("%d 0 R" % (3 + 2 * i) for i in range(pages))
    add(2, ("<< /Type /Pages /Kids [%s] /Count %d >>" % (kids, pages)).encode())
    filler = b"x" * (per_page - 60)
    for i in range(pages):
        pid, cid = 3 + 2 * i, 4 + 2 * i
        add(pid, (
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
            "/Resources << /Font << /F1 %d 0 R >> >> /Contents %d 0 R >>"
            % (font_id, cid)).encode())
        content = b"BT /F1 12 Tf 72 720 Td (Page %d of the coarse index court) Tj ET\n" % (i + 1)
        content += b"BT /F1 12 Tf 72 700 Td (" + filler + b") Tj ET\n"
        add(cid, ("<< /Length %d >>\nstream\n" % len(content)).encode() + content + b"\nendstream")
    add(font_id, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
    xref = len(out)
    out.extend(("xref\n0 %d\n" % (font_id + 1)).encode())
    out.extend(b"0000000000 65535 f \n")
    for n in range(1, font_id + 1):
        out.extend(("%010d 00000 n \n" % offsets[n]).encode())
    out.extend(("trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%EOF\n"
                % (font_id + 1, xref)).encode())
    return bytes(out)


DOCX_MAIN_CT = "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"


def gen_docx(doc_bytes, media_count=5, media_each=640 * 1024):
    """A valid OPC/DOCX: a small `document` part plus large media members the
    block-0 text request does not need."""
    paragraphs = max(1, doc_bytes // 120)
    body = []
    for i in range(paragraphs):
        body.append("<w:p><w:r><w:t xml:space=\"preserve\">Paragraph %d: the coarse structural index keeps the descriptor closure narrow.</w:t></w:r></w:p>" % i)
    document = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
        '<w:body>' + "".join(body) + '<w:sectPr/></w:body></w:document>'
    ).encode("utf-8")
    content_types = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Default Extension="png" ContentType="image/png"/>'
        '<Override PartName="/word/document.xml" ContentType="%s"/>'
        '</Types>' % DOCX_MAIN_CT
    ).encode("utf-8")
    rels = (
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>'
        '</Relationships>'
    ).encode("utf-8")
    media = media_each
    buf = _ZipBuilder()
    buf.add("[Content_Types].xml", content_types, compress=True)
    buf.add("_rels/.rels", rels, compress=True)
    buf.add("word/document.xml", document, compress=True)
    for i in range(media_count):
        buf.add("word/media/image%d.png" % i, lcg_bytes(media, seed=0x51ED + i), compress=False)
    return buf.bytes()


ODT_MIMETYPE = "application/vnd.oasis.opendocument.text"


def gen_odt(doc_bytes, media_count=5, media_each=640 * 1024):
    """A valid ODF/ODT: a small `content.xml` plus large Pictures members the
    block-0 text request does not need. `mimetype` is first and stored."""
    paragraphs = max(1, doc_bytes // 120)
    body = []
    for i in range(paragraphs):
        body.append("<text:p>Paragraph %d: the coarse structural index keeps the descriptor closure narrow.</text:p>" % i)
    content = (
        '<?xml version="1.0" encoding="UTF-8"?>'
        '<office:document-content '
        'xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" '
        'xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" '
        'xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" '
        'xmlns:xlink="http://www.w3.org/1999/xlink" '
        'office:version="1.2">'
        '<office:body><office:text>' + "".join(body) + '</office:text></office:body>'
        '</office:document-content>'
    ).encode("utf-8")
    manifest = (
        '<?xml version="1.0" encoding="UTF-8"?>'
        '<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.2">'
        '<manifest:file-entry manifest:full-path="/" manifest:media-type="%s"/>'
        '<manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>'
        % ODT_MIMETYPE
    )
    for i in range(media_count):
        manifest += '<manifest:file-entry manifest:full-path="Pictures/img%d.png" manifest:media-type="image/png"/>' % i
    manifest += '</manifest:manifest>'
    media = media_each
    buf = _ZipBuilder()
    buf.add("mimetype", ODT_MIMETYPE.encode("ascii"), compress=False)
    buf.add("META-INF/manifest.xml", manifest.encode("utf-8"), compress=True)
    buf.add("content.xml", content, compress=True)
    for i in range(media_count):
        buf.add("Pictures/img%d.png" % i, lcg_bytes(media, seed=0x0DD + i), compress=False)
    return buf.bytes()


def raw_deflate(data):
    """ZIP method 8 is a *raw* DEFLATE stream (no zlib header/adler trailer)."""
    co = zlib.compressobj(6, zlib.DEFLATED, -15)
    return co.compress(data) + co.flush()


class _ZipBuilder:
    """A minimal deterministic ZIP writer (one local header per entry, a central
    directory, and an EOCD; no data descriptors, no ZIP64)."""

    def __init__(self):
        self.out = bytearray()
        self.entries = []

    def add(self, name, data, compress):
        name_b = name.encode("utf-8")
        method = 8 if compress else 0
        payload = raw_deflate(data) if compress else data
        crc = zlib.crc32(data) & 0xFFFFFFFF
        local_off = len(self.out)
        self.out.extend(b"PK\x03\x04")
        self.out.extend(struct.pack("<HHHHHIIIHH", 20, 0x0800, method, 0, 0,
                                    crc, len(payload), len(data), len(name_b), 0))
        self.out.extend(name_b)
        self.out.extend(payload)
        self.entries.append((name_b, method, crc, len(payload), len(data), local_off))

    def bytes(self):
        cd_off = len(self.out)
        for (name_b, method, crc, comp, raw, local_off) in self.entries:
            self.out.extend(b"PK\x01\x02")
            self.out.extend(struct.pack("<HHHHHHIIIHHHHHII",
                                        20, 20, 0x0800, method, 0, 0, crc, comp, raw,
                                        len(name_b), 0, 0, 0, 0, 0, local_off))
            self.out.extend(name_b)
        cd_size = len(self.out) - cd_off
        self.out.extend(b"PK\x05\x06")
        self.out.extend(struct.pack("<HHHHIIH", 0, 0, len(self.entries), len(self.entries),
                                    cd_size, cd_off, 0))
        return bytes(self.out)


def cmd_corpus(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    rows = []
    pdf = os.path.join(out_dir, "large.pdf")
    with open(pdf, "wb") as f:
        f.write(gen_pdf(3 * 1024 * 1024))
    rows.append(("pdf", pdf))

    docx = os.path.join(out_dir, "large.docx")
    with open(docx, "wb") as f:
        f.write(gen_docx(256 * 1024))
    rows.append(("docx", docx))

    odt = os.path.join(out_dir, "large.odt")
    with open(odt, "wb") as f:
        f.write(gen_odt(256 * 1024))
    rows.append(("odt", odt))

    mkp = load_mod("mk_parquet", os.path.join(HERE, "make-parquet.py"))
    pq = os.path.join(out_dir, "large.parquet")
    d = mkp.large(4 * 1024 * 1024)
    mkp.write_parquet(pq, d["columns"], 50000)
    rows.append(("parquet", pq))

    print("fmt\tpath\tbytes\tsha256")
    for fmt, path in rows:
        print("%s\t%s\t%d\t%s" % (fmt, path, os.path.getsize(path), sha256_file(path)))


# ---------------------------------------------------------------------------
# index parsing: recover the source ranges a request's units cover
# ---------------------------------------------------------------------------
def parse_index(path):
    with open(path, "rb") as f:
        b = f.read()
    if b[:4] != b"VCI1":
        die("bad index magic")
    p = 4
    fmt = b[p]; p += 1
    p += 4  # object_count
    p += 4  # graph_ops
    source_len = struct.unpack_from("<Q", b, p)[0]; p += 8
    p += 8  # object_base
    p += 32  # sha
    count = struct.unpack_from("<I", b, p)[0]; p += 4
    units = []
    for _ in range(count):
        kind = b[p]; p += 1
        uid = struct.unpack_from("<I", b, p)[0]; p += 4
        off = struct.unpack_from("<Q", b, p)[0]; p += 8
        ln = struct.unpack_from("<I", b, p)[0]; p += 4
        units.append((kind, uid, off, ln))
    return fmt, source_len, units


def parse_map(path):
    out = {}
    with open(path) as f:
        for line in f:
            line = line.rstrip("\n")
            if "\t" not in line:
                continue
            k, v = line.split("\t", 1)
            out[k] = [int(x) for x in v.split(",") if x != ""]
    return out


def cmd_unit_ranges(idx, map_path, request):
    fmt, source_len, units = parse_index(idx)
    m = parse_map(map_path)
    ords = m.get(request)
    if ords is None:
        die("request %r not in the dependency map" % request)
    ranges = sorted((units[o][2], units[o][2] + units[o][3]) for o in ords if o < len(units))
    merged = []
    for a, b in ranges:
        if merged and a <= merged[-1][1]:
            merged[-1][1] = max(merged[-1][1], b)
        else:
            merged.append([a, b])
    if not merged:
        # The lane recorded no coarse unit (e.g. Parquet, whose model node reads
        # the whole exact source): the request's required region is the whole
        # source, so the page control is measured against that.
        merged = [[0, source_len]]
    json.dump(merged, sys.stdout)


# ---------------------------------------------------------------------------
# aggregate
# ---------------------------------------------------------------------------
def read_tsv(path):
    rows = []
    with open(path) as f:
        header = f.readline().rstrip("\n").split("\t")
        for line in f:
            line = line.rstrip("\n")
            if not line:
                continue
            rows.append(dict(zip(header, line.split("\t"))))
    return rows


def num(d, k):
    try:
        return int(d.get(k, "0"))
    except ValueError:
        return 0


# Phase 28.6 historical control: current VOLE closure (bytes_read) per format.
HISTORICAL_28_6 = {
    "pdf": {"vole_bytes_read": 36414410, "source_bytes": 36404840, "sqlite_pages": 135284,
            "floor_plus_dir": 83697, "sqlite_db": 40976384},
    "docx": {"vole_bytes_read": 2309293, "source_bytes": 2286200, "sqlite_pages": 28788,
             "floor_plus_dir": 17400, "sqlite_db": 2580480},
    "json": {"vole_bytes_read": 16778341, "source_bytes": 16777304, "sqlite_pages": 12404,
             "floor_plus_dir": 65584, "sqlite_db": 18886656},
    "csv": {"vole_bytes_read": 16778079, "source_bytes": 16777245, "sqlite_pages": 12404,
            "floor_plus_dir": 65584, "sqlite_db": 18882560},
    "parquet": {"vole_bytes_read": 16780999, "source_bytes": 16779953, "sqlite_pages": 450676,
                "floor_plus_dir": 401271, "sqlite_db": 18886656},
}


def cmd_aggregate(raw, out, camp, short):
    obs = read_tsv(os.path.join(raw, "observations.tsv"))
    lines = []
    lines.append("# Phase 29.2 — coarse structural index (real bytes read)\n")
    lines.append("The current lane reads the whole encoded descriptor for a narrow request.")
    lines.append("The coarse index partitions the verbatim authority object into format-native")
    lines.append("units and serves a request with real `pread` range reads. `closure` is the")
    lines.append("honest physical total (units + persisted index + manifest/index/seed reads); the")
    lines.append("control is `observe-batch` cold. Two request shapes are measured:\n")
    lines.append("- **span**: a `--byte-range` over the largest native coarse unit (the index's")
    lines.append("  native granularity).")
    lines.append("- **reading**: the natural reading request (`--page 1`/`--block 0`/")
    lines.append("  `--parquet-cell 0:0`, `--kind text`).\n")
    lines.append("| fmt | shape | source B | control B | units B | index B | closure B | page-SQLite B | closure/control | verdict vs control | closure/SQLite | units/SQLite | verdict vs SQLite | coarse wall µs | coarse RSS kB | matches | exact |")
    lines.append("|---|---|---:|---:|---:|---:|---:|---:|---:|:--:|---:|---:|:--:|---:|---:|:--:|:--:|")

    by_fmt = {}
    for r in obs:
        by_fmt.setdefault(r["fmt"], {})[r["shape"]] = r

    exact_ok = 0
    span_wins_control = 0
    span_wins_sqlite = 0
    span_wins_sqlite_units = 0
    reading_wins_control = 0
    measured_sqlite = 0
    for fmt in ["pdf", "docx", "odt", "parquet"]:
        q = by_fmt.get(fmt)
        if not q:
            continue
        rate = q.get("span") or q.get("reading")
        if num(rate, "exact_ok"):
            exact_ok += 1
        for shape in ["span", "reading"]:
            r = q.get(shape)
            if not r:
                continue
            src = num(r, "src_bytes")
            control = num(r, "control_bytes")
            unit_b = num(r, "coarse_unit_bytes")
            closure = num(r, "coarse_closure_bytes")
            dir_b = num(r, "coarse_index_bytes")
            sql = num(r, "sqlite_page_bytes")
            matches = num(r, "coarse_matches_full")
            ex = num(r, "exact_ok")
            wall = num(r, "coarse_wall_us")
            rss = num(r, "coarse_rss_kb")
            vs_c = ("%.2fx" % (control / closure)) if closure else "n/a"
            vs_s = ("%.2fx" % (sql / closure)) if (sql and closure) else "n/a"
            vs_s_u = ("%.2fx" % (sql / unit_b)) if (sql and unit_b) else "n/a"
            vc = "WIN" if (control > 0 and closure < control) else "LOSS"
            vs = ("WIN" if closure < sql else "LOSS") if sql else "n/m"
            if shape == "span":
                if control > 0 and closure < control:
                    span_wins_control += 1
                if sql > 0:
                    measured_sqlite += 1
                    if closure < sql:
                        span_wins_sqlite += 1
                    if unit_b < sql:
                        span_wins_sqlite_units += 1
            else:
                if control > 0 and closure < control:
                    reading_wins_control += 1
            lines.append("| %s | %s | %d | %d | %d | %d | %d | %s | %s | %s | %s | %s | %s | %d | %d | %s | %s |"
                         % (fmt, shape, src, control, unit_b, dir_b, closure,
                            sql if sql else "n/m", vs_c, vc, vs_s, vs_s_u, vs,
                            wall, rss, "yes" if matches else "NO", "yes" if ex else "NO"))

    lines.append("")
    lines.append("## Phase 28.6 historical control (different corpus; same mechanism)\n")
    lines.append("| fmt | 28.6 VOLE closure B | 28.6 source B | 28.6 page-SQLite B | 28.6 floor+dir B |")
    lines.append("|---|---:|---:|---:|---:|")
    for fmt, d in HISTORICAL_28_6.items():
        lines.append("| %s | %d | %d | %d | %d |" % (fmt, d["vole_bytes_read"], d["source_bytes"],
                                                       d["sqlite_pages"], d["floor_plus_dir"]))

    lines.append("")
    lines.append("## Verdict\n")
    lines.append("- exactness held: %d/%d formats `materialize --exact == source`."
                 % (exact_ok, len(by_fmt)))
    lines.append("- **span** requests: the coarse closure is narrower than the whole-descriptor")
    lines.append("  control in %d/%d formats, and beats the page-SQLite control in %d/%d"
                 % (span_wins_control, len(by_fmt), span_wins_sqlite, measured_sqlite))
    lines.append("  (on source bytes only, ignoring the persistent directory: %d/%d)."
                 % (span_wins_sqlite_units, measured_sqlite))
    lines.append("- **reading** requests: the coarse closure is narrower than the control in only")
    lines.append("  %d/%d formats — where the format model materializes the whole exact source"
                 % (reading_wins_control, len(by_fmt)))
    lines.append("  (DOCX/ODT/Parquet), the coarse units are built but not exercised.")
    lines.append("  Parquet's reading closure is 2.0x its control (the model materializes the")
    lines.append("  whole `DocumentExact` root twice), the largest loss.\n")
    lines.append("A format whose closure is not below the control is a *loss*, reported as such.")

    with open(os.path.join(camp, "SUMMARY.md"), "w") as f:
        f.write("\n".join(lines) + "\n")
    with open(os.path.join(out, "SUMMARY.md"), "w") as f:
        f.write("\n".join(lines) + "\n")

    receipt = {
        "phase": "29.2",
        "short": short,
        "n_docs": len(by_fmt),
        "exact_ok": exact_ok,
        "span_wins_over_control": span_wins_control,
        "reading_wins_over_control": reading_wins_control,
        "span_wins_over_sqlite_closure": span_wins_sqlite,
        "span_wins_over_sqlite_units_only": span_wins_sqlite_units,
        "sqlite_measured": measured_sqlite,
        "rows": obs,
        "historical_28_6": HISTORICAL_28_6,
    }
    with open(os.path.join(camp, "receipt.json"), "w") as f:
        json.dump(receipt, f, indent=2, sort_keys=True)
    print("aggregate: %d formats, exact %d/%d, span control wins %d/%d, span sqlite wins %d/%d, reading control wins %d/%d"
          % (len(by_fmt), exact_ok, len(by_fmt), span_wins_control, len(by_fmt),
             span_wins_sqlite, measured_sqlite, reading_wins_control, len(by_fmt)))


def main(argv):
    if len(argv) < 2:
        die("usage: corpus|unit-ranges|aggregate")
    cmd = argv[1]
    vals = {}
    i = 2
    while i < len(argv) - 1:
        if argv[i].startswith("--"):
            vals[argv[i]] = argv[i + 1]
            i += 2
        else:
            i += 1
    if cmd == "corpus":
        cmd_corpus(vals["--out"])
    elif cmd == "unit-ranges":
        cmd_unit_ranges(vals["--idx"], vals["--map"], vals["--request"])
    elif cmd == "aggregate":
        cmd_aggregate(vals["--raw"], vals["--out"], vals["--camp"], vals["--short"])
    else:
        die("unknown command %s" % cmd)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
