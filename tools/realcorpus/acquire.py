#!/usr/bin/env python3
# real100-v1 corpus acquisition harness (stdlib only; runs in the pinned
# `realcorpus` Docker service, never on the host).
#
# It does one thing and does it honestly: given a source URL and the
# pre-performance metadata for one document, it downloads the bytes, verifies
# that the download is the *expected* format by inspecting the bytes themselves
# (never the extension or the server's Content-Type), records the SHA-256 and
# byte length, writes the file into the corpus tree when redistribution is
# appropriate, and appends one manifest row.
#
# Manifest schema (exactly 20 fields, frozen in real100-v1/README.md):
#   id agency title publication_id format source_url landing_page retrieved_utc
#   sha256 byte_len publication_year publication_family document_type
#   producer_or_origin_if_known size_class structural_tags
#   cross_format_family_id revision_family_id rights_status redistributable
#
# `manifest.tsv` is the canonical store (lossless, tab-separated). `manifest.toml`
# is a generated view of the same rows. Both are rewritten after every add so the
# two never drift.
#
# Selection discipline: this tool never chooses documents. The operator picks a
# document on pre-performance attributes only (agency, format, series, year,
# size, structure, cross-format/revision availability). No VOLE runtime result,
# ingest outcome, byte count from the codec, or baseline number may influence a
# keep/drop decision, and a document is never substituted because it exposed a
# loss. Once selected, the document is frozen by SHA-256.
#
# Subcommands:
#   add           acquire one URL -> one manifest row (+ committed file)
#   verify        re-check every manifest row (hash + length + format)
#   fetch-missing download bytes for rows whose file is absent (then verify)
#   list          print the manifest as TSV

import argparse
import hashlib
import os
import shutil
import subprocess
import sys
import tempfile
import zipfile
from datetime import datetime, timezone

FIELDS = [
    "id", "agency", "title", "publication_id", "format", "source_url",
    "landing_page", "retrieved_utc", "sha256", "byte_len", "publication_year",
    "publication_family", "document_type", "producer_or_origin_if_known",
    "size_class", "structural_tags", "cross_format_family_id",
    "revision_family_id", "rights_status", "redistributable",
]

FORMATS = ("pdf", "docx", "epub")
AGENCIES = ("nasa", "nist")

# Files larger than this are never read whole into memory by the format probe;
# the probe only ever reads the small header + central directory.
_UA = ("vole-document-realcorpus/1.0 (corpus acquisition harness; "
       "+https://github.com/vole-document)")


def err(msg):
    print("error: %s" % msg, file=sys.stderr)
    return 1


def now_utc():
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sha256_file(path, chunk=1 << 20):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while True:
            b = f.read(chunk)
            if not b:
                break
            h.update(b)
    return h.hexdigest()


# ---------------------------------------------------------------------------
# Format detection by bytes.
# ---------------------------------------------------------------------------
def detect_format(path):
    """Return 'pdf' | 'docx' | 'epub' | None, from the bytes only.

    PDF: the spec permits up to 1024 bytes of leading junk before `%PDF-`
    (several NTRS legacy scans carry a short archive wrapper that embeds the
    original filename), so the header is located, not assumed to be at offset
    0. The OPC (ZIP) families must start with the local-file header.
    """
    try:
        with open(path, "rb") as f:
            head = f.read(1024)
    except OSError:
        return None
    if b"%PDF-" in head[:1024]:
        return "pdf"
    if head[:4] == b"PK\x03\x04":
        return _detect_opc(path)
    return None


def _detect_opc(path):
    """Distinguish the ZIP-based OPC families (DOCX, EPUB) by their members."""
    try:
        with zipfile.ZipFile(path) as z:
            names = set(z.namelist())
            # EPUB: a `mimetype` member holding `application/epub+zip`, or the
            # OCF container descriptor. Check the mimetype first (cheap).
            if "mimetype" in names:
                try:
                    mt = z.read("mimetype")[:64]
                except (KeyError, zipfile.BadZipFile):
                    mt = b""
                if b"epub" in mt.lower():
                    return "epub"
            if "META-INF/container.xml" in names:
                return "epub"
            if "word/document.xml" in names:
                return "docx"
            if "[Content_Types].xml" in names:
                try:
                    ct = z.read("[Content_Types].xml")[:8192].lower()
                except (KeyError, zipfile.BadZipFile):
                    ct = b""
                if b"wordprocessingml" in ct:
                    return "docx"
                if b"epub" in ct:
                    return "epub"
    except (zipfile.BadZipFile, OSError):
        return None
    return None


def size_class(byte_len):
    kib = 1024
    mib = 1024 * 1024
    if byte_len < 100 * kib:
        return "<100KiB"
    if byte_len < 1 * mib:
        return "100KiB-1MiB"
    if byte_len < 10 * mib:
        return "1-10MiB"
    if byte_len < 50 * mib:
        return "10-50MiB"
    if byte_len < 100 * mib:
        return "50-100MiB"
    return ">100MiB"


# ---------------------------------------------------------------------------
# Download.
# ---------------------------------------------------------------------------
def download(url, dest, timeout=900):
    cmd = [
        "curl", "-fL",
        "--retry", "3", "--retry-all-errors", "--retry-delay", "2",
        "--connect-timeout", "30", "--max-time", str(timeout),
        "-A", _UA,
        "-o", dest,
        "-w", "%{http_code}\t%{size_download}\t%{url_effective}",
        url,
    ]
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError("curl failed (rc=%d) for %s: %s"
                           % (r.returncode, url, r.stderr.strip()[:300]))
    parts = r.stdout.strip().split("\t")
    return {
        "http_code": parts[0] if parts else "",
        "size_download": parts[1] if len(parts) > 1 else "",
        "url_effective": parts[2] if len(parts) > 2 else url,
    }


# ---------------------------------------------------------------------------
# Manifest I/O.
# ---------------------------------------------------------------------------
def manifest_paths(corpus):
    return (
        os.path.join(corpus, "manifest.tsv"),
        os.path.join(corpus, "manifest.toml"),
    )


def read_rows(corpus):
    tsv, _ = manifest_paths(corpus)
    if not os.path.exists(tsv):
        return []
    rows = []
    with open(tsv, "r", encoding="utf-8") as f:
        header = f.readline().rstrip("\n").split("\t")
        if header != FIELDS:
            raise RuntimeError("manifest.tsv header does not match the frozen "
                               "schema: %r" % (header,))
        for line in f:
            line = line.rstrip("\n")
            if not line:
                continue
            vals = line.split("\t")
            if len(vals) != len(FIELDS):
                raise RuntimeError("manifest.tsv row has %d fields, want %d: %r"
                                   % (len(vals), len(FIELDS), line[:120]))
            rows.append(dict(zip(FIELDS, vals)))
    return rows


def _sanitize(v):
    """TSV is the canonical store, so no field may contain a tab or newline."""
    return (v or "").replace("\t", " ").replace("\n", " ").replace("\r", " ").strip()


def write_manifests(corpus, rows, meta):
    tsv, toml = manifest_paths(corpus)
    # Preserve a freeze marker across regeneration: a frozen corpus is terminal,
    # so rewriting the generated view must not silently unfreeze it.
    frozen_lines = []
    if os.path.exists(toml):
        try:
            with open(toml, encoding="utf-8") as pf:
                frozen_lines = [ln.rstrip("\n") for ln in pf
                                if ln.startswith(("frozen = ", "frozen_utc = ",
                                                  "documents = "))]
        except OSError:
            frozen_lines = []
    os.makedirs(corpus, exist_ok=True)
    with open(tsv, "w", encoding="utf-8") as f:
        f.write("\t".join(FIELDS) + "\n")
        for row in rows:
            f.write("\t".join(_sanitize(row.get(k, "")) for k in FIELDS) + "\n")
    with open(toml, "w", encoding="utf-8") as f:
        f.write("# %s — real100-v1 corpus manifest (generated; do not edit by hand)\n" % meta.get("corpus", corpus))
        f.write("# Canonical store is manifest.tsv; this TOML is a view of it.\n")
        f.write("# Regenerate with: tools/realcorpus/verify.sh --write-manifests\n")
        f.write('schema_version = 1\n')
        f.write('corpus = %s\n' % toml_str(meta.get("corpus", "")))
        f.write('generated_utc = %s\n' % toml_str(now_utc()))
        for line in frozen_lines:
            f.write(line + "\n")
        if meta.get("note"):
            f.write('note = %s\n' % toml_str(meta["note"]))
        f.write("\n")
        for row in rows:
            f.write("[[document]]\n")
            for k in FIELDS:
                v = row.get(k, "")
                if k == "byte_len":
                    f.write("%s = %d\n" % (k, int(v or 0)))
                elif k == "publication_year":
                    if v.isdigit():
                        f.write("%s = %d\n" % (k, int(v)))
                    else:
                        f.write("%s = %s\n" % (k, toml_str(v)))
                elif k == "structural_tags":
                    tags = [t for t in (v or "").split(";") if t]
                    f.write("%s = [%s]\n"
                            % (k, ", ".join(toml_str(t) for t in tags)))
                elif k == "redistributable":
                    f.write("%s = %s\n" % (k, "true" if v == "true" else "false"))
                else:
                    f.write("%s = %s\n" % (k, toml_str(v)))
            f.write("\n")


def toml_str(s):
    out = ['"']
    for ch in (s or ""):
        if ch == "\\":
            out.append("\\\\")
        elif ch == '"':
            out.append('\\"')
        elif ch == "\n":
            out.append("\\n")
        elif ch == "\r":
            out.append("\\r")
        elif ch == "\t":
            out.append("\\t")
        elif ord(ch) < 0x20:
            out.append("\\u%04x" % ord(ch))
        else:
            out.append(ch)
    out.append('"')
    return "".join(out)


# ---------------------------------------------------------------------------
# Paths for a row's bytes.
# ---------------------------------------------------------------------------
def doc_path(corpus, row):
    """Committed location for redistributable bytes."""
    return os.path.join(corpus, "documents", row["agency"], row["format"],
                        "%s.%s" % (row["id"], row["format"]))


def cache_path(corpus, row):
    """On-demand (gitignored) location for non-redistributable bytes."""
    return os.path.join(corpus, ".cache", row["agency"], row["format"],
                        "%s.%s" % (row["id"], row["format"]))


def existing_path(corpus, row):
    for p in (doc_path(corpus, row), cache_path(corpus, row)):
        if os.path.exists(p):
            return p
    return None


# ---------------------------------------------------------------------------
# Commands.
# ---------------------------------------------------------------------------
def cmd_add(args):
    rows = read_rows(args.corpus)
    if any(r["id"] == args.id for r in rows):
        return err("id already present in manifest: %s" % args.id)
    if args.format not in FORMATS:
        return err("--format must be one of %s" % (FORMATS,))
    if args.agency not in AGENCIES:
        return err("--agency must be one of %s" % (AGENCIES,))

    redistributable = (args.redistributable == "true")
    os.makedirs(args.corpus, exist_ok=True)
    tmp = tempfile.NamedTemporaryFile(prefix="rc-", suffix=".part", delete=False)
    tmp.close()
    try:
        info = download(args.url, tmp.name, timeout=args.timeout)
        detected = detect_format(tmp.name)
        if detected is None:
            return err("downloaded bytes are not a recognized PDF/DOCX/EPUB "
                       "(url=%s, effective=%s, %s bytes)"
                       % (args.url, info["url_effective"], info["size_download"]))
        if detected != args.format:
            return err("format mismatch for %s: expected %s, bytes are %s "
                       "(refusing to record a mislabelled document)"
                       % (args.url, args.format, detected))
        digest = sha256_file(tmp.name)
        nbytes = os.path.getsize(tmp.name)

        row = {
            "id": args.id,
            "agency": args.agency,
            "title": args.title,
            "publication_id": args.publication_id,
            "format": args.format,
            "source_url": args.url,
            "landing_page": args.landing_page,
            "retrieved_utc": now_utc(),
            "sha256": digest,
            "byte_len": str(nbytes),
            "publication_year": str(args.year),
            "publication_family": args.family,
            "document_type": args.document_type,
            "producer_or_origin_if_known": args.producer,
            "size_class": args.size_class or size_class(nbytes),
            "structural_tags": args.structural_tags,
            "cross_format_family_id": args.cross_format_family_id,
            "revision_family_id": args.revision_family_id,
            "rights_status": args.rights_status,
            "redistributable": "true" if redistributable else "false",
        }
        for prev in rows:
            if prev["sha256"] == digest:
                print("note: identical bytes already recorded as id=%s"
                      % prev["id"], file=sys.stderr)

        if redistributable:
            dest = doc_path(args.corpus, row)
            os.makedirs(os.path.dirname(dest), exist_ok=True)
            shutil.move(tmp.name, dest)
            committed = dest
        elif args.keep_private:
            dest = cache_path(args.corpus, row)
            os.makedirs(os.path.dirname(dest), exist_ok=True)
            shutil.move(tmp.name, dest)
            committed = dest
        else:
            committed = None
        rows.append(row)
        write_manifests(args.corpus, rows,
                        {"corpus": args.corpus_name or os.path.basename(args.corpus)})
        print("added %s  %s  %d bytes  sha256=%s" %
              (row["id"], row["format"], nbytes, digest[:16] + "…"))
        print("  bytes: %s" % (committed if committed else "(not stored; redistributable=false)"))
        return 0
    finally:
        if os.path.exists(tmp.name):
            os.unlink(tmp.name)


def _verify_row(corpus, row, fetch_missing, timeout):
    path = existing_path(corpus, row)
    fetched = False
    if path is None:
        if not fetch_missing:
            return "MISSING", None
        dest = cache_path(corpus, row)
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        try:
            download(row["source_url"], dest, timeout=timeout)
        except RuntimeError as e:
            return "FETCH-FAIL(%s)" % str(e)[:60], None
        path = dest
        fetched = True
    problems = []
    nbytes = os.path.getsize(path)
    if str(nbytes) != str(row["byte_len"]):
        problems.append("len %s!=%s" % (nbytes, row["byte_len"]))
    digest = sha256_file(path)
    if digest != row["sha256"]:
        problems.append("sha256 %s!=%s" % (digest[:12], row["sha256"][:12]))
    detected = detect_format(path)
    if detected != row["format"]:
        problems.append("format %s!=%s" % (detected, row["format"]))
    if problems:
        return "FAIL(" + ";".join(problems) + ")", path
    return ("OK(fetched)" if fetched else "OK"), path


def cmd_verify(args):
    rows = read_rows(args.corpus)
    if not rows:
        print("manifest is empty: %s" % args.corpus)
        return 0
    bad = 0
    for row in rows:
        status, path = _verify_row(args.corpus, row, args.fetch_missing,
                                   args.timeout)
        print("%-14s %-10s %s" % (status, row["id"], os.path.relpath(path, args.corpus) if path else row["source_url"]))
        if not status.startswith("OK"):
            bad += 1
    print("verified %d rows, %d not OK" % (len(rows), bad))
    if bad and not args.allow_missing:
        return 1
    return 0


def cmd_fetch_missing(args):
    args.fetch_missing = True
    args.allow_missing = False
    return cmd_verify(args)


def cmd_list(args):
    rows = read_rows(args.corpus)
    print("\t".join(FIELDS))
    for row in rows:
        print("\t".join(row.get(k, "") for k in FIELDS))
    return 0


def cmd_init(args):
    rows = read_rows(args.corpus)
    write_manifests(args.corpus, rows,
                    {"corpus": args.corpus_name or os.path.basename(args.corpus),
                     "note": args.note})
    sums = os.path.join(args.corpus, "SHA256SUMS")
    if not os.path.exists(sums):
        with open(sums, "w", encoding="utf-8") as f:
            f.write("")
    print("initialized %s (%d rows)" % (args.corpus, len(rows)))
    return 0


def _merge_tags(cur, add):
    out = [t for t in cur.split(";") if t]
    for t in add.split(";"):
        if t and t not in out:
            out.append(t)
    return ";".join(out)


def cmd_retag(args):
    """Merge probe-derived objective structural tags into every row.

    Only objective, byte-grounded tags are merged (scanned, figure-heavy,
    complex-xref, very-large for PDF; table/image/heading/list for DOCX;
    multi-chapter/image-heavy/nav-heavy for EPUB). Curated tags are preserved.
    """
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    import probe  # local module, read-only byte probe
    rows = read_rows(args.corpus)
    probes = {"pdf": probe.probe_pdf, "docx": probe.probe_docx,
              "epub": probe.probe_epub}
    changed = 0
    for r in rows:
        p = existing_path(args.corpus, r)
        if not p:
            print("skip (no bytes): %s" % r["id"])
            continue
        fmt = probe.detect(p)
        if fmt is None:
            print("skip (unrecognized): %s" % r["id"])
            continue
        facts = probes[fmt](p)
        facts["format"] = fmt
        sug = probe.suggest(facts)
        new = _merge_tags(r["structural_tags"], sug)
        if new != r["structural_tags"]:
            print("%-16s %s -> %s" % (r["id"], r["structural_tags"], new))
            r["structural_tags"] = new
            changed += 1
    write_manifests(args.corpus, rows,
                    {"corpus": args.corpus_name or os.path.basename(args.corpus)})
    print("retagged %d rows" % changed)
    return 0


def cmd_set_tags(args):
    rows = read_rows(args.corpus)
    hit = False
    for r in rows:
        if r["id"] == args.id:
            if args.add:
                cur = [t for t in r["structural_tags"].split(";") if t]
                for t in args.tags.split(";"):
                    if t and t not in cur:
                        cur.append(t)
                r["structural_tags"] = ";".join(cur)
            else:
                r["structural_tags"] = args.tags
            hit = True
            print("%s structural_tags = %s" % (r["id"], r["structural_tags"]))
    if not hit:
        return err("id not found: %s" % args.id)
    write_manifests(args.corpus, rows, {"corpus": args.corpus_name or os.path.basename(args.corpus)})
    return 0


def cmd_add_tsv(args):
    """Acquire every row of a selection TSV (header = the manifest fields).

    Parsing is done here rather than in the shell: a tab is IFS-whitespace, so
    POSIX `read` collapses the empty optional fields (tags, cross, revision) and
    misaligns the row. csv.DictReader treats the tab as a plain delimiter.
    """
    import csv
    ok = skip = fail = updated = 0
    with open(args.selection, encoding="utf-8") as f:
        reader = csv.DictReader(f, delimiter="\t")
        rows = list(reader)
    for row in rows:
        present = {r["id"]: r for r in read_rows(args.corpus)}
        if row["id"] in present:
            if args.update_tags:
                all_rows = read_rows(args.corpus)
                changed = False
                for rr in all_rows:
                    if rr["id"] != row["id"]:
                        continue
                    if row.get("tags", ""):
                        merged = _merge_tags(rr["structural_tags"], row["tags"])
                        if merged != rr["structural_tags"]:
                            rr["structural_tags"] = merged
                            changed = True
                    for fld, key in (("cross_format_family_id", "cross"),
                                     ("revision_family_id", "rev")):
                        if row.get(key, "") and rr[fld] != row[key]:
                            rr[fld] = row[key]
                            changed = True
                if changed:
                    write_manifests(args.corpus, all_rows,
                                    {"corpus": args.corpus_name or os.path.basename(args.corpus)})
                    updated += 1
            print("skip (present): %s" % row["id"])
            skip += 1
            continue
        ns = argparse.Namespace(
            corpus=args.corpus, id=row["id"], agency=row["agency"],
            format=row["format"], url=row["url"],
            landing_page=row.get("landing", ""), title=row.get("title", ""),
            publication_id=row.get("pubid", ""), year=row.get("year", ""),
            family=row.get("family", ""),
            document_type=row.get("doctype", ""),
            producer=row.get("producer", ""), size_class="",
            structural_tags=row.get("tags", ""),
            cross_format_family_id=row.get("cross", ""),
            revision_family_id=row.get("rev", ""),
            rights_status=row.get("rights", ""),
            redistributable=row.get("redist", "true") or "true",
            keep_private=False, corpus_name=args.corpus_name or "",
            timeout=args.timeout)
        try:
            rc = cmd_add(ns)
        except Exception as e:  # noqa: BLE001
            print("FAILED (recorded, continuing): %s: %s"
                  % (row["id"], str(e)[:200]), file=sys.stderr)
            rc = 1
        if rc == 0:
            ok += 1
        else:
            print("FAILED (recorded, continuing): %s" % row["id"],
                  file=sys.stderr)
            fail += 1
    print("acquire: %d added, %d skipped, %d failed, %d tags updated"
          % (ok, skip, fail, updated))
    return 0


def build_parser():
    p = argparse.ArgumentParser(
        description="real100-v1 corpus acquisition harness (stdlib only)")
    sub = p.add_subparsers(dest="cmd", required=True)

    def add_corpus(sp):
        sp.add_argument("--corpus", required=True,
                        help="corpus root (e.g. real100-v1 or real100-v1/pilot)")

    a = sub.add_parser("add")
    add_corpus(a)
    a.add_argument("--id", required=True)
    a.add_argument("--agency", required=True, choices=AGENCIES)
    a.add_argument("--format", required=True, choices=FORMATS)
    a.add_argument("--url", required=True)
    a.add_argument("--landing-page", default="")
    a.add_argument("--title", default="")
    a.add_argument("--publication-id", default="")
    a.add_argument("--year", default="")
    a.add_argument("--family", default="")
    a.add_argument("--document-type", default="")
    a.add_argument("--producer", default="")
    a.add_argument("--size-class", default="")
    a.add_argument("--structural-tags", default="")
    a.add_argument("--cross-format-family-id", default="")
    a.add_argument("--revision-family-id", default="")
    a.add_argument("--rights-status", default="")
    a.add_argument("--redistributable", choices=("true", "false"), default="true")
    a.add_argument("--keep-private", action="store_true",
                   help="cache bytes on disk for a redistributable=false row "
                        "(the .cache dir is gitignored)")
    a.add_argument("--corpus-name", default="")
    a.add_argument("--timeout", type=int, default=900)
    a.set_defaults(func=cmd_add)

    v = sub.add_parser("verify")
    add_corpus(v)
    v.add_argument("--fetch-missing", action="store_true")
    v.add_argument("--allow-missing", action="store_true")
    v.add_argument("--timeout", type=int, default=900)
    v.set_defaults(func=cmd_verify)

    f = sub.add_parser("fetch-missing")
    add_corpus(f)
    f.add_argument("--timeout", type=int, default=900)
    f.set_defaults(func=cmd_fetch_missing)

    l = sub.add_parser("list")
    add_corpus(l)
    l.set_defaults(func=cmd_list)

    i = sub.add_parser("init")
    add_corpus(i)
    i.add_argument("--corpus-name", default="")
    i.add_argument("--note", default="")
    i.set_defaults(func=cmd_init)

    t = sub.add_parser("set-tags")
    add_corpus(t)
    t.add_argument("--id", required=True)
    t.add_argument("--tags", required=True)
    t.add_argument("--add", action="store_true",
                   help="merge into existing tags instead of replacing")
    t.add_argument("--corpus-name", default="")
    t.set_defaults(func=cmd_set_tags)

    rt = sub.add_parser("retag")
    add_corpus(rt)
    rt.add_argument("--corpus-name", default="")
    rt.set_defaults(func=cmd_retag)

    at = sub.add_parser("add-tsv")
    add_corpus(at)
    at.add_argument("--selection", required=True)
    at.add_argument("--corpus-name", default="")
    at.add_argument("--update-tags", action="store_true")
    at.add_argument("--timeout", type=int, default=900)
    at.set_defaults(func=cmd_add_tsv)

    return p


def main(argv=None):
    args = build_parser().parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
