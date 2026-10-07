#!/bin/sh
# real100-v1 development pilot: 20 real NASA/NIST documents spanning the five
# cells (NASA PDF/EPUB, NIST PDF/DOCX/EPUB) and the eras, doc types and
# structural categories the frozen corpus must cover.
#
# DEVELOPMENT-ONLY. These documents are used to shake out producer quirks and
# harness bugs. They are recorded under real100-v1/pilot/ and are NEVER part of
# the frozen 100. A pilot document may later be re-selected into the frozen 100;
# if so it is acquired again under real100-v1/ with its own id and hash.
#
# Selection was made on pre-performance attributes only: agency, format, series,
# stiType, year, size, and the availability of agency-published cross-format /
# revision families. No codec result influenced any choice. Freeze is by SHA-256.
#
#   docker compose run --rm --no-TTY realcorpus tools/realcorpus/select-pilot.sh
#
# Re-running is safe: any id already present in the manifest is skipped. A source
# that is unreachable or fails a check is reported and skipped (never faked).
set -u
here=$(cd "$(dirname "$0")" && pwd)
CORPUS=${CORPUS:-/work/real100-v1/pilot}
PUBDOM="US-Gov-Public-Domain"
FAILED=""

add_doc() {
    id=$1; agency=$2; fmt=$3; url=$4; landing=$5; title=$6; pubid=$7; year=$8
    family=$9; doctype=${10}; producer=${11}; tags=${12}; rights=${13}; redist=${14}
    cross=${15:-}; rev=${16:-}
    if [ -f "$CORPUS/manifest.tsv" ] && grep -q "^${id}	" "$CORPUS/manifest.tsv"; then
        echo "skip (present): $id"
        return 0
    fi
    if ! python3 "$here/acquire.py" add --corpus "$CORPUS" \
        --id "$id" --agency "$agency" --format "$fmt" --url "$url" \
        --landing-page "$landing" --title "$title" --publication-id "$pubid" \
        --year "$year" --family "$family" --document-type "$doctype" \
        --producer "$producer" --structural-tags "$tags" \
        --cross-format-family-id "$cross" --revision-family-id "$rev" \
        --rights-status "$rights" --redistributable "$redist"; then
        echo "FAILED (recorded, continuing): $id" >&2
        FAILED="$FAILED $id"
    fi
}

NTRS="https://ntrs.nasa.gov"

# --- NASA PDF (NTRS) ------------------------------------------------------
add_doc nasa-pdf-0001 nasa pdf \
  "$NTRS/api/citations/19640056668/downloads/19640056668.pdf" \
  "$NTRS/citations/19640056668" \
  "Summary of NACA Research on Afterburners for Turbojet Engines" \
  "NACA-RM-E55L12" 1956 NACA NACA "NASA/NACA (Legacy CDMS)" \
  "scanned;legacy-image;figure-heavy;center:headquarters" "$PUBDOM" true

add_doc nasa-pdf-0002 nasa pdf \
  "$NTRS/api/citations/19630009859/downloads/19630009859.pdf" \
  "$NTRS/citations/19630009859" \
  "Full-Scale Wind-Tunnel Investigation of a Flexible-Wing Manned Test Vehicle" \
  "NASA-TN-D-1946" 1963 TN TN "NASA (Legacy CDMS)" \
  "figure-heavy;table-heavy" "$PUBDOM" true

add_doc nasa-pdf-0003 nasa pdf \
  "$NTRS/api/citations/19920015435/downloads/19920015435.pdf" \
  "$NTRS/citations/19920015435" \
  "NASA Aerodynamics Program" \
  "NASA-TM-4368" 1992 TM TM "NASA (Legacy CDMS)" \
  "table-heavy;figure-heavy;very-large" "$PUBDOM" true

add_doc nasa-pdf-0004 nasa pdf \
  "$NTRS/api/citations/20020039536/downloads/20020039536.pdf" \
  "$NTRS/citations/20020039536" \
  "Research on Hazardous States of Awareness and Physiological Factors in Aerospace Operations" \
  "NASA/TM-2002-211444" 2002 TM TM "NASA Langley Research Center" \
  "table-heavy;reference-heavy;center:langley" "$PUBDOM" true

add_doc nasa-pdf-0005 nasa pdf \
  "$NTRS/api/citations/20160003515/downloads/20160003515.pdf" \
  "$NTRS/citations/20160003515" \
  "Boeing Smart Rotor Full-scale Wind Tunnel Test Data Report" \
  "NASA/TM-2016-216048" 2016 TM TM "NASA Ames Research Center" \
  "table-heavy;figure-heavy;center:ames" "$PUBDOM" true

add_doc nasa-pdf-0006 nasa pdf \
  "$NTRS/api/citations/20080008301/downloads/20080008301.pdf" \
  "$NTRS/citations/20080008301" \
  "NASA Systems Engineering Handbook" \
  "NASA/SP-2007-6105 Rev1" 2007 SP HANDBOOK "NASA Headquarters" \
  "born-digital;reference-heavy;appendix-heavy;center:headquarters" \
  "$PUBDOM" true "" "rev-nasa-se-handbook-6105"

# --- NASA EPUB (nasa.gov official e-books) --------------------------------
add_doc nasa-epub-0001 nasa epub \
  "https://www.nasa.gov/wp-content/uploads/2023/08/economic-development-of-low-earth-orbit-ebook_0.epub" \
  "https://www.nasa.gov/ebooks/economic-development-of-low-earth-orbit/" \
  "Economic Development of Low Earth Orbit" \
  "NASA eBook" 2016 HISTORY BOOK "NASA" \
  "multi-chapter;reference-heavy" "$PUBDOM" true

add_doc nasa-epub-0002 nasa epub \
  "https://www.nasa.gov/wp-content/uploads/2023/08/2018_earth_as_art-nasa.epub" \
  "https://www.nasa.gov/ebooks/earth-as-art/" \
  "Earth as Art"  "NASA eBook" 2018 PHOTO BOOK "NASA" \
  "image-heavy" "$PUBDOM" true

add_doc nasa-epub-0003 nasa epub \
  "https://www.nasa.gov/wp-content/uploads/2019/10/iss-fluid-physics-2020.epub" \
  "https://www.nasa.gov/science-research/for-researchers/a-researchers-guide-to-fluid-physics/" \
  "A Researcher's Guide to: Fluid Physics" \
  "NASA ISS Researcher's Guide" 2020 RESEARCH GUIDE "NASA" \
  "simple;nav-heavy" "$PUBDOM" true

add_doc nasa-epub-0004 nasa epub \
  "https://www.nasa.gov/wp-content/uploads/2019/11/earth_at_night-ebook.epub" \
  "https://www.nasa.gov/ebooks/earth-at-night/" \
  "Earth at Night" "NASA eBook" 2019 PHOTO BOOK "NASA" \
  "image-heavy;unusual" "$PUBDOM" true

# --- NIST PDF (NVL) -------------------------------------------------------
add_doc nist-pdf-0002 nist pdf \
  "https://nvlpubs.nist.gov/nistpubs/SpecialPublications/NIST.SP.800-53r5.pdf" \
  "https://csrc.nist.gov/pubs/sp/800/53/r5/upd1/final" \
  "Security and Privacy Controls for Information Systems and Organizations" \
  "NIST SP 800-53 Rev. 5" 2020 SP SP "NIST" \
  "table-heavy;appendix-heavy;reference-heavy" "$PUBDOM" true "" "rev-nist-sp800-53"

add_doc nist-pdf-0003 nist pdf \
  "https://nvlpubs.nist.gov/nistpubs/SpecialPublications/NIST.SP.800-18r2.pdf" \
  "https://csrc.nist.gov/pubs/sp/800/18/r2/final" \
  "Guide for Developing Security Plans for Federal Information Systems" \
  "NIST SP 800-18 Rev. 2" 2025 SP SP "NIST" \
  "born-digital;reference-heavy" "$PUBDOM" true "cf-nist-sp800-18r2" "rev-nist-sp800-18"

add_doc nist-pdf-0004 nist pdf \
  "https://nvlpubs.nist.gov/nistpubs/Legacy/SP/nistspecialpublication800-115.pdf" \
  "https://csrc.nist.gov/pubs/sp/800/115/final" \
  "Technical Guide to Information Security Testing and Assessment" \
  "NIST SP 800-115" 2008 OTHER SP "NIST" \
  "table-heavy;figure-heavy" "$PUBDOM" true "cf-nist-sp800-115"

add_doc nist-pdf-0005 nist pdf \
  "https://nvlpubs.nist.gov/nistpubs/TechnicalNotes/NIST.TN.2161.pdf" \
  "https://nvlpubs.nist.gov/nistpubs/TechnicalNotes/NIST.TN.2161.pdf" \
  "NIST Technical Note 2161 (Engineering Laboratory)" \
  "NIST TN 2161" 2020 TN TN "NIST" \
  "equation-heavy;figure-heavy" "$PUBDOM" true

add_doc nist-pdf-0006 nist pdf \
  "https://nvlpubs.nist.gov/nistpubs/hb/2024/NIST.HB.44-2024.pdf" \
  "https://www.nist.gov/pml/owm/nist-handbook-44" \
  "Specifications, Tolerances, and Other Technical Requirements for Weighing and Measuring Devices" \
  "NIST Handbook 44 (2024)" 2024 HANDBOOK HANDBOOK "NIST" \
  "very-large;table-heavy;appendix-heavy" "$PUBDOM" true "" "rev-nist-hb44"

# --- NIST DOCX (CSRC) -----------------------------------------------------
add_doc nist-docx-0001 nist docx \
  "https://csrc.nist.gov/files/pubs/sp/800/18/r2/ipd/docs/sp800-18r2_system_security_plan_outline_example.docx" \
  "https://csrc.nist.gov/pubs/sp/800/18/r2/final" \
  "SP 800-18 Rev. 2 System Security Plan Outline Example" \
  "NIST SP 800-18 Rev. 2 (supporting file)" 2025 SP SP "NIST" \
  "form;table-heavy;deep-headings" "$PUBDOM" true "cf-nist-sp800-18r2" "rev-nist-sp800-18"

add_doc nist-docx-0002 nist docx \
  "https://csrc.nist.gov/files/pubs/sp/800/34/r1/upd1/final/docs/sp800-34-rev1_cp_template_high_impact_system.docx" \
  "https://csrc.nist.gov/pubs/sp/800/34/r1/upd1/final" \
  "SP 800-34 Rev. 1 Contingency Planning Template (High-Impact System)" \
  "NIST SP 800-34 Rev. 1 (supporting file)" 2010 SP SP "NIST" \
  "form;table-heavy;list-heavy" "$PUBDOM" true

# --- NIST EPUB (CSRC legacy e-books) --------------------------------------
add_doc nist-epub-0001 nist epub \
  "https://csrc.nist.gov/publications/nistpubs/800-115/sp800_115.epub" \
  "https://csrc.nist.gov/pubs/sp/800/115/final" \
  "Technical Guide to Information Security Testing and Assessment" \
  "NIST SP 800-115 (EPUB)" 2008 SP SP "NIST" \
  "simple;table-heavy" "$PUBDOM" true "cf-nist-sp800-115"

add_doc nist-epub-0002 nist epub \
  "https://csrc.nist.gov/publications/nistpubs/800-145/sp800_145.epub" \
  "https://csrc.nist.gov/pubs/sp/800/145/final" \
  "The NIST Definition of Cloud Computing" \
  "NIST SP 800-145 (EPUB)" 2011 SP SP "NIST" \
  "simple" "$PUBDOM" true

echo "pilot selection complete; corpus: $CORPUS"
if [ -n "$FAILED" ]; then
    echo "failed ids:$FAILED" >&2
    exit 1
fi
