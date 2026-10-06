# ADR-0030 — A byte-authoritative ZIP physical layer (shared by DOCX/EPUB)

Status: accepted (Phase 12.0).
Extends ADR-0009 (PDF byte authority), ADR-0024 (layered authority). Relates:
ADR-0016 (bounded decode). Cites plan §DEC-3, §DEC-9, §12, §15; research B §1–§7,
E §1/§5/§7, I §1/§6/§10.

## Context

Two of the three Phase-12 formats are ZIP containers: DOCX is an OPC package and
EPUB is an OCF container. The obvious API — a logical `name -> entry` map, or the
`zip` crate — **collapses duplicate names**, normalizes metadata, hides raw
descriptors / prefix / extra-field order, and may reject weird-but-valid archives.
That cannot guarantee byte-exact reconstruction and lets a name map become decoder
authority. It repeats the PDF lesson (ADR-0009): the physical bytes are the
authority, oracles are not.

## Decision

One byte-authoritative ZIP **scanner** is shared by DOCX and EPUB, analogous to the
PDF physical scanner:

* A complete byte cover of `[0, N)`: `Prefix · LocalHeader · MemberData ·
  DataDescriptor(±sig) · CentralDirectory · ArchiveExtraData · Zip64Eocd ·
  Zip64EocdLocator · Eocd(comment) · Trailing · Unclassified`, with a `validate()`
  that rejects any gap / overlap / wrong total (`CoverageViolation`).
* `PhysicalMemberId = (archive ordinal, local-header offset)` is the identity,
  distinct from the advisory `LogicalPartName` (OPC/OCF lookup only). **Duplicate
  names are never normalized or collapsed**; a name lookup that is ambiguous is a
  typed decline.
* The exact leaf for a member is its **raw compressed span** (`SourceSlice` /
  `Literal` / `Concat`): no unzip/rezip, no bit-exact recompression required.
  DEFLATE is touched only to produce `Q_gen` observations, reusing
  `miniz_oxide` (raw `inflate`, not a second inflater).
* ZIP CRC is **CRC-32/ISO-HDLC via `crc32fast`**, never `crc32c` (Castagnoli).
* `zip` / `rawzip` are **oracle-only / dev-only** (differential offsets/sizes),
  never exactness authority.
* New caps in `src/limits.rs` (members, per-member and aggregate uncompressed,
  compression ratio, name/extra/comment/central-dir/prefix/trailing bytes) and
  `ErrorClass::InvalidZipStructure` (exit 18).
* **Reject vs opaque-fallback split** (plan §DEC-9): cover/identity broken
  (overlap, impossible offset, ZIP64 contradiction, malformed descriptor,
  multi-disk, traversal, EOCD ambiguity) → typed **reject**; semantics/resource
  only (bomb, unknown method, encryption, CRC fault) → preserve exact bytes and
  **decline decode** (`UnsupportedFeature`/`ResourceLimit`/`IntegrityMismatch`).
* No `PathBuf` is ever built from a member name; names are opaque logical bytes.

## Consequences

* `materialize(descriptor) == original_bytes` holds for arbitrary archives,
  including ones the `zip` oracle refuses; the exact closure is the span cover.
* DOCX and EPUB share this layer up to container interpretation; divergence happens
  only at the OPC/OCF XML layer (ADR-0032/0033).
* The "logical member map" becomes a **derived observation that can diverge** from
  the physical cover — the property the skeptic must try to falsify with duplicate
  names, CD/LFH disagreement, and prefix stubs.

**Rejected:** a name-keyed member map; the `zip`/`rawzip` crates as authority;
unzip/rezip or bit-exact DEFLATE recompression; `crc32c`; extraction to filesystem
paths; silently choosing one side of a local-vs-central disagreement.
