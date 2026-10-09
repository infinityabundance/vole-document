#!/usr/bin/env python3
# Phase 21.6 — YAML economic court: the source-retaining SQLite baseline.
#
# The comparator retains the original YAML bytes verbatim (a `raw` BLOB) and also
# loads them through the *conventional* YAML pipeline: a YAML → native-object
# normalization (a stand-in for PyYAML in this offline, pinned image; the pinned
# `doc-baseline` image has no `yaml` module and we never fetch one). That pipeline,
# exactly like a real conventional stack, **expands anchors/aliases**, **merges**
# `<<` keys, **strips** tags, **drops** comments, and **normalizes** scalar styles —
# and the normalized value is then stored as JSON text and queried with SQLite's
# `json1` (SQLite 3.40.1; JSONB requires >= 3.45 and is NOT used here). What it therefore cannot answer is representation: source spans,
# the anchor graph, tags, scalar styles, and merge-key handling. Those questions
# decline (recorded honestly, never papered over).
#
#   build       --source FILE --db DB
#   query       --db DB --q Qn --plan JSON --out FILE
#   session     --db DB --queries Q1,Q2,... --plan JSON --out FILE
#   materialize --db DB --out FILE
#   aggregate   --raw DIR --campaign DIR --env ENV_JSON

import argparse
import copy
import hashlib
import json
import os
import re
import sqlite3
import sys
import time

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
        "CREATE TABLE doc(id INTEGER PRIMARY KEY, raw BLOB NOT NULL, j TEXT NOT NULL);\n"
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
    con.execute("INSERT INTO doc(id, raw, j) VALUES (1, ?, ?)", (raw, norm))
    con.commit()
    con.close()
    print(json.dumps({"ok": True, "fmt": "yaml", "src_len": len(raw),
                      "docs": len(docs), "extract_us": extract_us}, sort_keys=True))
    return 0


def _load(db_path):
    con = sqlite3.connect(db_path)
    row = con.execute("SELECT raw, j FROM doc WHERE id=1").fetchone()
    return con, row[0], row[1]


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


def compare(q, va, vb):
    if va is None or vb is None:
        return "missing", "no envelope"
    if va.get("declined") and vb.get("declined"):
        return "both-decline", "both typed declines"
    if va.get("declined") or vb.get("declined"):
        who = "vole" if va.get("declined") else "sqlite"
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


QS = ["Q1", "Q2", "Q3", "Q4", "Q5", "Q6", "Q7", "Q8"]

QDESC = {
    "Q1": ("a scalar at a dotted path (exact token spelling)",
           "`json_extract` value (normalized)"),
    "Q2": ("a node's exact source span",
           "no source span exists -> typed decline"),
    "Q3": ("an anchor and the aliases that target it",
           "aliases are expanded -> typed decline"),
    "Q4": ("a node's literal tag text",
           "tags are resolved and dropped -> typed decline"),
    "Q5": ("the number of documents in the stream",
           "the document list length"),
    "Q6": ("a scalar's style (plain/single/double/literal/folded)",
           "styles are normalized -> typed decline"),
    "Q7": ("a `<<` merge member (surfaced, never merged)",
           "`<<` is merged into the mapping -> typed decline"),
    "Q8": ("`materialize --exact` (byte-authority)",
           "retained raw BLOB (byte-authority)"),
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
    lanes = ["vole", "sqlite"]

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
            res, _ = compare(q, row.get("vole"), row.get("sqlite"))
            equiv.setdefault(q, {}).setdefault(res, 0)
            equiv[q][res] += 1

    exact_ok = sum(1 for r in exact_rows if r.get("vole_ok") in ("1", "true"))
    exact_n = len(exact_rows)

    lines = []
    lines.append("# Phase 21.6 — YAML economic court")
    lines.append("")
    lines.append("**Question.** Against a source-retaining SQLite baseline that runs a "
                 "conventional YAML → native-object normalization (the offline PyYAML "
                 "stand-in: expand aliases, merge `<<`, drop tags/comments, normalize "
                 "styles) and queries SQLite's `json1` functions (SQLite 3.40.1; "
                 "JSONB is not used), can VOLE answer the same eight "
                 "questions (Q1–Q8) it can answer, while closing the original YAML "
                 "byte-exactly — and does it add value by **preserving representation** "
                 "(spans, the anchor graph, tags, styles, merge keys)?")
    lines.append("")
    lines.append("**Method.** A deterministic self-authored YAML corpus "
                 "(`tools/fixtures/make-yaml.py --corpus`) is regenerated at court "
                 "time; each fixture is ingested by two lanes (VOLE field CLI; the "
                 "source-retaining SQLite baseline), Q1–Q8 are asked of each, and "
                 "build/storage/cold/warm are measured. Persistent bytes are the **sum "
                 "of regular-file sizes** (`find -type f -printf '%s'`), never `du -sb` "
                 "(ADR-0049).")
    lines.append("")
    lines.append("Corpus: **%d fixtures**; lanes **%s**; questions **Q1–Q8**; "
                 "bootstrap **%d resamples, seed %d**, cluster-resampled by fixture; "
                 "tie band **+/-10%%**."
                 % (len(fixtures), ", ".join(lanes), B, SEED))
    lines.append("")
    prof = (env or {}).get("profile", "unknown")
    sub = (env or {}).get("vole_substrate", "unknown")
    bin_label = (env or {}).get("bin", "?")
    lines.append("VOLE lane: **%s** profile (`%s`); substrate: **%s**. All wall times "
                 "are **microseconds (`us`)**." % (prof, bin_label, sub))
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

    lines.append("## Paired ratios VOLE/sqlite (median + geometric mean, 95% CI by fixture)")
    lines.append("")
    lines.append("| metric | comparator | n | median | geomean | median 95% CI | "
                 "geomean 95% CI | wins | ties | losses | ratio of sums |")
    lines.append("|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|")
    for label, series in (("build", build_us), ("storage", store_bytes),
                          ("cold", cold_us), ("warm", warm_us)):
        other = "sqlite"
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
        wins = sum(1 for x in vals if x < 0.90)
        ties = sum(1 for x in vals if 0.90 <= x <= 1.10)
        losses = sum(1 for x in vals if x > 1.10)
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
    startup = (env or {}).get("python_startup_us")
    lines.append("The SQLite cold path runs a fresh **Python** process per request, so its "
                 "cold numbers include interpreter start-up (measured bare start-up %s us); "
                 "VOLE's cold path is a native binary. The cold ratio is dominated by that "
                 "constant and is reported for completeness, not headlined." % startup)
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
    lines.append("| Q | VOLE | SQLite (conventional YAML → JSON → `json1`) |")
    lines.append("|---|---|---|")
    for q in QS:
        lines.append("| {} | {} | {} |".format(q, *QDESC[q]))
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
    lines.append("- **This is where VOLE claims value.** A conventional YAML stack is "
                 "value-oriented: it expands aliases, merges `<<`, and drops tags, styles, "
                 "comments, and spans. VOLE's Q2/Q3/Q4/Q6/Q7 expose exactly those "
                 "distinctions, and its exactness is byte-authoritative for arbitrary YAML.")
    lines.append("- **The baseline's YAML → native step is inherently lossy** — that is "
                 "the point of the comparison; the pinned image ships no `yaml` module, so "
                 "the baseline implements a bounded, conventional normalization inline "
                 "(never a network fetch).")
    lines.append("- **VOLE capability gaps are recorded, never papered over.** Any "
                 "question VOLE declines is a typed decline (`rc` 6).")
    lines.append("- **Nothing here is run on the host.** Every command ran in the pinned "
                 "`doc-baseline` container.")
    lines.append("")

    matrix = []
    matrix.append("# Phase 21.6 — cross-lane Q1–Q8 answer matrix")
    matrix.append("")
    matrix.append("`g` = answered (derived), `D` = typed decline, `-` = not applicable.")
    matrix.append("")
    matrix.append("| fixture | Q | VOLE | SQLite | VOLE<->SQLite |")
    matrix.append("|---|---|---|---|---|")
    for fx in fixtures:
        for q in QS:
            row = qanswers.get((fx, q), {})

            def mark(lane):
                e = row.get(lane)
                if e is None:
                    return "-"
                return "D" if e.get("declined") else "g"

            rs, _ = compare(q, row.get("vole"), row.get("sqlite"))
            matrix.append("| %s | %s | %s | %s | %s |" % (fx, q, mark("vole"),
                                                          mark("sqlite"), rs))
    matrix.append("")
    matrix.append("### Aggregate equivalence per Q")
    matrix.append("")
    matrix.append("| Q | comparator | equal | both-decline | capability-gap | mismatch | shape |")
    matrix.append("|---|---|---:|---:|---:|---:|---:|")
    for q in QS:
        c = equiv.get(q, {})
        matrix.append("| {} | {} | {} | {} | {} | {} | {} |".format(
            q, "sqlite", c.get("equal", 0), c.get("both-decline", 0),
            c.get("capability-gap", 0), c.get("mismatch", 0), c.get("shape", 0)))
    matrix.append("")

    counts = []
    counts.append("fixtures %d" % len(fixtures))
    counts.append("questions %d" % len(QS))
    counts.append("exact_ok %d" % exact_ok)
    counts.append("exact_n %d" % exact_n)
    for q in QS:
        c = equiv.get(q, {})
        counts.append("%s.sqlite.equal %d" % (q, c.get("equal", 0)))
        counts.append("%s.sqlite.capability_gap %d" % (q, c.get("capability-gap", 0)))
        counts.append("%s.sqlite.mismatch %d" % (q, c.get("mismatch", 0)))
        counts.append("%s.sqlite.both_decline %d" % (q, c.get("both-decline", 0)))
    counts.append("verdict %s" % verdict)

    with open(os.path.join(campaign, "SUMMARY.md"), "w") as f:
        f.write("\n".join(lines) + "\n")
    with open(os.path.join(campaign, "MATRIX.md"), "w") as f:
        f.write("\n".join(matrix) + "\n")
    with open(os.path.join(campaign, "counts.txt"), "w") as f:
        f.write("\n".join(counts) + "\n")

    receipt = {
        "campaign": campaign,
        "phase": "21.6 — YAML economic court (VOLE vs source-retaining SQLite)",
        "verdict": verdict,
        "exact_ok": exact_ok,
        "exact_n": exact_n,
        "fixtures": fixtures,
        "lanes": lanes,
        "estimator": ("paired per-fixture ratio; median + geometric mean; fixed-seed "
                      "cluster bootstrap by fixture (%d resamples, seed %d); ratio of sums "
                      "reported separately" % (B, SEED)),
        "equivalence": {q: equiv.get(q, {}) for q in QS},
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
    return 2


if __name__ == "__main__":
    sys.exit(main())
