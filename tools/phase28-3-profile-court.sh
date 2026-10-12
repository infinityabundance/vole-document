#!/usr/bin/env sh
# Phase 28.3 — hot-path profiling gate (MEASUREMENT ONLY).
#
# Attributes an observation's cost across stages and runs the paired ablation the
# review asked for: the `serve_source_range` inline-object clone, current vs a
# selection-before-materialization variant. It ships **no** production mechanism:
# the selective path is compiled only under the non-default `phase28-3-ablation`
# feature and selected by `VOLE_SERVE_SELECTIVE` (off unless set); the default
# build is byte-identical to today's.
#
# Design (per ADR-0054): every measured quantity is collected for both variants
# **within each repetition**, interleaved (odd rep current→selective, even rep
# selective→current), with a per-rep cold derived cache so the source-serve path
# is actually exercised (a warm cache never calls `serve_source_range`). Every raw
# sample is retained; reductions (median, geometric mean, fixed-seed cluster
# bootstrap 95% CI, ±10% tie band, MDE) are re-derivable estimator choices.
#
# It is self-contained in the `dev` lane on purpose (`/usr/bin/time`, `strace`,
# `python3` and `jq` are absent there): wall + peak RSS + CPU time come from the
# tiny `phase28-3-runstat` exec helper, allocation count/bytes from the phase-22.2
# LD_PRELOAD shim, and the statistics from `phase28-3-stats.awk`.
#
#   docker compose run --rm --no-TTY dev sh tools/phase28-3-profile-court.sh
#
# Env: REPS (default 8), BOOT (default 20000), DOCS (space-separated ids),
#      OP_TIMEOUT (default 300s), CAMP (receipt dir). No cap is ever raised here.

set -u
cd /work
export LC_ALL=C PYTHONDONTWRITEBYTECODE=1

BIN=${BIN:-target/release/vole-document}
REPS=${REPS:-8}
BOOT=${BOOT:-20000}
OP_TIMEOUT=${OP_TIMEOUT:-300}
MANIFEST=${MANIFEST:-real100-v1/manifest.tsv}
CORPUS=${CORPUS:-real100-v1/documents}
DOCS=${DOCS:-"nist-epub-0009 nist-epub-0003 nist-docx-0005 nist-docx-0014 nist-pdf-0002 nist-pdf-0004"}

[ -x "$BIN" ] || {
    echo "phase28-3: $BIN missing. Build it first:" >&2
    echo "  docker compose run --rm --no-TTY dev cargo build --release --all-features --locked -j 1" >&2
    exit 1
}
command -v cc >/dev/null 2>&1 || { echo "phase28-3: need cc (dev/doc-baseline have it)" >&2; exit 1; }
command -v awk >/dev/null 2>&1 || { echo "phase28-3: need awk" >&2; exit 1; }

SHORT=$(git rev-parse --short HEAD 2>/dev/null || echo nogit)
CAMP=${CAMP:-evidence/campaigns/$(date +%F)-phase28-3-profile-$SHORT}
RAW=$CAMP/raw
OUT=$CAMP/out
rm -rf "$CAMP"
mkdir -p "$RAW" "$OUT" "$CAMP/store"

RUNSTAT=/tmp/phase28-3-runstat
SHIM=/tmp/phase28-3-mallocc.so
cc -O2 -o "$RUNSTAT" tools/fixtures/phase28-3-runstat.c || { echo "runstat build failed" >&2; exit 1; }
cc -O2 -shared -fPIC -o "$SHIM" tools/fixtures/phase22-2-malloc-count.c -ldl || { echo "shim build failed" >&2; exit 1; }

kv() { # "line" key -> last numeric value of key=
    printf '%s' "$1" | grep -o "$2=[0-9]*" | tail -1 | cut -d= -f2
}

obs_for() { # fmt -> lines "FLAGS|REPRESENTATION"
    case "$1" in
        pdf) printf '%s\n' "--page 1|text" "--doc-text|text" "--metadata|metadata" ;;
        *)   printf '%s\n' "--doc-text|text" "--block 0|text" "--heading 0|text" "--metadata|metadata" ;;
    esac
}

resolve() { # id -> agency fmt byte_len sha256 path
    awk -F'\t' -v id="$1" -v corpus="$CORPUS" '
        $1==id { printf "%s\t%s\t%s\t%s\t%s/%s/%s/%s.%s\n", $2,$5,$10,$6,corpus,$2,$5,id,$5; exit }' "$MANIFEST"
}

echo "== phase28.3 profiling gate (REPS=$REPS BOOT=$BOOT docs=$(echo $DOCS | wc -w)) ==" >&2

# ---------------------------------------------------------------------------
# Metadata
# ---------------------------------------------------------------------------
{
    echo "# Phase 28.3 receipt metadata"
    echo "generated_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "git_commit: $(git rev-parse HEAD 2>/dev/null || echo nogit)"
    echo "git_branch: $(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo nogit)"
    echo "git_dirty: $(test -n "$(git --no-optional-locks status --porcelain 2>/dev/null)" && echo yes || echo no)"
    echo "rustc: $(rustc -V 2>/dev/null || echo 'n/a (build lane only)')"
    echo "cargo: $(cargo -V 2>/dev/null || echo 'n/a (build lane only)')"
    echo "arch: $(uname -m)"
    echo "nproc: $(nproc 2>/dev/null || echo '?')"
    echo "cargo_lock_sha256: $(sha256sum Cargo.lock | cut -d' ' -f1)"
    echo "binary: $BIN"
    echo "binary_sha256: $(sha256sum "$BIN" | cut -d' ' -f1)"
    echo "features: --all-features (includes phase28-3-ablation prototype)"
    echo "reps: $REPS"
    echo "bootstrap_resamples: $BOOT"
    echo "env_semantics: VOLE_PROFILE_OPEN=1 (stage profiler) on every run; VOLE_SERVE_SELECTIVE set only for the selective variant"
    echo "notes: perf not available in the pinned lane; in-process counters + LD_PRELOAD alloc shim + getrusage used instead."
} > "$CAMP/METADATA.txt"

# ---------------------------------------------------------------------------
# Collection
# ---------------------------------------------------------------------------
printf 'doc\tfmt\tobs\tvariant\trep\torder\twall_us\tuser_us\tsys_us\tmaxrss_kb\tminflt\tmajflt\tallocs\talloc_bytes\tobserve_us\tmaterialize_us\tsource_serve_us\tselect_us\tclone_bytes\treferenced_bytes\tobjects_total\tobjects_used\tserve_calls\tans_sha\trc\n' > "$RAW/samples.tsv"

measure() { # doc fmt obs_repr reqline variant rep order
    doc=$1; fmt=$2; obsr=$3; reqline=$4; var=$5; rep=$6; order=$7
    store="$CAMP/store/$doc"
    reqf="$RAW/req.txt"
    printf '%s\n' "$reqline" > "$reqf"
    rm -rf "$store/cache"
    out="$RAW/.out"; err="$RAW/.err"
    if [ "$var" = selective ]; then
        env VOLE_SERVE_SELECTIVE=1 VOLE_PROFILE_OPEN=1 RUNSTAT_PRELOAD="$SHIM" \
            timeout "$OP_TIMEOUT" "$RUNSTAT" "$BIN" observe-batch \
            --store "$store" --field "$FIELD" --requests "$reqf" >"$out" 2>"$err"
    else
        env -u VOLE_SERVE_SELECTIVE VOLE_PROFILE_OPEN=1 RUNSTAT_PRELOAD="$SHIM" \
            timeout "$OP_TIMEOUT" "$RUNSTAT" "$BIN" observe-batch \
            --store "$store" --field "$FIELD" --requests "$reqf" >"$out" 2>"$err"
    fi
    rc=$?
    rl=$(grep '^runstat ' "$err" | tail -1)
    sl=$(grep 'vole-profile-stage' "$err" | tail -1)
    ml=$(grep -o '\[malloc-count\].*' "$err" | tail -1)
    wall=$(kv "$rl" wall_us); user=$(kv "$rl" user_us); sys=$(kv "$rl" sys_us)
    rss=$(kv "$rl" maxrss_kb); minflt=$(kv "$rl" minflt); majflt=$(kv "$rl" majflt)
    allocs=$(printf '%s' "$ml" | grep -o 'allocs=[0-9]*' | cut -d= -f2)
    abytes=$(printf '%s' "$ml" | grep -o 'bytes=[0-9]*' | cut -d= -f2)
    obs_us=$(kv "$sl" observe_us); mat_us=$(kv "$sl" materialize_us)
    src_us=$(kv "$sl" source_serve_us); sel_us=$(kv "$sl" select_us)
    clb=$(kv "$sl" clone_bytes); refb=$(kv "$sl" referenced_bytes)
    otot=$(kv "$sl" objects_total); oused=$(kv "$sl" objects_used)
    scalls=$(kv "$sl" serve_calls)
    asha=$(sed 's/"stats":{[^}]*}/"stats":{}/' "$out" | sha256sum | cut -d' ' -f1)
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$doc" "$fmt" "$obsr" "$var" "$rep" "$order" \
        "${wall:-0}" "${user:-0}" "${sys:-0}" "${rss:-0}" "${minflt:-0}" "${majflt:-0}" \
        "${allocs:-0}" "${abytes:-0}" \
        "${obs_us:-0}" "${mat_us:-0}" "${src_us:-0}" "${sel_us:-0}" "${clb:-0}" "${refb:-0}" "${otot:-0}" "${oused:-0}" "${scalls:-0}" \
        "$asha" "$rc" >> "$RAW/samples.tsv"
}

for doc in $DOCS; do
    row=$(resolve "$doc")
    [ -n "$row" ] || { echo "  $doc: not in manifest, skipped" >&2; continue; }
    agency=$(printf '%s' "$row" | cut -f1)
    fmt=$(printf '%s' "$row" | cut -f2)
    blen=$(printf '%s' "$row" | cut -f3)
    path=$(printf '%s' "$row" | cut -f5)
    store="$CAMP/store/$doc"
    rm -rf "$store"
    if ! timeout "$OP_TIMEOUT" "$BIN" field-build "$path" --store "$store" \
            --profile runtime >"$RAW/$doc.build.json" 2>"$RAW/$doc.build.err"; then
        echo "  $doc: field-build FAILED" >&2; continue
    fi
    FIELD=$(grep -o '"field":"[0-9a-f]\{64\}"' "$RAW/$doc.build.json" | head -1 | cut -d'"' -f4)
    slen=$(grep -o '"source_len":[0-9]*' "$RAW/$doc.build.json" | head -1 | cut -d: -f2)
    [ -n "$FIELD" ] || { echo "  $doc: no field id" >&2; continue; }
    echo "== $doc ($fmt, $blen B) field=$FIELD ==" >&2

    while IFS= read -r spec; do
        [ -n "$spec" ] || continue
        flags=$(printf '%s' "$spec" | cut -d'|' -f1)
        repr=$(printf '%s' "$spec" | cut -d'|' -f2)
        reqline="$flags --kind $repr"
        # eligibility probe (cold): a typed decline is recorded, not a failure.
        rm -rf "$store/cache"
        printf '%s\n' "$reqline" > "$RAW/req.txt"
        env -u VOLE_SERVE_SELECTIVE VOLE_PROFILE_OPEN=1 \
            timeout "$OP_TIMEOUT" "$BIN" observe-batch --store "$store" --field "$FIELD" \
            --requests "$RAW/req.txt" >/dev/null 2>"$RAW/.probe.err"
        prc=$?
        if [ "$prc" -ne 0 ]; then
            echo "  $doc  $spec  -> decline (rc=$prc), skipped" >&2
            printf '%s\t%s\tdecline\n' "$doc" "$reqline" >> "$RAW/skipped.tsv"
            continue
        fi
        echo "  $doc  $spec" >&2
        rep=1
        while [ "$rep" -le "$REPS" ]; do
            if [ $((rep % 2)) -eq 1 ]; then
                measure "$doc" "$fmt" "$flags" "$reqline" current "$rep" 1
                measure "$doc" "$fmt" "$flags" "$reqline" selective "$rep" 2
            else
                measure "$doc" "$fmt" "$flags" "$reqline" selective "$rep" 1
                measure "$doc" "$fmt" "$flags" "$reqline" current "$rep" 2
            fi
            rep=$((rep + 1))
        done
    done <<EOF
$(obs_for "$fmt")
EOF
    rm -rf "$store"
done

# ---------------------------------------------------------------------------
# Analysis
# ---------------------------------------------------------------------------
awk -v outdir="$OUT" -v B="$BOOT" -f tools/fixtures/phase28-3-stats.awk "$RAW/samples.tsv" > "$OUT/pooled.txt"

pctl_of() { # file p
    sort -n "$1" | awk -v p="$2" '
        { a[NR]=$1 }
        END { n=NR; if(n==0){print 0; exit}
              k=(n-1)*p; f=int(k); c=f+1
              if(f==c) print a[f+1]; else print a[f+1]*(c-k)+a[c+1]*(k-f) }'
}

fmt4() { awk -v v="$1" 'BEGIN{printf "%.4f", v}'; }

{
    echo "# Phase 28.3 — hot-path profiling: paired ablation of the \`serve_source_range\` clone"
    echo
    echo "_Measurement-only gate. No production mechanism; the selective prototype is compiled"
    echo "only under the non-default \`phase28-3-ablation\` feature and enabled by \`VOLE_SERVE_SELECTIVE\`._"
    echo
    echo "## Per-stage attribution (median over reps, variant=current)"
    echo
    echo "| doc | obs | wall ms | observe ms | materialize ms | source_serve µs | src-serve % of obs | select µs | clone bytes | referenced bytes | calls | clone B/call | obj/call used/total | peak RSS KiB |"
    echo "|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|"
    awk -F'\t' 'NR>1 && $4=="current" {
        calls=$18; cpc=(calls>0)?$11/calls:0; share=($7>0)?100*$9/$7:0
        printf "| %s | %s | %.1f | %.1f | %.1f | %.1f | %.2f%% | %.1f | %d | %d | %d | %.0f | %.0f/%.0f | %d |\n",
            $1,$3,$6/1000,$7/1000,$8/1000,$9,share,$10,$11,$12,calls,cpc, (calls>0?$14/calls:0), (calls>0?$13/calls:0), $15 }' "$OUT/per_obs_stages.tsv"
    echo
    echo "## Paired ablation (ratio = selective / current; <1 means selective faster)"
    echo
    echo "- paired samples: $(awk -F' ' '/^POOLED_N/{print $2}' "$OUT/pooled.txt"); documents (clusters): $(awk -F' ' '/^DOCS/{print $2}' "$OUT/pooled.txt")"
    echo "- pooled median paired ratio: **$(awk -F' ' '/^POOLED_MEDIAN/{printf "%.4f", $2}' "$OUT/pooled.txt")**"
    echo "- pooled geometric-mean paired ratio: **$(awk -F' ' '/^POOLED_GEOMEAN/{printf "%.4f", $2}' "$OUT/pooled.txt")**"
    med_lo=$(pctl_of "$OUT/boot_median.txt" 0.025); med_hi=$(pctl_of "$OUT/boot_median.txt" 0.975)
    gm_lo=$(pctl_of "$OUT/boot_geomean.txt" 0.025); gm_hi=$(pctl_of "$OUT/boot_geomean.txt" 0.975)
    echo "- fixed-seed cluster bootstrap 95% CI (B=$(awk -F' ' '/^DRAWS/{print $2}' "$OUT/pooled.txt"), resampled by document):"
    echo "  - median: **$(fmt4 "$med_lo") .. $(fmt4 "$med_hi")**"
    echo "  - geometric mean: **$(fmt4 "$gm_lo") .. $(fmt4 "$gm_hi")**"
    echo "- MDE (80% power, normal approx from the CI half-width): median-CI **$(awk -v lo="$med_lo" -v hi="$med_hi" 'BEGIN{printf "%.4f", (1.959963985+0.8416212336)/1.959963985*((hi-lo)/2)}')**; geomean-CI **$(awk -v lo="$gm_lo" -v hi="$gm_hi" 'BEGIN{printf "%.4f", (1.959963985+0.8416212336)/1.959963985*((hi-lo)/2)}')**"
    echo "- observation ceiling for ANY clone-removal (share of wall in the whole \`serve_source_range\` call, i.e. clone + window eval + channel decode), median across (doc,obs): **$(awk -F'\t' 'NR>1 && $4=="current" && $7>0 {printf "%.2f\n", 100*$9/$7}' "$OUT/per_obs_stages.tsv" | sort -n | awk '{a[NR]=$1} END{if(NR==0){print 0}else{print a[int((NR+1)/2)]}}')%** (the clone alone is a fraction of this)."
    echo "- p50/p95/p99 wall (all samples): current **$(pctl_of "$OUT/wall_current.txt" 0.5) / $(pctl_of "$OUT/wall_current.txt" 0.95) / $(pctl_of "$OUT/wall_current.txt" 0.99) µs**, selective **$(pctl_of "$OUT/wall_selective.txt" 0.5) / $(pctl_of "$OUT/wall_selective.txt" 0.95) / $(pctl_of "$OUT/wall_selective.txt" 0.99) µs**"
    echo
    echo "| doc | n | median ratio | geom-mean ratio | win | tie | loss |"
    echo "|---|---:|---:|---:|---:|---:|---:|"
    awk -F'\t' 'NR>1 { printf "| %s | %d | %.4f | %.4f | %d | %d | %d |\n",$1,$2,$3,$4,$5,$6,$7 }' "$OUT/doc_ratios.tsv"
    echo
    echo "## Control axis (must NOT move)"
    echo
    echo "Byte counts the window references are a property of the descriptor, not of the"
    echo "variant, so \`referenced_bytes\` must be identical across variants; on this corpus"
    echo "the object table is a single always-referenced whole-source object, so"
    echo "\`clone_bytes\` is also identical (the selective variant copies the same bytes)."
    echo
    echo "| doc | obs | clone cur / sel | referenced cur / sel |"
    echo "|---|---|---:|---:|"
    awk -F'\t' 'NR>1 { c[$1"\t"$3"\t"$4]=$11; r[$1"\t"$3"\t"$4]=$12; seen[$1"\t"$3]=1 }
        END { for (k in seen) { split(k,p,"\t");
              printf "| %s | %s | %d / %d | %d / %d |\n", p[1],p[2], c[k"\tcurrent"],c[k"\tselective"], r[k"\tcurrent"],r[k"\tselective"] } }' "$OUT/per_obs_stages.tsv"
    echo
    echo "## Exactness"
    echo
    echo "Every run's answer JSON is normalised by eliding the volatile \`stats\` object and"
    echo "SHA-256'd; the current and selective variants must agree for every (doc, obs)."
    echo
    awk -F'\t' 'NR>1 { s[$1"\t"$3"\t"$4]=$24 } END {
        n=0; bad=0
        for (k in s) {
            if (k ~ /\tcurrent$/) {
                base=substr(k,1,length(k)-length("current"))
                sel=s[base "selective"]
                if (sel=="") continue
                n++
                if (s[k]!=sel) { bad++; print "MISMATCH\t" base "\t" s[k] "\t" sel }
            }
        }
        printf "EXACTNESS pairs=%d mismatches=%d\n", n, bad
    }' "$RAW/samples.tsv" | tee "$OUT/exactness.txt"
    echo
    # ---- decision -------------------------------------------------------
    sel_noop=$(awk -F'\t' 'NR>1 && $4=="current" && $11!=$12 {n++} END{print n+0}' "$OUT/per_obs_stages.tsv")
    med_resolved=$(awk -v hi="$med_hi" 'BEGIN{print (hi<1.0)?"yes":"no"}')
    gm_resolved=$(awk -v hi="$gm_hi" 'BEGIN{print (hi<1.0)?"yes":"no"}')
    echo "## Decision"
    echo
    if [ "$sel_noop" -eq 0 ]; then
        echo "- **Selection-before-materialization is a no-op on this corpus** (0 observations where the"
        echo "  window references fewer bytes than the object table): \`clone_bytes == referenced_bytes\`"
        echo "  for every (doc, obs). The object table is a single whole-source inline object that every"
        echo "  window references, so copying only the referenced objects copies exactly the same bytes."
    else
        echo "- $sel_noop observations reference fewer bytes than the object table: selection copies less."
    fi
    echo "- End-to-end paired effect (selective/current): pooled median $(awk -F' ' '/^POOLED_MEDIAN/{printf "%.4f", $2}' "$OUT/pooled.txt"), geometric mean $(awk -F' ' '/^POOLED_GEOMEAN/{printf "%.4f", $2}' "$OUT/pooled.txt")."
    echo "- **Resolved faster beyond the CI?**: median estimator **$med_resolved**, geometric-mean estimator **$gm_resolved**."
    if [ "$med_resolved" = no ] && [ "$gm_resolved" = no ]; then
        echo "- **Verdict: NEGATIVE — no resolved end-to-end improvement.** 28.4 should ship neither the"
        echo "  selection-before-materialization rewrite (it copies the same bytes) nor claim a clone win:"
        echo "  the whole \`serve_source_range\` call is a small share of wall (see the ceiling above), and any"
        echo "  remaining clone cost is a zero-copy-eval question that a selective rewrite does not address."
    else
        echo "- **Verdict: possible resolved improvement — a 28.4 candidate.** See the tables above."
    fi
    echo
    echo "Raw samples: \`$RAW/samples.tsv\` (one row per run). Estimator tables under \`$OUT/\`."
} > "$CAMP/summary.md"

cat "$OUT/exactness.txt"
echo "phase28.3 profile: done — $CAMP/summary.md" >&2

chown -R 1000:1000 "$CAMP" 2>/dev/null || true
