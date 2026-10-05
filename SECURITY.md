# Security policy and threat model

## Scope

A `.voldoc` descriptor can expand to a much larger document. The decoder is
therefore treated as an attack surface from the beginning, not as an appendix.

## The decoder never executes document content

Normative materialization emits **bytes**. It never executes or fetches:

- PDF JavaScript, actions, or embedded executables
- Office macros
- HTML/JavaScript or external resources
- external XML entities or network URLs
- TeX macros, shell escapes, or arbitrary plugins
- arbitrary code of any kind (no WASM, no scripting, no `eval`)

Applications downstream decide what to do with the emitted bytes.

## Hostile-input contract

Malformed input may produce a **typed error** and malformed `.voldoc` may be
**rejected**. Malformed input must not:

- panic
- hang or loop unbounded
- allocate without bound
- overflow (all length/offset arithmetic is checked)
- write outside the requested output
- invoke external processes or touch the network

## Resource bounds

All decode and encode paths take [`Limits`](../src/limits.rs). The descriptor
declares its expected output size; the implementation may impose stricter local
maxima. Candidates that would exceed a bound are rejected **before** catastrophic
work:

- `max_input_bytes`, `max_output_bytes`
- `max_record_len`, `max_record_count`
- `max_object_count`, `max_graph_ops`, `max_repeat_count`

The graph analyzer computes the predicted output length with checked arithmetic
before any allocation, and the coverage certificate is validated before
materialization.

## Framing and integrity

- CRC-32C guards the header and every record (corruption detection only).
- SHA-256 is the durable whole-source archival identity.
- The digest is not a security boundary against a malicious *author*; it is a
  correctness and archival boundary. Cryptographic authenticity (signing of
  descriptors) is a separate, future concern and is not claimed.

## Integers

All parsing of untrusted lengths/offsets uses checked arithmetic or bounded
comparisons. `reserved` fields must be zero. Unknown **mandatory** feature bits
and record tags fail closed; only explicitly-optional records are skipped.

## Dependencies

- The crate is `#![forbid(unsafe_code)]`.
- The only Phase-1 dependency is `sha2` (RustCrypto), pinned exactly.
- `ryg-rans-rs` (Phase 2) is optional and must be used through its **safe manual**
  decode API; the panicking convenience decoder is banned on the hostile path.
- `preflate-rs` (Phase 6) reconstruction can panic on hostile stored state; it
  must run in an isolated, bounded stage with a store-raw fallback.

"Written in Rust" is not a security claim. The parser's safety comes from its
bounds, checked arithmetic, fail-closed defaults, and hostile-input courts.

## Reporting

This is a research alpha. Report issues via the project issue tracker. Do not
include non-redistributable document bytes in a report; provide hashes and a
minimal reproducer.
