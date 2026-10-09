#!/usr/bin/env python3
# Phase 21.5.3 (FIX 4) — stratified real-world multi-format smoke court.
#
# A concrete, bounded first step toward comparing against independently sourced
# real-world documents. It ingests the pinned samples in
# tools/realcorpus/formats-manifest.tsv (fetched into the gitignored
# realformats-v1/documents/), and reports **pooled costs ALONGSIDE medians** so the
# size/time-weighted picture is visible next to the unweighted per-document one.
#
# Stratification: by format (xlsx/pptx/ods/odp/json/yaml) and by size class
# (small < 16 KiB, medium < 256 KiB, large >= 256 KiB). Every present sample must
# materialize byte-exactly (`length + SHA-256 + cmp`); a sample whose bytes are
# absent is recorded as SKIPPED (run tools/realcorpus/fetch-formats.sh).
#
# Emits `$OUT/smoke.json` (+ per-sample raw). FAIL if any present sample is not
# byte-exact.

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
SELECTORS = [["--metadata", "--kind", "metadata"],
             ["--doc-text", "--kind", "text"],
             ["--byte-range", "0..64", "--kind", "exact"]]


def run(cmd, timeout=600):
    return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)


def sha256(b):
    return hashlib.sha256(b).hexdigest()


def size_class(n):
    if n < 16 * 1024:
        return "small(<16KiB)"
    if n < 256 * 1024:
        return "medium(<256KiB)"
    return "large(>=256KiB)"


def read_manifest(path):
    rows = []
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


class Smoke:
    def __init__(self, bin_path, corpus, out):
        self.bin = bin_path
        self.corpus = corpus
        self.out = out
        self.samples = []
        self.skipped = []

    def ingest_one(self, row):
        f = row["format"]
        src = os.path.join(self.corpus, "documents", f, row["id"])
        if not os.path.exists(src):
            self.skipped.append({"id": row["id"], "format": f,
                                 "reason": "bytes absent; run tools/realcorpus/fetch-formats.sh"})
            return None
        raw = open(src, "rb").read()
        if len(raw) != int(row["bytes"]) or sha256(raw) != row["sha256"]:
            self.skipped.append({"id": row["id"], "format": f, "reason": "manifest mismatch on disk"})
            return None
        store = os.path.join(self.out, "stores", row["id"])
        best_build, field = None, None
        for _ in range(REPS):
            shutil.rmtree(store, ignore_errors=True)
            os.makedirs(store, exist_ok=True)
            a = time.monotonic()
            p = run([self.bin, "field-build", src, "--store", store,
                     "--profile", "runtime", "--packed"])
            us = int((time.monotonic() - a) * 1_000_000)
            if p.returncode != 0:
                return {"id": row["id"], "format": f, "size_class": size_class(len(raw)),
                        "error": "field-build rc=%d" % p.returncode, "exact": False}
            field = json.loads(p.stdout)["ingest"]["field"]
            best_build = us if best_build is None else min(best_build, us)
        # exactness + observation round-trip
        out_bin = os.path.join(self.out, "mat-" + row["id"])
        p = run([self.bin, "materialize", "--store", store, "--field", field,
                 "--exact", "--packed", "--output", out_bin])
        exact = p.returncode == 0 and os.path.exists(out_bin) and open(out_bin, "rb").read() == raw
        obs_us, obs_ok = 0, True
        for s in SELECTORS:
            a = time.monotonic()
            q = run([self.bin, "observe", "--store", store, "--field", field, "--packed"] + s)
            obs_us += int((time.monotonic() - a) * 1_000_000)
            obs_ok = obs_ok and q.returncode == 0
        store_bytes = sum(os.path.getsize(os.path.join(dp, fn))
                          for dp, _, fns in os.walk(store) for fn in fns)
        return {"id": row["id"], "format": f, "size_class": size_class(len(raw)),
                "source_bytes": len(raw), "store_bytes": store_bytes,
                "build_us": best_build, "observe_us": obs_us, "obs_ok": obs_ok,
                "exact": exact, "field": field}

    def run(self, manifest):
        rows = read_manifest(manifest)
        for row in rows:
            r = self.ingest_one(row)
            if r is not None:
                self.samples.append(r)
        # strata
        def agg(rs):
            if not rs:
                return None
            builds = [x["build_us"] for x in rs if "build_us" in x]
            stores = [x["store_bytes"] for x in rs if "store_bytes" in x]
            srcs = [x["source_bytes"] for x in rs if "source_bytes" in x]
            per_byte = [x["build_us"] / x["store_bytes"] for x in rs
                        if x.get("store_bytes")]
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
                "all_exact": all(x.get("exact") for x in rs),
            }

        by_format = {}
        for x in self.samples:
            by_format.setdefault(x["format"], []).append(x)
        by_size = {}
        for x in self.samples:
            by_size.setdefault(x["size_class"], []).append(x)
        all_exact = all(x.get("exact") for x in self.samples) and self.samples
        report = {
            "phase": "21.5.3 — stratified real-world multi-format smoke",
            "manifest": manifest,
            "corpus_dir": self.corpus,
            "reps": REPS,
            "storage_accounting": "sum of regular-file sizes (os.path.getsize walk); never du -sb (ADR-0049)",
            "samples": self.samples,
            "skipped": self.skipped,
            "overall": agg(self.samples),
            "by_format": {k: agg(v) for k, v in sorted(by_format.items())},
            "by_size_class": {k: agg(v) for k, v in sorted(by_size.items())},
            "verdict": "PASS" if all_exact else "FAIL",
        }
        with open(os.path.join(self.out, "smoke.json"), "w") as f:
            json.dump(report, f, indent=2, sort_keys=True)
        return report


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", required=True)
    ap.add_argument("--corpus", default="realformats-v1")
    ap.add_argument("--manifest", default="tools/realcorpus/formats-manifest.tsv")
    ap.add_argument("--out", required=True)
    ns = ap.parse_args(argv)
    os.makedirs(ns.out, exist_ok=True)
    rep = Smoke(ns.bin, ns.corpus, ns.out).run(ns.manifest)
    print(json.dumps({"verdict": rep["verdict"],
                      "ran": len(rep["samples"]),
                      "skipped": len(rep["skipped"])}, indent=2))
    return 0 if rep["verdict"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
