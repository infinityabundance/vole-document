#![cfg(all(feature = "rans", feature = "deflate-replay"))]
//! Phase 8.2 court: the seek reader serves the same slice as the full
//! materializer while reading only the records the query needs.
//!
//! The seek reader is only a bytes-read win for descriptors whose reconstructed
//! source lives in many `OBJECT`/`ENTROPY_CHANNEL` records behind per-region ops
//! (the `PDF_DEFLATE_REPLAY_RANS_INDEXED` lane); a `PDF_PHYSICAL` descriptor
//! inlines the whole source into a single `GRAPH` op, so it is not tested here.
//!
//! For the `flate.pdf` sample and a modest `large_pdf`, this court builds the
//! seekable indexed candidate, writes it to a temporary file, and asserts for
//! start/middle/end byte ranges and an indirect object that:
//!   (a) the seeked slice equals the full materialization slice byte-for-byte;
//!   (b) `bytes_read` is strictly less than the descriptor file size;
//!   (c) a late query on the large PDF reads a small fraction of the file;
//!   (d) a descriptor without a directory is declined (never silently read whole);
//!   (e) the result is deterministic.

use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};

use vole_document::Error;
use vole_document::ErrorClass;
use vole_document::adapter::pdf::{
    large_pdf, propose_pdf_deflate_replay_rans, propose_pdf_deflate_replay_rans_indexed,
    sample_pdfs, scan,
};
use vole_document::container::Descriptor;
use vole_document::limits::Limits;
use vole_document::materialize::materialize;
use vole_document::materialize::observation::{
    ObservationReport, ObservationSelector, materialize_observation,
};
use vole_document::materialize::seek::materialize_observation_seeked;

const DEFAULT: Limits = Limits::DEFAULT;

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
            "vole-observation-seek-{tag}-{}-{nanos}",
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

fn sample(name: &str) -> Vec<u8> {
    sample_pdfs()
        .into_iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("sample {name} is missing from the corpus"))
        .1
}

fn seeked(path: &Path, selector: ObservationSelector) -> ObservationReport {
    let file = File::open(path).unwrap();
    materialize_observation_seeked(file, selector, DEFAULT)
        .unwrap_or_else(|e| panic!("seeked view failed: {e}"))
}

fn seeked_err(path: &Path, selector: ObservationSelector) -> Error {
    let file = File::open(path).unwrap();
    materialize_observation_seeked(file, selector, DEFAULT).unwrap_err()
}

/// Serialize the seekable indexed replay candidate for `source` to a temp file
/// and return `(path, descriptor_bytes, full_materialization)`.
fn persist_seekable(dir: &TempDir, source: &[u8], name: &str) -> (PathBuf, Vec<u8>, Vec<u8>) {
    let candidate = propose_pdf_deflate_replay_rans_indexed(source, DEFAULT)
        .unwrap()
        .expect("the sample has FlateDecode streams");
    assert!(
        candidate.descriptor.seek_directory,
        "the indexed candidate must enable the seek directory"
    );
    let (bytes, cost) = candidate.descriptor.serialize().unwrap();
    assert!(
        cost.directory > 0,
        "an enabled directory must be charged ({cost:?})"
    );
    let path = dir.join(name);
    fs::write(&path, &bytes).unwrap();
    let re_read = fs::read(&path).unwrap();
    assert_eq!(
        re_read, bytes,
        "the descriptor must round-trip byte-for-byte"
    );

    let parsed = Descriptor::parse(&re_read, DEFAULT).unwrap();
    assert!(
        parsed.descriptor.seek_directory,
        "the parse must recover the directory flag"
    );
    let full = materialize(&parsed, DEFAULT).unwrap();
    assert_eq!(full, source, "the disk descriptor must materialize exactly");
    (path, bytes, full)
}

/// (a)+(b)+(e): each window equals the full slice, reads less than the file, and
/// is deterministic.
fn assert_windows(path: &Path, file_len: usize, full: &[u8], ranges: &[(&str, u64, u64)]) {
    for (tag, offset, len) in ranges {
        let selector = ObservationSelector::ByteRange {
            offset: *offset,
            len: *len,
        };
        let report = seeked(path, selector);
        let a = *offset as usize;
        assert_eq!(
            report.bytes,
            &full[a..a + *len as usize],
            "[{tag}] seeked slice must equal the full slice"
        );
        assert_eq!(report.range, (*offset, *offset + *len));
        assert_eq!(report.stats.output_bytes, *len);
        assert!(
            !report.stats.integrity_verified,
            "[{tag}] partial view is not a verified read"
        );
        assert!(
            report.stats.bytes_read < file_len as u64,
            "[{tag}] bytes_read {} must be < descriptor size {file_len}",
            report.stats.bytes_read
        );

        // (e) determinism.
        let again = seeked(path, selector);
        assert_eq!(again.bytes, report.bytes, "[{tag}] bytes are deterministic");
        assert_eq!(
            again.stats.bytes_read, report.stats.bytes_read,
            "[{tag}] bytes_read is deterministic"
        );
    }
}

#[test]
fn seeked_view_matches_full_on_flate_pdf() {
    let source = sample("flate.pdf");
    let dir = TempDir::new("flate");
    let (path, bytes, full) = persist_seekable(&dir, &source, "flate.voldoc");

    let len = full.len() as u64;
    let window = 64u64;
    assert!(len > 3 * window, "flate.pdf is too small for 3 windows");
    let ranges = [
        ("start", 0u64, window),
        ("middle", len / 2, window),
        ("end", len - window, window),
    ];
    assert_windows(&path, bytes.len(), &full, &ranges);

    // One indirect object equals its scanner span.
    let physical = scan(&source, DEFAULT).unwrap();
    let obj = physical
        .objects
        .get(physical.objects.len() / 2)
        .expect("flate.pdf has indirect objects");
    let report = seeked(
        &path,
        ObservationSelector::PdfIndirectObject {
            object: obj.number as u32,
            generation: obj.generation as u16,
        },
    );
    assert_eq!(
        report.bytes,
        &source[obj.start as usize..obj.end as usize],
        "seeked indirect object must equal physical::scan"
    );
    assert!(
        report.stats.bytes_read < bytes.len() as u64,
        "object query bytes_read {} must be < descriptor size {}",
        report.stats.bytes_read,
        bytes.len()
    );

    // The in-memory Phase-7 path agrees on the same descriptor (the seek reader
    // cannot diverge from it).
    let parsed = Descriptor::parse(&bytes, DEFAULT).unwrap();
    let in_memory = materialize_observation(
        &parsed,
        ObservationSelector::ByteRange {
            offset: 0,
            len: window,
        },
        DEFAULT,
    )
    .unwrap();
    assert_eq!(in_memory.bytes, &full[..window as usize]);
}

#[test]
fn late_query_on_large_pdf_reads_a_small_fraction() {
    const OBJECTS: u64 = 40;
    const TARGET_BYTES: u64 = 40 * 8 * 1024;
    let source = large_pdf(OBJECTS, TARGET_BYTES);
    assert!(source.starts_with(b"%PDF-1.5\n"));

    let dir = TempDir::new("large");
    let (path, bytes, full) = persist_seekable(&dir, &source, "large.voldoc");

    let len = full.len() as u64;
    let window = 64u64;
    let ranges = [
        ("start", 0u64, window),
        ("middle", len / 2, window),
        ("end", len - window, window),
    ];
    assert_windows(&path, bytes.len(), &full, &ranges);

    // (c) A late query must read a small fraction of the descriptor.
    let offset = len - window;
    let report = seeked(
        &path,
        ObservationSelector::ByteRange {
            offset,
            len: window,
        },
    );
    let fraction = report.stats.bytes_read as f64 / bytes.len() as f64;
    println!(
        "late query: bytes_read={} descriptor_size={} fraction={:.4}",
        report.stats.bytes_read,
        bytes.len(),
        fraction
    );
    assert_eq!(report.bytes, &full[offset as usize..]);
    assert!(
        report.stats.bytes_read * 2 < bytes.len() as u64,
        "late query bytes_read {} must be < 50% of descriptor size {}",
        report.stats.bytes_read,
        bytes.len()
    );
}

#[test]
fn a_descriptor_without_a_directory_is_declined() {
    let source = sample("flate.pdf");

    // (i) An indexed-but-not-seekable descriptor: the header declares the index
    // feature but not the seek feature, so the seek path must decline.
    let mut candidate = propose_pdf_deflate_replay_rans_indexed(&source, DEFAULT)
        .unwrap()
        .unwrap();
    candidate.descriptor.seek_directory = false;
    let (bytes, cost) = candidate.descriptor.serialize().unwrap();
    assert_eq!(cost.directory, 0);
    let dir = TempDir::new("decline-indexed");
    let path = dir.join("in.voldoc");
    fs::write(&path, &bytes).unwrap();
    let err = seeked_err(&path, ObservationSelector::ByteRange { offset: 0, len: 1 });
    assert_eq!(
        err.class(),
        ErrorClass::UnsupportedFeature,
        "a descriptor without a directory must decline with UnsupportedFeature"
    );

    // (ii) The plain un-indexed replay descriptor is likewise declined.
    let unindexed = propose_pdf_deflate_replay_rans(&source, DEFAULT)
        .unwrap()
        .expect("flate.pdf has FlateDecode streams");
    let (bytes, _) = unindexed.descriptor.serialize().unwrap();
    let path = dir.join("unindexed.voldoc");
    fs::write(&path, &bytes).unwrap();
    let err = seeked_err(&path, ObservationSelector::ByteRange { offset: 0, len: 1 });
    assert_eq!(err.class(), ErrorClass::UnsupportedFeature);
}
