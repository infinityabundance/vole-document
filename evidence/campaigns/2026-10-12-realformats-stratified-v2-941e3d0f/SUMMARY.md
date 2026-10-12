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
| 308 | 284 | 24 | 0 | 0 | 308 | 274 | 15 | 41 |

By declared format (ran): arrow=9, asciidoc=7, bib=3, cbor=8, cfbf=5, config=3, container=10, csv=12, doc=9, docx=10, eml=9, epub=3, feed=6, geojson=13, gis=10, hdf=9, html=8, json=11, json5=1, jsonl=9, logstream=6, markdown=6, mdx=7, mhtml=3, msg=7, notebook=8, odp=5, ods=8, odt=8, parquet=11, pdf=3, pkgmeta=5, ppt=7, pptx=12, rst=6, rtf=4, toml=9, xls=9, xlsx=14, xml=4, yaml=11

## Stratified tables

## By declared format

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| arrow | 9 | 214925 | 23382 | 1519711 | 8335 | 84560 | 9802 | 31734 | 2911 | yes |
| asciidoc | 7 | 171076 | 23598 | 371099 | 8386 | 74471 | 10143 | 20661 | 2854 | yes |
| bib | 3 | 74741 | 26116 | 1575364 | 545657 | 24406 | 10954 | 10430 | 3493 | yes |
| cbor | 8 | 173630 | 23165 | 58385 | 5521.0 | 50333 | 9664 | 22243 | 2778 | yes |
| cfbf | 5 | 122816 | 24051 | 1054209 | 134375 | 47496 | 9903 | 16351 | 3146 | yes |
| config | 3 | 82465 | 28489 | 372893 | 7731 | 34702 | 11007 | 9697 | 3019 | yes |
| container | 10 | 226179 | 20342 | 2454600 | 9865.5 | 91861 | 9980 | 33446 | 2954 | yes |
| csv | 12 | 292634 | 24204 | 2500572 | 66873.0 | 125856 | 10116 | 36126 | 2952 | yes |
| doc | 9 | 3418069 | 422858 | 1364349 | 57692 | 941567 | 89244 | 496200 | 41662 | yes |
| docx | 10 | 2803173 | 360558 | 1290131 | 69418.0 | 5460457 | 643338 | 409261 | 40310 | yes |
| eml | 9 | 1633060 | 23627 | 93592 | 6266 | 430102 | 10033 | 188539 | 3105 | yes |
| epub | 3 | 61519 | 20506 | 135781 | 33764 | 20632 | 9898 | 9749 | 3039 | yes |
| feed | 6 | 143749 | 23984 | 59015 | 8622.5 | 61845 | 10232 | 18077 | 2998 | yes |
| geojson | 13 | 342134 | 25975 | 9615938 | 431186 | 150630 | 11349 | 46704 | 3243 | yes |
| gis | 10 | 247886 | 23829 | 2240691 | 11066.0 | 100295 | 10087 | 31188 | 2925 | yes |
| hdf | 9 | 211730 | 23732 | 424354 | 14926 | 53280 | 5422 | 27908 | 2981 | yes |
| html | 8 | 196217 | 23814 | 430673 | 23085.5 | 79965 | 9997 | 23035 | 2820 | yes |
| json | 11 | 273451 | 23652 | 3263643 | 14189 | 107652 | 9976 | 34116 | 2892 | yes |
| json5 | 1 | 23555 | 23555 | 3852 | 3852 | 9948 | 9948 | 2937 | 2937 | yes |
| jsonl | 9 | 934015 | 32652 | 12464612 | 1837044 | 478261 | 16391 | 190552 | 5311 | yes |
| logstream | 6 | 210370 | 36134 | 4211830 | 657668.0 | 67392 | 11530 | 24175 | 3976 | yes |
| markdown | 6 | 155660 | 24503 | 630422 | 18609.0 | 61956 | 10118 | 18323 | 2933 | yes |
| mdx | 7 | 170723 | 23951 | 172724 | 13099 | 74108 | 10318 | 21463 | 2990 | yes |
| mhtml | 3 | 70832 | 23664 | 45097 | 7936 | 27572 | 10274 | 9708 | 3000 | yes |
| msg | 7 | 325094 | 36609 | 573955 | 49500 | 98254 | 8364 | 38895 | 4919 | yes |
| notebook | 8 | 190063 | 23621 | 237285 | 7683.0 | 80713 | 10026 | 24011 | 2990 | yes |
| odp | 5 | 103158 | 20536 | 680037 | 138597 | 189870 | 37325 | 15664 | 2817 | yes |
| ods | 8 | 161200 | 20091 | 253580 | 30052.5 | 195939 | 23418 | 22663 | 2822 | yes |
| odt | 8 | 163827 | 20703 | 296774 | 31907.5 | 166900 | 24043 | 25337 | 3093 | yes |
| parquet | 11 | 1547537 | 23449 | 1976458 | 9855 | 622972 | 9769 | 179760 | 2809 | yes |
| pdf | 3 | 72677 | 23752 | 97690 | 12438 | 2248 | 756 | 9362 | 3078 | yes |
| pkgmeta | 5 | 116896 | 23425 | 179398 | 17167 | 50837 | 9814 | 13994 | 2799 | yes |
| ppt | 7 | 747550 | 116728 | 1920007 | 138588 | 178582 | 26236 | 105357 | 13310 | yes |
| pptx | 12 | 2848067 | 206324 | 3645029 | 127760.0 | 5865502 | 456916 | 318937 | 17506 | yes |
| rst | 6 | 152596 | 24477 | 397249 | 29476.5 | 64904 | 10350 | 18309 | 3068 | yes |
| rtf | 4 | 96188 | 23376 | 191974 | 15266.5 | 34269 | 10152 | 12762 | 2844 | yes |
| toml | 9 | 210297 | 23322 | 39681 | 4013 | 89127 | 9742 | 26374 | 2807 | yes |
| xls | 9 | 3579379 | 442153 | 1278024 | 61291 | 1332859 | 192750 | 366146 | 43208 | yes |
| xlsx | 14 | 3215865 | 373564 | 1282259 | 30390.0 | 3972077 | 183131 | 462306 | 31282 | yes |
| xml | 4 | 96066 | 23715 | 254734 | 41559.0 | 44019 | 10372 | 11755 | 2936 | yes |
| yaml | 11 | 257584 | 23316 | 195757 | 9179 | 107499 | 9720 | 31912 | 2797 | yes |

## By size class

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| large(>=256KiB) | 25 | 3519882 | 35045 | 36836336 | 1146204 | 2184960 | 15876 | 418808 | 5315 | yes |
| medium(<256KiB) | 86 | 10728355 | 27109 | 19477295 | 134945.5 | 9976261 | 11602 | 1431341 | 3490 | yes |
| small(<16KiB) | 197 | 11890416 | 23495 | 3539797 | 11954 | 9594697 | 9959 | 1566018 | 2897 | yes |

## By complexity

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| complex | 16 | 2116525 | 28852 | 12213569 | 178642.5 | 983153 | 17494 | 244140 | 3539 | yes |
| hostile | 24 | 488090 | 19431 | 341455 | 6083.5 | 94012 | 702 | 71103 | 2834 | yes |
| moderate | 19 | 1290337 | 23316 | 2368557 | 44661 | 520480 | 10697 | 225626 | 3002 | yes |
| nested | 114 | 18018082 | 25438 | 17206985 | 36496.0 | 18258975 | 21187 | 2344993 | 3824 | yes |
| simple | 37 | 1784833 | 24134 | 6961605 | 23143 | 833682 | 10673 | 220462 | 2957 | yes |
| structured | 98 | 2440786 | 23650 | 20761257 | 14515.5 | 1065616 | 10138 | 309843 | 2952 | yes |

## By stratum (real vs malformed/hostile)

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| hostile | 24 | 488090 | 19431 | 341455 | 6083.5 | 94012 | 702 | 71103 | 2834 | yes |
| real | 284 | 25650563 | 23824 | 59511973 | 30843.0 | 21661906 | 10366 | 3345064 | 3050 | yes |

### Workload dimension

`narrow` = one `observe --metadata --kind metadata` per document (a bounded metadata projection).
`full` = one `materialize --exact --packed` per document (byte-authority closure).
Both are pooled and median; a size/time-weighted world is `pooled`, an unweighted per-document world is `median` (ADR-0054: a claim names its estimator).

**Detection.** 22/308 samples were detected as `opaque` by the bounded detector; 57 samples declined the narrow observation. The byte-authority gate is unaffected by detection; every sample is byte-exactly closed.

## Findings (detector choice != declared format)

A **finding** is a real sample whose detector choice differs from its declared format: a conservative `decline-opaque`, or a `misclassified` claim by another detector. Every finding is reconciled in the phase report (fixed-with-test, or recorded as a typed decline / recorded misclassification with its reason). The byte-authority closure is unaffected by any finding.

Summary by kind: decline-opaque=5, misclassified=10

| id | declared | detected | kind | resolution |
|---|---|---|---|---|
| asciidoctor-CHANGELOG.adoc | asciidoc | rst | misclassified | recorded misclassification (precedence) — the reST detector is tried before AsciiDoc and claims `asciidoctor-CHANGELOG.adoc`, `asciidoctor-locale-attributes-ja.adoc`, `asciidoctor-locale-attributes-sr.adoc`, `asciidoctor-pages-macos.adoc`, and `asciidoctor-pages-stylesheet-modes.adoc` via its field-list (`:name: value`) signal; AsciiDoc attribute entries are syntactically RST field lists, and reordering AsciiDoc before reST is not clearly warranted (reST precedence is load-bearing), so it is not reordered. Phase 28.1 re-checked each and left them recorded rather than contaminating reST (src/adapter/rst.rs, asciidoc.rs). |
| asciidoctor-locale-attributes-ja.adoc | asciidoc | rst | misclassified | recorded misclassification (precedence) — the reST detector is tried before AsciiDoc and claims `asciidoctor-CHANGELOG.adoc`, `asciidoctor-locale-attributes-ja.adoc`, `asciidoctor-locale-attributes-sr.adoc`, `asciidoctor-pages-macos.adoc`, and `asciidoctor-pages-stylesheet-modes.adoc` via its field-list (`:name: value`) signal; AsciiDoc attribute entries are syntactically RST field lists, and reordering AsciiDoc before reST is not clearly warranted (reST precedence is load-bearing), so it is not reordered. Phase 28.1 re-checked each and left them recorded rather than contaminating reST (src/adapter/rst.rs, asciidoc.rs). |
| asciidoctor-locale-attributes-sr.adoc | asciidoc | rst | misclassified | recorded misclassification (precedence) — the reST detector is tried before AsciiDoc and claims `asciidoctor-CHANGELOG.adoc`, `asciidoctor-locale-attributes-ja.adoc`, `asciidoctor-locale-attributes-sr.adoc`, `asciidoctor-pages-macos.adoc`, and `asciidoctor-pages-stylesheet-modes.adoc` via its field-list (`:name: value`) signal; AsciiDoc attribute entries are syntactically RST field lists, and reordering AsciiDoc before reST is not clearly warranted (reST precedence is load-bearing), so it is not reordered. Phase 28.1 re-checked each and left them recorded rather than contaminating reST (src/adapter/rst.rs, asciidoc.rs). |
| asciidoctor-pages-macos.adoc | asciidoc | rst | misclassified | recorded misclassification (precedence) — the reST detector is tried before AsciiDoc and claims `asciidoctor-CHANGELOG.adoc`, `asciidoctor-locale-attributes-ja.adoc`, `asciidoctor-locale-attributes-sr.adoc`, `asciidoctor-pages-macos.adoc`, and `asciidoctor-pages-stylesheet-modes.adoc` via its field-list (`:name: value`) signal; AsciiDoc attribute entries are syntactically RST field lists, and reordering AsciiDoc before reST is not clearly warranted (reST precedence is load-bearing), so it is not reordered. Phase 28.1 re-checked each and left them recorded rather than contaminating reST (src/adapter/rst.rs, asciidoc.rs). |
| asciidoctor-pages-stylesheet-modes.adoc | asciidoc | rst | misclassified | recorded misclassification (precedence) — the reST detector is tried before AsciiDoc and claims `asciidoctor-CHANGELOG.adoc`, `asciidoctor-locale-attributes-ja.adoc`, `asciidoctor-locale-attributes-sr.adoc`, `asciidoctor-pages-macos.adoc`, and `asciidoctor-pages-stylesheet-modes.adoc` via its field-list (`:name: value`) signal; AsciiDoc attribute entries are syntactically RST field lists, and reordering AsciiDoc before reST is not clearly warranted (reST precedence is load-bearing), so it is not reordered. Phase 28.1 re-checked each and left them recorded rather than contaminating reST (src/adapter/rst.rs, asciidoc.rs). |
| cbor-data-integer.cbor | cbor | opaque | decline-opaque | recorded (unchanged reason) — two of the three recorded CBOR declines are genuine detector boundaries and stay recorded: `cbor-data-integer.cbor` is a lone scalar (`18 2a`, 2 bytes) below the deliberate structural bar; `cbor-rfc8949-good.cbor` is a genuinely well-formed single map whose deepest RFC 8949 vector nests to **512 levels**, above the deliberate `max_cbor_depth` cap of 256, so it is declined on the DoS bound (raising that cap to admit one sample is not warranted). The third, `cbor-data-tagged_date.cbor`, is fixed — see the `cbor`/`cbor` note below. The lone-scalar and depth declines are conservative by design (src/adapter/cbor.rs). |
| cbor-rfc8949-good.cbor | cbor | opaque | decline-opaque | recorded (unchanged reason) — two of the three recorded CBOR declines are genuine detector boundaries and stay recorded: `cbor-data-integer.cbor` is a lone scalar (`18 2a`, 2 bytes) below the deliberate structural bar; `cbor-rfc8949-good.cbor` is a genuinely well-formed single map whose deepest RFC 8949 vector nests to **512 levels**, above the deliberate `max_cbor_depth` cap of 256, so it is declined on the DoS bound (raising that cap to admit one sample is not warranted). The third, `cbor-data-tagged_date.cbor`, is fixed — see the `cbor`/`cbor` note below. The lone-scalar and depth declines are conservative by design (src/adapter/cbor.rs). |
| poi-document-Bug60942b.doc | doc | cfbf | misclassified | recorded decline — `poi-document-Bug60942b.doc` stays here: it is a **Word 6.0** FIB (`wIdent` 0xA5DC, `nFib` 0x0065), outside the supported Word 97-2003 `nFib` subset the adapter models, so it stays at the generic CFBF layer. The second recorded cause, `poi-document-testRangeDelete.doc` (a genuine Word FIB whose `WordDocument` mini-stream is **fragmented** and whose **root mini stream is itself fragmented**), is **fixed (28.1)**: a mini sector's absolute position is now resolved through the root mini stream's FAT chain (`CfbfModel::mini_chain`, added in model version 2), so `cfbf::stream_leading`'s first-chunk prefix read works for a fragmented mini stream; regression test `legacy_cfbf_detection::fragmented_mini_stream_in_fragmented_root_stream_is_detected` (src/adapter/cfbf.rs, doc.rs). |
| cpython-msg-25.eml | eml | opaque | decline-opaque | recorded decline — the sample is an **mbox** (a leading `From ` envelope line, not an RFC 5322 header) whose `multipart/report` boundary is folded and truncated (`;\n\tbo`), so the bounded header-block scan declines rather than admitting an unmodellable message container (src/adapter/eml.rs). |
| loghub-HPC-HPC_2k.log | logstream | opaque | decline-opaque | recorded decline + fixed (28.1) — `loghub-HPC-HPC_2k.log` stays recorded: it is a space-separated event record with **no timestamp and no level** (`134681 node-246 unix.hw state_change.unavailable …`), which is not a log-stream line grammar and is left to the fixed-width/tabular heuristics. Three other declines are **fixed**: the detector's generic grammar now also accepts a `YY/MM/DD HH:MM:SS` slash timestamp (Spark) and a bracketed `[MM.DD HH:MM:SS]` timestamp (Proxifier), and a new BSD-without-`<PRI>` dialect accepts `Mmm dd hh:mm:ss host tag…: msg` (macOS `loghub-Mac-Mac_2k.log`, whose lenient tag region can embed spaces and `([pid])`). Detection stays conservative (≥2 non-blank lines, **every** non-blank line must match). Regression tests `logstream::tests::detects_the_widened_dialects` and `logstream::tests::the_widened_grammar_still_declines_prose` (src/adapter/logstream.rs). |
| loghub-OpenStack-OpenStack_2k.log | logstream | config | misclassified | recorded misclassification (precedence) — `loghub-OpenStack-OpenStack_2k.log` is still claimed by the config detector (`KEY: value` shape) before the line-dialect log detector runs; reordering logstream before config risks the load-bearing config detector, so it is not reordered (src/adapter/logstream.rs, config.rs). |
| loghub-Windows-Windows_2k.log | logstream | csv | misclassified | recorded misclassification (precedence) — `loghub-Windows-Windows_2k.log` is still claimed by the CSV detector (consistent field count) before the (later) logstream detector runs; reordering risks the load-bearing CSV detector, so it is not reordered (src/adapter/logstream.rs, csv.rs). |
| poi-Divino.pptx | pptx | opaque | decline-opaque | recorded decline — `poi-Divino.pptx` is **truncated**: it has no EOCD / central directory (and no valid ZIP structure), so it is not a valid OPC/OOXML package and correctly stays Opaque (the whole-source exact leaf is unaffected). |
| cpython-whatsnew-3.3.rst | rst | markdown | misclassified | recorded misclassification (precedence) — the reST detector is tried after Markdown, and the Markdown detector claims `cpython-whatsnew-3.3.rst` via its structural marks (the over+underlined `***` RST title is a Markdown thematic break); reordering is not clearly warranted, so it is recorded (src/adapter/rst.rs, markdown.rs). |
| cpython-workflows-project-updater.yml | yaml | config | misclassified | recorded misclassification (precedence) — `cpython-workflows-project-updater.yml` is a GitHub Actions workflow that the bounded YAML detector declines (a `${{ … }}` expression / flow mapping outside the modelled YAML subset) and is then claimed by the config detector (`KEY: value` shape); recorded rather than weakening either detector (src/adapter/yaml.rs, config.rs). |

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

Phase 28.1 detector fixes landed here: (1) `cfbf::stream_leading` now resolves a
mini sector's absolute position through the root mini stream's **FAT chain**
(`CfbfModel::mini_chain`, model version 2), so a `.doc` whose `WordDocument`
mini-stream and root mini stream are both fragmented still detects - regression
test `legacy_cfbf_detection::fragmented_mini_stream_in_fragmented_root_stream_is_detected`;
(2) the log-stream generic grammar now also accepts a `YY/MM/DD HH:MM:SS` slash
timestamp (Spark) and a bracketed `[MM.DD HH:MM:SS]` timestamp (Proxifier), and a
new BSD-without-`<PRI>` dialect accepts `Mmm dd hh:mm:ss host tag…: msg` (macOS) -
regression tests `logstream::tests::detects_the_widened_dialects` and
`logstream::tests::the_widened_grammar_still_declines_prose`; (3) AsciiDoc now
claims an `xref:`-macro-only navigation list - regression test
`asciidoc::tests::detects_a_cross_reference_list_and_declines_its_negations`;
(4) CBOR now claims a top-level **tag** at two nodes - regression test
`cbor::tests::detects_a_tagged_scalar_and_rejects_a_bare_scalar`. The remaining
findings are recorded precedence/decline boundaries.

No real sample was skipped.

### Per-sample

| id | declared | detected | stratum | size class | complexity | src B | store B | build us | narrow us | full us | exact |
|---|---|---|---|---|---|---:|---:|---:|---:|---:|---|
| arrow-file-1.0.0-bigendian-generated_decimal256.arrow_file | arrow | arrow | real | large(>=256KiB) | nested | 363106 | 736384 | 27290 | 11242 | 6304 | True |
| arrow-file-1.0.0-littleendian-generated_union.arrow_file | arrow | arrow | real | small(<16KiB) | nested | 3562 | 11806 | 23382 | 9600 | 2811 | True |
| arrow-file-cpp-21.0.0-generated_dictionary_unsigned.arrow_file | arrow | arrow | real | small(<16KiB) | nested | 2170 | 8335 | 23140 | 9612 | 2640 | True |
| arrow-file-cpp-21.0.0-generated_null_trivial.arrow_file | arrow | arrow | real | small(<16KiB) | nested | 514 | 4774 | 23555 | 9558 | 2684 | True |
| arrow-stream-1.0.0-littleendian-generated_decimal256.stream | arrow | arrow | real | large(>=256KiB) | nested | 360464 | 732924 | 28913 | 11013 | 4564 | True |
| arrow-stream-1.0.0-littleendian-generated_nested_dictionary.stream | arrow | arrow | real | small(<16KiB) | nested | 2632 | 9850 | 23421 | 10056 | 2911 | True |
| arrow-stream-1.0.0-littleendian-generated_null_trivial.stream | arrow | arrow | real | small(<16KiB) | nested | 312 | 4582 | 23211 | 13062 | 3903 | True |
| arrow-stream-cpp-21.0.0-generated_custom_metadata.stream | arrow | arrow | real | small(<16KiB) | nested | 1504 | 7330 | 23326 | 9802 | 2801 | True |
| hostile2-magic-arrow | arrow | opaque | hostile | small(<16KiB) | hostile | 514 | 3726 | 18687 | 615 | 3116 | True |
| asciidoctor-CHANGELOG.adoc | asciidoc | rst | real | medium(<256KiB) | structured | 140952 | 307359 | 29501 | 10579 | 3481 | True |
| asciidoctor-cli-nav.adoc | asciidoc | asciidoc | real | small(<16KiB) | structured | 222 | 4794 | 23598 | 10510 | 2897 | True |
| asciidoctor-locale-attributes-ja.adoc | asciidoc | rst | real | small(<16KiB) | structured | 732 | 6089 | 23739 | 10143 | 2798 | True |
| asciidoctor-locale-attributes-sr.adoc | asciidoc | rst | real | small(<16KiB) | structured | 863 | 6641 | 23348 | 9710 | 2768 | True |
| asciidoctor-pages-contexts-ref.adoc | asciidoc | asciidoc | real | small(<16KiB) | structured | 3640 | 15247 | 23774 | 9789 | 2854 | True |
| asciidoctor-pages-macos.adoc | asciidoc | rst | real | small(<16KiB) | structured | 1577 | 8386 | 23556 | 9636 | 2759 | True |
| asciidoctor-pages-stylesheet-modes.adoc | asciidoc | rst | real | small(<16KiB) | structured | 7284 | 22583 | 23560 | 14104 | 3104 | True |
| bibtex-build-strings-base.bib | bib | bib | real | medium(<256KiB) | structured | 145131 | 545657 | 26116 | 10954 | 3493 | True |
| bibtex-strings.bib | bib | bib | real | large(>=256KiB) | structured | 271806 | 1025695 | 30002 | 12984 | 4146 | True |
| hostile2-trunc-bib | bib | opaque | hostile | small(<16KiB) | hostile | 800 | 4012 | 18623 | 468 | 2791 | True |
| cbor-data-all_types.cbor | cbor | cbor | real | small(<16KiB) | nested | 354 | 6300 | 23137 | 9750 | 2663 | True |
| cbor-data-integer.cbor | cbor | opaque | real | small(<16KiB) | nested | 2 | 3214 | 18633 | 572 | 2793 | True |
| cbor-data-tagged_date.cbor | cbor | cbor | real | small(<16KiB) | nested | 22 | 3615 | 23286 | 9739 | 2827 | True |
| cbor-rfc8949-appendixA-mt2.cbor | cbor | cbor | real | small(<16KiB) | nested | 177 | 4742 | 23193 | 9630 | 2766 | True |
| cbor-rfc8949-appendixA-mt3.cbor | cbor | cbor | real | small(<16KiB) | nested | 487 | 6867 | 23278 | 9697 | 2725 | True |
| cbor-rfc8949-appendixA-streaming.cbor | cbor | cbor | real | small(<16KiB) | nested | 1145 | 13386 | 23914 | 9895 | 2916 | True |
| cbor-rfc8949-good.cbor | cbor | opaque | real | small(<16KiB) | nested | 13797 | 17009 | 19566 | 534 | 2762 | True |
| hostile2-trunc-cbor | cbor | opaque | hostile | small(<16KiB) | hostile | 40 | 3252 | 18623 | 516 | 2791 | True |
| poi-hpsf-TestVisioWithCodepage.vsd | cfbf | cfbf | real | medium(<256KiB) | nested | 45568 | 95591 | 24034 | 10061 | 3175 | True |
| poi-ole-poifs-only-zero-byte-streams.ole2 | cfbf | cfbf | real | small(<16KiB) | nested | 1536 | 7063 | 23231 | 9715 | 2691 | True |
| poi-publisher-LinkAt20.pub | cfbf | cfbf | real | medium(<256KiB) | nested | 64512 | 134375 | 24173 | 11701 | 3088 | True |
| poi-publisher-Sample_2010.pub | cfbf | cfbf | real | medium(<256KiB) | nested | 72704 | 150775 | 24051 | 9903 | 3146 | True |
| poi-publisher-clusterfuzz-testcase-minimized-POIHPBFFuzzer-4701121678278656.pub | cfbf | cfbf | real | large(>=256KiB) | nested | 331491 | 666405 | 27327 | 6116 | 4251 | True |
| cpython-libregrtest-mypy.ini | config | config | real | small(<16KiB) | simple | 997 | 7731 | 23482 | 13594 | 2932 | True |
| poi-gradle.properties | config | config | real | small(<16KiB) | simple | 555 | 5630 | 30494 | 10101 | 3019 | True |
| poi-resources-font_metrics.properties | config | config | real | medium(<256KiB) | simple | 146498 | 359532 | 28489 | 11007 | 3746 | True |
| cpython-namespace_pkgs-missing_directory.zip | container | container | real | small(<16KiB) | nested | 515 | 7457 | 20255 | 9842 | 2914 | True |
| cpython-test-recursion.tar | container | container | real | small(<16KiB) | nested | 516 | 4470 | 23661 | 5460 | 2959 | True |
| cpython-test-zipdir_backslash.zip | container | container | real | small(<16KiB) | nested | 192 | 6019 | 20078 | 9958 | 2948 | True |
| cpython-zipdata01-ziptestdata.zip | container | container | real | small(<16KiB) | nested | 876 | 9797 | 20344 | 12318 | 2943 | True |
| hostile2-trunc-container | container | opaque | hostile | small(<16KiB) | hostile | 5000 | 8212 | 18602 | 491 | 2770 | True |
| ne-zip-make-graticules-and-bounding-box-ne_1d5_graticules_beta2.zip | container | container | real | large(>=256KiB) | nested | 1000316 | 2055115 | 40892 | 13720 | 6643 | True |
| ne-zip-zip-ne_50m_graticules_1.zip | container | container | real | medium(<256KiB) | nested | 128416 | 264104 | 21295 | 10371 | 3478 | True |
| ne-zip-zip-ne_50m_graticules_15.zip | container | container | real | small(<16KiB) | nested | 10012 | 27300 | 20339 | 10068 | 3075 | True |
| ne-zip-zip-ne_50m_graticules_5.zip | container | container | real | medium(<256KiB) | nested | 27460 | 62192 | 20801 | 10002 | 3045 | True |
| poi-openxml4j-dcterms_bug_56479.zip | container | container | real | small(<16KiB) | nested | 1277 | 9934 | 19912 | 9631 | 2671 | True |
| parquet-delta-expect.csv | csv | csv | real | medium(<256KiB) | moderate | 159803 | 552658 | 25698 | 10697 | 3303 | True |
| vega-data-airports.csv | csv | csv | real | medium(<256KiB) | simple | 210363 | 893639 | 27285 | 12042 | 3697 | True |
| vega-data-co2-concentration.csv | csv | csv | real | medium(<256KiB) | simple | 18547 | 93286 | 24336 | 11619 | 2942 | True |
| vega-data-flights-airport.csv | csv | csv | real | medium(<256KiB) | simple | 65572 | 515711 | 24836 | 11584 | 3201 | True |
| vega-data-gapminder-health-income.csv | csv | csv | real | small(<16KiB) | simple | 8605 | 40460 | 23999 | 10059 | 3067 | True |
| vega-data-github.csv | csv | csv | real | medium(<256KiB) | simple | 21059 | 97252 | 24264 | 10172 | 2774 | True |
| vega-data-iowa-electricity.csv | csv | csv | real | small(<16KiB) | simple | 1531 | 10264 | 23243 | 9819 | 2822 | True |
| vega-data-population_engineers_hurricanes.csv | csv | csv | real | small(<16KiB) | simple | 1852 | 12779 | 24145 | 10059 | 2962 | True |
| vega-global-temp.csv | csv | csv | real | small(<16KiB) | simple | 1663 | 14668 | 23582 | 9658 | 2757 | True |
| vega-iowa-electricity.csv | csv | csv | real | small(<16KiB) | simple | 1531 | 10264 | 23472 | 9829 | 2850 | True |
| vega-population-engineers.csv | csv | csv | real | small(<16KiB) | simple | 1852 | 12779 | 23308 | 9645 | 2789 | True |
| vega-tsv-data-unemployment.tsv | csv | csv | real | medium(<256KiB) | simple | 34739 | 246812 | 24466 | 10673 | 2962 | True |
| hostile2-magic-doc | doc | opaque | hostile | medium(<256KiB) | hostile | 17920 | 21132 | 19500 | 643 | 2696 | True |
| poi-document-Bug41898.doc | doc | doc | real | medium(<256KiB) | nested | 24064 | 51548 | 422858 | 82680 | 39451 | True |
| poi-document-Bug46817.doc | doc | doc | real | medium(<256KiB) | nested | 17920 | 39260 | 476579 | 75123 | 151812 | True |
| poi-document-Bug53380_2.doc | doc | doc | real | medium(<256KiB) | nested | 31744 | 66908 | 434927 | 95661 | 33987 | True |
| poi-document-Bug60942b.doc | doc | cfbf | real | small(<16KiB) | nested | 6144 | 16749 | 385715 | 176684 | 38396 | True |
| poi-document-ThreeColHead.doc | doc | doc | real | medium(<256KiB) | nested | 27136 | 57692 | 430124 | 204832 | 79682 | True |
| poi-document-cap.stanford.edu_profiles_viewbiosketch_facultyid-4009-name-m_maciv | doc | doc | real | medium(<256KiB) | nested | 47104 | 97628 | 398866 | 89244 | 47971 | True |
| poi-document-parentinvguid.doc | doc | doc | real | large(>=256KiB) | nested | 398848 | 801116 | 412524 | 133152 | 60543 | True |
| poi-document-testRangeDelete.doc | doc | doc | real | medium(<256KiB) | nested | 104448 | 212316 | 436976 | 83548 | 41662 | True |
| hostile2-magic-docx | docx | opaque | hostile | small(<16KiB) | hostile | 10045 | 13257 | 19746 | 690 | 2953 | True |
| hostile2-trunc-docx | docx | docx | hostile | small(<16KiB) | hostile | 2297 | 14328 | 20078 | 28501 | 2947 | True |
| poi-document-58618.docx | docx | docx | real | small(<16KiB) | nested | 10045 | 52985 | 26266 | 556462 | 38082 | True |
| poi-document-60329.docx | docx | docx | real | medium(<256KiB) | nested | 31124 | 154025 | 364320 | 908399 | 73145 | True |
| poi-document-Bug51170.docx | docx | docx | real | medium(<256KiB) | nested | 239621 | 720218 | 445496 | 473055 | 33597 | True |
| poi-document-Bug60341.docx | docx | docx | real | small(<16KiB) | nested | 2297 | 14328 | 376323 | 730892 | 42538 | True |
| poi-document-NumberingWOverrides.docx | docx | docx | real | small(<16KiB) | nested | 15190 | 72123 | 399025 | 612332 | 3776 | True |
| poi-document-ThreeColHeadFoot.docx | docx | docx | real | small(<16KiB) | nested | 13123 | 66713 | 356796 | 759350 | 110810 | True |
| poi-document-capitalized.docx | docx | docx | real | small(<16KiB) | nested | 12104 | 73162 | 470348 | 674344 | 45247 | True |
| poi-document-checkboxes.docx | docx | docx | real | medium(<256KiB) | nested | 20800 | 108992 | 324775 | 716432 | 56166 | True |
| cpython-data-msg_08.txt | eml | eml | real | small(<16KiB) | structured | 454 | 5868 | 23372 | 9713 | 2734 | True |
| cpython-data-msg_22.txt | eml | eml | real | small(<16KiB) | structured | 1894 | 9580 | 23446 | 10232 | 3105 | True |
| cpython-data-msg_23.txt | eml | eml | real | small(<16KiB) | structured | 139 | 4246 | 23511 | 10033 | 3028 | True |
| cpython-data-msg_33.txt | eml | eml | real | small(<16KiB) | structured | 750 | 6266 | 23627 | 9925 | 2950 | True |
| cpython-data-msg_34.txt | eml | eml | real | small(<16KiB) | structured | 300 | 4725 | 23766 | 11330 | 3911 | True |
| cpython-data-msg_43.txt | eml | eml | real | small(<16KiB) | structured | 9166 | 24520 | 23588 | 10024 | 2937 | True |
| cpython-msg-01.eml | eml | eml | real | small(<16KiB) | simple | 459 | 5533 | 447146 | 183972 | 51717 | True |
| cpython-msg-25.eml | eml | opaque | real | small(<16KiB) | moderate | 5122 | 8334 | 617749 | 822 | 72168 | True |
| cpython-msg-43.eml | eml | eml | real | small(<16KiB) | complex | 9166 | 24520 | 426855 | 184051 | 45989 | True |
| hostile2-magic-epub | epub | opaque | hostile | medium(<256KiB) | hostile | 30552 | 33764 | 20639 | 807 | 3854 | True |
| tika-test-documents-testEPUB.epub | epub | epub | real | medium(<256KiB) | nested | 30552 | 72075 | 20374 | 9898 | 3039 | True |
| tika-test-documents-testEPUB_xml_ext.epub | epub | epub | real | small(<16KiB) | nested | 7823 | 29942 | 20506 | 9927 | 2856 | True |
| feed-1.1-extensive-noerror.xml | feed | feed | real | small(<16KiB) | structured | 1705 | 13105 | 23362 | 9749 | 2873 | True |
| feed-1.1-extensive-nowarn.xml | feed | feed | real | small(<16KiB) | structured | 1747 | 13189 | 24019 | 10176 | 3012 | True |
| feed-a33-3.3-duplicate-updated.xml | feed | feed | real | small(<16KiB) | structured | 1015 | 9234 | 23950 | 10288 | 3161 | True |
| feed-a33-3.3-published_no_year.xml | feed | feed | real | small(<16KiB) | structured | 875 | 7971 | 24822 | 11185 | 2985 | True |
| feed-a33-3.3-published_seconds.xml | feed | feed | real | small(<16KiB) | structured | 895 | 8011 | 24025 | 10477 | 3145 | True |
| feed-a33-3.3-updated-example2.xml | feed | feed | real | small(<16KiB) | structured | 706 | 7505 | 23571 | 9970 | 2901 | True |
| hostile2-trunc-geojson | geojson | opaque | hostile | small(<16KiB) | hostile | 2000 | 5212 | 19362 | 804 | 2859 | True |
| natural-earth-countries.geojson | geojson | geojson | real | large(>=256KiB) | complex | 838726 | 4054747 | 39735 | 19112 | 5679 | True |
| natural-earth-places.geojson | geojson | geojson | real | medium(<256KiB) | moderate | 166071 | 830240 | 26405 | 11349 | 3486 | True |
| ne-bbox.geojson | geojson | geojson | real | small(<16KiB) | simple | 4813 | 34888 | 23541 | 9949 | 2753 | True |
| ne-geojson-ne_10m_admin_0_antarctic_claim_limit_lines.geojson | geojson | geojson | real | medium(<256KiB) | structured | 58099 | 431186 | 26233 | 10926 | 2953 | True |
| ne-geojson-ne_10m_bathymetry_A_10000.geojson | geojson | geojson | real | small(<16KiB) | structured | 2860 | 18223 | 23275 | 9790 | 2770 | True |
| ne-geojson-ne_10m_parks_and_protected_lands_scale_rank.geojson | geojson | geojson | real | medium(<256KiB) | structured | 202844 | 1032397 | 28530 | 15158 | 5140 | True |
| ne-geojson-ne_110m_admin_0_boundary_lines_land.geojson | geojson | geojson | real | large(>=256KiB) | structured | 340010 | 1722622 | 30940 | 15485 | 4418 | True |
| ne-geojson-ne_110m_admin_1_states_provinces_lakes.geojson | geojson | geojson | real | medium(<256KiB) | structured | 181275 | 868587 | 27053 | 12679 | 3713 | True |
| ne-geojson-ne_110m_glaciated_areas.geojson | geojson | geojson | real | medium(<256KiB) | structured | 21720 | 116257 | 23738 | 9922 | 2634 | True |
| ne-geojson-ne_50m_geography_regions_elevation_points.geojson | geojson | geojson | real | medium(<256KiB) | structured | 104139 | 438795 | 25975 | 13294 | 3243 | True |
| ne-lakes.geojson | geojson | geojson | real | small(<16KiB) | moderate | 7719 | 43208 | 23532 | 9901 | 2796 | True |
| ne-regions.geojson | geojson | geojson | real | small(<16KiB) | simple | 4099 | 19576 | 23815 | 12261 | 4260 | True |
| hostile2-trunc-gis | gis | opaque | hostile | small(<16KiB) | hostile | 200 | 3412 | 18617 | 584 | 2858 | True |
| togeojson-data-bad.gpx | gis | gis | real | small(<16KiB) | structured | 833 | 8679 | 24015 | 10067 | 2995 | True |
| togeojson-data-blue_hills.gpx | gis | gis | real | large(>=256KiB) | structured | 268694 | 2069245 | 38298 | 15366 | 4465 | True |
| togeojson-data-null_geometry.kml | gis | gis | real | small(<16KiB) | structured | 286 | 4889 | 23579 | 10997 | 2893 | True |
| togeojson-data-opacity_override.kml | gis | gis | real | small(<16KiB) | structured | 625 | 7617 | 23551 | 10039 | 2957 | True |
| togeojson-data-opencpn.gpx | gis | gis | real | small(<16KiB) | structured | 10637 | 66507 | 24332 | 10107 | 2890 | True |
| togeojson-data-point_extra_coordinates.kml | gis | gis | real | small(<16KiB) | structured | 386 | 5465 | 23562 | 10404 | 2794 | True |
| togeojson-data-schema.kml | gis | gis | real | small(<16KiB) | structured | 2158 | 16426 | 23643 | 12947 | 3361 | True |
| togeojson-data-style.kml | gis | gis | real | small(<16KiB) | structured | 1372 | 13453 | 24087 | 9841 | 2851 | True |
| togeojson-data-styles.kml | gis | gis | real | small(<16KiB) | structured | 6374 | 44998 | 24202 | 9943 | 3124 | True |
| hostile2-trunc-hdf | hdf | opaque | hostile | small(<16KiB) | hostile | 600 | 3812 | 18723 | 713 | 2775 | True |
| netcdf-h5-h5_test-ref_tst_h_compounds2.h5 | hdf | hdf | real | small(<16KiB) | nested | 557 | 4534 | 23332 | 5422 | 3876 | True |
| netcdf-h5-nczarr_test-ref_groups.h5 | hdf | hdf | real | small(<16KiB) | nested | 9836 | 23282 | 23329 | 9764 | 2829 | True |
| netcdf-ncdump-ref_nc_test_netcdf4_4_0.nc | hdf | hdf | real | medium(<256KiB) | nested | 162812 | 329044 | 25462 | 5661 | 3676 | True |
| netcdf-ncdump-ref_no_ncproperty.nc | hdf | hdf | real | small(<16KiB) | nested | 1262 | 5944 | 24482 | 5339 | 2898 | True |
| netcdf-ncdump-ref_test_365_day_1900.nc | hdf | hdf | real | small(<16KiB) | nested | 9348 | 22248 | 25414 | 10186 | 3014 | True |
| netcdf-ncdump-ref_tst_compounds2.nc | hdf | hdf | real | small(<16KiB) | nested | 5753 | 14926 | 23732 | 5552 | 2856 | True |
| netcdf-ncdump-ref_tst_compounds4.nc | hdf | hdf | real | small(<16KiB) | nested | 526 | 4472 | 23495 | 5359 | 3003 | True |
| netcdf-nctestfiles-test_vlen2.nc | hdf | hdf | real | small(<16KiB) | nested | 6336 | 16092 | 23761 | 5284 | 2981 | True |
| feedval-05-version_12_supports_atom_02.html | html | html | real | medium(<256KiB) | structured | 47054 | 110374 | 28446 | 10115 | 3232 | True |
| feedval-21-live.html | html | html | real | small(<16KiB) | structured | 2277 | 19579 | 24124 | 10265 | 3016 | True |
| feedval-22-known_bugs.html | html | html | real | small(<16KiB) | structured | 3478 | 24861 | 23583 | 10536 | 2718 | True |
| feedval-29-new_mailing_list_for_validator_users.html | html | html | real | small(<16KiB) | structured | 2823 | 21310 | 23861 | 10018 | 3002 | True |
| mdn-doc-structure.html | html | html | real | small(<16KiB) | moderate | 3525 | 20244 | 23750 | 9611 | 2788 | True |
| mdn-fonts-demo.html | html | html | real | medium(<256KiB) | complex | 36867 | 189800 | 25511 | 9976 | 2853 | True |
| mdn-getting-started.html | html | html | real | small(<16KiB) | simple | 224 | 5429 | 23767 | 9674 | 2713 | True |
| mdn-planets.html | html | html | real | small(<16KiB) | moderate | 4302 | 39076 | 23175 | 9770 | 2713 | True |
| cpython-fuzz_json_loads_corpus-empty_array.json | json | json | real | small(<16KiB) | structured | 3 | 3509 | 23564 | 9959 | 2914 | True |
| cpython-fuzz_json_loads_corpus-pass1.json | json | json | real | small(<16KiB) | structured | 1441 | 8910 | 23463 | 9950 | 2892 | True |
| cpython-fuzz_json_loads_corpus-simple_array.json | json | json | real | small(<16KiB) | structured | 25 | 3678 | 23415 | 9976 | 2903 | True |
| hostile2-magic-json | json | opaque | hostile | small(<16KiB) | hostile | 2743 | 5955 | 19011 | 608 | 2765 | True |
| vega-data-barley.json | json | json | real | small(<16KiB) | structured | 8487 | 47477 | 23652 | 9748 | 2814 | True |
| vega-data-budget.json | json | json | real | large(>=256KiB) | structured | 391353 | 1645334 | 34528 | 14247 | 4283 | True |
| vega-data-burtin.json | json | json | real | small(<16KiB) | structured | 2743 | 14189 | 23780 | 9685 | 2745 | True |
| vega-data-penguins.json | json | json | real | medium(<256KiB) | structured | 67119 | 266741 | 24998 | 10360 | 3111 | True |
| vega-data-volcano.json | json | json | real | medium(<256KiB) | structured | 21167 | 178662 | 23699 | 10361 | 2820 | True |
| vega-data-weekly-weather.json | json | json | real | small(<16KiB) | structured | 1281 | 12815 | 23208 | 9978 | 2850 | True |
| vega-data-world-110m.json | json | json | real | medium(<256KiB) | structured | 119410 | 1076373 | 30133 | 12780 | 4019 | True |
| mdx-js-renovate.json5 | json5 | json5 | real | small(<16KiB) | structured | 92 | 3852 | 23555 | 9948 | 2937 | True |
| jsonlines-datagov100.json | jsonl | jsonl | real | large(>=256KiB) | complex | 696007 | 2253927 | 34450 | 15876 | 5311 | True |
| jsonlines-datagov100.json | jsonl | jsonl | real | large(>=256KiB) | structured | 696007 | 2253927 | 35045 | 16391 | 5757 | True |
| oai-data-dbpedia_samples.jsonl | jsonl | jsonl | real | medium(<256KiB) | structured | 64512 | 167137 | 24469 | 10163 | 3175 | True |
| oai-data-drone_training.jsonl | jsonl | jsonl | real | large(>=256KiB) | structured | 387706 | 1837044 | 30132 | 17489 | 4560 | True |
| oai-data-example_requests_to_parallel_process.jsonl | jsonl | jsonl | real | large(>=256KiB) | structured | 548917 | 2831447 | 32652 | 18012 | 5169 | True |
| oai-data-toy_chat_fine_tuning.jsonl | jsonl | jsonl | real | medium(<256KiB) | structured | 27385 | 61273 | 24279 | 10142 | 3064 | True |
| oai-dbpedia.jsonl | jsonl | jsonl | real | medium(<256KiB) | moderate | 64512 | 167137 | 279275 | 162707 | 45623 | True |
| oai-parallel-requests.jsonl | jsonl | jsonl | real | large(>=256KiB) | complex | 548917 | 2831447 | 448854 | 217285 | 57677 | True |
| oai-toy-chat.jsonl | jsonl | jsonl | real | medium(<256KiB) | moderate | 27385 | 61273 | 24859 | 10196 | 60216 | True |
| loghub-HPC-HPC_2k.log | logstream | opaque | real | medium(<256KiB) | simple | 151178 | 154390 | 31724 | 890 | 3149 | True |
| loghub-Mac-Mac_2k.log | logstream | logstream | real | large(>=256KiB) | simple | 319414 | 846329 | 40545 | 11677 | 4069 | True |
| loghub-OpenStack-OpenStack_2k.log | logstream | config | real | large(>=256KiB) | simple | 595119 | 1329738 | 35434 | 17078 | 5315 | True |
| loghub-Proxifier-Proxifier_2k.log | logstream | logstream | real | medium(<256KiB) | simple | 236962 | 613425 | 37751 | 11383 | 3923 | True |
| loghub-Spark-Spark_2k.log | logstream | logstream | real | medium(<256KiB) | simple | 196268 | 566037 | 36835 | 11076 | 3689 | True |
| loghub-Windows-Windows_2k.log | logstream | csv | real | large(>=256KiB) | simple | 285433 | 701911 | 28081 | 15288 | 4030 | True |
| commonmark-readme.md | markdown | markdown | real | small(<16KiB) | simple | 7671 | 23143 | 23832 | 9665 | 2713 | True |
| commonmark-spec.txt | markdown | markdown | real | medium(<256KiB) | complex | 205025 | 530161 | 33464 | 11816 | 3720 | True |
| cpython-ISSUE_TEMPLATE-crash.md | markdown | markdown | real | small(<16KiB) | simple | 1142 | 6380 | 25992 | 10125 | 2992 | True |
| cpython-rhel7-README.md | markdown | markdown | real | small(<16KiB) | simple | 608 | 5323 | 23366 | 9713 | 2874 | True |
| cpython-wasm-README.md | markdown | markdown | real | medium(<256KiB) | simple | 16560 | 51340 | 24872 | 10112 | 2813 | True |
| rust-readme.md | markdown | markdown | real | small(<16KiB) | simple | 3137 | 14075 | 24134 | 10525 | 3211 | True |
| mdx-blog-conf.mdx | mdx | mdx | real | small(<16KiB) | structured | 2790 | 12397 | 23409 | 9734 | 2756 | True |
| mdx-blog-v1.mdx | mdx | mdx | real | small(<16KiB) | structured | 9150 | 33026 | 24460 | 10318 | 2990 | True |
| mdx-community-sponsor.mdx | mdx | mdx | real | small(<16KiB) | structured | 5513 | 15451 | 23951 | 10209 | 3109 | True |
| mdx-docs-404.mdx | mdx | mdx | real | small(<16KiB) | structured | 545 | 5246 | 23637 | 9757 | 2816 | True |
| mdx-docs-getting-started.mdx | mdx | mdx | real | medium(<256KiB) | structured | 26777 | 87116 | 26833 | 10492 | 3111 | True |
| mdx-guides-embed.mdx | mdx | mdx | real | small(<16KiB) | structured | 3652 | 13099 | 23637 | 12438 | 3915 | True |
| mdx-packages-index.mdx | mdx | mdx | real | small(<16KiB) | structured | 995 | 6389 | 24796 | 11160 | 2766 | True |
| hostile2-trunc-mhtml | mhtml | mhtml | hostile | small(<16KiB) | hostile | 1000 | 7936 | 23068 | 11987 | 3792 | True |
| tika-test-documents-testMHTMLFirefox.mhtml | mhtml | mhtml | real | small(<16KiB) | nested | 13759 | 30944 | 23664 | 5311 | 2916 | True |
| tika-test-documents-testUserDefinedCharset.mhtml | mhtml | mhtml | real | small(<16KiB) | nested | 621 | 6217 | 24100 | 10274 | 3000 | True |
| hostile2-trunc-msg | msg | opaque | hostile | small(<16KiB) | hostile | 1500 | 4712 | 18721 | 591 | 2803 | True |
| poi-hsmf-ASCII_CP1251_LCID1049.msg | msg | msg | real | small(<16KiB) | nested | 3584 | 11471 | 121703 | 53062 | 11464 | True |
| poi-hsmf-attachment_msg_inlineImg.msg | msg | msg | real | medium(<256KiB) | nested | 147456 | 298332 | 33276 | 9046 | 5890 | True |
| poi-hsmf-example_sent_regular.msg | msg | msg | real | medium(<256KiB) | nested | 53248 | 109916 | 41093 | 8183 | 3687 | True |
| poi-hsmf-logsat.com_signatures_valid.msg | msg | msg | real | medium(<256KiB) | nested | 23040 | 49500 | 37213 | 12148 | 6141 | True |
| poi-hsmf-message_1981.msg | msg | msg | real | medium(<256KiB) | nested | 29696 | 62812 | 36479 | 8364 | 4919 | True |
| poi-hsmf-simple_test_msg.msg | msg | msg | real | medium(<256KiB) | nested | 16896 | 37212 | 36609 | 6860 | 3991 | True |
| hostile2-trunc-notebook | notebook | notebook | hostile | small(<16KiB) | hostile | 355 | 5075 | 24033 | 10171 | 3163 | True |
| jupyter-Notebook-Custom-Keyboard-Shortcuts.ipynb | notebook | notebook | real | small(<16KiB) | structured | 1148 | 7493 | 23523 | 9969 | 2911 | True |
| jupyter-Notebook-Importing-Notebooks.ipynb | notebook | notebook | real | small(<16KiB) | structured | 13552 | 47792 | 23595 | 9946 | 2874 | True |
| jupyter-Notebook-Running-Code.ipynb | notebook | notebook | real | medium(<256KiB) | structured | 52012 | 133052 | 24532 | 10344 | 3077 | True |
| jupyter-Notebook-What-is-the-Jupyter-Notebook.ipynb | notebook | notebook | real | small(<16KiB) | structured | 7541 | 24181 | 23712 | 10243 | 2977 | True |
| jupyter-nbs-other.ipynb | notebook | notebook | real | small(<16KiB) | structured | 786 | 6744 | 23583 | 10047 | 3045 | True |
| jupyter-notebooks-simple.ipynb | notebook | notebook | real | small(<16KiB) | structured | 355 | 5075 | 23438 | 10005 | 2962 | True |
| jupyter-notebooks-simple_toc.ipynb | notebook | notebook | real | small(<16KiB) | structured | 1040 | 7873 | 23647 | 9988 | 3002 | True |
| odfpy-cols.odp | odp | odp | real | medium(<256KiB) | moderate | 18037 | 148305 | 20147 | 37325 | 2816 | True |
| odfpy-emb.odp | odp | odp | real | medium(<256KiB) | complex | 16403 | 106233 | 20868 | 36979 | 2801 | True |
| odfpy-examples-cols.odp | odp | odp | real | medium(<256KiB) | nested | 18037 | 148305 | 20536 | 37740 | 3031 | True |
| odfpy-examples-ol.odp | odp | odp | real | small(<16KiB) | nested | 14940 | 138597 | 20213 | 40584 | 4199 | True |
| odfpy-ol.odp | odp | odp | real | small(<16KiB) | simple | 14940 | 138597 | 21394 | 37242 | 2817 | True |
| odfpy-chinese.ods | ods | ods | real | small(<16KiB) | moderate | 11765 | 44661 | 20706 | 23470 | 2787 | True |
| odfpy-empty.ods | ods | ods | real | small(<16KiB) | simple | 6819 | 29699 | 19780 | 23219 | 2718 | True |
| odfpy-examples-chinese_spreadsheet.ods | ods | ods | real | small(<16KiB) | nested | 11765 | 44661 | 20379 | 23351 | 2950 | True |
| odfpy-examples-empty.ods | ods | ods | real | small(<16KiB) | nested | 6819 | 29699 | 20130 | 23365 | 2816 | True |
| odfpy-examples-pythagoras-kspread.ods | ods | ods | real | small(<16KiB) | nested | 5037 | 22024 | 20337 | 26159 | 2852 | True |
| odfpy-examples-pythagoras.ods | ods | ods | real | small(<16KiB) | nested | 6837 | 30406 | 19859 | 28061 | 2827 | True |
| odfpy-pyth-kspread.ods | ods | ods | real | small(<16KiB) | simple | 5037 | 22024 | 19957 | 23304 | 2756 | True |
| odfpy-pythagoras.ods | ods | ods | real | small(<16KiB) | simple | 6837 | 30406 | 20052 | 25010 | 2957 | True |
| hostile2-trunc-odt | odt | opaque | hostile | small(<16KiB) | hostile | 3000 | 6212 | 18732 | 629 | 2803 | True |
| odfpy-examples-definitionlists.odt | odt | odt | real | small(<16KiB) | nested | 7824 | 32070 | 21002 | 27095 | 3081 | True |
| odfpy-examples-nasty.odt | odt | odt | real | small(<16KiB) | nested | 1539 | 11051 | 20085 | 19023 | 2916 | True |
| odfpy-examples-no_userfields.odt | odt | odt | real | medium(<256KiB) | nested | 19228 | 52978 | 20977 | 24079 | 3105 | True |
| odfpy-examples-parastyles.odt | odt | odt | real | small(<16KiB) | nested | 7194 | 32845 | 20839 | 24165 | 3871 | True |
| odfpy-examples-serious_poem.odt | odt | odt | real | small(<16KiB) | nested | 8539 | 31745 | 20399 | 23787 | 3047 | True |
| odfpy-examples-simpletable.odt | odt | odt | real | small(<16KiB) | nested | 7055 | 31250 | 21226 | 24115 | 3176 | True |
| odfpy-examples-userfields_ooo3.odt | odt | odt | real | medium(<256KiB) | nested | 38420 | 98623 | 20567 | 24007 | 3338 | True |
| hostile2-trunc-parquet | parquet | opaque | hostile | small(<16KiB) | hostile | 800 | 4012 | 20810 | 545 | 2720 | True |
| parquet-alltypes-plain.parquet | parquet | parquet | real | small(<16KiB) | simple | 1851 | 9203 | 437032 | 194267 | 58313 | True |
| parquet-alltypes-tiny.parquet | parquet | parquet | real | large(>=256KiB) | complex | 454233 | 914573 | 468669 | 177845 | 56744 | True |
| parquet-data-datapage_v2.snappy.parquet | parquet | parquet | real | small(<16KiB) | nested | 1165 | 6792 | 23449 | 9734 | 2746 | True |
| parquet-data-fixed_length_byte_array.parquet | parquet | parquet | real | small(<16KiB) | nested | 4437 | 12708 | 23129 | 9642 | 2778 | True |
| parquet-data-fixed_length_decimal.parquet | parquet | parquet | real | small(<16KiB) | nested | 677 | 5166 | 23127 | 9769 | 2809 | True |
| parquet-data-float16_nonzeros_and_nans.parquet | parquet | parquet | real | small(<16KiB) | nested | 501 | 4788 | 26151 | 9694 | 2720 | True |
| parquet-data-lz4_raw_compressed_larger.parquet | parquet | parquet | real | large(>=256KiB) | nested | 380836 | 765507 | 27558 | 11306 | 5329 | True |
| parquet-data-nation.dict-malformed.parquet | parquet | parquet | real | small(<16KiB) | nested | 2850 | 9855 | 23109 | 9511 | 2768 | True |
| parquet-delta-binary.parquet | parquet | parquet | real | medium(<256KiB) | complex | 72971 | 161889 | 451063 | 180690 | 39925 | True |
| parquet-geospatial-geography-points.parquet | parquet | parquet | real | medium(<256KiB) | nested | 33026 | 81965 | 23440 | 9969 | 2908 | True |
| asciidoctor-fixtures-hello-asciidoctor.pdf | pdf | pdf | real | small(<16KiB) | nested | 847 | 6557 | 23752 | 756 | 2838 | True |
| cpython-library-turtle-star.pdf | pdf | pdf | real | small(<16KiB) | nested | 4418 | 12438 | 23598 | 652 | 3446 | True |
| netcdf-docs-docmap.pdf | pdf | pdf | real | medium(<256KiB) | nested | 64347 | 78695 | 25327 | 840 | 3078 | True |
| cargo-cargo.toml | pkgmeta | pkgmeta | real | small(<16KiB) | complex | 7717 | 78906 | 23328 | 9814 | 2735 | True |
| jupyter-package.json | pkgmeta | pkgmeta | real | small(<16KiB) | structured | 3448 | 17137 | 23443 | 9949 | 2877 | True |
| jupyter-py-pyproject.toml | pkgmeta | pkgmeta | real | small(<16KiB) | structured | 7903 | 50514 | 23541 | 11766 | 2892 | True |
| serde-cargo.toml | pkgmeta | pkgmeta | real | small(<16KiB) | moderate | 2431 | 17167 | 23159 | 9600 | 2691 | True |
| vega-package.json | pkgmeta | pkgmeta | real | small(<16KiB) | structured | 2497 | 15674 | 23425 | 9708 | 2799 | True |
| hostile2-trunc-ppt | ppt | cfbf | hostile | small(<16KiB) | hostile | 8000 | 19423 | 23385 | 5339 | 2842 | True |
| poi-slideshow-42474-2.ppt | ppt | ppt | real | large(>=256KiB) | nested | 571392 | 1146204 | 133633 | 41253 | 31930 | True |
| poi-slideshow-54332a.ppt | ppt | ppt | real | medium(<256KiB) | nested | 67584 | 138588 | 125974 | 27031 | 13310 | True |
| poi-slideshow-Single_Coloured_Page.ppt | ppt | ppt | real | small(<16KiB) | nested | 9728 | 22876 | 113202 | 25819 | 12756 | True |
| poi-slideshow-basic_test_ppt_file.ppt | ppt | ppt | real | small(<16KiB) | nested | 15360 | 34140 | 119004 | 29250 | 12576 | True |
| poi-slideshow-bug62092.ppt | ppt | ppt | real | medium(<256KiB) | nested | 161792 | 327004 | 116728 | 26236 | 17438 | True |
| poi-slideshow-datetime.ppt | ppt | ppt | real | medium(<256KiB) | nested | 114176 | 231772 | 115624 | 23654 | 14505 | True |
| hostile2-trunc-pptx | pptx | opaque | hostile | medium(<256KiB) | hostile | 120000 | 123212 | 20854 | 855 | 3542 | True |
| poi-61515.pptx | pptx | pptx | real | medium(<256KiB) | moderate | 29270 | 99853 | 21337 | 42581 | 3414 | True |
| poi-Divino.pptx | pptx | opaque | real | large(>=256KiB) | complex | 523999 | 527211 | 32194 | 1813 | 4977 | True |
| poi-SampleShow.pptx | pptx | pptx | real | medium(<256KiB) | moderate | 39083 | 132308 | 27930 | 49215 | 3181 | True |
| poi-bug60993.pptx | pptx | pptx | real | medium(<256KiB) | complex | 21942 | 95612 | 21611 | 48713 | 3130 | True |
| poi-slideshow-49386-null_dates.pptx | pptx | pptx | real | medium(<256KiB) | nested | 17124 | 62331 | 411425 | 886063 | 79657 | True |
| poi-slideshow-au.asn.aes.www_conferences_2011_presentations_Fri_20Room4Level4_20 | pptx | pptx | real | large(>=256KiB) | nested | 536086 | 1622797 | 661113 | 1004229 | 37734 | True |
| poi-slideshow-bug58144-headers-footers-2007.pptx | pptx | pptx | real | medium(<256KiB) | nested | 42035 | 136662 | 380454 | 867361 | 41885 | True |
| poi-slideshow-bug62736.pptx | pptx | pptx | real | medium(<256KiB) | nested | 50926 | 164564 | 469762 | 1104872 | 63612 | True |
| poi-slideshow-bug65551.pptx | pptx | pptx | real | medium(<256KiB) | nested | 34383 | 110096 | 399129 | 864618 | 30035 | True |
| poi-slideshow-pptx2svg.pptx | pptx | pptx | real | medium(<256KiB) | nested | 149172 | 467022 | 380496 | 951926 | 44569 | True |
| poi-table_test.pptx | pptx | pptx | real | medium(<256KiB) | moderate | 28935 | 103361 | 21762 | 43256 | 3201 | True |
| cpython-faq-gui.rst | rst | rst | real | small(<16KiB) | structured | 3058 | 11685 | 23721 | 13780 | 3052 | True |
| cpython-library-asyncio-protocol.rst | rst | rst | real | medium(<256KiB) | structured | 32975 | 83009 | 29183 | 10087 | 3131 | True |
| cpython-library-mmap.rst | rst | rst | real | small(<16KiB) | structured | 14634 | 35272 | 24705 | 10228 | 2911 | True |
| cpython-library-python.rst | rst | rst | real | small(<16KiB) | structured | 502 | 4725 | 23573 | 9842 | 2842 | True |
| cpython-library-sunau.rst | rst | rst | real | small(<16KiB) | structured | 7423 | 23681 | 24249 | 10472 | 3085 | True |
| cpython-whatsnew-3.3.rst | rst | markdown | real | medium(<256KiB) | structured | 99150 | 238877 | 27165 | 10495 | 3288 | True |
| hostile2-trunc-rtf | rtf | opaque | hostile | small(<16KiB) | hostile | 1000 | 4212 | 18558 | 502 | 2801 | True |
| ne-10m_cultural-10m_us_parks_-READ_ME.rtf | rtf | rtf | real | small(<16KiB) | structured | 2058 | 11954 | 23398 | 9769 | 2765 | True |
| ne-10m_parks_and_protected_areas-10m_us_parks_READ_ME.rtf | rtf | rtf | real | small(<16KiB) | structured | 3718 | 18579 | 23355 | 10536 | 2886 | True |
| poi-quick-contents-message.rtf | rtf | rtf | real | medium(<256KiB) | structured | 25528 | 157229 | 30877 | 13462 | 4310 | True |
| hostile2-magic-toml | toml | toml | hostile | small(<16KiB) | hostile | 37 | 4013 | 23432 | 9850 | 2963 | True |
| toml-array-string-quote-comma-02.toml | toml | toml | real | small(<16KiB) | structured | 20 | 3692 | 23302 | 10884 | 3891 | True |
| toml-datetime-local.toml | toml | toml | real | small(<16KiB) | structured | 88 | 3951 | 23382 | 9742 | 2771 | True |
| toml-key-escapes.toml | toml | toml | real | small(<16KiB) | structured | 163 | 4593 | 23346 | 9690 | 2760 | True |
| toml-spec-1.0.0-float-2.toml | toml | toml | real | small(<16KiB) | structured | 276 | 4701 | 23702 | 10133 | 2832 | True |
| toml-spec-1.1.0-common-0.toml | toml | toml | real | small(<16KiB) | structured | 122 | 3969 | 23263 | 9804 | 2858 | True |
| toml-string-basic-escape-03.toml | toml | toml | real | small(<16KiB) | structured | 56 | 3739 | 23322 | 9701 | 2807 | True |
| toml-valid-implicit-and-explicit-before.toml | toml | toml | real | small(<16KiB) | structured | 37 | 4013 | 23302 | 9719 | 2782 | True |
| toml-valid-multibyte.toml | toml | toml | real | small(<16KiB) | structured | 1156 | 7010 | 23246 | 9604 | 2710 | True |
| hostile2-trunc-xls | xls | cfbf | hostile | small(<16KiB) | hostile | 1500 | 6423 | 23679 | 7609 | 2792 | True |
| poi-spreadsheet-43493.xls | xls | xls | real | medium(<256KiB) | nested | 66048 | 135516 | 492600 | 81557 | 50077 | True |
| poi-spreadsheet-47251_1.xls | xls | xls | real | medium(<256KiB) | nested | 26112 | 102820 | 449368 | 192750 | 50416 | True |
| poi-spreadsheet-48325.xls | xls | xls | real | small(<16KiB) | nested | 4096 | 15039 | 376034 | 201271 | 33764 | True |
| poi-spreadsheet-49524.xls | xls | xls | real | small(<16KiB) | nested | 13824 | 31068 | 468319 | 102642 | 55409 | True |
| poi-spreadsheet-50298.xls | xls | xls | real | medium(<256KiB) | nested | 20480 | 61291 | 442153 | 197664 | 27912 | True |
| poi-spreadsheet-DStar.xls | xls | xls | real | medium(<256KiB) | nested | 35328 | 104549 | 439366 | 275067 | 39633 | True |
| poi-spreadsheet-EmbeddedChartHeaderTest.xls | xls | xls | real | small(<16KiB) | nested | 15360 | 43754 | 471982 | 193441 | 43208 | True |
| poi-spreadsheet-bug66319.xls | xls | xls | real | large(>=256KiB) | nested | 387072 | 777564 | 415878 | 80858 | 62935 | True |
| hostile2-trunc-xlsx | xlsx | opaque | hostile | small(<16KiB) | hostile | 15000 | 18212 | 18891 | 753 | 2880 | True |
| poi-47889.xlsx | xlsx | xlsx | real | small(<16KiB) | simple | 3555 | 19151 | 20089 | 28246 | 2768 | True |
| poi-49609.xlsx | xlsx | xlsx | real | medium(<256KiB) | complex | 105424 | 218879 | 21199 | 5655 | 3273 | True |
| poi-59021.xlsx | xlsx | xlsx | real | small(<16KiB) | moderate | 1933 | 12665 | 20265 | 32324 | 3066 | True |
| poi-DataValidation.xlsx | xlsx | xlsx | real | small(<16KiB) | moderate | 3286 | 18607 | 20278 | 28442 | 2956 | True |
| poi-linkext.xlsx | xlsx | xlsx | real | small(<16KiB) | complex | 3274 | 18583 | 20074 | 43106 | 3358 | True |
| poi-spreadsheet-123233_charts.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 12957 | 47188 | 378593 | 667475 | 40588 | True |
| poi-spreadsheet-56511.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 9688 | 35780 | 383430 | 474186 | 50137 | True |
| poi-spreadsheet-58731.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 9023 | 33113 | 372768 | 590901 | 129209 | True |
| poi-spreadsheet-59021.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 1933 | 12665 | 380701 | 627023 | 50887 | True |
| poi-spreadsheet-StructuredRefs-lots-with-lookups.xlsx | xlsx | xlsx | real | large(>=256KiB) | nested | 325446 | 687123 | 385195 | 286123 | 21975 | True |
| poi-spreadsheet-TwoSheetsOneHidden.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 7962 | 30038 | 374361 | 555153 | 52280 | True |
| poi-spreadsheet-WithChartSheet.xlsx | xlsx | xlsx | real | medium(<256KiB) | nested | 30333 | 99513 | 416695 | 552551 | 46871 | True |
| poi-spreadsheet-duplicate-filename-case-insensitive.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 10721 | 30742 | 403326 | 80139 | 52058 | True |
| maven-core-pom.xml | xml | xml | real | small(<16KiB) | moderate | 7411 | 46651 | 23846 | 10043 | 3002 | True |
| maven-pom.xml | xml | xml | real | medium(<256KiB) | complex | 28489 | 167485 | 25148 | 10702 | 3220 | True |
| poi-quick-contents-quick.xml | xml | xml | real | small(<16KiB) | structured | 143 | 4131 | 23488 | 13320 | 2663 | True |
| poi-spreadsheet-SampleSS.xml | xml | xml | real | small(<16KiB) | structured | 5115 | 36467 | 23584 | 9954 | 2870 | True |
| alertmanager-ci.yml | yaml | yaml | real | small(<16KiB) | simple | 1348 | 9179 | 23198 | 9511 | 2693 | True |
| alertmanager-simple.yml | yaml | yaml | real | small(<16KiB) | moderate | 3953 | 16344 | 23316 | 9598 | 2687 | True |
| cpython-workflows-build.yml | yaml | yaml | real | medium(<256KiB) | structured | 19106 | 65679 | 24065 | 10071 | 3068 | True |
| cpython-workflows-project-updater.yml | yaml | config | real | small(<16KiB) | structured | 741 | 7022 | 23273 | 9618 | 2797 | True |
| cpython-workflows-require-pr-label.yml | yaml | yaml | real | small(<16KiB) | structured | 507 | 5822 | 23259 | 9841 | 2951 | True |
| cpython-workflows-reusable-ubuntu.yml | yaml | yaml | real | small(<16KiB) | structured | 3356 | 14842 | 23528 | 10328 | 3766 | True |
| cpython-workflows-reusable-windows.yml | yaml | yaml | real | small(<16KiB) | structured | 1157 | 7847 | 23484 | 9864 | 2944 | True |
| hostile2-trunc-yaml | yaml | yaml | hostile | small(<16KiB) | hostile | 4000 | 17939 | 23716 | 9741 | 2827 | True |
| prometheus-ci.yml | yaml | yaml | real | small(<16KiB) | complex | 10201 | 39596 | 23502 | 9720 | 2748 | True |
| prometheus-web-config.yml | yaml | yaml | real | small(<16KiB) | simple | 531 | 5022 | 23095 | 9634 | 2699 | True |
| prometheus.yml | yaml | yaml | real | small(<16KiB) | moderate | 934 | 6465 | 23148 | 9573 | 2732 | True |

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
