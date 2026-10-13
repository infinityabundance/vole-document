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
| 308 | 284 | 24 | 0 | 0 | 308 | 279 | 10 | 41 |

By declared format (ran): arrow=9, asciidoc=7, bib=3, cbor=8, cfbf=5, config=3, container=10, csv=12, doc=9, docx=10, eml=9, epub=3, feed=6, geojson=13, gis=10, hdf=9, html=8, json=11, json5=1, jsonl=9, logstream=6, markdown=6, mdx=7, mhtml=3, msg=7, notebook=8, odp=5, ods=8, odt=8, parquet=11, pdf=3, pkgmeta=5, ppt=7, pptx=12, rst=6, rtf=4, toml=9, xls=9, xlsx=14, xml=4, yaml=11

## Stratified tables

## By declared format

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| arrow | 9 | 213295 | 22973 | 1519711 | 8335 | 83926 | 9623 | 28165 | 2702 | yes |
| asciidoc | 7 | 173020 | 23405 | 376494 | 8861 | 70668 | 9631 | 21339 | 2731 | yes |
| bib | 3 | 73059 | 26022 | 1575364 | 545657 | 23349 | 11125 | 9819 | 3353 | yes |
| cbor | 8 | 171928 | 22861 | 58385 | 5521.0 | 49294 | 9475 | 22696 | 2688 | yes |
| cfbf | 5 | 121585 | 23880 | 1054209 | 134375 | 47649 | 9654 | 15670 | 2993 | yes |
| config | 3 | 73060 | 23108 | 372893 | 7731 | 29883 | 9622 | 8652 | 2795 | yes |
| container | 10 | 223905 | 19860 | 2454600 | 9865.5 | 86736 | 9516 | 32382 | 2781 | yes |
| csv | 12 | 287029 | 23339 | 2500572 | 66873.0 | 123142 | 9768 | 35404 | 2757 | yes |
| doc | 9 | 212899 | 23497 | 1364349 | 57692 | 50245 | 5310 | 27424 | 2810 | yes |
| docx | 10 | 202009 | 19926 | 1290131 | 69418.0 | 325162 | 36598 | 30015 | 2851 | yes |
| eml | 9 | 388700 | 23052 | 93592 | 6266 | 77133 | 9577 | 24367 | 2714 | yes |
| epub | 3 | 60219 | 20108 | 135781 | 33764 | 20064 | 9777 | 8244 | 2759 | yes |
| feed | 6 | 138213 | 23012 | 59015 | 8622.5 | 59251 | 9538 | 17456 | 2722 | yes |
| geojson | 13 | 333857 | 24407 | 9615938 | 431186 | 141497 | 10643 | 44781 | 3292 | yes |
| gis | 10 | 240327 | 23134 | 2240691 | 11066.0 | 94892 | 9610 | 27876 | 2689 | yes |
| hdf | 9 | 205197 | 23073 | 424354 | 14926 | 50119 | 5058 | 25358 | 2761 | yes |
| html | 8 | 192598 | 23216 | 430673 | 23085.5 | 81739 | 9642 | 21782 | 2704 | yes |
| json | 11 | 271575 | 23797 | 3263643 | 14189 | 104244 | 9687 | 33301 | 2801 | yes |
| json5 | 1 | 23092 | 23092 | 3852 | 3852 | 12303 | 12303 | 2675 | 2675 | yes |
| jsonl | 9 | 259026 | 29958 | 12464612 | 1837044 | 119440 | 14784 | 37367 | 4158 | yes |
| logstream | 6 | 207235 | 35983 | 4211830 | 657668.0 | 59078 | 11094 | 24004 | 3846 | yes |
| markdown | 6 | 151565 | 23443 | 630422 | 18609.0 | 59370 | 9570 | 16952 | 2700 | yes |
| mdx | 7 | 165381 | 23228 | 172724 | 13099 | 67755 | 9672 | 19171 | 2735 | yes |
| mhtml | 3 | 69603 | 23151 | 45097 | 7936 | 24290 | 9562 | 8266 | 2764 | yes |
| msg | 7 | 163484 | 23435 | 573955 | 49500 | 36351 | 5110 | 19946 | 2756 | yes |
| notebook | 8 | 187277 | 23134 | 237285 | 7683.0 | 81557 | 9585 | 22876 | 2742 | yes |
| odp | 5 | 100325 | 20036 | 680037 | 138597 | 188802 | 37178 | 14257 | 2849 | yes |
| ods | 8 | 160407 | 20022 | 253580 | 30052.5 | 189852 | 23166 | 22652 | 2748 | yes |
| odt | 8 | 161245 | 20027 | 296774 | 31907.5 | 165522 | 23350 | 23613 | 2802 | yes |
| parquet | 11 | 261506 | 23068 | 1976458 | 9855 | 101456 | 9693 | 34486 | 2815 | yes |
| pdf | 3 | 70702 | 23138 | 97690 | 12438 | 1449 | 449 | 9340 | 2744 | yes |
| pkgmeta | 5 | 116030 | 23142 | 179398 | 17167 | 48230 | 9606 | 13686 | 2766 | yes |
| ppt | 7 | 173611 | 24470 | 1920007 | 138588 | 39400 | 5375 | 22627 | 2963 | yes |
| pptx | 12 | 273458 | 20684 | 3645029 | 127760.0 | 443025 | 41727 | 38789 | 2946 | yes |
| rst | 6 | 147649 | 23859 | 397249 | 29476.5 | 60730 | 9601 | 16897 | 2745 | yes |
| rtf | 4 | 88288 | 23077 | 191974 | 15266.5 | 36063 | 11214 | 10817 | 2693 | yes |
| toml | 9 | 209108 | 23076 | 39681 | 4013 | 88055 | 9594 | 24351 | 2691 | yes |
| xls | 9 | 216317 | 23508 | 1278024 | 61291 | 70471 | 9644 | 26748 | 2779 | yes |
| xlsx | 14 | 284296 | 20035 | 1282259 | 30390.0 | 329210 | 27750 | 41197 | 2790 | yes |
| xml | 4 | 94020 | 23420 | 254734 | 41559.0 | 42757 | 10429 | 12210 | 2791 | yes |
| yaml | 11 | 254183 | 23047 | 195757 | 9179 | 109769 | 9631 | 29849 | 2716 | yes |

## By size class

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| large(>=256KiB) | 25 | 787445 | 29958 | 36836336 | 1146204 | 352982 | 13107 | 113160 | 4248 | yes |
| medium(<256KiB) | 86 | 2068861 | 23718 | 19480786 | 134945.5 | 1301041 | 10081 | 264313 | 2914 | yes |
| small(<16KiB) | 197 | 4563977 | 23039 | 3541701 | 11954 | 2239905 | 9599 | 550034 | 2727 | yes |

## By complexity

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| complex | 16 | 420304 | 24174 | 12213569 | 178642.5 | 258966 | 11436 | 56688 | 2990 | yes |
| hostile | 24 | 481759 | 19235 | 341455 | 6083.5 | 84186 | 484 | 66725 | 2754 | yes |
| moderate | 19 | 608249 | 23148 | 2368557 | 44661 | 368333 | 10065 | 54307 | 2771 | yes |
| nested | 114 | 2599641 | 23015 | 17206985 | 36496.0 | 1705656 | 9622 | 347416 | 2806 | yes |
| simple | 37 | 923092 | 23212 | 6961605 | 23143 | 450724 | 9677 | 111354 | 2755 | yes |
| structured | 98 | 2387238 | 23220 | 20766652 | 14515.5 | 1026063 | 9655 | 291017 | 2726 | yes |

## By stratum (real vs malformed/hostile)

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| hostile | 24 | 481759 | 19235 | 341455 | 6083.5 | 84186 | 484 | 66725 | 2754 | yes |
| real | 284 | 6938524 | 23156 | 59517368 | 30843.0 | 3809742 | 9672 | 860782 | 2767 | yes |

### Workload dimension

`narrow` = one `observe --metadata --kind metadata` per document (a bounded metadata projection).
`full` = one `materialize --exact --packed` per document (byte-authority closure).
Both are pooled and median; a size/time-weighted world is `pooled`, an unweighted per-document world is `median` (ADR-0054: a claim names its estimator).

**Detection.** 22/308 samples were detected as `opaque` by the bounded detector; 57 samples declined the narrow observation. The byte-authority gate is unaffected by detection; every sample is byte-exactly closed.

## Findings (detector choice != declared format)

A **finding** is a real sample whose detector choice differs from its declared format: a conservative `decline-opaque`, or a `misclassified` claim by another detector. Every finding is reconciled in the phase report (fixed-with-test, or recorded as a typed decline / recorded misclassification with its reason). The byte-authority closure is unaffected by any finding.

Summary by kind: decline-opaque=5, misclassified=5

| id | declared | detected | kind | resolution |
|---|---|---|---|---|
| cbor-data-integer.cbor | cbor | opaque | decline-opaque | recorded (unchanged reason) — two of the three recorded CBOR declines are genuine detector boundaries and stay recorded: `cbor-data-integer.cbor` is a lone scalar (`18 2a`, 2 bytes) below the deliberate structural bar; `cbor-rfc8949-good.cbor` is a genuinely well-formed single map whose deepest RFC 8949 vector nests to **512 levels**, above the deliberate `max_cbor_depth` cap of 256, so it is declined on the DoS bound (raising that cap to admit one sample is not warranted). The third, `cbor-data-tagged_date.cbor`, is fixed — see the `cbor`/`cbor` note below. The lone-scalar and depth declines are conservative by design (src/adapter/cbor.rs). Phase 30.5 re-attacked both: a lone 2-byte scalar is still below the structural bar (admitting it would widen the CBOR false-positive surface to any two bytes), and the 512-level sample stays above the deliberate DoS depth cap; both remain recorded. |
| cbor-rfc8949-good.cbor | cbor | opaque | decline-opaque | recorded (unchanged reason) — two of the three recorded CBOR declines are genuine detector boundaries and stay recorded: `cbor-data-integer.cbor` is a lone scalar (`18 2a`, 2 bytes) below the deliberate structural bar; `cbor-rfc8949-good.cbor` is a genuinely well-formed single map whose deepest RFC 8949 vector nests to **512 levels**, above the deliberate `max_cbor_depth` cap of 256, so it is declined on the DoS bound (raising that cap to admit one sample is not warranted). The third, `cbor-data-tagged_date.cbor`, is fixed — see the `cbor`/`cbor` note below. The lone-scalar and depth declines are conservative by design (src/adapter/cbor.rs). Phase 30.5 re-attacked both: a lone 2-byte scalar is still below the structural bar (admitting it would widen the CBOR false-positive surface to any two bytes), and the 512-level sample stays above the deliberate DoS depth cap; both remain recorded. |
| poi-document-Bug60942b.doc | doc | cfbf | misclassified | recorded decline — `poi-document-Bug60942b.doc` stays here: it is a **Word 6.0** FIB (`wIdent` 0xA5DC, `nFib` 0x0065), outside the supported Word 97-2003 `nFib` subset the adapter models, so it stays at the generic CFBF layer. The second recorded cause, `poi-document-testRangeDelete.doc` (a genuine Word FIB whose `WordDocument` mini-stream is **fragmented** and whose **root mini stream is itself fragmented**), is **fixed (28.1)**: a mini sector's absolute position is now resolved through the root mini stream's FAT chain (`CfbfModel::mini_chain`, added in model version 2), so `cfbf::stream_leading`'s first-chunk prefix read works for a fragmented mini stream; regression test `legacy_cfbf_detection::fragmented_mini_stream_in_fragmented_root_stream_is_detected` (src/adapter/cfbf.rs, doc.rs). Phase 30.5 re-attacked the Word 6.0 decline: admitting `nFib` 0x0065 would require a second FIB/piece-table dialect outside the Word 97–2003 subset, so it stays at the CFBF layer. |
| cpython-msg-25.eml | eml | opaque | decline-opaque | recorded decline — the sample is an **mbox** (a leading `From ` envelope line, not an RFC 5322 header) whose `multipart/report` boundary is folded and truncated (`;\n\tbo`), so the bounded header-block scan declines rather than admitting an unmodellable message container (src/adapter/eml.rs). |
| loghub-HPC-HPC_2k.log | logstream | opaque | decline-opaque | recorded decline + fixed (28.1) — `loghub-HPC-HPC_2k.log` stays recorded: it is a space-separated event record with **no timestamp and no level** (`134681 node-246 unix.hw state_change.unavailable …`), which is not a log-stream line grammar and is left to the fixed-width/tabular heuristics. Three other declines are **fixed**: the detector's generic grammar now also accepts a `YY/MM/DD HH:MM:SS` slash timestamp (Spark) and a bracketed `[MM.DD HH:MM:SS]` timestamp (Proxifier), and a new BSD-without-`<PRI>` dialect accepts `Mmm dd hh:mm:ss host tag…: msg` (macOS `loghub-Mac-Mac_2k.log`, whose lenient tag region can embed spaces and `([pid])`). Detection stays conservative (≥2 non-blank lines, **every** non-blank line must match). Regression tests `logstream::tests::detects_the_widened_dialects` and `logstream::tests::the_widened_grammar_still_declines_prose` (src/adapter/logstream.rs). Phase 30.5 re-attacked the HPC decline: the record has neither a timestamp nor a level, so no conservative dialect admits it; it stays recorded. |
| loghub-OpenStack-OpenStack_2k.log | logstream | config | misclassified | recorded misclassification (precedence) — `loghub-OpenStack-OpenStack_2k.log` is still claimed by the config detector (`KEY: value` shape) before the line-dialect log detector runs; reordering logstream before config risks the load-bearing config detector, so it is not reordered (src/adapter/logstream.rs, config.rs). Phase 30.5 re-attacked it: the OpenStack record carries a leading rotated-log filename token before the timestamp, outside the admitted log-stream grammar, so the logstream detector declines it regardless of order; it stays recorded. |
| loghub-Windows-Windows_2k.log | logstream | csv | misclassified | recorded misclassification (precedence) — `loghub-Windows-Windows_2k.log` is still claimed by the CSV detector (consistent field count) before the (later) logstream detector runs; reordering risks the load-bearing CSV detector, so it is not reordered (src/adapter/logstream.rs, csv.rs). Phase 30.5 re-attacked it: the Windows log separates the timestamp seconds from the level with a comma (`04:30:30, Info`), which the CSV detector reads as a consistent two-field row; reordering risks the load-bearing CSV detector, so it stays recorded. |
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

Phase 30.5 detector fixes landed here: (1) the five AsciiDoc attribute/locale/
CHANGELOG/stylesheet pages that reST claimed via its field-list signal are now
claimed as AsciiDoc, because each carries an **AsciiDoc-only** mark reST cannot
produce (a column-0 preprocessor/conditional directive `ifdef::`/`ifndef::`/
`endif::`/`include::`, or a `[#anchor]` block anchor); a new narrow predicate
`asciidoc::has_asciidoc_only_mark` admits them and `rst::looks_like_other` now
declines such a source, with no detector reorder and no change to a genuine reST
field list - regression test
`asciidoc::tests::detects_asciidoc_only_marks_and_declines_their_negations`.
(2) The legacy pre-0.15 **no-continuation** Arrow IPC stream framing (a bare 4-byte
metadata length, a first `Message` header type `Schema`) is now claimed,
conservatively gated by all-zero metadata-to-body padding and an end-of-stream
marker that is the exact end of the source - regression tests
`arrow_adapter::legacy_stream_framing_is_detected_and_built` and
`arrow_adapter::legacy_stream_near_misses_decline`. The legacy DOC story-to-section
mapping, textbox anchors, and piece-local `doc-find` remain recorded (typed
unclaimed surfaces), and the PPT list arena now names/selects non-conformant
`other:<recInstance>` lists (`recInstance >= 4`).

No real sample was skipped.

### Per-sample

| id | declared | detected | stratum | size class | complexity | src B | store B | build us | narrow us | full us | exact |
|---|---|---|---|---|---|---:|---:|---:|---:|---:|---|
| arrow-file-1.0.0-bigendian-generated_decimal256.arrow_file | arrow | arrow | real | large(>=256KiB) | nested | 363106 | 736384 | 27179 | 11013 | 4104 | True |
| arrow-file-1.0.0-littleendian-generated_union.arrow_file | arrow | arrow | real | small(<16KiB) | nested | 3562 | 11806 | 23010 | 12914 | 3842 | True |
| arrow-file-cpp-21.0.0-generated_dictionary_unsigned.arrow_file | arrow | arrow | real | small(<16KiB) | nested | 2170 | 8335 | 22962 | 9576 | 2702 | True |
| arrow-file-cpp-21.0.0-generated_null_trivial.arrow_file | arrow | arrow | real | small(<16KiB) | nested | 514 | 4774 | 23419 | 9623 | 2669 | True |
| arrow-stream-1.0.0-littleendian-generated_decimal256.stream | arrow | arrow | real | large(>=256KiB) | nested | 360464 | 732924 | 27294 | 11556 | 4082 | True |
| arrow-stream-1.0.0-littleendian-generated_nested_dictionary.stream | arrow | arrow | real | small(<16KiB) | nested | 2632 | 9850 | 22973 | 9698 | 2697 | True |
| arrow-stream-1.0.0-littleendian-generated_null_trivial.stream | arrow | arrow | real | small(<16KiB) | nested | 312 | 4582 | 22902 | 9565 | 2688 | True |
| arrow-stream-cpp-21.0.0-generated_custom_metadata.stream | arrow | arrow | real | small(<16KiB) | nested | 1504 | 7330 | 22898 | 9554 | 2711 | True |
| hostile2-magic-arrow | arrow | opaque | hostile | small(<16KiB) | hostile | 514 | 3726 | 20658 | 427 | 2670 | True |
| asciidoctor-CHANGELOG.adoc | asciidoc | asciidoc | real | medium(<256KiB) | structured | 140952 | 310850 | 33045 | 10559 | 3928 | True |
| asciidoctor-cli-nav.adoc | asciidoc | asciidoc | real | small(<16KiB) | structured | 222 | 4794 | 23405 | 9536 | 2778 | True |
| asciidoctor-locale-attributes-ja.adoc | asciidoc | asciidoc | real | small(<16KiB) | structured | 732 | 6412 | 23237 | 12046 | 3877 | True |
| asciidoctor-locale-attributes-sr.adoc | asciidoc | asciidoc | real | small(<16KiB) | structured | 863 | 7164 | 23213 | 9521 | 2633 | True |
| asciidoctor-pages-contexts-ref.adoc | asciidoc | asciidoc | real | small(<16KiB) | structured | 3640 | 15247 | 23554 | 9631 | 2676 | True |
| asciidoctor-pages-macos.adoc | asciidoc | asciidoc | real | small(<16KiB) | structured | 1577 | 8861 | 23139 | 9594 | 2731 | True |
| asciidoctor-pages-stylesheet-modes.adoc | asciidoc | asciidoc | real | small(<16KiB) | structured | 7284 | 23166 | 23427 | 9781 | 2716 | True |
| bibtex-build-strings-base.bib | bib | bib | real | medium(<256KiB) | structured | 145131 | 545657 | 26022 | 11125 | 3353 | True |
| bibtex-strings.bib | bib | bib | real | large(>=256KiB) | structured | 271806 | 1025695 | 28639 | 11765 | 3742 | True |
| hostile2-trunc-bib | bib | opaque | hostile | small(<16KiB) | hostile | 800 | 4012 | 18398 | 459 | 2724 | True |
| cbor-data-all_types.cbor | cbor | cbor | real | small(<16KiB) | nested | 354 | 6300 | 24202 | 9538 | 2694 | True |
| cbor-data-integer.cbor | cbor | opaque | real | small(<16KiB) | nested | 2 | 3214 | 18270 | 424 | 2624 | True |
| cbor-data-tagged_date.cbor | cbor | cbor | real | small(<16KiB) | nested | 22 | 3615 | 23109 | 9568 | 2678 | True |
| cbor-rfc8949-appendixA-mt2.cbor | cbor | cbor | real | small(<16KiB) | nested | 177 | 4742 | 22909 | 9412 | 2674 | True |
| cbor-rfc8949-appendixA-mt3.cbor | cbor | cbor | real | small(<16KiB) | nested | 487 | 6867 | 22878 | 9562 | 2681 | True |
| cbor-rfc8949-appendixA-streaming.cbor | cbor | cbor | real | small(<16KiB) | nested | 1145 | 13386 | 22844 | 9872 | 3783 | True |
| cbor-rfc8949-good.cbor | cbor | opaque | real | small(<16KiB) | nested | 13797 | 17009 | 19249 | 517 | 2763 | True |
| hostile2-trunc-cbor | cbor | opaque | hostile | small(<16KiB) | hostile | 40 | 3252 | 18467 | 401 | 2799 | True |
| poi-hpsf-TestVisioWithCodepage.vsd | cfbf | cfbf | real | medium(<256KiB) | nested | 45568 | 95591 | 23880 | 9654 | 2885 | True |
| poi-ole-poifs-only-zero-byte-streams.ole2 | cfbf | cfbf | real | small(<16KiB) | nested | 1536 | 7063 | 23021 | 9640 | 2717 | True |
| poi-publisher-LinkAt20.pub | cfbf | cfbf | real | medium(<256KiB) | nested | 64512 | 134375 | 23791 | 9885 | 2993 | True |
| poi-publisher-Sample_2010.pub | cfbf | cfbf | real | medium(<256KiB) | nested | 72704 | 150775 | 23885 | 12140 | 3120 | True |
| poi-publisher-clusterfuzz-testcase-minimized-POIHPBFFuzzer-4701121678278656.pub | cfbf | cfbf | real | large(>=256KiB) | nested | 331491 | 666405 | 27008 | 6330 | 3955 | True |
| cpython-libregrtest-mypy.ini | config | config | real | small(<16KiB) | simple | 997 | 7731 | 23108 | 9622 | 2689 | True |
| poi-gradle.properties | config | config | real | small(<16KiB) | simple | 555 | 5630 | 23056 | 9529 | 2795 | True |
| poi-resources-font_metrics.properties | config | config | real | medium(<256KiB) | simple | 146498 | 359532 | 26896 | 10732 | 3168 | True |
| cpython-namespace_pkgs-missing_directory.zip | container | container | real | small(<16KiB) | nested | 515 | 7457 | 19793 | 9504 | 2810 | True |
| cpython-test-recursion.tar | container | container | real | small(<16KiB) | nested | 516 | 4470 | 22835 | 6216 | 3742 | True |
| cpython-test-zipdir_backslash.zip | container | container | real | small(<16KiB) | nested | 192 | 6019 | 19795 | 9487 | 2752 | True |
| cpython-zipdata01-ziptestdata.zip | container | container | real | small(<16KiB) | nested | 876 | 9797 | 19747 | 9575 | 2676 | True |
| hostile2-trunc-container | container | opaque | hostile | small(<16KiB) | hostile | 5000 | 8212 | 18410 | 410 | 2728 | True |
| ne-zip-make-graticules-and-bounding-box-ne_1d5_graticules_beta2.zip | container | container | real | large(>=256KiB) | nested | 1000316 | 2055115 | 40692 | 13107 | 6067 | True |
| ne-zip-zip-ne_50m_graticules_1.zip | container | container | real | medium(<256KiB) | nested | 128416 | 264104 | 21168 | 9903 | 3200 | True |
| ne-zip-zip-ne_50m_graticules_15.zip | container | container | real | small(<16KiB) | nested | 10012 | 27300 | 19926 | 9561 | 2730 | True |
| ne-zip-zip-ne_50m_graticules_5.zip | container | container | real | medium(<256KiB) | nested | 27460 | 62192 | 21745 | 9528 | 2937 | True |
| poi-openxml4j-dcterms_bug_56479.zip | container | container | real | small(<16KiB) | nested | 1277 | 9934 | 19794 | 9445 | 2740 | True |
| parquet-delta-expect.csv | csv | csv | real | medium(<256KiB) | moderate | 159803 | 552658 | 25637 | 10652 | 3509 | True |
| vega-data-airports.csv | csv | csv | real | medium(<256KiB) | simple | 210363 | 893639 | 26851 | 11602 | 3494 | True |
| vega-data-co2-concentration.csv | csv | csv | real | medium(<256KiB) | simple | 18547 | 93286 | 23523 | 9677 | 2679 | True |
| vega-data-flights-airport.csv | csv | csv | real | medium(<256KiB) | simple | 65572 | 515711 | 24557 | 11176 | 3018 | True |
| vega-data-gapminder-health-income.csv | csv | csv | real | small(<16KiB) | simple | 8605 | 40460 | 23110 | 9600 | 2755 | True |
| vega-data-github.csv | csv | csv | real | medium(<256KiB) | simple | 21059 | 97252 | 23466 | 9858 | 2811 | True |
| vega-data-iowa-electricity.csv | csv | csv | real | small(<16KiB) | simple | 1531 | 10264 | 23055 | 9424 | 2759 | True |
| vega-data-population_engineers_hurricanes.csv | csv | csv | real | small(<16KiB) | simple | 1852 | 12779 | 23126 | 9453 | 2720 | True |
| vega-global-temp.csv | csv | csv | real | small(<16KiB) | simple | 1663 | 14668 | 23212 | 10195 | 2704 | True |
| vega-iowa-electricity.csv | csv | csv | real | small(<16KiB) | simple | 1531 | 10264 | 23180 | 9604 | 2731 | True |
| vega-population-engineers.csv | csv | csv | real | small(<16KiB) | simple | 1852 | 12779 | 22985 | 9476 | 2654 | True |
| vega-tsv-data-unemployment.tsv | csv | csv | real | medium(<256KiB) | simple | 34739 | 246812 | 24327 | 12425 | 3570 | True |
| hostile2-magic-doc | doc | opaque | hostile | medium(<256KiB) | hostile | 17920 | 21132 | 19417 | 470 | 2830 | True |
| poi-document-Bug41898.doc | doc | doc | real | medium(<256KiB) | nested | 24064 | 51548 | 23497 | 5096 | 2799 | True |
| poi-document-Bug46817.doc | doc | doc | real | medium(<256KiB) | nested | 17920 | 39260 | 23285 | 4994 | 2717 | True |
| poi-document-Bug53380_2.doc | doc | doc | real | medium(<256KiB) | nested | 31744 | 66908 | 23632 | 7474 | 3475 | True |
| poi-document-Bug60942b.doc | doc | cfbf | real | small(<16KiB) | nested | 6144 | 16749 | 23312 | 9585 | 2799 | True |
| poi-document-ThreeColHead.doc | doc | doc | real | medium(<256KiB) | nested | 27136 | 57692 | 23370 | 5023 | 2747 | True |
| poi-document-cap.stanford.edu_profiles_viewbiosketch_facultyid-4009-name-m_maciv | doc | doc | real | medium(<256KiB) | nested | 47104 | 97628 | 23514 | 5310 | 2810 | True |
| poi-document-parentinvguid.doc | doc | doc | real | large(>=256KiB) | nested | 398848 | 801116 | 28419 | 6839 | 4072 | True |
| poi-document-testRangeDelete.doc | doc | doc | real | medium(<256KiB) | nested | 104448 | 212316 | 24453 | 5454 | 3175 | True |
| hostile2-magic-docx | docx | opaque | hostile | small(<16KiB) | hostile | 10045 | 13257 | 18785 | 447 | 2686 | True |
| hostile2-trunc-docx | docx | docx | hostile | small(<16KiB) | hostile | 2297 | 14328 | 19877 | 27577 | 2726 | True |
| poi-document-58618.docx | docx | docx | real | small(<16KiB) | nested | 10045 | 52985 | 19836 | 36592 | 2852 | True |
| poi-document-60329.docx | docx | docx | real | medium(<256KiB) | nested | 31124 | 154025 | 20544 | 37002 | 2806 | True |
| poi-document-Bug51170.docx | docx | docx | real | medium(<256KiB) | nested | 239621 | 720218 | 22789 | 42630 | 3519 | True |
| poi-document-Bug60341.docx | docx | docx | real | small(<16KiB) | nested | 2297 | 14328 | 20004 | 28094 | 2850 | True |
| poi-document-NumberingWOverrides.docx | docx | docx | real | small(<16KiB) | nested | 15190 | 72123 | 19888 | 36512 | 2780 | True |
| poi-document-ThreeColHeadFoot.docx | docx | docx | real | small(<16KiB) | nested | 13123 | 66713 | 19965 | 37909 | 3994 | True |
| poi-document-capitalized.docx | docx | docx | real | small(<16KiB) | nested | 12104 | 73162 | 19888 | 36603 | 2939 | True |
| poi-document-checkboxes.docx | docx | docx | real | medium(<256KiB) | nested | 20800 | 108992 | 20433 | 41796 | 2863 | True |
| cpython-data-msg_08.txt | eml | eml | real | small(<16KiB) | structured | 454 | 5868 | 22894 | 9577 | 2714 | True |
| cpython-data-msg_22.txt | eml | eml | real | small(<16KiB) | structured | 1894 | 9580 | 23015 | 9641 | 2647 | True |
| cpython-data-msg_23.txt | eml | eml | real | small(<16KiB) | structured | 139 | 4246 | 23052 | 9430 | 2647 | True |
| cpython-data-msg_33.txt | eml | eml | real | small(<16KiB) | structured | 750 | 6266 | 22854 | 9699 | 2663 | True |
| cpython-data-msg_34.txt | eml | eml | real | small(<16KiB) | structured | 300 | 4725 | 22901 | 9475 | 2725 | True |
| cpython-data-msg_43.txt | eml | eml | real | small(<16KiB) | structured | 9166 | 24520 | 23187 | 9609 | 2727 | True |
| cpython-msg-01.eml | eml | eml | real | small(<16KiB) | simple | 459 | 5533 | 24802 | 9550 | 2654 | True |
| cpython-msg-25.eml | eml | opaque | real | small(<16KiB) | moderate | 5122 | 8334 | 202805 | 568 | 2762 | True |
| cpython-msg-43.eml | eml | eml | real | small(<16KiB) | complex | 9166 | 24520 | 23190 | 9584 | 2828 | True |
| hostile2-magic-epub | epub | opaque | hostile | medium(<256KiB) | hostile | 30552 | 33764 | 20108 | 508 | 2759 | True |
| tika-test-documents-testEPUB.epub | epub | epub | real | medium(<256KiB) | nested | 30552 | 72075 | 19971 | 9779 | 2842 | True |
| tika-test-documents-testEPUB_xml_ext.epub | epub | epub | real | small(<16KiB) | nested | 7823 | 29942 | 20140 | 9777 | 2643 | True |
| feed-1.1-extensive-noerror.xml | feed | feed | real | small(<16KiB) | structured | 1705 | 13105 | 23008 | 9590 | 2648 | True |
| feed-1.1-extensive-nowarn.xml | feed | feed | real | small(<16KiB) | structured | 1747 | 13189 | 23247 | 11622 | 3925 | True |
| feed-a33-3.3-duplicate-updated.xml | feed | feed | real | small(<16KiB) | structured | 1015 | 9234 | 22939 | 9487 | 2743 | True |
| feed-a33-3.3-published_no_year.xml | feed | feed | real | small(<16KiB) | structured | 875 | 7971 | 23015 | 9617 | 2722 | True |
| feed-a33-3.3-published_seconds.xml | feed | feed | real | small(<16KiB) | structured | 895 | 8011 | 23015 | 9459 | 2723 | True |
| feed-a33-3.3-updated-example2.xml | feed | feed | real | small(<16KiB) | structured | 706 | 7505 | 22989 | 9476 | 2695 | True |
| hostile2-trunc-geojson | geojson | opaque | hostile | small(<16KiB) | hostile | 2000 | 5212 | 18639 | 529 | 3292 | True |
| natural-earth-countries.geojson | geojson | geojson | real | large(>=256KiB) | complex | 838726 | 4054747 | 39380 | 18273 | 5788 | True |
| natural-earth-places.geojson | geojson | geojson | real | medium(<256KiB) | moderate | 166071 | 830240 | 26412 | 11623 | 3655 | True |
| ne-bbox.geojson | geojson | geojson | real | small(<16KiB) | simple | 4813 | 34888 | 23178 | 9617 | 2672 | True |
| ne-geojson-ne_10m_admin_0_antarctic_claim_limit_lines.geojson | geojson | geojson | real | medium(<256KiB) | structured | 58099 | 431186 | 24407 | 10643 | 3026 | True |
| ne-geojson-ne_10m_bathymetry_A_10000.geojson | geojson | geojson | real | small(<16KiB) | structured | 2860 | 18223 | 23157 | 9614 | 2680 | True |
| ne-geojson-ne_10m_parks_and_protected_lands_scale_rank.geojson | geojson | geojson | real | medium(<256KiB) | structured | 202844 | 1032397 | 27281 | 12149 | 4500 | True |
| ne-geojson-ne_110m_admin_0_boundary_lines_land.geojson | geojson | geojson | real | large(>=256KiB) | structured | 340010 | 1722622 | 29635 | 14191 | 3954 | True |
| ne-geojson-ne_110m_admin_1_states_provinces_lakes.geojson | geojson | geojson | real | medium(<256KiB) | structured | 181275 | 868587 | 26485 | 11795 | 3571 | True |
| ne-geojson-ne_110m_glaciated_areas.geojson | geojson | geojson | real | medium(<256KiB) | structured | 21720 | 116257 | 23699 | 10057 | 2639 | True |
| ne-geojson-ne_50m_geography_regions_elevation_points.geojson | geojson | geojson | real | medium(<256KiB) | structured | 104139 | 438795 | 25153 | 13767 | 3362 | True |
| ne-lakes.geojson | geojson | geojson | real | small(<16KiB) | moderate | 7719 | 43208 | 23148 | 9629 | 2771 | True |
| ne-regions.geojson | geojson | geojson | real | small(<16KiB) | simple | 4099 | 19576 | 23283 | 9610 | 2871 | True |
| hostile2-trunc-gis | gis | opaque | hostile | small(<16KiB) | hostile | 200 | 3412 | 18380 | 472 | 2695 | True |
| togeojson-data-bad.gpx | gis | gis | real | small(<16KiB) | structured | 833 | 8679 | 23017 | 9615 | 2694 | True |
| togeojson-data-blue_hills.gpx | gis | gis | real | large(>=256KiB) | structured | 268694 | 2069245 | 35796 | 14400 | 3796 | True |
| togeojson-data-null_geometry.kml | gis | gis | real | small(<16KiB) | structured | 286 | 4889 | 22862 | 9747 | 2640 | True |
| togeojson-data-opacity_override.kml | gis | gis | real | small(<16KiB) | structured | 625 | 7617 | 23077 | 9602 | 2697 | True |
| togeojson-data-opencpn.gpx | gis | gis | real | small(<16KiB) | structured | 10637 | 66507 | 23867 | 9769 | 2744 | True |
| togeojson-data-point_extra_coordinates.kml | gis | gis | real | small(<16KiB) | structured | 386 | 5465 | 23068 | 9425 | 2684 | True |
| togeojson-data-schema.kml | gis | gis | real | small(<16KiB) | structured | 2158 | 16426 | 23190 | 9606 | 2660 | True |
| togeojson-data-style.kml | gis | gis | real | small(<16KiB) | structured | 1372 | 13453 | 23314 | 12748 | 2626 | True |
| togeojson-data-styles.kml | gis | gis | real | small(<16KiB) | structured | 6374 | 44998 | 23756 | 9508 | 2640 | True |
| hostile2-trunc-hdf | hdf | opaque | hostile | small(<16KiB) | hostile | 600 | 3812 | 18435 | 486 | 2699 | True |
| netcdf-h5-h5_test-ref_tst_h_compounds2.h5 | hdf | hdf | real | small(<16KiB) | nested | 557 | 4534 | 23154 | 4974 | 2820 | True |
| netcdf-h5-nczarr_test-ref_groups.h5 | hdf | hdf | real | small(<16KiB) | nested | 9836 | 23282 | 23039 | 9327 | 2767 | True |
| netcdf-ncdump-ref_nc_test_netcdf4_4_0.nc | hdf | hdf | real | medium(<256KiB) | nested | 162812 | 329044 | 24955 | 5586 | 3348 | True |
| netcdf-ncdump-ref_no_ncproperty.nc | hdf | hdf | real | small(<16KiB) | nested | 1262 | 5944 | 22973 | 5071 | 2784 | True |
| netcdf-ncdump-ref_test_365_day_1900.nc | hdf | hdf | real | small(<16KiB) | nested | 9348 | 22248 | 23073 | 9620 | 2709 | True |
| netcdf-ncdump-ref_tst_compounds2.nc | hdf | hdf | real | small(<16KiB) | nested | 5753 | 14926 | 23270 | 5026 | 2754 | True |
| netcdf-ncdump-ref_tst_compounds4.nc | hdf | hdf | real | small(<16KiB) | nested | 526 | 4472 | 23020 | 4971 | 2716 | True |
| netcdf-nctestfiles-test_vlen2.nc | hdf | hdf | real | small(<16KiB) | nested | 6336 | 16092 | 23278 | 5058 | 2761 | True |
| feedval-05-version_12_supports_atom_02.html | html | html | real | medium(<256KiB) | structured | 47054 | 110374 | 28003 | 9924 | 2903 | True |
| feedval-21-live.html | html | html | real | small(<16KiB) | structured | 2277 | 19579 | 23078 | 9603 | 2691 | True |
| feedval-22-known_bugs.html | html | html | real | small(<16KiB) | structured | 3478 | 24861 | 23082 | 9626 | 2615 | True |
| feedval-29-new_mailing_list_for_validator_users.html | html | html | real | small(<16KiB) | structured | 2823 | 21310 | 23105 | 9657 | 2654 | True |
| mdn-doc-structure.html | html | html | real | small(<16KiB) | moderate | 3525 | 20244 | 23549 | 9569 | 2635 | True |
| mdn-fonts-demo.html | html | html | real | medium(<256KiB) | complex | 36867 | 189800 | 25451 | 14029 | 2802 | True |
| mdn-getting-started.html | html | html | real | small(<16KiB) | simple | 224 | 5429 | 23003 | 9464 | 2718 | True |
| mdn-planets.html | html | html | real | small(<16KiB) | moderate | 4302 | 39076 | 23327 | 9867 | 2764 | True |
| cpython-fuzz_json_loads_corpus-empty_array.json | json | json | real | small(<16KiB) | structured | 3 | 3509 | 22742 | 9315 | 2633 | True |
| cpython-fuzz_json_loads_corpus-pass1.json | json | json | real | small(<16KiB) | structured | 1441 | 8910 | 23797 | 9472 | 2886 | True |
| cpython-fuzz_json_loads_corpus-simple_array.json | json | json | real | small(<16KiB) | structured | 25 | 3678 | 23034 | 9435 | 2635 | True |
| hostile2-magic-json | json | opaque | hostile | small(<16KiB) | hostile | 2743 | 5955 | 18643 | 464 | 2801 | True |
| vega-data-barley.json | json | json | real | small(<16KiB) | structured | 8487 | 47477 | 23363 | 10481 | 4092 | True |
| vega-data-budget.json | json | json | real | large(>=256KiB) | structured | 391353 | 1645334 | 33076 | 13625 | 4244 | True |
| vega-data-burtin.json | json | json | real | small(<16KiB) | structured | 2743 | 14189 | 23199 | 9687 | 2704 | True |
| vega-data-penguins.json | json | json | real | medium(<256KiB) | structured | 67119 | 266741 | 25101 | 10222 | 2892 | True |
| vega-data-volcano.json | json | json | real | medium(<256KiB) | structured | 21167 | 178662 | 23870 | 9762 | 2606 | True |
| vega-data-weekly-weather.json | json | json | real | small(<16KiB) | structured | 1281 | 12815 | 24574 | 9618 | 2774 | True |
| vega-data-world-110m.json | json | json | real | medium(<256KiB) | structured | 119410 | 1076373 | 30176 | 12163 | 3034 | True |
| mdx-js-renovate.json5 | json5 | json5 | real | small(<16KiB) | structured | 92 | 3852 | 23092 | 12303 | 2675 | True |
| jsonlines-datagov100.json | jsonl | jsonl | real | large(>=256KiB) | complex | 696007 | 2253927 | 34477 | 15827 | 5401 | True |
| jsonlines-datagov100.json | jsonl | jsonl | real | large(>=256KiB) | structured | 696007 | 2253927 | 34472 | 15483 | 6563 | True |
| oai-data-dbpedia_samples.jsonl | jsonl | jsonl | real | medium(<256KiB) | structured | 64512 | 167137 | 23780 | 9852 | 2991 | True |
| oai-data-drone_training.jsonl | jsonl | jsonl | real | large(>=256KiB) | structured | 387706 | 1837044 | 29958 | 14784 | 4158 | True |
| oai-data-example_requests_to_parallel_process.jsonl | jsonl | jsonl | real | large(>=256KiB) | structured | 548917 | 2831447 | 32047 | 17050 | 5141 | True |
| oai-data-toy_chat_fine_tuning.jsonl | jsonl | jsonl | real | medium(<256KiB) | structured | 27385 | 61273 | 23737 | 9829 | 2892 | True |
| oai-dbpedia.jsonl | jsonl | jsonl | real | medium(<256KiB) | moderate | 64512 | 167137 | 25411 | 10065 | 2876 | True |
| oai-parallel-requests.jsonl | jsonl | jsonl | real | large(>=256KiB) | complex | 548917 | 2831447 | 31536 | 16879 | 4629 | True |
| oai-toy-chat.jsonl | jsonl | jsonl | real | medium(<256KiB) | moderate | 27385 | 61273 | 23608 | 9671 | 2716 | True |
| loghub-HPC-HPC_2k.log | logstream | opaque | real | medium(<256KiB) | simple | 151178 | 154390 | 30913 | 850 | 4203 | True |
| loghub-Mac-Mac_2k.log | logstream | logstream | real | large(>=256KiB) | simple | 319414 | 846329 | 40401 | 11518 | 3797 | True |
| loghub-OpenStack-OpenStack_2k.log | logstream | config | real | large(>=256KiB) | simple | 595119 | 1329738 | 36284 | 13490 | 4956 | True |
| loghub-Proxifier-Proxifier_2k.log | logstream | logstream | real | medium(<256KiB) | simple | 236962 | 613425 | 36652 | 11033 | 3786 | True |
| loghub-Spark-Spark_2k.log | logstream | logstream | real | medium(<256KiB) | simple | 196268 | 566037 | 35682 | 11037 | 3367 | True |
| loghub-Windows-Windows_2k.log | logstream | csv | real | large(>=256KiB) | simple | 285433 | 701911 | 27303 | 11150 | 3895 | True |
| commonmark-readme.md | markdown | markdown | real | small(<16KiB) | simple | 7671 | 23143 | 23543 | 9599 | 2634 | True |
| commonmark-spec.txt | markdown | markdown | real | medium(<256KiB) | complex | 205025 | 530161 | 33955 | 11437 | 3478 | True |
| cpython-ISSUE_TEMPLATE-crash.md | markdown | markdown | real | small(<16KiB) | simple | 1142 | 6380 | 23003 | 9517 | 2681 | True |
| cpython-rhel7-README.md | markdown | markdown | real | small(<16KiB) | simple | 608 | 5323 | 23343 | 9537 | 2682 | True |
| cpython-wasm-README.md | markdown | markdown | real | medium(<256KiB) | simple | 16560 | 51340 | 24546 | 9738 | 2760 | True |
| rust-readme.md | markdown | markdown | real | small(<16KiB) | simple | 3137 | 14075 | 23175 | 9542 | 2717 | True |
| mdx-blog-conf.mdx | mdx | mdx | real | small(<16KiB) | structured | 2790 | 12397 | 23306 | 9638 | 2729 | True |
| mdx-blog-v1.mdx | mdx | mdx | real | small(<16KiB) | structured | 9150 | 33026 | 23686 | 9717 | 2758 | True |
| mdx-community-sponsor.mdx | mdx | mdx | real | small(<16KiB) | structured | 5513 | 15451 | 23138 | 9608 | 2712 | True |
| mdx-docs-404.mdx | mdx | mdx | real | small(<16KiB) | structured | 545 | 5246 | 23107 | 9672 | 2735 | True |
| mdx-docs-getting-started.mdx | mdx | mdx | real | medium(<256KiB) | structured | 26777 | 87116 | 25893 | 9745 | 2646 | True |
| mdx-guides-embed.mdx | mdx | mdx | real | small(<16KiB) | structured | 3652 | 13099 | 23228 | 9719 | 2796 | True |
| mdx-packages-index.mdx | mdx | mdx | real | small(<16KiB) | structured | 995 | 6389 | 23023 | 9656 | 2795 | True |
| hostile2-trunc-mhtml | mhtml | mhtml | hostile | small(<16KiB) | hostile | 1000 | 7936 | 23148 | 9562 | 2764 | True |
| tika-test-documents-testMHTMLFirefox.mhtml | mhtml | mhtml | real | small(<16KiB) | nested | 13759 | 30944 | 23304 | 5141 | 2794 | True |
| tika-test-documents-testUserDefinedCharset.mhtml | mhtml | mhtml | real | small(<16KiB) | nested | 621 | 6217 | 23151 | 9587 | 2708 | True |
| hostile2-trunc-msg | msg | opaque | hostile | small(<16KiB) | hostile | 1500 | 4712 | 19053 | 482 | 2756 | True |
| poi-hsmf-ASCII_CP1251_LCID1049.msg | msg | msg | real | small(<16KiB) | nested | 3584 | 11471 | 23219 | 9618 | 2722 | True |
| poi-hsmf-attachment_msg_inlineImg.msg | msg | msg | real | medium(<256KiB) | nested | 147456 | 298332 | 24935 | 5625 | 3351 | True |
| poi-hsmf-example_sent_regular.msg | msg | msg | real | medium(<256KiB) | nested | 53248 | 109916 | 24108 | 5352 | 2934 | True |
| poi-hsmf-logsat.com_signatures_valid.msg | msg | msg | real | medium(<256KiB) | nested | 23040 | 49500 | 23350 | 5093 | 2792 | True |
| poi-hsmf-message_1981.msg | msg | msg | real | medium(<256KiB) | nested | 29696 | 62812 | 23435 | 5110 | 2680 | True |
| poi-hsmf-simple_test_msg.msg | msg | msg | real | medium(<256KiB) | nested | 16896 | 37212 | 25384 | 5071 | 2711 | True |
| hostile2-trunc-notebook | notebook | notebook | hostile | small(<16KiB) | hostile | 355 | 5075 | 23023 | 9469 | 2777 | True |
| jupyter-Notebook-Custom-Keyboard-Shortcuts.ipynb | notebook | notebook | real | small(<16KiB) | structured | 1148 | 7493 | 23149 | 9607 | 2671 | True |
| jupyter-Notebook-Importing-Notebooks.ipynb | notebook | notebook | real | small(<16KiB) | structured | 13552 | 47792 | 23538 | 9830 | 2750 | True |
| jupyter-Notebook-Running-Code.ipynb | notebook | notebook | real | medium(<256KiB) | structured | 52012 | 133052 | 23969 | 13247 | 3805 | True |
| jupyter-Notebook-What-is-the-Jupyter-Notebook.ipynb | notebook | notebook | real | small(<16KiB) | structured | 7541 | 24181 | 24490 | 9563 | 2684 | True |
| jupyter-nbs-other.ipynb | notebook | notebook | real | small(<16KiB) | structured | 786 | 6744 | 23019 | 10895 | 2735 | True |
| jupyter-notebooks-simple.ipynb | notebook | notebook | real | small(<16KiB) | structured | 355 | 5075 | 23120 | 9547 | 2687 | True |
| jupyter-notebooks-simple_toc.ipynb | notebook | notebook | real | small(<16KiB) | structured | 1040 | 7873 | 22969 | 9399 | 2767 | True |
| odfpy-cols.odp | odp | odp | real | medium(<256KiB) | moderate | 18037 | 148305 | 19945 | 36794 | 2834 | True |
| odfpy-emb.odp | odp | odp | real | medium(<256KiB) | complex | 16403 | 106233 | 20262 | 40280 | 2871 | True |
| odfpy-examples-cols.odp | odp | odp | real | medium(<256KiB) | nested | 18037 | 148305 | 19905 | 37435 | 2945 | True |
| odfpy-examples-ol.odp | odp | odp | real | small(<16KiB) | nested | 14940 | 138597 | 20177 | 37115 | 2849 | True |
| odfpy-ol.odp | odp | odp | real | small(<16KiB) | simple | 14940 | 138597 | 20036 | 37178 | 2758 | True |
| odfpy-chinese.ods | ods | ods | real | small(<16KiB) | moderate | 11765 | 44661 | 20228 | 27763 | 2784 | True |
| odfpy-empty.ods | ods | ods | real | small(<16KiB) | simple | 6819 | 29699 | 20044 | 23283 | 2729 | True |
| odfpy-examples-chinese_spreadsheet.ods | ods | ods | real | small(<16KiB) | nested | 11765 | 44661 | 20353 | 23415 | 2809 | True |
| odfpy-examples-empty.ods | ods | ods | real | small(<16KiB) | nested | 6819 | 29699 | 19890 | 23210 | 2727 | True |
| odfpy-examples-pythagoras-kspread.ods | ods | ods | real | small(<16KiB) | nested | 5037 | 22024 | 19735 | 23085 | 2767 | True |
| odfpy-examples-pythagoras.ods | ods | ods | real | small(<16KiB) | nested | 6837 | 30406 | 20221 | 22912 | 2711 | True |
| odfpy-pyth-kspread.ods | ods | ods | real | small(<16KiB) | simple | 5037 | 22024 | 19937 | 23121 | 3441 | True |
| odfpy-pythagoras.ods | ods | ods | real | small(<16KiB) | simple | 6837 | 30406 | 19999 | 23063 | 2684 | True |
| hostile2-trunc-odt | odt | opaque | hostile | small(<16KiB) | hostile | 3000 | 6212 | 20298 | 463 | 2719 | True |
| odfpy-examples-definitionlists.odt | odt | odt | real | small(<16KiB) | nested | 7824 | 32070 | 19762 | 23182 | 2800 | True |
| odfpy-examples-nasty.odt | odt | odt | real | small(<16KiB) | nested | 1539 | 11051 | 19798 | 18790 | 2804 | True |
| odfpy-examples-no_userfields.odt | odt | odt | real | medium(<256KiB) | nested | 19228 | 52978 | 20020 | 23339 | 2840 | True |
| odfpy-examples-parastyles.odt | odt | odt | real | small(<16KiB) | nested | 7194 | 32845 | 21325 | 23415 | 2758 | True |
| odfpy-examples-serious_poem.odt | odt | odt | real | small(<16KiB) | nested | 8539 | 31745 | 20113 | 26739 | 2789 | True |
| odfpy-examples-simpletable.odt | odt | odt | real | small(<16KiB) | nested | 7055 | 31250 | 19895 | 23361 | 2842 | True |
| odfpy-examples-userfields_ooo3.odt | odt | odt | real | medium(<256KiB) | nested | 38420 | 98623 | 20034 | 26233 | 4061 | True |
| hostile2-trunc-parquet | parquet | opaque | hostile | small(<16KiB) | hostile | 800 | 4012 | 18455 | 377 | 2652 | True |
| parquet-alltypes-plain.parquet | parquet | parquet | real | small(<16KiB) | simple | 1851 | 9203 | 23068 | 9683 | 2706 | True |
| parquet-alltypes-tiny.parquet | parquet | parquet | real | large(>=256KiB) | complex | 454233 | 914573 | 28346 | 11434 | 4268 | True |
| parquet-data-datapage_v2.snappy.parquet | parquet | parquet | real | small(<16KiB) | nested | 1165 | 6792 | 25029 | 9585 | 2640 | True |
| parquet-data-fixed_length_byte_array.parquet | parquet | parquet | real | small(<16KiB) | nested | 4437 | 12708 | 23024 | 10559 | 3794 | True |
| parquet-data-fixed_length_decimal.parquet | parquet | parquet | real | small(<16KiB) | nested | 677 | 5166 | 23023 | 9900 | 2697 | True |
| parquet-data-float16_nonzeros_and_nans.parquet | parquet | parquet | real | small(<16KiB) | nested | 501 | 4788 | 22789 | 9554 | 2815 | True |
| parquet-data-lz4_raw_compressed_larger.parquet | parquet | parquet | real | large(>=256KiB) | nested | 380836 | 765507 | 27543 | 11159 | 4248 | True |
| parquet-data-nation.dict-malformed.parquet | parquet | parquet | real | small(<16KiB) | nested | 2850 | 9855 | 22797 | 9479 | 2736 | True |
| parquet-delta-binary.parquet | parquet | parquet | real | medium(<256KiB) | complex | 72971 | 161889 | 24018 | 10033 | 3110 | True |
| parquet-geospatial-geography-points.parquet | parquet | parquet | real | medium(<256KiB) | nested | 33026 | 81965 | 23414 | 9693 | 2820 | True |
| asciidoctor-fixtures-hello-asciidoctor.pdf | pdf | pdf | real | small(<16KiB) | nested | 847 | 6557 | 23138 | 437 | 2744 | True |
| cpython-library-turtle-star.pdf | pdf | pdf | real | small(<16KiB) | nested | 4418 | 12438 | 23004 | 449 | 2669 | True |
| netcdf-docs-docmap.pdf | pdf | pdf | real | medium(<256KiB) | nested | 64347 | 78695 | 24560 | 563 | 3927 | True |
| cargo-cargo.toml | pkgmeta | pkgmeta | real | small(<16KiB) | complex | 7717 | 78906 | 23372 | 9827 | 2766 | True |
| jupyter-package.json | pkgmeta | pkgmeta | real | small(<16KiB) | structured | 3448 | 17137 | 23115 | 9424 | 2773 | True |
| jupyter-py-pyproject.toml | pkgmeta | pkgmeta | real | small(<16KiB) | structured | 7903 | 50514 | 23376 | 9775 | 2688 | True |
| serde-cargo.toml | pkgmeta | pkgmeta | real | small(<16KiB) | moderate | 2431 | 17167 | 23025 | 9598 | 2799 | True |
| vega-package.json | pkgmeta | pkgmeta | real | small(<16KiB) | structured | 2497 | 15674 | 23142 | 9606 | 2660 | True |
| hostile2-trunc-ppt | ppt | cfbf | hostile | small(<16KiB) | hostile | 8000 | 19423 | 23158 | 5133 | 2695 | True |
| poi-slideshow-42474-2.ppt | ppt | ppt | real | large(>=256KiB) | nested | 571392 | 1146204 | 29973 | 7176 | 4718 | True |
| poi-slideshow-54332a.ppt | ppt | ppt | real | medium(<256KiB) | nested | 67584 | 138588 | 24526 | 5673 | 2963 | True |
| poi-slideshow-Single_Coloured_Page.ppt | ppt | ppt | real | small(<16KiB) | nested | 9728 | 22876 | 23096 | 5152 | 2716 | True |
| poi-slideshow-basic_test_ppt_file.ppt | ppt | ppt | real | small(<16KiB) | nested | 15360 | 34140 | 23380 | 5168 | 2859 | True |
| poi-slideshow-bug62092.ppt | ppt | ppt | real | medium(<256KiB) | nested | 161792 | 327004 | 25008 | 5375 | 3311 | True |
| poi-slideshow-datetime.ppt | ppt | ppt | real | medium(<256KiB) | nested | 114176 | 231772 | 24470 | 5723 | 3365 | True |
| hostile2-trunc-pptx | pptx | opaque | hostile | medium(<256KiB) | hostile | 120000 | 123212 | 20082 | 724 | 3200 | True |
| poi-61515.pptx | pptx | pptx | real | medium(<256KiB) | moderate | 29270 | 99853 | 20590 | 41197 | 2828 | True |
| poi-Divino.pptx | pptx | opaque | real | large(>=256KiB) | complex | 523999 | 527211 | 26278 | 1462 | 4501 | True |
| poi-SampleShow.pptx | pptx | pptx | real | medium(<256KiB) | moderate | 39083 | 132308 | 20541 | 46084 | 2924 | True |
| poi-bug60993.pptx | pptx | pptx | real | medium(<256KiB) | complex | 21942 | 95612 | 20434 | 45830 | 2802 | True |
| poi-slideshow-49386-null_dates.pptx | pptx | pptx | real | medium(<256KiB) | nested | 17124 | 62331 | 20408 | 41326 | 2841 | True |
| poi-slideshow-au.asn.aes.www_conferences_2011_presentations_Fri_20Room4Level4_20 | pptx | pptx | real | large(>=256KiB) | nested | 536086 | 1622797 | 39934 | 49164 | 4796 | True |
| poi-slideshow-bug58144-headers-footers-2007.pptx | pptx | pptx | real | medium(<256KiB) | nested | 42035 | 136662 | 20696 | 41275 | 3025 | True |
| poi-slideshow-bug62736.pptx | pptx | pptx | real | medium(<256KiB) | nested | 50926 | 164564 | 20998 | 41831 | 2968 | True |
| poi-slideshow-bug65551.pptx | pptx | pptx | real | medium(<256KiB) | nested | 34383 | 110096 | 20759 | 41623 | 2876 | True |
| poi-slideshow-pptx2svg.pptx | pptx | pptx | real | medium(<256KiB) | nested | 149172 | 467022 | 22065 | 46407 | 3352 | True |
| poi-table_test.pptx | pptx | pptx | real | medium(<256KiB) | moderate | 28935 | 103361 | 20673 | 46102 | 2676 | True |
| cpython-faq-gui.rst | rst | rst | real | small(<16KiB) | structured | 3058 | 11685 | 23259 | 9548 | 2675 | True |
| cpython-library-asyncio-protocol.rst | rst | rst | real | medium(<256KiB) | structured | 32975 | 83009 | 27004 | 12465 | 2872 | True |
| cpython-library-mmap.rst | rst | rst | real | small(<16KiB) | structured | 14634 | 35272 | 24072 | 9654 | 2665 | True |
| cpython-library-python.rst | rst | rst | real | small(<16KiB) | structured | 502 | 4725 | 23237 | 9514 | 2763 | True |
| cpython-library-sunau.rst | rst | rst | real | small(<16KiB) | structured | 7423 | 23681 | 23646 | 9452 | 2727 | True |
| cpython-whatsnew-3.3.rst | rst | markdown | real | medium(<256KiB) | structured | 99150 | 238877 | 26431 | 10097 | 3195 | True |
| hostile2-trunc-rtf | rtf | opaque | hostile | small(<16KiB) | hostile | 1000 | 4212 | 18493 | 454 | 2769 | True |
| ne-10m_cultural-10m_us_parks_-READ_ME.rtf | rtf | rtf | real | small(<16KiB) | structured | 2058 | 11954 | 23112 | 9523 | 2696 | True |
| ne-10m_parks_and_protected_areas-10m_us_parks_READ_ME.rtf | rtf | rtf | real | small(<16KiB) | structured | 3718 | 18579 | 23042 | 12904 | 2662 | True |
| poi-quick-contents-message.rtf | rtf | rtf | real | medium(<256KiB) | structured | 25528 | 157229 | 23641 | 13182 | 2690 | True |
| hostile2-magic-toml | toml | toml | hostile | small(<16KiB) | hostile | 37 | 4013 | 23124 | 9527 | 2657 | True |
| toml-array-string-quote-comma-02.toml | toml | toml | real | small(<16KiB) | structured | 20 | 3692 | 23111 | 9767 | 2637 | True |
| toml-datetime-local.toml | toml | toml | real | small(<16KiB) | structured | 88 | 3951 | 22924 | 9555 | 2627 | True |
| toml-key-escapes.toml | toml | toml | real | small(<16KiB) | structured | 163 | 4593 | 23049 | 9598 | 2822 | True |
| toml-spec-1.0.0-float-2.toml | toml | toml | real | small(<16KiB) | structured | 276 | 4701 | 23047 | 9594 | 2691 | True |
| toml-spec-1.1.0-common-0.toml | toml | toml | real | small(<16KiB) | structured | 122 | 3969 | 23108 | 11214 | 2744 | True |
| toml-string-basic-escape-03.toml | toml | toml | real | small(<16KiB) | structured | 56 | 3739 | 23076 | 9591 | 2822 | True |
| toml-valid-implicit-and-explicit-before.toml | toml | toml | real | small(<16KiB) | structured | 37 | 4013 | 23066 | 9658 | 2633 | True |
| toml-valid-multibyte.toml | toml | toml | real | small(<16KiB) | structured | 1156 | 7010 | 24603 | 9551 | 2718 | True |
| hostile2-trunc-xls | xls | cfbf | hostile | small(<16KiB) | hostile | 1500 | 6423 | 23187 | 5161 | 2779 | True |
| poi-spreadsheet-43493.xls | xls | xls | real | medium(<256KiB) | nested | 66048 | 135516 | 23669 | 5432 | 2849 | True |
| poi-spreadsheet-47251_1.xls | xls | xls | real | medium(<256KiB) | nested | 26112 | 102820 | 23508 | 9738 | 2731 | True |
| poi-spreadsheet-48325.xls | xls | xls | real | small(<16KiB) | nested | 4096 | 15039 | 24244 | 9644 | 2773 | True |
| poi-spreadsheet-49524.xls | xls | xls | real | small(<16KiB) | nested | 13824 | 31068 | 23416 | 5083 | 2687 | True |
| poi-spreadsheet-50298.xls | xls | xls | real | medium(<256KiB) | nested | 20480 | 61291 | 23256 | 9694 | 2849 | True |
| poi-spreadsheet-DStar.xls | xls | xls | real | medium(<256KiB) | nested | 35328 | 104549 | 23527 | 9764 | 2897 | True |
| poi-spreadsheet-EmbeddedChartHeaderTest.xls | xls | xls | real | small(<16KiB) | nested | 15360 | 43754 | 23141 | 9648 | 2764 | True |
| poi-spreadsheet-bug66319.xls | xls | xls | real | large(>=256KiB) | nested | 387072 | 777564 | 28369 | 6307 | 4419 | True |
| hostile2-trunc-xlsx | xlsx | opaque | hostile | small(<16KiB) | hostile | 15000 | 18212 | 18561 | 492 | 2797 | True |
| poi-47889.xlsx | xlsx | xlsx | real | small(<16KiB) | simple | 3555 | 19151 | 20168 | 27501 | 2793 | True |
| poi-49609.xlsx | xlsx | xlsx | real | medium(<256KiB) | complex | 105424 | 218879 | 22045 | 5547 | 3246 | True |
| poi-59021.xlsx | xlsx | xlsx | real | small(<16KiB) | moderate | 1933 | 12665 | 19717 | 27900 | 2791 | True |
| poi-DataValidation.xlsx | xlsx | xlsx | real | small(<16KiB) | moderate | 3286 | 18607 | 20007 | 32225 | 2752 | True |
| poi-linkext.xlsx | xlsx | xlsx | real | small(<16KiB) | complex | 3274 | 18583 | 19915 | 27767 | 2702 | True |
| poi-spreadsheet-123233_charts.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 12957 | 47188 | 20275 | 27703 | 2761 | True |
| poi-spreadsheet-56511.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 9688 | 35780 | 20063 | 30423 | 3814 | True |
| poi-spreadsheet-58731.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 9023 | 33113 | 19868 | 27569 | 2789 | True |
| poi-spreadsheet-59021.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 1933 | 12665 | 19680 | 27732 | 2720 | True |
| poi-spreadsheet-StructuredRefs-lots-with-lookups.xlsx | xlsx | xlsx | real | large(>=256KiB) | nested | 325446 | 687123 | 23406 | 29000 | 3866 | True |
| poi-spreadsheet-TwoSheetsOneHidden.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 7962 | 30038 | 19908 | 32299 | 2723 | True |
| poi-spreadsheet-WithChartSheet.xlsx | xlsx | xlsx | real | medium(<256KiB) | nested | 30333 | 99513 | 20592 | 27942 | 2637 | True |
| poi-spreadsheet-duplicate-filename-case-insensitive.xlsx | xlsx | xlsx | real | small(<16KiB) | nested | 10721 | 30742 | 20091 | 5110 | 2806 | True |
| maven-core-pom.xml | xml | xml | real | small(<16KiB) | moderate | 7411 | 46651 | 23277 | 9816 | 2754 | True |
| maven-pom.xml | xml | xml | real | medium(<256KiB) | complex | 28489 | 167485 | 24329 | 11042 | 2724 | True |
| poi-quick-contents-quick.xml | xml | xml | real | small(<16KiB) | structured | 143 | 4131 | 22852 | 9615 | 2828 | True |
| poi-spreadsheet-SampleSS.xml | xml | xml | real | small(<16KiB) | structured | 5115 | 36467 | 23562 | 12284 | 3904 | True |
| alertmanager-ci.yml | yaml | yaml | real | small(<16KiB) | simple | 1348 | 9179 | 23270 | 9673 | 2675 | True |
| alertmanager-simple.yml | yaml | yaml | real | small(<16KiB) | moderate | 3953 | 16344 | 23302 | 9635 | 2716 | True |
| cpython-workflows-build.yml | yaml | yaml | real | medium(<256KiB) | structured | 19106 | 65679 | 23294 | 13671 | 2751 | True |
| cpython-workflows-project-updater.yml | yaml | config | real | small(<16KiB) | structured | 741 | 7022 | 22969 | 9537 | 2703 | True |
| cpython-workflows-require-pr-label.yml | yaml | yaml | real | small(<16KiB) | structured | 507 | 5822 | 22954 | 9523 | 2664 | True |
| cpython-workflows-reusable-ubuntu.yml | yaml | yaml | real | small(<16KiB) | structured | 3356 | 14842 | 23136 | 9631 | 2677 | True |
| cpython-workflows-reusable-windows.yml | yaml | yaml | real | small(<16KiB) | structured | 1157 | 7847 | 22928 | 9520 | 2751 | True |
| hostile2-trunc-yaml | yaml | yaml | hostile | small(<16KiB) | hostile | 4000 | 17939 | 22960 | 9692 | 2751 | True |
| prometheus-ci.yml | yaml | yaml | real | small(<16KiB) | complex | 10201 | 39596 | 23316 | 9715 | 2772 | True |
| prometheus-web-config.yml | yaml | yaml | real | small(<16KiB) | simple | 531 | 5022 | 23007 | 9597 | 2628 | True |
| prometheus.yml | yaml | yaml | real | small(<16KiB) | moderate | 934 | 6465 | 23047 | 9575 | 2761 | True |

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
