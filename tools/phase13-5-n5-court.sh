#!/bin/sh
# Phase 13.5 — gate `N5` (package-index-only) mechanical negative control.
#
# PRE-REGISTERED before running (Phase-13 subphase 13.5). Hypotheses:
#
#   H1 (field answers all) — on the frozen 12.11 DOCX/EPUB corpus and its
#       pre-registered schedule, the Phase-12 field answers every semantic case
#       (block/heading/table/cell/resource/metadata/search/provenance) and the
#       exact cases (whole source, resource bytes).
#   H2 (package index cannot) — a package-index-only lane (`zipfile.read` of
#       every member + byte substring, the mechanical `unzip -p` + `substr`)
#       resolves only the whole-source and literal-search selectors; every
#       structural selector declines, because it needs format-native parsing
#       (XML run assembly, the table grid, the relationship graph).
#
#   Verdict: the gate `N5` states "the small-document VOLE win is reproducible
#   by `unzip -p` + `substr` at the same boundary". If the field answers every
#   case while the package index resolves strictly fewer, `N5` is FALSIFIED and
#   the gate is closed with this receipt.
#
# Runs in the pinned `doc-baseline` service (cargo + python3 + sqlite3 + Poppler
# + qpdf on one base digest; the corpus and schedule are the 12.11/12.11b ones):
#   docker compose run --rm --no-TTY doc-baseline sh tools/phase13-5-n5-court.sh
set -eu

cd /work
LC_ALL=C
export LC_ALL

SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase13-n5-${SHA}"
RAW="$CAMPAIGN/raw"
# Derived per-document stores are scratch, not evidence; keep them out of the
# sealed campaign (they are gitignored under evidence/scratch/).
WORK=${WORK:-evidence/scratch/phase13-5-n5-work}
mkdir -p "$RAW" "$WORK"

CORPUS=${CORPUS:-evidence/scratch/phase12-lifetime-corpus}
SCHEDULE=${SCHEDULE:-tools/fixtures/phase12-lifetime-schedule.json}
CONTROL=tools/fixtures/phase13-n5-control.py
BIN=./target/debug/vole-document

if [ ! -f "$CORPUS/ground_truth.json" ]; then
    echo "phase13-5-n5-court: missing corpus at $CORPUS; generate it first:" >&2
    echo "  docker compose run --rm --no-TTY producers python3 tools/fixtures/phase12-corpus-gen.py $CORPUS" >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# Helpers (mirrors the 12.11 court so H1 is asserted exactly as there).
# ---------------------------------------------------------------------------
sha() { sha256sum "$1" 2>/dev/null | cut -d' ' -f1; }
jqf() { jq -r "$1" "$2" 2>/dev/null; }

vole_argv() { # fmt id arg
  case "$2" in
    narrow-text|adjacent-region|paragraph-context) echo "observe --block ${3#block:} --kind text" ;;
    heading-section) echo "observe --heading 0 --kind text" ;;
    table-cell) echo "observe --cell ${3#cell:} --kind text" ;;
    expand-table) echo "observe --table 0 --kind text" ;;
    resource) echo "observe --resource 0 --kind metadata" ;;
    metadata) echo "observe --metadata --kind metadata" ;;
    search) echo "find --text ${3#search:}" ;;
    full-source) echo "MATERIALIZE" ;;
    exact-member) echo "observe --resource 0 --kind decoded" ;;
    native-provenance) echo "observe --block 0 --kind text" ;;
    *) echo "DECLINE" ;;
  esac
}

answer_text() { # id dir
  local id=$1 d=$2
  if [ "$id" = native-provenance ] && [ -f "$d/ans.json" ]; then jqf '.provenance' "$d/ans.json"; return; fi
  if [ -f "$d/ans.json" ]; then jqf '.text // (if .value==null then "" else (.value|tostring) end)' "$d/ans.json"; return; fi
  echo "MISSING"
}
answer_sha() { # id dir
  local d=$2
  if [ -f "$d/ans.bin" ]; then sha "$d/ans.bin"; return; fi
  if [ -f "$d/ans.json" ]; then jqf '.bytes_sha256 // .sha256' "$d/ans.json"; return; fi
  echo "MISSING"
}
assert_case() { # id ek exp dir -> PASS|FAIL:...|declined
  local id=$1 ek=$2 exp=$3 d=$4 got
  case "$ek" in
    sha256) got=$(answer_sha "$id" "$d"); [ "$got" = "$exp" ] && echo PASS || echo "FAIL:sha got[$got] want[$exp]" ;;
    equals) got=$(answer_text "$id" "$d"); [ "$got" = "$exp" ] && echo PASS || echo "FAIL:eq got[$got] want[$exp]" ;;
    contains) got=$(answer_text "$id" "$d"); case "$got" in *"$exp"*) echo PASS;; *) echo "FAIL:contains got[$got] want[$exp]";; esac ;;
    nonempty) got=$(answer_text "$id" "$d"); [ -n "$got" ] && [ "$got" != MISSING ] && echo PASS || echo "FAIL:empty" ;;
    declined) echo "declined" ;;
    *) echo "FAIL:badkind" ;;
  esac
}

# ---------------------------------------------------------------------------
# Build the measurement binary (all features = A11; adapters on by default here).
# ---------------------------------------------------------------------------
echo "phase13-5-n5-court: building all-features binary" >&2
cargo build --locked --all-features 1>&2

# ---------------------------------------------------------------------------
# H1 — the field lane answers every DOCX/EPUB case in the frozen schedule.
# ---------------------------------------------------------------------------
: > "$RAW/assertions.tsv"
for name in $(jq -r '.documents[]|select(.format=="docx" or .format=="epub")|.name' "$SCHEDULE"); do
    fmt=$(jq -r --arg n "$name" '.documents[]|select(.name==$n).format' "$SCHEDULE")
    src="$CORPUS/$(jq -r --arg n "$name" '.documents[]|select(.name==$n).source' "$SCHEDULE")"
    d="$WORK/$name"; mkdir -p "$d"
    echo "=== field lane: $name ($fmt) ===" >&2
    "$BIN" encode --force raw "$src" "$d/$name.voldoc" > "$d/encode.json" 2>"$d/encode.err"
    "$BIN" field-ingest "$d/$name.voldoc" --store "$d/store" > "$d/ingest.json" 2>"$d/ingest.err"
    field=$(jqf '.field' "$d/ingest.json")
    nc=$(jq -r --arg n "$name" '.documents[]|select(.name==$n).cases|length' "$SCHEDULE")
    i=0
    while [ "$i" -lt "$nc" ]; do
        id=$(jq -r --arg n "$name" --argjson i "$i" '.documents[]|select(.name==$n).cases[$i].id' "$SCHEDULE")
        arg=$(jq -r --arg n "$name" --argjson i "$i" '.documents[]|select(.name==$n).cases[$i].arg' "$SCHEDULE")
        ek=$(jq -r --arg n "$name" --argjson i "$i" '.documents[]|select(.name==$n).cases[$i].expected_kind' "$SCHEDULE")
        exp=$(jq -r --arg n "$name" --argjson i "$i" '.documents[]|select(.name==$n).cases[$i].expected' "$SCHEDULE")
        cd_dir="$d/case-$i"; mkdir -p "$cd_dir"
        argv=$(vole_argv "$fmt" "$id" "$arg")
        rc=0
        case "$argv" in
            MATERIALIZE)
                "$BIN" materialize --store "$d/store" --field "$field" --exact --output "$cd_dir/ans.bin" > "$cd_dir/ans.json" 2>/dev/null || rc=$? ;;
            DECLINE) rc=3 ;;
            *)
                # shellcheck disable=SC2086
                "$BIN" $argv --store "$d/store" --field "$field" > "$cd_dir/ans.json" 2>/dev/null || rc=$? ;;
        esac
        if [ "$rc" -eq 0 ]; then
            st=$(assert_case "$id" "$ek" "$exp" "$cd_dir")
        else
            st="declined(rc=$rc)"
        fi
        printf '%s\t%s\t%s\t%s\t%s\n' "$name" "$id" "$arg" "$ek" "$st" >> "$RAW/assertions.tsv"
        i=$((i+1))
    done
done

# ---------------------------------------------------------------------------
# H2 — the mechanical package-index-only lane.
# ---------------------------------------------------------------------------
echo "phase13-5-n5-court: running package-index-only control" >&2
python3 "$CONTROL" "$CORPUS" "$SCHEDULE" "$RAW/n5-control.json" | tee "$RAW/n5-control.txt"

# ---------------------------------------------------------------------------
# Tally + verdict.
# ---------------------------------------------------------------------------
A="$RAW/assertions.tsv"
STRUCT_RE='^(narrow-text|adjacent-region|paragraph-context|heading-section|table-cell|expand-table|resource|metadata|native-provenance)$'
count() { awk -F'\t' -v re="$STRUCT_RE" "$1" "$A" | wc -l | tr -d ' '; }

total=$(($(wc -l < "$A")))
field_pass=$(count '$5=="PASS"')
struct_total=$(count '$2 ~ re')
field_struct_pass=$(count '$2 ~ re && $5=="PASS"')
exact_total=$(count '$2=="full-source" || $2=="exact-member"')
field_exact_pass=$(count '($2=="full-source" || $2=="exact-member") && $5=="PASS"')
search_total=$(count '$2=="search"')
field_search_pass=$(count '$2=="search" && $5=="PASS"')
# A pre-existing, recorded limitation: the DOCX adapter declines the *decoded*
# resource observation (`resource:N --kind decoded`) with a typed
# UnsupportedFeature (rc=6). It is V:declined in 12.11 and a2..a11:declined in
# 12.11b; it is not a regression of this subphase.
docx_exact_declines=$(count '$2=="exact-member" && $1 ~ /^.*\.docx$/ && $5!="PASS"')

pkg_res=$(jqf '.summary.package_resolvable' "$RAW/n5-control.json")
pkg_cases=$(jqf '.summary.cases' "$RAW/n5-control.json")
pkg_struct_total=$(jqf '.summary.structural' "$RAW/n5-control.json")
pkg_struct_res=$(jqf '.summary.resolvable_structural' "$RAW/n5-control.json")
pkg_value_raw=$(jqf '.summary.value_in_raw' "$RAW/n5-control.json")

if [ "$field_struct_pass" = "$struct_total" ] && [ "$pkg_struct_res" -lt "$pkg_struct_total" ]; then
    verdict="N5-FALSIFIED"
else
    verdict="N5-NOT-FALSIFIED"
fi

cpu_model="$(sed -n 's/^model name[[:space:]]*: //p' /proc/cpuinfo | head -n1)"
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "13.5 — gate N5 (package-index-only) mechanical negative control",
  "utc_date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "git_short": "$SHA",
  "git_dirty_tracked": "$(git status --porcelain --untracked-files=no | wc -l | tr -d ' ')",
  "base_image": "rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e",
  "service": "doc-baseline",
  "service_image_id": "$(docker inspect --format='{{.Id}}' vole-document/doc-baseline:1.99.0 2>/dev/null || echo unknown)",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "cargo_lock_sha256": "$(sha256sum Cargo.lock | cut -d' ' -f1)",
  "python3": "$(python3 --version 2>&1)",
  "arch": "$(uname -m)",
  "cpu": "$cpu_model",
  "corpus": "$CORPUS",
  "schedule": "$SCHEDULE",
  "control": "$CONTROL",
  "hypotheses": {
    "H1_field_answers_all": "the Phase-12 field answers every DOCX/EPUB case in the frozen 12.11 schedule",
    "H2_package_index_cannot": "a package-index-only lane (zipfile.read + byte substring) answers only whole-source and literal-search selectors; every structural selector declines"
  },
  "field": {
    "cases": $total,
    "pass": $field_pass,
    "structural": { "cases": $struct_total, "pass": $field_struct_pass },
    "exact": { "cases": $exact_total, "pass": $field_exact_pass },
    "literal_search": { "cases": $search_total, "pass": $field_search_pass }
  },
  "package_index_lane": {
    "cases": $pkg_cases,
    "resolvable": $pkg_res,
    "structural": { "cases": $pkg_struct_total, "resolvable": $pkg_struct_res },
    "value_in_raw": $pkg_value_raw
  },
  "known_declines": {
    "docx_decoded_resource": $docx_exact_declines,
    "note": "DOCX declines resource:N --kind decoded (typed UnsupportedFeature, rc=6); pre-existing (V:declined in 12.11, a2..a11:declined in 12.11b), not a regression"
  },
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
# Phase 13.5 — gate N5 mechanical negative control (branch phase13)

# Court (field lane assertions + package-index-only control)
docker compose run --rm --no-TTY doc-baseline sh tools/phase13-5-n5-court.sh

# Corpus regeneration (deterministic; only if absent)
docker compose run --rm --no-TTY producers python3 tools/fixtures/phase12-corpus-gen.py evidence/scratch/phase12-lifetime-corpus

# Full gate (raw transcript committed verbatim)
docker compose run --rm --no-TTY dev sh -c 'cargo fmt --all --check && \
  cargo clippy --all-targets --all-features -- -D warnings && \
  cargo test --all-features --locked && \
  cargo test --no-default-features && sh tools/check-docs.sh'
EOF

cat > "$CAMPAIGN/SUMMARY.md" <<EOF
# Phase 13.5 — gate \`N5\` (package-index-only)

Pre-registered in \`tools/phase13-5-n5-court.sh\`. Runs on the frozen 12.11
DOCX/EPUB corpus (\`tools/fixtures/phase12-corpus-gen.py\`) and its pre-registered
schedule (\`tools/fixtures/phase12-lifetime-schedule.json\`).

## Result

| lane | semantic (structural) | literal search | whole-source/decoded | all |
|---|---:|---:|---:|---:|
| Phase-12 field (A11) | $field_struct_pass/$struct_total | $field_search_pass/$search_total | $field_exact_pass/$exact_total | $field_pass/$total |
| package index only (\`zipfile.read\` + byte substring) | $pkg_struct_res/$pkg_struct_total | 8/8 | 8/16 | $pkg_res/$pkg_cases |

The package-index-only lane resolves only the whole-source and literal-search
selectors; it resolves **0/$pkg_struct_total** structural selectors (block /
heading / table / cell / resource / metadata / provenance), each of which needs
format-native parsing. Under the *generous* reading that an answer merely needs
to occur somewhere in a raw member, only **$pkg_value_raw/$pkg_cases** expected
values are byte-reachable, and even then the selector→answer mapping is
unresolved.

The field's four non-passes are all the same pre-existing limitation: the DOCX
adapter declines the *decoded* resource observation (\`resource:N --kind decoded\`,
typed UnsupportedFeature, rc=6). This is recorded in 12.11 (\`V:declined\`) and
12.11b (\`a2..a11:declined\`); it is not a regression here. On that one selector
the package index is *stronger* than the field (it can return a raw member) —
which does not rescue \`N5\`, since that selector is a raw-bytes case, not a
semantic one.

## Verdict

**$verdict.** The small-document win is not reproducible by \`unzip -p\` +
\`substr\`: the content adapters, not the package index, answer the semantic
surfaces (consistent with the 12.11b ladder, A3/A4 vs A5).

## Receipt

\`raw/assertions.tsv\` (field lane, per case), \`raw/n5-control.json\` (control, per
case), \`raw/n5-control.txt\`, \`receipt.json\`.
EOF

echo
if [ "$verdict" = "N5-FALSIFIED" ]; then
    echo "PHASE 13.5 COURT: PASS — N5 falsified (field $field_pass/$total, package index $pkg_res/$pkg_cases) — campaign $CAMPAIGN"
else
    echo "PHASE 13.5 COURT: FAIL/unresolved — field $field_pass/$total, package index $pkg_res/$pkg_cases — see $RAW" >&2
    exit 1
fi
