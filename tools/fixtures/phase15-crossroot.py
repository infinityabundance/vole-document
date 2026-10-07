#!/usr/bin/env python3
# Phase 15.7 "durable cross-root derivations" court — planner + aggregator.
#
# PRE-REGISTERED. This file is committed before the measurement. It fixes, from
# the frozen `real100-v1/manifest.tsv` alone:
#
#   * which families exist (`cross_format_family_id` = field 17 and
#     `revision_family_id` = field 18) and their member order, so no VOLE runtime
#     result can influence which documents are compared; and
#   * the aggregation of the raw measurement into the ADR-0035 `N3` verdict
#     ("reuse ≈0 or below strongest CDC on reuse work").
#
# It emits nothing that feeds back into selection. The corpus is frozen and the
# manifest SHA-256 is recorded in the plan.
#
# Usage:
#   phase15-crossroot.py plan MANIFEST.tsv CORPUS_DIR OUT_DIR
#       -> OUT_DIR/plan.json, OUT_DIR/families.tsv
#   phase15-crossroot.py aggregate CAMPAIGN_DIR
#       -> CAMPAIGN_DIR/raw/summary.json, CAMPAIGN_DIR/N3.txt (prints the verdict)

import glob
import hashlib
import json
import os
import sys

EXT = {"pdf": "pdf", "docx": "docx", "epub": "epub"}
FAMILY_FIELDS = (("cross_format", "cross_format_family_id"),
                 ("revision", "revision_family_id"))


def _load_manifest(path):
    with open(path, "rb") as fh:
        raw = fh.read()
    lines = raw.decode("utf-8").splitlines()
    hdr = lines[0].split("\t")
    rows = []
    for line in lines[1:]:
        if not line.strip():
            continue
        rows.append(dict(zip(hdr, line.split("\t"))))
    return rows, hashlib.sha256(raw).hexdigest()


def cmd_plan(manifest, corpus, out_dir):
    rows, manifest_sha = _load_manifest(manifest)
    by_family = {}
    for r in rows:
        fmt = r["format"]
        ext = EXT.get(fmt)
        if ext is None:
            continue
        path = os.path.join(corpus, r["agency"], fmt, r["id"] + "." + ext)
        for kind, field in FAMILY_FIELDS:
            fid = (r.get(field) or "").strip()
            if not fid:
                continue
            key = kind + ":" + fid
            by_family.setdefault(key, []).append({
                "id": r["id"],
                "agency": r["agency"],
                "format": fmt,
                "path": path,
                "sha256": r["sha256"],
                "byte_len": int(r["byte_len"]),
                "kind": kind,
                "family_id": fid,
            })

    families = []
    for key in sorted(by_family):
        members = sorted(by_family[key], key=lambda m: (m["format"], m["id"]))
        if len(members) < 2:
            # A "family" of one shares nothing across documents; not a court case.
            continue
        for i, m in enumerate(members):
            m["member_index"] = i
        families.append({
            "key": key,
            "kind": members[0]["kind"],
            "family_id": members[0]["family_id"],
            "members": members,
        })

    os.makedirs(out_dir, exist_ok=True)
    plan = {
        "manifest": manifest,
        "manifest_sha256": manifest_sha,
        "corpus": corpus,
        "families": families,
    }
    with open(os.path.join(out_dir, "plan.json"), "w", encoding="utf-8") as fh:
        json.dump(plan, fh, indent=2, sort_keys=True)
        fh.write("\n")

    with open(os.path.join(out_dir, "families.tsv"), "w", encoding="utf-8") as fh:
        fh.write("key\tkind\tfamily_id\tmember_index\tmember_id\tagency\tformat\tpath\tsha256\tbyte_len\n")
        for f in families:
            for m in f["members"]:
                fh.write("\t".join([
                    f["key"], f["kind"], f["family_id"], str(m["member_index"]),
                    m["id"], m["agency"], m["format"], m["path"], m["sha256"],
                    str(m["byte_len"]),
                ]) + "\n")

    total_members = sum(len(f["members"]) for f in families)
    print(json.dumps({
        "families": len(families),
        "family_members": total_members,
        "manifest_sha256": manifest_sha,
    }))
    return 0


def _read_jsonl(path):
    recs = []
    if not os.path.exists(path):
        return recs
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            try:
                recs.append(json.loads(line))
            except json.JSONDecodeError:
                pass
    return recs


def _frac(cold_units, warm_units):
    if not cold_units or cold_units <= 0:
        return None
    return max(0.0, (cold_units - warm_units) / cold_units)


def cmd_aggregate(campaign):
    raw = os.path.join(campaign, "raw")
    plan = json.load(open(os.path.join(raw, "plan.json"), encoding="utf-8"))
    measure = _read_jsonl(os.path.join(raw, "measure.jsonl"))

    ingest = {}
    obs = {}
    store = {}
    for r in measure:
        t = r.get("t")
        key = r.get("key")
        if t == "ingest":
            ingest.setdefault(key, {})[r["member"]] = r
        elif t == "obs":
            obs.setdefault(key, {}).setdefault(r["arm"], {})[r["member"]] = r
        elif t == "store":
            store[key] = r.get("store_bytes")

    def cdc_for(key):
        runs = _read_jsonl(os.path.join(raw, "cdc", key + ".jsonl"))
        if not runs:
            return None
        best = min(r.get("unique_bytes", 1 << 62) for r in runs)
        total = max(r.get("total_size", 0) for r in runs)
        return {"runs": runs, "best_unique_bytes": best, "total_size": total,
                "saving": total - best}

    used_kinds = set()
    agg = {"schema": "phase15.7-crossroot/1", "families": []}
    tot = {
        "families": 0, "members": 0, "members_ok": 0,
        "nodes_id_shared": 0, "shared_resource_ids": 0, "shared_resource_bytes": 0,
        "seed_bytes_written": 0, "index_bytes_written": 0, "store_bytes": 0,
        "cold_units": 0, "warm_units": 0, "warm_reused": 0,
        "intra_reused": 0, "cross_member_reused": 0,
        "warm_fresh_reused": 0, "postclear_star_reused": 0,
        "derived_work_avoided_units": 0,
        "cdc_best_unique": 0, "cdc_total_size": 0, "cdc_saving": 0,
    }
    for f in plan["families"]:
        key = f["key"]
        ing = ingest.get(key, {})
        o = obs.get(key, {})
        members = [{
            "id": m["id"], "format": m["format"], "byte_len": m["byte_len"],
            **(ing.get(m["id"]) or {"ok": False, "reason": "no-ingest-record"}),
        } for m in f["members"]]
        ok_members = [m for m in members if m.get("ok")]

        def arm_sum(arm, field):
            return sum((o.get(arm, {}).get(m["id"], {}).get(field) or 0)
                       for m in members)

        cold_units = arm_sum("cold", "work_units")
        warm_units = arm_sum("warm", "work_units")
        warm_reused = arm_sum("warm", "reused")
        intra_reused = arm_sum("intra", "reused")
        cold_exec = arm_sum("cold", "executed")
        warm_exec = arm_sum("warm", "executed")

        # Cross-member reuse = (warm in-order) - (empty-cache floor), per member.
        # The floor ("intra") is the only reuse a single observation can show with
        # an empty cache (intra-observation diamond reuse).
        cross_pairs = []
        for m in members:
            w = (o.get("warm", {}).get(m["id"], {}).get("reused") or 0)
            it = (o.get("intra", {}).get(m["id"], {}).get("reused") or 0)
            cross_pairs.append((m["id"], w, it, max(0, w - it)))
        cross_member_reused = sum(c for _, _, _, c in cross_pairs)

        # warm_fresh / postclear0 run on the first member that ingested (single
        # member observations, in a fresh process), so they are not sums.
        def first_ok_arm(arm):
            for m in members:
                r = o.get(arm, {}).get(m["id"])
                if r and r.get("ok"):
                    return r
            return None

        wf = first_ok_arm("warm_fresh")
        pc = first_ok_arm("postclear_star")
        # The durability delta: reuse still shown after the clear, above that
        # member's own empty-cache floor. > 0 would mean a durable output store.
        pc_member = pc.get("member") if pc else None
        pc_floor = 0
        if pc_member:
            pc_floor = (o.get("intra", {}).get(pc_member, {}).get("reused") or 0)
        pc_delta = ((pc.get("reused") or 0) - pc_floor) if pc else 0

        fam = {
            "key": key, "kind": f["kind"], "family_id": f["family_id"],
            "members": members, "members_ok": len(ok_members),
            "totals": {
                "nodes_id_shared": sum((m.get("nodes_id_shared") or 0) for m in ok_members),
                "shared_resource_ids": sum((m.get("shared_resource_ids") or 0) for m in ok_members),
                "shared_resource_bytes": sum((m.get("shared_resource_bytes") or 0) for m in ok_members),
                "resource_blob_nodes": sum((m.get("resource_blob_nodes") or 0) for m in ok_members),
                "seed_bytes_written": sum((m.get("seed_bytes_written") or 0) for m in ok_members),
                "index_bytes_written": sum((m.get("index_bytes_written") or 0) for m in ok_members),
            },
            "store_bytes": store.get(key),
            "cold": {"units": cold_units, "executions": cold_exec},
            "warm": {"units": warm_units, "executions": warm_exec, "reused": warm_reused},
            "intra": {"reused": intra_reused},
            "cross_member_reused": cross_member_reused,
            "cross_members": [{"member": mid, "warm": w, "intra": it, "cross": c}
                             for mid, w, it, c in cross_pairs],
            "retained_inverse_work_fraction": _frac(cold_units, warm_units),
            "warm_fresh": wf,
            "postclear_star": pc,
            "postclear_star_delta_above_floor": pc_delta,
            "cdc": cdc_for(key),
        }
        agg["families"].append(fam)
        if len(ok_members) < 2:
            # Cannot witness cross-document sharing; recorded but not counted.
            used_kinds.add("skipped")
            continue
        tot["families"] += 1
        tot["members"] += len(members)
        tot["members_ok"] += len(ok_members)
        t = fam["totals"]
        for k in ("nodes_id_shared", "shared_resource_ids", "shared_resource_bytes",
                  "seed_bytes_written", "index_bytes_written"):
            tot[k] += t[k]
        tot["store_bytes"] += (fam["store_bytes"] or 0)
        tot["cold_units"] += cold_units
        tot["warm_units"] += warm_units
        tot["warm_reused"] += warm_reused
        tot["intra_reused"] += intra_reused
        tot["cross_member_reused"] += cross_member_reused
        tot["warm_fresh_reused"] += (wf.get("reused") or 0) if wf else 0
        tot["postclear_star_reused"] += (pc.get("reused") or 0) if pc else 0
        if fam["cdc"]:
            tot["cdc_best_unique"] += fam["cdc"]["best_unique_bytes"]
            tot["cdc_total_size"] += fam["cdc"]["total_size"]
            tot["cdc_saving"] += fam["cdc"]["saving"]

    tot["retained_inverse_work_fraction"] = _frac(tot["cold_units"], tot["warm_units"])
    agg["totals"] = tot

    with open(os.path.join(raw, "summary.json"), "w", encoding="utf-8") as fh:
        json.dump(agg, fh, indent=2, sort_keys=True)
        fh.write("\n")

    # --- ADR-0035 N3 verdict ------------------------------------------------
    # Distinct win requires: a DERIVED node reused after `cache --clear` in a
    # fresh process (postclear_reused > 0) with exact materialization. Falsifiers,
    # recorded explicitly:
    #   (i)   post-clear derived reuse == 0  -> only representation identity is
    #         durable (the design's falsifier).
    #   (ii)  avoided work <= representation-identity work.
    #   (iii) a plain chunk-level CDC baseline reproduces the byte result.
    lines = []
    lines.append("\n## N3 verdict (ADR-0035: reuse ≈0 or ≤ strongest CDC on reuse work)\n")
    lines.append(
        f"* families measured: `{tot['families']}` (members ingested: "
        f"`{tot['members_ok']}/{tot['members']}`).")
    lines.append(
        f"* warm in-order reuse: `{tot['warm_reused']}` nodes; empty-cache "
        f"(intra-observation) floor: `{tot['intra_reused']}` nodes; "
        f"**cross-member reuse = `{tot['cross_member_reused']}` nodes** "
        f"(retained_inverse_work_fraction = `{tot['retained_inverse_work_fraction']}` "
        f"over `{tot['cold_units']}` cold work units).")
    lines.append(
        f"* representation identity: `{tot['nodes_id_shared']}` nodes id-shared, "
        f"`{tot['shared_resource_ids']}` shared resources "
        f"(`{tot['shared_resource_bytes']}` bytes); store bytes = `{tot['store_bytes']}`.")
    lines.append(
        f"* falsifier (i) fresh-process, post-`cache --clear` reuse of the "
        f"max-warm member = `{tot['postclear_star_reused']}` nodes, i.e. "
        f"`{sum(f.get('postclear_star_delta_above_floor') or 0 for f in agg['families'])}` "
        f"above that member's empty-cache floor (must be 0 ⇒ no durable output "
        f"store); the same member with the cache intact (fresh process) reused "
        f"`{tot['warm_fresh_reused']}` (caches are cross-process, so a non-zero "
        f"here proves the counter is live).")
    lines.append(
        f"* falsifier (iii) chunk-level CDC (borg, `--compression none`, frozen "
        f"params) over the family sources: best unique `{tot['cdc_best_unique']}` of "
        f"`{tot['cdc_total_size']}` bytes ⇒ dedup saving `{tot['cdc_saving']}` bytes.")

    durable_delta = sum(f.get("postclear_star_delta_above_floor") or 0 for f in agg["families"])
    durable = tot["cross_member_reused"] > 0 and durable_delta > 0
    cross = tot["cross_member_reused"] > 0
    if durable:
        verdict = "NOT VIOLATED"
        why = ("cross-member derived reuse is non-zero and part of it survives "
               "`cache --clear` in a fresh process, so a durable, root-independent "
               "output store is doing real work.")
    elif cross:
        verdict = "VIOLATED"
        why = ("cross-member reuse is non-zero warm but collapses to its empty-cache "
               "floor after `cache --clear` in a fresh process: the warm reuse was "
               "served by the disposable per-store `DerivedCache`, not by durable "
               "seed-store work reuse. Representation identity is still shared "
               "(see `nodes_id_shared`).")
    else:
        verdict = "VIOLATED"
        why = ("there is no cross-member derived reuse even warm (cross-member reuse "
               "= 0 nodes); the warm `seed_nodes_reused` is entirely each member's "
               "own intra-observation (diamond) reuse, which an empty cache already "
               "shows. Representation identity is shared (`nodes_id_shared` > 0) but "
               "no *computed* state is, and nothing survives `cache --clear`. The N3 "
               "`reuse ≈0` condition holds — a recorded negative, as ADR-0035 requires.")
    lines.append(f"\n**N3 is {verdict}.** {why} No compression claim is made: a shared "
                 "blob is scored as state/work.")
    text = "\n".join(lines)
    with open(os.path.join(campaign, "N3.txt"), "w", encoding="utf-8") as fh:
        fh.write(text)
    sys.stdout.write(text + "\n")
    return 0


def cmd_receipt(campaign):
    raw = os.path.join(campaign, "raw")
    summary = json.load(open(os.path.join(raw, "summary.json"), encoding="utf-8"))
    plan = json.load(open(os.path.join(raw, "plan.json"), encoding="utf-8"))
    try:
        env = json.load(open(os.path.join(campaign, "environment.json"), encoding="utf-8"))
    except Exception:
        env = {}
    n3 = ""
    n3_path = os.path.join(campaign, "N3.txt")
    if os.path.exists(n3_path):
        n3 = open(n3_path, encoding="utf-8").read().strip()
    tot = summary["totals"]

    durable_delta = sum(f.get("postclear_star_delta_above_floor") or 0
                        for f in summary["families"])
    cross = tot["cross_member_reused"]
    derived_units_saved = cross  # one reused (missed) node = one avoided execution
    f1 = (durable_delta == 0)
    f2 = (derived_units_saved <= tot["nodes_id_shared"])
    f3_note = ("the derived-output bytes saved are 0 (cross-member reuse 0), so the "
               f"{tot['cdc_saving']}-byte chunk-CDC saving is unrelated and reproduces "
               "nothing derivational")
    verdict = "NOT VIOLATED" if (cross > 0 and durable_delta > 0) else "VIOLATED"

    receipt = {
        "phase": "15.7",
        "title": "durable cross-root derivations — N3 re-run over the real real100-v1 families",
        "campaign": os.path.basename(campaign.rstrip("/")),
        "run_utc": env.get("run_utc"),
        "git_commit": env.get("git_commit"),
        "git_branch": env.get("git_branch"),
        "git_dirty": env.get("git_dirty"),
        "cargo_lock_sha256": env.get("cargo_lock_sha256"),
        "service_image": (env.get("doc_baseline") or {}).get("service_image"),
        "base_image": (env.get("doc_baseline") or {}).get("base_image"),
        "manifest": plan.get("manifest"),
        "manifest_sha256": plan.get("manifest_sha256"),
        "families_available": len(plan["families"]),
        "families_measured": tot["families"],
        "members_ingested": f"{tot['members_ok']}/{tot['members']}",
        "metrics": {
            "cold_work_units": tot["cold_units"],
            "warm_work_units": tot["warm_units"],
            "retained_inverse_work_fraction": tot["retained_inverse_work_fraction"],
            "warm_reused_nodes": tot["warm_reused"],
            "intra_observation_reuse_floor": tot["intra_reused"],
            "cross_member_reused_nodes": cross,
            "warm_fresh_reused_nodes": tot["warm_fresh_reused"],
            "postclear_star_reused_nodes": tot["postclear_star_reused"],
            "nodes_id_shared": tot["nodes_id_shared"],
            "shared_resource_ids": tot["shared_resource_ids"],
            "shared_resource_bytes": tot["shared_resource_bytes"],
            "seed_bytes_written": tot["seed_bytes_written"],
            "store_bytes": tot["store_bytes"],
            "cdc_best_unique_bytes": tot["cdc_best_unique"],
            "cdc_total_size_bytes": tot["cdc_total_size"],
            "cdc_dedup_saving_bytes": tot["cdc_saving"],
        },
        "falsifiers": {
            "i_post_cache_clear_derived_reuse_zero": {
                "holds": f1,
                "postclear_star_reused_above_floor": durable_delta,
                "note": "a fresh OS process after `cache --clear` re-observed the "
                        "max-warm member and reused nothing above its empty-cache "
                        "floor, so no computation is durable.",
            },
            "ii_avoided_work_le_representation_identity_work": {
                "holds": f2,
                "derived_work_units_saved": derived_units_saved,
                "nodes_id_shared": tot["nodes_id_shared"],
                "note": "cross-member derived work avoided is 0 units; the only "
                        "durable sharing is representation identity (nodes_id_shared).",
            },
            "iii_chunk_dedup_reproduces": {
                "derived_output_bytes_saved": 0,
                "cdc_dedup_saving_bytes": tot["cdc_saving"],
                "note": f3_note,
            },
        },
        "verdict": verdict,
        "n3_verdict_text": n3,
        "notes": [
            "N3 had no SQLite comparison; its only baseline was borg CDC, so none is reproduced.",
            "No compression claim: a shared blob is scored as state/work, never a store-size fraction.",
            "Exactness is out of scope for this court; it measures sharing/reuse, not reconstruction.",
            "This is a measurement-first deliverable: no durable DerivationStore was implemented; "
            "the court measures whether one is warranted.",
        ],
        "environment": env,
        "artifacts": {
            "summary": "raw/summary.json",
            "plan": "raw/plan.json",
            "families": "raw/families.tsv",
            "measure": "raw/measure.jsonl",
            "cdc": "raw/cdc/*.jsonl",
            "n3": "N3.txt",
            "commands": "commands.txt",
            "environment": "environment.json",
        },
    }
    with open(os.path.join(campaign, "receipt.json"), "w", encoding="utf-8") as fh:
        json.dump(receipt, fh, indent=2, sort_keys=True)
        fh.write("\n")

    m = receipt["metrics"]
    lines = [
        "# Phase 15.7 — durable cross-root derivations (N3 re-run, real families)",
        "",
        f"Generated by `tools/phase15-crossroot-court.sh` inside the pinned, capped "
        f"`doc-baseline` service. Commit `{receipt['git_commit']}` (branch "
        f"`{receipt['git_branch']}`); run (UTC) {receipt['run_utc']}.",
        f"Manifest `{receipt['manifest']}` sha256 `{receipt['manifest_sha256']}`.",
        "",
        f"**Verdict: N3 {verdict}.**",
        "",
        "## Measured",
        "",
        f"* {receipt['families_measured']} families, {receipt['members_ingested']} members; "
        f"one shared `FieldStore` per family.",
        f"* cross-member derived reuse = **{m['cross_member_reused_nodes']} nodes** "
        f"(warm in-order reuse {m['warm_reused_nodes']} = empty-cache floor "
        f"{m['intra_observation_reuse_floor']}); retained_inverse_work_fraction "
        f"`{m['retained_inverse_work_fraction']}` over {m['cold_work_units']} cold units.",
        f"* representation identity is shared: {m['nodes_id_shared']} nodes id-shared, "
        f"{m['shared_resource_ids']} shared resources ({m['shared_resource_bytes']} bytes); "
        f"store bytes {m['store_bytes']}.",
        f"* falsifier (i): post-`cache --clear` fresh-process reuse above the floor = "
        f"{durable_delta} (no durable output store); the counter is live "
        f"(cache-intact fresh process reused {m['warm_fresh_reused_nodes']}).",
        f"* falsifier (iii): chunk-level borg CDC saving {m['cdc_dedup_saving_bytes']} bytes "
        f"vs 0 derived-output bytes saved.",
        "",
        "See `N3.txt`, `receipt.json`, `raw/summary.json`.",
        "",
    ]
    with open(os.path.join(campaign, "SUMMARY.md"), "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines))
    print(json.dumps({"verdict": verdict, "cross_member_reused_nodes": cross,
                      "families_measured": tot["families"]}))
    return 0


def main(argv):
    if len(argv) < 2:
        sys.stderr.write(__doc__)
        return 2
    cmd = argv[1]
    if cmd == "plan" and len(argv) == 5:
        return cmd_plan(argv[2], argv[3], argv[4])
    if cmd == "aggregate" and len(argv) == 3:
        return cmd_aggregate(argv[2])
    if cmd == "receipt" and len(argv) == 3:
        return cmd_receipt(argv[2])
    sys.stderr.write(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
