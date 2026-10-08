# Phase 22.7 results — agent end-to-end cost per correct, grounded task

Branch `staging`. Measured at commit `ee8723b` (dirty tree: the Phase-22.7 change
set, recorded in the receipt). Base `main` @ `v0.1.0-alpha.28` (Phase 25).

**Verdict: `VOLE LOSS` — the ≥2× gate is not met.** VOLE costs **1.418×** the
baseline per **correct, grounded** task (VOLE **16/27** vs baseline **27/27**);
the ratio is driven by **coverage/grounding**, not token economy. The one axis the
phase cares about most — **model-input tokens** — is a **tie** where both answer
(EPUB 387 vs 379). This is the last Phase-22 subphase and a publishable negative,
consistent with the pre-registered prior (the earlier model-token tests were
mixed).

## Method (deterministic, no real LLM)

A **frozen multi-format inspection workflow** runs the **same two-step shape** on
both backends: **locate** (search → coordinates only) → **read** (unit text +
source span) → answer. A **deterministic scripted agent** (explicitly **not** an
LLM) executes it; **model-input tokens** are counted with the **pinned offline
tokenizer** (`bert-base-uncased`, `tokenizers 0.20.3`, asset SHA-256 verified on
disk) over the exact transcript fed forward — a **proxy transcript**, so real
tool-choice/generation is **not** measured.

- **Documents/tasks:** 9 `real100-v1` documents (3 pdf, 3 docx, 3 epub), **3 frozen
  tasks each = 27 tasks**; expected answer + span are derived from the document
  itself and **frozen** in the receipt (identical for both backends).
- **Backends:** VOLE (`field-build`/`find`/`observe-batch` with provenance spans)
  and a Poppler + SQLite conventional baseline given the same task-level steps.
- **Measured per task:** correct; grounded (the span is valid and backs the
  answer); tokens (pinned tokenizer); tool calls; wall; document-preparation
  compute; storage bytes; peak RSS; and **total cost per correct grounded task**
  with stated unit prices (2026-10-08, illustrative).

**Receipt.**
[`2026-10-08-phase22-7-agent-ee8723b`](../../evidence/campaigns/2026-10-08-phase22-7-agent-ee8723b/)
(service `llm-workingset`, which carries the pinned tokenizer). Court
`tools/phase22-7-agent-court.sh`.

## Cost per correct grounded task (VOLE / baseline)

| region | VOLE corr/grnd | baseline corr/grnd | tokens/task V vs B | cost/cg VOLE | cost/cg base | ratio | verdict |
|---|---|---|---|---|---|---|---|
| pdf | **0/9** | 9/9 | 167 vs 328 | undefined | $0.000993 | **∞** | VOLE loss |
| docx | 7/9 | 9/9 | 311 vs 346 | $0.001222 | $0.001041 | **1.174** | VOLE loss |
| epub | 9/9 | 9/9 | **387 vs 379** | $0.001170 | $0.001142 | **1.024** | tie |
| **pooled** | **16/27** | **27/27** | 288 vs 351 | **$0.001501** | **$0.001059** | **1.418** | **VOLE loss** |

**Gate (≥2× lower cost per correct grounded task → ≤0.5×): NOT MET** (VOLE 1.42×).

## What this does and does not prove

- **Proves (measured).** On this frozen workflow VOLE is **not** cheaper per
  correct, grounded task; it answers+grounds **16/27** vs the baseline's **27/27**.
  The pooled 1.42× is a **region declaration**: it is dominated by **PDF 0/9**,
  where VOLE's `page:1 text` observation is a bounded **heuristic** projection
  (`basis=heuristic`) that exposes **no source span**, so the answers are
  **ungrounded by construction**; DOCX loses 2 tasks (VOLE `find` searches
  paragraphs but **not tables**); **EPUB is a token tie**.
- **Does not prove** a real agent-task economy (no LLM; tokens are a proxy
  transcript; real tool-choice and generation are not measured), and the sample is
  small (9 tasks/region) and deterministic (no interval quoted). Monetary prices
  are illustrative, not a vendor's list price.
- **Not a token claim.** Where both answer, tokens are essentially equal — so
  there is **no** VOLE token advantage to claim.

## Decision

Recorded as a **negative**: no agent-cost win, and the pre-registered "keep
correctness fixed before any token claim" rule blocks any token-efficiency claim
here. The receipt points at the **cheapest concrete fixes** (recorded, not built):
emit the backing package-member span in EPUB `find` (already computed), and provide
a **PDF text source span** — the latter would remove the PDF grounding gap that
dominates the pooled loss. See [phase-22-plan.md](phase-22-plan.md) §22.7.
