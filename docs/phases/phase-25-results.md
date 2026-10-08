# Phase 25 results — durability corrections: directory ancestry + packed-header recovery

Branch `phase25`. Measured at commit `fc63436` (dirty tree: the new campaign
directories, recorded in the receipt). Base `main` @ `v0.1.0-alpha.27` (Phase 23).

Phase 23 ([ADR-0057](../adr/0057-directory-fsync-and-power-loss-proxy.md)) added
the directory `fsync` and a barrier-log power-loss model. An **external review**
found two correctness gaps that court could not see, plus a court weakness. All
three were **verified in the code before fixing** and are closed here
([ADR-0058](../adr/0058-directory-ancestry-durability-and-unpublished-segment-recovery.md)).

## The three findings (verified, then fixed)

### 1. Directory *ancestry* was not durable

`durable::create_dir_all` created directories but never synced the **parent** of a
new one. Syncing a store's leaf directory (`index/aa/bb/`) therefore did not make
`index/aa/` durable — a power cut could lose the ancestor and orphan every node
beneath it, even though the leaf was synced. The proxy model couldn't see this
either: it ignored `mkdir` events and rebuilt the directory tree by copying the
live filesystem.

**Fix.** `durable::create_dir_all` now finds the missing components, creates them
shallowest-first, and `fsync`s each new entry's parent (honoring
`DirSyncPolicy`). On the default operator-store path
(`FieldStore::open`/`open_packed`/`open_entropyfs`, `FsSeedStore`, `FsIndexStore`,
`DerivedCache`, `PromotedStore`) the raw `fs::create_dir_all` calls are replaced
by it. The proxy now folds `mkdir` into the model and prunes non-durable
directories **deepest-first**, so the whole subtree is removed with them.

### 2. The packed-header crash window

`PackWriter::ensure_open` did `create_file -> write header -> sync_dir(dir)` —
syncing the **directory before the header bytes**. A durable directory entry
could thus point at a segment whose header was lost. The Phase-23 receipt recorded
the symptom: `packed-batch-cut-record.after_body` and
`packed-batch-cut-flush.before_sync` recovered to **`fail_closed`** (a truncated
header), not to a reopen.

**Fix.** `ensure_open` now syncs the header file **before** the directory, so a
durable entry never precedes a durable header; and `scan_open_segment` treats an
incomplete/unparseable header on an **unsealed** (always unpublished) segment as
an **absent** segment — `open_write` re-creates it from scratch at the same id.
The store **reopens** and recovers the prefix; no wrong bytes are served.

### 3. The court was too lenient

`strict_verdict` accepted a **no-manifest** state that merely *failed closed*, and
`lenient_verdict` (the cut arms) ignored `manifests` and exactness entirely, so a
cut leaving an unusable published field could pass.

**Fix.** The verdicts are unified on the strict rule: a no-manifest state must
**reopen** (`reopens_ok` — prefix recovery); a published manifest must be exact
and serviceable. `lenient_verdict` is removed.

## Result

**Receipt.**
[`2026-10-08-phase23-durability-fc63436-phase25`](../../evidence/campaigns/2026-10-08-phase23-durability-fc63436-phase25/)
(same court `tools/phase23-powerloss-court.sh`, now with the ancestry model and
the strict verdicts). Service `dev`
(`mem_limit == memswap_limit == 8g`, `pids_limit 4096`). Document `nist-pdf-0002`.

**32 cases — 20 PASS / 0 FAIL / 0 shipped-arm CRITICAL**, under the **stricter**
rules. The 12 counterfactual `model-drop` arms are all CRITICAL (sensitivity);
`bad_hash` **0**.

| arm | before (Phase 23) | after (Phase 25) |
|---|---|---|
| `packed-batch-cut-record.after_body` | `fail_closed` (truncated header) | **opens**, prefix `truncated=1` |
| `packed-batch-cut-flush.before_sync` | `fail_closed` (truncated header) | **opens**, prefix `truncated=1` |
| every other cut | opens | opens |
| shipped `complete`/`safe` | PASS | PASS (unchanged) |
| `model-drop` (counterfactual) | 12 CRITICAL | 12 CRITICAL (now whole subtrees: `off` fs 827 dirs, `drop-seed` 812, `drop-index` 10) |

**Measured cost** (median of 3, default-feature binary, host ext4 bind mount):
fs store `off` **723 ms → safe 1,381 ms** (~1.91×); packed `off` **22 ms → safe
34 ms** (~1.55×); below noise on tmpfs. Phase 23 measured 717 → 1,400 / 23 → 35
with 7 reps; the ancestry syncs are within run-to-run variance at this scale.

## Gate outcomes (all in the pinned services)

| gate | rc |
|---|---:|
| `cargo fmt --all --check` | 0 |
| `cargo clippy --all-targets --all-features -- -D warnings` | 0 |
| `cargo test --locked --all-features` | 0 |
| `cargo test --locked --no-default-features` | 0 |
| MSRV `1.89.0` `cargo build --locked --all-features` | 0 |
| `cargo audit` / `cargo deny check` | 0 / 0 |
| `tools/check-compose-caps.sh` / `tools/check-docs.sh` | 0 / 0 |
| `sh tools/phase1-court.sh` | 0 (PASS) |

## Claim, and what remains unproven

**Claim.** With `DirSyncPolicy::Safe` (the default), a completed ingest's published
manifest and the nodes it references survive a modelled power loss; an
**unpublished** segment with an incomplete header is discarded so the store
**reopens and recovers the prefix**; and adding a *new* directory makes its entry
durable in its parent, so no ancestor is lost while its child is synced.

**Unproven (the residual, unchanged).** This is still a **model derived from a
barrier log, not a real power cut** — no device write cache, filesystem journal,
torn sectors, or real directory-entry loss; a physical proxy is not reproducible
in the pinned, unprivileged, hard-capped lane.

See [ADR-0058](../adr/0058-directory-ancestry-durability-and-unpublished-segment-recovery.md)
for the decision, and [ADR-0057](../adr/0057-directory-fsync-and-power-loss-proxy.md)
for the Phase-23 design this corrects.
