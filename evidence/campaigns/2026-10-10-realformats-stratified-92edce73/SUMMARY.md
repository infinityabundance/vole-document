# Phase 21.15 (ITEM 2) — stratified real-world multi-format court

**Question.** Beyond a 12-document smoke: ingest a **stratified** population
of independently sourced real-world documents across the Wave-2 formats
(XLSX, PPTX, ODS, ODP, JSON, YAML, CSV, Markdown, XML, HTML, TOML, JSONL, EML,
Parquet), plus a malformed/hostile stratum, and report **pooled costs
alongside medians** per format and per stratum, separated by workload (narrow
observation vs full materialize).

**Verdict: `PASS`**

## Counts

| ran real | ran hostile | skipped | blocked | exact | samples |
|---:|---:|---:|---:|---:|---:|
| 52 | 8 | 0 | 0 | 60 | 60 |

By format (ran): csv=4, eml=4, html=4, json=7, jsonl=4, markdown=3, odp=3, ods=4, parquet=4, pptx=6, toml=2, xlsx=7, xml=3, yaml=5

## Stratified tables

## By format

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| csv | 4 | 94562 | 23168 | 590369 | 13723.5 | 39127 | 9580 | 11418 | 2718 | yes |
| eml | 4 | 78240 | 18441 | 32457 | 7273.0 | 12007 | 988 | 10714 | 2682 | yes |
| html | 4 | 93151 | 23171 | 255996 | 29660.0 | 39052 | 9698 | 11355 | 2848 | yes |
| json | 7 | 181919 | 23096 | 5825021 | 43116 | 64142 | 9699 | 25518 | 2730 | yes |
| jsonl | 4 | 114223 | 27858 | 5313784 | 1210532.0 | 54693 | 12800 | 15956 | 4018 | yes |
| markdown | 3 | 73466 | 23247 | 567379 | 23143 | 30737 | 9542 | 8896 | 2750 | yes |
| odp | 3 | 61189 | 20417 | 316310 | 138585 | 80231 | 37508 | 10022 | 3031 | yes |
| ods | 4 | 80817 | 20150 | 126742 | 30040.5 | 92721 | 23182 | 11387 | 2834 | yes |
| parquet | 4 | 94225 | 23478 | 1118877 | 97550.5 | 31648 | 9818 | 12738 | 2858 | yes |
| pptx | 6 | 131155 | 20832 | 1081509 | 113280.5 | 169904 | 41686 | 19389 | 2950 | yes |
| toml | 2 | 46406 | 23203 | 31097 | 15548.5 | 21719 | 10860 | 5496 | 2748 | yes |
| xlsx | 7 | 139812 | 20063 | 207350 | 18571 | 114127 | 27617 | 19829 | 2813 | yes |
| xml | 3 | 65648 | 23290 | 229348 | 46651 | 20243 | 9765 | 8319 | 2713 | yes |
| yaml | 5 | 111083 | 23118 | 71987 | 6465 | 39113 | 9644 | 14230 | 2760 | yes |

## By size class

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| large(>=256KiB) | 6 | 190679 | 32492 | 11422351 | 1584250.0 | 71203 | 13565 | 30624 | 5055 | yes |
| medium(<256KiB) | 17 | 381951 | 21163 | 3538718 | 132296 | 296015 | 10070 | 51175 | 2957 | yes |
| small(<16KiB) | 37 | 793266 | 22943 | 807157 | 16344 | 442246 | 9644 | 103468 | 2740 | yes |

## By complexity

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| complex | 16 | 402916 | 23674 | 11961203 | 164687.0 | 203641 | 10763 | 56816 | 2942 | yes |
| hostile | 8 | 165806 | 18714 | 1048977 | 16712.0 | 6555 | 506 | 25049 | 2744 | yes |
| moderate | 19 | 423687 | 23074 | 2359273 | 44649 | 357286 | 9925 | 54358 | 2806 | yes |
| simple | 17 | 373487 | 23006 | 398773 | 14668 | 241982 | 9644 | 49044 | 2764 | yes |

## By stratum (real vs malformed/hostile)

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| hostile | 8 | 165806 | 18714 | 1048977 | 16712.0 | 6555 | 506 | 25049 | 2744 | yes |
| real | 52 | 1200090 | 23098 | 14719249 | 32599.0 | 802909 | 9845 | 160218 | 2798 | yes |

## By format x size class

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| csv/medium(<256KiB) | 1 | 25414 | 25414 | 552658 | 552658 | 10554 | 10554 | 3359 | 3359 | yes |
| csv/small(<16KiB) | 3 | 69148 | 23006 | 37711 | 12779 | 28573 | 9506 | 8059 | 2687 | yes |
| eml/small(<16KiB) | 4 | 78240 | 18441 | 32457 | 7273.0 | 12007 | 988 | 10714 | 2682 | yes |
| html/medium(<256KiB) | 1 | 23735 | 23735 | 191247 | 191247 | 10021 | 10021 | 2957 | 2957 | yes |
| html/small(<16KiB) | 3 | 69416 | 23101 | 64749 | 20244 | 29031 | 9644 | 8398 | 2740 | yes |
| json/large(>=256KiB) | 2 | 68968 | 34484 | 4895193 | 2447596.5 | 23104 | 11552 | 11552 | 5776 | yes |
| json/medium(<256KiB) | 1 | 25518 | 25518 | 828220 | 828220 | 11656 | 11656 | 3262 | 3262 | yes |
| json/small(<16KiB) | 4 | 87433 | 22996 | 101608 | 27140.0 | 29382 | 9616 | 10704 | 2670 | yes |
| jsonl/large(>=256KiB) | 2 | 66392 | 33196 | 5085374 | 2542687.0 | 35061 | 17530 | 10110 | 5055 | yes |
| jsonl/medium(<256KiB) | 2 | 47831 | 23916 | 228410 | 114205.0 | 19632 | 9816 | 5846 | 2923 | yes |
| markdown/medium(<256KiB) | 1 | 26981 | 26981 | 530161 | 530161 | 11672 | 11672 | 3480 | 3480 | yes |
| markdown/small(<16KiB) | 2 | 46485 | 23242 | 37218 | 18609.0 | 19065 | 9532 | 5416 | 2708 | yes |
| odp/medium(<256KiB) | 2 | 40772 | 20386 | 177725 | 88862.5 | 42723 | 21362 | 5795 | 2898 | yes |
| odp/small(<16KiB) | 1 | 20417 | 20417 | 138585 | 138585 | 37508 | 37508 | 4227 | 4227 | yes |
| ods/small(<16KiB) | 4 | 80817 | 20150 | 126742 | 30040.5 | 92721 | 23182 | 11387 | 2834 | yes |
| parquet/large(>=256KiB) | 1 | 28191 | 28191 | 914573 | 914573 | 11456 | 11456 | 4237 | 4237 | yes |
| parquet/medium(<256KiB) | 2 | 42896 | 21448 | 195101 | 97550.5 | 10625 | 5312 | 5716 | 2858 | yes |
| parquet/small(<16KiB) | 1 | 23138 | 23138 | 9203 | 9203 | 9567 | 9567 | 2785 | 2785 | yes |
| pptx/large(>=256KiB) | 1 | 27128 | 27128 | 527211 | 527211 | 1582 | 1582 | 4725 | 4725 | yes |
| pptx/medium(<256KiB) | 5 | 104027 | 20803 | 554298 | 103349 | 168322 | 41841 | 14664 | 2874 | yes |
| toml/small(<16KiB) | 2 | 46406 | 23203 | 31097 | 15548.5 | 21719 | 10860 | 5496 | 2748 | yes |
| xlsx/medium(<256KiB) | 1 | 21163 | 21163 | 113413 | 113413 | 776 | 776 | 3169 | 3169 | yes |
| xlsx/small(<16KiB) | 6 | 118649 | 19990 | 93937 | 18391.5 | 113351 | 27722 | 16660 | 2788 | yes |
| xml/medium(<256KiB) | 1 | 23614 | 23614 | 167485 | 167485 | 10034 | 10034 | 2927 | 2927 | yes |
| xml/small(<16KiB) | 2 | 42034 | 21017 | 61863 | 30931.5 | 10209 | 5104 | 5392 | 2696 | yes |
| yaml/small(<16KiB) | 5 | 111083 | 23118 | 71987 | 6465 | 39113 | 9644 | 14230 | 2760 | yes |

### Workload dimension

`narrow` = one `observe --metadata --kind metadata` per document (a bounded metadata projection).
`full` = one `materialize --exact --packed` per document (byte-authority closure).
Both are pooled and median; a size/time-weighted world is `pooled`, an unweighted per-document world is `median`.

**Detection honesty.** 14/60 samples were detected as `opaque` by the bounded detector (so the metadata observation declines fast, which lowers their `narrow` cost); they are still byte-exactly closed by the opaque floor. 14 samples declined the narrow observation. The byte-authority gate is unaffected by detection.

**Pooled vs median.** *pooled* sums the cost over every sample (big files dominate); *median* is the unweighted per-document value. `pooled us/B` is total build time over total stored bytes; `median us/B` is the median of each sample's build-time-per-stored-byte.

No real sample was skipped.

### Per-sample

| id | format | detected | stratum | size class | complexity | src B | store B | build us | narrow us | full us | exact |
|---|---|---|---|---|---|---:|---:|---:|---:|---:|---|
| poi-47889.xlsx | xlsx | xlsx | real | small(<16KiB) | simple | 3555 | 19139 | 20063 | 28966 | 2764 | True |
| poi-49609.xlsx | xlsx | opaque | real | medium(<256KiB) | complex | 105424 | 113413 | 21163 | 776 | 3169 | True |
| poi-59021.xlsx | xlsx | xlsx | real | small(<16KiB) | moderate | 1933 | 12653 | 21128 | 28062 | 2823 | True |
| poi-DataValidation.xlsx | xlsx | xlsx | real | small(<16KiB) | moderate | 3286 | 18595 | 20394 | 27827 | 2881 | True |
| poi-linkext.xlsx | xlsx | xlsx | real | small(<16KiB) | complex | 3274 | 18571 | 19918 | 27617 | 2740 | True |
| poi-SampleShow.pptx | pptx | pptx | real | medium(<256KiB) | moderate | 39083 | 132296 | 21078 | 42160 | 3025 | True |
| poi-Divino.pptx | pptx | opaque | real | large(>=256KiB) | complex | 523999 | 527211 | 27128 | 1582 | 4725 | True |
| poi-table_test.pptx | pptx | pptx | real | medium(<256KiB) | moderate | 28935 | 103349 | 20803 | 42137 | 2873 | True |
| poi-61515.pptx | pptx | pptx | real | medium(<256KiB) | moderate | 29270 | 99841 | 20748 | 41841 | 2874 | True |
| poi-bug60993.pptx | pptx | pptx | real | medium(<256KiB) | complex | 21942 | 95600 | 20536 | 41530 | 2796 | True |
| odfpy-pythagoras.ods | ods | ods | real | small(<16KiB) | simple | 6837 | 30394 | 20527 | 23214 | 2861 | True |
| odfpy-chinese.ods | ods | ods | real | small(<16KiB) | moderate | 11765 | 44649 | 20295 | 23028 | 2806 | True |
| odfpy-empty.ods | ods | ods | real | small(<16KiB) | simple | 6819 | 29687 | 19989 | 23329 | 2920 | True |
| odfpy-pyth-kspread.ods | ods | ods | real | small(<16KiB) | simple | 5037 | 22012 | 20006 | 23150 | 2800 | True |
| odfpy-cols.odp | odp | odp | real | medium(<256KiB) | moderate | 18037 | 148293 | 20048 | 42099 | 3031 | True |
| odfpy-emb.odp | odp | opaque | real | medium(<256KiB) | complex | 16403 | 29432 | 20724 | 624 | 2764 | True |
| odfpy-ol.odp | odp | odp | real | small(<16KiB) | simple | 14940 | 138585 | 20417 | 37508 | 4227 | True |
| natural-earth-countries.geojson | json | json | real | large(>=256KiB) | complex | 838726 | 4053255 | 35677 | 20994 | 5833 | True |
| natural-earth-places.geojson | json | json | real | medium(<256KiB) | moderate | 166071 | 828220 | 25518 | 11656 | 3262 | True |
| ne-regions.geojson | json | json | real | small(<16KiB) | simple | 4099 | 19476 | 22943 | 9533 | 2730 | True |
| ne-lakes.geojson | json | json | real | small(<16KiB) | moderate | 7719 | 43116 | 23049 | 9764 | 2667 | True |
| ne-bbox.geojson | json | json | real | small(<16KiB) | simple | 4813 | 34804 | 23096 | 9699 | 2673 | True |
| prometheus-ci.yml | yaml | yaml | real | small(<16KiB) | complex | 10201 | 39596 | 23278 | 9623 | 2688 | True |
| alertmanager-ci.yml | yaml | opaque | real | small(<16KiB) | simple | 1348 | 4560 | 18189 | 487 | 3346 | True |
| prometheus.yml | yaml | yaml | real | small(<16KiB) | moderate | 934 | 6465 | 23095 | 9653 | 2671 | True |
| prometheus-web-config.yml | yaml | yaml | real | small(<16KiB) | simple | 531 | 5022 | 23118 | 9644 | 2765 | True |
| alertmanager-simple.yml | yaml | yaml | real | small(<16KiB) | moderate | 3953 | 16344 | 23403 | 9706 | 2760 | True |
| parquet-delta-expect.csv | csv | csv | real | medium(<256KiB) | moderate | 159803 | 552658 | 25414 | 10554 | 3359 | True |
| vega-iowa-electricity.csv | csv | csv | real | small(<16KiB) | simple | 1531 | 10264 | 23006 | 9655 | 2748 | True |
| vega-global-temp.csv | csv | csv | real | small(<16KiB) | simple | 1663 | 14668 | 23330 | 9412 | 2687 | True |
| vega-population-engineers.csv | csv | csv | real | small(<16KiB) | simple | 1852 | 12779 | 22812 | 9506 | 2624 | True |
| commonmark-readme.md | markdown | markdown | real | small(<16KiB) | simple | 7671 | 23143 | 23238 | 9542 | 2750 | True |
| commonmark-spec.txt | markdown | markdown | real | medium(<256KiB) | complex | 205025 | 530161 | 26981 | 11672 | 3480 | True |
| rust-readme.md | markdown | markdown | real | small(<16KiB) | simple | 3137 | 14075 | 23247 | 9523 | 2666 | True |
| maven-pom.xml | xml | xml | real | medium(<256KiB) | complex | 28489 | 167485 | 23614 | 10034 | 2927 | True |
| maven-core-pom.xml | xml | xml | real | small(<16KiB) | moderate | 7411 | 46651 | 23290 | 9765 | 2713 | True |
| mdn-getting-started.html | html | html | real | small(<16KiB) | simple | 224 | 5429 | 23241 | 9644 | 3008 | True |
| mdn-doc-structure.html | html | html | real | small(<16KiB) | moderate | 3525 | 20244 | 23101 | 9636 | 2650 | True |
| mdn-planets.html | html | html | real | small(<16KiB) | moderate | 4302 | 39076 | 23074 | 9751 | 2740 | True |
| mdn-fonts-demo.html | html | xml | real | medium(<256KiB) | complex | 36867 | 191247 | 23735 | 10021 | 2957 | True |
| serde-cargo.toml | toml | markdown | real | small(<16KiB) | moderate | 2431 | 10079 | 23187 | 9588 | 2699 | True |
| cargo-cargo.toml | toml | markdown | real | small(<16KiB) | complex | 7717 | 21018 | 23219 | 12131 | 2797 | True |
| jsonlines-datagov100.json | jsonl | jsonl | real | large(>=256KiB) | complex | 696007 | 2253927 | 34699 | 15674 | 5175 | True |
| oai-toy-chat.jsonl | jsonl | jsonl | real | medium(<256KiB) | moderate | 27385 | 61273 | 23809 | 9707 | 2745 | True |
| oai-dbpedia.jsonl | jsonl | jsonl | real | medium(<256KiB) | moderate | 64512 | 167137 | 24022 | 9925 | 3101 | True |
| oai-parallel-requests.jsonl | jsonl | jsonl | real | large(>=256KiB) | complex | 548917 | 2831447 | 31693 | 19387 | 4935 | True |
| cpython-msg-01.eml | eml | eml | real | small(<16KiB) | simple | 459 | 5533 | 23127 | 9603 | 2690 | True |
| cpython-msg-25.eml | eml | opaque | real | small(<16KiB) | moderate | 5122 | 8334 | 18231 | 427 | 2678 | True |
| cpython-msg-43.eml | eml | opaque | real | small(<16KiB) | complex | 9166 | 12378 | 18541 | 450 | 2686 | True |
| parquet-alltypes-plain.parquet | parquet | parquet | real | small(<16KiB) | simple | 1851 | 9203 | 23138 | 9567 | 2785 | True |
| parquet-delta-binary.parquet | parquet | parquet | real | medium(<256KiB) | complex | 72971 | 161889 | 23819 | 10070 | 2907 | True |
| parquet-alltypes-tiny.parquet | parquet | parquet | real | large(>=256KiB) | complex | 454233 | 914573 | 28191 | 11456 | 4237 | True |
| hostile-trunc-xlsx | xlsx | opaque | hostile | small(<16KiB) | hostile | 15000 | 18212 | 18683 | 458 | 2813 | True |
| hostile-magic-xlsx | xlsx | opaque | hostile | small(<16KiB) | hostile | 3555 | 6767 | 18463 | 421 | 2639 | True |
| hostile-trunc-pptx | pptx | opaque | hostile | medium(<256KiB) | hostile | 120000 | 123212 | 20862 | 654 | 3096 | True |
| hostile-magic-json | json | opaque | hostile | large(>=256KiB) | hostile | 838726 | 841938 | 33291 | 2110 | 5719 | True |
| hostile-trunc-parquet | parquet | opaque | hostile | medium(<256KiB) | hostile | 30000 | 33212 | 19077 | 555 | 2809 | True |
| hostile-trunc-json-mid | json | opaque | hostile | small(<16KiB) | hostile | 1000 | 4212 | 18345 | 386 | 2634 | True |
| hostile-trunc-eml | eml | opaque | hostile | small(<16KiB) | hostile | 3000 | 6212 | 18341 | 1527 | 2660 | True |
| hostile-trunc-xml | xml | opaque | hostile | small(<16KiB) | hostile | 12000 | 15212 | 18744 | 444 | 2679 | True |

## Scope (honest)

- **A stratified real-world population, still not a random sample of the
  world.** Samples are pinned by tag/commit and SHA-256 from stable,
  permissively licensed public sources (Apache POI, odfpy, Natural Earth,
  Prometheus/Alertmanager, CommonMark, Maven, MDN, serde/cargo,
  parquet-testing, openai-cookbook, CPython, jsonlines); the bytes are
  gitignored and re-verified on every acquisition. It is stratified, not an
  unbiased distribution.
- **Complexity is a curated label; size class is measured.** Size class comes
  from the byte length; the complexity stratum is a curation label stated in
  the manifest.
- **A malformed/hostile stratum is included.** It is derived deterministically
  from pinned real bytes (truncation, wrong-magic, mid-structure corruption),
  pinned by SHA-256 in `tools/realcorpus/hostile-strata.tsv`.
- **Only byte-exact closure is a byte-authority claim.** Each sample's bytes
  are built from a scratch copy that is then deleted, and a fresh process must
  reproduce `length + SHA-256 + cmp`.
- **Missing samples are SKIPPED/BLOCKED, never fabricated.** Nothing here is
  run on the host; the acquisition ran in the capped `realcorpus` lane and the
  court in the capped `analytical` lane.
