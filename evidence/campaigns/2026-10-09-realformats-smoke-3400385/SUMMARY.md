# Phase 21.5.3 (FIX 4) — stratified real-world multi-format smoke

**Question.** A concrete, bounded first step toward comparing against
independently sourced real-world documents: ingest the pinned samples in
`tools/realcorpus/formats-manifest.tsv` (XLSX, PPTX, ODS, ODP, JSON, YAML
from stable public sources) and report **pooled costs alongside medians**, so
the size/time-weighted picture is visible next to the unweighted one.

**Verdict: `PASS`**

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled us/B | median us/B | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---|
| ALL | 12 | 309908 | 23390 | 5979774 | 79127.0 | 0.05183 | 0.36322 | yes |
| size large(>=256KiB) | 2 | 73261 | 36630 | 4580658 | 2290329.0 | 0.01599 | 0.03357 | yes |
| size medium(<256KiB) | 5 | 122262 | 23631 | 1254736 | 134994 | 0.09744 | 0.17505 | yes |
| size small(<16KiB) | 5 | 114385 | 23095 | 144380 | 30490 | 0.79225 | 0.75851 | yes |
| format json | 2 | 72658 | 36329 | 4881667 | 2440833.5 | 0.01488 | 0.02296 | yes |
| format odp | 2 | 45620 | 22810 | 177917 | 88958.5 | 0.25641 | 0.46771 | yes |
| format ods | 2 | 46222 | 23111 | 75235 | 37617.5 | 0.61437 | 0.63733 | yes |
| format pptx | 2 | 53374 | 26687 | 662301 | 331150.5 | 0.08059 | 0.11573 | yes |
| format xlsx | 2 | 46180 | 23090 | 138306 | 69153.0 | 0.33390 | 0.55498 | yes |
| format yaml | 2 | 45854 | 22927 | 44348 | 22174.0 | 1.03396 | 2.51137 | yes |

**Pooled vs median.** *pooled* sums the cost over every sample (so big files
dominate); *median* is the unweighted per-document value (so every small
fixture counts equally). `pooled us/B` is total build time over total stored
bytes; `median us/B` is the median of each sample's build-time-per-stored-byte.

All manifest samples were present and ingested.

### Per-sample

| id | format | size class | src B | store B | build us | exact |
|---|---|---|---:|---:|---:|---|
| poi-47889.xlsx | xlsx | small(<16KiB) | 3555 | 24797 | 22309 | True |
| poi-49609.xlsx | xlsx | medium(<256KiB) | 105424 | 113509 | 23871 | True |
| poi-SampleShow.pptx | pptx | medium(<256KiB) | 39083 | 134994 | 23631 | True |
| poi-Divino.pptx | pptx | large(>=256KiB) | 523999 | 527307 | 29743 | True |
| odfpy-pythagoras.ods | ods | small(<16KiB) | 6837 | 30490 | 23127 | True |
| odfpy-chinese.ods | ods | small(<16KiB) | 11765 | 44745 | 23095 | True |
| odfpy-cols.odp | odp | medium(<256KiB) | 18037 | 148389 | 22470 | True |
| odfpy-emb.odp | odp | medium(<256KiB) | 16403 | 29528 | 23150 | True |
| natural-earth-countries.geojson | json | large(>=256KiB) | 838726 | 4053351 | 43518 | True |
| natural-earth-places.geojson | json | medium(<256KiB) | 166071 | 828316 | 29140 | True |
| prometheus-ci.yml | yaml | small(<16KiB) | 10201 | 39692 | 25454 | True |
| alertmanager-ci.yml | yaml | small(<16KiB) | 1348 | 4656 | 20400 | True |

## Scope (honest)

- **Bounded first step, not a population.** 12 real documents across 6
  formats; stratified by format and size class. It is a smoke court, not a
  claim about real-world distributions.
- The samples come from stable, permissively licensed public sources (Apache
  POI / odfpy / Natural Earth / Prometheus), pinned by tag or commit and by
  SHA-256; the bytes are gitignored and re-verified on every acquisition.
- Every present sample is required to materialize byte-exactly
  (`length + SHA-256 + cmp`); a missing sample is recorded SKIPPED, never
  replaced by a synthetic one.
- Nothing here is run on the host.
