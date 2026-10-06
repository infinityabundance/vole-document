//! Centralized resource bounds.
//!
//! Every decode and encode path takes a [`Limits`]. Untrusted descriptors must
//! be rejected *before* catastrophic work is performed, so all arithmetic on
//! declared lengths and offsets is checked against these bounds and uses
//! checked integer operations.

/// Hard upper bounds applied while parsing and materializing a descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum accepted source/descriptor input size.
    pub max_input_bytes: u64,
    /// Maximum reconstructed output size for a single materialization.
    pub max_output_bytes: u64,
    /// Admission cap on the output of a single `DEFLATE_REPLAY`.
    ///
    /// This is a **VOLE replay-profile policy limit**, not an RFC 1951 maximum.
    /// RFC 1951 permits arbitrarily many empty non-final stored blocks, so it
    /// gives no finite `f(decompressed_size)` bound on `compressed_size`; a
    /// bitstream that inflates to zero bytes may be arbitrarily large. VOLE
    /// therefore declines to replay a descriptor whose declared output exceeds
    /// this policy cap (see ADR-0016).
    pub max_replay_bytes: u64,
    /// Maximum length of a single record payload.
    pub max_record_len: u32,
    /// Maximum number of records in a container.
    pub max_record_count: u32,
    /// Maximum number of distinct byte objects (`OBJECT` records).
    pub max_object_count: u32,
    /// Maximum number of DRA instructions in a reconstruction graph.
    pub max_graph_ops: u32,
    /// Maximum repeat count for a single `REPEAT_LAST` instruction.
    pub max_repeat_count: u64,
    /// Maximum number of symbols in a single entropy channel.
    pub max_channel_symbols: u64,
    /// Maximum number of distinct entropy models (`MODEL` records).
    pub max_model_count: u32,
    /// Maximum number of entropy channels (`ENTROPY_CHANNEL` records).
    pub max_channel_count: u32,
    /// Maximum encoded size of a single entropy model payload.
    pub max_entropy_model_bytes: u32,
    /// Maximum number of lexical spans produced for a PDF input.
    pub max_pdf_spans: u32,
    /// Maximum number of selectors in a single `OBSERVATION_INDEX` record.
    ///
    /// Bounds the admissions of the optional partial-decode index (Phase 7.3)
    /// before allocation: an index whose selector table would exceed this is
    /// rejected at parse and declined by the index builder. It mirrors the
    /// object/graph scale so the table cannot dwarf the document it describes.
    pub max_index_selectors: u32,
    /// Maximum accepted size of an optional `DIRECTORY` record payload.
    ///
    /// Bounds the seek directory (Phase 8) before allocation: a directory larger
    /// than this is declined at decode rather than trusted. A directory is roughly
    /// `13 * record_count` bytes, so this also caps the record count a directory
    /// can describe.
    pub max_directory_bytes: u32,
}

impl Limits {
    /// The default archival limits: generous, but always finite.
    pub const DEFAULT: Limits = Limits {
        max_input_bytes: 1 << 40,  // 1 TiB
        max_output_bytes: 1 << 40, // 1 TiB
        max_replay_bytes: 1 << 34, // 16 GiB
        max_record_len: 1 << 31,   // 2 GiB
        max_record_count: 1 << 20, // ~1M records
        max_object_count: 1 << 20,
        max_graph_ops: 1 << 20,
        max_repeat_count: 1 << 32,
        max_channel_symbols: 1 << 40,
        max_model_count: 1 << 16,
        max_channel_count: 1 << 16,
        max_entropy_model_bytes: 4096,
        max_pdf_spans: 1 << 26,
        max_index_selectors: 1 << 20,
        max_directory_bytes: 1 << 20,
    };

    /// Tight limits for hostile-input testing and fuzzing.
    pub const STRICT: Limits = Limits {
        max_input_bytes: 1 << 26,  // 64 MiB
        max_output_bytes: 1 << 26, // 64 MiB
        max_replay_bytes: 1 << 26, // 64 MiB
        max_record_len: 1 << 24,   // 16 MiB
        max_record_count: 1 << 16, // 65536
        max_object_count: 1 << 16,
        max_graph_ops: 1 << 16,
        max_repeat_count: 1 << 24,
        max_channel_symbols: 1 << 26,
        max_model_count: 1 << 12,
        max_channel_count: 1 << 12,
        max_entropy_model_bytes: 4096,
        max_pdf_spans: 1 << 16,
        max_index_selectors: 1 << 16,
        max_directory_bytes: 1 << 18,
    };
}

impl Default for Limits {
    fn default() -> Self {
        Limits::DEFAULT
    }
}
