//! Phase 10.1 — the governor court (logic, determinism, hypotheses, and the
//! encoder-only / zero-decode-authority grep gate).
//!
//! Compiled only under the non-default `dsfb-search` feature. The hypotheses are
//! pre-registered in `research/subagents/phase-10/dsfb-contract.md` §4.3; H2 is
//! judged only on the disjoint holdout set.
#![cfg(feature = "dsfb-search")]

use std::fs;
use std::path::{Path, PathBuf};

use vole_document::encode::candidates::CandidateKind;
use vole_document::encode::governor::{
    self, Accept, Partition, ReplayMode, SearchConfig, StrategyResult, WorkloadSet, workloads,
};
use vole_document::limits::Limits;

fn run(strategy: &str, bytes: &[u8]) -> StrategyResult {
    let limits = Limits::DEFAULT;
    match strategy {
        "exhaustive" => governor::run_exhaustive(bytes, limits).unwrap(),
        "fixed" => governor::run_fixed(bytes, limits).unwrap(),
        "guided" => governor::run_guided(bytes, limits).unwrap(),
        _ => unreachable!(),
    }
}

fn raw_only(input: &[u8]) -> Vec<u8> {
    vole_document::encode::encode_with(input, Limits::DEFAULT, Some(CandidateKind::Raw))
        .unwrap()
        .0
}

#[test]
fn h1_guided_is_never_worse_than_fixed() {
    for w in workloads() {
        let fixed = run("fixed", &w.bytes);
        let guided = run("guided", &w.bytes);
        assert!(
            guided.final_bytes <= fixed.final_bytes,
            "H1 falsified on {}: guided {} > fixed {}",
            w.name,
            guided.final_bytes,
            fixed.final_bytes
        );
    }
}

#[test]
fn h2_or_h3_holds() {
    // H2 (approaches exhaustive with fewer candidates on the holdout) and H3
    // (honest failure — no measurable byte benefit) are both pre-registered;
    // at least one must hold, and H2 is judged only on the holdout set.
    let mut holdout_total = 0usize;
    let mut holdout_hits = 0usize;
    let mut all_equal = true;
    let mut fixed_total = 0u64;
    let mut exhaustive_total = 0u64;

    for w in workloads() {
        let fixed = run("fixed", &w.bytes);
        let exhaustive = run("exhaustive", &w.bytes);
        let guided = run("guided", &w.bytes);
        fixed_total += fixed.final_bytes;
        exhaustive_total += exhaustive.final_bytes;
        if exhaustive.final_bytes != fixed.final_bytes {
            all_equal = false;
        }
        if w.set == WorkloadSet::Holdout {
            holdout_total += 1;
            let cond = guided.final_bytes == exhaustive.final_bytes
                && guided.candidates_evaluated.saturating_mul(2) <= exhaustive.candidates_evaluated;
            if cond {
                holdout_hits += 1;
            }
        }
    }

    let h2 = holdout_total > 0 && holdout_hits * 5 >= holdout_total * 4;
    let h3 = all_equal || fixed_total == exhaustive_total;
    assert!(
        h2 || h3,
        "neither H2 ({holdout_hits}/{holdout_total}) nor H3 (all_equal={all_equal}) holds"
    );
    // Record the observed outcome so the test log is itself evidence.
    eprintln!(
        "governor court: H2 {holdout_hits}/{holdout_total}; H3 all_equal={all_equal}; \
         fixed_total={fixed_total} exhaustive_total={exhaustive_total}"
    );
}

#[test]
fn h4_negative_controls_stop_raw_byte_exactly() {
    for w in workloads() {
        if !w.negative {
            continue;
        }
        let guided = run("guided", &w.bytes);
        assert_eq!(
            guided.winner,
            CandidateKind::Raw,
            "{} must accept RAW",
            w.name
        );
        assert_eq!(
            guided.stopped,
            Some(Accept::Raw),
            "{} must Stop(Raw)",
            w.name
        );
        assert_eq!(
            guided.bytes,
            raw_only(&w.bytes),
            "{} must match the RAW descriptor byte-for-byte",
            w.name
        );
    }
}

#[test]
fn guided_is_deterministic() {
    for w in workloads() {
        let a = run("guided", &w.bytes);
        let b = run("guided", &w.bytes);
        assert_eq!(
            a.bytes, b.bytes,
            "{} guided bytes must be identical",
            w.name
        );
        assert_eq!(a.final_bytes, b.final_bytes, "{} final bytes", w.name);
        assert_eq!(
            a.candidates_evaluated, b.candidates_evaluated,
            "{} candidates",
            w.name
        );
    }
}

#[test]
fn governed_descriptor_decodes_without_governance() {
    // The decode path has no governor: a governed descriptor is materialized by
    // the ordinary decoder, byte-exactly. (The `--no-default-features` build in
    // the gate then proves the same bytes decode with the feature absent.)
    for w in workloads() {
        let guided = run("guided", &w.bytes);
        let (out, parsed) =
            vole_document::materialize::decode_to_bytes(&guided.bytes, Limits::DEFAULT).unwrap();
        assert_eq!(
            out, w.bytes,
            "{} governed bytes must materialize exactly",
            w.name
        );
        assert_eq!(parsed.descriptor.source_len, w.bytes.len() as u64);
    }
}

#[test]
fn knobs_are_honored_and_wire_legal() {
    let w = workloads()
        .into_iter()
        .find(|w| w.name == "flate.pdf")
        .unwrap();
    let limits = Limits::DEFAULT;

    // `packed = false` drops the packed-framing layout families.
    let unpacked = governor::propose_configured(
        &w.bytes,
        SearchConfig {
            packed: false,
            ..SearchConfig::DEFAULT
        },
        limits,
    )
    .unwrap();
    assert!(!unpacked.iter().any(|c| matches!(
        c.kind,
        CandidateKind::PdfLayout | CandidateKind::PdfLayoutRans
    )));

    // `replay = Off` drops every replay family.
    let no_replay = governor::propose_configured(
        &w.bytes,
        SearchConfig {
            replay: ReplayMode::Off,
            ..SearchConfig::DEFAULT
        },
        limits,
    )
    .unwrap();
    assert!(!no_replay.iter().any(|c| matches!(
        c.kind,
        CandidateKind::PdfDeflateReplay
            | CandidateKind::PdfDeflateReplayRans
            | CandidateKind::PdfDeflateReplayRansIndexed
    )));

    // `depth` is a prefix of the fixed family order.
    let shallow = governor::propose_configured(
        &w.bytes,
        SearchConfig {
            depth: 2,
            ..SearchConfig::DEFAULT
        },
        limits,
    )
    .unwrap();
    for c in &shallow {
        assert!(
            matches!(
                c.kind,
                CandidateKind::Raw | CandidateKind::Rle | CandidateKind::ByteRans
            ),
            "depth=2 emitted {}",
            c.kind.name()
        );
    }

    // Every knob setting still yields byte-exact, decodable candidates.
    for cfg in [
        SearchConfig {
            packed: false,
            ..SearchConfig::DEFAULT
        },
        SearchConfig {
            replay: ReplayMode::Off,
            ..SearchConfig::DEFAULT
        },
        SearchConfig {
            partition: Partition::ByRole,
            ..SearchConfig::DEFAULT
        },
        SearchConfig::by_scale(8),
        SearchConfig::by_scale(10),
    ] {
        for c in governor::propose_configured(&w.bytes, cfg, limits).unwrap() {
            let (encoded, _) = c.descriptor.serialize().unwrap();
            let parsed = vole_document::container::Descriptor::parse(&encoded, limits).unwrap();
            let out = vole_document::materialize::materialize(&parsed, limits).unwrap();
            assert_eq!(
                out,
                w.bytes,
                "{:?} {} must materialize exactly",
                cfg,
                c.kind.name()
            );
        }
    }
}

#[test]
fn by_role_partition_is_used() {
    // The ByRole partition must actually change the typed-channel candidate, but
    // remain decodable and exact.
    let w = workloads()
        .into_iter()
        .find(|w| w.name == "bigtext.pdf")
        .unwrap();
    let limits = Limits::DEFAULT;
    let by_kind = governor::propose_configured(&w.bytes, SearchConfig::DEFAULT, limits).unwrap();
    let by_role = governor::propose_configured(
        &w.bytes,
        SearchConfig {
            partition: Partition::ByRole,
            ..SearchConfig::DEFAULT
        },
        limits,
    )
    .unwrap();
    let kind_bytes = by_kind
        .iter()
        .find(|c| c.kind == CandidateKind::PdfChannels)
        .map(|c| c.descriptor.serialize().unwrap().0);
    let role_bytes = by_role
        .iter()
        .find(|c| c.kind == CandidateKind::PdfChannels)
        .map(|c| c.descriptor.serialize().unwrap().0);
    assert_eq!(kind_bytes.is_some(), role_bytes.is_some());
    if let (Some(a), Some(b)) = (kind_bytes, role_bytes) {
        assert_ne!(a, b, "ByRole must differ from ByKind");
    }
}

/// The encoder-only / zero-decode-authority grep gate.
#[test]
fn decode_path_never_references_the_governor() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let decode_dirs = ["src/materialize", "src/container", "src/dra", "src/store"];
    let mut offenders: Vec<String> = Vec::new();
    for dir in decode_dirs {
        for file in rust_files(&root.join(dir)) {
            let text = fs::read_to_string(&file).unwrap();
            // The decode path must not import the encoder or name the governor.
            if text.contains("governor") || text.contains("crate::encode") {
                offenders.push(file.strip_prefix(&root).unwrap().display().to_string());
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "decode path references governance: {offenders:?}"
    );

    // The only non-governance `dsfb` mention in `src/` is the pre-existing note
    // in `src/store/entropyfs.rs` that the EntropyFS engine pulls `dsfb`
    // transitively — never a governance reference.
    let mut dsfb_files: Vec<String> = Vec::new();
    for file in rust_files(&root.join("src")) {
        if fs::read_to_string(&file).unwrap().contains("dsfb") {
            dsfb_files.push(file.strip_prefix(&root).unwrap().display().to_string());
        }
    }
    assert!(dsfb_files.contains(&"src/store/entropyfs.rs".to_string()));

    // The feature is dependency-free and non-default. (Comments may *mention*
    // `dep:dsfb` to record the verdict; the actual definition must be empty.)
    let cargo = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    let feature_line = cargo
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("dsfb-search"))
        .expect("dsfb-search feature must be declared");
    assert_eq!(
        feature_line, "dsfb-search = []",
        "dsfb-search must be empty"
    );
    let default_line = cargo
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("default ="))
        .unwrap_or("");
    assert!(
        !default_line.contains("dsfb-search"),
        "dsfb-search must be non-default"
    );

    // Feature separation: dsfb-search implies neither store nor entropyfs-store,
    // nor any capability feature.
    for forbidden in ["store", "entropyfs", "rans", "deflate-replay"] {
        assert!(
            !feature_line.contains(forbidden),
            "dsfb-search must not imply {forbidden}"
        );
    }
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}
