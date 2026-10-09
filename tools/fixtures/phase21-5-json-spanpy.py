#!/usr/bin/env python3
# Phase 21.5.3 (FIX 1c) — the span-preserving conventional baseline.
#
# This is the strongest realistic competitor the external review asked for: a
# **pure-Python (stdlib only)** parse that RETAINS the source bytes and records
# every token's exact byte span. Unlike the SQLite lane it is span- *and*
# spelling-preserving, so it can answer a node's exact source span, its exact raw
# token bytes, and the true duplicate-key member count — the three questions VOLE
# previously "won" only because its comparator was weak.
#
# It answers the SAME Q1–Q8 envelope grammar as the SQLite lane (so the court can
# compare like with like), reading a persisted span table rather than re-parsing
# per query. `build` parses and writes `<db>/raw.bin` (the retained source),
# `<db>/model.json` (the span table) and `<db>/meta.json`; `query`/`session`/
# `materialize` read them. Everything is stdlib (`json` only for decoding a string
# token and serializing the span table).
#
#   build       --source FILE --db DIR
#   query       --db DIR --q Qn --plan JSON --out FILE
#   session     --db DIR --queries Q1,Q2,... --plan JSON --out FILE
#   materialize --db DIR --out FILE

import argparse
import hashlib
import json
import os
import sys
import time

# Kind codes mirror `src/adapter/json.rs` (K_OBJECT..K_NULL).
K_OBJECT, K_ARRAY, K_STRING, K_NUMBER, K_TRUE, K_FALSE, K_NULL = range(7)
KIND_NAME = {
    K_OBJECT: "object", K_ARRAY: "array", K_STRING: "string", K_NUMBER: "number",
    K_TRUE: "true", K_FALSE: "false", K_NULL: "null",
}


def sha256_hex(b):
    return hashlib.sha256(b).hexdigest()


def now_us():
    return int(time.monotonic() * 1_000_000)


def envelope(q, value=None, *, declined=False, code=None, reason="", detail=None):
    e = {"q": q, "lane": "spanpy", "declined": bool(declined), "native": True,
         "value": None if declined else value, "detail": detail or {}}
    e["decline"] = {"code": code, "detail": reason} if declined else None
    return e


# --- a small, exact scanner over the raw source bytes ------------------------


class Scanner:
    """A recursive-descent JSON scanner recording (kind, start, end) for every
    token, plus object member order and duplicate keys. Numbers/escapes are kept
    as raw source tokens (never round-tripped through a float)."""

    def __init__(self, src):
        self.b = src
        self.n = len(src)
        self.at = 0
        self.nodes = []  # [kind, start, end, decoded_string_or_None, children]

    def _add(self, kind, start, end, dec=None, children=None):
        self.nodes.append([kind, start, end, dec, children or []])
        return len(self.nodes) - 1

    def _ws(self):
        b, n = self.b, self.n
        while self.at < n and b[self.at] in b" \t\r\n":
            self.at += 1

    def _err(self, msg):
        raise ValueError("%s at byte %d" % (msg, self.at))

    def parse(self):
        self._ws()
        root = self.value()
        self._ws()
        if self.at != self.n:
            self._err("trailing data after JSON value")
        return root

    def value(self):
        if self.at >= self.n:
            self._err("unexpected end of input")
        c = self.b[self.at]
        if c == 0x7B:  # {
            return self.object()
        if c == 0x5B:  # [
            return self.array()
        if c == 0x22:  # "
            return self.string()
        if c == 0x74:  # t
            return self.literal(b"true", K_TRUE)
        if c == 0x66:  # f
            return self.literal(b"false", K_FALSE)
        if c == 0x6E:  # n
            return self.literal(b"null", K_NULL)
        return self.number()

    def literal(self, word, kind):
        start = self.at
        if self.b[start:start + len(word)] != word:
            self._err("bad literal")
        self.at += len(word)
        return self._add(kind, start, self.at)

    def number(self):
        start = self.at
        b, n = self.b, self.n
        if self.at < n and b[self.at] == 0x2D:  # -
            self.at += 1
        while self.at < n and 0x30 <= b[self.at] <= 0x39:
            self.at += 1
        if self.at < n and b[self.at] == 0x2E:  # .
            self.at += 1
            while self.at < n and 0x30 <= b[self.at] <= 0x39:
                self.at += 1
        if self.at < n and b[self.at] in (0x65, 0x45):  # e/E
            self.at += 1
            if self.at < n and b[self.at] in (0x2B, 0x2D):
                self.at += 1
            while self.at < n and 0x30 <= b[self.at] <= 0x39:
                self.at += 1
        if self.at == start:
            self._err("invalid number")
        return self._add(K_NUMBER, start, self.at)

    def string(self):
        start = self.at
        self.at += 1  # opening quote
        b, n = self.b, self.n
        while self.at < n:
            c = b[self.at]
            if c == 0x22:  # closing quote
                self.at += 1
                tok = self.b[start:self.at]
                dec = json.loads(tok.decode("utf-8"))
                return self._add(K_STRING, start, self.at, dec)
            if c == 0x5C:  # backslash: skip the escape's second byte
                self.at += 2
                continue
            self.at += 1
        self._err("unterminated string")

    def object(self):
        start = self.at
        self.at += 1  # {
        children = []
        self._ws()
        if self.at < self.n and self.b[self.at] == 0x7D:  # }
            self.at += 1
            return self._add(K_OBJECT, start, self.at, None, children)
        while True:
            self._ws()
            if self.at >= self.n or self.b[self.at] != 0x22:
                self._err("expected object key")
            children.append(self.string())
            self._ws()
            if self.at >= self.n or self.b[self.at] != 0x3A:  # :
                self._err("expected ':'")
            self.at += 1
            self._ws()
            children.append(self.value())
            self._ws()
            if self.at >= self.n:
                self._err("unterminated object")
            d = self.b[self.at]
            if d == 0x2C:  # ,
                self.at += 1
                continue
            if d == 0x7D:  # }
                self.at += 1
                return self._add(K_OBJECT, start, self.at, None, children)
            self._err("expected ',' or '}'")

    def array(self):
        start = self.at
        self.at += 1  # [
        children = []
        self._ws()
        if self.at < self.n and self.b[self.at] == 0x5D:  # ]
            self.at += 1
            return self._add(K_ARRAY, start, self.at, None, children)
        while True:
            self._ws()
            children.append(self.value())
            self._ws()
            if self.at >= self.n:
                self._err("unterminated array")
            d = self.b[self.at]
            if d == 0x2C:  # ,
                self.at += 1
                continue
            if d == 0x5D:  # ]
                self.at += 1
                return self._add(K_ARRAY, start, self.at, None, children)
            self._err("expected ',' or ']'")


# --- model helpers -----------------------------------------------------------


def _escape_seg(s):
    return s.replace("~", "~0").replace("/", "~1")


def _unescape_seg(s):
    out = []
    i = 0
    while i < len(s):
        if s[i] == "~" and i + 1 < len(s):
            if s[i + 1] == "0":
                out.append("~")
                i += 2
                continue
            if s[i + 1] == "1":
                out.append("/")
                i += 2
                continue
        out.append(s[i])
        i += 1
    return "".join(out)


def parse_pointer(pointer):
    if pointer == "":
        return []
    if not pointer.startswith("/"):
        raise ValueError("pointer must be empty or start with '/'")
    return [_unescape_seg(raw) for raw in pointer.split("/")[1:]]


def pointer_of(path, leaf_key=None):
    s = "".join("/" + seg for seg in path)
    if leaf_key is not None:
        s += "/" + _escape_seg(leaf_key)
    return s


def node_of(model, index):
    return model[index]


def token_bytes(raw, node):
    return raw[node[1]:node[2]]


def render(model, raw, index, out):
    node = model[index]
    kind, children = node[0], node[4]
    if kind == K_OBJECT:
        out.append("{")
        i = 0
        while i + 1 < len(children):
            if i > 0:
                out.append(",")
            render(model, raw, children[i], out)
            out.append(":")
            render(model, raw, children[i + 1], out)
            i += 2
        out.append("}")
    elif kind == K_ARRAY:
        out.append("[")
        for i, c in enumerate(children):
            if i > 0:
                out.append(",")
            render(model, raw, c, out)
        out.append("]")
    else:
        out.append(token_bytes(raw, node).decode("utf-8", "replace"))


def subtree_text(model, raw, index):
    out = []
    render(model, raw, index, out)
    return "".join(out)


def node_text(model, raw, index):
    node = model[index]
    if node[0] == K_STRING:
        return node[3]
    if node[0] in (K_OBJECT, K_ARRAY):
        return subtree_text(model, raw, index)
    return token_bytes(raw, node).decode("utf-8", "replace")


def resolve_pointer(model, raw, pointer, root):
    segs = parse_pointer(pointer)
    index = root
    matches = 1
    for seg in segs:
        node = model[index]
        kind, children = node[0], node[4]
        if kind == K_OBJECT:
            found = 0
            first = None
            i = 0
            while i + 1 < len(children):
                k = children[i]
                v = children[i + 1]
                i += 2
                if model[k][0] == K_STRING and model[k][3] == seg:
                    found += 1
                    if first is None:
                        first = v
            if first is None:
                raise KeyError("no member %r" % seg)
            index = first
            matches = found
        elif kind == K_ARRAY:
            if seg == "" or (len(seg) > 1 and seg[0] == "0") or not seg.isdigit():
                raise ValueError("JSON array segment %r is not a canonical index" % seg)
            idx = int(seg)
            if idx >= len(children):
                raise KeyError("array index out of range")
            index = children[idx]
            matches = 1
        else:
            raise KeyError("cannot index into a %s" % KIND_NAME[kind])
    return index, matches


def walk_find(model, index, path, is_value, pattern, out):
    node = model[index]
    kind, children = node[0], node[4]
    if kind == K_OBJECT:
        i = 0
        while i + 1 < len(children):
            k = children[i]
            v = children[i + 1]
            i += 2
            key = model[k][3]
            if pattern in key:
                out.append({"pointer": pointer_of(path, key), "role": "key", "text": key})
            path.append(_escape_seg(key))
            walk_find(model, v, path, True, pattern, out)
            path.pop()
    elif kind == K_ARRAY:
        for idx, c in enumerate(children):
            path.append(str(idx))
            walk_find(model, c, path, True, pattern, out)
            path.pop()
    elif kind == K_STRING and is_value:
        text = node[3]
        if pattern in text:
            out.append({"pointer": pointer_of(path, None), "role": "value", "text": text})


def find(model, raw, pattern, root):
    out = []
    walk_find(model, root, [], True, pattern, out)
    return out


# --- queries -----------------------------------------------------------------


def _load(db_dir):
    with open(os.path.join(db_dir, "raw.bin"), "rb") as f:
        raw = f.read()
    with open(os.path.join(db_dir, "model.json")) as f:
        model = json.load(f)
    return raw, model


def run_query(raw, model, q, plan):
    root = len(model) - 1  # the scanner appends the root last (post-order)
    if q == "Q1" or q == "Q5":
        ptr = plan["value_ptr"] if q == "Q1" else plan["array_ptr"]
        try:
            index, _ = resolve_pointer(model, raw, ptr, root)
        except (KeyError, ValueError) as e:
            return envelope(q, declined=True, code="missing", reason=str(e))
        return envelope(q, node_text(model, raw, index))
    if q == "Q2":
        try:
            index, _ = resolve_pointer(model, raw, plan["span_ptr"], root)
        except (KeyError, ValueError) as e:
            return envelope(q, declined=True, code="missing", reason=str(e))
        node = model[index]
        return envelope(q, [node[1], node[2]])
    if q == "Q3":
        try:
            index, _ = resolve_pointer(model, raw, plan["kind_ptr"], root)
        except (KeyError, ValueError) as e:
            return envelope(q, declined=True, code="missing", reason=str(e))
        return envelope(q, KIND_NAME[model[index][0]])
    if q == "Q4":
        try:
            _, matches = resolve_pointer(model, raw, plan["dup_ptr"], root)
        except (KeyError, ValueError):
            return envelope(q, {"exists": False, "duplicate_count": 0})
        return envelope(q, {"exists": True, "duplicate_count": matches})
    if q == "Q6":
        return envelope(q, {"length": len(raw), "sha256": sha256_hex(raw)})
    if q == "Q7":
        out = find(model, raw, plan["find_pat"], root)
        return envelope(q, out, detail={"count": len(out)})
    if q == "Q8":
        try:
            index, _ = resolve_pointer(model, raw, plan["token_ptr"], root)
        except (KeyError, ValueError) as e:
            return envelope(q, declined=True, code="missing", reason=str(e))
        b = token_bytes(raw, model[index])
        return envelope(q, {"sha256": sha256_hex(b), "len": len(b)})
    return envelope(q, declined=True, code="unknown-question", reason=q)


# --- CLI ---------------------------------------------------------------------


def build(source, db_dir):
    os.makedirs(db_dir, exist_ok=True)
    for name in ("raw.bin", "model.json", "meta.json"):
        try:
            os.remove(os.path.join(db_dir, name))
        except FileNotFoundError:
            pass
    t0 = now_us()
    with open(source, "rb") as f:
        raw = f.read()
    sc = Scanner(raw)
    sc.parse()
    model = sc.nodes
    extract_us = now_us() - t0
    with open(os.path.join(db_dir, "raw.bin"), "wb") as f:
        f.write(raw)
    with open(os.path.join(db_dir, "model.json"), "w") as f:
        json.dump(model, f, separators=(",", ":"))
    with open(os.path.join(db_dir, "meta.json"), "w") as f:
        json.dump({"doc_len": len(raw), "nodes": len(model),
                   "root": len(model) - 1}, f, sort_keys=True)
    print(json.dumps({"ok": True, "fmt": "json", "src_len": len(raw),
                      "nodes": len(model), "extract_us": extract_us}, sort_keys=True))
    return 0


def query(db_dir, q, plan, out):
    raw, model = _load(db_dir)
    env = run_query(raw, model, q, plan)
    env["q"] = q
    with open(out, "w") as f:
        json.dump(env, f, sort_keys=True)
    return 0


def session(db_dir, queries, plan, out):
    raw, model = _load(db_dir)
    batch = []
    for q in queries.split(","):
        q = q.strip()
        if not q:
            continue
        t0 = now_us()
        env = run_query(raw, model, q, plan)
        us = now_us() - t0
        env["q"] = q
        batch.append({"q": q, "us": us, "env": env})
    with open(out, "w") as f:
        json.dump({"batch": batch}, f, sort_keys=True)
    return 0


def materialize(db_dir, out):
    with open(os.path.join(db_dir, "raw.bin"), "rb") as f:
        raw = f.read()
    with open(out, "wb") as f:
        f.write(raw)
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    b = sub.add_parser("build")
    b.add_argument("--source", required=True)
    b.add_argument("--db", required=True)
    q = sub.add_parser("query")
    q.add_argument("--db", required=True)
    q.add_argument("--q", required=True)
    q.add_argument("--plan", default="{}")
    q.add_argument("--out", required=True)
    s = sub.add_parser("session")
    s.add_argument("--db", required=True)
    s.add_argument("--queries", required=True)
    s.add_argument("--plan", default="{}")
    s.add_argument("--out", required=True)
    m = sub.add_parser("materialize")
    m.add_argument("--db", required=True)
    m.add_argument("--out", required=True)
    ns = ap.parse_args(argv)
    if ns.cmd == "build":
        return build(ns.source, ns.db)
    if ns.cmd == "query":
        return query(ns.db, ns.q, json.loads(ns.plan), ns.out)
    if ns.cmd == "session":
        return session(ns.db, ns.queries, json.loads(ns.plan), ns.out)
    if ns.cmd == "materialize":
        return materialize(ns.db, ns.out)
    return 2


if __name__ == "__main__":
    sys.exit(main())
