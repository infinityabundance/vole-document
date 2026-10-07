# ADR-0039: Byte-level partial-materialization checkpoints are byte-exact and advisory but do not pay their framing (recorded negative)

- **Status:** Accepted — recorded negative result (Phase 13.4)
- **Date:** 2026-10-07

## Context

Phase 8 landed the seek `DIRECTORY` and a partial descriptor lane (ADR-0018,
ADR-0019); Phase 11 landed the observation engine (ADR-0024, ADR-0026). The
`RecordTag::Checkpoint` (`0x60`) slot was reserved for **literal byte-level
checkpoint records** and was never used: a planner had no persisted per-op output
boundary table to consume. Phase 8's measured floor is dominated by the `GRAPH`
record (and, secondarily, the `OBSERVATION_INDEX`), so a checkpoint could only
help by *replacing* a record the seek reader already reads.

The question this ADR answers: **if a descriptor persists a bounded per-op
checkpoint table, and the seek reader consumes it in place of the
`OBSERVATION_INDEX`, does that reduce bytes read or CPU for a random-access
observation?** The Phase-8 floor makes a negative the expected outcome; the
mechanism is implemented and measured rather than reasoned about.

## Decision

- **An optional `CHECKPOINT` record** (`RecordTag::Checkpoint = 0x60`, written
  `FLAG_OPTIONAL`) carries one versioned, little-endian `checkpoint_v1` payload: a
  per-op output-boundary table (`out_start:u64`, `out_len:u64` per op), the
  declared source length, and a `graph_crc32c` binding that ties the table to the
  exact `GRAPH` record. An ignorable optional bit `FEATURE_CHECKPOINTS` (`1 << 2`)
  is set in the header's `optional_features`. `FORMAT_MINOR` does not bump and the
  **universe string is unchanged**: the checkpoint adds no new opcode, coder,
  limit semantic, or adapter meaning — its meaning is entirely derived from the
  existing reconstruction program. A decoder that skips it still fully
  materializes; a checkpointed descriptor round-trips
  (`materialize(descriptor) == original_bytes`, length + SHA-256 + `cmp`).
- **A checkpoint requires a seek directory.** It is locatable only through the
  `DIRECTORY` class index, so `serialize` rejects `checkpoints.is_some()` without
  `seek_directory` (which itself requires an `OBSERVATION_INDEX`). The record is
  emitted as a `FLAG_OPTIONAL` record; the `DIRECTORY`'s `CLASS_INDEX` gains the
  `CHECKPOINT` class.
- **The checkpoint is advisory, never authority (mirrors ADR-0019).**
  `CheckpointTable::validate` re-derives *every* op length from the authoritative
  program with `Program::analyze_ops`, requires the entries to be contiguous and
  to cover the source exactly, and requires the `graph_crc32c` binding to match the
  `GRAPH` record. A lying, corrupt, oversized, out-of-closure, or non-optional
  checkpoint is **rejected**: `Descriptor::parse` fails closed, and the seek reader
  ignores it and **falls back to the Phase-8 index lane** — never to a guess, and
  never denying service. A rejected checkpoint costs exactly its record bytes.
- **The seek reader consumes it for one query class.** A raw byte-range selector
  is the only selector that needs no `OBSERVATION_INDEX`; the seek reader
  (`materialize_observation_seeked`) validates the checkpoint, selects the op
  window with the checkpoint's lengths via the shared
  `select_ops_from_lengths`/`selection_references` path, and reads **no index
  record**. Other selectors (PDF object/stream/revision) keep the index lane.
  `materialize`/`decode`/`verify` are unchanged and remain the archival authority.

## Consequences

- **Measured result (receipt
  `evidence/campaigns/2026-10-07-phase13-checkpoints-<shortsha>/`):** on the
  deterministic `pdf-make-large` corpus and the `PDF_DEFLATE_REPLAY_RANS_INDEXED`
  lane (the Phase-8 seek lane), the checkpoint lane serves **byte-identical**
  slices with **identical `ops_evaluated`** to the index lane for every
  start/quarter/middle/end query. It is a **recorded negative**: the checkpoint
  record is *larger* than the `OBSERVATION_INDEX` it replaces, so every query
  reads **more**, not fewer, bytes.

  | objects | descriptor (floor → checkpoint) | Δ per byte-range query (`bytes_read`) |
  | ---: | ---: | ---: |
  | 20 | 94,754 → 98,984 B | **+259 B** |
  | 40 | 189,073 → 197,143 B | **+439 B** |
  | 120 | 567,382 → 590,812 B | **+1,159 B** |

  The overhead is a constant per descriptor (both lanes read a fixed record set)
  and grows with op count: the checkpoint's 16 B/op boundary table is wider than
  the index's 9 B/op op table (`out_len:u32 | dep_kind:u8 | dep_id:u32`), and its
  own record framing tips it over. On this corpus the index is ~5–7 % smaller than
  the checkpoint, so the checkpoint never reduces either axis.
- **Why there is no win (the mechanism is materially redundant).** The
  `OBSERVATION_INDEX` already persists exactly the per-op output lengths the
  checkpoint recomputes (and more: dependencies, selectors, digests). A checkpoint
  can only pay if it is *smaller* than the index record it lets the reader skip;
  storing every op's absolute start offset doubles the entry to 16 B, and even a
  length-only table would be the index's own op table minus its dependency bytes,
  i.e. a strictly weaker copy. The `GRAPH` record — the irreducible floor for any
  lane that must evaluate ops — dominates regardless, so no per-op table can
  reduce the floor. **The byte-level checkpoint is therefore closed as a recorded
  negative**; the mechanism stays implemented (opt-in, `checkpoints: Some(..)`)
  so the negative is reproducible and the fallback discipline is tested.
- **The advisory discipline is the real product.** The negative is only credible
  because the exactness contract holds in both directions: an honest checkpoint
  serves byte-identical output, and a lying/corrupt/non-optional one is rejected
  by the full parser and ignored by the seek reader (which then reads the index
  and serves the same bytes). No validate-or-decline rule was weakened, no cap was
  raised (`max_checkpoint_bytes` bounds the record before allocation), and the
  universe semantics are unchanged.
- **Honest limits.** One corpus (the deterministic `pdf-make-large` generator) and
  one lane (`PDF_DEFLATE_REPLAY_RANS_INDEXED`); no population claim. The
  checkpoint lane is byte-range-only (PDF selectors keep the index). The
  `graph_crc32c` binding detects a checkpoint reused against a different graph but
  is not a cryptographic binding. Unreferenced checkpoint entries cannot exist:
  the table is one entry per op and every entry is validated against
  `analyze_ops`.

## References

- `src/container/checkpoint.rs` (`checkpoint_v1`, `CheckpointTable`),
  `src/container/record.rs` (`RecordTag::Checkpoint`, `read_record_at`),
  `src/container/header.rs` (`FEATURE_CHECKPOINTS`),
  `src/container/descriptor.rs` (checkpoint serialize/parse + validation),
  `src/container/directory.rs` (`CLASS_INDEX_TAGS`),
  `src/materialize/seek.rs` (the checkpoint lane + index-lane fallback)
- `tests/phase13_checkpoints.rs`, `tools/phase13-4-checkpoint-court.sh`
- ADR-0018: partial materialization (decode-CPU precursor);
  ADR-0019: seek-based partial I/O (the bytes-read floor the checkpoint is
  measured against)
