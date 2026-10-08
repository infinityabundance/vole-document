# Phase 22.4 — unknown-query lifetime frontier (measurement court)

On a **pre-registered, seeded hidden schedule** the encoder never saw, per **equivalent successful observation** (session cost / |observations answered by BOTH lanes|). Declines are excluded from numerator and denominator identically. VOLE/SQLite ratio **< 1 favours VOLE**. Every adaptation is charged. **No single averaged headline is reported:** every number below is a named region.

- documents: **12**; lanes: VOLE-packed vs {full, adaptive, hist}; depths: **C0, C1, C2, C3, C4, C5**; reps: **25**; bootstrap: **20000 resamples, seed 220400**, cluster-resampled by document; tie band **+/-10%**.
- **binding prior (ADR-0046):** the Phase-15.6 adaptive-promotion experiment LOST; a VOLE win is **not** presumed likely. The pre-registered expectation is no advantage outside a narrow region, if any.
- hidden schedule: seed **220400**, length **24**, one per document (recipe frozen in `tools/fixtures/phase22-4-schedule.py`); counts:
- pooled schedule counts over 12 docs: bytes=39, doc-text=22, heading=25, metadata=44, resource=27, revision=62, table=21, text=48

## Primary frontier — VOLE vs the tuned `full` envelope, region by region

### Pooled over documents, by depth (primary `full`)

| lane | depth | docs | pairs | median (95% CI) | geomean (95% CI) | W/T/L | MDE | outcome |
|---|---|---:|---:|---|---|---|---:|---|
| full | C0 | 12 | 300 | 1.066 (0.820..1.484) | 1.119 (0.919..1.374) | 5/1/6 | +/-0.332 | unresolved |
| full | C1 | 12 | 300 | 1.072 (0.834..1.551) | 1.148 (0.942..1.405) | 5/1/6 | +/-0.359 | unresolved |
| full | C2 | 12 | 300 | 1.068 (0.806..1.389) | 1.070 (0.887..1.288) | 5/1/6 | +/-0.292 | unresolved |
| full | C3 | 12 | 300 | 1.076 (0.804..1.394) | 1.071 (0.889..1.290) | 5/1/6 | +/-0.295 | unresolved |
| full | C4 | 12 | 300 | 0.986 (0.729..1.324) | 0.991 (0.820..1.199) | 6/1/5 | +/-0.298 | unresolved |
| full | C5 | 12 | 300 | 1.001 (0.742..1.324) | 0.995 (0.820..1.203) | 5/2/5 | +/-0.291 | unresolved |

### Format `docx` (4 docs), by depth (primary `full`)

| lane | depth | docs | pairs | median (95% CI) | geomean (95% CI) | W/T/L | MDE | outcome |
|---|---|---:|---:|---|---|---|---:|---|
| full | C0 | 4 | 100 | 0.940 (0.730..1.323) | 0.992 (0.775..1.270) | 2/0/2 | +/-0.297 | unresolved |
| full | C1 | 4 | 100 | 0.998 (0.740..1.408) | 1.018 (0.794..1.304) | 2/0/2 | +/-0.334 | unresolved |
| full | C2 | 4 | 100 | 0.974 (0.649..1.275) | 0.933 (0.714..1.218) | 2/0/2 | +/-0.313 | unresolved |
| full | C3 | 4 | 100 | 0.934 (0.651..1.294) | 0.929 (0.710..1.216) | 2/0/2 | +/-0.322 | unresolved |
| full | C4 | 4 | 100 | 0.880 (0.599..1.182) | 0.865 (0.657..1.138) | 2/1/1 | +/-0.292 | unresolved |
| full | C5 | 4 | 100 | 0.882 (0.607..1.205) | 0.872 (0.659..1.155) | 2/1/1 | +/-0.299 | unresolved |

### Format `epub` (4 docs), by depth (primary `full`)

| lane | depth | docs | pairs | median (95% CI) | geomean (95% CI) | W/T/L | MDE | outcome |
|---|---|---:|---:|---|---|---|---:|---|
| full | C0 | 4 | 100 | 1.696 (1.546..1.969) | 1.724 (1.536..1.934) | 0/0/4 | +/-0.211 | loss |
| full | C1 | 4 | 100 | 1.817 (1.561..2.034) | 1.762 (1.553..1.999) | 0/0/4 | +/-0.237 | loss |
| full | C2 | 4 | 100 | 1.607 (1.416..1.759) | 1.576 (1.440..1.724) | 0/0/4 | +/-0.172 | loss |
| full | C3 | 4 | 100 | 1.633 (1.396..1.756) | 1.573 (1.426..1.736) | 0/0/4 | +/-0.180 | loss |
| full | C4 | 4 | 100 | 1.521 (1.300..1.641) | 1.480 (1.340..1.622) | 0/0/4 | +/-0.170 | loss |
| full | C5 | 4 | 100 | 1.504 (1.301..1.640) | 1.466 (1.335..1.611) | 0/0/4 | +/-0.170 | loss |

### Format `pdf` (4 docs), by depth (primary `full`)

| lane | depth | docs | pairs | median (95% CI) | geomean (95% CI) | W/T/L | MDE | outcome |
|---|---|---:|---:|---|---|---|---:|---|
| full | C0 | 4 | 100 | 0.816 (0.775..0.907) | 0.819 (0.770..0.886) | 3/1/0 | +/-0.066 | win |
| full | C1 | 4 | 100 | 0.830 (0.790..0.932) | 0.843 (0.798..0.901) | 3/1/0 | +/-0.071 | win |
| full | C2 | 4 | 100 | 0.815 (0.762..0.946) | 0.833 (0.774..0.916) | 3/1/0 | +/-0.092 | win |
| full | C3 | 4 | 100 | 0.815 (0.786..0.938) | 0.840 (0.782..0.915) | 3/1/0 | +/-0.076 | win |
| full | C4 | 4 | 100 | 0.732 (0.696..0.870) | 0.761 (0.694..0.843) | 4/0/0 | +/-0.087 | win |
| full | C5 | 4 | 100 | 0.750 (0.697..0.893) | 0.770 (0.708..0.862) | 3/1/0 | +/-0.098 | win |

### Size class `1-10MiB` (3 docs), by depth (primary `full`)

| lane | depth | docs | pairs | median (95% CI) | geomean (95% CI) | W/T/L | MDE | outcome |
|---|---|---:|---:|---|---|---|---:|---|
| full | C0 | 3 | 75 | 0.928 (0.819..1.528) | 1.051 (0.819..1.530) | 1/1/1 | +/-0.355 | unresolved |
| full | C1 | 3 | 75 | 0.973 (0.870..1.601) | 1.077 (0.848..1.573) | 1/1/1 | +/-0.365 | unresolved |
| full | C2 | 3 | 75 | 0.989 (0.820..1.563) | 1.060 (0.829..1.499) | 1/1/1 | +/-0.371 | unresolved |
| full | C3 | 3 | 75 | 0.967 (0.839..1.511) | 1.066 (0.845..1.488) | 1/1/1 | +/-0.336 | unresolved |
| full | C4 | 3 | 75 | 0.887 (0.776..1.448) | 1.002 (0.774..1.447) | 2/0/1 | +/-0.336 | unresolved |
| full | C5 | 3 | 75 | 0.922 (0.777..1.430) | 0.997 (0.764..1.412) | 1/1/1 | +/-0.327 | unresolved |

### Size class `100KiB-1MiB` (5 docs), by depth (primary `full`)

| lane | depth | docs | pairs | median (95% CI) | geomean (95% CI) | W/T/L | MDE | outcome |
|---|---|---:|---:|---|---|---|---:|---|
| full | C0 | 5 | 125 | 1.201 (0.801..1.759) | 1.128 (0.838..1.517) | 2/0/3 | +/-0.479 | unresolved |
| full | C1 | 5 | 125 | 1.209 (0.799..1.906) | 1.166 (0.869..1.564) | 2/0/3 | +/-0.554 | unresolved |
| full | C2 | 5 | 125 | 1.142 (0.779..1.704) | 1.094 (0.840..1.424) | 2/0/3 | +/-0.463 | unresolved |
| full | C3 | 5 | 125 | 1.139 (0.789..1.712) | 1.098 (0.845..1.426) | 2/0/3 | +/-0.461 | unresolved |
| full | C4 | 5 | 125 | 1.076 (0.701..1.570) | 1.004 (0.757..1.332) | 2/1/2 | +/-0.434 | unresolved |
| full | C5 | 5 | 125 | 1.081 (0.708..1.569) | 1.016 (0.769..1.342) | 2/1/2 | +/-0.431 | unresolved |

### Size class `<100KiB` (4 docs), by depth (primary `full`)

| lane | depth | docs | pairs | median (95% CI) | geomean (95% CI) | W/T/L | MDE | outcome |
|---|---|---:|---:|---|---|---|---:|---|
| full | C0 | 4 | 100 | 1.135 (0.730..1.902) | 1.161 (0.775..1.738) | 2/0/2 | +/-0.586 | unresolved |
| full | C1 | 4 | 100 | 1.119 (0.740..1.986) | 1.181 (0.794..1.755) | 2/0/2 | +/-0.623 | unresolved |
| full | C2 | 4 | 100 | 1.093 (0.649..1.689) | 1.047 (0.714..1.537) | 2/0/2 | +/-0.520 | unresolved |
| full | C3 | 4 | 100 | 1.099 (0.651..1.678) | 1.042 (0.710..1.527) | 2/0/2 | +/-0.514 | unresolved |
| full | C4 | 4 | 100 | 0.947 (0.599..1.617) | 0.967 (0.657..1.424) | 2/0/2 | +/-0.509 | unresolved |
| full | C5 | 4 | 100 | 0.847 (0.607..1.591) | 0.968 (0.659..1.422) | 2/0/2 | +/-0.492 | unresolved |

## Secondary comparators — `adaptive` (all adaptation charged) and `hist`

### Pooled over documents, by depth (secondary lanes)

| lane | depth | docs | pairs | median (95% CI) | geomean (95% CI) | W/T/L | MDE | outcome |
|---|---|---:|---:|---|---|---|---:|---|
| adaptive | C0 | 12 | 300 | 1.155 (0.865..1.642) | 1.192 (0.975..1.463) | 4/2/6 | +/-0.389 | unresolved |
| adaptive | C1 | 12 | 300 | 1.113 (0.844..1.586) | 1.164 (0.954..1.429) | 5/1/6 | +/-0.371 | unresolved |
| adaptive | C2 | 12 | 300 | 1.031 (0.809..1.427) | 1.074 (0.890..1.295) | 5/1/6 | +/-0.309 | unresolved |
| adaptive | C3 | 12 | 300 | 1.077 (0.802..1.408) | 1.072 (0.893..1.298) | 5/1/6 | +/-0.303 | unresolved |
| adaptive | C4 | 12 | 300 | 0.980 (0.730..1.345) | 0.996 (0.820..1.219) | 6/1/5 | +/-0.308 | unresolved |
| adaptive | C5 | 12 | 300 | 1.001 (0.734..1.379) | 1.005 (0.829..1.225) | 5/2/5 | +/-0.322 | unresolved |
| hist | C0 | 12 | 300 | 1.146 (0.878..1.690) | 1.230 (0.998..1.525) | 4/2/6 | +/-0.406 | unresolved |
| hist | C1 | 12 | 300 | 1.107 (0.854..1.628) | 1.193 (0.967..1.485) | 5/1/6 | +/-0.387 | unresolved |
| hist | C2 | 12 | 300 | 1.080 (0.818..1.530) | 1.130 (0.926..1.384) | 5/1/6 | +/-0.356 | unresolved |
| hist | C3 | 12 | 300 | 1.088 (0.814..1.520) | 1.126 (0.925..1.386) | 5/1/6 | +/-0.353 | unresolved |
| hist | C4 | 12 | 300 | 1.019 (0.733..1.438) | 1.040 (0.850..1.288) | 5/2/5 | +/-0.353 | unresolved |
| hist | C5 | 12 | 300 | 1.001 (0.735..1.417) | 1.038 (0.850..1.280) | 5/2/5 | +/-0.341 | unresolved |

## Frontier axes (VOLE/SQLite; <1 favours VOLE), pooled over documents

| axis | lane | docs | median (95% CI) | MDE | outcome |
|---|---|---:|---|---:|---|
| build us | full | 12 | 0.525 (0.222..2.060) | +/-0.919 | unresolved |
| store bytes | full | 12 | 0.468 (0.313..0.737) | +/-0.212 | win |
| build us | adaptive | 12 | 0.548 (0.225..2.274) | +/-1.024 | unresolved |
| store bytes | adaptive | 12 | 0.463 (0.308..0.736) | +/-0.214 | win |
| build us | hist | 12 | 0.501 (0.207..2.346) | +/-1.070 | unresolved |
| store bytes | hist | 12 | 0.287 (0.185..0.541) | +/-0.178 | win |
| peak RSS C0 | full | 12 | 1.283 (1.252..1.390) | +/-0.069 | loss |
| peak RSS C0 | adaptive | 12 | 1.317 (1.270..1.387) | +/-0.058 | loss |
| peak RSS C0 | hist | 12 | 1.319 (1.260..1.445) | +/-0.093 | loss |
| peak RSS C1 | full | 12 | 1.290 (1.254..1.353) | +/-0.050 | loss |
| peak RSS C1 | adaptive | 12 | 1.272 (1.234..1.370) | +/-0.068 | loss |
| peak RSS C1 | hist | 12 | 1.320 (1.268..1.456) | +/-0.094 | loss |
| peak RSS C2 | full | 12 | 1.246 (1.212..1.303) | +/-0.046 | loss |
| peak RSS C2 | adaptive | 12 | 1.249 (1.205..1.307) | +/-0.051 | loss |
| peak RSS C2 | hist | 12 | 1.300 (1.251..1.438) | +/-0.094 | loss |
| peak RSS C3 | full | 12 | 1.243 (1.211..1.319) | +/-0.054 | loss |
| peak RSS C3 | adaptive | 12 | 1.243 (1.214..1.301) | +/-0.044 | loss |
| peak RSS C3 | hist | 12 | 1.289 (1.258..1.388) | +/-0.065 | loss |
| peak RSS C4 | full | 12 | 1.239 (1.218..1.278) | +/-0.030 | loss |
| peak RSS C4 | adaptive | 12 | 1.249 (1.222..1.280) | +/-0.029 | loss |
| peak RSS C4 | hist | 12 | 1.342 (1.245..1.424) | +/-0.090 | loss |
| peak RSS C5 | full | 12 | 1.245 (1.205..1.325) | +/-0.060 | loss |
| peak RSS C5 | adaptive | 12 | 1.245 (1.209..1.288) | +/-0.039 | loss |
| peak RSS C5 | hist | 12 | 1.304 (1.239..1.353) | +/-0.057 | loss |

### Build and store-bytes frontier per format region

| axis | format | lane | docs | median (95% CI) | MDE | outcome |
|---|---|---|---:|---|---:|---|
| build us | docx | full | 4 | 0.375 (0.220..0.570) | +/-0.175 | win |
| store bytes | docx | full | 4 | 0.321 (0.251..0.764) | +/-0.256 | win |
| build us | docx | adaptive | 4 | 0.388 (0.224..0.569) | +/-0.173 | win |
| store bytes | docx | adaptive | 4 | 0.321 (0.244..0.762) | +/-0.259 | win |
| build us | docx | hist | 4 | 0.348 (0.201..0.542) | +/-0.171 | win |
| store bytes | docx | hist | 4 | 0.210 (0.152..0.598) | +/-0.223 | win |
| build us | epub | full | 4 | 2.060 (0.515..3.064) | +/-1.275 | unresolved |
| store bytes | epub | full | 4 | 0.348 (0.173..0.651) | +/-0.239 | win |
| build us | epub | adaptive | 4 | 2.274 (0.532..2.851) | +/-1.160 | unresolved |
| store bytes | epub | adaptive | 4 | 0.339 (0.166..0.643) | +/-0.238 | win |
| build us | epub | hist | 4 | 2.346 (0.410..2.897) | +/-1.243 | unresolved |
| store bytes | epub | hist | 4 | 0.207 (0.091..0.459) | +/-0.184 | win |
| build us | pdf | full | 4 | 0.355 (0.173..4.935) | +/-2.381 | unresolved |
| store bytes | pdf | full | 4 | 0.769 (0.568..1.445) | +/-0.438 | unresolved |
| build us | pdf | adaptive | 4 | 0.368 (0.176..4.967) | +/-2.395 | unresolved |
| store bytes | pdf | adaptive | 4 | 0.769 (0.568..1.445) | +/-0.438 | unresolved |
| build us | pdf | hist | 4 | 0.352 (0.167..4.858) | +/-2.345 | unresolved |
| store bytes | pdf | hist | 4 | 0.566 (0.331..1.124) | +/-0.397 | unresolved |

## Lifetime composite per region — build + charged adaptation(<=Cd) + N schedule passes

Per document: VOLE `build + N x one-session`; SQLite `build + sum(ensure for capability <= Cd) + N x one-session`. The adaptation is a one-time cost and is amortized over **N schedule passes**. At N=1 the lifetime sees the schedule once (adaptation dominates the SQLite side); at N=25 (the measured rep count) the one-time terms amortize and the ratio approaches the query region. Paired VOLE/SQLite ratio; per-equivalent-observation normalization cancels in the ratio.

### Horizon N=1 schedule pass(es)

| lane | depth | docs | median (95% CI) | MDE | outcome |
|---|---|---:|---|---:|---|
| full | C0 | 12 | 0.561 (0.246..2.055) | +/-0.904 | unresolved |
| full | C1 | 12 | 0.561 (0.246..2.055) | +/-0.905 | unresolved |
| full | C2 | 12 | 0.558 (0.246..2.052) | +/-0.903 | unresolved |
| full | C3 | 12 | 0.558 (0.246..2.052) | +/-0.903 | unresolved |
| full | C4 | 12 | 0.555 (0.245..2.050) | +/-0.902 | unresolved |
| full | C5 | 12 | 0.555 (0.245..2.050) | +/-0.902 | unresolved |
| adaptive | C0 | 12 | 0.572 (0.250..2.264) | +/-1.007 | unresolved |
| adaptive | C1 | 12 | 0.296 (0.129..1.437) | +/-0.654 | unresolved |
| adaptive | C2 | 12 | 0.203 (0.087..1.093) | +/-0.503 | unresolved |
| adaptive | C3 | 12 | 0.176 (0.081..1.023) | +/-0.471 | unresolved |
| adaptive | C4 | 12 | 0.139 (0.062..0.752) | +/-0.345 | win |
| adaptive | C5 | 12 | 0.124 (0.059..0.718) | +/-0.330 | win |
| hist | C0 | 12 | 0.515 (0.230..2.340) | +/-1.055 | unresolved |
| hist | C1 | 12 | 0.514 (0.230..2.340) | +/-1.055 | unresolved |
| hist | C2 | 12 | 0.514 (0.230..2.337) | +/-1.054 | unresolved |
| hist | C3 | 12 | 0.513 (0.230..2.338) | +/-1.054 | unresolved |
| hist | C4 | 12 | 0.512 (0.229..2.335) | +/-1.053 | unresolved |
| hist | C5 | 12 | 0.512 (0.229..2.335) | +/-1.053 | unresolved |

### Horizon N=25 schedule pass(es)

| lane | depth | docs | median (95% CI) | MDE | outcome |
|---|---|---:|---|---:|---|
| full | C0 | 12 | 0.685 (0.611..1.960) | +/-0.674 | unresolved |
| full | C1 | 12 | 0.688 (0.619..1.970) | +/-0.676 | unresolved |
| full | C2 | 12 | 0.664 (0.593..1.916) | +/-0.661 | unresolved |
| full | C3 | 12 | 0.662 (0.592..1.918) | +/-0.663 | unresolved |
| full | C4 | 12 | 0.635 (0.568..1.880) | +/-0.656 | unresolved |
| full | C5 | 12 | 0.636 (0.571..1.878) | +/-0.654 | unresolved |
| adaptive | C0 | 12 | 0.712 (0.633..2.099) | +/-0.733 | unresolved |
| adaptive | C1 | 12 | 0.498 (0.390..1.490) | +/-0.550 | unresolved |
| adaptive | C2 | 12 | 0.376 (0.279..1.160) | +/-0.441 | unresolved |
| adaptive | C3 | 12 | 0.329 (0.264..1.094) | +/-0.415 | unresolved |
| adaptive | C4 | 12 | 0.272 (0.208..0.827) | +/-0.310 | win |
| adaptive | C5 | 12 | 0.247 (0.198..0.793) | +/-0.297 | win |
| hist | C0 | 12 | 0.696 (0.609..2.233) | +/-0.812 | unresolved |
| hist | C1 | 12 | 0.682 (0.600..2.229) | +/-0.815 | unresolved |
| hist | C2 | 12 | 0.659 (0.591..2.188) | +/-0.798 | unresolved |
| hist | C3 | 12 | 0.657 (0.588..2.193) | +/-0.802 | unresolved |
| hist | C4 | 12 | 0.637 (0.566..2.145) | +/-0.789 | unresolved |
| hist | C5 | 12 | 0.637 (0.565..2.149) | +/-0.792 | unresolved |

## Adaptation, storage growth and reads (all charged)

| id | lane | depth | ensure us | materialized | bytes after |
|---|---|---|---:|---|---:|
| nist-pdf-0002 | adaptive | C1 | 275052 | coord,idx_blocks_doc | 655360 |
| nist-pdf-0002 | adaptive | C2 | 272065 | provenance | 663552 |
| nist-pdf-0002 | adaptive | C3 | 25620 | none | 663552 |
| nist-pdf-0002 | adaptive | C4 | 274721 | revisions | 671744 |
| nist-pdf-0002 | adaptive | C5 | 25872 | none | 671744 |
| nist-pdf-0004 | adaptive | C1 | 363003 | coord,idx_blocks_doc | 1695744 |
| nist-pdf-0004 | adaptive | C2 | 360975 | provenance | 1703936 |
| nist-pdf-0004 | adaptive | C3 | 26299 | none | 1703936 |
| nist-pdf-0004 | adaptive | C4 | 359617 | revisions | 1712128 |
| nist-pdf-0004 | adaptive | C5 | 25583 | none | 1712128 |
| nist-pdf-0016 | adaptive | C1 | 67871 | coord,idx_blocks_doc | 401408 |
| nist-pdf-0016 | adaptive | C2 | 68687 | provenance | 409600 |
| nist-pdf-0016 | adaptive | C3 | 27918 | none | 409600 |
| nist-pdf-0016 | adaptive | C4 | 68811 | revisions | 417792 |
| nist-pdf-0016 | adaptive | C5 | 25222 | none | 417792 |
| nist-pdf-0017 | adaptive | C1 | 304290 | coord,idx_blocks_doc | 1949696 |
| nist-pdf-0017 | adaptive | C2 | 303194 | provenance | 1957888 |
| nist-pdf-0017 | adaptive | C3 | 25506 | none | 1957888 |
| nist-pdf-0017 | adaptive | C4 | 304333 | revisions | 1966080 |
| nist-pdf-0017 | adaptive | C5 | 25868 | none | 1966080 |
| nist-docx-0005 | adaptive | C1 | 44862 | coord,idx_blocks_doc | 237568 |
| nist-docx-0005 | adaptive | C2 | 39372 | provenance | 262144 |
| nist-docx-0005 | adaptive | C3 | 25274 | none | 262144 |
| nist-docx-0005 | adaptive | C4 | 39687 | revisions | 270336 |
| nist-docx-0005 | adaptive | C5 | 25158 | none | 270336 |
| nist-docx-0008 | adaptive | C1 | 32912 | coord,idx_blocks_doc | 110592 |
| nist-docx-0008 | adaptive | C2 | 34562 | provenance | 118784 |
| nist-docx-0008 | adaptive | C3 | 24723 | none | 118784 |
| nist-docx-0008 | adaptive | C4 | 34099 | revisions | 126976 |
| nist-docx-0008 | adaptive | C5 | 25687 | none | 126976 |
| nist-docx-0009 | adaptive | C1 | 103831 | coord,idx_blocks_doc | 466944 |
| nist-docx-0009 | adaptive | C2 | 105298 | provenance | 475136 |
| nist-docx-0009 | adaptive | C3 | 26382 | none | 475136 |
| nist-docx-0009 | adaptive | C4 | 106545 | revisions | 483328 |
| nist-docx-0009 | adaptive | C5 | 27616 | none | 483328 |
| nist-docx-0014 | adaptive | C1 | 138122 | coord,idx_blocks_doc | 1536000 |
| nist-docx-0014 | adaptive | C2 | 133456 | provenance | 1560576 |
| nist-docx-0014 | adaptive | C3 | 25300 | none | 1560576 |
| nist-docx-0014 | adaptive | C4 | 134035 | revisions | 1568768 |
| nist-docx-0014 | adaptive | C5 | 28456 | none | 1568768 |
| nist-epub-0003 | adaptive | C1 | 50404 | coord,idx_blocks_doc | 557056 |
| nist-epub-0003 | adaptive | C2 | 42522 | provenance | 610304 |
| nist-epub-0003 | adaptive | C3 | 25264 | none | 610304 |
| nist-epub-0003 | adaptive | C4 | 38793 | revisions | 618496 |
| nist-epub-0003 | adaptive | C5 | 27765 | none | 618496 |
| nist-epub-0006 | adaptive | C1 | 105812 | coord,idx_blocks_doc | 307200 |
| nist-epub-0006 | adaptive | C2 | 81300 | provenance | 327680 |
| nist-epub-0006 | adaptive | C3 | 26110 | none | 327680 |
| nist-epub-0006 | adaptive | C4 | 181146 | revisions | 335872 |
| nist-epub-0006 | adaptive | C5 | 28130 | none | 335872 |
| nist-epub-0008 | adaptive | C1 | 126352 | coord,idx_blocks_doc | 118784 |
| nist-epub-0008 | adaptive | C2 | 106059 | provenance | 135168 |
| nist-epub-0008 | adaptive | C3 | 26128 | none | 135168 |
| nist-epub-0008 | adaptive | C4 | 102040 | revisions | 143360 |
| nist-epub-0008 | adaptive | C5 | 24994 | none | 143360 |
| nist-epub-0009 | adaptive | C1 | 107577 | coord,idx_blocks_doc | 1892352 |
| nist-epub-0009 | adaptive | C2 | 96627 | provenance | 1949696 |
| nist-epub-0009 | adaptive | C3 | 24985 | none | 1949696 |
| nist-epub-0009 | adaptive | C4 | 65829 | revisions | 1957888 |
| nist-epub-0009 | adaptive | C5 | 24389 | none | 1957888 |

VOLE charges its own derived-cache writes inside its measured session (the `observe-batch` `stats.cache_bytes_written`), so no separate VOLE adaptation row exists. The adaptive lane's `ensure` rows above are its lazy materialization, timed in full.

## Declines and answer equivalence

- equivalence results (VOLE vs the primary `full` lane, every depth, every schedule slot): capability=50, decline=120, divergent=102, observable=74, projected=24, raw=756, shape=602
- **value mismatches: 0**

| depth | lane | slots | VOLE answered | SQLite answered | common |
|---|---|---:|---:|---:|---:|
| C0 | full | 288 | 243 | 206 | 206 |
| C0 | adaptive | 288 | 243 | 206 | 206 |
| C0 | hist | 288 | 243 | 206 | 206 |
| C1 | full | 288 | 243 | 206 | 206 |
| C1 | adaptive | 288 | 243 | 206 | 206 |
| C1 | hist | 288 | 243 | 206 | 206 |
| C2 | full | 288 | 243 | 206 | 206 |
| C2 | adaptive | 288 | 243 | 206 | 206 |
| C2 | hist | 288 | 243 | 206 | 206 |
| C3 | full | 288 | 243 | 206 | 206 |
| C3 | adaptive | 288 | 243 | 206 | 206 |
| C3 | hist | 288 | 243 | 206 | 206 |
| C4 | full | 288 | 243 | 268 | 243 |
| C4 | adaptive | 288 | 243 | 268 | 243 |
| C4 | hist | 288 | 243 | 268 | 243 |
| C5 | full | 288 | 243 | 268 | 243 |
| C5 | adaptive | 288 | 243 | 268 | 243 |
| C5 | hist | 288 | 243 | 268 | 243 |

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

