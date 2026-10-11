# Phase 21.27.1 — MHTML (MIME HTML) court

**Question.** Does MHTML — a browser-saved MIME `multipart/related` web
archive — close exactly and expose a representation-preserving model (the MIME
envelope; the ordered parts with their exact spans; the ordered sub-resources
keyed by `Content-Location`/`Content-ID`; the root HTML part parsed by the
reused HTML scanner; quoted-printable/base64 parts decoded for observations
while the raw encoded bytes stay in the source) on top of the whole-source exact
leaf, while keeping an MHTML-**specific** detection boundary (a plain
`multipart/related` email stays `eml`, a plain HTML document stays `html`,
and plain prose stays `opaque`)?

**Method.** Each self-authored fixture is generated in-court with GNU
`/usr/bin/printf`, ingested via `field-build`, observed, then — after the
**source file and the standalone descriptor are deleted** — rematerialized
exactly in a fresh process and compared with `length + SHA-256 + cmp`. An
out-of-range native resource and an unsupported common pair are required to
decline typed; a malformed native argument is a usage error; the prose, HTML,
plain MIME, and two `multipart/related` email controls pin the detection
boundaries. The court runs in the pinned `dev` service using only POSIX `sh`,
coreutils, git, and the shipped binary (no python3, no jq).

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | sha256 match | cmp | exact | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: |
| `base.mhtml` | mhtml | 876 | 876 | true | true | true | -1 |
| `nostart.mhtml` | mhtml | 322 | 322 | true | true | true | -1 |
| `envelope.mhtml` | mhtml | 158 | 158 | true | true | true | -1 |
| `large.mhtml` | mhtml | 67225 | 67225 | true | true | true | -1 |
| `prose.txt` | opaque | 92 | 92 | true | true | true | 6 |
| `plain.html` | html | 54 | 54 | true | true | true | -1 |
| `plain.eml` | eml | 87 | 87 | true | true | true | -1 |
| `related.eml` | eml | 155 | 155 | true | true | true | -1 |
| `related_no_marker.eml` | eml | 118 | 118 | true | true | true | -1 |

## Counts

```
fixtures 9
mhtml_fixtures 4
control_fixtures 5
exact_ok 9
exact_fail 0
surface_fail 0
boundary_ok 5
opaque_controls_ok 1
typed_declines_ok 1
usage_ok 1
binary a12902b153125c9e984b2d2de63f56d241ca42ddab792a89ee37b8eb209ca126
```

## Per-fixture observation files (raw/)

- `base.mhtml.common.resource.bin`
- `base.mhtml.common.search.json`
- `base.mhtml.find.json`
- `base.mhtml.find.stylesheet.json`
- `base.mhtml.location.json`
- `base.mhtml.metadata.json`
- `base.mhtml.res0.json`
- `base.mhtml.res1.bin`
- `base.mhtml.res1.json`
- `base.mhtml.root.json`
- `base.mhtml.root.text.json`
- `base.mhtml.text.json`
- `build.log`
- `envelope.mhtml.common.search.json`
- `envelope.mhtml.find.json`
- `envelope.mhtml.location.json`
- `envelope.mhtml.metadata.json`
- `envelope.mhtml.root.json`
- `envelope.mhtml.root.text.json`
- `envelope.mhtml.text.json`
- `large.mhtml.common.search.json`
- `large.mhtml.find.json`
- `large.mhtml.location.json`
- `large.mhtml.metadata.json`
- `large.mhtml.root.json`
- `large.mhtml.root.text.json`
- `large.mhtml.text.json`
- `nostart.mhtml.common.search.json`
- `nostart.mhtml.find.json`
- `nostart.mhtml.location.json`
- `nostart.mhtml.metadata.json`
- `nostart.mhtml.root.json`
- `nostart.mhtml.root.text.json`
- `nostart.mhtml.text.json`
- `plain.eml.metadata.json`
- `plain.html.metadata.json`
- `prose.txt.metadata.json`
- `related.eml.metadata.json`
- `related_no_marker.eml.metadata.json`
- `results.tsv`

## Scope (honest)

- **Shipped here:** byte-based, conservative, MHTML-specific detection before
  EML; the reuse of the EML MIME layer and the HTML scanner; the ordered
  sub-resource table; the canonical derived model; native `mhtml-root`/
  `mhtml-resource`/`mhtml-location`/`mhtml-find`; common `metadata`/`text`/
  `resource`/`search-match`.
- **Exactness** is the whole source (a RAW-like `DocumentExact`): the MHTML model
  is derived (`Q_gen`) and never on the exactness path (ADR-0060: the model node
  depends on the `sha256(source)` root, so no source-reading node aliases another
  field's source).
- **Encodings** are decoded only for a derived observation: the exact raw
  (encoded) body span is retained, so the decoded bytes are never a substitute
  for the encoded form (and vice versa).
- **The boundary is honest:** a plain `multipart/related` message with a
  `From`+`Subject` envelope and a `text/html` part is admitted as MHTML (the
  envelope signal is genuine); it is a documented over-approximation. Content-Base
  URL resolution is never performed.
- **Not claimed here:** a browser/rendering oracle, CSS/JS execution, or
  sub-resource URL resolution.
- **Never run on the host:** every command above ran in the pinned `dev`
  container.
