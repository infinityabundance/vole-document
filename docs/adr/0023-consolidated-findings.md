# ADR-0023: The current VOLE representation stack does not beat purpose-built baselines on any measured axis

- **Status:** Accepted — superseding top-level decision (Phase 10.2). This ADR
  **supersedes the standing of every prior "win"** in this repository: none is
  general, and the only surviving results are scoped and stated below.
- **Date:** 2026-10-06
- **Amends:** none. **Relates to / consolidates:** ADR-0010, ADR-0011, ADR-0012,
  ADR-0013, ADR-0015, ADR-0017, ADR-0018, ADR-0019, ADR-0021, ADR-0022.
  **Explicitly cited by this decision:** ADR-0017, ADR-0018, ADR-0019, ADR-0021,
  ADR-0022.

## Context

Ten phases built a byte-exact reversible representation stack and measured it,
axis by axis, against purpose-built baselines. The results are scattered across
per-phase ADRs, and several early "wins" were relative to a weak order-0 lane
(`BYTE_RANS`) rather than to real compressors; independent adversarial reviews
progressively corrected the framing. Phase 10.2 asked for one authoritative
statement so no reader can assemble a false general claim from the individual
phases.

## Decision

1. **Adopt the consolidated verdict as the repository's top-level claim.**

   > **The current VOLE representation stack does not beat purpose-built
   > baselines on any measured axis. The durable results are exact byte-exactness,
   > an auditable/typed reconstruction representation, and a complete, receipted
   > record of the negatives.**

   `FINDINGS.md` is the authoritative expansion of this ADR; every number in it
   links to a sealed receipt and names its baseline.

2. **Only two scoped results survive** (and both are bounded):

   - a **small partial-decode CPU win** for late random-access queries on large
     documents, versus sequential `gzip`/`xz` (ADR-0018), realized after Phase 8 as
     a bytes-read win **versus non-seekable sequential codecs only** (ADR-0019) —
     never beating zstd's decoder, **losing to fine-block seekable formats but
     reading less than coarse-block ones** (BGZF 23,808 B and xz-64KiB 15,344 B
     read less than VOLE's 460,713 B; xz-4MiB 708,612 B and pixz 2,810,832 B read
     more), and carrying an offset-independent ~440 KB floor; and
   - **whole-object dedup of identical opaque files** in the content-addressed
     store (ADR-0021) — real but **marginal versus CDC** and large only versus
     per-file LZ, which is the wrong comparison for a sharing axis.

3. **One qualitative capability difference is under-claimed (stated as a
   capability, not a bytes-read win).** A single `.voldoc` artifact simultaneously
   provides full byte-exact archival materialization **and** structural
   observation (`--pdf-object`/`--pdf-stream`/`--pdf-revision`). BGZF and blocked
   xz provide byte-range seeks only — the seekable court measured byte ranges
   only — so neither can answer an object/stream/revision query without first
   reconstructing and re-parsing the document. This is a capability difference,
   not a size or bytes-read advantage.

4. **Three axes are recorded losses**, each against a named purpose-built
   baseline: whole-file size vs `gzip`/`zstd`/`xz`/`brotli` (0/27, ADR-0017);
   random-access bytes read vs fine-block seekable formats (2.6–30× more,
   ADR-0019; VOLE reads **less** than coarse-block seekable formats, so it sits
   between fine and coarse block sizes rather than always losing); cross-document
   sharing vs per-file LZ **and** generic CDC (ADR-0021). The cause is structural:
   the DRA + typed-residual + entropy-channel representation is **coarser** than
   LZ77 plus seekable-block formats, and byte-exactness is shared with any lossless
   compressor, so it is a floor rather than an advantage.

5. **Encoder-only search governance buys nothing measurable** (ADR-0022): the
   fixed complete-cost heuristic already attains the exhaustive minimum on every
   workload in the frozen cohort (median byte benefit 0 ‰). The mechanism is
   retained as an optional, encoder-only substrate; the fixed court is kept.

6. **Claim discipline becomes normative.** From this ADR forward, in this
   repository:

   - no whole-file "compression" claim may be made without the four generic
     compressors on a committed corpus and a sealed receipt (ADR-0017);
   - cross-document sharing is store **amortization**, never "compression", and a
     store root is never compared to a whole document (ADR-0021);
   - exactness is stated as the invariant, **not** as a competitive win;
   - no "population" claim may be drawn from the locally generated, deterministic
     corpora;
   - withdrawn claims (§6 of `FINDINGS.md`) stay withdrawn and are cited wherever
     the affected phase is described.

## Consequences

- Prior ADRs are **not** deleted; ADR-0015's structural win, for example, remains
  a true, scoped per-mechanism result (over `BYTE_RANS`, on a fixture we authored)
  but must be read under this ADR: it is not a compression result and its enabling
  condition has never been observed in genuinely transformed producer output.
- The engineering outputs — the exact container, the bounded DRA with a checked
  coverage certificate, the typed residual channels, the entropy capsule, the
  fail-closed feature/mandatory-bit policy, the seek reader, the content-addressed
  store, and the fuzzing/isolation work — are unaffected and remain correct and
  exact. What is retired is any claim that they beat purpose-built tools.
- The exit criteria to supersede this ADR are the three falsifiable directions in
  `FINDINGS.md` §7 (finer-than-object shareable units; a model stronger than LZ; a
  producer-population corpus), each with an explicit prior that it, too, may lose.
- This is a docs-only decision: no wire byte, candidate, decoder behaviour, or
  feature bit changes.

## References

- [`FINDINGS.md`](../../FINDINGS.md) — the authoritative consolidated findings
- ADR-0017 (whole-file size: 0/27 vs generic compressors)
- ADR-0018 (partial materialization: scoped decode-CPU win, no I/O win)
- ADR-0019 (seek-based I/O: scoped win vs non-seekable; loses to fine-block
  seekable formats, reads less than coarse-block ones)
- ADR-0021 (cross-document sharing: robust loss vs LZ and CDC)
- ADR-0022 (encoder-only governance: no measurable byte benefit)
- `docs/evidence/phase{6,7,7b,7c,8,9}-skeptic-review.md` (the falsified-claims log)
