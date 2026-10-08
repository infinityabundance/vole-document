# Phase 22.4 — unknown-query lifetime frontier (measurement court)

On a **pre-registered, seeded hidden schedule** the encoder never saw, per **equivalent successful observation** (session cost / |observations answered by BOTH lanes|). Declines are excluded from numerator and denominator identically. VOLE/SQLite ratio **< 1 favours VOLE**. Every adaptation is charged. **No single averaged headline is reported:** every number below is a named region.

- documents: **12**; lanes: VOLE-packed vs {full, adaptive, hist}; depths: **C0, C1, C2, C3, C4, C5**; reps: **15**; bootstrap: **10000 resamples, seed 220400**, cluster-resampled by document; tie band **+/-10%**.
- **binding prior (ADR-0046):** the Phase-15.6 adaptive-promotion experiment LOST; a VOLE win is **not** presumed likely. The pre-registered expectation is no advantage outside a narrow region, if any.
- hidden schedule: seed **220477**, length **24**, one per document (recipe frozen in `tools/fixtures/phase22-4-schedule.py`); counts:
- pooled schedule counts over 12 docs: bytes=49, doc-text=28, heading=19, metadata=53, resource=26, revision=42, table=24, text=47

## Primary frontier — VOLE vs the tuned `full` envelope, region by region

### Pooled over documents, by depth (primary `full`)

| lane | depth | docs | pairs | median (95% CI) | geomean (95% CI) | W/T/L | MDE | outcome |
|---|---|---:|---:|---|---|---|---:|---|
| full | C0 | 12 | 180 | 0.995 (0.839..1.780) | 1.191 (0.942..1.556) | 5/2/5 | +/-0.470 | unresolved |
| full | C1 | 12 | 180 | 1.017 (0.833..1.822) | 1.211 (0.946..1.573) | 4/3/5 | +/-0.494 | unresolved |
| full | C2 | 12 | 180 | 0.975 (0.777..1.530) | 1.119 (0.880..1.443) | 5/2/5 | +/-0.377 | unresolved |
| full | C3 | 12 | 180 | 0.969 (0.781..1.609) | 1.114 (0.874..1.450) | 5/2/5 | +/-0.414 | unresolved |
| full | C4 | 12 | 180 | 0.903 (0.727..1.328) | 1.050 (0.828..1.366) | 6/1/5 | +/-0.300 | unresolved |
| full | C5 | 12 | 180 | 0.901 (0.706..1.599) | 1.044 (0.813..1.358) | 7/0/5 | +/-0.446 | unresolved |

### Format `docx` (4 docs), by depth (primary `full`)

| lane | depth | docs | pairs | median (95% CI) | geomean (95% CI) | W/T/L | MDE | outcome |
|---|---|---:|---:|---|---|---|---:|---|
| full | C0 | 4 | 60 | 0.994 (0.767..1.319) | 0.990 (0.817..1.226) | 2/1/1 | +/-0.276 | unresolved |
| full | C1 | 4 | 60 | 1.008 (0.760..1.337) | 0.991 (0.808..1.233) | 2/1/1 | +/-0.288 | unresolved |
| full | C2 | 4 | 60 | 0.909 (0.681..1.255) | 0.896 (0.709..1.136) | 2/1/1 | +/-0.287 | unresolved |
| full | C3 | 4 | 60 | 0.937 (0.665..1.221) | 0.894 (0.709..1.127) | 2/1/1 | +/-0.278 | unresolved |
| full | C4 | 4 | 60 | 0.886 (0.630..1.188) | 0.845 (0.670..1.077) | 2/1/1 | +/-0.279 | unresolved |
| full | C5 | 4 | 60 | 0.810 (0.612..1.199) | 0.836 (0.661..1.090) | 3/0/1 | +/-0.293 | unresolved |

### Format `epub` (4 docs), by depth (primary `full`)

| lane | depth | docs | pairs | median (95% CI) | geomean (95% CI) | W/T/L | MDE | outcome |
|---|---|---:|---:|---|---|---|---:|---|
| full | C0 | 4 | 60 | 2.302 (1.513..2.395) | 2.084 (1.603..2.404) | 0/0/4 | +/-0.441 | loss |
| full | C1 | 4 | 60 | 2.345 (1.479..2.476) | 2.128 (1.629..2.495) | 0/0/4 | +/-0.498 | loss |
| full | C2 | 4 | 60 | 2.168 (1.279..2.285) | 1.925 (1.449..2.290) | 0/0/4 | +/-0.503 | loss |
| full | C3 | 4 | 60 | 2.127 (1.279..2.290) | 1.917 (1.434..2.293) | 0/0/4 | +/-0.505 | loss |
| full | C4 | 4 | 60 | 2.021 (1.221..2.298) | 1.825 (1.372..2.220) | 0/0/4 | +/-0.538 | loss |
| full | C5 | 4 | 60 | 2.057 (1.210..2.216) | 1.830 (1.352..2.204) | 0/0/4 | +/-0.503 | loss |

### Format `pdf` (4 docs), by depth (primary `full`)

| lane | depth | docs | pairs | median (95% CI) | geomean (95% CI) | W/T/L | MDE | outcome |
|---|---|---:|---:|---|---|---|---:|---|
| full | C0 | 4 | 60 | 0.831 (0.753..0.907) | 0.819 (0.742..0.903) | 3/1/0 | +/-0.077 | win |
| full | C1 | 4 | 60 | 0.831 (0.756..0.940) | 0.842 (0.765..0.927) | 2/2/0 | +/-0.092 | win |
| full | C2 | 4 | 60 | 0.832 (0.713..0.902) | 0.812 (0.737..0.895) | 3/1/0 | +/-0.094 | win |
| full | C3 | 4 | 60 | 0.811 (0.713..0.898) | 0.808 (0.736..0.888) | 3/1/0 | +/-0.092 | win |
| full | C4 | 4 | 60 | 0.752 (0.673..0.836) | 0.750 (0.667..0.843) | 4/0/0 | +/-0.081 | win |
| full | C5 | 4 | 60 | 0.701 (0.668..0.847) | 0.744 (0.671..0.826) | 4/0/0 | +/-0.089 | win |

### Size class `1-10MiB` (3 docs), by depth (primary `full`)

| lane | depth | docs | pairs | median (95% CI) | geomean (95% CI) | W/T/L | MDE | outcome |
|---|---|---:|---:|---|---|---|---:|---|
| full | C0 | 3 | 45 | 0.943 (0.873..2.395) | 1.257 (0.886..2.432) | 1/1/1 | +/-0.761 | unresolved |
| full | C1 | 3 | 45 | 0.976 (0.900..2.518) | 1.295 (0.909..2.527) | 0/2/1 | +/-0.809 | unresolved |
| full | C2 | 3 | 45 | 0.925 (0.900..2.354) | 1.234 (0.892..2.343) | 1/1/1 | +/-0.727 | unresolved |
| full | C3 | 3 | 45 | 0.938 (0.892..2.437) | 1.229 (0.872..2.355) | 1/1/1 | +/-0.772 | unresolved |
| full | C4 | 3 | 45 | 0.862 (0.828..2.342) | 1.184 (0.833..2.336) | 2/0/1 | +/-0.757 | unresolved |
| full | C5 | 3 | 45 | 0.896 (0.843..2.297) | 1.159 (0.824..2.279) | 2/0/1 | +/-0.727 | unresolved |

### Size class `100KiB-1MiB` (5 docs), by depth (primary `full`)

| lane | depth | docs | pairs | median (95% CI) | geomean (95% CI) | W/T/L | MDE | outcome |
|---|---|---:|---:|---|---|---|---:|---|
| full | C0 | 5 | 75 | 1.046 (0.762..2.154) | 1.128 (0.795..1.673) | 2/1/2 | +/-0.696 | unresolved |
| full | C1 | 5 | 75 | 1.064 (0.761..2.233) | 1.149 (0.818..1.687) | 2/1/2 | +/-0.736 | unresolved |
| full | C2 | 5 | 75 | 1.003 (0.723..2.039) | 1.078 (0.783..1.553) | 2/1/2 | +/-0.658 | unresolved |
| full | C3 | 5 | 75 | 0.990 (0.735..1.998) | 1.073 (0.782..1.543) | 2/1/2 | +/-0.631 | unresolved |
| full | C4 | 5 | 75 | 0.919 (0.683..1.828) | 0.993 (0.713..1.418) | 2/1/2 | +/-0.572 | unresolved |
| full | C5 | 5 | 75 | 0.885 (0.675..1.855) | 0.999 (0.711..1.463) | 3/0/2 | +/-0.590 | unresolved |

### Size class `<100KiB` (4 docs), by depth (primary `full`)

| lane | depth | docs | pairs | median (95% CI) | geomean (95% CI) | W/T/L | MDE | outcome |
|---|---|---:|---:|---|---|---|---:|---|
| full | C0 | 4 | 60 | 1.193 (0.767..2.299) | 1.224 (0.820..1.861) | 2/0/2 | +/-0.766 | unresolved |
| full | C1 | 4 | 60 | 1.153 (0.760..2.437) | 1.229 (0.808..1.902) | 2/0/2 | +/-0.839 | unresolved |
| full | C2 | 4 | 60 | 1.109 (0.681..2.202) | 1.089 (0.709..1.719) | 2/0/2 | +/-0.760 | unresolved |
| full | C3 | 4 | 60 | 1.000 (0.665..2.135) | 1.086 (0.709..1.711) | 2/0/2 | +/-0.735 | unresolved |
| full | C4 | 4 | 60 | 1.065 (0.630..2.055) | 1.028 (0.670..1.611) | 2/0/2 | +/-0.713 | unresolved |
| full | C5 | 4 | 60 | 1.010 (0.612..2.066) | 1.020 (0.662..1.626) | 2/0/2 | +/-0.727 | unresolved |

## Secondary comparators — `adaptive` (all adaptation charged) and `hist`

### Pooled over documents, by depth (secondary lanes)

| lane | depth | docs | pairs | median (95% CI) | geomean (95% CI) | W/T/L | MDE | outcome |
|---|---|---:|---:|---|---|---|---:|---|
| adaptive | C0 | 12 | 180 | 1.039 (0.841..2.002) | 1.237 (0.967..1.607) | 4/2/6 | +/-0.581 | unresolved |
| adaptive | C1 | 12 | 180 | 1.001 (0.831..1.805) | 1.205 (0.942..1.582) | 4/3/5 | +/-0.487 | unresolved |
| adaptive | C2 | 12 | 180 | 0.969 (0.772..1.476) | 1.121 (0.882..1.460) | 4/3/5 | +/-0.352 | unresolved |
| adaptive | C3 | 12 | 180 | 0.965 (0.782..1.572) | 1.116 (0.879..1.447) | 6/1/5 | +/-0.395 | unresolved |
| adaptive | C4 | 12 | 180 | 0.904 (0.715..1.473) | 1.047 (0.822..1.371) | 6/1/5 | +/-0.379 | unresolved |
| adaptive | C5 | 12 | 180 | 0.896 (0.723..1.504) | 1.041 (0.815..1.357) | 6/1/5 | +/-0.391 | unresolved |
| hist | C0 | 12 | 180 | 1.050 (0.844..1.785) | 1.253 (0.974..1.632) | 3/3/6 | +/-0.470 | unresolved |
| hist | C1 | 12 | 180 | 1.026 (0.838..1.882) | 1.228 (0.951..1.623) | 4/3/5 | +/-0.522 | unresolved |
| hist | C2 | 12 | 180 | 0.993 (0.774..1.827) | 1.171 (0.903..1.547) | 5/2/5 | +/-0.527 | unresolved |
| hist | C3 | 12 | 180 | 1.003 (0.789..1.568) | 1.171 (0.915..1.533) | 5/2/5 | +/-0.389 | unresolved |
| hist | C4 | 12 | 180 | 0.913 (0.722..1.588) | 1.089 (0.844..1.450) | 6/1/5 | +/-0.433 | unresolved |
| hist | C5 | 12 | 180 | 0.915 (0.730..1.707) | 1.088 (0.841..1.438) | 6/1/5 | +/-0.489 | unresolved |

## Frontier axes (VOLE/SQLite; <1 favours VOLE), pooled over documents

| axis | lane | docs | median (95% CI) | MDE | outcome |
|---|---|---:|---|---:|---|
| build us | full | 12 | 0.563 (0.220..0.618) | +/-0.199 | win |
| store bytes | full | 12 | 0.468 (0.313..0.737) | +/-0.212 | win |
| build us | adaptive | 12 | 0.567 (0.222..0.637) | +/-0.207 | win |
| store bytes | adaptive | 12 | 0.463 (0.308..0.736) | +/-0.214 | win |
| build us | hist | 12 | 0.448 (0.208..0.570) | +/-0.181 | win |
| store bytes | hist | 12 | 0.287 (0.185..0.541) | +/-0.178 | win |
| peak RSS C0 | full | 12 | 1.305 (1.253..1.355) | +/-0.051 | loss |
| peak RSS C0 | adaptive | 12 | 1.313 (1.244..1.395) | +/-0.075 | loss |
| peak RSS C0 | hist | 12 | 1.361 (1.267..1.446) | +/-0.089 | loss |
| peak RSS C1 | full | 12 | 1.303 (1.249..1.342) | +/-0.047 | loss |
| peak RSS C1 | adaptive | 12 | 1.287 (1.259..1.355) | +/-0.048 | loss |
| peak RSS C1 | hist | 12 | 1.368 (1.263..1.425) | +/-0.081 | loss |
| peak RSS C2 | full | 12 | 1.270 (1.240..1.313) | +/-0.036 | loss |
| peak RSS C2 | adaptive | 12 | 1.277 (1.237..1.299) | +/-0.031 | loss |
| peak RSS C2 | hist | 12 | 1.314 (1.261..1.400) | +/-0.070 | loss |
| peak RSS C3 | full | 12 | 1.265 (1.236..1.292) | +/-0.028 | loss |
| peak RSS C3 | adaptive | 12 | 1.278 (1.235..1.304) | +/-0.034 | loss |
| peak RSS C3 | hist | 12 | 1.348 (1.248..1.395) | +/-0.074 | loss |
| peak RSS C4 | full | 12 | 1.266 (1.211..1.296) | +/-0.043 | loss |
| peak RSS C4 | adaptive | 12 | 1.281 (1.236..1.321) | +/-0.042 | loss |
| peak RSS C4 | hist | 12 | 1.323 (1.248..1.375) | +/-0.064 | loss |
| peak RSS C5 | full | 12 | 1.246 (1.220..1.306) | +/-0.043 | loss |
| peak RSS C5 | adaptive | 12 | 1.262 (1.234..1.317) | +/-0.042 | loss |
| peak RSS C5 | hist | 12 | 1.316 (1.252..1.370) | +/-0.059 | loss |

### Build and store-bytes frontier per format region

| axis | format | lane | docs | median (95% CI) | MDE | outcome |
|---|---|---|---:|---|---:|---|
| build us | docx | full | 4 | 0.363 (0.218..0.610) | +/-0.196 | win |
| store bytes | docx | full | 4 | 0.321 (0.251..0.764) | +/-0.256 | win |
| build us | docx | adaptive | 4 | 0.355 (0.221..0.608) | +/-0.194 | win |
| store bytes | docx | adaptive | 4 | 0.321 (0.244..0.762) | +/-0.259 | win |
| build us | docx | hist | 4 | 0.337 (0.202..0.553) | +/-0.175 | win |
| store bytes | docx | hist | 4 | 0.210 (0.152..0.598) | +/-0.223 | win |
| build us | epub | full | 4 | 0.614 (0.522..0.782) | +/-0.130 | win |
| store bytes | epub | full | 4 | 0.348 (0.173..0.651) | +/-0.239 | win |
| build us | epub | adaptive | 4 | 0.637 (0.529..0.769) | +/-0.120 | win |
| store bytes | epub | adaptive | 4 | 0.339 (0.166..0.643) | +/-0.238 | win |
| build us | epub | hist | 4 | 0.468 (0.355..0.729) | +/-0.187 | win |
| store bytes | epub | hist | 4 | 0.207 (0.091..0.459) | +/-0.184 | win |
| build us | pdf | full | 4 | 0.401 (0.176..5.082) | +/-2.453 | unresolved |
| store bytes | pdf | full | 4 | 0.769 (0.568..1.445) | +/-0.438 | unresolved |
| build us | pdf | adaptive | 4 | 0.404 (0.181..5.203) | +/-2.511 | unresolved |
| store bytes | pdf | adaptive | 4 | 0.769 (0.568..1.445) | +/-0.438 | unresolved |
| build us | pdf | hist | 4 | 0.391 (0.173..5.003) | +/-2.415 | unresolved |
| store bytes | pdf | hist | 4 | 0.566 (0.331..1.124) | +/-0.397 | unresolved |

## Lifetime composite per region — build + charged adaptation(<=Cd) + N schedule passes

Per document: VOLE `build + N x one-session`; SQLite `build + sum(ensure for capability <= Cd) + N x one-session`. The adaptation is a one-time cost and is amortized over **N schedule passes**. At N=1 the lifetime sees the schedule once (adaptation dominates the SQLite side); at N=15 (the measured rep count) the one-time terms amortize and the ratio approaches the query region. Paired VOLE/SQLite ratio; per-equivalent-observation normalization cancels in the ratio.

### Horizon N=1 schedule pass(es)

| lane | depth | docs | median (95% CI) | MDE | outcome |
|---|---|---:|---|---:|---|
| full | C0 | 12 | 0.613 (0.244..0.735) | +/-0.245 | win |
| full | C1 | 12 | 0.612 (0.244..0.735) | +/-0.245 | win |
| full | C2 | 12 | 0.610 (0.244..0.732) | +/-0.244 | win |
| full | C3 | 12 | 0.610 (0.244..0.731) | +/-0.244 | win |
| full | C4 | 12 | 0.607 (0.244..0.730) | +/-0.243 | win |
| full | C5 | 12 | 0.607 (0.244..0.728) | +/-0.242 | win |
| adaptive | C0 | 12 | 0.613 (0.246..0.762) | +/-0.258 | win |
| adaptive | C1 | 12 | 0.316 (0.129..0.395) | +/-0.133 | win |
| adaptive | C2 | 12 | 0.218 (0.087..0.281) | +/-0.097 | win |
| adaptive | C3 | 12 | 0.185 (0.081..0.230) | +/-0.074 | win |
| adaptive | C4 | 12 | 0.147 (0.063..0.184) | +/-0.061 | win |
| adaptive | C5 | 12 | 0.132 (0.060..0.159) | +/-0.050 | win |
| hist | C0 | 12 | 0.510 (0.231..0.591) | +/-0.180 | win |
| hist | C1 | 12 | 0.509 (0.230..0.590) | +/-0.180 | win |
| hist | C2 | 12 | 0.507 (0.230..0.589) | +/-0.179 | win |
| hist | C3 | 12 | 0.507 (0.230..0.589) | +/-0.180 | win |
| hist | C4 | 12 | 0.506 (0.230..0.587) | +/-0.179 | win |
| hist | C5 | 12 | 0.506 (0.230..0.587) | +/-0.179 | win |

### Horizon N=15 schedule pass(es)

| lane | depth | docs | median (95% CI) | MDE | outcome |
|---|---|---:|---|---:|---|
| full | C0 | 12 | 0.667 (0.489..1.450) | +/-0.480 | unresolved |
| full | C1 | 12 | 0.663 (0.489..1.450) | +/-0.480 | unresolved |
| full | C2 | 12 | 0.641 (0.482..1.393) | +/-0.456 | unresolved |
| full | C3 | 12 | 0.644 (0.478..1.375) | +/-0.449 | unresolved |
| full | C4 | 12 | 0.617 (0.473..1.340) | +/-0.433 | unresolved |
| full | C5 | 12 | 0.617 (0.474..1.337) | +/-0.432 | unresolved |
| adaptive | C0 | 12 | 0.672 (0.494..1.478) | +/-0.492 | unresolved |
| adaptive | C1 | 12 | 0.433 (0.292..0.959) | +/-0.334 | win |
| adaptive | C2 | 12 | 0.319 (0.206..0.719) | +/-0.257 | win |
| adaptive | C3 | 12 | 0.273 (0.194..0.624) | +/-0.215 | win |
| adaptive | C4 | 12 | 0.224 (0.152..0.514) | +/-0.181 | win |
| adaptive | C5 | 12 | 0.202 (0.146..0.465) | +/-0.160 | win |
| hist | C0 | 12 | 0.652 (0.473..1.304) | +/-0.416 | unresolved |
| hist | C1 | 12 | 0.648 (0.469..1.290) | +/-0.410 | unresolved |
| hist | C2 | 12 | 0.634 (0.468..1.253) | +/-0.392 | unresolved |
| hist | C3 | 12 | 0.634 (0.466..1.264) | +/-0.399 | unresolved |
| hist | C4 | 12 | 0.610 (0.458..1.225) | +/-0.383 | unresolved |
| hist | C5 | 12 | 0.607 (0.459..1.229) | +/-0.385 | unresolved |

## Adaptation, storage growth and reads (all charged)

| id | lane | depth | ensure us | materialized | bytes after |
|---|---|---|---:|---|---:|
| nist-pdf-0002 | adaptive | C1 | 264114 | coord,idx_blocks_doc | 655360 |
| nist-pdf-0002 | adaptive | C2 | 264529 | provenance | 663552 |
| nist-pdf-0002 | adaptive | C3 | 25659 | none | 663552 |
| nist-pdf-0002 | adaptive | C4 | 271820 | revisions | 671744 |
| nist-pdf-0002 | adaptive | C5 | 25296 | none | 671744 |
| nist-pdf-0004 | adaptive | C1 | 353517 | coord,idx_blocks_doc | 1695744 |
| nist-pdf-0004 | adaptive | C2 | 350608 | provenance | 1703936 |
| nist-pdf-0004 | adaptive | C3 | 24652 | none | 1703936 |
| nist-pdf-0004 | adaptive | C4 | 344261 | revisions | 1712128 |
| nist-pdf-0004 | adaptive | C5 | 25677 | none | 1712128 |
| nist-pdf-0016 | adaptive | C1 | 70360 | coord,idx_blocks_doc | 401408 |
| nist-pdf-0016 | adaptive | C2 | 68230 | provenance | 409600 |
| nist-pdf-0016 | adaptive | C3 | 25740 | none | 409600 |
| nist-pdf-0016 | adaptive | C4 | 70357 | revisions | 417792 |
| nist-pdf-0016 | adaptive | C5 | 25145 | none | 417792 |
| nist-pdf-0017 | adaptive | C1 | 292537 | coord,idx_blocks_doc | 1949696 |
| nist-pdf-0017 | adaptive | C2 | 289856 | provenance | 1957888 |
| nist-pdf-0017 | adaptive | C3 | 24360 | none | 1957888 |
| nist-pdf-0017 | adaptive | C4 | 288455 | revisions | 1966080 |
| nist-pdf-0017 | adaptive | C5 | 24894 | none | 1966080 |
| nist-docx-0005 | adaptive | C1 | 43000 | coord,idx_blocks_doc | 237568 |
| nist-docx-0005 | adaptive | C2 | 39996 | provenance | 262144 |
| nist-docx-0005 | adaptive | C3 | 26259 | none | 262144 |
| nist-docx-0005 | adaptive | C4 | 40787 | revisions | 270336 |
| nist-docx-0005 | adaptive | C5 | 26107 | none | 270336 |
| nist-docx-0008 | adaptive | C1 | 34443 | coord,idx_blocks_doc | 110592 |
| nist-docx-0008 | adaptive | C2 | 32356 | provenance | 118784 |
| nist-docx-0008 | adaptive | C3 | 24894 | none | 118784 |
| nist-docx-0008 | adaptive | C4 | 31368 | revisions | 126976 |
| nist-docx-0008 | adaptive | C5 | 24191 | none | 126976 |
| nist-docx-0009 | adaptive | C1 | 101816 | coord,idx_blocks_doc | 466944 |
| nist-docx-0009 | adaptive | C2 | 102499 | provenance | 475136 |
| nist-docx-0009 | adaptive | C3 | 25832 | none | 475136 |
| nist-docx-0009 | adaptive | C4 | 101616 | revisions | 483328 |
| nist-docx-0009 | adaptive | C5 | 24921 | none | 483328 |
| nist-docx-0014 | adaptive | C1 | 137589 | coord,idx_blocks_doc | 1536000 |
| nist-docx-0014 | adaptive | C2 | 136644 | provenance | 1560576 |
| nist-docx-0014 | adaptive | C3 | 26447 | none | 1560576 |
| nist-docx-0014 | adaptive | C4 | 133978 | revisions | 1568768 |
| nist-docx-0014 | adaptive | C5 | 24632 | none | 1568768 |
| nist-epub-0003 | adaptive | C1 | 49923 | coord,idx_blocks_doc | 557056 |
| nist-epub-0003 | adaptive | C2 | 40943 | provenance | 610304 |
| nist-epub-0003 | adaptive | C3 | 23533 | none | 610304 |
| nist-epub-0003 | adaptive | C4 | 39028 | revisions | 618496 |
| nist-epub-0003 | adaptive | C5 | 23475 | none | 618496 |
| nist-epub-0006 | adaptive | C1 | 36293 | coord,idx_blocks_doc | 307200 |
| nist-epub-0006 | adaptive | C2 | 33880 | provenance | 327680 |
| nist-epub-0006 | adaptive | C3 | 27037 | none | 327680 |
| nist-epub-0006 | adaptive | C4 | 35298 | revisions | 335872 |
| nist-epub-0006 | adaptive | C5 | 26482 | none | 335872 |
| nist-epub-0008 | adaptive | C1 | 29266 | coord,idx_blocks_doc | 118784 |
| nist-epub-0008 | adaptive | C2 | 29578 | provenance | 135168 |
| nist-epub-0008 | adaptive | C3 | 24689 | none | 135168 |
| nist-epub-0008 | adaptive | C4 | 29366 | revisions | 143360 |
| nist-epub-0008 | adaptive | C5 | 25096 | none | 143360 |
| nist-epub-0009 | adaptive | C1 | 57522 | coord,idx_blocks_doc | 1892352 |
| nist-epub-0009 | adaptive | C2 | 43668 | provenance | 1949696 |
| nist-epub-0009 | adaptive | C3 | 25981 | none | 1949696 |
| nist-epub-0009 | adaptive | C4 | 43323 | revisions | 1957888 |
| nist-epub-0009 | adaptive | C5 | 25454 | none | 1957888 |

VOLE charges its own derived-cache writes inside its measured session (the `observe-batch` `stats.cache_bytes_written`), so no separate VOLE adaptation row exists. The adaptive lane's `ensure` rows above are its lazy materialization, timed in full.

## Declines and answer equivalence

- equivalence results (VOLE vs the primary `full` lane, every depth, every schedule slot): capability=32, decline=96, divergent=144, observable=52, projected=72, raw=738, shape=594
- **value mismatches: 0**

| depth | lane | slots | VOLE answered | SQLite answered | common |
|---|---|---:|---:|---:|---:|
| C0 | full | 288 | 256 | 230 | 230 |
| C0 | adaptive | 288 | 256 | 230 | 230 |
| C0 | hist | 288 | 256 | 230 | 230 |
| C1 | full | 288 | 256 | 230 | 230 |
| C1 | adaptive | 288 | 256 | 230 | 230 |
| C1 | hist | 288 | 256 | 230 | 230 |
| C2 | full | 288 | 256 | 230 | 230 |
| C2 | adaptive | 288 | 256 | 230 | 230 |
| C2 | hist | 288 | 256 | 230 | 230 |
| C3 | full | 288 | 256 | 230 | 230 |
| C3 | adaptive | 288 | 256 | 230 | 230 |
| C3 | hist | 288 | 256 | 230 | 230 |
| C4 | full | 288 | 256 | 272 | 256 |
| C4 | adaptive | 288 | 256 | 272 | 256 |
| C4 | hist | 288 | 256 | 272 | 256 |
| C5 | full | 288 | 256 | 272 | 256 |
| C5 | adaptive | 288 | 256 | 272 | 256 |
| C5 | hist | 288 | 256 | 272 | 256 |

`common` = schedule observations answered by BOTH VOLE and that lane; the per-equivalent-observation cost divides each lane's session by this same number (which cancels in the paired ratio). At C0..C3 the `full`/`adaptive`/`hist` lanes decline `revision` (a C4 capability) while VOLE's depth-independent store answers it; at C4..C5 VOLE declines a few observations the SQLite lanes answer. That is a contract capability difference, recorded, not a value error.

## Exact original closure (length + SHA-256 + byte compare)

| id | fmt | lane | ok | rc | sha match | len | cmp vs source | cmp vs VOLE |
|---|---|---|---:|---:|---|---:|---|---|
| nist-pdf-0002 | pdf | vole | 1 | 0 | 182ae5d23011108fba08c3a56c5029b8a26a428480ae38ac61877f6f3365db41 | 263703 | eq | n/a |
| nist-pdf-0002 | pdf | full | 1 | 0 | 182ae5d23011108fba08c3a56c5029b8a26a428480ae38ac61877f6f3365db41 | 263703 | eq | eq |
| nist-pdf-0002 | pdf | adaptive | 1 | 0 | 182ae5d23011108fba08c3a56c5029b8a26a428480ae38ac61877f6f3365db41 | 263703 | eq | eq |
| nist-pdf-0002 | pdf | hist | 1 | 0 | 182ae5d23011108fba08c3a56c5029b8a26a428480ae38ac61877f6f3365db41 | 263703 | eq | eq |
| nist-pdf-0004 | pdf | vole | 1 | 0 | 47f0fdf31f4f2b37fb2a8fa0bf77235aad0b3daa74a6397603a619a839500a42 | 1105061 | eq | n/a |
| nist-pdf-0004 | pdf | full | 1 | 0 | 47f0fdf31f4f2b37fb2a8fa0bf77235aad0b3daa74a6397603a619a839500a42 | 1105061 | eq | eq |
| nist-pdf-0004 | pdf | adaptive | 1 | 0 | 47f0fdf31f4f2b37fb2a8fa0bf77235aad0b3daa74a6397603a619a839500a42 | 1105061 | eq | eq |
| nist-pdf-0004 | pdf | hist | 1 | 0 | 47f0fdf31f4f2b37fb2a8fa0bf77235aad0b3daa74a6397603a619a839500a42 | 1105061 | eq | eq |
| nist-pdf-0016 | pdf | vole | 1 | 0 | 942a4f929dfbd2b4af2e4e03df7f6e6377054346afd9bee346ed0ebac5db384b | 310627 | eq | n/a |
| nist-pdf-0016 | pdf | full | 1 | 0 | 942a4f929dfbd2b4af2e4e03df7f6e6377054346afd9bee346ed0ebac5db384b | 310627 | eq | eq |
| nist-pdf-0016 | pdf | adaptive | 1 | 0 | 942a4f929dfbd2b4af2e4e03df7f6e6377054346afd9bee346ed0ebac5db384b | 310627 | eq | eq |
| nist-pdf-0016 | pdf | hist | 1 | 0 | 942a4f929dfbd2b4af2e4e03df7f6e6377054346afd9bee346ed0ebac5db384b | 310627 | eq | eq |
| nist-pdf-0017 | pdf | vole | 1 | 0 | 0df0fdd676df643874adfbbf767b8508af3cc6520aff5cfef7d39428e1c63cd4 | 1466246 | eq | n/a |
| nist-pdf-0017 | pdf | full | 1 | 0 | 0df0fdd676df643874adfbbf767b8508af3cc6520aff5cfef7d39428e1c63cd4 | 1466246 | eq | eq |
| nist-pdf-0017 | pdf | adaptive | 1 | 0 | 0df0fdd676df643874adfbbf767b8508af3cc6520aff5cfef7d39428e1c63cd4 | 1466246 | eq | eq |
| nist-pdf-0017 | pdf | hist | 1 | 0 | 0df0fdd676df643874adfbbf767b8508af3cc6520aff5cfef7d39428e1c63cd4 | 1466246 | eq | eq |
| nist-docx-0005 | docx | vole | 1 | 0 | c429c2b93397b318fa14c9718afb564f9162913e1a71a0d964849a5b862a996d | 57045 | eq | n/a |
| nist-docx-0005 | docx | full | 1 | 0 | c429c2b93397b318fa14c9718afb564f9162913e1a71a0d964849a5b862a996d | 57045 | eq | eq |
| nist-docx-0005 | docx | adaptive | 1 | 0 | c429c2b93397b318fa14c9718afb564f9162913e1a71a0d964849a5b862a996d | 57045 | eq | eq |
| nist-docx-0005 | docx | hist | 1 | 0 | c429c2b93397b318fa14c9718afb564f9162913e1a71a0d964849a5b862a996d | 57045 | eq | eq |
| nist-docx-0008 | docx | vole | 1 | 0 | 3fe358f1cf866c3a6208692517e1705b86581c08bfba49000b95998bc0b581a0 | 28783 | eq | n/a |
| nist-docx-0008 | docx | full | 1 | 0 | 3fe358f1cf866c3a6208692517e1705b86581c08bfba49000b95998bc0b581a0 | 28783 | eq | eq |
| nist-docx-0008 | docx | adaptive | 1 | 0 | 3fe358f1cf866c3a6208692517e1705b86581c08bfba49000b95998bc0b581a0 | 28783 | eq | eq |
| nist-docx-0008 | docx | hist | 1 | 0 | 3fe358f1cf866c3a6208692517e1705b86581c08bfba49000b95998bc0b581a0 | 28783 | eq | eq |
| nist-docx-0009 | docx | vole | 1 | 0 | b90eb95de2c2dfcc5e91615f479ae55450d1a44ad87d4d666c3bc53c169e4ef9 | 152516 | eq | n/a |
| nist-docx-0009 | docx | full | 1 | 0 | b90eb95de2c2dfcc5e91615f479ae55450d1a44ad87d4d666c3bc53c169e4ef9 | 152516 | eq | eq |
| nist-docx-0009 | docx | adaptive | 1 | 0 | b90eb95de2c2dfcc5e91615f479ae55450d1a44ad87d4d666c3bc53c169e4ef9 | 152516 | eq | eq |
| nist-docx-0009 | docx | hist | 1 | 0 | b90eb95de2c2dfcc5e91615f479ae55450d1a44ad87d4d666c3bc53c169e4ef9 | 152516 | eq | eq |
| nist-docx-0014 | docx | vole | 1 | 0 | f3160b5c856c9f324de83c22dcc733b28874ae896413f5e8342a65c975026c2f | 948422 | eq | n/a |
| nist-docx-0014 | docx | full | 1 | 0 | f3160b5c856c9f324de83c22dcc733b28874ae896413f5e8342a65c975026c2f | 948422 | eq | eq |
| nist-docx-0014 | docx | adaptive | 1 | 0 | f3160b5c856c9f324de83c22dcc733b28874ae896413f5e8342a65c975026c2f | 948422 | eq | eq |
| nist-docx-0014 | docx | hist | 1 | 0 | f3160b5c856c9f324de83c22dcc733b28874ae896413f5e8342a65c975026c2f | 948422 | eq | eq |
| nist-epub-0003 | epub | vole | 1 | 0 | d7c59053c31dc8cd169c1d3aee746193681275cbae353695d742db0ffa7cb7c9 | 89755 | eq | n/a |
| nist-epub-0003 | epub | full | 1 | 0 | d7c59053c31dc8cd169c1d3aee746193681275cbae353695d742db0ffa7cb7c9 | 89755 | eq | eq |
| nist-epub-0003 | epub | adaptive | 1 | 0 | d7c59053c31dc8cd169c1d3aee746193681275cbae353695d742db0ffa7cb7c9 | 89755 | eq | eq |
| nist-epub-0003 | epub | hist | 1 | 0 | d7c59053c31dc8cd169c1d3aee746193681275cbae353695d742db0ffa7cb7c9 | 89755 | eq | eq |
| nist-epub-0006 | epub | vole | 1 | 0 | a3472bf6093135d5e5d5aa4cdc6a555448df77c14867b1d02a4c007f3e800def | 105867 | eq | n/a |
| nist-epub-0006 | epub | full | 1 | 0 | a3472bf6093135d5e5d5aa4cdc6a555448df77c14867b1d02a4c007f3e800def | 105867 | eq | eq |
| nist-epub-0006 | epub | adaptive | 1 | 0 | a3472bf6093135d5e5d5aa4cdc6a555448df77c14867b1d02a4c007f3e800def | 105867 | eq | eq |
| nist-epub-0006 | epub | hist | 1 | 0 | a3472bf6093135d5e5d5aa4cdc6a555448df77c14867b1d02a4c007f3e800def | 105867 | eq | eq |
| nist-epub-0008 | epub | vole | 1 | 0 | 42707b70a478a289a35146af3fb0d7ace341dae804625a160728d7589e126437 | 35841 | eq | n/a |
| nist-epub-0008 | epub | full | 1 | 0 | 42707b70a478a289a35146af3fb0d7ace341dae804625a160728d7589e126437 | 35841 | eq | eq |
| nist-epub-0008 | epub | adaptive | 1 | 0 | 42707b70a478a289a35146af3fb0d7ace341dae804625a160728d7589e126437 | 35841 | eq | eq |
| nist-epub-0008 | epub | hist | 1 | 0 | 42707b70a478a289a35146af3fb0d7ace341dae804625a160728d7589e126437 | 35841 | eq | eq |
| nist-epub-0009 | epub | vole | 1 | 0 | 1901af80bc4552842315f686062fcbd67b7534a912a3f66e2e18cff7200db475 | 1237255 | eq | n/a |
| nist-epub-0009 | epub | full | 1 | 0 | 1901af80bc4552842315f686062fcbd67b7534a912a3f66e2e18cff7200db475 | 1237255 | eq | eq |
| nist-epub-0009 | epub | adaptive | 1 | 0 | 1901af80bc4552842315f686062fcbd67b7534a912a3f66e2e18cff7200db475 | 1237255 | eq | eq |
| nist-epub-0009 | epub | hist | 1 | 0 | 1901af80bc4552842315f686062fcbd67b7534a912a3f66e2e18cff7200db475 | 1237255 | eq | eq |

- `vole`: **12/12** byte-exact (length + SHA-256 + `cmp` vs source).
- `full`: **12/12** byte-exact (length + SHA-256 + `cmp` vs source).
- `adaptive`: **12/12** byte-exact (length + SHA-256 + `cmp` vs source).
- `hist`: **12/12** byte-exact (length + SHA-256 + `cmp` vs source).

## Verdict (deterministic)

A **VOLE win** is claimed ONLY where the 95% CI of the paired median ratio excludes 1.0 in VOLE's favour (upper bound < 1.0) in a named region of the primary `full` comparator. Everything else is a recorded tie / loss / insufficient-resolution.

- **wins (6)**: full@pdf@C0, full@pdf@C1, full@pdf@C2, full@pdf@C3, full@pdf@C4, full@pdf@C5
- **losses (6)**: full@epub@C0, full@epub@C1, full@epub@C2, full@epub@C3, full@epub@C4, full@epub@C5
- **ties (0)**: none
- **insufficient resolution (12)**: full@C0, full@C1, full@C2, full@C3, full@C4, full@C5, full@docx@C0, full@docx@C1, full@docx@C2, full@docx@C3, full@docx@C4, full@docx@C5

**Verdict: MIXED[win:full@pdf@C0,full@pdf@C1,full@pdf@C2,full@pdf@C3,full@pdf@C4,full@pdf@C5][loss:full@epub@C0,full@epub@C1,full@epub@C2,full@epub@C3,full@epub@C4,full@epub@C5].**

## What this does not prove

- No production code changed and **nothing ships**; this is a measurement court on the shipping binary.
- Regions whose CI still includes 1.0 are **unresolved at this N**, never parity; the MDE column is the honest resolution floor.
- The `sqlite3` CLI exposes no sub-millisecond per-statement wall, so the **paired** comparison is the cumulative session cost per equivalent successful observation; the VOLE per-request cumulative curve (`raw/cumulative_vole.csv`) is descriptive only and unpaired.
- Persistent bytes are measured on the store directories; VOLE's derived cache (written during the schedule) and the SQLite WAL are reported separately and are NOT part of the `store bytes` axis.
- The adaptive lane is materialized once per depth before the timed reps and charged in full; its steady-state queries run on the fully adapted store (this FAVOURS the competitor).
- The hidden schedule is one pre-registered draw per document; a different seed is a different frozen schedule. A separate, clearly-labelled seed robustness check (`evidence/campaigns/*-phase22-4-lifetime-*-seedcheck/`) was run to test whether the region split survives re-drawing the schedule; it is robustness evidence, not part of this pre-registered receipt.

