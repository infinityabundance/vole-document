# Campaign: 2026-10-05-phase3-486aa17 — Phase 3 — PDF physical scanner

## Method

The exact binary built from this commit materializes a deterministic sample
corpus with `pdf-make-samples`, then for every sample runs `pdf-inspect`
(structural view), `encode`/`decode`/`verify` (exact container lane) and
`cmp` between the decoded bytes and the source. Every sample's `/Length`
and `startxref` are correct by construction, so the corpus needs no PDF
writer and no canonicalization. Counts are aggregate over the whole corpus;
the bytes are the authority, not the file name.

## Corpus

| file | source_len | is_pdf | spans | objects | revisions | encoded_len | winner | byte_compare |
| --- | ---: | :---: | ---: | ---: | ---: | ---: | --- | --- |
| classic.pdf | 329 | true | 21 | 3 | 1 | 642 | RAW | equal |
| xrefstream.pdf | 251 | true | 20 | 3 | 1 | 564 | RAW | equal |
| objstm.pdf | 341 | true | 23 | 3 | 1 | 654 | RAW | equal |
| incremental.pdf | 456 | true | 28 | 3 | 2 | 769 | RAW | equal |
| mixedeol.pdf | 248 | true | 19 | 2 | 1 | 561 | RAW | equal |
| traptext.pdf | 266 | true | 17 | 2 | 1 | 579 | RAW | equal |
| trapstream.pdf | 250 | true | 19 | 2 | 1 | 563 | RAW | equal |
| malformed.pdf | 85 | false | 8 | 1 | 0 | 398 | RAW | equal |
| notpdf.bin | 41 | false | 16 | 0 | 0 | 354 | RAW | equal |

`malformed.pdf` (truncated, no `%%EOF`) and `notpdf.bin` are deliberate
negative controls: both must be rejected while still round-tripping exactly
through the opaque RAW lane.

## Coverage results

pdf_count = 8
valid_pdf_count = 7
total_spans = 171
total_objects = 19
total_revisions = 8
all_covered = true

Every well-formed PDF is detected (`is_pdf` true) with at least one object
and one `%%EOF`-terminated revision; both negative controls report
`is_pdf` false.

## Exactness

sum_source = 2267
sum_encoded = 5084
all_exact = true

Every decoded output was byte-identical to its source (`cmp` equal), so
`materialize(descriptor) == original_bytes` holds for the whole corpus.

## Court outcome

The literal PDF candidate won the complete-cost court in 0 of
7 well-formed PDFs; the remaining files were won by RAW.
This is the expected Phase-3 result: the PDF lane persists each physical
span as one `INLINE` op with no structural compression, so its per-span
overhead almost always loses to a single RAW literal once the complete cost
is charged. Structural wins that could change this are Phase 5.

## Oracle

The independent qpdf differential oracle is tools/pdf-oracle.sh, run
separately in the tools image; it writes `oracle.jsonl` (see manifest
`oracle`). It is a correctness reference, never byte authority.

## Verdict

PASS
