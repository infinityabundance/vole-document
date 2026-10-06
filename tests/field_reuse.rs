//! Cross-process persisted computation reuse court (Phase 11.8).
//!
//! Reuse is claimed by **execution counters across a process boundary**, never by
//! wall-clock: a new OS process with no inherited in-memory state reports
//! `seed_nodes_reused > 0` and `seed_nodes_executed == 0` for a chain a previous
//! process computed. The whole file is gated on the `field` feature so
//! `--no-default-features` builds an empty (compiling) test target.

#![cfg(feature = "field")]

use std::path::{Path, PathBuf};
use std::process::Command;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::field::cache::DerivedCache;
use vole_document::field::observe::{ObserveRequest, Representation, Selector, observe};
use vole_document::field::{Field, FieldId, FieldStore, ingest as field_ingest};
use vole_document::limits::Limits;

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-reuse-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let chunks: Vec<&[u8]> = data.chunks(0xFFFF).collect();
    for (i, chunk) in chunks.iter().enumerate() {
        out.push(u8::from(i + 1 == chunks.len()));
        let len = chunk.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(chunk);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

struct PdfBuilder {
    buf: Vec<u8>,
    offsets: Vec<(u64, u64)>,
}

impl PdfBuilder {
    fn new() -> Self {
        PdfBuilder {
            buf: Vec::new(),
            offsets: Vec::new(),
        }
    }
    fn text(&mut self, s: &str) {
        self.buf.extend_from_slice(s.as_bytes());
    }
    fn raw(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }
    fn obj(&mut self, number: u64, body: &[u8]) {
        self.offsets.push((number, self.buf.len() as u64));
        self.text(&format!("{number} 0 obj\n"));
        self.raw(body);
        self.text("\nendobj\n");
    }
    fn stream_obj(&mut self, number: u64, extra: &str, data: &[u8]) {
        self.offsets.push((number, self.buf.len() as u64));
        self.text(&format!(
            "{number} 0 obj\n<< /Length {}{extra} >>\nstream\n",
            data.len()
        ));
        self.raw(data);
        self.text("\nendstream\nendobj\n");
    }
    fn offset_of(&self, number: u64) -> u64 {
        self.offsets
            .iter()
            .find(|&&(n, _)| n == number)
            .map(|&(_, off)| off)
            .unwrap()
    }
    fn classic_trailer(&mut self, size: u64, extra: &str) {
        let xref = self.buf.len() as u64;
        self.text(&format!("xref\n0 {size}\n"));
        self.raw(b"0000000000 65535 f \n");
        for number in 1..size {
            let off = self.offset_of(number);
            self.text(&format!("{off:010} 00000 n \n"));
        }
        self.text(&format!(
            "trailer\n<< /Size {size}{extra} >>\nstartxref\n{xref}\n%%EOF\n"
        ));
    }
}

/// A classic-xref PDF with one page and a lone-Flate content stream `(Hello) Tj`.
fn fixture_pdf() -> Vec<u8> {
    let content = b"BT /F1 12 Tf 72 720 Td (Hello) Tj ET\n";
    let encoded = zlib_stored(content);
    let mut w = PdfBuilder::new();
    w.text("%PDF-1.5\n");
    w.obj(1, b"<< /Type /Catalog /Pages 2 0 R >>");
    w.obj(2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.obj(
        3,
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
    );
    w.stream_obj(4, " /Filter /FlateDecode", &encoded);
    w.obj(5, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
    w.classic_trailer(6, " /Root 1 0 R");
    w.buf
}

fn opaque_descriptor(source: &[u8]) -> Vec<u8> {
    let d = Descriptor {
        universe: vole_document::container::UNIVERSE.to_string(),
        source_format: vole_document::SOURCE_FORMAT_PDF,
        format_basis: "pdf:reuse-test".to_string(),
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

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_vole-document")
}

fn run(args: &[&str]) -> (bool, String, String) {
    let output = Command::new(bin())
        .args(args)
        .output()
        .expect("failed to spawn vole-document");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn write_descriptor(dir: &Path, bytes: &[u8]) -> PathBuf {
    let path = dir.join("input.voldoc");
    std::fs::write(&path, bytes).unwrap();
    path
}

fn json_field_id(stdout: &str) -> String {
    let key = "\"field\":\"";
    let start = stdout.find(key).expect("field key present") + key.len();
    let rest = &stdout[start..];
    let end = rest.find('"').expect("closing quote");
    rest[..end].to_string()
}

/// Parse `"key":N` from a flat or nested-but-simple JSON body.
fn json_u64(stdout: &str, key: &str) -> u64 {
    let needle = format!("\"{key}\":");
    let start = stdout
        .find(&needle)
        .unwrap_or_else(|| panic!("key {key:?} not found in {stdout}"))
        + needle.len();
    let rest = &stdout[start..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end]
        .parse()
        .unwrap_or_else(|_| panic!("key {key:?} is not an integer in {stdout}"))
}

/// Ingest the fixture through the CLI, returning `(store_dir, field_hex)`.
fn ingest_fixture(dir: &Path) -> (PathBuf, String) {
    let descriptor = write_descriptor(dir, &opaque_descriptor(&fixture_pdf()));
    let store = dir.join("store");
    let (ok, stdout, stderr) = run(&[
        "field-ingest",
        descriptor.to_str().unwrap(),
        "--store",
        store.to_str().unwrap(),
    ]);
    assert!(ok, "field-ingest failed: {stderr}");
    (store, json_field_id(&stdout))
}

#[test]
fn cross_process_reuse_is_proven_by_execution_counters() {
    let dir = temp_dir("xproc");
    let (store, field_hex) = ingest_fixture(&dir);
    let store_s = store.to_str().unwrap();

    let cmd = [
        "observe", "--store", store_s, "--field", &field_hex, "--page", "1", "--kind", "text",
    ];

    // Process A: cold cache; it executes the derived chain and persists it.
    let (ok, a_out, a_err) = run(&cmd);
    assert!(ok, "A observe failed: {a_err}");
    assert!(a_out.contains("Hello"), "A answer: {a_out}");
    let a_exec = json_u64(&a_out, "seed_nodes_executed");
    let a_reused = json_u64(&a_out, "seed_nodes_reused");
    let a_written = json_u64(&a_out, "cache_bytes_written");
    assert!(a_exec > 0, "A must execute on a cold cache: {a_out}");
    assert_eq!(a_reused, 0, "A must not reuse on a cold cache: {a_out}");
    assert!(a_written > 0, "A must populate the cache: {a_out}");

    // Process B: a NEW OS process with no inherited state.
    let (ok, b_out, b_err) = run(&cmd);
    assert!(ok, "B observe failed: {b_err}");
    assert!(b_out.contains("Hello"), "B answer: {b_out}");
    let b_exec = json_u64(&b_out, "seed_nodes_executed");
    let b_reused = json_u64(&b_out, "seed_nodes_reused");

    println!(
        "cross-process reuse: A exec={a_exec} reused={a_reused} written={a_written}; \
         B exec={b_exec} reused={b_reused}"
    );

    assert!(b_reused > 0, "B must reuse persisted work: {b_out}");
    assert_eq!(
        b_exec, 0,
        "B must execute zero nodes for the reused subtree: {b_out}"
    );
    assert!(
        b_exec < a_exec,
        "B must execute strictly fewer nodes than A"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn warm_and_cold_observations_are_byte_identical() {
    let dir = temp_dir("warmcold");
    let source = fixture_pdf();
    let descriptor = opaque_descriptor(&source);
    let mut store = FieldStore::open(&dir).unwrap();
    let report = field_ingest::ingest_pdf(&mut store, &descriptor, Limits::DEFAULT).unwrap();
    let id = report.field;

    let req = ObserveRequest::new(Selector::Page(1), Representation::Text);
    // First observation: cold cache, deepens and populates.
    let (first, _, _) = observe(&mut store, &id, &req, Limits::DEFAULT).unwrap();
    // Second observation: warm cache, whole-subtree reuse.
    let (warm, warm_stats, _) = observe(&mut store, &id, &req, Limits::DEFAULT).unwrap();
    assert_eq!(first.value, warm.value);
    assert!(warm_stats.seed_nodes_reused > 0, "warm: {warm_stats:?}");
    assert_eq!(warm_stats.seed_nodes_executed, 0, "warm: {warm_stats:?}");

    // Cold court: caching disabled recomputes every node, but byte-identically.
    let mut cold_req = ObserveRequest::new(Selector::Page(1), Representation::Text);
    cold_req.use_cache = false;
    let (cold, cold_stats, _) = observe(&mut store, &id, &cold_req, Limits::DEFAULT).unwrap();
    assert_eq!(warm.value, cold.value);
    assert_eq!(cold_stats.seed_nodes_reused, 0, "cold: {cold_stats:?}");
    assert!(cold_stats.seed_nodes_executed > 0, "cold: {cold_stats:?}");

    // Full bake is byte-identical, before and after the warm/cold observations.
    let field = Field::open(&store, &id, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), source);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn poisoned_cache_never_returns_wrong_bytes() {
    let dir = temp_dir("poison");
    let source = fixture_pdf();
    let descriptor = opaque_descriptor(&source);
    let mut store = FieldStore::open(&dir).unwrap();
    let report = field_ingest::ingest_pdf(&mut store, &descriptor, Limits::DEFAULT).unwrap();
    let id = report.field;
    let req = ObserveRequest::new(Selector::Page(1), Representation::Text);
    let (good, _, _) = observe(&mut store, &id, &req, Limits::DEFAULT).unwrap();

    // Corrupt every cache output file, leaving its sidecar digest intact.
    let cache = DerivedCache::open(store.root().join("cache")).unwrap();
    let mut poisoned = 0u32;
    for entry in std::fs::read_dir(cache.root()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) == Some("b3") {
            continue;
        }
        std::fs::write(&path, b"poisoned!!!").unwrap();
        poisoned += 1;
    }
    assert!(
        poisoned > 0,
        "the warm observation must have written cache files"
    );

    // The observation must recompute (a corrupt entry is a miss) and stay correct.
    let (after, stats, _) = observe(&mut store, &id, &req, Limits::DEFAULT).unwrap();
    assert_eq!(
        after.value, good.value,
        "poisoned cache must never change the answer"
    );
    assert!(
        stats.seed_nodes_executed > 0,
        "a poisoned cache must force recomputation: {stats:?}"
    );

    // The full bake is unaffected by the disposable cache.
    let field = Field::open(&store, &id, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), source);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn cache_is_disposable_and_accounted_separately() {
    let dir = temp_dir("disposable");
    let (store, field_hex) = ingest_fixture(&dir);
    let store_s = store.to_str().unwrap();
    let observe_cmd = [
        "observe", "--store", store_s, "--field", &field_hex, "--page", "1", "--kind", "text",
    ];

    let (ok, _, err) = run(&observe_cmd);
    assert!(ok, "warm observe failed: {err}");

    // The cache universe is reported and non-empty...
    let (ok, out, err) = run(&["cache", "--store", store_s]);
    assert!(ok, "cache failed: {err}");
    let before = json_u64(&out, "cache_bytes");
    assert!(before > 0, "warm cache should have bytes: {out}");

    // ...but the store's own namespaces are separate and untouched by a clear.
    assert!(store.join("seed").read_dir().unwrap().next().is_some());
    let (ok, out, err) = run(&["cache", "--store", store_s, "--clear"]);
    assert!(ok, "cache --clear failed: {err}");
    assert_eq!(json_u64(&out, "cache_bytes"), 0, "clear: {out}");
    assert!(json_u64(&out, "reclaimed") > 0, "clear: {out}");
    assert!(
        store.join("seed").read_dir().unwrap().next().is_some(),
        "clearing the cache must not touch the seed store"
    );

    // Observation still returns the correct answer, repopulating the cache.
    let (ok, out, err) = run(&observe_cmd);
    assert!(ok, "post-clear observe failed: {err}");
    assert!(out.contains("Hello"), "post-clear answer: {out}");
    let (_, out, _) = run(&["cache", "--store", store_s]);
    assert!(
        json_u64(&out, "cache_bytes") > 0,
        "cache must repopulate: {out}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn reuse_survives_a_full_bake() {
    let dir = temp_dir("bake");
    let source = fixture_pdf();
    let (store, field_hex) = ingest_fixture(&dir);
    let store_s = store.to_str().unwrap();
    let observe_cmd = [
        "observe", "--store", store_s, "--field", &field_hex, "--page", "1", "--kind", "text",
    ];
    let (ok, _, err) = run(&observe_cmd);
    assert!(ok, "warm observe failed: {err}");

    // A full bake reads the descriptor only and leaves the derived cache intact.
    let baked = dir.join("baked.pdf");
    let (ok, _, err) = run(&[
        "materialize",
        "--store",
        store_s,
        "--field",
        &field_hex,
        "--exact",
        "--output",
        baked.to_str().unwrap(),
    ]);
    assert!(ok, "materialize failed: {err}");
    assert_eq!(std::fs::read(&baked).unwrap(), source);

    // Reuse still happens after the bake.
    let (ok, out, err) = run(&observe_cmd);
    assert!(ok, "post-bake observe failed: {err}");
    assert!(out.contains("Hello"), "post-bake answer: {out}");
    assert!(
        json_u64(&out, "seed_nodes_reused") > 0,
        "reuse must survive a full bake: {out}"
    );
    assert_eq!(json_u64(&out, "seed_nodes_executed"), 0, "post-bake: {out}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn fresh_store_without_cache_still_materializes_exactly() {
    let dir = temp_dir("c3");
    let source = fixture_pdf();
    let descriptor = opaque_descriptor(&source);
    let mut store = FieldStore::open(&dir).unwrap();
    let report = field_ingest::ingest_pdf(&mut store, &descriptor, Limits::DEFAULT).unwrap();

    // C3: no cache present at all (the disposable universe is gone).
    std::fs::remove_dir_all(store.root().join("cache")).ok();

    let req = ObserveRequest::new(Selector::Page(1), Representation::Text);
    let (answer, stats, promoted) =
        observe(&mut store, &report.field, &req, Limits::DEFAULT).unwrap();
    match &answer.value {
        vole_document::field::provenance::AnswerValue::Text(t) => {
            assert!(t.contains("Hello"), "got {t:?}")
        }
        other => panic!("expected text, got {other:?}"),
    }
    // No cache existed, so nothing could be reused; work happened.
    assert_eq!(stats.seed_nodes_reused, 0, "{stats:?}");
    let _ = promoted;

    // Exact materialization is still byte-identical without any cache.
    let field = Field::open(&store, &report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), source);
    let _ = FieldId::from_hex(&report.field.to_hex()).unwrap();

    std::fs::remove_dir_all(&dir).ok();
}
