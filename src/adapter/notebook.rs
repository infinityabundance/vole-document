//! Bounded, representation-preserving Jupyter notebook (`.ipynb`, nbformat) adapter
//! (Phase 21.24).
//!
//! A Jupyter notebook's physical bytes are JSON (the nbformat serialization), so the
//! representation-preserving JSON parser is the whole physical layer: this adapter
//! **reuses** [`crate::adapter::json`] — never a second JSON parser — and adds only a
//! bounded **semantic** projection on top. The exact leaf is still the whole source
//! (a `DocumentExact`, a RAW-like authority); everything this module produces is a
//! bounded, deterministic (`Q_gen`) projection that never sits on the exactness path.
//!
//! ## Reuse, not a second node arena
//!
//! The notebook model embeds the JSON [`JsonModel`]: the **same** node arena the JSON
//! adapter defines (kind, exact `[start, end)` token span, ordered children). So
//! member **order**, **duplicate keys**, numeric **spelling** (`1` vs `1e0`), string-
//! escape **spelling**, and every token's **exact byte span** carry the identical
//! guarantees. A notebook answer's spans are therefore all `Q_gen` projections of the
//! JSON parse; none of them is on the exactness path.
//!
//! ## What is preserved
//!
//! A notebook is a JSON object with an integer `nbformat` and an array `cells`. This
//! adapter records the exact `nbformat`/`nbformat_minor` tokens and the `metadata`
//! object node (member order and duplicate keys preserved by the JSON model), and for
//! every cell in document order:
//!
//! * the exact `cell_type` string (`code`/`markdown`/`raw`, or any other string);
//! * `source` **exactly as written** — either a single string or an array of line
//!   strings. The representation is never re-joined, split, or normalized; the two
//!   forms are recorded distinctly ([`SRC_STRING`] vs [`SRC_LINES`]);
//! * `execution_count` (the exact token), `metadata`, and `attachments` (its member
//!   keys) when present;
//! * for code cells (or any cell carrying `outputs`), the ordered `outputs` array,
//!   preserving each output's exact `output_type` string (`stream`/`execute_result`/
//!   `display_data`/`error`, or any other string) and every field token, including a
//!   `stream` output's `text` and an `execute_result`/`display_data` output's `data`
//!   representation.
//!
//! ## Detection (a bounded semantic sub-test)
//!
//! The notebook shares its physical bytes with [`crate::adapter::json`], so detection
//! is a **bounded semantic test** run *before* the generic JSON detector
//! ([`crate::field::document_format`]). It reuses the JSON detector first, then
//! requires the source to parse as exactly one JSON value that is an nbformat-shaped
//! object:
//!
//! * the root is an object with a non-negative integer-literal `nbformat` (≥ 1) and an
//!   array `cells`;
//! * every `cells` element is an object with a string `cell_type`;
//! * any *present* recognized field has its nbformat-appropriate kind: a cell
//!   `source` is a string or an array of strings; a cell `execution_count` is a number
//!   or `null`; a cell `metadata`/`attachments` and the root `metadata` are objects;
//!   a cell `outputs` is an array whose every element is an object with a string
//!   `output_type`.
//!
//! A plain JSON document (no `nbformat`, or a non-object root) therefore stays
//! [`crate::field::document_format::DocumentFormat::Json`]; a JSON document that
//! merely has a `cells` key but is not nbformat-shaped (a `cells` array of non-objects,
//! a missing/invalid `nbformat`, a cell with no `cell_type`) also stays `Json`; and
//! prose or malformed input stays [`crate::field::document_format::DocumentFormat::Opaque`].
//!
//! ## Recorded boundaries (honest)
//!
//! * `nbformat` must be written as a plain non-negative **integer literal** (`4`). A
//!   numerically-integral-but-differently-spelled token (`4.0`, `4e0`, `-0`) is **not**
//!   the claimed shape and stays `Json`; the adapter never reparses a number into a
//!   binary integer.
//! * The `nbformat` value is **not** restricted to the current version: 1..4 (and
//!   beyond) are all accepted, because the shape is what is claimed, not a version.
//! * `cell_type`/`output_type` strings are preserved verbatim but **not** restricted
//!   to the known set; an unrecognized value is recorded as given ([`C_UNKNOWN`] /
//!   [`O_UNKNOWN`]) and never normalized.
//! * A `source` string is never split and a `source` array is never joined, so the
//!   two representations cannot be conflated.
//! * A notebook that is shaped like nbformat but semantically odd (an `execution_count`
//!   on a markdown cell, an `outputs` array on a non-code cell) is preserved verbatim;
//!   the adapter records what is present and does not reject it.
//!
//! ## Bounds and no panics
//!
//! Untrusted input can only ever yield a typed decline, never a panic or an unbounded
//! allocation: the source length by [`Limits::max_notebook_document_bytes`], the
//! embedded JSON node count by [`Limits::max_notebook_nodes`] (in addition to the JSON
//! caps the shared parser already enforces), the cell count by
//! [`Limits::max_notebook_cells`], the total output count by
//! [`Limits::max_notebook_outputs`], the aggregate source token bytes by
//! [`Limits::max_notebook_source_bytes`], and the notebook structural recursion by
//! [`Limits::max_notebook_depth`].

use crate::adapter::json::{
    self, JNode, JsonMatch, JsonModel, K_ARRAY, K_NULL, K_NUMBER, K_OBJECT, K_STRING,
};
use crate::error::{Error, ErrorClass, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model nodes (defends the decoder against a hostile blob).
pub const MAX_MODEL_NODES: u32 = 1 << 24;
/// Hard cap on decoded anchor indices (cells + outputs).
pub const MAX_MODEL_ANCHORS: u32 = 1 << 24;

/// Sentinel for an absent optional anchor index.
pub const ABSENT: u32 = u32::MAX;

/// Cell class: a `code` cell.
pub const C_CODE: u8 = 0;
/// Cell class: a `markdown` cell.
pub const C_MARKDOWN: u8 = 1;
/// Cell class: a `raw` cell.
pub const C_RAW: u8 = 2;
/// Cell class: any other `cell_type` string (preserved verbatim).
pub const C_UNKNOWN: u8 = 3;

/// The known nbformat cell type names, in class order.
pub const CELL_TYPE_NAMES: [&str; 3] = ["code", "markdown", "raw"];

/// The stable cell type name for a class tag.
pub const fn cell_type_name(class: u8) -> &'static str {
    match class {
        C_CODE => "code",
        C_MARKDOWN => "markdown",
        C_RAW => "raw",
        C_UNKNOWN => "unknown",
        _ => "unknown",
    }
}

/// The class tag for a cell type name (`C_UNKNOWN` for anything else).
pub fn cell_class_of(name: &str) -> u8 {
    match CELL_TYPE_NAMES.iter().position(|&n| n == name) {
        Some(i) => i as u8,
        None => C_UNKNOWN,
    }
}

/// Output class: a `stream` output.
pub const O_STREAM: u8 = 0;
/// Output class: an `execute_result` output.
pub const O_EXECUTE_RESULT: u8 = 1;
/// Output class: a `display_data` output.
pub const O_DISPLAY_DATA: u8 = 2;
/// Output class: an `error` output.
pub const O_ERROR: u8 = 3;
/// Output class: any other `output_type` string (preserved verbatim).
pub const O_UNKNOWN: u8 = 4;

/// The known nbformat output type names, in class order.
pub const OUTPUT_TYPE_NAMES: [&str; 4] = ["stream", "execute_result", "display_data", "error"];

/// The stable output type name for a class tag.
pub const fn output_type_name(class: u8) -> &'static str {
    match class {
        O_STREAM => "stream",
        O_EXECUTE_RESULT => "execute_result",
        O_DISPLAY_DATA => "display_data",
        O_ERROR => "error",
        O_UNKNOWN => "unknown",
        _ => "unknown",
    }
}

/// The class tag for an output type name (`O_UNKNOWN` for anything else).
pub fn output_class_of(name: &str) -> u8 {
    match OUTPUT_TYPE_NAMES.iter().position(|&n| n == name) {
        Some(i) => i as u8,
        None => O_UNKNOWN,
    }
}

/// Source form: the cell has no `source` member.
pub const SRC_MISSING: u8 = 0;
/// Source form: `source` is a single JSON string.
pub const SRC_STRING: u8 = 1;
/// Source form: `source` is an array of line strings (never re-joined).
pub const SRC_LINES: u8 = 2;

/// The stable source-form name for a form tag.
pub const fn source_form_name(form: u8) -> &'static str {
    match form {
        SRC_MISSING => "missing",
        SRC_STRING => "string",
        SRC_LINES => "lines",
        _ => "unknown",
    }
}

/// The canonical per-cell anchor: the cell object node plus its recorded fields as
/// indices into the embedded JSON arena. [`ABSENT`] marks a missing optional member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellAnchor {
    /// Index of the cell object node.
    pub node: u32,
    /// The cell class tag (`C_*`).
    pub class: u8,
    /// Index of the cell's `"cell_type"` **value** string node.
    pub cell_type_node: u32,
    /// Index of the `"source"` value node, or [`ABSENT`].
    pub source_node: u32,
    /// One of the `SRC_*` form tags.
    pub source_form: u8,
    /// Index of the `"execution_count"` value node, or [`ABSENT`].
    pub execution_count_node: u32,
    /// Index of the cell `"metadata"` object node, or [`ABSENT`].
    pub metadata_node: u32,
    /// Index of the `"attachments"` object node, or [`ABSENT`].
    pub attachments_node: u32,
    /// The ordered `"outputs"` object node indices (possibly empty).
    pub outputs: Vec<u32>,
}

/// The canonical derived notebook model (the materialization of a `NotebookModel`
/// node). It embeds the JSON [`JsonModel`] and stores the semantic anchors as indices
/// into that arena, so every span is a `Q_gen` projection of the JSON parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotebookModel {
    /// Index of the root node (a JSON object).
    pub root: u32,
    /// The decoded `nbformat` integer.
    pub nbformat: u32,
    /// The decoded `nbformat_minor` integer (`0` when absent).
    pub nbformat_minor: u32,
    /// Index of the root's `"nbformat"` **value** node.
    pub nbformat_node: u32,
    /// Index of the root's `"nbformat_minor"` value node, or [`ABSENT`].
    pub nbformat_minor_node: u32,
    /// Index of the root's `"metadata"` object node, or [`ABSENT`].
    pub metadata_node: u32,
    /// The cell anchors in document order.
    pub cells: Vec<CellAnchor>,
    /// The embedded representation-preserving JSON arena.
    pub json: JsonModel,
}

impl NotebookModel {
    /// The JSON node at `index`, if present.
    pub fn node(&self, index: u32) -> Option<&JNode> {
        self.json.node(index)
    }

    /// The number of cells in the model.
    pub fn cell_count(&self) -> u32 {
        self.cells.len() as u32
    }

    /// The total number of outputs across every cell.
    pub fn output_count(&self) -> u32 {
        self.cells.iter().map(|c| c.outputs.len() as u32).sum()
    }

    /// The cell anchor at `index`, if present.
    pub fn cell(&self, index: u32) -> Option<&CellAnchor> {
        self.cells.get(index as usize)
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let json_bytes = self.json.encode();
        let anchors: usize = self.cells.iter().map(|c| 1 + c.outputs.len()).sum();
        let mut out = Vec::with_capacity(64 + json_bytes.len() + anchors * 4);
        out.extend_from_slice(b"NBKM");
        out.push(MODEL_VERSION);
        out.extend_from_slice(&[0, 0, 0]); // reserved
        out.extend_from_slice(&self.nbformat.to_le_bytes());
        out.extend_from_slice(&self.nbformat_minor.to_le_bytes());
        out.extend_from_slice(&self.doc_len().to_le_bytes());
        out.extend_from_slice(&self.root.to_le_bytes());
        out.extend_from_slice(&self.nbformat_node.to_le_bytes());
        out.extend_from_slice(&self.nbformat_minor_node.to_le_bytes());
        out.extend_from_slice(&self.metadata_node.to_le_bytes());
        out.extend_from_slice(&(self.cells.len() as u32).to_le_bytes());
        for c in &self.cells {
            out.extend_from_slice(&c.node.to_le_bytes());
            out.push(c.class);
            out.push(c.source_form);
            out.extend_from_slice(&[0, 0]); // reserved
            out.extend_from_slice(&c.cell_type_node.to_le_bytes());
            out.extend_from_slice(&c.source_node.to_le_bytes());
            out.extend_from_slice(&c.execution_count_node.to_le_bytes());
            out.extend_from_slice(&c.metadata_node.to_le_bytes());
            out.extend_from_slice(&c.attachments_node.to_le_bytes());
            out.extend_from_slice(&(c.outputs.len() as u32).to_le_bytes());
            for o in &c.outputs {
                out.extend_from_slice(&o.to_le_bytes());
            }
        }
        out.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&json_bytes);
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<NotebookModel> {
        let mut r = Reader::new(bytes);
        if r.bytes(4)? != b"NBKM" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        if r.bytes(3)? != [0, 0, 0] {
            return Err(corrupt("unknown model flags"));
        }
        let nbformat = r.u32()?;
        let nbformat_minor = r.u32()?;
        let doc_len = r.u64()?;
        let root = r.u32()?;
        let nbformat_node = r.u32()?;
        let nbformat_minor_node = r.u32()?;
        let metadata_node = r.u32()?;
        let cn = r.u32()?;
        if cn > MAX_MODEL_ANCHORS {
            return Err(corrupt("cell count is implausible"));
        }
        let mut cells = Vec::with_capacity(cn as usize);
        for _ in 0..cn {
            let node = r.u32()?;
            let class = r.u8()?;
            if class > C_UNKNOWN {
                return Err(corrupt("unknown cell class"));
            }
            let source_form = r.u8()?;
            if source_form > SRC_LINES {
                return Err(corrupt("unknown source form"));
            }
            if r.bytes(2)? != [0, 0] {
                return Err(corrupt("unknown cell flags"));
            }
            let cell_type_node = r.u32()?;
            let source_node = r.u32()?;
            let execution_count_node = r.u32()?;
            let cell_metadata = r.u32()?;
            let attachments_node = r.u32()?;
            let on = r.u32()?;
            if on > MAX_MODEL_ANCHORS {
                return Err(corrupt("output count is implausible"));
            }
            let mut outputs = Vec::with_capacity(on as usize);
            for _ in 0..on {
                outputs.push(r.u32()?);
            }
            cells.push(CellAnchor {
                node,
                class,
                cell_type_node,
                source_node,
                source_form,
                execution_count_node,
                metadata_node: cell_metadata,
                attachments_node,
                outputs,
            });
        }
        let json_len = r.u32()?;
        let json_bytes = r.bytes(json_len as usize)?;
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        let json = JsonModel::decode(json_bytes)?;
        if json.doc_len != doc_len {
            return Err(corrupt("model doc_len disagrees with the JSON model"));
        }
        let nnodes = json.nodes.len() as u32;
        for idx in [root, nbformat_node] {
            if idx >= nnodes {
                return Err(corrupt("model anchor is out of range"));
            }
        }
        for idx in [nbformat_minor_node, metadata_node] {
            if idx != ABSENT && idx >= nnodes {
                return Err(corrupt("model optional anchor is out of range"));
            }
        }
        for c in &cells {
            if c.node >= nnodes || c.cell_type_node >= nnodes {
                return Err(corrupt("cell anchor is out of range"));
            }
            for idx in [
                c.source_node,
                c.execution_count_node,
                c.metadata_node,
                c.attachments_node,
            ] {
                if idx != ABSENT && idx >= nnodes {
                    return Err(corrupt("cell optional anchor is out of range"));
                }
            }
            if c.source_node == ABSENT && c.source_form != SRC_MISSING {
                return Err(corrupt("cell source form disagrees with its anchor"));
            }
            for &o in &c.outputs {
                if o >= nnodes {
                    return Err(corrupt("output index is out of range"));
                }
            }
        }
        Ok(NotebookModel {
            root,
            nbformat,
            nbformat_minor,
            nbformat_node,
            nbformat_minor_node,
            metadata_node,
            cells,
            json,
        })
    }

    /// The exact source length the spans are relative to.
    pub fn doc_len(&self) -> u64 {
        self.json.doc_len
    }
}

/// A lexical match from [`find`]: a record over the shared JSON match vocabulary
/// (canonical RFC 6901 pointer, key/value role, exact span, decoded text).
pub type NotebookMatch = JsonMatch;

/// Byte-based, conservative notebook detector. See the module docs for the exact
/// semantic boundary.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.len() as u64 > limits.max_notebook_document_bytes {
        return false;
    }
    // Reuse the JSON detector first: a notebook document is a JSON document.
    if !json::detect(source, limits) {
        return false;
    }
    parse(source, limits, false).is_ok()
}

/// Parse `source` into a [`NotebookModel`]. `build` selects whether the semantic cell
/// anchors are retained (detection runs with `build = false`); the JSON arena is always
/// built because the semantic test walks it. The shared JSON parser enforces the JSON
/// caps; this function adds the notebook caps on top.
pub fn parse(source: &[u8], limits: Limits, build: bool) -> Result<NotebookModel> {
    if source.len() as u64 > limits.max_notebook_document_bytes {
        return Err(Error::resource_limit(format!(
            "notebook source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_notebook_document_bytes
        )));
    }
    // Reuse the shared JSON parser: a notebook source must be exactly one JSON value.
    // A malformed source becomes a typed notebook decline (resource errors are kept as
    // resource errors so caps are never laundered into structure errors).
    let json = json::parse(source, limits, true).map_err(|e| match e.class() {
        ErrorClass::ResourceLimit => e,
        _ => invalid_notebook(format!(
            "notebook source is not a single JSON value: {}",
            e.message()
        )),
    })?;
    if json.nodes.len() as u64 > limits.max_notebook_nodes as u64 {
        return Err(Error::resource_limit(format!(
            "notebook document exceeds the {}-node cap",
            limits.max_notebook_nodes
        )));
    }
    // The shared JSON parser already bounds nesting by `max_json_depth`; this adds the
    // notebook's own (usually tighter) structural bound on top.
    if json.max_depth as u64 > limits.max_notebook_depth as u64 {
        return Err(Error::resource_limit(format!(
            "notebook nesting exceeds the {}-level cap",
            limits.max_notebook_depth
        )));
    }
    let a = analyze(&json, source, limits)?;
    Ok(NotebookModel {
        root: json.root,
        nbformat: a.nbformat,
        nbformat_minor: a.nbformat_minor,
        nbformat_node: a.nbformat_node,
        nbformat_minor_node: a.nbformat_minor_node,
        metadata_node: a.metadata_node,
        cells: if build { a.cells } else { Vec::new() },
        json,
    })
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of a `NotebookModel` node).
pub fn build_notebook_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits, true)?.encode())
}

/// Classify `source` as an nbformat document, returning the decoded `nbformat`, or
/// decline typed when it is not structurally consistent under the semantic test.
pub fn classify(source: &[u8], limits: Limits) -> Result<u32> {
    Ok(parse(source, limits, false)?.nbformat)
}

// ---------------------------------------------------------------------------
// Semantic projections over the embedded JSON model
// ---------------------------------------------------------------------------

/// The value node of the **first** object member named `key` (duplicate keys are
/// preserved, but a reader resolves the first, exactly like the JSON adapter).
pub fn object_member(
    model: &NotebookModel,
    source: &[u8],
    obj: u32,
    key: &str,
) -> Result<Option<u32>> {
    let node = model
        .node(obj)
        .ok_or_else(|| corrupt("object index is out of range"))?;
    if node.kind != K_OBJECT {
        return Err(corrupt("node is not a JSON object"));
    }
    let mut i = 0usize;
    while i + 1 < node.children.len() {
        let k = node.children[i];
        let v = node.children[i + 1];
        i += 2;
        let kn = model
            .node(k)
            .ok_or_else(|| corrupt("object key index is out of range"))?;
        if kn.kind == K_STRING && json::decode_string(source, kn)? == key {
            return Ok(Some(v));
        }
    }
    Ok(None)
}

/// Every object member of `obj` in document order as `(decoded key, value node)`.
/// Member order and duplicate keys are preserved exactly.
pub fn object_members(
    model: &NotebookModel,
    source: &[u8],
    obj: u32,
) -> Result<Vec<(String, u32)>> {
    let node = model
        .node(obj)
        .ok_or_else(|| corrupt("object index is out of range"))?;
    if node.kind != K_OBJECT {
        return Err(corrupt("node is not a JSON object"));
    }
    let mut out = Vec::with_capacity(node.children.len() / 2);
    let mut i = 0usize;
    while i + 1 < node.children.len() {
        let k = node.children[i];
        let v = node.children[i + 1];
        i += 2;
        let kn = model
            .node(k)
            .ok_or_else(|| corrupt("object key index is out of range"))?;
        let key = json::decode_string(source, kn)?;
        out.push((key, v));
    }
    Ok(out)
}

/// The decoded string value of an object member, if present and a string.
pub fn member_string_value(
    model: &NotebookModel,
    source: &[u8],
    obj: u32,
    key: &str,
) -> Result<Option<String>> {
    match object_member(model, source, obj, key)? {
        Some(v) => {
            let n = model
                .node(v)
                .ok_or_else(|| corrupt("member value index is out of range"))?;
            if n.kind == K_STRING {
                Ok(Some(json::decode_string(source, n)?))
            } else {
                Ok(None)
            }
        }
        None => Ok(None),
    }
}

/// The member keys of an object member (document order, duplicates preserved), when
/// the member is present and an object; `None` otherwise.
pub fn member_keys(
    model: &NotebookModel,
    source: &[u8],
    obj: u32,
    key: &str,
) -> Result<Option<Vec<String>>> {
    let Some(v) = object_member(model, source, obj, key)? else {
        return Ok(None);
    };
    let n = model
        .node(v)
        .ok_or_else(|| corrupt("member value index is out of range"))?;
    if n.kind != K_OBJECT {
        return Ok(None);
    }
    let mut keys = Vec::with_capacity(n.children.len() / 2);
    let mut i = 0usize;
    while i + 1 < n.children.len() {
        let k = n.children[i];
        i += 2;
        let kn = model
            .node(k)
            .ok_or_else(|| corrupt("member key index is out of range"))?;
        keys.push(json::decode_string(source, kn)?);
    }
    Ok(Some(keys))
}

/// The decoded `"cell_type"` of a cell object node, if it is a string.
pub fn cell_type_of(model: &NotebookModel, source: &[u8], cell: u32) -> Result<Option<String>> {
    member_string_value(model, source, cell, "cell_type")
}

/// The decoded `"output_type"` of an output object node, if it is a string.
pub fn output_type_of(model: &NotebookModel, source: &[u8], output: u32) -> Result<Option<String>> {
    member_string_value(model, source, output, "output_type")
}

/// The exact token bytes of a node (`[start, end)`), bounded by the source length.
pub fn token_bytes<'a>(source: &'a [u8], node: &JNode) -> Result<&'a [u8]> {
    json::token_bytes(source, node)
}

/// Decode a JSON string node to its Rust text.
pub fn decode_string(source: &[u8], node: &JNode) -> Result<String> {
    json::decode_string(source, node)
}

/// The exact token span `[start, end)` of a node.
pub fn node_span(model: &NotebookModel, index: u32) -> Result<(u64, u64)> {
    let n = model
        .node(index)
        .ok_or_else(|| corrupt("node index is out of range"))?;
    Ok((n.start, n.end))
}

/// A bounded, case-sensitive lexical search over object keys and string values,
/// reusing the shared JSON [`json::find`] over the embedded arena.
pub fn find(
    model: &NotebookModel,
    source: &[u8],
    pattern: &str,
    limits: Limits,
) -> Result<Vec<NotebookMatch>> {
    json::find(&model.json, source, pattern, limits)
}

/// A deterministic canonical text projection of the whole document (the shared JSON
/// canonical text: member order and token spelling preserved).
pub fn canonical_text(model: &NotebookModel, source: &[u8]) -> Result<String> {
    json::canonical_text(&model.json, source)
}

// ---------------------------------------------------------------------------
// Internals: the semantic test and anchor collection
// ---------------------------------------------------------------------------

struct Analyzed {
    nbformat: u32,
    nbformat_minor: u32,
    nbformat_node: u32,
    nbformat_minor_node: u32,
    metadata_node: u32,
    cells: Vec<CellAnchor>,
}

fn analyze(json: &JsonModel, source: &[u8], limits: Limits) -> Result<Analyzed> {
    let root = json.root;
    let root_node = json
        .node(root)
        .ok_or_else(|| corrupt("root index is out of range"))?;
    if root_node.kind != K_OBJECT {
        return Err(invalid_notebook(
            "notebook root must be a JSON object with an integer \"nbformat\" and an array \"cells\"",
        ));
    }

    // nbformat: a plain non-negative integer literal (never reparsed).
    let nbformat_node = member_node(json, source, root, "nbformat")?
        .ok_or_else(|| invalid_notebook("notebook root has no \"nbformat\" member"))?;
    let nn = json
        .node(nbformat_node)
        .ok_or_else(|| corrupt("nbformat node index is out of range"))?;
    if nn.kind != K_NUMBER {
        return Err(invalid_notebook("notebook \"nbformat\" must be an integer"));
    }
    let nbformat = parse_nonneg_integer(token_text(source, nn)?.as_str()).ok_or_else(|| {
        invalid_notebook("notebook \"nbformat\" must be a plain non-negative integer literal")
    })?;
    if nbformat == 0 {
        return Err(invalid_notebook("notebook \"nbformat\" must be at least 1"));
    }

    // nbformat_minor: optional, but the same integer rule when present.
    let nbformat_minor_node = member_node(json, source, root, "nbformat_minor")?;
    let nbformat_minor = match nbformat_minor_node {
        Some(n) => {
            let node = json
                .node(n)
                .ok_or_else(|| corrupt("nbformat_minor node index is out of range"))?;
            if node.kind != K_NUMBER {
                return Err(invalid_notebook(
                    "notebook \"nbformat_minor\" must be an integer",
                ));
            }
            parse_nonneg_integer(token_text(source, node)?.as_str()).ok_or_else(|| {
                invalid_notebook(
                    "notebook \"nbformat_minor\" must be a plain non-negative integer literal",
                )
            })?
        }
        None => 0,
    };

    // root metadata: optional object.
    let metadata_node = member_node(json, source, root, "metadata")?;
    if let Some(m) = metadata_node {
        let node = json
            .node(m)
            .ok_or_else(|| corrupt("metadata node index is out of range"))?;
        if node.kind != K_OBJECT {
            return Err(invalid_notebook(
                "notebook root \"metadata\" must be an object",
            ));
        }
    }

    // cells: required array.
    let cells_node = member_node(json, source, root, "cells")?
        .ok_or_else(|| invalid_notebook("notebook root has no \"cells\" member"))?;
    let cn = json
        .node(cells_node)
        .ok_or_else(|| corrupt("cells node index is out of range"))?;
    if cn.kind != K_ARRAY {
        return Err(invalid_notebook("notebook \"cells\" must be an array"));
    }

    let mut cells: Vec<CellAnchor> = Vec::with_capacity(cn.children.len().min(4096));
    let mut total_outputs: u64 = 0;
    let mut total_source_bytes: u64 = 0;
    for &cell in &cn.children {
        if cells.len() as u64 >= limits.max_notebook_cells as u64 {
            return Err(Error::resource_limit(format!(
                "notebook exceeds the {}-cell cap",
                limits.max_notebook_cells
            )));
        }
        let node = json
            .node(cell)
            .ok_or_else(|| corrupt("cell index is out of range"))?;
        if node.kind != K_OBJECT {
            return Err(invalid_notebook("notebook cell must be a JSON object"));
        }
        // cell_type: required string.
        let cell_type_node = member_node(json, source, cell, "cell_type")?
            .ok_or_else(|| invalid_notebook("notebook cell has no \"cell_type\" member"))?;
        let ctn = json
            .node(cell_type_node)
            .ok_or_else(|| corrupt("cell_type node index is out of range"))?;
        if ctn.kind != K_STRING {
            return Err(invalid_notebook(
                "notebook cell \"cell_type\" must be a string",
            ));
        }
        let cell_type = json::decode_string(source, ctn)?;
        let class = cell_class_of(&cell_type);

        // source: optional; a string or an array of strings (never re-joined).
        let source_node = member_node(json, source, cell, "source")?;
        let mut source_form = SRC_MISSING;
        if let Some(s) = source_node {
            let sn = json
                .node(s)
                .ok_or_else(|| corrupt("source node index is out of range"))?;
            match sn.kind {
                K_STRING => source_form = SRC_STRING,
                K_ARRAY => {
                    for &el in &sn.children {
                        let en = json
                            .node(el)
                            .ok_or_else(|| corrupt("source element index is out of range"))?;
                        if en.kind != K_STRING {
                            return Err(invalid_notebook(
                                "notebook cell \"source\" array elements must be strings",
                            ));
                        }
                    }
                    source_form = SRC_LINES;
                }
                _ => {
                    return Err(invalid_notebook(
                        "notebook cell \"source\" must be a string or an array of strings",
                    ));
                }
            }
            total_source_bytes = total_source_bytes.saturating_add(sn.end.saturating_sub(sn.start));
            if total_source_bytes > limits.max_notebook_source_bytes {
                return Err(Error::resource_limit(format!(
                    "notebook source exceeds the {}-byte cap",
                    limits.max_notebook_source_bytes
                )));
            }
        }

        // execution_count: optional number or null.
        let execution_count_node = member_node(json, source, cell, "execution_count")?;
        if let Some(e) = execution_count_node {
            let en = json
                .node(e)
                .ok_or_else(|| corrupt("execution_count node index is out of range"))?;
            if en.kind != K_NUMBER && en.kind != K_NULL {
                return Err(invalid_notebook(
                    "notebook cell \"execution_count\" must be an integer or null",
                ));
            }
        }

        // cell metadata: optional object.
        let cell_metadata = member_node(json, source, cell, "metadata")?;
        if let Some(m) = cell_metadata {
            let node = json
                .node(m)
                .ok_or_else(|| corrupt("cell metadata node index is out of range"))?;
            if node.kind != K_OBJECT {
                return Err(invalid_notebook(
                    "notebook cell \"metadata\" must be an object",
                ));
            }
        }

        // attachments: optional object.
        let attachments_node = member_node(json, source, cell, "attachments")?;
        if let Some(a) = attachments_node {
            let node = json
                .node(a)
                .ok_or_else(|| corrupt("attachments node index is out of range"))?;
            if node.kind != K_OBJECT {
                return Err(invalid_notebook(
                    "notebook cell \"attachments\" must be an object",
                ));
            }
        }

        // outputs: optional array of objects each with a string output_type.
        let mut outputs: Vec<u32> = Vec::new();
        if let Some(o) = member_node(json, source, cell, "outputs")? {
            let on = json
                .node(o)
                .ok_or_else(|| corrupt("outputs node index is out of range"))?;
            if on.kind != K_ARRAY {
                return Err(invalid_notebook(
                    "notebook cell \"outputs\" must be an array",
                ));
            }
            for &out in &on.children {
                total_outputs = total_outputs.saturating_add(1);
                if total_outputs > limits.max_notebook_outputs as u64 {
                    return Err(Error::resource_limit(format!(
                        "notebook exceeds the {}-output cap",
                        limits.max_notebook_outputs
                    )));
                }
                let un = json
                    .node(out)
                    .ok_or_else(|| corrupt("output index is out of range"))?;
                if un.kind != K_OBJECT {
                    return Err(invalid_notebook("notebook output must be a JSON object"));
                }
                let ot = member_node(json, source, out, "output_type")?.ok_or_else(|| {
                    invalid_notebook("notebook output has no \"output_type\" member")
                })?;
                let otn = json
                    .node(ot)
                    .ok_or_else(|| corrupt("output_type node index is out of range"))?;
                if otn.kind != K_STRING {
                    return Err(invalid_notebook(
                        "notebook output \"output_type\" must be a string",
                    ));
                }
                outputs.push(out);
            }
        }

        cells.push(CellAnchor {
            node: cell,
            class,
            cell_type_node,
            source_node: source_node.unwrap_or(ABSENT),
            source_form,
            execution_count_node: execution_count_node.unwrap_or(ABSENT),
            metadata_node: cell_metadata.unwrap_or(ABSENT),
            attachments_node: attachments_node.unwrap_or(ABSENT),
            outputs,
        });
    }

    Ok(Analyzed {
        nbformat,
        nbformat_minor,
        nbformat_node,
        nbformat_minor_node: nbformat_minor_node.unwrap_or(ABSENT),
        metadata_node: metadata_node.unwrap_or(ABSENT),
        cells,
    })
}

/// The value node of the first object member named `key`, operating directly on the
/// embedded JSON arena (used by the semantic test before a model exists).
fn member_node(json: &JsonModel, source: &[u8], obj: u32, key: &str) -> Result<Option<u32>> {
    let node = json
        .node(obj)
        .ok_or_else(|| corrupt("object index is out of range"))?;
    if node.kind != K_OBJECT {
        return Err(corrupt("node is not a JSON object"));
    }
    let mut i = 0usize;
    while i + 1 < node.children.len() {
        let k = node.children[i];
        let v = node.children[i + 1];
        i += 2;
        let kn = json
            .node(k)
            .ok_or_else(|| corrupt("object key index is out of range"))?;
        if kn.kind == K_STRING && json::decode_string(source, kn)? == key {
            return Ok(Some(v));
        }
    }
    Ok(None)
}

/// The literal token text of a node (lossy UTF-8; JSON numbers/keywords are ASCII).
fn token_text(source: &[u8], node: &JNode) -> Result<String> {
    Ok(String::from_utf8_lossy(json::token_bytes(source, node)?).into_owned())
}

/// Parse a plain non-negative integer literal (`[0-9]+`). A sign, fraction, exponent,
/// or any other character declines, so `4.0`/`4e0`/`-0`/`0x4` are not integers here.
fn parse_nonneg_integer(tok: &str) -> Option<u32> {
    if tok.is_empty() {
        return None;
    }
    let mut v: u64 = 0;
    for b in tok.bytes() {
        if !b.is_ascii_digit() {
            return None;
        }
        v = v.checked_mul(10)?.checked_add((b - b'0') as u64)?;
        if v > u32::MAX as u64 {
            return None;
        }
    }
    Some(v as u32)
}

fn corrupt(msg: &str) -> Error {
    Error::invalid_model(format!("corrupt notebook model: {msg}"))
}

fn invalid_notebook(msg: impl Into<String>) -> Error {
    Error::invalid_notebook_structure(msg)
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(b: &'a [u8]) -> Self {
        Reader { b, at: 0 }
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

    const NOTEBOOK: &[u8] = br##"{
      "cells": [
        {"cell_type": "markdown", "metadata": {}, "source": ["# Title\n", "text\n"]},
        {"cell_type": "code", "execution_count": 1, "metadata": {},
         "outputs": [
           {"output_type": "stream", "name": "stdout", "text": ["hi\n"]},
           {"output_type": "execute_result", "execution_count": 1,
            "data": {"text/plain": "2"}, "metadata": {}}
         ],
         "source": "print(1 + 1)\n"},
        {"cell_type": "raw", "metadata": {}, "source": "raw bytes"}
      ],
      "metadata": {"kernelspec": {"name": "python3"}, "language_info": {"name": "python"}},
      "nbformat": 4,
      "nbformat_minor": 5
    }"##;

    fn model(src: &[u8]) -> NotebookModel {
        parse(src, Limits::DEFAULT, true).unwrap()
    }

    #[test]
    fn detects_notebook_and_rejects_json_and_junk() {
        assert!(detect(NOTEBOOK, Limits::DEFAULT));
        assert!(detect(br#"{"nbformat":4,"cells":[]}"#, Limits::DEFAULT));
        assert!(detect(
            br#"{"nbformat":3,"nbformat_minor":0,"cells":[{"cell_type":"code","source":""}]}"#,
            Limits::DEFAULT
        ));
        // Plain JSON, a bare cells key, and an unrelated object stay JSON.
        assert!(!detect(br#"{"a":1}"#, Limits::DEFAULT));
        assert!(!detect(
            br#"{"cells":[{"cell_type":"code"}]}"#,
            Limits::DEFAULT
        ));
        assert!(!detect(
            br#"{"nbformat":4,"cells":[1,2,3]}"#,
            Limits::DEFAULT
        ));
        assert!(!detect(br#"{"nbformat":"4","cells":[]}"#, Limits::DEFAULT));
        assert!(!detect(br#"{"nbformat":4.0,"cells":[]}"#, Limits::DEFAULT));
        assert!(!detect(
            br#"{"nbformat":4,"cells":[{"not_cell_type":1}]}"#,
            Limits::DEFAULT
        ));
        assert!(!detect(b"[1,2,3]", Limits::DEFAULT));
        // Prose and emptiness stay Opaque.
        assert!(!detect(b"the quick brown fox", Limits::DEFAULT));
        assert!(!detect(b"", Limits::DEFAULT));
    }

    #[test]
    fn cell_types_and_source_representation_are_preserved() {
        let m = model(NOTEBOOK);
        assert_eq!(m.nbformat, 4);
        assert_eq!(m.nbformat_minor, 5);
        assert_eq!(m.cell_count(), 3);
        assert_eq!(m.cells[0].class, C_MARKDOWN);
        assert_eq!(m.cells[1].class, C_CODE);
        assert_eq!(m.cells[2].class, C_RAW);
        // Markdown source is an array form; code source is a string form.
        assert_eq!(m.cells[0].source_form, SRC_LINES);
        assert_eq!(m.cells[1].source_form, SRC_STRING);
        assert_eq!(m.cells[2].source_form, SRC_STRING);
        // The exact cell_type token is preserved.
        let ct = m.node(m.cells[1].cell_type_node).unwrap();
        assert_eq!(token_bytes(NOTEBOOK, ct).unwrap(), b"\"code\"");
        // The two source forms are never conflated.
        let src_lines = m.node(m.cells[0].source_node).unwrap();
        assert_eq!(src_lines.kind, K_ARRAY);
        assert_eq!(src_lines.children.len(), 2);
        let src_str = m.node(m.cells[1].source_node).unwrap();
        assert_eq!(src_str.kind, K_STRING);
    }

    #[test]
    fn outputs_preserve_type_and_fields() {
        let m = model(NOTEBOOK);
        let code = &m.cells[1];
        assert_eq!(code.outputs.len(), 2);
        let o0 = code.outputs[0];
        assert_eq!(output_type_of(&m, NOTEBOOK, o0).unwrap().unwrap(), "stream");
        assert_eq!(
            member_string_value(&m, NOTEBOOK, o0, "name")
                .unwrap()
                .unwrap(),
            "stdout"
        );
        // A stream's `text` may be an array (preserved, never joined).
        let text = object_member(&m, NOTEBOOK, o0, "text").unwrap().unwrap();
        assert_eq!(m.node(text).unwrap().kind, K_ARRAY);
        let o1 = code.outputs[1];
        assert_eq!(
            output_type_of(&m, NOTEBOOK, o1).unwrap().unwrap(),
            "execute_result"
        );
        let data_keys = member_keys(&m, NOTEBOOK, o1, "data").unwrap().unwrap();
        assert_eq!(data_keys, vec!["text/plain"]);
    }

    #[test]
    fn attachments_and_metadata_order_are_preserved() {
        let src = br#"{"nbformat":4,"nbformat_minor":5,"metadata":{"a":1,"a":2},
          "cells":[{"cell_type":"markdown","source":"x",
          "attachments":{"img.png":{"image/png":"AAAA"},"b.txt":{"text/plain":"bb"}}}]}"#;
        let m = model(src);
        let cell = m.cells[0].node;
        let keys = member_keys(&m, src, cell, "attachments").unwrap().unwrap();
        assert_eq!(keys, vec!["img.png", "b.txt"]);
        // Duplicate metadata keys are preserved by the JSON model.
        let members = object_members(&m, src, m.metadata_node).unwrap();
        let mk: Vec<&str> = members.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(mk, vec!["a", "a"]);
    }

    #[test]
    fn model_roundtrips_and_rejects_corruption() {
        let m = model(NOTEBOOK);
        let bytes = m.encode();
        assert_eq!(NotebookModel::decode(&bytes).unwrap(), m);
        for cut in 0..bytes.len() {
            let _ = NotebookModel::decode(&bytes[..cut]);
        }
        let mut bad = bytes.clone();
        bad[0] ^= 0xFF;
        assert!(NotebookModel::decode(&bad).is_err());
    }

    #[test]
    fn malformed_and_inconsistent_decline_typed() {
        for src in [
            &b"{\"nbformat\":4}"[..],
            &b"{\"nbformat\":4,\"cells\":[1]}"[..],
            &b"{\"nbformat\":4,\"cells\":[{\"cell_type\":5}]}"[..],
            &b"{\"nbformat\":4,\"cells\":[{\"cell_type\":\"code\",\"source\":5}]}"[..],
            &b"{\"nbformat\":4,\"cells\":[{\"cell_type\":\"code\",\"outputs\":[1]}]}"[..],
            &b"[]"[..],
        ] {
            let e = parse(src, Limits::DEFAULT, true).unwrap_err();
            assert_eq!(
                e.class(),
                ErrorClass::InvalidNotebookStructure,
                "src={src:?}"
            );
        }
        // Malformed JSON is a typed decline too.
        let e = parse(b"{", Limits::DEFAULT, true).unwrap_err();
        assert_eq!(e.class(), ErrorClass::InvalidNotebookStructure);
    }

    #[test]
    fn caps_decline_typed() {
        let tight = Limits {
            max_notebook_cells: 1,
            ..Limits::DEFAULT
        };
        assert!(!detect(NOTEBOOK, tight));
        let e = parse(NOTEBOOK, tight, true).unwrap_err();
        assert_eq!(e.class(), ErrorClass::ResourceLimit);

        let tight_bytes = Limits {
            max_notebook_document_bytes: 4,
            ..Limits::DEFAULT
        };
        assert!(!detect(NOTEBOOK, tight_bytes));

        // Too many outputs declines typed.
        let mut big = Vec::new();
        big.extend_from_slice(b"{\"nbformat\":4,\"cells\":[{\"cell_type\":\"code\",\"outputs\":[");
        for i in 0..64 {
            if i > 0 {
                big.push(b',');
            }
            big.extend_from_slice(
                b"{\"output_type\":\"stream\",\"name\":\"stdout\",\"text\":\"x\"}",
            );
        }
        big.extend_from_slice(b"]}]}");
        let tight_out = Limits {
            max_notebook_outputs: 8,
            ..Limits::DEFAULT
        };
        assert!(parse(&big, Limits::DEFAULT, true).is_ok());
        let e = parse(&big, tight_out, true).unwrap_err();
        assert_eq!(e.class(), ErrorClass::ResourceLimit);
    }

    #[test]
    fn find_reuses_json_vocabulary() {
        let m = model(NOTEBOOK);
        let ms = find(&m, NOTEBOOK, "output_type", Limits::DEFAULT).unwrap();
        assert_eq!(ms.len(), 2);
        assert!(
            ms.iter()
                .all(|m| m.role == crate::adapter::json::MatchRole::Key)
        );
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut x: u64 = 0x0B0C_2024_1234_5678;
        for _ in 0..512 {
            let mut buf = vec![0u8; 256];
            for b in buf.iter_mut() {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                *b = (x & 0xFF) as u8;
            }
            let _ = detect(&buf, Limits::STRICT);
            if let Ok(m) = parse(&buf, Limits::STRICT, true) {
                let _ = m.encode();
                let _ = canonical_text(&m, &buf);
                let _ = find(&m, &buf, "a", Limits::STRICT);
                for c in &m.cells {
                    let _ = cell_type_of(&m, &buf, c.node);
                    for &o in &c.outputs {
                        let _ = output_type_of(&m, &buf, o);
                    }
                }
            }
        }
    }
}
