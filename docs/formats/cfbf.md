# CFBF / OLE2 compound files

A CFBF / OLE2 compound file is the **compound-document physical layer** of Phase
21 Wave 2 (subphase 21.35) — the container behind legacy Word (`.doc`), Excel
(`.xls`), PowerPoint (`.ppt`), and Outlook (`.msg`) files. It is **not a
package** (no OPC/ZIP layer): the exact leaf is the whole source (a RAW-like
`DocumentExact`) and every derived observation is a bounded, span-preserving
(`Q_gen`) projection. The adapter is gated behind the **non-default,
dependency-free** `cfbf = []` feature. The **recorded dialect**
(`cfb3` / `cfb4`) lives in the model.

## Authority boundary — the physical layer only

The adapter parses the **physical layer**: the 512-byte header (signature,
version, byte order, sector/mini-sector shifts, the FAT/DIFAT/directory/MiniFAT
counts and first sectors), the DIFAT and FAT, the 128-byte directory entries with
verbatim UTF-16LE names (incl. non-ASCII) and the reconstructed red-black
sibling/child tree, and each stream resolved to an **exact byte span** (regular
streams through the FAT, small streams through the MiniFAT inside the root mini
stream). It deliberately stops at the physical layer: **the legacy DOC/XLS/PPT/
MSG *semantic* models are later subphases**, not claimed here. Nothing is
un-OLE'd or re-OLE'd.

Detection is a **strong-magic + header-validation** test: the 8-byte signature
`D0 CF 11 E0 A1 B1 1A E1` at offset 0 **and** an internally consistent header
(byte order `FFFE`, a major version of 3 or 4, a sector shift of 9 or 12
consistent with the version, a mini-sector shift of 6, at least one FAT sector,
and a first directory sector inside the file). It is inserted with the strong-
magic binaries and before every weak, no-magic-byte heuristic.

**What it cannot distinguish.** A **signature-only** blob (its header fields are
zero), a corrupted signature, a bad sector shift, an out-of-range directory
sector, and prose all **stay `Opaque`**. A blob with a **plausible header but a
malformed body is admitted** (its whole-source exact leaf is unaffected) and its
model then declines typed (`InvalidCfbfStructure`, exit 51) — an invalid
directory object type, a cyclic/overlapping FAT or MiniFAT chain, an out-of-range
sector ID, a truncated stream chain, or a header/sector-size mismatch.

**Span caveat.** A stream's `data_span` is its exact `(offset, length)` when its
sector chain is **contiguous** (the common case); a **non-contiguous chain
records a bounding hull and its `ExactBytes` declines typed** rather than
returning interleaved foreign bytes.

## Supported observations

Common selectors: `metadata`, `text`, `search-match`. Native: `cfbf-dialect`,
`cfbf-entry`, `cfbf-stream`, `cfbf-find`. A `cfbf` selector on an `Opaque`
control declines typed (exit 6).

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted
compound file, and after the source **and** descriptor are deleted in a fresh
process. Exactness is the whole source (a RAW-like `DocumentExact`): the
`CfbfModel` node is derived (`Q_gen`) and never on the exactness path (ADR-0060:
the model node depends on the `sha256(source)` root).

**Adapter-court H1:** **2/2 byte-exact** (`cfbf_v3`, `cfbf_v4`), with 2 native-
entry checks, 3 structural declines, and 5 opaque controls (signature-only,
corrupted signature, bad sector shift, out-of-range directory sector, prose).

## Economic court

Measured by `tools/phase21-35-cfbf-econ-court.sh` (a thin wrapper over the shared
`textfmt-court.sh` engine) against two comparators — a **source-retaining SQLite
store** with conventional extraction, and a conventional **decode-to-host-values
load** (`conv`). Corpus 2 fixtures, questions Q1–Q12, exactness **7/7**.
Estimator = paired per-fixture ratio VOLE/comparator, median + geometric mean with
a fixed-seed (2137), fixture-clustered 10000-resample 95 % CI, tie band ±10 %;
ratio-of-sums reported separately. A ratio < 1 favours VOLE.

| metric | comparator | median | geomean | median 95% CI | geomean 95% CI | W/T/L | ratio-of-sums |
|---|---|---:|---:|---|---|---:|---:|
| build | sqlite | 0.643 | 0.627 | 0.498..0.788 | 0.498..0.788 | 2/0/0 | 0.600 |
| build | conv | 0.815 | 0.796 | 0.640..0.991 | 0.640..0.991 | 1/1/0 | 0.764 |
| storage | sqlite | 1.481 | 1.467 | 1.281..1.680 | 1.281..1.680 | 0/0/2 | 1.557 |
| storage | conv | 333.156 | 290.461 | 169.984..496.328 | 169.984..496.328 | 0/0/2 | 333.811 |
| cold | sqlite | 0.023 | 0.023 | 0.020..0.026 | 0.020..0.026 | 2/0/0 | 0.023 |
| cold | conv | 0.027 | 0.027 | 0.025..0.029 | 0.025..0.029 | 2/0/0 | 0.027 |
| warm | sqlite | 0.113 | 0.111 | 0.092..0.135 | 0.092..0.135 | 2/0/0 | 0.113 |
| warm | conv | 1.880 | 1.848 | 1.535..2.224 | 1.535..2.224 | 0/0/2 | 1.878 |

Build is a win on both lanes; **storage-vs-SQLite is a recorded loss** (median
1.481, ratio-of-sums 1.557) and storage-vs-`conv` is a very large recorded loss;
cold and warm are wins vs SQLite, and warm is a **recorded loss vs the
conventional in-process load**.

## Honest negatives

* **`comparator_absent` (recorded):** **CFBF/OLE2 has no faithful Python-stdlib
  decoder**, so the conventional `conv` lane declines **every question** as a
  typed `comparator_absent` decline — never a silent or fabricated answer — and
  VOLE capability is shown as a `capability-gap` against it.
* **This is the physical layer only.** The legacy **DOC/XLS/PPT/MSG semantic
  models are later work** on top of this layer and are not claimed here.
* **Storage-vs-SQLite is a recorded loss** (VOLE's descriptor + DAG + index is
  larger), and storage-vs-`conv` is a very large recorded loss.
* A **non-contiguous stream chain** declines `ExactBytes` (a bounding hull is
  recorded) rather than returning interleaved foreign bytes.
* A plausible header with a malformed body is admitted; its model declines typed
  (exit 51).
* Only exact closure (Q6) is a byte-authority claim; the **conventional load is
  deliberately the weaker comparator**.
* The **cold ratios** are dominated by the comparators' fresh-process **Python
  start-up**.
* The economic court is a **self-authored deterministic corpus** (by
  `tools/fixtures/make-cfbf.py` and `examples/cfbf_fixtures.rs`), not a real-world
  OLE population.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.35.1 CFBF/OLE2 adapter court (exactness 2/2):
  [2026-10-11-phase21-35-1-cfbf-42791bce](../../evidence/campaigns/2026-10-11-phase21-35-1-cfbf-42791bce/).
- Phase 21.35 CFBF/OLE2 economic court (exactness 7/7):
  [2026-10-11-phase21-35-cfbf-econ-372a86e0](../../evidence/campaigns/2026-10-11-phase21-35-cfbf-econ-372a86e0/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
