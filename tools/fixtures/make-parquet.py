#!/usr/bin/env python3
# Phase 21.14 — deterministic, stdlib-only Parquet fixture generator.
#
# The `analytical` container has DuckDB but **not** pyarrow, so this generator
# constructs valid Apache Parquet files by hand: a small Thrift-Compact writer for
# the footer/page headers, and PLAIN / RLE_DICTIONARY page payloads for the common
# physical types. Every file it writes is a real Parquet file that DuckDB can read
# (the court cross-checks VOLE against DuckDB on the same bytes), so the generator
# doubles as an independent correctness witness for the VOLE reader.
#
# Encodings written: PLAIN (0) and RLE_DICTIONARY (8), with RLE (3) definition
# levels. Codecs written: UNCOMPRESSED (0) and GZIP (2). Two fixtures deliberately
# **declare** an unsupported codec (ZSTD) or encoding (DELTA_BINARY_PACKED) so the
# adapter must decline them typed; one is a decompression-size bomb. Controls that
# must stay Opaque (prose, a truncated file, an inconsistent footer length) are also
# emitted.
#
#   python3 tools/fixtures/make-parquet.py                 # write tools/fixtures/parquet/
#   python3 tools/fixtures/make-parquet.py --corpus DIR    # write DIR/, print TSV
#
# The TSV is `name<TAB>bytes<TAB>sha256` per fixture, matching the other generators.

import gzip
import hashlib
import os
import struct
import sys

# --- physical type tags ------------------------------------------------------
T_BOOLEAN = 0
T_INT32 = 1
T_INT64 = 2
T_INT96 = 3
T_FLOAT = 4
T_DOUBLE = 5
T_BYTE_ARRAY = 6
T_FLBA = 7

REP_REQUIRED = 0
REP_OPTIONAL = 1

ENC_PLAIN = 0
ENC_RLE = 3
ENC_DELTA_BINARY_PACKED = 5
ENC_RLE_DICTIONARY = 8

CODEC_UNCOMPRESSED = 0
CODEC_GZIP = 2
CODEC_ZSTD = 6


# --- Thrift-Compact writer ---------------------------------------------------
class TS:
    """A minimal Thrift-Compact struct writer."""

    def __init__(self):
        self.b = bytearray()
        self.last = 0
        self._done = False

    def _varint(self, n):
        while True:
            x = n & 0x7F
            n >>= 7
            if n:
                self.b.append(x | 0x80)
            else:
                self.b.append(x)
                return

    def _zig(self, n):
        zz = ((n << 1) ^ (n >> 63)) & 0xFFFFFFFFFFFFFFFF
        self._varint(zz)

    def _fh(self, fid, t):
        d = fid - self.last
        if 1 <= d <= 15:
            self.b.append((d << 4) | t)
        else:
            self.b.append(t)
            self._zig(fid)
        self.last = fid

    def i32(self, fid, v):
        self._fh(fid, 5)
        self._zig(v)

    def i64(self, fid, v):
        self._fh(fid, 6)
        self._zig(v)

    def boolean(self, fid, v):
        self._fh(fid, 1 if v else 2)

    def binary(self, fid, data):
        self._fh(fid, 8)
        self._varint(len(data))
        self.b.extend(data)

    def string(self, fid, s):
        self.binary(fid, s.encode("utf-8"))

    def struct(self, fid, sub):
        self._fh(fid, 12)
        self.b.extend(sub.finish())

    def _lh(self, t, size):
        if size < 15:
            self.b.append((size << 4) | t)
        else:
            self.b.append(0xF0 | t)
            self._varint(size)

    def list_i32(self, fid, vals):
        self._fh(fid, 9)
        self._lh(5, len(vals))
        for v in vals:
            self._zig(v)

    def list_string(self, fid, vals):
        self._fh(fid, 9)
        self._lh(8, len(vals))
        for s in vals:
            data = s.encode("utf-8")
            self._varint(len(data))
            self.b.extend(data)

    def list_struct(self, fid, subs):
        self._fh(fid, 9)
        self._lh(12, len(subs))
        for s in subs:
            self.b.extend(s.finish())

    def finish(self):
        assert not self._done, "a Thrift struct was finished twice"
        self._done = True
        self.b.append(0)
        return bytes(self.b)


# --- page payload encoders ---------------------------------------------------
def plain_values(physical, values, type_length=None):
    out = bytearray()
    if physical == T_BOOLEAN:
        for i in range(0, len(values), 8):
            byte = 0
            for j, v in enumerate(values[i:i + 8]):
                if v:
                    byte |= 1 << j
            out.append(byte)
    elif physical == T_INT32:
        for v in values:
            out += struct.pack("<i", v)
    elif physical == T_INT64:
        for v in values:
            out += struct.pack("<q", v)
    elif physical == T_FLOAT:
        for v in values:
            out += struct.pack("<f", v)
    elif physical == T_DOUBLE:
        for v in values:
            out += struct.pack("<d", v)
    elif physical == T_BYTE_ARRAY:
        for v in values:
            out += struct.pack("<I", len(v))
            out += v
    elif physical == T_FLBA:
        for v in values:
            assert len(v) == type_length, "FLBA value width mismatch"
            out += v
    else:
        raise ValueError("unsupported physical type %r" % physical)
    return bytes(out)


def _put_varint(out, n):
    while True:
        x = n & 0x7F
        n >>= 7
        if n:
            out.append(x | 0x80)
        else:
            out.append(x)
            return


def rle_runs(values, width):
    """Encode levels/indices as a sequence of RLE runs (no bit-packing)."""
    out = bytearray()
    bytew = (width + 7) // 8
    i = 0
    while i < len(values):
        j = i
        while j < len(values) and values[j] == values[i]:
            j += 1
        _put_varint(out, (j - i) << 1)
        if bytew:
            out += int(values[i]).to_bytes(bytew, "little")
        i = j
    return bytes(out)


def bitpack_lsb(values, width):
    """Bit-pack values least-significant-bit first (Parquet's convention)."""
    if width == 0:
        return b""
    out = bytearray((len(values) * width + 7) // 8)
    bitpos = 0
    for v in values:
        for j in range(width):
            if (v >> j) & 1:
                out[bitpos // 8] |= 1 << (bitpos % 8)
            bitpos += 1
    return bytes(out)


def def_levels_region(levels, max_def):
    width = max_def.bit_length()
    stream = rle_runs(levels, width)
    return struct.pack("<I", len(stream)) + stream


def dict_indices_region(indices, dict_size):
    width = (dict_size - 1).bit_length()
    groups = (len(indices) + 7) // 8
    padded = list(indices) + [0] * (groups * 8 - len(indices))
    out = bytearray([width])
    _put_varint(out, (groups << 1) | 1)
    out += bitpack_lsb(padded, width)
    return bytes(out)


def compress(codec, payload):
    if codec == CODEC_UNCOMPRESSED:
        return payload
    if codec == CODEC_GZIP:
        return gzip.compress(payload, mtime=0)
    # Deliberately "supported-looking" bytes for an unsupported codec.
    return payload


def page_header(page_type, uncompressed, compressed, data_hdr=None, dict_hdr=None):
    t = TS()
    t.i32(1, page_type)
    t.i32(2, uncompressed)
    t.i32(3, compressed)
    if data_hdr is not None:
        nv, enc, defenc, repenc = data_hdr
        h = TS()
        h.i32(1, nv)
        h.i32(2, enc)
        h.i32(3, defenc)
        h.i32(4, repenc)
        t.struct(5, h)
    if dict_hdr is not None:
        nv, enc = dict_hdr
        h = TS()
        h.i32(1, nv)
        h.i32(2, enc)
        t.struct(7, h)
    return t.finish()


def build_chunk(col):
    """Build one column chunk; return (bytes, meta)."""
    physical = col["physical"]
    values = col["values"]
    optional = col.get("optional", False)
    use_dict = col.get("dict", False)
    codec = col.get("codec", CODEC_UNCOMPRESSED)
    page_rows = col.get("page_rows") or max(len(values), 1)
    max_def = 1 if optional else 0

    defs_all = [1 if v is not None else 0 for v in values] if optional else None
    present_all = [v for v in values if v is not None]

    dict_vals = None
    dict_index = None
    if use_dict:
        dict_vals = sorted(set(present_all))
        dict_index = {v: i for i, v in enumerate(dict_vals)}

    out = bytearray()
    dict_off = None
    data_off = None
    total_unc = 0
    total_comp = 0
    pages = 0
    i = 0
    first = True
    while i < len(values) or first:
        page_vals = values[i:i + page_rows]
        page_defs = defs_all[i:i + page_rows] if optional else None
        page_present = [v for v in page_vals if v is not None]
        payload = bytearray()
        if optional:
            payload += def_levels_region(page_defs, max_def)
        if use_dict:
            payload += dict_indices_region([dict_index[v] for v in page_present], len(dict_vals))
        else:
            payload += plain_values(physical, page_present, col.get("type_length"))
        nvals = len(page_vals)
        if first and use_dict:
            dpayload = plain_values(physical, dict_vals, col.get("type_length"))
            dcomp = compress(codec, dpayload)
            hdr = page_header(2, len(dpayload), len(dcomp), dict_hdr=(len(dict_vals), ENC_PLAIN))
            dict_off = len(out)
            out += hdr + dcomp
            total_unc += len(hdr) + len(dpayload)
            total_comp += len(hdr) + len(dcomp)
        enc = ENC_RLE_DICTIONARY if use_dict else col.get("encoding", ENC_PLAIN)
        comp = compress(codec, bytes(payload))
        hdr = page_header(
            0,
            col.get("declared_uncompressed", len(payload)),
            len(comp),
            data_hdr=(nvals, enc, ENC_RLE, ENC_RLE),
        )
        if data_off is None:
            data_off = len(out)
        out += hdr + comp
        total_unc += len(hdr) + col.get("declared_uncompressed", len(payload))
        total_comp += len(hdr) + len(comp)
        pages += 1
        i += page_rows
        first = False
        if not values:
            break

    encodings = [ENC_PLAIN, ENC_RLE]
    if use_dict:
        encodings.append(ENC_RLE_DICTIONARY)
    meta = {
        "num_values": len(values),
        "dict_off": dict_off,
        "data_off": data_off,
        "total_unc": total_unc,
        "total_comp": total_comp,
        "pages": pages,
        "encodings": encodings,
    }
    return bytes(out), meta


def leaf_schema(col):
    s = TS()
    s.i32(1, col["physical"])
    if col["physical"] == T_FLBA:
        s.i32(2, col["type_length"])
    s.i32(3, REP_OPTIONAL if col.get("optional") else REP_REQUIRED)
    s.string(4, col["name"])
    if "converted" in col:
        s.i32(6, col["converted"])
    return s


def write_parquet(path, columns, rows_per_rg=None, created_by="make-parquet.py (Phase 21.14)"):
    all_rows = max((len(c["values"]) for c in columns), default=0)
    bounds = []
    if rows_per_rg:
        i = 0
        while i < all_rows or not bounds:
            bounds.append((i, min(i + rows_per_rg, all_rows)))
            i += rows_per_rg
            if i >= all_rows:
                break
    else:
        bounds = [(0, all_rows)]

    out = bytearray(b"PAR1")
    row_groups = []
    for (lo, hi) in bounds:
        chunk_ts = []
        rg_unc = 0
        rg_comp = 0
        for c in columns:
            sub = dict(c)
            sub["values"] = c["values"][lo:hi]
            base = len(out)
            data, meta = build_chunk(sub)
            out += data
            file_off = base + (meta["dict_off"] if meta["dict_off"] is not None else meta["data_off"])
            cm = TS()
            cm.i32(1, c["physical"])
            cm.list_i32(2, meta["encodings"])
            cm.list_string(3, [c["name"]])
            cm.i32(4, c.get("codec", CODEC_UNCOMPRESSED))
            cm.i64(5, meta["num_values"])
            cm.i64(6, meta["total_unc"])
            cm.i64(7, meta["total_comp"])
            cm.i64(9, base + meta["data_off"])
            if meta["dict_off"] is not None:
                cm.i64(11, base + meta["dict_off"])
            # Statistics (modern min_value/max_value, plain-encoded).
            present = [v for v in c["values"][lo:hi] if v is not None]
            if present:
                # `min_value`/`max_value` are PLAIN-encoded (4-byte LE length
                # prefix + bytes for BYTE_ARRAY; fixed width for FLBA).
                st = TS()
                st.i64(3, sum(1 for v in c["values"][lo:hi] if v is None))
                st.binary(5, plain_values(c["physical"], [max(present)], c.get("type_length")))
                st.binary(6, plain_values(c["physical"], [min(present)], c.get("type_length")))
                cm.struct(12, st)
            cc = TS()
            cc.i64(2, file_off)
            cc.struct(3, cm)
            chunk_ts.append(cc)
            rg_unc += meta["total_unc"]
            rg_comp += meta["total_comp"]
        rg = TS()
        rg.list_struct(1, chunk_ts)
        rg.i64(2, rg_unc)
        rg.i64(3, hi - lo)
        rg.i64(6, rg_comp)
        row_groups.append(rg)

    schema = [leaf_schema(c) for c in columns]
    root = TS()
    root.string(4, "schema")
    root.i32(5, len(columns))

    fm = TS()
    fm.i32(1, 1)
    fm._fh(2, 9)
    fm._lh(12, len(schema) + 1)
    fm.b.extend(root.finish())
    for s in schema:
        fm.b.extend(s.finish())
    fm.i64(3, all_rows)
    fm.list_struct(4, row_groups)
    fm.string(6, created_by)
    footer = fm.finish()

    out += footer
    out += struct.pack("<I", len(footer))
    out += b"PAR1"
    with open(path, "wb") as f:
        f.write(bytes(out))
    return len(out)


# --- the fixture family ------------------------------------------------------
def _ba(s):
    return s.encode("utf-8")


def small_plain():
    cols = [
        {"name": "flag", "physical": T_BOOLEAN,
         "values": [True, False, True, True, False, False, True, False, True, True]},
        {"name": "i32", "physical": T_INT32, "converted": 17,
         "values": [1, -2, 300, -4000, 5, 6, 7, -8, 9, 10]},
        {"name": "i64", "physical": T_INT64, "converted": 18,
         "values": [10 ** 12, -5, 7, 8, 9, 10, 11, 12, 13, 14]},
        {"name": "f", "physical": T_FLOAT, "values": [1.5, -2.25, 0.0, 3.5, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]},
        {"name": "d", "physical": T_DOUBLE, "values": [1.25, -3.5, 2.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]},
        {"name": "s", "physical": T_BYTE_ARRAY, "converted": 0,
         "values": [_ba(x) for x in ["alpha", "beta", "gamma", "delta", "epsilon",
                                     "zeta", "eta", "theta", "iota", "kappa"]]},
        {"name": "raw", "physical": T_FLBA, "type_length": 4,
         "values": [b"\x00\x01\x02\x03", b"\x04\x05\x06\x07", b"\x08\x09\x0a\x0b",
                    b"\x0c\x0d\x0e\x0f", b"\x10\x11\x12\x13", b"\x14\x15\x16\x17",
                    b"\x18\x19\x1a\x1b", b"\x1c\x1d\x1e\x1f", b"\x20\x21\x22\x23",
                    b"\x24\x25\x26\x27"]},
    ]
    return {"columns": cols}


def optional():
    cols = [
        {"name": "id", "physical": T_INT64, "converted": 18,
         "values": [1, 2, 3, 4, 5, 6], "optional": True},
        {"name": "note", "physical": T_BYTE_ARRAY, "converted": 0, "optional": True,
         "values": [_ba("first"), None, _ba("third"), None, None, _ba("last")]},
    ]
    return {"columns": cols}


def dictionary():
    cols = [
        {"name": "city", "physical": T_BYTE_ARRAY, "converted": 0, "dict": True,
         "values": [_ba(x) for x in ["Oslo", "Bergen", "Oslo", "Trondheim", "Bergen",
                                     "Oslo", "Oslo", "Bergen", "Trondheim", "Oslo",
                                     "Bergen", "Bergen", "Oslo", "Trondheim", "Oslo"]]},
        {"name": "code", "physical": T_INT32, "dict": True, "converted": 17,
         "values": [10, 20, 10, 30, 20, 10, 10, 20, 30, 10, 20, 20, 10, 30, 10]},
    ]
    return {"columns": cols}


def gzip_file():
    d = small_plain()
    for c in d["columns"]:
        c["codec"] = CODEC_GZIP
    return d


def multi_rg():
    n = 24
    cols = [
        {"name": "k", "physical": T_INT64,
         "values": [i for i in range(n)]},
        {"name": "label", "physical": T_BYTE_ARRAY, "converted": 0,
         "values": [_ba("row-%03d" % i) for i in range(n)]},
    ]
    return {"columns": cols, "rows_per_rg": 8}


def two_pages():
    n = 20
    cols = [
        {"name": "v", "physical": T_INT32, "page_rows": 7,
         "values": [i * i for i in range(n)]},
        {"name": "tag", "physical": T_BYTE_ARRAY, "converted": 0, "page_rows": 6,
         "values": [_ba("t%02d" % i) for i in range(n)]},
    ]
    return {"columns": cols}


def large(target=2 * 1024 * 1024):
    n = 0
    ints = []
    ds = []
    ss = []
    size = 0
    i = 0
    while size < target:
        ints.append((i * 2654435761) % (1 << 40))
        ds.append(i * 0.5 - 1000.0)
        s = _ba("record-%07d-%s" % (i, "x" * 12))
        ss.append(s)
        size += 8 + 8 + 4 + len(s)
        i += 1
        n += 1
    cols = [
        {"name": "id", "physical": T_INT64, "values": ints},
        {"name": "score", "physical": T_DOUBLE, "values": ds},
        {"name": "name", "physical": T_BYTE_ARRAY, "converted": 0, "values": ss},
    ]
    return {"columns": cols, "rows_per_rg": 50000}


def unsupported_codec():
    d = dictionary()
    d["columns"][0]["codec"] = CODEC_ZSTD
    return d


def unsupported_encoding():
    cols = [
        {"name": "x", "physical": T_INT32, "encoding": ENC_DELTA_BINARY_PACKED,
         "values": [1, 2, 3, 4, 5]},
    ]
    return {"columns": cols}


def bomb():
    # A GZIP column whose GZIP page declares a decompressed size of 2^40 bytes.
    cols = [
        {"name": "z", "physical": T_INT64, "codec": CODEC_GZIP,
         "declared_uncompressed": 1 << 40, "values": [1, 2, 3, 4]},
    ]
    return {"columns": cols}


def emit_corpus(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    manifest = []

    def emit(name, data, fn=None):
        path = os.path.join(out_dir, name)
        if fn is None:
            with open(path, "wb") as f:
                f.write(data)
            data2 = data
        else:
            fn(path)
            with open(path, "rb") as f:
                data2 = f.read()
        manifest.append((name, len(data2), hashlib.sha256(data2).hexdigest()))

    for name, spec in [
        ("small_plain.parquet", small_plain),
        ("optional.parquet", optional),
        ("dictionary.parquet", dictionary),
        ("gzip.parquet", gzip_file),
        ("multi_rg.parquet", multi_rg),
        ("two_pages.parquet", two_pages),
        ("large.parquet", large),
        ("unsupported_codec.parquet", unsupported_codec),
        ("unsupported_encoding.parquet", unsupported_encoding),
        ("bomb.parquet", bomb),
    ]:
        d = spec()
        path = os.path.join(out_dir, name)
        write_parquet(path, d["columns"], d.get("rows_per_rg"))
        with open(path, "rb") as f:
            data = f.read()
        manifest.append((name, len(data), hashlib.sha256(data).hexdigest()))

    # Opaque controls.
    prose = (
        b"This is plain prose, not a Parquet file.\n"
        b"It has PAR1 nowhere meaningful and no footer.\n"
    )
    emit("prose.txt", prose)

    # A truncated Parquet file: valid bytes minus the trailing magic.
    good = os.path.join(out_dir, "small_plain.parquet")
    with open(good, "rb") as f:
        g = f.read()
    emit("truncated.parquet", g[:-4])

    # A file with PAR1 at both ends but an inconsistent footer length.
    bad = bytearray(g)
    n = len(bad)
    bad[n - 8:n - 4] = struct.pack("<I", 0xFFFFFFF)
    emit("badlen.parquet", bytes(bad))

    for name, length, sha in manifest:
        print("%s\t%d\t%s" % (name, length, sha))
    return manifest


def main():
    if len(sys.argv) >= 3 and sys.argv[1] == "--corpus":
        emit_corpus(sys.argv[2])
        return 0
    here = os.path.dirname(os.path.abspath(__file__))
    out = os.path.join(here, "parquet")
    os.makedirs(out, exist_ok=True)
    for name, length, sha in emit_corpus(out):
        print("%s\t%d bytes" % (name, length))
    return 0


if __name__ == "__main__":
    sys.exit(main())
