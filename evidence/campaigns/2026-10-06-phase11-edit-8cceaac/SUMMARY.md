# Phase 11.12 — immutable node-level edit witness

Commit under test: `8cceaaced3f3ceb29cf51bbc47cac48d11d91289` (branch `phase11`); tree dirty: ``

Image: `vole-document/db-baseline:1.99.0` (id `sha256:41418aa8a7aadd376fa434b82af2c12f09a624dda81b1c3e3b6aa3a4bb738f1d`), base `rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e`.
Rust: `1.99.0 (b940084d7 2026-09-28)` / `1.99.0 (5f94df478 2026-08-27)`; `jq-1.6`; `version 6.1`.
Arch: `x86_64`; `Cargo.lock` sha256: `25fc018b20d57fe22b704fd528cfa9f3dc64969ed7aa0345fb0cd294e913eea2`.

Source: `evidence/corpus/phase7-producers/libreoffice-export.pdf` — `74371B`, 61 pages, sha256 `a853f8e28323ab35f6eda288dc07f36de034560dc03d0a6cf10f8a7a70054529`.

## The edit

One small node-level edit: replace page `1`'s decoded content with a
single-operator content stream (`54` B: `BT /F1 12 Tf 72 720 Td (VOLE EDIT WITNESS PAGE) Tj ET`).

`R0` = `85c4415e41f397c8be8ccdeb7cf7ffd5a2ea51269b7a02649188fb6fe2b77544`
`R1` = `4591fa4fdef1183e688870e74c9646219d0d84176494e4dd98d096ea7f5df672`

## What is genuinely shared vs copied

| quantity | value |
|---|---:|
| index entries carried forward unchanged (same key, same node id) | 318 |
| index entries replaced (the edited page) | 1 |
| index tree nodes that already existed (not rewritten) | 2 |
| index tree nodes written anew | 2 |
| new seed nodes (Literal + PageContent) | 2 |
| payload bytes newly persisted (all nodes + manifest) | 8776 |
| descriptor bytes read by the edit | 0 |
| descriptor blobs opened by the edit (strace) | 0 |
| manifest bytes read by the edit | 246 |
| index bytes read by the edit | 43688 |


Shared by id (never rewritten): the descriptor blob, the `DocumentExact` root,
every unaffected seed node, and every index node that already existed.
Newly written: two seed nodes, the rewritten index leaf/spine, and one manifest.

## Exactness (both roots, after the edit)

| root | length | sha256 | cmp vs source |
|---|---:|---|---|
| R0 (before edit) | 74371 | `a853f8e28323ab35f6eda288dc07f36de034560dc03d0a6cf10f8a7a70054529` | equal |
| R0 (after edit) | 74371 | `a853f8e28323ab35f6eda288dc07f36de034560dc03d0a6cf10f8a7a70054529` | equal |
| R1 (new root) | 74371 | `a853f8e28323ab35f6eda288dc07f36de034560dc03d0a6cf10f8a7a70054529` | equal |

Both roots materialize the **same original bytes**: the exact archive is
untouched by construction, so this is a procedural edit of a derived page,
not a rewrite of the PDF.

## Observation (edited page 1, unaffected page 2)

- R1 page 1 text contains the marker `VOLE EDIT WITNESS PAGE`: `true`
- R0 page 1 text is byte-identical before and after the edit: `true`
- R0 and R1 page 2 text are byte-identical (unaffected page shared): `true`
- R1 page 1 structure `content_streams`: `[]` (empty: the new bytes are not in the source)

```json
{
  "edit_page": {
    "page": 1,
    "r0_text": "\u0005\u0006\n\u0005\u001f\n#\n\u0005$\n\u0005%\n#\n\u0005'\n\u0005(\n#\n#\n\u0006\n#\n \n$\n#\n&\n'\n#\n)\n#\n#\n\u001f\n \n#\n%\n&\n#\n(\n",
    "r0_text_after_edit": "\u0005\u0006\n\u0005\u001f\n#\n\u0005$\n\u0005%\n#\n\u0005'\n\u0005(\n#\n#\n\u0006\n#\n \n$\n#\n&\n'\n#\n)\n#\n#\n\u001f\n \n#\n%\n&\n#\n(\n",
    "r1_text": "VOLE EDIT WITNESS PAGE\n",
    "r1_text_contains_marker": true,
    "r0_structure": {
      "page": 1,
      "text_bytes": 62,
      "draw_ops": 0,
      "path_ops": 1,
      "content_streams": [
        5
      ]
    },
    "r1_structure": {
      "page": 1,
      "text_bytes": 23,
      "draw_ops": 0,
      "path_ops": 0,
      "content_streams": []
    },
    "r0_after_matches_before": true
  },
  "unchanged_page": {
    "page": 2,
    "r0_text": ")\n#\n\u0006\n\u001f\n#\n$\n%\n#\n'\n(\n#\n#\n\u0006\n#\n \n$\n#\n&\n'\n#\n)\n#\n#\n\u001f\n \n#\n%\n&\n",
    "r1_text": ")\n#\n\u0006\n\u001f\n#\n$\n%\n#\n'\n(\n#\n#\n\u0006\n#\n \n$\n#\n&\n'\n#\n)\n#\n#\n\u001f\n \n#\n%\n&\n",
    "identical": true
  }
}
```

## Verdict

All witness checks pass: `true`.

- `r0_exact_before`: true
- `r0_exact_after_edit`: true
- `r1_exact_same_original_bytes`: true
- `r0_unchanged_by_edit`: true
- `r1_shows_edited_page`: true
- `unaffected_page_shared`: true
- `edit_used_content_addressed_nodes`: true
- `unaffected_bindings_reused`: true
- `unaffected_index_nodes_shared`: true
- `edit_read_no_descriptor_bytes`: true
- `edit_opened_no_descriptor_blob`: true

## Supported subset (narrow, stated honestly)

This court demonstrates **exactly one** operation: replacing one existing
page's decoded content bytes in an already-indexed field, with new content
`<= MAX_EDIT_CONTENT_BYTES = 49152` bytes. The exact `.voldoc` descriptor is
copied verbatim, so `materialize(R1) == materialize(R0) == the original
source`; only the *derived page observations* change. There is:

- no generic document editing (no insert/delete/reorder, no object-graph or
  cross-reference mutation, no re-encoding);
- **no authorial-intent claim** — the edit is a procedural override of a
  decoded page projection, not a new PDF;
- no whole-document rewrite: the edit reads **0** descriptor bytes and opens
  **0** descriptor blobs (witnessed by `strace`);
- no source span for the edited page (its bytes are not in the source), and
  no content-stream object numbers in its `structure` observation;

## Honest losses / limitations

- The edit reads the whole hierarchical index (all leaves) to carry untouched
  selector bindings forward: `index_bytes_read = 43688 B`,
  against `bytes_newly_persisted = 8776 B`. The read is
  descriptor-free but is not free.
- Index-*node* reuse requires a multi-node index; a single-leaf index
  necessarily rewrites its only leaf (reuse is then witnessed at entry level,
  which the counters report separately).
- The edited page's materialized *source span* is lost, and `structure`
  reports no content streams for it; `text` and `preview` are computed from
  the new bytes as usual.
