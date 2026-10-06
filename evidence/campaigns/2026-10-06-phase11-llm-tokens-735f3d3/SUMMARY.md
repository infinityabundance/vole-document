# Phase 11.13 — LLM working-set token court (pinned tokenizer)

Commit under test: `735f3d3fec165473429db1dadcb5b961db029694` (branch `phase11`); tree dirty: ``

Image: `vole-document/llm-workingset:1.99.0` (id `sha256:28aee8f6fe427b6b68659cb20ab08be4b2ef7fc629a66d5ad9a784196b6597bd`), base `rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e`.
Rust: `1.99.0 (b940084d7 2026-09-28)` / `1.99.0 (5f94df478 2026-08-27)`; Python `3.11.2`; Poppler `22.12.0`; `jq-1.6`.
Arch: `x86_64`; `Cargo.lock` sha256: `25fc018b20d57fe22b704fd528cfa9f3dc64969ed7aa0345fb0cd294e913eea2`.

## Tokenizer (pinned, offline)

`bert-base-uncased`, implementation `huggingface/tokenizers (Python bindings)` v`0.20.3`.

Asset: `tools/tokenizers/bert-base-uncased.tokenizer.json`
SHA-256 `ce64fce797c24f68df90b40a3f74f579b336a493db14bd583fd520ea0d8c9a98` — verified at court time: `true`.

```json
{
  "name": "bert-base-uncased",
  "implementation": "huggingface/tokenizers (Python bindings)",
  "implementation_version": "0.20.3",
  "asset_path": "tools/tokenizers/bert-base-uncased.tokenizer.json",
  "asset_sha256": "ce64fce797c24f68df90b40a3f74f579b336a493db14bd583fd520ea0d8c9a98",
  "asset_sha256_expected": "ce64fce797c24f68df90b40a3f74f579b336a493db14bd583fd520ea0d8c9a98",
  "asset_verified": true,
  "add_special_tokens": false,
  "config": {
    "tokenizer_json_version": "1.0",
    "model": "WordPiece",
    "unk_token": "[UNK]",
    "continuing_subword_prefix": "##",
    "max_input_chars_per_word": 100,
    "normalizer": {
      "type": "BertNormalizer",
      "clean_text": true,
      "handle_chinese_chars": true,
      "strip_accents": null,
      "lowercase": true
    },
    "pre_tokenizer": {
      "type": "BertPreTokenizer"
    },
    "post_processor": "TemplateProcessing",
    "decoder": "WordPiece",
    "vocab_size": 30522
  }
}
```

## Per-case working set (bytes and tokens)

| case | kind | B0 bytes | B1 bytes | V bytes | B0 tokens | B1 tokens | V tokens | V vs B1 | V vs B0 |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| large-50 | generated | 4485 | 91 | 527275 | 2722 | 54 | 340182 | loss | loss |
| large-400 | generated | 35868 | 91 | 65913 | 22142 | 54 | 41433 | loss | loss |
| libreoffice-export | producer | 122315 | 1933 | 62 | 27783 | 420 | 22 | win | win |
| cairo-vector | producer | 71634 | 11939 | 11936 | 15450 | 2575 | 2575 | tie | win |
| pdftex-doc | producer | 58824 | 9804 | 622 | 12744 | 2124 | 256 | win | win |
| reportlab-multipage | producer | 70842 | 11807 | 11804 | 15396 | 2566 | 2566 | tie | win |

## Verdict

V vs page-local Poppler B1 (tokens): **2 win / 2 tie / 2 loss**.
V vs whole-document B0 (tokens): **4 win / 0 tie / 2 loss**.

Any token reduction over B1 on the tested observations: `true`; on **all** tested observations: `false`.

## Honest losses and caveats

- **large-50**: V tokens 340182 vs B1 54 / B0 2722 — V_page_text_larger_than_B1_page_local, V_page_text_larger_than_B0_whole_document, V_tokens_exceed_B1_page_local, V_tokens_exceed_B0_whole_document
- **large-400**: V tokens 41433 vs B1 54 / B0 22142 — V_page_text_larger_than_B1_page_local, V_page_text_larger_than_B0_whole_document, V_tokens_exceed_B1_page_local, V_tokens_exceed_B0_whole_document

- Token counts are tokenizer-specific: `bert-base-uncased` (WordPiece, vocab 30522). They are **not** a claim about any other model's tokenizer.
- B0/B1 are Poppler reading-order extracts; V is VOLE's bounded heuristic text-run projection (`basis=heuristic`). A smaller V in bytes/tokens is a *working-set* measurement, never a text-quality claim.
- `add_special_tokens=false`: the count excludes the tokenizer's `[CLS]`/`[SEP]` (a constant +2 per candidate if included).
