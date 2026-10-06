# Phase 12 — results

Branch: `phase12`. This is a **cumulative** results document; each section names
its commit under test and links its sealed receipt. Everything here was measured
in the pinned Docker services; nothing ran on the host.

> **Skeptic correction note (2026-10-06, `15b5729`).** An independent adversarial
> review ([`docs/reviews/phase-12-skeptic-review.md`](../reviews/phase-12-skeptic-review.md))
> re-checked every headline against the sealed receipts. Findings: the security
> class tally (12.13) was wrong; two crossover / store sentences (12.11) were
> over-generalised; the claim that A0/A1-unsupported surfaces are "excluded from
> the lifetime frontier" is false (they are scheduled and the decliner is charged
> 0 bytes); "FTS5/BM25 reported separately" has no receipt; the **required
> ablation ladder `A1b`/`A2`–`A10`, §106 and §107 was not run** (only A0/A1/V were
> measured); the **"PDF no regression" acceptance gate (§109/`N6`) has no receipt
> at all**; ADR-0034's mandated post-`cache --clear`, OS-witness and CDC controls
> are absent from 12.8; the 12.9 equivalence is a self-authored generator-defined
> triplet; the 12.14 demo was run at `15b5729`, not `7ac2b09`, and has no sealed
> receipt; and the default-feature build fails `cargo clippy -- -D warnings` on
> pre-existing dead code in `src/field/document_format.rs`. Each is annotated
> inline below; the numbers are not rewritten.
>
> **Phase 12.11b update (`06db12a`).** The skeptic's **F11** (the required ablation
> ladder was not measured) is now closed: the ladder `A1b`, `A2`–`A11` was run in
> `evidence/campaigns/2026-10-06-phase12-lifetime-ablations-06db12a/` and attributes
> the win to the **content adapters** (A4 → A5) and **persistent semantic reuse**
> (A5 → A6). `A7`, `A8` are recorded **not separable**; `A10` is measured
> ingest-side. See [Ablation ladder (12.11b)](#ablation-ladder-1211b). `N6` (PDF no
> regression) remains open.

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

> **Skeptic correction (2026-10-06, `15b5729`).** ADR-0034 requires the reuse
> fraction to be receipted warm **and post-`cache --clear`**, cross-checked with an
> **OS-level witness**, and compared against a **CDC baseline** (`N3` makes
> "reuse ≤ strongest CDC" a no-go). The 12.8 receipt contains **none of the
> three**: `retained_inverse_work_fraction = 0.339907` is a single *warm,
> in-process* observation (`grep -ri 'cache --clear|cdc|witness'` over the court,
> driver, library test and receipt returns nothing). Without the post-clear control
> it cannot be distinguished from a process-memory cache, and `N3` is **not
> evaluated**. The fraction is computed from executions/bytes (not source size),
> as claimed; only the controls are missing.

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

* **The triplet is self-authored and its ground truth is generator-defined.**
  `report.pdf`/`.docx`/`.epub` and `ground_truth.json` are all emitted by the
  same `tools/fixtures/doc-triplet-gen.py`, so 96/96 shows each adapter reads back
  content the generator wrote — adapter consistency, not independence from a
  third-party document. *(Skeptic note, `15b5729`.)* The shared `metadata`
  selector is a shared *name* with per-format semantics (PDF `source_len`/`sha256`,
  DOCX story, EPUB package summary), not a common observation.
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

> **Skeptic correction (2026-10-06, `15b5729`).** This court measured only **A0,
> A1 and V**. The plan §138–145 / ADR-0035 **required** ablations `A1b` and
> `A2`–`A10`, plus §106 (eager-vs-progressive) and §107
> (raw-compressed-vs-decoded-persisted); **none was run in this receipt**
> (`receipt.json` contains `a0`/`a1`/`v` only). The headline therefore describes
> the Phase-12 field *as a whole* versus A0/A1 and the rung that produces the win
> is attributed separately in **12.11b** below. The **"PDF no regression"
> acceptance gate (§109 / `N6`) has no receipt** (no Phase-11-vs-Phase-12 PDF
> comparison exists), so `N6` is not evaluated. The `delta` documents are **~60 KB
> synthetic**, so "large-document frontier" is scoped to this corpus. See
> [`docs/reviews/phase-12-skeptic-review.md`](../reviews/phase-12-skeptic-review.md).
>
> **Phase 12.11b update (`06db12a`).** The required ladder (`A1b`, `A2`–`A11`) was
> run in `evidence/campaigns/2026-10-06-phase12-lifetime-ablations-06db12a/` and
> attributes the win: the **content adapters** (A4 → A5) enable the semantic
> surfaces and **persistent semantic reuse** (A5 → A6) trades bytes for CPU; A7/A8
> are recorded **not separable** and A10 is measured ingest-side. See
> [Ablation ladder (12.11b)](#ablation-ladder-1211b). `N6` (PDF no regression) is
> still open.
>
> **Skeptic note on baseline fairness.** A0 for DOCX/EPUB runs a **fresh `python3`
> process per query** and A1's one-time build is also a `python3` process, so both
> pay Python interpreter startup on the measured boundary (A0 ~1.6 MB read per
> query; A1's one-time 1.8–2.7 MB). "VOLE beats A0 on all N" and "VOLE wins the
> cold one-time comparison" are therefore partly a language-runtime artifact; the
> A1 acceptance gate is the meaningful comparison, and there VOLE loses the large
> documents.

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

*(Skeptic correction, `15b5729`.)* The `alpha/bravo/charlie` row is an aggregate
that is wrong for the **PDFs**: the receipt gives `alpha.pdf`/`bravo.pdf`/`charlie.pdf`
wall/CPU vs A0 as **N≥10**, not "all N" (at `N=1`, `alpha.pdf` A0 wall `2.343 ms`
< V `3.808 ms`; `raw/cumulative.jsonl`). Only the byte metric is "all N" for the
small PDFs. Also, the `delta` documents are **~60 KB** synthetic, so "large" here
means "large in this corpus", not real-world scale.

So the result is genuinely mixed: VOLE owns the small-document frontier except
for the small PDFs' wall/CPU at `N=1` (where A0 wins or ties narrowly), and it owns
the cold one-time comparison, while the source-retaining SQLite baseline takes
over the **large** (60 KB synthetic) documents' byte frontier from N=10–1000 and
their wall/CPU frontier at N=1000. This is exactly the admissible "A1 wins simple
lookups" region ADR-0035 requires to exist.

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
VOLE's store is **larger** than A1's `.db` for `delta.pdf` (462 KB vs 221 KB) and
`delta.docx` (902 KB vs 823 KB) but **smaller** for `delta.epub` (599 KB vs
819 KB). *(Skeptic correction, `15b5729`: the earlier "larger on `delta`" was
false for `delta.epub`.)*

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
  `delta.pdf` and `delta.docx` but is *smaller* on `delta.epub` (599,424 B vs
  819,200 B; receipt per-document table). *(Skeptic correction, `15b5729`.)*
* **VOLE declines DOCX exact resource bytes.** `observe --resource 0 --kind
  decoded` is unsupported for DOCX (typed capability error); A0/A1 answer the
  member bytes. (EPUB and PDF exact bytes are answered and asserted.)
* **PDF has no native provenance.** The `native-provenance` observation returns
  the empty string for PDF page text (the 12.9 gap); it is VOLE-only for
  DOCX/EPUB and not a capability A1 can answer at all.
* **The `search` lane uses substring semantics.** FTS5 is built and `ANALYZE`d,
  but VOLE `find` is a lexical *substring* match, so A1 answered the same query
  with `LIKE` — a tokenizer would miss `XF13A` glued to adjacent glyphs by
  Poppler's text reordering. *(Skeptic correction, `15b5729`.)* The earlier
  wording "Whole-token FTS5/BM25 remains a real A1 capability, reported
  separately" is **withdrawn**: no FTS5/BM25 measurement exists in the receipt
  (the only search query run is `text LIKE '%…%'`), so the FTS lane is **not
  measured**, not separately reported.
* A0/A1 cannot answer the VOLE-only surfaces (native provenance; PDF structure,
  preview, object/decoded-stream bytes). *(Skeptic correction, `15b5729`.)* The
  earlier claim that those surfaces "are excluded from the lifetime frontier" is
  **false**: the schedule includes them for every document and a declining system
  is charged **0 bytes** for the case (e.g. `alpha.docx` `exact-member`
  A0 `1,629,325` / A1 `32,569` / V `0`, while `native-provenance` A0 `0` / A1 `0`
  / V `16,861`). The cumulative comparison therefore sums over **different
  answered subsets** and is not like-for-like on those two cases; the surfaces are
  not claimed as lifetime wins, but they are not excluded either.

## Ablation ladder (12.11b)

**Receipt:** `evidence/campaigns/2026-10-06-phase12-lifetime-ablations-06db12a/`
(`receipt.json`, `SUMMARY.md`, `commands.txt`, `gates.txt`, `raw/` incl. the wide
`case_bytes.tsv`, `cumulative.jsonl` — 960 rows — `assertions.tsv`,
`a10_shared_ingest.json`, `gate.txt`). Commit under test `06db12a` (dirty `""`);
10 lanes × 12 documents × N∈{1,10,100,1000} × 2 passes; **0 assertion failures**.
Court: `tools/phase12-lifetime-court.sh`. The rung table is pre-registered in the
committed schedule (`tools/fixtures/phase12-lifetime-schedule.json` `.ladder`,
emitted by `tools/fixtures/phase12-schedule.py`), so the ladder cannot be chosen
after seeing a result.

This **closes the skeptic's F11** (the required ladder was not run). Each rung is
a separate measured lane with its own switch — a **feature-set build** or a
**runtime flag**, never copy-pasted code:

| rung | lane / switch | status | what it is |
|---|---|---|---|
| A0 | `a0` | baseline | direct tooling per query |
| A1 | `a1` | baseline | source-retaining SQLite+FTS5 |
| A1b | `a1b` = A1 + `result_cache` materialized views | measured | A1 + cached popular results (A1's own SQL is the single source of truth) |
| A2 | `a2` = build `rans,store,field` | measured | Phase-11 field, PDF only |
| A3 | `a3` = build `...,package` | measured | ZIP physical members only (no OPC/OCF graph) |
| A4 | `a4` = build `...,package,opc` | measured | OPC package graph (content types, part/relationship resolution); no `docx`/`epub` content |
| A5 | `a5` = build `...,package,opc,docx,epub` + `--no-cache` | measured | + progressive semantic inversion (the `docx`/`epub` content adapters), no persistent reuse |
| A6 | `a6` = same build, DerivedCache on | measured | A5 + persistent semantic reuse |
| A7 | n/a | **not separable** | the common vocabulary *is* the dispatch (`Ctx::common_dispatch`); there is no native-only CLI path, so removing it is capability-only (definitional, `N2`) |
| A8 | n/a | **not separable** | the hierarchical index is built unconditionally at ingest and every narrow observation resolves through it (`indexed_exact`); there is no non-indexed build |
| A9 | `a9` = all-features + `--entropyfs` | measured | EntropyFS fine-grained range access |
| A10 | shared-store ingest proxy | **not separable (per-document court)** | one store per document by construction; measured as the shared-store ingest delta |
| A11 | `a11` = all-features | measured | full Phase-12 system (≡ the prior `V` lane) |

§106 (eager-vs-progressive): **not separable** — progressive inversion is the only
implemented mode (model nodes are registered at ingest and parsed on first
materialization). §107 (raw-compressed-vs-decoded-persisted): **proxied by A5 vs
A6** — A5 (`--no-cache`) recomputes from the compressed raw spans; A6 persists the
decoded model/member outputs in the disposable cache.

### Mechanism attribution (which capability moves which metric)

Headline, from `receipt.json` `cumulative` (N=1000 warm) and the answered-set delta
(`SUMMARY.md` "Mechanism attribution"):

* **The content adapters (A4 → A5), not the package graph, are the dominant
  mechanism.** A4 (OPC graph only) answers the same single `full-source` case as
  A3; adding the `docx`/`epub` semantic inversion (A5) moves the DOCX/EPUB answered
  set from **1/12 to 11–12/12** cases (block/heading/table/cell/metadata/resource/
  search), at ~10–20× the warm per-query bytes (`delta.docx` 5.8 MB → 66 MB;
  `alpha.docx` 0.87 MB → 13 MB). Without it every pre-registered semantic surface
  declines.
* **Persistent semantic reuse (A5 → A6) trades bytes for CPU**, exactly as the
  ADR-0035 boundary predicts. `delta.docx` warm N=1000: CPU **4590 ms → 1480 ms**,
  wall **4396 ms → 1303 ms**, but process reads **66 MB → 330 MB** (a cache hit
  re-reads the cached decoded payload). Small docs barely move (bytes ≈ +5%,
  CPU ≈ −14%).
* **The ZIP layer and the OPC graph (A2 → A3 → A4) do not move the frozen
  schedule's answered set.** A3's store grows (member raw/decoded nodes) and A4
  adds the OPC graph, but the workload is entirely *common semantic* surfaces, so
  A2/A3/A4 all answer only `full-source` (e.g. `delta.docx` read 5.84 MB → 5.78 MB →
  5.78 MB). Their value is visible only through native selectors (`member`,
  `package-part`, `relationship`), which the frozen schedule does not exercise —
  recorded, not hidden.
* **EntropyFS (A9) is a loss on this frontier.** `delta.docx` warm N=1000: reads
  **694 MB** vs A11's **331 MB**, CPU **2190 ms** vs **1560 ms**; the backend
  disables the narrow cache short-circuit by design, so it pays full work per query.
* **A11 ≈ A6** (the all-features build adds only dynamic-loader reads): because
  A7/A8 are not separable, the "full system" is, on this workload, the cache-on
  content lane.
* **A1 → A1b** moves bytes *up* (the materialized `result_cache` is re-read) while
  moving CPU down for the cached answers; its one-time population is amortized at
  N=1000 (`alpha.docx` A1 `29,304,533 B / 1280 ms`; A1b `37,283,329 B / 2300 ms`).

### Crossover with the ladder (warm pass)

The 12.11 crossover is **unchanged in direction but now attributable**: A11 beats
A0 on wall/CPU for every small document and loses the large-document byte frontier
to A1 (`delta.docx` bytes from N=10, `delta.epub` from N=100, `delta.pdf` from
N=1000; wall/CPU from N=1000 for `delta.docx`/`delta.epub`). A1b tracks A1 (its
materialized cache does not beat A1 on bytes). See `SUMMARY.md` "Crossover".

### `N5` (package-index-only)

The ladder's A3 (ZIP physical) and A4 (OPC graph) rungs are the closest honest
measurement of `N5` ("reproducible by `unzip -p` + `substr` at the same
boundary"): neither a ZIP index nor the OPC package graph **can answer any of the
pre-registered semantic surfaces** (they decline 11/12 DOCX and 11/12 EPUB cases).
So the small-document win is **not** reproducible by a package index alone; the
*content adapters* (A5) are what answer. This is evidence against `N5`, though the
court does not itself run `unzip -p`+`substr` (that mechanical check remains the
negative control's job).

### Honest gaps (12.11b)

* **A7 and A8 are not separable** in the landed architecture (the common dispatch
  and the ingest-built index are not behind switches); recorded as `not separable`
  with the reason, never fabricated. **§106** is not separable (progressive
  inversion is the only mode).
* **A10 cannot be measured as a per-query lifetime effect** in a per-document
  court; the ingest-side proxy (all 12 documents into one shared store:
  `shared_resource_ids=7`, `shared_resource_bytes=7224`, `nodes_id_shared=47`) is
  a representation fact, not a frontier metric. The `N3` reuse-work court remains
  the 12.8 share receipt.
* **A3 and A4 are invisible on the frozen schedule**: it exercises only common
  semantic surfaces, so the ZIP-physical and OPC-graph rungs' capabilities are
  not scored (they change persistent bytes, not the answered set).
* The per-query byte boundary includes each process's dynamic-loader reads, so a
  larger binary (A9/A11, all-features) pays more per spawn. A3/A4/A5/A6 share
  binaries pairwise (`a3`=`package`; `a4`=`package,opc`; `a5`/`a6`
  =`package,opc,docx,epub`), so the A3↔A4↔A5↔A6 comparisons cross binaries and
  carry this confound.
* Corpus is the same self-authored **~0.8–61 KB** set as 12.11; "large" means
  "large in this corpus".

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
scoped verdict: VOLE wins the small-document lifetime frontier except the small
PDFs' wall/CPU at `N=1`, and the cold one-time comparison; it ties or loses
PDF-page and simple-lookup surfaces; and it **loses the large (60 KB synthetic)
document frontier to the source-retaining SQLite baseline** at
N ≥ 10–1000 bytes and N = 1000 wall/CPU. Every loss is recorded above; a headline
that never lost would not have been credible. *(Skeptic: the required ablation
ladder was not run and the PDF no-regression gate has no receipt — see the notes
at the top of the 12.11 section.)*

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
| `opaque-preserve-and-decline` (typed) | 22 | a raw malformed/hostile ZIP; the universal ingest falls back to the byte-exact opaque floor and the unsupported observation declines `UnsupportedFeature` |
| `accept` | 7 | a well-formed (if unusual) DOCX/EPUB the pipeline answers; scripted/remote/encrypted content is retained as inert data |

*(Skeptic correction, `15b5729`.)* The earlier counts in this table were
`opaque-preserve-and-decline` **29** and `accept` **10** (summing with `16` to 55
for a 45-fixture corpus). The sealed receipt is authoritative:
`outcomes.tsv` / `SUMMARY.md` tally **16 reject / 22 decline / 7 accept = 45**
(`awk -F'\t' 'NR>1{print $4}' outcomes.tsv | sort | uniq -c`). The 315 assertions
(45 × 7 invariants) and their PASS result are unchanged.

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
* **No pre-registered decline threshold.** The `opaque-preserve-and-decline`
  outcome (22/45 fixtures) is the intended §DEC-9 safety behaviour, but ADR-0035
  `N4` ("ETL illusion") requires a pre-registered decline-rate threshold and none
  exists, so `N4` is **not evaluated**. *(Skeptic note, `15b5729`.)*
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

### Observed on this tree (commit `15b5729`, `doc-baseline`)

*(Skeptic correction, `15b5729`.)* The earlier attribution to commit `7ac2b09` is
impossible: `tools/phase12-demo.sh` and the `--spine-item` selector were both
added in `15b5729` (`git ls-tree 7ac2b09` has no `tools/phase12-demo.sh`). The
values below were re-verified live by the Phase-12.15 skeptic on `15b5729`
(`evidence/campaigns/2026-10-06-phase12-skeptic-15b5729/raw/demo.txt`, exit `0`).
This demo has **no sealed receipt of its own**; the numbers live only here. The
`capabilities` step reads the sealed oracle copy after deletion (see caveats), so
"fresh processes query the store with the source gone" applies to the query and
materialization steps, not to `capabilities`. The final banner says "no step
declined" because the declining PDF selectors are never invoked.

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
