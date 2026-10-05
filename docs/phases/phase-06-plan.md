# Phase 6 — Exact DEFLATE replay (`preflate-rs`)

Branch: `phase6`. Base: `main` @ `e067294` (`v0.1.0-alpha.7`).
Status: **IN PROGRESS**.

## Why this phase

Phases 4 / 5 / 5.7 / 5.8 established a converging negative: at the tested scale,
PDF structural proceduralization of *plain* PDF syntax does not beat a
whole-file order-0 byte-rANS lane (`BYTE_RANS`). Phase 6 attacks a **different
layer**: bytes that are *already entropy-coded* by the producer. A
`/FlateDecode` stream is (to an order-0 model) near-random, so no amount of
order-0 modelling helps the raw stream bytes. Exact DEFLATE replay replaces
those bytes with `(plaintext, correction state)` and regenerates the original
deflate bitstream deterministically — a representation the whole-file lane
cannot express.

This is the strongest existing-prior-art match to the VOLE thesis: `preflate`
reconstructs the *original* DEFLATE bitstream from plaintext plus compact
correction state. It is a **candidate**, never a blanket transform.

## Frozen contract

### Wire / semantics

- New DRA op `DEFLATE_REPLAY` (opcode `0x0A`), DRA version **7**:

  ```text
  Op::DeflateReplay { plaintext_object: u32, corrections_object: u32,
                      declared_output_len: u32 }
  ```

  Semantics: emit exactly `recreate_whole_deflate_stream(plaintext, corrections)`
  — the **raw** DEFLATE bytes (RFC 1951), no zlib wrapper. The op carries
  `declared_output_len` (like `PACKED_CHANNELS`) so the coverage certificate can
  bound the output statically; evaluation must produce exactly that many bytes
  or fail with a typed error.

- `preflate-rs` expects **raw DEFLATE** (verified: a full zlib stream returns
  `Err(NonZeroPadding)`; the zlib 2-byte header + 4-byte Adler-32 trailer must be
  stripped by the caller and re-emitted as literal program bytes).

- Reconstruction can **panic** on hostile correction data (verified: index-OOB /
  `panic!` on mutated blobs) *and* can return `Ok` with wrong bytes. Therefore
  the evaluator wraps `recreate_whole_deflate_stream` in
  `std::panic::catch_unwind` and converts `Err`/panic into a typed
  `Error::codec_replay(...)`; the whole-source SHA-256 court remains the final
  authority on correctness.

- A mandatory feature bit `FEATURE_DEFLATE_REPLAY = 1 << 0` is declared in the
  header whenever any `DeflateReplay` op is present. `SUPPORTED_MANDATORY_FEATURES`
  includes it only when the `deflate-replay` crate feature is compiled. Unknown
  mandatory bits fail closed. `--no-default-features` can still *parse* a
  descriptor but must reject evaluation of the op with `UnsupportedFeature`
  (never silently reinterpret).

- Universe string becomes:
  `vole-document;universe;phase6;exact-bytes;dra-7;opaque+entropy+pdf+channels+offsets+packed+packed-channels+deflate-replay`

> **Post-implementation note (Phase 6.7).** This frozen contract records the
> Phase-6 plan as written (DRA v7). The shipped op is now DRA **v8** with a
> `replay_codec` tag and the universe
> `…;dra-8;…;deflate-replay-preflate-0.7.6-experimental`, plus a static decode-time
> resource bound. See ADR-0007, ADR-0014, ADR-0016, and `SPEC.md` for the current
> normative description.

### Encoder analysis (never authority)

- Flate discovery uses **VOLE's own** scanned stream spans (`PdfStreamSpan`)
  plus a `/Filter` classification read from the object's leading dictionary — not
  generic preflate container scanning. Only a lone `FlateDecode` filter (a bare
  name or a single-element array) is eligible; filter chains and other filters
  are declined.
- Each eligible stream: verify the zlib framing `(CMF*256+FLG) % 31 == 0`,
  strip 2 + 4 bytes, run preflate with a bounded `PreflateConfig`
  (`plain_text_limit` derived from `Limits`; `max_chain_length` pinned;
  `verify_compression = true`), require `chunk.compressed_size == raw.len()`, and
  require `recreate_whole_deflate_stream(...) == raw` **before** the stream may
  enter the document-level size court. Preflate analysis is itself wrapped in
  `catch_unwind` (a debug-build `u16` overflow inside preflate is possible on
  very large highly-repetitive data); a panic means "decline this stream".
- The zlib 2-byte header and 4-byte Adler-32 trailer are emitted as literal
  `INLINE` bytes; only the raw DEFLATE middle is replayed. Adler-32 *regeneration*
  is a possible later subphase; it is not assumed.

### Candidates

- `PDF_DEFLATE_REPLAY` — physical span order; each eligible Flate stream span
  becomes `INLINE(header) · DEFLATE_REPLAY · INLINE(adler)`, all other spans
  stay `INLINE`; plaintext and corrections are raw `OBJECT`s.
- `PDF_DEFLATE_REPLAY_RANS` — same, but the plaintext of each stream is carried
  as its own order-0 byte-rANS `ENTROPY_CHANNEL` (corrections stay raw objects).

Both are the only candidates added; the court decides.

### Never

- Do not use a PDF writer. Do not renumber, normalize, or resave.
- Do not let `preflate` decide stream boundaries (VOLE's span authority does).
- Do not make `preflate` a decode-time *search*: decode only replays the exact
  stored corrections.
- Do not claim savings from entropy estimates; only complete serialized bytes.

## Gates

1. Bit-exact per-stream replay: `recreate_whole_deflate_stream(plain, corr) == raw`.
2. Whole-document exactness: `materialize(D) == source` (length + SHA-256 + byte
   compare), enforced by decode-before-commit in the court.
3. Hostile input: a mutated `.voldoc` containing a `DEFLATE_REPLAY` op never
   panics or hangs; it returns a typed error or a reconstruction mismatch.
4. Honest complete-cost: the candidate is adopted only if it beats the incumbent
   on serialized `.voldoc` bytes; ties/declines recorded verbatim.

## Subphases

- **6.0** plan + branch. *(this file)*
- **6.1** dependency + feature wiring; `adapter/pdf/deflate` safe wrapper
  (bounded analyze/replay, `catch_unwind`); unit round-trip on generated
  fixtures; MSRV build.
- **6.2** `/Filter` classification on `PdfStreamSpan` (`cos::dict_filter`).
- **6.3** `DEFLATE_REPLAY` op (DRA v7) + analysis + evaluator; universe bump;
  hostile-input tests.
- **6.4** `PDF_DEFLATE_REPLAY` candidate + Flate-bearing PDF fixtures +
  exactness + forced-candidate court.
- **6.5** `PDF_DEFLATE_REPLAY_RANS` variant (plaintext as rANS channel).
- **6.6** court script + campaign receipt + ADR + docs + `0.1.0-alpha.8` freeze.

## Explicitly out of scope (later phases)

Nested content-stream proceduralization of the plaintext (Phase 7); grammar /
templates (8); cross-document plaintext sharing and EntropyFS (9); DSFB search
governance (10).
