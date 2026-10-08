# Phase 22.7 (P6) — document-agent economic court

**Question.** Across the whole document-agent workload, what is the cost per
correct, grounded task, and does VOLE cost **>=2x less** than a conventional
tuned baseline?

**Answer (measured).** GATE NOT MET

**Verdict.** VOLE loss

## Estimator and interval

Estimator: **ratio of summed costs** (`cost/corr+grnd` VOLE / baseline), pooled and by region.
The correctness, grounding and token counts are **deterministic** under the pinned
toolchain (same command, same tokenizer, same documents), so no sampling interval is
quoted; wall/RSS/storage are single-run and reported as the physical vector. The frozen
task set is small (9 tasks per region, 27 total); a region where VOLE has no
correct-grounded task is a **resolved loss**, not 'unresolved'.

## Frozen documents

| doc | fmt | source bytes | ref units | tasks | VOLE prep ms | VOLE store B | base prep ms | base store B |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| nist-pdf-0001 | pdf | 544047 | 3065 | 3 | 175 | 4936401 | 408 | 667426 |
| nist-pdf-0016 | pdf | 310627 | 296 | 3 | 39 | 1142985 | 45 | 63010 |
| nist-pdf-0004 | pdf | 1105061 | 3054 | 3 | 65 | 9880127 | 353 | 635545 |
| nist-docx-0001 | docx | 2286200 | 123 | 3 | 42 | 4819049 | 11 | 40982 |
| nist-docx-0005 | docx | 57045 | 243 | 3 | 22 | 570057 | 14 | 92823 |
| nist-docx-0013 | docx | 25400 | 13 | 3 | 22 | 165116 | 9 | 14811 |
| nist-epub-0001 | epub | 168431 | 1096 | 3 | 24 | 1232513 | 37 | 553318 |
| nist-epub-0003 | epub | 89755 | 810 | 3 | 27 | 795344 | 30 | 330454 |
| nist-epub-0008 | epub | 35841 | 78 | 3 | 43 | 142913 | 10 | 39739 |

## Tokenizer / input measure

`bert-base-uncased` (kind `pinned-tokenizer`). {"implementation": "huggingface/tokenizers", "implementation_version": "0.20.3", "asset_path": "tools/tokenizers/bert-base-uncased.tokenizer.json", "asset_sha256": "ce64fce797c24f68df90b40a3f74f579b336a493db14bd583fd520ea0d8c9a98", "asset_sha256_expected": "ce64fce797c24f68df90b40a3f74f579b336a493db14bd583fd520ea0d8c9a98", "asset_verified": true, "add_special_tokens": false, "vocab_size": 30522}

## Pooled

| backend | tasks | correct | grounded | corr+grnd | tokens | cost total | cost/corr+grnd |
|---|---:|---:|---:|---:|---:|---:|---:|
| vole | 27 | 16 | 16 | 16 | 7779 | 0.024016891 | 0.001501056 |
| baseline | 27 | 27 | 27 | 27 | 9472 | 0.028589111 | 0.001058856 |

Pooled cost ratio VOLE/baseline: **1.4176** (VOLE loss).

## Regions

| region | ratio | verdict |
|---|---:|---|
| docx | 1.1736 | VOLE loss |
| epub | 1.0242 | VOLE loss |
| pdf | inf | VOLE loss (no correct grounded task) |

## Frozen tasks (the same expectation for both backends)

Expected answer = the reference unit text (normalised); expected span = byte range in the reference text. Anchor is a token that occurs in exactly one reference unit.

| doc | fmt | unit | anchor | expected span | expected answer (first 80 chars) |
|---|---|---|---|---|---|
| nist-pdf-0001 | pdf | line | organization-developed | [204667,204759] |         security requirements and vulnerabilities in organization-developed appl |
| nist-pdf-0001 | pdf | line | vulnerability-related | [104290,104390] | penetration or vulnerability-related testing. For example, organizations can con |
| nist-pdf-0001 | pdf | line | methodology-updating | [159139,159237] | methodology-updating techniques as appropriate, and update their tool kits. For  |
| nist-pdf-0016 | pdf | line | cryptographic-based | [1767,1859] | cryptographic-based security systems to provide adequate information security fo |
| nist-pdf-0016 | pdf | line | responsibility | [1576,1671] | responsibility of every federal organization in providing adequate security in i |
| nist-pdf-0016 | pdf | line | Qualifications | [11149,11236] | 13. Qualifications. The security requirements specified in this standard are bas |
| nist-pdf-0004 | pdf | line | Infrastructure-as-a-Service | [36403,36501] |           Infrastructure-as-a-Service. Infrastructure-as-a-Service (IaaS) is a  |
| nist-pdf-0004 | pdf | line | provider-authenticated | [112908,112997] | resources from provider-authenticated entities and vice versa. Identity federati |
| nist-pdf-0004 | pdf | line | Software-as-a-Service | [34875,34970] |           Software-as-a-Service. Software-as-a-Service (SaaS) is a model of ser |
| nist-docx-0001 | docx | paragraph | commercial-off-the-shelf | [7760,7841] | Software type (e.g., commercial-off-the-shelf, open-source, internally developed |
| nist-docx-0001 | docx | paragraph | responsibilities | [2115,2436] | Identify authorizing officials, system owners, and other key roles with system r |
| nist-docx-0001 | docx | paragraph | interconnection | [9384,9532] | Type of agreement used for the information exchange (e.g., interconnection secur |
| nist-docx-0005 | docx | paragraph | interconnections | [32150,32281] | If the system does not have any direct interconnections, then this appendix may  |
| nist-docx-0005 | docx | paragraph | well-documented | [22521,22922] | It is important that all recovery events be well-documented, including actions t |
| nist-docx-0005 | docx | paragraph | interruptions | [6110,6264] | Identify the activities, resources, and procedures to carry out {system name} pr |
| nist-docx-0013 | docx | paragraph | DepartmentEVIDENCE | [0,65] | Anywhere Police DepartmentEVIDENCE CHAIN OF CUSTODY TRACKING FORM |
| nist-docx-0013 | docx | table | Description | [461,573] | Description of Evidence \| Item # \| Quantity \| Description of Item (Model, Serial |
| nist-docx-0013 | docx | paragraph | Seized | [381,460] | Date/Time Seized: __________________Location of Seizure: ______________________ |
| nist-epub-0001 | epub | block | vulnerability-related | [84440,85008] | Organizations can move beyond passive wireless scanning to conduct active scanni |
| nist-epub-0001 | epub | block | network-exploitable | [70895,71416] | For local vulnerability scanning, a scanner is installed on each host to be scan |
| nist-epub-0001 | epub | block | telecommunications | [213757,214109] | Skills needed to conduct remote access testing include TCP/IP and networking kno |
| nist-epub-0003 | epub | block | misconfigurations | [111529,112233] | Vulnerability scanners are automated tools that are used to identify vulnerabili |
| nist-epub-0003 | epub | block | Defense-in-Depth | [33251,33573] | Defense-in-Depth—Organizations should understand that a single security mechanis |
| nist-epub-0003 | epub | block | security-related | [45152,45227] | Monitoring system integrity, protection levels, and security-related events |
| nist-epub-0008 | epub | block | authoritative | [0,80] | For the authoritative PDF of this publication, visit csrc.nist.gov/publications. |
| nist-epub-0008 | epub | block | self-service | [5559,5773] | On-demand self-service. A consumer can unilaterally provision computing capabili |
| nist-epub-0008 | epub | block | responsible | [3241,3787] | NIST is responsible for developing standards and guidelines, including minimum r |

## Method and provenance

- Commit under test: `ee8723bdefb74670dd6f15fdae7a788b208bb81f` (branch `staging`); dirty: `?? evidence/campaigns/2026-10-08-phase22-7-agent-ee8723b/;?? tools/fixtures/phase22-7-agent.py;?? tools/phase22-7-agent-court.sh;`.
- Service: `llm-workingset`; base image `rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e`; `rustc 1.99.0 (b940084d7 2026-09-28)`; `cargo 1.99.0 (5f94df478 2026-08-27)`; `Python 3.11.2`; sqlite `3.40.1`; poppler `22.12.0`.
- Tokenizer: `tools/tokenizers/bert-base-uncased.tokenizer.json` sha256 `ce64fce797c24f68df90b40a3f74f579b336a493db14bd583fd520ea0d8c9a98`; runtime present: `yes` (`0.20.3`).

## What is REAL vs STAND-IN

- **Real:** the VOLE binary, `field-build`/`find`/`observe-batch`; the baseline Poppler/SQLite
  pipeline; the documents; the reference extraction; the correctness and grounding checks;
  build/query wall, storage bytes and peak RSS.
- **Stand-in:** there is **no LLM**. The "agent" is a deterministic script; the number of
  model-input tokens is the token count of the exact transcript the script feeds forward,
  under the pinned tokenizer (or the labelled proxy if the runtime is absent). Real-model
  behaviour (tool-choice, retries, correctness of a generated answer) is NOT measured.
- **Stated prices** for the monetary column are illustrative (see summary.json); they are not
  any vendor's list price. The physical cost vector is reported beside them.

## What this does NOT prove

- It is not a result from a real LLM agent; it does not measure generated-answer quality.
- Per-format generality is limited to pdf/docx/epub and the frozen sample; unresolved where
  the sample cannot support a claim.
- It does not reopen the falsified cross-document dedup / adaptive-promotion results.

