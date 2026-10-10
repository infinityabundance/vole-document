#!/usr/bin/env python3
# Phase 21.13 — EML economic court baselines.
#
# Two comparators:
#
#   * `sqlite`  — a **source-retaining** baseline: the raw bytes are stored as a
#                 SQLite BLOB (so it can answer `materialize`), plus derived tables
#                 (unfolded headers, decoded text bodies, decoded attachments, part
#                 count) parsed once at build via the stdlib `email` module.
#   * `conv`    — a **conventional `email.message_from_bytes` load**: it keeps a
#                 derived view only. It loses the *source* (so it cannot
#                 materialize) and every source offset/spelling, but the `email`
#                 module does decode base64/quoted-printable, so Q1–Q5 are
#                 answerable.
#
# Neither comparator records exact source spans, and neither preserves header
# order/duplicate headers as a *representation* claim; the court only asks
# questions both can answer, and records exact closure (Q6) as the byte-authority
# question. Runs on Python stdlib + sqlite3 only (the pinned `doc-baseline`
# service).
#
#   build --lane sqlite|conv --source F --out DIR
#   query --lane ... --dir DIR --q Qn --plan JSON --out OUT
#   session --lane ... --dir DIR --queries Q1,... --plan JSON --out OUT
#   materialize --lane sqlite --dir DIR --out OUT
#   aggregate --raw RAW --campaign CAMPAIGN --env ENV

import argparse
import email
import email.policy
import hashlib
import json
import os
import re
import sqlite3
import sys
import time


def now_us():
    return int(time.monotonic() * 1_000_000)


def envelope(q, value=None, *, declined=False, code=None, reason="", detail=None):
    e = {"q": q, "lane": "?", "declined": bool(declined),
         "value": None if declined else value, "detail": detail or {}}
    e["decline"] = {"code": code, "detail": reason} if declined else None
    return e


_FOLD = re.compile(r"\r?\n")


def unfold(raw):
    """Unfold a header value the same way the VOLE adapter does: join the
    trimmed non-empty lines with a single space."""
    if raw is None:
        return ""
    return " ".join(part.strip() for part in _FOLD.split(raw) if part.strip() != "")


# --- the shared EML derived view --------------------------------------------

def parse_view(source_bytes):
    """Parse `source_bytes` with the stdlib `email` module into a span-free derived
    view: the part count, unfolded headers, decoded text bodies, and decoded
    attachments."""
    msg = email.message_from_bytes(source_bytes, policy=email.policy.compat32)
    walk = list(msg.walk())
    headers = []
    bodies = []
    attachments = []
    for pi, part in enumerate(walk):
        for name, value in part.items():
            headers.append([pi, name, unfold(value)])
        if not part.is_multipart():
            ctype = part.get_content_type()
            payload = part.get_payload(decode=True)
            if payload is None:
                payload = b""
            if ctype.startswith("text/"):
                text = payload.decode("utf-8", "replace")
                bodies.append([pi, ctype, text])
            disp = part.get_content_disposition()
            fname = part.get_filename()
            if disp == "attachment" or fname:
                attachments.append({
                    "filename": fname,
                    "media_type": ctype,
                    "length": len(payload),
                    "sha256": hashlib.sha256(payload).hexdigest(),
                })
    body_text = ""
    for ctype_pref in ("text/plain", ""):
        for _pi, ctype, text in bodies:
            if ctype_pref and ctype != ctype_pref:
                continue
            body_text = text
            break
        if body_text:
            break
    return {
        "parts": len(walk),
        "headers": headers,
        "bodies": bodies,
        "attachments": attachments,
        "body_text": body_text,
    }


# --- build ------------------------------------------------------------------

def build(lane, source_path, out_dir):
    os.makedirs(out_dir, exist_ok=True)
    with open(source_path, "rb") as f:
        data = f.read()
    view = parse_view(data)
    if lane == "sqlite":
        db = os.path.join(out_dir, "t.sqlite")
        if os.path.exists(db):
            os.remove(db)
        con = sqlite3.connect(db)
        con.execute("CREATE TABLE source(id INTEGER PRIMARY KEY, blob BLOB)")
        con.execute("INSERT INTO source(id, blob) VALUES(1, ?)", (sqlite3.Binary(data),))
        con.execute("CREATE TABLE meta(parts INTEGER, body_text TEXT)")
        con.execute("INSERT INTO meta(parts, body_text) VALUES(?, ?)",
                    (view["parts"], view["body_text"]))
        con.execute("CREATE TABLE headers(part INTEGER, name TEXT, value TEXT)")
        for pi, name, value in view["headers"]:
            con.execute("INSERT INTO headers(part, name, value) VALUES(?,?,?)", (pi, name, value))
        con.execute("CREATE TABLE bodies(part INTEGER, media TEXT, text TEXT)")
        for pi, media, text in view["bodies"]:
            con.execute("INSERT INTO bodies(part, media, text) VALUES(?,?,?)", (pi, media, text))
        con.execute("CREATE TABLE attachments(ordinal INTEGER PRIMARY KEY, filename TEXT, "
                    "media TEXT, length INTEGER, sha256 TEXT)")
        for i, a in enumerate(view["attachments"]):
            con.execute("INSERT INTO attachments(ordinal, filename, media, length, sha256) "
                        "VALUES(?,?,?,?,?)",
                        (i, a["filename"], a["media_type"], a["length"], a["sha256"]))
        con.commit()
        con.close()
    else:  # conv
        with open(os.path.join(out_dir, "parsed.json"), "w") as f:
            json.dump(view, f, sort_keys=True)
    print(json.dumps({"lane": lane, "parts": view["parts"]}))


# --- query ------------------------------------------------------------------

def _find_headers(view, name):
    name_l = name.lower()
    for _pi, hn, hv in view["headers"]:
        if hn.lower() == name_l:
            return hv
    return None


def _find_count(view, pat):
    n = 0
    for _pi, hn, hv in view["headers"]:
        if pat in hn:
            n += 1
        if pat in hv:
            n += 1
    for _pi, _media, text in view["bodies"]:
        if pat in text:
            n += 1
    return n


def _view_query(view, d, q, plan):
    if q == "Q1":
        v = _find_headers(view, plan.get("head", "Subject"))
        if v is None:
            return envelope(q, declined=True, code="no-such-header", reason=q)
        return envelope(q, v)
    if q == "Q2":
        return envelope(q, view["body_text"])
    if q == "Q3":
        att = plan.get("att", "")
        if att == "":
            return envelope(q, declined=True, code="no-attachment", reason=q)
        i = int(att)
        if i < 0 or i >= len(view["attachments"]):
            return envelope(q, declined=True, code="no-attachment", reason=q)
        a = view["attachments"][i]
        return envelope(q, {"length": a["length"], "sha256": a["sha256"]})
    if q == "Q4":
        return envelope(q, view["parts"])
    if q == "Q5":
        return envelope(q, _find_count(view, plan.get("find", "")))
    if q == "Q6":
        return envelope(q, declined=True, code="not-native",
                        reason="a conventional load does not retain the source bytes")
    return envelope(q, declined=True, code="unknown-question", reason=q)


def _sqlite_query(db, d, q, plan):
    con = sqlite3.connect(db)
    try:
        if q == "Q1":
            row = con.execute("SELECT value FROM headers WHERE lower(name)=lower(?) LIMIT 1",
                              (plan.get("head", "Subject"),)).fetchone()
            if row is None:
                return envelope(q, declined=True, code="no-such-header", reason=q)
            return envelope(q, row[0])
        if q == "Q2":
            row = con.execute("SELECT body_text FROM meta WHERE rowid=1").fetchone()
            return envelope(q, row[0] if row else "")
        if q == "Q3":
            att = plan.get("att", "")
            if att == "":
                return envelope(q, declined=True, code="no-attachment", reason=q)
            row = con.execute("SELECT length, sha256 FROM attachments WHERE ordinal=?",
                              (int(att),)).fetchone()
            if row is None:
                return envelope(q, declined=True, code="no-attachment", reason=q)
            return envelope(q, {"length": row[0], "sha256": row[1]})
        if q == "Q4":
            row = con.execute("SELECT parts FROM meta WHERE rowid=1").fetchone()
            return envelope(q, row[0] if row else 0)
        if q == "Q5":
            pat = plan.get("find", "")
            n = con.execute("SELECT COUNT(*) FROM headers WHERE instr(name, ?) > 0", (pat,)).fetchone()[0]
            n += con.execute("SELECT COUNT(*) FROM headers WHERE instr(value, ?) > 0", (pat,)).fetchone()[0]
            n += con.execute("SELECT COUNT(*) FROM bodies WHERE instr(text, ?) > 0", (pat,)).fetchone()[0]
            return envelope(q, n)
        if q == "Q6":
            row = con.execute("SELECT blob FROM source WHERE id=1").fetchone()
            if row is None:
                return envelope(q, declined=True, code="no-source", reason=q)
            blob = bytes(row[0])
            return envelope(q, {"length": len(blob), "sha256": hashlib.sha256(blob).hexdigest()})
    finally:
        con.close()
    return envelope(q, declined=True, code="unknown-question", reason=q)


def _query(lane, d, q, plan):
    if lane == "sqlite":
        return _sqlite_query(os.path.join(d, "t.sqlite"), d, q, plan)
    with open(os.path.join(d, "parsed.json")) as f:
        view = json.load(f)
    return _view_query(view, d, q, plan)


def query(lane, d, q, plan, out):
    env = _query(lane, d, q, plan)
    env["lane"] = lane
    with open(out, "w") as f:
        json.dump(env, f, sort_keys=True)
    return 0


def session(lane, d, queries, plan, out):
    batch = []
    for q in queries.split(","):
        t0 = now_us()
        env = _query(lane, d, q, plan)
        dt = now_us() - t0
        env["lane"] = lane
        batch.append({"q": q, "us": dt, "declined": env["declined"]})
    with open(out, "w") as f:
        json.dump({"batch": batch}, f, sort_keys=True)
    return 0


def materialize(lane, d, out):
    if lane != "sqlite":
        return 1
    con = sqlite3.connect(os.path.join(d, "t.sqlite"))
    try:
        row = con.execute("SELECT blob FROM source WHERE id=1").fetchone()
    finally:
        con.close()
    if row is None:
        return 1
    with open(out, "wb") as f:
        f.write(bytes(row[0]))
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


QS = ["Q1", "Q2", "Q3", "Q4", "Q5", "Q6"]
TIE = 0.10
LANES = ["vole", "sqlite", "conv"]

QDESC = {
    "Q1": ("a header value (Subject)", "the received header value", "the received header value"),
    "Q2": ("a body part's text", "the decoded text/plain body", "the decoded text/plain body"),
    "Q3": ("an attachment's exact decoded bytes", "the decoded attachment BLOB",
           "the decoded attachment bytes"),
    "Q4": ("the MIME part count", "the parsed part count", "the parsed part count"),
    "Q5": ("a lexical find over headers + text bodies", "the matching count",
           "the matching count"),
    "Q6": ("`materialize --exact` (byte-authority)", "retained raw BLOB (byte-authority)",
           "**DECLINE** `not-native` (the source is not retained)"),
}


def compare(q, va, vb):
    if va is None or vb is None:
        return "missing", "no envelope"
    if va.get("declined") and vb.get("declined"):
        return "both-decline", "both typed declines"
    if va.get("declined") or vb.get("declined"):
        who = "comparator" if va.get("declined") else "vole"
        return "capability-gap", "%s declines" % who
    a, b = va.get("value"), vb.get("value")
    if isinstance(a, dict) and isinstance(b, dict) and "sha256" in a and "sha256" in b:
        ok = a.get("sha256") == b.get("sha256") and a.get("length") == b.get("length")
        return ("equal" if ok else "mismatch"), "length+sha256"
    return ("equal" if a == b else "mismatch"), "value"


def aggregate(raw, campaign, env_path=None):
    P19 = _load_p19()
    B = 10000
    SEED = 21131
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
    lanes = LANES

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

    def cold_by_fixture(lane):
        perrep = {}
        for r in cold_rows:
            if r["lane"] != lane or r.get("rc") != "0":
                continue
            key = (r["fixture"], r["rep"])
            perrep[key] = perrep.get(key, 0) + int(r["us"])
        out = {}
        for (fx, _rep), v in perrep.items():
            if fx not in out or v < out[fx]:
                out[fx] = v
        return out

    build_us = {lane: best_by_fixture(build_rows, lane) for lane in lanes}
    store_bytes = {lane: {} for lane in lanes}
    for r in storage_rows:
        store_bytes[r["lane"]][r["fixture"]] = int(r["bytes"])
    cold_us = {lane: cold_by_fixture(lane) for lane in lanes}
    warm_us = {lane: {} for lane in lanes}
    warm_perrep = {lane: {} for lane in lanes}
    for r in warm_rows:
        if r.get("rc") != "0":
            continue
        key = (r["fixture"], r["rep"])
        warm_perrep[r["lane"]][key] = warm_perrep[r["lane"]].get(key, 0) + int(r["us"])
    for lane in lanes:
        for (fx, _rep), v in warm_perrep[lane].items():
            if fx not in warm_us[lane] or v < warm_us[lane][fx]:
                warm_us[lane][fx] = v

    qanswers = {}
    equiv = {}
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
            for lane in ("sqlite", "conv"):
                res, _ = compare(q, row.get("vole"), row.get(lane))
                equiv.setdefault((q, lane), {}).setdefault(res, 0)
                equiv[(q, lane)][res] += 1

    exact_ok = sum(1 for r in exact_rows if r.get("vole_ok") in ("1", "true"))
    exact_n = len(exact_rows)

    lines = []
    lines.append("# Phase 21.13 — EML economic court")
    lines.append("")
    lines.append("**Question.** Against a source-retaining SQLite baseline *and* a "
                 "conventional `email.message_from_bytes` baseline, can VOLE answer the "
                 "same questions (Q1–Q6) it can answer, at comparable "
                 "build/storage/cold/warm cost, while closing the original EML "
                 "byte-exactly — and does it add value by **preserving representation** "
                 "(exact header spans and order, duplicate headers, folded header spans, "
                 "and the resolved MIME tree)?")
    lines.append("")
    lines.append("**Method.** A deterministic self-authored EML corpus "
                 "(`tools/fixtures/make-eml.py --corpus`) is regenerated at court time; "
                 "each fixture is ingested by three lanes (VOLE field CLI; a "
                 "source-retaining SQLite baseline; a conventional `email` load), Q1–Q6 "
                 "are asked of each, and build/storage/cold/warm are measured. Persistent "
                 "bytes are the **sum of regular-file sizes** (`find -type f -printf '%s'`), "
                 "never `du -sb` (ADR-0049).")
    lines.append("")
    lines.append("Corpus: **%d fixtures**; lanes **%s**; questions **Q1–Q6**; bootstrap "
                 "**%d resamples, seed %d**, cluster-resampled by fixture; tie band "
                 "**+/-%d%%**." % (len(fixtures), ", ".join(lanes), B, SEED, int(TIE * 100)))
    lines.append("")
    prof = (env or {}).get("profile", "unknown")
    sub = (env or {}).get("vole_substrate", "unknown")
    bin_label = (env or {}).get("bin", "?")
    lines.append("VOLE lane: **%s** profile (`%s`); substrate: **%s**. All wall times are "
                 "**microseconds (`us`)**." % (prof, bin_label, sub))
    lines.append("")
    lines.append("## Verdict")
    lines.append("")
    verdict = "PASS" if exact_ok == exact_n and exact_n > 0 else "FAIL"
    lines.append("- **VOLE exactness (Q6): %d/%d byte-exact** (length + SHA-256 + `cmp`, "
                 "after source + descriptor deletion in a fresh process)." % (exact_ok, exact_n))
    lines.append("- **COURT VERDICT: %s** (fails unless exactness is 100%% on the VOLE lane "
                 "for every fixture)." % verdict)
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

    lines.append("## Paired ratios VOLE/other (median + geometric mean, 95% CI by fixture)")
    lines.append("")
    lines.append("| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | "
                 "wins | ties | losses | ratio of sums |")
    lines.append("|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|")
    for label, series in (("build", build_us), ("storage", store_bytes),
                          ("cold", cold_us), ("warm", warm_us)):
        for other in ("sqlite", "conv"):
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
            lines.append("| {} | {} | {} | {:.3f} | {:.3f} | {:.3f}..{:.3f} | {:.3f}..{:.3f} | "
                         "{} | {} | {} | {:.3f} |".format(
                             label, other, len(vals), statistics.median(vals), P19.geomean(vals),
                             lo_m, hi_m, lo_g, hi_g, wins, ties, losses,
                             (sums_v / sums_o if sums_o else 0)))
    lines.append("")
    lines.append("A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, "
                 "summarised by the median and geometric mean with a fixed-seed cluster bootstrap "
                 "over fixtures; `ratio of sums` (a size-weighted pooled view) is reported "
                 "separately and named as such. With only %d fixture clusters the bootstrap is "
                 "coarse and is stated as such, not as a precise interval." % len(fixtures))
    lines.append("")
    startup = (env or {}).get("python_startup_us")
    lines.append("The SQLite and conv cold paths run a fresh **Python** process per request, "
                 "so their cold numbers include the interpreter start-up (measured bare start-up "
                 "%s us); VOLE's cold path is a native binary. The cold ratio is dominated by that "
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
    lines.append("| Q | VOLE | SQLite (source-retaining) | conv (`email` load) |")
    lines.append("|---|---|---|---|")
    for q in QS:
        lines.append("| {} | {} | {} | {} |".format(q, *QDESC[q]))
    lines.append("")

    lines.append("## Scope (honest)")
    lines.append("")
    lines.append("- **Self-authored deterministic corpus, NOT a real-world population.** "
                 "Fixtures are generated by `tools/fixtures/make-eml.py` (Python stdlib "
                 "only). Every claim is scoped to these files; the aggregate carries a "
                 "fixture-clustered CI.")
    lines.append("- **Only Q6 is a byte-authority claim.** `materialize --exact == source` "
                 "(length + SHA-256 + `cmp`) reproduces the original bytes. Every other "
                 "observation is a DERIVED projection; semantic agreement is not archival "
                 "equality.")
    lines.append("- **Where VOLE claims value.** Neither comparator exposes exact header "
                 "source spans, header order as a representation, duplicate headers as "
                 "distinct entries, folded-header spans, or the resolved MIME tree with "
                 "exact `Content-Transfer-Encoding` constituent spans; the `email` module "
                 "normalizes all of that. VOLE's native `eml-header`/`eml-part`/`eml-find` "
                 "expose exactly those, and its exactness is byte-authoritative.")
    lines.append("- **VOLE capability gaps are recorded, never papered over.** Any question "
                 "VOLE declines is a typed decline (`rc` 6).")
    lines.append("- **Nothing here is run on the host.** Every command ran in the pinned "
                 "`doc-baseline` container.")
    lines.append("")

    matrix = []
    matrix.append("# Phase 21.13 — cross-lane Q1–Q6 answer matrix")
    matrix.append("")
    matrix.append("`g` = answered (derived), `D` = typed decline, `-` = not applicable.")
    matrix.append("")
    matrix.append("| fixture | Q | VOLE | SQLite | conv | VOLE↔SQLite | VOLE↔conv |")
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
            rr, _ = compare(q, row.get("vole"), row.get("conv"))
            matrix.append("| %s | %s | %s | %s | %s | %s | %s |" % (
                fx, q, mark("vole"), mark("sqlite"), mark("conv"), rs, rr))
    matrix.append("")
    matrix.append("### Aggregate equivalence per Q")
    matrix.append("")
    matrix.append("| Q | comparator | equal | both-decline | capability-gap | mismatch |")
    matrix.append("|---|---|---:|---:|---:|---:|")
    for q in QS:
        for lane in ("sqlite", "conv"):
            c = equiv.get((q, lane), {})
            matrix.append("| {} | {} | {} | {} | {} | {} |".format(
                q, lane, c.get("equal", 0), c.get("both-decline", 0),
                c.get("capability-gap", 0), c.get("mismatch", 0)))
    matrix.append("")

    counts = []
    counts.append("fixtures %d" % len(fixtures))
    counts.append("questions %d" % len(QS))
    counts.append("exact_ok %d" % exact_ok)
    counts.append("exact_n %d" % exact_n)
    for q in QS:
        for lane in ("sqlite", "conv"):
            c = equiv.get((q, lane), {})
            counts.append("%s.%s.equal %d" % (q, lane, c.get("equal", 0)))
            counts.append("%s.%s.capability_gap %d" % (q, lane, c.get("capability-gap", 0)))
            counts.append("%s.%s.mismatch %d" % (q, lane, c.get("mismatch", 0)))
            counts.append("%s.%s.both_decline %d" % (q, lane, c.get("both-decline", 0)))
    counts.append("verdict %s" % verdict)

    with open(os.path.join(campaign, "SUMMARY.md"), "w") as f:
        f.write("\n".join(lines) + "\n")
    with open(os.path.join(campaign, "MATRIX.md"), "w") as f:
        f.write("\n".join(matrix) + "\n")
    with open(os.path.join(campaign, "counts.txt"), "w") as f:
        f.write("\n".join(counts) + "\n")

    receipt = {
        "campaign": campaign,
        "phase": "21.13 — EML economic court (VOLE vs SQLite vs conventional email load)",
        "verdict": verdict,
        "exact_ok": exact_ok,
        "exact_n": exact_n,
        "fixtures": fixtures,
        "lanes": lanes,
        "estimator": "paired per-fixture ratio; median + geometric mean; fixed-seed cluster "
                     "bootstrap by fixture (%d resamples, seed %d); tie band +/-%d%%; ratio of "
                     "sums reported separately" % (B, SEED, int(TIE * 100)),
        "equivalence": {"%s.%s" % (q, o): equiv.get((q, o), {}) for q in QS for o in ("sqlite", "conv")},
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
    b.add_argument("--lane", required=True)
    b.add_argument("--source", required=True)
    b.add_argument("--out", required=True)
    q = sub.add_parser("query")
    q.add_argument("--lane", required=True)
    q.add_argument("--dir", required=True)
    q.add_argument("--q", required=True)
    q.add_argument("--plan", default="{}")
    q.add_argument("--out", required=True)
    s = sub.add_parser("session")
    s.add_argument("--lane", required=True)
    s.add_argument("--dir", required=True)
    s.add_argument("--queries", required=True)
    s.add_argument("--plan", default="{}")
    s.add_argument("--out", required=True)
    m = sub.add_parser("materialize")
    m.add_argument("--lane", required=True)
    m.add_argument("--dir", required=True)
    m.add_argument("--out", required=True)
    a = sub.add_parser("aggregate")
    a.add_argument("--raw", required=True)
    a.add_argument("--campaign", required=True)
    a.add_argument("--env", default=None)
    ns = ap.parse_args(argv)
    if ns.cmd == "build":
        return build(ns.lane, ns.source, ns.out)
    if ns.cmd == "query":
        return query(ns.lane, ns.dir, ns.q, json.loads(ns.plan), ns.out)
    if ns.cmd == "session":
        return session(ns.lane, ns.dir, ns.queries, json.loads(ns.plan), ns.out)
    if ns.cmd == "materialize":
        return materialize(ns.lane, ns.dir, ns.out)
    if ns.cmd == "aggregate":
        return aggregate(ns.raw, ns.campaign, ns.env)
    return 2


if __name__ == "__main__":
    sys.exit(main())
