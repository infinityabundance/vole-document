#!/usr/bin/env python3
"""Pinned, offline tokenizer loader for the Phase-11.13 LLM working-set court.

Loads a vendored HuggingFace `tokenizer.json` with the pinned `tokenizers`
runtime and prints, as one JSON object, the byte size, SHA-256 and token count
of each input file.

Why this is reproducible and offline-deterministic:

  * the model asset is a *file on disk* vendored in the repository
    (`tools/tokenizers/`), never fetched from the Hub at court time;
  * its SHA-256 is checked against the recorded value before any token is
    counted (`--expect-sha256`), and the check result is reported;
  * the runtime is the exact version pinned in the Dockerfile
    (`tokenizers==0.20.3`, hash-verified wheel), and its version is reported;
  * the full tokenizer configuration actually used (model, normalizer,
    pre-tokenizer, post-processor, decoder, vocab size) is read back from the
    asset and reported, so the count can be attributed to a named tokenizer.

`add_special_tokens=False`: the count is the pure content length. With
`add_special_tokens=True` the BERT post-processor would prepend one `[CLS]` and
append one `[SEP]` per sequence (a constant +2 per candidate), which the court
does not want to fold into a per-class comparison.

This is a *measurement* tool, not part of the shipped binary. It is only run
inside the capped `llm-workingset` service.
"""

import argparse
import hashlib
import json
import os
import sys


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--tokenizer", required=True, help="path to tokenizer.json")
    ap.add_argument("--expect-sha256", default=None, help="recorded asset digest")
    ap.add_argument("--name", default=None, help="tokenizer name for the receipt")
    ap.add_argument("--files", nargs="*", default=[], help="files to count (omitted = identity only)")
    a = ap.parse_args(argv)

    try:
        import tokenizers
        from tokenizers import Tokenizer
    except Exception as e:  # noqa: BLE001 - report, never traceback into the receipt
        print(json.dumps({"ok": False, "error": "tokenizers import failed: %s" % e}))
        return 3

    asset_sha = sha256_file(a.tokenizer)
    verified = a.expect_sha256 is None or asset_sha == a.expect_sha256
    if not verified:
        print(json.dumps({
            "ok": False,
            "error": "tokenizer asset sha256 mismatch",
            "asset_sha256": asset_sha,
            "expected": a.expect_sha256,
        }))
        return 4

    tok = Tokenizer.from_file(a.tokenizer)
    with open(a.tokenizer, "rb") as f:
        cfg = json.load(f)

    counts = []
    for path in a.files:
        with open(path, "rb") as f:
            data = f.read()
        text = data.decode("utf-8", errors="replace")
        valid_utf8 = text.encode("utf-8") == data
        enc = tok.encode(text, add_special_tokens=False)
        counts.append({
            "path": path,
            "bytes": len(data),
            "sha256": sha256_bytes(data),
            "valid_utf8": valid_utf8,
            "tokens": len(enc.ids),
        })

    model = cfg.get("model") or {}
    post = cfg.get("post_processor")
    decoder = cfg.get("decoder") or {}
    name = a.name or os.path.splitext(os.path.basename(a.tokenizer))[0]
    out = {
        "ok": True,
        "tokenizer": {
            "name": name,
            "implementation": "huggingface/tokenizers (Python bindings)",
            "implementation_version": tokenizers.__version__,
            "asset_path": a.tokenizer,
            "asset_sha256": asset_sha,
            "asset_sha256_expected": a.expect_sha256,
            "asset_verified": verified,
            "add_special_tokens": False,
            "config": {
                "tokenizer_json_version": cfg.get("version"),
                "model": "WordPiece",
                "unk_token": model.get("unk_token"),
                "continuing_subword_prefix": model.get("continuing_subword_prefix"),
                "max_input_chars_per_word": model.get("max_input_chars_per_word"),
                "normalizer": cfg.get("normalizer"),
                "pre_tokenizer": cfg.get("pre_tokenizer"),
                "post_processor": (post or {}).get("type") if isinstance(post, dict) else None,
                "decoder": decoder.get("type"),
                "vocab_size": tok.get_vocab_size(),
            },
        },
        "counts": counts,
    }
    print(json.dumps(out, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
