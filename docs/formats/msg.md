# MSG (legacy Outlook message, MS-OXMSG)

Legacy Outlook message (`.msg`, MS-OXMSG) is a **CFBF-based legacy semantic model**
built on the shared compound-document physical layer (Phase 21.35): the exact leaf
is the **whole source** (a RAW-like `DocumentExact`) and every derived observation
is a bounded, span-preserving (`Q_gen`) projection. The adapter is gated behind the
**non-default** `msg = ["cfbf"]` feature (it reuses the `cfbf` physical layer).

The model is a bounded MAPI property-stream subset: it claims the top-level message
property stream, the property sub-streams, and the recipient/attachment
sub-storages, and declines everything else **typed**, never guessing and never
panicking.

## Authority boundary

The exact physical source is the whole compound file — the header, DIFAT/FAT/
MiniFAT, the directory tree, and every stream resolved to an exact byte span. No
un-OLE/re-OLE is performed: exactness is the whole source, and the MSG model is
derived (`Q_gen`), never on the exactness path (ADR-0060).

## Detection boundary

The MSG detector is built **on top of** the CFBF detector and runs **after** the
Word, Excel, and PowerPoint detectors and **before** the generic `Cfbf` claim. The
boundary is honest: an OLE file that merely has a root-level stream named
`__properties_version1.0` is admitted here, and its derived model then declines
typed if that stream is not a well-formed message property list. So:

- a `.msg` whose root storage has `__properties_version1.0` becomes `Msg`;
- a generic compound file and an OLE file whose root lacks the property stream stay
  `Cfbf` (and a `msg` selector on them declines typed, exit 6);
- prose stays `Opaque`;
- `.docx`/`.xlsx`/`.pptx` are OPC ZIPs and never reach the CFBF detector at all.

## Native inverse representation

A bounded MS-OXMSG subset:

- the top-level message `__properties_version1.0` header (`Reserved`,
  `NextRecipientId`, `NextAttachmentId`, `RecipientCount`, `AttachmentCount`,
  trailing `Reserved`) and its fixed-size 16-byte property entries (raw tag/id/type
  `0xPPPPTTTT`, flags, value bytes, exact span, owner) **in order**;
- the `__substg1.0_<PPPPTTTT>` sub-streams (`0x001F` UTF-16LE, `0x001E`
  ANSI/CP1252, `0x0102` binary) with their exact spans and decoded text;
- the recipient (`__recip_version1.0_<id>`) and attachment
  (`__attach_version1.0_<id>`) sub-storages with their own property lists and
  sub-streams (order and ids preserved).

The top-level `__properties_version1.0` is a 32-byte header followed by 16-byte
fixed entries; a sub-storage `__properties_version1.0` is an 8-byte `Reserved`
header followed by the same 16-byte entries.

## Supported observations

Common selectors: `metadata`, `text`, `search-match`. Native: `msg-summary`,
`msg-property`, `msg-recipient`, `msg-attachment`, `msg-find`.

## Declined (typed, never guessed)

A `.msg` with a plausible root `__properties_version1.0` but a malformed body — a
property stream shorter than its 32-byte header, a fixed entry that overruns the
stream, a `__substg1.0_*` name that is not `PPPPTTTT` hex, an unreferenced
sub-stream, an unsupported sub-stream type, a `RecipientCount`/`AttachmentCount`
mismatch, a sub-storage without its own property stream, or a non-decimal
sub-storage id — is admitted and its model declines typed
(`InvalidMsgStructure`, exit 55). Never a panic.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted `.msg`,
including those the adapter declines to interpret natively, and after the source
**and** descriptor are deleted in a fresh process.

**Adapter-court H1 (byte-exactness):** **1/1** normal fixture byte-exact
(`msg_valid`, with its recipient and attachment); 8/8 structural declines
`InvalidMsgStructure`; 2/2 CFBF controls stay `Cfbf`; 1/1 opaque control stays
`Opaque`.

## Honest negatives

- **No separate economic court.** There is no faithful conventional decoder for
  legacy MS-OXMSG, so — exactly as for HDF5/CFBF — an economic comparison would be
  `comparator_absent`. The validation is the adapter court plus the stratified
  real-world population (Phase 26.5), not an economic court.
- **Not claimed:** constructs outside the modeled surface — the top-level
  `__properties_version1.0`, the `__substg1.0_*` sub-streams, and the
  recipient/attachment sub-storages — are not claimed (named-property streams,
  body selection the model does not resolve, and deeper embedded-message
  recursion are beyond the bounded subset); as are the other legacy CFBF semantic
  models (see [DOC](doc.md), [XLS](xls.md), [PPT](ppt.md)).
- **Real-world coverage is positive here.** In the Phase 26.5 population all six
  real `.msg` samples (Apache POI `hsmf` messages) were detected as `msg` — the
  legacy `.doc`/`.xls`/`.ppt` detectors, by contrast, recorded declines on several
  real files (see their pages).

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 26.4.1 legacy Outlook MSG adapter court (H1 exactness 1/1):
  [2026-10-11-phase26-4-1-msg-966c3fd4](../../evidence/campaigns/2026-10-11-phase26-4-1-msg-966c3fd4/).
- Stratified real-world population v2 (Phase 26.5):
  [2026-10-11-realformats-stratified-v2-703ca5ac](../../evidence/campaigns/2026-10-11-realformats-stratified-v2-703ca5ac/).
- Results: [phase-26-results.md](../phases/phase-26-results.md).
