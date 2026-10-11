# Phase 26.5 — stratified real-world population v2 (larger scale)

**Question.** At a genuinely larger real-world scale than the v1 60-sample
smoke: ingest a stratified population of independently sourced documents
across the supported format families (OOXML + legacy binary office, ODF,
structured text, prose, tabular/analytical, messaging, geospatial,
scientific/container), plus an extended malformed/hostile stratum, record
the detector's choice for every sample, and report **pooled costs alongside
medians** per declared format and per stratum. The v1 60-sample court remains
the historical control.

**Verdict: `PASS`** (byte-exact closure; findings are recorded, not fatal)

## Counts

| samples | real | hostile | skipped | blocked | exact | detection agree | findings | formats |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 308 | 284 | 24 | 0 | 0 | 308 | 257 | 32 | 41 |

By declared format (ran): arrow=9, asciidoc=7, bib=3, cbor=8, cfbf=5, config=3, container=10, csv=12, doc=9, docx=10, eml=9, epub=3, feed=6, geojson=13, gis=10, hdf=9, html=8, json=11, json5=1, jsonl=9, logstream=6, markdown=6, mdx=7, mhtml=3, msg=7, notebook=8, odp=5, ods=8, odt=8, parquet=11, pdf=3, pkgmeta=5, ppt=7, pptx=12, rst=6, rtf=4, toml=9, xls=9, xlsx=14, xml=4, yaml=11

## Stratified tables

## By declared format

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| arrow | 9 | 211604 | 23688 | 1133894 | 5844 | 28801 | 1729 | 30213 | 2994 | yes |
| asciidoc | 7 | 171079 | 24024 | 369739 | 8386 | 63004 | 9836 | 21587 | 2876 | yes |
| bib | 3 | 77816 | 26791 | 1575364 | 545657 | 25141 | 11372 | 10276 | 3349 | yes |
| cbor | 8 | 170247 | 21540 | 58004 | 5521.0 | 43742 | 5387 | 23833 | 2814 | yes |
| cfbf | 5 | 126873 | 24748 | 1054137 | 134355 | 48978 | 10343 | 16915 | 3266 | yes |
| config | 3 | 76956 | 24071 | 372893 | 7731 | 35408 | 10402 | 11156 | 3163 | yes |
| container | 10 | 233182 | 20958 | 2454600 | 9865.5 | 93340 | 10328 | 36410 | 3156 | yes |
| csv | 12 | 321343 | 26042 | 2500572 | 66873.0 | 140101 | 11398 | 44084 | 3454 | yes |
| doc | 9 | 225972 | 24500 | 1365346 | 57692 | 61531 | 5767 | 31087 | 3268 | yes |
| docx | 10 | 215799 | 21576 | 1290131 | 69418.0 | 351881 | 39587 | 35582 | 3450 | yes |
| eml | 9 | 434506 | 24048 | 93592 | 6266 | 88818 | 10143 | 27593 | 3030 | yes |
| epub | 3 | 62010 | 20731 | 135781 | 33764 | 22655 | 9735 | 9846 | 2954 | yes |
| feed | 6 | 143678 | 23878 | 59015 | 8622.5 | 59863 | 9970 | 18543 | 2957 | yes |
| geojson | 13 | 356879 | 25510 | 9615938 | 431186 | 159094 | 12053 | 47208 | 3392 | yes |
| gis | 10 | 250598 | 23908 | 2235229 | 11032.5 | 100222 | 10038 | 31188 | 2881 | yes |
| hdf | 9 | 211975 | 23685 | 424354 | 14926 | 57340 | 5567 | 26615 | 2897 | yes |
| html | 8 | 204297 | 24150 | 430673 | 23085.5 | 82486 | 10213 | 24509 | 3102 | yes |
| json | 11 | 292872 | 24247 | 3263643 | 14189 | 124092 | 10255 | 36207 | 3075 | yes |
| json5 | 1 | 23648 | 23648 | 3852 | 3852 | 9979 | 9979 | 2901 | 2901 | yes |
| jsonl | 9 | 275794 | 31656 | 12464612 | 1837044 | 142736 | 15943 | 41254 | 4670 | yes |
| logstream | 6 | 214653 | 37146 | 2948319 | 281400.0 | 35336 | 1793 | 25422 | 4134 | yes |
| markdown | 6 | 158931 | 24394 | 630422 | 18609.0 | 62847 | 10204 | 18981 | 3046 | yes |
| mdx | 7 | 173327 | 24276 | 172724 | 13099 | 72515 | 10326 | 21548 | 3075 | yes |
| mhtml | 3 | 70963 | 23594 | 45097 | 7936 | 24929 | 9704 | 8936 | 3009 | yes |
| msg | 7 | 166414 | 24340 | 573955 | 49500 | 41046 | 5722 | 22574 | 3038 | yes |
| notebook | 8 | 190900 | 23678 | 237285 | 7683.0 | 79309 | 9813 | 24146 | 2971 | yes |
| odp | 5 | 105661 | 21166 | 680037 | 138597 | 212633 | 43046 | 15948 | 3222 | yes |
| ods | 8 | 168634 | 20830 | 253580 | 30052.5 | 204684 | 24588 | 24997 | 3091 | yes |
| odt | 8 | 164229 | 20483 | 296774 | 31907.5 | 174076 | 24209 | 24734 | 2974 | yes |
| parquet | 11 | 271184 | 24103 | 1976458 | 9855 | 105863 | 10305 | 37881 | 3208 | yes |
| pdf | 3 | 76033 | 25396 | 97690 | 12438 | 2200 | 646 | 9014 | 2969 | yes |
| pkgmeta | 5 | 120771 | 24114 | 179398 | 17167 | 54129 | 10321 | 16620 | 3166 | yes |
| ppt | 7 | 182144 | 25745 | 1923104 | 139631 | 57577 | 7883 | 25497 | 3564 | yes |
| pptx | 12 | 293143 | 22178 | 3645029 | 127760.0 | 455069 | 43646 | 48765 | 3521 | yes |
| rst | 6 | 158799 | 26086 | 397249 | 29476.5 | 66459 | 10194 | 18621 | 3057 | yes |
| rtf | 4 | 92386 | 24369 | 191974 | 15266.5 | 31477 | 10134 | 11893 | 2967 | yes |
| toml | 9 | 216216 | 23962 | 39681 | 4013 | 95018 | 9896 | 27711 | 2950 | yes |
| xls | 9 | 224805 | 24130 | 1279204 | 61291 | 87874 | 10141 | 30425 | 3076 | yes |
| xlsx | 14 | 296391 | 20864 | 1282259 | 30390.0 | 348689 | 29192 | 47113 | 3322 | yes |
| xml | 4 | 101908 | 25168 | 254734 | 41559.0 | 49084 | 11436 | 13674 | 3155 | yes |
| yaml | 11 | 268329 | 24264 | 195757 | 9179 | 116489 | 10468 | 36789 | 3209 | yes |

## By size class

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| large(>=256KiB) | 25 | 849533 | 32847 | 35931853 | 1146204 | 371559 | 14137 | 126061 | 4894 | yes |
| medium(<256KiB) | 86 | 2189510 | 24790 | 18741085 | 135221.0 | 1375837 | 10804 | 298294 | 3314 | yes |
| small(<16KiB) | 197 | 4763906 | 23802 | 3529161 | 11954 | 2369119 | 10137 | 613941 | 3018 | yes |

## By complexity

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| complex | 16 | 455874 | 25458 | 12213569 | 178642.5 | 276343 | 12626 | 66479 | 3778 | yes |
| hostile | 24 | 492951 | 19494 | 341455 | 6083.5 | 96974 | 752 | 69830 | 2874 | yes |
| moderate | 19 | 670875 | 24191 | 2368557 | 44661 | 381943 | 11872 | 65032 | 3267 | yes |
| nested | 114 | 2700571 | 23769 | 16825989 | 36496.0 | 1791019 | 10320 | 391477 | 3168 | yes |
| simple | 37 | 981406 | 24344 | 5698094 | 23143 | 483252 | 10608 | 130976 | 3218 | yes |
| structured | 98 | 2501272 | 24190 | 20754435 | 14515.5 | 1086984 | 10198 | 314502 | 3022 | yes |

## By stratum (real vs malformed/hostile)

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| hostile | 24 | 492951 | 19494 | 341455 | 6083.5 | 96974 | 752 | 69830 | 2874 | yes |
| real | 284 | 7309998 | 24110 | 57860644 | 30843.0 | 4019541 | 10368 | 968466 | 3154 | yes |

### Workload dimension

`narrow` = one `observe --metadata --kind metadata` per document (a bounded metadata projection).
`full` = one `materialize --exact --packed` per document (byte-authority closure).
Both are pooled and median; a size/time-weighted world is `pooled`, an unweighted per-document world is `median` (ADR-0054: a claim names its estimator).

**Detection.** 31/308 samples were detected as `opaque` by the bounded detector; 64 samples declined the narrow observation. The byte-authority gate is unaffected by detection; every sample is byte-exactly closed.

## Findings (detector choice != declared format)

A **finding** is a real sample whose detector choice differs from its declared format: a conservative `decline-opaque`, or a `misclassified` claim by another detector. Every finding is reconciled in the phase report (fixed-with-test, or recorded as a typed decline / recorded misclassification with its reason). The byte-authority closure is unaffected by any finding.

Summary by kind: decline-opaque=14, misclassified=18

| id | declared | detected | kind | resolution |
|---|---|---|---|---|
| arrow-stream-1.0.0-littleendian-generated_decimal256.stream | arrow | opaque | decline-opaque | recorded decline — a bare Arrow IPC *stream* (no leading `ARROW1` magic); the arrow detector admits file format (magic+footer) and magic-prefixed streams only (src/adapter/arrow.rs). The whole-source exact leaf is unaffected. |
| arrow-stream-1.0.0-littleendian-generated_nested_dictionary.stream | arrow | opaque | decline-opaque | recorded decline — a bare Arrow IPC *stream* (no leading `ARROW1` magic); the arrow detector admits file format (magic+footer) and magic-prefixed streams only (src/adapter/arrow.rs). The whole-source exact leaf is unaffected. |
| arrow-stream-1.0.0-littleendian-generated_null_trivial.stream | arrow | opaque | decline-opaque | recorded decline — a bare Arrow IPC *stream* (no leading `ARROW1` magic); the arrow detector admits file format (magic+footer) and magic-prefixed streams only (src/adapter/arrow.rs). The whole-source exact leaf is unaffected. |
| arrow-stream-cpp-21.0.0-generated_custom_metadata.stream | arrow | opaque | decline-opaque | recorded decline — a bare Arrow IPC *stream* (no leading `ARROW1` magic); the arrow detector admits file format (magic+footer) and magic-prefixed streams only (src/adapter/arrow.rs). The whole-source exact leaf is unaffected. |
| asciidoctor-CHANGELOG.adoc | asciidoc | rst | misclassified | recorded misclassification — the reST detector (tried before AsciiDoc; the precedence is load-bearing for real reST) claims attribute-only/locale/CHANGELOG AsciiDoc via its field-list/adornment signal (src/adapter/rst.rs, asciidoc.rs). Not reordered. |
| asciidoctor-cli-nav.adoc | asciidoc | opaque | decline-opaque | recorded decline — a `* xref:` link-list-only AsciiDoc page carries no AsciiDoc-specific block/title signal, so it stays Opaque rather than being guessed (src/adapter/asciidoc.rs). |
| asciidoctor-locale-attributes-ja.adoc | asciidoc | rst | misclassified | recorded misclassification — the reST detector (tried before AsciiDoc; the precedence is load-bearing for real reST) claims attribute-only/locale/CHANGELOG AsciiDoc via its field-list/adornment signal (src/adapter/rst.rs, asciidoc.rs). Not reordered. |
| asciidoctor-locale-attributes-sr.adoc | asciidoc | rst | misclassified | recorded misclassification — the reST detector (tried before AsciiDoc; the precedence is load-bearing for real reST) claims attribute-only/locale/CHANGELOG AsciiDoc via its field-list/adornment signal (src/adapter/rst.rs, asciidoc.rs). Not reordered. |
| asciidoctor-pages-macos.adoc | asciidoc | rst | misclassified | recorded misclassification — the reST detector (tried before AsciiDoc; the precedence is load-bearing for real reST) claims attribute-only/locale/CHANGELOG AsciiDoc via its field-list/adornment signal (src/adapter/rst.rs, asciidoc.rs). Not reordered. |
| asciidoctor-pages-stylesheet-modes.adoc | asciidoc | rst | misclassified | recorded misclassification — the reST detector (tried before AsciiDoc; the precedence is load-bearing for real reST) claims attribute-only/locale/CHANGELOG AsciiDoc via its field-list/adornment signal (src/adapter/rst.rs, asciidoc.rs). Not reordered. |
| cbor-data-integer.cbor | cbor | opaque | decline-opaque | recorded decline — small CBOR values (a lone integer, a tagged date) and a multi-value concatenation fall below the detector's ≥3-node/container/self-describe threshold; conservative by design (src/adapter/cbor.rs). |
| cbor-data-tagged_date.cbor | cbor | opaque | decline-opaque | recorded decline — small CBOR values (a lone integer, a tagged date) and a multi-value concatenation fall below the detector's ≥3-node/container/self-describe threshold; conservative by design (src/adapter/cbor.rs). |
| cbor-rfc8949-good.cbor | cbor | opaque | decline-opaque | recorded decline — small CBOR values (a lone integer, a tagged date) and a multi-value concatenation fall below the detector's ≥3-node/container/self-describe threshold; conservative by design (src/adapter/cbor.rs). |
| poi-document-Bug60942b.doc | doc | cfbf | misclassified | recorded decline — a genuine CFBF compound file whose `WordDocument` stream is present but whose Word FIB variant the legacy .doc detector does not claim, so it stays at the generic CFBF physical layer (src/adapter/doc.rs). |
| poi-document-testRangeDelete.doc | doc | cfbf | misclassified | recorded decline — a genuine CFBF compound file whose `WordDocument` stream is present but whose Word FIB variant the legacy .doc detector does not claim, so it stays at the generic CFBF physical layer (src/adapter/doc.rs). |
| cpython-msg-25.eml | eml | opaque | decline-opaque | recorded decline — a multi-message mbox fixture whose root `multipart/report` boundary the header scan cannot parse; the multipart-without-boundary guard declines rather than admit an unmodellable tree (src/adapter/eml.rs). |
| togeojson-data-blue_hills.gpx | gis | xml | misclassified | recorded decline — GPX 1.0 (not 1.1) and a namespace-less KML do not match the exact KML 2.2 / GPX 1.1 namespace URI the gis detector requires, so they stay Xml (src/adapter/gis.rs). |
| togeojson-data-style.kml | gis | xml | misclassified | recorded decline — GPX 1.0 (not 1.1) and a namespace-less KML do not match the exact KML 2.2 / GPX 1.1 namespace URI the gis detector requires, so they stay Xml (src/adapter/gis.rs). |
| loghub-HPC-HPC_2k.log | logstream | opaque | decline-opaque | recorded decline — HPC/Mac/Proxifier/Spark logs contain lines that do not match any dialect (RFC 5424/3164 or a leading timestamp/level), so the whole-file detector declines (src/adapter/logstream.rs). |
| loghub-Mac-Mac_2k.log | logstream | opaque | decline-opaque | recorded decline — HPC/Mac/Proxifier/Spark logs contain lines that do not match any dialect (RFC 5424/3164 or a leading timestamp/level), so the whole-file detector declines (src/adapter/logstream.rs). |
| loghub-OpenStack-OpenStack_2k.log | logstream | config | misclassified | recorded misclassification — the config detector (`KEY=VALUE` shape) claims an OpenStack log stream that the line-dialect log detector declines (src/adapter/logstream.rs, config.rs). |
| loghub-Proxifier-Proxifier_2k.log | logstream | opaque | decline-opaque | recorded decline — HPC/Mac/Proxifier/Spark logs contain lines that do not match any dialect (RFC 5424/3164 or a leading timestamp/level), so the whole-file detector declines (src/adapter/logstream.rs). |
| loghub-Spark-Spark_2k.log | logstream | opaque | decline-opaque | recorded decline — HPC/Mac/Proxifier/Spark logs contain lines that do not match any dialect (RFC 5424/3164 or a leading timestamp/level), so the whole-file detector declines (src/adapter/logstream.rs). |
| loghub-Windows-Windows_2k.log | logstream | csv | misclassified | recorded misclassification — the CSV detector claims a Windows event-style log whose lines have a consistent field count; the every-line-dialect log detector declines it (src/adapter/logstream.rs, csv.rs). |
| poi-slideshow-54332a.ppt | ppt | cfbf | misclassified | recorded decline — a genuine CFBF file whose `PowerPoint Document` stream is present but whose record variant the legacy .ppt detector does not claim, so it stays at the generic CFBF physical layer (src/adapter/ppt.rs). |
| poi-slideshow-basic_test_ppt_file.ppt | ppt | cfbf | misclassified | recorded decline — a genuine CFBF file whose `PowerPoint Document` stream is present but whose record variant the legacy .ppt detector does not claim, so it stays at the generic CFBF physical layer (src/adapter/ppt.rs). |
| poi-slideshow-datetime.ppt | ppt | cfbf | misclassified | recorded decline — a genuine CFBF file whose `PowerPoint Document` stream is present but whose record variant the legacy .ppt detector does not claim, so it stays at the generic CFBF physical layer (src/adapter/ppt.rs). |
| poi-Divino.pptx | pptx | opaque | decline-opaque | recorded decline — a valid OOXML deck whose OPC control-member decode declines (oversized/unsupported member), so detection stays Opaque (the whole-source exact leaf is unaffected). |
| cpython-whatsnew-3.3.rst | rst | markdown | misclassified | recorded misclassification — the reST detector declines a real CPython `whatsnew` document and the Markdown detector claims it via its structural marks (src/adapter/rst.rs, markdown.rs). |
| poi-spreadsheet-43493.xls | xls | cfbf | misclassified | recorded decline — a genuine CFBF file whose BIFF8 `Workbook`/`Book` stream is present but whose BIFF variant the legacy .xls detector does not claim, so it stays at the generic CFBF physical layer (src/adapter/xls.rs). |
| poi-spreadsheet-bug66319.xls | xls | cfbf | misclassified | recorded decline — a genuine CFBF file whose BIFF8 `Workbook`/`Book` stream is present but whose BIFF variant the legacy .xls detector does not claim, so it stays at the generic CFBF physical layer (src/adapter/xls.rs). |
| cpython-workflows-project-updater.yml | yaml | config | misclassified | recorded misclassification — a GitHub Actions workflow that the YAML detector declines is claimed by the config detector (`KEY: value`/`on:` shape) (src/adapter/yaml.rs, config.rs). |

Each finding is either **fixed with a focused regression test** or
**recorded** as a typed decline / recorded misclassification with its reason
(a *recorded* detector-boundary finding never changes the exact bytes; the
byte-authority gate is `materialize == source` for every sample). Detector
fixes landed in this phase: (1) the RTF detector now tolerates trailing
NUL/ASCII-whitespace padding after the balanced root group (real writers append
a final CRLF; Word appends a NUL) — regression test
`rtf::tests::trailing_padding_parses_and_keeps_the_root_group_span`;
(2) the EML detector/parser now decode RFC 2231 extended parameters
(`boundary*=charset'lang'value`), so real `multipart/signed` mail detects —
regression test `eml::tests::rfc2231_extended_boundary_is_decoded`.

No real sample was skipped.

### Per-sample

| id | declared | detected | stratum | size class | complexity | src B | store B | build us | narrow us | full us | exact |
|---|---|---|---|---|---|---:|---:|---:|---:|---:|---|
| arrow-file-1.0.0-bigendian-generated_decimal256.arrow_file | arrow | arrow | real | large(>=256KiB) | nested | 363106 | 729638 | 28932 | 6946 | 4523 | True |
| arrow-file-1.0.0-littleendian-generated_union.arrow_file | arrow | arrow | real | small(<16KiB) | nested | 3562 | 10550 | 23899 | 5774 | 3155 | True |
| arrow-file-cpp-21.0.0-generated_dictionary_unsigned.arrow_file | arrow | arrow | real | small(<16KiB) | nested | 2170 | 7766 | 23688 | 5320 | 2961 | True |
| arrow-file-cpp-21.0.0-generated_null_trivial.arrow_file | arrow | arrow | real | small(<16KiB) | nested | 514 | 4454 | 24230 | 5591 | 3004 | True |
| arrow-stream-1.0.0-littleendian-generated_decimal256.stream | arrow | opaque | real | large(>=256KiB) | nested | 360464 | 363676 | 34819 | 1621 | 5147 | True |
| arrow-stream-1.0.0-littleendian-generated_nested_dictionary.stream | arrow | opaque | real | small(<16KiB) | nested | 2632 | 5844 | 19216 | 789 | 2994 | True |
| arrow-stream-1.0.0-littleendian-generated_null_trivial.stream | arrow | opaque | real | small(<16KiB) | nested | 312 | 3524 | 18973 | 541 | 2716 | True |
| arrow-stream-cpp-21.0.0-generated_custom_metadata.stream | arrow | opaque | real | small(<16KiB) | nested | 1504 | 4716 | 18996 | 490 | 2796 | True |
| hostile2-magic-arrow | arrow | opaque | hostile | small(<16KiB) | hostile | 514 | 3726 | 18851 | 1729 | 2917 | True |
| asciidoctor-CHANGELOG.adoc | asciidoc | rst | real | medium(<256KiB) | structured | 140952 | 307359 | 31471 | 10614 | 3333 | True |
| asciidoctor-cli-nav.adoc | asciidoc | opaque | real | small(<16KiB) | structured | 222 | 3434 | 18816 | 533 | 2703 | True |
| asciidoctor-locale-attributes-ja.adoc | asciidoc | rst | real | small(<16KiB) | structured | 732 | 6089 | 23650 | 9774 | 2858 | True |
| asciidoctor-locale-attributes-sr.adoc | asciidoc | rst | real | small(<16KiB) | structured | 863 | 6641 | 23752 | 9836 | 2916 | True |
| asciidoctor-pages-contexts-ref.adoc | asciidoc | asciidoc | real | small(<16KiB) | structured | 3640 | 15247 | 24083 | 11035 | 2862 | True |
| asciidoctor-pages-macos.adoc | asciidoc | rst | real | small(<16KiB) | structured | 1577 | 8386 | 25283 | 9808 | 2876 | True |
| asciidoctor-pages-stylesheet-modes.adoc | asciidoc | rst | real | small(<16KiB) | structured | 7284 | 22583 | 24024 | 11404 | 4039 | True |
| bibtex-build-strings-base.bib | bib | bib | real | medium(<256KiB) | structured | 145131 | 545657 | 26791 | 11372 | 3349 | True |
| bibtex-strings.bib | bib | bib | real | large(>=256KiB) | structured | 271806 | 1025695 | 30764 | 13254 | 4227 | True |
| hostile2-trunc-bib | bib | opaque | hostile | small(<16KiB) | hostile | 800 | 4012 | 20261 | 515 | 2700 | True |
| cbor-data-all_types.cbor | cbor | cbor | real | small(<16KiB) | nested | 354 | 6300 | 23510 | 9826 | 2821 | True |
| cbor-data-integer.cbor | cbor | opaque | real | small(<16KiB) | nested | 2 | 3214 | 18988 | 566 | 2804 | True |
| cbor-data-tagged_date.cbor | cbor | opaque | real | small(<16KiB) | nested | 22 | 3234 | 18868 | 508 | 2720 | True |
| cbor-rfc8949-appendixA-mt2.cbor | cbor | cbor | real | small(<16KiB) | nested | 177 | 4742 | 23530 | 11558 | 3964 | True |
| cbor-rfc8949-appendixA-mt3.cbor | cbor | cbor | real | small(<16KiB) | nested | 487 | 6867 | 23559 | 9922 | 2806 | True |
| cbor-rfc8949-appendixA-streaming.cbor | cbor | cbor | real | small(<16KiB) | nested | 1145 | 13386 | 23388 | 9925 | 2946 | True |
| cbor-rfc8949-good.cbor | cbor | opaque | real | small(<16KiB) | nested | 13797 | 17009 | 19691 | 948 | 3090 | True |
| hostile2-trunc-cbor | cbor | opaque | hostile | small(<16KiB) | hostile | 40 | 3252 | 18713 | 489 | 2682 | True |
| poi-hpsf-TestVisioWithCodepage.vsd | cfbf | cfbf | real | medium(<256KiB) | nested | 45568 | 95579 | 24748 | 11580 | 3266 | True |
| poi-ole-poifs-only-zero-byte-streams.ole2 | cfbf | cfbf | real | small(<16KiB) | nested | 1536 | 7059 | 24113 | 10372 | 3180 | True |
| poi-publisher-LinkAt20.pub | cfbf | cfbf | real | medium(<256KiB) | nested | 64512 | 134355 | 24511 | 10071 | 2984 | True |
| poi-publisher-Sample_2010.pub | cfbf | cfbf | real | medium(<256KiB) | nested | 72704 | 150739 | 24942 | 10343 | 3403 | True |
| poi-publisher-clusterfuzz-testcase-minimized-POIHPBFFuzzer-4701121678278656.pub | cfbf | cfbf | real | large(>=256KiB) | nested | 331491 | 666405 | 28559 | 6612 | 4082 | True |
| cpython-libregrtest-mypy.ini | config | config | real | small(<16KiB) | simple | 997 | 7731 | 23724 | 10402 | 3163 | True |
| poi-gradle.properties | config | config | real | small(<16KiB) | simple | 555 | 5630 | 24071 | 10175 | 3069 | True |
| poi-resources-font_metrics.properties | config | config | real | medium(<256KiB) | simple | 146498 | 359532 | 29161 | 14831 | 4924 | True |
| cpython-namespace_pkgs-missing_directory.zip | container | container | real | small(<16KiB) | nested | 515 | 7457 | 20697 | 10562 | 3280 | True |
| cpython-test-recursion.tar | container | container | real | small(<16KiB) | nested | 516 | 4470 | 24963 | 5573 | 3188 | True |
| cpython-test-zipdir_backslash.zip | container | container | real | small(<16KiB) | nested | 192 | 6019 | 20617 | 10333 | 3125 | True |
| cpython-zipdata01-ziptestdata.zip | container | container | real | small(<16KiB) | nested | 876 | 9797 | 21034 | 10323 | 3114 | True |
| hostile2-trunc-container | container | opaque | hostile | small(<16KiB) | hostile | 5000 | 8212 | 19180 | 709 | 2869 | True |
| ne-zip-make-graticules-and-bounding-box-ne_1d5_graticules_beta2.zip | container | container | real | large(>=256KiB) | nested | 1000316 | 2055115 | 42112 | 14137 | 7230 | True |
| ne-zip-zip-ne_50m_graticules_1.zip | container | container | real | medium(<256KiB) | nested | 128416 | 264104 | 21883 | 10424 | 3296 | True |
| ne-zip-zip-ne_50m_graticules_15.zip | container | container | real | small(<16KiB) | nested | 10012 | 27300 | 20780 | 10874 | 4173 | True |
| ne-zip-zip-ne_50m_graticules_5.zip | container | container | real | medium(<256KiB) | nested | 27460 | 62192 | 20883 | 10106 | 3092 | True |
| poi-openxml4j-dcterms_bug_56479.zip | container | container | real | small(<16KiB) | nested | 1277 | 9934 | 21033 | 10299 | 3043 | True |
| parquet-delta-expect.csv | csv | csv | real | medium(<256KiB) | moderate | 159803 | 552658 | 27161 | 11544 | 3713 | True |
| vega-data-airports.csv | csv | csv | real | medium(<256KiB) | simple | 210363 | 893639 | 29054 | 13049 | 3942 | True |
| vega-data-co2-concentration.csv | csv | csv | real | medium(<256KiB) | simple | 18547 | 93286 | 28542 | 10785 | 3017 | True |
| vega-data-flights-airport.csv | csv | csv | real | medium(<256KiB) | simple | 65572 | 515711 | 25830 | 13279 | 4642 | True |
| vega-data-gapminder-health-income.csv | csv | csv | real | small(<16KiB) | simple | 8605 | 40460 | 24199 | 11251 | 3979 | True |
| vega-data-github.csv | csv | csv | real | medium(<256KiB) | simple | 21059 | 97252 | 25202 | 10717 | 3196 | True |
| vega-data-iowa-electricity.csv | csv | csv | real | small(<16KiB) | simple | 1531 | 10264 | 27840 | 14180 | 4782 | True |
| vega-data-population_engineers_hurricanes.csv | csv | csv | real | small(<16KiB) | simple | 1852 | 12779 | 34723 | 12491 | 3196 | True |
| vega-global-temp.csv | csv | csv | real | small(<16KiB) | simple | 1663 | 14668 | 24366 | 11853 | 4395 | True |
| vega-iowa-electricity.csv | csv | csv | real | small(<16KiB) | simple | 1531 | 10264 | 23828 | 9901 | 2987 | True |
| vega-population-engineers.csv | csv | csv | real | small(<16KiB) | simple | 1852 | 12779 | 24344 | 10157 | 3131 | True |
| vega-tsv-data-unemployment.tsv | csv | csv | real | medium(<256KiB) | simple | 34739 | 246812 | 26254 | 10894 | 3104 | True |
| hostile2-magic-doc | doc | opaque | hostile | medium(<256KiB) | hostile | 17920 | 21132 | 20241 | 912 | 2905 | True |
| poi-document-Bug41898.doc | doc | doc | real | medium(<256KiB) | nested | 24064 | 51548 | 26936 | 5737 | 3268 | True |
| poi-document-Bug46817.doc | doc | doc | real | medium(<256KiB) | nested | 17920 | 39260 | 24500 | 5767 | 4069 | True |
| poi-document-Bug53380_2.doc | doc | doc | real | medium(<256KiB) | nested | 31744 | 66908 | 24186 | 9010 | 3265 | True |
| poi-document-Bug60942b.doc | doc | cfbf | real | small(<16KiB) | nested | 6144 | 16721 | 23979 | 10769 | 3225 | True |
| poi-document-ThreeColHead.doc | doc | doc | real | medium(<256KiB) | nested | 27136 | 57692 | 24107 | 5395 | 2725 | True |
| poi-document-cap.stanford.edu_profiles_viewbiosketch_facultyid-4009-name-m_maciv | doc | doc | real | medium(<256KiB) | nested | 47104 | 97628 | 24657 | 5756 | 3369 | True |
| poi-document-parentinvguid.doc | doc | doc | real | large(>=256KiB) | nested | 398848 | 801116 | 30716 | 7212 | 4668 | True |
| poi-document-testRangeDelete.doc | doc | cfbf | real | medium(<256KiB) | nested | 104448 | 213341 | 26650 | 10973 | 3593 | True |
| hostile2-magic-docx | docx | opaque | hostile | small(<16KiB) | hostile | 10045 | 13257 | 19435 | 586 | 2879 | True |
| hostile2-trunc-docx | docx | docx | hostile | small(<16KiB) | hostile | 2297 | 14328 | 20449 | 33540 | 3307 | True |
| poi-document-58618.docx | docx | docx | real | small(<16KiB) | nested | 10045 | 52985 | 22079 | 43339 | 3319 | True |
| poi-document-60329.docx | docx | docx | real | medium(<256KiB) | nested | 31124 | 154025 | 22123 | 39886 | 3517 | True |
| poi-document-Bug51170.docx | docx | docx | real | medium(<256KiB) | nested | 239621 | 720218 | 24535 | 45626 | 4172 | True |
| poi-document-Bug60341.docx | docx | docx | real | small(<16KiB) | nested | 2297 | 14328 | 20874 | 28805 | 3851 | True |
| poi-document-NumberingWOverrides.docx | docx | docx | real | small(<16KiB) | nested | 15190 | 72123 | 21488 | 38286 | 3363 | True |
| poi-document-ThreeColHeadFoot.docx | docx | docx | real | small(<16KiB) | nested | 13123 | 66713 | 21792 | 40946 | 3382 | True |
| poi-document-capitalized.docx | docx | docx | real | small(<16KiB) | nested | 12104 | 73162 | 21361 | 41579 | 3657 | True |
| poi-document-checkboxes.docx | docx | docx | real | medium(<256KiB) | nested | 20800 | 108992 | 21663 | 39288 | 4135 | True |
| cpython-data-msg_08.txt | eml | eml | real | small(<16KiB) | structured | 454 | 5868 | 24081 | 10279 | 3030 | True |
| cpython-data-msg_22.txt | eml | eml | real | small(<16KiB) | structured | 1894 | 9580 | 23779 | 13959 | 2863 | True |
| cpython-data-msg_23.txt | eml | eml | real | small(<16KiB) | structured | 139 | 4246 | 23842 | 9970 | 2832 | True |
| cpython-data-msg_33.txt | eml | eml | real | small(<16KiB) | structured | 750 | 6266 | 23900 | 9968 | 2909 | True |
| cpython-data-msg_34.txt | eml | eml | real | small(<16KiB) | structured | 300 | 4725 | 24215 | 10587 | 3193 | True |
| cpython-data-msg_43.txt | eml | eml | real | small(<16KiB) | structured | 9166 | 24520 | 23893 | 10137 | 2990 | True |
| cpython-msg-01.eml | eml | eml | real | small(<16KiB) | simple | 459 | 5533 | 24048 | 10143 | 3203 | True |
| cpython-msg-25.eml | eml | opaque | real | small(<16KiB) | moderate | 5122 | 8334 | 242270 | 1238 | 3188 | True |
| cpython-msg-43.eml | eml | eml | real | small(<16KiB) | complex | 9166 | 24520 | 24478 | 12537 | 3385 | True |
| hostile2-magic-epub | epub | opaque | hostile | medium(<256KiB) | hostile | 30552 | 33764 | 20731 | 580 | 2954 | True |
| tika-test-documents-testEPUB.epub | epub | epub | real | medium(<256KiB) | nested | 30552 | 72075 | 20540 | 9735 | 2951 | True |
| tika-test-documents-testEPUB_xml_ext.epub | epub | epub | real | small(<16KiB) | nested | 7823 | 29942 | 20739 | 12340 | 3941 | True |
| feed-1.1-extensive-noerror.xml | feed | feed | real | small(<16KiB) | structured | 1705 | 13105 | 23971 | 10095 | 3012 | True |
| feed-1.1-extensive-nowarn.xml | feed | feed | real | small(<16KiB) | structured | 1747 | 13189 | 24533 | 9965 | 2981 | True |
| feed-a33-3.3-duplicate-updated.xml | feed | feed | real | small(<16KiB) | structured | 1015 | 9234 | 23955 | 9975 | 2925 | True |
| feed-a33-3.3-published_no_year.xml | feed | feed | real | small(<16KiB) | structured | 875 | 7971 | 23735 | 10205 | 3904 | True |
| feed-a33-3.3-published_seconds.xml | feed | feed | real | small(<16KiB) | structured | 895 | 8011 | 23802 | 9865 | 2788 | True |
| feed-a33-3.3-updated-example2.xml | feed | feed | real | small(<16KiB) | structured | 706 | 7505 | 23682 | 9758 | 2933 | True |
| hostile2-trunc-geojson | geojson | opaque | hostile | small(<16KiB) | hostile | 2000 | 5212 | 19330 | 511 | 2784 | True |
| natural-earth-countries.geojson | geojson | geojson | real | large(>=256KiB) | complex | 838726 | 4054747 | 45534 | 22655 | 6311 | True |
| natural-earth-places.geojson | geojson | geojson | real | medium(<256KiB) | moderate | 166071 | 830240 | 27919 | 14511 | 4095 | True |
| ne-bbox.geojson | geojson | geojson | real | small(<16KiB) | simple | 4813 | 34888 | 24231 | 10503 | 3392 | True |
| ne-geojson-ne_10m_admin_0_antarctic_claim_limit_lines.geojson | geojson | geojson | real | medium(<256KiB) | structured | 58099 | 431186 | 25510 | 12053 | 3362 | True |
| ne-geojson-ne_10m_bathymetry_A_10000.geojson | geojson | geojson | real | small(<16KiB) | structured | 2860 | 18223 | 24065 | 10123 | 2953 | True |
| ne-geojson-ne_10m_parks_and_protected_lands_scale_rank.geojson | geojson | geojson | real | medium(<256KiB) | structured | 202844 | 1032397 | 28912 | 13725 | 3660 | True |
| ne-geojson-ne_110m_admin_0_boundary_lines_land.geojson | geojson | geojson | real | large(>=256KiB) | structured | 340010 | 1722622 | 33450 | 14272 | 4053 | True |
| ne-geojson-ne_110m_admin_1_states_provinces_lakes.geojson | geojson | geojson | real | medium(<256KiB) | structured | 181275 | 868587 | 27891 | 12610 | 3725 | True |
| ne-geojson-ne_110m_glaciated_areas.geojson | geojson | geojson | real | medium(<256KiB) | structured | 21720 | 116257 | 24495 | 16180 | 3065 | True |
| ne-geojson-ne_50m_geography_regions_elevation_points.geojson | geojson | geojson | real | medium(<256KiB) | structured | 104139 | 438795 | 26502 | 11079 | 3290 | True |
| ne-lakes.geojson | geojson | geojson | real | small(<16KiB) | moderate | 7719 | 43208 | 24705 | 10561 | 3068 | True |
| ne-regions.geojson | geojson | geojson | real | small(<16KiB) | simple | 4099 | 19576 | 24335 | 10311 | 3450 | True |
| hostile2-trunc-gis | gis | opaque | hostile | small(<16KiB) | hostile | 200 | 3412 | 18853 | 711 | 2805 | True |
| togeojson-data-bad.gpx | gis | gis | real | small(<16KiB) | structured | 833 | 8679 | 23663 | 9922 | 2940 | True |
| togeojson-data-blue_hills.gpx | gis | xml | real | large(>=256KiB) | structured | 268694 | 2063850 | 39010 | 16200 | 4930 | True |
| togeojson-data-null_geometry.kml | gis | gis | real | small(<16KiB) | structured | 286 | 4889 | 23813 | 9806 | 2876 | True |
| togeojson-data-opacity_override.kml | gis | gis | real | small(<16KiB) | structured | 625 | 7617 | 23770 | 9864 | 2813 | True |
| togeojson-data-opencpn.gpx | gis | gis | real | small(<16KiB) | structured | 10637 | 66507 | 24396 | 10362 | 3053 | True |
| togeojson-data-point_extra_coordinates.kml | gis | gis | real | small(<16KiB) | structured | 386 | 5465 | 23643 | 9731 | 2796 | True |
| togeojson-data-schema.kml | gis | gis | real | small(<16KiB) | structured | 2158 | 16426 | 24002 | 10153 | 2877 | True |
| togeojson-data-style.kml | gis | xml | real | small(<16KiB) | structured | 1372 | 13386 | 25191 | 10515 | 3213 | True |
| togeojson-data-styles.kml | gis | gis | real | small(<16KiB) | structured | 6374 | 44998 | 24257 | 12958 | 2885 | True |
| hostile2-trunc-hdf | hdf | opaque | hostile | small(<16KiB) | hostile | 600 | 3812 | 18609 | 499 | 2735 | True |
| netcdf-h5-h5_test-ref_tst_h_compounds2.h5 | hdf | hdf | real | small(<16KiB) | nested | 557 | 4534 | 23549 | 5170 | 2866 | True |
| netcdf-h5-nczarr_test-ref_groups.h5 | hdf | hdf | real | small(<16KiB) | nested | 9836 | 23282 | 25723 | 9724 | 2914 | True |
| netcdf-ncdump-ref_nc_test_netcdf4_4_0.nc | hdf | hdf | real | medium(<256KiB) | nested | 162812 | 329044 | 25663 | 5756 | 3568 | True |
| netcdf-ncdump-ref_no_ncproperty.nc | hdf | hdf | real | small(<16KiB) | nested | 1262 | 5944 | 23229 | 5154 | 2897 | True |
| netcdf-ncdump-ref_test_365_day_1900.nc | hdf | hdf | real | small(<16KiB) | nested | 9348 | 22248 | 23918 | 11334 | 2918 | True |
| netcdf-ncdump-ref_tst_compounds2.nc | hdf | hdf | real | small(<16KiB) | nested | 5753 | 14926 | 23685 | 8943 | 2853 | True |
| netcdf-ncdump-ref_tst_compounds4.nc | hdf | hdf | real | small(<16KiB) | nested | 526 | 4472 | 23532 | 5193 | 2799 | True |
| netcdf-nctestfiles-test_vlen2.nc | hdf | hdf | real | small(<16KiB) | nested | 6336 | 16092 | 24067 | 5567 | 3065 | True |
| feedval-05-version_12_supports_atom_02.html | html | html | real | medium(<256KiB) | structured | 47054 | 110374 | 29970 | 10270 | 3127 | True |
| feedval-21-live.html | html | html | real | small(<16KiB) | structured | 2277 | 19579 | 23912 | 9867 | 2833 | True |
| feedval-22-known_bugs.html | html | html | real | small(<16KiB) | structured | 3478 | 24861 | 23721 | 9890 | 2843 | True |
| feedval-29-new_mailing_list_for_validator_users.html | html | html | real | small(<16KiB) | structured | 2823 | 21310 | 23753 | 9882 | 2833 | True |
| mdn-doc-structure.html | html | html | real | small(<16KiB) | moderate | 3525 | 20244 | 24389 | 10451 | 3124 | True |
| mdn-fonts-demo.html | html | html | real | medium(<256KiB) | complex | 36867 | 189800 | 29299 | 10731 | 3176 | True |
| mdn-getting-started.html | html | html | real | small(<16KiB) | simple | 224 | 5429 | 23818 | 10156 | 3079 | True |
| mdn-planets.html | html | html | real | small(<16KiB) | moderate | 4302 | 39076 | 25435 | 11239 | 3494 | True |
| cpython-fuzz_json_loads_corpus-empty_array.json | json | json | real | small(<16KiB) | structured | 3 | 3509 | 24088 | 10170 | 2923 | True |
| cpython-fuzz_json_loads_corpus-pass1.json | json | json | real | small(<16KiB) | structured | 1441 | 8910 | 24124 | 9976 | 2837 | True |
| cpython-fuzz_json_loads_corpus-simple_array.json | json | json | real | small(<16KiB) | structured | 25 | 3678 | 24378 | 9983 | 3041 | True |
| hostile2-magic-json | json | opaque | hostile | small(<16KiB) | hostile | 2743 | 5955 | 19427 | 556 | 2773 | True |
| vega-data-barley.json | json | json | real | small(<16KiB) | structured | 8487 | 47477 | 24247 | 10020 | 2921 | True |
| vega-data-budget.json | json | json | real | large(>=256KiB) | structured | 391353 | 1645334 | 35271 | 17590 | 4894 | True |
| vega-data-burtin.json | json | json | real | small(<16KiB) | structured | 2743 | 14189 | 24184 | 10255 | 3075 | True |
| vega-data-penguins.json | json | json | real | medium(<256KiB) | structured | 67119 | 266741 | 33342 | 12147 | 3195 | True |
| vega-data-volcano.json | json | json | real | medium(<256KiB) | structured | 21167 | 178662 | 25916 | 15757 | 3149 | True |
| vega-data-weekly-weather.json | json | json | real | small(<16KiB) | structured | 1281 | 12815 | 23728 | 13483 | 4158 | True |
| vega-data-world-110m.json | json | json | real | medium(<256KiB) | structured | 119410 | 1076373 | 34167 | 14155 | 3241 | True |
| mdx-js-renovate.json5 | json5 | json5 | real | small(<16KiB) | structured | 92 | 3852 | 23648 | 9979 | 2901 | True |
| jsonlines-datagov100.json | jsonl | jsonl | real | large(>=256KiB) | complex | 696007 | 2253927 | 37963 | 16845 | 5692 | True |
| jsonlines-datagov100.json | jsonl | jsonl | real | large(>=256KiB) | structured | 696007 | 2253927 | 38583 | 18481 | 5745 | True |
| oai-data-dbpedia_samples.jsonl | jsonl | jsonl | real | medium(<256KiB) | structured | 64512 | 167137 | 24592 | 10446 | 3142 | True |
| oai-data-drone_training.jsonl | jsonl | jsonl | real | large(>=256KiB) | structured | 387706 | 1837044 | 31656 | 15943 | 4359 | True |
| oai-data-example_requests_to_parallel_process.jsonl | jsonl | jsonl | real | large(>=256KiB) | structured | 548917 | 2831447 | 32847 | 22640 | 5159 | True |
| oai-data-toy_chat_fine_tuning.jsonl | jsonl | jsonl | real | medium(<256KiB) | structured | 27385 | 61273 | 24274 | 10045 | 2962 | True |
| oai-dbpedia.jsonl | jsonl | jsonl | real | medium(<256KiB) | moderate | 64512 | 167137 | 25759 | 11262 | 3596 | True |
| oai-parallel-requests.jsonl | jsonl | jsonl | real | large(>=256KiB) | complex | 548917 | 2831447 | 35812 | 23542 | 5929 | True |
| oai-toy-chat.jsonl | jsonl | jsonl | real | medium(<256KiB) | moderate | 27385 | 61273 | 24308 | 13532 | 4670 | True |
| loghub-HPC-HPC_2k.log | logstream | opaque | real | medium(<256KiB) | simple | 151178 | 154390 | 38118 | 1225 | 3291 | True |
| loghub-Mac-Mac_2k.log | logstream | opaque | real | large(>=256KiB) | simple | 319414 | 322626 | 37740 | 1723 | 4077 | True |
| loghub-OpenStack-OpenStack_2k.log | logstream | config | real | large(>=256KiB) | simple | 595119 | 1329738 | 39545 | 17474 | 5467 | True |
| loghub-Proxifier-Proxifier_2k.log | logstream | opaque | real | medium(<256KiB) | simple | 236962 | 240174 | 36552 | 1323 | 3828 | True |
| loghub-Spark-Spark_2k.log | logstream | opaque | real | medium(<256KiB) | simple | 196268 | 199480 | 33754 | 1863 | 4568 | True |
| loghub-Windows-Windows_2k.log | logstream | csv | real | large(>=256KiB) | simple | 285433 | 701911 | 28944 | 11728 | 4191 | True |
| commonmark-readme.md | markdown | markdown | real | small(<16KiB) | simple | 7671 | 23143 | 24604 | 10375 | 3105 | True |
| commonmark-spec.txt | markdown | markdown | real | medium(<256KiB) | complex | 205025 | 530161 | 37471 | 12243 | 3936 | True |
| cpython-ISSUE_TEMPLATE-crash.md | markdown | markdown | real | small(<16KiB) | simple | 1142 | 6380 | 23751 | 9744 | 2875 | True |
| cpython-rhel7-README.md | markdown | markdown | real | small(<16KiB) | simple | 608 | 5323 | 24091 | 10076 | 2987 | True |
| cpython-wasm-README.md | markdown | markdown | real | medium(<256KiB) | simple | 16560 | 51340 | 24831 | 10226 | 2940 | True |
| rust-readme.md | markdown | markdown | real | small(<16KiB) | simple | 3137 | 14075 | 24183 | 10183 | 3138 | True |
| mdx-blog-conf.mdx | mdx | mdx | real | small(<16KiB) | structured | 2790 | 12397 | 23719 | 9738 | 2724 | True |
| mdx-blog-v1.mdx | mdx | mdx | real | small(<16KiB) | structured | 9150 | 33026 | 24720 | 10492 | 3175 | True |
| mdx-community-sponsor.mdx | mdx | mdx | real | small(<16KiB) | structured | 5513 | 15451 | 24276 | 10364 | 3083 | True |
| mdx-docs-404.mdx | mdx | mdx | real | small(<16KiB) | structured | 545 | 5246 | 23676 | 9770 | 2853 | True |
| mdx-docs-getting-started.mdx | mdx | mdx | real | medium(<256KiB) | structured | 26777 | 87116 | 27608 | 10326 | 3075 | True |
| mdx-guides-embed.mdx | mdx | mdx | real | small(<16KiB) | structured | 3652 | 13099 | 23733 | 12121 | 3859 | True |
| mdx-packages-index.mdx | mdx | mdx | real | small(<16KiB) | structured | 995 | 6389 | 25595 | 9704 | 2779 | True |
| hostile2-trunc-mhtml | mhtml | mhtml | hostile | small(<16KiB) | hostile | 1000 | 7936 | 23507 | 10029 | 3030 | True |
| tika-test-documents-testMHTMLFirefox.mhtml | mhtml | mhtml | real | small(<16KiB) | nested | 13759 | 30944 | 23862 | 5196 | 3009 | True |
| tika-test-documents-testUserDefinedCharset.mhtml | mhtml | mhtml | real | small(<16KiB) | nested | 621 | 6217 | 23594 | 9704 | 2897 | True |
| hostile2-trunc-msg | msg | opaque | hostile | small(<16KiB) | hostile | 1500 | 4712 | 18804 | 628 | 2878 | True |
| poi-hsmf-ASCII_CP1251_LCID1049.msg | msg | msg | real | small(<16KiB) | nested | 3584 | 11471 | 24162 | 10429 | 4054 | True |
| poi-hsmf-attachment_msg_inlineImg.msg | msg | msg | real | medium(<256KiB) | nested | 147456 | 298332 | 25802 | 6069 | 3682 | True |
| poi-hsmf-example_sent_regular.msg | msg | msg | real | medium(<256KiB) | nested | 53248 | 109916 | 24294 | 5378 | 3038 | True |
| poi-hsmf-logsat.com_signatures_valid.msg | msg | msg | real | medium(<256KiB) | nested | 23040 | 49500 | 24465 | 5350 | 2864 | True |
| poi-hsmf-message_1981.msg | msg | msg | real | medium(<256KiB) | nested | 29696 | 62812 | 24547 | 7470 | 3051 | True |
| poi-hsmf-simple_test_msg.msg | msg | msg | real | medium(<256KiB) | nested | 16896 | 37212 | 24340 | 5722 | 3007 | True |
| hostile2-trunc-notebook | notebook | notebook | hostile | small(<16KiB) | hostile | 355 | 5075 | 23755 | 9795 | 3304 | True |
| jupyter-Notebook-Custom-Keyboard-Shortcuts.ipynb | notebook | notebook | real | small(<16KiB) | structured | 1148 | 7493 | 23511 | 9793 | 2818 | True |
| jupyter-Notebook-Importing-Notebooks.ipynb | notebook | notebook | real | small(<16KiB) | structured | 13552 | 47792 | 24009 | 9948 | 3025 | True |
| jupyter-Notebook-Running-Code.ipynb | notebook | notebook | real | medium(<256KiB) | structured | 52012 | 133052 | 25107 | 10331 | 3171 | True |
| jupyter-Notebook-What-is-the-Jupyter-Notebook.ipynb | notebook | notebook | real | small(<16KiB) | structured | 7541 | 24181 | 23591 | 10064 | 2917 | True |
| jupyter-nbs-other.ipynb | notebook | notebook | real | small(<16KiB) | structured | 786 | 6744 | 23818 | 9775 | 2869 | True |
| jupyter-notebooks-simple.ipynb | notebook | notebook | real | small(<16KiB) | structured | 355 | 5075 | 23601 | 9831 | 3213 | True |
| jupyter-notebooks-simple_toc.ipynb | notebook | notebook | real | small(<16KiB) | structured | 1040 | 7873 | 23508 | 9772 | 2829 | True |
| odfpy-cols.odp | odp | odp | real | medium(<256KiB) | moderate | 18037 | 148305 | 21166 | 43046 | 3150 | True |
| odfpy-emb.odp | odp | odp | real | medium(<256KiB) | complex | 16403 | 106233 | 21770 | 39316 | 3251 | True |
| odfpy-examples-cols.odp | odp | odp | real | medium(<256KiB) | nested | 18037 | 148305 | 20699 | 39357 | 3222 | True |
| odfpy-examples-ol.odp | odp | odp | real | small(<16KiB) | nested | 14940 | 138597 | 20762 | 44670 | 3073 | True |
| odfpy-ol.odp | odp | odp | real | small(<16KiB) | simple | 14940 | 138597 | 21264 | 46244 | 3252 | True |
| odfpy-chinese.ods | ods | ods | real | small(<16KiB) | moderate | 11765 | 44661 | 22884 | 24670 | 3080 | True |
| odfpy-empty.ods | ods | ods | real | small(<16KiB) | simple | 6819 | 29699 | 20850 | 29167 | 3265 | True |
| odfpy-examples-chinese_spreadsheet.ods | ods | ods | real | small(<16KiB) | nested | 11765 | 44661 | 21253 | 24254 | 3096 | True |
| odfpy-examples-empty.ods | ods | ods | real | small(<16KiB) | nested | 6819 | 29699 | 20609 | 24309 | 3081 | True |
| odfpy-examples-pythagoras-kspread.ods | ods | ods | real | small(<16KiB) | nested | 5037 | 22024 | 20900 | 24505 | 3050 | True |
| odfpy-examples-pythagoras.ods | ods | ods | real | small(<16KiB) | nested | 6837 | 30406 | 20542 | 28863 | 3086 | True |
| odfpy-pyth-kspread.ods | ods | ods | real | small(<16KiB) | simple | 5037 | 22024 | 20786 | 24222 | 3165 | True |
| odfpy-pythagoras.ods | ods | ods | real | small(<16KiB) | simple | 6837 | 30406 | 20810 | 24694 | 3174 | True |
| hostile2-trunc-odt | odt | opaque | hostile | small(<16KiB) | hostile | 3000 | 6212 | 19553 | 793 | 2922 | True |
| odfpy-examples-definitionlists.odt | odt | odt | real | small(<16KiB) | nested | 7824 | 32070 | 20205 | 27798 | 2941 | True |
| odfpy-examples-nasty.odt | odt | odt | real | small(<16KiB) | nested | 1539 | 11051 | 20376 | 19301 | 2982 | True |
| odfpy-examples-no_userfields.odt | odt | odt | real | medium(<256KiB) | nested | 19228 | 52978 | 20827 | 29461 | 3338 | True |
| odfpy-examples-parastyles.odt | odt | odt | real | small(<16KiB) | nested | 7194 | 32845 | 20382 | 23703 | 2967 | True |
| odfpy-examples-serious_poem.odt | odt | odt | real | small(<16KiB) | nested | 8539 | 31745 | 20584 | 24470 | 3211 | True |
| odfpy-examples-simpletable.odt | odt | odt | real | small(<16KiB) | nested | 7055 | 31250 | 20868 | 23948 | 2866 | True |
| odfpy-examples-userfields_ooo3.odt | odt | odt | real | medium(<256KiB) | nested | 38420 | 98623 | 21434 | 24602 | 3507 | True |
| hostile2-trunc-parquet | parquet | opaque | hostile | small(<16KiB) | hostile | 800 | 4012 | 18720 | 522 | 2713 | True |
| parquet-alltypes-plain.parquet | parquet | parquet | real | small(<16KiB) | simple | 1851 | 9203 | 24330 | 10305 | 3218 | True |
| parquet-alltypes-tiny.parquet | parquet | parquet | real | large(>=256KiB) | complex | 454233 | 914573 | 30612 | 12026 | 4880 | True |
| parquet-data-datapage_v2.snappy.parquet | parquet | parquet | real | small(<16KiB) | nested | 1165 | 6792 | 23992 | 9981 | 2941 | True |
| parquet-data-fixed_length_byte_array.parquet | parquet | parquet | real | small(<16KiB) | nested | 4437 | 12708 | 23873 | 10466 | 3030 | True |
| parquet-data-fixed_length_decimal.parquet | parquet | parquet | real | small(<16KiB) | nested | 677 | 5166 | 24103 | 10070 | 3029 | True |
| parquet-data-float16_nonzeros_and_nans.parquet | parquet | parquet | real | small(<16KiB) | nested | 501 | 4788 | 23850 | 10098 | 4072 | True |
| parquet-data-lz4_raw_compressed_larger.parquet | parquet | parquet | real | large(>=256KiB) | nested | 380836 | 765507 | 28503 | 11274 | 4436 | True |
| parquet-data-nation.dict-malformed.parquet | parquet | parquet | real | small(<16KiB) | nested | 2850 | 9855 | 24023 | 10139 | 2980 | True |
| parquet-delta-binary.parquet | parquet | parquet | real | medium(<256KiB) | complex | 72971 | 161889 | 25020 | 10559 | 3208 | True |
| parquet-geospatial-geography-points.parquet | parquet | parquet | real | medium(<256KiB) | nested | 33026 | 81965 | 24158 | 10423 | 3374 | True |
| asciidoctor-fixtures-hello-asciidoctor.pdf | pdf | pdf | real | small(<16KiB) | nested | 847 | 6557 | 23434 | 603 | 2744 | True |
| cpython-library-turtle-star.pdf | pdf | pdf | real | small(<16KiB) | nested | 4418 | 12438 | 27203 | 951 | 3301 | True |
| netcdf-docs-docmap.pdf | pdf | pdf | real | medium(<256KiB) | nested | 64347 | 78695 | 25396 | 646 | 2969 | True |
| cargo-cargo.toml | pkgmeta | pkgmeta | real | small(<16KiB) | complex | 7717 | 78906 | 24555 | 13218 | 4342 | True |
| jupyter-package.json | pkgmeta | pkgmeta | real | small(<16KiB) | structured | 3448 | 17137 | 23890 | 9845 | 2803 | True |
| jupyter-py-pyproject.toml | pkgmeta | pkgmeta | real | small(<16KiB) | structured | 7903 | 50514 | 24021 | 10462 | 3194 | True |
| serde-cargo.toml | pkgmeta | pkgmeta | real | small(<16KiB) | moderate | 2431 | 17167 | 24191 | 10321 | 3115 | True |
| vega-package.json | pkgmeta | pkgmeta | real | small(<16KiB) | structured | 2497 | 15674 | 24114 | 10283 | 3166 | True |
| hostile2-trunc-ppt | ppt | cfbf | hostile | small(<16KiB) | hostile | 8000 | 19423 | 24092 | 5896 | 2865 | True |
| poi-slideshow-42474-2.ppt | ppt | ppt | real | large(>=256KiB) | nested | 571392 | 1146204 | 31637 | 7883 | 5470 | True |
| poi-slideshow-54332a.ppt | ppt | cfbf | real | medium(<256KiB) | nested | 67584 | 139631 | 25745 | 10862 | 3564 | True |
| poi-slideshow-Single_Coloured_Page.ppt | ppt | ppt | real | small(<16KiB) | nested | 9728 | 22876 | 24380 | 5609 | 3103 | True |
| poi-slideshow-basic_test_ppt_file.ppt | ppt | cfbf | real | small(<16KiB) | nested | 15360 | 35167 | 24071 | 10036 | 2992 | True |
| poi-slideshow-bug62092.ppt | ppt | ppt | real | medium(<256KiB) | nested | 161792 | 327004 | 26364 | 6468 | 3787 | True |
| poi-slideshow-datetime.ppt | ppt | cfbf | real | medium(<256KiB) | nested | 114176 | 232799 | 25855 | 10823 | 3716 | True |
| hostile2-trunc-pptx | pptx | opaque | hostile | medium(<256KiB) | hostile | 120000 | 123212 | 21367 | 1178 | 3479 | True |
| poi-61515.pptx | pptx | pptx | real | medium(<256KiB) | moderate | 29270 | 99853 | 21882 | 43262 | 3531 | True |
| poi-Divino.pptx | pptx | opaque | real | large(>=256KiB) | complex | 523999 | 527211 | 28254 | 2009 | 5026 | True |
| poi-SampleShow.pptx | pptx | pptx | real | medium(<256KiB) | moderate | 39083 | 132308 | 23304 | 42780 | 3071 | True |
| poi-bug60993.pptx | pptx | pptx | real | medium(<256KiB) | complex | 21942 | 95612 | 22072 | 43088 | 3241 | True |
| poi-slideshow-49386-null_dates.pptx | pptx | pptx | real | medium(<256KiB) | nested | 17124 | 62331 | 21699 | 45526 | 5119 | True |
| poi-slideshow-au.asn.aes.www_conferences_2011_presentations_Fri_20Room4Level4_20 | pptx | pptx | real | large(>=256KiB) | nested | 536086 | 1622797 | 42851 | 47044 | 6415 | True |
| poi-slideshow-bug58144-headers-footers-2007.pptx | pptx | pptx | real | medium(<256KiB) | nested | 42035 | 136662 | 22337 | 44029 | 3511 | True |
| poi-slideshow-bug62736.pptx | pptx | pptx | real | medium(<256KiB) | nested | 50926 | 164564 | 22285 | 48275 | 3390 | True |
| poi-slideshow-bug65551.pptx | pptx | pptx | real | medium(<256KiB) | nested | 34383 | 110096 | 21557 | 45704 | 4588 | True |
| poi-slideshow-pptx2svg.pptx | pptx | pptx | real | medium(<256KiB) | nested | 149172 | 467022 | 24010 | 49414 | 3976 | True |
| poi-table_test.pptx | pptx | pptx | real | medium(<256KiB) | moderate | 28935 | 103361 | 21525 | 42760 | 3418 | True |
| cpython-faq-gui.rst | rst | rst | real | small(<16KiB) | structured | 3058 | 11685 | 27044 | 15098 | 2904 | True |
| cpython-library-asyncio-protocol.rst | rst | rst | real | medium(<256KiB) | structured | 32975 | 83009 | 29020 | 10230 | 3121 | True |
| cpython-library-mmap.rst | rst | rst | real | small(<16KiB) | structured | 14634 | 35272 | 25129 | 9957 | 3222 | True |
| cpython-library-python.rst | rst | rst | real | small(<16KiB) | structured | 502 | 4725 | 24201 | 10158 | 2993 | True |
| cpython-library-sunau.rst | rst | rst | real | small(<16KiB) | structured | 7423 | 23681 | 24359 | 9915 | 2826 | True |
| cpython-whatsnew-3.3.rst | rst | markdown | real | medium(<256KiB) | structured | 99150 | 238877 | 29046 | 11101 | 3555 | True |
| hostile2-trunc-rtf | rtf | opaque | hostile | small(<16KiB) | hostile | 1000 | 4212 | 18801 | 696 | 2767 | True |
| ne-10m_cultural-10m_us_parks_-READ_ME.rtf | rtf | rtf | real | small(<16KiB) | structured | 2058 | 11954 | 24601 | 10135 | 3018 | True |
| ne-10m_parks_and_protected_areas-10m_us_parks_READ_ME.rtf | rtf | rtf | real | small(<16KiB) | structured | 3718 | 18579 | 24137 | 10134 | 2916 | True |
| poi-quick-contents-message.rtf | rtf | rtf | real | medium(<256KiB) | structured | 25528 | 157229 | 24847 | 10512 | 3192 | True |
| hostile2-magic-toml | toml | toml | hostile | small(<16KiB) | hostile | 37 | 4013 | 23671 | 9896 | 2828 | True |
| toml-array-string-quote-comma-02.toml | toml | toml | real | small(<16KiB) | structured | 20 | 3692 | 23470 | 12502 | 2867 | True |
| toml-datetime-local.toml | toml | toml | real | small(<16KiB) | structured | 88 | 3951 | 25526 | 9725 | 2953 | True |
| toml-key-escapes.toml | toml | toml | real | small(<16KiB) | structured | 163 | 4593 | 24147 | 12992 | 4304 | True |
| toml-spec-1.0.0-float-2.toml | toml | toml | real | small(<16KiB) | structured | 276 | 4701 | 24197 | 10193 | 3056 | True |
| toml-spec-1.1.0-common-0.toml | toml | toml | real | small(<16KiB) | structured | 122 | 3969 | 24083 | 10213 | 3278 | True |
| toml-string-basic-escape-03.toml | toml | toml | real | small(<16KiB) | structured | 56 | 3739 | 23692 | 9742 | 2698 | True |
| toml-valid-implicit-and-explicit-before.toml | toml | toml | real | small(<16KiB) | structured | 37 | 4013 | 23468 | 9869 | 2777 | True |
| toml-valid-multibyte.toml | toml | toml | real | small(<16KiB) | structured | 1156 | 7010 | 23962 | 9886 | 2950 | True |
| hostile2-trunc-xls | xls | cfbf | hostile | small(<16KiB) | hostile | 1500 | 6423 | 23587 | 5367 | 2950 | True |
| poi-spreadsheet-43493.xls | xls | cfbf | real | medium(<256KiB) | nested | 66048 | 136087 | 24354 | 13167 | 4629 | True |
| poi-spreadsheet-47251_1.xls | xls | xls | real | medium(<256KiB) | nested | 26112 | 102820 | 26271 | 10316 | 3132 | True |
| poi-spreadsheet-48325.xls | xls | xls | real | small(<16KiB) | nested | 4096 | 15039 | 24261 | 10141 | 3076 | True |
| poi-spreadsheet-49524.xls | xls | xls | real | small(<16KiB) | nested | 13824 | 31068 | 24016 | 6492 | 2971 | True |
| poi-spreadsheet-50298.xls | xls | xls | real | medium(<256KiB) | nested | 20480 | 61291 | 24130 | 10433 | 3115 | True |
| poi-spreadsheet-DStar.xls | xls | xls | real | medium(<256KiB) | nested | 35328 | 104549 | 23902 | 9857 | 2864 | True |
| poi-spreadsheet-EmbeddedChartHeaderTest.xls | xls | xls | real | small(<16KiB) | nested | 15360 | 43754 | 23881 | 10134 | 3037 | True |
| poi-spreadsheet-bug66319.xls | xls | cfbf | real | large(>=256KiB) | nested | 387072 | 778173 | 30403 | 11967 | 4651 | True |
| hostile2-trunc-xlsx | xlsx | opaque | hostile | small(<16KiB) | hostile | 15000 | 18212 | 19279 | 900 | 2931 | True |
| poi-47889.xlsx | xlsx | xlsx | real | small(<16KiB) | simple | 3555 | 19151 | 20941 | 36344 | 3154 | True |
| poi-49609.xlsx | xlsx | xlsx | real | medium(<256KiB) | complex | 105424 | 218879 | 21952 | 5975 | 3620 | True |
| poi-59021.xlsx | xlsx | xlsx | real | small(<16KiB) | moderate | 1933 | 12665 | 20822 | 30000 | 3114 | True |
| poi-DataValidation.xlsx | xlsx | xlsx | real | small(<16KiB) | moderate | 3286 | 18607 | 20907 | 28649 | 3267 | True |
| poi-linkext.xlsx | xlsx | xlsx | real | small(<16KiB) | complex | 3274 | 18583 | 20423 | 28416 | 2807 | True |
| poi-spreadsheet-123233_charts.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 12957 | 47188 | 21740 | 29706 | 3441 | True |
| poi-spreadsheet-56511.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 9688 | 35780 | 20164 | 28978 | 3377 | True |
| poi-spreadsheet-58731.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 9023 | 33113 | 20279 | 31926 | 4023 | True |
| poi-spreadsheet-59021.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 1933 | 12665 | 20633 | 33142 | 3037 | True |
| poi-spreadsheet-StructuredRefs-lots-with-lookups.xlsx | xlsx | xlsx | real | large(>=256KiB) | nested | 325446 | 687123 | 25016 | 30481 | 4499 | True |
| poi-spreadsheet-TwoSheetsOneHidden.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 7962 | 30038 | 20794 | 28728 | 2978 | True |
| poi-spreadsheet-WithChartSheet.xlsx | xlsx | xlsx | real | medium(<256KiB) | nested | 30333 | 99513 | 21960 | 29405 | 3464 | True |
| poi-spreadsheet-duplicate-filename-case-insensitive.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 10721 | 30742 | 21481 | 6039 | 3401 | True |
| maven-core-pom.xml | xml | xml | real | small(<16KiB) | moderate | 7411 | 46651 | 23964 | 10049 | 2898 | True |
| maven-pom.xml | xml | xml | real | medium(<256KiB) | complex | 28489 | 167485 | 25895 | 12715 | 4466 | True |
| poi-quick-contents-quick.xml | xml | xml | real | small(<16KiB) | structured | 143 | 4131 | 27607 | 16162 | 3307 | True |
| poi-spreadsheet-SampleSS.xml | xml | xml | real | small(<16KiB) | structured | 5115 | 36467 | 24442 | 10158 | 3003 | True |
| alertmanager-ci.yml | yaml | yaml | real | small(<16KiB) | simple | 1348 | 9179 | 24592 | 10608 | 3247 | True |
| alertmanager-simple.yml | yaml | yaml | real | small(<16KiB) | moderate | 3953 | 16344 | 24264 | 10196 | 3421 | True |
| cpython-workflows-build.yml | yaml | yaml | real | medium(<256KiB) | structured | 19106 | 65679 | 24951 | 10488 | 3068 | True |
| cpython-workflows-project-updater.yml | yaml | config | real | small(<16KiB) | structured | 741 | 7022 | 25225 | 10204 | 3058 | True |
| cpython-workflows-require-pr-label.yml | yaml | yaml | real | small(<16KiB) | structured | 507 | 5822 | 23941 | 9946 | 2970 | True |
| cpython-workflows-reusable-ubuntu.yml | yaml | yaml | real | small(<16KiB) | structured | 3356 | 14842 | 24447 | 11671 | 4431 | True |
| cpython-workflows-reusable-windows.yml | yaml | yaml | real | small(<16KiB) | structured | 1157 | 7847 | 24240 | 10449 | 3130 | True |
| hostile2-trunc-yaml | yaml | yaml | hostile | small(<16KiB) | hostile | 4000 | 17939 | 23735 | 9937 | 2853 | True |
| prometheus-ci.yml | yaml | yaml | real | small(<16KiB) | complex | 10201 | 39596 | 24764 | 10468 | 3209 | True |
| prometheus-web-config.yml | yaml | yaml | real | small(<16KiB) | simple | 531 | 5022 | 24150 | 10650 | 3383 | True |
| prometheus.yml | yaml | yaml | real | small(<16KiB) | moderate | 934 | 6465 | 24020 | 11872 | 4019 | True |

## Scope (honest)

- **A larger stratified public-sample population, still not a random sample
  of the world.** Samples are pinned by tag/commit and SHA-256 from stable,
  permissively licensed public sources (Apache POI, odfpy, vega-datasets,
  Natural Earth, parquet-testing, arrow-testing, CPython, toml-test,
  asciidoctor, mdx-js, jupyter, libcbor, OpenAPI-Specification, feedvalidator,
  togeojson, netcdf-c, Apache Tika, bibtex-parser, loghub, jsonlines,
  openai-cookbook); the bytes are gitignored and re-verified on every
  acquisition. Rows whose source license is unclear are marked
  `redistributable=false` (only length + SHA-256 are pinned). It is
  stratified, not an unbiased distribution.
- **The declared format is the sample's true/most-specific format.** Where a
  v1 control row was labelled with its coarse physical encoding, v2 declares
  the genuinely more-specific format (GeoJSON over JSON, package-manifest over
  TOML); that is curation, not a detection finding.
- **Complexity is a curated label; size class is measured.** Size class comes
  from the byte length; the complexity stratum is a curation label stated in
  the manifest.
- **A malformed/hostile stratum is included.** It is derived deterministically
  from pinned real bytes (truncation, wrong-magic, dropped tail), pinned by
  SHA-256 in `tools/realcorpus/hostile-strata-v2.tsv`.
- **Only byte-exact closure is a byte-authority claim.** Each sample's bytes
  are built from a scratch copy that is then deleted, and a fresh process must
  reproduce `length + SHA-256 + cmp`.
- **A finding is not a failure of authority.** A detector decline/misclass is
  recorded and reconciled; it never changes the exact bytes.
- **Missing samples are SKIPPED/BLOCKED, never fabricated.** Nothing here is
  run on the host; the acquisition ran in the capped `realcorpus` lane and the
  court in the capped `analytical` lane.
