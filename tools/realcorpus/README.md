# tools/realcorpus — real100-v1 acquisition harness

Pinned, memory-capped, Docker-only tooling for acquiring the `real100-v1` corpus.
Nothing here runs on the host; everything runs in the `realcorpus` service:

```sh
docker compose run --rm --no-TTY realcorpus <command>
```

The service is `FROM debian:bookworm-slim@sha256:3783cc…` (the same digest as the
`tools` service) plus `curl`, `ca-certificates`, `python3` (3.11.2),
`poppler-utils` and `qpdf`, and is capped (`mem_limit == memswap_limit`, hard
`pids_limit`, `cpus`) exactly like every other lane.

## Components

| File | Role |
|---|---|
| `acquire.py` | The workhorse. `add` downloads one URL, verifies its **bytes** are the expected format (PDF header within the first 1024 bytes; OPC ZIP members for DOCX/EPUB), records provenance + SHA-256 + byte length, and appends one manifest row. `add-tsv` runs a whole selection (idempotent; `--update-tags` syncs curated tags/families onto present rows). `verify` re-checks every row (hash + length + format). `fetch-missing` downloads absent bytes on demand. `retag` merges probe-derived objective tags. `init`, `list`, `set-tags` round it out. |
| `probe.py` | Read-only byte probe (Poppler/qpdf + stdlib). Prints pages, text length, image/font inventory, object/xref streams, DOCX tables/headings/lists/images, EPUB spine/media, and a set of objective suggested tags. |
| `check-diversity.py` | Evaluates all 80 diversity gates from a manifest and prints a PASS/PARTIAL/FAIL compliance table. Reads pre-performance attributes only. |
| `discover-nasa-ebooks.py` | Read-only: enumerates NASA e-book landing pages and their PDF/EPUB assets → `sources/nasa_ebook_assets.tsv`. |
| `discover-ntrs.py` | Read-only: queries the NTRS search API → `sources/nasa_ntrs_candidates.tsv`. |
| `select-real100.py` | Deterministically builds the frozen selection → `sources/real100_selection.tsv`. |
| `select-real100.sh` / `select-pilot.sh` | Acquisition drivers over the selection / pilot list. |
| `verify.sh` | Gate: verifies every row, regenerates `SHA256SUMS`, optionally prints the diversity table. Exit non-zero on any hash/length/format mismatch. |

## Typical use

```sh
# verify everything acquired (regenerates SHA256SUMS, prints diversity)
docker compose run --rm --no-TTY realcorpus sh tools/realcorpus/verify.sh --diversity

# fetch bytes for redistributable=false rows, then verify
docker compose run --rm --no-TTY realcorpus \
  sh tools/realcorpus/verify.sh --corpus real100-v1 --fetch-missing

# acquire a single document
docker compose run --rm --no-TTY realcorpus tools/realcorpus/fetch.sh \
  --corpus real100-v1 --id nasa-pdf-0001 --agency nasa --format pdf \
  --url https://… --title "…" --year 1998 --family NASA-NTRS \
  --document-type TM --rights-status US-Gov-Public-Domain --redistributable true

# print the gate evaluator's help
docker compose run --rm --no-TTY realcorpus python3 tools/realcorpus/check-diversity.py --help
```

## Format verification is by bytes

`acquire.py` refuses to record a document whose bytes are not the declared
format, so a mislabelled extension or a rewritten Content-Type cannot enter the
manifest. PDF detection scans the first 1024 bytes for `%PDF-` (several NTRS
legacy scans carry a leading wrapper); DOCX/EPUB are distinguished by their OPC
members (`word/document.xml` vs `mimetype`/`META-INF/container.xml`).
