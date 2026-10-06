//! Phase 12.11 / 12.12 — library-level mirror of the two shell courts.
//!
//! The measurement courts themselves are shell/Python and run inside the pinned
//! `doc-baseline` / `llm-workingset` services (Docker only). What a Rust test can
//! usefully guard, dependency-free, is the **pre-registration contract** and the
//! static shape the courts depend on:
//!
//! * the committed schedule (`tools/fixtures/phase12-lifetime-schedule.json`)
//!   exists, was frozen before measurement (`pre_registered: true`), names every
//!   document, every pre-registered case id, the four N values, two passes and the
//!   deterministic round-robin expansion rule;
//! * the schedule records the exact corpus source digests and the alpha markers
//!   and expected answers the court asserts;
//! * the corpus generator reuses the 12.9 generator and defines the large `delta`
//!   variant the "the court must be able to lose" property needs;
//! * the baseline engine builds a source-retaining SQLite+FTS5 cache;
//! * both court scripts are present.
//!
//! It deliberately does NOT re-implement the shell courts (that would be a second
//! source of truth); it pins the frozen interface they share.

const SCHEDULE: &str = include_str!("../tools/fixtures/phase12-lifetime-schedule.json");
const CORPUS_GEN: &str = include_str!("../tools/fixtures/phase12-corpus-gen.py");
const BASELINE: &str = include_str!("../tools/fixtures/phase12-baseline.py");
const SCHEDULE_PY: &str = include_str!("../tools/fixtures/phase12-schedule.py");

#[test]
fn schedule_is_pre_registered_and_complete() {
    assert!(SCHEDULE.contains("\"campaign\": \"phase12-lifetime-court\""));
    assert!(
        SCHEDULE.contains("\"pre_registered\": true"),
        "the schedule must declare itself pre-registered"
    );
    assert!(
        SCHEDULE.contains("\"passes\": 2"),
        "two passes (cold + warm)"
    );
    assert!(
        SCHEDULE.contains("\"rule\": \"case(i) = cases[(i-1) mod len(cases)], i = 1..N\""),
        "the expansion rule must be frozen and unambiguous"
    );
    for n in [1, 10, 100, 1000] {
        assert!(
            SCHEDULE.contains(&format!("\n    {n}")) || SCHEDULE.contains(&format!("{n},")),
            "N={n} must be in the pre-registered N set"
        );
    }
    // 12 documents: 4 logical variants x 3 formats.
    for name in [
        "alpha.pdf",
        "alpha.docx",
        "alpha.epub",
        "bravo.pdf",
        "bravo.docx",
        "bravo.epub",
        "charlie.pdf",
        "charlie.docx",
        "charlie.epub",
        "delta.pdf",
        "delta.docx",
        "delta.epub",
    ] {
        assert!(
            SCHEDULE.contains(&format!("\"name\": \"{name}\"")),
            "missing {name}"
        );
    }
    // Every pre-registered case id the courts dispatch on.
    for id in [
        "narrow-text",
        "heading-section",
        "paragraph-context",
        "table-cell",
        "expand-table",
        "resource",
        "metadata",
        "adjacent-region",
        "search",
        "full-source",
        "exact-member",
        "native-provenance",
    ] {
        assert!(
            SCHEDULE.contains(&format!("\"id\": \"{id}\"")),
            "missing case {id}"
        );
    }
    // The alpha expected answers tie the schedule to the canonical 12.9 report.
    assert!(SCHEDULE.contains("\"expected\": \"Introduction\""));
    assert!(SCHEDULE.contains("\"expected\": \"bravo-seven\""));
    assert!(SCHEDULE.contains("Alpha paragraph carries marker XF12A."));
    // The large incompressible variant that makes an A1 win possible exists.
    assert!(SCHEDULE.contains("\"variant\": \"delta\""));
    assert!(SCHEDULE.contains("\"name\": \"delta.docx\""));
}

#[test]
fn corpus_generator_reuses_129_and_defines_a_large_variant() {
    assert!(
        CORPUS_GEN.contains("doc-triplet-gen.py"),
        "must reuse the 12.9 generator"
    );
    assert!(CORPUS_GEN.contains("VARIANT_PARAMS[\"delta\"]"));
    assert!(
        CORPUS_GEN.contains("\"XF15A\""),
        "delta carries its own marker"
    );
    // The alphabet/bravo/charlie/delta variants are all emitted.
    for key in ["alpha", "bravo", "charlie", "delta"] {
        assert!(CORPUS_GEN.contains(&format!("\"{key}\"")));
    }
}

#[test]
fn baseline_engine_is_a_source_retaining_sqlite_fts5_cache() {
    // Source-retaining: the exact original lives in the DB so exact recovery is
    // possible; the schema keeps text, structure, tables/cells, resources, FTS5.
    assert!(BASELINE.contains("CREATE TABLE source_blob"));
    assert!(BASELINE.contains("payload BLOB NOT NULL"));
    assert!(BASELINE.contains("CREATE VIRTUAL TABLE fts USING fts5"));
    assert!(BASELINE.contains("ANALYZE"));
    assert!(BASELINE.contains("CREATE TABLE table_cells"));
    assert!(BASELINE.contains("CREATE TABLE resources"));
    // A0 direct extraction and A1 one-time build both exist.
    assert!(BASELINE.contains("def extract_pdf"));
    assert!(BASELINE.contains("def extract_docx"));
    assert!(BASELINE.contains("def extract_epub"));
}

#[test]
fn schedule_emitter_freezes_the_order() {
    assert!(SCHEDULE_PY.contains("pre_registered"));
    assert!(SCHEDULE_PY.contains("case(i) = cases[(i-1) mod len(cases)]"));
    // Per-format case lists (DOCX keeps list items separate, EPUB merges them).
    assert!(SCHEDULE_PY.contains("DOCX_CASES"));
    assert!(SCHEDULE_PY.contains("EPUB_CASES"));
    assert!(SCHEDULE_PY.contains("PDF_CASES"));
}

#[test]
fn court_scripts_are_present() {
    for p in [
        "tools/phase12-lifetime-court.sh",
        "tools/phase12-llm-court.sh",
        "tools/fixtures/phase12-baseline.py",
    ] {
        assert!(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(p)
                .is_file(),
            "missing {p}"
        );
    }
}
