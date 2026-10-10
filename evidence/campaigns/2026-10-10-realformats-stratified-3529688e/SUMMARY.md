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
| csv | 4 | 98585 | 24065 | 590369 | 13723.5 | 42718 | 10408 | 13372 | 3080 | yes |
| eml | 4 | 90851 | 23713 | 50107 | 10027.0 | 31022 | 10050 | 12043 | 3010 | yes |
| html | 4 | 96610 | 24049 | 254549 | 29660.0 | 41077 | 10096 | 12111 | 2976 | yes |
| json | 7 | 193431 | 25405 | 5825021 | 43116 | 67817 | 10396 | 29180 | 3694 | yes |
| jsonl | 4 | 124224 | 30528 | 5313784 | 1210532.0 | 63834 | 14586 | 21444 | 4668 | yes |
| markdown | 3 | 76640 | 24168 | 567379 | 23143 | 33153 | 10261 | 9867 | 3035 | yes |
| odp | 3 | 61860 | 20553 | 393099 | 138585 | 122042 | 39916 | 10215 | 3236 | yes |
| ods | 4 | 84396 | 20861 | 126742 | 30040.5 | 103343 | 24144 | 15143 | 3105 | yes |
| parquet | 4 | 98015 | 24250 | 1118877 | 97550.5 | 35007 | 11020 | 17086 | 4432 | yes |
| pptx | 6 | 130332 | 20842 | 1081509 | 113280.5 | 172565 | 42214 | 20278 | 3134 | yes |
| toml | 2 | 47872 | 23936 | 65039 | 32519.5 | 20840 | 10420 | 6035 | 3018 | yes |
| xlsx | 7 | 140149 | 19959 | 312804 | 18571 | 120111 | 27793 | 21999 | 2972 | yes |
| xml | 3 | 68154 | 24013 | 229348 | 46651 | 21437 | 10202 | 9055 | 3040 | yes |
| yaml | 5 | 124103 | 24666 | 76606 | 9179 | 52829 | 10486 | 16052 | 3205 | yes |

## By size class

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| large(>=256KiB) | 6 | 204641 | 35634 | 11422351 | 1584250.0 | 79578 | 15195 | 36935 | 5928 | yes |
| medium(<256KiB) | 17 | 392158 | 21252 | 3719514 | 148293 | 347447 | 11954 | 60044 | 3359 | yes |
| small(<16KiB) | 37 | 838423 | 23843 | 863368 | 18212 | 500770 | 10261 | 116901 | 3031 | yes |

## By complexity

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| complex | 16 | 425144 | 24838 | 12184916 | 178642.5 | 271202 | 12079 | 68335 | 3590 | yes |
| hostile | 8 | 179252 | 19423 | 1054485 | 16712.0 | 17451 | 762 | 29646 | 3195 | yes |
| moderate | 19 | 434980 | 24011 | 2362440 | 44649 | 370763 | 10986 | 60211 | 3062 | yes |
| simple | 17 | 395846 | 23843 | 403392 | 14668 | 268379 | 10342 | 55688 | 3036 | yes |

## By stratum (real vs malformed/hostile)

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| hostile | 8 | 179252 | 19423 | 1054485 | 16712.0 | 17451 | 762 | 29646 | 3195 | yes |
| real | 52 | 1255970 | 24016 | 14950748 | 39336.0 | 910344 | 10845 | 184234 | 3090 | yes |

## By format x size class

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| csv/medium(<256KiB) | 1 | 26479 | 26479 | 552658 | 552658 | 11765 | 11765 | 4176 | 4176 | yes |
| csv/small(<16KiB) | 3 | 72106 | 24018 | 37711 | 12779 | 30953 | 10199 | 9196 | 3069 | yes |
| eml/small(<16KiB) | 4 | 90851 | 23713 | 50107 | 10027.0 | 31022 | 10050 | 12043 | 3010 | yes |
| html/medium(<256KiB) | 1 | 24847 | 24847 | 189800 | 189800 | 10881 | 10881 | 3218 | 3218 | yes |
| html/small(<16KiB) | 3 | 71763 | 24011 | 64749 | 20244 | 30196 | 10084 | 8893 | 2967 | yes |
| json/large(>=256KiB) | 2 | 74661 | 37330 | 4895193 | 2447596.5 | 23665 | 11832 | 12561 | 6280 | yes |
| json/medium(<256KiB) | 1 | 25721 | 25721 | 828220 | 828220 | 12321 | 12321 | 3694 | 3694 | yes |
| json/small(<16KiB) | 4 | 93049 | 24415 | 101608 | 27140.0 | 31831 | 10306 | 12925 | 3106 | yes |
| jsonl/large(>=256KiB) | 2 | 73711 | 36856 | 5085374 | 2542687.0 | 42141 | 21070 | 14261 | 7130 | yes |
| jsonl/medium(<256KiB) | 2 | 50513 | 25256 | 228410 | 114205.0 | 21693 | 10846 | 7183 | 3592 | yes |
| markdown/medium(<256KiB) | 1 | 28629 | 28629 | 530161 | 530161 | 12656 | 12656 | 3831 | 3831 | yes |
| markdown/small(<16KiB) | 2 | 48011 | 24006 | 37218 | 18609.0 | 20497 | 10248 | 6036 | 3018 | yes |
| odp/medium(<256KiB) | 2 | 41528 | 20764 | 254514 | 127257.0 | 78145 | 39072 | 7256 | 3628 | yes |
| odp/small(<16KiB) | 1 | 20332 | 20332 | 138585 | 138585 | 43897 | 43897 | 2959 | 2959 | yes |
| ods/small(<16KiB) | 4 | 84396 | 20861 | 126742 | 30040.5 | 103343 | 24144 | 15143 | 3105 | yes |
| parquet/large(>=256KiB) | 1 | 29995 | 29995 | 914573 | 914573 | 12204 | 12204 | 5186 | 5186 | yes |
| parquet/medium(<256KiB) | 2 | 44349 | 22174 | 195101 | 97550.5 | 12716 | 6358 | 8864 | 4432 | yes |
| parquet/small(<16KiB) | 1 | 23671 | 23671 | 9203 | 9203 | 10087 | 10087 | 3036 | 3036 | yes |
| pptx/large(>=256KiB) | 1 | 26274 | 26274 | 527211 | 527211 | 1568 | 1568 | 4927 | 4927 | yes |
| pptx/medium(<256KiB) | 5 | 104058 | 20841 | 554298 | 103349 | 170997 | 42502 | 15351 | 3090 | yes |
| toml/small(<16KiB) | 2 | 47872 | 23936 | 65039 | 32519.5 | 20840 | 10420 | 6035 | 3018 | yes |
| xlsx/medium(<256KiB) | 1 | 21218 | 21218 | 218867 | 218867 | 5741 | 5741 | 3350 | 3350 | yes |
| xlsx/small(<16KiB) | 6 | 118931 | 19958 | 93937 | 18391.5 | 114370 | 27982 | 18649 | 2962 | yes |
| xml/medium(<256KiB) | 1 | 24816 | 24816 | 167485 | 167485 | 10532 | 10532 | 3121 | 3121 | yes |
| xml/small(<16KiB) | 2 | 43338 | 21669 | 61863 | 30931.5 | 10905 | 5452 | 5934 | 2967 | yes |
| yaml/small(<16KiB) | 5 | 124103 | 24666 | 76606 | 9179 | 52829 | 10486 | 16052 | 3205 | yes |

### Workload dimension

`narrow` = one `observe --metadata --kind metadata` per document (a bounded metadata projection).
`full` = one `materialize --exact --packed` per document (byte-authority closure).
Both are pooled and median; a size/time-weighted world is `pooled`, an unweighted per-document world is `median`.

**Detection honesty.** 9/60 samples were detected as `opaque` by the bounded detector (so the metadata observation declines fast, which lowers their `narrow` cost); they are still byte-exactly closed by the opaque floor. 10 samples declined the narrow observation. The byte-authority gate is unaffected by detection.

**Pooled vs median.** *pooled* sums the cost over every sample (big files dominate); *median* is the unweighted per-document value. `pooled us/B` is total build time over total stored bytes; `median us/B` is the median of each sample's build-time-per-stored-byte.

No real sample was skipped.

### Per-sample

| id | format | detected | stratum | size class | complexity | src B | store B | build us | narrow us | full us | exact |
|---|---|---|---|---|---|---:|---:|---:|---:|---:|---|
| poi-47889.xlsx | xlsx | xlsx | real | small(<16KiB) | simple | 3555 | 19139 | 20560 | 28436 | 3010 | True |
| poi-49609.xlsx | xlsx | xlsx | real | medium(<256KiB) | complex | 105424 | 218867 | 21218 | 5741 | 3350 | True |
| poi-59021.xlsx | xlsx | xlsx | real | small(<16KiB) | moderate | 1933 | 12653 | 20493 | 28170 | 2843 | True |
| poi-DataValidation.xlsx | xlsx | xlsx | real | small(<16KiB) | moderate | 3286 | 18595 | 19959 | 27793 | 2720 | True |
| poi-linkext.xlsx | xlsx | xlsx | real | small(<16KiB) | complex | 3274 | 18571 | 19958 | 28499 | 2952 | True |
| poi-SampleShow.pptx | pptx | pptx | real | medium(<256KiB) | moderate | 39083 | 132296 | 21252 | 42502 | 3090 | True |
| poi-Divino.pptx | pptx | opaque | real | large(>=256KiB) | complex | 523999 | 527211 | 26274 | 1568 | 4927 | True |
| poi-table_test.pptx | pptx | pptx | real | medium(<256KiB) | moderate | 28935 | 103349 | 20608 | 41927 | 2872 | True |
| poi-61515.pptx | pptx | pptx | real | medium(<256KiB) | moderate | 29270 | 99841 | 20515 | 42856 | 2852 | True |
| poi-bug60993.pptx | pptx | pptx | real | medium(<256KiB) | complex | 21942 | 95600 | 20841 | 42801 | 3178 | True |
| odfpy-pythagoras.ods | ods | ods | real | small(<16KiB) | simple | 6837 | 30394 | 22352 | 23903 | 5953 | True |
| odfpy-chinese.ods | ods | ods | real | small(<16KiB) | moderate | 11765 | 44649 | 21117 | 31155 | 3202 | True |
| odfpy-empty.ods | ods | ods | real | small(<16KiB) | simple | 6819 | 29687 | 20605 | 23900 | 3008 | True |
| odfpy-pyth-kspread.ods | ods | ods | real | small(<16KiB) | simple | 5037 | 22012 | 20322 | 24385 | 2980 | True |
| odfpy-cols.odp | odp | odp | real | medium(<256KiB) | moderate | 18037 | 148293 | 20553 | 38229 | 3236 | True |
| odfpy-emb.odp | odp | odp | real | medium(<256KiB) | complex | 16403 | 106221 | 20975 | 39916 | 4020 | True |
| odfpy-ol.odp | odp | odp | real | small(<16KiB) | simple | 14940 | 138585 | 20332 | 43897 | 2959 | True |
| natural-earth-countries.geojson | json | json | real | large(>=256KiB) | complex | 838726 | 4053255 | 35511 | 20989 | 6296 | True |
| natural-earth-places.geojson | json | json | real | medium(<256KiB) | moderate | 166071 | 828220 | 25721 | 12321 | 3694 | True |
| ne-regions.geojson | json | json | real | small(<16KiB) | simple | 4099 | 19476 | 24655 | 10217 | 3139 | True |
| ne-lakes.geojson | json | json | real | small(<16KiB) | moderate | 7719 | 43116 | 24175 | 10396 | 3074 | True |
| ne-bbox.geojson | json | json | real | small(<16KiB) | simple | 4813 | 34804 | 25405 | 10505 | 3801 | True |
| prometheus-ci.yml | yaml | yaml | real | small(<16KiB) | complex | 10201 | 39596 | 25017 | 10486 | 3205 | True |
| alertmanager-ci.yml | yaml | yaml | real | small(<16KiB) | simple | 1348 | 9179 | 25958 | 11179 | 3362 | True |
| prometheus.yml | yaml | yaml | real | small(<16KiB) | moderate | 934 | 6465 | 24666 | 10556 | 3178 | True |
| prometheus-web-config.yml | yaml | yaml | real | small(<16KiB) | simple | 531 | 5022 | 24417 | 10342 | 3245 | True |
| alertmanager-simple.yml | yaml | yaml | real | small(<16KiB) | moderate | 3953 | 16344 | 24045 | 10266 | 3062 | True |
| parquet-delta-expect.csv | csv | csv | real | medium(<256KiB) | moderate | 159803 | 552658 | 26479 | 11765 | 4176 | True |
| vega-iowa-electricity.csv | csv | csv | real | small(<16KiB) | simple | 1531 | 10264 | 23976 | 10617 | 3037 | True |
| vega-global-temp.csv | csv | csv | real | small(<16KiB) | simple | 1663 | 14668 | 24112 | 10137 | 3090 | True |
| vega-population-engineers.csv | csv | csv | real | small(<16KiB) | simple | 1852 | 12779 | 24018 | 10199 | 3069 | True |
| commonmark-readme.md | markdown | markdown | real | small(<16KiB) | simple | 7671 | 23143 | 24168 | 10236 | 3035 | True |
| commonmark-spec.txt | markdown | markdown | real | medium(<256KiB) | complex | 205025 | 530161 | 28629 | 12656 | 3831 | True |
| rust-readme.md | markdown | markdown | real | small(<16KiB) | simple | 3137 | 14075 | 23843 | 10261 | 3001 | True |
| maven-pom.xml | xml | xml | real | medium(<256KiB) | complex | 28489 | 167485 | 24816 | 10532 | 3121 | True |
| maven-core-pom.xml | xml | xml | real | small(<16KiB) | moderate | 7411 | 46651 | 24013 | 10202 | 3040 | True |
| mdn-getting-started.html | html | html | real | small(<16KiB) | simple | 224 | 5429 | 23665 | 10004 | 2941 | True |
| mdn-doc-structure.html | html | html | real | small(<16KiB) | moderate | 3525 | 20244 | 24011 | 10084 | 2967 | True |
| mdn-planets.html | html | html | real | small(<16KiB) | moderate | 4302 | 39076 | 24087 | 10108 | 2985 | True |
| mdn-fonts-demo.html | html | html | real | medium(<256KiB) | complex | 36867 | 189800 | 24847 | 10881 | 3218 | True |
| serde-cargo.toml | toml | toml | real | small(<16KiB) | moderate | 2431 | 13246 | 23864 | 10031 | 3038 | True |
| cargo-cargo.toml | toml | toml | real | small(<16KiB) | complex | 7717 | 51793 | 24008 | 10809 | 2997 | True |
| jsonlines-datagov100.json | jsonl | jsonl | real | large(>=256KiB) | complex | 696007 | 2253927 | 37953 | 18186 | 8670 | True |
| oai-toy-chat.jsonl | jsonl | jsonl | real | medium(<256KiB) | moderate | 27385 | 61273 | 25297 | 10707 | 3744 | True |
| oai-dbpedia.jsonl | jsonl | jsonl | real | medium(<256KiB) | moderate | 64512 | 167137 | 25216 | 10986 | 3439 | True |
| oai-parallel-requests.jsonl | jsonl | jsonl | real | large(>=256KiB) | complex | 548917 | 2831447 | 35758 | 23955 | 5591 | True |
| cpython-msg-01.eml | eml | eml | real | small(<16KiB) | simple | 459 | 5533 | 23787 | 10074 | 3022 | True |
| cpython-msg-25.eml | eml | opaque | real | small(<16KiB) | moderate | 5122 | 8334 | 18909 | 709 | 2999 | True |
| cpython-msg-43.eml | eml | eml | real | small(<16KiB) | complex | 9166 | 24520 | 24516 | 10025 | 2991 | True |
| parquet-alltypes-plain.parquet | parquet | parquet | real | small(<16KiB) | simple | 1851 | 9203 | 23671 | 10087 | 3036 | True |
| parquet-delta-binary.parquet | parquet | parquet | real | medium(<256KiB) | complex | 72971 | 161889 | 24828 | 11954 | 4802 | True |
| parquet-alltypes-tiny.parquet | parquet | parquet | real | large(>=256KiB) | complex | 454233 | 914573 | 29995 | 12204 | 5186 | True |
| hostile-trunc-xlsx | xlsx | opaque | hostile | small(<16KiB) | hostile | 15000 | 18212 | 19119 | 763 | 2972 | True |
| hostile-magic-xlsx | xlsx | opaque | hostile | small(<16KiB) | hostile | 3555 | 6767 | 18842 | 709 | 4152 | True |
| hostile-trunc-pptx | pptx | opaque | hostile | medium(<256KiB) | hostile | 120000 | 123212 | 20842 | 911 | 3359 | True |
| hostile-magic-json | json | opaque | hostile | large(>=256KiB) | hostile | 838726 | 841938 | 39150 | 2676 | 6265 | True |
| hostile-trunc-parquet | parquet | opaque | hostile | medium(<256KiB) | hostile | 30000 | 33212 | 19521 | 762 | 4062 | True |
| hostile-trunc-json-mid | json | opaque | hostile | small(<16KiB) | hostile | 1000 | 4212 | 18814 | 713 | 2911 | True |
| hostile-trunc-eml | eml | eml | hostile | small(<16KiB) | hostile | 3000 | 11720 | 23639 | 10214 | 3031 | True |
| hostile-trunc-xml | xml | opaque | hostile | small(<16KiB) | hostile | 12000 | 15212 | 19325 | 703 | 2894 | True |

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
  court in the capped `doc-baseline` lane.
