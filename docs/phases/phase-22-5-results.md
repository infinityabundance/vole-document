# Phase 22.5 results — remote selective materialization

Branch `staging`. Measured at commit `762c81c` (dirty tree: the Phase-22.5 change
set, recorded in the receipt). Base `main` @ `v0.1.0-alpha.28` (Phase 25).

**Verdict: `LOSS` under the labelled model** — VOLE's selective remote read
transfers a **median 3.15× more bytes** than a page-level SQLite interface on this
corpus (0 wins / 17 losses / 7 unresolved). Exact byte-range reads **tie on bytes
and win modelled latency**, and one large PDF is 0.54× — both recorded
**unresolved**, not wins. This is a **negative/limited** result, consistent with
the phase's pre-registered prior, and it is recorded as such.

## What is a model vs. real (stated up front)

- **Real (local, deterministic):** all **byte counts** (VOLE's instrumented
  per-class `descriptor/manifest/index/seed_bytes_read`; SQLite page offsets from
  `strace …pread64` with `PRAGMA mmap_size=0`); all **request counts and bytes
  served** — a real loopback `http.server` with HTTP `Range`/`206`, hit by a
  coalescing client, with the server access log cross-checked against the client;
  `materialize --exact`; per-range SHA-256 verification.
- **Model (stated):** the **geometry** mapping VOLE's class bytes onto concrete
  ranges (byte counts exact; placed as a contiguous prefix per class namespace —
  locality is an **upper bound**, so request count is a **lower bound**); the
  **latency/cost constants**; coalescing policy `gap=0`. Model id
  `remote-selective-v1`, dated **2026-10-08**: `T = N_requests·RTT +
  B_transferred/BW + C_decode` (sequential), primary `cloud-same-region`
  **RTT 25 ms / BW 100 MB/s**, sensitivity `edge-fast` 5 ms / 1000 MB/s and
  `wan-slow` 80 ms / 25 MB/s; `C_decode` = measured local decode wall.
- **No S3, no network egress.**

## Pre-registered observations

- **O1 text** — pdf `--page 1 --kind text`; docx/epub `--block 0 --kind text`.
- **O2 bytes** — `--byte-range 0..64 --kind exact`.
- **O3 resource** — docx/epub `--resource 0`; pdf `--metadata` (PDF's contract has
  no `resource` observation; the substitution is recorded).

## Result (VOLE selective vs page-level SQLite; median, primary profile)

12 docs, 36 observations, **33 answered** (3 declined: VOLE `resource` on docs
without resources, rc 6).

| region | n | VOLE B | SQL B | bytes ratio | p95 lat VOLE | p95 lat SQL | lat ratio | verdict |
|---|---:|---:|---:|---:|---:|---:|---:|---|
| all | 33 | 264,466 | 20,480 | **3.154** | 87.8 ms | 101.8 ms | 0.667 | loss |
| obs=text | 12 | 133,693 | 20,480 | 6.53 | 98.3 | 101.8 | 0.73 | loss |
| obs=bytes | 12 | 208,889 | 219,136 | **0.95** | 41.2 | 90.8 | 0.35 | unresolved |
| obs=metadata (pdf) | 4 | 708,608 | 8,192 | 86.5 | 42.0 | 26.7 | 1.26 | loss |
| obs=resource (docx/epub) | 5 | 109,995 | 12,288 | 8.95 | 76.6 | 51.9 | ≈1.0 | loss |

- **Whole-object controls:** VOLE selective is a **large fraction of the whole
  store** (`store/select` ≈ 1.10–1.28); SQLite page-level is a **tiny fraction of
  the whole `.db`** (`db/page` 2.7–145). Against the **conservative whole-`.db`**
  control VOLE *would* transfer fewer bytes (`VOLE-selective/whole-db` 0.33–0.70)
  — but a realistic page interface exists, so the page-level control decides.
- **Coalescing is real and load-bearing:** SQLite 1,567 disjoint page ranges → 90
  requests (17.4×); VOLE 100 → 55 (1.8×).
- **One near-win:** `nist-pdf-0017` text — VOLE 11,038 B (4 req) vs SQL 20,480 B →
  **0.54**; latency 1.37 → **unresolved** (just above the 0.5 threshold and
  latency not competitive). That document's descriptor carries a usable
  observation index; most documents' cold descriptor closure is ~the whole
  encoded document.

## Integrity

`materialize --exact` **12/12** (length + SHA-256; `cmp` equal 12/12). Selective
range GETs verified (206 + exact length + SHA-256 vs source slice): **VOLE 55/55,
SQLite 96/96**; server access log **223 = 223** client requests.

## What this does and does not prove

- **Proves (under the model, deterministic bytes).** With a realistic page-level
  SQLite interface, VOLE's selective remote closure is **not** a major bytes win
  on this corpus — the dominant cost is that a narrow observation's descriptor
  closure is ≈the whole encoded document for most documents. Request coalescing is
  real for both lanes.
- **Does not prove** anything about real S3 (latency/egress/models); the geometry
  placement is an upper bound on locality; page-level SQLite visibility required
  `mmap_size=0` (realistic for a remote client) and its in-process cache was left
  at default; SQLite "pays to retain the source" (the `bytes` observation reads
  the whole `source_blob` overflow chain) — fair under the equal contract, but
  named. The verdict is **control-sensitive**: against the whole-`.db` control
  VOLE wins bytes.

## Decision

Recorded as a **model-scoped negative** with one **region of interest**: large
documents whose descriptor carries an observation index are the only place VOLE's
remote closure approaches page-level selectivity. No mechanism change is
justified by this court. See [phase-22-plan.md](phase-22-plan.md) §22.5.
