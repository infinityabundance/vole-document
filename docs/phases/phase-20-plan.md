# Phase 20 — Hardening and economics

> **PLANNED — not started.** No code, court, or receipt for Phase 20 exists yet.
> This document is the contract to be frozen *before* implementation; the
> orchestrator owns integration and commits. Anything below that looks like a
> measured number is a **target or a predecessor's result**, never a Phase-20
> result.

Branch (to be created): `phase20`. Base: `main` @ `v0.1.0-alpha.24` (Phase 19).
Predecessors: [phase-18-results.md](phase-18-results.md),
[phase-19-results.md](phase-19-results.md), ADR-0053, ADR-0054.

## Why

Phase 18 inverted the equal-contract build position by **deleting unnecessary
durability syncs** (`SyncPolicy::Batch`, ADR-0053) and Phase 19 re-read that win
with paired, interleaved repetitions (ADR-0054). That leaves four open, ranked
problems, in the order a risk-first queue demands:

1. **The durability win is unproven under faults.** `SyncPolicy::Batch` is
   argued correct from the append-only/prefix-recovery design, but no court has
   *injected* crashes at the durability boundaries and shown the argument holds.
   A correctness claim about a store the exactness path depends on outranks every
   performance question.
2. **Memory, not wall, is the next binding constraint** (`nasa-pdf-0001`: a
   409 MiB source → 2342 MiB peak RSS ≈ 5.7×; Phase 19.3).
3. **The warm lane is a modest real loss** (~1.29×, ~0.7 ms per session; Phase
   19.2).
4. **C4b corpus/external lineage remains open** (ADR-0052); it is external
   metadata a single document cannot derive.

## Rules (unchanged, enforced)

Exactness is the invariant: `materialize(descriptor) == original_bytes`, and
every exact court requires **all three** of equal length, equal SHA-256, and
`cmp`. **Docker only** — never `cargo`/tools/corpus work on the host; every
command runs in a digest-pinned, memory-capped compose service. One production
crate. One branch per phase; commit and push after every subphase. Research
subagents are read-only. Negative and partial results are recorded, never
buried. **No cap is raised to make a run pass**, and no validate-or-decline rule
is weakened. No claim without a sealed receipt.

---

## 20.1 Crash / power-cut fault-injection court

**Question.** Does the packed seed store, under **both** `SyncPolicy::Batch`
(default) and `SyncPolicy::Each`, hold its exactness contract across an
arbitrary crash at every meaningful durability boundary — and is the recovery
set *exactly a prefix* of the appended record sequence?

**Method.** Instrument the packed writer (`src/store/pack.rs`) with a
deterministic, pre-registered set of **inject points** and a child-process
killer. The on-disk layout is fixed and known: `<root>/fieldpack/seg-N.pack`
(`24 B` header + `u32_len || body` records) and the immutable
`seg-N.idx` published atomically at seal; the open segment is the `.pack`
without an `.idx`; the field manifest (`FieldRoot`) is published only after
`PackedSeedStore::flush` has made every node it references durable.

Inject at each boundary:

| Boundary | Inject points |
|---|---|
| Record framing | before / during / after the `u32` length prefix |
| Record body | during / after the body write |
| Segment flush | before / during / after `PackedSeedStore::flush` |
| Index publication | before / during / after the `seg-N.idx` seal (atomic rename) |
| Manifest publication | before / during / after `FieldStore::put_field` |

Injection modes, applied at every point above:

- `SIGKILL` of the builder process;
- a hard process **abort** (no unwinding, no `Drop` flush);
- **truncated** `.pack` bytes (cut mid-prefix, mid-body, and at a record edge);
- **truncated** `.idx` bytes (cut mid-header, mid-bucket table, mid-record run);
- **bit flips** in a sealed segment's `.pack` and `.idx`;
- a **zeroed tail** of the open segment.

Every (boundary, mode, policy) cell is a receipt row. Run the whole matrix under
`SyncPolicy::Batch` **and** `SyncPolicy::Each`, with a fixed seed and a recorded
cell order.

**Courts / receipts.** New `tools/phase20-crash-court.sh` driving a new Rust
integration court (`tests/crash_recovery.rs`) that forks the builder, kills it at
the injected point, then reopens the store. Sealed campaign under
`evidence/campaigns/<date>-phase20-crash-<sha>/` recording base image digest,
toolchain, `Cargo.lock` SHA-256, commit/dirty state, exact command, and the raw
matrix. Results doc `docs/phases/phase-20-results.md`.

**Assertions.**

*With **no** published manifest* (crash before manifest durability):

- `FieldStore::open` succeeds and the store reopens;
- **no partial node is fetchable**: every `get_node` either returns whole
  canonical bytes or is absent — never bytes whose `NodeId::of_node` differs
  from the requested id;
- every recovered id **re-hashes correctly** (the recovered set is exactly a
  prefix of the appended sequence; the torn tail is discarded — truncated on a
  read-write reopen, ignored on a read-only reopen);
- no dangling manifest exists.

*With a published manifest* (crash after manifest durability):

- the **root node and every dependency node exist** and are fetchable;
- `materialize --exact` **succeeds and matches the source** (length + SHA-256 +
  `cmp`);
- observations remain valid (the text/metadata answers are byte-identical to a
  clean store of the same field);
- sealed `seg-N.pack`/`seg-N.idx` pairs are immutable and their entries cannot be
  lost or reordered.

Tamper cases (`bit flips`, truncated `.idx`) must **fail closed**: a lying index
is rejected and never silently trusted, and a flipped body is caught by the
unchanged whole-node re-hash gate.

**Honest falsifier / success criterion.** The mechanism is **falsified** if, in
any cell, a partial node is observable, a published manifest references a
missing node, `materialize --exact` mismatches, recovery is not a prefix, or the
store crash-loops on reopen. Success = **zero violations across the full matrix
under both policies**, plus a recorded statement of the at-risk window per
policy (Batch: at most the records since the last seal/flush; Each: at most the
in-flight record).

---

## 20.2 Large-source memory architecture

**Question.** The measured `nasa-pdf-0001` ceiling is a 409 MiB source →
**2342 MiB** peak RSS (~5.7×; Phase 19.3). Can peak memory be **bounded** for
sources ≳1 GiB while exactness and the observation surface are unchanged?

**Method.** Profile peak RSS **per stage** (Stage A capture; Stage B inversion;
seed-node write; descriptor serialization/materialization) on a measured
RSS-vs-source curve at roughly 10 MiB, 50 MiB, 100 MiB, 200 MiB, 400 MiB,
800 MiB, and ≥1 GiB. Attribute the largest resident buffers and replace
whole-source resident copies with streaming/chunked processing wherever the
shape permits — the precedent is 16.3, where `propose_rle` moved from a
~16 B/run pre-allocation to two O(1)-memory streaming passes. Explicitly **not**
a lever: raising `mem_limit`.

**Courts / receipts.** A memory court (`tools/phase20-memory-court.sh`) recording
peak RSS (harness self-report cross-checked with `/usr/bin/time -v`) per source
size and per stage, on the frozen `real100-v1` plus deterministic ≥1 GiB
synthetic sources; before/after `.voldoc` SHA-256 equality so the change is
proven output-preserving. Campaign + `docs/phases/phase-20-results.md`.

**Honest falsifier / success criterion.** Falsified if the curve at ≥1 GiB is
still ≈linear at ~5× source, or if bounded memory changes any `.voldoc` byte or
breaks `materialize --exact`. Success = a measured curve that is
**sublinear/flat in the fixed overhead** (peak RSS ≤ `C + k·source` with a
measured `k` well below the current ~5.7×), exactness 100/100 on the frozen
population, and no raised cap. If no bound is found, the 17.7×→8.9× partial fix
stands and the residual is recorded (ADR-0041 extended).

---

## 20.3 Warm heterogeneous query

**Question.** Is the Phase-19.2 warm loss (~1.29×, ~0.7 ms per session) a
closeable constant, or a durable property of the resident session?

**Method.** Profile the warm one-session lane to attribute the per-session cost
against the known mechanism: the session pays a one-time full `Field::open`
descriptor parse while the cold `observe` path uses the `narrow_probe`
short-circuit (ADR-0042). Candidate levers, each measured separately: a **lazy
session open** (isolated in 16.4, never shipped), hoisting the probe into the
session, reusing the parsed manifest/closure across requests, and avoiding the
per-request descriptor re-read. Measure with the **paired, interleaved, N=100**
discipline of ADR-0054: both lanes measured repeatedly at every depth, every
sample retained, fixed-seed cluster bootstrap 95% CIs, a pre-registered ±10% tie
band, and an explicitly stated estimator (paired median/geometric mean vs ratio
of sums).

**Courts / receipts.** Reuse the 12-document C0–C5 contract court; new campaign
`evidence/campaigns/<date>-phase20-warm-<sha>/`; exactness 12/12 both lanes and
480 warm envelopes with 0 mismatches as the regression guard.

**Honest falsifier / success criterion.** Success = the paired estimator moves
**outside the pre-registered tie band** in VOLE's favour (or, at minimum, the CI
is below a pre-registered non-inferiority bound) with observations and exactness
unchanged. Falsified/recorded-as-durable if no lever moves the paired estimate
outside the band — then the loss is recorded, **not** tuned away, and ADR-0042/
ADR-0054 are extended.

---

## 20.4 C4b / ExternalContext

**Question.** Can corpus/external lineage be admitted as an **explicit, typed
external layer** (`basis: ExternalMetadata`) beside the document-derived field —
never contaminating the core field to satisfy a benchmark?

**Method.** Define a typed `ExternalContext` (dataset id, family, member,
head, revision-family, provenance) supplied **explicitly** by the caller, and
answer the C4b tuple from it as a distinct observation whose provenance basis is
unambiguously `ExternalMetadata`. It is never a default, never inferred, never on
the decode path, and the core exact field still satisfies
`materialize(descriptor) == original_bytes`. Re-run the C0–C5 contract court to
see whether C4b closes **when the typed context is supplied**, and record the
answer as an external-input answer, not a document-derived one.

**Courts / receipts.** The contract-equivalent court with and without the
external context; a provenance assertion that the answer basis is
`ExternalMetadata`; exactness 12/12 both lanes. Campaign + results doc.

**Honest falsifier / success criterion.** Success = C4b answers when given the
typed context and still declines typed when not given it; the core field's
observations and exactness are byte-identical to a run without the layer; the
provenance records the external basis. **Falsified** if admitting external
metadata requires weakening provenance, mutating the core field, or letting an
external input touch decoder authority — in which case the ADR-0050/ADR-0052 open
question stands unchanged.

---

## Acceptance

The phase is complete only when every subphase above is measured and sealed and
the release is honestly scoped. Each subphase: exactness preserved (or the
regression recorded); a sealed receipt with base image digest, toolchain,
`Cargo.lock` SHA-256, commit and dirty state, CPU architecture, oracle versions,
and the exact command; negatives/partials recorded; docs reconciled. 20.1 runs
**first** because a correctness court over the store the exactness path depends
on outranks the economics.
