# XLS (legacy Excel 97–2003, BIFF8)

Legacy Excel (`.xls`, BIFF8) is a **CFBF-based legacy semantic model** built on
the shared compound-document physical layer (Phase 21.35): the exact leaf is the
**whole source** (a RAW-like `DocumentExact`) and every derived observation is a
bounded, span-preserving (`Q_gen`) projection. The adapter is gated behind the
**non-default** `xls = ["cfbf"]` feature (it reuses the `cfbf` physical layer).

The model is a bounded BIFF8 subset: it claims a specific set of records and
declines everything else **typed**, never guessing and never panicking.

## Authority boundary

The exact physical source is the whole compound file — the header, DIFAT/FAT/
MiniFAT, the directory tree, and every stream resolved to an exact byte span. No
un-OLE/re-OLE is performed: exactness is the whole source, and the XLS model is
derived (`Q_gen`), never on the exactness path (ADR-0060).

## Detection boundary

The XLS detector is built **on top of** the CFBF detector and runs **after** the
Word detector and **before** PowerPoint/MSG and the generic `Cfbf` claim. So:

- a `.xls` whose `Workbook`/`Book` stream opens with a BIFF8 `BOF` (id `0x0809`,
  version `0x0600`) whose substream type is a recognized worksheet/globals type
  becomes `Xls`;
- a generic compound file, a `.xls` with a bad `BOF` version (BIFF5/7), and a
  `.xls` with an unrecognized substream type stay `Cfbf`;
- prose stays `Opaque`;
- **`.xlsx` is an OPC ZIP** and never reaches the CFBF detector at all — it is
  detected as [XLSX](xlsx.md) in the ZIP family.

**Detection hardening (Phase 27.3).** The detector now reads only the stream
**header prefix** (`cfbf::stream_leading`), so a **fragmented** `Workbook` chain
(non-contiguous CFBF sector chain) still detects — the stream's leading bytes are
not required to be one contiguous span. It also matches the well-known
`Workbook`/`Book` stream name **ASCII-case-insensitively**, so a `WORKBOOK` stream
detects. Both are regression-tested
(`legacy_cfbf_detection::fragmented_workbook_stream_is_detected`,
`legacy_cfbf_detection::uppercase_workbook_stream_name_is_detected`). The two real
Apache POI `.xls` files that previously stayed `Cfbf`
(`poi-spreadsheet-43493.xls`, `poi-spreadsheet-bug66319.xls`) now detect as `xls`;
the real root cause was the **fragmented chain**, not the BIFF8 variant.

## Native inverse representation

A bounded BIFF8 record-stream subset:

- the record stream (`[u16 id][u16 len][payload]`) with every record's exact byte
  span, and the `BOF`/`EOF` substream framing and BIFF version;
- the ordered `BOUNDSHEET` sheets (name and `BOF` stream position);
- the `SST` shared-string table, with `CONTINUE` reassembly of a continued string;
- the per-sheet cells (`LABELSST`/`NUMBER`/`RK`/`BOOLERR`/`FORMULA`+`STRING`/
  `BLANK`/`MULBLANK`) with row/column, exact record span, and the exact **stored
  value spelling** (the `RK` compact numeric encoding is decoded);
- `DIMENSIONS`/`ROW`/`FORMAT`/`XF` recorded by span.

## Supported observations

Common selectors: `metadata`, `text`, `search-match`. Native: `xls-biff`,
`xls-sheet`, `xls-cell`, `xls-record`, `xls-find`.

## Declined (typed, never guessed)

A bad `BOF` version, an unrecognized substream type, a leading substream that is
not workbook globals, a truncated record, an out-of-range `BOUNDSHEET` position,
an `SST` count mismatch or a malformed `CONTINUE`, and a `FORMULA` string result
with no `STRING` record are typed declines (`InvalidXlsStructure`, exit 53).

A `.xls` with a plausible BIFF8 `BOF` but a malformed body **is admitted** (its
whole-source exact leaf is unaffected) and its model then declines typed.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted `.xls`,
including those the adapter declines to interpret natively, and after the source
**and** descriptor are deleted in a fresh process.

**Adapter-court H1 (byte-exactness):** **1/1** normal fixture byte-exact
(`xls_valid`, with its cells); 4/4 structural declines `InvalidXlsStructure`; 3/3
CFBF controls stay `Cfbf`; 1/1 opaque control stays `Opaque`.

## Honest negatives

- **Economic court (Phase 27.4).** A separate economic court now exists: a
  source-retaining SQLite lane (**Lane B**, byte authority) is the single
  substantive comparator, and the conventional decode-to-host-values lane
  (**Lane C**) is `comparator_absent` — there is no faithful pure-Python/stdlib
  decoder for legacy BIFF8, so every Lane-C question is a recorded typed decline
  rather than a fabricated answer. VOLE exactness (Q6) is **5/5** byte-exact; VOLE
  builds cheaper than the byte store (paired median **0.548×**) and is far cheaper
  cold/warm, but carries **~1.19×** the storage of a bytes-only store; cold is
  Python-startup-dominated and the single-fixture CIs are degenerate. Only Q6 is a
  byte-authority claim.
- **Not claimed:** BIFF5/7 (an older `Book` stream with 1-byte string lengths),
  password-encrypted/obfuscated workbooks, and the other legacy CFBF semantic
  models (see [DOC](doc.md), [PPT](ppt.md), [MSG](msg.md)).
- **Real-world declines were detection, not modeling.** The two genuine Apache POI
  `.xls` files the Phase 26.5 population recorded at `Cfbf`
  (`poi-spreadsheet-43493.xls`, `poi-spreadsheet-bug66319.xls`) were declined
  because their `Workbook` stream is a **fragmented** CFBF sector chain; Phase 27.3
  fixed the detector to read the stream header prefix, so they now detect as
  `xls`.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 26.2.1 legacy Excel adapter court (H1 exactness 1/1):
  [2026-10-11-phase26-2-1-xls-966c3fd4](../../evidence/campaigns/2026-10-11-phase26-2-1-xls-966c3fd4/).
- Phase 27.3 detection hardening (fragmented chain + case-insensitive
  `Workbook`): `tests/legacy_cfbf_detection.rs`.
- Phase 27.4 legacy Excel **economic** court (Lane B source-retaining SQLite, Lane
  C `comparator_absent`; exactness 5/5):
  [2026-10-12-phase27-4-xls-econ-58f7572f](../../evidence/campaigns/2026-10-12-phase27-4-xls-econ-58f7572f/).
- Stratified real-world population v2 (Phase 26.5; re-run under 27.3/27.5):
  [2026-10-11-realformats-stratified-v2-703ca5ac](../../evidence/campaigns/2026-10-11-realformats-stratified-v2-703ca5ac/).
- Results: [phase-26-results.md](../phases/phase-26-results.md),
  [phase-27-results.md](../phases/phase-27-results.md).
