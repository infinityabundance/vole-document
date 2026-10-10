#!/usr/bin/env python3
# Phase 21.15 (ITEM 2) — stratified real-world multi-format court driver.
#
# A genuinely stratified comparison against independently sourced real-world
# documents, not a 12-document smoke. It ingests every sample pinned in
# tools/realcorpus/formats-manifest.tsv (14 formats: XLSX, PPTX, ODS, ODP, JSON,
# YAML, CSV, Markdown, XML, HTML, TOML, JSONL, EML, Parquet), plus a
# **malformed/hostile** stratum derived deterministically from pinned real bytes
# (tools/realcorpus/hostile-strata.tsv: truncation, wrong-magic, mid-structure
# corruption), and reports **pooled costs ALONGSIDE medians**, stratified by
# format, by size class, by complexity, and by the malformed/hostile flag, and
# separated by **workload** (narrow observation `observe --metadata` vs full
# closure `materialize --exact`).
#
# Exactness (the byte-authority court): each sample's bytes are copied to a
# scratch path, `field-build` runs on that copy, the copy is DELETED, and a fresh
# `materialize --exact` process must reproduce `length + SHA-256 + cmp` against
# the in-memory original. A sample whose bytes are absent is recorded SKIPPED (it
# is never fabricated); an absent/blocked source is recorded with its reason.
#
# Emits `$OUT/stratified.json` (+ per-sample raw). Verdict PASS iff every present
# real sample and every hostile sample closes byte-exactly.

import argparse
import hashlib
import json
import os
import shutil
import statistics
import subprocess
import sys
import time

REPS = 2
NARROW = ["--metadata", "--kind", "metadata"]
FULL = ["--exact", "--packed"]


def run(cmd, timeout=1200):
    return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)


def sha256(b):
    return hashlib.sha256(b).hexdigest()


def size_class(n):
    if n < 16 * 1024:
        return "small(<16KiB)"
    if n < 256 * 1024:
        return "medium(<256KiB)"
    return "large(>=256KiB)"


def read_tsv(path):
    rows = []
    if not os.path.exists(path):
        return rows
    with open(path) as f:
        hdr = f.readline().rstrip("\n").split("\t")
        for line in f:
            if not line.strip():
                continue
            vals = line.rstrip("\n").split("\t")
            if len(vals) != len(hdr):
                continue
            rows.append(dict(zip(hdr, vals)))
    return rows


def apply_transform(data, transform):
    kind, arg = transform.split(":", 1)
    if kind == "truncate":
        return data[:int(arg)]
    if kind == "replace4":
        return arg.encode("ascii") + data[4:]
    if kind == "drop-last":
        return data[: len(data) - int(arg)]
    raise ValueError("unknown transform %r" % transform)


class Court:
    def __init__(self, bin_path, corpus, out):
        self.bin = bin_path
        self.corpus = corpus
        self.out = out
        self.tmp = os.path.join(out, "tmp")
        os.makedirs(self.tmp, exist_ok=True)
        self.samples = []
        self.skipped = []
        self.blocked = []

    def _measure(self, src_bytes, sid, fmt, complexity, stratum):
        """Ingest one byte string; return a sample record (or raise)."""
        scratch = os.path.join(self.tmp, sid + ".src")
        with open(scratch, "wb") as f:
            f.write(src_bytes)
        store = os.path.join(self.out, "stores", sid)
        best_build, best_narrow, best_full, detected = None, None, None, None
        field = None
        for _ in range(REPS):
            shutil.rmtree(store, ignore_errors=True)
            os.makedirs(store, exist_ok=True)
            a = time.monotonic()
            p = run([self.bin, "field-build", scratch, "--store", store,
                     "--profile", "runtime", "--packed"])
            us = int((time.monotonic() - a) * 1_000_000)
            if p.returncode != 0:
                return {"id": sid, "format": fmt, "stratum": stratum,
                        "size_class": size_class(len(src_bytes)),
                        "complexity": complexity, "exact": False,
                        "error": "field-build rc=%d" % p.returncode}
            info = json.loads(p.stdout)
            field = info["ingest"]["field"]
            detected = info["ingest"].get("format")
            best_build = us if best_build is None else min(best_build, us)
        # narrow observation workload (bounded projection)
        a = time.monotonic()
        q = run([self.bin, "observe", "--store", store, "--field", field,
                 "--packed"] + NARROW)
        best_narrow = int((time.monotonic() - a) * 1_000_000)
        obs_ok = q.returncode == 0
        # full closure workload: delete the source, then rematerialize in a fresh
        # process and compare to the in-memory original.
        os.remove(scratch)
        out_bin = os.path.join(self.tmp, sid + ".out")
        a = time.monotonic()
        m = run([self.bin, "materialize", "--store", store, "--field", field,
                 "--output", out_bin] + FULL)
        best_full = int((time.monotonic() - a) * 1_000_000)
        exact = (m.returncode == 0 and os.path.exists(out_bin)
                 and open(out_bin, "rb").read() == src_bytes)
        store_bytes = sum(os.path.getsize(os.path.join(dp, fn))
                          for dp, _, fns in os.walk(store) for fn in fns)
        return {"id": sid, "format": fmt, "format_detected": detected,
                "stratum": stratum, "size_class": size_class(len(src_bytes)),
                "complexity": complexity, "source_bytes": len(src_bytes),
                "store_bytes": store_bytes, "build_us": best_build,
                "narrow_us": best_narrow, "full_us": best_full,
                "obs_ok": obs_ok, "exact": exact, "field": field}

    def ingest_real(self, row):
        f = row["format"]
        src = os.path.join(self.corpus, "documents", f, row["id"])
        if not os.path.exists(src):
            self.skipped.append({"id": row["id"], "format": f, "stratum": "real",
                                 "reason": "bytes absent; run tools/realcorpus/fetch-formats.sh"})
            return
        raw = open(src, "rb").read()
        if len(raw) != int(row["bytes"]) or sha256(raw) != row["sha256"]:
            self.skipped.append({"id": row["id"], "format": f, "stratum": "real",
                                 "reason": "manifest mismatch on disk"})
            return
        self.samples.append(self._measure(raw, row["id"], f,
                                          row.get("complexity", "unknown"), "real"))

    def ingest_hostile(self, row, real_by_id):
        base = row["base_id"]
        if base not in real_by_id:
            self.blocked.append({"id": row["id"], "stratum": "hostile",
                                 "reason": "base %s not in manifest" % base})
            return
        b = real_by_id[base]
        src = os.path.join(self.corpus, "documents", b["format"], base)
        if not os.path.exists(src):
            self.blocked.append({"id": row["id"], "stratum": "hostile",
                                 "reason": "base bytes absent: %s" % base})
            return
        base_raw = open(src, "rb").read()
        if sha256(base_raw) != b["sha256"]:
            self.blocked.append({"id": row["id"], "stratum": "hostile",
                                 "reason": "base bytes mismatch: %s" % base})
            return
        derived = apply_transform(base_raw, row["transform"])
        if len(derived) != int(row["bytes"]) or sha256(derived) != row["sha256"]:
            self.blocked.append({"id": row["id"], "stratum": "hostile",
                                 "reason": "derived bytes do not match the pinned recipe"})
            return
        self.samples.append(self._measure(derived, row["id"], b["format"], "hostile", "hostile"))

    def run(self, manifest_path, hostile_path):
        manifest = read_tsv(manifest_path)
        real_by_id = {r["id"]: r for r in manifest}
        for row in manifest:
            self.ingest_real(row)
        for row in read_tsv(hostile_path):
            self.ingest_hostile(row, real_by_id)

        def agg(rs):
            if not rs:
                return None
            ok = [x for x in rs if "build_us" in x]
            builds = [x["build_us"] for x in ok]
            stores = [x["store_bytes"] for x in ok]
            srcs = [x["source_bytes"] for x in ok]
            narrow = [x["narrow_us"] for x in ok]
            full = [x["full_us"] for x in ok]
            per_byte = [x["build_us"] / x["store_bytes"] for x in ok if x.get("store_bytes")]
            return {
                "n": len(rs),
                "pooled_build_us": sum(builds),
                "median_build_us": statistics.median(builds) if builds else None,
                "pooled_store_bytes": sum(stores),
                "median_store_bytes": statistics.median(stores) if stores else None,
                "pooled_source_bytes": sum(srcs),
                "median_source_bytes": statistics.median(srcs) if srcs else None,
                "pooled_build_us_per_store_byte": (sum(builds) / sum(stores)) if stores and sum(stores) else None,
                "median_build_us_per_store_byte": statistics.median(per_byte) if per_byte else None,
                "pooled_narrow_us": sum(narrow),
                "median_narrow_us": statistics.median(narrow) if narrow else None,
                "pooled_full_us": sum(full),
                "median_full_us": statistics.median(full) if full else None,
                "pooled_narrow_us_per_source_byte": (sum(narrow) / sum(srcs)) if srcs and sum(srcs) else None,
                "pooled_full_us_per_source_byte": (sum(full) / sum(srcs)) if srcs and sum(srcs) else None,
                "all_exact": all(x.get("exact") for x in rs),
                "opaque_observed": sum(1 for x in ok if x.get("format_detected") == "opaque"),
            }

        def group(keyfn):
            g = {}
            for x in self.samples:
                g.setdefault(keyfn(x), []).append(x)
            return {k: agg(v) for k, v in sorted(g.items())}

        real = [x for x in self.samples if x.get("stratum") == "real"]
        hostile = [x for x in self.samples if x.get("stratum") == "hostile"]
        all_exact = bool(self.samples) and all(x.get("exact") for x in self.samples)
        report = {
            "phase": "21.15 — stratified real-world multi-format court",
            "manifest": manifest_path,
            "hostile_recipes": hostile_path,
            "corpus_dir": self.corpus,
            "reps": REPS,
            "narrow_selector": " ".join(NARROW),
            "full_selector": " ".join(FULL),
            "storage_accounting": "sum of regular-file sizes (os.path.getsize walk); never du -sb (ADR-0049)",
            "samples": self.samples,
            "skipped": self.skipped,
            "blocked": self.blocked,
            "overall": agg(self.samples),
            "real": agg(real),
            "hostile": agg(hostile),
            "by_format": group(lambda x: x["format"]),
            "by_size_class": group(lambda x: x["size_class"]),
            "by_complexity": group(lambda x: x["complexity"]),
            "by_stratum": group(lambda x: x["stratum"]),
            "by_format_size": group(lambda x: "%s/%s" % (x["format"], x["size_class"])),
            "verdict": "PASS" if all_exact else "FAIL",
            "counts": {
                "samples": len(self.samples),
                "real": len(real),
                "hostile": len(hostile),
                "ran_real": sum(1 for x in real if "build_us" in x),
                "ran_hostile": sum(1 for x in hostile if "build_us" in x),
                "skipped": len(self.skipped),
                "blocked": len(self.blocked),
                "exact": sum(1 for x in self.samples if x.get("exact")),
            },
        }
        with open(os.path.join(self.out, "stratified.json"), "w") as f:
            json.dump(report, f, indent=2, sort_keys=True)
        return report


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", required=True)
    ap.add_argument("--corpus", default="realformats-v1")
    ap.add_argument("--manifest", default="tools/realcorpus/formats-manifest.tsv")
    ap.add_argument("--hostile", default="tools/realcorpus/hostile-strata.tsv")
    ap.add_argument("--out", required=True)
    ns = ap.parse_args(argv)
    os.makedirs(ns.out, exist_ok=True)
    rep = Court(ns.bin, ns.corpus, ns.out).run(ns.manifest, ns.hostile)
    print(json.dumps({"verdict": rep["verdict"], **rep["counts"]}, indent=2))
    return 0 if rep["verdict"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
