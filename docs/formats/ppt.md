# PPT (legacy PowerPoint 97–2003, MS-PPT)

Legacy PowerPoint (`.ppt`, MS-PPT) is a **CFBF-based legacy semantic model** built
on the shared compound-document physical layer (Phase 21.35): the exact leaf is the
**whole source** (a RAW-like `DocumentExact`) and every derived observation is a
bounded, span-preserving (`Q_gen`) projection. The adapter is gated behind the
**non-default** `ppt = ["cfbf"]` feature (it reuses the `cfbf` physical layer).

The model is a bounded MS-PPT record-stream subset: it claims a specific set of
records and declines everything else **typed**, never guessing and never panicking.

## Authority boundary

The exact physical source is the whole compound file — the header, DIFAT/FAT/
MiniFAT, the directory tree, and every stream resolved to an exact byte span. No
un-OLE/re-OLE is performed: exactness is the whole source, and the PPT model is
derived (`Q_gen`), never on the exactness path (ADR-0060).

## Detection boundary

The PPT detector is built **on top of** the CFBF detector and runs **after** the
Word and Excel detectors and **before** MSG and the generic `Cfbf` claim. So:

- a `.ppt` whose `PowerPoint Document` stream opens with a `Document` record
  (`recVer 0xF`, `recType 1000`, header `0F 00 E8 03`) becomes `Ppt`;
- a generic compound file, and a `PowerPoint Document` stream whose first record
  is not a `Document`, stay `Cfbf`;
- prose stays `Opaque`;
- **`.pptx` is an OPC ZIP** and never reaches the CFBF detector at all — it is
  detected as [PPTX](pptx.md) in the ZIP family.

## Native inverse representation

A bounded MS-PPT record-stream subset:

- the record stream (the 8-byte `RecordHeader` with `recVer = w & 0x0F`,
  `recInstance = (w >> 4) & 0x0FFF`, `recType`, `recLen`) with every record's exact
  byte span, stream offset, and nesting depth;
- the `Document` container (the top-level record);
- every `SlideListWithText` container (`recType 4080`), distinguished by
  `recInstance` — `slides` (0), `masters` (1), `notes` (2), `handout` (3);
- the ordered `SlidePersistAtom` entries (`persistIdRef`, `cTexts`, exact span);
- the `TextCharsAtom` (UTF-16LE) and `TextBytesAtom` (CP1252) runs with their exact
  atom/header spans and decoded text;
- the `Current User` stream's `UserEditAtom` offset when it resolves.

The model is a derived `PptModel` whose wire **version is 2**: the
`SlideListWithText` list arena is the source of truth, and the `slides`/`runs`
projection is derived from the `slides` list. The model is derived and never on
the exactness path.

## List surface (Phase 27.2)

Every `SlideListWithText` container is materialized as its own typed answer via
`--ppt-list <name>` (`slides`|`masters`|`notes`|`handout`): the exact container
span, the ordered `SlidePersistAtom` entries, and for each entry the ordered text
runs with their exact spans and decoded text. An absent list and a
present-but-empty list are typed **empty** answers (`slides`: 0, `runs`: 0,
`empty`: true), never errors; a present container still reports its span. The
slides-list projection (`ppt-slide`/`ppt-run`/`ppt-record`, canonical text) is
byte-for-byte unchanged.

## Supported observations

Common selectors: `metadata`, `text`, `search-match`. Native: `ppt-document`,
`ppt-slide`, `ppt-run`, `ppt-record`, `ppt-list`
(`slides`/`masters`/`notes`/`handout`), `ppt-find`.

## Declined (typed, never guessed)

A `Document` container whose declared `recLen` is smaller than its children (a
child overruns its container), a `recLen` that overruns the stream, a truncated
record header, an odd-length `TextCharsAtom` (a text-atom/`recLen` mismatch) —
including one inside an **auxiliary** list (`masters`/`notes`/`handout`), which
Phase 26 silently skipped — a text atom with no preceding `SlidePersistAtom`, and
an out-of-range `Current User` `UserEditAtom` offset are typed declines
(`InvalidPptStructure`, exit 54).

A `.ppt` with a plausible `Document` record but a malformed body **is admitted**
(its whole-source exact leaf is unaffected) and its model then declines typed.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted `.ppt`,
including those the adapter declines to interpret natively, and after the source
**and** descriptor are deleted in a fresh process.

**Adapter-court H1 (byte-exactness):** **2/2** normal fixtures byte-exact
(`ppt_valid`, `ppt_lists`); 6/6 structural declines `InvalidPptStructure`; 2/2
CFBF controls stay `Cfbf`; 1/1 opaque control stays `Opaque`.

## Honest negatives

- **Economic court (Phase 27.4).** A separate economic court now exists: a
  source-retaining SQLite lane (**Lane B**, byte authority) is the single
  substantive comparator, and the conventional decode-to-host-values lane
  (**Lane C**) is `comparator_absent` — there is no faithful pure-Python/stdlib
  decoder for legacy MS-PPT, so every Lane-C question is a recorded typed decline
  rather than a fabricated answer. VOLE exactness (Q6) is **5/5** byte-exact; VOLE
  builds cheaper than the byte store (paired median **0.738×**) and is far cheaper
  cold/warm, but carries **~1.19×** the storage of a bytes-only store; cold is
  Python-startup-dominated and the CIs are degenerate at n=1–2. Only Q6 is a
  byte-authority claim.
- **Not claimed:** the `Current User` stream's *content* beyond the `UserEditAtom`
  offset, embedded OLE objects, pictures, and the other legacy CFBF semantic
  models (see [DOC](doc.md), [XLS](xls.md), [MSG](msg.md)).
- **Real-world declines were detection, not modeling.** The Phase 26.5 population's
  three genuine Apache POI `.ppt` files (`poi-slideshow-54332a.ppt`,
  `poi-slideshow-basic_test_ppt_file.ppt`, `poi-slideshow-datetime.ppt`) stayed at
  `Cfbf` because their `PowerPoint Document` stream is a **fragmented** CFBF sector
  chain, not because of a record variant. Phase 27.3 fixed the detector to read
  only the stream header prefix (`cfbf::stream_leading`), so these files now detect
  as `ppt`.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 26.3.1 legacy PowerPoint adapter court (H1 exactness 1/1):
  [2026-10-11-phase26-3-1-ppt-966c3fd4](../../evidence/campaigns/2026-10-11-phase26-3-1-ppt-966c3fd4/).
- Phase 27.2.1 legacy PowerPoint **list** court (H1 exactness 2/2;
  slides/masters/notes/handout):
  [2026-10-12-phase27-2-1-ppt-lists-64626a39](../../evidence/campaigns/2026-10-12-phase27-2-1-ppt-lists-64626a39/).
- Phase 27.4 legacy PowerPoint **economic** court (Lane B source-retaining SQLite,
  Lane C `comparator_absent`; exactness 5/5):
  [2026-10-12-phase27-4-ppt-econ-58f7572f](../../evidence/campaigns/2026-10-12-phase27-4-ppt-econ-58f7572f/).
- Stratified real-world population v2 (Phase 26.5; re-run under 27.3/27.5):
  [2026-10-11-realformats-stratified-v2-703ca5ac](../../evidence/campaigns/2026-10-11-realformats-stratified-v2-703ca5ac/).
- Results: [phase-26-results.md](../phases/phase-26-results.md),
  [phase-27-results.md](../phases/phase-27-results.md).
