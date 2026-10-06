# Phase 12 — independent adversarial skeptic review

- **Reviewer role:** independent adversarial skeptic (Phase 12.15), read-only on
  `src/`. The reviewer implemented none of the Phase-12 mechanisms.
- **Branch / HEAD under review:** `phase12` @ `15b5729`.
- **Scope:** try to falsify Phase 12's headline claims against the sealed receipts;
  correct over-reach in `docs/phases/phase-12-results.md`,
  `docs/phases/phase-12-plan.md`, `FINDINGS.md` and ADRs 0029–0035.
- **Review receipt:** `evidence/campaigns/2026-10-06-phase12-skeptic-15b5729/`.
- **Rule applied:** a claim survives only if a sealed receipt proves it as
  written. A prior subagent's confidence, a script's intent, or a descriptive
  comment is not evidence.

Verdicts: **confirmed** (a receipt proves the claim as scoped), **overstated**
(a receipt proves something weaker or differently bounded), **falsified** (a
receipt contradicts the claim, or the evidence a claim requires is absent).

The full gate was re-run and passed (see §Gate). The default-feature build has a
dead-code failure (§F6).

---

## Summary of verdicts

| # | Claim | Severity | Verdict |
|---|---|---|---|
| F1 | 12.13 "315/315" hostile-input safety, outcome classes | MED | **confirmed** (invariants) — **falsified count** (class tally) |
| F2 | 12.10/12.9 DOCX/EPUB `materialize == original` (len+SHA+`cmp`, source+descriptor deleted, fresh process) | HIGH | **confirmed** |
| F3 | 12.11 lifetime headline (VOLE small-doc frontier, A1 large-doc byte/wall/CPU wins) | HIGH | **overstated** (boundary + scope) |
| F4 | 12.11 "VOLE-only surfaces are excluded from the lifetime frontier" | MED | **falsified** |
| F5 | 12.11 "whole-token FTS5/BM25 … reported separately" | MED | **falsified** (no such measurement) |
| F6 | Default-feature build is clean | MED | **falsified** (dead code, clippy fails) |
| F7 | 12.8 cross-document sharing is genuine exact-content identity | HIGH | **confirmed** |
| F8 | 12.8 `retained_inverse_work_fraction` per ADR-0034 (post-`cache --clear`, OS witness, CDC baseline) | HIGH | **falsified** (controls absent) |
| F9 | 12.12 pinned offline tokenizer, working-set-only | MED | **confirmed** |
| F10 | 12.9 cross-format equivalence (96/96) | HIGH | **overstated** (self-authored, circular ground truth) |
| F11 | Plan §105–107 required ablation ladder (A1b, A2–A10, §106/§107) | HIGH | **falsified** (only A0/A1 measured) |
| F12 | §109/N6 "PDF no regression" acceptance gate | HIGH | **falsified** (no receipt at all) |
| F13 | 12.14 flagship demo output is live, not recorded | MED | **confirmed** (reproduced) — **commit attribution falsified**, no sealed receipt |
| F14 | "dictionary"-style residuals (reject vs opaque; the widened-API split) | LOW | **confirmed** (disclosed), with presentation caveats |
| F15 | Top-of-file numeric claims in `phase-12-results.md` | LOW | **falsified** (two counts/ranges wrong) |
| F16 | A0 (and A1's one-time build) pay Python interpreter startup per invocation | MED | **overstated** (runtime artifact flatters VOLE) |

> **Close-out status (2026-10-06, `0d23a02` + receipts).** Findings F5, F6, F8, F12
> and F13's sealed-receipt gap are addressed by dated amendments; no prior numbers
> are rewritten. **F12/`N6` — closed**: the PDF no-regression court
> (`evidence/campaigns/2026-10-06-phase12-pdf-noregression-0d23a02/`) compares A2 vs
> A11 on 16 corpus PDFs / 128 `explain --analyze` observations (32/32 byte-exact, 0
> regressions). **F8/`N3` — evaluated, VIOLATED**: the controls receipt
> (`evidence/campaigns/2026-10-06-phase12-share-controls-dce2705/`) adds the
> post-`cache --clear` (in-process + fresh-process) control and a raw CDC baseline;
> post-clear reuse is `0.0`, so cross-document *work* reuse is a recorded negative.
> **F5 — measured**: the FTS5 amendment
> (`evidence/campaigns/2026-10-06-phase12-fts5-amendment-22302f9/`) measures a real
> FTS5 `trigram`/`unicode61` index vs `LIKE`. **F6 — fixed**: the `package`-only
> items in `src/field/document_format.rs` are feature-gated; default-feature clippy
> is clean. **F13 — sealed**: `evidence/campaigns/2026-10-06-phase12-demo-fb8a592/`
> records a live demo run (exit `0`). F3/F4/F10/F11/F15's scoping corrections stand;
> F11's ladder and its A7/A8 "not separable" records are unchanged.

---

## F1 — Security court invariants hold; the outcome-class tally is wrong (MED)

**Claim.** `docs/phases/phase-12-results.md` L319–342: *"Result: 315/315 court
assertions passed"*, and the class table *`reject` 16, `opaque-preserve-and-decline`
29, `accept` 10*.

**Counter-evidence (measured).** The invariant half is real: every fixture has
exactly 7 assertions (`expect_{reject,accept,decline}`, `no_hang`, `no_network`,
`no_panic`, `no_path_escape`, `no_subprocess`, `opaque_exact`),
`raw/assertions.tsv` = 315 lines, all PASS, and every fixture is
`materialize --exact`-equal. The **tally is wrong**:

```text
$ awk -F'\t' 'NR>1{print $4}' evidence/...-security-33f6d04/outcomes.tsv | sort | uniq -c
      7 accept
     22 opaque-preserve-and-decline
     16 reject
```

The receipt `SUMMARY.md` prints the same `7 / 22 / 16` tally and its fixture table
has 45 rows. The results-doc figures `29 / 10` sum with `16` to **55**, not 45.

**Correction.** The results-doc class table must read `reject 16,
opaque-preserve-and-decline 22, accept 7` (total 45). "Opaque-preserve-and-decline"
is not semantic failure hidden: it is the documented §DEC-9 behaviour for a raw
ZIP whose *physical* cover is broken (the universal `field-ingest` falls back to
the byte-exact opaque PDF lane; the typed scanner rejection is asserted
separately in `tests/phase12_security.rs`). The `decline rate` over the hostile
corpus (22/45) is a hostile-input property, not a capability claim — but ADR-0035
`N4` requires a *pre-registered decline threshold*, and none exists and none is
evaluated.

---

## F2 — DOCX/EPUB exactness is genuinely verified, not a digest string (HIGH — confirmed)

**Claim.** 12.10 and 12.9: DOCX/EPUB rematerialize byte-exactly after the source
**and** descriptor are deleted, in a fresh process.

**Evidence (verified in the court script, not just the prose).**
`tools/phase12-removal-court.sh` L96–99 deletes the private source and the
`.voldoc`; L156–166 rematerializes in a new CLI process and compares
`stat -c%s` **and** `sha256sum` **and** `cmp` against a sealed oracle
(`38/38`). The triplet court independently asserts `materialize.cmp = equal` for
all three formats. The share court verifies length + SHA-256 + raw byte equality
(`examples/phase12_share_court.rs` L145–147). No normalization is applied; a
re-zipped-but-logically-equivalent file would fail `cmp`. **Confirmed.**

---

## F3 — The lifetime headline is honestly mixed but overstates two things (HIGH — overstated)

**Claim.** `phase-12-results.md` L164–179, L209–211: the crossover table and the
"small-document frontier / **larger** store on `delta`" prose.

**Counter-evidence (measured), from `…-lifetime-3eaf576/SUMMARY.md` and
`raw/case_bytes.tsv`:**

1. **The `alpha/bravo/charlie` crossover row is wrong for the PDFs.** The receipt
   gives `alpha.pdf`/`bravo.pdf`/`charlie.pdf` wall/CPU vs A0 as **N≥10**, not
   "all N": at `N=1`, `alpha.pdf` A0 wall `2.343 ms` < V `3.808 ms`
   (`raw/cumulative.jsonl`; SUMMARY "Cumulative numbers"). Only the byte metric
   is "all N" for the small PDFs.
2. **The `delta` store claim is wrong for `delta.epub`.** Receipt per-document
   table: `delta.epub` V store **599,424 B** vs A1 `.db` **819,200 B** — VOLE is
   *smaller*, contradicting "on `delta` VOLE's store … is **larger**" and "its
   store exceeds A1's `.db` on all three `delta` documents". It is larger only
   for `delta.pdf` (462 KB vs 221 KB) and `delta.docx` (902 KB vs 823 KB).

**Correction.** Scope the crossover to "wall/CPU all N for DOCX/EPUB, N≥10 for
PDF" and remove the universal "larger on `delta`" claim.

**What survives.** The genuinely mixed headline — VOLE wins the small-document
frontier and the cold one-time comparison; the source-retaining SQLite+FTS5
baseline wins the large-document byte frontier (`delta.docx` N≥10,
`delta.epub` N≥100, `delta.pdf` N≥1000) and `delta.docx`/`delta.epub` wall/CPU at
N=1000 — is supported by `raw/cumulative.jsonl` and is disclosed at L199–225 and
L282–287. The ingest cost (`VOLE's one-time read 2.3×–22.6× the source`) is
disclosed. The byte boundary is respected: per-query bytes are process
`read`+`pread64` under `strace` for **all three** systems (`case_bytes.tsv`
columns), and VOLE's instrumented `ObservedStats.bytes_read` is reported
separately and never summed (`lifetime-court.sh` L396). **The boundary claim
survives.**

**Scope caveat not in the doc.** `delta`, the "deliberately large" losing
document, is **~60 KB** source / ~160 KB extracted text
(`phase12-corpus-gen.py`); the whole 12-document corpus is 841 B–61 KB. The
"large-document frontier" is a *toy-size* claim and must be stated as such.

---

## F4 — "VOLE-only surfaces are excluded from the lifetime frontier" is false (MED — falsified)

**Claim.** `phase-12-results.md` L223–225: the surfaces `A1` cannot answer "are
excluded from the lifetime frontier".

**Counter-evidence (measured).** They are **included**. The schedule contains
`exact-member` and `native-provenance` for every document
(`tools/fixtures/phase12-lifetime-schedule.json`), the runner round-robins over
the full case list (`lifetime-court.sh` L276–287), and a declining system is
charged **0 bytes** for the case (`case_bytes.tsv`): e.g. `alpha.docx`
`exact-member` A0 `1,629,325` / A1 `32,569` / **V `0`** (V declines), while
`native-provenance` A0 `0` / A1 `0` / V `16,861` (A0/A1 decline). So the
cumulative-cost comparison sums over **different answered subsets** and rewards
declining. The net direction on `delta.docx` (where V skips a 389,886-byte read
A1 answers) actually *understates* V's large-document byte loss — i.e. it does not
manufacture the headline — but the doc's exclusion statement is contradicted by
the receipt and the frontier is not a like-for-like observation set.

**Correction.** Either exclude declined cases from both numerator and denominator
(or report the frontier on the common answered set), and state the declined-case
handling explicitly; remove "excluded".

---

## F5 — "Whole-token FTS5/BM25 … reported separately" has no receipt (MED — falsified)

**Claim.** `phase-12-results.md` L221–222.

**Counter-evidence.** `grep -ri bm25` across the whole project matches only that
one sentence. The A1 build does create `fts5` and `ANALYZE`
(`phase12-baseline.py` L364–366, L413–414), but the only search query actually
measured is `text LIKE '%…%'` (`lifetime-court.sh` L136), and the receipt contains
no FTS/BM25 number. Nothing is "reported separately". The like-for-like claim is
therefore not made, and the FTS capability is never measured.

**Correction.** Drop "reported separately"; either measure the FTS5 lane or state
that it was not measured.

---

## F6 — The default-feature build is not clean (MED — falsified)

**Claim (implied by the release posture).** The tree is warning-clean.

**Counter-evidence (measured, this review).** `cargo clippy --all-targets -- -D
warnings` (default features) exits **101**:

```text
error: constant `CONTENT_TYPES_MEMBER` is never used   src/field/document_format.rs:36
error: constant `PACKAGE_RELS_MEMBER` is never used    src/field/document_format.rs:38
error: constant `OFFICE_DOCUMENT_FRAGMENT` is never used src/field/document_format.rs:40
error: function `contains` is never used               src/field/document_format.rs:195
```

The three constants and `contains` are used only inside
`#[cfg(feature = "package")]` (`detect_zip_family`), but are not themselves
gated, so they are dead in the default build (`rans,store,field`). Raw output:
`raw/clippy-default.txt`. The **all-features** gate is clean (§Gate), which is why
this was not caught. The fix is to `#[cfg(feature = "package")]` those items (not
done here: `src/` is read-only for this review).

---

## F7 — Cross-document sharing is genuine exact-content identity (HIGH — confirmed)

**Claim.** 12.8: a byte-identical resource in a DOCX and an EPUB resolves to one
content id; the control shares nothing.

**Evidence.** `examples/phase12_share_court.rs` generates a **randomized**
resource (`raw/shared.resource.bin`, seeded, `0x4a91a4c4…`), embeds it in a DOCX
and an EPUB, and a *different* randomized resource in a structurally identical
control EPUB in a **fresh store**. Receipt `raw/metrics.json`: the shared blob id
`4def054e…` is present (`contains_node`), the EPUB reports
`shared_resource_ids:1`, `nodes_id_shared:2`, the control
`shared_resource_ids:0`/`nodes_id_shared:0`, and `warm_bytes_exact:true`. The
`retained_inverse_work_fraction` is `0.339907`, computed by
`src/field/dag.rs` L263–273 as
`(Δ node_executions + Δ input_bytes) / (cold executions + cold input_bytes)` — from
executions/bytes, **not** from source size, exactly as ADR-0034 requires. The
"no compression claim" scoping is explicit in the receipt. **Confirmed.**

---

## F8 — ADR-0034's mandated reuse controls are absent (HIGH — falsified)

**Claim (ADR-0034, plan §DEC-8/§91).** Reuse must be receipted warm **and
post-`cache --clear`**, **cross-checked with an OS-level witness**, and
**compared against a CDC baseline**; `N3` makes "reuse below strongest CDC" a
no-go.

**Counter-evidence.** None of the three exists. `grep -ri
'cache --clear|cdc|witness'` over the 12.8 court, driver, library test and receipt
returns nothing. The `0.339907` fraction is a single warm (in-process) observation
from `observe …cache=false` then `…cache=true`
(`examples/phase12_share_court.rs` L121–122). Without the post-`cache --clear`
control the fraction cannot be distinguished from a process-memory cache, and
without the CDC baseline `N3` cannot be evaluated. The 12.14 demo does run a
`--no-cache` vs warm pair (a partial, later substitute), but that is not the 12.8
court and is not the mandated control.

**Correction.** Scope the 12.8 claim to "a warm, in-process reuse observation";
either add the three controls or record `N3` as **not evaluated**.

---

## F9 — The LLM tokenizer is genuinely pinned and offline (MED — confirmed)

**Claim.** 12.12: token counts use a pinned, offline, digest-verified
`bert-base-uncased` asset.

**Evidence.** `tools/tokenizers/bert-base-uncased.tokenizer.json` is tracked (not
gitignored); its on-disk SHA-256 equals the recorded
`ce64fce797c24f68df90b40a3f74f579b336a493db14bd583fd520ea0d8c9a98`
(verified this review). `tools/phase12-llm-court.sh` L55–64 checks the digest
**before any count** and aborts otherwise; `tools/llm-tokenize.py` loads the file
with the pinned `tokenizers==0.20.3` runtime and never hits the network. The
V-vs-B1/B0 verdicts (`1/8/3` and `9/0/3`) match the per-case table. The scope
("a working-set measurement, never a text-quality claim"; `add_special_tokens=false`)
is stated. **Confirmed.**

---

## F10 — 12.9 equivalence is a self-authored, circular triplet (HIGH — overstated)

**Claim.** 12.9 "cross-format equivalence", 96/96.

**Counter-evidence.** The PDF, DOCX and EPUB and the `ground_truth.json` are all
emitted by the **same** generator, `tools/fixtures/doc-triplet-gen.py`. The
assertions check that the three adapters read back content the generator wrote;
they are not independent of the ground truth and not evidence about documents the
generator did not author. Additionally, the shared `metadata` selector is a shared
*name*, not a shared meaning: PDF returns its own `source_len`/`source_sha256`
descriptor, DOCX a story structure, EPUB a package summary (receipt table and
`phase-12-results.md` L81–90). ADR-0029 itself warns that "a shared Rust `Selector`
enum is interface reuse, not shared semantics"; the common vocabulary is real, but
`metadata` is not a common observation.

**Correction.** State that the equivalence is demonstrated **on a self-authored,
generator-defined triplet** (adapter-consistency, not third-party independence)
and that `metadata` is a shared selector name with per-format semantics. The
per-format declines (PDF heading/block/table/cell/resource/link → exit 6) and the
absent title observation are already disclosed.

**What survives.** The common selector enum *is* real (shared variants in
`src/field/observe.rs` L94–122) and the PDF lane *does* answer the common
`find`/`text`/`metadata` surface (12.9 assertions; demo `capabilities pdf` =
`["metadata","text","search-match"]`). PDF is not "only native". The claim is not
a universal AST; it is a thin shared vocabulary plus native escape hatches, as
ADR-0031 requires.

---

## F11 — The required ablation ladder was not run (HIGH — falsified)

**Claim.** `phase-12-plan.md` L138–145 and ADR-0035 declare **required**
ablations `A1b`, `A2`–`A10`, plus §106 (eager-vs-progressive) and §107
(raw-compressed-vs-decoded-persisted).

**Counter-evidence.** The lifetime receipt contains exactly **three** systems:
`a0`, `a1`, `v` (`receipt.json` `documents[].{vole,a0,a1}`; `cumulative.jsonl`
`"system"` ∈ {a0,a1,v}). No `A1b`, no Phase-11 (`A2`) field lane, no
`A3`–`A10` rungs, no eager/progressive or raw/decoded ablation exists anywhere in
`evidence/campaigns/2026-10-06-phase12-*` or `tests/`. Without them the headline
cannot attribute *which* Phase-12 mechanism produces the small-document win (is it
the ZIP layer, the progressive inversion, the indexes, or the shared store?) —
the exact attribution the plan's §105 ladder exists to provide.

**Correction.** Record `A1b`/`A2`–`A10`/§106/§107 as **not measured** and scope the
win to "the Phase-12 field as a whole vs A0/A1", not to any named mechanism.

---

## F12 — The "PDF no regression" acceptance gate has no receipt (HIGH — falsified)

**Claim.** `phase-12-plan.md` §109/L149, ADR-0029 L40–41, ADR-0035 `N6`: the
widened universal surface must not slow or change the PDF lane.

**Counter-evidence.** There is no Phase-11-vs-Phase-12 PDF comparison in the tree:
no receipt, no test, and `grep -ri 'regress'` over `tests/**` and the Phase-12
docs finds only the sentence that *states* the requirement. The lifetime court
reports `alpha.pdf`/`delta.pdf` numbers but with no Phase-11 baseline, so they
cannot show a regression or its absence. `N6` is therefore not evaluated, and the
release cannot claim the gate green.

**Correction.** Run a PDF-only no-regression court (same corpus, Phase-11 vs
Phase-12 binaries; descriptor bytes, warm bytes-read, wall) or record the gate as
open.

---

## F13 — The demo is live; its commit attribution is wrong and it has no receipt (MED)

**Claim.** `phase-12-results.md` L416–428: the demo's values were "Observed on
this tree (commit `7ac2b09`, `doc-baseline`)" and are "all live, none recorded".

**Evidence.** "Live" is **confirmed**: I ran
`docker compose run --rm --no-TTY doc-baseline sh tools/phase12-demo.sh` on
`15b5729`; it exited `0` and reproduced the doc's representative values exactly
(`raw/demo.txt`): `Cell(B7)=bravo-seven`, `descriptor=3939 manifest=306
index=3772 seed=86`, `member_decodes=2 xml_parses=2 seed_nodes_reused=2`,
`whole_source_materialized=false`, `retained_inverse_work_fraction=0.035454`,
`cmp=equal` for all three. No fabricated output.

**Counter-evidence.** The commit is **impossible**: `tools/phase12-demo.sh` and
the `--spine-item` selector do not exist at `7ac2b09` (`git ls-tree`/`git grep`);
both were added in **`15b5729`**. And there is **no sealed receipt** for the demo —
its numbers live only in prose, so they are not independently reproducible from
the evidence tree (a "claim without a sealed receipt"). The `capabilities` step
runs on the **sealed oracle copy** after deletion (script L140–149); the results
doc discloses this, but the demo's banner "no step declined" is only because the
declining PDF selectors are never invoked.

**Correction.** Attribute the demo to `15b5729`, add a receipt directory, and
restate "no step declined" as "no invoked step declined".

---

## F14 — Reject-vs-opaque and widened-API presentation (LOW — confirmed, disclosed)

The results doc honestly discloses that a raw ZIP with a broken physical cover is
routed to the opaque PDF lane by `field-ingest` (the typed `InvalidZipStructure`
rejection is a *library* property asserted in `tests/phase12_security.rs`), and
that PDF declines the widened selectors. No misrepresentation. The only caveat is
presentational: two different code paths (universal ingest vs the scanner) are
summarised in one "outcome class" table; the doc already says "Both statements
hold; neither is a substitute for the other."

---

## F15 — Minor numeric errors in the results file (LOW — falsified)

Besides F3/F1, two smaller items:

* the crossover row (F3.1);
* the `delta`-store sentence (F3.2).

Both are corrected inline with a skeptic note.

---

## F16 — Both baselines pay Python interpreter startup, which flatters VOLE (MED — overstated)

**Claim (implied).** VOLE "beats direct per-query tooling" and wins the cold
one-time comparison.

**Counter-evidence.** `A0` for DOCX/EPUB runs a **fresh `python3` process per
query** (`lifetime-court.sh` L174–177 → `phase12-baseline.py query`), so its
per-query process-read (`alpha.docx` `1,627,234 B`; `1.6 MB` every query) and its
wall are dominated by interpreter startup, not by document work. `A1`'s one-time
build is likewise a `python3` process (`23–66 ms`, `1.8–2.7 MB` — the results doc
itself says "dominated by the Python stdlib extractor's interpreter startup").
A competent native tool (or a warm interpreter) would remove most of A0's
per-query cost and much of A1's one-time cost. So "VOLE beats A0 on all N for
DOCX/EPUB" and "VOLE wins the cold one-time comparison" are partly a
language-runtime artifact, not a representation result.

**Correction.** State that A0 (and A1's one-time build) are Python-process
implementations whose startup is charged every time, and that the A1 lane — the
ADR-0035 acceptance gate — is the meaningful comparison (where VOLE loses the
large documents).

---

## Gate (re-run this review)

```
docker compose run --rm --no-TTY dev sh -c 'cargo fmt --all --check && \
  cargo clippy --all-targets --all-features -- -D warnings && \
  cargo test --all-features --locked && cargo test --no-default-features'
```

**Result: exit 0.** `fmt` clean; all-features clippy clean; `--all-features`
tests `409 + 245 + …` passed, 0 failed (2 ignored across the run); `--no-default-features`
tests `245` passed, 0 failed. Raw: `raw/gate.txt`. Default-feature
`cargo clippy --all-targets -- -D warnings` **fails (exit 101)** with the four
dead-code errors of F6 (`raw/clippy-default.txt`).

---

## Go / no-go — when Phase 12 (or a sub-claim) must be recorded negative

Per ADR-0035 (`N1`–`N6`), a negative result is a successful phase. Concretely,
this review finds the following **measured** outcomes would (further) require a
negative record, and the evidence needed to decide each is currently **missing**:

1. **`N6` — PDF regression.** If a Phase-11-vs-Phase-12 PDF court shows any PDF
   surface (descriptor bytes, warm bytes-read, wall) worse on `phase12`, record
   the widened API as a PDF regression (F12). *Not measured.* — **Resolved
   (`0d23a02`): measured, no regression; gate closed.**
2. **`N3` — reuse.** If a **post-`cache --clear`** (cold-process) rerun of the 12.8
   sharing query does not reduce work versus a fresh store, or the reduction is
   **≤ the strongest CDC baseline on the same corpus**, record cross-document reuse
   as a negative. *Not measured* (F8). — **Resolved (`dce2705`): measured; the
   post-clear fraction is `0.0`, so cross-document reuse is recorded as a negative.**
3. **`N5` — package-index-only.** If the small-document VOLE win is reproducible by
   `unzip -p` + `substr` at the same boundary, the "procedural field" adds nothing
   over a package index. *Not measured* (the `A3`/`A4` rungs of F11 would decide
   it).
4. **`N1` — empty frontier on a like-for-like set.** If, on the **common answered
   set** (declined cases removed from both numerator and denominator, F4), A1
   beats VOLE on the small documents at every N, the small-document win
   disappears.
5. **`N4` — ETL illusion.** The lifetime schedule's decline rate has no threshold;
   if a pre-registered threshold is set and exceeded, record `N4`.
6. **Large-document workload.** If the intended workload is *real* large documents
   (not the 60 KB synthetic `delta`), the A1 byte/wall/CPU wins at
   N=10–1000 already recorded for `delta.*` are the negative, and the phase must
   not headline a large-document win it does not have.

**Do not** record Phase 12 as a *positive* lifetime result beyond: "on a
self-authored 12-document corpus (841 B–61 KB), the Phase-12 field beats A0 on the
small-document frontier and beats the source-retaining SQLite+FTS5 baseline in the
cold one-time comparison, and loses to it on the large synthetic documents and on
simple lookups." Everything stronger is unsupported by the receipts.

## Honest gaps this review could not close

* Real-world PDF/DOCX/EPUB corpora (all Phase-12 corpora are locally generated).
* A Phase-11 baseline for the PDF lane (F12). — **closed (`0d23a02`).**
* A current, like-for-like search lane (F5). — **closed (`22302f9`).**
* The `N3`/`N5` controls (F8/F11). — **`N3` closed (`dce2705`); `N5` still open.**
* A sealed demo receipt (F13). — **closed (`fb8a592`).**
