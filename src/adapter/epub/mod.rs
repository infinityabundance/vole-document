//! EPUB (OCF) package adapter (Phase 12.5, ADR-0033).
//!
//! An `.epub` is an **OCF abstract container** on the ZIP physical layer, not an
//! OPC package: it has no `[Content_Types].xml`, its package document is located
//! **semantically** from `META-INF/container.xml` (never a hardcoded
//! `OEBPS/content.opf`), and its reading order is the Package Document **spine**
//! (reflowable EPUB has **no intrinsic pages**). This module therefore reuses the
//! 12.1/12.2 ZIP layer and the shared bounded-XML policy but **not** OPC.
//!
//! Everything here is derived (`Q_gen`) state. Exact authority remains the 12.2
//! ZIP member raw spans, so an invalid or hostile EPUB is still an exact archival
//! object: only the *derived* observation declines typed, and
//! `materialize(descriptor) == original_bytes` is untouched.
//!
//! ## Two independent outcomes (research D §1)
//!
//! * **Exact-byte admissibility** — every member is exactly recoverable as a raw
//!   span regardless of validity.
//! * **EPUB conformance** — a *differential* judgement (`mimetype` placement,
//!   rootfile resolution, Package Document validity, spine integrity, resource
//!   closure) reported as [`EpubModel::issues`] and the [`MimetypeFacts`], never a
//!   gate on exactness.
//!
//! ## Security (research D §6/§8, I §4)
//!
//! No script execution, no remote fetch, no browser. External targets are inert
//! strings. `META-INF/*` other than `container.xml` is preserved and never
//! interpreted. XML is bounded by [`crate::limits::Limits`].

use std::collections::BTreeMap;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::adapter::package::xml::{
    XmlState, attr_of, doctype_declined, harden_xml, read_attrs, xml_err,
};
use crate::adapter::package::zip::{ZipMember, ZipPhysical, scan};
use crate::error::{Error, Result};
use crate::limits::Limits;

/// The exact `mimetype` payload required by OCF.
pub const EPUB_MIMETYPE: &str = "application/epub+zip";
/// The mandatory OCF container descriptor member.
pub const CONTAINER_MEMBER: &str = "META-INF/container.xml";
/// The OCF container-descriptor namespace.
pub const CONTAINER_NS: &str = "urn:oasis:names:tc:opendocument:xmlns:container";
/// The default (and only normative) package-document media type.
pub const OPF_MEDIA_TYPE: &str = "application/oebps-package+xml";
/// The legacy NCX media type (EPUB 2).
pub const NCX_MEDIA_TYPE: &str = "application/x-dtbncx+xml";

/// Sentinel ordinal meaning "not a member of this container".
const NO_MEMBER: u32 = u32::MAX;

/// Version of the EPUB extraction/reading profile semantics.
pub const EPUB_EXTRACT_PROFILE_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Reading profile
// ---------------------------------------------------------------------------

/// Which spine items participate in the reading order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpineScope {
    /// Only `linear="yes"` itemrefs (the default).
    LinearOnly,
    /// Every itemref in spine order.
    All,
}

impl SpineScope {
    /// Stable lower-case name.
    pub const fn name(self) -> &'static str {
        match self {
            SpineScope::LinearOnly => "linear-only",
            SpineScope::All => "all",
        }
    }
}

/// Whether the navigation document contributes to a reading projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavScope {
    /// The nav document is excluded from reading text (the default); it stays
    /// separately queryable.
    Excluded,
    /// The nav document contributes.
    Included,
}

impl NavScope {
    /// Stable lower-case name.
    pub const fn name(self) -> &'static str {
        match self {
            NavScope::Excluded => "excluded",
            NavScope::Included => "included",
        }
    }
}

/// A versioned, explicit reading profile. The identity is recorded in every answer
/// and hashed into the canonical selector, so a projection is never hidden and two
/// profiles never collide. `scripted` is always static-only (there is no other
/// admissible mode); it is recorded anyway so the claim is explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EpubExtractProfile {
    /// Profile semantics version; must equal [`EPUB_EXTRACT_PROFILE_VERSION`].
    pub version: u32,
    /// Which spine items participate.
    pub spine: SpineScope,
    /// Whether the nav document contributes to reading text.
    pub nav: NavScope,
    /// Include hidden (`hidden` attribute) content.
    pub hidden: bool,
    /// The only admissible scripting mode: static-only.
    pub scripted: bool,
}

impl EpubExtractProfile {
    /// The declared default: linear-only spine, nav excluded, hidden excluded,
    /// scripted static-only.
    pub const DEFAULT: EpubExtractProfile = EpubExtractProfile {
        version: EPUB_EXTRACT_PROFILE_VERSION,
        spine: SpineScope::LinearOnly,
        nav: NavScope::Excluded,
        hidden: false,
        scripted: true,
    };

    /// A stable, human-readable profile fingerprint used in canonical selectors.
    pub fn fingerprint(&self) -> String {
        format!(
            "v{}-{}-{}-h{}-s{}",
            self.version,
            self.spine.name(),
            self.nav.name(),
            self.hidden as u8,
            self.scripted as u8,
        )
    }

    /// The canonical 5-byte profile block.
    pub fn encode(&self) -> [u8; 5] {
        [
            self.version as u8,
            match self.spine {
                SpineScope::LinearOnly => 0,
                SpineScope::All => 1,
            },
            match self.nav {
                NavScope::Excluded => 0,
                NavScope::Included => 1,
            },
            self.hidden as u8,
            self.scripted as u8,
        ]
    }

    /// Decode a profile block produced by [`Self::encode`].
    pub fn decode(b: &[u8]) -> Result<EpubExtractProfile> {
        if b.len() != 5 {
            return Err(corrupt("EPUB profile must be 5 bytes"));
        }
        if b[0] as u32 != EPUB_EXTRACT_PROFILE_VERSION {
            return Err(Error::unsupported_version(format!(
                "EPUB extraction profile version {} is not supported",
                b[0]
            )));
        }
        let spine = match b[1] {
            0 => SpineScope::LinearOnly,
            1 => SpineScope::All,
            _ => return Err(corrupt("bad EPUB spine scope")),
        };
        let nav = match b[2] {
            0 => NavScope::Excluded,
            1 => NavScope::Included,
            _ => return Err(corrupt("bad EPUB nav scope")),
        };
        if b[4] != 1 {
            return Err(Error::unsupported_feature(
                "scripted EPUB content is not supported; only static-only is admissible",
            ));
        }
        Ok(EpubExtractProfile {
            version: b[0] as u32,
            spine,
            nav,
            hidden: b[3] != 0,
            scripted: true,
        })
    }
}

impl Default for EpubExtractProfile {
    fn default() -> Self {
        EpubExtractProfile::DEFAULT
    }
}

// ---------------------------------------------------------------------------
// Model types
// ---------------------------------------------------------------------------

/// The independent OCF `mimetype` conformance facts (research D §1). None of these
/// gate exact preservation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MimetypeFacts {
    /// A member named exactly `mimetype` exists.
    pub present: bool,
    /// It is the first member in the archive (ordinal 0).
    pub first: bool,
    /// It is `stored` (method 0), not DEFLATE-compressed.
    pub stored: bool,
    /// Its local file header carries no extra field.
    pub no_extra: bool,
    /// Its decoded payload is exactly [`EPUB_MIMETYPE`] with no extra bytes.
    pub exact_bytes: bool,
    /// Its physical ordinal (`NO_MEMBER` when absent).
    pub ordinal: u32,
    /// All of the above hold.
    pub conformant: bool,
}

/// One `rootfile` from `META-INF/container.xml`, resolved to a container member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootFile {
    /// `full-path` exactly as written.
    pub full_path: String,
    /// The resolved container-relative member name (empty when unresolved).
    pub member: String,
    /// `media-type` (defaults to [`OPF_MEDIA_TYPE`]).
    pub media_type: String,
    /// The resolved member ordinal (`NO_MEMBER` when unresolved).
    pub ordinal: u32,
}

/// One Package Document metadata entry (a Dublin Core element or a `<meta>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataEntry {
    /// The element local name (e.g. `title`, `identifier`, or `meta`).
    pub name: String,
    /// `property` (EPUB 3 refinements), when present.
    pub property: Option<String>,
    /// `refines` (IDREF to the refined element), when present.
    pub refines: Option<String>,
    /// `id`, when present.
    pub id: Option<String>,
    /// The legacy `<meta name="...">` attribute, when present.
    pub attr_name: Option<String>,
    /// The legacy `<meta scheme="...">` attribute, when present.
    pub scheme: Option<String>,
    /// The element's text content (or the legacy `content` attribute).
    pub value: String,
}

/// One Package Document manifest item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestItem {
    /// `id` (unique within the package).
    pub id: String,
    /// `href` exactly as written.
    pub href: String,
    /// `media-type`.
    pub media_type: String,
    /// The space-separated `properties` set.
    pub properties: Vec<String>,
    /// `fallback` (IDREF), when present.
    pub fallback: Option<String>,
    /// The resolved container member name, when internal and present.
    pub resolved: Option<String>,
    /// The resolved member ordinal (`NO_MEMBER` when unresolved/external).
    pub ordinal: u32,
    /// The href is an absolute URI: an inert identifier, never fetched.
    pub external: bool,
}

impl ManifestItem {
    /// Whether the item is the navigation document (`properties="nav"`).
    pub fn is_nav(&self) -> bool {
        self.properties.iter().any(|p| p == "nav")
    }

    /// The resolved member ordinal, when the item is an internal present resource.
    pub fn resolved_ordinal(&self) -> Option<u32> {
        if self.external || self.ordinal == NO_MEMBER {
            None
        } else {
            Some(self.ordinal)
        }
    }
}

/// One Package Document spine `itemref`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpineItemRef {
    /// `idref` (→ manifest item).
    pub idref: String,
    /// `linear` (`yes` = true).
    pub linear: bool,
    /// The space-separated `properties` set.
    pub properties: Vec<String>,
    /// The manifest index the `idref` resolves to (`NO_MEMBER` when dangling).
    pub item_index: u32,
    /// The resolved member ordinal (`NO_MEMBER` when unresolved/external).
    pub ordinal: u32,
}

/// The resolved Package Document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageDoc {
    /// The package document's container member name.
    pub member: String,
    /// The package document's member ordinal.
    pub ordinal: u32,
    /// `<package @version>`.
    pub version: Option<String>,
    /// `<package @unique-identifier>` (IDREF).
    pub unique_identifier: Option<String>,
    /// `<spine @page-progression-direction>`.
    pub page_progression: Option<String>,
    /// `rendition:layout` (`reflowable` | `pre-paginated`), when declared.
    pub layout: Option<String>,
    /// `<spine @toc>` (legacy NCX IDREF), when present.
    pub toc: Option<String>,
    /// The manifest id of the cover image, when declared.
    pub cover_id: Option<String>,
    /// Metadata entries in document order.
    pub metadata: Vec<MetadataEntry>,
    /// Manifest items in document order.
    pub manifest: Vec<ManifestItem>,
    /// Spine itemrefs in reading order.
    pub spine: Vec<SpineItemRef>,
    /// The manifest index of the nav document, when present.
    pub nav_item: Option<u32>,
    /// The manifest index of the legacy NCX, when present.
    pub ncx_item: Option<u32>,
    /// Non-fatal structural observations.
    pub issues: Vec<String>,
}

impl PackageDoc {
    /// The manifest item with `id`, if any.
    pub fn manifest_by_id(&self, id: &str) -> Option<&ManifestItem> {
        self.manifest.iter().find(|m| m.id == id)
    }

    /// The manifest indices participating in the reading order under `profile`.
    pub fn reading_order(&self, profile: &EpubExtractProfile) -> Vec<u32> {
        self.spine
            .iter()
            .filter(|s| profile.spine == SpineScope::All || s.linear)
            .map(|s| s.item_index)
            .collect()
    }
}

/// The canonical EPUB discovery model (the derived state of the `EpubModel` node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpubModel {
    /// The `mimetype` conformance facts.
    pub mimetype: MimetypeFacts,
    /// The container `rootfile`s, in document order.
    pub rootfiles: Vec<RootFile>,
    /// The Package Document resolved from the first rootfile, when available.
    pub package: Option<PackageDoc>,
    /// Model-level non-fatal observations.
    pub issues: Vec<String>,
}

impl EpubModel {
    /// Deterministically encode the model (length-prefixed little-endian).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"EPBM");
        out.push(1);

        let m = &self.mimetype;
        for b in [
            m.present,
            m.first,
            m.stored,
            m.no_extra,
            m.exact_bytes,
            m.conformant,
        ] {
            out.push(b as u8);
        }
        put_u32(&mut out, m.ordinal);

        put_u32(&mut out, self.rootfiles.len() as u32);
        for r in &self.rootfiles {
            put_str(&mut out, &r.full_path);
            put_str(&mut out, &r.member);
            put_str(&mut out, &r.media_type);
            put_u32(&mut out, r.ordinal);
        }

        match &self.package {
            None => out.push(0),
            Some(p) => {
                out.push(1);
                put_str(&mut out, &p.member);
                put_u32(&mut out, p.ordinal);
                put_opt_str(&mut out, p.version.as_deref());
                put_opt_str(&mut out, p.unique_identifier.as_deref());
                put_opt_str(&mut out, p.page_progression.as_deref());
                put_opt_str(&mut out, p.layout.as_deref());
                put_opt_str(&mut out, p.toc.as_deref());
                put_opt_str(&mut out, p.cover_id.as_deref());
                put_opt_u32(&mut out, p.nav_item);
                put_opt_u32(&mut out, p.ncx_item);

                put_u32(&mut out, p.metadata.len() as u32);
                for e in &p.metadata {
                    put_str(&mut out, &e.name);
                    put_opt_str(&mut out, e.property.as_deref());
                    put_opt_str(&mut out, e.refines.as_deref());
                    put_opt_str(&mut out, e.id.as_deref());
                    put_opt_str(&mut out, e.attr_name.as_deref());
                    put_opt_str(&mut out, e.scheme.as_deref());
                    put_str(&mut out, &e.value);
                }

                put_u32(&mut out, p.manifest.len() as u32);
                for it in &p.manifest {
                    put_str(&mut out, &it.id);
                    put_str(&mut out, &it.href);
                    put_str(&mut out, &it.media_type);
                    put_strs(&mut out, &it.properties);
                    put_opt_str(&mut out, it.fallback.as_deref());
                    put_opt_str(&mut out, it.resolved.as_deref());
                    put_u32(&mut out, it.ordinal);
                    out.push(it.external as u8);
                }

                put_u32(&mut out, p.spine.len() as u32);
                for s in &p.spine {
                    put_str(&mut out, &s.idref);
                    out.push(s.linear as u8);
                    put_strs(&mut out, &s.properties);
                    put_u32(&mut out, s.item_index);
                    put_u32(&mut out, s.ordinal);
                }
                put_strs(&mut out, &p.issues);
            }
        }
        put_strs(&mut out, &self.issues);
        out
    }

    /// Decode a model produced by [`Self::encode`].
    pub fn decode(bytes: &[u8]) -> Result<EpubModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"EPBM" {
            return Err(corrupt("bad EPUB model magic"));
        }
        if r.u8()? != 1 {
            return Err(corrupt("unsupported EPUB model version"));
        }
        let mimetype = MimetypeFacts {
            present: r.u8()? != 0,
            first: r.u8()? != 0,
            stored: r.u8()? != 0,
            no_extra: r.u8()? != 0,
            exact_bytes: r.u8()? != 0,
            conformant: r.u8()? != 0,
            ordinal: r.u32()?,
        };
        let n = bounded(&mut r, "rootfile")?;
        let mut rootfiles = Vec::with_capacity(n as usize);
        for _ in 0..n {
            rootfiles.push(RootFile {
                full_path: r.string()?,
                member: r.string()?,
                media_type: r.string()?,
                ordinal: r.u32()?,
            });
        }
        let package = if r.u8()? == 0 {
            None
        } else {
            Some(read_package(&mut r)?)
        };
        let issues = read_strs(&mut r)?;
        if !r.at_end() {
            return Err(corrupt("EPUB model has trailing bytes"));
        }
        Ok(EpubModel {
            mimetype,
            rootfiles,
            package,
            issues,
        })
    }
}

fn read_package(r: &mut BinReader<'_>) -> Result<PackageDoc> {
    let member = r.string()?;
    let ordinal = r.u32()?;
    let version = r.opt_string()?;
    let unique_identifier = r.opt_string()?;
    let page_progression = r.opt_string()?;
    let layout = r.opt_string()?;
    let toc = r.opt_string()?;
    let cover_id = r.opt_string()?;
    let nav_item = r.opt_u32()?;
    let ncx_item = r.opt_u32()?;

    let n = bounded(r, "metadata")?;
    let mut metadata = Vec::with_capacity(n as usize);
    for _ in 0..n {
        metadata.push(MetadataEntry {
            name: r.string()?,
            property: r.opt_string()?,
            refines: r.opt_string()?,
            id: r.opt_string()?,
            attr_name: r.opt_string()?,
            scheme: r.opt_string()?,
            value: r.string()?,
        });
    }

    let n = bounded(r, "manifest")?;
    let mut manifest = Vec::with_capacity(n as usize);
    for _ in 0..n {
        manifest.push(ManifestItem {
            id: r.string()?,
            href: r.string()?,
            media_type: r.string()?,
            properties: read_strs(r)?,
            fallback: r.opt_string()?,
            resolved: r.opt_string()?,
            ordinal: r.u32()?,
            external: r.u8()? != 0,
        });
    }

    let n = bounded(r, "spine")?;
    let mut spine = Vec::with_capacity(n as usize);
    for _ in 0..n {
        spine.push(SpineItemRef {
            idref: r.string()?,
            linear: r.u8()? != 0,
            properties: read_strs(r)?,
            item_index: r.u32()?,
            ordinal: r.u32()?,
        });
    }
    let issues = read_strs(r)?;
    Ok(PackageDoc {
        member,
        ordinal,
        version,
        unique_identifier,
        page_progression,
        layout,
        toc,
        cover_id,
        metadata,
        manifest,
        spine,
        nav_item,
        ncx_item,
        issues,
    })
}

fn bounded(r: &mut BinReader<'_>, what: &str) -> Result<u32> {
    let n = r.u32()?;
    if n as u64 > 1 << 24 {
        return Err(corrupt(&format!("EPUB {what} count is implausible")));
    }
    Ok(n)
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// Build the canonical EPUB discovery model from an exact OCF/ZIP source (the
/// derived [`crate::field::node::NodeKind::EpubModel`] computation).
///
/// Fails closed with a typed error (`InvalidPackageStructure`/`InvalidXmlStructure`/
/// `InvalidZipStructure`/`ResourceLimit`) when the container descriptor is missing
/// or malformed, or when no declared rootfile resolves to a package document. The
/// exact bytes remain recoverable regardless.
pub fn build_epub_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    let physical = scan(source, limits)?;
    physical.validate(source.len() as u64)?;
    let model = discover(source, &physical, limits)?;
    Ok(model.encode())
}

fn discover(source: &[u8], physical: &ZipPhysical, limits: Limits) -> Result<EpubModel> {
    let mut issues: Vec<String> = Vec::new();

    // Member lookup by exact (case-sensitive) container name.
    let mut lookup: BTreeMap<String, u32> = BTreeMap::new();
    for member in &physical.members {
        match core::str::from_utf8(&member.name) {
            Ok(name) => {
                if lookup.insert(name.to_string(), member.id.ordinal).is_some() {
                    issues.push(format!("duplicate container member name {name:?}"));
                }
            }
            Err(_) => issues.push(
                "container has a non-UTF-8 member name (preserved, uninterpreted)".to_string(),
            ),
        }
    }

    let mimetype = mimetype_facts(source, physical, limits);

    // Parse `META-INF/container.xml`.
    let container = physical
        .members
        .iter()
        .find(|m| m.name.as_slice() == CONTAINER_MEMBER.as_bytes())
        .ok_or_else(|| {
            Error::invalid_package_structure(
                "OCF container has no META-INF/container.xml descriptor",
            )
        })?;
    let container_bytes = decode_member(source, container, limits).ok_or_else(|| {
        Error::invalid_package_structure(
            "container.xml could not be decoded (encrypted or oversized)",
        )
    })?;
    let rootfiles = parse_container(&container_bytes, limits, &lookup, &mut issues)?;

    // Locate the Package Document from the first *resolvable* rootfile.
    let first = rootfiles
        .iter()
        .find(|r| r.ordinal != NO_MEMBER)
        .ok_or_else(|| {
            Error::invalid_package_structure(
                "no container rootfile resolves to a package document member",
            )
        })?;
    let opf_member = physical
        .members
        .iter()
        .find(|m| m.id.ordinal == first.ordinal)
        .ok_or_else(|| Error::invalid_package_structure("rootfile member vanished"))?;
    let opf_bytes = decode_member(source, opf_member, limits).ok_or_else(|| {
        Error::invalid_package_structure(
            "package document could not be decoded (encrypted or oversized)",
        )
    })?;

    let base_dir = dir_of(&first.member);
    let mut package = parse_package(
        &opf_bytes,
        limits,
        &first.member,
        first.ordinal,
        &base_dir,
        &lookup,
    )?;
    package.issues.extend(issues.clone());

    if rootfiles.len() > 1 {
        issues.push(format!(
            "container declares {} rootfiles (renditions); the first is the default and the ambiguity is recorded, never silently resolved",
            rootfiles.len()
        ));
    }

    Ok(EpubModel {
        mimetype,
        rootfiles,
        package: Some(package),
        issues,
    })
}

fn mimetype_facts(source: &[u8], physical: &ZipPhysical, limits: Limits) -> MimetypeFacts {
    let member = physical
        .members
        .iter()
        .find(|m| m.name.as_slice() == b"mimetype");
    let Some(m) = member else {
        return MimetypeFacts {
            present: false,
            first: false,
            stored: false,
            no_extra: false,
            exact_bytes: false,
            ordinal: NO_MEMBER,
            conformant: false,
        };
    };
    let first = physical
        .members
        .first()
        .is_some_and(|x| x.name.as_slice() == b"mimetype");
    let stored = m.method == 0;
    // The local file header is `30 + name_len + extra_len` bytes; any excess over
    // `30 + name_len` is a local extra field.
    let local_extra = m.local_header.1.saturating_sub(30 + m.name.len() as u64);
    let no_extra = local_extra == 0;
    let exact_bytes =
        decode_member(source, m, limits).is_some_and(|b| b == EPUB_MIMETYPE.as_bytes());
    MimetypeFacts {
        present: true,
        first,
        stored,
        no_extra,
        exact_bytes,
        ordinal: m.id.ordinal,
        conformant: first && stored && no_extra && exact_bytes,
    }
}

/// Decode one member (stored or raw-DEFLATE, unencrypted, within `max_xml_part_bytes`).
fn decode_member(source: &[u8], member: &ZipMember, limits: Limits) -> Option<Vec<u8>> {
    const FLAG_ENCRYPTED: u16 = 0x0001;
    if member.flags & FLAG_ENCRYPTED != 0 {
        return None;
    }
    if member.uncompressed_size > limits.max_xml_part_bytes {
        return None;
    }
    let off = usize::try_from(member.data.0).ok()?;
    let len = usize::try_from(member.data.1).ok()?;
    let raw = source.get(off..off.checked_add(len)?)?;
    match member.method {
        0 => Some(raw.to_vec()),
        8 => crate::field::derive::inflate_raw_deflate(raw, member.uncompressed_size, limits).ok(),
        _ => None,
    }
}

/// The directory prefix of a member name, ending in `/` (`a/b/c` → `a/b/`).
fn dir_of(name: &str) -> String {
    match name.rfind('/') {
        Some(i) => name[..=i].to_string(),
        None => String::new(),
    }
}

/// Whether an href is an absolute URI (has an RFC 3986 scheme) or protocol-relative.
fn is_absolute_href(href: &str) -> bool {
    if href.starts_with("//") {
        return true;
    }
    let b = href.as_bytes();
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

/// Resolve a container-relative reference against `base_dir` (RFC 3986 path merge
/// with `.`/`..` normalization clamped inside the container root). Never a host
/// path. Returns `Ok("")` for a pure fragment/query reference to the base itself.
fn resolve_member_path(base_dir: &str, target: &str, limits: Limits) -> Result<String> {
    if target.is_empty() {
        return Err(Error::invalid_package_structure("href is empty"));
    }
    if target.bytes().any(|b| b == 0 || b == b'\\' || b < 0x20) {
        return Err(Error::invalid_package_structure(
            "href contains forbidden bytes",
        ));
    }
    let no_fragment = target.split('#').next().unwrap_or("");
    let path = no_fragment.split('?').next().unwrap_or("");
    if path.is_empty() {
        return Ok(String::new());
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
                        "href escapes the container root",
                    ));
                }
            }
            other => segs.push(other),
        }
    }
    if segs.is_empty() {
        return Ok(String::new());
    }
    let resolved = segs.join("/");
    if resolved.len() as u64 > u64::from(limits.max_opc_part_name_bytes) {
        return Err(Error::resource_limit(
            "resolved href exceeds max_opc_part_name_bytes",
        ));
    }
    Ok(resolved)
}

// ---------------------------------------------------------------------------
// META-INF/container.xml
// ---------------------------------------------------------------------------

fn parse_container(
    xml: &[u8],
    limits: Limits,
    lookup: &BTreeMap<String, u32>,
    issues: &mut Vec<String>,
) -> Result<Vec<RootFile>> {
    harden_xml(xml, limits)?;
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut st = XmlState::new();
    let mut out: Vec<RootFile> = Vec::new();
    let mut saw_root = false;
    loop {
        let ev = reader.read_event().map_err(xml_err)?;
        st.event(limits)?;
        match ev {
            Event::Eof => break,
            Event::DocType(_) => return Err(doctype_declined()),
            Event::Start(e) => {
                st.open(limits)?;
                let local = e.name().local_name().as_ref().to_string();
                if !saw_root {
                    if local != "container" {
                        return Err(Error::invalid_package_structure(
                            "container.xml root element is not <container>",
                        ));
                    }
                    saw_root = true;
                } else if local == "rootfile" {
                    push_rootfile(&e, limits, lookup, issues, &mut out)?;
                }
            }
            Event::Empty(e) => {
                st.leaf(limits)?;
                let local = e.name().local_name().as_ref().to_string();
                if !saw_root {
                    return Err(Error::invalid_package_structure(
                        "container.xml root element is not <container>",
                    ));
                }
                if local == "rootfile" {
                    push_rootfile(&e, limits, lookup, issues, &mut out)?;
                }
            }
            Event::End(_) => st.close(),
            Event::Text(t) => st.text(t.len(), limits)?,
            _ => {}
        }
    }
    if !saw_root {
        return Err(Error::invalid_xml_structure("container.xml is empty"));
    }
    if out.len() as u64 > u64::from(limits.max_epub_rootfiles) {
        return Err(Error::resource_limit(
            "container.xml exceeds max_epub_rootfiles",
        ));
    }
    if out.is_empty() {
        return Err(Error::invalid_package_structure(
            "container.xml declares no rootfile",
        ));
    }
    Ok(out)
}

fn push_rootfile(
    e: &BytesStart<'_>,
    limits: Limits,
    lookup: &BTreeMap<String, u32>,
    issues: &mut Vec<String>,
    out: &mut Vec<RootFile>,
) -> Result<()> {
    let attrs = read_attrs(e, limits)?;
    let full = attr_of(&attrs, "full-path")
        .ok_or_else(|| Error::invalid_package_structure("rootfile lacks full-path"))?;
    if full.is_empty() {
        return Err(Error::invalid_package_structure(
            "rootfile full-path is empty",
        ));
    }
    let media_type = attr_of(&attrs, "media-type")
        .unwrap_or(OPF_MEDIA_TYPE)
        .to_string();
    let (member, ordinal) = if is_absolute_href(full) {
        issues.push(format!(
            "rootfile full-path {full:?} is an absolute URI; inert, never fetched"
        ));
        (String::new(), NO_MEMBER)
    } else {
        match resolve_member_path("", full, limits) {
            Ok(m) => {
                let ord = lookup.get(&m).copied().unwrap_or(NO_MEMBER);
                if ord == NO_MEMBER {
                    issues.push(format!(
                        "rootfile full-path {full:?} does not resolve to a container member"
                    ));
                }
                (m, ord)
            }
            Err(_) => {
                issues.push(format!(
                    "rootfile full-path {full:?} is not a safe container path"
                ));
                (String::new(), NO_MEMBER)
            }
        }
    };
    out.push(RootFile {
        full_path: full.to_string(),
        member,
        media_type,
        ordinal,
    });
    Ok(())
}

// ---------------------------------------------------------------------------
// Package Document
// ---------------------------------------------------------------------------

struct RawItem {
    id: String,
    href: String,
    media_type: String,
    properties: Vec<String>,
    fallback: Option<String>,
}

struct RawSpine {
    idref: String,
    linear: bool,
    properties: Vec<String>,
}

#[derive(Default)]
struct RawPackage {
    version: Option<String>,
    unique_identifier: Option<String>,
    page_progression: Option<String>,
    toc: Option<String>,
    metadata: Vec<MetadataEntry>,
    items: Vec<RawItem>,
    spine: Vec<RawSpine>,
}

struct PendingMeta {
    local: String,
    attrs: Vec<(String, String)>,
    text: String,
}

fn split_ws(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_string).collect()
}

fn read_item(e: &BytesStart<'_>, limits: Limits) -> Result<RawItem> {
    let attrs = read_attrs(e, limits)?;
    let id = attr_of(&attrs, "id")
        .ok_or_else(|| Error::invalid_package_structure("manifest item lacks id"))?;
    let href = attr_of(&attrs, "href")
        .ok_or_else(|| Error::invalid_package_structure("manifest item lacks href"))?;
    Ok(RawItem {
        id: id.to_string(),
        href: href.to_string(),
        media_type: attr_of(&attrs, "media-type").unwrap_or("").to_string(),
        properties: attr_of(&attrs, "properties")
            .map(split_ws)
            .unwrap_or_default(),
        fallback: attr_of(&attrs, "fallback").map(str::to_string),
    })
}

fn read_itemref(e: &BytesStart<'_>, limits: Limits) -> Result<RawSpine> {
    let attrs = read_attrs(e, limits)?;
    let idref = attr_of(&attrs, "idref")
        .ok_or_else(|| Error::invalid_package_structure("spine itemref lacks idref"))?;
    Ok(RawSpine {
        idref: idref.to_string(),
        linear: attr_of(&attrs, "linear") != Some("no"),
        properties: attr_of(&attrs, "properties")
            .map(split_ws)
            .unwrap_or_default(),
    })
}

fn finalize_meta(out: &mut RawPackage, local: &str, attrs: Vec<(String, String)>, text: String) {
    let value = attr_of(&attrs, "content")
        .map(str::to_string)
        .unwrap_or_else(|| text.trim().to_string());
    out.metadata.push(MetadataEntry {
        name: local.to_string(),
        property: attr_of(&attrs, "property").map(str::to_string),
        refines: attr_of(&attrs, "refines").map(str::to_string),
        id: attr_of(&attrs, "id").map(str::to_string),
        attr_name: attr_of(&attrs, "name").map(str::to_string),
        scheme: attr_of(&attrs, "scheme").map(str::to_string),
        value,
    });
}

fn parse_package(
    xml: &[u8],
    limits: Limits,
    member: &str,
    ordinal: u32,
    base_dir: &str,
    lookup: &BTreeMap<String, u32>,
) -> Result<PackageDoc> {
    harden_xml(xml, limits)?;
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut st = XmlState::new();
    let mut raw = RawPackage::default();
    let mut stack: Vec<String> = Vec::new();
    let mut pending: Option<PendingMeta> = None;
    let mut saw_root = false;

    loop {
        let ev = reader.read_event().map_err(xml_err)?;
        st.event(limits)?;
        match ev {
            Event::Eof => break,
            Event::DocType(_) => return Err(doctype_declined()),
            Event::Start(e) => {
                st.open(limits)?;
                let local = e.name().local_name().as_ref().to_string();
                if !saw_root {
                    if local != "package" {
                        return Err(Error::invalid_package_structure(
                            "package document root element is not <package>",
                        ));
                    }
                    saw_root = true;
                    let attrs = read_attrs(&e, limits)?;
                    raw.version = attr_of(&attrs, "version").map(str::to_string);
                    raw.unique_identifier =
                        attr_of(&attrs, "unique-identifier").map(str::to_string);
                } else {
                    let parent = stack.last().map(String::as_str);
                    match local.as_str() {
                        "spine" if parent == Some("package") => {
                            let attrs = read_attrs(&e, limits)?;
                            raw.toc = attr_of(&attrs, "toc").map(str::to_string);
                            raw.page_progression =
                                attr_of(&attrs, "page-progression-direction").map(str::to_string);
                        }
                        "item" if parent == Some("manifest") => {
                            raw.items.push(read_item(&e, limits)?);
                        }
                        "itemref" if parent == Some("spine") => {
                            raw.spine.push(read_itemref(&e, limits)?);
                        }
                        _ if parent == Some("metadata") => {
                            let attrs = read_attrs(&e, limits)?;
                            pending = Some(PendingMeta {
                                local: local.clone(),
                                attrs,
                                text: String::new(),
                            });
                        }
                        _ => {}
                    }
                }
                stack.push(local);
            }
            Event::Empty(e) => {
                st.leaf(limits)?;
                let local = e.name().local_name().as_ref().to_string();
                if !saw_root {
                    return Err(Error::invalid_package_structure(
                        "package document root element is not <package>",
                    ));
                }
                let parent = stack.last().map(String::as_str);
                match local.as_str() {
                    "item" if parent == Some("manifest") => {
                        raw.items.push(read_item(&e, limits)?);
                    }
                    "itemref" if parent == Some("spine") => {
                        raw.spine.push(read_itemref(&e, limits)?);
                    }
                    _ if parent == Some("metadata") => {
                        let attrs = read_attrs(&e, limits)?;
                        finalize_meta(&mut raw, &local, attrs, String::new());
                    }
                    _ => {}
                }
            }
            Event::End(e) => {
                let local = e.name().local_name().as_ref().to_string();
                if pending.as_ref().is_some_and(|p| p.local == local)
                    && let Some(p) = pending.take()
                {
                    finalize_meta(&mut raw, &p.local, p.attrs, p.text);
                }
                stack.pop();
            }
            Event::Text(t) => {
                st.text(t.len(), limits)?;
                if let Some(p) = pending.as_mut() {
                    p.text.push_str(t.into_inner().as_ref());
                }
            }
            Event::GeneralRef(r) => {
                if let Some(p) = pending.as_mut() {
                    p.text.push_str(&entity_ref_text(r));
                }
            }
            _ => {}
        }
    }

    if !saw_root {
        return Err(Error::invalid_xml_structure("package document is empty"));
    }
    if !stack.is_empty() {
        return Err(Error::invalid_xml_structure(
            "package document is not well-formed (unclosed elements)",
        ));
    }
    if raw.items.len() as u64 > u64::from(limits.max_epub_manifest_items) {
        return Err(Error::resource_limit(
            "manifest exceeds max_epub_manifest_items",
        ));
    }
    if raw.spine.len() as u64 > u64::from(limits.max_epub_spine_items) {
        return Err(Error::resource_limit("spine exceeds max_epub_spine_items"));
    }

    let mut issues: Vec<String> = Vec::new();

    // Resolve manifest hrefs to container members (external targets stay inert).
    let mut manifest: Vec<ManifestItem> = Vec::with_capacity(raw.items.len());
    for it in &raw.items {
        let (resolved, ordinal, external) = if is_absolute_href(&it.href) {
            (None, NO_MEMBER, true)
        } else {
            match resolve_member_path(base_dir, &it.href, limits) {
                Ok(m) if m.is_empty() => {
                    issues.push(format!("manifest item {:?} has no resource path", it.id));
                    (None, NO_MEMBER, false)
                }
                Ok(m) => {
                    let ord = lookup.get(&m).copied().unwrap_or(NO_MEMBER);
                    if ord == NO_MEMBER {
                        issues.push(format!(
                            "manifest item {:?} targets missing member {m:?}",
                            it.id
                        ));
                    }
                    (Some(m), ord, false)
                }
                Err(_) => {
                    issues.push(format!("manifest item {:?} has an unsafe href", it.id));
                    (None, NO_MEMBER, false)
                }
            }
        };
        manifest.push(ManifestItem {
            id: it.id.clone(),
            href: it.href.clone(),
            media_type: it.media_type.clone(),
            properties: it.properties.clone(),
            fallback: it.fallback.clone(),
            resolved,
            ordinal,
            external,
        });
    }

    // Resolve spine idrefs to manifest indices.
    let mut spine: Vec<SpineItemRef> = Vec::with_capacity(raw.spine.len());
    for s in &raw.spine {
        let idx = manifest.iter().position(|m| m.id == s.idref);
        let item_index = idx.map_or(NO_MEMBER, |i| i as u32);
        let ordinal = idx.map_or(NO_MEMBER, |i| manifest[i].ordinal);
        if idx.is_none() {
            issues.push(format!("spine itemref {:?} does not resolve", s.idref));
        }
        spine.push(SpineItemRef {
            idref: s.idref.clone(),
            linear: s.linear,
            properties: s.properties.clone(),
            item_index,
            ordinal,
        });
    }

    let nav_item = manifest
        .iter()
        .position(ManifestItem::is_nav)
        .map(|i| i as u32);
    let ncx_item = manifest
        .iter()
        .position(|m| m.media_type == NCX_MEDIA_TYPE)
        .map(|i| i as u32);
    let layout = raw
        .metadata
        .iter()
        .find(|m| m.property.as_deref() == Some("rendition:layout"))
        .map(|m| m.value.clone());

    // Cover identity: EPUB 3 `properties="cover-image"`, else legacy `<meta name="cover">`.
    let cover_prop = manifest
        .iter()
        .find(|m| m.properties.iter().any(|p| p == "cover-image"))
        .map(|m| m.id.clone());
    let cover_meta = raw
        .metadata
        .iter()
        .find(|m| m.attr_name.as_deref() == Some("cover"))
        .map(|m| m.value.clone());
    let cover_id = match (&cover_prop, &cover_meta) {
        (Some(a), Some(b)) if a != b => {
            issues.push(format!(
                "cover identity disagrees: properties cover-image {a:?} vs legacy meta cover {b:?}"
            ));
            None
        }
        (Some(a), _) => Some(a.clone()),
        (None, Some(b)) => Some(b.clone()),
        (None, None) => None,
    };

    if raw
        .metadata
        .iter()
        .all(|m| m.property.as_deref() != Some("dcterms:modified"))
    {
        issues.push("package metadata is missing the required dcterms:modified".to_string());
    }

    Ok(PackageDoc {
        member: member.to_string(),
        ordinal,
        version: raw.version,
        unique_identifier: raw.unique_identifier,
        page_progression: raw.page_progression,
        layout,
        toc: raw.toc,
        cover_id,
        metadata: raw.metadata,
        manifest,
        spine,
        nav_item,
        ncx_item,
        issues,
    })
}

// ---------------------------------------------------------------------------
// Navigation document
// ---------------------------------------------------------------------------

/// One flattened navigation entry from the EPUB Navigation Document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavEntry {
    /// 0-based depth in the nested `ol`/`ul` structure.
    pub depth: u32,
    /// The enclosing `<nav>`'s `epub:type` (`toc`, `landmarks`, `page-list`, …).
    pub nav_type: String,
    /// The anchor label text.
    pub label: String,
    /// `href` exactly as written.
    pub href: String,
    /// The resolved internal member (empty for a pure fragment or external).
    pub member: Option<String>,
    /// The resolved fragment (after `#`), when present.
    pub fragment: Option<String>,
    /// The href is an absolute URI: inert, never fetched.
    pub external: bool,
}

/// Parse the EPUB Navigation Document (bounded XHTML) into a flattened list of
/// navigation entries in document order. Never executes scripts and never fetches
/// an external target: an absolute href is an inert string.
pub fn parse_nav_document(xml: &[u8], base_dir: &str, limits: Limits) -> Result<Vec<NavEntry>> {
    harden_xml(xml, limits)?;
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut st = XmlState::new();
    let mut out: Vec<NavEntry> = Vec::new();
    let mut nav_types: Vec<String> = Vec::new();
    let mut list_depth: u32 = 0;
    let mut pending_a: Option<(String, String)> = None; // (href, label)
    let mut nodes: u64 = 0;

    loop {
        let ev = reader.read_event().map_err(xml_err)?;
        st.event(limits)?;
        match ev {
            Event::Eof => break,
            Event::DocType(_) => return Err(doctype_declined()),
            Event::Start(e) | Event::Empty(e) => {
                st.leaf(limits)?;
                nodes = nodes.saturating_add(1);
                if nodes > u64::from(limits.max_xhtml_nodes) {
                    return Err(Error::resource_limit("nav exceeds max_xhtml_nodes"));
                }
                let local = e.name().local_name().as_ref().to_string();
                match local.as_str() {
                    "nav" => {
                        let attrs = read_attrs(&e, limits)?;
                        let ty = attr_of(&attrs, "type").unwrap_or("").to_string();
                        nav_types.push(ty);
                    }
                    "ol" | "ul" => {
                        list_depth = list_depth.saturating_add(1);
                        if list_depth > limits.max_epub_nav_depth {
                            return Err(Error::resource_limit(
                                "nav nesting exceeds max_epub_nav_depth",
                            ));
                        }
                    }
                    "a" => {
                        let attrs = read_attrs(&e, limits)?;
                        let href = attr_of(&attrs, "href").unwrap_or("").to_string();
                        pending_a = Some((href, String::new()));
                    }
                    _ => {}
                }
            }
            Event::End(e) => {
                let local = e.name().local_name().as_ref().to_string();
                match local.as_str() {
                    "nav" => {
                        nav_types.pop();
                    }
                    "ol" | "ul" => list_depth = list_depth.saturating_sub(1),
                    "a" => {
                        if let Some((href, label)) = pending_a.take() {
                            let nav_type = nav_types.last().cloned().unwrap_or_default();
                            let fragment = href
                                .split_once('#')
                                .map(|(_, f)| f.to_string())
                                .filter(|f| !f.is_empty());
                            let (member, external) = classify_href(base_dir, &href, limits);
                            out.push(NavEntry {
                                depth: list_depth,
                                nav_type,
                                label,
                                href,
                                member,
                                fragment,
                                external,
                            });
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(t) => {
                st.text(t.len(), limits)?;
                if let Some((_, label)) = pending_a.as_mut() {
                    label.push_str(t.into_inner().as_ref());
                }
            }
            Event::GeneralRef(r) => {
                if let Some((_, label)) = pending_a.as_mut() {
                    label.push_str(&entity_ref_text(r));
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

/// Decode a general entity/character reference without any DTD or external lookup.
fn entity_ref_text(r: quick_xml::events::BytesRef<'_>) -> String {
    if r.is_char_ref() {
        match r.resolve_char_ref() {
            Ok(Some(c)) => c.to_string(),
            _ => String::new(),
        }
    } else {
        match r.into_inner().as_ref() {
            "amp" => "&".to_string(),
            "lt" => "<".to_string(),
            "gt" => ">".to_string(),
            "quot" => "\"".to_string(),
            "apos" => "'".to_string(),
            // No DTD/entity resolver: an undeclared entity is never expanded.
            _ => String::new(),
        }
    }
}

fn classify_href(base_dir: &str, href: &str, limits: Limits) -> (Option<String>, bool) {
    if href.is_empty() || is_absolute_href(href) {
        return (None, is_absolute_href(href));
    }
    match resolve_member_path(base_dir, href, limits) {
        Ok(m) if m.is_empty() => (None, false),
        Ok(m) => (Some(m), false),
        Err(_) => (None, false),
    }
}

// ---------------------------------------------------------------------------
// Codec helpers
// ---------------------------------------------------------------------------

fn corrupt(msg: &str) -> Error {
    Error::invalid_package_structure(format!("corrupt EPUB model: {msg}"))
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

fn put_opt_u32(out: &mut Vec<u8>, v: Option<u32>) {
    match v {
        Some(v) => {
            out.push(1);
            put_u32(out, v);
        }
        None => out.push(0),
    }
}

fn put_strs(out: &mut Vec<u8>, items: &[String]) {
    put_u32(out, items.len() as u32);
    for s in items {
        put_str(out, s);
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
            .ok_or_else(|| corrupt("length overflow"))?;
        let s = self
            .b
            .get(self.at..end)
            .ok_or_else(|| corrupt("truncated"))?;
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
        if n as u64 > 1 << 28 {
            return Err(corrupt("string length is implausible"));
        }
        let b = self.bytes(n)?;
        String::from_utf8(b.to_vec()).map_err(|_| corrupt("string is not UTF-8"))
    }

    fn opt_string(&mut self) -> Result<Option<String>> {
        if self.u8()? == 0 {
            Ok(None)
        } else {
            Ok(Some(self.string()?))
        }
    }

    fn opt_u32(&mut self) -> Result<Option<u32>> {
        if self.u8()? == 0 {
            Ok(None)
        } else {
            Ok(Some(self.u32()?))
        }
    }

    fn at_end(&self) -> bool {
        self.at == self.b.len()
    }
}

fn read_strs(r: &mut BinReader<'_>) -> Result<Vec<String>> {
    let n = r.u32()?;
    if n as u64 > 1 << 24 {
        return Err(corrupt("string list count is implausible"));
    }
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        out.push(r.string()?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_roundtrips_and_fingerprints_differ() {
        let p = EpubExtractProfile::DEFAULT;
        assert_eq!(EpubExtractProfile::decode(&p.encode()).unwrap(), p);
        let mut all = p;
        all.spine = SpineScope::All;
        assert_ne!(all.fingerprint(), p.fingerprint());
        assert_eq!(EpubExtractProfile::decode(&all.encode()).unwrap(), all);
    }

    #[test]
    fn resolve_member_path_clamps_and_rejects_traversal() {
        let l = Limits::DEFAULT;
        assert_eq!(
            resolve_member_path("OEBPS/", "text/c1.xhtml", l).unwrap(),
            "OEBPS/text/c1.xhtml"
        );
        assert_eq!(
            resolve_member_path("OEBPS/text/", "../img/a.png", l).unwrap(),
            "OEBPS/img/a.png"
        );
        assert_eq!(
            resolve_member_path("", "package.opf", l).unwrap(),
            "package.opf"
        );
        assert!(resolve_member_path("OEBPS/", "../../escape", l).is_err());
    }

    #[test]
    fn absolute_href_detection() {
        assert!(is_absolute_href("https://example.com/x"));
        assert!(is_absolute_href("//cdn.example.com/x"));
        assert!(!is_absolute_href("text/ch1.xhtml"));
        assert!(!is_absolute_href("../a/b.png"));
    }
}
