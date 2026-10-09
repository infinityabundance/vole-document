# YAML

YAML is the second **structured-tree** format (Phase 21 Wave 2). Like JSON it is
not a package: the whole source is the document, and the exact leaf is the source
itself. The adapter is gated behind the **non-default, dependency-free**
`yaml = []` feature.

## Authority boundary

Detection is byte-based and **conservative**: the source is not PDF, not a ZIP
family, not admitted by the JSON rule, and the *whole* source parses as YAML under
the limits **with a mapping or sequence at every document root**. A bare scalar or
a plain-text blob stays `Opaque` and round-trips exactly through the RAW lane.
YAML decoding is a derived (`Q_gen`) judgement and never gates exactness.

## Representation preservation (the point of the format)

A "YAML → JSON" pipeline destroys most of what makes YAML a document. This adapter
preserves:

* **exact source spans** for every node and scalar;
* **anchors** (`&a`) and **aliases** (`*a`) as a **graph** — an alias node reports
  the anchor it targets and is never expanded-and-rewritten;
* **tags** (`!!str`, `!<...>`, custom) as the literal tag text;
* **multiple documents** (`---`/`...`) as an ordered document list;
* **scalar styles** — plain, single-quoted, double-quoted, literal `|`, folded
  `>` — as distinct kinds with their exact bytes;
* **merge keys** (`<<`) surfaced as such, not silently merged;
* **mapping order** and **duplicate keys**;
* **comment spans**.

## Supported subset

Streams of documents (`---`/`...`); block mappings/sequences (including compact
`- key: v`); flow `[…]`/`{…}`; scalar styles plain/single/double/literal/folded;
anchors, tags, aliases; merge keys; mapping order, duplicate keys, comment spans.
Observations: native `--yaml-path`, `--yaml-node`, `--yaml-documents`,
`--yaml-anchor NAME` (the anchor + its aliases), `--yaml-find`; common `metadata`,
`text`, `find`.

## Unsupported (declined typed, never guessed)

`%YAML`/`%TAG` directives; explicit keys (`?`); flow-collection and anchored/
tagged/aliased mapping keys; multi-line plain/flow scalars and quoted-scalar line
continuations; content on the same line as `---`; tab indentation. Each is a typed
decline (`InvalidYamlStructure`, exit 22, or `UnsupportedFeature`), never a silent
approximation.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted YAML,
and after the source **and** descriptor are deleted in a fresh process (the 21.6.1
court and the 21.6 economic court, exactness **10/10** and **8/8**). The exact leaf
is the whole source; the derived model is never on the exactness path (ADR-0060:
the model node depends on the `DocumentExact` root keyed on `sha256(source)`).

## Security limits

Bounded depth (`max_yaml_depth`), nodes (`max_yaml_nodes`), scalars
(`max_yaml_scalars`), anchors (`max_yaml_anchors`), documents
(`max_yaml_documents`), string bytes (`max_yaml_string_bytes`), and document bytes
(`max_yaml_document_bytes`); over any cap declines typed. No external reference is
ever fetched.

## Known limitations

This is a **bounded** YAML 1.2 core subset, not a full YAML processor (see
"Unsupported"). The economic court is measured on a **self-authored deterministic
corpus**; only exact closure is a byte-authority claim. The SQLite comparator
normalizes YAML to JSON, which is inherently lossy — that loss *is* the
representation-preservation value VOLE claims.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.6.1 YAML court: `tests/yaml_adapter.rs`; campaign
  `evidence/campaigns/2026-10-09-phase21-6-1-yaml-ceab8ec/`.
- Phase 21.6.2 economic court: `tools/phase21-6-yaml-court.sh`;
  campaign `evidence/campaigns/2026-10-09-phase21-6-yaml-econ-ceab8ec/`.
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
