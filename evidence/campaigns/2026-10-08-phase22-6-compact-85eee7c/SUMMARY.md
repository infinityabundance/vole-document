# Phase 22.6 (P5) — compact structural representation (measurement court)

**Question.** Can an existing typed index / structural node be encoded
more compactly while preserving the same observation capability?

**Answer (measured).** NO WIN (below bar)

## Headline

* Population: **100** documents.
* Total persistent footprint: **2808956109** B.
* Typed structural bytes (seed + index + field manifest, i.e. **not** the
  exact-authority `descriptor/`): **77665688** B =
  **2.765%** of the footprint.
* Combined compact representation of the typed bytes: **44902837** B,
  byte-exact round-trip verified for **100/100** documents.
* Observation invariance (labelled subset of **3** docs): the store
  rebuilt from the decoded compact bytes is byte-identical to the original for
  **3/3**, and the semantic `observe-batch` answers
  (with the stateful `stats` block removed) match for **3/3**.
* Saving: **32762851** B = **1.1664%** of the total
  persistent footprint. Bar = **5%**. Verdict: **NO WIN (below bar)**.

## Byte composition by namespace (whole population)

| namespace | bytes | share |
|---|---:|---:|
| `descriptor/` | 2731290421 | 97.235% |
| `seed/` | 60349064 | 2.148% |
| `index/` | 17273125 | 0.615% |
| `field/` | 43499 | 0.002% |

## Seed-node byte composition by `NodeKind` (whole population)

| kind | name | bytes | nodes | share of seed |
|---|---|---:|---:|---:|
| 22 | ResourceBlob | 34860100 | 144 | 57.76% |
| 4 | PdfObject | 10253760 | 155360 | 16.99% |
| 5 | PdfStreamEncoded | 6291480 | 85020 | 10.43% |
| 6 | PdfStreamDecoded | 4611452 | 49058 | 7.64% |
| 25 | PdfRevisionLineage | 2472124 | 192 | 4.10% |
| 9 | PageContent | 1319852 | 16349 | 2.19% |
| 16 | PackageMemberDecoded | 307930 | 2905 | 0.51% |
| 15 | PackageMemberRaw | 204190 | 2917 | 0.34% |
| 3 | PdfRevision | 8976 | 132 | 0.01% |
| 1 | DocumentExact | 3600 | 60 | 0.01% |
| 18 | DocxModel | 3440 | 40 | 0.01% |
| 20 | EpubModel | 3440 | 40 | 0.01% |
| 17 | PackageOpcModel | 3400 | 40 | 0.01% |
| 23 | OdtModel | 3400 | 40 | 0.01% |
| 14 | PackageRoot | 1920 | 40 | 0.00% |

Seed-node internal split (whole population): header=11244132 B, deps=2006848 B, params=41516764 B, prov=5581320 B

> Note: **34860100 B** of the seed bytes (57.76%) are `ResourceBlob`
> (kind 22) nodes, which embed an *exact* resource payload (image/font) in
> their params. That is exact data, not structure: the candidate params
> encoder correctly picks mode `raw` for kind 22, so it neither compresses nor
> credits those bytes. They inflate the *typed* fraction without contributing
> to the saving, so the true structural-redundancy saving is the measured one.

## Candidate encodings (measured on the actual canonical bytes)

| candidate | target class | orig B | compact B | saving B | saving / footprint | round-trip |
|---|---|---:|---:|---:|---:|---|
| E_header | seed node header folding | 11244132 | 1865284 | 9378848 | 0.3339% | 100/100 |
| A_delta | index numerics + seed numeric params (delta/varint) | 48716489 | 41846425 | 6870064 | 0.2446% | 100/100 |
| B_dict | provenance string dictionary | 5596339 | 338218 | 5258121 | 0.1872% | 100/100 |
| D_intern | structural-id interning (content-address recompute) | 12077760 | 834670 | 11243090 | 0.4003% | 100/100 |
| C_bitmap | sparse number-set bitmap+rank (vs delta list) | 312660 (as delta list) | 354576 (bitmap) | - | - | 100/100 |

## Region split — where a byte win exists

| size class | docs | total B | typed B | typed frac | compact saving / footprint |
|---|---:|---:|---:|---:|---:|
| <100KiB | 8 | 478234 | 70834 | 14.81% | 7.6312% |
| 100KiB-1MiB | 17 | 6558598 | 839400 | 12.80% | 7.4538% |
| 1-10MiB | 40 | 171426717 | 8683186 | 5.07% | 3.2146% |
| 10-100MiB | 30 | 1332504954 | 51975067 | 3.90% | 1.0862% |
| >=100MiB | 5 | 1297987606 | 16097201 | 1.24% | 0.9441% |

| format | docs | total B | typed B | typed frac | compact saving / footprint |
|---|---:|---:|---:|---:|---:|
| docx | 15 | 14297348 | 1971587 | 13.79% | 0.7957% |
| epub | 25 | 298088936 | 33760456 | 11.33% | 0.2062% |
| pdf | 60 | 2496569825 | 41933645 | 1.68% | 1.2831% |

Documents whose OWN footprint saving clears the 5% bar: **27** (nasa-pdf-0028, nasa-pdf-eb-01, nist-docx-0005, nist-docx-0006, nist-docx-0007, nist-docx-0008, nist-docx-0010, nist-docx-0011, nist-docx-0012, nist-docx-0013, nist-epub-0002, nist-epub-0003, nist-epub-0006, nist-epub-0008, nist-epub-0010, nist-pdf-0001, nist-pdf-0002, nist-pdf-0003, nist-pdf-0004, nist-pdf-0005, nist-pdf-0007, nist-pdf-0010, nist-pdf-0014, nist-pdf-0015, nist-pdf-0016, nist-pdf-0017, nist-pdf-0020),
covering **1.432%** of the population's persistent bytes.

## What the court cannot conclude

- It does not build or ship a compact store; it only sizes one and
  proves byte-exact decode. Decode cost (a topological content-address
  pass + BLAKE3 verification) is NOT measured here.
- `observe` is stateful: a `--page N --kind text` (or equivalent derived)
  observation runs demand-driven `deepen_page`, which adds a derived field
  manifest. Observation invariance is therefore argued from **byte identity
  of the rebuilt store** (the primary proof) plus a semantic re-run on two
  fresh-equivalent stores; the `stats`/timing block is not an answer.
- The `descriptor/` namespace (the exact authority) is ~= the source
  length + 481 B of container framing on every document; it is left
  untouched. Compacting it is generic compression (a different
  mechanism, out of this phase's scope) and not a typed-node question.
- Per-format generality is limited to the formats present in
  `real100-v1` (pdf/docx/epub); ODT/PPTX/XLSX are not in this population.
