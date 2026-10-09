# ADR-0060 — permanent cross-field identity court

**Invariant.** The output of a field observation is a **pure function of the
field's content id (NodeId)** — independent of the store it lives in and of
what else the store contains. The court FAILS on any cross-field aliasing or
wrong-document answer.

**Verdict: `PASS`**

## Cases

| case | what it asserts |
|---|---|
| interleave | PDF + DOCX/XLSX/PPTX/ODS/ODP + JSON + YAML built & observed interleaved in ONE store; every observation equals the same document built alone, and field ids do not depend on store contents |
| programs | identical source via direct `field-build --profile runtime` vs searching `encode`/`field-ingest` sharing a store: identical observations, no aliasing of a second document |
| edit | `field-edit` one PDF page: the original field is unchanged and NO other field is affected |
| cache-clear | `cache --clear` then re-observe: the same answers |
| crash | SIGKILL a build mid-way, reopen the store: no wrong-document bytes served, post-crash rebuild exact |

## Checks

| check | result |
|---|---|
| cache_clear_materialize_exact | PASS |
| cache_clear_same_answers | PASS |
| crash_recovery_build_exact | PASS |
| crash_store_reopens_no_wrong_bytes | PASS |
| edit_new_field_shows_marker | PASS |
| edit_no_other_field_affected | PASS |
| edit_original_field_unchanged | PASS |
| interleave_field_ids_independent | PASS |
| interleave_observations_equal | PASS |
| programs_identical_observations | PASS |
| programs_materialize_exact_source | PASS |
| programs_no_aliasing_other_document | PASS |

## Notes

```json
{
  "alone_field": {
    "doc.docx": "169c5d8fc27619f4ef1f180dd65aca09b15a4738368892e4fc74a480563f5ea1",
    "doc.json": "c3488fdd279ec2701c5c8b3ec0da6fbe2b6ba4ca8e24162873af34a65000ad76",
    "doc.odp": "4f3d613d2fa60360ff5d3051b6653a447529515f511b5314ba07376ef7da88b6",
    "doc.ods": "d9e818e166026ab972dba7e19539761e430dd050ce8c3da1c88d371b560ca9d6",
    "doc.pdf": "7abe8609a9b5f20ac040ad2563abd1f9ba90f46f9707d65659dca51e1447bc90",
    "doc.pptx": "73851b2d556168a84763398f42932cacb9033869f3316c6bf3a4ba44ae34629f",
    "doc.xlsx": "fc4903ba5a320d79e9dd181e6659b2dbc3fc3594f730bb30156f2d496f70b77e",
    "doc.yaml": "9f46a8c35f4fb5107682a1bb63dffd0ea775539efc5cdb2de5c529f0fc9a9468"
  },
  "crash": {
    "killed_builds": 5,
    "large_source": "large.pdf",
    "note": "a partial field id is unknown and so is never fetched; every served answer after each SIGKILL equalled the pre-crash answer, and no read returned a wrong document"
  },
  "edit": {
    "edited_field": "93c03967577a12aa00f71c134bbb7e5eef83ad9558f908b47b13505e643c1f0d",
    "marker": "VOLE IDENTITY EDIT 21.5.3",
    "original_field": "7abe8609a9b5f20ac040ad2563abd1f9ba90f46f9707d65659dca51e1447bc90"
  },
  "inter_field": {
    "doc.docx": "169c5d8fc27619f4ef1f180dd65aca09b15a4738368892e4fc74a480563f5ea1",
    "doc.json": "c3488fdd279ec2701c5c8b3ec0da6fbe2b6ba4ca8e24162873af34a65000ad76",
    "doc.odp": "4f3d613d2fa60360ff5d3051b6653a447529515f511b5314ba07376ef7da88b6",
    "doc.ods": "d9e818e166026ab972dba7e19539761e430dd050ce8c3da1c88d371b560ca9d6",
    "doc.pdf": "7abe8609a9b5f20ac040ad2563abd1f9ba90f46f9707d65659dca51e1447bc90",
    "doc.pptx": "73851b2d556168a84763398f42932cacb9033869f3316c6bf3a4ba44ae34629f",
    "doc.xlsx": "fc4903ba5a320d79e9dd181e6659b2dbc3fc3594f730bb30156f2d496f70b77e",
    "doc.yaml": "9f46a8c35f4fb5107682a1bb63dffd0ea775539efc5cdb2de5c529f0fc9a9468"
  },
  "programs": {
    "direct_field": "7abe8609a9b5f20ac040ad2563abd1f9ba90f46f9707d65659dca51e1447bc90",
    "fields_coincide": false,
    "searching_field": "dcc92b5d5f0042ffbc8f40fc6a40f5667dc1523b6cddf14b9506f720e7a0b6de",
    "second_doc_field": "c3488fdd279ec2701c5c8b3ec0da6fbe2b6ba4ca8e24162873af34a65000ad76"
  }
}
```

## Scope (honest)

- This is a **fixed, self-authored** fixture set (one document per format),
  not a real-world population; it is a permanent regression court, not a
  coverage claim.
- `SIGKILL` timing is not asserted to land at a particular phase; the
  assertion is that after each kill every *served* answer still equals the
  pre-crash answer, and that a post-crash rebuild is exact. A partial field id
  is unknown and is therefore not fetched.
- Nothing here is run on the host; every command ran in the pinned
  `doc-baseline` container.
