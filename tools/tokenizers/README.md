# Vendored tokenizer asset (Phase 11.13 LLM working-set token court)

This directory holds the **pinned, offline** tokenizer identity used by
`tools/field-llm-workingset.sh` and `tools/llm-token-court.sh`. The court loads
this file directly; it never contacts the network.

## Asset

| field | value |
|---|---|
| file | `bert-base-uncased.tokenizer.json` |
| SHA-256 | `ce64fce797c24f68df90b40a3f74f579b336a493db14bd583fd520ea0d8c9a98` |
| size | 466,062 bytes |
| source | <https://huggingface.co/bert-base-uncased> |
| source revision | `86b5e0934494bd15c9632b12f734a8a67f723594` (git `main` at fetch time) |
| fetched | 2026-10-06, `git fetch --depth 1` with `GIT_LFS_SKIP_SMUDGE=1` (the file is a normal Git blob, not LFS) |
| license | Apache-2.0 (BERT, Devlin et al.; the `bert-base-uncased` Hub repository) |

`bert-base-uncased.tokenizer.json.sha256` holds the same digest in
`sha256sum -c` format; the court passes it to `tools/llm-tokenize.py` as
`--expect-sha256`, so a mutated asset fails the court instead of silently
changing the count.

## What the asset contains (read back at court time)

Read from the file by `tools/llm-tokenize.py` and reported verbatim in every
receipt:

* `version` = `1.0`
* model = **WordPiece** (`unk_token` = `[UNK]`, `continuing_subword_prefix` = `##`,
  `max_input_chars_per_word` = 100), vocab size **30522**
* normalizer = `BertNormalizer` (`clean_text` = true, `handle_chinese_chars` = true,
  `strip_accents` = null, `lowercase` = true)
* pre-tokenizer = `BertPreTokenizer`
* post-processor = `TemplateProcessing` adding `[CLS]`/`[SEP]` (excluded from the
  reported count via `add_special_tokens=False`)
* decoder = `WordPiece` (`prefix` = `##`)

## Why this tokenizer

The task explicitly permits a HuggingFace `tokenizer.json` such as
`bert-base-uncased`. It is a real, widely used tokenizer whose exact
implementation is the pinned HuggingFace `tokenizers` runtime
(`tokenizers==0.20.3`, hash-pinned wheel installed in the `llm-workingset`
Docker stage), so no tokenizer algorithm is hand-rolled here and correctness is
delegated to the reference implementation.

Token counts are **tokenizer-specific**. Every number this asset produces is
labelled `bert-base-uncased` (WordPiece, 30522); it is not a claim about any
other model's tokenizer, and a smaller VOLE answer in these units is a
working-set measurement, not a text-quality claim.
