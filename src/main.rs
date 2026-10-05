//! `vole-document` command-line interface.
//!
//! File output is written atomically: a temporary sibling is written, fsynced,
//! then renamed into place. A partial or failed operation never replaces the
//! destination.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use vole_document::adapter::pdf;
use vole_document::container::UNIVERSE;
use vole_document::dra::Op;
use vole_document::encode::candidates::CandidateKind;
use vole_document::error::{Error, Result};
use vole_document::limits::Limits;
use vole_document::{encode, integrity, materialize};

const USAGE_HEAD: &str = "\
vole-document — byte-exact procedural document storage

USAGE:
    vole-document encode [--force KIND] INPUT  OUTPUT.voldoc
    vole-document decode     INPUT.voldoc     OUTPUT
    vole-document materialize INPUT.voldoc    OUTPUT
    vole-document verify     INPUT.voldoc
    vole-document inspect    INPUT.voldoc
    vole-document pdf-inspect INPUT
    vole-document pdf-make-samples DIR
";

/// The `deflate-stats` line is advertised only when the replay stack is built in.
#[cfg(feature = "deflate-replay")]
const USAGE_DEFLATE_STATS: &str = "    vole-document deflate-stats INPUT...\n";
#[cfg(not(feature = "deflate-replay"))]
const USAGE_DEFLATE_STATS: &str = "";

const USAGE_TAIL: &str = "\
    vole-document capabilities

KIND (for encode --force): raw | rle | byte-rans | pdf-physical | pdf-channels |
    pdf-layout | pdf-layout-rans | pdf-deflate-replay | pdf-deflate-replay-rans
    Forces the complete-cost court to consider only that candidate family, for
    honest per-mechanism ablation. Fails when the input does not propose it.

EXIT CODES:
    0 ok   2 usage   3 io   4 invalid-container   5 unsupported-version
    6 unsupported-feature   7 integrity-mismatch   8 resource-limit
    9 invalid-graph  10 invalid-model  15 coverage-violation
    16 reconstruction-mismatch  70 internal-invariant
";

/// The full usage text, with the replay-gated command line included only when
/// the feature is present.
fn usage() -> String {
    format!("{USAGE_HEAD}{USAGE_DEFLATE_STATS}{USAGE_TAIL}")
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            let code = e.exit_code();
            ExitCode::from(u8::try_from(code).unwrap_or(70))
        }
    }
}

fn run(args: &[String]) -> Result<()> {
    // Hidden internal replay-worker mode: handled before any normal parsing,
    // because it reads a framed request from stdin and never returns. It is not
    // advertised in USAGE.
    #[cfg(feature = "deflate-replay")]
    {
        use vole_document::codec::deflate::{
            REPLAY_WORKER_SUBCOMMAND, install_default_replay_worker, run_worker_stdio,
        };
        if args.get(1).map(String::as_str) == Some(REPLAY_WORKER_SUBCOMMAND) {
            run_worker_stdio();
        }
        // Isolate DEFLATE replay by default: point the library at this binary.
        // `std::env::set_var` is `unsafe` under Rust 2024 (and this crate forbids
        // `unsafe`), so the path is installed via a safe library setter; an
        // explicit `VOLE_REPLAY_WORKER` still takes precedence.
        if std::env::var_os("VOLE_REPLAY_WORKER").is_none() {
            if let Ok(exe) = std::env::current_exe() {
                install_default_replay_worker(exe);
            }
        }
    }

    let cmd = match args.get(1).map(String::as_str) {
        Some(c) => c,
        None => {
            print!("{}", usage());
            return Err(Error::usage("no subcommand given"));
        }
    };
    let limits = Limits::DEFAULT;

    match cmd {
        "-h" | "--help" | "help" => {
            print!("{}", usage());
            Ok(())
        }
        "capabilities" => cmd_capabilities(),
        "encode" => cmd_encode_args(args, limits),
        "decode" | "materialize" => {
            let input = arg(args, 2, "INPUT.voldoc")?;
            let output = arg(args, 3, "OUTPUT")?;
            cmd_decode(&input, &output, limits)
        }
        "verify" => {
            let input = arg(args, 2, "INPUT.voldoc")?;
            cmd_verify(&input, limits)
        }
        "inspect" => {
            let input = arg(args, 2, "INPUT.voldoc")?;
            cmd_inspect(&input, limits)
        }
        "pdf-inspect" => {
            let input = arg(args, 2, "INPUT")?;
            cmd_pdf_inspect(&input, limits)
        }
        "pdf-make-samples" => {
            let dir = arg(args, 2, "DIR")?;
            cmd_pdf_make_samples(&dir)
        }
        #[cfg(feature = "deflate-replay")]
        "deflate-stats" => {
            let inputs: Vec<PathBuf> = args
                .get(2..)
                .unwrap_or(&[])
                .iter()
                .map(PathBuf::from)
                .collect();
            if inputs.is_empty() {
                return Err(Error::usage(
                    "deflate-stats requires at least one INPUT (a PDF)",
                ));
            }
            cmd_deflate_stats(&inputs, limits)
        }
        other => Err(Error::usage(format!(
            "unknown subcommand {other:?}\n\n{}",
            usage()
        ))),
    }
}

fn arg(args: &[String], idx: usize, name: &str) -> Result<PathBuf> {
    args.get(idx)
        .map(PathBuf::from)
        .ok_or_else(|| Error::usage(format!("missing argument {name}")))
}

fn cmd_encode(
    input: &Path,
    output: &Path,
    limits: Limits,
    force: Option<CandidateKind>,
) -> Result<()> {
    let source = fs::read(input)?;
    if source.len() as u64 > limits.max_input_bytes {
        return Err(Error::resource_limit(
            "input exceeds configured input limit",
        ));
    }
    let (bytes, report) = encode::encode_with(&source, limits, force)?;
    write_atomic(output, &bytes)?;

    println!("{}", report_json(&report));
    Ok(())
}

/// Parse the `encode` subcommand arguments: an optional `--force KIND` flag and
/// the two positional paths `INPUT` and `OUTPUT.voldoc`.
///
/// The flag accepts either `--force KIND` or `--force=KIND` and may appear
/// before or after the positionals, so existing `encode IN OUT` invocations are
/// unchanged.
fn cmd_encode_args(args: &[String], limits: Limits) -> Result<()> {
    let mut force: Option<CandidateKind> = None;
    let mut positional: Vec<&str> = Vec::new();
    let mut i = 2;
    while i < args.len() {
        let a = args[i].as_str();
        if a == "--force" {
            let kind = args
                .get(i + 1)
                .ok_or_else(|| Error::usage("--force requires a KIND argument"))?;
            force = Some(parse_force_kind(kind)?);
            i += 2;
        } else if let Some(kind) = a.strip_prefix("--force=") {
            force = Some(parse_force_kind(kind)?);
            i += 1;
        } else {
            positional.push(a);
            i += 1;
        }
    }
    let input = positional
        .first()
        .map(PathBuf::from)
        .ok_or_else(|| Error::usage("missing argument INPUT"))?;
    let output = positional
        .get(1)
        .map(PathBuf::from)
        .ok_or_else(|| Error::usage("missing argument OUTPUT.voldoc"))?;
    if positional.len() > 2 {
        return Err(Error::usage(format!(
            "unexpected extra argument {:?}",
            positional[2]
        )));
    }
    cmd_encode(&input, &output, limits, force)
}

/// Map a `--force` KIND spelling to a [`CandidateKind`].
fn parse_force_kind(s: &str) -> Result<CandidateKind> {
    match s {
        "raw" => Ok(CandidateKind::Raw),
        "rle" => Ok(CandidateKind::Rle),
        "byte-rans" => Ok(CandidateKind::ByteRans),
        "pdf-physical" => Ok(CandidateKind::PdfPhysical),
        "pdf-channels" => Ok(CandidateKind::PdfChannels),
        "pdf-layout" => Ok(CandidateKind::PdfLayout),
        "pdf-layout-rans" => Ok(CandidateKind::PdfLayoutRans),
        "pdf-deflate-replay" => Ok(CandidateKind::PdfDeflateReplay),
        "pdf-deflate-replay-rans" => Ok(CandidateKind::PdfDeflateReplayRans),
        other => Err(Error::usage(format!(
            "unknown --force kind {other:?}; expected one of raw, rle, byte-rans, pdf-physical, pdf-channels, pdf-layout, pdf-layout-rans, pdf-deflate-replay, pdf-deflate-replay-rans"
        ))),
    }
}

fn cmd_decode(input: &Path, output: &Path, limits: Limits) -> Result<()> {
    let encoded = fs::read(input)?;
    let (bytes, parsed) = materialize::decode_to_bytes(&encoded, limits)?;
    write_atomic(output, &bytes)?;
    println!(
        "{{\"ok\":true,\"source_len\":{},\"sha256\":\"{}\",\"graph_ops\":{},\"objects\":{}}}",
        bytes.len(),
        integrity::to_hex(&parsed.descriptor.source_sha256),
        parsed.descriptor.program.ops.len(),
        parsed.descriptor.objects.len()
    );
    Ok(())
}

fn cmd_verify(input: &Path, limits: Limits) -> Result<()> {
    let encoded = fs::read(input)?;
    let report = materialize::verify(&encoded, limits)?;
    println!(
        "{{\"ok\":true,\"source_len\":{},\"sha256\":\"{}\",\"objects\":{},\"graph_ops\":{}}}",
        report.source_len, report.sha256_hex, report.object_count, report.graph_ops
    );
    Ok(())
}

fn cmd_inspect(input: &Path, limits: Limits) -> Result<()> {
    let encoded = fs::read(input)?;
    let parsed = vole_document::container::Descriptor::parse(&encoded, limits)?;
    let d = &parsed.descriptor;
    let ops: Vec<String> = d.program.ops.iter().map(describe_op).collect();
    let ops_json = ops
        .iter()
        .map(|s| format!("\"{s}\""))
        .collect::<Vec<_>>()
        .join(",");
    let object_lens: Vec<String> = d.objects.iter().map(|o| o.len().to_string()).collect();
    println!(
        concat!(
            "{{",
            "\"universe\":\"{}\",",
            "\"universe_id\":\"{}\",",
            "\"format_version\":\"{}.{}\",",
            "\"exactness_profile\":\"EXACT_BYTES\",",
            "\"source_format\":{},",
            "\"format_basis\":\"{}\",",
            "\"source_len\":{},",
            "\"source_sha256\":\"{}\",",
            "\"object_count\":{},",
            "\"object_lens\":[{}],",
            "\"graph_ops\":{},",
            "\"program\":[{}],",
            "\"encoded_len\":{},",
            "\"cost\":{}",
            "}}"
        ),
        d.universe,
        integrity::to_hex(&parsed.universe_id),
        vole_document::container::header::FORMAT_MAJOR,
        vole_document::container::header::FORMAT_MINOR,
        d.source_format,
        d.format_basis,
        d.source_len,
        integrity::to_hex(&d.source_sha256),
        d.objects.len(),
        object_lens.join(","),
        d.program.ops.len(),
        ops_json,
        encoded.len(),
        parsed.cost.to_json(),
    );
    Ok(())
}

/// Machine-readable structural view of a PDF input for the differential oracle.
///
/// Emits one JSON line. `is_pdf` mirrors [`pdf::detect`]: a `%PDF-` header, at
/// least one indirect object, and at least one `%%EOF`. Non-PDFs are reported
/// with `"is_pdf":false` and exit 0; a real scan failure (I/O, resource limit,
/// coverage) is returned as a typed error with a nonzero exit code.
fn cmd_pdf_inspect(input: &Path, limits: Limits) -> Result<()> {
    let bytes = fs::read(input)?;
    let is_pdf = pdf::detect(&bytes, limits);
    let physical = pdf::scan(&bytes, limits)?;

    let objects: Vec<String> = physical
        .objects
        .iter()
        .map(|o| {
            format!(
                "{{\"number\":{},\"generation\":{},\"role\":\"{}\"}}",
                o.number,
                o.generation,
                role_name(o.role)
            )
        })
        .collect();
    let startxref: Vec<String> = physical.startxref.iter().map(u64::to_string).collect();

    println!(
        concat!(
            "{{",
            "\"file\":\"{}\",",
            "\"is_pdf\":{},",
            "\"span_count\":{},",
            "\"object_count\":{},",
            "\"objects\":[{}],",
            "\"revision_count\":{},",
            "\"startxref\":[{}],",
            "\"eof_count\":{}",
            "}}"
        ),
        json_escape(&input.display().to_string()),
        is_pdf,
        physical.spans.len(),
        physical.objects.len(),
        objects.join(","),
        physical.revisions.len(),
        startxref.join(","),
        physical.eofs.len(),
    );
    Ok(())
}

/// Write the deterministic Phase-3 sample corpus to `dir`, one file per entry.
///
/// Every sample is assembled with correct `/Length` and `startxref` by
/// construction; see [`pdf::samples`]. Writing is atomic per file so a failed
/// run cannot leave a partially written sample in place.
fn cmd_pdf_make_samples(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    let samples = pdf::samples::sample_pdfs();
    let mut names: Vec<String> = Vec::with_capacity(samples.len());
    for (name, bytes) in &samples {
        write_atomic(&dir.join(name), bytes)?;
        names.push(format!("\"{}\"", json_escape(name)));
    }
    println!(
        "{{\"ok\":true,\"dir\":\"{}\",\"count\":{},\"samples\":[{}]}}",
        json_escape(&dir.display().to_string()),
        names.len(),
        names.join(",")
    );
    Ok(())
}

/// Machine-readable exact-DEFLATE-replay correction-ratio view of each input.
///
/// Emits one JSON line per input: per-`FlateDecode`-stream `compressed_bytes`,
/// `plaintext_bytes`, `correction_bytes`, `rans_plaintext_bytes`, the ratios
/// `correction/compressed`, `(plaintext+correction)/compressed`, and
/// `(rans_plaintext+correction)/compressed`, plus the aggregate. A non-PDF is
/// reported with `"is_pdf":false` and no streams (exit 0). Ratios are rendered
/// as fixed-point decimals computed with integer arithmetic only; they are
/// diagnostics and never a persisted representation.
///
/// Two aggregate complete-cost figures are reported so shared plaintext is not
/// overcounted: `replayed_rans_full_bytes` is the **naive per-stream** sum
/// (each stream pays its own rANS plaintext cost plus its own correction), while
/// `replayed_rans_dedup_bytes` charges each **unique** plaintext (rANS) and each
/// **unique** correction blob once, mirroring what the shared-channel
/// `PDF_DEFLATE_REPLAY_RANS` candidate actually stores. The deduped figure is
/// therefore ≤ the naive one, with equality exactly when no plaintext or
/// correction repeats.
#[cfg(feature = "deflate-replay")]
fn cmd_deflate_stats(inputs: &[PathBuf], limits: Limits) -> Result<()> {
    for input in inputs {
        let bytes = fs::read(input)?;
        let stats = pdf::deflate_stats(&bytes, limits)?;
        println!("{}", deflate_stats_json(input, &stats));
    }
    Ok(())
}

#[cfg(feature = "deflate-replay")]
fn deflate_stats_json(input: &Path, stats: &pdf::DeflateStats) -> String {
    let streams: Vec<String> = stats.streams.iter().map(stream_stats_json).collect();
    let s = &stats.summary;
    let replayed_full = s.plaintext_bytes.saturating_add(s.correction_bytes);
    let replayed_rans_full = s.rans_plaintext_bytes.saturating_add(s.correction_bytes);
    let replayed_rans_dedup = s.replayed_rans_dedup_bytes;
    format!(
        concat!(
            "{{",
            "\"file\":\"{}\",",
            "\"is_pdf\":{},",
            "\"streams\":[{}],",
            "\"summary\":{{",
            "\"flate_streams\":{},",
            "\"replayed\":{},",
            "\"declined\":{},",
            "\"compressed_bytes\":{},",
            "\"plaintext_bytes\":{},",
            "\"correction_bytes\":{},",
            "\"rans_plaintext_bytes\":{},",
            "\"correction_over_compressed\":{},",
            "\"replayed_full_bytes\":{},",
            "\"replayed_rans_full_bytes\":{},",
            "\"replayed_rans_dedup_bytes\":{}",
            "}}",
            "}}"
        ),
        json_escape(&input.display().to_string()),
        stats.is_pdf,
        streams.join(","),
        s.flate_streams,
        s.replayed,
        s.declined,
        s.compressed_bytes,
        s.plaintext_bytes,
        s.correction_bytes,
        s.rans_plaintext_bytes,
        ratio6(s.correction_bytes, s.compressed_bytes),
        replayed_full,
        replayed_rans_full,
        replayed_rans_dedup,
    )
}

#[cfg(feature = "deflate-replay")]
fn stream_stats_json(s: &pdf::StreamStats) -> String {
    let reason = match s.decline_reason {
        Some(r) => format!("\"{r}\""),
        None => "null".to_string(),
    };
    let raw_ratio = match s.correction_bytes {
        Some(c) => ratio6(c, s.compressed_bytes),
        None => "null".to_string(),
    };
    let plain_ratio = match (s.plaintext_bytes, s.correction_bytes) {
        (Some(p), Some(c)) => ratio6(p.saturating_add(c), s.compressed_bytes),
        _ => "null".to_string(),
    };
    let rans_ratio = match (s.rans_plaintext_bytes, s.correction_bytes) {
        (Some(r), Some(c)) => ratio6(r.saturating_add(c), s.compressed_bytes),
        _ => "null".to_string(),
    };
    format!(
        concat!(
            "{{",
            "\"object\":{},",
            "\"generation\":{},",
            "\"compressed_bytes\":{},",
            "\"replayed\":{},",
            "\"decline_reason\":{},",
            "\"plaintext_bytes\":{},",
            "\"correction_bytes\":{},",
            "\"rans_plaintext_bytes\":{},",
            "\"raw_ratio\":{},",
            "\"plain_ratio\":{},",
            "\"rans_ratio\":{}",
            "}}"
        ),
        s.object,
        s.generation,
        s.compressed_bytes,
        s.replayed,
        reason,
        opt_u64(s.plaintext_bytes),
        opt_u64(s.correction_bytes),
        opt_u64(s.rans_plaintext_bytes),
        raw_ratio,
        plain_ratio,
        rans_ratio,
    )
}

#[cfg(feature = "deflate-replay")]
fn opt_u64(v: Option<u64>) -> String {
    match v {
        Some(n) => n.to_string(),
        None => "null".to_string(),
    }
}

/// Fixed-point ratio with six decimals, computed with integer arithmetic only
/// (truncating division). Diagnostics only; never decides a persisted value.
#[cfg(feature = "deflate-replay")]
fn ratio6(num: u64, den: u64) -> String {
    if den == 0 {
        return "0.000000".to_string();
    }
    let scaled = u128::from(num) * 1_000_000;
    let q = scaled / u128::from(den);
    format!("{}.{:06}", q / 1_000_000, q % 1_000_000)
}

/// Stable string name for an object role, as used in the oracle JSON.
fn role_name(role: pdf::ObjRole) -> &'static str {
    match role {
        pdf::ObjRole::Generic => "Generic",
        pdf::ObjRole::XRefStream => "XRefStream",
        pdf::ObjRole::ObjectStream => "ObjectStream",
    }
}

/// Minimal JSON string escaping for the `file` field (paths may contain quotes).
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn cmd_capabilities() -> Result<()> {
    println!(
        concat!(
            "{{",
            "\"crate\":\"vole-document\",",
            "\"format_major\":{},",
            "\"format_minor\":{},",
            "\"dra_version\":{},",
            "\"exactness_profiles\":[\"EXACT_BYTES\"],",
            "\"source_formats\":[\"OPAQUE\",\"PDF\"],",
            "\"universe\":\"{}\",",
            "\"dra_ops\":[\"EMIT_OBJECT\",\"INLINE\",\"REPEAT_LAST\",\"DECODE_CHANNEL\",\"INTERLEAVE_CHANNELS\",\"MARK_OFFSET\",\"EMIT_OFFSET\",\"PACK_SEGMENTS\",\"PACKED_CHANNELS\",\"DEFLATE_REPLAY\"],",
            "\"entropy_channels\":[\"ORDER0_BYTE_RANS\"],",
            "\"features\":{{\"rans\":{},\"deflate_replay\":{}}}",
            "}}"
        ),
        vole_document::container::header::FORMAT_MAJOR,
        vole_document::container::header::FORMAT_MINOR,
        vole_document::dra::program::DRA_VERSION,
        UNIVERSE,
        cfg!(feature = "rans"),
        cfg!(feature = "deflate-replay"),
    );
    Ok(())
}

fn describe_op(op: &Op) -> String {
    match op {
        Op::EmitObject { object_id } => format!("EMIT_OBJECT({object_id})"),
        Op::Inline { bytes } => format!("INLINE({})", bytes.len()),
        Op::RepeatLast { count } => format!("REPEAT_LAST({count})"),
        Op::DecodeChannel { channel_id } => format!("DECODE_CHANNEL({channel_id})"),
        Op::InterleaveChannels {
            first_payload_channel,
            payload_channel_count,
            ..
        } => format!("INTERLEAVE_CHANNELS({first_payload_channel},{payload_channel_count})"),
        Op::MarkOffset { slot } => format!("MARK_OFFSET({slot})"),
        Op::EmitOffset { slot, width } => format!("EMIT_OFFSET({slot},{width})"),
        Op::PackSegments { data_object, items } => {
            format!("PACK_SEGMENTS({data_object},{})", items.len())
        }
        Op::PackedChannels {
            data_channel,
            plan_channel,
            declared_output_len,
        } => format!("PACKED_CHANNELS({data_channel},{plan_channel},{declared_output_len})"),
        Op::DeflateReplay {
            replay_codec,
            source_kind,
            source_id,
            corrections_object,
            declared_output_len,
        } => format!(
            "DEFLATE_REPLAY({replay_codec},{source_kind},{source_id},{corrections_object},{declared_output_len})"
        ),
    }
}

fn report_json(r: &encode::EncodeReport) -> String {
    format!(
        concat!(
            "{{",
            "\"ok\":true,",
            "\"candidate\":\"{}\",",
            "\"candidates_evaluated\":{},",
            "\"source_len\":{},",
            "\"encoded_len\":{},",
            "\"ratio\":{:.6},",
            "\"sha256\":\"{}\",",
            "\"graph_ops\":{},",
            "\"cost\":{}",
            "}}"
        ),
        r.kind.name(),
        r.candidates_evaluated,
        r.source_len,
        r.encoded_len,
        r.compression_ratio(),
        r.sha256_hex,
        r.graph_ops,
        r.cost.to_json(),
    )
}

/// Write `bytes` to `path` atomically: temp sibling, fsync, rename, fsync dir.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "out".to_string());
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = dir.join(format!(".{name}.voldoc-tmp-{}-{nanos}", std::process::id()));

    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(Error::from(e));
    }
    // Best-effort directory durability (Linux).
    if let Ok(d) = fs::File::open(&dir) {
        let _ = d.sync_all();
    }
    Ok(())
}
