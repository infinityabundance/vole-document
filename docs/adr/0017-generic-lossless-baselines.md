# ADR-0017: Generic lossless compressors are the whole-file comparator, and VOLE's whole-file lanes lose to them

- **Status:** Accepted — recorded methodology and negative result (Phase 7.0c)
- **Date:** 2026-10-05

## Context

Through Phase 6 the complete-cost court compared VOLE's structural candidates
against `BYTE_RANS`, a whole-file **order-0 byte-rANS** lane with no LZ. A win
over `BYTE_RANS` was repeatedly described as a compression result, but it is not
one: order-0 entropy coding without modelling is far weaker than any modern
lossless compressor. The strongest VOLE result before this ADR — exact DEFLATE
replay on a shared, entropy-coded plaintext (ADR-0015) — was measured only
against `BYTE_RANS`.

An honest test needs the same input compressed by the general-purpose tools a
user could reach for, on the **same complete file**, with **every** byte of
framing, model, index, and residual overhead charged, and every result
round-trip verified lossless.

## Decision

- **The comparator is generic lossless compression on the complete file.**
  `gzip -9`, `zstd -19 --long=27`, `xz -9e`, and `brotli -q 11` are run on the
  source and must decompress to a byte-identical copy before their size is
  scored (`tools/baselines.sh`; a failed round-trip is recorded `null`, never
  accepted). VOLE's number is the smallest complete serialized `.voldoc` across
  the auto winner and every forced lane, so it is the most favourable VOLE
  figure.
- **Wins over `BYTE_RANS` alone are not compression claims.** They may be
  reported only as a per-mechanism ablation, explicitly labelled relative to the
  order-0 baseline.
- **The two axes are never conflated** (see ADR-0018): a whole-file size result
  and a random-access query-cost result are separate receipts and separate
  verdicts.

## Consequences

- **Measured result (Phase 7.0c, receipt
  `evidence/campaigns/2026-10-05-phase7-baselines-7b9f662/`):** on 27 locally
  generated files (23 Phase-7.0 producer/synthetic, 4 Phase-7.0b
  generator-family), the best VOLE lane beats `gzip`/`zstd`/`xz`/`brotli` on
  **0 files**. The best generic compressor is smaller on every file by **460,320
  bytes** in total. On `cairo-vector.pdf` the best VOLE lane (34,574 B) is
  **2.07×** brotli (16,670 B); on `_synthetic/flate.pdf` (36,102 B) it is
  **1.91×** xz (18,884 B). The best VOLE lane *does* beat `BYTE_RANS` on 13/27
  files — which is exactly the misleading comparison this ADR retires.
- **Scope of the negative:** VOLE's byte-exact structural reconstruction is
  unchanged and remains the prime-directive property (materialize == source).
  What does not survive is any *whole-file compression* claim. No candidate is
  adopted, removed, or changed by this ADR.
- **Reinforced on the large corpus (Phase 7.3, ADR-0018):** on a 33.79 MB,
  800-object PDF the best VOLE lane is 17,392,713 B against xz's 5,841,896 B
  (**2.98×**). The loss is not a small-file artifact.
- Any future whole-file claim must be stated against these four compressors, on
  a committed corpus with a sealed receipt, or it is not a claim.

## References

- `tools/baselines.sh`, `tools/baselines.jq`, `tools/baselines-merge.jq`,
  `tools/baselines-table.jq`
- Receipt `evidence/campaigns/2026-10-05-phase7-baselines-7b9f662/`
- `docs/evidence/phase7-corpus-report.md` (Phase 7.0c amendment)
- ADR-0015: exact DEFLATE replay wins on shared plaintext (a per-mechanism
  result, now bounded by this ADR)
- ADR-0018: partial materialization is a query-cost result, not a whole-file one
