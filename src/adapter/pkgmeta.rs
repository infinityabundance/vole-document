//! Bounded, representation-preserving **package-metadata** adapter (Phase 21.29).
//!
//! A package manifest's physical bytes belong to another format: `package.json` and
//! `package-lock.json` are JSON, `Cargo.toml`, `pyproject.toml`, and `Cargo.lock` are
//! TOML. So the physical layer is already defined — this adapter **reuses** the shared
//! [`crate::adapter::json`] and [`crate::adapter::toml`] parsers (never a second JSON
//! or TOML parser) and adds only a bounded **semantic** projection on top. The exact
//! leaf is still the whole source (a `DocumentExact`, a RAW-like authority); everything
//! this module produces is a bounded, deterministic (`Q_gen`) projection that never
//! sits on the exactness path.
//!
//! ## Reuse, not a second node arena
//!
//! The model embeds the reused arena — the JSON [`JsonModel`] for the JSON dialects,
//! the TOML [`TomlModel`] for the TOML dialects. So member **order**, **duplicate
//! keys**, numeric **spelling** (`1e3`, `1_000`, `0x1F`), string-escape **spelling**,
//! key **quoting**, and every token's **exact byte span** carry the identical
//! guarantees they carry in the base format. A package-metadata answer's spans are
//! therefore all `Q_gen` projections of the reused parse; none is on the exactness
//! path. **Nothing is normalized or re-serialized.**
//!
//! ## What is preserved
//!
//! A manifest is a **section tree** plus the **key/value entries** inside each
//! section. This adapter records, in document order:
//!
//! * every **section** (a named table / array-of-tables / array-of-tables element in
//!   TOML, or an object / array member in JSON) with its **exact name token span**
//!   (when named) and its **exact value span**;
//! * every **entry** (a `key = value` pair in TOML, or a `"key": value` member in
//!   JSON) with its **exact key token span** and its **exact value span**, grouped by
//!   its owning section and kept in document order (duplicates preserved).
//!
//! The **dialect is recorded** ([`D_NPM_PACKAGE`], [`D_CARGO_MANIFEST`],
//! [`D_PYPROJECT_MANIFEST`], [`D_CARGO_LOCK`], [`D_NPM_LOCK`]), exactly as CSV/config
//! record theirs.
//!
//! ## Detection (a bounded semantic sub-test, content-only)
//!
//! A manifest's bytes are JSON or TOML, so detection is a **bounded semantic test**
//! run *before* the generic JSON and TOML detectors
//! ([`crate::field::document_format::detect_document_format`]) and **never** consults a
//! file name. Each dialect requires a **strong semantic shape**:
//!
//! * **`npm_package`** — a JSON object with a string `name` **and** a string
//!   `version` **and** at least one of `dependencies`/`devDependencies`/`scripts`/
//!   `main`/`engines` (≥ 2 package-specific keys, so a bare `{"name":…,"version":…}`
//!   is unambiguous and stays [`crate::field::document_format::DocumentFormat::Json`]);
//! * **`npm_lock`** — a JSON object with a `lockfileVersion` member **and** an object
//!   `packages` or `dependencies` member;
//! * **`cargo_manifest`** — TOML with a `[package]` **table** carrying a string
//!   `name` (and a string `version` or a `[dependencies]`/`[workspace]` table);
//! * **`pyproject_manifest`** — TOML with a `[build-system]` table carrying `requires`,
//!   or a `[project]` table with a string `name`+`version` (PEP 621), or a
//!   `[tool.poetry]` table;
//! * **`cargo_lock`** — TOML with a top-level `version` and a `[[package]]`
//!   array-of-tables whose element tables carry a string `name`.
//!
//! ## Recorded ambiguities (honest)
//!
//! * A generic JSON document `{"name":"x","version":"1.0.0"}` — or any JSON with
//!   `name`+`version` but no third package-specific key — is **byte-for-byte
//!   indistinguishable** from a minimal `package.json`; it is deliberately **not**
//!   claimed and stays `Json`.
//! * A generic TOML document `name = "x"` / `version = "1.0.0"` with no `[package]`
//!   table is indistinguishable from a Cargo manifest header and stays `Toml`; Cargo
//!   detection requires the `[package]` **table**, never a bare `name`/`version`.
//! * A virtual Cargo workspace manifest carrying **only** `[workspace]` (no
//!   `[package]`) is not claimed and stays `Toml`.
//! * A manifest that is shaped like a dialect but semantically odd is preserved
//!   verbatim; the adapter records what is present and never rejects it.
//!
//! ## Bounds and no panics
//!
//! Untrusted input can only ever yield a typed decline, never a panic or an unbounded
//! allocation: the source length by [`Limits::max_pkgmeta_document_bytes`], the reused
//! arena node count by [`Limits::max_pkgmeta_nodes`] (in addition to the JSON/TOML caps
//! the shared parsers already enforce), the section count by
//! [`Limits::max_pkgmeta_sections`], the entry count by [`Limits::max_pkgmeta_entries`],
//! and the structural recursion by [`Limits::max_pkgmeta_depth`].

use crate::adapter::json::{self, JsonModel, K_ARRAY, K_OBJECT, K_STRING};
use crate::adapter::toml::{
    self, K_ARRAY as T_ARRAY, K_ARRAY_TABLE as T_ARRAY_TABLE, K_INLINE_TABLE as T_INLINE_TABLE,
    K_ROOT as T_ROOT, K_TABLE as T_TABLE, TomlModel, is_string as toml_is_string,
    is_table_like as toml_is_table_like,
};
use crate::error::{Error, ErrorClass, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded arena nodes (defends the decoder against a hostile blob).
pub const MAX_MODEL_NODES: u32 = 1 << 24;
/// Hard cap on decoded sections (defends the decoder against a hostile blob).
pub const MAX_MODEL_SECTIONS: u32 = 1 << 20;
/// Hard cap on decoded entries (defends the decoder against a hostile blob).
pub const MAX_MODEL_ENTRIES: u32 = 1 << 22;

/// Sentinel for an absent index (no parent / no name node / no entry).
pub const NONE: u32 = u32::MAX;

/// Dialect: an npm `package.json`.
pub const D_NPM_PACKAGE: u8 = 0;
/// Dialect: a Cargo `Cargo.toml`.
pub const D_CARGO_MANIFEST: u8 = 1;
/// Dialect: a Python `pyproject.toml`.
pub const D_PYPROJECT_MANIFEST: u8 = 2;
/// Dialect: a Cargo `Cargo.lock`.
pub const D_CARGO_LOCK: u8 = 3;
/// Dialect: an npm `package-lock.json`.
pub const D_NPM_LOCK: u8 = 4;

/// The stable dialect name.
pub const fn dialect_name(dialect: u8) -> &'static str {
    match dialect {
        D_NPM_PACKAGE => "npm_package",
        D_CARGO_MANIFEST => "cargo_manifest",
        D_PYPROJECT_MANIFEST => "pyproject_manifest",
        D_CARGO_LOCK => "cargo_lock",
        D_NPM_LOCK => "npm_lock",
        _ => "unknown",
    }
}

/// Whether `dialect` is served by the reused JSON arena (else the reused TOML arena).
pub const fn is_json_dialect(dialect: u8) -> bool {
    matches!(dialect, D_NPM_PACKAGE | D_NPM_LOCK)
}

/// Section kind: the document root (a JSON object root / a TOML root table).
pub const S_ROOT: u8 = 0;
/// Section kind: a named table (TOML `[a]` / inline table) or a JSON object member.
pub const S_TABLE: u8 = 1;
/// Section kind: an array of tables (TOML `[[a]]`) or a JSON array member.
pub const S_ARRAY: u8 = 2;
/// Section kind: an element of an array (a TOML array-of-tables element table, or a
/// JSON array element object/array).
pub const S_ELEMENT: u8 = 3;

/// The stable section-kind name.
pub const fn section_kind_name(kind: u8) -> &'static str {
    match kind {
        S_ROOT => "root",
        S_TABLE => "table",
        S_ARRAY => "array",
        S_ELEMENT => "element",
        _ => "unknown",
    }
}

/// Role: the document root.
pub const ROLE_ROOT: u8 = 0;
/// Role: a `[package]` table.
pub const ROLE_PACKAGE: u8 = 1;
/// Role: a `[project]` table (PEP 621).
pub const ROLE_PROJECT: u8 = 2;
/// Role: a `[build-system]` table.
pub const ROLE_BUILD_SYSTEM: u8 = 3;
/// Role: a `[tool]` table.
pub const ROLE_TOOL: u8 = 4;
/// Role: a `dependencies` table / member.
pub const ROLE_DEPENDENCIES: u8 = 5;
/// Role: a `dev-dependencies` / `devDependencies` table / member.
pub const ROLE_DEV_DEPENDENCIES: u8 = 6;
/// Role: a `build-dependencies` table / member.
pub const ROLE_BUILD_DEPENDENCIES: u8 = 7;
/// Role: a `peerDependencies` member.
pub const ROLE_PEER_DEPENDENCIES: u8 = 8;
/// Role: an `optionalDependencies` member.
pub const ROLE_OPTIONAL_DEPENDENCIES: u8 = 9;
/// Role: a `scripts` table / member.
pub const ROLE_SCRIPTS: u8 = 10;
/// Role: an `engines` table / member.
pub const ROLE_ENGINES: u8 = 11;
/// Role: a `[features]` table.
pub const ROLE_FEATURES: u8 = 12;
/// Role: a `[workspace]` table / `workspaces` member.
pub const ROLE_WORKSPACE: u8 = 13;
/// Role: a `[lib]` table.
pub const ROLE_LIB: u8 = 14;
/// Role: a `[[bin]]` table / `bin` member.
pub const ROLE_BIN: u8 = 15;
/// Role: a `[profile]` table.
pub const ROLE_PROFILE: u8 = 16;
/// Role: a `[target]` table.
pub const ROLE_TARGET: u8 = 17;
/// Role: a `[[package]]` lockfile element.
pub const ROLE_LOCK_PACKAGE: u8 = 18;
/// Role: an `exports` member.
pub const ROLE_EXPORTS: u8 = 19;
/// Role: any other section / member.
pub const ROLE_OTHER: u8 = 20;

/// The stable role name.
pub const fn role_name(role: u8) -> &'static str {
    match role {
        ROLE_ROOT => "root",
        ROLE_PACKAGE => "package",
        ROLE_PROJECT => "project",
        ROLE_BUILD_SYSTEM => "build-system",
        ROLE_TOOL => "tool",
        ROLE_DEPENDENCIES => "dependencies",
        ROLE_DEV_DEPENDENCIES => "dev-dependencies",
        ROLE_BUILD_DEPENDENCIES => "build-dependencies",
        ROLE_PEER_DEPENDENCIES => "peer-dependencies",
        ROLE_OPTIONAL_DEPENDENCIES => "optional-dependencies",
        ROLE_SCRIPTS => "scripts",
        ROLE_ENGINES => "engines",
        ROLE_FEATURES => "features",
        ROLE_WORKSPACE => "workspace",
        ROLE_LIB => "lib",
        ROLE_BIN => "bin",
        ROLE_PROFILE => "profile",
        ROLE_TARGET => "target",
        ROLE_LOCK_PACKAGE => "lock-package",
        ROLE_EXPORTS => "exports",
        _ => "other",
    }
}

/// The role tag for a role name, if it is a known role.
pub fn role_for_name(name: &str) -> Option<u8> {
    (0u8..=ROLE_OTHER).find(|&r| role_name(r) == name)
}

/// The role tag for a section/member name under `dialect`.
pub fn role_of(dialect: u8, name: &str) -> u8 {
    match name {
        "package" => {
            if dialect == D_CARGO_LOCK {
                ROLE_LOCK_PACKAGE
            } else {
                ROLE_PACKAGE
            }
        }
        "project" => ROLE_PROJECT,
        "build-system" => ROLE_BUILD_SYSTEM,
        "tool" => ROLE_TOOL,
        "dependencies" => ROLE_DEPENDENCIES,
        "dev-dependencies" | "devDependencies" => ROLE_DEV_DEPENDENCIES,
        "build-dependencies" => ROLE_BUILD_DEPENDENCIES,
        "peerDependencies" => ROLE_PEER_DEPENDENCIES,
        "optionalDependencies" => ROLE_OPTIONAL_DEPENDENCIES,
        "scripts" => ROLE_SCRIPTS,
        "engines" => ROLE_ENGINES,
        "features" => ROLE_FEATURES,
        "workspace" | "workspaces" => ROLE_WORKSPACE,
        "lib" => ROLE_LIB,
        "bin" => ROLE_BIN,
        "profile" => ROLE_PROFILE,
        "target" => ROLE_TARGET,
        "exports" => ROLE_EXPORTS,
        _ => ROLE_OTHER,
    }
}

/// One recorded section (a table / array / element / root) with exact spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkgSection {
    /// The parent section index ([`NONE`] for the root).
    pub parent: u32,
    /// The role tag (`ROLE_*`).
    pub role: u8,
    /// The section kind (`S_*`).
    pub kind: u8,
    /// The naming key node (arena index), or [`NONE`] when unnamed.
    pub name_node: u32,
    /// The section's container node (arena index).
    pub node: u32,
    /// The first entry index owned by this section.
    pub first_entry: u32,
    /// The number of entries owned by this section.
    pub entry_count: u32,
    /// The naming key token's first source byte (`0` when unnamed).
    pub name_start: u64,
    /// One past the naming key token.
    pub name_end: u64,
    /// The section value/table's first source byte.
    pub start: u64,
    /// One past the section value/table.
    pub end: u64,
}

/// One recorded key/value entry with exact spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkgEntry {
    /// The owning section index.
    pub section: u32,
    /// The key node (arena index).
    pub key_node: u32,
    /// The value node (arena index).
    pub value_node: u32,
    /// The value node's kind tag (in the arena's own kind namespace).
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

/// The canonical derived package-metadata model (the materialization of a
/// `PkgmetaModel` node). It embeds the reused arena — the JSON [`JsonModel`] **or**
/// the TOML [`TomlModel`] — so every span is a `Q_gen` projection of that parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkgmetaModel {
    /// The recorded dialect (`D_*`).
    pub dialect: u8,
    /// The root node index in the embedded arena.
    pub root: u32,
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// The recorded sections, in document order.
    pub sections: Vec<PkgSection>,
    /// The recorded entries, grouped by section and in document order within a
    /// section.
    pub entries: Vec<PkgEntry>,
    /// The reused JSON arena (present iff the dialect is a JSON dialect).
    pub json: Option<JsonModel>,
    /// The reused TOML arena (present iff the dialect is a TOML dialect).
    pub toml: Option<TomlModel>,
}

impl PkgmetaModel {
    /// The recorded dialect name.
    pub fn dialect_name(&self) -> &'static str {
        dialect_name(self.dialect)
    }

    /// Whether this model is served by the reused JSON arena.
    pub fn is_json(&self) -> bool {
        self.json.is_some()
    }

    /// The section at `index`, if present.
    pub fn section(&self, index: u32) -> Option<&PkgSection> {
        self.sections.get(index as usize)
    }

    /// The entry at `index`, if present.
    pub fn entry(&self, index: u32) -> Option<&PkgEntry> {
        self.entries.get(index as usize)
    }

    /// The exact source length the spans are relative to.
    pub fn doc_len(&self) -> u64 {
        match (&self.json, &self.toml) {
            (Some(j), _) => j.doc_len,
            (_, Some(t)) => t.doc_len,
            _ => self.doc_len,
        }
    }

    /// The number of arena nodes.
    pub fn arena_len(&self) -> u32 {
        match (&self.json, &self.toml) {
            (Some(j), _) => j.nodes.len() as u32,
            (_, Some(t)) => t.nodes.len() as u32,
            _ => 0,
        }
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let arena = match (&self.json, &self.toml) {
            (Some(j), _) => j.encode(),
            (_, Some(t)) => t.encode(),
            _ => Vec::new(),
        };
        let mut out = Vec::with_capacity(
            32 + self.sections.len() * 60 + self.entries.len() * 48 + arena.len(),
        );
        out.extend_from_slice(b"PKGM");
        out.push(MODEL_VERSION);
        out.push(self.dialect);
        out.push(if self.json.is_some() { 0 } else { 1 });
        out.push(0); // reserved
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&self.root.to_le_bytes());
        out.extend_from_slice(&(self.sections.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());
        for s in &self.sections {
            out.extend_from_slice(&s.parent.to_le_bytes());
            out.push(s.role);
            out.push(s.kind);
            out.extend_from_slice(&[0, 0]); // reserved
            out.extend_from_slice(&s.name_node.to_le_bytes());
            out.extend_from_slice(&s.node.to_le_bytes());
            out.extend_from_slice(&s.first_entry.to_le_bytes());
            out.extend_from_slice(&s.entry_count.to_le_bytes());
            out.extend_from_slice(&s.name_start.to_le_bytes());
            out.extend_from_slice(&s.name_end.to_le_bytes());
            out.extend_from_slice(&s.start.to_le_bytes());
            out.extend_from_slice(&s.end.to_le_bytes());
        }
        for e in &self.entries {
            out.extend_from_slice(&e.section.to_le_bytes());
            out.extend_from_slice(&e.key_node.to_le_bytes());
            out.extend_from_slice(&e.value_node.to_le_bytes());
            out.push(e.value_kind);
            out.extend_from_slice(&[0, 0, 0]); // reserved
            out.extend_from_slice(&e.key_start.to_le_bytes());
            out.extend_from_slice(&e.key_end.to_le_bytes());
            out.extend_from_slice(&e.value_start.to_le_bytes());
            out.extend_from_slice(&e.value_end.to_le_bytes());
        }
        out.extend_from_slice(&(arena.len() as u32).to_le_bytes());
        out.extend_from_slice(&arena);
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<PkgmetaModel> {
        let mut r = Reader::new(bytes);
        if r.bytes(4)? != b"PKGM" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let dialect = r.u8()?;
        if dialect > D_NPM_LOCK {
            return Err(corrupt("unknown dialect"));
        }
        let arena_kind = r.u8()?;
        if arena_kind > 1 {
            return Err(corrupt("unknown arena kind"));
        }
        if r.bytes(1)? != [0] {
            return Err(corrupt("unknown model flags"));
        }
        let doc_len = r.u64()?;
        let root = r.u32()?;
        let section_count = r.u32()?;
        let entry_count = r.u32()?;
        if section_count > MAX_MODEL_SECTIONS {
            return Err(corrupt("section count is implausible"));
        }
        if entry_count > MAX_MODEL_ENTRIES {
            return Err(corrupt("entry count is implausible"));
        }
        let mut sections = Vec::with_capacity(section_count as usize);
        for _ in 0..section_count {
            let parent = r.u32()?;
            let role = r.u8()?;
            let kind = r.u8()?;
            if r.bytes(2)? != [0, 0] {
                return Err(corrupt("unknown section flags"));
            }
            let name_node = r.u32()?;
            let node = r.u32()?;
            let first_entry = r.u32()?;
            let entry_count = r.u32()?;
            let name_start = r.u64()?;
            let name_end = r.u64()?;
            let start = r.u64()?;
            let end = r.u64()?;
            if kind > S_ELEMENT || role > ROLE_OTHER {
                return Err(corrupt("unknown section tag"));
            }
            for (lo, hi) in [(name_start, name_end), (start, end)] {
                if lo > hi || hi > doc_len {
                    return Err(corrupt("section span is outside the document"));
                }
            }
            sections.push(PkgSection {
                parent,
                role,
                kind,
                name_node,
                node,
                first_entry,
                entry_count,
                name_start,
                name_end,
                start,
                end,
            });
        }
        let mut entries = Vec::with_capacity(entry_count as usize);
        for _ in 0..entry_count {
            let section = r.u32()?;
            let key_node = r.u32()?;
            let value_node = r.u32()?;
            let value_kind = r.u8()?;
            if r.bytes(3)? != [0, 0, 0] {
                return Err(corrupt("unknown entry flags"));
            }
            let key_start = r.u64()?;
            let key_end = r.u64()?;
            let value_start = r.u64()?;
            let value_end = r.u64()?;
            for (lo, hi) in [(key_start, key_end), (value_start, value_end)] {
                if lo > hi || hi > doc_len {
                    return Err(corrupt("entry span is outside the document"));
                }
            }
            entries.push(PkgEntry {
                section,
                key_node,
                value_node,
                value_kind,
                key_start,
                key_end,
                value_start,
                value_end,
            });
        }
        let arena_len = r.u32()?;
        let arena_bytes = r.bytes(arena_len as usize)?;
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        let (json, toml) = match arena_kind {
            0 => (Some(JsonModel::decode(arena_bytes)?), None),
            _ => (None, Some(TomlModel::decode(arena_bytes)?)),
        };
        let arena_nodes = match (&json, &toml) {
            (Some(j), _) => j.nodes.len() as u32,
            (_, Some(t)) => t.nodes.len() as u32,
            _ => 0,
        };
        let arena_doc_len = match (&json, &toml) {
            (Some(j), _) => j.doc_len,
            (_, Some(t)) => t.doc_len,
            _ => 0,
        };
        if arena_nodes == 0 {
            return Err(corrupt("model has no arena"));
        }
        if is_json_dialect(dialect) != json.is_some() {
            return Err(corrupt("dialect and arena disagree"));
        }
        if arena_doc_len != doc_len {
            return Err(corrupt("model doc_len disagrees with the arena"));
        }
        if root >= arena_nodes {
            return Err(corrupt("root index is out of range"));
        }
        if sections.is_empty() {
            return Err(corrupt("model has no root section"));
        }
        for s in &sections {
            if s.node >= arena_nodes {
                return Err(corrupt("section node is out of range"));
            }
            if s.name_node != NONE && s.name_node >= arena_nodes {
                return Err(corrupt("section name node is out of range"));
            }
            if s.parent != NONE && s.parent as usize >= sections.len() {
                return Err(corrupt("section parent is out of range"));
            }
            let end = s.first_entry as u64 + s.entry_count as u64;
            if end > entries.len() as u64 {
                return Err(corrupt("section entry range is out of range"));
            }
        }
        for e in &entries {
            if e.section as usize >= sections.len()
                || e.key_node >= arena_nodes
                || e.value_node >= arena_nodes
            {
                return Err(corrupt("entry index is out of range"));
            }
        }
        Ok(PkgmetaModel {
            dialect,
            root,
            doc_len,
            sections,
            entries,
            json,
            toml,
        })
    }
}

/// A lexical match from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkgMatch {
    /// The owning section index.
    pub section: u32,
    /// The matching entry index, or [`NONE`] for a section-name match.
    pub entry: u32,
    /// What matched.
    pub role: MatchRole,
    /// The exact source span of the matching construct.
    pub start: u64,
    /// One past the matching construct.
    pub end: u64,
    /// The matched (decoded) text.
    pub text: String,
}

/// Whether a match is a section name, an entry key, or an entry value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchRole {
    /// A section name.
    Section,
    /// An entry key.
    Key,
    /// An entry value.
    Value,
}

impl MatchRole {
    /// Stable lower-case name.
    pub const fn name(self) -> &'static str {
        match self {
            MatchRole::Section => "section",
            MatchRole::Key => "key",
            MatchRole::Value => "value",
        }
    }
}

// ---------------------------------------------------------------------------
// Detection / construction
// ---------------------------------------------------------------------------

/// Byte-based, conservative package-metadata detector. See the module docs for the
/// exact semantic boundary.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.len() as u64 > limits.max_pkgmeta_document_bytes {
        return false;
    }
    parse(source, limits, false).is_ok()
}

/// Parse `source` into a [`PkgmetaModel`]. `build` selects whether the section/entry
/// projections are retained (detection runs with `build = false`). The reused arena is
/// always built because the semantic test walks it.
pub fn parse(source: &[u8], limits: Limits, build: bool) -> Result<PkgmetaModel> {
    if source.len() as u64 > limits.max_pkgmeta_document_bytes {
        return Err(Error::resource_limit(format!(
            "package-metadata source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_pkgmeta_document_bytes
        )));
    }
    // The physical bytes are JSON or TOML. Reuse the shared detectors, then the shared
    // parsers (which enforce their own caps). A resource error is never laundered into
    // a structure error.
    if json::detect(source, limits) {
        let json = json::parse(source, limits, true).map_err(keep_resource)?;
        if json.nodes.len() as u64 > limits.max_pkgmeta_nodes as u64 {
            return Err(Error::resource_limit(format!(
                "package-metadata document exceeds the {}-node cap",
                limits.max_pkgmeta_nodes
            )));
        }
        let dialect = classify_json(&json, source)?;
        return build_json(json, source, dialect, build, limits);
    }
    if toml::detect(source, limits) {
        let toml = toml::parse(source, limits).map_err(keep_resource)?;
        if toml.nodes.len() as u64 > limits.max_pkgmeta_nodes as u64 {
            return Err(Error::resource_limit(format!(
                "package-metadata document exceeds the {}-node cap",
                limits.max_pkgmeta_nodes
            )));
        }
        let dialect = classify_toml(&toml, source)?;
        return build_toml(toml, source, dialect, build, limits);
    }
    Err(invalid_pkgmeta(
        "source is neither a JSON nor a TOML package manifest",
    ))
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of a `PkgmetaModel` node).
pub fn build_pkgmeta_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits, true)?.encode())
}

fn keep_resource(e: Error) -> Error {
    match e.class() {
        ErrorClass::ResourceLimit => e,
        _ => invalid_pkgmeta(format!("source is not a valid manifest: {}", e.message())),
    }
}

fn classify_json(json: &JsonModel, source: &[u8]) -> Result<u8> {
    let root = json.root;
    let rn = json
        .node(root)
        .ok_or_else(|| corrupt("root index is out of range"))?;
    if rn.kind != K_OBJECT {
        return Err(invalid_pkgmeta("a JSON manifest root must be an object"));
    }
    // Lockfiles first: `lockfileVersion` is decisive, and a v2/v3 `package-lock.json`
    // also carries `name`+`version` (so it must never be claimed as an `npm_package`).
    if json_member(json, source, root, "lockfileVersion").is_some()
        && (json_object_member(json, source, root, "packages")
            || json_object_member(json, source, root, "dependencies"))
    {
        return Ok(D_NPM_LOCK);
    }
    if json_string_member(json, source, root, "name")
        && json_string_member(json, source, root, "version")
        && (json_member(json, source, root, "dependencies").is_some()
            || json_member(json, source, root, "devDependencies").is_some()
            || json_member(json, source, root, "scripts").is_some()
            || json_member(json, source, root, "main").is_some()
            || json_member(json, source, root, "engines").is_some())
    {
        return Ok(D_NPM_PACKAGE);
    }
    Err(invalid_pkgmeta(
        "JSON is not a recognized package.json / package-lock.json shape",
    ))
}

fn classify_toml(toml: &TomlModel, source: &[u8]) -> Result<u8> {
    let root = toml.root;
    // Cargo.lock: a top-level `version` and a `[[package]]` array-of-tables whose
    // element tables carry a string `name`.
    if toml_member(toml, source, root, "version").is_some()
        && let Some(pkg) = toml_member(toml, source, root, "package")
        && toml.node(pkg).map(|n| n.kind) == Some(T_ARRAY_TABLE)
        && cargo_lock_elements_ok(toml, source, pkg)?
    {
        return Ok(D_CARGO_LOCK);
    }
    // Cargo.toml: a `[package]` **table** with a string `name` (plus a string
    // `version` or a `[dependencies]`/`[workspace]` table). A bare `name`/`version` is
    // never enough.
    if let Some(pkg) = toml_member(toml, source, root, "package")
        && toml.node(pkg).map(|n| n.kind) == Some(T_TABLE)
        && toml_string_member(toml, source, pkg, "name")
        && (toml_string_member(toml, source, pkg, "version")
            || toml_member(toml, source, root, "dependencies").is_some()
            || toml_member(toml, source, root, "workspace").is_some())
    {
        return Ok(D_CARGO_MANIFEST);
    }
    // pyproject.toml: `[build-system]` with `requires`, `[project]` with
    // `name`+`version`, or `[tool.poetry]`.
    if let Some(bs) = toml_member(toml, source, root, "build-system")
        && toml.node(bs).map(|n| n.kind) == Some(T_TABLE)
        && toml_member(toml, source, bs, "requires").is_some()
    {
        return Ok(D_PYPROJECT_MANIFEST);
    }
    if let Some(project) = toml_member(toml, source, root, "project")
        && toml.node(project).map(|n| n.kind) == Some(T_TABLE)
        && toml_string_member(toml, source, project, "name")
        && toml_string_member(toml, source, project, "version")
    {
        return Ok(D_PYPROJECT_MANIFEST);
    }
    if let Some(tool) = toml_member(toml, source, root, "tool")
        && toml.node(tool).map(|n| n.kind) == Some(T_TABLE)
        && let Some(poetry) = toml_member(toml, source, tool, "poetry")
        && toml.node(poetry).map(|n| n.kind) == Some(T_TABLE)
    {
        return Ok(D_PYPROJECT_MANIFEST);
    }
    Err(invalid_pkgmeta(
        "TOML is not a recognized Cargo.toml / pyproject.toml / Cargo.lock shape",
    ))
}

fn cargo_lock_elements_ok(toml: &TomlModel, source: &[u8], array: u32) -> Result<bool> {
    let node = toml
        .node(array)
        .ok_or_else(|| corrupt("package array index is out of range"))?;
    let Some(&first) = node.children.first() else {
        return Ok(false);
    };
    let el = toml
        .node(first)
        .ok_or_else(|| corrupt("package element is out of range"))?;
    if el.kind != T_TABLE {
        return Ok(false);
    }
    Ok(toml_string_member(toml, source, first, "name"))
}

/// The value node of the first JSON object member named `key`.
fn json_member(json: &JsonModel, source: &[u8], obj: u32, key: &str) -> Option<u32> {
    let node = json.node(obj)?;
    if node.kind != K_OBJECT {
        return None;
    }
    let mut i = 0usize;
    while i + 1 < node.children.len() {
        let k = node.children[i];
        let v = node.children[i + 1];
        i += 2;
        let kn = json.node(k)?;
        if kn.kind == K_STRING && json::decode_string(source, kn).ok().as_deref() == Some(key) {
            return Some(v);
        }
    }
    None
}

/// Whether a first JSON object member named `key` exists and is a string value.
fn json_string_member(json: &JsonModel, source: &[u8], obj: u32, key: &str) -> bool {
    match json_member(json, source, obj, key) {
        Some(v) => json.node(v).map(|n| n.kind == K_STRING).unwrap_or(false),
        None => false,
    }
}

/// Whether a first JSON object member named `key` exists and is an object value.
fn json_object_member(json: &JsonModel, source: &[u8], obj: u32, key: &str) -> bool {
    match json_member(json, source, obj, key) {
        Some(v) => json.node(v).map(|n| n.kind == K_OBJECT).unwrap_or(false),
        None => false,
    }
}

/// The value node of the first TOML entry named `key` in a table-like node.
fn toml_member(toml: &TomlModel, source: &[u8], table: u32, key: &str) -> Option<u32> {
    let node = toml.node(table)?;
    if !toml_is_table_like(node.kind) {
        return None;
    }
    let mut i = 0usize;
    while i + 1 < node.children.len() {
        let k = node.children[i];
        let v = node.children[i + 1];
        i += 2;
        let kn = toml.node(k)?;
        if toml::key_text(source, kn).ok().as_deref() == Some(key) {
            return Some(v);
        }
    }
    None
}

/// Whether a first TOML entry named `key` in `table` exists and is a string value.
fn toml_string_member(toml: &TomlModel, source: &[u8], table: u32, key: &str) -> bool {
    match toml_member(toml, source, table, key) {
        Some(v) => toml
            .node(v)
            .map(|n| toml_is_string(n.kind))
            .unwrap_or(false),
        None => false,
    }
}

// ---------------------------------------------------------------------------
// Section / entry construction (JSON dialects)
// ---------------------------------------------------------------------------

fn build_json(
    json: JsonModel,
    source: &[u8],
    dialect: u8,
    build: bool,
    limits: Limits,
) -> Result<PkgmetaModel> {
    let root = json.root;
    let doc_len = json.doc_len;
    let mut sections: Vec<PkgSection> = Vec::new();
    let mut entries: Vec<PkgEntry> = Vec::new();
    let mut containers: Vec<(u32, u8)> = Vec::new(); // (node, arena kind)
    let rn = json
        .node(root)
        .ok_or_else(|| corrupt("root index is out of range"))?;
    sections.push(PkgSection {
        parent: NONE,
        role: ROLE_ROOT,
        kind: S_ROOT,
        name_node: NONE,
        node: root,
        first_entry: 0,
        entry_count: 0,
        name_start: 0,
        name_end: 0,
        start: rn.start,
        end: rn.end,
    });
    containers.push((root, rn.kind));
    // The section/entry projection is always built so the caps are enforced during
    // detection too; it is discarded when `build` is false.
    collect_json(
        &json,
        source,
        dialect,
        limits,
        0,
        &mut sections,
        &mut containers,
        1,
    )?;
    build_entries_json(&json, limits, &containers, &mut sections, &mut entries)?;
    if !build {
        sections.clear();
        entries.clear();
    }
    Ok(PkgmetaModel {
        dialect,
        root,
        doc_len,
        sections,
        entries,
        json: Some(json),
        toml: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn collect_json(
    json: &JsonModel,
    source: &[u8],
    dialect: u8,
    limits: Limits,
    parent: u32,
    sections: &mut Vec<PkgSection>,
    containers: &mut Vec<(u32, u8)>,
    depth: u32,
) -> Result<()> {
    if depth > limits.max_pkgmeta_depth {
        return Err(Error::resource_limit(format!(
            "package-metadata nesting exceeds the {}-level cap",
            limits.max_pkgmeta_depth
        )));
    }
    let (node_index, _) = *containers
        .get(parent as usize)
        .ok_or_else(|| corrupt("parent section is out of range"))?;
    let node = json
        .node(node_index)
        .ok_or_else(|| corrupt("container node is out of range"))?;
    match node.kind {
        K_OBJECT => {
            let mut i = 0usize;
            while i + 1 < node.children.len() {
                let k = node.children[i];
                let v = node.children[i + 1];
                i += 2;
                let vn = json
                    .node(v)
                    .ok_or_else(|| corrupt("member value index is out of range"))?;
                if matches!(vn.kind, K_OBJECT | K_ARRAY) {
                    let kn = json
                        .node(k)
                        .ok_or_else(|| corrupt("member key index is out of range"))?;
                    let name = json::decode_string(source, kn)?;
                    let skind = if vn.kind == K_OBJECT {
                        S_TABLE
                    } else {
                        S_ARRAY
                    };
                    let child = push_section(
                        sections,
                        containers,
                        limits,
                        parent,
                        role_of(dialect, &name),
                        skind,
                        k,
                        v,
                        vn.kind,
                        vn.start,
                        vn.end,
                        kn.start,
                        kn.end,
                    )?;
                    collect_json(
                        json,
                        source,
                        dialect,
                        limits,
                        child,
                        sections,
                        containers,
                        depth + 1,
                    )?;
                }
            }
        }
        K_ARRAY => {
            for &el in &node.children {
                let en = json
                    .node(el)
                    .ok_or_else(|| corrupt("array element index is out of range"))?;
                if matches!(en.kind, K_OBJECT | K_ARRAY) {
                    let skind = if en.kind == K_OBJECT {
                        S_ELEMENT
                    } else {
                        S_ARRAY
                    };
                    let child = push_section(
                        sections, containers, limits, parent, ROLE_OTHER, skind, NONE, el, en.kind,
                        en.start, en.end, 0, 0,
                    )?;
                    collect_json(
                        json,
                        source,
                        dialect,
                        limits,
                        child,
                        sections,
                        containers,
                        depth + 1,
                    )?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn build_entries_json(
    json: &JsonModel,
    limits: Limits,
    containers: &[(u32, u8)],
    sections: &mut [PkgSection],
    entries: &mut Vec<PkgEntry>,
) -> Result<()> {
    for si in 0..sections.len() {
        let (node_index, node_kind) = containers[si];
        let first = entries.len() as u32;
        let mut count = 0u32;
        if node_kind == K_OBJECT {
            let n = json
                .node(node_index)
                .ok_or_else(|| corrupt("section node is out of range"))?;
            let mut i = 0usize;
            while i + 1 < n.children.len() {
                let k = n.children[i];
                let v = n.children[i + 1];
                i += 2;
                if entries.len() as u64 >= limits.max_pkgmeta_entries as u64 {
                    return Err(Error::resource_limit(format!(
                        "package-metadata document exceeds the {}-entry cap",
                        limits.max_pkgmeta_entries
                    )));
                }
                let kn = json
                    .node(k)
                    .ok_or_else(|| corrupt("key index is out of range"))?;
                let vn = json
                    .node(v)
                    .ok_or_else(|| corrupt("value index is out of range"))?;
                entries.push(PkgEntry {
                    section: si as u32,
                    key_node: k,
                    value_node: v,
                    value_kind: vn.kind,
                    key_start: kn.start,
                    key_end: kn.end,
                    value_start: vn.start,
                    value_end: vn.end,
                });
                count += 1;
            }
        }
        sections[si].first_entry = first;
        sections[si].entry_count = count;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Section / entry construction (TOML dialects)
// ---------------------------------------------------------------------------

fn build_toml(
    toml: TomlModel,
    source: &[u8],
    dialect: u8,
    build: bool,
    limits: Limits,
) -> Result<PkgmetaModel> {
    let root = toml.root;
    let doc_len = toml.doc_len;
    let mut sections: Vec<PkgSection> = Vec::new();
    let mut entries: Vec<PkgEntry> = Vec::new();
    let mut containers: Vec<(u32, u8)> = Vec::new(); // (node, arena kind)
    let rn = toml
        .node(root)
        .ok_or_else(|| corrupt("root index is out of range"))?;
    sections.push(PkgSection {
        parent: NONE,
        role: ROLE_ROOT,
        kind: S_ROOT,
        name_node: NONE,
        node: root,
        first_entry: 0,
        entry_count: 0,
        name_start: 0,
        name_end: 0,
        start: rn.start,
        end: rn.end,
    });
    containers.push((root, rn.kind));
    // The section/entry projection is always built so the caps are enforced during
    // detection too; it is discarded when `build` is false.
    collect_toml(
        &toml,
        source,
        dialect,
        limits,
        0,
        &mut sections,
        &mut containers,
        1,
    )?;
    build_entries_toml(&toml, limits, &containers, &mut sections, &mut entries)?;
    if !build {
        sections.clear();
        entries.clear();
    }
    Ok(PkgmetaModel {
        dialect,
        root,
        doc_len,
        sections,
        entries,
        json: None,
        toml: Some(toml),
    })
}

#[allow(clippy::too_many_arguments)]
fn collect_toml(
    toml: &TomlModel,
    source: &[u8],
    dialect: u8,
    limits: Limits,
    parent: u32,
    sections: &mut Vec<PkgSection>,
    containers: &mut Vec<(u32, u8)>,
    depth: u32,
) -> Result<()> {
    if depth > limits.max_pkgmeta_depth {
        return Err(Error::resource_limit(format!(
            "package-metadata nesting exceeds the {}-level cap",
            limits.max_pkgmeta_depth
        )));
    }
    let (node_index, node_kind) = *containers
        .get(parent as usize)
        .ok_or_else(|| corrupt("parent section is out of range"))?;
    let n = toml
        .node(node_index)
        .ok_or_else(|| corrupt("container node is out of range"))?;
    match node_kind {
        T_ROOT | T_TABLE | T_INLINE_TABLE => {
            let mut i = 0usize;
            while i + 1 < n.children.len() {
                let k = n.children[i];
                let v = n.children[i + 1];
                i += 2;
                let vn = toml
                    .node(v)
                    .ok_or_else(|| corrupt("member value index is out of range"))?;
                let skind = match vn.kind {
                    T_TABLE | T_INLINE_TABLE => Some(S_TABLE),
                    T_ARRAY_TABLE | T_ARRAY => Some(S_ARRAY),
                    _ => None,
                };
                if let Some(skind) = skind {
                    let kn = toml
                        .node(k)
                        .ok_or_else(|| corrupt("member key index is out of range"))?;
                    let name = toml::key_text(source, kn)?;
                    let child = push_section(
                        sections,
                        containers,
                        limits,
                        parent,
                        role_of(dialect, &name),
                        skind,
                        k,
                        v,
                        vn.kind,
                        vn.start,
                        vn.end,
                        kn.start,
                        kn.end,
                    )?;
                    collect_toml(
                        toml,
                        source,
                        dialect,
                        limits,
                        child,
                        sections,
                        containers,
                        depth + 1,
                    )?;
                }
            }
        }
        T_ARRAY_TABLE | T_ARRAY => {
            for &el in &n.children {
                let en = toml
                    .node(el)
                    .ok_or_else(|| corrupt("array element index is out of range"))?;
                if matches!(en.kind, T_TABLE | T_INLINE_TABLE) {
                    let child = push_section(
                        sections, containers, limits, parent, ROLE_OTHER, S_ELEMENT, NONE, el,
                        en.kind, en.start, en.end, 0, 0,
                    )?;
                    collect_toml(
                        toml,
                        source,
                        dialect,
                        limits,
                        child,
                        sections,
                        containers,
                        depth + 1,
                    )?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn build_entries_toml(
    toml: &TomlModel,
    limits: Limits,
    containers: &[(u32, u8)],
    sections: &mut [PkgSection],
    entries: &mut Vec<PkgEntry>,
) -> Result<()> {
    for si in 0..sections.len() {
        let (node_index, node_kind) = containers[si];
        let first = entries.len() as u32;
        let mut count = 0u32;
        if toml_is_table_like(node_kind) {
            let n = toml
                .node(node_index)
                .ok_or_else(|| corrupt("section node is out of range"))?;
            let mut i = 0usize;
            while i + 1 < n.children.len() {
                let k = n.children[i];
                let v = n.children[i + 1];
                i += 2;
                if entries.len() as u64 >= limits.max_pkgmeta_entries as u64 {
                    return Err(Error::resource_limit(format!(
                        "package-metadata document exceeds the {}-entry cap",
                        limits.max_pkgmeta_entries
                    )));
                }
                let kn = toml
                    .node(k)
                    .ok_or_else(|| corrupt("key index is out of range"))?;
                let vn = toml
                    .node(v)
                    .ok_or_else(|| corrupt("value index is out of range"))?;
                entries.push(PkgEntry {
                    section: si as u32,
                    key_node: k,
                    value_node: v,
                    value_kind: vn.kind,
                    key_start: kn.start,
                    key_end: kn.end,
                    value_start: vn.start,
                    value_end: vn.end,
                });
                count += 1;
            }
        }
        sections[si].first_entry = first;
        sections[si].entry_count = count;
    }
    Ok(())
}

/// Append a new section and return its index.
#[allow(clippy::too_many_arguments)]
fn push_section(
    sections: &mut Vec<PkgSection>,
    containers: &mut Vec<(u32, u8)>,
    limits: Limits,
    parent: u32,
    role: u8,
    kind: u8,
    name_node: u32,
    node: u32,
    node_kind: u8,
    start: u64,
    end: u64,
    name_start: u64,
    name_end: u64,
) -> Result<u32> {
    if sections.len() as u64 >= limits.max_pkgmeta_sections as u64 {
        return Err(Error::resource_limit(format!(
            "package-metadata document exceeds the {}-section cap",
            limits.max_pkgmeta_sections
        )));
    }
    let idx = sections.len() as u32;
    sections.push(PkgSection {
        parent,
        role,
        kind,
        name_node,
        node,
        first_entry: 0,
        entry_count: 0,
        name_start,
        name_end,
        start,
        end,
    });
    containers.push((node, node_kind));
    Ok(idx)
}

// ---------------------------------------------------------------------------
// Public accessors over the embedded arena
// ---------------------------------------------------------------------------

/// The decoded text of a key node in the embedded arena (JSON string or TOML key).
pub fn key_text(model: &PkgmetaModel, source: &[u8], key_node: u32) -> Result<String> {
    match (&model.json, &model.toml) {
        (Some(j), _) => {
            let n = j
                .node(key_node)
                .ok_or_else(|| corrupt("key node is out of range"))?;
            json::decode_string(source, n)
        }
        (_, Some(t)) => {
            let n = t
                .node(key_node)
                .ok_or_else(|| corrupt("key node is out of range"))?;
            toml::key_text(source, n)
        }
        _ => Err(corrupt("model has no arena")),
    }
}

/// The recorded (decoded) name of a section; the empty string when unnamed.
pub fn section_name(model: &PkgmetaModel, source: &[u8], section: &PkgSection) -> Result<String> {
    if section.name_node == NONE {
        return Ok(String::new());
    }
    key_text(model, source, section.name_node)
}

/// The exact source bytes of a section's value/table span.
pub fn section_bytes<'a>(
    model: &PkgmetaModel,
    source: &'a [u8],
    section: &PkgSection,
) -> Result<&'a [u8]> {
    let _ = model;
    slice(source, section.start, section.end)
}

/// The exact source bytes of an entry's value span.
pub fn entry_value_bytes<'a>(
    model: &PkgmetaModel,
    source: &'a [u8],
    entry: &PkgEntry,
) -> Result<&'a [u8]> {
    let _ = model;
    slice(source, entry.value_start, entry.value_end)
}

/// The decoded text of an entry's key.
pub fn entry_key_text(model: &PkgmetaModel, source: &[u8], entry: &PkgEntry) -> Result<String> {
    key_text(model, source, entry.key_node)
}

/// Whether an entry's value is a string scalar in the arena's kind namespace.
pub fn entry_value_is_string(model: &PkgmetaModel, entry: &PkgEntry) -> bool {
    match (&model.json, &model.toml) {
        (Some(_), _) => entry.value_kind == K_STRING,
        (_, Some(_)) => toml_is_string(entry.value_kind),
        _ => false,
    }
}

/// The decoded text of an entry's value: the decoded string for a string scalar, else
/// the exact token bytes rendered lossily.
pub fn entry_value_text(model: &PkgmetaModel, source: &[u8], entry: &PkgEntry) -> Result<String> {
    match (&model.json, &model.toml) {
        (Some(j), _) if entry.value_kind == K_STRING => {
            let n = j
                .node(entry.value_node)
                .ok_or_else(|| corrupt("value node is out of range"))?;
            json::decode_string(source, n)
        }
        (_, Some(t)) if toml_is_string(entry.value_kind) => {
            let n = t
                .node(entry.value_node)
                .ok_or_else(|| corrupt("value node is out of range"))?;
            toml::string_content(source, n)
        }
        _ => {
            let bytes = slice(source, entry.value_start, entry.value_end)?;
            Ok(String::from_utf8_lossy(bytes).into_owned())
        }
    }
}

/// The stable kind name of an entry's value in the embedded arena's kind namespace.
pub fn entry_value_kind_name(model: &PkgmetaModel, entry: &PkgEntry) -> &'static str {
    match (&model.json, &model.toml) {
        (Some(_), _) => json::kind_name(entry.value_kind),
        (_, Some(_)) => toml::kind_name(entry.value_kind),
        _ => "unknown",
    }
}

/// A bounded, case-sensitive lexical search across section names, entry keys, and
/// entry value texts, in document order.
pub fn find(
    model: &PkgmetaModel,
    source: &[u8],
    pattern: &str,
    limits: Limits,
) -> Result<Vec<PkgMatch>> {
    let cap = limits.max_pkgmeta_entries as u64;
    let mut out: Vec<PkgMatch> = Vec::new();
    for (si, s) in model.sections.iter().enumerate() {
        let name = section_name(model, source, s)?;
        if !name.is_empty() && name.contains(pattern) {
            out.push(PkgMatch {
                section: si as u32,
                entry: NONE,
                role: MatchRole::Section,
                start: s.name_start,
                end: s.name_end,
                text: name,
            });
        }
        if out.len() as u64 > cap {
            return Err(find_cap(cap));
        }
    }
    for (ei, e) in model.entries.iter().enumerate() {
        let key = entry_key_text(model, source, e)?;
        let key_hit = key.contains(pattern);
        if key_hit {
            out.push(PkgMatch {
                section: e.section,
                entry: ei as u32,
                role: MatchRole::Key,
                start: e.key_start,
                end: e.key_end,
                text: key,
            });
        }
        let value = entry_value_text(model, source, e)?;
        if !key_hit && value.contains(pattern) {
            out.push(PkgMatch {
                section: e.section,
                entry: ei as u32,
                role: MatchRole::Value,
                start: e.value_start,
                end: e.value_end,
                text: value,
            });
        }
        if out.len() as u64 > cap {
            return Err(find_cap(cap));
        }
    }
    Ok(out)
}

fn find_cap(cap: u64) -> Error {
    Error::resource_limit(format!("package-metadata find exceeds the {cap}-match cap"))
}

/// A deterministic text projection: the manifest's exact source text (lossily
/// decoded). Nothing is normalized or re-serialized.
pub fn canonical_text(_model: &PkgmetaModel, source: &[u8]) -> Result<String> {
    Ok(String::from_utf8_lossy(source).into_owned())
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn corrupt(msg: &str) -> Error {
    Error::invalid_model(format!("corrupt package-metadata model: {msg}"))
}

fn invalid_pkgmeta(msg: impl Into<String>) -> Error {
    Error::invalid_pkgmeta_structure(msg)
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

    const NPM: &[u8] = br#"{"name":"demo","version":"1.0.0","scripts":{"build":"tsc"},"dependencies":{"react":"^18.0.0"}}"#;
    const CARGO: &[u8] = b"[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[dependencies]\nserde = { version = \"1\", features = [\"derive\"] }\n";
    const PYPROJECT: &[u8] =
        b"[build-system]\nrequires = [\"setuptools>=61\"]\n\n[project]\nname = \"demo\"\nversion = \"1.0.0\"\n";
    const CARGO_LOCK: &[u8] = b"version = 3\n\n[[package]]\nname = \"demo\"\nversion = \"0.1.0\"\n";
    const NPM_LOCK: &[u8] =
        br#"{"name":"demo","version":"1.0.0","lockfileVersion":3,"packages":{"":{"name":"demo"}}}"#;

    #[test]
    fn detects_each_dialect_and_rejects_generic() {
        assert_eq!(
            parse(NPM, Limits::DEFAULT, false).unwrap().dialect,
            D_NPM_PACKAGE
        );
        assert_eq!(
            parse(CARGO, Limits::DEFAULT, false).unwrap().dialect,
            D_CARGO_MANIFEST
        );
        assert_eq!(
            parse(PYPROJECT, Limits::DEFAULT, false).unwrap().dialect,
            D_PYPROJECT_MANIFEST
        );
        assert_eq!(
            parse(CARGO_LOCK, Limits::DEFAULT, false).unwrap().dialect,
            D_CARGO_LOCK
        );
        assert_eq!(
            parse(NPM_LOCK, Limits::DEFAULT, false).unwrap().dialect,
            D_NPM_LOCK
        );
        // Generic JSON/TOML are not claimed.
        assert!(!detect(
            br#"{"name":"x","version":"1.0.0"}"#,
            Limits::DEFAULT
        ));
        assert!(!detect(
            b"name = \"x\"\nversion = \"1.0.0\"\n",
            Limits::DEFAULT
        ));
        // Prose is not claimed.
        assert!(!detect(b"just some prose\n", Limits::DEFAULT));
    }

    #[test]
    fn preserves_spans_order_and_duplicates() {
        let src = br#"{"name":"demo","version":"1.0.0","dependencies":{"react":"^18.0.0","react":"^19.0.0"}}"#;
        let m = parse(src, Limits::DEFAULT, true).unwrap();
        // The duplicate `react` member is preserved.
        let dep_entries: Vec<&PkgEntry> = m
            .entries
            .iter()
            .filter(|e| {
                m.section(e.section)
                    .map(|s| s.kind == S_TABLE)
                    .unwrap_or(false)
            })
            .collect();
        assert_eq!(dep_entries.len(), 2);
        let texts: Vec<String> = dep_entries
            .iter()
            .map(|e| String::from_utf8_lossy(entry_value_bytes(&m, src, e).unwrap()).into_owned())
            .collect();
        assert_eq!(texts, vec!["\"^18.0.0\"", "\"^19.0.0\""]);
    }

    #[test]
    fn roundtrips_and_rejects_corruption() {
        let m = parse(CARGO, Limits::DEFAULT, true).unwrap();
        let enc = m.encode();
        let dec = PkgmetaModel::decode(&enc).unwrap();
        assert_eq!(dec.dialect, m.dialect);
        assert_eq!(dec.sections, m.sections);
        assert_eq!(dec.entries, m.entries);
        // Flip a byte and require a typed decline.
        let mut bad = enc.clone();
        let n = bad.len();
        bad[n - 1] ^= 0xFF;
        assert!(PkgmetaModel::decode(&bad).is_err());
    }

    #[test]
    fn caps_decline_typed() {
        let tight = Limits {
            max_pkgmeta_sections: 1,
            ..Limits::DEFAULT
        };
        assert_eq!(
            parse(CARGO, tight, true).unwrap_err().class(),
            ErrorClass::ResourceLimit
        );
        let tight = Limits {
            max_pkgmeta_entries: 1,
            ..Limits::DEFAULT
        };
        assert_eq!(
            parse(CARGO, tight, true).unwrap_err().class(),
            ErrorClass::ResourceLimit
        );
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        for _ in 0..64 {
            let mut buf = vec![0u8; 256];
            for b in buf.iter_mut() {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                *b = (x & 0xFF) as u8;
            }
            let _ = detect(&buf, Limits::STRICT);
            let _ = parse(&buf, Limits::STRICT, true);
            let _ = build_pkgmeta_model(&buf, Limits::STRICT);
            let _ = PkgmetaModel::decode(&buf);
        }
    }
}
