#!/usr/bin/env python3
"""Phase 22.5 — aggregate the remote selective-read court into a sealed receipt.

Reads the court's raw tables (`observations.tsv`, `exact.tsv`, `subset.tsv`) and
writes `SUMMARY.md`, `MATRIX.md`, `summary.json`, `counts.txt`.

This is a **model** aggregator: the bytes and request counts are the court's real
local measurements; the latency figures come from the stated cost model
(`T = N_requests * RTT + B_transferred / BW + C_decode`).  The model name,
constants and date are printed into every artifact.
"""

import json
import math
import os
import statistics
import sys

MODEL = "remote-selective-v1"
MODEL_DATE = "2026-10-08"
MODEL_EQ = "T = N_requests * RTT + B_transferred / BW + C_decode  (sequential, P=1)"
PROFILES = [
    ("cloud-same-region", 0.025, 100e6, "PRIMARY"),
    ("edge-fast", 0.005, 1000e6, "sensitivity"),
    ("wan-slow", 0.080, 25e6, "sensitivity"),
]
PRIMARY = PROFILES[0]
THETA_BYTES = 0.5   # "major reduction": VOLE transfers <= half the SQLite control


def fnum(v, d=0.0):
    try:
        return float(v)
    except (TypeError, ValueError):
        return d


def percentile(vals, p):
    if not vals:
        return float("nan")
    s = sorted(vals)
    if len(s) == 1:
        return s[0]
    k = (len(s) - 1) * p
    lo = int(math.floor(k))
    hi = int(math.ceil(k))
    if lo == hi:
        return s[lo]
    return s[lo] + (s[hi] - s[lo]) * (k - lo)


def med(vals):
    vals = [v for v in vals if not math.isnan(v)]
    return statistics.median(vals) if vals else float("nan")


def fmt(x, nd=3):
    if x is None or (isinstance(x, float) and math.isnan(x)):
        return "n/a"
    return ("%.*f" % (nd, x))


def latency(profile, req, byts, decode_s):
    _, rtt, bw, _ = profile
    return req * rtt + byts / bw + decode_s


def load_tsv(path):
    with open(path) as f:
        hdr = f.readline().rstrip("\n").split("\t")
        return [dict(zip(hdr, l.rstrip("\n").split("\t"))) for l in f if l.strip()]


def load_subset(path):
    """subset.tsv is header-less pipe-delimited (Phase 22.2 generator)."""
    cols = ["id", "agency", "fmt", "size_class", "byte_len", "sha256",
            "path", "family", "is_head"]
    out = []
    with open(path) as f:
        for line in f:
            if line.strip():
                out.append(dict(zip(cols, line.rstrip("\n").split("|"))))
    return out


def region_key(row, kind):
    if kind == "obs":
        return "obs=%s" % row["obs"]
    if kind == "obs_fmt":
        return "obs=%s/%s" % (row["obs"], row["fmt"])
    if kind == "obs_size":
        return "obs=%s/%s" % (row["obs"], row["size_class"])
    return "all"


def verdict(median_bytes_ratio, median_lat_ratio):
    if math.isnan(median_bytes_ratio):
        return "no-data"
    if median_bytes_ratio <= THETA_BYTES and not math.isnan(median_lat_ratio) \
            and median_lat_ratio <= 1.0:
        return "win"
    if median_bytes_ratio > 1.0:
        return "loss"
    return "unresolved"


def main(argv):
    raw, campaign = argv[1], argv[2]
    obs = load_tsv(os.path.join(raw, "observations.tsv"))
    exact = load_tsv(os.path.join(raw, "exact.tsv"))
    subset = load_subset(os.path.join(raw, "subset.tsv"))
    ids = [r["id"] for r in subset]

    # ---- per-observation derived metrics ------------------------------------
    rows = []
    for r in obs:
        answered = (r["vole_rc"] == "0" and r["sql_declined"] == "0"
                    and fnum(r["sql_fetch_bytes"]) > 0)
        store_b = fnum(r["store_bytes"])
        db_b = fnum(r["db_bytes"])
        vby = fnum(r["vole_fetch_bytes"])
        vreq = fnum(r["vole_fetch_requests"])
        sby = fnum(r["sql_fetch_bytes"])
        sreq = fnum(r["sql_fetch_requests"])
        vdec = fnum(r["vole_decode_us"]) / 1e6
        sdec = fnum(r["sql_decode_us"]) / 1e6
        d = {
            "id": r["id"], "fmt": r["fmt"], "size_class": r["size_class"],
            "obs": r["obs"], "answered": answered,
            "src_bytes": fnum(r["src_bytes"]),
            "store_bytes": store_b, "db_bytes": db_b,
            "vole_rc": r["vole_rc"], "sql_declined": r["sql_declined"],
            "vole_classes": {"D": fnum(r["D"]), "M": fnum(r["M"]),
                             "I": fnum(r["I"]), "S": fnum(r["S"])},
            "vole_bytes": vby, "vole_requests": vreq,
            "vole_plan_ranges": fnum(r["vole_plan_ranges"]),
            "vole_ret": fnum(r["vole_ret"]), "vole_dec_us": fnum(r["vole_decode_us"]),
            "sql_raw_preads": fnum(r["sql_raw_preads"]),
            "sql_distinct": fnum(r["sql_distinct"]),
            "sql_plan_ranges": fnum(r["sql_plan_ranges"]),
            "sql_bytes": sby, "sql_requests": sreq, "sql_dec_us": fnum(r["sql_decode_us"]),
            "whole_store_bytes": fnum(r["whole_store_bytes"]),
            "whole_db_bytes": fnum(r["whole_db_bytes"]),
            "srv_client": fnum(r["srv_client_requests"]),
            "srv_server": fnum(r["srv_server_requests"]),
        }
        if answered and sby > 0:
            d["bytes_ratio"] = vby / sby
            d["store_vs_sql_bytes_ratio"] = store_b / sby
            d["vole_selectivity"] = vby / store_b if store_b else float("nan")
            d["sql_selectivity"] = sby / db_b if db_b else float("nan")
            for name, rtt, bw, _role in PROFILES:
                tv = latency((name, rtt, bw, ""), vreq, vby, vdec)
                ts = latency((name, rtt, bw, ""), sreq, sby, sdec)
                d["lat_%s_vole" % name] = tv
                d["lat_%s_sql" % name] = ts
                d["lat_%s_ratio" % name] = tv / ts if ts else float("nan")
        rows.append(d)

    # ---- regions ------------------------------------------------------------
    regions = {}
    for kind in ("all", "obs", "obs_fmt", "obs_size"):
        for r in rows:
            regions.setdefault(region_key(r, kind), {"kind": kind, "rows": []})
            regions[region_key(r, kind)]["rows"].append(r)

    # ---- integrity ----------------------------------------------------------
    ex_ok = sum(1 for e in exact if e["vole_ok"] == "1")
    ex_cmp = sum(1 for e in exact if e["cmp_eq"] == "eq")
    tot_vreq = sum(fnum(r["vole_fetch_requests"]) for r in obs)
    tot_vver = sum(fnum(r["vole_fetch_verified"]) for r in obs)
    tot_sreq = sum(fnum(r["sql_fetch_requests"]) for r in obs)
    tot_sver = sum(fnum(r["sql_fetch_verified"]) for r in obs)
    tot_srv_client = sum(fnum(r["srv_client_requests"]) for r in obs)
    tot_srv_server = sum(fnum(r["srv_server_requests"]) for r in obs)

    # ---- region verdicts ----------------------------------------------------
    region_out = {}
    for key, reg in regions.items():
        rs = [r for r in reg["rows"] if r["answered"]]
        if not rs:
            region_out[key] = {"n": 0, "verdict": "no-data"}
            continue
        br = [r["bytes_ratio"] for r in rs]
        lr = [r["lat_cloud-same-region_ratio"] for r in rs]
        p95v = percentile([r["lat_cloud-same-region_vole"] for r in rs], 0.95)
        p95s = percentile([r["lat_cloud-same-region_sql"] for r in rs], 0.95)
        region_out[key] = {
            "n": len(rs),
            "median_bytes_ratio": med(br),
            "median_lat_ratio_primary": med(lr),
            "p95_lat_vole_primary_ms": p95v * 1e3,
            "p95_lat_sql_primary_ms": p95s * 1e3,
            "median_vole_bytes": med([r["vole_bytes"] for r in rs]),
            "median_sql_bytes": med([r["sql_bytes"] for r in rs]),
            "median_store_bytes": med([r["store_bytes"] for r in rs]),
            "median_db_bytes": med([r["db_bytes"] for r in rs]),
            "median_vole_selectivity": med([r["vole_selectivity"] for r in rs]),
            "median_sql_selectivity": med([r["sql_selectivity"] for r in rs]),
            "median_vole_requests": med([r["vole_requests"] for r in rs]),
            "median_sql_requests": med([r["sql_requests"] for r in rs]),
            "median_store_over_select": med([r["store_bytes"] / r["vole_bytes"]
                                             if r["vole_bytes"] else float("nan") for r in rs]),
            "median_db_over_page": med([r["db_bytes"] / r["sql_bytes"]
                                        if r["sql_bytes"] else float("nan") for r in rs]),
            "wins": sum(1 for r in rs if verdict(r["bytes_ratio"],
                       r["lat_cloud-same-region_ratio"]) == "win"),
            "ties": sum(1 for r in rs if verdict(r["bytes_ratio"],
                       r["lat_cloud-same-region_ratio"]) == "unresolved"),
            "losses": sum(1 for r in rs if verdict(r["bytes_ratio"],
                         r["lat_cloud-same-region_ratio"]) == "loss"),
        }
        region_out[key]["verdict"] = verdict(
            region_out[key]["median_bytes_ratio"],
            region_out[key]["median_lat_ratio_primary"])

    overall = region_out.get("all", {})
    wins = [k for k, v in region_out.items()
            if v.get("verdict") == "win" and k != "all"]
    losses = [k for k, v in region_out.items()
              if v.get("verdict") == "loss" and k != "all"]
    unresolved = [k for k, v in region_out.items()
                  if v.get("verdict") == "unresolved" and k != "all"]
    if overall.get("verdict") == "win":
        vtag = "WIN"
    elif overall.get("verdict") == "loss":
        vtag = "LOSS"
    else:
        vtag = "UNRESOLVED"
    verdict_line = ("%s[bytes_ratio=%s][lat_ratio=%s][wins=%d][losses=%d]"
                    "[unresolved=%d]" % (
                        vtag, fmt(overall.get("median_bytes_ratio", float("nan"))),
                        fmt(overall.get("median_lat_ratio_primary", float("nan"))),
                        len(wins), len(losses), len(unresolved)))

    # ---- MATRIX.md ----------------------------------------------------------
    m = ["# Phase 22.5 — per-observation matrix (MODEL: %s, %s)" % (MODEL, MODEL_DATE),
         "",
         "Bytes / requests are REAL local measurements; latency is MODELLED (%s)." % MODEL_EQ,
         "`bytes_ratio` = VOLE selective bytes / SQLite page-level bytes (<1 favours VOLE).",
         "`lat_ratio` = VOLE / SQLite modelled latency at the PRIMARY profile.",
         "",
         "| id | fmt | size | obs | VOLE B | VOLE req | SQL B | SQL req | store B | db B | bytes_ratio | lat_ratio | VOLE sel. | SQL sel. | verdict |",
         "|---|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|"]
    for r in rows:
        if not r["answered"]:
            m.append("| %s | %s | %s | %s | - | - | - | - | %d | %d | - | - | - | - | declined(rc=%s,sql_decl=%s) |"
                     % (r["id"], r["fmt"], r["size_class"], r["obs"],
                        r["store_bytes"], r["db_bytes"], r["vole_rc"], r["sql_declined"]))
            continue
        m.append("| %s | %s | %s | %s | %d | %d | %d | %d | %d | %d | %s | %s | %s | %s | %s |" % (
            r["id"], r["fmt"], r["size_class"], r["obs"],
            r["vole_bytes"], r["vole_requests"], r["sql_bytes"], r["sql_requests"],
            r["store_bytes"], r["db_bytes"],
            fmt(r["bytes_ratio"], 2), fmt(r["lat_cloud-same-region_ratio"], 2),
            fmt(r["vole_selectivity"], 3), fmt(r["sql_selectivity"], 3),
            verdict(r["bytes_ratio"], r["lat_cloud-same-region_ratio"])))
    open(os.path.join(campaign, "MATRIX.md"), "w").write("\n".join(m) + "\n")

    # ---- SUMMARY.md ---------------------------------------------------------
    def region_table(kind_keys):
        out = ["| region | n | VOLE B (med) | SQL B (med) | bytes_ratio (med) | VOLE sel. | SQL sel. | p95 lat VOLE (ms) | p95 lat SQL (ms) | lat_ratio (med) | W/T/L | verdict |",
               "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---|"]
        for k in kind_keys:
            v = region_out[k]
            if v["n"] == 0:
                continue
            out.append("| %s | %d | %s | %s | %s | %s | %s | %s | %s | %s | %d/%d/%d | %s |" % (
                k, v["n"], fmt(v["median_vole_bytes"], 0), fmt(v["median_sql_bytes"], 0),
                fmt(v["median_bytes_ratio"]), fmt(v["median_vole_selectivity"]),
                fmt(v["median_sql_selectivity"]),
                fmt(v["p95_lat_vole_primary_ms"], 2), fmt(v["p95_lat_sql_primary_ms"], 2),
                fmt(v["median_lat_ratio_primary"]),
                v["wins"], v["ties"], v["losses"], v["verdict"]))
        return out

    by_obs = sorted([k for k in region_out if k.startswith("obs=") and "/" not in k])
    by_obs_fmt = sorted([k for k in region_out if k.startswith("obs=") and k.count("/") == 1])
    by_obs_size = sorted([k for k in region_out if k.startswith("obs=") and k.count("/") == 1
                          and any(sc in k for sc in ("<100KiB", "100KiB-1MiB", "1-10MiB"))])
    # disambiguate fmt vs size keys by checking membership
    fmts = {"pdf", "docx", "epub"}
    sizes = {"<100KiB", "100KiB-1MiB", "1-10MiB"}
    by_obs_fmt = [k for k in by_obs_fmt if k.split("/")[-1] in fmts]
    by_obs_size = [k for k in by_obs_size if k.split("/")[-1] in sizes]

    s = []
    s.append("# Phase 22.5 — remote selective materialization (%s, **MODEL**)" % MODEL)
    s.append("")
    s.append("**This is an explicitly-labelled MODEL of remote selective reads, not a real S3 benchmark.**")
    s.append("Real object storage is unavailable in the pinned lane. The **bytes** a selective remote")
    s.append("read must transfer and the **request counts** are REAL local measurements (VOLE instrumented")
    s.append("`*_bytes_read`; SQLite `pread64` page offsets under `strace` with `mmap_size=0`, captured by a")
    s.append("real loopback HTTP `Range`/`206` server + coalescing client). The **latency** numbers apply the")
    s.append("stated cost model; the **geometry** (class bytes -> ranges) is an upper-bound model where the")
    s.append("exact node->offset map is not exposed. **A VOLE win is claimed only where the bytes ratio is")
    s.append("<= %.1fx and the modelled p95 is competitive, in a named region, under model %s with the"
             % (THETA_BYTES, MODEL))
    s.append("constants below (date %s).**" % MODEL_DATE)
    s.append("")
    s.append("## Question")
    s.append("")
    s.append("Can bounded *local* selective materialization (Phase 20.2) extend to **remote** storage? A")
    s.append("remote field fetches a compact directory and only the necessary immutable segments via")
    s.append("byte-range reads, with a range planner. The control is the tuned SQLite envelope given a")
    s.append("comparably optimised remote **page cache + source-range interface**.")
    s.append("")
    s.append("## Model and constants (date %s)" % MODEL_DATE)
    s.append("")
    s.append("```text")
    s.append(MODEL_EQ)
    s.append("```")
    s.append("")
    s.append("| profile | role | RTT | BW |")
    s.append("|---|---|---:|---:|")
    for name, rtt, bw, role in PROFILES:
        s.append("| %s | %s | %.0f ms | %.0f MB/s |" % (name, role, rtt * 1e3, bw / 1e6))
    s.append("")
    s.append("- `C_decode` = the measured **local** decode wall of the same observation (VOLE `stats.wall_micros`;")
    s.append("  SQLite the `sqlite3` query wall).  Requests are modelled sequentially (P=1, no pipelining).")
    s.append("- Coalescing: `gap=0` (merge only touching/overlapping ranges) — the byte-minimal plan.  A range")
    s.append("  server access log is cross-checked against the client's request count.")
    s.append("")
    s.append("## Pre-registered observations")
    s.append("")
    s.append("- **O1 text** — pdf `--page 1 --kind text`; docx/epub `--block 0 --kind text`.")
    s.append("- **O2 bytes** — `--byte-range 0..64 --kind exact`.")
    s.append("- **O3 resource** — docx/epub `--resource 0 --kind metadata`; pdf `--metadata --kind metadata`")
    s.append("  (the PDF contract has **no** resource observation; metadata substitutes, recorded).")
    s.append("")
    s.append("## Method")
    s.append("")
    s.append("- 12-document `real100-v1` subset (4 pdf / 4 docx / 4 epub), identical IDs to Phase 22.2.")
    s.append("- Per document: VOLE `field-build --profile runtime --packed` once; SQLite `converters.py` config")
    s.append("  `full` through C5 once.")
    s.append("- Per observation: cold VOLE `observe --no-cache` -> instrumented per-class bytes -> ranges in a")
    s.append("  flat store image -> real loopback range fetch; SQLite answer SQL (depth C0) under `strace`")
    s.append("  `pread64` with `PRAGMA mmap_size=0` -> page ranges -> real loopback range fetch.")
    s.append("- Controls: whole store image, whole `.db`.")
    s.append("")
    s.append("## Results — VOLE selective vs SQLite page-level (per region)")
    s.append("")
    s.append("### Pooled over documents")
    s.extend(region_table(["all"]))
    s.append("")
    s.append("### By observation")
    s.extend(region_table(by_obs))
    s.append("")
    s.append("### By observation x format")
    s.extend(region_table(by_obs_fmt))
    s.append("")
    s.append("### By observation x size class")
    s.extend(region_table(by_obs_size))
    s.append("")
    s.append("### Latency sensitivity — median VOLE/SQLite modelled latency ratio")
    s.append("")
    s.append("| obs | %s |" % " | ".join(p[0] for p in PROFILES))
    s.append("|---|%s" % ("---:|" * len(PROFILES)))
    for k in by_obs:
        rs = [r for r in rows if r["answered"] and k == region_key(r, "obs")]
        if not rs:
            continue
        vals = [med([r.get("lat_%s_ratio" % p[0], float("nan")) for r in rs])
                for p in PROFILES]
        s.append("| %s | %s |" % (k.split("=", 1)[1],
                                  " | ".join(fmt(v) for v in vals)))
    s.append("")
    s.append("## Whole-object vs selective (bytes moved)")
    s.append("")
    s.append("How selective is each lane relative to its own whole object?  `store/select` = whole store /")
    s.append("VOLE selective; `db/page` = whole `.db` / SQLite page-level.  VOLE's selective read is often a")
    s.append("**large fraction of the whole store** (it must read the descriptor closure), while SQLite's")
    s.append("page-level read is a **tiny fraction of the whole `.db`** (a few 4 KiB pages) for the same query.")
    s.append("")
    s.append("| obs | n | median store/select | median db/page | median VOLE/SQL |")
    s.append("|---|---:|---:|---:|---:|")
    for k in by_obs:
        v = region_out[k]
        if v["n"] == 0:
            continue
        s.append("| %s | %d | %s | %s | %s |" % (
            k.split("=", 1)[1], v["n"], fmt(v["median_store_over_select"], 2),
            fmt(v["median_db_over_page"], 1), fmt(v["median_bytes_ratio"])))
    s.append("")
    s.append("### Conservative control — whole-`.db` download (favours VOLE)")
    s.append("")
    s.append("The gate uses the **best** SQLite interface (page-level).  For transparency, against the")
    s.append("conservative whole-`.db` download the modelled VOLE selective read transfers **fewer** bytes")
    s.append("than SQLite would:")
    s.append("")
    s.append("| obs | n | median VOLE-selective / whole-`.db` | median whole-store / whole-`.db` |")
    s.append("|---|---:|---:|---:|")
    for k in by_obs:
        rs = [r for r in rows if r["answered"] and k == region_key(r, "obs")]
        if not rs:
            continue
        s.append("| %s | %d | %s | %s |" % (
            k.split("=", 1)[1], len(rs),
            fmt(med([r["vole_bytes"] / r["db_bytes"] for r in rs]), 3),
            fmt(med([r["store_bytes"] / r["db_bytes"] for r in rs]), 3)))
    s.append("")
    s.append("## Notable exceptions")
    s.append("")
    exc = [r for r in rows if r["answered"] and r["bytes_ratio"] < 1.0]
    s.append("Rows where the modelled VOLE selective read transfers **fewer** bytes than the SQLite")
    s.append("page-level interface (%d of %d answered):" % (len(exc), sum(1 for r in rows if r["answered"])))
    s.append("")
    s.append("| id | fmt | obs | VOLE B | SQL B | bytes_ratio | lat_ratio | note |")
    s.append("|---|---|---|---:|---:|---:|---:|---|")
    for r in sorted(exc, key=lambda x: x["bytes_ratio"]):
        if r["obs"] == "bytes":
            note = "exact byte-range: SQLite reads the whole source-blob overflow chain"
        else:
            note = "descriptor partial-read closure (observation index)"
        if r["lat_cloud-same-region_ratio"] <= 0.6:
            note += "; VOLE modelled-latency advantage"
        elif r["lat_cloud-same-region_ratio"] > 1.0:
            note += "; modelled latency not competitive here"
        s.append("| %s | %s | %s | %d | %d | %s | %s | %s |" % (
            r["id"], r["fmt"], r["obs"], r["vole_bytes"], r["sql_bytes"],
            fmt(r["bytes_ratio"], 2), fmt(r["lat_cloud-same-region_ratio"], 2), note))
    s.append("")
    s.append("## Coalescing effect (range planner)")
    s.append("")
    s.append("The planner merges touching/overlapping ranges on the same object (`gap=0`).  It matters most")
    s.append("for SQLite's page-level plan, where dozens of 4 KiB page reads collapse into a few requests.")
    s.append("")
    s.append("| lane | obs | sum ranges before | sum requests after | reduction |")
    s.append("|---|---|---:|---:|---:|")
    for lane, rk, qk in (("VOLE", "vole_plan_ranges", "vole_requests"),
                         ("SQLite", "sql_plan_ranges", "sql_requests")):
        tb = ta = 0
        for k in by_obs:
            rs = [r for r in rows if r["answered"] and k == region_key(r, "obs")]
            if not rs:
                continue
            b = sum(fnum(r[rk]) for r in rs)
            a = sum(fnum(r[qk]) for r in rs)
            tb += b
            ta += a
            s.append("| %s | %s | %d | %d | %s |" % (
                lane, k.split("=", 1)[1], b, a, fmt(b / a, 2) if a else "n/a"))
        s.append("| %s | **total** | **%d** | **%d** | **%s** |" % (
            lane, tb, ta, fmt(tb / ta, 2) if ta else "n/a"))
    s.append("")
    s.append("## Integrity")
    s.append("")
    s.append("- `materialize --exact`: **%d/%d** byte-exact (length + SHA-256; `cmp` equal **%d/%d**)."
             % (ex_ok, len(exact), ex_cmp, len(exact)))
    s.append("- Selective range GETs verified (HTTP 206 + exact length + SHA-256 vs source slice):")
    s.append("  **VOLE %d/%d**, **SQLite %d/%d**." % (tot_vver, tot_vreq, tot_sver, tot_sreq))
    s.append("- Range-server access-log cross-check (client requests vs server log lines): **%d vs %d**."
             % (tot_srv_client, tot_srv_server))
    s.append("")
    s.append("## Verdict")
    s.append("")
    s.append("Rule: a **VOLE win** requires the median bytes-transferred ratio `VOLE/SQLite <= %.1fx` **and**"
             % THETA_BYTES)
    s.append("the median modelled latency ratio `<= 1.0` at the PRIMARY profile.  `> 1.0` bytes is a **loss**;")
    s.append("anything between is **unresolved**.  All under model %s with constants above (date %s)."
             % (MODEL, MODEL_DATE))
    s.append("")
    s.append("- **Verdict: %s**" % verdict_line)
    s.append("- VOLE-win regions: %s" % (", ".join(wins) if wins else "none"))
    s.append("- VOLE-loss regions: %s" % (", ".join(losses) if losses else "none"))
    s.append("- Unresolved regions: %s" % (", ".join(unresolved) if unresolved else "none"))
    s.append("")
    _decl = len(obs) - sum(1 for r in rows if r["answered"])
    _decl = len(obs) - sum(1 for r in rows if r["answered"])
    _br_all = fmt(overall.get("median_bytes_ratio", float("nan")))
    _lr_all = fmt(overall.get("median_lat_ratio_primary", float("nan")))
    _br_b = fmt(region_out.get("obs=bytes", {}).get("median_bytes_ratio", float("nan")))
    _lr_b = fmt(region_out.get("obs=bytes", {}).get("median_lat_ratio_primary", float("nan")))
    s.append("**How to read this verdict.**  VOLE does **not** achieve a *major* bytes reduction against the")
    s.append("best SQLite interface on this corpus: the median VOLE/SQLite bytes ratio is **%s** (>1 = VOLE" % _br_all)
    s.append("transfers more).  The modelled latency ratio is **%s** because VOLE issues very few requests" % _lr_all)
    s.append("while SQLite issues several, but the gate's first condition (a major byte reduction) fails, so this")
    s.append("is a **LOSS** for the remote-selective byte court.")
    s.append("")
    s.append("Two structural facts explain it and are worth naming:")
    s.append("")
    s.append("1. SQLite's page-level remote read is **extremely selective** for narrow queries (a handful of 4 KiB")
    s.append("   pages, `db/page` in the tens-to-hundreds), because the tuned `full` envelope materializes the")
    s.append("   queried columns into pages.  VOLE's cold remote read must fetch the **descriptor closure**, which")
    s.append("   for most of these documents is roughly the whole encoded document.")
    s.append("2. The one region that is **competitive** is `obs=bytes` (an exact 0..64 byte-range): there the SQLite")
    s.append("   control must read the whole `source_blob` overflow chain, so VOLE bytes are at parity")
    s.append("   (`bytes_ratio` %s) with a large modelled latency advantage (`lat_ratio` %s).  It is recorded as" % (_br_b, _lr_b))
    s.append("   **unresolved**, not a win, because the byte reduction is not *major*.")
    s.append("")
    s.append("Declined observations (excluded from ratios): **%d** of %d (VOLE `resource` on documents that have" % (_decl, len(obs)))
    s.append("no resources is an unsupported capability, rc=6).")
    s.append("")
    s.append("Against the **conservative** whole-`.db` control VOLE would transfer fewer bytes (whole-store /")
    s.append("whole-`.db` < 1 for most documents), but that control is exactly the one the brief says not to")
    s.append("force if a realistic alternative exists.  A page-level remote interface exists, so it is used.")
    s.append("")
    s.append("## What this is NOT / does not prove")
    s.append("")
    s.append("- **It is a MODEL.** No S3, no network egress, no real remote latency was measured. The loopback")
    s.append("  HTTP server measures request COUNT and bytes served; the latency figures are the stated cost")
    s.append("  model, not observations.")
    s.append("- VOLE's class bytes are exact (instrumented), but their **placement** inside each namespace")
    s.append("  region is an upper-bound model: the request count is a **lower bound** on the true scattered read.")
    s.append("- The SQLite page-level interface reads only the pages the query touches **because it may retain**")
    s.append("  the extracted rows; the whole-`.db` download remains the conservative control and is reported.")
    s.append("- No production code changed and **nothing ships**; this is a measurement court on the shipping")
    s.append("  binary.")
    s.append("")
    open(os.path.join(campaign, "SUMMARY.md"), "w").write("\n".join(s) + "\n")

    # ---- summary.json -------------------------------------------------------
    out = {
        "phase": "22.5",
        "model": MODEL,
        "model_equation": MODEL_EQ,
        "model_date": MODEL_DATE,
        "profiles": {n: {"rtt_s": r, "bw_Bps": b, "role": role}
                     for n, r, b, role in PROFILES},
        "theta_bytes": THETA_BYTES,
        "docs": len(ids),
        "observations": len(obs),
        "answered_observations": sum(1 for r in rows if r["answered"]),
        "verdict": verdict_line,
        "verdict_tag": vtag,
        "wins": wins, "losses": losses, "unresolved": unresolved,
        "integrity": {
            "exact_ok": ex_ok, "exact_total": len(exact),
            "cmp_eq": ex_cmp,
            "vole_ranges_verified": int(tot_vver), "vole_ranges_total": int(tot_vreq),
            "sql_ranges_verified": int(tot_sver), "sql_ranges_total": int(tot_sreq),
            "srv_client_requests": int(tot_srv_client),
            "srv_server_requests": int(tot_srv_server),
        },
        "regions": {k: {kk: (None if isinstance(vv, float) and math.isnan(vv) else vv)
                        for kk, vv in v.items() if kk != "rows"}
                    for k, v in region_out.items()},
        "per_observation": [
            {k: (None if isinstance(v, float) and math.isnan(v) else v)
             for k, v in r.items() if k not in ("vole_classes",)} for r in rows
        ],
    }
    json.dump(out, open(os.path.join(campaign, "summary.json"), "w"), indent=1,
              sort_keys=True)

    # ---- counts.txt ---------------------------------------------------------
    c = ["docs=%d" % len(ids),
         "observations=%d" % len(obs),
         "answered=%d" % out["answered_observations"],
         "verdict=%s" % verdict_line,
         "wins=%s" % (",".join(wins) if wins else "none"),
         "losses=%s" % (",".join(losses) if losses else "none"),
         "unresolved=%s" % (",".join(unresolved) if unresolved else "none"),
         "exact_ok=%d/%d" % (ex_ok, len(exact)),
         "vole_ranges_verified=%d/%d" % (int(tot_vver), int(tot_vreq)),
         "sql_ranges_verified=%d/%d" % (int(tot_sver), int(tot_sreq)),
         "srv_client_requests=%d" % int(tot_srv_client),
         "srv_server_requests=%d" % int(tot_srv_server),
         "model=%s" % MODEL, "model_date=%s" % MODEL_DATE]
    open(os.path.join(campaign, "counts.txt"), "w").write("\n".join(c) + "\n")

    print("verdict: %s" % verdict_line)
    print("exact: %d/%d  srv cross-check: %d/%d" % (
        ex_ok, len(exact), int(tot_srv_client), int(tot_srv_server)))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
