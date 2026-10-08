# Phase 18.3 — one-pass enriched authority (attach the observation index before the single serialize)

Commit `b7046f0` (branch `phase18`, dirty tree: the four source files below carry
the change). All measurement in the digest-pinned, hard-capped `doc-baseline`
service; the gate in `dev`. No court ran on the host.

## The bottleneck, and what was changed

`field-build --profile runtime` produced the exact authority in two steps:
`encode_with(…, Raw)` serialized the descriptor, then
`with_observation_index` **parsed that blob and re-serialized it whole** just to
append the ignorable `OBSERVATION_INDEX` record. For the RAW profile the object
payload is essentially the entire source, so that was one extra parse (a
source-sized copy) plus one extra serialize (another source-sized copy) on the
largest inputs.

Change:

* `src/container/descriptor.rs`: added `Descriptor::with_observation_index(limits)`
  — the op table is now a pure function of the descriptor (`Program::analyze_ops`
  + `primary_dependency`). Same fallbacks as before (already-indexed, limit
  breach, `u32` overflow → unchanged).
* `src/field/ingest.rs`: `with_observation_index(bytes)` now just parses once and
  calls the new method, then serializes — behaviour byte-identical to before.
* `src/encode/mod.rs`: added `encode_with_observation_index`, which enriches the
  forced candidate **before** the court serializes it, so the court's single
  serialize/parse/materialize round trip covers the enriched authority. If the
  enriched lane is ever refused it falls back to exactly the plain forced lane.
  Public `encode`/`encode_with` are untouched.
* `src/field/build.rs`: `build_field_with` now calls the one-pass variant.

## Byte compatibility

* `encode` is **unchanged**: the forced lane still emits no index. Witness test
  `encode::tests::forced_encode_omits_index_and_enrichment_is_byte_identical`
  asserts `parse(encode_with(Raw)).observation_index.is_none()`.
* The same test asserts `encode_with_observation_index(…) ==
  with_observation_index(encode_with(…))` **byte for byte**, so the stored
  authority is identical. Empirically the 9-doc `dir_enc_bytes` match per
  document before/after, and the 12-doc contract `v_desc_bytes` match the
  Phase-18.2 run exactly (e.g. `nist-pdf-0002` 264184, `nist-pdf-0017` 1466727).
* `Descriptor::parse` is unchanged, so descriptors produced before this change
  still decode and verify identically. Direct-court `direct-authority-decode`
  **9/9**; contract `materialize --exact` **12/12**.

## Exactness

Direct court: current **9/9**, direct field **9/9**, direct authority decode
**9/9** (length + SHA-256 + `cmp`). Contract court: VOLE `materialize --exact`
**12/12**, SQLite retained blob **12/12**. Observation schedule identical:
**53 equal, 46 decline-equal, 0 divergent** (99 total).

## Measurement

### 9-document direct-field court (A/B)

| lane | direct sum ms | direct median ms |
|---|---:|---:|
| before (18.2 HEAD) | 3723 | 524 |
| after | 3617 | 491 |
| Δ | **−2.8%** | **−6.3%** |

The untouched `current` control lane (encode + field-ingest) moved **+3.8%**
between the two runs (7800 → 8099 ms), so the wall delta is **within the noise
floor**. The robust signal is peak RSS on the large documents:
`nasa-pdf-0007` 73476 → 62032 KB (−15.6%), `nasa-epub-0006` 78408 → 67292 KB
(−14.2%), `nist-docx-0001` 19816 → 16944 KB (−14.5%).

**Honest reading:** removing two full passes over the source buys only ~3% wall;
the encode court's own serialize + parse + materialize, the Stage-B scan and the
store write still dominate. The one-pass form's clear win is one fewer
source-sized buffer.

### Contract court (C0..C5, 12 documents)

| run | VOLE build sum ms | SQLite C5 build sum ms | gap |
|---|---:|---:|---:|
| Phase 18.1 baseline | 11905 | 1644 | 7.24× |
| Phase 18.2 | 11689 | 1621 | 7.21× |
| this change | 11605 | 1544 | **7.52×** |

**The gap did not improve.** VOLE's own build sum is flat (11689 → 11605,
−0.7%); the ratio rose because SQLite's C5 sum fell 1621 → 1544 ms this run.
More importantly, the contract VOLE sum is dominated by **one document**:
`nist-pdf-0017` is 9425 of the 11605 ms (81%); the other 11 docs sum to 2180 ms.

### Isolation probe (`raw/probe-pdf0017.txt`)

`nist-pdf-0017`'s `field-build` writes **6842 seed-node files**. The same build is
**9.40 s** on the bind-mounted `/work` store but **1.39 s** into `/tmp` (identical
6842 files); `encode` is 2.62 s and `field-ingest` 1.38 s. So the dominant
contract-build term is a **small-file store-write effect on the bind mount**, not
the descriptor encode or the observation-index reserialize. This phase removed
the reserialize; the dominant term is untouched and out of this phase's scope.

## Gate

`cargo fmt --all --check` **pass**; `cargo clippy --all-targets --all-features --
-D warnings` **pass**; `cargo test --locked --all-features` **pass**;
`cargo test --locked --no-default-features` **pass**; `sh tools/phase1-court.sh`
**PASS** (8/8 `cmp=equal`).

## Residual risks

* The wall A/B is single-run and within the control lane's own variance; only the
  RSS reduction is a trustworthy signal. The change is wall-neutral, RSS-positive.
* The contract gap is dominated by `nist-pdf-0017`'s bind-mount store write; a
  phase that wants to move the gap must attack seed-store write amplification, not
  the descriptor encode.
* The observe-lane archive is byte-identical, but that identity is established by
  the witness test; if `with_observation_index` and the enriched serialize ever
  diverge, that test is the tripwire.
* `field-build`'s process is unchanged in shape: the authority is still stored and
  still verified (`ingest_verified`) against the original source.
