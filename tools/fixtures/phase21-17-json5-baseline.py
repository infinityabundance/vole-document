#!/usr/bin/env python3
# Phase 21.17 (economic court) — the two conventional comparators for the JSON5 /
# JSONC economic court. Runs on Python stdlib + `sqlite3` only (the pinned
# `doc-baseline` service; no third-party module, no network).
#
#   * `sqlite` — a **source-retaining** baseline. It keeps the original bytes
#     verbatim (a `raw` BLOB, so it can answer `materialize` / byte authority) AND
#     an **extracted strict-JSON text** obtained by normalizing the JSON5/JSONC
#     source with the vendored loader (`json5mini`). The normalization preserves
#     member order and **duplicate keys**, converts JSON5 spellings to strict JSON
#     (`0xFF`->`255`, `.5`->`0.5`), maps `Infinity`->`1e999`, and **strips
#     comments**; `NaN` has **no** strict-JSON representation, so a source that
#     contains it cannot be normalized at all and every JSON question declines
#     typed (a real limitation, recorded, not papered over). Structure/kinds are
#     answered with SQLite's built-in JSON functions (`json_extract` / `json_type`
#     / `json_tree`); no source span, key span, exact token spelling, comment span,
#     or recorded dialect is preserved by SQLite (the dialect is stored as a
#     separate column, so it *is* answerable).
#   * `conv`    — a **conventional JSON5 -> object load** (the vendored loader):
#     it keeps only a derived host-value view. It drops the source bytes, every
#     source offset, comments, duplicate keys, the recorded dialect, and exact
#     numeric/escape spelling, so it must decline typed on all of those.
#
#   build       --lane sqlite|conv --source FILE --out DIR
#   query       --lane ... --dir DIR --q Qn --plan JSON --out FILE
#   session     --lane ... --dir DIR --queries Q1,... --plan JSON --out FILE
#   materialize --lane sqlite --dir DIR --out FILE
#   version
#   aggregate   --raw DIR --campaign DIR --env ENV_JSON

import argparse
import hashlib
import json
import math
import os
import re
import sys
import time

import json5mini

SIMPLE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
IDX = re.compile(r"^(0|[1-9][0-9]*)$")


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

KIND_MAP = {
    "text": "string",
    "integer": "number",
    "real": "number",
    "true": "true",
    "false": "false",
    "null": "null",
    "object": "object",
    "array": "array",
}


def norm_kind(t):
    return KIND_MAP.get(t, t)


def kind_of_py(v):
    if isinstance(v, bool):
        return "true" if v else "false"
    if v is None:
        return "null"
    if isinstance(v, dict):
        return "object"
    if isinstance(v, list):
        return "array"
    if isinstance(v, str):
        return "string"
    return "number"


def canon_py(kind, v):
    """Canonical string for a host value, matching the VOLE lane's canonical form."""
    if kind == "string":
        return v if isinstance(v, str) else str(v)
    if kind == "number":
        if isinstance(v, bool):
            return "true" if v else "false"
        if isinstance(v, float):
            if math.isnan(v):
                return "nan"
            return repr(v)
        return str(v)
    return kind


# --- pointer helpers ---------------------------------------------------------


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


def build(lane, source_path, out_dir):
    os.makedirs(out_dir, exist_ok=True)
    with open(source_path, "rb") as f:
        raw = f.read()
    text = raw.decode("utf-8")
    if lane == "sqlite":
        norm_ok = 1
        note = "ok"
        try:
            val, dialect = json5mini.parse(text, pairs=True)
            norm = json5mini.dump_strict(val)
        except json5mini.JSON5Error as e:
            val = None
            _, dialect = _dialect_only(text)
            norm = None
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
                    "dialect TEXT, norm_ok INTEGER, note TEXT)")
        con.execute("INSERT INTO doc VALUES(1,?,?,?,?,?)",
                    (con_sql.Binary(raw), norm, dialect, norm_ok, note))
        con.commit()
        con.close()
        print(json.dumps({"ok": True, "lane": lane, "src_len": len(raw),
                          "norm_ok": norm_ok, "dialect": dialect, "note": note},
                         sort_keys=True))
    else:  # conv
        val, dialect = json5mini.parse(text, pairs=False)
        with open(os.path.join(out_dir, "parsed.json"), "w") as f:
            json.dump({"value": val}, f, allow_nan=True)
        print(json.dumps({"ok": True, "lane": lane, "src_len": len(raw),
                          "dialect": dialect}, sort_keys=True))
    return 0


def _dialect_only(text):
    try:
        return None, json5mini.parse(text, pairs=True)[1]
    except json5mini.JSON5Error:
        return None, None


def _sqlite():
    import sqlite3
    return sqlite3


# --- queries -----------------------------------------------------------------


def _decl(q, code, reason):
    return envelope(q, declined=True, code=code, reason=reason)


def _sqlite_query(db, q, plan):
    con_sql = _sqlite()
    con = con_sql.connect(db)
    try:
        row = con.execute("SELECT raw, j, dialect, norm_ok, note FROM doc WHERE id=1").fetchone()
        raw, j, dialect, norm_ok, note = row[0], row[1], row[2], row[3], row[4]
        if q == "Q11":
            if dialect is None:
                return _decl(q, "no-dialect", note or "no recorded dialect")
            return envelope(q, dialect, detail={"source": "stored dialect column"})
        if q in ("Q1", "Q3", "Q4", "Q5", "Q7") and not norm_ok:
            return _decl(q, "no-strict-json-representation",
                         "JSON5 source cannot be normalized to strict JSON: %s" % note)
        if q in ("Q1", "Q5"):
            ptr = plan["value_ptr"] if q == "Q1" else plan["array_ptr"]
            path = to_sqlite_path(ptr)
            t = con.execute("SELECT json_type(?,?)", (j, path)).fetchone()[0]
            if t is None:
                return _decl(q, "missing", "no node at path %s" % ptr)
            v = con.execute("SELECT json_extract(?,?)", (j, path)).fetchone()[0]
            kind = norm_kind(t)
            return envelope(q, {"kind": kind, "value": canon_py(kind, v)},
                            detail={"json_type": t, "normalized": True})
        if q == "Q2":
            return _decl(q, "no-source-span", "SQLite exposes no source byte span")
        if q == "Q3":
            path = to_sqlite_path(plan["kind_ptr"])
            t = con.execute("SELECT json_type(?,?)", (j, path)).fetchone()[0]
            if t is None:
                return _decl(q, "missing", "no node at path")
            return envelope(q, norm_kind(t), detail={"json_type": t})
        if q == "Q4":
            path = to_sqlite_path(plan["dup_ptr"])
            n = con.execute("SELECT count(*) FROM json_tree(?) WHERE fullkey = ?",
                            (j, path)).fetchone()[0]
            return envelope(q, {"exists": n > 0, "duplicate_count": n},
                            detail={"method": "json_tree fullkey count; duplicates enumerated"})
        if q == "Q6":
            return envelope(q, {"length": len(raw), "sha256": sha256_hex(raw)})
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
        if q == "Q8":
            return _decl(q, "re-serialized-not-exact",
                         "SQLite re-serializes a value; the source token spelling is not preserved")
        if q == "Q9":
            return _decl(q, "comments-stripped",
                         "comments are removed by the JSON5->strict-JSON normalization")
        if q == "Q10":
            return _decl(q, "no-key-span",
                         "SQLite exposes no key byte span or key kind")
        if q == "Q12":
            return _decl(q, "normalized-not-exact-spelling",
                         "numbers are normalized by the strict-JSON text")
        return _decl(q, "unknown-question", q)
    finally:
        con.close()


def _conv_load(d):
    with open(os.path.join(d, "parsed.json")) as f:
        return json.load(f)["value"]


def _conv_find(root, pat):
    out = []

    def walk(node, ptr):
        if isinstance(node, dict):
            for k, v in node.items():
                if pat in k:
                    out.append({"pointer": ptr + "/" + _escape_seg(k), "role": "key", "text": k})
                walk(v, ptr + "/" + _escape_seg(k))
        elif isinstance(node, list):
            for i, v in enumerate(node):
                walk(v, ptr + "/" + str(i))
        elif isinstance(node, str):
            if pat in node:
                out.append({"pointer": ptr, "role": "value", "text": node})

    walk(root, "")
    out.sort(key=lambda m: (m["pointer"], m["role"], m["text"]))
    return out


def _conv_query(d, q, plan):
    root = _conv_load(d)
    if q in ("Q1", "Q5"):
        ptr = plan["value_ptr"] if q == "Q1" else plan["array_ptr"]
        ok, v = resolve_pointer(root, ptr)
        if not ok:
            return _decl(q, "missing", "no node at path %s" % ptr)
        kind = kind_of_py(v)
        if kind in ("object", "array"):
            return _decl(q, "not-a-scalar", "the plan targets a scalar")
        return envelope(q, {"kind": kind, "value": canon_py(kind, v)},
                        detail={"host-value": True})
    if q == "Q2":
        return _decl(q, "no-source-span",
                     "a conventional JSON5->object load keeps no source span")
    if q == "Q3":
        ok, v = resolve_pointer(root, plan["kind_ptr"])
        if not ok:
            return _decl(q, "missing", "no node at path")
        return envelope(q, kind_of_py(v))
    if q == "Q4":
        return _decl(q, "duplicates-collapsed",
                     "a conventional object load collapses duplicate keys (last wins)")
    if q == "Q6":
        return _decl(q, "no-source-bytes",
                     "a conventional JSON5->object load does not retain the source bytes")
    if q == "Q7":
        out = _conv_find(root, plan["find_pat"])
        return envelope(q, out, detail={"count": len(out)})
    if q == "Q8":
        return _decl(q, "host-value-not-token",
                     "a conventional load yields host values, not the exact source token")
    if q == "Q9":
        return _decl(q, "comments-dropped",
                     "a conventional JSON5 load drops comments")
    if q == "Q10":
        return _decl(q, "no-key-span",
                     "a conventional load keeps no key span or key kind")
    if q == "Q11":
        return _decl(q, "dialect-not-recorded",
                     "a conventional JSON5->object load does not record jsonc-vs-json5")
    if q == "Q12":
        return _decl(q, "spelling-lost",
                     "a conventional load parses numbers into host values, losing spelling")
    return _decl(q, "unknown-question", q)


def run_query(lane, d, q, plan):
    if lane == "sqlite":
        env = _sqlite_query(os.path.join(d, "x.sqlite"), q, plan)
    else:
        env = _conv_query(d, q, plan)
    env["lane"] = lane
    return env


def query(lane, d, q, plan, out):
    env = run_query(lane, d, q, plan)
    with open(out, "w") as f:
        json.dump(env, f, sort_keys=True)
    return 0


def session(lane, d, queries, plan, out):
    batch = []
    for q in queries.split(","):
        q = q.strip()
        if not q:
            continue
        t0 = now_us()
        env = run_query(lane, d, q, plan)
        dt = now_us() - t0
        batch.append({"q": q, "us": dt, "declined": env["declined"]})
    with open(out, "w") as f:
        json.dump({"batch": batch}, f, sort_keys=True)
    return 0


def materialize(lane, d, out):
    con_sql = _sqlite()
    con = con_sql.connect(os.path.join(d, "x.sqlite"))
    try:
        raw = con.execute("SELECT raw FROM doc WHERE id=1").fetchone()[0]
    finally:
        con.close()
    with open(out, "wb") as f:
        f.write(bytes(raw))
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


TIE = 0.10
QS = ["Q1", "Q2", "Q3", "Q4", "Q5", "Q6", "Q7", "Q8", "Q9", "Q10", "Q11", "Q12"]


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
            ok = a.get("kind") == b.get("kind") and a.get("value") == b.get("value")
            return ("equal" if ok else "mismatch"), "kind+canonical value"
        return "shape", "not dict"
    if q in ("Q4", "Q6", "Q8", "Q12", "Q9", "Q10"):
        if isinstance(a, dict) and isinstance(b, dict):
            return ("equal" if a == b else "mismatch"), "dict"
        return "shape", "not dict"
    if q == "Q2":
        return ("equal" if list(a or []) == list(b or []) else "mismatch"), "span"
    if q == "Q3":
        return ("equal" if str(a) == str(b) else "mismatch"), "kind"
    if q == "Q7":
        def norm(m):
            return (m.get("pointer"), m.get("role"), m.get("text"))
        return ("equal" if sorted(map(norm, a or [])) == sorted(map(norm, b or []))
                else "mismatch"), "match-set"
    if q == "Q11":
        return ("equal" if str(a) == str(b) else "mismatch"), "dialect"
    return ("equal" if a == b else "mismatch"), "default"


QDESC = {
    "Q1": ("a scalar at a pointer (exact token spelling)",
           "`json_extract` value (normalized)", "host value"),
    "Q2": ("the exact source span of a node",
           "no source span exists -> typed decline", "no source span -> typed decline"),
    "Q3": ("a node's kind", "`json_type` (normalized)", "host-value kind"),
    "Q4": ("key existence + duplicate-key count",
           "`json_tree` fullkey count (duplicates **enumerated**)",
           "duplicate keys collapsed -> typed decline"),
    "Q5": ("an array element value at an index",
           "`json_extract` at the index (normalized)", "host value"),
    "Q6": ("`materialize --exact` (byte-authority)",
           "retained raw BLOB (byte-authority)", "no source bytes -> typed decline"),
    "Q7": ("lexical find over keys/strings (with spans)",
           "`json_tree` scan over keys/strings (no spans)",
           "host-structure walk (no spans)"),
    "Q8": ("the exact raw string token bytes at a pointer",
           "`json()` re-serialization (spelling lost) -> typed decline",
           "host value (token lost) -> typed decline"),
    "Q9": ("comment count + exact comment spans (JSON5-only)",
           "comments stripped by normalization -> typed decline",
           "comments dropped -> typed decline"),
    "Q10": ("an unquoted/plain key's exact span + key kind (JSON5-only)",
            "no key span exists -> typed decline", "no key span -> typed decline"),
    "Q11": ("the recorded dialect (jsonc vs json5, JSON5-only)",
            "stored dialect column", "dialect not recorded -> typed decline"),
    "Q12": ("the exact raw numeric token at a pointer (JSON5-only)",
            "numbers normalized -> typed decline", "numbers parsed -> typed decline"),
}


def aggregate(raw, campaign, env_path=None):
    P19 = _load_p19()
    B = 10000
    SEED = 21721
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
        key = (fx, rep)
        warm_perrep[lane][key] = warm_perrep[lane].get(key, 0) + int(r["us"])
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

    lines = []
    lines.append("# Phase 21.17 — JSON5 / JSONC economic court")
    lines.append("")
    lines.append("**Question.** Against two conventional comparators — a "
                 "source-retaining SQLite store that must first normalize JSON5 to "
                 "*strict* JSON, and a conventional JSON5 -> object load — on "
                 "contract-equivalent terms, can VOLE answer the same twelve "
                 "questions (Q1–Q12) it can answer, while closing the original JSON5 "
                 "byte-exactly — and does it add value by **preserving representation** "
                 "(comments, source spans, key spans, duplicate keys, and exact "
                 "numeric/escape spelling)?")
    lines.append("")
    lines.append("**Method.** A deterministic self-authored JSON5/JSONC corpus "
                 "(`tools/fixtures/make-json5.py --corpus`) is regenerated at court "
                 "time; each fixture is ingested by **three lanes** (VOLE field CLI "
                 "with `--profile runtime --packed`; a source-retaining SQLite baseline "
                 "that keeps the raw bytes and queries `json_extract`/`json_type`/"
                 "`json_tree` on a JSON5->strict-JSON normalization; and a conventional "
                 "JSON5->object load via a vendored pure-Python loader), Q1–Q12 are "
                 "asked of each, and build/storage/cold/warm are measured. Persistent "
                 "bytes are the **sum of regular-file sizes** (`find -type f -printf "
                 "'%s'`), never `du -sb` (ADR-0049). Wall times are **microseconds "
                 "(`us`)**.")
    lines.append("")
    lines.append("Corpus: **%d fixtures**; lanes **%s**; questions **Q1–Q12**; "
                 "bootstrap **%d resamples, seed %d**, cluster-resampled by fixture; "
                 "tie band **+/-%d%%**."
                 % (len(fixtures), ", ".join(lanes), B, SEED, int(TIE * 100)))
    lines.append("")
    prof = (env or {}).get("profile", "unknown")
    sub = (env or {}).get("vole_substrate", "unknown")
    bin_label = (env or {}).get("bin", "?")
    lines.append("VOLE lane: **%s** profile (`%s`); substrate: **%s**. The comparators "
                 "(SQLite C + Python, and the pure-Python JSON5 load) are unaffected by "
                 "the Rust profile while the entropyfs build is not, so the release "
                 "default keeps the comparison fair to VOLE."
                 % (prof, bin_label, sub))
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
                 "conventional JSON5->object |")
    lines.append("|---|---|---|---|")
    for q in QS:
        lines.append("| {} | {} | {} | {} |".format(q, *QDESC[q]))
    lines.append("")

    lines.append("## Comparison normalization (contract-equivalence)")
    lines.append("")
    lines.append("- **Q1/Q5 (value):** compared as `(kind, canonical value)`. VOLE's "
                 "canonical value decodes strings and canonicalizes numbers (`0xFF` -> "
                 "`255`, `.5` -> `0.5`, `Infinity` -> `inf`) via the same vendored "
                 "loader, so a *value* answer is compared on value, not on spelling; the "
                 "spelling difference is captured separately by Q8/Q12. A spelling "
                 "difference is therefore NOT counted as a Q1/Q5 mismatch.")
    lines.append("- **Q3 (kind):** SQLite `json_type` is normalized to the VOLE "
                 "vocabulary (`text`->`string`, `integer`/`real`->`number`).")
    lines.append("- **Q4 (duplicates):** the SQLite lane's normalization preserves "
                 "duplicate members, so `json_tree` counts them exactly; the conventional "
                 "load collapses them and declines. A duplicate-key **count** difference "
                 "(e.g. in the `dup` fixture) is a recorded mismatch, not hidden.")
    lines.append("- **Q7 (find):** the match set is compared as `(pointer, role, text)`; "
                 "SQLite `fullkey` is normalized to an RFC 6901 pointer.")
    lines.append("- **Q8/Q12 (token spelling):** VOLE returns the exact source token "
                 "bytes; both comparators decline (SQLite re-serializes; the conventional "
                 "load yields host values).")
    lines.append("- **Q9/Q10/Q11 (JSON5-only):** comment spans, key spans, and the "
                 "recorded dialect are VOLE's representation surface; the comparators "
                 "decline, except the SQLite lane's **stored dialect column** (Q11).")
    lines.append("")

    lines.append("## Scope (honest)")
    lines.append("")
    lines.append("- **Self-authored deterministic corpus, NOT a real-world population.** "
                 "The fixtures are generated by `tools/fixtures/make-json5.py` (Python "
                 "stdlib only). Every claim is scoped to these files; the aggregate "
                 "carries a fixture-clustered CI and is not extrapolated.")
    lines.append("- **Only Q6 is a byte-authority claim.** `materialize --exact == "
                 "source` (length + SHA-256 + `cmp`) and the retained blob reproduce the "
                 "original bytes. Every other observation is a DERIVED projection "
                 "(`Q_gen`, `exact:false`); semantic agreement is not archival equality.")
    lines.append("- **The strict-JSON boundary is a real comparator limitation.** A "
                 "source-retaining store fronted by *strict* JSON cannot represent "
                 "JSON5's `Infinity`/`NaN` at all: `Infinity` is mapped to `1e999`, and a "
                 "source containing `NaN` cannot be normalized, so that lane declines "
                 "every JSON question for it (recorded, e.g. the `numbers` fixture).")
    lines.append("- **The conventional load is deliberately the weaker comparator.** "
                 "It is a JSON5->object load (not a span-preserving scanner): it drops "
                 "spans, comments, duplicate keys, key spans, the dialect, and spelling. "
                 "A *span-preserving* JSON5 loader could in principle match VOLE on "
                 "Q2/Q8/Q10/Q12; such a lane is **not** built here and no claim is made "
                 "against it — the honest differentiator this court measures is exact "
                 "closure (Q6) plus the representation surface against these two "
                 "comparators.")
    lines.append("- **VOLE capability gaps are recorded, never papered over.** Any "
                 "question VOLE declines is a typed decline (rc 6) and appears as a "
                 "`capability-gap`, never claimed as equivalence.")
    lines.append("- **Nothing here is run on the host.** Every command ran in a pinned "
                 "container (dev toolchain + python3 + sqlite3).")
    lines.append("")

    matrix = []
    matrix.append("# Phase 21.17 — cross-lane Q1–Q12 answer matrix")
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
        "phase": ("21.17 — JSON5/JSONC economic court "
                  "(VOLE vs source-retaining strict-JSON SQLite + conventional JSON5 load)"),
        "verdict": verdict,
        "exact_ok": exact_ok,
        "exact_n": exact_n,
        "fixtures": fixtures,
        "lanes": lanes,
        "estimator": ("paired per-fixture ratio; median + geometric mean; fixed-seed "
                      "cluster bootstrap by fixture (%d resamples, seed %d); tie band "
                      "+/-%d%%; ratio of sums reported separately"
                      % (B, SEED, int(TIE * 100))),
        "equivalence": {q: equiv.get(q, {}) for q in QS},
        "comparators": comparators,
        "ratios": ratio_report,
        "environment": env,
    }
    with open(os.path.join(campaign, "receipt.json"), "w") as f:
        json.dump(receipt, f, indent=2, sort_keys=True)
    print("\n".join(lines))
    return 0 if verdict == "PASS" else 1


def version():
    con_sql = _sqlite()
    print(json.dumps({"module": "sqlite3", "sqlite_version": con_sql.sqlite_version,
                      "json5mini": "vendored-pure-python"}, sort_keys=True))
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)

    b = sub.add_parser("build")
    b.add_argument("--lane", required=True, choices=["sqlite", "conv"])
    b.add_argument("--source", required=True)
    b.add_argument("--out", required=True)

    q = sub.add_parser("query")
    q.add_argument("--lane", required=True, choices=["sqlite", "conv"])
    q.add_argument("--dir", required=True)
    q.add_argument("--q", required=True)
    q.add_argument("--plan", default="{}")
    q.add_argument("--out", required=True)

    s = sub.add_parser("session")
    s.add_argument("--lane", required=True, choices=["sqlite", "conv"])
    s.add_argument("--dir", required=True)
    s.add_argument("--queries", required=True)
    s.add_argument("--plan", default="{}")
    s.add_argument("--out", required=True)

    m = sub.add_parser("materialize")
    m.add_argument("--lane", required=True, choices=["sqlite", "conv"])
    m.add_argument("--dir", required=True)
    m.add_argument("--out", required=True)

    ag = sub.add_parser("aggregate")
    ag.add_argument("--raw", required=True)
    ag.add_argument("--campaign", required=True)
    ag.add_argument("--env", default=None)

    sub.add_parser("version")

    ns = ap.parse_args(argv)
    if ns.cmd == "build":
        return build(ns.lane, ns.source, ns.out)
    if ns.cmd == "query":
        return query(ns.lane, ns.dir, ns.q, json.loads(ns.plan), ns.out)
    if ns.cmd == "session":
        return session(ns.lane, ns.dir, ns.queries, json.loads(ns.plan), ns.out)
    if ns.cmd == "materialize":
        if ns.lane != "sqlite":
            return 2
        return materialize(ns.lane, ns.dir, ns.out)
    if ns.cmd == "aggregate":
        return aggregate(ns.raw, ns.campaign, ns.env)
    if ns.cmd == "version":
        return version()
    return 2


if __name__ == "__main__":
    sys.exit(main())
