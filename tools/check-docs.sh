#!/bin/sh
# Documentation integrity checks for vole-document.
#
# Pure POSIX shell; no network, no toolchain. Run from anywhere:
#   sh tools/check-docs.sh
# Inside Docker (the canonical path):
#   docker compose run --rm --no-TTY dev sh -c 'sh tools/check-docs.sh'
#
# Enforces:
#   1. every internal Markdown link resolves on disk;
#   2. no root-level reader docs besides README.md (AGENTS.md is the one
#      permitted operational-metadata exception and is not a reader doc);
#   3. every ADR reference resolves and ADR numbers are unique;
#   4. phase/evidence links resolve (covered by the link check);
#   5. the required documentation set exists;
#   6. README.md is at most 300 lines;
#   7. every compose.yaml service is hard-capped (OOM containment), via
#      tools/check-compose-caps.sh.
#
# `research/` is gitignored and intentionally excluded.

set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

fail=0
note() { printf '%s\n' "$*"; }
err() { printf 'FAIL: %s\n' "$*"; fail=1; }

# --- 6. README size -------------------------------------------------------
readme_lines=$(wc -l < README.md | tr -d ' ')
if [ "$readme_lines" -gt 300 ]; then
    err "README.md is $readme_lines lines (must be <= 300)"
else
    note "OK: README.md is $readme_lines lines (<= 300)"
fi

# --- 5. required docs exist ----------------------------------------------
required_docs="
README.md
AGENTS.md
CITATION.cff
docs/README.md
docs/SECURITY.md
docs/architecture/overview.md
docs/architecture/authority-and-exactness.md
docs/architecture/document-field.md
docs/architecture/inverse-proceduralization.md
docs/architecture/observations-and-provenance.md
docs/architecture/persistence-and-caching.md
docs/architecture/multi-format-adapters.md
docs/formats/pdf.md
docs/formats/docx.md
docs/formats/epub.md
docs/reference/specification.md
docs/reference/conformance.md
docs/reference/cli.md
docs/reference/format-support.md
docs/project/status.md
docs/project/findings.md
docs/project/changelog.md
docs/project/roadmap.md
docs/project/doc-migration-ledger.md
docs/evidence/README.md
docs/adr/README.md
"
missing_required=0
for doc in $required_docs; do
    if [ ! -f "$doc" ]; then
        err "required doc missing: $doc"
        missing_required=1
    fi
done
if [ "$missing_required" -eq 0 ]; then note "OK: required documentation set present"; fi

# --- 2. no root-level reader docs besides README.md ----------------------
for f in "$root"/*.md; do
    [ -e "$f" ] || continue
    case "$(basename "$f")" in
        README.md) : ;;
        AGENTS.md) note "OK: AGENTS.md at root (operational agent metadata, permitted)" ;;
        *) err "root-level reader doc not allowed: $(basename "$f")" ;;
    esac
done

# --- 3. duplicate ADR numbers --------------------------------------------
adr_nums=$(for f in docs/adr/[0-9][0-9][0-9][0-9]-*.md; do
    [ -e "$f" ] || continue
    basename "$f" | cut -c1-4
done)
dupes=$(printf '%s\n' "$adr_nums" | sort | uniq -d)
if [ -n "$dupes" ]; then
    for d in $dupes; do err "duplicate ADR number: $d"; done
fi
note "OK: ADR numbers unique"

# --- 3b. every ADR-NNNN reference resolves -------------------------------
adr_refs=$(grep -rhoE 'ADR-[0-9]{4}' --include='*.md' . \
    | grep -v './target/' | sort -u)
for ref in $adr_refs; do
    num=$(printf '%s' "$ref" | cut -c5-8)
    match=$(find docs/adr -maxdepth 1 -name "$num-*.md" | head -n 1)
    if [ -z "$match" ]; then
        err "ADR reference has no file: $ref"
    fi
done
note "OK: every ADR-NNNN reference resolves"

# --- 1 + 4. internal Markdown links resolve ------------------------------
# `research/` is gitignored and intentionally excluded. Test fixtures and
# pinned corpus/scratch data are Markdown *inputs* (whose link syntax is
# deliberately not a real reference), not reader documentation, so they are
# excluded from the reader-doc link scan too. Sealed receipts under
# `evidence/campaigns/` remain scanned.
md_files=$(find . -name '*.md' -not -path './target/*' -not -path './.git/*' \
    -not -path './research/*' -not -path './tools/fixtures/*' \
    -not -path './realformats-v1/*' -not -path './evidence/scratch/*')

link_count=0
for f in $md_files; do
    dir=$(dirname "$f")
    # Extract markdown link targets: `](target)`.
    targets=$(grep -oE '\]\([^)]+\)' "$f" 2>/dev/null | sed -e 's/^](//' -e 's/)$//' || true)
    [ -n "$targets" ] || continue
    for target in $targets; do
        case "$target" in
            http://*|https://*|mailto:*|'#'*|'<'*) continue ;;
        esac
        path=${target%%#*}
        [ -n "$path" ] || continue
        link_count=$((link_count + 1))
        resolved="$dir/$path"
        if [ ! -e "$resolved" ]; then
            err "broken Markdown link: $f -> $target"
        fi
    done
done
note "OK: scanned $link_count internal Markdown links"

# --- 7. every compose service is hard-capped (OOM containment) -----------
if [ -f tools/check-compose-caps.sh ]; then
    if sh tools/check-compose-caps.sh; then :; else err "compose OOM-containment audit failed"; fi
else
    err "required check missing: tools/check-compose-caps.sh"
fi

if [ "$fail" -ne 0 ]; then
    printf '\ncheck-docs.sh: FAILED\n'
    exit 1
fi
printf '\ncheck-docs.sh: all documentation checks passed\n'
