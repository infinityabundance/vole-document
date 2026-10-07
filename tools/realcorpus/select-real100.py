#!/usr/bin/env python3
# real100-v1 selection generator (stdlib only; runs in the pinned `realcorpus`
# service). It produces `real100-v1/sources/real100_selection.tsv`, the frozen,
# reviewable list of the 100 documents, from three pre-collected candidate
# pools. It never downloads and never runs the codec: selection is by
# pre-performance attributes only (agency, format, series/stiType, year, size,
# cross-format/revision availability).
#
# Determinism: pools are sorted; ties break on id. Re-running reproduces the
# same selection. The output TSV is the record of *what was selected and why*;
# `select-real100.sh` performs the acquisition.
#
#   docker compose run --rm --no-TTY realcorpus \
#     python3 tools/realcorpus/select-real100.py
#
# Output columns:
#   id agency format url landing title pubid year family doctype producer
#   tags cross rev rights redist

import collections
import os
import re
import sys

ROOT = "/work/real100-v1"
SRC = os.path.join(ROOT, "sources")
OUT = os.path.join(SRC, "real100_selection.tsv")
RIGHTS = "US-Gov-Public-Domain"

NIST = "https://nvlpubs.nist.gov/nistpubs"
CSRC = "https://csrc.nist.gov"

ERAS = [("pre1960", 0, 1959, 5), ("1960-79", 1960, 1979, 7),
        ("1980-99", 1980, 1999, 8), ("2000-14", 2000, 2014, 8),
        ("2015-24", 2015, 2024, 8), ("2025-26", 2025, 2026, 4)]
DOC_QUOTA = {"TM": 8, "TR": 8, "CR": 4, "CONFERENCE": 4, "SP": 2, "NACA": 5}

# --- NASA e-book cross-format pages ---------------------------------------
# Chosen by pre-performance attributes only: upload year (the era proxy the
# e-book PDF carries) and landing-page category. Ten pages are cross-paired with
# their official PDF; the two 2025 uploads put those e-book PDFs in the 2025-26
# era band and the eight 2015-24 uploads land in 2015-24, leaving the older era
# bands to NTRS technical publications.
CROSS_PAGES = [
    # 2025 uploads (e-book PDFs -> 2025-26 era)
    "https://www.nasa.gov/history/history-publications-and-resources/nasa-history-series/a-history-of-near-earth-objects-research/",
    "https://www.nasa.gov/history/ascension/",
    # 2015-2024 uploads (e-book PDFs -> 2015-24 era)
    "https://www.nasa.gov/missions/station/a-researchers-guide-to-cellular-biology/",
    "https://www.nasa.gov/missions/station/a-researchers-guide-to-acceleration-environment/",
    "https://www.nasa.gov/missions/station/a-researchers-guide-to-combustion-science/",
    "https://www.nasa.gov/aeronautics/beyond-tube-and-wing/",
    "https://www.nasa.gov/ebooks/earth-at-night/",
    "https://www.nasa.gov/ebooks/earth/",
    "https://www.nasa.gov/ebooks/earth-as-art/",
    "https://www.nasa.gov/ebooks/counting-the-many-ways-the-space-station-benefits-humankind/",
]
# Five further EPUB-only pages chosen for category spread (history/photo/guide/aero).
EXTRA_EPUB_PAGES = [
    "https://www.nasa.gov/history/history-publications-and-resources/nasa-history-series/50-years-of-solar-system-exploration-historical-perspectives/",
    "https://www.nasa.gov/aeronautics/a-new-twist-in-flight-research/",
    "https://www.nasa.gov/missions/station/a-researchers-guide-to-fundamental-physics/",
    "https://www.nasa.gov/history/history-publications-and-resources/nasa-history-series/archaeology-anthropology-and-interstellar-communication/",
    "https://www.nasa.gov/science-research/for-researchers/a-researchers-guide-to-earth-observations/",
]

# --- curated NTRS technical publications (30) -----------------------------
# Selected by pre-performance attributes only: era, doctype, centre and the
# candidate's measured byte size (read-only HEAD probe, sources/ntrs_sizes.tsv).
#   5 pre-1960 NACA (3 in the >100 MiB band, 2 in 50-100 MiB)
#   7 for 1960-79  (6 TR + 1 CR which is 50-100 MiB)
#   8 for 1980-99  (3 TM + 3 CR + 1 conference + 1 SP handbook)
#   8 for 2000-14  (3 TM + 3 conference + 1 SP + 1 CR)
#   2 for 2025-26  (2 TM)
CURATED_NTRS = [
    "19930091112", "20050019296", "20050019295", "19710070068", "19930091058",
    "19670002631", "19630009859", "19680022079", "19980223593",
    "19660028846", "19660023256", "19760013038",
    "19830022035", "19890006474", "19820003180",
    "19900001638", "19970035961", "19970014917",
    "19960051323", "19960002194",
    "20110007108", "20020082935", "20020039536",
    "20010024890", "20030005526", "20020067708",
    "20080008301", "20050192421",
    "20260002490", "20250004035",
]


def load_tsv(path, has_header=True):
    rows = []
    with open(path, encoding="utf-8") as f:
        lines = [l.rstrip("\n") for l in f if l.strip()]
    if has_header:
        lines = lines[1:]
    for l in lines:
        rows.append(l.split("\t"))
    return rows


def classify_ntrs(report_number, sti_type, year):
    rn = (report_number or "").upper()
    st = (sti_type or "").upper()
    if "NACA" in rn or (year and year < 1960):
        return "NACA"
    if re.search(r"[-/ ]TM[-/ ]|/TM[ /]|TM-X", rn):
        return "TM"
    if re.search(r"[-/ ]TN[-/ ]|[-/ ]TR[-/ ]|/TN[ /]", rn):
        return "TR"
    if re.search(r"[-/ ]CR[-/ ]|/CR[ /]", rn):
        return "CR"
    if re.search(r"[-/ ]SP[-/ ]|/SP[ /]|^SP-", rn) or st == "SPECIAL_PUBLICATION":
        return "SP"
    if st.startswith("CONFERENCE"):
        return "CONFERENCE"
    if st in ("CONTRACTOR_REPORT", "CONTRACTOR_OR_GRANTEE_REPORT"):
        return "CR"
    if st == "TECHNICAL_MEMORANDUM":
        return "TM"
    return "OTHER"


def era_of(year):
    for name, lo, hi, _ in ERAS:
        if lo <= year <= hi:
            return name
    return None


def curate_nasa(doctype, year, title, center):
    """Pre-performance structural tags for a NASA PDF.

    Derived from the series (doctype) and title keywords only -- never from any
    codec result. Objective facts (scanned via scan-conversion producer,
    figure-heavy, complex-xref, very-large) are added later by probe.py's
    `retag`, which reads the downloaded bytes.
    """
    tl = (title or "").lower()
    t = []
    if doctype == "NACA":
        t += ["scanned", "legacy-image"]
    elif doctype == "CR":
        t += ["scanned", "legacy-image"] if (year and year < 1990) else ["table-heavy"]
    elif doctype == "TM":
        t += ["scanned", "legacy-image"] if (year and year < 2000) else ["table-heavy"]
    elif doctype == "TR":
        t += ["figure-heavy", "table-heavy"]
    elif doctype == "CONFERENCE":
        t += ["multi-column", "figure-heavy"]
    elif doctype == "SP":
        t += ["appendix-heavy", "reference-heavy", "born-digital"]
    if any(k in tl for k in ("aerodynam", "aeroacoustic", "fluid", "cfd",
                             "computational", "orbit", "trajector", "thermal",
                             "structural", "physics", "combustion", "turbulen",
                             "heat transfer", "equation")):
        t.append("equation-heavy")
    if any(k in tl for k in ("handbook", "reference", "guide")):
        t += ["appendix-heavy", "reference-heavy"]
    if any(k in tl for k in ("data", "test", "measurement", "instrument")):
        t.append("table-heavy")
    if any(k in tl for k in ("multi-column", "proceedings", "conference")):
        t.append("multi-column")
    if center and center not in ("Legacy CDMS",):
        t.append("center:%s" % center.lower().split()[0])
    return ";".join(dict.fromkeys(t))


def curate_epub(cat, title):
    tl = (title or "").lower()
    t = []
    if cat == "history":
        t = ["multi-chapter", "reference-heavy"]
    elif cat == "photo":
        t = ["image-heavy"]
    elif cat == "guide":
        t = ["simple", "nav-heavy"]
    elif cat == "aero":
        t = ["multi-chapter"]
    if "index" in tl or "reference" in tl or "dictionary" in tl:
        t.append("reference-heavy")
    if "night" in tl or "art" in tl:
        t.append("unusual")
    return ";".join(dict.fromkeys(t))


class Sel:
    def __init__(self):
        self.rows = []

    def add(self, id, agency, fmt, url, landing, title, pubid, year, family,
            doctype, producer, tags, cross, rev, redist="true"):
        self.rows.append([id, agency, fmt, url, landing, title.replace("\t", " "),
                          pubid, str(year), family, doctype, producer, tags,
                          cross, rev, RIGHTS, redist])


def nasa_epub_pages():
    rows = load_tsv(os.path.join(SRC, "nasa_ebook_assets.tsv"))
    pages = collections.defaultdict(dict)  # page -> {kind: url}
    titles = {}
    for kind, title, page, url in rows:
        pages[page][kind] = url
        titles[page] = title
    return pages, titles


def category(page):
    if "/nasa-history-series/" in page:
        return "history"
    if "/ebooks/earth" in page:
        return "photo"
    if "researcher" in page.lower():
        return "guide"
    if "/aeronautics/" in page:
        return "aero"
    return "other"


def pick_epubs(pages):
    groups = collections.defaultdict(list)
    epub_pages = [p for p in pages if "epub" in pages[p]]
    for page in epub_pages:
        groups[category(page)].append(page)
    for g in groups:
        groups[g].sort()
    want = {"history": 4, "photo": 3, "guide": 4, "aero": 2, "other": 2}
    chosen = []
    for g in ("history", "photo", "guide", "aero", "other"):
        chosen += groups[g][:want[g]]
    # top up to 15 from anything remaining
    pool = sorted(epub_pages)
    for p in pool:
        if len(chosen) >= 15:
            break
        if p not in chosen:
            chosen.append(p)
    return chosen[:15]


def year_from_url(url):
    m = re.search(r"/uploads/(\d{4})/", url)
    return int(m.group(1)) if m else ""


def build():
    sel = Sel()
    pages, titles = nasa_epub_pages()

    # 1) NASA EPUB (15): 10 cross-format e-books followed by 5 EPUB-only pages.
    cross_pages = list(CROSS_PAGES)
    extra_pages = list(EXTRA_EPUB_PAGES)
    for p in cross_pages + extra_pages:
        if p not in pages or "epub" not in pages[p] or "pdf" not in pages[p]:
            raise SystemExit("missing EPUB/PDF page in pool: %s" % p)
    cross_ids = {}
    for i, p in enumerate(cross_pages, 1):
        cross_ids[p] = "cf-nasa-ebook-%02d" % i

    for i, p in enumerate(cross_pages + extra_pages, 1):
        sel.add("nasa-epub-%04d" % i, "nasa", "epub", pages[p]["epub"], p,
                titles[p], "NASA eBook", year_from_url(pages[p]["epub"]),
                "NASA-EBOOK", "SP", "NASA", curate_epub(category(p), titles[p]),
                cross_ids.get(p, ""), "")

    # 2) NASA PDF (40) = 10 paired e-book PDFs + 30 curated NTRS technical
    #    publications. The NTRS list is curated by pre-performance attributes
    #    only (era, doctype, centre, measured byte size), never by a codec
    #    result; see CURATED_NTRS.
    ebooks = []
    for p in cross_pages:
        url = pages[p]["pdf"]
        ebooks.append({"url": url, "landing": p, "title": titles[p],
                       "pubid": os.path.basename(url),
                       "year": year_from_url(pages[p]["epub"]),
                       "doctype": "EBOOK", "center": "", "cross": cross_ids[p]})

    ntrs = {}
    for pid, year, sti, center, rn, title, pdf, land in load_tsv(
            os.path.join(SRC, "nasa_ntrs_candidates.tsv")):
        y = int(year) if year.isdigit() else None
        ntrs[pid] = {"id": pid, "year": y, "sti": sti, "center": center,
                     "rn": rn, "title": title, "url": pdf, "landing": land,
                     "doctype": classify_ntrs(rn, sti, y)}
    missing = [pid for pid in CURATED_NTRS if pid not in ntrs]
    if missing:
        raise SystemExit("curated NTRS ids not in pool: %s" % missing)
    picks = [ntrs[pid] for pid in CURATED_NTRS]

    for i, e in enumerate(ebooks, 1):
        sel.add("nasa-pdf-eb-%02d" % i, "nasa", "pdf", e["url"], e["landing"],
                e["title"], e["pubid"], e["year"], "NASA-EBOOK", e["doctype"],
                "NASA", "", e["cross"], "")
    for i, c in enumerate(picks, 1):
        tags = curate_nasa(c["doctype"], c["year"], c["title"], c["center"])
        sel.add("nasa-pdf-%04d" % i, "nasa", "pdf", c["url"], c["landing"],
                c["title"], c["rn"] or c["id"], c["year"] or "", "NASA-NTRS",
                c["doctype"], c["center"] or "NASA", tags, "", "")

    build_nist(sel)
    build_nist_docx(sel)
    build_nist_epub(sel)
    return sel


def build_nist(sel):
    # (pubid, year, family, url, landing, title)
    docs = [
        ("NIST SP 800-115", 2008, "SP",
         NIST + "/Legacy/SP/nistspecialpublication800-115.pdf",
         CSRC + "/pubs/sp/800/115/final",
         "Technical Guide to Information Security Testing and Assessment", ""),
        ("NIST SP 800-123", 2009, "SP",
         NIST + "/Legacy/SP/nistspecialpublication800-123.pdf",
         CSRC + "/pubs/sp/800/123/final",
         "Guide to General Server Security", ""),
        ("NIST SP 800-30 Rev. 1", 2012, "SP",
         NIST + "/Legacy/SP/nistspecialpublication800-30r1.pdf",
         CSRC + "/pubs/sp/800/30/r1/final",
         "Guide for Conducting Risk Assessments", ""),
        ("NIST SP 800-144", 2011, "SP",
         NIST + "/Legacy/SP/nistspecialpublication800-144.pdf",
         CSRC + "/pubs/sp/800/144/final",
         "Guidelines on Security and Privacy in Public Cloud Computing", ""),
        ("NIST SP 800-18 Rev. 2", 2025, "SP",
         NIST + "/SpecialPublications/NIST.SP.800-18r2.pdf",
         CSRC + "/pubs/sp/800/18/r2/final",
         "Guide for Developing Security Plans for Federal Information Systems", "rev-nist-sp800-18"),
        ("NISTIR 8206", 2018, "NISTIR",
         NIST + "/ir/2018/NIST.IR.8206.pdf",
         CSRC + "/pubs/nistir/8206/final",
         "Digital Investigation Techniques: A NIST Scientific Foundation Review", ""),
        ("NISTIR 8176", 2017, "NISTIR",
         NIST + "/ir/2017/NIST.IR.8176.pdf",
         CSRC + "/pubs/nistir/8176/final",
         "Bluetooth Security for Low Energy Devices", ""),
        ("NISTIR 8286", 2020, "NISTIR",
         NIST + "/ir/2020/NIST.IR.8286.pdf",
         CSRC + "/pubs/nistir/8286/final",
         "Integrating Cybersecurity and Enterprise Risk Management (ERM)", ""),
        ("NISTIR 8286A", 2021, "NISTIR",
         NIST + "/ir/2021/NIST.IR.8286A.pdf",
         CSRC + "/pubs/nistir/8286a/final",
         "Identifying and Estimating Cybersecurity Risk for Enterprise Risk Management", "rev-nistir-8286"),
        ("NIST TN 2161", 2020, "TN",
         NIST + "/TechnicalNotes/NIST.TN.2161.pdf",
         NIST + "/TechnicalNotes/NIST.TN.2161.pdf",
         "NIST Technical Note 2161", ""),
        ("NIST TN 2188", 2024, "TN",
         NIST + "/TechnicalNotes/NIST.TN.2188.pdf",
         NIST + "/TechnicalNotes/NIST.TN.2188.pdf",
         "NIST Technical Note 2188", ""),
        ("NIST TN 2077", 2016, "TN",
         NIST + "/TechnicalNotes/NIST.TN.2077.pdf",
         NIST + "/TechnicalNotes/NIST.TN.2077.pdf",
         "NIST Technical Note 2077", ""),
        ("NIST Handbook 44 (2024)", 2024, "HANDBOOK",
         NIST + "/hb/2024/NIST.HB.44-2024.pdf",
         "https://www.nist.gov/pml/owm/nist-handbook-44",
         "Specifications, Tolerances, and Other Technical Requirements for Weighing and Measuring Devices", "rev-nist-hb44"),
        ("NIST Handbook 44 (2023)", 2023, "HANDBOOK",
         NIST + "/hb/2023/NIST.HB.44-2023.pdf",
         "https://www.nist.gov/pml/owm/nist-handbook-44",
         "Specifications, Tolerances, and Other Technical Requirements for Weighing and Measuring Devices (2023)", "rev-nist-hb44"),
        ("NIST Handbook 130 (2024)", 2024, "HANDBOOK",
         NIST + "/hb/2024/NIST.HB.130-2024.pdf",
         "https://www.nist.gov/pml/owm/nist-handbook-130",
         "Uniform Laws and Regulations in the Areas of Legal Metrology and Engine Fuel Quality", "rev-nist-hb130"),
        ("NIST FIPS 140-3", 2019, "FIPS",
         NIST + "/FIPS/NIST.FIPS.140-3.pdf",
         CSRC + "/pubs/fips/140-3/final",
         "Security Requirements for Cryptographic Modules", "rev-nist-fips-140"),
        ("NIST FIPS 140-2", 2001, "FIPS",
         NIST + "/FIPS/NIST.FIPS.140-2.pdf",
         CSRC + "/pubs/fips/140-2/upd2/final",
         "Security Requirements for Cryptographic Modules (FIPS 140-2)", "rev-nist-fips-140"),
        ("NIST CSWP 29 (CSF 2.0)", 2024, "OTHER",
         NIST + "/CSWP/NIST.CSWP.29.pdf",
         "https://www.nist.gov/cyberframework",
         "The NIST Cybersecurity Framework (CSF) 2.0", ""),
        ("NIST AI 100-1", 2023, "OTHER",
         NIST + "/ai/NIST.AI.100-1.pdf",
         CSRC + "/pubs/ai/100/1/final",
         "Artificial Intelligence Risk Management Framework (AI RMF 1.0)", "rev-nist-ai-100"),
        ("NIST AI 600-1", 2024, "OTHER",
         NIST + "/ai/NIST.AI.600-1.pdf",
         CSRC + "/pubs/ai/600/1/final",
         "Artificial Intelligence Risk Management Framework: Generative AI Profile", "rev-nist-ai-600"),
    ]
    cross_map = {
        "NIST SP 800-115": "cf-nist-sp800-115",
        "NIST SP 800-123": "cf-nist-sp800-123",
        "NIST SP 800-30 Rev. 1": "cf-nist-sp800-30r1",
        "NIST SP 800-144": "cf-nist-sp800-144",
        "NIST SP 800-18 Rev. 2": "cf-nist-sp800-18r2",
        "NIST FIPS 140-2": "cf-nist-fips140-2",
    }
    for i, (pubid, year, fam, url, landing, title, rev) in enumerate(docs, 1):
        sel.add("nist-pdf-%04d" % i, "nist", "pdf", url, landing, title, pubid,
                year, fam, fam, "NIST", "", cross_map.get(pubid, ""), rev)


def curate_docx(title):
    tl = title.lower()
    t = []
    if any(k in tl for k in ("template", "outline", "form", "certificate")):
        t.append("form")
        t.append("unusual")
    if "roles and responsibilities" in tl:
        t += ["deep-headings", "procedure-heavy"]
    if "deltas" in tl:
        t.append("table-heavy")
    if "index" in tl:
        t.append("index")
    if "bia template" in tl:
        t.append("procedure-heavy")
    return ";".join(dict.fromkeys(t))


def build_nist_docx(sel):
    docs = [
        ("NIST SP 800-18 Rev. 2 (SSP outline example)", 2025, "SP",
         CSRC + "/files/pubs/sp/800/18/r2/ipd/docs/sp800-18r2_system_security_plan_outline_example.docx",
         "cf-nist-sp800-18r2", "rev-nist-sp800-18"),
        ("NIST SP 800-18 Rev. 2 (privacy plan outline example)", 2025, "SP",
         CSRC + "/files/pubs/sp/800/18/r2/final/docs/sp800-18r2_system_privacy_plan_outline_example.docx",
         "cf-nist-sp800-18r2", "rev-nist-sp800-18"),
        ("NIST SP 800-18 Rev. 2 (roles and responsibilities)", 2025, "SP",
         CSRC + "/files/pubs/sp/800/18/r2/final/docs/sp800-18r2_roles_and_responsibilities.docx",
         "cf-nist-sp800-18r2", "rev-nist-sp800-18"),
        ("NIST SP 800-18 Rev. 2 (C-SCRM plan outline example)", 2025, "SP",
         CSRC + "/files/pubs/sp/800/18/r2/final/docs/sp800-18r2_c-scrm_plan_outline_example.docx",
         "cf-nist-sp800-18r2", "rev-nist-sp800-18"),
        ("NIST SP 800-34 Rev. 1 (CP template, high impact)", 2010, "SP",
         CSRC + "/files/pubs/sp/800/34/r1/upd1/final/docs/sp800-34-rev1_cp_template_high_impact_system.docx",
         "cf-nist-sp800-34r1", "rev-nist-sp800-34"),
        ("NIST SP 800-34 Rev. 1 (CP template, moderate impact)", 2010, "SP",
         CSRC + "/files/pubs/sp/800/34/r1/upd1/final/docs/sp800-34-rev1_cp_template_moderate_impact_system.docx",
         "cf-nist-sp800-34r1", "rev-nist-sp800-34"),
        ("NIST SP 800-34 Rev. 1 (CP template, low impact)", 2010, "SP",
         CSRC + "/files/pubs/sp/800/34/r1/upd1/final/docs/sp800-34-rev1_cp_template_low_impact_system.docx",
         "cf-nist-sp800-34r1", "rev-nist-sp800-34"),
        ("NIST SP 800-34 Rev. 1 (BIA template)", 2010, "SP",
         CSRC + "/files/pubs/sp/800/34/r1/upd1/final/docs/sp800-34-rev1_bia_template.docx",
         "cf-nist-sp800-34r1", "rev-nist-sp800-34"),
        ("NIST SP 800-53 Rev. 5 (collaboration index template)", 2020, "SP",
         CSRC + "/files/pubs/sp/800/53/r5/upd1/final/docs/sp800-53-collaboration-index-template.docx",
         "cf-nist-sp800-53r5", "rev-nist-sp800-53"),
        ("NIST SP 800-88 Rev. 1 (sample certificate of sanitization)", 2014, "SP",
         CSRC + "/files/pubs/sp/800/88/r1/final/docs/sample-certificate-of-sanitization.docx",
         "cf-nist-sp800-88r1", "rev-nist-sp800-88"),
        ("NIST SP 800-218 (deltas from draft to final)", 2022, "SP",
         CSRC + "/files/pubs/sp/800/218/final/docs/800-218-deltas-from-draft-to-final.docx",
         "cf-nist-sp800-218", "rev-nist-sp800-218"),
        ("NIST SP 800-218 (deltas from working paper to final)", 2022, "SP",
         CSRC + "/files/pubs/sp/800/218/final/docs/800-218-deltas-from-wp-to-final.docx",
         "cf-nist-sp800-218", "rev-nist-sp800-218"),
        ("NIST Sample Chain-of-Custody Form", 2017, "OTHER",
         "https://www.nist.gov/system/files/documents/2017/04/28/Sample-Chain-of-Custody-Form.docx",
         "", ""),
        ("NIST CFReDS Data Leakage Case (answers)", "", "OTHER",
         "https://cfreds-archive.nist.gov/data_leakage_case/leakage-answers.docx",
         "", ""),
        ("NIST CFReDS Mobile Device Data Population (Android Z970)", "", "OTHER",
         "https://s3.amazonaws.com/cftt.cfreds.nist.gov/cfreds/mobile/CHIPOFF/ZTE+Z970/Android+Z970_Mobile+Device+Data+Population.docx",
         "", ""),
    ]
    for i, (pubid, year, fam, url, cross, rev) in enumerate(docs, 1):
        sel.add("nist-docx-%04d" % i, "nist", "docx", url, url, pubid, pubid,
                year, fam, fam, "NIST", curate_docx(pubid), cross, rev)


def build_nist_epub(sel):
    base = CSRC + "/publications/nistpubs"
    docs = [
        ("800-115", "800-115/sp800_115.epub", 2008, "Technical Guide to Information Security Testing and Assessment", "cf-nist-sp800-115"),
        ("800-122", "800-122/sp800_122.epub", 2010, "Guide to Protecting the Confidentiality of Personally Identifiable Information", ""),
        ("800-123", "800-123/sp800_123.epub", 2009, "Guide to General Server Security", "cf-nist-sp800-123"),
        ("800-30-rev1", "800-30-rev1/sp800_30_r1.epub", 2012, "Guide for Conducting Risk Assessments", "cf-nist-sp800-30r1"),
        ("800-127", "800-127/sp800_127.epub", 2010, "Guide to Bluetooth Security", ""),
        ("800-133", "800-133/sp800_133.epub", 2012, "Recommendation for Cryptographic Key Generation", ""),
        ("800-144", "800-144/sp800_144.epub", 2011, "Guidelines on Security and Privacy in Public Cloud Computing", "cf-nist-sp800-144"),
        ("800-145", "800-145/sp800_145.epub", 2011, "The NIST Definition of Cloud Computing", ""),
        ("800-146", "800-146/sp800_146.epub", 2012, "Cloud Computing Synopsis and Recommendations", ""),
        ("FIPS 140-2", "https://csrc.nist.gov/files/pubs/fips/140-2/upd2/final/docs/fips140_2_chng2_20021203.epub", 2001, "Security Requirements for Cryptographic Modules (FIPS 140-2)", "cf-nist-fips140-2"),
    ]
    for i, (num, path, year, title, cross) in enumerate(docs[:10], 1):
        url = path if path.startswith("http") else "%s/%s" % (base, path)
        tags = ""
        short = {"800-123", "800-145", "800-84", "800-92", "800-127"}
        if num == "800-115":
            tags = "unusual;table-heavy;reference-heavy;figure-heavy"
        elif num == "FIPS 140-2":
            tags = "figure-heavy;reference-heavy;table-heavy"
        elif num in short:
            tags = "simple;nav-heavy"
        else:
            tags = "reference-heavy;figure-heavy"
        if num == "FIPS 140-2":
            pubid, fam = "NIST FIPS 140-2 (EPUB)", "FIPS"
        else:
            pubid, fam = "NIST SP %s (EPUB)" % num, "SP"
        sel.add("nist-epub-%04d" % i, "nist", "epub", url, url,
                title, pubid, year, fam, fam, "NIST", tags, cross, "")


def main():
    sel = build()
    with open(OUT, "w", encoding="utf-8") as f:
        f.write("id\tagency\tformat\turl\tlanding\ttitle\tpubid\tyear\tfamily\t"
                "doctype\tproducer\ttags\tcross\trev\trights\tredist\n")
        for r in sel.rows:
            f.write("\t".join(r) + "\n")
    comp = collections.Counter((r[1], r[2]) for r in sel.rows)
    print("wrote %d selected documents to %s" % (len(sel.rows), OUT))
    for k in sorted(comp):
        print("  %-5s %-5s %d" % (k[0], k[1], comp[k]))
    dt = collections.Counter(r[9] for r in sel.rows if r[1] == "nasa" and r[2] == "pdf")
    print("  nasa-pdf doctypes:", dict(dt))
    eras = collections.Counter()
    for r in sel.rows:
        if r[1] == "nasa" and r[2] == "pdf" and r[7].isdigit():
            n = era_of(int(r[7]))
            if n:
                eras[n] += 1
    print("  nasa-pdf eras:", dict(eras))
    return 0


if __name__ == "__main__":
    sys.exit(main())
