//! Bounded, representation-preserving GeoJSON (RFC 7946) adapter (Phase 21.22).
//!
//! GeoJSON is the **spatial** Wave-2 format. Its physical bytes are JSON, so the
//! representation-preserving JSON parser is the whole physical layer: this adapter
//! **reuses** [`crate::adapter::json`] — never a second JSON parser — and adds only a
//! bounded **semantic** projection on top. The exact leaf is still the whole source
//! (a `DocumentExact`, a RAW-like authority); everything this module produces is a
//! bounded, deterministic (`Q_gen`) projection that never sits on the exactness path.
//!
//! ## Reuse, not a second node arena
//!
//! The GeoJSON model embeds the JSON [`JsonModel`]: the **same** node arena the JSON
//! adapter defines (kind, exact `[start, end)` token span, ordered children). So
//! member **order**, **duplicate keys** (including a duplicate `type`/`id`/`bbox`/
//! any keyword), numeric **spelling** (`1e3`, `1.0`, `-0` are literal token spans),
//! string-escape **spelling**, and every token's **exact byte span** carry the
//! identical guarantees. A GeoJSON answer's spans are therefore all `Q_gen`
//! projections of the JSON parse; none of them is on the exactness path.
//!
//! ## What is preserved
//!
//! GeoJSON is a JSON object whose `"type"` member is one of the nine RFC 7946 type
//! names. This adapter records the **exact type string**, the geometry and feature
//! object nodes in document order, and exposes:
//!
//! * the seven geometry types (`Point`, `MultiPoint`, `LineString`,
//!   `MultiLineString`, `Polygon`, `MultiPolygon`, `GeometryCollection`), plus
//!   `Feature` and `FeatureCollection`;
//! * `coordinates` nesting (a position is `[lon, lat, (alt)]`) — never normalized,
//!   reordered, or reparsed: every coordinate number keeps its exact byte span and
//!   literal spelling;
//! * `properties` (arbitrary JSON, with member order and duplicate keys preserved by
//!   the JSON model), `id`, `bbox`, `geometry`, and `features` order;
//! * **foreign members** — any key other than the spec keys for the object's class —
//!   are preserved and reported, never dropped.
//!
//! ## Detection (a bounded semantic sub-test)
//!
//! GeoJSON shares its physical bytes with [`crate::adapter::json`], so detection is a
//! **bounded semantic test** run *before* the generic JSON detector
//! ([`crate::field::document_format`]). It reuses the JSON detector first, then
//! requires the source to parse as exactly one JSON value whose root is an object
//! with a `"type"` member equal to one of the nine GeoJSON type names **and** whose
//! shape is consistent:
//!
//! * a geometry (`Point`…`MultiPolygon`) has an **array** `coordinates` member;
//! * `GeometryCollection` has an **array** `geometries` member;
//! * `Feature` has `geometry` (a geometry object or `null`) and `properties` (an
//!   object or `null`); a present, non-null `geometry` must itself be a geometry;
//! * `FeatureCollection` has an **array** `features` member whose every element is a
//!   well-formed `Feature`.
//!
//! A plain JSON document (no `type`, or a non-object root) therefore stays
//! [`crate::field::document_format::DocumentFormat::Json`]; a JSON document whose
//! `"type"` is an unrelated string (e.g. a JSON Schema `"type":"object"`, a
//! JSON-LD `@type`, or a bare `{"type":"widget"}`) stays `Json`; and prose or
//! malformed input stays [`crate::field::document_format::DocumentFormat::Opaque`].
//!
//! ## Recorded boundaries (honest)
//!
//! * A `"type"` string that is one of the nine names but whose object does **not**
//!   satisfy the shape test above (e.g. `{"type":"Point"}` with no `coordinates`)
//!   is **not** claimed and stays `Json`.
//! * A geometry object's `coordinates` values are **not** validated against the
//!   RFC 7946 positional grammar (a position must be an array of 2+ numbers, a
//!   ring must be closed, …). The adapter preserves whatever JSON is present; it
//!   does not reject a structurally-shaped-but-numerically-invalid GeoJSON.
//! * A `null`/absent optional member (`bbox`, `id`) is recorded as absent, exactly
//!   as JSON has it.
//! * Duplicate `type`/`coordinates`/… members are preserved by the JSON model;
//!   classification uses the **first** `type` member, as a document reader would.
//!
//! ## Bounds and no panics
//!
//! Untrusted input can only ever yield a typed decline, never a panic or an
//! unbounded allocation: the source length by [`Limits::max_geojson_document_bytes`],
//! the embedded JSON node count by [`Limits::max_geojson_nodes`] (in addition to the
//! JSON caps the shared parser already enforces), the geometry and feature counts by
//! [`Limits::max_geojson_geometries`] / [`Limits::max_geojson_features`], the total
//! coordinate-number count by [`Limits::max_geojson_coordinates`], and the GeoJSON
//! structural recursion by [`Limits::max_geojson_depth`].

use crate::adapter::json::{
    self, JNode, JsonMatch, JsonModel, K_ARRAY, K_NULL, K_NUMBER, K_OBJECT, K_STRING,
};
use crate::error::{Error, ErrorClass, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model nodes (defends the decoder against a hostile blob).
pub const MAX_MODEL_NODES: u32 = 1 << 24;
/// Hard cap on decoded anchor indices (geometries + features).
pub const MAX_MODEL_ANCHORS: u32 = 1 << 24;

/// GeoJSON class: a `Point` geometry.
pub const G_POINT: u8 = 0;
/// GeoJSON class: a `MultiPoint` geometry.
pub const G_MULTIPOINT: u8 = 1;
/// GeoJSON class: a `LineString` geometry.
pub const G_LINESTRING: u8 = 2;
/// GeoJSON class: a `MultiLineString` geometry.
pub const G_MULTILINESTRING: u8 = 3;
/// GeoJSON class: a `Polygon` geometry.
pub const G_POLYGON: u8 = 4;
/// GeoJSON class: a `MultiPolygon` geometry.
pub const G_MULTIPOLYGON: u8 = 5;
/// GeoJSON class: a `GeometryCollection` geometry.
pub const G_GEOMETRYCOLLECTION: u8 = 6;
/// GeoJSON class: a `Feature`.
pub const G_FEATURE: u8 = 7;
/// GeoJSON class: a `FeatureCollection`.
pub const G_FEATURE_COLLECTION: u8 = 8;

/// The nine RFC 7946 type names, in class order.
pub const TYPE_NAMES: [&str; 9] = [
    "Point",
    "MultiPoint",
    "LineString",
    "MultiLineString",
    "Polygon",
    "MultiPolygon",
    "GeometryCollection",
    "Feature",
    "FeatureCollection",
];

/// The stable type name for a class tag.
pub const fn class_name(class: u8) -> &'static str {
    match class {
        G_POINT => "Point",
        G_MULTIPOINT => "MultiPoint",
        G_LINESTRING => "LineString",
        G_MULTILINESTRING => "MultiLineString",
        G_POLYGON => "Polygon",
        G_MULTIPOLYGON => "MultiPolygon",
        G_GEOMETRYCOLLECTION => "GeometryCollection",
        G_FEATURE => "Feature",
        G_FEATURE_COLLECTION => "FeatureCollection",
        _ => "unknown",
    }
}

/// The class tag for a GeoJSON type name, if it is one of the nine.
pub fn class_of(name: &str) -> Option<u8> {
    TYPE_NAMES.iter().position(|&n| n == name).map(|i| i as u8)
}

/// Whether `class` is one of the seven geometry classes (not Feature/FeatureCollection).
pub const fn is_geometry_class(class: u8) -> bool {
    class <= G_GEOMETRYCOLLECTION
}

/// Whether `key` is a spec member name for an object of `class` (everything else is a
/// foreign member, which is preserved and reported, never dropped).
pub fn is_spec_key(class: u8, key: &str) -> bool {
    match class {
        G_FEATURE => matches!(key, "type" | "geometry" | "properties" | "id" | "bbox"),
        G_FEATURE_COLLECTION => matches!(key, "type" | "features" | "bbox"),
        G_GEOMETRYCOLLECTION => matches!(key, "type" | "geometries" | "bbox"),
        _ => matches!(key, "type" | "coordinates" | "bbox"),
    }
}

/// The canonical derived GeoJSON model (the materialization of a `GeojsonModel`
/// node). It embeds the JSON [`JsonModel`] and stores the semantic anchors as
/// indices into that arena, so every span is a `Q_gen` projection of the JSON parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeojsonModel {
    /// The root object's class tag (`G_*`).
    pub class: u8,
    /// Index of the root node (a JSON object).
    pub root: u32,
    /// Index of the root's `"type"` **value** string node.
    pub type_node: u32,
    /// The geometry object node indices in document order (pre-order; a
    /// `GeometryCollection`'s members are included).
    pub geometries: Vec<u32>,
    /// The `Feature` object node indices in document order (the root `Feature`, or
    /// every element of a `FeatureCollection`'s `features` array).
    pub features: Vec<u32>,
    /// The embedded representation-preserving JSON arena.
    pub json: JsonModel,
}

impl GeojsonModel {
    /// The JSON node at `index`, if present.
    pub fn node(&self, index: u32) -> Option<&JNode> {
        self.json.node(index)
    }

    /// The number of geometry objects in the model.
    pub fn geometry_count(&self) -> u32 {
        self.geometries.len() as u32
    }

    /// The number of features in the model.
    pub fn feature_count(&self) -> u32 {
        self.features.len() as u32
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let json_bytes = self.json.encode();
        let mut out = Vec::with_capacity(
            48 + json_bytes.len() + (self.geometries.len() + self.features.len()) * 4,
        );
        out.extend_from_slice(b"GEOJ");
        out.push(MODEL_VERSION);
        out.push(self.class);
        out.extend_from_slice(&[0, 0]); // reserved
        out.extend_from_slice(&self.doc_len().to_le_bytes());
        out.extend_from_slice(&self.root.to_le_bytes());
        out.extend_from_slice(&self.type_node.to_le_bytes());
        out.extend_from_slice(&(self.geometries.len() as u32).to_le_bytes());
        for g in &self.geometries {
            out.extend_from_slice(&g.to_le_bytes());
        }
        out.extend_from_slice(&(self.features.len() as u32).to_le_bytes());
        for f in &self.features {
            out.extend_from_slice(&f.to_le_bytes());
        }
        out.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&json_bytes);
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<GeojsonModel> {
        let mut r = Reader::new(bytes);
        if r.bytes(4)? != b"GEOJ" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let class = r.u8()?;
        if class > G_FEATURE_COLLECTION {
            return Err(corrupt("unknown GeoJSON class"));
        }
        if r.bytes(2)? != [0, 0] {
            return Err(corrupt("unknown model flags"));
        }
        let doc_len = r.u64()?;
        let root = r.u32()?;
        let type_node = r.u32()?;
        let gn = r.u32()?;
        if gn > MAX_MODEL_ANCHORS {
            return Err(corrupt("geometry count is implausible"));
        }
        let mut geometries = Vec::with_capacity(gn as usize);
        for _ in 0..gn {
            geometries.push(r.u32()?);
        }
        let fn_ = r.u32()?;
        if fn_ > MAX_MODEL_ANCHORS {
            return Err(corrupt("feature count is implausible"));
        }
        let mut features = Vec::with_capacity(fn_ as usize);
        for _ in 0..fn_ {
            features.push(r.u32()?);
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
        if root >= nnodes || type_node >= nnodes {
            return Err(corrupt("model anchor is out of range"));
        }
        for &g in &geometries {
            if g >= nnodes {
                return Err(corrupt("geometry index is out of range"));
            }
        }
        for &f in &features {
            if f >= nnodes {
                return Err(corrupt("feature index is out of range"));
            }
        }
        Ok(GeojsonModel {
            class,
            root,
            type_node,
            geometries,
            features,
            json,
        })
    }

    /// The exact source length the spans are relative to.
    pub fn doc_len(&self) -> u64 {
        self.json.doc_len
    }

    /// The recorded class tag of the whole document.
    pub fn root_class(&self) -> u8 {
        self.class
    }
}

/// A lexical match from [`find`]: a record over the shared JSON match vocabulary
/// (canonical RFC 6901 pointer, key/value role, exact span, decoded text).
pub type GeojsonMatch = JsonMatch;

/// Byte-based, conservative GeoJSON detector. See the module docs for the exact
/// semantic boundary.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.len() as u64 > limits.max_geojson_document_bytes {
        return false;
    }
    // Reuse the JSON detector first: a GeoJSON document is a JSON document.
    if !json::detect(source, limits) {
        return false;
    }
    parse(source, limits, false).is_ok()
}

/// Classify `source` as one GeoJSON class tag, or decline typed when it is not a
/// structurally-consistent GeoJSON document under the semantic test.
pub fn classify(source: &[u8], limits: Limits) -> Result<u8> {
    Ok(parse(source, limits, false)?.class)
}

/// Parse `source` into a [`GeojsonModel`]. `build` selects whether the semantic
/// anchor vectors (geometries/features) are retained (detection runs with
/// `build = false`); the JSON arena is always built because the semantic test walks
/// it. The shared JSON parser enforces the JSON caps; this function adds the GeoJSON
/// caps on top.
pub fn parse(source: &[u8], limits: Limits, build: bool) -> Result<GeojsonModel> {
    if source.len() as u64 > limits.max_geojson_document_bytes {
        return Err(Error::resource_limit(format!(
            "GeoJSON source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_geojson_document_bytes
        )));
    }
    // Reuse the shared JSON parser: a GeoJSON source must be exactly one JSON value.
    // A malformed source becomes a typed GeoJSON decline (resource errors are kept as
    // resource errors so caps are never laundered into structure errors).
    let json = json::parse(source, limits, true).map_err(|e| match e.class() {
        ErrorClass::ResourceLimit => e,
        _ => invalid_geojson(format!(
            "GeoJSON source is not a single JSON value: {}",
            e.message()
        )),
    })?;
    if json.nodes.len() as u64 > limits.max_geojson_nodes as u64 {
        return Err(Error::resource_limit(format!(
            "GeoJSON document exceeds the {}-node cap",
            limits.max_geojson_nodes
        )));
    }
    let (class, type_node, geometries_all, features_all) = analyze(&json, source, limits)?;
    Ok(GeojsonModel {
        class,
        root: json.root,
        type_node,
        geometries: if build { geometries_all } else { Vec::new() },
        features: if build { features_all } else { Vec::new() },
        json,
    })
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of a `GeojsonModel` node).
pub fn build_geojson_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits, true)?.encode())
}

// ---------------------------------------------------------------------------
// Semantic projections over the embedded JSON model
// ---------------------------------------------------------------------------

/// The value node of the **first** object member named `key` (duplicate keys are
/// preserved, but a reader resolves the first, exactly like the JSON adapter).
pub fn object_member(
    model: &GeojsonModel,
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
pub fn object_members(model: &GeojsonModel, source: &[u8], obj: u32) -> Result<Vec<(String, u32)>> {
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

/// The exact token bytes of a node (`[start, end)`), bounded by the source length.
pub fn token_bytes<'a>(source: &'a [u8], node: &JNode) -> Result<&'a [u8]> {
    json::token_bytes(source, node)
}

/// Decode a JSON string node to its Rust text.
pub fn decode_string(source: &[u8], node: &JNode) -> Result<String> {
    json::decode_string(source, node)
}

/// The `"type"` value string of an object node, if it is an object with a string
/// `"type"` member (the first, if duplicated).
pub fn type_of_object(model: &GeojsonModel, source: &[u8], obj: u32) -> Result<Option<String>> {
    match object_member(model, source, obj, "type")? {
        Some(v) => {
            let n = model
                .node(v)
                .ok_or_else(|| corrupt("type value index is out of range"))?;
            if n.kind == K_STRING {
                Ok(Some(json::decode_string(source, n)?))
            } else {
                Ok(None)
            }
        }
        None => Ok(None),
    }
}

/// The exact token span `[start, end)` of a node.
pub fn node_span(model: &GeojsonModel, index: u32) -> Result<(u64, u64)> {
    let n = model
        .node(index)
        .ok_or_else(|| corrupt("node index is out of range"))?;
    Ok((n.start, n.end))
}

/// The `coordinates` member value node of a (non-collection) geometry, if present.
pub fn coordinates_node(model: &GeojsonModel, source: &[u8], geom: u32) -> Result<Option<u32>> {
    object_member(model, source, geom, "coordinates")
}

/// The `geometries` member value node of a `GeometryCollection`, if present.
pub fn geometries_node(model: &GeojsonModel, source: &[u8], geom: u32) -> Result<Option<u32>> {
    object_member(model, source, geom, "geometries")
}

/// Every numeric leaf node under `node` in document order (a coordinate number, its
/// exact byte span, and its literal spelling preserved). Enforces
/// [`Limits::max_geojson_coordinates`] and [`Limits::max_geojson_depth`].
pub fn coordinate_numbers(model: &GeojsonModel, node: u32, limits: Limits) -> Result<Vec<u32>> {
    let mut out: Vec<u32> = Vec::new();
    collect_number_leaves(model, node, limits, &mut out, 1)?;
    Ok(out)
}

fn collect_number_leaves(
    model: &GeojsonModel,
    index: u32,
    limits: Limits,
    out: &mut Vec<u32>,
    depth: u32,
) -> Result<()> {
    if depth > limits.max_geojson_depth {
        return Err(Error::resource_limit(format!(
            "GeoJSON coordinate nesting exceeds the {}-level cap",
            limits.max_geojson_depth
        )));
    }
    let n = model
        .node(index)
        .ok_or_else(|| corrupt("coordinate node index is out of range"))?;
    match n.kind {
        K_NUMBER => {
            if out.len() as u64 >= limits.max_geojson_coordinates {
                return Err(Error::resource_limit(format!(
                    "GeoJSON coordinates exceed the {}-number cap",
                    limits.max_geojson_coordinates
                )));
            }
            out.push(index);
        }
        K_ARRAY | K_OBJECT => {
            for &c in &n.children {
                collect_number_leaves(model, c, limits, out, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// The **foreign members** of an object: every member whose key is not a spec key for
/// the object's class, in document order. Never dropped; a foreign member is a
/// first-class, reportable fact.
pub fn foreign_members(
    model: &GeojsonModel,
    source: &[u8],
    obj: u32,
) -> Result<Vec<(String, u32)>> {
    let class = match type_of_object(model, source, obj)? {
        Some(name) => class_of(&name).unwrap_or(G_FEATURE_COLLECTION),
        None => G_FEATURE_COLLECTION,
    };
    let mut out = Vec::new();
    for (key, val) in object_members(model, source, obj)? {
        if !is_spec_key(class, &key) {
            out.push((key, val));
        }
    }
    Ok(out)
}

/// A bounded, case-sensitive lexical search over object keys and string values,
/// reusing the shared JSON [`json::find`] over the embedded arena. Matches are
/// returned in document order with their canonical pointers and exact spans.
pub fn find(
    model: &GeojsonModel,
    source: &[u8],
    pattern: &str,
    limits: Limits,
) -> Result<Vec<GeojsonMatch>> {
    json::find(&model.json, source, pattern, limits)
}

/// A deterministic canonical text projection of the whole document (the shared JSON
/// canonical text: member order and token spelling preserved).
pub fn canonical_text(model: &GeojsonModel, source: &[u8]) -> Result<String> {
    json::canonical_text(&model.json, source)
}

// ---------------------------------------------------------------------------
// Internals: the semantic test and anchor collection
// ---------------------------------------------------------------------------

fn analyze(
    json: &JsonModel,
    source: &[u8],
    limits: Limits,
) -> Result<(u8, u32, Vec<u32>, Vec<u32>)> {
    let root = json.root;
    let root_node = json
        .node(root)
        .ok_or_else(|| corrupt("root index is out of range"))?;
    if root_node.kind != K_OBJECT {
        return Err(invalid_geojson(
            "GeoJSON root must be a JSON object with a string \"type\" member",
        ));
    }
    // The first string "type" member is the classification key.
    let type_node = object_member_json(json, source, root, "type")?
        .ok_or_else(|| invalid_geojson("GeoJSON root object has no \"type\" member"))?;
    let tn = json
        .node(type_node)
        .ok_or_else(|| corrupt("type node index is out of range"))?;
    if tn.kind != K_STRING {
        return Err(invalid_geojson(
            "GeoJSON root \"type\" member is not a string",
        ));
    }
    let type_name = json::decode_string(source, tn)?;
    let class = class_of(&type_name).ok_or_else(|| {
        invalid_geojson(format!(
            "GeoJSON root \"type\" {type_name:?} is not a GeoJSON type name"
        ))
    })?;

    let mut geometries: Vec<u32> = Vec::new();
    let mut features: Vec<u32> = Vec::new();

    match class {
        G_FEATURE => {
            let geom = object_member_json(json, source, root, "geometry")?
                .ok_or_else(|| invalid_geojson("GeoJSON Feature has no \"geometry\" member"))?;
            let gn = json
                .node(geom)
                .ok_or_else(|| corrupt("Feature geometry index is out of range"))?;
            // `properties` is required by RFC 7946 (an object or null).
            let props = object_member_json(json, source, root, "properties")?
                .ok_or_else(|| invalid_geojson("GeoJSON Feature has no \"properties\" member"))?;
            let pn = json
                .node(props)
                .ok_or_else(|| corrupt("Feature properties index is out of range"))?;
            if pn.kind != K_OBJECT && pn.kind != K_NULL {
                return Err(invalid_geojson(
                    "GeoJSON Feature \"properties\" must be an object or null",
                ));
            }
            features.push(root);
            if gn.kind == K_OBJECT {
                let gclass = geometry_class_of_object(json, source, geom)?;
                collect_geometry(json, source, geom, gclass, limits, 2, &mut geometries)?;
            } else if gn.kind != K_NULL {
                return Err(invalid_geojson(
                    "GeoJSON Feature \"geometry\" must be a geometry object or null",
                ));
            }
        }
        G_FEATURE_COLLECTION => {
            let feats = object_member_json(json, source, root, "features")?.ok_or_else(|| {
                invalid_geojson("GeoJSON FeatureCollection has no \"features\" member")
            })?;
            let fn_ = json
                .node(feats)
                .ok_or_else(|| corrupt("FeatureCollection features index is out of range"))?;
            if fn_.kind != K_ARRAY {
                return Err(invalid_geojson(
                    "GeoJSON FeatureCollection \"features\" must be an array",
                ));
            }
            for &feat in &fn_.children {
                if features.len() as u64 >= limits.max_geojson_features as u64 {
                    return Err(Error::resource_limit(format!(
                        "GeoJSON FeatureCollection exceeds the {}-feature cap",
                        limits.max_geojson_features
                    )));
                }
                analyze_feature(json, source, feat, limits, &mut geometries, &mut features)?;
            }
        }
        _ => {
            // A bare geometry root.
            collect_geometry(json, source, root, class, limits, 1, &mut geometries)?;
        }
    }

    if features.len() as u64 > limits.max_geojson_features as u64 {
        return Err(Error::resource_limit(format!(
            "GeoJSON exceeds the {}-feature cap",
            limits.max_geojson_features
        )));
    }
    Ok((class, type_node, geometries, features))
}

/// Validate one `Feature` element (inside a `FeatureCollection`) and collect its
/// geometry, in document order.
fn analyze_feature(
    json: &JsonModel,
    source: &[u8],
    feat: u32,
    limits: Limits,
    geometries: &mut Vec<u32>,
    features: &mut Vec<u32>,
) -> Result<()> {
    let n = json
        .node(feat)
        .ok_or_else(|| corrupt("feature index is out of range"))?;
    if n.kind != K_OBJECT {
        return Err(invalid_geojson(
            "GeoJSON FeatureCollection element is not an object",
        ));
    }
    let tnode = object_member_json(json, source, feat, "type")?
        .ok_or_else(|| invalid_geojson("GeoJSON Feature has no \"type\" member"))?;
    let tval = json
        .node(tnode)
        .ok_or_else(|| corrupt("feature type index is out of range"))?;
    if tval.kind != K_STRING || json::decode_string(source, tval)? != "Feature" {
        return Err(invalid_geojson(
            "GeoJSON FeatureCollection element is not a Feature",
        ));
    }
    let geom = object_member_json(json, source, feat, "geometry")?
        .ok_or_else(|| invalid_geojson("GeoJSON Feature has no \"geometry\" member"))?;
    let props = object_member_json(json, source, feat, "properties")?
        .ok_or_else(|| invalid_geojson("GeoJSON Feature has no \"properties\" member"))?;
    let gn = json
        .node(geom)
        .ok_or_else(|| corrupt("feature geometry index is out of range"))?;
    let pn = json
        .node(props)
        .ok_or_else(|| corrupt("feature properties index is out of range"))?;
    if pn.kind != K_OBJECT && pn.kind != K_NULL {
        return Err(invalid_geojson(
            "GeoJSON Feature \"properties\" must be an object or null",
        ));
    }
    features.push(feat);
    if gn.kind == K_OBJECT {
        let gclass = geometry_class_of_object(json, source, geom)?;
        collect_geometry(json, source, geom, gclass, limits, 3, geometries)?;
    } else if gn.kind != K_NULL {
        return Err(invalid_geojson(
            "GeoJSON Feature \"geometry\" must be a geometry object or null",
        ));
    }
    Ok(())
}

/// The geometry class of an object node whose `"type"` is a geometry type name.
fn geometry_class_of_object(json: &JsonModel, source: &[u8], obj: u32) -> Result<u8> {
    let tnode = object_member_json(json, source, obj, "type")?
        .ok_or_else(|| invalid_geojson("GeoJSON geometry has no \"type\" member"))?;
    let tval = json
        .node(tnode)
        .ok_or_else(|| corrupt("geometry type index is out of range"))?;
    if tval.kind != K_STRING {
        return Err(invalid_geojson("GeoJSON geometry \"type\" is not a string"));
    }
    let name = json::decode_string(source, tval)?;
    match class_of(&name) {
        Some(c) if is_geometry_class(c) => Ok(c),
        _ => Err(invalid_geojson(format!(
            "GeoJSON geometry \"type\" {name:?} is not a geometry type name"
        ))),
    }
}

/// Collect a geometry object and (for a collection) its members in pre-order, with
/// the geometry-count and depth caps enforced.
fn collect_geometry(
    json: &JsonModel,
    source: &[u8],
    node: u32,
    class: u8,
    limits: Limits,
    depth: u32,
    out: &mut Vec<u32>,
) -> Result<()> {
    if depth > limits.max_geojson_depth {
        return Err(Error::resource_limit(format!(
            "GeoJSON geometry nesting exceeds the {}-level cap",
            limits.max_geojson_depth
        )));
    }
    if out.len() as u64 >= limits.max_geojson_geometries as u64 {
        return Err(Error::resource_limit(format!(
            "GeoJSON exceeds the {}-geometry cap",
            limits.max_geojson_geometries
        )));
    }
    if class == G_GEOMETRYCOLLECTION {
        let g = object_member_json(json, source, node, "geometries")?.ok_or_else(|| {
            invalid_geojson("GeoJSON GeometryCollection has no \"geometries\" member")
        })?;
        let gn = json
            .node(g)
            .ok_or_else(|| corrupt("GeometryCollection geometries index is out of range"))?;
        if gn.kind != K_ARRAY {
            return Err(invalid_geojson(
                "GeoJSON GeometryCollection \"geometries\" must be an array",
            ));
        }
    } else {
        let c = object_member_json(json, source, node, "coordinates")?
            .ok_or_else(|| invalid_geojson("GeoJSON geometry has no \"coordinates\" member"))?;
        let cn = json
            .node(c)
            .ok_or_else(|| corrupt("geometry coordinates index is out of range"))?;
        if cn.kind != K_ARRAY {
            return Err(invalid_geojson(
                "GeoJSON geometry \"coordinates\" must be an array",
            ));
        }
    }
    out.push(node);
    if class == G_GEOMETRYCOLLECTION {
        let g = object_member_json(json, source, node, "geometries")?.ok_or_else(|| {
            invalid_geojson("GeoJSON GeometryCollection has no \"geometries\" member")
        })?;
        let gn = json
            .node(g)
            .ok_or_else(|| corrupt("GeometryCollection geometries index is out of range"))?;
        for &child in &gn.children {
            let cclass = geometry_class_of_object(json, source, child)?;
            collect_geometry(json, source, child, cclass, limits, depth + 1, out)?;
        }
    }
    Ok(())
}

/// The value node of the first object member named `key`, operating directly on the
/// embedded JSON arena (used by the semantic test before a model exists).
fn object_member_json(json: &JsonModel, source: &[u8], obj: u32, key: &str) -> Result<Option<u32>> {
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

fn corrupt(msg: &str) -> Error {
    Error::invalid_model(format!("corrupt GeoJSON model: {msg}"))
}

fn invalid_geojson(msg: impl Into<String>) -> Error {
    Error::invalid_geojson_structure(msg)
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

    const FEATURE_COLLECTION: &[u8] = br#"{
      "type": "FeatureCollection",
      "bbox": [100.0, 0.0, 105.0, 1.0],
      "features": [
        {"type":"Feature","id":1,
         "geometry":{"type":"Point","coordinates":[1.25,-2.5e1]},
         "properties":{"name":"A","name":"B","n":1e3},
         "vendor":"x"},
        {"type":"Feature","id":2,
         "geometry":{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,0]]]},
         "properties":null}
      ],
      "title": "kept"
    }"#;

    fn model(src: &[u8]) -> GeojsonModel {
        parse(src, Limits::DEFAULT, true).unwrap()
    }

    #[test]
    fn detects_geojson_and_rejects_json_and_junk() {
        assert!(detect(FEATURE_COLLECTION, Limits::DEFAULT));
        assert!(detect(
            br#"{"type":"GeometryCollection","geometries":[]}"#,
            Limits::DEFAULT
        ));
        assert!(detect(
            b"{\"type\":\"Point\",\"coordinates\":[0,0]}",
            Limits::DEFAULT
        ));
        // Plain JSON with no GeoJSON type is not GeoJSON.
        assert!(!detect(b"{\"a\":1}", Limits::DEFAULT));
        assert!(!detect(b"[1,2,3]", Limits::DEFAULT));
        // An unrelated "type" string stays JSON.
        assert!(!detect(b"{\"type\":\"object\"}", Limits::DEFAULT));
        assert!(!detect(b"{\"type\":\"widget\",\"x\":1}", Limits::DEFAULT));
        // A GeoJSON type name with no consistent shape is not claimed.
        assert!(!detect(b"{\"type\":\"Point\"}", Limits::DEFAULT));
        assert!(!detect(
            b"{\"type\":\"FeatureCollection\"}",
            Limits::DEFAULT
        ));
        assert!(!detect(
            b"{\"type\":\"Feature\",\"geometry\":null}",
            Limits::DEFAULT
        ));
        // Prose is not JSON, let alone GeoJSON.
        assert!(!detect(b"the quick brown fox", Limits::DEFAULT));
        assert!(!detect(b"", Limits::DEFAULT));
    }

    #[test]
    fn root_type_and_anchors_are_preserved() {
        let m = model(FEATURE_COLLECTION);
        assert_eq!(m.class, G_FEATURE_COLLECTION);
        assert_eq!(m.root_class(), G_FEATURE_COLLECTION);
        assert_eq!(m.feature_count(), 2);
        assert_eq!(m.geometry_count(), 2);
        // The exact type token bytes are preserved.
        let tn = m.node(m.type_node).unwrap();
        assert_eq!(
            token_bytes(FEATURE_COLLECTION, tn).unwrap(),
            br#""FeatureCollection""#
        );
        assert_eq!(
            type_of_object(&m, FEATURE_COLLECTION, m.root)
                .unwrap()
                .unwrap(),
            "FeatureCollection"
        );
        // Features are in document order.
        let f0 = m.node(m.features[0]).unwrap();
        let f1 = m.node(m.features[1]).unwrap();
        assert!(f0.start < f1.start);
    }

    #[test]
    fn coordinates_preserve_spelling_and_spans() {
        let m = model(FEATURE_COLLECTION);
        let point = m.geometries[0];
        let coords = coordinates_node(&m, FEATURE_COLLECTION, point)
            .unwrap()
            .unwrap();
        let nums = coordinate_numbers(&m, coords, Limits::DEFAULT).unwrap();
        assert_eq!(nums.len(), 2);
        let t0 = token_bytes(FEATURE_COLLECTION, m.node(nums[0]).unwrap()).unwrap();
        let t1 = token_bytes(FEATURE_COLLECTION, m.node(nums[1]).unwrap()).unwrap();
        assert_eq!(t0, b"1.25");
        // Numeric spelling preserved verbatim (not reparsed).
        assert_eq!(t1, b"-2.5e1");
    }

    #[test]
    fn properties_keep_order_and_duplicate_keys() {
        let m = model(FEATURE_COLLECTION);
        let props = object_member(&m, FEATURE_COLLECTION, m.features[0], "properties")
            .unwrap()
            .unwrap();
        let members = object_members(&m, FEATURE_COLLECTION, props).unwrap();
        let keys: Vec<&str> = members.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["name", "name", "n"]);
    }

    #[test]
    fn foreign_members_are_reported_not_dropped() {
        let m = model(FEATURE_COLLECTION);
        // Root foreign member "title" (not a FeatureCollection spec key).
        let root_foreign = foreign_members(&m, FEATURE_COLLECTION, m.root).unwrap();
        assert_eq!(root_foreign.len(), 1);
        assert_eq!(root_foreign[0].0, "title");
        // Feature 0 foreign member "vendor".
        let f_foreign = foreign_members(&m, FEATURE_COLLECTION, m.features[0]).unwrap();
        assert_eq!(f_foreign.len(), 1);
        assert_eq!(f_foreign[0].0, "vendor");
    }

    #[test]
    fn all_seven_geometry_types_are_recognized() {
        let cases: [&[u8]; 7] = [
            br#"{"type":"Point","coordinates":[0,0]}"#,
            br#"{"type":"MultiPoint","coordinates":[[0,0]]}"#,
            br#"{"type":"LineString","coordinates":[[0,0],[1,1]]}"#,
            br#"{"type":"MultiLineString","coordinates":[[[0,0],[1,1]]]}"#,
            br#"{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,0]]]}"#,
            br#"{"type":"MultiPolygon","coordinates":[[[[0,0],[1,0],[1,1],[0,0]]]]}"#,
            br#"{"type":"GeometryCollection","geometries":[{"type":"Point","coordinates":[0,0]}]}"#,
        ];
        for (i, src) in cases.iter().enumerate() {
            assert!(detect(src, Limits::DEFAULT), "case {i} not detected");
            let m = model(src);
            assert_eq!(m.class as usize, i);
            assert_eq!(m.geometry_count(), if i == 6 { 2 } else { 1 });
        }
    }

    #[test]
    fn model_roundtrips_and_rejects_corruption() {
        let m = model(FEATURE_COLLECTION);
        let bytes = m.encode();
        assert_eq!(GeojsonModel::decode(&bytes).unwrap(), m);
        for cut in 0..bytes.len() {
            let _ = GeojsonModel::decode(&bytes[..cut]);
        }
        let mut bad = bytes.clone();
        bad[0] ^= 0xFF;
        assert!(GeojsonModel::decode(&bad).is_err());
    }

    #[test]
    fn malformed_and_inconsistent_decline_typed() {
        // Valid JSON but not GeoJSON → InvalidGeojsonStructure.
        for src in [
            &b"{\"type\":\"object\"}"[..],
            &b"{\"type\":\"Point\"}"[..],
            &b"{\"type\":\"Feature\",\"geometry\":null}"[..],
            &b"[]"[..],
        ] {
            let e = parse(src, Limits::DEFAULT, true).unwrap_err();
            assert_eq!(
                e.class(),
                ErrorClass::InvalidGeojsonStructure,
                "src={src:?}"
            );
        }
        // Malformed JSON → typed decline too.
        let e = parse(b"{", Limits::DEFAULT, true).unwrap_err();
        assert_eq!(e.class(), ErrorClass::InvalidGeojsonStructure);
    }

    #[test]
    fn caps_decline_typed() {
        let tight = Limits {
            max_geojson_features: 1,
            ..Limits::DEFAULT
        };
        assert!(!detect(FEATURE_COLLECTION, tight));
        let e = parse(FEATURE_COLLECTION, tight, true).unwrap_err();
        assert_eq!(e.class(), ErrorClass::ResourceLimit);

        let tight_bytes = Limits {
            max_geojson_document_bytes: 4,
            ..Limits::DEFAULT
        };
        assert!(!detect(FEATURE_COLLECTION, tight_bytes));

        // A dense coordinate array over the coordinate cap declines typed.
        let mut big = Vec::new();
        big.extend_from_slice(b"{\"type\":\"LineString\",\"coordinates\":[");
        for i in 0..64 {
            if i > 0 {
                big.push(b',');
            }
            big.extend_from_slice(b"[0,0]");
        }
        big.extend_from_slice(b"]}");
        let tight_coords = Limits {
            max_geojson_coordinates: 8,
            ..Limits::DEFAULT
        };
        let m = parse(&big, Limits::DEFAULT, true).unwrap();
        let coords = coordinates_node(&m, &big, m.geometries[0])
            .unwrap()
            .unwrap();
        let e = coordinate_numbers(&m, coords, tight_coords).unwrap_err();
        assert_eq!(e.class(), ErrorClass::ResourceLimit);
    }

    #[test]
    fn find_reuses_json_vocabulary() {
        let m = model(FEATURE_COLLECTION);
        let ms = find(&m, FEATURE_COLLECTION, "name", Limits::DEFAULT).unwrap();
        // Two "name" keys (duplicates preserved) → both reported.
        assert_eq!(ms.len(), 2);
        assert!(
            ms.iter()
                .all(|m| m.role == crate::adapter::json::MatchRole::Key)
        );
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
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
                for &g in &m.geometries {
                    let _ = coordinates_node(&m, &buf, g);
                }
            }
        }
    }
}
