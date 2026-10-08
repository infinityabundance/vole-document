//! Deterministic materialization.
//!
//! The materializer is deliberately boring: it evaluates a bounded program,
//! checks the reconstructed length, and checks the archival digest. It never
//! searches, guesses, optimizes, or invokes external tools.

pub mod observation;
pub mod seek;

use crate::container::{Descriptor, ObjectSource, ParsedDescriptor};
use crate::error::{Error, Result};
use crate::integrity::{sha256, to_hex};
use crate::limits::Limits;
use crate::store::{NullResolver, ObjectResolver};

/// Decode every entropy channel in table order, each against the model it
/// references. Each channel's model must already have been cross-validated by
/// `Descriptor::parse`.
#[cfg(feature = "rans")]
fn decode_channels(d: &Descriptor, limits: Limits) -> Result<Vec<Vec<u8>>> {
    use crate::entropy::rans::{Capsule, decode_channel};

    let mut channels: Vec<Vec<u8>> = Vec::with_capacity(d.channels.len());
    for channel in &d.channels {
        let model = d.models.get(channel.model_id as usize).ok_or_else(|| {
            Error::invalid_model(format!(
                "entropy channel references missing model {}",
                channel.model_id
            ))
        })?;
        let capsule = Capsule {
            initial_state: channel.initial_state,
            payload: channel.payload.clone(),
            symbol_count: channel.symbol_count,
            decoded_length: channel.decoded_length,
        };
        channels.push(decode_channel(model, &capsule, limits)?);
    }
    Ok(channels)
}

/// Without the `rans` feature there is no entropy decoder. A descriptor with no
/// channels is still exactly materializable (the RAW/RLE floor); one that
/// declares channels is refused as an explicit capability limit rather than
/// silently reinterpreted.
#[cfg(not(feature = "rans"))]
fn decode_channels(d: &Descriptor, _limits: Limits) -> Result<Vec<Vec<u8>>> {
    if d.channels.is_empty() {
        Ok(Vec::new())
    } else {
        Err(Error::unsupported_feature(
            "this build was compiled without the `rans` feature",
        ))
    }
}

/// Resolve every object once, verifying referenced ids and lengths, then hand
/// the DRA the same plain object vector the standalone path uses.
fn resolve_objects<R: ObjectResolver + ?Sized>(
    d: &Descriptor,
    resolver: &R,
) -> Result<Vec<Vec<u8>>> {
    let mut out: Vec<Vec<u8>> = Vec::with_capacity(d.objects.len());
    for (i, src) in d.objects.iter().enumerate() {
        match src {
            ObjectSource::Inline(bytes) => out.push(bytes.clone()),
            ObjectSource::External { id, len } => {
                let bytes = resolver.get(id, *len)?;
                if bytes.len() as u64 != *len {
                    return Err(Error::integrity_mismatch(format!(
                        "external object {i} ({id}) has {} bytes, EXTERNAL_REF declared {len}",
                        bytes.len()
                    )));
                }
                out.push(bytes);
            }
        }
    }
    Ok(out)
}

/// Resolve every object, **moving** inline payloads out of `d` instead of
/// cloning them.
///
/// Byte-for-byte equivalent to [`resolve_objects`] except that an
/// [`ObjectSource::Inline`] payload is taken (leaving an empty `Vec` behind)
/// rather than duplicated. The direct build's exactness proof holds the source,
/// the serialized authority, the parsed descriptor, and the reconstructed
/// output at once; cloning the source-sized inline object would add a fifth
/// resident copy at the peak. It is safe here because the caller owns `d` and
/// never reads its now-empty inline objects again.
fn take_objects<R: ObjectResolver + ?Sized>(
    d: &mut Descriptor,
    resolver: &R,
) -> Result<Vec<Vec<u8>>> {
    let mut out: Vec<Vec<u8>> = Vec::with_capacity(d.objects.len());
    for (i, src) in d.objects.iter_mut().enumerate() {
        match src {
            ObjectSource::Inline(bytes) => out.push(std::mem::take(bytes)),
            ObjectSource::External { id, len } => {
                let bytes = resolver.get(id, *len)?;
                if bytes.len() as u64 != *len {
                    return Err(Error::integrity_mismatch(format!(
                        "external object {i} ({id}) has {} bytes, EXTERNAL_REF declared {len}",
                        bytes.len()
                    )));
                }
                out.push(bytes);
            }
        }
    }
    Ok(out)
}

/// Materialize the exact source bytes for a parsed descriptor, **consuming** its
/// inline object payloads.
///
/// Byte-identical to [`materialize`] (same decoder, same length and SHA-256
/// checks); it differs only in that it moves the inline object bytes rather than
/// cloning them. A caller that owns the [`ParsedDescriptor`] and does not need
/// its objects again pays one fewer source-sized copy at the peak. Used by the
/// encoder court's decode-before-commit proof and by the direct-build ingest
/// verification, both of which hold the source and the authority simultaneously.
pub(crate) fn materialize_in_place(
    parsed: &mut ParsedDescriptor,
    limits: Limits,
) -> Result<Vec<u8>> {
    let d = &mut parsed.descriptor;
    let channels = decode_channels(d, limits)?;
    let objects = take_objects(d, &NullResolver)?;

    let out = d.program.eval(&objects, &channels, limits)?;
    if out.len() as u64 != d.source_len {
        return Err(Error::reconstruction_mismatch(format!(
            "materialized {} bytes but {} were declared",
            out.len(),
            d.source_len
        )));
    }
    let digest = sha256(&out);
    if digest != d.source_sha256 {
        return Err(Error::integrity_mismatch(format!(
            "materialized SHA-256 {} != declared {}",
            to_hex(&digest),
            to_hex(&d.source_sha256)
        )));
    }
    Ok(out)
}

/// Materialize the exact source bytes for a parsed descriptor, resolving any
/// external object references through `resolver`.
///
/// Enforces, in order: program bounds (via `eval`), reconstructed length, and
/// whole-source SHA-256. The DRA is unchanged: it consumes the same plain
/// object vector the standalone path uses.
pub fn materialize_with<R: ObjectResolver>(
    parsed: &ParsedDescriptor,
    resolver: &R,
    limits: Limits,
) -> Result<Vec<u8>> {
    let d = &parsed.descriptor;

    let channels = decode_channels(d, limits)?;
    let objects = resolve_objects(d, resolver)?;

    let out = d.program.eval(&objects, &channels, limits)?;
    if out.len() as u64 != d.source_len {
        return Err(Error::reconstruction_mismatch(format!(
            "materialized {} bytes but {} were declared",
            out.len(),
            d.source_len
        )));
    }
    let digest = sha256(&out);
    if digest != d.source_sha256 {
        return Err(Error::integrity_mismatch(format!(
            "materialized SHA-256 {} != declared {}",
            to_hex(&digest),
            to_hex(&d.source_sha256)
        )));
    }
    Ok(out)
}

/// Materialize the exact source bytes for a parsed descriptor.
///
/// Standalone entry point: no resolver. Succeeds iff there are no external
/// references; a descriptor with an [`ObjectSource::External`] object errors
/// [`crate::ErrorClass::MissingExternalObject`].
pub fn materialize(parsed: &ParsedDescriptor, limits: Limits) -> Result<Vec<u8>> {
    materialize_with(parsed, &NullResolver, limits)
}

/// Parse and materialize in one step, resolving external objects through
/// `resolver`.
pub fn decode_to_bytes_with<R: ObjectResolver>(
    bytes: &[u8],
    resolver: &R,
    limits: Limits,
) -> Result<(Vec<u8>, ParsedDescriptor)> {
    let parsed = Descriptor::parse(bytes, limits)?;
    let out = materialize_with(&parsed, resolver, limits)?;
    Ok((out, parsed))
}

/// Parse and materialize in one step (no resolver).
pub fn decode_to_bytes(bytes: &[u8], limits: Limits) -> Result<(Vec<u8>, ParsedDescriptor)> {
    decode_to_bytes_with(bytes, &NullResolver, limits)
}

/// The result of a deep verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyReport {
    /// Reconstructed source length.
    pub source_len: u64,
    /// Lower-case hex SHA-256 of the reconstructed source.
    pub sha256_hex: String,
    /// Number of raw byte objects in the descriptor.
    pub object_count: usize,
    /// Number of DRA instructions in the reconstruction program.
    pub graph_ops: usize,
}

/// Deep verification: parse, materialize, and check the archival digest.
pub fn verify(bytes: &[u8], limits: Limits) -> Result<VerifyReport> {
    let (out, parsed) = decode_to_bytes(bytes, limits)?;
    Ok(VerifyReport {
        source_len: out.len() as u64,
        sha256_hex: to_hex(&sha256(&out)),
        object_count: parsed.descriptor.objects.len(),
        graph_ops: parsed.descriptor.program.ops.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dra::Op;
    use crate::{EXACTNESS_PROFILE_EXACT_BYTES, SOURCE_FORMAT_OPAQUE};

    fn descriptor_for(source: &[u8]) -> Descriptor {
        Descriptor {
            universe: crate::container::UNIVERSE.to_string(),
            source_format: SOURCE_FORMAT_OPAQUE,
            format_basis: "opaque:test".to_string(),
            models: vec![],
            channels: vec![],
            objects: vec![crate::container::ObjectSource::Inline(source.to_vec())],
            program: crate::dra::Program::new(vec![Op::EmitObject { object_id: 0 }]),
            observation_index: None,
            seek_directory: false,
            checkpoints: None,
            source_sha256: sha256(source),
            source_len: source.len() as u64,
        }
    }

    #[test]
    fn exact_roundtrip() {
        let source = b"exact bytes must survive";
        let d = descriptor_for(source);
        let (bytes, _) = d.serialize().unwrap();
        let (out, _) = decode_to_bytes(&bytes, Limits::DEFAULT).unwrap();
        assert_eq!(out, source);
        let _ = EXACTNESS_PROFILE_EXACT_BYTES;
    }

    #[test]
    fn detects_digest_mismatch() {
        let source = b"abcdef";
        let mut d = descriptor_for(source);
        d.source_sha256[0] ^= 0xFF;
        let (bytes, _) = d.serialize().unwrap();
        let e = decode_to_bytes(&bytes, Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::IntegrityMismatch);
    }

    #[test]
    fn materialize_in_place_matches_and_empties_inline_objects() {
        let source = b"in-place must reconstruct identical bytes".repeat(64);
        let d = descriptor_for(&source);
        let (bytes, _) = d.serialize().unwrap();

        let mut parsed = Descriptor::parse(&bytes, Limits::DEFAULT).unwrap();
        let out = materialize_in_place(&mut parsed, Limits::DEFAULT).unwrap();
        assert_eq!(out, source, "in-place output must equal the source");
        // The inline payloads were moved out (the peak-saving property).
        assert!(
            parsed
                .descriptor
                .objects
                .iter()
                .all(|o| o.as_inline().is_some_and(|b| b.is_empty())),
            "in-place materialization must take, not clone, inline objects"
        );
        // Fail closed if reused: the descriptor can no longer name its object.
        assert!(materialize_in_place(&mut parsed, Limits::DEFAULT).is_err());
    }
}
