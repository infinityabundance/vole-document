# ADR-0022: Encoder-only search governance, with zero decode authority

- **Status:** Accepted — implemented; court measured (Phase 10.1). The
  governance *mechanism* is `ADOPTED` as an optional encoder feature; the
  parametric *search* is a **recorded negative** ("no measurable byte benefit on
  this cohort").
- **Date:** 2026-10-06
- **Amends:** none. **Relates to:** ADR-0006 (rANS substrate), ADR-0007 (exact
  replay), ADR-0008 (EntropyFS optional / `dsfb` non-optional there),
  ADR-0013 (layout+rANS negative), ADR-0015 (replay positive), ADR-0017
  (whole-file loss).
- **Campaign:** `evidence/campaigns/2026-10-05-phase10-governor-d2b09c9/`
  (branch `phase10`, commit `d2b09c9`).

## Context

The Phase-10 brief asks for **DSFB encoder-only search governance** with **zero
decode authority**: a component that observes a typed residual diagnostic trace
and may *recommend* candidates or a larger/smaller budget, subject to the rule
that a recommendation is only ever another candidate that reaches the same
complete-cost court. The success condition is
`DsfbGuided.final ≤ FixedHeuristic.final` while approaching an exhaustive oracle
with materially fewer candidates; negligible benefit must be recorded, not
manufactured.

### What `dsfb` actually is (empirical)

`dsfb 0.1.2` is a real, published crate, already in this lockfile **only
transitively** as a non-optional dependency of the optional `entropyfs = "=0.7.17"`
engine (feature `entropyfs-store`, ADR-0008). It resolves and builds under the
pinned MSRV (`rust-version = "1.70"`; Rust 1.89 container). But its own manifest
and API define it as **"Drift-Slew Fusion Bootstrap (DSFB) state estimation"** —
a Kalman-like `f64` observer (`DsfbParams`, `DsfbState { phi, omega, alpha }`,
`DsfbObserver::step`), whose "residual" is a per-channel measurement innovation
and whose job is to produce normalized **trust weights** over sensor channels. It
pulls `rand`/`rand_distr` and has:

- **no** concept of a candidate family, complete serialized cost, or search budget;
- **no** notion of a compressed-byte residual class or a source region;
- **no** API that accepts a residual diagnostic and returns a search directive.

Its residual/trust machinery is semantically a sensor-fusion mechanism over a
floating-point time series. Using it would force a `rand` tree and an `f64`
decision path into the encoder for no applicable API. **Verdict: real, MSRV-OK,
present only transitively, unavailable-for-purpose.**

## Decision

1. **A dependency-free VOLE-owned governor.** The `dsfb-search` cargo feature is
   `dsfb-search = []` — **not** `["dep:dsfb"]`. It is non-default and implies
   nothing else (`store`, `rans`, `deflate-replay` are independent). The name is
   retained for continuity with the brief; the `dsfb` crate is **not** a
   dependency of it.
2. **Encoder-only by construction.** `src/encode/governor.rs` (compiled only
   under `dsfb-search`) defines the typed residual diagnostics
   (`SourceRegion`, `FormatStructure`, `ResidualClass`,
   `RunStats`/`Periodicity`/`Recurrence`, `ResidualDiagnostic`, `ResidualTrace`),
   a parametric candidate space (`SearchConfig { scale_bits, partition, replay,
   packed, depth }`), a pure governor (`govern(&ResidualTrace) -> SearchDirective`
   with a frozen dominant-residual table and a bounded `Budget`), and the
   config-parameterized generator `propose_configured`. All fields are
   integer-only; there is no RNG and no floating point.
3. **A recommendation is only a candidate.** Every candidate any config or the
   governor enables is built by the *existing* generators and reaches the
   unmodified [`crate::encode::court::run`]: serialize → `Descriptor::parse` →
   `materialize` → byte-compare → complete cost. The governor writes no descriptor
   field, sets no feature bit, and leaves no state.
4. **No wire change, no decoder change.** No governance state is persisted in a
   `.voldoc`; `src/container/header.rs` is untouched and no feature bit is added.
   A descriptor produced under governance decodes in any build without
   `dsfb-search` (given the same underlying `rans`/`deflate-replay` capability as
   any other descriptor).

### Wire-legality of every knob

| knob | enumerated | why it needs no decoder change |
|---|---|---|
| `scale_bits` | `{8,10,12}` | `EntropyModel::from_counts(counts, sb)`; `Descriptor::parse` only requires `channel.scale_bits == model.scale_bits`. The channel is self-describing. |
| `partition` | `ByKind` / `ByRole` | `Op::InterleaveChannels` uses the kind byte only as an *opaque* payload-channel index bounded by `payload_channel_count`; the `ByRole` variant maps the 12 kinds onto 3 roles drawn from the same 12-channel space (`channels::role_id`/`split_role`). |
| `replay` | `Off`/`Dedup`/`DedupRans` | selects which existing `DEFLATE_REPLAY` lanes are *admitted*; encoder-side subset choice. |
| `packed` | `Packed`/`unpacked` | `packed=false` *excludes* the packed-framing layout lanes (`PACK_SEGMENTS`/`PACKED_CHANNELS`); an unpacked layout builder would be a representation change and is out of scope. |
| `depth` | `0..=9` | prefix length of the fixed family order; encoder-side. |

The exhaustive grid is `scale_bits(3) × partition(2) × replay(3) × depth(5) = 90`
configs (packed fixed to `Packed`, matching the frozen contract). Because a
deeper / replay-richer config's candidate set is a superset of a shallower one's,
the byte minimum over the grid equals the minimum over the six *maximal* configs
(`depth=9`, `DedupRans`, packed) for each `(partition, scale_bits)`.

### The governor

`govern` maps the dominant residual class of the **incumbent** (the diagnostic
matching `trace.best`) to a directive from a fixed table:

```text
RRaw dominant / incumbent is RAW   -> Stop(Raw)              (the RAW floor)
RCodec dominant                    -> Next(scale_bits = 10)   (then 8)
RStruct dominant                   -> Deepen(PDF_LAYOUT_RANS)
RLexical | ROrder dominant         -> Next(partition = ByRole)
RValue dominant                    -> Deepen(BYTE_RANS)
RPackage dominant, small source    -> Stop(BestSoFar)         (framing floor)
RPackage dominant, larger source   -> Widen(-1)               (shrink the budget)
budget exhausted                   -> Stop(BestSoFar)         (always bounded)
```

Residual classes are an encoder-side **cost attribution** over the priced
candidate (they sum exactly to its serialized length); they are explicitly *not*
a claim that a generating program was discovered, and are distinct from the wire
record category `CostBreakdown::residuals` (always 0 here).

## Court result (measured)

Small deterministic in-memory workloads (`src/adapter/pdf/samples.rs` plus a
synthetic opaque trio), split into disjoint **tune/holdout/control** sets *before*
measuring; H2 is judged only on the holdout. `tools/governor-court.sh` runs
`Exhaustive` / `FixedHeuristic` / `DsfbGuided` over every workload and records
final complete bytes, candidates evaluated, CPU, wall, and peak RSS.

Pre-registered hypotheses (campaign `2026-10-05-phase10-governor-d2b09c9`):

| hypothesis | verdict |
|---|---|
| **H1** `DsfbGuided.final ≤ FixedHeuristic.final` for every workload | **HELD** |
| **H2** on ≥80 % of holdout, `guided.final == exhaustive.final` with ≤ ½ the candidates | **HELD** (8/8 holdout) |
| **H3** honest failure — no measurable byte benefit | **HELD** (`fixed == exhaustive` on **every** workload; median benefit 0 ‰) |
| **H4** negative controls `Stop(Raw)` and match RAW byte-for-byte | **HELD** |

Representative rows (`final` bytes / candidates evaluated / wall):

| workload | src | fixed | exhaustive | guided |
|---|---:|---|---|---|
| `flate.pdf` | 57,513 | 36,161 / 10 / 37 ms | 36,161 / 438 / 2,014 ms | 36,161 / 150 / 527 ms |
| `bigtext.pdf` | 65,549 | 38,274 / 7 / 27 ms | 38,274 / 414 / 2,136 ms | 38,274 / 126 / 487 ms |
| `many.pdf` | 9,881 | 5,301 / 7 / 6 ms | 5,301 / 414 / 401 ms | 5,301 / 126 / 106 ms |
| `synthetic-raw` | 8,192 | 8,646 / 3 | 8,646 / 234 | 8,646 / 3 |

**The honest outcome (risk R1 in the frozen contract).** The fixed heuristic
already attains the exhaustive minimum on **every** workload in this cohort: the
parametric space adds **zero** bytes. H2 also holds, but only because the grid's
byte minimum is attained by the maximal configs and the governor reaches them. The
substantive result is **H3**: *no measurable byte benefit; the fixed complete-cost
court is retained and the governor remains an optional encoder feature.*

## Consequences

- **Zero decode authority is proven, not asserted.** A governor-produced
  descriptor for `many.pdf` (winner `BYTE_RANS`) was decoded by the **default**
  build (no `dsfb-search`) and compared byte-exactly; a `flate.pdf` descriptor
  (winner `PDF_DEFLATE_REPLAY_RANS`) was decoded by a build with the underlying
  `deflate-replay` capability and still *no* `dsfb-search`. A grep gate asserts
  `src/{materialize,container,dra,store}` reference neither `governor` nor
  `crate::encode`; `header.rs` references no governance.
- **`dsfb` is recorded as inspected, MSRV-OK, transitively present,
  unavailable-for-purpose.** `dsfb-search` does not depend on it. No floating
  point and no RNG enter the encoder's decision path.
- **No win is claimed.** The mechanism is correct, exact, deterministic, and
  bounded, but on this cohort it buys nothing: the fixed heuristic is already
  optimal. It is retained (feature-gated, encoder-only) as a substrate for future
  search work, with the negative recorded rather than hidden.
- **Scope.** The cohort is small and locally generated; **no population claim** is
  made. H2 is reported only on the holdout set.

## References

- `evidence/campaigns/2026-10-05-phase10-governor-d2b09c9/` (`manifest.json`,
  `environment.json`, `workloads.json`, `results.json`, `hypotheses.json`,
  `decode-proof.json`, `court-table.txt`, `report.md`, `gates.txt`)
- `research/subagents/phase-10/dsfb-contract.md` (frozen design contract)
- `src/encode/governor.rs`, `examples/governor_court.rs`, `tests/governor.rs`,
  `tools/governor-court.sh`
- ADR-0008 (`dsfb` is a non-optional dependency of the *optional* EntropyFS
  engine only), ADR-0017 (whole-file loss), ADR-0013 (layout+rANS negative)
