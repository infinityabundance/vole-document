//! Packed procedural seed store (`fieldpack`) — Phase 15.3.
//!
//! The reference [`FsSeedStore`](super::FsSeedStore) pays one file (+ two
//! dentries) per content-addressed node, and a `openat`/`newfstatat`/`close`
//! per read. This backend replaces the `seed/` namespace with a small number of
//! append-only **segments** and one immutable hash index per segment, so a
//! `NodeId -> bytes` lookup is a couple of bounded `pread`s plus the *unchanged*
//! BLAKE3 re-hash gate.
//!
//! ## On-disk layout (`<root>/fieldpack/`)
//!
//! ```text
//! seg-NNNNNNNN.pack   append-only payload; header + `u32_len || body` records
//! seg-NNNNNNNN.idx    immutable hash index written at seal (Git-pack convention)
//! ```
//!
//! A `.pack` **with** a sibling `.idx` is *sealed* (never written again); the
//! single `.pack` **without** an `.idx` is the open append segment. No mutable
//! manifest file is ever written, so every metadata file is immutable once
//! written.
//!
//! ### `seg-N.pack`
//!
//! ```text
//! header (24 B): magic b"VOLFPAK1" | version u8 | reserved [u8;7] | seg_id u32 | reserved2 u32
//! records from offset 24: len u32 (1..=u32::MAX) | body [u8; len]
//! ```
//!
//! The node id is **not** stored; it is recomputed as `NodeId::of_node(body)` on
//! read, exactly the reference verification discipline.
//!
//! ### `seg-N.idx`
//!
//! ```text
//! header (32 B): magic b"VOLFPIDX" | version u8 | log2_buckets u8 | reserved [u8;6]
//!                | seg_id u32 | entry_count u64 | reserved_tail u32
//! bucket_off: (nbuckets + 1) * u64   // absolute offset of each bucket's record run
//! records:    entry_count * 48 B, sorted by (bucket, id)
//!             id [u8;32] | off u64 (absolute offset of the record BODY) | len u32 | flags u32
//! ```
//!
//! `bucket(id) = u16_le(id[0..2]) & (nbuckets - 1)`; the ids are already
//! uniformly random BLAKE3 digests, so no extra hashing is needed. `nbuckets` is
//! the smallest power of two (`log2_buckets` in `8..=24`) holding the load factor
//! at or below 0.7.
//!
//! ## Exactness
//!
//! Node identity, canonical bytes, the whole-node re-hash gate, the strict
//! no-clip range semantics, and the `(id, len)` enumeration set are byte-for-byte
//! identical to [`FsSeedStore`](super::FsSeedStore). The segment framing lives
//! *outside* the payload, so it can never change a node's bytes or id. Nothing
//! here is on the `materialize --exact` path.
//!
//! ## Durability policy and crash consistency
//!
//! Appended records live in the open segment until it is **sealed** (an `.idx`
//! is published atomically and a new segment is opened) or explicitly
//! **flushed** (a durability barrier with no visibility change). [`SyncPolicy`]
//! decides when an appended record is forced to stable storage:
//!
//! * [`SyncPolicy::Batch`] (default) — one `fsync` per segment, at seal and at
//!   [`PackedSeedStore::flush`], never per record. This is the sound choice for
//!   this format: the segment is append-only and self-describing, so crash
//!   recovery is a **prefix** recovery (see below), and a published field
//!   manifest is written only after [`PackedSeedStore::flush`] has made every
//!   node it references durable (`FieldStore::put_field`).
//! * [`SyncPolicy::Each`] — a `fdatasync` after every record (the pre-18.5
//!   behavior). Stronger per-`put_node` durability, at one sync round-trip per
//!   seed node.
//!
//! **What a crash guarantees.** A record is a `u32` length prefix followed by
//! its body. On reopen the open segment is scanned from its header: scanning
//! stops at the first prefix that is absent, zero, oversized, or whose body does
//! not fit inside the file. Every record before that point is complete and is
//! recovered; the torn tail is discarded (and truncated on a read-write reopen;
//! a read-only reopen simply ignores it). Therefore:
//!
//! * no **partial** node is ever observable — a record is recovered whole or not
//!   at all, and every fetched node is re-hashed against its id (the unchanged
//!   [`SeedStore::get_node`] gate);
//! * the recovered set is exactly a **prefix** of the appended record sequence;
//! * only records not yet forced to stable storage at the crash are at risk
//!   (with [`SyncPolicy::Batch`], at most the records since the last seal/flush);
//! * a **sealed** segment is never written again, so its `.pack`/`.idx` pair is
//!   immutable and its entries cannot be lost or reordered;
//! * an unsealed segment that lost its tail can be **left incomplete**: recovery
//!   drops the torn tail, and the caller may append from the recovered end. Any
//!   node referenced by a durable field manifest is durable by construction
//!   (the flush-before-publish ordering above), so a dangling manifest cannot
//!   survive a crash.
//!
//! Reads use `std::os::unix::fs::FileExt::read_exact_at` (a *safe* `pread`);
//! mmap is deliberately out of scope because the crate `forbid`s `unsafe_code`
//! (see the Phase 15.3 design, §2.5).

use core::cmp::Ordering;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::error::{Error, Result};

use super::{IoCounters, NodeId, SeedStore};

/// Segment header magic.
pub const PACK_MAGIC: &[u8; 8] = b"VOLFPAK1";
/// Segment header version.
pub const PACK_VERSION: u8 = 1;
/// Fixed segment header length (bytes).
pub const PACK_HEADER_LEN: u64 = 24;
/// Index header magic.
pub const IDX_MAGIC: &[u8; 8] = b"VOLFPIDX";
/// Index header version.
pub const IDX_VERSION: u8 = 1;
/// Fixed index header length (bytes).
pub const IDX_HEADER_LEN: usize = 32;
/// Fixed index record length (bytes).
pub const IDX_RECORD_LEN: usize = 48;
/// The `fieldpack/` directory name under a store root.
pub const PACK_DIR: &str = "fieldpack";
/// Default append-segment size threshold: a new record that would cross this is
/// written to a freshly-opened segment instead.
pub const MAX_SEGMENT_BYTES: u64 = 64 * 1024 * 1024;
/// A single canonical node may not exceed this (the record length field is `u32`).
pub const MAX_PACK_NODE_BYTES: u64 = u32::MAX as u64;

/// When the packed writer forces appended records to stable storage.
///
/// The default is [`SyncPolicy::Batch`]; see the module docs for the crash
/// consistency it provides. [`SyncPolicy::Each`] restores the pre-18.5 per-record
/// barrier for callers that want the strongest per-`put_node` durability at the
/// cost of one sync round-trip per node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncPolicy {
    /// One `fsync` per segment (at seal and at [`PackedSeedStore::flush`]).
    #[default]
    Batch,
    /// A `fdatasync` after every appended record.
    Each,
}

/// The 4-byte little-endian record length prefix.
const RECORD_PREFIX: u64 = 4;
const MIN_LOG2_BUCKETS: u8 = 8;
const MAX_LOG2_BUCKETS: u8 = 24;
/// Target load factor ≤ 7/10 keeps each bucket run near one record.
const LOAD_NUM: u64 = 7;
const LOAD_DEN: u64 = 10;

fn seg_name(seg_id: u32, ext: &str) -> String {
    format!("seg-{seg_id:08}.{ext}")
}

fn pack_path(dir: &Path, seg_id: u32) -> PathBuf {
    dir.join(seg_name(seg_id, "pack"))
}

fn idx_path(dir: &Path, seg_id: u32) -> PathBuf {
    dir.join(seg_name(seg_id, "idx"))
}

fn encode_pack_header(seg_id: u32) -> [u8; PACK_HEADER_LEN as usize] {
    let mut h = [0u8; PACK_HEADER_LEN as usize];
    h[0..8].copy_from_slice(PACK_MAGIC);
    h[8] = PACK_VERSION;
    // reserved 9..16 stays zero
    h[16..20].copy_from_slice(&seg_id.to_le_bytes());
    // reserved2 20..24 stays zero
    h
}

fn parse_pack_header(head: &[u8; PACK_HEADER_LEN as usize], path: &Path) -> Result<u32> {
    if &head[0..8] != PACK_MAGIC {
        return Err(Error::integrity_mismatch(format!(
            "packed segment {} has an unrecognised magic",
            path.display()
        )));
    }
    if head[8] != PACK_VERSION {
        return Err(Error::unsupported_version(format!(
            "packed segment {} has version {}, expected {PACK_VERSION}",
            path.display(),
            head[8]
        )));
    }
    Ok(u32::from_le_bytes(head[16..20].try_into().unwrap()))
}

fn bucket_of(id: &NodeId, nbuckets: usize) -> usize {
    let b = id.as_bytes();
    usize::from(u16::from_le_bytes([b[0], b[1]])) & (nbuckets - 1)
}

fn choose_log2(entry_count: u64) -> u8 {
    let mut l = MIN_LOG2_BUCKETS;
    while l < MAX_LOG2_BUCKETS {
        let nb = 1u64 << l;
        if entry_count <= nb * LOAD_NUM / LOAD_DEN {
            break;
        }
        l += 1;
    }
    l
}

/// A safe `pread`-style exact read. On unix this is
/// [`std::os::unix::fs::FileExt::read_exact_at`]; elsewhere it degrades to a
/// `seek`+`read_exact` (same bounded-range semantics, no `unsafe`).
fn pread_exact(f: &fs::File, buf: &mut [u8], offset: u64) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        f.read_exact_at(buf, offset)
            .map_err(|e| Error::io(format!("reading packed segment at {offset}: {e}")))
    }
    #[cfg(not(unix))]
    {
        use std::io::{Seek, SeekFrom};
        let mut f = f;
        f.seek(SeekFrom::Start(offset))
            .and_then(|_| f.read_exact(buf))
            .map_err(|e| Error::io(format!("reading packed segment at {offset}: {e}")))
    }
}

/// Build the immutable `.idx` bytes for one sealed segment.
fn build_idx(seg_id: u32, entries: &[(NodeId, u64, u32)]) -> Vec<u8> {
    let n = entries.len();
    let log2 = choose_log2(n as u64);
    let nbuckets = 1usize << log2;

    // Sort by (bucket, id) so each bucket is a contiguous, id-sorted run.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_unstable_by(|&a, &b| {
        let ba = bucket_of(&entries[a].0, nbuckets);
        let bb = bucket_of(&entries[b].0, nbuckets);
        ba.cmp(&bb)
            .then_with(|| entries[a].0.as_bytes().cmp(entries[b].0.as_bytes()))
    });

    let rec_base = IDX_HEADER_LEN as u64 + (nbuckets as u64 + 1) * 8;
    let mut bucket_off = vec![0u64; nbuckets + 1];
    let mut pos = 0usize;
    for (b, slot) in bucket_off.iter_mut().enumerate().take(nbuckets) {
        *slot = rec_base + (pos as u64) * IDX_RECORD_LEN as u64;
        while pos < n && bucket_of(&entries[order[pos]].0, nbuckets) == b {
            pos += 1;
        }
    }
    bucket_off[nbuckets] = rec_base + (n as u64) * IDX_RECORD_LEN as u64;

    let mut out = Vec::with_capacity(rec_base as usize + n * IDX_RECORD_LEN);
    out.extend_from_slice(IDX_MAGIC);
    out.push(IDX_VERSION);
    out.push(log2);
    out.extend_from_slice(&[0u8; 6]);
    out.extend_from_slice(&seg_id.to_le_bytes());
    out.extend_from_slice(&(n as u64).to_le_bytes());
    out.extend_from_slice(&[0u8; 4]);
    debug_assert_eq!(out.len(), IDX_HEADER_LEN);
    for off in &bucket_off {
        out.extend_from_slice(&off.to_le_bytes());
    }
    for &oi in &order {
        let (id, off, len) = entries[oi];
        out.extend_from_slice(id.as_bytes());
        out.extend_from_slice(&off.to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
    }
    out
}

/// A sealed segment: its `.idx` bucket directory is resident; the pack/index
/// files are opened lazily.
#[derive(Debug)]
struct SealedSeg {
    pack: PathBuf,
    log2_buckets: u8,
    bucket_off: Vec<u64>,
    entry_count: u64,
    idx_file: fs::File,
    pack_file: Option<fs::File>,
}

impl SealedSeg {
    fn open(pack: PathBuf, idx: PathBuf, seg_id: u32) -> Result<Self> {
        let mut idx_file = fs::File::open(&idx)
            .map_err(|e| Error::io(format!("opening packed index {}: {e}", idx.display())))?;
        let file_len = idx_file.metadata()?.len();
        if file_len < IDX_HEADER_LEN as u64 {
            return Err(Error::integrity_mismatch(format!(
                "packed index {} is shorter than its header",
                idx.display()
            )));
        }
        let mut head = [0u8; IDX_HEADER_LEN];
        idx_file.read_exact(&mut head)?;
        if &head[0..8] != IDX_MAGIC {
            return Err(Error::integrity_mismatch(format!(
                "packed index {} has an unrecognised magic",
                idx.display()
            )));
        }
        if head[8] != IDX_VERSION {
            return Err(Error::unsupported_version(format!(
                "packed index {} has version {}, expected {IDX_VERSION}",
                idx.display(),
                head[8]
            )));
        }
        let log2_buckets = head[9];
        let idx_seg_id = u32::from_le_bytes(head[16..20].try_into().unwrap());
        let entry_count = u64::from_le_bytes(head[20..28].try_into().unwrap());
        if idx_seg_id != seg_id {
            return Err(Error::integrity_mismatch(format!(
                "packed index {} names segment {idx_seg_id}, expected {seg_id}",
                idx.display()
            )));
        }
        if !(MIN_LOG2_BUCKETS..=MAX_LOG2_BUCKETS).contains(&log2_buckets) {
            return Err(Error::integrity_mismatch(format!(
                "packed index {} has log2_buckets {log2_buckets} outside \
                 {MIN_LOG2_BUCKETS}..={MAX_LOG2_BUCKETS}",
                idx.display()
            )));
        }
        let nbuckets = 1usize << log2_buckets;
        let dir_len = (nbuckets + 1) * 8;
        if IDX_HEADER_LEN as u64 + dir_len as u64 > file_len {
            return Err(Error::integrity_mismatch(format!(
                "packed index {} is too short for its bucket directory",
                idx.display()
            )));
        }
        let mut dir = vec![0u8; dir_len];
        idx_file.read_exact(&mut dir)?;
        let mut bucket_off = Vec::with_capacity(nbuckets + 1);
        for i in 0..=nbuckets {
            bucket_off.push(u64::from_le_bytes(
                dir[i * 8..i * 8 + 8].try_into().unwrap(),
            ));
        }
        // The directory must be non-decreasing and describe exactly the record
        // run, which must fit inside the file.
        let rec_base = IDX_HEADER_LEN as u64 + dir_len as u64;
        let rec_end = rec_base + entry_count * IDX_RECORD_LEN as u64;
        let sane = bucket_off[0] == rec_base
            && bucket_off[nbuckets] == rec_end
            && rec_end <= file_len
            && bucket_off.windows(2).all(|w| w[0] <= w[1]);
        if !sane {
            return Err(Error::integrity_mismatch(format!(
                "packed index {} has an inconsistent bucket directory",
                idx.display()
            )));
        }
        Ok(SealedSeg {
            pack,
            log2_buckets,
            bucket_off,
            entry_count,
            idx_file,
            pack_file: None,
        })
    }

    fn pack_file(&mut self) -> Result<&fs::File> {
        if self.pack_file.is_none() {
            self.pack_file = Some(fs::File::open(&self.pack).map_err(|e| {
                Error::io(format!(
                    "opening packed segment {}: {e}",
                    self.pack.display()
                ))
            })?);
        }
        Ok(self.pack_file.as_ref().unwrap())
    }

    /// Locate `id` in this segment, returning `(body_offset, len)`.
    fn lookup(&mut self, id: &NodeId) -> Result<Option<(u64, u32)>> {
        let nbuckets = 1usize << self.log2_buckets;
        let b = bucket_of(id, nbuckets);
        let start = self.bucket_off[b];
        let end = self.bucket_off[b + 1];
        if end <= start {
            return Ok(None);
        }
        let mut run = vec![0u8; (end - start) as usize];
        pread_exact(&self.idx_file, &mut run, start)?;
        let key = id.as_bytes();
        let n = run.len() / IDX_RECORD_LEN;
        let (mut lo, mut hi) = (0usize, n);
        while lo < hi {
            let mid = (lo + hi) / 2;
            let base = mid * IDX_RECORD_LEN;
            match run[base..base + 32].cmp(key.as_slice()) {
                Ordering::Less => lo = mid + 1,
                Ordering::Greater => hi = mid,
                Ordering::Equal => {
                    let off = u64::from_le_bytes(run[base + 32..base + 40].try_into().unwrap());
                    let len = u32::from_le_bytes(run[base + 40..base + 44].try_into().unwrap());
                    return Ok(Some((off, len)));
                }
            }
        }
        Ok(None)
    }

    fn collect_all(&mut self, out: &mut Vec<(NodeId, u64)>) -> Result<()> {
        if self.entry_count == 0 {
            return Ok(());
        }
        let nbuckets = 1usize << self.log2_buckets;
        let rec_base = IDX_HEADER_LEN as u64 + (nbuckets as u64 + 1) * 8;
        let total = usize::try_from(self.entry_count)
            .ok()
            .and_then(|n| n.checked_mul(IDX_RECORD_LEN))
            .ok_or_else(|| Error::resource_limit("packed index record count overflows"))?;
        let mut buf = vec![0u8; total];
        pread_exact(&self.idx_file, &mut buf, rec_base)?;
        for chunk in buf.as_chunks::<IDX_RECORD_LEN>().0 {
            let id = NodeId::from_bytes(chunk[0..32].try_into().unwrap());
            let len = u32::from_le_bytes(chunk[40..44].try_into().unwrap());
            out.push((id, u64::from(len)));
        }
        Ok(())
    }
}

/// The single open (unsealed) segment for a read-only open.
#[derive(Debug)]
struct OpenSeg {
    path: PathBuf,
    entries: HashMap<NodeId, (u64, u32)>,
    file: Option<fs::File>,
}

impl OpenSeg {
    fn file(&mut self) -> Result<&fs::File> {
        if self.file.is_none() {
            self.file = Some(fs::File::open(&self.path).map_err(|e| {
                Error::io(format!(
                    "opening packed segment {}: {e}",
                    self.path.display()
                ))
            })?);
        }
        Ok(self.file.as_ref().unwrap())
    }
}

#[derive(Debug, Default)]
struct PackReader {
    sealed: Vec<SealedSeg>,
    open: Option<OpenSeg>,
}

/// The append-only writer for the open segment.
#[derive(Debug)]
struct PackWriter {
    dir: PathBuf,
    current: Option<fs::File>,
    current_len: u64,
    next_seg_id: u32,
    max_segment_bytes: u64,
    sync_policy: SyncPolicy,
    pending: HashMap<NodeId, (u64, u32)>,
}

impl PackWriter {
    fn ensure_open(&mut self) -> Result<()> {
        if self.current.is_none() {
            let path = pack_path(&self.dir, self.next_seg_id);
            let mut f = fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .read(true)
                .write(true)
                .open(&path)
                .map_err(|e| {
                    Error::io(format!("creating packed segment {}: {e}", path.display()))
                })?;
            f.write_all(&encode_pack_header(self.next_seg_id))?;
            self.current = Some(f);
            self.current_len = PACK_HEADER_LEN;
        }
        Ok(())
    }
}

#[derive(Debug)]
struct PackState {
    reader: RefCell<PackReader>,
    writer: Option<RefCell<PackWriter>>,
}

/// Where a located record's bytes live.
#[derive(Clone, Copy, Debug)]
enum Src {
    /// The live writer's current segment (in-process, unsealed).
    Writer,
    /// The recovered open segment of a read-only store.
    ReaderOpen,
    /// Sealed segment `n`.
    Sealed(usize),
}

#[derive(Clone, Copy, Debug)]
struct Located {
    src: Src,
    off: u64,
    len: u32,
}

/// A packed content-addressed store of canonical procedural seed nodes.
///
/// Implements [`SeedStore`] with the same identity, bytes, verification, range,
/// and enumeration semantics as [`super::FsSeedStore`]; only the physical layout
/// differs (`fieldpack/` segments instead of one file per node).
pub struct PackedSeedStore {
    root: PathBuf,
    io: IoCounters,
    state: Rc<PackState>,
}

impl std::fmt::Debug for PackedSeedStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PackedSeedStore")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

/// Discover the segment ids under `dir`: sealed ids (with an `.idx`) ascending,
/// and the single open id (a `.pack` with no `.idx`), if any.
fn discover(dir: &Path) -> Result<(Vec<u32>, Option<u32>)> {
    let mut sealed = Vec::new();
    let mut open: Option<u32> = None;
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((sealed, open)),
        Err(e) => {
            return Err(Error::io(format!(
                "reading packed store {}: {e}",
                dir.display()
            )));
        }
    };
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(idpart) = name
            .strip_prefix("seg-")
            .and_then(|r| r.strip_suffix(".pack"))
        else {
            continue;
        };
        let Ok(seg_id) = idpart.parse::<u32>() else {
            continue;
        };
        if idx_path(dir, seg_id).exists() {
            sealed.push(seg_id);
        } else {
            open = Some(open.map_or(seg_id, |o| o.max(seg_id)));
        }
    }
    sealed.sort_unstable();
    Ok((sealed, open))
}

/// `(id, body_offset, len)` for every complete record in an unsealed segment,
/// paired with the length of the last complete record boundary.
type ScannedOpenSegment = (Vec<(NodeId, u64, u32)>, u64);

/// Scan an unsealed segment from its header, returning every complete record's
/// `(id, body_offset, len)` and the length of the last complete record boundary
/// (a torn tail is truncated to this on reopen).
fn scan_open_segment(path: &Path, expected: u32) -> Result<ScannedOpenSegment> {
    let f = fs::File::open(path)
        .map_err(|e| Error::io(format!("opening packed segment {}: {e}", path.display())))?;
    let file_len = f.metadata()?.len();
    if file_len < PACK_HEADER_LEN {
        return Err(Error::integrity_mismatch(format!(
            "packed segment {} has a truncated header",
            path.display()
        )));
    }
    let mut head = [0u8; PACK_HEADER_LEN as usize];
    pread_exact(&f, &mut head, 0)?;
    if parse_pack_header(&head, path)? != expected {
        return Err(Error::integrity_mismatch(format!(
            "packed segment {} names a different segment id",
            path.display()
        )));
    }
    let mut entries = Vec::new();
    let mut p = PACK_HEADER_LEN;
    loop {
        if p + RECORD_PREFIX > file_len {
            break;
        }
        let mut pre = [0u8; 4];
        pread_exact(&f, &mut pre, p)?;
        let len = u64::from(u32::from_le_bytes(pre));
        if len == 0 || len > MAX_PACK_NODE_BYTES {
            break;
        }
        if p + RECORD_PREFIX + len > file_len {
            break;
        }
        let mut body = vec![0u8; len as usize];
        pread_exact(&f, &mut body, p + RECORD_PREFIX)?;
        let id = NodeId::of_node(&body);
        entries.push((
            id,
            p + RECORD_PREFIX,
            u32::try_from(len).unwrap_or(u32::MAX),
        ));
        p += RECORD_PREFIX + len;
    }
    Ok((entries, p))
}

impl PackedSeedStore {
    fn pack_dir(&self) -> PathBuf {
        self.root.join(PACK_DIR)
    }

    /// Open the store read-only: sealed segments via their `.idx`, plus the open
    /// segment (if any) recovered by a framing scan.
    pub fn open_read(root: impl AsRef<Path>, io: IoCounters) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let dir = root.join(PACK_DIR);
        let (sealed_ids, open_id) = discover(&dir)?;
        let mut sealed = Vec::with_capacity(sealed_ids.len());
        for seg_id in sealed_ids {
            sealed.push(SealedSeg::open(
                pack_path(&dir, seg_id),
                idx_path(&dir, seg_id),
                seg_id,
            )?);
        }
        let open = match open_id {
            Some(seg_id) => {
                let path = pack_path(&dir, seg_id);
                let (entries, _valid_len) = scan_open_segment(&path, seg_id)?;
                let mut map = HashMap::with_capacity(entries.len());
                for (id, off, len) in entries {
                    map.insert(id, (off, len));
                }
                Some(OpenSeg {
                    path,
                    entries: map,
                    file: None,
                })
            }
            None => None,
        };
        Ok(PackedSeedStore {
            root,
            io,
            state: Rc::new(PackState {
                reader: RefCell::new(PackReader { sealed, open }),
                writer: None,
            }),
        })
    }

    /// Open the store read-write, recovering (and truncating the torn tail of) an
    /// unsealed segment so appends continue from the last complete record.
    ///
    /// Uses the default [`SyncPolicy::Batch`].
    pub fn open_write(root: impl AsRef<Path>, io: IoCounters) -> Result<Self> {
        Self::open_write_internal(root.as_ref(), io, MAX_SEGMENT_BYTES, SyncPolicy::default())
    }

    /// Open the store read-write with an explicit durability [`SyncPolicy`].
    pub fn open_write_with_policy(
        root: impl AsRef<Path>,
        io: IoCounters,
        policy: SyncPolicy,
    ) -> Result<Self> {
        Self::open_write_internal(root.as_ref(), io, MAX_SEGMENT_BYTES, policy)
    }

    fn open_write_internal(
        root: &Path,
        io: IoCounters,
        max_segment_bytes: u64,
        policy: SyncPolicy,
    ) -> Result<Self> {
        let root = root.to_path_buf();
        let dir = root.join(PACK_DIR);
        fs::create_dir_all(&dir)?;
        let (sealed_ids, open_id) = discover(&dir)?;
        let next_seg_for_fresh = sealed_ids.last().copied().map_or(0, |m| m + 1);
        let mut sealed = Vec::with_capacity(sealed_ids.len());
        for seg_id in sealed_ids {
            sealed.push(SealedSeg::open(
                pack_path(&dir, seg_id),
                idx_path(&dir, seg_id),
                seg_id,
            )?);
        }
        let reader = PackReader { sealed, open: None };
        let writer = match open_id {
            Some(seg_id) => {
                let path = pack_path(&dir, seg_id);
                let (entries, valid_len) = scan_open_segment(&path, seg_id)?;
                if valid_len < fs::metadata(&path)?.len() {
                    fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .open(&path)?
                        .set_len(valid_len)?;
                }
                let f = fs::OpenOptions::new()
                    .read(true)
                    .append(true)
                    .open(&path)
                    .map_err(|e| {
                        Error::io(format!("opening packed segment {}: {e}", path.display()))
                    })?;
                let mut pending = HashMap::with_capacity(entries.len());
                for (id, off, len) in entries {
                    pending.insert(id, (off, len));
                }
                PackWriter {
                    dir: dir.clone(),
                    current: Some(f),
                    current_len: valid_len,
                    next_seg_id: seg_id,
                    max_segment_bytes,
                    sync_policy: policy,
                    pending,
                }
            }
            None => PackWriter {
                dir: dir.clone(),
                current: None,
                current_len: 0,
                next_seg_id: next_seg_for_fresh,
                max_segment_bytes,
                sync_policy: policy,
                pending: HashMap::new(),
            },
        };
        Ok(PackedSeedStore {
            root,
            io,
            state: Rc::new(PackState {
                reader: RefCell::new(reader),
                writer: Some(RefCell::new(writer)),
            }),
        })
    }

    /// The store root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Seal the open segment: fsync it, publish `seg-N.idx` atomically, and make
    /// it readable. Called by [`crate::field::FieldStore::sync`].
    pub fn seal(&self) -> Result<()> {
        let Some(w_cell) = self.state.writer.as_ref() else {
            return Ok(());
        };
        let (seg_id, pending) = {
            let mut w = w_cell.borrow_mut();
            if w.pending.is_empty() {
                return Ok(());
            }
            if let Some(f) = w.current.as_ref() {
                f.sync_all()?;
            }
            let seg_id = w.next_seg_id;
            let pending = std::mem::take(&mut w.pending);
            w.current = None;
            w.current_len = 0;
            w.next_seg_id = w
                .next_seg_id
                .checked_add(1)
                .ok_or_else(|| Error::resource_limit("packed segment id space exhausted"))?;
            (seg_id, pending)
        };
        let entries: Vec<(NodeId, u64, u32)> = pending
            .into_iter()
            .map(|(id, (off, len))| (id, off, len))
            .collect();
        let idx = build_idx(seg_id, &entries);
        let dir = self.pack_dir();
        let idx_file = idx_path(&dir, seg_id);
        #[cfg(feature = "fault-inject")]
        crate::fault::hit("seal.before_idx");
        crate::field::write_atomic(&idx_file, &idx)?;
        #[cfg(feature = "fault-inject")]
        crate::fault::hit("seal.after_idx");
        let seg = SealedSeg::open(pack_path(&dir, seg_id), idx_file, seg_id)?;
        self.state.reader.borrow_mut().sealed.push(seg);
        Ok(())
    }

    fn locate(&self, id: &NodeId) -> Result<Option<Located>> {
        if let Some(w_cell) = self.state.writer.as_ref()
            && let Some(&(off, len)) = w_cell.borrow().pending.get(id)
        {
            return Ok(Some(Located {
                src: Src::Writer,
                off,
                len,
            }));
        }
        let mut r = self.state.reader.borrow_mut();
        if let Some(o) = r.open.as_ref()
            && let Some(&(off, len)) = o.entries.get(id)
        {
            return Ok(Some(Located {
                src: Src::ReaderOpen,
                off,
                len,
            }));
        }
        for (i, s) in r.sealed.iter_mut().enumerate() {
            if let Some((off, len)) = s.lookup(id)? {
                return Ok(Some(Located {
                    src: Src::Sealed(i),
                    off,
                    len,
                }));
            }
        }
        Ok(None)
    }

    fn read_at(&self, loc: &Located, delta: u64, buf: &mut [u8]) -> Result<()> {
        let off = loc.off + delta;
        match loc.src {
            Src::Writer => {
                let w = self
                    .state
                    .writer
                    .as_ref()
                    .ok_or_else(|| Error::internal_invariant("packed writer vanished"))?
                    .borrow();
                let f = w.current.as_ref().ok_or_else(|| {
                    Error::internal_invariant("packed writer has no open segment")
                })?;
                pread_exact(f, buf, off)
            }
            Src::ReaderOpen => {
                let mut r = self.state.reader.borrow_mut();
                let o = r
                    .open
                    .as_mut()
                    .ok_or_else(|| Error::internal_invariant("packed open segment vanished"))?;
                let f = o.file()?;
                pread_exact(f, buf, off)
            }
            Src::Sealed(i) => {
                let mut r = self.state.reader.borrow_mut();
                let s = r
                    .sealed
                    .get_mut(i)
                    .ok_or_else(|| Error::internal_invariant("packed sealed segment vanished"))?;
                let f = s.pack_file()?;
                pread_exact(f, buf, off)
            }
        }
    }

    /// Store the canonical bytes of one node (idempotent).
    ///
    /// With [`SyncPolicy::Each`] the record is `fdatasync`'d before returning;
    /// with the default [`SyncPolicy::Batch`] it is visible immediately (through
    /// the pending map) but becomes crash-durable at the next seal or
    /// [`PackedSeedStore::flush`]. See the module docs for the exact semantics.
    pub fn insert(&self, canonical: &[u8]) -> Result<NodeId> {
        let id = NodeId::of_node(canonical);
        if self.has(&id)? {
            return Ok(id);
        }
        if canonical.len() as u64 > MAX_PACK_NODE_BYTES {
            return Err(Error::resource_limit(format!(
                "seed node is {} bytes, exceeding the packed maximum {MAX_PACK_NODE_BYTES}",
                canonical.len()
            )));
        }
        let w_cell =
            self.state.writer.as_ref().ok_or_else(|| {
                Error::unsupported_feature("packed seed store is opened read-only")
            })?;
        let need_seal = {
            let w = w_cell.borrow();
            w.current.is_some()
                && w.current_len + RECORD_PREFIX + canonical.len() as u64 > w.max_segment_bytes
        };
        if need_seal {
            self.seal()?;
        }
        let mut w = w_cell.borrow_mut();
        w.ensure_open()?;
        let body_off = w.current_len + RECORD_PREFIX;
        let sync_each = w.sync_policy == SyncPolicy::Each;
        let mut rec = Vec::with_capacity(RECORD_PREFIX as usize + canonical.len());
        rec.extend_from_slice(&(canonical.len() as u32).to_le_bytes());
        rec.extend_from_slice(canonical);
        let f = w
            .current
            .as_mut()
            .ok_or_else(|| Error::internal_invariant("packed writer failed to open a segment"))?;
        // The `fault-inject` build splits the record write at the framing
        // boundary so the court can abort between prefix and body; the shipped
        // build writes the whole record in one call (unchanged).
        #[cfg(feature = "fault-inject")]
        {
            crate::fault::hit("record.before_prefix");
            f.write_all(&rec[..RECORD_PREFIX as usize])?;
            crate::fault::hit("record.after_prefix");
            f.write_all(&rec[RECORD_PREFIX as usize..])?;
            crate::fault::hit("record.after_body");
        }
        #[cfg(not(feature = "fault-inject"))]
        f.write_all(&rec)?;
        if sync_each {
            f.sync_data()?;
        }
        w.current_len = body_off + canonical.len() as u64;
        w.pending.insert(id, (body_off, canonical.len() as u32));
        Ok(id)
    }

    /// Force every appended record in the open segment to stable storage without
    /// sealing it. A durability barrier, not a visibility change: the pending map
    /// already serves every appended node. Called by
    /// [`FieldStore::put_field`](crate::field::FieldStore) so a published manifest
    /// only ever references nodes that are already durable.
    pub fn flush(&self) -> Result<()> {
        let Some(w_cell) = self.state.writer.as_ref() else {
            return Ok(());
        };
        let w = w_cell.borrow();
        if let Some(f) = w.current.as_ref() {
            #[cfg(feature = "fault-inject")]
            crate::fault::hit("flush.before_sync");
            f.sync_all()?;
            #[cfg(feature = "fault-inject")]
            crate::fault::hit("flush.after_sync");
        }
        Ok(())
    }

    /// Fetch a node, verifying `NodeId::of_node(bytes) == id`.
    pub fn fetch(&self, id: &NodeId) -> Result<Vec<u8>> {
        let loc = self.locate(id)?.ok_or_else(|| {
            Error::missing_external_object(format!("seed node {id} is not present"))
        })?;
        let mut buf = vec![0u8; loc.len as usize];
        self.read_at(&loc, 0, &mut buf)?;
        let actual = NodeId::of_node(&buf);
        if actual != *id {
            return Err(Error::integrity_mismatch(format!(
                "seed node {id} content hashes to {actual}"
            )));
        }
        self.io.add_seed(buf.len() as u64);
        Ok(buf)
    }

    /// Fetch `len` bytes at `offset` (strict; no EOF clip, no whole-node gate).
    pub fn fetch_range(&self, id: &NodeId, offset: u64, len: u64) -> Result<Vec<u8>> {
        let loc = self.locate(id)?.ok_or_else(|| {
            Error::missing_external_object(format!("seed node {id} is not present"))
        })?;
        let stored = u64::from(loc.len);
        if offset.checked_add(len).is_none_or(|end| end > stored) {
            return Err(Error::integrity_mismatch(format!(
                "seed node {id} range [{offset}, {}) exceeds stored length {stored}",
                offset.saturating_add(len)
            )));
        }
        let mut buf = vec![0u8; usize::try_from(len).unwrap_or(usize::MAX)];
        self.read_at(&loc, offset, &mut buf)?;
        self.io.add_seed(buf.len() as u64);
        Ok(buf)
    }

    /// Whether `id` is present.
    pub fn has(&self, id: &NodeId) -> Result<bool> {
        Ok(self.locate(id)?.is_some())
    }

    /// Every stored `(id, canonical_len)`, sorted ascending.
    pub fn entries(&self) -> Result<Vec<(NodeId, u64)>> {
        let mut out: Vec<(NodeId, u64)> = Vec::new();
        {
            let mut r = self.state.reader.borrow_mut();
            for s in r.sealed.iter_mut() {
                s.collect_all(&mut out)?;
            }
            if let Some(o) = r.open.as_ref() {
                for (id, (_, len)) in &o.entries {
                    out.push((*id, u64::from(*len)));
                }
            }
        }
        if let Some(w_cell) = self.state.writer.as_ref() {
            for (id, (_, len)) in &w_cell.borrow().pending {
                out.push((*id, u64::from(*len)));
            }
        }
        out.sort_unstable();
        Ok(out)
    }
}

impl SeedStore for PackedSeedStore {
    fn put_node(&mut self, canonical: &[u8]) -> Result<NodeId> {
        self.insert(canonical)
    }

    fn get_node(&self, id: &NodeId) -> Result<Vec<u8>> {
        self.fetch(id)
    }

    fn get_node_range(&self, id: &NodeId, offset: u64, len: u64) -> Result<Vec<u8>> {
        self.fetch_range(id, offset, len)
    }

    fn contains_node(&self, id: &NodeId) -> Result<bool> {
        self.has(id)
    }

    fn list_nodes(&self) -> Result<Vec<(NodeId, u64)>> {
        self.entries()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{FsSeedStore, SeedStore};

    fn temp_root(label: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "vole-pack-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        p
    }

    fn sample_nodes() -> Vec<Vec<u8>> {
        vec![
            b"alpha".to_vec(),
            b"beta-node".to_vec(),
            b"gamma-delta-epsilon".to_vec(),
            vec![0u8; 97],
            (0..255u8).collect(),
        ]
    }

    fn pack_file(root: &Path, seg_id: u32) -> PathBuf {
        root.join(PACK_DIR).join(seg_name(seg_id, "pack"))
    }

    #[test]
    fn put_get_contains_list_match_fs_semantics() {
        let root = temp_root("rt");
        let nodes = sample_nodes();

        // The reference backend: one file per node.
        let mut fs_store = FsSeedStore::open(&root).unwrap();
        // The packed backend: segments under the same root.
        let packed = PackedSeedStore::open_write(&root, IoCounters::new()).unwrap();

        let fs_ids: Vec<NodeId> = nodes
            .iter()
            .map(|n| fs_store.put_node(n).unwrap())
            .collect();
        let packed_ids: Vec<NodeId> = nodes.iter().map(|n| packed.insert(n).unwrap()).collect();
        assert_eq!(fs_ids, packed_ids, "node ids are content-derived and equal");

        // Idempotent: re-putting does not change ids or the enumeration set.
        let again: Vec<NodeId> = nodes.iter().map(|n| packed.insert(n).unwrap()).collect();
        assert_eq!(again, packed_ids);

        for (id, node) in packed_ids.iter().zip(&nodes) {
            assert!(packed.has(id).unwrap());
            assert!(packed.contains_node(id).unwrap());
            assert_eq!(&packed.fetch(id).unwrap(), node);
            assert_eq!(&packed.get_node(id).unwrap(), node);
            assert_eq!(&fs_store.get_node(id).unwrap(), node);
        }

        // Same (id, len) set and ordering as the reference backend.
        assert_eq!(packed.entries().unwrap(), fs_store.list_nodes().unwrap());
        assert_eq!(packed.list_nodes().unwrap(), fs_store.list_nodes().unwrap());

        // Seal, then a fresh read-only open sees exactly the same set.
        packed.seal().unwrap();
        assert!(pack_file(&root, 0).exists(), "segment 0 is written");
        assert!(
            idx_path(&root.join(PACK_DIR), 0).exists(),
            "segment 0 is sealed"
        );
        let ro = PackedSeedStore::open_read(&root, IoCounters::new()).unwrap();
        assert_eq!(ro.entries().unwrap(), fs_store.list_nodes().unwrap());
        for (id, node) in packed_ids.iter().zip(&nodes) {
            assert!(ro.has(id).unwrap());
            assert_eq!(&ro.fetch(id).unwrap(), node);
        }

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn missing_node_is_a_miss() {
        let root = temp_root("miss");
        let packed = PackedSeedStore::open_write(&root, IoCounters::new()).unwrap();
        packed.insert(b"only one").unwrap();

        let absent = NodeId::from_bytes([0x5A; 32]);
        assert!(!packed.has(&absent).unwrap());
        let e = packed.fetch(&absent).unwrap_err();
        assert_eq!(e.class(), crate::ErrorClass::MissingExternalObject);
        assert_eq!(
            packed.fetch_range(&absent, 0, 1).unwrap_err().class(),
            crate::ErrorClass::MissingExternalObject
        );

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn corrupted_body_fails_the_hash_gate() {
        let root = temp_root("corrupt");
        let packed = PackedSeedStore::open_write(&root, IoCounters::new()).unwrap();
        let id = packed.insert(b"canonical node bytes").unwrap();
        packed.seal().unwrap();

        // Flip one body byte in the sealed segment (body starts after the 24-byte
        // header and the 4-byte length prefix).
        let path = pack_file(&root, 0);
        let mut bytes = fs::read(&path).unwrap();
        let body_at = PACK_HEADER_LEN as usize + RECORD_PREFIX as usize;
        bytes[body_at] ^= 0xFF;
        fs::write(&path, &bytes).unwrap();

        let ro = PackedSeedStore::open_read(&root, IoCounters::new()).unwrap();
        assert_eq!(
            ro.fetch(&id).unwrap_err().class(),
            crate::ErrorClass::IntegrityMismatch
        );

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn range_reads_are_strict_and_ungated() {
        let root = temp_root("range");
        let packed = PackedSeedStore::open_write(&root, IoCounters::new()).unwrap();
        let id = packed.insert(b"0123456789").unwrap();
        assert_eq!(packed.fetch_range(&id, 2, 3).unwrap(), b"234");
        assert_eq!(packed.fetch_range(&id, 0, 10).unwrap(), b"0123456789");
        assert_eq!(
            packed.fetch_range(&id, 8, 5).unwrap_err().class(),
            crate::ErrorClass::IntegrityMismatch
        );
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn sealing_spans_multiple_segments() {
        let root = temp_root("segments");
        // A tiny limit forces a new segment every couple of records.
        let packed =
            PackedSeedStore::open_write_internal(&root, IoCounters::new(), 64, SyncPolicy::Batch)
                .unwrap();
        let nodes = sample_nodes();
        let ids: Vec<NodeId> = nodes.iter().map(|n| packed.insert(n).unwrap()).collect();
        packed.seal().unwrap();

        let dir = root.join(PACK_DIR);
        let segs: Vec<u32> = discover(&dir).unwrap().0;
        assert!(
            segs.len() >= 2,
            "the tiny limit must roll segments, got {segs:?}"
        );

        // A read-only reopen still locates every node across segments.
        let ro = PackedSeedStore::open_read(&root, IoCounters::new()).unwrap();
        assert_eq!(ro.entries().unwrap().len(), nodes.len());
        for (id, node) in ids.iter().zip(&nodes) {
            assert_eq!(&ro.fetch(id).unwrap(), node);
        }
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn torn_tail_is_truncated_on_reopen() {
        let root = temp_root("torn");
        {
            let packed = PackedSeedStore::open_write(&root, IoCounters::new()).unwrap();
            packed.insert(b"first record").unwrap();
            packed.insert(b"second record").unwrap();
            // Deliberately do NOT seal: the segment stays open with no `.idx`.
        }
        let path = pack_file(&root, 0);
        // Append a torn record: a length prefix promising a body that is not there.
        {
            let mut bytes = fs::read(&path).unwrap();
            bytes.extend_from_slice(&9u32.to_le_bytes());
            bytes.extend_from_slice(b"partial");
            fs::write(&path, &bytes).unwrap();
        }
        let before = fs::metadata(&path).unwrap().len();

        // Reopening read-write truncates the torn tail and keeps the complete records.
        let packed = PackedSeedStore::open_write(&root, IoCounters::new()).unwrap();
        let after = fs::metadata(&path).unwrap().len();
        assert!(after < before, "torn tail must be truncated");
        let entries = packed.entries().unwrap();
        assert_eq!(entries.len(), 2, "both complete records recovered");
        assert_eq!(packed.fetch(&entries[0].0).unwrap(), b"first record");
        assert_eq!(packed.fetch(&entries[1].0).unwrap(), b"second record");

        // Appending after recovery keeps the file consistent.
        packed.insert(b"third record").unwrap();
        packed.seal().unwrap();
        let ro = PackedSeedStore::open_read(&root, IoCounters::new()).unwrap();
        assert_eq!(ro.entries().unwrap().len(), 3);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn same_ingest_produces_identical_segments() {
        let a = temp_root("det-a");
        let b = temp_root("det-b");
        let nodes = sample_nodes();
        for root in [&a, &b] {
            let packed = PackedSeedStore::open_write(root, IoCounters::new()).unwrap();
            for n in &nodes {
                packed.insert(n).unwrap();
            }
            packed.seal().unwrap();
        }
        assert_eq!(
            fs::read(pack_file(&a, 0)).unwrap(),
            fs::read(pack_file(&b, 0)).unwrap(),
            "pack segments are byte-identical"
        );
        assert_eq!(
            fs::read(idx_path(&a.join(PACK_DIR), 0)).unwrap(),
            fs::read(idx_path(&b.join(PACK_DIR), 0)).unwrap(),
            "index segments are byte-identical"
        );
        fs::remove_dir_all(&a).ok();
        fs::remove_dir_all(&b).ok();
    }

    /// The default [`SyncPolicy::Batch`] makes appended records visible and
    /// recoverable without a per-record sync: a crash that tears the trailing
    /// record recovers exactly the complete prefix, and no partial node is ever
    /// returned (the per-fetch re-hash gate still applies).
    #[test]
    fn batch_policy_recovers_a_complete_prefix_after_a_torn_tail() {
        let root = temp_root("batch-torn");
        let nodes = sample_nodes();
        let ids: Vec<NodeId> = {
            let packed = PackedSeedStore::open_write_with_policy(
                &root,
                IoCounters::new(),
                SyncPolicy::Batch,
            )
            .unwrap();
            nodes.iter().map(|n| packed.insert(n).unwrap()).collect()
            // Drop without sealing: the tail is durable only to the page cache.
        };
        let path = pack_file(&root, 0);
        // Simulate a power-loss torn tail: a length prefix whose body is short.
        {
            let mut bytes = fs::read(&path).unwrap();
            bytes.extend_from_slice(&11u32.to_le_bytes());
            bytes.extend_from_slice(b"partial");
            fs::write(&path, &bytes).unwrap();
        }
        let before = fs::metadata(&path).unwrap().len();

        let packed = PackedSeedStore::open_write(&root, IoCounters::new()).unwrap();
        assert!(
            fs::metadata(&path).unwrap().len() < before,
            "the torn tail is truncated to the last complete record boundary"
        );
        assert_eq!(
            packed.entries().unwrap().len(),
            nodes.len(),
            "exactly the complete prefix is recovered"
        );
        for (id, node) in ids.iter().zip(&nodes) {
            assert_eq!(&packed.fetch(id).unwrap(), node);
        }
        fs::remove_dir_all(&root).ok();
    }

    /// The sync policy is a durability distinction only: it must not change any
    /// stored byte, node id, or the `(id, len)` enumeration set. An explicit
    /// [`PackedSeedStore::flush`] is a no-op when nothing has been appended.
    #[test]
    fn sync_policy_does_not_change_stored_bytes() {
        let a = temp_root("policy-batch");
        let b = temp_root("policy-each");
        let nodes = sample_nodes();
        for (root, policy) in [(&a, SyncPolicy::Batch), (&b, SyncPolicy::Each)] {
            let packed =
                PackedSeedStore::open_write_with_policy(root, IoCounters::new(), policy).unwrap();
            packed.flush().unwrap();
            for n in &nodes {
                packed.insert(n).unwrap();
            }
            packed.flush().unwrap();
            packed.seal().unwrap();
        }
        let ro = PackedSeedStore::open_read(&a, IoCounters::new()).unwrap();
        for node in &nodes {
            let id = NodeId::of_node(node);
            assert!(ro.has(&id).unwrap());
            assert_eq!(&ro.fetch(&id).unwrap(), node);
        }
        assert_eq!(
            fs::read(pack_file(&a, 0)).unwrap(),
            fs::read(pack_file(&b, 0)).unwrap(),
            "the policy must not change the pack bytes"
        );
        assert_eq!(
            fs::read(idx_path(&a.join(PACK_DIR), 0)).unwrap(),
            fs::read(idx_path(&b.join(PACK_DIR), 0)).unwrap(),
            "the policy must not change the index bytes"
        );
        fs::remove_dir_all(&a).ok();
        fs::remove_dir_all(&b).ok();
    }
}
