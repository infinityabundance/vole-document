#!/usr/bin/env python3
# Phase 21.6 / 21.6.2 (FIX 1) — YAML economic court: the source-retaining SQLite
# baseline.
#
# The comparator retains the original YAML bytes verbatim (a `raw` BLOB) and also
# loads them through the *conventional* YAML pipeline: a YAML → native-object
# normalization (a stand-in for PyYAML in this offline, pinned image; the pinned
# images ship no `yaml` module and we never fetch one). That pipeline, exactly like
# a real conventional stack, **expands anchors/aliases**, **merges** `<<` keys,
# **strips** tags, **drops** comments, and **normalizes** scalar styles — and the
# normalized value is stored as JSON and queried with SQLite's JSON functions.
#
# Phase 21.6.2 (FIX 1) parity with the JSON court:
#   * the lane now prefers the **hash-pinned modern SQLite** (`pysqlite3`, bundled
#     3.51.1) so the `jsonb` claim is true (JSONB ships only from 3.45.0); it falls
#     back to the stdlib `sqlite3` and records which module + version it used
#     (`version`). The normalized JSON is stored as JSONB (`jsonb(j)`) and queried
#     through it where available.
#   * a **span-preserving pure-Python comparator** (`phase21-6-yaml-spanpy.py`) is
#     added as the serious conventional competitor: it retains the source and
#     records every node's exact byte span plus anchors/tags/styles/merge keys, so
#     the representation questions this lane declines are answerable there.
# What this normalizing lane therefore cannot answer is representation: source
# spans, the anchor graph, tags, scalar styles, and merge-key handling. Those
# questions decline (recorded honestly, never papered over).
#
#   build       --source FILE --db DB
#   query       --db DB --q Qn --plan JSON --out FILE
#   session     --db DB --queries Q1,Q2,... --plan JSON --out FILE
#   materialize --db DB --out FILE
#   version
#   aggregate   --raw DIR --campaign DIR --env ENV_JSON

import argparse
import copy
import hashlib
import json
import os
import re
import sys
import time

try:
    # The hash-pinned wheel in the `analytical` image; bundles SQLite 3.51.1 so
    # `jsonb` (>= 3.45) is genuinely available.
    import pysqlite3 as sqlite3
    SQLITE_MODULE = "pysqlite3"
except ImportError:  # pragma: no cover - lanes without the pinned wheel
    import sqlite3
    SQLITE_MODULE = "sqlite3"


def _has_jsonb(con):
    try:
        con.execute("SELECT jsonb('1')").fetchone()
        return True
    except sqlite3.Error:
        return False

INT = re.compile(r"^-?\d+$")
FLOAT = re.compile(r"^-?\d+\.\d+([eE][-+]?\d+)?$")
IDX = re.compile(r"^(0|[1-9][0-9]*)$")


def sha256_hex(b):
    return hashlib.sha256(b).hexdigest()


def now_us():
    return int(time.monotonic() * 1_000_000)


def envelope(q, value=None, *, declined=False, code=None, reason="", detail=None):
    e = {"q": q, "lane": "sqlite", "declined": bool(declined), "native": True,
         "value": None if declined else value, "detail": detail or {}}
    e["decline"] = {"code": code, "detail": reason} if declined else None
    return e


# --- A conventional YAML -> native normalization (offline PyYAML stand-in) ---


def _strip_comment(s):
    out = []
    i = 0
    q = None
    while i < len(s):
        c = s[i]
        if q:
            out.append(c)
            if c == "\\" and q == '"':
                if i + 1 < len(s):
                    out.append(s[i + 1])
                    i += 2
                    continue
            if c == q:
                q = None
            i += 1
            continue
        if c == "'" or c == '"':
            q = c
            out.append(c)
            i += 1
            continue
        if c == "#" and i > 0 and s[i - 1] in " \t":
            break
        out.append(c)
        i += 1
    return "".join(out).rstrip()


def _unquote_single(s):
    inner = s[1:-1] if len(s) >= 2 and s.endswith("'") else s[1:]
    return inner.replace("''", "'")


def _unquote_double(s):
    inner = s[1:-1] if len(s) >= 2 and s.endswith('"') else s[1:]
    try:
        return json.loads('"' + inner + '"')
    except ValueError:
        return inner


def scalar(text):
    s = text.strip()
    if s == "":
        return None
    if s.startswith("'"):
        return _unquote_single(s)
    if s.startswith('"'):
        return _unquote_double(s)
    low = s.lower()
    if low in ("true", "false"):
        return low == "true"
    if low in ("null", "~"):
        return None
    if INT.match(s):
        return int(s)
    if FLOAT.match(s):
        return float(s)
    return s


def _parse_props(s):
    anchor = None
    tag = None
    s = s.lstrip()
    while True:
        if s.startswith("&"):
            j = 1
            while j < len(s) and s[j] not in " \t,[]{}":
                j += 1
            anchor = s[1:j]
            s = s[j:].lstrip()
            continue
        if s.startswith("!<"):
            j = s.find(">")
            tag = s[: j + 1] if j >= 0 else s
            s = s[j + 1:].lstrip() if j >= 0 else ""
            continue
        if s.startswith("!"):
            j = 1
            while j < len(s) and s[j] not in " \t,[]{}":
                j += 1
            tag = s[:j]
            s = s[j:].lstrip()
            continue
        break
    return anchor, tag, s


def _split_key(s):
    i = 0
    q = None
    while i < len(s):
        c = s[i]
        if q:
            if c == "\\" and q == '"':
                i += 2
                continue
            if c == q:
                q = None
            i += 1
            continue
        if c == "'" or c == '"':
            q = c
            i += 1
            continue
        if c == ":" and (i + 1 >= len(s) or s[i + 1] in " \t"):
            return s[:i].strip(), s[i + 1:]
        i += 1
    return None


class _Flow:
    def __init__(self, s, env):
        self.s = s
        self.i = 0
        self.env = env

    def ws(self):
        while self.i < len(self.s) and self.s[self.i] in " \t":
            self.i += 1

    def node(self):
        self.ws()
        if self.i >= len(self.s):
            return None
        c = self.s[self.i]
        if c == "[":
            return self.seq()
        if c == "{":
            return self.map()
        if c == "'":
            j = self.i + 1
            buf = []
            while j < len(self.s):
                if self.s[j] == "'" and (j + 1 >= len(self.s) or self.s[j + 1] != "'"):
                    break
                if self.s[j] == "'" and j + 1 < len(self.s) and self.s[j + 1] == "'":
                    buf.append("'")
                    j += 2
                    continue
                buf.append(self.s[j])
                j += 1
            self.i = j + 1
            return "".join(buf)
        if c == '"':
            j = self.i + 1
            while j < len(self.s):
                if self.s[j] == "\\":
                    j += 2
                    continue
                if self.s[j] == '"':
                    break
                j += 1
            raw = self.s[self.i: j + 1]
            self.i = j + 1
            return _unquote_double(raw)
        j = self.i
        while j < len(self.s) and self.s[j] not in ",[]{}":
            if self.s[j] == ":" and (j + 1 >= len(self.s) or self.s[j + 1] in " \t,]}"):
                break
            j += 1
        tok = self.s[self.i:j].strip()
        self.i = j
        return scalar(tok)

    def seq(self):
        self.i += 1
        out = []
        self.ws()
        if self.i < len(self.s) and self.s[self.i] == "]":
            self.i += 1
            return out
        while True:
            out.append(self.node())
            self.ws()
            if self.i < len(self.s) and self.s[self.i] == ",":
                self.i += 1
                self.ws()
                if self.i < len(self.s) and self.s[self.i] == "]":
                    self.i += 1
                    break
            elif self.i < len(self.s) and self.s[self.i] == "]":
                self.i += 1
                break
            else:
                break
        return out

    def map(self):
        self.i += 1
        out = {}
        self.ws()
        if self.i < len(self.s) and self.s[self.i] == "}":
            self.i += 1
            return out
        while True:
            key = self.node()
            self.ws()
            if self.i < len(self.s) and self.s[self.i] == ":":
                self.i += 1
                val = self.node()
            else:
                val = None
            out[key if isinstance(key, str) else str(key)] = val
            self.ws()
            if self.i < len(self.s) and self.s[self.i] == ",":
                self.i += 1
                self.ws()
                if self.i < len(self.s) and self.s[self.i] == "}":
                    self.i += 1
                    break
            elif self.i < len(self.s) and self.s[self.i] == "}":
                self.i += 1
                break
            else:
                break
        return out


class _Reader:
    def __init__(self, lines, env):
        self.lines = list(lines)
        self.i = 0
        self.env = env

    def eof(self):
        return self.i >= len(self.lines)

    def raw(self):
        return self.lines[self.i]

    def indent(self):
        r = self.raw()
        return len(r) - len(r.lstrip(" "))

    def skip(self):
        while not self.eof():
            s = self.raw().strip()
            if s == "" or s.startswith("#"):
                self.i += 1
            else:
                break

    def inline(self, text):
        anchor, tag, rest = _parse_props(text)
        val = self._inline_bare(rest)
        if anchor is not None:
            self.env[anchor] = val
        return val

    def _inline_bare(self, rest):
        rest = rest.strip()
        if rest == "":
            return None
        if rest.startswith("*"):
            name = rest[1:].strip()
            return copy.deepcopy(self.env.get(name))
        if rest[0] in "[{":
            return _Flow(rest, self.env).node()
        return scalar(rest)

    def block_scalar(self, style, parent_indent):
        # collect following lines more indented than parent.
        body = []
        base = None
        while not self.eof():
            r = self.raw()
            if r.strip() == "":
                body.append("")
                self.i += 1
                continue
            ind = len(r) - len(r.lstrip(" "))
            if ind <= parent_indent:
                break
            if base is None:
                base = ind
            body.append(r[min(base, len(r)):])
            self.i += 1
        if style == ">":
            out = []
            prev = False
            for b in body:
                if b != "" and prev:
                    out.append(" ")
                out.append(b)
                out.append("\n")
                prev = b != ""
            return "".join(out)
        return "\n".join(body) + ("\n" if body else "")

    def child(self, parent_indent):
        self.skip()
        if self.eof():
            return None
        cur = self.indent()
        if cur > parent_indent:
            return self.node(cur)
        if cur == parent_indent and self.raw().lstrip().startswith("-"):
            return self.seq(cur)
        return None

    def node(self, indent):
        self.skip()
        if self.eof():
            return None
        cur = self.indent()
        if cur < indent:
            return None
        stripped = _strip_comment(self.raw().lstrip())
        anchor, tag, rest = _parse_props(stripped)
        if rest == "":
            self.i += 1
            val = self.child(cur)
            if anchor is not None:
                self.env[anchor] = val
            return val
        if rest.startswith("- ") or rest == "-":
            return self.seq(cur)
        if _split_key(stripped) is not None:
            return self.map(cur)
        val = self._inline_bare(rest)
        self.i += 1
        if anchor is not None:
            self.env[anchor] = val
        return val

    def map(self, indent):
        out = {}
        while True:
            self.skip()
            if self.eof():
                break
            cur = self.indent()
            if cur != indent:
                break
            stripped = _strip_comment(self.raw().lstrip())
            if stripped.startswith("- "):
                break
            kv = _split_key(stripped)
            if kv is None:
                break
            key_text, val_text = kv
            key = scalar(key_text)
            key = key if isinstance(key, str) else ("" if key is None else str(key))
            self.i += 1
            anchor, tag, rest = _parse_props(_strip_comment(val_text))
            rest = rest.rstrip()
            if rest.startswith("|") or rest.startswith(">"):
                val = self.block_scalar(rest[0], indent)
            elif rest == "":
                val = self.child(indent)
            else:
                val = self._inline_bare(rest)
            if anchor is not None:
                self.env[anchor] = val
            if key == "<<":
                self._merge(out, val)
            else:
                out[key] = val
        return out

    @staticmethod
    def _merge(out, val):
        if isinstance(val, dict):
            for k, v in val.items():
                out.setdefault(k, v)
        elif isinstance(val, list):
            for item in val:
                if isinstance(item, dict):
                    for k, v in item.items():
                        out.setdefault(k, v)

    def seq(self, indent):
        out = []
        while True:
            self.skip()
            if self.eof():
                break
            cur = self.indent()
            if cur != indent:
                break
            r = self.raw()
            lstripped = r.lstrip()
            if not (lstripped.startswith("- ") or lstripped == "-"):
                break
            after = r[cur + 1:]
            stripped = after.lstrip(" ")
            if stripped == "":
                self.i += 1
                out.append(self.child(indent))
                continue
            content_col = cur + 1 + (len(after) - len(stripped))
            self.lines[self.i] = " " * content_col + stripped
            out.append(self.node(content_col))
        return out


def load_yaml_docs(text):
    lines = text.split("\n")
    if lines and lines[-1] == "":
        lines.pop()
    docs = []
    cur = []
    started = False
    for ln in lines:
        s = ln.rstrip()
        if s == "---" or s.startswith("--- "):
            if started or cur:
                docs.append(cur)
                cur = []
            started = True
            rest = s[3:].strip()
            if rest and not rest.startswith("#"):
                cur.append(rest)
            continue
        if s == "...":
            if started or cur:
                docs.append(cur)
                cur = []
            started = False
            continue
        cur.append(ln)
    if cur or started:
        docs.append(cur)
    out = []
    for d in docs:
        r = _Reader(d, {})
        r.skip()
        if r.eof():
            out.append(None)
            continue
        out.append(r.node(r.indent()))
    return out


# --- build -------------------------------------------------------------------


def _schema():
    return (
        "PRAGMA journal_mode=DELETE;\n"
        "CREATE TABLE doc(id INTEGER PRIMARY KEY, raw BLOB NOT NULL, j TEXT NOT NULL, jb BLOB);\n"
    )


def build(source, db_path):
    for suffix in ("", "-wal", "-shm", "-journal"):
        try:
            os.remove(db_path + suffix)
        except FileNotFoundError:
            pass
    t0 = now_us()
    with open(source, "rb") as f:
        raw = f.read()
    text = raw.decode("utf-8")
    docs = load_yaml_docs(text)
    norm = json.dumps(docs, ensure_ascii=False)
    extract_us = now_us() - t0
    con = sqlite3.connect(db_path)
    con.executescript(_schema())
    if _has_jsonb(con):
        # Store the normalized value as JSONB (>= 3.45) and query through it.
        con.execute("INSERT INTO doc(id, raw, j, jb) VALUES (1, ?, ?, jsonb(?))",
                    (raw, norm, norm))
    else:
        con.execute("INSERT INTO doc(id, raw, j, jb) VALUES (1, ?, ?, NULL)",
                    (raw, norm))
    con.commit()
    con.close()
    print(json.dumps({"ok": True, "fmt": "yaml", "src_len": len(raw),
                      "docs": len(docs), "extract_us": extract_us}, sort_keys=True))
    return 0


def _load(db_path):
    con = sqlite3.connect(db_path)
    row = con.execute("SELECT raw, j, jb FROM doc WHERE id=1").fetchone()
    jval = row[2] if row[2] is not None else row[1]
    return con, row[0], jval


# --- queries -----------------------------------------------------------------


def norm_scalar(v):
    if v is None:
        return None
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, int):
        return str(v)
    if isinstance(v, float):
        return repr(v)
    return str(v)


def _doc_path(pointer):
    segs = pointer.split(".") if pointer else []
    doc = 0
    if segs and segs[0].startswith("doc") and segs[0][3:].isdigit():
        doc = int(segs[0][3:])
        segs = segs[1:]
    path = "$[%d]" % doc
    for s in segs:
        if IDX.match(s):
            path += "[%s]" % s
        elif re.match(r"^[A-Za-z_][A-Za-z0-9_]*$", s):
            path += "." + s
        else:
            path += '."' + s.replace('"', '""') + '"'
    return path


def _q_value(con, j, pointer):
    v = con.execute("SELECT json_extract(?, ?)", (j, _doc_path(pointer))).fetchone()[0]
    if v is None:
        return envelope(1, declined=True, code="missing", reason="no value at path")
    return envelope(1, norm_scalar(v))


def _q_span():
    return envelope(2, declined=True, code="no-source-span",
                    reason="a conventional YAML stack exposes no source byte span")


def _q_anchor():
    return envelope(3, declined=True, code="aliases-expanded",
                    reason="anchors/aliases are expanded; the graph is not retained")


def _q_tag():
    return envelope(4, declined=True, code="tags-dropped",
                    reason="tags are resolved to native types and dropped")


def _q_docs(con, j):
    n = con.execute("SELECT json_array_length(?)", (j,)).fetchone()[0]
    return envelope(5, int(n or 0))


def _q_style():
    return envelope(6, declined=True, code="style-normalized",
                    reason="scalar styles are normalized away in the native object")


def _q_merge():
    return envelope(7, declined=True, code="merge-expanded",
                    reason="`<<` merge keys are expanded and never surface as members")


def _q_exact(raw):
    return envelope(8, {"length": len(raw), "sha256": sha256_hex(raw)})


def run_query(con, raw, j, q, plan):
    if q == "Q1":
        return _q_value(con, j, plan["value_path"])
    if q == "Q2":
        return _q_span()
    if q == "Q3":
        return _q_anchor()
    if q == "Q4":
        return _q_tag()
    if q == "Q5":
        return _q_docs(con, j)
    if q == "Q6":
        return _q_style()
    if q == "Q7":
        return _q_merge()
    if q == "Q8":
        return _q_exact(raw)
    return envelope(q, declined=True, code="unknown-question", reason=q)


def query(db_path, q, plan, out):
    con, raw, j = _load(db_path)
    env = run_query(con, raw, j, q, plan)
    env["q"] = q
    con.close()
    with open(out, "w") as f:
        json.dump(env, f, sort_keys=True)
    return 0


def session(db_path, queries, plan, out):
    con, raw, j = _load(db_path)
    batch = []
    for q in queries.split(","):
        q = q.strip()
        if not q:
            continue
        t0 = now_us()
        env = run_query(con, raw, j, q, plan)
        env["q"] = q
        us = now_us() - t0
        batch.append({"q": q, "us": us, "env": env})
    con.close()
    with open(out, "w") as f:
        json.dump({"batch": batch}, f, sort_keys=True)
    return 0


def materialize(db_path, out):
    con, raw, _ = _load(db_path)
    con.close()
    with open(out, "wb") as f:
        f.write(raw)
    return 0


# --- aggregate ---------------------------------------------------------------


def _load_p19():
    import importlib.util
    here = os.path.dirname(os.path.abspath(__file__))
    path = os.path.join(here, "phase19-repeat.py")
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


def compare(q, va, vb, vb_name="sqlite"):
    if va is None or vb is None:
        return "missing", "no envelope"
    if va.get("declined") and vb.get("declined"):
        return "both-decline", "both typed declines"
    if va.get("declined") or vb.get("declined"):
        who = "vole" if va.get("declined") else vb_name
        return "capability-gap", "%s declines" % who
    a, b = va.get("value"), vb.get("value")
    if q in ("Q1", "Q4", "Q6"):
        return ("equal" if str(a) == str(b) else "mismatch"), "scalar"
    if q == "Q2":
        return ("equal" if list(a or []) == list(b or []) else "mismatch"), "span"
    if q == "Q3":
        if isinstance(a, dict) and isinstance(b, dict):
            ok = a.get("anchor") == b.get("anchor") and a.get("alias_count") == b.get("alias_count")
            return ("equal" if ok else "mismatch"), "anchor+alias_count"
        return "shape", "not dict"
    if q == "Q5":
        return ("equal" if str(a) == str(b) else "mismatch"), "document count"
    if q == "Q7":
        if isinstance(a, dict) and isinstance(b, dict):
            ok = a.get("kind") == b.get("kind") and a.get("matches") == b.get("matches")
            return ("equal" if ok else "mismatch"), "merge-member"
        return "shape", "not dict"
    if q == "Q8":
        if isinstance(a, dict) and isinstance(b, dict):
            ok = a.get("sha256") == b.get("sha256") and a.get("length") == b.get("length")
            return ("equal" if ok else "mismatch"), "length+sha256"
        return "shape", "not dict"
    return ("equal" if a == b else "mismatch"), "default"


TIE = 0.10


QS = ["Q1", "Q2", "Q3", "Q4", "Q5", "Q6", "Q7", "Q8"]

QDESC = {
    "Q1": ("a scalar at a dotted path (exact token spelling)",
           "`json_extract` value (normalized)",
           "decoded scalar at the dotted path"),
    "Q2": ("a node's exact source span",
           "no source span exists -> typed decline",
           "exact byte span `[start,end)`"),
    "Q3": ("an anchor and the aliases that target it",
           "aliases are expanded -> typed decline",
           "anchor graph (name + alias count), never expanded"),
    "Q4": ("a node's literal tag text",
           "tags are resolved and dropped -> typed decline",
           "literal tag text (or null)"),
    "Q5": ("the number of documents in the stream",
           "the document list length",
           "the document count"),
    "Q6": ("a scalar's style (plain/single/double/literal/folded)",
           "styles are normalized -> typed decline",
           "scalar/container style"),
    "Q7": ("a `<<` merge member (surfaced, never merged)",
           "`<<` is merged into the mapping -> typed decline",
           "`<<` surfaced as a member (kind + match count)"),
    "Q8": ("`materialize --exact` (byte-authority)",
           "retained raw BLOB (byte-authority)",
           "retained raw bytes (byte-authority)"),
}


def aggregate(raw, campaign, env_path=None):
    P19 = _load_p19()
    B = 10000
    SEED = 21621
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
    lanes = ["vole", "sqlite", "spanpy"]
    comparators = ["sqlite", "spanpy"]

    build_rows = read_tsv(os.path.join(raw, "build.tsv"))
    storage_rows = read_tsv(os.path.join(raw, "storage.tsv"))
    cold_rows = read_tsv(os.path.join(raw, "cold.tsv"))
    warm_rows = read_tsv(os.path.join(raw, "warm.tsv"))
    exact_rows = read_tsv(os.path.join(raw, "exact.tsv"))

    def best_by_fixture(rows, lane, metric="us", sum_per_rep=False):
        per = {}
        for r in rows:
            if r["lane"] != lane or r.get("rc") != "0":
                continue
            fx = r["fixture"]
            v = int(r[metric])
            if sum_per_rep:
                key = (fx, r["rep"])
                per[key] = per.get(key, 0) + v
            else:
                if fx not in per or v < per[fx]:
                    per[fx] = v
        if sum_per_rep:
            out = {}
            for (fx, _rep), v in per.items():
                if fx not in out or v < out[fx]:
                    out[fx] = v
            return out
        return per

    build_us = {lane: best_by_fixture(build_rows, lane) for lane in lanes}
    store_bytes = {lane: {} for lane in lanes}
    for r in storage_rows:
        store_bytes[r["lane"]][r["fixture"]] = int(r["bytes"])
    cold_us = {lane: best_by_fixture(cold_rows, lane, sum_per_rep=True) for lane in lanes}
    warm_us = {lane: best_by_fixture(warm_rows, lane, sum_per_rep=True) for lane in lanes}

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
                res, _ = compare(q, row.get("vole"), row.get(comp), comp)
                equiv.setdefault(q, {}).setdefault(comp, {}).setdefault(res, 0)
                equiv[q][comp][res] += 1

    exact_ok = sum(1 for r in exact_rows if r.get("vole_ok") in ("1", "true"))
    exact_n = len(exact_rows)

    lines = []
    lines.append("# Phase 21.6.2 — YAML economic court")
    lines.append("")
    lines.append("**Question.** Against a source-retaining SQLite baseline that runs a "
                 "conventional YAML → native-object normalization (the offline PyYAML "
                 "stand-in: expand aliases, merge `<<`, drop tags/comments, normalize "
                 "styles) and queries SQLite's JSON functions on a hash-pinned modern "
                 "SQLite (`jsonb`), **and** against a span-preserving pure-Python YAML "
                 "scanner, can VOLE answer the same eight questions (Q1–Q8) while closing "
                 "the original YAML byte-exactly — and does it add value by **preserving "
                 "representation** (spans, the anchor graph, tags, styles, merge keys)?")
    lines.append("")
    lines.append("**Method.** A deterministic self-authored YAML corpus "
                 "(`tools/fixtures/make-yaml.py --corpus`) is regenerated at court "
                 "time; each fixture is ingested by **three lanes** (VOLE field CLI; the "
                 "source-retaining SQLite baseline; a span-preserving pure-Python YAML "
                 "scanner that retains the source and records every node's exact byte "
                 "span), Q1–Q8 are asked of each, and build/storage/cold/warm are "
                 "measured. Persistent bytes are the **sum of regular-file sizes** "
                 "(`find -type f -printf '%s'`), never `du -sb` (ADR-0049).")
    lines.append("")
    lines.append("Corpus: **%d fixtures**; lanes **%s**; questions **Q1–Q8**; "
                 "bootstrap **%d resamples, seed %d**, cluster-resampled by fixture; "
                 "tie band **+/-%d%%**."
                 % (len(fixtures), ", ".join(lanes), B, SEED, int(TIE * 100)))
    lines.append("")
    prof = (env or {}).get("profile", "unknown")
    sub = (env or {}).get("vole_substrate", "unknown")
    bin_label = (env or {}).get("bin", "?")
    lines.append("VOLE lane: **%s** profile (`%s`); substrate: **%s**. The comparators "
                 "(SQLite C + Python, and the pure-Python span-preserving scanner) are "
                 "unaffected by the Rust profile while the entropyfs build is not, so the "
                 "release default keeps the comparison fair to VOLE. All wall times are "
                 "**microseconds (`us`)**." % (prof, bin_label, sub))
    lines.append("")
    lines.append("## Verdict")
    lines.append("")
    verdict = "PASS" if exact_ok == exact_n and exact_n > 0 else "FAIL"
    lines.append("- **VOLE exactness (Q8): %d/%d byte-exact** (length + SHA-256 + "
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
    lines.append("|---|---:|" + "".join("---:|" for _ in lanes) + "".join("---:|" for _ in lanes))
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
                 "and named as such. With only %d fixture clusters the bootstrap is coarse "
                 "and is stated as such." % len(fixtures))
    lines.append("")
    sup = (env or {}).get("supersedes") if isinstance(env, dict) else None
    if isinstance(sup, dict):
        def _now(metric, comp):
            r = ratio_report.get(metric, {}).get(comp)
            return "{:.3f}".format(r["median"]) if r else "n/a"
        lines.append("## Supersedes — Phase 21.6 (ceab8ec)")
        lines.append("")
        lines.append("`[SUPERSEDED: %s]` — that campaign's SQLite lane ran SQLite **%s** "
                     "(no `jsonb`) and compared only against a normalizing baseline; it "
                     "could not answer Q2/Q3/Q4/Q6/Q7 and recorded a span-preserving YAML "
                     "comparator as a follow-up. This campaign runs a hash-pinned modern "
                     "SQLite (**%s**, via `pysqlite3`, JSONB=%s) **and** adds the "
                     "span-preserving pure-Python scanner."
                     % (sup.get("campaign", "?"), sup.get("sqlite_version_then", "?"),
                        (env or {}).get("sqlite_version", "?"),
                        (env or {}).get("jsonb_available", "?")))
        lines.append("")
        lines.append("| metric | comparator | then (median) | now (median) |")
        lines.append("|---|---|---:|---:|")
        for metric in ("build", "storage", "cold", "warm"):
            then = sup.get("%s_median_then" % metric)
            if then is None:
                continue
            lines.append("| %s | sqlite | [SUPERSEDED: %.3f] | %s |"
                         % (metric, then, _now(metric, "sqlite")))
        lines.append("")
        lines.append("The headline change: the representation questions this court used "
                     "to credit VOLE (Q2 span, Q3 anchors, Q4 tags, Q6 styles, Q7 merge "
                     "keys) are now answered by a *span-preserving conventional YAML "
                     "scanner* too, so VOLE's representation advantage over a serious "
                     "competitor **shrinks or vanishes**. Verdict still FAILs unless VOLE "
                     "exactness is 100%.")
        lines.append("")
    startup = (env or {}).get("python_startup_us") if isinstance(env, dict) else None
    lines.append("Both conventional comparator cold paths (SQLite C + Python, and the "
                 "pure-Python span-preserving scanner) run a fresh **Python** process "
                 "per request, so their cold numbers include interpreter start-up as part "
                 "of that lane's honest per-request cost (measured bare start-up %s us); "
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
    lines.append("| Q | VOLE | SQLite (conventional YAML → JSON, modern) | "
                 "span-preserving Python |")
    lines.append("|---|---|---|---|")
    for q in QS:
        lines.append("| {} | {} | {} | {} |".format(q, *QDESC[q]))
    lines.append("")

    lines.append("## Scope (honest)")
    lines.append("")
    lines.append("- **Self-authored deterministic corpus, NOT a real-world population.** "
                 "Fixtures are generated by `tools/fixtures/make-yaml.py` (Python stdlib "
                 "only). Every claim is scoped to these files.")
    lines.append("- **Only Q8 is a byte-authority claim.** `materialize --exact == source` "
                 "(length + SHA-256 + `cmp`) reproduces the original bytes. Every other "
                 "observation is a DERIVED projection; semantic agreement is not archival "
                 "equality.")
    lines.append("- **What survives against the strongest competitor.** Against a "
                 "*span-preserving* pure-Python YAML scanner, VOLE's representation "
                 "advantages in Q2 (spans), Q3 (anchor graph), Q4 (tags), Q6 (styles), "
                 "and Q7 (merge keys) **no longer distinguish it** — the conventional "
                 "scanner answers them too. VOLE's remaining, real advantages are (i) "
                 "byte-authoritative exact closure of the whole source (Q8) and (ii) the "
                 "economics. This is the honest picture the corrected comparator shows.")
    lines.append("- **The normalizing SQLite lane's YAML → native step is inherently "
                 "lossy** — that is the point of the comparison; the pinned image ships "
                 "no `yaml` module, so both baselines implement a bounded, conventional "
                 "scanner inline (never a network fetch).")
    lines.append("- **VOLE capability gaps are recorded, never papered over.** Any "
                 "question VOLE declines is a typed decline (`rc` 6 or `rc` 2) and appears "
                 "as a `capability-gap`, never claimed as equivalence.")
    lines.append("- **Nothing here is run on the host.** Every command ran in the pinned "
                 "`analytical` container (dev toolchain + python3 + hash-pinned modern "
                 "SQLite).")
    lines.append("")

    matrix = []
    matrix.append("# Phase 21.6.2 — cross-lane Q1–Q8 answer matrix")
    matrix.append("")
    matrix.append("`g` = answered (derived), `D` = typed decline, `-` = not applicable.")
    matrix.append("")
    matrix.append("| fixture | Q | VOLE | SQLite | spanpy | VOLE<->sqlite | "
                  "VOLE<->spanpy |")
    matrix.append("|---|---|---|---|---|---|---|")
    for fx in fixtures:
        for q in QS:
            row = qanswers.get((fx, q), {})

            def mark(lane):
                e = row.get(lane)
                if e is None:
                    return "-"
                return "D" if e.get("declined") else "g"

            rs, _ = compare(q, row.get("vole"), row.get("sqlite"), "sqlite")
            rp, _ = compare(q, row.get("vole"), row.get("spanpy"), "spanpy")
            matrix.append("| %s | %s | %s | %s | %s | %s | %s |" % (
                fx, q, mark("vole"), mark("sqlite"), mark("spanpy"), rs, rp))
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
        "phase": "21.6.2 — YAML economic court (VOLE vs source-retaining SQLite "
                 "modern/JSONB + span-preserving Python)",
        "verdict": verdict,
        "exact_ok": exact_ok,
        "exact_n": exact_n,
        "fixtures": fixtures,
        "lanes": lanes,
        "comparators": comparators,
        "estimator": ("paired per-fixture ratio; median + geometric mean; fixed-seed "
                      "cluster bootstrap by fixture (%d resamples, seed %d); tie band "
                      "+/-%d%%; ratio of sums reported separately"
                      % (B, SEED, int(TIE * 100))),
        "equivalence": {q: equiv.get(q, {}) for q in QS},
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
    a = sub.add_parser("aggregate")
    a.add_argument("--raw", required=True)
    a.add_argument("--campaign", required=True)
    a.add_argument("--env", default=None)
    sub.add_parser("version")
    ns = ap.parse_args(argv)
    if ns.cmd == "build":
        return build(ns.source, ns.db)
    if ns.cmd == "query":
        return query(ns.db, ns.q, json.loads(ns.plan), ns.out)
    if ns.cmd == "session":
        return session(ns.db, ns.queries, json.loads(ns.plan), ns.out)
    if ns.cmd == "materialize":
        return materialize(ns.db, ns.out)
    if ns.cmd == "aggregate":
        return aggregate(ns.raw, ns.campaign, ns.env)
    if ns.cmd == "version":
        con = sqlite3.connect(":memory:")
        print(json.dumps({"module": SQLITE_MODULE,
                          "sqlite_version": sqlite3.sqlite_version,
                          "jsonb": _has_jsonb(con)}, sort_keys=True))
        con.close()
        return 0
    return 2


if __name__ == "__main__":
    sys.exit(main())
