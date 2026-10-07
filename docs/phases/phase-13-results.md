# Phase 13 results

Branch: `phase13`. Base: `main` @ `13efaf3` (`v0.1.0-alpha.16`, Phase 12).
Plan: [`phase-13-plan.md`](phase-13-plan.md).

Phase 13 closes the remaining Phase-12 `PROPOSED` items and the last open gate.
Each subphase is measured to a sealed campaign; negatives are recorded, not
buried.

## PDF length/revision size (13.1)

**Question.** Can a PDF's structurally determined `/Length` values and its
revision/xref chain be *regenerated* instead of stored, and does that pay its own
framing? (Phase 3 recovers them as observations; Phase 11 persists them; neither
prices them.)

**Candidate.** `PDF_LENGTH_REVISION` (`encode --force pdf-length-revision`): one
`PACK_SEGMENTS` program over one literal data object that regenerates (a) xref
entry offsets equal to a marked object position, (b) `startxref` / trailer
`/Prev` absolute offsets (one mark slot per revision), and (c) each directly-sized
stream's `/Length` by marking the output offset equal to the length and emitting
that mark. It reuses only the existing positional ops — no new opcode, DRA
version, universe, or feature bit. Every precondition failure falls back to a
literal or declines, and the finished program must round-trip
(serialize → parse → materialize → byte-compare) before it is returned.

**Court.** `tools/phase13-court.sh` (pre-registered H1–H4) over **28 complete
files** — 11 Phase-7 producer/synthetic PDFs, 4 generator-family PDFs, the
12-file `pdf-make-samples` set, and one `pdf-make-large` document — comparing the
forced lane against the current VOLE ladder and, via `tools/baselines.sh`, against
`gzip -9` / `zstd -19 --long=27` / `xz -9e` / `brotli -q 11` on the same files.
Sealed receipt:
`evidence/campaigns/2026-10-07-phase13-pdf-length-revision-12fc84e/`.

**Outcome — expected negative (ADR-0036).**

| axis | result |
| --- | --- |
| byte-exactness where proposed | **21/21 exact**, 0 failures |
| declines | **7/28** (4 xref-stream PDFs, 1 malformed PDF, 1 non-PDF, 1 > 254-object PDF) |
| wins vs the current VOLE ladder | **0** (ties 0, losses 21) |
| wins vs the best generic compressor | **0** (ties 0, losses 21) |
| does it ever lower the ladder? | **no** (`ladder_total_excl == best_vole_total_incl`) |

The candidate is *larger than the source* on essentially every file it proposes
(`classic.pdf` 329 → 905 B; `bigtext.pdf` 65,549 → 66,123 B vs xz's 824 B;
`cairo-vector.pdf` 58,424 → 59,016 B vs brotli's 16,670 B). Regenerating a
`/Length` or an xref/`/Prev` offset costs a `Mark` plus an `Emit` item (and, for
`/Length`, a slot per distinct length) per field, which exceeds the few digits it
removes — the same per-site framing that sank `PDF_LAYOUT` (ADR-0011/0012/0013).
The last unmeasured PDF structural idea is therefore measured and **closed as a
recorded negative**; the candidate stays available as a forced ablation lane and
is never the auto winner.
