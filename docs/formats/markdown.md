# Markdown

Markdown is the **prose** format of Phase 21 Wave 2. Like JSON/YAML it is not a
package — the whole source is the document, and the exact leaf is the source.
The adapter is gated behind the **non-default, dependency-free** `markdown = []`
feature.

## Authority boundary

Markdown has **no magic bytes**, and plain prose is itself a valid Markdown
paragraph, so detection is a documented, conservative heuristic: the source must
carry a **structural mark** — an ATX heading, a fenced code block, front matter,
a table, a reference definition, or a footnote definition. Plain prose stays
`Opaque` rather than being guessed at, and still round-trips exactly through the
RAW lane.

## Representation preservation

A bounded, line-based CommonMark subset that keeps, for every construct, its
**exact source byte span** and bytes. The source is never re-flowed or rendered:
a block's exact bytes are literally `source[span]`, and the canonical text
projection is the source itself. Preserved: ATX headings and levels, paragraphs,
ordered/unordered/nested lists, fenced code with language tags, indented code,
blockquotes, GFM tables, inline and reference links and images with targets and
titles, reference definitions, footnotes, and YAML/TOML front matter.

## Supported observations

Common selectors: `metadata`, `text`, `heading`, `block`, `find`. Native:
`--md-heading N`, `--md-block N`, `--md-code`, `--md-link N`, `--md-find PATTERN`.

## Unsupported / honest cost

Setext headings, HTML blocks, and nested inline emphasis inside link text are
left as literal text rather than guessed. It is not a full CommonMark
conformance oracle. The economic court is measured on a **self-authored
deterministic corpus**.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted
Markdown, and after the source **and** descriptor are deleted in a fresh process
(the 21.8.1 court and the 21.8 economic court, exactness **12/12** and **9/9**).
The exact leaf is the whole source; the derived model is never on the exactness
path (ADR-0060: the model node depends on the `DocumentExact` root keyed on
`sha256(source)`).

## Security limits

Bounded block/inline/node/byte caps; a source over any cap declines typed. No
external reference is fetched (link targets are recorded as text, never
dereferenced) and no content is executed.

## Known limitations

A bounded CommonMark subset, not a renderer. The economic court compares a
source-retaining SQLite baseline **and** a conventional Markdown→HTML/text render
baseline; the render lane is inherently lossy (it re-flows, strips markers, drops
source offsets, and cannot reproduce the original bytes — Q8 a typed
`not-native` decline), which is the point of that comparison. One curated **Q6
(lexical find)** mismatch is recorded, not hidden: the conventional load and VOLE
segment blocks differently. Only exact closure is a byte-authority claim.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.8.1 Markdown court: `tools/phase21-8-1-markdown-court.sh`; campaign
  [2026-10-10-phase21-8-1-markdown-0e6a97c3](../../evidence/campaigns/2026-10-10-phase21-8-1-markdown-0e6a97c3/).
- Phase 21.8 economic court: `tools/phase21-8-markdown-court.sh` (SQLite + a
  Markdown→HTML/text render baseline; exactness 9/9); campaign
  [2026-10-10-phase21-8-markdown-econ-0e6a97c3](../../evidence/campaigns/2026-10-10-phase21-8-markdown-econ-0e6a97c3/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
