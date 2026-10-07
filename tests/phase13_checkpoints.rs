#![cfg(all(feature = "rans", feature = "deflate-replay"))]
//! Phase 13.4 court: byte-level partial-materialization checkpoint records.
//!
//! The optional `CHECKPOINT` record (tag `0x60`, `FLAG_OPTIONAL`) carries a
//! bounded per-op output-boundary table. The seek reader consumes it for a raw
//! byte-range query to select the op window **without reading the
//! `OBSERVATION_INDEX` record**; the index lane is the Phase-8 floor.
//!
//! Pre-registered hypotheses:
//!
//!   H1 (byte-exactness) — a checkpointed descriptor materializes exactly, and
//!       the checkpoint lane serves the same bytes as the index lane for every
//!       query; it never changes an output byte.
//!   H2 (advisory) — a lying/corrupt/non-optional checkpoint is *rejected*: the
//!       full parser fails closed and the seek reader falls back to the Phase-8
//!       index lane (identical bytes; it pays only for the rejected read), never
//!       to a guess.
//!   H3 (work) — measured against the Phase-8 seek floor on the same descriptor:
//!       report `bytes_read` with and without the checkpoint for byte-range
//!       queries, across descriptor sizes. A negative (the per-op table costs at
//!       least what the index op table it replaces costs, and the GRAPH dominates
//!       regardless) is an admissible and expected outcome. This test asserts no
//!       direction; it records the measured numbers.
//!
//! The corpus is the deterministic, multi-object `pdf-make-large` document (the
//! same generator as the Phase-7/8 partial courts), plus the small `flate.pdf`
//! sample.

use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};

use vole_document::adapter::pdf::{
    large_pdf, propose_pdf_deflate_replay_rans_indexed, sample_pdfs,
};
use vole_document::container::{CheckpointTable, Descriptor};
use vole_document::limits::Limits;
use vole_document::materialize::materialize;
use vole_document::materialize::observation::{ObservationReport, ObservationSelector};
use vole_document::materialize::seek::materialize_observation_seeked;

const DEFAULT: Limits = Limits::DEFAULT;

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
            "vole-phase13-checkpoints-{tag}-{}-{nanos}",
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

/// The seekable indexed replay descriptor for `source` (the Phase-8 floor), with
/// its consistent checkpoint optionally attached.
fn seekable_descriptor(source: &[u8], with_checkpoint: bool) -> Descriptor {
    let mut d = propose_pdf_deflate_replay_rans_indexed(source, DEFAULT)
        .unwrap()
        .expect("the source has FlateDecode streams")
        .descriptor;
    assert!(
        d.seek_directory,
        "the indexed candidate enables the directory"
    );
    if with_checkpoint {
        let object_lens: Vec<u64> = d.objects.iter().map(|o| o.len()).collect();
        let channel_lens: Vec<u64> = d.channels.iter().map(|c| c.decoded_length).collect();
        let table = CheckpointTable::from_program(
            &d.program,
            &object_lens,
            &channel_lens,
            d.source_len,
            DEFAULT,
        )
        .unwrap();
        d.checkpoints = Some(table);
    }
    d
}

fn write_descriptor(dir: &TempDir, name: &str, d: &Descriptor) -> (PathBuf, Vec<u8>) {
    let (bytes, cost) = d.serialize().unwrap();
    assert_eq!(
        cost.total(),
        bytes.len() as u64,
        "cost accounting must be exact"
    );
    let path = dir.join(name);
    fs::write(&path, &bytes).unwrap();
    (path, bytes)
}

fn seeked(path: &Path, offset: u64, len: u64) -> ObservationReport {
    let file = File::open(path).unwrap();
    materialize_observation_seeked(
        file,
        ObservationSelector::ByteRange { offset, len },
        DEFAULT,
    )
    .unwrap_or_else(|e| panic!("seeked view failed: {e}"))
}

fn sample(name: &str) -> Vec<u8> {
    sample_pdfs()
        .into_iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("sample {name} is missing"))
        .1
}

fn queries(len: u64) -> Vec<(&'static str, u64, u64)> {
    vec![
        ("start", 0, 64),
        ("quarter", len / 4, 64),
        ("middle", len / 2, 256),
        ("end", len - 64, 64),
    ]
}

/// H1 + H3: the checkpoint lane serves the same bytes as the index lane; the
/// measured `bytes_read`/ops are reported across descriptor sizes (the crossover
/// between the per-op checkpoint table and the index record it replaces).
#[test]
fn checkpoint_lane_matches_index_lane_and_reports_work() {
    println!(
        "objects,descriptor_bytes_floor,descriptor_bytes_cp,query,offset,len,bytes_read_floor,bytes_read_cp,delta,ops_eval_floor,ops_eval_cp,output_bytes"
    );
    for objects in [20u64, 40, 120] {
        let source = large_pdf(objects, objects * 8 * 1024);
        let dir = TempDir::new(&format!("large-{objects}"));

        let floor = seekable_descriptor(&source, false);
        let (floor_path, floor_bytes) = write_descriptor(&dir, "floor.voldoc", &floor);
        let checks = seekable_descriptor(&source, true);
        let (cp_path, cp_bytes) = write_descriptor(&dir, "cp.voldoc", &checks);
        assert!(
            cp_bytes.len() > floor_bytes.len(),
            "the checkpoint adds bytes"
        );

        let parsed = Descriptor::parse(&cp_bytes, DEFAULT).unwrap();
        let full = materialize(&parsed, DEFAULT).unwrap();
        assert_eq!(
            full, source,
            "the checkpointed descriptor must materialize exactly"
        );

        let len = full.len() as u64;
        for (tag, off, n) in queries(len) {
            let f = seeked(&floor_path, off, n);
            let c = seeked(&cp_path, off, n);
            assert_eq!(
                c.bytes,
                &full[off as usize..(off + n) as usize],
                "[{objects}/{tag}] checkpoint lane slice must equal the source slice"
            );
            assert_eq!(
                c.bytes, f.bytes,
                "[{objects}/{tag}] checkpoint lane and index lane must agree byte-for-byte"
            );
            assert_eq!(c.range, f.range, "[{objects}/{tag}] resolved ranges agree");
            assert_eq!(
                c.stats.ops_evaluated, f.stats.ops_evaluated,
                "[{objects}/{tag}] the op window must be identical"
            );
            assert!(
                !c.stats.integrity_verified,
                "[{objects}/{tag}] a view is not a verified read"
            );
            let delta = c.stats.bytes_read as i64 - f.stats.bytes_read as i64;
            println!(
                "{objects},{},{},{tag},{off},{n},{},{},{delta},{},{},{}",
                floor_bytes.len(),
                cp_bytes.len(),
                f.stats.bytes_read,
                c.stats.bytes_read,
                f.stats.ops_evaluated,
                c.stats.ops_evaluated,
                c.stats.output_bytes,
            );
        }
        assert!(
            len > 4 * 64,
            "the document must be large enough for the query set"
        );
    }
}

/// H1 on the small sample; H3 determinism.
#[test]
fn checkpoint_is_exact_and_deterministic_on_flate_pdf() {
    let source = sample("flate.pdf");
    let dir = TempDir::new("flate");
    let checks = seekable_descriptor(&source, true);
    let (path, _) = write_descriptor(&dir, "flate.voldoc", &checks);
    let parsed = Descriptor::parse(&fs::read(&path).unwrap(), DEFAULT).unwrap();
    let full = materialize(&parsed, DEFAULT).unwrap();
    assert_eq!(full, source);
    let len = full.len() as u64;
    for (tag, off, n) in queries(len) {
        let a = seeked(&path, off, n);
        let b = seeked(&path, off, n);
        assert_eq!(a.bytes, &full[off as usize..(off + n) as usize], "[{tag}]");
        assert_eq!(a.bytes, b.bytes, "[{tag}] deterministic");
        assert_eq!(
            a.stats.bytes_read, b.stats.bytes_read,
            "[{tag}] deterministic"
        );
    }
}

/// H2: a lying checkpoint is rejected: the full parser fails closed, and the
/// seek reader pays for the (rejected) checkpoint read, then falls back to the
/// index lane and still serves the exact bytes (never a guess, never a denial).
#[test]
fn a_lying_checkpoint_falls_back_to_the_index_lane() {
    let source = large_pdf(40, 40 * 8 * 1024);
    let dir = TempDir::new("lying");

    let floor = seekable_descriptor(&source, false);
    let (floor_path, floor_bytes) = write_descriptor(&dir, "floor.voldoc", &floor);

    // A CRC-valid but lying checkpoint (its GRAPH binding is wrong).
    let mut lying = seekable_descriptor(&source, true);
    lying.checkpoints.as_mut().unwrap().graph_crc32c ^= 0xFFFF_FFFF;
    let (lying_path, lying_bytes) = write_descriptor(&dir, "lying.voldoc", &lying);
    // The full parser rejects it outright — it is never authority.
    assert!(
        Descriptor::parse(&lying_bytes, DEFAULT).is_err(),
        "a lying checkpoint must be rejected by the full parser"
    );

    let parsed = Descriptor::parse(&fs::read(&floor_path).unwrap(), DEFAULT).unwrap();
    let full = materialize(&parsed, DEFAULT).unwrap();
    let len = full.len() as u64;
    let (off, n) = (len / 2, 256);
    let f = seeked(&floor_path, off, n);
    let c = seeked(&lying_path, off, n);
    assert_eq!(c.bytes, &full[off as usize..(off + n) as usize]);
    assert_eq!(c.bytes, f.bytes, "the fallback must serve the same bytes");
    assert_eq!(
        c.stats.ops_evaluated, f.stats.ops_evaluated,
        "the fallback must select the same op window"
    );
    // The rejected checkpoint is read once (its record is the only difference
    // between the two descriptors) and then discarded; the index lane serves.
    let checkpoint_record = lying_bytes.len() as u64 - floor_bytes.len() as u64;
    assert_eq!(
        c.stats.bytes_read,
        f.stats.bytes_read + checkpoint_record,
        "a rejected checkpoint costs exactly its record and then falls back"
    );
}

/// H2: a descriptor with no checkpoint emits no `CHECKPOINT` record and no
/// checkpoint feature bit; an ignorable record never changes universe semantics.
#[test]
fn an_absent_checkpoint_changes_no_bytes_and_still_decodes() {
    let source = sample("flate.pdf");
    let floor = seekable_descriptor(&source, false);
    let (bytes, cost) = floor.serialize().unwrap();
    assert_eq!(cost.checkpoints, 0, "no checkpoint record is charged");
    assert_eq!(
        floor.optional_features() & vole_document::container::FEATURE_CHECKPOINTS,
        0
    );
    let parsed = Descriptor::parse(&bytes, DEFAULT).unwrap();
    assert!(parsed.descriptor.checkpoints.is_none());
    assert_eq!(materialize(&parsed, DEFAULT).unwrap(), source);
}
