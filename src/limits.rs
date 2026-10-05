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
}

impl Limits {
    /// The default archival limits: generous, but always finite.
    pub const DEFAULT: Limits = Limits {
        max_input_bytes: 1 << 40,  // 1 TiB
        max_output_bytes: 1 << 40, // 1 TiB
        max_record_len: 1 << 31,   // 2 GiB
        max_record_count: 1 << 20, // ~1M records
        max_object_count: 1 << 20,
        max_graph_ops: 1 << 20,
        max_repeat_count: 1 << 32,
        max_channel_symbols: 1 << 40,
        max_model_count: 1 << 16,
        max_channel_count: 1 << 16,
        max_entropy_model_bytes: 4096,
    };

    /// Tight limits for hostile-input testing and fuzzing.
    pub const STRICT: Limits = Limits {
        max_input_bytes: 1 << 26,  // 64 MiB
        max_output_bytes: 1 << 26, // 64 MiB
        max_record_len: 1 << 24,   // 16 MiB
        max_record_count: 1 << 16, // 65536
        max_object_count: 1 << 16,
        max_graph_ops: 1 << 16,
        max_repeat_count: 1 << 24,
        max_channel_symbols: 1 << 26,
        max_model_count: 1 << 12,
        max_channel_count: 1 << 12,
        max_entropy_model_bytes: 4096,
    };
}

impl Default for Limits {
    fn default() -> Self {
        Limits::DEFAULT
    }
}
