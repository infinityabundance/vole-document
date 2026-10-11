//! Phase 21.33 court: the generic archive-container adapter (ZIP and TAR).
//!
//! Gated on `container` (which implies `package`), so a build without the feature
//! compiles an empty target.
//!
//! A container has no package layer: the exact leaf is the whole source (a
//! `DocumentExact`) and every derived observation is a bounded, representation-
//! preserving projection of it. The court checks:
//!
//! 1. **Byte-based detection** and the boundaries: a plain ZIP → `Container`, a
//!    plain TAR → `Container`; an office `.docx`/`.odt` source still detects as its
//!    office format (never reclassified); a random `PK`-junk blob and prose stay
//!    `Opaque`.
//! 2. **Representation preservation**: the ordered member list with raw names
//!    (incl. non-UTF-8), method, declared sizes, CRC-32, and exact spans; a stored
//!    and a deflated ZIP member; a ustar TAR with files + a directory; a GNU
//!    long-name + PAX-extension TAR; the empty ZIP.
//! 3. **Selectors**: native `container-dialect`/`container-entry`/`container-find`
//!    plus common `metadata`/`text`/`search-match`.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 5. **Fail closed**: malformed inputs decline typed (bad EOCD, truncated member,
//!    bad TAR checksum); random bytes never panic.
//! 6. **No cross-field aliasing** (ADR-0060): the model node depends on the exact
//!    `sha256(source)` root.

#![cfg(all(feature = "field", feature = "container"))]

use std::path::PathBuf;

use vole_document::adapter::package::crc32_iso_hdlc;
use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_CONTAINER_MODEL, SelectorKey, lookup};
use vole_document::field::ingest::{IngestReport, ingest_pdf};
use vole_document::field::node::NodeKind;
use vole_document::field::observe::{ObserveRequest, Representation, Selector, observe};
use vole_document::field::provenance::{AnswerValue, FieldAnswer};
use vole_document::field::{Field, FieldId, FieldStore};
use vole_document::integrity::sha256;
use vole_document::limits::Limits;
use vole_document::store::NodeId;

// ---------------------------------------------------------------------------
// Dependency-free ZIP writer (test ground truth)
// ---------------------------------------------------------------------------

fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// A valid single-block raw DEFLATE stream that stores `data` uncompressed.
fn stored_deflate(data: &[u8]) -> Vec<u8> {
    assert!(
        data.len() <= 0xFFFF,
        "test deflate helper is single-block only"
    );
    let mut out = Vec::with_capacity(data.len() + 5);
    out.push(0x01); // BFINAL=1, BTYPE=00 (stored)
    let len = data.len() as u16;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&(!len).to_le_bytes());
    out.extend_from_slice(data);
    out
}

/// Build a ZIP from `(name, content, method)` triples (method 0 stored, 8 deflate).
/// UTF-8 flag bit 11 is set when a name byte is `>= 0x80` (never a decode).
fn build_zip(entries: &[(&[u8], &[u8], u16)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut offsets: Vec<u32> = Vec::new();
    let mut crcs: Vec<u32> = Vec::new();
    let mut payloads: Vec<Vec<u8>> = Vec::new();
    for (name, content, method) in entries {
        offsets.push(out.len() as u32);
        crcs.push(crc32_iso_hdlc(content));
        let payload = if *method == 8 {
            stored_deflate(content)
        } else {
            content.to_vec()
        };
        payloads.push(payload.clone());
        let mut flags = 0u16;
        if name.iter().any(|&b| b >= 0x80) {
            flags |= 0x0800;
        }
        put_u32(&mut out, 0x0403_4b50);
        put_u16(&mut out, 20); // version needed
        put_u16(&mut out, flags);
        put_u16(&mut out, *method);
        put_u16(&mut out, 0); // MS-DOS time
        put_u16(&mut out, 0x0021); // MS-DOS date
        put_u32(&mut out, crcs[offsets.len() - 1]);
        put_u32(&mut out, payload.len() as u32);
        put_u32(&mut out, content.len() as u32);
        put_u16(&mut out, name.len() as u16);
        put_u16(&mut out, 0); // extra len
        out.extend_from_slice(name);
        out.extend_from_slice(&payload);
    }
    let cd_offset = out.len() as u32;
    for (i, (name, content, method)) in entries.iter().enumerate() {
        let mut flags = 0u16;
        if name.iter().any(|&b| b >= 0x80) {
            flags |= 0x0800;
        }
        put_u32(&mut out, 0x0201_4b50);
        put_u16(&mut out, 20); // version made by
        put_u16(&mut out, 20); // version needed
        put_u16(&mut out, flags);
        put_u16(&mut out, *method);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0x0021);
        put_u32(&mut out, crcs[i]);
        put_u32(&mut out, payloads[i].len() as u32);
        put_u32(&mut out, content.len() as u32);
        put_u16(&mut out, name.len() as u16);
        put_u16(&mut out, 0); // extra len
        put_u16(&mut out, 0); // comment len
        put_u16(&mut out, 0); // disk start
        put_u16(&mut out, 0); // internal attrs
        put_u32(&mut out, 0); // external attrs
        put_u32(&mut out, offsets[i]);
        out.extend_from_slice(name);
    }
    let cd_size = out.len() as u32 - cd_offset;
    put_u32(&mut out, 0x0605_4b50);
    put_u16(&mut out, 0);
    put_u16(&mut out, 0);
    put_u16(&mut out, entries.len() as u16);
    put_u16(&mut out, entries.len() as u16);
    put_u32(&mut out, cd_size);
    put_u32(&mut out, cd_offset);
    put_u16(&mut out, 0); // comment len
    out
}

// ---------------------------------------------------------------------------
// Dependency-free TAR writer (test ground truth)
// ---------------------------------------------------------------------------

fn put_octal(field: &mut [u8], v: u64) {
    let digits = field.len() - 1;
    let s = format!("{v:0>digits$o}");
    let bytes = s.as_bytes();
    field[..digits].copy_from_slice(&bytes[bytes.len() - digits..]);
    field[digits] = 0;
}

fn tar_header(
    name: &[u8],
    mode: u64,
    size: u64,
    typeflag: u8,
    linkname: &[u8],
    ustar: bool,
) -> Vec<u8> {
    let mut h = vec![0u8; 512];
    let n = name.len().min(100);
    h[..n].copy_from_slice(&name[..n]);
    put_octal(&mut h[100..108], mode);
    put_octal(&mut h[108..116], 0);
    put_octal(&mut h[116..124], 0);
    put_octal(&mut h[124..136], size);
    put_octal(&mut h[136..148], 0);
    for b in &mut h[148..156] {
        *b = b' ';
    }
    h[156] = typeflag;
    let ln = linkname.len().min(100);
    h[157..157 + ln].copy_from_slice(&linkname[..ln]);
    if ustar {
        h[257..263].copy_from_slice(b"ustar\0");
        h[263..265].copy_from_slice(b"00");
    }
    let sum: u64 = h.iter().map(|&b| b as u64).sum();
    let s = format!("{sum:06o}");
    h[148..154].copy_from_slice(s.as_bytes());
    h[154] = 0;
    h[155] = b' ';
    h
}

fn padded(size: u64) -> u64 {
    size.saturating_add(511) & !511
}

/// Append one TAR member (header + padded data) to `out`.
fn push_tar_member(
    out: &mut Vec<u8>,
    name: &[u8],
    typeflag: u8,
    content: &[u8],
    linkname: &[u8],
    ustar: bool,
) {
    out.extend_from_slice(&tar_header(
        name,
        if typeflag == b'5' { 0o755 } else { 0o644 },
        content.len() as u64,
        typeflag,
        linkname,
        ustar,
    ));
    out.extend_from_slice(content);
    let pad = padded(content.len() as u64) as usize - content.len();
    out.extend(std::iter::repeat_n(0u8, pad));
}

fn pax_record(key: &str, val: &str) -> Vec<u8> {
    let rest = format!(" {key}={val}\n");
    let mut len = rest.len() + 1;
    loop {
        let total = len.to_string().len() + rest.len();
        if total == len {
            break;
        }
        len = total;
    }
    let mut out = len.to_string().into_bytes();
    out.extend_from_slice(rest.as_bytes());
    out
}

/// A ustar TAR with a regular file, a directory, and a second file.
fn build_tar_ustar() -> Vec<u8> {
    let mut out = Vec::new();
    push_tar_member(
        &mut out,
        b"hello.txt",
        b'0',
        b"hello tar world\n",
        b"",
        true,
    );
    push_tar_member(&mut out, b"dir/", b'5', b"", b"", true);
    push_tar_member(
        &mut out,
        b"dir/nested.bin",
        b'0',
        &[0u8, 1, 2, 3, 4, 5],
        b"",
        true,
    );
    // Two zero blocks end the archive.
    out.extend(std::iter::repeat_n(0u8, 1024));
    out
}

/// A ustar TAR using a GNU long-name (`L`) extension for the member name, a GNU
/// long-link (`K`) extension, and a PAX (`x`) `path` override for another member.
fn build_tar_extended() -> Vec<u8> {
    let long = b"this/is/a/very/long/member/name/that/does/not/fit/in/one/hundred/bytes/somewhere/deeper/still/further.txt";
    assert!(long.len() > 100);
    let mut out = Vec::new();
    // GNU long name for the first member.
    {
        let mut namez = long.to_vec();
        namez.push(0);
        push_tar_member(&mut out, b"././@LongLink", b'L', &namez, b"", true);
    }
    push_tar_member(&mut out, &long[..100], b'0', b"payload-L\n", b"", true);
    // PAX path override for the second member (short header name).
    {
        let rec = pax_record("path", "pax/overridden-name.txt");
        push_tar_member(&mut out, b"paxheader", b'x', &rec, b"", true);
    }
    push_tar_member(&mut out, b"short-name", b'0', b"payload-X\n", b"", true);
    // A symlink with a linkname.
    push_tar_member(
        &mut out,
        b"link",
        b'2',
        b"",
        b"pax/overridden-name.txt",
        true,
    );
    out.extend(std::iter::repeat_n(0u8, 1024));
    out
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-container-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn opaque_descriptor(source: &[u8]) -> Vec<u8> {
    let d = Descriptor {
        universe: vole_document::container::UNIVERSE.to_string(),
        source_format: vole_document::SOURCE_FORMAT_OPAQUE,
        format_basis: "opaque:container-test".to_string(),
        models: vec![],
        channels: vec![],
        objects: vec![ObjectSource::Inline(source.to_vec())],
        program: Program::new(vec![Op::EmitObject { object_id: 0 }]),
        observation_index: None,
        seek_directory: false,
        checkpoints: None,
        source_sha256: sha256(source),
        source_len: source.len() as u64,
    };
    d.serialize().unwrap().0
}

struct Fixture {
    root: PathBuf,
    store: FieldStore,
    report: IngestReport,
    source: Vec<u8>,
}

impl Fixture {
    fn new(label: &str, source: &[u8]) -> Fixture {
        let root = temp_dir(label);
        let mut store = FieldStore::open(&root).unwrap();
        let descriptor = opaque_descriptor(source);
        let report = ingest_pdf(&mut store, &descriptor, Limits::DEFAULT).unwrap();
        Fixture {
            root,
            store,
            report,
            source: source.to_vec(),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

fn observe_ok(
    store: &mut FieldStore,
    field: &FieldId,
    selector: Selector,
    representation: Representation,
) -> FieldAnswer {
    let req = ObserveRequest::new(selector, representation);
    observe(store, field, &req, Limits::DEFAULT).unwrap().0
}

fn observe_err(
    store: &mut FieldStore,
    field: &FieldId,
    selector: Selector,
    representation: Representation,
) -> ErrorClass {
    let req = ObserveRequest::new(selector, representation);
    observe(store, field, &req, Limits::DEFAULT)
        .unwrap_err()
        .class()
}

fn answer_text(answer: &FieldAnswer) -> String {
    match &answer.value {
        AnswerValue::Text(t) => t.clone(),
        other => panic!("expected text, got {other:?}"),
    }
}

fn answer_json(answer: &FieldAnswer) -> String {
    match &answer.value {
        AnswerValue::Json(j) => j.clone(),
        other => panic!("expected json, got {other:?}"),
    }
}

fn answer_bytes(answer: &FieldAnswer) -> Vec<u8> {
    match &answer.value {
        AnswerValue::Bytes(b) => b.clone(),
        other => panic!("expected bytes, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_byte_based_and_conservative() {
    let zip = build_zip(&[(b"hello.txt", b"hello zip world\n", 0)]);
    assert_eq!(
        detect_document_format(&zip, Limits::DEFAULT),
        DocumentFormat::Container
    );
    let tar = build_tar_ustar();
    assert_eq!(
        detect_document_format(&tar, Limits::DEFAULT),
        DocumentFormat::Container
    );
    let empty_zip = build_zip(&[]);
    assert_eq!(
        detect_document_format(&empty_zip, Limits::DEFAULT),
        DocumentFormat::Container
    );

    // A random `PK`-prefixed blob without a valid EOCD is NOT a container.
    let mut junk = b"PK\x03\x04".to_vec();
    junk.extend(std::iter::repeat_n(0x41u8, 4096));
    assert_eq!(
        detect_document_format(&junk, Limits::DEFAULT),
        DocumentFormat::Opaque
    );

    // Prose stays Opaque.
    assert_eq!(
        detect_document_format(
            b"The quick brown fox jumps over the lazy dog.\nPlain prose, not an archive.\n",
            Limits::DEFAULT
        ),
        DocumentFormat::Opaque
    );
}

#[test]
#[cfg(all(feature = "docx", feature = "odt"))]
fn office_zip_family_is_never_reclassified() {
    // A `.docx` (an OPC package with a WordprocessingML main content type) must stay
    // `Docx`, never a plain container. The `_rels/.rels` relationship keeps this true
    // even when the `xlsx`/`pptx` features are off (the legacy relationship-only rule).
    let docx = build_zip(&[
        (
            b"[Content_Types].xml",
            br#"<Types><Override ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml" PartName="/word/document.xml"/></Types>"#,
            0,
        ),
        (
            b"_rels/.rels",
            br#"<Relationships><Relationship Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
            0,
        ),
        (b"word/document.xml", b"<w:document/>", 0),
    ]);
    assert_eq!(
        detect_document_format(&docx, Limits::DEFAULT),
        DocumentFormat::Docx
    );

    // A `.odt` (an ODF package with the OpenDocument text media type) must stay `Odt`.
    let odt = build_zip(&[
        (b"mimetype", b"application/vnd.oasis.opendocument.text", 0),
        (b"content.xml", b"<office:document-content/>", 0),
    ]);
    assert_eq!(
        detect_document_format(&odt, Limits::DEFAULT),
        DocumentFormat::Odt
    );
}

#[test]
fn zip_members_preserve_names_methods_sizes_and_spans() {
    let zip = build_zip(&[
        (b"hello.txt", b"hello zip world\n", 0),
        (b"data.bin", &[0u8; 300], 8),
    ]);
    let mut fx = Fixture::new("zipmodel", &zip);
    let field = fx.report.field;

    // Dialect.
    let dialect = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::ContainerDialect,
        Representation::Text,
    ));
    assert_eq!(dialect, "zip");

    // The stored member 0: ExactBytes is the exact raw span (local header + payload).
    let raw0 = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        Selector::ContainerEntry { index: 0 },
        Representation::ExactBytes,
    ));
    assert!(
        raw0.starts_with(b"PK\x03\x04"),
        "member span must start at the LFH"
    );
    assert!(raw0.windows(9).any(|w| w == b"hello zip"));
    let meta0 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::ContainerEntry { index: 0 },
        Representation::Metadata,
    ));
    assert!(meta0.contains("\"method\":0"), "{meta0}");
    assert!(meta0.contains("\"kind\":\"stored\""), "{meta0}");
    assert!(meta0.contains("\"name\":\"hello.txt\""), "{meta0}");

    // The deflated member 1: method 8 preserved verbatim, raw compressed payload.
    let meta1 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::ContainerEntry { index: 1 },
        Representation::Metadata,
    ));
    assert!(meta1.contains("\"method\":8"), "{meta1}");
    assert!(meta1.contains("\"kind\":\"deflate\""), "{meta1}");
    assert!(meta1.contains("\"uncompressed_size\":300"), "{meta1}");

    // An out-of-range index declines typed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::ContainerEntry { index: 9 },
            Representation::Metadata
        ),
        ErrorClass::UnsupportedFeature
    );

    // Find resolves member names; the common surface answers.
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::ContainerFind {
            pattern: "zip".to_string(),
        },
        Representation::Metadata,
    ));
    assert!(found.contains("\"matches\":["), "{found}");
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert!(text.contains("hello.txt"), "{text}");
    let common = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(common.contains("\"format\":\"container\""), "{common}");
    assert!(common.contains("\"dialect\":\"zip\""), "{common}");
}

#[test]
fn zip_non_utf8_name_is_preserved_verbatim() {
    // A raw name with bytes >= 0x80 (flag bit 11 set) must be preserved exactly; the
    // text view is lossy-only and never changes the exact leaf.
    let raw_name: &[u8] = b"caf\xe9/n\xf1.txt";
    let zip = build_zip(&[(raw_name, b"body\n", 0)]);
    let mut fx = Fixture::new("zipnonutf8", &zip);
    let field = fx.report.field;
    let name = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::ContainerEntry { index: 0 },
        Representation::Text,
    ));
    // Lossy: the 0xe9 / 0xf1 bytes become U+FFFD, but there is one member and it is
    // byte-preserved in the model (checked through the exact leaf below).
    assert_eq!(name.matches('\u{FFFD}').count(), 2, "{name:?}");
    let field_handle = Field::open(&fx.store, &field, Limits::DEFAULT).unwrap();
    assert_eq!(
        field_handle.materialize_exact(Limits::DEFAULT).unwrap(),
        fx.source
    );
}

#[test]
fn tar_ustar_members_and_directory() {
    let tar = build_tar_ustar();
    let mut fx = Fixture::new("tarustar", &tar);
    let field = fx.report.field;

    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::ContainerDialect,
            Representation::Text,
        )),
        "tar"
    );

    // Member 0 ExactBytes is the exact 512-byte header block + padded payload.
    let raw0 = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        Selector::ContainerEntry { index: 0 },
        Representation::ExactBytes,
    ));
    assert_eq!(raw0, tar[0..1024].to_vec());

    // The directory member (index 1) is recorded as a directory.
    let meta1 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::ContainerEntry { index: 1 },
        Representation::Metadata,
    ));
    assert!(meta1.contains("\"kind\":\"directory\""), "{meta1}");
    assert!(meta1.contains("\"directory\":true"), "{meta1}");
    assert!(meta1.contains("\"typeflag\":53"), "{meta1}"); // ASCII '5'
}

#[test]
fn tar_gnu_and_pax_extensions_are_folded_in() {
    let tar = build_tar_extended();
    let mut fx = Fixture::new("tarext", &tar);
    let field = fx.report.field;

    // Member 0 uses the GNU long name (`L`).
    let meta0 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::ContainerEntry { index: 0 },
        Representation::Metadata,
    ));
    assert!(meta0.contains("this/is/a/very/long/member/name"), "{meta0}");
    assert!(meta0.contains("\"ext\":2"), "GNU long-name bit: {meta0}");

    // Member 1 uses a PAX (`x`) path override.
    let meta1 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::ContainerEntry { index: 1 },
        Representation::Metadata,
    ));
    assert!(meta1.contains("pax/overridden-name.txt"), "{meta1}");
    assert!(meta1.contains("\"ext\":1"), "PAX bit: {meta1}");

    // The symlink's linkname is preserved.
    let meta2 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::ContainerEntry { index: 2 },
        Representation::Metadata,
    ));
    assert!(meta2.contains("pax/overridden-name.txt"), "{meta2}");
    assert!(meta2.contains("\"kind\":\"symlink\""), "{meta2}");
}

#[test]
fn empty_zip_has_zero_members() {
    let zip = build_zip(&[]);
    let mut fx = Fixture::new("emptyzip", &zip);
    let field = fx.report.field;
    let common = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(common.contains("\"members\":0"), "{common}");
    assert!(common.contains("\"dialect\":\"zip\""), "{common}");
}

#[test]
fn malformed_inputs_decline_typed() {
    // A bad EOCD: a valid ZIP with the EOCD signature destroyed.
    let mut bad_eocd = build_zip(&[(b"a.txt", b"a", 0)]);
    let n = bad_eocd.len();
    bad_eocd[n - 22..n - 18].copy_from_slice(b"XXXX");
    assert_ne!(
        detect_document_format(&bad_eocd, Limits::DEFAULT),
        DocumentFormat::Container
    );
    let e = vole_document::adapter::container::parse(&bad_eocd, Limits::DEFAULT).unwrap_err();
    assert_eq!(e.class(), ErrorClass::InvalidContainerStructure);

    // A truncated member: a valid ZIP cut in half (its cover/central directory no
    // longer resolves).
    let good = build_zip(&[(b"a.txt", b"the payload", 0)]);
    let half = &good[..good.len() / 2];
    assert_ne!(
        detect_document_format(half, Limits::DEFAULT),
        DocumentFormat::Container
    );
    assert!(vole_document::adapter::container::parse(half, Limits::DEFAULT).is_err());

    // A bad TAR checksum: a valid TAR header with one non-checksum byte flipped.
    let mut bad_tar = build_tar_ustar();
    bad_tar[0] ^= 0x01; // "hello.txt" -> "iel..." changes the byte sum
    assert_ne!(
        detect_document_format(&bad_tar, Limits::DEFAULT),
        DocumentFormat::Container
    );
    let e = vole_document::adapter::container::parse(&bad_tar, Limits::DEFAULT).unwrap_err();
    assert_eq!(e.class(), ErrorClass::InvalidContainerStructure);

    // A truncated TAR (header block cut short).
    let trunc = &build_tar_ustar()[..600];
    let e = vole_document::adapter::container::parse(trunc, Limits::DEFAULT).unwrap_err();
    assert_eq!(e.class(), ErrorClass::InvalidContainerStructure);
}

#[test]
fn exact_materialization_is_byte_identical() {
    for (label, src) in [
        (
            "exact-zip",
            build_zip(&[(b"a.txt", b"alpha", 0), (b"b.bin", &[7u8; 200], 8)]),
        ),
        ("exact-tar", build_tar_ustar()),
        ("exact-tar-ext", build_tar_extended()),
    ] {
        let fx = Fixture::new(label, &src);
        let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
        let out = field.materialize_exact(Limits::DEFAULT).unwrap();
        assert_eq!(out.len(), src.len(), "{label}");
        assert_eq!(sha256(&out), sha256(&src), "{label}");
        assert_eq!(out, src, "{label}");
    }
}

#[test]
fn removal_then_fresh_process_is_queryable_and_exact() {
    let root = temp_dir("removal");
    let src_path = root.join("archive.zip");
    let desc_path = root.join("archive.voldoc");
    let source = build_zip(&[(b"one.txt", b"first\n", 0), (b"two.txt", b"second\n", 0)]);
    std::fs::write(&src_path, &source).unwrap();
    let descriptor = opaque_descriptor(&source);
    std::fs::write(&desc_path, &descriptor).unwrap();

    let store_root = root.join("store");
    let field_id = {
        let mut store = FieldStore::open(&store_root).unwrap();
        let report = ingest_pdf(&mut store, &descriptor, Limits::DEFAULT).unwrap();
        report.field
        // store drops here: a fresh process reopens the directory.
    };

    std::fs::remove_file(&src_path).unwrap();
    std::fs::remove_file(&desc_path).unwrap();

    let mut store2 = FieldStore::open(&store_root).unwrap();
    let dialect = answer_text(&observe_ok(
        &mut store2,
        &field_id,
        Selector::ContainerDialect,
        Representation::Text,
    ));
    assert_eq!(dialect, "zip");
    let raw = answer_bytes(&observe_ok(
        &mut store2,
        &field_id,
        Selector::ContainerEntry { index: 0 },
        Representation::ExactBytes,
    ));
    assert!(raw.starts_with(b"PK\x03\x04"));

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), source.len());
    assert_eq!(sha256(&exact), sha256(&source));
    assert_eq!(exact, source);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn container_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived container model reads the source bytes, so its single
    // dependency must be the exact `DocumentExact` root (keyed by `sha256(source)`).
    let source = build_zip(&[(b"k.txt", b"v", 0)]);
    let fx = Fixture::new("adr0060", &source);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let manifest = field.manifest().clone();
    let root = manifest.root_node;
    let root_node = load_node(fx.store.seeds(), &root).unwrap();
    assert_eq!(root_node.kind, NodeKind::DocumentExact);

    assert!(manifest.has_index());
    let istore = FsIndexStore::open(fx.store.root()).unwrap();
    let iroot = NodeId::from_bytes(manifest.index_root);
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_CONTAINER_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::ContainerModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the container model must depend on exactly the exact root"
    );
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0xC0FF_EE21_3344_5566;
    for _ in 0..256 {
        let mut buf = vec![0u8; 512];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        let _ = vole_document::adapter::container::detect(&buf, Limits::STRICT);
        let _ = vole_document::adapter::container::parse(&buf, Limits::STRICT);
    }
    // `PK`-prefixed junk is the most likely false-positive path; fuzz it too.
    for _ in 0..256 {
        let mut buf = b"PK\x03\x04".to_vec();
        buf.resize(256, 0);
        for b in buf.iter_mut().skip(4) {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        let _ = vole_document::adapter::container::parse(&buf, Limits::STRICT);
    }
}
