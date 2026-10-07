# Phase 13.5 — gate `N5` (package-index-only)

Pre-registered in `tools/phase13-5-n5-court.sh`. Runs on the frozen 12.11
DOCX/EPUB corpus (`tools/fixtures/phase12-corpus-gen.py`) and its pre-registered
schedule (`tools/fixtures/phase12-lifetime-schedule.json`).

## Result

| lane | semantic (structural) | literal search | whole-source/decoded | all |
|---|---:|---:|---:|---:|
| Phase-12 field (A11) | 72/72 | 8/8 | 12/16 | 92/96 |
| package index only (`zipfile.read` + byte substring) | 0/72 | 8/8 | 8/16 | 16/96 |

The package-index-only lane resolves only the whole-source and literal-search
selectors; it resolves **0/72** structural selectors (block /
heading / table / cell / resource / metadata / provenance), each of which needs
format-native parsing. Under the *generous* reading that an answer merely needs
to occur somewhere in a raw member, only **59/96** expected
values are byte-reachable, and even then the selector→answer mapping is
unresolved.

The field's four non-passes are all the same pre-existing limitation: the DOCX
adapter declines the *decoded* resource observation (`resource:N --kind decoded`,
typed UnsupportedFeature, rc=6). This is recorded in 12.11 (`V:declined`) and
12.11b (`a2..a11:declined`); it is not a regression here. On that one selector
the package index is *stronger* than the field (it can return a raw member) —
which does not rescue `N5`, since that selector is a raw-bytes case, not a
semantic one.

## Verdict

**N5-FALSIFIED.** The small-document win is not reproducible by `unzip -p` +
`substr`: the content adapters, not the package index, answer the semantic
surfaces (consistent with the 12.11b ladder, A3/A4 vs A5).

## Receipt

`raw/assertions.tsv` (field lane, per case), `raw/n5-control.json` (control, per
case), `raw/n5-control.txt`, `receipt.json`.
