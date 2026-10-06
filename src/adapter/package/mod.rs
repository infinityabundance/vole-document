//! Package-container adapters (Phase 12).
//!
//! DOCX (an OPC package) and EPUB (an OCF container) are both ZIP archives. This
//! module hosts the **byte-authoritative ZIP physical layer** they share, gated
//! behind the non-default `package` feature. It is the ZIP analogue of the PDF
//! physical scanner: a complete byte cover of `[0, N)` with no gap and no
//! overlap, a conservative failure mode, and raw stored bytes (never a logical,
//! name-keyed member map) as the authority for exact reconstruction.
//!
//! No DOCX/EPUB semantics live here, and no decode is performed: a member's exact
//! leaf is its raw compressed span. Later subphases (12.2+) build package,
//! procedural, and observation layers on top of this cover.

pub mod zip;

pub use zip::{
    CentralEntry, DataDescriptor, DescriptorStyle, EocdRecord, NameHazard, PhysicalMemberId,
    Zip64Eocd, Zip64EocdLocator, Zip64Records, ZipMember, ZipPhysical, ZipSpan, ZipSpanKind,
    classify_name, crc32_iso_hdlc, require_safe_name, scan, verify_stored_crc,
};
