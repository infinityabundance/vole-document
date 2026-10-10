#!/usr/bin/env python3
# Phase 21.19 (economic court) — a **vendored, self-contained pure-Python
# MessagePack decoder** (the MessagePack spec). No third-party code, no network
# (the container has none). It exists so the economic court has an honest,
# reproducible **conventional comparator**: a MessagePack -> host-value load.
#
# It decodes to a JSON-compatible host model (dict/list/int/float/str/bool/None) and
# therefore **drops** exactly the representation a conventional load cannot carry:
#
#   * `bin` vs `str` — a `bin` payload is decoded with latin-1 into the *same*
#     Python `str` type as a `str`, so the byte-vs-text distinction is
#     **conflated** (the defining MessagePack-vs-JSON gap);
#   * the exact format byte actually used, encoding width **and signedness**
#     (`0x17` vs `0xcc 0x17` vs `0xd0 0x17`) — every integer is an unbounded Python
#     `int`, so both are gone;
#   * float width (`float32` vs `float64`) — every float is a Python `float`
#     (IEEE-754 double), so the width is gone;
#   * extension type numbers — an `ext` is unwrapped to its payload (the type is
#     never kept);
#   * map key order + duplicate keys — an object becomes a `dict` (last wins) unless
#     `pairs=True`, in which case a `Pairs` list keeps order and duplicates;
#   * every exact source span, and the exact token spelling.
#
# A conventional load drops all of those, which is why the court's Lane C must
# decline typed on the questions that need them. This module deliberately does NOT
# hide that.
#
# Public API mirrors `cbor_min`:
#   loads(data, pairs=False) -> host value; raises MsgpackError on malformed input
#   dump_strict(value)       -> strict JSON text preserving duplicate object members
#   host_class(v)            -> coarse value class
#   canon_value(v)           -> canonical string for cross-lane value comparison
#   Pairs / MsgpackError

import json
import math
import struct

__all__ = ["MsgpackError", "Pairs", "loads", "dump_strict", "host_class", "canon_value"]

MAX_DEPTH = 256
MAX_ITEMS = 4_000_000
MAX_STR_BYTES = 64 * 1024 * 1024


class MsgpackError(ValueError):
    """A malformed MessagePack source, or a value with no strict-JSON form."""


class Pairs(list):
    """A MessagePack map: a list of ``(key, value)`` pairs (order + duplicates kept)."""


def _signed(u, bits):
    return u - (1 << bits) if u >= (1 << (bits - 1)) else u


class _Dec:
    def __init__(self, data, pairs):
        self.b = bytes(data)
        self.at = 0
        self.pairs = pairs
        self.items = 0

    def _err(self, msg):
        raise MsgpackError("%s at offset %d" % (msg, self.at))

    def _take(self, n):
        if n < 0 or self.at + n > len(self.b):
            self._err("truncated item")
        out = self.b[self.at:self.at + n]
        self.at += n
        return out

    def _u(self, n):
        return int.from_bytes(self._take(n), "big")

    def _str(self, n):
        if n > MAX_STR_BYTES:
            self._err("str is too long")
        data = self._take(n)
        try:
            return data.decode("utf-8")
        except UnicodeDecodeError:
            self._err("str payload is not valid UTF-8")

    def _bin(self, n):
        if n > MAX_STR_BYTES:
            self._err("bin is too long")
        # Byte-vs-text is CONFLATED (latin-1), exactly as a conventional load does.
        return self._take(n).decode("latin-1")

    def _ext(self, payload):
        # The extension type is dropped; only the payload survives.
        return payload.decode("latin-1")

    def _array(self, n, depth):
        if n > MAX_ITEMS:
            self._err("array is too long")
        return [self.item(depth + 1) for _ in range(n)]

    def _map(self, n, depth):
        if n > MAX_ITEMS:
            self._err("map is too long")
        pairs = Pairs()
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
            except TypeError:
                d[repr(k)] = v
        return d

    def item(self, depth):
        if depth > MAX_DEPTH:
            self._err("nesting too deep")
        self.items += 1
        if self.items > MAX_ITEMS:
            self._err("too many items")
        nb = self._u(1)
        if nb <= 0x7F:                      # positive fixint
            return nb
        if nb >= 0xE0:                      # negative fixint
            return nb - 256
        if 0x80 <= nb <= 0x8F:              # fixmap
            return self._map(nb & 0x0F, depth)
        if 0x90 <= nb <= 0x9F:              # fixarray
            return self._array(nb & 0x0F, depth)
        if 0xA0 <= nb <= 0xBF:              # fixstr
            return self._str(nb & 0x1F)
        if nb == 0xC0:                      # nil
            return None
        if nb == 0xC1:                      # never used
            self._err("reserved format byte 0xc1")
        if nb == 0xC2:
            return False
        if nb == 0xC3:
            return True
        if nb == 0xC4:
            return self._bin(self._u(1))
        if nb == 0xC5:
            return self._bin(self._u(2))
        if nb == 0xC6:
            return self._bin(self._u(4))
        if nb == 0xC7:                      # ext8
            n = self._u(1)
            t = self._u(1)
            return self._ext(self._take(n))
        if nb == 0xC8:                      # ext16
            n = self._u(2)
            t = self._u(1)
            return self._ext(self._take(n))
        if nb == 0xC9:                      # ext32
            n = self._u(4)
            t = self._u(1)
            return self._ext(self._take(n))
        if nb == 0xCA:
            return struct.unpack(">f", self._take(4))[0]
        if nb == 0xCB:
            return struct.unpack(">d", self._take(8))[0]
        if nb == 0xCC:
            return self._u(1)
        if nb == 0xCD:
            return self._u(2)
        if nb == 0xCE:
            return self._u(4)
        if nb == 0xCF:
            return self._u(8)
        if nb == 0xD0:
            return _signed(self._u(1), 8)
        if nb == 0xD1:
            return _signed(self._u(2), 16)
        if nb == 0xD2:
            return _signed(self._u(4), 32)
        if nb == 0xD3:
            return _signed(self._u(8), 64)
        if 0xD4 <= nb <= 0xD8:              # fixext1/2/4/8/16
            n = 1 << (nb - 0xD4)
            t = self._u(1)
            return self._ext(self._take(n))
        if nb == 0xD9:                      # str8
            return self._str(self._u(1))
        if nb == 0xDA:                      # str16
            return self._str(self._u(2))
        if nb == 0xDB:                      # str32
            return self._str(self._u(4))
        if nb == 0xDC:                      # array16
            return self._array(self._u(2), depth)
        if nb == 0xDD:                      # array32
            return self._array(self._u(4), depth)
        if nb == 0xDE:                      # map16
            return self._map(self._u(2), depth)
        if nb == 0xDF:                      # map32
            return self._map(self._u(4), depth)
        self._err("unknown format byte 0x%02x" % nb)


def loads(data, pairs=False):
    d = _Dec(data, pairs)
    v = d.item(0)
    if d.at != len(data):
        raise MsgpackError("trailing bytes after the single item at offset %d" % d.at)
    return v


def dump_strict(value):
    """Re-emit STRICT JSON. Objects given as `Pairs`/`dict` keep order; `Pairs` keeps
    **duplicates**. Raises `MsgpackError` for a value with no strict-JSON
    representation (`NaN`, or a non-string map key)."""
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
                raise MsgpackError("map key %r has no strict-JSON representation" % (k,))
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
                raise MsgpackError("map key %r has no strict-JSON representation" % (k,))
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
            raise MsgpackError("NaN has no strict-JSON representation")
        if math.isinf(v):
            out.append("1e999" if v > 0 else "-1e999")
        else:
            out.append(repr(v))
    elif isinstance(v, str):
        out.append(json.dumps(v, ensure_ascii=False))
    else:
        raise MsgpackError("unsupported value %r" % (type(v),))


def host_class(v):
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
