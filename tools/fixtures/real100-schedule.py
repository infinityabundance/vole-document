#!/usr/bin/env python3
# real100-v1 frontier-court schedule (PRE-REGISTERED).
#
# Reads real100-v1/manifest.tsv and emits, per document, the deterministic
# workload ops the court runs in every lane. The op set is fixed by *format*
# only — never by a VOLE runtime result or a per-document measurement — so the
# schedule is a function of the frozen corpus alone and is reproducible from
# `manifest.tsv` (whose SHA-256 is recorded in the receipt).
#
# The corpus is frozen (one SHA-256 per row, verified before the court runs).
# This file is committed *before* the first measurement.
#
# Usage: python3 tools/fixtures/real100-schedule.py MANIFEST.tsv CORPUS_DIR OUT.json

import hashlib
import json
import os
import sys

REPEAT_N = 5

# The common observation surface, mapped to a VOLE selector per format. `None`
# means the format has no such native selector and a typed decline is expected.
def v_selector(fmt, workload):
    if workload == "text_once":
        return ("page:1", "text") if fmt == "pdf" else ("block:0", "text")
    if workload == "heading":
        return None if fmt == "pdf" else ("heading:0", "text")
    if workload == "table":
        return None if fmt == "pdf" else ("table:0", "text")
    if workload == "resource":
        return None if fmt == "pdf" else ("resource:0", "metadata")
    if workload == "metadata":
        return ("metadata", "metadata")
    return None


WORKLOADS = ["text_once", "heading", "table", "resource", "metadata"]
EXT = {"pdf": "pdf", "docx": "docx", "epub": "epub"}


def main(argv):
    if len(argv) != 4:
        sys.stderr.write(__doc__)
        return 2
    manifest, corpus, out_path = argv[1], argv[2], argv[3]
    with open(manifest, "rb") as fh:
        manifest_bytes = fh.read()
    lines = manifest_bytes.decode("utf-8").splitlines()
    hdr = lines[0].split("\t")
    docs = []
    for line in lines[1:]:
        if not line.strip():
            continue
        r = dict(zip(hdr, line.split("\t")))
        fmt = r["format"]
        path = os.path.join(corpus, r["agency"], fmt, r["id"] + "." + EXT[fmt])
        ops = []
        for w in WORKLOADS:
            sel = v_selector(fmt, w)
            if sel is None:
                ops.append({"workload": w, "supported": False})
            else:
                ops.append({"workload": w, "supported": True,
                            "selector": sel[0], "kind": sel[1]})
        docs.append({
            "id": r["id"],
            "agency": r["agency"],
            "format": fmt,
            "path": path,
            "sha256": r["sha256"],
            "byte_len": int(r["byte_len"]),
            "size_class": r["size_class"],
            "structural_tags": r["structural_tags"],
            "document_type": r["document_type"],
            "cross_format_family_id": r.get("cross_format_family_id", ""),
            "ops": ops,
        })
    schedule = {
        "campaign": "real100-frontier",
        "corpus": corpus,
        "manifest": manifest,
        "manifest_sha256": hashlib.sha256(manifest_bytes).hexdigest(),
        "repeat_n": REPEAT_N,
        "workloads": WORKLOADS + ["text_repeat", "exact"],
        "documents": docs,
    }
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(schedule, fh, indent=2, sort_keys=True)
        fh.write("\n")
    print(json.dumps({"documents": len(docs),
                      "manifest_sha256": schedule["manifest_sha256"]}))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
