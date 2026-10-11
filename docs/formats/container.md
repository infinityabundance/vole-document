# archive containers (ZIP / TAR)

A standalone **ZIP** or **TAR** archive is the **generic archive-container**
format of Phase 21 Wave 2 (subphase 21.33). It reuses the byte-authoritative ZIP
physical layer (ADR-0030) for ZIP and adds a bounded TAR parser. Like the other
Wave-2 formats it is **not a package at the exactness layer**: the exact leaf is
the whole source (a RAW-like `DocumentExact`; for ZIP the reused `PackageRoot`),
and every derived observation is a bounded, span-preserving (`Q_gen`) projection.
The adapter is gated behind the **non-default** `container = ["package"]` feature
(it needs the ZIP cover, so it implies `package`; the TAR half is
dependency-free).

## Authority boundary

Detection is a **strong-magic + validation** test, inserted **after the entire
office ZIP family** (so an office package is never reclassified as a plain
archive) and **before every weak, no-magic-byte heuristic**. ZIP is claimed on
`PK\x03\x04` at offset 0, or the empty-archive `PK\x05\x06` with a valid EOCD and
a coherent central directory. TAR is claimed on a valid `ustar`/old-GNU header at
offset 257, or a plausible v7 header at offset 0 whose checksum matches. The
**recorded dialect** (`zip` / `tar`) lives in the model.

**What it cannot distinguish / what declines.** A `PK`-prefixed blob without a
valid EOCD, a 512-byte-junk blob whose checksum fails, and prose all **stay
`Opaque`**. A structurally truncated TAR (a valid first header, then a truncated
member) **detects as `container`** and its model declines typed
(`InvalidContainerStructure`, exit 49). Malformed EOCD, truncated ZIP, and a TAR
with a bad header checksum are typed declines; a bad/short input stays `Opaque`
(exit 6).

## Representation preservation (the point of the format)

Every member's **raw name bytes** (including non-UTF-8 names and the ZIP UTF-8
flag bit 11), method (stored/deflate), both declared sizes, CRC-32, and exact
member span are recorded. TAR records the typeflag/mode/linkname with the GNU
long-name (`L`) and long-link (`K`) and PAX (`x`/`g`) extensions folded in.
Nothing is unzipped, re-zipped, decompressed, or re-serialized.

## Supported observations

Common selectors: `metadata`, `text`, `search-match`. Native:
`container-dialect`, `container-entry`, `container-find`.

## Unsupported / honest cost

An out-of-range member index declines typed (exit 6); a container selector on an
office package or an `Opaque` control declines typed (exit 6). Office-package
semantics are separate adapters (DOCX/EPUB/ODT/ODS/ODP/XLSX/PPTX) and are not
claimed here.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted ZIP
or TAR, and after the source **and** descriptor are deleted in a fresh process.
Exactness is the whole source: for TAR a `DocumentExact`, for ZIP the reused
`PackageRoot`. The derived `ContainerModel` is never on the exactness path
(ADR-0060: the model node depends on the exact source root, so no source-reading
node aliases another field's source).

**Adapter-court H1:** **6/6 byte-exact** (4 ZIP fixtures — `zip_stored`,
`zip_multi`, `zip_nonutf8`, `zip_empty` — + 2 TAR fixtures `tar_ustar`,
`tar_extended`), with 2 office-format controls (DOCX/ODT stay `docx`/`odt`), 5
opaque controls, 5 native-entry checks, 1 native-entry decline, and 1 structural
(TAR) decline.

## Economic court

Measured by `tools/phase21-33-container-econ-court.sh` (a thin wrapper over the
shared `textfmt-court.sh` engine) against two comparators — a **source-retaining
SQLite store** with conventional extraction, and a conventional
**decode-to-host-values load** (`conv`). Corpus 5 fixtures, questions Q1–Q12,
exactness **12/12**. Estimator = paired per-fixture ratio VOLE/comparator, median
+ geometric mean with a fixed-seed (2135), fixture-clustered 10000-resample 95 %
CI, tie band ±10 %; ratio-of-sums reported separately. A ratio < 1 favours VOLE.

| metric | comparator | median | geomean | median 95% CI | geomean 95% CI | W/T/L | ratio-of-sums |
|---|---|---:|---:|---|---|---:|---:|
| build | sqlite | 0.706 | 0.656 | 0.563..0.724 | 0.596..0.716 | 5/0/0 | 0.651 |
| build | conv | 0.887 | 0.826 | 0.680..0.952 | 0.733..0.920 | 3/2/0 | 0.818 |
| storage | sqlite | 0.980 | 1.050 | 0.837..1.438 | 0.864..1.276 | 2/1/2 | 1.097 |
| storage | conv | 24.449 | 24.342 | 20.861..26.944 | 22.405..25.917 | 0/0/5 | 24.625 |
| cold | sqlite | 0.025 | 0.025 | 0.023..0.026 | 0.024..0.025 | 5/0/0 | 0.025 |
| cold | conv | 0.025 | 0.026 | 0.025..0.028 | 0.025..0.027 | 5/0/0 | 0.026 |
| warm | sqlite | 0.098 | 0.098 | 0.092..0.103 | 0.094..0.101 | 5/0/0 | 0.098 |
| warm | conv | 1.246 | 1.296 | 1.169..1.523 | 1.196..1.422 | 0/0/5 | 1.308 |

Build is a win on both lanes; **storage-vs-SQLite is a recorded mixed loss**
(2/1/2, median 0.980, ratio-of-sums 1.097 — the descriptor + DAG + index is
larger here) and storage-vs-`conv` is a large recorded loss; cold and warm are
wins on both lanes.

## Honest negatives

* **Storage-vs-SQLite is a recorded loss/mixed** (ratio-of-sums 1.097), and
  storage-vs-`conv` is a large recorded loss (the conventional lane stores even
  less).
* A **structurally truncated TAR** is detected as `container` and its model
  declines typed; malformed EOCD / bad-checksum TAR stay `Opaque`.
* Only exact closure (Q6) is a byte-authority claim; the **conventional load is
  deliberately the weaker comparator**.
* The **cold ratios** are dominated by the comparators' fresh-process **Python
  start-up**.
* The economic court is a **self-authored deterministic corpus** (by
  `tools/fixtures/make-container.py` and `examples/container_fixtures.rs`), not a
  real-world archive population.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0030](../adr/0030-zip-physical-layer.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.33.1 archive-container adapter court (exactness 6/6):
  [2026-10-11-phase21-33-1-container-7362e74b](../../evidence/campaigns/2026-10-11-phase21-33-1-container-7362e74b/).
- Phase 21.33 archive-container economic court (exactness 12/12):
  [2026-10-11-phase21-33-container-econ-372a86e0](../../evidence/campaigns/2026-10-11-phase21-33-container-econ-372a86e0/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
