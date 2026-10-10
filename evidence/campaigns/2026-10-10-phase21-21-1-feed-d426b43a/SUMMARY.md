# Phase 21.21.1 — RSS/Atom feed court

**Question.** Does the RSS 2.0 / Atom 1.0 feed adapter close exactly and
expose a representation-preserving model (the recorded dialect; exact
element/attribute spans; element order; Atom `link href/rel` and RSS
`guid isPermaLink` attribute spelling; the Atom namespace declaration) on top
of the whole-source exact leaf — while keeping the bounded semantic
sub-detection boundary (before the generic XML detector) and declining
malformed/ambiguous inputs typed?

**Method.** Each self-authored fixture is generated in-court with GNU
`/usr/bin/printf`, ingested via `field-build`, observed, then — after the
**source file and the standalone descriptor are deleted** — rematerialized
exactly in a fresh process and compared with `length + SHA-256 + cmp`. An
out-of-range record and an unknown field are required to decline typed; the
plain-XML / no-channel / non-Atom-feed / no-entry controls and prose pin the
detection boundaries. The court runs in the pinned `dev` service using only
POSIX `sh`, coreutils, git, and the shipped binary (no python3, no jq).

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `rss.xml` | feed | 466 | 466 | true | true | true | 6 | -1 |
| `atom.atom` | feed | 518 | 518 | true | true | true | 6 | -1 |
| `plain.xml` | xml | 43 | 43 | true | true | true | -1 | 6 |
| `nochannel.xml` | xml | 25 | 25 | true | true | true | -1 | 6 |
| `nons.atom` | xml | 38 | 38 | true | true | true | -1 | 6 |
| `noentry.atom` | xml | 59 | 59 | true | true | true | -1 | 6 |
| `prose.txt` | opaque | 70 | 70 | true | true | true | -1 | 6 |

## Counts

```
fixtures 7
feed_fixtures 2
exact_ok 7
exact_fail 0
surface_fail 0
typed_decline_records_ok 2
typed_decline_fields_ok 2
cross_format_controls_ok 4
usage_ok 1
opaque_controls_ok 1
opaque_controls 1
binary dab94091ba30a4f0b6f4e2cfc793f64cd54a9f0266ba5ab44d27911ea959c3d1
```

## Per-fixture observation files (raw/)

- `atom.atom.e0.json`
- `atom.atom.link.json`
- `atom.atom.link.txt.json`
- `atom.atom.metadata.json`
- `atom.atom.search.json`
- `atom.atom.text.json`
- `atom.atom.title1.json`
- `nochannel.xml.feed-decline.json`
- `noentry.atom.feed-decline.json`
- `nons.atom.feed-decline.json`
- `plain.xml.feed-decline.json`
- `prose.txt.metadata.json`
- `results.tsv`
- `rss.xml.channel.json`
- `rss.xml.e0.json`
- `rss.xml.e1.json`
- `rss.xml.find.json`
- `rss.xml.guid.txt.json`
- `rss.xml.language.json`
- `rss.xml.metadata.json`
- `rss.xml.search.json`
- `rss.xml.text.json`

## Scope (honest)

- **Shipped here:** byte-based bounded semantic feed detection; one bounded
  RSS 2.0 / Atom 1.0 model reusing the shared XML parser with a recorded
  dialect; native `feed-channel`/`feed-field`/`feed-entry`/
  `feed-entry-field`/`feed-find`; common `metadata`/`text`/`search-match`.
- **Precedence:** after the strong magic-byte binaries, the JSON family,
  CBOR/MessagePack, TOML, and the config family; before the generic
  standalone-XML and HTML detectors. A feed is a more specific claim than a
  bare XML tree, so it is tried first.
- **Detection boundary:** a feed's physical bytes are XML. RSS is claimed on
  a `<rss>` root with a `<channel>` child; Atom on a `<feed>` root in the
  Atom namespace with at least one `<entry>` child. A plain XML document, an
  `<rss>`-shaped-but-invalid document, a non-Atom `<feed>`, and an Atom
  `<feed>` with no `<entry>` all stay `Xml`; prose stays `Opaque`.
- **Recorded negative (not distinguished):** RSS 1.0 (its root is
  `rdf:RDF`, an RDF graph) and Atom 0.3 (a different namespace URI) are
  **not** recognized and stay `Xml`; only RSS 2.0's `<rss><channel>` shape
  and Atom 1.0's namespace are claimed.
- **Declines:** an out-of-range record, an unknown field, a malformed
  `--feed-entry-field` argument, a cap breach, and a non-feed source are
  typed (`InvalidFeedStructure` rc 35, unsupported-feature rc 6,
  resource-limit rc 8, or usage rc 2); such input stays `Xml`/`Opaque` when
  detection declines.
- **Exactness** is the whole source (a RAW-like `DocumentExact`): the feed
  model is derived (`Q_gen`) and never on the exactness path (ADR-0060: the
  model node depends on the `sha256(source)` root).
- **Not claimed here:** the economic court (separate script).
- **Never run on the host:** every command above ran in the pinned `dev`
  container.
