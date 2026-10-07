# Authority and exactness

The exact profile has one normative outcome. Every other notion of correctness is
a projection, advisory, or disposable.

## The invariant

```text
descriptor:       materialize(descriptor) == original_bytes
persistent field: materialize(field_root)  == original_bytes
```

Every exact court checks all three:

```text
materialized_length == source_length
SHA256(materialized) == SHA256(source)
byte_compare(materialized, source) == equal
```

Parsing successfully, producing the same text, the same object graph, the same
pages, the same rendering, or a canonical re-save are not substitutes
(ADR-0001, ADR-0023). Exactness holds after the source is deleted and across
process restarts, and the derived cache is off the authority path.

## Layered authority

Authority is layered and never confused (ADR-0024, extended to multiple formats
by ADR-0029).

| Plane | Examples | Authority |
|---|---|---|
| Exact reconstruction | DRA program + objects + channels, `INTEGRITY`, whole-source SHA-256 | Normative; `materialize == original_bytes` |
| Format-native procedural state | PDF object/stream/revision; OPC part/relationship; WML paragraph/table/story; EPUB package/manifest/spine | Normative for native observations |
| Shared observation vocabulary | common selector/representation set | A shared vocabulary, never shared semantics |
| Indexes | hierarchical observation index, seek `DIRECTORY` | Advisory; accelerate only; validate-or-decline |
| Derived caches | page preview, text projection | Disposable; off-wire; closure-keyed |
| Trajectory / agent metadata | provenance, comparison, agent verbs | Zero authority |

The document root is exact only through the DRA. A seed `NodeId` never replaces
the whole-source SHA-256; the seed DAG never substitutes for `INTEGRITY`. No
agent, LLM, or search sits on the decode path.

## Physical bytes are the authority; oracles are not

PDF physical bytes are the authority, not a tool's semantic view (ADR-0009). The
owned scanner covers `[0, len)` with a contiguous span partition; qpdf/Poppler/
MuPDF are differential oracles only. The same principle extends to ZIP: a
byte-authoritative physical scanner covers `[0, N)` and a member's exact leaf is
its raw compressed span — no unzip/rezip, and the `zip`/`rawzip` crates are
oracle-only (ADR-0030). A logical member map is a derived observation that can
diverge from the physical cover.

## Rejected

- One lossy universal AST / lowest-common-denominator schema across formats
  (ADR-0029).
- A derived projection promoted to exact state, or a shared node that maps
  different bytes to one store entry (ADR-0034).
- “Rust means the parser is secure”: safety comes from bounds, checked
  arithmetic, fail-closed defaults, and hostile-input courts, not the language.

## Relevant ADRs and evidence

ADRs [0001](../adr/0001-exact-bytes-only.md),
[0004](../adr/0004-wire-format.md), [0005](../adr/0005-bounded-dra.md),
[0009](../adr/0009-pdf-byte-authority.md),
[0023](../adr/0023-consolidated-findings.md),
[0024](../adr/0024-document-field-authority.md),
[0029](../adr/0029-multi-format-authority-model.md),
[0030](../adr/0030-zip-physical-layer.md). Standing invariants and the courts that
enforce them are in [Conformance](../reference/conformance.md); the threat model
is in [Security](../SECURITY.md).
