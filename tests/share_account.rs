#![cfg(feature = "field")]
//! Phase 11.14: the `share-account` CLI verb.
//!
//! `share-account --store DIR INPUT.voldoc...` stores every inline fine unit of
//! the cohort in `<DIR>/share` and prints the `ShareReport` as JSON. This court
//! checks the verb end to end against the real CLI binary:
//!
//! * the physical store holds exactly the reported unique units
//!   (`store_bytes == unique_bytes`);
//! * byte-identical descriptors share every unit, so two copies have the same
//!   `unique_bytes` and twice the `total_bytes` of one;
//! * the output is deterministic;
//! * the usage errors fail closed (missing `--store`, no inputs).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use vole_document::adapter::pdf::sample_pdfs;

/// The built CLI binary, invoked as a process to exercise the verb.
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
            "vole-share-account-{tag}-{}-{nanos}",
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

fn run(args: &[String]) -> Output {
    Command::new(CLI).args(args).output().expect("spawn CLI")
}

/// Extract a top-level unsigned integer field from the single-line JSON output.
fn field_u64(json: &str, key: &str) -> u64 {
    let pat = format!("\"{key}\":");
    let start = json.find(&pat).expect("field present") + pat.len();
    let rest = &json[start..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end].parse().expect("field is an integer")
}

fn encode(src: &Path, out: &Path) -> Output {
    run(&[
        "encode".to_string(),
        src.display().to_string(),
        out.display().to_string(),
    ])
}

#[test]
fn share_account_stores_units_and_is_deterministic() {
    let dir = TempDir::new("main");
    let src = dir.join("doc.pdf");
    fs::write(&src, sample("flate.pdf")).unwrap();
    let a = dir.join("a.voldoc");
    let b = dir.join("b.voldoc");
    assert!(encode(&src, &a).status.success(), "encode a");
    assert!(encode(&src, &b).status.success(), "encode b");
    let store = dir.join("store").display().to_string();

    // One descriptor: the store must hold exactly the report's unique units.
    let one = run(&[
        "share-account".to_string(),
        "--store".to_string(),
        store.clone(),
        a.display().to_string(),
    ]);
    assert!(
        one.status.success(),
        "share-account failed: {:?}",
        one.status
    );
    let one_json = String::from_utf8(one.stdout).unwrap();
    assert!(one_json.contains("\"ok\":true"), "{one_json}");
    assert!(one_json.contains("\"by_kind\":["), "{one_json}");
    let one_unique = field_u64(&one_json, "unique_bytes");
    let one_total = field_u64(&one_json, "total_bytes");
    assert!(one_unique > 0, "{one_json}");
    assert!(one_total >= one_unique, "{one_json}");
    assert_eq!(
        field_u64(&one_json, "store_bytes"),
        one_unique,
        "the store must hold exactly the unique units"
    );

    // Two byte-identical descriptors: every unit is shared, so `unique_bytes` is
    // unchanged and `total_bytes` doubles.
    let args = [
        "share-account".to_string(),
        "--store".to_string(),
        store,
        a.display().to_string(),
        b.display().to_string(),
    ];
    let two = run(&args);
    assert!(
        two.status.success(),
        "share-account failed: {:?}",
        two.status
    );
    let two_json = String::from_utf8(two.stdout).unwrap();
    assert_eq!(field_u64(&two_json, "unique_bytes"), one_unique);
    assert_eq!(field_u64(&two_json, "total_bytes"), 2 * one_total);
    assert_eq!(field_u64(&two_json, "store_bytes"), one_unique);

    // Deterministic: a fresh run prints the identical line.
    let again = run(&args);
    assert_eq!(String::from_utf8(again.stdout).unwrap(), two_json);
}

#[test]
fn share_account_usage_errors_fail_closed() {
    // No `--store`.
    let missing_store = run(&["share-account".to_string(), "in.voldoc".to_string()]);
    assert!(!missing_store.status.success());

    // `--store` but no input.
    let missing_inputs = run(&[
        "share-account".to_string(),
        "--store".to_string(),
        "/nonexistent-store".to_string(),
    ]);
    assert!(!missing_inputs.status.success());
}
