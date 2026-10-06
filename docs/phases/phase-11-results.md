# Phase 11 — results (11.9 fair baselines, 11.10 LLM working set, 11.11 resilience + source removal)

Branch: `phase11`. Commit under test: `63f43fb` (plus this subphase's court
re-run; no `src/`, `tests/`, `fuzz/`, or `Cargo.*` change beyond the
`observe`-JSON comma fix already in `63f43fb`).

Sealed receipt:
`evidence/campaigns/2026-10-06-phase11-63f43fb/`
(`receipt.json`, `SUMMARY.md`, `commands.txt`, `gates.txt`, `raw/`).

A **first** run of this court (against `b2658eb`) exposed a defect in the
committed field CLI: `observe`/`preview --json` omitted the comma before the
`"stats"` object, so the whole line was not valid JSON. That was fixed in
`src/main.rs` (commit `63f43fb`, this receipt): the `observe` output now parses
standalone with `jq`, and each case cross-checks the parsed `stats` object
against `explain --analyze` (they match).

Everything below was measured in the pinned Docker services; nothing ran on the
host. The field court runs in a new `db-baseline` service
(`Dockerfile` stage `FROM baseline`, same pinned base digest as `dev`/`baseline`
— `rust:1.99.0-slim-bookworm@sha256:452176…`) that adds `sqlite3`, Poppler, and
`qpdf` so one image (local id `sha256:41418aa8…`) measures every lane on one
base digest.

## What was built

| file | role |
|---|---|
| `tools/field-court.sh` | §77 headline demonstration (§11.9 + §11.11 + LLM court), JSON + `SUMMARY.md` |
| `tools/field-baseline-db.sh` | A1 fair preprocessed-SQLite baseline (`pdftotext` per page → indexed table → query) |
| `tools/field-llm-workingset.sh` | §11.10 B0/B1/V UTF-8 byte court, `context_waste_ratio` |
| `Dockerfile` / `compose.yaml` | new `db-baseline` stage/service (pinned base) |

Court inputs: two deterministic generated documents (`pdf-make-large` at 50 and
400 objects; the generator targets a fixed 32 MiB source) and one real producer
document (`evidence/corpus/phase7-producers/libreoffice-export.pdf`, 61 pages).
Each case: `encode` → `field-ingest` → fresh-process `observe`/`preview`/
`explain --analyze` → page scrub → **source removal** → fresh process re-query +
`materialize --exact` → cache clear → correctness.

## Accounting universes (ADR-0027, kept separate)

| case | pages | source B | descriptor B | seed B | index B | cache init B | cache after cold B | nodes | index nodes |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| large-50 | 50 | 33,571,029 | 17,156,472 | 49,966 | 11,788 | 40 | 4,218,977 | 255 | 3 |
| large-400 | 400 | 33,673,730 | 17,283,339 | 331,066 | 90,498 | 40 | 528,035 | 2,005 | 12 |
| producer | 61 | 74,371 | 72,910 | 61,590 | 14,648 | 40 | 50,104 | 320 | 3 |

The derived cache (universe 4) is written on the first observation of a page and
is large relative to the page text it accelerates (e.g. 4.2 MB for a 0.5 MB page
text); it is counted, and it is fully disposable (below). The descriptor
(universe 2) is the exact archival authority and is re-read in full by every
process.

## Wins

* **Exactness after source removal.** For all three cases, with the source PDF
  deleted, a new process both answered queries and `materialize --exact`d a file
  with matching length, matching SHA-256, and `cmp` byte-equality
  (`large-50` 33,571,029 B, `large-400` 33,673,730 B, producer 74,371 B).
* **Bounded procedural working set.** The instrumented seed-store `bytes_read`
  is **357 B** cold on every page of every case (50 pages, 400 pages, and the
  74 KB producer), i.e. it does not grow with page count or document size, and
  is **0 B** warm.
* **Cross-process reuse.** A repeated query in a fresh process reports
  `seed_nodes_reused = 1`, `seed_nodes_executed = 0`
  (`seed_nodes_fetched = 0`, `bytes_read = 0`).
* **Disposable cache.** After `cache --clear` (reclaiming 26,366,641 /
  3,298,225 / 305,123 B), the raw `preview` output SHA-256 is unchanged and
  observation remains correct — the cache is confirmed off-wire.
* **`observe` JSON is valid and self-consistent.** The `observe`/`preview
  --json` output now parses standalone with `jq`, and the parsed `stats` object
  is byte-identical to the corresponding `explain --analyze` counters (excluding
  `wall_micros`) for every case.

## Losses (recorded, not hidden)

* **Full descriptor reload per process.** `ObserveStats.bytes_read` counts only
  the *procedural* universe. A single cold `observe` process actually reads
  **35,686,961 B** (`large-50`), **34,767,035 B** (`large-400`), or
  **169,126 B** (producer) via `read`/`pread64` — dominated by the descriptor
  blob (read twice on a cold first touch: the observation and its promotion each
  open the field; once warm). Only the procedural field is page-bounded.
* **A1 SQLite wins the narrow-query byte court.** A page-text lookup from the
  indexed SQLite baseline reads **24,393 B** (18 syscalls) in ~1 ms, versus a
  VOLE `observe` that reloads a 17 MB descriptor. The SQLite one-time cost is
  charged in full: 400 pages extracted with `pdftotext` in **1,612 ms** into a
  **53,248 B** database.
* **A0 raw tooling reads less per page.** `pdftotext -f 1 -l 1` reads
  **136,128 B** (467 `read`/`pread64` calls) and returns 91 B of text; it also
  *mmaps* 31,922,563 B (lazy — reported separately, never folded into
  "bytes read"). `qpdf --linearize` places the end of page 1 at byte 84,937 of
  33,676,367 (**first-page prefix fraction 0.002522**).
* **VOLE is not a whole-file compressor (ADR-0017).** xz -9e on the 33.67 MB
  source is **5,691,168 B**; the best complete `.voldoc` is **17,283,279 B**
  (gzip 17,069,569, zstd 17,044,900, xz 17,126,300). All compressor results were
  round-trip verified.
* **LLM working set.** UTF-8 bytes handed to a model (tokens **not** claimed):

  | case | B0 whole doc | B1 page-local | V VOLE page text | B0/V |
  |---|---:|---:|---:|---:|
  | large-50 | 4,485 | 91 | 527,275 | 0.0085 |
  | large-400 | 35,868 | 91 | 65,913 | 0.544 |
  | producer | 122,315 | 1,933 | 62 | 1,972.8 |

  On the generated (overlapping-text) documents V is far larger than page-local
  Poppler; on the real producer document V is smaller. V is a bounded *heuristic
  text-run projection*, not Poppler reading-order extraction — the byte
  comparison is not a text-quality claim in either direction.

## Tokens

`tokens: null`, `tokens_reason`: *"no pinned offline tokenizer (no vendored
BPE/model with a recorded SHA-256); UTF-8 bytes reported only, token counts not
claimed."* No tokenizer was installed, and no "tokens saved" is claimed.

## Gates

* `docker compose run --rm --no-TTY dev sh -c 'cargo build --all-features --locked'` — OK.
* `docker compose build db-baseline` — OK.
* No `src/`, `tests/`, `fuzz/`, or `Cargo.*` file changed.

## Reproduce

```sh
docker compose run --rm --no-TTY dev sh -c 'cargo build --all-features --locked'
docker compose build db-baseline
docker compose run --rm --no-TTY --user "$(id -u):$(id -g)" db-baseline \
    sh tools/field-court.sh evidence/campaigns/2026-10-06-phase11-63f43fb
```

Environment recorded in the receipt: rustc `1.99.0 (b940084d7 2026-09-28)`,
cargo `1.99.0 (5f94df478 2026-08-27)`, `Cargo.lock` SHA-256
`25fc018b20d57fe22b704fd528cfa9f3dc64969ed7aa0345fb0cd294e913eea2`, arch
`x86_64`; oracles Poppler `22.12.0`, qpdf `11.3.0`, sqlite3 `3.40.1`, strace
`6.1`, jq `1.6`, gzip `1.12`, zstd `1.5.4`, xz `5.4.1`. Base digests:
`rust:1.99.0-slim-bookworm@sha256:452176…`,
`debian:bookworm-slim@sha256:3783cc…`.

## Honest gaps

* The two generated sizes share a fixed 32 MiB source (the generator has no
  size flag), so "document size" in the boundedness table varies by **page
  count**; the real producer document supplies the genuine source-size
  variation (74 KB vs 33 MB), and page-cold `bytes_read` is 357 B at both ends.
* Poppler's page-local path is measured on `read`/`pread64` **and** on `mmap`;
  the mmap figure is lazy and is reported separately. VOLE's cold path is not
  page-local in total process I/O because it reloads the descriptor.
* No pinned offline tokenizer was available, so no token counts are claimed.
