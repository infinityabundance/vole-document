# Campaign: 2026-10-05-phase4-3840bc4 — Phase 4 — forced-candidate ablation

## Method

The exact binary built from this commit materializes the deterministic
sample corpus with `pdf-make-samples`, then for every corpus file forces
each candidate family in turn: `encode --force KIND IN OUT` for KIND in
{raw, rle, byte-rans, pdf-physical, pdf-channels}. A forced encode runs the
*same* complete-cost court over a one-element candidate set, so the forced
descriptor is still serialized, decoded, and byte-compared before it is
returned; forcing selects a lane, it never bypasses exactness. When the
input does not propose a kind the command exits with a typed Usage error
("is not proposed"), which is recorded honestly as `null` rather than
substituted. In parallel, the ordinary `encode` gives the auto winner, and
its output is decoded and `cmp`ed against the source and checked with
`verify`.

## Per-file table

All sizes are serialized `.voldoc` bytes; `null` means the input does not
propose that kind. `byte_cmp` is the auto winner's decoded output compared
to the source.

| file | source | RAW | RLE | BYTE_RANS | PDF_PHYS | PDF_CHAN | winner | byte_cmp |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| bigtext.pdf | 65549 | 65871 | 385346 | 38142 | 66008 | 46432 | BYTE_RANS | equal |
| classic.pdf | 329 | 651 | 2030 | 696 | 738 | 3741 | RAW | equal |
| incremental.pdf | 456 | 778 | 2618 | 768 | 900 | 3826 | BYTE_RANS | equal |
| malformed.pdf | 85 | 407 | 792 | 500 | null | null | RAW | equal |
| mixedeol.pdf | 248 | 570 | 1601 | 635 | 647 | 4667 | RAW | equal |
| notpdf.bin | 41 | 363 | 531 | 433 | null | null | RAW | equal |
| objstm.pdf | 341 | 663 | 2103 | 708 | 760 | 3766 | RAW | equal |
| trapstream.pdf | 250 | 572 | 1609 | 639 | 649 | 4679 | RAW | equal |
| traptext.pdf | 266 | 588 | 1705 | 659 | 655 | 3208 | RAW | equal |
| xrefstream.pdf | 251 | 573 | 1738 | 679 | 655 | 3691 | RAW | equal |

## Cumulative ladder

Each rung adds one mechanism and takes the per-file running minimum, summed
over the 10-file corpus. Adding a mechanism can only lower or hold
the total, never raise it.

| rung | mechanism added | total bytes | step delta |
| --- | --- | ---: | ---: |
| A0 | RAW | 71036 | — |
| A1 | + RLE | 71036 | 0 |
| A2 | + BYTE_RANS | 43297 | -27739 |
| A3 | + PDF_PHYSICAL | 43297 | 0 |
| A4 | + PDF_CHANNELS | 43297 | 0 |

## Leave-one-out

Removing only the channel mechanism from the full portfolio leaves the A3
rung:

- A4_without_channels = A3 = 43297
- leave_one_out_channels_delta = A4 - A3 = 0

So the PDF typed-channel mechanism saved 0 bytes across the
corpus relative to the same portfolio without it.

Auto-winner counts: RAW=8 RLE=0 BYTE_RANS=2
PDF_PHYSICAL=0 PDF_CHANNELS=0.

## Honest interpretation

The ablation is reported plainly. On the text-heavy scale sample
`bigtext.pdf` (source 65549 bytes) the forced sizes were
RAW=65871, BYTE_RANS=38142, PDF_CHANNELS=46432: PDF_CHANNELS beats
RAW but loses to BYTE_RANS. Splitting the PDF into typed lexical channels and
entropy-coding each one pays per-channel model and length overheads that a
single order-0 byte-rANS channel over the whole file does not, and on this
corpus that overhead is not recovered. PDF_PHYSICAL likewise rarely wins: its
one-literal-per-span program adds per-span framing that RAW avoids.

These are measured, scoped results for *this* deterministic corpus and this
commit. They say the current channel split does not yet pay on text-heavy
PDFs; they do not say typed channels cannot pay on other inputs. The winner
is always decided by actual serialized bytes, and every auto winner
round-trips byte-exactly (all_exact=true).

## Verdict

PASS
