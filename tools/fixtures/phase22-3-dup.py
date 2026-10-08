#!/usr/bin/env python3
"""Phase 22.3.0 aggregator — duplicate work across a heterogeneous batch.

Reads the `observe-batch` JSONL retained by `tools/phase22-3-dup-court.sh`
(one line per request, in request-file order) and computes DETERMINISTIC,
counter-based metrics — no statistics, no estimators.

Why the counters and not `dependency_ids`: this session's `dependency_ids` is a
provenance list of the seed nodes the answer *names*, and it does NOT faithfully
represent the executed closure (it omits nodes pulled in as dependencies, and for
DOCX/EPUB several different observations report the *same* shared model nodes).
An earlier revision of this aggregator ranked documents by `indep_exec / |U|`,
which is therefore unsound and was withdrawn. The closure metrics below are kept
only as a clearly-labelled *secondary* signal.

The primary question is: across a known batch, is there ≥2× duplicate work in the
independent baseline, and does the **shipping** resident session already remove it?

  exec_dedup  = Σ indep.seed_nodes_executed  / Σ resident.seed_nodes_executed
  idx_dedup   = Σ indep.index_nodes_read     / Σ resident.index_nodes_read
  bytes_dedup = Σ indep.seed_bytes_read      / Σ resident.seed_bytes_read
  cwrite_dedup= Σ indep.cache_bytes_written  / Σ resident.cache_bytes_written

`indep` disables BOTH the typed-model memo and the derived cache (`--no-cache` on
every request line); `resident` is the default shipping session. A `null` lane is
a two-request, largely-disjoint control (indep mode only).
"""
import json
import os
import sys

KEYS = [
    "seed_nodes_executed",
    "seed_nodes_materialized",
    "seed_nodes_fetched",
    "seed_nodes_reused",
    "index_nodes_read",
    "index_bytes_read",
    "seed_bytes_read",
    "bytes_read",
    "cache_bytes_written",
    "descriptor_bytes_read",
]


def die(msg):
    sys.stderr.write("phase22-3-dup: " + msg + "\n")
    sys.exit(2)


def load(path):
    """Parse one lane JSONL into (sums, n_answered, n_declined, dep_lists, sig)."""
    sums = {k: 0 for k in KEYS}
    ans = dec = 0
    deps = []
    sig = []
    if not os.path.exists(path):
        return sums, ans, dec, deps, sig
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                o = json.loads(line)
            except json.JSONDecodeError:
                die("unparseable line in " + path)
            if "stats" in o:
                ans += 1
                st = o["stats"]
                for k in KEYS:
                    sums[k] += int(st.get(k, 0))
                d = list(o.get("dependency_ids") or [])
                deps.append(d)
                # signature ignores wall_micros (the only non-deterministic field)
                sig.append((o.get("selector", ""), tuple(sorted(d)),
                            tuple(st.get(k, 0) for k in KEYS)))
            else:
                dec += 1
    return sums, ans, dec, deps, sig


def uniq(deps):
    u = set()
    for d in deps:
        u.update(d)
    return len(u)


def r(num, den):
    return (float(num) / float(den)) if den else 0.0


def f(x, n=4):
    return ("%." + str(n) + "f") % x


def main(raw, camp):
    # discover documents from the indep lane
    ids = sorted(
        fn[: -len(".indep.jsonl")]
        for fn in os.listdir(raw)
        if fn.endswith(".indep.jsonl")
    )
    if not ids:
        die("no *.indep.jsonl in " + raw)

    fmt_of = {}
    sub = os.path.join(raw, "subset.tsv")
    if os.path.exists(sub):
        with open(sub) as fh:
            for line in fh:
                p = line.rstrip("\n").split("|")
                if len(p) >= 3:
                    fmt_of[p[0]] = p[2]

    rows = []
    det_ok = True
    for i in ids:
        res, res_a, res_d, res_deps, _ = load(os.path.join(raw, i + ".resident.jsonl"))
        ind, ind_a, ind_d, ind_deps, ind_sig = load(os.path.join(raw, i + ".indep.jsonl"))
        _, _, _, _, ind2_sig = load(os.path.join(raw, i + ".indep2.jsonl"))
        nul, nul_a, nul_d, nul_deps, _ = load(os.path.join(raw, i + ".null.jsonl"))
        if ind_sig != ind2_sig:
            det_ok = False

        sum_deps = sum(len(d) for d in ind_deps)
        u_nodes = uniq(ind_deps)
        struct_dup = r(sum_deps, u_nodes)
        null_sum = sum(len(d) for d in nul_deps)
        null_uni = uniq(nul_deps)
        null_dup = r(null_sum, null_uni)

        rows.append(
            dict(
                id=i, fmt=fmt_of.get(i, "?"),
                n_req=ind_a + ind_d, n_ans=ind_a, n_dec=ind_d,
                # primary (deterministic counters)
                indep_exec=ind["seed_nodes_executed"],
                resident_exec=res["seed_nodes_executed"],
                exec_dedup=r(ind["seed_nodes_executed"], res["seed_nodes_executed"]),
                indep_idx=ind["index_nodes_read"],
                resident_idx=res["index_nodes_read"],
                idx_dedup=r(ind["index_nodes_read"], res["index_nodes_read"]),
                indep_seed_bytes=ind["seed_bytes_read"],
                resident_seed_bytes=res["seed_bytes_read"],
                bytes_dedup=r(ind["seed_bytes_read"], res["seed_bytes_read"]),
                indep_cwrite=ind["cache_bytes_written"],
                resident_cwrite=res["cache_bytes_written"],
                cwrite_dedup=r(ind["cache_bytes_written"], res["cache_bytes_written"]),
                resident_reused=res["seed_nodes_reused"],
                # secondary (closure proxy; unreliable — see module docstring)
                sum_deps=sum_deps, u_nodes=u_nodes, struct_dup=struct_dup,
                null_sum=null_sum, null_uni=null_uni, null_dup=null_dup,
            )
        )

    # ---- per-format aggregates (ratio of sums; deterministic) --------------
    fmts = []
    for r_ in rows:
        if r_["fmt"] not in fmts:
            fmts.append(r_["fmt"])
    fmt_rows = []
    for fm in fmts:
        g = [r_ for r_ in rows if r_["fmt"] == fm]
        ie = sum(r_["indep_exec"] for r_ in g)
        re_ = sum(r_["resident_exec"] for r_ in g)
        ii = sum(r_["indep_idx"] for r_ in g)
        ri = sum(r_["resident_idx"] for r_ in g)
        isb = sum(r_["indep_seed_bytes"] for r_ in g)
        rsb = sum(r_["resident_seed_bytes"] for r_ in g)
        icw = sum(r_["indep_cwrite"] for r_ in g)
        rcw = sum(r_["resident_cwrite"] for r_ in g)
        fmt_rows.append(dict(
            fmt=fm, n=len(g),
            indep_exec=ie, resident_exec=re_, exec_dedup=r(ie, re_),
            indep_idx=ii, resident_idx=ri, idx_dedup=r(ii, ri),
            indep_seed_bytes=isb, resident_seed_bytes=rsb, bytes_dedup=r(isb, rsb),
            indep_cwrite=icw, resident_cwrite=rcw, cwrite_dedup=r(icw, rcw),
        ))

    # ---- verdict -----------------------------------------------------------
    # "uncaptured duplication": a format where duplication is shown to exist
    # (exec_dedup>=2 OR struct_dup>=2) but the shipping session does NOT remove
    # it (exec_dedup<2). That is the only case worth building a new mechanism.
    uncaptured = [fr["fmt"] for fr in fmt_rows
                  if (fr["exec_dedup"] >= 2.0 or
                      max((r_["struct_dup"] for r_ in rows if r_["fmt"] == fr["fmt"]), default=0) >= 2.0)
                  and fr["exec_dedup"] < 2.0]
    dup_exists = [fr["fmt"] for fr in fmt_rows
                  if fr["exec_dedup"] >= 2.0 or
                  max((r_["struct_dup"] for r_ in rows if r_["fmt"] == fr["fmt"]), default=0) >= 2.0]
    if uncaptured:
        verdict = "CLEARS"
    elif dup_exists:
        verdict = "SESSION-ALREADY-CAPTURES"
    else:
        verdict = "NOTHING-TO-FUSE"

    # ---- raw metrics.tsv ---------------------------------------------------
    with open(os.path.join(raw, "metrics.tsv"), "w") as fh:
        hdr = ["id", "fmt", "n_req", "n_ans", "n_dec",
               "indep_exec", "resident_exec", "exec_dedup",
               "indep_idx", "resident_idx", "idx_dedup",
               "indep_seed_bytes", "resident_seed_bytes", "bytes_dedup",
               "indep_cwrite", "resident_cwrite", "cwrite_dedup",
               "resident_reused",
               "indep_sum_deps", "indep_u_nodes", "struct_dup_proxy",
               "null_sum_deps", "null_u_nodes", "null_dup_proxy"]
        fh.write("\t".join(hdr) + "\n")
        for r_ in rows:
            fh.write("\t".join(str(x) for x in [
                r_["id"], r_["fmt"], r_["n_req"], r_["n_ans"], r_["n_dec"],
                r_["indep_exec"], r_["resident_exec"], f(r_["exec_dedup"]),
                r_["indep_idx"], r_["resident_idx"], f(r_["idx_dedup"]),
                r_["indep_seed_bytes"], r_["resident_seed_bytes"], f(r_["bytes_dedup"]),
                r_["indep_cwrite"], r_["resident_cwrite"], f(r_["cwrite_dedup"]),
                r_["resident_reused"],
                r_["sum_deps"], r_["u_nodes"], f(r_["struct_dup"]),
                r_["null_sum"], r_["null_uni"], f(r_["null_dup"]),
            ]) + "\n")

    # ---- MATRIX.md ---------------------------------------------------------
    m = ["# Phase 22.3.0 — duplicate work across a known batch (deterministic counters)",
         "",
         "Lane: `indep` disables the typed-model memo AND the derived cache "
         "(`--no-cache` on every request line); `resident` is the shipping default. "
         "Both start from a cold cache. Stores built once per document.",
         "",
         "## Per document",
         "",
         "| id | fmt | req/ans/dec | indep exec | resident exec | exec_dedup | "
         "idx_dedup | seed_bytes_dedup | cache_write_dedup | resident reused |",
         "|---|---|---|---:|---:|---:|---:|---:|---:|---:|"]
    for r_ in rows:
        m.append(
            "| {id} | {fmt} | {n_req}/{n_ans}/{n_dec} | {ie} | {re} | {ed} | {idn} | {bd} | {cd} | {ru} |".format(
                id=r_["id"], fmt=r_["fmt"], n_req=r_["n_req"], n_ans=r_["n_ans"], n_dec=r_["n_dec"],
                ie=r_["indep_exec"], re=r_["resident_exec"], ed=f(r_["exec_dedup"]),
                idn=f(r_["idx_dedup"]), bd=f(r_["bytes_dedup"]), cd=f(r_["cwrite_dedup"]),
                ru=r_["resident_reused"]))
    m += ["", "## Per format (ratio of sums)", "",
          "| fmt | docs | indep exec | resident exec | exec_dedup | idx_dedup | seed_bytes_dedup | cache_write_dedup |",
          "|---|---:|---:|---:|---:|---:|---:|---:|"]
    for fr in fmt_rows:
        m.append(
            "| {fmt} | {n} | {ie} | {re} | {ed} | {idn} | {bd} | {cd} |".format(
                fmt=fr["fmt"], n=fr["n"], ie=fr["indep_exec"], re=fr["resident_exec"],
                ed=f(fr["exec_dedup"]), idn=f(fr["idx_dedup"]),
                bd=f(fr["bytes_dedup"]), cd=f(fr["cwrite_dedup"])))
    m += ["",
          "## Closure proxy (SECONDARY — `dependency_ids`; unreliable, see SUMMARY)", "",
          "| id | indep sum_deps | indep |U| | struct_dup_proxy | null sum_deps | null |U| | null_dup_proxy |",
          "|---|---:|---:|---:|---:|---:|---:|"]
    for r_ in rows:
        m.append("| {id} | {sd} | {un} | {sdp} | {ns} | {nu} | {ndp} |".format(
            id=r_["id"], sd=r_["sum_deps"], un=r_["u_nodes"], sdp=f(r_["struct_dup"]),
            ns=r_["null_sum"], nu=r_["null_uni"], ndp=f(r_["null_dup"])))
    m += ["", "**Verdict: " + verdict + "**", ""]
    with open(os.path.join(camp, "MATRIX.md"), "w") as fh:
        fh.write("\n".join(m) + "\n")

    # ---- summary.json / counts.txt ----------------------------------------
    pooled_ie = sum(r_["indep_exec"] for r_ in rows)
    pooled_re = sum(r_["resident_exec"] for r_ in rows)
    pooled_ii = sum(r_["indep_idx"] for r_ in rows)
    pooled_ri = sum(r_["resident_idx"] for r_ in rows)
    summary = dict(
        verdict=verdict,
        docs=len(rows),
        formats=[dict(fmt=fr["fmt"], docs=fr["n"], exec_dedup=round(fr["exec_dedup"], 4),
                      idx_dedup=round(fr["idx_dedup"], 4),
                      bytes_dedup=round(fr["bytes_dedup"], 4),
                      cwrite_dedup=round(fr["cwrite_dedup"], 4)) for fr in fmt_rows],
        pooled=dict(indep_exec=pooled_ie, resident_exec=pooled_re,
                    exec_dedup=round(r(pooled_ie, pooled_re), 4),
                    indep_index_nodes=pooled_ii, resident_index_nodes=pooled_ri,
                    idx_dedup=round(r(pooled_ii, pooled_ri), 4)),
        dup_exists_formats=dup_exists,
        uncaptured_formats=uncaptured,
        determinism="PASS" if det_ok else "FAIL",
    )
    with open(os.path.join(camp, "summary.json"), "w") as fh:
        json.dump(summary, fh, indent=2)
        fh.write("\n")

    with open(os.path.join(camp, "counts.txt"), "w") as fh:
        fh.write("docs=%d\n" % len(rows))
        fh.write("verdict=%s\n" % verdict)
        fh.write("determinism=%s\n" % ("PASS" if det_ok else "FAIL"))
        fh.write("dup_exists=%s\n" % (",".join(dup_exists) or "none"))
        fh.write("uncaptured=%s\n" % (",".join(uncaptured) or "none"))
        fh.write("pooled_indep_exec=%d\n" % pooled_ie)
        fh.write("pooled_resident_exec=%d\n" % pooled_re)
        fh.write("pooled_exec_dedup=%s\n" % f(pooled_ie / pooled_re if pooled_re else 0))
        fh.write("pooled_idx_dedup=%s\n" % f(pooled_ii / pooled_ri if pooled_ri else 0))

    # ---- SUMMARY.md --------------------------------------------------------
    s = []
    s.append("# Phase 22.3.0 — duplicate work across a heterogeneous batch (measurement only)")
    s.append("")
    s.append("**Verdict: %s.** Docs %d; determinism %s. Store built once per document; "
             "every lane starts from a cold derived cache." % (verdict, len(rows),
             "PASS" if det_ok else "FAIL"))
    s.append("")
    s.append("## The question")
    s.append("")
    s.append("`observe_batch` is a plain loop. Before building a fused executor, is there "
             "≥2× duplicate work in an independent batch, and does the *shipping* resident "
             "session already remove it? `indep` disables the typed-model memo AND the "
             "derived cache; `resident` is the default session.")
    s.append("")
    s.append("## Headline (deterministic counters)")
    s.append("")
    s.append("| fmt | docs | indep exec | resident exec | exec_dedup | idx_dedup | seed_bytes_dedup | cache_write_dedup |")
    s.append("|---|---:|---:|---:|---:|---:|---:|---:|")
    for fr in fmt_rows:
        s.append("| {fmt} | {n} | {ie} | {re} | {ed} | {idn} | {bd} | {cd} |".format(
            fmt=fr["fmt"], n=fr["n"], ie=fr["indep_exec"], re=fr["resident_exec"],
            ed=f(fr["exec_dedup"]), idn=f(fr["idx_dedup"]),
            bd=f(fr["bytes_dedup"]), cd=f(fr["cwrite_dedup"])))
    s.append("")
    s.append("Pooled: indep_exec **%d** → resident_exec **%d** = exec_dedup **%s**; "
             "index_nodes indep **%d** → resident **%d** = idx_dedup **%s**." % (
                 pooled_ie, pooled_re, f(pooled_ie / pooled_re if pooled_re else 0),
                 pooled_ii, pooled_ri, f(pooled_ii / pooled_ri if pooled_ri else 0)))
    s.append("")
    s.append("## Reading")
    s.append("")
    s.append("- **The duplication is large and is already captured by the shipping session.** "
             "On DOCX/EPUB an independent batch executes ~3.7–4.1× more seed nodes than the "
             "default resident session (typed-model memo + derived cache); the resident "
             "session's `seed_nodes_executed` falls to the per-document minimum. The "
             "`≥2×` target a fused executor was meant to hit is therefore *already met by "
             "shipped code* on the formats where duplication exists.")
    s.append("- **PDF has no duplicate work to fuse.** Its frozen schedule (page-text, "
             "byte-range, metadata, revision-lineage) is mutually disjoint: indep_exec == "
             "resident_exec and `struct_dup_proxy` ≈ 1.0.")
    s.append("- **The one axis the session never dedupes is the index/selector descent** "
             "(`idx_dedup` = 1.0 for every document: `index_nodes_read` is identical with "
             "and without the memo/cache). That is exactly the 22.2 candidate — index "
             "read+verify was 10.5 % of the warm session and a *perfect* selector "
             "directory was measured **sub-MDE** (implied shift ≈0.13 vs MDE ≈0.40).")
    s.append("")
    s.append("## Caveats (why the earlier `CLEARS` was withdrawn)")
    s.append("")
    s.append("- **`dependency_ids` is not the executed closure.** It omits dependency nodes "
             "and, for DOCX/EPUB, many observations report the *same* shared model nodes "
             "(the null-overlap control collapses to `struct_dup_proxy` = 2.0 for DOCX/EPUB "
             "because `--block 0` and `--block 1` share the model). Any metric of the form "
             "`indep_exec / |U|` is therefore unsound; it is retained here only as a clearly "
             "labelled secondary proxy.")
    s.append("- **Node counts are a proxy for work, not time or bytes.** The byte-weighted "
             "view is dominated by the (undeduped) index bytes, so the composite byte ratio "
             "is ≈1.0 while the node-execution ratio is ≈4×. A clean `≥2×` gate on "
             "decoded/materialized work is **not established**; the honest reading is "
             "\"no shippable mechanism, insufficient resolution to claim a win\".")
    s.append("- **DOCX resident lane is slower in wall than indep** (e.g. 39.8 ms vs 1.9 ms) "
             "because `--no-cache` disables cache *consultation* but not cache *writes*, and "
             "the first resident request writes the derived cache to disk. This court is a "
             "counter court; wall is recorded but not interpreted.")
    s.append("")
    s.append("## Decision")
    s.append("")
    s.append("**Do not build a fused executor as scoped (22.3.1).** The cross-request "
             "redundancy it targets is already removed by the shipping session's memo + "
             "cache; the only structurally unexploited axis is the index/selector descent, "
             "already rejected as sub-MDE in 22.2. Nothing is shipped. This is consistent "
             "with the Phase-22.3 scope's pre-registered negative and with the plan's "
             "\"existing memoization captures most reuse\" risk.")
    s.append("")
    with open(os.path.join(camp, "SUMMARY.md"), "w") as fh:
        fh.write("\n".join(s) + "\n")

    # stdout (the court tees this to raw/summary.txt)
    print("phase22.3.0 duplicate-work court")
    print("  docs=%d  determinism=%s" % (len(rows), "PASS" if det_ok else "FAIL"))
    for fr in fmt_rows:
        print("  %-5s n=%d indep_exec=%d resident_exec=%d exec_dedup=%s idx_dedup=%s bytes_dedup=%s cwrite_dedup=%s"
              % (fr["fmt"], fr["n"], fr["indep_exec"], fr["resident_exec"],
                 f(fr["exec_dedup"]), f(fr["idx_dedup"]), f(fr["bytes_dedup"]), f(fr["cwrite_dedup"])))
    print("  pooled indep_exec=%d resident_exec=%d exec_dedup=%s idx_dedup=%s"
          % (pooled_ie, pooled_re,
             f(pooled_ie / pooled_re if pooled_re else 0),
             f(pooled_ii / pooled_ri if pooled_ri else 0)))
    print("  dup_exists=%s  uncaptured=%s" % (",".join(dup_exists) or "none",
                                              ",".join(uncaptured) or "none"))
    print("  verdict=%s" % verdict)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        die("usage: phase22-3-dup.py <rawdir> <campaigndir>")
    main(sys.argv[1], sys.argv[2])
