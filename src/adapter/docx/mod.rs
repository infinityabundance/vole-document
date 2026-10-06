//! WordprocessingML (DOCX) adapter (Phase 12.4, ADR-0032).
//!
//! A `.docx` is an OPC package on the ZIP layer, and the main part is identified
//! **semantically** by the package `officeDocument` relationship — never by a
//! hardcoded `/word/document.xml`. This module adds the WordprocessingML
//! semantics on top of the generic 12.3 OPC core:
//!
//! * **main-part discovery** (relationship → content type → later root element),
//! * a **declared versioned extraction profile** (tracked changes Final/Original,
//!   field result vs code, hidden text, tabs/breaks),
//! * **explicit story scoping** (`Main`, `Header{n}`, `Footer{n}`, `Footnote{id}`,
//!   `Endnote{id}`, `Comment{id}`; `TextBox` is declared but declined), and
//! * a bounded WordprocessingML subset parsed into a canonical, derived
//!   ([`StoryModel`]) serialization.
//!
//! Everything here is **derived (`Q_gen`) state**: exact authority stays with the
//! Phase-12.2 ZIP member raw spans, and the exact original package still
//! materializes byte-identically regardless of any decline. The parsed model
//! deliberately omits everything it does not model (unknown namespaces are
//! preserved in the exact bytes and simply not interpreted).

pub mod wml;

use std::collections::BTreeMap;

use crate::adapter::package::opc::{
    OFFICE_DOCUMENT_REL, OFFICE_DOCUMENT_REL_STRICT, OpcModel, OpcPart,
};
use crate::error::{Error, Result};
use crate::limits::Limits;

/// Story kind: the main document part.
const KIND_MAIN: u8 = 0;
/// Story kind: a header part.
const KIND_HEADER: u8 = 1;
/// Story kind: a footer part.
const KIND_FOOTER: u8 = 2;
/// Story kind: the footnotes part (individual notes are selected by id).
const KIND_FOOTNOTES: u8 = 3;
/// Story kind: the endnotes part.
const KIND_ENDNOTES: u8 = 4;
/// Story kind: the comments part.
const KIND_COMMENTS: u8 = 5;
/// Story kind: a text box (declared; not part-backed, so declined).
pub const KIND_TEXTBOX: u8 = 6;

/// Version of the DOCX extraction profile semantics.
pub const DOCX_EXTRACT_PROFILE_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Extraction profile
// ---------------------------------------------------------------------------

/// How tracked changes (`w:ins`/`w:del`) are resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackedChanges {
    /// Accept every revision: insertions kept, deletions dropped.
    Final,
    /// Reject every revision: insertions dropped, deletions kept.
    Original,
    /// Keep both, in document order.
    All,
}

impl TrackedChanges {
    /// Stable lower-case name.
    pub const fn name(self) -> &'static str {
        match self {
            TrackedChanges::Final => "final",
            TrackedChanges::Original => "original",
            TrackedChanges::All => "all",
        }
    }
}

/// How a field (`w:fldSimple`, complex fields) is projected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldMode {
    /// The cached field result.
    Result,
    /// The field instruction code.
    Code,
    /// Both, in encounter order.
    Both,
}

impl FieldMode {
    /// Stable lower-case name.
    pub const fn name(self) -> &'static str {
        match self {
            FieldMode::Result => "result",
            FieldMode::Code => "code",
            FieldMode::Both => "both",
        }
    }
}

/// A versioned, explicit text-extraction profile. The profile identity is
/// recorded in every answer (and hashed into the canonical selector), so a
/// projection is never hidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocxExtractProfile {
    /// Profile semantics version; must equal [`DOCX_EXTRACT_PROFILE_VERSION`].
    pub version: u32,
    /// Tracked-changes resolution.
    pub tracked: TrackedChanges,
    /// Field projection.
    pub fields: FieldMode,
    /// Include header stories in a whole-document projection.
    pub include_headers: bool,
    /// Include footnote stories.
    pub include_footnotes: bool,
    /// Include endnote stories.
    pub include_endnotes: bool,
    /// Include comment stories.
    pub include_comments: bool,
    /// Include `w:vanish` (hidden) run text.
    pub hidden: bool,
    /// Render `w:tab` as `\t`.
    pub tabs: bool,
    /// Render `w:br`/`w:cr` as `\n`.
    pub breaks: bool,
}

impl DocxExtractProfile {
    /// The declared default profile: revisions accepted, field result, notes and
    /// headers included, hidden text excluded, tabs and breaks rendered.
    pub const DEFAULT: DocxExtractProfile = DocxExtractProfile {
        version: DOCX_EXTRACT_PROFILE_VERSION,
        tracked: TrackedChanges::Final,
        fields: FieldMode::Result,
        include_headers: true,
        include_footnotes: true,
        include_endnotes: true,
        include_comments: true,
        hidden: false,
        tabs: true,
        breaks: true,
    };

    /// A stable, human-readable profile fingerprint used in canonical selectors.
    pub fn fingerprint(&self) -> String {
        format!(
            "v{}-{}-{}-h{}-f{}-e{}-c{}-x{}-t{}-b{}",
            self.version,
            self.tracked.name(),
            self.fields.name(),
            self.include_headers as u8,
            self.include_footnotes as u8,
            self.include_endnotes as u8,
            self.include_comments as u8,
            self.hidden as u8,
            self.tabs as u8,
            self.breaks as u8,
        )
    }

    /// The canonical 10-byte profile block (version is implicit in the node).
    pub fn encode(&self) -> [u8; 10] {
        [
            self.version as u8,
            match self.tracked {
                TrackedChanges::Final => 0,
                TrackedChanges::Original => 1,
                TrackedChanges::All => 2,
            },
            match self.fields {
                FieldMode::Result => 0,
                FieldMode::Code => 1,
                FieldMode::Both => 2,
            },
            self.include_headers as u8,
            self.include_footnotes as u8,
            self.include_endnotes as u8,
            self.include_comments as u8,
            self.hidden as u8,
            self.tabs as u8,
            self.breaks as u8,
        ]
    }

    /// Decode a profile block produced by [`Self::encode`].
    pub fn decode(b: &[u8]) -> Result<DocxExtractProfile> {
        if b.len() != 10 {
            return Err(corrupt("DOCX profile must be 10 bytes"));
        }
        if b[0] as u32 != DOCX_EXTRACT_PROFILE_VERSION {
            return Err(Error::unsupported_version(format!(
                "DOCX extraction profile version {} is not supported",
                b[0]
            )));
        }
        let tracked = match b[1] {
            0 => TrackedChanges::Final,
            1 => TrackedChanges::Original,
            2 => TrackedChanges::All,
            _ => return Err(corrupt("bad tracked-changes selector")),
        };
        let fields = match b[2] {
            0 => FieldMode::Result,
            1 => FieldMode::Code,
            2 => FieldMode::Both,
            _ => return Err(corrupt("bad field selector")),
        };
        Ok(DocxExtractProfile {
            version: b[0] as u32,
            tracked,
            fields,
            include_headers: b[3] != 0,
            include_footnotes: b[4] != 0,
            include_endnotes: b[5] != 0,
            include_comments: b[6] != 0,
            hidden: b[7] != 0,
            tabs: b[8] != 0,
            breaks: b[9] != 0,
        })
    }
}

impl Default for DocxExtractProfile {
    fn default() -> Self {
        DocxExtractProfile::DEFAULT
    }
}

// ---------------------------------------------------------------------------
// Stories
// ---------------------------------------------------------------------------

/// An explicitly named WordprocessingML story. Stories are never silently mixed:
/// every text/find observation is scoped to exactly one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocxStory {
    /// The main document story.
    Main,
    /// The `n`-th header (0-based, ordered by part name).
    Header(u32),
    /// The `n`-th footer.
    Footer(u32),
    /// The footnote with the given `w:id`.
    Footnote(u32),
    /// The endnote with the given `w:id`.
    Endnote(u32),
    /// The comment with the given `w:id`.
    Comment(u32),
    /// A text box (declared; not separately part-backed, so declined).
    TextBox(u32),
}

impl DocxStory {
    /// The story kind byte and the part-level index, or `None` when the story is
    /// declared but not part-backed (`TextBox`).
    pub const fn kind_index(self) -> Option<(u8, u32)> {
        Some(match self {
            DocxStory::Main => (KIND_MAIN, 0),
            DocxStory::Header(n) => (KIND_HEADER, n),
            DocxStory::Footer(n) => (KIND_FOOTER, n),
            DocxStory::Footnote(_) => (KIND_FOOTNOTES, 0),
            DocxStory::Endnote(_) => (KIND_ENDNOTES, 0),
            DocxStory::Comment(_) => (KIND_COMMENTS, 0),
            DocxStory::TextBox(_) => return None,
        })
    }

    /// The note id this story selects inside a notes/comments part, if any.
    pub const fn note_id(self) -> Option<u32> {
        match self {
            DocxStory::Footnote(id) | DocxStory::Endnote(id) | DocxStory::Comment(id) => Some(id),
            _ => None,
        }
    }

    /// The expected root element local name for this story's part.
    pub const fn expected_root(self) -> &'static str {
        match self {
            DocxStory::Main => "document",
            DocxStory::Header(_) => "hdr",
            DocxStory::Footer(_) => "ftr",
            DocxStory::Footnote(_) => "footnotes",
            DocxStory::Endnote(_) => "endnotes",
            DocxStory::Comment(_) => "comments",
            DocxStory::TextBox(_) => "txbxContent",
        }
    }

    /// Stable display name (used in canonical selectors).
    pub fn name(self) -> String {
        match self {
            DocxStory::Main => "main".to_string(),
            DocxStory::Header(n) => format!("header:{n}"),
            DocxStory::Footer(n) => format!("footer:{n}"),
            DocxStory::Footnote(id) => format!("footnote:{id}"),
            DocxStory::Endnote(id) => format!("endnote:{id}"),
            DocxStory::Comment(id) => format!("comment:{id}"),
            DocxStory::TextBox(n) => format!("textbox:{n}"),
        }
    }

    /// The story tag byte used in node parameters.
    const fn tag(self) -> u8 {
        match self {
            DocxStory::Main => 0,
            DocxStory::Header(_) => 1,
            DocxStory::Footer(_) => 2,
            DocxStory::Footnote(_) => 3,
            DocxStory::Endnote(_) => 4,
            DocxStory::Comment(_) => 5,
            DocxStory::TextBox(_) => KIND_TEXTBOX,
        }
    }

    fn from_tag(tag: u8, index: u32) -> Result<DocxStory> {
        Ok(match tag {
            0 => DocxStory::Main,
            1 => DocxStory::Header(index),
            2 => DocxStory::Footer(index),
            3 => DocxStory::Footnote(index),
            4 => DocxStory::Endnote(index),
            5 => DocxStory::Comment(index),
            KIND_TEXTBOX => DocxStory::TextBox(index),
            _ => return Err(corrupt("bad story tag")),
        })
    }
}

/// A part that backs a story.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocxPartRef {
    /// Absolute OPC part name.
    pub name: String,
    /// Physical member ordinal.
    pub ordinal: u32,
    /// Content type, when known.
    pub content_type: Option<String>,
}

/// One discovered story and the part that backs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocxStoryPart {
    /// Story kind byte.
    pub kind: u8,
    /// Part-level index (header/footer position; `0` otherwise).
    pub index: u32,
    /// The backing part.
    pub part: DocxPartRef,
}

/// The canonical DOCX discovery model: main part, styles part, and stories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocxModel {
    /// The main document part (resolved via the `officeDocument` relationship).
    pub main: DocxPartRef,
    /// The styles part, when present (used to resolve heading identity).
    pub styles: Option<DocxPartRef>,
    /// Discovered story parts, sorted by `(kind, index)`.
    pub stories: Vec<DocxStoryPart>,
}

impl DocxModel {
    /// The part backing a story, when discovered.
    pub fn story_part(&self, story: DocxStory) -> Option<&DocxPartRef> {
        let (kind, index) = story.kind_index()?;
        self.stories
            .iter()
            .find(|s| s.kind == kind && s.index == index)
            .map(|s| &s.part)
    }

    /// Encode the discovery model canonically.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"DCXM");
        out.push(1);
        put_part(&mut out, &self.main);
        match &self.styles {
            Some(p) => {
                out.push(1);
                put_part(&mut out, p);
            }
            None => out.push(0),
        }
        put_u32(&mut out, self.stories.len() as u32);
        for s in &self.stories {
            out.push(s.kind);
            put_u32(&mut out, s.index);
            put_part(&mut out, &s.part);
        }
        out
    }

    /// Decode a model produced by [`DocxModel::encode`].
    pub fn decode(bytes: &[u8]) -> Result<DocxModel> {
        let mut r = ByteReader::new(bytes);
        if r.bytes(4)? != b"DCXM" {
            return Err(corrupt("bad DOCX model magic"));
        }
        if r.u8()? != 1 {
            return Err(corrupt("unsupported DOCX model version"));
        }
        let main = read_part(&mut r)?;
        let styles = if r.u8()? == 0 {
            None
        } else {
            Some(read_part(&mut r)?)
        };
        let n = r.u32()?;
        let mut stories = Vec::new();
        for _ in 0..n {
            let kind = r.u8()?;
            let index = r.u32()?;
            let part = read_part(&mut r)?;
            stories.push(DocxStoryPart { kind, index, part });
        }
        if !r.at_end() {
            return Err(corrupt("DOCX model has trailing bytes"));
        }
        Ok(DocxModel {
            main,
            styles,
            stories,
        })
    }
}

fn put_part(out: &mut Vec<u8>, p: &DocxPartRef) {
    put_str(out, &p.name);
    put_u32(out, p.ordinal);
    put_opt_str(out, p.content_type.as_deref());
}

fn read_part(r: &mut ByteReader<'_>) -> Result<DocxPartRef> {
    Ok(DocxPartRef {
        name: r.string()?,
        ordinal: r.u32()?,
        content_type: r.opt_string()?,
    })
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

fn rel_matches(rel_type: &str, suffix: &str) -> bool {
    rel_type == suffix || rel_type.ends_with(&format!("/{suffix}"))
}

fn main_relationship_types() -> [&'static str; 2] {
    [OFFICE_DOCUMENT_REL, OFFICE_DOCUMENT_REL_STRICT]
}

fn is_main_content_type(ct: &str) -> bool {
    matches!(
        ct,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"
            | "application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml"
            | "application/vnd.ms-word.document.macroEnabled.main+xml"
            | "application/vnd.ms-word.template.macroEnabledTemplate.main+xml"
    )
}

/// Discover the main part by relationship (package `_rels/.rels`), failing closed
/// on zero, ambiguous, external, or non-part targets — never a hardcoded path.
fn find_main(model: &OpcModel) -> Result<&OpcPart> {
    let types = main_relationship_types();
    let mut matched: u32 = 0;
    let mut target: Option<&OpcPart> = None;
    for r in &model.package_rels {
        if !types.contains(&r.rel_type.as_str()) {
            continue;
        }
        matched += 1;
        let resolved = r.resolved.as_deref().ok_or_else(|| {
            Error::invalid_package_structure(format!(
                "officeDocument relationship {:?} has an external/unresolved target",
                r.id
            ))
        })?;
        let part = model.part_by_name(resolved).ok_or_else(|| {
            Error::invalid_package_structure(format!(
                "officeDocument relationship {:?} targets {resolved:?}, which is not a package part",
                r.id
            ))
        })?;
        if let Some(prev) = target
            && !prev.name.eq_ignore_ascii_case(&part.name)
        {
            return Err(Error::invalid_package_structure(
                "more than one distinct officeDocument target part is ambiguous",
            ));
        }
        target = Some(part);
    }
    if matched == 0 {
        return Err(Error::invalid_package_structure(
            "package has no officeDocument relationship (not a DOCX main part)",
        ));
    }
    let part = target.ok_or_else(|| {
        Error::invalid_package_structure("officeDocument relationship has no internal target")
    })?;
    let ct = part.content_type.as_deref().unwrap_or("");
    if !is_main_content_type(ct) {
        return Err(Error::invalid_package_structure(format!(
            "officeDocument target {:?} has content type {:?}, not a WordprocessingML main document",
            part.name, ct
        )));
    }
    Ok(part)
}

/// A relationship of the main part whose type matches one of `suffixes`, resolved
/// to a package part.
fn main_related_parts(model: &OpcModel, main: &OpcPart, suffixes: &[&str]) -> Vec<OpcPart> {
    let mut out: Vec<OpcPart> = Vec::new();
    let Some((_, rels)) = model.part_rels.iter().find(|(o, _)| *o == main.ordinal) else {
        return out;
    };
    for r in rels {
        if !suffixes.iter().any(|s| rel_matches(&r.rel_type, s)) {
            continue;
        }
        let Some(resolved) = r.resolved.as_deref() else {
            continue;
        };
        if let Some(p) = model.part_by_name(resolved)
            && !out.iter().any(|q| q.name.eq_ignore_ascii_case(&p.name))
        {
            out.push(p.clone());
        }
    }
    out
}

fn parts_by_content_type(model: &OpcModel, suffix: &str) -> Vec<OpcPart> {
    let mut out: Vec<OpcPart> = model
        .parts
        .iter()
        .filter(|p| {
            p.content_type
                .as_deref()
                .is_some_and(|ct| ct.ends_with(suffix))
        })
        .cloned()
        .collect();
    out.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });
    out
}

fn to_ref(p: &OpcPart) -> DocxPartRef {
    DocxPartRef {
        name: p.name.clone(),
        ordinal: p.ordinal,
        content_type: p.content_type.clone(),
    }
}

fn discover(model: &OpcModel, limits: Limits) -> Result<DocxModel> {
    let main = find_main(model)?;

    let styles = main_related_parts(model, main, &["styles"])
        .into_iter()
        .next()
        .map(|p| to_ref(&p))
        .or_else(|| {
            parts_by_content_type(model, "wordprocessingml.styles+xml")
                .first()
                .map(to_ref)
        });

    let mut stories: Vec<DocxStoryPart> = Vec::new();

    let headers = {
        let mut v = main_related_parts(model, main, &["header"]);
        if v.is_empty() {
            v = parts_by_content_type(model, "wordprocessingml.header+xml");
        }
        v.sort_by(|a, b| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        });
        v
    };
    for (i, p) in headers.iter().enumerate() {
        stories.push(DocxStoryPart {
            kind: KIND_HEADER,
            index: i as u32,
            part: to_ref(p),
        });
    }

    let footers = {
        let mut v = main_related_parts(model, main, &["footer"]);
        if v.is_empty() {
            v = parts_by_content_type(model, "wordprocessingml.footer+xml");
        }
        v.sort_by(|a, b| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        });
        v
    };
    for (i, p) in footers.iter().enumerate() {
        stories.push(DocxStoryPart {
            kind: KIND_FOOTER,
            index: i as u32,
            part: to_ref(p),
        });
    }

    for (kind, suffix) in [
        (KIND_FOOTNOTES, "footnotes"),
        (KIND_ENDNOTES, "endnotes"),
        (KIND_COMMENTS, "comments"),
    ] {
        let part = main_related_parts(model, main, &[suffix])
            .into_iter()
            .next()
            .or_else(|| {
                parts_by_content_type(model, &format!("wordprocessingml.{suffix}+xml"))
                    .first()
                    .cloned()
            });
        if let Some(p) = part {
            stories.push(DocxStoryPart {
                kind,
                index: 0,
                part: to_ref(&p),
            });
        }
    }

    stories.push(DocxStoryPart {
        kind: KIND_MAIN,
        index: 0,
        part: to_ref(main),
    });
    stories.sort_by_key(|s| (s.kind, s.index));

    let _ = limits;
    Ok(DocxModel {
        main: to_ref(main),
        styles,
        stories,
    })
}

/// Build the canonical DOCX discovery model from a canonical OPC model
/// (the derived [`crate::field::node::NodeKind::DocxModel`] computation).
pub fn build_docx_model(opc_bytes: &[u8], limits: Limits) -> Result<Vec<u8>> {
    let model = OpcModel::decode(opc_bytes)?;
    let docx = discover(&model, limits)?;
    Ok(docx.encode())
}

// ---------------------------------------------------------------------------
// Node parameters
// ---------------------------------------------------------------------------

/// Canonical parameters for a [`crate::field::node::NodeKind::DocxStory`] node:
/// `version(1) · tag(1) · index(4) · profile(10) · len-prefixed part name`.
pub fn story_params(story: DocxStory, part_name: &str, profile: &DocxExtractProfile) -> Vec<u8> {
    let mut out = Vec::with_capacity(24 + part_name.len());
    out.push(1);
    out.push(story.tag());
    out.extend_from_slice(&story_index(story).to_le_bytes());
    out.extend_from_slice(&profile.encode());
    let b = part_name.as_bytes();
    out.extend_from_slice(&(b.len() as u32).to_le_bytes());
    out.extend_from_slice(b);
    out
}

const fn story_index(story: DocxStory) -> u32 {
    match story {
        DocxStory::Main => 0,
        DocxStory::Header(n) | DocxStory::Footer(n) | DocxStory::TextBox(n) => n,
        DocxStory::Footnote(id) | DocxStory::Endnote(id) | DocxStory::Comment(id) => id,
    }
}

/// Decode parameters produced by [`story_params`].
pub fn read_story_params(params: &[u8]) -> Result<(DocxStory, String, DocxExtractProfile)> {
    let mut r = ByteReader::new(params);
    if r.u8()? != 1 {
        return Err(corrupt("unsupported DOCX story params version"));
    }
    let tag = r.u8()?;
    let index = r.u32()?;
    let profile = DocxExtractProfile::decode(r.bytes(10)?)?;
    let name = r.string()?;
    if !r.at_end() {
        return Err(corrupt("DOCX story params have trailing bytes"));
    }
    Ok((DocxStory::from_tag(tag, index)?, name, profile))
}

// ---------------------------------------------------------------------------
// Small canonical byte codec helpers
// ---------------------------------------------------------------------------

fn corrupt(msg: &str) -> Error {
    Error::invalid_package_structure(format!("corrupt DOCX model: {msg}"))
}

pub(crate) fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

pub(crate) fn put_str(out: &mut Vec<u8>, s: &str) {
    put_u32(out, s.len() as u32);
    out.extend_from_slice(s.as_bytes());
}

pub(crate) fn put_opt_str(out: &mut Vec<u8>, s: Option<&str>) {
    match s {
        Some(s) => {
            out.push(1);
            put_str(out, s);
        }
        None => out.push(0),
    }
}

/// A bounds-checked little-endian reader shared by the model and story codecs.
pub(crate) struct ByteReader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> ByteReader<'a> {
    pub(crate) fn new(b: &'a [u8]) -> Self {
        ByteReader { b, at: 0 }
    }

    pub(crate) fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
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

    pub(crate) fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    pub(crate) fn u32(&mut self) -> Result<u32> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub(crate) fn string(&mut self) -> Result<String> {
        let n = self.u32()? as usize;
        let bytes = self.bytes(n)?;
        core::str::from_utf8(bytes)
            .map(str::to_string)
            .map_err(|_| corrupt("string is not UTF-8"))
    }

    pub(crate) fn opt_string(&mut self) -> Result<Option<String>> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.string()?)),
            _ => Err(corrupt("bad optional-string tag")),
        }
    }

    pub(crate) fn at_end(&self) -> bool {
        self.at == self.b.len()
    }
}

/// The styles table used to resolve heading identity (never locale names or
/// `styleId` spelling): `styleId → resolved outline level`.
#[derive(Debug, Default, Clone)]
pub struct StyleTable {
    outline: BTreeMap<String, u8>,
}

impl StyleTable {
    /// The resolved 0-based outline level for a paragraph style id, if any.
    pub fn outline_level(&self, style_id: &str) -> Option<u8> {
        self.outline.get(style_id).copied()
    }

    /// Build a table from resolved `(styleId, outlineLvl)` pairs.
    pub(crate) fn from_outline_pairs(pairs: Vec<(String, u8)>) -> StyleTable {
        StyleTable {
            outline: pairs.into_iter().collect(),
        }
    }
}

/// Parse and harden a WordprocessingML `.xml` part into a [`StyleTable`].
pub fn parse_styles(bytes: &[u8], limits: Limits) -> Result<StyleTable> {
    wml::parse_styles(bytes, limits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_roundtrips_and_fingerprints() {
        let p = DocxExtractProfile::DEFAULT;
        assert_eq!(DocxExtractProfile::decode(&p.encode()).unwrap(), p);
        assert!(p.fingerprint().contains("final"));
        let mut q = p;
        q.tracked = TrackedChanges::Original;
        q.fields = FieldMode::Code;
        assert_ne!(q.encode(), p.encode());
        assert_eq!(DocxExtractProfile::decode(&q.encode()).unwrap(), q);
    }

    #[test]
    fn story_params_roundtrip() {
        let p = DocxExtractProfile::DEFAULT;
        for story in [
            DocxStory::Main,
            DocxStory::Header(2),
            DocxStory::Footer(0),
            DocxStory::Footnote(7),
            DocxStory::Comment(3),
        ] {
            let params = story_params(story, "/word/document.xml", &p);
            let (s, name, prof) = read_story_params(&params).unwrap();
            assert_eq!(s, story);
            assert_eq!(name, "/word/document.xml");
            assert_eq!(prof, p);
        }
    }

    #[test]
    fn model_roundtrip() {
        let m = DocxModel {
            main: DocxPartRef {
                name: "/word/document.xml".into(),
                ordinal: 3,
                content_type: Some("ct".into()),
            },
            styles: Some(DocxPartRef {
                name: "/word/styles.xml".into(),
                ordinal: 4,
                content_type: None,
            }),
            stories: vec![DocxStoryPart {
                kind: KIND_MAIN,
                index: 0,
                part: DocxPartRef {
                    name: "/word/document.xml".into(),
                    ordinal: 3,
                    content_type: Some("ct".into()),
                },
            }],
        };
        assert_eq!(DocxModel::decode(&m.encode()).unwrap(), m);
    }

    #[test]
    fn textbox_is_declared_but_not_part_backed() {
        assert!(DocxStory::TextBox(0).kind_index().is_none());
        assert_eq!(DocxStory::TextBox(0).expected_root(), "txbxContent");
    }
}
