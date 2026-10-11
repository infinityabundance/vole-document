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
- the `SlideListWithText` slides (`persistIdRef`, in order);
- the `TextCharsAtom` (UTF-16LE) and `TextBytesAtom` (CP1252) runs with their exact
  spans and decoded text;
- the `Current User` stream's `UserEditAtom` offset when it resolves.

## Supported observations

Common selectors: `metadata`, `text`, `search-match`. Native: `ppt-document`,
`ppt-slide`, `ppt-run`, `ppt-record`, `ppt-find`.

## Declined (typed, never guessed)

A `Document` container whose declared `recLen` is smaller than its children (a
child overruns its container), a `recLen` that overruns the stream, a truncated
record header, an odd-length `TextCharsAtom` (a text-atom/`recLen` mismatch), a
text atom with no preceding `SlidePersistAtom`, and an out-of-range `Current User`
`UserEditAtom` offset are typed declines (`InvalidPptStructure`, exit 54).

A `.ppt` with a plausible `Document` record but a malformed body **is admitted**
(its whole-source exact leaf is unaffected) and its model then declines typed.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted `.ppt`,
including those the adapter declines to interpret natively, and after the source
**and** descriptor are deleted in a fresh process.

**Adapter-court H1 (byte-exactness):** **1/1** normal fixture byte-exact
(`ppt_valid`, with its text runs); 5/5 structural declines `InvalidPptStructure`;
2/2 CFBF controls stay `Cfbf`; 1/1 opaque control stays `Opaque`.

## Honest negatives

- **No separate economic court.** There is no faithful conventional decoder for
  legacy MS-PPT, so — exactly as for HDF5/CFBF — an economic comparison would be
  `comparator_absent`. The validation is the adapter court plus the stratified
  real-world population (Phase 26.5), not an economic court.
- **Not claimed:** the `Current User` stream's *content* beyond the `UserEditAtom`
  offset, the master/notes `SlideListWithText` lists (only the slides list is
  modeled), embedded OLE objects, pictures, and the other legacy CFBF semantic
  models (see [DOC](doc.md), [XLS](xls.md), [MSG](msg.md)).
- **Real-world declines are expected.** In the Phase 26.5 population three genuine
  Apache POI `.ppt` files (`poi-slideshow-54332a.ppt`,
  `poi-slideshow-basic_test_ppt_file.ppt`, `poi-slideshow-datetime.ppt`) have a
  `PowerPoint Document` stream whose record variant this bounded detector does not
  claim, so they stay at the generic `Cfbf` layer — a **recorded** boundary.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 26.3.1 legacy PowerPoint adapter court (H1 exactness 1/1):
  [2026-10-11-phase26-3-1-ppt-966c3fd4](../../evidence/campaigns/2026-10-11-phase26-3-1-ppt-966c3fd4/).
- Stratified real-world population v2 (Phase 26.5):
  [2026-10-11-realformats-stratified-v2-703ca5ac](../../evidence/campaigns/2026-10-11-realformats-stratified-v2-703ca5ac/).
- Results: [phase-26-results.md](../phases/phase-26-results.md).
