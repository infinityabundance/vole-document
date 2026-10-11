# scientific containers (NetCDF classic / HDF5 subset)

A scientific container — **NetCDF classic** (CDF-1/CDF-2) or **HDF5** — is the
**scientific-container** format of Phase 21 Wave 2 (subphase 21.34). It is **not a
package** (no OPC/ZIP layer): the exact leaf is the whole source (a RAW-like
`DocumentExact`) and every derived observation is a bounded, span-preserving
(`Q_gen`) projection. The adapter is gated behind the **non-default,
dependency-free** `hdf = []` feature. The **recorded dialect** (`netcdf` /
`hdf5`) lives in the model.

## Authority boundary

Detection is a **strong-magic-byte** test (never a file name). NetCDF classic
requires `CDF\x01`/`CDF\x02` at offset 0; HDF5 requires the 8-byte signature
`\x89HDF\r\n\x1a\n` at offset 0 **or** 512, with a superblock version in `0..=3`,
an offset/length size in `{2,4,8}`, and a superblock that fits inside the source.
It is inserted with the other strong-magic binaries, after the office ZIP family
and the generic archive container and before Parquet and every weak,
no-magic-byte heuristic.

**What it cannot distinguish.** HDF5 detection is a **signature +
superblock-header validation, not a full-file test**: a random
`\x89HDF`-prefixed blob with a plausible superblock header is admitted and its
model may then decline typed. NetCDF detection is the 4-byte magic only.

**Supported vs declined (the honest scope).** **NetCDF classic is supported in
full** — the magic, `numrecs` (incl. the `STREAMING` sentinel), the dimension
table (incl. the unlimited/record dimension), the global attribute list, the
variable list (name, dimensions, per-variable attributes, type, `vsize`, `begin`),
and the record/array data layout, with exact spans. **HDF5 is a documented
bounded subset** — superblock v0/v1, the symbol-table root-group link traversal
to contiguous datasets and nested groups, v1 object headers with their message
list, the fixed-point/float/string datatype classes, the simple/scalar dataspace,
the contiguous data layout, and v1 attributes. Everything outside the subset
**declines typed** (exit 6 for `unsupported-feature`): superblock v2/v3 (the v2
link machinery), v2/v3 object headers, compact/chunked/virtual layouts, filtered
datasets, external data files, and every other datatype class
(bitfield/opaque/compound/reference/enum/vlen/array); unknown object-header
message types also decline typed. A malformed NetCDF header and a bad NetCDF tag
decline **structurally** (`InvalidHdfStructure`, exit 50).

## Representation preservation (the point of the format)

The reused arena records the group/dataset tree with each dataset's datatype,
simple dataspace, and contiguous layout, the root-group attributes, and every
object's exact source span. **Dataset values are never decoded or converted**
(contiguous spans are recorded, not read); filtered data is never decompressed;
external/virtual storage is never resolved; NetCDF-4 (HDF5-backed) semantics are
not interpreted beyond the HDF5 subset.

## Supported observations

Common selectors: `metadata`, `text`, `search-match`. Native: `hdf-dialect`,
`hdf-object`, `hdf-dimension`, `hdf-attribute`, `hdf-find`.

## Unsupported / honest cost

An out-of-range native index and an `hdf` selector on an `Opaque` control decline
typed (exit 6). Dataset value decoding, filter decompression, and the HDF5 v2
link machinery are **not claimed**.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted
scientific container, and after the source **and** descriptor are deleted in a
fresh process. Exactness is the whole source (a RAW-like `DocumentExact`): the
`HdfModel` node is derived (`Q_gen`) and never on the exactness path (ADR-0060:
the model node depends on the `sha256(source)` root).

**Adapter-court H1:** **4/4 byte-exact** (2 NetCDF fixtures `netcdf_cdf1`,
`netcdf_cdf2` + 2 HDF5 fixtures `hdf5_basic`, `hdf5_userblock`), with 4 native-
object checks, 2 native-dimension checks, 4 native-attribute checks, 3 structural
declines, 2 out-of-subset declines, and 4 opaque controls.

## Economic court

Measured by `tools/phase21-34-hdf-econ-court.sh` (a thin wrapper over the shared
`textfmt-court.sh` engine) against two comparators — a **source-retaining SQLite
store** with conventional extraction, and a conventional **decode-to-host-values
load** (`conv`). Corpus 4 fixtures, questions Q1–Q12, exactness **8/8**.
Estimator = paired per-fixture ratio VOLE/comparator, median + geometric mean with
a fixed-seed (2136), fixture-clustered 10000-resample 95 % CI, tie band ±10 %;
ratio-of-sums reported separately. A ratio < 1 favours VOLE.

| metric | comparator | median | geomean | median 95% CI | geomean 95% CI | W/T/L | ratio-of-sums |
|---|---|---:|---:|---|---|---:|---:|
| build | sqlite | 0.702 | 0.712 | 0.643..0.811 | 0.661..0.776 | 4/0/0 | 0.708 |
| build | conv | 0.974 | 0.988 | 0.944..1.063 | 0.953..1.039 | 0/4/0 | 0.986 |
| storage | sqlite | 0.766 | 0.782 | 0.658..0.985 | 0.664..0.921 | 3/1/0 | 0.793 |
| storage | conv | 29.258 | 24.206 | 11.832..51.796 | 11.945..49.054 | 0/0/4 | 21.340 |
| cold | sqlite | 0.022 | 0.023 | 0.021..0.026 | 0.021..0.025 | 4/0/0 | 0.023 |
| cold | conv | 0.023 | 0.024 | 0.023..0.028 | 0.023..0.027 | 4/0/0 | 0.024 |
| warm | sqlite | 0.097 | 0.096 | 0.089..0.103 | 0.091..0.101 | 4/0/0 | 0.097 |
| warm | conv | 1.336 | 1.349 | 1.299..1.430 | 1.310..1.402 | 0/0/4 | 1.346 |

Build is a win vs SQLite and ~parity with the conventional load; storage is
~0.77× SQLite but a large **recorded loss vs the conventional load**; cold and
warm are wins vs SQLite, and warm is a **recorded loss vs the conventional
in-process load**.

## Honest negatives

* **`comparator_absent` (recorded):** **HDF5 has no faithful Python-stdlib
  decoder**, so the conventional `conv` lane declines **every HDF5 fixture
  question** as a typed `comparator_absent` decline — never a silent or
  fabricated answer — and VOLE capability is shown as a `capability-gap` against
  it. Only the NetCDF-classic part is decoded by `conv`.
* **HDF5 is a bounded subset**: superblock v2/v3, v2/v3 object headers,
  compact/chunked/virtual layouts, filtered datasets, external data files, and
  every datatype class other than fixed-point/float/string **decline typed**.
* Dataset values are never decoded; filtered data is never decompressed; the v2
  link machinery is never traversed.
* **Storage-vs-`conv` and warm-vs-`conv` are recorded losses.**
* Only exact closure (Q6) is a byte-authority claim; the **conventional load is
  deliberately the weaker comparator**.
* The **cold ratios** are dominated by the comparators' fresh-process **Python
  start-up**.
* The economic court is a **self-authored deterministic corpus** (by
  `tools/fixtures/make-hdf.py` and `examples/hdf_fixtures.rs`), not a real-world
  scientific-file population.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.34.1 scientific-container adapter court (exactness 4/4):
  [2026-10-10-phase21-34-1-hdf-ea347edb](../../evidence/campaigns/2026-10-10-phase21-34-1-hdf-ea347edb/).
- Phase 21.34 scientific-container economic court (exactness 8/8):
  [2026-10-11-phase21-34-hdf-econ-372a86e0](../../evidence/campaigns/2026-10-11-phase21-34-hdf-econ-372a86e0/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
