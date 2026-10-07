#![cfg(feature = "deflate-replay")]
//! Phase-7.1b process-isolation courts for the F2 replay-allocation finding.
//!
//! `preflate-rs` 0.7.6 can allocate ~2.5 GiB reconstructing from a 33-byte
//! hostile correction blob (fuzz finding F2). The decoder now runs replay in a
//! separate process under an `RLIMIT_AS` address-space cap and a wall-clock
//! timeout (`src/codec/deflate.rs::replay_bounded`). These tests exercise the
//! isolation from the CLI boundary:
//!
//! 1. positive — a replay-lane `.voldoc` decodes byte-exactly with the worker
//!    configured;
//! 2. negative — the committed F2 fixture, embedded in a descriptor, fails closed
//!    (typed error) under a small (`32 MiB`) address-space cap without exhausting
//!    the host;
//! 3. fallback — with no worker configured, the library still reconstructs
//!    in-process.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use vole_document::SOURCE_FORMAT_PDF;
use vole_document::adapter::pdf::samples::sample_pdfs;
use vole_document::codec::deflate::{replay_bounded, try_replay};
use vole_document::container::{Descriptor, ObjectSource, UNIVERSE};
use vole_document::dra::{Op, Program};
use vole_document::encode;
use vole_document::encode::candidates::CandidateKind;
use vole_document::integrity::sha256;
use vole_document::limits::Limits;

/// The built CLI binary, used both as the decode driver and as the worker.
const CLI: &str = env!("CARGO_BIN_EXE_vole-document");

/// A self-cleaning temporary directory (no `tempfile` dependency).
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut path = std::env::temp_dir();
        path.push(format!(
            "vole-replay-iso-{tag}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        TempDir { path }
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Fetch a named PDF from the canonical sample corpus.
fn corpus(name: &str) -> Vec<u8> {
    sample_pdfs()
        .into_iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("sample {name} is missing from the corpus"))
        .1
}

fn zlib(data: &[u8]) -> Vec<u8> {
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use std::io::Write;
    let mut e = ZlibEncoder::new(Vec::new(), Compression::new(6));
    e.write_all(data).unwrap();
    e.finish().unwrap()
}

/// Gate 1 (positive): a descriptor that exercises the replay lane decodes
/// byte-exactly when the CLI is its own isolated worker.
#[test]
fn cli_decode_with_isolated_worker_is_byte_exact() {
    let src = corpus("flate.pdf");
    // Force the raw-plaintext replay lane so a `DEFLATE_REPLAY` op is certainly
    // present and therefore certain to go through the isolated worker.
    let (encoded, _) =
        encode::encode_with(&src, Limits::DEFAULT, Some(CandidateKind::PdfDeflateReplay))
            .expect("force pdf-deflate-replay");

    let dir = TempDir::new("pos");
    let inp = dir.join("in.voldoc");
    let outp = dir.join("out.pdf");
    fs::write(&inp, &encoded).unwrap();

    let out = Command::new(CLI)
        .arg("decode")
        .arg(&inp)
        .arg(&outp)
        .env("VOLE_REPLAY_WORKER", CLI)
        .output()
        .expect("run decode");

    assert!(
        out.status.success(),
        "isolated decode failed (status {:?}): {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        fs::read(&outp).unwrap(),
        src,
        "isolated decode must reproduce the source byte-for-byte"
    );
}

/// Gate 2 (negative): the F2 fixture embedded in a `.voldoc` must fail closed
/// under a small address-space cap and must not exhaust the host.
#[test]
fn hostile_f2_descriptor_fails_closed_under_small_memory_cap() {
    const FIXTURE: &[u8] = include_bytes!("fixtures/deflate_replay_unbounded_alloc.bin");
    assert_eq!(FIXTURE.len(), 33, "committed F2 fixture changed");
    // Reproduce the fuzz target's `(plaintext, corrections)` split.
    let at = (FIXTURE[0] as usize).wrapping_mul(31) % (FIXTURE.len() + 1);
    let (plaintext, corrections) = FIXTURE.split_at(at);

    // The declared output must satisfy the VOLE replay-profile admission limit,
    // which for this tiny plaintext is `2 * P + 1024`; a small declared length
    // proves the profile bound is not what stops F2. `preflate` allocates from
    // the correction blob regardless of this value.
    let declared = 4u32;
    let descriptor = Descriptor {
        universe: UNIVERSE.to_string(),
        source_format: SOURCE_FORMAT_PDF,
        format_basis: "pdf-deflate-replay;test=f2;phase=7.1b".to_string(),
        models: vec![],
        channels: vec![],
        objects: vec![
            ObjectSource::Inline(plaintext.to_vec()),
            ObjectSource::Inline(corrections.to_vec()),
        ],
        program: Program::new(vec![Op::DeflateReplay {
            replay_codec: vole_document::dra::op::REPLAY_DEFLATE_PREFLATE_0_7_6,
            source_kind: vole_document::dra::op::DEFLATE_SOURCE_OBJECT,
            source_id: 0,
            corrections_object: 1,
            declared_output_len: declared,
        }]),
        observation_index: None,
        seek_directory: false,
        checkpoints: None,
        source_sha256: sha256(&[0u8; 4]),
        source_len: u64::from(declared),
    };
    let (encoded, _) = descriptor
        .serialize()
        .expect("serialize hostile descriptor");

    let dir = TempDir::new("neg");
    let inp = dir.join("f2.voldoc");
    let outp = dir.join("f2.out");
    fs::write(&inp, &encoded).unwrap();

    let out = Command::new(CLI)
        .arg("decode")
        .arg(&inp)
        .arg(&outp)
        .env("VOLE_REPLAY_WORKER", CLI)
        .env("VOLE_REPLAY_MEM_MB", "32")
        .output()
        .expect("run decode");

    assert!(
        !out.status.success(),
        "hostile F2 decode must fail closed, but it succeeded"
    );
    eprintln!(
        "F2 isolation: status={:?} stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr).trim()
    );
    let code = out.status.code().unwrap_or(-1);
    // CodecReplay = 13, ResourceLimit = 8; both are typed, fail-closed classes.
    assert!(
        code == 13 || code == 8,
        "expected CodecReplay(13) or ResourceLimit(8), got {code}; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !outp.exists(),
        "a failed decode must not leave an output file"
    );
}

/// Gate 4 (no silent fallback): when a worker path is configured, a broken worker
/// is a typed error — the decode must *not* quietly fall back to the in-process
/// path. This proves the CLI decode path above actually engages the worker.
#[test]
fn configured_but_broken_worker_does_not_silently_fall_back() {
    let src = corpus("flate.pdf");
    let (encoded, _) =
        encode::encode_with(&src, Limits::DEFAULT, Some(CandidateKind::PdfDeflateReplay))
            .expect("force pdf-deflate-replay");

    let dir = TempDir::new("broken");
    let inp = dir.join("in.voldoc");
    let outp = dir.join("out.pdf");
    fs::write(&inp, &encoded).unwrap();

    let out = Command::new(CLI)
        .arg("decode")
        .arg(&inp)
        .arg(&outp)
        .env("VOLE_REPLAY_WORKER", "/nonexistent/vole-replay-worker")
        .output()
        .expect("run decode");

    assert!(
        !out.status.success(),
        "a configured but broken worker must fail closed, not fall back"
    );
    assert_eq!(
        out.status.code(),
        Some(13),
        "broken worker must surface CodecReplay(13); stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!outp.exists(), "no output may be written on failure");
}

/// Gate 5 (fallback): with no worker configured, `replay_bounded` reconstructs
/// in-process. The test binary never installs a default worker, so the library
/// takes the documented fallback.
#[test]
fn replay_bounded_falls_back_in_process_without_worker() {
    if std::env::var_os("VOLE_REPLAY_WORKER").is_some() {
        // The environment demands isolation; the fallback cannot be observed.
        return;
    }
    let data = b"BT /F1 12 Tf (fallback) Tj ET\n".repeat(64);
    let z = zlib(&data);
    let plan = try_replay(&z, Limits::DEFAULT).expect("replay plan");
    let raw = replay_bounded(
        &plan.plaintext,
        &plan.corrections,
        plan.raw_len,
        Limits::DEFAULT,
    )
    .expect("in-process fallback must reconstruct");
    assert_eq!(
        raw,
        &z[2..z.len() - 4],
        "fallback must reproduce the raw DEFLATE payload"
    );
}
