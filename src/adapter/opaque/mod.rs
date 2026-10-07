//! The opaque exact adapter.
//!
//! This is the correctness floor for *every* file type: arbitrary bytes are
//! partitioned into raw objects and reconstructed by a literal program. It
//! establishes container framing, integrity, resource bounds, the materializer,
//! the cost court, and receipts before any format-specific complexity arrives.

use crate::SOURCE_FORMAT_OPAQUE;
use crate::container::{Descriptor, ObjectSource, UNIVERSE};
use crate::dra::{Op, Program};
use crate::error::Result;
use crate::integrity::sha256;
use crate::limits::Limits;

/// Provenance string recorded in the FORMAT record.
pub const FORMAT_BASIS: &str = "opaque;unconditional-exact-fallback";

/// Propose the minimal exact representation of `input`.
///
/// For RAW the cheapest program is a single literal object referenced once, so
/// that is what this adapter emits. The block-partition and RLE candidates of
/// later phases must *beat* this on complete cost to be admitted.
pub fn propose(input: &[u8], _limits: Limits) -> Result<Descriptor> {
    Ok(Descriptor {
        universe: UNIVERSE.to_string(),
        source_format: SOURCE_FORMAT_OPAQUE,
        format_basis: FORMAT_BASIS.to_string(),
        models: vec![],
        channels: vec![],
        objects: vec![ObjectSource::Inline(input.to_vec())],
        program: Program::new(vec![Op::EmitObject { object_id: 0 }]),
        observation_index: None,
        seek_directory: false,
        checkpoints: None,
        source_sha256: sha256(input),
        source_len: input.len() as u64,
    })
}
