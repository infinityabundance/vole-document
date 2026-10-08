# Phase 17 item 1 — direct source → field ingestion (no candidate search)

The runtime path to build a field used to be `encode SRC OUT.voldoc` (the whole
candidate portfolio: PDF DEFLATE replay, BYTE_RANS, RLE, RAW, …) followed by
`field-ingest OUT.voldoc --store DIR`. The field is **not** a compression
product, and its observations are derived by re-scanning the *materialized
source* (Stage B), never by reading the winning reconstruction program — so the
portfolio search is pure overhead for a runtime ingest.

This campaign measures the new direct path:

```
vole-document field-build INPUT --store DIR [--profile runtime] [--workers N] [--voldoc OUT.voldoc] [--entropyfs | --packed]
```

It produces both the exact authority (the fixed profile's `.voldoc`, stored in
the field and optionally written to disk) and the field in **one process**, with
**no candidate search**. The existing `encode` / `field-ingest` behaviour is
untouched, and `field-build` reuses them internally. The `--voldoc` authority is
an ordinary `.voldoc`: feeding it back through `field-ingest` yields the **same
field id** (smoke-checked).

## The fixed program (`--profile runtime`)

`runtime` fixes the program to the literal **`RAW`** floor: one literal object
and one `EMIT_OBJECT`. Exactly **one** candidate is priced — and still serialized,
decoded, and byte-compared by the same complete-cost court the searched path
uses (`candidates_evaluated == 1`).

**Why RAW is observation-complete.** Both `ingest_pdf_with` (Stage B) and
`ingest_package_with` materialize the exact source from the descriptor and then
**re-scan the source bytes** into exact spans, object/stream/page nodes, package
members, resource blobs and the hierarchical index. The descriptor program is
consulted only for the advisory `OBSERVATION_INDEX` op table (a seek
optimization that falls back to the full descriptor path when it cannot apply)
and for the exactness check. Structure exposure is therefore a function of the
*source*, not of the program, so a single deterministic exact program preserves
it. RAW is also the cheapest to build, which is the point of the profile.

## Measurement — `doc-baseline` (mem_limit == memswap_limit == 6g, pids_limit 4096, cpus 8)

9-document subset spanning pdf/docx/epub and four size classes. Wall ms and peak
RSS are from GNU `/usr/bin/time -v`; current = `encode` + `field-ingest`, direct =
`field-build` (one process). `auth B` = serialized exact authority (`encoded_len`).

| id | fmt | size class | src B | cur ms | dir ms | speedup | cur RSS KB | dir RSS KB | cur auth B | dir auth B |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| nist-pdf-0002 | pdf | 100KiB-1MiB | 263703 | 1417 | 726 | 1.95× | 51792 | 8708 | 255769 | 264184 |
| nist-pdf-0004 | pdf | 1-10MiB | 1105061 | 2265 | 673 | 3.37× | 24544 | 15228 | 1099654 | 1105542 |
| nasa-pdf-0007 | pdf | 1-10MiB | 10034634 | 2123 | 873 | 2.43× | 103568 | 101892 | 9824310 | 10035115 |
| nist-docx-0010 | docx | <100KiB | 67602 | 76 | 72 | 1.06× | 15596 | 4204 | 66132 | 68083 |
| nist-docx-0001 | docx | 1-10MiB | 2286200 | 214 | 166 | 1.29× | 23856 | 23464 | 2286046 | 2286681 |
| nist-epub-0008 | epub | <100KiB | 35841 | 61 | 51 | 1.20× | 9204 | 3976 | 36295 | 36322 |
| nist-epub-0004 | epub | 100KiB-1MiB | 336417 | 223 | 190 | 1.17× | 65244 | 6480 | 336871 | 336898 |
| nasa-epub-0011 | epub | 1-10MiB | 10383142 | 776 | 558 | 1.39× | 95220 | 94844 | 10383053 | 10383623 |
| nasa-epub-0006 | epub | 10-50MiB | 10867010 | 773 | 569 | 1.36× | 100308 | 100968 | 10866279 | 10867491 |

**Aggregate:** wall **7928 → 3878 ms (2.04× sum, 1.39× median)**; by format **pdf
2.56×**, **epub 1.34×**, **docx 1.22×**. Peak RSS **median 51 792 → 15 228 KB**.
Authority bytes **35 154 409 → 35 383 939 (1.007×)**: RAW is at most a few percent
larger, and only on PDFs (packages already win with RAW). Single-run, no
repetition; the direct path's cost is dominated by materializing/rescanning the
source, so the speedup is largest for PDFs (this is the orthogonal search cost,
which grows with stream count and compressibility) and smallest for small
packages.

## Exactness — length + SHA-256 + `cmp`

**9/9** for each of three closures: the current field (`materialize --exact`),
the direct field (`materialize --exact`), and the direct authority decoded
standalone (`decode DIRECT.voldoc`). `raw/exact.tsv` records, per document, the
source SHA-256 and the reconstructed SHA-256 for all three; every reconstructed
length equals the source length and every digest equals the source digest.

## Observation equality

Over a fixed 11-entry schedule (`metadata`, `doc-text`, `page1 text/structure/
operators`, `byte-range exact`, `object exact`, `stream decoded`, `table`,
`resource`, `search`) run through the `observe` CLI on both stores:
**99 comparisons — 53 equal, 46 decline-equal, 0 divergent** (`raw/observe.tsv`).
Normalization drops only the field id and the read/wall counters (not the
answer).

### The one difference (recorded, not hidden)

For **PDF**, `Selector::Metadata` is the native *structural descriptor*
projection and embeds the chosen program's **`object_count`** and **`graph_ops`**.
A fixed program reports its own; the searched winner reports its winner's:

| id | searched `object_count/graph_ops` | direct (`RAW`) |
|---|---|---|
| nist-pdf-0002 | 29/1346 | 1/1 |
| nist-pdf-0004 | 32/763 | 1/1 |
| nasa-pdf-0007 | 0/1 | 1/1 |

Every other metadata field is byte-identical. DOCX/EPUB common metadata is
format-model derived (encoder-independent), so packages show **no** difference.
This is a real finding: the common PDF metadata projection is not
encoder-independent. It is documented here rather than silently changed, because
altering a shipped observation's contract is out of scope for this
implementation task.

## What is preserved vs lost

- **Preserved:** exactness (all three closures); text, structure, operators,
  preview, byte-range/object/stream/table/resource observations, search, and
  package (DOCX/EPUB) metadata — all byte-identical to the searched path.
- **Lost / differs:** the two descriptor-shape counters in PDF
  `Selector::Metadata` (`object_count`, `graph_ops`), which are representation
  facts about the chosen program, not document facts.

## Gates (all in Docker; `dev` service)

- `cargo fmt --all --check` — **PASS**
- `cargo clippy --all-targets --all-features -- -D warnings` — **PASS**
- `cargo test --locked --all-features` — **PASS**
- `cargo test --locked --no-default-features` — **PASS**
- `sh tools/phase1-court.sh` — **PASS** (Phase 1 exact core)

Plus `tests/phase17_direct_field.rs`: an ablation test that builds one PDF
fixture through the portfolio `encode`+`ingest` and through `field-build`,
requiring `candidates_evaluated == 1`, byte-exact materialization, identical
recovered `root_node`/`index_root`/`node_count`, and equal answers across the
schedule (with the metadata shape counters isolated).

## Residual risks

- **Metadata shape counters** (`object_count`, `graph_ops`) remain
  encoder-dependent in the shipped PDF metadata projection; a consumer that
  compares PDF metadata bytes across encoders would see this difference. A
  follow-up could make that projection report only encoder-independent facts.
- **Bigger authority for PDFs** (up to a few % here; potentially more on
  highly compressible PDFs) because RAW does not compress.
- **Wall times are single-run.** The direct path is ~one extra pass over the
  source (court decode+compare) plus the ingest scan; the win is the removed
  search, not a reduced pass count.
- The `field-build` CLI reads the stored authority back **only when `--voldoc`
  is requested**; the default path computes the authority once and does not
  re-read it.
- Files this item did not touch: `real100-v1/`, `tools/real100-court.sh`,
  `tools/fixtures/real100-frontier.py`.
