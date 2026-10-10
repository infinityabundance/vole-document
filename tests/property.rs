//! Property / mutation-fuzzing court for the Phase-2 descriptor, model,
//! channel, and DRA parsers.
//!
//! Each test maps to a predeclared fuzzing property from the Phase-2 plan
//! (`docs/phases/phase-02-plan.md`, subphase 2.8):
//!
//! * `decode(encode(x)) == x` — [`roundtrip_property`].
//! * `parse(serialize(d)) == d` — [`parse_serialize_stability`].
//! * `reject(corrupt)` — [`random_bytes_never_panic`],
//!   [`mutated_valid_descriptors_are_safe`].
//! * `work <= limit` — [`oversized_claims_are_bounded`],
//!   [`limits_never_change_reconstructed_bytes`].
//!
//! Everything here is deterministic for a fixed seed. The iteration count is
//! read from `VOLE_FUZZ_ITERS` (default 2000) so `tools/soak-fuzz.sh` can raise
//! it without changing the code. Results derived from untrusted input are never
//! unwrapped: a malformed input must yield a typed `Err`, never a panic.

use vole_document::container::{Descriptor, ObjectSource, UNIVERSE};
use vole_document::dra::{Op, Program};
use vole_document::entropy::{
    CODER_ORDER0_BYTE_RANS, CODER_VERSION_1, EntropyChannelDescriptor, EntropyModel,
};
use vole_document::error::ErrorClass;
use vole_document::integrity::sha256;
use vole_document::limits::Limits;
use vole_document::materialize;
use vole_document::{SOURCE_FORMAT_OPAQUE, encode};

/// Deterministic, seedable PRNG so every court case is reproducible.
///
/// This is the same xorshift64 pattern used by `tests/exact.rs`.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        // Avoid the zero fixed point of xorshift.
        Rng(seed | 1)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        let mut v = Vec::with_capacity(n + 8);
        while v.len() < n {
            v.extend_from_slice(&self.next_u64().to_le_bytes());
        }
        v.truncate(n);
        v
    }
}

/// Default iteration count for the court; raised by `tools/soak-fuzz.sh`.
fn iters() -> usize {
    std::env::var("VOLE_FUZZ_ITERS")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(2000)
}

/// Input byte distributions exercised by the round-trip and mutation courts.
#[derive(Clone, Copy, Debug)]
enum Dist {
    /// Uniform pseudo-random bytes (high entropy).
    Uniform,
    /// Short runs of equal bytes (RLE-shaped).
    Runs,
    /// A heavily skewed stream (roughly 90% zero).
    Skewed,
    /// Low-entropy English-like text.
    Text,
}

/// Generate `len` bytes with the requested distribution.
fn gen_input(rng: &mut Rng, len: usize, dist: Dist) -> Vec<u8> {
    match dist {
        Dist::Uniform => rng.bytes(len),
        Dist::Runs => {
            let mut v = Vec::with_capacity(len);
            while v.len() < len {
                let b = rng.next_u64() as u8;
                let run = 1 + (rng.next_u64() % 64) as usize;
                let run = run.min(len - v.len());
                v.extend(std::iter::repeat_n(b, run));
            }
            v
        }
        Dist::Skewed => (0..len)
            .map(|_| {
                if rng.next_u64().is_multiple_of(10) {
                    rng.next_u64() as u8
                } else {
                    0
                }
            })
            .collect(),
        Dist::Text => {
            const WORDS: &[u8] = b"the quick brown fox jumps over the lazy dog. ";
            (0..len)
                .map(|_| WORDS[(rng.next_u64() as usize) % WORDS.len()])
                .collect()
        }
    }
}

/// A small, deterministic corpus of varied raw inputs.
fn corpus_inputs(rng: &mut Rng) -> Vec<Vec<u8>> {
    let mut long_run = vec![7u8; 4096];
    long_run.extend_from_slice(b"tail");
    vec![
        Vec::new(),
        vec![0u8; 1024],
        vec![0xAAu8; 777],
        b"The quick brown fox jumps over the lazy dog. ".repeat(40),
        rng.bytes(2048),
        (0u8..=255).collect(),
        long_run,
    ]
}

/// The shared untrusted-input court.
///
/// Parsing/mutation courts must never panic and must never report an
/// implementation bug for hostile input. When a decode succeeds, the
/// reconstructed bytes must obey the exactness invariants the descriptor
/// declared.
fn assert_untrusted_is_safe(bytes: &[u8], limits: Limits, label: &str) {
    let parsed = match Descriptor::parse(bytes, limits) {
        Ok(p) => p,
        Err(e) => {
            assert_ne!(
                e.class(),
                ErrorClass::InternalInvariant,
                "[{label}] parse of untrusted input reported an internal invariant"
            );
            return;
        }
    };

    match materialize::decode_to_bytes(bytes, limits) {
        Ok((out, parsed_again)) => {
            assert_eq!(
                out.len() as u64,
                parsed.descriptor.source_len,
                "[{label}] reconstructed length disagrees with the descriptor"
            );
            assert_eq!(
                sha256(&out),
                parsed.descriptor.source_sha256,
                "[{label}] reconstructed digest disagrees with the descriptor"
            );
            assert_eq!(
                parsed_again.descriptor.source_len,
                parsed.descriptor.source_len
            );
            assert_eq!(
                parsed_again.descriptor.source_sha256,
                parsed.descriptor.source_sha256
            );
        }
        Err(e) => assert_ne!(
            e.class(),
            ErrorClass::InternalInvariant,
            "[{label}] decode of untrusted input reported an internal invariant"
        ),
    }
}

/// Property `reject(corrupt)`: arbitrary bytes parse/decode to a typed `Ok`/`Err`
/// and never panic; a successful decode is exact.
#[test]
fn random_bytes_never_panic() {
    let mut rng = Rng::new(0x5EED_0001);
    for _ in 0..iters() {
        let len = (rng.next_u64() % 4097) as usize;
        let bytes = rng.bytes(len);
        assert_untrusted_is_safe(&bytes, Limits::STRICT, "random-bytes");
    }
}

/// Property `reject(corrupt)`: a valid container with a few random byte XORs is
/// parsed and decoded safely, never panicking, and any accepted result is exact.
#[test]
fn mutated_valid_descriptors_are_safe() {
    let mut corpus_rng = Rng::new(0x5EED_0002);
    let corpus: Vec<Vec<u8>> = corpus_inputs(&mut corpus_rng)
        .into_iter()
        .map(|input| {
            encode::encode(&input, Limits::DEFAULT)
                .expect("encode under DEFAULT is total over bounded inputs")
                .0
        })
        .collect();
    assert!(!corpus.is_empty(), "corpus must not be empty");

    let mut rng = Rng::new(0x5EED_0003);
    for _ in 0..iters() {
        let base = &corpus[(rng.next_u64() as usize) % corpus.len()];
        let mut mutated = base.clone();
        let flips = 1 + (rng.next_u64() % 8) as usize;
        for _ in 0..flips {
            let pos = (rng.next_u64() as usize) % mutated.len();
            mutated[pos] ^= 1u8 << (rng.next_u64() % 8);
        }
        assert_untrusted_is_safe(&mutated, Limits::STRICT, "mutated");
    }
}

/// Property `decode(encode(x)) == x` for varied distributions: encoding under
/// `DEFAULT` is total over bounded inputs, and the round trip is byte- and
/// digest-exact.
#[test]
fn roundtrip_property() {
    let mut rng = Rng::new(0x5EED_0004);
    for i in 0..iters() {
        let dist = match i % 4 {
            0 => Dist::Uniform,
            1 => Dist::Runs,
            2 => Dist::Skewed,
            _ => Dist::Text,
        };
        let len = (rng.next_u64() % 4097) as usize;
        let input = gen_input(&mut rng, len, dist);

        let (bytes, _) = encode::encode(&input, Limits::DEFAULT).unwrap_or_else(|e| {
            panic!("[{i}] encode under DEFAULT must be total for bounded input: {e}")
        });
        let (out, parsed) = materialize::decode_to_bytes(&bytes, Limits::DEFAULT)
            .unwrap_or_else(|e| panic!("[{i}] decode of freshly encoded bytes failed: {e}"));

        assert_eq!(out, input, "[{i}] {dist:?} round trip is not byte-exact");
        assert_eq!(sha256(&out), sha256(&input), "[{i}] digest mismatch");
        assert_eq!(out.len() as u64, parsed.descriptor.source_len);
    }
}

/// Property `parse(serialize(d)) == d`: re-serializing a parsed descriptor
/// reproduces the exact same container bytes.
#[test]
fn parse_serialize_stability() {
    let mut rng = Rng::new(0x5EED_0005);
    for i in 0..iters() {
        let dist = match i % 4 {
            0 => Dist::Uniform,
            1 => Dist::Runs,
            2 => Dist::Skewed,
            _ => Dist::Text,
        };
        let len = (rng.next_u64() % 2049) as usize;
        let input = gen_input(&mut rng, len, dist);

        let (bytes, _) = encode::encode(&input, Limits::DEFAULT)
            .unwrap_or_else(|e| panic!("[{i}] encode failed: {e}"));
        let parsed = Descriptor::parse(&bytes, Limits::DEFAULT)
            .unwrap_or_else(|e| panic!("[{i}] parse of freshly encoded bytes failed: {e}"));
        let (again, _) = parsed
            .descriptor
            .serialize()
            .unwrap_or_else(|e| panic!("[{i}] re-serialize failed: {e}"));

        assert_eq!(
            again, bytes,
            "[{i}] parse->serialize changed the container bytes"
        );
    }
}

/// A minimal well-formed descriptor carrying `source` as a single literal.
fn literal_descriptor(source: &[u8]) -> Descriptor {
    Descriptor {
        universe: UNIVERSE.to_string(),
        source_format: SOURCE_FORMAT_OPAQUE,
        format_basis: "opaque;property".to_string(),
        models: vec![],
        channels: vec![],
        objects: vec![ObjectSource::Inline(source.to_vec())],
        program: Program::new(vec![Op::EmitObject { object_id: 0 }]),
        observation_index: None,
        seek_directory: false,
        checkpoints: None,
        source_sha256: sha256(source),
        source_len: source.len() as u64,
    }
}

/// Assert a crafted descriptor is rejected with a typed bound error by both the
/// structural parser and the full decoder, before any work proportional to the
/// declared (hostile) value.
fn assert_bounded_rejection(d: &Descriptor, label: &str) {
    let (bytes, _) = d
        .serialize()
        .unwrap_or_else(|e| panic!("[{label}] crafted descriptor failed to serialize: {e}"));

    let parse_err = Descriptor::parse(&bytes, Limits::STRICT)
        .err()
        .unwrap_or_else(|| panic!("[{label}] oversized claim was accepted by parse"));
    let decode_err = materialize::decode_to_bytes(&bytes, Limits::STRICT)
        .err()
        .unwrap_or_else(|| panic!("[{label}] oversized claim was accepted by decode"));

    for (stage, e) in [("parse", parse_err), ("decode", decode_err)] {
        assert!(
            matches!(
                e.class(),
                ErrorClass::ResourceLimit
                    | ErrorClass::CoverageViolation
                    | ErrorClass::EntropyDecode
            ),
            "[{label}] {stage} rejected with unexpected class {:?}",
            e.class()
        );
    }
}

/// Property `work <= limit`: declared sizes far beyond `Limits::STRICT` are
/// rejected with a typed error before any allocation proportional to them. All
/// crafted values are held just past the limit so this test cannot OOM.
#[test]
fn oversized_claims_are_bounded() {
    let strict = Limits::STRICT;

    // (a) declared source_len beyond max_output_bytes.
    let mut d = literal_descriptor(b"tiny");
    d.source_len = strict.max_output_bytes + 1;
    assert_bounded_rejection(&d, "source-len-over-limit");

    // (b) REPEAT_LAST count beyond max_repeat_count.
    let mut d = literal_descriptor(b"x");
    let count = (strict.max_repeat_count as u32) + 1;
    d.program = Program::new(vec![
        Op::EmitObject { object_id: 0 },
        Op::RepeatLast { count },
    ]);
    d.source_len = 1 + u64::from(count);
    assert_bounded_rejection(&d, "repeat-count-over-limit");

    // (c) channel symbol_count beyond max_channel_symbols.
    let mut d = literal_descriptor(b"");
    d.models = vec![EntropyModel::uniform(8).expect("uniform model")];
    let symbols = strict.max_channel_symbols + 1;
    d.channels = vec![EntropyChannelDescriptor {
        coder: CODER_ORDER0_BYTE_RANS,
        coder_version: CODER_VERSION_1,
        scale_bits: 8,
        lane_count: 1,
        model_id: 0,
        symbol_count: symbols,
        decoded_length: symbols,
        initial_state: 1,
        payload: Vec::new(),
    }];
    d.program = Program::new(vec![Op::DecodeChannel { channel_id: 0 }]);
    d.source_len = symbols;
    assert_bounded_rejection(&d, "channel-symbols-over-limit");

    // (d) channel decoded_length beyond max_output_bytes.
    let mut d = literal_descriptor(b"");
    d.models = vec![EntropyModel::uniform(8).expect("uniform model")];
    let decoded = strict.max_output_bytes + 1;
    d.channels = vec![EntropyChannelDescriptor {
        coder: CODER_ORDER0_BYTE_RANS,
        coder_version: CODER_VERSION_1,
        scale_bits: 8,
        lane_count: 1,
        model_id: 0,
        symbol_count: 1,
        decoded_length: decoded,
        initial_state: 1,
        payload: Vec::new(),
    }];
    d.program = Program::new(vec![Op::DecodeChannel { channel_id: 0 }]);
    d.source_len = decoded;
    assert_bounded_rejection(&d, "channel-decoded-over-limit");
}

/// Property `work <= limit`: decoding the same bytes under `DEFAULT` and under a
/// scaled-up `Limits` yields identical reconstructed bytes. Limits gate
/// admission; they never change the materialized output.
#[test]
fn limits_never_change_reconstructed_bytes() {
    let scaled = Limits {
        max_input_bytes: 1 << 50,
        max_output_bytes: 1 << 50,
        max_replay_bytes: 1 << 50,
        max_record_len: u32::MAX,
        max_record_count: 1 << 24,
        max_object_count: 1 << 24,
        max_graph_ops: 1 << 24,
        max_repeat_count: 1 << 40,
        max_channel_symbols: 1 << 50,
        max_model_count: 1 << 20,
        max_channel_count: 1 << 20,
        max_entropy_model_bytes: 1 << 20,
        max_pdf_spans: 1 << 26,
        max_index_selectors: 1 << 26,
        max_directory_bytes: 1 << 26,
        max_checkpoint_bytes: 1 << 26,
        max_zip_members: 1 << 24,
        max_zip_member_compressed: 1 << 50,
        max_zip_member_uncompressed: 1 << 50,
        max_zip_aggregate_uncompressed: 1 << 52,
        max_zip_compression_ratio: 1 << 20,
        max_zip_name_bytes: 1 << 24,
        max_zip_extra_bytes: 1 << 24,
        max_zip_entry_comment_bytes: 1 << 24,
        max_zip_archive_comment_bytes: 1 << 24,
        max_zip_central_dir_bytes: 1 << 30,
        max_zip_prefix_bytes: 1 << 30,
        max_zip_trailing_bytes: 1 << 30,
        max_xml_depth: 1 << 12,
        max_xml_part_bytes: 1 << 50,
        max_xml_events: 1 << 30,
        max_xml_nodes: 1 << 30,
        max_xml_attrs_per_element: 1 << 20,
        max_xml_text_bytes: 1 << 50,
        max_xml_document_bytes: 1 << 50,
        max_opc_rels: 1 << 24,
        max_opc_rel_depth: 1 << 16,
        max_opc_content_types_overrides: 1 << 24,
        max_opc_part_name_bytes: 1 << 24,
        max_epub_rootfiles: 1 << 8,
        max_epub_manifest_items: 1 << 24,
        max_epub_spine_items: 1 << 24,
        max_epub_nav_depth: 1 << 12,
        max_epub_fallback_chain: 1 << 10,
        max_xhtml_nodes: 1 << 30,
        max_odt_manifest_entries: 1 << 24,
        max_odt_blocks: 1 << 24,
        max_odt_notes: 1 << 24,
        max_ods_sheets: 1 << 12,
        max_ods_cells: 1 << 30,
        max_ods_repeated_span: 1 << 24,
        max_ods_merges: 1 << 24,
        max_ods_named_expressions: 1 << 24,
        max_ods_styles: 1 << 24,
        max_ods_comments: 1 << 24,
        max_xlsx_sheets: 1 << 12,
        max_xlsx_cells: 1 << 30,
        max_xlsx_shared_strings: 1 << 24,
        max_xlsx_merges: 1 << 24,
        max_xlsx_hyperlinks: 1 << 24,
        max_xlsx_comments: 1 << 24,
        max_xlsx_tables: 1 << 24,
        max_xlsx_table_columns: 1 << 24,
        max_xlsx_defined_names: 1 << 24,
        max_xlsx_drawings: 1 << 24,
        max_xlsx_style_records: 1 << 24,
        max_xlsx_col: 1 << 24,
        max_xlsx_row: 1 << 24,
        max_pptx_slides: 1 << 12,
        max_pptx_shapes_per_slide: 1 << 30,
        max_pptx_text_runs: 1 << 30,
        max_pptx_group_depth: 1 << 10,
        max_pptx_media: 1 << 24,
        max_pptx_tables: 1 << 24,
        max_pptx_table_cells: 1 << 30,
        max_pptx_notes: 1 << 24,
        max_pptx_layouts: 1 << 24,
        max_pptx_masters: 1 << 24,
        max_odp_slides: 1 << 12,
        max_odp_shapes_per_slide: 1 << 30,
        max_odp_text_runs: 1 << 30,
        max_odp_group_depth: 1 << 10,
        max_odp_media: 1 << 24,
        max_odp_tables: 1 << 24,
        max_odp_table_cells: 1 << 30,
        max_odp_notes: 1 << 24,
        max_odp_masters: 1 << 24,
        max_json_depth: 1 << 12,
        max_json_nodes: 1 << 30,
        max_json_string_bytes: 1 << 50,
        max_json_document_bytes: 1 << 50,
        max_json5_depth: 1 << 12,
        max_json5_nodes: 1 << 30,
        max_json5_string_bytes: 1 << 50,
        max_json5_comments: 1 << 30,
        max_json5_document_bytes: 1 << 50,
        max_yaml_depth: 1 << 12,
        max_yaml_nodes: 1 << 30,
        max_yaml_scalars: 1 << 30,
        max_yaml_anchors: 1 << 24,
        max_yaml_documents: 1 << 16,
        max_yaml_string_bytes: 1 << 50,
        max_yaml_document_bytes: 1 << 50,
        max_csv_rows: 1 << 30,
        max_csv_cols: 1 << 24,
        max_csv_record_bytes: 1 << 30,
        max_csv_field_bytes: 1 << 30,
        max_csv_document_bytes: 1 << 50,
        max_csv_sampled_records_for_detection: 1 << 20,
        max_markdown_depth: 1 << 12,
        max_markdown_nodes: 1 << 30,
        max_markdown_blocks: 1 << 28,
        max_markdown_inline_spans: 1 << 30,
        max_markdown_code_bytes: 1 << 50,
        max_markdown_document_bytes: 1 << 50,
        max_html_depth: 1 << 12,
        max_html_nodes: 1 << 30,
        max_html_attrs: 1 << 30,
        max_html_text_bytes: 1 << 50,
        max_html_script_bytes: 1 << 50,
        max_html_document_bytes: 1 << 50,
        max_toml_depth: 1 << 12,
        max_toml_nodes: 1 << 30,
        max_toml_keys: 1 << 30,
        max_toml_string_bytes: 1 << 50,
        max_toml_document_bytes: 1 << 50,
        max_jsonl_records: 1 << 30,
        max_jsonl_line_bytes: 1 << 50,
        max_jsonl_nodes: 1 << 30,
        max_jsonl_document_bytes: 1 << 50,
        max_eml_depth: 1 << 12,
        max_eml_parts: 1 << 30,
        max_eml_headers: 1 << 30,
        max_eml_part_bytes: 1 << 50,
        max_eml_decoded_bytes: 1 << 50,
        max_eml_document_bytes: 1 << 50,
        max_parquet_row_groups: 1 << 30,
        max_parquet_columns: 1 << 30,
        max_parquet_pages_per_chunk: 1 << 30,
        max_parquet_values: 1 << 50,
        max_parquet_decompressed_bytes: 1 << 50,
        max_parquet_document_bytes: 1 << 50,
        max_parquet_footer_bytes: 1 << 50,
        max_arrow_messages: 1 << 30,
        max_arrow_columns: 1 << 30,
        max_arrow_batches: 1 << 30,
        max_arrow_rows: 1 << 50,
        max_arrow_buffers: 1 << 40,
        max_arrow_values: 1 << 50,
        max_arrow_decompressed_bytes: 1 << 50,
        max_arrow_document_bytes: 1 << 50,
        max_arrow_metadata_bytes: 1 << 50,
        max_cbor_depth: 1 << 12,
        max_cbor_nodes: 1 << 30,
        max_cbor_string_bytes: 1 << 50,
        max_cbor_document_bytes: 1 << 50,
        max_msgpack_depth: 1 << 12,
        max_msgpack_nodes: 1 << 30,
        max_msgpack_str_bytes: 1 << 50,
        max_msgpack_bin_bytes: 1 << 50,
        max_msgpack_ext_bytes: 1 << 50,
        max_msgpack_document_bytes: 1 << 50,
        max_config_lines: 1 << 30,
        max_config_nodes: 1 << 30,
        max_config_entries: 1 << 30,
        max_config_depth: 1 << 12,
        max_config_line_bytes: 1 << 50,
        max_config_key_bytes: 1 << 30,
        max_config_value_bytes: 1 << 50,
        max_config_document_bytes: 1 << 50,
        max_feed_entries: 1 << 30,
        max_feed_fields: 1 << 30,
        max_feed_field_bytes: 1 << 50,
        max_feed_text_bytes: 1 << 50,
        max_feed_document_bytes: 1 << 50,
    };

    let mut rng = Rng::new(0x5EED_0006);
    for input in corpus_inputs(&mut rng) {
        let (bytes, _) = encode::encode(&input, Limits::DEFAULT)
            .expect("encode under DEFAULT must be total over bounded inputs");
        let (default_out, _) = materialize::decode_to_bytes(&bytes, Limits::DEFAULT)
            .expect("decode under DEFAULT must succeed");
        let (scaled_out, _) = materialize::decode_to_bytes(&bytes, scaled)
            .expect("decode under scaled limits must succeed");

        assert_eq!(default_out, input, "DEFAULT decode is not exact");
        assert_eq!(scaled_out, input, "scaled decode is not exact");
        assert_eq!(
            default_out, scaled_out,
            "limits changed reconstructed bytes"
        );
    }
}
