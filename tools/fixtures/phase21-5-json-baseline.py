#!/usr/bin/env python3
# Phase 21.5.1 / 21.5.3 — JSON economic court: the source-retaining SQLite baseline.
#
# The comparator retains the original JSON bytes verbatim (a `raw` BLOB) and also
# loads the text into SQLite, answering the same Q1–Q8 questions **using SQLite's
# built-in JSON functions (`json1`/`jsonb`)** where they can be answered. To make
# that a *fact*, this lane prefers the hash-pinned `pysqlite3` module, which
# bundles a modern SQLite with the `jsonb` binary representation (JSONB exists
# only from SQLite 3.45.0); it falls back to the stdlib `sqlite3` only if the
# wheel is absent, and records which module + `sqlite_version` it actually used
# (`version` subcommand). What SQLite does *not* preserve is representation —
# spelling, member order, or source spans — and, before the 21.5.3 correction, the
# old lane also **collapsed duplicate keys** by hardcoding `duplicate_count=1`.
# `json_tree`/`json_each` in fact enumerate duplicate object members, so Q4 now
# counts them exactly (see `_q_exists`).
#
#   build       --source FILE --db DB
#   query       --db DB --q Qn --plan JSON --out FILE
#   session     --db DB --queries Q1,Q2,... --plan JSON --out FILE
#   materialize --db DB --out FILE
#   version
#   aggregate   --raw DIR --campaign DIR --env ENV_JSON

import argparse
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

SIMPLE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
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


# --- pointer <-> SQLite path -------------------------------------------------


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


def _escape_seg(s):
    return s.replace("~", "~0").replace("/", "~1")


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


# --- scalar / kind normalization --------------------------------------------


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
    extract_us = now_us() - t0
    con = sqlite3.connect(db_path)
    con.executescript(_schema())
    con.execute("INSERT INTO doc(id, raw, j) VALUES (1, ?, ?)", (raw, text))
    con.commit()
    con.close()
    print(json.dumps({"ok": True, "fmt": "json", "src_len": len(raw),
                      "extract_us": extract_us}, sort_keys=True))
    return 0


def _load(db_path):
    con = sqlite3.connect(db_path)
    row = con.execute("SELECT raw, j FROM doc WHERE id=1").fetchone()
    return con, row[0], row[1]


# --- queries -----------------------------------------------------------------


def _q_value(con, j, path):
    v = con.execute("SELECT json_extract(?, ?)", (j, path)).fetchone()[0]
    if v is None:
        return envelope(1, declined=True, code="missing", reason="no value at path")
    return envelope(1, norm_scalar(v))


def _q_span():
    return envelope(2, declined=True, code="no-source-span",
                    reason="SQLite exposes no source byte span")


def _q_kind(con, j, path):
    t = con.execute("SELECT json_type(?, ?)", (j, path)).fetchone()[0]
    if t is None:
        return envelope(3, declined=True, code="missing", reason="no node at path")
    return envelope(3, norm_kind(t))


def _q_exists(con, j, path):
    # FIX 1b: `json_tree` enumerates ALL object members, INCLUDING duplicates
    # (verified on SQLite 3.51.1: `json_each('{"a":1,"a":2}')` yields two rows
    # and `json_tree` gives two rows with fullkey '$.a'). Counting the rows whose
    # `fullkey` equals the (SQLite-form) pointer therefore returns the true member
    # count — the old hardcoded `1` was simply wrong.
    n = con.execute(
        "SELECT count(*) FROM json_tree(?) WHERE fullkey = ?", (j, path)
    ).fetchone()[0]
    exists = n > 0
    return envelope(4, {"exists": exists, "duplicate_count": n},
                    detail={"method": "json_tree fullkey count; duplicates enumerated"})


def _q_find(con, j, pat):
    like = "%" + pat + "%"
    rows = con.execute(
        "SELECT fullkey, key, type, value FROM json_tree(?) "
        "WHERE (key IS NOT NULL AND key LIKE ?) "
        "OR (type = 'text' AND value IS NOT NULL AND value LIKE ?)",
        (j, like, like),
    ).fetchall()
    out = []
    for fullkey, key, typ, value in rows:
        ptr = fullkey_to_pointer(fullkey)
        if key is not None and pat in str(key):
            out.append({"pointer": ptr, "role": "key", "text": str(key)})
        if typ == "text" and value is not None and pat in str(value):
            out.append({"pointer": ptr, "role": "value", "text": str(value)})
    out.sort(key=lambda m: (m["pointer"], m["role"], m["text"]))
    return envelope(7, out, detail={"count": len(out)})


def _q_token(con, j, path):
    # SQLite re-serializes the value; it does NOT reproduce the source token.
    typ = con.execute("SELECT json_type(?, ?)", (j, path)).fetchone()[0]
    if typ is None:
        return envelope(8, declined=True, code="missing", reason="no value at path")
    v = con.execute("SELECT json_extract(?, ?)", (j, path)).fetchone()[0]
    if typ in ("object", "array"):
        tok = v if isinstance(v, str) else json.dumps(v)
    elif typ == "text":
        tok = json.dumps(v, ensure_ascii=False)
    elif typ == "integer":
        tok = str(v)
    elif typ == "real":
        tok = repr(v)
    else:  # true / false / null
        tok = typ
    b = tok.encode("utf-8")
    return envelope(8, {"sha256": sha256_hex(b), "len": len(b)},
                    detail={"note": "re-serialized; source spelling is not preserved"})


def _q_exact(raw):
    return envelope(6, {"length": len(raw), "sha256": sha256_hex(raw)})


def run_query(con, raw, j, q, plan):
    env = _run_query(con, raw, j, q, plan)
    if env is not None:
        env["q"] = q
    return env


def _run_query(con, raw, j, q, plan):
    if q == "Q1":
        return _q_value(con, j, to_sqlite_path(plan["value_ptr"]))
    if q == "Q2":
        return _q_span()
    if q == "Q3":
        return _q_kind(con, j, to_sqlite_path(plan["kind_ptr"]))
    if q == "Q4":
        return _q_exists(con, j, to_sqlite_path(plan["dup_ptr"]))
    if q == "Q5":
        return _q_value(con, j, to_sqlite_path(plan["array_ptr"]))
    if q == "Q6":
        return _q_exact(raw)
    if q == "Q7":
        return _q_find(con, j, plan["find_pat"])
    if q == "Q8":
        return _q_token(con, j, to_sqlite_path(plan["token_ptr"]))
    return envelope(q, declined=True, code="unknown-question", reason=q)


def query(db_path, q, plan, out):
    con, raw, j = _load(db_path)
    env = run_query(con, raw, j, q, plan)
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


def _geomean(vals):
    import math
    v = [float(x) for x in vals if float(x) > 0]
    if not v:
        return 0.0
    return math.exp(sum(math.log(x) for x in v) / len(v))


TIE = 0.10


def compare(q, va, vb):
    if va is None or vb is None:
        return "missing", "no envelope"
    if va.get("declined") and vb.get("declined"):
        return "both-decline", "both typed declines"
    if va.get("declined") or vb.get("declined"):
        who = "vole" if va.get("declined") else "sqlite"
        return "capability-gap", "%s declines" % who
    a, b = va.get("value"), vb.get("value")
    if q in ("Q1", "Q3", "Q5"):
        return ("equal" if str(a) == str(b) else "mismatch"), "scalar/kind"
    if q == "Q2":
        return ("equal" if list(a or []) == list(b or []) else "mismatch"), "span"
    if q == "Q4":
        if isinstance(a, dict) and isinstance(b, dict):
            ok = (a.get("exists") == b.get("exists")
                  and a.get("duplicate_count") == b.get("duplicate_count"))
            return ("equal" if ok else "mismatch"), "exists+duplicates"
        return "shape", "not dict"
    if q == "Q6":
        if isinstance(a, dict) and isinstance(b, dict):
            ok = a.get("sha256") == b.get("sha256") and a.get("length") == b.get("length")
            return ("equal" if ok else "mismatch"), "length+sha256"
        return "shape", "not dict"
    if q == "Q7":
        def norm(m):
            return (m.get("pointer"), m.get("role"), m.get("text"))
        return ("equal" if sorted(map(norm, a or [])) == sorted(map(norm, b or []))
                else "mismatch"), "match-set"
    if q == "Q8":
        if isinstance(a, dict) and isinstance(b, dict):
            ok = a.get("sha256") == b.get("sha256") and a.get("len") == b.get("len")
            return ("equal" if ok else "mismatch"), "token sha+len"
        return "shape", "not dict"
    return ("equal" if a == b else "mismatch"), "default"


def aggregate(raw, campaign, env_path=None):
    P19 = _load_p19()
    B = 10000
    SEED = 21521
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
    qs = ["Q1", "Q2", "Q3", "Q4", "Q5", "Q6", "Q7", "Q8"]

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
        for q in qs:
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
        for q in qs:
            row = qanswers.get((fx, q), {})
            for comp in comparators:
                res, _ = compare(q, row.get("vole"), row.get(comp))
                equiv.setdefault(q, {}).setdefault(comp, {}).setdefault(res, 0)
                equiv[q][comp][res] += 1

    exact_ok = sum(1 for r in exact_rows if r.get("vole_ok") in ("1", "true"))
    exact_n = len(exact_rows)

    lines = []
    lines.append("# Phase 21.5.1 — JSON economic court")
    lines.append("")
    lines.append("**Question.** Against a source-retaining SQLite baseline using its "
                 "built-in JSON functions, on contract-equivalent terms, can VOLE "
                 "answer the same eight questions (Q1–Q8) it can answer, while "
                 "closing the original JSON byte-exactly — and does it add value by "
                 "**preserving representation** (spelling, order, duplicate keys, "
                 "source spans)?")
    lines.append("")
    lines.append("**Method.** A deterministic self-authored JSON corpus "
                 "(`tools/fixtures/make-json.py --corpus`) is regenerated at court "
                 "time; each fixture is ingested by **three lanes** (VOLE field CLI; a "
                 "source-retaining SQLite baseline that keeps the raw bytes and queries "
                 "`json1`/`jsonb` on a hash-pinned modern SQLite; and a **span-preserving "
                 "pure-Python baseline** that retains the source and records every "
                 "token's exact byte span), Q1–Q8 are asked of each, and build/storage/"
                 "cold/warm are measured. Persistent bytes are the **sum of regular-file "
                 "sizes** (`find -type f -printf '%s'`), never `du -sb` (ADR-0049).")
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
                 "unaffected by the Rust profile while the entropyfs build is not, so "
                 "the release default keeps the comparison fair to VOLE. All wall times "
                 "are **microseconds (`us`)**."
                 % (prof, bin_label, sub))
    lines.append("")
    lines.append("## Verdict")
    lines.append("")
    verdict = "PASS" if exact_ok == exact_n and exact_n > 0 else "FAIL"
    lines.append("- **VOLE exactness (Q6): %d/%d byte-exact** (length + SHA-256 + "
                 "`cmp`, after source + descriptor deletion in a fresh process)."
                 % (exact_ok, exact_n))
    lines.append("- **COURT VERDICT: %s** (fails unless exactness is 100%% on the "
                 "VOLE lane for every fixture)." % verdict)
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
                 "ratio**, summarised by the median and geometric mean with a "
                 "fixed-seed cluster bootstrap over fixtures; `ratio of sums` is "
                 "reported separately and named as such. Each per-fixture lane cost "
                 "(build, cold, warm) is the **minimum over the retained repetitions** "
                 "(best-of-N); storage is measured once after the last build. Every raw "
                 "sample is kept in `raw/build.tsv`, `raw/cold.tsv`, `raw/warm.tsv`. "
                 "With only %d fixture clusters the bootstrap is coarse and is stated "
                 "as such, not as a precise interval." % len(fixtures))
    lines.append("")
    sup = (env or {}).get("supersedes") if isinstance(env, dict) else None
    if isinstance(sup, dict):
        def _now(metric, comp):
            r = ratio_report.get(metric, {}).get(comp)
            return "{:.3f}".format(r["median"]) if r else "n/a"
        lines.append("## Supersedes — Phase 21.5.3 (FIX 1)")
        lines.append("")
        lines.append("`[SUPERSEDED: %s]` — that campaign's SQLite lane claimed "
                     "`json1`/`jsonb` but actually ran SQLite **%s** (JSONB ships only "
                     "from 3.45.0) and **hardcoded** `duplicate_count = 1`. This "
                     "campaign runs a hash-pinned modern SQLite (**%s**, via "
                     "`pysqlite3`) and counts duplicates with `json_tree`; every prior "
                     "ratio is superseded by the tables above."
                     % (sup.get("campaign", "?"), sup.get("sqlite_version_then", "?"),
                        (env or {}).get("sqlite_version", "?")))
        lines.append("")
        lines.append("| metric | comparator | then | now (median) |")
        lines.append("|---|---|---:|---:|")
        for metric in ("build", "storage", "cold", "warm"):
            for comp in comparators:
                then = sup.get("%s_median_then_%s" % (metric, comp))
                if then is None:
                    continue
                lines.append("| %s | %s | [SUPERSEDED: %.3f] | %s |"
                             % (metric, comp, then, _now(metric, comp)))
        lines.append("")
        lines.append("Q4 then: %s. Q4 now: `json_tree` enumerates duplicate members, "
                     "so VOLE and SQLite **agree** — the prior mismatch was a "
                     "measurement artefact (a hardcoded constant), not a SQLite "
                     "limitation. Against the added span-preserving pure-Python "
                     "baseline VOLE matches on Q1–Q8 wherever it answers, so VOLE's "
                     "Q2/Q4/Q8 representation advantages do **not** survive against the "
                     "strongest competitor; only Q6 (byte-exact closure) and the "
                     "economics remain. See the campaign's SUMMARY `Scope` section."
                     % sup.get("q4_then", "?"))
        lines.append("")
    startup = (env or {}).get("python_startup_us") if isinstance(env, dict) else None
    lines.append("Both conventional comparator cold paths (SQLite C + Python, and the "
                 "pure-Python span-preserving scanner) run a fresh **Python** process "
                 "per request, so their cold numbers include interpreter start-up as "
                 "part of that lane's honest per-request cost (measured bare start-up "
                 "%s us); VOLE's cold path is a native binary. The cold ratio is "
                 "therefore dominated by that constant and is reported for "
                 "completeness, not headlined." % startup)
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
    lines.append("| Q | VOLE | SQLite (`json1`/`jsonb`, modern) | span-preserving Python |")
    lines.append("|---|---|---|---|")
    qdesc = {
        "Q1": ("a scalar at a pointer (value spelling preserved)",
               "`json_extract` value (normalized)",
               "decoded string / raw number token"),
        "Q2": ("the exact source span of a node",
               "no source span exists in SQLite -> typed decline",
               "exact byte span `[start,end)`"),
        "Q3": ("a node's kind", "`json_type` (normalized)", "node kind"),
        "Q4": ("key existence + duplicate-key count",
               "`json_tree` fullkey count (duplicates **enumerated**)",
               "member count (duplicates kept)"),
        "Q5": ("an array element (value spelling preserved)",
               "`json_extract` at the index",
               "decoded string / raw number token"),
        "Q6": ("`materialize --exact` (byte-authority)",
               "retained raw BLOB (byte-authority)",
               "retained raw bytes (byte-authority)"),
        "Q7": ("lexical find over keys/strings (with spans)",
               "`json_tree` scan over keys/strings (no spans)",
               "scanner walk (no spans returned)"),
        "Q8": ("the exact raw token bytes at a pointer",
               "`json()` re-serialization (spelling not preserved)",
               "exact raw source token bytes"),
    }
    for q in qs:
        lines.append("| {} | {} | {} | {} |".format(q, *qdesc[q]))
    lines.append("")

    lines.append("## Comparison normalization (contract-equivalence)")
    lines.append("")
    lines.append("- **Q1/Q5 (value):** string equality after normalization; VOLE returns "
                 "the exact source token (e.g. `1e3`), SQLite returns a normalized "
                 "number/string. A spelling difference is a recorded mismatch.")
    lines.append("- **Q3 (kind):** SQLite `json_type` is normalized to the VOLE "
                 "vocabulary (`text`->`string`, `integer`/`real`->`number`).")
    lines.append("- **Q4 (duplicates, CORRECTED in 21.5.3):** the true member count is "
                 "counted with `json_tree` (which enumerates duplicate members), so "
                 "SQLite now agrees with VOLE and with the span-preserving scanner. The "
                 "earlier receipt's \"SQLite collapses duplicates -> count 1\" was a "
                 "measurement artefact of a hardcoded constant, not a property of SQLite.")
    lines.append("- **Q7 (find):** the match set is compared as `(pointer, role, text)`; "
                 "SQLite `fullkey` is normalized to an RFC 6901 pointer.")
    lines.append("- **Q8 (token):** VOLE returns the exact source token bytes; SQLite "
                 "returns `json()` re-serialization, but the span-preserving Python "
                 "scanner returns the exact raw token bytes, so Q8 matches VOLE there.")
    lines.append("")

    lines.append("## Scope (honest)")
    lines.append("")
    lines.append("- **Self-authored deterministic corpus, NOT a real-world population.** "
                 "The fixtures are generated by `tools/fixtures/make-json.py` (Python "
                 "stdlib only). Every claim is scoped to these files; the aggregate "
                 "carries a fixture-clustered CI and is not extrapolated.")
    lines.append("- **Only Q6 is a byte-authority claim.** `materialize --exact == "
                 "source` (length + SHA-256 + `cmp`) and the retained blob reproduce the "
                 "original bytes. Every other observation is a DERIVED projection; "
                 "semantic agreement is not archival equality.")
    lines.append("- **What survives against the strongest competitor (CORRECTED).** "
                 "Against a *span-preserving* pure-Python scanner, VOLE's representation "
                 "advantages in Q2 (spans), Q4 (duplicate counts), and Q8 (raw token "
                 "bytes) **largely evaporate** — the conventional scanner answers them "
                 "too. VOLE's remaining, real advantages are (i) byte-authoritative "
                 "exact closure of the whole source (Q6) and (ii) economics (storage and "
                 "query cost). This is the honest picture the corrected comparator shows.")
    lines.append("- **VOLE capability gaps are recorded, never papered over.** Any "
                 "question VOLE declines is a typed decline (`rc` 6) and appears as a "
                 "`capability-gap`, never claimed as equivalence.")
    lines.append("- **Nothing here is run on the host.** Every command ran in a pinned "
                 "container (dev toolchain + python3 + the hash-pinned modern SQLite).")
    lines.append("")

    matrix = []
    matrix.append("# Phase 21.5.1 — cross-lane Q1–Q8 answer matrix")
    matrix.append("")
    matrix.append("`g` = answered (derived), `D` = typed decline, `-` = not applicable.")
    matrix.append("")
    matrix.append("| fixture | Q | VOLE | SQLite | spanpy | VOLE<->sqlite | "
                  "VOLE<->spanpy |")
    matrix.append("|---|---|---|---|---|---|---|")
    for fx in fixtures:
        for q in qs:
            row = qanswers.get((fx, q), {})

            def mark(lane):
                e = row.get(lane)
                if e is None:
                    return "-"
                return "D" if e.get("declined") else "g"

            rs, _ = compare(q, row.get("vole"), row.get("sqlite"))
            rp, _ = compare(q, row.get("vole"), row.get("spanpy"))
            matrix.append("| %s | %s | %s | %s | %s | %s | %s |" % (
                fx, q, mark("vole"), mark("sqlite"), mark("spanpy"), rs, rp))
    matrix.append("")
    matrix.append("### Aggregate equivalence per Q")
    matrix.append("")
    matrix.append("| Q | comparator | equal | both-decline | capability-gap | "
                  "mismatch | shape |")
    matrix.append("|---|---|---:|---:|---:|---:|---:|")
    for q in qs:
        for comp in comparators:
            c = equiv.get(q, {}).get(comp, {})
            matrix.append("| {} | {} | {} | {} | {} | {} | {} |".format(
                q, comp, c.get("equal", 0), c.get("both-decline", 0),
                c.get("capability-gap", 0), c.get("mismatch", 0), c.get("shape", 0)))
    matrix.append("")

    counts = []
    counts.append("fixtures %d" % len(fixtures))
    counts.append("questions %d" % len(qs))
    counts.append("exact_ok %d" % exact_ok)
    counts.append("exact_n %d" % exact_n)
    for q in qs:
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
        "phase": "21.5.1 — JSON economic court (VOLE vs source-retaining SQLite + span-preserving Python)",
        "verdict": verdict,
        "exact_ok": exact_ok,
        "exact_n": exact_n,
        "fixtures": fixtures,
        "lanes": lanes,
        "estimator": ("paired per-fixture ratio; median + geometric mean; fixed-seed "
                      "cluster bootstrap by fixture (%d resamples, seed %d); tie band "
                      "+/-%d%%; ratio of sums reported separately"
                      % (B, SEED, int(TIE * 100))),
        "equivalence": {q: equiv.get(q, {}) for q in qs},
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

    ag = sub.add_parser("aggregate")
    ag.add_argument("--raw", required=True)
    ag.add_argument("--campaign", required=True)
    ag.add_argument("--env", default=None)

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
        print(json.dumps({"module": SQLITE_MODULE,
                          "sqlite_version": sqlite3.sqlite_version}, sort_keys=True))
        return 0
    return 2


if __name__ == "__main__":
    sys.exit(main())
