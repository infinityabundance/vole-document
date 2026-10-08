#!/usr/bin/env python3
# phase20.2 large-source memory aggregator.
#
# Reads the court's per-source rows (`raw/build.tsv`) and reports the
# RSS-vs-source curve for the direct build, a least-squares linear fit of peak
# RSS against source size (peak_KiB = C + k * source_bytes), the implied peak for
# a 1 GiB and a 2 GiB source against the 6 GiB lane cap, and the exactness /
# descriptor-SHA columns. It also diffs the descriptor SHA-256 against a
# pre-change baseline table (`raw/wire_before.tsv`, id<TAB>sha) when present.
#
# Usage: phase20-memory.py RAW

import os
import sys

GIB_KIB = 1024 * 1024


def read_tsv(path):
    if not path or not os.path.exists(path):
        return []
    with open(path) as fh:
        hdr = fh.readline().rstrip("\n").split("\t")
        rows = []
        for line in fh:
            if not line.strip():
                continue
            vals = line.rstrip("\n").split("\t")
            if len(vals) != len(hdr):
                continue
            rows.append(dict(zip(hdr, vals)))
        return rows


def ti(s, default=0):
    try:
        return int(s)
    except (TypeError, ValueError):
        return default


def fit(xs, ys):
    n = len(xs)
    if n == 0:
        return None
    mx = sum(xs) / n
    my = sum(ys) / n
    den = sum((x - mx) ** 2 for x in xs)
    if den == 0:
        return (my, 0.0)
    b = sum((xs[i] - mx) * (ys[i] - my) for i in range(n)) / den
    a = my - b * mx
    return (a, b)


def main():
    raw = sys.argv[1] if len(sys.argv) > 1 else "raw"
    rows = [r for r in read_tsv(os.path.join(raw, "build.tsv")) if ti(r["src_bytes"]) > 0]
    rows.sort(key=lambda r: ti(r["src_bytes"]))

    print("== Phase 20.2 large-source memory curve ==")
    print(f"  sources: {len(rows)}")
    print()
    hdr = f"{'id':<26}{'src B':>13}{'MiB':>9}{'rc':>4}{'wall ms':>10}{'peak MiB':>10}{'RSS/src':>9}{'exact':>7}  descriptor_sha256"
    print(hdr)
    print("-" * len(hdr))

    ok = []
    for r in rows:
        blen = ti(r["src_bytes"])
        rss = ti(r["rss_kb"])
        rc = ti(r["rc"])
        ratio = rss * 1024.0 / blen if blen else 0.0
        exact = r.get("exact_ok", "0")
        print(
            f"{r['id']:<26}{blen:>13,}{blen / 1048576.0:>9.1f}{rc:>4}"
            f"{ti(r['wall_ms']):>10,}{rss / 1024.0:>10.1f}{ratio:>9.3f}{exact:>7}"
            f"  {r.get('descriptor_sha', '')[:16]}"
        )
        if rc == 0 and blen >= 10 * 1048576:
            ok.append((blen, rss))

    print()
    if ok:
        xs = [b for b, _ in ok]
        ys = [s for _, s in ok]
        a, b = fit(xs, ys)
        print("Least-squares fit over sources >= 10 MiB (peak_KiB = C + k * source_bytes):")
        print(f"  k = {b:.4f} KiB/B  (asymptotic peak/source ratio = {b * 1024:.3f}x)")
        print(f"  C = {a / 1024.0:.1f} MiB fixed overhead")
        for gib in (1, 2):
            peak_kib = a + b * (gib * 1024 ** 3)
            print(
                f"  predicted peak for a {gib} GiB source: {peak_kib / GIB_KIB:.2f} GiB "
                f"({peak_kib / 1024.0:.0f} MiB) vs 6 GiB cap -> "
                f"{'FITS' if peak_kib < 6 * GIB_KIB else 'DOES NOT FIT'}"
            )

    # Exactness.
    total = len(rows)
    exact = sum(1 for r in rows if r.get("exact_ok") == "1")
    print()
    print(f"Exactness (length + SHA-256 + cmp): {exact}/{total}")

    # Wire identity vs a pre-change baseline.
    before = read_tsv(os.path.join(raw, "wire_before.tsv"))
    if before:
        before_map = {r["id"]: r.get("descriptor_sha256", "") for r in before}
        same = 0
        diff = 0
        missing = 0
        for r in rows:
            b = before_map.get(r["id"])
            if not b:
                missing += 1
            elif b == r.get("descriptor_sha", ""):
                same += 1
            else:
                diff += 1
                print(f"WIRE DIFF {r['id']}: before={b[:16]} after={r.get('descriptor_sha','')[:16]}")
        print(f"Wire identity vs baseline: {same} identical, {diff} different, {missing} not in baseline")
    else:
        print("Wire identity: no raw/wire_before.tsv baseline supplied")


if __name__ == "__main__":
    main()
