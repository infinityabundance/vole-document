# Phase 11 — independent adversarial skeptic review

- **Reviewer role:** independent adversarial skeptic (Phase 11.13), read-only on
  `src/`.
- **Branch / HEAD under review:** `phase11` @ `9bd766d`.
- **Scope:** falsify Phase 11's headline claims; correct overreach in
  `docs/phases/phase-11-results.md`, `FINDINGS.md` §7 and ADRs 0024–0028.
- **Review receipt:** `evidence/campaigns/2026-10-06-phase11-skeptic-9bd766d/`.
- **Rule applied:** a claim survives only if a sealed receipt proves it.
  A subagent's confidence is not evidence.

Verdicts: **confirmed** (receipt proves the claim as scoped), **overstated**
(the receipt proves something weaker or differently bounded), **falsified**
(the receipt contradicts the claim, or the required evidence is absent).

---

## F1 — "Warm narrow observation … 8.4–8.9 KB total, beats SQLite 24,393 B" (SEVERITY: HIGH — FALSIFIED as a like-for-like byte comparison)

**Claim.** `docs/phases/phase-11-results.md` L376–388 and
`evidence/…-desc-free-a8ad6f4/SUMMARY.md` L198–205: *"the warm total (8,945 B,
0.099 ms) is below both baselines — A1's 24,393 B / 1 ms and A0's 136,128 B /
8 ms — so the warm byte court and the warm wall court are both wins."*

**Counter-evidence (measured).** The warm VOLE process physically re-reads the
cached page-text payload, but that read is excluded from `bytes_read` because it
is served by the derived cache (universe 4). The lifetime court straces the warm
VOLE observation; `evidence/…-desc-free-a8ad6f4/lifetime/raw/large.vole.warm.strace`
shows one process reading:

```text
read(3, "VOLDFLD1…", 246)        = 246     # manifest
read(3, "r\1…", 8148)            = 8148    # index leaf
read(3, "000000:000000:77400e4f…", 527275) = 527275   # cached page-text answer
```

Process `read`/`pread64` total = **543,175 B** for `large`, of which
**527,275 B** is the cached answer (`bytes_returned`). The instrumented warm
`bytes_read` is **8,486 B**. So the 8.4–8.9 KB figure is an
*overhead-excluding-the-answer* number, while A1's 24,393 B
(`tools/field-baseline-db.sh` straces the whole `sqlite3` process) *includes* its
answer. On the primary `large-400` case `bytes_returned` is 65,913 B, so the
warm process read is ≥ 8,945 + 65,913 ≈ **74.9 KB** (plus ELF/libc), which is
**above** A1's 24,393 B — the claimed win is reversed under a like-for-like
process measurement. ADR-0027 itself says "percentages with differing boundaries
are never compared", and the lifetime court's own note says the two scalars "are
not the same measurement and are never summed together" — yet the warm win is
declared by comparing exactly those two scalars.

**What survives:** (a) warm `descriptor_bytes_read == 0` is real and witnessed;
(b) the *procedural-overhead* working set (descriptor+manifest+index+seed) is
genuinely 8.4–8.9 KB, and (c) warm **wall** is a real win (99 µs vs A1 1 ms /
A0 8 ms). At process level the warm byte court is a **loss vs A1 on `large-400`
and `large`** (payload-dominated) and a still-win vs A0 (136 KB) and vs A1 on the
small producer documents (process reads 10.4–22.2 KB vs A1 ≈ 27.8–32.9 KB).

**Correction applied:** the results doc and the desc-free receipt's win bullet now
state that the 8.4–8.9 KB is an overhead-only figure; the physical warm read
includes the cached answer payload (65.9 KB on `large-400`, 527 KB on `large`);
the warm *byte* win over A1 is withdrawn on the large cases and the warm *wall*
win plus the vs-A0 win stand.

---

## F2 — "Bounded procedural working set … 357 B cold on every page" (SEVERITY: MEDIUM — OVERSTATED / STALE)

**Claim.** `docs/phases/phase-11-results.md` L61–64: *"The instrumented
seed-store `bytes_read` is **357 B** cold on every page of every case … i.e. it
does not grow with page count or document size, and is 0 B warm."*

**Counter-evidence.** 357 B is the pre-partial-lane number from receipt
`…-63f43fb`. After the measurement-boundary change (`…-io-2e0a840`, `…-partial-b7de39d`,
`…-desc-free-a8ad6f4`) the seed class is **449 B**, and — the material point —
the *observation's* working set is no longer the seed class at all: the cold
`bytes_read` (descriptor closure + manifest + index + seed) is
**232,515–363,647 B** (desc-free receipt per-case table), dominated by the
descriptor record closure, not 357/449 B. `docs/phases/phase-11-results.md`
L78–83 and the desc-free section say this, but the headline "Win" bullet does
not — it presents the seed class as "the working set".

**Correction applied:** the win bullet now reports 449 B (not 357 B) and scopes
it explicitly to the **seed-store class**; it states that the honest total cold
observation is 232–364 KB (descriptor closure + index) and is *not* O(1), so
"bounded working set" means "page-closure-bounded", not "constant".

---

## F3 — "Cross-process reuse: process B executes 0 nodes" (SEVERITY: LOW — CONFIRMED, but must name its dependency)

**Claim.** `phase-11-results.md` L65–67; ADR-0025 ("kills the warm HashMap
illusion").

**Evidence.** Real: each CLI invocation is a new OS process, and the warm
`observe` reports `seed_nodes_executed = 0`, `seed_nodes_fetched = 0`
(`raw/…observe-text-warm.json.gz`). This is *not* a warm in-process map.

**But** the reuse is served by the **persisted derived cache** (universe 4).
`raw/…explain-after-clear.json` (a fresh process after `cache --clear` reports
`seed_nodes_executed = 2`, `seed_nodes_fetched = 2`, `cache_bytes_written =
1,677,797`) shows that clearing the cache re-executes the nodes. So the claim is
true only "with the derived cache present", and the reuse it proves is
cache-served, not seed-DAG recomputation.

**Correction applied:** the results doc and ADR-0025 now state the dependency on
the disposable derived cache and the observed cache-clear re-execution.

---

## F4 — ADR-0028 / FINDINGS §7 cite a receipt that does not exist (SEVERITY: MEDIUM — CLAIM NOT BACKED BY A PRESENT SEALED RECEIPT)

**Claim/evidence pointer.** `docs/adr/0028-finer-than-object-sharing.md` L8–9
and L185, and `FINDINGS.md` L278, cite
`evidence/campaigns/2026-10-06-phase11-share-0f3d1d3/` at commit `0f3d1d3`.

**Counter-evidence.** No such directory exists; `git ls-tree 0f3d1d3` has no
share receipt and `0f3d1d3` is **unreachable** from `HEAD` (a dangling pre-rebase
commit). The sealed receipt actually present is
`evidence/campaigns/2026-10-06-phase11-share-d9f818a/`, whose `receipt.json`
records commit `d9f818a` and whose numbers (unique 208,001 vs CDC 160,668,
tar-xz 92,752, …) are exactly those quoted in ADR-0028. The `d9f818a` receipt is
also unreachable from `HEAD` by commit id, but its *directory* is committed in
the tree, so it is the present artifact.

**Correction applied:** all three references now point to
`2026-10-06-phase11-share-d9f818a` / commit `d9f818a`.

*(The share claim itself — see F7 — is otherwise confirmed.)*

---

## F5 — "The `.voldoc` descriptor is copied verbatim" (SEVERITY: LOW — OVERSTATED WORDING)

**Claim.** `phase-11-results.md` L455–459 (immutable edit witness).

**Counter-evidence.** The descriptor blob is **shared by content id**, never
copied. `src/field/edit.rs` L149 ("Read the manifest only: no descriptor blob, no
document parse") and the witnessed counters (`descriptor_bytes_read = 0`,
`descriptor_file_opens = 0` via `strace`) prove no descriptor bytes move. Only
the 32-byte `descriptor_id` (and the `DocumentExact` root id) is written into the
new manifest. "Copied verbatim" invites the reading that the 72,937 B blob was
byte-copied — which would itself be a PDF-level no-op; the receipt shows the
stronger, genuine property (id-shared, read-free) instead.

**What survives.** The edit witness is genuine within its narrow scope: 318
index entries carried forward by id, 2 new seed nodes, descriptor read = 0. It
is a *derived page-projection* edit: `materialize(R1)` still yields the original
PDF bytes (the exact archive is untouched by construction), so the "exactness of
R1" is inherited, not a new reconstruction capability — which the doc already
states.

**Correction applied:** wording changed to "shared by content id (neither read
nor rewritten); only its content id is written into the new manifest".

---

## F6 — Measurement-boundary / hygiene issues (SEVERITY: LOW–MEDIUM)

- **F6a (MEDIUM):** the lifetime crossover table compares VOLE's *instrumented*
  `bytes_read` (descriptor+manifest+index+seed, cache read excluded) against
  A0/A1's *process* `read`/`pread64` total. The direction of the recorded wins/
  losses mostly survives (large loses either way; small docs win either way at
  process level), but the magnitudes are not comparable and the table header
  does not restate the exclusion per row. Noted; not re-derived here.
- **F6b (LOW):** `evidence/…-desc-free-a8ad6f4/SUMMARY.md` contains the entire
  "Descriptor-free narrow observations" section **twice** (L159–230 and
  L232–302). Apparent duplicate append; harmless but a receipt-hygiene defect.
- **F6c (LOW):** `docs/phases/phase-11-results.md` L3 still says "Commit under
  test: `63f43fb`" although the document is a cumulative multi-commit results
  file (it now carries a8ad6f4 / 824faa9 / 8cceaac sections). Added a header note.
- **F6d (LOW):** the `63f43fb` court receipt was sealed from a **dirty** tree
  (`tools/field-court.sh` modified). The exactness triple it proves is still
  valid (`length` + `SHA-256` + `cmp` vs a pre-removal golden copy, materialize
  run in a new process), but the committed tool is not byte-identical to the one
  that ran. Recorded, not corrected.

---

## F7 — Shareable units / `unique_bytes` (SEVERITY: — CONFIRMED, modulo F4)

`unique_bytes` is explicitly a **lower bound** (excludes root/program/GRAPH
framing; the 8,435 B excluded is reported) and the losses are recorded
(vs strongest CDC −22.7 %; vs `tar | xz -9e`; per-stratum LOSS on the near
stratum). The marginal 1.6 % whole-cohort edge over per-file min LZ is correctly
labelled non-robust and smaller than the excluded framing. ADR-0028 is honest.
Only the receipt pointer was wrong (F4).

---

## F8 — Immutable-edit witness: procedural vs copied, stated exactly

- **Procedural (new work):** 2 seed nodes (`Literal` + `PageContent`), 2 index
  tree nodes (rewritten leaf + spine), 1 manifest; 8,776 payload bytes.
- **Shared by id (no read, no write):** the descriptor blob, the `DocumentExact`
  root id, every unaffected seed node, 318 index entries, and 2 pre-existing
  index tree nodes.
- **Cost:** 43,688 B of index read vs 8,776 B written — recorded as a loss.
- **Limits (as stated):** one page per call, ≤ 48 KiB content, no insert/delete/
  reorder, descriptor unchanged, `materialize(R1) == original` trivially.

Verdict: **confirmed** within the declared narrow subset (with F5's wording fix).

---

## Claims that survived without correction

| Claim | Verdict | Receipt |
|---|---|---|
| `materialize(R) == original` byte-for-byte with source removed, new process | **confirmed** | `…-63f43fb/receipt.json` (len + SHA-256 + `cmp`, `source_removed=true`) |
| Cross-process reuse = 0 executed nodes (real OS process) | **confirmed** (cache-served scope, F3) | `…-63f43fb/raw/…observe-text-warm.json.gz` |
| Pinned, offline tokenizer; smaller V is a working-set claim only | **confirmed** | `…-llm-tokens-824faa9`; asset SHA-256 verified at court time |
| Shareable units lose to CDC; `unique_bytes` is a lower bound | **confirmed** | `…-share-d9f818a` (F4 pointer fix) |
| `descriptor_bytes_read == 0` on the warm short-circuit | **confirmed** | `…-desc-free-a8ad6f4` + `tests/field_partial.rs` |

## What the review could not verify (stated honestly)

- The warm process byte cost is measured only for `large` (50 pages) in the
  lifetime raw strace; the `large-400` figure (≈ 74.9 KB) is *inferred* from
  `bytes_returned` + instrumented overhead, not sealed by a court-time strace.
  A follow-up should add a warm `strace` to the field court for each case so the
  claim is measured rather than inferred.
