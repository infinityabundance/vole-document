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

use vole_document::container::UNIVERSE_V1;
use vole_document::dra::Op;
use vole_document::error::{Error, Result};
use vole_document::limits::Limits;
use vole_document::{encode, integrity, materialize};

const USAGE: &str = "\
vole-document — byte-exact procedural document storage

USAGE:
    vole-document encode     INPUT            OUTPUT.voldoc
    vole-document decode     INPUT.voldoc     OUTPUT
    vole-document materialize INPUT.voldoc    OUTPUT
    vole-document verify     INPUT.voldoc
    vole-document inspect    INPUT.voldoc
    vole-document capabilities

EXIT CODES:
    0 ok   2 usage   3 io   4 invalid-container   5 unsupported-version
    6 unsupported-feature   7 integrity-mismatch   8 resource-limit
    9 invalid-graph  10 invalid-model  15 coverage-violation
    16 reconstruction-mismatch  70 internal-invariant
";

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
    let cmd = match args.get(1).map(String::as_str) {
        Some(c) => c,
        None => {
            print!("{USAGE}");
            return Err(Error::usage("no subcommand given"));
        }
    };
    let limits = Limits::DEFAULT;

    match cmd {
        "-h" | "--help" | "help" => {
            print!("{USAGE}");
            Ok(())
        }
        "capabilities" => cmd_capabilities(),
        "encode" => {
            let input = arg(args, 2, "INPUT")?;
            let output = arg(args, 3, "OUTPUT.voldoc")?;
            cmd_encode(&input, &output, limits)
        }
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
        other => Err(Error::usage(format!(
            "unknown subcommand {other:?}\n\n{USAGE}"
        ))),
    }
}

fn arg(args: &[String], idx: usize, name: &str) -> Result<PathBuf> {
    args.get(idx)
        .map(PathBuf::from)
        .ok_or_else(|| Error::usage(format!("missing argument {name}")))
}

fn cmd_encode(input: &Path, output: &Path, limits: Limits) -> Result<()> {
    let source = fs::read(input)?;
    if source.len() as u64 > limits.max_input_bytes {
        return Err(Error::resource_limit(
            "input exceeds configured input limit",
        ));
    }
    let (bytes, report) = encode::encode(&source, limits)?;
    write_atomic(output, &bytes)?;

    println!("{}", report_json(&report));
    Ok(())
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

fn cmd_capabilities() -> Result<()> {
    println!(
        concat!(
            "{{",
            "\"crate\":\"vole-document\",",
            "\"format_major\":{},",
            "\"format_minor\":{},",
            "\"exactness_profiles\":[\"EXACT_BYTES\"],",
            "\"source_formats\":[\"OPAQUE\"],",
            "\"universe\":\"{}\",",
            "\"dra_ops\":[\"EMIT_OBJECT\",\"INLINE\",\"REPEAT_LAST\"],",
            "\"entropy_channels\":[],",
            "\"features\":{{\"rans\":{},\"pdf\":false,\"deflate_replay\":false}}",
            "}}"
        ),
        vole_document::container::header::FORMAT_MAJOR,
        vole_document::container::header::FORMAT_MINOR,
        UNIVERSE_V1,
        cfg!(feature = "rans"),
    );
    Ok(())
}

fn describe_op(op: &Op) -> String {
    match op {
        Op::EmitObject { object_id } => format!("EMIT_OBJECT({object_id})"),
        Op::Inline { bytes } => format!("INLINE({})", bytes.len()),
        Op::RepeatLast { count } => format!("REPEAT_LAST({count})"),
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
