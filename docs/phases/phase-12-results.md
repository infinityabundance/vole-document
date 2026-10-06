# Phase 12 — results

Branch: `phase12`. This is a **cumulative** results document; each section names
its commit under test and links its sealed receipt. Everything here was measured
in the pinned Docker services; nothing ran on the host.

## Cross-document reuse (12.8)

Subphase 12.8 asks for genuine, honestly scoped cross-document procedural state
under exact content identity (ADR-0034): same bytes → same content id → one
stored blob/node, scored as **work** and as a **representation fact**, never as a
byte fraction.

**Receipt:** `evidence/campaigns/2026-10-06-phase12-share-e7ef693/`
(`SUMMARY.md`, `commands.txt`, `environment.json`, `raw/` incl. `metrics.json`),
produced by `tools/phase12-share-court.sh` in the pinned `dev` service over a
**randomized** cohort (resource bytes read from `/dev/urandom`; seed recorded in
`raw/metrics.json`). Court driver: `examples/phase12_share_court.rs`.

### What is genuinely shared

* A byte-identical resource embedded in a **DOCX** and an **EPUB** resolves to
  **one** `ResourceBlob` content id (verified `contains_node`), i.e. one stored
  blob and one decoded member, with no second keying scheme. The randomized
  shared blob id is recorded in `raw/metrics.json`.
* The second document writes **0 bytes** for the shared blob — a representation
  fact, reported as `nodes_id_shared` and kept distinct from work reuse
  (`nodes_reused`).
* Work reuse is receipted: the observed `retained_inverse_work_fraction` was
  **0.3399** for the sharing EPUB (cold `4` node executions / `12217` cold input
  bytes → warm `2` / `8065`), computed from node executions + cold input bytes,
  never from source size.
* Exactness holds for every root in the receipt:
  `materialize_exact(root) == source` (length **and** SHA-256 **and** byte
  identity) for the DOCX, the sharing EPUB, and the control EPUB.

### What is *not* shared (recorded, not hidden)

* The Phase-11 **PDF** adapter extracts **no** embedded resource blob, so
  PDF↔DOCX resource sharing does not exist in this architecture. The court and
  the 12.8 integration court assert the reported `resource_blob_nodes == 0` for
  a PDF embedding the same bytes; a genuine cross-format share is demonstrated
  **DOCX↔EPUB** instead.
* Raw DEFLATE members and per-source span records are **not** content identity
  and are never claimed as sharing.
* The no-sharing control (a structurally identical EPUB embedding different
  bytes in a fresh store) shares no resource, shares no node id, and
  deduplicates zero bytes.

### Honest scope

`seed_bytes_saved_on_second_document` (measured `4258` bytes: the blob + decoded
seed-node records not rewritten) is a **seed-node record** fact, **not** an
overall store-size saving: each document's exact source descriptor is stored
independently and still contains the resource bytes. A shared blob pays its own
byte cost for the first document and is scored as state/work, so **no
compression claim** is made. This is the modest, real reuse the landed
architecture supports; it is recorded as measured, not manufactured.

## Cross-format equivalence (12.9)

Subphase 12.9 asks whether one common observation vocabulary over three
formats yields the **same logical answer with different native provenance**,
without inventing coordinates a format does not have (plan §DEC-5).

**Receipt:** `evidence/campaigns/2026-10-06-phase12-triplet-dc4d5a3/`
(`SUMMARY.md`, `commands.txt`, `environment.json`, `raw/` incl. `ground_truth.json`
and `assertions.tsv`). Generator: `tools/fixtures/doc-triplet-gen.py` (Python
stdlib only, `producers` image). Court: `tools/phase12-triplet-court.sh`
(`db-baseline`). Library mirror: `tests/phase12_courts.rs`.

One known logical report — title, three headings, two marked paragraphs, an
ordered list, an 8×2 table whose `B7` cell is `bravo-seven`, one link, and one
shared image resource — was emitted deterministically as `report.pdf`,
`report.docx` and `report.epub` plus a `ground_truth.json`. All three were
ingested through the **one** format-agnostic pipeline and queried through the
**one** common CLI. Result: **96/96 assertions passed**.

### Same logical answer, different native provenance

| observation | PDF | DOCX | EPUB |
|---|---|---|---|
| `find "XF12A"` (marker) | yes | yes | yes |
| whole-document text | yes | yes | yes |
| `metadata` | structural descriptor | story structure | package summary |
| heading | typed decline | Introduction/Method/Findings | Introduction/Method/Findings |
| block/paragraph | typed decline | yes | yes |
| Table / Cell(B7) | typed decline | `bravo-seven` | `bravo-seven` |
| Resource (the one image) | typed decline | `rel=rIdImg` | `href=images/image1.png` (`image/png`) |
| Link | typed decline | text `external` | `href=https://example.com/phase12` |

Every supported answer carries `format=<fmt>;common;<native>` provenance
(asserted for each answer), so the equivalence is produced by different native
adapters over distinct byte representations — a WordprocessingML projection, a
bounded-XHTML projection, and the heuristic PDF text-runs extractor. Exactness
is untouched: `materialize --exact` matches every source by length, SHA-256 and
`cmp` for all three formats.

### Honest gaps (recorded, not approximated)

* **No cross-format document-title observation.** The common vocabulary has no
  title selector: PDF `metadata` returns the structural field descriptor
  (`source_len`/`source_sha256`), DOCX `metadata` returns the story structure,
  and the EPUB `metadata` representation (the only one the capability set
  admits) is the package summary without the `dc:title` array. The title is
  present in the sources and ground truth but is **not** claimed as a common
  observation.
* **PDF has no heading/table/cell/resource/link coordinate.** Those common
  selectors return a typed capability error (exit `6`), asserted for each.
* **PDF `search-match` provenance carries no native suffix** beyond the adapter
  tag (`format=pdf;common;`); PDF has no native search coordinate to name.

## Source removal (12.10)

Subphase 12.10 asks whether each format is still queryable and byte-exactly
rematerializable **after the source is deleted, in a fresh process**.

**Receipt:** `evidence/campaigns/2026-10-06-phase12-removal-dc4d5a3/`
(`SUMMARY.md`, `commands.txt`, `environment.json`, `raw/` incl. `assertions.tsv`).
Court: `tools/phase12-removal-court.sh` (`db-baseline`). Library mirror:
`tests/phase12_courts.rs`.

For each of PDF, DOCX and EPUB the court ingested from a private copy, recorded
the oracle length + SHA-256, then **deleted the source and the ingest
descriptor**. Every common and native query and the `materialize --exact` ran in
a **new process** against the field store alone. Result: **38/38 assertions
passed**; the rematerialized bytes matched the recorded oracle by length,
SHA-256 and `cmp` for all three formats. The receipt keeps a sealed oracle copy
of each source for `cmp`; the store never references it.

### Honest scope

This demonstrates source independence and restart survival for the triplet —
not compression, and not a claim about arbitrary documents. The Phase-12.11
lifetime and Phase-12.12 LLM working-set courts carry the cost claims.
