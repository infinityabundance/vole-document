//! Scientific-container adapter (Phase 21.34): **NetCDF classic** (CDF-1 / CDF-2)
//! and a documented **bounded subset of HDF5**.
//!
//! Like every Wave-2 format, this adapter has **no separate package layer**: the
//! exact leaf is the whole source (a `DocumentExact`, RAW-like authority) and every
//! derived observation is a bounded, span-preserving (`Q_gen`) projection of it.
//! Nothing is decompressed, re-serialized, or rewritten for exactness; the model is
//! never on the exactness path.
//!
//! The model records, verbatim, the structures the format makes authoritative, with
//! each field's **exact source span**:
//!
//! * **NetCDF classic** (fully parseable and fully supported): the magic
//!   (`CDF\x01`/`CDF\x02`), `numrecs` (including the `STREAMING` sentinel), the
//!   dimension list (names + lengths + the unlimited/record dimension), the global
//!   attribute list, the variable list (name, dimensions, per-variable attributes,
//!   type, `vsize`, and the per-variable data `begin` offset), and the record/array
//!   data layout (the data-region span and each variable's exact data span).
//! * **HDF5** (a documented bounded subset; everything else declines **typed**):
//!   the superblock versions 0–3 (signature at offset 0, or the user-block signature
//!   at 512), the root group and its **symbol-table** link traversal down to
//!   **contiguous** datasets and nested groups, **v1 object headers** with their
//!   message list (including object-header continuations), the supported datatype
//!   classes (fixed-point / floating-point / string), the **simple** (and scalar)
//!   dataspace, the contiguous **data layout**, and **v1 attribute messages**.
//!
//! ## The boundary it cannot cross (honest)
//!
//! Detection is a **strong magic + validation** test; it never consults a file name:
//!
//! * NetCDF requires `CDF\x01`/`CDF\x02` at offset 0.
//! * HDF5 requires the 8-byte signature `\x89HDF\r\n\x1a\n` at offset 0 **or** at
//!   offset 512 (the user-block form), a superblock version in `0..=3`, offset/length
//!   sizes in `{2,4,8}`, and a superblock that fits inside the source.
//!
//! A random `\x89HDF`-prefixed blob with a bad superblock, prose, and every other
//! format stay `Opaque`.
//!
//! HDF5 is *much* larger than this subset. The adapter deliberately declines typed
//! (never guesses, never panics) on: superblock versions whose root group is only
//! reachable through the v2 link machinery; **v2/v3 object headers** (SWMR / compact /
//! checksummed); **compact / chunked / virtual** data layouts; **filtered** datasets
//! (any filter-pipeline message); **external data files**; and every datatype class
//! other than fixed-point, floating-point, and string (bitfield, opaque, compound,
//! reference, enum, variable-length, array). Unknown object-header message types also
//! decline typed. NetCDF classic is supported in full; NetCDF-4 (which is HDF5) is
//! only reached through the HDF5 subset above.

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model objects (defends the decoder against a hostile blob).
pub const MAX_MODEL_OBJECTS: u32 = 1 << 22;
/// Hard cap on decoded model dimensions (defends the decoder).
pub const MAX_MODEL_DIMENSIONS: u32 = 1 << 22;
/// Hard cap on decoded model attributes per list (defends the decoder).
pub const MAX_MODEL_ATTRIBUTES: u32 = 1 << 22;
/// Hard cap on a decoded name length (defends the decoder).
pub const MAX_MODEL_NAME_BYTES: u32 = 1 << 24;
/// Hard cap on a decoded attribute value length (defends the decoder).
pub const MAX_MODEL_ATTR_VALUE_BYTES: u32 = 1 << 24;

/// The recorded dialect: a NetCDF classic file (CDF-1 / CDF-2).
pub const DIALECT_NETCDF: u8 = 0;
/// The recorded dialect: an HDF5 file (the bounded subset).
pub const DIALECT_HDF5: u8 = 1;

/// Object kind: a group (HDF5 only).
pub const KIND_GROUP: u8 = 0;
/// Object kind: a dataset (an HDF5 dataset or a NetCDF variable).
pub const KIND_DATASET: u8 = 1;

/// The parent index of a root-level object (no parent).
pub const NO_PARENT: u32 = u32::MAX;

/// Normalized type: unknown / unspecified.
pub const T_UNKNOWN: u8 = 0;
/// Normalized type: 8-bit signed integer.
pub const T_I8: u8 = 1;
/// Normalized type: character.
pub const T_CHAR: u8 = 2;
/// Normalized type: 16-bit signed integer.
pub const T_I16: u8 = 3;
/// Normalized type: 32-bit signed integer.
pub const T_I32: u8 = 4;
/// Normalized type: 32-bit float.
pub const T_F32: u8 = 5;
/// Normalized type: 64-bit float.
pub const T_F64: u8 = 6;
/// Normalized type: 8-bit unsigned integer.
pub const T_U8: u8 = 7;
/// Normalized type: 16-bit unsigned integer.
pub const T_U16: u8 = 8;
/// Normalized type: 32-bit unsigned integer.
pub const T_U32: u8 = 9;
/// Normalized type: 64-bit unsigned integer.
pub const T_U64: u8 = 10;
/// Normalized type: 64-bit signed integer.
pub const T_I64: u8 = 11;
/// Normalized type: a string (HDF5 string datatype only).
pub const T_STRING: u8 = 12;

/// Match field: an object (group / dataset / variable) name.
pub const FIELD_OBJECT: u8 = 0;
/// Match field: a dimension name (NetCDF).
pub const FIELD_DIMENSION: u8 = 1;
/// Match field: an attribute name (global / root group / per object).
pub const FIELD_ATTRIBUTE: u8 = 2;
/// Match field: a group path component.
pub const FIELD_GROUP: u8 = 3;

/// Stable lower-case dialect name.
pub const fn dialect_name(dialect: u8) -> &'static str {
    match dialect {
        DIALECT_HDF5 => "hdf5",
        _ => "netcdf",
    }
}

/// Stable lower-case object-kind name.
pub const fn kind_name(kind: u8) -> &'static str {
    match kind {
        KIND_GROUP => "group",
        _ => "dataset",
    }
}

/// Stable lower-case normalized type name.
pub const fn type_name(ty: u8) -> &'static str {
    match ty {
        T_I8 => "int8",
        T_CHAR => "char",
        T_I16 => "int16",
        T_I32 => "int32",
        T_F32 => "float32",
        T_F64 => "float64",
        T_U8 => "uint8",
        T_U16 => "uint16",
        T_U32 => "uint32",
        T_U64 => "uint64",
        T_I64 => "int64",
        T_STRING => "string",
        _ => "unknown",
    }
}

/// Stable match-field name.
pub const fn field_name(field: u8) -> &'static str {
    match field {
        FIELD_DIMENSION => "dimension",
        FIELD_ATTRIBUTE => "attribute",
        FIELD_GROUP => "group",
        _ => "object",
    }
}

/// One NetCDF dimension (name + length + the unlimited flag).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HdfDimension {
    /// Raw name bytes (never decoded or normalized on the model path).
    pub name: Vec<u8>,
    /// `(offset, length)` of the name bytes in the source.
    pub name_span: (u64, u64),
    /// The declared length (0 for the unlimited/record dimension).
    pub length: u64,
    /// Whether this is the unlimited (record) dimension.
    pub unlimited: bool,
}

/// One attribute (global / root group / per object), with its exact value span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HdfAttribute {
    /// Raw name bytes.
    pub name: Vec<u8>,
    /// `(offset, length)` of the name bytes in the source.
    pub name_span: (u64, u64),
    /// Normalized element type.
    pub ty: u8,
    /// Declared element count.
    pub num_values: u64,
    /// `(offset, length)` of the value bytes in the source.
    pub data_span: (u64, u64),
    /// The raw value bytes (bounded).
    pub value: Vec<u8>,
}

/// One object: an HDF5 group / dataset, or a NetCDF variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HdfObject {
    /// Raw name bytes.
    pub name: Vec<u8>,
    /// `(offset, length)` of the name bytes in the source (HDF5: the heap string).
    pub name_span: (u64, u64),
    /// Index of the parent object, or [`NO_PARENT`] for a root-level object.
    pub parent: u32,
    /// Nesting depth (the root group is depth 0).
    pub depth: u32,
    /// [`KIND_GROUP`] or [`KIND_DATASET`].
    pub kind: u8,
    /// Normalized element type ([`KIND_DATASET`] only; `T_UNKNOWN` for a group).
    pub ty: u8,
    /// Element size in bytes ([`KIND_DATASET`] only; 0 for a group).
    pub elem_size: u64,
    /// The resolved shape (the dataspace dims / the variable shape).
    pub dims: Vec<u64>,
    /// The NetCDF dimension ids (empty for HDF5).
    pub dim_ids: Vec<u32>,
    /// Whether this is a NetCDF record variable (HDF5: always `false`).
    pub is_record: bool,
    /// Per-object attributes.
    pub attrs: Vec<HdfAttribute>,
    /// `(offset, length)` of the object header (HDF5); `(0,0)` for NetCDF.
    pub header_span: (u64, u64),
    /// `(offset, length)` of the object's data region.
    pub data_span: (u64, u64),
    /// The declared data offset (HDF5 data address / NetCDF `begin`).
    pub offset: u64,
}

/// The canonical derived model (the materialization of an `HdfModel` node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HdfModel {
    /// The recorded dialect ([`DIALECT_NETCDF`] or [`DIALECT_HDF5`]).
    pub dialect: u8,
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// The NetCDF `numrecs` field (0 for HDF5).
    pub numrecs: u64,
    /// Whether the NetCDF `numrecs` field carried the `STREAMING` sentinel.
    pub streaming: bool,
    /// `(offset, length)` of the header / superblock region.
    pub header_span: (u64, u64),
    /// `(offset, length)` of the data region (NetCDF record/array data; `(0,0)` for
    /// HDF5, whose datasets carry their own spans).
    pub data_span: (u64, u64),
    /// The NetCDF dimension table (empty for HDF5, whose dims are per dataset).
    pub dimensions: Vec<HdfDimension>,
    /// The global (NetCDF) / root-group (HDF5) attribute list.
    pub attributes: Vec<HdfAttribute>,
    /// The ordered object list (NetCDF variables; HDF5 groups + datasets).
    pub objects: Vec<HdfObject>,
}

impl HdfModel {
    /// The object at `index`, if present.
    pub fn object(&self, index: u32) -> Option<&HdfObject> {
        self.objects.get(index as usize)
    }

    /// The dimension at `index`, if present.
    pub fn dimension(&self, index: u32) -> Option<&HdfDimension> {
        self.dimensions.get(index as usize)
    }

    /// The global/root attribute at `index`, if present.
    pub fn attribute(&self, index: u32) -> Option<&HdfAttribute> {
        self.attributes.get(index as usize)
    }

    /// The number of datasets.
    pub fn dataset_count(&self) -> usize {
        self.objects
            .iter()
            .filter(|o| o.kind == KIND_DATASET)
            .count()
    }

    /// The number of groups.
    pub fn group_count(&self) -> usize {
        self.objects.iter().filter(|o| o.kind == KIND_GROUP).count()
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"HDFM");
        out.push(MODEL_VERSION);
        out.push(self.dialect);
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&self.numrecs.to_le_bytes());
        out.push(u8::from(self.streaming));
        out.extend_from_slice(&self.header_span.0.to_le_bytes());
        out.extend_from_slice(&self.header_span.1.to_le_bytes());
        out.extend_from_slice(&self.data_span.0.to_le_bytes());
        out.extend_from_slice(&self.data_span.1.to_le_bytes());
        push_dimensions(&mut out, &self.dimensions);
        push_attributes(&mut out, &self.attributes);
        out.extend_from_slice(&(self.objects.len() as u32).to_le_bytes());
        for o in &self.objects {
            push_name(&mut out, &o.name, o.name_span);
            out.extend_from_slice(&o.parent.to_le_bytes());
            out.extend_from_slice(&o.depth.to_le_bytes());
            out.push(o.kind);
            out.push(o.ty);
            out.extend_from_slice(&o.elem_size.to_le_bytes());
            push_u64_list(&mut out, &o.dims);
            push_u32_list(&mut out, &o.dim_ids);
            out.push(u8::from(o.is_record));
            push_attributes(&mut out, &o.attrs);
            out.extend_from_slice(&o.header_span.0.to_le_bytes());
            out.extend_from_slice(&o.header_span.1.to_le_bytes());
            out.extend_from_slice(&o.data_span.0.to_le_bytes());
            out.extend_from_slice(&o.data_span.1.to_le_bytes());
            out.extend_from_slice(&o.offset.to_le_bytes());
        }
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<HdfModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"HDFM" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let dialect = r.u8()?;
        if dialect > DIALECT_HDF5 {
            return Err(corrupt("unknown dialect"));
        }
        let doc_len = r.u64()?;
        let numrecs = r.u64()?;
        let streaming = match r.u8()? {
            0 => false,
            1 => true,
            _ => return Err(corrupt("invalid boolean")),
        };
        let header_span = (r.u64()?, r.u64()?);
        let data_span = (r.u64()?, r.u64()?);
        check_span(header_span, doc_len, "header")?;
        check_span(data_span, doc_len, "data")?;
        let dimensions = read_dimensions(&mut r, doc_len)?;
        let attributes = read_attributes(&mut r, doc_len, MAX_MODEL_ATTR_VALUE_BYTES)?;
        let count = r.u32()?;
        if count > MAX_MODEL_OBJECTS {
            return Err(corrupt("model object count is implausible"));
        }
        let mut objects = Vec::with_capacity(count.min(1024) as usize);
        for _ in 0..count {
            let (name, name_span) = read_name(&mut r, doc_len)?;
            let parent = r.u32()?;
            let depth = r.u32()?;
            let kind = r.u8()?;
            if kind > KIND_DATASET {
                return Err(corrupt("unknown object kind"));
            }
            let ty = r.u8()?;
            if ty > T_STRING {
                return Err(corrupt("unknown normalized type"));
            }
            let elem_size = r.u64()?;
            let dims = read_u64_list(&mut r, MAX_MODEL_DIMENSIONS)?;
            let dim_ids = read_u32_list(&mut r, MAX_MODEL_DIMENSIONS)?;
            let is_record = match r.u8()? {
                0 => false,
                1 => true,
                _ => return Err(corrupt("invalid boolean")),
            };
            let attrs = read_attributes(&mut r, doc_len, MAX_MODEL_ATTR_VALUE_BYTES)?;
            let header_span = (r.u64()?, r.u64()?);
            let data_span = (r.u64()?, r.u64()?);
            let offset = r.u64()?;
            check_span(header_span, doc_len, "object header")?;
            check_span(data_span, doc_len, "object data")?;
            if offset > doc_len {
                return Err(corrupt("object offset is outside the document"));
            }
            if parent != NO_PARENT && parent >= count {
                return Err(corrupt("object parent index is out of range"));
            }
            objects.push(HdfObject {
                name,
                name_span,
                parent,
                depth,
                kind,
                ty,
                elem_size,
                dims,
                dim_ids,
                is_record,
                attrs,
                header_span,
                data_span,
                offset,
            });
        }
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        Ok(HdfModel {
            dialect,
            doc_len,
            numrecs,
            streaming,
            header_span,
            data_span,
            dimensions,
            attributes,
            objects,
        })
    }
}

fn push_name(out: &mut Vec<u8>, name: &[u8], span: (u64, u64)) {
    out.extend_from_slice(&(name.len() as u32).to_le_bytes());
    out.extend_from_slice(name);
    out.extend_from_slice(&span.0.to_le_bytes());
    out.extend_from_slice(&span.1.to_le_bytes());
}

fn push_u64_list(out: &mut Vec<u8>, list: &[u64]) {
    out.extend_from_slice(&(list.len() as u32).to_le_bytes());
    for v in list {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

fn push_u32_list(out: &mut Vec<u8>, list: &[u32]) {
    out.extend_from_slice(&(list.len() as u32).to_le_bytes());
    for v in list {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

fn push_attributes(out: &mut Vec<u8>, attrs: &[HdfAttribute]) {
    out.extend_from_slice(&(attrs.len() as u32).to_le_bytes());
    for a in attrs {
        push_name(out, &a.name, a.name_span);
        out.push(a.ty);
        out.extend_from_slice(&a.num_values.to_le_bytes());
        out.extend_from_slice(&a.data_span.0.to_le_bytes());
        out.extend_from_slice(&a.data_span.1.to_le_bytes());
        out.extend_from_slice(&(a.value.len() as u32).to_le_bytes());
        out.extend_from_slice(&a.value);
    }
}

fn push_dimensions(out: &mut Vec<u8>, dims: &[HdfDimension]) {
    out.extend_from_slice(&(dims.len() as u32).to_le_bytes());
    for d in dims {
        push_name(out, &d.name, d.name_span);
        out.extend_from_slice(&d.length.to_le_bytes());
        out.push(u8::from(d.unlimited));
    }
}

fn read_name(r: &mut BinReader<'_>, doc_len: u64) -> Result<(Vec<u8>, (u64, u64))> {
    let n = r.u32()?;
    if n > MAX_MODEL_NAME_BYTES {
        return Err(corrupt("model name length is implausible"));
    }
    let name = r.bytes(n as usize)?.to_vec();
    let span = (r.u64()?, r.u64()?);
    check_span(span, doc_len, "name")?;
    Ok((name, span))
}

fn read_u64_list(r: &mut BinReader<'_>, cap: u32) -> Result<Vec<u64>> {
    let n = r.u32()?;
    if n > cap {
        return Err(corrupt("model list length is implausible"));
    }
    let mut out = Vec::with_capacity(n.min(1024) as usize);
    for _ in 0..n {
        out.push(r.u64()?);
    }
    Ok(out)
}

fn read_u32_list(r: &mut BinReader<'_>, cap: u32) -> Result<Vec<u32>> {
    let n = r.u32()?;
    if n > cap {
        return Err(corrupt("model list length is implausible"));
    }
    let mut out = Vec::with_capacity(n.min(1024) as usize);
    for _ in 0..n {
        out.push(r.u32()?);
    }
    Ok(out)
}

fn read_attributes(
    r: &mut BinReader<'_>,
    doc_len: u64,
    value_cap: u32,
) -> Result<Vec<HdfAttribute>> {
    let n = r.u32()?;
    if n > MAX_MODEL_ATTRIBUTES {
        return Err(corrupt("model attribute count is implausible"));
    }
    let mut out = Vec::with_capacity(n.min(1024) as usize);
    for _ in 0..n {
        let (name, name_span) = read_name(r, doc_len)?;
        let ty = r.u8()?;
        if ty > T_STRING {
            return Err(corrupt("unknown attribute type"));
        }
        let num_values = r.u64()?;
        let data_span = (r.u64()?, r.u64()?);
        check_span(data_span, doc_len, "attribute data")?;
        let vlen = r.u32()?;
        if vlen > value_cap {
            return Err(corrupt("model attribute value is implausible"));
        }
        let value = r.bytes(vlen as usize)?.to_vec();
        out.push(HdfAttribute {
            name,
            name_span,
            ty,
            num_values,
            data_span,
            value,
        });
    }
    Ok(out)
}

fn check_span(span: (u64, u64), doc_len: u64, what: &str) -> Result<()> {
    if span.0 > span.1 || span.1 > doc_len {
        return Err(corrupt(format!("{what} span is outside the document")));
    }
    Ok(())
}

fn read_dimensions(r: &mut BinReader<'_>, doc_len: u64) -> Result<Vec<HdfDimension>> {
    let n = r.u32()?;
    if n > MAX_MODEL_DIMENSIONS {
        return Err(corrupt("model dimension count is implausible"));
    }
    let mut out = Vec::with_capacity(n.min(1024) as usize);
    for _ in 0..n {
        let (name, name_span) = read_name(r, doc_len)?;
        let length = r.u64()?;
        let unlimited = match r.u8()? {
            0 => false,
            1 => true,
            _ => return Err(corrupt("invalid boolean")),
        };
        out.push(HdfDimension {
            name,
            name_span,
            length,
            unlimited,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Public byte helpers (exact source spans)
// ---------------------------------------------------------------------------

/// The exact raw bytes of an object's data region (`KIND_DATASET` only).
pub fn object_bytes<'a>(source: &'a [u8], obj: &HdfObject) -> Result<&'a [u8]> {
    if obj.kind != KIND_DATASET {
        return Err(Error::unsupported_feature(
            "a group object has no data bytes",
        ));
    }
    slice(source, obj.data_span.0, obj.data_span.1, "object data span")
}

/// The exact raw bytes of an attribute value.
pub fn attribute_bytes<'a>(source: &'a [u8], attr: &HdfAttribute) -> Result<&'a [u8]> {
    slice(
        source,
        attr.data_span.0,
        attr.data_span.1,
        "attribute data span",
    )
}

/// The full path of an object (group components joined with `/`, `/`-rooted). Built
/// from the parent chain; bounded by the object count.
pub fn object_path(model: &HdfModel, index: u32) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut cur = Some(index);
    let mut guard = 0u32;
    while let Some(i) = cur {
        let Some(obj) = model.object(i) else { break };
        parts.push(String::from_utf8_lossy(&obj.name).into_owned());
        cur = if obj.parent == NO_PARENT {
            None
        } else {
            Some(obj.parent)
        };
        guard += 1;
        if guard > model.objects.len() as u32 + 1 {
            break;
        }
    }
    parts.reverse();
    let joined: Vec<String> = parts.into_iter().filter(|p| !p.is_empty()).collect();
    if joined.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", joined.join("/"))
    }
}

/// A compact canonical text rendering of the model (the common `text` observation).
/// Bounded by `max_bytes`.
pub fn canonical_text(model: &HdfModel, _source: &[u8], max_bytes: u64) -> Result<String> {
    let mut out = String::new();
    out.push_str(&format!(
        "dialect\t{}\tobjects\t{}\tdimensions\t{}\tattributes\t{}\n",
        dialect_name(model.dialect),
        model.objects.len(),
        model.dimensions.len(),
        model.attributes.len(),
    ));
    for (i, d) in model.dimensions.iter().enumerate() {
        out.push_str(&format!(
            "dim\t{i}\t{}\t{}\t{}\n",
            String::from_utf8_lossy(&d.name),
            d.length,
            d.unlimited
        ));
    }
    for (i, a) in model.attributes.iter().enumerate() {
        out.push_str(&format!(
            "attr\t{i}\t{}\t{}\t{}\n",
            String::from_utf8_lossy(&a.name),
            type_name(a.ty),
            a.num_values
        ));
    }
    for (i, o) in model.objects.iter().enumerate() {
        let shape = o
            .dims
            .iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join("x");
        out.push_str(&format!(
            "obj\t{i}\t{}\t{}\t{}\t{}\t{}\n",
            kind_name(o.kind),
            object_path(model, i as u32),
            type_name(o.ty),
            shape,
            o.data_span.1.saturating_sub(o.data_span.0),
        ));
        if out.len() as u64 > max_bytes {
            return Err(Error::resource_limit(format!(
                "hdf canonical text exceeded the {max_bytes}-byte budget"
            )));
        }
    }
    Ok(out)
}

/// One lexical-match record from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HdfMatch {
    /// The object index (`FIELD_OBJECT`/`FIELD_GROUP`), dimension index
    /// (`FIELD_DIMENSION`), or global-attribute index (`FIELD_ATTRIBUTE`).
    pub entry: u32,
    /// One of the `FIELD_*` tags.
    pub field: u8,
    /// The match's exact source byte offset.
    pub start: u64,
    /// One past the match's last source byte.
    pub end: u64,
    /// The matched text.
    pub text: String,
}

/// A bounded, case-sensitive lexical search over every object/dimension/attribute
/// name (and every per-object attribute name).
pub fn find(model: &HdfModel, pattern: &str, max_bytes: u64) -> Result<Vec<HdfMatch>> {
    let needle = pattern.as_bytes();
    if needle.is_empty() {
        return Ok(Vec::new());
    }
    let mut out: Vec<HdfMatch> = Vec::new();
    let mut estimated: u64 = 0;
    let mut scan = |field: u8, entry: u32, hay: &[u8], anchor: (u64, u64)| -> Result<()> {
        let mut at = 0usize;
        while at + needle.len() <= hay.len() {
            if &hay[at..at + needle.len()] == needle {
                estimated = estimated.saturating_add(64 + needle.len() as u64);
                if estimated > max_bytes {
                    return Err(Error::resource_limit(format!(
                        "hdf find exceeded the {max_bytes}-byte budget"
                    )));
                }
                let start = anchor.0.saturating_add(at as u64);
                let end = start.saturating_add(needle.len() as u64);
                out.push(HdfMatch {
                    entry,
                    field,
                    start,
                    end,
                    text: String::from_utf8_lossy(&hay[at..at + needle.len()]).into_owned(),
                });
                at += needle.len();
            } else {
                at += 1;
            }
        }
        Ok(())
    };
    for (i, d) in model.dimensions.iter().enumerate() {
        scan(FIELD_DIMENSION, i as u32, &d.name, d.name_span)?;
    }
    for (i, a) in model.attributes.iter().enumerate() {
        scan(FIELD_ATTRIBUTE, i as u32, &a.name, a.name_span)?;
    }
    for (i, o) in model.objects.iter().enumerate() {
        let field = if o.kind == KIND_GROUP {
            FIELD_GROUP
        } else {
            FIELD_OBJECT
        };
        scan(field, i as u32, &o.name, o.name_span)?;
        for a in &o.attrs {
            scan(FIELD_ATTRIBUTE, i as u32, &a.name, a.name_span)?;
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// The 8-byte HDF5 signature.
pub const HDF5_SIGNATURE: [u8; 8] = [0x89, b'H', b'D', b'F', b'\r', b'\n', 0x1a, b'\n'];
/// The 4-byte NetCDF classic magics.
pub const NETCDF_CDF1: [u8; 4] = [b'C', b'D', b'F', 0x01];
/// The 4-byte NetCDF CDF-2 magic.
pub const NETCDF_CDF2: [u8; 4] = [b'C', b'D', b'F', 0x02];

/// Fast-fail scientific-container detector: a NetCDF classic magic, or a validated
/// HDF5 superblock. Never consults a file name.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.len() as u64 > limits.max_hdf_document_bytes {
        return false;
    }
    detect_netcdf(source) || detect_hdf5(source).is_some()
}

/// Whether `source` begins with a NetCDF classic (CDF-1 / CDF-2) magic.
pub fn detect_netcdf(source: &[u8]) -> bool {
    source.len() >= 4 && (source.starts_with(&NETCDF_CDF1) || source.starts_with(&NETCDF_CDF2))
}

/// Validate an HDF5 superblock and return its parsed form, or `None`. Requires the
/// signature at offset 0 or 512, a version in `0..=3`, offset/length sizes in
/// `{2,4,8}`, and a superblock that fits inside the source.
pub fn detect_hdf5(source: &[u8]) -> Option<Superblock> {
    let base = superblock_offset(source)?;
    parse_superblock(source, base).ok()
}

/// The signature offset (0, or 512 for the user-block form), or `None`.
fn superblock_offset(source: &[u8]) -> Option<usize> {
    if source.len() >= 8 && source[..8] == HDF5_SIGNATURE {
        Some(0)
    } else if source.len() >= 520 && source[512..520] == HDF5_SIGNATURE {
        Some(512)
    } else {
        None
    }
}

/// The parsed HDF5 superblock header (the fields detection validates).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Superblock {
    /// The superblock version (0..=3).
    pub version: u8,
    /// The size of file offsets in bytes (2, 4, or 8).
    pub offset_size: u8,
    /// The size of file lengths in bytes (2, 4, or 8).
    pub length_size: u8,
    /// The signature offset (0 or 512).
    pub base_offset: u64,
    /// The declared EOF address (0 if undefined).
    pub eof_address: u64,
    /// The root group symbol table entry address (v0/v1) or the root group object
    /// header address (v2/v3).
    pub root_address: u64,
    /// The group leaf-node K (v0/v1; 0 for v2/v3).
    pub group_leaf_k: u16,
}

fn valid_size(size: u8) -> bool {
    matches!(size, 2 | 4 | 8)
}

/// Parse the HDF5 superblock at `base`. Fail-closed: any malformed field is an error.
fn parse_superblock(source: &[u8], base: usize) -> Result<Superblock> {
    let base_u64 = base as u64;
    let version = get_u8(source, base + 8)?;
    match version {
        0 | 1 => {
            let off = get_u8(source, base + 13)?;
            let len = get_u8(source, base + 14)?;
            if !valid_size(off) || !valid_size(len) {
                return Err(decline("unsupported HDF5 offset/length size"));
            }
            let group_leaf_k = get_u16_le(source, base + 16)?;
            // Fixed header: 24 bytes (v0) or 28 (v1), then four offsets, then the
            // root group symbol table entry.
            let offs_at = (base + if version == 1 { 28 } else { 24 }) as u64;
            let _base_addr = read_off_at(source, offs_at, off)?;
            let _free_addr = read_off_at(source, offs_at + off as u64, off)?;
            let eof_address = read_off_at(source, offs_at + 2 * off as u64, off)?;
            let _driver = read_off_at(source, offs_at + 3 * off as u64, off)?;
            let ste_at = offs_at + 4 * off as u64;
            // The root symbol table entry: link name offset, object header address.
            let _link = read_off_at(source, ste_at, off)?;
            let root_address = read_off_at(source, ste_at + off as u64, off)?;
            validate_superblock_end(source, eof_address, off, base_u64)?;
            Ok(Superblock {
                version,
                offset_size: off,
                length_size: len,
                base_offset: base_u64,
                eof_address,
                root_address,
                group_leaf_k,
            })
        }
        2 | 3 => {
            let off = get_u8(source, base + 9)?;
            let len = get_u8(source, base + 10)?;
            if !valid_size(off) || !valid_size(len) {
                return Err(decline("unsupported HDF5 offset/length size"));
            }
            // base address, superblock extension address, EOF address, root group
            // object header address, then a 4-byte checksum.
            let offs_at = base_u64 + 12;
            let _base_addr = read_off_at(source, offs_at, off)?;
            let _ext_addr = read_off_at(source, offs_at + off as u64, off)?;
            let eof_address = read_off_at(source, offs_at + 2 * off as u64, off)?;
            let root_address = read_off_at(source, offs_at + 3 * off as u64, off)?;
            validate_superblock_end(source, eof_address, off, base_u64)?;
            Ok(Superblock {
                version,
                offset_size: off,
                length_size: len,
                base_offset: base_u64,
                eof_address,
                root_address,
                group_leaf_k: 0,
            })
        }
        _ => Err(decline("unsupported HDF5 superblock version")),
    }
}

/// The byte length of a v0/v1 superblock header.
fn superblock_len(version: u8, off: u8) -> u64 {
    let fixed = if version == 1 { 28u64 } else { 24u64 };
    fixed + 4 * off as u64 + (2 * off as u64 + 24)
}

fn validate_superblock_end(
    source: &[u8],
    eof_address: u64,
    offset_size: u8,
    base: u64,
) -> Result<()> {
    // The declared EOF address, when defined, must not exceed the source. The HDF5
    // "undefined address" sentinel is all-ones.
    let undefined = match offset_size {
        2 => 0xffffu64,
        4 => 0xffff_ffff,
        _ => 0xffff_ffff_ffff_ffff,
    };
    if eof_address != undefined && eof_address > source.len() as u64 {
        return Err(decline("declared end-of-file address is out of bounds"));
    }
    if base > source.len() as u64 {
        return Err(decline("superblock offset is out of bounds"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Parse entry point
// ---------------------------------------------------------------------------

/// Parse `source` into an [`HdfModel`] under `limits`. Declines typed on any
/// malformed or over-large input; never panics.
pub fn parse(source: &[u8], limits: Limits) -> Result<HdfModel> {
    if source.len() as u64 > limits.max_hdf_document_bytes {
        return Err(Error::resource_limit(format!(
            "hdf source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_hdf_document_bytes
        )));
    }
    if detect_netcdf(source) {
        return build_netcdf_model(source, limits);
    }
    let sb = detect_hdf5(source).ok_or_else(|| {
        invalid_hdf("source is neither a NetCDF classic file nor a valid HDF5 superblock")
    })?;
    build_hdf5_model(source, limits, sb)
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of an `HdfModel` node).
pub fn build_hdf_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits)?.encode())
}

// ---------------------------------------------------------------------------
// NetCDF classic parser (full)
// ---------------------------------------------------------------------------

const NC_DIMENSION: u32 = 0x0A;
const NC_VARIABLE: u32 = 0x0B;
const NC_ATTRIBUTE: u32 = 0x0C;

/// A position-tracking reader for the NetCDF classic parser.
struct P<'a> {
    b: &'a [u8],
    at: usize,
    /// 4 for CDF-1, 8 for CDF-2.
    off: u8,
    limits: Limits,
    name_bytes: u64,
}

impl<'a> P<'a> {
    fn new(b: &'a [u8], version: u8, limits: Limits) -> Self {
        P {
            b,
            at: 0,
            off: if version == 2 { 8 } else { 4 },
            limits,
            name_bytes: 0,
        }
    }

    fn pos(&self) -> u64 {
        self.at as u64
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| invalid_hdf("netcdf length overflow"))?;
        let out = self
            .b
            .get(self.at..end)
            .ok_or_else(|| invalid_hdf("netcdf input ended early"))?;
        self.at = end;
        Ok(out)
    }

    fn u32_be(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64_be(&mut self) -> Result<u64> {
        let b = self.take(8)?;
        Ok(u64::from_be_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    /// A non-negative `INT` (always 32-bit) — counts and `nelems`.
    fn int(&mut self) -> Result<u64> {
        Ok(self.u32_be()? as u64)
    }

    /// A `NON_NEG`: 32-bit in CDF-1, 64-bit in CDF-2.
    fn nonneg(&mut self) -> Result<u64> {
        if self.off == 8 {
            self.u64_be()
        } else {
            Ok(self.u32_be()? as u64)
        }
    }

    /// An `OFFSET`: 32-bit in CDF-1, 64-bit in CDF-2.
    fn offset(&mut self) -> Result<u64> {
        self.nonneg()
    }

    /// An `NC_string`: a 32-bit length then that many bytes (span recorded). The stored
    /// name excludes the trailing NUL the format writes; the span covers the name
    /// characters.
    fn nc_string(&mut self) -> Result<(Vec<u8>, (u64, u64))> {
        let n = self.u32_be()? as u64;
        self.name_bytes = self.name_bytes.saturating_add(n);
        if self.name_bytes > self.limits.max_hdf_name_bytes {
            return Err(Error::resource_limit(format!(
                "netcdf name bytes exceed max_hdf_name_bytes {}",
                self.limits.max_hdf_name_bytes
            )));
        }
        let start = self.pos();
        if n > self.limits.max_hdf_entry_name_bytes {
            return Err(Error::resource_limit(format!(
                "netcdf name {n} bytes exceeds max_hdf_entry_name_bytes {}",
                self.limits.max_hdf_entry_name_bytes
            )));
        }
        let raw = self.take(n as usize)?;
        let name = match raw.split_last() {
            Some((0, rest)) => rest.to_vec(),
            _ => raw.to_vec(),
        };
        let end = start + name.len() as u64;
        Ok((name, (start, end)))
    }
}

/// The netCDF classic element size for an on-disk type code.
fn nc_type(ty: u32) -> Result<(u8, u64)> {
    Ok(match ty {
        1 => (T_I8, 1),
        2 => (T_CHAR, 1),
        3 => (T_I16, 2),
        4 => (T_I32, 4),
        5 => (T_F32, 4),
        6 => (T_F64, 8),
        _ => return Err(decline("unsupported NetCDF type")),
    })
}

fn round_up_4(n: u64) -> u64 {
    n.saturating_add(3) & !3
}

fn build_netcdf_model(source: &[u8], limits: Limits) -> Result<HdfModel> {
    let version = source[3];
    let mut p = P::new(source, version, limits);
    let magic = p.take(4)?;
    debug_assert!(magic == NETCDF_CDF1 || magic == NETCDF_CDF2);
    let numrecs = p.nonneg()?;
    let streaming =
        (version == 1 && numrecs == u64::from(u32::MAX)) || (version == 2 && numrecs == u64::MAX);

    // Dimension list.
    let mut dimensions: Vec<HdfDimension> = Vec::new();
    let dim_tag = p.u32_be()?;
    if dim_tag != 0 {
        if dim_tag != NC_DIMENSION {
            return Err(invalid_hdf("netcdf dimension list has a bad tag"));
        }
        let nelems = p.int()?;
        if nelems > limits.max_hdf_dimensions {
            return Err(Error::resource_limit(format!(
                "netcdf declares {nelems} dimensions, exceeding max_hdf_dimensions {}",
                limits.max_hdf_dimensions
            )));
        }
        for _ in 0..nelems {
            let (name, name_span) = p.nc_string()?;
            let length = p.nonneg()?;
            dimensions.push(HdfDimension {
                name,
                name_span,
                length,
                unlimited: length == 0,
            });
        }
    }

    // Attribute lists share one aggregate cap across global + per-variable attributes.
    let mut attr_total = 0u64;

    // Global attribute list.
    let attributes = read_nc_attrs(&mut p, limits, &mut attr_total)?;

    // Variable list.
    let mut objects: Vec<HdfObject> = Vec::new();
    let var_tag = p.u32_be()?;
    let mut data_starts: Vec<u64> = Vec::new();
    if var_tag != 0 {
        if var_tag != NC_VARIABLE {
            return Err(invalid_hdf("netcdf variable list has a bad tag"));
        }
        let nelems = p.int()?;
        if nelems > limits.max_hdf_objects {
            return Err(Error::resource_limit(format!(
                "netcdf declares {nelems} variables, exceeding max_hdf_objects {}",
                limits.max_hdf_objects
            )));
        }
        for _ in 0..nelems {
            let (name, name_span) = p.nc_string()?;
            let ndims = p.int()?;
            if ndims > limits.max_hdf_dimensions {
                return Err(Error::resource_limit(
                    "netcdf variable has an implausible rank",
                ));
            }
            let mut dim_ids: Vec<u32> = Vec::with_capacity(ndims.min(64) as usize);
            for _ in 0..ndims {
                dim_ids.push(p.u32_be()?);
            }
            let attrs = read_nc_attrs(&mut p, limits, &mut attr_total)?;
            let ty_code = p.u32_be()?;
            let (ty, elem_size) = nc_type(ty_code)?;
            let vsize = p.nonneg()?;
            let begin = p.offset()?;
            // Resolve the shape from the dimension table.
            let mut dims: Vec<u64> = Vec::with_capacity(dim_ids.len());
            for &d in &dim_ids {
                let dl = dimensions
                    .get(d as usize)
                    .map(|x| x.length)
                    .ok_or_else(|| {
                        invalid_hdf("netcdf variable references an unknown dimension")
                    })?;
                dims.push(dl);
            }
            let is_record = dim_ids
                .first()
                .and_then(|&d| dimensions.get(d as usize))
                .map(|x| x.unlimited)
                .unwrap_or(false);
            if begin > source.len() as u64 {
                return Err(invalid_hdf("netcdf variable data offset is out of bounds"));
            }
            let data_end = begin.saturating_add(vsize).min(source.len() as u64);
            if vsize > 0 {
                data_starts.push(begin);
            }
            objects.push(HdfObject {
                name,
                name_span,
                parent: NO_PARENT,
                depth: 0,
                kind: KIND_DATASET,
                ty,
                elem_size,
                dims,
                dim_ids,
                is_record,
                attrs,
                header_span: (0, 0),
                data_span: (begin, data_end),
                offset: begin,
            });
        }
    }

    let header_end = p.pos();
    let data_start = data_starts.iter().copied().min().unwrap_or(header_end);
    let data_start = data_start.max(header_end).min(source.len() as u64);
    Ok(HdfModel {
        dialect: DIALECT_NETCDF,
        doc_len: source.len() as u64,
        numrecs,
        streaming,
        header_span: (0, header_end),
        data_span: (data_start, source.len() as u64),
        dimensions,
        attributes,
        objects,
    })
}

fn read_nc_attrs(p: &mut P<'_>, limits: Limits, total: &mut u64) -> Result<Vec<HdfAttribute>> {
    let mut out = Vec::new();
    let tag = p.u32_be()?;
    if tag == 0 {
        return Ok(out);
    }
    if tag != NC_ATTRIBUTE {
        return Err(invalid_hdf("netcdf attribute list has a bad tag"));
    }
    let nelems = p.int()?;
    for _ in 0..nelems {
        *total = total.saturating_add(1);
        if *total > limits.max_hdf_attributes {
            return Err(Error::resource_limit(format!(
                "netcdf attribute count exceeds max_hdf_attributes {}",
                limits.max_hdf_attributes
            )));
        }
        let (name, name_span) = p.nc_string()?;
        let ty_code = p.u32_be()?;
        let (ty, elem_size) = nc_type(ty_code)?;
        let count = p.nonneg()?;
        let data_len = count.saturating_mul(elem_size);
        if data_len > limits.max_hdf_attr_value_bytes {
            return Err(Error::resource_limit(format!(
                "netcdf attribute value {data_len} bytes exceeds max_hdf_attr_value_bytes {}",
                limits.max_hdf_attr_value_bytes
            )));
        }
        let start = p.pos();
        let value = p.take(data_len as usize)?.to_vec();
        let end = p.pos();
        // Attribute values are padded to a 4-byte boundary.
        let pad = round_up_4(data_len).saturating_sub(data_len) as usize;
        p.take(pad)?;
        out.push(HdfAttribute {
            name,
            name_span,
            ty,
            num_values: count,
            data_span: (start, end),
            value,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// HDF5 parser (bounded subset)
// ---------------------------------------------------------------------------

const MSG_NIL: u16 = 0x0000;
const MSG_DATASPACE: u16 = 0x0001;
const MSG_LINK_INFO: u16 = 0x0002;
const MSG_DATATYPE: u16 = 0x0003;
const MSG_FILL_OLD: u16 = 0x0004;
const MSG_FILL: u16 = 0x0005;
const MSG_LINK: u16 = 0x0006;
const MSG_EXTERNAL: u16 = 0x0007;
const MSG_LAYOUT: u16 = 0x0008;
const MSG_BOGUS: u16 = 0x0009;
const MSG_GROUP_INFO: u16 = 0x000A;
const MSG_FILTER: u16 = 0x000B;
const MSG_ATTRIBUTE: u16 = 0x000C;
const MSG_COMMENT: u16 = 0x000D;
const MSG_MTIME_OLD: u16 = 0x000E;
const MSG_SHARED_TABLE: u16 = 0x000F;
const MSG_CONTINUATION: u16 = 0x0010;
const MSG_SYMBOL_TABLE: u16 = 0x0011;
const MSG_MTIME: u16 = 0x0012;
const MSG_BTREE_K: u16 = 0x0013;
const MSG_DRIVER_INFO: u16 = 0x0014;
const MSG_ATTR_INFO: u16 = 0x0015;
const MSG_REF_COUNT: u16 = 0x0016;

const DT_FIXED: u8 = 0x0;
const DT_FLOAT: u8 = 0x1;
const DT_STRING: u8 = 0x3;

const MAX_HEADER_MESSAGES: u64 = 1 << 16;
const MAX_CONTINUATIONS: u64 = 1 << 8;
const MAX_BTREE_DEPTH: u32 = 32;
const MAX_BTREE_NODES: u64 = 1 << 20;

/// One parsed object-header message (its exact body span).
#[derive(Debug, Clone, Copy)]
struct Msg {
    ty: u16,
    body: (u64, u64),
}

/// A shared HDF5 parse state (source, limits, running counters).
struct H<'a> {
    b: &'a [u8],
    limits: Limits,
    off: usize,
    len: usize,
    sb: Superblock,
    objects: Vec<HdfObject>,
    attributes: Vec<HdfAttribute>,
    attr_total: u64,
    name_bytes: u64,
    btree_nodes: u64,
}

impl<'a> H<'a> {
    fn name_add(&mut self, n: u64) -> Result<()> {
        self.name_bytes = self.name_bytes.saturating_add(n);
        if self.name_bytes > self.limits.max_hdf_name_bytes {
            return Err(Error::resource_limit(format!(
                "hdf5 name bytes exceed max_hdf_name_bytes {}",
                self.limits.max_hdf_name_bytes
            )));
        }
        Ok(())
    }

    fn off_at(&self, at: u64) -> Result<u64> {
        read_off_at(self.b, at, self.off as u8)
    }

    fn len_at(&self, at: u64) -> Result<u64> {
        read_len_at(self.b, at, self.len as u8)
    }

    /// Read the v1 object header at `addr` into its message list.
    fn read_object_header(&self, addr: u64) -> Result<Vec<Msg>> {
        let a = addr as usize;
        let version = get_u8(self.b, a)?;
        if version != 1 {
            return Err(decline(format!(
                "unsupported HDF5 object header version {version} (only v1 is in the supported subset)"
            )));
        }
        let num_messages = get_u16_le(self.b, a + 2)? as u64;
        if num_messages > MAX_HEADER_MESSAGES {
            return Err(Error::resource_limit(
                "hdf5 object header declares too many messages",
            ));
        }
        let header_size = get_u32_le(self.b, a + 8)? as u64;
        let msg_start = addr
            .checked_add(12)
            .ok_or_else(|| invalid_hdf("hdf5 object header offset overflow"))?;
        let inline_end = msg_start
            .checked_add(header_size)
            .ok_or_else(|| invalid_hdf("hdf5 object header size overflow"))?;
        if inline_end > self.b.len() as u64 {
            return Err(invalid_hdf("hdf5 object header extends past the file"));
        }
        let mut blocks: Vec<(u64, u64)> = vec![(msg_start, inline_end)];
        let mut remaining = num_messages;
        let mut continuations = 0u64;
        let mut out: Vec<Msg> = Vec::new();
        while remaining > 0 {
            let Some(&(p, end)) = blocks.first() else {
                break;
            };
            if p >= end {
                blocks.remove(0);
                continue;
            }
            if end - p < 8 {
                return Err(invalid_hdf("hdf5 object header message is truncated"));
            }
            let ty = get_u16_le(self.b, p as usize)?;
            let size = get_u16_le(self.b, p as usize + 2)? as u64;
            let _flags = get_u8(self.b, p as usize + 4)?;
            let body_start = p + 8;
            let body_end = body_start
                .checked_add(size)
                .ok_or_else(|| invalid_hdf("hdf5 message size overflow"))?;
            if body_end > end {
                return Err(invalid_hdf("hdf5 object header message overruns its block"));
            }
            blocks[0].0 = body_end;
            remaining -= 1;
            match ty {
                MSG_NIL => {}
                MSG_CONTINUATION => {
                    continuations += 1;
                    if continuations > MAX_CONTINUATIONS {
                        return Err(Error::resource_limit(
                            "hdf5 object header has too many continuations",
                        ));
                    }
                    let caddr = self.off_at(body_start)?;
                    let clen = self.len_at(body_start + self.off as u64)?;
                    let cend = caddr
                        .checked_add(clen)
                        .ok_or_else(|| invalid_hdf("hdf5 continuation size overflow"))?;
                    if cend > self.b.len() as u64 {
                        return Err(invalid_hdf("hdf5 continuation extends past the file"));
                    }
                    blocks.push((caddr, cend));
                }
                _ => out.push(Msg {
                    ty,
                    body: (body_start, body_end),
                }),
            }
        }
        if remaining != 0 {
            return Err(invalid_hdf(
                "hdf5 object header message count is unsatisfied",
            ));
        }
        Ok(out)
    }
}

fn expect_benign_or_decline(ty: u16) -> Result<()> {
    match ty {
        MSG_FILL_OLD | MSG_FILL | MSG_BOGUS | MSG_GROUP_INFO | MSG_COMMENT | MSG_MTIME_OLD
        | MSG_SHARED_TABLE | MSG_MTIME | MSG_BTREE_K | MSG_DRIVER_INFO | MSG_ATTR_INFO
        | MSG_REF_COUNT => Ok(()),
        MSG_EXTERNAL => Err(decline(
            "HDF5 external data files are outside the supported subset",
        )),
        MSG_FILTER => Err(decline(
            "HDF5 filtered datasets are outside the supported subset",
        )),
        MSG_LINK_INFO | MSG_LINK => Err(decline(
            "HDF5 v2 link messages (linked groups) are outside the supported subset",
        )),
        MSG_DATASPACE | MSG_DATATYPE | MSG_LAYOUT | MSG_ATTRIBUTE | MSG_SYMBOL_TABLE
        | MSG_CONTINUATION => Ok(()),
        other => Err(decline(format!(
            "unsupported HDF5 object header message type 0x{other:04x}"
        ))),
    }
}

fn build_hdf5_model(source: &[u8], limits: Limits, sb: Superblock) -> Result<HdfModel> {
    if sb.version >= 2 {
        return Err(decline(
            "HDF5 superblock v2/v3 (v2 object headers / link info) is outside the supported subset",
        ));
    }
    let mut h = H {
        b: source,
        limits,
        off: sb.offset_size as usize,
        len: sb.length_size as usize,
        sb,
        objects: Vec::new(),
        attributes: Vec::new(),
        attr_total: 0,
        name_bytes: 0,
        btree_nodes: 0,
    };
    if sb.root_address == 0 || sb.root_address >= source.len() as u64 {
        return Err(invalid_hdf("hdf5 root group address is out of bounds"));
    }
    // The root group itself is object 0.
    read_group(&mut h, sb.root_address, Vec::new(), (0, 0), NO_PARENT, 0)?;
    let sb_len = superblock_len(sb.version, sb.offset_size);
    let header_span = (
        sb.base_offset,
        (sb.base_offset + sb_len).min(source.len() as u64),
    );
    Ok(HdfModel {
        dialect: DIALECT_HDF5,
        doc_len: source.len() as u64,
        numrecs: 0,
        streaming: false,
        header_span,
        data_span: (0, 0),
        dimensions: Vec::new(),
        attributes: std::mem::take(&mut h.attributes),
        objects: h.objects,
    })
}

/// Read one group (its object header, attributes, and — via the symbol table —
/// its members). `parent` is the parent object index; a root group passes
/// [`NO_PARENT`].
fn read_group(
    h: &mut H<'_>,
    addr: u64,
    name: Vec<u8>,
    name_span: (u64, u64),
    parent: u32,
    depth: u32,
) -> Result<()> {
    if depth > h.limits.max_hdf_depth {
        return Err(Error::resource_limit(format!(
            "hdf5 group depth {depth} exceeds max_hdf_depth {}",
            h.limits.max_hdf_depth
        )));
    }
    if h.objects.len() as u64 >= h.limits.max_hdf_objects {
        return Err(Error::resource_limit(format!(
            "hdf5 object count exceeds max_hdf_objects {}",
            h.limits.max_hdf_objects
        )));
    }
    let msgs = h.read_object_header(addr)?;
    let mut attrs: Vec<HdfAttribute> = Vec::new();
    let mut symbol: Option<(u64, u64)> = None;
    for m in &msgs {
        expect_benign_or_decline(m.ty)?;
        match m.ty {
            MSG_SYMBOL_TABLE => {
                let btree = h.off_at(m.body.0)?;
                let heap = h.off_at(m.body.0 + h.off as u64)?;
                symbol = Some((btree, heap));
            }
            MSG_ATTRIBUTE => attrs.push(read_attribute(h, m.body)?),
            _ => {}
        }
    }
    let kind = KIND_GROUP;
    let my_index = h.objects.len() as u32;
    let header_span = (
        addr,
        addr.saturating_add(12 + get_u32_le(h.b, addr as usize + 8)? as u64),
    );
    h.objects.push(HdfObject {
        name,
        name_span,
        parent,
        depth,
        kind,
        ty: T_UNKNOWN,
        elem_size: 0,
        dims: Vec::new(),
        dim_ids: Vec::new(),
        is_record: false,
        attrs,
        header_span,
        data_span: (0, 0),
        offset: addr,
    });
    if my_index == 0 {
        // The root group's attributes are promoted to the model-level attribute list.
        // (`read_attribute` already charged them against the aggregate cap.)
        let a = std::mem::take(&mut h.objects[0].attrs);
        h.attributes.extend(a);
    }
    if let Some((btree, heap)) = symbol {
        read_group_members(h, btree, heap, my_index, depth)?;
    }
    Ok(())
}

fn read_group_members(h: &mut H<'_>, btree: u64, heap: u64, parent: u32, depth: u32) -> Result<()> {
    // Local heap: signature "HEAP", version, reserved, data size, free-list offset,
    // data segment address.
    let hp = heap as usize;
    if get_u32_le(h.b, hp)? != u32::from_le_bytes(*b"HEAP") {
        return Err(invalid_hdf("hdf5 local heap has a bad signature"));
    }
    let data_size = h.len_at(heap + 8)?;
    let data_addr = h.off_at(heap + 8 + 2 * h.len as u64)?;
    if data_addr
        .checked_add(data_size)
        .map(|e| e > h.b.len() as u64)
        .unwrap_or(true)
    {
        return Err(invalid_hdf("hdf5 local heap data segment is out of bounds"));
    }
    let mut nodes: Vec<u64> = Vec::new();
    collect_symbol_nodes(h, btree, 0, &mut nodes)?;
    for node in nodes {
        let leaf_k = h.sb.group_leaf_k.max(1) as u64;
        for e in 0..(2 * leaf_k) {
            let entry = node + e * (2 * h.off as u64 + 24);
            let link_off = h.off_at(entry)?;
            let oh_addr = h.off_at(entry + h.off as u64)?;
            if link_off == 0 && oh_addr == 0 {
                continue;
            }
            let (link, link_span) = heap_string(h, data_addr, data_size, link_off)?;
            if oh_addr == 0 || oh_addr >= h.b.len() as u64 {
                return Err(invalid_hdf(
                    "hdf5 symbol table entry has a bad object address",
                ));
            }
            let child_msgs = h.read_object_header(oh_addr)?;
            let is_group = child_msgs.iter().any(|m| m.ty == MSG_SYMBOL_TABLE);
            let has_dataset = child_msgs
                .iter()
                .any(|m| m.ty == MSG_DATATYPE || m.ty == MSG_LAYOUT);
            if is_group {
                read_group(h, oh_addr, link, link_span, parent, depth + 1)?;
            } else if has_dataset {
                read_dataset(h, oh_addr, child_msgs, link, link_span, parent, depth + 1)?;
            } else {
                return Err(decline(
                    "hdf5 object is neither a symbol-table group nor a typed dataset",
                ));
            }
        }
    }
    Ok(())
}

fn collect_symbol_nodes(h: &mut H<'_>, btree: u64, level: u32, out: &mut Vec<u64>) -> Result<()> {
    if level > MAX_BTREE_DEPTH {
        return Err(Error::resource_limit("hdf5 B-tree is too deep"));
    }
    h.btree_nodes += 1;
    if h.btree_nodes > MAX_BTREE_NODES {
        return Err(Error::resource_limit("hdf5 B-tree has too many nodes"));
    }
    let a = btree as usize;
    let node_type = get_u8(h.b, a)?;
    if node_type != 0 {
        return Err(decline("hdf5 B-tree is not a group (symbol table) B-tree"));
    }
    let node_level = get_u8(h.b, a + 1)?;
    let entries = get_u16_le(h.b, a + 2)? as u64;
    if entries > MAX_BTREE_NODES {
        return Err(Error::resource_limit(
            "hdf5 B-tree node has too many entries",
        ));
    }
    let off = h.off as u64;
    let base = a as u64 + 4 + 2 * off;
    // Children: for i in 0..entries, child_i at base + (2i+1)*off, plus child_entries
    // at base + entries*2*off.
    for i in 0..entries {
        let child = h.off_at(base + (2 * i + 1) * off)?;
        if node_level == 0 {
            out.push(child);
        } else {
            collect_symbol_nodes(h, child, level + 1, out)?;
        }
    }
    let last = h.off_at(base + entries * 2 * off)?;
    if node_level == 0 {
        out.push(last);
    } else {
        collect_symbol_nodes(h, last, level + 1, out)?;
    }
    Ok(())
}

fn heap_string(
    h: &mut H<'_>,
    data_addr: u64,
    data_size: u64,
    link_off: u64,
) -> Result<(Vec<u8>, (u64, u64))> {
    if link_off >= data_size {
        return Err(invalid_hdf("hdf5 heap string offset is out of range"));
    }
    let start = data_addr + link_off;
    let max = data_size - link_off;
    let s = start as usize;
    let e = (start + max).min(h.b.len() as u64) as usize;
    let hay =
        h.b.get(s..e)
            .ok_or_else(|| invalid_hdf("hdf5 heap string is out of bounds"))?;
    let n = hay.iter().position(|&b| b == 0).unwrap_or(hay.len());
    h.name_add(n as u64)?;
    Ok((hay[..n].to_vec(), (start, start + n as u64)))
}

fn read_dataset(
    h: &mut H<'_>,
    addr: u64,
    msgs: Vec<Msg>,
    name: Vec<u8>,
    name_span: (u64, u64),
    parent: u32,
    depth: u32,
) -> Result<()> {
    if h.objects.len() as u64 >= h.limits.max_hdf_objects {
        return Err(Error::resource_limit(format!(
            "hdf5 object count exceeds max_hdf_objects {}",
            h.limits.max_hdf_objects
        )));
    }
    let mut attrs: Vec<HdfAttribute> = Vec::new();
    let mut dataspace: Option<Vec<u64>> = None;
    let mut ty = T_UNKNOWN;
    let mut elem_size = 0u64;
    let mut layout: Option<(u64, u64)> = None;
    for m in &msgs {
        expect_benign_or_decline(m.ty)?;
        match m.ty {
            MSG_DATASPACE => dataspace = Some(read_dataspace(h, m.body)?),
            MSG_DATATYPE => {
                let (t, s) = read_datatype(h, m.body)?;
                ty = t;
                elem_size = s;
            }
            MSG_LAYOUT => layout = Some(read_layout(h, m.body)?),
            MSG_ATTRIBUTE => attrs.push(read_attribute(h, m.body)?),
            _ => {}
        }
    }
    let dims = dataspace.ok_or_else(|| decline("hdf5 dataset has no dataspace"))?;
    let (data_off, data_len) = layout.ok_or_else(|| decline("hdf5 dataset has no data layout"))?;
    let data_end = data_off
        .checked_add(data_len)
        .ok_or_else(|| invalid_hdf("hdf5 dataset data size overflow"))?;
    if data_end > h.b.len() as u64 {
        return Err(invalid_hdf("hdf5 dataset data extends past the file"));
    }
    let header_span = (
        addr,
        addr.saturating_add(12 + get_u32_le(h.b, addr as usize + 8)? as u64),
    );
    h.objects.push(HdfObject {
        name,
        name_span,
        parent,
        depth,
        kind: KIND_DATASET,
        ty,
        elem_size,
        dims,
        dim_ids: Vec::new(),
        is_record: false,
        attrs,
        header_span,
        data_span: (data_off, data_end),
        offset: data_off,
    });
    Ok(())
}

fn read_dataspace(h: &H<'_>, body: (u64, u64)) -> Result<Vec<u64>> {
    let a = body.0 as usize;
    let version = get_u8(h.b, a)?;
    if version != 1 {
        return Err(decline(format!(
            "unsupported HDF5 dataspace version {version}"
        )));
    }
    let rank = get_u8(h.b, a + 1)? as u64;
    let flags = get_u8(h.b, a + 2)?;
    if rank == 0 {
        return Ok(Vec::new());
    }
    if rank > h.limits.max_hdf_dimensions {
        return Err(Error::resource_limit("hdf5 dataspace rank is implausible"));
    }
    if body.1 < body.0 + 4 {
        return Err(invalid_hdf("hdf5 dataspace body is truncated"));
    }
    let mut dims = Vec::with_capacity(rank.min(64) as usize);
    let mut at = body.0 + 4;
    for _ in 0..rank {
        dims.push(h.len_at(at)?);
        at += h.len as u64;
        if at > body.1 {
            return Err(invalid_hdf("hdf5 dataspace dims overrun the message"));
        }
    }
    if flags & 1 != 0 {
        at += rank * h.len as u64;
        if at > body.1 {
            return Err(invalid_hdf("hdf5 dataspace max-dims overrun the message"));
        }
    }
    Ok(dims)
}

fn read_datatype(h: &H<'_>, body: (u64, u64)) -> Result<(u8, u64)> {
    let a = body.0 as usize;
    let class_version = get_u8(h.b, a)?;
    let version = class_version & 0x0F;
    if version != 1 {
        return Err(decline(format!(
            "unsupported HDF5 datatype version {version}"
        )));
    }
    let class = class_version >> 4;
    let _bitfield = get_u32_le(h.b, a + 1)?;
    let size = get_u32_le(h.b, a + 4)? as u64;
    if body.1 < body.0 + 8 {
        return Err(invalid_hdf("hdf5 datatype body is truncated"));
    }
    match class {
        DT_FIXED => {
            // Standard element sizes map onto the normalized integer types; a
            // non-standard size (which carries extra bit-precision properties) is
            // declined typed rather than guessed.
            let signed = (get_u32_le(h.b, a + 1)? & 0x1) != 0;
            let ty = match (signed, size) {
                (true, 1) => T_I8,
                (true, 2) => T_I16,
                (true, 4) => T_I32,
                (true, 8) => T_I64,
                (false, 1) => T_U8,
                (false, 2) => T_U16,
                (false, 4) => T_U32,
                (false, 8) => T_U64,
                _ => {
                    return Err(decline(format!(
                        "unsupported HDF5 fixed-point element size {size}"
                    )));
                }
            };
            Ok((ty, size))
        }
        DT_FLOAT => {
            let ty = match size {
                4 => T_F32,
                8 => T_F64,
                _ => {
                    return Err(decline(format!(
                        "unsupported HDF5 floating-point element size {size}"
                    )));
                }
            };
            Ok((ty, size))
        }
        DT_STRING => Ok((T_STRING, size)),
        other => Err(decline(format!("unsupported HDF5 datatype class {other}"))),
    }
}

fn read_layout(h: &H<'_>, body: (u64, u64)) -> Result<(u64, u64)> {
    let a = body.0 as usize;
    let version = get_u8(h.b, a)?;
    if version != 1 && version != 2 {
        return Err(decline(format!(
            "unsupported HDF5 data layout version {version}"
        )));
    }
    let class = get_u8(h.b, a + 2)?;
    if class != 1 {
        return Err(decline(match class {
            0 => "HDF5 compact data layout is outside the supported subset",
            2 => "HDF5 chunked data layout is outside the supported subset",
            _ => "HDF5 virtual data layout is outside the supported subset",
        }));
    }
    // Contiguous: address (offset) then size (length), after the 8-byte header.
    let at = body.0 + 8;
    let address = h.off_at(at)?;
    let size = h.len_at(at + h.off as u64)?;
    if body.1 < at {
        return Err(invalid_hdf("hdf5 data layout body is truncated"));
    }
    Ok((address, size))
}

fn read_attribute(h: &mut H<'_>, body: (u64, u64)) -> Result<HdfAttribute> {
    h.attr_total += 1;
    if h.attr_total > h.limits.max_hdf_attributes {
        return Err(Error::resource_limit(format!(
            "hdf5 attribute count exceeds max_hdf_attributes {}",
            h.limits.max_hdf_attributes
        )));
    }
    let a = body.0 as usize;
    let version = get_u8(h.b, a)?;
    if version != 1 {
        return Err(decline(format!(
            "unsupported HDF5 attribute version {version}"
        )));
    }
    let name_size = get_u16_le(h.b, a + 2)? as u64;
    let dt_size = get_u16_le(h.b, a + 4)? as u64;
    let ds_size = get_u16_le(h.b, a + 6)? as u64;
    let name_start = body.0 + 8;
    let dt_start = name_start + name_size;
    let ds_start = dt_start + dt_size;
    let data_start = ds_start + ds_size;
    if data_start > body.1 {
        return Err(invalid_hdf("hdf5 attribute body is truncated"));
    }
    h.name_add(name_size)?;
    let name_end = name_start + name_size;
    let raw_name = get_slice(h.b, name_start as usize, name_end as usize)?;
    let name = trim_nul(raw_name);
    let (ty, _elem) = read_datatype(h, (dt_start, ds_start))?;
    let dims = read_dataspace(h, (ds_start, data_start))?;
    let num_values = dims.iter().product::<u64>().max(1);
    let data_len = body.1 - data_start;
    if data_len > h.limits.max_hdf_attr_value_bytes {
        return Err(Error::resource_limit(format!(
            "hdf5 attribute value {data_len} bytes exceeds max_hdf_attr_value_bytes {}",
            h.limits.max_hdf_attr_value_bytes
        )));
    }
    let value = get_slice(h.b, data_start as usize, body.1 as usize)?.to_vec();
    Ok(HdfAttribute {
        name,
        name_span: (name_start, name_end),
        ty,
        num_values,
        data_span: (data_start, body.1),
        value,
    })
}

// ---------------------------------------------------------------------------
// Little-endian byte helpers
// ---------------------------------------------------------------------------

fn get_u8(b: &[u8], at: usize) -> Result<u8> {
    b.get(at)
        .copied()
        .ok_or_else(|| invalid_hdf("input ended early"))
}

fn get_u16_le(b: &[u8], at: usize) -> Result<u16> {
    let s = get_slice(b, at, at + 2)?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}

fn get_u32_le(b: &[u8], at: usize) -> Result<u32> {
    let s = get_slice(b, at, at + 4)?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn get_slice(b: &[u8], start: usize, end: usize) -> Result<&[u8]> {
    if end < start {
        return Err(invalid_hdf("span is reversed"));
    }
    b.get(start..end)
        .ok_or_else(|| invalid_hdf("input ended early"))
}

fn read_off_at(b: &[u8], at: u64, size: u8) -> Result<u64> {
    read_le(b, at, size)
}

fn read_len_at(b: &[u8], at: u64, size: u8) -> Result<u64> {
    read_le(b, at, size)
}

fn read_le(b: &[u8], at: u64, size: u8) -> Result<u64> {
    let s = usize::try_from(at).map_err(|_| invalid_hdf("offset is out of range"))?;
    let n = size as usize;
    let end = s
        .checked_add(n)
        .ok_or_else(|| invalid_hdf("offset overflow"))?;
    let bytes = get_slice(b, s, end)?;
    let mut v: u64 = 0;
    for (i, &byte) in bytes.iter().enumerate() {
        v |= (byte as u64) << (8 * i);
    }
    Ok(v)
}

fn trim_nul(field: &[u8]) -> Vec<u8> {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    field[..end].to_vec()
}

fn slice<'a>(source: &'a [u8], start: u64, end: u64, why: &'static str) -> Result<&'a [u8]> {
    let s = usize::try_from(start).map_err(|_| invalid_hdf(why))?;
    let e = usize::try_from(end).map_err(|_| invalid_hdf(why))?;
    source.get(s..e).ok_or_else(|| invalid_hdf(why))
}

fn invalid_hdf(message: impl Into<String>) -> Error {
    Error::invalid_hdf_structure(message)
}

fn decline(message: impl Into<String>) -> Error {
    Error::unsupported_feature(message)
}

fn corrupt(message: impl Into<String>) -> Error {
    Error::invalid_hdf_structure(format!("hdf model: {}", message.into()))
}

/// A minimal fail-closed little-endian reader for model decoding.
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
            .ok_or_else(|| corrupt("length overflow"))?;
        let out = self
            .b
            .get(self.at..end)
            .ok_or_else(|| corrupt("unexpected end of model"))?;
        self.at = end;
        Ok(out)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    fn u32(&mut self) -> Result<u32> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Result<u64> {
        let b = self.bytes(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    fn at_end(&self) -> bool {
        self.at == self.b.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialect_and_kind_names_are_stable() {
        assert_eq!(dialect_name(DIALECT_NETCDF), "netcdf");
        assert_eq!(dialect_name(DIALECT_HDF5), "hdf5");
        assert_eq!(kind_name(KIND_GROUP), "group");
        assert_eq!(kind_name(KIND_DATASET), "dataset");
        assert_eq!(type_name(T_F64), "float64");
        assert_eq!(type_name(T_STRING), "string");
        assert_eq!(field_name(FIELD_DIMENSION), "dimension");
    }

    #[test]
    fn prose_is_not_a_scientific_container() {
        assert!(!detect(
            b"the quick brown fox jumps over the lazy dog\n",
            Limits::DEFAULT
        ));
    }

    #[test]
    fn bad_hdf5_superblock_is_not_detected() {
        let mut buf = HDF5_SIGNATURE.to_vec();
        buf.push(9); // invalid superblock version
        buf.resize(128, 0);
        assert!(!detect(&buf, Limits::DEFAULT));
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut x: u64 = 0x1234_5678_9abc_def0;
        for _ in 0..2048 {
            let mut buf = vec![0u8; 256];
            for b in buf.iter_mut() {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                *b = (x & 0xFF) as u8;
            }
            let _ = detect(&buf, Limits::DEFAULT);
            let _ = parse(&buf, Limits::STRICT);
        }
        for sig in [&HDF5_SIGNATURE[..], &NETCDF_CDF1[..], &NETCDF_CDF2[..]] {
            for _ in 0..512 {
                let mut buf = sig.to_vec();
                buf.resize(256, 0);
                for b in buf.iter_mut().skip(sig.len()) {
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    *b = (x & 0xFF) as u8;
                }
                let _ = detect(&buf, Limits::DEFAULT);
                let _ = parse(&buf, Limits::STRICT);
            }
        }
    }
}
