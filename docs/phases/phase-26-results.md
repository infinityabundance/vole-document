# Phase 26 results — legacy Office semantics, real-world scale, and the Arrow comparator

Branch `phase26`. Phase 26 closes the three items the `v0.1.0-alpha.35` release
recorded as *not done*, on the same rule as every other subphase: exactness is the
invariant, negatives are recorded, no claim without evidence. The frozen contract
is [phase-26-plan.md](phase-26-plan.md).

```text
26.1  Legacy DOC  (Word 97-2003)          — CFBF + the Word FIB / piece table / text
26.2  Legacy XLS  (BIFF8)                 — CFBF + the BIFF8 workbook record stream
26.3  Legacy PPT  (PowerPoint 97-2003)    — CFBF + the PowerPoint record stream
26.4  Legacy MSG  (Outlook)               — CFBF + the MAPI property streams
26.5  Stratified real-world population v2 + detection reconciliation
26.6  pyarrow lane for the Arrow economic comparator
```

All six subphases are **shipped**. The four legacy semantic models reuse the
Phase-21.35 CFBF/OLE2 physical layer; the population is a genuinely larger
stratified court; the Arrow comparator now measures Arrow with a real Arrow
reader.

## 26.1–26.4 — the four legacy semantic models

Each is a **CFBF-based legacy semantic model**: the exact leaf is the **whole
source** (a RAW-like `DocumentExact`) and every derived observation is a bounded,
span-preserving (`Q_gen`) projection. The features are non-default and build on
`cfbf`: `doc = ["cfbf"]`, `xls = ["cfbf"]`, `ppt = ["cfbf"]`, `msg = ["cfbf"]`.
The detectors are built **on top of** the CFBF detector and run **after** the
generic CFBF physical layer and **before** the generic `Cfbf` claim — in the fixed
order **DOC → XLS → PPT → MSG → Cfbf** — so a recognized legacy file becomes its
own format while a generic compound file stays `Cfbf`. A `.docx`/`.xlsx`/`.pptx`
is an OPC ZIP and never reaches the CFBF detector at all.

**Adapter-court H1 (byte-exactness), after source *and* descriptor deletion in a
fresh process:**

| Subphase | Format | Feature | H1 exactness | Structural declines | CFBF controls | Opaque control |
|---|---|---|---|---:|---:|---:|
| 26.1 | DOC (MS-DOC) | `doc = ["cfbf"]` | **2/2** | 4/4 | 3/3 | 1/1 |
| 26.2 | XLS (BIFF8) | `xls = ["cfbf"]` | **1/1** | 4/4 | 3/3 | 1/1 |
| 26.3 | PPT (MS-PPT) | `ppt = ["cfbf"]` | **1/1** | 5/5 | 2/2 | 1/1 |
| 26.4 | MSG (MS-OXMSG) | `msg = ["cfbf"]` | **1/1** | 8/8 | 2/2 | 1/1 |

Typed structural declines, one class per format: `InvalidDocStructure` (exit 52),
`InvalidXlsStructure` (53), `InvalidPptStructure` (54), `InvalidMsgStructure` (55).
A plausible-header file with a malformed body is admitted (its whole-source exact
leaf is unaffected) and its derived model then declines typed; nothing ever panics.

Per-format supported/declined subsets and detection boundaries are on
[DOC](../formats/doc.md), [XLS](../formats/xls.md), [PPT](../formats/ppt.md) and
[MSG](../formats/msg.md).

## Scope note — no separate economic court for the four legacy formats

The four legacy formats do **not** have a separate economic court. There is **no
faithful conventional decoder** for legacy binary Office or MS-OXMSG, so an
economic comparison would be the same `comparator_absent` situation recorded for
HDF5 and CFBF: the conventional lane would decline every question typed rather
than answer. Their validation is therefore the **adapter courts** (H1 exactness +
structural declines + controls) plus the **stratified real-world court** (26.5).
This is stated plainly rather than implying an economic comparison that was not
run.

## 26.5 — stratified real-world population v2

**Verdict `PASS`:** 284 real samples + 24 hostile = **308 samples across 41
formats**, every one **byte-exact** (308/308 exact, 0 skipped, 0 blocked), with the
detector choice recorded for each. Samples are **stratified** by format, size
(small/medium/large), complexity (simple/moderate/structured/nested/complex) and
hostility, and each is **pinned** by byte length + SHA-256 + license +
`redistributable` flag in [`tools/realcorpus/formats-manifest-v2.tsv`](../../tools/realcorpus/formats-manifest-v2.tsv)
(non-redistributable bytes stay gitignored and are regenerated; the network is
never trusted — a mismatch fails the run). The hostile stratum is derived
deterministically from pinned real bytes and pinned in
[`tools/realcorpus/hostile-strata-v2.tsv`](../../tools/realcorpus/hostile-strata-v2.tsv).
The `narrow` and `full` costs are reported **pooled and median** (ADR-0054: a claim
names its estimator). The 60-sample v1 court
([`2026-10-10-realformats-stratified-3529688e`](../../evidence/campaigns/2026-10-10-realformats-stratified-3529688e/))
remains the historical control.

**Detection findings — 32 in total**, every one **reconciled**
([`findings-v2-reconciliation.tsv`](../../tools/realcorpus/findings-v2-reconciliation.tsv)):
14 `decline-opaque`, 18 `misclassified`. Two are **genuine adapter bugs, fixed
with focused regression tests**:

1. **RTF trailing padding.** Real writers append a final CRLF (Word a NUL) after
   the balanced root group; the detector now tolerates trailing NUL/ASCII-whitespace
   padding — regression test
   `rtf::tests::trailing_padding_parses_and_keeps_the_root_group_span`.
2. **EML RFC 2231 extended boundary.** A `boundary*=charset'lang'value` parameter
   is now decoded, so real `multipart/signed` mail detects — regression test
   `eml::tests::rfc2231_extended_boundary_is_decoded`.

The other **30 are recorded typed declines / detector-precedence
misclassifications** with reasons (e.g. the reST detector legitimately claims some
attribute-only AsciiDoc; the config/CSV detectors claim some log streams; a bare
Arrow IPC *stream* and some older GPX/KML namespaces decline to `Opaque`/`Xml`;
several real `.doc`/`.xls`/`.ppt` compound files stay at the generic `Cfbf` layer
because their variant is outside the bounded legacy model). A *recorded* finding
never changes the exact bytes. All 32 are byte-exact; the byte-authority gate
(`materialize == source`) is unaffected by any finding.

**Coverage gaps recorded, not fabricated.** `msgpack` and `fixedwidth` have **no
real sample** in this population (they remain adapter-court-only), and the
legacy-DOC/XLS/PPT detectors declined some genuine real files to the `Cfbf` layer
(recorded); the MSG detector detected all six real `.msg` samples.

## 26.6 — the pyarrow lane and the Arrow-conformance correction

**Root cause.** The earlier Arrow economic court's comparator read a **Parquet
projection** because the pinned DuckDB wheel exposes only the Arrow C data
interface, not an Arrow IPC file reader — so **no real Arrow reader was ever in the
loop** and the hand-crafted corpus was never validated against one. Adding the
hash-pinned **`pyarrow 25.0.1`** lane (Dockerfile + `analytical` service) found the
corpus was **not conformant Arrow IPC**: (1) tag-only Flatbuffers `Field.type`
**union** values were omitted (a bare union tag, rejected by the reference reader
as an unexpected null), (2) the footer `recordBatches` vector was encoded as a
vector of *tables* rather than `Block` **structs**, and (3) `Block.metaDataLength`
omitted the 8-byte continuation + length prefix.

**Fix.** Both `tools/fixtures/make-arrow.py` and VOLE's footer `Block` reader in
`src/adapter/arrow.rs` were corrected to the Arrow wire format and the committed
fixtures under `tools/fixtures/arrow/` regenerated (their SHA-256 values changed).
`pyarrow` now accepts the file-format output, and the Arrow adapter suite
(`tests/arrow_adapter.rs`, 13 tests, **13/13**) exercises the conformant fixtures
plus the recorded typed declines. The two prior campaigns carry explicit
[CORRECTION.md](../../evidence/campaigns/2026-10-10-phase21-16-arrow-econ-4fdc7ad0/CORRECTION.md)
amendments; the re-sealed court is
[`2026-10-11-phase21-16-arrow-econ-74965426`](../../evidence/campaigns/2026-10-11-phase21-16-arrow-econ-74965426/).

**Results (equal contract, VOLE / comparator paired per-fixture ratios; median;
< 1 favours VOLE; ADR-0054 estimator).** VOLE exactness **3/3**. Where pyarrow
answers, **0 mismatches** (Q1 column values, Q2 predicate count, Q4 batch count, Q5
lexical find: 3 equal / 0 gap / 0 mismatch each).

| Axis | VOLE / pyarrow (median) | Who wins |
|---|---:|---|
| build | 0.512 | **VOLE** (~2× faster) |
| storage | 2.156 | **pyarrow** (the Arrow file is ≈ **0.46×** VOLE's store) |
| cold | 0.024 | **VOLE** (inflated by the comparator's fresh-process Python start-up) |
| warm | 7.295 | **pyarrow** decisively (geomean 9.452) |

- **pyarrow wins warm decisively** (median **7.295×**, geomean **9.452×**) — the
  reference Arrow reader opens the `.arrow` file directly and computes in native
  columnar code, while VOLE materializes a bounded observation and counts in the
  driver.
- **VOLE wins build** (median **0.512×**) and **cold** (median **0.024×**, but read
  with care: VOLE's cold is a single in-process observation while the comparator's
  includes a fresh-process Python + `pyarrow` import).
- **pyarrow wins storage** — VOLE's store is **2.156×** the Arrow file (ratio of
  sums 2.007); equivalently the Arrow file is **≈ 0.46×** VOLE's store.
- **pyarrow declines exact spans** (Q3: not retainable — no exact source buffer
  span in the Arrow IPC metadata) and **byte authority** (Q6: `not-native` — pyarrow
  is not a source-retaining store). Only Q6 is a byte-authority claim, and only VOLE
  and the source-retaining SQLite lane can make it. Where pyarrow answers, **0
  mismatches**.
- Against DuckDB (still on a Parquet projection) the warm contrast is ~**1.224×**
  (unresolved); the DuckDB lane is retained as the projection comparator and the
  pyarrow lane as the true-Arrow comparator.

**Stream-fixture caveat.** VOLE's Arrow detector requires the `ARROW1` magic: it
admits the IPC **file** format (magic + a consistent footer) and **magic-prefixed**
streams only. A bare Arrow IPC **stream** (no leading `ARROW1`) is a typed
`Opaque` decline. In the 26.5 population the four real `*.stream` fixtures
(`arrow-stream-*`) are declined to `Opaque` and recorded — a conservative
boundary, not a byte-authority failure.

## Cross-cutting

- Format pages: [DOC](../formats/doc.md), [XLS](../formats/xls.md),
  [PPT](../formats/ppt.md), [MSG](../formats/msg.md).
- Capability matrix and detection order:
  [format-support.md](../reference/format-support.md).
- Receipts: [26.1 DOC](../../evidence/campaigns/2026-10-11-phase26-1-1-doc-966c3fd4/),
  [26.2 XLS](../../evidence/campaigns/2026-10-11-phase26-2-1-xls-966c3fd4/),
  [26.3 PPT](../../evidence/campaigns/2026-10-11-phase26-3-1-ppt-966c3fd4/),
  [26.4 MSG](../../evidence/campaigns/2026-10-11-phase26-4-1-msg-966c3fd4/),
  [26.5 stratified v2](../../evidence/campaigns/2026-10-11-realformats-stratified-v2-703ca5ac/),
  [26.6 Arrow econ + pyarrow](../../evidence/campaigns/2026-10-11-phase21-16-arrow-econ-74965426/).

## What is not claimed

- The legacy models are **bounded**: a construct outside the subset is a typed
  decline, never a guess. Real Word/Excel/PowerPoint files whose variant is outside
  the subset stay at the generic `Cfbf` layer (recorded).
- The 284-sample population is **stratified, not a random sample of the world**.
- **No economic comparison was run** for the four legacy formats (no faithful
  conventional decoder exists).
- A recorded detector decline/misclassification **never** changes the exact bytes.
