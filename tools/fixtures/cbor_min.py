#!/usr/bin/env python3
# Phase 21.18 (economic court) — a **vendored, self-contained pure-Python CBOR
# decoder** (RFC 8949 subset). No third-party code, no network (the container has
# none). It exists so the economic court has an honest, reproducible **conventional
# comparator**: a CBOR -> host-value load.
#
# It decodes to a JSON-compatible host model (dict/list/int/float/str/bool/None) and
# therefore **drops** exactly the representation a conventional load cannot carry:
#
#   * byte string vs text string — `bin` (CBOR major type 2) is decoded with latin-1
#     into the *same* Python `str` type as a text string, so the byte-vs-text
#     distinction is **conflated** (the defining CBOR-vs-JSON gap);
#   * the encoding width actually used (0x17 vs 0x1817) — every integer becomes an
#     unbounded Python `int`, so the width and the signedness are gone;
#   * float width (half/single/double) — every float becomes a Python `float`
#     (IEEE-754 double), so the width is gone;
#   * tags (major type 6) — a tag is unwrapped (its number is never kept);
#   * map key order + duplicate keys — an object becomes a `dict` (last wins) unless
#     `pairs=True`, in which case a `Pairs` list keeps order and duplicates;
#   * every exact source span, and the exact token spelling.
#
# A conventional load drops all of those, which is why the court's Lane C must
# decline typed on the questions that need them. This module deliberately does NOT
# hide that.
#
# Public API:
#   loads(data, pairs=False) -> host value; raises CborError on malformed input
#   dump_strict(value)       -> strict JSON text preserving duplicate object members
#                               (raises for a value with no strict-JSON form: NaN, a
#                               non-string map key)
#   host_class(v)            -> coarse value class (number/string/bool/null/array/object)
#   canon_value(v)           -> canonical string for cross-lane value comparison
#   Pairs                    -> list subclass marking an object (ordered pairs)
#   CborError                -> decode / normalization error

import json
import math
import struct

__all__ = ["CborError", "Pairs", "loads", "dump_strict", "host_class", "canon_value"]

MAX_DEPTH = 256
MAX_ITEMS = 4_000_000
MAX_STR_BYTES = 64 * 1024 * 1024


class CborError(ValueError):
    """A malformed CBOR source, or a value with no strict-JSON representation."""


class Pairs(list):
    """A CBOR map: a list of ``(key, value)`` pairs (order + duplicates kept)."""


def _f16(bits):
    sign = (bits >> 15) & 1
    exp = (bits >> 10) & 0x1F
    frac = bits & 0x3FF
    if exp == 0:
        val = frac * 2.0 ** -24
    elif exp == 31:
        val = float("inf") if frac == 0 else float("nan")
    else:
        val = (1.0 + frac / 1024.0) * 2.0 ** (exp - 15)
    return -val if sign else val


class _Dec:
    def __init__(self, data, pairs):
        self.b = bytes(data)
        self.at = 0
        self.pairs = pairs
        self.items = 0

    def _err(self, msg):
        raise CborError("%s at offset %d" % (msg, self.at))

    def _take(self, n):
        if n < 0 or self.at + n > len(self.b):
            self._err("truncated item")
        out = self.b[self.at:self.at + n]
        self.at += n
        return out

    def _u(self, n):
        return int.from_bytes(self._take(n), "big")

    def _arg(self, ai):
        if ai < 24:
            return ai
        if ai == 24:
            return self._u(1)
        if ai == 25:
            return self._u(2)
        if ai == 26:
            return self._u(4)
        if ai == 27:
            return self._u(8)
        if ai == 31:
            return None  # indefinite-length marker
        self._err("reserved additional information %d" % ai)

    def item(self, depth):
        if depth > MAX_DEPTH:
            self._err("nesting too deep")
        self.items += 1
        if self.items > MAX_ITEMS:
            self._err("too many items")
        ib = self._u(1)
        major = ib >> 5
        ai = ib & 0x1F
        if major == 0:
            a = self._arg(ai)
            if a is None:
                self._err("indefinite uint is not well-formed")
            return a
        if major == 1:
            a = self._arg(ai)
            if a is None:
                self._err("indefinite negint is not well-formed")
            return -1 - a
        if major == 2:
            return self._string(ai, "bytes")
        if major == 3:
            return self._string(ai, "text")
        if major == 4:
            return self._array(ai, depth)
        if major == 5:
            return self._map(ai, depth)
        if major == 6:
            tag = self._arg(ai)
            if tag is None:
                self._err("indefinite tag is not well-formed")
            # A tag is transparent: unwrap it, never interpret it.
            return self.item(depth + 1)
        # major type 7: simple values and floats
        if ai == 20:
            return False
        if ai == 21:
            return True
        if ai == 22:
            return None
        if ai == 23:
            return None  # undefined -> null-ish host value
        if ai == 24:
            self._u(1)
            return None  # unassigned simple value
        if ai == 25:
            return _f16(self._u(2))
        if ai == 26:
            return struct.unpack(">f", self._take(4))[0]
        if ai == 27:
            return struct.unpack(">d", self._take(8))[0]
        if ai == 31:
            self._err("unexpected break")
        if ai < 20:
            return None  # unassigned simple value
        self._err("reserved simple/float additional information %d" % ai)

    def _chunk(self, ai, kind):
        if ai == 31:
            self._err("nested indefinite string chunk")
        n = self._arg(ai)
        if n is None:
            self._err("indefinite string chunk length")
        if n > MAX_STR_BYTES:
            self._err("string is too long")
        self.items += 1
        return self._take(n)

    def _string(self, ai, kind):
        if ai == 31:
            parts = []
            while True:
                if self.at >= len(self.b):
                    self._err("unterminated indefinite string")
                nb = self.b[self.at]
                if nb == 0xFF:
                    self.at += 1
                    break
                m2 = nb >> 5
                a2 = nb & 0x1F
                if (kind == "bytes" and m2 != 2) or (kind == "text" and m2 != 3):
                    self._err("indefinite string has a mismatched chunk")
                self.at += 1  # consume the chunk's head byte
                parts.append(self._chunk(a2, kind))
            data = b"".join(parts)
        else:
            data = self._chunk(ai, kind)
        if kind == "bytes":
            # Byte-vs-text is CONFLATED (latin-1), exactly as a conventional load does.
            return data.decode("latin-1")
        try:
            return data.decode("utf-8")
        except UnicodeDecodeError:
            self._err("text string is not valid UTF-8")

    def _array(self, ai, depth):
        out = []
        if ai == 31:
            while True:
                if self.at >= len(self.b):
                    self._err("unterminated indefinite array")
                if self.b[self.at] == 0xFF:
                    self.at += 1
                    break
                out.append(self.item(depth + 1))
        else:
            n = self._arg(ai)
            if n is None:
                self._err("bad array length")
            if n > MAX_ITEMS:
                self._err("array is too long")
            for _ in range(n):
                out.append(self.item(depth + 1))
        return out

    def _map(self, ai, depth):
        pairs = Pairs()
        if ai == 31:
            while True:
                if self.at >= len(self.b):
                    self._err("unterminated indefinite map")
                if self.b[self.at] == 0xFF:
                    self.at += 1
                    break
                k = self.item(depth + 1)
                v = self.item(depth + 1)
                pairs.append((k, v))
        else:
            n = self._arg(ai)
            if n is None:
                self._err("bad map length")
            if n > MAX_ITEMS:
                self._err("map is too long")
            for _ in range(n):
                k = self.item(depth + 1)
                v = self.item(depth + 1)
                pairs.append((k, v))
        if self.pairs:
            return pairs
        d = {}
        for k, v in pairs:
            try:
                d[k] = v
            except TypeError:  # unhashable key (e.g. a list): keep a stable stand-in
                d[repr(k)] = v
        return d


def loads(data, pairs=False):
    d = _Dec(data, pairs)
    v = d.item(0)
    if d.at != len(data):
        raise CborError("trailing bytes after the single item at offset %d" % d.at)
    return v


def dump_strict(value):
    """Re-emit STRICT JSON. Objects given as `Pairs`/`dict` keep order; `Pairs` keeps
    **duplicates**. Raises `CborError` for a value with no strict-JSON representation
    (`NaN`, or a non-string map key)."""
    out = []
    _dump(value, out)
    return "".join(out)


def _dump(v, out):
    if isinstance(v, Pairs):
        out.append("{")
        for i, (k, val) in enumerate(v):
            if i:
                out.append(",")
            if not isinstance(k, str):
                raise CborError("map key %r has no strict-JSON representation" % (k,))
            out.append(json.dumps(k, ensure_ascii=False))
            out.append(":")
            _dump(val, out)
        out.append("}")
    elif isinstance(v, list):
        out.append("[")
        for i, e in enumerate(v):
            if i:
                out.append(",")
            _dump(e, out)
        out.append("]")
    elif isinstance(v, dict):
        out.append("{")
        for i, (k, val) in enumerate(v.items()):
            if i:
                out.append(",")
            if not isinstance(k, str):
                raise CborError("map key %r has no strict-JSON representation" % (k,))
            out.append(json.dumps(k, ensure_ascii=False))
            out.append(":")
            _dump(val, out)
        out.append("}")
    elif isinstance(v, bool):
        out.append("true" if v else "false")
    elif v is None:
        out.append("null")
    elif isinstance(v, int):
        out.append(str(v))
    elif isinstance(v, float):
        if math.isnan(v):
            raise CborError("NaN has no strict-JSON representation")
        if math.isinf(v):
            out.append("1e999" if v > 0 else "-1e999")
        else:
            out.append(repr(v))
    elif isinstance(v, str):
        out.append(json.dumps(v, ensure_ascii=False))
    else:
        raise CborError("unsupported value %r" % (type(v),))


def host_class(v):
    """Coarse value class for cross-lane comparison (a conventional host model)."""
    if isinstance(v, bool):
        return "bool"
    if v is None:
        return "null"
    if isinstance(v, (Pairs, dict)):
        return "object"
    if isinstance(v, list):
        return "array"
    if isinstance(v, str):
        return "string"
    return "number"


def canon_value(v):
    """Canonical string for a scalar host value, matching the VOLE lane's canonical
    form (numbers: `str(int)` / `repr(float)`; bool/null named; strings verbatim)."""
    if isinstance(v, bool):
        return "true" if v else "false"
    if v is None:
        return "null"
    if isinstance(v, float):
        if math.isnan(v):
            return "nan"
        return repr(v)
    if isinstance(v, int):
        return str(v)
    if isinstance(v, str):
        return v
    return json.dumps(v, sort_keys=True, ensure_ascii=False, default=str)
