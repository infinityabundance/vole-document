//! Hostile-input court: malformed `.voldoc` must produce typed errors, never
//! panics, hangs, unbounded allocation, or silent reinterpretation.

use vole_document::container::record::write_record;
use vole_document::container::{Descriptor, HEADER_LEN, Header, Record, RecordReader};
use vole_document::error::ErrorClass;
use vole_document::limits::Limits;
use vole_document::materialize;
use vole_document::{Error, encode};

use vole_document::SOURCE_FORMAT_OPAQUE;
use vole_document::container::UNIVERSE;
use vole_document::dra::{Op, Program};
use vole_document::encode::candidates::{Candidate, CandidateKind};
use vole_document::integrity::sha256;

fn valid_container() -> Vec<u8> {
    let (bytes, _) = encode::encode(b"a small exact document payload", Limits::DEFAULT).unwrap();
    bytes
}

fn records_of(bytes: &[u8]) -> Vec<Record> {
    let mut r = RecordReader::new(bytes, HEADER_LEN, Limits::DEFAULT);
    let mut v = Vec::new();
    while let Some(rec) = r.next_record().unwrap() {
        v.push(rec);
    }
    v
}

fn rebuild(header: &[u8], recs: &[Record]) -> Vec<u8> {
    let mut out = header.to_vec();
    for r in recs {
        write_record(&mut out, r.tag, r.flags, &r.payload).unwrap();
    }
    out
}

fn set_trailer_count(recs: &mut [Record]) {
    let n = recs.len() as u32;
    if let Some(t) = recs.iter_mut().find(|r| r.tag == 0xFF) {
        t.payload[0..4].copy_from_slice(&n.to_le_bytes());
    }
}

fn insert_before_trailer(recs: &mut Vec<Record>, rec: Record) {
    let idx = recs.iter().position(|r| r.tag == 0xFF).unwrap();
    recs.insert(idx, rec);
    set_trailer_count(recs);
}

#[test]
fn empty_input_is_invalid_container() {
    let e = Descriptor::parse(&[], Limits::DEFAULT).unwrap_err();
    assert_eq!(e.class(), ErrorClass::InvalidContainer);
}

#[test]
fn bad_magic_is_rejected() {
    let mut bytes = valid_container();
    bytes[0] = b'Z';
    let e = Descriptor::parse(&bytes, Limits::DEFAULT).unwrap_err();
    assert_eq!(e.class(), ErrorClass::InvalidContainer);
}

#[test]
fn every_single_byte_flip_is_detected() {
    // Every byte of a valid container is covered by either the header CRC or a
    // record CRC, so any single-byte mutation must be rejected.
    let bytes = valid_container();
    for i in 0..bytes.len() {
        let mut m = bytes.clone();
        m[i] ^= 0x01;
        let r = materialize::decode_to_bytes(&m, Limits::DEFAULT);
        assert!(r.is_err(), "byte {i} flip was not detected");
        if let Err(e) = r {
            assert!(
                matches!(
                    e.class(),
                    ErrorClass::InvalidContainer
                        | ErrorClass::IntegrityMismatch
                        | ErrorClass::CoverageViolation
                        | ErrorClass::InvalidGraph
                        | ErrorClass::ResourceLimit
                ),
                "unexpected class for byte {i}: {:?}",
                e.class()
            );
        }
    }
}

#[test]
fn short_truncations_are_rejected() {
    let bytes = valid_container();
    for len in 0..bytes.len() {
        let e = Descriptor::parse(&bytes[..len], Limits::DEFAULT).unwrap_err();
        assert!(
            matches!(
                e.class(),
                ErrorClass::InvalidContainer | ErrorClass::CoverageViolation
            ),
            "len {len} gave {:?}",
            e.class()
        );
    }
}

#[test]
fn unknown_mandatory_record_fails_closed() {
    let bytes = valid_container();
    let header = &bytes[0..HEADER_LEN];
    let mut recs = records_of(&bytes);
    insert_before_trailer(
        &mut recs,
        Record {
            tag: 0x7F,
            flags: 0,
            payload: b"future!".to_vec(),
        },
    );
    let crafted = rebuild(header, &recs);
    let e = Descriptor::parse(&crafted, Limits::DEFAULT).unwrap_err();
    assert_eq!(e.class(), ErrorClass::UnsupportedFeature);
}

#[test]
fn unknown_optional_record_is_skipped() {
    let bytes = valid_container();
    let header = &bytes[0..HEADER_LEN];
    let original = Descriptor::parse(&bytes, Limits::DEFAULT)
        .unwrap()
        .descriptor;

    let mut recs = records_of(&bytes);
    insert_before_trailer(
        &mut recs,
        Record {
            tag: 0x7F,
            flags: 0x01,
            payload: b"skippable".to_vec(),
        },
    );
    let crafted = rebuild(header, &recs);

    let parsed = Descriptor::parse(&crafted, Limits::DEFAULT).unwrap();
    assert_eq!(parsed.descriptor, original);
    let out = materialize::decode_to_bytes(&crafted, Limits::DEFAULT)
        .unwrap()
        .0;
    assert_eq!(out, b"a small exact document payload");
}

#[test]
fn duplicate_universe_record_is_rejected() {
    let bytes = valid_container();
    let header = &bytes[0..HEADER_LEN];
    let mut recs = records_of(&bytes);
    let uni = recs.iter().find(|r| r.tag == 0x01).unwrap().clone();
    insert_before_trailer(&mut recs, uni);
    let crafted = rebuild(header, &recs);
    let e = Descriptor::parse(&crafted, Limits::DEFAULT).unwrap_err();
    assert_eq!(e.class(), ErrorClass::InvalidContainer);
}

#[test]
fn future_major_version_is_unsupported_not_misparsed() {
    let mut h = Header::new([0u8; 16], 0, 0, 0);
    h.major = 9;
    let e = Descriptor::parse(&h.encode(), Limits::DEFAULT).unwrap_err();
    assert_eq!(e.class(), ErrorClass::UnsupportedVersion);
}

#[test]
fn unknown_mandatory_feature_bit_fails_closed() {
    let mut h = Header::new([0u8; 16], 0, 0, 0);
    // A bit no build supports (bit 0, `FEATURE_DEFLATE_REPLAY`, is supported when
    // the `deflate-replay` feature is compiled in; the fail-closed behavior for a
    // *known* feature in a build *without* it is covered in `descriptor.rs`).
    h.mandatory_features = 0x8000_0000;
    let e = Descriptor::parse(&h.encode(), Limits::DEFAULT).unwrap_err();
    assert_eq!(e.class(), ErrorClass::UnsupportedFeature);
}

#[test]
fn wrong_exactness_profile_is_unsupported() {
    let mut h = Header::new([0u8; 16], 0, 0, 0);
    h.exactness_profile = 2;
    let e = Descriptor::parse(&h.encode(), Limits::DEFAULT).unwrap_err();
    assert_eq!(e.class(), ErrorClass::UnsupportedFeature);
}

#[test]
fn repeat_expansion_is_bounded() {
    // A small descriptor that would expand hugely is rejected by the output
    // limit before materializing.
    let descriptor = Descriptor {
        universe: UNIVERSE.to_string(),
        source_format: SOURCE_FORMAT_OPAQUE,
        format_basis: "opaque;test".to_string(),
        models: vec![],
        channels: vec![],
        objects: vec![vec![0u8; 1024]],
        program: Program::new(vec![
            Op::EmitObject { object_id: 0 },
            Op::RepeatLast { count: 1 << 20 },
        ]),
        source_sha256: sha256(&vec![0u8; 1024]),
        source_len: 1024,
    };
    let (bytes, _) = descriptor.serialize().unwrap();
    let limits = Limits {
        max_output_bytes: 1 << 16,
        ..Limits::DEFAULT
    };
    let e = materialize::decode_to_bytes(&bytes, limits).unwrap_err();
    assert_eq!(e.class(), ErrorClass::ResourceLimit);
}

#[test]
fn court_rejects_inexact_candidate() {
    // A candidate that cannot reproduce the source must fail the court rather
    // than being silently admitted.
    let input = b"the real source bytes";
    let bogus = Descriptor {
        universe: UNIVERSE.to_string(),
        source_format: SOURCE_FORMAT_OPAQUE,
        format_basis: "opaque;test".to_string(),
        models: vec![],
        channels: vec![],
        objects: vec![b"wrong bytes".to_vec()],
        program: Program::new(vec![Op::EmitObject { object_id: 0 }]),
        source_sha256: sha256(b"wrong bytes"),
        source_len: b"wrong bytes".len() as u64,
    };
    let cands = vec![Candidate {
        kind: CandidateKind::Raw,
        descriptor: bogus,
    }];
    let e: Error = vole_document::encode::court::run(input, cands, Limits::DEFAULT).unwrap_err();
    assert_eq!(e.class(), ErrorClass::ReconstructionMismatch);
}
