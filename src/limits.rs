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
    /// Maximum accepted size of an optional `CHECKPOINT` record payload.
    ///
    /// Bounds the byte-level partial-materialization checkpoint (Phase 13.4)
    /// before allocation: a checkpoint larger than this is declined at decode
    /// rather than trusted. A checkpoint is `20 + 16 * op_count` bytes, so this
    /// also caps the op count a checkpoint can describe.
    pub max_checkpoint_bytes: u32,
    // The ZIP caps below mirror the threat model and DEFAULT/STRICT values frozen
    // in `research/subagents/phase-12/I-security.md` §6 (threat ids Z1–Z15), as
    // required by plan §DEC-3/§DEC-9.
    /// Maximum number of ZIP members accepted in one archive (Phase 12, Z2).
    pub max_zip_members: u32,
    /// Maximum declared compressed size of a single ZIP member (Phase 12, Z1).
    pub max_zip_member_compressed: u64,
    /// Maximum declared uncompressed size of a single ZIP member (Phase 12, Z1).
    pub max_zip_member_uncompressed: u64,
    /// Maximum sum of declared uncompressed sizes across all members (Z2).
    pub max_zip_aggregate_uncompressed: u64,
    /// Maximum declared uncompressed/compressed ratio for one member (Z1).
    pub max_zip_compression_ratio: u32,
    /// Maximum raw name byte length of one member (Phase 12, Z13/Z14).
    pub max_zip_name_bytes: u32,
    /// Maximum raw extra-field byte length of one member (Z7/Z14).
    pub max_zip_extra_bytes: u32,
    /// Maximum per-entry comment byte length (Z14).
    pub max_zip_entry_comment_bytes: u32,
    /// Maximum archive comment byte length (Z14/Z15).
    pub max_zip_archive_comment_bytes: u32,
    /// Maximum central-directory byte length (Z14).
    pub max_zip_central_dir_bytes: u64,
    /// Maximum leading bytes before the first local header (Z14).
    pub max_zip_prefix_bytes: u64,
    /// Maximum trailing bytes after the EOCD record (Z14).
    pub max_zip_trailing_bytes: u64,
    // The XML/OPC caps below mirror the threat model and DEFAULT/STRICT values
    // frozen in `research/subagents/phase-12/I-security.md` §6 (§2 XML, §3 OPC),
    // as required by plan §DEC-4/§DEC-9. XML is derived (`Q_gen`) state only.
    /// Maximum XML element nesting depth before a typed decline (Phase 12, §2).
    pub max_xml_depth: u32,
    /// Maximum decoded byte length of a single XML part (Phase 12, §2).
    pub max_xml_part_bytes: u64,
    /// Maximum number of XML pull events in a single part (Phase 12, §2).
    pub max_xml_events: u64,
    /// Maximum number of XML element nodes in a single part (Phase 12, §2).
    pub max_xml_nodes: u64,
    /// Maximum number of attributes on a single XML element (Phase 12, §2).
    pub max_xml_attrs_per_element: u32,
    /// Maximum total text bytes accepted across a single XML part (Phase 12, §2).
    pub max_xml_text_bytes: u64,
    /// Maximum relationships across all `.rels` parts (Phase 12, §3).
    pub max_opc_rels: u32,
    /// Maximum internal relationship traversal depth (Phase 12 cycles, §3).
    pub max_opc_rel_depth: u32,
    /// Maximum `Default`+`Override` entries in `[Content_Types].xml` (Phase 12, §3).
    pub max_opc_content_types_overrides: u32,
    /// Maximum byte length of an OPC part name (Phase 12, §3).
    pub max_opc_part_name_bytes: u32,
    // The EPUB/OCF caps below mirror the threat model and DEFAULT/STRICT values
    // frozen in `research/subagents/phase-12/I-security.md` §6 (§4 EPUB), as
    // required by plan §DEC-5/§DEC-9. EPUB semantics are derived (`Q_gen`) only.
    /// Maximum `rootfile` entries accepted in `META-INF/container.xml` (Phase 12, §4).
    pub max_epub_rootfiles: u32,
    /// Maximum Package Document manifest items accepted (Phase 12, §4).
    pub max_epub_manifest_items: u32,
    /// Maximum Package Document spine `itemref`s accepted (Phase 12, §4).
    pub max_epub_spine_items: u32,
    /// Maximum navigation-document nesting depth accepted (Phase 12, §4).
    pub max_epub_nav_depth: u32,
    /// Maximum manifest `fallback` chain length followed (Phase 12, §4).
    pub max_epub_fallback_chain: u32,
    /// Maximum XHTML element nodes accepted in one content/nav document (Phase 12, §4).
    pub max_xhtml_nodes: u32,
    // The ODT/ODF caps below mirror the EPUB caps above (Phase 13.3 applies the same
    // bounded-XML policy to the OpenDocument content model). ODT semantics are derived
    // (`Q_gen`) only.
    /// Maximum `file-entry` elements accepted in `META-INF/manifest.xml` (Phase 13.3).
    pub max_odt_manifest_entries: u32,
    /// Maximum block elements accepted in one OpenDocument content part (Phase 13.3).
    pub max_odt_blocks: u32,
    /// Maximum notes accepted in one OpenDocument content part (Phase 13.3).
    pub max_odt_notes: u32,
    // The XLSX/SpreadsheetML caps below bound the derived semantic model
    // (Phase 21.1.1). XLSX semantics are derived (`Q_gen`) only.
    /// Maximum `<sheet>` declarations accepted in one workbook (Phase 21.1.1).
    pub max_xlsx_sheets: u32,
    /// Maximum cells accepted in one worksheet part (Phase 21.1.1).
    pub max_xlsx_cells: u64,
    /// Maximum shared strings accepted in `xl/sharedStrings.xml` (Phase 21.1.1).
    pub max_xlsx_shared_strings: u32,
    /// Maximum merged ranges accepted in one worksheet (Phase 21.1.1).
    pub max_xlsx_merges: u32,
    /// Maximum hyperlinks accepted in one worksheet (Phase 21.1.2).
    pub max_xlsx_hyperlinks: u32,
    /// Maximum comments accepted in one comments part (Phase 21.1.2).
    pub max_xlsx_comments: u32,
    /// Maximum table parts accepted across a workbook (Phase 21.1.2).
    pub max_xlsx_tables: u32,
    /// Maximum columns accepted in one table part (Phase 21.1.2).
    pub max_xlsx_table_columns: u32,
    /// Maximum defined/named ranges accepted in one workbook (Phase 21.1.2).
    pub max_xlsx_defined_names: u32,
    /// Maximum drawing parts accepted across a workbook (Phase 21.1.2).
    pub max_xlsx_drawings: u32,
    /// Maximum style records (fonts/fills/`cellXfs`) accepted in `styles.xml` (Phase 21.1.2).
    pub max_xlsx_style_records: u32,
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
        max_checkpoint_bytes: 1 << 20,
        max_zip_members: 1 << 20,
        max_zip_member_compressed: 1 << 34,
        max_zip_member_uncompressed: 1 << 34,
        max_zip_aggregate_uncompressed: 1 << 36,
        max_zip_compression_ratio: 1024,
        max_zip_name_bytes: 1 << 16,
        max_zip_extra_bytes: 1 << 16,
        max_zip_entry_comment_bytes: 1 << 16,
        max_zip_archive_comment_bytes: 1 << 16,
        max_zip_central_dir_bytes: 1 << 28,
        max_zip_prefix_bytes: 1 << 20,
        max_zip_trailing_bytes: 1 << 20,
        max_xml_depth: 256,
        max_xml_part_bytes: 1 << 28,
        max_xml_events: 1 << 24,
        max_xml_nodes: 1 << 24,
        max_xml_attrs_per_element: 4096,
        max_xml_text_bytes: 1 << 28,
        max_opc_rels: 1 << 20,
        max_opc_rel_depth: 64,
        max_opc_content_types_overrides: 1 << 20,
        max_opc_part_name_bytes: 1 << 16,
        max_epub_rootfiles: 16,
        max_epub_manifest_items: 1 << 20,
        max_epub_spine_items: 1 << 20,
        max_epub_nav_depth: 64,
        max_epub_fallback_chain: 32,
        max_xhtml_nodes: 1 << 24,
        max_odt_manifest_entries: 1 << 20,
        max_odt_blocks: 1 << 20,
        max_odt_notes: 1 << 20,
        max_xlsx_sheets: 4096,
        max_xlsx_cells: 1 << 24,
        max_xlsx_shared_strings: 1 << 20,
        max_xlsx_merges: 1 << 20,
        max_xlsx_hyperlinks: 1 << 20,
        max_xlsx_comments: 1 << 20,
        max_xlsx_tables: 1 << 20,
        max_xlsx_table_columns: 1 << 16,
        max_xlsx_defined_names: 1 << 20,
        max_xlsx_drawings: 1 << 14,
        max_xlsx_style_records: 1 << 16,
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
        max_checkpoint_bytes: 1 << 18,
        max_zip_members: 1 << 16,
        max_zip_member_compressed: 1 << 26,
        max_zip_member_uncompressed: 1 << 26,
        max_zip_aggregate_uncompressed: 1 << 27,
        max_zip_compression_ratio: 256,
        max_zip_name_bytes: 4096,
        max_zip_extra_bytes: 4096,
        max_zip_entry_comment_bytes: 4096,
        max_zip_archive_comment_bytes: 4096,
        max_zip_central_dir_bytes: 1 << 20,
        max_zip_prefix_bytes: 1 << 16,
        max_zip_trailing_bytes: 1 << 16,
        max_xml_depth: 64,
        max_xml_part_bytes: 1 << 20,
        max_xml_events: 1 << 16,
        max_xml_nodes: 1 << 16,
        max_xml_attrs_per_element: 256,
        max_xml_text_bytes: 1 << 20,
        max_opc_rels: 1 << 14,
        max_opc_rel_depth: 16,
        max_opc_content_types_overrides: 1 << 12,
        max_opc_part_name_bytes: 4096,
        max_epub_rootfiles: 4,
        max_epub_manifest_items: 1 << 14,
        max_epub_spine_items: 1 << 14,
        max_epub_nav_depth: 16,
        max_epub_fallback_chain: 8,
        max_xhtml_nodes: 1 << 16,
        max_odt_manifest_entries: 1 << 14,
        max_odt_blocks: 1 << 14,
        max_odt_notes: 1 << 12,
        max_xlsx_sheets: 64,
        max_xlsx_cells: 1 << 16,
        max_xlsx_shared_strings: 1 << 14,
        max_xlsx_merges: 1 << 14,
        max_xlsx_hyperlinks: 1 << 14,
        max_xlsx_comments: 1 << 14,
        max_xlsx_tables: 1 << 14,
        max_xlsx_table_columns: 1 << 12,
        max_xlsx_defined_names: 1 << 14,
        max_xlsx_drawings: 1 << 12,
        max_xlsx_style_records: 1 << 12,
    };
}

impl Default for Limits {
    fn default() -> Self {
        Limits::DEFAULT
    }
}
