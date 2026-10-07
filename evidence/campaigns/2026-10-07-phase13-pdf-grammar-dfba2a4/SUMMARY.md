# Campaign: 2026-10-07-phase13-pdf-grammar-dfba2a4 — Phase 13.2

PDF COS grammar/templates (repeated COS-token phrases) as a **size**
mechanism. Pre-registered hypotheses: H1 byte-exactness; H2 0 wins vs the
current VOLE ladder; H3 0 wins vs the best generic compressor; H4 honest
declines on inputs with no recurring COS-structural phrase.

## Result

```json
{
  "files": 28,
  "declined": 21,
  "proposed": 7,
  "wins_vs_ladder": 0,
  "ties_vs_ladder": 4,
  "losses_vs_ladder": 3,
  "wins_vs_prev": 4,
  "ties_vs_prev": 0,
  "losses_vs_prev": 3,
  "wins_vs_generic": 0,
  "ties_vs_generic": 0,
  "losses_vs_generic": 7,
  "lowers_ladder_count": 0,
  "improves_auto_count": 4,
  "prev_delta_wins_total": 34505,
  "prev_delta_losses_total": -16427320,
  "prev_delta_total": -16392815,
  "ladder_total_excl": 17773284,
  "prev_ladder_total": 17807789,
  "best_vole_total_incl": 17773284,
  "new_total": 33767312,
  "generic_total": 5833194
}
```

Exactness of the forced lane:

```json
{"proposed":7,"declined":21,"exact_ok":7,"exact_fail":0}
```

## Per-file (bytes; `new` = PDF_COS_TEMPLATE, `ladder` = best of the current VOLE lanes,
`prev` = best pre-existing VOLE lane = the auto winner this subphase replaced)

| file | source | new | prev | ladder | generic | vs prev | vs generic |
| --- | ---: | ---: | ---: | ---: | ---: | --- | --- |
| `cairo-vector.pdf` | 58424 | 19411 | 34633 | 19411 | 16670 | win | loss |
| `gs-default.pdf` | 2890 | - | 3255 | 3255 | 1598 | decline | decline |
| `gs-ebook.pdf` | 2888 | - | 3254 | 3254 | 1543 | decline | decline |
| `gs-prepress.pdf` | 6480 | - | 6888 | 6888 | 4654 | decline | decline |
| `gs-printer.pdf` | 9143 | - | 9499 | 9499 | 6808 | decline | decline |
| `gs-screen.pdf` | 2889 | - | 3253 | 3253 | 1571 | decline | decline |
| `hand-base1.pdf` | 120845 | - | 74680 | 74680 | 2884 | decline | decline |
| `hand-base2.pdf` | 181787 | - | 56944 | 56944 | 3454 | decline | decline |
| `large.pdf` | 33615730 | 33591494 | 17190273 | 17190273 | 5677552 | loss | loss |
| `libreoffice-export.pdf` | 74371 | 66895 | 72850 | 66895 | 56810 | win | loss |
| `pdftex-doc.pdf` | 23622 | 18197 | 24076 | 18197 | 17062 | win | loss |
| `qpdf-compress.pdf` | 72610 | - | 49465 | 49465 | 6120 | decline | decline |
| `qpdf-linearize.pdf` | 73143 | - | 49726 | 49726 | 6176 | decline | decline |
| `qpdf-nocompress.pdf` | 120893 | - | 74672 | 74672 | 2888 | decline | decline |
| `qpdf-preserve-objectstreams.pdf` | 181872 | - | 57039 | 57039 | 3520 | decline | decline |
| `reportlab-multipage.pdf` | 10906 | 3754 | 11203 | 3754 | 2012 | win | loss |
| `bigtext.pdf` | 65549 | - | 38274 | 38274 | 824 | decline | decline |
| `classic.pdf` | 329 | - | 783 | 783 | 172 | decline | decline |
| `flate.pdf` | 57513 | 57927 | 36161 | 36161 | 18884 | loss | loss |
| `incremental.pdf` | 456 | - | 900 | 900 | 190 | decline | decline |
| `malformed.pdf` | 85 | - | 539 | 539 | 83 | decline | decline |
| `many.pdf` | 9881 | 9634 | 5301 | 5301 | 801 | loss | loss |
| `mixedeol.pdf` | 248 | - | 702 | 702 | 156 | decline | decline |
| `notpdf.bin` | 41 | - | 495 | 495 | 46 | decline | decline |
| `objstm.pdf` | 341 | - | 795 | 795 | 201 | decline | decline |
| `trapstream.pdf` | 250 | - | 704 | 704 | 169 | decline | decline |
| `traptext.pdf` | 266 | - | 720 | 720 | 183 | decline | decline |
| `xrefstream.pdf` | 251 | - | 705 | 705 | 163 | decline | decline |

## Interpretation

The candidate is byte-exact where it is proposed (H1 holds) and the result
is **mixed, not a pure loss**. On the 4 files where repeated COS boilerplate
recurs enough to amortize the per-occurrence framing it **becomes the best
VOLE lane** (the auto winner drops by 5879–15222 B; total
34505 B over those files) — a genuine scoped positive on the *VOLE ladder* — but it
never beats a generic compressor (0 wins; brotli/xz are 2–4× smaller) and it
loses to the pre-existing ladder on the 3 large/entropy-heavy files it does propose. It declines
on 21/28 because no COS phrase recurs enough. The top-level verdict is
unchanged: a bounded structural grammar beats a whole-file order-0 lane on
repetitive syntax, but not a purpose-built generic LZ (ADR-0037).
