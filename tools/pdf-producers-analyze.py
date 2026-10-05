#!/usr/bin/env python3
"""Analyze the Phase-7.0b generator-family corpus.

Pure function of three committed artifacts:
  * evidence/corpus/phase7-producers/provenance.json  (producer/version/command)
  * evidence/corpus/phase7-producers/deflate-stats.jsonl (per-Flate-stream diagnostic)
  * <campaign>/court-results.jsonl                     (complete-cost court)

Emits a JSON analysis on stdout (the sealed campaign's results.json). Ratios use
the nearest-rank percentile (matching the Phase-7.0 corpus report). No wire,
candidate, or decode semantics are involved.
"""
import json
import math
import sys


def pctl(values, q):
    if not values:
        return None
    xs = sorted(values)
    rank = max(1, min(len(xs), math.ceil(q * len(xs))))
    return round(xs[rank - 1], 6)


def main():
    stats_path, court_path, prov_path = sys.argv[1], sys.argv[2], sys.argv[3]
    stats = {}
    with open(stats_path) as f:
        for line in f:
            line = line.strip()
            if line:
                rec = json.loads(line)
                stats[rec["file"].split("/")[-1]] = rec
    court = []
    with open(court_path) as f:
        for line in f:
            line = line.strip()
            if line:
                court.append(json.loads(line))
    prov = json.load(open(prov_path))
    producer = {p["name"]: p["producer"] for p in prov["files"]}

    per_file = []
    for row in court:
        name = row["file"]
        st = stats.get(name, {"streams": [], "summary": {}})
        streams = st.get("streams", [])
        ratios = [s["raw_ratio"] for s in streams if s.get("replayed")]
        flate = st["summary"].get("flate_streams", 0)
        replayed = st["summary"].get("replayed", 0)
        declined = st["summary"].get("declined", 0)
        total = replayed + declined
        br = row["byte_rans"]
        rr = row["deflate_replay_rans"]
        if rr is None or br is None:
            verdict, delta = "decline", None
        elif rr < br:
            verdict, delta = "win", br - rr
        elif rr > br:
            verdict, delta = "lose", br - rr  # negative
        else:
            verdict, delta = "tie", 0
        per_file.append({
            "file": name,
            "producer": producer.get(name, "?"),
            "source_len": row["source_len"],
            "flate_streams": flate,
            "replayed": replayed,
            "declined": declined,
            "acceptance": round(replayed / total, 6) if total else None,
            "raw_p10": pctl(ratios, 0.10),
            "raw_p50": pctl(ratios, 0.50),
            "raw_p90": pctl(ratios, 0.90),
            "rans_naive_bytes": st["summary"].get("replayed_rans_full_bytes"),
            "rans_dedup_bytes": st["summary"].get("replayed_rans_dedup_bytes"),
            "auto_candidate": row["auto_candidate"],
            "auto_len": row["auto_len"],
            "raw": row["raw"],
            "byte_rans": br,
            "deflate_replay": row["deflate_replay"],
            "deflate_replay_rans": rr,
            "replay_vs_byte_rans": verdict,
            "delta_bytes": delta,
            "verify_ok": row["verify_ok"],
            "roundtrip_ok": row["roundtrip_ok"],
        })

    all_ratios = [s["raw_ratio"] for r in stats.values() for s in r.get("streams", []) if s.get("replayed")]
    wins = sum(1 for r in per_file if r["replay_vs_byte_rans"] == "win")
    loses = sum(1 for r in per_file if r["replay_vs_byte_rans"] == "lose")
    declines = sum(1 for r in per_file if r["replay_vs_byte_rans"] == "decline")
    out = {
        "units": "bytes",
        "method": "exact DEFLATE replay (preflate 0.7.6) correction ratio per FlateDecode stream, plus the complete-cost court (serialized .voldoc size)",
        "files": len(per_file),
        "overall": {
            "flate_streams": sum(r["flate_streams"] for r in per_file),
            "replayed": sum(r["replayed"] for r in per_file),
            "declined": sum(r["declined"] for r in per_file),
            "raw_p10": pctl(all_ratios, 0.10),
            "raw_p50": pctl(all_ratios, 0.50),
            "raw_p90": pctl(all_ratios, 0.90),
        },
        "replay_vs_byte_rans": {"win": wins, "lose": loses, "decline": declines},
        "per_file": per_file,
    }
    json.dump(out, sys.stdout, indent=2)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
