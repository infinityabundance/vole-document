# Phase 12.13 — security + hostile-input court

**Result: 315/315 assertions passed, 0 failed.** Corpus determinism: **ok**.

Every fixture in the committed hostile corpus (`tests/fixtures/phase12-hostile`) was pushed through
the universal ingest/observe path inside the capped `doc-baseline` service. The
court asserts: never panic; never fetch the network (strace); never spawn a
foreign process (strace); never touch a canary file outside the tree (no
member-name path escape); a typed error, never `InternalInvariant`; and
`materialize --exact == original` for every preserved case.

| fixture | category | predict | observed | class | exact |
|---|---|---|---|---|---|
| docx_ambiguous_main.docx | opc | reject | reject | InvalidPackageStructure | yes |
| docx_bad_content_types.docx | opc | reject | reject | InvalidPackageStructure | yes |
| docx_deep_nesting.docx | xml | reject | reject | InvalidXmlStructure | yes |
| docx_doctype.docx | xml | reject | reject | InvalidXmlStructure | yes |
| docx_duplicate_part.docx | opc | reject | reject | InvalidPackageStructure | yes |
| docx_entity_bomb.docx | xml | reject | reject | InvalidXmlStructure | yes |
| docx_external_main.docx | opc | reject | reject | InvalidPackageStructure | yes |
| docx_missing_main_rel.docx | opc | reject | reject | InvalidPackageStructure | yes |
| docx_non_utf8.docx | xml | reject | reject | InvalidXmlStructure | yes |
| docx_nul.docx | xml | reject | reject | InvalidXmlStructure | yes |
| docx_ok.docx | opc | accept | accept | - | yes |
| docx_oversized_text.docx | xml | accept | accept | - | yes |
| docx_traversal_part.docx | opc | reject | reject | InvalidPackageStructure | yes |
| epub_bad_container.epub | epub | reject | reject | InvalidXmlStructure | yes |
| epub_bad_mimetype.epub | epub | accept | accept | - | yes |
| epub_bad_opf.epub | epub | reject | reject | InvalidXmlStructure | yes |
| epub_doctype_xhtml.epub | epub | reject | reject | InvalidXmlStructure | yes |
| epub_encrypted.epub | epub | accept | accept | - | yes |
| epub_missing_opf.epub | epub | reject | reject | InvalidPackageStructure | yes |
| epub_ok.epub | epub | accept | accept | - | yes |
| epub_remote.epub | epub | accept | accept | - | yes |
| epub_scripted.epub | epub | accept | accept | - | yes |
| epub_spine_inconsistent.epub | epub | reject | reject | InvalidPackageStructure | yes |
| z_absolute_name.zip | zip-identity | decline | opaque-preserve-and-decline | UnsupportedFeature | yes |
| z_backslash_name.zip | zip-identity | decline | opaque-preserve-and-decline | UnsupportedFeature | yes |
| z_bad_cd_offset.zip | zip-structure | decline | opaque-preserve-and-decline | UnsupportedFeature | yes |
| z_bad_crc.zip | zip-resource | decline | opaque-preserve-and-decline | UnsupportedFeature | yes |
| z_bad_descriptor.zip | zip-structure | decline | opaque-preserve-and-decline | UnsupportedFeature | yes |
| z_bad_eocd_comment.zip | zip-structure | decline | opaque-preserve-and-decline | UnsupportedFeature | yes |
| z_drive_name.zip | zip-identity | decline | opaque-preserve-and-decline | UnsupportedFeature | yes |
| z_duplicate_names.zip | zip-identity | decline | opaque-preserve-and-decline | UnsupportedFeature | yes |
| z_empty.zip | zip-benign | decline | opaque-preserve-and-decline | UnsupportedFeature | yes |
| z_encrypted.zip | zip-resource | decline | opaque-preserve-and-decline | UnsupportedFeature | yes |
| z_eps_stored.zip | zip-benign | decline | opaque-preserve-and-decline | UnsupportedFeature | yes |
| z_huge_declared.zip | zip-resource | decline | opaque-preserve-and-decline | UnsupportedFeature | yes |
| z_multidisk.zip | zip-structure | decline | opaque-preserve-and-decline | UnsupportedFeature | yes |
| z_no_eocd.zip | zip-structure | decline | opaque-preserve-and-decline | UnsupportedFeature | yes |
| z_nul_name.zip | zip-identity | decline | opaque-preserve-and-decline | UnsupportedFeature | yes |
| z_overlap.zip | zip-structure | decline | opaque-preserve-and-decline | UnsupportedFeature | yes |
| z_prefix_stub.zip | zip-benign | decline | opaque-preserve-and-decline | UnsupportedFeature | yes |
| z_ratio_bomb.zip | zip-resource | decline | opaque-preserve-and-decline | UnsupportedFeature | yes |
| z_traversal_name.zip | zip-identity | decline | opaque-preserve-and-decline | UnsupportedFeature | yes |
| z_truncated.zip | zip-structure | decline | opaque-preserve-and-decline | UnsupportedFeature | yes |
| z_unknown_method.zip | zip-resource | decline | opaque-preserve-and-decline | UnsupportedFeature | yes |
| z_zip64_inconsistent.zip | zip-structure | decline | opaque-preserve-and-decline | UnsupportedFeature | yes |

## Outcome-class tally

```
accept                           7
opaque-preserve-and-decline      22
reject                           16
```

## Universal invariants

```
1 PASS  docx_ambiguous_main.docx.no_hang   bounded
1 PASS  docx_ambiguous_main.docx.no_network clean
1 PASS  docx_ambiguous_main.docx.no_panic  clean
1 PASS  docx_ambiguous_main.docx.no_path_escape canary-intact
1 PASS  docx_ambiguous_main.docx.no_subprocess clean
1 PASS  docx_ambiguous_main.docx.opaque_exact yes
1 PASS  docx_bad_content_types.docx.no_hang bounded
1 PASS  docx_bad_content_types.docx.no_network clean
1 PASS  docx_bad_content_types.docx.no_panic clean
1 PASS  docx_bad_content_types.docx.no_path_escape canary-intact
1 PASS  docx_bad_content_types.docx.no_subprocess clean
1 PASS  docx_bad_content_types.docx.opaque_exact yes
1 PASS  docx_deep_nesting.docx.no_hang     bounded
1 PASS  docx_deep_nesting.docx.no_network  clean
1 PASS  docx_deep_nesting.docx.no_panic    clean
1 PASS  docx_deep_nesting.docx.no_path_escape canary-intact
1 PASS  docx_deep_nesting.docx.no_subprocess clean
1 PASS  docx_deep_nesting.docx.opaque_exact yes
1 PASS  docx_doctype.docx.no_hang          bounded
1 PASS  docx_doctype.docx.no_network       clean
1 PASS  docx_doctype.docx.no_panic         clean
1 PASS  docx_doctype.docx.no_path_escape   canary-intact
1 PASS  docx_doctype.docx.no_subprocess    clean
1 PASS  docx_doctype.docx.opaque_exact     yes
1 PASS  docx_duplicate_part.docx.no_hang   bounded
1 PASS  docx_duplicate_part.docx.no_network clean
1 PASS  docx_duplicate_part.docx.no_panic  clean
1 PASS  docx_duplicate_part.docx.no_path_escape canary-intact
1 PASS  docx_duplicate_part.docx.no_subprocess clean
1 PASS  docx_duplicate_part.docx.opaque_exact yes
1 PASS  docx_entity_bomb.docx.no_hang      bounded
1 PASS  docx_entity_bomb.docx.no_network   clean
1 PASS  docx_entity_bomb.docx.no_panic     clean
1 PASS  docx_entity_bomb.docx.no_path_escape canary-intact
1 PASS  docx_entity_bomb.docx.no_subprocess clean
1 PASS  docx_entity_bomb.docx.opaque_exact yes
1 PASS  docx_external_main.docx.no_hang    bounded
1 PASS  docx_external_main.docx.no_network clean
1 PASS  docx_external_main.docx.no_panic   clean
1 PASS  docx_external_main.docx.no_path_escape canary-intact
1 PASS  docx_external_main.docx.no_subprocess clean
1 PASS  docx_external_main.docx.opaque_exact yes
1 PASS  docx_missing_main_rel.docx.no_hang bounded
1 PASS  docx_missing_main_rel.docx.no_network clean
1 PASS  docx_missing_main_rel.docx.no_panic clean
1 PASS  docx_missing_main_rel.docx.no_path_escape canary-intact
1 PASS  docx_missing_main_rel.docx.no_subprocess clean
1 PASS  docx_missing_main_rel.docx.opaque_exact yes
1 PASS  docx_non_utf8.docx.no_hang         bounded
1 PASS  docx_non_utf8.docx.no_network      clean
1 PASS  docx_non_utf8.docx.no_panic        clean
1 PASS  docx_non_utf8.docx.no_path_escape  canary-intact
1 PASS  docx_non_utf8.docx.no_subprocess   clean
1 PASS  docx_non_utf8.docx.opaque_exact    yes
1 PASS  docx_nul.docx.no_hang              bounded
1 PASS  docx_nul.docx.no_network           clean
1 PASS  docx_nul.docx.no_panic             clean
1 PASS  docx_nul.docx.no_path_escape       canary-intact
1 PASS  docx_nul.docx.no_subprocess        clean
1 PASS  docx_nul.docx.opaque_exact         yes
1 PASS  docx_ok.docx.no_hang               bounded
1 PASS  docx_ok.docx.no_network            clean
1 PASS  docx_ok.docx.no_panic              clean
1 PASS  docx_ok.docx.no_path_escape        canary-intact
1 PASS  docx_ok.docx.no_subprocess         clean
1 PASS  docx_ok.docx.opaque_exact          yes
1 PASS  docx_oversized_text.docx.no_hang   bounded
1 PASS  docx_oversized_text.docx.no_network clean
1 PASS  docx_oversized_text.docx.no_panic  clean
1 PASS  docx_oversized_text.docx.no_path_escape canary-intact
1 PASS  docx_oversized_text.docx.no_subprocess clean
1 PASS  docx_oversized_text.docx.opaque_exact yes
1 PASS  docx_traversal_part.docx.no_hang   bounded
1 PASS  docx_traversal_part.docx.no_network clean
1 PASS  docx_traversal_part.docx.no_panic  clean
1 PASS  docx_traversal_part.docx.no_path_escape canary-intact
1 PASS  docx_traversal_part.docx.no_subprocess clean
1 PASS  docx_traversal_part.docx.opaque_exact yes
1 PASS  epub_bad_container.epub.no_hang    bounded
1 PASS  epub_bad_container.epub.no_network clean
1 PASS  epub_bad_container.epub.no_panic   clean
1 PASS  epub_bad_container.epub.no_path_escape canary-intact
1 PASS  epub_bad_container.epub.no_subprocess clean
1 PASS  epub_bad_container.epub.opaque_exact yes
1 PASS  epub_bad_mimetype.epub.no_hang     bounded
1 PASS  epub_bad_mimetype.epub.no_network  clean
1 PASS  epub_bad_mimetype.epub.no_panic    clean
1 PASS  epub_bad_mimetype.epub.no_path_escape canary-intact
1 PASS  epub_bad_mimetype.epub.no_subprocess clean
1 PASS  epub_bad_mimetype.epub.opaque_exact yes
1 PASS  epub_bad_opf.epub.no_hang          bounded
1 PASS  epub_bad_opf.epub.no_network       clean
1 PASS  epub_bad_opf.epub.no_panic         clean
1 PASS  epub_bad_opf.epub.no_path_escape   canary-intact
1 PASS  epub_bad_opf.epub.no_subprocess    clean
1 PASS  epub_bad_opf.epub.opaque_exact     yes
1 PASS  epub_doctype_xhtml.epub.no_hang    bounded
1 PASS  epub_doctype_xhtml.epub.no_network clean
1 PASS  epub_doctype_xhtml.epub.no_panic   clean
1 PASS  epub_doctype_xhtml.epub.no_path_escape canary-intact
1 PASS  epub_doctype_xhtml.epub.no_subprocess clean
1 PASS  epub_doctype_xhtml.epub.opaque_exact yes
1 PASS  epub_encrypted.epub.no_hang        bounded
1 PASS  epub_encrypted.epub.no_network     clean
1 PASS  epub_encrypted.epub.no_panic       clean
1 PASS  epub_encrypted.epub.no_path_escape canary-intact
1 PASS  epub_encrypted.epub.no_subprocess  clean
1 PASS  epub_encrypted.epub.opaque_exact   yes
1 PASS  epub_missing_opf.epub.no_hang      bounded
1 PASS  epub_missing_opf.epub.no_network   clean
1 PASS  epub_missing_opf.epub.no_panic     clean
1 PASS  epub_missing_opf.epub.no_path_escape canary-intact
1 PASS  epub_missing_opf.epub.no_subprocess clean
1 PASS  epub_missing_opf.epub.opaque_exact yes
1 PASS  epub_ok.epub.no_hang               bounded
1 PASS  epub_ok.epub.no_network            clean
1 PASS  epub_ok.epub.no_panic              clean
1 PASS  epub_ok.epub.no_path_escape        canary-intact
1 PASS  epub_ok.epub.no_subprocess         clean
1 PASS  epub_ok.epub.opaque_exact          yes
1 PASS  epub_remote.epub.no_hang           bounded
1 PASS  epub_remote.epub.no_network        clean
1 PASS  epub_remote.epub.no_panic          clean
1 PASS  epub_remote.epub.no_path_escape    canary-intact
1 PASS  epub_remote.epub.no_subprocess     clean
1 PASS  epub_remote.epub.opaque_exact      yes
1 PASS  epub_scripted.epub.no_hang         bounded
1 PASS  epub_scripted.epub.no_network      clean
1 PASS  epub_scripted.epub.no_panic        clean
1 PASS  epub_scripted.epub.no_path_escape  canary-intact
1 PASS  epub_scripted.epub.no_subprocess   clean
1 PASS  epub_scripted.epub.opaque_exact    yes
1 PASS  epub_spine_inconsistent.epub.no_hang bounded
1 PASS  epub_spine_inconsistent.epub.no_network clean
1 PASS  epub_spine_inconsistent.epub.no_panic clean
1 PASS  epub_spine_inconsistent.epub.no_path_escape canary-intact
1 PASS  epub_spine_inconsistent.epub.no_subprocess clean
1 PASS  epub_spine_inconsistent.epub.opaque_exact yes
1 PASS  z_absolute_name.zip.no_hang        bounded
1 PASS  z_absolute_name.zip.no_network     clean
1 PASS  z_absolute_name.zip.no_panic       clean
1 PASS  z_absolute_name.zip.no_path_escape canary-intact
1 PASS  z_absolute_name.zip.no_subprocess  clean
1 PASS  z_absolute_name.zip.opaque_exact   yes
1 PASS  z_backslash_name.zip.no_hang       bounded
1 PASS  z_backslash_name.zip.no_network    clean
1 PASS  z_backslash_name.zip.no_panic      clean
1 PASS  z_backslash_name.zip.no_path_escape canary-intact
1 PASS  z_backslash_name.zip.no_subprocess clean
1 PASS  z_backslash_name.zip.opaque_exact  yes
1 PASS  z_bad_cd_offset.zip.no_hang        bounded
1 PASS  z_bad_cd_offset.zip.no_network     clean
1 PASS  z_bad_cd_offset.zip.no_panic       clean
1 PASS  z_bad_cd_offset.zip.no_path_escape canary-intact
1 PASS  z_bad_cd_offset.zip.no_subprocess  clean
1 PASS  z_bad_cd_offset.zip.opaque_exact   yes
1 PASS  z_bad_crc.zip.no_hang              bounded
1 PASS  z_bad_crc.zip.no_network           clean
1 PASS  z_bad_crc.zip.no_panic             clean
1 PASS  z_bad_crc.zip.no_path_escape       canary-intact
1 PASS  z_bad_crc.zip.no_subprocess        clean
1 PASS  z_bad_crc.zip.opaque_exact         yes
1 PASS  z_bad_descriptor.zip.no_hang       bounded
1 PASS  z_bad_descriptor.zip.no_network    clean
1 PASS  z_bad_descriptor.zip.no_panic      clean
1 PASS  z_bad_descriptor.zip.no_path_escape canary-intact
1 PASS  z_bad_descriptor.zip.no_subprocess clean
1 PASS  z_bad_descriptor.zip.opaque_exact  yes
1 PASS  z_bad_eocd_comment.zip.no_hang     bounded
1 PASS  z_bad_eocd_comment.zip.no_network  clean
1 PASS  z_bad_eocd_comment.zip.no_panic    clean
1 PASS  z_bad_eocd_comment.zip.no_path_escape canary-intact
1 PASS  z_bad_eocd_comment.zip.no_subprocess clean
1 PASS  z_bad_eocd_comment.zip.opaque_exact yes
1 PASS  z_drive_name.zip.no_hang           bounded
1 PASS  z_drive_name.zip.no_network        clean
1 PASS  z_drive_name.zip.no_panic          clean
1 PASS  z_drive_name.zip.no_path_escape    canary-intact
1 PASS  z_drive_name.zip.no_subprocess     clean
1 PASS  z_drive_name.zip.opaque_exact      yes
1 PASS  z_duplicate_names.zip.no_hang      bounded
1 PASS  z_duplicate_names.zip.no_network   clean
1 PASS  z_duplicate_names.zip.no_panic     clean
1 PASS  z_duplicate_names.zip.no_path_escape canary-intact
1 PASS  z_duplicate_names.zip.no_subprocess clean
1 PASS  z_duplicate_names.zip.opaque_exact yes
1 PASS  z_empty.zip.no_hang                bounded
1 PASS  z_empty.zip.no_network             clean
1 PASS  z_empty.zip.no_panic               clean
1 PASS  z_empty.zip.no_path_escape         canary-intact
1 PASS  z_empty.zip.no_subprocess          clean
1 PASS  z_empty.zip.opaque_exact           yes
1 PASS  z_encrypted.zip.no_hang            bounded
1 PASS  z_encrypted.zip.no_network         clean
1 PASS  z_encrypted.zip.no_panic           clean
1 PASS  z_encrypted.zip.no_path_escape     canary-intact
1 PASS  z_encrypted.zip.no_subprocess      clean
1 PASS  z_encrypted.zip.opaque_exact       yes
1 PASS  z_eps_stored.zip.no_hang           bounded
1 PASS  z_eps_stored.zip.no_network        clean
1 PASS  z_eps_stored.zip.no_panic          clean
1 PASS  z_eps_stored.zip.no_path_escape    canary-intact
1 PASS  z_eps_stored.zip.no_subprocess     clean
1 PASS  z_eps_stored.zip.opaque_exact      yes
1 PASS  z_huge_declared.zip.no_hang        bounded
1 PASS  z_huge_declared.zip.no_network     clean
1 PASS  z_huge_declared.zip.no_panic       clean
1 PASS  z_huge_declared.zip.no_path_escape canary-intact
1 PASS  z_huge_declared.zip.no_subprocess  clean
1 PASS  z_huge_declared.zip.opaque_exact   yes
1 PASS  z_multidisk.zip.no_hang            bounded
1 PASS  z_multidisk.zip.no_network         clean
1 PASS  z_multidisk.zip.no_panic           clean
1 PASS  z_multidisk.zip.no_path_escape     canary-intact
1 PASS  z_multidisk.zip.no_subprocess      clean
1 PASS  z_multidisk.zip.opaque_exact       yes
1 PASS  z_no_eocd.zip.no_hang              bounded
1 PASS  z_no_eocd.zip.no_network           clean
1 PASS  z_no_eocd.zip.no_panic             clean
1 PASS  z_no_eocd.zip.no_path_escape       canary-intact
1 PASS  z_no_eocd.zip.no_subprocess        clean
1 PASS  z_no_eocd.zip.opaque_exact         yes
1 PASS  z_nul_name.zip.no_hang             bounded
1 PASS  z_nul_name.zip.no_network          clean
1 PASS  z_nul_name.zip.no_panic            clean
1 PASS  z_nul_name.zip.no_path_escape      canary-intact
1 PASS  z_nul_name.zip.no_subprocess       clean
1 PASS  z_nul_name.zip.opaque_exact        yes
1 PASS  z_overlap.zip.no_hang              bounded
1 PASS  z_overlap.zip.no_network           clean
1 PASS  z_overlap.zip.no_panic             clean
1 PASS  z_overlap.zip.no_path_escape       canary-intact
1 PASS  z_overlap.zip.no_subprocess        clean
1 PASS  z_overlap.zip.opaque_exact         yes
1 PASS  z_prefix_stub.zip.no_hang          bounded
1 PASS  z_prefix_stub.zip.no_network       clean
1 PASS  z_prefix_stub.zip.no_panic         clean
1 PASS  z_prefix_stub.zip.no_path_escape   canary-intact
1 PASS  z_prefix_stub.zip.no_subprocess    clean
1 PASS  z_prefix_stub.zip.opaque_exact     yes
1 PASS  z_ratio_bomb.zip.no_hang           bounded
1 PASS  z_ratio_bomb.zip.no_network        clean
1 PASS  z_ratio_bomb.zip.no_panic          clean
1 PASS  z_ratio_bomb.zip.no_path_escape    canary-intact
1 PASS  z_ratio_bomb.zip.no_subprocess     clean
1 PASS  z_ratio_bomb.zip.opaque_exact      yes
1 PASS  z_traversal_name.zip.no_hang       bounded
1 PASS  z_traversal_name.zip.no_network    clean
1 PASS  z_traversal_name.zip.no_panic      clean
1 PASS  z_traversal_name.zip.no_path_escape canary-intact
1 PASS  z_traversal_name.zip.no_subprocess clean
1 PASS  z_traversal_name.zip.opaque_exact  yes
1 PASS  z_truncated.zip.no_hang            bounded
1 PASS  z_truncated.zip.no_network         clean
1 PASS  z_truncated.zip.no_panic           clean
1 PASS  z_truncated.zip.no_path_escape     canary-intact
1 PASS  z_truncated.zip.no_subprocess      clean
1 PASS  z_truncated.zip.opaque_exact       yes
1 PASS  z_unknown_method.zip.no_hang       bounded
1 PASS  z_unknown_method.zip.no_network    clean
1 PASS  z_unknown_method.zip.no_panic      clean
1 PASS  z_unknown_method.zip.no_path_escape canary-intact
1 PASS  z_unknown_method.zip.no_subprocess clean
1 PASS  z_unknown_method.zip.opaque_exact  yes
1 PASS  z_zip64_inconsistent.zip.no_hang   bounded
1 PASS  z_zip64_inconsistent.zip.no_network clean
1 PASS  z_zip64_inconsistent.zip.no_panic  clean
1 PASS  z_zip64_inconsistent.zip.no_path_escape canary-intact
1 PASS  z_zip64_inconsistent.zip.no_subprocess clean
1 PASS  z_zip64_inconsistent.zip.opaque_exact yes
```

## Failing assertions (if any)

_none_

The library half of this court is `tests/phase12_security.rs` (typed
rejection of the structural/list threats under `Limits::STRICT` and
`DEFAULT`, plus XML hardening); the fuzz half is the 18-target campaign.

---

## Environment (receipt)

- Commit under test: `33f6d044b1b1d5a74c6e561376b6a93170e08385` (branch `phase12`), dirty files: ` M fuzz/Cargo.lock; M fuzz/Cargo.toml; M fuzz/README.md; M tools/fuzz.sh;?? evidence/campaigns/2026-10-06-phase12-security-33f6d04/;?? fuzz/fuzz_targets/common_observe.rs;?? fuzz/fuzz_targets/docx_wml.rs;?? fuzz/fuzz_targets/epub_content.rs;?? fuzz/fuzz_targets/epub_package.rs;?? fuzz/fuzz_targets/opc_rels.rs;?? fuzz/fuzz_targets/xml_part.rs;?? fuzz/fuzz_targets/zip_decode.rs;?? fuzz/fuzz_targets/zip_scan.rs;?? fuzz/seeds/hostile.docx;?? fuzz/seeds/hostile.epub;?? fuzz/seeds/hostile_bad_eocd.zip;?? fuzz/seeds/hostile_doctype.docx;?? fuzz/seeds/hostile_scripted.epub;?? fuzz/seeds/hostile_truncated.zip;?? tests/fixtures/phase12-hostile/;?? tests/phase12_security.rs;?? tools/fixtures/phase12-hostile-gen.py;?? tools/phase12-security-court.sh;`
- Service: `vole-document/doc-baseline:1.99.0`; base `rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e`
- Toolchain: rustc `1.99.0`, cargo `1.99.0 (5f94df478 2026-08-27)`, arch `x86_64`
- `Cargo.lock` sha256: `710e2b53719f567587472f4deb99a361530aa247dfb48633442c4a0dd576de1e`
- Run (UTC): 2026-10-06T16:28:23Z
