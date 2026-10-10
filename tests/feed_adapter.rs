//! Phase 21.21 court: the RSS 2.0 / Atom 1.0 feed adapter.
//!
//! Gated on `field` + `feed` (which implies `xml`), so a build without the feature
//! compiles an empty target.
//!
//! A feed is XML, so its physical bytes are shared with the standalone XML adapter.
//! The distinct claim is a **bounded semantic sub-detection** run before the generic
//! XML detector. The court checks:
//!
//! 1. **Byte-based detection and the boundaries**: an RSS `<rss><channel>` document
//!    → `Feed`; an Atom `<feed xmlns="…/Atom">` with an `<entry>` → `Feed`; a plain
//!    XML document stays `Xml`; an `<rss>`-shaped-but-invalid document and a
//!    non-Atom `<feed>` stay `Xml`; prose stays `Opaque`.
//! 2. **Representation preservation**: the recorded dialect; exact element/attribute
//!    spans; element order; attribute spelling (Atom `<link href=… rel=…>`, RSS
//!    `<guid isPermaLink=…>`); and the Atom namespace declaration.
//! 3. **Selectors**: native `feed-channel`/`feed-field`/`feed-entry`/
//!    `feed-entry-field`/`feed-find` plus common `metadata`/`text`/`search-match`.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 5. **Fail closed**: malformed declines typed; random bytes never panic.
//! 6. **No cross-field aliasing** (ADR-0060): the model node depends on the exact
//!    `sha256(source)` root.

#![cfg(all(feature = "field", feature = "feed"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_FEED_MODEL, SelectorKey, lookup};
use vole_document::field::ingest::{IngestReport, ingest_pdf};
use vole_document::field::node::NodeKind;
use vole_document::field::observe::{ObserveRequest, Representation, Selector, observe};
use vole_document::field::provenance::{AnswerValue, FieldAnswer};
use vole_document::field::{Field, FieldId, FieldStore};
use vole_document::integrity::sha256;
use vole_document::limits::Limits;
use vole_document::store::NodeId;

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-feed-{label}-{}-{}",
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
        format_basis: "opaque:feed-test".to_string(),
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

/// An RSS 2.0 feed: a `<channel>`, four channel fields, two `<item>` records with
/// the item vocabulary, and a `<guid isPermaLink="false">` attribute.
const RSS: &[u8] = b"<?xml version=\"1.0\"?>\n<rss version=\"2.0\">\n<channel>\n\
<title>Example Feed</title>\n<link>https://example.com/</link>\n\
<description>An example</description>\n<language>en</language>\n\
<item>\n<title>First</title>\n<link>https://example.com/1</link>\n\
<description>one</description>\n<pubDate>Mon, 01 Jan 2024 00:00:00 GMT</pubDate>\n\
<guid isPermaLink=\"false\">urn:1</guid>\n<category>news</category>\n</item>\n\
<item>\n<title>Second</title>\n<guid>urn:2</guid>\n</item>\n</channel>\n</rss>\n";

/// An Atom 1.0 feed: the Atom namespace declaration, four feed fields (incl. a
/// `<link href=… rel=…>`), and two `<entry>` records.
const ATOM: &[u8] = b"<?xml version=\"1.0\"?>\n\
<feed xmlns=\"http://www.w3.org/2005/Atom\">\n<id>urn:feed:1</id>\n\
<title>Example</title>\n<updated>2024-01-01T00:00:00Z</updated>\n\
<link href=\"https://example.com/\" rel=\"alternate\"/>\n\
<entry>\n<id>urn:entry:1</id>\n<title>First</title>\n\
<link href=\"https://example.com/1\"/>\n<updated>2024-01-01T00:00:00Z</updated>\n\
<summary>one</summary>\n<content type=\"html\">&lt;b&gt;one&lt;/b&gt;</content>\n</entry>\n\
<entry>\n<id>urn:entry:2</id>\n<title>Second</title>\n\
<link href=\"https://example.com/2\"/>\n</entry>\n</feed>\n";

/// A plain XML document: not a feed (stays `Xml`).
const PLAIN_XML: &[u8] = b"<note><to>Tove</to><from>Jani</from></note>";
/// An `<rss>` root with no `<channel>`: not a feed (stays `Xml`).
const NO_CHANNEL: &[u8] = b"<rss version=\"2.0\"></rss>";
/// A `<feed>` with no Atom namespace: not a feed (stays `Xml`).
const NO_NS: &[u8] = b"<feed><entry><id>x</id></entry></feed>";
/// Atom `<feed>` with no `<entry>`: not a feed (stays `Xml`).
const NO_ENTRY: &[u8] = b"<feed xmlns=\"http://www.w3.org/2005/Atom\"><id>x</id></feed>";
/// Plain prose: never a feed.
const PROSE: &[u8] = b"The quick brown fox jumps over the lazy dog.\nPlain prose, not a feed.\n";

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_byte_based_and_conservative() {
    for (bytes, want) in [
        (RSS, DocumentFormat::Feed),
        (ATOM, DocumentFormat::Feed),
        // The documented boundaries: plain XML / invalid shapes stay `Xml`; prose
        // stays `Opaque`. (The `xml` feature is implied by `feed`.)
        (PLAIN_XML, DocumentFormat::Xml),
        (NO_CHANNEL, DocumentFormat::Xml),
        (NO_NS, DocumentFormat::Xml),
        (NO_ENTRY, DocumentFormat::Xml),
        (PROSE, DocumentFormat::Opaque),
    ] {
        assert_eq!(
            detect_document_format(bytes, Limits::DEFAULT),
            want,
            "{bytes:?}"
        );
    }

    let caps = capabilities_for_format(DocumentFormat::Feed);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for n in [
        "feed-channel",
        "feed-field",
        "feed-entry",
        "feed-entry-field",
        "feed-find",
    ] {
        assert!(caps.native_selectors.contains(&n), "missing {n}");
    }
    assert!(caps.to_json().contains("\"format\":\"feed\""));
    // The fresh exit code is stable and distinct.
    assert_eq!(ErrorClass::InvalidFeedStructure.exit_code(), 35);
}

#[test]
fn rss_preserves_spans_order_and_attributes() {
    let m = vole_document::adapter::feed::parse(RSS, Limits::DEFAULT, true).unwrap();
    assert_eq!(m.dialect, vole_document::adapter::feed::DIALECT_RSS);
    assert_eq!(m.entry_count(), 2);
    let ch = vole_document::adapter::feed::channel_field_nodes(&m, RSS).unwrap();
    assert_eq!(ch.len(), 4);
    assert_eq!(
        vole_document::adapter::feed::field_bytes(&m, RSS, ch[0]).unwrap(),
        b"<title>Example Feed</title>"
    );
    // Records are in document order.
    assert!(m.entries[0] < m.entries[1]);
    let e0 = vole_document::adapter::feed::entry_field_nodes(&m, RSS, 0).unwrap();
    let e1 = vole_document::adapter::feed::entry_field_nodes(&m, RSS, 1).unwrap();
    assert_eq!(e0.len(), 6);
    assert_eq!(e1.len(), 2);
    // The `guid` attribute spelling survives with its exact value.
    let guid = e0[4];
    let attrs = vole_document::adapter::feed::field_attrs(&m, RSS, guid).unwrap();
    assert_eq!(attrs.len(), 1);
    assert_eq!(attrs[0].name, "isPermaLink");
    assert_eq!(attrs[0].value, "false");

    // Through the field: dialect metadata, native entry-field text, exact bytes.
    let mut fx = Fixture::new("rss", RSS);
    let field = fx.report.field;
    assert_eq!(fx.report.format, DocumentFormat::Feed);
    let meta = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(meta.contains("\"format\":\"feed\""), "{meta}");
    assert!(meta.contains("\"dialect\":\"rss\""), "{meta}");
    assert!(meta.contains("\"entries\":2"), "{meta}");
    let first = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::FeedEntryField {
            entry: 0,
            name: "title".to_string(),
        },
        Representation::Text,
    ));
    assert_eq!(first, "First");
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            Selector::FeedEntryField {
                entry: 0,
                name: "guid".to_string(),
            },
            Representation::ExactBytes,
        )),
        b"<guid isPermaLink=\"false\">urn:1</guid>"
    );
}

#[test]
fn atom_preserves_namespace_and_link_href() {
    let m = vole_document::adapter::feed::parse(ATOM, Limits::DEFAULT, true).unwrap();
    assert_eq!(m.dialect, vole_document::adapter::feed::DIALECT_ATOM);
    assert_eq!(m.entry_count(), 2);
    // The namespace declaration is preserved as an attribute on the root.
    let root = m.node(m.root).unwrap();
    let ns = m.xml.attr(root.attrs[0]).unwrap();
    assert_eq!(ns.is_ns, 1);
    assert!(ns.value_start <= ns.value_end);
    // The channel/feed `link` is a void element whose text is the `href`.
    let fields = vole_document::adapter::feed::channel_field_nodes(&m, ATOM).unwrap();
    let link = fields[3];
    assert_eq!(
        vole_document::adapter::feed::field_text(&m, ATOM, link).unwrap(),
        "https://example.com/"
    );
    let attrs = vole_document::adapter::feed::field_attrs(&m, ATOM, link).unwrap();
    assert!(
        attrs
            .iter()
            .any(|a| a.name == "href" && a.value == "https://example.com/")
    );
    assert!(
        attrs
            .iter()
            .any(|a| a.name == "rel" && a.value == "alternate")
    );
    // Entry content retains the entity reference literally (never expanded).
    let e0 = vole_document::adapter::feed::entry_field_nodes(&m, ATOM, 0).unwrap();
    assert_eq!(
        vole_document::adapter::feed::field_text(&m, ATOM, e0[5]).unwrap(),
        "&lt;b&gt;one&lt;/b&gt;"
    );

    // Through the field: the channel structure reports the dialect and the Atom
    // namespace is observable through the root's attributes.
    let mut fx = Fixture::new("atom", ATOM);
    let field = fx.report.field;
    let chan = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::FeedChannel,
        Representation::Structure,
    ));
    assert!(chan.contains("\"dialect\":\"atom\""), "{chan}");
    let link_struct = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::FeedField {
            name: "link".to_string(),
        },
        Representation::Structure,
    ));
    assert!(link_struct.contains("\"name\":\"href\""), "{link_struct}");
    assert!(
        link_struct.contains("https://example.com/"),
        "{link_struct}"
    );
}

#[test]
fn native_and_common_selectors_resolve() {
    let mut fx = Fixture::new("selectors", RSS);
    let field = fx.report.field;

    // `feed-channel`: the container descriptor.
    let ch = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::FeedChannel,
        Representation::Structure,
    ));
    assert!(ch.contains("\"entries\":2"), "{ch}");
    // `feed-field`: a channel/feed-level field.
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::FeedField {
                name: "language".to_string()
            },
            Representation::Text
        )),
        "en"
    );
    // `feed-entry`: one record.
    let e1 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::FeedEntry { index: 1 },
        Representation::Structure,
    ));
    assert!(e1.contains("\"index\":1"), "{e1}");
    assert!(e1.contains("\"name\":\"title\""), "{e1}");
    // `feed-find`: a lexical search over field values.
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::FeedFind {
            pattern: "First".to_string(),
        },
        Representation::Text,
    ));
    assert!(found.contains("\"entry\":0"), "{found}");
    assert!(found.contains("\"name\":\"title\""), "{found}");
    // Common metadata / text / search-match.
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert!(text.contains("Example Feed"), "{text}");
    assert!(text.contains("Second"), "{text}");
    let sm = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("Example Feed".to_string()),
        Representation::Text,
    ));
    assert!(sm.contains("\"name\":\"title\""), "{sm}");

    // An unknown field, an out-of-range entry, and an unsupported common pair
    // decline typed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::FeedField {
                name: "nope".to_string()
            },
            Representation::Text
        ),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::FeedEntry { index: 99 },
            Representation::Text
        ),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::Table(0),
            Representation::Text
        ),
        ErrorClass::UnsupportedFeature
    );
}

#[test]
fn exact_materialization_is_byte_identical() {
    for src in [RSS, ATOM, PLAIN_XML, NO_CHANNEL, PROSE] {
        let fx = Fixture::new("exact", src);
        let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
        let out = field.materialize_exact(Limits::DEFAULT).unwrap();
        assert_eq!(out.len(), fx.source.len());
        assert_eq!(sha256(&out), sha256(&fx.source));
        assert_eq!(out, fx.source);
    }
}

#[test]
fn removal_then_fresh_process_is_queryable_and_exact() {
    let root = temp_dir("removal");
    let src_path = root.join("doc.atom");
    let desc_path = root.join("doc.voldoc");
    std::fs::write(&src_path, ATOM).unwrap();
    std::fs::write(&desc_path, opaque_descriptor(ATOM)).unwrap();

    let store_root = root.join("store");
    let field_id = {
        let mut store = FieldStore::open(&store_root).unwrap();
        let report = ingest_pdf(&mut store, &opaque_descriptor(ATOM), Limits::DEFAULT).unwrap();
        report.field
        // store drops here: a fresh process reopens the directory.
    };

    std::fs::remove_file(&src_path).unwrap();
    std::fs::remove_file(&desc_path).unwrap();

    let mut store2 = FieldStore::open(&store_root).unwrap();
    let bytes = answer_bytes(&observe_ok(
        &mut store2,
        &field_id,
        Selector::FeedEntryField {
            entry: 1,
            name: "title".to_string(),
        },
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"<title>Second</title>");

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), ATOM.len());
    assert_eq!(sha256(&exact), sha256(ATOM));
    assert_eq!(exact, ATOM);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn malformed_inputs_decline_typed() {
    // Not well-formed XML.
    let e =
        vole_document::adapter::feed::parse(b"<rss><channel>", Limits::DEFAULT, true).unwrap_err();
    assert_eq!(e.class(), ErrorClass::InvalidFeedStructure);
    // Well-formed XML, but not a feed.
    let e = vole_document::adapter::feed::parse(b"<a><b/></a>", Limits::DEFAULT, true).unwrap_err();
    assert_eq!(e.class(), ErrorClass::InvalidFeedStructure);
    // A feed over the STRICT entry cap declines typed (resource limit, not invalid).
    let mut big = String::from("<rss version=\"2.0\"><channel>");
    for _ in 0..5000 {
        big.push_str("<item><title>t</title></item>");
    }
    big.push_str("</channel></rss>");
    let e = vole_document::adapter::feed::parse(big.as_bytes(), Limits::STRICT, true).unwrap_err();
    assert_eq!(e.class(), ErrorClass::ResourceLimit);
    assert_ne!(
        detect_document_format(big.as_bytes(), Limits::STRICT),
        DocumentFormat::Feed
    );
}

#[test]
fn interleaved_fixtures_do_not_alias() {
    // ADR-0060: a source-reading node must carry its own source-identity input.
    let root = temp_dir("alias");
    let mut store = FieldStore::open(&root).unwrap();

    let fa = ingest_pdf(&mut store, &opaque_descriptor(RSS), Limits::DEFAULT)
        .unwrap()
        .field;
    let fb = ingest_pdf(&mut store, &opaque_descriptor(ATOM), Limits::DEFAULT)
        .unwrap()
        .field;

    for (field, src, want) in [
        (fa, RSS, b"<title>Example Feed</title>".to_vec()),
        (fb, ATOM, b"<title>Example</title>".to_vec()),
    ] {
        let got = answer_bytes(&observe_ok(
            &mut store,
            &field,
            Selector::FeedField {
                name: "title".to_string(),
            },
            Representation::ExactBytes,
        ));
        assert_eq!(got, want, "aliased source for {field:?}");
        let field_handle = Field::open(&store, &field, Limits::DEFAULT).unwrap();
        assert_eq!(
            field_handle.materialize_exact(Limits::DEFAULT).unwrap(),
            src
        );
    }
    drop(store);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn feed_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived feed model reads the source bytes, so its single
    // dependency must be the exact `DocumentExact` root (keyed by `sha256(source)`).
    // This is what prevents cross-field source aliasing.
    let fx = Fixture::new("adr0060", RSS);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let manifest = field.manifest().clone();
    let root = manifest.root_node;
    let root_node = load_node(fx.store.seeds(), &root).unwrap();
    assert_eq!(root_node.kind, NodeKind::DocumentExact);

    assert!(manifest.has_index());
    let istore = FsIndexStore::open(fx.store.root()).unwrap();
    let iroot = NodeId::from_bytes(manifest.index_root);
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_FEED_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::FeedModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the feed model must depend on exactly the exact root"
    );
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0xFEED_2020_21DE_AD00;
    for _ in 0..128 {
        let mut buf = vec![0u8; 512];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        if let Ok(m) = vole_document::adapter::feed::parse(&buf, Limits::STRICT, true) {
            let _ = vole_document::adapter::feed::canonical_text(&m, &buf, 1 << 20);
            let _ = vole_document::adapter::feed::build_feed_model(&buf, Limits::STRICT);
        }
    }
    // The XML-shaped hostile inputs must also never panic.
    for src in [
        b"<rss".as_slice(),
        b"<feed xmlns=\"http://www.w3.org/2005/Atom\">".as_slice(),
        b"<rss><channel><item>".as_slice(),
    ] {
        let _ = detect_document_format(src, Limits::STRICT);
    }
}
