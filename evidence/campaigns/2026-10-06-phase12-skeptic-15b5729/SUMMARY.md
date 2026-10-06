# Phase 12.15 — independent adversarial skeptic review receipt

- **Commit under review:** `15b5729` (branch `phase12`).
- **Reviewer:** independent adversarial skeptic; did not implement any Phase-12
  mechanism; READ-ONLY on `src/`.
- **Full review:** [`docs/reviews/phase-12-skeptic-review.md`](../../../docs/reviews/phase-12-skeptic-review.md).
- **Rule:** a claim survives only if a sealed receipt proves it.

## Verdicts

**Falsified (receipt contradicts the claim, or required evidence absent):**

* F4 — "A0/A1-unsupported surfaces are excluded from the lifetime frontier" is
  false; they are scheduled and the decliner is charged 0 bytes
  (`raw/case_bytes.tsv`).
* F5 — "whole-token FTS5/BM25 … reported separately" has no measurement anywhere;
  the only measured search is `LIKE` (`lifetime-court.sh` L136).
* F6 — the default-feature build fails `cargo clippy --all-targets -- -D warnings`
  (exit 101) on four dead items in `src/field/document_format.rs` (`raw/clippy-default.txt`).
* F8 — ADR-0034's mandated post-`cache --clear`, OS-witness and CDC controls are
  absent from the 12.8 receipt; `N3` is not evaluated.
* F11 — the required ablation ladder (`A1b`, `A2`–`A10`, §106, §107) was not run;
  only A0/A1/V exist in the receipt.
* F12 — the §109/`N6` "PDF no regression" gate has no receipt at all.
* F13 (part) — the 12.14 demo is attributed to `7ac2b09`, where it did not exist;
  the demo has no sealed receipt.
* F15 — the 12.13 class tally (`29`/`10`) and the `delta` store sentence (F3.2)
  were wrong.

**Overstated:**

* F3 — the small-document crossover aggregate (`all N` is false for the PDFs'
  wall/CPU) and "large" means a ~60 KB synthetic document.
* F10 — the 12.9 "equivalence" is a self-authored, generator-defined triplet with
  circular ground truth; `metadata` is a shared name, not shared semantics.
* F16 — A0 (fresh `python3` per query) and A1's one-time build (also `python3`)
  pay interpreter startup on the measured boundary, which partly manufactures
  "VOLE beats A0" and the cold one-time win.

**Confirmed (survive as scoped):**

* F2 — DOCX/EPUB exactness after source **and** descriptor deletion, fresh
  process, length + SHA-256 + `cmp` (removal court 38/38; triplet `cmp=equal`).
* F7 — cross-document sharing is genuine exact-content identity (randomized
  resource; one content id; control shares nothing; fraction from
  executions/bytes, not source size).
* F3 (core) — the mixed lifetime headline and the recorded losses;
  the one-boundary byte accounting (process `read`/`pread64` for A0/A1/V;
  instrumented VOLE never summed); the one-time ingest-cost disclosure.
* F9 — the LLM tokenizer is pinned, offline and digest-verified (asset tracked,
  SHA-256 matches, checked before any count).
* F13 (core) — the flagship demo output is live and was reproduced here.
* F1 (core) — the 315 hostile-input assertions all pass (7 invariants × 45
  fixtures); byte-exact preservation holds.

## Gate (re-run this review)

```sh
docker compose run --rm --no-TTY dev sh -c \
 'cargo fmt --all --check && \
  cargo clippy --all-targets --all-features -- -D warnings && \
  cargo test --all-features --locked && cargo test --no-default-features'
```

**exit 0.** `fmt` clean; all-features clippy clean; all-features tests
**677 passed / 0 failed / 1 ignored**; no-default-features tests
**304 passed / 0 failed / 1 ignored**. Raw: `raw/gate.txt`
(sha256 `35618434dd373a6eda753bdea71e76f13644b7329d2da6b967f150199a37d887`).

Default-feature `cargo clippy --all-targets -- -D warnings` (the task's explicit
check) **fails, exit 101**, with four `dead_code` errors in
`src/field/document_format.rs` (`CONTENT_TYPES_MEMBER`,
`PACKAGE_RELS_MEMBER`, `OFFICE_DOCUMENT_FRAGMENT`, `contains`) — pre-existing
(added in 12.7) and not `#[cfg(feature = "package")]`-gated. Raw:
`raw/clippy-default.txt`
(sha256 `ec0fd110623dd9923505cb6328b37abd70af0d3694924fbbee140be903c466c6`).

Flagship demo re-run: exit 0, values reproduced (`raw/demo.txt`, sha256
`df53ec18e55bfdacda230f4f76416f0c1da807adb85733e35b530aac5e6c716e`).

## Go / no-go

Phase 12 (or the lifetime sub-claim) must be recorded **negative** if any of:
`N6` a Phase-11-vs-Phase-12 PDF court shows any PDF-surface regression (needed
receipt is absent); `N3` a post-`cache --clear` reuse measurement is ≈0 or ≤ the
strongest CDC (needed measurement is absent); `N5` the small-document win is
reproducible by `unzip -p` + `substr` at the same boundary (the `A3`/`A4` rungs of
the missing ladder); `N1` A1 wins the small documents at every N on the common
answered set; or the workload is real large documents, where the already-recorded
A1 wins are the negative. See the review's "Go / no-go" section.

## Doc changes made (corrections are notes/amendments, not rewrites)

* `docs/phases/phase-12-results.md` — top skeptic-correction note; 12.11 ablation
  and PDF-no-regression note; crossover aggregate corrected; `delta` store
  sentence corrected (two places); FTS bullet rescoped; "excluded from the
  frontier" corrected; 12.9 self-authored-triplet note; 12.13 class tally
  corrected to 16/22/7 and `N4` note; demo commit corrected to `15b5729`.
* `docs/phases/phase-12-plan.md` — post-review status note; §105 ablations marked
  not all run.
* `FINDINGS.md` — new §9 Phase-12 addendum (scoped verdict + corrections + open
  gates); no prior text rewritten.
* `docs/adr/0029-multi-format-authority-model.md`,
  `docs/adr/0034-cross-document-identity-sharing.md`,
  `docs/adr/0035-phase12-lifetime-benchmark.md` — post-review amendments.
