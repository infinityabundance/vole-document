//! Bounded, representation-preserving **API / specification** adapter (Phase 21.30).
//!
//! The API-specification family — JSON Schema, OpenAPI 3.x, Swagger 2.0, and
//! AsyncAPI — is written in JSON, so its physical bytes belong to another format: the
//! shared [`crate::adapter::json`] parser. This adapter **reuses** that
//! representation-preserving JSON arena (never a second JSON parser) and adds only a
//! bounded **semantic** projection on top. The exact leaf is still the whole source (a
//! `DocumentExact`, a RAW-like authority); everything this module produces is a
//! bounded, deterministic (`Q_gen`) projection that never sits on the exactness path.
//!
//! ## Reuse, not a second node arena
//!
//! The model embeds the JSON [`JsonModel`] — the **same** node arena the JSON adapter
//! defines (kind, exact `[start, end)` token span, ordered children). So member
//! **order**, **duplicate keys** (including a duplicate `$ref`/`paths`/any keyword),
//! numeric **spelling**, string-escape **spelling**, key **quoting**, and every
//! token's **exact byte span** carry the identical guarantees they carry in the base
//! format. An API-spec answer's spans are therefore all `Q_gen` projections of the
//! reused parse; none is on the exactness path. **Nothing is normalized, resolved, or
//! re-serialized.**
//!
//! ## What is preserved
//!
//! An API specification is a JSON object with a **recorded dialect** and a
//! **spec-version string**. This adapter records, in document order:
//!
//! * every **object** in the document (the root, every object member value, and every
//!   object reached inside an array), with its **role**, its exact value span, its
//!   naming key's exact span (when named), and its member range;
//! * every **member** (a `"key": value` pair) of every recorded object, with its exact
//!   key token span and value token span, in document order (duplicates preserved);
//! * every **`$ref`** member whose value is a string, with its exact value span — the
//!   reference target is preserved **verbatim** and is **never resolved**, dereferenced,
//!   or fetched;
//! * the **spec-version string** (the `$schema` / `openapi` / `swagger` / `asyncapi`
//!   value) as an exact token span, never reparsed or normalized.
//!
//! The **dialect** ([`D_JSON_SCHEMA`], [`D_OPENAPI`], [`D_SWAGGER`], [`D_ASYNCAPI`])
//! is recorded, exactly as the CSV/config/feed/pkgmeta adapters record theirs.
//!
//! Because the JSON tree *is* the schema/spec graph, the recorded member/object
//! projection exposes the JSON-Schema keyword tree (`type`/`properties`/`items`/
//! `$defs`/`definitions`/`required`/`enum`/`allOf`/`oneOf`/`anyOf`/`format`…), the
//! OpenAPI `paths` + operations (`get`/`post`/…), operation `parameters`/`responses`/
//! `requestBody`, the `components` (`schemas`/`parameters`/`responses`/
//! `securitySchemes`), the Swagger 2.0 `basePath`/`definitions`/`parameters`/
//! `responses`, and the AsyncAPI `channels`/`components`/`operations`/`servers` — all
//! as ordered, exact-span facts. Keyword **spelling** is the exact key token.
//!
//! ## Detection (a bounded semantic sub-test, content-only)
//!
//! An API spec's bytes are JSON, so detection is a **bounded semantic test** run
//! *before* the generic JSON detector
//! ([`crate::field::document_format::detect_document_format`]) and **never** consults a
//! file name. It is deliberately conservative and claims only on a **strong, root-level
//! string marker**:
//!
//! * **`json_schema`** — a root object with a string `$schema` whose value names a
//!   JSON-Schema URI (it contains `json-schema.org`);
//! * **`openapi`** — a root object with a string `openapi` whose value begins with
//!   `3.` (OpenAPI 3.x);
//! * **`swagger`** — a root object with a string `swagger` equal to `"2.0"`;
//! * **`asyncapi`** — a root object with a non-empty string `asyncapi`.
//!
//! ## Recorded boundaries (honest)
//!
//! * A JSON-Schema-**shaped** object with **no** `$schema` (e.g.
//!   `{"type":"object","properties":{…}}`) is **byte-for-byte indistinguishable** from
//!   a plain JSON tree and is deliberately **not** claimed; it stays
//!   [`crate::field::document_format::DocumentFormat::Json`].
//! * A plain JSON object that merely contains a `properties`/`paths`/`components` key
//!   but lacks the marker also stays `Json`.
//! * An `openapi` value that is not a `3.`-prefixed string, and a `swagger` value that
//!   is not exactly `"2.0"`, are not claimed.
//! * `$ref` targets are **never resolved**: a dangling, external, or cyclic reference
//!   is preserved verbatim, exactly as a document reader would see it.
//! * A spec-shaped-but-semantically-odd document is preserved verbatim; the adapter
//!   records what is present and never rejects it.
//!
//! ## Bounds and no panics
//!
//! Untrusted input can only ever yield a typed decline, never a panic or an unbounded
//! allocation: the source length by [`Limits::max_apispec_document_bytes`], the reused
//! arena node count by [`Limits::max_apispec_nodes`] (in addition to the JSON caps the
//! shared parser already enforces), the recorded object count by
//! [`Limits::max_apispec_objects`], the member count by [`Limits::max_apispec_members`],
//! the `$ref` count by [`Limits::max_apispec_refs`], and the structural recursion by
//! [`Limits::max_apispec_depth`].

use crate::adapter::json::{self, JNode, JsonMatch, JsonModel, K_ARRAY, K_OBJECT, K_STRING};
use crate::error::{Error, ErrorClass, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded objects (defends the decoder against a hostile blob).
pub const MAX_MODEL_OBJECTS: u32 = 1 << 24;
/// Hard cap on decoded members (defends the decoder against a hostile blob).
pub const MAX_MODEL_MEMBERS: u32 = 1 << 24;
/// Hard cap on decoded `$ref` records (defends the decoder against a hostile blob).
pub const MAX_MODEL_REFS: u32 = 1 << 22;

/// Sentinel for an absent index (no parent / no name node / no member).
pub const NONE: u32 = u32::MAX;

/// Dialect: a JSON Schema document (`$schema` marker).
pub const D_JSON_SCHEMA: u8 = 0;
/// Dialect: an OpenAPI 3.x document (`openapi` marker).
pub const D_OPENAPI: u8 = 1;
/// Dialect: a Swagger 2.0 document (`swagger: "2.0"` marker).
pub const D_SWAGGER: u8 = 2;
/// Dialect: an AsyncAPI document (`asyncapi` marker).
pub const D_ASYNCAPI: u8 = 3;

/// The stable dialect name.
pub const fn dialect_name(dialect: u8) -> &'static str {
    match dialect {
        D_JSON_SCHEMA => "json_schema",
        D_OPENAPI => "openapi",
        D_SWAGGER => "swagger",
        D_ASYNCAPI => "asyncapi",
        _ => "unknown",
    }
}

/// The root marker key for a dialect.
pub const fn dialect_marker(dialect: u8) -> &'static str {
    match dialect {
        D_JSON_SCHEMA => "$schema",
        D_OPENAPI => "openapi",
        D_SWAGGER => "swagger",
        D_ASYNCAPI => "asyncapi",
        _ => "",
    }
}

/// Object role: the document root.
pub const ROLE_ROOT: u8 = 0;
/// Object role: an `info` object.
pub const ROLE_INFO: u8 = 1;
/// Object role: a `paths` object.
pub const ROLE_PATHS: u8 = 2;
/// Object role: a path item (a `paths` member value).
pub const ROLE_PATH_ITEM: u8 = 3;
/// Object role: an operation (`get`/`post`/…).
pub const ROLE_OPERATION: u8 = 4;
/// Object role: a `parameters` object / array.
pub const ROLE_PARAMETERS: u8 = 5;
/// Object role: a `responses` object.
pub const ROLE_RESPONSES: u8 = 6;
/// Object role: a `requestBody` object.
pub const ROLE_REQUEST_BODY: u8 = 7;
/// Object role: a `components` object.
pub const ROLE_COMPONENTS: u8 = 8;
/// Object role: a `schemas` object.
pub const ROLE_SCHEMAS: u8 = 9;
/// Object role: a `definitions` object.
pub const ROLE_DEFINITIONS: u8 = 10;
/// Object role: a `$defs` object.
pub const ROLE_DEFS: u8 = 11;
/// Object role: a `properties` object.
pub const ROLE_PROPERTIES: u8 = 12;
/// Object role: an `items` schema.
pub const ROLE_ITEMS: u8 = 13;
/// Object role: a `required` array.
pub const ROLE_REQUIRED: u8 = 14;
/// Object role: an `enum` array.
pub const ROLE_ENUM: u8 = 15;
/// Object role: an `allOf`/`oneOf`/`anyOf`/`prefixItems` array.
pub const ROLE_COMBINATOR: u8 = 16;
/// Object role: a `security` array.
pub const ROLE_SECURITY: u8 = 17;
/// Object role: a `securitySchemes` object.
pub const ROLE_SECURITY_SCHEMES: u8 = 18;
/// Object role: a `servers` array / object.
pub const ROLE_SERVERS: u8 = 19;
/// Object role: a `channels` object.
pub const ROLE_CHANNELS: u8 = 20;
/// Object role: an `operations` object.
pub const ROLE_OPERATIONS: u8 = 21;
/// Object role: a `tags` array.
pub const ROLE_TAGS: u8 = 22;
/// Object role: a channel item (a `channels` member value).
pub const ROLE_CHANNEL_ITEM: u8 = 23;
/// Object role: a `basePath` scalar.
pub const ROLE_BASE_PATH: u8 = 24;
/// Object role: any other object.
pub const ROLE_OTHER: u8 = 25;

/// The stable role name.
pub const fn role_name(role: u8) -> &'static str {
    match role {
        ROLE_ROOT => "root",
        ROLE_INFO => "info",
        ROLE_PATHS => "paths",
        ROLE_PATH_ITEM => "path-item",
        ROLE_OPERATION => "operation",
        ROLE_PARAMETERS => "parameters",
        ROLE_RESPONSES => "responses",
        ROLE_REQUEST_BODY => "request-body",
        ROLE_COMPONENTS => "components",
        ROLE_SCHEMAS => "schemas",
        ROLE_DEFINITIONS => "definitions",
        ROLE_DEFS => "$defs",
        ROLE_PROPERTIES => "properties",
        ROLE_ITEMS => "items",
        ROLE_REQUIRED => "required",
        ROLE_ENUM => "enum",
        ROLE_COMBINATOR => "combinator",
        ROLE_SECURITY => "security",
        ROLE_SECURITY_SCHEMES => "security-schemes",
        ROLE_SERVERS => "servers",
        ROLE_CHANNELS => "channels",
        ROLE_OPERATIONS => "operations",
        ROLE_TAGS => "tags",
        ROLE_CHANNEL_ITEM => "channel-item",
        ROLE_BASE_PATH => "base-path",
        _ => "other",
    }
}

/// The role tag for a member key, given the owning object's role. A path/channel item
/// is recognized by its **parent** (the key is an arbitrary path/channel name).
pub fn role_for_member(parent_role: u8, key: &str) -> u8 {
    if parent_role == ROLE_PATHS {
        return ROLE_PATH_ITEM;
    }
    if parent_role == ROLE_CHANNELS {
        return ROLE_CHANNEL_ITEM;
    }
    match key {
        "info" => ROLE_INFO,
        "paths" => ROLE_PATHS,
        "components" => ROLE_COMPONENTS,
        "schemas" => ROLE_SCHEMAS,
        "definitions" => ROLE_DEFINITIONS,
        "$defs" => ROLE_DEFS,
        "properties" => ROLE_PROPERTIES,
        "items" => ROLE_ITEMS,
        "required" => ROLE_REQUIRED,
        "enum" => ROLE_ENUM,
        "allOf" | "oneOf" | "anyOf" | "prefixItems" => ROLE_COMBINATOR,
        "parameters" => ROLE_PARAMETERS,
        "responses" => ROLE_RESPONSES,
        "requestBody" => ROLE_REQUEST_BODY,
        "security" => ROLE_SECURITY,
        "securitySchemes" => ROLE_SECURITY_SCHEMES,
        "servers" => ROLE_SERVERS,
        "channels" => ROLE_CHANNELS,
        "operations" => ROLE_OPERATIONS,
        "tags" => ROLE_TAGS,
        "basePath" => ROLE_BASE_PATH,
        "get" | "put" | "post" | "delete" | "options" | "head" | "patch" | "trace" => {
            ROLE_OPERATION
        }
        _ => ROLE_OTHER,
    }
}

/// The marker-key role tag (the spec-version key of a dialect).
pub fn is_marker_key(key: &str) -> bool {
    matches!(key, "$schema" | "openapi" | "swagger" | "asyncapi")
}

/// One recorded object with exact spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiObject {
    /// The parent object index ([`NONE`] for the root).
    pub parent: u32,
    /// The role tag (`ROLE_*`).
    pub role: u8,
    /// The node kind in the JSON arena's namespace ([`K_OBJECT`] / [`K_ARRAY`]).
    pub kind: u8,
    /// The naming key node (arena index), or [`NONE`] when unnamed.
    pub name_node: u32,
    /// The object node (arena index).
    pub node: u32,
    /// The first member index owned by this object.
    pub first_member: u32,
    /// The number of members owned by this object.
    pub member_count: u32,
    /// The naming key token's first source byte (`0` when unnamed).
    pub name_start: u64,
    /// One past the naming key token.
    pub name_end: u64,
    /// The object value's first source byte.
    pub start: u64,
    /// One past the object value.
    pub end: u64,
}

/// One recorded key/value member with exact spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiMember {
    /// The owning object index.
    pub object: u32,
    /// The key node (arena index).
    pub key_node: u32,
    /// The value node (arena index).
    pub value_node: u32,
    /// The value node's kind in the JSON arena's namespace.
    pub value_kind: u8,
    /// The key token's first source byte.
    pub key_start: u64,
    /// One past the key token.
    pub key_end: u64,
    /// The value's first source byte.
    pub value_start: u64,
    /// One past the value.
    pub value_end: u64,
}

/// One recorded `$ref` member whose value is a string (target preserved verbatim,
/// never resolved).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiRef {
    /// The owning object index.
    pub owner: u32,
    /// The `$ref` key node (arena index).
    pub key_node: u32,
    /// The target string node (arena index).
    pub value_node: u32,
    /// The target string token's first source byte.
    pub start: u64,
    /// One past the target string token.
    pub end: u64,
}

/// The canonical derived API-specification model (the materialization of an
/// `ApispecModel` node). It embeds the JSON [`JsonModel`] and stores the semantic
/// projection as indices into that arena, so every span is a `Q_gen` projection of the
/// JSON parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApispecModel {
    /// The recorded dialect (`D_*`).
    pub dialect: u8,
    /// The root node index in the embedded arena.
    pub root: u32,
    /// The spec-version string node (arena index).
    pub version_node: u32,
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// The recorded objects, in pre-order.
    pub objects: Vec<ApiObject>,
    /// The recorded members, grouped by object and in document order within an object.
    pub members: Vec<ApiMember>,
    /// The recorded `$ref` members, in document order.
    pub refs: Vec<ApiRef>,
    /// The embedded representation-preserving JSON arena.
    pub json: JsonModel,
}

impl ApispecModel {
    /// The recorded dialect name.
    pub fn dialect_name(&self) -> &'static str {
        dialect_name(self.dialect)
    }

    /// The JSON node at `index`, if present.
    pub fn node(&self, index: u32) -> Option<&JNode> {
        self.json.node(index)
    }

    /// The object at `index`, if present.
    pub fn object(&self, index: u32) -> Option<&ApiObject> {
        self.objects.get(index as usize)
    }

    /// The member at `index`, if present.
    pub fn member(&self, index: u32) -> Option<&ApiMember> {
        self.members.get(index as usize)
    }

    /// The `$ref` at `index`, if present.
    pub fn ref_(&self, index: u32) -> Option<&ApiRef> {
        self.refs.get(index as usize)
    }

    /// The exact source length the spans are relative to.
    pub fn doc_len(&self) -> u64 {
        self.json.doc_len
    }

    /// The number of arena nodes.
    pub fn arena_len(&self) -> u32 {
        self.json.nodes.len() as u32
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let json_bytes = self.json.encode();
        let mut out = Vec::with_capacity(
            44 + self.objects.len() * 60
                + self.members.len() * 48
                + self.refs.len() * 36
                + json_bytes.len(),
        );
        out.extend_from_slice(b"APSP");
        out.push(MODEL_VERSION);
        out.push(self.dialect);
        out.extend_from_slice(&[0, 0]); // reserved
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&self.root.to_le_bytes());
        out.extend_from_slice(&self.version_node.to_le_bytes());
        out.extend_from_slice(&(self.objects.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.members.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.refs.len() as u32).to_le_bytes());
        for o in &self.objects {
            out.extend_from_slice(&o.parent.to_le_bytes());
            out.push(o.role);
            out.push(o.kind);
            out.extend_from_slice(&[0, 0]); // reserved
            out.extend_from_slice(&o.name_node.to_le_bytes());
            out.extend_from_slice(&o.node.to_le_bytes());
            out.extend_from_slice(&o.first_member.to_le_bytes());
            out.extend_from_slice(&o.member_count.to_le_bytes());
            out.extend_from_slice(&o.name_start.to_le_bytes());
            out.extend_from_slice(&o.name_end.to_le_bytes());
            out.extend_from_slice(&o.start.to_le_bytes());
            out.extend_from_slice(&o.end.to_le_bytes());
        }
        for m in &self.members {
            out.extend_from_slice(&m.object.to_le_bytes());
            out.extend_from_slice(&m.key_node.to_le_bytes());
            out.extend_from_slice(&m.value_node.to_le_bytes());
            out.push(m.value_kind);
            out.extend_from_slice(&[0, 0, 0]); // reserved
            out.extend_from_slice(&m.key_start.to_le_bytes());
            out.extend_from_slice(&m.key_end.to_le_bytes());
            out.extend_from_slice(&m.value_start.to_le_bytes());
            out.extend_from_slice(&m.value_end.to_le_bytes());
        }
        for r in &self.refs {
            out.extend_from_slice(&r.owner.to_le_bytes());
            out.extend_from_slice(&r.key_node.to_le_bytes());
            out.extend_from_slice(&r.value_node.to_le_bytes());
            out.extend_from_slice(&[0, 0, 0, 0]); // reserved
            out.extend_from_slice(&r.start.to_le_bytes());
            out.extend_from_slice(&r.end.to_le_bytes());
        }
        out.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&json_bytes);
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<ApispecModel> {
        let mut r = Reader::new(bytes);
        if r.bytes(4)? != b"APSP" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let dialect = r.u8()?;
        if dialect > D_ASYNCAPI {
            return Err(corrupt("unknown dialect"));
        }
        if r.bytes(2)? != [0, 0] {
            return Err(corrupt("unknown model flags"));
        }
        let doc_len = r.u64()?;
        let root = r.u32()?;
        let version_node = r.u32()?;
        let object_count = r.u32()?;
        let member_count = r.u32()?;
        let ref_count = r.u32()?;
        if object_count > MAX_MODEL_OBJECTS {
            return Err(corrupt("object count is implausible"));
        }
        if member_count > MAX_MODEL_MEMBERS {
            return Err(corrupt("member count is implausible"));
        }
        if ref_count > MAX_MODEL_REFS {
            return Err(corrupt("ref count is implausible"));
        }
        let mut objects = Vec::with_capacity(object_count as usize);
        for _ in 0..object_count {
            let parent = r.u32()?;
            let role = r.u8()?;
            let kind = r.u8()?;
            if r.bytes(2)? != [0, 0] {
                return Err(corrupt("unknown object flags"));
            }
            let name_node = r.u32()?;
            let node = r.u32()?;
            let first_member = r.u32()?;
            let member_count = r.u32()?;
            let name_start = r.u64()?;
            let name_end = r.u64()?;
            let start = r.u64()?;
            let end = r.u64()?;
            if role > ROLE_OTHER || kind > K_ARRAY {
                return Err(corrupt("unknown object tag"));
            }
            for (lo, hi) in [(name_start, name_end), (start, end)] {
                if lo > hi || hi > doc_len {
                    return Err(corrupt("object span is outside the document"));
                }
            }
            objects.push(ApiObject {
                parent,
                role,
                kind,
                name_node,
                node,
                first_member,
                member_count,
                name_start,
                name_end,
                start,
                end,
            });
        }
        let mut members = Vec::with_capacity(member_count as usize);
        for _ in 0..member_count {
            let object = r.u32()?;
            let key_node = r.u32()?;
            let value_node = r.u32()?;
            let value_kind = r.u8()?;
            if r.bytes(3)? != [0, 0, 0] {
                return Err(corrupt("unknown member flags"));
            }
            let key_start = r.u64()?;
            let key_end = r.u64()?;
            let value_start = r.u64()?;
            let value_end = r.u64()?;
            if value_kind > crate::adapter::json::K_NULL {
                return Err(corrupt("unknown member value kind"));
            }
            for (lo, hi) in [(key_start, key_end), (value_start, value_end)] {
                if lo > hi || hi > doc_len {
                    return Err(corrupt("member span is outside the document"));
                }
            }
            members.push(ApiMember {
                object,
                key_node,
                value_node,
                value_kind,
                key_start,
                key_end,
                value_start,
                value_end,
            });
        }
        let mut refs = Vec::with_capacity(ref_count as usize);
        for _ in 0..ref_count {
            let owner = r.u32()?;
            let key_node = r.u32()?;
            let value_node = r.u32()?;
            if r.bytes(4)? != [0, 0, 0, 0] {
                return Err(corrupt("unknown ref flags"));
            }
            let start = r.u64()?;
            let end = r.u64()?;
            if start > end || end > doc_len {
                return Err(corrupt("ref span is outside the document"));
            }
            refs.push(ApiRef {
                owner,
                key_node,
                value_node,
                start,
                end,
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
        if nnodes == 0 {
            return Err(corrupt("model has no arena"));
        }
        if root >= nnodes || version_node >= nnodes {
            return Err(corrupt("root/version node is out of range"));
        }
        if objects.is_empty() {
            return Err(corrupt("model has no root object"));
        }
        if objects[0].parent != NONE || objects[0].node != root {
            return Err(corrupt("model root object does not match the root node"));
        }
        for o in &objects {
            if o.node >= nnodes {
                return Err(corrupt("object node is out of range"));
            }
            if o.name_node != NONE && o.name_node >= nnodes {
                return Err(corrupt("object name node is out of range"));
            }
            if o.parent != NONE && o.parent as usize >= objects.len() {
                return Err(corrupt("object parent is out of range"));
            }
            let end = o.first_member as u64 + o.member_count as u64;
            if end > members.len() as u64 {
                return Err(corrupt("object member range is out of range"));
            }
        }
        for m in &members {
            if m.object as usize >= objects.len() || m.key_node >= nnodes || m.value_node >= nnodes
            {
                return Err(corrupt("member index is out of range"));
            }
        }
        for rf in &refs {
            if rf.owner as usize >= objects.len()
                || rf.key_node >= nnodes
                || rf.value_node >= nnodes
            {
                return Err(corrupt("ref index is out of range"));
            }
        }
        Ok(ApispecModel {
            dialect,
            root,
            version_node,
            doc_len,
            objects,
            members,
            refs,
            json,
        })
    }
}

/// A lexical match from [`find`]: a record over the shared JSON match vocabulary
/// (canonical RFC 6901 pointer, key/value role, exact span, decoded text).
pub type ApispecMatch = JsonMatch;

/// Byte-based, conservative API-spec detector. See the module docs for the exact
/// semantic boundary.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.len() as u64 > limits.max_apispec_document_bytes {
        return false;
    }
    // Reuse the JSON detector first: an API spec is a JSON document.
    if !json::detect(source, limits) {
        return false;
    }
    parse(source, limits, false).is_ok()
}

/// Classify `source` as one API-spec dialect, or decline typed when it is not a
/// recognized document under the semantic test.
pub fn classify(source: &[u8], limits: Limits) -> Result<u8> {
    Ok(parse(source, limits, false)?.dialect)
}

/// Parse `source` into an [`ApispecModel`]. `build` selects whether the semantic
/// projections (objects/members/refs) are retained (detection runs with `build =
/// false`); the JSON arena is always built because the semantic test walks it. The
/// shared JSON parser enforces the JSON caps; this function adds the API-spec caps on
/// top.
pub fn parse(source: &[u8], limits: Limits, build: bool) -> Result<ApispecModel> {
    if source.len() as u64 > limits.max_apispec_document_bytes {
        return Err(Error::resource_limit(format!(
            "API-spec source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_apispec_document_bytes
        )));
    }
    // Reuse the shared JSON parser: an API spec must be exactly one JSON value. A
    // malformed source becomes a typed API-spec decline (resource errors are kept as
    // resource errors so caps are never laundered into structure errors).
    let json = json::parse(source, limits, true).map_err(|e| match e.class() {
        ErrorClass::ResourceLimit => e,
        _ => invalid_apispec(format!(
            "API-spec source is not a single JSON value: {}",
            e.message()
        )),
    })?;
    if json.nodes.len() as u64 > limits.max_apispec_nodes as u64 {
        return Err(Error::resource_limit(format!(
            "API-spec document exceeds the {}-node cap",
            limits.max_apispec_nodes
        )));
    }
    let (dialect, version_node) = classify_root(&json, source)?;
    let mut builder = Builder {
        json: &json,
        source,
        limits,
        objects: Vec::new(),
        members: Vec::new(),
        refs: Vec::new(),
    };
    builder.collect(json.root, NONE, NONE, 0, 0, ROLE_ROOT, 1)?;
    let Builder {
        objects,
        members,
        refs,
        ..
    } = builder;
    let (objects, members, refs) = if build {
        (objects, members, refs)
    } else {
        (Vec::new(), Vec::new(), Vec::new())
    };
    Ok(ApispecModel {
        dialect,
        root: json.root,
        version_node,
        doc_len: json.doc_len,
        objects,
        members,
        refs,
        json,
    })
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of an `ApispecModel` node).
pub fn build_apispec_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits, true)?.encode())
}

// ---------------------------------------------------------------------------
// Detection helper
// ---------------------------------------------------------------------------

/// Classify the root object and locate the spec-version string. Requires an object
/// root carrying a strong, root-level marker.
fn classify_root(json: &JsonModel, source: &[u8]) -> Result<(u8, u32)> {
    let root = json.root;
    let rn = json
        .node(root)
        .ok_or_else(|| corrupt("root index is out of range"))?;
    if rn.kind != K_OBJECT {
        return Err(invalid_apispec("an API-spec root must be a JSON object"));
    }
    let mut schema: Option<u32> = None;
    let mut openapi: Option<u32> = None;
    let mut swagger: Option<u32> = None;
    let mut asyncapi: Option<u32> = None;
    let mut i = 0usize;
    while i + 1 < rn.children.len() {
        let k = rn.children[i];
        let v = rn.children[i + 1];
        i += 2;
        let kn = json
            .node(k)
            .ok_or_else(|| corrupt("member key index is out of range"))?;
        if kn.kind != K_STRING {
            continue;
        }
        let key = json::decode_string(source, kn)?;
        let vn = json
            .node(v)
            .ok_or_else(|| corrupt("member value index is out of range"))?;
        if vn.kind != K_STRING {
            continue;
        }
        match key.as_str() {
            "$schema" if schema.is_none() => schema = Some(v),
            "openapi" if openapi.is_none() => openapi = Some(v),
            "swagger" if swagger.is_none() => swagger = Some(v),
            "asyncapi" if asyncapi.is_none() => asyncapi = Some(v),
            _ => {}
        }
    }
    if let Some(v) = swagger {
        let n = json
            .node(v)
            .ok_or_else(|| corrupt("swagger value index is out of range"))?;
        if json::decode_string(source, n)? == "2.0" {
            return Ok((D_SWAGGER, v));
        }
    }
    if let Some(v) = openapi {
        let n = json
            .node(v)
            .ok_or_else(|| corrupt("openapi value index is out of range"))?;
        if json::decode_string(source, n)?.starts_with("3.") {
            return Ok((D_OPENAPI, v));
        }
    }
    if let Some(v) = asyncapi {
        let n = json
            .node(v)
            .ok_or_else(|| corrupt("asyncapi value index is out of range"))?;
        if !json::decode_string(source, n)?.is_empty() {
            return Ok((D_ASYNCAPI, v));
        }
    }
    if let Some(v) = schema {
        let n = json
            .node(v)
            .ok_or_else(|| corrupt("$schema value index is out of range"))?;
        if json::decode_string(source, n)?.contains("json-schema.org") {
            return Ok((D_JSON_SCHEMA, v));
        }
    }
    Err(invalid_apispec(
        "root object carries no strong API-spec marker ($schema/openapi/swagger/asyncapi)",
    ))
}

// ---------------------------------------------------------------------------
// Object / member / ref construction
// ---------------------------------------------------------------------------

struct Builder<'a> {
    json: &'a JsonModel,
    source: &'a [u8],
    limits: Limits,
    objects: Vec<ApiObject>,
    members: Vec<ApiMember>,
    refs: Vec<ApiRef>,
}

impl Builder<'_> {
    /// Record the object node `node_index` (a JSON object) and recurse into its member
    /// values. `name_node`/`role` describe how the object was reached.
    #[allow(clippy::too_many_arguments)]
    fn collect(
        &mut self,
        node_index: u32,
        parent: u32,
        name_node: u32,
        name_start: u64,
        name_end: u64,
        role: u8,
        depth: u32,
    ) -> Result<u32> {
        if depth > self.limits.max_apispec_depth {
            return Err(Error::resource_limit(format!(
                "API-spec nesting exceeds the {}-level cap",
                self.limits.max_apispec_depth
            )));
        }
        if self.objects.len() as u64 >= self.limits.max_apispec_objects as u64 {
            return Err(Error::resource_limit(format!(
                "API-spec document exceeds the {}-object cap",
                self.limits.max_apispec_objects
            )));
        }
        let node = self
            .json
            .node(node_index)
            .ok_or_else(|| corrupt("object node index is out of range"))?;
        if node.kind != K_OBJECT {
            return Err(corrupt("recorded object node is not a JSON object"));
        }
        let start = node.start;
        let end = node.end;
        let children = node.children.clone();
        let index = self.objects.len() as u32;
        self.objects.push(ApiObject {
            parent,
            role,
            kind: K_OBJECT,
            name_node,
            node: node_index,
            first_member: self.members.len() as u32,
            member_count: 0,
            name_start,
            name_end,
            start,
            end,
        });
        let mut count = 0u32;
        let mut i = 0usize;
        while i + 1 < children.len() {
            let k = children[i];
            let v = children[i + 1];
            i += 2;
            if self.members.len() as u64 >= self.limits.max_apispec_members as u64 {
                return Err(Error::resource_limit(format!(
                    "API-spec document exceeds the {}-member cap",
                    self.limits.max_apispec_members
                )));
            }
            let kn = self
                .json
                .node(k)
                .ok_or_else(|| corrupt("member key index is out of range"))?;
            let vn = self
                .json
                .node(v)
                .ok_or_else(|| corrupt("member value index is out of range"))?;
            let key = if kn.kind == K_STRING {
                json::decode_string(self.source, kn)?
            } else {
                String::new()
            };
            self.members.push(ApiMember {
                object: index,
                key_node: k,
                value_node: v,
                value_kind: vn.kind,
                key_start: kn.start,
                key_end: kn.end,
                value_start: vn.start,
                value_end: vn.end,
            });
            count += 1;
            if key == "$ref" && vn.kind == K_STRING {
                if self.refs.len() as u64 >= self.limits.max_apispec_refs as u64 {
                    return Err(Error::resource_limit(format!(
                        "API-spec document exceeds the {}-ref cap",
                        self.limits.max_apispec_refs
                    )));
                }
                self.refs.push(ApiRef {
                    owner: index,
                    key_node: k,
                    value_node: v,
                    start: vn.start,
                    end: vn.end,
                });
            }
            if vn.kind == K_OBJECT {
                let child_role = role_for_member(role, &key);
                self.collect(v, index, k, kn.start, kn.end, child_role, depth + 1)?;
            } else if vn.kind == K_ARRAY {
                let child_role = role_for_member(role, &key);
                self.collect_array(v, index, child_role, depth + 1)?;
            }
        }
        self.objects[index as usize].member_count = count;
        Ok(index)
    }

    /// Recurse through an array, recording every object element. Array elements inherit
    /// the owning member's role (an object inside `allOf`/`parameters`/…).
    fn collect_array(&mut self, node_index: u32, parent: u32, role: u8, depth: u32) -> Result<()> {
        if depth > self.limits.max_apispec_depth {
            return Err(Error::resource_limit(format!(
                "API-spec nesting exceeds the {}-level cap",
                self.limits.max_apispec_depth
            )));
        }
        let node = self
            .json
            .node(node_index)
            .ok_or_else(|| corrupt("array node index is out of range"))?;
        if node.kind != K_ARRAY {
            return Err(corrupt("expected an array node"));
        }
        let children = node.children.clone();
        for el in children {
            let en = self
                .json
                .node(el)
                .ok_or_else(|| corrupt("array element index is out of range"))?;
            match en.kind {
                K_OBJECT => {
                    self.collect(el, parent, NONE, 0, 0, role, depth + 1)?;
                }
                K_ARRAY => {
                    self.collect_array(el, parent, role, depth + 1)?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Semantic projections over the embedded JSON model
// ---------------------------------------------------------------------------

/// The value node of the **first** object member named `key` (duplicate keys are
/// preserved, but a reader resolves the first, exactly like the JSON adapter).
pub fn object_member(
    model: &ApispecModel,
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
pub fn object_members(model: &ApispecModel, source: &[u8], obj: u32) -> Result<Vec<(String, u32)>> {
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

/// The exact spec-version token bytes (the marker value, e.g. `"3.1.0"`).
pub fn version_token<'a>(model: &ApispecModel, source: &'a [u8]) -> Result<&'a [u8]> {
    let n = model
        .node(model.version_node)
        .ok_or_else(|| corrupt("version node index is out of range"))?;
    json::token_bytes(source, n)
}

/// The decoded spec-version string (never normalized).
pub fn version_text(model: &ApispecModel, source: &[u8]) -> Result<String> {
    let n = model
        .node(model.version_node)
        .ok_or_else(|| corrupt("version node index is out of range"))?;
    json::decode_string(source, n)
}

/// The exact token bytes of an object's value span.
pub fn object_bytes<'a>(
    model: &ApispecModel,
    source: &'a [u8],
    object: &ApiObject,
) -> Result<&'a [u8]> {
    let _ = model;
    slice(source, object.start, object.end)
}

/// The decoded key text of a member.
pub fn member_key_text(model: &ApispecModel, source: &[u8], member: &ApiMember) -> Result<String> {
    let n = model
        .node(member.key_node)
        .ok_or_else(|| corrupt("member key node is out of range"))?;
    json::decode_string(source, n)
}

/// The exact value bytes of a member.
pub fn member_value_bytes<'a>(
    model: &ApispecModel,
    source: &'a [u8],
    member: &ApiMember,
) -> Result<&'a [u8]> {
    let _ = model;
    slice(source, member.value_start, member.value_end)
}

/// The decoded text of a member value: the decoded string for a string scalar, else
/// the exact token bytes rendered lossily.
pub fn member_value_text(
    model: &ApispecModel,
    source: &[u8],
    member: &ApiMember,
) -> Result<String> {
    if member.value_kind == K_STRING {
        let n = model
            .node(member.value_node)
            .ok_or_else(|| corrupt("member value node is out of range"))?;
        json::decode_string(source, n)
    } else {
        Ok(String::from_utf8_lossy(member_value_bytes(model, source, member)?).into_owned())
    }
}

/// The stable kind name of a member value in the JSON arena's namespace.
pub fn member_value_kind_name(member: &ApiMember) -> &'static str {
    json::kind_name(member.value_kind)
}

/// The decoded `$ref` target of a recorded reference (never resolved).
pub fn ref_target(model: &ApispecModel, source: &[u8], r: &ApiRef) -> Result<String> {
    let n = model
        .node(r.value_node)
        .ok_or_else(|| corrupt("ref value node is out of range"))?;
    json::decode_string(source, n)
}

/// A bounded, case-sensitive lexical search over object keys and string values,
/// reusing the shared JSON [`json::find`] over the embedded arena. Matches are
/// returned in document order with their canonical pointers and exact spans.
pub fn find(
    model: &ApispecModel,
    source: &[u8],
    pattern: &str,
    limits: Limits,
) -> Result<Vec<ApispecMatch>> {
    json::find(&model.json, source, pattern, limits)
}

/// A deterministic canonical text projection of the whole document (the shared JSON
/// canonical text: member order and token spelling preserved).
pub fn canonical_text(model: &ApispecModel, source: &[u8]) -> Result<String> {
    json::canonical_text(&model.json, source)
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn corrupt(msg: &str) -> Error {
    Error::invalid_model(format!("corrupt API-spec model: {msg}"))
}

fn invalid_apispec(msg: impl Into<String>) -> Error {
    Error::invalid_apispec_structure(msg)
}

fn slice(source: &[u8], start: u64, end: u64) -> Result<&[u8]> {
    let s = usize::try_from(start).map_err(|_| corrupt("span overflow"))?;
    let e = usize::try_from(end).map_err(|_| corrupt("span overflow"))?;
    source
        .get(s..e)
        .ok_or_else(|| corrupt("span is outside the source"))
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(b: &'a [u8]) -> Reader<'a> {
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
            .ok_or_else(|| corrupt("unexpected end of model"))?;
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEMA: &[u8] = br##"{
      "$schema": "https://json-schema.org/draft/2020-12/schema",
      "title": "Person",
      "type": "object",
      "properties": {
        "name": { "type": "string" },
        "age": { "type": "integer", "minimum": 0 }
      },
      "required": ["name"],
      "enum": ["a", "b"],
      "$defs": { "Id": { "type": "string", "format": "uuid" } },
      "definitions": { "Legacy": { "$ref": "#/$defs/Id" } },
      "allOf": [{ "type": "object" }]
    }"##;

    const OPENAPI: &[u8] = br##"{
      "openapi": "3.1.0",
      "info": { "title": "Demo", "version": "1.0.0" },
      "servers": [{ "url": "https://api.example.com" }],
      "paths": {
        "/pets": {
          "get": {
            "operationId": "listPets",
            "parameters": [{ "name": "limit", "in": "query" }],
            "responses": { "200": { "description": "ok" } }
          }
        }
      },
      "components": { "schemas": { "Pet": { "$ref": "#/components/schemas/Pet" } } },
      "tags": [{ "name": "pets" }],
      "security": [{ "apiKey": [] }]
    }"##;

    const SWAGGER: &[u8] = br#"{
      "swagger": "2.0",
      "info": { "title": "Demo", "version": "1.0.0" },
      "basePath": "/v1",
      "paths": { "/pets": { "get": { "responses": { "200": { "description": "ok" } } } } },
      "definitions": { "Pet": { "type": "object" } },
      "parameters": { "Limit": { "name": "limit", "in": "query" } },
      "responses": { "NotFound": { "description": "nope" } }
    }"#;

    const ASYNCAPI: &[u8] = br##"{
      "asyncapi": "2.6.0",
      "info": { "title": "Demo", "version": "1.0.0" },
      "channels": { "user/signedup": { "subscribe": { "message": { "$ref": "#/components/messages/User" } } } },
      "operations": { "sendUser": { "action": "send" } },
      "components": { "messages": { "User": { "name": "User" } } },
      "servers": { "prod": { "url": "broker.example.com" } }
    }"##;

    #[test]
    fn detects_each_dialect_and_rejects_generic() {
        assert_eq!(
            parse(SCHEMA, Limits::DEFAULT, false).unwrap().dialect,
            D_JSON_SCHEMA
        );
        assert_eq!(
            parse(OPENAPI, Limits::DEFAULT, false).unwrap().dialect,
            D_OPENAPI
        );
        assert_eq!(
            parse(SWAGGER, Limits::DEFAULT, false).unwrap().dialect,
            D_SWAGGER
        );
        assert_eq!(
            parse(ASYNCAPI, Limits::DEFAULT, false).unwrap().dialect,
            D_ASYNCAPI
        );
        // A JSON-Schema *shape* with no `$schema` is an honest ambiguity -> not claimed.
        assert!(!detect(
            br#"{"type":"object","properties":{"a":{"type":"string"}}}"#,
            Limits::DEFAULT
        ));
        assert!(!detect(br#"{"properties":{},"paths":{}}"#, Limits::DEFAULT));
        // An unrelated `openapi` value or `swagger` version is not claimed.
        assert!(!detect(br#"{"openapi":"2.0","paths":{}}"#, Limits::DEFAULT));
        assert!(!detect(br#"{"swagger":"1.2","paths":{}}"#, Limits::DEFAULT));
        // Plain JSON and prose are not claimed.
        assert!(!detect(br#"{"a":1}"#, Limits::DEFAULT));
        assert!(!detect(b"just some prose\n", Limits::DEFAULT));
    }

    #[test]
    fn preserves_spans_order_duplicates_refs_and_version() {
        let m = parse(SCHEMA, Limits::DEFAULT, true).unwrap();
        assert_eq!(m.dialect_name(), "json_schema");
        // The exact spec-version token is preserved verbatim.
        assert_eq!(
            version_token(&m, SCHEMA).unwrap(),
            br#""https://json-schema.org/draft/2020-12/schema""#
        );
        // Keyword spelling preserved: `required`/`$defs`/`definitions` are members.
        let keys: Vec<String> = object_members(&m, SCHEMA, m.root)
            .unwrap()
            .into_iter()
            .map(|(k, _)| k)
            .collect();
        assert!(keys.contains(&"$defs".to_string()));
        assert!(keys.contains(&"required".to_string()));
        // The `$ref` target is preserved verbatim (never resolved).
        assert_eq!(m.refs.len(), 1);
        assert_eq!(ref_target(&m, SCHEMA, &m.refs[0]).unwrap(), "#/$defs/Id");
        // A duplicate key is preserved.
        let dup =
            br#"{"$schema":"https://json-schema.org/draft-07/schema#","title":"a","title":"b"}"#;
        let m = parse(dup, Limits::DEFAULT, true).unwrap();
        let titles: Vec<&str> = m
            .members
            .iter()
            .filter(|mm| member_key_text(&m, dup, mm).unwrap() == "title")
            .map(|_| "title")
            .collect();
        assert_eq!(titles.len(), 2);
    }

    #[test]
    fn openapi_paths_operations_components_are_recorded() {
        let m = parse(OPENAPI, Limits::DEFAULT, true).unwrap();
        assert_eq!(m.dialect_name(), "openapi");
        assert_eq!(version_text(&m, OPENAPI).unwrap(), "3.1.0");
        // A path item and an operation are recorded with the right roles.
        assert!(m.objects.iter().any(|o| role_name(o.role) == "path-item"));
        assert!(m.objects.iter().any(|o| role_name(o.role) == "operation"));
        assert!(m.objects.iter().any(|o| role_name(o.role) == "components"));
        assert!(m.objects.iter().any(|o| role_name(o.role) == "schemas"));
        assert!(m.objects.iter().any(|o| role_name(o.role) == "servers"));
        assert!(m.objects.iter().any(|o| role_name(o.role) == "security"));
        assert_eq!(m.refs.len(), 1);
    }

    #[test]
    fn swagger_and_asyncapi_dialects_are_recorded() {
        let m = parse(SWAGGER, Limits::DEFAULT, true).unwrap();
        assert_eq!(m.dialect_name(), "swagger");
        assert_eq!(version_text(&m, SWAGGER).unwrap(), "2.0");
        assert!(m.objects.iter().any(|o| role_name(o.role) == "definitions"));
        assert!(m.objects.iter().any(|o| role_name(o.role) == "responses"));

        let m = parse(ASYNCAPI, Limits::DEFAULT, true).unwrap();
        assert_eq!(m.dialect_name(), "asyncapi");
        assert!(m.objects.iter().any(|o| role_name(o.role) == "channels"));
        assert!(
            m.objects
                .iter()
                .any(|o| role_name(o.role) == "channel-item")
        );
        assert!(m.objects.iter().any(|o| role_name(o.role) == "operations"));
        assert_eq!(m.refs.len(), 1);
    }

    #[test]
    fn roundtrips_and_rejects_corruption() {
        let m = parse(OPENAPI, Limits::DEFAULT, true).unwrap();
        let bytes = m.encode();
        assert_eq!(ApispecModel::decode(&bytes).unwrap(), m);
        for cut in 0..bytes.len() {
            let _ = ApispecModel::decode(&bytes[..cut]);
        }
        let mut bad = bytes.clone();
        bad[0] ^= 0xFF;
        assert!(ApispecModel::decode(&bad).is_err());
    }

    #[test]
    fn malformed_and_non_spec_decline_typed() {
        for src in [
            &b"{\"a\":1}"[..],
            &b"{\"type\":\"object\"}"[..],
            &b"[]"[..],
            &b"{"[..],
        ] {
            assert!(!detect(src, Limits::DEFAULT), "src={src:?}");
            let e = parse(src, Limits::DEFAULT, true).unwrap_err();
            assert_eq!(
                e.class(),
                ErrorClass::InvalidApispecStructure,
                "src={src:?}"
            );
        }
    }

    #[test]
    fn caps_decline_typed() {
        let tight = Limits {
            max_apispec_objects: 1,
            ..Limits::DEFAULT
        };
        assert!(!detect(OPENAPI, tight));
        assert_eq!(
            parse(OPENAPI, tight, true).unwrap_err().class(),
            ErrorClass::ResourceLimit
        );
        let tight = Limits {
            max_apispec_members: 1,
            ..Limits::DEFAULT
        };
        assert_eq!(
            parse(OPENAPI, tight, true).unwrap_err().class(),
            ErrorClass::ResourceLimit
        );
        let tight = Limits {
            max_apispec_depth: 1,
            ..Limits::DEFAULT
        };
        assert_eq!(
            parse(OPENAPI, tight, true).unwrap_err().class(),
            ErrorClass::ResourceLimit
        );
        let tight = Limits {
            max_apispec_nodes: 1,
            ..Limits::DEFAULT
        };
        assert_eq!(
            parse(OPENAPI, tight, true).unwrap_err().class(),
            ErrorClass::ResourceLimit
        );
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        for _ in 0..128 {
            let mut buf = vec![0u8; 512];
            for b in buf.iter_mut() {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                *b = (x & 0xFF) as u8;
            }
            let _ = detect(&buf, Limits::STRICT);
            if let Ok(m) = parse(&buf, Limits::STRICT, true) {
                let _ = canonical_text(&m, &buf);
                let _ = find(&m, &buf, "a", Limits::STRICT);
                let _ = version_text(&m, &buf);
            }
            let _ = build_apispec_model(&buf, Limits::STRICT);
            let _ = ApispecModel::decode(&buf);
        }
    }
}
