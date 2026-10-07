# Phase 7.3c — independent adversarial review of the partial-materialization result

- **Reviewer:** independent adversarial reviewer (Phase 7.3)
- **Subject:** campaign `2026-10-05-phase7-partial-a5764c9`, ADR-0018,
  `docs/evidence/phase7-partial-report.md`
- **Disposition:** headline **confirmed** (byte-exact query serving; a real
  decode-CPU win for non-early queries vs gzip/xz); three presentational defects
  found and corrected in prose. No measurement was re-run and no candidate or
  wire format changed; the sealed `results.json` and `queries.jsonl` raw fields
  are untouched.

## Angles attempted

1. **Exactness.** Re-derived the served slices against the source for the 18
   pre-registered queries. All 18 are byte-exact (`obs == full[a..b]`, and
   `verify` checks length + SHA-256). No counterexample found.
2. **Primary-metric arithmetic.** Summed the two reported cost fields and compared
   with the per-query raw record.
3. **Bytes read (the I/O question).** Compared the descriptor size actually read
   against the compressed prefix a sequential codec reads at the same offset.
4. **Wall time vs CPU time.** Separated the two: the win is a CPU-time result;
   wall time includes process startup and page-fault costs.
5. **Peak RSS.** Compared resident-set cost of the whole-descriptor parse against
   the baselines.
6. **Corpus generality.** Asked whether one locally generated document supports a
   population claim.

## Findings (all three presentational; none overturns the headline)

### F1 — Double-counted cost field (confirmed, corrected)

`descriptor_bytes_traversed` already includes the referenced entropy-channel
payload bytes, and `entropy_bytes_decoded` reports the same payload bytes again.
The campaign quoted `descriptor_bytes_traversed + entropy_bytes_decoded`
(0.41–0.46 MB), which **double-counts** the decoded channels. Reading the raw
`queries.jsonl` fields, the honest decode-side bound is
**`descriptor_bytes_traversed` alone = 412,161–433,694 B ≈ 0.41–0.43 MB**,
constant across offset (the no-channel queries report 412,161 B and
`entropy_bytes_decoded = 0`). The arithmetic is now stated as a subset relation
in `src/materialize/observation.rs` and every quoted number was corrected to
0.41–0.43 MB with the sum removed. The headline win fraction
(1.4–2.3 % of gzip's inflated bytes in the late region) is essentially unchanged.

### F2 — "decode CPU/allocation" mislabels the win (confirmed, corrected)

The win is decode **CPU**. Peak RSS is a **loss**: ~38 MB for VOLE versus gzip's
~1.2 MB (a ≈30× RSS *penalty*, already recorded). Calling the result an
"allocation" win inverted the sign of the memory axis. The label is now decode
**CPU** everywhere; the RSS loss remains recorded.

### F3 — Early-region boundary overstated (confirmed, corrected)

The original boundary ("early region ≤ ~1–8 MiB") is contradicted by the raw
table: at 8 MiB xz takes 0.06 s CPU versus VOLE's ~0.03 s, so 8 MiB is not a
loss to xz. The constant-cost region where VOLE can lose to gzip/xz starts below
roughly 8–16 MiB (gzip crosses near 8–16 MiB, reaching 0.11 s only at 31 MiB).
The boundary is now stated as **≤ ~8–16 MiB**.

## What the review did *not* falsify

- The query service is byte-exact on all 18 points.
- For non-early queries the indexed lane is a genuine **decode-CPU** win versus
  gzip and xz (≈2–5× vs gzip, ≈4–13× vs xz in the late region).
- The **true bytes-read comparison is a loss, not a win:** at 31 MiB gzip reads a
  ~9.8 MB compressed prefix while VOLE reads its entire **17.5 MB** `.voldoc`
  (`descriptor_bytes_traversed` is a CPU-side approximation, never a byte-read
  figure). No I/O win exists in v1; an mmap/seek reader is the prerequisite.
- **Wall vs CPU:** zstd's raw decompressor reaches every offset in ≤ 0.01 s *wall*,
  so VOLE never beats zstd; the VOLE figure is quoted as CPU time (user+sys).
- **RSS:** VOLE's whole-descriptor parse costs ~38 MB resident versus gzip's
  ~1.2 MB.

## Scope of the surviving claim

**Decode-CPU win for non-early random-access queries versus gzip and xz; never
beats zstd; bytes-read is a loss; an RSS penalty; measured on one 33.8 MB
locally generated PDF.** It is not a whole-file compression win, not an I/O win,
not an allocation win, and not a population claim.
