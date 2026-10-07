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

## PDF COS grammar/templates (13.2)

**Question.** Can a small grammar over the PDF **COS layer** — repeated
COS-token phrases stored once and instantiated by reference — pay its own
definition cost against a whole-file order-0 lane? (Phase 8 proposed it;
Phase 11 persisted content operators as *observations* only.)

**Candidate.** `PDF_COS_TEMPLATE` (`encode --force pdf-cos-template`): a bounded
COS-token n-gram template dictionary covered by a greedy leftmost-longest
program of `Op::EmitObject` (instantiate a template) and `Op::Inline` (its
literal parameters). Templates must contain a structural COS token (name,
delimiter, or keyword) and pass a net-savings estimate; discovery and selection
are capped and totally ordered, so the candidate is deterministic. It reuses
only `EMIT_OBJECT`/`INLINE` — no new opcode, DRA version, universe, or feature
bit. Every precondition failure declines, and the finished program must
round-trip (serialize → parse → materialize → byte-compare) before it is
returned.

**Court.** `tools/phase13-2-court.sh` (pre-registered H1–H4) over the **same 28
complete files** as 13.1, comparing the forced lane against the current VOLE
ladder, against the *pre-existing* lanes (the auto winner it may replace), and,
via `tools/baselines.sh`, against `gzip -9` / `zstd -19 --long=27` / `xz -9e` /
`brotli -q 11`. Sealed receipt:
`evidence/campaigns/2026-10-07-phase13-pdf-grammar-dfba2a4/`.

**Outcome — mixed (ADR-0037).**

| axis | result |
| --- | --- |
| byte-exactness where proposed | **7/7 exact**, 0 failures |
| declines | **21/28** (no COS phrase recurs enough) |
| vs the current VOLE ladder (contains the new auto winner) | win 0 / tie 4 / loss 3 |
| vs the **pre-existing** VOLE lanes | **win 4** / tie 0 / loss 3 |
| auto-winner gain on those 4 files | **34,505 B** (−5,879 to −15,222 B each) |
| wins vs the best generic compressor | **0** (ties 0, losses 7) |

`PDF_COS_TEMPLATE` **becomes the best VOLE lane** on `cairo-vector.pdf`
(34,633 → **19,411** B), `libreoffice-export.pdf` (72,850 → **66,895**),
`pdftex-doc.pdf` (24,076 → **18,197**) and `reportlab-multipage.pdf`
(11,203 → **3,754**) — the first VOLE lane to beat `BYTE_RANS` and
`PDF_DEFLATE_REPLAY_RANS` on real authoring output. It **loses to the
pre-existing ladder** on the three files it proposes but with dominant streams
or skewed text (`large.pdf` 17,190,273 → 33,591,494; `flate.pdf` 36,161 →
57,927; `many.pdf` 5,301 → 9,634 — order-0 rANS models repeated object bodies
more cheaply than the phrase framing). And it **never beats a generic
compressor**: on all 7 proposed files brotli/xz/zstd are 2–4× smaller. The
top-level verdict (ADR-0023) is therefore unchanged — a bounded structural
grammar beats a whole-file order-0 lane on repetitive syntax, but not a
purpose-built generic LZ. No wire change; the candidate stays available as a
forced lane and as the auto winner on the files where it genuinely wins.
