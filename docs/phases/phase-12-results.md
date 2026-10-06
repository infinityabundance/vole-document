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

## Lifetime workload (12.11)

Subphase 12.11 asks the plan's headline question: on a pre-registered mixed
PDF/DOCX/EPUB workload, does the Phase-12 VOLE field answer repeatedly with a
lower **cumulative lifetime cost** than direct per-query tooling (**A0**) and a
competent one-time-preprocessed, **source-retaining** SQLite+FTS5 cache (**A1**)?

**Receipt:** `evidence/campaigns/2026-10-06-phase12-lifetime-3eaf576/`
(`receipt.json`, `SUMMARY.md`, `commands.txt`, `gates.txt`, `raw/` incl.
`case_bytes.tsv`, `cumulative.jsonl`, `assertions.tsv`, `ground_truth.json`).
Court: `tools/phase12-lifetime-court.sh` in the pinned `doc-baseline` service.
The schedule was frozen in `tools/fixtures/phase12-lifetime-schedule.json` and
**committed before measurement** (`3eaf576`); the receipt records dirty `""` at
run time. Corpus: `tools/fixtures/phase12-corpus-gen.py` — 12 deterministic
documents (alpha/bravo/charlie/delta × PDF/DOCX/EPUB); `alpha` is byte-identical
to the 12.9 triplet. `delta` is deliberately large and incompressible
(~60 KB source, ~160 KB extracted text) so the court *can* lose.

Accounting (ADR-0027/0035): one-time costs charged in full; per-query bytes are
the process `read`+`pread64` total under `strace` for **all three** systems (one
boundary); VOLE's instrumented `ObserveStats.bytes_read` is reported separately
and never summed with process reads; four persistent universes reported
separately; cold + warm passes; every case answered and asserted (all comparable
assertions passed; only pre-registered declines remained).

### Crossover (warm pass)

| document | metric | V wins vs A0 | V wins vs A1 | A1 leads from |
|---|---|---|---|---|
| alpha/bravo/charlie (all formats) | wall / CPU / bytes | all N | all N | never |
| delta.docx | bytes | all N | N=1 only | **N=10** |
| delta.docx | CPU | all N | N=1..10 | **N=100** |
| delta.docx | wall | all N | N=1..100 | **N=1000** |
| delta.epub | bytes | all N | N=1..10 | **N=100** |
| delta.epub | CPU / wall | all N | N=1..100 | **N=1000** |
| delta.pdf | bytes | never | N=1..100 | **N=1000** |
| delta.pdf | wall / CPU | all N | all N | never |

So the result is genuinely mixed: VOLE owns the whole small-document frontier and
the cold one-time comparison, while the source-retaining SQLite baseline takes
over the **large** documents' byte frontier from N=10–1000 and their wall/CPU
frontier at N=1000. This is exactly the admissible "A1 wins simple lookups"
region ADR-0035 requires to exist.

### Cumulative cost at N=1 and N=1000 (warm)

| document | N | V wall·cpu·read | A1 wall·cpu·read | A0 wall·cpu·read |
|---|---:|---|---|---|
| alpha.docx | 1 | 2.9 ms · 0 ms · 39,938 B | 24.1 · 10 · 1,851,105 | 20.2 · 10 · 1,627,234 |
| alpha.docx | 1000 | 771 · 950 · 15.0 MB | 1089 · 1270 · 29.3 MB | 16724 · 16750 · 1357 MB |
| alpha.pdf | 1000 | 705 · 870 · 8.9 MB | 936 · 1100 · 26.3 MB | 1423 · 1640 · 20.1 MB |
| delta.docx | 1 | 5.8 · 0 · 0.53 MB | 40.1 · 30 · 2.77 MB | 27.6 · 20 · 1.74 MB |
| **delta.docx** | **1000** | **1457 · 1640 · 331 MB** | **1085 · 1260 · 68 MB** | 22815 · 22970 · 1459 MB |
| **delta.epub** | **1000** | **1163 · 1350 · 211 MB** | **1142 · 1340 · 67.6 MB** | 20708 · 20670 · 1310 MB |
| delta.pdf | 1000 | 895 · 1060 · 66.1 MB | 974 · 1130 · 55.9 MB | 17601 · 17840 · 43.8 MB |

One-time: VOLE `encode`+`field-ingest` 2–23 ms (read 19 KB–138 KB); A1
extraction+SQLite 23–66 ms (read 1.8–2.7 MB, dominated by the Python stdlib
extractor's interpreter startup, charged in full). Persistent bytes: on the small
documents VOLE's store (7.7–25 KB) is smaller than A1's `.db` (77 KB); on `delta`
VOLE's store (462–902 KB) is **larger** than A1's `.db` (221–823 KB).

### All recorded losses (VOLE does not win these)

* **A1 wins the large-document byte frontier.** `delta.docx` bytes from **N=10**,
  `delta.epub` from **N=100**, `delta.pdf` from **N=1000**; at N=1000 the
  source-retaining baseline reads 68 MB / 67.6 MB / 55.9 MB where VOLE reads
  331 MB / 211 MB / 66.1 MB, because VOLE reads the *full descriptor* (and its
  warm derived cache) on every observation.
* **A1 wins the large-document wall and CPU frontier at N=1000** on
  `delta.docx` (1085 ms / 1260 ms vs VOLE 1457 / 1640) and `delta.epub`
  (1142 / 1340 vs 1163 / 1350).
* **VOLE's one-time read is 2.3×–22.6× the source** on every document
  (`alpha.pdf` 21.2×, `charlie.pdf` 22.6×), and its store exceeds A1's `.db` on
  all three `delta` documents.
* **VOLE declines DOCX exact resource bytes.** `observe --resource 0 --kind
  decoded` is unsupported for DOCX (typed capability error); A0/A1 answer the
  member bytes. (EPUB and PDF exact bytes are answered and asserted.)
* **PDF has no native provenance.** The `native-provenance` observation returns
  the empty string for PDF page text (the 12.9 gap); it is VOLE-only for
  DOCX/EPUB and not a capability A1 can answer at all.
* **The `search` lane uses substring semantics.** FTS5 is built and `ANALYZE`d,
  but VOLE `find` is a lexical *substring* match, so A1 answered the same query
  with `LIKE` — a tokenizer would miss `XF13A` glued to adjacent glyphs by
  Poppler's text reordering. Whole-token FTS5/BM25 remains a real A1 capability,
  reported separately rather than compared on different semantics.
* A0/A1 cannot answer the VOLE-only surfaces (native provenance; PDF structure,
  preview, object/decoded-stream bytes), so those are excluded from the lifetime
  frontier and are not claimed as lifetime wins.

## LLM working set (12.12)

Subphase 12.12 compares the token working set handed to a model for the same
pre-registered question per document: **B0** the whole extracted document,
**B1** the local page/story/spine extract, and **V** the VOLE observation.

**Receipt:** `evidence/campaigns/2026-10-06-phase12-llm-3eaf576/`
(`receipt.json`, `SUMMARY.md`, `commands.txt`, `gates.txt`, `raw/`).
Court: `tools/phase12-llm-court.sh` in the pinned `llm-workingset` service.

**Tokenizer (pinned, offline, verified at court time):** `bert-base-uncased`
(WordPiece, vocab 30522), HuggingFace `tokenizers==0.20.3`, from the vendored
asset `tools/tokenizers/bert-base-uncased.tokenizer.json` with SHA-256
`ce64fce797c24f68df90b40a3f74f579b336a493db14bd583fd520ea0d8c9a98` checked before
any count; `add_special_tokens=false`. **No token number here is a claim about
any other tokenizer.** Questions: PDF "what is on page 1?"; DOCX "what is the
first body paragraph (block 1)?"; EPUB "what is the first spine paragraph
(block 1)?". B0/B1 for DOCX/EPUB use the named stdlib profiles
(`docx-story-txt-v1` / `docx-block-txt-v1`, `epub-spine-txt-v1` /
`epub-block-txt-v1`); for PDF, B0/B1 are Poppler whole-document / `-f1 -l1`.

| document | B0 tok | B1 tok | V tok | V vs B1 | V vs B0 | ctx waste B0/V | ctx waste tok |
|---|---:|---:|---:|---|---|---:|---:|
| alpha.pdf | 48 | 48 | 51 | loss | loss | 0.90 | 0.94 |
| bravo.pdf | 71 | 71 | 78 | loss | loss | 0.86 | 0.91 |
| charlie.pdf | 29 | 29 | 27 | win | win | 1.01 | 1.07 |
| delta.pdf | 34,319 | 34,319 | 38,853 | loss | loss | 0.66 | 0.88 |
| alpha.docx | 46 | 9 | 9 | tie | win | 6.5 | 5.1 |
| alpha.epub | 48 | 9 | 9 | tie | win | 6.6 | 5.3 |
| charlie.docx | 29 | 9 | 9 | tie | win | 2.6 | 3.2 |
| charlie.epub | 29 | 9 | 9 | tie | win | 2.6 | 3.2 |
| bravo.docx | 153 | 9 | 9 | tie | win | 18.0 | 17.0 |
| bravo.epub | 155 | 9 | 9 | tie | win | 18.2 | 17.2 |
| **delta.docx** | **91,257** | 10 | 10 | tie | **win** | **4305** | **9126** |
| **delta.epub** | **91,255** | 10 | 10 | tie | **win** | **4305** | **9126** |

**Verdict (`bert-base-uncased`).** V vs the local extract B1: **1 win / 8 tie /
3 loss**; V vs the whole document B0: **9 win / 0 tie / 3 loss**. Any reduction
over B1: `true`; on **all** questions: `false`.

### All recorded losses

* **PDF pages lose to Poppler.** On `alpha.pdf` (51 vs 48 tok), `bravo.pdf`
  (78 vs 71) and `delta.pdf` (38,853 vs 34,319) VOLE's bounded heuristic
  text-run projection is *larger* than Poppler's page extract — it is not
  Poppler reading order. `charlie.pdf` is a small win (27 vs 29).
* **DOCX/EPUB narrow answers tie, not win.** V's block-1 payload equals the
  named local extraction exactly (9–10 tokens), so there is no token saving on
  the local extract — the win is only against the whole document (B0), where the
  reduction is 2.6×–9126×.
* **A smaller V is a working-set measurement, never a text-quality claim**, and
  B0 == B1 for one-page PDFs (there is no separate "whole document" context).

---

Phase 12.11 and 12.12 are the phase's lifetime-cost evidence. The measured,
scoped verdict: VOLE wins the small-document lifetime frontier and the cold
one-time comparison, ties or loses PDF-page and simple-lookup surfaces, and
**loses the large-document frontier to the source-retaining SQLite baseline** at
N ≥ 10–1000 bytes and N = 1000 wall/CPU. Every loss is recorded above; a headline
that never lost would not have been credible.

## Security + fuzzing (12.13)

Subphase 12.13 asks whether the new ZIP/OPC/OCF/XML surfaces stay safe on
hostile input: **never panic, never build a filesystem path from a member name,
never fetch the network, never execute script, never exceed configured bounds
uncontrollably**, and either **return a typed error** or **preserve the exact
bytes and decline the observation**. The security contract is
`SECURITY.md`; the threat list is research I §1–§10 and plan §DEC-9/§123.

**Receipt:** `evidence/campaigns/2026-10-06-phase12-security-33f6d04/`
(`SUMMARY.md`, `commands.txt`, `environment.json`, `outcomes.tsv`,
`raw/assertions.tsv`, `fuzz-campaign/`). Generator:
`tools/fixtures/phase12-hostile-gen.py`. Courts:
`tools/phase12-security-court.sh` + `tests/phase12_security.rs`.

### Hostile corpus (deterministic, locally generated, tiny)

45 fixtures, **29,292 bytes total**, built from the Python standard library alone
(no third-party document bytes). The court regenerates the corpus and asserts it
is byte-identical to the committed `tests/fixtures/phase12-hostile/`
(`determinism: ok`). Coverage: truncated/bad-EOCD/bad-central-offset/
ZIP64-inconsistent/multi-disk/overlapping/malformed-descriptor ZIPs;
traversal/absolute/drive/backslash/NUL names; duplicate names; ratio and
declared-expansion bombs; bad CRC; encrypted and unknown-method members; empty
and prefix-stub archives; DOCTYPE/entity-bomb/deep-nesting/non-UTF-8/NUL XML;
malformed `[Content_Types].xml`; missing/ambiguous/external main relationship;
duplicate/traversal part names; malformed/missing/ambiguous package document;
malformed `container.xml`; spine inconsistency; scripted and remote XHTML;
encryption annotation; oversized text.

### Outcome classes (all 45 fixtures, `exact=yes`)

**Result: 315/315 court assertions passed.** Every fixture was
`materialize --exact == original` through both the opaque floor and the field
store; `net=clean` and `subproc=clean` for every `field-ingest` (strace showed
no `connect` and no foreign `execve`); no panic, no timeout, and no
`InternalInvariant` anywhere.

| class | count | meaning |
|---|---:|---|
| `reject` (typed) | 16 | a valid package whose identity/XML/DOCX/EPUB structure is broken; the derived model parse returns `InvalidPackageStructure`/`InvalidXmlStructure` **before** any semantics are guessed |
| `opaque-preserve-and-decline` (typed) | 29 | a raw malformed/hostile ZIP; the universal ingest falls back to the byte-exact opaque floor and the unsupported observation declines `UnsupportedFeature` |
| `accept` | 10 | a well-formed (if unusual) DOCX/EPUB the pipeline answers; scripted/remote/encrypted content is retained as inert data |

The split is exactly the plan §DEC-9 contract: **cover/identity broken → typed
reject; semantics/resource only → preserve exact bytes, decline typed.** Note the
architecture-level detail (recorded, not hidden): a raw ZIP whose *physical*
cover is broken (`z_truncated`, `z_overlap`, …) is **not** rejected by the
`field-ingest` verb — `is_zip` is a scan probe, so the input routes to the opaque
PDF lane and is preserved exactly. The typed **scanner** rejection
(`InvalidZipStructure`/`CoverageViolation`/`ResourceLimit`) is asserted directly
at the library layer in `tests/phase12_security.rs` (`zip_scan_is_cover_exact_and_typed`,
`structural_zip_faults_are_rejected_typed`, `name_hazards_are_rejected_and_never_become_paths`).
Both statements hold; neither is a substitute for the other.

### Library court (`tests/phase12_security.rs`, 15/15 pass)

Runs every fixture through `scan`, `build_opc_model`, `build_docx_model`,
`build_epub_model`, `detect_document_format`, and `capabilities_for_format`
under **both** `Limits::STRICT` and `Limits::DEFAULT`, plus direct XML-hardening
cases (DOCTYPE, NUL, non-UTF-8, UTF-16 BOM, depth bomb), relationship traps
(external inert, traversal rejected), and the EPUB reading path (`<!DOCTYPE`
refused; `<script>` is inert data, never reading text). No panic and no
`InternalInvariant` on any fixture.

### Fuzz campaign (8 new targets, bounded)

New coverage-guided targets reusing the existing `fuzz/` architecture (pinned
nightly + `cargo-fuzz`, 4 g / 4 cpus): `zip_scan`, `zip_decode`, `opc_rels`,
`docx_wml`, `epub_package`, `epub_content`, `xml_part`, `common_observe`
(features `docx`+`epub` now enabled in the fuzz package). Their invariants are
the cover-is-an-exact-partition rule, bounded decode/CRC, typed-only
failures, and capability self-consistency.

`FUZZ_SECONDS=10`, `FUZZ_RSS_MB=2048`, all 18 targets: the **eight Phase-12
targets finished `exit=0` with zero crash/OOM/timeout artifacts**. The only
artifact of the whole campaign was `deflate_replay`
(`oom-cba63e89…`) — the **pre-existing** upstream `preflate-rs` 0.7.6 F2
resource limitation already recorded in `fuzz/README.md` (ADR-0016), not a
Phase-12 surface and not a new finding. **No crash, hang, or resource
amplification was found in the new surfaces**, so no new regression fixture was
needed beyond the committed hostile corpus itself.

### Honest gaps

* The corpus is 45 hand-built fixtures from the research-I threat list, not a
  large real-world DOCX/EPUB corpus; it bounds the *named* threats, not every
  malformed package in the wild.
* The court measures the ingest/observe path plus the scanner/OpcModel/Docx/Epub
  builders; it does not drive the *full* field query planner over every native
  selector on every hostile package.
* The fuzz campaign is bounded (10 s/target on 4 cpus); it is a smoke-to-coverage
  sample, not a long soak. Deeper/longer campaigns remain available via
  `tools/fuzz.sh`.
* The one `deflate_replay` OOM is a known upstream limitation carried forward,
  not resolved here.

## Flagship demo (12.14)

Subphase 12.14 asks for one honest, reproducible end-to-end command — not a set
of separate courts. `tools/phase12-demo.sh` runs entirely inside the pinned
`doc-baseline` service and prints the real CLI output at every step; where a step
declines it says so, and a mismatch fails the run.

**Exact command (from the host):**

```sh
docker compose run --rm --no-TTY doc-baseline sh tools/phase12-demo.sh
```

The script is self-contained: it `cargo build --locked --all-features`, generates
the canonical triplet with the stdlib generator, ingests all three, deletes the
runtime sources, then queries and rematerializes in fresh processes.

### What it demonstrates

| step | demonstration |
|---|---|
| 1 | `tools/fixtures/doc-triplet-gen.py` emits `report.pdf`/`.docx`/`.epub` + `ground_truth.json` (deterministic; the PDF is byte-identical to the 12.9/12.11 `alpha`). |
| 2 | all three ingest through the one format-agnostic `field-ingest` into one store. |
| 3 | the runtime sources and descriptors are **deleted**; only the store remains (sealed oracle copies are kept for `capabilities`/`cmp`, never ingested — the 12.10 design). |
| 4 | fresh processes: `capabilities` per root; one `find XF12A` with distinct native provenance on all three; DOCX/EPUB heading, paragraph and `Table·Cell(B7)`; `observe --spine-item 0 --kind structure` on EPUB. |
| 4e | `explain --analyze` for a DOCX table-cell and an EPUB spine-item: `whole_source_materialized: no`, `member_decodes`, `xml_parses`, `seed_nodes_reused`, and the four ADR-0027 byte classes (`descriptor`/`manifest`/`index`/`seed`). |
| 5 | a repeated query shows retained work: `nodes_reused > 0` and a `retained_inverse_work_fraction` computed from the cold/warm receipted integers (mirroring `examples/phase12_share_court.rs`). |
| 6 | `materialize --exact` for all three, checked by length, SHA-256 **and** `cmp` against the oracle copies. |
| 7 | a pointer to the 12.11 receipt and where the source-retaining SQLite+FTS5 baseline genuinely wins. |

### Observed on this tree (commit `7ac2b09`, `doc-baseline`)

Representative values from a clean run (all live, none recorded): the triplet
`905 / 3458 / 2858` bytes; `find XF12A` matched on all three with
`format=pdf;common;`, `format=docx;common;docx;…` and
`format=epub;common;epub;…` provenance; `Cell(B7) = bravo-seven` on both DOCX
and EPUB; the DOCX cell `explain --analyze` reported
the four byte classes `descriptor=3939 manifest=306 index=3772 seed=86`, `2` member
decodes, `2` XML parses, `2` reused nodes, and
`whole_source_materialized=false`; the EPUB spine-item reported
`descriptor=3339 manifest=306 index=2499 seed=86`, `1` member decode, `2` XML
parses. `materialize --exact` returned `cmp=equal` for all three. The demo exits
`0`.

### Honest caveats

* `capabilities ROOT` reads a file path, so after the runtime source is deleted
  it runs on the **sealed oracle copy**, which is byte-identical to the deleted
  source (its SHA-256 is the one the generator recorded). The *queries and the
  materialization* run against the store alone; the oracle is never fed to the
  pipeline and the store never references it.
* The measured `retained_inverse_work_fraction` on the single-item EPUB spine
  query is small (`0.035454` here) because a one-chapter package leaves little
  derived work to reuse; the number is receipted, never asserted upward.
* The demo makes **no** lifetime claim of its own; step 7 only points at the
  12.11 receipt, including the region where the DB baseline wins.
* PDF has no heading/table/cell/spine coordinate, so the demo does not run those
  selectors on PDF (that typed decline is the 12.9 court's evidence); the demo
  shows the PDF capabilities list and the common `find`/`text`/`metadata` lane.
