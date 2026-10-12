# Phase 27 results — legacy depth, detection hardening, legacy economics, Arrow streams

Branch `phase27`. Phase 27 closes the four gaps Phase 26's own report named, on the
same rule as every other subphase: exactness is the invariant, negatives are
recorded, no claim without evidence. The frozen contract is
[phase-27-plan.md](phase-27-plan.md).

```text
27.1  Legacy DOC depth      — footnote/endnote/header/annotation/textbox/hdrtextbox CP sections
27.2  Legacy PPT depth      — master/notes/handout SlideListWithText lists
27.3  Detection hardening   — attack the recorded stratified-v2 findings
27.4  Legacy economic court — doc/xls/ppt/msg vs a source-retaining baseline
27.5  Arrow IPC stream detection — magic-less framing + a conformant stream fixture
```

All five subphases are **shipped**. The legacy-depth work extends the Phase-26
DocModel/PptModel, the detection hardening re-runs the stratified real-world court
that produced the findings, the legacy economic court measures the (now deeper)
legacy models on an equal contract, and the Arrow stream fix is a self-contained
detection/corpus correction.

## 27.1 — Legacy DOC depth (the sectioned surface)

The single contiguous CP space of the piece table is partitioned by the cumulative
FIB `ccp*` counts into seven sections and each is exposed as its own
span-preserving (`Q_gen`) answer via `--doc-section`:
`main`/`footnote`/`header`/`annotation`/`endnote`/`textbox`/`hdrtextbox`. Each
answer carries the section name and FIB `ccp*` field, the CP range, the ordered
pieces clipped to it (piece index, clipped CP range, exact `WordDocument`-stream
byte span, compressed flag, decoded text), and the concatenated decoded text; a
piece that crosses a section boundary appears clipped in every section it
overlaps; a zero-count section is a typed **empty** answer (never an error). The
main text `[0, ccpText)` and the existing `doc-fib`/`doc-piece`/`doc-text`/
`doc-find` answers are unchanged.

**Adapter-court H1 (byte-exactness), after source *and* descriptor deletion in a
fresh process:** **3/3** normal fixtures byte-exact (`doc_0table`, `doc_1table`,
`doc_sections`; the last has pieces deliberately crossing section boundaries), 4/4
structural declines `InvalidDocStructure` (exit 52), 3/3 CFBF controls stay `Cfbf`,
1/1 opaque control stays `Opaque`. The court also answers all **7/7** sections,
clips **10/10** boundary-crossing pieces exactly, and reports the empty
`hdrtextbox` as a typed empty answer; an unknown `--doc-section` name is a usage
error (exit 2). Per-format boundaries: [DOC](../formats/doc.md). Receipt:
[`2026-10-12-phase27-1-1-doc-section-fc5cce39`](../../evidence/campaigns/2026-10-12-phase27-1-1-doc-section-fc5cce39/).

**Not claimed (out of the sectioned subset):** the header/footer **story → section**
mapping and the textbox/header-textbox **anchors**; `doc-find` remains
document-level (not piece-local or section-scoped); the section text is decoded
CP1252/UTF-16 text with the exact bytes still available through the piece spans.

## 27.2 — Legacy PPT depth (the list surface)

Every `SlideListWithText` container (`recType 4080`) is materialized as its own
typed answer via `--ppt-list`, distinguished by `recInstance`: `slides` (0),
`masters` (1), `notes` (2), `handout` (3). Each answer carries the exact container
span, the ordered `SlidePersistAtom` entries (`persistIdRef`, `cTexts`, exact
span), and for each entry the ordered `TextCharsAtom` (UTF-16LE) / `TextBytesAtom`
(CP1252) runs with their exact atom/header spans and decoded text. An absent or
present-but-empty list is a typed **empty** answer (`slides`: 0, `runs`: 0,
`empty`: true). The derived `PptModel` wire **version is 2**: the list arena is now
the source of truth and the `slides`/`runs` projection is derived from the `slides`
list — while the slides-list projection itself stays byte-for-byte unchanged.

**Adapter-court H1 (byte-exactness):** **2/2** normal fixtures byte-exact
(`ppt_valid`, `ppt_lists`), 6/6 structural declines `InvalidPptStructure` (exit 54,
now including a malformed **auxiliary** list — an odd-length `TextCharsAtom` in a
notes list, which Phase 26 silently skipped), 2/2 CFBF controls stay `Cfbf`, 1/1
opaque control stays `Opaque`. Per-format boundaries: [PPT](../formats/ppt.md).
Receipt:
[`2026-10-12-phase27-2-1-ppt-lists-64626a39`](../../evidence/campaigns/2026-10-12-phase27-2-1-ppt-lists-64626a39/).

## 27.3 + 27.5 — Detection hardening

The stratified real-world population v2 court was re-run twice, and every sample
stayed **byte-exact** throughout (**308/308**, 0 skipped, 0 blocked):

| Run | Findings | Detection agreements | Receipt |
|---|---:|---:|---|
| Phase 26 (control) | 32 | 257 | [`…realformats-stratified-v2-703ca5ac`](../../evidence/campaigns/2026-10-11-realformats-stratified-v2-703ca5ac/) |
| Phase 27.3 | 25 (−7) | 264 (+7) | [`…realformats-stratified-v2-6f6f84d0`](../../evidence/campaigns/2026-10-12-realformats-stratified-v2-6f6f84d0/) |
| Phase 27.5 | 21 (−4) | 268 (+4) | [`…realformats-stratified-v2-efb51887`](../../evidence/campaigns/2026-10-12-realformats-stratified-v2-efb51887/) |

**The 7 fixes in 27.3.** Their real root cause was **fragmented CFBF sector
chains** — not the "variant gap" first recorded. The legacy PPT and XLS detectors
now read only a stream's **header prefix** (`cfbf::stream_leading`), so a
non-contiguous `PowerPoint Document`/`Workbook` chain still detects; the XLS
detector also matches the well-known `Workbook`/`Book` stream name
**ASCII-case-insensitively** (so a `WORKBOOK` stream detects); and the GIS detector
now claims **GPX 1.0** and a **namespace-less `<kml>`**. Concretely: 3 fragmented
POI `.ppt` files + 1 fragmented POI `.xls` + 1 uppercase-`WORKBOOK` `.xls` + 1
GPX-1.0 + 1 namespace-less-KML = **7**. Regression tests:
`legacy_cfbf_detection::fragmented_powerpoint_document_stream_is_detected`,
`…::fragmented_workbook_stream_is_detected`,
`…::uppercase_workbook_stream_name_is_detected`,
`gis::tests::detects_kml_and_gpx_and_rejects_junk`,
`gis_adapter::detection_is_byte_based_and_conservative`.

**The 4 fixes in 27.5.** The four real bare Apache Arrow IPC **stream** fixtures
(`arrow-stream-*`) — explicitly deferred by 27.3 — are now claimed as `arrow`:
the detector accepts the magic-less encapsulated-message framing (a
continuation-marked `Schema` message first, then well-formed
`RecordBatch`/`DictionaryBatch` messages, ending in an end-of-stream marker), in
addition to the file format and the historical magic+padding-prefixed stream. The
message inventory carries exact byte spans, contiguous from offset 0 and covering
the whole source. The **legacy pre-0.15 no-continuation** framing is deliberately
declined. Regression tests:
`arrow_adapter::stream_framing_is_detected_and_messages_are_ordered_and_exact`,
`arrow_adapter::stream_near_misses_decline_to_opaque`; court
`tools/phase27-5-1-arrow-stream-court.sh` (receipt
[`2026-10-12-phase27-5-1-arrow-stream-efb51887`](../../evidence/campaigns/2026-10-12-phase27-5-1-arrow-stream-efb51887/)).

**The remaining 21 are recorded, not fixed** — 11 `misclassified` and 10
`decline-opaque` — each with its reason in
[`findings-v2-reconciliation.tsv`](../../tools/realcorpus/findings-v2-reconciliation.tsv):

- **Detector-precedence choices (11 misclassified).** AsciiDoc claimed by reST
  (5: attribute-only/locale/CHANGELOG pages — AsciiDoc attribute entries are
  syntactically RST field lists); one real CPython `whatsnew` reST claimed by
  Markdown (the over+underlined `***` title is a Markdown thematic break); a
  GitHub-Actions YAML the bounded YAML detector declines claimed by config; an
  OpenStack log stream claimed by config and a Windows event log claimed by CSV
  (both before the later logstream detector). Reordering would risk the
  load-bearing reST/config/CSV detectors, so none was reordered.
- **Conservatism declines (10 decline-opaque).** A link-list-only AsciiDoc page
  (no AsciiDoc-specific mark); three CBOR samples below the detector's
  container/tag bar (a lone integer, a tagged date, an RFC 8949 multi-value
  concatenation); an mbox `multipart/report` whose boundary is folded/truncated;
  four log dialects outside the admitted timestamp/level grammar (HPC, Mac,
  Proxifier, Spark); and a truncated PPTX with no EOCD/central directory.
- **DOC detection declines (2, counted as misclassified).** `poi-document-Bug60942b.doc`
  is a **Word 6.0** FIB (`wIdent` `0xA5DC`, `nFib` `0x0065` — outside the supported
  Word 97–2003 subset), and `poi-document-testRangeDelete.doc` has a genuine FIB
  but a **fragmented root mini stream** (the `WordDocument` mini-stream and the
  root mini stream that indexes it), so the first chunk's absolute offset is not
  recoverable and the header-prefix read declines. Both stay at the generic `Cfbf`
  layer, byte-exact.

A *recorded* finding never changes the exact bytes; the byte-authority gate
(`materialize == source`) holds for every sample. The 60-sample v1 court
([`2026-10-10-realformats-stratified-3529688e`](../../evidence/campaigns/2026-10-10-realformats-stratified-3529688e/))
remains the historical control.

## 27.4 — the legacy economic courts

The four legacy formats now have **economic courts** (doc/xls/ppt/msg), measured
against a source-retaining SQLite store (**Lane B**, byte authority) and a
conventional decode-to-host-values load (**Lane C**). **Lane C is
`comparator_absent`**: legacy binary Office/MS-OXMSG has no faithful
pure-Python/stdlib decoder in the pinned image, so rather than fabricate one every
Lane-C question is recorded as a typed `comparator_absent` decline (exactly as the
HDF5/CFBF courts do). The head-to-head is therefore limited to a single substantive
comparator — the bytes store — which itself declines every *structural* question.

**VOLE exactness (Q6), after source *and* descriptor deletion in a fresh process:**

| Format | Corpus | Q6 exactness | Build (vs SQLite) | Storage (vs SQLite) | Cold | Warm |
|---|---:|---:|---:|---:|---:|---:|
| DOC | 3 fixtures | **7/7** | 0.793× (win) | 1.436× (loss) | 0.026× | 0.100× |
| XLS | 1 fixture | **5/5** | 0.548× (win) | 1.188× (loss) | 0.025× | 0.111× |
| PPT | 2 fixtures | **5/5** | 0.738× (win) | 1.188× (loss) | 0.021× | 0.101× |
| MSG | 1 fixture | **4/4** | 0.803× (win) | 1.209× (loss) | 0.022× | 0.118× |

Receipts:
[doc](../../evidence/campaigns/2026-10-12-phase27-4-doc-econ-58f7572f/),
[xls](../../evidence/campaigns/2026-10-12-phase27-4-xls-econ-58f7572f/),
[ppt](../../evidence/campaigns/2026-10-12-phase27-4-ppt-econ-58f7572f/),
[msg](../../evidence/campaigns/2026-10-12-phase27-4-msg-econ-58f7572f/).

**Read honestly.** The estimator is the paired per-fixture VOLE/comparator ratio
(median + geometric mean, fixed-seed cluster bootstrap; ADR-0054). VOLE builds
cheaper than the byte store and is far cheaper cold/warm, but it pays **~1.19–1.44×
the storage** of a bytes-only store (the descriptor/DAG/index cost over the raw
blob), and cold is **Python-startup-dominated** (each comparator cold probe is a
fresh `python3` process). At the small fixture counts (n=1–3) the bootstrap CIs are
**degenerate**. Only Q6 is a byte-authority claim. This closes the Phase-26 scope
note that said no separate legacy economic court existed.

## Cross-cutting

- Format pages: [DOC](../formats/doc.md), [XLS](../formats/xls.md),
  [PPT](../formats/ppt.md), [MSG](../formats/msg.md),
  [Arrow IPC](../formats/arrow.md).
- Capability matrix, new selectors, detection boundaries and receipts:
  [format-support.md](../reference/format-support.md).
- Detection reconciliation:
  [`findings-v2-reconciliation.tsv`](../../tools/realcorpus/findings-v2-reconciliation.tsv).

## What is not claimed

- The legacy models remain **bounded**: a construct outside the subset (including
  the DOC story→section mapping, textbox anchors, and piece-local `doc-find`) is a
  typed decline or an explicitly unclaimed surface, never a guess.
- The 308-sample population is **stratified, not a random sample of the world**, and
  a recorded detector finding never changes the exact bytes.
- The four legacy economic courts are **self-authored deterministic corpora** and
  their Lane C is `comparator_absent`; they are VOLE-vs-a-bytes-store measurements,
  not decode-vs-decode comparisons.
