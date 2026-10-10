//! Bounded, representation-preserving Apache Arrow IPC adapter (Phase 21.16).
//!
//! Apache Arrow IPC is an **analytical** Wave-2 format (the last listed): a
//! columnar container whose metadata is serialized with Google Flatbuffers and
//! whose data is a flat sequence of 8-byte-aligned buffers. VOLE does **not**
//! re-implement a columnar engine; it treats an Arrow file/stream as one more
//! document whose exact leaf is the **whole source** (a `DocumentExact`, RAW-like
//! authority) and whose derived (`Q_gen`) projection reads the real footer, schema
//! message, and record-batch buffers.
//!
//! ## What this adapter is
//!
//! A **bounded, dependency-free** reader of the Arrow IPC physical layout. It
//! parses the Flatbuffers `Footer` (file format) or the leading `Schema` message
//! (stream format), exposes the schema (field names, type tags/parameters,
//! nullability, children), a record-batch inventory with each batch's **exact
//! source span**, and decodes a documented subset of the columnar buffers with
//! checked arithmetic and hard bounds. The metadata is treated as **untrusted**:
//! every Flatbuffers read is bounds-checked and non-panicking.
//!
//! ## Supported / declined (honest scope)
//!
//! * **Formats:** the IPC **file** format (`ARROW1` magic + padded align + embedded
//!   stream with EOS + footer + little-endian int32 footer length + trailing
//!   `ARROW1`) and the IPC **stream** format when the leading `ARROW1` magic is
//!   present (magic + messages + EOS, no footer). A bare stream without the magic
//!   is not detectable and stays `Opaque`.
//! * **Types decoded:** `Int` (8/16/32/64, signed/unsigned), `FloatingPoint`
//!   (half/single/double), `Boolean`, `Date`, `Time`, `Timestamp`, `Duration` (as
//!   raw integers), `Utf8`/`LargeUtf8`, `Binary`/`LargeBinary`, `FixedSizeBinary`,
//!   with their **validity bitmaps**.
//! * **Types declined typed:** `Null`, `Decimal`, `Interval`, every nested type
//!   (`List`/`LargeList`/`FixedSizeList`/`ListView`/`LargeListView`/`Struct`/`Map`/
//!   `Union`/`RunEndEncoded`), the view types (`BinaryView`/`Utf8View`), and
//!   dictionary-encoded fields.
//! * **Compression:** any `BodyCompression` (LZ4_FRAME/ZSTD) is **declined typed**;
//!   only uncompressed bodies are decoded.
//! * **Endianness:** only little-endian bodies are decoded; big-endian is declined
//!   typed.
//!
//! It **never guesses**: a type, layout, or compression it does not implement is a
//! typed [`Error::unsupported_feature`] decline, never a wrong answer and never a
//! panic.
//!
//! ## Bounds and no panics
//!
//! Untrusted input can only ever yield a typed decline: the source length by
//! [`Limits::max_arrow_document_bytes`], the metadata by
//! [`Limits::max_arrow_metadata_bytes`], the message count by
//! [`Limits::max_arrow_messages`], the flattened column count by
//! [`Limits::max_arrow_columns`], the record-batch count by
//! [`Limits::max_arrow_batches`], the total row count by
//! [`Limits::max_arrow_rows`], the per-batch buffer count by
//! [`Limits::max_arrow_buffers`], the decoded value count by
//! [`Limits::max_arrow_values`], and the total declared body bytes by
//! [`Limits::max_arrow_decompressed_bytes`]. All offsets and lengths use checked
//! arithmetic.

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// The Arrow IPC file magic (`ARROW1`).
pub const MAGIC: &[u8; 6] = b"ARROW1";
/// The encapsulated-message continuation indicator (`0xFFFFFFFF`).
pub const CONTINUATION: u32 = 0xFFFF_FFFF;

/// Hard caps on decoded model arena sizes (defend the decoder against a hostile blob).
pub const MAX_MODEL_SCHEMA: u32 = 1 << 22;
/// Maximum decoded model leaf columns.
pub const MAX_MODEL_LEAVES: u32 = 1 << 20;
/// Maximum decoded model record batches.
pub const MAX_MODEL_BATCHES: u32 = 1 << 22;
/// Deepest Flatbuffers schema nesting accepted.
pub const MAX_FB_DEPTH: u32 = 64;

// --- Arrow `Type` union tags (Schema.fbs) -----------------------------------
/// Type: `NONE` (declined).
pub const T_NONE: i32 = 0;
/// Type: `Null` (declined).
pub const T_NULL: i32 = 1;
/// Type: `Int`.
pub const T_INT: i32 = 2;
/// Type: `FloatingPoint`.
pub const T_FLOAT: i32 = 3;
/// Type: `Binary`.
pub const T_BINARY: i32 = 4;
/// Type: `Utf8`.
pub const T_UTF8: i32 = 5;
/// Type: `Bool`.
pub const T_BOOL: i32 = 6;
/// Type: `Decimal` (declined).
pub const T_DECIMAL: i32 = 7;
/// Type: `Date`.
pub const T_DATE: i32 = 8;
/// Type: `Time`.
pub const T_TIME: i32 = 9;
/// Type: `Timestamp`.
pub const T_TIMESTAMP: i32 = 10;
/// Type: `Interval` (declined).
pub const T_INTERVAL: i32 = 11;
/// Type: `List` (declined).
pub const T_LIST: i32 = 12;
/// Type: `Struct_` (declined).
pub const T_STRUCT: i32 = 13;
/// Type: `Union` (declined).
pub const T_UNION: i32 = 14;
/// Type: `FixedSizeBinary`.
pub const T_FIXED_SIZE_BINARY: i32 = 15;
/// Type: `FixedSizeList` (declined).
pub const T_FIXED_SIZE_LIST: i32 = 16;
/// Type: `Map` (declined).
pub const T_MAP: i32 = 17;
/// Type: `Duration`.
pub const T_DURATION: i32 = 18;
/// Type: `LargeBinary`.
pub const T_LARGE_BINARY: i32 = 19;
/// Type: `LargeUtf8`.
pub const T_LARGE_UTF8: i32 = 20;
/// Type: `LargeList` (declined).
pub const T_LARGE_LIST: i32 = 21;
/// Type: `RunEndEncoded` (declined).
pub const T_RUN_END_ENCODED: i32 = 22;
/// Type: `BinaryView` (declined).
pub const T_BINARY_VIEW: i32 = 23;
/// Type: `Utf8View` (declined).
pub const T_UTF8_VIEW: i32 = 24;
/// Type: `ListView` (declined).
pub const T_LIST_VIEW: i32 = 25;
/// Type: `LargeListView` (declined).
pub const T_LARGE_LIST_VIEW: i32 = 26;

// --- Message header union tags (Message.fbs) --------------------------------
/// Message header: `Schema`.
pub const H_SCHEMA: i32 = 1;
/// Message header: `DictionaryBatch` (declined).
pub const H_DICTIONARY_BATCH: i32 = 2;
/// Message header: `RecordBatch`.
pub const H_RECORD_BATCH: i32 = 3;

/// Stable name of an Arrow `Type` union tag.
pub const fn type_name(tag: i32) -> &'static str {
    match tag {
        T_NONE => "NONE",
        T_NULL => "Null",
        T_INT => "Int",
        T_FLOAT => "FloatingPoint",
        T_BINARY => "Binary",
        T_UTF8 => "Utf8",
        T_BOOL => "Bool",
        T_DECIMAL => "Decimal",
        T_DATE => "Date",
        T_TIME => "Time",
        T_TIMESTAMP => "Timestamp",
        T_INTERVAL => "Interval",
        T_LIST => "List",
        T_STRUCT => "Struct",
        T_UNION => "Union",
        T_FIXED_SIZE_BINARY => "FixedSizeBinary",
        T_FIXED_SIZE_LIST => "FixedSizeList",
        T_MAP => "Map",
        T_DURATION => "Duration",
        T_LARGE_BINARY => "LargeBinary",
        T_LARGE_UTF8 => "LargeUtf8",
        T_LARGE_LIST => "LargeList",
        T_RUN_END_ENCODED => "RunEndEncoded",
        T_BINARY_VIEW => "BinaryView",
        T_UTF8_VIEW => "Utf8View",
        T_LIST_VIEW => "ListView",
        T_LARGE_LIST_VIEW => "LargeListView",
        _ => "UNKNOWN",
    }
}

/// Whether a top-level field of this type is a flat, unconditionally decodable
/// primitive/binary leaf (no children, no dictionary).
pub fn is_supported_flat(tag: i32) -> bool {
    matches!(
        tag,
        T_INT
            | T_FLOAT
            | T_BOOL
            | T_DATE
            | T_TIME
            | T_TIMESTAMP
            | T_DURATION
            | T_UTF8
            | T_LARGE_UTF8
            | T_BINARY
            | T_LARGE_BINARY
            | T_FIXED_SIZE_BINARY
    )
}

// ---------------------------------------------------------------------------
// The canonical derived model
// ---------------------------------------------------------------------------

/// One flattened schema field (the tree in pre-order, with parent links).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldNode {
    /// Nesting depth (top-level fields are 0).
    pub depth: u32,
    /// Parent field index (`u32::MAX` for a top-level field).
    pub parent: u32,
    /// The field name exactly as written.
    pub name: String,
    /// Whether the field is declared nullable.
    pub nullable: bool,
    /// The Arrow `Type` union tag.
    pub type_tag: i32,
    /// A canonical human-readable rendering of the type and its parameters.
    pub type_text: String,
    /// The `Int`/`Time`/`FixedSizeBinary` bit/byte width (0 when not applicable).
    pub bit_width: i32,
    /// Whether an `Int` is signed.
    pub int_signed: bool,
    /// The `FloatingPoint` precision (0 `HALF`, 1 `SINGLE`, 2 `DOUBLE`; -1 n/a).
    pub float_precision: i32,
    /// The `Union` mode (0 sparse, 1 dense; -1 n/a).
    pub union_mode: i32,
    /// The dictionary id if this field is dictionary-encoded.
    pub dict_id: Option<i64>,
    /// The declared child count (0 for a leaf).
    pub num_children: u32,
}

/// One decodable top-level column with its pre-computed buffer slots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeafColumn {
    /// The schema (top-level field) index.
    pub element: u32,
    /// The flattened node index within a record batch.
    pub node_index: u32,
    /// The column name.
    pub name: String,
    /// The Arrow `Type` union tag.
    pub type_tag: i32,
    /// Whether the column can be decoded by this build.
    pub supported: bool,
    /// The typed decline reason when `supported` is false.
    pub decline: Option<String>,
    /// The buffer slot of the validity bitmap (-1 if none).
    pub validity_slot: i32,
    /// The buffer slot of the offsets buffer (-1 for fixed-width).
    pub offsets_slot: i32,
    /// The buffer slot of the data buffer (-1 if none).
    pub data_slot: i32,
    /// The fixed-width element size in bytes (0 when not fixed-width).
    pub width: i32,
    /// Whether an `Int` is signed.
    pub int_signed: bool,
    /// The `FloatingPoint` precision (0/1/2; -1 n/a).
    pub float_precision: i32,
    /// Whether the bytes should be rendered as UTF-8 text.
    pub is_utf8: bool,
    /// Whether the offsets buffer holds 64-bit offsets.
    pub large_offsets: bool,
}

/// One record batch's inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchDesc {
    /// The message start offset (the continuation token) in the source.
    pub offset: u64,
    /// The encapsulated metadata size (as read from the message prefix).
    pub meta_len: u64,
    /// The body start offset in the source.
    pub body_offset: u64,
    /// The declared body length.
    pub body_len: u64,
    /// The exact source span `[span_start, span_end)` of the whole message.
    pub span_start: u64,
    /// One past the exact source span.
    pub span_end: u64,
    /// The number of rows in the batch.
    pub num_rows: i64,
    /// The number of flattened field nodes.
    pub num_nodes: u32,
    /// The number of buffers.
    pub num_buffers: u32,
    /// The body compression codec tag (-1 for uncompressed).
    pub compression: i32,
}

/// The canonical derived Arrow model (the materialization of an `ArrowModel`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrowModel {
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// The `MetadataVersion` recorded in the footer/schema.
    pub version: i32,
    /// The endianness recorded in the schema (0 little, 1 big).
    pub endianness: i32,
    /// Whether the source is the stream format (no footer).
    pub stream: bool,
    /// Whether the batch buffer slots are statically indexable.
    pub indexable: bool,
    /// The flattened schema (pre-order).
    pub schema: Vec<FieldNode>,
    /// The decodable top-level columns.
    pub leaves: Vec<LeafColumn>,
    /// The record batches.
    pub batches: Vec<BatchDesc>,
}

impl ArrowModel {
    /// The leaf column at `index`, if present.
    pub fn leaf(&self, index: u32) -> Option<&LeafColumn> {
        self.leaves.get(index as usize)
    }

    /// The field node at `index`, if present.
    pub fn field(&self, index: u32) -> Option<&FieldNode> {
        self.schema.get(index as usize)
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(128 + self.schema.len() * 48);
        out.extend_from_slice(b"ARTM");
        out.push(MODEL_VERSION);
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&self.version.to_le_bytes());
        out.extend_from_slice(&self.endianness.to_le_bytes());
        out.push(u8::from(self.stream));
        out.push(u8::from(self.indexable));
        out.extend_from_slice(&(self.schema.len() as u32).to_le_bytes());
        for f in &self.schema {
            out.extend_from_slice(&f.depth.to_le_bytes());
            out.extend_from_slice(&f.parent.to_le_bytes());
            enc_str(&mut out, &f.name);
            out.push(u8::from(f.nullable));
            out.extend_from_slice(&f.type_tag.to_le_bytes());
            enc_str(&mut out, &f.type_text);
            out.extend_from_slice(&f.bit_width.to_le_bytes());
            out.push(u8::from(f.int_signed));
            out.extend_from_slice(&f.float_precision.to_le_bytes());
            out.extend_from_slice(&f.union_mode.to_le_bytes());
            enc_opt_i64(&mut out, f.dict_id);
            out.extend_from_slice(&f.num_children.to_le_bytes());
        }
        out.extend_from_slice(&(self.leaves.len() as u32).to_le_bytes());
        for l in &self.leaves {
            out.extend_from_slice(&l.element.to_le_bytes());
            out.extend_from_slice(&l.node_index.to_le_bytes());
            enc_str(&mut out, &l.name);
            out.extend_from_slice(&l.type_tag.to_le_bytes());
            out.push(u8::from(l.supported));
            enc_opt_str(&mut out, l.decline.as_deref());
            out.extend_from_slice(&l.validity_slot.to_le_bytes());
            out.extend_from_slice(&l.offsets_slot.to_le_bytes());
            out.extend_from_slice(&l.data_slot.to_le_bytes());
            out.extend_from_slice(&l.width.to_le_bytes());
            out.push(u8::from(l.int_signed));
            out.extend_from_slice(&l.float_precision.to_le_bytes());
            out.push(u8::from(l.is_utf8));
            out.push(u8::from(l.large_offsets));
        }
        out.extend_from_slice(&(self.batches.len() as u32).to_le_bytes());
        for b in &self.batches {
            out.extend_from_slice(&b.offset.to_le_bytes());
            out.extend_from_slice(&b.meta_len.to_le_bytes());
            out.extend_from_slice(&b.body_offset.to_le_bytes());
            out.extend_from_slice(&b.body_len.to_le_bytes());
            out.extend_from_slice(&b.span_start.to_le_bytes());
            out.extend_from_slice(&b.span_end.to_le_bytes());
            out.extend_from_slice(&b.num_rows.to_le_bytes());
            out.extend_from_slice(&b.num_nodes.to_le_bytes());
            out.extend_from_slice(&b.num_buffers.to_le_bytes());
            out.extend_from_slice(&b.compression.to_le_bytes());
        }
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<ArrowModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"ARTM" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let doc_len = r.u64()?;
        let version = r.i32()?;
        let endianness = r.i32()?;
        let stream = r.u8()? != 0;
        let indexable = r.u8()? != 0;
        let sc = r.u32()?;
        if sc > MAX_MODEL_SCHEMA {
            return Err(corrupt("model schema is implausibly large"));
        }
        let mut schema = Vec::with_capacity(sc as usize);
        for _ in 0..sc {
            let depth = r.u32()?;
            let parent = r.u32()?;
            let name = r.string()?;
            let nullable = r.u8()? != 0;
            let type_tag = r.i32()?;
            let type_text = r.string()?;
            let bit_width = r.i32()?;
            let int_signed = r.u8()? != 0;
            let float_precision = r.i32()?;
            let union_mode = r.i32()?;
            let dict_id = r.opt_i64()?;
            let num_children = r.u32()?;
            schema.push(FieldNode {
                depth,
                parent,
                name,
                nullable,
                type_tag,
                type_text,
                bit_width,
                int_signed,
                float_precision,
                union_mode,
                dict_id,
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
            let node_index = r.u32()?;
            let name = r.string()?;
            let type_tag = r.i32()?;
            let supported = r.u8()? != 0;
            let decline = r.opt_str()?;
            let validity_slot = r.i32()?;
            let offsets_slot = r.i32()?;
            let data_slot = r.i32()?;
            let width = r.i32()?;
            let int_signed = r.u8()? != 0;
            let float_precision = r.i32()?;
            let is_utf8 = r.u8()? != 0;
            let large_offsets = r.u8()? != 0;
            leaves.push(LeafColumn {
                element,
                node_index,
                name,
                type_tag,
                supported,
                decline,
                validity_slot,
                offsets_slot,
                data_slot,
                width,
                int_signed,
                float_precision,
                is_utf8,
                large_offsets,
            });
        }
        let bc = r.u32()?;
        if bc > MAX_MODEL_BATCHES {
            return Err(corrupt("model batch count is implausible"));
        }
        let mut batches = Vec::with_capacity(bc as usize);
        for _ in 0..bc {
            let offset = r.u64()?;
            let meta_len = r.u64()?;
            let body_offset = r.u64()?;
            let body_len = r.u64()?;
            let span_start = r.u64()?;
            let span_end = r.u64()?;
            let num_rows = r.i64()?;
            let num_nodes = r.u32()?;
            let num_buffers = r.u32()?;
            let compression = r.i32()?;
            if span_start > span_end || span_end > doc_len {
                return Err(corrupt("model batch span is outside the document"));
            }
            batches.push(BatchDesc {
                offset,
                meta_len,
                body_offset,
                body_len,
                span_start,
                span_end,
                num_rows,
                num_nodes,
                num_buffers,
                compression,
            });
        }
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        Ok(ArrowModel {
            doc_len,
            version,
            endianness,
            stream,
            indexable,
            schema,
            leaves,
            batches,
        })
    }
}

// ---------------------------------------------------------------------------
// Bounded Flatbuffers reader
// ---------------------------------------------------------------------------

fn corrupt(msg: impl Into<String>) -> Error {
    Error::invalid_arrow_structure(msg)
}

fn align8(x: usize) -> Option<usize> {
    x.checked_add(7).map(|v| v & !7)
}

/// A bounds-checked, non-panicking Flatbuffers reader over one metadata buffer.
struct Fb<'a> {
    b: &'a [u8],
}

impl<'a> Fb<'a> {
    fn new(b: &'a [u8]) -> Self {
        Fb { b }
    }

    fn get(&self, off: usize, n: usize) -> Result<&'a [u8]> {
        let end = off
            .checked_add(n)
            .ok_or_else(|| corrupt("flatbuffer read overflows"))?;
        self.b
            .get(off..end)
            .ok_or_else(|| corrupt("flatbuffer read is out of bounds"))
    }

    fn u8(&self, off: usize) -> Result<u8> {
        Ok(self.get(off, 1)?[0])
    }
    fn u16(&self, off: usize) -> Result<u16> {
        let b = self.get(off, 2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn i16(&self, off: usize) -> Result<i16> {
        Ok(self.u16(off)? as i16)
    }
    fn u32(&self, off: usize) -> Result<u32> {
        let b = self.get(off, 4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn i32(&self, off: usize) -> Result<i32> {
        Ok(self.u32(off)? as i32)
    }
    fn u64(&self, off: usize) -> Result<u64> {
        let b = self.get(off, 8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }
    fn i64(&self, off: usize) -> Result<i64> {
        Ok(self.u64(off)? as i64)
    }

    /// The root table position.
    fn root(&self) -> Result<u32> {
        let t = self.u32(0)? as usize;
        if t == 0 || t >= self.b.len() {
            return Err(corrupt("flatbuffer root is out of bounds"));
        }
        Ok(t as u32)
    }

    /// The `(vtable position, vtable size)` of a table.
    fn vtable(&self, table: u32) -> Result<(usize, usize)> {
        let t = table as usize;
        let soff = self.i32(t)? as i64;
        let vt = (t as i64) - soff;
        if vt < 0 || vt as usize + 4 > self.b.len() {
            return Err(corrupt("flatbuffer vtable is out of bounds"));
        }
        let vt = vt as usize;
        let vtsize = self.u16(vt)? as usize;
        if vtsize < 4
            || vt
                .checked_add(vtsize)
                .map(|e| e > self.b.len())
                .unwrap_or(true)
        {
            return Err(corrupt("flatbuffer vtable size is out of bounds"));
        }
        Ok((vt, vtsize))
    }

    /// The absolute position of a present field value, or `None` if absent.
    fn field_off(&self, table: u32, field: u16) -> Result<Option<usize>> {
        let (vt, vtsize) = self.vtable(table)?;
        let slot = 4usize
            .checked_add(2 * field as usize)
            .ok_or_else(|| corrupt("flatbuffer field slot overflows"))?;
        if slot + 2 > vtsize {
            return Ok(None);
        }
        let off = self.u16(vt + slot)? as usize;
        if off == 0 {
            return Ok(None);
        }
        let pos = (table as usize)
            .checked_add(off)
            .ok_or_else(|| corrupt("flatbuffer field position overflows"))?;
        if pos >= self.b.len() {
            return Err(corrupt("flatbuffer field is out of bounds"));
        }
        Ok(Some(pos))
    }

    fn field_u8(&self, table: u32, field: u16) -> Result<Option<u8>> {
        match self.field_off(table, field)? {
            Some(p) => Ok(Some(self.u8(p)?)),
            None => Ok(None),
        }
    }
    fn field_i16(&self, table: u32, field: u16) -> Result<Option<i16>> {
        match self.field_off(table, field)? {
            Some(p) => Ok(Some(self.i16(p)?)),
            None => Ok(None),
        }
    }
    fn field_i32(&self, table: u32, field: u16) -> Result<Option<i32>> {
        match self.field_off(table, field)? {
            Some(p) => Ok(Some(self.i32(p)?)),
            None => Ok(None),
        }
    }
    fn field_i64(&self, table: u32, field: u16) -> Result<Option<i64>> {
        match self.field_off(table, field)? {
            Some(p) => Ok(Some(self.i64(p)?)),
            None => Ok(None),
        }
    }

    /// Follow a uoffset stored at `pos` to its target.
    fn indirect(&self, pos: usize) -> Result<usize> {
        let off = self.u32(pos)? as usize;
        let target = pos
            .checked_add(off)
            .ok_or_else(|| corrupt("flatbuffer indirect overflows"))?;
        if target >= self.b.len() {
            return Err(corrupt("flatbuffer indirect target is out of bounds"));
        }
        Ok(target)
    }

    /// A string field as a lossy UTF-8 string.
    fn field_string(&self, table: u32, field: u16) -> Result<Option<String>> {
        match self.field_off(table, field)? {
            Some(p) => {
                let s = self.indirect(p)?;
                let n = self.u32(s)? as usize;
                let bytes = self
                    .get(s + 4, n)
                    .map_err(|_| corrupt("flatbuffer string is out of bounds"))?;
                Ok(Some(String::from_utf8_lossy(bytes).into_owned()))
            }
            None => Ok(None),
        }
    }

    /// A table field, returning the target table position.
    fn field_table(&self, table: u32, field: u16) -> Result<Option<u32>> {
        match self.field_off(table, field)? {
            Some(p) => Ok(Some(self.indirect(p)? as u32)),
            None => Ok(None),
        }
    }

    /// The `(length, element data position)` of a vector of tables/strings.
    fn field_vector(&self, table: u32, field: u16) -> Result<Option<(u32, usize)>> {
        match self.field_off(table, field)? {
            Some(p) => {
                let v = self.indirect(p)?;
                let n = self.u32(v)?;
                Ok(Some((n, v + 4)))
            }
            None => Ok(None),
        }
    }

    /// The `(length, element data position)` of a vector of 8-aligned structs.
    fn field_struct_vector(&self, table: u32, field: u16) -> Result<Option<(u32, usize)>> {
        match self.field_off(table, field)? {
            Some(p) => {
                let v = self.indirect(p)?;
                let n = self.u32(v)?;
                let data = align8(v + 4).ok_or_else(|| corrupt("flatbuffer vector overflows"))?;
                Ok(Some((n, data)))
            }
            None => Ok(None),
        }
    }
}

// ---------------------------------------------------------------------------
// Encapsulated messages
// ---------------------------------------------------------------------------

/// A parsed encapsulated message's metadata, borrowing the metadata slice.
struct Msg<'a> {
    fb: Fb<'a>,
    root: u32,
    header_type: i32,
    meta_start: usize,
    meta_len: usize,
    body_len: i64,
}

impl<'a> Msg<'a> {
    fn body_offset(&self) -> Result<usize> {
        align8(
            self.meta_start
                .checked_add(self.meta_len)
                .ok_or_else(|| corrupt("message size overflows"))?,
        )
        .ok_or_else(|| corrupt("message body offset overflows"))
    }

    fn header(&self) -> Result<u32> {
        self.fb
            .field_table(self.root, 2)?
            .ok_or_else(|| corrupt("message has no header"))
    }
}

/// Parse one encapsulated message at `offset` (the continuation token).
fn parse_msg<'a>(source: &'a [u8], offset: usize, limits: Limits) -> Result<Msg<'a>> {
    let n = source.len();
    if offset.checked_add(8).map(|e| e > n).unwrap_or(true) {
        return Err(corrupt("message prefix is out of bounds"));
    }
    let cont = u32::from_le_bytes([
        source[offset],
        source[offset + 1],
        source[offset + 2],
        source[offset + 3],
    ]);
    if cont != CONTINUATION {
        return Err(corrupt("missing message continuation marker"));
    }
    let meta_len = u32::from_le_bytes([
        source[offset + 4],
        source[offset + 5],
        source[offset + 6],
        source[offset + 7],
    ]) as usize;
    if meta_len == 0 {
        return Err(corrupt("empty message metadata"));
    }
    if meta_len as u64 > limits.max_arrow_metadata_bytes {
        return Err(Error::resource_limit(format!(
            "Arrow message metadata is {meta_len} bytes, above the {}-byte cap",
            limits.max_arrow_metadata_bytes
        )));
    }
    let meta_start = offset + 8;
    let meta_end = meta_start
        .checked_add(meta_len)
        .ok_or_else(|| corrupt("message metadata overflows"))?;
    if meta_end > n {
        return Err(corrupt("message metadata is out of bounds"));
    }
    let fb = Fb::new(&source[meta_start..meta_end]);
    let root = fb.root()?;
    let header_type = fb.field_u8(root, 1)?.unwrap_or(0) as i32;
    let body_len = fb.field_i64(root, 3)?.unwrap_or(0);
    if body_len < 0 {
        return Err(corrupt("message body length is negative"));
    }
    Ok(Msg {
        fb,
        root,
        header_type,
        meta_start,
        meta_len,
        body_len,
    })
}

// ---------------------------------------------------------------------------
// Schema parsing
// ---------------------------------------------------------------------------

struct RawField {
    name: String,
    nullable: bool,
    type_tag: i32,
    type_text: String,
    bit_width: i32,
    int_signed: bool,
    float_precision: i32,
    union_mode: i32,
    dict_id: Option<i64>,
    children: Vec<RawField>,
}

fn time_unit_name(u: i32) -> &'static str {
    match u {
        0 => "SECOND",
        1 => "MILLISECOND",
        2 => "MICROSECOND",
        3 => "NANOSECOND",
        _ => "UNKNOWN",
    }
}

fn float_prec_name(p: i32) -> &'static str {
    match p {
        0 => "HALF",
        1 => "SINGLE",
        2 => "DOUBLE",
        _ => "UNKNOWN",
    }
}

fn parse_type(fb: &Fb, table: u32, tag: i32) -> Result<(String, i32, bool, i32, i32)> {
    // Returns (type_text, bit_width, int_signed, float_precision, union_mode).
    let mut bit_width = 0i32;
    let mut int_signed = false;
    let mut float_precision = -1i32;
    let mut union_mode = -1i32;
    let text = match tag {
        T_NULL => "Null".to_string(),
        T_INT => {
            let bw = fb.field_i32(table, 0)?.unwrap_or(0);
            let signed = fb.field_u8(table, 1)?.unwrap_or(0) != 0;
            bit_width = bw;
            int_signed = signed;
            format!(
                "Int({}, {})",
                bw,
                if signed { "signed" } else { "unsigned" }
            )
        }
        T_FLOAT => {
            let p = fb.field_i16(table, 0)?.unwrap_or(0) as i32;
            float_precision = p;
            format!("FloatingPoint({})", float_prec_name(p))
        }
        T_BINARY => "Binary".to_string(),
        T_UTF8 => "Utf8".to_string(),
        T_BOOL => "Bool".to_string(),
        T_DECIMAL => {
            let prec = fb.field_i32(table, 0)?.unwrap_or(0);
            let scale = fb.field_i32(table, 1)?.unwrap_or(0);
            let bw = fb.field_i32(table, 2)?.unwrap_or(128);
            format!("Decimal{}({}, {})", bw, prec, scale)
        }
        T_DATE => {
            let u = fb.field_i16(table, 0)?.unwrap_or(0) as i32;
            // Date32 (`DAY`) is a 4-byte int; Date64 (`MILLISECOND`) is 8-byte.
            bit_width = if u == 0 { 4 } else { 8 };
            format!("Date({})", if u == 0 { "DAY" } else { "MILLISECOND" })
        }
        T_TIME => {
            let u = fb.field_i16(table, 0)?.unwrap_or(0) as i32;
            let bw = fb.field_i32(table, 1)?.unwrap_or(32);
            bit_width = bw;
            format!("Time({}, {})", time_unit_name(u), bw)
        }
        T_TIMESTAMP => {
            let u = fb.field_i16(table, 0)?.unwrap_or(0) as i32;
            let tz = fb.field_string(table, 1)?.unwrap_or_default();
            if tz.is_empty() {
                format!("Timestamp({})", time_unit_name(u))
            } else {
                format!("Timestamp({}, {})", time_unit_name(u), tz)
            }
        }
        T_INTERVAL => {
            let u = fb.field_i16(table, 0)?.unwrap_or(0) as i32;
            let name = match u {
                0 => "YEAR_MONTH",
                1 => "DAY_TIME",
                2 => "MONTH_DAY_NANO",
                _ => "UNKNOWN",
            };
            format!("Interval({name})")
        }
        T_LIST => "List".to_string(),
        T_STRUCT => "Struct".to_string(),
        T_UNION => {
            let m = fb.field_i16(table, 0)?.unwrap_or(0) as i32;
            union_mode = m;
            format!("Union({})", if m == 0 { "sparse" } else { "dense" })
        }
        T_FIXED_SIZE_BINARY => {
            let w = fb.field_i32(table, 0)?.unwrap_or(0);
            bit_width = w;
            format!("FixedSizeBinary({w})")
        }
        T_FIXED_SIZE_LIST => {
            let s = fb.field_i32(table, 0)?.unwrap_or(0);
            format!("FixedSizeList({s})")
        }
        T_MAP => "Map".to_string(),
        T_DURATION => {
            let u = fb.field_i16(table, 0)?.unwrap_or(0) as i32;
            format!("Duration({})", time_unit_name(u))
        }
        T_LARGE_BINARY => "LargeBinary".to_string(),
        T_LARGE_UTF8 => "LargeUtf8".to_string(),
        T_LARGE_LIST => "LargeList".to_string(),
        T_RUN_END_ENCODED => "RunEndEncoded".to_string(),
        T_BINARY_VIEW => "BinaryView".to_string(),
        T_UTF8_VIEW => "Utf8View".to_string(),
        T_LIST_VIEW => "ListView".to_string(),
        T_LARGE_LIST_VIEW => "LargeListView".to_string(),
        _ => format!("NONE/UNKNOWN({tag})"),
    };
    Ok((text, bit_width, int_signed, float_precision, union_mode))
}

fn parse_field(fb: &Fb, table: u32, depth: u32, limits: Limits) -> Result<RawField> {
    if depth > MAX_FB_DEPTH {
        return Err(corrupt("schema nesting is too deep"));
    }
    let name = fb.field_string(table, 0)?.unwrap_or_default();
    let nullable = fb.field_u8(table, 1)?.unwrap_or(0) != 0;
    let type_tag = fb.field_u8(table, 2)?.unwrap_or(0) as i32;
    let type_table = fb.field_table(table, 3)?;
    let (type_text, bit_width, int_signed, float_precision, union_mode) = match type_table {
        Some(t) => parse_type(fb, t, type_tag)?,
        None => (type_name(type_tag).to_string(), 0, false, -1, -1),
    };
    let dict_id = match fb.field_table(table, 4)? {
        Some(d) => fb.field_i64(d, 0)?,
        None => None,
    };
    let mut children = Vec::new();
    if let Some((n, data)) = fb.field_vector(table, 5)? {
        if n as u64 > u64::from(limits.max_arrow_columns) {
            return Err(Error::resource_limit("Arrow schema has too many fields"));
        }
        for i in 0..n as usize {
            let pos = data
                .checked_add(4 * i)
                .ok_or_else(|| corrupt("schema child vector overflows"))?;
            let child = fb.indirect(pos)? as u32;
            children.push(parse_field(fb, child, depth + 1, limits)?);
        }
    }
    Ok(RawField {
        name,
        nullable,
        type_tag,
        type_text,
        bit_width,
        int_signed,
        float_precision,
        union_mode,
        dict_id,
        children,
    })
}

fn parse_schema(fb: &Fb, schema_table: u32, limits: Limits) -> Result<(i32, Vec<RawField>)> {
    let endianness = fb.field_i16(schema_table, 0)?.unwrap_or(0) as i32;
    let mut fields = Vec::new();
    if let Some((n, data)) = fb.field_vector(schema_table, 1)? {
        if n as u64 > u64::from(limits.max_arrow_columns) {
            return Err(Error::resource_limit(
                "Arrow schema has too many top-level fields",
            ));
        }
        for i in 0..n as usize {
            let pos = data
                .checked_add(4 * i)
                .ok_or_else(|| corrupt("schema field vector overflows"))?;
            let f = fb.indirect(pos)? as u32;
            fields.push(parse_field(fb, f, 0, limits)?);
        }
    }
    Ok((endianness, fields))
}

/// Parse the schema carried by a message (stream format).
fn parse_schema_msg(fb: &Fb, schema_table: u32, limits: Limits) -> Result<(i32, Vec<RawField>)> {
    parse_schema(fb, schema_table, limits)
}

fn flatten_fields(
    raw: &[RawField],
    out: &mut Vec<FieldNode>,
    depth: u32,
    parent: u32,
    limits: Limits,
) -> Result<()> {
    if depth > MAX_FB_DEPTH {
        return Err(corrupt("schema tree is too deep"));
    }
    for f in raw {
        if out.len() as u64 > u64::from(limits.max_arrow_columns) * 4 + 16 {
            return Err(Error::resource_limit("Arrow schema is too large"));
        }
        let me = out.len() as u32;
        out.push(FieldNode {
            depth,
            parent,
            name: f.name.clone(),
            nullable: f.nullable,
            type_tag: f.type_tag,
            type_text: f.type_text.clone(),
            bit_width: f.bit_width,
            int_signed: f.int_signed,
            float_precision: f.float_precision,
            union_mode: f.union_mode,
            dict_id: f.dict_id,
            num_children: f.children.len() as u32,
        });
        flatten_fields(&f.children, out, depth + 1, me, limits)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Buffer-layout accounting
// ---------------------------------------------------------------------------

/// The number of buffers a field of `tag` consumes itself (validity + extras),
/// or `None` when the layout is not statically known.
fn type_buffers(tag: i32, union_mode: i32) -> Option<u32> {
    Some(match tag {
        T_NULL => 0,
        T_INT | T_FLOAT | T_DECIMAL | T_DATE | T_TIME | T_TIMESTAMP | T_DURATION
        | T_FIXED_SIZE_BINARY | T_BOOL => 2,
        T_BINARY | T_UTF8 | T_LARGE_BINARY | T_LARGE_UTF8 => 3,
        T_LIST | T_LARGE_LIST => 2,
        T_LIST_VIEW | T_LARGE_LIST_VIEW => 3,
        T_FIXED_SIZE_LIST => 1,
        T_STRUCT => 1,
        T_MAP => 2,
        T_UNION => {
            if union_mode == 1 {
                2
            } else {
                1
            }
        }
        T_RUN_END_ENCODED => 0,
        // View types carry a variadic number of buffers, so later slots cannot be
        // statically indexed.
        T_BINARY_VIEW | T_UTF8_VIEW => return None,
        _ => return None,
    })
}

/// The total number of buffers of the subtree rooted at `idx`, plus the index of
/// the following sibling. `None` when any layout in the subtree is unknown.
fn subtree_buffers(schema: &[FieldNode], idx: usize, depth: u32) -> Option<(u32, usize)> {
    if depth > MAX_FB_DEPTH {
        return None;
    }
    let f = schema.get(idx)?;
    let mut count = if f.dict_id.is_some() {
        2
    } else {
        type_buffers(f.type_tag, f.union_mode)?
    };
    let mut i = idx.checked_add(1)?;
    if f.dict_id.is_some() {
        // A dictionary-encoded field does not serialize its children inline.
        i = i.checked_add(f.num_children as usize)?;
    } else {
        for _ in 0..f.num_children {
            let (c, ni) = subtree_buffers(schema, i, depth + 1)?;
            count = count.checked_add(c)?;
            i = ni;
        }
    }
    Some((count, i))
}

fn build_leaves(
    schema: &[FieldNode],
    indexable: &mut bool,
    limits: Limits,
) -> Result<Vec<LeafColumn>> {
    let mut leaves = Vec::new();
    let mut slot: i64 = 0;
    let mut i = 0usize;
    while i < schema.len() {
        let f = &schema[i];
        // This loop is entered only at depth-0 fields; subtree_buffers advances past
        // the whole subtree.
        if f.depth != 0 {
            // Should not happen (we always jump whole subtrees), but fail safe.
            i += 1;
            continue;
        }
        if leaves.len() as u64 >= u64::from(limits.max_arrow_columns) {
            return Err(Error::resource_limit(format!(
                "Arrow file exceeds the {}-column cap",
                limits.max_arrow_columns
            )));
        }
        let next_slot = match subtree_buffers(schema, i, 0) {
            Some((c, next)) => {
                let ns = slot + i64::from(c);
                (slot, next, Some(ns))
            }
            None => {
                *indexable = false;
                (slot, i + 1, None)
            }
        };
        let (base, next, maybe_next_slot) = next_slot;
        leaves.push(make_leaf(f, i as u32, base));
        if let Some(ns) = maybe_next_slot {
            slot = ns;
        }
        i = next;
    }
    Ok(leaves)
}

fn make_leaf(f: &FieldNode, element: u32, base: i64) -> LeafColumn {
    let mut leaf = LeafColumn {
        element,
        node_index: element,
        name: f.name.clone(),
        type_tag: f.type_tag,
        supported: false,
        decline: None,
        validity_slot: -1,
        offsets_slot: -1,
        data_slot: -1,
        width: 0,
        int_signed: f.int_signed,
        float_precision: f.float_precision,
        is_utf8: false,
        large_offsets: false,
    };
    if f.dict_id.is_some() {
        leaf.decline = Some("dictionary-encoded columns are declined".to_string());
        return leaf;
    }
    if f.num_children != 0 {
        leaf.decline = Some(format!(
            "{} is a nested type and is declined",
            type_name(f.type_tag)
        ));
        return leaf;
    }
    let validity = i32::try_from(base).unwrap_or(-1);
    match f.type_tag {
        T_INT => {
            if !matches!(f.bit_width, 8 | 16 | 32 | 64) {
                leaf.decline = Some(format!("Int({}) width is not supported", f.bit_width));
                return leaf;
            }
            leaf.validity_slot = validity;
            leaf.data_slot = validity.saturating_add(1);
            leaf.width = f.bit_width / 8;
            leaf.supported = true;
        }
        T_FLOAT => {
            let w = match f.float_precision {
                0 => 2,
                1 => 4,
                2 => 8,
                _ => {
                    leaf.decline = Some("unknown floating-point precision".to_string());
                    return leaf;
                }
            };
            leaf.validity_slot = validity;
            leaf.data_slot = validity.saturating_add(1);
            leaf.width = w;
            leaf.supported = true;
        }
        T_BOOL => {
            leaf.validity_slot = validity;
            leaf.data_slot = validity.saturating_add(1);
            leaf.supported = true;
        }
        T_DATE => {
            leaf.validity_slot = validity;
            leaf.data_slot = validity.saturating_add(1);
            leaf.width = if f.bit_width == 8 { 8 } else { 4 };
            leaf.supported = true;
        }
        T_TIMESTAMP | T_DURATION => {
            leaf.validity_slot = validity;
            leaf.data_slot = validity.saturating_add(1);
            leaf.width = 8;
            leaf.supported = true;
        }
        T_TIME => {
            leaf.validity_slot = validity;
            leaf.data_slot = validity.saturating_add(1);
            leaf.width = if f.bit_width == 64 { 8 } else { 4 };
            leaf.supported = true;
        }
        T_FIXED_SIZE_BINARY => {
            if f.bit_width <= 0 {
                leaf.decline = Some(format!("FixedSizeBinary({}) width is invalid", f.bit_width));
                return leaf;
            }
            leaf.validity_slot = validity;
            leaf.data_slot = validity.saturating_add(1);
            leaf.width = f.bit_width;
            leaf.supported = true;
        }
        T_UTF8 | T_BINARY | T_LARGE_UTF8 | T_LARGE_BINARY => {
            leaf.validity_slot = validity;
            leaf.offsets_slot = validity.saturating_add(1);
            leaf.data_slot = validity.saturating_add(2);
            leaf.is_utf8 = matches!(f.type_tag, T_UTF8 | T_LARGE_UTF8);
            leaf.large_offsets = matches!(f.type_tag, T_LARGE_UTF8 | T_LARGE_BINARY);
            leaf.supported = true;
        }
        T_NULL => leaf.decline = Some("Null columns are declined".to_string()),
        T_DECIMAL => leaf.decline = Some("Decimal columns are declined".to_string()),
        T_INTERVAL => leaf.decline = Some("Interval columns are declined".to_string()),
        T_RUN_END_ENCODED => leaf.decline = Some("RunEndEncoded columns are declined".to_string()),
        T_BINARY_VIEW | T_UTF8_VIEW => {
            leaf.decline = Some(format!("{} columns are declined", type_name(f.type_tag)))
        }
        other => {
            leaf.decline = Some(format!("{} columns are declined", type_name(other)));
        }
    }
    leaf
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// A pure, bounded, byte-level Arrow IPC detector.
///
/// True iff the source starts with the `ARROW1` magic (6 bytes, optionally
/// followed by up to 2 padding bytes to the 8-byte boundary), and is either:
/// * the **file** format — the source ends with the `ARROW1` magic preceded by a
///   little-endian `int32` footer length consistent with the file length (the
///   footer region begins no earlier than the 8-byte magic+padding prefix); or
/// * the **stream** format — a valid encapsulated `Schema` message begins at the
///   8-byte prefix (continuation token `0xFFFFFFFF` + a metadata size that fits).
///
/// It never decodes a buffer.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.len() as u64 > limits.max_arrow_document_bytes {
        return false;
    }
    let n = source.len();
    if n < 16 {
        return false;
    }
    if &source[0..6] != MAGIC {
        return false;
    }
    // A trailing `ARROW1` magic commits the source to the **file** format: the 4
    // bytes before it must be a footer length consistent with the file length.
    if &source[n - 6..n] == MAGIC {
        let fl = u32::from_le_bytes([source[n - 10], source[n - 9], source[n - 8], source[n - 7]]);
        if u64::from(fl) > limits.max_arrow_metadata_bytes {
            return false;
        }
        return match n.checked_sub(10 + fl as usize) {
            Some(start) => start >= 8,
            None => false,
        };
    }
    // No trailing magic: the **stream** format, which must begin with the 8-byte
    // magic+padding prefix followed by a valid encapsulated `Schema` message.
    stream_schema_ok(source, limits)
}

fn stream_schema_ok(source: &[u8], limits: Limits) -> bool {
    let msg = match parse_msg(source, 8, limits) {
        Ok(m) => m,
        Err(_) => return false,
    };
    if msg.header_type != H_SCHEMA {
        return false;
    }
    match msg.header() {
        Ok(t) => parse_schema(&msg.fb, t, limits).is_ok(),
        Err(_) => false,
    }
}

// ---------------------------------------------------------------------------
// Parsing the whole source into the model
// ---------------------------------------------------------------------------

/// Parse `source` into the canonical derived model.
pub fn parse(source: &[u8], limits: Limits) -> Result<ArrowModel> {
    let n = source.len();
    if n as u64 > limits.max_arrow_document_bytes {
        return Err(Error::resource_limit(format!(
            "Arrow source is {n} bytes, above the {}-byte document cap",
            limits.max_arrow_document_bytes
        )));
    }
    if n < 16 {
        return Err(corrupt("file is shorter than the Arrow minimum"));
    }
    if &source[0..6] != MAGIC {
        return Err(corrupt("missing ARROW1 magic prefix"));
    }
    if let Some(footer_start) = file_footer_start(source, limits)? {
        parse_file(source, footer_start, limits)
    } else {
        parse_stream(source, limits)
    }
}

/// The footer start offset for the file format, or `None` for the stream format.
fn file_footer_start(source: &[u8], limits: Limits) -> Result<Option<usize>> {
    let n = source.len();
    if n < 16 || &source[n - 6..n] != MAGIC {
        return Ok(None);
    }
    let fl =
        u32::from_le_bytes([source[n - 10], source[n - 9], source[n - 8], source[n - 7]]) as usize;
    if fl as u64 > limits.max_arrow_metadata_bytes {
        return Err(Error::resource_limit(format!(
            "Arrow footer is {fl} bytes, above the {}-byte metadata cap",
            limits.max_arrow_metadata_bytes
        )));
    }
    let start = n
        .checked_sub(10 + fl)
        .filter(|s| *s >= 8)
        .ok_or_else(|| corrupt("footer length is inconsistent with the file length"))?;
    Ok(Some(start))
}

fn parse_file(source: &[u8], footer_start: usize, limits: Limits) -> Result<ArrowModel> {
    let n = source.len();
    let fb = Fb::new(&source[footer_start..n - 10]);
    let root = fb.root()?;
    let version = fb.field_i16(root, 0)?.unwrap_or(0) as i32;
    let schema_table = fb
        .field_table(root, 1)?
        .ok_or_else(|| corrupt("footer has no schema"))?;
    let (endianness, raw) = parse_schema(&fb, schema_table, limits)?;
    let mut schema = Vec::new();
    flatten_fields(&raw, &mut schema, 0, u32::MAX, limits)?;
    finish_model(
        source,
        limits,
        version,
        endianness,
        false,
        schema,
        FileBatchSource::Footer(root),
        &fb,
    )
}

/// A light indirection so file- and stream-format batch parsing share one tail.
enum FileBatchSource {
    /// The footer root table, whose `recordBatches` (field 3) lists the blocks.
    Footer(u32),
    /// No footer (stream format): messages are walked from `start`.
    Stream(usize),
}

#[allow(clippy::too_many_arguments)]
fn finish_model(
    source: &[u8],
    limits: Limits,
    version: i32,
    endianness: i32,
    stream: bool,
    schema: Vec<FieldNode>,
    source_kind: FileBatchSource,
    fb: &Fb,
) -> Result<ArrowModel> {
    let n = source.len();
    let mut indexable = endianness == 0;
    let leaves = build_leaves(&schema, &mut indexable, limits)?;

    let mut batches: Vec<BatchDesc> = Vec::new();
    let mut total_rows: u64 = 0;
    let mut message_budget = limits.max_arrow_messages as u64;

    let push_batch = |desc: BatchDesc,
                      batches: &mut Vec<BatchDesc>,
                      total_rows: &mut u64,
                      message_budget: &mut u64|
     -> Result<()> {
        if batches.len() as u64 >= u64::from(limits.max_arrow_batches) {
            return Err(Error::resource_limit(format!(
                "Arrow file exceeds the {}-batch cap",
                limits.max_arrow_batches
            )));
        }
        if *message_budget == 0 {
            return Err(Error::resource_limit("Arrow file exceeds the message cap"));
        }
        *message_budget -= 1;
        let rows = u64::try_from(desc.num_rows).unwrap_or(0);
        *total_rows = total_rows.saturating_add(rows);
        if *total_rows > limits.max_arrow_rows {
            return Err(Error::resource_limit(
                "Arrow file exceeds the total-row cap",
            ));
        }
        batches.push(desc);
        Ok(())
    };

    match source_kind {
        FileBatchSource::Footer(root) => {
            let blocks = fb.field_vector(root, 3)?;
            if let Some((count, data)) = blocks {
                if count as u64 > u64::from(limits.max_arrow_batches) {
                    return Err(Error::resource_limit(
                        "Arrow footer lists too many record batches",
                    ));
                }
                for i in 0..count as usize {
                    let pos = data
                        .checked_add(4 * i)
                        .ok_or_else(|| corrupt("block vector overflows"))?;
                    let block = fb.indirect(pos)? as u32;
                    let offset = fb.field_i64(block, 0)?.unwrap_or(0);
                    if offset < 0 {
                        return Err(corrupt("record-batch block offset is negative"));
                    }
                    let desc = read_batch_desc(source, offset as usize, limits)?;
                    push_batch(desc, &mut batches, &mut total_rows, &mut message_budget)?;
                }
            }
        }
        FileBatchSource::Stream(start) => {
            let mut pos = start;
            let mut saw_schema = false;
            loop {
                if pos.checked_add(8).map(|e| e > n).unwrap_or(true) {
                    break;
                }
                let cont = u32::from_le_bytes([
                    source[pos],
                    source[pos + 1],
                    source[pos + 2],
                    source[pos + 3],
                ]);
                let mlen = u32::from_le_bytes([
                    source[pos + 4],
                    source[pos + 5],
                    source[pos + 6],
                    source[pos + 7],
                ]);
                if cont == CONTINUATION && mlen == 0 {
                    // End-of-stream marker.
                    break;
                }
                if cont != CONTINUATION {
                    // Tolerate a leading length (pre-0.15 streams) only before the
                    // schema message.
                    return Err(corrupt("missing stream continuation marker"));
                }
                let msg = parse_msg(source, pos, limits)?;
                match msg.header_type {
                    H_SCHEMA => {
                        if !saw_schema {
                            saw_schema = true;
                        }
                    }
                    H_RECORD_BATCH => {
                        let desc = batch_desc_from_msg(source, pos, &msg, limits)?;
                        push_batch(desc, &mut batches, &mut total_rows, &mut message_budget)?;
                    }
                    H_DICTIONARY_BATCH => {
                        // Dictionaries are recorded implicitly; the fields that use
                        // them are declined at decode. Count toward the message cap.
                        if message_budget == 0 {
                            return Err(Error::resource_limit(
                                "Arrow stream exceeds the message cap",
                            ));
                        }
                        message_budget -= 1;
                    }
                    other => {
                        return Err(Error::unsupported_feature(format!(
                            "Arrow message header {other} is not supported"
                        )));
                    }
                }
                let body_off = msg.body_offset()?;
                let body_len = usize::try_from(msg.body_len)
                    .map_err(|_| Error::resource_limit("Arrow body length is implausible"))?;
                let next = body_off
                    .checked_add(body_len)
                    .ok_or_else(|| corrupt("message advance overflows"))?;
                if next <= pos || next > n {
                    // A zero-advance or overrun stream is corrupt/truncated: stop.
                    if next > n {
                        return Err(corrupt("stream message body is out of bounds"));
                    }
                    break;
                }
                pos = next;
            }
        }
    }

    Ok(ArrowModel {
        doc_len: n as u64,
        version,
        endianness,
        stream,
        indexable,
        schema,
        leaves,
        batches,
    })
}

fn parse_stream(source: &[u8], limits: Limits) -> Result<ArrowModel> {
    let n = source.len();
    // The first message (at the 8-byte prefix) must be the schema.
    let msg = parse_msg(source, 8, limits)?;
    if msg.header_type != H_SCHEMA {
        return Err(corrupt("stream does not begin with a schema message"));
    }
    let schema_table = msg.header()?;
    let (endianness, raw) = parse_schema_msg(&msg.fb, schema_table, limits)?;
    let version = msg.fb.field_i16(msg.root, 0)?.unwrap_or(0) as i32;
    let mut schema = Vec::new();
    flatten_fields(&raw, &mut schema, 0, u32::MAX, limits)?;
    // Re-scan from the schema message to enumerate batches.
    let _ = n;
    finish_model(
        source,
        limits,
        version,
        endianness,
        true,
        schema,
        FileBatchSource::Stream(8),
        &msg.fb,
    )
}

/// Read a record batch's inventory from the message at `offset`.
fn read_batch_desc(source: &[u8], offset: usize, limits: Limits) -> Result<BatchDesc> {
    let msg = parse_msg(source, offset, limits)?;
    batch_desc_from_msg(source, offset, &msg, limits)
}

fn batch_desc_from_msg(
    source: &[u8],
    offset: usize,
    msg: &Msg,
    limits: Limits,
) -> Result<BatchDesc> {
    if msg.header_type != H_RECORD_BATCH {
        return Err(corrupt("message is not a record batch"));
    }
    let rb = msg.header()?;
    let rows = msg.fb.field_i64(rb, 0)?.unwrap_or(0);
    if rows < 0 {
        return Err(corrupt("record batch row count is negative"));
    }
    let (num_nodes, _) = msg.fb.field_struct_vector(rb, 1)?.unwrap_or((0, 0));
    let (num_buffers, _) = msg.fb.field_struct_vector(rb, 2)?.unwrap_or((0, 0));
    if u64::from(num_buffers) > limits.max_arrow_buffers {
        return Err(Error::resource_limit(format!(
            "record batch has {num_buffers} buffers, above the {}-buffer cap",
            limits.max_arrow_buffers
        )));
    }
    let compression = match msg.fb.field_table(rb, 3)? {
        Some(c) => {
            // BodyCompression.codec is a byte enum (0 LZ4_FRAME, 1 ZSTD).
            msg.fb.field_u8(c, 0)?.unwrap_or(0) as i32
        }
        None => -1,
    };
    let body_off = msg.body_offset()?;
    let body_len = usize::try_from(msg.body_len)
        .map_err(|_| Error::resource_limit("Arrow body length is implausible"))?;
    let span_start = offset as u64;
    let span_end = body_off
        .checked_add(body_len)
        .ok_or_else(|| corrupt("batch span overflows"))?;
    if span_end > source.len() {
        return Err(corrupt("record batch body is outside the source"));
    }
    Ok(BatchDesc {
        offset: offset as u64,
        meta_len: msg.meta_len as u64,
        body_offset: body_off as u64,
        body_len: body_len as u64,
        span_start,
        span_end: span_end as u64,
        num_rows: rows,
        num_nodes,
        num_buffers,
        compression,
    })
}

/// The parsed metadata of one record batch (for the `arrow-batch` observation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchDetail {
    /// The number of rows.
    pub rows: i64,
    /// The number of flattened field nodes.
    pub num_nodes: u32,
    /// The number of buffers.
    pub num_buffers: u32,
    /// The body compression codec tag (-1 for uncompressed).
    pub compression: i32,
    /// The exact source span of the whole message.
    pub span: (u64, u64),
    /// The body start offset.
    pub body_offset: u64,
    /// The per-field `(length, null_count)` nodes.
    pub nodes: Vec<(i64, i64)>,
    /// The per-buffer `(offset, length)` entries, relative to the body.
    pub buffers: Vec<(i64, i64)>,
}

/// Read one record batch's inventory (for `--arrow-batch`).
pub fn batch_detail(
    source: &[u8],
    model: &ArrowModel,
    index: u32,
    limits: Limits,
) -> Result<BatchDetail> {
    let desc = model
        .batches
        .get(index as usize)
        .ok_or_else(|| Error::unsupported_feature(format!("no Arrow batch {index}")))?;
    let meta = read_batch_meta(source, desc, limits)?;
    Ok(BatchDetail {
        rows: meta.rows,
        num_nodes: meta.nodes.len() as u32,
        num_buffers: meta.buffers.len() as u32,
        compression: meta.compression,
        span: (desc.span_start, desc.span_end),
        body_offset: desc.body_offset,
        nodes: meta.nodes,
        buffers: meta.buffers,
    })
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of an `ArrowModel` node).
pub fn build_arrow_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits)?.encode())
}

// ---------------------------------------------------------------------------
// Value decoding
// ---------------------------------------------------------------------------

/// A decoded leaf value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A null (a validity bit of 0).
    Null,
    /// A boolean.
    Bool(bool),
    /// A signed integer (`Int`, `Date`, `Time`, `Timestamp`, `Duration`).
    I64(i64),
    /// An unsigned integer (`Int`).
    U64(u64),
    /// A half-precision float (raw bits).
    F16(u16),
    /// A 32-bit float.
    F32(f32),
    /// A 64-bit float.
    F64(f64),
    /// Opaque bytes (`Binary`/`Utf8`/`FixedSizeBinary`).
    Bytes(Vec<u8>),
}

/// Render a decoded value as text, honouring the column's type.
pub fn value_text(v: &Value, leaf: &LeafColumn) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        Value::I64(n) => n.to_string(),
        Value::U64(n) => n.to_string(),
        Value::F16(bits) => format_f32(half_to_f32(*bits)),
        Value::F32(f) => format_f32(*f),
        Value::F64(f) => format_f64(*f),
        Value::Bytes(b) => {
            if leaf.is_utf8 {
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

/// Convert IEEE 754 binary16 bits to `f32`.
pub fn half_to_f32(bits: u16) -> f32 {
    let sign = u32::from(bits >> 15) << 31;
    let exp = u32::from((bits >> 10) & 0x1f);
    let frac = u32::from(bits & 0x3ff);
    let out = if exp == 0 {
        if frac == 0 {
            sign
        } else {
            // Subnormal: normalize.
            let mut e = -1i32;
            let mut f = frac;
            while f & 0x400 == 0 {
                f <<= 1;
                e -= 1;
            }
            let exp32 = (127 - 15 + e + 1) as u32;
            sign | (exp32 << 23) | ((f & 0x3ff) << 13)
        }
    } else if exp == 0x1f {
        sign | 0x7f80_0000 | (frac << 13)
    } else {
        sign | ((exp + (127 - 15)) << 23) | (frac << 13)
    };
    f32::from_bits(out)
}

struct DecodeBudget {
    values_left: u64,
    bytes_left: u64,
}

impl DecodeBudget {
    fn add_values(&mut self, n: u64) -> Result<()> {
        if n > self.values_left {
            return Err(Error::resource_limit("Arrow decode exceeded the value cap"));
        }
        self.values_left -= n;
        Ok(())
    }

    fn add_bytes(&mut self, n: u64) -> Result<()> {
        if n > self.bytes_left {
            return Err(Error::resource_limit("Arrow decode exceeded the byte cap"));
        }
        self.bytes_left -= n;
        Ok(())
    }
}

/// The parsed metadata of one record batch.
struct RecordBatchMeta {
    rows: i64,
    nodes: Vec<(i64, i64)>,
    buffers: Vec<(i64, i64)>,
    compression: i32,
}

fn read_batch_meta(source: &[u8], desc: &BatchDesc, limits: Limits) -> Result<RecordBatchMeta> {
    let msg = parse_msg(source, desc.offset as usize, limits)?;
    if msg.header_type != H_RECORD_BATCH {
        return Err(corrupt("message is not a record batch"));
    }
    let rb = msg.header()?;
    let rows = msg.fb.field_i64(rb, 0)?.unwrap_or(0);
    if rows < 0 {
        return Err(corrupt("record batch row count is negative"));
    }
    let mut nodes = Vec::new();
    if let Some((n, data)) = msg.fb.field_struct_vector(rb, 1)? {
        if u64::from(n) > limits.max_arrow_buffers {
            return Err(Error::resource_limit(
                "record batch has too many field nodes",
            ));
        }
        for i in 0..n as usize {
            let p = data
                .checked_add(16 * i)
                .ok_or_else(|| corrupt("field-node vector overflows"))?;
            nodes.push((msg.fb.i64(p)?, msg.fb.i64(p + 8)?));
        }
    }
    let mut buffers = Vec::new();
    if let Some((n, data)) = msg.fb.field_struct_vector(rb, 2)? {
        if u64::from(n) > limits.max_arrow_buffers {
            return Err(Error::resource_limit("record batch has too many buffers"));
        }
        for i in 0..n as usize {
            let p = data
                .checked_add(16 * i)
                .ok_or_else(|| corrupt("buffer vector overflows"))?;
            buffers.push((msg.fb.i64(p)?, msg.fb.i64(p + 8)?));
        }
    }
    let compression = match msg.fb.field_table(rb, 3)? {
        Some(c) => msg.fb.field_u8(c, 0)?.unwrap_or(0) as i32,
        None => -1,
    };
    Ok(RecordBatchMeta {
        rows,
        nodes,
        buffers,
        compression,
    })
}

fn slot(buffers: &[(i64, i64)], idx: i32) -> Result<&(i64, i64)> {
    if idx < 0 {
        return Err(corrupt("column has no such buffer slot"));
    }
    buffers
        .get(idx as usize)
        .ok_or_else(|| corrupt("buffer slot is out of range"))
}

/// Slice a buffer out of the source, relative to the batch body.
fn buffer_slice<'a>(source: &'a [u8], desc: &BatchDesc, b: &(i64, i64)) -> Result<&'a [u8]> {
    if b.0 < 0 || b.1 < 0 {
        return Err(corrupt("buffer offset or length is negative"));
    }
    let start = (desc.body_offset).checked_add(b.0 as u64);
    let end = start.and_then(|s| s.checked_add(b.1 as u64));
    match (start, end) {
        (Some(s), Some(e)) if e <= source.len() as u64 => Ok(&source[s as usize..e as usize]),
        _ => Err(corrupt("buffer is outside the source")),
    }
}

fn validity_bit(buf: &[u8], i: usize) -> bool {
    let byte = buf[i / 8];
    (byte >> (i % 8)) & 1 != 0
}

fn decode_leaf_column(
    source: &[u8],
    desc: &BatchDesc,
    leaf: &LeafColumn,
    meta: &RecordBatchMeta,
    budget: &mut DecodeBudget,
    out: &mut Vec<Value>,
) -> Result<()> {
    let rows_usize = usize::try_from(meta.rows)
        .map_err(|_| Error::resource_limit("record batch row count is implausible"))?;
    budget.add_values(desc.num_rows.max(0) as u64)?;
    let valid = slot(&meta.buffers, leaf.validity_slot).ok();
    let validity = match valid {
        Some(b) => {
            let s = buffer_slice(source, desc, b)?;
            if s.is_empty() {
                None
            } else {
                if s.len() < rows_usize.div_ceil(8) {
                    return Err(corrupt("validity bitmap is too short"));
                }
                Some(s)
            }
        }
        None => None,
    };
    let is_valid = |i: usize, v: Option<&[u8]>| match v {
        Some(b) => validity_bit(b, i),
        None => true,
    };

    match leaf.type_tag {
        T_BOOL => {
            let b = slot(&meta.buffers, leaf.data_slot)?;
            let s = buffer_slice(source, desc, b)?;
            if s.len() < rows_usize.div_ceil(8) {
                return Err(corrupt("Boolean data buffer is too short"));
            }
            budget.add_bytes(s.len() as u64)?;
            for i in 0..rows_usize {
                if !is_valid(i, validity) {
                    out.push(Value::Null);
                } else {
                    out.push(Value::Bool(validity_bit(s, i)));
                }
            }
        }
        T_UTF8 | T_BINARY | T_LARGE_UTF8 | T_LARGE_BINARY => {
            let ob = slot(&meta.buffers, leaf.offsets_slot)?;
            let a = buffer_slice(source, desc, ob)?;
            let width = if leaf.large_offsets { 8 } else { 4 };
            let need = rows_usize
                .checked_add(1)
                .and_then(|n| n.checked_mul(width))
                .ok_or_else(|| corrupt("offsets buffer size overflows"))?;
            if a.len() < need {
                return Err(corrupt("offsets buffer is too short"));
            }
            let db = slot(&meta.buffers, leaf.data_slot)?;
            let d = buffer_slice(source, desc, db)?;
            budget.add_bytes((a.len() + d.len()) as u64)?;
            let mut offs: Vec<u64> = Vec::with_capacity(rows_usize + 1);
            for i in 0..=rows_usize {
                let o = if width == 8 {
                    let p = i * 8;
                    i64::from_le_bytes(a[p..p + 8].try_into().unwrap())
                } else {
                    let p = i * 4;
                    i64::from(i32::from_le_bytes(a[p..p + 4].try_into().unwrap()))
                };
                if o < 0 {
                    return Err(corrupt("offset is negative"));
                }
                offs.push(o as u64);
            }
            for i in 0..rows_usize {
                if !is_valid(i, validity) {
                    out.push(Value::Null);
                    continue;
                }
                let (lo, hi) = (offs[i], offs[i + 1]);
                if hi < lo || hi > d.len() as u64 {
                    return Err(corrupt("value offset is out of range"));
                }
                out.push(Value::Bytes(d[lo as usize..hi as usize].to_vec()));
            }
        }
        _ => {
            // Fixed-width primitives.
            let w = leaf.width as usize;
            if w == 0 {
                return Err(corrupt("fixed-width column has zero width"));
            }
            let b = slot(&meta.buffers, leaf.data_slot)?;
            let s = buffer_slice(source, desc, b)?;
            let need = rows_usize
                .checked_mul(w)
                .ok_or_else(|| corrupt("data buffer size overflows"))?;
            if s.len() < need {
                return Err(corrupt("data buffer is too short"));
            }
            budget.add_bytes(need as u64)?;
            for i in 0..rows_usize {
                if !is_valid(i, validity) {
                    out.push(Value::Null);
                    continue;
                }
                let p = i * w;
                let v = decode_fixed(leaf, &s[p..p + w])?;
                out.push(v);
            }
        }
    }
    Ok(())
}

fn decode_fixed(leaf: &LeafColumn, b: &[u8]) -> Result<Value> {
    Ok(match leaf.type_tag {
        T_INT => {
            if leaf.int_signed {
                let n = match b.len() {
                    1 => i64::from(b[0] as i8),
                    2 => i64::from(i16::from_le_bytes([b[0], b[1]])),
                    4 => i64::from(i32::from_le_bytes([b[0], b[1], b[2], b[3]])),
                    8 => i64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
                    _ => return Err(corrupt("unsupported signed integer width")),
                };
                Value::I64(n)
            } else {
                let n = match b.len() {
                    1 => u64::from(b[0]),
                    2 => u64::from(u16::from_le_bytes([b[0], b[1]])),
                    4 => u64::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
                    8 => u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
                    _ => return Err(corrupt("unsupported unsigned integer width")),
                };
                Value::U64(n)
            }
        }
        T_FLOAT => match leaf.float_precision {
            0 => Value::F16(u16::from_le_bytes([b[0], b[1]])),
            1 => Value::F32(f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            2 => Value::F64(f64::from_le_bytes([
                b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
            ])),
            _ => return Err(corrupt("unknown floating-point precision")),
        },
        T_DATE => {
            if b.len() == 8 {
                Value::I64(i64::from_le_bytes([
                    b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
                ]))
            } else {
                Value::I64(i64::from(i32::from_le_bytes([b[0], b[1], b[2], b[3]])))
            }
        }
        T_TIMESTAMP | T_DURATION => Value::I64(i64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ])),
        T_TIME => {
            if b.len() == 8 {
                Value::I64(i64::from_le_bytes([
                    b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
                ]))
            } else {
                Value::I64(i64::from(i32::from_le_bytes([b[0], b[1], b[2], b[3]])))
            }
        }
        T_FIXED_SIZE_BINARY => Value::Bytes(b.to_vec()),
        _ => return Err(corrupt("not a fixed-width column")),
    })
}

/// Decode one leaf column's values across every record batch, in row order.
///
/// Declines typed when the column (or any batch) uses an unsupported type,
/// layout, compression, or endianness.
pub fn column_values(
    source: &[u8],
    model: &ArrowModel,
    leaf_index: u32,
    limits: Limits,
) -> Result<Vec<Value>> {
    let leaf = model
        .leaf(leaf_index)
        .ok_or_else(|| Error::unsupported_feature(format!("no Arrow column {leaf_index}")))?;
    if !model.indexable {
        return Err(Error::unsupported_feature(
            "Arrow batch buffer layout is not statically indexable",
        ));
    }
    if !leaf.supported {
        return Err(Error::unsupported_feature(format!(
            "Arrow column {}: {}",
            leaf.name,
            leaf.decline.as_deref().unwrap_or("unsupported")
        )));
    }
    let mut budget = DecodeBudget {
        values_left: limits.max_arrow_values,
        bytes_left: limits.max_arrow_decompressed_bytes,
    };
    let mut out: Vec<Value> = Vec::new();
    for desc in &model.batches {
        if desc.compression >= 0 {
            return Err(Error::unsupported_feature(
                "Arrow body compression is not supported",
            ));
        }
        let meta = read_batch_meta(source, desc, limits)?;
        decode_leaf_column(source, desc, leaf, &meta, &mut budget, &mut out)?;
    }
    Ok(out)
}

/// Decode a single cell (row within the whole file, leaf column) by decoding the
/// containing record batch only.
pub fn cell_value(
    source: &[u8],
    model: &ArrowModel,
    row: u64,
    leaf_index: u32,
    limits: Limits,
) -> Result<Value> {
    let leaf = model
        .leaf(leaf_index)
        .ok_or_else(|| Error::unsupported_feature(format!("no Arrow column {leaf_index}")))?;
    if !model.indexable {
        return Err(Error::unsupported_feature(
            "Arrow batch buffer layout is not statically indexable",
        ));
    }
    if !leaf.supported {
        return Err(Error::unsupported_feature(format!(
            "Arrow column {}: {}",
            leaf.name,
            leaf.decline.as_deref().unwrap_or("unsupported")
        )));
    }
    let mut base: u64 = 0;
    for desc in &model.batches {
        let rows = u64::try_from(desc.num_rows).unwrap_or(0);
        if row < base.saturating_add(rows) {
            if desc.compression >= 0 {
                return Err(Error::unsupported_feature(
                    "Arrow body compression is not supported",
                ));
            }
            let meta = read_batch_meta(source, desc, limits)?;
            let mut budget = DecodeBudget {
                values_left: limits.max_arrow_values,
                bytes_left: limits.max_arrow_decompressed_bytes,
            };
            let mut vals: Vec<Value> = Vec::new();
            decode_leaf_column(source, desc, leaf, &meta, &mut budget, &mut vals)?;
            let local = usize::try_from(row - base)
                .map_err(|_| Error::unsupported_feature("Arrow row index is out of range"))?;
            return vals
                .get(local)
                .cloned()
                .ok_or_else(|| Error::unsupported_feature("Arrow row index is out of range"));
        }
        base = base.saturating_add(rows);
    }
    Err(Error::unsupported_feature(format!(
        "Arrow row {row} is out of range"
    )))
}

/// The exact source span covering a column's buffers across every batch.
pub fn column_span(
    source: &[u8],
    model: &ArrowModel,
    leaf_index: u32,
    limits: Limits,
) -> Result<Option<(u64, u64)>> {
    let leaf = model
        .leaf(leaf_index)
        .ok_or_else(|| Error::unsupported_feature(format!("no Arrow column {leaf_index}")))?;
    if !model.indexable || !leaf.supported {
        return Ok(None);
    }
    let mut lo = u64::MAX;
    let mut hi = 0u64;
    let mut any = false;
    for desc in &model.batches {
        if desc.compression >= 0 {
            return Err(Error::unsupported_feature(
                "Arrow body compression is not supported",
            ));
        }
        let meta = read_batch_meta(source, desc, limits)?;
        for sidx in [leaf.validity_slot, leaf.offsets_slot, leaf.data_slot] {
            if sidx < 0 {
                continue;
            }
            let b = slot(&meta.buffers, sidx)?;
            if b.1 == 0 {
                continue;
            }
            let s = buffer_slice(source, desc, b)?;
            let start = s.as_ptr() as u64 - source.as_ptr() as u64;
            lo = lo.min(start);
            hi = hi.max(start + s.len() as u64);
            any = true;
        }
    }
    Ok(if any { Some((lo, hi)) } else { None })
}

/// The raw bytes of a column's buffers across every batch, concatenated.
pub fn column_bytes(
    source: &[u8],
    model: &ArrowModel,
    leaf_index: u32,
    limits: Limits,
    budget: u64,
) -> Result<Vec<u8>> {
    let leaf = model
        .leaf(leaf_index)
        .ok_or_else(|| Error::unsupported_feature(format!("no Arrow column {leaf_index}")))?;
    if !model.indexable || !leaf.supported {
        return Err(Error::unsupported_feature(format!(
            "Arrow column {} is not decodable",
            leaf.name
        )));
    }
    let mut out: Vec<u8> = Vec::new();
    for desc in &model.batches {
        if desc.compression >= 0 {
            return Err(Error::unsupported_feature(
                "Arrow body compression is not supported",
            ));
        }
        let meta = read_batch_meta(source, desc, limits)?;
        for sidx in [leaf.validity_slot, leaf.offsets_slot, leaf.data_slot] {
            if sidx < 0 {
                continue;
            }
            let b = slot(&meta.buffers, sidx)?;
            let s = buffer_slice(source, desc, b)?;
            out.extend_from_slice(s);
            if out.len() as u64 > budget {
                return Err(Error::resource_limit(
                    "Arrow column bytes exceeded the output budget",
                ));
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Binary model reader/writer helpers
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

fn enc_opt_i64(out: &mut Vec<u8>, v: Option<i64>) {
    match v {
        Some(v) => {
            out.push(1);
            out.extend_from_slice(&v.to_le_bytes());
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
            .ok_or_else(|| corrupt("model read overflows"))?;
        let s = self
            .b
            .get(self.at..end)
            .ok_or_else(|| corrupt("model read is out of bounds"))?;
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
        let s = self.bytes(n)?;
        Ok(String::from_utf8_lossy(s).into_owned())
    }

    fn opt_str(&mut self) -> Result<Option<String>> {
        if self.u8()? == 0 {
            Ok(None)
        } else {
            Ok(Some(self.string()?))
        }
    }

    fn opt_i64(&mut self) -> Result<Option<i64>> {
        if self.u8()? == 0 {
            Ok(None)
        } else {
            Ok(Some(self.i64()?))
        }
    }

    fn at_end(&self) -> bool {
        self.at == self.b.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_converts_known_values() {
        assert_eq!(half_to_f32(0x0000), 0.0);
        assert_eq!(half_to_f32(0x3c00), 1.0);
        assert_eq!(half_to_f32(0xc000), -2.0);
        assert_eq!(half_to_f32(0x7c00), f32::INFINITY);
        assert_eq!(half_to_f32(0x3555), 0.33325195);
    }

    #[test]
    fn type_names_roundtrip() {
        assert_eq!(type_name(T_INT), "Int");
        assert_eq!(type_name(T_LARGE_UTF8), "LargeUtf8");
        assert_eq!(type_name(999), "UNKNOWN");
    }

    #[test]
    fn model_encode_decode_roundtrips() {
        let m = ArrowModel {
            doc_len: 128,
            version: 4,
            endianness: 0,
            stream: true,
            indexable: true,
            schema: vec![FieldNode {
                depth: 0,
                parent: u32::MAX,
                name: "a".to_string(),
                nullable: true,
                type_tag: T_INT,
                type_text: "Int(32, signed)".to_string(),
                bit_width: 32,
                int_signed: true,
                float_precision: -1,
                union_mode: -1,
                dict_id: None,
                num_children: 0,
            }],
            leaves: vec![LeafColumn {
                element: 0,
                node_index: 0,
                name: "a".to_string(),
                type_tag: T_INT,
                supported: true,
                decline: None,
                validity_slot: 0,
                offsets_slot: -1,
                data_slot: 1,
                width: 4,
                int_signed: true,
                float_precision: -1,
                is_utf8: false,
                large_offsets: false,
            }],
            batches: vec![BatchDesc {
                offset: 8,
                meta_len: 24,
                body_offset: 40,
                body_len: 40,
                span_start: 8,
                span_end: 80,
                num_rows: 10,
                num_nodes: 1,
                num_buffers: 2,
                compression: -1,
            }],
        };
        let enc = m.encode();
        let dec = ArrowModel::decode(&enc).unwrap();
        assert_eq!(m, dec);
    }

    #[test]
    fn detect_rejects_short_and_plain() {
        assert!(!detect(b"ARROW1", Limits::DEFAULT));
        assert!(!detect(b"not an arrow file at all", Limits::DEFAULT));
    }

    #[test]
    fn buffer_counts_are_static_for_flat_types() {
        assert_eq!(type_buffers(T_INT, -1), Some(2));
        assert_eq!(type_buffers(T_UTF8, -1), Some(3));
        assert_eq!(type_buffers(T_UTF8_VIEW, -1), None);
        assert_eq!(type_buffers(T_STRUCT, -1), Some(1));
    }
}
