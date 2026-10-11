#!/usr/bin/env python3
# Phase 21.17 (economic court) — a **vendored, self-contained pure-Python JSON5 /
# JSONC loader**. No third-party code, no network (the container has none).
#
# It exists so the economic court has an honest, reproducible **conventional
# comparator**: a JSON5 -> host-value load. It parses the same grammar surface the
# shipped VOLE JSON5 adapter accepts (comments, unquoted IdentifierName keys,
# single-quoted strings, trailing commas, hex / leading-dot / trailing-dot /
# `Infinity` / `NaN` numbers, string continuations, the extra escapes, and the
# extended JSON5 whitespace set) and it *records the dialect* (`strict` / `jsonc` /
# `json5`) using the same rule as the adapter: a JSON5-only construct wins over a
# JSONC-only one (comments / trailing commas).
#
# A conventional load drops exact source spans, member order of duplicates, and the
# exact numeric/escape *spelling*; that is exactly why the court's Lane C must
# decline typed on those questions. This module deliberately does NOT hide that.
#
# It additionally provides `dump_strict`, which re-emits STRICT JSON while
# preserving duplicate object members and their order (used by the source-retaining
# SQLite lane to normalize JSON5 -> strict JSON before `json_extract`/`json_tree`).
#
# Public API:
#   loads(text)                -> host value (dict/list/str/int/float/bool/None)
#   parse(text, pairs=False)   -> (value, dialect)
#   loads_pairs(text)          -> object-preserving value (objects are `Pairs`)
#   dump_strict(value)         -> strict JSON text; raises on NaN (no JSON form)
#   canon_number(text)         -> canonical string for a JSON5 number token
#   Pairs                      -> list subclass marking an object (ordered pairs)
#   JSON5Error                 -> parse / normalization error

import json
import math

__all__ = [
    "JSON5Error", "Pairs", "parse", "loads", "loads_pairs", "dump_strict",
    "canon_number",
]


class JSON5Error(ValueError):
    """A malformed JSON5 source, or a value with no strict-JSON representation."""


class Pairs(list):
    """A JSON5 object: a list of ``(key, value)`` pairs (order + duplicates kept)."""


# --- whitespace -------------------------------------------------------------

def _ws_kind(ch):
    """0 = not whitespace, 1 = JSON whitespace, 2 = JSON5-only whitespace."""
    if ch in " \t\n\r":
        return 1
    if ch in "\v\f":
        return 2
    o = ord(ch)
    if (o in (0x00A0, 0xFEFF, 0x1680, 0x2028, 0x2029, 0x202F, 0x205F, 0x3000)
            or 0x2000 <= o <= 0x200A):
        return 2
    return 0


def _is_hex(s):
    return len(s) > 0 and all(c in "0123456789abcdefABCDEF" for c in s)


class _Parser:
    def __init__(self, text, pairs):
        self.s = text
        self.i = 0
        self.n = len(text)
        self.pairs = pairs
        self.uses_jsonc_only = False
        self.uses_json5 = False

    # -- helpers --
    def _err(self, msg):
        raise JSON5Error("%s at offset %d" % (msg, self.i))

    def _peek(self):
        return self.s[self.i] if self.i < self.n else ""

    def _skip_ws(self):
        while self.i < self.n:
            c = self.s[self.i]
            if c == "/" and self.i + 1 < self.n:
                d = self.s[self.i + 1]
                if d == "/":
                    self.uses_jsonc_only = True
                    self.i += 2
                    while self.i < self.n and self.s[self.i] not in "\n\r\u2028\u2029":
                        self.i += 1
                    continue
                if d == "*":
                    self.uses_jsonc_only = True
                    self.i += 2
                    while self.i + 1 < self.n and not (
                            self.s[self.i] == "*" and self.s[self.i + 1] == "/"):
                        self.i += 1
                    if self.i + 1 >= self.n:
                        self._err("unterminated block comment")
                    self.i += 2
                    continue
            k = _ws_kind(c)
            if k == 1:
                self.i += 1
                continue
            if k == 2:
                self.uses_json5 = True
                self.i += 1
                continue
            break

    # -- strings --
    def _string(self):
        quote = self.s[self.i]
        if quote == "'":
            self.uses_json5 = True
        self.i += 1
        out = []
        while True:
            if self.i >= self.n:
                self._err("unterminated string")
            c = self.s[self.i]
            if c == quote:
                self.i += 1
                return "".join(out)
            if c == "\\":
                self.i += 1
                if self.i >= self.n:
                    self._err("string ends inside an escape")
                e = self.s[self.i]
                if e == "\n":
                    self.i += 1
                    self.uses_json5 = True
                    continue
                if e == "\r":
                    self.i += 1
                    if self._peek() == "\n":
                        self.i += 1
                    self.uses_json5 = True
                    continue
                if e == "'":
                    self.i += 1
                    self.uses_json5 = True
                    out.append("'")
                    continue
                if e in ("b", "f", "n", "r", "t"):
                    out.append({"b": "\b", "f": "\f", "n": "\n",
                                "r": "\r", "t": "\t"}[e])
                    self.i += 1
                    continue
                if e in ('"', "\\", "/"):
                    out.append(e)
                    self.i += 1
                    continue
                if e == "0":
                    nxt = self.s[self.i + 1] if self.i + 1 < self.n else ""
                    if nxt.isdigit():
                        self._err("\\0 followed by a digit")
                    out.append("\0")
                    self.i += 1
                    self.uses_json5 = True
                    continue
                if e == "v":
                    out.append("\v")
                    self.i += 1
                    self.uses_json5 = True
                    continue
                if e == "x":
                    h = self.s[self.i + 1:self.i + 3]
                    if len(h) != 2 or not _is_hex(h):
                        self._err("bad \\x escape")
                    out.append(chr(int(h, 16)))
                    self.i += 3
                    self.uses_json5 = True
                    continue
                if e == "u":
                    h = self.s[self.i + 1:self.i + 5]
                    if len(h) != 4 or not _is_hex(h):
                        self._err("bad \\u escape")
                    cp = int(h, 16)
                    self.i += 5
                    if 0xD800 <= cp <= 0xDBFF and self.s[self.i:self.i + 2] == "\\u":
                        h2 = self.s[self.i + 2:self.i + 6]
                        if len(h2) == 4 and _is_hex(h2):
                            lo = int(h2, 16)
                            if 0xDC00 <= lo <= 0xDFFF:
                                cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00)
                                self.i += 6
                    out.append(chr(cp))
                    continue
                if e in ("\u2028", "\u2029"):
                    self.i += 1
                    self.uses_json5 = True
                    continue
                self._err("invalid string escape")
            o = ord(c)
            if o < 0x20:
                self._err("unescaped control character in string")
            if c in ("\u2028", "\u2029"):
                self._err("unescaped line terminator in string")
            out.append(c)
            self.i += 1

    # -- identifiers --
    def _identifier(self):
        c = self.s[self.i]
        if not (c == "$" or c == "_" or c.isalpha()):
            self._err("invalid identifier start")
        j = self.i
        self.i += 1
        while self.i < self.n:
            ch = self.s[self.i]
            if ch == "$" or ch == "_" or ch.isalnum() or ch in "\u200c\u200d":
                self.i += 1
            else:
                break
        return self.s[j:self.i]

    # -- numbers / keywords --
    def _number(self):
        start = self.i
        if self._peek() in "+-":
            if self._peek() == "+":
                self.uses_json5 = True
            self.i += 1
        c = self._peek()
        if c == "I":
            if self.s[self.i:self.i + 8] != "Infinity":
                self._err("bad number")
            self.i += 8
            self.uses_json5 = True
            return float("-inf") if self.s[start] == "-" else float("inf")
        if c == "N":
            if self.s[self.i:self.i + 3] != "NaN":
                self._err("bad number")
            self.i += 3
            self.uses_json5 = True
            return float("nan")
        if c == "0" and self.s[self.i + 1:self.i + 2] in ("x", "X"):
            self.i += 2
            j = self.i
            while self.i < self.n and self.s[self.i] in "0123456789abcdefABCDEF":
                self.i += 1
            if self.i == j:
                self._err("empty hexadecimal literal")
            self.uses_json5 = True
            v = int(self.s[j:self.i], 16)
            return -v if self.s[start] == "-" else v
        j = self.i
        while self.i < self.n and self.s[self.i].isdigit():
            self.i += 1
        int_digits = self.i > j
        saw_dot = False
        if self._peek() == ".":
            saw_dot = True
            self.i += 1
            k = self.i
            while self.i < self.n and self.s[self.i].isdigit():
                self.i += 1
            frac = self.i > k
            if not frac:
                if not int_digits:
                    self._err("number has no digits")
                self.uses_json5 = True            # trailing dot (`5.`)
            elif not int_digits:
                self.uses_json5 = True            # leading dot (`.5`)
        if self._peek() in ("e", "E"):
            self.i += 1
            if self._peek() in "+-":
                self.i += 1
            k = self.i
            while self.i < self.n and self.s[self.i].isdigit():
                self.i += 1
            if self.i == k:
                self._err("number exponent has no digits")
        if not int_digits and not saw_dot:
            self._err("number has no integer part")
        txt = self.s[start:self.i]
        if saw_dot or "e" in txt or "E" in txt:
            try:
                return float(txt)
            except ValueError:
                self._err("bad float")
        try:
            return int(txt)
        except ValueError:
            self._err("bad int")

    def _keyword(self):
        for kw, val in (("true", True), ("false", False), ("null", None)):
            if self.s.startswith(kw, self.i):
                self.i += len(kw)
                return val
        self._err("unexpected identifier %r" % self.s[self.i:self.i + 8])

    # -- values --
    def _value(self):
        c = self._peek()
        if c == "":
            self._err("unexpected end of input")
        if c == "{":
            return self._object()
        if c == "[":
            return self._array()
        if c in ('"', "'"):
            return self._string()
        if c in "+-." or c.isdigit():
            return self._number()
        if c in ("I", "N"):
            return self._number()
        if c in "tfn":
            return self._keyword()
        self._err("unexpected character %r" % c)

    def _object(self):
        self.i += 1
        pairs = Pairs()
        self._skip_ws()
        if self._peek() == "}":
            self.i += 1
            return pairs if self.pairs else {}
        while True:
            self._skip_ws()
            c = self._peek()
            if c in ('"', "'"):
                key = self._string()
            elif c and (c == "$" or c == "_" or c.isalpha()):
                key = self._identifier()
                self.uses_json5 = True
            else:
                self._err("object key must be an identifier or a string")
            self._skip_ws()
            if self._peek() != ":":
                self._err("expected ':' after object key")
            self.i += 1
            self._skip_ws()
            val = self._value()
            pairs.append((key, val))
            self._skip_ws()
            c = self._peek()
            if c == ",":
                self.i += 1
                self._skip_ws()
                if self._peek() == "}":
                    self.i += 1
                    self.uses_jsonc_only = True
                    break
            elif c == "}":
                self.i += 1
                break
            else:
                self._err("expected ',' or '}' in object")
        if self.pairs:
            return pairs
        d = {}
        for k, v in pairs:
            d[k] = v
        return d

    def _array(self):
        self.i += 1
        out = []
        self._skip_ws()
        if self._peek() == "]":
            self.i += 1
            return out
        while True:
            self._skip_ws()
            out.append(self._value())
            self._skip_ws()
            c = self._peek()
            if c == ",":
                self.i += 1
                self._skip_ws()
                if self._peek() == "]":
                    self.i += 1
                    self.uses_jsonc_only = True
                    break
            elif c == "]":
                self.i += 1
                break
            else:
                self._err("expected ',' or ']' in array")
        return out


def parse(text, pairs=False):
    """Parse one JSON5/JSONC value. Returns ``(value, dialect)`` where dialect is
    ``"strict"``, ``"jsonc"`` or ``"json5"``."""
    if isinstance(text, (bytes, bytearray)):
        text = bytes(text).decode("utf-8")
    p = _Parser(text, pairs)
    p._skip_ws()
    if p.i >= p.n:
        raise JSON5Error("empty input is not a JSON5 value")
    val = p._value()
    p._skip_ws()
    if p.i != p.n:
        raise JSON5Error("trailing bytes after the single JSON5 value at offset %d" % p.i)
    if p.uses_json5:
        dialect = "json5"
    elif p.uses_jsonc_only:
        dialect = "jsonc"
    else:
        dialect = "strict"
    return val, dialect


def loads(text):
    return parse(text, pairs=False)[0]


def loads_pairs(text):
    return parse(text, pairs=True)[0]


def dump_strict(value):
    """Re-emit STRICT JSON. Objects given as `Pairs` keep order and **duplicates**.
    Raises `JSON5Error` for a value with no strict-JSON representation (``NaN``)."""
    out = []
    _dump(value, out)
    return "".join(out)


def _dump(v, out):
    if isinstance(v, Pairs):
        out.append("{")
        for i, (k, val) in enumerate(v):
            if i:
                out.append(",")
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
    elif isinstance(v, bool):
        out.append("true" if v else "false")
    elif v is None:
        out.append("null")
    elif isinstance(v, int):
        out.append(str(v))
    elif isinstance(v, float):
        if math.isnan(v):
            raise JSON5Error("NaN has no strict-JSON representation")
        if math.isinf(v):
            out.append("1e999" if v > 0 else "-1e999")
        else:
            out.append(repr(v))
    elif isinstance(v, str):
        out.append(json.dumps(v, ensure_ascii=False))
    else:
        raise JSON5Error("unsupported value %r" % type(v))


def canon_number(text):
    """Canonical string for a JSON5 number token (``0xFF`` -> ``255``, ``.5`` ->
    ``0.5``, ``Infinity`` -> ``inf``), used to compare values across lanes."""
    v = loads(text)
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, float):
        if math.isnan(v):
            return "nan"
        return repr(v)
    return str(v)
