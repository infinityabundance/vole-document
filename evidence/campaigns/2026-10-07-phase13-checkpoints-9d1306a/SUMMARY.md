# Campaign: 2026-10-07-phase13-checkpoints-9d1306a — Phase 13.4

Byte-level partial-materialization **checkpoint records**. The optional
`FLAG_OPTIONAL` `CHECKPOINT` record (`RecordTag` `0x60`, payload `checkpoint_v1`)
carries a bounded per-op output-boundary table bound to the `GRAPH` record by a
CRC-32C. The seek reader consumes a **validated** checkpoint in place of the
`OBSERVATION_INDEX` for a raw byte range; a lying/corrupt/non-optional checkpoint
is rejected (full parser fails closed) and the seek reader falls back to the
Phase-8 index lane. The universe string is unchanged; `materialize == original`
is preserved.

Pre-registered claims:

- **H1 byte-exactness.** A checkpointed descriptor materializes exactly, and the
  checkpoint lane and the index lane serve the same bytes for every query
  (identical `ops_evaluated`).
- **H2 advisory.** A lying/corrupt/non-optional checkpoint is *rejected*; the
  reader falls back to the index lane (identical bytes, paying only for the
  rejected read), never to a guess.
- **H3 work (expected negative).** The per-op table is redundant with the
  observation index and its record is larger than the index record it replaces,
  so `bytes_read` does not fall and CPU is unchanged.

## Result

```json
{
  "court": "tests/phase13_checkpoints.rs",
  "court_tests": 4,
  "court_passed": 4,
  "in_file_checkpoint_tests": 7,
  "in_file_descriptor_tests": 5,
  "gate_total_passed": 1048,
  "gate_total_failed": 0,
  "verdict": "PASS (recorded negative on the work axis)"
}
```

Measured `bytes_read` (from the internal `CountingReader`) on the deterministic
`pdf-make-large` corpus, `PDF_DEFLATE_REPLAY_RANS_INDEXED` lane (start/quarter/
middle/end byte-range queries; the delta is constant per descriptor because both
lanes read a fixed record set):

| objects | descriptor floor → checkpoint | Δ `bytes_read` per query | `ops_evaluated` |
| ---: | ---: | ---: | --- |
| 20 | 94,754 → 98,984 B | **+259 B** | identical |
| 40 | 189,073 → 197,143 B | **+439 B** | identical |
| 120 | 567,382 → 590,812 B | **+1,159 B** | identical |

`cargo test --all-features --test phase13_checkpoints` → **4 passed / 0 failed**
(`raw/court.txt`). In-file → **7 + 5 passed / 0 failed**. Full gate →
**1048 passed / 0 failed** incl. `check-docs.sh` (`raw/gate.txt`).

## What the court establishes

1. **Exactness is untouched**: the checkpoint lane and the index lane agree
   byte-for-byte on every query, with the same op window; `materialize` is
   unchanged.
2. **The checkpoint is advisory, never authority**: the full parser rejects a
   lying/corrupt/non-optional checkpoint, and the seek reader falls back to the
   Phase-8 index lane — exact bytes, never a denial of service.
3. **A negative on the work axis**: the checkpoint record is larger than the
   `OBSERVATION_INDEX` it replaces (16 B/op boundary entries + framing vs the
   index's 9 B/op op table), so every byte-range query reads *more* bytes with
   identical CPU. The `GRAPH` record dominates the floor regardless.

## Honest limits

- One corpus (the deterministic `pdf-make-large` generator) and one lane
  (`PDF_DEFLATE_REPLAY_RANS_INDEXED`); **no population claim**.
- The checkpoint lane is **byte-range-only** (PDF object/stream/revision selectors
  keep the index lane).
- `graph_crc32c` binds a checkpoint to one `GRAPH` record but is not a
  cryptographic binding.
- The negative is structural, not tuned: the checkpoint recomputes information
  the index already persists, so no entry encoding can make it beat the index op
  table it duplicates.
