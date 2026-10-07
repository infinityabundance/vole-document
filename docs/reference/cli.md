# CLI reference

The `vole-document` binary is the only user-facing entry point. Commands are
selected by the first argument; the *format* of an input is detected from its
bytes, never from the file extension or a flag.

Feature-gated command groups appear only when built in: the entropy stack
(`rans`, default), the object store (`store`, default), and the document field
(`field`, default; DOCX/EPUB need `package,opc,docx,epub`).

## Core container

```text
vole-document encode      [--force KIND] INPUT  OUTPUT.voldoc
vole-document decode      INPUT.voldoc   OUTPUT
vole-document materialize INPUT.voldoc   OUTPUT
vole-document verify      INPUT.voldoc
vole-document inspect     INPUT.voldoc
vole-document capabilities [ROOT]
```

- `encode` runs the complete-cost court and writes the smallest exact
  descriptor. `--force KIND` restricts the court to one candidate family for
  honest per-mechanism ablation; it never bypasses exactness, and it fails with
  a typed usage error if the input does not propose that kind.
- `decode` and `materialize` are synonyms and produce the exact source bytes.
- `verify` (re)checks integrity and the coverage certificate without writing.
- `inspect` prints descriptor structure. `capabilities ROOT` detects `ROOT`'s
  document format from bytes and prints the supported selectors/representations.

`KIND` for `encode --force`:

```text
raw | rle | byte-rans | pdf-physical | pdf-channels | pdf-layout |
pdf-layout-rans | pdf-deflate-replay | pdf-deflate-replay-rans |
pdf-deflate-replay-rans-indexed | pdf-length-revision | pdf-cos-template
```

## Partial views (`rans`)

```text
vole-document view INPUT.voldoc [OUTPUT] --byte-range A:L | --pdf-object N:G |
    --pdf-stream N:G | --pdf-revision I [--stats]
```

Serves one byte range, indirect object, encoded stream, or revision from the
advisory seek `DIRECTORY` and observation index, checking every served slice
against the full materialization. Exactly one selector is required. A partial
read is an *observation* (`integrity_verified == false`); `materialize`/`decode`/
`verify` remain the archival authority.

## Object store (`store`)

```text
vole-document decode --store STORE_DIR INPUT.voldoc OUTPUT
vole-document store put     INPUT.voldoc STORE_DIR
vole-document store account STORE_DIR ROOT...
vole-document store gc      STORE_DIR ROOT...
```

`--store` externalizes the descriptor's objects into a content-addressed store;
`store account` reports the three accounting universes (standalone,
unique-reachable, amortized); `store gc` is a mark-and-sweep over the referenced
roots.

## Persistent document field (`field`)

```text
vole-document field-ingest INPUT.voldoc --store DIR [--entropyfs]
vole-document field-edit   --store DIR --field HEX --page N --content FILE [--entropyfs]
vole-document observe      --store DIR --field HEX (--page N | --object N | --stream N |
    --revision N | --byte-range A..B | --metadata | --doc-text | --heading N |
    --block N | --table N | --cell T:R:C | --resource N | --link N |
    --spine-item N | --text PATTERN) --kind KIND [--entropyfs]
vole-document find         --store DIR --field HEX --text PATTERN [--entropyfs]
vole-document explain      --store DIR --field HEX <selector> --kind KIND [--analyze] [--entropyfs]
vole-document preview      --store DIR --field HEX --page N [--json] [--entropyfs]
vole-document materialize  --store DIR --field HEX --exact --output FILE [--entropyfs]
vole-document cache        --store DIR [--clear] [--entropyfs]
vole-document field-store-stats --store DIR [--entropyfs]
```

- `field-ingest` inverse-proceduralizes one `.voldoc` into a persistent field
  and prints its field id (`HEX`) and roots. The format is detected from bytes.
- `observe` / `find` / `explain` / `preview` answer typed observations with
  provenance; `--kind` is one of
  `metadata|text|structure|operators|encoded|decoded|exact|preview|full`.
- `explain --analyze` executes the plan and reports the actual work (bytes read
  by class, nodes executed vs reused, decodes, wall/CPU).
- `materialize --exact` reconstructs the original bytes from the field alone.
- `cache --clear` reclaims the disposable derived cache.

`--entropyfs` requires a build with the `entropyfs-store` feature.

## Fine-unit sharing (`field`)

```text
vole-document share report INPUT.voldoc...
vole-document share-account --store DIR INPUT.voldoc...
vole-document share externalize --store DIR INPUT.voldoc OUTPUT.voldoc
```

## Tooling subcommands

```text
vole-document pdf-inspect INPUT
vole-document pdf-make-samples DIR
vole-document pdf-make-large DIR [OBJECTS]
vole-document deflate-stats INPUT...        # only with deflate-replay
```

These generate deterministic corpora and diagnostics for the courts; `pdf-make-large`
assembles a valid classic-xref PDF of `OBJECTS` distinct zlib streams (≥32 MiB).

## Exit codes

```text
0 ok                  2 usage               3 io
4 invalid-container   5 unsupported-version 6 unsupported-feature
7 integrity-mismatch  8 resource-limit      9 invalid-graph
10 invalid-model      15 coverage-violation 16 reconstruction-mismatch
70 internal-invariant
```

File output is written atomically: a temporary sibling is written, fsynced, then
renamed into place, so a partial or failed operation never replaces the
destination.
