//! Phase-12.8 cross-document procedural state court (feature `field`+`docx`+`epub`).
//!
//! Generates a small **randomized** cohort of two formats that embed the same
//! resource bytes (a DOCX and an EPUB) plus a no-sharing control, ingests them
//! through the real field pipeline, and reports what is genuinely shared under
//! exact content identity — and what is not:
//!
//! * one content-addressed `ResourceBlob` node id (one stored blob) for the
//!   byte-identical resource across the DOCX/EPUB boundary;
//! * the content-addressed decoded member sharing that same blob;
//! * per-document seed/index bytes written, bytes deduplicated, and id-shared
//!   nodes (`nodes_id_shared`) reported **separately** from work reuse
//!   (`nodes_reused`);
//! * `retained_inverse_work_fraction` from cold/warm node-execution and input-byte
//!   receipts (never from source size);
//! * a no-sharing control (different bytes) that shares nothing;
//! * `materialize_exact(root) == source` (length **and** SHA-256) for every root.
//!
//! The Phase-11 PDF adapter extracts **no** embedded resource blob, so PDF↔DOCX
//! resource sharing is out of scope; the genuine cross-format share demonstrated
//! here is DOCX↔EPUB.
//!
//! Usage (inside the pinned `dev` container):
//!   cargo run --all-features --example phase12_share_court -- OUTDIR [SEED]
//!
//! It writes `OUTDIR/raw/{fixtures,*.bin,metrics.json}` and `OUTDIR/SUMMARY.md`.
//! No compression claim is made: a shared blob is scored as *work* and as a
//! representation fact, never as an achieved store-size fraction.

use std::fs;
use std::path::PathBuf;

use vole_document::adapter::package::{crc32_iso_hdlc, scan};
use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::field::cache::DerivedCache;
use vole_document::field::dag::{
    EvalBudget, InverseWork, ReuseStats, load_node, materialize_node_cached,
    retained_inverse_work_fraction,
};
use vole_document::field::index::{FsIndexStore, SEL_PACKAGE_MEMBER_DECODED, SelectorKey, lookup};
use vole_document::field::ingest::{IngestOutcome, ingest};
use vole_document::field::observe::{
    ObserveRequest, ObserveStats, Representation, Selector, observe,
};
use vole_document::field::provenance::AnswerValue;
use vole_document::field::resource::{is_shareable_resource, resource_blob_node};
use vole_document::field::{Field, FieldStore};
use vole_document::limits::Limits;
use vole_document::store::NodeId;

/// Path of the embedded resource inside the generated EPUB.
const EPUB_RESOURCE: &str = "OEBPS/images/shared.png";
/// Path of the embedded resource inside the generated DOCX.
const DOCX_RESOURCE_ORDINAL_NAME: &str = "word/media/image1.png";

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| usage_and_exit());
    let seed = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "random".to_string());
    let out = PathBuf::from(out);
    // Stores are regenerable bytes; keep them in the gitignored scratch, never in
    // the sealed receipt.
    let scratch = std::env::args()
        .nth(3)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("evidence/scratch/phase12-share"));
    let raw = out.join("raw");
    fs::create_dir_all(&raw).expect("create raw dir");
    fs::remove_dir_all(&scratch).ok();
    fs::create_dir_all(&scratch).expect("create scratch dir");
    let limits = Limits::DEFAULT;

    let seed = seed_u64(&seed);
    // The control resource differs from the shared one but has the same length, so
    // the two EPUBs share an identical container layout and differ only in bytes.
    let shared_resource = randomized_resource(seed);
    let control_resource = randomized_resource(seed ^ 0x9E37_79B9_7F4A_7C15);

    fs::write(raw.join("shared.resource.bin"), &shared_resource).unwrap();
    fs::write(raw.join("control.resource.bin"), &control_resource).unwrap();

    let docx_shared_src = build_docx(&shared_resource);
    let epub_shared_src = build_epub(&shared_resource);
    let epub_control_src = build_epub(&control_resource);
    for (name, bytes) in [
        ("docx_shared.docx", &docx_shared_src),
        ("epub_shared.epub", &epub_shared_src),
        ("epub_control.epub", &epub_control_src),
    ] {
        fs::write(raw.join(name), bytes).unwrap();
    }

    assert!(
        is_shareable_resource(&shared_resource) && is_shareable_resource(&control_resource),
        "the randomized resources must carry a recognized signature"
    );

    // ----- ingest: shared lane (DOCX pays, EPUB shares) + control lane --------
    let mut store = FieldStore::open(scratch.join("store_shared")).unwrap();
    let docx = ingest_package(&mut store, &docx_shared_src, limits);
    let epub = ingest_package(&mut store, &epub_shared_src, limits);

    // No-sharing control: a structurally identical EPUB with different bytes in a
    // fresh store, so its resource is *not* already present and it can share
    // nothing. It is the byte-byte baseline for the shared EPUB.
    let mut control_store = FieldStore::open(scratch.join("store_control")).unwrap();
    let epub_control = ingest_package(&mut control_store, &epub_control_src, limits);

    // The shared blob's content id: identical bytes ⇒ one id across formats.
    let shared_blob = resource_blob_node(&shared_resource).content_id();
    let blob_present = store.seeds().contains_node(&shared_blob).unwrap();

    // ----- work reuse: warm the shared decoded node via the DOCX field --------
    let docx_ordinal = resource_ordinal(&docx_shared_src);
    let decoded = decoded_node_id(&store, &docx.field, docx_ordinal);
    warm_shared_node(&store, &docx.field, &decoded, limits);

    let selector = Selector::EpubResource(EPUB_RESOURCE.to_string());
    let cold = observe_stats(&mut store, &epub.field, &selector, false, limits);
    let (warm_answer, warm) = observe_answer(&mut store, &epub.field, &selector, true, limits);
    let warm_bytes_exact =
        matches!(&warm_answer.value, AnswerValue::Bytes(b) if *b == shared_resource);

    let cold_work = InverseWork::new(cold.seed_nodes_executed, cold.bytes_read);
    let warm_work = InverseWork::new(warm.seed_nodes_executed, warm.bytes_read);
    let fraction = retained_inverse_work_fraction(cold_work, warm_work);

    // ----- exactness (length + SHA-256 + byte identity) for every root --------
    let exact_inputs: [(&str, &FieldStore, vole_document::field::FieldId, &[u8]); 3] = [
        ("shared", &store, docx.field, docx_shared_src.as_slice()),
        ("shared", &store, epub.field, epub_shared_src.as_slice()),
        (
            "control",
            &control_store,
            epub_control.field,
            epub_control_src.as_slice(),
        ),
    ];
    let mut exact = Vec::new();
    for (lane, s, id, src) in exact_inputs {
        let field = Field::open(s, &id, limits).unwrap();
        let got = field.materialize_exact(limits).unwrap();
        let ok = got.len() == src.len()
            && vole_document::integrity::sha256(&got) == vole_document::integrity::sha256(src)
            && got == src;
        exact.push((lane, id.to_hex(), src.len(), ok));
        assert!(ok, "exactness failed for {lane} {}", id.to_hex());
    }

    // ----- report ------------------------------------------------------------
    let delta_seed_bytes = epub_control
        .seed_bytes_written
        .saturating_sub(epub.seed_bytes_written);
    let fraction_str = format!("{fraction:.6}");
    let exact_str = exact
        .iter()
        .map(|(lane, id, len, ok)| {
            format!(
                "{{\"lane\":\"{lane}\",\"field\":\"{id}\",\"source_len\":{len},\"materialize_exact\":{ok}}}"
            )
        })
        .collect::<Vec<_>>()
        .join(",");

    let metrics = format!(
        concat!(
            "{{\n",
            "  \"shared_resource_bytes\": {shared_len},\n",
            "  \"shared_blob_id\": \"{blob}\",\n",
            "  \"blob_present_in_store\": {blob_present},\n",
            "  \"docx\": {{ \"resource_blob_nodes\": {d_rbn}, \"shared_resource_ids\": {d_sri}, ",
            "\"shared_resource_bytes\": {d_srb}, \"nodes_id_shared\": {d_nis}, ",
            "\"seed_bytes_written\": {d_sbw}, \"index_bytes_written\": {d_ibw} }},\n",
            "  \"epub\": {{ \"resource_blob_nodes\": {e_rbn}, \"shared_resource_ids\": {e_sri}, ",
            "\"shared_resource_bytes\": {e_srb}, \"nodes_id_shared\": {e_nis}, ",
            "\"seed_bytes_written\": {e_sbw}, \"index_bytes_written\": {e_ibw} }},\n",
            "  \"epub_control\": {{ \"resource_blob_nodes\": {c_rbn}, \"shared_resource_ids\": {c_sri}, ",
            "\"shared_resource_bytes\": {c_srb}, \"nodes_id_shared\": {c_nis}, ",
            "\"seed_bytes_written\": {c_sbw}, \"index_bytes_written\": {c_ibw} }},\n",
            "  \"shared_lane\": {{ \"seed_bytes_written\": {l_sbw}, \"index_bytes_written\": {l_ibw} }},\n",
            "  \"control_lane\": {{ \"seed_bytes_written\": {cl_sbw}, \"index_bytes_written\": {cl_ibw} }},\n",
            "  \"seed_bytes_saved_on_second_document\": {delta},\n",
            "  \"cold\": {{ \"node_executions\": {cold_ex}, \"input_bytes\": {cold_in}, \"work_units\": {cold_units} }},\n",
            "  \"warm\": {{ \"node_executions\": {warm_ex}, \"input_bytes\": {warm_in}, \"work_units\": {warm_units}, ",
            "\"nodes_reused\": {warm_reused}, \"nodes_id_shared\": {warm_nis} }},\n",
            "  \"retained_inverse_work_fraction\": {fraction},\n",
            "  \"warm_bytes_exact\": {warm_bytes_exact},\n",
            "  \"exactness\": [{exact}]\n",
            "}}"
        ),
        shared_len = shared_resource.len(),
        blob = shared_blob.to_hex(),
        blob_present = blob_present,
        d_rbn = docx.resource_blob_nodes,
        d_sri = docx.shared_resource_ids,
        d_srb = docx.shared_resource_bytes,
        d_nis = docx.nodes_id_shared,
        d_sbw = docx.seed_bytes_written,
        d_ibw = docx.index_bytes_written,
        e_rbn = epub.resource_blob_nodes,
        e_sri = epub.shared_resource_ids,
        e_srb = epub.shared_resource_bytes,
        e_nis = epub.nodes_id_shared,
        e_sbw = epub.seed_bytes_written,
        e_ibw = epub.index_bytes_written,
        c_rbn = epub_control.resource_blob_nodes,
        c_sri = epub_control.shared_resource_ids,
        c_srb = epub_control.shared_resource_bytes,
        c_nis = epub_control.nodes_id_shared,
        c_sbw = epub_control.seed_bytes_written,
        c_ibw = epub_control.index_bytes_written,
        l_sbw = docx
            .seed_bytes_written
            .saturating_add(epub.seed_bytes_written),
        l_ibw = docx
            .index_bytes_written
            .saturating_add(epub.index_bytes_written),
        cl_sbw = epub_control.seed_bytes_written,
        cl_ibw = epub_control.index_bytes_written,
        delta = delta_seed_bytes,
        cold_ex = cold_work.node_executions,
        cold_in = cold_work.input_bytes,
        cold_units = cold_work.units(),
        warm_ex = warm_work.node_executions,
        warm_in = warm_work.input_bytes,
        warm_units = warm_work.units(),
        warm_reused = warm.seed_nodes_reused,
        warm_nis = warm.nodes_id_shared,
        fraction = fraction_str,
        warm_bytes_exact = warm_bytes_exact,
        exact = exact_str,
    );
    fs::write(raw.join("metrics.json"), format!("{metrics}\n")).unwrap();
    fs::write(
        out.join("SUMMARY.md"),
        summary(&metrics, &shared_resource, seed),
    )
    .unwrap();

    println!("{metrics}");
}

fn usage_and_exit() -> ! {
    eprintln!(
        "usage: phase12_share_court OUTDIR [SEED [SCRATCH]]\n\
         (run inside the pinned `dev` container: cargo run --all-features --example phase12_share_court -- OUTDIR SEED SCRATCH)"
    );
    std::process::exit(2);
}

fn summary(metrics: &str, resource: &[u8], seed: u64) -> String {
    format!(
        "# Phase 12.8 cross-document procedural state court\n\n\
         Generated by `examples/phase12_share_court.rs` inside the pinned `dev` image.\n\
         Randomized resource: {} bytes (seed {:#018x}); a DOCX and an EPUB embed the same\n\
         bytes, and a structurally identical EPUB control embeds different bytes in a\n\
         fresh store.\n\n\
         ## What is genuinely shared (exact content identity)\n\n\
         * the byte-identical resource resolves to **one** `ResourceBlob` content id in\n\
           both the DOCX and the EPUB (verified `contains_node`), and the decoded member\n\
           shares that same blob id;\n\
         * the second document writes 0 bytes for the shared blob (a representation fact,\n\
           `nodes_id_shared`), reported separately from work reuse (`nodes_reused`);\n\
         * work reuse is measured as a fraction of receipted inverse-work units (node\n\
           executions + cold input bytes), never from source size.\n\n\
         ## What is not shared\n\n\
         * the Phase-11 **PDF** adapter extracts no embedded resource blob, so PDF↔DOCX\n\
           resource sharing does not exist (recorded, not a failure);\n\
         * raw compressed (DEFLATE) members and per-source span records are not content\n\
           identity and are not claimed as sharing;\n\
         * the no-sharing control (different bytes) shares no resource, shares no node\n\
           id, and deduplicates zero bytes.\n\n\
         ## Accounting scope (no compression claim)\n\n\
         `seed_bytes_saved_on_second_document` is the *seed-node record* bytes the second\n\
         document did not rewrite because the blob and decoded member were already present.\n\
         It is NOT an overall store-size saving: each document's exact source descriptor\n\
         is stored independently and still contains the resource bytes. A shared blob is\n\
         therefore scored as state/work, never as an achieved store-size fraction.\n\n\
         ```json\n{}\n```\n",
        resource.len(),
        seed,
        metrics.trim_end()
    )
}

// ---------------------------------------------------------------------------
// randomized resource
// ---------------------------------------------------------------------------

fn seed_u64(arg: &str) -> u64 {
    if arg == "random" {
        let mut buf = [0u8; 8];
        let mut f = fs::File::open("/dev/urandom").expect("open /dev/urandom");
        std::io::Read::read_exact(&mut f, &mut buf).expect("read seed");
        u64::from_le_bytes(buf)
    } else {
        arg.parse().expect("SEED must be an integer or `random`")
    }
}

/// SplitMix64-expanded resource bytes: a PNG signature followed by pseudo-random
/// payload. Deterministic in `seed`, so the receipt is reproducible from it.
fn randomized_resource(seed: u64) -> Vec<u8> {
    const PAYLOAD: usize = 4096;
    let mut out = Vec::with_capacity(8 + PAYLOAD);
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut state = seed;
    while out.len() < 8 + PAYLOAD {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        out.extend_from_slice(&z.to_le_bytes());
    }
    out.truncate(8 + PAYLOAD);
    out
}

// ---------------------------------------------------------------------------
// dependency-free ZIP / document writers (court ground truth)
// ---------------------------------------------------------------------------

fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// A ZIP with stored (method 0) members, order preserved, UTF-8 flag set.
fn build_zip(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    const UTF8_FLAG: u16 = 0x0800;
    let mut out: Vec<u8> = Vec::new();
    let mut offsets: Vec<u32> = Vec::new();
    let mut crcs: Vec<u32> = Vec::new();
    for (name, content) in entries {
        offsets.push(out.len() as u32);
        crcs.push(crc32_iso_hdlc(content));
        put_u32(&mut out, 0x0403_4b50);
        put_u16(&mut out, 20);
        put_u16(&mut out, UTF8_FLAG);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u32(&mut out, *crcs.last().unwrap());
        put_u32(&mut out, content.len() as u32);
        put_u32(&mut out, content.len() as u32);
        put_u16(&mut out, name.len() as u16);
        put_u16(&mut out, 0);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(content);
    }
    let cd_start = out.len() as u32;
    for (i, (name, content)) in entries.iter().enumerate() {
        put_u32(&mut out, 0x0201_4b50);
        put_u16(&mut out, 20);
        put_u16(&mut out, 20);
        put_u16(&mut out, UTF8_FLAG);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u32(&mut out, crcs[i]);
        put_u32(&mut out, content.len() as u32);
        put_u32(&mut out, content.len() as u32);
        put_u16(&mut out, name.len() as u16);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u32(&mut out, 0);
        put_u32(&mut out, offsets[i]);
        out.extend_from_slice(name.as_bytes());
    }
    let cd_end = out.len() as u32;
    put_u32(&mut out, 0x0605_4b50);
    put_u16(&mut out, 0);
    put_u16(&mut out, 0);
    put_u16(&mut out, entries.len() as u16);
    put_u16(&mut out, entries.len() as u16);
    put_u32(&mut out, cd_end - cd_start);
    put_u32(&mut out, cd_start);
    put_u16(&mut out, 0);
    out
}

/// A minimal OPC DOCX embedding `png` at [`DOCX_RESOURCE_ORDINAL_NAME`].
fn build_docx(png: &[u8]) -> Vec<u8> {
    let content_types = br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_vec();
    let rels = br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_vec();
    let document = br#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Cross-document resource court</w:t></w:r></w:p></w:body></w:document>"#.to_vec();
    build_zip(&[
        ("[Content_Types].xml".to_string(), content_types),
        ("_rels/.rels".to_string(), rels),
        ("word/document.xml".to_string(), document),
        (DOCX_RESOURCE_ORDINAL_NAME.to_string(), png.to_vec()),
    ])
}

/// A minimal OCF EPUB embedding `png` at [`EPUB_RESOURCE`].
fn build_epub(png: &[u8]) -> Vec<u8> {
    let container = br#"<?xml version="1.0" encoding="UTF-8"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/package.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#.to_vec();
    let opf = br#"<?xml version="1.0" encoding="UTF-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">urn:uuid:court</dc:identifier><dc:title>Resource Court</dc:title><dc:language>en</dc:language></metadata><manifest><item id="c1" href="chapter.xhtml" media-type="application/xhtml+xml"/><item id="img" href="images/shared.png" media-type="image/png"/></manifest><spine><itemref idref="c1"/></spine></package>"#.to_vec();
    let chapter = br#"<?xml version="1.0" encoding="UTF-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>c</title></head><body><p>cross-document resource</p></body></html>"#.to_vec();
    build_zip(&[
        ("mimetype".to_string(), b"application/epub+zip".to_vec()),
        ("META-INF/container.xml".to_string(), container),
        ("OEBPS/package.opf".to_string(), opf),
        ("OEBPS/chapter.xhtml".to_string(), chapter),
        (EPUB_RESOURCE.to_string(), png.to_vec()),
    ])
}

fn opaque_descriptor(source: &[u8]) -> Vec<u8> {
    let d = Descriptor {
        universe: vole_document::container::UNIVERSE.to_string(),
        source_format: vole_document::SOURCE_FORMAT_OPAQUE,
        format_basis: "opaque:phase12-share".to_string(),
        models: vec![],
        channels: vec![],
        objects: vec![ObjectSource::Inline(source.to_vec())],
        program: Program::new(vec![Op::EmitObject { object_id: 0 }]),
        observation_index: None,
        seek_directory: false,
        source_sha256: vole_document::integrity::sha256(source),
        source_len: source.len() as u64,
    };
    d.serialize().unwrap().0
}

fn ingest_package(
    store: &mut FieldStore,
    source: &[u8],
    limits: Limits,
) -> vole_document::field::ingest_package::PackageIngestReport {
    match ingest(store, &opaque_descriptor(source), limits).unwrap() {
        IngestOutcome::Package(r) => r,
        other => panic!("expected a package ingest, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// reuse helpers (mirror the integration court)
// ---------------------------------------------------------------------------

fn resource_ordinal(source: &[u8]) -> u32 {
    let physical = scan(source, Limits::DEFAULT).unwrap();
    physical
        .members
        .iter()
        .find(|m| m.name.ends_with(b".png"))
        .expect("a .png member")
        .id
        .ordinal
}

fn decoded_node_id(
    store: &FieldStore,
    field: &vole_document::field::FieldId,
    ordinal: u32,
) -> NodeId {
    let manifest = store.get_field(field).unwrap();
    let istore = FsIndexStore::open(store.root()).unwrap();
    let root = NodeId::from_bytes(manifest.index_root);
    lookup(
        &istore,
        &root,
        &SelectorKey::new(SEL_PACKAGE_MEMBER_DECODED, ordinal),
    )
    .unwrap()
    .first()
    .expect("decoded member entry")
    .node_id
}

fn warm_shared_node(
    store: &FieldStore,
    field: &vole_document::field::FieldId,
    id: &NodeId,
    limits: Limits,
) {
    let opened = Field::open(store, field, limits).unwrap();
    let node = load_node(store.seeds(), id).unwrap();
    let mut cache = DerivedCache::open(store.root().join("cache")).unwrap();
    let mut budget = EvalBudget::default();
    let mut reuse = ReuseStats::default();
    let out = materialize_node_cached(
        opened.parsed(),
        store.seeds(),
        &mut cache,
        &node,
        limits,
        &mut budget,
        node.limits.max_depth,
        &mut reuse,
    )
    .unwrap();
    assert!(!out.is_empty(), "warmed node produced no bytes");
}

fn observe_answer(
    store: &mut FieldStore,
    field: &vole_document::field::FieldId,
    selector: &Selector,
    use_cache: bool,
    limits: Limits,
) -> (vole_document::field::provenance::FieldAnswer, ObserveStats) {
    let mut req = ObserveRequest::new(selector.clone(), Representation::DecodedBytes);
    req.use_cache = use_cache;
    let (answer, stats, _) = observe(store, field, &req, limits).unwrap();
    (answer, stats)
}

fn observe_stats(
    store: &mut FieldStore,
    field: &vole_document::field::FieldId,
    selector: &Selector,
    use_cache: bool,
    limits: Limits,
) -> ObserveStats {
    observe_answer(store, field, selector, use_cache, limits).1
}
