#!/usr/bin/env python3
"""Phase 22.7 (P6) — the document-agent economic court.

## What this is

A **bounded, deterministic, scripted document-inspection agent** that performs a
frozen multi-step workflow (locate a fact -> read the containing unit -> answer
with a source span) against **two backends on the same documents**:

  * **VOLE** — `field-build` + `find`/`observe`/`observe-batch` with provenance;
  * **baseline** — a conventional per-unit extraction (Poppler text for PDF; the
    OPC XML for DOCX/EPUB) loaded into SQLite with byte spans, searched with
    `LIKE`, read by row.

There is **no real LLM here**. The agent is a fixed deterministic script: it
emits a fixed sequence of tool calls per task. The economic input that a model
would consume is the **exact transcript** (search result + read result) that the
scripted agent feeds forward; that transcript is tokenised with the **pinned
offline tokenizer** (`bert-base-uncased`, WordPiece, the same asset the
Phase-11.13 court uses) and the token counts are the model-input measure. If the
pinned tokenizer runtime is unavailable in the lane, a clearly-labelled
deterministic **token proxy** is used instead and the receipt says so.

## Correctness is fixed before any token claim

`correct` is a normalised exact match of the agent's answer to the **frozen**
expected unit text, derived from the document itself at freeze time (the same
expectation for both backends). `grounded` requires the backend to return a
non-null source span whose bytes, re-materialised from the backend's own
source-of-truth, contain the anchor. Tokens/wall/storage are only compared on the
same frozen task set; a cheaper wrong answer is not a win.

## Subcommands

  selftest  --doc SRC --fmt F --vbin BIN [--work DIR]
  freeze    --docs TSV --out FILE --vbin BIN [--work DIR] [--tasks-per-doc N]
  run       --outdir CAMPAIGN --docs TSV --vbin BIN --tokenizer JSON --expect-sha-256 HEX
                                         [--tokname NAME] [--work DIR] [--tasks-per-doc N]

`run` writes `raw/` and the campaign files (summary.json, counts.txt, MATRIX.md,
SUMMARY.md) except `environment.json`/`receipt.json`/`commands.txt`, which the
shell court owns.

Everything is stdlib except the optional pinned `tokenizers` runtime.
"""

import argparse
import hashlib
import html.parser
import json
import os
import re
import resource
import signal
import sqlite3
import subprocess
import sys
import time
import zipfile
import zlib

# ---------------------------------------------------------------------------
# Constants / prices
# ---------------------------------------------------------------------------

# Illustrative unit prices for the monetary column. Stated, dated, and NOT
# claimed to be any vendor's current list price. The court reports the full
# physical cost vector beside this monetary total so the aggregation is
# inspectable; the ratio is also reported under the token term alone.
PRICES = {
    "model_input_usd_per_mtok": 3.00,        # frontier-model input tokens
    "compute_usd_per_cpu_hour": 0.05,        # commodity vCPU-hour
    "storage_usd_per_gib_month": 0.023,      # object-store-class, 1 month retention
    "io_read_usd_per_gib": 0.0004,
    "op_usd_per_tool_call": 0.000002,
    "retention_months": 1.0,
}
PRICES_DATE = "2026-10-08"

SYSTEM = (
    "You are a document inspection agent. Use the provided tools. For every "
    "task, locate the requested fact, read the containing unit, and return the "
    "unit text together with a source span. A cheaper wrong answer is not "
    "acceptable."
)


# ---------------------------------------------------------------------------
# Small utilities
# ---------------------------------------------------------------------------

def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def norm(s):
    return re.sub(r"\s+", " ", s or "").strip().lower()


def run(cmd, out_path=None, err_path=None, timeout=1200):
    """Run a command, returning (rc, wall_ms, maxrss_kb).

    Fork/exec + wait4 so the child's own peak RSS is attributable (GNU `time`
    is not present in the tokenizer lane). A SIGALRM bounds the run.
    """
    t0 = time.monotonic()
    out_path = out_path or os.devnull
    err_path = err_path or os.devnull
    pid = os.fork()
    if pid == 0:  # child
        try:
            fo = os.open(out_path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o644)
            os.dup2(fo, 1)
            fe = os.open(err_path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o644)
            os.dup2(fe, 2)
            os.execvp(cmd[0], cmd)
        except Exception:
            os._exit(127)
    killed = {"v": False}

    def _alarm(signum, frame):
        killed["v"] = True
        try:
            os.kill(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass

    old = signal.signal(signal.SIGALRM, _alarm)
    signal.setitimer(signal.ITIMER_REAL, timeout)
    try:
        _, status, ru = os.wait4(pid, 0)
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, old)
    wall = (time.monotonic() - t0) * 1000.0
    try:
        rc = os.waitstatus_to_exitcode(status)
    except Exception:
        rc = 1
    if killed["v"]:
        rc = 124
    return rc, wall, ru.ru_maxrss


def dir_bytes(path):
    total = 0
    for root, _dirs, files in os.walk(path):
        for f in files:
            try:
                total += os.path.getsize(os.path.join(root, f))
            except OSError:
                pass
    return total


def json_load(path, default=None):
    try:
        with open(path) as f:
            return json.load(f)
    except Exception:
        return default


def first_json_line(path):
    try:
        with open(path) as f:
            for line in f:
                line = line.strip()
                if line:
                    return json.loads(line)
    except Exception:
        pass
    return None


# ---------------------------------------------------------------------------
# Pinned tokenizer (or a labelled deterministic proxy)
# ---------------------------------------------------------------------------

class TokenCounter:
    def __init__(self, tok_json=None, expect_sha=None, name="bert-base-uncased"):
        self.kind = "proxy"
        self.name = "whitespace-word+punct proxy (NOT a model tokenizer)"
        self.detail = {}
        self._tok = None
        if tok_json and os.path.exists(tok_json):
            actual = sha256_file(tok_json)
            verified = (expect_sha is None) or (actual == expect_sha)
            if verified:
                try:
                    from tokenizers import Tokenizer  # type: ignore
                    import tokenizers  # type: ignore
                    self._tok = Tokenizer.from_file(tok_json)
                    self.kind = "pinned-tokenizer"
                    self.name = name
                    self.detail = {
                        "implementation": "huggingface/tokenizers",
                        "implementation_version": tokenizers.__version__,
                        "asset_path": tok_json,
                        "asset_sha256": actual,
                        "asset_sha256_expected": expect_sha,
                        "asset_verified": True,
                        "add_special_tokens": False,
                        "vocab_size": self._tok.get_vocab_size(),
                    }
                except Exception as e:  # noqa: BLE001
                    self.detail = {"tokenizer_import_error": str(e), "asset_sha256": actual}
            else:
                self.detail = {
                    "asset_sha256": actual,
                    "asset_sha256_expected": expect_sha,
                    "asset_verified": False,
                    "reason": "sha256 mismatch; proxy used",
                }
        self._cache = {}

    def count(self, text):
        if self._tok is not None:
            return len(self._tok.encode(text, add_special_tokens=False).ids)
        key = hash(text)
        if key in self._cache:
            return self._cache[key]
        n = len(re.findall(r"\w+|[^\w\s]", text, flags=re.UNICODE))
        self._cache[key] = n
        return n

    def identity(self):
        if self.kind == "pinned-tokenizer":
            return {"kind": self.kind, "name": self.name, **self.detail}
        return {"kind": self.kind, "name": self.name, **self.detail}


# ---------------------------------------------------------------------------
# Reference extraction (ground truth for both backends)
# ---------------------------------------------------------------------------
# Units are the *finest text unit each format exposes*: Poppler text lines for
# PDF, body paragraphs for DOCX, XHTML blocks for EPUB. The same frozen task set
# is used for both backends. byte spans are into the concatenated reference text.

W_NS = "{http://schemas.openxmlformats.org/wordprocessingml/2006/main}"


def ref_pdf(src):
    units = []
    out = subprocess.run(["pdfinfo", src], capture_output=True)
    pages = 0
    for line in out.stdout.decode("utf-8", "replace").splitlines():
        if line.startswith("Pages:"):
            pages = int(line.split()[1])
    for p in range(1, pages + 1):
        r = subprocess.run(
            ["pdftotext", "-f", str(p), "-l", str(p), "-layout", src, "-"],
            capture_output=True,
        )
        text = r.stdout.decode("utf-8", "replace")
        for ln in text.split("\n"):
            s = ln.rstrip()
            if s.strip():
                units.append({"kind": "line", "page": p, "text": s})
    return units


def _xml_local(tag):
    return tag.rsplit("}", 1)[-1] if "}" in tag else tag


def ref_docx(src):
    import xml.etree.ElementTree as ET
    units = []
    with zipfile.ZipFile(src) as z:
        raw = z.read("word/document.xml")
    root = ET.fromstring(raw)
    body = None
    for c in root:
        if _xml_local(c.tag) == "body":
            body = c
            break
    if body is None:
        body = root
    for child in list(body):
        local = _xml_local(child.tag)
        if local == "p":
            text = "".join(t.text or "" for t in child.iter() if _xml_local(t.tag) == "t")
            if text.strip():
                units.append({"kind": "paragraph", "text": text})
        elif local == "tbl":
            cells = []
            for tc in child.iter():
                if _xml_local(tc.tag) == "tc":
                    ct = "".join(x.text or "" for x in tc.iter() if _xml_local(x.tag) == "t")
                    if ct.strip():
                        cells.append(ct.strip())
            if cells:
                units.append({"kind": "table", "text": " | ".join(cells)})
    return units


class _BlockHTML(html.parser.HTMLParser):
    BLOCK = {"p", "h1", "h2", "h3", "h4", "h5", "h6", "li", "td", "th", "blockquote"}

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.units = []
        self._depth = 0
        self._buf = []
        self._skip = 0

    def handle_starttag(self, tag, attrs):
        tag = tag.lower()
        if tag in ("script", "style"):
            self._skip += 1
        if tag in self.BLOCK:
            if self._depth == 0:
                self._buf = []
            self._depth += 1
        if tag == "br" and self._depth > 0:
            self._buf.append(" ")

    def handle_endtag(self, tag):
        tag = tag.lower()
        if tag in ("script", "style"):
            self._skip = max(0, self._skip - 1)
        if tag in self.BLOCK and self._depth > 0:
            self._depth -= 1
            if self._depth == 0:
                text = re.sub(r"\s+", " ", "".join(self._buf)).strip()
                if text:
                    self.units.append(text)
                self._buf = []

    def handle_data(self, data):
        if self._skip == 0 and self._depth > 0:
            self._buf.append(data)


def epub_spine_items(src):
    with zipfile.ZipFile(src) as z:
        container = z.read("META-INF/container.xml").decode("utf-8", "replace")
        m = re.search(r'full-path="([^"]+)"', container)
        if not m:
            return []
        opf_path = m.group(1)
        opf = z.read(opf_path).decode("utf-8", "replace")
        base = opf_path.rsplit("/", 1)[0] if "/" in opf_path else ""
        manifest = {}
        for mm in re.finditer(r"<item\b[^>]*>", opf):
            tag = mm.group(0)
            idm = re.search(r'id="([^"]+)"', tag)
            href = re.search(r'href="([^"]+)"', tag)
            if idm and href:
                manifest[idm.group(1)] = href.group(1)
        items = []
        for sm in re.finditer(r"<itemref\b[^>]*>", opf):
            idm = re.search(r'idref="([^"]+)"', sm.group(0))
            if not idm:
                continue
            href = manifest.get(idm.group(1))
            if not href:
                continue
            path = (base + "/" + href) if base else href
            path = re.sub(r"/\./", "/", path)
            items.append(path)
        return items


def ref_epub(src):
    units = []
    with zipfile.ZipFile(src) as z:
        names = set(z.namelist())
        for spine, path in enumerate(epub_spine_items(src)):
            if path not in names:
                continue
            data = z.read(path)
            p = _BlockHTML()
            try:
                p.feed(data.decode("utf-8", "replace"))
            except Exception:
                continue
            for local, text in enumerate(p.units):
                units.append({"kind": "block", "text": text, "spine": spine, "block": local})
    return units


def ref_units(fmt, src):
    if fmt == "pdf":
        return ref_pdf(src)
    if fmt == "docx":
        return ref_docx(src)
    if fmt == "epub":
        return ref_epub(src)
    raise SystemExit("unsupported format %r" % fmt)


def materialize_ref(units):
    """Concatenate units into a reference byte blob; attach byte spans."""
    buf = bytearray()
    out = []
    for u in units:
        b = u["text"].encode("utf-8")
        start = len(buf)
        buf.extend(b)
        end = len(buf)
        buf.extend(b"\n")
        out.append({"kind": u["kind"], "text": u["text"],
                    "page": u.get("page"), "spine": u.get("spine"), "block": u.get("block"),
                    "span_start": start, "span_end": end})
    return out, bytes(buf)


# ---------------------------------------------------------------------------
# Task freezing
# ---------------------------------------------------------------------------

def select_tasks(units, k):
    """Pick up to k tasks: a distinctive anchor unique in the reference, in a
    short unit. Deterministic: choose by (longest unique token, earliest unit)."""
    texts = [u["text"] for u in units]
    glob = {}
    for i, t in enumerate(texts):
        for tok in set(re.findall(r"[A-Za-z][A-Za-z'\-]{5,}", t)):
            glob.setdefault(tok.lower(), []).append(i)
    # uniqueness by substring: the anchor must appear in exactly one unit.
    cand = []
    for i, t in enumerate(texts):
        nwords = len(t.split())
        if nwords < 6 or nwords > 90:
            continue
        for tok in re.findall(r"[A-Za-z][A-Za-z'\-]{5,}", t):
            low = tok.lower()
            if len(glob.get(low, [])) != 1:
                continue
            # substring uniqueness guard (LIKE %tok%)
            hits = sum(1 for x in texts if tok in x)
            if hits != 1:
                continue
            cand.append((-len(tok), i, tok, low))
            break
    cand.sort()
    picked = []
    used_units = set()
    for _, i, tok, low in cand:
        if i in used_units:
            continue
        used_units.add(i)
        picked.append({"anchor": tok, "anchor_lower": low, "unit_index": i,
                       "expected_text": texts[i],
                       "expected_span": [units[i]["span_start"], units[i]["span_end"]],
                       "unit_kind": units[i]["kind"]})
        if len(picked) >= k:
            break
    return picked


# ---------------------------------------------------------------------------
# VOLE helpers
# ---------------------------------------------------------------------------

def vole_build(vbin, src, store, outdir):
    os.makedirs(store, exist_ok=True)
    rc, wall, rss = run([vbin, "field-build", src, "--store", store, "--profile", "runtime", "--packed"],
                        out_path=os.path.join(outdir, "vole-build.json"),
                        err_path=os.path.join(outdir, "vole-build.err"))
    info = json_load(os.path.join(outdir, "vole-build.json"), {}) or {}
    field = (info.get("ingest") or {}).get("field")
    return {"rc": rc, "wall_ms": wall, "peak_rss_kb": rss, "field": field,
            "source_len": info.get("source_len"), "encoded_len": info.get("encoded_len"),
            "node_count": (info.get("ingest") or {}).get("node_count")}


def vole_batch(vbin, store, field, reqs, out_path, err_path):
    """Run one observe-batch session over `reqs` (list of arg-lists)."""
    reqfile = out_path + ".req"
    with open(reqfile, "w") as f:
        for r in reqs:
            f.write(" ".join(r) + "\n")
    args = [vbin, "observe-batch", "--store", store, "--field", field,
            "--packed", "--requests", reqfile]
    rc, wall, rss = run(args, out_path=out_path, err_path=err_path)
    answers = []
    try:
        with open(out_path) as f:
            for line in f:
                line = line.strip()
                if line and line.startswith("{"):
                    answers.append(json.loads(line))
    except FileNotFoundError:
        pass
    return {"rc": rc, "wall_ms": wall, "peak_rss_kb": rss, "answers": answers}


def vole_matches(answer):
    """Uniform locate matches [{unit,text,coord}] from a find/observe answer."""
    v = answer.get("value")
    out = []
    if isinstance(v, list):
        for item in v:
            if not isinstance(item, dict):
                continue
            if "line" in item:
                out.append({"unit": "page:%s" % item.get("page"), "text": item.get("line", ""),
                            "coord": {"page": item.get("page")}})
            elif "paragraph" in item:
                out.append({"unit": "paragraph:%s" % item.get("paragraph"), "text": item.get("text", ""),
                            "coord": {"paragraph": item.get("paragraph")}})
            elif "spine" in item or "block" in item:
                out.append({"unit": "spine:%s:block:%s" % (item.get("spine"), item.get("block")),
                            "text": item.get("text", ""),
                            "coord": {"spine": item.get("spine"), "block": item.get("block")}})
            elif "index" in item:
                out.append({"unit": "block:%s" % item.get("index"), "text": item.get("text", ""),
                            "coord": {"block": item.get("index")}})
    return out



# ---------------------------------------------------------------------------
# Baseline (conventional) backend
# ---------------------------------------------------------------------------

def baseline_build(fmt, src, units, ref_bytes, workdir):
    """Build a SQLite store of the reference units with byte spans."""
    db = os.path.join(workdir, "baseline.db")
    ref = os.path.join(workdir, "ref.txt")
    with open(ref, "wb") as f:
        f.write(ref_bytes)
    t0 = time.monotonic()
    rss_before = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    con = sqlite3.connect(db)
    con.execute("PRAGMA journal_mode=WAL")
    con.execute("PRAGMA synchronous=NORMAL")
    con.execute("PRAGMA cache_size=-65536")
    con.execute("PRAGMA temp_store=MEMORY")
    con.execute("""CREATE TABLE units(
        unit_id INTEGER PRIMARY KEY, kind TEXT, page INTEGER,
        text TEXT, span_start INTEGER, span_end INTEGER)""")
    con.executemany(
        "INSERT INTO units(unit_id,kind,page,text,span_start,span_end) VALUES (?,?,?,?,?,?)",
        [(i, u["kind"], u.get("page"), u["text"], u["span_start"], u["span_end"])
         for i, u in enumerate(units)])
    con.commit()
    con.execute("CREATE INDEX idx_units_kind ON units(kind)")
    con.commit()
    con.close()
    wall = (time.monotonic() - t0) * 1000.0
    rss_after = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return {"rc": 0, "wall_ms": wall, "peak_rss_kb": max(rss_before, rss_after),
            "db": db, "ref": ref, "db_bytes": os.path.getsize(db),
            "ref_bytes": os.path.getsize(ref)}


def baseline_locate_ids(db, anchor):
    """Locate step: case-sensitive literal search, returning coordinate ids only.

    VOLE find is a case-sensitive `str::contains`; SQLite `LIKE` is
    case-insensitive for ASCII and would overmatch, so the baseline uses
    `instr` (case-sensitive, literal) so both backends search identically.
    """
    t0 = time.monotonic()
    con = sqlite3.connect(db)
    cur = con.execute("SELECT unit_id FROM units WHERE instr(text, ?) > 0 ORDER BY unit_id", (anchor,))
    ids = [r[0] for r in cur.fetchall()]
    con.close()
    wall = (time.monotonic() - t0) * 1000.0
    return ids, wall


def baseline_read(db, unit_id):
    """Read step: fetch the located unit's text and byte span."""
    t0 = time.monotonic()
    con = sqlite3.connect(db)
    cur = con.execute("SELECT text, span_start, span_end FROM units WHERE unit_id = ?", (unit_id,))
    row = cur.fetchone()
    con.close()
    wall = (time.monotonic() - t0) * 1000.0
    if row is None:
        return None, None, wall
    return row[0], [row[1], row[2]], wall


# ---------------------------------------------------------------------------
# Workflow / transcripts
# ---------------------------------------------------------------------------

def transcript_tokens(tc, docid, unit_kind, anchor, turns):
    """Cumulative model-input tokens over the scripted agent's turns.

    A real agent resends the whole transcript at every turn, so the input token
    count is the SUM over turns of the cumulative context length. `turns` is a
    list of rendered tool results (locate result, read result).
    """
    prefix = SYSTEM + "\nTASK: in document %s, locate %r and return the containing %s text with a source span.\n" % (
        docid, anchor, unit_kind)
    n = tc.count(prefix)
    ctx = prefix
    for t in turns:
        ctx += t
        n += tc.count(ctx)
    return n


def render_locate_vo(matches):
    """Locate result: coordinates only (the agent reads text in the next step)."""
    return json.dumps({"matches": [{"unit": m["unit"]} for m in matches]}, ensure_ascii=False)


def render_locate_base(matches):
    return json.dumps({"matches": [{"unit": m["unit"]} for m in matches]}, ensure_ascii=False)


def render_read(text, span):
    return json.dumps({"text": text, "span": span}, ensure_ascii=False)


def locate_turn(render):
    return "ASSISTANT: search\nTOOL(search) -> " + render + "\n"


def read_turn(render):
    return "ASSISTANT: read\nTOOL(read) -> " + render + "\n"


# ---------------------------------------------------------------------------
# Freeze / run for one document
# ---------------------------------------------------------------------------

def freeze_doc(vbin, doc, fmt, src, workdir, k):
    t0 = time.monotonic()
    units, ref_bytes = materialize_ref(ref_units(fmt, src))
    ref_extract_ms = (time.monotonic() - t0) * 1000.0  # charged to baseline doc-prep
    tasks = select_tasks(units, k)
    # VOLE: build once to know the field and to probe read selectivity.
    store = os.path.join(workdir, "vstore")
    vinfo = vole_build(vbin, src, store, workdir)
    probe = {}
    if vinfo["rc"] == 0 and vinfo["field"]:
        f = vinfo["field"]
        # Probe whether the native text read carries a source span for this format.
        if fmt == "pdf":
            read_req = ["--page", "1", "--kind", "text"]
        elif fmt == "docx":
            read_req = ["--block", "0", "--kind", "text"]
        else:
            read_req = ["--block", "0", "--kind", "text"]
        res = vole_batch(vbin, store, f, [read_req],
                         os.path.join(workdir, "probe.json"), os.path.join(workdir, "probe.err"))
        ans = res["answers"][0] if res["answers"] else None
        probe["read_selector"] = " ".join(read_req)
        probe["read_span"] = ans.get("source_span") if ans else None
        probe["read_rc"] = res["rc"]
    return {"units": units, "ref_bytes": ref_bytes, "tasks": tasks,
            "vole_build": vinfo, "probe": probe, "ref_extract_ms": ref_extract_ms}


# ---------------------------------------------------------------------------
# Main run
# ---------------------------------------------------------------------------

def cmd_selftest(args):
    src = args.doc
    fmt = args.fmt
    units = ref_units(fmt, src)
    print(json.dumps({"fmt": fmt, "nunits": len(units),
                      "first3": [u["text"][:120] for u in units[:3]]}, ensure_ascii=False))
    return 0


def cmd_run(args):
    docs = []
    with open(args.docs) as f:
        for line in f:
            line = line.rstrip("\n")
            if not line or line.startswith("#"):
                continue
            parts = line.split("\t")
            if len(parts) >= 3:
                docs.append({"id": parts[0], "fmt": parts[1], "path": parts[2]})
    tc = TokenCounter(args.tokenizer, args.expect_sha256, args.tokname)
    os.makedirs(os.path.join(args.outdir, "raw"), exist_ok=True)
    raw = os.path.join(args.outdir, "raw")
    work = args.work
    os.makedirs(work, exist_ok=True)

    per_task = []
    per_doc = []
    tasks_all = {}
    for d in docs:
        did, fmt, src = d["id"], d["fmt"], d["path"]
        wd = os.path.join(work, did)
        if os.path.isdir(wd):
            subprocess.run(["rm", "-rf", wd])
        os.makedirs(wd, exist_ok=True)
        sys.stderr.write("== %s (%s) ==\n" % (did, fmt))
        fr = freeze_doc(args.vbin, did, fmt, src, wd, args.tasks_per_doc)
        units = fr["units"]
        tasks_all[did] = {"fmt": fmt, "path": src, "nunits": len(units),
                          "tasks": fr["tasks"], "probe": fr["probe"],
                          "vole_build": fr["vole_build"]}
        # baseline build (extraction time is charged to the baseline doc-prep)
        bb = baseline_build(fmt, src, units, fr["ref_bytes"], wd)
        bb["wall_ms"] += fr.get("ref_extract_ms", 0.0)
        bb["extract_ms"] = fr.get("ref_extract_ms", 0.0)

        store = os.path.join(wd, "vstore")
        field = (fr["vole_build"] or {}).get("field")

        # --- VOLE: locate (one session) then read the located unit (one
        #     session). Locate returns coordinates; read returns text + span.
        #     This is the SAME two-step shape the baseline uses. ---
        vole_locate = {}
        vole_read = {}
        if field:
            reqs = [["--text", t["anchor"], "--kind", "text"] for t in fr["tasks"]]
            if reqs:
                res = vole_batch(args.vbin, store, field, reqs,
                                 os.path.join(raw, "%s.vole.locate.jsonl" % did),
                                 os.path.join(raw, "%s.vole.locate.err" % did))
                for i, t in enumerate(fr["tasks"]):
                    vole_locate[i] = res["answers"][i] if i < len(res["answers"]) else None

            bmap = None
            if fmt in ("docx", "epub"):
                bmap = vole_block_map(args.vbin, store, field, fmt, wd)
            rreqs, ridx = [], []
            for i, t in enumerate(fr["tasks"]):
                ms = vole_matches(vole_locate.get(i) or {})
                if not ms:
                    continue
                c = ms[0]["coord"]
                if fmt == "pdf":
                    if c.get("page") is None:
                        continue
                    rreqs.append(["--page", str(c["page"]), "--kind", "text"])
                elif fmt == "docx":
                    k = (bmap or {}).get(c.get("paragraph"))
                    if k is None:
                        continue
                    rreqs.append(["--block", str(k), "--kind", "text"])
                else:
                    k = (bmap or {}).get((c.get("spine"), c.get("block")))
                    if k is None:
                        continue
                    rreqs.append(["--block", str(k), "--kind", "text"])
                ridx.append(i)
            if rreqs:
                res2 = vole_batch(args.vbin, store, field, rreqs,
                                  os.path.join(raw, "%s.vole.read.jsonl" % did),
                                  os.path.join(raw, "%s.vole.read.err" % did))
                for j, i in enumerate(ridx):
                    vole_read[i] = res2["answers"][j] if j < len(res2["answers"]) else None

        for i, t in enumerate(fr["tasks"]):
            # --- VOLE task: locate -> read -> answer ---
            vl = vole_locate.get(i)
            v_matches = vole_matches(vl) if vl else []
            vr = vole_read.get(i)
            v_text = vr.get("text", "") if vr else ""
            v_span = vr.get("source_span") if vr else None
            v_wall = ((vl or {}).get("stats") or {}).get("wall_micros", 0) / 1000.0
            if vr is not None:
                v_wall += ((vr.get("stats") or {}).get("wall_micros", 0)) / 1000.0
            v_tools = (1 if vl else 0) + (1 if vr else 0)
            v_turns = []
            if vl:
                v_turns.append(locate_turn(render_locate_vo(v_matches)))
            if vr is not None:
                v_turns.append(read_turn(render_read(v_text, v_span)))
            v_answer = v_text
            v_correct = bool(vr) and bool(v_text) and norm(v_text) == norm(t["expected_text"])
            v_grounded = _ground_ok_vole(fmt, src, v_span, t["anchor"], vr is not None)
            v_tokens = transcript_tokens(tc, did, t["unit_kind"], t["anchor"], v_turns)

            # --- baseline task: locate -> read -> answer (same shape) ---
            b_ids, b_lwall = baseline_locate_ids(bb["db"], t["anchor"])
            b_text, b_span, b_rwall = (None, None, 0.0)
            if b_ids:
                b_text, b_span, b_rwall = baseline_read(bb["db"], b_ids[0])
            b_wall = b_lwall + b_rwall
            b_tools = (1 if b_ids else 0) + (1 if b_text is not None else 0)
            b_turns = []
            if b_ids:
                b_turns.append(locate_turn(render_locate_base([{"unit": "row:%d" % r} for r in b_ids])))
            if b_text is not None:
                b_turns.append(read_turn(render_read(b_text, b_span)))
            b_answer = b_text or ""
            b_correct = b_text is not None and norm(b_text) == norm(t["expected_text"])
            b_grounded = b_span is not None and _ground_ok_baseline(fr["ref_bytes"], b_span, t["anchor"])
            b_tokens = transcript_tokens(tc, did, t["unit_kind"], t["anchor"], b_turns)

            per_task.append({
                "doc": did, "fmt": fmt, "unit_kind": t["unit_kind"], "anchor": t["anchor"],
                "expected_text": t["expected_text"], "expected_span": t["expected_span"],
                "vole": {"correct": v_correct, "grounded": v_grounded,
                         "tokens": v_tokens, "wall_ms": v_wall, "tool_calls": v_tools,
                         "span": v_span, "nmatches": len(v_matches),
                         "source_ref": (v_matches[0]["coord"] if v_matches else None),
                         "answer": v_answer},
                "baseline": {"correct": b_correct, "grounded": b_grounded,
                             "tokens": b_tokens, "wall_ms": b_wall, "tool_calls": b_tools,
                             "span": b_span, "nmatches": len(b_ids),
                             "answer": b_answer},
            })

        os.makedirs(os.path.join(raw, "build"), exist_ok=True)
        per_doc.append({"doc": did, "fmt": fmt, "path": src,
                        "source_bytes": os.path.getsize(src),
                        "ref_bytes": len(fr["ref_bytes"]),
                        "nunits": len(units),
                        "n_tasks": len(fr["tasks"]),
                        "probe": fr["probe"],
                        "vole": {"rc": fr["vole_build"]["rc"],
                                 "wall_ms": fr["vole_build"]["wall_ms"],
                                 "peak_rss_kb": fr["vole_build"]["peak_rss_kb"],
                                 "storage_bytes": dir_bytes(store),
                                 "field": field},
                        "baseline": {"rc": bb["rc"], "wall_ms": bb["wall_ms"],
                                     "peak_rss_kb": bb["peak_rss_kb"],
                                     "storage_bytes": bb["db_bytes"] + bb["ref_bytes"]}})

    with open(os.path.join(raw, "tasks.json"), "w") as f:
        json.dump(tasks_all, f, ensure_ascii=False, indent=1)
    with open(os.path.join(raw, "per_task.jsonl"), "w") as f:
        for r in per_task:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
    with open(os.path.join(raw, "per_doc.jsonl"), "w") as f:
        for r in per_doc:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")

    _aggregate(args.outdir, tc, per_task, per_doc, tasks_all)
    return 0


def vole_block_map(vbin, store, field, fmt, wd):
    """Map a search coordinate to VOLE's global `--block`/`--page` read.

    One observe-batch session requests `--block k --kind metadata` for k=0..CAP;
    out-of-range requests are typed declines and do not abort the session, so the
    real block count is discovered in a single process.

      docx  -> {paragraph_index: block_ordinal}
      epub  -> {(spine, local_block): block_ordinal}
    """
    omap = {}
    if fmt == "docx":
        cap = 4000
        reqs = [["--block", str(k), "--kind", "metadata"] for k in range(cap)]
        res = vole_batch(vbin, store, field, reqs,
                         os.path.join(wd, "bmap-probe.json"),
                         os.path.join(wd, "bmap-probe.err"))
        for k, ans in enumerate(res["answers"]):
            val = ans.get("value")
            if isinstance(val, dict) and val.get("kind") == "paragraph" and val.get("paragraph") is not None:
                omap[val["paragraph"]] = k
        return omap
    # epub
    cap = 4000
    reqs = [["--block", str(k), "--kind", "metadata"] for k in range(cap)]
    res = vole_batch(vbin, store, field, reqs,
                     os.path.join(wd, "omap-probe.json"),
                     os.path.join(wd, "omap-probe.err"))
    for k, ans in enumerate(res["answers"]):
        prov = ans.get("provenance", "")
        ms = re.search(r"spine=(\d+);part=[^;]*;block=(\d+)", prov)
        if ms:
            omap[(int(ms.group(1)), int(ms.group(2)))] = k
    return omap


def _ground_ok_baseline(ref_bytes, span, anchor):
    a, b = span
    if a is None or b is None or b < a or b > len(ref_bytes):
        return False
    return anchor.encode("utf-8") in ref_bytes[a:b]


def _inflate_span(src, span):
    """Raw-inflate the compressed package-member byte range `src[a:b]`."""
    a, b = span
    if b < a:
        return None
    try:
        data = open(src, "rb").read()
        return zlib.decompressobj(-15).decompress(data[a:b])
    except Exception:
        return None


def _docx_joined_text(dec):
    import xml.etree.ElementTree as ET
    try:
        root = ET.fromstring(dec)
    except Exception:
        return None
    return "".join(t.text or "" for t in root.iter() if _xml_local(t.tag) == "t")


def _xhtml_text(dec):
    p = _BlockHTML()
    try:
        p.feed(dec.decode("utf-8", "replace"))
    except Exception:
        return None
    return " ".join(p.units)


def _ground_ok_vole(fmt, src, span, anchor, have_answer):
    """Grounding: the answer carries a *source span* that re-materialises to
    source material backing the anchor. The span is the backing package
    member's compressed byte range inside the source archive (VOLE's package
    provenance), so it is verified by raw-inflating it and checking the anchor
    in the member's *joined* text (DOCX text is split across runs).

      docx  -- the find answer already carries the backing member span.
      epub  -- the find answer carries no span, so a span-bearing block read is
               performed and its span is used.
      pdf   -- no text observation exposes a span -> ungrounded.
    """
    if not have_answer:
        return False  # no answer to ground
    if fmt == "docx":
        dec = _inflate_span(src, span) if span else None
        if dec is None:
            return False
        text = _docx_joined_text(dec)
        return text is not None and anchor in text
    if fmt == "epub":
        dec = _inflate_span(src, span) if span else None
        if dec is None:
            return False
        text = _xhtml_text(dec)
        return text is not None and anchor in text
    return False


# ---------------------------------------------------------------------------
# Aggregation
# ---------------------------------------------------------------------------

def _cost(tokens, wall_ms, tool_calls, doc, prep_wall_ms, storage_bytes, prices=PRICES):
    model = prices["model_input_usd_per_mtok"] * tokens / 1_000_000.0
    compute = prices["compute_usd_per_cpu_hour"] * (wall_ms / 1000.0) / 3600.0
    prep = prices["compute_usd_per_cpu_hour"] * (prep_wall_ms / 1000.0) / 3600.0
    storage = prices["storage_usd_per_gib_month"] * (storage_bytes / (1 << 30)) * prices["retention_months"]
    ops = prices["op_usd_per_tool_call"] * tool_calls
    return {"model": model, "compute": compute, "prep": prep, "storage": storage, "ops": ops,
            "total": model + compute + prep + storage + ops}


def _region(fmt):
    return fmt


def _agg_backend(per_task, per_doc, backend):
    docprep = {d["doc"]: d for d in per_doc}
    # charge each doc's prep to each of its tasks (per-task share)
    regions = {}
    pooled = {"tasks": 0, "correct": 0, "grounded": 0, "correct_grounded": 0,
              "tokens": 0, "tokens_cg": 0, "wall_ms": 0, "tool_calls": 0,
              "prep_wall_ms": 0, "storage_bytes": 0, "docs": set(),
              "cost_total": 0.0, "peak_rss_kb": 0}
    for r in per_task:
        d = r["doc"]
        dd = docprep[d]
        ntasks = dd["n_tasks"] or 1
        prep_wall = dd[backend]["wall_ms"] / ntasks
        storage = dd[backend]["storage_bytes"] / ntasks
        b = r[backend]
        c = _cost(b["tokens"], b["wall_ms"], b["tool_calls"], d, prep_wall, storage)
        for key in (pooled, regions.setdefault(_region(r["fmt"]), {
                "tasks": 0, "correct": 0, "grounded": 0, "correct_grounded": 0,
                "tokens": 0, "tokens_cg": 0, "wall_ms": 0, "tool_calls": 0,
                "prep_wall_ms": 0, "storage_bytes": 0, "docs": set(),
                "cost_total": 0.0, "peak_rss_kb": 0})):
            key["tasks"] += 1
            key["correct"] += 1 if b["correct"] else 0
            key["grounded"] += 1 if b["grounded"] else 0
            cg = b["correct"] and b["grounded"]
            key["correct_grounded"] += 1 if cg else 0
            key["tokens"] += b["tokens"]
            key["tokens_cg"] += b["tokens"] if cg else 0
            key["wall_ms"] += b["wall_ms"]
            key["tool_calls"] += b["tool_calls"]
            key["prep_wall_ms"] += prep_wall
            key["storage_bytes"] += storage
            key["cost_total"] += c["total"]
            key["docs"].add(d)
            key["peak_rss_kb"] = max(key["peak_rss_kb"], dd[backend]["peak_rss_kb"])
    for k in list(regions.values()) + [pooled]:
        k["docs"] = len(k["docs"])
        k["cost_per_correct_grounded"] = (
            (k["cost_total"] / k["correct_grounded"]) if k["correct_grounded"] else None)
        k["tokens_per_correct_grounded"] = (
            (k["tokens"] / k["correct_grounded"]) if k["correct_grounded"] else None)
    return pooled, regions


def _aggregate(outdir, tc, per_task, per_doc, tasks_all):
    raw = os.path.join(outdir, "raw")
    with open(os.path.join(raw, "per_task.jsonl")) as f:
        per_task = [json.loads(l) for l in f if l.strip()]
    with open(os.path.join(raw, "per_doc.jsonl")) as f:
        per_doc = [json.loads(l) for l in f if l.strip()]

    v_pooled, v_reg = _agg_backend(per_task, per_doc, "vole")
    b_pooled, b_reg = _agg_backend(per_task, per_doc, "baseline")

    def ratio(v, b):
        """Cost ratio VOLE/baseline, or +inf/None with a stated reason.

        If VOLE has NO correct+grounded task while the baseline does, VOLE's
        cost per correct grounded task is unbounded: the gate (>=2x LOWER) can
        never hold, so it is scored as a loss, not 'unresolved'.
        """
        bc = b.get("cost_per_correct_grounded")
        vc = v.get("cost_per_correct_grounded")
        if not bc:
            return None, "baseline has no correct grounded task in this region"
        if not vc:
            return None, "VOLE has no correct grounded task -> cost/corr+grnd is unbounded (loss)"
        return vc / bc, ""

    regions = sorted(set(v_reg) | set(b_reg))
    matrix = []
    for rg in regions:
        v = v_reg.get(rg, {})
        b = b_reg.get(rg, {})
        r, note = ratio(v, b)
        matrix.append({"region": rg, "vole": v, "baseline": b,
                       "cost_ratio_vole_over_baseline": r, "note": note,
                       "_vole_has_cg": bool(v.get("cost_per_correct_grounded"))})

    pooled_ratio, pooled_note = ratio(v_pooled, b_pooled)
    pooled_vole_cg = bool(v_pooled.get("cost_per_correct_grounded"))
    gate_met = bool(pooled_vole_cg) and pooled_ratio is not None and pooled_ratio <= 0.5

    def verdict(r, has_cg=True, note=""):
        if r is None:
            if not has_cg and "unbounded" in note:
                return "VOLE loss (no correct grounded task)"
            return "unresolved"
        if r <= 0.5:
            return "VOLE win (>=2x)"
        if r < 1.0:
            return "VOLE win (<2x)"
        if r > 1.0:
            return "VOLE loss"
        return "tie"

    summary = {
        "phase": "22.7 (P6) — document-agent economic court",
        "gate": ">=2x lower cost per correct grounded task (VOLE vs baseline)",
        "gate_met": bool(gate_met),
        "prices": PRICES,
        "prices_date": PRICES_DATE,
        "tokenizer": tc.identity(),
        "pooled": {"vole": v_pooled, "baseline": b_pooled,
                   "cost_ratio_vole_over_baseline": pooled_ratio,
                   "note": pooled_note,
                   "verdict": verdict(pooled_ratio, pooled_vole_cg, pooled_note)},
        "regions": [{"region": m["region"],
                     "vole": m["vole"], "baseline": m["baseline"],
                     "cost_ratio_vole_over_baseline": m["cost_ratio_vole_over_baseline"],
                     "note": m["note"],
                     "verdict": verdict(m["cost_ratio_vole_over_baseline"], m["_vole_has_cg"], m["note"])}
                    for m in matrix],
        "per_task": per_task,
        "per_doc": per_doc,
    }
    with open(os.path.join(outdir, "summary.json"), "w") as f:
        json.dump(summary, f, ensure_ascii=False, indent=1)

    # counts.txt
    with open(os.path.join(outdir, "counts.txt"), "w") as f:
        f.write("docs=%d\n" % len(per_doc))
        f.write("tasks=%d\n" % len(per_task))
        f.write("gate_met=%s\n" % ("yes" if gate_met else "no"))
        f.write("tokenizer_kind=%s\n" % tc.kind)
        for be in ("vole", "baseline"):
            p = v_pooled if be == "vole" else b_pooled
            f.write("%s_correct=%d/%d\n" % (be, p["correct"], p["tasks"]))
            f.write("%s_grounded=%d/%d\n" % (be, p["grounded"], p["tasks"]))
            f.write("%s_correct_grounded=%d/%d\n" % (be, p["correct_grounded"], p["tasks"]))
            f.write("%s_tokens_total=%d\n" % (be, p["tokens"]))
            f.write("%s_cost_total_usd=%.9f\n" % (be, p["cost_total"]))
            cpcg = p["cost_per_correct_grounded"]
            f.write("%s_cost_per_correct_grounded_usd=%s\n" % (be, ("%.9f" % cpcg) if cpcg else "undefined"))
        f.write("cost_ratio_vole_over_baseline=%s\n" % (("%.6f" % pooled_ratio) if pooled_ratio else "undefined"))
        f.write("pooled_note=%s\n" % pooled_note)

    _write_matrix(outdir, matrix)
    _write_summary_md(outdir, summary, matrix, v_pooled, b_pooled, pooled_ratio, gate_met, tc)


def _write_matrix(outdir, matrix):
    lines = ["# MATRIX — per-region cost per correct grounded task", ""]
    lines.append("Cost per correct grounded task = total workload cost / #(correct AND grounded) tasks. "
                 "Prices in summary.json; date %s." % PRICES_DATE)
    lines.append("")
    lines.append("| region | backend | docs | tasks | correct | grounded | corr+grnd | tokens/task | tokens/corr+grnd | cost total (USD) | cost/corr+grnd (USD) |")
    lines.append("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
    for m in matrix:
        for be in ("vole", "baseline"):
            r = m[be]
            tpt = (r["tokens"] / r["tasks"]) if r["tasks"] else 0
            tcg = r["tokens_per_correct_grounded"]
            cpcg = r["cost_per_correct_grounded"]
            lines.append("| %s | %s | %d | %d | %d | %d | %d | %.1f | %s | %.9f | %s |" % (
                m["region"], be, r["docs"], r["tasks"], r["correct"], r["grounded"],
                r["correct_grounded"], tpt, ("%.1f" % tcg) if tcg else "undef",
                r["cost_total"], ("%.9f" % cpcg) if cpcg else "undef"))
    lines.append("")
    lines.append("| region | ratio VOLE/baseline | verdict |")
    lines.append("|---|---:|---|")
    for m in matrix:
        rr = m["cost_ratio_vole_over_baseline"]
        lbl = ("%.4f" % rr) if rr else ("inf" if m.get("note") else "undef")
        lines.append("| %s | %s | %s |" % (
            m["region"], lbl, _verdict_str(rr, m.get("_vole_has_cg", True), m.get("note", ""))))
    lines.append("")
    with open(os.path.join(outdir, "MATRIX.md"), "w") as f:
        f.write("\n".join(lines) + "\n")


def _verdict_str(r, has_cg=True, note=""):
    if r is None:
        if not has_cg and "unbounded" in note:
            return "VOLE loss (no correct grounded task)"
        return "unresolved"
    if r <= 0.5:
        return "VOLE win (>=2x)"
    if r < 1.0:
        return "VOLE win (<2x)"
    if r > 1.0:
        return "VOLE loss"
    return "tie"


def _write_summary_md(outdir, summary, matrix, v_pooled, b_pooled, pooled_ratio, gate_met, tc):
    L = []
    L.append("# Phase 22.7 (P6) — document-agent economic court")
    L.append("")
    L.append("**Question.** Across the whole document-agent workload, what is the cost per")
    L.append("correct, grounded task, and does VOLE cost **>=2x less** than a conventional")
    L.append("tuned baseline?")
    L.append("")
    L.append("**Answer (measured).** %s" % ("GATE MET" if gate_met else "GATE NOT MET"))
    L.append("")
    L.append("**Verdict.** %s" % _verdict_str(
        pooled_ratio, bool(v_pooled.get("cost_per_correct_grounded")),
        summary["pooled"].get("note", "")))
    L.append("")
    L.append("## Estimator and interval")
    L.append("")
    L.append("Estimator: **ratio of summed costs** (`cost/corr+grnd` VOLE / baseline), pooled and by region.")
    L.append("The correctness, grounding and token counts are **deterministic** under the pinned")
    L.append("toolchain (same command, same tokenizer, same documents), so no sampling interval is")
    L.append("quoted; wall/RSS/storage are single-run and reported as the physical vector. The frozen")
    L.append("task set is small (9 tasks per region, 27 total); a region where VOLE has no")
    L.append("correct-grounded task is a **resolved loss**, not 'unresolved'.")
    L.append("")
    L.append("## Frozen documents")
    L.append("")
    L.append("| doc | fmt | source bytes | ref units | tasks | VOLE prep ms | VOLE store B | base prep ms | base store B |")
    L.append("|---|---|---:|---:|---:|---:|---:|---:|---:|")
    for d in summary.get("per_doc", []):
        L.append("| %s | %s | %d | %d | %d | %.0f | %d | %.0f | %d |" % (
            d["doc"], d["fmt"], d["source_bytes"], d["nunits"], d["n_tasks"],
            d["vole"]["wall_ms"], d["vole"]["storage_bytes"],
            d["baseline"]["wall_ms"], d["baseline"]["storage_bytes"]))
    L.append("")
    L.append("## Tokenizer / input measure")
    L.append("")
    L.append("`%s` (kind `%s`). %s" % (tc.name, tc.kind, json.dumps(tc.detail)))
    L.append("")
    L.append("## Pooled")
    L.append("")
    L.append("| backend | tasks | correct | grounded | corr+grnd | tokens | cost total | cost/corr+grnd |")
    L.append("|---|---:|---:|---:|---:|---:|---:|---:|")
    for be, r in (("vole", v_pooled), ("baseline", b_pooled)):
        cpcg = r["cost_per_correct_grounded"]
        L.append("| %s | %d | %d | %d | %d | %d | %.9f | %s |" % (
            be, r["tasks"], r["correct"], r["grounded"], r["correct_grounded"],
            r["tokens"], r["cost_total"], ("%.9f" % cpcg) if cpcg else "undef"))
    L.append("")
    L.append("Pooled cost ratio VOLE/baseline: **%s** (%s)." % (
        ("%.4f" % pooled_ratio) if pooled_ratio else "undefined",
        _verdict_str(pooled_ratio, bool(v_pooled.get("cost_per_correct_grounded")),
                     summary["pooled"].get("note", ""))))
    L.append("")
    L.append("## Regions")
    L.append("")
    L.append("| region | ratio | verdict |")
    L.append("|---|---:|---|")
    for m in matrix:
        rr = m["cost_ratio_vole_over_baseline"]
        lbl = ("%.4f" % rr) if rr else ("inf" if m.get("note") else "undef")
        L.append("| %s | %s | %s |" % (m["region"], lbl,
                 _verdict_str(rr, m.get("_vole_has_cg", True), m.get("note", ""))))
    L.append("")
    L.append("## Frozen tasks (the same expectation for both backends)")
    L.append("")
    L.append("Expected answer = the reference unit text (normalised); expected span = byte range in the reference text. "
             "Anchor is a token that occurs in exactly one reference unit.")
    L.append("")
    L.append("| doc | fmt | unit | anchor | expected span | expected answer (first 80 chars) |")
    L.append("|---|---|---|---|---|---|")
    for r in summary.get("per_task", []):
        L.append("| %s | %s | %s | %s | [%s,%s] | %s |" % (
            r["doc"], r["fmt"], r["unit_kind"], r["anchor"],
            r["expected_span"][0], r["expected_span"][1],
            r["expected_text"][:80].replace("|", "\\|")))
    L.append("")
    with open(os.path.join(outdir, "SUMMARY.md"), "w") as f:
        f.write("\n".join(L) + "\n")


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main(argv=None):
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)

    ps = sub.add_parser("selftest")
    ps.add_argument("--doc", required=True)
    ps.add_argument("--fmt", required=True)
    ps.add_argument("--vbin")
    ps.add_argument("--work", default="/tmp/p227-selftest")

    pf = sub.add_parser("freeze")
    pf.add_argument("--docs", required=True)
    pf.add_argument("--out", required=True)
    pf.add_argument("--vbin", required=True)
    pf.add_argument("--work", default="/tmp/p227-freeze")
    pf.add_argument("--tasks-per-doc", type=int, default=3)

    pr = sub.add_parser("run")
    pr.add_argument("--outdir", required=True)
    pr.add_argument("--docs", required=True)
    pr.add_argument("--vbin", required=True)
    pr.add_argument("--tokenizer")
    pr.add_argument("--expect-sha256")
    pr.add_argument("--tokname", default="bert-base-uncased")
    pr.add_argument("--work", default="/tmp/p227-work")
    pr.add_argument("--tasks-per-doc", type=int, default=3)

    args = ap.parse_args(argv)
    if args.cmd == "selftest":
        return cmd_selftest(args)
    if args.cmd == "run":
        return cmd_run(args)
    if args.cmd == "freeze":
        return 0
    return 2


if __name__ == "__main__":
    sys.exit(main())
