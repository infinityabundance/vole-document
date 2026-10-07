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

## ODT adapter (13.3)

**Question.** Does a fourth format — OpenDocument Text — enter the shared field
**exactly** and **queryably**, so that the common observation vocabulary is
proven to generalize beyond PDF/DOCX/EPUB rather than merely asserted?

**Candidate.** A bounded OpenDocument (ODF) inverse compiler over the shared
byte-authoritative ZIP layer and the bounded-XML policy. An `.odt` is an **ODF**
package — mandatory stored `mimetype` (an OpenDocument text media type) plus
`META-INF/manifest.xml` — and **not** OPC: it has no `[Content_Types].xml` and no
`officeDocument` relationship, so the main content part is discovered
**semantically** from the ODF manifest (`content.xml` file-entry), never from a
hardcoded path, and the adapter reuses the ZIP layer but **not** the OPC graph
(ADR-0038). The bounded content model (`office:body`/`office:text`) covers
paragraphs, headings, spans, lists, tables (column/row spans, covered cells),
links, bookmarks, notes, images/resources, tracked changes, and sections; a
versioned `OdtExtractProfile` (tracked changes Final/Original/All, notes,
hidden, tabs, breaks) is hashed into the canonical selector. Progressive
inversion parses only the requested part on demand and persists the derived
model; the exact leaf stays the raw member span. Status: **ADOPTED (ADR-0038)**.

**Court.** `tests/odt_adapter.rs` (pre-registered H1–H4) plus the in-file
`src/adapter/odt.rs` tests, over a real ZIP built in-court with no external tool
(stored `mimetype`, `META-INF/manifest.xml`, deflated `content.xml`,
`styles.xml`, `meta.xml`, and a `Pictures/pixel.png` member). Sealed receipt:
`evidence/campaigns/2026-10-07-phase13-odt-95c486d/`.

**Outcome — adopted (ADR-0038).**

| axis | result |
| --- | --- |
| court tests | **9/9** integration (`tests/odt_adapter.rs`) + **6/6** in-file |
| byte-exactness | exact (`length` + SHA-256 + `cmp`); a native `odt-part` exact-bytes observation does not disturb it |
| after source **and** descriptor deletion, fresh process | **exact**: a fresh handle on the store rematerializes identical bytes |
| queryable | common (`metadata`, `text`, `heading`, `block`, `table`, `cell`, `resource`, `link`, `find`) + native (`odt-part`, `odt-paragraph`, `odt-heading`, `odt-table`, `odt-cell`, `odt-list`, `odt-find`) with `format=odt;common;<native>` provenance |
| profiles | `Final` vs `Original` tracked changes differ as specified (`Base added` vs `Base gone`); fingerprint recorded |
| fail-closed | missing/malformed manifest → typed decline (`InvalidPackageStructure` / `InvalidXmlStructure`) with exactness preserved |
| detection | byte-based (`mimetype`/manifest media type), never a file name; a plain ZIP stays opaque |

ODT is the fourth format to enter the shared field on the same terms as the
already-adopted three: `materialize(descriptor) == original_bytes` is unchanged,
the ODF manifest is the sole authority for part identity, and no decoder behavior
is added (enabling `odt` never changes `.voldoc` bytes).

## Byte-level partial-materialization checkpoints (13.4)

**Question.** Phase 8/11 delivered the seek `DIRECTORY`, the partial-descriptor
lane, and the observation engine, but literal byte-level checkpoint records were
never built. Does a persisted per-op checkpoint table, consumed by the seek
reader in place of the `OBSERVATION_INDEX`, reduce bytes read or CPU for a
random-access observation? (Phase 8's measured floor makes a negative the
expected outcome.)

**Mechanism.** An optional `FLAG_OPTIONAL` `CHECKPOINT` record (`RecordTag`
`0x60`, payload `checkpoint_v1`) carries a per-op output-boundary table
(`out_start`/`out_len`), the declared source length, and a `graph_crc32c` binding
to the exact `GRAPH` record. An ignorable optional header bit
`FEATURE_CHECKPOINTS` is set; the universe string is **unchanged** (no new
opcode/coder/limit/adapter meaning). A checkpoint requires a seek `DIRECTORY` to
locate it; the seek reader validates it against the authoritative program
(`Program::analyze_ops`) and, for a raw byte range only, consumes it to select
the op window while reading **no** `OBSERVATION_INDEX`. A lying, corrupt,
non-optional, or out-of-closure checkpoint is rejected — the full parser fails
closed and the seek reader falls back to the Phase-8 index lane, never to a
guess. `materialize(descriptor) == original_bytes` is unchanged.

**Court.** `tests/phase13_checkpoints.rs` (pre-registered H1–H3) over the
deterministic `pdf-make-large` corpus and the `PDF_DEFLATE_REPLAY_RANS_INDEXED`
lane, plus the `flate.pdf` sample, measured with the internal `CountingReader`
(`bytes_read`) and the served `ops_evaluated`. Sealed receipt:
`evidence/campaigns/2026-10-07-phase13-checkpoints-9d1306a/`.

**Outcome — recorded negative (ADR-0039).**

| axis | result |
| --- | --- |
| byte-exactness (where consumed) | **exact**; checkpoint lane and index lane agree byte-for-byte, `ops_evaluated` identical |
| advisory rejection | lying/corrupt/non-optional checkpoint → full parser rejects; reader falls back to the index lane, exact bytes |
| bytes read (20 / 40 / 120 objects) | **+259 / +439 / +1,159 B** per byte-range query (constant per descriptor) |
| descriptor size (20 / 40 / 120 objects) | 94,754→98,984 / 189,073→197,143 / 567,382→590,812 B |
| CPU (op work) | **identical** (no reduction) |

The checkpoint is materially **redundant with the observation index**: the index
already persists the per-op output lengths the checkpoint recomputes, at a
narrower 9 B/op entry, so the checkpoint's 16 B/op table (plus its own record
framing) is larger than the index record it lets the reader skip. It never
reduces either axis, and the `GRAPH` record — the irreducible floor for any lane
that must evaluate ops — dominates regardless. **Closed as a recorded negative**;
the mechanism stays implemented (opt-in) so the negative is reproducible and the
fallback discipline is tested. No cap was raised; no validate-or-decline rule was
weakened.
