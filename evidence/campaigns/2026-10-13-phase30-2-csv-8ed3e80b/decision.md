## Decision

- **The fused CSV kernel is faster than pyarrow on the same work (parse + aggregate):** median fused/pyarrow(read-inclusive) = **0.362** (< 1.0).
- Against the Phase-30.1 *aggregation-only* pyarrow lane (continuity), fused/pyarrow = 9.710.
- The serial scanner (`--workers 1`) against pyarrow read-inclusive is 0.805, so the parallel split supplies the remaining margin.
- Exactness: 3/3 fixtures byte-exact after the source and descriptor are deleted.
- Oracle agreement: ok (52 comparisons).
