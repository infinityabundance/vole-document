//! Byte-based document-format detection for the universal observation API
//! (Phase 12.7, ADR-0031, plan §DEC-5).
//!
//! The format of a field is decided from its **source bytes**, never from a file
//! name or extension. Detection is deliberately conservative: an ambiguous or
//! malformed input falls back to [`DocumentFormat::Opaque`] rather than guessing,
//! and a format whose adapter is not compiled in cannot be detected (the input is
//! then `Opaque`), so the reported capability set always matches what the build
//! can actually serve.
//!
//! * **PDF** — the byte-authoritative physical scanner admits a validated PDF
//!   ([`crate::adapter::pdf::detect`]).
//! * **DOCX** — a valid ZIP that also carries the OPC content-types part
//!   (`[Content_Types].xml`) and a package `officeDocument` relationship.
//! * **EPUB** — a valid ZIP that is an OCF container: the mandatory stored
//!   `mimetype` member equals `application/epub+zip`, or `META-INF/container.xml`
//!   names the OCF namespace and an OPF (`application/oebps-package+xml`) rootfile.
//! * **ODT** — a valid ZIP that is an OpenDocument (ODF) package: the mandatory
//!   stored `mimetype` member is an OpenDocument *text* media type, or
//!   `META-INF/manifest.xml` declares one.
//! * **ODS** — a valid ZIP that is an OpenDocument (ODF) package whose mandatory
//!   stored `mimetype` member is an OpenDocument *spreadsheet* media type, or whose
//!   `META-INF/manifest.xml` declares one (Phase 21.3.1). Mutually exclusive with
//!   ODT (a text document declares the text media type, a spreadsheet the
//!   spreadsheet one).
//! * **ODP** — a valid ZIP that is an OpenDocument (ODF) package whose mandatory
//!   stored `mimetype` member is an OpenDocument *presentation* media type, or whose
//!   `META-INF/manifest.xml` declares one (Phase 21.4.1). Mutually exclusive with
//!   ODT/ODS (a presentation declares the presentation media type).
//! * **Opaque** — everything else, including a ZIP that matches none of the above
//!   (or more than one — an ambiguous ZIP fails safe).
//!
//! The detected format is recorded in the field manifest's provenance (a
//! machine-readable `format=<name>;` prefix, see [`DocumentFormat::from_provenance`]),
//! so `observe`/`find`/`explain` can dispatch common selectors without reading the
//! whole source again.

use crate::limits::Limits;

/// The mandatory OCF `mimetype` payload.
pub const EPUB_MIMETYPE: &[u8] = b"application/epub+zip";
/// The mandatory OCF container descriptor member.
pub const CONTAINER_MEMBER: &[u8] = b"META-INF/container.xml";
/// The OCF container-descriptor namespace.
pub const CONTAINER_NS: &[u8] = b"urn:oasis:names:tc:opendocument:xmlns:container";
/// The default (and only normative) package-document media type.
pub const OPF_MEDIA_TYPE: &[u8] = b"application/oebps-package+xml";
/// The OPC content-types part.
#[cfg(feature = "package")]
const CONTENT_TYPES_MEMBER: &[u8] = b"[Content_Types].xml";
/// The OPC package-relationships part.
#[cfg(feature = "package")]
const PACKAGE_RELS_MEMBER: &[u8] = b"_rels/.rels";
/// The `officeDocument` relationship type fragment (transitional and strict).
#[cfg(feature = "package")]
const OFFICE_DOCUMENT_FRAGMENT: &[u8] = b"officeDocument";
/// The WordprocessingML document main content-type fragment (Phase 21.1.2).
#[cfg(feature = "package")]
const DOCX_MAIN_FRAGMENT: &[u8] = b"wordprocessingml.document.main+xml";
/// The canonical WordprocessingML main-part target fragment (Phase 21.1.2).
#[cfg(feature = "package")]
const DOCX_MAIN_TARGET: &[u8] = b"word/document.xml";
/// The ODF package manifest member (Phase 13.3).
#[cfg(feature = "odt")]
const ODT_MANIFEST_MEMBER: &[u8] = b"META-INF/manifest.xml";
/// The OpenDocument *text* media-type fragment (Phase 13.3).
#[cfg(feature = "odt")]
const ODT_TEXT_FRAGMENT: &[u8] = b"application/vnd.oasis.opendocument.text";
/// The ODF package manifest member for the ODS detection rule (Phase 21.3.1).
#[cfg(feature = "ods")]
const ODS_MANIFEST_MEMBER: &[u8] = b"META-INF/manifest.xml";
/// The OpenDocument *spreadsheet* media-type fragment (Phase 21.3.1).
#[cfg(feature = "ods")]
const ODS_SPREADSHEET_FRAGMENT: &[u8] = b"application/vnd.oasis.opendocument.spreadsheet";
/// The ODF package manifest member for the ODP detection rule (Phase 21.4.1).
#[cfg(feature = "odp")]
const ODP_MANIFEST_MEMBER: &[u8] = b"META-INF/manifest.xml";
/// The OpenDocument *presentation* media-type fragment (Phase 21.4.1).
#[cfg(feature = "odp")]
const ODP_PRESENTATION_FRAGMENT: &[u8] = b"application/vnd.oasis.opendocument.presentation";
/// The SpreadsheetML workbook main content-type fragment (Phase 21.1.1).
#[cfg(feature = "xlsx")]
const XLSX_MAIN_FRAGMENT: &[u8] = b"spreadsheetml.sheet.main+xml";
/// The SpreadsheetML content-type namespace fragment (Phase 21.1.1).
#[cfg(feature = "xlsx")]
const XLSX_NS_FRAGMENT: &[u8] = b"spreadsheetml";
/// A SpreadsheetML workbook part-target fragment (Phase 21.1.1).
#[cfg(feature = "xlsx")]
const XLSX_WORKBOOK_TARGET: &[u8] = b"xl/workbook.xml";
/// The PresentationML presentation main content-type fragment (Phase 21.2.1).
#[cfg(feature = "pptx")]
const PPTX_MAIN_FRAGMENT: &[u8] = b"presentationml.presentation.main+xml";
/// The PresentationML content-type namespace fragment (Phase 21.2.1).
#[cfg(feature = "pptx")]
const PPTX_NS_FRAGMENT: &[u8] = b"presentationml";
/// The canonical PresentationML main-part target fragment (Phase 21.2.1).
#[cfg(feature = "pptx")]
const PPTX_MAIN_TARGET: &[u8] = b"ppt/presentation.xml";

/// A detected document format (the class of the field's source bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentFormat {
    /// A validated PDF (physical indicators).
    Pdf,
    /// An OPC package with a WordprocessingML `officeDocument` part.
    Docx,
    /// An OCF container with an EPUB package document.
    Epub,
    /// An ODF package with an OpenDocument text content part.
    Odt,
    /// An ODF package with an OpenDocument spreadsheet content part.
    Ods,
    /// An ODF package with an OpenDocument presentation content part.
    Odp,
    /// An OPC package with a SpreadsheetML workbook part.
    Xlsx,
    /// An OPC package with a PresentationML presentation part.
    Pptx,
    /// Anything else; preserved exactly by the opaque floor.
    Opaque,
}

impl DocumentFormat {
    /// Stable lower-case name (used in JSON and in the manifest provenance token).
    pub const fn name(self) -> &'static str {
        match self {
            DocumentFormat::Pdf => "pdf",
            DocumentFormat::Docx => "docx",
            DocumentFormat::Epub => "epub",
            DocumentFormat::Odt => "odt",
            DocumentFormat::Ods => "ods",
            DocumentFormat::Odp => "odp",
            DocumentFormat::Xlsx => "xlsx",
            DocumentFormat::Pptx => "pptx",
            DocumentFormat::Opaque => "opaque",
        }
    }

    /// The adapter that serves this format (the observation layer's name for it).
    pub const fn adapter(self) -> &'static str {
        match self {
            DocumentFormat::Pdf => "pdf",
            DocumentFormat::Docx => "docx",
            DocumentFormat::Epub => "epub",
            DocumentFormat::Odt => "odt",
            DocumentFormat::Ods => "ods",
            DocumentFormat::Odp => "odp",
            DocumentFormat::Xlsx => "xlsx",
            DocumentFormat::Pptx => "pptx",
            DocumentFormat::Opaque => "opaque",
        }
    }

    /// Whether the adapter for this format is compiled into this build.
    pub const fn compiled(self) -> bool {
        match self {
            DocumentFormat::Pdf | DocumentFormat::Opaque => true,
            DocumentFormat::Docx => cfg!(feature = "docx"),
            DocumentFormat::Epub => cfg!(feature = "epub"),
            DocumentFormat::Odt => cfg!(feature = "odt"),
            DocumentFormat::Ods => cfg!(feature = "ods"),
            DocumentFormat::Odp => cfg!(feature = "odp"),
            DocumentFormat::Xlsx => cfg!(feature = "xlsx"),
            DocumentFormat::Pptx => cfg!(feature = "pptx"),
        }
    }

    /// The machine-readable `format=<name>;` provenance prefix recorded at ingest.
    pub fn provenance_prefix(self) -> String {
        format!("format={};", self.name())
    }

    /// Recover the recorded format from a field manifest's provenance string.
    ///
    /// Returns `None` for a manifest that does not carry the token (e.g. a field
    /// written before this subphase); such a field still serves every native
    /// selector, but common observations decline typed rather than guessing.
    pub fn from_provenance(provenance: &str) -> Option<DocumentFormat> {
        let rest = provenance.strip_prefix("format=")?;
        let name = rest.split(';').next()?;
        match name {
            "pdf" => Some(DocumentFormat::Pdf),
            "docx" => Some(DocumentFormat::Docx),
            "epub" => Some(DocumentFormat::Epub),
            "odt" => Some(DocumentFormat::Odt),
            "ods" => Some(DocumentFormat::Ods),
            "odp" => Some(DocumentFormat::Odp),
            "xlsx" => Some(DocumentFormat::Xlsx),
            "pptx" => Some(DocumentFormat::Pptx),
            "opaque" => Some(DocumentFormat::Opaque),
            _ => None,
        }
    }
}

/// Detect the document format of `source` from its bytes alone.
///
/// Never consults a file name or extension. A malformed or ambiguous input (or a
/// format whose adapter is not compiled) falls back to [`DocumentFormat::Opaque`].
pub fn detect_document_format(source: &[u8], limits: Limits) -> DocumentFormat {
    if crate::adapter::pdf::detect(source, limits) {
        return DocumentFormat::Pdf;
    }
    #[cfg(feature = "package")]
    {
        if let Some(format) = detect_zip_family(source, limits) {
            return format;
        }
    }
    DocumentFormat::Opaque
}

/// Whether `source` is a structurally valid ZIP archive.
///
/// Used by the universal ingest dispatcher so a generic ZIP (which detection
/// reports as `Opaque`) is still inverted through the byte-authoritative package
/// layer rather than the opaque floor.
#[cfg(feature = "package")]
pub fn is_zip(source: &[u8], limits: Limits) -> bool {
    crate::adapter::package::scan(source, limits).is_ok()
}

#[cfg(feature = "package")]
fn detect_zip_family(source: &[u8], limits: Limits) -> Option<DocumentFormat> {
    let physical = crate::adapter::package::scan(source, limits).ok()?;

    // EPUB (OCF): the mandatory `mimetype` member, or an OCF container that
    // resolves to an OPF rootfile.
    let mimetype = member_decoded(&physical, source, EPUB_MIMETYPE_MEMBER, limits);
    let mimetype_ok = mimetype.as_deref() == Some(EPUB_MIMETYPE);
    let container = member_decoded(&physical, source, CONTAINER_MEMBER, limits);
    let container_ok = container
        .as_deref()
        .is_some_and(|c| contains(c, CONTAINER_NS) && contains(c, OPF_MEDIA_TYPE));
    let is_epub = mimetype_ok || container_ok;

    // DOCX: an OPC package whose content types declare a WordprocessingML main
    // part, or whose package relationships declare an `officeDocument` part that
    // targets `word/document.xml`. The positive WordprocessingML signal (rather
    // than the mere absence of a SpreadsheetML one) keeps DOCX and XLSX mutually
    // exclusive without misclassifying a Word document that *embeds* an Excel
    // workbook (whose package declares SpreadsheetML content types for the
    // embedded part, but no SpreadsheetML workbook main part). Without `xlsx`
    // and `pptx` the legacy relationship-only rule stands.
    let content_types = member_decoded(&physical, source, CONTENT_TYPES_MEMBER, limits);
    let rels = member_decoded(&physical, source, PACKAGE_RELS_MEMBER, limits);
    #[cfg(any(feature = "xlsx", feature = "pptx"))]
    let is_docx = content_types
        .as_deref()
        .is_some_and(|ct| contains(ct, DOCX_MAIN_FRAGMENT))
        || rels.as_deref().is_some_and(|r| {
            contains(r, OFFICE_DOCUMENT_FRAGMENT) && contains(r, DOCX_MAIN_TARGET)
        });
    #[cfg(not(any(feature = "xlsx", feature = "pptx")))]
    let is_docx = content_types.is_some()
        && rels
            .as_deref()
            .is_some_and(|r| contains(r, OFFICE_DOCUMENT_FRAGMENT));

    // XLSX: an OPC package whose content types declare a SpreadsheetML workbook
    // (or whose `officeDocument` relationship targets a workbook part).
    #[cfg(feature = "xlsx")]
    let is_xlsx = content_types.as_deref().is_some_and(|ct| {
        contains(ct, XLSX_MAIN_FRAGMENT)
            || (contains(ct, XLSX_NS_FRAGMENT)
                && rels
                    .as_deref()
                    .is_some_and(|r| contains(r, XLSX_WORKBOOK_TARGET)))
    });
    #[cfg(not(feature = "xlsx"))]
    let is_xlsx = false;

    // PPTX: an OPC package whose content types declare a PresentationML main part
    // (or whose content types name PresentationML and whose `officeDocument`
    // relationship targets `ppt/presentation.xml`). The positive PresentationML
    // signal keeps it mutually exclusive with DOCX and XLSX: a Word/Excel document
    // that *embeds* a PowerPoint part declares only the PresentationML embed type
    // (`…presentationml.presentation`, not the `.main+xml` main part) and its
    // `officeDocument` relationship targets `word/document.xml`/`xl/workbook.xml`,
    // so it is never misclassified as PPTX.
    #[cfg(feature = "pptx")]
    let is_pptx = content_types.as_deref().is_some_and(|ct| {
        contains(ct, PPTX_MAIN_FRAGMENT)
            || (contains(ct, PPTX_NS_FRAGMENT)
                && rels
                    .as_deref()
                    .is_some_and(|r| contains(r, PPTX_MAIN_TARGET)))
    });
    #[cfg(not(feature = "pptx"))]
    let is_pptx = false;

    // ODT: an ODF package whose mandatory `mimetype` (or `META-INF/manifest.xml`)
    // declares an OpenDocument text media type.
    #[cfg(feature = "odt")]
    let is_odt = mimetype
        .as_deref()
        .is_some_and(|m| contains(m, ODT_TEXT_FRAGMENT))
        || member_decoded(&physical, source, ODT_MANIFEST_MEMBER, limits)
            .as_deref()
            .is_some_and(|m| contains(m, ODT_TEXT_FRAGMENT));
    #[cfg(not(feature = "odt"))]
    let is_odt = false;

    // ODS: an ODF package whose mandatory `mimetype` (or `META-INF/manifest.xml`)
    // declares an OpenDocument *spreadsheet* media type. The positive spreadsheet
    // fragment keeps it mutually exclusive with ODT (a text document declares the
    // text media type, never the spreadsheet one).
    #[cfg(feature = "ods")]
    let is_ods = mimetype
        .as_deref()
        .is_some_and(|m| contains(m, ODS_SPREADSHEET_FRAGMENT))
        || member_decoded(&physical, source, ODS_MANIFEST_MEMBER, limits)
            .as_deref()
            .is_some_and(|m| contains(m, ODS_SPREADSHEET_FRAGMENT));
    #[cfg(not(feature = "ods"))]
    let is_ods = false;

    // ODP: an ODF package whose mandatory `mimetype` (or `META-INF/manifest.xml`)
    // declares an OpenDocument *presentation* media type. The positive presentation
    // fragment keeps it mutually exclusive with ODT/ODS.
    #[cfg(feature = "odp")]
    let is_odp = mimetype
        .as_deref()
        .is_some_and(|m| contains(m, ODP_PRESENTATION_FRAGMENT))
        || member_decoded(&physical, source, ODP_MANIFEST_MEMBER, limits)
            .as_deref()
            .is_some_and(|m| contains(m, ODP_PRESENTATION_FRAGMENT));
    #[cfg(not(feature = "odp"))]
    let is_odp = false;

    // A ZIP matching more than one native signature is ambiguous: fail safe.
    let matches = [is_docx, is_epub, is_odt, is_ods, is_odp, is_xlsx, is_pptx]
        .iter()
        .filter(|b| **b)
        .count();
    match matches {
        1 if is_docx => Some(DocumentFormat::Docx),
        1 if is_epub => Some(DocumentFormat::Epub),
        1 if is_odt => Some(DocumentFormat::Odt),
        1 if is_ods => Some(DocumentFormat::Ods),
        1 if is_odp => Some(DocumentFormat::Odp),
        1 if is_xlsx => Some(DocumentFormat::Xlsx),
        1 if is_pptx => Some(DocumentFormat::Pptx),
        _ => None,
    }
}

#[cfg(feature = "package")]
const EPUB_MIMETYPE_MEMBER: &[u8] = b"mimetype";

/// Decode one member's bytes by exact name, bounded and decline-safe: encrypted,
/// oversized, unsupported-method, or out-of-range members yield `None`.
#[cfg(feature = "package")]
fn member_decoded(
    physical: &crate::adapter::package::ZipPhysical,
    source: &[u8],
    name: &[u8],
    limits: Limits,
) -> Option<Vec<u8>> {
    const FLAG_ENCRYPTED: u16 = 0x0001;
    let member = physical.members.iter().find(|m| m.name == name)?;
    if member.flags & FLAG_ENCRYPTED != 0 || member.uncompressed_size > limits.max_xml_part_bytes {
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

/// Byte-substring search (no allocation, case-sensitive).
#[cfg(feature = "package")]
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && needle.len() <= haystack.len()
        && haystack.windows(needle.len()).any(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_provenance_roundtrip() {
        for f in [
            DocumentFormat::Pdf,
            DocumentFormat::Docx,
            DocumentFormat::Epub,
            DocumentFormat::Odt,
            DocumentFormat::Ods,
            DocumentFormat::Odp,
            DocumentFormat::Xlsx,
            DocumentFormat::Pptx,
            DocumentFormat::Opaque,
        ] {
            let token = format!("{}field:package;members=1", f.provenance_prefix());
            assert_eq!(DocumentFormat::from_provenance(&token), Some(f));
        }
        assert_eq!(DocumentFormat::from_provenance("field:ingest-b"), None);
        assert_eq!(DocumentFormat::from_provenance("format=exotic;x"), None);
    }

    #[test]
    fn plain_bytes_are_opaque() {
        assert_eq!(
            detect_document_format(b"not a document", Limits::DEFAULT),
            DocumentFormat::Opaque
        );
    }

    #[test]
    fn a_corpus_pdf_is_detected() {
        let (_, pdf) = crate::adapter::pdf::sample_pdfs()
            .into_iter()
            .next()
            .expect("the PDF corpus is non-empty");
        assert_eq!(
            detect_document_format(&pdf, Limits::DEFAULT),
            DocumentFormat::Pdf
        );
    }
}
