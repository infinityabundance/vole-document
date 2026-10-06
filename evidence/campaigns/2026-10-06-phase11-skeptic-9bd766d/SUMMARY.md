# Phase 11.13 — skeptic review receipt

- **Role:** independent adversarial skeptic (Phase 11.13). Read-only on `src/`;
  edited docs only.
- **Branch / HEAD:** `phase11` @ `9bd766d1c6b3db50f4e71c71c8c7e5488cc263fc`.
- **Review document:** [`docs/reviews/phase-11-skeptic-review.md`](../../../docs/reviews/phase-11-skeptic-review.md).
- **Environment:** `vole-document/dev:1.99.0`, base
  `rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e`;
  rustc `1.99.0 (b940084d7 2026-09-28)`, cargo `1.99.0 (5f94df478 2026-08-27)`;
  `Cargo.lock` sha256 `25fc018b20d57fe22b704fd528cfa9f3dc64969ed7aa0345fb0cd294e913eea2`;
  arch `x86_64`. Docker only; nothing on the host.
- **Inputs read:** `docs/phases/phase-11-plan.md`, ADRs 0024–0028,
  `docs/phases/phase-11-results.md`, `FINDINGS.md` §7, and every
  `evidence/campaigns/2026-10-06-phase11-*` receipt.

## Verdicts (severity order)

| # | Claim | Verdict | Evidence |
|---|---|---|---|
| F1 | Warm "descriptor-free" 8.4–8.9 KB beats SQLite 24,393 B | **FALSIFIED** as a like-for-like byte comparison | `raw/warm-process-reads.txt`: warm process reads 543,175 B on `large` (527,275 B cached answer); `large-400` warm process ≥ 8,945 + 65,913 ≈ 74.9 KB > A1 24,393 B |
| F2 | "Bounded working set = 357 B cold" | **OVERSTATED / stale** | current seed class is 449 B (`…-partial-b7de39d`, `…-desc-free-a8ad6f4`); honest total cold is 232,515–363,647 B (descriptor closure + index) |
| F3 | "process B executes 0 nodes" | **CONFIRMED**, cache-served | real OS process; `raw` warm `seed_nodes_executed=0`; `cache --clear` re-executes (`explain-after-clear`) |
| F4 | ADR-0028 / FINDINGS §7 cite `…-share-0f3d1d3` | **not backed by a present receipt** | directory absent; commit `0f3d1d3` unreachable; sealed receipt is `…-share-d9f818a` |
| F5 | "descriptor copied verbatim" (edit) | **OVERSTATED wording** | descriptor **shared by content id**; 0 bytes read, 0 blobs opened (strace) |
| F6 | boundary/hygiene | **noted** | instrumented-vs-process bytes compared for a win; duplicate SUMMARY section; stale header commit; dirty-tree receipt |
| F7 | shareable units lose to CDC; `unique_bytes` lower bound | **CONFIRMED** (F4 pointer fixed) | `…-share-d9f818a` |
| F8 | immutable edit witness procedural-vs-copied | **CONFIRMED** (F5 wording fixed) | `…-edit-8cceaac` |
| — | exactness after source removal (len+SHA+cmp, new process) | **CONFIRMED** | `…-63f43fb/receipt.json` |
| — | pinned offline tokenizer; V is working-set only | **CONFIRMED** | `…-llm-tokens-824faa9`; asset sha256 `ce64fce797c24f68df90b40a3f74f579b336a493db14bd583fd520ea0d8c9a98` |

## Corrections applied

- `docs/phases/phase-11-results.md` — Wins bullets (449 B seed scope; reuse
  depends on the derived cache), the descriptor-free verdict (overhead-only, not
  a process byte win vs A1 on the large cases), honest-losses list, and the edit
  "copied verbatim" wording; added a cumulative/skeptic-correction header note.
- `FINDINGS.md` §7 — receipt pointer `…-share-0f3d1d3` → `…-share-d9f818a`.
- `docs/adr/0028-finer-than-object-sharing.md` — campaign path + commit
  corrected (2 places).
- `docs/adr/0027-cost-accounting.md` — correction: a cache-served observation
  must count the cached payload it re-reads; instrumented vs process `bytes
  read` scalars may not decide a win.
- `docs/adr/0025-procedural-seed-dag.md` — correction: reuse is across a real
  process boundary but is served by the persisted derived cache; a cache clear
  re-executes the nodes.

## Gate (required)

```
docker compose run --rm --no-TTY dev sh -c 'cargo fmt --all --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --all-features --locked && cargo test --no-default-features'
```

**Result: EXIT=0** — 58 `test result: ok` groups, no `FAILED`, no `error[`, no
warnings. Full log: `raw/gate.txt`.

## Claim discipline

This receipt records a *review*, not new performance claims. The one new number
it derives (warm process read bytes) is a read-only sum over the retained
lifetime straces, not a re-run; the `large-400` warm process figure is marked
**inferred** in the review (a court-time warm `strace` should be added).
