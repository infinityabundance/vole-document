# Phase 21.5.3 (FIX 4) — stratified real-world multi-format smoke

**Question.** A concrete, bounded first step toward comparing against
independently sourced real-world documents: ingest the pinned samples in
`tools/realcorpus/formats-manifest.tsv` (XLSX, PPTX, ODS, ODP, JSON, YAML
from stable public sources) and report **pooled costs alongside medians**, so
the size/time-weighted picture is visible next to the unweighted one.

**Verdict: `PASS`**

| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled us/B | median us/B | exact |
|---|---:|---:|---:|---:|---:|---:|---:|---|
| ALL | 12 | 287452 | 22123 | 5979774 | 79127.0 | 0.04807 | 0.33520 | yes |
| size large(>=256KiB) | 2 | 64578 | 32289 | 4580658 | 2290329.0 | 0.01410 | 0.03031 | yes |
| size medium(<256KiB) | 5 | 114142 | 22106 | 1254736 | 134994 | 0.09097 | 0.16711 | yes |
| size small(<16KiB) | 5 | 108732 | 21607 | 144380 | 30490 | 0.75310 | 0.72614 | yes |
| format json | 2 | 64816 | 32408 | 4881667 | 2440833.5 | 0.01328 | 0.02112 | yes |
| format odp | 2 | 42146 | 21073 | 177917 | 88958.5 | 0.23689 | 0.43390 | yes |
| format ods | 2 | 43423 | 21712 | 75235 | 37617.5 | 0.57716 | 0.60090 | yes |
| format pptx | 2 | 49652 | 24826 | 662301 | 331150.5 | 0.07497 | 0.10925 | yes |
| format xlsx | 2 | 43713 | 21856 | 138306 | 69153.0 | 0.31606 | 0.53305 | yes |
| format yaml | 2 | 43702 | 21851 | 44348 | 22174.0 | 0.98543 | 2.42340 | yes |

**Pooled vs median.** *pooled* sums the cost over every sample (so big files
dominate); *median* is the unweighted per-document value (so every small
fixture counts equally). `pooled us/B` is total build time over total stored
bytes; `median us/B` is the median of each sample's build-time-per-stored-byte.

All manifest samples were present and ingested.

### Per-sample

| id | format | size class | src B | store B | build us | exact |
|---|---|---|---:|---:|---:|---|
| poi-47889.xlsx | xlsx | small(<16KiB) | 3555 | 24797 | 21607 | True |
| poi-49609.xlsx | xlsx | medium(<256KiB) | 105424 | 113509 | 22106 | True |
| poi-SampleShow.pptx | pptx | medium(<256KiB) | 39083 | 134994 | 22559 | True |
| poi-Divino.pptx | pptx | large(>=256KiB) | 523999 | 527307 | 27093 | True |
| odfpy-pythagoras.ods | ods | small(<16KiB) | 6837 | 30490 | 22140 | True |
| odfpy-chinese.ods | ods | small(<16KiB) | 11765 | 44745 | 21283 | True |
| odfpy-cols.odp | odp | medium(<256KiB) | 18037 | 148389 | 20626 | True |
| odfpy-emb.odp | odp | medium(<256KiB) | 16403 | 29528 | 21520 | True |
| natural-earth-countries.geojson | json | large(>=256KiB) | 838726 | 4053351 | 37485 | True |
| natural-earth-places.geojson | json | medium(<256KiB) | 166071 | 828316 | 27331 | True |
| prometheus-ci.yml | yaml | small(<16KiB) | 10201 | 39692 | 23944 | True |
| alertmanager-ci.yml | yaml | small(<16KiB) | 1348 | 4656 | 19758 | True |

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
