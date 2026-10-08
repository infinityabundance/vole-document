# Phase 20.2 — large-source memory architecture for the direct build

**Status: reduced and measured; exactness and wire bytes unchanged; the ~1 GiB
source now fits the 6 GiB lane cap.**

Measured at the working tree on top of `7b897ba` (branch `phase20`), release
profile, in the `doc-baseline` lane (`mem_limit == memswap_limit == 6g`,
`pids_limit 4096`, `cpus 8`). Every command and provenance field is in
`raw/environment.json`, `commands.txt`, and `receipt.json`. Baseline (pre-change)
numbers come from the identical court run against `git stash`-ed `src/` (see
`raw/before_build.tsv`, `raw/before_environment.json`, `raw/before_summary.txt`).

## The measured problem

Phase 19.3 found the direct build's peak RSS on `nasa-pdf-0001` (a 408,854,600 B
source) was **2342 MiB ≈ 6.0× source** against the 6 GiB cap. This court
reproduces it and makes the growth an explicit curve, not an assumption.

## RSS-vs-source curve (peak RSS, `/usr/bin/time -v`)

| source | bytes | before peak | before ×src | after peak | after ×src |
|---|---:|---:|---:|---:|---:|
| `nist-pdf-0017` | 1.4 MiB | 19.4 MiB | 13.9 | 20.6 MiB | 14.8 |
| `nasa-pdf-0023` | 7.2 MiB | 46.0 MiB | 6.40 | 31.8 MiB | 4.42 |
| `nasa-pdf-0022` | 18.3 MiB | 112.7 MiB | 6.16 | 76.1 MiB | 4.16 |
| `nasa-pdf-0012` | 72.3 MiB | 437.8 MiB | 6.06 | 293.3 MiB | 4.06 |
| `nasa-pdf-0024` | 160.7 MiB | 967.3 MiB | 6.02 | 646.0 MiB | 4.02 |
| `nasa-pdf-0003` | 207.6 MiB | 1248.5 MiB | 6.02 | 833.4 MiB | 4.02 |
| `nasa-pdf-0002` | 294.5 MiB | 1769.9 MiB | 6.01 | 1181.1 MiB | 4.01 |
| `nasa-pdf-0001` | 389.9 MiB | **2342.2 MiB** | **6.007** | **1562.6 MiB** | **4.008** |
| `synth-opaque-1024m` | 1.0 GiB | **rc 137 (OOM), 6113 MiB** | 5.97 | **rc 0, 4099 MiB** | **4.003** |

Least-squares fit over sources ≥ 10 MiB (`peak_KiB = C + k·source_bytes`):

| | `k` (KiB/B) | asymptotic `k` (×source) | `C` | 1 GiB prediction |
|---|---:|---:|---:|---|
| before | 0.005858 | **5.998×** | 3.4 MiB | 6.00 GiB → **DOES NOT FIT** |
| after | 0.003903 | **3.997×** | 5.3 MiB | 4.00 GiB → **FITS** |

The old curve's ratio converges to **≈6.0×**; the new one to **≈4.0×** — a
**−33%** peak-RSS reduction on large sources. Wall is unchanged within noise
(`nasa-pdf-0001` 4.7 s before and after). Small documents are fixed-overhead
dominated and are not the subject.

## Bisected dominant allocation(s)

Instrumented per-stage RSS (`VOLE_MEM_TRACE`, `raw/bisect.txt`) on
`nasa-pdf-0001` shows the whole peak is the **encode court's decode-before-commit
proof**, holding **six** source-sized buffers at once:

```text
source (fs::read)             1 S   ← held by the CLI for the whole build
descriptor object payload     1 S   ← opaque::propose(input.to_vec())
serialize pending clone       1 S   ← ObjectSource::Inline(bytes.clone()) in Descriptor::serialize
serialize output (authority)  1 S
parsed objects copy           1 S   ← Descriptor::parse(rec.payload.to_vec())
resolve_objects clone         1 S   ← materialize_with clones inline objects for the DRA
eval output                   1 S   ← program.eval builds a Vec
```

The court peaked at **6.007 S** exactly (stage trace: `after_materialize`
`hwm=2398636 KiB`). **Stage B never dominates**: after Stage A the build is at
2 S, and `build.after_ingest` leaves the court's high-water mark unchanged — the
scan/index/node-write structures are not the constraint.

## What changed (exactness-preserving)

- **`src/encode/court.rs`** — `Court::offer` now destructures the candidate and
  `drop`s its (source-sized) object payload right after `serialize()`, *before*
  the parse→materialize round trip. The tie-break `work` value is captured first,
  so candidate selection is unchanged. **−1 S.**
- **`src/materialize/mod.rs`** — new `pub(crate) materialize_in_place` (with
  `take_objects`) **moves** inline object bytes out of the parsed descriptor
  (`std::mem::take`) instead of cloning them. Same decoder, same length and
  SHA-256 checks, byte-identical output. **−1 S.**
- **`src/field/mod.rs`** — `FieldStore::ingest_verified` uses
  `materialize_in_place`, so the direct build's independent store-boundary check
  pays one fewer source-sized copy. The check itself (length + byte equality) is
  fully retained.
- **`src/field/ingest_package.rs`** — `ingest_package_direct` (DOCX/EPUB/ZIP)
  uses `materialize_in_place` for the same reason; the package path drops from
  6.32× to 5.30× at 10 MiB (`nasa-epub-0006`).

No cap was raised. No wire byte, decode path, or `encode` output changed.

## Exactness / wire evidence

- **`materialize --exact == source` (length + SHA-256 + `cmp`): 16/16** sources
  after the change (15/16 before — the 1 GiB synthetic OOM-killed at the cap).
- **`.voldoc` bytes are unchanged.** `descriptor_sha256` before vs after:
  **15 identical, 0 different** (all 15 real `real100-v1` sources in the span,
  PDF + DOCX + EPUB). The synthetic has no pre-image because the old build could
  not complete it. See `raw/wire_before.tsv` and `raw/build.tsv`.
- A focused unit test (`materialize::tests::
  materialize_in_place_matches_and_empties_inline_objects`) pins that the new
  path equals the source and fails closed if reused.

## Does ~1 GiB fit the 6 GiB cap?

**Yes.** A 1 GiB deterministic source builds rc 0 at **4099 MiB** peak (4.003×),
leaving ~2 GiB headroom; the pre-change code **OOM-killed** (rc 137) at
6113 MiB. The cap boundary is now **~1.5 GiB** (6 GiB / 3.997) rather than
~1.0 GiB.

## Residual floor

The floor with the current architecture is **4 source-sized copies**
{source, serialized authority, parsed inline object, reconstructed output}. It
cannot be reduced to 3 without a `Descriptor` whose object payloads borrow from
the authority bytes (a lifetime-parameterized descriptor) — a large, invasive
change to a core wire type that this phase does not make. A ~2 GiB source would
still not fit the 6 GiB cap. Package (DOCX/EPUB) ingest shows a higher constant
than PDF/opaque at small sizes (`nasa-epub-0006` 5.30× at 10 MiB after vs 6.32×
before); the curve's large-source slope is dominated by the PDF/opaque path.

## Gates

| gate | result |
|---|---|
| `cargo fmt --all --check` | pass |
| `cargo clippy --all-targets --all-features -- -D warnings` | pass |
| `cargo test --locked --all-features` | pass |
| `cargo test --locked --no-default-features` | pass |
| `sh tools/phase1-court.sh` (dev) | **PASS** |

## Residual risks

- The RSS-vs-source curve is a single run per source on a bind-mounted host;
  the ratio is stable (6.00–6.02 before, 4.00–4.02 after) but not interval-bounded.
- The change removes one *redundant* copy, not the second independent exactness
  check: the encode court still serializes, parses, materializes, and byte-compares
  the authority, and `ingest_verified` still materializes and byte-compares it.
- `materialize_in_place` consumes the parsed descriptor's inline objects; it is
  `pub(crate)` and both call sites use a fresh parse, but the invariant is a real
  footgun for future callers.
