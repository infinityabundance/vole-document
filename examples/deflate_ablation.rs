//! Phase-15.5 DEFLATE-decompression ablation harness (feature `deflate-ablation`).
//!
//! Inflates the **real** compressed members of the frozen `real100-v1` corpus —
//! PDF `FlateDecode` zlib streams and ZIP method-8 raw DEFLATE members — and
//! reports, for **one candidate per process**, decoded-bytes GB/s, correctness
//! (decoded bytes byte-identical to the [`miniz_oxide`] reference) and peak RSS.
//!
//! One candidate per process keeps the caller's `/usr/bin/time -v` peak RSS clean.
//! Candidates:
//!
//! * `miniz` — [`miniz_oxide`] 0.8.9, scalar or SIMD adler depending on the
//!   compile-time `miniz-simd` feature (reported as `config`).
//! * `zlib-rs` — [`zlib_rs`] 0.5.5, safe Rust `Inflate` API only (never the C-ABI
//!   `libz-rs-sys`, which would require `unsafe` FFI).
//! * `zune-inflate` — [`zune_inflate`] 0.2.54.
//!
//! Extraction (scan + slice-copy) happens **before** timing, so GB/s measures only
//! inflate. The reference decode is [`miniz_oxide`] (the currently shipped
//! inflater, in whatever adler configuration this binary was built with); every
//! candidate's output must equal it byte-for-byte over the whole corpus. That
//! reference-vs-candidate equality over the frozen corpus is the byte-identity
//! witness required before any decoder swap.
//!
//! This example adds **no** decoder behavior and changes no `.voldoc` bytes.
//!
//! Usage:
//! ```text
//! cargo run --release --features deflate-ablation --example deflate_ablation -- \
//!   --candidate miniz --corpus real100-v1/documents --limit 10 --reps 3
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use vole_document::Limits;
use vole_document::adapter::package::zip;
use vole_document::adapter::pdf::{FilterClass, physical};

/// The compression wrapper of a member, which selects the candidate entry point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A PDF `FlateDecode` stream: a zlib (RFC 1950) wrapper over DEFLATE.
    PdfZlib,
    /// A ZIP method-8 member: bare DEFLATE (RFC 1951), no checksum.
    ZipDeflate,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::PdfZlib => "pdf-zlib",
            Kind::ZipDeflate => "zip-deflate",
        }
    }
}

/// One extracted compressed member plus its reference decode.
struct Member {
    kind: Kind,
    /// Byte offset of the payload within its document (for the raw table).
    offset: u64,
    /// The raw compressed payload (the exact leaf for this member).
    compressed: Vec<u8>,
    /// The declared uncompressed size, when the container states one (ZIP).
    declared_len: Option<u64>,
    /// The `miniz_oxide` reference decode; the correctness oracle.
    reference: Vec<u8>,
}

/// A resolved candidate backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Candidate {
    Miniz,
    ZlibRs,
    ZuneInflate,
}

impl Candidate {
    fn parse(s: &str) -> Option<Candidate> {
        match s {
            "miniz" => Some(Candidate::Miniz),
            "zlib-rs" => Some(Candidate::ZlibRs),
            "zune-inflate" => Some(Candidate::ZuneInflate),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Candidate::Miniz => "miniz",
            Candidate::ZlibRs => "zlib-rs",
            Candidate::ZuneInflate => "zune-inflate",
        }
    }
}

/// Parsed command line.
struct Args {
    candidate: Candidate,
    corpus: PathBuf,
    manifest: Option<PathBuf>,
    limit: Option<usize>,
    reps: usize,
    member_cap: u64,
    out: Option<PathBuf>,
}

fn usage() -> String {
    "usage: deflate_ablation --candidate <miniz|zlib-rs|zune-inflate> \
     [--corpus <dir>] [--manifest <tsv>] [--limit N] [--reps R] \
     [--member-cap BYTES] [--out <jsonl>]"
        .to_string()
}

fn parse_args() -> Result<Args, String> {
    let mut candidate: Option<Candidate> = None;
    let mut corpus = PathBuf::from("real100-v1/documents");
    let mut manifest = None;
    let mut limit = None;
    let mut reps = 3usize;
    let mut member_cap = 256u64 * 1024 * 1024;
    let mut out = None;

    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut value = |flag: &str| -> Result<String, String> {
            it.next().ok_or_else(|| format!("{flag} needs a value"))
        };
        match a.as_str() {
            "--candidate" => {
                let v = value("--candidate")?;
                candidate = Some(
                    Candidate::parse(&v)
                        .ok_or_else(|| format!("unknown candidate {v:?} ({} )", usage()))?,
                );
            }
            "--corpus" => corpus = PathBuf::from(value("--corpus")?),
            "--manifest" => manifest = Some(PathBuf::from(value("--manifest")?)),
            "--limit" => {
                limit = Some(
                    value("--limit")?
                        .parse()
                        .map_err(|_| "--limit must be an integer".to_string())?,
                );
            }
            "--reps" => {
                reps = value("--reps")?
                    .parse()
                    .map_err(|_| "--reps must be an integer".to_string())?;
                if reps == 0 {
                    reps = 1;
                }
            }
            "--member-cap" => {
                member_cap = value("--member-cap")?
                    .parse()
                    .map_err(|_| "--member-cap must be an integer".to_string())?;
            }
            "--out" => out = Some(PathBuf::from(value("--out")?)),
            "-h" | "--help" => return Err(usage()),
            other => return Err(format!("unknown argument {other:?}\n{}", usage())),
        }
    }

    Ok(Args {
        candidate: candidate.ok_or_else(|| format!("--candidate is required\n{}", usage()))?,
        corpus,
        manifest,
        limit,
        reps,
        member_cap,
        out,
    })
}

// ---------------------------------------------------------------------------
// Corpus discovery
// ---------------------------------------------------------------------------

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            collect_files(&p, out);
        } else if p.is_file() {
            out.push(p);
        }
    }
}

/// Parse `manifest.tsv` into an `id -> format` map (the `format` column is
/// advisory; the format actually used is sniffed from the bytes).
fn read_manifest(path: &Path) -> Result<HashMap<String, String>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("read manifest: {e}"))?;
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().unwrap_or("").split('\t').collect();
    let id_idx = header
        .iter()
        .position(|h| *h == "id")
        .ok_or("manifest has no `id` column")?;
    let fmt_idx = header
        .iter()
        .position(|h| *h == "format")
        .ok_or("manifest has no `format` column")?;
    let mut map = HashMap::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if let (Some(id), Some(fmt)) = (cols.get(id_idx), cols.get(fmt_idx)) {
            map.insert((*id).to_string(), (*fmt).to_string());
        }
    }
    Ok(map)
}

/// Sniff the container format by bytes (never by extension): `%PDF-` within the
/// first 1024 bytes, else a ZIP local-file/EOCD signature.
fn sniff_format(bytes: &[u8]) -> Option<&'static str> {
    let head = &bytes[..bytes.len().min(1024)];
    if head.windows(5).any(|w| w == b"%PDF-") {
        return Some("pdf");
    }
    if bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06") {
        return Some("zip");
    }
    None
}

// ---------------------------------------------------------------------------
// Member extraction (uses the shipped extractors; never re-implements scanning)
// ---------------------------------------------------------------------------

/// Extract every decodable compressed member of one document and compute its
/// `miniz_oxide` reference decode. Members whose reference exceeds `cap` (or
/// fail to decode) are declined and returned as a count.
fn extract_members(bytes: &[u8], fmt: &str, cap: u64) -> (Vec<Member>, u64, u64) {
    let mut members = Vec::new();
    let mut declined = 0u64;
    let mut scan_failed = 0u64;
    let cap_usize = usize::try_from(cap).unwrap_or(usize::MAX);

    match fmt {
        "pdf" => {
            let Ok(physical) = physical::scan(bytes, Limits::DEFAULT) else {
                scan_failed += 1;
                return (members, declined, scan_failed);
            };
            for s in &physical.streams {
                if s.filter != FilterClass::FlateDecode {
                    continue;
                }
                let Some(payload) = slice_at(bytes, s.data_start, s.data_len) else {
                    declined += 1;
                    continue;
                };
                if payload.len() > cap_usize {
                    declined += 1;
                    continue;
                }
                match decode(Candidate::Miniz, Kind::PdfZlib, payload, cap_usize) {
                    Ok(reference) if reference.len() as u64 <= cap => members.push(Member {
                        kind: Kind::PdfZlib,
                        offset: s.data_start,
                        compressed: payload.to_vec(),
                        declared_len: None,
                        reference,
                    }),
                    _ => declined += 1,
                }
            }
        }
        "zip" => {
            let Ok(zip) = zip::scan(bytes, Limits::DEFAULT) else {
                scan_failed += 1;
                return (members, declined, scan_failed);
            };
            for m in &zip.members {
                if m.method != 8 {
                    continue;
                }
                if m.uncompressed_size > cap {
                    declined += 1;
                    continue;
                }
                let Some(payload) = zip.member_raw(m, bytes).ok() else {
                    declined += 1;
                    continue;
                };
                if payload.len() > cap_usize {
                    declined += 1;
                    continue;
                }
                match decode(Candidate::Miniz, Kind::ZipDeflate, payload, cap_usize) {
                    Ok(reference)
                        if reference.len() as u64 == m.uncompressed_size
                            && reference.len() as u64 <= cap =>
                    {
                        members.push(Member {
                            kind: Kind::ZipDeflate,
                            offset: m.data.0,
                            compressed: payload.to_vec(),
                            declared_len: Some(m.uncompressed_size),
                            reference,
                        });
                    }
                    _ => declined += 1,
                }
            }
        }
        _ => {}
    }

    (members, declined, scan_failed)
}

fn slice_at(bytes: &[u8], start: u64, len: u64) -> Option<&[u8]> {
    let start = usize::try_from(start).ok()?;
    let len = usize::try_from(len).ok()?;
    bytes.get(start..start.checked_add(len)?)
}

// ---------------------------------------------------------------------------
// Candidate backends (safe Rust APIs only)
// ---------------------------------------------------------------------------

/// Decode `input` with the given candidate, bounded to `cap` output bytes.
///
/// `cap` is the exact reference length during timing/correctness, so a decode
/// that would exceed it is an error (a divergence), never a silent truncation.
fn decode(candidate: Candidate, kind: Kind, input: &[u8], cap: usize) -> Result<Vec<u8>, String> {
    match candidate {
        Candidate::Miniz => decode_miniz(kind, input, cap),
        Candidate::ZlibRs => decode_zlib_rs(kind, input, cap),
        Candidate::ZuneInflate => decode_zune(kind, input, cap),
    }
}

fn decode_miniz(kind: Kind, input: &[u8], cap: usize) -> Result<Vec<u8>, String> {
    let r = match kind {
        Kind::PdfZlib => miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(input, cap),
        Kind::ZipDeflate => miniz_oxide::inflate::decompress_to_vec_with_limit(input, cap),
    };
    r.map_err(|e| format!("miniz {:?}", e.status))
}

/// `zlib-rs` safe Rust API. We allocate exactly `cap` (the known output length)
/// and require a clean `StreamEnd`; `libz-rs-sys` (the C-ABI shim) is never used.
fn decode_zlib_rs(kind: Kind, input: &[u8], cap: usize) -> Result<Vec<u8>, String> {
    let zlib_header = kind == Kind::PdfZlib;
    // A zero-length member still needs one output byte to reach `StreamEnd`.
    let mut out = vec![0u8; cap.max(1)];
    let mut inflate = zlib_rs::Inflate::new(zlib_header, 15);
    match inflate.decompress(input, &mut out, zlib_rs::InflateFlush::Finish) {
        Ok(zlib_rs::Status::StreamEnd) => {
            let n = usize::try_from(inflate.total_out()).map_err(|_| "total_out overflow")?;
            out.truncate(n);
            Ok(out)
        }
        Ok(status) => Err(format!("zlib-rs status {status:?}")),
        Err(e) => Err(format!("zlib-rs {e:?}")),
    }
}

fn decode_zune(kind: Kind, input: &[u8], cap: usize) -> Result<Vec<u8>, String> {
    let options = zune_inflate::DeflateOptions::default().set_limit(cap);
    let mut dec = zune_inflate::DeflateDecoder::new_with_options(input, options);
    match kind {
        Kind::PdfZlib => dec.decode_zlib(),
        Kind::ZipDeflate => dec.decode_deflate(),
    }
    .map_err(|e| format!("zune-inflate {e:?}"))
}

// ---------------------------------------------------------------------------
// Reporting helpers
// ---------------------------------------------------------------------------

fn jesc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

/// Peak resident set size (`VmHWM`) in KiB from `/proc/self/status`, when the
/// platform exposes it. This is the process-wide peak; the caller's
/// `/usr/bin/time -v` reports the same quantity independently.
fn peak_rss_kib() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            let kib = rest.trim().trim_end_matches("kB").trim();
            return kib.parse().ok();
        }
    }
    None
}

fn median(mut ts: Vec<Duration>) -> Duration {
    ts.sort();
    ts[ts.len() / 2]
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

/// The first differing byte index of two slices, when they differ in length or
/// content.
fn first_diff(a: &[u8], b: &[u8]) -> Option<usize> {
    let n = a.len().min(b.len());
    for i in 0..n {
        if a[i] != b[i] {
            return Some(i);
        }
    }
    (a.len() != b.len()).then_some(n)
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> Result<(), String> {
    let args = parse_args()?;
    let manifest = match &args.manifest {
        Some(p) => read_manifest(p)?,
        None => HashMap::new(),
    };

    let mut files = Vec::new();
    collect_files(&args.corpus, &mut files);
    // Restrict to manifest ids when a manifest is given (the file stem is the id).
    if !manifest.is_empty() {
        files.retain(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| manifest.contains_key(s))
        });
    }
    if let Some(limit) = args.limit {
        files.truncate(limit);
    }
    if files.is_empty() {
        return Err(format!(
            "no documents found under {}",
            args.corpus.display()
        ));
    }

    let config = if cfg!(feature = "miniz-simd") {
        "simd"
    } else {
        "scalar"
    };

    let mut out_rows = String::new();
    let mut total_members = 0u64;
    let mut total_compressed = 0u64;
    let mut total_decoded = 0u64;
    let mut total_declined = 0u64;
    let mut total_scan_failed = 0u64;
    let mut total_wall = Duration::ZERO;
    let mut docs = 0u64;
    let mut mismatches = 0u64;
    let mut first_mismatch: Option<String> = None;

    for path in &files {
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("skip {}: {e}", path.display());
                continue;
            }
        };
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("<unnamed>")
            .to_string();
        let fmt = match sniff_format(&bytes) {
            Some(f) => f,
            None => {
                total_scan_failed += 1;
                continue;
            }
        };
        let manifest_format = manifest.get(&id).map(String::as_str).unwrap_or("");
        let (members, declined, scan_failed) = extract_members(&bytes, fmt, args.member_cap);
        total_declined += declined;
        total_scan_failed += scan_failed;
        drop(bytes);

        if members.is_empty() {
            continue;
        }

        // Correctness gate (uncounted, and a warm run for the timing pass).
        let mut doc_mismatch = 0u64;
        let mut doc_decoded = 0u64;
        let mut doc_compressed = 0u64;
        for m in &members {
            let expected = m.reference.len();
            doc_decoded += expected as u64;
            doc_compressed += m.compressed.len() as u64;
            match decode(args.candidate, m.kind, &m.compressed, expected) {
                Ok(got) => {
                    if let Some(d) = first_diff(&got, &m.reference) {
                        doc_mismatch += 1;
                        if first_mismatch.is_none() {
                            first_mismatch = Some(format!(
                                "{{\"doc\":\"{}\",\"kind\":\"{}\",\"offset\":{},\"first_diff_byte\":{}}}",
                                jesc(&id),
                                m.kind.as_str(),
                                m.offset,
                                d
                            ));
                        }
                    }
                    if let Some(declared) = m.declared_len
                        && declared != expected as u64
                    {
                        doc_mismatch += 1;
                    }
                }
                Err(e) => {
                    doc_mismatch += 1;
                    if first_mismatch.is_none() {
                        first_mismatch = Some(format!(
                            "{{\"doc\":\"{}\",\"kind\":\"{}\",\"offset\":{},\"error\":\"{}\"}}",
                            jesc(&id),
                            m.kind.as_str(),
                            m.offset,
                            jesc(&e)
                        ));
                    }
                }
            }
        }
        mismatches += doc_mismatch;

        // Timed passes: inflate every member of this document `reps` times.
        let mut times = Vec::with_capacity(args.reps);
        for _ in 0..args.reps {
            let t0 = Instant::now();
            for m in &members {
                let expected = m.reference.len();
                let got = decode(args.candidate, m.kind, &m.compressed, expected);
                std::hint::black_box(&got);
            }
            times.push(t0.elapsed());
        }
        let wall = median(times);
        let gbps = if wall.as_secs_f64() > 0.0 {
            (doc_decoded as f64 / 1e9) / wall.as_secs_f64()
        } else {
            0.0
        };
        out_rows.push_str(&format!(
            "{{\"candidate\":\"{}\",\"config\":\"{}\",\"doc\":\"{}\",\"format\":\"{}\",\
             \"manifest_format\":\"{}\",\"members\":{},\"compressed_bytes\":{},\
             \"decoded_bytes\":{},\"wall_ms\":{:.3},\"gbps_decoded\":{:.4},\
             \"declined\":{},\"mismatches\":{}}}\n",
            args.candidate.name(),
            config,
            jesc(&id),
            fmt,
            jesc(manifest_format),
            members.len(),
            doc_compressed,
            doc_decoded,
            ms(wall),
            gbps,
            declined,
            doc_mismatch,
        ));

        docs += 1;
        total_members += members.len() as u64;
        total_compressed += doc_compressed;
        total_decoded += doc_decoded;
        total_wall += wall;
    }

    if let Some(out) = &args.out {
        fs::write(out, &out_rows).map_err(|e| format!("write --out: {e}"))?;
    }

    let total_gbps = if total_wall.as_secs_f64() > 0.0 {
        (total_decoded as f64 / 1e9) / total_wall.as_secs_f64()
    } else {
        0.0
    };
    let rss = peak_rss_kib()
        .map(|k| k.to_string())
        .unwrap_or_else(|| "null".to_string());
    let fm = first_mismatch.unwrap_or_else(|| "null".to_string());

    println!(
        "{{\"candidate\":\"{}\",\"config\":\"{}\",\"docs\":{},\"members\":{},\
         \"compressed_bytes\":{},\"decoded_bytes\":{},\"wall_ms\":{:.3},\
         \"gbps_decoded\":{:.4},\"correct\":{},\"mismatches\":{},\"declined\":{},\
         \"scan_failed\":{},\"peak_rss_kib\":{},\"first_mismatch\":{}}}",
        args.candidate.name(),
        config,
        docs,
        total_members,
        total_compressed,
        total_decoded,
        ms(total_wall),
        total_gbps,
        mismatches == 0,
        mismatches,
        total_declined,
        total_scan_failed,
        rss,
        fm,
    );

    eprintln!(
        "deflate-ablation candidate={} config={} docs={} members={} \
         decoded={}B gbps={:.3} mismatches={} declined={} peak_rss={}KiB",
        args.candidate.name(),
        config,
        docs,
        total_members,
        total_decoded,
        total_gbps,
        mismatches,
        total_declined,
        rss,
    );

    if mismatches != 0 {
        return Err(format!(
            "candidate {} failed the correctness gate ({mismatches} mismatching members)",
            args.candidate.name()
        ));
    }
    Ok(())
}
