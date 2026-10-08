//! Durability primitives for the field store's atomic writers (Phase 23).
//!
//! Two concerns live here:
//!
//! * **GAP 1 — directory durability.** An atomic publish (`tmp -> fsync ->
//!   rename`) makes the *file's contents* durable but, on strict POSIX, not the
//!   **directory entry** the rename created: after a power cut the rename can be
//!   lost even though the bytes were synced. [`sync_dir`] `fsync`s the containing
//!   directory after a rename (Unix), and the callers `fsync` a directory when a
//!   new segment or index file is created. On by default; [`set_dir_sync_policy`]
//!   exposes the explicit [`DirSyncPolicy::Off`] escape hatch for measurement.
//!   [`create_dir_all`] additionally makes each newly created directory entry
//!   durable in its parent, so an *ancestor* directory (e.g. `index/aa/`) can
//!   never be lost while its child (`index/aa/bb/`) was synced.
//! * **GAP 2 — a barrier log.** Under the non-default `power-log` feature every
//!   durability barrier (`fsync`/`fdatasync`, and what byte range it covered) and
//!   every file creation / rename / directory sync is appended, in program order,
//!   to `$VOLE_POWER_LOG`. From that log the power-loss proxy
//!   (`tests/power_loss_proxy.rs`) reconstructs the post-power-loss state as
//!   "only bytes covered by a completed barrier survive". With the feature off
//!   these functions are thin pass-throughs: no wire byte, no on-disk layout, and
//!   no shipped behavior changes.
//!
//! Paths in the log are relative to `$VOLE_POWER_ROOT` when set, so the proxy can
//! map them onto a reconstructed store.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};

/// Whether an atomic publish `fsync`s the containing directory after the rename.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DirSyncPolicy {
    /// `fsync` the parent directory after every rename and on every new segment /
    /// index file. The default: required for a published manifest and the nodes
    /// it references to survive a real power cut.
    #[default]
    Safe,
    /// Skip directory `fsync`s. Faster ingest, but a rename may be lost across a
    /// power cut. For measurement and for callers that accept that risk.
    Off,
}

static DIR_SYNC_OFF: AtomicU8 = AtomicU8::new(0);

/// Set the process-wide directory-durability policy.
///
/// The default is [`DirSyncPolicy::Safe`]. The CLI (`--dir-sync=safe|off`) and
/// the power-loss proxy set it once at start-up.
pub fn set_dir_sync_policy(policy: DirSyncPolicy) {
    DIR_SYNC_OFF.store(matches!(policy, DirSyncPolicy::Off) as u8, Ordering::SeqCst);
}

/// The active process-wide directory-durability policy.
pub fn dir_sync_policy() -> DirSyncPolicy {
    if DIR_SYNC_OFF.load(Ordering::SeqCst) == 0 {
        DirSyncPolicy::Safe
    } else {
        DirSyncPolicy::Off
    }
}

fn dir_sync_active() -> bool {
    matches!(dir_sync_policy(), DirSyncPolicy::Safe)
}

// ---------------------------------------------------------------------------
// GAP 2: the barrier journal (feature `power-log`)
// ---------------------------------------------------------------------------

#[cfg(feature = "power-log")]
mod journal {
    use std::fs;
    use std::io::Write;
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};

    static LOG: OnceLock<Mutex<Option<fs::File>>> = OnceLock::new();

    fn writer() -> &'static Mutex<Option<fs::File>> {
        LOG.get_or_init(|| {
            let file = std::env::var("VOLE_POWER_LOG").ok().and_then(|p| {
                fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(p)
                    .ok()
            });
            Mutex::new(file)
        })
    }

    /// Render `path` relative to `$VOLE_POWER_ROOT` when it is a prefix.
    pub fn rel(path: &Path) -> String {
        if let Ok(root) = std::env::var("VOLE_POWER_ROOT")
            && let Ok(rest) = path.strip_prefix(&root)
        {
            return rest.to_string_lossy().into_owned();
        }
        path.to_string_lossy().into_owned()
    }

    /// Append one event line (unbuffered, so an abort right after still logs it).
    pub fn event(line: &str) {
        if let Ok(mut guard) = writer().lock()
            && let Some(f) = guard.as_mut()
        {
            let _ = f.write_all(line.as_bytes());
            let _ = f.write_all(b"\n");
            let _ = f.flush();
        }
    }
}

// ---------------------------------------------------------------------------
// instrumented, pass-through filesystem operations
// ---------------------------------------------------------------------------

/// `create_dir_all`, making **every newly created directory entry durable**.
///
/// After each `mkdir`, the containing directory is `fsync`ed (when
/// [`DirSyncPolicy::Safe`], the default) so the new entry survives a power cut.
/// This matters because syncing a store's *leaf* directory does **not** make its
/// **ancestors** durable: a power cut could otherwise lose `index/aa/` even though
/// `index/aa/bb/` was synced, orphaning every node written under it. Components
/// that already exist are left untouched (their parent entries are someone
/// else's responsibility).
///
/// Logged as `mkdir <path>` per created directory and `dirsync <parent>` (via
/// [`sync_dir`]) when the parent sync runs.
pub fn create_dir_all(path: &Path) -> io::Result<()> {
    // Collect the components that do not yet exist, deepest first.
    let mut missing: Vec<PathBuf> = Vec::new();
    let mut cur = path.to_path_buf();
    loop {
        if cur.is_dir() {
            break;
        }
        missing.push(cur.clone());
        match cur.parent() {
            Some(p) if !p.as_os_str().is_empty() => cur = p.to_path_buf(),
            _ => break,
        }
    }
    // Create shallowest first, syncing each new entry's parent.
    for dir in missing.iter().rev() {
        match fs::create_dir(dir) {
            Ok(()) => {
                #[cfg(feature = "power-log")]
                journal::event(&format!("mkdir\t{}", journal::rel(dir)));
                if let Some(parent) = dir.parent()
                    && !parent.as_os_str().is_empty()
                {
                    sync_dir(parent)?;
                }
            }
            // Raced (or a component already existed): the creator owns its sync.
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Truncating create (read + write), logged as `create <path>` (length 0). Opens
/// read+write so the packed writer can `pread` its own open segment.
pub fn create_file(path: &Path) -> io::Result<fs::File> {
    let f = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .read(true)
        .write(true)
        .open(path)?;
    #[cfg(feature = "power-log")]
    journal::event(&format!("create\t{}", journal::rel(path)));
    #[cfg(not(feature = "power-log"))]
    let _ = path;
    Ok(f)
}

/// `write_all`, logged as `write <path> <len-after>` (len is the file's total
/// length after the write; all writers here append or replace whole files).
pub fn write_all(f: &mut fs::File, path: &Path, bytes: &[u8]) -> io::Result<()> {
    f.write_all(bytes)?;
    #[cfg(feature = "power-log")]
    {
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        journal::event(&format!("write\t{}\t{len}", journal::rel(path)));
    }
    #[cfg(not(feature = "power-log"))]
    let _ = path;
    Ok(())
}

/// `sync_all`, logged as `fsync <path> <len>` (the barrier covers `[0, len)`).
pub fn sync_all(f: &fs::File, path: &Path) -> io::Result<()> {
    f.sync_all()?;
    #[cfg(feature = "power-log")]
    {
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        journal::event(&format!("fsync\t{}\t{len}", journal::rel(path)));
    }
    #[cfg(not(feature = "power-log"))]
    let _ = path;
    Ok(())
}

/// `sync_data`, logged as `fsync_data <path> <len>`.
pub fn sync_data(f: &fs::File, path: &Path) -> io::Result<()> {
    f.sync_data()?;
    #[cfg(feature = "power-log")]
    {
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        journal::event(&format!("fsync_data\t{}\t{len}", journal::rel(path)));
    }
    #[cfg(not(feature = "power-log"))]
    let _ = path;
    Ok(())
}

/// `fsync` the directory itself (honoring [`DirSyncPolicy`]), logged as
/// `dirsync <path>` when it actually runs. A directory `fsync` makes the
/// entries created/renamed in that directory up to this point durable.
pub fn sync_dir(path: &Path) -> io::Result<()> {
    #[cfg(not(feature = "power-log"))]
    let _ = path;
    if !dir_sync_active() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        let d = fs::File::open(path)?;
        d.sync_all()?;
    }
    #[cfg(feature = "power-log")]
    journal::event(&format!("dirsync\t{}", journal::rel(path)));
    Ok(())
}

/// `rename`, logged as `rename <from> <to>`.
pub fn rename(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)?;
    #[cfg(feature = "power-log")]
    journal::event(&format!(
        "rename\t{}\t{}",
        journal::rel(from),
        journal::rel(to)
    ));
    #[cfg(not(feature = "power-log"))]
    let _ = (from, to);
    Ok(())
}

/// `set_len`, logged as `setlen <path> <len>`.
pub fn set_len(f: &fs::File, path: &Path, len: u64) -> io::Result<()> {
    f.set_len(len)?;
    #[cfg(feature = "power-log")]
    journal::event(&format!("setlen\t{}\t{len}", journal::rel(path)));
    #[cfg(not(feature = "power-log"))]
    let _ = (path, len);
    Ok(())
}
