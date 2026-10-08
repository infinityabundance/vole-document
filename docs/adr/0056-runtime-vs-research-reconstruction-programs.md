# ADR-0056: The research/codec programme and the runtime DocumentField programme are distinct, and must not be conflated

- **Status:** Accepted — clarification of scope and IP/presentation (Phase 22 planning)
- **Date:** 2026-10-08

## Context

An external technical review (see [Phase 22 plan](../phases/phase-22-plan.md))
raised one **architectural and presentational** concern (§13): the repository
contains **two different programmes** whose claims are easy to mix, and mixing
them misstates both what is novel and what is being measured.

1. **The research / codec programme.** The `encode` path prices a **candidate
   portfolio** — PDF DEFLATE replay, `BYTE_RANS`, RLE, RAW, and the structural
   candidates — and selects the smallest exact program by the complete-cost
   court. Its claims are about **compression / representation cost**: how many
   bytes a reconstruction program needs. Its history is dominated by **recorded
   negatives** (ADR-0010/0011/0013/0017/0021/0028/0036/0037/0046/0047).

2. **The runtime DocumentField programme.** `field-build --profile runtime`
   fixes the exact authority to the literal **`RAW`** floor and, in the same
   process, builds the **native observation graph** (spans, objects/streams/
   pages, package members, resources, the hierarchical index) by **re-scanning
   the materialized source** ([ADR-0051](0051-direct-field-ingestion.md)). Its
   claims are **not** about compression.

The load-bearing facts that make these distinct:

- In the runtime profile **exactly one candidate is priced** and it is the
  **`RAW`** program, yet the observation surface is **unchanged**, because
  structure is a function of the **source bytes**, re-scanned by Stage B / package
  ingest — not of the winning program ([ADR-0051](0051-direct-field-ingestion.md);
  the one recorded caveat is that the PDF metadata projection's
  `object_count`/`graph_ops` counters are encoder-dependent).
- The candidate-portfolio search is therefore **pure overhead for a runtime
  ingest** — the same source is scanned and the same observations produced no
  matter which exact program won.
- **Exact reconstruction of an intact `RAW` source is the entry condition, not
  the result.** It is the correctness floor every lane must satisfy
  (`materialize(descriptor) == original_bytes`, length + SHA-256 + `cmp`); it is
  **not** a claim that `RAW` is a novel or compact reconstruction program.

## Decision

1. **Name and keep the two programmes separate.** The **research/codec
   programme** owns the candidate-portfolio search and every claim about
   compression or representation cost. The **runtime DocumentField programme**
   owns the persistent field, the observation graph, and the runtime build; its
   exact authority is the fixed `RAW` program.

2. **State plainly what the runtime is not claiming.** The runtime's economic
   mechanism is **not** "`RAW` is a novel compact reconstruction program". Its
   mechanism must **stand on its own merits**:
   - the **persistent dependency structure** (ADR-0025),
   - **typed exactness and provenance contracts** (ADR-0024/0026),
   - **selective materialization** (ADR-0018/0019/0039),
   - **format-native access** (ADR-0029/0031),
   - and the **cost of providing many observation types** at once, each with its
     own basis and spans.

3. **Do not conflate them in the IP story or in diligence.** A diligence or
   novelty statement about the runtime must not lean on the codec programme's
   (mostly negative) results, and a statement about the codec programme must not
   borrow the runtime's capability claims. The two have **different evidence,
   different gates, and different claim types**.

4. **Phase 22 is measured under this distinction.** The economic programme
   ([Phase 22 plan](../phases/phase-22-plan.md)) targets a **capability/cost
   frontier** for the runtime against the strongest honest competitor — not a
   whole-file compression win, which is a recorded loss on the codec axis
   (ADR-0017).

## Consequences

- The runtime build's measured wins (build time, peak RSS, storage at contract
  parity; [ADR-0051](0051-direct-field-ingestion.md), ADR-0053, Phase 19/20) are
  attributed to **removing non-load-bearing work and to the observation
  substrate**, not to a codec.
- The codec programme's negatives stay **published and attributed to the codec
  programme**; they are not a verdict on the runtime's capability claims, and the
  runtime's claims are not a vindication of the codec (ADR-0050's discipline,
  extended: neither programme is vindicated by the other).
- **Falsifier.** If any document, README, or diligence artefact ever describes
  the runtime build as a compression/representation-cost result, or cites the
  codec negatives as if they measured the runtime, this ADR has been violated.
- No wire byte, decode path, `encode` output, or observation contract changes.
  This is a **scope and presentation decision**, not a mechanism.

## References

- [Phase 22 plan](../phases/phase-22-plan.md) (the economic programme; the
  pointer from the plan to this ADR)
- [ADR-0051](0051-direct-field-ingestion.md) (`field-build --profile runtime` =
  fixed `RAW`; structure re-scanned from the source)
- ADR-0017 (whole-file compression is a recorded loss); ADR-0023 (consolidated
  findings); ADR-0050 (the SQLite-as-substrate open question — no more a codec
  question than this one)
- ADR-0024/0025/0026 (field authority, seed DAG, observation/provenance model);
  ADR-0029/0031 (multi-format layers, shared vocabulary)
- [`FINDINGS.md`](../project/findings.md) (the codec programme's top-level verdict)
