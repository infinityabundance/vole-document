# Campaign: 2026-10-07-phase13-pdf-length-revision-12fc84e — Phase 13.1

PDF `/Length` + revision proceduralization as a **size** mechanism.
Pre-registered hypotheses: H1 byte-exactness; H2 0 wins vs the current
VOLE ladder; H3 0 wins vs the best generic compressor; H4 honest
declines on xref-stream / non-PDF inputs.

## Result

```json
{
  "files": 28,
  "declined": 7,
  "proposed": 21,
  "wins_vs_ladder": 0,
  "ties_vs_ladder": 0,
  "losses_vs_ladder": 21,
  "wins_vs_generic": 0,
  "ties_vs_generic": 0,
  "losses_vs_generic": 21,
  "lowers_ladder_count": 0,
  "ladder_total_excl": 17807789,
  "best_vole_total_incl": 17807789,
  "new_total": 920497,
  "generic_total": 5833194
}
```

Exactness of the forced lane:

```json
{"proposed":21,"declined":7,"exact_ok":21,"exact_fail":0}
```

## Per-file (bytes; `new` = PDF_LENGTH_REVISION, `ladder` = best of the current VOLE lanes)

| file | source | new | ladder | generic | vs ladder | vs generic |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| `cairo-vector.pdf` | 58424 | 59016 | 34633 | 16670 | loss | loss |
| `gs-default.pdf` | 2890 | 3469 | 3255 | 1598 | loss | loss |
| `gs-ebook.pdf` | 2888 | 3467 | 3254 | 1543 | loss | loss |
| `gs-prepress.pdf` | 6480 | 7062 | 6888 | 4654 | loss | loss |
| `gs-printer.pdf` | 9143 | 9729 | 9499 | 6808 | loss | loss |
| `gs-screen.pdf` | 2889 | 3468 | 3253 | 1571 | loss | loss |
| `hand-base1.pdf` | 120845 | 121420 | 74680 | 2884 | loss | loss |
| `hand-base2.pdf` | 181787 | 182367 | 56944 | 3454 | loss | loss |
| `large.pdf` | 33615730 | - | 17190273 | 5677552 | decline | decline |
| `libreoffice-export.pdf` | 74371 | 75209 | 72850 | 56810 | loss | loss |
| `pdftex-doc.pdf` | 23622 | - | 24076 | 17062 | decline | decline |
| `qpdf-compress.pdf` | 72610 | - | 49465 | 6120 | decline | decline |
| `qpdf-linearize.pdf` | 73143 | - | 49726 | 6176 | decline | decline |
| `qpdf-nocompress.pdf` | 120893 | 121469 | 74672 | 2888 | loss | loss |
| `qpdf-preserve-objectstreams.pdf` | 181872 | 182453 | 57039 | 3520 | loss | loss |
| `reportlab-multipage.pdf` | 10906 | 11491 | 11203 | 2012 | loss | loss |
| `bigtext.pdf` | 65549 | 66123 | 38274 | 824 | loss | loss |
| `classic.pdf` | 329 | 905 | 783 | 172 | loss | loss |
| `flate.pdf` | 57513 | 58112 | 36161 | 18884 | loss | loss |
| `incremental.pdf` | 456 | 1030 | 900 | 190 | loss | loss |
| `malformed.pdf` | 85 | - | 539 | 83 | decline | decline |
| `many.pdf` | 9881 | 10263 | 5301 | 801 | loss | loss |
| `mixedeol.pdf` | 248 | 843 | 702 | 156 | loss | loss |
| `notpdf.bin` | 41 | - | 495 | 46 | decline | decline |
| `objstm.pdf` | 341 | 924 | 795 | 201 | loss | loss |
| `trapstream.pdf` | 250 | 834 | 704 | 169 | loss | loss |
| `traptext.pdf` | 266 | 843 | 720 | 183 | loss | loss |
| `xrefstream.pdf` | 251 | - | 705 | 163 | decline | decline |

## Interpretation

The candidate is byte-exact where it is proposed (H1 holds) but it **loses**:
it is never smaller than the current VOLE ladder and never beats a generic
compressor. Regenerating a `/Length` value or an xref/`/Prev` offset costs
a `Mark` plus an `Emit` item (and a slot) per field, which at these scales
exceeds the few digits it removes — the same per-site framing that sank
`PDF_LAYOUT` (ADR-0011/0012/0013). This is a recorded negative
(ADR-0036), not an adopted mechanism.
