# Phase 12 — Universal multi-format document field: PDF + DOCX + EPUB

Branch: `phase12`. Base: `main` @ `6c86938` (`v0.1.0-alpha.15`).
Status: **IN PROGRESS**.

Phase 12 proves the persistent procedural field is **not a PDF trick**. PDF, DOCX
and EPUB enter through different **native inverse compilers** and converge on one
persistent `DocumentField` that exposes **common** observations *and* retained
**format-native** structure, while keeping exact original reconstruction.

## Primary hypothesis (pre-registered, falsifiable)

> A format-neutral persistent procedural `DocumentField` can support repeated
> heterogeneous observations over PDF, DOCX and EPUB while preserving exact
> original reconstruction, and can reduce cumulative parsing / decoding /
> intermediate-materialization / LLM-working-set cost versus **both** direct
> per-query format tooling **and** a competent one-time-preprocessed
> SQLite+FTS baseline **that retains the source**, on at least one clearly
> defined workload region — after the source is deleted and across process
> restarts.

Success is **not** "we parsed three formats". The headline must be a lifetime-cost
result with the DB baseline as an acceptance gate (§117 of the brief).

## Frozen decisions (synthesis of research A–J)

**DEC-1 — Three representational layers stay distinct.** (A) exact physical source
state (PDF spans; ZIP local headers / compressed member spans / central directory
/ EOCD / ZIP64 / comments; XML member bytes); (B) format-native procedural state
(PDF object/stream/revision; OPC part/relationship/WordprocessingML
paragraph/table/story; EPUB package/manifest/spine/nav/XHTML); (C) a small shared
observation vocabulary. Layers reference each other; they are never conflated.

**DEC-2 — No lossy universal AST.** The common layer is a *vocabulary* plus native
escape hatches, not a lowest-common-denominator schema. `Selector::{Pdf,Docx,Epub}`
remain first-class.

**DEC-3 — A byte-authoritative ZIP physical layer, not a logical member API.**
One scanner shared by DOCX and EPUB, analogous to the PDF physical scanner: a
complete byte cover of `[0,N)` (`Prefix · LocalHeader · MemberData ·
DataDescriptor · CentralDirectory · ZIP64(EOCD|Locator) · EOCD(comment) ·
Trailing · Unclassified`) with `validate()` (no gap/overlap/wrong total).
`PhysicalMemberId = (archive ordinal, local-header offset)` distinct from the
advisory `LogicalPartName`; duplicate names are never normalized. The exact leaf
for a member is its **raw compressed span** — no unzip/rezip, no bit-exact
recompression required. Deflate reuses `miniz_oxide`; ZIP CRC is CRC-32/ISO-HDLC
(`crc32fast`), **not** CRC-32C. `zip`/`rawzip` are **oracle-only**.

**DEC-4 — XML parsing is derived-only.** `quick-xml =0.42.0`
(`default-features=false`, no DTD/entities, UTF-8) produces `Q_gen` structure;
exact XML bytes remain `Q_ref` authority. No `encoding_rs` (licence gate); a
non-UTF-8 part is preserved exactly and its semantic observation declined.

**DEC-5 — Capabilities are explicit.** Formats do not share coordinates. Reflowable
EPUB has **no intrinsic pages**; DOCX has **no intrinsic pagination**; capability
discovery (`capabilities ROOT`, machine-readable) reports supported
selectors/representations; unsupported observations return typed capability
errors. Never invent `Page(n)`.

**DEC-6 — Extraction is profile-driven, not hidden.** DOCX/EPUB text/table
projections take a versioned profile (tracked changes Final/Original, field
result vs code, notes/include, hidden, nav, scripted-static-only, linear-only);
the profile identity is recorded in the answer.

**DEC-7 — Progressive unbaking inside members.** `RawCompressedMember → DecodedMember
→ XMLTokenState → NativeNodes → SharedObservations`; deepen only on demand;
persist newly recovered state. Eager and progressive strategies are **both**
measured (§106).

**DEC-8 — Identity: three kinds.** content identity (bytes; cross-doc),
logical-occurrence identity (per-document), physical-source identity (span).
Sharing requires **exact versioned content identity**; cross-format semantic
dedup is **out of scope** until canonicalization/versioning/provenance/round-trip
are proven. Reuse is about *state*, not bytes (`retained_inverse_work_fraction`).

**DEC-9 — Security: ambiguous cover = reject; semantics/resource = preserve
opaque.** Bombs/unknown methods/encryption/CRC faults → preserve exact bytes,
decline the observation. Overlap/impossible offsets/ZIP64 contradiction/
malformed descriptors/multi-disk/traversal → typed rejection. No `PathBuf` from a
member name, ever. New limits + `ErrorClass::{InvalidZipStructure,
InvalidXmlStructure, InvalidPackageStructure}`.

**DEC-10 — Exactness and authority are untouched.** PDF exactness must not
regress (§109); DOCX exactness = exact original ZIP bytes; EPUB exactness = exact
original OCF/ZIP bytes (a valid re-zip does not count). The
zero-decode-authority boundary (`SourceServer`) is unchanged; the field never
gates `materialize(descriptor) == original_bytes`.

## Architecture

```text
PDF  ─┐
DOCX ─┼─→ format-native inverse compilers ─→ persistent DocumentField (seed DAG)
EPUB ─┘        (share the ZIP physical layer)         │
                                                       ├── common observations
                                                       │   (metadata/text/heading/
                                                       │    block/table/cell/
                                                       │    resource/link/find)
                                                       ├── native observations
                                                       │   (Pdf/Docx/Epub selectors)
                                                       └── exact closure
                                                              │
                       selective late materialization ←───────┘
```

Module plan (one crate, internal modules): `src/adapter/package/` (ZIP physical +
OPC core), `src/adapter/docx/`, `src/adapter/epub/`, shared additions in
`src/field/` (node kinds, common selectors/representations, planner/capabilities),
plus per-format index kinds and extract profiles.

## Subphases (commit + push each)

- **12.0** research + freeze — this file + ADRs 0029–0035.
- **12.1** generic byte-authoritative ZIP physical layer + exactness court.
- **12.2** package procedural state (member nodes, partial reads, progressive
  member decoding, EntropyFS persistence, EXPLAIN).
- **12.3** OPC core (content types, package/part relationships, part-name
  resolution, internal/external targets, package graph) — no WML assumptions.
- **12.4** DOCX adapter (declared WordprocessingML scope; stories; tables;
  resources; native provenance; common mappings).
- **12.5** EPUB OCF/package adapter (container, package, metadata, manifest, spine,
  nav, resource graph).
- **12.6** EPUB content observations (bounded XHTML: headings/paragraphs/lists/
  tables/links/resources/text; no scripting).
- **12.7** universal observation API (PDF+DOCX+EPUB through one `DocumentField`;
  `capabilities`; common + native selectors; provenance; EXPLAIN/ANALYZE).
- **12.8** cross-document procedural state (resources/decoded members/transforms;
  measured reuse, not byte claims).
- **12.9** cross-format equivalence court (canonical logical triplet).
- **12.10** source-removal/restart court (per format).
- **12.11** lifetime AI-document workload court (A0/A1/A1b vs Phase 11 vs Phase 12).
- **12.12** LLM working-set court (pinned tokenizer; bytes+tokens).
- **12.13** security + fuzz campaign.
- **12.14** reproducible flagship demo (`tools/phase12-demo.sh`).
- **12.15** independent skeptic review (correct docs before release).
- **12.16** release (gates + merge + tag + publish).

## Required ablations (§105)

A0 direct tooling · A1 preprocessed SQLite+FTS (source-retaining) · A2 Phase-11
field · A3 ZIP physical only · A4 + package graph · A5 + progressive semantic
inversion · A6 + persistent reuse · A7 + common observation layer · A8 +
hierarchical indexes · A9 + EntropyFS fine-grained range access · A10 +
cross-document sharing · A11 full Phase 12. Plus eager-vs-progressive (§106) and
raw-compressed-vs-decoded-persisted (§107).

## Acceptance gates (§109–122)

PDF no regression; DOCX/EPUB exactness (len+SHA-256+`cmp`); source independence
(delete + restart); common API + native API; progressive inversion with persistent
reuse; no whole-document bake for narrow observations (instrumented); fair DB
comparison; complete accounting; cross-format court; ≥1 genuine cross-document
reuse; hostile-input safety; reproducible demo.

## Negative controls (§123)

Tiny DOCX/EPUB; single-member ZIP; huge compressed XML; image-only package; no
tables / no headings; malformed package; encrypted member; random opaque ZIP;
duplicate-name ZIP; high-entropy resources; a query perfectly suited to SQLite;
a full-source materialization request. Some must favour the baseline — record it.

## Claim discipline (§124–132)

No "all documents"; no "VOLE beats databases/RAG"; no "eliminates ETL"; no
"EPUB has pages" / "DOCX observations are page-exact"; no "EntropyFS is
inherently faster"; no token claims without a named pinned tokenizer. Every claim
names corpus, workload, ingest, and the four accounting universes (ADR-0027).
Negative results are permanent and recorded.

## Working rules

Docker only (never the host); every service digest-pinned and memory-capped;
one production crate; subagents one at a time (research read-only under
gitignored `research/subagents/phase-12/`); commit + push per subphase; an
independent skeptic tries to falsify each headline.
