#![cfg(feature = "field")]
//! Phase 20.1 — the crash / power-cut fault-injection court.
//!
//! This test is the *engine* of `tools/phase20-crash-court.sh`. It attacks the
//! packed seed store's durability design (ADR-0053, `sync_policy`) empirically
//! by injecting faults, reopening the store, and asserting the recovery
//! invariants. It is `#[ignore]`d so the ordinary `cargo test` gate never runs
//! the (minutes-long, child-process-spawning, disk-mutating) matrix; the court
//! runs it with `--ignored` and a raw-table destination in `PHASE20_CRASH_RAW`.
//!
//! Three injection families, each run under `SyncPolicy::Batch` and
//! `SyncPolicy::Each`:
//!
//! * **A process death** — spawn `field-build --packed` and kill it with
//!   `SIGKILL` / `SIGABRT` across a sweep of the write timeline, plus kills
//!   polled to land right after the manifest / index appears.
//! * **B storage corruption** — truncate, bit-flip, and zero the `.pack` /
//!   `.idx`, and delete the `.idx` (models a crash before seal).
//! * **C deterministic in-code abort** — behind the non-default `fault-inject`
//!   feature, abort at named writer boundaries (`VOLE_FAULT_POINT`). Compiled
//!   out of the default build entirely; skipped if the feature is absent.
//!
//! Every case reopens the store and asserts: with no published manifest, the
//! store reopens and every present node re-hashes to its id (never a partial
//! node, torn tail discarded, recovered set exactly a prefix); with a published
//! manifest, the root node exists and `materialize --exact` matches the source.
//! Corruption cases must **fail closed** (typed error) if they cannot be served
//! exactly — never wrong bytes.
//!
//! ## Honest scope (stated here so it cannot be buried)
//!
//! `SIGKILL`/`SIGABRT` do **not** model true power loss: the page cache
//! survives process death, so every byte a `write` syscall completed is still
//! readable after reopen. This court therefore proves the *ordering* and
//! *prefix-recovery* argument (no manifest before its nodes, no partial node,
//! exactly-a-prefix recovery) and the storage-corruption fail-closed path. It
//! does **not** prove that an un-`fsync`ed record is lost on power loss, nor
//! that the `write_atomic` rename is directory-durable across a power cut (the
//! parent directory is never `fsync`ed). Those are argued, not measured.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use vole_document::Limits;
use vole_document::field::{Field, FieldId, FieldStore};
use vole_document::store::{IoCounters, NodeId, PackedSeedStore, SyncPolicy};

// ---------------------------------------------------------------------------
// small utilities
// ---------------------------------------------------------------------------

fn eprint_case(msg: &str) {
    eprintln!("[phase20-crash] {msg}");
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_vole-document")
}

fn policy_str(p: SyncPolicy) -> &'static str {
    match p {
        SyncPolicy::Batch => "batch",
        SyncPolicy::Each => "each",
    }
}

fn err_str(e: &vole_document::Error) -> String {
    format!("{:?}:{}", e.class(), e)
}

/// Recursively copy a directory tree (small store fixtures only).
fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

/// Recursively remove a directory, ignoring errors (scratch cleanup).
fn rm_rf(p: &Path) {
    let _ = fs::remove_dir_all(p);
}

/// Format a duration as integer milliseconds (rounded down).
#[allow(dead_code)]
fn ms(d: Duration) -> u128 {
    d.as_millis()
}

// ---------------------------------------------------------------------------
// store introspection (independent of the library under test)
// ---------------------------------------------------------------------------

/// The open `.pack` (a `.pack` with no sibling `.idx`), if any.
fn open_pack_path(root: &Path) -> Option<PathBuf> {
    let dir = root.join("fieldpack");
    let mut found = None;
    for entry in fs::read_dir(&dir).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(idpart) = name
            .strip_prefix("seg-")
            .and_then(|r| r.strip_suffix(".pack"))
        else {
            continue;
        };
        if !dir.join(format!("seg-{idpart}.idx")).exists() {
            found = Some(entry.path());
        }
    }
    found
}

fn idx_path_for(_root: &Path, pack: &Path) -> PathBuf {
    pack.with_extension("idx")
}

fn sealed_pack_paths(root: &Path) -> Vec<PathBuf> {
    let dir = root.join("fieldpack");
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(&dir) {
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(idpart) = name
                .strip_prefix("seg-")
                .and_then(|r| r.strip_suffix(".pack"))
                && dir.join(format!("seg-{idpart}.idx")).exists()
            {
                out.push(entry.path());
            }
        }
    }
    out.sort();
    out
}

/// The recovered framing scan: `(id, body_offset, len)` for every complete
/// record plus the recovered prefix length.
type PackScan = (Vec<(NodeId, u64, u32)>, u64);

/// An independent re-implementation of the framing scan: returns every complete
/// `(id, body_offset, len)` and the recovered prefix length. Used to check that
/// the library's recovered set is *exactly a prefix* of the appended records.
fn scan_pack_prefix(path: &Path) -> std::io::Result<PackScan> {
    let bytes = fs::read(path)?;
    let mut out = Vec::new();
    if bytes.len() < 24 {
        return Ok((out, 0));
    }
    let mut p = 24u64;
    while p + 4 <= bytes.len() as u64 {
        let len = u64::from(u32::from_le_bytes(
            bytes[p as usize..p as usize + 4].try_into().unwrap(),
        ));
        if len == 0 || len > u32::MAX as u64 {
            break;
        }
        if p + 4 + len > bytes.len() as u64 {
            break;
        }
        let body = &bytes[p as usize + 4..(p + 4 + len) as usize];
        out.push((NodeId::of_node(body), p + 4, len as u32));
        p += 4 + len;
    }
    Ok((out, p))
}

/// The published field manifest ids (`field/<64-hex>`), ignoring atomic-write
/// temp files.
fn list_manifests(root: &Path) -> Vec<FieldId> {
    let dir = root.join("field");
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(&dir) {
        for entry in rd.flatten() {
            if let Some(name) = entry.file_name().to_str()
                && let Ok(id) = FieldId::from_hex(name)
            {
                out.push(id);
            }
        }
    }
    out.sort_by_key(|i| i.to_hex());
    out
}

// ---------------------------------------------------------------------------
// reopen checks
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Chk {
    opened: bool,
    open_err: Option<String>,
    enum_err: Option<String>,
    n: usize,
    /// Enumerated ids whose fetched bytes did NOT re-hash to the id. Must be 0.
    bad_hash: usize,
    /// Enumerated ids whose fetch returned an error (an inconsistency, not wrong bytes).
    fetch_fail: usize,
    /// Recovered-id set equals the independent framing scan of the open segment.
    prefix_ok: bool,
    ids: Vec<NodeId>,
}

impl Chk {
    fn state(&self) -> &'static str {
        if self.opened {
            if self.enum_err.is_some() {
                "fail_closed"
            } else {
                "reopens_ok"
            }
        } else {
            "fail_closed"
        }
    }
    fn err(&self) -> String {
        self.open_err
            .clone()
            .or_else(|| self.enum_err.clone())
            .unwrap_or_else(|| "-".into())
    }
}

fn finish_chk(
    mut c: Chk,
    entries: Vec<(NodeId, u64)>,
    fetch: impl Fn(&NodeId) -> vole_document::Result<Vec<u8>>,
    indep: Option<std::io::Result<PackScan>>,
) -> Chk {
    let mut ids = Vec::with_capacity(entries.len());
    for (id, _len) in &entries {
        match fetch(id) {
            Ok(bytes) => {
                if NodeId::of_node(&bytes) != *id {
                    c.bad_hash += 1;
                }
                ids.push(*id);
            }
            Err(_) => {
                c.fetch_fail += 1;
                ids.push(*id);
            }
        }
    }
    c.n = entries.len();
    c.ids = ids;
    c.prefix_ok = match indep {
        // Only the open segment is independently scanned; compare the open
        // portion of the recovered set against it.
        Some(Ok((expected, _))) => {
            let exp: BTreeSet<[u8; 32]> = expected.iter().map(|(i, _, _)| *i.as_bytes()).collect();
            let got: BTreeSet<[u8; 32]> = entries.iter().map(|(i, _)| *i.as_bytes()).collect();
            // The recovered set may also contain sealed-segment ids; require the
            // open-scan set to be a subset and every scanned id to be present.
            exp.is_subset(&got) && exp.len() == got.intersection(&exp).count()
        }
        _ => true,
    };
    c
}

fn check_readonly(root: &Path) -> Chk {
    let indep = open_pack_path(root).map(|p| scan_pack_prefix(&p));
    match PackedSeedStore::open_read(root, IoCounters::new()) {
        Err(e) => Chk {
            opened: false,
            open_err: Some(err_str(&e)),
            ..Chk::default()
        },
        Ok(store) => match store.entries() {
            Err(e) => Chk {
                opened: true,
                enum_err: Some(err_str(&e)),
                ..Chk::default()
            },
            Ok(entries) => finish_chk(
                Chk {
                    opened: true,
                    ..Chk::default()
                },
                entries,
                |id| store.fetch(id),
                indep,
            ),
        },
    }
}

fn check_readwrite(root: &Path, policy: SyncPolicy) -> Chk {
    let indep = open_pack_path(root).map(|p| scan_pack_prefix(&p));
    match PackedSeedStore::open_write_with_policy(root, IoCounters::new(), policy) {
        Err(e) => Chk {
            opened: false,
            open_err: Some(err_str(&e)),
            ..Chk::default()
        },
        Ok(store) => match store.entries() {
            Err(e) => Chk {
                opened: true,
                enum_err: Some(err_str(&e)),
                ..Chk::default()
            },
            Ok(entries) => finish_chk(
                Chk {
                    opened: true,
                    ..Chk::default()
                },
                entries,
                |id| store.fetch(id),
                indep,
            ),
        },
    }
}

// ---------------------------------------------------------------------------
// exactness + observation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Exact {
    Match,
    Mismatch(String),
    FailClosed(String),
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Obs {
    Match,
    /// Recovered successfully but with content differing from the clean store.
    Wrong(String),
    /// Declined/behaved differently but returned no wrong bytes.
    Differ(String),
    FailClosed(String),
    NoBaseline,
}

fn run(args: &[&str]) -> Output {
    Command::new(bin())
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("spawn vole-document")
}

/// Project an `observe` answer line down to its deterministic content: drop the
/// leading `"field":"...",` (it names the *deepened* field handle and varies
/// with cache state) and the trailing `"stats":{...}` (I/O accounting and
/// `wall_micros` vary run to run). What remains — selector, representation,
/// provenance, basis, exactness, span, dependency ids, and the value — is the
/// answer the field supports.
fn answer_projection(s: &str) -> String {
    let mut out = s.to_string();
    if let Some(i) = out.find("\"field\":\"")
        && let Some(j) = out[i..].find("\",")
    {
        out.replace_range(i..i + j + 2, "");
    }
    if let Some(i) = out.find("\"stats\":") {
        let mut p = out[..i].trim_end().trim_end_matches(',').to_string();
        p.push_str("}\n");
        out = p;
    }
    out
}

/// Run two observation selectors (page-1 text + metadata) against `field` on a
/// **cold** store (the disposable `cache/` is removed first), returning the
/// projected answer bundle or the failing command's error.
fn observe_bundle(store: &Path, field: &str) -> std::result::Result<String, String> {
    // The derived cache is disposable and never on the exactness path; clearing
    // it forces the observation through the seed store the court is attacking.
    rm_rf(&store.join("cache"));
    let a = run(&[
        "observe",
        "--store",
        store.to_str().unwrap(),
        "--field",
        field,
        "--packed",
        "--page",
        "1",
        "--kind",
        "text",
    ]);
    let b = run(&[
        "observe",
        "--store",
        store.to_str().unwrap(),
        "--field",
        field,
        "--packed",
        "--metadata",
        "--kind",
        "metadata",
    ]);
    if a.status.success() && b.status.success() {
        Ok(format!(
            "{}\n{}\n",
            answer_projection(&String::from_utf8_lossy(&a.stdout)),
            answer_projection(&String::from_utf8_lossy(&b.stdout))
        ))
    } else {
        let err = if !a.status.success() {
            String::from_utf8_lossy(&a.stderr).trim().to_string()
        } else {
            String::from_utf8_lossy(&b.stderr).trim().to_string()
        };
        Err(err)
    }
}

fn exact_for(root: &Path, policy: SyncPolicy, source: &[u8]) -> (Exact, Vec<NodeId>) {
    let manifests = list_manifests(root);
    if manifests.is_empty() {
        return (Exact::NotApplicable, Vec::new());
    }
    let store = match FieldStore::open_packed_with_policy(root, policy) {
        Ok(s) => s,
        Err(e) => return (Exact::FailClosed(err_str(&e)), Vec::new()),
    };
    let mut roots = Vec::new();
    for id in &manifests {
        match Field::open(&store, id, Limits::DEFAULT) {
            Ok(field) => {
                roots.push(field.manifest().root_node);
                match field.materialize_exact(Limits::DEFAULT) {
                    Ok(bytes) => {
                        if bytes != source {
                            return (
                                Exact::Mismatch(format!(
                                    "materialize returned {} bytes != source {}",
                                    bytes.len(),
                                    source.len()
                                )),
                                roots,
                            );
                        }
                    }
                    Err(e) => return (Exact::FailClosed(err_str(&e)), roots),
                }
            }
            Err(e) => return (Exact::FailClosed(err_str(&e)), roots),
        }
    }
    (Exact::Match, roots)
}

/// The typed error class of an `observe` failure (`error: Class: ...`), so two
/// failures can be compared without comparing store-specific paths.
fn err_class(stderr: &str) -> String {
    let s = stderr.trim();
    let s = s.strip_prefix("error: ").unwrap_or(s);
    let class = s.split(':').next().unwrap_or(s).trim();
    if class.is_empty() || class.contains(' ') {
        "(unparsed)".to_string()
    } else {
        class.to_string()
    }
}

fn observe_for(root: &Path, baseline: &[(String, Result<String, String>)]) -> Obs {
    let present: BTreeSet<String> = list_manifests(root).iter().map(FieldId::to_hex).collect();
    // Compare only manifests the recovered store actually published; a clean
    // store's full field may legitimately be absent if the crash came first.
    let mut any = false;
    for (field_hex, base) in baseline {
        if !present.contains(field_hex) {
            continue;
        }
        any = true;
        match (base, observe_bundle(root, field_hex)) {
            (Ok(b), Ok(got)) if &got == b => {}
            (Ok(_), Ok(_)) => {
                return Obs::Wrong(format!("field {field_hex} observation content differs"));
            }
            (Ok(_), Err(e)) => {
                return Obs::FailClosed(format!("field {field_hex}: {e}"));
            }
            (Err(class), Err(e)) if err_class(&e) == *class => {}
            (Err(class), Err(e)) => {
                return Obs::Differ(format!(
                    "field {field_hex}: expected decline {class}, got {}",
                    err_class(&e)
                ));
            }
            (Err(class), Ok(_)) => {
                return Obs::Differ(format!(
                    "field {field_hex}: clean store declined ({class}) but recovered store answered"
                ));
            }
        }
    }
    if any { Obs::Match } else { Obs::NoBaseline }
}

// ---------------------------------------------------------------------------
// rows
// ---------------------------------------------------------------------------

const HEADER: &[&str] = &[
    "case",
    "family",
    "doc",
    "policy",
    "inject",
    "detail",
    "manifest",
    "idx",
    "ro",
    "ro_err",
    "rw",
    "rw_err",
    "entries",
    "bad_hash",
    "fetch_fail",
    "prefix",
    "exact",
    "observe",
    "verdict",
    "note",
];

struct Court {
    rows: Vec<String>,
    file: Option<std::fs::File>,
    failures: usize,
    criticals: usize,
}

impl Court {
    fn new(dest: Option<PathBuf>) -> Self {
        let file = dest.map(|p| {
            if let Some(parent) = p.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let mut f = fs::File::create(&p).expect("create raw table");
            writeln!(f, "{}", HEADER.join("\t")).unwrap();
            f
        });
        Court {
            rows: Vec::new(),
            file,
            failures: 0,
            criticals: 0,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn record(
        &mut self,
        case: &str,
        family: &str,
        doc: &str,
        policy: SyncPolicy,
        inject: &str,
        detail: &str,
        manifest: bool,
        idx: bool,
        ro: &Chk,
        rw: &Chk,
        exact: &Exact,
        obs: &Obs,
        verdict: &str,
        note: &str,
    ) {
        if verdict == "CRITICAL" {
            eprintln!("!!! CRITICAL: {case} {detail}: {exact:?} / {obs:?}");
        }
        if verdict == "FAIL" {
            self.failures += 1;
        }
        if verdict == "CRITICAL" {
            self.criticals += 1;
        }
        let row = [
            case.to_string(),
            family.to_string(),
            doc.to_string(),
            policy_str(policy).to_string(),
            inject.to_string(),
            detail.to_string(),
            ynu(manifest),
            ynu(idx),
            format!("{}:{}", ro.state(), ro.n),
            ro.err(),
            format!("{}:{}", rw.state(), rw.n),
            rw.err(),
            rw.n.to_string(),
            rw.bad_hash.to_string(),
            rw.fetch_fail.to_string(),
            if !ro.opened || ro.enum_err.is_some() {
                "n/a".to_string()
            } else if ro.prefix_ok {
                "ok".into()
            } else {
                "VIOLATED".to_string()
            },
            format!("{exact:?}"),
            format!("{obs:?}"),
            verdict.to_string(),
            note.to_string(),
        ];
        let line = row.join("\t");
        if let Some(f) = self.file.as_mut() {
            writeln!(f, "{line}").unwrap();
            f.flush().unwrap();
        }
        self.rows.push(line);
    }
}

fn ynu(b: bool) -> String {
    if b { "y".into() } else { "n".into() }
}

/// Shared verdict logic.
#[allow(clippy::too_many_arguments)]
fn verdict(
    family: &str,
    manifest: bool,
    ro: &Chk,
    rw: &Chk,
    exact: &Exact,
    obs: &Obs,
) -> (&'static str, String) {
    // Never acceptable, under any family: wrong bytes.
    if rw.bad_hash > 0 {
        return (
            "CRITICAL",
            format!("fetch returned {} wrong-byte node(s)", rw.bad_hash),
        );
    }
    // The prefix property is only checkable when the read-only enumeration
    // succeeded; a store that fails closed is not a prefix violation.
    if ro.opened && ro.enum_err.is_none() && !ro.prefix_ok {
        return (
            "CRITICAL",
            "recovered set is not a prefix of the open segment".into(),
        );
    }
    if matches!(exact, Exact::Mismatch(_)) {
        return (
            "CRITICAL",
            "materialize --exact returned mismatching bytes".into(),
        );
    }
    if matches!(obs, Obs::Wrong(_)) {
        return (
            "CRITICAL",
            "observation returned content differing from a clean store".into(),
        );
    }
    if family == "A" {
        // A SIGKILL cannot corrupt committed bytes: the store must reopen, and a
        // published manifest must materialize exactly.
        if manifest {
            return match (rw.opened, exact, obs) {
                (true, Exact::Match, Obs::Match) => (
                    "PASS",
                    "manifest present: exact; observation matches".into(),
                ),
                (true, Exact::Match, Obs::NoBaseline) => (
                    "PASS",
                    "manifest present: exact; no clean baseline to compare".into(),
                ),
                (true, Exact::Match, Obs::FailClosed(e)) => (
                    "FAIL",
                    format!("manifest present but observation did not answer: {e}"),
                ),
                (true, Exact::Match, Obs::Differ(e)) => (
                    "FAIL",
                    format!("manifest present but observation behaved differently: {e}"),
                ),
                (true, Exact::FailClosed(e), _) => (
                    "FAIL",
                    format!("manifest present but exact failed closed: {e}"),
                ),
                (true, Exact::NotApplicable, _) => (
                    "FAIL",
                    "manifest present but exactness not attempted".into(),
                ),
                (true, Exact::Mismatch(_), _) => unreachable!(),
                (true, Exact::Match, Obs::Wrong(_)) => unreachable!(),
                (false, _, _) => (
                    "FAIL",
                    format!("manifest present but reopen failed: {}", rw.err()),
                ),
            };
        }
        if !ro.opened {
            // Permitted by the task contract: a store left unusable with **no**
            // published manifest is acceptable if it is detected as damaged and
            // fails **closed** with a typed error (never wrong bytes). The only
            // family-A way to reach this is a kill inside the segment-header
            // write window, which leaves an open `.pack` shorter than a header.
            return (
                "PASS",
                format!(
                    "unusable but no manifest: detected, fails closed (typed): {}",
                    ro.err()
                ),
            );
        }
        if ro.enum_err.is_some() {
            return (
                "PASS",
                format!(
                    "no manifest: enumeration fails closed (typed): {}",
                    ro.err()
                ),
            );
        }
        if !rw.opened || rw.enum_err.is_some() {
            return (
                "PASS",
                format!(
                    "no manifest: read-write reopen fails closed (typed): {}",
                    rw.err()
                ),
            );
        }
        return (
            "PASS",
            "no manifest: reopens_ok, prefix, every id re-hashes".into(),
        );
    }
    // Family B (storage corruption) and C (deterministic abort): exactness may
    // legitimately fail closed, but it must never return wrong bytes.
    if !rw.opened || rw.enum_err.is_some() {
        return ("PASS", "fails closed: typed error, no bytes served".into());
    }
    if manifest {
        return match exact {
            Exact::Match => ("PASS", "manifest present: exact after recovery".into()),
            Exact::FailClosed(_) => ("PASS", "manifest present: fail closed (typed)".into()),
            Exact::NotApplicable => ("PASS", "reopens_ok; no manifest".into()),
            Exact::Mismatch(_) => unreachable!(),
        };
    }
    (
        "PASS",
        "reopens_ok; no manifest; every present id re-hashes".into(),
    )
}

// ---------------------------------------------------------------------------
// document discovery
// ---------------------------------------------------------------------------

fn find_doc(id: &str) -> Option<PathBuf> {
    fn walk(dir: &Path, id: &str, out: &mut Option<PathBuf>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, id, out);
            } else if p.file_stem().and_then(|s| s.to_str()) == Some(id) {
                *out = Some(p);
            }
        }
    }
    let mut out = None;
    walk(Path::new("real100-v1/documents"), id, &mut out);
    out
}

// ---------------------------------------------------------------------------
// family A: process death
// ---------------------------------------------------------------------------

enum Signal {
    Kill,
    Abrt,
}

fn signal_name(s: &Signal) -> &'static str {
    match s {
        Signal::Kill => "SIGKILL",
        Signal::Abrt => "SIGABRT",
    }
}

fn send(sig: &Signal, child: &mut std::process::Child) {
    match sig {
        Signal::Kill => {
            let _ = child.kill();
        }
        Signal::Abrt => {
            let _ = Command::new("kill")
                .arg("-ABRT")
                .arg(child.id().to_string())
                .status();
        }
    }
}

fn field_build_args(doc: &Path, store: &Path, policy: SyncPolicy) -> Vec<String> {
    vec![
        "field-build".into(),
        doc.to_str().unwrap().into(),
        "--store".into(),
        store.to_str().unwrap().into(),
        "--profile".into(),
        "runtime".into(),
        "--packed".into(),
        format!("--sync={}", policy_str(policy)),
    ]
}

/// Spawn a builder and kill it after `delay`.
fn kill_at(doc: &Path, store: &Path, policy: SyncPolicy, sig: &Signal, delay: Duration) {
    let mut child = Command::new(bin())
        .args(field_build_args(doc, store, policy))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn builder");
    std::thread::sleep(delay);
    send(sig, &mut child);
    let _ = child.wait();
}

#[derive(PartialEq, Eq)]
enum When {
    AfterManifest,
    AfterIdx,
}

/// Spawn a builder and kill it as soon as `when` becomes true (or at `timeout`).
fn kill_when(
    doc: &Path,
    store: &Path,
    policy: SyncPolicy,
    sig: &Signal,
    when: &When,
    timeout: Duration,
) {
    let mut child = Command::new(bin())
        .args(field_build_args(doc, store, policy))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn builder");
    let start = Instant::now();
    loop {
        let hit = match when {
            When::AfterManifest => !list_manifests(store).is_empty(),
            When::AfterIdx => !sealed_pack_paths(store).is_empty(),
        };
        if hit || start.elapsed() > timeout {
            break;
        }
        std::thread::sleep(Duration::from_micros(100));
    }
    send(sig, &mut child);
    let _ = child.wait();
}

// ---------------------------------------------------------------------------
// family B: storage corruption
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum Mut {
    TruncatePack(u64),
    TruncateIdx(u64),
    Flip(u64),
    IndexFlip(u64),
    ZeroTail(u64),
    DropIdx,
    DropIdxTruncate(u64),
}

impl Mut {
    fn detail(&self) -> String {
        match self {
            Mut::TruncatePack(o) => format!("truncate .pack to {o}"),
            Mut::TruncateIdx(o) => format!("truncate .idx to {o}"),
            Mut::Flip(o) => format!("flip bit in .pack at {o}"),
            Mut::IndexFlip(o) => format!("flip bit in .idx at {o}"),
            Mut::ZeroTail(n) => format!("zero last {n} B of .pack"),
            Mut::DropIdx => "delete .idx (crash before seal)".into(),
            Mut::DropIdxTruncate(o) => format!("delete .idx then truncate .pack to {o}"),
        }
    }
    fn name(&self) -> &'static str {
        match self {
            Mut::TruncatePack(_) => "truncate_pack",
            Mut::TruncateIdx(_) => "truncate_idx",
            Mut::Flip(_) => "flip_pack",
            Mut::IndexFlip(_) => "flip_idx",
            Mut::ZeroTail(_) => "zero_pack_tail",
            Mut::DropIdx => "drop_idx",
            Mut::DropIdxTruncate(_) => "drop_idx_truncate_pack",
        }
    }
}

fn flip_byte(path: &Path, off: u64, mask: u8) {
    let mut bytes = fs::read(path).unwrap();
    let i = off as usize;
    if i < bytes.len() {
        bytes[i] ^= mask;
        fs::write(path, &bytes).unwrap();
    }
}

fn truncate_to(path: &Path, len: u64) {
    let f = fs::OpenOptions::new().write(true).open(path).unwrap();
    f.set_len(len).unwrap();
}

fn zero_tail(path: &Path, n: u64) {
    let mut bytes = fs::read(path).unwrap();
    let len = bytes.len();
    let from = len.saturating_sub(n as usize);
    for b in &mut bytes[from..] {
        *b = 0;
    }
    fs::write(path, &bytes).unwrap();
}

fn apply_mut(root: &Path, pack: &Path, idx: &Path, m: &Mut) {
    match m {
        Mut::TruncatePack(o) => truncate_to(pack, *o),
        Mut::TruncateIdx(o) => truncate_to(idx, *o),
        Mut::Flip(o) => flip_byte(pack, *o, 0x01),
        Mut::IndexFlip(o) => flip_byte(idx, *o, 0x01),
        Mut::ZeroTail(n) => zero_tail(pack, *n),
        Mut::DropIdx => {
            fs::remove_file(idx).unwrap();
        }
        Mut::DropIdxTruncate(o) => {
            fs::remove_file(idx).unwrap();
            truncate_to(pack, *o);
        }
    }
    let _ = root;
}

/// A representative, deterministic set of byte offsets to truncate/flip at.
fn corruption_offsets(pack_len: u64, first_body_len: u64, idx_len: u64) -> Vec<u64> {
    let rec0_body = 24 + 4;
    let rec0_end = rec0_body + first_body_len;
    let mut v = vec![
        0,
        1,
        10,
        23, // inside the pack header
        24, // at the header/record boundary
        25,
        26,
        27, // inside the length prefix
        rec0_body - 1,
        rec0_body, // at the first body start
        rec0_body + 1,
        rec0_body + first_body_len / 2, // mid first body
        rec0_end - 1,
        rec0_end, // exactly at the first record boundary
        rec0_end + 1,
    ];
    v.push(pack_len.saturating_sub(1));
    v.push(pack_len / 2); // mid-segment
    v.retain(|o| *o <= pack_len);
    v.sort_unstable();
    v.dedup();
    let _ = idx_len;
    v
}

fn idx_offsets(idx_len: u64) -> Vec<u64> {
    let mut v = vec![0, 1, 10, 31, 32, 33, 40, 48, 56];
    v.push(idx_len.saturating_sub(1));
    v.push(idx_len / 2);
    v.retain(|o| *o <= idx_len);
    v.sort_unstable();
    v.dedup();
    v
}

// ---------------------------------------------------------------------------
// the court
// ---------------------------------------------------------------------------

fn scratch() -> PathBuf {
    PathBuf::from(std::env::var("PHASE20_SCRATCH").unwrap_or_else(|_| "/tmp/phase20-crash".into()))
}

fn build_clean(doc: &Path, store: &Path, policy: SyncPolicy) -> Duration {
    let t = Instant::now();
    let out = Command::new(bin())
        .args(field_build_args(doc, store, policy))
        .stdin(Stdio::null())
        .output()
        .expect("spawn clean build");
    let d = t.elapsed();
    assert!(
        out.status.success(),
        "clean build failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    d
}

/// A clean store answers both; a recovered store must answer identically.
fn baseline_observations(root: &Path) -> Vec<(String, Result<String, String>)> {
    // A cold baseline: the derived cache is never on the exactness path, so a
    // recovered store must reproduce the same answer without it.
    rm_rf(&root.join("cache"));
    list_manifests(root)
        .iter()
        .map(|id| {
            let hex = id.to_hex();
            let base = observe_bundle(root, &hex).map_err(|e| err_class(&e));
            (hex, base)
        })
        .collect()
}

/// Verify one already-corrupted store and record a row.
#[allow(clippy::too_many_arguments)]
fn verify_case(
    court: &mut Court,
    family: &str,
    doc: &str,
    policy: SyncPolicy,
    case: &str,
    inject: &str,
    detail: &str,
    root: &Path,
    source: &[u8],
    baseline: &[(String, Result<String, String>)],
    note_extra: &str,
) {
    let manifest = !list_manifests(root).is_empty();
    let idx = !sealed_pack_paths(root).is_empty();
    let ro = check_readonly(root);
    let rw = check_readwrite(root, policy);
    let (exact, _roots) = exact_for(root, policy, source);
    // Observations only exist when a manifest was published; otherwise there is
    // no field to query (and no baseline to compare against).
    let obs = if manifest {
        observe_for(root, baseline)
    } else {
        Obs::NoBaseline
    };
    let (v, note) = verdict(family, manifest, &ro, &rw, &exact, &obs);
    let note = if note_extra.is_empty() {
        note
    } else {
        format!("{note}; {note_extra}")
    };
    eprint_case(&format!(
        "{case} {detail}: {v} ro={} rw={} {exact:?} {obs:?}",
        ro.state(),
        rw.state()
    ));
    court.record(
        case, family, doc, policy, inject, detail, manifest, idx, &ro, &rw, &exact, &obs, v, &note,
    );
}

fn run_family_a(
    court: &mut Court,
    doc_id: &str,
    doc: &Path,
    source: &[u8],
    baseline: &[(String, Result<String, String>)],
    policy: SyncPolicy,
    reps: usize,
) {
    let base = scratch().join(format!("a-{doc_id}-{}", policy_str(policy)));
    // Total runtime of a clean build defines the sweep.
    rm_rf(&base);
    let clean = base.join("clean");
    let t = build_clean(doc, &clean, policy);
    rm_rf(&clean);
    eprint_case(&format!("family A {doc_id} {policy:?}: clean build {t:?}"));
    let total_us: u64 = u64::try_from(t.as_micros()).unwrap_or(u64::MAX).max(1_000);
    let points: u64 = 56;
    let step = (total_us / points).max(1);

    for sig in [Signal::Kill, Signal::Abrt] {
        // Deterministic delay sweep across the whole write timeline.
        let mut d = 0u64;
        let mut idx = 0usize;
        while d <= total_us + step && idx < (points as usize + 8) {
            for rep in 0..reps {
                let store = base.join(format!("s-{}-{idx}-{rep}", signal_name(&sig)));
                rm_rf(&store);
                kill_at(doc, &store, policy, &sig, Duration::from_micros(d));
                let case = format!("a-{doc_id}-{}-d{d}", signal_name(&sig));
                let detail = format!("{} @ {}us", signal_name(&sig), d);
                verify_case(
                    court,
                    "A",
                    doc_id,
                    policy,
                    &case,
                    "sigkill/sigabrt-at-delay",
                    &detail,
                    &store,
                    source,
                    baseline,
                    "SIGKILL/SIGABRT stop the process; page cache survives",
                );
                rm_rf(&store);
            }
            idx += 1;
            d += step;
        }

        // Boundary-targeted kills: land right after the manifest / idx appears.
        for when in [When::AfterManifest, When::AfterIdx] {
            for rep in 0..6 {
                let store = base.join(format!("w-{}-{rep}", signal_name(&sig)));
                rm_rf(&store);
                kill_when(
                    doc,
                    &store,
                    policy,
                    &sig,
                    &when,
                    t + Duration::from_millis(500),
                );
                let w = match when {
                    When::AfterManifest => "after-manifest",
                    When::AfterIdx => "after-idx",
                };
                let case = format!("a-{doc_id}-{}-{w}", signal_name(&sig));
                let detail = format!("{} @ {w}", signal_name(&sig));
                verify_case(
                    court,
                    "A",
                    doc_id,
                    policy,
                    &case,
                    "sigkill/sigabrt-at-boundary",
                    &detail,
                    &store,
                    source,
                    baseline,
                    "polled to the manifest/index publication window",
                );
                rm_rf(&store);
            }
        }
    }
    rm_rf(&base);
}

fn run_family_b(court: &mut Court, doc_id: &str, doc: &Path, source: &[u8], policy: SyncPolicy) {
    let base = scratch().join(format!("b-{doc_id}-{}", policy_str(policy)));
    rm_rf(&base);
    let clean = base.join("clean");
    let _ = doc;
    let _ = build_clean(doc, &clean, policy);
    let baseline = baseline_observations(&clean);
    eprint_case(&format!(
        "family B {doc_id} {policy:?}: clean manifests = {}",
        baseline.len()
    ));

    // Topology of the clean store (all sealed after `store.sync()`).
    let sealed = sealed_pack_paths(&clean);
    assert!(!sealed.is_empty(), "clean store has no sealed segment");
    let pack = sealed[0].clone();
    let idx = idx_path_for(&clean, &pack);
    let pack_len = fs::metadata(&pack).unwrap().len();
    let idx_len = fs::metadata(&idx).unwrap().len();
    let (scanned, _valid) = scan_pack_prefix(&pack).unwrap();
    let first_body_len = scanned.first().map(|(_, _, l)| u64::from(*l)).unwrap_or(0);

    let mut muts: Vec<Mut> = Vec::new();
    for o in corruption_offsets(pack_len, first_body_len, idx_len) {
        muts.push(Mut::TruncatePack(o));
    }
    // Open-segment (idx removed) truncations exercise prefix recovery directly.
    for o in [
        0,
        24,
        40,
        24 + 4 + first_body_len / 2,
        24 + 4 + first_body_len,
        pack_len / 2,
        pack_len.saturating_sub(1),
    ] {
        if o <= pack_len {
            muts.push(Mut::DropIdxTruncate(o));
        }
    }
    for o in idx_offsets(idx_len) {
        muts.push(Mut::TruncateIdx(o));
    }
    muts.push(Mut::DropIdx);
    // Bit flips: framing prefix, body, header, and idx internals.
    for o in [
        0,
        24,
        24 + 1,
        24 + 4,
        24 + 4 + 1,
        24 + 4 + first_body_len / 2,
        pack_len / 2,
        pack_len.saturating_sub(1),
    ] {
        if o < pack_len {
            muts.push(Mut::Flip(o));
        }
    }
    for o in [
        0,
        8,
        9,
        16,
        20,
        24,
        32,
        40,
        47,
        idx_len / 2,
        idx_len.saturating_sub(1),
    ] {
        if o < idx_len {
            muts.push(Mut::IndexFlip(o));
        }
    }
    for n in [1u64, 4, 8, 32, 128] {
        if n < pack_len {
            muts.push(Mut::ZeroTail(n));
        }
    }

    for (i, m) in muts.iter().enumerate() {
        let inj = base.join(format!("inj-{i}"));
        rm_rf(&inj);
        copy_dir(&clean, &inj).unwrap();
        // Re-resolve the mutated paths inside the copy.
        let pack2 = inj.join(pack.strip_prefix(&clean).unwrap());
        let idx2 = idx_path_for(&inj, &pack2);
        apply_mut(&inj, &pack2, &idx2, m);
        let case = format!("b-{doc_id}-{}-{i}", m.name());
        verify_case(
            court,
            "B",
            doc_id,
            policy,
            &case,
            m.name(),
            &m.detail(),
            &inj,
            source,
            &baseline,
            "tamper: must fail closed if not exactly serviceable",
        );
        rm_rf(&inj);
    }
    rm_rf(&base);
}

fn run_family_c(family_a_docs: &[(String, PathBuf, Vec<u8>)], court: &mut Court) {
    #[cfg(not(feature = "fault-inject"))]
    {
        let _ = (family_a_docs, &mut *court);
        eprint_case("family C: feature `fault-inject` not enabled — skipped");
    }
    #[cfg(feature = "fault-inject")]
    {
        let points = [
            "record.before_prefix",
            "record.after_prefix",
            "record.after_body",
            "flush.before_sync",
            "flush.after_sync",
            "seal.before_idx",
            "seal.after_idx",
            "manifest.before_flush",
            "manifest.after_flush",
            "manifest.after_publish",
        ];
        for (doc_id, doc, source) in family_a_docs {
            let clean = scratch().join(format!("c-clean-{doc_id}"));
            rm_rf(&clean);
            build_clean(doc, &clean, SyncPolicy::Batch);
            let baseline = baseline_observations(&clean);
            rm_rf(&clean);
            for policy in [SyncPolicy::Batch, SyncPolicy::Each] {
                for p in points {
                    let store = scratch().join(format!("c-{doc_id}-{}-{p}", policy_str(policy)));
                    rm_rf(&store);
                    // The builder aborts itself at the named point; no signal needed.
                    let _ = Command::new(bin())
                        .args(field_build_args(doc, &store, policy))
                        .env("VOLE_FAULT_POINT", p)
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status();
                    let case = format!("c-{doc_id}-{}-{p}", policy_str(policy));
                    verify_case(
                        court,
                        "C",
                        doc_id,
                        policy,
                        &case,
                        "fault-inject-abort",
                        p,
                        &store,
                        source,
                        &baseline,
                        "deterministic abort at a named writer boundary",
                    );
                    rm_rf(&store);
                }
            }
        }
    }
}

#[test]
#[ignore = "Phase 20.1 crash court: minutes-long fault-injection matrix; run via tools/phase20-crash-court.sh"]
fn crash_fault_injection_court() {
    let Ok(dest) = std::env::var("PHASE20_CRASH_RAW") else {
        eprintln!(
            "[phase20-crash] PHASE20_CRASH_RAW unset — this court runs only through \
             tools/phase20-crash-court.sh (or set PHASE20_CRASH_RAW to a TSV path)"
        );
        return;
    };
    let dest = PathBuf::from(dest);
    let reps: usize = std::env::var("PHASE20_REPS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2);
    let doc_ids: Vec<String> = std::env::var("PHASE20_DOCS")
        .unwrap_or_else(|_| "nist-pdf-0017 nist-pdf-0002".into())
        .split_whitespace()
        .map(str::to_owned)
        .collect();

    rm_rf(&scratch());
    fs::create_dir_all(scratch()).unwrap();
    let mut court = Court::new(Some(dest));

    let mut docs = Vec::new();
    for id in &doc_ids {
        let doc = find_doc(id).unwrap_or_else(|| panic!("corpus document {id} not found"));
        let source = fs::read(&doc).unwrap();
        docs.push((id.clone(), doc, source));
    }

    for (id, doc, source) in &docs {
        let clean = scratch().join(format!("clean-{id}"));
        rm_rf(&clean);
        build_clean(doc, &clean, SyncPolicy::Batch);
        let baseline = baseline_observations(&clean);
        rm_rf(&clean);
        for policy in [SyncPolicy::Batch, SyncPolicy::Each] {
            run_family_a(&mut court, id, doc, source, &baseline, policy, reps);
            run_family_b(&mut court, id, doc, source, policy);
        }
    }
    run_family_c(&docs, &mut court);

    rm_rf(&scratch());
    eprintln!(
        "[phase20-crash] cases={} failures={} criticals={}",
        court.rows.len(),
        court.failures,
        court.criticals
    );
    assert_eq!(
        court.criticals, 0,
        "CRITICAL: {} case(s) returned wrong bytes (see table)",
        court.criticals
    );
    assert_eq!(
        court.failures, 0,
        "{} case(s) FAILED the recovery invariants (see table)",
        court.failures
    );
}
