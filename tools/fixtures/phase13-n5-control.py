#!/usr/bin/env python3
# Phase 13.5 — mechanical "package-index-only" negative control for gate N5.
#
# The N5 gate asks whether the small-document VOLE win is reproducible by
# `unzip -p` + `substr` at the same boundary. This script builds that lane
# literally: it decompresses every ZIP member to raw bytes with the Python
# stdlib (`zipfile`, the mechanical equivalent of `unzip -p`) and tries to
# answer each pre-registered selector with byte operations only — no XML
# parsing, no relationship graph, no format semantics.
#
# It reports, per case:
#   * selector_class      exact | literal-search | structural
#   * package_resolvable  can the package index + byte substring answer THIS
#                         selector at all (ignoring cost)?
#   * value_in_raw        is the expected value a contiguous substring of some
#                         raw member (the weakest, generous reading)?
#   * value_member        the member that contains it (if any)
#   * brute_member_sha    for exact-member cases, whether some member's SHA-256
#                         equals the expected (a package index that hashes every
#                         member could match by brute force, without resolving
#                         the logical resource)
#
# Only `docx`/`epub` documents are considered: N5 is about package formats.
#
# Usage:
#   python3 tools/fixtures/phase13-n5-control.py CORPUS_DIR SCHEDULE.json OUT.json

import hashlib
import json
import os
import sys
import zipfile

LITERAL_SEARCH_IDS = {"search"}


def classify(cid):
    if cid in ("full-source", "exact-member"):
        return "exact"
    if cid in LITERAL_SEARCH_IDS:
        return "literal-search"
    return "structural"


def main(argv):
    if len(argv) != 4:
        sys.stderr.write(__doc__)
        return 2
    corpus, schedule_path, out_path = argv[1], argv[2], argv[3]
    with open(schedule_path, "r", encoding="utf-8") as fh:
        sched = json.load(fh)

    docs = []
    summary = {
        "documents": 0,
        "cases": 0,
        "package_resolvable": 0,
        "value_in_raw": 0,
        "exact": 0,
        "literal_search": 0,
        "structural": 0,
        "resolvable_exact": 0,
        "resolvable_literal_search": 0,
        "resolvable_structural": 0,
        "brute_member_sha_matches": 0,
    }

    for doc in sched["documents"]:
        fmt = doc["format"]
        if fmt not in ("docx", "epub"):
            continue
        summary["documents"] += 1
        path = os.path.join(corpus, doc["source"])
        with zipfile.ZipFile(path) as z:
            members = [(n, z.read(n)) for n in z.namelist()]
        member_sha = {hashlib.sha256(b).hexdigest() for _, b in members}

        records = []
        for case in doc["cases"]:
            cid = case["id"]
            arg = case["arg"]
            ek = case["expected_kind"]
            exp = case["expected"]
            cls = classify(cid)
            rec = {
                "id": cid,
                "arg": arg,
                "expected_kind": ek,
                "selector_class": cls,
                "package_resolvable": False,
                "value_in_raw": False,
                "value_member": None,
                "brute_member_sha": False,
                "note": "",
            }
            if cls == "exact":
                if cid == "full-source":
                    rec["package_resolvable"] = True
                    rec["note"] = "whole source bytes (cat); no package traversal needed"
                else:  # exact-member: arg `resource-bytes:N` names a logical resource
                    rec["brute_member_sha"] = exp in member_sha
                    rec["package_resolvable"] = False
                    rec["note"] = (
                        "selector names a logical resource; the package index "
                        "cannot resolve which member it is"
                        + (
                            " (a member SHA matches only by brute force)"
                            if rec["brute_member_sha"]
                            else ""
                        )
                    )
                    if rec["brute_member_sha"]:
                        summary["brute_member_sha_matches"] += 1
                summary["exact"] += 1
            elif cls == "literal-search":
                rec["package_resolvable"] = True
                rec["note"] = "byte-substring scan of members (unzip -p | substr)"
                summary["literal_search"] += 1
            else:
                rec["note"] = (
                    "structural selector; requires format-native parsing "
                    "(XML run assembly / table grid / relationship graph), "
                    "not a byte substring"
                )
                summary["structural"] += 1

            if ek != "sha256" and exp:
                needle = exp.encode("utf-8", "surrogatepass")
                for n, b in members:
                    if needle in b:
                        rec["value_in_raw"] = True
                        rec["value_member"] = n
                        break

            if rec["package_resolvable"]:
                summary["package_resolvable"] += 1
                summary["resolvable_" + cls.replace("-", "_")] += 1
            if rec["value_in_raw"]:
                summary["value_in_raw"] += 1
            summary["cases"] += 1
            records.append(rec)

        docs.append(
            {
                "name": doc["name"],
                "format": fmt,
                "source": doc["source"],
                "source_sha256": doc["sha256"],
                "member_count": len(members),
                "records": records,
            }
        )

    result = {
        "generator": "tools/fixtures/phase13-n5-control.py",
        "lane": "package-index-only: zipfile.read (mechanical `unzip -p`) + byte substring",
        "corpus": corpus,
        "schedule": schedule_path,
        "summary": summary,
        "documents": docs,
    }
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(result, fh, indent=2, sort_keys=True)
        fh.write("\n")

    s = summary
    print(
        "package-index-only lane: resolvable {r}/{c} cases "
        "(exact {e}, literal-search {l}, structural-declined {st}); "
        "value-in-raw {v}/{c}".format(
            r=s["package_resolvable"],
            c=s["cases"],
            e=s["exact"],
            l=s["literal_search"],
            st=s["structural"],
            v=s["value_in_raw"],
        )
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
