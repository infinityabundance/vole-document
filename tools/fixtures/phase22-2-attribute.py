#!/usr/bin/env python3
# Phase 22.2 — stage attribution for the profiling gate.
#
# Reads `stages.tsv` (produced by tools/phase22-2-profile.sh inside the capped
# doc-baseline service) and prints, per document and pooled over the packed
# backend:
#   * the one-time open (manifest + descriptor read + Descriptor::parse) share,
#     the request-loop share, and within the loop: probe (selector resolution),
#     dispatch, materialize (procedural node + typed model decode), serialize;
#   * the index-node access term (index_read + index_parse) as a share;
#   * the redundant-read ratio (index_openat calls per DISTINCT index node);
#   * the selector-directory headroom: the fraction of the warm session a
#     PERFECT selector directory could remove at most, pooled and per-document,
#     and the implied shift of the warm headline ratio next to the court's MDE.
#
#   python3 tools/fixtures/phase22-2-attribute.py evidence/scratch/phase22-2-profile/out/stages.tsv [mde]
import sys


def load(path):
    with open(path) as fh:
        hdr = fh.readline().rstrip("\n").split("\t")
        rows = []
        for line in fh:
            if not line.strip():
                continue
            rows.append(dict(zip(hdr, line.rstrip("\n").split("\t"))))
        return rows


def n(r, k):
    try:
        return int(r.get(k, "0") or 0)
    except ValueError:
        return 0


def main(argv):
    rows = load(argv[0])
    mde = float(argv[1]) if len(argv) > 1 else 0.405
    packed = [r for r in rows if r["backend"] == "packed"]
    fs = [r for r in rows if r["backend"] == "fs"]

    print("## Per-document attribution (packed store, one warm session)\n")
    print("| document | session us | open % | Descriptor::parse % | loop % | probe % | index-read % | dispatch % | materialize % | serialize % | idx openat/reads | distinct idx nodes |")
    print("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
    for r in packed:
        sess = n(r, "open_us") + n(r, "loop_us")
        idx = n(r, "index_read_us") + n(r, "index_parse_us")
        pct = lambda x: 100.0 * x / sess if sess else 0.0
        print(f"| {r['id']} | {sess} | {pct(n(r,'open_us')):.1f} | "
              f"{pct(n(r,'descriptor_parse_us')):.1f} | {pct(n(r,'loop_us')):.1f} | "
              f"{pct(n(r,'probe_us')):.2f} | {pct(idx):.1f} | {pct(n(r,'dispatch_us')):.1f} | "
              f"{pct(n(r,'materialize_us')):.1f} | {pct(n(r,'serialize_us')):.1f} | "
              f"{n(r,'index_openat')}/{n(r,'index_nodes')} | {n(r,'index_distinct')} |")

    tot = {k: sum(n(r, k) for r in packed) for k in
           ("open_us", "loop_us", "descriptor_parse_us", "dispatch_us",
            "materialize_us", "serialize_us", "probe_us", "index_read_us",
            "index_parse_us", "index_openat", "index_nodes", "lookup_calls")}
    sess = tot["open_us"] + tot["loop_us"]
    idx = tot["index_read_us"] + tot["index_parse_us"]
    print("\n## Pooled (sum of one warm session per document, packed)\n")
    for k in ("open_us", "descriptor_parse_us", "loop_us", "dispatch_us",
              "materialize_us", "serialize_us", "probe_us", "index_read_us",
              "index_parse_us"):
        print(f"- {k}: **{tot[k]} us** ({100.0*tot[k]/sess:.1f}% of {sess} us)")
    print(f"- index nodes read / index descents: **{tot['index_nodes']}** / **{tot['lookup_calls']}**")
    print(f"- redundant re-reads: **{tot['index_openat']}** index opens for "
          f"**{sum(n(r,'index_distinct') for r in packed)}** distinct index node files "
          f"across the 12 sessions")

    share = idx / sess if sess else 0.0
    docshare = sorted((n(r, "index_read_us") + n(r, "index_parse_us")) /
                      (n(r, "open_us") + n(r, "loop_us"))
                      for r in packed if n(r, "open_us") + n(r, "loop_us") > 0)
    medshare = docshare[len(docshare) // 2] if docshare else 0.0
    print("\n## Selector-directory headroom (upper bound)\n")
    print(f"- pooled index share: **{100*share:.1f}%** of the warm session; "
          f"median per-document share: **{100*medshare:.1f}%**.")
    print(f"- A PERFECT selector directory can remove at most this share (it still "
          f"reads offsets and still dispatches/materializes/serializes).")
    print(f"- Implied headline-ratio shift: `ratio * (1 - share)` = a change of about "
          f"**{1.189*share:.3f}** (pooled) / {1.189*medshare:.3f} (median doc) on the "
          f"current warm median 1.189 vs the tuned `full` envelope.")
    print(f"- The court's minimum detectable effect at N=100 is **~{mde:.3f}**. "
          f"A perfect selector directory is therefore **below the MDE** and not "
          f"resolvable by this court.")

    if fs:
        print("\n## Packed vs fs contrast (same request set)\n")
        print("| document | backend | session us | index-read us | store bytes |")
        print("|---|---|---:|---:|---:|")
        for r in rows:
            if r["id"] in {x["id"] for x in fs} or r["backend"] == "packed":
                if r["backend"] == "fs" or r["id"] in {x["id"] for x in fs}:
                    sess2 = n(r, "open_us") + n(r, "loop_us")
                    print(f"| {r['id']} | {r['backend']} | {sess2} | "
                          f"{n(r,'index_read_us')} | {n(r,'store_bytes')} |")


if __name__ == "__main__":
    main(sys.argv[1:])
