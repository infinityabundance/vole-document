//! Bounded, representation-preserving Parquet adapter (Phase 21.14).
//!
//! Apache Parquet is the **analytical** Wave-2 format — the plan's most adversarial
//! target because a columnar store (DuckDB/Parquet) *already* embodies projection,
//! predicate pushdown, compressed pages, and metadata indexes. VOLE does **not**
//! re-implement a columnar engine; it treats a Parquet file as one more document
//! whose exact leaf is the **whole source** (a `DocumentExact`, RAW-like authority)
//! and whose derived (`Q_gen`) projection reads the real footer and page layout.
//!
//! ## What this adapter is
//!
//! A **bounded, dependency-free** reader of the Parquet physical layout. It parses
//! the Thrift-Compact `FileMetaData` footer (schema, row groups, column chunks,
//! page metadata), exposes the schema and the row-group/column-chunk inventory —
//! each column chunk with its **exact source span** — and decodes a documented
//! subset of the values with checked arithmetic and hard bounds. Everything it
//! produces is derived from the footer and page headers; it never sits on the
//! exactness path.
//!
//! ## Supported / declined (honest scope)
//!
//! * **Encodings supported:** `PLAIN` (0) and `RLE_DICTIONARY`/`PLAIN_DICTIONARY`
//!   (8/2) for data pages, plus the `RLE` bit-packing hybrid for definition levels;
//!   the deprecated `BIT_PACKED` level encoding is **declined typed**.
//! * **Compression supported:** `UNCOMPRESSED` (0) and `GZIP` (2) when the shipped
//!   pure-Rust inflate seam (`field` feature) is compiled in; every other codec
//!   (`SNAPPY`, `ZSTD`, `BROTLI`, …) is **declined typed**.
//! * **Physical types supported:** `BOOLEAN`, `INT32`, `INT64`, `FLOAT`, `DOUBLE`,
//!   `BYTE_ARRAY`, `FIXED_LEN_BYTE_ARRAY`. `INT96` and any other value are
//!   **declined typed**.
//! * **Page types supported:** `DATA_PAGE` (v1) and `DICTIONARY_PAGE`.
//!   `DATA_PAGE_V2` and `INDEX_PAGE` content are **declined typed** (index pages are
//!   skipped without decoding).
//! * **Repetition supported:** flat columns (`max_rep == 0`, `max_def <= 1`). A
//!   repeated (list) or nested-optional column is **declined typed**.
//! * **Statistics:** the modern `min_value`/`max_value` (plain-encoded) fields are
//!   read; the legacy `min`/`max` fields are **not** interpreted (documented).
//!
//! It **never guesses**: an encoding, codec, type, or page layout it does not
//! implement is a typed [`Error::unsupported_feature`] decline, never a wrong
//! answer and never a panic.
//!
//! ## Detection
//!
//! [`detect`] is a pure, bounded, byte-level check: the source begins with `PAR1`,
//! ends with `PAR1`, and the 4-byte little-endian footer length immediately before
//! the trailing magic is *consistent* with the file length (`footer_len + 8` does
//! not overrun the leading magic). This is exactly the Parquet magic-byte contract
//! and is documented in
//! [`DocumentFormat::Parquet`](crate::field::document_format::DocumentFormat::Parquet).
//!
//! ## Bounds and no panics
//!
//! Untrusted input can only ever yield a typed decline: the source length by
//! [`Limits::max_parquet_document_bytes`], the footer by
//! [`Limits::max_parquet_footer_bytes`], the row-group count by
//! [`Limits::max_parquet_row_groups`], the leaf-column count by
//! [`Limits::max_parquet_columns`], the pages-per-chunk count by
//! [`Limits::max_parquet_pages_per_chunk`], the decoded value count by
//! [`Limits::max_parquet_values`], and the total decompressed page bytes by
//! [`Limits::max_parquet_decompressed_bytes`]. All offsets and lengths use checked
//! arithmetic.

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard caps on decoded model arena sizes (defend the decoder against a hostile blob).
pub const MAX_MODEL_SCHEMA: u32 = 1 << 22;
/// Maximum decoded model leaf columns.
pub const MAX_MODEL_LEAVES: u32 = 1 << 20;
/// Maximum decoded model row groups.
pub const MAX_MODEL_ROW_GROUPS: u32 = 1 << 20;
/// Maximum decoded model column chunks.
pub const MAX_MODEL_CHUNKS: u64 = 1 << 24;
/// Deepest Thrift-Compact nesting accepted in the footer.
pub const MAX_THRIFT_DEPTH: u32 = 64;

// --- physical types (parquet.thrift `Type`) ---------------------------------
/// Physical type: `BOOLEAN`.
pub const T_BOOLEAN: i32 = 0;
/// Physical type: `INT32`.
pub const T_INT32: i32 = 1;
/// Physical type: `INT64`.
pub const T_INT64: i32 = 2;
/// Physical type: `INT96` (declined typed).
pub const T_INT96: i32 = 3;
/// Physical type: `FLOAT`.
pub const T_FLOAT: i32 = 4;
/// Physical type: `DOUBLE`.
pub const T_DOUBLE: i32 = 5;
/// Physical type: `BYTE_ARRAY`.
pub const T_BYTE_ARRAY: i32 = 6;
/// Physical type: `FIXED_LEN_BYTE_ARRAY`.
pub const T_FIXED_LEN_BYTE_ARRAY: i32 = 7;

// --- field repetition (parquet.thrift `FieldRepetitionType`) ----------------
/// Repetition: `REQUIRED`.
pub const REP_REQUIRED: i32 = 0;
/// Repetition: `OPTIONAL`.
pub const REP_OPTIONAL: i32 = 1;
/// Repetition: `REPEATED`.
pub const REP_REPEATED: i32 = 2;

// --- encodings (parquet.thrift `Encoding`) ----------------------------------
/// Encoding: `PLAIN`.
pub const ENC_PLAIN: i32 = 0;
/// Encoding: `GROUP_VAR_INT` (declined typed).
pub const ENC_GROUP_VAR_INT: i32 = 1;
/// Encoding: `PLAIN_DICTIONARY`.
pub const ENC_PLAIN_DICTIONARY: i32 = 2;
/// Encoding: `RLE`.
pub const ENC_RLE: i32 = 3;
/// Encoding: `BIT_PACKED` (declined typed for levels).
pub const ENC_BIT_PACKED: i32 = 4;
/// Encoding: `DELTA_BINARY_PACKED` (declined typed).
pub const ENC_DELTA_BINARY_PACKED: i32 = 5;
/// Encoding: `DELTA_LENGTH_BYTE_ARRAY` (declined typed).
pub const ENC_DELTA_LENGTH_BYTE_ARRAY: i32 = 6;
/// Encoding: `DELTA_BYTE_ARRAY` (declined typed).
pub const ENC_DELTA_BYTE_ARRAY: i32 = 7;
/// Encoding: `RLE_DICTIONARY`.
pub const ENC_RLE_DICTIONARY: i32 = 8;
/// Encoding: `BYTE_STREAM_SPLIT` (declined typed).
pub const ENC_BYTE_STREAM_SPLIT: i32 = 9;

// --- compression codecs (parquet.thrift `CompressionCodec`) -----------------
/// Codec: `UNCOMPRESSED`.
pub const CODEC_UNCOMPRESSED: i32 = 0;
/// Codec: `SNAPPY` (declined typed).
pub const CODEC_SNAPPY: i32 = 1;
/// Codec: `GZIP` (supported when the `field` inflate seam is compiled).
pub const CODEC_GZIP: i32 = 2;
/// Codec: `LZO` (declined typed).
pub const CODEC_LZO: i32 = 3;
/// Codec: `BROTLI` (declined typed).
pub const CODEC_BROTLI: i32 = 4;
/// Codec: `LZ4` (declined typed).
pub const CODEC_LZ4: i32 = 5;
/// Codec: `ZSTD` (declined typed).
pub const CODEC_ZSTD: i32 = 6;
/// Codec: `LZ4_RAW` (declined typed).
pub const CODEC_LZ4_RAW: i32 = 7;

// --- page types (parquet.thrift `PageType`) ---------------------------------
/// Page type: `DATA_PAGE` (v1).
pub const PAGE_DATA: i32 = 0;
/// Page type: `INDEX_PAGE`.
pub const PAGE_INDEX: i32 = 1;
/// Page type: `DICTIONARY_PAGE`.
pub const PAGE_DICTIONARY: i32 = 2;
/// Page type: `DATA_PAGE_V2` (declined typed).
pub const PAGE_DATA_V2: i32 = 3;

/// Stable name of a physical type tag.
pub const fn physical_type_name(t: i32) -> &'static str {
    match t {
        T_BOOLEAN => "BOOLEAN",
        T_INT32 => "INT32",
        T_INT64 => "INT64",
        T_INT96 => "INT96",
        T_FLOAT => "FLOAT",
        T_DOUBLE => "DOUBLE",
        T_BYTE_ARRAY => "BYTE_ARRAY",
        T_FIXED_LEN_BYTE_ARRAY => "FIXED_LEN_BYTE_ARRAY",
        _ => "UNKNOWN",
    }
}

/// Stable name of a repetition tag.
pub const fn repetition_name(r: i32) -> &'static str {
    match r {
        REP_REQUIRED => "REQUIRED",
        REP_OPTIONAL => "OPTIONAL",
        REP_REPEATED => "REPEATED",
        _ => "UNKNOWN",
    }
}

/// Stable name of a compression codec tag.
pub const fn codec_name(c: i32) -> &'static str {
    match c {
        CODEC_UNCOMPRESSED => "UNCOMPRESSED",
        CODEC_SNAPPY => "SNAPPY",
        CODEC_GZIP => "GZIP",
        CODEC_LZO => "LZO",
        CODEC_BROTLI => "BROTLI",
        CODEC_LZ4 => "LZ4",
        CODEC_ZSTD => "ZSTD",
        CODEC_LZ4_RAW => "LZ4_RAW",
        _ => "UNKNOWN",
    }
}

/// Stable name of an encoding tag.
pub const fn encoding_name(e: i32) -> &'static str {
    match e {
        ENC_PLAIN => "PLAIN",
        ENC_GROUP_VAR_INT => "GROUP_VAR_INT",
        ENC_PLAIN_DICTIONARY => "PLAIN_DICTIONARY",
        ENC_RLE => "RLE",
        ENC_BIT_PACKED => "BIT_PACKED",
        ENC_DELTA_BINARY_PACKED => "DELTA_BINARY_PACKED",
        ENC_DELTA_LENGTH_BYTE_ARRAY => "DELTA_LENGTH_BYTE_ARRAY",
        ENC_DELTA_BYTE_ARRAY => "DELTA_BYTE_ARRAY",
        ENC_RLE_DICTIONARY => "RLE_DICTIONARY",
        ENC_BYTE_STREAM_SPLIT => "BYTE_STREAM_SPLIT",
        _ => "UNKNOWN",
    }
}

/// Stable name of a page type tag.
pub const fn page_type_name(t: i32) -> &'static str {
    match t {
        PAGE_DATA => "DATA_PAGE",
        PAGE_INDEX => "INDEX_PAGE",
        PAGE_DICTIONARY => "DICTIONARY_PAGE",
        PAGE_DATA_V2 => "DATA_PAGE_V2",
        _ => "UNKNOWN",
    }
}

/// Stable name of a legacy `ConvertedType` tag.
pub const fn converted_type_name(c: i32) -> &'static str {
    match c {
        0 => "UTF8",
        1 => "MAP",
        2 => "MAP_KEY_VALUE",
        3 => "LIST",
        4 => "ENUM",
        5 => "DECIMAL",
        6 => "DATE",
        7 => "TIME_MILLIS",
        8 => "TIME_MICROS",
        9 => "TIMESTAMP_MILLIS",
        10 => "TIMESTAMP_MICROS",
        11 => "UINT_8",
        12 => "UINT_16",
        13 => "UINT_32",
        14 => "UINT_64",
        15 => "INT_8",
        16 => "INT_16",
        17 => "INT_32",
        18 => "INT_64",
        19 => "JSON",
        20 => "BSON",
        21 => "INTERVAL",
        _ => "UNKNOWN",
    }
}

/// Stable name of a `LogicalType` union field id.
pub const fn logical_type_name(tag: i32) -> &'static str {
    match tag {
        1 => "STRING",
        2 => "MAP",
        3 => "LIST",
        4 => "ENUM",
        5 => "DECIMAL",
        6 => "DATE",
        7 => "TIME",
        8 => "TIMESTAMP",
        10 => "INTEGER",
        11 => "UNKNOWN",
        12 => "JSON",
        13 => "BSON",
        14 => "UUID",
        15 => "FLOAT16",
        _ => "UNKNOWN",
    }
}

/// Whether a leaf column's values are best rendered as UTF-8 text (a
/// string/enum/json logical type) rather than hex.
pub fn is_string_like(leaf: &LeafColumn) -> bool {
    if let Some(c) = leaf.converted
        && (c == 0 || c == 4 || c == 19)
    {
        return true;
    }
    matches!(leaf.logical, Some(1 | 4 | 12))
}

// ---------------------------------------------------------------------------
// The canonical derived model
// ---------------------------------------------------------------------------

/// One flattened schema element (the tree in pre-order, with parent links).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaElement {
    /// Nesting depth (the root is 0).
    pub depth: u32,
    /// Parent schema-element index (`u32::MAX` for the root).
    pub parent: u32,
    /// The element name exactly as written.
    pub name: String,
    /// The physical type tag, if this element is a leaf.
    pub physical: Option<i32>,
    /// The `type_length` (only meaningful for `FIXED_LEN_BYTE_ARRAY`).
    pub type_length: Option<i32>,
    /// The repetition tag.
    pub repetition: i32,
    /// The legacy converted type tag, if any.
    pub converted: Option<i32>,
    /// The `LogicalType` union field id, if any.
    pub logical: Option<i32>,
    /// The declared child count (0 for a leaf).
    pub num_children: u32,
}

/// One logical leaf column (a physical column in the row groups).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeafColumn {
    /// The schema-element index of this leaf.
    pub element: u32,
    /// The path from the schema root's child down to this leaf.
    pub path: Vec<String>,
    /// The physical type tag.
    pub physical: i32,
    /// The `type_length` for `FIXED_LEN_BYTE_ARRAY`.
    pub type_length: Option<i32>,
    /// The repetition tag.
    pub repetition: i32,
    /// The legacy converted type tag, if any.
    pub converted: Option<i32>,
    /// The `LogicalType` union field id, if any.
    pub logical: Option<i32>,
    /// The maximum definition level for this column.
    pub max_def: u32,
    /// The maximum repetition level for this column.
    pub max_rep: u32,
}

/// The statistics and inventory of one column chunk, with its exact source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnChunkDesc {
    /// The leaf-column index this chunk belongs to.
    pub leaf: u32,
    /// The compression codec tag.
    pub codec: i32,
    /// The number of values (including nulls) in the chunk.
    pub num_values: i64,
    /// The total compressed size of the chunk (including page headers).
    pub total_compressed_size: i64,
    /// The total uncompressed size of the chunk (including page headers).
    pub total_uncompressed_size: i64,
    /// The `data_page_offset`.
    pub data_page_offset: i64,
    /// The `dictionary_page_offset`, if any.
    pub dictionary_page_offset: Option<i64>,
    /// The `file_offset` recorded on the `ColumnChunk`.
    pub file_offset: i64,
    /// The encodings declared in the chunk metadata.
    pub encodings: Vec<i32>,
    /// The exact source span `[span_start, span_end)` of the chunk's pages.
    pub span_start: u64,
    /// One past the exact source span.
    pub span_end: u64,
    /// The modern plain-encoded `min_value`, if present.
    pub min_value: Option<Vec<u8>>,
    /// The modern plain-encoded `max_value`, if present.
    pub max_value: Option<Vec<u8>>,
    /// The declared `null_count`, if present.
    pub null_count: Option<i64>,
}

/// One row group and its column chunks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowGroupDesc {
    /// The number of rows in the row group.
    pub num_rows: i64,
    /// The declared total byte size of the row group.
    pub total_byte_size: i64,
    /// The column chunks, in leaf-column order.
    pub chunks: Vec<ColumnChunkDesc>,
}

/// The canonical derived Parquet model (the materialization of a `ParquetModel`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParquetModel {
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// The file format `version` recorded in the footer.
    pub version: i32,
    /// The number of rows recorded in the footer.
    pub num_rows: i64,
    /// The `created_by` string, if present.
    pub created_by: Option<String>,
    /// The flattened schema (the root first).
    pub schema: Vec<SchemaElement>,
    /// The logical leaf columns.
    pub leaves: Vec<LeafColumn>,
    /// The row groups.
    pub row_groups: Vec<RowGroupDesc>,
}

impl ParquetModel {
    /// The leaf column at `index`, if present.
    pub fn leaf(&self, index: u32) -> Option<&LeafColumn> {
        self.leaves.get(index as usize)
    }

    /// The row group at `index`, if present.
    pub fn row_group(&self, index: u32) -> Option<&RowGroupDesc> {
        self.row_groups.get(index as usize)
    }

    /// The number of column chunks across every row group.
    pub fn chunk_count(&self) -> u64 {
        self.row_groups
            .iter()
            .map(|rg| rg.chunks.len() as u64)
            .fold(0u64, u64::saturating_add)
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(128 + self.leaves.len() * 64);
        out.extend_from_slice(b"PQTM");
        out.push(MODEL_VERSION);
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&self.version.to_le_bytes());
        out.extend_from_slice(&self.num_rows.to_le_bytes());
        enc_opt_str(&mut out, self.created_by.as_deref());
        out.extend_from_slice(&(self.schema.len() as u32).to_le_bytes());
        for s in &self.schema {
            out.extend_from_slice(&s.depth.to_le_bytes());
            out.extend_from_slice(&s.parent.to_le_bytes());
            enc_str(&mut out, &s.name);
            enc_opt_i32(&mut out, s.physical);
            enc_opt_i32(&mut out, s.type_length);
            out.extend_from_slice(&s.repetition.to_le_bytes());
            enc_opt_i32(&mut out, s.converted);
            enc_opt_i32(&mut out, s.logical);
            out.extend_from_slice(&s.num_children.to_le_bytes());
        }
        out.extend_from_slice(&(self.leaves.len() as u32).to_le_bytes());
        for l in &self.leaves {
            out.extend_from_slice(&l.element.to_le_bytes());
            out.extend_from_slice(&(l.path.len() as u32).to_le_bytes());
            for p in &l.path {
                enc_str(&mut out, p);
            }
            out.extend_from_slice(&l.physical.to_le_bytes());
            enc_opt_i32(&mut out, l.type_length);
            out.extend_from_slice(&l.repetition.to_le_bytes());
            enc_opt_i32(&mut out, l.converted);
            enc_opt_i32(&mut out, l.logical);
            out.extend_from_slice(&l.max_def.to_le_bytes());
            out.extend_from_slice(&l.max_rep.to_le_bytes());
        }
        out.extend_from_slice(&(self.row_groups.len() as u32).to_le_bytes());
        for rg in &self.row_groups {
            out.extend_from_slice(&rg.num_rows.to_le_bytes());
            out.extend_from_slice(&rg.total_byte_size.to_le_bytes());
            out.extend_from_slice(&(rg.chunks.len() as u32).to_le_bytes());
            for c in &rg.chunks {
                out.extend_from_slice(&c.leaf.to_le_bytes());
                out.extend_from_slice(&c.codec.to_le_bytes());
                out.extend_from_slice(&c.num_values.to_le_bytes());
                out.extend_from_slice(&c.total_compressed_size.to_le_bytes());
                out.extend_from_slice(&c.total_uncompressed_size.to_le_bytes());
                out.extend_from_slice(&c.data_page_offset.to_le_bytes());
                enc_opt_i64(&mut out, c.dictionary_page_offset);
                out.extend_from_slice(&c.file_offset.to_le_bytes());
                out.extend_from_slice(&(c.encodings.len() as u32).to_le_bytes());
                for e in &c.encodings {
                    out.extend_from_slice(&e.to_le_bytes());
                }
                out.extend_from_slice(&c.span_start.to_le_bytes());
                out.extend_from_slice(&c.span_end.to_le_bytes());
                enc_opt_bytes(&mut out, c.min_value.as_deref());
                enc_opt_bytes(&mut out, c.max_value.as_deref());
                enc_opt_i64(&mut out, c.null_count);
            }
        }
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<ParquetModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"PQTM" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let doc_len = r.u64()?;
        let version = r.i32()?;
        let num_rows = r.i64()?;
        let created_by = r.opt_str()?;
        let sc = r.u32()?;
        if sc > MAX_MODEL_SCHEMA {
            return Err(corrupt("model schema is implausibly large"));
        }
        let mut schema = Vec::with_capacity(sc as usize);
        for _ in 0..sc {
            let depth = r.u32()?;
            let parent = r.u32()?;
            let name = r.string()?;
            let physical = r.opt_i32()?;
            let type_length = r.opt_i32()?;
            let repetition = r.i32()?;
            let converted = r.opt_i32()?;
            let logical = r.opt_i32()?;
            let num_children = r.u32()?;
            if depth as u64 > MAX_MODEL_SCHEMA as u64 {
                return Err(corrupt("model schema depth is implausible"));
            }
            schema.push(SchemaElement {
                depth,
                parent,
                name,
                physical,
                type_length,
                repetition,
                converted,
                logical,
                num_children,
            });
        }
        let lc = r.u32()?;
        if lc > MAX_MODEL_LEAVES {
            return Err(corrupt("model leaf count is implausible"));
        }
        let mut leaves = Vec::with_capacity(lc as usize);
        for _ in 0..lc {
            let element = r.u32()?;
            let pc = r.u32()?;
            if pc as u64 > MAX_MODEL_SCHEMA as u64 {
                return Err(corrupt("model column path is implausibly long"));
            }
            let mut path = Vec::with_capacity(pc as usize);
            for _ in 0..pc {
                path.push(r.string()?);
            }
            let physical = r.i32()?;
            let type_length = r.opt_i32()?;
            let repetition = r.i32()?;
            let converted = r.opt_i32()?;
            let logical = r.opt_i32()?;
            let max_def = r.u32()?;
            let max_rep = r.u32()?;
            leaves.push(LeafColumn {
                element,
                path,
                physical,
                type_length,
                repetition,
                converted,
                logical,
                max_def,
                max_rep,
            });
        }
        let rgc = r.u32()?;
        if rgc > MAX_MODEL_ROW_GROUPS {
            return Err(corrupt("model row-group count is implausible"));
        }
        let mut row_groups = Vec::with_capacity(rgc as usize);
        let mut total_chunks: u64 = 0;
        for _ in 0..rgc {
            let num_rows = r.i64()?;
            let total_byte_size = r.i64()?;
            let cc = r.u32()?;
            total_chunks = total_chunks.saturating_add(cc as u64);
            if total_chunks > MAX_MODEL_CHUNKS {
                return Err(corrupt("model chunk count is implausible"));
            }
            let mut chunks = Vec::with_capacity(cc as usize);
            for _ in 0..cc {
                let leaf = r.u32()?;
                let codec = r.i32()?;
                let num_values = r.i64()?;
                let total_compressed_size = r.i64()?;
                let total_uncompressed_size = r.i64()?;
                let data_page_offset = r.i64()?;
                let dictionary_page_offset = r.opt_i64()?;
                let file_offset = r.i64()?;
                let ec = r.u32()?;
                if ec as u64 > MAX_MODEL_CHUNKS {
                    return Err(corrupt("model encoding list is implausible"));
                }
                let mut encodings = Vec::with_capacity(ec as usize);
                for _ in 0..ec {
                    encodings.push(r.i32()?);
                }
                let span_start = r.u64()?;
                let span_end = r.u64()?;
                if span_start > span_end || span_end > doc_len {
                    return Err(corrupt("model chunk span is outside the document"));
                }
                let min_value = r.opt_bytes()?;
                let max_value = r.opt_bytes()?;
                let null_count = r.opt_i64()?;
                chunks.push(ColumnChunkDesc {
                    leaf,
                    codec,
                    num_values,
                    total_compressed_size,
                    total_uncompressed_size,
                    data_page_offset,
                    dictionary_page_offset,
                    file_offset,
                    encodings,
                    span_start,
                    span_end,
                    min_value,
                    max_value,
                    null_count,
                });
            }
            row_groups.push(RowGroupDesc {
                num_rows,
                total_byte_size,
                chunks,
            });
        }
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        Ok(ParquetModel {
            doc_len,
            version,
            num_rows,
            created_by,
            schema,
            leaves,
            row_groups,
        })
    }
}

// ---------------------------------------------------------------------------
// Thrift-Compact reader (bounded)
// ---------------------------------------------------------------------------

const CT_STOP: u8 = 0;
const CT_BOOL_TRUE: u8 = 1;
const CT_BOOL_FALSE: u8 = 2;
const CT_BYTE: u8 = 3;
const CT_I16: u8 = 4;
const CT_I32: u8 = 5;
const CT_I64: u8 = 6;
const CT_DOUBLE: u8 = 7;
const CT_BINARY: u8 = 8;
const CT_LIST: u8 = 9;
const CT_SET: u8 = 10;
const CT_MAP: u8 = 11;
const CT_STRUCT: u8 = 12;

/// A bounded Thrift-Compact value tree (only what the footer needs). Boolean,
/// double, and map payloads are consumed but not retained, so they carry no
/// field.
#[derive(Debug, Clone)]
enum TV {
    Bool,
    Int(i64),
    Dbl,
    Bin(Vec<u8>),
    List(Vec<TV>),
    Struct(Vec<(i16, TV)>),
    Map,
}

impl TV {
    fn field(&self, id: i16) -> Option<&TV> {
        match self {
            TV::Struct(fs) => fs.iter().find(|(f, _)| *f == id).map(|(_, v)| v),
            _ => None,
        }
    }

    fn as_int(&self) -> Option<i64> {
        match self {
            TV::Int(n) => Some(*n),
            _ => None,
        }
    }

    fn as_bin(&self) -> Option<&[u8]> {
        match self {
            TV::Bin(b) => Some(b),
            _ => None,
        }
    }

    fn as_list(&self) -> Option<&[TV]> {
        match self {
            TV::List(l) => Some(l),
            _ => None,
        }
    }

    fn as_struct(&self) -> Option<&[(i16, TV)]> {
        match self {
            TV::Struct(fs) => Some(fs),
            _ => None,
        }
    }
}

fn corrupt(msg: impl Into<String>) -> Error {
    Error::invalid_parquet_structure(msg)
}

struct TBuf<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> TBuf<'a> {
    fn new(b: &'a [u8]) -> Self {
        TBuf { b, at: 0 }
    }

    fn remaining(&self) -> usize {
        self.b.len() - self.at
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| corrupt("thrift overflow"))?;
        let s = self
            .b
            .get(self.at..end)
            .ok_or_else(|| corrupt("truncated thrift footer"))?;
        self.at = end;
        Ok(s)
    }

    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn uvarint(&mut self) -> Result<u64> {
        let mut acc: u64 = 0;
        let mut shift: u32 = 0;
        loop {
            let b = self.byte()?;
            if shift >= 64 {
                return Err(corrupt("thrift varint overflow"));
            }
            acc |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Ok(acc);
            }
            shift += 7;
        }
    }

    fn zigzag(&mut self) -> Result<i64> {
        let u = self.uvarint()?;
        Ok(((u >> 1) as i64) ^ -((u & 1) as i64))
    }

    fn read_value(&mut self, t: u8, depth: u32) -> Result<TV> {
        if depth > MAX_THRIFT_DEPTH {
            return Err(corrupt("thrift nesting is too deep"));
        }
        match t {
            CT_BOOL_TRUE | CT_BOOL_FALSE => Ok(TV::Bool),
            CT_BYTE => Ok(TV::Int(i64::from(self.byte()? as i8))),
            CT_I16 | CT_I32 | CT_I64 => Ok(TV::Int(self.zigzag()?)),
            CT_DOUBLE => {
                let _ = self.take(8)?;
                Ok(TV::Dbl)
            }
            CT_BINARY => {
                let n = self.uvarint()?;
                let n = usize::try_from(n).map_err(|_| corrupt("binary length overflow"))?;
                if n > self.remaining() {
                    return Err(corrupt("truncated thrift binary"));
                }
                Ok(TV::Bin(self.take(n)?.to_vec()))
            }
            CT_LIST | CT_SET => {
                let h = self.byte()?;
                let et = h & 0x0f;
                let mut size = u64::from(h >> 4);
                if size == 15 {
                    size = self.uvarint()?;
                }
                if size > self.remaining() as u64 {
                    return Err(corrupt("thrift list length exceeds the footer"));
                }
                let mut v = Vec::with_capacity(size as usize);
                for _ in 0..size {
                    v.push(self.read_value(et, depth + 1)?);
                }
                Ok(TV::List(v))
            }
            CT_MAP => {
                let size = self.uvarint()?;
                if size > self.remaining() as u64 {
                    return Err(corrupt("thrift map length exceeds the footer"));
                }
                if size > 0 {
                    let kv = self.byte()?;
                    let kt = kv >> 4;
                    let vt = kv & 0x0f;
                    for _ in 0..size {
                        let _ = self.read_value(kt, depth + 1)?;
                        let _ = self.read_value(vt, depth + 1)?;
                    }
                }
                Ok(TV::Map)
            }
            CT_STRUCT => self.read_struct(depth + 1),
            other => Err(corrupt(format!("unknown thrift type nibble {other}"))),
        }
    }

    fn read_struct(&mut self, depth: u32) -> Result<TV> {
        if depth > MAX_THRIFT_DEPTH {
            return Err(corrupt("thrift nesting is too deep"));
        }
        let mut fields: Vec<(i16, TV)> = Vec::new();
        let mut last_id: i16 = 0;
        loop {
            let b = self.byte()?;
            if b == CT_STOP {
                break;
            }
            let t = b & 0x0f;
            let delta = b >> 4;
            let id = if delta == 0 {
                let z = self.zigzag()?;
                i16::try_from(z).map_err(|_| corrupt("thrift field id out of range"))?
            } else {
                last_id
                    .checked_add(i16::from(delta))
                    .ok_or_else(|| corrupt("thrift field id overflow"))?
            };
            last_id = id;
            let v = self.read_value(t, depth + 1)?;
            fields.push((id, v));
        }
        Ok(TV::Struct(fields))
    }
}

fn parse_footer(footer: &[u8]) -> Result<TV> {
    let mut buf = TBuf::new(footer);
    let v = buf.read_struct(0)?;
    if buf.remaining() != 0 {
        return Err(corrupt("trailing bytes after the footer struct"));
    }
    Ok(v)
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// The magic bytes at both ends of a Parquet file.
pub const MAGIC: &[u8; 4] = b"PAR1";

/// A pure, bounded, byte-level Parquet detector.
///
/// True iff the source begins with `PAR1`, ends with `PAR1`, and the 4-byte
/// little-endian footer length before the trailing magic is consistent with the
/// file length (the footer region does not overrun the leading magic). The
/// detection is exactly the Parquet magic-byte contract and never decodes a page.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.len() as u64 > limits.max_parquet_document_bytes {
        return false;
    }
    let n = source.len();
    if n < 12 {
        return false;
    }
    if &source[0..4] != MAGIC || &source[n - 4..n] != MAGIC {
        return false;
    }
    let footer_len =
        u32::from_le_bytes([source[n - 8], source[n - 7], source[n - 6], source[n - 5]]);
    if u64::from(footer_len) > limits.max_parquet_footer_bytes {
        return false;
    }
    match n.checked_sub(8 + footer_len as usize) {
        Some(start) => start >= 4,
        None => false,
    }
}

// ---------------------------------------------------------------------------
// Parsing the footer into the model
// ---------------------------------------------------------------------------

struct RawSchema {
    name: String,
    physical: Option<i32>,
    type_length: Option<i32>,
    repetition: i32,
    converted: Option<i32>,
    logical: Option<i32>,
    num_children: u32,
}

fn schema_element(tv: &TV) -> Result<RawSchema> {
    let name = tv
        .field(4)
        .and_then(TV::as_bin)
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .ok_or_else(|| corrupt("schema element has no name"))?;
    let physical = tv.field(1).and_then(TV::as_int).map(|n| n as i32);
    let type_length = tv.field(2).and_then(TV::as_int).map(|n| n as i32);
    let repetition = tv
        .field(3)
        .and_then(TV::as_int)
        .map(|n| n as i32)
        .unwrap_or(REP_REQUIRED);
    let converted = tv.field(6).and_then(TV::as_int).map(|n| n as i32);
    let logical = tv.field(10).and_then(logical_tag);
    let num_children = tv
        .field(5)
        .and_then(TV::as_int)
        .map(|n| n.max(0) as u32)
        .unwrap_or(0);
    Ok(RawSchema {
        name,
        physical,
        type_length,
        repetition,
        converted,
        logical,
        num_children,
    })
}

/// The `LogicalType` union's single set field id.
fn logical_tag(tv: &TV) -> Option<i32> {
    let fs = tv.as_struct()?;
    fs.first().map(|(id, _)| i32::from(*id))
}

fn flatten_schema(
    raw: &[RawSchema],
    out: &mut Vec<SchemaElement>,
    idx: &mut usize,
    depth: u32,
    parent: u32,
    limits: Limits,
) -> Result<()> {
    if *idx >= raw.len() {
        return Err(corrupt("schema tree is truncated"));
    }
    if depth > MAX_THRIFT_DEPTH {
        return Err(corrupt("schema tree is too deep"));
    }
    if out.len() as u64 > u64::from(limits.max_parquet_columns) * 4 + 16 {
        return Err(Error::resource_limit("Parquet schema is too large"));
    }
    let me = *idx;
    let node = &raw[me];
    *idx += 1;
    out.push(SchemaElement {
        depth,
        parent,
        name: node.name.clone(),
        physical: node.physical,
        type_length: node.type_length,
        repetition: node.repetition,
        converted: node.converted,
        logical: node.logical,
        num_children: node.num_children,
    });
    for _ in 0..node.num_children {
        flatten_schema(raw, out, idx, depth + 1, me as u32, limits)?;
    }
    Ok(())
}

fn build_leaves(schema: &[SchemaElement], limits: Limits) -> Result<Vec<LeafColumn>> {
    let mut leaves = Vec::new();
    // A leaf is a schema element with a physical type and no children.
    for (i, s) in schema.iter().enumerate() {
        let Some(physical) = s.physical else {
            continue;
        };
        if s.num_children != 0 {
            continue;
        }
        if leaves.len() as u64 >= u64::from(limits.max_parquet_columns) {
            return Err(Error::resource_limit(format!(
                "Parquet file exceeds the {}-leaf-column cap",
                limits.max_parquet_columns
            )));
        }
        // Walk up to the root accumulating the path and the def/rep levels.
        let mut path = vec![s.name.clone()];
        let mut max_def = 0u32;
        let mut max_rep = 0u32;
        match s.repetition {
            REP_OPTIONAL => max_def += 1,
            REP_REPEATED => {
                max_def += 1;
                max_rep += 1;
            }
            _ => {}
        }
        let mut cur = s.parent;
        while cur != u32::MAX {
            let p = schema
                .get(cur as usize)
                .ok_or_else(|| corrupt("schema parent link is out of range"))?;
            if p.parent != u32::MAX || cur != 0 {
                path.push(p.name.clone());
            }
            match p.repetition {
                REP_OPTIONAL => max_def += 1,
                REP_REPEATED => {
                    max_def += 1;
                    max_rep += 1;
                }
                _ => {}
            }
            cur = p.parent;
        }
        path.reverse();
        leaves.push(LeafColumn {
            element: i as u32,
            path,
            physical,
            type_length: s.type_length,
            repetition: s.repetition,
            converted: s.converted,
            logical: s.logical,
            max_def,
            max_rep,
        });
    }
    Ok(leaves)
}

fn stats_of(tv: &TV) -> (Option<Vec<u8>>, Option<Vec<u8>>, Option<i64>) {
    let min = tv.field(6).and_then(TV::as_bin).map(<[u8]>::to_vec);
    let max = tv.field(5).and_then(TV::as_bin).map(<[u8]>::to_vec);
    let null_count = tv.field(3).and_then(TV::as_int);
    (min, max, null_count)
}

/// Parse `source` into the canonical derived model.
pub fn parse(source: &[u8], limits: Limits) -> Result<ParquetModel> {
    if source.len() as u64 > limits.max_parquet_document_bytes {
        return Err(Error::resource_limit(format!(
            "Parquet source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_parquet_document_bytes
        )));
    }
    let n = source.len();
    if n < 12 {
        return Err(corrupt("file is shorter than the Parquet minimum"));
    }
    if &source[0..4] != MAGIC || &source[n - 4..n] != MAGIC {
        return Err(corrupt("missing PAR1 magic at the start or end"));
    }
    let footer_len =
        u32::from_le_bytes([source[n - 8], source[n - 7], source[n - 6], source[n - 5]]);
    if u64::from(footer_len) > limits.max_parquet_footer_bytes {
        return Err(Error::resource_limit(format!(
            "Parquet footer is {footer_len} bytes, above the {}-byte footer cap",
            limits.max_parquet_footer_bytes
        )));
    }
    let footer_start = n
        .checked_sub(8 + footer_len as usize)
        .filter(|s| *s >= 4)
        .ok_or_else(|| corrupt("footer length is inconsistent with the file length"))?;
    let footer = &source[footer_start..n - 8];
    let meta = parse_footer(footer)?;

    let version = meta.field(1).and_then(TV::as_int).unwrap_or(0) as i32;
    let num_rows = meta
        .field(3)
        .and_then(TV::as_int)
        .ok_or_else(|| corrupt("footer has no num_rows"))?;
    let created_by = meta
        .field(6)
        .and_then(TV::as_bin)
        .map(|b| String::from_utf8_lossy(b).into_owned());

    // Schema.
    let schema_tv = meta
        .field(2)
        .and_then(TV::as_list)
        .ok_or_else(|| corrupt("footer has no schema"))?;
    let mut raw = Vec::with_capacity(schema_tv.len());
    for s in schema_tv {
        raw.push(schema_element(s)?);
    }
    if raw.is_empty() {
        return Err(corrupt("schema is empty"));
    }
    let mut schema = Vec::with_capacity(raw.len());
    let mut cursor = 0usize;
    flatten_schema(&raw, &mut schema, &mut cursor, 0, u32::MAX, limits)?;
    if cursor != raw.len() {
        return Err(corrupt("schema tree does not consume every element"));
    }
    let leaves = build_leaves(&schema, limits)?;

    // Row groups.
    let rg_tv = meta
        .field(4)
        .and_then(TV::as_list)
        .ok_or_else(|| corrupt("footer has no row_groups"))?;
    if rg_tv.len() as u64 > u64::from(limits.max_parquet_row_groups) {
        return Err(Error::resource_limit(format!(
            "Parquet file has {} row groups, above the {}-row-group cap",
            rg_tv.len(),
            limits.max_parquet_row_groups
        )));
    }
    let mut row_groups = Vec::with_capacity(rg_tv.len());
    for rg in rg_tv {
        let rg_rows = rg.field(3).and_then(TV::as_int).unwrap_or(0);
        let rg_bytes = rg.field(2).and_then(TV::as_int).unwrap_or(0);
        let cols = rg
            .field(1)
            .and_then(TV::as_list)
            .ok_or_else(|| corrupt("row group has no columns"))?;
        let mut chunks = Vec::with_capacity(cols.len());
        for cc in cols {
            let file_offset = cc.field(2).and_then(TV::as_int).unwrap_or(0);
            let md = cc
                .field(3)
                .ok_or_else(|| corrupt("column chunk has no metadata"))?;
            let codec = md.field(4).and_then(TV::as_int).unwrap_or(0) as i32;
            let num_values = md.field(5).and_then(TV::as_int).unwrap_or(0);
            let total_uncompressed_size = md.field(6).and_then(TV::as_int).unwrap_or(0);
            let total_compressed_size = md.field(7).and_then(TV::as_int).unwrap_or(0);
            let data_page_offset = md.field(9).and_then(TV::as_int).unwrap_or(0);
            let dict = md.field(11).and_then(TV::as_int);
            let encodings = md
                .field(2)
                .and_then(TV::as_list)
                .map(|l| l.iter().filter_map(TV::as_int).map(|n| n as i32).collect())
                .unwrap_or_default();
            let (min_value, max_value, null_count) =
                md.field(12).map(stats_of).unwrap_or((None, None, None));
            let leaf = chunks.len() as u32;
            // The exact source span: from the earliest page offset (dictionary
            // page first, if any) across `total_compressed_size` bytes.
            let mut start = data_page_offset;
            if let Some(d) = dict
                && d > 0
                && d < start
            {
                start = d;
            }
            let span_start =
                u64::try_from(start).map_err(|_| corrupt("column chunk offset is negative"))?;
            let span_end_i = start
                .checked_add(total_compressed_size)
                .ok_or_else(|| corrupt("column chunk span overflows"))?;
            let span_end =
                u64::try_from(span_end_i).map_err(|_| corrupt("column chunk end is negative"))?;
            if span_start < 4 || span_end > n as u64 || span_start > span_end {
                return Err(corrupt("column chunk span is outside the file"));
            }
            chunks.push(ColumnChunkDesc {
                leaf,
                codec,
                num_values,
                total_compressed_size,
                total_uncompressed_size,
                data_page_offset,
                dictionary_page_offset: dict,
                file_offset,
                encodings,
                span_start,
                span_end,
                min_value,
                max_value,
                null_count,
            });
        }
        row_groups.push(RowGroupDesc {
            num_rows: rg_rows,
            total_byte_size: rg_bytes,
            chunks,
        });
    }

    Ok(ParquetModel {
        doc_len: n as u64,
        version,
        num_rows,
        created_by,
        schema,
        leaves,
        row_groups,
    })
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of a `ParquetModel` node).
pub fn build_parquet_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits)?.encode())
}

// ---------------------------------------------------------------------------
// Value decoding
// ---------------------------------------------------------------------------

/// A decoded leaf value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A null (a definition level below the maximum).
    Null,
    /// A boolean.
    Bool(bool),
    /// A 32-bit integer.
    I32(i32),
    /// A 64-bit integer.
    I64(i64),
    /// A 32-bit float.
    F32(f32),
    /// A 64-bit float.
    F64(f64),
    /// An opaque byte string (`BYTE_ARRAY`/`FIXED_LEN_BYTE_ARRAY`).
    Bytes(Vec<u8>),
}

/// Render a decoded value as text, honouring the column's logical type.
pub fn value_text(v: &Value, leaf: &LeafColumn) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        Value::I32(n) => n.to_string(),
        Value::I64(n) => n.to_string(),
        Value::F32(f) => format_f32(*f),
        Value::F64(f) => format_f64(*f),
        Value::Bytes(b) => {
            if is_string_like(leaf) {
                String::from_utf8_lossy(b).into_owned()
            } else {
                let mut s = String::with_capacity(2 + b.len() * 2);
                s.push_str("0x");
                for byte in b {
                    s.push(char::from_digit(u32::from(byte >> 4), 16).unwrap_or('0'));
                    s.push(char::from_digit(u32::from(byte & 0x0f), 16).unwrap_or('0'));
                }
                s
            }
        }
    }
}

fn format_f32(f: f32) -> String {
    if f == f.trunc() && f.is_finite() && f.abs() < 1e15 {
        format!("{}", f as i64)
    } else {
        format!("{f}")
    }
}

fn format_f64(f: f64) -> String {
    if f == f.trunc() && f.is_finite() && f.abs() < 1e15 {
        format!("{}", f as i64)
    } else {
        format!("{f}")
    }
}

struct DecodeBudget {
    values_left: u64,
    bytes_left: u64,
}

impl DecodeBudget {
    fn add_values(&mut self, n: u64) -> Result<()> {
        if n > self.values_left {
            return Err(Error::resource_limit(
                "Parquet decode exceeded the value cap",
            ));
        }
        self.values_left -= n;
        Ok(())
    }

    fn add_bytes(&mut self, n: u64) -> Result<()> {
        if n > self.bytes_left {
            return Err(Error::resource_limit(
                "Parquet decode exceeded the decompressed-byte cap",
            ));
        }
        self.bytes_left -= n;
        Ok(())
    }
}

fn supported_physical(t: i32) -> bool {
    matches!(
        t,
        T_BOOLEAN | T_INT32 | T_INT64 | T_FLOAT | T_DOUBLE | T_BYTE_ARRAY | T_FIXED_LEN_BYTE_ARRAY
    )
}

fn supported_codec(c: i32) -> bool {
    c == CODEC_UNCOMPRESSED || (c == CODEC_GZIP && cfg!(feature = "field"))
}

/// Decode one leaf column's values across every row group, in row order.
///
/// Declines typed when the column (or any of its chunks) uses an unsupported
/// repetition, physical type, encoding, codec, or page layout.
pub fn leaf_values(
    source: &[u8],
    model: &ParquetModel,
    leaf_index: u32,
    limits: Limits,
) -> Result<Vec<Value>> {
    let leaf = model
        .leaf(leaf_index)
        .ok_or_else(|| Error::unsupported_feature(format!("no Parquet column {leaf_index}")))?;
    if leaf.max_rep > 0 {
        return Err(Error::unsupported_feature(
            "Parquet repeated (list) columns are not supported",
        ));
    }
    if leaf.max_def > 1 {
        return Err(Error::unsupported_feature(
            "Parquet nested optional columns are not supported",
        ));
    }
    if !supported_physical(leaf.physical) {
        return Err(Error::unsupported_feature(format!(
            "Parquet physical type {} is not supported",
            physical_type_name(leaf.physical)
        )));
    }
    let mut budget = DecodeBudget {
        values_left: limits.max_parquet_values,
        bytes_left: limits.max_parquet_decompressed_bytes,
    };
    let mut out: Vec<Value> = Vec::new();
    for rg in &model.row_groups {
        let chunk = rg
            .chunks
            .iter()
            .find(|c| c.leaf == leaf_index)
            .ok_or_else(|| corrupt("row group is missing a column chunk for a leaf"))?;
        decode_chunk(source, leaf, chunk, limits, &mut budget, &mut out)?;
    }
    Ok(out)
}

/// Decode a single cell (row within the whole file, leaf column) by decoding the
/// containing row group's column chunk only.
pub fn cell_value(
    source: &[u8],
    model: &ParquetModel,
    row: u64,
    leaf_index: u32,
    limits: Limits,
) -> Result<Value> {
    let leaf = model
        .leaf(leaf_index)
        .ok_or_else(|| Error::unsupported_feature(format!("no Parquet column {leaf_index}")))?;
    let mut base: u64 = 0;
    for rg in &model.row_groups {
        let rows = u64::try_from(rg.num_rows).unwrap_or(0);
        if row < base.saturating_add(rows) {
            let chunk = rg
                .chunks
                .iter()
                .find(|c| c.leaf == leaf_index)
                .ok_or_else(|| corrupt("row group is missing a column chunk for a leaf"))?;
            let mut budget = DecodeBudget {
                values_left: limits.max_parquet_values,
                bytes_left: limits.max_parquet_decompressed_bytes,
            };
            let mut vals: Vec<Value> = Vec::new();
            decode_chunk(source, leaf, chunk, limits, &mut budget, &mut vals)?;
            let local = usize::try_from(row - base)
                .map_err(|_| Error::unsupported_feature("Parquet row index is out of range"))?;
            return vals
                .get(local)
                .cloned()
                .ok_or_else(|| Error::unsupported_feature("Parquet row index is out of range"));
        }
        base = base.saturating_add(rows);
    }
    Err(Error::unsupported_feature(format!(
        "Parquet row {row} is out of range (row count {})",
        model.num_rows
    )))
}

struct PageHeader {
    page_type: i32,
    uncompressed_page_size: u64,
    compressed_page_size: u64,
    data_num_values: u64,
    data_encoding: i32,
    data_def_encoding: i32,
    data_rep_encoding: i32,
    dict_num_values: u64,
    dict_encoding: i32,
}

fn parse_page_header(source: &[u8], pos: usize) -> Result<(PageHeader, usize)> {
    let rest = source
        .get(pos..)
        .ok_or_else(|| corrupt("page header is outside the file"))?;
    // A page header is a bare Thrift-Compact struct; its serialized length is the
    // number of bytes the reader advanced.
    let mut buf = TBuf::new(rest);
    let tv = buf.read_struct(0)?;
    let header_len = buf.at;
    let g = |id: i16| tv.field(id).and_then(TV::as_int);
    let page_type = g(1).unwrap_or(-1) as i32;
    let uncompressed = g(2).unwrap_or(-1);
    let compressed = g(3).unwrap_or(-1);
    if uncompressed < 0 || compressed < 0 {
        return Err(corrupt("page header has a negative size"));
    }
    let dh = tv.field(5);
    let dph = tv.field(7);
    let header = PageHeader {
        page_type,
        uncompressed_page_size: uncompressed as u64,
        compressed_page_size: compressed as u64,
        data_num_values: dh
            .and_then(|d| d.field(1))
            .and_then(TV::as_int)
            .unwrap_or(0) as u64,
        data_encoding: dh
            .and_then(|d| d.field(2))
            .and_then(TV::as_int)
            .unwrap_or(-1) as i32,
        data_def_encoding: dh
            .and_then(|d| d.field(3))
            .and_then(TV::as_int)
            .unwrap_or(ENC_RLE as i64) as i32,
        data_rep_encoding: dh
            .and_then(|d| d.field(4))
            .and_then(TV::as_int)
            .unwrap_or(ENC_RLE as i64) as i32,
        dict_num_values: dph
            .and_then(|d| d.field(1))
            .and_then(TV::as_int)
            .unwrap_or(0) as u64,
        dict_encoding: dph
            .and_then(|d| d.field(2))
            .and_then(TV::as_int)
            .unwrap_or(-1) as i32,
    };
    Ok((header, header_len))
}

fn decompress_page(
    codec: i32,
    payload: &[u8],
    uncompressed_size: u64,
    limits: Limits,
    budget: &mut DecodeBudget,
) -> Result<Vec<u8>> {
    budget.add_bytes(uncompressed_size)?;
    match codec {
        CODEC_UNCOMPRESSED => {
            if payload.len() as u64 != uncompressed_size {
                return Err(corrupt("uncompressed page size disagrees with the payload"));
            }
            Ok(payload.to_vec())
        }
        CODEC_GZIP => {
            if !cfg!(feature = "field") {
                return Err(Error::unsupported_feature(
                    "Parquet GZIP pages require the `field` inflate seam",
                ));
            }
            #[cfg(feature = "field")]
            {
                let raw = gzip_body(payload)?;
                let out = crate::field::derive::inflate_raw_deflate(raw, uncompressed_size, limits)
                    .map_err(|_| corrupt("Parquet GZIP page failed to inflate"))?;
                if out.len() as u64 != uncompressed_size {
                    return Err(corrupt(
                        "GZIP page decoded length disagrees with the header",
                    ));
                }
                Ok(out)
            }
            #[cfg(not(feature = "field"))]
            {
                unreachable!()
            }
        }
        other => Err(Error::unsupported_feature(format!(
            "Parquet compression codec {} is not supported",
            codec_name(other)
        ))),
    }
}

/// Strip an RFC 1952 GZIP wrapper, returning the raw DEFLATE body.
fn gzip_body(gz: &[u8]) -> Result<&[u8]> {
    if gz.len() < 18 || gz[0] != 0x1f || gz[1] != 0x8b || gz[2] != 8 {
        return Err(corrupt("not a GZIP member"));
    }
    let flg = gz[3];
    let mut i = 10usize;
    if flg & 0x04 != 0 {
        let xlen = usize::from(gz.get(i).copied().unwrap_or(0))
            | (usize::from(gz.get(i + 1).copied().unwrap_or(0)) << 8);
        i = i
            .checked_add(2 + xlen)
            .ok_or_else(|| corrupt("GZIP extra field overflows"))?;
    }
    if flg & 0x08 != 0 {
        while i < gz.len() && gz[i] != 0 {
            i += 1;
        }
        i += 1;
    }
    if flg & 0x10 != 0 {
        while i < gz.len() && gz[i] != 0 {
            i += 1;
        }
        i += 1;
    }
    if flg & 0x02 != 0 {
        i += 2;
    }
    // The last 8 bytes are CRC-32 + ISIZE.
    if i + 8 > gz.len() {
        return Err(corrupt("GZIP member is truncated"));
    }
    Ok(&gz[i..gz.len() - 8])
}

fn decode_chunk(
    source: &[u8],
    leaf: &LeafColumn,
    chunk: &ColumnChunkDesc,
    limits: Limits,
    budget: &mut DecodeBudget,
    out: &mut Vec<Value>,
) -> Result<()> {
    if !supported_codec(chunk.codec) {
        return Err(Error::unsupported_feature(format!(
            "Parquet codec {} is not supported",
            codec_name(chunk.codec)
        )));
    }
    if chunk.num_values < 0 {
        return Err(corrupt("column chunk has a negative value count"));
    }
    let want = chunk.num_values as u64;
    budget.add_values(want)?;
    let start = usize::try_from(chunk.span_start).map_err(|_| corrupt("span overflow"))?;
    let end = usize::try_from(chunk.span_end).map_err(|_| corrupt("span overflow"))?;
    if end > source.len() || start > end {
        return Err(corrupt("column chunk span is outside the file"));
    }
    let mut pos = start;
    let mut produced: u64 = 0;
    let mut pages: u32 = 0;
    let mut dict: Option<Vec<Value>> = None;
    while produced < want {
        if pages >= limits.max_parquet_pages_per_chunk {
            return Err(Error::resource_limit(format!(
                "column chunk exceeds the {}-page cap",
                limits.max_parquet_pages_per_chunk
            )));
        }
        if pos >= end {
            return Err(corrupt("column chunk ended before all values were read"));
        }
        let (header, hlen) = parse_page_header(source, pos)?;
        let data_start = pos
            .checked_add(hlen)
            .ok_or_else(|| corrupt("page header overflows"))?;
        let data_end = data_start
            .checked_add(
                usize::try_from(header.compressed_page_size)
                    .map_err(|_| corrupt("page size overflows"))?,
            )
            .ok_or_else(|| corrupt("page size overflows"))?;
        if data_end > end || data_end > source.len() {
            return Err(corrupt("page data is outside the column chunk"));
        }
        let payload = &source[data_start..data_end];
        pages += 1;
        match header.page_type {
            PAGE_DICTIONARY => {
                let decoded = decompress_page(
                    chunk.codec,
                    payload,
                    header.uncompressed_page_size,
                    limits,
                    budget,
                )?;
                if header.dict_encoding != ENC_PLAIN {
                    return Err(Error::unsupported_feature(format!(
                        "Parquet dictionary encoding {} is not supported",
                        encoding_name(header.dict_encoding)
                    )));
                }
                dict = Some(decode_plain(
                    &decoded,
                    leaf,
                    header.dict_num_values as usize,
                )?);
            }
            PAGE_DATA => {
                let decoded = decompress_page(
                    chunk.codec,
                    payload,
                    header.uncompressed_page_size,
                    limits,
                    budget,
                )?;
                decode_data_page(leaf, &header, &decoded, &dict, out)?;
                produced = produced.saturating_add(header.data_num_values);
            }
            PAGE_INDEX => {}
            PAGE_DATA_V2 => {
                return Err(Error::unsupported_feature(
                    "Parquet DATA_PAGE_V2 pages are not supported",
                ));
            }
            other => {
                return Err(Error::unsupported_feature(format!(
                    "Parquet page type {} is not supported",
                    page_type_name(other)
                )));
            }
        }
        pos = data_end;
    }
    if produced != want {
        return Err(corrupt("column chunk value count disagrees with its pages"));
    }
    Ok(())
}

fn bit_width(max: u32) -> u32 {
    if max == 0 {
        0
    } else {
        32 - max.leading_zeros()
    }
}

fn decode_data_page(
    leaf: &LeafColumn,
    header: &PageHeader,
    decoded: &[u8],
    dict: &Option<Vec<Value>>,
    out: &mut Vec<Value>,
) -> Result<()> {
    let n = header.data_num_values as usize;
    let mut at = 0usize;
    if leaf.max_rep > 0 {
        // Repetition levels precede definition levels.
        let (_, used) = decode_levels(
            &decoded[at..],
            bit_width(leaf.max_rep),
            n,
            header.data_rep_encoding,
        )?;
        at += used;
    }
    let mut defs: Option<Vec<u32>> = None;
    if leaf.max_def > 0 {
        let (levels, used) = decode_levels(
            &decoded[at..],
            bit_width(leaf.max_def),
            n,
            header.data_def_encoding,
        )?;
        at += used;
        defs = Some(levels);
    }
    let present = match &defs {
        Some(d) => d.iter().filter(|v| **v == leaf.max_def).count(),
        None => n,
    };
    let value_bytes = decoded
        .get(at..)
        .ok_or_else(|| corrupt("data page value region is truncated"))?;
    let values: Vec<Value> = match header.data_encoding {
        ENC_PLAIN => decode_plain(value_bytes, leaf, present)?,
        ENC_PLAIN_DICTIONARY | ENC_RLE_DICTIONARY => {
            let dictionary = dict
                .as_ref()
                .ok_or_else(|| corrupt("dictionary-encoded data page has no dictionary page"))?;
            decode_dictionary_indices(value_bytes, dictionary, present)?
        }
        other => {
            return Err(Error::unsupported_feature(format!(
                "Parquet data encoding {} is not supported",
                encoding_name(other)
            )));
        }
    };
    // Rebuild the row-aligned value list (interleaving nulls).
    match defs {
        Some(defs) => {
            let mut vi = 0usize;
            for d in defs {
                if d == leaf.max_def {
                    out.push(values.get(vi).cloned().ok_or_else(|| {
                        corrupt("data page has fewer values than definition levels")
                    })?);
                    vi += 1;
                } else {
                    out.push(Value::Null);
                }
            }
        }
        None => out.extend(values),
    }
    Ok(())
}

/// Decode a run of definition/repetition levels, returning the levels and the
/// number of bytes consumed.
fn decode_levels(
    data: &[u8],
    width: u32,
    count: usize,
    encoding: i32,
) -> Result<(Vec<u32>, usize)> {
    match encoding {
        ENC_RLE => {
            // A 4-byte little-endian length prefix precedes the RLE/bit-packed run.
            if data.len() < 4 {
                return Err(corrupt("level stream is truncated"));
            }
            let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
            if 4 + len > data.len() {
                return Err(corrupt("level stream length overruns the page"));
            }
            let levels = decode_rle_bitpacked(&data[4..4 + len], width, count)?;
            Ok((levels, 4 + len))
        }
        ENC_BIT_PACKED => Err(Error::unsupported_feature(
            "Parquet deprecated BIT_PACKED level encoding is not supported",
        )),
        other => Err(Error::unsupported_feature(format!(
            "Parquet level encoding {} is not supported",
            encoding_name(other)
        ))),
    }
}

/// Decode dictionary indices: a leading bit-width byte then an RLE/bit-packed run.
fn decode_dictionary_indices(data: &[u8], dict: &[Value], count: usize) -> Result<Vec<Value>> {
    if data.is_empty() {
        return Err(corrupt("dictionary index stream is empty"));
    }
    let width = u32::from(data[0]);
    if width > 32 {
        return Err(corrupt("dictionary index bit width is out of range"));
    }
    let idx = decode_rle_bitpacked(&data[1..], width, count)?;
    let mut out = Vec::with_capacity(idx.len());
    for i in idx {
        let v = dict
            .get(i as usize)
            .ok_or_else(|| corrupt("dictionary index is out of range"))?;
        out.push(v.clone());
    }
    Ok(out)
}

/// Decode an RLE / bit-packed hybrid run, returning exactly `count` values
/// (clamped to the available entries). Bits are read least-significant-bit first
/// within each byte.
fn decode_rle_bitpacked(data: &[u8], width: u32, count: usize) -> Result<Vec<u32>> {
    let mut out: Vec<u32> = Vec::with_capacity(count);
    if width == 0 {
        out.resize(count, 0);
        return Ok(out);
    }
    let mut pos = 0usize;
    while out.len() < count {
        let header = read_uvarint(data, &mut pos)?;
        if header & 1 == 0 {
            let run = (header >> 1) as usize;
            let bytew = width.div_ceil(8) as usize;
            let val = read_le_uint(data, &mut pos, bytew)? as u32;
            for _ in 0..run {
                if out.len() >= count {
                    break;
                }
                out.push(val);
            }
        } else {
            let groups = (header >> 1) as usize;
            let total = groups.saturating_mul(8);
            let nbytes = groups.saturating_mul(width as usize);
            let start = pos;
            let end = start
                .checked_add(nbytes)
                .filter(|e| *e <= data.len())
                .ok_or_else(|| corrupt("bit-packed run overruns the page"))?;
            let mut bitpos = 0usize;
            for _ in 0..total {
                if out.len() >= count {
                    break;
                }
                let mut v: u32 = 0;
                for k in 0..width {
                    let bp = bitpos + k as usize;
                    let byte = data[start + bp / 8];
                    let bit = (byte >> (bp % 8)) & 1;
                    v |= u32::from(bit) << k;
                }
                bitpos += width as usize;
                out.push(v);
            }
            pos = end;
        }
    }
    out.truncate(count);
    Ok(out)
}

fn read_uvarint(data: &[u8], pos: &mut usize) -> Result<u64> {
    let mut acc: u64 = 0;
    let mut shift: u32 = 0;
    loop {
        let b = *data
            .get(*pos)
            .ok_or_else(|| corrupt("run header is truncated"))?;
        *pos += 1;
        if shift >= 64 {
            return Err(corrupt("run header varint overflow"));
        }
        acc |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Ok(acc);
        }
        shift += 7;
    }
}

fn read_le_uint(data: &[u8], pos: &mut usize, n: usize) -> Result<u64> {
    let end = pos
        .checked_add(n)
        .filter(|e| *e <= data.len())
        .ok_or_else(|| corrupt("fixed value is truncated"))?;
    let mut v: u64 = 0;
    for i in 0..n {
        v |= u64::from(data[*pos + i]) << (8 * i);
    }
    *pos = end;
    Ok(v)
}

fn decode_plain(data: &[u8], leaf: &LeafColumn, count: usize) -> Result<Vec<Value>> {
    let mut out = Vec::with_capacity(count);
    match leaf.physical {
        T_BOOLEAN => {
            let need = count.div_ceil(8);
            if data.len() < need {
                return Err(corrupt("boolean page is truncated"));
            }
            for i in 0..count {
                let bit = (data[i / 8] >> (i % 8)) & 1;
                out.push(Value::Bool(bit != 0));
            }
        }
        T_INT32 => {
            let mut at = 0usize;
            for _ in 0..count {
                let v = read_le_uint(data, &mut at, 4)? as u32 as i32;
                out.push(Value::I32(v));
            }
        }
        T_INT64 => {
            let mut at = 0usize;
            for _ in 0..count {
                let v = read_le_uint(data, &mut at, 8)? as i64;
                out.push(Value::I64(v));
            }
        }
        T_FLOAT => {
            let mut at = 0usize;
            for _ in 0..count {
                let v = read_le_uint(data, &mut at, 4)? as u32;
                out.push(Value::F32(f32::from_bits(v)));
            }
        }
        T_DOUBLE => {
            let mut at = 0usize;
            for _ in 0..count {
                let v = read_le_uint(data, &mut at, 8)?;
                out.push(Value::F64(f64::from_bits(v)));
            }
        }
        T_BYTE_ARRAY => {
            let mut at = 0usize;
            for _ in 0..count {
                let len = read_le_uint(data, &mut at, 4)? as usize;
                let end = at
                    .checked_add(len)
                    .filter(|e| *e <= data.len())
                    .ok_or_else(|| corrupt("byte array length overruns the page"))?;
                out.push(Value::Bytes(data[at..end].to_vec()));
                at = end;
            }
        }
        T_FIXED_LEN_BYTE_ARRAY => {
            let len = usize::try_from(
                leaf.type_length
                    .filter(|n| *n > 0)
                    .ok_or_else(|| corrupt("fixed-length column has no type length"))?,
            )
            .map_err(|_| corrupt("fixed-length type length overflows"))?;
            let mut at = 0usize;
            for _ in 0..count {
                let end = at
                    .checked_add(len)
                    .filter(|e| *e <= data.len())
                    .ok_or_else(|| corrupt("fixed-length value overruns the page"))?;
                out.push(Value::Bytes(data[at..end].to_vec()));
                at = end;
            }
        }
        other => {
            return Err(Error::unsupported_feature(format!(
                "Parquet physical type {} is not supported",
                physical_type_name(other)
            )));
        }
    }
    Ok(out)
}

/// Decode a statistics value (a plain-encoded scalar) for display.
pub fn stats_value_text(raw: &[u8], leaf: &LeafColumn) -> Option<String> {
    let values = decode_plain(raw, leaf, 1).ok()?;
    values.first().map(|v| value_text(v, leaf))
}

// ---------------------------------------------------------------------------
// Model binary helpers
// ---------------------------------------------------------------------------

fn enc_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn enc_opt_str(out: &mut Vec<u8>, s: Option<&str>) {
    match s {
        Some(s) => {
            out.push(1);
            enc_str(out, s);
        }
        None => out.push(0),
    }
}

fn enc_opt_i32(out: &mut Vec<u8>, v: Option<i32>) {
    match v {
        Some(n) => {
            out.push(1);
            out.extend_from_slice(&n.to_le_bytes());
        }
        None => out.push(0),
    }
}

fn enc_opt_i64(out: &mut Vec<u8>, v: Option<i64>) {
    match v {
        Some(n) => {
            out.push(1);
            out.extend_from_slice(&n.to_le_bytes());
        }
        None => out.push(0),
    }
}

fn enc_opt_bytes(out: &mut Vec<u8>, v: Option<&[u8]>) {
    match v {
        Some(b) => {
            out.push(1);
            out.extend_from_slice(&(b.len() as u32).to_le_bytes());
            out.extend_from_slice(b);
        }
        None => out.push(0),
    }
}

struct BinReader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> BinReader<'a> {
    fn new(b: &'a [u8]) -> Self {
        BinReader { b, at: 0 }
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| corrupt("reader overflow"))?;
        let s = self
            .b
            .get(self.at..end)
            .ok_or_else(|| corrupt("reader underrun"))?;
        self.at = end;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    fn u32(&mut self) -> Result<u32> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }

    fn u64(&mut self) -> Result<u64> {
        let b = self.bytes(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    fn i64(&mut self) -> Result<i64> {
        Ok(self.u64()? as i64)
    }

    fn string(&mut self) -> Result<String> {
        let n = self.u32()? as usize;
        let bytes = self.bytes(n)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| corrupt("model string is not UTF-8"))
    }

    fn opt_str(&mut self) -> Result<Option<String>> {
        if self.u8()? == 0 {
            Ok(None)
        } else {
            Ok(Some(self.string()?))
        }
    }

    fn opt_i32(&mut self) -> Result<Option<i32>> {
        if self.u8()? == 0 {
            Ok(None)
        } else {
            Ok(Some(self.i32()?))
        }
    }

    fn opt_i64(&mut self) -> Result<Option<i64>> {
        if self.u8()? == 0 {
            Ok(None)
        } else {
            Ok(Some(self.i64()?))
        }
    }

    fn opt_bytes(&mut self) -> Result<Option<Vec<u8>>> {
        if self.u8()? == 0 {
            Ok(None)
        } else {
            let n = self.u32()? as usize;
            Ok(Some(self.bytes(n)?.to_vec()))
        }
    }

    fn at_end(&self) -> bool {
        self.at == self.b.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(physical: i32) -> LeafColumn {
        LeafColumn {
            element: 0,
            path: vec!["c".to_string()],
            physical,
            type_length: None,
            repetition: REP_REQUIRED,
            converted: None,
            logical: None,
            max_def: 0,
            max_rep: 0,
        }
    }

    #[test]
    fn plain_roundtrips_numeric_types() {
        let mut i32s = Vec::new();
        for n in [1i32, -2, 300, i32::MIN, i32::MAX] {
            i32s.extend_from_slice(&n.to_le_bytes());
        }
        let got = decode_plain(&i32s, &leaf(T_INT32), 5).unwrap();
        assert_eq!(
            got,
            vec![
                Value::I32(1),
                Value::I32(-2),
                Value::I32(300),
                Value::I32(i32::MIN),
                Value::I32(i32::MAX)
            ]
        );

        let mut f64s = Vec::new();
        for f in [1.5f64, -2.25, 0.0] {
            f64s.extend_from_slice(&f.to_le_bytes());
        }
        let got = decode_plain(&f64s, &leaf(T_DOUBLE), 3).unwrap();
        assert_eq!(
            got,
            vec![Value::F64(1.5), Value::F64(-2.25), Value::F64(0.0)]
        );
    }

    #[test]
    fn plain_boolean_is_lsb_first() {
        // 0b0000_0101 -> true, false, true, false.
        let got = decode_plain(&[0b0000_0101], &leaf(T_BOOLEAN), 4).unwrap();
        assert_eq!(
            got,
            vec![
                Value::Bool(true),
                Value::Bool(false),
                Value::Bool(true),
                Value::Bool(false)
            ]
        );
    }

    #[test]
    fn rle_runs_and_bitpacked_runs_decode() {
        // An RLE run of three 5s (width 3): header = 3<<1 = 6, value byte 5.
        let rle = decode_rle_bitpacked(&[6, 5], 3, 3).unwrap();
        assert_eq!(rle, vec![5, 5, 5]);
        // A bit-packed run of eight 1-bit values (0b10101010 read LSB-first is
        // 0,1,0,1,0,1,0,1).
        let bp = decode_rle_bitpacked(&[0b0000_0011, 0b1010_1010], 1, 8).unwrap();
        assert_eq!(bp, vec![0, 1, 0, 1, 0, 1, 0, 1]);
    }

    #[test]
    fn gzip_body_strips_the_wrapper() {
        // A minimal gzip member with no optional fields: 10-byte header + body +
        // 8-byte trailer.
        let mut gz = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 255];
        gz.extend_from_slice(&[0x01, 0x02, 0x03]);
        gz.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(gzip_body(&gz).unwrap(), &[0x01, 0x02, 0x03]);
    }

    #[test]
    fn detect_requires_consistent_footer() {
        let mut good = b"PAR1".to_vec();
        good.extend_from_slice(b"data");
        good.extend_from_slice(&[0u8; 4]); // a 4-byte footer
        good.extend_from_slice(&4u32.to_le_bytes());
        good.extend_from_slice(b"PAR1");
        assert!(detect(&good, Limits::DEFAULT));
        // Truncated footer length is inconsistent.
        let mut bad = good.clone();
        let n = bad.len();
        bad[n - 8..n - 4].copy_from_slice(&999u32.to_le_bytes());
        assert!(!detect(&bad, Limits::DEFAULT));
        assert!(!detect(b"PAR1notreallyPAR1", Limits::DEFAULT));
    }
}
