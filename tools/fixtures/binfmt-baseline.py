#!/usr/bin/env python3
# Phase 21.18 / 21.19 (economic courts) — the two conventional comparators for the
# binary structured-tree courts (CBOR, MessagePack). Runs on Python stdlib +
# `sqlite3` only (the pinned `doc-baseline` service; no third-party module, no
# network). Shared by both formats via `--format cbor|msgpack`.
#
#   * `sqlite` — a **source-retaining** baseline. It keeps the original bytes
#     verbatim (a `raw` BLOB, so it can answer `materialize` / byte authority, Q6)
#     AND an **extracted strict-JSON text** obtained by normalizing the binary source
#     with the vendored decoder (`<fmt>_min.loads(..., pairs=True)` +
#     `dump_strict`) — the pairs mode preserves member order and **duplicate keys**
#     so `json_tree` can enumerate them (Q4). Structure/kinds are answered with
#     SQLite's built-in JSON functions (`json_extract`/`json_type`/`json_tree`). A
#     source that has **no** strict-JSON representation (a map with a non-text key, or
#     a `NaN`) cannot be normalized at all, in which case every JSON question declines
#     typed (a real limitation, recorded, not papered over). SQLite preserves no exact
#     source span, encoding width/head byte, byte-vs-text kind, tag/ext type, float
#     width, or exact token spelling (Q2/Q8/Q9/Q10/Q11/Q12 all decline).
#   * `conv`    — a **conventional binary -> host-value load** (the vendored
#     decoder): it keeps only a derived host-value view. It drops the source bytes,
#     every source offset, the encoding width/signedness, the byte-vs-text kind,
#     duplicate keys, the tag/ext type, and float width, so it must decline typed on
#     all of those (Q2/Q4/Q6/Q8/Q9/Q10/Q11/Q12).
#
#   plan --format F [--corpus DIR]           (lane fixtures, controls, per-fixture plans)
#   build --format F --lane sqlite|conv --source FILE --out DIR
#   query --format F --lane ... --dir DIR --q Qn --plan JSON --out FILE
#   session --format F --lane ... --dir DIR --queries Q1,... --plan JSON --out FILE
#   materialize --format F --lane sqlite --dir DIR --out FILE
#   version --format F
#   aggregate --format F --raw DIR --campaign DIR --env ENV_JSON

import argparse
import hashlib
import json
import os
import re
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)

import cbor_min
import msgpack_min

SIMPLE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
IDX = re.compile(r"^(0|[1-9][0-9]*)$")

# --- per-format configuration ------------------------------------------------

FORMATS = {
    "cbor": {
        "mod": cbor_min,
        "pointer_flag": "cbor-pointer",
        "node_flag": "cbor-node",
        "find_flag": "cbor-find",
        "phase": "21.18",
        "width_field": "info",
        "type_field": "tag",
        "type_label": "tag",
        "float_label": "float width",
    },
    "msgpack": {
        "mod": msgpack_min,
        "pointer_flag": "msgpack-pointer",
        "node_flag": "msgpack-node",
        "find_flag": "msgpack-find",
        "phase": "21.19",
        "width_field": "head",
        "type_field": "ext_type",
        "type_label": "extension type",
        "float_label": "float width",
    },
}

LANE_FIXTURES = {
    "cbor": ["basic.cbor", "widths.cbor", "floats.cbor", "bytestext.cbor",
             "dupkeys.cbor", "tags.cbor", "indef.cbor", "nested.cbor",
             "mapkeys.cbor", "nonfinite.cbor", "large.cbor"],
    "msgpack": ["basic.msgpack", "widths.msgpack", "floats.msgpack",
                "bytestext.msgpack", "dupkeys.msgpack", "ext.msgpack",
                "map16.msgpack", "nested.msgpack", "intkeys.msgpack",
                "nonfinite.msgpack", "large.msgpack"],
}

# The detection-boundary controls and the format class each MUST receive, and the
# observations each must take. `opaque` = the common observation declines typed.
CONTROLS = {
    "cbor": {
        "strict.json": "json",
        "prose.txt": "opaque",
        "single.cbor": "opaque",
        "scalar.cbor": "opaque",
        "badmap.cbor": "opaque",
        "unterm.cbor": "opaque",
        "trailing.cbor": "opaque",
        "fixarray3.bin": "opaque",
        "fixmap2.bin": "opaque",
    },
    "msgpack": {
        "strict.json": "json",
        "control.cbor": "cbor",
        "prose.txt": "opaque",
        "single.msgpack": "opaque",
        "scalar.msgpack": "opaque",
        "badc1.msgpack": "opaque",
        "badmap.msgpack": "opaque",
        "trailing.msgpack": "opaque",
        "fixarray3.bin": "opaque",
        "fixmap2.bin": "opaque",
    },
}

# Per-fixture plan (FROZEN): pointers chosen to exercise each question on a scalar
# or the relevant node kind. `value_ptr`/`array_ptr` are scalars.
PLANS = {
    "cbor": {
        "basic.cbor": {"value_ptr": "/a", "span_ptr": "/b", "kind_ptr": "/b/1",
                       "dup_ptr": "/a", "array_ptr": "/b/1", "find_pat": "b",
                       "token_ptr": "/c", "width_ptr": "/a", "bytetext_ptr": "/a",
                       "tag_ptr": "/a", "float_ptr": "/a"},
        "widths.cbor": {"value_ptr": "/3", "span_ptr": "/1", "kind_ptr": "/2",
                        "dup_ptr": "/0", "array_ptr": "/1", "find_pat": "z",
                        "token_ptr": "/1", "width_ptr": "/1", "bytetext_ptr": "/0",
                        "tag_ptr": "/0", "float_ptr": "/0"},
        "floats.cbor": {"value_ptr": "/0", "span_ptr": "/2", "kind_ptr": "/1",
                        "dup_ptr": "/0", "array_ptr": "/2", "find_pat": "z",
                        "token_ptr": "/1", "width_ptr": "/1", "bytetext_ptr": "/0",
                        "tag_ptr": "/0", "float_ptr": "/0"},
        "bytestext.cbor": {"value_ptr": "/1", "span_ptr": "/0", "kind_ptr": "/1",
                           "dup_ptr": "/1", "array_ptr": "/1", "find_pat": "abc",
                           "token_ptr": "/1", "width_ptr": "/0", "bytetext_ptr": "/0",
                           "tag_ptr": "/0", "float_ptr": "/0"},
        "dupkeys.cbor": {"value_ptr": "/a", "span_ptr": "/a", "kind_ptr": "/a",
                         "dup_ptr": "/a", "array_ptr": "/a", "find_pat": "a",
                         "token_ptr": "/a", "width_ptr": "/a", "bytetext_ptr": "/a",
                         "tag_ptr": "/a", "float_ptr": "/a"},
        "tags.cbor": {"value_ptr": "/n", "span_ptr": "/t", "kind_ptr": "/n",
                      "dup_ptr": "/n", "array_ptr": "/n", "find_pat": "n",
                      "token_ptr": "/n", "width_ptr": "/n", "bytetext_ptr": "/n",
                      "tag_ptr": "/t", "float_ptr": "/n"},
        "indef.cbor": {"value_ptr": "/0", "span_ptr": "/1", "kind_ptr": "/0",
                       "dup_ptr": "/0", "array_ptr": "/0", "find_pat": "x",
                       "token_ptr": "/0", "width_ptr": "/0", "bytetext_ptr": "/0",
                       "tag_ptr": "/0", "float_ptr": "/0"},
        "nested.cbor": {"value_ptr": "/a/b/1", "span_ptr": "/a", "kind_ptr": "/c",
                        "dup_ptr": "/a", "array_ptr": "/a/b/2", "find_pat": "b",
                        "token_ptr": "/c", "width_ptr": "/a/b/0", "bytetext_ptr": "/c",
                        "tag_ptr": "/c", "float_ptr": "/c"},
        "mapkeys.cbor": {"value_ptr": "/n", "span_ptr": "/m", "kind_ptr": "/n",
                         "dup_ptr": "/n", "array_ptr": "/n", "find_pat": "n",
                         "token_ptr": "/n", "width_ptr": "/n", "bytetext_ptr": "/n",
                         "tag_ptr": "/n", "float_ptr": "/n"},
        "nonfinite.cbor": {"value_ptr": "/0", "span_ptr": "/2", "kind_ptr": "/1",
                           "dup_ptr": "/0", "array_ptr": "/2", "find_pat": "z",
                           "token_ptr": "/1", "width_ptr": "/0", "bytetext_ptr": "/0",
                           "tag_ptr": "/0", "float_ptr": "/0"},
        "large.cbor": {"value_ptr": "/5", "span_ptr": "/0", "kind_ptr": "/5",
                       "dup_ptr": "/5", "array_ptr": "/5", "find_pat": "z",
                       "token_ptr": "/5", "width_ptr": "/0", "bytetext_ptr": "/5",
                       "tag_ptr": "/5", "float_ptr": "/5"},
    },
    "msgpack": {
        "basic.msgpack": {"value_ptr": "/a/1", "span_ptr": "/a", "kind_ptr": "/b",
                          "dup_ptr": "/a", "array_ptr": "/a/2", "find_pat": "a",
                          "token_ptr": "/b", "width_ptr": "/a/1", "bytetext_ptr": "/b",
                          "tag_ptr": "/b", "float_ptr": "/b"},
        "widths.msgpack": {"value_ptr": "/4", "span_ptr": "/1", "kind_ptr": "/2",
                           "dup_ptr": "/0", "array_ptr": "/1", "find_pat": "z",
                           "token_ptr": "/1", "width_ptr": "/1", "bytetext_ptr": "/0",
                           "tag_ptr": "/0", "float_ptr": "/0"},
        "floats.msgpack": {"value_ptr": "/0", "span_ptr": "/1", "kind_ptr": "/1",
                           "dup_ptr": "/0", "array_ptr": "/1", "find_pat": "z",
                           "token_ptr": "/1", "width_ptr": "/0", "bytetext_ptr": "/0",
                           "tag_ptr": "/0", "float_ptr": "/0"},
        "bytestext.msgpack": {"value_ptr": "/1", "span_ptr": "/0", "kind_ptr": "/1",
                              "dup_ptr": "/1", "array_ptr": "/1", "find_pat": "abc",
                              "token_ptr": "/1", "width_ptr": "/0", "bytetext_ptr": "/0",
                              "tag_ptr": "/0", "float_ptr": "/0"},
        "dupkeys.msgpack": {"value_ptr": "/a", "span_ptr": "/a", "kind_ptr": "/a",
                            "dup_ptr": "/a", "array_ptr": "/a", "find_pat": "a",
                            "token_ptr": "/a", "width_ptr": "/a", "bytetext_ptr": "/a",
                            "tag_ptr": "/a", "float_ptr": "/a"},
        "ext.msgpack": {"value_ptr": "/1", "span_ptr": "/0", "kind_ptr": "/0",
                        "dup_ptr": "/0", "array_ptr": "/1", "find_pat": "z",
                        "token_ptr": "/1", "width_ptr": "/0", "bytetext_ptr": "/0",
                        "tag_ptr": "/0", "float_ptr": "/0"},
        "map16.msgpack": {"value_ptr": "/a", "span_ptr": "/a", "kind_ptr": "/a",
                          "dup_ptr": "/a", "array_ptr": "/a", "find_pat": "a",
                          "token_ptr": "/a", "width_ptr": "", "bytetext_ptr": "/a",
                          "tag_ptr": "/a", "float_ptr": "/a"},
        "nested.msgpack": {"value_ptr": "/a/b/1", "span_ptr": "/a", "kind_ptr": "/a/b",
                           "dup_ptr": "/a", "array_ptr": "/a/b/2", "find_pat": "b",
                           "token_ptr": "/a/b/0", "width_ptr": "/a/b/0",
                           "bytetext_ptr": "/a/b/0", "tag_ptr": "/a/b/0",
                           "float_ptr": "/a/b/0"},
        "intkeys.msgpack": {"value_ptr": "/n", "span_ptr": "/m", "kind_ptr": "/n",
                            "dup_ptr": "/n", "array_ptr": "/n", "find_pat": "n",
                            "token_ptr": "/n", "width_ptr": "/n", "bytetext_ptr": "/n",
                            "tag_ptr": "/n", "float_ptr": "/n"},
        "nonfinite.msgpack": {"value_ptr": "/0", "span_ptr": "/1", "kind_ptr": "/1",
                              "dup_ptr": "/0", "array_ptr": "/1", "find_pat": "z",
                              "token_ptr": "/1", "width_ptr": "/0", "bytetext_ptr": "/0",
                              "tag_ptr": "/0", "float_ptr": "/0"},
        "large.msgpack": {"value_ptr": "/5", "span_ptr": "/0", "kind_ptr": "/5",
                          "dup_ptr": "/5", "array_ptr": "/5", "find_pat": "z",
                          "token_ptr": "/5", "width_ptr": "/5", "bytetext_ptr": "/5",
                          "tag_ptr": "/5", "float_ptr": "/5"},
    },
}

FMT_BIN_EXT = {"cbor": ".cbor", "msgpack": ".msgpack"}


def cfg(fmt):
    return FORMATS[fmt]


def sha256_hex(b):
    return hashlib.sha256(b).hexdigest()


def now_us():
    return int(time.monotonic() * 1_000_000)


def envelope(q, value=None, *, declined=False, code=None, reason="", detail=None):
    e = {"q": q, "lane": "?", "declined": bool(declined), "native": True,
         "value": None if declined else value, "detail": detail or {}}
    e["decline"] = {"code": code, "detail": reason} if declined else None
    return e


# --- kind / value normalization ---------------------------------------------

JSON_TYPE_CLASS = {
    "text": "string", "integer": "number", "real": "number", "true": "bool",
    "false": "bool", "null": "null", "object": "object", "array": "array",
}


def canon_py(klass, v):
    if klass == "string":
        return v if isinstance(v, str) else str(v)
    if klass == "number":
        if isinstance(v, bool):
            return "true" if v else "false"
        if isinstance(v, float):
            import math
            if math.isnan(v):
                return "nan"
            return repr(v)
        return str(v)
    if klass == "bool":
        return "true" if v else "false"
    return str(v)


# --- pointer helpers (RFC 6901) ---------------------------------------------


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


def _escape_seg(s):
    return s.replace("~", "~0").replace("/", "~1")


def to_sqlite_path(pointer):
    segs = pointer.split("/")[1:] if pointer else []
    path = "$"
    for raw in segs:
        s = _unescape_seg(raw)
        if IDX.match(s):
            path += "[%s]" % s
        elif SIMPLE.match(s):
            path += "." + s
        else:
            path += '."' + s.replace('"', '""') + '"'
    return path


def fullkey_to_pointer(fk):
    out = ""
    s = fk
    i = 0
    n = len(s)
    while i < n:
        c = s[i]
        if c == ".":
            i += 1
            buf = ""
            if i < n and s[i] == '"':
                i += 1
                while i < n:
                    if s[i] == '"' and (i + 1 >= n or s[i + 1] != '"'):
                        break
                    if s[i] == '"' and i + 1 < n and s[i + 1] == '"':
                        buf += '"'
                        i += 2
                        continue
                    buf += s[i]
                    i += 1
                i += 1
            else:
                while i < n and s[i] not in ".[":
                    buf += s[i]
                    i += 1
            out += "/" + _escape_seg(buf)
        elif c == "[":
            i += 1
            buf = ""
            while i < n and s[i] != "]":
                buf += s[i]
                i += 1
            i += 1
            out += "/" + buf
        else:
            i += 1
    return out


def resolve_pointer(root, pointer):
    if pointer == "":
        return True, root
    if not pointer.startswith("/"):
        return False, None
    cur = root
    for raw in pointer.split("/")[1:]:
        seg = _unescape_seg(raw)
        if isinstance(cur, dict):
            if seg in cur:
                cur = cur[seg]
            else:
                return False, None
        elif isinstance(cur, list):
            if IDX.match(seg) and int(seg) < len(cur):
                cur = cur[int(seg)]
            else:
                return False, None
        else:
            return False, None
    return True, cur


# --- build -------------------------------------------------------------------


def build(fmt, lane, source_path, out_dir):
    os.makedirs(out_dir, exist_ok=True)
    mod = cfg(fmt)["mod"]
    with open(source_path, "rb") as f:
        raw = f.read()
    if lane == "sqlite":
        norm_ok = 1
        note = "ok"
        norm = None
        try:
            norm = mod.dump_strict(mod.loads(raw, pairs=True))
        except ValueError as e:
            norm_ok = 0
            note = str(e)
        db = os.path.join(out_dir, "x.sqlite")
        for suffix in ("", "-wal", "-shm", "-journal"):
            try:
                os.remove(db + suffix)
            except FileNotFoundError:
                pass
        con_sql = _sqlite()
        con = con_sql.connect(db)
        con.execute("PRAGMA journal_mode=DELETE")
        con.execute("CREATE TABLE doc(id INTEGER PRIMARY KEY, raw BLOB, j TEXT, "
                    "norm_ok INTEGER, note TEXT)")
        con.execute("INSERT INTO doc VALUES(1,?,?,?,?)",
                    (con_sql.Binary(raw), norm, norm_ok, note))
        con.commit()
        con.close()
        print(json.dumps({"ok": True, "lane": lane, "src_len": len(raw),
                          "norm_ok": norm_ok, "note": note}, sort_keys=True))
    else:  # conv
        ok = 1
        note = "ok"
        try:
            val = mod.loads(raw, pairs=False)
        except ValueError as e:
            val = None
            ok = 0
            note = str(e)
        with open(os.path.join(out_dir, "parsed.json"), "w") as f:
            json.dump({"ok": ok, "note": note, "value": val}, f, allow_nan=True)
        print(json.dumps({"ok": True, "lane": lane, "src_len": len(raw),
                          "parse_ok": ok, "note": note}, sort_keys=True))
    return 0


def _sqlite():
    import sqlite3
    return sqlite3


# --- queries -----------------------------------------------------------------


def _decl(q, code, reason):
    return envelope(q, declined=True, code=code, reason=reason)


def _sqlite_query(con, q, plan):
    row = con.execute("SELECT raw, j, norm_ok, note FROM doc WHERE id=1").fetchone()
    raw, j, norm_ok, note = row[0], row[1], row[2], row[3]
    if q == "Q6":
        return envelope(q, {"length": len(raw), "sha256": sha256_hex(raw)})
    if q in ("Q2", "Q8", "Q9", "Q10", "Q11", "Q12"):
        msg = {
            "Q2": "SQLite exposes no source byte span",
            "Q8": "SQLite re-serializes a value; the exact token spelling is not preserved",
            "Q9": "the encoding width/head byte is lost by the JSON normalization",
            "Q10": "the JSON normalization conflates byte and text strings",
            "Q11": "tag/extension type is not preserved by the JSON normalization",
            "Q12": "float width is lost by the JSON normalization",
        }[q]
        return _decl(q, "no-representation", msg)
    if not norm_ok and q in ("Q1", "Q3", "Q4", "Q5", "Q7"):
        return _decl(q, "no-strict-json-representation",
                     "the source cannot be normalized to strict JSON: %s" % note)
    if q in ("Q1", "Q5"):
        ptr = plan["value_ptr"] if q == "Q1" else plan["array_ptr"]
        path = to_sqlite_path(ptr)
        t = con.execute("SELECT json_type(?,?)", (j, path)).fetchone()[0]
        if t is None:
            return _decl(q, "missing", "no node at path %s" % ptr)
        klass = JSON_TYPE_CLASS.get(t, t)
        v = con.execute("SELECT json_extract(?,?)", (j, path)).fetchone()[0]
        if klass in ("object", "array"):
            return _decl(q, "not-a-scalar", "the plan targets a scalar")
        return envelope(q, {"class": klass, "value": canon_py(klass, v)},
                        detail={"json_type": t, "normalized": True})
    if q == "Q3":
        path = to_sqlite_path(plan["kind_ptr"])
        t = con.execute("SELECT json_type(?,?)", (j, path)).fetchone()[0]
        if t is None:
            return _decl(q, "missing", "no node at path")
        return envelope(q, JSON_TYPE_CLASS.get(t, t), detail={"json_type": t})
    if q == "Q4":
        path = to_sqlite_path(plan["dup_ptr"])
        n = con.execute("SELECT count(*) FROM json_tree(?) WHERE fullkey = ?",
                        (j, path)).fetchone()[0]
        return envelope(q, {"exists": n > 0, "duplicate_count": n},
                        detail={"method": "json_tree fullkey count; duplicates enumerated"})
    if q == "Q7":
        pat = plan["find_pat"]
        like = "%" + pat + "%"
        rows = con.execute(
            "SELECT fullkey, key, type, value FROM json_tree(?) "
            "WHERE (key IS NOT NULL AND key LIKE ?) "
            "OR (type = 'text' AND value IS NOT NULL AND value LIKE ?)",
            (j, like, like)).fetchall()
        out = []
        for fullkey, key, typ, value in rows:
            ptr = fullkey_to_pointer(fullkey)
            if key is not None and pat in str(key):
                out.append({"pointer": ptr, "role": "key", "text": str(key)})
            if typ == "text" and value is not None and pat in str(value):
                out.append({"pointer": ptr, "role": "value", "text": str(value)})
        out.sort(key=lambda m: (m["pointer"], m["role"], m["text"]))
        return envelope(q, out, detail={"count": len(out)})
    return _decl(q, "unknown-question", q)


def _conv_load(d):
    with open(os.path.join(d, "parsed.json")) as f:
        obj = json.load(f)
    return obj.get("ok", 0), obj.get("value"), obj.get("note", "")


def _conv_find(node, pat, ptr, out):
    if isinstance(node, dict):
        for k, v in node.items():
            if isinstance(k, str) and pat in k:
                out.append({"pointer": ptr + "/" + _escape_seg(k), "role": "key", "text": k})
            _conv_find(v, pat, ptr + "/" + _escape_seg(str(k)), out)
    elif isinstance(node, list):
        for i, v in enumerate(node):
            _conv_find(v, pat, ptr + "/" + str(i), out)
    elif isinstance(node, str):
        if pat in node:
            out.append({"pointer": ptr, "role": "value", "text": node})


def _open_sqlite(d):
    return _sqlite().connect(os.path.join(d, "x.sqlite"))


def run_query(fmt, lane, d, q, plan, con=None, loaded=None):
    mod = cfg(fmt)["mod"]
    if lane == "sqlite":
        own = con is None
        if own:
            con = _open_sqlite(d)
        try:
            env = _sqlite_query(con, q, plan)
        finally:
            if own:
                con.close()
    else:
        if loaded is None:
            loaded = _conv_load(d)
        env = _conv_q(mod, loaded[1], q, plan)
    env["lane"] = lane
    return env


def _conv_q(mod, root, q, plan):
    if q in ("Q1", "Q5"):
        aptr = plan["value_ptr"] if q == "Q1" else plan["array_ptr"]
        found, v = resolve_pointer(root, aptr)
        if not found:
            return _decl(q, "missing", "no node at path %s" % aptr)
        klass = mod.host_class(v)
        if klass in ("object", "array"):
            return _decl(q, "not-a-scalar", "the plan targets a scalar")
        return envelope(q, {"class": klass, "value": mod.canon_value(v)},
                        detail={"host-value": True})
    if q == "Q2":
        return _decl(q, "no-source-span",
                     "a conventional binary->host load keeps no source span")
    if q == "Q3":
        found, v = resolve_pointer(root, plan["kind_ptr"])
        if not found:
            return _decl(q, "missing", "no node at path")
        return envelope(q, mod.host_class(v))
    if q == "Q4":
        return _decl(q, "duplicates-collapsed",
                     "a conventional object load collapses duplicate keys (last wins)")
    if q == "Q6":
        return _decl(q, "no-source-bytes",
                     "a conventional binary->host load does not retain the source bytes")
    if q == "Q7":
        out = []
        _conv_find(root, plan["find_pat"], "", out)
        out.sort(key=lambda m: (m["pointer"], m["role"], m["text"]))
        return envelope(q, out, detail={"count": len(out)})
    if q in ("Q8", "Q9", "Q10", "Q11", "Q12"):
        msg = {
            "Q8": "a conventional load yields host values, not the exact source token",
            "Q9": "the encoding width/head byte is lost",
            "Q10": "the conventional load conflates byte and text strings",
            "Q11": "the tag/extension type is discarded by a conventional load",
            "Q12": "the load yields host floats; the encoded width is lost",
        }[q]
        return _decl(q, "no-representation", msg)
    return _decl(q, "unknown-question", q)


def query(fmt, lane, d, q, plan, out):
    env = run_query(fmt, lane, d, q, plan)
    with open(out, "w") as f:
        json.dump(env, f, sort_keys=True)
    return 0


def session(fmt, lane, d, queries, plan, out):
    mod = cfg(fmt)["mod"]
    con = _open_sqlite(d) if lane == "sqlite" else None
    loaded = None if lane == "sqlite" else _conv_load(d)
    batch = []
    try:
        for q in queries.split(","):
            q = q.strip()
            if not q:
                continue
            t0 = now_us()
            env = run_query(fmt, lane, d, q, plan, con=con, loaded=loaded)
            dt = now_us() - t0
            batch.append({"q": q, "us": dt, "declined": env["declined"]})
    finally:
        if con is not None:
            con.close()
    with open(out, "w") as f:
        json.dump({"batch": batch}, f, sort_keys=True)
    return 0


def materialize(fmt, lane, d, out):
    con = _open_sqlite(d)
    try:
        raw = con.execute("SELECT raw FROM doc WHERE id=1").fetchone()[0]
    finally:
        con.close()
    with open(out, "wb") as f:
        f.write(bytes(raw))
    return 0


def version(fmt):
    con_sql = _sqlite()
    print(json.dumps({"module": "sqlite3", "sqlite_version": con_sql.sqlite_version,
                      "format": fmt, "decoder": "vendored-pure-python"}, sort_keys=True))
    return 0


def plan_dump(fmt):
    print(json.dumps({
        "format": fmt,
        "lane_fixtures": LANE_FIXTURES[fmt],
        "controls": CONTROLS[fmt],
        "plans": PLANS[fmt],
        "pointer_flag": cfg(fmt)["pointer_flag"],
        "node_flag": cfg(fmt)["node_flag"],
        "find_flag": cfg(fmt)["find_flag"],
    }, sort_keys=True))
    return 0


# --- aggregate ---------------------------------------------------------------

QS = ["Q1", "Q2", "Q3", "Q4", "Q5", "Q6", "Q7", "Q8", "Q9", "Q10", "Q11", "Q12"]
TIE = 0.10


def _load_p19():
    import importlib.util
    path = os.path.join(HERE, "phase19-repeat.py")
    spec = importlib.util.spec_from_file_location("phase19_repeat", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def read_tsv(path):
    if not os.path.exists(path):
        return []
    with open(path) as fh:
        hdr = fh.readline().rstrip("\n").split("\t")
        rows = []
        for line in fh:
            if not line.strip():
                continue
            vals = line.rstrip("\n").split("\t")
            if len(vals) != len(hdr):
                continue
            rows.append(dict(zip(hdr, vals)))
        return rows


def compare(q, va, vb):
    if va is None or vb is None:
        return "missing", "no envelope"
    if va.get("declined") and vb.get("declined"):
        return "both-decline", "both typed declines"
    if va.get("declined") or vb.get("declined"):
        who = "vole" if va.get("declined") else vb.get("lane", "comparator")
        return "capability-gap", "%s declines" % who
    a, b = va.get("value"), vb.get("value")
    if q in ("Q1", "Q5"):
        if isinstance(a, dict) and isinstance(b, dict):
            ok = a.get("class") == b.get("class") and a.get("value") == b.get("value")
            return ("equal" if ok else "mismatch"), "class+canonical value"
        return "shape", "not dict"
    if q in ("Q4", "Q6", "Q8"):
        if isinstance(a, dict) and isinstance(b, dict):
            return ("equal" if a == b else "mismatch"), "dict"
        return "shape", "not dict"
    if q == "Q2":
        return ("equal" if list(a or []) == list(b or []) else "mismatch"), "span"
    if q in ("Q3", "Q9", "Q10", "Q11", "Q12"):
        return ("equal" if str(a) == str(b) else "mismatch"), "scalar"
    if q == "Q7":
        def norm(m):
            return (m.get("pointer"), m.get("role"), m.get("text"))
        return ("equal" if sorted(map(norm, a or [])) == sorted(map(norm, b or []))
                else "mismatch"), "match-set"
    return ("equal" if a == b else "mismatch"), "default"


def qdesc(fmt):
    tl = cfg(fmt)["type_label"]
    return {
        "Q1": ("a scalar at a pointer (class + canonical value)",
               "`json_extract` value (normalized)", "host value"),
        "Q2": ("the exact source span of a node",
               "no source span exists -> typed decline", "no source span -> typed decline"),
        "Q3": ("a node's value class (number/string/bool/null/array/object)",
               "`json_type` (normalized)", "host-value class"),
        "Q4": ("key existence + duplicate-key count",
               "`json_tree` fullkey count (duplicates **enumerated**)",
               "duplicate keys collapsed -> typed decline"),
        "Q5": ("an array element value at an index",
               "`json_extract` at the index (normalized)", "host value"),
        "Q6": ("`materialize --exact` (byte-authority)",
               "retained raw BLOB (byte-authority)", "no source bytes -> typed decline"),
        "Q7": ("lexical find over text keys/values (with spans)",
               "`json_tree` scan over keys/strings (no spans)",
               "host-structure walk (no spans)"),
        "Q8": ("the exact raw token bytes at a pointer",
               "`json()` re-serialization (spelling lost) -> typed decline",
               "host value (token lost) -> typed decline"),
        "Q9": ("the exact encoding width/format byte actually used",
               "normalized to JSON numbers -> typed decline",
               "host integers unmarshal -> typed decline"),
        "Q10": ("the byte-vs-text kind (bin/bytes vs str/text)",
                "byte/text conflated by the JSON normalization -> typed decline",
                "byte/text conflated by the load -> typed decline"),
        "Q11": ("the %s of the node (preserved, never resolved)" % tl,
                "not preserved by the JSON normalization -> typed decline",
                "discarded by the load -> typed decline"),
        "Q12": ("the %s (half/single/double)" % cfg(fmt)["float_label"],
                "width lost by the JSON normalization -> typed decline",
                "host float; width lost -> typed decline"),
    }


def aggregate(fmt, raw, campaign, env_path=None):
    P19 = _load_p19()
    B = 10000
    SEED = int(cfg(fmt)["phase"].replace(".", "")) * 100 + 21
    import statistics

    env = {}
    if env_path and os.path.exists(env_path):
        with open(env_path) as f:
            try:
                env = json.load(f)
            except ValueError:
                env = {}

    docs = read_tsv(os.path.join(raw, "fixtures.tsv"))
    fixtures = [r["fixture"] for r in docs]
    lanes = ["vole", "sqlite", "conv"]
    comparators = ["sqlite", "conv"]

    build_rows = read_tsv(os.path.join(raw, "build.tsv"))
    storage_rows = read_tsv(os.path.join(raw, "storage.tsv"))
    cold_rows = read_tsv(os.path.join(raw, "cold.tsv"))
    warm_rows = read_tsv(os.path.join(raw, "warm.tsv"))
    exact_rows = read_tsv(os.path.join(raw, "exact.tsv"))

    def best_by_fixture(rows, lane, metric="us"):
        out = {}
        for r in rows:
            if r["lane"] != lane or r.get("rc") != "0":
                continue
            fx = r["fixture"]
            v = int(r[metric])
            if fx not in out or v < out[fx]:
                out[fx] = v
        return out

    build_us = {lane: best_by_fixture(build_rows, lane) for lane in lanes}
    store_bytes = {lane: {} for lane in lanes}
    store_files = {lane: {} for lane in lanes}
    for r in storage_rows:
        store_bytes[r["lane"]][r["fixture"]] = int(r["bytes"])
        store_files[r["lane"]][r["fixture"]] = int(r["files"])

    def cold_by_fixture(lane):
        perrep = {}
        for r in cold_rows:
            if r["lane"] != lane or r.get("rc") != "0":
                continue
            key = (r["fixture"], r["rep"])
            perrep[key] = perrep.get(key, 0) + int(r["us"])
        out = {}
        for (fx, rep), v in perrep.items():
            if fx not in out or v < out[fx]:
                out[fx] = v
        return out

    cold_us = {lane: cold_by_fixture(lane) for lane in lanes}
    warm_us = {lane: {} for lane in lanes}
    warm_perrep = {lane: {} for lane in lanes}
    for r in warm_rows:
        if r.get("rc") != "0":
            continue
        fx, lane, rep = r["fixture"], r["lane"], r["rep"]
        warm_perrep[lane][(fx, rep)] = warm_perrep[lane].get((fx, rep), 0) + int(r["us"])
    for lane in lanes:
        for (fx, rep), v in warm_perrep[lane].items():
            if fx not in warm_us[lane] or v < warm_us[lane][fx]:
                warm_us[lane][fx] = v

    qanswers = {}
    for fx in fixtures:
        for q in QS:
            row = {}
            for lane in lanes:
                p = os.path.join(raw, "qanswers", "%s.%s.%s.json" % (fx, q, lane))
                try:
                    with open(p) as f:
                        row[lane] = json.load(f)
                except (OSError, ValueError):
                    row[lane] = None
            qanswers[(fx, q)] = row

    equiv = {}
    for fx in fixtures:
        for q in QS:
            row = qanswers.get((fx, q), {})
            for comp in comparators:
                res, _ = compare(q, row.get("vole"), row.get(comp))
                equiv.setdefault(q, {}).setdefault(comp, {}).setdefault(res, 0)
                equiv[q][comp][res] += 1

    exact_ok = sum(1 for r in exact_rows if r.get("vole_ok") in ("1", "true"))
    exact_n = len(exact_rows)
    verdict = "PASS" if exact_ok == exact_n and exact_n > 0 else "FAIL"

    FMT = fmt.upper()
    lines = []
    lines.append("# Phase %s — %s economic court" % (cfg(fmt)["phase"], FMT))
    lines.append("")
    lines.append("**Question.** Against two conventional comparators — a "
                 "source-retaining SQLite store that must first normalize the binary "
                 "source to *strict* JSON, and a conventional %s -> host-value load — "
                 "on contract-equivalent terms, can VOLE answer the same twelve "
                 "questions (Q1–Q12) it can answer, while closing the original %s "
                 "byte-exactly — and does it add value by **preserving representation** "
                 "(source spans, the exact encoding width/format byte, byte-vs-text, "
                 "duplicate keys, tag/extension type, and float width)?" % (FMT, FMT))
    lines.append("")
    lines.append("**Method.** A deterministic self-authored %s corpus "
                 "(`tools/fixtures/make-%s.py --corpus`) is regenerated at court time; "
                 "each fixture is ingested by **three lanes** (VOLE field CLI with "
                 "`--profile runtime --packed`; a source-retaining SQLite baseline that "
                 "keeps the raw bytes and queries `json_extract`/`json_type`/`json_tree` "
                 "on a binary->strict-JSON normalization; and a conventional "
                 "binary->host-value load via a vendored pure-Python decoder), Q1–Q12 "
                 "are asked of each, and build/storage/cold/warm are measured. Persistent "
                 "bytes are the **sum of regular-file sizes** (`find -type f -printf "
                 "'%%s'`), never `du -sb` (ADR-0049). Wall times are **microseconds "
                 "(`us`)**." % (FMT, fmt))
    lines.append("")
    lines.append("Corpus: **%d fixtures**; lanes **%s**; questions **Q1–Q12**; "
                 "bootstrap **%d resamples, seed %d**, cluster-resampled by fixture; "
                 "tie band **+/-%d%%**."
                 % (len(fixtures), ", ".join(lanes), B, SEED, int(TIE * 100)))
    lines.append("")
    lines.append("## Pre-registered hypotheses")
    lines.append("")
    lines.append("- **H1 (byte-exactness, or FAIL).** For every fixture — including the "
                 "malformed/Opaque controls — VOLE `materialize --exact == source` "
                 "(length + SHA-256 + `cmp`) after the source file AND the standalone "
                 "descriptor are deleted, in a fresh process. The court FAILS unless "
                 "this is 100 %.")
    lines.append("- **H2 (contract questions answered where possible).** All three lanes "
                 "answer the same Q1–Q12 where their model permits: the coarse value "
                 "questions (Q1/Q3/Q5), the duplicate-key count (Q4), byte authority "
                 "(Q6), and lexical find (Q7).")
    lines.append("- **H3 (typed declines / pinned boundaries).** Every question a lane "
                 "cannot answer is a TYPED decline (rc 6, never a silent empty answer); "
                 "a malformed pointer is a usage error (rc 2); the detection boundaries "
                 "(strict JSON, the sibling binary format, lone scalars, malformed and "
                 "ambiguous inputs, prose) stay pinned.")
    lines.append("- **H4 (economics, ADR-0054).** build/storage/cold/warm are measured "
                 "per lane with a named estimator: the paired per-fixture ratio, "
                 "summarised by median and geometric mean with a fixed-seed, "
                 "fixture-clustered bootstrap **95 % CI**; the ratio-of-sums is "
                 "reported **separately** and named as such; every raw sample is "
                 "retained.")
    lines.append("")
    prof = (env or {}).get("profile", "unknown")
    sub = (env or {}).get("vole_substrate", "unknown")
    bin_label = (env or {}).get("bin", "?")
    lines.append("VOLE lane: **%s** profile (`%s`); substrate: **%s**. The comparators "
                 "(SQLite C + Python, and the pure-Python load) are unaffected by the "
                 "Rust profile while the entropyfs build is not, so the release default "
                 "keeps the comparison fair to VOLE." % (prof, bin_label, sub))
    lines.append("")
    lines.append("## Verdict")
    lines.append("")
    lines.append("- **VOLE exactness (Q6): %d/%d byte-exact** (length + SHA-256 + "
                 "`cmp`, after source + descriptor deletion in a fresh process)."
                 % (exact_ok, exact_n))
    lines.append("- **COURT VERDICT: %s** (fails unless exactness is 100%% on the VOLE "
                 "lane for every fixture)." % verdict)
    lines.append("")

    lines.append("## Build + storage (per lane, per fixture)")
    lines.append("")
    lines.append("Time columns are **microseconds (`us`)**.")
    lines.append("")
    lines.append("| fixture | src B | " + " | ".join("%s build us" % l for l in lanes) +
                 " | " + " | ".join("%s B" % l for l in lanes) + " |")
    lines.append("|---|---:|" + "".join("---:|" for _ in lanes) +
                 "".join("---:|" for _ in lanes))
    for r in docs:
        fx = r["fixture"]
        cells = [r["src_bytes"]]
        for lane in lanes:
            cells.append(str(build_us[lane].get(fx, "-")))
        for lane in lanes:
            cells.append(str(store_bytes[lane].get(fx, "-")))
        lines.append("| " + fx + " | " + " | ".join(cells) + " |")
    lines.append("")
    lines.append("`build us` is the best-of-N (min) of the retained repetitions.")
    lines.append("")

    lines.append("## Paired ratios VOLE/comparator (median + geometric mean, 95% CI by fixture)")
    lines.append("")
    lines.append("| metric | comparator | n | median | geomean | median 95% CI | "
                 "geomean 95% CI | wins | ties | losses | ratio of sums |")
    lines.append("|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|")
    ratio_report = {}
    for label, series in (("build", build_us), ("storage", store_bytes),
                          ("cold", cold_us), ("warm", warm_us)):
        for other in comparators:
            ratios = {}
            sums_v = sums_o = 0
            for fx in fixtures:
                v, o = series["vole"].get(fx), series[other].get(fx)
                if v is None or o in (None, 0):
                    continue
                ratios[fx] = float(v) / float(o)
                sums_v += v
                sums_o += o
            if not ratios:
                continue
            vals = list(ratios.values())
            bydoc = {fx: [rt] for fx, rt in ratios.items()}
            lo_m, hi_m, _ = P19.cluster_bootstrap(bydoc, statistics.median, B, SEED)
            lo_g, hi_g, _ = P19.cluster_bootstrap(bydoc, P19.geomean, B, SEED + 1)
            wins = sum(1 for x in vals if x < 1 - TIE)
            ties = sum(1 for x in vals if 1 - TIE <= x <= 1 + TIE)
            losses = sum(1 for x in vals if x > 1 + TIE)
            ratio_report.setdefault(label, {})[other] = {
                "n": len(vals), "median": statistics.median(vals),
                "geomean": P19.geomean(vals), "median_lo": lo_m, "median_hi": hi_m,
                "geomean_lo": lo_g, "geomean_hi": hi_g, "wins": wins, "ties": ties,
                "losses": losses, "ratio_of_sums": (sums_v / sums_o if sums_o else 0),
            }
            lines.append("| {} | {} | {} | {:.3f} | {:.3f} | {:.3f}..{:.3f} | "
                         "{:.3f}..{:.3f} | {} | {} | {} | {:.3f} |".format(
                             label, other, len(vals), statistics.median(vals),
                             P19.geomean(vals), lo_m, hi_m, lo_g, hi_g, wins, ties,
                             losses, (sums_v / sums_o if sums_o else 0)))
    lines.append("")
    lines.append("A ratio < 1 favours VOLE. The estimator is the **paired per-fixture "
                 "ratio**, summarised by the median and geometric mean with a fixed-seed "
                 "cluster bootstrap over fixtures; `ratio of sums` is reported separately "
                 "and named as such. Each per-fixture lane cost (build, cold, warm) is the "
                 "**minimum over the retained repetitions** (best-of-N); storage is "
                 "measured once after the last build. Every raw sample is kept in "
                 "`raw/build.tsv`, `raw/storage.tsv`, `raw/cold.tsv`, `raw/warm.tsv`. "
                 "With only %d fixture clusters the bootstrap is coarse and is stated as "
                 "such, not as a precise interval." % len(fixtures))
    lines.append("")
    startup = (env or {}).get("python_startup_us") if isinstance(env, dict) else None
    lines.append("Both conventional comparators run a fresh **Python** process per "
                 "request, so their cold numbers include interpreter start-up as part of "
                 "that lane's honest per-request cost (measured bare start-up %s us); "
                 "VOLE's cold path is a native binary. The cold ratio is therefore "
                 "dominated by that constant and is reported for completeness, not "
                 "headlined." % startup)
    lines.append("")
    lines.append("## Per-lane totals (sum over fixtures; times in microseconds `us`)")
    lines.append("")
    lines.append("| lane | build us | storage B | cold us | warm us |")
    lines.append("|---|---:|---:|---:|---:|")
    for lane in lanes:
        lines.append("| {} | {} | {} | {} | {} |".format(
            lane,
            sum(build_us[lane].get(fx, 0) for fx in fixtures),
            sum(store_bytes[lane].get(fx, 0) for fx in fixtures),
            sum(cold_us[lane].get(fx, 0) for fx in fixtures),
            sum(warm_us[lane].get(fx, 0) for fx in fixtures)))
    lines.append("")

    lines.append("## What each lane derives and what it declines")
    lines.append("")
    lines.append("| Q | VOLE | source-retaining SQLite (strict-JSON normalized) | "
                 "conventional %s->host |" % FMT)
    lines.append("|---|---|---|---|")
    QDESC = qdesc(fmt)
    for q in QS:
        lines.append("| {} | {} | {} | {} |".format(q, *QDESC[q]))
    lines.append("")

    lines.append("## Comparison normalization (contract-equivalence)")
    lines.append("")
    lines.append("- **Q1/Q5 (value):** compared as `(class, canonical value)`, where the "
                 "class is the coarse host class (`number`/`string`/`bool`/`null`/"
                 "`array`/`object`). The conventional model **conflates** byte and text "
                 "strings (a byte string is decoded to a host string), so a byte string's "
                 "value is compared as a string; the *byte-vs-text* distinction itself is "
                 "Q10, where both comparators decline.")
    lines.append("- **Q3 (class):** SQLite `json_type` is normalized to the same coarse "
                 "vocabulary (`text`->`string`, `integer`/`real`->`number`).")
    lines.append("- **Q4 (duplicates):** the SQLite lane's normalization preserves "
                 "duplicate members, so `json_tree` counts them exactly; the conventional "
                 "load collapses them and declines.")
    lines.append("- **Q7 (find):** the match set is compared as `(pointer, role, text)`; "
                 "SQLite `fullkey` is normalized to an RFC 6901 pointer and spans are not "
                 "compared (neither comparator keeps them).")
    lines.append("- **Q9/Q10/Q11/Q12 (representation):** the exact encoding width/format "
                 "byte, the byte-vs-text kind, the tag/extension type, and the float width "
                 "are VOLE's representation surface; both comparators decline.")
    lines.append("- **Recorded mismatches (duplicate keys).** For a document with duplicate "
                 "keys (`dupkeys.%s`), VOLE resolves a pointer to the **first** matching "
                 "member while the conventional load collapses to the **last** (Q1/Q5), and "
                 "VOLE's lexical find reports **two** matches while the collapsed load "
                 "reports one (Q7). Those differences are counted as `mismatch`, never "
                 "hidden." % FMT_BIN_EXT[fmt].lstrip("."))
    lines.append("")

    lines.append("## Scope (honest)")
    lines.append("")
    lines.append("- **Self-authored deterministic corpus, NOT a real-world population.** "
                 "The fixtures are generated by `tools/fixtures/make-%s.py` (Python "
                 "stdlib only). Every claim is scoped to these files; the aggregate "
                 "carries a fixture-clustered CI and is not extrapolated." % fmt)
    lines.append("- **Only Q6 is a byte-authority claim.** `materialize --exact == "
                 "source` (length + SHA-256 + `cmp`) and the retained blob reproduce the "
                 "original bytes. Every other observation is a DERIVED projection "
                 "(`Q_gen`, `exact:false`); semantic agreement is not archival equality.")
    lines.append("- **The strict-JSON boundary is a real comparator limitation.** A "
                 "source-retaining store fronted by *strict* JSON cannot represent a map "
                 "with a non-text key, nor `NaN`: those sources cannot be normalized at "
                 "all, so the SQLite lane declines every JSON question for them "
                 "(recorded, e.g. the `%s` and `nonfinite` fixtures)."
                 % ("mapkeys.cbor" if fmt == "cbor" else "intkeys.msgpack"))
    lines.append("- **The conventional load is deliberately the weaker comparator.** It "
                 "is a binary->host-value load: it drops spans, the encoding width/"
                 "signedness, the byte-vs-text kind, duplicate keys, the tag/extension "
                 "type, and float width. A *representation-preserving* decoder could in "
                 "principle match VOLE on several of those; such a lane is **not** built "
                 "here and no claim is made against it — the honest differentiator this "
                 "court measures is exact closure (Q6) plus the representation surface "
                 "against these two comparators.")
    lines.append("- **VOLE capability gaps are recorded, never papered over.** Any "
                 "question VOLE declines is a typed decline (rc 6) and appears as a "
                 "`capability-gap`, never claimed as equivalence.")
    lines.append("- **Nothing here is run on the host.** Every command ran in a pinned "
                 "container (dev toolchain + python3 + sqlite3).")
    lines.append("")

    matrix = []
    matrix.append("# Phase %s — cross-lane Q1–Q12 answer matrix" % cfg(fmt)["phase"])
    matrix.append("")
    matrix.append("`g` = answered (derived), `D` = typed decline, `-` = not applicable.")
    matrix.append("")
    matrix.append("| fixture | Q | VOLE | SQLite | conv | VOLE<->sqlite | VOLE<->conv |")
    matrix.append("|---|---|---|---|---|---|---|")
    for fx in fixtures:
        for q in QS:
            row = qanswers.get((fx, q), {})

            def mark(lane):
                e = row.get(lane)
                if e is None:
                    return "-"
                return "D" if e.get("declined") else "g"

            rs, _ = compare(q, row.get("vole"), row.get("sqlite"))
            rc_, _ = compare(q, row.get("vole"), row.get("conv"))
            matrix.append("| %s | %s | %s | %s | %s | %s | %s |" % (
                fx, q, mark("vole"), mark("sqlite"), mark("conv"), rs, rc_))
    matrix.append("")
    matrix.append("### Aggregate equivalence per Q")
    matrix.append("")
    matrix.append("| Q | comparator | equal | both-decline | capability-gap | "
                  "mismatch | shape |")
    matrix.append("|---|---|---:|---:|---:|---:|---:|")
    for q in QS:
        for comp in comparators:
            c = equiv.get(q, {}).get(comp, {})
            matrix.append("| {} | {} | {} | {} | {} | {} | {} |".format(
                q, comp, c.get("equal", 0), c.get("both-decline", 0),
                c.get("capability-gap", 0), c.get("mismatch", 0), c.get("shape", 0)))
    matrix.append("")

    counts = []
    counts.append("format %s" % fmt)
    counts.append("fixtures %d" % len(fixtures))
    counts.append("questions %d" % len(QS))
    counts.append("exact_ok %d" % exact_ok)
    counts.append("exact_n %d" % exact_n)
    for q in QS:
        for comp in comparators:
            c = equiv.get(q, {}).get(comp, {})
            counts.append("%s.%s.equal %d" % (q, comp, c.get("equal", 0)))
            counts.append("%s.%s.capability_gap %d" % (q, comp, c.get("capability-gap", 0)))
            counts.append("%s.%s.mismatch %d" % (q, comp, c.get("mismatch", 0)))
            counts.append("%s.%s.both_decline %d" % (q, comp, c.get("both-decline", 0)))
    counts.append("verdict %s" % verdict)

    with open(os.path.join(campaign, "SUMMARY.md"), "w") as f:
        f.write("\n".join(lines) + "\n")
    with open(os.path.join(campaign, "MATRIX.md"), "w") as f:
        f.write("\n".join(matrix) + "\n")
    with open(os.path.join(campaign, "counts.txt"), "w") as f:
        f.write("\n".join(counts) + "\n")

    receipt = {
        "campaign": campaign,
        "phase": ("%s — %s economic court (VOLE vs source-retaining strict-JSON SQLite "
                  "+ conventional %s->host load)"
                  % (cfg(fmt)["phase"], FMT, FMT)),
        "verdict": verdict,
        "exact_ok": exact_ok,
        "exact_n": exact_n,
        "fixtures": fixtures,
        "lanes": lanes,
        "estimator": ("paired per-fixture ratio; median + geometric mean; fixed-seed "
                      "cluster bootstrap by fixture (%d resamples, seed %d); tie band "
                      "+/-%d%%; ratio of sums reported separately"
                      % (B, SEED, int(TIE * 100))),
        "hypotheses": {
            "H1": "byte-exactness == 100% or FAIL (materialize --exact == source after "
                  "source + descriptor deletion)",
            "H2": "contract questions Q1-Q12 answered where the lane's model permits",
            "H3": "every unanswerable question is a typed decline (rc 6); malformed "
                  "pointer is a usage error (rc 2); detection boundaries pinned",
            "H4": "build/storage/cold/warm measured per lane with the ADR-0054 estimator",
        },
        "equivalence": {q: equiv.get(q, {}) for q in QS},
        "comparators": comparators,
        "ratios": ratio_report,
        "environment": env,
    }
    with open(os.path.join(campaign, "receipt.json"), "w") as f:
        json.dump(receipt, f, indent=2, sort_keys=True)
    print("\n".join(lines))
    return 0 if verdict == "PASS" else 1


def main(argv=None):
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("plan")
    p.add_argument("--format", required=True, choices=["cbor", "msgpack"])

    b = sub.add_parser("build")
    b.add_argument("--format", required=True, choices=["cbor", "msgpack"])
    b.add_argument("--lane", required=True, choices=["sqlite", "conv"])
    b.add_argument("--source", required=True)
    b.add_argument("--out", required=True)

    q = sub.add_parser("query")
    q.add_argument("--format", required=True, choices=["cbor", "msgpack"])
    q.add_argument("--lane", required=True, choices=["sqlite", "conv"])
    q.add_argument("--dir", required=True)
    q.add_argument("--q", required=True)
    q.add_argument("--plan", default="{}")
    q.add_argument("--out", required=True)

    s = sub.add_parser("session")
    s.add_argument("--format", required=True, choices=["cbor", "msgpack"])
    s.add_argument("--lane", required=True, choices=["sqlite", "conv"])
    s.add_argument("--dir", required=True)
    s.add_argument("--queries", required=True)
    s.add_argument("--plan", default="{}")
    s.add_argument("--out", required=True)

    m = sub.add_parser("materialize")
    m.add_argument("--format", required=True, choices=["cbor", "msgpack"])
    m.add_argument("--lane", required=True, choices=["sqlite", "conv"])
    m.add_argument("--dir", required=True)
    m.add_argument("--out", required=True)

    v = sub.add_parser("version")
    v.add_argument("--format", required=True, choices=["cbor", "msgpack"])

    ag = sub.add_parser("aggregate")
    ag.add_argument("--format", required=True, choices=["cbor", "msgpack"])
    ag.add_argument("--raw", required=True)
    ag.add_argument("--campaign", required=True)
    ag.add_argument("--env", default=None)

    ns = ap.parse_args(argv)
    if ns.cmd == "plan":
        return plan_dump(ns.format)
    if ns.cmd == "build":
        return build(ns.format, ns.lane, ns.source, ns.out)
    if ns.cmd == "query":
        return query(ns.format, ns.lane, ns.dir, ns.q, json.loads(ns.plan), ns.out)
    if ns.cmd == "session":
        return session(ns.format, ns.lane, ns.dir, ns.queries, json.loads(ns.plan), ns.out)
    if ns.cmd == "materialize":
        if ns.lane != "sqlite":
            return 2
        return materialize(ns.format, ns.lane, ns.dir, ns.out)
    if ns.cmd == "version":
        return version(ns.format)
    if ns.cmd == "aggregate":
        return aggregate(ns.format, ns.raw, ns.campaign, ns.env)
    return 2


if __name__ == "__main__":
    sys.exit(main())
