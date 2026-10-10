# Phase 12.10 — source-removal / restart court

**Result: 38/38 assertions passed, 0 failed.**

For each of PDF, DOCX and EPUB the source was ingested from a private copy,
the oracle length + SHA-256 recorded, then **the source and the ingest
descriptor were deleted**. Every query and the exact materialization then ran
in a fresh process against the field store alone.

| format | source deleted | re-query (fresh process) | materialize --exact |
|---|---|---|---|
| pdf | yes | common + native | len=905 sha256=fae3034c76c4... |
| docx | yes | common + native | len=3458 sha256=18a55a0830da... |
| epub | yes | common + native | len=2858 sha256=88f8b9120c89... |

`materialize --exact` matched the recorded oracle by length, SHA-256 and
`cmp` for all three formats — the field is self-contained, not a pointer to
the source.

## Assertions

```
PASS  pdf.oracle.len                         905
PASS  pdf.oracle.sha256                      fae3034c76c49e0f99af687e710147353b25c48bd6cd2acdb557ac9e174ebd07
PASS  pdf.source_deleted                     gone
PASS  pdf.descriptor_deleted                 gone
PASS  docx.oracle.len                        3458
PASS  docx.oracle.sha256                     18a55a0830da155f482a4b3790b3a1f2671474edfa7a5e48a59d8ddccb8a2fd9
PASS  docx.source_deleted                    gone
PASS  docx.descriptor_deleted                gone
PASS  epub.oracle.len                        2858
PASS  epub.oracle.sha256                     88f8b9120c8914911abadf30e25ef890faba70bfae28c67bc2ba7dfa2bbc09be
PASS  epub.source_deleted                    gone
PASS  epub.descriptor_deleted                gone
PASS  pdf.restart.find                       contains
PASS  pdf.restart.find.provenance            contains
PASS  pdf.restart.text                       contains
PASS  pdf.restart.native_page                contains
PASS  pdf.restart.metadata_len               905
PASS  docx.restart.find                      contains
PASS  docx.restart.find.provenance           contains
PASS  docx.restart.text                      contains
PASS  docx.restart.heading                   Introduction
PASS  docx.restart.cell_b7                   bravo-seven
PASS  docx.restart.resource_rel              rIdImg
PASS  epub.restart.find                      contains
PASS  epub.restart.find.provenance           contains
PASS  epub.restart.text                      contains
PASS  epub.restart.heading                   Introduction
PASS  epub.restart.cell_b7                   bravo-seven
PASS  epub.restart.resource_sha256           823d5d6eeee43d3e74e9a1b636b902dccd533c668ce433adf689fd10af8e42ba
PASS  pdf.restart.materialize.len            905
PASS  pdf.restart.materialize.sha256         fae3034c76c49e0f99af687e710147353b25c48bd6cd2acdb557ac9e174ebd07
PASS  pdf.restart.materialize.cmp            equal
PASS  docx.restart.materialize.len           3458
PASS  docx.restart.materialize.sha256        18a55a0830da155f482a4b3790b3a1f2671474edfa7a5e48a59d8ddccb8a2fd9
PASS  docx.restart.materialize.cmp           equal
PASS  epub.restart.materialize.len           2858
PASS  epub.restart.materialize.sha256        88f8b9120c8914911abadf30e25ef890faba70bfae28c67bc2ba7dfa2bbc09be
PASS  epub.restart.materialize.cmp           equal
```

---

## Environment (receipt)

- Commit under test: `7f007e2e11d1240f638fd10d8ebd3df2751dea33` (branch `main`), dirty files: `?? evidence/campaigns/2026-10-10-identity-7f007e2e/;?? evidence/campaigns/2026-10-10-phase12-removal-7f007e2e/;`
- Service image: `vole-document/db-baseline:1.99.0`; base `rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e`
- Toolchain: rustc `1.99.0`, cargo `1.99.0 (5f94df478 2026-08-27)`, arch `x86_64`
- `Cargo.lock` sha256: `7a36c1cbcddffca16d2bf86ed46e36b18d054e987da81fe13e007968ea423927`
- Run (UTC): 2026-10-10T11:05:10Z
