# ADR-0041: Sharing the PDF scan across the candidate portfolio (partial fix of the >100 MiB encode bound)

- **Status:** Accepted — partial fix; the peak-memory bound is recorded, not solved (Phase 14)
- **Date:** 2026-10-07

## Context

The `real100-v1` frontier court recorded a VOLE failure region: encoding the
largest PDFs (168–409 MB) either timed out (rc 124) or was OOM-killed (rc 137)
under the `doc-baseline` lane (6 GiB, 180 s per op). A first post-hoc
investigation (`tools/phase14-*`) traced it:

- A single physical `scan` of a 217 MB scanned PDF is **~37 s wall / 353 MB peak**,
  and the cost is **not** `lex` (97 ms) but the ~23,447 spurious objects the
  scanner derives from image bytes.
- The **portfolio ran that scan 5×** (each PDF proposer re-scanned), plus an
  `O(streams × bodies)` linear `lookup_body`, plus an `O(spans)`-per-`<<`
  `matching_dict_close`.
- The auto-court (`propose_all` + `court::run`) **built every candidate's payload
  at once** before pricing any.

## Decision

Three behaviour-preserving changes (same candidate set, same order, same bytes,
same court outcome; the tests pin this):

- **One shared PDF scan.** `propose_pdf`, `propose_pdf_channels`,
  `propose_pdf_layout(_rans)` and `propose_pdf_length_revision` gain
  `*_with(input, limits, &PdfPhysical)` variants; `propose_each` computes
  `physical::scan` **once** and passes it to each. The public `propose_*(input)`
  keep scanning internally for other callers.
- **`O(1)` body lookup.** `lookup_body` (linear over 23,447 bodies, called per
  stream) becomes a `(number, generation) -> (body_lo, body_hi)` `HashMap` built
  once (`build_body_index`, first-wins to match the linear scan).
- **`O(1)` dict matching.** `matching_dict_close`'s forward scan is replaced by a
  one-pass stack table (`DictMatch`), so an unmatched `<<` in binary data cannot
  make the scan quadratic.
- **Streaming court.** `court::Court` prices candidates one at a time
  (`offer`/`finish`), and `propose_each` streams the portfolio, so only the
  current best plus the candidate being priced stays resident.

## Consequences

- **Time improves ~1.75× on the large region.** `nasa-pdf-0003` (217 MB,
  all-features): **395 s → 227 s**; `nasa-pdf-0002` (308 MB) went from a
  180 s **timeout** to a completed **221 s** encode (byte-exact, `BYTE_RANS`).
- **The peak-memory bound is NOT fixed.** Peak RSS is unchanged (~3.75 GiB for a
  217 MB input, ~3.75 GiB even for `--force raw`, and a single `scan` is only
  353 MB) — so the ~17× resident growth is in a candidate **generator** or in
  `physical` retained across generators, not in the portfolio or the court. A
  409 MB input therefore still exceeds the 6 GiB cap. This is **recorded as an
  open negative**: the mechanism is narrowed but not closed, and the court's
  gates are not crossed (227 s still exceeds the court's 180 s op budget too).
- **No decoder/wire change.** All changes are encoder-side; `materialize ==
  source` is unchanged and no `.voldoc` byte changes for a given input.

## Next step (recorded, not done)

Attribute the ~17× peak: bisect a single `propose_*` generator's allocations
(`ByteRans`, `PdfChannels`, `PdfPhysical`'s per-span `Inline` Vecs, or the
`deflate-replay` proposers) with a peak-memory instrument per generator, then fix
the dominant one. Only then raise the court's op budget (a deliberate act) or the
lane cap.

## References

- `src/adapter/pdf/physical.rs` (`DictMatch`, `build_body_index`),
  `src/adapter/pdf/{adapter,layout,length_revision}.rs` (`*_with`),
  `src/encode/candidates.rs` (`propose_each`), `src/encode/court.rs` (`Court`)
- `tools/phase14-large-pdf-court.sh`; `evidence/campaigns/*-phase14-*`
- `docs/evidence/real100-frontier-report.md` (the finding)
