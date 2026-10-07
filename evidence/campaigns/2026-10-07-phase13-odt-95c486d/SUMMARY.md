# Campaign: 2026-10-07-phase13-odt-95c486d — Phase 13.3

ODT (OpenDocument Text) adapter over the shared byte-authoritative ZIP layer and
the bounded-XML policy. An `.odt` is an **ODF** package (mandatory stored
`mimetype` + `META-INF/manifest.xml`), not OPC, so the main content part is
discovered **semantically** from the ODF manifest, never a hardcoded path.

Pre-registered claims:

- **H1 byte-exactness.** `materialize(field) == original_bytes` (length +
  SHA-256 + `cmp`) for a hand-built minimal ODT, including after the source and
  descriptor are deleted in a fresh process.
- **H2 queryable.** Common (`metadata`, `text`, `heading`, `block`, `table`,
  `cell`, `resource`, `link`, `find`) and native (`odt-part`, `odt-paragraph`,
  `odt-heading`, `odt-table`, `odt-cell`, `odt-list`, `odt-find`) selectors
  resolve with `format=odt;common;<native>` provenance.
- **H3 profiles.** `OdtExtractProfile` tracked changes `Final` vs `Original`
  differ as specified and the identity is recorded.
- **H4 fail-closed.** A missing or malformed `META-INF/manifest.xml` is a typed
  decline (`InvalidPackageStructure`/`InvalidXmlStructure`) that leaves exactness
  untouched.

## Result

```json
{
  "court": "tests/odt_adapter.rs",
  "integration_tests": 9,
  "integration_passed": 9,
  "in_file_tests": 6,
  "in_file_passed": 6,
  "detection": "odt",
  "byte_exact_len_sha256_cmp": true,
  "removal_fresh_process_exact": true,
  "missing_manifest_decline": "InvalidPackageStructure",
  "malformed_manifest_decline": "InvalidXmlStructure",
  "profile_final": "Base added",
  "profile_original": "Base gone"
}
```

The court builds a real ZIP with no external tool: first member `mimetype`
(stored) = `application/vnd.oasis.opendocument.text`, then
`META-INF/manifest.xml`, a deflated `content.xml`, `styles.xml`, `meta.xml`, and a
`Pictures/pixel.png` member.

`cargo test --all-features --test odt_adapter` → **9 passed / 0 failed**.
In-file (`src/adapter/odt.rs`) → **6 passed / 0 failed**.

## What the court establishes

1. **Detection is byte-based** (`mimetype`/manifest media type), never a name; a
   plain ZIP is opaque.
2. **Content** resolves: heading `Title` (level 1), body paragraph, `r7c2` table
   cell by physical `(table, row, col)`, link text/href (external-inert), resource
   href + package member, list items, notes, sections.
3. **Profiles** differ: tracked-changes `Final` keeps the insertion and drops the
   deletion (`Base added`); `Original` does the reverse (`Base gone`); the profile
   fingerprint is recorded in provenance.
4. **Exactness**: `materialize_exact == source` (length + SHA-256 + `cmp`), and a
   native `odt-part` exact-bytes observation does not disturb it.
5. **Removal + fresh process**: after deleting the physical source and the
   external `.voldoc`, a fresh handle on the store directory rematerializes the
   identical bytes.
6. **Progressive inversion**: a re-observation after reopening the store reuses
   persisted derived state (`seed_nodes_reused >= 1`).
7. **Fail-closed**: missing manifest → `InvalidPackageStructure`; malformed
   manifest → `InvalidXmlStructure`; both preserve exactness.

## Honest limits

- The content model is a **bounded OpenDocument subset**, not a full ODF 1.2
  render. Point `text:change` content is substituted only inside an open
  paragraph; nested lists/tables are flattened into their enclosing construct;
  whitespace-only text nodes are dropped; a `draw:image` reference is resolved by
  package-relative path, not through a manifest relationship.
- Generic OPC `package-part` is **not** offered for ODT (ODF has no
  `[Content_Types].xml`); ODF package parts are exposed via `odt-part`.
- No intrinsic pagination: `Page(n)` is a typed decline and is never synthesized.
- One locally generated fixture; **no population claim**.
