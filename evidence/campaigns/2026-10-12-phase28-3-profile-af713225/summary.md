# Phase 28.3 — hot-path profiling: paired ablation of the `serve_source_range` clone

_Measurement-only gate. No production mechanism; the selective prototype is compiled
only under the non-default `phase28-3-ablation` feature and enabled by `VOLE_SERVE_SELECTIVE`._

## Per-stage attribution (median over reps, variant=current)

| doc | obs | wall ms | observe ms | materialize ms | source_serve µs | src-serve % of obs | select µs | clone bytes | referenced bytes | calls | clone B/call | obj/call used/total | peak RSS KiB |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nist-docx-0014 | --heading 0 | 67.9 | 63.5 | 63.0 | 738.0 | 1.16% | 4.0 | 1896844 | 1896844 | 2 | 948422 | 1/1 | 12754 |
| nist-epub-0003 | --heading 0 | 96.2 | 94.1 | 93.8 | 66.0 | 0.07% | 1.0 | 538530 | 538530 | 6 | 89755 | 1/1 | 5894 |
| nist-pdf-0004 | --doc-text | 1945.9 | 1941.3 | 1938.5 | 23178.0 | 1.19% | 108.0 | 88404880 | 88404880 | 80 | 1105061 | 1/1 | 8818 |
| nist-pdf-0002 | --page 1 | 120.8 | 112.6 | 112.5 | 106.0 | 0.09% | 1.0 | 263703 | 263703 | 1 | 263703 | 1/1 | 5196 |
| nist-docx-0005 | --block 0 | 40.8 | 38.8 | 38.6 | 16.0 | 0.04% | 2.0 | 114090 | 114090 | 2 | 57045 | 1/1 | 5834 |
| nist-epub-0009 | --metadata | 15.1 | 10.9 | 10.8 | 0.0 | 0.00% | 0.0 | 0 | 0 | 0 | 0 | 0/0 | 8980 |
| nist-docx-0014 | --block 0 | 66.1 | 62.2 | 61.8 | 622.0 | 1.00% | 2.0 | 1896844 | 1896844 | 2 | 948422 | 1/1 | 12756 |
| nist-epub-0003 | --block 0 | 25.2 | 23.3 | 23.2 | 11.0 | 0.05% | 1.0 | 89755 | 89755 | 1 | 89755 | 1/1 | 5474 |
| nist-pdf-0002 | --doc-text | 1267.0 | 1264.2 | 1262.3 | 1311.0 | 0.10% | 88.0 | 13976259 | 13976259 | 53 | 263703 | 1/1 | 5538 |
| nist-pdf-0002 | --metadata | 2.2 | 0.0 | 0.0 | 0.0 | 0.00% | 0.0 | 0 | 0 | 0 | 0 | 0/0 | 4672 |
| nist-epub-0009 | --doc-text | 309.1 | 304.1 | 302.8 | 5918.0 | 1.95% | 4.0 | 24745100 | 24745100 | 20 | 1237255 | 1/1 | 10626 |
| nist-docx-0014 | --metadata | 847.1 | 827.1 | 826.6 | 706.0 | 0.09% | 4.0 | 1896844 | 1896844 | 2 | 948422 | 1/1 | 12688 |
| nist-epub-0003 | --metadata | 11.1 | 9.3 | 9.2 | 0.0 | 0.00% | 0.0 | 0 | 0 | 0 | 0 | 0/0 | 5462 |
| nist-pdf-0004 | --page 1 | 27.3 | 23.2 | 23.2 | 353.0 | 1.52% | 1.0 | 1105061 | 1105061 | 1 | 1105061 | 1/1 | 8568 |
| nist-docx-0014 | --doc-text | 69.0 | 64.8 | 64.3 | 640.0 | 0.99% | 2.0 | 1896844 | 1896844 | 2 | 948422 | 1/1 | 12732 |
| nist-epub-0003 | --doc-text | 228.9 | 226.5 | 225.6 | 317.0 | 0.14% | 4.0 | 1346325 | 1346325 | 15 | 89755 | 1/1 | 6088 |
| nist-epub-0009 | --heading 0 | 100.0 | 95.5 | 95.0 | 1720.0 | 1.80% | 2.0 | 7423530 | 7423530 | 6 | 1237255 | 1/1 | 10228 |
| nist-epub-0009 | --block 0 | 29.0 | 24.8 | 24.7 | 285.0 | 1.15% | 1.0 | 1237255 | 1237255 | 1 | 1237255 | 1/1 | 9834 |
| nist-pdf-0004 | --metadata | 4.0 | 0.0 | 0.0 | 0.0 | 0.00% | 0.0 | 0 | 0 | 0 | 0 | 0/0 | 6300 |
| nist-docx-0005 | --metadata | 41.1 | 38.9 | 38.7 | 17.0 | 0.04% | 2.0 | 114090 | 114090 | 2 | 57045 | 1/1 | 5890 |
| nist-docx-0005 | --doc-text | 44.2 | 42.2 | 42.0 | 20.0 | 0.05% | 2.0 | 114090 | 114090 | 2 | 57045 | 1/1 | 5802 |

## Paired ablation (ratio = selective / current; <1 means selective faster)

- paired samples: 168; documents (clusters): 6
- pooled median paired ratio: **0.9999**
- pooled geometric-mean paired ratio: **1.0040**
- fixed-seed cluster bootstrap 95% CI (B=20000, resampled by document):
  - median: **0.9982 .. 1.0030**
  - geometric mean: **0.9925 .. 1.0145**
- MDE (80% power, normal approx from the CI half-width): median-CI **0.0035**; geomean-CI **0.0157**
- observation ceiling for ANY clone-removal (share of wall in the whole `serve_source_range` call, i.e. clone + window eval + channel decode), median across (doc,obs): **0.09%** (the clone alone is a fraction of this).
- p50/p95/p99 wall (all samples): current **65473.5 / 1.27299e+06 / 1.95585e+06 µs**, selective **65876.5 / 1.27184e+06 / 1.95968e+06 µs**

| doc | n | median ratio | geom-mean ratio | win | tie | loss |
|---|---:|---:|---:|---:|---:|---:|
| nist-epub-0003 | 32 | 0.9995 | 1.0218 | 1 | 28 | 3 |
| nist-docx-0005 | 24 | 0.9999 | 1.0099 | 1 | 20 | 3 |
| nist-pdf-0004 | 24 | 0.9988 | 1.0009 | 1 | 21 | 2 |
| nist-docx-0014 | 32 | 1.0006 | 0.9851 | 4 | 24 | 4 |
| nist-epub-0009 | 32 | 1.0141 | 1.0139 | 1 | 30 | 1 |
| nist-pdf-0002 | 24 | 0.9978 | 0.9897 | 2 | 20 | 2 |

## Control axis (must NOT move)

Byte counts the window references are a property of the descriptor, not of the
variant, so `referenced_bytes` must be identical across variants; on this corpus
the object table is a single always-referenced whole-source object, so
`clone_bytes` is also identical (the selective variant copies the same bytes).

| doc | obs | clone cur / sel | referenced cur / sel |
|---|---|---:|---:|
| nist-epub-0003 | --block 0 | 89755 / 89755 | 89755 / 89755 |
| nist-epub-0003 | --metadata | 0 / 0 | 0 / 0 |
| nist-pdf-0002 | --metadata | 0 / 0 | 0 / 0 |
| nist-docx-0014 | --block 0 | 1896844 / 1896844 | 1896844 / 1896844 |
| nist-epub-0009 | --metadata | 0 / 0 | 0 / 0 |
| nist-docx-0014 | --heading 0 | 1896844 / 1896844 | 1896844 / 1896844 |
| nist-docx-0005 | --metadata | 114090 / 114090 | 114090 / 114090 |
| nist-pdf-0004 | --page 1 | 1105061 / 1105061 | 1105061 / 1105061 |
| nist-docx-0014 | --metadata | 1896844 / 1896844 | 1896844 / 1896844 |
| nist-docx-0005 | --doc-text | 114090 / 114090 | 114090 / 114090 |
| nist-pdf-0002 | --doc-text | 13976259 / 13976259 | 13976259 / 13976259 |
| nist-epub-0009 | --doc-text | 24745100 / 24745100 | 24745100 / 24745100 |
| nist-epub-0003 | --doc-text | 1346325 / 1346325 | 1346325 / 1346325 |
| nist-epub-0003 | --heading 0 | 538530 / 538530 | 538530 / 538530 |
| nist-epub-0009 | --heading 0 | 7423530 / 7423530 | 7423530 / 7423530 |
| nist-pdf-0002 | --page 1 | 263703 / 263703 | 263703 / 263703 |
| nist-pdf-0004 | --doc-text | 88404880 / 88404880 | 88404880 / 88404880 |
| nist-docx-0005 | --block 0 | 114090 / 114090 | 114090 / 114090 |
| nist-pdf-0004 | --metadata | 0 / 0 | 0 / 0 |
| nist-docx-0014 | --doc-text | 1896844 / 1896844 | 1896844 / 1896844 |
| nist-epub-0009 | --block 0 | 1237255 / 1237255 | 1237255 / 1237255 |

## Exactness

Every run's answer JSON is normalised by eliding the volatile `stats` object and
SHA-256'd; the current and selective variants must agree for every (doc, obs).

EXACTNESS pairs=21 mismatches=0

## Decision

- **Selection-before-materialization is a no-op on this corpus** (0 observations where the
  window references fewer bytes than the object table): `clone_bytes == referenced_bytes`
  for every (doc, obs). The object table is a single whole-source inline object that every
  window references, so copying only the referenced objects copies exactly the same bytes.
- End-to-end paired effect (selective/current): pooled median 0.9999, geometric mean 1.0040.
- **Resolved faster beyond the CI?**: median estimator **no**, geometric-mean estimator **no**.
- **Verdict: NEGATIVE — no resolved end-to-end improvement.** 28.4 should ship neither the
  selection-before-materialization rewrite (it copies the same bytes) nor claim a clone win:
  the whole `serve_source_range` call is a small share of wall (see the ceiling above), and any
  remaining clone cost is a zero-copy-eval question that a selective rewrite does not address.

Raw samples: `evidence/campaigns/2026-10-12-phase28-3-profile-af713225/raw/samples.tsv` (one row per run). Estimator tables under `evidence/campaigns/2026-10-12-phase28-3-profile-af713225/out/`.
