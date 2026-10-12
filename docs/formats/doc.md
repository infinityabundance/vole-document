# DOC (legacy Word 97–2003)

Legacy Word (`.doc`, MS-DOC) is a **CFBF-based legacy semantic model** built on
the shared compound-document physical layer (Phase 21.35): the exact leaf is the
**whole source** (a RAW-like `DocumentExact`) and every derived observation is a
bounded, span-preserving (`Q_gen`) projection. The adapter is gated behind the
**non-default** `doc = ["cfbf"]` feature (it reuses the `cfbf` physical layer).

The model is deliberately **partial and honest**: it claims a bounded Word 97–2003
FIB/piece-table subset, declines everything else **typed**, and never guesses and
never panics.

## Authority boundary

The exact physical source is the whole compound file — the 512-byte header,
DIFAT/FAT/MiniFAT, the directory tree, and every stream resolved to an exact byte
span. No un-OLE/re-OLE is performed: exactness is the whole source, and the DOC
model is derived (`Q_gen`), never on the exactness path (ADR-0060: the model node
depends on the `sha256(source)` root).

## Detection boundary

The DOC detector is built **on top of** the CFBF detector and runs **after** the
generic CFBF physical layer and **before** the generic `Cfbf` claim. So:

- a `.doc` whose `WordDocument` stream opens with the Word FIB magic `0xA5EC` and
  a plausible Word 97–2003 `nFib`, with the selected table stream present, becomes
  `Doc`;
- a generic compound file, a `.doc` with a bad `wIdent`, and a `.doc` with an
  implausible `nFib` (Word 6/95) stay `Cfbf` (a msg/xls/ppt/msg detector also runs
  in this chain; see [Detection order](../reference/format-support.md));
- prose stays `Opaque`;
- **`.docx` is an OPC ZIP** and never reaches the CFBF detector at all — it is
  detected as [DOCX](docx.md) in the ZIP family.

## Native inverse representation

A bounded MS-DOC subset:

- the Word FIB: `wIdent`, `nFib`/version, the `fWhichTblStm` table selection,
  `fcMin`/`fcMac`, `cbMac`, the `ccp*` section counts, `fcClx`/`lcbClx`;
- the piece table (the `Prc` records and the `Pcdt` `PlcPcd`: the CP array and the
  8-byte PCDs), each piece's resolved `fc`, compressed flag, and exact
  `WordDocument`-stream byte span, and its decoded text — a mix of a compressed
  (CP1252) and an uncompressed (UTF-16LE) piece is exercised in both a `0Table`
  and a `1Table` document;
- the main document text over `[0, ccpText)`.

## Sectioned surface (Phase 27.1)

The single contiguous CP space of the piece table is partitioned by the cumulative
FIB `ccp*` counts into seven sections, and each is exposed as its own
span-preserving (`Q_gen`) answer via `--doc-section <name>`:

`main` (`ccpText`), `footnote` (`ccpFtn`), `header` (`ccpHdd`), `annotation`
(`ccpAtn`), `endnote` (`ccpEdn`), `textbox` (`ccpTxbx`), and `hdrtextbox`
(`ccpHdrTxbx`).

Each answer carries the section name and its FIB `ccp*` field, the section's CP
range, the ordered pieces clipped to it (piece index, clipped CP range, the exact
`WordDocument`-stream byte span, the compressed flag, and the decoded text), and
the section's concatenated decoded text. A piece that **crosses** a section
boundary appears clipped in every section it overlaps, so the clipping is
observable. A section whose `ccp*` count is zero is a typed **empty** answer
(exit 0, `"empty": true`, no pieces, empty text) — never an error. The existing
`doc-fib`/`doc-piece`/`doc-text`/`doc-find` answers and the main text over
`[0, ccpText)` are unchanged.

## Supported observations

Common selectors: `metadata`, `text`, `search-match`. Native: `doc-fib`,
`doc-piece`, `doc-text`, `doc-section` (`main`/`footnote`/`header`/`annotation`/
`endnote`/`textbox`/`hdrtextbox`), `doc-find`.

## Declined (typed, never guessed)

A bad `wIdent`, an implausible `nFib`, a missing selected table stream, an
out-of-range `fcClx`/`lcbClx` or `fcMin`/`fcMac`, a CLX past the table stream, an
absent `Pcdt` or an `lcb` that is not `12n+4`, a CP array that is not monotonic
(or does not start at 0), a PCD `fc` whose piece falls outside the
`WordDocument` stream, and a piece-table last CP that disagrees with the declared
`ccp*` totals are typed declines (`InvalidDocStructure`, exit 52).

An unknown `--doc-section` name is a usage error (exit 2). A `.doc` with a
plausible FIB but a malformed body **is admitted** (its whole-source exact leaf
is unaffected) and its model — and therefore every section selector — declines
typed.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted `.doc`,
including those the adapter declines to interpret natively, and after the source
**and** descriptor are deleted in a fresh process.

**Adapter-court H1 (byte-exactness):** **3/3** normal fixtures byte-exact
(`doc_0table`, `doc_1table`, each with both a compressed and an uncompressed
piece, and `doc_sections`, whose pieces cross section boundaries); 4/4 structural
declines `InvalidDocStructure`; 3/3 CFBF controls stay `Cfbf`; 1/1 opaque control
stays `Opaque`. The Phase 27.1.1 court also answers all **7/7** CP sections, with
10/10 boundary-crossing pieces clipped exactly and 1/1 empty section reported as
a typed empty answer.

## Honest negatives

- **Economic court (Phase 27.4).** A separate economic court now exists: a
  source-retaining SQLite lane (**Lane B**, byte authority) is the single
  substantive comparator, and the conventional decode-to-host-values lane
  (**Lane C**) is `comparator_absent` — there is no faithful pure-Python/stdlib
  decoder for a legacy `.doc`, so every Lane-C question is a recorded typed
  decline rather than a fabricated answer. VOLE exactness (Q6) is **7/7**
  byte-exact. VOLE builds cheaper than the byte store (paired median **0.793×**)
  and is far cheaper cold/warm, but carries **~1.44×** the storage of a bytes-only
  store (the descriptor/DAG/index cost); cold is Python-startup-dominated and the
  CIs are degenerate at the small fixture counts. Only Q6 is a byte-authority
  claim.
- **Not claimed (out of the sectioned subset):** the header/footer **story →
  section** mapping (which header belongs to which section) and the
  textbox/header-textbox **anchors** are outside this subset; the section text is
  decoded CP1252/UTF-16 text and the exact bytes remain available through the
  piece spans (and the whole source through the exact leaf). `doc-find` is
  **document-level**, not piece-local or section-scoped. Word 6/95 (an older
  FIB), Word 2007+ `.docx` (an OPC ZIP), and the other legacy CFBF semantic models
  (see [XLS](xls.md), [PPT](ppt.md), [MSG](msg.md)) are also not claimed.
- **Real-world declines are expected.** In the stratified v2 population two
  genuine Apache POI `.doc` compound files stay at the generic `Cfbf` layer — a
  **recorded** boundary, not a failure of byte authority: `poi-document-Bug60942b.doc`
  is a **Word 6.0** FIB (`wIdent` `0xA5DC`, `nFib` `0x0065`, outside the supported
  Word 97–2003 `nFib` subset), and `poi-document-testRangeDelete.doc` has a genuine
  Word FIB but its `WordDocument` mini-stream — and the root mini stream that
  indexes it — is **fragmented**, so the first chunk's absolute offset is not
  recoverable and the header-prefix read conservatively declines.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 26.1.1 legacy Word adapter court (H1 exactness 2/2):
  [2026-10-11-phase26-1-1-doc-966c3fd4](../../evidence/campaigns/2026-10-11-phase26-1-1-doc-966c3fd4/).
- Phase 27.1.1 legacy Word **section** court (H1 exactness 3/3; 7/7 sections):
  [2026-10-12-phase27-1-1-doc-section-fc5cce39](../../evidence/campaigns/2026-10-12-phase27-1-1-doc-section-fc5cce39/).
- Phase 27.4 legacy Word **economic** court (Lane B source-retaining SQLite, Lane
  C `comparator_absent`; exactness 7/7):
  [2026-10-12-phase27-4-doc-econ-58f7572f](../../evidence/campaigns/2026-10-12-phase27-4-doc-econ-58f7572f/).
- Stratified real-world population v2 (Phase 26.5; re-run under 27.3/27.5):
  [2026-10-11-realformats-stratified-v2-703ca5ac](../../evidence/campaigns/2026-10-11-realformats-stratified-v2-703ca5ac/).
- Results: [phase-26-results.md](../phases/phase-26-results.md),
  [phase-27-results.md](../phases/phase-27-results.md).
