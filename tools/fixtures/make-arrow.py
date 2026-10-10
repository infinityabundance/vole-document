#!/usr/bin/env python3
# Phase 21.16 — deterministic, stdlib-only Apache Arrow IPC fixture generator.
#
# The `analytical` container has DuckDB but **not** pyarrow, and DuckDB's wheel
# exposes only the Arrow **C data interface** scanners (`arrow_scan*`), not an IPC
# file reader. So this generator constructs valid Apache Arrow IPC files/streams by
# hand: a small, purpose-built Flatbuffers builder for the metadata
# (Schema/Message/RecordBatch/Footer) plus little-endian columnar buffer encoders
# for the common types. The `analytical` court therefore uses a Parquet/DuckDB
# *projection* of the same logical table as the columnar comparator (documented),
# and VOLE's own reader is cross-checked against the values this generator writes.
#
# The Flatbuffers metadata is built correctly (tables with vtables, forward
# uoffsets, 8-byte padding after the metadata, 8-aligned buffers) so the files are
# genuine Arrow IPC: the court/tests decode the very same bytes.
#
#   python3 tools/fixtures/make-arrow.py                # write tools/fixtures/arrow/
#   python3 tools/fixtures/make-arrow.py --corpus DIR   # write DIR/, print TSV
#   python3 tools/fixtures/make-arrow.py --econ DIR     # write the econ corpus
#                                                       # (.arrow + .parquet projections)
#
# The TSV is `name<TAB>bytes<TAB>sha256` per fixture, matching the other generators.

import hashlib
import os
import struct
import sys

# --- Arrow `Type` union tags (Schema.fbs) ---
T_NULL = 1
T_INT = 2
T_FLOAT = 3
T_BINARY = 4
T_UTF8 = 5
T_BOOL = 6
T_DECIMAL = 7
T_DATE = 8
T_TIME = 9
T_TIMESTAMP = 10
T_INTERVAL = 11
T_LIST = 12
T_STRUCT = 13
T_UNION = 14
T_FIXED_SIZE_BINARY = 15
T_FIXED_SIZE_LIST = 16
T_MAP = 17
T_DURATION = 18
T_LARGE_BINARY = 19
T_LARGE_UTF8 = 20
T_LARGE_LIST = 21
T_RUN_END_ENCODED = 22

H_SCHEMA = 1
H_RECORD_BATCH = 3

VERSION = 4  # MetadataVersion V5
CONT = 0xFFFFFFFF
MAGIC = b"ARROW1"


# ---------------------------------------------------------------------------
# Minimal Flatbuffers builder (backward)
# ---------------------------------------------------------------------------
class FBB:
    def __init__(self, size=1 << 20):
        self.buf = bytearray(size)
        self.head = size
        self.minalign = 1
        self.vtable = None
        self.object_end = 0

    def Offset(self):
        return len(self.buf) - self.head

    def _need(self, n):
        assert self.head - n >= 0, "flatbuffer arena exhausted"

    def Pad(self, n):
        self._need(n)
        self.head -= n  # bytes are pre-zeroed

    def Prep(self, size, additional):
        if size > self.minalign:
            self.minalign = size
        alignsize = (-(self.Offset() + additional)) & (size - 1)
        self.Pad(alignsize)

    def Place(self, x, fmt):
        size = struct.calcsize(fmt)
        self._need(size)
        self.head -= size
        struct.pack_into(fmt, self.buf, self.head, x)

    def Push(self, x, fmt):
        self.Prep(struct.calcsize(fmt), 0)
        self.Place(x, fmt)

    def PrependUOffsetRelative(self, off):
        self.Prep(4, 0)
        assert off <= self.Offset()
        rel = self.Offset() - off + 4
        self.Place(rel, "<I")

    # -- tables --
    def StartObject(self, n):
        self.vtable = [0] * n
        self.object_end = self.Offset()

    def SlotScalar(self, slot, fmt, x, present=True):
        if present:
            self.Push(x, fmt)
            self.vtable[slot] = self.Offset()

    def SlotOffset(self, slot, off, present=True):
        if present:
            self.PrependUOffsetRelative(off)
            self.vtable[slot] = self.Offset()

    def EndObject(self):
        self.Prep(4, 0)
        self.Place(0, "<i")  # soffset placeholder
        object_offset = self.Offset()
        n = len(self.vtable)
        for i in reversed(range(n)):
            off = self.vtable[i]
            if off != 0:
                off = object_offset - off
            self.Push(off, "<H")
        self.Push(object_offset - self.object_end, "<H")
        self.Push((n + 2) * 2, "<H")
        vtable_offset = self.Offset()
        pos = len(self.buf) - object_offset
        struct.pack_into("<i", self.buf, pos, vtable_offset - object_offset)
        self.vtable = None
        return object_offset

    # -- vectors --
    def StartVector(self, elem_size, n, alignment):
        self.Prep(4, elem_size * n)
        self.Prep(alignment, elem_size * n)

    def EndVector(self, n):
        self.Place(n, "<I")
        return self.Offset()

    def VectorOffset(self, offsets):
        self.StartVector(4, len(offsets), 4)
        for o in reversed(offsets):
            self.PrependUOffsetRelative(o)
        return self.EndVector(len(offsets))

    def VectorI32(self, vals):
        self.StartVector(4, len(vals), 4)
        for v in reversed(vals):
            self.Push(v, "<i")
        return self.EndVector(len(vals))

    def CreateString(self, s):
        x = s.encode("utf-8") if isinstance(s, str) else s
        self.Prep(4, len(x) + 1)
        self.Place(0, "B")
        self._need(len(x))
        self.head -= len(x)
        self.buf[self.head:self.head + len(x)] = x
        return self.EndVector(len(x))

    def StructVector16(self, pairs):
        # pairs: list of (a, b); memory layout per element is [a:i64][b:i64].
        self.StartVector(16, len(pairs), 8)
        for (a, b) in reversed(pairs):
            self.Place(b, "<q")
            self.Place(a, "<q")
        return self.EndVector(len(pairs))

    def Finish(self, root):
        self.Prep(self.minalign, 4)
        self.PrependUOffsetRelative(root)
        return self.Offset()

    def Output(self):
        return bytes(self.buf[self.head:])


# ---------------------------------------------------------------------------
# Arrow type tables
# ---------------------------------------------------------------------------
def build_type(b, ts):
    """Build a Type table; return its offset (or 0 for a tag-only type)."""
    k = ts["kind"]
    if k == "int":
        b.StartObject(2)
        if ts["signed"]:
            b.SlotScalar(1, "B", 1)
        b.SlotScalar(0, "i", ts["bw"])
        return b.EndObject()
    if k == "float":
        b.StartObject(1)
        b.SlotScalar(0, "h", ts["prec"])
        return b.EndObject()
    if k == "decimal":
        b.StartObject(3)
        b.SlotScalar(2, "i", ts.get("bw", 128))
        b.SlotScalar(1, "i", ts["scale"])
        b.SlotScalar(0, "i", ts["precision"])
        return b.EndObject()
    if k == "date":
        b.StartObject(1)
        b.SlotScalar(0, "h", ts["unit"])
        return b.EndObject()
    if k == "time":
        b.StartObject(2)
        b.SlotScalar(1, "i", ts["bw"])
        b.SlotScalar(0, "h", ts["unit"])
        return b.EndObject()
    if k == "timestamp":
        tz = b.CreateString(ts.get("tz", ""))
        b.StartObject(2)
        b.SlotOffset(1, tz)
        b.SlotScalar(0, "h", ts["unit"])
        return b.EndObject()
    if k == "duration":
        b.StartObject(1)
        b.SlotScalar(0, "h", ts["unit"])
        return b.EndObject()
    if k == "interval":
        b.StartObject(1)
        b.SlotScalar(0, "h", ts["unit"])
        return b.EndObject()
    if k == "fsb":
        b.StartObject(1)
        b.SlotScalar(0, "i", ts["width"])
        return b.EndObject()
    if k in ("list", "large_list"):
        b.StartObject(2)  # List has no fields of its own
        return b.EndObject()
    # tag-only types (bool/utf8/binary/large_*/struct/map/union family)
    return 0


def type_tag(ts):
    return {
        "null": T_NULL,
        "int": T_INT,
        "float": T_FLOAT,
        "bool": T_BOOL,
        "binary": T_BINARY,
        "utf8": T_UTF8,
        "decimal": T_DECIMAL,
        "date": T_DATE,
        "time": T_TIME,
        "timestamp": T_TIMESTAMP,
        "interval": T_INTERVAL,
        "list": T_LIST,
        "large_list": T_LARGE_LIST,
        "struct": T_STRUCT,
        "union": T_UNION,
        "fsb": T_FIXED_SIZE_BINARY,
        "duration": T_DURATION,
        "large_binary": T_LARGE_BINARY,
        "large_utf8": T_LARGE_UTF8,
    }[ts["kind"]]


def build_dict_encoding(b, dict_id, index_bw=32, signed=True):
    # DictionaryEncoding { id:long(0), indexType:Int(1), isOrdered:bool(2) }
    # The nested `Int` indexType table must be built *before* the outer object.
    idx = b.StartObject(2)
    if signed:
        b.SlotScalar(1, "B", 1)
    b.SlotScalar(0, "i", index_bw)
    idx_off = b.EndObject()
    b.StartObject(4)
    b.SlotOffset(1, idx_off)
    b.SlotScalar(0, "q", dict_id)
    return b.EndObject()


def build_field(b, name, ts, nullable=False, dict_id=None):
    """Build a Field table; children of nested types are built first."""
    name_off = b.CreateString(name)
    child_offsets = []
    for child in ts.get("children", []):
        child_offsets.append(build_field(b, child["name"], child["type"], child.get("nullable", False)))
    children_off = b.VectorOffset(child_offsets) if child_offsets else 0
    dict_off = build_dict_encoding(b, dict_id) if dict_id is not None else 0
    type_off = build_type(b, ts)
    b.StartObject(7)
    if children_off:
        b.SlotOffset(5, children_off)
    if dict_off:
        b.SlotOffset(4, dict_off)
    if type_off:
        b.SlotOffset(3, type_off)
    b.SlotScalar(2, "B", type_tag(ts))
    if nullable:
        b.SlotScalar(1, "B", 1)
    b.SlotOffset(0, name_off)
    return b.EndObject()


def build_schema(b, fields):
    # `fields` is a list of (name, type_spec, nullable, dict_id).
    offs = [build_field(b, nm, ts, nl, d) for (nm, ts, nl, d) in fields]
    fields_off = b.VectorOffset(offs)
    b.StartObject(4)  # endianness(0), fields(1), custom_metadata(2), features(3)
    b.SlotOffset(1, fields_off)
    return b.EndObject()


def build_message(b, header_type, header_off, body_len):
    b.StartObject(5)  # version(0), header_type(1), header(2), bodyLength(3)
    b.SlotScalar(3, "q", body_len)
    b.SlotOffset(2, header_off)
    b.SlotScalar(1, "B", header_type)
    b.SlotScalar(0, "h", VERSION)
    return b.EndObject()


def build_record_batch(b, length, nodes, buffers, compression=None):
    bufs_off = b.StructVector16(buffers)
    nodes_off = b.StructVector16(nodes)
    comp_off = 0
    if compression is not None:
        b.StartObject(2)  # codec(0), method(1)
        b.SlotScalar(1, "B", 0)
        b.SlotScalar(0, "B", compression)
        comp_off = b.EndObject()
    b.StartObject(5)  # length(0), nodes(1), buffers(2), compression(3)
    if comp_off:
        b.SlotOffset(3, comp_off)
    b.SlotOffset(2, bufs_off)
    b.SlotOffset(1, nodes_off)
    b.SlotScalar(0, "q", length)
    return b.EndObject()


def build_block(b, offset, meta_len, body_len):
    b.StartObject(3)  # offset(0), metaDataLength(1), bodyLength(2)
    b.SlotScalar(2, "q", body_len)
    b.SlotScalar(1, "i", meta_len)
    b.SlotScalar(0, "q", offset)
    return b.EndObject()


def build_footer(b, schema_off, blocks):
    blk_offs = [build_block(b, o, m, l) for (o, m, l) in blocks]
    if blk_offs:
        rbb = b.VectorOffset(blk_offs)
    else:
        rbb = 0
    b.StartObject(5)  # version(0), schema(1), dictionaries(2), recordBatches(3)
    if rbb:
        b.SlotOffset(3, rbb)
    b.SlotOffset(1, schema_off)
    b.SlotScalar(0, "h", VERSION)
    return b.EndObject()


def flatbuf(build):
    b = FBB()
    root = build(b)
    b.Finish(root)
    return b.Output()


# ---------------------------------------------------------------------------
# Columnar buffer encoders
# ---------------------------------------------------------------------------
def bitpack(bits):
    out = bytearray((len(bits) + 7) // 8)
    for i, bit in enumerate(bits):
        if bit:
            out[i // 8] |= 1 << (i % 8)
    return bytes(out)


def pad8(x):
    return x + b"\x00" * ((-len(x)) % 8)


def encode_column(ts, values):
    """Return a list of logical buffers for one column (validity, [offsets], data)
    plus the null count."""
    n = len(values)
    nulls = sum(1 for v in values if v is None)
    validity = bitpack([1 if v is not None else 0 for v in values]) if nulls else b""
    k = ts["kind"]
    bufs = [validity]
    if k == "bool":
        data = bitpack([1 if v else 0 for v in values])
        bufs.append(data)
        return bufs, nulls
    if k in ("utf8", "binary", "large_utf8", "large_binary"):
        wide = k in ("large_utf8", "large_binary")
        offs = []
        data = bytearray()
        acc = 0
        for v in values:
            offs.append(acc)
            if v is not None:
                data += v
                acc += len(v)
        offs.append(acc)
        if wide:
            offs_b = b"".join(struct.pack("<q", o) for o in offs)
        else:
            offs_b = b"".join(struct.pack("<i", o) for o in offs)
        bufs.append(offs_b)
        bufs.append(bytes(data))
        return bufs, nulls
    # fixed-width
    data = bytearray()
    for v in values:
        data += encode_fixed(ts, v)
    bufs.append(bytes(data))
    return bufs, nulls


def encode_fixed(ts, v):
    if v is None:
        v = 0
    k = ts["kind"]
    if k == "int":
        fmt = {8: "b", 16: "h", 32: "i", 64: "q"} if ts["signed"] else {8: "B", 16: "H", 32: "I", 64: "Q"}
        return struct.pack("<" + fmt[ts["bw"]], v)
    if k == "float":
        fmt = {0: "e", 1: "f", 2: "d"}[ts["prec"]]
        return struct.pack("<" + fmt, v)
    if k == "date":
        return struct.pack("<i" if ts["unit"] == 0 else "<q", v)
    if k == "time":
        return struct.pack("<i" if ts["bw"] == 32 else "<q", v)
    if k in ("timestamp", "duration", "interval"):
        return struct.pack("<q", v)
    if k == "fsb":
        assert len(v) == ts["width"]
        return v
    if k == "decimal":
        w = ts.get("bw", 128) // 8
        return int(v).to_bytes(w, "little", signed=True)
    raise ValueError("no fixed encoder for %r" % k)


# ---------------------------------------------------------------------------
# Encapsulated messages and whole files
# ---------------------------------------------------------------------------
def encapsulate(core, body):
    meta = pad8(core)
    body = pad8(body)
    return struct.pack("<I", CONT) + struct.pack("<i", len(meta)) + meta + body


def schema_message(fields):
    core = flatbuf(lambda b: build_message(b, H_SCHEMA, build_schema(b, fields), 0))
    return encapsulate(core, b"")


def record_batch_message(columns, rows, compression=None):
    """`columns` are (ts, values) for this batch. Returns (message_bytes, body_len)."""
    nodes = []
    buffers = []
    body = bytearray()
    for (ts, values) in columns:
        bufs, nulls = encode_column(ts, values)
        nodes.append((len(values), nulls))
        for buf in bufs:
            off = len(body)
            body += pad8(buf)  # keep the next buffer 8-aligned
            buffers.append((off, len(buf)))
    core = flatbuf(lambda b: build_message(
        b, H_RECORD_BATCH,
        build_record_batch(b, rows, nodes, buffers, compression), len(body)))
    return encapsulate(core, bytes(body)), len(body)


def write_file(path, fields, batches, stream=False):
    """`fields` is a list of (name, type_spec, nullable, dict_id);
    `batches` is a list of (columns, rows) where columns are (ts, values)."""
    out = bytearray(MAGIC + b"\x00\x00")
    out += schema_message(fields)
    blocks = []
    for (columns, rows) in batches:
        start = len(out)
        msg, body_len = record_batch_message(columns, rows)
        # meta_len (padded) is stored in the prefix
        meta_len = struct.unpack_from("<i", msg, 4)[0]
        out += msg
        blocks.append((start, meta_len, body_len))
    if stream:
        out += struct.pack("<I", CONT) + struct.pack("<i", 0)  # EOS
    else:
        out += struct.pack("<I", CONT) + struct.pack("<i", 0)  # EOS inside file
        footer = flatbuf(lambda b: build_footer(b, build_schema(b, fields), blocks))
        out += footer
        out += struct.pack("<i", len(footer))
        out += MAGIC
    with open(path, "wb") as f:
        f.write(bytes(out))
    return len(out)


# ---------------------------------------------------------------------------
# The fixture family
# ---------------------------------------------------------------------------
INT64 = {"kind": "int", "bw": 64, "signed": True}
INT32 = {"kind": "int", "bw": 32, "signed": True}
INT8 = {"kind": "int", "bw": 8, "signed": True}
U8 = {"kind": "int", "bw": 8, "signed": False}
U16 = {"kind": "int", "bw": 16, "signed": False}
U32 = {"kind": "int", "bw": 32, "signed": False}
U64 = {"kind": "int", "bw": 64, "signed": False}
F16 = {"kind": "float", "prec": 0}
F32 = {"kind": "float", "prec": 1}
F64 = {"kind": "float", "prec": 2}
BOOL = {"kind": "bool"}
UTF8 = {"kind": "utf8"}
LUTF8 = {"kind": "large_utf8"}
BIN = {"kind": "binary"}
LBIN = {"kind": "large_binary"}
DATE32 = {"kind": "date", "unit": 0}
TS_MICRO = {"kind": "timestamp", "unit": 2, "tz": "UTC"}
TIME64 = {"kind": "time", "unit": 2, "bw": 64}
DUR_MICRO = {"kind": "duration", "unit": 2}
FSB4 = {"kind": "fsb", "width": 4}
DEC128 = {"kind": "decimal", "precision": 10, "scale": 2, "bw": 128}


def f_primitives():
    rows = 8
    cols = [
        ("i8", INT8, [i - 4 for i in range(rows)]),
        ("i16", {"kind": "int", "bw": 16, "signed": True}, [i * 100 - 300 for i in range(rows)]),
        ("i32", INT32, [i * 1000 - 4000 for i in range(rows)]),
        ("i64", INT64, [i * 10 ** 9 - 4 * 10 ** 9 for i in range(rows)]),
        ("u8", U8, [200 + i for i in range(rows)]),
        ("u16", U16, [60000 + i for i in range(rows)]),
        ("u32", U32, [4 * 10 ** 9 + i for i in range(rows)]),
        ("u64", U64, [2 ** 63 + i for i in range(rows)]),
        ("f16", F16, [0.5 + i for i in range(rows)]),
        ("f32", F32, [1.5 * i - 2.0 for i in range(rows)]),
        ("f64", F64, [i * 0.25 - 1.0 for i in range(rows)]),
        ("flag", BOOL, [i % 3 == 0 for i in range(rows)]),
    ]
    fields = [(nm, ts, False, None) for (nm, ts, _) in cols]
    return fields, [([(ts, v) for (_, ts, v) in cols], rows)]


def f_nullable():
    cols = [
        ("id", INT64, [1, 2, 3, 4, 5, 6]),
        ("note", UTF8, [b"first", None, b"third", None, None, b"last"]),
        ("score", F64, [1.5, None, -2.0, 3.25, None, 0.0]),
    ]
    fields = [(nm, ts, True, None) for (nm, ts, _) in cols]
    return fields, [([(ts, v) for (_, ts, v) in cols], 6)]


def f_strings():
    cols = [
        ("s", UTF8, [b"alpha", b"", b"gamma", b"d" * 300, b"eps"]),
        ("ls", LUTF8, [b"one", b"two", b"three", b"four", b"five"]),
        ("b", BIN, [b"\x00\x01", b"\xff", b"", b"\x10\x20\x30", b"\xaa" * 40]),
        ("lb", LBIN, [b"x", b"yy", b"zzz", b"", b"wwww"]),
    ]
    fields = [(nm, ts, False, None) for (nm, ts, _) in cols]
    return fields, [([(ts, v) for (_, ts, v) in cols], 5)]


def f_temporal():
    cols = [
        ("d", DATE32, [19000 + i for i in range(5)]),
        ("ts", TS_MICRO, [1_700_000_000_000_000 + i for i in range(5)]),
        ("t", TIME64, [3600_000_000 + i for i in range(5)]),
        ("dur", DUR_MICRO, [10 ** 6 * i for i in range(5)]),
    ]
    fields = [(nm, ts, False, None) for (nm, ts, _) in cols]
    return fields, [([(ts, v) for (_, ts, v) in cols], 5)]


def f_multi_batch():
    fields = [("k", INT32, False, None), ("label", UTF8, False, None)]
    batches = []
    for b in range(3):
        k = [b * 5 + i for i in range(5)]
        lab = [b"row-%02d" % (b * 5 + i) for i in range(5)]
        batches.append(([(INT32, k), (UTF8, lab)], 5))
    return fields, batches


def f_stream():
    fields = [("a", INT32, False, None), ("b", UTF8, True, None)]
    batches = [
        ([(INT32, [1, 2, 3]), (UTF8, [b"x", None, b"z"])], 3),
        ([(INT32, [4, 5]), (UTF8, [b"p", b"q"])], 2),
    ]
    return fields, batches


def f_fsb():
    fields = [("raw", FSB4, False, None)]
    vals = [bytes([i, i + 1, i + 2, i + 3]) for i in range(5)]
    return fields, [([(FSB4, vals)], 5)]


def f_large(target=1_500_000):
    ids = []
    scores = []
    names = []
    size = 0
    i = 0
    while size < target:
        ids.append((i * 2654435761) % (1 << 40))
        scores.append(i * 0.5 - 1000.0)
        nm = b"record-%07d-%s" % (i, b"x" * 12)
        names.append(nm)
        size += 8 + 8 + 4 + len(nm)
        i += 1
    fields = [("id", INT64, False, None), ("score", F64, False, None), ("name", UTF8, False, None)]
    batches = []
    step = 5000
    for lo in range(0, len(ids), step):
        hi = min(lo + step, len(ids))
        batches.append(([(INT64, ids[lo:hi]), (F64, scores[lo:hi]), (UTF8, names[lo:hi])], hi - lo))
    return fields, batches


def f_unsupported_decimal():
    fields = [("dec", DEC128, False, None)]
    vals = [1, 2, 3, 4, 5]
    return fields, [([(DEC128, vals)], 5)]


def f_unsupported_nested():
    list_ts = {"kind": "list", "children": [{"name": "item", "type": INT32}]}
    fields = [("lst", list_ts, False, None)]
    # A valid List<Int32> body: list validity, offsets, child validity, child values.
    bufs = [b"", struct.pack("<iii", 0, 2, 3), b"", struct.pack("<iii", 10, 20, 30)]
    return fields, bufs, 2


def f_unsupported_dictionary():
    fields = [("city", UTF8, False, 7)]
    indices = [0, 1, 0, 2]
    return fields, [([(INT32, indices)], 4)]


def f_unsupported_compressed():
    fields = [("z", INT64, False, None)]
    return fields, [([(INT64, [1, 2, 3, 4])], 4)]


def f_bomb():
    fields = [("big", INT64, False, None)]
    return fields, 2 ** 31


def f_malformed():
    # ARROW1 both ends, a consistent footer length, but a garbage footer body.
    junk = b"\x00" * 64
    return MAGIC + b"\x00\x00" + junk + struct.pack("<i", len(junk)) + MAGIC


# ---------------------------------------------------------------------------
# Emit
# ---------------------------------------------------------------------------
def _emit(path, data):
    with open(path, "wb") as f:
        f.write(data)
    return data


def write_nested(path, fields, bufs, rows):
    """Emit a record batch with an arbitrary pre-built buffer list (for declines)."""
    out = bytearray(MAGIC + b"\x00\x00")
    out += schema_message(fields)
    blocks = []
    body = bytearray()
    buffers = []
    for buf in bufs:
        off = len(body)
        body += pad8(buf)
        buffers.append((off, len(buf)))
    nodes = [(rows, 0)]
    core = flatbuf(lambda b: build_message(
        b, H_RECORD_BATCH, build_record_batch(b, rows, nodes, buffers), len(body)))
    msg = encapsulate(core, bytes(body))
    meta_len = struct.unpack_from("<i", msg, 4)[0]
    start = len(out)
    out += msg
    blocks.append((start, meta_len, len(body)))
    out += struct.pack("<I", CONT) + struct.pack("<i", 0)
    footer = flatbuf(lambda b: build_footer(b, build_schema(b, fields), blocks))
    out += footer
    out += struct.pack("<i", len(footer))
    out += MAGIC
    return _emit(path, bytes(out))


def emit_corpus(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    manifest = []

    def emit(name, data):
        p = os.path.join(out_dir, name)
        _emit(p, data)
        manifest.append((name, len(data), hashlib.sha256(data).hexdigest()))

    for name, fn in [
        ("primitives.arrow", f_primitives),
        ("nullable.arrow", f_nullable),
        ("strings.arrow", f_strings),
        ("temporal.arrow", f_temporal),
        ("multi_batch.arrow", f_multi_batch),
        ("fsb.arrow", f_fsb),
        ("large.arrow", f_large),
    ]:
        fields, batches = fn()
        p = os.path.join(out_dir, name)
        write_file(p, fields, batches)
        with open(p, "rb") as f:
            data = f.read()
        manifest.append((name, len(data), hashlib.sha256(data).hexdigest()))

    fields, batches = f_stream()
    p = os.path.join(out_dir, "stream.arrow")
    write_file(p, fields, batches, stream=True)
    with open(p, "rb") as f:
        data = f.read()
    manifest.append(("stream.arrow", len(data), hashlib.sha256(data).hexdigest()))

    # Typed declines / bombs.
    fields, batches = f_unsupported_decimal()
    p = os.path.join(out_dir, "unsupported_decimal.arrow")
    write_file(p, fields, batches)
    with open(p, "rb") as f:
        data = f.read()
    manifest.append(("unsupported_decimal.arrow", len(data), hashlib.sha256(data).hexdigest()))

    fields, bufs, rows = f_unsupported_nested()
    p = os.path.join(out_dir, "unsupported_nested.arrow")
    write_nested(p, fields, bufs, rows)
    with open(p, "rb") as f:
        data = f.read()
    manifest.append(("unsupported_nested.arrow", len(data), hashlib.sha256(data).hexdigest()))

    fields, batches = f_unsupported_dictionary()
    p = os.path.join(out_dir, "unsupported_dictionary.arrow")
    write_file(p, fields, batches)
    with open(p, "rb") as f:
        data = f.read()
    manifest.append(("unsupported_dictionary.arrow", len(data), hashlib.sha256(data).hexdigest()))

    # Compressed body: emit a normal batch but declare ZSTD compression.
    fields, batches = f_unsupported_compressed()
    out = bytearray(MAGIC + b"\x00\x00")
    out += schema_message(fields)
    columns, rows = batches[0]
    nodes = [(rows, 0)]
    body = bytearray()
    buffers = []
    for (ts, vals) in columns:
        bufs, nulls = encode_column(ts, vals)
        for buf in bufs:
            off = len(body)
            body += pad8(buf)
            buffers.append((off, len(buf)))
    core = flatbuf(lambda b: build_message(
        b, H_RECORD_BATCH, build_record_batch(b, rows, nodes, buffers, compression=1), len(body)))
    msg = encapsulate(core, bytes(body))
    start = len(out)
    out += msg
    meta_len = struct.unpack_from("<i", msg, 4)[0]
    blocks = [(start, meta_len, len(body))]
    out += struct.pack("<I", CONT) + struct.pack("<i", 0)
    footer = flatbuf(lambda b: build_footer(b, build_schema(b, fields), blocks))
    out += footer + struct.pack("<i", len(footer)) + MAGIC
    emit("unsupported_compressed.arrow", bytes(out))

    # Bomb: a record batch that claims 2^31 rows in a tiny body.
    fields, big_rows = f_bomb()
    out = bytearray(MAGIC + b"\x00\x00")
    out += schema_message(fields)
    nodes = [(big_rows, 0)]
    body = bytearray()
    buffers = []
    bl, _ = encode_column(INT64, [0])
    for buf in bl:
        off = len(body)
        body += pad8(buf)
        buffers.append((off, len(buf)))
    core = flatbuf(lambda b: build_message(
        b, H_RECORD_BATCH, build_record_batch(b, big_rows, nodes, buffers), len(body)))
    msg = encapsulate(core, bytes(body))
    start = len(out)
    out += msg
    meta_len = struct.unpack_from("<i", msg, 4)[0]
    blocks = [(start, meta_len, len(body))]
    out += struct.pack("<I", CONT) + struct.pack("<i", 0)
    footer = flatbuf(lambda b: build_footer(b, build_schema(b, fields), blocks))
    out += footer + struct.pack("<i", len(footer)) + MAGIC
    emit("bomb.arrow", bytes(out))

    # Malformed Flatbuffers: ARROW1 both ends, consistent footer length, garbage body.
    emit("malformed_flatbuf.arrow", f_malformed())

    # Opaque controls.
    emit("prose.txt", b"This is plain prose, not an Arrow file.\nNo ARROW1 footer here.\n")
    emit("magic_only.bin", MAGIC + b" not an arrow payload")
    good = bytearray(open(os.path.join(out_dir, "primitives.arrow"), "rb").read())
    emit("truncated.arrow", bytes(good[:64]))
    bad = bytearray(good)
    n = len(bad)
    bad[n - 10:n - 6] = struct.pack("<i", 0x0FFFFFFF)
    emit("badlen.arrow", bytes(bad))

    for name, length, sha in manifest:
        print("%s\t%d\t%s" % (name, length, sha))
    return manifest


# --- economics corpus -------------------------------------------------------
def _load_parquet_writer():
    import importlib.util
    here = os.path.dirname(os.path.abspath(__file__))
    path = os.path.join(here, "make-parquet.py")
    spec = importlib.util.spec_from_file_location("make_parquet", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


ECON = {
    "e_small.arrow": lambda: (
        [("id", INT64, False, None), ("name", UTF8, False, None), ("score", F64, False, None)],
        [([(INT64, list(range(200))),
           (UTF8, [b"row-%05d" % i for i in range(200)]),
           (F64, [i * 0.5 - 50.0 for i in range(200)])], 200)],
    ),
    "e_multi.arrow": lambda: (
        [("k", INT32, False, None), ("tag", UTF8, False, None)],
        [([(INT32, list(range(b * 500, (b + 1) * 500))),
           (UTF8, [b"t%05d" % i for i in range(b * 500, (b + 1) * 500)])], 500)
         for b in range(4)],
    ),
    "e_large.arrow": lambda: f_large(2_000_000),
}


def emit_econ(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    mp = _load_parquet_writer()
    manifest = []
    for name, fn in ECON.items():
        fields, batches = fn()
        p = os.path.join(out_dir, name)
        write_file(p, fields, batches)
        with open(p, "rb") as f:
            data = f.read()
        manifest.append((name, len(data), hashlib.sha256(data).hexdigest()))
        # A Parquet projection of the same logical table (for the DuckDB comparator).
        pq = os.path.join(out_dir, name[: -len(".arrow")] + ".parquet")
        _write_parquet_projection(mp, pq, fields, batches)
    for name, length, sha in manifest:
        print("%s\t%d\t%s" % (name, length, sha))
    return manifest


def _write_parquet_projection(mp, path, fields, batches):
    # Map each Arrow field to a Parquet physical column and concatenate the batches
    # (in field order). This is a *projection of the same logical table*, used only
    # as the columnar comparator in the economic court. `rows_per_rg` matches the
    # Arrow batch size so the row-group count equals the record-batch count.
    parquet_cols = []
    for idx, (nm, ts, _nl, _d) in enumerate(fields):
        vals = []
        for (columns, _rows) in batches:
            _cts, cvals = columns[idx]
            vals.extend(cvals)
        physical, converted = _parquet_map(ts)
        spec = {"name": nm, "physical": physical, "values": vals}
        if converted is not None:
            spec["converted"] = converted
        parquet_cols.append(spec)
    rows_per_rg = batches[0][1] if len(batches) > 1 else None
    mp.write_parquet(path, parquet_cols, rows_per_rg=rows_per_rg)


def _parquet_map(ts):
    k = ts["kind"]
    if k == "int":
        # Parquet has no INT8/16; the projection uses INT64 (values are in range).
        return 2, None
    if k == "float":
        return (5 if ts["prec"] == 2 else 4), None
    if k in ("utf8", "large_utf8"):
        return 6, 0  # BYTE_ARRAY with the UTF8 converted type
    raise ValueError("cannot project %r to Parquet" % k)


def main():
    if len(sys.argv) >= 3 and sys.argv[1] == "--corpus":
        emit_corpus(sys.argv[2])
        return 0
    if len(sys.argv) >= 3 and sys.argv[1] == "--econ":
        emit_econ(sys.argv[2])
        return 0
    here = os.path.dirname(os.path.abspath(__file__))
    out = os.path.join(here, "arrow")
    os.makedirs(out, exist_ok=True)
    for name, length, sha in emit_corpus(out):
        print("%s\t%d bytes" % (name, length))
    return 0


if __name__ == "__main__":
    sys.exit(main())
