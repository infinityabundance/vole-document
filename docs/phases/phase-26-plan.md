# Phase 26 — Legacy Office semantics, real-world scale, and the Arrow comparator

> **Status:** planned (frozen contract below). Branch `phase26`.
> Phase 21 completed the format surface (Wave 1 office + the full Wave 2 breadth),
> including the **CFBF/OLE2 physical layer** (21.35) that this phase builds the
> legacy **semantic** models on. Phase 26 closes the three items the Phase-21
> release recorded as *not done*, and it is judged by the same rule as every other
> subphase: exactness is the invariant, negatives are recorded, no claim without
> evidence.

## Why

Three honest gaps remain after `v0.1.0-alpha.35`:

1. **Legacy Office / Outlook semantics.** The CFBF/OLE2 layer (21.35) exposes the
   compound-document tree (streams with exact spans) but deliberately ships **no
   semantic model**. The most common legacy document formats — Word 97–2003
   (`.doc`), Excel 97–2003 (`.xls`), PowerPoint 97–2003 (`.ppt`), and Outlook
   message (`.msg`) — are all CFBF containers, so one shared physical layer should
   yield four semantic models, exactly as one OPC layer yielded DOCX/XLSX/PPTX and
   one ODF layer yielded ODT/ODS/ODP.
2. **Real-world scale.** The stratified real-world court is a **60-sample smoke**
   (52 real + 8 hostile, 14 formats). It is a population-lite, not a stratified
   population, and it left detection findings to reconcile.
3. **The Arrow comparator.** The Arrow economic court used a Parquet projection
   because the pinned DuckDB wheel lacks an Arrow IPC reader. A `pyarrow`-based
   lane would measure Arrow on its own terms.

## The order (frozen)

```text
26.1  Legacy DOC  (Word 97-2003)   — CFBF + the Word FIB / piece table / text
26.2  Legacy XLS  (BIFF8)          — CFBF + the BIFF8 workbook record stream
26.3  Legacy PPT  (PowerPoint 97-2003) — CFBF + the PowerPoint record stream
26.4  Legacy MSG  (Outlook)        — CFBF + the MAPI property streams
26.5  Stratified real-world population (larger scale) + detection reconciliation
26.6  pyarrow lane for the Arrow economic comparator
```

Reasons for the order, briefly: the legacy semantic models come first because
they extend the freshest shared substrate (CFBF) and complete the format
surface; the stratified population follows so it exercises the **whole** format
set including the new legacy models; the pyarrow lane is last because it is a
measurement-infrastructure fix, not a capability.

## The reuse architecture

The CFBF layer (21.35) already owns the physical services: the header, DIFAT/FAT/
MiniFAT, the directory tree with `parent`/`depth`, and every stream resolved to an
**exact byte span** (FAT-chained regular streams and MiniFAT-chained mini
streams). Each legacy adapter **reuses that layer** and adds only a semantic
model over a named stream (or set of streams):

```text
                 CFBF / OLE2 physical layer (21.35)
                              │
        ┌───────────┬─────────┼─────────┬──────────┐
       DOC         XLS       PPT       MSG
        │           │         │         │
   Word FIB +   BIFF8      PowerPoint  MAPI property
   piece table  records    records     streams
```

The exact leaf remains the **whole source** (the CFBF file is not a package with
an inner byte-authority the way an OPC part is; the physical-layer streams carry
exact spans, and the semantic answers are `Q_gen` projections). No un-OLE/re-OLE.

## Acceptance (per subphase, unchanged)

Exactness is the invariant (`materialize == source`, length + SHA-256 + `cmp`,
after source **and** descriptor deletion, in a fresh process); Docker only,
digest-pinned and memory-capped; one branch (`phase26`) with commit-and-push per
subphase; a sealed receipt per court; negatives and partials recorded; no cap
raised to make a run pass; no claim without evidence. A legacy semantic model is
a **bounded** subset: every construct outside it must **decline typed**, never
guess and never panic, and the receipt must state exactly what is supported vs
declined.

## 26.5 — the stratified population, in detail

The court must be a **population**, not a smoke: samples **stratified** by
format, by **size** (small / medium / large), by **complexity** (simple /
structured / nested), and by **hostility** (valid / malformed / truncated /
magic-collision). Each sample is **pinned** in the manifest by byte length and
SHA-256 (the network is never trusted — a mismatch fails the run), with its
source and license recorded and a `redistributable` flag; non-redistributable
bytes stay gitignored and are regenerated, and large evidence is length+SHA-256+
bounded-prefix.

For every sample the court asserts byte-exact closure and records what the
detector chose; **any** detection finding is reconciled in the same subphase
(either fixed in the adapter with a regression test, or recorded as a typed
decline with the reason). The 60-sample court is kept as a historical control.

## 26.6 — the pyarrow lane, in detail

Add a pinned service (or extend an existing one) carrying `pyarrow`, and a third
comparator lane for the Arrow economic court that reads the Arrow IPC file with
`pyarrow` directly (projection + predicate pushdown where the format supports
it), replacing the Parquet-projection surrogate. The lane must be compared on
equal contract: the same questions, the same exactness gate, the same ADR-0054
estimator. If `pyarrow` cannot be pinned reproducibly, record the negative and
keep the Parquet surrogate, stated plainly.
