//! Conformance court: descriptor stability, canonical re-encoding, coverage
//! authority, and format-drift detection.

use vole_document::adapter::opaque::FORMAT_BASIS;
use vole_document::container::record::RECORD_OVERHEAD;
use vole_document::container::{Descriptor, UNIVERSE, universe_id_from_str};
use vole_document::dra::{Authority, Op, Program};
use vole_document::encode::candidates::CandidateKind;
use vole_document::integrity::sha256;
use vole_document::limits::Limits;
use vole_document::materialize;
use vole_document::{SOURCE_FORMAT_OPAQUE, encode};

fn raw_descriptor(source: &[u8]) -> Descriptor {
    Descriptor {
        universe: UNIVERSE.to_string(),
        source_format: SOURCE_FORMAT_OPAQUE,
        format_basis: FORMAT_BASIS.to_string(),
        models: vec![],
        channels: vec![],
        objects: vec![source.to_vec()],
        program: Program::new(vec![Op::EmitObject { object_id: 0 }]),
        observation_index: None,
        source_sha256: sha256(source),
        source_len: source.len() as u64,
    }
}

#[test]
fn canonical_reencoding_is_byte_stable() {
    let source = b"canonical descriptor stability";
    let (bytes, _) = encode::encode(source, Limits::DEFAULT).unwrap();
    let parsed = Descriptor::parse(&bytes, Limits::DEFAULT).unwrap();
    let (again, cost) = parsed.descriptor.serialize().unwrap();
    assert_eq!(
        again, bytes,
        "serialize(parse(x)) must equal x for canonical descriptors"
    );
    assert_eq!(cost.total(), bytes.len() as u64);
}

#[test]
fn parse_serialize_model_roundtrip() {
    let d = raw_descriptor(b"model roundtrip");
    let (bytes, _) = d.serialize().unwrap();
    let parsed = Descriptor::parse(&bytes, Limits::DEFAULT).unwrap();
    assert_eq!(parsed.descriptor, d);
}

#[test]
fn header_universe_id_matches_declaration() {
    let (bytes, _) = encode::encode(b"universe", Limits::DEFAULT).unwrap();
    let parsed = Descriptor::parse(&bytes, Limits::DEFAULT).unwrap();
    assert_eq!(parsed.universe_id, universe_id_from_str(UNIVERSE));
}

#[test]
fn fixed_overhead_formula_holds() {
    // Golden structural length, computed from the format definition. A change
    // here means the wire layout drifted and must be a deliberate, versioned
    // decision, not an accident.
    //
    // The RLE candidate would win for a short or repetitive source, so pick a
    // 16-byte non-repeating sequence where a literal object is strictly cheaper
    // and RAW is the court's winner. This keeps the RAW overhead formula under
    // test without weakening it.
    let source = b"0123456789abcdef";
    let (bytes, report) = encode::encode(source, Limits::DEFAULT).unwrap();
    assert_eq!(report.kind, CandidateKind::Raw);
    let graph_len = 1 + 4 + 1 + 4; // version + op_count + EMIT_OBJECT + id
    let expected = 64 // header
        + (RECORD_OVERHEAD + UNIVERSE.len())
        + (RECORD_OVERHEAD + 5 + FORMAT_BASIS.len())
        + (RECORD_OVERHEAD + source.len())
        + (RECORD_OVERHEAD + graph_len)
        + (RECORD_OVERHEAD + 40)
        + (RECORD_OVERHEAD + 20);
    assert_eq!(bytes.len(), expected);
    assert_eq!(report.cost.total(), expected as u64);
    assert_eq!(report.cost.header, 64);
    assert_eq!(report.cost.objects, source.len() as u64);
}

#[test]
fn coverage_authority_literal_then_generated() {
    let source = b"abcabcabc";
    let d = Descriptor {
        universe: UNIVERSE.to_string(),
        source_format: SOURCE_FORMAT_OPAQUE,
        format_basis: FORMAT_BASIS.to_string(),
        models: vec![],
        channels: vec![],
        objects: vec![b"abc".to_vec()],
        program: Program::new(vec![
            Op::EmitObject { object_id: 0 },
            Op::RepeatLast { count: 2 },
        ]),
        observation_index: None,
        source_sha256: sha256(source),
        source_len: source.len() as u64,
    };
    let (bytes, _) = d.serialize().unwrap();
    let parsed = Descriptor::parse(&bytes, Limits::DEFAULT).unwrap();

    let (len, cov) = parsed
        .descriptor
        .program
        .analyze_objects(&parsed.descriptor.objects, Limits::DEFAULT)
        .unwrap();
    assert_eq!(len, source.len() as u64);
    cov.validate(len).unwrap();
    assert_eq!(cov.spans[0].authority, Authority::Literal);
    assert_eq!(cov.spans[1].authority, Authority::Generated);

    let out = materialize::decode_to_bytes(&bytes, Limits::DEFAULT)
        .unwrap()
        .0;
    assert_eq!(out, source);
}

#[test]
fn inline_literal_program_is_exact() {
    let block = b"repeated-";
    let source: Vec<u8> = block.repeat(3); // INLINE once + REPEAT_LAST 2
    let program = Program::new(vec![
        Op::Inline {
            bytes: block.to_vec(),
        },
        Op::RepeatLast { count: 2 },
    ]);
    let d = Descriptor {
        universe: UNIVERSE.to_string(),
        source_format: SOURCE_FORMAT_OPAQUE,
        format_basis: FORMAT_BASIS.to_string(),
        models: vec![],
        channels: vec![],
        objects: vec![],
        program,
        observation_index: None,
        source_sha256: sha256(&source),
        source_len: source.len() as u64,
    };
    let (bytes, _) = d.serialize().unwrap();
    let out = materialize::decode_to_bytes(&bytes, Limits::DEFAULT)
        .unwrap()
        .0;
    assert_eq!(out, source);
}

#[test]
fn verify_report_is_accurate() {
    let source = b"verify me exactly";
    let (bytes, _) = encode::encode(source, Limits::DEFAULT).unwrap();
    let vr = materialize::verify(&bytes, Limits::DEFAULT).unwrap();
    assert_eq!(vr.source_len, source.len() as u64);
    assert_eq!(vr.object_count, 1);
    assert_eq!(vr.graph_ops, 1);
    assert_eq!(
        vr.sha256_hex,
        vole_document::integrity::to_hex(&sha256(source))
    );
}
