#![cfg(feature = "rans")]
//! Phase 7.3-i2 court: partial-materialization views.
//!
//! Gate: for every selector, `materialize_observation` must return exactly the
//! slice `materialize(&parsed)[a..b]` of the full reconstruction, and it must
//! decline — never silently full-materialize — when the index or a section is
//! absent. Cost fields are checked only as *relative* facts (fewer ops touched,
//! fewer channels decoded), never as absolute thresholds.

use vole_document::adapter::pdf::{PdfPhysical, propose_pdf, sample_pdfs, scan};
use vole_document::container::Descriptor;
use vole_document::container::ParsedDescriptor;
use vole_document::container::observation::{
    DEP_NONE, ObservationIndex, ObservationSelector as IndexSelector, OpEntry, SECTION_OP_TABLE,
    SECTION_PDF_SELECTORS, SELECTOR_OBJECT, SELECTOR_STREAM,
};
use vole_document::dra::{Op, Program};
use vole_document::encode::candidates::CandidateKind;
use vole_document::integrity::sha256;
use vole_document::limits::Limits;
use vole_document::materialize::materialize;
use vole_document::materialize::observation::{ObservationSelector, materialize_observation};
use vole_document::{ErrorClass, SOURCE_FORMAT_OPAQUE};

const DEFAULT: Limits = Limits::DEFAULT;

/// Build a `PDF_PHYSICAL` descriptor for `source`, then attach a hand-built
/// observation index (op table from `analyze_ops`, one object selector and one
/// encoded-stream selector) and round-trip it through serialize/parse so the
/// index is validated exactly as a real decoder would see it.
fn pdf_observation_descriptor(source: &[u8]) -> (ParsedDescriptor, PdfPhysical) {
    let physical = scan(source, DEFAULT).unwrap();
    let cand = propose_pdf(source, DEFAULT)
        .unwrap()
        .expect("validated PDF must propose a candidate");
    assert_eq!(cand.kind, CandidateKind::PdfPhysical);
    let mut d = cand.descriptor;

    let object_lens: Vec<u64> = d.objects.iter().map(|o| o.len() as u64).collect();
    let channel_lens: Vec<u64> = d.channels.iter().map(|c| c.decoded_length).collect();
    let per_op = d
        .program
        .analyze_ops(&object_lens, &channel_lens, DEFAULT)
        .unwrap();
    let ops: Vec<OpEntry> = per_op
        .iter()
        .map(|&len| OpEntry {
            out_len: u32::try_from(len).unwrap(),
            dep_kind: DEP_NONE,
            dep_id: 0,
        })
        .collect();

    let obj = physical
        .objects
        .iter()
        .find(|o| o.number == 1)
        .expect("classic/bigtext PDFs have object 1");
    let mut selectors = vec![IndexSelector {
        kind: SELECTOR_OBJECT,
        number: obj.number as u32,
        generation: obj.generation as u32,
        out_off: obj.start,
        out_len: obj.end - obj.start,
    }];
    if let Some(stream) = physical.streams.first() {
        selectors.push(IndexSelector {
            kind: SELECTOR_STREAM,
            number: stream.object as u32,
            generation: stream.generation as u32,
            out_off: stream.data_start,
            out_len: stream.data_len,
        });
    }

    d.observation_index = Some(ObservationIndex {
        section_flags: SECTION_OP_TABLE | SECTION_PDF_SELECTORS,
        ops,
        selectors,
        digests: Vec::new(),
    });

    let (bytes, _cost) = d.serialize().unwrap();
    let parsed = Descriptor::parse(&bytes, DEFAULT).unwrap();
    (parsed, physical)
}

fn sample(name: &str) -> Vec<u8> {
    sample_pdfs()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, bytes)| bytes)
        .unwrap_or_else(|| panic!("missing sample {name}"))
}

/// A byte range strictly inside one physical span, at roughly the midpoint.
fn mid_span_range(source: &[u8], physical: &PdfPhysical) -> (u64, u64) {
    let half = source.len() as u64 / 2;
    let span = physical
        .spans
        .iter()
        .filter(|s| s.len >= 8 && s.start <= half)
        .max_by_key(|s| s.start)
        .expect("a mid-file span with room for an interior range");
    (span.start + 1, 4)
}

// ---------------------------------------------------------------------------
// (a) A mid-file byte range is exact and evaluates fewer ops.
// ---------------------------------------------------------------------------

#[test]
fn mid_file_range_is_exact_and_skips_ops() {
    for name in ["classic.pdf", "bigtext.pdf"] {
        let source = sample(name);
        let (parsed, physical) = pdf_observation_descriptor(&source);
        let full = materialize(&parsed, DEFAULT).unwrap();

        let (offset, len) = mid_span_range(&source, &physical);
        let report = materialize_observation(
            &parsed,
            ObservationSelector::ByteRange { offset, len },
            DEFAULT,
        )
        .unwrap_or_else(|e| panic!("[{name}] observation failed: {e}"));

        let a = offset as usize;
        assert_eq!(
            report.bytes,
            &full[a..a + len as usize],
            "[{name}] mid-file slice must equal the full materialization slice"
        );
        assert_eq!(report.stats.output_bytes, len);
        assert!(
            report.stats.ops_evaluated < report.stats.ops_total,
            "[{name}] linear skipping must evaluate fewer than {} ops, got {}",
            report.stats.ops_total,
            report.stats.ops_evaluated
        );
        assert!(
            report.stats.work_amplification() < 1.0,
            "[{name}] amplification"
        );
    }
}

// ---------------------------------------------------------------------------
// (b) ByteRange{0, source_len} equals the full materialization.
// ---------------------------------------------------------------------------

#[test]
fn whole_range_equals_full_materialization() {
    for name in ["classic.pdf", "bigtext.pdf"] {
        let source = sample(name);
        let (parsed, _) = pdf_observation_descriptor(&source);
        let full = materialize(&parsed, DEFAULT).unwrap();

        let report = materialize_observation(
            &parsed,
            ObservationSelector::ByteRange {
                offset: 0,
                len: source.len() as u64,
            },
            DEFAULT,
        )
        .unwrap();

        assert_eq!(report.bytes, full, "[{name}] whole range != full");
        assert_eq!(
            report.stats.ops_evaluated, report.stats.ops_total,
            "[{name}] whole range must evaluate every op"
        );
        assert_eq!(report.stats.work_amplification(), 1.0);
        assert_eq!(report.stats.output_bytes, source.len() as u64);
    }
}

// ---------------------------------------------------------------------------
// (c) PdfIndirectObject returns the object's exact bytes vs. physical::scan.
// ---------------------------------------------------------------------------

#[test]
fn pdf_indirect_object_matches_physical_scan() {
    for name in ["classic.pdf", "bigtext.pdf"] {
        let source = sample(name);
        let (parsed, _) = pdf_observation_descriptor(&source);
        let full = materialize(&parsed, DEFAULT).unwrap();

        // Re-scan independently and compare against the scanner's ground truth.
        let p = scan(&source, DEFAULT).unwrap();
        let obj = p.objects.iter().find(|o| o.number == 1).unwrap();

        let report = materialize_observation(
            &parsed,
            ObservationSelector::PdfIndirectObject {
                object: obj.number as u32,
                generation: obj.generation as u16,
            },
            DEFAULT,
        )
        .unwrap();

        let expected = &source[obj.start as usize..obj.end as usize];
        assert_eq!(report.bytes, expected, "[{name}] object bytes from scanner");
        assert_eq!(
            report.bytes,
            &full[obj.start as usize..obj.end as usize],
            "[{name}] object bytes vs. full materialization"
        );
    }
}

#[test]
fn pdf_encoded_stream_matches_physical_scan() {
    let source = sample("bigtext.pdf");
    let (parsed, physical) = pdf_observation_descriptor(&source);
    let stream = physical.streams.first().expect("bigtext.pdf has a stream");

    let report = materialize_observation(
        &parsed,
        ObservationSelector::PdfEncodedStream {
            object: stream.object as u32,
            generation: stream.generation as u16,
        },
        DEFAULT,
    )
    .unwrap();

    let start = stream.data_start as usize;
    let end = start + stream.data_len as usize;
    assert_eq!(report.bytes, &source[start..end]);
}

// ---------------------------------------------------------------------------
// (d) Missing index declines with a typed UnsupportedFeature.
// ---------------------------------------------------------------------------

#[test]
fn missing_index_is_declined() {
    let source = sample("classic.pdf");
    let d = propose_pdf(&source, DEFAULT).unwrap().unwrap().descriptor;
    assert!(
        d.observation_index.is_none(),
        "candidate must carry no index"
    );
    let (bytes, _) = d.serialize().unwrap();
    let parsed = Descriptor::parse(&bytes, DEFAULT).unwrap();

    let err = materialize_observation(
        &parsed,
        ObservationSelector::ByteRange { offset: 0, len: 1 },
        DEFAULT,
    )
    .unwrap_err();
    assert_eq!(err.class(), ErrorClass::UnsupportedFeature);
}

// ---------------------------------------------------------------------------
// (e) Out-of-range / absent selectors return typed errors.
// ---------------------------------------------------------------------------

#[test]
fn out_of_range_and_absent_selectors_are_typed_errors() {
    let source = sample("classic.pdf");
    let (parsed, _) = pdf_observation_descriptor(&source);
    let n = source.len() as u64;

    let cases: Vec<(ObservationSelector, ErrorClass)> = vec![
        (
            ObservationSelector::ByteRange { offset: 0, len: 0 },
            ErrorClass::Usage,
        ),
        (
            ObservationSelector::ByteRange { offset: n, len: 1 },
            ErrorClass::Usage,
        ),
        (
            ObservationSelector::ByteRange {
                offset: n - 1,
                len: 2,
            },
            ErrorClass::Usage,
        ),
        (
            ObservationSelector::PdfIndirectObject {
                object: 9999,
                generation: 0,
            },
            ErrorClass::UnsupportedFeature,
        ),
        (
            ObservationSelector::PdfEncodedStream {
                object: 9999,
                generation: 0,
            },
            ErrorClass::UnsupportedFeature,
        ),
        (
            ObservationSelector::PdfRevision { index: 0 },
            ErrorClass::UnsupportedFeature,
        ),
    ];

    for (selector, expected) in cases {
        let err = materialize_observation(&parsed, selector, DEFAULT).unwrap_err();
        assert_eq!(err.class(), expected, "selector {selector:?}");
    }
}

// ---------------------------------------------------------------------------
// (f) Empty / tiny / oversized requests never panic.
// ---------------------------------------------------------------------------

#[test]
fn tiny_and_oversized_requests_do_not_panic() {
    let source = sample("classic.pdf");
    let (parsed, _) = pdf_observation_descriptor(&source);
    let n = source.len() as u64;

    // Tiny but valid.
    let one = materialize_observation(
        &parsed,
        ObservationSelector::ByteRange {
            offset: n / 2,
            len: 1,
        },
        DEFAULT,
    )
    .unwrap();
    assert_eq!(one.bytes.len(), 1);

    // Oversized and empty are typed errors, not panics.
    assert!(
        materialize_observation(
            &parsed,
            ObservationSelector::ByteRange {
                offset: 0,
                len: n + 1
            },
            DEFAULT,
        )
        .is_err()
    );
    assert!(
        materialize_observation(
            &parsed,
            ObservationSelector::ByteRange { offset: 0, len: 0 },
            DEFAULT,
        )
        .is_err()
    );
}

#[test]
fn empty_source_requests_are_typed_errors() {
    let empty: &[u8] = b"";
    let descriptor = Descriptor {
        universe: vole_document::container::UNIVERSE.to_string(),
        source_format: SOURCE_FORMAT_OPAQUE,
        format_basis: "test;empty".to_string(),
        models: Vec::new(),
        channels: Vec::new(),
        objects: Vec::new(),
        program: Program::new(Vec::new()),
        observation_index: Some(ObservationIndex {
            section_flags: SECTION_OP_TABLE,
            ops: Vec::new(),
            selectors: Vec::new(),
            digests: Vec::new(),
        }),
        seek_directory: false,
        source_sha256: sha256(empty),
        source_len: 0,
    };
    let (bytes, _) = descriptor.serialize().unwrap();
    let parsed = Descriptor::parse(&bytes, DEFAULT).unwrap();

    // No panic for a zero-length request or an out-of-range one.
    assert_eq!(
        materialize_observation(
            &parsed,
            ObservationSelector::ByteRange { offset: 0, len: 0 },
            DEFAULT,
        )
        .unwrap_err()
        .class(),
        ErrorClass::Usage
    );
    assert_eq!(
        materialize_observation(
            &parsed,
            ObservationSelector::ByteRange { offset: 0, len: 1 },
            DEFAULT,
        )
        .unwrap_err()
        .class(),
        ErrorClass::Usage
    );
}

// ---------------------------------------------------------------------------
// Lazy entropy decoding: only the channels an evaluated op references.
// ---------------------------------------------------------------------------

/// A three-channel descriptor whose program decodes only channels 0 and 1;
/// channel 2 is present in the descriptor but unreferenced.
fn channel_descriptor() -> (ParsedDescriptor, Vec<u8>) {
    use vole_document::entropy::{
        CODER_ORDER0_BYTE_RANS, CODER_VERSION_1, EntropyChannelDescriptor, EntropyModel,
        encode_channel,
    };

    let a = b"A".repeat(40);
    let b = b"B".repeat(40);
    let c = b"C".repeat(40);
    let mut counts = [0u64; 256];
    for &x in a.iter().chain(b.iter()).chain(c.iter()) {
        counts[x as usize] += 1;
    }
    let model = EntropyModel::from_counts(&counts, 12).unwrap();
    let make = |data: &[u8]| {
        let capsule = encode_channel(&model, data).unwrap();
        EntropyChannelDescriptor {
            coder: CODER_ORDER0_BYTE_RANS,
            coder_version: CODER_VERSION_1,
            scale_bits: model.scale_bits,
            lane_count: 1,
            model_id: 0,
            symbol_count: capsule.symbol_count,
            decoded_length: capsule.decoded_length,
            initial_state: capsule.initial_state,
            payload: capsule.payload,
        }
    };
    let channels = vec![make(&a), make(&b), make(&c)];
    let source: Vec<u8> = [a.clone(), b.clone()].concat();
    let program = Program::new(vec![
        Op::DecodeChannel { channel_id: 0 },
        Op::DecodeChannel { channel_id: 1 },
    ]);
    let per_op = program.analyze_ops(&[], &[40, 40, 40], DEFAULT).unwrap();
    let ops: Vec<OpEntry> = per_op
        .iter()
        .map(|&len| OpEntry {
            out_len: u32::try_from(len).unwrap(),
            dep_kind: DEP_NONE,
            dep_id: 0,
        })
        .collect();

    let descriptor = Descriptor {
        universe: vole_document::container::UNIVERSE.to_string(),
        source_format: SOURCE_FORMAT_OPAQUE,
        format_basis: "test;channels".to_string(),
        models: vec![model],
        channels,
        objects: Vec::new(),
        program,
        observation_index: Some(ObservationIndex {
            section_flags: SECTION_OP_TABLE,
            ops,
            selectors: Vec::new(),
            digests: Vec::new(),
        }),
        seek_directory: false,
        source_sha256: sha256(&source),
        source_len: source.len() as u64,
    };
    let (bytes, _) = descriptor.serialize().unwrap();
    let parsed = Descriptor::parse(&bytes, DEFAULT).unwrap();
    (parsed, source)
}

#[test]
fn only_referenced_channels_are_decoded() {
    let (parsed, source) = channel_descriptor();
    assert_eq!(materialize(&parsed, DEFAULT).unwrap(), source);

    // A range inside channel 1 decodes only channel 1: channel 0 is skipped
    // (linear op skipping) and the unreferenced channel 2 is never decoded.
    let report = materialize_observation(
        &parsed,
        ObservationSelector::ByteRange {
            offset: 40,
            len: 40,
        },
        DEFAULT,
    )
    .unwrap();
    assert_eq!(report.bytes, source[40..80]);
    assert_eq!(report.stats.ops_evaluated, 1);
    assert_eq!(report.stats.ops_total, 2);
    assert_eq!(report.stats.channels_decoded, 1);
    assert_eq!(report.stats.channels_total, 3);
    assert!(report.stats.entropy_bytes_decoded > 0);

    // The whole range decodes exactly the two referenced channels.
    let whole = materialize_observation(
        &parsed,
        ObservationSelector::ByteRange {
            offset: 0,
            len: source.len() as u64,
        },
        DEFAULT,
    )
    .unwrap();
    assert_eq!(whole.bytes, source);
    assert_eq!(whole.stats.channels_decoded, 2);
    assert_eq!(whole.stats.ops_evaluated, 2);
}
