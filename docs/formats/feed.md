# RSS / Atom feeds

RSS 2.0 / Atom 1.0 is the **syndication** format of Phase 21 Wave 2 (subphase
21.21). A feed is XML, so it is not a package — the whole source is the document,
and the exact leaf is the source. The adapter **reuses the shared bounded XML
parser** and is gated behind the **non-default** `feed = ["xml"]` feature.

## Authority boundary

A feed's physical bytes are XML, so detection is a **bounded semantic
sub-detection** run **before** the generic standalone-XML detector. RSS is
claimed on a `<rss>` root with a `<channel>` child; Atom on a `<feed>` root in
the Atom namespace (`http://www.w3.org/2005/Atom`) with at least one `<entry>`
child. A plain XML document, an `<rss>`-shaped-but-invalid document, a non-Atom
`<feed>`, and an Atom `<feed>` with no `<entry>` all stay `Xml`; prose stays
`Opaque` and round-trips exactly through the RAW lane.

**What it cannot distinguish.** RSS 1.0 (its root is `rdf:RDF`, an RDF graph)
and Atom 0.3 (a different namespace URI) are **not** recognized and stay `Xml`;
only RSS 2.0's `<rss><channel>` shape and the Atom 1.0 namespace are claimed. The
dialect (`rss`/`atom`) is a recorded property of the model.

## Representation preservation (the point of the format)

A bounded feed projection over the shared XML element/attribute tree preserves
the recorded dialect; exact element/attribute spans; element order; attribute
spelling (Atom `<link href=… rel=…>`, RSS `<guid isPermaLink=…>`); and the Atom
namespace declaration. Every answer's spans are `Q_gen` projections of the XML
parse.

## Supported observations

Common selectors: `metadata`, `text`, `search-match`. Native: `feed-channel`,
`feed-field`, `feed-entry`, `feed-entry-field`, `feed-find`.

## Unsupported / honest cost

A general RSS/Atom feed reader (RSS 1.0/RDF, Atom 0.3, OPML, podcast/feed
extensions, date parsing) is not claimed. An out-of-range record, an unknown
field, a malformed `--feed-entry-field` argument, a cap breach, and a non-feed
source are typed declines (`InvalidFeedStructure`, exit 35; `unsupported-feature`,
exit 6; `resource_limit`, exit 8; usage, exit 2); `Page(n)` is a typed decline.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted feed,
and after the source **and** descriptor are deleted in a fresh process. The exact
leaf is the whole source; the derived model is never on the exactness path
(ADR-0060: the model node depends on the `DocumentExact` root keyed on
`sha256(source)`). Adapter-court H1 exactness **7/7** (2 feed fixtures + 4
plain-XML/no-channel/non-Atom/no-entry controls + 1 opaque control); 4 cross-format
XML controls.

## Economic court

Measured by `tools/phase21-21-feed-court.sh` (a thin wrapper over the shared
`textfmt-court.sh` engine) against two conventional comparators — a
**source-retaining store** plus conventional extraction, and a conventional
**decode-to-host-values load**. Corpus 7 fixtures, questions Q1–Q12, exactness
**11/11**. Estimator = paired per-fixture ratio VOLE/comparator, median +
geometric mean with a fixed-seed, fixture-clustered 95 % CI; ratio-of-sums
reported separately. A ratio < 1 favours VOLE.

| metric | comparator | median | geomean | median 95% CI | geomean 95% CI | W/T/L | ratio-of-sums |
|---|---|---:|---:|---|---|---:|---:|
| build | sqlite | 0.966 | 0.968 | 0.949..0.995 | 0.947..0.992 | 0/7/0 | 0.968 |
| build | conv | 0.960 | 0.945 | 0.920..0.995 | 0.891..0.986 | 1/6/0 | 0.937 |
| storage | sqlite | 0.655 | 0.680 | 0.646..0.668 | 0.649..0.737 | 7/0/0 | 0.846 |
| storage | conv | 0.626 | 0.618 | 0.620..0.637 | 0.594..0.633 | 7/0/0 | 0.556 |
| cold | sqlite | 0.030 | 0.036 | 0.030..0.034 | 0.030..0.051 | 7/0/0 | 0.041 |
| warm | sqlite | 0.120 | 0.144 | 0.108..0.124 | 0.111..0.228 | 7/0/0 | 0.415 |
| warm | conv | 0.120 | 0.151 | 0.114..0.122 | 0.116..0.251 | 7/0/0 | 0.478 |

Build is ~parity/slightly faster (median ~0.96); storage ~0.66× SQLite and ~0.63×
the conventional load; warm ~0.12× both.

## Honest negatives

* **Older namespaces stay `Xml`**: RSS 1.0/RDF and Atom 0.3 are not recognized.
* The feed positional grammar (dates, `link` relation semantics) is not
  validated; the model preserves rather than interprets.
* The conventional load is deliberately the weaker comparator; a
  span-preserving loader is not built here.
* The economic court is measured on a **self-authored deterministic corpus**;
  only exact closure (Q6) is a byte-authority claim.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.21.1 feed adapter court: `tools/phase21-21-1-feed-court.sh` (exactness
  7/7); campaign
  [2026-10-10-phase21-21-1-feed-d426b43a](../../evidence/campaigns/2026-10-10-phase21-21-1-feed-d426b43a/).
- Phase 21.21 economic court: `tools/phase21-21-feed-court.sh` (exactness 11/11);
  campaign
  [2026-10-10-phase21-21-feed-econ-735d9b69](../../evidence/campaigns/2026-10-10-phase21-21-feed-econ-735d9b69/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
