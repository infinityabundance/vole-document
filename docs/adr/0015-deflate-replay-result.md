# ADR-0015: Exact DEFLATE replay wins on shared plaintext with a large/weakly-coded appearance

- **Status:** Accepted — first measured positive for a PDF structural candidate
- **Date:** 2026-10-05

## Context

Phases 4 / 5 / 5.7 / 5.8 converged on a negative: at the tested scale, PDF
structural proceduralization of *plain* PDF syntax does not beat a whole-file
order-0 rANS lane (`BYTE_RANS`). ADR-0007 had already frozen the Phase-6 plan to
attack a **different layer**: bytes the producer has *already* entropy-coded.
A `/FlateDecode` stream is near-random to an order-0 model, so no amount of
byte modelling helps the raw stream bytes; exact DEFLATE replay replaces them
with `(plaintext, correction state)` and regenerates the **original** deflate
bitstream deterministically — a representation the whole-file lane cannot
express.

Phase 6 implemented that plan:

- the `DEFLATE_REPLAY` DRA op (opcode `0x0A`), bumping the DRA graph to
  **version 7** and moving the universe to
  `phase6;exact-bytes;dra-7;opaque+entropy+pdf+channels+offsets+packed+packed-channels+deflate-replay`.
  It reconstructs exactly `recreate_whole_deflate_stream(plaintext,
  corrections)` for the raw DEFLATE middle of a stream, with a
  `declared_output_len` validated at evaluation;
- stream discovery by VOLE's own byte-authoritative physical scanner plus a lone
  `/FlateDecode` classification — `preflate` never decides stream boundaries;
- two candidates: `PDF_DEFLATE_REPLAY` (plaintexts are raw, content-deduplicated
  `OBJECT`s) and `PDF_DEFLATE_REPLAY_RANS` (each unique plaintext is its own
  order-0 byte-rANS `ENTROPY_CHANNEL`, shared by every stream that produces it,
  so shared plaintext is stored once);
- `preflate` reconstruction and analysis are wrapped in `catch_unwind` and
  bounded; the whole-source SHA-256 court remains the final authority;
- a mandatory `FEATURE_DEFLATE_REPLAY` bit, so a build without the feature fails
  closed rather than reinterpreting the op.

The `flate.pdf` sample is a composed-but-real case: six real zlib streams
(`p1` at levels 9, 6, 1, and 0; a distinct `p2`; a distinct `p3`), with one
plaintext (`p1`) shared across four of them and one stream stored at level 0
(weak producer coding).

## Outcome

**Implement both lanes, keep them exact, and adopt the rANS-plaintext variant as
a candidate that can win.** Unlike every earlier PDF structural candidate, exact
DEFLATE replay of a shared, entropy-coded plaintext that *also* includes a
large/weakly-coded appearance **beats `BYTE_RANS`** on the measured fixture.

Measured campaign `2026-10-05-phase6-ec92c1a` over the deterministic 12-file
corpus (verdict PASS; every auto winner round-trips byte-exactly through
`cmp` + `verify`, `all_exact=true`):

- **On `flate.pdf` (57,513 B): `PDF_DEFLATE_REPLAY_RANS` = 36,068 B, a 13,195 B
  win over `BYTE_RANS` = 49,263 B** (and a 21,812 B win over RAW = 57,880 B). The
  rANS lane is the auto winner and the only file on which a replay lane is even
  proposed.
- **The raw-plaintext variant loses.** `PDF_DEFLATE_REPLAY` = 56,702 B, 7,439 B
  *larger* than `BYTE_RANS`, because the plaintext of a strongly-compressed
  stream is nearly as large as the stream bytes it replaces, and the plaintext
  is expensive to store literally even after object deduplication.
- **The ladder moves for the first time.** A0 RAW = 139,614; A2 +`BYTE_RANS` =
  98,224; A6 +`PDF_LAYOUT_RANS` = 98,224; A7 +`PDF_DEFLATE_REPLAY` = 98,224;
  A8 +`PDF_DEFLATE_REPLAY_RANS` = **85,029** (step delta −13,195). The
  **leave-one-out delta for the rANS replay mechanism is −13,195 B**; the
  raw-replay and layout+rANS leave-one-out deltas are 0.
- **Head-to-head vs `BYTE_RANS`: win 1, lose 0, decline 11.** The other eleven
  files have no lone `FlateDecode` stream and decline the lane, which is
  recorded verbatim and never scored as a win.
- **The winner's cost is dominated by its one channel.** The `flate.pdf`
  breakdown is header 64 / universe 121 / format 70 / record framing 204 /
  objects 639 / graph 1,498 / models 689 / entropy payload 32,723 / integrity 40
  / trailer 20 = 36,068. The descriptor replays all six streams
  (`streams=6 replayed=6`) and codes their three unique plaintexts as three
  order-0 channels (`channels=3 objects=6`); only `p1` is shared (across four
  streams), while `p2` and `p3` are unique.

## Reasoning

- **Shared plaintext is stored once and decoded once.** Only `p1` is shared: it
  appears in four streams at levels 0/1/6/9. The rANS lane codes `p1` exactly
  once as an order-0 channel and every stream that produces it references that
  channel; the materializer decodes each channel once. `p2` and `p3` are unique,
  so the descriptor exposes 6 streams but only **3 unique plaintexts**
  (`streams=6 replayed=6 channels=3 objects=6`), against the six deflate streams
  the whole-file lane carries.
- **Weak producer coding is re-expressed only when it rides on a shared
  plaintext.** Of `p1`'s four appearances exactly one is weakly coded: level 0
  is stored (~verbatim, 31,998 B); level 1 is only ~19% of the plaintext
  (6,197 B), and levels 6 and 9 are strong (3,768 B / 3,410 B). Order-0 rANS
  over the shared plaintext is cheaper than carrying the large stored
  appearance, and sharing makes that saving available to every stream that
  reproduces the plaintext. The large/weak appearance alone does not win (a
  unique level-0 stream loses), and sharing alone does not win (four strong
  level-9 appearances lose), so the win requires both — see the negative
  controls below.
- **The correction state pays for itself.** The stored correction blobs plus two
  literal zlib fragments (header + Adler-32, 6 B each) are small next to the
  bitstream bytes they replace; the graph and object framing (1,498 + 639 B) is
  the fixed container cost, and it is more than covered by the 13,195 B saved.
- **This is a different layer from the converging negatives.** Phases 4–5.8 all
  proceduralize *plain* syntax that `BYTE_RANS` already models well. Exact replay
  proceduralizes bytes that are *already entropy-coded* — the region where an
  order-0 model is helpless — and the win comes from that substitution, not from
  modelling the stream bytes.

## Scope — honest characterization

These are measured, scoped results for *one synthetic composed fixture*, at
commit `ec92c1a`. They are **not** a general compression claim:

- **Winning region (conjunctive):** a plaintext that is **shared across
  streams** **and** whose appearances include at least one **large/weakly-coded**
  stream, so that order-0 rANS of the shared plaintext costs less than the sum of
  the compressed appearances it replaces. Neither property alone wins. Here
  `flate.pdf` has both, so the lane wins; the negative controls below show each
  conjunct alone losing.
- **Losing region:** any case missing either conjunct — **unique** plaintext, or
  a shared plaintext whose appearances are all **strongly compressed**. Replaying
  a stream whose plaintext is essentially as large as its bitstream, storing the
  plaintext raw, and paying the correction and graph overhead loses to carrying
  the bitstream — which is exactly the `PDF_DEFLATE_REPLAY` result. Replay loses
  by up to **4.46×** on a single unique strongly-compressed stream. A corpus of
  many small, maximally-compressed, non-repeating streams is expected to lose.
- **The corpus is composed.** `flate.pdf` is assembled deterministically with
  correct `/Length` and offsets; its streams are real zlib output, but the
  document is synthetic. Only one sample exercises the lane, so the win is
  demonstrated on a single composed case and the boundary between the winning
  and losing regions is characterized structurally, not by a population.
- **The winner is always decided by actual serialized bytes** in the
  complete-cost court, and every auto winner round-trips byte-exactly; no saving
  is claimed from an entropy estimate or from bitstream length alone.

## Falsification / negative controls

An independent adversarial reviewer (Phase 6) independently reproduced the
following controls in Docker and falsified an earlier "shared **or** weakly
coded" reading of the winning region. Each uses the same complete-cost court;
"loses" means `PDF_DEFLATE_REPLAY_RANS` is larger than `BYTE_RANS`:

| fixture | `BYTE_RANS` | replay+rANS | outcome |
| --- | ---: | ---: | --- |
| single unique level-9 stream | 4,476 | 19,958 | replay loses 4.46× |
| single unique level-0 stream | 19,418 | 19,656 | replay loses (weak coding **alone** loses) |
| four identical level-9 streams (shared, strong) | 13,914 | 20,403 | replay loses (sharing **alone** loses) |
| `flate.pdf` with the level-0 stream replaced by a level-6 stream | 29,375 | 36,027 | replay loses (strong appearances) |

The win therefore requires **both** conjuncts simultaneously: the plaintext must
be shared across streams **and** at least one of its appearances must be
large/weakly coded. This boundary is characterized on synthetic fixtures, not by
a population (see also `docs/evidence/phase6-skeptic-review.md`).

## Consequences

- **`PDF_DEFLATE_REPLAY_RANS` is `ADOPTED` as a winning candidate** for inputs
  with lone-`FlateDecode` streams; it is proposed for every such input and can
  win the complete-cost court when the conjunctive condition above holds.
  `PDF_DEFLATE_REPLAY` (raw plaintext) stays implemented and available but
  is `RECORDED (rejected vs BYTE_RANS)` on the measured case.
- **The lexer change is load-bearing.** `stream` + EOL payloads are now opaque
  spans, so the scanner's `/Filter` classification and exact stream-data spans
  drive replay; `preflate` never discovers streams. This did not regress any
  corpus file (all 12 round-trip byte-exactly).
- **The baseline remains `BYTE_RANS`.** A replay lane must beat `BYTE_RANS`, not
  RAW. This ADR records the first PDF structural candidate that does.
- **Next lever.** Nested content-stream proceduralization of the plaintext
  itself (Phase 7), and cross-document plaintext sharing (EntropyFS form, Phase
  9), are the natural successors: the win here comes from sharing and
  re-coding the plaintext, and both are directions a successor phase would test
  rather than established general mechanisms.
- **Dependency note.** The `deflate-replay` feature transitively pulls
  LGPL-3.0-or-later `cabac`; see ADR-0014. The feature is optional; a build with
  `--no-default-features --features rans` omits the lane entirely and rejects the
  op with `UnsupportedFeature`.
- **Preserve the evidence.** Campaign `2026-10-05-phase6-ec92c1a` (results,
  cumulative and leave-one-out ablations, verification triples, negative
  controls, qpdf oracle) remains under `evidence/campaigns/`.

## References

- ADR-0007: exact DEFLATE replay is a per-stream candidate
- ADR-0013: layout + rANS does not beat whole-file order-0 rANS (the prior
  converging negative)
- ADR-0014: `preflate-rs` pulls LGPL-3.0-or-later `cabac`
- Campaign `2026-10-05-phase6-ec92c1a`
- `tools/phase6-court.sh`; `tests/pdf_deflate.rs`
