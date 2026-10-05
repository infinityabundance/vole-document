#!/bin/sh
# Phase-7.0b generator-family Flate corpus.
#
# Runs entirely inside the separate `producers` image (ReportLab, Cairo,
# LibreOffice, pdfTeX, qpdf). Unlike the pinned `tools` producers (qpdf,
# Ghostscript) these are genuine *authoring* generators with their own DEFLATE
# behaviour, which is exactly what Phase 7.0 showed was missing: the qpdf and
# Ghostscript *transformer* lineage never produces the shared-plaintext win
# region. This script measures whether real authoring applications do.
#
# It generates, from the same deterministic content document used by
# `tools/pdf-corpus.sh` (`gen_content`: "Line i of <seed>: …"), one PDF per
# available family:
#   * ReportLab  — a 6-page document whose pages share one sizeable content
#                  stream, page-compressed with Flate (`pageCompression=1`);
#   * Cairo      — a vector-heavy page (600 strokes + 200 rectangles + text),
#                  repeated across pages (pycairo PDF surface, Flate by default);
#   * LibreOffice— a headless HTML->PDF export (`--convert-to pdf`, fixed
#                  UserInstallation profile dir, SAL_USE_VCLPLUGIN=svp);
#   * pdfTeX     — a small `.tex` compiled with pdflatex (Flate streams by
#                  default), SOURCE_DATE_EPOCH/FORCE_SOURCE_DATE pinned.
#
# Every produced PDF is `qpdf --check`'d. Then, for each output, the script:
#   1. fingerprints every /FlateDecode stream payload (sha256 + length) with
#      `qpdf --json` + `qpdf --show-object=N --raw-stream-data`;
#   2. runs `qpdf --deterministic-id` (which normalizes /ID from a content hash)
#      and re-fingerprints;
#   3. keeps the normalized output ONLY if every Flate payload is byte-identical
#      (i.e. normalization is /ID-only and does not rewrite the DEFLATE streams);
#      otherwise it keeps the raw producer output and records the caveat.
# It also builds each family twice and records whether the outputs are
# byte-reproducible run to run.
#
# `provenance.json` records name, sha256, producer, producer_version, command,
# license:"locally-generated", plus the Flate stream count, the /ID
# normalization decision, and the reproducibility verdict. No third-party bytes
# are used; the regenerable `.pdf` bytes are gitignored.
#
# Usage (from the host):
#   docker compose run --rm --no-TTY producers sh tools/pdf-corpus-producers.sh
set -eu

cd /work
LC_ALL=C
export LC_ALL

OUT=evidence/corpus/phase7-producers
TMP="$OUT/.tmp"
mkdir -p "$OUT" "$TMP"

# ---------------------------------------------------------------------------
# Availability probes (honest: a family that is not present is skipped, not
# substituted with a synthetic stand-in).
# ---------------------------------------------------------------------------
have_reportlab=0; have_cairo=0; have_lo=0; have_tex=0
python3 -c 'import reportlab' 2>/dev/null && have_reportlab=1 || true
python3 -c 'import cairo' 2>/dev/null && have_cairo=1 || true
command -v soffice >/dev/null 2>&1 && have_lo=1 || true
command -v pdflatex >/dev/null 2>&1 && have_tex=1 || true

RL_V="-"; [ "$have_reportlab" -eq 1 ] && RL_V=$(python3 -c 'import reportlab; print(reportlab.Version)')
CAIRO_V="-"; [ "$have_cairo" -eq 1 ] && CAIRO_V=$(python3 -c 'import cairo; print(cairo.version + "/libcairo " + cairo.cairo_version_string())')
LO_V="-"; [ "$have_lo" -eq 1 ] && LO_V=$(soffice --version 2>/dev/null | awk '{print $2; exit}')
TEX_V="-"; [ "$have_tex" -eq 1 ] && TEX_V=$(pdflatex --version 2>/dev/null | head -1 | sed 's/^pdfTeX //; s/ (TeX Live.*//')
QPDF_V="$(qpdf --version | awk '{print $3; exit}')"
echo "tool versions: reportlab=$RL_V cairo=$CAIRO_V libreoffice=$LO_V pdftex=$TEX_V qpdf=$QPDF_V"

# ---------------------------------------------------------------------------
# Deterministic content document. Mirrors tools/pdf-corpus.sh `gen_content`
# byte-for-byte so the two corpora share one content lineage.
# ---------------------------------------------------------------------------
cat > "$TMP/content.py" <<'PY'
def content_lines(seed, n):
    out = []
    for i in range(1, n + 1):
        if i % 3 == 0:
            out.append("Line %d of %s: repeated text shared across pages, amount %d.00"
                       % (i, seed, (i * 7) % 1000))
        else:
            out.append("Line %d of %s: the quick brown fox jumps over the lazy dog %d"
                       % (i, seed, i))
    return out
PY

# ---------------------------------------------------------------------------
# ReportLab: 6 pages, each with one identical two-column, 170-line content
# stream (~11 KB plaintext per page), page-compressed with Flate.
# ---------------------------------------------------------------------------
cat > "$TMP/gen_reportlab.py" <<'PY'
import sys
sys.path.insert(0, sys.argv[6])
from reportlab import rl_config
rl_config.invariant = 1
rl_config.useA85 = 0
rl_config.pageCompression = 1
from reportlab.pdfgen import canvas
from content import content_lines

path, pages, ncol, nlines, seed = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4]), sys.argv[5]
lines = content_lines(seed, nlines)
c = canvas.Canvas(path, pagesize=(612, 792), pageCompression=1)
c.setTitle("VOLE generator-family corpus")
c.setAuthor("VOLE-Document")
for _p in range(pages):
    c.setFont("Helvetica", 6)
    for col in range(ncol):
        x = 36 + col * 276
        y = 780
        for ln in lines:
            c.drawString(x, y, ln)
            y -= 9
    c.showPage()
c.save()
PY

# ---------------------------------------------------------------------------
# Cairo: a vector-heavy page repeated across pages (identical plaintext).
# ---------------------------------------------------------------------------
cat > "$TMP/gen_cairo.py" <<'PY'
import sys
sys.path.insert(0, sys.argv[4])
import cairo
from content import content_lines

path, pages, seed = sys.argv[1], int(sys.argv[2]), sys.argv[3]
lines = content_lines(seed, 170)
W, H = 612, 792
surf = cairo.PDFSurface(path, W, H)
try:
    surf.set_metadata(cairo.PDF_METADATA_CREATE_DATE, "2026-10-05T00:00:00Z")
    surf.set_metadata(cairo.PDF_METADATA_MOD_DATE, "2026-10-05T00:00:00Z")
    surf.set_metadata(cairo.PDF_METADATA_TITLE, "VOLE generator-family corpus")
    surf.set_metadata(cairo.PDF_METADATA_AUTHOR, "VOLE-Document")
except Exception:
    pass
ctx = cairo.Context(surf)
for _p in range(pages):
    ctx.set_source_rgb(0, 0, 0)
    ctx.set_line_width(0.7)
    for i in range(600):
        x = 16 + ((i * 37) % 580)
        y = 16 + ((i * 53) % 760)
        ctx.move_to(x, y)
        ctx.line_to(x + 24, y + 12)
        ctx.stroke()
    for i in range(200):
        x = 16 + ((i * 29) % 560)
        y = 16 + ((i * 71) % 740)
        ctx.rectangle(x, y, 18, 9)
        ctx.stroke()
    ctx.select_font_face("Helvetica", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_NORMAL)
    ctx.set_font_size(6)
    for i, ln in enumerate(lines):
        col = i // 85
        row = i % 85
        ctx.move_to(36 + col * 276, 780 - row * 9)
        ctx.show_text(ln)
    ctx.show_page()
surf.finish()
PY

# ---------------------------------------------------------------------------
# LibreOffice: deterministic HTML converted headless to PDF.
# ---------------------------------------------------------------------------
cat > "$TMP/gen_html.py" <<'PY'
import sys
sys.path.insert(0, sys.argv[4])
from content import content_lines

path, npar, seed = sys.argv[1], int(sys.argv[2]), sys.argv[3]
lines = content_lines(seed, npar)
with open(path, "w") as f:
    f.write('<!DOCTYPE html>\n<html><head><meta charset="utf-8">'
            '<title>VOLE generator-family corpus</title></head><body>\n')
    for ln in lines:
        esc = ln.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
        f.write("<p>%s</p>\n" % esc)
    f.write("</body></html>\n")
PY

# ---------------------------------------------------------------------------
# pdfTeX: a small .tex with repeated identical pages (Flate by default).
# ---------------------------------------------------------------------------
cat > "$TMP/gen_tex.py" <<'PY'
import sys
sys.path.insert(0, sys.argv[5])
from content import content_lines

path, pages, nlines, seed = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), sys.argv[4]
lines = content_lines(seed, nlines)

def esc(s):
    for a, b in [("\\", "\\textbackslash{}"), ("&", "\\&"), ("%", "\\%"),
                 ("$", "\\$"), ("#", "\\#"), ("_", "\\_"), ("{", "\\{"),
                 ("}", "\\}"), ("~", "\\textasciitilde{}"), ("^", "\\textasciicircum{}")]:
        s = s.replace(a, b)
    return s

with open(path, "w") as f:
    f.write("\\documentclass{article}\n")
    f.write("\\usepackage[margin=0.5in]{geometry}\n")
    f.write("\\pagestyle{empty}\n")
    f.write("\\begin{document}\n\\fontsize{4}{4.8}\\selectfont\n")
    for p in range(pages):
        for ln in lines:
            f.write(esc(ln) + "\\par\n")
        if p != pages - 1:
            f.write("\\newpage\n")
    f.write("\\end{document}\n")
PY

CONTENT_SEED="producers"
RL_PAGES=6; RL_COLS=2; RL_LINES=85
CAIRO_PAGES=6
LO_PARS=1700
TEX_PAGES=6; TEX_LINES=140

# gen_all DIR — build every available family into DIR (all outputs there).
gen_all() {
  _dir=$1
  mkdir -p "$_dir"
  if [ "$have_reportlab" -eq 1 ]; then
    python3 "$TMP/gen_reportlab.py" "$_dir/reportlab-multipage.pdf" \
      "$RL_PAGES" "$RL_COLS" "$RL_LINES" "$CONTENT_SEED" "$TMP"
  fi
  if [ "$have_cairo" -eq 1 ]; then
    python3 "$TMP/gen_cairo.py" "$_dir/cairo-vector.pdf" "$CAIRO_PAGES" "$CONTENT_SEED" "$TMP"
  fi
  if [ "$have_lo" -eq 1 ]; then
    python3 "$TMP/gen_html.py" "$_dir/lo-input.html" "$LO_PARS" "$CONTENT_SEED" "$TMP"
    rm -rf "$_dir/loprofile" "$_dir/lopdf"
    mkdir -p "$_dir/lopdf"
    # Fixed profile dir + the headless VCL plugin; no display, no first-start UX.
    SAL_USE_VCLPLUGIN=svp soffice --headless --nologo --nofirststartwizard \
      -env:UserInstallation="file://$PWD/$_dir/loprofile" \
      --convert-to pdf --outdir "$_dir/lopdf" "$_dir/lo-input.html" >/dev/null 2>&1
    mv "$_dir/lopdf/lo-input.pdf" "$_dir/libreoffice-export.pdf"
  fi
  if [ "$have_tex" -eq 1 ]; then
    python3 "$TMP/gen_tex.py" "$_dir/tex-input.tex" "$TEX_PAGES" "$TEX_LINES" "$CONTENT_SEED" "$TMP"
    ( cd "$_dir" && SOURCE_DATE_EPOCH=0 FORCE_SOURCE_DATE=1 \
        pdflatex -interaction=nonstopmode -halt-on-error tex-input.tex >/dev/null 2>&1 )
    mv "$_dir/tex-input.pdf" "$_dir/pdftex-doc.pdf"
  fi
}

# fingerprint FILE — one sorted line "len:sha256" per /FlateDecode stream payload,
# a canonical multiset that is invariant to qpdf object renumbering. Empty output
# means no Flate streams were located.
fingerprint() {
  _f=$1
  qpdf --json "$_f" 2>/dev/null | python3 -c '
import sys, json
d = json.load(sys.stdin)
objs = {}
for e in d["qpdf"]:
    if isinstance(e, dict) and any(str(k).startswith("obj:") for k in e):
        objs = e
        break
nums = []
for k, v in objs.items():
    if k.startswith("obj:") and isinstance(v, dict) and "stream" in v:
        flt = v["stream"]["dict"].get("/Filter")
        fs = flt if isinstance(flt, list) else ([flt] if flt else [])
        if any(str(x) == "/FlateDecode" for x in fs):
            nums.append(int(k[4:].split()[0]))
print("\n".join(str(n) for n in sorted(nums)))
' | while IFS= read -r _n; do
      [ -n "$_n" ] || continue
      _h=$(qpdf --show-object="$_n" --raw-stream-data "$_f" 2>/dev/null | sha256sum | cut -d' ' -f1)
      _l=$(qpdf --show-object="$_n" --raw-stream-data "$_f" 2>/dev/null | wc -c | tr -d ' ')
      printf '%s:%s\n' "$_l" "$_h"
    done | sort
}

sha_of() { sha256sum "$1" | cut -d' ' -f1; }
count_flate() { fingerprint "$1" | grep -c . || true; }

# normalize NAME RAW — apply qpdf /ID normalization only if it is /ID-only,
# then place the final file at $OUT/NAME and print "decision|sha256|flate".
# --stream-data=preserve keeps every Flate payload byte-identical (qpdf's default
# --stream-data=compress would re-encode them at qpdf's own zlib level); the
# fingerprint comparison below proves the payloads survived unchanged.
normalize() {
  _name=$1; _raw=$2
  _raw_fp=$(fingerprint "$_raw")
  _norm="$TMP/norm-$_name"
  qpdf --deterministic-id --stream-data=preserve --object-streams=preserve "$_raw" "$_norm" 2>/dev/null
  _norm_fp=$(fingerprint "$_norm")
  if [ "$_raw_fp" = "$_norm_fp" ]; then
    cp "$_norm" "$OUT/$_name"
    _decision="normalized:qpdf --deterministic-id --stream-data=preserve --object-streams=preserve;/ID-only (all Flate payloads byte-identical)"
  else
    cp "$_raw" "$OUT/$_name"
    _decision="raw:qpdf --deterministic-id --stream-data=preserve --object-streams=preserve rewrote Flate payloads; raw producer output kept, non-determinism caveat recorded"
  fi
  _fl=$(count_flate "$OUT/$_name")
  printf '%s|%s|%s\n' "$_decision" "$(sha_of "$OUT/$_name")" "$_fl"
}

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------
echo "=== building each family twice (determinism check) ==="
rm -rf "$TMP/det1" "$TMP/det2"
gen_all "$TMP/det1"
gen_all "$TMP/det2"

PROVENANCE="$OUT/provenance.json"
ENTRIES="$TMP/provenance-entries.jsonl"
: > "$ENTRIES"

# record NAME PRODUCER PVER CMD REPRO_RUN1_SHA REPRO_RUN2_SHA
record() {
  _name=$1; _producer=$2; _pver=$3; _cmd=$4; _s1=$5; _s2=$6
  if [ "$_s1" = "$_s2" ]; then _repro=true; else _repro=false; fi
  _res=$(normalize "$_name" "$TMP/det1/$_name")
  _decision=$(printf '%s' "$_res" | cut -d'|' -f1)
  _sha=$(printf '%s' "$_res" | cut -d'|' -f2)
  _fl=$(printf '%s' "$_res" | cut -d'|' -f3)
  printf '{"name":"%s","sha256":"%s","producer":"%s","producer_version":"%s","command":"%s","license":"locally-generated","flate_streams":%s,"id_normalization":"%s","byte_reproducible":%s}\n' \
    "$_name" "$_sha" "$_producer" "$_pver" "$_cmd" "$_fl" "$_decision" "$_repro" >> "$ENTRIES"
  echo "  $_name flate=$_fl reproducible=$_repro"
}

if [ "$have_reportlab" -eq 1 ]; then
  record "reportlab-multipage.pdf" "ReportLab" "$RL_V" \
    "python3 gen_reportlab.py reportlab-multipage.pdf pages=$RL_PAGES cols=$RL_COLS lines/col=$RL_LINES seed=$CONTENT_SEED (canvas, pageCompression=1, useA85=0, rl_config.invariant=1)" \
    "$(sha_of "$TMP/det1/reportlab-multipage.pdf")" "$(sha_of "$TMP/det2/reportlab-multipage.pdf")"
else
  echo "SKIP ReportLab (python3-reportlab not available)"
fi

if [ "$have_cairo" -eq 1 ]; then
  record "cairo-vector.pdf" "Cairo" "$CAIRO_V" \
    "python3 gen_cairo.py cairo-vector.pdf pages=$CAIRO_PAGES seed=$CONTENT_SEED (pycairo PDFSurface, fixed metadata)" \
    "$(sha_of "$TMP/det1/cairo-vector.pdf")" "$(sha_of "$TMP/det2/cairo-vector.pdf")"
else
  echo "SKIP Cairo (python3-cairo not available)"
fi

if [ "$have_lo" -eq 1 ]; then
  record "libreoffice-export.pdf" "LibreOffice Writer" "$LO_V" \
    "SAL_USE_VCLPLUGIN=svp soffice --headless --nologo --nofirststartwizard -env:UserInstallation=file://<fixed>/ --convert-to pdf --outdir <dir> lo-input.html (paras=$LO_PARS seed=$CONTENT_SEED)" \
    "$(sha_of "$TMP/det1/libreoffice-export.pdf")" "$(sha_of "$TMP/det2/libreoffice-export.pdf")"
else
  echo "SKIP LibreOffice (soffice not available)"
fi

if [ "$have_tex" -eq 1 ]; then
  record "pdftex-doc.pdf" "pdfTeX (pdflatex)" "$TEX_V" \
    "SOURCE_DATE_EPOCH=0 FORCE_SOURCE_DATE=1 pdflatex -interaction=nonstopmode -halt-on-error tex-input.tex (pages=$TEX_PAGES lines/page=$TEX_LINES seed=$CONTENT_SEED)" \
    "$(sha_of "$TMP/det1/pdftex-doc.pdf")" "$(sha_of "$TMP/det2/pdftex-doc.pdf")"
else
  echo "SKIP pdfTeX (pdflatex not available)"
fi

# ---------------------------------------------------------------------------
# Validate every produced PDF with the independent qpdf oracle (strict).
# ---------------------------------------------------------------------------
echo
echo "=== qpdf --check (strict) ==="
_fail=0
_total=0
for _f in "$OUT"/*.pdf; do
  [ -e "$_f" ] || continue
  _total=$((_total + 1))
  _name=$(basename "$_f")
  set +e
  qpdf --check "$_f" > "$TMP/check.txt" 2>&1
  _rc=$?
  set -e
  case "$_rc" in
    0) _verdict=valid ;;
    *) _verdict=INVALID; _fail=1
       echo "FINDING: qpdf --check failed for $_name (rc=$_rc)" >&2
       cat "$TMP/check.txt" >&2 ;;
  esac
  printf '%-28s %s\n' "$_name" "$_verdict"
done

# ---------------------------------------------------------------------------
# Provenance ledger
# ---------------------------------------------------------------------------
{
  printf '{\n'
  printf '  "generator": "tools/pdf-corpus-producers.sh",\n'
  printf '  "license": "locally-generated",\n'
  printf '  "note": "No third-party document bytes. Every .pdf is produced by the pinned producers image from our own deterministic inputs; the .pdf bytes are gitignored and regenerable from this script.",\n'
  printf '  "reproducibility": "Each family is generated twice and the two SHA-256s compared. byte_reproducible is true only when they match. /ID normalization is applied per file ONLY when qpdf --deterministic-id --stream-data=preserve --object-streams=preserve leaves every /FlateDecode payload byte-identical (id_normalization field); otherwise the raw producer output is kept and the non-determinism caveat stands.",\n'
  printf '  "tools": {"reportlab": "%s", "cairo": "%s", "libreoffice": "%s", "pdftex": "%s", "qpdf": "%s"},\n' \
    "$RL_V" "$CAIRO_V" "$LO_V" "$TEX_V" "$QPDF_V"
  printf '  "families_present": {"reportlab": %s, "cairo": %s, "libreoffice": %s, "pdftex": %s},\n' \
    "$have_reportlab" "$have_cairo" "$have_lo" "$have_tex"
  printf '  "files": [\n'
  awk 'NR>1{printf ",\n"} {printf "    %s", $0}' "$ENTRIES"
  printf '\n  ]\n'
  printf '}\n'
} > "$PROVENANCE"

echo
echo "provenance: $PROVENANCE ($_total generated PDFs)"
rm -rf "$TMP"

if [ "$_fail" -ne 0 ]; then
  echo "PRODUCERS CORPUS: FAIL" >&2
  exit 1
fi
echo "PRODUCERS CORPUS: OK ($_total generated PDFs, all qpdf --check clean)"
