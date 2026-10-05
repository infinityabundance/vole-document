# Campaign: 2026-10-05-phase2-f6af30b — Phase 2 — native rANS floor

## Method

Every corpus item was encoded twice: with the exact CORE binary (RAW + RLE,
no entropy dependency) and with the FULL binary (RAW + RLE + BYTE_RANS).
Each result was decoded by the same binary that produced it, byte-compared
with cmp, and verify-ed. Lengths are serialized .voldoc byte counts. The
"winner" is the candidate the complete-cost court selected in the FULL
binary, where model bytes are charged like any other bytes. The corpus is
deterministic except rand-urandom-64k.bin, which is recorded by hash.

## Results

| file | source_len | core_len | full_len | winner | byte_compare |
| --- | ---: | ---: | ---: | --- | --- |
| empty.bin | 0 | 272 | 272 | RLE | equal |
| one.bin | 1 | 278 | 278 | RLE | equal |
| zeros-64k.bin | 65536 | 283 | 283 | RLE | equal |
| runs.bin | 65536 | 3088 | 3088 | RLE | equal |
| pattern256.bin | 256 | 565 | 565 | RAW | equal |
| text-256k.bin | 262144 | 262453 | 143746 | BYTE_RANS | equal |
| skewed.bin | 65536 | 65845 | 11382 | BYTE_RANS | equal |
| rand-det-64k.bin | 65536 | 65845 | 65845 | RAW | equal |
| rand-urandom-64k.bin | 65536 | 65845 | 65845 | RAW | equal |

## Negative controls

empty.bin and one.bin (tiny) plus rand-det-64k.bin and rand-urandom-64k.bin
(high-entropy) must keep full_len == core_len and must not be won by
BYTE_RANS: order-0 rANS must lose to RAW/RLE once the serialized model cost
is charged. Each control must satisfy both conditions; otherwise the court
exits nonzero.

| file | core_len | full_len | winner | ok |
| --- | ---: | ---: | --- | --- |
| empty.bin | 272 | 272 | RLE | true |
| one.bin | 278 | 278 | RLE | true |
| rand-det-64k.bin | 65845 | 65845 | RAW | true |
| rand-urandom-64k.bin | 65845 | 65845 | RAW | true |

All negative controls passed: true.

## Cumulative ladder

sum_source = 590081
sum_core = 464474
sum_full = 291304
delta = sum_core - sum_full = 173170

## Attribution

Per-file delta (core_len - full_len) for files whose FULL winner is
BYTE_RANS:

{"name":"text-256k.bin","core_len":262453,"full_len":143746,"delta":118707}
{"name":"skewed.bin","core_len":65845,"full_len":11382,"delta":54463}

Attribution sum = 173170 (equals the ladder delta: 173170).

## Interpretation

Order-0 byte rANS wins only where the byte histogram is skewed enough to
repay the serialized model: here the skewed and English-like text items.
It loses to RAW/RLE on high-entropy and tiny items after the model cost is
charged, which is exactly what the negative controls require. The measured
ladder delta is attributable, file by file, to the two BYTE_RANS winners
listed above; no other item changed length between the core and full
binaries.

## Verdict

PASS
