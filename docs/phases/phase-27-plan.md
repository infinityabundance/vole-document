# Phase 27 — Legacy depth, detection hardening, legacy economics, Arrow streams

> **Status:** planned (frozen contract below). Branch `phase27`.
> Phase 26 delivered the legacy Office/MSG **semantic models**, the stratified
> real-world **population v2**, and the **pyarrow** Arrow lane — and, in doing so,
> recorded four honest limitations. Phase 27 closes them. Same rule as every
> subphase: exactness is the invariant, negatives are recorded, no claim without
> evidence.

## Why

Phase 26's own report named four gaps:

1. **Legacy depth.** The legacy models are bounded subsets: Word **footnote /
   endnote / header / footer / textbox** text and PowerPoint **master / notes**
   text lists are recorded (spans, counts) but **not materialized as separate
   answers**.
2. **Detection findings.** The stratified real-world court v2 reconciled **32
   findings**: 2 were fixed, **30 were recorded** as typed declines or
   detector-precedence misclassifications. Several of those misclassifications
   are worth attacking where the precedence is genuinely wrong (not merely
   conservative).
3. **Legacy economics.** The legacy formats have **no separate economic court**
   (no faithful conventional decoder exists — the same `comparator_absent`
   situation as HDF5/CFBF).
4. **Arrow streams.** The Arrow **stream** fixture carried a non-spec `ARROW1`
   prefix (a VOLE detection convention), so real magic-less Arrow IPC streams
   would stay `Opaque`.

## The order (frozen)

```text
27.1  Legacy DOC depth   — footnote/endnote/header/footer/textbox text as typed answers
27.2  Legacy PPT depth   — master/notes SlideListWithText text lists
27.3  Detection hardening — attack the 30 recorded stratified-v2 findings
27.4  Legacy economic court — doc/xls/ppt/msg vs a source-retaining baseline
27.5  Arrow IPC stream detection — magic-less streaming framing + a conformant stream fixture
```

Reasons for the order: the legacy-depth work extends the freshest models; the
detection hardening re-runs the real-world court that produced the findings; the
legacy economic court then measures the (now deeper) legacy models on equal
contract; the Arrow stream fix is a self-contained detection/corpus correction.

## Acceptance (per subphase, unchanged)

Exactness is the invariant (`materialize == source`, length + SHA-256 + `cmp`,
after source **and** descriptor deletion, in a fresh process); Docker only,
digest-pinned and memory-capped; one branch (`phase27`) with commit-and-push per
subphase; a sealed receipt per court; negatives and partials recorded; no cap
raised to make a run pass; no claim without evidence. Every new observation of a
legacy or stream construct must be a **bounded, span-preserving `Q_gen`
projection**; anything outside the extended subset still declines **typed**.

## 27.3 — the detection hardening, in detail

The court records, for every sample, the **detector's choice** and the **declared
format**. A `misclassified` finding is attacked in this order:
1. If the precedence is **wrong** (a stronger, more-specific signal exists), fix
   the detection order or the semantic sub-test, add a **regression test** on the
   real sample, and re-run the court.
2. If the input is genuinely ambiguous or below a deliberate conservatism
   threshold, keep the decline/choice and record it **as-is** with the reason.
The outcome of each of the 30 findings must be one of: **fixed** (with test) or
**recorded** (unchanged, with reason). The stratified-v2 court is re-run and
re-sealed, and the v1 court stays the historical control.

## 27.5 — Arrow streams, in detail

Arrow IPC has two framings: the **file** format (a leading `ARROW1` magic) and
the **stream** format (a bare `Schema` message followed by `RecordBatch`/`DictionaryBatch`
messages and an end-of-stream marker, **with no magic**). Detection must accept a
**stream** framing as well — a valid `Message` header + metadata-length sequence
that parses as a bounded Arrow schema/batch stream — without claiming arbitrary
bytes. The corpus gains a **spec-conformant** (magic-less) stream fixture, and the
court asserts byte-exact closure on it. If a safe magic-less detector cannot be
made conservative enough, the negative is recorded and the file-only detection
stands, stated plainly.
