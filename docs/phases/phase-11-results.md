# Phase 11 — results (11.9 fair baselines, 11.10 LLM working set, 11.11 resilience + source removal)

Branch: `phase11`. This is a **cumulative** results document: the field court
below was measured at `63f43fb`, and the later sections carry their own commit
under test (`…-llm-tokens-824faa9`, `…-lifetime-5a7edd3`, `…-desc-free-a8ad6f4`,
`…-edit-8cceaac`). No `src/`, `tests/`, `fuzz/`, or `Cargo.*` change beyond the
`observe`-JSON comma fix already in `63f43fb` and the per-subphase changes each
section names.

> **Skeptic correction note (2026-10-06, `9bd766d`).** An independent adversarial
> review ([`docs/reviews/phase-11-skeptic-review.md`](../reviews/phase-11-skeptic-review.md))
> found the *Wins* bullets below stale or overstated relative to later receipts
> and corrected them in place; the corrected text is marked "(corr.)". Nothing
> was deleted — the receipt numbers are unchanged, only their scope is fixed. The
> strongest correction: the warm "descriptor-free" byte win is an
> **overhead-only** figure, not a process-level byte win (see *Descriptor-free
> narrow observations*).

Sealed receipt (field court at `63f43fb`):
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
  **Confirmed** (receipt `length` + `SHA-256` + `cmp`, `source_removed=true`).
* **Bounded *seed* working set (corr.).** The instrumented **seed-store**
  `bytes_read` is **449 B** cold on every page of every case (50 pages, 400
  pages, and the 74 KB producer), i.e. it does not grow with page count or
  document size, and is **0 B** warm. *(Superseded number: `63f43fb` reported
  357 B; the partial-lane receipts `…-io-2e0a840` / `…-partial-b7de39d` /
  `…-desc-free-a8ad6f4` report 449 B and are the current ones. This is the
  **seed class only**: the honest total cold observation is
  **232,515–363,647 B** — descriptor record closure + manifest + index + seed —
  so "bounded" means page-closure-bounded, **not** O(1).)*
* **Cross-process reuse (corr.).** A repeated query in a fresh **OS process**
  reports `seed_nodes_reused = 1`, `seed_nodes_executed = 0`
  (`seed_nodes_fetched = 0`). The reuse is served by the **persisted derived
  cache** (universe 4), not by recomputation from the seed DAG: after
  `cache --clear` a fresh process re-executes the nodes
  (`seed_nodes_executed = 2`, `seed_nodes_fetched = 2`,
  `cache_bytes_written = 1,677,797`; `raw/…explain-after-clear.json`). The claim
  holds "with the derived cache present".
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

## Tokens (as of `63f43fb`: not claimed)

For that receipt `tokens: null`, `tokens_reason`: *"no pinned offline tokenizer
(no vendored BPE/model with a recorded SHA-256); UTF-8 bytes reported only, token
counts not claimed."* No "tokens saved" was claimed. This is **superseded** by the
pinned tokenizer court below — see *LLM token working set (pinned tokenizer)*
(receipt `2026-10-06-phase11-llm-tokens-824faa9`).

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
  variation (74 KB vs 33 MB), and the page-cold **seed-class** `bytes_read` is
  357 B at both ends in `63f43fb` (**449 B** in the current partial-lane receipts
  `…-partial-b7de39d` / `…-desc-free-a8ad6f4`; the total cold observation there is
  232–364 KB once the descriptor closure and index are charged).
* Poppler's page-local path is measured on `read`/`pread64` **and** on `mmap`;
  the mmap figure is lazy and is reported separately. VOLE's cold path is not
  page-local in total process I/O because it reloads the descriptor.
* No pinned offline tokenizer was available in `63f43fb`, so no token counts
  were claimed there. A pinned, offline tokenizer now exists and the 11.13 token
  court below reports counts (a mixed win/tie/loss, recorded honestly).

## LLM token working set (pinned tokenizer) — Phase 11.13

Sealed receipt: `evidence/campaigns/2026-10-06-phase11-llm-tokens-824faa9/`
(`receipt.json`, `SUMMARY.md`, `commands.txt`, `gates.txt`, `raw/`). Commit under
test `824faa9`.

The §11.10 court above reported UTF-8 bytes only because no pinned offline
tokenizer existed. Phase 11.13 closes that gap:

* **Tokenizer (pinned, offline).** `bert-base-uncased` (WordPiece, vocab 30522),
  loaded by the hash-pinned HuggingFace `tokenizers==0.20.3` runtime from the
  vendored asset `tools/tokenizers/bert-base-uncased.tokenizer.json`
  (466,062 B, Apache-2.0, HF revision `86b5e093…`). The asset SHA-256
  `ce64fce797c24f68df90b40a3f74f579b336a493db14bd583fd520ea0d8c9a98` is verified
  **at court time**; the full tokenizer config (model, normalizer, pre-tokenizer,
  post-processor, decoder, vocab size) is read back from the asset and printed
  next to every count. The court never touches the network. `add_special_tokens`
  is `false`, so the tokenizer's `[CLS]`/`[SEP]` (+2 per candidate) is excluded.
* **New pinned image/service.** `Dockerfile` stage `llm-workingset` (`FROM dev`,
  same base digest `rust:1.99.0-slim-bookworm@sha256:452176…`) adds Poppler, `jq`,
  `python3`, and the hash-verified `tokenizers` cp311 wheel; `compose.yaml`
  service `llm-workingset` is capped (`mem_limit`/`memswap_limit` 4g, `cpus` 4).
  Local image `sha256:28aee8f6…`.
* **Tools.** `tools/field-llm-workingset.sh` now reports bytes **and** tokens for
  B0/B1/V, the token-space ratios, and `answer_relevant_tokens` (= B1 tokens);
  `tools/llm-tokenize.py` is the small loader; `tools/llm-token-court.sh` reruns
  the court over generated and real producer documents and writes the receipt.

**No `src/`, `tests/`, `fuzz/`, `Cargo.toml`, or `Cargo.lock` change** — the
pinned tokenizer lives in the opt-in image, so the default/MSRV/all-features
Rust graph is byte-identical (`Cargo.lock` SHA-256 still `25fc018b…`).

### Measured (tokens under `bert-base-uncased`)

| case | kind | B0 bytes | B1 bytes | V bytes | B0 tokens | B1 tokens | V tokens | V vs B1 (tokens) |
|---|---|---:|---:|---:|---:|---:|---:|---|
| large-50 | generated | 4,485 | 91 | 527,275 | 2,722 | 54 | 340,182 | **loss** |
| large-400 | generated | 35,868 | 91 | 65,913 | 22,142 | 54 | 41,433 | **loss** |
| libreoffice-export | producer | 122,315 | 1,933 | 62 | 27,783 | 420 | 22 | **win** (19.1×) |
| cairo-vector | producer | 71,634 | 11,939 | 11,936 | 15,450 | 2,575 | 2,575 | tie |
| pdftex-doc | producer | 58,824 | 9,804 | 622 | 12,744 | 2,124 | 256 | **win** (8.3×) |
| reportlab-multipage | producer | 70,842 | 11,807 | 11,804 | 15,396 | 2,566 | 2,566 | tie |

**Verdict (tokens).** Against the page-local Poppler extract B1: **2 win / 2 tie /
2 loss**. Against the whole-document extract B0: **4 win / 0 tie / 2 loss**. So
VOLE *does* reduce the token working set on the real producer documents
(libreoffice, pdfTeX), ties on two vector-heavy producers, and is far larger on
the two generated (overlapping-text) documents, where its heuristic text-run
projection inflates the page answer. Any-reduction: `true`; on all tested
observations: `false`.

**Honest caveats.** Token counts are tokenizer-specific: these are
`bert-base-uncased` (WordPiece, vocab 30522) numbers and are **not** a claim
about any other model's tokenizer. B0/B1 are Poppler reading-order extracts; V is
VOLE's bounded heuristic text-run projection (`basis=heuristic`), never Poppler
reading order. A smaller V in bytes *or* tokens is a **working-set** measurement,
never a text-quality claim, and no "tokens saved" is claimed without naming this
tokenizer.

### Gates

* `docker compose build llm-workingset` — OK (image `sha256:28aee8f6…`).
* `cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings &&
  cargo test --all-features --locked && cargo test --no-default-features` — OK.
* No `src/`, `tests/`, `fuzz/`, or `Cargo.*` change; `Cargo.lock` SHA-256
  unchanged.

```sh
docker compose build llm-workingset
docker compose run --rm --no-TTY -e HOST_IMAGE_ID=<id> llm-workingset \
    sh tools/llm-token-court.sh evidence/campaigns/2026-10-06-phase11-llm-tokens-824faa9
```

## Lifetime cost (Phase 11.12 priority #7 — 1/10/100/1,000-query court)

Sealed receipt:
`evidence/campaigns/2026-10-06-phase11-lifetime-5a7edd3/`
(`receipt.json`, `SUMMARY.md`, `commands.txt`, `gates.txt`, `raw/`). Built by the
new `tools/field-lifetime-court.sh` (bash; needs `${EPOCHREALTIME}` for fork-free
microsecond wall timing), run in the pinned `db-baseline` service
(`sha256:41418aa8…`, base `rust:1.99.0-slim-bookworm@sha256:452176…`). No
`src/`, `tests/`, `fuzz/`, or `Cargo.*` file changed.

Corpus: the four `evidence/corpus/phase7-producers/*.pdf` real documents plus one
`pdf-make-large` document (`large`, 33,571,029 B / 50 pages). N ∈ {1, 10, 100,
1000}; each of the two passes answers the whole schedule in a **fresh process per
query** (process startup charged). The pre-registered page-text lane is the only
surface all three systems can answer, so it is the comparable lane; VOLE answers
an additional 5-surface set (text, structure, preview, object bytes, decoded
stream) that the baselines cannot.

Accounting (ADR-0027, four universes kept separate):

* one-time — VOLE = `encode` + `field-ingest`; A1 = per-page `pdftotext` into an
  indexed SQLite table; A0 = none. All charged in full.
* `total_wall_ms` = one-time + the sum of per-query process wall.
* `total_bytes_read` — VOLE = the instrumented `ObserveStats.bytes_read`
  (descriptor+manifest+index+seed); A0/A1 = process `read`/`pread64` total under
  `strace` (includes library reads). These are different measurements and are
  never summed together.
* CPU/peak RSS = an aggregate batch of the first N queries under `/usr/bin/time
  -v` (a single sub-millisecond query is below its 10 ms resolution).

### Crossover (warm pass) — for which N VOLE's cumulative cost first drops below each baseline

| document | src B | pages | one-time VOLE ms | one-time A1 ms | <A0 wall | <A1 wall | <A0 bytes | <A1 bytes | <A1 cpu |
|---|---:|---:|---:|---:|---|---|---|---|---|
| cairo-vector | 58,424 | 6 | 142 | 27 | 100 | **never** | 100 | 100 | 1000 |
| libreoffice-export | 74,371 | 61 | 501 | 177 | 1000 | **never** | 1000 | 100 | 1000 |
| pdftex-doc | 23,622 | 6 | 95 | 26 | 100 | 1000 | 100 | 100 | 1000 |
| reportlab-multipage | 10,906 | 6 | 55 | 24 | 100 | 1000 | 100 | 100 | 1 |
| large | 33,571,029 | 50 | 5,254 | 624 | 1000 | **never** | 1000 | **never** | **never** |

Cumulative wall at N=1000 (VOLE / A0 / A1 ms): cairo 826 / 4,371 / 789;
libreoffice 1,124 / 2,836 / 952; pdftex 702 / 4,103 / 824; reportlab 732 / 4,039 /
834; large 7,816 / 12,678 / 1,468.

**Verdict (stated plainly).**

* VOLE **crosses A0 raw tooling on wall on every document** (N=100 for the small
  documents, N=1000 for the 61-page `libreoffice-export` and the 33.5 MB `large`
  document). A0 re-parses the PDF on every query, so VOLE's one-time ingest
  amortises within the tested range.
* VOLE **crosses the preprocessed SQLite baseline (A1) on wall only on the two
  smallest documents (pdftex-doc, reportlab-multipage) at N=1000, and loses on
  cairo-vector, libreoffice-export, and large.** On `cairo-vector` VOLE is 826 ms
  vs A1 789 ms at N=1000 (a 4.7% loss); on `large` A1's per-query ~1.5 ms beats
  VOLE's ~2.6 ms because every fresh VOLE process re-reads its descriptor
  closure. **This is a recorded presence-or-absence: the A1 wall crossover does
  not exist for 3 of 5 documents.**
* VOLE **crosses A1 on bytes-read on the four small documents (N=100) but never
  on `large`** (438 MB vs 60 MB at N=1000): a narrow indexed SQLite lookup reads
  a few KB, while a VOLE observation re-reads its descriptor/index closure.
* On CPU, VOLE crosses A0 on every document; it crosses A1 on the four small
  documents at N=1000 but never on `large`. (CPU resolution is 10 ms.)

### VOLE ingest amortisation

One-time VOLE cost as a fraction of its cumulative wall at N=1000: **7.5%**
(reportlab), 13.5% (pdftex), 17.2% (cairo), **44.6%** (libreoffice, 61 pages),
**67.2%** (large) — i.e. on the large document the one-time `encode`+`ingest`
(5,254 ms) is still two-thirds of the lifetime cost at a thousand queries. The
marginal VOLE text observation is ~0.6 ms (small documents) to ~2.6 ms (large).

### VOLE multi-surface lane (VOLE alone)

Cumulative at N=1000 (cumulative wall ms / cumulative bytes read / marginal ms):
cairo 820 / 17.7 MB / 0.68; libreoffice 1,179 / 29.5 MB / 0.87; pdftex 738 /
9.0 MB / 0.64; reportlab 739 / 5.9 MB / 0.69; **large 13,671 / 3.87 GB /
36.6**. The large-document multi-surface lane is dominated by preview (~527 KB
per call) and decoded-stream (~82 KB per call) observations.

### All recorded losses

* **A1 wall not crossed** on cairo-vector, libreoffice-export, and large (above).
* **A1 bytes-read not crossed** on large; **A1 CPU not crossed** on large.
* **One-time read is 12×–57× the source**: VOLE `encode`+`ingest` reads 4,059,405,565
  B for the 33,571,029 B `large` source (encode alone 236 MB, ~7× the source);
  30.99× (cairo), 57.38× (libreoffice), 39.56× (pdftex), 51.82× (reportlab).
* **Persistent bytes**: VOLE store is 616 KB–59.4 MB vs A1's 20 KB–184 KB and the
  source (10.9 KB–33.6 MB). The derived cache (universe 4) dominates the store
  (e.g. 42.2 MB of the 59.4 MB `large` store).
* **Not a compressor** (ADR-0017): xz on the source is far smaller than the
  `.voldoc` (e.g. `large`: 5,684,044 B vs 17,156,412 B; `reportlab`: 2,124 B vs
  11,203 B). All compressor round-trips were verified.

### Honest gaps

* CPU/peak RSS are aggregate batch measurements; a single query is below
  `/usr/bin/time`'s 10 ms CPU resolution, and the wrapping shell's small constant
  CPU is included for every system.
* VOLE and A0/A1 `bytes_read` use **different definitions** (instrumented
  procedural/descriptor classes vs process `read`/`pread64` totals). A VOLE
  process-level `strace` cross-check is recorded in `raw/`, but the two scalars
  are not the same measurement.
* Only page text is a like-for-like surface; VOLE's structure/preview/object/
  decoded lanes have no conventional equivalent and are reported for VOLE alone.
* Selector discovery is a bounded scan (object 1..64, stream 1..300) run as
  query setup; it is not charged to any lifetime cost.
* The generated document is the unmodified `pdf-make-large` output (fixed ~32 MiB
  source, 50 pages); the generator has no size flag.

## Descriptor-free narrow observations (Phase 11 priority #2)

Receipt: [`evidence/campaigns/2026-10-06-phase11-desc-free-a8ad6f4/`](../../evidence/campaigns/2026-10-06-phase11-desc-free-a8ad6f4/SUMMARY.md)
(field court) and its [`lifetime/`](../../evidence/campaigns/2026-10-06-phase11-desc-free-a8ad6f4/lifetime/SUMMARY.md)
sub-receipt (lifetime court). Both courts were run unchanged; every prior
receipt is retained.

**What changed.** `observe` now tries a cache-first short-circuit before opening
the descriptor. For `Page(Text|Preview|Structure)` and `Stream(Decoded|Operators)`
with caching enabled, it resolves the selector from the **field manifest + the
hierarchical observation index only**, computes the target derived node id with
the *same* constructors `ingest`/`deepen` use, and asks the disposable derived
cache. On a hit the ordinary evaluation core still runs — against a trip-wire
`SourceServer` — so the `FieldAnswer` and every work counter are byte-identical to
the normal path while `descriptor_bytes_read` is **0**. On a miss the probe hands
the normal path the manifest it read, the index entries it resolved, and the
target's integrity-checked cache bytes, so a cold observation reads nothing
(descriptor, manifest, or index) a second time. The short-circuit is limited to
the filesystem backend's partial lane; `materialize_exact` is untouched.

**Measured (same court, same machine).** Page-1 text, `bytes_read` classes.
"Before" is receipt `2026-10-06-phase11-partial-b7de39d` (identical cold
numbers in both runs).

| case | cold desc B (before=after) | cold manifest B | cold index B | cold seed B | cold total B (before=after) |
|---|---:|---:|---:|---:|---:|
| large-50 | 354712 | 246 | 8240 | 449 | 363647 |
| large-400 | 223073 | 249 | 8744 | 449 | 232515 |
| producer | 72908 | 246 | 8282 | 449 | 81885 |

| case | warm desc B before | warm desc B after | warm manifest B | warm index B | warm seed B | warm total B before | warm total B after | warm wall µs before → after |
|---|---:|---:|---:|---:|---:|---:|---:|---|
| large-50 | 23632 | **0** | 201 | 8240 | 0 | 32073 | **8441** | 505 → 352 |
| large-400 | 182483 | **0** | 201 | 8744 | 0 | 191428 | **8945** | 777 → 99 |
| producer | 481 | **0** | 201 | 8282 | 0 | 8964 | **8483** | 57 → 56 |

A0 Poppler (`pdftotext -f 1 -l 1`, primary 33,673,730 B document): **136,128 B**
read/pread, 8 ms. A1 preprocessed SQLite: **24,393 B** read/pread, 1 ms.

**Verdict — narrow page-text byte + wall court.** For a **warm/reused** page-text
observation `descriptor_bytes_read` drops from 23,632 / 182,483 / 481 B to **0**,
and the instrumented **procedural-overhead** total (descriptor + manifest +
index + seed) is 8,441 / 8,945 / 8,483 B. *(Skeptic correction, `9bd766d`: this
8.4–8.9 KB is an **overhead-only** figure — it excludes the cached answer
payload, which the warm process physically re-reads from the derived cache.
`evidence/…-desc-free-a8ad6f4/lifetime/raw/large.vole.warm.strace` shows the warm
process reading 246 B manifest + 8,148 B index + **527,275 B cached page-text
answer** = 543,175 B `read`/`pread64`, while the instrumented `bytes_read` is
8,486 B. On the primary `large-400` case `bytes_returned` is 65,913 B, so the
warm process read is ≥ 8,945 + 65,913 ≈ **74.9 KB** — above A1's 24,393 B. The
warm **byte** court is therefore **NOT a win over A1** on `large-400` or `large`
under a like-for-like process measurement; it is an overhead/
latency observation.)* What is genuinely measured: warm `descriptor_bytes_read
== 0`; a warm **wall** win (0.099 ms vs A1 1 ms / A0 8 ms); and a warm *process*
byte win over A0 (136,128 B) and over A1 on the small producer documents
(process reads 10.4–22.2 KB vs A1 ≈ 27.8–32.9 KB per query). The **cold**
observation is a **loss**: byte-identical to before (232,515 B / 13.7 ms) and
above both baselines. The lifetime court's `large` bytes-read crossover is still
**absent** (and larger at process level) because the one-time ingest (~406 MB)
dominates within N ≤ 1,000.

**Honest losses.**

* **Cold is unchanged and loses.** A cache miss still reads the descriptor
  closure; VOLE cold (232,515 B on `large-400`) is above A1 (24,393 B) and A0
  (136,128 B). Nothing here improves the first observation.
* **The warm "total" excludes the cached answer payload (corr., `9bd766d`).**
  The 8,441–8,945 B is descriptor + manifest + index + seed only; the warm
  process also reads the derived-cache answer it returns (65,913 B on
  `large-400`, 527,275 B on `large`). A like-for-like process byte comparison vs
  A1 is therefore a **loss** on the large cases; the win over A1 survives only
  on the small producer documents and over A0 everywhere.
* **The win is reuse-only.** It needs both a warm derived cache *and* the derived
  seed chain already present; if either is absent the probe falls through.
* **Limited surface.** Only `Page(Text|Preview|Structure)` and
  `Stream(Decoded|Operators)` are short-circuited, and only on the filesystem
  backend. `ByteRange`, `Object`, `Revision`, `Stream(EncodedBytes)`, whole-
  document reads, and the EntropyFS backend still open the descriptor.
* **Manifest + index dominate the *overhead*.** 201 B manifest + 8,240–8,744 B
  index leaf = 98% of the 8,441–8,945 B non-payload overhead (the returned cached
  payload is a separate, larger read). A larger index, or a selector spread
  across more index leaves, would erode the overhead margin.
* **Not a whole-document win.** The derived cache must first be written (one
  `bytes_returned`-sized entry per observed node), and VOLE remains far larger
  than A0/A1 on cold and whole-file comparisons.
* **Dependency-cache integrity still fails closed.** The probe mirrors
  `materialize_inner`'s guard exactly: a cache error or oversized entry is a
  miss, never wrong bytes, and a poisoned cache forces recomputation.

## Immutable edit witness (Phase 11.12)

Receipt: [`evidence/campaigns/2026-10-06-phase11-edit-8cceaac/`](../../evidence/campaigns/2026-10-06-phase11-edit-8cceaac/SUMMARY.md).

The procedural field is a **document-field trajectory**, not a stack of unrelated
whole-document snapshots. `field::edit::replace_page_content` implements one
narrow, immutable, content-addressed edit: it installs caller-supplied bytes as
the decoded content of one existing page of an indexed field and mints a new
root `R1` that structurally shares all unaffected procedural state with `R0`.

**What is genuinely shared vs copied (one edit, real producer PDF).** Input:
`evidence/corpus/phase7-producers/libreoffice-export.pdf` (74,371 B, 61 pages,
sha256 `a853f8e2…`). Edit page 1's decoded content to a 54 B one-operator stream.

| quantity | value |
|---|---:|
| index entries carried forward unchanged (same key, same node id) | 318 |
| index entries replaced (the edited page) | 1 |
| index tree nodes that already existed (not rewritten) | 2 |
| index tree nodes written anew | 2 |
| new seed nodes (`Literal` + `PageContent`) | 2 |
| payload bytes newly persisted (all nodes + manifest) | 8,776 |
| descriptor bytes read by the edit | 0 |
| descriptor blobs opened by the edit (`strace`) | 0 |
| manifest bytes read by the edit | 246 |
| index bytes read by the edit | 43,688 |

Shared **by id, never rewritten**: the descriptor blob, the `DocumentExact`
root, every unaffected `PageContent`/`PdfObject`/`PdfStreamEncoded`/
`PdfStreamDecoded` node, and every index node that already existed. Newly
written: two seed nodes, the index leaf holding the edited page and the internal
spine above it, and one manifest. The edit reads **no descriptor bytes** and, as
witnessed by `strace`, opens **no descriptor blob** — no re-parse and no re-bake
of the old document.

**Exactness.** `R0` before the edit, `R0` after the edit, and `R1` all
materialize the original 74,371 bytes with matching length, matching SHA-256
(`a853f8e2…`) and `cmp`-equal bytes. The exact archive is untouched by
construction, so it is the *derived* page observation that changes: `R1` page 1
text is `VOLE EDIT WITNESS PAGE`, `R0` page 1 text is byte-identical before and
after the edit, and pages 2…61 observe byte-identically in both roots. Repeating
the identical edit yields the identical `R1` and writes zero new bytes
(idempotent, content-addressed); a different content yields a different `R1`.

**Supported subset (narrow, stated honestly).** Exactly one operation: replace
one existing page's decoded content bytes in an already-indexed filesystem field,
with new content `<= MAX_EDIT_CONTENT_BYTES` (48 KiB). The `.voldoc` descriptor
blob is **shared by content id** — neither read nor rewritten; only its 32-byte
content id (and the `DocumentExact` root id) is written into the new manifest
(`src/field/edit.rs`: *"Read the manifest only: no descriptor blob, no document
parse"*; witnessed by `descriptor_bytes_read == 0` and `strace`
`descriptor_file_opens == 0`). *(Skeptic correction, `9bd766d`: earlier wording
said "the `.voldoc` descriptor is copied verbatim", which wrongly implies a byte
copy; the receipt proves the stronger id-shared, read-free property.)* This is a
**procedural edit of a derived page projection, not a rewrite of the PDF**, and
it makes no authorial-intent claim. There is no generic editing (no
insert/delete/reorder, no object-graph or cross-reference mutation, no
re-encoding), one page per call, and the edited page loses its source byte span
and reports no content-stream object numbers in `structure`.

**Honest losses.** The edit reads the whole hierarchical index (all leaves) to
carry untouched bindings forward — **43,688 B read** against **8,776 B** written;
the read is descriptor-free but not free. Index-*node* reuse needs a multi-node
index (this producer has four; a single-leaf index necessarily rewrites its one
leaf, and reuse is then witnessed only at entry level, which the counters report
separately).

Gates for this subphase (pinned Docker `dev`): `cargo fmt --all` clean;
`cargo clippy --all-targets --all-features -- -D warnings` clean;
`cargo test --all-features --locked` and `cargo test --no-default-features` both
green (29 test-result groups each); the edit court runs and seals the receipt.
