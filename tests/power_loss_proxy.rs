#![cfg(all(feature = "field", feature = "power-log"))]
//! Phase 23 — the **model-based power-loss proxy** (GAP 2).
//!
//! Phase 20.1 injects process death (`SIGKILL`/`SIGABRT`) and deterministic
//! aborts, but the OS page cache survives those, so **no `fsync` boundary is
//! exercised** and `SyncPolicy::Batch` and `Each` were indistinguishable — which
//! is *not* evidence that batching is power-safe. This proxy makes the barrier
//! boundary explicit:
//!
//! 1. Every durability barrier in the field store's atomic writers is logged, in
//!    program order, by the `power-log`-gated [`vole_document::store::durable`]
//!    wrappers: `create`/`rename`/`dirsync`, and `fsync`/`fsync_data` **with the
//!    byte length the barrier covered** (the barrier covers `[0, len)`).
//! 2. From that log this test **reconstructs the post-power-loss state**: a file
//!    survives only if (a) its creation/rename was followed by a `dirsync` of its
//!    parent directory, and (b) its content is truncated to the length at its last
//!    completed file barrier (`fsync`/`fsync_data`), or to zero if it never had
//!    one. Everything written after the last barrier to that file is lost.
//! 3. The Phase-20.1 invariants are then checked **against the reconstructed
//!    state**: with no published manifest, the store reopens, no partial node is
//!    fetchable, and every present id re-hashes; with a published manifest, the
//!    root node exists and `materialize --exact` equals the source (length +
//!    SHA-256 + byte compare).
//!
//! This is a **model**, not a real power cut (see `SUMMARY.md` for what it does
//! and does not prove). It also runs *counterfactual* arms (`model-drop-dirsync`
//! and `dir_sync=off`) that remove directory barriers, to show the model is
//! sensitive to exactly the GAP-1 class of bug it is meant to catch.
//!
//! Run only through `tools/phase23-powerloss-court.sh`; `PHASE23_PROXY_RAW` names
//! the TSV destination.

use std::collections::HashMap;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use vole_document::Limits;
use vole_document::field::index::{FsIndexStore, validate};
use vole_document::field::{Field, FieldId, FieldStore};
use vole_document::store::{NodeId, SyncPolicy};

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

// ---------------------------------------------------------------------------
// small filesystem utilities
// ---------------------------------------------------------------------------

fn rm_rf(p: &Path) {
    let _ = fs::remove_dir_all(p);
}

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

fn walk_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(root) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk_files(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// Collect every directory strictly under `root` (not `root` itself).
fn walk_dirs(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(root) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.push(p.clone());
            walk_dirs(&p, out);
        }
    }
}

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
// the barrier-log model
// ---------------------------------------------------------------------------

/// The durability state of one path after a prefix of the barrier log.
#[derive(Debug, Default, Clone, Copy)]
struct Entry {
    /// A `dirsync` of the parent occurred after this file's create/rename.
    dirent: bool,
    /// Length covered by the last completed file barrier (the durable prefix).
    sync_len: Option<u64>,
}

fn parent_rel(rel: &str) -> &str {
    match rel.rfind('/') {
        Some(i) => &rel[..i],
        None => "",
    }
}

fn is_dropped(dir: &str, prefixes: &[&str]) -> bool {
    prefixes
        .iter()
        .any(|p| dir == *p || dir.starts_with(&format!("{p}/")))
}

/// Fold the barrier log into per-path durability. `drop_dirsync` removes
/// directory barriers whose path is in the given subtree (a counterfactual).
fn build_model(log: &str, drop_dirsync: &[&str]) -> HashMap<String, Entry> {
    let mut m: HashMap<String, Entry> = HashMap::new();
    for line in log.lines() {
        let mut it = line.split('\t');
        let op = it.next().unwrap_or("");
        match op {
            "create" => {
                let p = it.next().unwrap_or("");
                m.insert(p.to_string(), Entry::default());
            }
            "write" => {}
            "mkdir" => {
                let p = it.next().unwrap_or("");
                m.insert(p.to_string(), Entry::default());
            }
            "fsync" | "fsync_data" => {
                let p = it.next().unwrap_or("");
                let len: u64 = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                m.entry(p.to_string()).or_default().sync_len = Some(len);
            }
            "setlen" => {
                let p = it.next().unwrap_or("");
                let len: u64 = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                if let Some(e) = m.get_mut(p) {
                    e.sync_len = Some(e.sync_len.map_or(len, |v| v.min(len)));
                }
            }
            "dirsync" => {
                let dir = it.next().unwrap_or("");
                if is_dropped(dir, drop_dirsync) {
                    continue;
                }
                for (p, e) in m.iter_mut() {
                    if parent_rel(p) == dir {
                        e.dirent = true;
                    }
                }
            }
            "rename" => {
                let from = it.next().unwrap_or("");
                let to = it.next().unwrap_or("");
                let src = m.remove(from).unwrap_or_default();
                m.insert(
                    to.to_string(),
                    Entry {
                        dirent: false,
                        sync_len: src.sync_len,
                    },
                );
            }
            _ => {}
        }
    }
    m
}

#[derive(Debug, Default)]
struct ReconStats {
    removed: usize,
    removed_dirs: usize,
    truncated: usize,
}

/// Materialize the post-power-loss state into `dst` from the end/abort-state at
/// `src`, using the folded barrier model.
fn reconstruct(
    src: &Path,
    dst: &Path,
    model: &HashMap<String, Entry>,
) -> std::io::Result<ReconStats> {
    copy_dir(src, dst)?;
    let mut stats = ReconStats::default();

    // Phase 25: a directory whose *entry* was never made durable (its creation
    // was not followed by a parent `dirsync`) is lost, and everything beneath it
    // with it. Prune deepest-first so a child removal cannot resurrect a parent.
    let mut dirs = Vec::new();
    walk_dirs(dst, &mut dirs);
    dirs.sort();
    for d in dirs.iter().rev() {
        let rel = d.strip_prefix(dst).unwrap().to_string_lossy().into_owned();
        if matches!(model.get(&rel), Some(e) if !e.dirent) {
            fs::remove_dir_all(d)?;
            stats.removed_dirs += 1;
        }
    }

    let mut files = Vec::new();
    walk_files(dst, &mut files);
    for f in files {
        let rel = f.strip_prefix(dst).unwrap().to_string_lossy().into_owned();
        let Some(e) = model.get(&rel) else {
            continue; // not created by the logged run: treat as pre-existing/durable
        };
        if !e.dirent {
            fs::remove_file(&f)?;
            stats.removed += 1;
            continue;
        }
        let cur = fs::metadata(&f)?.len();
        let dlen = e.sync_len.unwrap_or(0);
        if dlen < cur {
            let file = fs::OpenOptions::new().write(true).open(&f)?;
            file.set_len(dlen)?;
            stats.truncated += 1;
        }
    }
    Ok(stats)
}

// ---------------------------------------------------------------------------
// reopen + invariant checks (Phase 20.1 semantics)
// ---------------------------------------------------------------------------

struct Chk {
    opened: bool,
    state: &'static str,
    nodes: usize,
    bad_hash: usize,
    err: String,
}

fn check_seeds(root: &Path, packed: bool, policy: SyncPolicy) -> Chk {
    let store = if packed {
        FieldStore::open_packed_with_policy(root, policy)
    } else {
        FieldStore::open(root)
    };
    let store = match store {
        Ok(s) => s,
        Err(e) => {
            return Chk {
                opened: false,
                state: "fail_closed",
                nodes: 0,
                bad_hash: 0,
                err: err_str(&e),
            };
        }
    };
    match store.seeds().list_nodes() {
        Err(e) => Chk {
            opened: true,
            state: "fail_closed",
            nodes: 0,
            bad_hash: 0,
            err: err_str(&e),
        },
        Ok(entries) => {
            let mut bad = 0usize;
            for (id, _len) in &entries {
                if let Ok(bytes) = store.seeds().get_node(id)
                    && vole_document::store::NodeId::of_node(&bytes) != *id
                {
                    bad += 1;
                }
            }
            Chk {
                opened: true,
                state: "reopens_ok",
                nodes: entries.len(),
                bad_hash: bad,
                err: "-".into(),
            }
        }
    }
}

/// Materialize every manifest's exact source and require the root node present.
/// `Ok(())` or a fail-closed description.
fn check_manifests(
    root: &Path,
    packed: bool,
    policy: SyncPolicy,
    source: &[u8],
) -> (String, usize) {
    let manifests = list_manifests(root);
    if manifests.is_empty() {
        return ("n/a".into(), 0);
    }
    let store = match if packed {
        FieldStore::open_packed_with_policy(root, policy)
    } else {
        FieldStore::open(root)
    } {
        Ok(s) => s,
        Err(e) => return (format!("OpenFailClosed[{}]", err_str(&e)), manifests.len()),
    };
    for id in &manifests {
        let field = match Field::open(&store, id, Limits::DEFAULT) {
            Ok(f) => f,
            Err(e) => return (format!("FailClosed[{}]", err_str(&e)), manifests.len()),
        };
        // Root and index-root dependency nodes must exist.
        if !matches!(
            store.seeds().contains_node(&field.manifest().root_node),
            Ok(true)
        ) {
            return (format!("MissingRoot[{}]", id.to_hex()), manifests.len());
        }
        if field.manifest().has_index() {
            let root = NodeId::from_bytes(field.manifest().index_root);
            match FsIndexStore::open(store.root()) {
                Ok(istore) => {
                    if let Err(e) = validate(&istore, &root) {
                        return (format!("MissingIndex[{}]", err_str(&e)), manifests.len());
                    }
                }
                Err(e) => return (format!("MissingIndex[{}]", err_str(&e)), manifests.len()),
            }
        }
        match field.materialize_exact(Limits::DEFAULT) {
            Ok(bytes) => {
                if bytes.len() != source.len() || bytes != source {
                    return (format!("Mismatch[{}]", id.to_hex()), manifests.len());
                }
            }
            Err(e) => return (format!("FailClosed[{}]", err_str(&e)), manifests.len()),
        }
    }
    ("Match".into(), manifests.len())
}

fn run(args: &[&str]) -> Output {
    Command::new(bin())
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("spawn vole-document")
}

/// Drop the leading `"field":"...",` and trailing `"stats":{...}` (they vary
/// with the handle/cache state and wall time) so two answers can be compared.
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

/// Run two cold observation selectors (page-1 text + metadata) against `field`,
/// projecting the answer; a typed decline is returned as its error class. The
/// disposable `cache/` is cleared so the observation goes through the seed store.
fn observe_bundle(store: &Path, packed: bool, field_hex: &str) -> Result<String, String> {
    rm_rf(&store.join("cache"));
    let mut base: Vec<&str> = vec!["observe", "--store", store.to_str().unwrap()];
    if packed {
        base.push("--packed");
    }
    let mut a = base.clone();
    a.extend_from_slice(&["--field", field_hex, "--page", "1", "--kind", "text"]);
    let mut b = base.clone();
    b.extend_from_slice(&["--field", field_hex, "--metadata", "--kind", "metadata"]);
    let oa = run(&a);
    let ob = run(&b);
    if oa.status.success() && ob.status.success() {
        Ok(format!(
            "{}\n{}\n",
            answer_projection(&String::from_utf8_lossy(&oa.stdout)),
            answer_projection(&String::from_utf8_lossy(&ob.stdout))
        ))
    } else {
        let e = if !oa.status.success() {
            String::from_utf8_lossy(&oa.stderr).into_owned()
        } else {
            String::from_utf8_lossy(&ob.stderr).into_owned()
        };
        Err(err_class(&e))
    }
}

/// Compare the recovered store's answers to a clean baseline for every manifest
/// it still publishes. A typed decline equal to the baseline's is a match.
fn observe_vs_baseline(
    root: &Path,
    packed: bool,
    baseline: &[(String, Result<String, String>)],
) -> String {
    let present: std::collections::BTreeSet<String> =
        list_manifests(root).iter().map(FieldId::to_hex).collect();
    let mut any = false;
    for (hex, base) in baseline {
        if !present.contains(hex) {
            continue;
        }
        any = true;
        match (base, observe_bundle(root, packed, hex)) {
            (Ok(b), Ok(g)) if &g == b => {}
            (Ok(_), Ok(_)) => return format!("Wrong[{hex}]"),
            (Ok(_), Err(e)) => return format!("FailClosed[{hex}:{e}]"),
            (Err(c), Err(e)) if c == &e => {}
            (Err(c), Err(e)) => return format!("Differ[{hex}:{c}!={e}]"),
            (Err(_), Ok(_)) => return format!("Differ[{hex}:declined-clean-answered]"),
        }
    }
    if any {
        "Match".into()
    } else {
        "NoBaseline".into()
    }
}

// ---------------------------------------------------------------------------
// case execution
// ---------------------------------------------------------------------------

fn field_build_args(
    doc: &Path,
    store: &Path,
    packed: bool,
    policy: SyncPolicy,
    dir_sync: &str,
) -> Vec<String> {
    let mut v = vec![
        "field-build".into(),
        doc.to_str().unwrap().into(),
        "--store".into(),
        store.to_str().unwrap().into(),
        "--profile".into(),
        "runtime".into(),
        format!("--sync={}", policy_str(policy)),
        format!("--dir-sync={dir_sync}"),
    ];
    if packed {
        v.push("--packed".into());
    }
    v
}

struct Row {
    case: String,
    backend: String,
    policy: String,
    dir_sync: String,
    arm: String,
    inject: String,
    manifests: usize,
    state: String,
    nodes: usize,
    bad_hash: usize,
    exact: String,
    observe: String,
    verdict: String,
    note: String,
}

fn strict_verdict(
    manifests: usize,
    state: &str,
    bad: usize,
    exact: &str,
    obs: &str,
) -> (&'static str, String) {
    if bad > 0 {
        return ("CRITICAL", format!("{bad} wrong-byte node(s) served"));
    }
    if exact.starts_with("Mismatch") {
        return ("CRITICAL", "materialize returned bytes != source".into());
    }
    if manifests == 0 {
        if state == "reopens_ok" {
            return (
                "PASS",
                "no manifest: reopens; every present id re-hashes".into(),
            );
        }
        // Phase 25: Phase 20's requirement is PREFIX RECOVERY — with no published
        // manifest the store must reopen, not merely fail closed.
        return (
            "CRITICAL",
            format!(
                "no published manifest but the store did not reopen (prefix recovery violated): {state}"
            ),
        );
    }
    if exact.starts_with("OpenFailClosed") || exact.starts_with("FailClosed") {
        return (
            "CRITICAL",
            format!("published manifest unserviceable after power loss: {exact}"),
        );
    }
    if exact.starts_with("MissingIndex") {
        return (
            "CRITICAL",
            format!("published manifest index nodes lost after power loss: {exact}"),
        );
    }
    if exact.starts_with("MissingRoot") {
        return (
            "CRITICAL",
            format!("published manifest root node lost after power loss: {exact}"),
        );
    }
    if obs.starts_with("Wrong") || obs.starts_with("Differ") {
        return (
            "CRITICAL",
            format!("published manifest observation differs from a clean store: {obs}"),
        );
    }
    if obs.starts_with("FailClosed") {
        return (
            "CRITICAL",
            format!("published manifest dependencies unserviceable: {obs}"),
        );
    }
    (
        "PASS",
        "published manifest exact; dependencies present".into(),
    )
}

/// Run one complete (or aborted) build, reconstruct the power-loss state, and
/// check the invariants. Returns the row.
#[allow(clippy::too_many_arguments)]
fn one_case(
    label: &str,
    backend: &str,
    doc: &Path,
    source: &[u8],
    packed: bool,
    policy: SyncPolicy,
    dir_sync: &str,
    arm: &str,
    inject: &str,
    fault_point: Option<&str>,
    drop_dirsync: &[&str],
    baseline: &[(String, Result<String, String>)],
    scratch: &Path,
) -> Row {
    let store = scratch.join(format!("{label}-src"));
    let logp = scratch.join(format!("{label}.log"));
    rm_rf(&store);
    let _ = fs::remove_file(&logp);

    let args = field_build_args(doc, &store, packed, policy, dir_sync);
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut cmd = Command::new(bin());
    cmd.args(&argv)
        .env("VOLE_POWER_LOG", &logp)
        .env("VOLE_POWER_ROOT", &store)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(p) = fault_point {
        cmd.env("VOLE_FAULT_POINT", p);
    }
    let status = cmd.status().expect("spawn field-build");
    let _ = status; // an aborted run is expected for the cut arms

    let log = fs::read_to_string(&logp).unwrap_or_default();
    let model = build_model(&log, drop_dirsync);

    let recon = scratch.join(format!("{label}-recon"));
    rm_rf(&recon);
    let stats = reconstruct(&store, &recon, &model).expect("reconstruct");

    let chk = check_seeds(&recon, packed, policy);
    let (exact, manifests) = check_manifests(&recon, packed, policy, source);
    let observe = if manifests > 0 {
        observe_vs_baseline(&recon, packed, baseline)
    } else {
        "n/a".into()
    };

    // Phase 25: the SAME rules apply to every arm — a cut that publishes a
    // manifest must be exact/serviceable, and a cut with no manifest must reopen
    // (prefix recovery). The old `lenient` rule accepted either.
    let (verdict, note) = strict_verdict(manifests, chk.state, chk.bad_hash, &exact, &observe);

    eprintln!(
        "[phase23-proxy] {label} ({backend}/{policy:?}/{dir_sync}/{arm}/{inject}): {verdict} \
         open={} removed={} removed_dirs={} truncated={} manifests={manifests} {exact} obs={observe} err={}",
        chk.opened, stats.removed, stats.removed_dirs, stats.truncated, chk.err
    );

    Row {
        case: label.into(),
        backend: backend.into(),
        policy: policy_str(policy).into(),
        dir_sync: dir_sync.into(),
        arm: arm.into(),
        inject: inject.into(),
        manifests,
        state: chk.state.into(),
        nodes: chk.nodes,
        bad_hash: chk.bad_hash,
        exact,
        observe,
        verdict: verdict.into(),
        note: format!(
            "open={} removed={} removed_dirs={} truncated={}; {}",
            chk.opened, stats.removed, stats.removed_dirs, stats.truncated, note
        ),
    }
}

const HEADER: &[&str] = &[
    "case",
    "backend",
    "policy",
    "dir_sync",
    "arm",
    "inject",
    "manifests",
    "state",
    "nodes",
    "bad_hash",
    "exact",
    "observe",
    "verdict",
    "note",
];

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

#[test]
#[ignore = "Phase 23 power-loss proxy: model-based simulation over a barrier log; run via tools/phase23-powerloss-court.sh"]
fn power_loss_proxy_court() {
    let Ok(dest) = std::env::var("PHASE23_PROXY_RAW") else {
        eprintln!(
            "[phase23-proxy] PHASE23_PROXY_RAW unset — this proxy runs only through \
             tools/phase23-powerloss-court.sh (or set PHASE23_PROXY_RAW to a TSV path)"
        );
        return;
    };
    let dest = PathBuf::from(dest);
    if let Some(parent) = dest.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut file = fs::File::create(&dest).expect("create proxy TSV");
    writeln!(file, "{}", HEADER.join("\t")).unwrap();

    let doc_ids: Vec<String> = std::env::var("PHASE23_DOCS")
        .unwrap_or_else(|_| "nist-pdf-0002".into())
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    let cut_points: Vec<String> = std::env::var("PHASE23_FAULT_POINTS")
        .unwrap_or_else(|_| {
            "record.after_body,flush.before_sync,flush.after_sync,seal.before_idx,manifest.after_publish"
                .into()
        })
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let scratch = PathBuf::from(
        std::env::var("PHASE23_PROXY_SCRATCH").unwrap_or_else(|_| "/tmp/phase23-proxy".into()),
    );
    rm_rf(&scratch);
    fs::create_dir_all(&scratch).unwrap();

    let mut rows: Vec<Row> = Vec::new();
    let mut push = |f: &mut fs::File, r: Row| {
        let arr = [
            r.case.clone(),
            r.backend.clone(),
            r.policy.clone(),
            r.dir_sync.clone(),
            r.arm.clone(),
            r.inject.clone(),
            r.manifests.to_string(),
            r.state.clone(),
            r.nodes.to_string(),
            r.bad_hash.to_string(),
            r.exact.clone(),
            r.observe.clone(),
            r.verdict.clone(),
            r.note.clone(),
        ];
        writeln!(f, "{}", arr.join("\t")).unwrap();
        f.flush().unwrap();
        rows.push(r);
    };

    for doc_id in &doc_ids {
        let doc = find_doc(doc_id).unwrap_or_else(|| panic!("corpus document {doc_id} not found"));
        let source = fs::read(&doc).unwrap();

        for packed in [false, true] {
            let backend = if packed { "packed" } else { "fs" };

            // A clean, complete build for this backend is the observation
            // baseline: observations of a recovered store are compared to it.
            let base_store = scratch.join(format!("{doc_id}-{backend}-baseline"));
            rm_rf(&base_store);
            {
                let args = field_build_args(&doc, &base_store, packed, SyncPolicy::Batch, "safe");
                let argv: Vec<&str> = args.iter().map(String::as_str).collect();
                let out = Command::new(bin())
                    .args(&argv)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .expect("baseline build");
                assert!(out.success(), "clean baseline build failed for {backend}");
            }
            let baseline: Vec<(String, Result<String, String>)> = list_manifests(&base_store)
                .iter()
                .map(|id| {
                    let hex = id.to_hex();
                    let r = observe_bundle(&base_store, packed, &hex);
                    (hex, r)
                })
                .collect();

            for policy in [SyncPolicy::Batch, SyncPolicy::Each] {
                let pol = policy_str(policy);

                // --- complete runs: safe (the shipped default) and off -------
                for dir_sync in ["safe", "off"] {
                    let label = format!("{doc_id}-{backend}-{pol}-{dir_sync}-complete");
                    let r = one_case(
                        &label,
                        backend,
                        &doc,
                        &source,
                        packed,
                        policy,
                        dir_sync,
                        "complete",
                        "-",
                        None,
                        &[],
                        &baseline,
                        &scratch,
                    );
                    push(&mut file, r);
                }

                // --- counterfactual: dir syncs on, but drop a subtree -------
                // (a model fault; must be caught as CRITICAL, proving sensitivity)
                let seed_ns = if packed { "fieldpack" } else { "seed" };
                for drop in [seed_ns, "index", "descriptor"] {
                    let label = format!("{doc_id}-{backend}-{pol}-drop-{drop}");
                    let r = one_case(
                        &label,
                        backend,
                        &doc,
                        &source,
                        packed,
                        policy,
                        "safe",
                        "model-drop",
                        &format!("drop-dirsync:{drop}"),
                        None,
                        &[drop],
                        &baseline,
                        &scratch,
                    );
                    push(&mut file, r);
                }

                // --- cut arms (deterministic abort at a writer boundary) ----
                if packed {
                    for point in &cut_points {
                        let label = format!("{doc_id}-{backend}-{pol}-cut-{point}");
                        let r = one_case(
                            &label,
                            backend,
                            &doc,
                            &source,
                            packed,
                            policy,
                            "safe",
                            "cut",
                            point,
                            Some(point.as_str()),
                            &[],
                            &baseline,
                            &scratch,
                        );
                        push(&mut file, r);
                    }
                } else {
                    // The fs store's only deterministic boundary is the (first)
                    // manifest publish; still a useful cut.
                    let label = format!("{doc_id}-{backend}-{pol}-cut-manifest.after_publish");
                    let r = one_case(
                        &label,
                        backend,
                        &doc,
                        &source,
                        packed,
                        policy,
                        "safe",
                        "cut",
                        "manifest.after_publish",
                        Some("manifest.after_publish"),
                        &[],
                        &baseline,
                        &scratch,
                    );
                    push(&mut file, r);
                }
            }
        }
    }

    rm_rf(&scratch);

    let crit = rows.iter().filter(|r| r.verdict == "CRITICAL").count();
    let shipped_crit = rows
        .iter()
        .filter(|r| r.verdict == "CRITICAL" && r.arm != "model-drop")
        .count();
    let fail = rows.iter().filter(|r| r.verdict == "FAIL").count();
    eprintln!(
        "[phase23-proxy] cases={} critical={crit} (shipped-arm critical={shipped_crit}) fail={fail}",
        rows.len()
    );
    // The shipped arms (complete / cut, safe or off) must never be CRITICAL.
    assert_eq!(
        shipped_crit, 0,
        "CRITICAL: the shipped durability design lost or corrupted a published field under the model"
    );
    assert_eq!(
        fail, 0,
        "FAIL: {fail} case(s) violated a recovery invariant"
    );
    // The counterfactual arms must each be CRITICAL, or the model is not
    // sensitive to exactly the GAP-1 class of bug it exists to catch.
    let model_drop = rows.iter().filter(|r| r.arm == "model-drop").count();
    let model_drop_crit = rows
        .iter()
        .filter(|r| r.arm == "model-drop" && r.verdict == "CRITICAL")
        .count();
    assert!(model_drop > 0, "no model-drop counterfactual arms ran");
    assert_eq!(
        model_drop_crit, model_drop,
        "the model failed to flag a dropped directory barrier as CRITICAL"
    );
}
