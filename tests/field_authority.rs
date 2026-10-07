//! Zero-decode-authority gate for the Phase-11 procedural field.
//!
//! The exact materializer must never depend on the field, the seed DAG, the
//! observation planner, a cache, or any search process. This test fails if the
//! decode path references field/search state, so "the field is an additional
//! plane, never a prerequisite" is checked mechanically rather than asserted in
//! prose (ADR-0024, `docs/phases/phase-11-1-governance.md`).

use std::fs;
use std::path::Path;

fn rs_files(dir: &str) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(rs_files(&path.to_string_lossy()));
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path.to_string_lossy().into_owned());
        }
    }
    out.sort();
    out
}

/// Patterns that would indicate the decode path consulting field/search state.
const FORBIDDEN: &[&str] = &["field::", "SeedStore", "seed::", "dsfb", "NodeId"];

#[test]
fn decode_path_never_references_the_field_or_search() {
    let mut hits: Vec<String> = Vec::new();
    for dir in [
        "src/materialize",
        "src/container",
        "src/dra",
        "src/entropy",
        "src/codec",
    ] {
        for file in rs_files(dir) {
            let src = fs::read_to_string(&file).unwrap_or_default();
            for (i, line) in src.lines().enumerate() {
                let trimmed = line.trim_start();
                // Comments and doc-links are not code.
                if trimmed.starts_with("//") || trimmed.starts_with("*") {
                    continue;
                }
                for pat in FORBIDDEN {
                    if line.contains(pat) {
                        hits.push(format!(
                            "{}:{}: {}",
                            Path::new(&file).display(),
                            i + 1,
                            line.trim()
                        ));
                    }
                }
            }
        }
    }
    assert!(
        hits.is_empty(),
        "the decode path referenced field/search state (zero-authority violation):\n{}",
        hits.join("\n")
    );
}

/// A descriptor with no field state materializes exactly, and the field's
/// presence is never required to decode it.
#[test]
fn field_is_not_required_to_materialize() {
    use vole_document::container::{Descriptor, ObjectSource};
    use vole_document::dra::{Op, Program};
    use vole_document::materialize::{decode_to_bytes, materialize};
    use vole_document::{Limits, SOURCE_FORMAT_OPAQUE};

    let source = b"a descriptor that never knew about fields";
    let d = Descriptor {
        universe: vole_document::container::UNIVERSE.to_string(),
        source_format: SOURCE_FORMAT_OPAQUE,
        format_basis: "opaque:gate".to_string(),
        models: vec![],
        channels: vec![],
        objects: vec![ObjectSource::Inline(source.to_vec())],
        program: Program::new(vec![Op::EmitObject { object_id: 0 }]),
        observation_index: None,
        seek_directory: false,
        checkpoints: None,
        source_sha256: vole_document::integrity::sha256(source),
        source_len: source.len() as u64,
    };
    let (bytes, _) = d.serialize().unwrap();
    let (out, parsed) = decode_to_bytes(&bytes, Limits::DEFAULT).unwrap();
    assert_eq!(out, source);
    assert_eq!(materialize(&parsed, Limits::DEFAULT).unwrap(), source);
}
