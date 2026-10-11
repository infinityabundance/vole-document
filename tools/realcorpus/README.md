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
| `formats-manifest.tsv` | A pinned, checksummed set of **52 real-world samples across 14 Wave-2 formats** (XLSX, PPTX, ODS, ODP, JSON, YAML, CSV, Markdown, XML, HTML, TOML, JSONL, EML, Parquet) from stable, permissively licensed public sources (Apache POI, odfpy, Natural Earth, Prometheus/Alertmanager, CommonMark, Apache Maven, MDN, serde/cargo, parquet-testing, openai-cookbook, CPython, jsonlines), pinned by tag/commit and by SHA-256, with a curated `complexity` stratum. Bytes are gitignored. |
| `hostile-strata.tsv` | A pinned **malformed/hostile** stratum derived deterministically from pinned real bytes (truncation, wrong-magic, mid-structure corruption); each derived sample is pinned by SHA-256 so the derivation is falsifiable. |
| `fetch-formats.sh` | Downloads + verifies the format manifest into gitignored `realformats-v1/documents/<format>/<id>` (`--check` verifies only). Never trusts the network: a length/SHA-256 mismatch fails. Runs in the `realcorpus` service. |

The stratified smoke court for those samples is `tools/realformats-smoke-court.sh`
(runs in `doc-baseline`); it reports **pooled costs alongside medians**, stratified
by format and size class.

### Phase 26.5 — stratified population v2 (larger scale)

| File | Role |
|---|---|
| `formats-manifest-v2.tsv` | A **larger** pinned population: **284 real samples across 41 formats** (OOXML + legacy binary office `.doc`/`.xls`/`.ppt`/`.msg`, ODF, structured text, prose, tabular/analytical, messaging, geospatial, scientific/container, and the Wave-2 families), from stable permissively-licensed public sources pinned by tag/commit + SHA-256. Supersedes the 52-sample v1 manifest (kept as the historical control). |
| `hostile-strata-v2.tsv` | An extended **malformed/hostile** stratum (24 deterministic truncations / wrong-magic / dropped-tail derivations of pinned v2 bytes), each pinned by SHA-256. |
| `sources/formats-v2-candidates.tsv` | The curated (id, format, url, source, license, redistributable, notes, complexity) selection the manifest is built from. |
| `discover-formats-v2.py` | Read-only discovery helper (GitHub git-trees API) that enumerated the candidate files. |
| `build-formats-v2.py` | The workhorse: `select` (from a discovery snapshot) → `merge` (with the v1 control rows) → `build` (download + SHA-256) → `hostile` (derive + pin the hostile stratum). |
| `findings-v2-reconciliation.tsv` | The per-(format, detected) resolution of every detection finding, rendered into the court receipt. |
| `fetch-formats.sh` | Unchanged; run it with `MANIFEST=tools/realcorpus/formats-manifest-v2.tsv` to acquire/verify the v2 population. |

The v2 court is `tools/realformats-stratified-v2-court.sh` (runs in `analytical`,
RELEASE VOLE). It asserts byte-exact closure for every sample, records the
detector's choice, reports pooled-and-median costs per declared format/stratum,
and lists every detection **finding** with its reconciliation. It bounds its
receipt (per-sample store blobs are not committed).

```sh
docker compose run --rm --no-TTY realcorpus sh -c 'MANIFEST=tools/realcorpus/formats-manifest-v2.tsv sh tools/realcorpus/fetch-formats.sh'
docker compose run --rm --no-TTY analytical sh tools/realformats-stratified-v2-court.sh
```

The stratified smoke court for those samples is `tools/realformats-smoke-court.sh`
(runs in `doc-baseline`); it reports **pooled costs alongside medians**, stratified
by format and size class.

The full **stratified real-world court** is `tools/realformats-stratified-court.sh`
(runs in `analytical`, RELEASE VOLE); it ingests all 52 real samples plus the 8
hostile derivations, asserts byte-exact closure from a deleted scratch copy in a
fresh process, and reports **pooled costs alongside medians** per format, per size
class, per complexity, and per stratum (real vs hostile), separated by **workload**
(narrow `observe --metadata` vs full `materialize --exact`).

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

# Phase 21.5.3 (FIX 4): acquire the pinned real multi-format samples, then run the
# stratified smoke court (pooled costs alongside medians) in the doc-baseline lane.
docker compose run --rm --no-TTY realcorpus sh tools/realcorpus/fetch-formats.sh
docker compose run --rm --no-TTY doc-baseline bash tools/realformats-smoke-court.sh

# Phase 21.15 (ITEM 2): acquire the 52-sample 14-format set, then run the full
# stratified real-world court (pooled vs median per format/stratum/workload) in
# the RELEASE analytical lane.
docker compose run --rm --no-TTY realcorpus sh tools/realcorpus/fetch-formats.sh
docker compose run --rm --no-TTY analytical bash tools/realformats-stratified-court.sh
```

## Format verification is by bytes

`acquire.py` refuses to record a document whose bytes are not the declared
format, so a mislabelled extension or a rewritten Content-Type cannot enter the
manifest. PDF detection scans the first 1024 bytes for `%PDF-` (several NTRS
legacy scans carry a leading wrapper); DOCX/EPUB are distinguished by their OPC
members (`word/document.xml` vs `mimetype`/`META-INF/container.xml`).
