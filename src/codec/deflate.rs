//! Exact DEFLATE bitstream replay, built on `preflate-rs` (Phase 6).
//!
//! `preflate` reconstructs the *original* raw DEFLATE (RFC 1951) bitstream from
//! the decompressed plaintext plus a compact correction blob. This module wraps
//! that capability in the guarantees VOLE-Document requires:
//!
//! 1. **Bit-exactness before admission.** [`try_replay`] only returns a plan when
//!    a fresh replay of `(plaintext, corrections)` reproduces the raw DEFLATE
//!    payload byte-for-byte. A stream that cannot be reproduced is declined.
//! 2. **No panic, ever.** Both analysis and reconstruction are isolated behind
//!    [`std::panic::catch_unwind`]: `preflate` can panic on hostile correction
//!    data (observed: index-out-of-bounds and explicit `panic!`), so a panic is
//!    converted into a typed [`crate::ErrorClass::CodecReplay`] error rather than
//!    escaping. The catch is not a substitute for the whole-source SHA-256 court;
//!    it only bounds failure.
//! 3. **Bounded work.** Analysis runs with a pinned chain bound and a plaintext
//!    limit derived from [`Limits`], and a stream that is not fully consumed
//!    (a silently-truncated result) is declined.
//! 4. **Process isolation on the decode path.** `preflate` reconstruction can
//!    allocate unboundedly from hostile corrections (fuzz finding F2). The
//!    decoder calls [`replay_bounded`], which runs reconstruction in a child
//!    process under an `RLIMIT_AS` address-space cap and a wall-clock timeout;
//!    [`replay_raw`] remains for the encoder's own verification and the fuzz
//!    targets.
//!
//! The caller owns zlib (RFC 1950) framing: `preflate` operates on the raw
//! DEFLATE payload, so the 2-byte zlib header and 4-byte Adler-32 trailer are
//! returned verbatim in the [`ReplayPlan`] and re-emitted as literal program
//! bytes. Adler-32 *regeneration* is deliberately not assumed here.

use std::io::{self, Read, Write};
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use preflate_rs::{PreflateConfig, preflate_whole_deflate_stream, recreate_whole_deflate_stream};

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Size of the zlib (RFC 1950) 2-byte header.
pub const ZLIB_HEADER_LEN: usize = 2;
/// Size of the zlib (RFC 1950) 4-byte Adler-32 trailer.
pub const ZLIB_TRAILER_LEN: usize = 4;
/// Minimum viable zlib stream: header + at least one DEFLATE byte + trailer.
pub const MIN_ZLIB_LEN: usize = ZLIB_HEADER_LEN + 1 + ZLIB_TRAILER_LEN;

/// Pinned hash-chain lookup bound. Fixed so an encoded result does not depend on
/// a `preflate-rs` default changing between versions in the sealed universe.
pub const MAX_CHAIN_LENGTH: u32 = 4096;

/// A verified exact-replay plan for one zlib-wrapped stream.
///
/// The reconstruction is `header · raw_deflate(plaintext, corrections) · adler`.
/// `plaintext` is the decompressed data; `corrections` is the opaque,
/// version-coupled `preflate` state that makes the replay exact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayPlan {
    /// The exact 2-byte zlib header from the source.
    pub header: [u8; ZLIB_HEADER_LEN],
    /// Decompressed plaintext (stored as an object or entropy channel).
    pub plaintext: Vec<u8>,
    /// Opaque `preflate` correction blob (stored verbatim).
    pub corrections: Vec<u8>,
    /// The exact 4-byte Adler-32 trailer from the source.
    pub adler: [u8; ZLIB_TRAILER_LEN],
    /// Length of the original raw DEFLATE payload that the op reproduces.
    pub raw_len: u32,
}

/// Whether `bytes` begins with a structurally valid zlib (RFC 1950) header.
///
/// Checks only the header invariants: compression method 8 (DEFLATE), a window
/// size within the DEFLATE maximum (`CINFO <= 7`), and the mod-31 FCHECK. This
/// is a shape test, never authority: the exact-reconstruction gate decides.
pub fn zlib_header_valid(bytes: &[u8]) -> bool {
    if bytes.len() < ZLIB_HEADER_LEN {
        return false;
    }
    let cmf = bytes[0];
    let flg = bytes[1];
    let method = cmf & 0x0f;
    let cinfo = cmf >> 4;
    method == 8 && cinfo <= 7 && (u16::from(cmf) * 256 + u16::from(flg)).is_multiple_of(31)
}

/// The bounded `preflate` analysis configuration derived from `limits`.
///
/// `plain_text_limit` caps the decompressed size at the smaller of the output
/// and single-record limits, so an admitted plaintext is always storable as one
/// object. `verify_compression` is on: `preflate` internally recompresses and
/// checks, turning many otherwise-silent corruptions into errors.
fn config(limits: Limits) -> PreflateConfig {
    let cap = limits
        .max_output_bytes
        .min(u64::from(limits.max_record_len));
    PreflateConfig {
        max_chain_length: MAX_CHAIN_LENGTH,
        plain_text_limit: cap.min(usize::MAX as u64) as usize,
        verify_compression: true,
    }
}

/// Recreate a raw DEFLATE stream from plaintext plus `preflate` corrections.
///
/// Never panics: a `preflate` panic on malformed corrections is caught and
/// returned as [`crate::ErrorClass::CodecReplay`]. A returned `Ok` is **not**
/// by itself proof of exactness — callers must compare against the source.
pub fn replay_raw(plaintext: &[u8], corrections: &[u8]) -> Result<Vec<u8>> {
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
        recreate_whole_deflate_stream(plaintext, corrections)
    }));
    match outcome {
        Ok(Ok(bytes)) => Ok(bytes),
        Ok(Err(e)) => Err(Error::codec_replay(format!("deflate replay failed: {e}"))),
        Err(_) => Err(Error::codec_replay(
            "deflate replay panicked on malformed correction state",
        )),
    }
}

// ---------------------------------------------------------------------------
// Process-isolated replay (Phase 7.1b, contains fuzz finding F2)
// ---------------------------------------------------------------------------
//
// `preflate` 0.7.6 can allocate unboundedly while reconstructing from a hostile
// correction blob (F2: ~2.5 GiB from a 33-byte input) and offers no bounded
// streaming sink. `catch_unwind` addresses panics, not memory growth. The
// decode path therefore runs reconstruction in a **separate process** whose
// address space is capped with `RLIMIT_AS` (`ulimit -v`) and whose wall-clock
// time is bounded, so a malformed `.vcdoc` cannot amplify memory in the
// decoder. The in-process [`replay_raw`] stays for the encoder's own
// verification and the fuzz targets.

/// Hidden subcommand that runs the isolated replay worker.
///
/// Deliberately not advertised in the CLI usage text: it is an internal
/// rendezvous between a decoding process and its own executable.
pub const REPLAY_WORKER_SUBCOMMAND: &str = "__replay-worker";

/// Largest length-prefixed field the worker will accept from its parent.
///
/// An internal sanity bound, not a wire-format limit: a request larger than
/// this is refused before any allocation, so a corrupt parent can never drive
/// an unbounded read.
pub const REPLAY_WORKER_MAX_FIELD: u32 = 1 << 30;

/// Default wall-clock budget for one isolated replay, in milliseconds.
pub const REPLAY_DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// A decoded worker request: the `(plaintext, corrections, declared_len)` triple.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerRequest {
    /// Decompressed plaintext handed to `preflate`.
    pub plaintext: Vec<u8>,
    /// Opaque `preflate` correction blob.
    pub corrections: Vec<u8>,
    /// The declared raw-DEFLATE length carried through (informational to the
    /// worker; the parent validates it).
    pub declared_len: u32,
}

fn read_field<R: Read>(r: &mut R, max: u32) -> io::Result<Vec<u8>> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len)?;
    let n = u32::from_le_bytes(len);
    if n > max {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("field length {n} exceeds worker bound {max}"),
        ));
    }
    let mut buf = vec![0u8; n as usize];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

/// Read a framed request (little-endian):
/// `[u32 plaintext_len][plaintext][u32 corrections_len][corrections][u32 declared_len]`.
pub fn read_worker_request<R: Read>(r: &mut R) -> io::Result<WorkerRequest> {
    let plaintext = read_field(r, REPLAY_WORKER_MAX_FIELD)?;
    let corrections = read_field(r, REPLAY_WORKER_MAX_FIELD)?;
    let mut declared = [0u8; 4];
    r.read_exact(&mut declared)?;
    Ok(WorkerRequest {
        plaintext,
        corrections,
        declared_len: u32::from_le_bytes(declared),
    })
}

/// Serialize a framed request; the exact inverse of [`read_worker_request`].
pub fn encode_worker_request(
    plaintext: &[u8],
    corrections: &[u8],
    declared_len: u32,
) -> Result<Vec<u8>> {
    let p = u32::try_from(plaintext.len())
        .map_err(|_| Error::codec_replay("replay plaintext exceeds u32 framing"))?;
    let c = u32::try_from(corrections.len())
        .map_err(|_| Error::codec_replay("replay corrections exceed u32 framing"))?;
    let mut buf = Vec::with_capacity(12 + plaintext.len() + corrections.len());
    buf.extend_from_slice(&p.to_le_bytes());
    buf.extend_from_slice(plaintext);
    buf.extend_from_slice(&c.to_le_bytes());
    buf.extend_from_slice(corrections);
    buf.extend_from_slice(&declared_len.to_le_bytes());
    Ok(buf)
}

/// Write a framed reply (little-endian): `[u8 status][u32 payload_len][payload]`.
///
/// `status` is `0` for success (`payload` = raw DEFLATE) and `1` for an error
/// (`payload` = a UTF-8 message).
pub fn write_worker_reply<W: Write>(w: &mut W, status: u8, payload: &[u8]) -> io::Result<()> {
    let n = u32::try_from(payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "worker reply too large"))?;
    w.write_all(&[status])?;
    w.write_all(&n.to_le_bytes())?;
    w.write_all(payload)?;
    w.flush()
}

/// Read a framed reply, bounding the declared payload length.
///
/// `Ok(None)` means the child closed the pipe without sending a reply (a crash
/// or an abort before the reply).
pub fn read_worker_reply<R: Read>(
    r: &mut R,
    max_payload: u64,
) -> io::Result<Option<(u8, Vec<u8>)>> {
    let mut status = [0u8; 1];
    match r.read(&mut status) {
        Ok(0) => return Ok(None),
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let mut len = [0u8; 4];
    r.read_exact(&mut len)?;
    let n = u32::from_le_bytes(len);
    if u64::from(n) > max_payload {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("worker reply length {n} exceeds bound {max_payload}"),
        ));
    }
    let mut payload = vec![0u8; n as usize];
    r.read_exact(&mut payload)?;
    Ok(Some((status[0], payload)))
}

/// Worker entry point: read one framed request from stdin, replay it, write one
/// framed reply to stdout, then exit. **Never returns.**
///
/// A panic inside `preflate` is caught (with a deliberately non-aborting hook)
/// and reported as an error reply, so a preflate panic yields a typed
/// [`crate::ErrorClass::CodecReplay`] at the parent instead of an abort with a
/// lost reply. A true allocation failure under `RLIMIT_AS` still aborts the
/// child, which the parent also treats as failure.
pub fn run_worker_stdio() -> ! {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("replay worker panic (isolated): {info}");
    }));
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut r = stdin.lock();
    let mut w = stdout.lock();
    let code = match read_worker_request(&mut r) {
        Ok(req) => {
            let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
                recreate_whole_deflate_stream(&req.plaintext, &req.corrections)
            }));
            match outcome {
                Ok(Ok(bytes)) => {
                    let _ = write_worker_reply(&mut w, 0, &bytes);
                    0
                }
                Ok(Err(e)) => {
                    let _ = write_worker_reply(
                        &mut w,
                        1,
                        format!("deflate replay failed: {e}").as_bytes(),
                    );
                    1
                }
                Err(_) => {
                    let _ = write_worker_reply(
                        &mut w,
                        1,
                        b"deflate replay panicked on malformed correction state",
                    );
                    1
                }
            }
        }
        Err(e) => {
            let _ = write_worker_reply(
                &mut w,
                1,
                format!("malformed worker request: {e}").as_bytes(),
            );
            2
        }
    };
    std::process::exit(code);
}

/// Cached resolution of the worker executable path.
static WORKER_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
/// A programmatically installed default worker path (see
/// [`install_default_replay_worker`]).
static INSTALLED_WORKER: OnceLock<PathBuf> = OnceLock::new();

/// Install the default replay-worker executable used when `VOLE_REPLAY_WORKER`
/// is unset.
///
/// The CLI calls this with its own `current_exe()` so `decode`/`verify` are
/// isolated by default. It exists because `std::env::set_var` is `unsafe` under
/// Rust 2024 and this crate forbids `unsafe`; an explicit `VOLE_REPLAY_WORKER`
/// environment variable always takes precedence.
pub fn install_default_replay_worker(path: PathBuf) {
    let _ = INSTALLED_WORKER.set(path);
}

fn resolve_worker_path() -> Option<PathBuf> {
    WORKER_PATH
        .get_or_init(|| {
            std::env::var_os("VOLE_REPLAY_WORKER")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .or_else(|| INSTALLED_WORKER.get().cloned())
        })
        .clone()
}

fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok().and_then(|v| v.trim().parse().ok())
}

fn replay_timeout() -> Duration {
    Duration::from_millis(env_u64("VOLE_REPLAY_TIMEOUT_MS").unwrap_or(REPLAY_DEFAULT_TIMEOUT_MS))
}

/// Address-space cap for the worker in KiB, for `ulimit -v`.
fn replay_memory_cap_kb(
    plaintext: &[u8],
    corrections: &[u8],
    declared_len: u32,
    limits: Limits,
) -> u64 {
    if let Some(mb) = env_u64("VOLE_REPLAY_MEM_MB") {
        return mb.saturating_mul(1024);
    }
    let total = plaintext.len() as u64 + corrections.len() as u64 + u64::from(declared_len);
    let bytes = total.saturating_mul(8).saturating_add(64 << 20);
    let lo = 256u64 << 20;
    let hi = limits.max_replay_bytes.min(1u64 << 31).max(1);
    let cap = if hi >= lo { bytes.clamp(lo, hi) } else { hi };
    cap / 1024
}

fn describe_worker_exit(status: std::process::ExitStatus) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            if sig == 6 {
                return "replay worker aborted (SIGABRT); likely exceeded the address-space cap"
                    .to_string();
            }
            return format!("replay worker killed by signal {sig}");
        }
    }
    match status.code() {
        Some(0) => "replay worker exited 0 without a reply".to_string(),
        Some(c) => format!("replay worker exited with status {c}"),
        None => "replay worker terminated abnormally".to_string(),
    }
}

/// Decode-path replay: run `recreate_whole_deflate_stream` in a process-isolated
/// child with an address-space (`RLIMIT_AS`) cap and a wall-clock bound.
///
/// Resolution order for the worker executable:
///
/// 1. the `VOLE_REPLAY_WORKER` environment variable, then
/// 2. a path installed with [`install_default_replay_worker`].
///
/// If neither is set the call **falls back to the in-process [`replay_raw`]**;
/// this keeps the library usable without a worker, at the cost of isolation.
/// When a worker path *is* configured, a spawn failure is a typed
/// [`crate::ErrorClass::CodecReplay`] error and never silently falls back.
///
/// Environment knobs: `VOLE_REPLAY_MEM_MB` overrides the address-space cap
/// (default `clamp((plaintext+corrections+declared) * 8 + 64 MiB, 256 MiB,
/// limits.max_replay_bytes.min(2 GiB))`); `VOLE_REPLAY_TIMEOUT_MS` overrides the
/// wall-clock budget (default 30000 ms).
///
/// The returned payload is the child's reply; the caller still checks its length
/// against the declared output length.
pub fn replay_bounded(
    plaintext: &[u8],
    corrections: &[u8],
    declared_len: u32,
    limits: Limits,
) -> Result<Vec<u8>> {
    let Some(worker) = resolve_worker_path() else {
        return replay_raw(plaintext, corrections);
    };

    let cap_kb = replay_memory_cap_kb(plaintext, corrections, declared_len, limits);
    let timeout = replay_timeout();
    let script = format!(
        "ulimit -v {cap_kb} 2>/dev/null; ulimit -t {} 2>/dev/null; exec \"$0\" {REPLAY_WORKER_SUBCOMMAND}",
        timeout.as_secs().max(1)
    );
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(&script)
        .arg(&worker)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            Error::codec_replay(format!(
                "failed to spawn replay worker {}: {e}",
                worker.display()
            ))
        })?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| Error::codec_replay("replay worker stdin unavailable"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::codec_replay("replay worker stdout unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| Error::codec_replay("replay worker stderr unavailable"))?;

    let request = encode_worker_request(plaintext, corrections, declared_len)?;
    // Drain stderr on a helper thread so a chatty child cannot deadlock its pipe.
    let stderr_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr.take(8192).read_to_end(&mut buf);
        String::from_utf8_lossy(&buf).into_owned()
    });
    // Read the reply concurrently; the pipe closes on child exit, including an
    // abort under `RLIMIT_AS`.
    let max_reply = cap_kb.saturating_mul(1024).max(1);
    let reply_thread = std::thread::spawn(move || read_worker_reply(&mut stdout, max_reply));

    // Send the request. A dead child yields a broken pipe; the status check
    // below turns that into the right failure class.
    let write_err = stdin.write_all(&request).err();
    drop(stdin);

    // Enforce the wall-clock bound by polling; kill on timeout.
    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    timed_out = true;
                    let _ = child.kill();
                    break None;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(_) => {
                let _ = child.kill();
                break None;
            }
        }
    };
    let status = match status {
        Some(status) => Some(status),
        None => child.wait().ok(),
    };
    let reply = reply_thread
        .join()
        .unwrap_or_else(|_| Err(io::Error::other("replay reply reader panicked")));
    let stderr_text = stderr_thread.join().unwrap_or_default();

    if timed_out {
        return Err(Error::codec_replay(format!(
            "replay worker exceeded the {timeout:?} time bound and was killed"
        )));
    }

    match reply {
        Ok(Some((0, payload))) => Ok(payload),
        Ok(Some((1, payload))) => Err(Error::codec_replay(
            String::from_utf8_lossy(&payload).into_owned(),
        )),
        Ok(Some((other, _))) => Err(Error::codec_replay(format!(
            "replay worker returned unknown status {other}"
        ))),
        Ok(None) | Err(_) => {
            let mut msg = String::from("replay worker produced no reply");
            if let Some(status) = status {
                msg.push_str("; ");
                msg.push_str(&describe_worker_exit(status));
            }
            if let Some(e) = write_err {
                msg.push_str(&format!("; request write failed: {e}"));
            }
            if !stderr_text.trim().is_empty() {
                msg.push_str("; stderr: ");
                msg.push_str(stderr_text.trim());
            }
            Err(Error::codec_replay(msg))
        }
    }
}

/// Why [`try_replay_detailed`] declined a stream.
///
/// Diagnostic only: the reason never changes whether a stream is admitted —
/// [`try_replay`] and [`try_replay_detailed`] apply the *same* exact
/// reconstruction gate, and only the carried reason differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayDecline {
    /// Shorter than [`MIN_ZLIB_LEN`].
    TooShort,
    /// Larger than the configured `max_input_bytes`.
    TooLarge,
    /// Not a structurally valid zlib (RFC 1950) header.
    NotZlib,
    /// No raw DEFLATE payload between the header and the Adler-32 trailer.
    EmptyPayload,
    /// The `preflate` analyzer panicked (caught; never escapes).
    AnalyzerPanic,
    /// The `preflate` analyzer returned an error.
    AnalyzerError,
    /// The analyzer did not consume the whole raw payload.
    NotFullyConsumed,
    /// The analyzer reported a non-empty dictionary prefix.
    DictionaryPrefix,
    /// The decompressed plaintext exceeds the storable single-record limit.
    PlaintextTooLarge,
    /// The correction blob exceeds the storable single-record limit.
    CorrectionsTooLarge,
    /// A fresh replay did not reproduce the raw payload byte-for-byte.
    NotReproducible,
}

impl ReplayDecline {
    /// A short, stable snake_case name for diagnostics and JSON output.
    pub fn name(self) -> &'static str {
        match self {
            ReplayDecline::TooShort => "too_short",
            ReplayDecline::TooLarge => "too_large",
            ReplayDecline::NotZlib => "not_zlib",
            ReplayDecline::EmptyPayload => "empty_payload",
            ReplayDecline::AnalyzerPanic => "analyzer_panic",
            ReplayDecline::AnalyzerError => "analyzer_error",
            ReplayDecline::NotFullyConsumed => "not_fully_consumed",
            ReplayDecline::DictionaryPrefix => "dictionary_prefix",
            ReplayDecline::PlaintextTooLarge => "plaintext_too_large",
            ReplayDecline::CorrectionsTooLarge => "corrections_too_large",
            ReplayDecline::NotReproducible => "not_reproducible",
        }
    }
}

/// Attempt an exact replay plan for a zlib-wrapped stream.
///
/// Returns `Some(plan)` only when the plan's replay reproduces the raw DEFLATE
/// payload **byte-for-byte** and every bound is respected. Returns `None`
/// (decline, never panic) for a stream that is not zlib-shaped, too short, not
/// fully consumed by the analyzer, larger than the limits, or not exactly
/// reproducible. Declining is always safe: the caller keeps the raw bytes.
pub fn try_replay(bytes: &[u8], limits: Limits) -> Option<ReplayPlan> {
    try_replay_detailed(bytes, limits).ok()
}

/// The same gate as [`try_replay`], reporting *why* a stream was declined.
pub fn try_replay_detailed(
    bytes: &[u8],
    limits: Limits,
) -> std::result::Result<ReplayPlan, ReplayDecline> {
    let total = bytes.len();
    if total > limits.max_input_bytes as usize {
        return Err(ReplayDecline::TooLarge);
    }
    if total < MIN_ZLIB_LEN {
        return Err(ReplayDecline::TooShort);
    }
    if !zlib_header_valid(bytes) {
        return Err(ReplayDecline::NotZlib);
    }
    let raw = &bytes[ZLIB_HEADER_LEN..total - ZLIB_TRAILER_LEN];
    if raw.is_empty() {
        return Err(ReplayDecline::EmptyPayload);
    }

    // Analysis itself can panic (a debug-build `u16` overflow inside preflate on
    // very large, highly repetitive input), so it is isolated too.
    let analyzed = match panic::catch_unwind(AssertUnwindSafe(|| {
        preflate_whole_deflate_stream(raw, &config(limits))
    })) {
        Ok(Ok(analyzed)) => analyzed,
        Ok(Err(_)) => return Err(ReplayDecline::AnalyzerError),
        Err(_) => return Err(ReplayDecline::AnalyzerPanic),
    };
    let (chunk, plain) = analyzed;

    // Full consumption is mandatory: a too-small `plain_text_limit` can return a
    // silently truncated stream with `compressed_size < raw.len()`.
    if chunk.compressed_size != raw.len() {
        return Err(ReplayDecline::NotFullyConsumed);
    }
    // A whole-stream first chunk has no dictionary prefix; a non-empty prefix
    // would mean the plaintext is not self-contained and we decline rather than
    // guess the concatenation order.
    if !plain.prefix().is_empty() {
        return Err(ReplayDecline::DictionaryPrefix);
    }
    if plaintext_len_exceeds(plain.text(), limits) {
        return Err(ReplayDecline::PlaintextTooLarge);
    }
    if chunk.corrections.len() as u64 > u64::from(limits.max_record_len) {
        return Err(ReplayDecline::CorrectionsTooLarge);
    }

    let plaintext = plain.text().to_vec();
    // The decisive gate: the exact replay must reproduce the raw payload.
    if replay_raw(&plaintext, &chunk.corrections).ok().as_deref() != Some(raw) {
        return Err(ReplayDecline::NotReproducible);
    }

    let raw_len = match u32::try_from(raw.len()) {
        Ok(n) => n,
        Err(_) => return Err(ReplayDecline::TooLarge),
    };
    let mut header = [0u8; ZLIB_HEADER_LEN];
    header.copy_from_slice(&bytes[..ZLIB_HEADER_LEN]);
    let mut adler = [0u8; ZLIB_TRAILER_LEN];
    adler.copy_from_slice(&bytes[total - ZLIB_TRAILER_LEN..]);

    Ok(ReplayPlan {
        header,
        plaintext,
        corrections: chunk.corrections,
        adler,
        raw_len,
    })
}

/// Whether the plaintext exceeds the storable single-record limit.
fn plaintext_len_exceeds(plaintext: &[u8], limits: Limits) -> bool {
    plaintext.len() as u64 > u64::from(limits.max_record_len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use std::io::Write;

    fn zlib(data: &[u8], level: u32) -> Vec<u8> {
        let mut e = ZlibEncoder::new(Vec::new(), Compression::new(level));
        e.write_all(data).unwrap();
        e.finish().unwrap()
    }

    fn content_like() -> Vec<u8> {
        let mut v = Vec::new();
        for i in 0..400 {
            v.extend_from_slice(
                format!(
                    "BT /F1 12 Tf 72 {} Td (Invoice line {i:05} amount 456.78) Tj ET\n",
                    700 - (i % 40)
                )
                .as_bytes(),
            );
        }
        v
    }

    #[test]
    fn zlib_header_shape_is_checked() {
        assert!(zlib_header_valid(&[0x78, 0x9c, 0x00, 0x00, 0x00, 0x00]));
        assert!(zlib_header_valid(&[0x78, 0x01, 0x00, 0x00, 0x00, 0x00]));
        assert!(!zlib_header_valid(&[0x00, 0x00]));
        // Wrong method (3) even though mod-31 holds.
        assert!(!zlib_header_valid(&[0x3b, 0x00, 0x00, 0x00, 0x00, 0x00]));
        // Bad FCHECK.
        assert!(!zlib_header_valid(&[0x78, 0x9d, 0x00, 0x00, 0x00, 0x00]));
        assert!(!zlib_header_valid(&[0x78]));
    }

    #[test]
    fn replay_plan_round_trips_byte_exactly() {
        for (name, data, level) in [
            ("content", content_like(), 6),
            (
                "text",
                b"the quick brown fox jumps over the lazy dog. ".repeat(200),
                9,
            ),
            ("small", b"hello hello hello".to_vec(), 6),
            (
                "incompressible",
                (0..4096u32)
                    .map(|i| (i.wrapping_mul(2654435761) >> 13) as u8)
                    .collect(),
                6,
            ),
        ] {
            let z = zlib(&data, level);
            let plan = try_replay(&z, Limits::DEFAULT)
                .unwrap_or_else(|| panic!("{name} must produce a replay plan"));
            // Reassemble exactly as the DRA op will.
            let mut rebuilt = Vec::new();
            rebuilt.extend_from_slice(&plan.header);
            rebuilt.extend_from_slice(&replay_raw(&plan.plaintext, &plan.corrections).unwrap());
            rebuilt.extend_from_slice(&plan.adler);
            assert_eq!(rebuilt, z, "{name} must replay byte-for-byte");
        }
    }

    #[test]
    fn replay_declines_non_zlib_and_short() {
        assert!(try_replay(b"not zlib at all", Limits::DEFAULT).is_none());
        assert!(try_replay(&[0x78, 0x9c, 0x00, 0x00, 0x00], Limits::DEFAULT).is_none());
        assert!(try_replay(&[], Limits::DEFAULT).is_none());
        assert!(try_replay(&[0x78, 0x9c], Limits::DEFAULT).is_none());
    }

    #[test]
    fn hostile_corrections_never_panic() {
        // `preflate` can panic on malformed correction state; replay_raw must
        // catch it and return a typed error. A caught panic may still print via
        // the default hook — that is log-only and does not affect the result.
        let plain = content_like();
        let mut state = 0x1234_5678_9abc_def0u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..48 {
            let mut blob = vec![0u8; (next() % 96) as usize];
            for b in blob.iter_mut() {
                *b = next() as u8;
            }
            // Must return Ok or Err, never unwind.
            let _ = replay_raw(&plain, &blob);
            let _ = replay_raw(&blob, &plain);
            let _ = replay_raw(&[], &blob);
        }
        assert!(replay_raw(&plain, &[]).is_err() || replay_raw(&plain, &[]).is_ok());
    }

    #[test]
    fn tiny_plaintext_limit_declines_instead_of_truncating() {
        let data = content_like();
        let z = zlib(&data, 6);
        let limits = Limits {
            max_record_len: 8,
            ..Limits::DEFAULT
        };
        // The plaintext cannot fit in 8 bytes, so the plan must decline rather
        // than return a silently truncated stream.
        assert!(try_replay(&z, limits).is_none());
    }

    #[test]
    fn worker_framing_round_trips() {
        let req = encode_worker_request(b"plain", b"corrections", 7).unwrap();
        let mut cur = std::io::Cursor::new(req);
        assert_eq!(
            read_worker_request(&mut cur).unwrap(),
            WorkerRequest {
                plaintext: b"plain".to_vec(),
                corrections: b"corrections".to_vec(),
                declared_len: 7,
            }
        );

        let mut reply = Vec::new();
        write_worker_reply(&mut reply, 0, b"raw deflate").unwrap();
        let mut rc = std::io::Cursor::new(reply);
        assert_eq!(
            read_worker_reply(&mut rc, 1024).unwrap(),
            Some((0, b"raw deflate".to_vec()))
        );

        // An oversized field length is refused before any allocation.
        let mut bad = Vec::new();
        bad.extend_from_slice(&u32::MAX.to_le_bytes());
        let mut bc = std::io::Cursor::new(bad);
        assert!(read_worker_request(&mut bc).is_err());

        // An oversized reply payload length is refused too.
        let mut big_reply = vec![0u8; 5];
        big_reply[1..].copy_from_slice(&u32::MAX.to_le_bytes());
        let mut brc = std::io::Cursor::new(big_reply);
        assert!(read_worker_reply(&mut brc, 16).is_err());

        // A closed pipe means no reply, not an error.
        let mut empty = std::io::Cursor::new(Vec::new());
        assert_eq!(read_worker_reply(&mut empty, 1024).unwrap(), None);
    }
}
