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
| 308 | 284 | 24 | 0 | 0 | 308 | 268 | 21 | 41 |

By declared format (ran): arrow=9, asciidoc=7, bib=3, cbor=8, cfbf=5, config=3, container=10, csv=12, doc=9, docx=10, eml=9, epub=3, feed=6, geojson=13, gis=10, hdf=9, html=8, json=11, json5=1, jsonl=9, logstream=6, markdown=6, mdx=7, mhtml=3, msg=7, notebook=8, odp=5, ods=8, odt=8, parquet=11, pdf=3, pkgmeta=5, ppt=7, pptx=12, rst=6, rtf=4, toml=9, xls=9, xlsx=14, xml=4, yaml=11

## Stratified tables

## By declared format

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| arrow | 9 | 219927 | 24010 | 1519711 | 8335 | 94624 | 10494 | 30190 | 3114 | yes |
| asciidoc | 7 | 168261 | 23959 | 369739 | 8386 | 65082 | 10182 | 22973 | 3007 | yes |
| bib | 3 | 75797 | 27302 | 1575364 | 545657 | 24122 | 11069 | 10247 | 3513 | yes |
| cbor | 8 | 169856 | 21354 | 58004 | 5521.0 | 42497 | 5316 | 22720 | 2832 | yes |
| cfbf | 5 | 126894 | 24478 | 1054137 | 134355 | 50197 | 10244 | 16921 | 3185 | yes |
| config | 3 | 77572 | 24717 | 372893 | 7731 | 33696 | 11072 | 10656 | 3480 | yes |
| container | 10 | 226478 | 20374 | 2454600 | 9865.5 | 91575 | 10018 | 33263 | 2986 | yes |
| csv | 12 | 2002559 | 25772 | 2500572 | 66873.0 | 1000346 | 11080 | 212014 | 3244 | yes |
| doc | 9 | 218747 | 24073 | 1365346 | 57692 | 55441 | 5658 | 28553 | 2980 | yes |
| docx | 10 | 214858 | 20648 | 1290131 | 69418.0 | 331626 | 37735 | 31239 | 3078 | yes |
| eml | 9 | 402318 | 23750 | 93592 | 6266 | 84766 | 10061 | 29862 | 2972 | yes |
| epub | 3 | 62815 | 21017 | 135781 | 33764 | 21492 | 10164 | 9116 | 3015 | yes |
| feed | 6 | 142591 | 23720 | 59015 | 8622.5 | 63445 | 10063 | 17404 | 2906 | yes |
| geojson | 13 | 2276218 | 27834 | 9615938 | 431186 | 1235910 | 14307 | 261242 | 3890 | yes |
| gis | 10 | 253962 | 24296 | 2240691 | 11066.0 | 99511 | 10230 | 33224 | 3110 | yes |
| hdf | 9 | 210294 | 23667 | 424354 | 14926 | 53581 | 5367 | 27424 | 2952 | yes |
| html | 8 | 610663 | 74590 | 430673 | 23085.5 | 250964 | 28174 | 64318 | 7897 | yes |
| json | 11 | 282144 | 23840 | 3263643 | 14189 | 111079 | 10182 | 36042 | 3001 | yes |
| json5 | 1 | 23532 | 23532 | 3852 | 3852 | 9938 | 9938 | 2992 | 2992 | yes |
| jsonl | 9 | 317563 | 34189 | 12464612 | 1837044 | 144737 | 15963 | 44301 | 5355 | yes |
| logstream | 6 | 201328 | 34136 | 2948319 | 281400.0 | 30873 | 1475 | 25387 | 4071 | yes |
| markdown | 6 | 1399340 | 209011 | 630422 | 18609.0 | 799461 | 111895 | 265861 | 24566 | yes |
| mdx | 7 | 170984 | 23998 | 172724 | 13099 | 71754 | 10230 | 22705 | 3069 | yes |
| mhtml | 3 | 71781 | 23892 | 45097 | 7936 | 25870 | 9991 | 9123 | 3022 | yes |
| msg | 7 | 166773 | 24136 | 573955 | 49500 | 38632 | 5561 | 21404 | 2978 | yes |
| notebook | 8 | 196968 | 24356 | 237285 | 7683.0 | 86644 | 10539 | 26102 | 3094 | yes |
| odp | 5 | 935318 | 93123 | 680037 | 138597 | 2448249 | 710143 | 170505 | 33401 | yes |
| ods | 8 | 1684328 | 192740 | 253580 | 30052.5 | 1808654 | 212646 | 203589 | 23247 | yes |
| odt | 8 | 159934 | 20064 | 296774 | 31907.5 | 165859 | 23643 | 23025 | 2856 | yes |
| parquet | 11 | 270641 | 23665 | 1976458 | 9855 | 106650 | 10175 | 37346 | 3095 | yes |
| pdf | 3 | 72396 | 23683 | 97690 | 12438 | 2568 | 828 | 9200 | 2826 | yes |
| pkgmeta | 5 | 355716 | 24214 | 179398 | 17167 | 137653 | 14242 | 32902 | 3026 | yes |
| ppt | 7 | 177510 | 24345 | 1920007 | 138588 | 42366 | 5689 | 24199 | 3282 | yes |
| pptx | 12 | 2041923 | 33724 | 3645029 | 127760.0 | 3622841 | 43938 | 264339 | 5224 | yes |
| rst | 6 | 151948 | 24380 | 397249 | 29476.5 | 64792 | 10224 | 18658 | 3062 | yes |
| rtf | 4 | 90551 | 23608 | 191974 | 15266.5 | 30563 | 9790 | 11779 | 2890 | yes |
| toml | 9 | 212988 | 23660 | 39681 | 4013 | 94857 | 9892 | 27900 | 2924 | yes |
| xls | 9 | 228044 | 24238 | 1278024 | 61291 | 75301 | 10047 | 32001 | 3162 | yes |
| xlsx | 14 | 2150417 | 22019 | 1282259 | 30390.0 | 2858175 | 31932 | 330526 | 3145 | yes |
| xml | 4 | 936534 | 214890 | 254734 | 41559.0 | 589720 | 125901 | 114683 | 16728 | yes |
| yaml | 11 | 1980109 | 23932 | 195757 | 9179 | 1226898 | 11969 | 303732 | 3116 | yes |

## By size class

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| large(>=256KiB) | 25 | 1577900 | 31169 | 36312633 | 1146204 | 631037 | 13692 | 209326 | 4482 | yes |
| medium(<256KiB) | 86 | 6157560 | 24818 | 18738444 | 134935.5 | 7073301 | 10666 | 788889 | 3288 | yes |
| small(<16KiB) | 197 | 14003120 | 23757 | 3538024 | 11954 | 10488671 | 10132 | 1921452 | 2997 | yes |

## By complexity

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| complex | 16 | 4181092 | 367066 | 12213569 | 178642.5 | 3774886 | 67394 | 511722 | 35087 | yes |
| hostile | 24 | 491375 | 19332 | 341455 | 6083.5 | 92402 | 774 | 72430 | 2930 | yes |
| moderate | 19 | 5448404 | 375668 | 2368557 | 44661 | 6645208 | 244599 | 655385 | 36846 | yes |
| nested | 114 | 2681608 | 23664 | 17207529 | 36496.0 | 1758203 | 10163 | 371476 | 3082 | yes |
| simple | 37 | 6475608 | 33523 | 5698094 | 23143 | 4858963 | 11768 | 992060 | 4174 | yes |
| structured | 98 | 2460493 | 23914 | 20759897 | 14515.5 | 1063347 | 10182 | 316594 | 3018 | yes |

## By stratum (real vs malformed/hostile)

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| hostile | 24 | 491375 | 19332 | 341455 | 6083.5 | 92402 | 774 | 72430 | 2930 | yes |
| real | 284 | 21247205 | 24032 | 58247646 | 30843.0 | 18100607 | 10286 | 2847237 | 3117 | yes |

### Workload dimension

`narrow` = one `observe --metadata --kind metadata` per document (a bounded metadata projection).
`full` = one `materialize --exact --packed` per document (byte-authority closure).
Both are pooled and median; a size/time-weighted world is `pooled`, an unweighted per-document world is `median` (ADR-0054: a claim names its estimator).

**Detection.** 27/308 samples were detected as `opaque` by the bounded detector; 61 samples declined the narrow observation. The byte-authority gate is unaffected by detection; every sample is byte-exactly closed.

## Findings (detector choice != declared format)

A **finding** is a real sample whose detector choice differs from its declared format: a conservative `decline-opaque`, or a `misclassified` claim by another detector. Every finding is reconciled in the phase report (fixed-with-test, or recorded as a typed decline / recorded misclassification with its reason). The byte-authority closure is unaffected by any finding.

Summary by kind: decline-opaque=10, misclassified=11

| id | declared | detected | kind | resolution |
|---|---|---|---|---|
| asciidoctor-CHANGELOG.adoc | asciidoc | rst | misclassified | recorded misclassification (precedence) — the reST detector is tried before AsciiDoc and claims attribute-only/locale/CHANGELOG AsciiDoc via its field-list (`:name: value`) / adornment signal; AsciiDoc attribute entries are syntactically RST field lists, and reordering AsciiDoc before reST is not clearly warranted (reST precedence is load-bearing), so it is not reordered (src/adapter/rst.rs, asciidoc.rs). |
| asciidoctor-cli-nav.adoc | asciidoc | opaque | decline-opaque | recorded decline — a `* xref:` link-list-only AsciiDoc page carries no AsciiDoc-specific block/title signal (no `=` title, `==` section, `|===` table, delimited block, or `[…]` block-attribute line), so it stays Opaque rather than being guessed (src/adapter/asciidoc.rs). |
| asciidoctor-locale-attributes-ja.adoc | asciidoc | rst | misclassified | recorded misclassification (precedence) — the reST detector is tried before AsciiDoc and claims attribute-only/locale/CHANGELOG AsciiDoc via its field-list (`:name: value`) / adornment signal; AsciiDoc attribute entries are syntactically RST field lists, and reordering AsciiDoc before reST is not clearly warranted (reST precedence is load-bearing), so it is not reordered (src/adapter/rst.rs, asciidoc.rs). |
| asciidoctor-locale-attributes-sr.adoc | asciidoc | rst | misclassified | recorded misclassification (precedence) — the reST detector is tried before AsciiDoc and claims attribute-only/locale/CHANGELOG AsciiDoc via its field-list (`:name: value`) / adornment signal; AsciiDoc attribute entries are syntactically RST field lists, and reordering AsciiDoc before reST is not clearly warranted (reST precedence is load-bearing), so it is not reordered (src/adapter/rst.rs, asciidoc.rs). |
| asciidoctor-pages-macos.adoc | asciidoc | rst | misclassified | recorded misclassification (precedence) — the reST detector is tried before AsciiDoc and claims attribute-only/locale/CHANGELOG AsciiDoc via its field-list (`:name: value`) / adornment signal; AsciiDoc attribute entries are syntactically RST field lists, and reordering AsciiDoc before reST is not clearly warranted (reST precedence is load-bearing), so it is not reordered (src/adapter/rst.rs, asciidoc.rs). |
| asciidoctor-pages-stylesheet-modes.adoc | asciidoc | rst | misclassified | recorded misclassification (precedence) — the reST detector is tried before AsciiDoc and claims attribute-only/locale/CHANGELOG AsciiDoc via its field-list (`:name: value`) / adornment signal; AsciiDoc attribute entries are syntactically RST field lists, and reordering AsciiDoc before reST is not clearly warranted (reST precedence is load-bearing), so it is not reordered (src/adapter/rst.rs, asciidoc.rs). |
| cbor-data-integer.cbor | cbor | opaque | decline-opaque | recorded decline — a lone integer, a tagged date, and an RFC 8949 multi-value concatenation all fall below the detector's deliberate bar (a full-input single well-formed parse whose root is a container/tag reaching >= 3 nodes); conservative by design (src/adapter/cbor.rs). |
| cbor-data-tagged_date.cbor | cbor | opaque | decline-opaque | recorded decline — a lone integer, a tagged date, and an RFC 8949 multi-value concatenation all fall below the detector's deliberate bar (a full-input single well-formed parse whose root is a container/tag reaching >= 3 nodes); conservative by design (src/adapter/cbor.rs). |
| cbor-rfc8949-good.cbor | cbor | opaque | decline-opaque | recorded decline — a lone integer, a tagged date, and an RFC 8949 multi-value concatenation all fall below the detector's deliberate bar (a full-input single well-formed parse whose root is a container/tag reaching >= 3 nodes); conservative by design (src/adapter/cbor.rs). |
| poi-document-Bug60942b.doc | doc | cfbf | misclassified | recorded decline — two distinct causes: (a) `Bug60942b.doc` is a **Word 6.0** FIB (`wIdent` 0xA5DC, `nFib` 0x0065), which is outside the supported Word 97-2003 `nFib` subset the adapter models, so it stays at the generic CFBF layer; (b) `testRangeDelete.doc` has a genuine Word FIB but its `WordDocument` mini-stream is **fragmented and its root mini stream is itself fragmented**, so the first chunk's absolute offset is not recoverable from the model and `stream_leading` conservatively declines (src/adapter/doc.rs, cfbf.rs). |
| poi-document-testRangeDelete.doc | doc | cfbf | misclassified | recorded decline — two distinct causes: (a) `Bug60942b.doc` is a **Word 6.0** FIB (`wIdent` 0xA5DC, `nFib` 0x0065), which is outside the supported Word 97-2003 `nFib` subset the adapter models, so it stays at the generic CFBF layer; (b) `testRangeDelete.doc` has a genuine Word FIB but its `WordDocument` mini-stream is **fragmented and its root mini stream is itself fragmented**, so the first chunk's absolute offset is not recoverable from the model and `stream_leading` conservatively declines (src/adapter/doc.rs, cfbf.rs). |
| cpython-msg-25.eml | eml | opaque | decline-opaque | recorded decline — the sample is an **mbox** (a leading `From ` envelope line, not an RFC 5322 header) whose `multipart/report` boundary is folded and truncated (`;\\n\\tbo`), so the bounded header-block scan declines rather than admitting an unmodellable message container (src/adapter/eml.rs). |
| loghub-HPC-HPC_2k.log | logstream | opaque | decline-opaque | recorded decline — HPC (no timestamp/level), Proxifier (`[MM.DD HH:MM:SS]`), Spark (`YY/MM/DD HH:MM:SS`) and Mac (BSD timestamp with no `<PRI>`) lines do not match the detector's admitted generic timestamp/level grammar, so the whole-file detector declines; widening the grammar is deferred as a deliberate conservatism (src/adapter/logstream.rs). |
| loghub-Mac-Mac_2k.log | logstream | opaque | decline-opaque | recorded decline — HPC (no timestamp/level), Proxifier (`[MM.DD HH:MM:SS]`), Spark (`YY/MM/DD HH:MM:SS`) and Mac (BSD timestamp with no `<PRI>`) lines do not match the detector's admitted generic timestamp/level grammar, so the whole-file detector declines; widening the grammar is deferred as a deliberate conservatism (src/adapter/logstream.rs). |
| loghub-OpenStack-OpenStack_2k.log | logstream | config | misclassified | recorded misclassification (precedence) — the config detector (properties `KEY: value` shape) claims an OpenStack log stream that the line-dialect log detector declines; reordering logstream before config risks the load-bearing config detector, so it is not reordered (src/adapter/logstream.rs, config.rs). |
| loghub-Proxifier-Proxifier_2k.log | logstream | opaque | decline-opaque | recorded decline — HPC (no timestamp/level), Proxifier (`[MM.DD HH:MM:SS]`), Spark (`YY/MM/DD HH:MM:SS`) and Mac (BSD timestamp with no `<PRI>`) lines do not match the detector's admitted generic timestamp/level grammar, so the whole-file detector declines; widening the grammar is deferred as a deliberate conservatism (src/adapter/logstream.rs). |
| loghub-Spark-Spark_2k.log | logstream | opaque | decline-opaque | recorded decline — HPC (no timestamp/level), Proxifier (`[MM.DD HH:MM:SS]`), Spark (`YY/MM/DD HH:MM:SS`) and Mac (BSD timestamp with no `<PRI>`) lines do not match the detector's admitted generic timestamp/level grammar, so the whole-file detector declines; widening the grammar is deferred as a deliberate conservatism (src/adapter/logstream.rs). |
| loghub-Windows-Windows_2k.log | logstream | csv | misclassified | recorded misclassification (precedence) — the CSV detector claims a Windows event-style log whose lines have a consistent field count, before the (later) logstream detector runs; reordering risks the load-bearing CSV detector, so it is not reordered (src/adapter/logstream.rs, csv.rs). |
| poi-Divino.pptx | pptx | opaque | decline-opaque | recorded decline — the sample is **truncated**: it has no EOCD / central directory (and no valid ZIP structure), so it is not a valid OPC/OOXML package and correctly stays Opaque (the whole-source exact leaf is unaffected). |
| cpython-whatsnew-3.3.rst | rst | markdown | misclassified | recorded misclassification (precedence) — the reST detector is tried after Markdown, and the Markdown detector claims a real CPython `whatsnew` document via its structural marks (the over+underlined `***` RST title is a Markdown thematic break); reordering is not clearly warranted, so it is recorded (src/adapter/rst.rs, markdown.rs). |
| cpython-workflows-project-updater.yml | yaml | config | misclassified | recorded misclassification (precedence) — a GitHub Actions workflow that the bounded YAML detector declines (a `${{ … }}` expression / flow mapping outside the modelled YAML subset) is claimed by the config detector (`KEY: value` shape); recorded rather than weakening either detector (src/adapter/yaml.rs, config.rs). |

Each finding is either **fixed with a focused regression test** or
**recorded** as a typed decline / recorded misclassification with its reason
(a *recorded* detector-boundary finding never changes the exact bytes; the
byte-authority gate is `materialize == source` for every sample). Phase 27.3
detector fixes landed here: (1) the legacy PPT/XLS detectors now read only the
stream **header prefix** (`cfbf::stream_leading`), so a **fragmented**
`PowerPoint Document`/`Workbook` chain still detects - regression tests
`legacy_cfbf_detection::fragmented_powerpoint_document_stream_is_detected` and
`legacy_cfbf_detection::fragmented_workbook_stream_is_detected`; (2) the XLS
detector matches the well-known `Workbook`/`Book` stream name
ASCII-case-insensitively - regression test
`legacy_cfbf_detection::uppercase_workbook_stream_name_is_detected`; (3) the
GIS detector now claims GPX 1.0 and a namespace-less `<kml>` - regression tests
`gis::tests::detects_kml_and_gpx_and_rejects_junk` and
`gis_adapter::detection_is_byte_based_and_conservative`. Phase 27.5 (this court)
landed the bare, magic-less Arrow IPC **stream** framing: the arrow detector now
claims a coherent encapsulated-message sequence (a continuation-marked `Schema`
first, ending in an end-of-stream marker), so all four `arrow-stream-*.stream`
samples are claimed as `arrow` - regression tests
`arrow_adapter::stream_framing_is_detected_and_messages_are_ordered_and_exact`,
`arrow_adapter::stream_near_misses_decline_to_opaque`, and court
`tools/phase27-5-1-arrow-stream-court.sh`.

No real sample was skipped.

### Per-sample

| id | declared | detected | stratum | size class | complexity | src B | store B | build us | narrow us | full us | exact |
|---|---|---|---|---|---|---:|---:|---:|---:|---:|---|
| arrow-file-1.0.0-bigendian-generated_decimal256.arrow_file | arrow | arrow | real | large(>=256KiB) | nested | 363106 | 736384 | 28376 | 11785 | 4436 | True |
| arrow-file-1.0.0-littleendian-generated_union.arrow_file | arrow | arrow | real | small(<16KiB) | nested | 3562 | 11806 | 24042 | 15500 | 3114 | True |
| arrow-file-cpp-21.0.0-generated_dictionary_unsigned.arrow_file | arrow | arrow | real | small(<16KiB) | nested | 2170 | 8335 | 24198 | 10253 | 2997 | True |
| arrow-file-cpp-21.0.0-generated_null_trivial.arrow_file | arrow | arrow | real | small(<16KiB) | nested | 514 | 4774 | 24010 | 10494 | 3110 | True |
| arrow-stream-1.0.0-littleendian-generated_decimal256.stream | arrow | arrow | real | large(>=256KiB) | nested | 360464 | 732924 | 29170 | 11167 | 4420 | True |
| arrow-stream-1.0.0-littleendian-generated_nested_dictionary.stream | arrow | arrow | real | small(<16KiB) | nested | 2632 | 9850 | 23779 | 10037 | 2894 | True |
| arrow-stream-1.0.0-littleendian-generated_null_trivial.stream | arrow | arrow | real | small(<16KiB) | nested | 312 | 4582 | 23790 | 14416 | 3300 | True |
| arrow-stream-cpp-21.0.0-generated_custom_metadata.stream | arrow | arrow | real | small(<16KiB) | nested | 1504 | 7330 | 23845 | 10213 | 3157 | True |
| hostile2-magic-arrow | arrow | opaque | hostile | small(<16KiB) | hostile | 514 | 3726 | 18717 | 759 | 2762 | True |
| asciidoctor-CHANGELOG.adoc | asciidoc | rst | real | medium(<256KiB) | structured | 140952 | 307359 | 29892 | 10898 | 3621 | True |
| asciidoctor-cli-nav.adoc | asciidoc | opaque | real | small(<16KiB) | structured | 222 | 3434 | 19136 | 733 | 2775 | True |
| asciidoctor-locale-attributes-ja.adoc | asciidoc | rst | real | small(<16KiB) | structured | 732 | 6089 | 23597 | 9994 | 2996 | True |
| asciidoctor-locale-attributes-sr.adoc | asciidoc | rst | real | small(<16KiB) | structured | 863 | 6641 | 23642 | 13010 | 4497 | True |
| asciidoctor-pages-contexts-ref.adoc | asciidoc | asciidoc | real | small(<16KiB) | structured | 3640 | 15247 | 24012 | 10182 | 2959 | True |
| asciidoctor-pages-macos.adoc | asciidoc | rst | real | small(<16KiB) | structured | 1577 | 8386 | 23959 | 10003 | 3007 | True |
| asciidoctor-pages-stylesheet-modes.adoc | asciidoc | rst | real | small(<16KiB) | structured | 7284 | 22583 | 24023 | 10262 | 3118 | True |
| bibtex-build-strings-base.bib | bib | bib | real | medium(<256KiB) | structured | 145131 | 545657 | 27302 | 11069 | 3513 | True |
| bibtex-strings.bib | bib | bib | real | large(>=256KiB) | structured | 271806 | 1025695 | 29394 | 12239 | 3946 | True |
| hostile2-trunc-bib | bib | opaque | hostile | small(<16KiB) | hostile | 800 | 4012 | 19101 | 814 | 2788 | True |
| cbor-data-all_types.cbor | cbor | cbor | real | small(<16KiB) | nested | 354 | 6300 | 23668 | 9904 | 2848 | True |
| cbor-data-integer.cbor | cbor | opaque | real | small(<16KiB) | nested | 2 | 3214 | 19082 | 676 | 2815 | True |
| cbor-data-tagged_date.cbor | cbor | opaque | real | small(<16KiB) | nested | 22 | 3234 | 18710 | 646 | 2748 | True |
| cbor-rfc8949-appendixA-mt2.cbor | cbor | cbor | real | small(<16KiB) | nested | 177 | 4742 | 23451 | 9886 | 2785 | True |
| cbor-rfc8949-appendixA-mt3.cbor | cbor | cbor | real | small(<16KiB) | nested | 487 | 6867 | 23450 | 9936 | 2912 | True |
| cbor-rfc8949-appendixA-streaming.cbor | cbor | cbor | real | small(<16KiB) | nested | 1145 | 13386 | 23366 | 10031 | 2907 | True |
| cbor-rfc8949-good.cbor | cbor | opaque | real | small(<16KiB) | nested | 13797 | 17009 | 19343 | 745 | 2893 | True |
| hostile2-trunc-cbor | cbor | opaque | hostile | small(<16KiB) | hostile | 40 | 3252 | 18786 | 673 | 2812 | True |
| poi-hpsf-TestVisioWithCodepage.vsd | cfbf | cfbf | real | medium(<256KiB) | nested | 45568 | 95579 | 23942 | 10228 | 3123 | True |
| poi-ole-poifs-only-zero-byte-streams.ole2 | cfbf | cfbf | real | small(<16KiB) | nested | 1536 | 7059 | 26579 | 10386 | 3265 | True |
| poi-publisher-LinkAt20.pub | cfbf | cfbf | real | medium(<256KiB) | nested | 64512 | 134355 | 24449 | 12857 | 3166 | True |
| poi-publisher-Sample_2010.pub | cfbf | cfbf | real | medium(<256KiB) | nested | 72704 | 150739 | 24478 | 10244 | 3185 | True |
| poi-publisher-clusterfuzz-testcase-minimized-POIHPBFFuzzer-4701121678278656.pub | cfbf | cfbf | real | large(>=256KiB) | nested | 331491 | 666405 | 27446 | 6482 | 4182 | True |
| cpython-libregrtest-mypy.ini | config | config | real | small(<16KiB) | simple | 997 | 7731 | 23773 | 12487 | 4174 | True |
| poi-gradle.properties | config | config | real | small(<16KiB) | simple | 555 | 5630 | 24717 | 10137 | 3002 | True |
| poi-resources-font_metrics.properties | config | config | real | medium(<256KiB) | simple | 146498 | 359532 | 29082 | 11072 | 3480 | True |
| cpython-namespace_pkgs-missing_directory.zip | container | container | real | small(<16KiB) | nested | 515 | 7457 | 20095 | 9846 | 2998 | True |
| cpython-test-recursion.tar | container | container | real | small(<16KiB) | nested | 516 | 4470 | 23418 | 5419 | 2879 | True |
| cpython-test-zipdir_backslash.zip | container | container | real | small(<16KiB) | nested | 192 | 6019 | 20236 | 9908 | 2886 | True |
| cpython-zipdata01-ziptestdata.zip | container | container | real | small(<16KiB) | nested | 876 | 9797 | 20245 | 10016 | 2975 | True |
| hostile2-trunc-container | container | opaque | hostile | small(<16KiB) | hostile | 5000 | 8212 | 19058 | 738 | 2901 | True |
| ne-zip-make-graticules-and-bounding-box-ne_1d5_graticules_beta2.zip | container | container | real | large(>=256KiB) | nested | 1000316 | 2055115 | 40538 | 13693 | 6393 | True |
| ne-zip-zip-ne_50m_graticules_1.zip | container | container | real | medium(<256KiB) | nested | 128416 | 264104 | 21355 | 10087 | 3132 | True |
| ne-zip-zip-ne_50m_graticules_15.zip | container | container | real | small(<16KiB) | nested | 10012 | 27300 | 20273 | 10019 | 2943 | True |
| ne-zip-zip-ne_50m_graticules_5.zip | container | container | real | medium(<256KiB) | nested | 27460 | 62192 | 20474 | 11793 | 3133 | True |
| poi-openxml4j-dcterms_bug_56479.zip | container | container | real | small(<16KiB) | nested | 1277 | 9934 | 20786 | 10056 | 3023 | True |
| parquet-delta-expect.csv | csv | csv | real | medium(<256KiB) | moderate | 159803 | 552658 | 478854 | 171327 | 47417 | True |
| vega-data-airports.csv | csv | csv | real | medium(<256KiB) | simple | 210363 | 893639 | 27314 | 11747 | 3668 | True |
| vega-data-co2-concentration.csv | csv | csv | real | medium(<256KiB) | simple | 18547 | 93286 | 24457 | 10273 | 3017 | True |
| vega-data-flights-airport.csv | csv | csv | real | medium(<256KiB) | simple | 65572 | 515711 | 26784 | 11429 | 3280 | True |
| vega-data-gapminder-health-income.csv | csv | csv | real | small(<16KiB) | simple | 8605 | 40460 | 23925 | 10279 | 2994 | True |
| vega-data-github.csv | csv | csv | real | medium(<256KiB) | simple | 21059 | 97252 | 24534 | 10314 | 3015 | True |
| vega-data-iowa-electricity.csv | csv | csv | real | small(<16KiB) | simple | 1531 | 10264 | 23703 | 10112 | 3208 | True |
| vega-data-population_engineers_hurricanes.csv | csv | csv | real | small(<16KiB) | simple | 1852 | 12779 | 24065 | 9969 | 2958 | True |
| vega-global-temp.csv | csv | csv | real | small(<16KiB) | simple | 1663 | 14668 | 464165 | 189902 | 51991 | True |
| vega-iowa-electricity.csv | csv | csv | real | small(<16KiB) | simple | 1531 | 10264 | 379103 | 191593 | 53421 | True |
| vega-population-engineers.csv | csv | csv | real | small(<16KiB) | simple | 1852 | 12779 | 480894 | 362670 | 33998 | True |
| vega-tsv-data-unemployment.tsv | csv | csv | real | medium(<256KiB) | simple | 34739 | 246812 | 24761 | 10731 | 3047 | True |
| hostile2-magic-doc | doc | opaque | hostile | medium(<256KiB) | hostile | 17920 | 21132 | 20077 | 879 | 2980 | True |
| poi-document-Bug41898.doc | doc | doc | real | medium(<256KiB) | nested | 24064 | 51548 | 23488 | 5539 | 2954 | True |
| poi-document-Bug46817.doc | doc | doc | real | medium(<256KiB) | nested | 17920 | 39260 | 23383 | 5210 | 2856 | True |
| poi-document-Bug53380_2.doc | doc | doc | real | medium(<256KiB) | nested | 31744 | 66908 | 24073 | 6258 | 3476 | True |
| poi-document-Bug60942b.doc | doc | cfbf | real | small(<16KiB) | nested | 6144 | 16721 | 23350 | 9669 | 2757 | True |
| poi-document-ThreeColHead.doc | doc | doc | real | medium(<256KiB) | nested | 27136 | 57692 | 25800 | 5396 | 2789 | True |
| poi-document-cap.stanford.edu_profiles_viewbiosketch_facultyid-4009-name-m_maciv | doc | doc | real | medium(<256KiB) | nested | 47104 | 97628 | 24706 | 5658 | 3042 | True |
| poi-document-parentinvguid.doc | doc | doc | real | large(>=256KiB) | nested | 398848 | 801116 | 28816 | 6670 | 4431 | True |
| poi-document-testRangeDelete.doc | doc | cfbf | real | medium(<256KiB) | nested | 104448 | 213341 | 25054 | 10162 | 3268 | True |
| hostile2-magic-docx | docx | opaque | hostile | small(<16KiB) | hostile | 10045 | 13257 | 19373 | 675 | 2851 | True |
| hostile2-trunc-docx | docx | docx | hostile | small(<16KiB) | hostile | 2297 | 14328 | 20360 | 28271 | 3064 | True |
| poi-document-58618.docx | docx | docx | real | small(<16KiB) | nested | 10045 | 52985 | 24257 | 37599 | 3075 | True |
| poi-document-60329.docx | docx | docx | real | medium(<256KiB) | nested | 31124 | 154025 | 20788 | 42523 | 3142 | True |
| poi-document-Bug51170.docx | docx | docx | real | medium(<256KiB) | nested | 239621 | 720218 | 23024 | 38527 | 3781 | True |
| poi-document-Bug60341.docx | docx | docx | real | small(<16KiB) | nested | 2297 | 14328 | 20390 | 28189 | 2994 | True |
| poi-document-NumberingWOverrides.docx | docx | docx | real | small(<16KiB) | nested | 15190 | 72123 | 20362 | 42355 | 3054 | True |
| poi-document-ThreeColHeadFoot.docx | docx | docx | real | small(<16KiB) | nested | 13123 | 66713 | 24813 | 37871 | 3081 | True |
| poi-document-capitalized.docx | docx | docx | real | small(<16KiB) | nested | 12104 | 73162 | 20509 | 37356 | 3108 | True |
| poi-document-checkboxes.docx | docx | docx | real | medium(<256KiB) | nested | 20800 | 108992 | 20982 | 38260 | 3089 | True |
| cpython-data-msg_08.txt | eml | eml | real | small(<16KiB) | structured | 454 | 5868 | 23459 | 10135 | 3001 | True |
| cpython-data-msg_22.txt | eml | eml | real | small(<16KiB) | structured | 1894 | 9580 | 23817 | 10054 | 3035 | True |
| cpython-data-msg_23.txt | eml | eml | real | small(<16KiB) | structured | 139 | 4246 | 23548 | 10061 | 2967 | True |
| cpython-data-msg_33.txt | eml | eml | real | small(<16KiB) | structured | 750 | 6266 | 23717 | 10182 | 3129 | True |
| cpython-data-msg_34.txt | eml | eml | real | small(<16KiB) | structured | 300 | 4725 | 23569 | 9924 | 2864 | True |
| cpython-data-msg_43.txt | eml | eml | real | small(<16KiB) | structured | 9166 | 24520 | 24040 | 10065 | 2972 | True |
| cpython-msg-01.eml | eml | eml | real | small(<16KiB) | simple | 459 | 5533 | 32830 | 13657 | 6078 | True |
| cpython-msg-25.eml | eml | opaque | real | small(<16KiB) | moderate | 5122 | 8334 | 203588 | 791 | 2888 | True |
| cpython-msg-43.eml | eml | eml | real | small(<16KiB) | complex | 9166 | 24520 | 23750 | 9897 | 2928 | True |
| hostile2-magic-epub | epub | opaque | hostile | medium(<256KiB) | hostile | 30552 | 33764 | 20701 | 801 | 3015 | True |
| tika-test-documents-testEPUB.epub | epub | epub | real | medium(<256KiB) | nested | 30552 | 72075 | 21017 | 10527 | 3194 | True |
| tika-test-documents-testEPUB_xml_ext.epub | epub | epub | real | small(<16KiB) | nested | 7823 | 29942 | 21097 | 10164 | 2907 | True |
| feed-1.1-extensive-noerror.xml | feed | feed | real | small(<16KiB) | structured | 1705 | 13105 | 23943 | 10159 | 2876 | True |
| feed-1.1-extensive-nowarn.xml | feed | feed | real | small(<16KiB) | structured | 1747 | 13189 | 23733 | 9989 | 2827 | True |
| feed-a33-3.3-duplicate-updated.xml | feed | feed | real | small(<16KiB) | structured | 1015 | 9234 | 23651 | 13344 | 2948 | True |
| feed-a33-3.3-published_no_year.xml | feed | feed | real | small(<16KiB) | structured | 875 | 7971 | 23891 | 9874 | 2874 | True |
| feed-a33-3.3-published_seconds.xml | feed | feed | real | small(<16KiB) | structured | 895 | 8011 | 23706 | 9942 | 2942 | True |
| feed-a33-3.3-updated-example2.xml | feed | feed | real | small(<16KiB) | structured | 706 | 7505 | 23667 | 10137 | 2937 | True |
| hostile2-trunc-geojson | geojson | opaque | hostile | small(<16KiB) | hostile | 2000 | 5212 | 19078 | 610 | 3890 | True |
| natural-earth-countries.geojson | geojson | geojson | real | large(>=256KiB) | complex | 838726 | 4054747 | 446874 | 286303 | 51353 | True |
| natural-earth-places.geojson | geojson | geojson | real | medium(<256KiB) | moderate | 166071 | 830240 | 464422 | 244599 | 47101 | True |
| ne-bbox.geojson | geojson | geojson | real | small(<16KiB) | simple | 4813 | 34888 | 507383 | 210552 | 37163 | True |
| ne-geojson-ne_10m_admin_0_antarctic_claim_limit_lines.geojson | geojson | geojson | real | medium(<256KiB) | structured | 58099 | 431186 | 25946 | 14459 | 3186 | True |
| ne-geojson-ne_10m_bathymetry_A_10000.geojson | geojson | geojson | real | small(<16KiB) | structured | 2860 | 18223 | 23489 | 10022 | 2918 | True |
| ne-geojson-ne_10m_parks_and_protected_lands_scale_rank.geojson | geojson | geojson | real | medium(<256KiB) | structured | 202844 | 1032397 | 27624 | 12427 | 3513 | True |
| ne-geojson-ne_110m_admin_0_boundary_lines_land.geojson | geojson | geojson | real | large(>=256KiB) | structured | 340010 | 1722622 | 30121 | 14307 | 4247 | True |
| ne-geojson-ne_110m_admin_1_states_provinces_lakes.geojson | geojson | geojson | real | medium(<256KiB) | structured | 181275 | 868587 | 27834 | 12220 | 3602 | True |
| ne-geojson-ne_110m_glaciated_areas.geojson | geojson | geojson | real | medium(<256KiB) | structured | 21720 | 116257 | 23607 | 10166 | 2917 | True |
| ne-geojson-ne_50m_geography_regions_elevation_points.geojson | geojson | geojson | real | medium(<256KiB) | structured | 104139 | 438795 | 25733 | 11033 | 3360 | True |
| ne-lakes.geojson | geojson | geojson | real | small(<16KiB) | moderate | 7719 | 43208 | 393851 | 288786 | 50113 | True |
| ne-regions.geojson | geojson | geojson | real | small(<16KiB) | simple | 4099 | 19576 | 260256 | 120426 | 47879 | True |
| hostile2-trunc-gis | gis | opaque | hostile | small(<16KiB) | hostile | 200 | 3412 | 19031 | 701 | 2777 | True |
| togeojson-data-bad.gpx | gis | gis | real | small(<16KiB) | structured | 833 | 8679 | 24095 | 10265 | 3002 | True |
| togeojson-data-blue_hills.gpx | gis | gis | real | large(>=256KiB) | structured | 268694 | 2069245 | 36848 | 15351 | 4081 | True |
| togeojson-data-null_geometry.kml | gis | gis | real | small(<16KiB) | structured | 286 | 4889 | 23934 | 11930 | 4240 | True |
| togeojson-data-opacity_override.kml | gis | gis | real | small(<16KiB) | structured | 625 | 7617 | 24243 | 10195 | 3107 | True |
| togeojson-data-opencpn.gpx | gis | gis | real | small(<16KiB) | structured | 10637 | 66507 | 24349 | 10132 | 3654 | True |
| togeojson-data-point_extra_coordinates.kml | gis | gis | real | small(<16KiB) | structured | 386 | 5465 | 24395 | 10443 | 3215 | True |
| togeojson-data-schema.kml | gis | gis | real | small(<16KiB) | structured | 2158 | 16426 | 28799 | 10149 | 3036 | True |
| togeojson-data-style.kml | gis | gis | real | small(<16KiB) | structured | 1372 | 13453 | 23761 | 10071 | 2998 | True |
| togeojson-data-styles.kml | gis | gis | real | small(<16KiB) | structured | 6374 | 44998 | 24507 | 10274 | 3114 | True |
| hostile2-trunc-hdf | hdf | opaque | hostile | small(<16KiB) | hostile | 600 | 3812 | 18875 | 652 | 2873 | True |
| netcdf-h5-h5_test-ref_tst_h_compounds2.h5 | hdf | hdf | real | small(<16KiB) | nested | 557 | 4534 | 23362 | 5316 | 2775 | True |
| netcdf-h5-nczarr_test-ref_groups.h5 | hdf | hdf | real | small(<16KiB) | nested | 9836 | 23282 | 23667 | 9920 | 3080 | True |
| netcdf-ncdump-ref_nc_test_netcdf4_4_0.nc | hdf | hdf | real | medium(<256KiB) | nested | 162812 | 329044 | 25801 | 6033 | 3496 | True |
| netcdf-ncdump-ref_no_ncproperty.nc | hdf | hdf | real | small(<16KiB) | nested | 1262 | 5944 | 23808 | 5362 | 2927 | True |
| netcdf-ncdump-ref_test_365_day_1900.nc | hdf | hdf | real | small(<16KiB) | nested | 9348 | 22248 | 23820 | 10100 | 3389 | True |
| netcdf-ncdump-ref_tst_compounds2.nc | hdf | hdf | real | small(<16KiB) | nested | 5753 | 14926 | 23606 | 5337 | 2952 | True |
| netcdf-ncdump-ref_tst_compounds4.nc | hdf | hdf | real | small(<16KiB) | nested | 526 | 4472 | 23620 | 5367 | 2905 | True |
| netcdf-nctestfiles-test_vlen2.nc | hdf | hdf | real | small(<16KiB) | nested | 6336 | 16092 | 23735 | 5494 | 3027 | True |
| feedval-05-version_12_supports_atom_02.html | html | html | real | medium(<256KiB) | structured | 47054 | 110374 | 28796 | 10374 | 3288 | True |
| feedval-21-live.html | html | html | real | small(<16KiB) | structured | 2277 | 19579 | 23553 | 9928 | 2861 | True |
| feedval-22-known_bugs.html | html | html | real | small(<16KiB) | structured | 3478 | 24861 | 25744 | 10121 | 3027 | True |
| feedval-29-new_mailing_list_for_validator_users.html | html | html | real | small(<16KiB) | structured | 2823 | 21310 | 23680 | 10256 | 2965 | True |
| mdn-doc-structure.html | html | html | real | small(<16KiB) | moderate | 3525 | 20244 | 131279 | 45975 | 12506 | True |
| mdn-fonts-demo.html | html | html | real | medium(<256KiB) | complex | 36867 | 189800 | 120383 | 54227 | 13483 | True |
| mdn-getting-started.html | html | html | real | small(<16KiB) | simple | 224 | 5429 | 135799 | 53242 | 13576 | True |
| mdn-planets.html | html | html | real | small(<16KiB) | moderate | 4302 | 39076 | 121429 | 56841 | 12612 | True |
| cpython-fuzz_json_loads_corpus-empty_array.json | json | json | real | small(<16KiB) | structured | 3 | 3509 | 23391 | 9992 | 2999 | True |
| cpython-fuzz_json_loads_corpus-pass1.json | json | json | real | small(<16KiB) | structured | 1441 | 8910 | 23757 | 9876 | 2877 | True |
| cpython-fuzz_json_loads_corpus-simple_array.json | json | json | real | small(<16KiB) | structured | 25 | 3678 | 25510 | 9800 | 2905 | True |
| hostile2-magic-json | json | opaque | hostile | small(<16KiB) | hostile | 2743 | 5955 | 19290 | 772 | 4027 | True |
| vega-data-barley.json | json | json | real | small(<16KiB) | structured | 8487 | 47477 | 23758 | 10182 | 2849 | True |
| vega-data-budget.json | json | json | real | large(>=256KiB) | structured | 391353 | 1645334 | 37074 | 14933 | 4413 | True |
| vega-data-burtin.json | json | json | real | small(<16KiB) | structured | 2743 | 14189 | 23830 | 10132 | 3001 | True |
| vega-data-penguins.json | json | json | real | medium(<256KiB) | structured | 67119 | 266741 | 26186 | 10993 | 3407 | True |
| vega-data-volcano.json | json | json | real | medium(<256KiB) | structured | 21167 | 178662 | 23840 | 10846 | 3037 | True |
| vega-data-weekly-weather.json | json | json | real | small(<16KiB) | structured | 1281 | 12815 | 24440 | 10285 | 2986 | True |
| vega-data-world-110m.json | json | json | real | medium(<256KiB) | structured | 119410 | 1076373 | 31068 | 13268 | 3541 | True |
| mdx-js-renovate.json5 | json5 | json5 | real | small(<16KiB) | structured | 92 | 3852 | 23532 | 9938 | 2992 | True |
| jsonlines-datagov100.json | jsonl | jsonl | real | large(>=256KiB) | complex | 696007 | 2253927 | 57517 | 19440 | 5887 | True |
| jsonlines-datagov100.json | jsonl | jsonl | real | large(>=256KiB) | structured | 696007 | 2253927 | 34959 | 15963 | 5473 | True |
| oai-data-dbpedia_samples.jsonl | jsonl | jsonl | real | medium(<256KiB) | structured | 64512 | 167137 | 24227 | 10286 | 3139 | True |
| oai-data-drone_training.jsonl | jsonl | jsonl | real | large(>=256KiB) | structured | 387706 | 1837044 | 30974 | 16494 | 4482 | True |
| oai-data-example_requests_to_parallel_process.jsonl | jsonl | jsonl | real | large(>=256KiB) | structured | 548917 | 2831447 | 33237 | 19301 | 5355 | True |
| oai-data-toy_chat_fine_tuning.jsonl | jsonl | jsonl | real | medium(<256KiB) | structured | 27385 | 61273 | 23896 | 10262 | 3085 | True |
| oai-dbpedia.jsonl | jsonl | jsonl | real | medium(<256KiB) | moderate | 64512 | 167137 | 34756 | 15859 | 7268 | True |
| oai-parallel-requests.jsonl | jsonl | jsonl | real | large(>=256KiB) | complex | 548917 | 2831447 | 43808 | 23431 | 5692 | True |
| oai-toy-chat.jsonl | jsonl | jsonl | real | medium(<256KiB) | moderate | 27385 | 61273 | 34189 | 13701 | 3920 | True |
| loghub-HPC-HPC_2k.log | logstream | opaque | real | medium(<256KiB) | simple | 151178 | 154390 | 31658 | 1210 | 3990 | True |
| loghub-Mac-Mac_2k.log | logstream | opaque | real | large(>=256KiB) | simple | 319414 | 322626 | 36293 | 1501 | 4131 | True |
| loghub-OpenStack-OpenStack_2k.log | logstream | config | real | large(>=256KiB) | simple | 595119 | 1329738 | 36333 | 13692 | 5026 | True |
| loghub-Proxifier-Proxifier_2k.log | logstream | opaque | real | medium(<256KiB) | simple | 236962 | 240174 | 33523 | 1449 | 4011 | True |
| loghub-Spark-Spark_2k.log | logstream | opaque | real | medium(<256KiB) | simple | 196268 | 199480 | 34748 | 1253 | 3852 | True |
| loghub-Windows-Windows_2k.log | logstream | csv | real | large(>=256KiB) | simple | 285433 | 701911 | 28773 | 11768 | 4377 | True |
| commonmark-readme.md | markdown | markdown | real | small(<16KiB) | simple | 7671 | 23143 | 495073 | 213787 | 52917 | True |
| commonmark-spec.txt | markdown | markdown | real | medium(<256KiB) | complex | 205025 | 530161 | 438531 | 312257 | 46139 | True |
| cpython-ISSUE_TEMPLATE-crash.md | markdown | markdown | real | small(<16KiB) | simple | 1142 | 6380 | 24167 | 9945 | 2993 | True |
| cpython-rhel7-README.md | markdown | markdown | real | small(<16KiB) | simple | 608 | 5323 | 23547 | 9923 | 2846 | True |
| cpython-wasm-README.md | markdown | markdown | real | medium(<256KiB) | simple | 16560 | 51340 | 24875 | 10003 | 2785 | True |
| rust-readme.md | markdown | markdown | real | small(<16KiB) | simple | 3137 | 14075 | 393147 | 243546 | 158181 | True |
| mdx-blog-conf.mdx | mdx | mdx | real | small(<16KiB) | structured | 2790 | 12397 | 23855 | 10027 | 3018 | True |
| mdx-blog-v1.mdx | mdx | mdx | real | small(<16KiB) | structured | 9150 | 33026 | 24295 | 10052 | 2996 | True |
| mdx-community-sponsor.mdx | mdx | mdx | real | small(<16KiB) | structured | 5513 | 15451 | 23754 | 10233 | 3271 | True |
| mdx-docs-404.mdx | mdx | mdx | real | small(<16KiB) | structured | 545 | 5246 | 23855 | 10230 | 4270 | True |
| mdx-docs-getting-started.mdx | mdx | mdx | real | medium(<256KiB) | structured | 26777 | 87116 | 27068 | 10551 | 3069 | True |
| mdx-guides-embed.mdx | mdx | mdx | real | small(<16KiB) | structured | 3652 | 13099 | 23998 | 10443 | 3084 | True |
| mdx-packages-index.mdx | mdx | mdx | real | small(<16KiB) | structured | 995 | 6389 | 24159 | 10218 | 2997 | True |
| hostile2-trunc-mhtml | mhtml | mhtml | hostile | small(<16KiB) | hostile | 1000 | 7936 | 23892 | 9991 | 2954 | True |
| tika-test-documents-testMHTMLFirefox.mhtml | mhtml | mhtml | real | small(<16KiB) | nested | 13759 | 30944 | 24016 | 5602 | 3022 | True |
| tika-test-documents-testUserDefinedCharset.mhtml | mhtml | mhtml | real | small(<16KiB) | nested | 621 | 6217 | 23873 | 10277 | 3147 | True |
| hostile2-trunc-msg | msg | opaque | hostile | small(<16KiB) | hostile | 1500 | 4712 | 19002 | 736 | 2876 | True |
| poi-hsmf-ASCII_CP1251_LCID1049.msg | msg | msg | real | small(<16KiB) | nested | 3584 | 11471 | 23938 | 9999 | 2978 | True |
| poi-hsmf-attachment_msg_inlineImg.msg | msg | msg | real | medium(<256KiB) | nested | 147456 | 298332 | 25829 | 5930 | 3520 | True |
| poi-hsmf-example_sent_regular.msg | msg | msg | real | medium(<256KiB) | nested | 53248 | 109916 | 25854 | 5561 | 3272 | True |
| poi-hsmf-logsat.com_signatures_valid.msg | msg | msg | real | medium(<256KiB) | nested | 23040 | 49500 | 24136 | 5477 | 2827 | True |
| poi-hsmf-message_1981.msg | msg | msg | real | medium(<256KiB) | nested | 29696 | 62812 | 24189 | 5563 | 2847 | True |
| poi-hsmf-simple_test_msg.msg | msg | msg | real | medium(<256KiB) | nested | 16896 | 37212 | 23825 | 5366 | 3084 | True |
| hostile2-trunc-notebook | notebook | notebook | hostile | small(<16KiB) | hostile | 355 | 5075 | 23571 | 10050 | 3056 | True |
| jupyter-Notebook-Custom-Keyboard-Shortcuts.ipynb | notebook | notebook | real | small(<16KiB) | structured | 1148 | 7493 | 25326 | 10672 | 3348 | True |
| jupyter-Notebook-Importing-Notebooks.ipynb | notebook | notebook | real | small(<16KiB) | structured | 13552 | 47792 | 24396 | 13517 | 4388 | True |
| jupyter-Notebook-Running-Code.ipynb | notebook | notebook | real | medium(<256KiB) | structured | 52012 | 133052 | 25100 | 10583 | 3288 | True |
| jupyter-Notebook-What-is-the-Jupyter-Notebook.ipynb | notebook | notebook | real | small(<16KiB) | structured | 7541 | 24181 | 24315 | 10495 | 3131 | True |
| jupyter-nbs-other.ipynb | notebook | notebook | real | small(<16KiB) | structured | 786 | 6744 | 23524 | 9899 | 2984 | True |
| jupyter-notebooks-simple.ipynb | notebook | notebook | real | small(<16KiB) | structured | 355 | 5075 | 26984 | 11400 | 2896 | True |
| jupyter-notebooks-simple_toc.ipynb | notebook | notebook | real | small(<16KiB) | structured | 1040 | 7873 | 23752 | 10028 | 3011 | True |
| odfpy-cols.odp | odp | odp | real | medium(<256KiB) | moderate | 18037 | 148305 | 93123 | 820784 | 33401 | True |
| odfpy-emb.odp | odp | odp | real | medium(<256KiB) | complex | 16403 | 106233 | 410125 | 710143 | 34759 | True |
| odfpy-examples-cols.odp | odp | odp | real | medium(<256KiB) | nested | 18037 | 148305 | 20854 | 38953 | 3405 | True |
| odfpy-examples-ol.odp | odp | odp | real | small(<16KiB) | nested | 14940 | 138597 | 20321 | 38202 | 3283 | True |
| odfpy-ol.odp | odp | odp | real | small(<16KiB) | simple | 14940 | 138597 | 390895 | 840167 | 95657 | True |
| odfpy-chinese.ods | ods | ods | real | small(<16KiB) | moderate | 11765 | 44661 | 400508 | 403449 | 48944 | True |
| odfpy-empty.ods | ods | ods | real | small(<16KiB) | simple | 6819 | 29699 | 400243 | 401312 | 52387 | True |
| odfpy-examples-chinese_spreadsheet.ods | ods | ods | real | small(<16KiB) | nested | 11765 | 44661 | 20605 | 23834 | 2934 | True |
| odfpy-examples-empty.ods | ods | ods | real | small(<16KiB) | nested | 6819 | 29699 | 20430 | 23529 | 2904 | True |
| odfpy-examples-pythagoras-kspread.ods | ods | ods | real | small(<16KiB) | nested | 5037 | 22024 | 20128 | 23979 | 3018 | True |
| odfpy-examples-pythagoras.ods | ods | ods | real | small(<16KiB) | nested | 6837 | 30406 | 20312 | 23317 | 2861 | True |
| odfpy-pyth-kspread.ods | ods | ods | real | small(<16KiB) | simple | 5037 | 22024 | 437226 | 477318 | 47065 | True |
| odfpy-pythagoras.ods | ods | ods | real | small(<16KiB) | simple | 6837 | 30406 | 364876 | 431916 | 43476 | True |
| hostile2-trunc-odt | odt | opaque | hostile | small(<16KiB) | hostile | 3000 | 6212 | 19062 | 736 | 2756 | True |
| odfpy-examples-definitionlists.odt | odt | odt | real | small(<16KiB) | nested | 7824 | 32070 | 20059 | 23402 | 2915 | True |
| odfpy-examples-nasty.odt | odt | odt | real | small(<16KiB) | nested | 1539 | 11051 | 19949 | 18729 | 2807 | True |
| odfpy-examples-no_userfields.odt | odt | odt | real | medium(<256KiB) | nested | 19228 | 52978 | 20161 | 28190 | 2913 | True |
| odfpy-examples-parastyles.odt | odt | odt | real | small(<16KiB) | nested | 7194 | 32845 | 20069 | 23655 | 2873 | True |
| odfpy-examples-serious_poem.odt | odt | odt | real | small(<16KiB) | nested | 8539 | 31745 | 20069 | 23631 | 2838 | True |
| odfpy-examples-simpletable.odt | odt | odt | real | small(<16KiB) | nested | 7055 | 31250 | 20041 | 23731 | 2818 | True |
| odfpy-examples-userfields_ooo3.odt | odt | odt | real | medium(<256KiB) | nested | 38420 | 98623 | 20524 | 23785 | 3105 | True |
| hostile2-trunc-parquet | parquet | opaque | hostile | small(<16KiB) | hostile | 800 | 4012 | 19172 | 741 | 2806 | True |
| parquet-alltypes-plain.parquet | parquet | parquet | real | small(<16KiB) | simple | 1851 | 9203 | 23665 | 10141 | 2937 | True |
| parquet-alltypes-tiny.parquet | parquet | parquet | real | large(>=256KiB) | complex | 454233 | 914573 | 30784 | 11964 | 4527 | True |
| parquet-data-datapage_v2.snappy.parquet | parquet | parquet | real | small(<16KiB) | nested | 1165 | 6792 | 23642 | 10175 | 3095 | True |
| parquet-data-fixed_length_byte_array.parquet | parquet | parquet | real | small(<16KiB) | nested | 4437 | 12708 | 23501 | 9822 | 2897 | True |
| parquet-data-fixed_length_decimal.parquet | parquet | parquet | real | small(<16KiB) | nested | 677 | 5166 | 23660 | 9883 | 2856 | True |
| parquet-data-float16_nonzeros_and_nans.parquet | parquet | parquet | real | small(<16KiB) | nested | 501 | 4788 | 24889 | 10398 | 3101 | True |
| parquet-data-lz4_raw_compressed_larger.parquet | parquet | parquet | real | large(>=256KiB) | nested | 380836 | 765507 | 29166 | 11777 | 4599 | True |
| parquet-data-nation.dict-malformed.parquet | parquet | parquet | real | small(<16KiB) | nested | 2850 | 9855 | 23549 | 9977 | 2934 | True |
| parquet-delta-binary.parquet | parquet | parquet | real | medium(<256KiB) | complex | 72971 | 161889 | 24646 | 10929 | 4148 | True |
| parquet-geospatial-geography-points.parquet | parquet | parquet | real | medium(<256KiB) | nested | 33026 | 81965 | 23967 | 10843 | 3446 | True |
| asciidoctor-fixtures-hello-asciidoctor.pdf | pdf | pdf | real | small(<16KiB) | nested | 847 | 6557 | 23620 | 828 | 2779 | True |
| cpython-library-turtle-star.pdf | pdf | pdf | real | small(<16KiB) | nested | 4418 | 12438 | 23683 | 647 | 2826 | True |
| netcdf-docs-docmap.pdf | pdf | pdf | real | medium(<256KiB) | nested | 64347 | 78695 | 25093 | 1093 | 3595 | True |
| cargo-cargo.toml | pkgmeta | pkgmeta | real | small(<16KiB) | complex | 7717 | 78906 | 152398 | 50257 | 12598 | True |
| jupyter-package.json | pkgmeta | pkgmeta | real | small(<16KiB) | structured | 3448 | 17137 | 23879 | 9960 | 2898 | True |
| jupyter-py-pyproject.toml | pkgmeta | pkgmeta | real | small(<16KiB) | structured | 7903 | 50514 | 24214 | 10505 | 3026 | True |
| serde-cargo.toml | pkgmeta | pkgmeta | real | small(<16KiB) | moderate | 2431 | 17167 | 131425 | 52689 | 11448 | True |
| vega-package.json | pkgmeta | pkgmeta | real | small(<16KiB) | structured | 2497 | 15674 | 23800 | 14242 | 2932 | True |
| hostile2-trunc-ppt | ppt | cfbf | hostile | small(<16KiB) | hostile | 8000 | 19423 | 23798 | 5445 | 2964 | True |
| poi-slideshow-42474-2.ppt | ppt | ppt | real | large(>=256KiB) | nested | 571392 | 1146204 | 31169 | 7970 | 5329 | True |
| poi-slideshow-54332a.ppt | ppt | ppt | real | medium(<256KiB) | nested | 67584 | 138588 | 24345 | 6862 | 3282 | True |
| poi-slideshow-Single_Coloured_Page.ppt | ppt | ppt | real | small(<16KiB) | nested | 9728 | 22876 | 23915 | 5390 | 2933 | True |
| poi-slideshow-basic_test_ppt_file.ppt | ppt | ppt | real | small(<16KiB) | nested | 15360 | 34140 | 23796 | 5281 | 2843 | True |
| poi-slideshow-bug62092.ppt | ppt | ppt | real | medium(<256KiB) | nested | 161792 | 327004 | 25462 | 5689 | 3507 | True |
| poi-slideshow-datetime.ppt | ppt | ppt | real | medium(<256KiB) | nested | 114176 | 231772 | 25025 | 5729 | 3341 | True |
| hostile2-trunc-pptx | pptx | opaque | hostile | medium(<256KiB) | hostile | 120000 | 123212 | 20837 | 1013 | 3337 | True |
| poi-61515.pptx | pptx | pptx | real | medium(<256KiB) | moderate | 29270 | 99853 | 390782 | 815883 | 51301 | True |
| poi-Divino.pptx | pptx | opaque | real | large(>=256KiB) | complex | 523999 | 527211 | 353346 | 1915 | 48137 | True |
| poi-SampleShow.pptx | pptx | pptx | real | medium(<256KiB) | moderate | 39083 | 132308 | 345347 | 704764 | 44651 | True |
| poi-bug60993.pptx | pptx | pptx | real | medium(<256KiB) | complex | 21942 | 95612 | 394399 | 782809 | 35415 | True |
| poi-slideshow-49386-null_dates.pptx | pptx | pptx | real | medium(<256KiB) | nested | 17124 | 62331 | 20905 | 42332 | 3255 | True |
| poi-slideshow-au.asn.aes.www_conferences_2011_presentations_Fri_20Room4Level4_20 | pptx | pptx | real | large(>=256KiB) | nested | 536086 | 1622797 | 41516 | 45357 | 5648 | True |
| poi-slideshow-bug58144-headers-footers-2007.pptx | pptx | pptx | real | medium(<256KiB) | nested | 42035 | 136662 | 21631 | 42788 | 3333 | True |
| poi-slideshow-bug62736.pptx | pptx | pptx | real | medium(<256KiB) | nested | 50926 | 164564 | 25932 | 42878 | 3567 | True |
| poi-slideshow-bug65551.pptx | pptx | pptx | real | medium(<256KiB) | nested | 34383 | 110096 | 21489 | 42755 | 3196 | True |
| poi-slideshow-pptx2svg.pptx | pptx | pptx | real | medium(<256KiB) | nested | 149172 | 467022 | 23306 | 44999 | 4800 | True |
| poi-table_test.pptx | pptx | pptx | real | medium(<256KiB) | moderate | 28935 | 103361 | 382433 | 1055348 | 57699 | True |
| cpython-faq-gui.rst | rst | rst | real | small(<16KiB) | structured | 3058 | 11685 | 23688 | 13680 | 2972 | True |
| cpython-library-asyncio-protocol.rst | rst | rst | real | medium(<256KiB) | structured | 32975 | 83009 | 28701 | 10293 | 3061 | True |
| cpython-library-mmap.rst | rst | rst | real | small(<16KiB) | structured | 14634 | 35272 | 24762 | 10081 | 3062 | True |
| cpython-library-python.rst | rst | rst | real | small(<16KiB) | structured | 502 | 4725 | 23697 | 9982 | 2868 | True |
| cpython-library-sunau.rst | rst | rst | real | small(<16KiB) | structured | 7423 | 23681 | 23999 | 10154 | 3343 | True |
| cpython-whatsnew-3.3.rst | rst | markdown | real | medium(<256KiB) | structured | 99150 | 238877 | 27101 | 10602 | 3352 | True |
| hostile2-trunc-rtf | rtf | opaque | hostile | small(<16KiB) | hostile | 1000 | 4212 | 19443 | 776 | 2855 | True |
| ne-10m_cultural-10m_us_parks_-READ_ME.rtf | rtf | rtf | real | small(<16KiB) | structured | 2058 | 11954 | 23619 | 9720 | 2833 | True |
| ne-10m_parks_and_protected_areas-10m_us_parks_READ_ME.rtf | rtf | rtf | real | small(<16KiB) | structured | 3718 | 18579 | 23598 | 9860 | 2925 | True |
| poi-quick-contents-message.rtf | rtf | rtf | real | medium(<256KiB) | structured | 25528 | 157229 | 23891 | 10207 | 3166 | True |
| hostile2-magic-toml | toml | toml | hostile | small(<16KiB) | hostile | 37 | 4013 | 23379 | 9862 | 2941 | True |
| toml-array-string-quote-comma-02.toml | toml | toml | real | small(<16KiB) | structured | 20 | 3692 | 23660 | 14577 | 2940 | True |
| toml-datetime-local.toml | toml | toml | real | small(<16KiB) | structured | 88 | 3951 | 24023 | 9892 | 2888 | True |
| toml-key-escapes.toml | toml | toml | real | small(<16KiB) | structured | 163 | 4593 | 23578 | 9784 | 3507 | True |
| toml-spec-1.0.0-float-2.toml | toml | toml | real | small(<16KiB) | structured | 276 | 4701 | 23761 | 9941 | 2873 | True |
| toml-spec-1.1.0-common-0.toml | toml | toml | real | small(<16KiB) | structured | 122 | 3969 | 23389 | 9901 | 2924 | True |
| toml-string-basic-escape-03.toml | toml | toml | real | small(<16KiB) | structured | 56 | 3739 | 23872 | 9881 | 2892 | True |
| toml-valid-implicit-and-explicit-before.toml | toml | toml | real | small(<16KiB) | structured | 37 | 4013 | 23490 | 9861 | 2848 | True |
| toml-valid-multibyte.toml | toml | toml | real | small(<16KiB) | structured | 1156 | 7010 | 23836 | 11158 | 4087 | True |
| hostile2-trunc-xls | xls | cfbf | hostile | small(<16KiB) | hostile | 1500 | 6423 | 23756 | 5891 | 3279 | True |
| poi-spreadsheet-43493.xls | xls | xls | real | medium(<256KiB) | nested | 66048 | 135516 | 24609 | 5763 | 3331 | True |
| poi-spreadsheet-47251_1.xls | xls | xls | real | medium(<256KiB) | nested | 26112 | 102820 | 24480 | 10207 | 3001 | True |
| poi-spreadsheet-48325.xls | xls | xls | real | small(<16KiB) | nested | 4096 | 15039 | 28079 | 10047 | 5581 | True |
| poi-spreadsheet-49524.xls | xls | xls | real | small(<16KiB) | nested | 13824 | 31068 | 23859 | 5495 | 3020 | True |
| poi-spreadsheet-50298.xls | xls | xls | real | medium(<256KiB) | nested | 20480 | 61291 | 24238 | 10424 | 3162 | True |
| poi-spreadsheet-DStar.xls | xls | xls | real | medium(<256KiB) | nested | 35328 | 104549 | 23934 | 10307 | 3135 | True |
| poi-spreadsheet-EmbeddedChartHeaderTest.xls | xls | xls | real | small(<16KiB) | nested | 15360 | 43754 | 23981 | 10147 | 3078 | True |
| poi-spreadsheet-bug66319.xls | xls | xls | real | large(>=256KiB) | nested | 387072 | 777564 | 31108 | 7020 | 4414 | True |
| hostile2-trunc-xlsx | xlsx | opaque | hostile | small(<16KiB) | hostile | 15000 | 18212 | 19195 | 737 | 2947 | True |
| poi-47889.xlsx | xlsx | xlsx | real | small(<16KiB) | simple | 3555 | 19151 | 388879 | 560162 | 117094 | True |
| poi-49609.xlsx | xlsx | xlsx | real | medium(<256KiB) | complex | 105424 | 218879 | 380785 | 80561 | 51251 | True |
| poi-59021.xlsx | xlsx | xlsx | real | small(<16KiB) | moderate | 1933 | 12665 | 403473 | 545593 | 36846 | True |
| poi-DataValidation.xlsx | xlsx | xlsx | real | small(<16KiB) | moderate | 3286 | 18607 | 375668 | 684319 | 46113 | True |
| poi-linkext.xlsx | xlsx | xlsx | real | small(<16KiB) | complex | 3274 | 18583 | 411462 | 769780 | 50222 | True |
| poi-spreadsheet-123233_charts.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 12957 | 47188 | 21067 | 28816 | 3136 | True |
| poi-spreadsheet-56511.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 9688 | 35780 | 20479 | 33634 | 3120 | True |
| poi-spreadsheet-58731.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 9023 | 33113 | 20523 | 28305 | 3064 | True |
| poi-spreadsheet-59021.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 1933 | 12665 | 20575 | 28436 | 3115 | True |
| poi-spreadsheet-StructuredRefs-lots-with-lookups.xlsx | xlsx | xlsx | real | large(>=256KiB) | nested | 325446 | 687123 | 24260 | 30514 | 4347 | True |
| poi-spreadsheet-TwoSheetsOneHidden.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 7962 | 30038 | 20613 | 33351 | 3140 | True |
| poi-spreadsheet-WithChartSheet.xlsx | xlsx | xlsx | real | medium(<256KiB) | nested | 30333 | 99513 | 22971 | 28399 | 3150 | True |
| poi-spreadsheet-duplicate-filename-case-insensitive.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 10721 | 30742 | 20467 | 5568 | 2981 | True |
| maven-core-pom.xml | xml | xml | real | small(<16KiB) | moderate | 7411 | 46651 | 483286 | 241944 | 30589 | True |
| maven-pom.xml | xml | xml | real | medium(<256KiB) | complex | 28489 | 167485 | 404374 | 328066 | 78395 | True |
| poi-quick-contents-quick.xml | xml | xml | real | small(<16KiB) | structured | 143 | 4131 | 25405 | 9858 | 2866 | True |
| poi-spreadsheet-SampleSS.xml | xml | xml | real | small(<16KiB) | structured | 5115 | 36467 | 23469 | 9852 | 2833 | True |
| alertmanager-ci.yml | yaml | yaml | real | small(<16KiB) | simple | 1348 | 9179 | 467809 | 187685 | 56706 | True |
| alertmanager-simple.yml | yaml | yaml | real | small(<16KiB) | moderate | 3953 | 16344 | 468623 | 167939 | 32442 | True |
| cpython-workflows-build.yml | yaml | yaml | real | medium(<256KiB) | structured | 19106 | 65679 | 23675 | 10138 | 3019 | True |
| cpython-workflows-project-updater.yml | yaml | config | real | small(<16KiB) | structured | 741 | 7022 | 23932 | 10116 | 3116 | True |
| cpython-workflows-require-pr-label.yml | yaml | yaml | real | small(<16KiB) | structured | 507 | 5822 | 23746 | 9895 | 2901 | True |
| cpython-workflows-reusable-ubuntu.yml | yaml | yaml | real | small(<16KiB) | structured | 3356 | 14842 | 23396 | 11969 | 3021 | True |
| cpython-workflows-reusable-windows.yml | yaml | yaml | real | small(<16KiB) | structured | 1157 | 7847 | 23496 | 9960 | 3014 | True |
| hostile2-trunc-yaml | yaml | yaml | hostile | small(<16KiB) | hostile | 4000 | 17939 | 23821 | 10079 | 2919 | True |
| prometheus-ci.yml | yaml | yaml | real | small(<16KiB) | complex | 10201 | 39596 | 487910 | 322907 | 66788 | True |
| prometheus-web-config.yml | yaml | yaml | real | small(<16KiB) | simple | 531 | 5022 | 302333 | 171593 | 51680 | True |
| prometheus.yml | yaml | yaml | real | small(<16KiB) | moderate | 934 | 6465 | 111368 | 314617 | 78126 | True |

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
