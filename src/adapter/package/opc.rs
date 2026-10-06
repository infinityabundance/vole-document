//! Generic OPC (Open Packaging Conventions, ISO/IEC 29500-2) core (Phase 12.3).
//!
//! This module is **format-neutral**: it models the abstract OPC package (content
//! types, parts, package/part relationships) with no WordprocessingML or EPUB
//! assumptions. A DOCX adapter (12.4) discovers its main part through the
//! relationship type it supplies; nothing here hardcodes `/word/document.xml`.
//!
//! Everything produced here is **derived (`Q_gen`) state**. Exact authority stays
//! with the Phase-12.2 ZIP member raw spans; this module never owns exact bytes and
//! never feeds an exact node (ADR-0024/0026). XML parsing is hardened per
//! research E §2 / I §2–§3: `quick-xml` with `default-features = false` (UTF-8
//! only, no `encoding_rs`), a hard refusal of `<!DOCTYPE` (no DTD, no external/internal
//! entity expansion, so no XXE/billion-laughs), and explicit depth/event/node/
//! attribute/text/size bounds.
//!
//! ## Inert external relationships
//!
//! `TargetMode="External"` targets — and any absolute URI target — are **inert
//! identifiers**. They are stored as strings and never resolved to a part, a
//! filesystem path, or a network location. `Relationship::is_external` is the only
//! predicate a consumer needs, and `OpcModel::resolved_part` refuses to follow one.

use std::collections::BTreeMap;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::error::{Error, Result};
use crate::limits::Limits;

use super::xml::XmlState;
use super::xml::{attr_of, doctype_declined, harden_xml, read_attrs, xml_err};

/// The canonical (reserved) OPC content-types part name.
pub const CONTENT_TYPES_PART: &str = "/[Content_Types].xml";
/// The canonical package-relationships part name.
pub const PACKAGE_RELS_PART: &str = "/_rels/.rels";
/// The transitional OPC content-types namespace.
pub const CONTENT_TYPES_NS: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
/// The transitional OPC relationships namespace.
pub const RELATIONSHIPS_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
/// The transitional `officeDocument` relationship type (DOCX main part).
pub const OFFICE_DOCUMENT_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
/// The strict `officeDocument` relationship type.
pub const OFFICE_DOCUMENT_REL_STRICT: &str =
    "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument";

/// Which universe a relationship target lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetMode {
    /// A part inside the package (resolvable to an absolute part name).
    Internal,
    /// A URI outside the package: an inert identifier, never fetched.
    External,
}

impl TargetMode {
    /// Stable lower-case name.
    pub const fn name(self) -> &'static str {
        match self {
            TargetMode::Internal => "internal",
            TargetMode::External => "external",
        }
    }
}

/// One OPC relationship (the resolved form).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relationship {
    /// The `Id` (unique within its `.rels` part at most; never a stable key).
    pub id: String,
    /// The opaque absolute relationship `Type` URI.
    pub rel_type: String,
    /// The `Target` exactly as written in the source.
    pub target: String,
    /// The resolved target mode (declared `TargetMode`, or inferred for absolute URIs).
    pub mode: TargetMode,
    /// For internal targets, the absolute part name the target resolves to. For
    /// external targets this is always `None` — the identifier is inert.
    pub resolved: Option<String>,
}

impl Relationship {
    /// Whether this relationship is inert (external or otherwise not a package part).
    pub fn is_external(&self) -> bool {
        self.mode == TargetMode::External || self.resolved.is_none()
    }
}

/// The `[Content_Types].xml` content-type table.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContentTypes {
    /// `Extension` (lower-cased) → content type, from `Default` elements.
    pub defaults: BTreeMap<String, String>,
    /// Canonical part name (lower-cased) → content type, from `Override` elements.
    pub overrides: BTreeMap<String, String>,
}

impl ContentTypes {
    /// The content type for a part name: an `Override` wins over a `Default`, and
    /// `Default` matches by extension (case-insensitively). A part with neither has
    /// no content type.
    pub fn content_type_for(&self, part_name: &str) -> Option<&str> {
        if let Some(ct) = self.overrides.get(&part_name.to_ascii_lowercase()) {
            return Some(ct.as_str());
        }
        let ext = extension_of(part_name)?;
        self.defaults
            .get(&ext.to_ascii_lowercase())
            .map(String::as_str)
    }
}

/// One OPC part, as a member of the package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpcPart {
    /// The physical member ordinal (identity; stable only for these exact bytes).
    pub ordinal: u32,
    /// The absolute (`/`-rooted) canonical part name.
    pub name: String,
    /// The content type, or `None` when no `Default`/`Override` matches.
    pub content_type: Option<String>,
}

/// The generic OPC package graph: content types + parts + relationships.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpcModel {
    /// The parsed content-type table.
    pub content_types: ContentTypes,
    /// Parts, sorted by case-insensitive part name.
    pub parts: Vec<OpcPart>,
    /// Package-level (`_rels/.rels`) relationships, sorted by id.
    pub package_rels: Vec<Relationship>,
    /// Part-level relationships: `(owner member ordinal, relationships)`, sorted by owner.
    pub part_rels: Vec<(u32, Vec<Relationship>)>,
    /// Non-fatal structural observations (dangling targets, undecodable metadata).
    pub issues: Vec<String>,
}

impl OpcModel {
    /// Look up a part by absolute part name (OPC part-name equivalence is
    /// case-insensitive).
    pub fn part_by_name(&self, name: &str) -> Option<&OpcPart> {
        self.parts
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// Resolve a relationship id. Ids are only unique within one `.rels` part, so a
    /// duplicated id across owners is ambiguous and is a typed decline.
    pub fn relationship_by_id(&self, id: &str) -> Result<Option<(&Relationship, Option<u32>)>> {
        let mut found: Option<(&Relationship, Option<u32>)> = None;
        for r in &self.package_rels {
            if r.id == id {
                if found.is_some() {
                    return Err(Error::invalid_package_structure(format!(
                        "relationship id {id:?} is ambiguous"
                    )));
                }
                found = Some((r, None));
            }
        }
        for (owner, rels) in &self.part_rels {
            for r in rels {
                if r.id == id {
                    if found.is_some() {
                        return Err(Error::invalid_package_structure(format!(
                            "relationship id {id:?} is ambiguous"
                        )));
                    }
                    found = Some((r, Some(*owner)));
                }
            }
        }
        Ok(found)
    }

    /// Discover the part(s) targeted by relationships of `rel_type`, searching the
    /// package relationships and every part's relationships. Fails closed:
    ///
    /// * no match → `InvalidPackageStructure` (missing);
    /// * an external/unresolvable target → `InvalidPackageStructure` (never fetched);
    /// * a target that is not a part → `InvalidPackageStructure`;
    /// * more than one distinct target part → `InvalidPackageStructure` (ambiguous).
    pub fn parts_by_relationship_type(&self, rel_type: &str) -> Result<Vec<&OpcPart>> {
        let mut matched = 0u32;
        let mut parts: Vec<&OpcPart> = Vec::new();
        let mut consider = |r: &Relationship, matched: &mut u32| -> Result<()> {
            if r.rel_type != rel_type {
                return Ok(());
            }
            *matched += 1;
            let resolved = r.resolved.as_deref().ok_or_else(|| {
                Error::invalid_package_structure(format!(
                    "relationship {:?} of the requested type has an external/unresolved target",
                    r.id
                ))
            })?;
            let part = self.part_by_name(resolved).ok_or_else(|| {
                Error::invalid_package_structure(format!(
                    "relationship {:?} targets {resolved:?}, which is not a package part",
                    r.id
                ))
            })?;
            if !parts
                .iter()
                .any(|p| p.name.eq_ignore_ascii_case(&part.name))
            {
                parts.push(part);
            }
            Ok(())
        };
        for r in &self.package_rels {
            consider(r, &mut matched)?;
        }
        for (_, rels) in &self.part_rels {
            for r in rels {
                consider(r, &mut matched)?;
            }
        }
        if matched == 0 {
            return Err(Error::invalid_package_structure(format!(
                "no relationship of type {rel_type:?} in the package"
            )));
        }
        if parts.len() > 1 {
            return Err(Error::invalid_package_structure(format!(
                "relationship type {rel_type:?} is ambiguous: {} distinct target parts",
                parts.len()
            )));
        }
        Ok(parts)
    }

    /// The set of internal part names reachable from `start`, following internal
    /// relationships. Bounded by `max_opc_rel_depth` and a visited set, so a cycle
    /// terminates rather than recursing without bound.
    pub fn reachable_parts(&self, start: &str, limits: Limits) -> Result<Vec<String>> {
        let mut visited: Vec<String> = Vec::new();
        let mut out: Vec<String> = Vec::new();
        self.walk(start, 0, limits, &mut visited, &mut out)?;
        Ok(out)
    }

    fn walk(
        &self,
        name: &str,
        depth: u32,
        limits: Limits,
        visited: &mut Vec<String>,
        out: &mut Vec<String>,
    ) -> Result<()> {
        if depth >= limits.max_opc_rel_depth {
            return Err(Error::resource_limit(format!(
                "OPC relationship traversal exceeded max_opc_rel_depth {}",
                limits.max_opc_rel_depth
            )));
        }
        if visited.iter().any(|v| v.eq_ignore_ascii_case(name)) {
            return Ok(());
        }
        visited.push(name.to_string());
        out.push(name.to_string());
        let Some(part) = self.part_by_name(name) else {
            return Ok(());
        };
        let Some((_, rels)) = self.part_rels.iter().find(|(o, _)| *o == part.ordinal) else {
            return Ok(());
        };
        for r in rels {
            if let Some(target) = r.resolved.as_deref() {
                self.walk(target, depth + 1, limits, visited, out)?;
            }
        }
        Ok(())
    }

    /// Deterministically encode the model (the derived serialization stored in a
    /// `PackageOpcModel` seed node). Length-prefixed little-endian.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"OPCM");
        out.push(1);
        put_u32(&mut out, self.content_types.defaults.len() as u32);
        for (k, v) in &self.content_types.defaults {
            put_str(&mut out, k);
            put_str(&mut out, v);
        }
        put_u32(&mut out, self.content_types.overrides.len() as u32);
        for (k, v) in &self.content_types.overrides {
            put_str(&mut out, k);
            put_str(&mut out, v);
        }
        put_u32(&mut out, self.parts.len() as u32);
        for p in &self.parts {
            put_u32(&mut out, p.ordinal);
            put_opt_str(&mut out, p.content_type.as_deref());
            put_str(&mut out, &p.name);
        }
        put_rels(&mut out, &self.package_rels);
        put_u32(&mut out, self.part_rels.len() as u32);
        for (owner, rels) in &self.part_rels {
            put_u32(&mut out, *owner);
            put_rels(&mut out, rels);
        }
        put_u32(&mut out, self.issues.len() as u32);
        for i in &self.issues {
            put_str(&mut out, i);
        }
        out
    }

    /// Decode a model produced by [`OpcModel::encode`].
    pub fn decode(bytes: &[u8]) -> Result<OpcModel> {
        let mut r = ByteReader::new(bytes);
        if r.bytes(4)? != b"OPCM" {
            return Err(corrupt("bad OPC model magic"));
        }
        let version = r.u8()?;
        if version != 1 {
            return Err(corrupt("unsupported OPC model version"));
        }
        let mut content_types = ContentTypes::default();
        let nd = r.u32()?;
        for _ in 0..nd {
            let k = r.string()?;
            let v = r.string()?;
            content_types.defaults.insert(k, v);
        }
        let no = r.u32()?;
        for _ in 0..no {
            let k = r.string()?;
            let v = r.string()?;
            content_types.overrides.insert(k, v);
        }
        let np = r.u32()?;
        let mut parts = Vec::new();
        for _ in 0..np {
            let ordinal = r.u32()?;
            let content_type = r.opt_string()?;
            let name = r.string()?;
            parts.push(OpcPart {
                ordinal,
                name,
                content_type,
            });
        }
        let package_rels = read_rels(&mut r)?;
        let npr = r.u32()?;
        let mut part_rels = Vec::new();
        for _ in 0..npr {
            let owner = r.u32()?;
            let rels = read_rels(&mut r)?;
            part_rels.push((owner, rels));
        }
        let ni = r.u32()?;
        let mut issues = Vec::new();
        for _ in 0..ni {
            issues.push(r.string()?);
        }
        if !r.at_end() {
            return Err(corrupt("OPC model has trailing bytes"));
        }
        Ok(OpcModel {
            content_types,
            parts,
            package_rels,
            part_rels,
            issues,
        })
    }
}

fn corrupt(msg: &str) -> Error {
    Error::invalid_package_structure(format!("corrupt OPC model: {msg}"))
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    put_u32(out, s.len() as u32);
    out.extend_from_slice(s.as_bytes());
}

fn put_opt_str(out: &mut Vec<u8>, s: Option<&str>) {
    match s {
        Some(s) => {
            out.push(1);
            put_str(out, s);
        }
        None => out.push(0),
    }
}

fn put_rels(out: &mut Vec<u8>, rels: &[Relationship]) {
    put_u32(out, rels.len() as u32);
    for r in rels {
        put_str(out, &r.id);
        put_str(out, &r.rel_type);
        put_str(out, &r.target);
        out.push(match r.mode {
            TargetMode::Internal => 0,
            TargetMode::External => 1,
        });
        put_opt_str(out, r.resolved.as_deref());
    }
}

fn read_rels(r: &mut ByteReader<'_>) -> Result<Vec<Relationship>> {
    let n = r.u32()?;
    let mut out = Vec::new();
    for _ in 0..n {
        let id = r.string()?;
        let rel_type = r.string()?;
        let target = r.string()?;
        let mode = match r.u8()? {
            0 => TargetMode::Internal,
            1 => TargetMode::External,
            _ => return Err(corrupt("bad relationship target mode")),
        };
        let resolved = r.opt_string()?;
        out.push(Relationship {
            id,
            rel_type,
            target,
            mode,
            resolved,
        });
    }
    Ok(out)
}

/// A tiny bounds-checked little-endian reader for the derived model encoding.
struct ByteReader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> ByteReader<'a> {
    fn new(b: &'a [u8]) -> Self {
        ByteReader { b, at: 0 }
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| corrupt("read overflow"))?;
        if end > self.b.len() {
            return Err(corrupt("truncated"));
        }
        let s = &self.b[self.at..end];
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

    fn string(&mut self) -> Result<String> {
        let n = self.u32()? as usize;
        let bytes = self.bytes(n)?;
        core::str::from_utf8(bytes)
            .map(str::to_string)
            .map_err(|_| corrupt("string is not UTF-8"))
    }

    fn opt_string(&mut self) -> Result<Option<String>> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.string()?)),
            _ => Err(corrupt("bad optional-string tag")),
        }
    }

    fn at_end(&self) -> bool {
        self.at == self.b.len()
    }
}

/// Convert a ZIP member name (relative, no leading `/`) into an absolute OPC part
/// name, validating the part-name grammar. A traversal name (`..`, absolute, drive,
/// backslash) is a typed decline.
pub fn part_name_from_member(name: &str) -> Result<String> {
    if name.is_empty() {
        return Err(Error::invalid_package_structure(
            "empty package member name",
        ));
    }
    let part = format!("/{name}");
    validate_part_name(&part)?;
    Ok(part)
}

/// Validate the OPC part-name grammar (ISC 29500-2 §9.1.1): absolute, `/`-rooted,
/// non-empty segments, no `.`/`..` segment, no trailing `/`, ASCII, and only
/// `pchar` / `pct-encoded` bytes. The reserved `/[Content_Types].xml` is accepted
/// verbatim (its `[`/`]` are not `pchar`).
pub fn validate_part_name(name: &str) -> Result<()> {
    if name == CONTENT_TYPES_PART {
        return Ok(());
    }
    if !name.starts_with('/') {
        return Err(Error::invalid_package_structure(
            "part name must be absolute (`/`-rooted)",
        ));
    }
    if name.len() > 1 && name.ends_with('/') {
        return Err(Error::invalid_package_structure(
            "part name must not end with `/`",
        ));
    }
    if !name.is_ascii() {
        return Err(Error::invalid_package_structure("part name must be ASCII"));
    }
    for seg in name[1..].split('/') {
        if seg.is_empty() {
            return Err(Error::invalid_package_structure(
                "part name has an empty segment",
            ));
        }
        if seg == "." || seg == ".." {
            return Err(Error::invalid_package_structure(
                "part name must not contain `.` or `..` segments",
            ));
        }
        validate_segment(seg)?;
    }
    Ok(())
}

fn validate_segment(seg: &str) -> Result<()> {
    let b = seg.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'%' {
            if i + 2 >= b.len() || !b[i + 1].is_ascii_hexdigit() || !b[i + 2].is_ascii_hexdigit() {
                return Err(Error::invalid_package_structure(
                    "malformed percent-encoding in part name",
                ));
            }
            i += 3;
            continue;
        }
        if is_pchar(c) {
            i += 1;
            continue;
        }
        return Err(Error::invalid_package_structure(format!(
            "invalid byte 0x{c:02x} in part name"
        )));
    }
    Ok(())
}

fn is_pchar(c: u8) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            b'-' | b'.'
                | b'_'
                | b'~'
                | b'!'
                | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
                | b':'
                | b'@'
        )
}

fn extension_of(part_name: &str) -> Option<&str> {
    let last = part_name.rsplit('/').next()?;
    let dot = last.rfind('.')?;
    if dot + 1 >= last.len() {
        return None;
    }
    Some(&last[dot + 1..])
}

/// The directory base URI of a source part, ending in `/` (`/a/b/c.xml` → `/a/b/`).
pub fn part_base_dir(part_name: &str) -> String {
    match part_name.rfind('/') {
        Some(i) => part_name[..=i].to_string(),
        None => "/".to_string(),
    }
}

/// Whether a relationship target is an absolute URI (has an RFC 3986 scheme),
/// which makes it an inert external identifier regardless of `TargetMode`.
pub fn is_absolute_uri(target: &str) -> bool {
    let b = target.as_bytes();
    if b.is_empty() || !b[0].is_ascii_alphabetic() {
        return false;
    }
    let mut i = 1;
    while i < b.len() {
        let c = b[i];
        if c == b':' {
            return true;
        }
        if !(c.is_ascii_alphanumeric() || c == b'+' || c == b'-' || c == b'.') {
            return false;
        }
        i += 1;
    }
    false
}

/// Resolve an internal relationship `Target` against the source part's base URI
/// (RFC 3986 §5), normalizing `.`/`..` segments and clamping the result inside the
/// package root. A target that escapes the root, is empty, or is not a valid part
/// name is a typed decline — a relationship target is never a host path.
pub fn resolve_part_target(base_dir: &str, target: &str, limits: Limits) -> Result<String> {
    if target.is_empty() {
        return Err(Error::invalid_package_structure(
            "relationship target is empty",
        ));
    }
    if target.bytes().any(|b| b == 0 || b == b'\\' || b < 0x20) {
        return Err(Error::invalid_package_structure(
            "relationship target contains forbidden bytes",
        ));
    }
    let no_fragment = target.split('#').next().unwrap_or("");
    let path = no_fragment.split('?').next().unwrap_or("");
    if path.is_empty() {
        return Err(Error::invalid_package_structure(
            "relationship target has no path",
        ));
    }
    let combined = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("{base_dir}{path}")
    };
    let mut segs: Vec<&str> = Vec::new();
    for seg in combined.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if segs.pop().is_none() {
                    return Err(Error::invalid_package_structure(
                        "relationship target escapes the package root",
                    ));
                }
            }
            other => segs.push(other),
        }
    }
    if segs.is_empty() {
        return Err(Error::invalid_package_structure(
            "relationship target resolves to the package root",
        ));
    }
    let resolved = format!("/{}", segs.join("/"));
    validate_part_name(&resolved)?;
    if resolved.len() as u64 > u64::from(limits.max_opc_part_name_bytes) {
        return Err(Error::resource_limit(
            "resolved part name exceeds max_opc_part_name_bytes",
        ));
    }
    Ok(resolved)
}

/// The inputs to a package parse: member `(ordinal, absolute part name)` pairs, the
/// decoded `[Content_Types].xml`, and decoded `.rels` parts keyed by owner ordinal
/// (`None` = the package).
pub fn parse_opc(
    members: &[(u32, String)],
    content_types_xml: Option<&[u8]>,
    rels: &[(Option<u32>, &[u8])],
    limits: Limits,
) -> Result<OpcModel> {
    // Part-name validation and case-insensitive duplicate detection. Ambiguity is a
    // typed decline, never a last-wins guess (research C §8, I §3).
    let mut seen: BTreeMap<String, ()> = BTreeMap::new();
    for (_, name) in members {
        validate_part_name(name)?;
        if name.len() as u64 > u64::from(limits.max_opc_part_name_bytes) {
            return Err(Error::resource_limit(
                "part name exceeds max_opc_part_name_bytes",
            ));
        }
        if seen.insert(name.to_ascii_lowercase(), ()).is_some() {
            return Err(Error::invalid_package_structure(
                "duplicate or case-equivalent part name makes package identity ambiguous",
            ));
        }
    }

    let ct_bytes = content_types_xml
        .ok_or_else(|| Error::invalid_package_structure("package has no [Content_Types].xml"))?;
    let content_types = parse_content_types(ct_bytes, limits)?;

    let mut parts: Vec<OpcPart> = members
        .iter()
        .map(|(ordinal, name)| OpcPart {
            ordinal: *ordinal,
            name: name.clone(),
            content_type: content_types.content_type_for(name).map(str::to_string),
        })
        .collect();
    parts.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });

    let mut package_rels: Vec<Relationship> = Vec::new();
    let mut part_rels: Vec<(u32, Vec<Relationship>)> = Vec::new();
    let mut issues: Vec<String> = Vec::new();

    for (owner, xml) in rels {
        let base_dir = match owner {
            None => "/".to_string(),
            Some(o) => {
                let name = members
                    .iter()
                    .find(|(ord, _)| ord == o)
                    .map(|(_, n)| n.as_str())
                    .ok_or_else(|| {
                        Error::invalid_package_structure(
                            "part relationships owner is not a package part",
                        )
                    })?;
                part_base_dir(name)
            }
        };
        let mut parsed = parse_relationships(xml, &base_dir, limits)?;
        parsed.sort_by(|a, b| a.id.cmp(&b.id));
        let mut ids: BTreeMap<&str, ()> = BTreeMap::new();
        for r in &parsed {
            if ids.insert(r.id.as_str(), ()).is_some() {
                return Err(Error::invalid_package_structure(
                    "duplicate relationship id within one relationships part",
                ));
            }
        }
        for r in &parsed {
            if let Some(target) = r.resolved.as_deref()
                && !parts.iter().any(|p| p.name.eq_ignore_ascii_case(target))
            {
                issues.push(format!(
                    "relationship {:?} targets missing part {target:?}",
                    r.id
                ));
            }
        }
        match owner {
            None => package_rels = parsed,
            Some(o) => part_rels.push((*o, parsed)),
        }
    }
    part_rels.sort_by_key(|(o, _)| *o);

    Ok(OpcModel {
        content_types,
        parts,
        package_rels,
        part_rels,
        issues,
    })
}

/// Parse and harden `[Content_Types].xml`.
pub fn parse_content_types(xml: &[u8], limits: Limits) -> Result<ContentTypes> {
    harden_xml(xml, limits)?;
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut st = XmlState::new();
    let mut out = ContentTypes::default();
    let mut items: u64 = 0;
    loop {
        let ev = reader.read_event().map_err(xml_err)?;
        st.event(limits)?;
        match ev {
            Event::Eof => break,
            Event::DocType(_) => return Err(doctype_declined()),
            Event::Start(e) => {
                st.open(limits)?;
                handle_content_types_element(&mut out, &mut items, &e, limits)?;
            }
            Event::Empty(e) => {
                st.leaf(limits)?;
                handle_content_types_element(&mut out, &mut items, &e, limits)?;
            }
            Event::End(_) => st.close(),
            Event::Text(t) => st.text(t.len(), limits)?,
            _ => {}
        }
    }
    Ok(out)
}

fn handle_content_types_element(
    out: &mut ContentTypes,
    items: &mut u64,
    e: &BytesStart<'_>,
    limits: Limits,
) -> Result<()> {
    let local = e.name().local_name();
    match local.as_ref() {
        "Default" => {
            let attrs = read_attrs(e, limits)?;
            let ext = attr_of(&attrs, "Extension")
                .ok_or_else(|| Error::invalid_package_structure("Default lacks Extension"))?;
            let ct = attr_of(&attrs, "ContentType")
                .ok_or_else(|| Error::invalid_package_structure("Default lacks ContentType"))?;
            if ext.is_empty() || ct.is_empty() {
                return Err(Error::invalid_package_structure(
                    "Default has an empty Extension/ContentType",
                ));
            }
            charge_content_type(items, limits)?;
            out.defaults
                .insert(ext.to_ascii_lowercase(), ct.to_string());
        }
        "Override" => {
            let attrs = read_attrs(e, limits)?;
            let part = attr_of(&attrs, "PartName")
                .ok_or_else(|| Error::invalid_package_structure("Override lacks PartName"))?;
            let ct = attr_of(&attrs, "ContentType")
                .ok_or_else(|| Error::invalid_package_structure("Override lacks ContentType"))?;
            validate_part_name(part)?;
            if part.len() as u64 > u64::from(limits.max_opc_part_name_bytes) {
                return Err(Error::resource_limit(
                    "Override PartName exceeds max_opc_part_name_bytes",
                ));
            }
            charge_content_type(items, limits)?;
            out.overrides
                .insert(part.to_ascii_lowercase(), ct.to_string());
        }
        // `<Types>` and any unknown/foreign element are retained as data, not interpreted.
        _ => {}
    }
    Ok(())
}

fn charge_content_type(items: &mut u64, limits: Limits) -> Result<()> {
    *items = items.saturating_add(1);
    if *items > u64::from(limits.max_opc_content_types_overrides) {
        return Err(Error::resource_limit(
            "content types exceed max_opc_content_types_overrides",
        ));
    }
    Ok(())
}

/// Parse and harden one `.rels` part, resolving internal targets against `base_dir`.
pub fn parse_relationships(
    xml: &[u8],
    base_dir: &str,
    limits: Limits,
) -> Result<Vec<Relationship>> {
    harden_xml(xml, limits)?;
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut st = XmlState::new();
    let mut out: Vec<Relationship> = Vec::new();
    loop {
        let ev = reader.read_event().map_err(xml_err)?;
        st.event(limits)?;
        match ev {
            Event::Eof => break,
            Event::DocType(_) => return Err(doctype_declined()),
            Event::Start(e) => {
                st.open(limits)?;
                handle_relationship_element(&mut out, &e, base_dir, limits)?;
            }
            Event::Empty(e) => {
                st.leaf(limits)?;
                handle_relationship_element(&mut out, &e, base_dir, limits)?;
            }
            Event::End(_) => st.close(),
            Event::Text(t) => st.text(t.len(), limits)?,
            _ => {}
        }
    }
    Ok(out)
}

fn handle_relationship_element(
    out: &mut Vec<Relationship>,
    e: &BytesStart<'_>,
    base_dir: &str,
    limits: Limits,
) -> Result<()> {
    if e.name().local_name().as_ref() != "Relationship" {
        return Ok(());
    }
    let attrs = read_attrs(e, limits)?;
    let id = attr_of(&attrs, "Id")
        .ok_or_else(|| Error::invalid_package_structure("Relationship lacks Id"))?;
    let rel_type = attr_of(&attrs, "Type")
        .ok_or_else(|| Error::invalid_package_structure("Relationship lacks Type"))?;
    let target = attr_of(&attrs, "Target")
        .ok_or_else(|| Error::invalid_package_structure("Relationship lacks Target"))?;
    if id.is_empty() || rel_type.is_empty() {
        return Err(Error::invalid_package_structure(
            "Relationship has an empty Id/Type",
        ));
    }
    let declared = match attr_of(&attrs, "TargetMode") {
        None | Some("Internal") => TargetMode::Internal,
        Some("External") => TargetMode::External,
        Some(other) => {
            return Err(Error::invalid_package_structure(format!(
                "unknown TargetMode {other:?}"
            )));
        }
    };
    if out.len() as u64 >= u64::from(limits.max_opc_rels) {
        return Err(Error::resource_limit("relationships exceed max_opc_rels"));
    }
    // External (declared or an absolute URI) is an inert identifier: never resolved.
    let external = declared == TargetMode::External || is_absolute_uri(target);
    let (mode, resolved) = if external {
        (TargetMode::External, None)
    } else {
        (
            TargetMode::Internal,
            Some(resolve_part_target(base_dir, target, limits)?),
        )
    };
    out.push(Relationship {
        id: id.to_string(),
        rel_type: rel_type.to_string(),
        target: target.to_string(),
        mode,
        resolved,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CT: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#;

    fn rels_xml(body: &str) -> Vec<u8> {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{body}</Relationships>"#
        )
        .into_bytes()
    }

    #[test]
    fn part_name_grammar() {
        assert!(validate_part_name("/word/document.xml").is_ok());
        assert!(validate_part_name("/a/b_c-d.e~f").is_ok());
        assert!(validate_part_name("/a/%20b").is_ok());
        assert!(validate_part_name(CONTENT_TYPES_PART).is_ok());
        assert!(validate_part_name("word/document.xml").is_err()); // not absolute
        assert!(validate_part_name("/a/").is_err()); // trailing slash
        assert!(validate_part_name("/a//b").is_err()); // empty segment
        assert!(validate_part_name("/a/../b").is_err()); // traversal
        assert!(validate_part_name("/a/./b").is_err()); // dot segment
        assert!(validate_part_name("/a/b%2").is_err()); // bad pct
        assert!(validate_part_name("/a/b\\c").is_err()); // backslash
        assert!(validate_part_name("/a/[b].xml").is_err()); // not pchar
    }

    #[test]
    fn content_types_precedence_and_extension_matching() {
        let ct = parse_content_types(CT, Limits::DEFAULT).unwrap();
        assert_eq!(
            ct.content_type_for("/word/document.xml"),
            Some(
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"
            ),
        );
        assert_eq!(
            ct.content_type_for("/word/styles.xml"),
            Some("application/xml")
        );
        assert_eq!(
            ct.content_type_for("/_rels/.rels"),
            Some("application/vnd.openxmlformats-package.relationships+xml"),
        );
        // Case-insensitive override and extension matching.
        assert_eq!(
            ct.content_type_for("/WORD/DOCUMENT.XML"),
            Some(
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"
            ),
        );
    }

    #[test]
    fn target_resolution_is_rfc3986_and_clamped() {
        let l = Limits::DEFAULT;
        assert_eq!(
            resolve_part_target("/", "word/document.xml", l).unwrap(),
            "/word/document.xml"
        );
        assert_eq!(
            resolve_part_target("/word/", "media/image1.png", l).unwrap(),
            "/word/media/image1.png"
        );
        assert_eq!(
            resolve_part_target("/word/", "../media/image1.png", l).unwrap(),
            "/media/image1.png"
        );
        assert_eq!(
            resolve_part_target("/", "/abs/part.xml", l).unwrap(),
            "/abs/part.xml"
        );
        assert_eq!(
            resolve_part_target("/a/b/", "c.xml#frag?q=1", l).unwrap(),
            "/a/b/c.xml"
        );
        assert!(resolve_part_target("/", "../../escape.xml", l).is_err());
        assert!(resolve_part_target("/", "a/../..", l).is_err());
        assert!(resolve_part_target("/", "", l).is_err());
    }

    #[test]
    fn external_targets_are_inert() {
        let pkg = rels_xml(
            r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com/x" TargetMode="External"/>"#,
        );
        let rels = parse_relationships(&pkg, "/", Limits::DEFAULT).unwrap();
        assert_eq!(rels.len(), 2);
        assert_eq!(rels[0].resolved.as_deref(), Some("/word/document.xml"));
        assert!(!rels[0].is_external());
        assert!(rels[1].is_external());
        assert_eq!(rels[1].resolved, None);
    }

    #[test]
    fn absolute_uri_target_is_external_even_without_target_mode() {
        let pkg = rels_xml(
            r#"<Relationship Id="rId1" Type="http://x/y" Target="http://evil.example/p"/>"#,
        );
        let rels = parse_relationships(&pkg, "/", Limits::DEFAULT).unwrap();
        assert!(rels[0].is_external());
        assert_eq!(rels[0].resolved, None);
    }

    #[test]
    fn doctype_is_refused() {
        let bad = br#"<?xml version="1.0"?><!DOCTYPE foo [<!ENTITY x "y">]><Types xmlns="x"/>"#;
        assert_eq!(
            parse_content_types(bad, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidXmlStructure
        );
    }

    #[test]
    fn malformed_xml_is_typed() {
        let bad = br#"<Types><Default"#;
        assert_eq!(
            parse_content_types(bad, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidXmlStructure
        );
    }

    #[test]
    fn utf16_and_nul_are_refused() {
        let utf16 = [0xFF, 0xFE, b'<', 0, b'T', 0];
        assert_eq!(
            parse_content_types(&utf16, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidXmlStructure
        );
        let nul = b"<Types\0/>";
        assert_eq!(
            parse_content_types(nul, Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidXmlStructure
        );
    }

    #[test]
    fn depth_is_bounded() {
        let deep = format!("<a>{}</a>", "<b>".repeat(100) + &"</b>".repeat(100));
        let l = Limits {
            max_xml_depth: 8,
            ..Limits::DEFAULT
        };
        assert_eq!(
            parse_content_types(deep.as_bytes(), l).unwrap_err().class(),
            crate::ErrorClass::InvalidXmlStructure
        );
    }

    #[test]
    fn model_roundtrips_and_discovers() {
        let pkg = rels_xml(
            r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>"#,
        );
        let members = vec![
            (0u32, "/[Content_Types].xml".to_string()),
            (1u32, "/_rels/.rels".to_string()),
            (2u32, "/word/document.xml".to_string()),
        ];
        let model = parse_opc(
            &members,
            Some(CT),
            &[(None, pkg.as_slice())],
            Limits::DEFAULT,
        )
        .unwrap();
        let doc = model
            .parts_by_relationship_type(OFFICE_DOCUMENT_REL)
            .unwrap();
        assert_eq!(doc.len(), 1);
        assert_eq!(doc[0].name, "/word/document.xml");
        assert!(doc[0].content_type.is_some());

        let bytes = model.encode();
        let back = OpcModel::decode(&bytes).unwrap();
        assert_eq!(back, model);
        assert_eq!(back.part_by_name("/word/DOCUMENT.xml").unwrap().ordinal, 2);
    }

    #[test]
    fn missing_ambiguous_and_cyclic_discovery_fail_closed() {
        let none = rels_xml(r#"<Relationship Id="rId1" Type="t/x" Target="word/a.xml"/>"#);
        let members = vec![
            (0u32, "/[Content_Types].xml".to_string()),
            (1u32, "/word/a.xml".to_string()),
        ];
        let model = parse_opc(
            &members,
            Some(CT),
            &[(None, none.as_slice())],
            Limits::DEFAULT,
        )
        .unwrap();
        assert_eq!(
            model
                .parts_by_relationship_type("t/absent")
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidPackageStructure
        );

        let dup = rels_xml(
            r#"<Relationship Id="rId1" Type="t/dup" Target="word/a.xml"/><Relationship Id="rId2" Type="t/dup" Target="word/b.xml"/>"#,
        );
        let members2 = vec![
            (0u32, "/[Content_Types].xml".to_string()),
            (1u32, "/word/a.xml".to_string()),
            (2u32, "/word/b.xml".to_string()),
        ];
        let model2 = parse_opc(
            &members2,
            Some(CT),
            &[(None, dup.as_slice())],
            Limits::DEFAULT,
        )
        .unwrap();
        assert_eq!(
            model2
                .parts_by_relationship_type("t/dup")
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidPackageStructure
        );

        let ext = rels_xml(
            r#"<Relationship Id="rId1" Type="t/dup" Target="https://x/y" TargetMode="External"/>"#,
        );
        let model3 = parse_opc(
            &members,
            Some(CT),
            &[(None, ext.as_slice())],
            Limits::DEFAULT,
        )
        .unwrap();
        assert_eq!(
            model3
                .parts_by_relationship_type("t/dup")
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidPackageStructure
        );
    }

    #[test]
    fn cycles_are_bounded_not_infinite() {
        let a = rels_xml(r#"<Relationship Id="rId1" Type="t/a" Target="b.xml"/>"#);
        let b = rels_xml(r#"<Relationship Id="rId2" Type="t/b" Target="a.xml"/>"#);
        let members = vec![
            (0u32, "/[Content_Types].xml".to_string()),
            (1u32, "/a.xml".to_string()),
            (2u32, "/b.xml".to_string()),
        ];
        let model = parse_opc(
            &members,
            Some(CT),
            &[(Some(1), a.as_slice()), (Some(2), b.as_slice())],
            Limits::DEFAULT,
        )
        .unwrap();
        let reach = model.reachable_parts("/a.xml", Limits::DEFAULT).unwrap();
        assert_eq!(reach, vec!["/a.xml".to_string(), "/b.xml".to_string()]);
    }

    #[test]
    fn duplicate_or_case_equivalent_parts_decline() {
        let members = vec![
            (0u32, "/[Content_Types].xml".to_string()),
            (1u32, "/a.xml".to_string()),
            (2u32, "/A.XML".to_string()),
        ];
        assert_eq!(
            parse_opc(&members, Some(CT), &[], Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidPackageStructure
        );
    }

    #[test]
    fn missing_content_types_declines() {
        let members = vec![(0u32, "/a.xml".to_string())];
        assert_eq!(
            parse_opc(&members, None, &[], Limits::DEFAULT)
                .unwrap_err()
                .class(),
            crate::ErrorClass::InvalidPackageStructure
        );
    }

    #[test]
    fn traversal_member_name_declines() {
        assert!(part_name_from_member("../etc/passwd").is_err());
        assert!(part_name_from_member("/abs").is_err());
        assert!(part_name_from_member("a/../../b").is_err());
        assert_eq!(
            part_name_from_member("word/document.xml").unwrap(),
            "/word/document.xml"
        );
    }

    #[test]
    fn absolute_uri_detection() {
        assert!(is_absolute_uri("https://x/y"));
        assert!(is_absolute_uri("file:///etc/passwd"));
        assert!(is_absolute_uri("C:/Windows/x"));
        assert!(!is_absolute_uri("word/document.xml"));
        assert!(!is_absolute_uri("../media/x.png"));
        assert!(!is_absolute_uri(""));
    }
}
