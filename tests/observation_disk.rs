#![cfg(all(feature = "rans", feature = "deflate-replay"))]
//! Phase 7.3 durability court: an indexed descriptor served from a real file.
//!
//! The in-memory courts (`observation.rs`, `observation_cli.rs`) serve views from
//! committed samples held in RAM. This court closes the durability gap: it
//! generates a PDF with the **same shared generator** as `pdf-make-large`
//! ([`large_pdf`]), encodes it with the `--force`-equivalent
//! `propose_pdf_deflate_replay_rans_indexed` candidate, writes the serialized
//! descriptor to a temporary file, **re-reads** it, parses exactly those re-read
//! bytes, and asserts `materialize_observation` returns the same bytes the full
//! materializer does — for a start, a middle, and an end byte range, and for one
//! indirect-object and one encoded-stream selector checked against an independent
//! `physical::scan`.
//!
//! `OBJECTS` is deliberately small: this court tests the disk round-trip, not
//! corpus scale (the sealed campaign uses 800). It stays deterministic and fast
//! (well under a few seconds) and creates no shared state.

use std::fs;
use std::path::PathBuf;

use vole_document::adapter::pdf::{large_pdf, propose_pdf_deflate_replay_rans_indexed, scan};
use vole_document::container::Descriptor;
use vole_document::limits::Limits;
use vole_document::materialize::materialize;
use vole_document::materialize::observation::{ObservationSelector, materialize_observation};

const DEFAULT: Limits = Limits::DEFAULT;
/// Pages (and `FlateDecode` streams) in the generated corpus PDF.
const OBJECTS: u64 = 40;
/// Source-size floor; scaled across `OBJECTS`, this yields ~320 KiB of source.
const TARGET_BYTES: u64 = 40 * 8 * 1024;

/// A self-cleaning temporary directory (no `tempfile` dependency), mirroring
/// `tests/observation_cli.rs`.
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
            "vole-observation-disk-{tag}-{}-{nanos}",
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

#[test]
fn indexed_descriptor_survives_a_disk_round_trip() {
    // 1. Generate via the shared `pdf-make-large` generator (small OBJECTS).
    let source = large_pdf(OBJECTS, TARGET_BYTES);
    assert!(source.starts_with(b"%PDF-1.5\n"));
    assert!(source.ends_with(b"%%EOF\n"));

    // 2. `--force pdf-deflate-replay-rans-indexed`-equivalent encode.
    let candidate = propose_pdf_deflate_replay_rans_indexed(&source, DEFAULT)
        .unwrap()
        .expect("the generated corpus has distinct FlateDecode streams");
    assert!(
        candidate.descriptor.observation_index.is_some(),
        "the indexed lane must carry an observation index"
    );
    let (bytes, _cost) = candidate.descriptor.serialize().unwrap();

    // 3. Persist and re-read: the descriptor under test is what came back from
    //    disk, not the in-memory serialization.
    let dir = TempDir::new("roundtrip");
    let path = dir.join("large.pdf-deflate-replay-rans-indexed.voldoc");
    fs::write(&path, &bytes).unwrap();
    let re_read = fs::read(&path).unwrap();
    assert_eq!(
        re_read, bytes,
        "the descriptor must round-trip byte-for-byte"
    );

    let parsed = Descriptor::parse(&re_read, DEFAULT).unwrap();
    let full = materialize(&parsed, DEFAULT).unwrap();
    assert_eq!(full, source, "the disk descriptor must materialize exactly");

    // 4. Start / middle / end byte ranges equal the full-materialization slices.
    let len = full.len() as u64;
    let window = 48u64;
    assert!(
        len > 3 * window,
        "generated source is too small for 3 windows"
    );
    let ranges = [
        ("start", 0u64, window),
        ("middle", len / 2, window),
        ("end", len - window, window),
    ];
    for (tag, offset, wlen) in ranges {
        let report = materialize_observation(
            &parsed,
            ObservationSelector::ByteRange { offset, len: wlen },
            DEFAULT,
        )
        .unwrap_or_else(|e| panic!("[{tag}] disk observation failed: {e}"));
        let a = offset as usize;
        assert_eq!(
            report.bytes,
            &full[a..a + wlen as usize],
            "[{tag}] disk-served slice must equal the full materialization slice"
        );
        assert_eq!(report.stats.output_bytes, wlen);
    }

    // 5. One indirect-object and one encoded-stream selector, each compared
    //    against an independent `physical::scan` of the source.
    let physical = scan(&source, DEFAULT).unwrap();

    let object = physical
        .objects
        .get(physical.objects.len() / 2)
        .expect("the generated PDF has many indirect objects");
    let report = materialize_observation(
        &parsed,
        ObservationSelector::PdfIndirectObject {
            object: object.number as u32,
            generation: object.generation as u16,
        },
        DEFAULT,
    )
    .unwrap();
    assert_eq!(
        report.bytes,
        &source[object.start as usize..object.end as usize],
        "indirect-object bytes must match physical::scan"
    );

    let stream = physical
        .streams
        .get(physical.streams.len() / 2)
        .expect("the generated PDF has many FlateDecode streams");
    let report = materialize_observation(
        &parsed,
        ObservationSelector::PdfEncodedStream {
            object: stream.object as u32,
            generation: stream.generation as u16,
        },
        DEFAULT,
    )
    .unwrap();
    let start = stream.data_start as usize;
    let end = start + stream.data_len as usize;
    assert_eq!(
        report.bytes,
        &source[start..end],
        "encoded-stream bytes must match physical::scan"
    );
}
