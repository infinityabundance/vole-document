# Phase 9 — independent adversarial review

- **Reviewer:** independent adversarial reviewer (Phase 9)
- **Scope:** the Phase-9.1/9.2 store (`ObjectStore` / `EmbeddedStore` /
  `EXTERNAL_REF` / `externalize`/`hydrate`/`gc` / optional `EntropyFsStore`,
  ADR-0020) and the Phase-9.3 cohort court
  (`2026-10-05-phase9-store-fdb2845`, ADR-0021) with its headline framing that
  cross-document sharing loses the store axis.
- **Method:** all controls reproduced in the pinned Docker services
  (`docker compose run --rm --no-TTY dev|tools|baseline`); the court was re-run
  with a **forced** per-stream candidate (`encode --force pdf-deflate-replay`)
  and with a per-stratum candidate oracle to separate the mechanism from the
  selector. No measured number in `results.json` was altered; the corrections
  are prose.
- **Verdict:** the **global negative is robust and reproducible** — VOLE loses
  the store axis to per-file LZ and to generic CDC, and it still loses when the
  finest candidate in the current set is forced. But **several published claims
  are wrong or overstated**: the negative is *partly* an artifact of candidate
  selection / externalization granularity, the "only win" wording is inaccurate
  (a forced candidate wins over raw CDC on `shared-payload`), the ADR sentence
  that a finer shareable unit is "not implied by the current candidate set" is
  **false** (`PDF_DEFLATE_REPLAY` *is* in the set), and the compressed-CDC
  citation is **non-deterministic** and was mis-quoted.

## Angles

| # | Angle | Verdict | Basis |
| --- | --- | --- | --- |
| 1 | Store exactness and closure (the invariant that must not regress) | PASS | 37/37 standalone + 37/37 store-backed roots decoded and `cmp`-byte-exact; `store account` closure `dangling = 0`; `externalize`/`hydrate` round-trip identical bytes |
| 2 | **Headline: does the store lose the cross-document axis?** | PASS (robust) | auto `U = 3,369,900` > per-file min LZ `1,304,307` and > raw CDC `771,383`; a forced `--force pdf-deflate-replay` run gives `U = 2,360,054` and a per-stratum oracle ≈ `2,537,730`, both still losses |
| 3 | Is the loss a mechanism limit or a **granularity/selection artifact**? | **PARTLY ARTIFACT** | `externalize` replaces only `Descriptor.objects`; the auto winner emits 0–1 objects/file. Forcing `PDF_DEFLATE_REPLAY` (a candidate in the current set) emits one object per deflate stream and flips `shared-payload` |
| 4 | Claim: "the only (genuine) win is byte-identical opaque repeats" | **OVERSTATED** | the `repeat-bin` win is real, but forcing `PDF_DEFLATE_REPLAY` also wins `shared-payload` over *raw* CDC (`264,139` vs `285,257`) — still a loss to LZ/compressed CDC |
| 5 | CDC-compressed citation is a fixed number | **FALSIFIED** | `--compression none` raw is deterministic (`771,383`, run1 == run2); compressed CDC is **not** (210,835–210,840 across runs). The docs cited `210,836`, which matches neither recorded run |
| 6 | Accounting pass: are `S`/`U`/`A` ever conflated with whole-file size? | PASS with restatement | `S = 3,762,694`, `U = A = 3,369,900`, unique object bytes `786,501`; root framing is `U − object bytes = 2,583,399`, stated explicitly; no store root is compared to a whole file |

## Angle 3 — the artifact-rescue result (the qualifying evidence)

`externalize` replaces exactly the entries of `Descriptor.objects`. The auto
complete-cost winner for most PDFs is a channels/program candidate
(`BYTE_RANS`, `PDF_DEFLATE_REPLAY_RANS`) whose bulk lives in `ENTROPY_CHANNEL`
payloads and `Op::Inline` bytes inside the `GRAPH` — neither is an object-table
entry. So the auto run has almost nothing to share. Forcing a candidate that
*does* emit per-stream objects (and it is in the current set):

| quantity | auto winner | forced `pdf-deflate-replay` | per-stratum oracle |
| --- | ---: | ---: | ---: |
| global `U` | 3,369,900 | **2,360,054** | ≈ **2,537,730** |
| `shared-payload` `U` | 800,210 | **264,139** (2 unique objects) | — |
| `shared-payload` vs CDC raw | LOSS (285,257) | **WIN** (285,257) | — |

So the negative survives candidate selection and granularity (both forced and
oracle totals still exceed raw CDC `771,383` and per-file LZ `1,304,307`), but
the *size* of the loss is partly the selector/granularity, not the mechanism.
`shared-bin` (`U = 657,870` vs CDC `147,253`) remains a pure granularity loss —
even the forced candidate cannot share one 128 KB payload under five distinct
8-byte prefixes because the whole stream differs. **No current candidate emits
more than one object per file**; the finest existing shareable unit is the whole
deflate stream. Finer sharing (individual channel payloads, arbitrary sub-object
chunks) is a representation change.

This falsifies the ADR-0021 sentence: *"A future positive would require the
shareable unit to be finer than a whole DRA object … which is a representation
change, not implied by the current candidate set."* `PDF_DEFLATE_REPLAY` is in
the current candidate set and already shares at deflate-stream granularity.

## Angle 4 — the corrected win statement

- **`repeat-bin`** (four byte-identical opaque binaries) is a real win under the
  auto candidate: `U = 133,048` vs per-file LZ `524,308` (**−74.6 %**), CDC raw
  `140,640` (−5.4 %), CDC+zstd `133,865` (**−0.6 %, ≈815–820 B** — real but
  marginal).
- **`shared-payload`**, forced to `PDF_DEFLATE_REPLAY`, is a second win **over
  raw CDC only**: `264,139` vs `285,257`; it still **loses** to per-file LZ
  `34,591` and CDC+zstd `28,195`.

Both are reported per stratum and neither is generalised. The corrected
statement is: *the auto-candidate win is `repeat-bin`; a forced current-set
candidate additionally wins `shared-payload` over raw CDC, while still losing to
LZ and compressed CDC.*

## Angle 5 — the determinism caveat (corrected citation)

`cdc-global.json` records `--compression none` as deterministic
(`771,383` run1 == run2). `cdc-global-zstd.json` records
`borg_deterministic: false` with `210,838` / `210,840`, and an independent
re-run observed `210,835`–`210,840`. The published `210,836` matches **neither**
recorded run. The corrected citation is a **range, marked non-deterministic**,
with the deterministic raw number (`771,383 B`) as the primary comparison. This
does not change any verdict.

## Angle 6 — accounting pass

`S = 3,762,694`, `U = A = 3,369,900`, unique object bytes `786,501`, and store
physical bytes `786,501` are mutually consistent; the amortized rule is
fractional by reference count, integerized by largest remainder, so `Σ A_i == U`
exactly. The root framing is `U − object bytes = 2,583,399 B`, i.e. the store
roots carry the documents' entropy channels and program bytes and must never be
reported as if they were the document. No conflation of `S` with a store root
was found in the corrected prose.

## Required restatement

> Over 37 locally generated files (5,579,469 B, 10 deliberate-sharing strata),
> the content-addressed store's unique-reachable universe loses the
> cross-document axis: `U = 3,369,900 B` vs per-file min LZ `1,304,307 B` and
> generic CDC `771,383 B` (raw, deterministic). The negative is **robust** — a
> forced `PDF_DEFLATE_REPLAY` run gives `U = 2,360,054 B` and a per-stratum
> oracle ≈ `2,537,730 B`, both still losses — but it is **partly an artifact of
> candidate selection and externalization granularity**: the auto winner emits
> 0–1 objects per file. Forcing `PDF_DEFLATE_REPLAY` (a current-set candidate;
> one object per deflate stream) flips `shared-payload` to `264,139 B`, a win
> over **raw** CDC (`285,257 B`) that still loses to per-file LZ (`34,591 B`) and
> compressed CDC (`28,195 B`). No current candidate emits more than one object
> per file. Compressed-CDC numbers are **non-deterministic** (210,835–210,840 B)
> and are never cited as a fixed figure. Cross-document sharing is store
> *amortization*, never "compression".
