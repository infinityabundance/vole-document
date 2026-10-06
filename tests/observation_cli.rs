#![cfg(all(feature = "rans", feature = "deflate-replay"))]
//! Phase 7.3-i3 court: the indexed replay candidate and the `view` CLI.
//!
//! Three things must hold. (1) `PDF_DEFLATE_REPLAY_RANS_INDEXED` serializes
//! byte-exactly and is larger than the un-indexed `PDF_DEFLATE_REPLAY_RANS` by
//! exactly the charged index cost, so the index is never free. (2)
//! `materialize_observation` (and therefore `view`) serves a narrow slice equal
//! to the corresponding slice of the full reconstruction, for every selector. (3)
//! an un-indexed descriptor is declined by `view`, never silently fully
//! materialized.
//!
//! The `flate.pdf` sample carries real `/FlateDecode` streams, so the indexed
//! replay candidate is live there. `bigtext.pdf` has no FlateDecode stream, so
//! the indexed replay candidate honestly declines; the `view` plumbing is still
//! exercised on `bigtext.pdf` through a `PDF_PHYSICAL`-based index so a second,
//! larger sample covers the same code path.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use vole_document::ErrorClass;
use vole_document::adapter::pdf::{
    PdfPhysical, propose_pdf, propose_pdf_deflate_replay_rans,
    propose_pdf_deflate_replay_rans_indexed, sample_pdfs, scan,
};
use vole_document::container::Descriptor;
use vole_document::container::observation::{
    DEP_NONE, ObservationIndex, ObservationSelector as IndexSelector, OpEntry, SECTION_OP_TABLE,
    SECTION_PDF_SELECTORS, SELECTOR_OBJECT, SELECTOR_REVISION, SELECTOR_STREAM,
};
use vole_document::encode::candidates::CandidateKind;
use vole_document::encode::encode_with;
use vole_document::limits::Limits;
use vole_document::materialize::materialize;
use vole_document::materialize::observation::{ObservationSelector, materialize_observation};

const DEFAULT: Limits = Limits::DEFAULT;
/// The built CLI binary, invoked as a process to exercise the `view` front end.
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
            "vole-observation-cli-{tag}-{}-{nanos}",
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

/// Run `view INPUT <extra...>` and return the captured process output.
fn run_view(input: &Path, extra: &[String]) -> std::process::Output {
    let mut cmd = Command::new(CLI);
    cmd.arg("view").arg(input);
    for a in extra {
        cmd.arg(a);
    }
    cmd.output().expect("spawn CLI view")
}

/// Write `bytes` to `dir/in.voldoc` and return the path.
fn write_input(dir: &TempDir, bytes: &[u8]) -> PathBuf {
    let path = dir.join("in.voldoc");
    fs::write(&path, bytes).unwrap();
    path
}

/// Build a `PDF_PHYSICAL` descriptor with an observation index derived in the
/// test from an independent `physical::scan`. This is a fixture for exercising
/// the `view` plumbing on files the replay lane does not cover; the production
/// index builder is tested on `flate.pdf` above.
fn indexed_physical_descriptor(source: &[u8]) -> (Descriptor, PdfPhysical) {
    let physical = scan(source, DEFAULT).unwrap();
    let mut d = propose_pdf(source, DEFAULT)
        .unwrap()
        .expect("a validated PDF must propose PDF_PHYSICAL")
        .descriptor;

    let object_lens: Vec<u64> = d.objects.iter().map(|o| o.len()).collect();
    let channel_lens: Vec<u64> = d.channels.iter().map(|c| c.decoded_length).collect();
    let per_op = d
        .program
        .analyze_ops(&object_lens, &channel_lens, DEFAULT)
        .unwrap();
    let ops: Vec<OpEntry> = per_op
        .iter()
        .map(|&len| OpEntry {
            out_len: u32::try_from(len).unwrap(),
            dep_kind: DEP_NONE,
            dep_id: 0,
        })
        .collect();

    let mut selectors: Vec<IndexSelector> = Vec::new();
    for o in &physical.objects {
        selectors.push(IndexSelector {
            kind: SELECTOR_OBJECT,
            number: o.number as u32,
            generation: o.generation as u32,
            out_off: o.start,
            out_len: o.end - o.start,
        });
    }
    for s in &physical.streams {
        if s.data_len > 0 {
            selectors.push(IndexSelector {
                kind: SELECTOR_STREAM,
                number: s.object as u32,
                generation: s.generation as u32,
                out_off: s.data_start,
                out_len: s.data_len,
            });
        }
    }
    for r in &physical.revisions {
        selectors.push(IndexSelector {
            kind: SELECTOR_REVISION,
            number: r.index,
            generation: 0,
            out_off: r.start,
            out_len: r.end - r.start,
        });
    }
    selectors.sort_by_key(|s| (s.kind, s.number, s.generation, s.out_off));

    d.observation_index = Some(ObservationIndex {
        section_flags: SECTION_OP_TABLE | SECTION_PDF_SELECTORS,
        ops,
        selectors,
        digests: Vec::new(),
    });
    (d, physical)
}

// ---------------------------------------------------------------------------
// (1) Indexed candidate: exact, larger by exactly the index cost, deterministic.
// ---------------------------------------------------------------------------

#[test]
fn indexed_candidate_is_exact_charged_and_deterministic() {
    let src = sample("flate.pdf");

    let indexed = propose_pdf_deflate_replay_rans_indexed(&src, DEFAULT)
        .unwrap()
        .expect("flate.pdf has FlateDecode streams");
    assert_eq!(indexed.kind, CandidateKind::PdfDeflateReplayRansIndexed);
    assert_eq!(indexed.kind.name(), "PDF_DEFLATE_REPLAY_RANS_INDEXED");
    assert!(
        indexed.descriptor.observation_index.is_some(),
        "the indexed candidate must carry an index"
    );
    assert!(
        indexed.descriptor.seek_directory,
        "only the indexed candidate enables the seek directory"
    );

    let (indexed_bytes, indexed_cost) = indexed.descriptor.serialize().unwrap();
    assert!(
        indexed_cost.directory > 0,
        "an enabled seek directory must be charged"
    );
    // Parsing the serialized bytes recovers the directory (cost and flag).
    let reparsed = Descriptor::parse(&indexed_bytes, DEFAULT).unwrap();
    assert!(reparsed.descriptor.seek_directory);
    assert_eq!(reparsed.cost.directory, indexed_cost.directory);
    let unindexed = propose_pdf_deflate_replay_rans(&src, DEFAULT)
        .unwrap()
        .expect("flate.pdf has FlateDecode streams");
    let (unindexed_bytes, _) = unindexed.descriptor.serialize().unwrap();

    // The only structural differences are the optional index record and (Phase 8)
    // the optional seek DIRECTORY record, so the size delta is exactly the charged
    // index cost plus the charged directory cost.
    assert!(
        indexed_bytes.len() > unindexed_bytes.len(),
        "indexed {} must exceed un-indexed {}",
        indexed_bytes.len(),
        unindexed_bytes.len()
    );
    assert_eq!(
        (indexed_bytes.len() - unindexed_bytes.len()) as u64,
        indexed_cost.index + indexed_cost.directory,
        "size delta must equal the charged index plus directory cost"
    );
    assert_eq!(indexed_cost.total(), indexed_bytes.len() as u64);

    // Both variants are byte-exact.
    for (bytes, label) in [(&indexed_bytes, "indexed"), (&unindexed_bytes, "unindexed")] {
        let (out, _) = vole_document::materialize::decode_to_bytes(bytes, DEFAULT).unwrap();
        assert_eq!(out, src, "[{label}] must reconstruct the source");
    }

    // Determinism: proposing twice is byte-identical, and forcing the kind in the
    // court reproduces the same bytes.
    let again = propose_pdf_deflate_replay_rans_indexed(&src, DEFAULT)
        .unwrap()
        .unwrap()
        .descriptor
        .serialize()
        .unwrap()
        .0;
    assert_eq!(indexed_bytes, again, "proposal must be deterministic");
    let (a, report_a) = encode_with(
        &src,
        DEFAULT,
        Some(CandidateKind::PdfDeflateReplayRansIndexed),
    )
    .unwrap();
    let (b, _) = encode_with(
        &src,
        DEFAULT,
        Some(CandidateKind::PdfDeflateReplayRansIndexed),
    )
    .unwrap();
    assert_eq!(a, b, "encode twice must be identical");
    assert_eq!(report_a.candidates_evaluated, 1);
    assert_eq!(
        a, indexed_bytes,
        "forced bytes equal the proposed descriptor"
    );
}

// ---------------------------------------------------------------------------
// (2) `view` serves exact slices for every selector.
// ---------------------------------------------------------------------------

#[test]
fn view_serves_mid_byte_range_from_indexed_candidate() {
    let src = sample("flate.pdf");
    let (bytes, _) = propose_pdf_deflate_replay_rans_indexed(&src, DEFAULT)
        .unwrap()
        .unwrap()
        .descriptor
        .serialize()
        .unwrap();

    let dir = TempDir::new("mid");
    let inp = write_input(&dir, &bytes);
    let offset = (src.len() / 2) as u64;
    let len = 64u64;

    let out = run_view(&inp, &[format!("--byte-range={offset}:{len}")]);
    assert!(out.status.success(), "view failed: {:?}", out.status);
    assert_eq!(
        out.stdout,
        &src[offset as usize..(offset + len) as usize],
        "mid-file byte range must equal the source slice"
    );
    let stats = String::from_utf8_lossy(&out.stderr);
    assert!(
        stats.contains("\"work_amplification\""),
        "stats JSON must carry work_amplification: {stats}"
    );
    assert!(
        stats.contains("\"selector\":\"byte-range:"),
        "stats JSON must carry the selector: {stats}"
    );
    assert!(
        stats.contains("\"bytes_read\":"),
        "stats JSON must carry the real bytes_read: {stats}"
    );
    assert!(
        stats.contains("\"integrity_verified\":false"),
        "a partial view must report integrity_verified false: {stats}"
    );
}

#[test]
fn view_serves_pdf_selectors_from_indexed_candidate() {
    let src = sample("flate.pdf");
    let (bytes, _) = propose_pdf_deflate_replay_rans_indexed(&src, DEFAULT)
        .unwrap()
        .unwrap()
        .descriptor
        .serialize()
        .unwrap();
    let dir = TempDir::new("pdfsel");
    let inp = write_input(&dir, &bytes);
    let physical = scan(&src, DEFAULT).unwrap();

    // PdfIndirectObject: the full object span from physical::scan.
    let obj = physical
        .objects
        .iter()
        .find(|o| o.number == 4)
        .expect("flate.pdf has object 4");
    let out = run_view(
        &inp,
        &[format!("--pdf-object={}:{}", obj.number, obj.generation)],
    );
    assert!(out.status.success(), "pdf-object failed: {:?}", out.status);
    assert_eq!(
        out.stdout,
        &src[obj.start as usize..obj.end as usize],
        "object 4 must equal its scanner span"
    );

    // PdfEncodedStream: the physical stream-data span.
    let stream = physical.streams.first().expect("flate.pdf has streams");
    let out = run_view(
        &inp,
        &[format!(
            "--pdf-stream={}:{}",
            stream.object, stream.generation
        )],
    );
    assert!(out.status.success(), "pdf-stream failed: {:?}", out.status);
    let ds = stream.data_start as usize;
    assert_eq!(
        out.stdout,
        &src[ds..ds + stream.data_len as usize],
        "encoded stream must equal its data slice"
    );

    // PdfRevision: the revision's byte extent.
    let rev = physical
        .revisions
        .first()
        .expect("flate.pdf has a revision");
    let out = run_view(&inp, &[format!("--pdf-revision={}", rev.index)]);
    assert!(
        out.status.success(),
        "pdf-revision failed: {:?}",
        out.status
    );
    assert_eq!(
        out.stdout,
        &src[rev.start as usize..rev.end as usize],
        "revision must equal its byte extent"
    );
}

#[test]
fn view_whole_range_equals_full_materialization() {
    let src = sample("flate.pdf");
    let cand = propose_pdf_deflate_replay_rans_indexed(&src, DEFAULT)
        .unwrap()
        .unwrap();
    let (bytes, _) = cand.descriptor.serialize().unwrap();
    let parsed = Descriptor::parse(&bytes, DEFAULT).unwrap();
    let full = materialize(&parsed, DEFAULT).unwrap();

    let dir = TempDir::new("whole");
    let inp = write_input(&dir, &bytes);
    let out = run_view(&inp, &[format!("--byte-range=0:{}", src.len())]);
    assert!(out.status.success(), "whole range failed: {:?}", out.status);
    assert_eq!(
        out.stdout, full,
        "whole range must equal the full materialization"
    );
}

#[test]
fn view_stats_only_prints_json_without_bytes() {
    let src = sample("flate.pdf");
    let (bytes, _) = propose_pdf_deflate_replay_rans_indexed(&src, DEFAULT)
        .unwrap()
        .unwrap()
        .descriptor
        .serialize()
        .unwrap();
    let dir = TempDir::new("stats");
    let inp = write_input(&dir, &bytes);

    let out = run_view(
        &inp,
        &[
            format!("--byte-range=0:{}", src.len()),
            "--stats".to_string(),
        ],
    );
    assert!(out.status.success(), "stats-only failed: {:?}", out.status);
    let json = String::from_utf8(out.stdout).expect("stats-only stdout is JSON text");
    assert!(json.contains("\"work_amplification\""), "{json}");
    assert!(json.contains("\"bytes\":"), "{json}");
    assert!(
        json.len() < src.len(),
        "stats-only output must not be the document bytes"
    );
}

// ---------------------------------------------------------------------------
// (3) An un-indexed descriptor is declined, never silently fully materialized.
// ---------------------------------------------------------------------------

#[test]
fn view_declines_unindexed_descriptor() {
    let src = sample("flate.pdf");
    let unindexed = propose_pdf_deflate_replay_rans(&src, DEFAULT)
        .unwrap()
        .expect("flate.pdf has FlateDecode streams");
    assert!(unindexed.descriptor.observation_index.is_none());
    let (bytes, _) = unindexed.descriptor.serialize().unwrap();

    let dir = TempDir::new("decline");
    let inp = write_input(&dir, &bytes);
    let out = run_view(&inp, &[format!("--byte-range=0:{}", src.len())]);
    assert_eq!(
        out.status.code(),
        Some(ErrorClass::UnsupportedFeature.exit_code()),
        "an un-indexed descriptor must decline with UnsupportedFeature"
    );
    assert!(
        out.stdout.is_empty(),
        "a declined view must not emit bytes to stdout"
    );

    // The library declines identically (no silent full materialization).
    let parsed = Descriptor::parse(&bytes, DEFAULT).unwrap();
    let err = materialize_observation(
        &parsed,
        ObservationSelector::ByteRange { offset: 0, len: 1 },
        DEFAULT,
    )
    .unwrap_err();
    assert_eq!(err.class(), ErrorClass::UnsupportedFeature);
}

// ---------------------------------------------------------------------------
// (4) The indexed replay candidate honestly declines without FlateDecode streams.
// ---------------------------------------------------------------------------

#[test]
fn indexed_candidate_declines_without_flate_streams() {
    let src = sample("bigtext.pdf");
    assert!(
        propose_pdf_deflate_replay_rans(&src, DEFAULT)
            .unwrap()
            .is_none(),
        "bigtext.pdf has no FlateDecode streams"
    );
    assert!(
        propose_pdf_deflate_replay_rans_indexed(&src, DEFAULT)
            .unwrap()
            .is_none(),
        "the indexed replay lane must decline when there is nothing to replay"
    );
}

// ---------------------------------------------------------------------------
// (5) `view` on a second, larger sample through a PDF_PHYSICAL-based index.
// ---------------------------------------------------------------------------

#[test]
fn view_serves_bigtext_physical_slices() {
    let src = sample("bigtext.pdf");
    let (d, physical) = indexed_physical_descriptor(&src);
    let (bytes, _) = d.serialize().unwrap();
    let dir = TempDir::new("bigtext");
    let inp = write_input(&dir, &bytes);

    // (a) mid-file byte range.
    let offset = (src.len() / 2) as u64;
    let len = 64u64;
    let out = run_view(&inp, &[format!("--byte-range={offset}:{len}")]);
    assert!(out.status.success(), "bigtext mid range: {:?}", out.status);
    assert_eq!(out.stdout, &src[offset as usize..(offset + len) as usize]);

    // (b) an indirect object equals its scanner span.
    let obj = physical.objects.iter().find(|o| o.number == 4).unwrap();
    let out = run_view(
        &inp,
        &[format!("--pdf-object={}:{}", obj.number, obj.generation)],
    );
    assert!(out.status.success(), "bigtext object: {:?}", out.status);
    assert_eq!(out.stdout, &src[obj.start as usize..obj.end as usize]);

    // (c) the whole range equals the full materialization.
    let parsed = Descriptor::parse(&bytes, DEFAULT).unwrap();
    let full = materialize(&parsed, DEFAULT).unwrap();
    let out = run_view(&inp, &[format!("--byte-range=0:{}", src.len())]);
    assert!(out.status.success(), "bigtext whole: {:?}", out.status);
    assert_eq!(out.stdout, full);
}
