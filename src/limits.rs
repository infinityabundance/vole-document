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
    /// Maximum source length admitted for byte-based standalone XML detection
    /// (Phase 21.9). A bare XML source above this cap — whose adapter reads the
    /// whole source as one `DocumentExact` — falls back to
    /// [`crate::field::document_format::DocumentFormat::Opaque`].
    pub max_xml_document_bytes: u64,
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
    // The ODS/OpenDocument-Spreadsheet caps below bound the derived spreadsheet
    // model (Phase 21.3.1). ODS semantics are derived (`Q_gen`) only.
    /// Maximum `table:table` sheets accepted in one OpenDocument spreadsheet
    /// content part (Phase 21.3.1).
    pub max_ods_sheets: u32,
    /// Maximum *expanded* grid slots accepted across one spreadsheet, after
    /// `table:number-rows-repeated`/`table:number-columns-repeated` expansion
    /// (Phase 21.3.1). Each expanded row charges at least one slot even when it
    /// declares no cells, so an empty-row repeat bomb still declines. This is the
    /// ODS analogue of the XLSX coordinate bound (ADR-0059): a repeated span is
    /// bounded and the *expanded* count declines typed rather than allocating.
    pub max_ods_cells: u64,
    /// Maximum repeat count admitted for a single
    /// `table:number-columns-repeated`/`table:number-rows-repeated` attribute
    /// (Phase 21.3.1). A declaration above this bound is a typed resource-limit
    /// decline, never an allocation.
    pub max_ods_repeated_span: u32,
    /// Maximum merged spans (`table:number-columns-spanned`>1 or
    /// `table:number-rows-spanned`>1) accepted across one spreadsheet
    /// (Phase 21.3.1).
    pub max_ods_merges: u32,
    /// Maximum named expressions
    /// (`table:named-range`/`table:named-expression`) accepted across one
    /// spreadsheet (Phase 21.3.1).
    pub max_ods_named_expressions: u32,
    /// Maximum `style:style` cell-style records accepted in one spreadsheet's
    /// automatic styles or styles part (Phase 21.3.1).
    pub max_ods_styles: u32,
    /// Maximum `office:annotation` cell comments accepted across one spreadsheet
    /// (Phase 21.3.1).
    pub max_ods_comments: u32,
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
    /// Maximum 0-based column index accepted in a cell reference (Phase 21.1.2).
    ///
    /// The Excel-conformant grid is 16,384 columns wide (A..XFD), so a valid
    /// 0-based column is `< 16384`; a coordinate at or beyond this bound is a
    /// typed resource-limit decline. Bounding the coordinate here keeps a single
    /// hostile reference from driving an unbounded projection downstream.
    pub max_xlsx_col: u32,
    /// Maximum 0-based row index accepted in a cell/row reference (Phase 21.1.2).
    ///
    /// The Excel-conformant grid is 1,048,576 rows tall (1..1048576), so a valid
    /// 0-based row is `< 1 << 20`; a coordinate at or beyond this bound is a
    /// typed resource-limit decline.
    pub max_xlsx_row: u32,
    // The PPTX/PresentationML caps below bound the derived semantic model
    // (Phase 21.2.1). PPTX semantics are derived (`Q_gen`) only.
    /// Maximum slides accepted in one presentation (Phase 21.2.1).
    pub max_pptx_slides: u32,
    /// Maximum shapes accepted in one slide's shape tree, including group
    /// descendants (Phase 21.2.1).
    pub max_pptx_shapes_per_slide: u32,
    /// Maximum text runs (`a:t`) accepted in one slide (Phase 21.2.1).
    pub max_pptx_text_runs: u32,
    /// Maximum group-shape nesting depth accepted in one slide (Phase 21.2.1).
    pub max_pptx_group_depth: u32,
    /// Maximum media parts (images/audio/video) exposed by one presentation
    /// (Phase 21.2.1).
    pub max_pptx_media: u32,
    /// Maximum embedded tables accepted in one slide (Phase 21.2.1).
    pub max_pptx_tables: u32,
    /// Maximum table cells accepted across one slide's tables (Phase 21.2.1).
    pub max_pptx_table_cells: u32,
    /// Maximum notes-slide parts accepted in one presentation (Phase 21.2.1).
    pub max_pptx_notes: u32,
    /// Maximum slide-layout parts accepted in one presentation (Phase 21.2.1).
    pub max_pptx_layouts: u32,
    /// Maximum slide-master parts accepted in one presentation, and the bound
    /// applied to theme parts (Phase 21.2.1).
    pub max_pptx_masters: u32,
    // The ODP/OpenDocument-Presentation caps below bound the derived presentation
    // model (Phase 21.4.1). ODP semantics are derived (`Q_gen`) only.
    /// Maximum `draw:page` slides accepted in one OpenDocument presentation
    /// content part (Phase 21.4.1).
    pub max_odp_slides: u32,
    /// Maximum shapes accepted in one slide, including group descendants
    /// (Phase 21.4.1).
    pub max_odp_shapes_per_slide: u32,
    /// Maximum text runs (`text:span`) accepted in one slide (Phase 21.4.1).
    pub max_odp_text_runs: u32,
    /// Maximum `draw:g` group nesting depth accepted in one slide (Phase 21.4.1).
    pub max_odp_group_depth: u32,
    /// Maximum `Pictures/*` media parts exposed by one presentation (Phase 21.4.1).
    pub max_odp_media: u32,
    /// Maximum embedded tables (`table:table`) accepted in one slide
    /// (Phase 21.4.1).
    pub max_odp_tables: u32,
    /// Maximum table cells accepted across one slide's tables, after
    /// `table:number-columns-repeated`/`table:number-rows-repeated` expansion
    /// (Phase 21.4.1). An over-large repeat declines typed rather than allocating.
    pub max_odp_table_cells: u32,
    /// Maximum notes pages (`presentation:notes`) accepted in one presentation
    /// (Phase 21.4.1).
    pub max_odp_notes: u32,
    /// Maximum `style:master-page` master pages accepted, and the bound applied to
    /// `style:style` records, in one presentation (Phase 21.4.1).
    pub max_odp_masters: u32,
    // The JSON caps below bound the derived, span-preserving structured-tree model
    // (Phase 21.5.1). JSON is not a package: the whole source parses as exactly one
    // JSON value beneath these caps, and everything derived is `Q_gen` only.
    /// Maximum JSON container nesting depth accepted (objects/arrays). A deeper
    /// document is not detected as JSON (and any direct parse declines typed)
    /// rather than risking unbounded recursion (Phase 21.5.1).
    pub max_json_depth: u32,
    /// Maximum JSON nodes (values plus object member keys) accepted in one
    /// document. An over-large document declines typed rather than allocating
    /// (Phase 21.5.1).
    pub max_json_nodes: u32,
    /// Maximum total raw string-token bytes accepted across one JSON document
    /// (the bytes between the quotes, escapes included). A conservative upper
    /// bound on the decoded text (Phase 21.5.1).
    pub max_json_string_bytes: u64,
    /// Maximum source length admitted for byte-based JSON detection. Larger inputs
    /// fall back to [`crate::field::document_format::DocumentFormat::Opaque`]
    /// (Phase 21.5.1).
    pub max_json_document_bytes: u64,
    // The YAML caps below bound the derived, span-preserving structured-tree model
    // (Phase 21.6.1). Like JSON, YAML is not a package: the whole source parses as a
    // bounded stream of documents beneath these caps, and everything derived is
    // `Q_gen` only.
    /// Maximum YAML container nesting depth accepted (mappings/sequences). A deeper
    /// document is not detected as YAML (and any direct parse declines typed) rather
    /// than risking unbounded recursion (Phase 21.6.1).
    pub max_yaml_depth: u32,
    /// Maximum YAML nodes (containers, scalars, aliases, empties) accepted in one
    /// stream. An over-large document declines typed rather than allocating
    /// (Phase 21.6.1).
    pub max_yaml_nodes: u32,
    /// Maximum YAML scalar nodes accepted in one stream (Phase 21.6.1).
    pub max_yaml_scalars: u32,
    /// Maximum YAML anchors (`&a`) accepted in one stream (Phase 21.6.1).
    pub max_yaml_anchors: u32,
    /// Maximum documents accepted in one YAML stream (Phase 21.6.1).
    pub max_yaml_documents: u32,
    /// Maximum total raw scalar-token bytes accepted across one YAML stream. A
    /// conservative upper bound on the decoded text (Phase 21.6.1).
    pub max_yaml_string_bytes: u64,
    /// Maximum source length admitted for byte-based YAML detection. Larger inputs
    /// fall back to [`crate::field::document_format::DocumentFormat::Opaque`]
    /// (Phase 21.6.1).
    pub max_yaml_document_bytes: u64,
    // The CSV/TSV caps below bound the derived, span-preserving tabular model and
    // its streaming reads (Phase 21.7.1). CSV/TSV is not a package: the whole
    // source is the exact leaf, and everything derived is `Q_gen` only.
    /// Maximum records accepted in one CSV/TSV table for a **built** span model
    /// (including the header row). A larger table declines typed rather than
    /// allocating; the streaming read selectors (`csv-row`/`csv-cell`/...) are not
    /// bounded by this (they are bounded by one record at a time) (Phase 21.7.1).
    pub max_csv_rows: u32,
    /// Maximum fields accepted in a single CSV/TSV record. A record above this
    /// bound is a typed resource-limit decline, never an allocation (Phase 21.7.1).
    pub max_csv_cols: u32,
    /// Maximum byte length of a single CSV/TSV record (terminator included) when a
    /// span is retained; a longer record declines typed (Phase 21.7.1).
    pub max_csv_record_bytes: u32,
    /// Maximum byte length of a single CSV/TSV field (quotes included); a longer
    /// field declines typed (Phase 21.7.1).
    pub max_csv_field_bytes: u32,
    /// Maximum source length admitted for byte-based CSV/TSV detection and parsing.
    /// Larger inputs fall back to
    /// [`crate::field::document_format::DocumentFormat::Opaque`] (Phase 21.7.1).
    pub max_csv_document_bytes: u64,
    /// Maximum records scanned when deciding whether a source is a CSV/TSV table.
    /// Detection is a bounded sample, so a huge source is classified in bounded
    /// memory (Phase 21.7.1).
    pub max_csv_sampled_records_for_detection: u32,
    // The Markdown caps below bound the derived, span-preserving prose model and its
    // inline extraction (Phase 21.8.1). Markdown is not a package: the whole source
    // is the exact leaf, and everything derived is `Q_gen` only.
    /// Maximum container nesting depth accepted (list item depth, blockquote marker
    /// depth). A deeper document is a typed resource-limit decline (Phase 21.8.1).
    pub max_markdown_depth: u32,
    /// Maximum nodes (blocks plus inline spans) accepted in one document. An
    /// over-large document declines typed rather than allocating (Phase 21.8.1).
    pub max_markdown_nodes: u32,
    /// Maximum blocks accepted in one document (Phase 21.8.1).
    pub max_markdown_blocks: u32,
    /// Maximum inline spans accepted in one document (Phase 21.8.1).
    pub max_markdown_inline_spans: u32,
    /// Maximum total code content bytes accepted across one document (fenced and
    /// indented code). A larger document declines typed (Phase 21.8.1).
    pub max_markdown_code_bytes: u64,
    /// Maximum source length admitted for byte-based Markdown detection and
    /// parsing. Larger inputs fall back to
    /// [`crate::field::document_format::DocumentFormat::Opaque`] (Phase 21.8.1).
    pub max_markdown_document_bytes: u64,
    // The HTML caps below bound the derived, span-preserving, error-recovering HTML
    // model (Phase 21.10). HTML is not a package: the whole source is the exact
    // leaf, and everything derived is `Q_gen` only. Unlike XML the HTML scanner is
    // **error-recovering** (implicit tag closing, void elements, unquoted
    // attributes, stray end tags), so a malformed document is recovered rather than
    // declined — only a bound or a forbidden DOCTYPE internal subset declines.
    /// Maximum element (and raw-text) nesting depth accepted. A deeper document is
    /// a typed resource-limit decline (Phase 21.10).
    pub max_html_depth: u32,
    /// Maximum nodes (elements, text runs, comments, DOCTYPE, raw text) accepted in
    /// one document. An over-large document declines typed rather than allocating
    /// (Phase 21.10).
    pub max_html_nodes: u32,
    /// Maximum attributes accepted across one document (Phase 21.10).
    pub max_html_attrs: u64,
    /// Maximum total text bytes accepted across one document. A larger document
    /// declines typed (Phase 21.10).
    pub max_html_text_bytes: u64,
    /// Maximum total raw `<script>`/`<style>` content bytes accepted across one
    /// document (their raw text is captured, never executed or parsed) (Phase
    /// 21.10).
    pub max_html_script_bytes: u64,
    /// Maximum source length admitted for byte-based HTML detection and parsing.
    /// Larger inputs fall back to
    /// [`crate::field::document_format::DocumentFormat::Opaque`] (Phase 21.10).
    pub max_html_document_bytes: u64,
    // The TOML caps below bound the derived, span-preserving TOML model (Phase
    // 21.11). TOML is not a package: the whole source is the exact leaf, and
    // everything derived is `Q_gen` only. TOML's duplicate-key and redefinition
    // rules are enforced (a violation is a typed decline).
    /// Maximum table/key nesting depth accepted. A deeper document is a typed
    /// resource-limit decline (Phase 21.11).
    pub max_toml_depth: u32,
    /// Maximum nodes (tables, arrays, keys, scalars) accepted in one document. An
    /// over-large document declines typed rather than allocating (Phase 21.11).
    pub max_toml_nodes: u32,
    /// Maximum keys accepted across one document (Phase 21.11).
    pub max_toml_keys: u64,
    /// Maximum total string/key bytes accepted across one document. A larger
    /// document declines typed (Phase 21.11).
    pub max_toml_string_bytes: u64,
    /// Maximum source length admitted for byte-based TOML detection and parsing.
    /// Larger inputs fall back to
    /// [`crate::field::document_format::DocumentFormat::Opaque`] (Phase 21.11).
    pub max_toml_document_bytes: u64,
    // The JSONL/NDJSON caps below bound the derived, per-line span-preserving model
    // (Phase 21.12). JSONL is not a package: the whole source is the exact leaf, and
    // everything derived is `Q_gen` only. Each non-blank line is parsed by the
    // shared JSON parser (never a second parser), so a line is bounded by the JSON
    // depth cap as well.
    /// Maximum non-blank record lines accepted in one JSONL document. A larger
    /// document declines typed rather than allocating (Phase 21.12).
    pub max_jsonl_records: u32,
    /// Maximum byte length of one record line's JSON text (the terminator
    /// excluded). A longer line declines typed (Phase 21.12).
    pub max_jsonl_line_bytes: u64,
    /// Maximum total JSON nodes (values plus object member keys) accepted across
    /// **all** records of one JSONL document. This is a whole-document budget (a
    /// single line is additionally bounded by the JSON node cap); an over-large
    /// document declines typed rather than allocating (Phase 21.12).
    pub max_jsonl_nodes: u32,
    /// Maximum source length admitted for byte-based JSONL detection and parsing.
    /// Larger inputs fall back to
    /// [`crate::field::document_format::DocumentFormat::Opaque`] (Phase 21.12).
    pub max_jsonl_document_bytes: u64,
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
        max_xml_document_bytes: 1 << 34,
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
        max_ods_sheets: 4096,
        max_ods_cells: 1 << 24,
        max_ods_repeated_span: 1 << 20,
        max_ods_merges: 1 << 20,
        max_ods_named_expressions: 1 << 20,
        max_ods_styles: 1 << 16,
        max_ods_comments: 1 << 20,
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
        max_xlsx_col: 16384,
        max_xlsx_row: 1 << 20,
        max_pptx_slides: 4096,
        max_pptx_shapes_per_slide: 1 << 20,
        max_pptx_text_runs: 1 << 22,
        max_pptx_group_depth: 64,
        max_pptx_media: 1 << 14,
        max_pptx_tables: 1 << 14,
        max_pptx_table_cells: 1 << 20,
        max_pptx_notes: 1 << 16,
        max_pptx_layouts: 1 << 14,
        max_pptx_masters: 1 << 14,
        max_odp_slides: 4096,
        max_odp_shapes_per_slide: 1 << 20,
        max_odp_text_runs: 1 << 22,
        max_odp_group_depth: 64,
        max_odp_media: 1 << 14,
        max_odp_tables: 1 << 14,
        max_odp_table_cells: 1 << 20,
        max_odp_notes: 1 << 16,
        max_odp_masters: 1 << 14,
        max_json_depth: 256,
        max_json_nodes: 1 << 24,
        max_json_string_bytes: 1 << 28,
        max_json_document_bytes: 1 << 34,
        max_yaml_depth: 256,
        max_yaml_nodes: 1 << 24,
        max_yaml_scalars: 1 << 24,
        max_yaml_anchors: 1 << 20,
        max_yaml_documents: 1 << 16,
        max_yaml_string_bytes: 1 << 28,
        max_yaml_document_bytes: 1 << 34,
        max_csv_rows: 1 << 24,
        max_csv_cols: 1 << 16,
        max_csv_record_bytes: 1 << 26,
        max_csv_field_bytes: 1 << 24,
        max_csv_document_bytes: 1 << 34,
        max_csv_sampled_records_for_detection: 1024,
        max_markdown_depth: 256,
        max_markdown_nodes: 1 << 24,
        max_markdown_blocks: 1 << 22,
        max_markdown_inline_spans: 1 << 24,
        max_markdown_code_bytes: 1 << 28,
        max_markdown_document_bytes: 1 << 34,
        max_html_depth: 256,
        max_html_nodes: 1 << 24,
        max_html_attrs: 1 << 26,
        max_html_text_bytes: 1 << 28,
        max_html_script_bytes: 1 << 28,
        max_html_document_bytes: 1 << 34,
        max_toml_depth: 256,
        max_toml_nodes: 1 << 24,
        max_toml_keys: 1 << 22,
        max_toml_string_bytes: 1 << 28,
        max_toml_document_bytes: 1 << 34,
        max_jsonl_records: 1 << 22,
        max_jsonl_line_bytes: 1 << 26,
        max_jsonl_nodes: 1 << 24,
        max_jsonl_document_bytes: 1 << 34,
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
        max_xml_document_bytes: 1 << 26,
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
        max_ods_sheets: 64,
        max_ods_cells: 1 << 16,
        max_ods_repeated_span: 1 << 14,
        max_ods_merges: 1 << 14,
        max_ods_named_expressions: 1 << 14,
        max_ods_styles: 1 << 12,
        max_ods_comments: 1 << 14,
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
        max_xlsx_col: 16384,
        max_xlsx_row: 1 << 20,
        max_pptx_slides: 64,
        max_pptx_shapes_per_slide: 1 << 16,
        max_pptx_text_runs: 1 << 18,
        max_pptx_group_depth: 16,
        max_pptx_media: 1 << 12,
        max_pptx_tables: 1 << 12,
        max_pptx_table_cells: 1 << 16,
        max_pptx_notes: 1 << 12,
        max_pptx_layouts: 1 << 12,
        max_pptx_masters: 1 << 12,
        max_odp_slides: 64,
        max_odp_shapes_per_slide: 1 << 16,
        max_odp_text_runs: 1 << 18,
        max_odp_group_depth: 16,
        max_odp_media: 1 << 12,
        max_odp_tables: 1 << 12,
        max_odp_table_cells: 1 << 16,
        max_odp_notes: 1 << 12,
        max_odp_masters: 1 << 12,
        max_json_depth: 64,
        max_json_nodes: 1 << 16,
        max_json_string_bytes: 1 << 20,
        max_json_document_bytes: 1 << 26,
        max_yaml_depth: 64,
        max_yaml_nodes: 1 << 16,
        max_yaml_scalars: 1 << 16,
        max_yaml_anchors: 1 << 12,
        max_yaml_documents: 1 << 10,
        max_yaml_string_bytes: 1 << 20,
        max_yaml_document_bytes: 1 << 26,
        max_csv_rows: 1 << 16,
        max_csv_cols: 1 << 12,
        max_csv_record_bytes: 1 << 20,
        max_csv_field_bytes: 1 << 18,
        max_csv_document_bytes: 1 << 26,
        max_csv_sampled_records_for_detection: 64,
        max_markdown_depth: 64,
        max_markdown_nodes: 1 << 16,
        max_markdown_blocks: 1 << 14,
        max_markdown_inline_spans: 1 << 16,
        max_markdown_code_bytes: 1 << 20,
        max_markdown_document_bytes: 1 << 26,
        max_html_depth: 64,
        max_html_nodes: 1 << 16,
        max_html_attrs: 1 << 16,
        max_html_text_bytes: 1 << 20,
        max_html_script_bytes: 1 << 20,
        max_html_document_bytes: 1 << 26,
        max_toml_depth: 64,
        max_toml_nodes: 1 << 16,
        max_toml_keys: 1 << 14,
        max_toml_string_bytes: 1 << 20,
        max_toml_document_bytes: 1 << 26,
        max_jsonl_records: 1 << 12,
        max_jsonl_line_bytes: 1 << 20,
        max_jsonl_nodes: 1 << 16,
        max_jsonl_document_bytes: 1 << 26,
    };
}

impl Default for Limits {
    fn default() -> Self {
        Limits::DEFAULT
    }
}
