# Changelog

All notable changes are recorded here. The format is pre-1.0 and provisional.

## [0.1.0-alpha.31] — Phase 21 Wave 2 begins: JSON (structured tree) — *unreleased*

- **21.5 — JSON adapter + economic court.** Non-default, dependency-free
  `json = []` feature. Byte-based, conservative detection (the whole source must
  parse as exactly one JSON value; malformed → Opaque). A bounded,
  **representation-preserving** parser: exact token spans, member order, numeric
  spelling (`1e3`), string escape spelling (`\u00e9`), and **duplicate keys kept
  distinct**. Native `--json-pointer` (RFC 6901), `--json-node`, `--json-find`;
  common metadata/text/find. Exact closure 8/8 (adapter) and 7/7 (economic).
  Economic court vs a source-retaining SQLite `json1`/`jsonb` baseline: build
  **1.244×**, storage **0.599×**, cold **0.046×**, warm **0.570×**; the two
  cross-lane mismatches (duplicate-key count, escape spelling) are exactly the
  representation-preservation value VOLE claims.
  `2026-10-09-phase21-5-1-json-0d94667`, `…-phase21-5-json-econ-0d94667`.

## [0.1.0-alpha.30] — Phase 21 Wave 1: six office formats (XLSX, PPTX, ODS, ODP) + economic courts

Opens the Phase-21 **format programme** by adding the fifth document format, XLSX,
over the shared byte-authoritative OPC/ZIP layer, and judging it on the
equal-contract capability/cost frontier rather than by format count. Plan:
[phase-21-plan.md](../phases/phase-21-plan.md); decision:
[ADR-0059](../adr/0059-xlsx-adapter-and-analytical-comparator.md).

### Added

- **21.1.1 — XLSX adapter (OPC surface + exact closure).** Non-default
  `xlsx = ["opc"]` feature; byte-based `DocumentFormat::Xlsx` detection, mutually
  exclusive with `docx`/`epub`/`odt`. Native selectors `--sheet`, `--xlsx-cell`,
  `--xlsx-find`, plus common `metadata`/`text`/`table`/`cell`. Exactness 2/2 after
  source + descriptor deletion. `2026-10-08-phase21-1-xlsx-4d26514`.
- **21.1.2 — the SpreadsheetML semantic model.** Styles (number formats, fonts,
  fills, alignment), merged ranges, comments (+VML note anchors), internal/external
  hyperlinks, defined names, tables, drawings/charts/media as a relationship graph,
  package external relationships, and a bounded deterministic **displayed-value**
  projection kept distinct from the stored formula and cached result. Exactness
  3/3. `2026-10-09-phase21-2-xlsx-5802be9`.
- **21.1.3 — the XLSX economic court.** Three lanes (VOLE; a source-retaining
  SQLite baseline; a **DuckDB/Parquet** comparator the plan mandates for tabular
  formats) over a deterministic self-authored 8-workbook corpus; Q1–Q10 on
  contract-equivalent terms. Paired ratios (median, fixed-seed cluster bootstrap by
  fixture): build **0.637×** SQLite / **0.291×** DuckDB (8/0/0), storage **0.202×**
  SQLite (8/0/0) but 1.610× DuckDB, warm 0.704× SQLite (5/0/3). VOLE records
  formula-dependents and chart→table linkage as typed **capability gaps**. Exactness
  8/8. `2026-10-09-phase21-3-xlsx-b2400f1`.
- **Pinned analytical comparator.** New opt-in, hard-capped `analytical` service
  (same pinned base digest as `doc-baseline`) with a hash-pinned duckdb 1.5.6 wheel;
  smoke court `tools/phase21-3-analytical-smoke.sh`.
- **21.2.1 — PPTX (PresentationML) adapter.** Non-default `pptx = ["opc"]`
  feature; byte-based detection mutually exclusive with DOCX/XLSX. Bounded model:
  slide order from `p:sldIdLst` (never `slideN.xml` order), shapes (text/run-level,
  pictures→media, embedded tables, groups, connectors), notes, layouts, masters,
  themes, media. Native `--slide`, `--pptx-shape`, `--pptx-notes`, `--pptx-layouts`,
  `--pptx-masters`, `--pptx-theme`, `--pptx-media`, `--pptx-tables`, `--pptx-find`.
  Exactness 5/5. `2026-10-09-phase21-2-pptx-8aab956`.
- **21.2.3 — the PPTX economic court.** 8 deterministic decks; Q1–Q8 vs a
  source-retaining SQLite baseline (PPTX is not tabular, so no DuckDB lane).
  Paired medians: build **0.879×** (4/4/0), storage **0.967×**, cold **0.094×**,
  warm **0.447×**; exactness 8/8. `2026-10-09-phase21-3-pptx-054ce93`.
- **21.3.1 — ODS (OpenDocument Spreadsheet) adapter.** Non-default `ods = ["opc"]`
  feature over the ODF substrate (like ODT): `office:body/office:spreadsheet` —
  sheets/rows/cells with typed values + displayed text + stored formula + style kept
  distinct, merges, repeated cells/rows (bounded: a bomb declines typed before
  allocation), named expressions, styles, comments. Native `--ods-sheet`,
  `--ods-cell`, `--ods-styles`, `--ods-named-expressions`, `--ods-comments`,
  `--ods-find`. Exactness 8/8. `2026-10-09-phase21-3-ods-ef26d97`.
- **21.3.2 — the ODS economic court.** 8 deterministic workbooks; Q1–Q10 vs a
  source-retaining SQLite baseline **and** a DuckDB/Parquet comparator. Paired
  medians: build **0.708×** SQLite / **0.334×** DuckDB (8/0/0), storage **0.175×**
  SQLite (8/0/0) but **1.356×** DuckDB, warm **0.477×** SQLite. Exactness 8/8.
  `2026-10-09-phase21-3-2-ods-3dd5827`.
- **21.4 — ODP (OpenDocument Presentation) adapter + economic court.** Non-default
  `odp = ["opc"]` feature over the ODF substrate: slides = `draw:page` in
  **document order** (never page-name/file order), shapes (text boxes/runs,
  images→`Pictures/`, bounded groups, tables), notes, masters, styles, media.
  Native `--odp-slide`, `--odp-shape`, `--odp-notes`, `--odp-masters`,
  `--odp-media`, `--odp-tables`, `--odp-find`. Economic court vs a source-retaining
  SQLite baseline: paired medians build **1.018×** (~parity), storage **1.448×**
  (VOLE larger here, stated), cold **0.123×**, warm **0.392×**; exactness 6/6
  (adapter) and 8/8 (economic). This **completes Wave 1: six office formats from two
  shared substrates**. `2026-10-09-phase21-4-1-odp-957a800`,
  `2026-10-09-phase21-4-odp-econ-957a800`.

### Fixed (21.2.1 — from an independent audit; ADR-0060)

- **Source-scoped node identity.** Several nodes that read the source bytes
  (`DocumentExact`, `SourceSlice`, `PdfObject`/`PdfRevision`/`PdfStreamEncoded`,
  `PackageRoot`, `PackageMemberRaw`) did not include a source-identity input, so the
  derived cache (keyed on `NodeId` alone, shared across fields) could serve one
  document's bytes for another when two fields shared a store. Fixed by keying
  exact roots on `sha256(source)` and making span nodes depend on the root; this
  also makes field ids independent of store contents. Symptoms fixed: a PPTX deck's
  common text collapsing to one slide, and `--byte-range --kind exact` returning
  another field's bytes.

### Fixed (21.1.2b — from an independent adversarial audit)

- Checked arithmetic on A1 cell references (an over-long reference no longer
  overflows `u64`) and bounded coordinates (`max_xlsx_col`/`max_xlsx_row`) and
  bounded the sheet-text projection before building it.
- DOCX detection now uses a **positive** WordprocessingML signal, so a Word
  document that embeds an Excel workbook is no longer misdetected as `Opaque`.
- Merged-range **refs** (not just a count) are exposed; "missing parts decline
  typed" wording corrected to the precise four-part contract.

## [0.1.0-alpha.29] — Phase 22 economic programme complete (22.3–22.7)

Completes the Phase-22 **economic programme** (P2–P6). **Every subphase is a
measurement or court result; no production mechanism was shipped.** The programme
strengthened the competitor first (22.1) and then tested, one at a time, whether a
new mechanism would pay — and recorded the negatives honestly, exactly as the
programme's "failed gates are publishable" rule requires. Results:
[phase-22-results.md](../phases/phase-22-results.md),
[phase-22-3-results.md](../phases/phase-22-3-results.md),
[phase-22-4-results.md](../phases/phase-22-4-results.md),
[phase-22-5-results.md](../phases/phase-22-5-results.md),
[phase-22-6-results.md](../phases/phase-22-6-results.md),
[phase-22-7-results.md](../phases/phase-22-7-results.md).

### Measured / recorded

- **22.2 — compact query-native directory: `SESSION`/`NO WIN` class negative.** The
  index/selector layer is **10.6 %** of the warm session (probe **0.4 %**),
  redundant re-reads of one depth-0 leaf (493 opens / 20 files); a perfect
  directory removes ≤10.6 % (≈**0.13** shift) vs court **MDE ≈0.399** —
  **below resolution**. Nothing shipped (`2026-10-08-phase22-2-d81689c`).
- **22.3.0 — fused heterogeneous execution: `SESSION-ALREADY-CAPTURES`.** The
  duplication is real (an independent DOCX/EPUB batch executes **3.66–4.11×** more
  seed nodes) but the shipping session's memo + derived cache **already remove
  it**; the only undeduped axis is the index descent — the 22.2 sub-MDE candidate.
  Nothing shipped. A first `|U|`-based `CLEARS` was **withdrawn** (unsound metric)
  (`2026-10-08-phase22-3-dup-ca9f29e`).
- **22.4 — unknown-query lifetime frontier: `MIXED` region split.** Pre-registered
  hidden schedule (seed 220400), all adaptation charged: resolved VOLE **win** in
  the **PDF** region (0.73–0.83×), resolved **loss** in **EPUB** (1.50–1.82×),
  **unresolved** pooled/DOCX/size-classes; storage **0.468× (win)**; peak RSS a
  resolved loss (`2026-10-08-phase22-4-lifetime-b4225fe`).
- **22.5 — remote selective materialization: negative under a labelled model.**
  `remote-selective-v1` (no S3; real byte counts + a real loopback range server):
  VOLE selective transfers **median 3.15× more bytes** than a page-level SQLite
  control (0 wins / 17 losses / 7 unresolved); exact byte-range reads tie on bytes
  and win modelled latency (`2026-10-08-phase22-5-remote-762c81c`).
- **22.6 — compact structural representation: no win.** The exact `descriptor/` is
  **97.2 %** of the footprint, so typed bytes are only **2.765 %**; the best
  candidate encoding saves **1.166 %** of the footprint vs a 5 % bar; bitmap/rank
  **loses** to a sorted delta list (`2026-10-08-phase22-6-compact-85eee7c`).
- **22.7 — agent end-to-end cost: VOLE loss.** Deterministic scripted agent (no
  LLM), 27 frozen tasks, pinned offline tokenizer: cost per correct grounded task
  VOLE **1.418×** the baseline (gate ≤0.5×) — **not met**; VOLE **16/27**
  correct+grounded vs **27/27** (PDF 0/9 heuristic text with no span; EPUB a token
  tie) (`2026-10-08-phase22-7-agent-ee8723b`).

### Notes

- No `src/` change: every subphase added only `tools/` courts and `evidence/`
  receipts. Exactness (`materialize == source`) is untouched.
- Recorded follow-ups (not built): a PDF text **source span** and an EPUB `find`
  member span (22.7); in-session verified-node memo (22.2/22.3).

## [0.1.0-alpha.28] — Phase 25: durability corrections (directory ancestry + packed-header recovery)

An external review found **two correctness gaps** in the Phase-23 durability work
(plus a court weakness). All three were **verified in the code before fixing** and
are closed here. Results: [phase-25-results.md](../phases/phase-25-results.md);
decision: [ADR-0058](../adr/0058-directory-ancestry-durability-and-unpublished-segment-recovery.md).

### Fixed

- **Directory ancestry was not durable.** `durable::create_dir_all` created
directories but never synced their **parent**, so an ancestor (`index/aa/`) could
be lost across a power cut even though its child (`index/aa/bb/`) was synced —
orphaning every node beneath it. It now creates each missing component and
`fsync`s each new entry's parent; the store-open paths
(`FieldStore::open`/`open_packed`/`open_entropyfs`, `FsSeedStore`, `FsIndexStore`,
`DerivedCache`, `PromotedStore`) route through it.
- **The packed segment header was synced after its directory.**
`PackWriter::ensure_open` could leave a durable directory entry pointing at a
segment with an incomplete header. It now syncs the header file **before** the
directory; and `scan_open_segment` treats an incomplete/unparseable header on an
**unsealed** (always unpublished) segment as **absent**, so the store **reopens
and recovers the prefix** instead of failing closed.

### Changed

- **The power-loss proxy models directory creation and ancestry** (`build_model`
folds `mkdir`; `reconstruct` prunes non-durable directories deepest-first).
- **The verdicts are unified on the strict rule:** a no-manifest state must
**reopen** (prefix recovery); a published manifest must be exact and serviceable.
`lenient_verdict` is removed.

### Measured

- Re-sealed `2026-10-08-phase23-durability-fc63436-phase25`: **32 cases, 20 PASS /
0 FAIL / 0 shipped-arm CRITICAL** under the **stricter** rules; 12 counterfactual
`model-drop` CRITICAL; `bad_hash` 0. Every cut arm now **opens** — including
`packed-batch-cut-record.after_body` and `packed-batch-cut-flush.before_sync`,
which previously failed closed (now prefix `truncated=1`). Dir-sync cost (median
of 3, host ext4 bind mount): fs `off` **723 → safe 1,381 ms** (~1.91×), packed
**22 → 34 ms** (~1.55×); below noise on tmpfs.

### Recorded

- **Scope unchanged: the proxy is a model from a barrier log, not a real power
cut** (no device write cache, journal, torn sectors, or real directory-entry
loss).

## [0.1.0-alpha.27] — Phase 22.2 profiling gate + Phase 23 durability gaps closed

Two review-driven increments. **Phase 22.2** is a **profiling gate that shipped
nothing**: the compact query-native selector directory was **not** built because
attribution shows the index/selector layer is only **10.6 %** of the warm session
(selector resolution **0.4 %**) and that share is redundant re-reads of one
immutable depth-0 leaf (493 opens / 20 files), so a perfect directory would shift
the warm headline by ≈**0.13** against a court **MDE ≈0.40** — **below
resolution** — while adding persistent bytes. The before/after warm court against
the Phase-22.1 tuned `full` envelope is a **NULL** (1.211 → 1.293, overlapping
CIs; no layout byte changed). **Phase 23** closes the two durability gaps Phase
20.1 named but could not measure, by engineering: every atomic publish now
`fsync`s its containing **directory** (default `DirSyncPolicy::Safe`;
`--dir-sync=off` escape hatch), and a **model-based power-loss proxy**
reconstructs the post-power-loss store from a barrier log and re-runs the
Phase-20.1 invariants. Results: [phase-22-results.md](../phases/phase-22-results.md),
[phase-23-results.md](../phases/phase-23-results.md).

### Added

- **Directory `fsync` on every atomic writer** (`src/store/durable.rs`;
  `field::write_atomic`, `FsSeedStore::put_node`, `FsIndexStore::put`, packed
  `ensure_open`/`seal`; CLI `--dir-sync=safe|off`). `DirSyncPolicy::Safe` is the
  default; on strict POSIX a rename is not durable until the parent directory is
  synced, so this is what makes a published manifest survive a power cut.
  `--dir-sync=off` exists to *measure* the cost and to *show* the barrier is
  load-bearing.
- **A model-based power-loss proxy** (non-default `power-log` feature +
  `tests/power_loss_proxy.rs` + `tools/phase23-powerloss-court.sh`): every
  durability barrier is journaled with the byte range it covered; the proxy folds
  the log into a per-path model (a file survives only if its create/rename was
  followed by a parent `dirsync`; content truncated to its last completed
  barrier), reconstructs the post-power-loss store, and checks the Phase-20.1
  invariants under `Batch`/`Each` × fs/packed, complete and cut.
- **`src/field/prof.rs`** profiler extension (`VOLE_PROFILE_OPEN`, off by
  default) and the 22.2 profiling/court scripts (`tools/phase22-2-profile.sh`,
  `tools/phase22-2-court.sh`).

### Measured

- **22.2 warm-session attribution** (12-doc packed subset; receipt
  `2026-10-08-phase22-2-d81689c`): pooled session **22,219 µs = open 55.5 %
  (`Descriptor::parse` 44.7 %) + loop 44.5 %**; inside the loop probe **0.4 %**,
  dispatch 32.0 %, serialize 8.4 %, materialize 6.3 %; index read+verify 10.5 %,
  index decode 0.1 %. The index term is **redundancy** — 493 node opens for 20
  distinct files (one root leaf re-read 117×) — not lookup work. Before/after
  paired warm court vs tuned `full`: **1.211 → 1.293** (overlapping CIs) = NULL;
  exactness 12/12, 480 answers / 0 mismatches; bytes 0.763×.
- **23.1 directory-`fsync` cost** (median of 7, plain binary, host ext4 bind
  mount): fs **717 → 1,400 ms** (~1.95×; one dir-`fsync` per node, ~6,800 for
  `nist-pdf-0017`), packed **23 → 35 ms** (~1.5×); below noise on tmpfs.
- **23.2 power-loss proxy** (receipt `2026-10-08-phase23-durability-8816507`):
  **32 cases — 20 PASS / 0 FAIL / 0 CRITICAL on shipped arms**; the 12
  counterfactual `model-drop` arms are all **CRITICAL** (sensitivity control);
  `bad_hash` **0**; `safe` survives **4/4**, `off` loses **4/4**. `Batch`/`Each`
  are power-equivalent for the published field.

### Changed

- The **ADR-0053** note that the rename is "argued, not measured" is now
  **closed** ([ADR-0057](../adr/0057-directory-fsync-and-power-loss-proxy.md));
  `Batch` vs `Each` is stated precisely (equivalent for the published-manifest
  invariant; differ only in the unreferenced open-segment tail).

### Recorded

- **22.2 is a publishable negative.** Nothing structural shipped; the compact
  selector directory is ~3× below the court's minimum detectable effect. The
  cheapest real lever — an **in-session verified-node memo** (zero persistent
  bytes) — is left for a **higher-resolution** court; `dispatch` (32 %) is one
  bucket and must be split before crediting anything against it.
- **The power-loss proxy is a model, not a physical power cut.** It does not
  exercise the device write cache, the filesystem journal, torn sectors, or a
  real directory-entry loss; a physical proxy is not reproducible in the pinned,
  unprivileged, hard-capped lane. That residual is stated, not approximated.

## [0.1.0-alpha.26] — Phase 22.1: the competitor envelope (strengthen the competitor first)

Phase 22.1 is the **P0 competitor gate** for the Phase-22 economic programme:
before any frontier claim is made, the competitor is **maximized first**. Six
purpose-tuned SQLite configurations are added (
`tools/fixtures/phase22-competitors.py`), and the Phase-18 baseline is kept
**unmodified** as the historical-control lane `hist`. The strengthened competitor
**shrinks** the previously reported VOLE edges: the earlier storage headline was
measured against a baseline carrying a **contract-dead** trigram/FTS index (the
contract has **no search observation**), so the honest storage advantage against
a Pareto-tuned equal-contract SQLite is **~0.76×, not 0.53×**; the build advantage
survives but is smaller; cold survives; and the warm advantage **does not exist**.
**No capability gap remains.** Results:
[phase-22-results.md](../phases/phase-22-results.md).

### Added

- **Six-configuration SQLite competitive envelope + equal-contract court**
  (`tools/fixtures/phase22-competitors.py`, `tools/phase22-competitors-court.sh`):
  `minimal` (source + value/metadata, no secondary indexes, no FTS), `fts`
  (minimal + **one** FTS5, external-content, `detail=none`, `columnsize=0`,
  unicode61 — no trigram), `structural` (+ `native_coord` column/index), `full`
  (all contract projections; **no FTS** because the contract has no search
  observation), `adaptive` (builds minimal, lazily materializes on first demand,
  all cost charged), `hybrid` (`full` + 128 MB cache, `journal_size_limit`,
  Poppler native PDF extraction). The Phase-18 Python/Poppler baseline is kept
  **unmodified** as the historical-control lane `hist`. All lanes use one
  transaction + `executemany` per build and explicit PRAGMAs.

### Measured

- **22.1 competitor frontier.** Receipt
  `2026-10-08-phase22-competitors-86d9312` (12 documents, C0–C5, N=10, paired and
  interleaved, bootstrap 20,000 seed 220019). Frontier (pooled medians): VOLE
  build **13.2 ms**, **7,778,087 B**, cold **8.44 ms**, warm **2.49 ms**; at C5,
  `full` **59.8 ms / 10,201,834 B / 10.92 ms / 2.00 ms**, `adaptive`
  **58.2 / 9,661,209 / 68.63 / 2.01**, `hybrid` ≈ `full`, `hist`
  **68.5 / 14,723,834 / 11.10 / 1.94**. Paired VOLE ratios at C5 — build
  **0.219** vs `full` (0.194 vs `hist`), bytes **0.762** vs `full` (0.805 vs
  `adaptive`, 0.528 vs `hist`), cold **0.812** vs `full`, warm **1.211** vs
  `full` (95% CI **1.006–1.483**) and 1.253 vs `hist`. Equivalence **1920/1920**
  byte-identical, **0** mismatches; exactness **12/12** VOLE and **6/6** per
  document per configuration.
- **Durability compared explicitly.** Matched setting is SQLite
  `journal_mode=WAL` + `synchronous=NORMAL`; a separate `synchronous=FULL` probe
  measures build **6.4 → 24.3 ms** (**~3.6×**), bytes unchanged. `NORMAL` and
  `FULL` are reported separately, never conflated. **Neither side has a true
  power-loss receipt** (Phase 20.1 injects process death only).

### Recorded

- **The earlier storage headline was measured against a contract-dead index.**
  The `hist` control carries the Phase-18 baseline's **two** FTS5 indexes though
  the contract has no search observation; removing that dead index accounts for
  **≈+44% bytes / 4.5 MB over 12 docs**. **What survives, shrinks, or vanishes:**
  build advantage **survives, shrinks slightly** (≈5.2× vs control → ≈4.6–4.8×
  vs optimized); storage **survives but shrinks** from the previously reported
  **0.53×** to **0.76–0.80×**; cold **survives** (~0.81×); the **warm advantage
  does not exist** — VOLE is **1.15–1.25× slower** and `full`/`hybrid`/`hist` win
  warm (consistent with ADR-0054); **no capability gap remains**. Strengthening
  the competitor is the point; the shrinkage is the finding.
- **Not measured:** true power loss (both sides); `FULL` on a multi-commit
  workload; a genuinely native DOCX/EPUB hybrid extractor; cross-host results.
- **Corrections propagated** (old numbers kept, marked superseded, forward-pointed)
  in `README.md`, `docs/project/{findings,status,roadmap}.md`,
  `docs/phases/phase-18-results.md`, `docs/phases/phase-19-results.md`,
  [ADR-0053](../adr/0053-batched-packed-sync.md), and
  [ADR-0054](../adr/0054-repeatability-and-paired-measurement.md): the `0.53×`
  storage, `0.82×`/`1.09×` Phase-18, and `1.29×` Phase-19 figures were all
  measured against the **historical control** configuration, not the tuned
  competitor.

## [0.1.0-alpha.25] — Phase 20: hardening and economics

Phase 20 is a **hardening-and-economics phase**. It **adversarially attacks** the
packed store's durability with a 1,300-case fault-injection court (20.1), cuts
large-source peak memory by **33%** so a ~1 GiB source now ingests (20.2),
profiles the warm loss and records it as **durable** (20.3), and closes **C4b**
with an explicit, typed **external** layer rather than contaminating the field
(20.4). Exactness and the wire are untouched: `materialize --exact` 16/16 (20.2),
12/12 + 480 envelopes / 0 mismatches (20.3), 12/12 both lanes (20.4), and
descriptor SHA-256 identical **15/15** (20.2). Results:
[phase-20-results.md](../phases/phase-20-results.md).

### Added

- **`ExternalContext` — external/corpus lineage as a separate typed layer**
  (20.4, [ADR-0055](../adr/0055-external-context-typed-external-metadata.md)):
  `src/field/external.rs` (`ExternalContext { dataset_id, lineage { family,
  member, head, revision_family }, origin harness|operator|catalog, source }`,
  canonical `VOLECTX1`), stored at `<store>/external/<FieldId>`, disjoint from
  the field's `descriptor/index/manifest/seed|fieldpack` and never in the seed
  DAG or exactness authority; removal is one `unlink`. New
  `Selector::ExternalLineage` (`external-lineage`) answered for `--kind lineage`,
  with `Basis::ExternalMetadata` (`is_exact() == false`); CLI
  `observe --external-lineage --kind lineage` and `field-external --store --field
  (--lineage FAMILY:MEMBER:HEAD | --clear)`. No context is a typed decline
  (`UnsupportedFeature`, rc 6).
- **Crash court + non-default `fault-inject` feature** (20.1): `tests/crash_recovery.rs`
  and `tools/phase20-crash-court.sh`; a deterministic in-code abort behind the
  non-default `fault-inject` feature at ten named writer points.
- **Env-gated open profiler** (20.3): `VOLE_PROFILE_OPEN` (off by default; a
  single `getenv` on the default path).

### Changed

- **Large-source memory (20.2), no wire change.** Four files remove a redundant
  source-sized copy from the encode court's decode-before-commit proof:
  `Court::offer` drops the candidate descriptor right after `serialize()`
  (`src/encode/court.rs`); `materialize_in_place`/`take_objects` **move** inline
  object bytes (`std::mem::take`) instead of cloning (`src/materialize/mod.rs`);
  `ingest_verified` (`src/field/mod.rs`) and `ingest_package_direct`
  (`src/field/ingest_package.rs`) use it.

### Measured

- **20.1 crash / power-cut fault-injection court.** Receipt
  `2026-10-08-phase20-crash-47acde7`. **1,300 cases, PASS 1,300 / FAIL 0 /
  CRITICAL 0** under **both** `SyncPolicy::Batch` and `SyncPolicy::Each`
  (650/650 each); families A process death 1,024, B storage corruption 236,
  C deterministic abort 40. `bad_hash = 0`; prefix violations **0**; the
  whole-node re-hash gate rejected **212,812** nodes across **110** cases;
  corruption fails closed typed. **Scope:** proves ordering / no-partial-node /
  prefix recovery / fail-closed, **not** true power loss (page cache survives
  `SIGKILL`) or torn rename (`write_atomic` never dir-fsyncs).
- **20.2 large-source memory.** Receipt `2026-10-08-phase20-memory-7b897ba`.
  RSS/source **5.998× → 3.997×** (−33%); `nasa-pdf-0001` **2342 → 1563 MiB**; a
  1 GiB synthetic **rc 137 (OOM) @6113 MiB → rc 0 @4099 MiB** (~1 GiB now fits
  with ~2 GiB headroom; cap boundary ~1.0 → ~1.5 GiB). Exactness **16/16** after
  (15/16 before); descriptor SHA-256 identical **15/15**; wall unchanged. No cap
  raised. Residual floor: 4 copies; reaching 3 needs a lifetime-borrowing
  `Descriptor` (a wire-type change).
- **20.3 warm heterogeneous query — profiled, no change shipped.** Receipt
  `2026-10-08-phase20-warm-5ed5957`. Dominant term is `Descriptor::parse`
  (~**1.7 ns/B**, ≈590 MB/s) + the full `Field::open` (1.6–2.6 ms; ~45–50% of a
  losing session; 58 µs for the 29 KB doc). A lazy/partial open **cannot** help
  this contract (`probe_eligible` misses the schedule's `metadata`, `revision`,
  and docx/epub selectors), and there is no redundant re-read/re-hash. Paired
  N=100 before/after: median **1.292 → 1.322** (CI half-width ±0.311, MDE ≈0.44)
  — **NULL**; the loss is recorded **durable**. Exactness 12/12; 480 envelopes,
  0 mismatches.
- **20.4 C4b closes under an equal external input.** Receipt
  `2026-10-08-phase20-c4b-5ab2e76`. A plain `--metadata` answer is byte-identical
  before-attach / after-attach / after-clear **12/12**; `materialize --exact`
  matches attached and after removal **12/12**; the external query declines rc 6
  after clear **12/12**; supplied equally to both lanes, both answer C4b **12/12**
  and VOLE's tuple equals SQLite's **12/12**. Cost: sidecar **1001 B**, attach
  **26 ms**, query **8 ms** vs SQLite's **19 ms**; storage 0.49×, build 7.34×,
  warm 1.38×. Residual: C5b batch folding not measured.

### Recorded

- **Durability scope is stated with the evidence.** Ordering, prefix recovery,
  and fail-closed corruption handling are **proven**; loss of un-`fsync`ed
  records under true power loss and torn/lost rename are **not**
  (a Phase-20.1 amendment to [ADR-0053](../adr/0053-batched-packed-sync.md)).
- **The warm ~1.29× loss is durable**, a property of the resident session's
  descriptor parse that decoder authority requires — recorded, not tuned away.
- **Phase 20 changes no wire byte, decode path, or `encode` output.**

## [0.1.0-alpha.24] — Phase 19: repeatability, paired measurement, and the full-population direct build

Phase 19 is a **measurement-discipline phase**. It replaces Phase 18's two
single-run point estimates — the equal-contract build at **0.82×** SQLite and the
warm lane at **1.09×** — with **paired, interleaved, repeated** measurements and a
fixed-seed bootstrap interval (19.1), extends the warm lane to **N=100** (19.2),
and runs the new direct build over the **full frozen `real100-v1` population**
(19.3). The build win **survives** the paired measurement with a CI below 1.0, but
its *magnitude* is **estimator-dependent** (paired median 0.18 vs ratio-of-sums
~0.96); the warm position is a **modest real loss (~1.29×)**, marginal under the
median estimator but resolved under the geometric mean; and the direct path is
**100/100 built and 100/100 exact** on the frozen corpus. No wire byte, decode
path, or `encode` output changed. Results:
[phase-19-results.md](../phases/phase-19-results.md);
[ADR-0054](../adr/0054-repeatability-and-paired-measurement.md).

### Measured

- **19.1 repeatability court (paired, interleaved, N=10).** Re-measured the same
  12-document C0–C5 contract court with N=10 reps of **both** lanes at every
  depth, interleaved, every sample retained; bootstrap 20,000, seed 190019,
  ±10% tie band. **Build established < 1.0:** paired median **0.182** (95% CI
  **0.102–0.228**, entirely below 1); geometric mean 0.203 (0.128–0.395); 11 win
  / 0 tie / 1 loss (the loss is `nist-pdf-0017` at **4.440×**). **Warm not
  resolved:** paired median **1.077** (95% CI 0.809–1.391, includes 1.0);
  geometric mean 1.065 (0.880–1.287); 5 win / 1 tie / 6 loss. Best-of-3 (min) vs
  median-of-10: build **0.977 → 0.963**, warm **1.218 → 1.194** — the min
  reduction did not materially bias either estimate (≤2.4%). Exactness 12/12 both
  lanes. Campaign `2026-10-08-phase19-repeat-d6c8c4c`.
- **19.2 high-N warm repeat (N=100).** Stores built once, then the warm
  one-session lane repeated N=100 per (document, depth, lane), interleaved;
  bootstrap 20,000, seed 190102. Pooled warm medians VOLE **2.25–2.32 ms**
  (CV ≈53%) vs SQLite **1.54–1.65 ms** (CV ≈21–25%). Pooled median paired ratio
  **1.292** (95% CI **0.994–1.658**, includes 1.0); geometric mean **1.283**
  (95% CI **1.069–1.535**, excludes 1.0); per-document 2 win / 3 tie / 7 loss.
  Verdict: **not resolved under the median estimator**, **resolved under the
  geometric mean**. Variance floor: within-cell between-rep CV 5.4% (p90 9.4%);
  median-CI half-width ±0.332; MDE(80%) ≈0.474; an independent identical re-run
  also gave "includes 1.0". Exactness 12/12 both; 480 warm envelopes, 0
  mismatches. Campaign `2026-10-08-phase19-warm-6b66eab`.
- **19.3 direct build over the full `real100-v1`.**
  `field-build --profile runtime --packed --sync=batch` on all 100 documents:
  build **100/100** (pdf 60/60, docx 15/15, epub 25/25; rc histogram all 0 — no
  124 timeouts, no 137 OOMs). The OLD two-step path built 97/100 (`rc124`×2,
  `rc137`×1 on `nasa-pdf-0001`/`0002`/`0003`); the direct path **recovered all
  three**. Wall median **64 ms**, sum 221,408 ms; slowest `nasa-pdf-0003`
  **87.5 s** (under budget). Peak RSS median **30.9 MiB**, max **2342 MiB**
  (`nasa-pdf-0001`, 409 MiB source ≈5.7×) — **memory, not wall, is the binding
  constraint** for sources ≳1 GiB. Persistent **2,829,898,049 B = 1.036×**
  source; 2,691 files / 4,610 dirs. **Exactness 100/100.** Cold coverage text
  92/100, metadata 98/100; all declines typed (`rc 6` for `nasa-pdf-eb-*` text —
  no page 1; `rc 20` `InvalidPackageStructure` for `nist-docx-0011/0012`). vs the
  old path (file-size-corrected; `du -sb` deliberately not compared): on 96
  common docs the direct build is far faster — median **61 ms** vs 2,095 ms
  (paired **0.083×**) — but stores **+5.3%** bytes (1.830 vs 1.738 GB); 15/96
  byte-identical (all epub). Campaign `2026-10-08-phase19-real100-direct-954dbc2`.

### Corrected / recorded

- **The Phase-18 build headline corrects from 0.82× to ~0.96× under the
  ratio-of-sums estimator.** VOLE's total build is dominated by one heavy
  document (`nist-pdf-0017`) while SQLite's is spread, so the 0.82 → 0.96 move is
  **SQLite-lane host variance** (1893 → 1582 ms); VOLE's own build sum is
  reproducible (1545.5 vs 1552 ms). Under the paired per-rep estimator the build
  win is **0.182** with a CI entirely below 1.0 — the build win stands, its size
  depends on the estimator ([ADR-0054](../adr/0054-repeatability-and-paired-measurement.md)).
- **The Phase-18 warm 1.09× re-reads as ~1.29×**, and the N=10 warm estimate
  (1.077) was **under-sampled**: the corrected reading is a modest **real loss**,
  marginal under the median estimator and resolved under the geometric mean.
- **Storage is population-dependent.** On the full population VOLE's bytes are
  **~0.96×** the SQLite `real100` database — **not** the **0.53×** of the
  12-document contract subset; the subset is not representative.
- **Measurement discipline (ADR-0054).** A performance claim must state its
  estimator (paired median/geometric mean vs ratio of sums) **and** its interval;
  samples are retained, and the ±10% tie band is reported alongside.

## [0.1.0-alpha.23] — Phase 18: build-cost programme — the equal-contract build position inverted

Phase 18 is a **build-cost programme**. Starting from Phase 17's direct
`field-build`, it removes the remaining *unnecessary* work from the runtime build
one mechanism at a time — a redundant source materialization (18.2), a redundant
re-serialize of the authority (18.3), and finally the **per-node durability
sync** (18.4 falsifies the file-count hypothesis; 18.5 adopts batched syncs). The
equal-contract build gap falls **10.74× → 7.24× → 0.82×** — VOLE now builds the
12-document subset *faster* than SQLite — while storage stays ~0.5× and cold
queries stay a tie. Results:
[phase-18-results.md](../phases/phase-18-results.md); ADR-0053.

### Added

- **`SyncPolicy { Batch, Each }`** in the packed seed store, exposed as `--sync=batch|each`
  (CLI) and `PackedSeedStore::open_write_with_policy` / `FieldStore::open_packed_with_policy`
  (library) (18.5). `Batch` (the default) syncs once per segment — at **seal** and
  at an explicit **flush** — instead of once per record; `Each` restores the
  pre-18.5 per-record `fdatasync` ([ADR-0053](../adr/0053-batched-packed-sync.md)).
- **`observe-batch --packed`** (18.5). `SessionOptions` gained `packed`, so the
  packed store can serve the one-session warm lane; the 18.4 typed `rc 6`
  rejection is removed.
- **`FieldStore::ingest_verified(descriptor, source, limits)`** plus
  `ingest_pdf_direct` / `ingest_package_direct` (18.2): the direct build verifies
  the materialized authority against the caller's **original source** and scans
  that source once, removing the source → authority → source round trip.
- **`Descriptor::with_observation_index(limits)`** as a pure method, and
  **`encode_with_observation_index`** (18.3): the observation index is attached
  *before* the court's single serialize, removing a source-sized parse and
  re-serialize. `encode` still emits **no** index.

### Changed

- The direct `field-build` path scans the **original** input; `encode` and
  `field-ingest` are byte-for-byte unchanged (they share the same `ingest_parsed`
  tail) (18.2).
- The packed store's default durability policy is now **`Batch`**; `put_field`
  **flushes before publishing a manifest**, so a durable manifest never references
  a non-durable node (18.5).

### Measured

- **18.1 contract court with the direct build** (12 documents). Build wall sum VOLE
  **11,905 ms vs SQLite 1,644 ms = 7.24×** (the two-step `encode`+`field-ingest`
  gap was 10.74×); storage **0.49×** SQLite (7,168,131 vs 14,721,024 B); cold sum
  **779 vs 772 ms (tie)**; warm **1.33×**. Campaign
  `2026-10-08-phase18-contract-direct-f2a34a5`.
- **18.2 one-pass direct build** (9-document direct court A/B). Wall **−6.1%**
  (sum 4012 → 3769 ms), peak RSS median **−28.8%** (15,184 → 10,812 KB); the
  contract gap held at **7.21×**; exactness 9/9 (three closures) and 12/12;
  observations 0 divergent. The removed `RAW` materializations were cheap (a
  `memcpy`), so RSS is the trustworthy win. Campaign
  `2026-10-08-phase18-onepass-61b11350`.
- **18.3 observation index before the single serialize** (9-document A/B). Wall
  **neutral** (direct sum 3723 → 3617 ms, but the untouched control moved +3.8%);
  large-document RSS **−14–16%** (`nasa-pdf-0007` −15.6%, `nasa-epub-0006` −14.2%,
  `nist-docx-0001` −14.5%); exactness 9/9 + 12/12; one-pass authority byte-identical
  to `with_observation_index(encode_with(…))` (witness test). Campaign
  `2026-10-08-phase18-inindex-b7046f0`.
- **18.4 packed as an ingest-write optimization — FALSIFIED.** Packed build
  **11,312 ms** vs same-run fs-direct **11,715 ms** vs SQLite **1,597 ms** → gap
  **7.08×** vs 7.34× (within noise); files collapse (**120/202** vs 8621/9618) but
  `insert` synced **per node** (6792 `fdatasync` vs 6842 `fsync`), so the term is
  sync **latency**, not file count; `observe-batch` still rejected `--packed`
  (rc 6). Storage 7,778,087 B = **0.53×** SQLite. Campaign
  `2026-10-08-phase18-contract-packed-fb23021`.
- **18.5 batched packed-store durability — the win.** Worst doc `nist-pdf-0017`
  packed wall **9392 → 1415 ms**, `fdatasync` **6792 → 0**; same-run 12-document
  contract court VOLE build sum **1552 ms vs SQLite 1893 ms = 0.82×** (VOLE now
  builds **~1.22× faster**; was 7.08×/7.21×/10.74×); storage **0.53×** (7,778,087
  vs 14,721,024 B; 120 vs 8621 files); warm `observe-batch --packed` **1.09×**
  SQLite over C0–C5; policy probe `Batch` 2644 ms vs `Each` 9230 ms; exactness
  `materialize --exact --packed` **12/12**. Campaigns
  `2026-10-08-phase18-batched-sync-14a7e6f` and
  `2026-10-08-phase18-contract-packed-14a7e6f`; [ADR-0053](../adr/0053-batched-packed-sync.md).

### Corrected / recorded

- **The 16.5/17.2 two-step `10.74×` and the 17.1 `field-build 2.04×` are not
  composable.** The composable equal-contract build number is **7.24×** (18.1)
  under the direct `field-build`, before the 18.5 sync batching took it to 0.82×.
- **C4 is two observables.** 18.1 splits it: **C4a document-native lineage** (the
  PDF internal incremental chain) is VOLE's answer (**4/4** PDFs; typed unsupported
  for 8/8 docx/epub) and is byte-derivable from what the baseline retains — a
  *where-the-work-happens* difference, not hidden information; **C4b
  corpus/external lineage** (family/member/head) is dataset metadata the harness
  supplies (baseline **12/12**, VOLE **0/12**, not given it). Recorded as two
  cases, never forced equal.
- **The build wall is dominated by one document's store write.** `nist-pdf-0017`
  is **9425 of 11,605 ms (81%)**, **6842 seed files**, **9.40 s** on the
  bind-mounted store vs **1.39 s** in `/tmp` (18.3). The lever is seed-store write
  amplification — which is what 18.5 then removed (sync batching), **not** a codec
  or parallelism.

## [0.1.0-alpha.22] — Phase 17: direct field build and a revision-lineage surface

Phase 17 attacks the two weaknesses Phase 16 made explicit. It adds a **direct
source → field build path** that skips the compression candidate search the
field does not need, and a **PDF revision-lineage query surface** — then shows
that adding that surface splits the Phase-16.5 C4 decline in two, because the
contract's C4 tuple is external corpus metadata. Released as
`v0.1.0-alpha.22` from `phase17`; base `main` @ `d83ddba`
(`v0.1.0-alpha.21`). Results:
[phase-17-results.md](../phases/phase-17-results.md); ADRs 0051–0052.

### Added

- **`field-build INPUT --store DIR [--profile runtime] [--workers N] [--voldoc OUT] [--entropyfs | --packed]`**
  (17.1). A direct source → field ingestion path that builds the exact authority
  and the field in **one process** with **no candidate search**. `--profile
  runtime` fixes the program to the literal `RAW` floor (one literal object, one
  `EMIT_OBJECT`); exactly one candidate is priced and still passed through the
  full complete-cost court (`candidates_evaluated == 1`). New
  `src/field/build.rs`; `encode`/`field-ingest` are untouched and reused
  internally.
- **PDF revision lineage** (17.2). New `Selector::Revisions` / `Representation::Lineage`
  (`revisions`/`lineage`), CLI `observe --revisions --kind lineage` and
  `observe --revision N --kind lineage`, advertised in capabilities. A new
  `NodeKind::PdfRevisionLineage` is computed **once** at PDF ingest from the
  existing byte-authoritative scan and indexed (`SEL_REVISIONS`,
  `SEL_REVISION_LINEAGE`), so an observe is **O(depth)**. Non-PDF formats are a
  **typed decline** (`UnsupportedFeature`, rc 6).

### Measured

- **17.1 direct build** (`doc-baseline`, 9 documents across pdf/docx/epub and
  four size classes). Build wall sum **7,928 → 3,878 ms (2.04×)**; by format pdf
  **2.56×**, epub **1.34×**, docx **1.22×**. Peak RSS median **51,792 → 15,228 KB
  (3.4× smaller)**. Authority bytes **35,154,409 → 35,383,939 (1.007×)**.
  Exactness **9/9** for each of three closures (current field, direct field,
  direct authority decoded standalone; length + SHA-256 + `cmp`). Observation
  equality over an 11-entry schedule: **53 equal / 46 decline-equal / 0
  divergent**. Campaign `2026-10-08-phase17-direct-field-d83ddba`.
- **17.2 revision-lineage contract court** (12 documents). New surface answers
  the lineage cold and in a mixed batch; **C0–C3 satisfied**, **C4/C5 still do
  not close**. SQLite lane byte-identical to Phase 16.5 (verified by diff). Cost
  vs SQLite: storage **0.47×** (6,988,757 vs 14,721,024 B; lineage adds
  **74,980 B** over 12 documents), build **10.74×**, cold 138 vs 129 ms, warm 37
  vs 25 ms. Campaign `2026-10-08-phase17-revision-1179386`.

### Recorded findings

- **The PDF metadata projection is not encoder-independent** (17.1). For PDF,
  `Selector::Metadata` embeds the chosen program's `object_count`/`graph_ops`;
  a fixed `RAW` program reports 1/1 where the searched winner reported 29/1346
  (`nist-pdf-0002`), 32/763 (`nist-pdf-0004`) and 0/1 (`nasa-pdf-0007`). All
  other metadata fields are byte-identical and packages are unaffected. Recorded,
  not silently changed.
- **C4 is not a pure surface gap** (17.2). The surface half is fixed; the
  contract's C4 tuple is the **corpus family/member/head** — external metadata a
  single-document field cannot derive from PDF bytes. For PDFs the two lanes
  report *different* lineage observables (recorded as `different observable`,
  never equality); for DOCX/EPUB VOLE declines typed while the baseline answers.
- **Residual risk:** lineage fidelity is bounded by the Phase-3 `%%EOF` scanner
  (on `nist-pdf-0016` it splits at an embedded early `%%EOF` at offset 505 and
  reports an inverted `/Prev`) — not an independent PDF-conformance oracle.

### Open question

- The honest next question is a **contract-definition** one, not a missing
  surface: is the contract's C4 tuple the right definition, or should VOLE
  ingest corpus/revision-family metadata as an **explicit external input**?
  Recorded in [ADR-0052](../adr/0052-revision-lineage-surface.md); no code or
  wire change decided.

## [0.1.0-alpha.21] — Phase 16: backend adoption, large-PDF fix, and a storage-accounting correction

Phase 16 follows Phase 15: it adopts the inflate backend Phase 15 only
recommended, finishes the packed-storage court on the **full** `real100-v1`
population, fixes the **>100 MiB PDF encode pathology** that court surfaced,
continues the residency line, and tests whether **SQLite loses under an equal
capability contract**. It closes with the measurement correction that removed a
false VOLE storage advantage. Released as `v0.1.0-alpha.21` from `phase16`; base
`main` @ `ebb6636` (`v0.1.0-alpha.20`). Results:
[phase-16-results.md](../phases/phase-16-results.md); ADRs 0049–0050.

### Changed

- **`zlib-rs` adopted as the shipped DEFLATE inflate backend** (16.1). One helper
  `src/field/inflate.rs` owns every inflate, with RFC 1950 (zlib) vs RFC 1951
  (raw) selected by a `Wrapper`. `tests/deflate_backend_equivalence.rs` is the
  byte-identity witness. End-to-end median `field-ingest` **0.917×** (~8 %
  faster); `encode` **0.996×** (control/noise floor); peak RSS **1.000×**.
- **`propose_rle` streams instead of pre-allocating** (16.3). The RLE candidate
  built its `Vec<(u8,u64)>` (~16 B/run) *before* its `max_graph_ops` decline; it
  now counts in **O(1) memory** and materializes ops only once admitted. Peak/
  input on the large PDFs **17.7× → 8.9×**; no `.voldoc` byte changed.

### Fixed

- **The `>100 MiB` PDF encode pathology is fixed** (16.3). `nasa-pdf-0001`
  (409 MB) now completes **byte-exactly** (rc 0, ~40 s); `nasa-pdf-0002`/`0003`
  peak RSS roughly halve and now exceed only the **wall** op budget, not memory.
  Before/after `.voldoc` SHA-256 identical **77/77**; all gates pass.

### Measured

- **16.1 `zlib-rs` end-to-end** (3-document court, one process per op):
  `encode` 2,510 → 2,500 ms (`0.996×`); `field-ingest` 1,200 → 1,100 ms
  (`0.917×`); combined 3,710 → 3,600 ms (`0.970×`); peak ingest RSS `1.000×`.
  Byte-identity **2,075 members / 17 documents**, `materialize --exact` PASS on
  every row. Campaign `2026-10-08-phase16-zlib-ebb6636`.
- **16.2 full packed court.** Full `real100-v1`: encode 97/100, `fs` and packed
  `field-ingest` 97/100, A1 build 98/100; field id `fs`==packed 97/97; head-to-head
  **95** common-success documents. Failures: `nasa-pdf-0001` encode rc 137 (fixed
  in 16.3), `0002`/`0003` rc 124, two A1 builds rc 1 (pre-existing baseline XML
  parse errors). Campaign's `du -sb` byte numbers are **superseded by 16.6**.
  Court `2026-10-08-phase16-packed-full-0b21928`.
- **16.3 large-PDF fix.** `nasa-pdf-0001` rc 137 → **rc 0** (40 s, exact);
  `0002` 5,356,860 → **2,696,872 KB**; `0003` 3,758,292 → **1,906,148 KB**;
  forced `rle` 6.28 GiB OOM → **401,524 KB** (declines). Campaign
  `2026-10-08-phase16-largepdf-ce8af8f`.
- **16.5 contract-equivalent court.** Under the same escalating contract C0–C5
  (12 documents): equality at C0–C3, **VOLE declines C4/C5** (no revision query
  surface); SQLite builds **~10×** faster, serves the warm session **~1.47×**
  faster, and escalating C0 → C4 costs it only **~+3 %** persistent bytes with
  flat query cost. Campaign `2026-10-08-phase16-contract-45d2c0e`.

### Recorded negatives

- **16.4 residency with `narrow_probe`** (ADR-0042 extended). `narrow_probe` was
  refactored into a shared core and `observe_session` gives the resident session
  the short-circuit; the probe works (observations 2..N: `descriptor_bytes_read
  = 0`, partial, ~25 µs, 208/360 hits; answer equality **90/0**), but **no size
  class flips**: cold median **6 ms** vs resident **9 ms**, sums 3,092 vs
  3,894 ms. Mechanism: the one-time full `Field::open` descriptor parse
  (`repeat=1 == repeat=5 == 0.14 s`); the isolated lever is a **lazy session
  open** (recorded, out of scope). Campaign
  `2026-10-08-phase16-resident-probe-5d331f2`.
- **16.5 SQLite does not lose under the equal contract** (ADR-0050). VOLE's
  lone edge is storage, and 16.6 corrects even that to ~0.9×; the decisive depth
  is **C4** revision lineage, which VOLE cannot answer. Recorded as an open
  architectural question, not a switch.

### Corrected

- **16.6 storage accounting** (ADR-0049). `du -sb` counted **4096 B per
  directory inode**: VOLE `fs` = 1,723,650,951 file bytes vs 2,620,072,839 `du`
  (52 % overhead, 218,853 dirs); packed 1,738,386,483 vs 1,752,767,539 (0.8 %,
  3,511 dirs); A1 db 0. Corrected headlines: 15.3 packed/`fs` **0.719× →
  1.007×**; 16.2 `fs`/SQLite **1.377× → 0.906×**, packed/SQLite **0.921× →
  0.914×**, packed/`fs` **0.669× → 1.009×**. **Refuted:** "VOLE is 1.377×
  SQLite" and "packed closes the gap" — there was no byte gap; both VOLE
  backends are at/below SQLite on file bytes, and the packed win is
  file/directory **count** (3,511 vs 218,853 dirs). Campaign
  `2026-10-08-phase16-storage-correction-2978e1d`.

## [0.1.0-alpha.20] — Phase 15: performance programme

Phase 15 repairs the performance *measurement* first, then measures whether the
lifetime economic frontier can be earned. Released as `v0.1.0-alpha.20` from
`phase15`; base `main` @ `c6977cd` (`v0.1.0-alpha.19`). Results:
[phase-15-results.md](../phases/phase-15-results.md); ADRs 0042–0048.

### Added

- **Packed seed store** (`--packed`): an optional, immutable, segmented `fieldpack`
  backend for the seed namespace (`NodeId -> (segment, offset, len)`); identity is
  unchanged, so field ids are unchanged (ADR-0043).
- **Resident session** (`DocumentFieldSession` + `observe-batch`): many observations
  in one process, one JSON answer per line (ADR-0042).
- **Bounded parallel ingest** (non-default feature `parallel`, `--workers N`) —
  a bounded Rayon pool over the independent ingest sites (ADR-0044).
- **DEFLATE ablation harness** (feature `deflate-ablation`; deps `zlib-rs`,
  `zune-inflate`; `examples/deflate_ablation.rs`) and `miniz-simd`, plus the
  `memmem-scan` `memchr::memmem::Finder` replacement for the PDF `find_endstream`
  scan (ADR-0045).
- **Adaptive promotion** (`--promote[=BYTES]`) — opt-in, default-off, never on the
  exactness path (ADR-0046).

### Measured

- **15.1 Court repair.** The frozen `real100-v1` court re-run on the **release**
  binary with the storage universes split: VOLE 455 answered/245 declined, a1
  506/194, a0 508/192; `materialize --exact` 97/100 (v), 98/100 (a1), 100/100 (a0).
  Storage: persistent `2,667,668,262 B`, descriptor `1,704,524,849 B`, A1 db
  `2,891,784,192 B` (whole-population `du -sb` aggregates; the store figure is
  method-inflated — ADR-0049). VOLE holds `pdf`/`text_repeat` and `docx`/`table`; everything
  else loses to SQLite/FTS, `exact` loses to the source file. Campaign
  `2026-10-07-real100-release-baseline-866f489`.
- **15.3 Packed store (win).** File/directory count `0.009x` (111x fewer) at
  **byte parity** (file bytes `1.007x`); latency parity; field id identical 12/12,
  byte-exact 12/12 both (`2026-10-07-phase15-packed-8c195e8`). *(Corrected: the
  earlier "persistent bytes `0.719x`" was a `du -sb` directory-inode artifact;
  ADR-0049.)*
- **15.5 DEFLATE ablation (recommendation).** `zlib-rs` **1.58x** GB/s at 1.00x RSS,
  byte-identical — **meets** the pre-registered bar and is the recommended backend
  swap (not yet adopted); `miniz-simd` (1.11x) enabled as a free, output-preserving
  change (`2026-10-07-phase15-deflate-e676166`).
- **15.4 Parallel ingest (determinism positive).** Median speedup `1.00x` at 2/4/8,
  `0.99x` at 16; field id identical across every worker count 10/10 and
  `materialize --exact` == source 10/10 (`2026-10-07-phase15-workers-122c026`).

### Recorded negatives

- **15.2 Residency (negative/partial).** The resident lane wins only below ~1 MiB;
  the cold lane is faster at 1–100 MiB (aggregate `text_repeat` cold 7 ms vs
  resident 9 ms), because the cold path's `narrow_probe` short-circuit is lost
  above ~1 MiB (`2026-10-07-real100-release-resident-78f7ea8`; ADR-0042).
- **15.6 Adaptive promotion (negative).** All three pre-registered falsifiers fire;
  promoted bytes cut durable bytes **0.0%** (bar 20%); `sq_full` fastest at every
  depth; mechanism ships opt-in/default-off (`2026-10-07-phase15-diversity-4786f8e`,
  `…-revision-4786f8e`; ADR-0046).
- **15.7 Durable cross-root derivations (negative; `N3` violated).** Cross-member
  derived reuse **0 nodes**; post-`cache --clear` reuse above the floor **0**; only
  representation identity is shared; no Rust change was made
  (`2026-10-07-phase15-crossroot-7429d61`; ADR-0047).
- **15.1 Large PDFs.** 3 of 5 `>100 MiB` documents still fail at `encode`
  (`nasa-pdf-0001` OOM rc 137; `0002`/`0003` timeout rc 124); 2 succeed (`0020`,
  `0024`). The Phase-14 bound is unchanged (ADR-0041). `perf` is absent from the
  lane, so no perf-class counters are claimed.

### Deferred

- **15.8 CUDA batch lane** is **not measured**: the bandwidth gate is unopened, the
  pinned Docker lanes on this host cannot see the GPU (no NVIDIA runtime
  registered; `docker info` lists only `runc`), `nvCOMP` is proprietary and not on
  the `deny.toml` allow-list, and the repo requires Docker-reproducible evidence.
  An explicit, reasoned deferral (ADR-0048;
  `research/subagents/phase-15/design-15.8-cuda.md`).

### Fixed

- Stale documentation reconciled: `README.md` `## Current status` (previously
  `alpha.16` / "Phase 13 in progress") and the `docs/project/status.md` release
  line.

## [0.1.0-alpha.19] — Phase 14: large-PDF encode bound (partial fix)

The `real100-v1` frontier court's second failure region: encoding the largest PDFs
(168–409 MB) timed out or was OOM-killed under the 6 GiB lane. Phase 14 narrowed
it and improved the encoder, but did **not** solve the memory bound (ADR-0041).

### Changed

- **One shared PDF physical scan across the candidate portfolio.** Each PDF
  proposer used to re-scan; `propose_each` now scans once and passes `&PdfPhysical`
  to `*_with` variants. `lookup_body` becomes a `HashMap` index (was
  `O(streams x bodies)`) and dict matching becomes a one-pass stack table (was
  `O(spans)` per `<<`).
- **Streaming court.** `court::Court` prices candidates one at a time; encoder
  memory no longer holds every candidate payload at once.

### Measured (frozen architecture, `real100-v1`, all-features, 6 GiB lane)

- `nasa-pdf-0003` (217 MB): **395 s -> 236 s**, byte-exact (`BYTE_RANS`).
- `nasa-pdf-0002` (308 MB): **180 s timeout -> 276 s completed**, byte-exact.
- `nasa-pdf-0024` (169 MB) 39 s (`PDF_CHANNELS`), `nasa-pdf-0020` (178 MB) 16 s
  (`PDF_COS_TEMPLATE`), both byte-exact.
- `nasa-pdf-0001` (409 MB): **still OOM-killed** (peak 6.28 GiB > 6 GiB).

### Recorded negative

- **The peak-memory bound is unchanged (~17x input).** The still-failing document
  is unchanged, and the improvements are encoder-side only (no wire/decoder
  change). The next step — attributing the ~17x peak to a specific generator — is
  recorded in ADR-0041, not done. The court's own gates are not crossed (236 s
  still exceeds its 180 s op budget).

## [0.1.0-alpha.18] — Phase 13.7: benign `DOCTYPE` (real-corpus EPUB fix)

The `real100-v1` frontier court found that the frozen EPUB adapter declined **all**
content observations on real books — every NASA EPUB has spine content documents
carrying the standard XHTML `<!DOCTYPE …>`, which the bounded-XML policy refused
outright. 13.7 splits the policy (ADR-0040).

### Fixed

- **Benign `DOCTYPE` accepted, internal subsets still refused.** A declaration
  without an internal subset (bare, or with a `PUBLIC`/`SYSTEM` identifier) is
  accepted and ignored; a declaration with a subset (`[…]`) is refused. Entities
  are never resolved and the external identifier is never fetched, so the no-DTD
  property is preserved; hostile fixtures (internal subsets) still decline.
  Parse-side only: no decoder behavior, no wire change.
- **Real EPUB content parses.** Re-running the frozen frontcourt over `real100-v1`
  (campaign `2026-10-07-real100-frontier-c14e06f`), the VOLE EPUB declines fall
  **62 → 6** and VOLE ops answered rise **399 → 455**. The residual 6 are
  non-DOCTYPE (a bare `&` in an attribute; an EPUB with no level-0 heading).
- Removed the dead, wire-neutral `max_xml_doctype` limit.

### Review

- An **independent adversarial subagent** pass was run and recorded
  (`docs/reviews/phase-13-skeptic-review.md`, `research/subagents/phase-13/`). It
  confirmed 13.1–13.5 and the `real100-v1` numbers, could not break 13.7's
  security property, and overstated/falsified four items — all corrected (the
  13.7 motivating wording, the 13.7 effect, a factually wrong sentence about the
  largest successful PDF descriptor, and under-disclosed A1/storage facts).

## [0.1.0-alpha.17] — Phase 13: closing the remaining proposals and the `N5` gate

Phase 13 closes the Phase-12 `PROPOSED` items and the last open skeptic gate.
Every subphase is measured with the same exactness invariant
(`materialize(descriptor) == original_bytes`); the wire is unchanged (`dra-8`,
`FORMAT_MINOR` do not move) and every negative result is recorded.

### Added

- **ODT adapter** (`src/adapter/odt.rs`, feature `odt`) — a bounded OpenDocument
  (ODF) inverse over the shared ZIP + bounded-XML layers. Resolves the main content
  part semantically from `META-INF/manifest.xml` (never a hardcoded path) and reuses
  the ZIP layer but **not** the OPC graph (ODF is not OPC). Byte-exact and queryable
  after source **and** descriptor deletion in a fresh process; typed declines on a
  missing/malformed manifest with exactness preserved (ADR-0038).
- **Optional `CHECKPOINT` record** (`src/container/checkpoint.rs`, `RecordTag`
  `0x60`, ignorable) — validate-or-decline, falls back to the observation index
  (ADR-0039, recorded negative).

### Measured

- **13.1** PDF `/Length`/revision as a *size* mechanism: byte-exact, **0 wins** vs
  the ladder and vs generic (ADR-0036).
- **13.2** PDF `PDF_COS_TEMPLATE` grammar/templates: byte-exact, a scoped VOLE-ladder
  win (4/28 files) but **0 wins vs generic** (ADR-0037).
- **13.3** ODT adapter: **adopted** (ADR-0038).
- **13.4** Byte-level checkpoints: byte-exact and advisory, but **redundant with the
  observation index** (16 B/op vs 9 B/op; +259/+439/+1,159 B per byte-range query,
  identical op work) — recorded negative (ADR-0039).
- **13.5** Gate `N5` (package-index-only): the literal mechanical control
  (`zipfile.read`/`unzip -p` + byte substring) answers **0/72** structural selectors
  while the field answers **72/72** — `N5` **falsified**, gate closed
  (`2026-10-07-phase13-n5-21948bd`).

### Fixed

- Stale documentation reconciled: the `real100-v1` corpora caveat, the
  ODT-is-ODF-not-OPC wording, the Phase-13.3 results section, and exact receipt ids.
- Default-feature `cargo clippy --all-targets -- -D warnings` re-verified clean (the
  `package`-only items in `src/field/document_format.rs` are feature-gated).

### Reproducibility

- `compose.yaml` OOM containment is now machine-checked: `tools/check-compose-caps.sh`
  fails if any service lacks `mem_limit == memswap_limit` + `pids_limit`, wired into
  `tools/check-docs.sh` and a CI `integrity` job.

### Corpus

- **`real100-v1`** — a frozen 100-document NASA/NIST corpus (60 PDF / 15 DOCX /
  25 EPUB), selected independently of VOLE performance and frozen by SHA-256.
  Document bytes are fetched on demand and are not committed; the manifest,
  `SHA256SUMS` and byte lengths are.
- **First frontier court** over the frozen corpus, with the frozen architecture
  and no tuning: `tools/real100-court.sh` maps VOLE vs a source-retaining
  SQLite+FTS5 baseline vs direct tooling. Mixed result — VOLE wins repeated
  observations and DOCX tables/metadata; it loses cold one-shot lookups *and* two
  structural regions: real EPUB content (the bounded-XML policy forbids the XHTML
  `DOCTYPE` these files carry — 62 declines) and >100 MiB PDFs (encode OOM/timeout
  under the lane cap). Exactness: VOLE 97/100, SQLite/FTS 98/100, direct 100/100.
  Report: `docs/evidence/real100-frontier-report.md`.

### Review

- Phase-13 adversarial review (`docs/reviews/phase-13-skeptic-review.md`): the
  13.1/13.2 aggregate totals are **not comparable across file sets** and must not
  be quoted as a size ratio (per-file verdicts unchanged). The independent-review
  gate is **partial**: the mandated independent skeptic was canceled, so the review
  is an in-session pass by the author's reasoning path and is recorded as such.

## [0.1.0-alpha.16] — Phase 12: universal multi-format document field

Phase 12 makes the persistent procedural field **format-universal**. PDF, DOCX and
EPUB now enter through separate **native inverse compilers** — the Phase-11 PDF
adapter, a WordprocessingML (DOCX) inverse and a bounded-XHTML/OCF (EPUB) inverse —
built over a shared **byte-authoritative ZIP** layer (a physical member scanner plus
an OPC/OCF package graph), and all three converge on **one** persistent
`DocumentField` exposing **common** observations *and* retained **format-native**
structure. Exactness is unchanged and remains the only normative profile:
`materialize(descriptor) == original_bytes` (length + SHA-256 + `cmp`). The wire is
unchanged (`dra-8`, `FORMAT_MINOR` do not move) and no decode-path semantics changed.
The courts are deliberately mixed; every loss, tie and negative below is recorded.

### Added

- **Byte-authoritative ZIP layer** (`src/adapter/package/`) — a deterministic
  physical member scanner (cover = exact partition; spans, central directory,
  ZIP64, data descriptors, prefix/trailing bytes, CRC) and an OPC/OCF package graph
  (content types, part/relationship resolution; cycles and depth bounded).
  Feature-gated behind `package` / `opc`.
- **DOCX inverse** (`src/adapter/docx/`, feature `docx`) — a bounded
  WordprocessingML projection: story structure, paragraphs/runs, tables/grids,
  tracked changes, notes, textboxes.
- **EPUB inverse** (`src/adapter/epub/`, feature `epub`) — a bounded OCF/XHTML
  projection: package summary, spine/reading order, blocks, links and resources.
- **Format-agnostic field ingest + common vocabulary** — one `field-ingest` and one
  common CLI (`capabilities`, `find`, `text`, `metadata`, `observe`, `explain
  [--analyze]`) over PDF+DOCX+EPUB, with per-answer provenance
  (`format=<fmt>;common;<native>`); format detection is byte-based.
- **Feature-gated Phase-12 courts** — `examples/phase12_share_court.rs`
  (`docx`+`epub`), `tools/phase12-*.sh`, `tests/phase12_courts.rs`,
  `tests/phase12_lifetime.rs`, `tests/phase12_security.rs`, and 8 new bounded fuzz
  targets (`zip_scan`, `zip_decode`, `opc_rels`, `docx_wml`, `epub_package`,
  `epub_content`, `xml_part`, `common_observe`).

### Measured

- **Exactness after source + descriptor deletion.** The removal court deletes the
  private source and the `.voldoc` descriptor, then rematerializes each format in a
  **new process**, asserting length + SHA-256 + `cmp` against a sealed oracle:
  **38/38 assertions pass** (PDF, DOCX, EPUB).
- **Triplet court (12.9).** One known logical report emitted as `.pdf`/`.docx`/
  `.epub` and queried through the **one** common CLI: **96/96 assertions pass**; the
  same logical answers carry different native provenance. *(Ground truth is
  generator-defined — self-authored consistency, not third-party independence.)*
- **Ablation ladder (12.11b).** `A1b`, `A2`–`A6`, `A9`, `A11` are measured lanes;
  `A7`/`A8` are recorded **not separable**; `A10` is measured ingest-side; §106 is
  not separable; §107 is proxied by A5 (`--no-cache`) vs A6. The win is attributed
  to the **content adapters** (A4→A5: the DOCX/EPUB answered set moves 1/12 →
  11–12/12 cases) and to **persistent semantic reuse** (A5→A6), which **trades
  bytes for CPU** (`delta.docx` warm N=1000 CPU 4590→1480 ms while process reads
  66→330 MB). The ZIP/OPC rungs (A2→A3→A4) do not move the frozen schedule's
  answered set. **EntropyFS (A9) is a loss**: `delta.docx` reads 694 MB vs A11's
  331 MB.
- **Mixed lifetime result (12.11).** VOLE owns the **small-document** frontier
  (wall/CPU/bytes vs A0, and vs the source-retaining SQLite+FTS5 baseline A1 on
  small documents) and the cold one-time comparison, but A1 wins the **large ~60 KB
  synthetic** documents' byte frontier from N=10 (`delta.docx`), N=100
  (`delta.epub`) and N=1000 (`delta.pdf`), and wall/CPU at N=1000 for
  `delta.docx`/`delta.epub`.
- **LLM working set (12.12).** Pinned, offline `bert-base-uncased` (WordPiece voc-30522,
  hash-verified at court time). V vs the page-local extract B1: **1 win / 8 tie /
  3 loss**; V vs the whole document B0: **9 win / 0 tie / 3 loss**.
- **Security + fuzzing (12.13).** **315/315 court assertions pass** over 45 hostile
  fixtures (**16** typed `reject`, **22** `opaque-preserve-and-decline`, **7**
  `accept`; every fixture `materialize --exact`); library court 15/15; the 8 new
  fuzz targets all `exit=0` with no crash/OOM/timeout.
- **`N6` PDF no-regression — closed.** A2 (the Phase-11 PDF field) vs A11 (the
  unified field) over 16 corpus PDFs / 128 `explain --analyze` observations:
  **32/32 lane-documents byte-exact, 0 regressions**.
- **FTS5 amendment.** A real FTS5 `trigram` index answers identically to `LIKE` here
  but reads *more*; the whole-token `unicode61` index misses embedded markers. No
  "FTS5 is faster" claim is made.

### Recorded losses and negatives

- **`N3` cross-document durable reuse — VIOLATED (negative).** The warm
  `retained_inverse_work_fraction` (**0.339907**) drops to **0.0** after
  `cache --clear`, both in-process and in a fresh OS process; the reuse was served
  by the on-disk derived cache, not by durable seed-store work. Representation
  identity is still genuinely shared (`nodes_id_shared=2`, `shared_resource_ids=1`),
  and the strongest raw CDC baseline saved `−1374` bytes. Cross-document **work**
  reuse is a recorded negative.
- A1 wins the large-document byte frontier (N=10–1000) and wall/CPU at N=1000.
- VOLE's one-time read is **2.3×–22.6×** the source; its store exceeds A1's `.db` on
  `delta.pdf`/`delta.docx` (but is smaller on `delta.epub`).
- VOLE **declines** DOCX exact resource bytes; PDF has **no** native provenance and
  no heading/table/cell/resource/link coordinate (typed declines); there is no
  cross-format document-title observation.
- PDF page text **loses to Poppler** in tokens on 3/4 PDFs; DOCX/EPUB narrow token
  answers **tie** the local extract (they win only against the whole document).
- `N4` (decline-rate threshold) is **not evaluated** — no pre-registered threshold
  exists; the 22/45 decline rate is a hostile-input property, not a capability.
- EntropyFS (A9) is a loss on this lifetime frontier.

### Fixed

- Default-feature `cargo clippy -- -D warnings`: the `package`-only constants and
  helper in `src/field/document_format.rs` are now gated behind
  `#[cfg(feature = "package")]`.

### Notes

- An independent adversarial review
  ([`docs/reviews/phase-12-skeptic-review.md`](../reviews/phase-12-skeptic-review.md))
  re-checked every headline; its corrections and amendments are applied — the
  security class tally, the crossover/store scoping, the withdrawal of the
  unmeasured FTS5 claim, the now-run ablation ladder, the sealed PDF no-regression /
  reuse-controls / FTS5 / demo receipts, and the default-feature clippy fix. The
  authoritative results are
  [`docs/phases/phase-12-results.md`](../phases/phase-12-results.md).

## [0.1.0-alpha.15] — docs/status tables

Documentation-only release. **No behaviour change; exactness is unchanged.** The
normative profile is still `materialize(descriptor) == original_bytes` (length +
SHA-256 + `cmp`), the wire is unchanged (`dra-8`, `FORMAT_MINOR` do not move),
and no feature, decoder, or candidate semantics changed. The crate is
republished so that crates.io and docs.rs reflect the corrected status tables
already on `main`.

### Changed

- **Render-visible status tables corrected** (`README.md`, `PROJECT_STATE.md`):
  removed a stale "Planned" row, reclassified partial-materialization checkpoints
  as **SUPERSEDED** by the Phase-11 observation engine, marked nested-content
  proceduralization **PARTLY DELIVERED** (observation), added the Phase-11 rows,
  and fixed a stale "(Phases 7+)" note. These are accounting corrections to the
  ledger; no mechanism, representation, or measurement changed.

## [0.1.0-alpha.14] — docs/metadata

Documentation and package-metadata release only. **No behaviour change; exactness is
unchanged.** The normative profile is still `materialize(descriptor) == original_bytes`
(length + SHA-256 + `cmp`), the wire is unchanged (`dra-8`, `FORMAT_MINOR` do not move),
and no feature, decoder, or candidate semantics changed. The crate is republished so that
crates.io and docs.rs reflect the metadata and docs already on `main`.

### Changed

- **README rewritten to the Phase-11 reality.** It now presents the project as a
  *persistent procedural document runtime* and reports the scoped measured wins, the
  recorded losses/ties, and the skeptic corrections rather than a headline-only claim.
- **Crate description/keywords/categories updated** (`Cargo.toml`): description now names
  the persistent procedural document runtime; keywords
  `["document", "pdf", "procedural", "reconstruction", "queryable"]`; categories
  `["encoding", "filesystem", "data-structures"]`.
- **`lib.rs` crate doc updated** to match the Phase-11 surface (persistent procedural
  document field, observations with provenance, selective late materialization) while
  staying explicit that exactness remains the only normative profile.

## [0.1.0-alpha.13] — unreleased

Phase 11 — **the persistent procedural document field**. A content-addressed,
queryable field over the exact `.voldoc` descriptor: a fine-grained procedural
seed DAG, a bounded hierarchical observation index, and a typed observation
query engine with provenance and `EXPLAIN`. Exactness is unchanged and remains
the only normative profile (`materialize(root) == original_bytes`; length +
SHA-256 + `cmp`, with the source removed and in a new process). The field is a
reconstruction of *observations over* the exact archive — it is never on the
decode path and never a substitute for it. Docs + version only: `dra-8` and
`FORMAT_MINOR` do not move.

### Added

- **Phase 11.2 — persistent procedural entropy seed DAG** (`src/field/node.rs`,
  `dag.rs`; `src/store/seed.rs`; ADR-0025). Canonical, immutable `SeedNode`s with
  `NodeId = BLAKE3-256("VOLE:PSEED:v1" || canonical state || canonical dependency
  ids)`, domain-separated from the object table's `Id`. Green = present with a
  complete id-matching closure; red = absent; no mutation and no invalidation
  pass — the content id *is* the fingerprint. `SeedStore` with the reference
  `FsSeedStore` (atomic tmp→sync→rename, range reads) and the optional
  `EntropyFsSeedStore` (one blob per node) behind the existing `entropyfs-store`
  feature.
- **Phase 11.3 — hierarchical observation index** (new optional record
  `HIER_INDEX = 0x72`; `src/field/index.rs`; ADR-0024/0026). Bounded, advisory,
  and re-derivable: a lying, corrupt, cyclic, out-of-closure, or oversized index
  is rejected fail-closed; a missing index falls back to the honest prefix path.
- **Phase 11.4–11.6 — progressive inverse-proceduralization, observation engine,
  and selective late materialization** (`src/field/ingest.rs`, `observe.rs`,
  `plan.rs`, `explain.rs`, `provenance.rs`, `cache.rs`; ADRs 0024, 0026, 0027).
  Typed Rust API + CLI (`field-ingest`/`query`/`observe`/`find`/`explain`/
  `preview`), a typed `FieldAnswer { basis, scope, dependency ids, source spans }`
  with `basis ∈ {authored, directly-observed, deterministically-derived,
  inferred, heuristic, unresolved}`, and `EXPLAIN` / `EXPLAIN ANALYZE`. A
  deterministic planner and per-component late-materialization frontier mean a
  narrow observation need not rebuild the document; no agent/LLM/model is ever on
  the decode path.
- **Phase 11.8 — persistent computation reuse** (ADR-0025). A repeated query in a
  fresh OS process reports `seed_nodes_executed = 0`. The reuse is served by the
  **persisted derived cache** (off-wire, closure-keyed, disposable, and counted as
  a fourth accounting universe); after `cache --clear` a fresh process
  re-executes the nodes. The claim holds *with the derived cache present*.
- **Phase 11.12 — optional immutable edit witness** (`src/field/edit.rs`;
  declared narrow subset only). A node-level page-content override derives a new
  root that shares everything unaffected **by content id** — the descriptor blob
  and the `DocumentExact` root id are neither read nor rewritten, only their
  content ids are written into the new manifest. Limits: one page per call,
  ≤ 48 KiB content, no insert/delete/reorder, descriptor unchanged,
  `materialize(R1) == original` trivially.
- **Phase 11.14 — finer-than-object shareable units** (`src/field/share.rs`, the
  `share-account` CLI; ADR-0028), externalizing channel payloads / sub-object
  chunks as their own units.

### Measured — Phase 11 courts (recorded wins, ties, and losses)

- **Exactness after source removal (win — the invariant).** For all three court
  cases, with the source PDF deleted, a new process answered queries and
  `materialize --exact`ed a file with matching length, matching SHA-256, and
  `cmp` byte-equality (`large-50` 33,571,029 B; `large-400` 33,673,730 B;
  producer 74,371 B). Receipt `2026-10-06-phase11-63f43fb`.
- **Fair baselines vs A1 preprocessed SQLite and A0 raw tooling (losses
  recorded).** A page-text lookup from the indexed SQLite baseline reads
  **24,393 B** (18 syscalls) in ~1 ms; A0 `pdftotext -f 1 -l 1` reads
  **136,128 B** and returns 91 B of text. A cold VOLE `observe` reloads its
  descriptor closure (**35,686,961 B** `large-50`, **34,767,035 B** `large-400`,
  **169,126 B** producer via `read`/`pread64`). The SQLite one-time cost is
  charged in full (400 pages extracted with `pdftotext` in 1,612 ms into a
  53,248 B database). VOLE is not a whole-file compressor (ADR-0017): xz -9e on
  the 33.67 MB source is 5,691,168 B vs the best complete `.voldoc` 17,283,279 B.
- **Lifetime court** (1 / 10 / 100 / 1,000 queries, fresh process per query;
  `2026-10-06-phase11-lifetime-5a7edd3`) — **mixed.** VOLE crosses A0 raw tooling
  on wall on every document, but crosses the preprocessed SQLite baseline on wall
  only on the two smallest documents at N=1000 and **loses on cairo-vector,
  libreoffice-export, and large** (the A1 wall crossover is absent on 3 of 5
  documents). VOLE crosses A1 on bytes-read on the four small documents (N=100)
  but never on `large`. Ingest amortization at N=1000 ranges 7.5%–67.2%.
- **Descriptor-free warm path (priority #2) — overhead-only, not a process-level
  byte win** (`2026-10-06-phase11-desc-free-a8ad6f4`). Warm narrow observations
  read **0 descriptor bytes**; the **8.4–8.9 KB** procedural-overhead figure
  excludes the cached answer payload the warm process physically re-reads
  (65.9 KB on `large-400`, 527 KB on `large`), so the byte win over A1 is
  withdrawn on the large cases. The warm **wall** win (99 µs vs A1 ~1 ms / A0
  ~8 ms) and the vs-A0 win stand.
- **LLM working set with a pinned offline tokenizer**
  (`2026-10-06-phase11-llm-tokens-824faa9`). `bert-base-uncased` (WordPiece,
  vocab 30522), loaded by the hash-pinned `tokenizers==0.20.3` runtime from a
  vendored asset whose SHA-256 is verified **at court time**; the court never
  touches the network and `add_special_tokens=false`. Against page-local Poppler
  B1: **2 win / 2 tie / 2 loss**; against whole-document B0: 4 win / 2 loss.
  Token counts are tokenizer-specific; a smaller V is a **working-set**
  measurement, never a text-quality claim.
- **Finer-than-object sharing loses to CDC (recorded negative; ADR-0028).** Over
  4 real producer documents + a byte-identical repeat + a few-bytes-changed
  near-duplicate (8 files, 337,877 B), the fine-unit unique **lower bound**
  (208,001 B) loses to the strongest content-defined chunking (160,668 B) and to
  `tar | xz -9e` (92,752 B), with a per-stratum loss on the near stratum.
  `unique_bytes` is explicitly a lower bound (excludes 8,435 B of
  root/program/GRAPH framing). Receipt `2026-10-06-phase11-share-d9f818a`.
- **Immutable edit witness (scoped win).** 318 index entries carried forward by
  id, 2 new seed nodes, `descriptor_bytes_read = 0`; cost 43,688 B of index read
  vs 8,776 B written (recorded as a loss). Receipt
  `2026-10-06-phase11-edit-8cceaac`.

### Corrected — independent adversarial skeptic review (Phase 11.13)

- An independent review (`docs/reviews/phase-11-skeptic-review.md`; receipt
  `2026-10-06-phase11-skeptic-9bd766d`) attacked every headline claim. Corrections
  applied in place: the warm descriptor-free byte "win" is **overhead-only** and
  is withdrawn vs A1 on the large cases; the cold working-set headline is the
  **449 B seed class only** (the honest total cold observation is 232–364 KB —
  descriptor closure + index — so "bounded" means page-closure-bounded, not
  O(1)); cross-process reuse is **cache-served**, not seed-DAG recomputation; a
  dangling ADR-0028/`FINDINGS.md` receipt pointer was fixed
  (`…-share-0f3d1d3` → `…-share-d9f818a`); the edit witness **shares the
  descriptor by content id**, it does not copy it. Confirmed without correction:
  exactness after source removal; a pinned offline tokenizer; shareable units
  losing to CDC with `unique_bytes` a lower bound; and `descriptor_bytes_read ==
  0` on the warm short-circuit.

### Notes

- The seed DAG and the field manifest live in the **store**, referenced by
  content id; the descriptor remains the exact archival authority. The new
  `HIER_INDEX` record is skippable — a decoder without it still fully
  materializes via the DRA.
- The `63f43fb` field-court receipt was sealed from a **dirty** tree
  (`tools/field-court.sh` modified). The exactness triple it proves is still
  valid (materialize in a new process against a pre-removal golden copy), but the
  committed tool is not byte-identical to the one that ran. Recorded, not hidden.

## [0.1.0-alpha.12] — unreleased

Phase 10 — **DSFB encoder-only search governance** (10.1) and the **top-level
negative-results consolidation** (10.2). Docs + version only: no wire, candidate,
decoder, or feature-bit change.

### Added

- **Phase 10.1 — encoder-only search governance** (`src/encode/governor.rs`,
  ADR-0022; landed under this version). A non-default, **dependency-free** feature
  `dsfb-search = []` (**not** `["dep:dsfb"]`) adds typed, integer-only residual
  diagnostics, a tiny parametric candidate space over *existing* mechanisms
  (`SearchConfig { scale_bits, partition, replay, packed, depth }`), a pure
  `govern(&ResidualTrace) -> SearchDirective`, and `propose_configured`. **Zero
  decode authority:** no wire change, `header.rs` untouched, no feature bit;
  every candidate reaches the unmodified complete-cost court. The published
  `dsfb 0.1.2` crate is recorded as real, MSRV-OK, transitively present only via
  `entropyfs-store`, and *unavailable-for-purpose* (it is Drift-Slew Fusion
  Bootstrap state estimation, not a search governor).
- **Phase 10.2 — `FINDINGS.md` + ADR-0023 (top-level consolidated decision).**
  One authoritative document stating what was built, what was measured, against
  which baseline, what won, what lost, and why — every claim linked to a sealed
  receipt and naming its baseline. [`FINDINGS.md`](findings.md) supersedes the
  per-phase narratives. ADR-0023 records the verdict: **the current VOLE
  representation stack does not beat purpose-built baselines on any measured
  axis; the durable results are byte-exactness, an auditable representation, and
  the recorded negatives.** It consolidates ADR-0017 (whole-file loss, 0/27),
  ADR-0018/0019 (scoped partial-decode wins), ADR-0021 (cross-document sharing
  loss), and ADR-0022 (no byte benefit). The falsified-claims log (the Phase-6
  qpdf "win" and the Phase-7.0b Cairo "win", both fixture artifacts) and the
  three falsifiable directions that could change the conclusion are included.
- README gains a prominent **Findings** pointer and scoped status wording;
  `PROJECT_STATE.md` gains the consolidated ledger row.

### Measured — Phase 10.1 governor court (recorded negative)

- **The parametric search adds no bytes on the frozen cohort** (campaign
  `2026-10-05-phase10-governor-d2b09c9`; ADR-0022). Over small deterministic
  samples + a synthetic trio (disjoint tune/holdout/control sets; H2 judged only
  on holdout): **H1 HELD** (`DsfbGuided.final ≤ FixedHeuristic.final` everywhere),
  **H2 HELD** (guided == exhaustive on 8/8 holdout with ≤ ½ the candidates),
  **H3 HELD** (`fixed == exhaustive` on *every* workload; median byte benefit
  0 ‰), **H4 HELD** (negative controls `Stop(Raw)` match the RAW descriptor
  byte-for-byte). Representative rows (final bytes / candidates): `flate.pdf`
  36,161/10 (fixed), 36,161/438 (exhaustive), 36,161/150 (guided); `bigtext.pdf`
  38,274 at 7/414/126; `many.pdf` 5,301 at 7/414/126. **Zero decode authority,
  proven:** a governor-produced descriptor decodes byte-exactly in the **default**
  build (no `dsfb-search`). Small locally generated cohort; **no population
  claim**. The fixed complete-cost court is retained.

### Notes

- **Claim discipline is now normative (ADR-0023).** No whole-file "compression"
  claim without the four generic compressors on a committed corpus and a sealed
  receipt; cross-document sharing is store *amortization*, never "compression";
  exactness is the invariant, not a competitive win; no population claim from the
  locally generated corpora; withdrawn claims stay withdrawn wherever the
  affected phase is described.

## [0.1.0-alpha.11] — unreleased

Phase 9 — **cross-document content-addressed store** (9.1 store core, 9.2
optional EntropyFS backend + store CLI). Exactness is unchanged: a store-backed
descriptor must materialize the exact same bytes as its standalone form. **No
size or win is claimed here** — whether object-granularity sharing beats per-file
LZ and generic content-defined-chunk dedup is the Phase 9.3 cohort measurement.

### Added

- **Phase 9.1 — `ObjectStore` + `EmbeddedStore` + store-backed descriptor form**
  (ADR-0020). `Id = BLAKE3-256(object bytes)` is the relational store namespace,
  kept strictly distinct from the SHA-256 whole-source *archival* identity (an
  `Id` never appears in `INTEGRITY`). New optional-but-load-bearing `EXTERNAL_REF`
  record (`0x80`; `id:[u8;32]`, `len:u64LE`, exactly 40 B) puts each object-table
  entry in one of two forms; the entry's position remains its `object_id`, so the
  DRA is untouched. A store-backed descriptor declares the mandatory
  `FEATURE_EXTERNAL_OBJECTS` (`1 << 1`) bit; a build without the new default
  `store` feature fails closed with `UnsupportedFeature` (exit 6). New universe
  suffix `+external-objects-v1` (prefix `phase9`; `dra-8` and `FORMAT_MINOR`
  unchanged). `externalize` (inline → external) and `hydrate` (external → inline)
  convert both ways and materialize identical bytes. `gc` mark-and-sweeps the
  union of the roots' external ids; a non-empty dangling set is a live
  `MissingExternalObject`. `EmbeddedStore` is a raw local content-addressed
  directory (`objects/<aa>/<bb>/<64-hex-id>`) with atomic write-then-rename `put`,
  strict `get_range`, per-object `remove`, and a `STORE` backend marker.
  `CostBreakdown::external_refs` charges the 40 B payload (framing to
  `record_framing`). Tests: `tests/store.rs`.
- **Phase 9.2 — optional `EntropyFsStore` + store CLI + docs.** A thin
  `ObjectStore` adapter over the embeddable `entropyfs = "=0.7.17"`
  `engine::Engine` (`default-features = false`), behind the non-default,
  heavy `entropyfs-store` feature (implies `store`; pulls a non-optional `dsfb`
  and a ~40-crate tree). `put → put_blob`, `get → get_blob` (full whole-blob
  BLAKE3 gate), `get_range → read_blob_range` plus an explicit strict
  `offset + len <= stored_len` check (the engine clips at EOF), `contains →
  contains`; `BlobId` is BLAKE3-256, asserted identical to our `Id`, so an
  `EmbeddedStore`-externalized descriptor resolves unchanged. `list`/`remove`
  decline with `UnsupportedFeature` (no per-blob delete), so mark-and-sweep GC
  cannot reclaim through it; reclamation is EntropyFS's own reachability-GC
  policy. New CLI: `decode --store STORE_DIR`, `store put INPUT.voldoc STORE_DIR`
  (writes `STORE_DIR/<stem>.voldoc`), `store account STORE_DIR ROOT...`, and
  `store gc STORE_DIR ROOT...`. `src/store/account.rs` computes the three
  universes. Tests: `tests/entropyfs.rs`; formatting both ways proven
  byte-exact.
- **Three accounting universes (kept permanently distinct).** Standalone
  `S = Σ|serialize(d_i)|` (whole-file; the only universe comparable to a per-file
  compressor); unique-reachable `U = Σ|serialize(e_i)| + Σ len(o)` (shared bytes
  counted once); amortized `A = Σ(|serialize(e_i)| + Σ len(o)/refcount(o))` with
  the split **fractional by reference count**, integerized by largest remainder so
  `Σ A_i == U` exactly. `store account` prints all three and the per-root split.
- Docs: `SPEC.md` (`EXTERNAL_REF` record, `ObjectSource` model, mandatory feature
  bits, universe string, feature policy), `docs/adr/0020-content-addressed-store.md`.
- **Phase 10.1 — encoder-only search governance** (`src/encode/governor.rs`, ADR-0022).
  A non-default, **dependency-free** feature `dsfb-search = []` (**not**
  `["dep:dsfb"]`) adds typed, integer-only residual diagnostics
  (`SourceRegion`, `FormatStructure`, `ResidualClass`, `RunStats`/`Periodicity`/
  `Recurrence`, `ResidualDiagnostic`, `ResidualTrace`), a tiny parametric candidate
  space over existing mechanisms (`SearchConfig { scale_bits ∈ {8,10,12},
  partition ∈ {ByKind,ByRole}, replay ∈ {Off,Dedup,DedupRans}, packed, depth }`), a
  pure `govern(&ResidualTrace) -> SearchDirective` with a frozen dominant-residual
  table and a bounded `Budget`, and `propose_configured` (parameterizes `BYTE_RANS`
  and `PDF_CHANNELS` by `scale_bits`, toggles partition/replay/packed/depth). **Zero
  decode authority:** no wire change, `header.rs` untouched, no feature bit; every
  candidate reaches the unmodified complete-cost court. The example
  `governor_court`, `tools/governor-court.sh`, and `tests/governor.rs` are gated by
  the feature. The `dsfb 0.1.2` crate is recorded as real, MSRV-OK, transitively
  present only via `entropyfs-store`, and unavailable-for-purpose (it is Drift-Slew
  Fusion Bootstrap state estimation, not a search governor).

### Measured — Phase 9.3 store court (recorded negative)

- **Cross-document sharing does not beat per-file LZ or generic
  content-defined-chunk dedup** on the phase9 cohort (campaign
  `2026-10-05-phase9-store-fdb2845`; ADR-0021). 37 locally generated files,
  5,579,469 B source, 10 strata. Standalone `S = 3,762,694 B`; unique reachable
  `U = 3,369,900 B` (`==` amortized `A`); unique object bytes `786,501 B`.
  Per-file min LZ `1,304,307 B` (gzip 1,495,074 / zstd 1,334,444 / xz 1,307,612 /
  brotli 1,306,498). Strongest CDC (pinned borg 1.2.4, chunker `10,15,11,127`,
  `--compression none`) deterministic `771,383 B`; the same with
  `--compression zstd,19` is **non-deterministic** (observed 210,835–210,840 B,
  never a fixed citation). **`U` loses to all three.** All 37 standalone and
  store-backed roots decoded and `cmp` byte-exact; closure `dangling = 0`.
- **The negative is robust but its size is partly an artifact of candidate
  selection / externalization granularity, not of the mechanism's limits.**
  `externalize` replaces only `Descriptor.objects`, and the auto complete-cost
  winner codes most PDF bulk in entropy channels / `GRAPH` inlines, so it emits
  0–1 objects per file. Forcing `PDF_DEFLATE_REPLAY` — a candidate **in the
  current set** — emits one object per deflate stream: the global `U` falls to
  `2,360,054 B` (a per-stratum oracle gives ~`2,537,730 B`), still a loss vs LZ
  and raw CDC. The earlier claim that no current candidate could share finer than
  a whole file is **withdrawn**: the finest unit the current set can emit is the
  whole deflate stream; no current candidate emits more than one object per file,
  and finer sharing (channel payloads / sub-object chunks) remains a
  representation change.
- Under the **auto** candidate the only win is `repeat-bin` (four byte-identical
  opaque binaries): `U = 133,048 B` vs per-file LZ `524,308 B` (−74.6 %), CDC
  `140,640 B` raw (−5.4 %) / `133,863 B` compressed (−0.6 %, ~0.8 KB — real but
  marginal). `shared-payload` — the fixture expected to win — loses in the auto
  run (`U = S = 800,210 B` vs CDC `285,257 B`) because the auto winner codes the
  shared stream in entropy channels, not the object table; **forcing
  `PDF_DEFLATE_REPLAY` flips it to `264,139 B`** (2 unique objects), a win over
  *raw* CDC (`285,257 B`) that still loses to per-file LZ (`34,591 B`) and
  compressed CDC (`28,195 B`). `shared-bin`, `one-changed`, `incremental`,
  `reexport`, `repeat-pdf` and the pre-registered `shifted` control all lose to
  CDC. **Cause:** `externalize` replaces only `Descriptor.objects`; for
  channels/program winners that table is empty or a single whole-file object, so
  sharing is whole-object-granular and coarse and CDC captures the same (and
  near-duplicate) sharing the store misses. Independent adversarial review:
  `docs/evidence/phase9-skeptic-review.md`.
- **Tooling.** `tools/store-cohort.sh` (deterministic sharing cohort + committed
  `cohort.json` ledger), `tools/store-court.sh` (three universes vs per-file LZ
  and strongest CDC, per stratum), `tools/chunk-dedup.sh` (pinned borg CDC with a
  parameter sweep), `borgbackup=1.2.4-1` added to the `baseline` image. Full
  report `docs/evidence/phase9-store-report.md`; ADR-0021.

### Measured — Phase 10.1 governor court (recorded negative)

- **The parametric search adds no bytes on the frozen cohort** (campaign
  `2026-10-05-phase10-governor-d2b09c9`; ADR-0022). Over small deterministic
  samples + a synthetic trio (disjoint tune/holdout/control sets; H2 judged only on
  holdout): **H1 HELD** (`DsfbGuided.final ≤ FixedHeuristic.final` everywhere),
  **H2 HELD** (guided == exhaustive on 8/8 holdout with ≤ ½ the candidates),
  **H3 HELD** (`fixed == exhaustive` on *every* workload; median byte benefit
  0 ‰), **H4 HELD** (negative controls `Stop(Raw)` and match the RAW descriptor
  byte-for-byte). Representative rows (final bytes / candidates): `flate.pdf`
  36,161/10 (fixed), 36,161/438 (exhaustive), 36,161/150 (guided); `bigtext.pdf`
  38,274 at 7/414/126; `many.pdf` 5,301 at 7/414/126.
- **Zero decode authority, proven.** A governor-produced descriptor for `many.pdf`
  (winner `BYTE_RANS`) decodes byte-exactly in the **default** build (no
  `dsfb-search`); a `flate.pdf` descriptor (winner `PDF_DEFLATE_REPLAY_RANS`)
  decodes in a build with the underlying `deflate-replay` capability and still no
  `dsfb-search`. A grep gate asserts the decode path references neither `governor`
  nor `crate::encode`.
- **Honest verdict:** the fixed heuristic already attains the exhaustive minimum
  on every workload, so the governance mechanism is retained as an optional
  encoder feature but the **fixed complete-cost court is retained**; the negative
  is recorded, not hidden. Small locally generated cohort; **no population claim**.

### Notes

- **Claim discipline.** No size or compression claim is made for the store; a
  store root reference is never reported as a whole-document size. The honest
  comparison (per-file LZ and generic CDC dedup over a reproducible cohort) and
  the pre-registered negative are now **measured and recorded**: cross-document
  sharing is store *amortization*, never "compression", and it loses the store
  axis to generic CDC on this cohort (ADR-0021). `dsfb` remains a hard dependency
  of the *optional* `entropyfs-store` feature only and retains **zero** decode
  authority (ADR-0008). Phase 10.1 adds an encoder-only governor that likewise has
  **zero** decode authority and does **not** depend on `dsfb` (ADR-0022).
- **Corrections (independent adversarial review, `docs/evidence/phase9-skeptic-review.md`).**
  The Phase-9.3 claims were corrected after an independent review: (i) the
  negative is restated as robust but **partly an externalization-granularity /
  candidate-selection artifact** (`PDF_DEFLATE_REPLAY` is in the current set and,
  forced, flips `shared-payload` to a win over raw CDC); (ii) the "only win is
  byte-identical opaque repeats" wording is replaced by the accurate statement;
  (iii) the compressed-CDC citation is a **non-deterministic range**
  (210,835–210,840 B), with the deterministic raw figure (`771,383 B`) as the
  primary baseline. No measured campaign number in `results.json` was altered.

## [0.1.0-alpha.10] — unreleased

Phase 8 — **seek-based partial I/O** — plus the Phase-8.4 seekable-baseline
amendment that restates its result. Exactness is unchanged: the prime directive
is still `materialize(descriptor) == original_bytes`, and whole-file compression
remains a recorded negative against generic lossless tools (ADR-0017). Phase 8
adds one optional wire record (the seek `DIRECTORY`) and a `Read + Seek` reader,
and measures a new axis (random-access bytes read). The honest result is scoped:
a bytes-read win **only versus non-seekable sequential** codecs.

### Added

- Phase 8.1/8.2 — seek `DIRECTORY` record and `Read + Seek` reader (ADR-0019):
  - An optional `seek_directory_v1` record (`RecordTag::Directory = 0x71`) written
    as the first record at fixed offset 64 with `FLAG_OPTIONAL`, carrying
    LOCATORS, a CLASS_INDEX, and CHANNEL_LENGTHS. Its position is a constant, so
    no header field is consumed and `FORMAT_MINOR` does not bump; a new ignorable
    `FEATURE_SEEK_DIRECTORY` optional bit is set, and a decoder that ignores the
    record still materializes exactly. Bounded by `Limits::max_directory_bytes` /
    `max_directory_entries`.
  - `materialize_observation_seeked` (`src/materialize/seek.rs`) serves the same
    narrow observation as Phase 7.3 from a `Read + Seek` source. The directory is
    **advisory, never authority**: locators are cross-checked against record
    framing, the class index against a linear scan, and the observation index is
    re-derived over directory-derived lengths; a lying directory is rejected and
    a missing/oversized one declines (`UnsupportedFeature`). A partial read is an
    *observation* (`integrity_verified == false`); `materialize`/`decode`/`verify`
    remain the archival authority.
  - `view` peeks only the 64-byte header before choosing the seek path, so a
    seekable descriptor is never `fs::read` whole. New Phase-8 universe suffix
    `+seek-directory-v1`; non-seek serialization re-bases only by the universe
    length (+18 B on the stress corpus).
- Phase 8.3 — seek bytes-read court: `strace` added to the opt-in `baseline`
  image; `tools/seek-court.sh` / `tools/seek-table.jq` record the instrumented
  `bytes_read`, a descriptor-file-attributed `strace -P` cross-check, syscall
  count, wall/CPU/peak-RSS, and sequential gzip/zstd/xz compressed-prefix
  baselines.
- Phase 8.4 — seekable/blocked random-access baseline (the honest comparison):
  `bgzip` (htslib 1.16 via the `tabix` package) and `pixz` 1.0.7 added to the
  pinned `baseline` stage; `tools/seekable-baselines.sh`,
  `tools/bgzf-seek-probe.pl`, `tools/xz-seek-probe.pl`,
  `tools/xz-block-reframe.pl`, `tools/seekable-table.jq` build and measure
  `bgzip -l 9` (BGZF), `xz --block-size=64KiB|1MiB|4MiB`, and `pixz`.

### Measured

- Campaign `2026-10-05-phase8-seek-08de2a9` (seek bytes-read court; ADR-0019) —
  on a 33,789,340 B (32.22 MiB), 800-stream deterministic PDF (seekable
  descriptor 17,566,832 B), the seeked `view` reads a **constant
  439,679–461,367 B** for all 18 pre-registered queries (18/18 byte-exact; floor
  = header 64 + DIRECTORY 27,390 + GRAPH 265,462 + OBSERVATION_INDEX 146,711 +
  INTEGRITY 52). That is **4.7 %–~21× fewer bytes than gzip's compressed prefix**
  and ~12–13× fewer than zstd/xz in the late region; CPU ~0.00 s and peak RSS
  ~3.8 MB (from ~38 MB). The `strace` cross-check equals the instrumented count +
  exactly 64 B (the header peek). **Restated (Phase 8.4):** this is a win only
  against **non-seekable sequential** codecs. Against **seekable/blocked**
  formats, the same late query (`--byte-range=32505856:256`; VOLE 460,713 B)
  reads **more**: bgzip 23,808 B (~19×), `xz --block-size=64KiB` 15,344 B (~30×),
  `xz 1MiB` 179,892 B (~2.6×); only `xz 4MiB` (708,612 B) and pixz 2,810,832 B
  (16 MiB blocks) read more than VOLE. **Not a general random-access-I/O win.**
  It also loses at `a = 0` vs gzip (327,680 B) and xz (73,728 B) and at early
  queries (≤ ~1.7 MiB) vs xz's prefix. `zstd --seekable` does not exist in zstd
  1.5.4. One locally generated corpus; no population claim. Receipts under
  `evidence/campaigns/2026-10-05-phase8-seek-08de2a9/` (`report.md`,
  `query-table.md`, `seekable.jsonl`, `seekable-table.md`, `seekable-report.md`);
  reports `docs/evidence/phase8-seek-report.md`,
  `docs/evidence/phase8-skeptic-review.md`.
- **Whole-file remains a recorded negative (ADR-0017).** The best VOLE lane on
  this corpus is **3.01×** xz, and even the seekable BGZF archive (7,995,600 B)
  is smaller than the 17,566,832 B seekable descriptor (0.46×). The directory
  buys query capability, not size.

### Notes

- **Claim discipline.** The seeked `view` reads a *floor* dominated by
  GRAPH + OBSERVATION_INDEX + DIRECTORY: a 256-byte request still incurs
  ~440 KB (~1,700×), and the write-up records where it loses at offset 0, at
  early queries, and against compact-block seekable formats. Independent review:
  `docs/evidence/phase8-skeptic-review.md` (independent adversarial reviewer,
  Phase 8).
- **Validator caveat.** A *referenced* channel's `decoded_length` is
  cross-checked against its record (`InvalidContainer` on disagreement); an
  *unreferenced* `CHANNEL_LENGTHS` entry is not validated. Benign: `analyze_ops`
  never uses an unused length, so no wrong bytes are served.
- No DRA, candidate, or entropy semantics changed in Phase 8.

## [0.1.0-alpha.9] — unreleased

Phase 7 is **hardening plus a pivoted result**. It closes two Phase-6 unknowns
(behaviour on a producer corpus, and coverage-guided fuzzing), then retires the
whole-file *compression* claim honestly and measures a different axis —
random-access decode cost. Phase 7 adds no wire record beyond the optional,
advisory `OBSERVATION_INDEX` (Phase 7.3). Phase 8 (branch `phase8`, ADR-0019)
adds a second optional wire record, the seek `DIRECTORY`, and a seek reader; its
measured **bytes-read** result is recorded under Measured below. Exactness is
unchanged in both phases.

### Added

- Phase 7.0 — producer-stratified Flate correction-ratio harness:
  - `vole-document deflate-stats INPUT...` emits, per `FlateDecode` stream,
    `compressed_bytes`/`plaintext_bytes`/`correction_bytes`/`rans_plaintext_bytes`
    and the ratios `correction/compressed`, `(plaintext+corr)/compressed`,
    `(rans(plaintext)+corr)/compressed`, plus replayed/declined counts. It is
    **diagnostics only** and changes no wire format or candidate behavior.
  - The summary reports the rANS complete cost two ways so shared plaintext is
    not overcounted: `replayed_rans_full_bytes` (naive per-stream sum) and
    `replayed_rans_dedup_bytes` (one charge per **unique** plaintext plus one per
    **unique** correction blob, mirroring the shared-channel candidate).
  - `tools/pdf-corpus.sh` builds a locally-generated, producer-stratified corpus
    from distinct lineages (Ghostscript 10.00.0 at
    `/default`/`/prepress`/`/printer`/`/ebook`/`/screen`, qpdf 11.3.0
    compress/linearize/object-streams=preserve/nocompress, a hand-written
    stored-block-zlib base, plus the Phase-3 synthetic set), validating every
    produced PDF with `qpdf --check` and writing a provenance ledger
    (`provenance.json`). The regenerable `.pdf` bytes are gitignored; no
    third-party bytes. `--deterministic-id` makes qpdf outputs byte-reproducible;
    Ghostscript `pdfwrite` output is not (per-run `/ID`, recorded in the ledger).
  - Harness tests in `tests/deflate_stats.rs`; the real complete-cost court is
    `tools/pdf-court.sh` (`encode FILE OUT` and
    `encode --force raw|byte-rans|pdf-deflate-replay|pdf-deflate-replay-rans FILE
    OUT`), which records each lane's complete `.voldoc` size and `verify` +
    `decode`/`cmp`s the auto winner byte-exact. A forced kind the input does not
    propose is a typed `Usage` decline recorded as `null`.
- Phase 7.0b — generator-family Flate corpus (real *authoring* generators):
  - A separate, opt-in `producers` `Dockerfile` stage / compose service
    (`vole-document/producers:bookworm`, base
    `debian:bookworm-slim@sha256:3783cc01…` — the same digest as `tools`)
    carrying generators with distinct DEFLATE behaviour: **ReportLab 3.6.12**,
    **Cairo 1.20.1/libcairo 1.16.0**, **LibreOffice Writer 7.4.7.2**, **pdfTeX
    3.141592653-2.6-1.40.24** (plus qpdf 11.3.0 for `/ID` normalization). It is
    deliberately **not** part of the fast `tools` gate; the image grows to
    ~724 MB. All four families built and ran; none was skipped.
  - `tools/pdf-corpus-producers.sh` generates one PDF per available family from
    the same deterministic content document as `tools/pdf-corpus.sh`, fingerprints
    every `/FlateDecode` payload (`qpdf --json` + `qpdf --show-object
    --raw-stream-data`), applies `qpdf --deterministic-id --stream-data=preserve
    --object-streams=preserve` **only** when it leaves every payload
    byte-identical, builds each family twice to record byte-reproducibility,
    `qpdf --check`s every output, and writes a provenance ledger. LibreOffice
    output is not byte-reproducible (run-varying metadata); the caveat is
    recorded. The regenerable `.pdf` bytes are gitignored; no third-party bytes.
  - `tools/pdf-producers-analyze.py` reduces the `deflate-stats` and complete-cost
    court outputs to a per-producer analysis. No wire format, candidate, or
    decode path changed.
- Phase 7.0c — generic-compressor baseline ladder (ADR-0017):
  - A pinned, opt-in `baseline` `Dockerfile` stage / compose service
    (`vole-document/baseline:1.99.0`, derived from the `dev` stage on base
    `rust:1.99.0-slim-bookworm@sha256:452176c0…`, so rustc/cargo match the
    measurement binary) adding the generic compressors `gzip`, `zstd`, `xz` and
    `brotli` (plus `jq`). It is deliberately **not** part of any fast gate.
  - `tools/baselines.sh` records, for every corpus file, the source size and the
    smallest **lossless** complete-file size for `gzip -9`, `zstd -19 --long=27`,
    `xz -9e` and `brotli -q 11` (each decompressed and `cmp`'d against the source
    before it is scored), plus the complete serialized `.voldoc` size of every
    VOLE lane (auto, `raw`, `rle`, `byte-rans`, and every forced structural kind).
    `tools/baselines.jq`, `tools/baselines-merge.jq` and `tools/baselines-table.jq`
    reduce and render the JSON table.
- Phase 7.1 — coverage-guided fuzzing:
  - A standalone `cargo-fuzz` package in `fuzz/` (kept out of `cargo package` by
    `exclude = ["fuzz", "research", "evidence"]`) with ten libFuzzer targets:
    `voldoc_parse`, `voldoc_roundtrip`, `dra_program`, `rans_model`,
    `rans_channel`, `pdf_lexer`, `pdf_scan`, `pdf_xref`, `deflate_replay`,
    `materializer`. Each asserts the hostile-input invariants (never panic or
    report an internal invariant, bounded work, deterministic, reconstructed
    length == declared length, unknown versions/features/codecs fail closed).
  - A pinned **dated nightly** fuzz toolchain: a `fuzz` `Dockerfile` stage on
    `rustlang/rust:nightly-bookworm-slim-2026-10-04@sha256:58f725c9…`
    (`rustc 1.101.0-nightly (db8f076d2 2026-10-03)`, `cargo
    1.101.0-nightly`) plus `cargo-fuzz 0.13.2` and `libfuzzer-sys 0.4.13`, wired
    as the `fuzz` compose service (host network, a dedicated `fuzz/target`
    volume). `RUSTUP_TOOLCHAIN` overrides the root `rust-toolchain.toml`.
  - `tools/fuzz.sh` runs a bounded campaign (`FUZZ_SECONDS`/`FUZZ_RSS_MB`, default
    60 s / 2048 MiB per target), copies the committed `fuzz/seeds/` into each
    target's corpus, and records duration, coverage, execs, and crash/OOM counts.
  - Five small committed seed inputs under `fuzz/seeds/`; regeneration is
    documented in `fuzz/README.md`.
- Phase 7.1b — process-isolated DEFLATE replay (contains fuzz finding F2):
  - The **decode path** no longer calls `preflate` in-process. A hidden worker
    subcommand `__replay-worker` (not advertised in `USAGE`) reads a framed
    `(plaintext, corrections, declared_len)` request from stdin, calls
    `recreate_whole_deflate_stream`, and writes a framed reply to stdout. Framing
    is little-endian: request `[u32 plaintext_len][plaintext][u32
    corrections_len][corrections][u32 declared_len]`; reply `[u8 status]
    [u32 payload_len][payload]` (`0` = raw DEFLATE, `1` = a UTF-8 error message).
    Field lengths above a small internal bound are refused before allocation, and
    a preflate panic is caught by a non-aborting hook and returned as an error
    reply instead of an abort with lost output.
  - `replay_bounded(plaintext, corrections, declared_len, limits)` spawns
    `sh -c 'ulimit -v <KB> 2>/dev/null; ulimit -t <SEC> 2>/dev/null; exec "$0"
    __replay-worker' <worker>` with `stdin`/`stdout`/`stderr` piped, an
    `RLIMIT_AS` address-space cap, and a polled wall-clock timeout that `kill()`s
    on expiry. A non-zero exit, an abort (SIGABRT, the likely memory-cap case), a
    malformed reply, or a timeout each yields a typed `CodecReplay` error; a child
    `status=1` message is surfaced verbatim.
  - The decoder (`Program::eval`'s `DEFLATE_REPLAY` arm) calls `replay_bounded`;
    the in-process `replay_raw` is retained for the encoder's own `try_replay`
    verification and the fuzz targets. No wire format, candidate, or DRA op
    changed.
  - Knobs and defaults: `VOLE_REPLAY_WORKER` (worker executable; the CLI sets
    itself via a safe library setter when unset, since `std::env::set_var` is
    `unsafe` under Rust 2024), `VOLE_REPLAY_MEM_MB` (address-space cap; default
    `clamp((plaintext+corrections+declared) * 8 + 64 MiB, 256 MiB,
    limits.max_replay_bytes.min(2 GiB))`), `VOLE_REPLAY_TIMEOUT_MS` (default
    30000). With no worker configured the library falls back to the in-process
    path; a configured-but-broken worker is a typed error and **never** silently
    falls back.
  - Courts: `tests/replay_isolation.rs` (positive byte-exact decode through the
    worker, the F2 fixture failing closed under a 32 MiB cap, no silent fallback
    on a broken worker, and the documented in-process fallback) plus the
    worker-protocol framing round-trip unit test in `src/codec/deflate.rs`.
- Phase 7.3 — partial materialization and observation views (ADR-0018):
  - `Program::analyze_ops(object_lens, channel_lens, limits) -> Result<Vec<u64>>`
    returns the exact output length each instruction produces, sharing one walk
    with `analyze` so every existing rejection is unchanged.
  - A new optional, advisory `OBSERVATION_INDEX` record (tag `0x70`, written with
    `FLAG_OPTIONAL`, placed after `GRAPH` and before `INTEGRITY`) carries an op
    table, PDF selectors, and output-block digests gated by a `section_flags:
    u8`. It is **checked, never authority**: `Descriptor::parse` re-derives each
    `out_len` via `analyze_ops`, checks dependency ids and selector/digest ranges
    against the analyzed total, and rejects any contradiction with
    `CoverageViolation`. A decoder that ignores it still materializes exactly.
    `Descriptor` gains `observation_index: Option<ObservationIndex>` and an
    optional header feature bit `FEATURE_OBSERVATION_INDEX`; `cost.index` charges
    the record payload + framing, and `cost.total()` remains exactly the
    serialized length.
  - `materialize_observation` / `view_to_bytes`
    (`src/materialize/observation.rs`) serve one output range and report measured
    `ObservationStats` (ops/channels/objects touched, entropy bytes decoded,
    descriptor bytes traversed, work amplification). A descriptor without an
    index is **declined**, never silently fully materialized; when the program is
    a sequence of linear independent ops, only the ops intersecting the range are
    evaluated and only the referenced entropy channels are decoded.
  - `vole-document view INPUT.voldoc [OUTPUT] --byte-range A:L | --pdf-object N:G
    | --pdf-stream N:G | --pdf-revision I [--stats]`; the `--stats` JSON reports
    the resolved `range_start`/`range_len` so a harness can price sequential
    baselines at the same output offset.
  - `PDF_DEFLATE_REPLAY_RANS_INDEXED`
    (`encode --force pdf-deflate-replay-rans-indexed`) attaches the advisory
    observation index built from the same physical scan.
  - `vole-document pdf-make-large DIR [OBJECTS]`: an encode-time, binary-only
    deterministic generator of a valid classic-xref PDF with `OBJECTS` (default
    800) distinct real-zlib `FlateDecode` streams and ≥32 MiB of source; correct
    `/Length` and offsets by construction; no library or runtime dependency; the
    regenerable `.pdf` bytes are gitignored.
- Phase-7 universe string
  `vole-document;universe;phase7;exact-bytes;dra-8;opaque+entropy+pdf+channels+offsets+packed+packed-channels+deflate-replay-preflate-0.7.6-experimental+observation-index-v1`
  (DRA stays v8; no existing candidate bytes change beyond the universe bump and
  the optional index record).
- Phase 8 — seek-based partial I/O (branch `phase8`, ADR-0019):
  - An optional seek `DIRECTORY` record (`RecordTag::Directory = 0x71`,
    `seek_directory_v1`) written as the first record at fixed offset 64 with
    `FLAG_OPTIONAL`, carrying per-record LOCATORS (tag, offset, payload_len), a
    CLASS_INDEX for O(1) class lookup, and CHANNEL_LENGTHS. Its position is a
    constant, so no header field is consumed and `FORMAT_MINOR` does not bump; a
    new ignorable `FEATURE_SEEK_DIRECTORY` optional bit is set and a decoder that
    ignores the record still materializes exactly. Descriptor serialization
    becomes a two-pass build when the directory is enabled, and `cost.directory`
    charges its payload + framing so `cost.total()` stays exactly the serialized
    length. Bounded by `Limits::max_directory_bytes` / `max_directory_entries`; an
    oversized directory is declined before allocation.
  - `materialize_observation_seeked` (`src/materialize/seek.rs`) serves the same
    narrow observation as Phase 7.3 but from a `Read + Seek` source, reading only
    the header, DIRECTORY, GRAPH, OBSERVATION_INDEX, INTEGRITY, and the referenced
    OBJECT/ENTROPY_CHANNEL/MODEL records. The directory is **advisory, never
    authority**: locators are cross-checked against record framing, the class
    index against a linear scan, and the observation index is re-derived over
    directory-derived lengths; a missing/oversized/lying directory is declined
    (`UnsupportedFeature`) or rejected (`InvalidContainer`), never silently fully
    read. The report carries a real `bytes_read` (an internal `CountingReader`)
    and `integrity_verified == false` — a partial read is an *observation*, not
    an archival verification.
  - `view` peeks only the 64-byte header before choosing the seek path, so the
    CLI no longer `fs::read`s the whole descriptor when it is seekable.
  - New Phase-8 universe suffix `+seek-directory-v1`; the serialized record
    sequence for `seek_directory == false` is otherwise byte-identical to
    Phase 7 (non-seek lanes re-base only by the universe length, +18 B here).
  - Measurement tooling: `strace` added to the opt-in `baseline` image, and
    `tools/seek-court.sh` / `tools/seek-table.jq` record the instrumented
    `bytes_read`, a descriptor-file-attributed `strace` cross-check, syscall
    count, wall/CPU/peak-RSS, the Phase-7 stats, and sequential gzip/zstd/xz
    compressed-prefix baselines.

### Fixed

- PDF lexer stream boundary (`src/adapter/pdf/lexer.rs::find_endstream`): the
  `endstream` keyword is now located by **right-termination** (the byte
  immediately after `endstream` must be PDF whitespace, a PDF delimiter, or EOF)
  instead of requiring a preceding CR/LF. Real producers — confirmed for
  **Ghostscript 10.00.0** — write the stream payload directly before `endstream`
  with **no intervening EOL**, which previously made the opaque payload span
  over-read to a *later* `endstream`, swallowing whole stream objects and
  mis-slicing the `/Length` region (the captured bytes began `0x0a`, so
  `try_replay` declined them as `not_zlib`). The physical scanner's
  `/Length`-based resolution is unchanged; no wire format or candidate changed.
  This moves the Phase-7.0 exact-replay acceptance from **11/17** to **24/24**
  (`1.000`) and raises the stream census (the old over-read had hidden whole
  stream objects). New lexer tests cover the no-EOL case, the normal
  `\nendstream` case, an `endstream` at EOF, and an `endstream`-like run lacking
  a right terminator; a new integration test builds a PDF whose payload abuts
  `endstream` and checks the exact scan and byte-exact `try_replay`.
- **ADR-0016 correction (Phase 7.0).** The `DEFLATE_REPLAY` output bound is a
  VOLE **replay-profile admission limit** —
  `min(max_output_bytes, max_replay_bytes, 2*P + 1024)` for a `P`-byte plaintext
  — a deliberate **policy** bound, **not an RFC 1951 maximum**. RFC 1951 permits
  arbitrarily many **empty non-final stored blocks**, so a legal bitstream that
  inflates to zero bytes can be arbitrarily large and no finite
  `f(decompressed_size)` bound exists. `2*P + 1024` is an algorithmic expansion
  figure, not a theorem. The alpha.8 prose ("statically rejected above
  `2*P + 1024`") is superseded; the limit may decline a legitimate exact replay
  whose real output exceeds the profile, and that decline is an honest admission
  cost. No behavior changed — the re-framing makes the existing bound's nature
  explicit.
- Fuzz finding **F1** (upstream `preflate-rs` 0.7.6 `1 << params.window_bits`
  shift-overflow on hostile corrections): mitigated by installing a
  non-aborting panic hook in the fuzz targets so `replay_raw`'s documented
  `catch_unwind` boundary is exercised, with a minimized 35-byte regression
  fixture (`tests/fixtures/deflate_replay_shift_overflow.bin`) and a test
  asserting a typed `CodecReplay` error.

### Measured

- Campaign `2026-10-05-phase7-corpus-b-c4eb77e` (amendment; verdict RECORDED;
  diagnostic, no new candidate) — re-measured after the lexer stream-boundary
  fix: **24 `FlateDecode` streams, 24 replayed / 0 declined (acceptance 1.000)**
  across Ghostscript 10.00.0, qpdf 11.3.0, a hand-written stored-block-zlib base,
  and the Phase-3 synthetic set; the census rose 17 → 24 because the old
  over-read had swallowed whole stream objects. Corpus-wide
  `correction/compressed` **p10 0.000320 / p50 0.014716 / p90 0.097360** (the
  `0.004518` figure is the `pdf-make-samples` subset median only). The Phase-6
  win region appears **only in our own hand-authored fixtures**
  (`hand-base2.pdf` deduped rANS 55,531 vs naive 111,062; `_synthetic/flate.pdf`
  34,051 vs 89,437); `qpdf-preserve-objectstreams.pdf` shows the same geometry
  only because qpdf copied and renumbered the fixture's two byte-identical raw
  streams (`ec028dc1…`), so **99.93% of that win is inherited**, not produced by
  a transformer. Supersedes the original `2026-10-05-phase7-corpus-f1f8d26`
  (11/17), which is retained, not rewritten. Report
  `docs/evidence/phase7-corpus-report.md`; review
  `docs/evidence/phase7-skeptic-review.md`.
- Campaign `2026-10-05-phase7-court-99dc72e` (verdict RECORDED; measurement, no
  new candidate) — the decisive **complete-cost court** over the 23-file locally
  generated producer corpus. `PDF_DEFLATE_REPLAY_RANS` vs `BYTE_RANS`:
  **win 3 / lose 8 / decline 12 — all 3 wins self-authored.** The wins are
  exactly the Phase-6 shared-plaintext geometry: `hand-base2.pdf`
  (112,011 → 56,885, −55,126 B), `_synthetic/flate.pdf` (49,291 → 36,102,
  −13,189 B), and `qpdf-preserve-objectstreams.pdf` (112,147 → 56,980,
  −55,167 B) — the last only because qpdf `--object-streams=preserve` copied the
  fixture's two byte-identical raw streams, **99.93% inherited**. Every
  **genuinely transformed** producer output loses or declines (all 5 Ghostscript
  variants and both qpdf compression variants lose; the 12 files with no
  replayable Flate lane decline). All 23 auto winners `verify` + `cmp`
  byte-exact. The corpus is locally generated and is **not** a population
  sample; qpdf/Ghostscript are transformers. Receipt under
  `evidence/campaigns/2026-10-05-phase7-court-99dc72e/`.
- Campaign `2026-10-05-phase7-producers-e071250` (verdict RECORDED; measurement,
  no new candidate) — the Phase-7.0b **generator-family** corpus and
  complete-cost court over four real *authoring generators* (ReportLab 3.6.12,
  Cairo 1.20.1/libcairo 1.16.0, LibreOffice Writer 7.4.7.2, pdfTeX
  3.141592653-2.6-1.40.24). Exact-replay acceptance is **87/87 = 1.000**
  (corpus-wide `correction/compressed` p10/p50/p90 =
  0.034759/0.047945/0.068028). Under complete cost vs `BYTE_RANS`: win 1 / lose 3
  / decline 0 (Cairo −24,137 B, 58,711 → 34,574; ReportLab −3,312 B; pdfTeX
  −10,663 B; LibreOffice −302,474 B). **Corrected in Phase 7.0c:** the Cairo
  delta is a **repeated-identical-bytes harness artifact** — our generator drew
  one identical page six times, so Cairo emitted six streams byte-identical in
  *compressed* bytes as well as plaintext; generic LZ captures ~2× more
  (`gzip -9` 17,382 B, `zlib -9` 17,376 B, `xz -9e` 16,852 B) and `BYTE_RANS` is
  a weak order-0 baseline with no LZ. The withdrawn framing ("a genuine authoring
  application produces the win region", "first authoring-generator witness") is
  corrected: *a repeated-identical-bytes region already captured better by
  generic LZ, not the shared-plaintext-vs-distinct-compression mechanism.* All 4
  auto winners `verify` + `cmp` byte-exact; no population claim. Prose amendment
  in the receipt; review `docs/evidence/phase7b-skeptic-review.md`.
- Campaign `2026-10-05-phase7-baselines-7b9f662` (verdict RECORDED; measurement,
  no new candidate; ADR-0017) — the **generic-compressor baseline ladder** over
  27 corpus files (phase7 23 + producers 4), comparing complete files against
  `gzip -9`, `zstd -19 --long=27`, `xz -9e`, `brotli -q 11` (each round-trip
  verified lossless) and the best VOLE lane (minimum over auto/raw/rle/byte-rans
  and every forced structural kind). **The headline honest negative: the best
  VOLE lane beats gzip/zstd/xz/brotli on 0 files**; the best generic is smaller
  on every file, **+460,320 B** in total (phase7 +410,355; producers +49,965). On
  `cairo-vector.pdf` the best VOLE 34,574 B is **2.07×** brotli's 16,670 B; on
  `_synthetic/flate.pdf` 36,102 B is **1.91×** xz's 18,884 B. Prior "wins" were
  relative to the weak order-0 `BYTE_RANS` lane and do not survive the generic
  ladder. All 27 auto winners `verify` + `cmp` byte-exact. Receipt under
  `evidence/campaigns/2026-10-05-phase7-baselines-7b9f662/` (full table
  `baseline-table.md`); report section in `docs/evidence/phase7-corpus-report.md`.
- Campaign `2026-10-05-phase7-fuzz-ca6a92b` (verdict PASS_WITH_FINDINGS) — a
  bounded coverage-guided libFuzzer campaign (60 s/target, `-rss_limit_mb=2048`)
  over the ten targets in the pinned nightly `fuzz` service. **Nine of ten
  targets observed zero crashes** (e.g. `voldoc_parse` 23.3M execs, cov 89;
  `rans_model` 58.9M execs; `pdf_xref` cov 736 / ft 4229). `deflate_replay`
  reported two **upstream `preflate-rs` 0.7.6** findings: (F1) the
  shift-overflow panic, mitigated as described under Fixed; and (F2) an
  **unbounded reconstruction allocation** (a 33-byte hostile
  `(plaintext, corrections)` pair, peak RSS 2532 MiB at the 2048 MiB policy),
  reported as an upstream limitation — `REPLAY_OUTPUT_RATIO` bounds the declared
  output, not the third-party decoder's internal allocation, and preflate 0.7.6
  has no bounded streaming reconstruction sink (ADR-0016). **F2 is now contained
  on the decode path** by Phase 7.1b process isolation; the same artifact run
  under the pinned `fuzz` service reports `libFuzzer: out-of-memory` with
  `1744830464` bytes in one allocation and `peak_rss_mb: 2524`, while a CLI
  `decode` under a 32 MiB worker cap fails closed as a typed error without
  exhausting the host. Fuzzing is evidence, not a proof of absence. No wire
  format or candidate changed. Receipt under
  `evidence/campaigns/2026-10-05-phase7-fuzz-ca6a92b/`.
- Campaign `2026-10-05-phase7-partial-a5764c9` (verdict SCOPED POSITIVE;
  measurement, no wire/candidate change beyond the index record) — the
  **partial-materialization query court** on a 33,789,340 B (32.22 MiB),
  800-stream deterministic PDF (`pdf-make-large`; 800 replayed / 0 declined;
  `qpdf --check` rc 0). All 18 pre-registered queries (6 byte-ranges at
  0/1/8/16/24/31 MiB + 8 `--pdf-stream` + 4 `--pdf-object`) are byte-exact. For
  mid/late queries the indexed lane touches **~0.41–0.43 MB**
  (`descriptor_bytes_traversed` alone; its `entropy_bytes_decoded` breakdown is a
  subset already counted there and must not be added) versus gzip inflating
  `a + len`: in the late region that is **1.4–2.3 %** of gzip's bytes, and VOLE
  is **~2–5× faster than gzip** and **~4–13× faster than xz** on decode CPU. It
  **loses** in the early region (≤ ~8–16 MiB), **never beats zstd's raw
  decompressor** on wall time, uses ~38 MB peak RSS versus gzip's ~1.2 MB, and
  whole-file size is still **2.98×** xz (best VOLE 17,392,713 B / indexed
  17,539,424 B vs xz 5,841,896 B). The decisive v1 caveat: `view` reads and
  parses the **whole** descriptor, so on-disk I/O is **not** reduced and
  `descriptor_bytes_traversed` is a CPU-side approximation, not a bytes-read
  figure. Receipt under
  `evidence/campaigns/2026-10-05-phase7-partial-a5764c9/`; report
  `docs/evidence/phase7-partial-report.md`; review
  `docs/evidence/phase7c-skeptic-review.md`; ADR-0018.
- Campaign `2026-10-05-phase8-seek-08de2a9` (verdict SCOPED POSITIVE on bytes
  read **versus non-seekable sequential codecs**; ADR-0019) — the **seek-based
  partial-I/O court**, closing ADR-0018's explicit no-I/O-win caveat. On the same
  33,789,340 B (32.22 MiB), 800-stream deterministic PDF as Phase 7.3, the
  seekable descriptor is **17,566,832 B** (the new DIRECTORY adds 27,390 B; index
  + directory = 174,101 B over the non-seek base). All 18 pre-registered queries
  are byte-exact. The seeked `view` reads a **constant 439,679–461,367 B**
  regardless of offset — exactly header 64 + DIRECTORY 27,390 + GRAPH 265,462 +
  OBSERVATION_INDEX 146,711 + INTEGRITY 52 at `a = 0`, plus one referenced channel
  + model where a channel is needed — which is ≤ 2.6 % of the descriptor for
  every query (H1 18/18). In the late region (≥ 50 % in, H2 8/8) that is
  **4.7 %–~21× fewer bytes than gzip's compressed prefix** (9,764,864 B vs
  460,713 B at 31 MiB) and ~12–13× fewer than zstd/xz. A `strace -P`
  descriptor-file cross-check equals the instrumented count + exactly 64 B (the
  header peek), so there is no hidden whole-file read. CPU drops to ~0.00 s and
  peak RSS from Phase 7's ~38 MB to **~3.8 MB**. **Amendment (Phase 8.4): this is
  not a general random-access-I/O win** — against seekable/blocked formats the
  same late query reads **more**: bgzip (BGZF) 23,808 B (~19×), xz
  --block-size=64KiB 15,344 B (~30×), xz 1MiB 179,892 B (~2.6×) all read less
  than VOLE's 460,713 B (only xz 4MiB 708,612 B and pixz 2,810,832 B read more),
  and BGZF's whole file (7,995,600 B) is smaller than the seekable descriptor.
  It also **loses on bytes at `a = 0`** versus gzip (327,680 B) and xz (73,728 B)
  and at early queries (≤ ~1.7 MiB) versus xz's tiny compressed prefix; the
  floor is constant in the offset and would dominate a descriptor below ~9 MB.
  Whole-file size is still **3.01×** xz. One locally generated corpus; **no
  population claim**. Receipt under `evidence/campaigns/2026-10-05-phase8-seek-08de2a9/`
  (`report.md`, `query-table.md`, `seekable.jsonl`, `seekable-table.md`,
  `seekable-report.md`); reports `docs/evidence/phase8-seek-report.md`,
  `docs/evidence/phase8-skeptic-review.md`; drivers `tools/seek-court.sh`,
  `tools/seekable-baselines.sh`.

### Notes

- **Honest framing.** VOLE's prime directive is exactness
  (`materialize(descriptor) == original_bytes`), and on whole-file size VOLE's
  best lane loses to every generic lossless compressor tested (ADR-0017). The
  pivot is deliberate: Phase 7 measures **random-access decode cost** and Phase 8
  the **random-access I/O cost** (bytes read), each against sequential
  decompression to the same offset. Both axes are separate receipts and separate
  verdicts and are never conflated. The Phase-8 bytes-read win is scoped to one
  large locally generated PDF and **only to non-seekable sequential codecs**: a
  purpose-built seekable/blocked format (BGZF ~24 KB; blocked xz 15–180 KB) reads
  **2.6–30× less** for the same late query, so it is not a general
  random-access-I/O win; it also loses at the start of the file and versus xz's
  early prefix.
- The wire format remains **PROVISIONAL** and is not frozen v1. `OBSERVATION_INDEX`
  (`observation_index_v1`) is optional and **advisory**: it is re-derived and
  re-checked against the authoritative program at parse and can only cause a
  `CoverageViolation`, never change what `materialize` produces. It costs bytes
  (146,711 B on the stress corpus) and does not make the descriptor smaller.
- The Phase-6 win over `BYTE_RANS` is real and byte-exact, but its enabling
  condition (a plaintext shared across streams **and** a large/weakly-coded
  appearance) is **not produced by the tested transformers**: on the Phase-7.0
  corpus every genuine transformer output loses or declines, and the Phase-7.0b
  Cairo "win" is a repeated-identical-bytes harness artifact already beaten by
  generic LZ. The `ADOPTED` status of `PDF_DEFLATE_REPLAY_RANS` therefore means
  "implemented and can win when shared plaintext is present", not "shown to win
  on real producer output". Closing that gap is Phase 7.2 (nested content
  proceduralization).
- **F2 residual.** Process isolation bounds the *decoder's* exposure: a
  malformed `.voldoc` cannot amplify memory in the decoder process because the
  third-party allocation runs in a child with a hard address-space cap and a
  timeout. It is **not** a proof that `preflate` is bounded. The residual is a
  library consumer that decodes untrusted `.voldoc` without a worker
  (`VOLE_REPLAY_WORKER` unset and no installed default): that path is in-process
  and still exposed. The CLI installs itself as the default worker.
- Fuzzing is **evidence, not a proof of absence**: nine zero-crash targets mean
  no crash was observed in that 60 s budget on that build, not that none exists.
  Both `deflate_replay` findings are upstream `preflate-rs` defects surfaced
  through our wrapper.
- No candidate is adopted, removed, or changed by the baseline ladder or the
  query-cost court; the only new encode-time code is the `pdf-make-large`
  generator and the optional index record.

## [0.1.0-alpha.8] — unreleased

### Added

- Phase 6 — exact DEFLATE replay (`preflate-rs`):
  - `DEFLATE_REPLAY` DRA op (opcode `0x0A`), bumping the DRA graph to version
    `8`. It begins with a `replay_codec` tag (`REPLAY_DEFLATE_PREFLATE_0_7_6`),
    which names the correction representation as an **experimental,
    version-coupled** preflate-0.7.6 layout (not frozen v1); an unknown codec fails
    closed as `UnsupportedFeature` (never `InvalidGraph`). It emits exactly
    `recreate_whole_deflate_stream(plaintext, corrections)` — the **raw** DEFLATE
    bytes (RFC 1951, no zlib wrapper) — with a `declared_output_len` statically
    rejected above `2*P + 1024` for a `P`-byte plaintext *before* the engine runs
    (ADR-0016) and validated at evaluation; plaintext and corrections are bounded
    by `max_record_len`. `source_kind` is `0` (plaintext
    from the object table) or `1` (plaintext from an entropy channel).
    Reconstruction is isolated with `catch_unwind`, so hostile corrections yield a
    typed `CodecReplay`/`InvalidGraph`, never a panic or a silent success. A
    mandatory `FEATURE_DEFLATE_REPLAY` bit is declared whenever the op is present;
    a build without the feature fails closed with `UnsupportedFeature`.
  - A lexer change: `stream` followed by EOL now makes the payload an **opaque
    span**, so a PDF's stream-data bytes are a byte-authoritative span and a lone
    `/FlateDecode` is classified from the object dictionary. `preflate` never
    decides stream boundaries.
  - `PDF_DEFLATE_REPLAY` candidate: physical span order; each eligible stream span
    becomes `INLINE(zlib header) · DEFLATE_REPLAY · INLINE(Adler-32)`, with the
    plaintexts and correction blobs as content-deduplicated objects.
  - `PDF_DEFLATE_REPLAY_RANS` candidate: each **unique** plaintext is one order-0
    byte-rANS `ENTROPY_CHANNEL`, shared by every stream that produces it, so
    shared plaintext is stored once and decoded once; corrections stay raw
    deduplicated objects.
  - `encode --force` accepts `pdf-deflate-replay` and `pdf-deflate-replay-rans`.
  - Dependency `preflate-rs = "=0.7.6"` under the **opt-in** `deflate-replay`
    feature (`default = ["rans"]`; enable with `--features deflate-replay`),
    which transitively pulls LGPL-3.0-or-later `cabac` (ADR-0014). The default
    build is permissive-only. `flate2` (dev-only) uses the `zlib-rs` backend.
- Phase-6 universe string
  `vole-document;universe;phase6;exact-bytes;dra-8;opaque+entropy+pdf+channels+offsets+packed+packed-channels+deflate-replay-preflate-0.7.6-experimental`.
- Courts: the Phase-6 replay gates in `tests/pdf_deflate.rs` and the
  forced-candidate ablation court `tools/phase6-court.sh` over the 12-file
  corpus.

### Measured

- Campaign `2026-10-05-phase6-0d0bb79` (DRA v8, verdict PASS; the earlier
  `2026-10-05-phase6-ec92c1a` receipt is retained): a 12-file corpus; every
  auto winner round-trips byte-exactly (`cmp` + `verify`, `all_exact=true`); the
  qpdf differential oracle re-checks clean.
- **On `flate.pdf` (57,513 B) the shared-plaintext rANS replay lane is the auto
  winner at 36,102 B, a 13,189 B win over `BYTE_RANS` (49,291 B)** — the first
  measured positive for a PDF structural candidate. The raw-plaintext variant
  `PDF_DEFLATE_REPLAY` = 56,736 B loses to `BYTE_RANS` (7,445 B larger), and RAW =
  57,908 B.
- The sample exposes 6 FlateDecode streams but only **3 unique plaintexts**; the
  rANS lane stores 3 plaintext channels (only `p1`, across four streams, is
  shared) plus 6 correction objects.
- Cumulative ladder: A0 RAW = 139,950; A2 +`BYTE_RANS` = 98,560;
  A6 +`PDF_LAYOUT_RANS` = 98,560; A7 +`PDF_DEFLATE_REPLAY` = 98,560;
  A8 +`PDF_DEFLATE_REPLAY_RANS` = **85,371**. Leave-one-out delta for the rANS
  replay mechanism = **−13,189**; auto winners RAW = 8, `BYTE_RANS` = 3,
  `PDF_DEFLATE_REPLAY_RANS` = 1.
- Head-to-head `PDF_DEFLATE_REPLAY_RANS` vs `BYTE_RANS`: **win 1, lose 0,
  decline 11** (only `flate.pdf` has a lone FlateDecode stream; every decline is
  recorded verbatim, never scored).
- **Scoped result.** One composed sample at commit `0d0bb79`: the winning region
  is a plaintext that is *shared across streams* **and** *also* has a
  large/weakly-coded appearance (neither sharing alone nor weak coding alone
  wins); the losing region is unique, strongly-compressed plaintext, where the
  plaintext is no smaller than the bitstream it replaces. Receipt under
  `evidence/campaigns/2026-10-05-phase6-0d0bb79/`.

### Notes

- The wire format remains **PROVISIONAL** and is not frozen v1. `DEFLATE_REPLAY`
  (DRA v8; `replay_codec`-tagged, statically resource-bounded — ADR-0016) is exact
  and bounded; `PDF_DEFLATE_REPLAY_RANS` is `ADOPTED` as a
  winning candidate and `PDF_DEFLATE_REPLAY` is `RECORDED (rejected vs
  BYTE_RANS)`. This is the first PDF structural candidate to beat a whole-file
  order-0 rANS lane; the earlier converging negatives (ADR-0010–ADR-0013) apply
  to proceduralizing *plain* syntax, while exact replay attacks bytes that are
  *already* entropy-coded (ADR-0015).
- The `deflate-replay` feature is **opt-in**: the default build is
  `default = ["rans"]` (permissive-only), and `--features deflate-replay` (or
  `--all-features`) enables the lane; a build without it rejects the op with
  `UnsupportedFeature` (see ADR-0014 for the LGPL consequence).

## [0.1.0-alpha.7] — unreleased

### Added

- Phase 5.8 — layout + rANS residual:
  - `PACKED_CHANNELS` DRA op (opcode `0x09`), bumping the DRA graph to version
    `6`. It reconstructs output from a **data** entropy channel interpreted by a
    serialized item table carried in a **plan** entropy channel, with a declared
    output length validated at evaluation. Unknown tags, truncation, unmarked
    slots, bad widths, literal overruns, missing channels, a declared-length
    mismatch, and unconsumed data are rejected with typed errors before an inexact
    result is ever returned.
  - `PDF_LAYOUT_RANS` candidate: codes the layout plan's literal data object
    (channel 0) and `encode_items` of its item table (channel 1) each as their own
    order-0 byte-rANS channel with its own model, reconstructed by one
    `PACKED_CHANNELS` op. Byte-exact and verified serialize → parse → materialize →
    byte-compare before it is returned.
  - `encode --force` accepts `pdf-layout-rans`.
- Phase-5.8 universe string
  `vole-document;universe;phase5-8;exact-bytes;dra-6;opaque+entropy+pdf+channels+offsets+packed+packed-channels`.
- Courts: layout+rANS gates in `src/adapter/pdf/layout.rs`, the `PACKED_CHANNELS`
  evaluator gates in `src/dra/program.rs`, and the Phase-5.8 forced-candidate
  ablation court `tools/phase5-8-court.sh` over the 11-file corpus.

### Measured

- Campaign `2026-10-05-phase5-8-cf8048d` (verdict PASS): an 11-file corpus; every
  auto winner round-trips byte-exactly (`cmp` + `verify`).
- Cumulative ladder (serialized `.voldoc` bytes): A0 RAW = 81,591;
  A2 +BYTE_RANS = 48,818; A6 +PDF_LAYOUT_RANS = 48,818. Leave-one-out layout+rANS
  delta = 0; layout+rANS wins 0 of 8 head-to-head comparisons and is never the
  auto winner.
- Forced sizes: `classic.pdf` RAW **683** / `BYTE_RANS` **728** / `PDF_LAYOUT`
  **731** / `PDF_LAYOUT_RANS` **883**; `bigtext.pdf` RAW **65,903** /
  `BYTE_RANS` **38,174** / `PDF_LAYOUT_RANS` **38,341**; `many.pdf` RAW **10,235**
  / `BYTE_RANS` **5,201** / `PDF_LAYOUT` **10,089** / `PDF_LAYOUT_RANS` **5,914**
  (breakdown: data 7,877, plan 1,815, models 645, payload 4,775).
- **Recorded negative result:** head-to-head against `BYTE_RANS` the layout+rANS
  candidate wins 0, loses 8, and is declined by 3 files. Channel 0 codes nearly
  the whole file — the same job `BYTE_RANS` does with one channel — while the plan
  channel plus a second model are added metadata `BYTE_RANS` never pays.
  `PDF_LAYOUT_RANS` stays implemented and available but is **rejected by the
  complete-cost court**. Receipt under
  `evidence/campaigns/2026-10-05-phase5-8-cf8048d/`.

### Notes

- The wire format remains **PROVISIONAL** and is not frozen v1. `PACKED_CHANNELS`
  (DRA v6) is exact and bounded; `PDF_LAYOUT_RANS` is `RECORDED (rejected vs
  BYTE_RANS)`. This is the fourth converging negative (ADR-0010, ADR-0011,
  ADR-0012, ADR-0013): at the tested scale, PDF structural proceduralization does
  not beat a whole-file order-0 rANS lane.

## [0.1.0-alpha.6] — unreleased

### Added

- Phase 5.7 — packed segment framing (Phase-6 preparation):
  - `PACK_SEGMENTS` DRA op (opcode `0x08`), bumping the DRA graph to version `5`.
    One op carries a compact item table over a single data object —
    `Literal { len }` (a `u32` LEB128 varint), `Mark { slot }`, and
    `Emit { slot, width }` — so per-segment op framing is paid once instead of
    once per span. The data object must be consumed exactly; unknown tags,
    truncation, unmarked slots, bad widths, overruns, and unconsumed data are
    rejected before allocation.
  - Layout candidate rebuilt on the packed op (**layout-v2**) with **literal
    coalescing**, which merges adjacent literal runs (on `many.pdf`, 200 objects,
    9,881 B, the item table fell from **1,413 to 805** items).
  - A large classic-xref scale sample (`many.pdf`, hundreds of xref entries) to
    expose the scaling win.
- Phase-5.7 universe string
  `vole-document;universe;phase6-prep;exact-bytes;dra-5;opaque+entropy+pdf+channels+offsets`.
- Courts: packed-op/layout-v2 gates in `tests/pdf_layout.rs` and the extended
  forced-candidate ablation court `tools/phase5-court.sh` over the enlarged
  corpus.

### Measured

- Campaign `2026-10-05-phase5-4521778` (verdict PASS): an 11-file corpus; every
  auto winner round-trips byte-exactly (`cmp` + `verify`).
- Cumulative ladder (serialized `.voldoc` bytes): A0 RAW = 81,371;
  A2 +BYTE_RANS = 48,598; A5 +PDF_LAYOUT = 48,598. Leave-one-out layout delta = 0;
  layout wins 0 of 8 classic-xref samples and is never the auto winner.
- Forced sizes: `many.pdf` layout-v2 **10,069** vs RAW **10,215** (layout now
  **beats RAW by 146 B** at scale) vs `BYTE_RANS` 5,181; `classic.pdf` layout 711
  vs RAW 663; `bigtext.pdf` layout 65,929 vs RAW 65,883 / `BYTE_RANS` 38,154.
- **Measured partial positive:** packed framing plus coalescing makes structural
  layout prediction beat RAW at document scale, but the residual data object is
  still stored literally, so an order-0 `BYTE_RANS` lane dominates it. The
  remaining lever is to entropy-code the residual (structural prediction
  composed with rANS on the residual), not to pack literals further. Receipt
  under `evidence/campaigns/2026-10-05-phase5-4521778/`.

### Notes

- The wire format remains **PROVISIONAL** and is not frozen v1. The packed op and
  layout-v2 are exact and bounded; layout-v2 is `RECORDED (rejected vs
  BYTE_RANS)` with the residual-entropy-coded form `PROPOSED` (Phase 6+). See
  ADR-0012.

## [0.1.0-alpha.5] — unreleased

### Added

- Phase 5 — PDF classic-xref layout proceduralization:
  - Positional DRA ops `MARK_OFFSET` (opcode `0x06`) and `EMIT_OFFSET`
    (opcode `0x07`), bumping the DRA graph to version `4`. `MARK_OFFSET` records
    the current output position into one of 256 bounded slots (slot `255`
    reserved for the most recent xref section start); `EMIT_OFFSET` emits a
    marked position as a fixed-width, zero-padded decimal field.
  - `PDF_LAYOUT` candidate: regenerates classic cross-reference entry offsets and
    the `startxref` value from marked object/section positions instead of storing
    the digits, with literal per-site fallback. Byte-exact and verified
    end-to-end before it is returned.
  - `encode --force` accepts `pdf-layout`.
- Phase-5 universe string
  `vole-document;universe;phase-5;exact-bytes;dra-4;opaque+entropy+pdf+channels+offsets`.
- Courts: `tests/pdf_layout.rs` and the forced-candidate ablation court
  `tools/phase5-court.sh`.

### Measured

- Campaign `2026-10-05-phase5-7193001` (verdict PASS): on a deterministic 10-file
  corpus every file round-trips byte-exactly through its auto-winning lane
  (`cmp` + `verify`), and the qpdf oracle re-check passes (classic 4/4, twopage
  6/6, incremental 5/5).
- Cumulative ladder (serialized `.voldoc` bytes): A0 RAW = 71,116;
  A1 +RLE = 71,116; A2 +BYTE_RANS = 43,377; A3 +PDF_PHYSICAL = 43,377;
  A4 +PDF_CHANNELS = 43,377; A5 +PDF_LAYOUT = 43,377. Leave-one-out layout
  delta = 0. Auto winners: RAW = 8, BYTE_RANS = 2, PDF_PHYSICAL = 0,
  PDF_CHANNELS = 0, PDF_LAYOUT = 0.
- Prediction works and is exact: `classic.pdf` regenerates 3 of 4 xref entry
  offsets plus the `startxref`; `incremental.pdf` regenerates 5 of 7 entries plus
  two `startxref` values. Forced sizes still lose: `classic.pdf` 798 vs RAW 659;
  `bigtext.pdf` 66,066 vs RAW 65,879 / `BYTE_RANS` 38,150. Layout wins 0 of 7
  classic-xref files.
- **Recorded negative result:** correct structural prediction does not pay while
  each predicted field needs its own framed DRA op; the per-segment
  `MarkOffset`/`EmitOffset` framing costs more than the ~7 digits saved per
  offset. `PDF_LAYOUT` stays implemented and available but is **rejected by the
  complete-cost court**. Receipt under
  `evidence/campaigns/2026-10-05-phase5-7193001/`.

### Notes

- The wire format remains **PROVISIONAL** and is not frozen v1. The positional
  ops and the layout lane are exact and bounded but lose on complete cost;
  amortizing the framing (e.g. a packed segment table) or applying prediction to
  very large / many-offset documents is `PROPOSED` (Phases 6+). See ADR-0011.

## [0.1.0-alpha.4] — unreleased

### Added

- Phase 4 — PDF lexical/structural typed channels:
  - Deterministic, exactly-reversible **channel transposition** (`split`/`join`):
    the Phase-3.1 lexical span cover is transposed into one kind id per token,
    one 4-byte length per token, and one concatenated payload stream per lexical
    kind (`KIND_COUNT = 12`). `join(split(x)) == x` by construction for every
    byte string.
  - `INTERLEAVE_CHANNELS` DRA op (opcode `0x05`), bumping the DRA graph to
    version `3`. It replays the kind/length sequence against the per-kind
    payload channels with bounded, non-overlapping output spans.
  - `PDF_CHANNELS` candidate: one order-0 byte-rANS model per channel, built on
    the fixed channel layout (ch0 kinds, ch1 lengths, ch2..13 per-kind
    payloads).
  - Compact **model wire v2**: sparse `[symbol u8][freq u16]` entries for present
    symbols or the dense 256-entry table, whichever serializes smaller; legacy
    v1 dense models remain decodable.
  - Forced-candidate ablation: `encode --force KIND` runs the same complete-cost
    court over a one-element candidate set (`raw`, `rle`, `byte-rans`,
    `pdf-physical`, `pdf-channels`).
- Phase-4 universe string
  `vole-document;universe;phase-4;exact-bytes;dra-3;opaque+entropy+pdf+channels`.
- Courts: `tests/pdf_channels.rs` and the forced-candidate ablation court
  `tools/phase4-court.sh`.

### Measured

- Campaign `2026-10-05-phase4-3840bc4` (verdict PASS): on a deterministic 10-file
  corpus, every file round-trips byte-exactly through its auto-winning lane
  (`cmp` + `verify`), and the qpdf oracle re-check passes.
- Cumulative ladder (serialized `.voldoc` bytes): A0 RAW = 71,036;
  A1 +RLE = 71,036; A2 +BYTE_RANS = 43,297; A3 +PDF_PHYSICAL = 43,297;
  A4 +PDF_CHANNELS = 43,297. Leave-one-out channel delta = 0. Auto winners:
  RAW = 8, BYTE_RANS = 2, PDF_PHYSICAL = 0, PDF_CHANNELS = 0.
- On the text-heavy scale sample `bigtext.pdf` (65,549 B) forced sizes were
  RAW = 65,871, BYTE_RANS = 38,142, PDF_CHANNELS = 46,432: typed channels beat
  RAW but lose to BYTE_RANS by ~8,290 B. Compact sparse models cut per-channel
  model overhead from 7,224 B to 1,981 B, which is not enough to close the gap.
- **Recorded negative result:** coarse lexical transposition plus per-channel
  order-0 models does **not** beat a monolithic order-0 `BYTE_RANS` on this
  corpus. `PDF_CHANNELS` is preserved as an exact, available lane but is
  **rejected by the complete-cost court**, not adopted. Receipt under
  `evidence/campaigns/2026-10-05-phase4-3840bc4/`.

### Notes

- The wire format remains **PROVISIONAL** and is not frozen v1. The typed-channel
  lane is exact and bounded but loses on complete cost; contextual/ordering
  mechanisms that could condition a channel on its neighbours are `PROPOSED`
  (Phases 5+). See ADR-0010.

## [0.1.0-alpha.3] — unreleased

### Added

- Phase 3 — byte-authoritative PDF physical authority:
  - Owned PDF lexical scanner (whitespace, comments, literal/hex strings with
    escapes and nesting, names, delimiters, regular tokens) producing a
    contiguous, non-overlapping span cover of `[0, len)`.
  - Conservative physical structure scanner: `%PDF-` header, `obj`/`endobj`,
    `stream`/`endstream`, `xref`, `trailer`, `startxref`, `%%EOF`, and comments;
    stream payloads are treated as opaque bytes when `/Length` is unusable.
  - Direct/indirect `/Length` resolution with CRLF/LF handling, and an
    append-only revision map (`/Prev` chain; `/Size` never decreases).
  - xref-stream and object-stream structural role detection.
  - Validated PDF detection (a `%PDF-` header **and** an indirect object **and**
    `%%EOF`); file extensions are never authority.
  - `PdfPhysical` view plus a `PDF_PHYSICAL` candidate that persists the physical
    partition as one literal `INLINE` op per span, with conservative opaque
    fallback. Span kinds are deterministic analysis metadata recomputable by
    `scan`.
- `source_format = 1` (PDF) and the Phase-3 universe string
  `vole-document;universe;phase-3;exact-bytes;dra-2;opaque+entropy+pdf`.
- Courts: `tests/pdf.rs` (total coverage, forced physical exactness, validated
  detection, revision mapping, hostile random bytes) and the qpdf differential
  oracle `tools/pdf-oracle.sh`.

### Measured

- Campaign `2026-10-05-phase3-486aa17` (verdict PASS): deterministic 9-item
  corpus (7 valid PDFs plus `malformed.pdf` and `notpdf.bin` negative controls).
  Coverage `all_covered = true` — 171 spans, 19 objects, 8 revisions; exactness
  `all_exact = true` (`materialize(descriptor) == original_bytes` for every
  item). qpdf 11.3 oracle: 100% object-number agreement (classic 4/4, two-page
  6/6, incremental 5/5), `qpdf --check` valid, `pdfinfo` pages 1/2/1. Court
  outcome: RAW won all 9 items and `PDF_PHYSICAL` won 0 — the expected Phase-3
  result, because the literal physical lane carries no structural compression
  yet (that is Phase 5). Receipt under
  `evidence/campaigns/2026-10-05-phase3-486aa17/`.

### Notes

- The wire format remains **PROVISIONAL** and is not frozen v1. The PDF physical
  authority is implemented and measured; PDF structural compression and
  xref/`/Length` proceduralization remain `PROPOSED` (Phases 5+).

## [0.1.0-alpha.2] — unreleased

### Added

- Phase 2 — native rANS floor (order-0 / typed byte channels):
  - Canonical integer-only entropy model (`MODEL`, 516-byte dense table over the
    byte alphabet) with deterministic normalization and validation.
  - Scalar order-0 byte rANS codec built on the `ryg-rans-rs` **safe manual**
    API; the panicking convenience decoder is not used.
  - Complete entropy **capsule** (model + decoder state + renormalization payload
    + counts) as a typed `ENTROPY_CHANNEL` (33-byte header + payload); never a
    scalar "seed".
  - `DECODE_CHANNEL` DRA op (DRA version 2) and materialization of channels.
  - `BYTE_RANS` and `RLE` candidates competing on complete serialized cost, with
    model bytes charged like any other bytes.
  - Feature `rans` (`default = ["rans"]`); `--no-default-features` keeps the exact
    RAW/RLE floor and returns an explicit `UnsupportedFeature` for
    channel-bearing descriptors instead of a silent reinterpretation.
- Phase-2 universe string
  `vole-document;universe;phase-2;exact-bytes;dra-2;opaque+entropy`.
- Courts: entropy acceptance gates (`tests/entropy.rs`), reference-oracle parity
  and frozen goldens (`tests/goldens.rs`), deterministic property/mutation
  fuzzing (`tests/property.rs`), and the soak script `tools/soak-fuzz.sh`.

### Measured

- Campaign `2026-10-05-phase2-f6af30b` (verdict PASS): on a 9-file mixed corpus,
  `sum_source = 590081`, `sum_core = 464474` (RAW + RLE), `sum_full = 291304`
  (RAW + RLE + BYTE_RANS), `delta = 173170`, fully attributed to the two
  `BYTE_RANS` wins. Negative controls hold (RAW on random, RLE on tiny); model
  cost is charged. This is a scoped order-0 result, not a general compression
  claim. Receipt under `evidence/campaigns/2026-10-05-phase2-f6af30b/`.

### Notes

- The wire format remains **PROVISIONAL** and is not frozen v1. PDF and other
  format-aware mechanisms remain `PROPOSED` (Phases 3+).

## [0.1.0-alpha.1] — unreleased

### Added

- Exact `.voldoc` container core (Phase 1):
  - 64-byte fixed header with CRC-32C self-check and fail-closed version/feature
    negotiation.
  - Length-delimited records with per-record CRC-32C.
  - Typed errors with stable, documented CLI exit codes.
  - Centralized resource [`Limits`].
  - SHA-256 whole-source archival identity.
- Document Reconstruction Algebra literal subset (`EMIT_OBJECT`, `INLINE`,
  `REPEAT_LAST`) with a derived **coverage certificate** validated before
  allocation.
- RAW exact opaque adapter (the correctness floor for every file type).
- Complete-cost candidate court with decode-before-commit: the winner is priced
  from its serialized bytes and byte-compared against the source.
- CLI: `encode`, `decode`, `materialize`, `verify`, `inspect`, `capabilities`.
- Docker-only, digest-pinned toolchain gates: stable (Rust 1.99.0), MSRV
  (Rust 1.89.0), and a PDF oracle tools image (qpdf/Poppler/MuPDF/Ghostscript).
- Courts: exact, malformed (hostile input), conformance.
- Phase 0 research synthesis and ADRs.

### Notes

- No compression headline was claimed in Phase 1. rANS and format-aware
  mechanisms were `PROPOSED` (Phases 2+), not `ADOPTED`.
