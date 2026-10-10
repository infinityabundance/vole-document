# EML / MIME

EML/MIME is the **messaging** format of Phase 21 Wave 2. It is not a package —
the whole source is the document, and the exact leaf is the source. The adapter
is gated behind the **non-default, dependency-free** `eml = []` feature.

## Authority boundary

Detection is byte-based and **conservative**: a header block followed by the
message body, with each header of the form `name: value`. A leading Unix-mbox
`From ` envelope line is skipped before the header scan (so an mbox-extracted
message is detected). EML is tried after JSON/JSONL and before
YAML/TOML/CSV/Markdown/XML/HTML. Prose and malformed input stay `Opaque` and
round-trip exactly through the RAW lane.

## Representation preservation

A bounded, span-preserving **message** model: every header's exact
name/value/**full span**, header **order** and **duplicate headers**, **folded
headers**, the resolved `multipart/*` tree, and the exact
`Content-Transfer-Encoding`-decoded constituent bytes (including attachments).

## Supported observations

Common selectors: `metadata`, `text`, `resource`, `find`. Native:
`--eml-header NAME`, `--eml-part N`, `--eml-attachments`, `--eml-body`,
`--eml-find PATTERN`.

## Unsupported / honest cost

A multipart without a boundary, an unknown transfer encoding, a non-UTF-8
charset for text, and encrypted/signed S/MIME are typed declines. A multipart
**root with no boundary** declines detection (keeping detection consistent with
parse). An out-of-range part declines typed, never a silent empty answer.

## Security limits

Bounded part/header/decoded-byte/document caps; a source over any cap declines
typed. Decoded constituents are surfaced as **bytes**; nothing is executed. No
external reference is fetched.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted EML,
and after the source **and** descriptor are deleted in a fresh process (the
21.13.1 court and the 21.13 economic court, exactness **7/7** and **6/6**). The
exact leaf is the whole source; the derived model is never on the exactness path
(ADR-0060: the model node depends on the `DocumentExact` root keyed on
`sha256(source)`).

## Known limitations

A bounded MIME reader, not a mail-client renderer. The economic court compares a
source-retaining SQLite baseline **and** a conventional `email` load baseline;
neither comparator exposes exact header source spans, header order as a
representation, duplicate headers as distinct entries, folded-header spans, or
the resolved MIME tree with exact constituent spans — the `email` module
normalizes all of that. Only exact closure is a byte-authority claim.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.13.1 EML court: `tools/phase21-13-1-eml-court.sh` (exactness 7/7,
  6 typed declines, 1 opaque control); campaign
  [2026-10-10-phase21-13-1-eml-02dc88a7](../../evidence/campaigns/2026-10-10-phase21-13-1-eml-02dc88a7/).
- Phase 21.13 economic court: `tools/phase21-13-eml-court.sh` (SQLite + a
  conventional `email` load baseline; exactness 6/6); campaign
  [2026-10-10-phase21-13-eml-econ-02dc88a7](../../evidence/campaigns/2026-10-10-phase21-13-eml-econ-02dc88a7/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
