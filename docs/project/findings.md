# VOLE-Document — consolidated findings

**Status:** authoritative top-level summary of the VOLE-Document programme, superseding
the per-phase narratives. Decision record: [ADR-0023](../adr/0023-consolidated-findings.md).
**Version:** `0.1.0-alpha.12` (Phase 10.2) — this document is **frozen at Phase 10.2**.
For Phase 11 and Phase 12 see **§9** (the current release is `0.1.0-alpha.16`,
Phase 12); the authoritative Phase-12 evidence is
[`docs/phases/phase-12-results.md`](../phases/phase-12-results.md).

This document states, in one place, **what was built, what was measured, against
which baseline, what won, what lost, and why.** Every number below is traceable to
a sealed receipt under [`evidence/campaigns/`](../../evidence/campaigns/) and to an ADR
in [`docs/adr/`](../adr/). Where a claim was over-stated by earlier text, the
correction and the independent review that found it are cited in §6. Nothing here
is a population claim; every corpus was locally generated and deterministic.

Headline verdict (ADR-0023):

> **The current VOLE representation stack does not beat purpose-built baselines on
> any measured axis.** The durable results are byte-exactness, an auditable
> reconstruction representation, and a complete, receipted record of the
> negatives.

---

## 1. What this is

VOLE-Document is a **reversible representation stack** for a document's bytes:

```text
source bytes
  → deterministic structure        (Document Reconstruction Algebra, DRA)
  + typed residual channels        (literal / lexical / layout / replay residuals)
  + entropy-capsule channels       (order-0 byte-rANS channels, one model each)
  → exact materialization          (materialize(descriptor) == original_bytes)
```

The governing invariant of the **exact profile** is uncompromising and is checked
by three independent courts on every admitted input:

```text
materialized_length   == source_length
SHA256(materialized)  == SHA256(source)
byte_compare(materialized, source) == equal
```

Parsing successfully, producing "the same" text/object graph/pages/rendering, or a
canonical re-save are **not** substitutes. Parsing, semantic equality,
canonicalization, rendering, and re-saving are never archival equality.

- **Entropy is a substrate, not the model.** rANS is the coding layer *beneath*
  the representation; it is never the procedural model. The project is
  deliberately not "a PDF optimizer that happens to use rANS".
- **A "seed" is a full capsule.** An entropy channel carries its model, counts,
  decoder-entry state, and renormalization payload. "rANS state alone
  reconstructs arbitrary data" is false and is not claimed.
- **Bounded, non-Turing-complete.** The DRA is a literal reconstruction program
  with a checked coverage certificate: it rejects gaps/overlaps before allocation,
  has no loops or recursion, and cannot run away.
- **Oracles are not authority.** qpdf/Poppler/MuPDF are differential oracles;
  PDF physical *bytes* are the authority.

---

## 2. What was built

One crate, module-separated. The work proceeded phase by phase on branch `phaseN`,
released as annotated tags. (An `0.1.0-alpha.1` CHANGELOG section exists for the
Phase-1 exact core, but **no `v0.1.0-alpha.1` git tag is present**; the tag lineage
begins at `v0.1.0-alpha.2`. Verified with `git ls-remote --tags origin`.)

| Phase | Tag | What was built |
|---|---|---|
| 1 | *(no tag; CHANGELOG section only)* | **Exact container + DRA.** Length-delimited CRC-32C records, typed errors + stable exit codes, centralized resource limits, SHA-256 archival identity, bounded literal DRA (`EMIT_OBJECT`/`INLINE`/`REPEAT_LAST`), checked coverage certificate, RAW exact opaque adapter, complete-cost court with decode-before-commit. |
| 2 | `v0.1.0-alpha.2` | **rANS floor.** Native order-0 / typed byte-channel rANS; RLE candidate; `BYTE_RANS` candidate; the entropy *capsule* (full decoder-entry state). |
| 3 | `v0.1.0-alpha.3` | **PDF physical authority.** Byte-authoritative lexical span cover + structural scanner, `/Length` resolution, incremental revision map (`/Prev` chain), object-role detection; qpdf differential oracle (oracle, never authority). |
| 4 | `v0.1.0-alpha.4` | **Typed lexical channels** (`split`/`join`, `INTERLEAVE_CHANNELS`, DRA v3) + compact entropy model wire v2. Measured and **rejected** on complete cost (ADR-0010). |
| 5 | `v0.1.0-alpha.5` | **PDF layout prediction** (`MARK_OFFSET`/`EMIT_OFFSET`, DRA v4). Exact but **rejected** on DRA framing cost (ADR-0011). |
| 5.7 | `v0.1.0-alpha.6` | **Packed framing** (`PACK_SEGMENTS`, DRA v5). Layout-v2 beats RAW at scale but still loses to `BYTE_RANS` (ADR-0012). |
| 5.8 | `v0.1.0-alpha.7` | **Layout + rANS** (`PACKED_CHANNELS`, DRA v6). Exact; **rejected** vs `BYTE_RANS` (ADR-0013). |
| 6 | `v0.1.0-alpha.8` | **Exact DEFLATE replay.** `DEFLATE_REPLAY` DRA op (DRA v8, explicit `replay_codec` tag); `PDF_DEFLATE_REPLAY` (raw plaintext) and `PDF_DEFLATE_REPLAY_RANS` (shared rANS plaintext). Opt-in `deflate-replay` feature pulls **LGPL-3.0-or-later `cabac`** (ADR-0014); decode-time output bounded by the VOLE replay-profile admission limit (ADR-0016). |
| 7 | `v0.1.0-alpha.9` | **Generic-baseline reckoning + partial materialization + fuzzing.** gzip/zstd/xz/brotli baseline ladder (ADR-0017); `OBSERVATION_INDEX` + `view` (ADR-0018); ten coverage-guided `cargo-fuzz` targets; **process-isolated DEFLATE replay** (`__replay-worker` under `RLIMIT_AS` + timeout) containing fuzz finding F2 (ADR-0016). |
| 8 | `v0.1.0-alpha.10` | **Observation views + seek directory.** Optional seek `DIRECTORY` record (`seek_directory_v1`, tag `0x71`) + `Read + Seek` reader; `view` peeks only the fixed 64-byte header and never `fs::read`s the whole descriptor (ADR-0019). |
| 9 | `v0.1.0-alpha.11` | **Content-addressed store.** `ObjectStore`/`EmbeddedStore`, `EXTERNAL_REF` (`0x80`, mandatory `FEATURE_EXTERNAL_OBJECTS`), `externalize`/`hydrate`/`gc`, three accounting universes; optional `EntropyFsStore` adapter (ADR-0020). |
| 10.1 | *(unreleased until `alpha.12`)* | **Encoder-only governor.** `dsfb-search = []` (dependency-free, **not** `dep:dsfb`): typed integer-only residual diagnostics, a tiny parametric space over existing mechanisms, a pure `govern(&ResidualTrace)`, and a court. Zero decode authority (ADR-0022). |

---

## 3. What was measured, and against which baseline

Every number names the baseline it was measured against and the receipt that
sealed it. "Exactness" is baseline-independent: the baseline is the source bytes
themselves, checked by length + SHA-256 + byte compare.

### 3.1 Headline axes

| Axis | Baseline (what it is measured *against*) | Result | Verdict | Receipt | ADR |
|---|---|---|---|---|---|
| **Exactness** (all phases) | the source bytes (`length` + `SHA-256` + `byte_compare`) | every admitted input in every sealed campaign round-trips byte-exactly (e.g. 27/27 Phase 7.0c, 18/18 Phase 7.3, 18/18 Phase 8, 37/37 Phase 9, all Phase-10 workloads) | **HELD** (prime directive) | all campaigns | 0001 |
| **Whole-file size** | best of `gzip -9`, `zstd -19 --long=27`, `xz -9e`, `brotli -q 11` (each round-trip verified) | best VOLE lane beats a generic compressor on **0/27** files; best generic smaller on every file by **+460,320 B** total | **LOSS** | `2026-10-05-phase7-baselines-7b9f662` | [0017](../adr/0017-generic-lossless-baselines.md) |
| **Random-access bytes read** | **seekable/blocked** formats: bgzip (BGZF 64 KiB), `xz --block-size=64KiB/1MiB/4MiB`, pixz (16 MiB) | late query VOLE **460,713 B** loses to **fine-block** formats (BGZF **23,808 B** ~19×, xz-64KiB **15,344 B** ~30×, xz-1MiB **179,892 B** ~2.6×) but reads **less** than **coarse-block** ones (xz-4MiB **708,612 B**, pixz **2,810,832 B**) | **MIXED (loss vs fine-block)** | `2026-10-05-phase8-seek-08de2a9` (`seekable.jsonl`) | [0019](../adr/0019-seek-based-io.md) |
| **Random-access bytes read** | **non-seekable sequential** gzip/zstd/xz compressed prefixes | late query VOLE 460,713 B vs gzip prefix 9,764,864 B → **12–21×** fewer bytes | **SCOPED WIN** | `2026-10-05-phase8-seek-08de2a9` | [0019](../adr/0019-seek-based-io.md) |
| **Partial-view decode CPU** | sequential `gzip`/`zstd`/`xz` decoders | late queries ≈**2–5×** faster than gzip, ≈**4–13×** than xz; **never beats zstd's decoder**; RSS ~38 MB vs gzip ~1.2 MB | **SCOPED WIN (CPU only)** | `2026-10-05-phase7-partial-a5764c9` | [0018](../adr/0018-partial-materialization.md) |
| **Cross-document sharing** | per-file min LZ **and** content-defined-chunk dedup (borg 1.2.4, chunker `10,15,11,127`) | unique-reachable `U = 3,369,900 B` vs per-file LZ **1,304,307 B** vs CDC-raw **771,383 B** | **ROBUST LOSS** | `2026-10-05-phase9-store-fdb2845` | [0021](../adr/0021-cross-document-sharing-result.md) |
| **Encoder-only search governance** | fixed complete-cost heuristic and an exhaustive candidate grid | `fixed == exhaustive` on **every** workload; median byte benefit **0 ‰**; guided matches exhaustive on 8/8 holdout with ≤ ½ the candidates | **RECORDED NEGATIVE** | `2026-10-05-phase10-governor-d2b09c9` | [0022](../adr/0022-encoder-only-search-governance.md) |

### 3.2 Details and receipts for each axis

**Exactness.** The only normative profile is `materialize(descriptor) ==
original_bytes`; parsing/semantic-equality/canonicalization are not substitutes.
Confirmed by the standing invariants in [`CONFORMANCE.md`](../reference/conformance.md) and by
every sealed campaign's verification triples.

**Whole-file size (ADR-0017, receipt `2026-10-05-phase7-baselines-7b9f662`).** On 27
locally generated files (23 Phase-7.0 producer/synthetic + 4 Phase-7.0b
generator-family), the best VOLE lane — the minimum complete serialized `.voldoc`
across auto/raw/rle/byte-rans and every forced structural kind — beats
`gzip`/`zstd`/`xz`/`brotli` on **0 files**; the best generic is smaller on **every**
file, **+460,320 B** in total. On `cairo-vector.pdf` best VOLE 34,574 B is
**2.07×** brotli (16,670 B); on `_synthetic/flate.pdf` 36,102 B is **1.91×** xz
(18,884 B). Reinforced on the large corpus (ADR-0018): best VOLE 17,392,713 B vs xz
5,841,896 B (**2.98×**). The internal per-mechanism court (below) is *not* a
compression result.

**Random-access bytes read (ADR-0019, receipt `2026-10-05-phase8-seek-08de2a9`).**
On a 33,789,340 B / 800-stream PDF whose seekable descriptor is 17,566,832 B, the
seeked `view` reads a **constant 439,679–461,367 B** (a floor of header +
DIRECTORY + GRAPH + OBSERVATION_INDEX + INTEGRITY) and all 18/18 queries are
byte-exact. Versus **non-seekable sequential** gzip/zstd/xz prefixes this is a
**12–21×** scoped win in the late region. Versus **fine-block seekable** formats
it **loses** (BGZF 23,808 B and xz-64KiB 15,344 B read 19–30× fewer bytes;
xz-1MiB 179,892 B reads ~2.6× fewer; BGZF is also smaller whole-file,
7,995,600 B < 17,566,832 B), but versus **coarse-block seekable** formats it
reads **less** (xz-4MiB 708,612 B; pixz 2,810,832 B with 16 MiB blocks). It
therefore **sits between fine and coarse block sizes** — a loss to fine-block
formats and a minor win over coarse-block ones — so it is **not a general
random-access-I/O win.** It also loses at offset 0 and in the early region; the
offset-independent floor would dominate a descriptor smaller than ~9 MB. The
Phase-8.4 amendment and `docs/evidence/phase8-skeptic-review.md` record this
plainly.

**Partial-view decode CPU (ADR-0018, receipt `2026-10-05-phase7-partial-a5764c9`).**
18/18 queries byte-exact; mid/late queries touch ~0.41–0.43 MB
(`descriptor_bytes_traversed` alone) vs gzip inflating `a+len`, ≈2–5× faster than
gzip and ≈4–13× than xz on CPU. It **never beats zstd's decoder**, loses in the
early region (≤ ~8–16 MiB), and in v1 reads the whole descriptor so on-disk I/O is
**not** reduced (that gap became the Phase-8 seek reader). Peak RSS ~38 MB vs
gzip's ~1.2 MB.

**Cross-document sharing (ADR-0021, receipt `2026-10-05-phase9-store-fdb2845`).**
37 locally generated files (5,579,469 B, 10 deliberate-sharing strata); all 37
standalone and store-backed roots byte-exact; closure `dangling = 0`. `S =
3,762,694`, `U = A = 3,369,900`, unique object bytes 786,501. Per-file min LZ
1,304,307 B; strongest pinned CDC (borg 1.2.4, `--compression none`) deterministic
**771,383 B** (the zstd-chunk variant is **non-deterministic**, observed
210,835–210,840 B, never cited as a fixed figure). **`U` loses to all three.** The
negative is **robust** (forcing `--force pdf-deflate-replay` gives `U = 2,360,054 B`;
a per-stratum oracle ≈ `2,537,730 B`, both still losses) but its *size* is **partly
an artifact of externalization granularity / candidate selection**: `externalize`
replaces only `Descriptor.objects`, and the auto winner emits 0–1 objects per file.
Under the auto candidate the only win is `repeat-bin` (four byte-identical opaque
binaries, `U = 133,048 B`); forcing `PDF_DEFLATE_REPLAY` (one object per deflate
stream — a candidate in the current set) flips `shared-payload` to `264,139 B`, a
win over **raw** CDC (285,257 B) that still loses to LZ (34,591 B) and compressed
CDC (28,195 B). No current candidate emits more than one object per file. Sharing
is store **amortization**, never "compression".

**Encoder-only search governance (ADR-0022, receipt
`2026-10-05-phase10-governor-d2b09c9`).** Over a small deterministic cohort split
into disjoint tune/holdout/control sets: **H1 HELD** (guided never worse than fixed),
**H2 HELD** (guided == exhaustive on 8/8 holdout with ≤ ½ candidates), **H4 HELD**
(negative controls match RAW byte-for-byte) — but **H3 HELD too**, and it is the
substantive result: the fixed heuristic already attains the exhaustive minimum on
**every** workload, so the parametric search adds **zero bytes** (median benefit
0 ‰). Representative rows (final bytes / candidates): `flate.pdf` 36,161
(10/438/150); `bigtext.pdf` 38,274 (7/414/126); `many.pdf` 5,301 (7/414/126). The
fixed complete-cost court is retained. Zero decode authority is *proven*: a
governor-produced descriptor decodes byte-exactly in a default build **without**
`dsfb-search`.

### 3.3 Internal per-mechanism court (relative to the weak order-0 `BYTE_RANS` lane)

These are *ablation* results, not compression claims (ADR-0017). They explain the
representation's shape.

| Candidate | Baseline | Result | ADR |
|---|---|---|---|
| `PDF_CHANNELS` (Phase 4) | `BYTE_RANS` | loses (`bigtext.pdf` 46,432 vs 38,142 B) | [0010](../adr/0010-typed-channels-rejected.md) |
| `PDF_LAYOUT` (Phase 5) | `BYTE_RANS` / RAW | loses on DRA framing (0/7 classic-xref files) | [0011](../adr/0011-layout-prediction-framing.md) |
| layout-v2 packed (Phase 5.7) | RAW / `BYTE_RANS` | beats RAW at scale (`many.pdf` 10,069 vs 10,215) but loses to `BYTE_RANS` (5,181) | [0012](../adr/0012-packed-framing-threshold.md) |
| `PDF_LAYOUT_RANS` (Phase 5.8) | `BYTE_RANS` | loses (0 win / 8 lose / 3 decline) | [0013](../adr/0013-layout-rans-not-profitable.md) |
| `PDF_DEFLATE_REPLAY_RANS` (Phase 6) | `BYTE_RANS` | wins on one synthetic fixture (`flate.pdf` 36,102 vs 49,291 B) when plaintext is **both** shared **and** large/weakly coded; loses/declines on every genuinely transformed producer output | [0015](../adr/0015-deflate-replay-result.md) |

---

## 4. What won (scoped, honest)

Exactly two scoped results survive:

1. **A small partial-decode CPU win on large documents** for late random-access
   queries, versus sequential `gzip`/`xz` (ADR-0018), later realized as a
   bytes-read win **versus non-seekable sequential codecs only** (ADR-0019). It
   never beats zstd's decoder, **loses to fine-block seekable formats but reads
   less than coarse-block ones** (BGZF 23,808 B and xz-64KiB 15,344 B read less
   than VOLE's 460,713 B; xz-4MiB 708,612 B and pixz 2,810,832 B read more), and
   carries an offset-independent ~440 KB floor.
2. **Whole-object dedup of *identical opaque* files** in the content-addressed
   store (ADR-0021). This is real but **marginal versus CDC**: `repeat-bin`
   `U = 133,048 B` vs CDC+zstd `133,863 B` (~0.8 KB, ~0.6 %), and large only versus
   *per-file LZ* (`524,308 B`) — which is the wrong comparison for a sharing axis.

Beyond those two scoped results there is one **under-claimed qualitative
capability difference** (stated as a capability, **not** a bytes-read win): a
single `.voldoc` artifact simultaneously provides full byte-exact archival
materialization **and** structural observation (`--pdf-object`, `--pdf-stream`,
`--pdf-revision`). BGZF and blocked xz offer byte-range seeks only — the
seekable court above measured byte ranges only — so a byte-range seekable format
cannot answer an object/stream/revision query without first reconstructing and
re-parsing the document. This is a capability BGZF/blocked-xz do not offer; it
is not a size or bytes-read advantage.

Nothing else wins. In particular the store wins **nothing** on near-duplicate or
sub-object sharing (CDC captures those), and the governor wins **nothing** at all.

---

## 5. What lost, and why

| Lost axis | Against | Why |
|---|---|---|
| **Whole-file size** | gzip/zstd/xz/brotli | The DRA + typed-residual + entropy-channel representation is **coarser** than LZ77/long-range matching. Structural prediction of plain syntax removes fewer bytes than the per-site program/model overhead it adds (Phases 4/5/5.7/5.8 all converge here); once structure is predicted, the residual is ordinary byte data a monolithic order-0 lane codes better. |
| **Random-access I/O** | seekable/blocked formats (BGZF, blocked xz) | VOLE's index + directory + graph are one large floor read together, whereas compact seekable-block formats read only the covering block(s) + index. VOLE's floor is offset-independent; a 256-byte request still costs ~440 KB. |
| **Cross-document sharing** | per-file LZ **and** generic CDC | We made sharing **whole-object-granular** (the finest unit the candidate set can emit is the whole deflate stream). `externalize` never touches the entropy-channel payloads and `GRAPH` inlines where the auto winner puts the bulk, so it emits 0–1 objects/file; CDC operates at sub-object granularity and captures the same (and near-duplicate) sharing the store misses. |

And the deeper reason **exactness itself is not a win**: byte-exactness is shared
with any lossless compressor — it is the floor, not an advantage. The programme's
genuine durable outputs are exactness, an auditable/typed representation, and the
recorded negatives, not a size or speed victory.

---

## 6. Falsified-claims log

Every over-stated claim was corrected by an independent adversarial reviewer; the
correction is preserved rather than rewritten, and `results.json` numbers were
left untouched (corrections are prose/amendments).

| Withdrawn claim | Why it was false | Review |
|---|---|---|
| **Phase-6 "win reproduces on qpdf transformer output"** | The "qpdf win" is a `qpdf --object-streams=preserve` **copy** of our own `hand-base2.pdf` fixture's two byte-identical raw streams (`ec028dc1…`); **99.93 %** of it is inherited. Every genuinely transformed producer output loses or declines (win 3 / lose 8 / decline 12, all 3 wins self-authored). | [`phase7-skeptic-review.md`](../reviews/phase7-skeptic-review.md) |
| **Phase-7.0b "Cairo: first authoring-generator witness"** | The generator repeated **one identical page six times**, so Cairo emitted six byte-identical streams (plaintext *and* compressed bytes); generic LZ does ~2× better (gzip 17,382 B; xz-9e 16,852 B) than the 34,574 B "win", which was only vs the weak order-0 `BYTE_RANS`. | [`phase7b-skeptic-review.md`](../reviews/phase7b-skeptic-review.md) |
| **Phase-6 win region = "shared **or** weakly coded"** | Each conjunct alone loses (four negative controls); the win requires shared **and** large/weakly-coded plaintext. | [`phase6-skeptic-review.md`](../reviews/phase6-skeptic-review.md) |
| **Phase-6 descriptor labels: "three *shared* plaintext channels", "six stored bitstreams", "levels 0/1 almost verbatim"** | The real `PDF_DEFLATE_REPLAY_RANS` descriptor for `flate.pdf` is `streams=6 replayed=6 channels=3 objects=6`, but **only `p1` is shared** (four streams at levels 0/1/6/9; `p2`/`p3` are unique); `BYTE_RANS` **order-0-codes** the streams rather than storing them; level 1 is ~19 % of the plaintext (6,197 B), **not** verbatim (only level 0 is, 31,998 B). | [`phase6-skeptic-review.md`](../reviews/phase6-skeptic-review.md) |
| **Phase-7c F1 primary metric = `descriptor_bytes_traversed + entropy_bytes_decoded` (0.41–0.46 MB)** | **Double-counts** the decoded channels: `descriptor_bytes_traversed` already includes the referenced entropy-channel payload bytes that `entropy_bytes_decoded` reports again. Superseded by **`descriptor_bytes_traversed` alone (412,161–433,694 B ≈ 0.41–0.43 MB)**, constant across offset. | [`phase7c-skeptic-review.md`](../reviews/phase7c-skeptic-review.md) |
| **Phase-7.0 `correction/compressed` p50 = 0.004518** | Misattributed: 0.004518 is the `pdf-make-samples` **subset** median; the corpus-wide p50 is **0.014716**. | [`phase7-skeptic-review.md`](../reviews/phase7-skeptic-review.md) |
| **Phase-8 "bytes-read win" as a general random-access-I/O result** | Holds only against **non-seekable sequential** codecs; against **fine-block** seekable/blocked formats VOLE reads **2.6–30× more** (it reads **less** than **coarse-block** ones: xz-4MiB 708,612 B, pixz 2,810,832 B). Also, the "reads only the 64-byte header, DIRECTORY, …" wording hid a ~440 KB floor. | [`phase8-skeptic-review.md`](../reviews/phase8-skeptic-review.md) |
| **Phase-9 "the only win is byte-identical opaque repeats"; "a finer unit is not implied by the current set"; fixed CDC-zstd 210,836 B** | Granularity: forcing `PDF_DEFLATE_REPLAY` (a current-set candidate) wins `shared-payload` over raw CDC. Compressed-CDC is **non-deterministic** (210,835–210,840 B). The negative survives; its size is partly an artifact. | [`phase9-skeptic-review.md`](../reviews/phase9-skeptic-review.md) |
| **Phase-7.3 partial "allocation" win / early boundary ≤ 1–8 MiB** | It is a decode-**CPU** win; peak RSS is a **loss** (~38 MB vs gzip ~1.2 MB); the early boundary is ≤ ~8–16 MiB. | [`phase7c-skeptic-review.md`](../reviews/phase7c-skeptic-review.md) |

All review documents: [`phase6`](../reviews/phase6-skeptic-review.md),
[`phase7`](../reviews/phase7-skeptic-review.md),
[`phase7b`](../reviews/phase7b-skeptic-review.md),
[`phase7c`](../reviews/phase7c-skeptic-review.md),
[`phase8`](../reviews/phase8-skeptic-review.md),
[`phase9`](../reviews/phase9-skeptic-review.md). Phase-11 and Phase-12
corrections are recorded separately in **§9** above and in their reviews:
[`phase-11-skeptic-review.md`](../reviews/phase-11-skeptic-review.md),
[`phase-12-skeptic-review.md`](../reviews/phase-12-skeptic-review.md).

---

## 7. What would change the conclusion

Each of these is a concrete, falsifiable next step — and each carries an explicit
**prior that it may also lose**, because every measured attempt so far has:

1. **Finer-than-object shareable units.** Externalize individual
   `ENTROPY_CHANNEL` payloads and/or arbitrary sub-object chunks, so sharing is not
   limited to whole DRA objects. *Prior: loses* — generic CDC already operates at
   sub-object granularity, so this must beat a rolling-hash chunker, not just the
   current store.
   **Measured (2026-10-06, Phase 11.14; ADR-0028).** Implemented and measured
   (`share-account` + `tools/field-share-court.sh`;
   [`receipt`](../../evidence/campaigns/2026-10-06-phase11-share-d9f818a/)). Over 4 real
   producer documents + a byte-identical repeat + a few-bytes-changed
   near-duplicate (8 files, 337,877 B), the fine-unit unique **lower bound**
   (208,001 B) **loses to** the strongest content-defined chunking (borg
   `10,15,11,127` raw, 160,668 B, −22.7 %) and to a single-stream `tar | xz -9e`
   (92,752 B). Its only win is a marginal whole-cohort edge over per-file min LZ
   (211,301 B, 1.6 %) that is **smaller than the 8,435 B of root/program framing
   the metric excludes**, and it does not hold per stratum (LOSS to LZ on
   producer/repeat/near; the near change destroys the one order-0 channel
   payload: 143,706 B unique of 143,706 B total vs CDC 82,757 B for the pair).
   **The prior is confirmed: finer-than-object units still lose to CDC**, and the
   negative is now recorded for both object granularity (ADR-0021) and fine-unit
   granularity (ADR-0028).
2. **A structural model stronger than LZ.** A representation that removes
   structure *without* paying a per-site plan (e.g. canonical/parametric layout,
   nested content proceduralization) could in principle beat LZ77 on
   high-structure documents. *Prior: loses* — four independent plain-syntax
   proceduralizations (Phases 4/5/5.7/5.8) all lost on framing/model overhead.
3. **Producer-population corpora.** The enabling condition of the one structural
   win (shared plaintext that is also large/weakly coded) has never been observed
   in genuinely transformed producer output — only in our own fixtures. A
   third-party corpus could confirm or refute that the region exists in the wild.
   *Prior: loses/unknown* — on the tested locally generated corpora it does not
   appear.

---

## 8. Reproducibility

- **Docker only.** No `cargo`/`rustc`/tests/fuzzing/oracles ever run on the host.
  All commands go through the pinned services in [`compose.yaml`](../../compose.yaml):
  `dev` (`rust:1.99.0-slim-bookworm`, digest-pinned), `msrv` (`rust:1.89`),
  `tools`/`producers` (Debian bookworm, digest-pinned), `baseline` (dev + 
  gzip/zstd/xz/brotli/borg), `policy` (`cargo audit && cargo deny check`), `fuzz`
  (dated nightly `nightly-2026-10-04` + `cargo-fuzz 0.13.2`).
- **Receipt schema.** Every sealed run lives under
  `evidence/campaigns/<date>-<phase>-<gitsha>/` and records `manifest.json` +
  `environment.json` + results, including: base image name **and digest**, `rustc`/
  `cargo` versions, `Cargo.lock` SHA-256, git commit **and** dirty state, CPU
  architecture, external-oracle versions, the exact command line, and any
  environment variables that affect semantics. Receipts are immutable: new runs get
  new directories and **corrections are amendments, not rewrites**.
- **Tags are the durable record.** Annotated tags **`v0.1.0-alpha.2`** …
  **`v0.1.0-alpha.16`** exist. **`v0.1.0-alpha.16`** is the current release
  (Phase 12; see §9). **`v0.1.0-alpha.1`** has **no git tag** — its
  CHANGELOG section exists, but the tag lineage begins at `v0.1.0-alpha.2`.
  *(This bullet is a dated corrective: it previously stopped at
  `v0.1.0-alpha.12` as the then-current release.)* Merged phase branches are
  deleted once the tag preserves the history.
- **Feature / mandatory-bit policy.** The default build is permissive-only
  (`default = ["rans", "store", "field"]`; the `field` feature was added in
  Phase 11 — this bullet previously read `["rans", "store"]`). `deflate-replay`
  is **opt-in** and pulls LGPL `cabac` (ADR-0014). A descriptor whose
  `replay_codec` tag is unknown, or that needs a capability the build lacks
  (channel, external object, seek directory), sets a **mandatory feature bit** and
  **fails closed** with `UnsupportedFeature` (exit 6) rather than being
  reinterpreted. Semantic changes bump the universe string; optional capabilities
  use ignorable optional bits. The wire format is pre-1.0 and not yet frozen
  (ADR-0004).

### Gate suite (Phase 10.2)

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features --locked
cargo test --locked
cargo test --no-default-features
msrv cargo test --locked --all-features
policy sh -c 'cargo audit && cargo deny check'
```

All of the above run inside Docker (`docker compose run --rm --no-TTY <service>
<cmd>`). Gate receipts are recorded alongside the campaigns.

---

## 9. Phase-12 addendum (released as `0.1.0-alpha.16`)

This consolidated document is frozen at Phase 10.2 and does **not** yet cover
Phase 11 or Phase 12. The following is inserted only so a reader does not mistake
it for an endorsement of the Phase-12 headline; the authoritative Phase-12
evidence is [`docs/phases/phase-12-results.md`](../phases/phase-12-results.md),
its sealed receipts under `evidence/campaigns/2026-10-06-phase12-*`, and the
independent review
[`docs/reviews/phase-12-skeptic-review.md`](../reviews/phase-12-skeptic-review.md).

**Scoped Phase-12 verdict (measured).** On a **self-authored** 12-document
PDF/DOCX/EPUB corpus (841 B – 61 KB), the persistent procedural field (a) is
byte-exact for all three formats after source **and** descriptor deletion in a
fresh process (length + SHA-256 + `cmp`), (b) shares an exactly byte-identical
resource across DOCX/EPUB by content id, and (c) beats direct per-query tooling
(and the source-retaining SQLite+FTS5 baseline in the cold one-time comparison)
on the small-document lifetime frontier — while **losing** to that baseline on
the large synthetic documents' byte frontier (N=10–1000) and wall/CPU at N=1000,
and losing PDF page text to Poppler in the LLM working-set court.

**Corrections and open gates (Phase-12.15 skeptic, `15b5729`).**

* The required Phase-12 ablation ladder (`A1b`, `A2`–`A10`, eager-vs-progressive,
  raw-vs-decoded) was **not run**; only A0/A1/V were measured, so no named
  mechanism is attributable.
* The §109/`N6` **"PDF no regression" gate has no receipt**.
* ADR-0034's mandated post-`cache --clear`, OS-witness and CDC controls for the
  reuse fraction are **absent** from the 12.8 receipt; `N3` is not evaluated.
* The 12.13 hostile-input class tally was corrected to **16 reject / 22 decline /
  7 accept** (45 fixtures).

**Close-out (Phase 12.16, `0d23a02` + sealed receipts; released as
`0.1.0-alpha.16`).** The required ablation ladder **has now been run**
(`evidence/campaigns/2026-10-06-phase12-lifetime-ablations-06db12a/`): the win is
attributed to the **content adapters** (A4→A5) and to **persistent semantic reuse**
(A5→A6, bytes for CPU); EntropyFS (A9) is a loss; `A7`/`A8` are **not separable**.
`N6` (PDF no regression) is **closed** (A2 vs A11: 32/32 byte-exact, 0 regressions).
`N3` is **evaluated and VIOLATED**: post-`cache --clear` reuse is `0.0` (in-process
and fresh process), so cross-document **work** reuse is a recorded negative while
representation identity remains shared. FTS5 is measured (trigram ties `LIKE` but
reads more; `unicode61` misses embedded markers). Default-feature clippy is clean
and the 12.14 demo has a sealed receipt. `N4` (decline-rate threshold) remains
**not evaluated**.
* The 12.9 equivalence is a **generator-defined, circular** triplet, not an
  independent real-world corpus.
* The default-feature build fails `cargo clippy -- -D warnings` on pre-existing
  dead code in `src/field/document_format.rs` (four items).

No Phase-12 claim should be stated more strongly than this.
