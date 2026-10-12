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
| 308 | 284 | 24 | 0 | 0 | 308 | 264 | 25 | 41 |

By declared format (ran): arrow=9, asciidoc=7, bib=3, cbor=8, cfbf=5, config=3, container=10, csv=12, doc=9, docx=10, eml=9, epub=3, feed=6, geojson=13, gis=10, hdf=9, html=8, json=11, json5=1, jsonl=9, logstream=6, markdown=6, mdx=7, mhtml=3, msg=7, notebook=8, odp=5, ods=8, odt=8, parquet=11, pdf=3, pkgmeta=5, ppt=7, pptx=12, rst=6, rtf=4, toml=9, xls=9, xlsx=14, xml=4, yaml=11

## Stratified tables

## By declared format

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| arrow | 9 | 200793 | 23054 | 1142785 | 5844 | 43569 | 1281 | 29821 | 2766 | yes |
| asciidoc | 7 | 164803 | 23485 | 369739 | 8386 | 60091 | 9820 | 19980 | 2810 | yes |
| bib | 3 | 73215 | 26064 | 1575364 | 545657 | 23445 | 11157 | 11928 | 3871 | yes |
| cbor | 8 | 166536 | 20983 | 58004 | 5521.0 | 43725 | 5068 | 23141 | 2740 | yes |
| cfbf | 5 | 123207 | 23765 | 1054137 | 134355 | 46774 | 9666 | 16657 | 3092 | yes |
| config | 3 | 73183 | 23162 | 372893 | 7731 | 33441 | 10515 | 8562 | 2739 | yes |
| container | 10 | 222312 | 20068 | 2454600 | 9865.5 | 90425 | 9577 | 33901 | 2722 | yes |
| csv | 12 | 291750 | 23808 | 2500572 | 66873.0 | 124134 | 10367 | 37588 | 2805 | yes |
| doc | 9 | 213823 | 23534 | 1365346 | 57692 | 56361 | 6011 | 27964 | 2896 | yes |
| docx | 10 | 203288 | 20052 | 1290131 | 69418.0 | 330178 | 37118 | 31227 | 2998 | yes |
| eml | 9 | 368039 | 23226 | 93592 | 6266 | 78469 | 9661 | 26026 | 2781 | yes |
| epub | 3 | 60422 | 20080 | 135781 | 33764 | 22855 | 9634 | 8347 | 2808 | yes |
| feed | 6 | 140003 | 23214 | 59015 | 8622.5 | 58038 | 9641 | 17297 | 2731 | yes |
| geojson | 13 | 338886 | 24897 | 9615938 | 431186 | 144180 | 10715 | 44417 | 3320 | yes |
| gis | 10 | 241785 | 23408 | 2240691 | 11066.0 | 97030 | 9843 | 30958 | 2794 | yes |
| hdf | 9 | 205888 | 23093 | 424354 | 14926 | 51979 | 5245 | 25675 | 2786 | yes |
| html | 8 | 194038 | 23664 | 430673 | 23085.5 | 78449 | 9806 | 22621 | 2815 | yes |
| json | 11 | 275792 | 23255 | 3263643 | 14189 | 111213 | 10044 | 35596 | 3038 | yes |
| json5 | 1 | 23111 | 23111 | 3852 | 3852 | 9717 | 9717 | 2733 | 2733 | yes |
| jsonl | 9 | 260065 | 30368 | 12464612 | 1837044 | 125948 | 15471 | 37178 | 4430 | yes |
| logstream | 6 | 197356 | 33472 | 2948319 | 281400.0 | 29503 | 1349 | 23176 | 3756 | yes |
| markdown | 6 | 155098 | 24672 | 630422 | 18609.0 | 64975 | 10067 | 19281 | 2871 | yes |
| mdx | 7 | 167279 | 23392 | 172724 | 13099 | 69505 | 9734 | 19606 | 2831 | yes |
| mhtml | 3 | 69963 | 23176 | 45097 | 7936 | 26812 | 9764 | 9204 | 2734 | yes |
| msg | 7 | 165885 | 23456 | 573955 | 49500 | 38187 | 5177 | 21939 | 2807 | yes |
| notebook | 8 | 187737 | 23342 | 237285 | 7683.0 | 81296 | 9850 | 24079 | 2838 | yes |
| odp | 5 | 102247 | 20487 | 680037 | 138597 | 218809 | 40940 | 15059 | 3050 | yes |
| ods | 8 | 160706 | 19926 | 253580 | 30052.5 | 195950 | 23546 | 22915 | 2835 | yes |
| odt | 8 | 157324 | 19800 | 296774 | 31907.5 | 162048 | 23060 | 23154 | 2798 | yes |
| parquet | 11 | 262197 | 23146 | 1976458 | 9855 | 100602 | 9656 | 33855 | 2738 | yes |
| pdf | 3 | 71326 | 23417 | 97690 | 12438 | 1474 | 477 | 9539 | 2987 | yes |
| pkgmeta | 5 | 116825 | 23297 | 179398 | 17167 | 48937 | 9669 | 13839 | 2759 | yes |
| ppt | 7 | 173029 | 23951 | 1920007 | 138588 | 41625 | 5436 | 24030 | 2906 | yes |
| pptx | 12 | 281636 | 21033 | 3645029 | 127760.0 | 437485 | 42390 | 39593 | 2976 | yes |
| rst | 6 | 150027 | 24342 | 397249 | 29476.5 | 60516 | 9776 | 17435 | 2878 | yes |
| rtf | 4 | 88928 | 23304 | 191974 | 15266.5 | 29763 | 9580 | 11067 | 2760 | yes |
| toml | 9 | 208267 | 23202 | 39681 | 4013 | 91238 | 9622 | 26673 | 2755 | yes |
| xls | 9 | 216106 | 23531 | 1278024 | 61291 | 73357 | 9278 | 29382 | 2833 | yes |
| xlsx | 14 | 288535 | 20290 | 1282259 | 30390.0 | 339247 | 28484 | 42269 | 2938 | yes |
| xml | 4 | 94860 | 23720 | 254734 | 41559.0 | 39842 | 9954 | 11559 | 2906 | yes |
| yaml | 11 | 256343 | 23230 | 195757 | 9179 | 107902 | 9722 | 31015 | 2754 | yes |

## By size class

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| large(>=256KiB) | 25 | 792570 | 30367 | 35943385 | 1146204 | 335644 | 13604 | 118196 | 4676 | yes |
| medium(<256KiB) | 86 | 2075195 | 23806 | 18738444 | 134935.5 | 1280992 | 10102 | 268462 | 3010 | yes |
| small(<16KiB) | 197 | 4544848 | 23162 | 3530346 | 11954 | 2272458 | 9705 | 573628 | 2780 | yes |

## By complexity

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| complex | 16 | 422987 | 24238 | 12213569 | 178642.5 | 258216 | 10833 | 60592 | 3149 | yes |
| hostile | 24 | 481036 | 18862 | 341455 | 6083.5 | 86331 | 510 | 68529 | 2770 | yes |
| moderate | 19 | 595333 | 23352 | 2368557 | 44661 | 365986 | 10361 | 57280 | 2907 | yes |
| nested | 114 | 2590919 | 23077 | 16830603 | 36496.0 | 1695465 | 9645 | 359461 | 2855 | yes |
| simple | 37 | 922895 | 23659 | 5698094 | 23143 | 463596 | 9947 | 115630 | 2927 | yes |
| structured | 98 | 2399443 | 23389 | 20759897 | 14515.5 | 1019500 | 9793 | 298794 | 2817 | yes |

## By stratum (real vs malformed/hostile)

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| hostile | 24 | 481036 | 18862 | 341455 | 6083.5 | 86331 | 510 | 68529 | 2770 | yes |
| real | 284 | 6931577 | 23290 | 57870720 | 30843.0 | 3802763 | 9843 | 891757 | 2864 | yes |

### Workload dimension

`narrow` = one `observe --metadata --kind metadata` per document (a bounded metadata projection).
`full` = one `materialize --exact --packed` per document (byte-authority closure).
Both are pooled and median; a size/time-weighted world is `pooled`, an unweighted per-document world is `median` (ADR-0054: a claim names its estimator).

**Detection.** 31/308 samples were detected as `opaque` by the bounded detector; 65 samples declined the narrow observation. The byte-authority gate is unaffected by detection; every sample is byte-exactly closed.

## Findings (detector choice != declared format)

A **finding** is a real sample whose detector choice differs from its declared format: a conservative `decline-opaque`, or a `misclassified` claim by another detector. Every finding is reconciled in the phase report (fixed-with-test, or recorded as a typed decline / recorded misclassification with its reason). The byte-authority closure is unaffected by any finding.

Summary by kind: decline-opaque=14, misclassified=11

| id | declared | detected | kind | resolution |
|---|---|---|---|---|
| arrow-stream-1.0.0-littleendian-generated_decimal256.stream | arrow | opaque | decline-opaque | deferred to 27.5 — a bare Arrow IPC *stream* (no leading `ARROW1` magic); the arrow detector claims the file format (magic+footer) and magic-prefixed streams only (src/adapter/arrow.rs). The 27.5 Arrow-stream subphase owns bare-stream detection; the whole-source exact leaf is unaffected. |
| arrow-stream-1.0.0-littleendian-generated_nested_dictionary.stream | arrow | opaque | decline-opaque | deferred to 27.5 — a bare Arrow IPC *stream* (no leading `ARROW1` magic); the arrow detector claims the file format (magic+footer) and magic-prefixed streams only (src/adapter/arrow.rs). The 27.5 Arrow-stream subphase owns bare-stream detection; the whole-source exact leaf is unaffected. |
| arrow-stream-1.0.0-littleendian-generated_null_trivial.stream | arrow | opaque | decline-opaque | deferred to 27.5 — a bare Arrow IPC *stream* (no leading `ARROW1` magic); the arrow detector claims the file format (magic+footer) and magic-prefixed streams only (src/adapter/arrow.rs). The 27.5 Arrow-stream subphase owns bare-stream detection; the whole-source exact leaf is unaffected. |
| arrow-stream-cpp-21.0.0-generated_custom_metadata.stream | arrow | opaque | decline-opaque | deferred to 27.5 — a bare Arrow IPC *stream* (no leading `ARROW1` magic); the arrow detector claims the file format (magic+footer) and magic-prefixed streams only (src/adapter/arrow.rs). The 27.5 Arrow-stream subphase owns bare-stream detection; the whole-source exact leaf is unaffected. |
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
`gis_adapter::detection_is_byte_based_and_conservative`. Bare Arrow IPC
streams are deferred to Phase 27.5.

No real sample was skipped.

### Per-sample

| id | declared | detected | stratum | size class | complexity | src B | store B | build us | narrow us | full us | exact |
|---|---|---|---|---|---|---:|---:|---:|---:|---:|---|
| arrow-file-1.0.0-bigendian-generated_decimal256.arrow_file | arrow | arrow | real | large(>=256KiB) | nested | 363106 | 736384 | 27060 | 10752 | 4171 | True |
| arrow-file-1.0.0-littleendian-generated_union.arrow_file | arrow | arrow | real | small(<16KiB) | nested | 3562 | 11806 | 23114 | 9519 | 2730 | True |
| arrow-file-cpp-21.0.0-generated_dictionary_unsigned.arrow_file | arrow | arrow | real | small(<16KiB) | nested | 2170 | 8335 | 23274 | 9589 | 2759 | True |
| arrow-file-cpp-21.0.0-generated_null_trivial.arrow_file | arrow | arrow | real | small(<16KiB) | nested | 514 | 4774 | 23054 | 10316 | 3820 | True |
| arrow-stream-1.0.0-littleendian-generated_decimal256.stream | arrow | opaque | real | large(>=256KiB) | nested | 360464 | 363676 | 29603 | 1281 | 4034 | True |
| arrow-stream-1.0.0-littleendian-generated_nested_dictionary.stream | arrow | opaque | real | small(<16KiB) | nested | 2632 | 5844 | 19213 | 828 | 4062 | True |
| arrow-stream-1.0.0-littleendian-generated_null_trivial.stream | arrow | opaque | real | small(<16KiB) | nested | 312 | 3524 | 18399 | 402 | 2739 | True |
| arrow-stream-cpp-21.0.0-generated_custom_metadata.stream | arrow | opaque | real | small(<16KiB) | nested | 1504 | 4716 | 18613 | 421 | 2740 | True |
| hostile2-magic-arrow | arrow | opaque | hostile | small(<16KiB) | hostile | 514 | 3726 | 18463 | 461 | 2766 | True |
| asciidoctor-CHANGELOG.adoc | asciidoc | rst | real | medium(<256KiB) | structured | 140952 | 307359 | 29176 | 10624 | 3363 | True |
| asciidoctor-cli-nav.adoc | asciidoc | opaque | real | small(<16KiB) | structured | 222 | 3434 | 18398 | 494 | 2699 | True |
| asciidoctor-locale-attributes-ja.adoc | asciidoc | rst | real | small(<16KiB) | structured | 732 | 6089 | 23287 | 9892 | 2810 | True |
| asciidoctor-locale-attributes-sr.adoc | asciidoc | rst | real | small(<16KiB) | structured | 863 | 6641 | 23552 | 9594 | 2866 | True |
| asciidoctor-pages-contexts-ref.adoc | asciidoc | asciidoc | real | small(<16KiB) | structured | 3640 | 15247 | 23518 | 9709 | 2712 | True |
| asciidoctor-pages-macos.adoc | asciidoc | rst | real | small(<16KiB) | structured | 1577 | 8386 | 23387 | 9958 | 2823 | True |
| asciidoctor-pages-stylesheet-modes.adoc | asciidoc | rst | real | small(<16KiB) | structured | 7284 | 22583 | 23485 | 9820 | 2707 | True |
| bibtex-build-strings-base.bib | bib | bib | real | medium(<256KiB) | structured | 145131 | 545657 | 26064 | 11157 | 3239 | True |
| bibtex-strings.bib | bib | bib | real | large(>=256KiB) | structured | 271806 | 1025695 | 28490 | 11835 | 4818 | True |
| hostile2-trunc-bib | bib | opaque | hostile | small(<16KiB) | hostile | 800 | 4012 | 18661 | 453 | 3871 | True |
| cbor-data-all_types.cbor | cbor | cbor | real | small(<16KiB) | nested | 354 | 6300 | 23304 | 9686 | 2841 | True |
| cbor-data-integer.cbor | cbor | opaque | real | small(<16KiB) | nested | 2 | 3214 | 18294 | 454 | 2669 | True |
| cbor-data-tagged_date.cbor | cbor | opaque | real | small(<16KiB) | nested | 22 | 3234 | 18225 | 407 | 2787 | True |
| cbor-rfc8949-appendixA-mt2.cbor | cbor | cbor | real | small(<16KiB) | nested | 177 | 4742 | 23151 | 12898 | 3962 | True |
| cbor-rfc8949-appendixA-mt3.cbor | cbor | cbor | real | small(<16KiB) | nested | 487 | 6867 | 23169 | 9673 | 2764 | True |
| cbor-rfc8949-appendixA-streaming.cbor | cbor | cbor | real | small(<16KiB) | nested | 1145 | 13386 | 23003 | 9726 | 2685 | True |
| cbor-rfc8949-good.cbor | cbor | opaque | real | small(<16KiB) | nested | 13797 | 17009 | 18963 | 419 | 2716 | True |
| hostile2-trunc-cbor | cbor | opaque | hostile | small(<16KiB) | hostile | 40 | 3252 | 18427 | 462 | 2717 | True |
| poi-hpsf-TestVisioWithCodepage.vsd | cfbf | cfbf | real | medium(<256KiB) | nested | 45568 | 95579 | 23523 | 11843 | 4020 | True |
| poi-ole-poifs-only-zero-byte-streams.ole2 | cfbf | cfbf | real | small(<16KiB) | nested | 1536 | 7059 | 23000 | 9363 | 2709 | True |
| poi-publisher-LinkAt20.pub | cfbf | cfbf | real | medium(<256KiB) | nested | 64512 | 134355 | 23765 | 9666 | 2968 | True |
| poi-publisher-Sample_2010.pub | cfbf | cfbf | real | medium(<256KiB) | nested | 72704 | 150739 | 23827 | 9865 | 3092 | True |
| poi-publisher-clusterfuzz-testcase-minimized-POIHPBFFuzzer-4701121678278656.pub | cfbf | cfbf | real | large(>=256KiB) | nested | 331491 | 666405 | 29092 | 6037 | 3868 | True |
| cpython-libregrtest-mypy.ini | config | config | real | small(<16KiB) | simple | 997 | 7731 | 23029 | 13243 | 2695 | True |
| poi-gradle.properties | config | config | real | small(<16KiB) | simple | 555 | 5630 | 23162 | 9683 | 2739 | True |
| poi-resources-font_metrics.properties | config | config | real | medium(<256KiB) | simple | 146498 | 359532 | 26992 | 10515 | 3128 | True |
| cpython-namespace_pkgs-missing_directory.zip | container | container | real | small(<16KiB) | nested | 515 | 7457 | 19676 | 9523 | 2714 | True |
| cpython-test-recursion.tar | container | container | real | small(<16KiB) | nested | 516 | 4470 | 22958 | 5090 | 2707 | True |
| cpython-test-zipdir_backslash.zip | container | container | real | small(<16KiB) | nested | 192 | 6019 | 19722 | 9515 | 2689 | True |
| cpython-zipdata01-ziptestdata.zip | container | container | real | small(<16KiB) | nested | 876 | 9797 | 19930 | 12102 | 2684 | True |
| hostile2-trunc-container | container | opaque | hostile | small(<16KiB) | hostile | 5000 | 8212 | 18574 | 588 | 2660 | True |
| ne-zip-make-graticules-and-bounding-box-ne_1d5_graticules_beta2.zip | container | container | real | large(>=256KiB) | nested | 1000316 | 2055115 | 40390 | 13675 | 7448 | True |
| ne-zip-zip-ne_50m_graticules_1.zip | container | container | real | medium(<256KiB) | nested | 128416 | 264104 | 20759 | 10129 | 3245 | True |
| ne-zip-zip-ne_50m_graticules_15.zip | container | container | real | small(<16KiB) | nested | 10012 | 27300 | 19794 | 9543 | 2729 | True |
| ne-zip-zip-ne_50m_graticules_5.zip | container | container | real | medium(<256KiB) | nested | 27460 | 62192 | 20207 | 9611 | 2850 | True |
| poi-openxml4j-dcterms_bug_56479.zip | container | container | real | small(<16KiB) | nested | 1277 | 9934 | 20302 | 10649 | 4175 | True |
| parquet-delta-expect.csv | csv | csv | real | medium(<256KiB) | moderate | 159803 | 552658 | 25326 | 10423 | 3821 | True |
| vega-data-airports.csv | csv | csv | real | medium(<256KiB) | simple | 210363 | 893639 | 26546 | 11489 | 4817 | True |
| vega-data-co2-concentration.csv | csv | csv | real | medium(<256KiB) | simple | 18547 | 93286 | 26156 | 9891 | 2614 | True |
| vega-data-flights-airport.csv | csv | csv | real | medium(<256KiB) | simple | 65572 | 515711 | 24586 | 10950 | 3007 | True |
| vega-data-gapminder-health-income.csv | csv | csv | real | small(<16KiB) | simple | 8605 | 40460 | 23253 | 10485 | 3839 | True |
| vega-data-github.csv | csv | csv | real | medium(<256KiB) | simple | 21059 | 97252 | 23659 | 11489 | 2577 | True |
| vega-data-iowa-electricity.csv | csv | csv | real | small(<16KiB) | simple | 1531 | 10264 | 25161 | 9647 | 2728 | True |
| vega-data-population_engineers_hurricanes.csv | csv | csv | real | small(<16KiB) | simple | 1852 | 12779 | 23047 | 9557 | 2724 | True |
| vega-global-temp.csv | csv | csv | real | small(<16KiB) | simple | 1663 | 14668 | 23292 | 9689 | 2753 | True |
| vega-iowa-electricity.csv | csv | csv | real | small(<16KiB) | simple | 1531 | 10264 | 23264 | 9551 | 2695 | True |
| vega-population-engineers.csv | csv | csv | real | small(<16KiB) | simple | 1852 | 12779 | 23502 | 10311 | 3156 | True |
| vega-tsv-data-unemployment.tsv | csv | csv | real | medium(<256KiB) | simple | 34739 | 246812 | 23958 | 10652 | 2857 | True |
| hostile2-magic-doc | doc | opaque | hostile | medium(<256KiB) | hostile | 17920 | 21132 | 19252 | 590 | 2896 | True |
| poi-document-Bug41898.doc | doc | doc | real | medium(<256KiB) | nested | 24064 | 51548 | 23158 | 5180 | 2809 | True |
| poi-document-Bug46817.doc | doc | doc | real | medium(<256KiB) | nested | 17920 | 39260 | 24775 | 6011 | 2751 | True |
| poi-document-Bug53380_2.doc | doc | doc | real | medium(<256KiB) | nested | 31744 | 66908 | 23534 | 4949 | 2663 | True |
| poi-document-Bug60942b.doc | doc | cfbf | real | small(<16KiB) | nested | 6144 | 16721 | 23132 | 9605 | 2786 | True |
| poi-document-ThreeColHead.doc | doc | doc | real | medium(<256KiB) | nested | 27136 | 57692 | 23231 | 7511 | 3587 | True |
| poi-document-cap.stanford.edu_profiles_viewbiosketch_facultyid-4009-name-m_maciv | doc | doc | real | medium(<256KiB) | nested | 47104 | 97628 | 23787 | 5553 | 3061 | True |
| poi-document-parentinvguid.doc | doc | doc | real | large(>=256KiB) | nested | 398848 | 801116 | 28683 | 6658 | 4179 | True |
| poi-document-testRangeDelete.doc | doc | cfbf | real | medium(<256KiB) | nested | 104448 | 213341 | 24271 | 10304 | 3232 | True |
| hostile2-magic-docx | docx | opaque | hostile | small(<16KiB) | hostile | 10045 | 13257 | 19026 | 533 | 2763 | True |
| hostile2-trunc-docx | docx | docx | hostile | small(<16KiB) | hostile | 2297 | 14328 | 20078 | 27715 | 2716 | True |
| poi-document-58618.docx | docx | docx | real | small(<16KiB) | nested | 10045 | 52985 | 19888 | 38647 | 4111 | True |
| poi-document-60329.docx | docx | docx | real | medium(<256KiB) | nested | 31124 | 154025 | 20751 | 37302 | 3053 | True |
| poi-document-Bug51170.docx | docx | docx | real | medium(<256KiB) | nested | 239621 | 720218 | 22766 | 40399 | 3694 | True |
| poi-document-Bug60341.docx | docx | docx | real | small(<16KiB) | nested | 2297 | 14328 | 19759 | 27802 | 2965 | True |
| poi-document-NumberingWOverrides.docx | docx | docx | real | small(<16KiB) | nested | 15190 | 72123 | 20275 | 36847 | 3031 | True |
| poi-document-ThreeColHeadFoot.docx | docx | docx | real | small(<16KiB) | nested | 13123 | 66713 | 20025 | 42256 | 3264 | True |
| poi-document-capitalized.docx | docx | docx | real | small(<16KiB) | nested | 12104 | 73162 | 19888 | 36934 | 2777 | True |
| poi-document-checkboxes.docx | docx | docx | real | medium(<256KiB) | nested | 20800 | 108992 | 20832 | 41743 | 2853 | True |
| cpython-data-msg_08.txt | eml | eml | real | small(<16KiB) | structured | 454 | 5868 | 23077 | 9571 | 2715 | True |
| cpython-data-msg_22.txt | eml | eml | real | small(<16KiB) | structured | 1894 | 9580 | 23226 | 9661 | 2674 | True |
| cpython-data-msg_23.txt | eml | eml | real | small(<16KiB) | structured | 139 | 4246 | 23316 | 9788 | 2781 | True |
| cpython-data-msg_33.txt | eml | eml | real | small(<16KiB) | structured | 750 | 6266 | 22973 | 9595 | 2715 | True |
| cpython-data-msg_34.txt | eml | eml | real | small(<16KiB) | structured | 300 | 4725 | 23221 | 9615 | 2674 | True |
| cpython-data-msg_43.txt | eml | eml | real | small(<16KiB) | structured | 9166 | 24520 | 23235 | 9675 | 2864 | True |
| cpython-msg-01.eml | eml | eml | real | small(<16KiB) | simple | 459 | 5533 | 23175 | 9765 | 3713 | True |
| cpython-msg-25.eml | eml | opaque | real | small(<16KiB) | moderate | 5122 | 8334 | 182319 | 785 | 2907 | True |
| cpython-msg-43.eml | eml | eml | real | small(<16KiB) | complex | 9166 | 24520 | 23497 | 10014 | 2983 | True |
| hostile2-magic-epub | epub | opaque | hostile | medium(<256KiB) | hostile | 30552 | 33764 | 20035 | 510 | 2823 | True |
| tika-test-documents-testEPUB.epub | epub | epub | real | medium(<256KiB) | nested | 30552 | 72075 | 20080 | 12711 | 2808 | True |
| tika-test-documents-testEPUB_xml_ext.epub | epub | epub | real | small(<16KiB) | nested | 7823 | 29942 | 20307 | 9634 | 2716 | True |
| feed-1.1-extensive-noerror.xml | feed | feed | real | small(<16KiB) | structured | 1705 | 13105 | 23427 | 9533 | 2718 | True |
| feed-1.1-extensive-nowarn.xml | feed | feed | real | small(<16KiB) | structured | 1747 | 13189 | 24009 | 9553 | 2801 | True |
| feed-a33-3.3-duplicate-updated.xml | feed | feed | real | small(<16KiB) | structured | 1015 | 9234 | 23224 | 9754 | 2723 | True |
| feed-a33-3.3-published_no_year.xml | feed | feed | real | small(<16KiB) | structured | 875 | 7971 | 23052 | 9916 | 3634 | True |
| feed-a33-3.3-published_seconds.xml | feed | feed | real | small(<16KiB) | structured | 895 | 8011 | 23204 | 9595 | 2739 | True |
| feed-a33-3.3-updated-example2.xml | feed | feed | real | small(<16KiB) | structured | 706 | 7505 | 23087 | 9687 | 2682 | True |
| hostile2-trunc-geojson | geojson | opaque | hostile | small(<16KiB) | hostile | 2000 | 5212 | 18881 | 480 | 2769 | True |
| natural-earth-countries.geojson | geojson | geojson | real | large(>=256KiB) | complex | 838726 | 4054747 | 39745 | 18782 | 5644 | True |
| natural-earth-places.geojson | geojson | geojson | real | medium(<256KiB) | moderate | 166071 | 830240 | 26366 | 11850 | 3544 | True |
| ne-bbox.geojson | geojson | geojson | real | small(<16KiB) | simple | 4813 | 34888 | 23833 | 9901 | 2792 | True |
| ne-geojson-ne_10m_admin_0_antarctic_claim_limit_lines.geojson | geojson | geojson | real | medium(<256KiB) | structured | 58099 | 431186 | 25919 | 14415 | 3042 | True |
| ne-geojson-ne_10m_bathymetry_A_10000.geojson | geojson | geojson | real | small(<16KiB) | structured | 2860 | 18223 | 24348 | 9897 | 2789 | True |
| ne-geojson-ne_10m_parks_and_protected_lands_scale_rank.geojson | geojson | geojson | real | medium(<256KiB) | structured | 202844 | 1032397 | 28082 | 12581 | 3565 | True |
| ne-geojson-ne_110m_admin_0_boundary_lines_land.geojson | geojson | geojson | real | large(>=256KiB) | structured | 340010 | 1722622 | 30001 | 13604 | 4026 | True |
| ne-geojson-ne_110m_admin_1_states_provinces_lakes.geojson | geojson | geojson | real | medium(<256KiB) | structured | 181275 | 868587 | 26707 | 11956 | 3731 | True |
| ne-geojson-ne_110m_glaciated_areas.geojson | geojson | geojson | real | medium(<256KiB) | structured | 21720 | 116257 | 23496 | 9868 | 2670 | True |
| ne-geojson-ne_50m_geography_regions_elevation_points.geojson | geojson | geojson | real | medium(<256KiB) | structured | 104139 | 438795 | 24897 | 10715 | 3320 | True |
| ne-lakes.geojson | geojson | geojson | real | small(<16KiB) | moderate | 7719 | 43208 | 23352 | 10361 | 3623 | True |
| ne-regions.geojson | geojson | geojson | real | small(<16KiB) | simple | 4099 | 19576 | 23259 | 9770 | 2902 | True |
| hostile2-trunc-gis | gis | opaque | hostile | small(<16KiB) | hostile | 200 | 3412 | 18344 | 454 | 2772 | True |
| togeojson-data-bad.gpx | gis | gis | real | small(<16KiB) | structured | 833 | 8679 | 23436 | 9852 | 2820 | True |
| togeojson-data-blue_hills.gpx | gis | gis | real | large(>=256KiB) | structured | 268694 | 2069245 | 35701 | 14205 | 4994 | True |
| togeojson-data-null_geometry.kml | gis | gis | real | small(<16KiB) | structured | 286 | 4889 | 23087 | 10244 | 3771 | True |
| togeojson-data-opacity_override.kml | gis | gis | real | small(<16KiB) | structured | 625 | 7617 | 23183 | 9834 | 2759 | True |
| togeojson-data-opencpn.gpx | gis | gis | real | small(<16KiB) | structured | 10637 | 66507 | 23718 | 9785 | 2761 | True |
| togeojson-data-point_extra_coordinates.kml | gis | gis | real | small(<16KiB) | structured | 386 | 5465 | 24050 | 9736 | 2816 | True |
| togeojson-data-schema.kml | gis | gis | real | small(<16KiB) | structured | 2158 | 16426 | 23335 | 9898 | 2818 | True |
| togeojson-data-style.kml | gis | gis | real | small(<16KiB) | structured | 1372 | 13453 | 23380 | 9752 | 2759 | True |
| togeojson-data-styles.kml | gis | gis | real | small(<16KiB) | structured | 6374 | 44998 | 23551 | 13270 | 2688 | True |
| hostile2-trunc-hdf | hdf | opaque | hostile | small(<16KiB) | hostile | 600 | 3812 | 18506 | 463 | 2753 | True |
| netcdf-h5-h5_test-ref_tst_h_compounds2.h5 | hdf | hdf | real | small(<16KiB) | nested | 557 | 4534 | 23008 | 5047 | 2694 | True |
| netcdf-h5-nczarr_test-ref_groups.h5 | hdf | hdf | real | small(<16KiB) | nested | 9836 | 23282 | 23123 | 9549 | 2887 | True |
| netcdf-ncdump-ref_nc_test_netcdf4_4_0.nc | hdf | hdf | real | medium(<256KiB) | nested | 162812 | 329044 | 25440 | 5578 | 3359 | True |
| netcdf-ncdump-ref_no_ncproperty.nc | hdf | hdf | real | small(<16KiB) | nested | 1262 | 5944 | 23267 | 5245 | 2786 | True |
| netcdf-ncdump-ref_test_365_day_1900.nc | hdf | hdf | real | small(<16KiB) | nested | 9348 | 22248 | 23321 | 9567 | 2974 | True |
| netcdf-ncdump-ref_tst_compounds2.nc | hdf | hdf | real | small(<16KiB) | nested | 5753 | 14926 | 23060 | 6263 | 2729 | True |
| netcdf-ncdump-ref_tst_compounds4.nc | hdf | hdf | real | small(<16KiB) | nested | 526 | 4472 | 23070 | 5115 | 2802 | True |
| netcdf-nctestfiles-test_vlen2.nc | hdf | hdf | real | small(<16KiB) | nested | 6336 | 16092 | 23093 | 5152 | 2691 | True |
| feedval-05-version_12_supports_atom_02.html | html | html | real | medium(<256KiB) | structured | 47054 | 110374 | 27774 | 9804 | 3002 | True |
| feedval-21-live.html | html | html | real | small(<16KiB) | structured | 2277 | 19579 | 23445 | 9809 | 2672 | True |
| feedval-22-known_bugs.html | html | html | real | small(<16KiB) | structured | 3478 | 24861 | 23242 | 9765 | 2766 | True |
| feedval-29-new_mailing_list_for_validator_users.html | html | html | real | small(<16KiB) | structured | 2823 | 21310 | 23287 | 9626 | 2727 | True |
| mdn-doc-structure.html | html | html | real | small(<16KiB) | moderate | 3525 | 20244 | 23739 | 9819 | 2864 | True |
| mdn-fonts-demo.html | html | html | real | medium(<256KiB) | complex | 36867 | 189800 | 25224 | 9872 | 2943 | True |
| mdn-getting-started.html | html | html | real | small(<16KiB) | simple | 224 | 5429 | 23683 | 10007 | 2927 | True |
| mdn-planets.html | html | html | real | small(<16KiB) | moderate | 4302 | 39076 | 23644 | 9747 | 2720 | True |
| cpython-fuzz_json_loads_corpus-empty_array.json | json | json | real | small(<16KiB) | structured | 3 | 3509 | 23174 | 9603 | 2701 | True |
| cpython-fuzz_json_loads_corpus-pass1.json | json | json | real | small(<16KiB) | structured | 1441 | 8910 | 23131 | 9532 | 2694 | True |
| cpython-fuzz_json_loads_corpus-simple_array.json | json | json | real | small(<16KiB) | structured | 25 | 3678 | 23176 | 13007 | 3829 | True |
| hostile2-magic-json | json | opaque | hostile | small(<16KiB) | hostile | 2743 | 5955 | 18748 | 500 | 2774 | True |
| vega-data-barley.json | json | json | real | small(<16KiB) | structured | 8487 | 47477 | 23255 | 11964 | 3872 | True |
| vega-data-budget.json | json | json | real | large(>=256KiB) | structured | 391353 | 1645334 | 33787 | 13856 | 4186 | True |
| vega-data-burtin.json | json | json | real | small(<16KiB) | structured | 2743 | 14189 | 25971 | 9670 | 2731 | True |
| vega-data-penguins.json | json | json | real | medium(<256KiB) | structured | 67119 | 266741 | 25268 | 10517 | 3038 | True |
| vega-data-volcano.json | json | json | real | medium(<256KiB) | structured | 21167 | 178662 | 23987 | 10044 | 2851 | True |
| vega-data-weekly-weather.json | json | json | real | small(<16KiB) | structured | 1281 | 12815 | 23130 | 9750 | 3641 | True |
| vega-data-world-110m.json | json | json | real | medium(<256KiB) | structured | 119410 | 1076373 | 32165 | 12770 | 3279 | True |
| mdx-js-renovate.json5 | json5 | json5 | real | small(<16KiB) | structured | 92 | 3852 | 23111 | 9717 | 2733 | True |
| jsonlines-datagov100.json | jsonl | jsonl | real | large(>=256KiB) | complex | 696007 | 2253927 | 34232 | 17951 | 5362 | True |
| jsonlines-datagov100.json | jsonl | jsonl | real | large(>=256KiB) | structured | 696007 | 2253927 | 34961 | 15471 | 5355 | True |
| oai-data-dbpedia_samples.jsonl | jsonl | jsonl | real | medium(<256KiB) | structured | 64512 | 167137 | 24239 | 10410 | 3159 | True |
| oai-data-drone_training.jsonl | jsonl | jsonl | real | large(>=256KiB) | structured | 387706 | 1837044 | 30368 | 17300 | 4430 | True |
| oai-data-example_requests_to_parallel_process.jsonl | jsonl | jsonl | real | large(>=256KiB) | structured | 548917 | 2831447 | 32179 | 17905 | 4835 | True |
| oai-data-toy_chat_fine_tuning.jsonl | jsonl | jsonl | real | medium(<256KiB) | structured | 27385 | 61273 | 24084 | 9985 | 3036 | True |
| oai-dbpedia.jsonl | jsonl | jsonl | real | medium(<256KiB) | moderate | 64512 | 167137 | 23999 | 9877 | 2927 | True |
| oai-parallel-requests.jsonl | jsonl | jsonl | real | large(>=256KiB) | complex | 548917 | 2831447 | 32647 | 17403 | 5170 | True |
| oai-toy-chat.jsonl | jsonl | jsonl | real | medium(<256KiB) | moderate | 27385 | 61273 | 23356 | 9646 | 2904 | True |
| loghub-HPC-HPC_2k.log | logstream | opaque | real | medium(<256KiB) | simple | 151178 | 154390 | 30583 | 745 | 3117 | True |
| loghub-Mac-Mac_2k.log | logstream | opaque | real | large(>=256KiB) | simple | 319414 | 322626 | 36302 | 1512 | 3872 | True |
| loghub-OpenStack-OpenStack_2k.log | logstream | config | real | large(>=256KiB) | simple | 595119 | 1329738 | 35837 | 13653 | 5208 | True |
| loghub-Proxifier-Proxifier_2k.log | logstream | opaque | real | medium(<256KiB) | simple | 236962 | 240174 | 32979 | 1048 | 3704 | True |
| loghub-Spark-Spark_2k.log | logstream | opaque | real | medium(<256KiB) | simple | 196268 | 199480 | 33965 | 1186 | 3466 | True |
| loghub-Windows-Windows_2k.log | logstream | csv | real | large(>=256KiB) | simple | 285433 | 701911 | 27690 | 11359 | 3809 | True |
| commonmark-readme.md | markdown | markdown | real | small(<16KiB) | simple | 7671 | 23143 | 24530 | 10224 | 3194 | True |
| commonmark-spec.txt | markdown | markdown | real | medium(<256KiB) | complex | 205025 | 530161 | 34267 | 15378 | 4887 | True |
| cpython-ISSUE_TEMPLATE-crash.md | markdown | markdown | real | small(<16KiB) | simple | 1142 | 6380 | 23323 | 9574 | 2720 | True |
| cpython-rhel7-README.md | markdown | markdown | real | small(<16KiB) | simple | 608 | 5323 | 23094 | 9532 | 2738 | True |
| cpython-wasm-README.md | markdown | markdown | real | medium(<256KiB) | simple | 16560 | 51340 | 25069 | 10357 | 2884 | True |
| rust-readme.md | markdown | markdown | real | small(<16KiB) | simple | 3137 | 14075 | 24815 | 9910 | 2858 | True |
| mdx-blog-conf.mdx | mdx | mdx | real | small(<16KiB) | structured | 2790 | 12397 | 23175 | 9503 | 2665 | True |
| mdx-blog-v1.mdx | mdx | mdx | real | small(<16KiB) | structured | 9150 | 33026 | 23392 | 9740 | 2831 | True |
| mdx-community-sponsor.mdx | mdx | mdx | real | small(<16KiB) | structured | 5513 | 15451 | 23221 | 9549 | 2839 | True |
| mdx-docs-404.mdx | mdx | mdx | real | small(<16KiB) | structured | 545 | 5246 | 23409 | 9734 | 2764 | True |
| mdx-docs-getting-started.mdx | mdx | mdx | real | medium(<256KiB) | structured | 26777 | 87116 | 27354 | 9867 | 2899 | True |
| mdx-guides-embed.mdx | mdx | mdx | real | small(<16KiB) | structured | 3652 | 13099 | 23418 | 11461 | 2766 | True |
| mdx-packages-index.mdx | mdx | mdx | real | small(<16KiB) | structured | 995 | 6389 | 23310 | 9651 | 2842 | True |
| hostile2-trunc-mhtml | mhtml | mhtml | hostile | small(<16KiB) | hostile | 1000 | 7936 | 23835 | 9764 | 2715 | True |
| tika-test-documents-testMHTMLFirefox.mhtml | mhtml | mhtml | real | small(<16KiB) | nested | 13759 | 30944 | 23176 | 5184 | 2734 | True |
| tika-test-documents-testUserDefinedCharset.mhtml | mhtml | mhtml | real | small(<16KiB) | nested | 621 | 6217 | 22952 | 11864 | 3755 | True |
| hostile2-trunc-msg | msg | opaque | hostile | small(<16KiB) | hostile | 1500 | 4712 | 18843 | 510 | 3322 | True |
| poi-hsmf-ASCII_CP1251_LCID1049.msg | msg | msg | real | small(<16KiB) | nested | 3584 | 11471 | 27864 | 9494 | 2767 | True |
| poi-hsmf-attachment_msg_inlineImg.msg | msg | msg | real | medium(<256KiB) | nested | 147456 | 298332 | 25225 | 5813 | 3360 | True |
| poi-hsmf-example_sent_regular.msg | msg | msg | real | medium(<256KiB) | nested | 53248 | 109916 | 23791 | 7114 | 4301 | True |
| poi-hsmf-logsat.com_signatures_valid.msg | msg | msg | real | medium(<256KiB) | nested | 23040 | 49500 | 23381 | 5177 | 2592 | True |
| poi-hsmf-message_1981.msg | msg | msg | real | medium(<256KiB) | nested | 29696 | 62812 | 23325 | 5014 | 2790 | True |
| poi-hsmf-simple_test_msg.msg | msg | msg | real | medium(<256KiB) | nested | 16896 | 37212 | 23456 | 5065 | 2807 | True |
| hostile2-trunc-notebook | notebook | notebook | hostile | small(<16KiB) | hostile | 355 | 5075 | 23333 | 9577 | 2790 | True |
| jupyter-Notebook-Custom-Keyboard-Shortcuts.ipynb | notebook | notebook | real | small(<16KiB) | structured | 1148 | 7493 | 23337 | 9798 | 2770 | True |
| jupyter-Notebook-Importing-Notebooks.ipynb | notebook | notebook | real | small(<16KiB) | structured | 13552 | 47792 | 23302 | 9903 | 2810 | True |
| jupyter-Notebook-Running-Code.ipynb | notebook | notebook | real | medium(<256KiB) | structured | 52012 | 133052 | 24270 | 9948 | 3107 | True |
| jupyter-Notebook-What-is-the-Jupyter-Notebook.ipynb | notebook | notebook | real | small(<16KiB) | structured | 7541 | 24181 | 23498 | 9711 | 2853 | True |
| jupyter-nbs-other.ipynb | notebook | notebook | real | small(<16KiB) | structured | 786 | 6744 | 23557 | 10177 | 3117 | True |
| jupyter-notebooks-simple.ipynb | notebook | notebook | real | small(<16KiB) | structured | 355 | 5075 | 23092 | 12466 | 3809 | True |
| jupyter-notebooks-simple_toc.ipynb | notebook | notebook | real | small(<16KiB) | structured | 1040 | 7873 | 23348 | 9716 | 2823 | True |
| odfpy-cols.odp | odp | odp | real | medium(<256KiB) | moderate | 18037 | 148305 | 20677 | 42490 | 3191 | True |
| odfpy-emb.odp | odp | odp | real | medium(<256KiB) | complex | 16403 | 106233 | 21137 | 38609 | 3083 | True |
| odfpy-examples-cols.odp | odp | odp | real | medium(<256KiB) | nested | 18037 | 148305 | 19992 | 37366 | 2847 | True |
| odfpy-examples-ol.odp | odp | odp | real | small(<16KiB) | nested | 14940 | 138597 | 19954 | 40940 | 2888 | True |
| odfpy-ol.odp | odp | odp | real | small(<16KiB) | simple | 14940 | 138597 | 20487 | 59404 | 3050 | True |
| odfpy-chinese.ods | ods | ods | real | small(<16KiB) | moderate | 11765 | 44661 | 20811 | 23497 | 2900 | True |
| odfpy-empty.ods | ods | ods | real | small(<16KiB) | simple | 6819 | 29699 | 19958 | 28740 | 3061 | True |
| odfpy-examples-chinese_spreadsheet.ods | ods | ods | real | small(<16KiB) | nested | 11765 | 44661 | 20360 | 23596 | 2857 | True |
| odfpy-examples-empty.ods | ods | ods | real | small(<16KiB) | nested | 6819 | 29699 | 19807 | 23038 | 2743 | True |
| odfpy-examples-pythagoras-kspread.ods | ods | ods | real | small(<16KiB) | nested | 5037 | 22024 | 19893 | 23384 | 2748 | True |
| odfpy-examples-pythagoras.ods | ods | ods | real | small(<16KiB) | nested | 6837 | 30406 | 19810 | 26426 | 2813 | True |
| odfpy-pyth-kspread.ods | ods | ods | real | small(<16KiB) | simple | 5037 | 22024 | 20204 | 23860 | 3007 | True |
| odfpy-pythagoras.ods | ods | ods | real | small(<16KiB) | simple | 6837 | 30406 | 19863 | 23409 | 2786 | True |
| hostile2-trunc-odt | odt | opaque | hostile | small(<16KiB) | hostile | 3000 | 6212 | 18351 | 483 | 2720 | True |
| odfpy-examples-definitionlists.odt | odt | odt | real | small(<16KiB) | nested | 7824 | 32070 | 19752 | 23029 | 2806 | True |
| odfpy-examples-nasty.odt | odt | odt | real | small(<16KiB) | nested | 1539 | 11051 | 19823 | 18972 | 2731 | True |
| odfpy-examples-no_userfields.odt | odt | odt | real | medium(<256KiB) | nested | 19228 | 52978 | 19804 | 23223 | 2789 | True |
| odfpy-examples-parastyles.odt | odt | odt | real | small(<16KiB) | nested | 7194 | 32845 | 19826 | 26873 | 3620 | True |
| odfpy-examples-serious_poem.odt | odt | odt | real | small(<16KiB) | nested | 8539 | 31745 | 19775 | 22988 | 2725 | True |
| odfpy-examples-simpletable.odt | odt | odt | real | small(<16KiB) | nested | 7055 | 31250 | 19797 | 23389 | 2838 | True |
| odfpy-examples-userfields_ooo3.odt | odt | odt | real | medium(<256KiB) | nested | 38420 | 98623 | 20196 | 23091 | 2925 | True |
| hostile2-trunc-parquet | parquet | opaque | hostile | small(<16KiB) | hostile | 800 | 4012 | 18688 | 483 | 2672 | True |
| parquet-alltypes-plain.parquet | parquet | parquet | real | small(<16KiB) | simple | 1851 | 9203 | 23708 | 9947 | 3012 | True |
| parquet-alltypes-tiny.parquet | parquet | parquet | real | large(>=256KiB) | complex | 454233 | 914573 | 28773 | 11462 | 4676 | True |
| parquet-data-datapage_v2.snappy.parquet | parquet | parquet | real | small(<16KiB) | nested | 1165 | 6792 | 23123 | 9584 | 2657 | True |
| parquet-data-fixed_length_byte_array.parquet | parquet | parquet | real | small(<16KiB) | nested | 4437 | 12708 | 23089 | 9656 | 2738 | True |
| parquet-data-fixed_length_decimal.parquet | parquet | parquet | real | small(<16KiB) | nested | 677 | 5166 | 23086 | 9542 | 2708 | True |
| parquet-data-float16_nonzeros_and_nans.parquet | parquet | parquet | real | small(<16KiB) | nested | 501 | 4788 | 23084 | 9588 | 2698 | True |
| parquet-data-lz4_raw_compressed_larger.parquet | parquet | parquet | real | large(>=256KiB) | nested | 380836 | 765507 | 27629 | 10890 | 4002 | True |
| parquet-data-nation.dict-malformed.parquet | parquet | parquet | real | small(<16KiB) | nested | 2850 | 9855 | 23146 | 9575 | 2675 | True |
| parquet-delta-binary.parquet | parquet | parquet | real | medium(<256KiB) | complex | 72971 | 161889 | 24260 | 10074 | 3144 | True |
| parquet-geospatial-geography-points.parquet | parquet | parquet | real | medium(<256KiB) | nested | 33026 | 81965 | 23611 | 9801 | 2873 | True |
| asciidoctor-fixtures-hello-asciidoctor.pdf | pdf | pdf | real | small(<16KiB) | nested | 847 | 6557 | 23106 | 397 | 2682 | True |
| cpython-library-turtle-star.pdf | pdf | pdf | real | small(<16KiB) | nested | 4418 | 12438 | 23417 | 477 | 3870 | True |
| netcdf-docs-docmap.pdf | pdf | pdf | real | medium(<256KiB) | nested | 64347 | 78695 | 24803 | 600 | 2987 | True |
| cargo-cargo.toml | pkgmeta | pkgmeta | real | small(<16KiB) | complex | 7717 | 78906 | 23238 | 10139 | 2759 | True |
| jupyter-package.json | pkgmeta | pkgmeta | real | small(<16KiB) | structured | 3448 | 17137 | 23332 | 9596 | 2744 | True |
| jupyter-py-pyproject.toml | pkgmeta | pkgmeta | real | small(<16KiB) | structured | 7903 | 50514 | 23241 | 9552 | 2786 | True |
| serde-cargo.toml | pkgmeta | pkgmeta | real | small(<16KiB) | moderate | 2431 | 17167 | 23297 | 9669 | 2681 | True |
| vega-package.json | pkgmeta | pkgmeta | real | small(<16KiB) | structured | 2497 | 15674 | 23717 | 9981 | 2869 | True |
| hostile2-trunc-ppt | ppt | cfbf | hostile | small(<16KiB) | hostile | 8000 | 19423 | 23209 | 5458 | 2857 | True |
| poi-slideshow-42474-2.ppt | ppt | ppt | real | large(>=256KiB) | nested | 571392 | 1146204 | 30367 | 7684 | 5146 | True |
| poi-slideshow-54332a.ppt | ppt | ppt | real | medium(<256KiB) | nested | 67584 | 138588 | 23951 | 5294 | 2906 | True |
| poi-slideshow-Single_Coloured_Page.ppt | ppt | ppt | real | small(<16KiB) | nested | 9728 | 22876 | 23281 | 5183 | 2793 | True |
| poi-slideshow-basic_test_ppt_file.ppt | ppt | ppt | real | small(<16KiB) | nested | 15360 | 34140 | 23256 | 5102 | 2755 | True |
| poi-slideshow-bug62092.ppt | ppt | ppt | real | medium(<256KiB) | nested | 161792 | 327004 | 24725 | 5436 | 3342 | True |
| poi-slideshow-datetime.ppt | ppt | ppt | real | medium(<256KiB) | nested | 114176 | 231772 | 24240 | 7468 | 4231 | True |
| hostile2-trunc-pptx | pptx | opaque | hostile | medium(<256KiB) | hostile | 120000 | 123212 | 20336 | 762 | 3256 | True |
| poi-61515.pptx | pptx | pptx | real | medium(<256KiB) | moderate | 29270 | 99853 | 20852 | 46542 | 2853 | True |
| poi-Divino.pptx | pptx | opaque | real | large(>=256KiB) | complex | 523999 | 527211 | 26327 | 1576 | 4515 | True |
| poi-SampleShow.pptx | pptx | pptx | real | medium(<256KiB) | moderate | 39083 | 132308 | 25871 | 42171 | 3012 | True |
| poi-bug60993.pptx | pptx | pptx | real | medium(<256KiB) | complex | 21942 | 95612 | 20479 | 41624 | 2767 | True |
| poi-slideshow-49386-null_dates.pptx | pptx | pptx | real | medium(<256KiB) | nested | 17124 | 62331 | 20562 | 45763 | 2798 | True |
| poi-slideshow-au.asn.aes.www_conferences_2011_presentations_Fri_20Room4Level4_20 | pptx | pptx | real | large(>=256KiB) | nested | 536086 | 1622797 | 40932 | 45000 | 5124 | True |
| poi-slideshow-bug58144-headers-footers-2007.pptx | pptx | pptx | real | medium(<256KiB) | nested | 42035 | 136662 | 20882 | 41844 | 2941 | True |
| poi-slideshow-bug62736.pptx | pptx | pptx | real | medium(<256KiB) | nested | 50926 | 164564 | 21026 | 42563 | 3206 | True |
| poi-slideshow-bug65551.pptx | pptx | pptx | real | medium(<256KiB) | nested | 34383 | 110096 | 21040 | 44822 | 2936 | True |
| poi-slideshow-pptx2svg.pptx | pptx | pptx | real | medium(<256KiB) | nested | 149172 | 467022 | 22275 | 42217 | 3355 | True |
| poi-table_test.pptx | pptx | pptx | real | medium(<256KiB) | moderate | 28935 | 103361 | 21054 | 42601 | 2830 | True |
| cpython-faq-gui.rst | rst | rst | real | small(<16KiB) | structured | 3058 | 11685 | 24567 | 11230 | 2881 | True |
| cpython-library-asyncio-protocol.rst | rst | rst | real | medium(<256KiB) | structured | 32975 | 83009 | 27898 | 9622 | 2876 | True |
| cpython-library-mmap.rst | rst | rst | real | small(<16KiB) | structured | 14634 | 35272 | 24117 | 9665 | 2717 | True |
| cpython-library-python.rst | rst | rst | real | small(<16KiB) | structured | 502 | 4725 | 23171 | 9691 | 2779 | True |
| cpython-library-sunau.rst | rst | rst | real | small(<16KiB) | structured | 7423 | 23681 | 23765 | 9861 | 2912 | True |
| cpython-whatsnew-3.3.rst | rst | markdown | real | medium(<256KiB) | structured | 99150 | 238877 | 26509 | 10447 | 3270 | True |
| hostile2-trunc-rtf | rtf | opaque | hostile | small(<16KiB) | hostile | 1000 | 4212 | 18568 | 456 | 2647 | True |
| ne-10m_cultural-10m_us_parks_-READ_ME.rtf | rtf | rtf | real | small(<16KiB) | structured | 2058 | 11954 | 23335 | 9585 | 2789 | True |
| ne-10m_parks_and_protected_areas-10m_us_parks_READ_ME.rtf | rtf | rtf | real | small(<16KiB) | structured | 3718 | 18579 | 23273 | 9576 | 2732 | True |
| poi-quick-contents-message.rtf | rtf | rtf | real | medium(<256KiB) | structured | 25528 | 157229 | 23752 | 10146 | 2899 | True |
| hostile2-magic-toml | toml | toml | hostile | small(<16KiB) | hostile | 37 | 4013 | 23389 | 9717 | 2780 | True |
| toml-array-string-quote-comma-02.toml | toml | toml | real | small(<16KiB) | structured | 20 | 3692 | 23231 | 9599 | 3120 | True |
| toml-datetime-local.toml | toml | toml | real | small(<16KiB) | structured | 88 | 3951 | 23391 | 9700 | 2735 | True |
| toml-key-escapes.toml | toml | toml | real | small(<16KiB) | structured | 163 | 4593 | 22859 | 9488 | 2755 | True |
| toml-spec-1.0.0-float-2.toml | toml | toml | real | small(<16KiB) | structured | 276 | 4701 | 23228 | 14076 | 4237 | True |
| toml-spec-1.1.0-common-0.toml | toml | toml | real | small(<16KiB) | structured | 122 | 3969 | 23041 | 9620 | 2749 | True |
| toml-string-basic-escape-03.toml | toml | toml | real | small(<16KiB) | structured | 56 | 3739 | 22910 | 9622 | 2704 | True |
| toml-valid-implicit-and-explicit-before.toml | toml | toml | real | small(<16KiB) | structured | 37 | 4013 | 23202 | 9874 | 2909 | True |
| toml-valid-multibyte.toml | toml | toml | real | small(<16KiB) | structured | 1156 | 7010 | 23016 | 9542 | 2684 | True |
| hostile2-trunc-xls | xls | cfbf | hostile | small(<16KiB) | hostile | 1500 | 6423 | 23274 | 5272 | 2833 | True |
| poi-spreadsheet-43493.xls | xls | xls | real | medium(<256KiB) | nested | 66048 | 135516 | 23804 | 5514 | 3052 | True |
| poi-spreadsheet-47251_1.xls | xls | xls | real | medium(<256KiB) | nested | 26112 | 102820 | 23720 | 9946 | 2679 | True |
| poi-spreadsheet-48325.xls | xls | xls | real | small(<16KiB) | nested | 4096 | 15039 | 22892 | 9708 | 2826 | True |
| poi-spreadsheet-49524.xls | xls | xls | real | small(<16KiB) | nested | 13824 | 31068 | 23605 | 5268 | 2785 | True |
| poi-spreadsheet-50298.xls | xls | xls | real | medium(<256KiB) | nested | 20480 | 61291 | 23488 | 9976 | 2897 | True |
| poi-spreadsheet-DStar.xls | xls | xls | real | medium(<256KiB) | nested | 35328 | 104549 | 23407 | 9278 | 2825 | True |
| poi-spreadsheet-EmbeddedChartHeaderTest.xls | xls | xls | real | small(<16KiB) | nested | 15360 | 43754 | 23531 | 12088 | 4140 | True |
| poi-spreadsheet-bug66319.xls | xls | xls | real | large(>=256KiB) | nested | 387072 | 777564 | 28385 | 6307 | 5345 | True |
| hostile2-trunc-xlsx | xlsx | opaque | hostile | small(<16KiB) | hostile | 15000 | 18212 | 18583 | 504 | 2758 | True |
| poi-47889.xlsx | xlsx | xlsx | real | small(<16KiB) | simple | 3555 | 19151 | 20709 | 33380 | 3074 | True |
| poi-49609.xlsx | xlsx | xlsx | real | medium(<256KiB) | complex | 105424 | 218879 | 21241 | 5614 | 3154 | True |
| poi-59021.xlsx | xlsx | xlsx | real | small(<16KiB) | moderate | 1933 | 12665 | 20079 | 28622 | 3051 | True |
| poi-DataValidation.xlsx | xlsx | xlsx | real | small(<16KiB) | moderate | 3286 | 18607 | 20126 | 28298 | 2915 | True |
| poi-linkext.xlsx | xlsx | xlsx | real | small(<16KiB) | complex | 3274 | 18583 | 20208 | 29519 | 2903 | True |
| poi-spreadsheet-123233_charts.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 12957 | 47188 | 20600 | 27783 | 2868 | True |
| poi-spreadsheet-56511.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 9688 | 35780 | 20250 | 32896 | 2964 | True |
| poi-spreadsheet-58731.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 9023 | 33113 | 21026 | 28795 | 3006 | True |
| poi-spreadsheet-59021.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 1933 | 12665 | 19968 | 28159 | 2885 | True |
| poi-spreadsheet-StructuredRefs-lots-with-lookups.xlsx | xlsx | xlsx | real | large(>=256KiB) | nested | 325446 | 687123 | 23389 | 29486 | 3979 | True |
| poi-spreadsheet-TwoSheetsOneHidden.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 7962 | 30038 | 20331 | 32606 | 2962 | True |
| poi-spreadsheet-WithChartSheet.xlsx | xlsx | xlsx | real | medium(<256KiB) | nested | 30333 | 99513 | 21893 | 28345 | 2884 | True |
| poi-spreadsheet-duplicate-filename-case-insensitive.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 10721 | 30742 | 20132 | 5240 | 2866 | True |
| maven-core-pom.xml | xml | xml | real | small(<16KiB) | moderate | 7411 | 46651 | 23921 | 10236 | 2963 | True |
| maven-pom.xml | xml | xml | real | medium(<256KiB) | complex | 28489 | 167485 | 24217 | 10204 | 3017 | True |
| poi-quick-contents-quick.xml | xml | xml | real | small(<16KiB) | structured | 143 | 4131 | 23202 | 9705 | 2729 | True |
| poi-spreadsheet-SampleSS.xml | xml | xml | real | small(<16KiB) | structured | 5115 | 36467 | 23520 | 9697 | 2850 | True |
| alertmanager-ci.yml | yaml | yaml | real | small(<16KiB) | simple | 1348 | 9179 | 23090 | 9489 | 2688 | True |
| alertmanager-simple.yml | yaml | yaml | real | small(<16KiB) | moderate | 3953 | 16344 | 23230 | 9637 | 2793 | True |
| cpython-workflows-build.yml | yaml | yaml | real | medium(<256KiB) | structured | 19106 | 65679 | 23809 | 9872 | 2525 | True |
| cpython-workflows-project-updater.yml | yaml | config | real | small(<16KiB) | structured | 741 | 7022 | 23192 | 10310 | 2694 | True |
| cpython-workflows-require-pr-label.yml | yaml | yaml | real | small(<16KiB) | structured | 507 | 5822 | 23260 | 9722 | 2881 | True |
| cpython-workflows-reusable-ubuntu.yml | yaml | yaml | real | small(<16KiB) | structured | 3356 | 14842 | 23059 | 9616 | 2754 | True |
| cpython-workflows-reusable-windows.yml | yaml | yaml | real | small(<16KiB) | structured | 1157 | 7847 | 23130 | 9738 | 2696 | True |
| hostile2-trunc-yaml | yaml | yaml | hostile | small(<16KiB) | hostile | 4000 | 17939 | 23632 | 10136 | 2899 | True |
| prometheus-ci.yml | yaml | yaml | real | small(<16KiB) | complex | 10201 | 39596 | 23495 | 9995 | 3585 | True |
| prometheus-web-config.yml | yaml | yaml | real | small(<16KiB) | simple | 531 | 5022 | 23132 | 9672 | 2719 | True |
| prometheus.yml | yaml | yaml | real | small(<16KiB) | moderate | 934 | 6465 | 23314 | 9715 | 2781 | True |

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
