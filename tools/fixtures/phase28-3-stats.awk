# Phase 28.3 — paired ablation statistics (POSIX awk / mawk compatible).
#
# Reads the raw per-run samples TSV on stdin (header + one row per run) and writes
# the ADR-0054 estimator tables plus the raw bootstrap draws:
#
#   per_obs_stages.tsv   per (doc,obs,variant) medians of every stage counter
#   paired.tsv           per (doc,obs) median / geometric-mean paired ratio
#   doc_ratios.tsv       per-document pooled ratio, wins/ties/losses (±10% band)
#   skipped.tsv           (doc,obs) pairs whose runs declined (rc != 0)
#   pooled_ratios.txt     every paired ratio, one per line (for pooled stats)
#   wall_current.txt / wall_selective.txt   every wall sample (for p50/p95/p99)
#   boot_median.txt / boot_geomean.txt      fixed-seed cluster-bootstrap draws
#
# The estimator is the per-rep paired ratio selective/current; < 1 means the
# selective variant is faster. The bootstrap resamples *documents* (clusters) with
# replacement (ADR-0054), so a document's correlated reps are not treated as
# independent. The RNG is a self-contained MINSTD LCG (seed = 881726454), exact in
# IEEE doubles, so the CI is reproducible across mawk/gawk.
#
# Invoke:  awk -v outdir=DIR -v B=20000 -f phase28-3-stats.awk raw/samples.tsv

BEGIN {
    FS = "\t"; OFS = "\t"
    if (B == "") B = 20000
    seed = 881726454
    TIE = 0.10
    NM = split("wall_us user_us sys_us maxrss_kb minflt majflt allocs alloc_bytes observe_us materialize_us source_serve_us select_us clone_bytes referenced_bytes objects_total objects_used serve_calls", MET, " ")
}

function rnd() { seed = (seed * 48271) % 2147483647; return seed / 2147483647 }

function toarr(str, a,    n, i, t, cnt) {
    cnt = split(str, t, " ")
    n = 0
    for (i = 1; i <= cnt; i++) if (t[i] != "") a[++n] = t[i] + 0
    return n
}
function qsort(a, lo, hi,    i, j, p, t) {
    if (lo >= hi) return
    p = a[int((lo + hi) / 2)]; i = lo; j = hi
    while (i <= j) {
        while (a[i] < p) i++
        while (a[j] > p) j--
        if (i <= j) { t = a[i]; a[i] = a[j]; a[j] = t; i++; j-- }
    }
    qsort(a, lo, j); qsort(a, i, hi)
}
function median(str,    a, n) {
    n = toarr(str, a); if (n == 0) return 0
    qsort(a, 1, n)
    return (n % 2) ? a[(n + 1) / 2] : (a[n / 2] + a[n / 2 + 1]) / 2
}
function gm(str,    a, n, i, s) {
    n = toarr(str, a); if (n == 0) return 0
    s = 0; for (i = 1; i <= n; i++) s += log(a[i])
    return exp(s / n)
}
function pctl(str, p,    a, n, k, f, cc) {
    n = toarr(str, a); if (n == 0) return 0
    qsort(a, 1, n)
    if (n == 1) return a[1]
    k = (n - 1) * p; f = int(k); cc = f + 1
    if (f == cc) return a[f + 1]
    return a[f + 1] * (cc - k) + a[cc + 1] * (k - f)
}
function cnt(str,    a) { return toarr(str, a) }

NR == 1 { for (i = 1; i <= NF; i++) c[$i] = i; next }
{
    doc = $c["doc"]; fmt = $c["fmt"]; obs = $c["obs"]; var = $c["variant"]
    rc = $c["rc"] + 0
    if (rc != 0) { skipped[doc SUBSEP obs] = 1; next }
    k = doc SUBSEP obs SUBSEP ($c["rep"] + 0)
    rapp[k SUBSEP var] = $c["wall_us"] + 0
    repseen[k] = 1
    sk = doc SUBSEP fmt SUBSEP obs SUBSEP var
    sklist[sk] = 1
    for (m = 1; m <= NM; m++) { met = MET[m]; vals[sk SUBSEP met] = vals[sk SUBSEP met] " " ($c[met] + 0) }
    wl[var] = wl[var] " " ($c["wall_us"] + 0)
}

END {
    # ---- paired ratios (selective / current) -----------------------------
    for (k in repseen) {
        cur = rapp[k SUBSEP "current"]; sel = rapp[k SUBSEP "selective"]
        if (cur > 0 && sel > 0) {
            r = sel / cur
            split(k, P, SUBSEP); d = P[1]; o = P[2]
            obsratio[d SUBSEP o] = obsratio[d SUBSEP o] " " r
            docratio[d] = docratio[d] " " r
            pooled = pooled " " r
        }
    }
    for (d in docratio) doclist[++nd] = d

    # ---- per-obs stage table --------------------------------------------
    f1 = outdir "/per_obs_stages.tsv"
    print "doc\tfmt\tobs\tvariant\tn\twall_us\tobserve_us\tmaterialize_us\tsource_serve_us\tselect_us\tclone_bytes\treferenced_bytes\tobjects_total\tobjects_used\tmaxrss_kb\tallocs\talloc_bytes\tserve_calls" > f1
    for (sk in sklist) {
        split(sk, P, SUBSEP)
        printf "%s\t%s\t%s\t%s\t%d\t%.0f\t%.0f\t%.0f\t%.0f\t%.0f\t%.0f\t%.0f\t%.0f\t%.0f\t%.0f\t%.0f\t%.0f\t%.0f\n",
            P[1], P[2], P[3], P[4], cnt(vals[sk SUBSEP "wall_us"]),
            median(vals[sk SUBSEP "wall_us"]), median(vals[sk SUBSEP "observe_us"]),
            median(vals[sk SUBSEP "materialize_us"]), median(vals[sk SUBSEP "source_serve_us"]),
            median(vals[sk SUBSEP "select_us"]), median(vals[sk SUBSEP "clone_bytes"]),
            median(vals[sk SUBSEP "referenced_bytes"]), median(vals[sk SUBSEP "objects_total"]),
            median(vals[sk SUBSEP "objects_used"]), median(vals[sk SUBSEP "maxrss_kb"]),
            median(vals[sk SUBSEP "allocs"]), median(vals[sk SUBSEP "alloc_bytes"]),
            median(vals[sk SUBSEP "serve_calls"]) > f1
    }

    # ---- per-obs paired ratio -------------------------------------------
    f2 = outdir "/paired.tsv"
    print "doc\tobs\tn\tmedian_ratio\tgm_ratio" > f2
    for (k in obsratio) {
        split(k, P, SUBSEP)
        printf "%s\t%s\t%d\t%.4f\t%.4f\n", P[1], P[2], cnt(obsratio[k]), median(obsratio[k]), gm(obsratio[k]) > f2
    }

    # ---- per-document pooled ratio + outcome ----------------------------
    f3 = outdir "/doc_ratios.tsv"
    print "doc\tn\tmedian_ratio\tgm_ratio\twins\tties\tlosses" > f3
    for (d in docratio) {
        med = median(docratio[d]); gmv = gm(docratio[d]); w = 0; t = 0; l = 0
        n = toarr(docratio[d], a)
        for (i = 1; i <= n; i++) {
            if (a[i] < 1 - TIE) w++
            else if (a[i] > 1 + TIE) l++
            else t++
        }
        printf "%s\t%d\t%.4f\t%.4f\t%d\t%d\t%d\n", d, n, med, gmv, w, t, l > f3
    }

    # ---- raw streams ----------------------------------------------------
    n = toarr(pooled, pa)
    for (i = 1; i <= n; i++) print pa[i] > (outdir "/pooled_ratios.txt")
    n = toarr(wl["current"], wa); for (i = 1; i <= n; i++) print wa[i] > (outdir "/wall_current.txt")
    n = toarr(wl["selective"], wb); for (i = 1; i <= n; i++) print wb[i] > (outdir "/wall_selective.txt")

    for (k in skipped) { split(k, P, SUBSEP); print P[1] "\t" P[2] > (outdir "/skipped.tsv") }

    # ---- fixed-seed cluster bootstrap draws -----------------------------
    for (b = 1; b <= B; b++) {
        s = ""
        for (j = 1; j <= nd; j++) { idx = 1 + int(rnd() * nd); s = s docratio[doclist[idx]] }
        print median(s) > (outdir "/boot_median.txt")
        print gm(s) > (outdir "/boot_geomean.txt")
    }

    # ---- pooled point estimates to stdout -------------------------------
    printf "POOLED_N %d\n", cnt(pooled)
    printf "POOLED_MEDIAN %.4f\n", median(pooled)
    printf "POOLED_GEOMEAN %.4f\n", gm(pooled)
    printf "DOCS %d\n", nd
    printf "DRAWS %d\n", B
}
