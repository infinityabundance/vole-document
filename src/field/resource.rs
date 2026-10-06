//! Cross-document content-addressed resource sharing (Phase 12.8, ADR-0034).
//!
//! A *shareable resource* is a byte sequence embedded verbatim in more than one
//! document — an image, a font, or an embedded attachment. Its identity is
//! **content identity**: a [`ResourceBlob`](crate::field::node::NodeKind::ResourceBlob)
//! node holds the exact bytes inline, so its canonical encoding (hence its
//! [`NodeId`]) depends only on those bytes and is identical across every document
//! and every format that embeds them. There is no second keying scheme: the
//! existing content-addressed seed store physically shares one blob and one cache
//! entry for all occurrences.
//!
//! Two honesty rules govern this module:
//!
//! * **Only byte-identical units share.** Recognition is a *byte signature*
//!   (PNG/JPEG/GIF/WOFF/WOFF2/TTF/OTF), never a member name or a declared media
//!   type: a name is logical bytes and never authority (research B), and the
//!   plan forbids cross-format *semantic* dedup (ADR-0034 §3). A wrong
//!   classification can only cost a little space, never correctness, because
//!   sharing still requires exact byte equality.
//! * **A shared blob is not a compression claim.** It pays its own byte cost for
//!   the first document that carries it; a second document carrying the same bytes
//!   writes nothing. Reuse is scored as *work* (`nodes_reused`,
//!   `retained_inverse_work_fraction`), never as a byte fraction.

use crate::error::Result;
use crate::field::FieldStore;
use crate::field::node::{NodeKind, SeedNode};
use crate::store::NodeId;

/// Largest resource recorded as one content-addressed blob (1 MiB). A unit above
/// this bound keeps its exact source-span leaf and simply does not share; bounding
/// the blob keeps a single seed node within a sane framing envelope.
pub const MAX_SHARED_RESOURCE_BYTES: usize = 1 << 20;

/// Format-neutral provenance for a shared resource blob. It is part of the node's
/// canonical encoding, hence of its [`NodeId`]: it must be identical in every
/// adapter or the same bytes would hash differently and never share.
pub const RESOURCE_BLOB_PROVENANCE: &str = "res:blob";

/// The content-addressed node for a shareable resource: the exact bytes held
/// inline, no dependencies. Identical bytes always encode to identical canonical
/// bytes, hence to one [`NodeId`], across every format.
pub fn resource_blob_node(bytes: &[u8]) -> SeedNode {
    let mut node = SeedNode::new(
        NodeKind::ResourceBlob,
        bytes.len() as u64,
        bytes.to_vec(),
        Vec::new(),
        RESOURCE_BLOB_PROVENANCE,
    );
    node.limits.max_output_bytes = node.limits.max_output_bytes.max(bytes.len() as u64);
    node
}

/// Whether `bytes` carry a recognized binary-resource signature and are within
/// the sharing size bound. Purely a byte-level fact about the payload.
pub fn is_shareable_resource(bytes: &[u8]) -> bool {
    bytes.len() <= MAX_SHARED_RESOURCE_BYTES && starts_with_signature(bytes)
}

fn starts_with_signature(b: &[u8]) -> bool {
    const SIGNATURES: &[&[u8]] = &[
        b"\x89PNG\r\n\x1a\n", // PNG
        b"\xFF\xD8\xFF",      // JPEG (JFIF/EXIF)
        b"GIF87a",            // GIF 87a
        b"GIF89a",            // GIF 89a
        b"wOFF",              // WOFF
        b"wOF2",              // WOFF2
        b"\x00\x01\x00\x00",  // TrueType
        b"OTTO",              // OpenType/CFF
        b"ttcf",              // TrueType collection
    ];
    SIGNATURES.iter().any(|sig| b.starts_with(sig))
}

/// The outcome of registering one shared resource blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceRegistration {
    /// The resource's content id.
    pub id: NodeId,
    /// Bytes held by the blob.
    pub len: u64,
    /// Whether the id already existed in the store: a **representation** fact —
    /// `true` means the bytes were already present and nothing new was written.
    /// This is the `nodes_id_shared` witness; it is *not* a work-reuse witness.
    pub preexisting: bool,
}

/// Register a shared resource blob in `store`, returning its content id and
/// whether the id already existed. Idempotent: a repeat writes nothing.
pub fn register_resource_blob(
    store: &mut FieldStore,
    bytes: &[u8],
) -> Result<ResourceRegistration> {
    let node = resource_blob_node(bytes);
    let id = node.content_id();
    let preexisting = store.seeds().contains_node(&id)?;
    store.seeds_mut().put_node(&node.encode_canonical())?;
    Ok(ResourceRegistration {
        id,
        len: bytes.len() as u64,
        preexisting,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_are_recognized_and_bounded() {
        assert!(is_shareable_resource(b"\x89PNG\r\n\x1a\nrest"));
        assert!(is_shareable_resource(b"\xFF\xD8\xFF\xE0jpeg"));
        assert!(is_shareable_resource(b"GIF89a...."));
        assert!(is_shareable_resource(b"wOFF...."));
        // Not a resource: XML, plain text, and an empty slice.
        assert!(!is_shareable_resource(b"<?xml version=\"1.0\"?>"));
        assert!(!is_shareable_resource(b"plain text"));
        assert!(!is_shareable_resource(b""));
    }

    #[test]
    fn identical_bytes_produce_one_content_id() {
        let bytes = b"\x89PNG\r\n\x1a\nshared-image-bytes";
        let a = resource_blob_node(bytes);
        let b = resource_blob_node(bytes);
        assert_eq!(a.content_id(), b.content_id());
        let different = resource_blob_node(b"\x89PNG\r\n\x1a\nother-image-bytes");
        assert_ne!(a.content_id(), different.content_id());
    }

    #[test]
    fn oversized_units_do_not_share() {
        let mut big = vec![0u8; MAX_SHARED_RESOURCE_BYTES + 1];
        big[0] = 0x89;
        big[1] = b'P';
        assert!(!is_shareable_resource(&big));
    }
}
