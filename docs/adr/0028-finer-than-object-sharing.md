# ADR-0028: Finer-than-object shareable units still lose to content-defined chunking (measured)

- **Status:** Accepted — recorded measurement (Phase 11.14)
- **Date:** 2026-10-06
- **Extends:** ADR-0021 (object-granularity sharing loses). **Relates to:** ADR-0020
  (store contract and accounting universes), ADR-0027 (cost accounting),
  `FINDINGS.md` §7 direction 1.
- **Campaign:** `evidence/campaigns/2026-10-06-phase11-share-0f3d1d3/`
  (branch `phase11`, commit `0f3d1d3`).

## Context

ADR-0021 measured that sharing whole DRA **objects** does not beat generic
content-defined chunking (CDC), and named **finer-than-object shareable units** as
the first direction that could change the conclusion — with the explicit prior
*"loses"*, because generic CDC already operates at sub-object granularity, so a
finer unit must beat a rolling-hash chunker, not just the coarser store.
`src/field/share.rs` (Phase 11.4 scaffolding) named four fine unit kinds and
content-addressed each; it did not yet have a court or a verdict.

This ADR finishes subphase 11.14: it adds a `share-account` verb that stores the
fine units and prints the `ShareReport`, and a court that measures, on one run,
whether fine units share better than whole objects, against a per-file compressor
ladder and against borg CDC with the frozen Phase-9.3 parameters plus a
sensitivity sweep (ADR-0021's rigor: the *strongest* CDC, never a strawman).

## Decision

### What is measured

`extract_units` names and BLAKE3-256-content-addresses four kinds of unit:

| kind | unit | source record |
|---|---|---|
| `object` | one object-table entry's raw bytes | `OBJECT` / `EXTERNAL_REF` |
| `channel_payload` | an entropy channel's renormalization payload | `ENTROPY_CHANNEL` |
| `channel_header` | a channel's fixed 33-byte header | `ENTROPY_CHANNEL` |
| `model` | a canonical entropy model's encoded bytes | `MODEL` |

`share-account --store DIR` writes every inline unit into the same
content-addressed share namespace and prints the report; the store holds exactly
the distinct units, so `store_bytes == unique_bytes` is an independent check on
the in-memory figure (asserted in the court, and unit-tested).

**Accounting discipline (unchanged, ADR-0020).** `unique_bytes` counts only
distinct *unit* bytes. It excludes all record/root framing, the universe, the
graph, the integrity manifest, and any `Op::Inline` program bytes. It is therefore
a **lower bound** on any store form, never an achieved store size, and is only
compared to the per-file compressor ladder and to generic CDC — never to a single
file. The court reports the excluded bytes explicitly.

These four kinds are **stored and measured, not wire-externalized**: the
descriptor still carries them inline, and resolving a channel payload from the
store during materialization would be a representation change that this subphase
deliberately does not attempt. Only `object` units are wire-externalizable today
(the Phase-9 `EXTERNAL_REF`), which is exactly ADR-0021's object-granularity case.

### Cohort

Locally generated, no third-party bytes (`evidence/corpus/phase7-producers/`,
regenerable and byte-reproducible via `tools/pdf-corpus-producers.sh`):

```text
producer   4 files, 167,323 B   real authoring-generator PDFs (ReportLab, Cairo,
                                LibreOffice, pdfTeX), auto complete-cost winners
repeat     2 files,  21,812 B   two byte-identical copies of reportlab-multipage
near       2 files, 148,742 B   libreoffice-export and a variant with 8 bytes
                                ("VOLENEAR") changed at offset len/2
whole      8 files, 337,877 B
```

Every descriptor is `verify`-ed, `decode`-ed and byte-compared (`cmp`) against
its source before any sharing number is reported, so `materialize == source` holds
for the whole cohort.

### Measured result (bytes)

```text
cohort source                                       337,877
descriptor standalone S (auto winners)              310,868
  of which GRAPH/program bytes (not units)            4,799
  total non-unit bytes (excluded from unique_bytes)   8,435
VOLE fine-unit total_bytes                          302,433
VOLE fine-unit unique_bytes (lower bound)           208,001   (31 units -> 21 unique)
  channel_payload   273,909 total -> 181,644 unique
  object             24,280 total ->  24,280 unique
  channel_header        297 total ->     198 unique
  model               3,947 total ->   1,879 unique
per-file gzip -9 sum                                216,296
per-file zstd -19 sum                               212,703
per-file xz -9e sum                                 211,532
per-file min LZ sum                                 211,301
tar | xz -9e (single stream, deterministic archive)  92,752
CDC borg 1.2.4, frozen 19,23,21,4095, raw, --c  none 244,397  (deterministic)
CDC borg 1.2.4, strongest swept 10,15,11,127, raw    160,668
```

### Verdict

**Finer-than-object units do not beat the strongest generic baseline.**

- **vs strongest CDC: LOSS.** `208,001` vs `160,668` B (**−22.7 %**, i.e. CDC is
  22.7 % smaller). The fine-unit figure is already a *lower bound* and its
  `channel_payload` units are themselves rANS-coded, so this is compared
  favourably to raw CDC — and it still loses.
- **vs `tar | xz -9e` single stream: LOSS.** `208,001` vs `92,752` B.
- **vs per-file min LZ: marginal, non-robust WIN.** `208,001` vs `211,301` B
  (1.6 %), but the margin (3,300 B) is **smaller than the 8,435 B of root/program
  framing the metric excludes**, so it is not an achieved-store win; and it does
  **not** hold per stratum (below).
- **vs the frozen coarse CDC: WIN** (`208,001` vs `244,397`). This is an artifact
  of cohort scale: borg's frozen minimum chunk is 2¹⁹ = 512 KiB, larger than every
  file here, so the frozen chunker degrades to whole-file dedup. The sweep
  corrects it (`10,15,11,127`), which is why the verdict uses the strongest result
  (ADR-0021's rigor).

### Per-stratum verdict (`vs` = VOLE unique_bytes vs the named baseline)

| stratum | files | source | descriptor S | VOLE unique | min LZ | CDC frozen | CDC strongest | vs LZ | vs CDC frozen | vs CDC strongest |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|---|
| producer | 4 | 167,323 | 142,762 | 136,115 | 93,089 | 168,890 | 152,177 | LOSS | WIN | WIN |
| repeat | 2 | 21,812 | 22,406 | 10,756 | 4,248 | 12,027 | 12,198 | LOSS | WIN | WIN |
| near | 2 | 148,742 | 145,700 | 144,289 | 113,964 | 149,881 | 82,757 | LOSS | WIN | LOSS |

- **near — the decisive loss.** The two documents differ by 8 bytes, yet each
  auto winner is a single order-0 `BYTE_RANS` channel, so the changed byte
  destroys the *entire* renormalization payload: `channel_payload` shares nothing
  (`143,706` total → `143,706` unique). CDC re-synchronizes and keeps `82,757` B of
  the 148,742 B pair (**−42.7 %** for CDC). Fine units have no sub-payload
  granularity to resynchronize on.
- **repeat — a dedup win over CDC, but a loss to LZ.** Byte-identical descriptors
  share every unit (`10,756` B), which beats both CDC figures, but the descriptor
  units are still larger than `xz -9e` of the source (`4,248` B), so per-file LZ
  wins: dedup is not compression.
- **producer — VOLE wins on distinct files here, but against *raw* CDC only.**
  Four distinct files share nothing (`136,115` B unique), yet this is below CDC's
  raw chunk total (`152,177` B): the descriptor units are entropy-coded while CDC
  chunks are raw source bytes, so this compares coding + dedup against dedup
  alone. It is not a sharing win; it is the coding the descriptor already paid for.
- The whole-cohort VOLE `unique_bytes` equals the producer stratum's `136,115` B
  plus the near variant's one changed payload (`71,886` B): the repeat and the
  near base are fully shared copies. That dedup is real, and CDC captures it too.

### Cause (measured)

Fine units are **content-addressed at a fixed granularity the encoder chooses**,
and the auto winner chooses one order-0 channel per most documents, so a
near-duplicate change rewrites that one payload end to end; there is no
rolling-hash resynchronization. Generic CDC abstracts over byte offsets and keeps
the shared prefix/suffix chunks. On whole-object dedup (byte-identical repeats)
the store wins because it is exact and reference-free; on near-duplicates it has
no finer unit to fall back to. Richer structural candidates (`PDF_DEFLATE_REPLAY*`)
can emit more units, but their residual payload is still one channel per document,
and their *program* differences live in `GRAPH` bytes that are not units at all.

**Metric blind spot (structural).** `unique_bytes` excludes `GRAPH`/program
bytes. A change can land entirely in the graph and leave every extracted unit
byte-identical, in which case `unique_bytes` overstates sharing; the court reports
the excluded bytes (8,435 B here) so the figure is never read as an achieved store.
The near stratum above was chosen so the change destroys a *unit* (a channel
payload), which is the strongest test of fine-unit sharing, not the blind spot.

## Consequences

- **Recorded negative (extends ADR-0021).** Finer-than-object units, as named by
  the current candidate set, do **not** beat generic CDC: `208,001` (lower bound)
  vs `160,668` B raw CDC on near-duplicates, and they lose to a single-stream
  `tar | xz -9e`. The prior in `FINDINGS.md` §7 is confirmed with a measured
  receipt and an ADR, not asserted. Object-granularity (ADR-0021) and
  fine-unit (this ADR) sharing now both have recorded negatives against CDC.
- **Claim discipline.** Cross-document sharing is store *amortization*, never
  "compression"; the descriptor units are themselves entropy-coded, so a fine-unit
  figure must never be presented as beating a compressor. `store_bytes ==
  unique_bytes` shows the store holds the units once, and nothing more.
- **What would change it.** A representation that yields *many independent,
  localizable* shareable units per document (so a near-duplicate change touches a
  subset) or a fine unit that is a rolling-hash chunk. Until that is measured, the
  honest result stands: **generic CDC is the stronger cross-document baseline.**
- The fine-unit machinery (`share.rs`, `share-account`) remains a *measurement
  instrument*; it is not wired into materialization and changes no descriptor
  bytes or exactness property.

## References

- `evidence/campaigns/2026-10-06-phase11-share-0f3d1d3/` (`receipt.json`,
  `results.json`, `SUMMARY.md`, `commands.txt`, `raw/` with `cohort.jsonl`,
  `encode.jsonl`, `lz.jsonl`, `share-cohort.json`, `share-strata.jsonl`,
  `cdc-frozen.json`, `cdc-sweep.json`, `cdc-strata.jsonl`)
- `src/field/share.rs`, `src/main.rs` (`share-account`), `tests/share_account.rs`
- `tools/field-share-court.sh`, `tools/chunk-dedup.sh`
- ADR-0020 (store contract, three universes), ADR-0021 (object-granularity
  negative), ADR-0027 (cost accounting), `FINDINGS.md` §7
