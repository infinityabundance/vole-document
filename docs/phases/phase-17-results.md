# Phase 17 results

Branch: `phase17`. Base: `main` @ `d83ddba` (`v0.1.0-alpha.21`, Phase 16).

Phase 17 attacks the two weaknesses Phase 16 made explicit. It adds a **direct
source → field ingestion path** that skips the compression candidate search the
field does not need (17.1), and it adds the **PDF revision-lineage query
surface** the Phase-16.5 contract court recorded as missing (17.2). Every number
links to a sealed receipt under
[`evidence/campaigns/`](../../evidence/campaigns/); negatives are recorded, not
buried. No claim here is a population claim: every corpus is frozen.

## What Phase 17 established

- **The build-economics weakness is materially reduced** (17.1): `field-build`
  ingests source → field directly with a fixed, non-searched exactness program
  (`--profile runtime` = `RAW`), cutting the build wall sum **7,928 → 3,878 ms
  (2.04×)** and the peak RSS median **51,792 → 15,228 KB (3.4× smaller)** on a
  9-document subset — at **1.007×** authority bytes, exactness **9/9**, and **0**
  divergent observations.
- **The revision surface now exists** (17.2): a PDF **revision-lineage**
  selector/representation, computed once at ingest from the existing
  byte-authoritative scan and indexed, answers the revision chain cold and in a
  mixed batch, with a **typed decline** for non-PDF formats. This is a real
  capability addition, not a re-answer.
- **C4/C5 still do not close** (17.2): the contract court re-run splits the
  Phase-16.5 C4 decline in two. The **surface** half is fixed, but the contract's
  C4 tuple is the **corpus family/member/head** — external metadata a
  single-document field cannot derive from PDF bytes. For PDFs the two lanes
  report *different* lineage observables (recorded as such, never equality); for
  DOCX/EPUB VOLE declines while the baseline answers.
- **The honest next question** is therefore not "add more surface": it is
  whether the contract's **C4 definition** is the right one, or whether VOLE
  should ingest **corpus/revision-family metadata as an explicit external input**
  ([ADR-0052](../adr/0052-revision-lineage-surface.md)).

## Direct source → field ingestion (17.1)

**Question.** The runtime path to build a field was `encode SRC OUT.voldoc` (the
whole candidate portfolio: PDF DEFLATE replay, `BYTE_RANS`, RLE, RAW, …) followed
by `field-ingest OUT.voldoc --store DIR`. The field's observations are derived by
re-scanning the *materialized source* (Stage B), never by reading the winning
reconstruction program, so the portfolio search is pure overhead for a runtime
ingest. Can a direct path remove that overhead without changing exactness or the
observation surface?

**Receipt.** [`2026-10-08-phase17-direct-field-d83ddba`](../../evidence/campaigns/2026-10-08-phase17-direct-field-d83ddba/).

**Result — ADOPTED; 2.04× build, 3.4× lower RSS, 1.007× authority, exactness
9/9, observations 0 divergent.**

New `src/field/build.rs` and the CLI

```
field-build INPUT --store DIR [--profile runtime] [--workers N] [--voldoc OUT] [--entropyfs | --packed]
```

build the exact authority and the field in **one process** with **no candidate
search**. `--profile runtime` fixes the program to the literal **`RAW`** floor:
one literal object and one `EMIT_OBJECT`. Exactly **one** candidate is priced —
and still serialized, decoded and byte-compared by the same complete-cost court
the searched path uses (`candidates_evaluated == 1`). The existing `encode` /
`field-ingest` behaviour is untouched, and `field-build` reuses them internally;
the `--voldoc` authority is an ordinary `.voldoc` (feeding it back through
`field-ingest` yields the same field id, smoke-checked).

**Why RAW preserves the observation surface.** This is the load-bearing fact:
observations are *not* read from the winning program. Both `ingest_pdf_with`
(Stage B) and `ingest_package_with` materialize the exact source from the
descriptor and then **re-scan the source bytes** into exact spans,
object/stream/page nodes, package members, resource blobs and the hierarchical
index. The descriptor program is consulted only for the advisory
`OBSERVATION_INDEX` op table (a seek optimization that falls back to the full
descriptor path when it cannot apply) and for the exactness check. Structure
exposure is therefore a function of the **source**, not of the program, so one
deterministic exact program preserves it.

Measured on `doc-baseline` (mem_limit == memswap_limit == 6g, pids_limit 4096,
cpus 8), **9 documents** spanning pdf/docx/epub and four size classes; `current`
= `encode` + `field-ingest`, `direct` = `field-build` (one process):

| quantity (median) | current | direct | ratio |
|---|---:|---:|---:|
| wall ms (sum over 9) | 7,928 | 3,878 | **2.04×** |
| peak RSS KB (median) | 51,792 | 15,228 | **3.4× smaller** |
| authority bytes (sum) | 35,154,409 | 35,383,939 | **1.007×** |

By-format wall (sum): **pdf 2.56×** (n=3), **epub 1.34×** (n=4), **docx 1.22×**
(n=2). **Exactness is 9/9** for each of three closures — the current field
(`materialize --exact`), the direct field (`materialize --exact`), and the direct
authority decoded standalone (`decode DIRECT.voldoc`) — each checked by
**length + SHA-256 + `cmp`**; every reconstructed length equals the source length
and every digest equals the source digest. **Observation equality** over a fixed
11-entry schedule (`metadata`, `doc-text`, page-1
text/structure/operators, byte-range exact, object exact, stream decoded, table,
resource, search): **99 comparisons — 53 equal, 46 decline-equal, 0 divergent**
(normalization drops only the field id and the read/wall counters, never the
answer).

**The one recorded difference (not silently changed).** For **PDF**,
`Selector::Metadata` is the native *structural-descriptor* projection and embeds
the chosen program's `object_count` and `graph_ops`. A fixed program reports its
own; the searched winner reports its winner's:

| id | searched `object_count/graph_ops` | direct (`RAW`) |
|---|---|---|
| nist-pdf-0002 | 29/1346 | 1/1 |
| nist-pdf-0004 | 32/763 | 1/1 |
| nasa-pdf-0007 | 0/1 | 1/1 |

Every other metadata field is byte-identical, and DOCX/EPUB common metadata is
format-model derived (encoder-independent), so packages show **no** difference.
This is a real finding: the shipped common PDF metadata projection is **not
encoder-independent** (its two shape counters are representation facts about the
chosen program, not document facts). It is documented rather than changed because
altering a shipped observation's contract is out of scope for this item.

**Interpretation.** The direct path removes the *search*, not a materialization
pass: its cost is dominated by materializing/rescanning the source, so the win is
largest for PDFs (the format whose search cost grows with stream count and
compressibility) and smallest for small packages. A single deterministic, exact
program is a sound runtime choice precisely because structure comes from the
source.

**Limitation.** A 9-document subset; wall times are single-run (no repetition).
RAW authority is a few percent larger on PDFs (it does not compress), and could
be more on highly compressible PDFs. The two PDF metadata shape counters remain
encoder-dependent. No population claim.

## Revision lineage surface (17.2)

**Question.** Phase 16.5 recorded VOLE's C4/C5 decline as a missing CLI surface
(its CLI had no revision query). Add a PDF revision-lineage surface and re-run
the contract-equivalent court: does C4/C5 close, and at what cost?

**Receipt.** [`2026-10-08-phase17-revision-1179386`](../../evidence/campaigns/2026-10-08-phase17-revision-1179386/).

**Result — SURFACE ADDED; C0–C3 satisfied; C4/C5 still do not close.**

New `Selector::Revisions` (canonical `revisions`) and `Representation::Lineage`
(`lineage`), with CLI `observe --revisions --kind lineage` and
`observe --revision N --kind lineage`, advertised in capabilities. A new
`NodeKind::PdfRevisionLineage` is computed **once** at PDF ingest from the
existing byte-authoritative scan — never re-parsing the source at query time —
and indexed under `SEL_REVISIONS` / `SEL_REVISION_LINEAGE`, so an observe is
**O(depth)** into the store. It answers the `%PDF-` header, the revision count,
the ordered revision indices and byte spans, the resolved `startxref`/`/Prev`
chains, and each revision's object/stream membership. Non-PDF formats are a
**typed decline** (`UnsupportedFeature`, rc 6), never an empty answer.

Real-corpus example: default-subset PDFs answer `count: 2` — e.g.
`nist-pdf-0016` (`%PDF-1.6`) reports revision 0 over `[0, 510)` defining objects
`{467, 488}`, and revision 1 over `[512, 310625)`.

**The contract court re-run** (the SQLite lane is byte-identical to Phase 16.5,
verified by diff) on the same **12-document** subset, cost per depth:

| depth | VOLE cold ms | SQLite cold ms | VOLE warm ms | SQLite warm ms | VOLE B | SQLite B | VOLE satisfies? |
|---|---:|---:|---:|---:|---:|---:|---|
| C0 | 138 | 129 | 37 | 25 | 6,988,757 | 14,270,464 | yes |
| C1 | 135 | 128 | 38 | 25 | 6,988,757 | 14,376,960 | yes |
| C2 | 136 | 124 | 36 | 28 | 6,988,757 | 14,622,720 | yes |
| C3 | 129 | 125 | 37 | 27 | 6,988,757 | 14,622,720 | yes |
| C4 | 133 | 129 | 37 | 29 | 6,988,757 | 14,721,024 | **no (different observable / declines)** |
| C5 | 142 | 134 | 37 | 29 | 6,988,757 | 14,721,024 | **no (different observable / declines)** |

Cost vs SQLite on this subset: storage **0.47×** (6,988,757 vs 14,721,024 B;
the lineage surface adds **74,980 B** over the 12 documents), build **10.74×**,
cold 138 vs 129 ms, warm 37 vs 25 ms. **C0–C3 are satisfied by both lanes;
C4/C5 are not.**

**Why C4 does not close.** The surface half *is* fixed: VOLE now answers a
revision-lineage observation for every PDF, both cold and in a mixed batch, where
Phase 16.5 declined. But the contract's C4 tuple is the **corpus
family/member/head** — external metadata the PDF bytes cannot derive. So the two
lanes answer *different* observables: VOLE reports the PDF's internal incremental
revision chain; the baseline reports the corpus tuple supplied as frozen
metadata. Neither value can equal the other by construction, so this is recorded
as **`different observable`** — never as equality and never as an error. For
DOCX/EPUB, which have no PDF revision structure, VOLE declines typed while the
baseline answers the corpus tuple. C4 therefore does not close on the contract's
terms.

**Interpretation.** Adding the surface is a genuine capability addition and it
confirms the decoder-authority boundary is clean: the new node and index kinds
change store bytes only, and the `.voldoc` descriptor and `materialize == bytes`
are untouched. It does **not** close the contract, because C4 as defined is not
PDF-derivable.

**Limitation.** A 12-document subset; no population claim. Lineage fidelity is
bounded by the Phase-3 `%%EOF` scanner — on `nist-pdf-0016` it splits at an
embedded early `%%EOF` at offset 505 and reports an inverted `/Prev` — so the
answer is **not** an independent PDF-conformance oracle, and this is recorded as
residual risk.

## Cross-cutting notes

- Every exactness statement in Phase 17 is the same invariant:
  `materialize(descriptor) == original_bytes` (length + SHA-256 + `cmp`). 17.1
  carries a three-way closure (current field / direct field / direct authority);
  17.2 leaves the descriptor and `materialize == bytes` untouched.
- **No cap was raised** and no validate-or-decline rule was weakened. 17.1
  removes *search* work (a runtime optimization); 17.2 adds a derived, indexed
  node. Neither changes a wire byte or the decode path.
- No corpus or schedule was tuned against any measurement; the `real100-v1`
  manifest is frozen by SHA-256.
- The open question Phase 17 hands forward is a **contract-definition** question
  (C4), not a missing-surface one; see
  [ADR-0052](../adr/0052-revision-lineage-surface.md).
