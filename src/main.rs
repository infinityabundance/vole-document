//! `vole-document` command-line interface.
//!
//! File output is written atomically: a temporary sibling is written, fsynced,
//! then renamed into place. A partial or failed operation never replaces the
//! destination.

use std::fs;
#[cfg(feature = "rans")]
use std::fs::File;
use std::io::Write;
#[cfg(feature = "rans")]
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use vole_document::adapter::pdf;
#[cfg(feature = "store")]
use vole_document::container::Descriptor;
use vole_document::container::UNIVERSE;
#[cfg(feature = "rans")]
use vole_document::container::header::{FEATURE_SEEK_DIRECTORY, HEADER_LEN, Header};
use vole_document::dra::Op;
use vole_document::encode::candidates::CandidateKind;
use vole_document::error::{Error, Result};
#[cfg(feature = "field")]
use vole_document::field::{
    Field, FieldId, FieldStore, build as field_build,
    cache::DerivedCache,
    capabilities as field_capabilities,
    document_format::detect_document_format,
    edit as field_edit,
    explain::explain,
    ingest as field_ingest,
    observe::{ObserveRequest, ObserveStats, Representation, Selector, observe},
    provenance::{AnswerValue, FieldAnswer},
    share,
};
use vole_document::limits::Limits;
#[cfg(feature = "rans")]
use vole_document::materialize::observation::{ObservationReport, ObservationSelector};
#[cfg(feature = "store")]
use vole_document::store::{EmbeddedStore, account, externalize, gc};
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
    vole-document pdf-make-large DIR [OBJECTS]
";

/// The `deflate-stats` line is advertised only when the replay stack is built in.
#[cfg(feature = "deflate-replay")]
const USAGE_DEFLATE_STATS: &str = "    vole-document deflate-stats INPUT...\n";
#[cfg(not(feature = "deflate-replay"))]
const USAGE_DEFLATE_STATS: &str = "";

/// The `view` line is advertised only when the entropy decoder is built in.
#[cfg(feature = "rans")]
const USAGE_VIEW: &str = "\
    vole-document view      INPUT.voldoc [OUTPUT] --byte-range A:L | --pdf-object N:G |\n\
        --pdf-stream N:G | --pdf-revision I [--stats]\n";
#[cfg(not(feature = "rans"))]
const USAGE_VIEW: &str = "";

/// The store lines are advertised only when the object store is built in.
#[cfg(feature = "store")]
const USAGE_STORE: &str = "\
    vole-document decode --store STORE_DIR INPUT.voldoc OUTPUT\n\
    vole-document store put     INPUT.voldoc STORE_DIR\n\
    vole-document store account STORE_DIR ROOT...\n\
    vole-document store gc      STORE_DIR ROOT...\n";
#[cfg(not(feature = "store"))]
const USAGE_STORE: &str = "";

/// The field observation verbs are advertised only when the field is built in.
#[cfg(feature = "field")]
const USAGE_FIELD: &str = "\
    vole-document field-ingest INPUT.voldoc --store DIR [--workers N] [--entropyfs | --packed]
    vole-document field-build INPUT --store DIR [--profile runtime] [--workers N] [--voldoc OUT.voldoc] [--entropyfs | --packed]
        (direct source -> field: one process, no candidate search; the exact
         authority is the fixed profile's `.voldoc`, stored in the field)
    vole-document field-edit --store DIR --field HEX --page N --content FILE [--entropyfs | --packed]
    vole-document field-external --store DIR --field HEX [--lineage FAMILY:MEMBER:HEAD]
        [--dataset ID] [--revision-family ID] [--origin harness|operator|catalog] [--source LABEL]
        (attaches an EXTERNAL corpus/dataset context BESIDE the field: family/member/head
         the document bytes do not contain; stored in <store>/external/, never in the seed
         DAG, index, manifest, or exactness authority; --clear removes it. No --lineage and
         no --clear shows the attached record, or declines typed when none is attached)
    vole-document field-external --store DIR --field HEX --clear
    vole-document observe --store DIR --field HEX [--entropyfs | --packed] [--promote[=BYTES]] (--page N | --object N | --stream N |
        --revision N | --revisions | --external-lineage | --byte-range A..B | --metadata | --doc-text | --heading N |
        --block N | --table N | --cell T:R:C | --resource N | --link N |
        --spine-item N | --sheet N | --xlsx-cell A1 | --text PATTERN |
        --xlsx-styles | --xlsx-defined-names | --xlsx-external-rels |
        --xlsx-comments | --xlsx-hyperlinks | --xlsx-tables | --xlsx-drawing |
        --slide N | --pptx-shape I | --pptx-notes N | --pptx-layouts |
        --pptx-masters | --pptx-theme | --pptx-media N | --pptx-tables |
        --pptx-find PATTERN |
        --ods-sheet N | --ods-cell B7|R:C | --ods-styles | --ods-named-expressions |
        --ods-comments | --ods-find PATTERN |
        --odp-slide N | --odp-shape I | --odp-notes N | --odp-masters |
        --odp-media N | --odp-tables | --odp-find PATTERN |
        --json-pointer PATH | --json-node PATH | --json-find PATTERN |
        --yaml-path PATH | --yaml-node PATH | --yaml-documents | --yaml-anchor NAME |
        --yaml-find PATTERN |
        --csv-row N | --csv-cell R:C | --csv-header | --csv-range R1:C1:R2:C2 |
        --csv-find PATTERN |
        --md-heading N | --md-block N | --md-code N | --md-link N |
        --md-find PATTERN |
        --xml-path P | --xml-element P | --xml-attr PATH@NAME |
        --xml-namespaces | --xml-find PATTERN |
        --html-path P | --html-element P | --html-attr PATH@NAME |
        --html-scripts | --html-find PATTERN |
        --jsonl-line N | --jsonl-pointer N:POINTER | --jsonl-find PATTERN |
        --eml-header NAME | --eml-part N | --eml-attachments | --eml-body |
        --eml-find PATTERN |
        --parquet-schema | --parquet-column N | --parquet-row-group N |
        --parquet-cell R:C |
        --arrow-schema | --arrow-column NAME|N | --arrow-batch N |
        --arrow-cell R:C) --kind metadata|text|structure|operators|
        encoded|decoded|exact|preview|lineage|full
    vole-document observe-batch --store DIR --field HEX [--entropyfs | --packed] [--promote[=BYTES]]
        [--requests FILE|-] [--repeat N]
        (one process serving many observations: one JSON answer per line; each
         request line is the per-observation flag grammar WITHOUT --store/--field)
    vole-document find    --store DIR --field HEX --text PATTERN [--entropyfs | --packed]
        (format-agnostic lexical search: the common SearchMatch selector)
    vole-document explain --store DIR --field HEX <selector> --kind KIND [--analyze] [--entropyfs | --packed]
    vole-document preview --store DIR --field HEX --page N [--json] [--entropyfs | --packed]
    vole-document materialize --store DIR --field HEX --exact --output FILE [--entropyfs | --packed]
    vole-document cache  --store DIR [--clear] [--entropyfs | --packed]
    vole-document field-store-stats --store DIR [--entropyfs | --packed]
    (--entropyfs needs a build with the entropyfs-store feature)
    (--workers N parallelizes independently decodable ingest work; needs the
     `parallel` feature; absent or 1 is serial, 0 is available_parallelism)
    (--packed replaces the seed/ namespace with fieldpack/; mutually exclusive
     with --entropyfs; --sync=batch|each selects the packed writer's durability
     policy, batch by default: one fsync per segment instead of one per node)
    (--promote[=BYTES] opts into a durable, byte-budgeted promotion layer over the
     reused intermediates (Phase 15.6); off by default and never on the exactness path)
";
#[cfg(not(feature = "field"))]
const USAGE_FIELD: &str = "";

/// The fine-unit sharing verbs are advertised only when the field is built in.
#[cfg(feature = "field")]
const USAGE_SHARE: &str = "\
    vole-document share report INPUT.voldoc...\n\
    vole-document share-account --store DIR INPUT.voldoc...\n\
    vole-document share externalize --store DIR INPUT.voldoc OUTPUT.voldoc\n";
#[cfg(not(feature = "field"))]
const USAGE_SHARE: &str = "";

const USAGE_TAIL: &str = "\
    vole-document capabilities [ROOT]

KIND (for encode --force): raw | rle | byte-rans | pdf-physical | pdf-channels |
    pdf-layout | pdf-layout-rans | pdf-deflate-replay | pdf-deflate-replay-rans |
    pdf-deflate-replay-rans-indexed | pdf-length-revision | pdf-cos-template
    Forces the complete-cost court to consider only that candidate family, for
    honest per-mechanism ablation. Fails when the input does not propose it.

    capabilities ROOT detects ROOT's document format from bytes (never a file
    name) and prints the supported common selectors/representations.

EXIT CODES:
    0 ok   2 usage   3 io   4 invalid-container   5 unsupported-version
    6 unsupported-feature   7 integrity-mismatch   8 resource-limit
    9 invalid-graph  10 invalid-model  15 coverage-violation
    16 reconstruction-mismatch  70 internal-invariant
";

/// The full usage text, with the replay-gated command line included only when
/// the feature is present.
fn usage() -> String {
    format!(
        "{USAGE_HEAD}{USAGE_VIEW}{USAGE_DEFLATE_STATS}{USAGE_STORE}{USAGE_FIELD}{USAGE_SHARE}{USAGE_TAIL}"
    )
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
        if std::env::var_os("VOLE_REPLAY_WORKER").is_none()
            && let Ok(exe) = std::env::current_exe()
        {
            install_default_replay_worker(exe);
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
        "capabilities" => cmd_capabilities(args),
        "encode" => cmd_encode_args(args, limits),
        "decode" | "materialize" => {
            #[cfg(feature = "field")]
            if args
                .iter()
                .any(|a| a.as_str() == "--field" || a.starts_with("--field="))
            {
                return cmd_field_materialize(args, limits);
            }
            cmd_decode_args(args, limits)
        }
        "verify" => {
            let input = arg(args, 2, "INPUT.voldoc")?;
            cmd_verify(&input, limits)
        }
        "inspect" => {
            let input = arg(args, 2, "INPUT.voldoc")?;
            cmd_inspect(&input, limits)
        }
        #[cfg(feature = "rans")]
        "view" => cmd_view_args(args, limits),
        #[cfg(feature = "store")]
        "store" => cmd_store_args(args, limits),
        "pdf-inspect" => {
            let input = arg(args, 2, "INPUT")?;
            cmd_pdf_inspect(&input, limits)
        }
        "pdf-make-samples" => {
            let dir = arg(args, 2, "DIR")?;
            cmd_pdf_make_samples(&dir)
        }
        "pdf-make-large" => {
            let dir = arg(args, 2, "DIR")?;
            let objects = args.get(3).map(String::as_str);
            cmd_pdf_make_large(&dir, objects)
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
        #[cfg(feature = "field")]
        "field-ingest" => cmd_field_ingest(args, limits),
        #[cfg(feature = "field")]
        "field-build" => cmd_field_build(args, limits),
        #[cfg(feature = "field")]
        "field-edit" => cmd_field_edit(args, limits),
        #[cfg(feature = "field")]
        "field-external" => cmd_field_external(args),
        #[cfg(feature = "field")]
        "observe" => cmd_field_observe(args, limits),
        #[cfg(feature = "field")]
        "observe-batch" => cmd_field_observe_batch(args, limits),
        #[cfg(feature = "field")]
        "find" => cmd_field_find(args, limits),
        #[cfg(feature = "field")]
        "explain" => cmd_field_explain(args, limits),
        #[cfg(feature = "field")]
        "preview" => cmd_field_preview(args, limits),
        #[cfg(feature = "field")]
        "cache" => cmd_field_cache(args),
        #[cfg(feature = "field")]
        "field-store-stats" => cmd_field_store_stats(args),
        #[cfg(feature = "field")]
        "share" => cmd_share_args(args, limits),
        #[cfg(feature = "field")]
        "share-account" => cmd_share_account(args, limits),
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
        "pdf-deflate-replay-rans-indexed" => Ok(CandidateKind::PdfDeflateReplayRansIndexed),
        "pdf-length-revision" => Ok(CandidateKind::PdfLengthRevision),
        "pdf-cos-template" => Ok(CandidateKind::PdfCosTemplate),
        other => Err(Error::usage(format!(
            "unknown --force kind {other:?}; expected one of raw, rle, byte-rans, pdf-physical, pdf-channels, pdf-layout, pdf-layout-rans, pdf-deflate-replay, pdf-deflate-replay-rans, pdf-deflate-replay-rans-indexed, pdf-length-revision, pdf-cos-template"
        ))),
    }
}

/// Parse and dispatch the `view` subcommand: serve a narrow observation of a
/// `.voldoc` that carries an `OBSERVATION_INDEX`.
///
/// Exactly one selector flag is required. `--stats` with no `OUTPUT` prints the
/// stats object only (no bytes); otherwise bytes go to `OUTPUT` atomically or to
/// stdout, with the stats object on stdout (or stderr when bytes occupy stdout).
#[cfg(feature = "rans")]
fn cmd_view_args(args: &[String], limits: Limits) -> Result<()> {
    let mut selector: Option<ObservationSelector> = None;
    let mut stats = false;
    let mut positional: Vec<&str> = Vec::new();
    let mut i = 2;
    while i < args.len() {
        let a = args[i].as_str();
        if a == "--stats" {
            stats = true;
            i += 1;
        } else if let Some(v) = a.strip_prefix("--byte-range=") {
            set_selector(&mut selector, parse_byte_range(v)?)?;
            i += 1;
        } else if let Some(v) = a.strip_prefix("--pdf-object=") {
            set_selector(&mut selector, parse_pdf_object(v)?)?;
            i += 1;
        } else if let Some(v) = a.strip_prefix("--pdf-stream=") {
            set_selector(&mut selector, parse_pdf_stream(v)?)?;
            i += 1;
        } else if let Some(v) = a.strip_prefix("--pdf-revision=") {
            set_selector(&mut selector, parse_pdf_revision(v)?)?;
            i += 1;
        } else if a == "--byte-range" {
            let v = args
                .get(i + 1)
                .ok_or_else(|| Error::usage("--byte-range requires A:L"))?;
            set_selector(&mut selector, parse_byte_range(v)?)?;
            i += 2;
        } else if a == "--pdf-object" {
            let v = args
                .get(i + 1)
                .ok_or_else(|| Error::usage("--pdf-object requires N:G"))?;
            set_selector(&mut selector, parse_pdf_object(v)?)?;
            i += 2;
        } else if a == "--pdf-stream" {
            let v = args
                .get(i + 1)
                .ok_or_else(|| Error::usage("--pdf-stream requires N:G"))?;
            set_selector(&mut selector, parse_pdf_stream(v)?)?;
            i += 2;
        } else if a == "--pdf-revision" {
            let v = args
                .get(i + 1)
                .ok_or_else(|| Error::usage("--pdf-revision requires I"))?;
            set_selector(&mut selector, parse_pdf_revision(v)?)?;
            i += 2;
        } else {
            positional.push(a);
            i += 1;
        }
    }

    let selector = selector.ok_or_else(|| {
        Error::usage(
            "view requires exactly one of --byte-range, --pdf-object, --pdf-stream, --pdf-revision",
        )
    })?;
    let input = positional
        .first()
        .map(PathBuf::from)
        .ok_or_else(|| Error::usage("missing argument INPUT.voldoc"))?;
    if positional.len() > 2 {
        return Err(Error::usage(format!(
            "unexpected extra argument {:?}",
            positional[2]
        )));
    }
    let output = positional.get(1).map(PathBuf::from);
    cmd_view(&input, output.as_deref(), selector, stats, limits)
}

/// Record the one selector a `view` invocation may carry.
#[cfg(feature = "rans")]
fn set_selector(
    slot: &mut Option<ObservationSelector>,
    selector: ObservationSelector,
) -> Result<()> {
    if slot.is_some() {
        return Err(Error::usage(
            "view accepts exactly one selector; more than one was given",
        ));
    }
    *slot = Some(selector);
    Ok(())
}

#[cfg(feature = "rans")]
fn parse_byte_range(value: &str) -> Result<ObservationSelector> {
    let (offset, len) = value
        .split_once(':')
        .ok_or_else(|| Error::usage("--byte-range must be A:L (e.g. 1024:4096)"))?;
    let offset: u64 = offset
        .parse()
        .map_err(|_| Error::usage(format!("--byte-range offset {offset:?} is not a u64")))?;
    let len: u64 = len
        .parse()
        .map_err(|_| Error::usage(format!("--byte-range length {len:?} is not a u64")))?;
    Ok(ObservationSelector::ByteRange { offset, len })
}

#[cfg(feature = "rans")]
fn parse_pdf_object(value: &str) -> Result<ObservationSelector> {
    let (object, generation) = parse_n_g(value, "--pdf-object")?;
    Ok(ObservationSelector::PdfIndirectObject { object, generation })
}

#[cfg(feature = "rans")]
fn parse_pdf_stream(value: &str) -> Result<ObservationSelector> {
    let (object, generation) = parse_n_g(value, "--pdf-stream")?;
    Ok(ObservationSelector::PdfEncodedStream { object, generation })
}

#[cfg(feature = "rans")]
fn parse_pdf_revision(value: &str) -> Result<ObservationSelector> {
    let index: u32 = value
        .parse()
        .map_err(|_| Error::usage(format!("--pdf-revision {value:?} is not a u32")))?;
    Ok(ObservationSelector::PdfRevision { index })
}

#[cfg(feature = "rans")]
fn parse_n_g(value: &str, flag: &str) -> Result<(u32, u16)> {
    let (n, g) = value
        .split_once(':')
        .ok_or_else(|| Error::usage(format!("{flag} must be N:G (e.g. 4:0)")))?;
    let object: u32 = n
        .parse()
        .map_err(|_| Error::usage(format!("{flag} object {n:?} is not a u32")))?;
    let generation: u16 = g
        .parse()
        .map_err(|_| Error::usage(format!("{flag} generation {g:?} is not a u16")))?;
    Ok((object, generation))
}

#[cfg(feature = "rans")]
fn cmd_view(
    input: &Path,
    output: Option<&Path>,
    selector: ObservationSelector,
    stats: bool,
    limits: Limits,
) -> Result<()> {
    // Peek only the fixed 64-byte header first. A descriptor that advertises the
    // seek feature is served by the seek reader over the same handle, so the
    // process never `fs::read`s the whole descriptor: the bytes-read claim is a
    // real on-disk-I/O claim, not a post-hoc approximation. A descriptor without
    // one keeps the Phase-7 in-memory path (which must parse the whole file).
    let mut file = File::open(input)?;
    let mut hdr = [0u8; HEADER_LEN];
    file.read_exact(&mut hdr)
        .map_err(|_| Error::invalid_container("truncated header"))?;
    let header = Header::decode(&hdr)?;
    if header.optional_features & FEATURE_SEEK_DIRECTORY != 0 {
        // `materialize_observation_seeked` seeks to offset 0 itself for the
        // header, so rewind our peek rather than leaving the position at 64.
        file.seek(SeekFrom::Start(0))
            .map_err(|e| Error::io(format!("seek to start failed: {e}")))?;
        let report = vole_document::materialize::seek::materialize_observation_seeked(
            file, selector, limits,
        )?;
        let json = observation_json(&selector, header.declared_source_len, &report);
        return emit_view(output, stats, &report, &json);
    }
    drop(file);
    let encoded = fs::read(input)?;
    let parsed = vole_document::container::Descriptor::parse(&encoded, limits)?;
    let report = vole_document::materialize::observation::materialize_observation(
        &parsed, selector, limits,
    )?;
    let json = observation_json(&selector, parsed.descriptor.source_len, &report);
    emit_view(output, stats, &report, &json)
}

/// Write the served bytes (or print stats) exactly as the `view` CLI documents.
#[cfg(feature = "rans")]
fn emit_view(
    output: Option<&Path>,
    stats: bool,
    report: &ObservationReport,
    json: &str,
) -> Result<()> {
    match output {
        Some(path) => {
            write_atomic(path, &report.bytes)?;
            println!("{json}");
        }
        // Stats-only: no bytes are emitted, so stdout stays a single JSON line.
        None if stats => println!("{json}"),
        None => {
            std::io::stdout()
                .write_all(&report.bytes)
                .map_err(Error::from)?;
            eprintln!("{json}");
        }
    }
    Ok(())
}

/// One JSON line describing a served observation and its measured cost.
#[cfg(feature = "rans")]
fn observation_json(
    selector: &ObservationSelector,
    source_len: u64,
    report: &ObservationReport,
) -> String {
    let s = &report.stats;
    format!(
        concat!(
            "{{",
            "\"ok\":true,",
            "\"selector\":\"{}\",",
            "\"source_len\":{},",
            "\"range_start\":{},",
            "\"range_len\":{},",
            "\"bytes\":{},",
            "\"ops_evaluated\":{},",
            "\"ops_total\":{},",
            "\"objects_fetched\":{},",
            "\"objects_total\":{},",
            "\"channels_decoded\":{},",
            "\"channels_total\":{},",
            "\"entropy_bytes_decoded\":{},",
            "\"descriptor_bytes_traversed\":{},",
            "\"bytes_read\":{},",
            "\"integrity_verified\":{},",
            "\"output_bytes\":{},",
            "\"work_amplification\":{:.6}",
            "}}"
        ),
        selector_label(selector),
        source_len,
        report.range.0,
        report.range.1.saturating_sub(report.range.0),
        report.bytes.len(),
        s.ops_evaluated,
        s.ops_total,
        s.objects_fetched,
        s.objects_total,
        s.channels_decoded,
        s.channels_total,
        s.entropy_bytes_decoded,
        s.descriptor_bytes_traversed,
        s.bytes_read,
        s.integrity_verified,
        s.output_bytes,
        s.work_amplification(),
    )
}

#[cfg(feature = "rans")]
fn selector_label(selector: &ObservationSelector) -> String {
    match selector {
        ObservationSelector::ByteRange { offset, len } => format!("byte-range:{offset}:{len}"),
        ObservationSelector::PdfIndirectObject { object, generation } => {
            format!("pdf-object:{object}:{generation}")
        }
        ObservationSelector::PdfEncodedStream { object, generation } => {
            format!("pdf-stream:{object}:{generation}")
        }
        ObservationSelector::PdfRevision { index } => format!("pdf-revision:{index}"),
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

/// Parse `decode`/`materialize` arguments: an optional `--store STORE_DIR`
/// (store-backed descriptors), then the positional `INPUT.voldoc` and `OUTPUT`.
///
/// A standalone `decode INPUT OUTPUT` invocation is unchanged. Without the
/// `store` cargo feature, `--store` is refused with `UnsupportedFeature` rather
/// than ignored.
fn cmd_decode_args(args: &[String], limits: Limits) -> Result<()> {
    let mut store_dir: Option<PathBuf> = None;
    let mut positional: Vec<&str> = Vec::new();
    let mut i = 2;
    while i < args.len() {
        let a = args[i].as_str();
        if a == "--store" {
            let dir = args
                .get(i + 1)
                .ok_or_else(|| Error::usage("--store requires a STORE_DIR argument"))?;
            store_dir = Some(PathBuf::from(dir));
            i += 2;
        } else if let Some(dir) = a.strip_prefix("--store=") {
            store_dir = Some(PathBuf::from(dir));
            i += 1;
        } else {
            positional.push(a);
            i += 1;
        }
    }
    let input = positional
        .first()
        .map(PathBuf::from)
        .ok_or_else(|| Error::usage("missing argument INPUT.voldoc"))?;
    let output = positional
        .get(1)
        .map(PathBuf::from)
        .ok_or_else(|| Error::usage("missing argument OUTPUT"))?;
    if positional.len() > 2 {
        return Err(Error::usage(format!(
            "unexpected extra argument {:?}",
            positional[2]
        )));
    }
    if let Some(dir) = store_dir {
        #[cfg(feature = "store")]
        {
            return cmd_decode_with_store(&dir, &input, &output, limits);
        }
        #[cfg(not(feature = "store"))]
        {
            let _ = dir;
            return Err(Error::unsupported_feature(
                "this build was compiled without the `store` feature",
            ));
        }
    }
    cmd_decode(&input, &output, limits)
}

/// Materialize a store-backed descriptor, resolving `EXTERNAL_REF` objects
/// through an [`EmbeddedStore`] rooted at `store_dir`.
#[cfg(feature = "store")]
fn cmd_decode_with_store(
    store_dir: &Path,
    input: &Path,
    output: &Path,
    limits: Limits,
) -> Result<()> {
    let encoded = fs::read(input)?;
    let store = EmbeddedStore::open(store_dir)?;
    let (bytes, parsed) = materialize::decode_to_bytes_with(&encoded, &store, limits)?;
    write_atomic(output, &bytes)?;
    println!(
        concat!(
            "{{",
            "\"ok\":true,",
            "\"source_len\":{},",
            "\"sha256\":\"{}\",",
            "\"graph_ops\":{},",
            "\"objects\":{},",
            "\"store\":\"{}\"",
            "}}"
        ),
        bytes.len(),
        integrity::to_hex(&parsed.descriptor.source_sha256),
        parsed.descriptor.program.ops.len(),
        parsed.descriptor.objects.len(),
        store_dir.display()
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

/// `store put | account | gc` argument dispatch (Phase 9.2).
///
/// Every `ROOT...` is a `ROOT.voldoc` descriptor. `put` writes the store-backed
/// descriptor to `STORE_DIR/<input-stem>.voldoc`; `account` and `gc` read their
/// roots in place.
#[cfg(feature = "store")]
fn cmd_store_args(args: &[String], limits: Limits) -> Result<()> {
    match args.get(2).map(String::as_str) {
        Some("put") => {
            let input = arg(args, 3, "INPUT.voldoc")?;
            let store_dir = arg(args, 4, "STORE_DIR")?;
            if let Some(extra) = args.get(5) {
                return Err(Error::usage(format!("unexpected extra argument {extra:?}")));
            }
            cmd_store_put(&input, &store_dir, limits)
        }
        Some("account") => {
            let store_dir = arg(args, 3, "STORE_DIR")?;
            let roots = root_args(args, 4)?;
            cmd_store_account(&store_dir, &roots, limits)
        }
        Some("gc") => {
            let store_dir = arg(args, 3, "STORE_DIR")?;
            let roots = root_args(args, 4)?;
            cmd_store_gc(&store_dir, &roots, limits)
        }
        Some(other) => Err(Error::usage(format!(
            "unknown store subcommand {other:?}; expected put | account | gc"
        ))),
        None => Err(Error::usage(
            "store requires a subcommand: put | account | gc",
        )),
    }
}

#[cfg(feature = "store")]
fn root_args(args: &[String], start: usize) -> Result<Vec<PathBuf>> {
    let roots: Vec<PathBuf> = args
        .get(start..)
        .unwrap_or(&[])
        .iter()
        .map(PathBuf::from)
        .collect();
    if roots.is_empty() {
        return Err(Error::usage("expected at least one ROOT.voldoc"));
    }
    Ok(roots)
}

/// Externalize every object of `input` into `store_dir` and write the
/// store-backed descriptor to `STORE_DIR/<input-stem>.voldoc`.
///
/// An input that is already store-backed resolves its references through the
/// destination store before re-putting every object, so the operation is
/// idempotent and every object ends up in `store_dir` (a reference the store
/// cannot satisfy fails closed).
#[cfg(feature = "store")]
fn cmd_store_put(input: &Path, store_dir: &Path, limits: Limits) -> Result<()> {
    let encoded = fs::read(input)?;
    let mut descriptor = Descriptor::parse(&encoded, limits)?.descriptor;
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| Error::usage(format!("input {input:?} has no usable file stem")))?;
    let output = store_dir.join(format!("{stem}.voldoc"));

    let mut store = EmbeddedStore::open(store_dir)?;
    let resolver = store.clone();
    externalize(&mut descriptor, &resolver, &mut store)?;
    let (bytes, _cost) = descriptor.serialize()?;
    write_atomic(&output, &bytes)?;
    let stats = store.stats()?;
    println!(
        concat!(
            "{{",
            "\"ok\":true,",
            "\"output\":\"{}\",",
            "\"objects\":{},",
            "\"stored_objects\":{},",
            "\"stored_bytes\":{},",
            "\"root_bytes\":{}",
            "}}"
        ),
        output.display(),
        descriptor.objects.len(),
        stats.object_count,
        stats.stored_bytes,
        bytes.len()
    );
    Ok(())
}

/// Print the three accounting universes (standalone / unique-reachable /
/// amortized) over the roots, plus the per-root amortized split.
#[cfg(feature = "store")]
fn cmd_store_account(store_dir: &Path, roots: &[PathBuf], limits: Limits) -> Result<()> {
    let store = EmbeddedStore::open(store_dir)?;
    let mut descriptors = Vec::with_capacity(roots.len());
    for path in roots {
        let encoded = fs::read(path)?;
        descriptors.push(Descriptor::parse(&encoded, limits)?.descriptor);
    }
    let report = account(&descriptors, &store)?;
    let stats = store.stats()?;
    let per_root: Vec<String> = report
        .roots
        .iter()
        .zip(roots)
        .map(|(r, path)| {
            format!(
                concat!(
                    "{{",
                    "\"root\":\"{}\",",
                    "\"root_bytes\":{},",
                    "\"standalone_bytes\":{},",
                    "\"reachable_objects\":{},",
                    "\"amortized_bytes\":{}",
                    "}}"
                ),
                path.display(),
                r.root_bytes,
                r.standalone_bytes,
                r.reachable_objects,
                r.amortized_bytes
            )
        })
        .collect();
    println!(
        concat!(
            "{{",
            "\"ok\":true,",
            "\"roots\":{},",
            "\"standalone_bytes\":{},",
            "\"unique_reachable_bytes\":{},",
            "\"amortized_bytes\":{},",
            "\"unique_objects\":{},",
            "\"unique_object_bytes\":{},",
            "\"stored_bytes\":{},",
            "\"dangling\":{},",
            "\"per_root\":[{}]",
            "}}"
        ),
        report.roots.len(),
        report.standalone_bytes,
        report.unique_reachable_bytes,
        report.amortized_bytes,
        report.unique_objects,
        report.unique_object_bytes,
        stats.stored_bytes,
        report.dangling.len(),
        per_root.join(","),
    );
    Ok(())
}

/// Run mark-and-sweep GC over the roots and print the report. Every `ROOT` must
/// be a store-backed descriptor (one with external references); a standalone
/// root would otherwise mark nothing and let GC sweep the whole store.
#[cfg(feature = "store")]
fn cmd_store_gc(store_dir: &Path, roots: &[PathBuf], limits: Limits) -> Result<()> {
    let store = EmbeddedStore::open(store_dir)?;
    let mut descriptors = Vec::with_capacity(roots.len());
    for path in roots {
        let encoded = fs::read(path)?;
        descriptors.push(Descriptor::parse(&encoded, limits)?.descriptor);
    }
    for (path, d) in roots.iter().zip(&descriptors) {
        if !d
            .objects
            .iter()
            .any(|o| matches!(o, vole_document::container::ObjectSource::External { .. }))
        {
            return Err(Error::usage(format!(
                "ROOT {} is not store-backed (no EXTERNAL_REF); refusing to GC",
                path.display()
            )));
        }
    }
    let report = gc(&descriptors, &store)?;
    let dangling: Vec<String> = report
        .dangling
        .iter()
        .map(|id| format!("\"{}\"", id.to_hex()))
        .collect();
    println!(
        concat!(
            "{{",
            "\"ok\":true,",
            "\"reachable\":{},",
            "\"swept\":{},",
            "\"bytes_reclaimed\":{},",
            "\"dangling\":{},",
            "\"dangling_ids\":[{}]",
            "}}"
        ),
        report.reachable,
        report.swept,
        report.bytes_reclaimed,
        report.dangling.len(),
        dangling.join(",")
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

/// Number of `FlateDecode` streams (and pages) the large generator emits by
/// default.
const LARGE_DEFAULT_OBJECTS: u64 = 800;

/// Source-corpus size floor for the partial-materialization query court
/// (contract §5.1, ≥ 32 MiB). The per-stream plaintext length is scaled so the
/// generated PDF reaches this regardless of the object count.
const LARGE_TARGET_BYTES: u64 = 32 * 1024 * 1024;

/// Upper bound on the requested object count, so a hostile argument cannot make
/// the generator allocate without bound.
const LARGE_MAX_OBJECTS: u64 = 100_000;

/// Write a large, deterministic, multi-object classic-xref PDF to `DIR/large.pdf`.
///
/// `OBJECTS` (default 800) is the number of pages; each page carries its own
/// `/FlateDecode` content stream, so there are `OBJECTS` lone-`FlateDecode`
/// streams and `2*OBJECTS + 3` indirect objects (catalog, pages, font, and one
/// page + one content object per page). Per-stream plaintext is generated from a
/// stream-indexed LCG so every stream's bytes are *distinct* (no accidental
/// cross-stream sharing), and each stream is a real, self-contained zlib
/// (RFC 1950) stream whose `/Length` and every xref offset are correct by
/// construction. The per-stream plaintext length is scaled so the total source is
/// at least 32 MiB.
///
/// This subcommand is encode-time corpus tooling: it lives in the binary, adds no
/// library or runtime dependency, and its output is regenerable and gitignored.
fn cmd_pdf_make_large(dir: &Path, objects_arg: Option<&str>) -> Result<()> {
    let objects: u64 = match objects_arg {
        Some(s) => s
            .parse()
            .map_err(|_| Error::usage(format!("OBJECTS {s:?} is not a non-negative integer")))?,
        None => LARGE_DEFAULT_OBJECTS,
    };
    if objects == 0 || objects > LARGE_MAX_OBJECTS {
        return Err(Error::usage(format!(
            "OBJECTS must be between 1 and {LARGE_MAX_OBJECTS}"
        )));
    }
    let bytes = pdf::large_pdf(objects, LARGE_TARGET_BYTES);
    fs::create_dir_all(dir)?;
    let name = "large.pdf";
    write_atomic(&dir.join(name), &bytes)?;
    let indirect_objects = 2 * objects + 3;
    println!(
        "{{\"ok\":true,\"dir\":\"{}\",\"file\":\"{}\",\"objects\":{},\"streams\":{},\"indirect_objects\":{},\"source_len\":{},\"sha256\":\"{}\"}}",
        json_escape(&dir.display().to_string()),
        name,
        objects,
        objects,
        indirect_objects,
        bytes.len(),
        integrity::to_hex(&integrity::sha256(&bytes)),
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

fn cmd_capabilities(args: &[String]) -> Result<()> {
    // `capabilities ROOT` discovers the detected document format and the common
    // selectors/representations its adapter serves (Phase 12.7); the bare
    // `capabilities` verb remains the codec/format-level report.
    if let Some(path) = args.get(2) {
        if let Some(extra) = args.get(3) {
            return Err(Error::usage(format!("unexpected extra argument {extra:?}")));
        }
        return cmd_document_capabilities(Path::new(path));
    }
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

/// `capabilities ROOT`: detect `ROOT`'s document format from bytes (never a file
/// name) and print the machine-readable common-observation capability set.
#[cfg(feature = "field")]
fn cmd_document_capabilities(input: &Path) -> Result<()> {
    let bytes = fs::read(input)?;
    let limits = Limits::DEFAULT;
    let source: Vec<u8> =
        if bytes.len() >= 8 && bytes[..8] == vole_document::container::header::MAGIC {
            let parsed = vole_document::container::Descriptor::parse(&bytes, limits)?;
            materialize::materialize(&parsed, limits)?
        } else {
            bytes
        };
    let fmt = detect_document_format(&source, limits);
    println!(
        "{}",
        field_capabilities::capabilities_for_format(fmt).to_json()
    );
    Ok(())
}

#[cfg(not(feature = "field"))]
fn cmd_document_capabilities(_input: &Path) -> Result<()> {
    Err(Error::unsupported_feature(
        "document capabilities require a build with the field feature",
    ))
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

/// Parsed arguments for the Phase-11 field observation verbs.
#[cfg(feature = "field")]
#[derive(Default)]
struct FieldArgs {
    store: Option<PathBuf>,
    field: Option<String>,
    page: Option<u32>,
    object: Option<u32>,
    stream: Option<u32>,
    revision: Option<u32>,
    /// `--revisions`: the whole PDF revision lineage (Phase 17).
    revisions: bool,
    /// `--external-lineage`: the external corpus/dataset lineage tuple (Phase
    /// 20.4), answered from the attached external context, never the document.
    external_lineage: bool,
    byte_range: Option<(u64, u64)>,
    kind: Option<String>,
    text: Option<String>,
    metadata: bool,
    doc_text: bool,
    heading: Option<u32>,
    block: Option<u32>,
    table: Option<u32>,
    cell: Option<(u32, u32, u32)>,
    resource: Option<u32>,
    link: Option<u32>,
    /// The EPUB reading-order coordinate (`--spine-item N`); reflowable EPUB has
    /// no intrinsic pages, so this is the native reading coordinate (plan DEC-5).
    #[cfg(feature = "epub")]
    spine_item: Option<u32>,
    /// The XLSX workbook-order sheet index (`--sheet N`, Phase 21.1.1). Also used
    /// as the sheet of an `--xlsx-cell` when both are given.
    #[cfg(feature = "xlsx")]
    sheet: Option<u32>,
    /// The XLSX cell reference (`--xlsx-cell A1`, Phase 21.1.1).
    #[cfg(feature = "xlsx")]
    xlsx_cell: Option<String>,
    /// `--xlsx-styles`: the parsed style table (Phase 21.1.2).
    #[cfg(feature = "xlsx")]
    xlsx_styles: bool,
    /// `--xlsx-defined-names`: the workbook defined/named ranges (Phase 21.1.2).
    #[cfg(feature = "xlsx")]
    xlsx_defined_names: bool,
    /// `--xlsx-external-rels`: the package external relationships (Phase 21.1.2).
    #[cfg(feature = "xlsx")]
    xlsx_external_rels: bool,
    /// `--xlsx-comments`: the comments of the `--sheet` worksheet (Phase 21.1.2).
    #[cfg(feature = "xlsx")]
    xlsx_comments: bool,
    /// `--xlsx-hyperlinks`: the hyperlinks of the `--sheet` worksheet (Phase 21.1.2).
    #[cfg(feature = "xlsx")]
    xlsx_hyperlinks: bool,
    /// `--xlsx-tables`: the tables of the `--sheet` worksheet (Phase 21.1.2).
    #[cfg(feature = "xlsx")]
    xlsx_tables: bool,
    /// `--xlsx-drawing`: the drawing of the `--sheet` worksheet (Phase 21.1.2).
    #[cfg(feature = "xlsx")]
    xlsx_drawing: bool,
    /// The PPTX presentation-order slide index (`--slide N`, Phase 21.2.1). Also
    /// used as the slide of `--pptx-shape`/`--pptx-tables` when given.
    #[cfg(feature = "pptx")]
    slide: Option<u32>,
    /// The PPTX shape index (`--pptx-shape I`, flattened pre-order within `--slide`).
    #[cfg(feature = "pptx")]
    pptx_shape: Option<u32>,
    /// The PPTX notes-slide index (`--pptx-notes N`, Phase 21.2.1).
    #[cfg(feature = "pptx")]
    pptx_notes: Option<u32>,
    /// `--pptx-layouts`: the slide-layout parts (Phase 21.2.1).
    #[cfg(feature = "pptx")]
    pptx_layouts: bool,
    /// `--pptx-masters`: the slide-master parts (Phase 21.2.1).
    #[cfg(feature = "pptx")]
    pptx_masters: bool,
    /// `--pptx-theme`: the theme parts (Phase 21.2.1).
    #[cfg(feature = "pptx")]
    pptx_theme: bool,
    /// `--pptx-media N`: the N-th media resource (Phase 21.2.1).
    #[cfg(feature = "pptx")]
    pptx_media: Option<u32>,
    /// `--pptx-tables`: the embedded tables of the `--slide` slide (Phase 21.2.1).
    #[cfg(feature = "pptx")]
    pptx_tables: bool,
    /// `--pptx-find`: a lexical text search over slides (Phase 21.2.1).
    #[cfg(feature = "pptx")]
    pptx_find: Option<String>,
    /// The ODS document-order sheet index (`--ods-sheet N`, Phase 21.3.1). Also
    /// used as the sheet of `--ods-cell`/`--ods-comments` when given.
    #[cfg(feature = "ods")]
    ods_sheet: Option<u32>,
    /// The ODS cell reference (`--ods-cell B7` or `--ods-cell row:col`, Phase 21.3.1).
    #[cfg(feature = "ods")]
    ods_cell: Option<String>,
    /// `--ods-styles`: the parsed cell styles and number formats (Phase 21.3.1).
    #[cfg(feature = "ods")]
    ods_styles: bool,
    /// `--ods-named-expressions`: the spreadsheet's named ranges/expressions
    /// (Phase 21.3.1).
    #[cfg(feature = "ods")]
    ods_named_expressions: bool,
    /// `--ods-comments`: the cell comments of the `--ods-sheet` sheet (Phase 21.3.1).
    #[cfg(feature = "ods")]
    ods_comments: bool,
    /// `--ods-find`: a lexical text search over sheet cells (Phase 21.3.1).
    #[cfg(feature = "ods")]
    ods_find: Option<String>,
    /// The ODP document-order slide index (`--odp-slide N`, Phase 21.4.1). Also used
    /// as the slide of `--odp-shape`/`--odp-tables` when given.
    #[cfg(feature = "odp")]
    odp_slide: Option<u32>,
    /// The ODP shape index (`--odp-shape I`, flattened pre-order within `--odp-slide`).
    #[cfg(feature = "odp")]
    odp_shape: Option<u32>,
    /// The ODP notes-slide index (`--odp-notes N`, Phase 21.4.1).
    #[cfg(feature = "odp")]
    odp_notes: Option<u32>,
    /// `--odp-masters`: the master pages (Phase 21.4.1).
    #[cfg(feature = "odp")]
    odp_masters: bool,
    /// `--odp-media N`: the N-th media resource (Phase 21.4.1).
    #[cfg(feature = "odp")]
    odp_media: Option<u32>,
    /// `--odp-tables`: the embedded tables of the `--odp-slide` slide (Phase 21.4.1).
    #[cfg(feature = "odp")]
    odp_tables: bool,
    /// `--odp-find`: a lexical text search over slides (Phase 21.4.1).
    #[cfg(feature = "odp")]
    odp_find: Option<String>,
    /// `--json-pointer POINTER`: resolve an RFC 6901 JSON pointer, returning the
    /// node's kind, exact source span, and exact token bytes (Phase 21.5.1).
    #[cfg(feature = "json")]
    json_pointer: Option<String>,
    /// `--json-node POINTER`: the structural view of a JSON node (kind, span,
    /// parent/child spans, member key/value spans) (Phase 21.5.1).
    #[cfg(feature = "json")]
    json_node: Option<String>,
    /// `--json-find`: a lexical, case-sensitive search over JSON keys/strings
    /// (Phase 21.5.1).
    #[cfg(feature = "json")]
    json_find: Option<String>,
    /// `--yaml-path PATH`: resolve a dotted YAML path (optional leading `docN`),
    /// returning the node's kind/style, exact source span, and exact token bytes
    /// (Phase 21.6.1).
    #[cfg(feature = "yaml")]
    yaml_path: Option<String>,
    /// `--yaml-node PATH`: the structural view of a YAML node (kind, style, span,
    /// parent/child spans, anchor/tag/alias, member key/value spans) (Phase 21.6.1).
    #[cfg(feature = "yaml")]
    yaml_node: Option<String>,
    /// `--yaml-documents`: the ordered YAML document list (Phase 21.6.1).
    #[cfg(feature = "yaml")]
    yaml_documents: bool,
    /// `--yaml-anchor NAME`: resolve a YAML anchor and its aliases (Phase 21.6.1).
    #[cfg(feature = "yaml")]
    yaml_anchor: Option<String>,
    /// `--yaml-find`: a lexical, case-sensitive search over YAML keys/scalars
    /// (Phase 21.6.1).
    #[cfg(feature = "yaml")]
    yaml_find: Option<String>,
    /// `--csv-row N`: a CSV/TSV record by 0-based physical index (the header row is
    /// index 0), returning its exact bytes, decoded text, or structural view
    /// (Phase 21.7.1).
    #[cfg(feature = "csv")]
    csv_row: Option<u32>,
    /// `--csv-cell R:C` or `--csv-cell R:COLNAME`: a CSV/TSV cell (Phase 21.7.1).
    #[cfg(feature = "csv")]
    csv_cell: Option<String>,
    /// `--csv-header`: the CSV/TSV header row (record 0) (Phase 21.7.1).
    #[cfg(feature = "csv")]
    csv_header: bool,
    /// `--csv-range R1:C1:R2:C2`: a CSV/TSV rectangular range of cells (Phase 21.7.1).
    #[cfg(feature = "csv")]
    csv_range: Option<String>,
    /// `--csv-find`: a lexical, case-sensitive search over CSV/TSV field text
    /// (Phase 21.7.1).
    #[cfg(feature = "csv")]
    csv_find: Option<String>,
    /// `--md-heading N`: the N-th ATX heading in document order (Phase 21.8.1).
    #[cfg(feature = "markdown")]
    md_heading: Option<u32>,
    /// `--md-block N`: the N-th block in document order (Phase 21.8.1).
    #[cfg(feature = "markdown")]
    md_block: Option<u32>,
    /// `--md-code N`: the N-th code block (fenced or indented) in document order
    /// (Phase 21.8.1).
    #[cfg(feature = "markdown")]
    md_code: Option<u32>,
    /// `--md-link N`: the N-th link or image in document order (Phase 21.8.1).
    #[cfg(feature = "markdown")]
    md_link: Option<u32>,
    /// `--md-find`: a lexical, case-sensitive search over Markdown block content
    /// (Phase 21.8.1).
    #[cfg(feature = "markdown")]
    md_find: Option<String>,
    /// `--xml-path P`: resolve a simple element path (`/a/b[2]/c`; `""` is the root
    /// element), returning the element's name, exact source span, and exact bytes
    /// (Phase 21.9).
    #[cfg(feature = "xml")]
    xml_path: Option<String>,
    /// `--xml-element P`: the structural view of an element (name, spans, attributes)
    /// at the same path (Phase 21.9).
    #[cfg(feature = "xml")]
    xml_element: Option<String>,
    /// `--xml-attr PATH@NAME`: an attribute's exact quoted value / spans (Phase 21.9).
    #[cfg(feature = "xml")]
    xml_attr: Option<String>,
    /// `--xml-namespaces`: every namespace declaration in document order (Phase 21.9).
    #[cfg(feature = "xml")]
    xml_namespaces: bool,
    /// `--xml-find`: a lexical, case-sensitive search over XML names/values/text
    /// (Phase 21.9).
    #[cfg(feature = "xml")]
    xml_find: Option<String>,
    /// `--html-path P`: resolve a simple element path (`/html/body[2]/p`; `""` is the
    /// root element), returning the element's name, exact source span, and exact
    /// bytes (Phase 21.10).
    #[cfg(feature = "html")]
    html_path: Option<String>,
    /// `--html-element P`: the structural view of an element (name, spans,
    /// attributes, quoting) at the same path (Phase 21.10).
    #[cfg(feature = "html")]
    html_element: Option<String>,
    /// `--html-attr PATH@NAME`: an attribute's exact value / spans (Phase 21.10).
    #[cfg(feature = "html")]
    html_attr: Option<String>,
    /// `--html-scripts`: every raw `<script>`/`<style>` element in document order
    /// (Phase 21.10). Their content is captured raw and never executed.
    #[cfg(feature = "html")]
    html_scripts: bool,
    /// `--html-find`: a lexical, case-sensitive search over HTML names/values/text
    /// (Phase 21.10).
    #[cfg(feature = "html")]
    html_find: Option<String>,
    /// `--toml-path P`: resolve a dotted path (`server.ports[0]`; `""` is the root
    /// table), returning the value's kind, exact spelling, and exact source span
    /// (Phase 21.11).
    #[cfg(feature = "toml")]
    toml_path: Option<String>,
    /// `--toml-table P`: the keys of the table at the dotted path (Phase 21.11).
    #[cfg(feature = "toml")]
    toml_table: Option<String>,
    /// `--toml-find`: a lexical, case-sensitive search over TOML keys and string
    /// values (Phase 21.11).
    #[cfg(feature = "toml")]
    toml_find: Option<String>,
    /// `--jsonl-line N`: the N-th JSONL record (0-based; blank lines do not count),
    /// returning its kind, exact line span, terminator, and exact value bytes
    /// (Phase 21.12).
    #[cfg(feature = "jsonl")]
    jsonl_line: Option<u32>,
    /// `--jsonl-pointer N:POINTER`: resolve an RFC 6901 pointer into record `N`
    /// (Phase 21.12).
    #[cfg(feature = "jsonl")]
    jsonl_pointer: Option<String>,
    /// `--jsonl-find`: a lexical, case-sensitive search over every JSONL record's
    /// keys and string values (Phase 21.12).
    #[cfg(feature = "jsonl")]
    jsonl_find: Option<String>,
    /// `--eml-header NAME`: every header named `NAME` (case-insensitive) across every
    /// message part, with exact spans (Phase 21.13).
    #[cfg(feature = "eml")]
    eml_header: Option<String>,
    /// `--eml-part N`: the N-th MIME part (0-based; the root message is 0), returning
    /// its kind, spans, and exact decoded bytes (Phase 21.13).
    #[cfg(feature = "eml")]
    eml_part: Option<u32>,
    /// `--eml-attachments`: every attachment leaf part in document order
    /// (Phase 21.13).
    #[cfg(feature = "eml")]
    eml_attachments: bool,
    /// `--eml-body`: the message body text (the first `text/plain` leaf, else the
    /// first `text/*`) (Phase 21.13).
    #[cfg(feature = "eml")]
    eml_body: bool,
    /// `--eml-find`: a lexical, case-sensitive search over every part's header
    /// names/values and decoded `text/*` bodies (Phase 21.13).
    #[cfg(feature = "eml")]
    eml_find: Option<String>,
    /// `--parquet-schema`: the Parquet schema (Phase 21.14).
    #[cfg(feature = "parquet")]
    parquet_schema: bool,
    /// `--parquet-column N`: the N-th logical leaf column (0-based): its inventory,
    /// decoded values (as `text`), or raw chunk bytes (as `exact`) (Phase 21.14).
    #[cfg(feature = "parquet")]
    parquet_column: Option<u32>,
    /// `--parquet-row-group N`: the N-th row group (0-based) inventory (Phase 21.14).
    #[cfg(feature = "parquet")]
    parquet_row_group: Option<u32>,
    /// `--parquet-cell R:C`: the decoded cell at whole-file row `R`, leaf column `C`
    /// (Phase 21.14).
    #[cfg(feature = "parquet")]
    parquet_cell: Option<(u32, u32)>,
    /// `--arrow-schema`: the Arrow IPC schema (Phase 21.16).
    #[cfg(feature = "arrow")]
    arrow_schema: bool,
    /// `--arrow-column NAME|N`: the column named `NAME` or the `N`-th top-level
    /// column (0-based): its inventory, decoded values (as `text`), or raw buffer
    /// bytes (as `exact`) (Phase 21.16).
    #[cfg(feature = "arrow")]
    arrow_column: Option<String>,
    /// `--arrow-batch N`: the N-th record batch (0-based) inventory (Phase 21.16).
    #[cfg(feature = "arrow")]
    arrow_batch: Option<u32>,
    /// `--arrow-cell R:C`: the decoded cell at whole-file row `R`, column `C`
    /// (a name or a 0-based index) (Phase 21.16).
    #[cfg(feature = "arrow")]
    arrow_cell: Option<(u64, String)>,
    output: Option<PathBuf>,
    content: Option<PathBuf>,
    /// `observe-batch`: the request file (a path, or `-` for stdin; default stdin).
    requests: Option<PathBuf>,
    /// `observe-batch`: repeat every request line this many times (>= 1).
    repeat: Option<u32>,
    /// `field-ingest`: the bounded worker-pool size (`--workers N`). Absent or `1`
    /// is serial; `0` is `available_parallelism`; `N > 1` is exactly `N` threads.
    /// Only honored by a build with the `parallel` feature.
    workers: Option<u32>,
    /// `field-build`: the fixed, non-searched reconstruction profile (`--profile
    /// NAME`). Absent means `runtime`.
    profile: Option<String>,
    /// `field-build`: an optional path to also write the exact `.voldoc`
    /// authority (`--voldoc OUT.voldoc`). The field stores the authority either
    /// way; this only makes a standalone copy for inspection/verification.
    voldoc: Option<PathBuf>,
    analyze: bool,
    json: bool,
    no_cache: bool,
    entropyfs: bool,
    /// `--packed`: serve seeds from `fieldpack/` segments instead of `seed/`
    /// (mutually exclusive with `--entropyfs`).
    packed: bool,
    /// `--sync=batch|each`: the packed writer's durability policy. Default
    /// `batch` (one fsync per segment); `each` restores one sync per seed node.
    /// Relevant only to `--packed` writes.
    sync_policy: vole_document::store::SyncPolicy,
    /// `--dir-sync=safe|off` (Phase 23): whether an atomic publish `fsync`s the
    /// containing directory after the rename. `safe` (the default) makes the
    /// rename durable across a power cut; `off` skips it (faster ingest, a lost
    /// rename is possible).
    dir_sync: vole_document::store::DirSyncPolicy,
    /// `--promote[=BYTES]`: opt-in durable promotion of reused intermediates
    /// (Phase 15.6). Off by default.
    promote: bool,
    /// `--promote=BYTES`: the durable promoted-store byte budget (Phase 15.6).
    promote_bytes: Option<u64>,
    positional: Vec<String>,
}

#[cfg(feature = "field")]
fn field_arg_value(
    args: &[String],
    i: &mut usize,
    flag: &str,
    inline: Option<&str>,
) -> Result<String> {
    match inline {
        Some(v) => {
            *i += 1;
            Ok(v.to_string())
        }
        None => {
            let v = args
                .get(*i + 1)
                .ok_or_else(|| Error::usage(format!("{flag} requires a value")))?;
            *i += 2;
            Ok(v.clone())
        }
    }
}

#[cfg(feature = "field")]
fn parse_field_args(args: &[String]) -> Result<FieldArgs> {
    let mut out = FieldArgs::default();
    let mut i = 2;
    while i < args.len() {
        let a = args[i].as_str();
        let (flag, inline) = match a.split_once('=') {
            Some((f, v)) => (f, Some(v)),
            None => (a, None),
        };
        match flag {
            "--analyze" => {
                out.analyze = true;
                i += 1;
            }
            "--json" => {
                out.json = true;
                i += 1;
            }
            "--exact" => i += 1,
            "--no-cache" => {
                out.no_cache = true;
                i += 1;
            }
            "--promote" => {
                out.promote = true;
                if let Some(v) = inline {
                    out.promote_bytes = Some(parse_promote_bytes(v)?);
                }
                i += 1;
            }
            "--entropyfs" => {
                out.entropyfs = true;
                i += 1;
            }
            "--packed" => {
                out.packed = true;
                i += 1;
            }
            "--sync" => {
                let v = field_arg_value(args, &mut i, "--sync", inline)?;
                out.sync_policy = match v.as_str() {
                    "batch" => vole_document::store::SyncPolicy::Batch,
                    "each" => vole_document::store::SyncPolicy::Each,
                    other => {
                        return Err(Error::usage(format!(
                            "--sync expects batch or each, got {other:?}"
                        )));
                    }
                };
            }
            "--dir-sync" => {
                let v = field_arg_value(args, &mut i, "--dir-sync", inline)?;
                out.dir_sync = match v.as_str() {
                    "safe" => vole_document::store::DirSyncPolicy::Safe,
                    "off" => vole_document::store::DirSyncPolicy::Off,
                    other => {
                        return Err(Error::usage(format!(
                            "--dir-sync expects safe or off, got {other:?}"
                        )));
                    }
                };
            }
            "--store" => {
                out.store = Some(PathBuf::from(field_arg_value(
                    args, &mut i, "--store", inline,
                )?));
            }
            "--field" => out.field = Some(field_arg_value(args, &mut i, "--field", inline)?),
            "--page" => {
                out.page = Some(parse_field_u32(
                    &field_arg_value(args, &mut i, "--page", inline)?,
                    "--page",
                )?);
            }
            "--object" => {
                out.object = Some(parse_field_u32(
                    &field_arg_value(args, &mut i, "--object", inline)?,
                    "--object",
                )?);
            }
            "--stream" => {
                out.stream = Some(parse_field_u32(
                    &field_arg_value(args, &mut i, "--stream", inline)?,
                    "--stream",
                )?);
            }
            "--revision" => {
                out.revision = Some(parse_field_u32(
                    &field_arg_value(args, &mut i, "--revision", inline)?,
                    "--revision",
                )?);
            }
            "--revisions" => {
                out.revisions = true;
                i += 1;
            }
            "--external-lineage" => {
                out.external_lineage = true;
                i += 1;
            }
            "--byte-range" => {
                out.byte_range = Some(parse_field_range(&field_arg_value(
                    args,
                    &mut i,
                    "--byte-range",
                    inline,
                )?)?);
            }
            "--kind" => out.kind = Some(field_arg_value(args, &mut i, "--kind", inline)?),
            "--text" => out.text = Some(field_arg_value(args, &mut i, "--text", inline)?),
            "--metadata" => {
                out.metadata = true;
                i += 1;
            }
            "--doc-text" => {
                out.doc_text = true;
                i += 1;
            }
            "--heading" => {
                out.heading = Some(parse_field_u32(
                    &field_arg_value(args, &mut i, "--heading", inline)?,
                    "--heading",
                )?);
            }
            "--block" => {
                out.block = Some(parse_field_u32(
                    &field_arg_value(args, &mut i, "--block", inline)?,
                    "--block",
                )?);
            }
            "--table" => {
                out.table = Some(parse_field_u32(
                    &field_arg_value(args, &mut i, "--table", inline)?,
                    "--table",
                )?);
            }
            "--cell" => {
                out.cell = Some(parse_cell_triple(&field_arg_value(
                    args, &mut i, "--cell", inline,
                )?)?);
            }
            "--resource" => {
                out.resource = Some(parse_field_u32(
                    &field_arg_value(args, &mut i, "--resource", inline)?,
                    "--resource",
                )?);
            }
            "--link" => {
                out.link = Some(parse_field_u32(
                    &field_arg_value(args, &mut i, "--link", inline)?,
                    "--link",
                )?);
            }
            #[cfg(feature = "epub")]
            "--spine-item" => {
                out.spine_item = Some(parse_field_u32(
                    &field_arg_value(args, &mut i, "--spine-item", inline)?,
                    "--spine-item",
                )?);
            }
            #[cfg(feature = "xlsx")]
            "--sheet" => {
                out.sheet = Some(parse_field_u32(
                    &field_arg_value(args, &mut i, "--sheet", inline)?,
                    "--sheet",
                )?);
            }
            #[cfg(feature = "xlsx")]
            "--xlsx-cell" => {
                out.xlsx_cell = Some(field_arg_value(args, &mut i, "--xlsx-cell", inline)?);
            }
            #[cfg(feature = "xlsx")]
            "--xlsx-styles" => {
                out.xlsx_styles = true;
                i += 1;
            }
            #[cfg(feature = "xlsx")]
            "--xlsx-defined-names" => {
                out.xlsx_defined_names = true;
                i += 1;
            }
            #[cfg(feature = "xlsx")]
            "--xlsx-external-rels" => {
                out.xlsx_external_rels = true;
                i += 1;
            }
            #[cfg(feature = "xlsx")]
            "--xlsx-comments" => {
                out.xlsx_comments = true;
                i += 1;
            }
            #[cfg(feature = "xlsx")]
            "--xlsx-hyperlinks" => {
                out.xlsx_hyperlinks = true;
                i += 1;
            }
            #[cfg(feature = "xlsx")]
            "--xlsx-tables" => {
                out.xlsx_tables = true;
                i += 1;
            }
            #[cfg(feature = "xlsx")]
            "--xlsx-drawing" => {
                out.xlsx_drawing = true;
                i += 1;
            }
            #[cfg(feature = "pptx")]
            "--slide" => {
                out.slide = Some(parse_field_u32(
                    &field_arg_value(args, &mut i, "--slide", inline)?,
                    "--slide",
                )?);
            }
            #[cfg(feature = "pptx")]
            "--pptx-shape" => {
                out.pptx_shape = Some(parse_field_u32(
                    &field_arg_value(args, &mut i, "--pptx-shape", inline)?,
                    "--pptx-shape",
                )?);
            }
            #[cfg(feature = "pptx")]
            "--pptx-notes" => {
                out.pptx_notes = Some(parse_field_u32(
                    &field_arg_value(args, &mut i, "--pptx-notes", inline)?,
                    "--pptx-notes",
                )?);
            }
            #[cfg(feature = "pptx")]
            "--pptx-layouts" => {
                out.pptx_layouts = true;
                i += 1;
            }
            #[cfg(feature = "pptx")]
            "--pptx-masters" => {
                out.pptx_masters = true;
                i += 1;
            }
            #[cfg(feature = "pptx")]
            "--pptx-theme" => {
                out.pptx_theme = true;
                i += 1;
            }
            #[cfg(feature = "pptx")]
            "--pptx-media" => {
                out.pptx_media = Some(parse_field_u32(
                    &field_arg_value(args, &mut i, "--pptx-media", inline)?,
                    "--pptx-media",
                )?);
            }
            #[cfg(feature = "pptx")]
            "--pptx-tables" => {
                out.pptx_tables = true;
                i += 1;
            }
            #[cfg(feature = "pptx")]
            "--pptx-find" => {
                out.pptx_find = Some(field_arg_value(args, &mut i, "--pptx-find", inline)?);
            }
            #[cfg(feature = "ods")]
            "--ods-sheet" => {
                out.ods_sheet = Some(parse_field_u32(
                    &field_arg_value(args, &mut i, "--ods-sheet", inline)?,
                    "--ods-sheet",
                )?);
            }
            #[cfg(feature = "ods")]
            "--ods-cell" => {
                out.ods_cell = Some(field_arg_value(args, &mut i, "--ods-cell", inline)?);
            }
            #[cfg(feature = "ods")]
            "--ods-styles" => {
                out.ods_styles = true;
                i += 1;
            }
            #[cfg(feature = "ods")]
            "--ods-named-expressions" => {
                out.ods_named_expressions = true;
                i += 1;
            }
            #[cfg(feature = "ods")]
            "--ods-comments" => {
                out.ods_comments = true;
                i += 1;
            }
            #[cfg(feature = "ods")]
            "--ods-find" => {
                out.ods_find = Some(field_arg_value(args, &mut i, "--ods-find", inline)?);
            }
            #[cfg(feature = "odp")]
            "--odp-slide" => {
                out.odp_slide = Some(parse_field_u32(
                    &field_arg_value(args, &mut i, "--odp-slide", inline)?,
                    "--odp-slide",
                )?);
            }
            #[cfg(feature = "odp")]
            "--odp-shape" => {
                out.odp_shape = Some(parse_field_u32(
                    &field_arg_value(args, &mut i, "--odp-shape", inline)?,
                    "--odp-shape",
                )?);
            }
            #[cfg(feature = "odp")]
            "--odp-notes" => {
                out.odp_notes = Some(parse_field_u32(
                    &field_arg_value(args, &mut i, "--odp-notes", inline)?,
                    "--odp-notes",
                )?);
            }
            #[cfg(feature = "odp")]
            "--odp-masters" => {
                out.odp_masters = true;
                i += 1;
            }
            #[cfg(feature = "odp")]
            "--odp-media" => {
                out.odp_media = Some(parse_field_u32(
                    &field_arg_value(args, &mut i, "--odp-media", inline)?,
                    "--odp-media",
                )?);
            }
            #[cfg(feature = "odp")]
            "--odp-tables" => {
                out.odp_tables = true;
                i += 1;
            }
            #[cfg(feature = "odp")]
            "--odp-find" => {
                out.odp_find = Some(field_arg_value(args, &mut i, "--odp-find", inline)?);
            }
            #[cfg(feature = "json")]
            "--json-pointer" => {
                out.json_pointer = Some(field_arg_value(args, &mut i, "--json-pointer", inline)?);
            }
            #[cfg(feature = "json")]
            "--json-node" => {
                out.json_node = Some(field_arg_value(args, &mut i, "--json-node", inline)?);
            }
            #[cfg(feature = "json")]
            "--json-find" => {
                out.json_find = Some(field_arg_value(args, &mut i, "--json-find", inline)?);
            }
            #[cfg(feature = "yaml")]
            "--yaml-path" => {
                out.yaml_path = Some(field_arg_value(args, &mut i, "--yaml-path", inline)?);
            }
            #[cfg(feature = "yaml")]
            "--yaml-node" => {
                out.yaml_node = Some(field_arg_value(args, &mut i, "--yaml-node", inline)?);
            }
            #[cfg(feature = "yaml")]
            "--yaml-documents" => {
                out.yaml_documents = true;
                i += 1;
            }
            #[cfg(feature = "yaml")]
            "--yaml-anchor" => {
                out.yaml_anchor = Some(field_arg_value(args, &mut i, "--yaml-anchor", inline)?);
            }
            #[cfg(feature = "yaml")]
            "--yaml-find" => {
                out.yaml_find = Some(field_arg_value(args, &mut i, "--yaml-find", inline)?);
            }
            #[cfg(feature = "csv")]
            "--csv-row" => {
                let v = field_arg_value(args, &mut i, "--csv-row", inline)?;
                out.csv_row = Some(v.parse().map_err(|_| {
                    Error::usage(format!(
                        "--csv-row value {v:?} is not a non-negative integer"
                    ))
                })?);
            }
            #[cfg(feature = "csv")]
            "--csv-cell" => {
                out.csv_cell = Some(field_arg_value(args, &mut i, "--csv-cell", inline)?);
            }
            #[cfg(feature = "csv")]
            "--csv-header" => {
                out.csv_header = true;
                i += 1;
            }
            #[cfg(feature = "csv")]
            "--csv-range" => {
                out.csv_range = Some(field_arg_value(args, &mut i, "--csv-range", inline)?);
            }
            #[cfg(feature = "csv")]
            "--csv-find" => {
                out.csv_find = Some(field_arg_value(args, &mut i, "--csv-find", inline)?);
            }
            #[cfg(feature = "markdown")]
            "--md-heading" => {
                out.md_heading = Some(parse_field_u32(
                    &field_arg_value(args, &mut i, "--md-heading", inline)?,
                    "--md-heading",
                )?);
            }
            #[cfg(feature = "markdown")]
            "--md-block" => {
                out.md_block = Some(parse_field_u32(
                    &field_arg_value(args, &mut i, "--md-block", inline)?,
                    "--md-block",
                )?);
            }
            #[cfg(feature = "markdown")]
            "--md-code" => {
                out.md_code = Some(parse_field_u32(
                    &field_arg_value(args, &mut i, "--md-code", inline)?,
                    "--md-code",
                )?);
            }
            #[cfg(feature = "markdown")]
            "--md-link" => {
                out.md_link = Some(parse_field_u32(
                    &field_arg_value(args, &mut i, "--md-link", inline)?,
                    "--md-link",
                )?);
            }
            #[cfg(feature = "markdown")]
            "--md-find" => {
                out.md_find = Some(field_arg_value(args, &mut i, "--md-find", inline)?);
            }
            #[cfg(feature = "xml")]
            "--xml-path" => {
                out.xml_path = Some(field_arg_value(args, &mut i, "--xml-path", inline)?);
            }
            #[cfg(feature = "xml")]
            "--xml-element" => {
                out.xml_element = Some(field_arg_value(args, &mut i, "--xml-element", inline)?);
            }
            #[cfg(feature = "xml")]
            "--xml-attr" => {
                out.xml_attr = Some(field_arg_value(args, &mut i, "--xml-attr", inline)?);
            }
            #[cfg(feature = "xml")]
            "--xml-namespaces" => {
                out.xml_namespaces = true;
                i += 1;
            }
            #[cfg(feature = "xml")]
            "--xml-find" => {
                out.xml_find = Some(field_arg_value(args, &mut i, "--xml-find", inline)?);
            }
            #[cfg(feature = "html")]
            "--html-path" => {
                out.html_path = Some(field_arg_value(args, &mut i, "--html-path", inline)?);
            }
            #[cfg(feature = "html")]
            "--html-element" => {
                out.html_element = Some(field_arg_value(args, &mut i, "--html-element", inline)?);
            }
            #[cfg(feature = "html")]
            "--html-attr" => {
                out.html_attr = Some(field_arg_value(args, &mut i, "--html-attr", inline)?);
            }
            #[cfg(feature = "html")]
            "--html-scripts" => {
                out.html_scripts = true;
                i += 1;
            }
            #[cfg(feature = "html")]
            "--html-find" => {
                out.html_find = Some(field_arg_value(args, &mut i, "--html-find", inline)?);
            }
            #[cfg(feature = "toml")]
            "--toml-path" => {
                out.toml_path = Some(field_arg_value(args, &mut i, "--toml-path", inline)?);
            }
            #[cfg(feature = "toml")]
            "--toml-table" => {
                out.toml_table = Some(field_arg_value(args, &mut i, "--toml-table", inline)?);
            }
            #[cfg(feature = "toml")]
            "--toml-find" => {
                out.toml_find = Some(field_arg_value(args, &mut i, "--toml-find", inline)?);
            }
            #[cfg(feature = "jsonl")]
            "--jsonl-line" => {
                let v = field_arg_value(args, &mut i, "--jsonl-line", inline)?;
                out.jsonl_line = Some(parse_field_u32(&v, "--jsonl-line")?);
            }
            #[cfg(feature = "jsonl")]
            "--jsonl-pointer" => {
                out.jsonl_pointer = Some(field_arg_value(args, &mut i, "--jsonl-pointer", inline)?);
            }
            #[cfg(feature = "jsonl")]
            "--jsonl-find" => {
                out.jsonl_find = Some(field_arg_value(args, &mut i, "--jsonl-find", inline)?);
            }
            #[cfg(feature = "eml")]
            "--eml-header" => {
                out.eml_header = Some(field_arg_value(args, &mut i, "--eml-header", inline)?);
            }
            #[cfg(feature = "eml")]
            "--eml-part" => {
                let v = field_arg_value(args, &mut i, "--eml-part", inline)?;
                out.eml_part = Some(parse_field_u32(&v, "--eml-part")?);
            }
            #[cfg(feature = "eml")]
            "--eml-attachments" => {
                out.eml_attachments = true;
                i += 1;
            }
            #[cfg(feature = "eml")]
            "--eml-body" => {
                out.eml_body = true;
                i += 1;
            }
            #[cfg(feature = "eml")]
            "--eml-find" => {
                out.eml_find = Some(field_arg_value(args, &mut i, "--eml-find", inline)?);
            }
            #[cfg(feature = "parquet")]
            "--parquet-schema" => {
                out.parquet_schema = true;
                i += 1;
            }
            #[cfg(feature = "parquet")]
            "--parquet-column" => {
                let v = field_arg_value(args, &mut i, "--parquet-column", inline)?;
                out.parquet_column = Some(parse_field_u32(&v, "--parquet-column")?);
            }
            #[cfg(feature = "parquet")]
            "--parquet-row-group" => {
                let v = field_arg_value(args, &mut i, "--parquet-row-group", inline)?;
                out.parquet_row_group = Some(parse_field_u32(&v, "--parquet-row-group")?);
            }
            #[cfg(feature = "parquet")]
            "--parquet-cell" => {
                let v = field_arg_value(args, &mut i, "--parquet-cell", inline)?;
                out.parquet_cell = Some(parse_cell_pair(&v)?);
            }
            #[cfg(feature = "arrow")]
            "--arrow-schema" => {
                out.arrow_schema = true;
                i += 1;
            }
            #[cfg(feature = "arrow")]
            "--arrow-column" => {
                out.arrow_column = Some(field_arg_value(args, &mut i, "--arrow-column", inline)?);
            }
            #[cfg(feature = "arrow")]
            "--arrow-batch" => {
                let v = field_arg_value(args, &mut i, "--arrow-batch", inline)?;
                out.arrow_batch = Some(parse_field_u32(&v, "--arrow-batch")?);
            }
            #[cfg(feature = "arrow")]
            "--arrow-cell" => {
                let v = field_arg_value(args, &mut i, "--arrow-cell", inline)?;
                out.arrow_cell = Some(parse_arrow_cell(&v)?);
            }
            "--output" => {
                out.output = Some(PathBuf::from(field_arg_value(
                    args, &mut i, "--output", inline,
                )?));
            }
            "--content" => {
                out.content = Some(PathBuf::from(field_arg_value(
                    args,
                    &mut i,
                    "--content",
                    inline,
                )?));
            }
            "--requests" => {
                out.requests = Some(PathBuf::from(field_arg_value(
                    args,
                    &mut i,
                    "--requests",
                    inline,
                )?));
            }
            "--workers" => {
                out.workers = Some(parse_field_u32(
                    &field_arg_value(args, &mut i, "--workers", inline)?,
                    "--workers",
                )?);
            }
            "--profile" => {
                out.profile = Some(field_arg_value(args, &mut i, "--profile", inline)?);
            }
            "--voldoc" => {
                out.voldoc = Some(PathBuf::from(field_arg_value(
                    args, &mut i, "--voldoc", inline,
                )?));
            }
            "--repeat" => {
                out.repeat = Some(parse_field_u32(
                    &field_arg_value(args, &mut i, "--repeat", inline)?,
                    "--repeat",
                )?);
            }
            other if !other.starts_with("--") => {
                out.positional.push(other.to_string());
                i += 1;
            }
            other => return Err(Error::usage(format!("unknown field argument {other:?}"))),
        }
    }
    // Phase 23: the directory-durability policy is process-wide (it also guards
    // the free `write_atomic`), so install it as soon as the arguments are known.
    vole_document::store::set_dir_sync_policy(out.dir_sync);
    Ok(out)
}

#[cfg(feature = "field")]
fn parse_field_u32(value: &str, flag: &str) -> Result<u32> {
    value
        .parse()
        .map_err(|_| Error::usage(format!("{flag} value {value:?} is not a u32")))
}

#[cfg(feature = "field")]
fn parse_field_range(value: &str) -> Result<(u64, u64)> {
    let (a, b) = value
        .split_once("..")
        .ok_or_else(|| Error::usage("--byte-range must be A..B (e.g. 0..128)"))?;
    let start: u64 = a
        .parse()
        .map_err(|_| Error::usage(format!("--byte-range start {a:?} is not a u64")))?;
    let end: u64 = b
        .parse()
        .map_err(|_| Error::usage(format!("--byte-range end {b:?} is not a u64")))?;
    if end < start {
        return Err(Error::usage("--byte-range end precedes its start"));
    }
    Ok((start, end - start))
}

#[cfg(feature = "field")]
fn parse_cell_triple(value: &str) -> Result<(u32, u32, u32)> {
    let parts: Vec<&str> = value.split(':').collect();
    if parts.len() != 3 {
        return Err(Error::usage("--cell must be TABLE:ROW:COL (e.g. 0:1:1)"));
    }
    let mut out = [0u32; 3];
    for (i, p) in parts.iter().enumerate() {
        out[i] = p
            .parse()
            .map_err(|_| Error::usage(format!("--cell component {p:?} is not a u32")))?;
    }
    Ok((out[0], out[1], out[2]))
}

#[cfg(feature = "field")]
fn parse_cell_pair(value: &str) -> Result<(u32, u32)> {
    let parts: Vec<&str> = value.split(':').collect();
    if parts.len() != 2 {
        return Err(Error::usage("--parquet-cell must be ROW:COL (e.g. 0:1)"));
    }
    let mut out = [0u32; 2];
    for (i, p) in parts.iter().enumerate() {
        out[i] = p
            .parse()
            .map_err(|_| Error::usage(format!("--parquet-cell component {p:?} is not a u32")))?;
    }
    Ok((out[0], out[1]))
}

/// Parse `--arrow-cell R:C` where `R` is a whole-file row (u64) and `C` is a column
/// name or a 0-based top-level column index (Phase 21.16).
#[cfg(all(feature = "field", feature = "arrow"))]
fn parse_arrow_cell(value: &str) -> Result<(u64, String)> {
    let (row, col) = value
        .split_once(':')
        .ok_or_else(|| Error::usage("--arrow-cell must be ROW:COL (e.g. 0:1)"))?;
    let row: u64 = row
        .parse()
        .map_err(|_| Error::usage(format!("--arrow-cell row {row:?} is not a u64")))?;
    if col.is_empty() {
        return Err(Error::usage("--arrow-cell column is empty"));
    }
    Ok((row, col.to_string()))
}

#[cfg(feature = "field")]
fn field_selector(out: &FieldArgs) -> Result<Selector> {
    let mut chosen: Vec<Selector> = Vec::new();
    if let Some(n) = out.page {
        chosen.push(Selector::Page(n));
    }
    if let Some(n) = out.object {
        chosen.push(Selector::Object(n));
    }
    if let Some(n) = out.stream {
        chosen.push(Selector::Stream(n));
    }
    if let Some(n) = out.revision {
        chosen.push(Selector::Revision(n));
    }
    if out.revisions {
        chosen.push(Selector::Revisions);
    }
    if out.external_lineage {
        chosen.push(Selector::ExternalLineage);
    }
    if let Some((offset, len)) = out.byte_range {
        chosen.push(Selector::ByteRange { offset, len });
    }
    if let Some(text) = &out.text {
        chosen.push(Selector::SearchMatch(text.clone()));
    }
    if out.metadata {
        chosen.push(Selector::Metadata);
    }
    if out.doc_text {
        chosen.push(Selector::Text);
    }
    if let Some(n) = out.heading {
        chosen.push(Selector::Heading(n));
    }
    if let Some(n) = out.block {
        chosen.push(Selector::Block(n));
    }
    if let Some(n) = out.table {
        chosen.push(Selector::Table(n));
    }
    if let Some((table, row, col)) = out.cell {
        chosen.push(Selector::Cell { table, row, col });
    }
    if let Some(n) = out.resource {
        chosen.push(Selector::Resource(n));
    }
    if let Some(n) = out.link {
        chosen.push(Selector::Link(n));
    }
    #[cfg(feature = "epub")]
    if let Some(n) = out.spine_item {
        chosen.push(Selector::EpubSpineItem {
            index: n,
            profile: vole_document::adapter::epub::EpubExtractProfile::DEFAULT,
        });
    }
    // XLSX: `--xlsx-cell A1` is the cell selector and consumes `--sheet N` as its
    // sheet; `--sheet N` alone is the native sheet selector. The Phase-21.1.2
    // flags select the styles table, defined names, external relationships, or a
    // per-sheet comments/hyperlinks/tables/drawing observation (each with `--sheet`).
    #[cfg(feature = "xlsx")]
    {
        let profile = vole_document::adapter::xlsx::XlsxExtractProfile::DEFAULT;
        let sheet = out.sheet.unwrap_or(0);
        let mut specific = false;
        if out.xlsx_styles {
            chosen.push(Selector::XlsxStyles);
            specific = true;
        }
        if out.xlsx_defined_names {
            chosen.push(Selector::XlsxDefinedNames);
            specific = true;
        }
        if out.xlsx_external_rels {
            chosen.push(Selector::XlsxExternalRels);
            specific = true;
        }
        if out.xlsx_comments {
            chosen.push(Selector::XlsxComments { sheet });
            specific = true;
        }
        if out.xlsx_hyperlinks {
            chosen.push(Selector::XlsxHyperlinks { sheet });
            specific = true;
        }
        if out.xlsx_tables {
            chosen.push(Selector::XlsxTables { sheet });
            specific = true;
        }
        if out.xlsx_drawing {
            chosen.push(Selector::XlsxDrawing { sheet });
            specific = true;
        }
        if let Some(cell) = &out.xlsx_cell {
            chosen.push(Selector::XlsxCell {
                sheet,
                cell: cell.clone(),
                profile,
            });
            specific = true;
        }
        // `--sheet N` alone is the native sheet selector; when any other XLSX flag
        // consumed it, it is not added again.
        if !specific && let Some(index) = out.sheet {
            chosen.push(Selector::XlsxSheet { index, profile });
        }
    }
    // PPTX: `--pptx-shape I` consumes `--slide N`; `--slide N` alone is the native
    // slide selector. `--pptx-tables` consumes `--slide N`. The list selectors and
    // `--pptx-notes`/`--pptx-media` stand alone.
    #[cfg(feature = "pptx")]
    {
        let profile = vole_document::adapter::pptx::PptxExtractProfile::DEFAULT;
        let slide = out.slide.unwrap_or(0);
        let mut specific = false;
        if out.pptx_layouts {
            chosen.push(Selector::PptxLayouts);
            specific = true;
        }
        if out.pptx_masters {
            chosen.push(Selector::PptxMasters);
            specific = true;
        }
        if out.pptx_theme {
            chosen.push(Selector::PptxTheme);
            specific = true;
        }
        if out.pptx_tables {
            chosen.push(Selector::PptxTables { slide, profile });
            specific = true;
        }
        if let Some(index) = out.pptx_notes {
            chosen.push(Selector::PptxNotes { index, profile });
            specific = true;
        }
        if let Some(ordinal) = out.pptx_media {
            chosen.push(Selector::PptxMedia { ordinal });
            specific = true;
        }
        if let Some(index) = out.pptx_shape {
            chosen.push(Selector::PptxShape {
                slide,
                index,
                profile,
            });
            specific = true;
        }
        if let Some(pattern) = &out.pptx_find {
            chosen.push(Selector::PptxFind {
                pattern: pattern.clone(),
                profile,
            });
            specific = true;
        }
        // `--slide N` alone is the native slide selector; when another PPTX flag
        // consumed it, it is not added again.
        if !specific && let Some(index) = out.slide {
            chosen.push(Selector::PptxSlide { index, profile });
        }
    }
    // ODS: `--ods-cell B7` is the cell selector and consumes `--ods-sheet N` as its
    // sheet; `--ods-sheet N` alone is the native sheet selector. `--ods-comments`
    // consumes `--ods-sheet`; the styles/named-expressions/find flags stand alone.
    #[cfg(feature = "ods")]
    {
        let profile = vole_document::adapter::ods::OdsExtractProfile::DEFAULT;
        let sheet = out.ods_sheet.unwrap_or(0);
        let mut specific = false;
        if out.ods_styles {
            chosen.push(Selector::OdsStyles);
            specific = true;
        }
        if out.ods_named_expressions {
            chosen.push(Selector::OdsNamedExpressions);
            specific = true;
        }
        if out.ods_comments {
            chosen.push(Selector::OdsComments { sheet });
            specific = true;
        }
        if let Some(cell) = &out.ods_cell {
            chosen.push(Selector::OdsCell {
                sheet,
                cell: cell.clone(),
                profile,
            });
            specific = true;
        }
        if let Some(pattern) = &out.ods_find {
            chosen.push(Selector::OdsFind {
                pattern: pattern.clone(),
                profile,
            });
            specific = true;
        }
        if !specific && let Some(index) = out.ods_sheet {
            chosen.push(Selector::OdsSheet { index, profile });
        }
    }
    // ODP: `--odp-shape I` consumes `--odp-slide N`; `--odp-slide N` alone is the
    // native slide selector. `--odp-tables` consumes `--odp-slide`. The list selectors
    // and `--odp-notes`/`--odp-media` stand alone.
    #[cfg(feature = "odp")]
    {
        let profile = vole_document::adapter::odp::OdpExtractProfile::DEFAULT;
        let slide = out.odp_slide.unwrap_or(0);
        let mut specific = false;
        if out.odp_masters {
            chosen.push(Selector::OdpMasters);
            specific = true;
        }
        if out.odp_tables {
            chosen.push(Selector::OdpTables { slide, profile });
            specific = true;
        }
        if let Some(index) = out.odp_notes {
            chosen.push(Selector::OdpNotes { index, profile });
            specific = true;
        }
        if let Some(ordinal) = out.odp_media {
            chosen.push(Selector::OdpMedia { ordinal });
            specific = true;
        }
        if let Some(index) = out.odp_shape {
            chosen.push(Selector::OdpShape {
                slide,
                index,
                profile,
            });
            specific = true;
        }
        if let Some(pattern) = &out.odp_find {
            chosen.push(Selector::OdpFind {
                pattern: pattern.clone(),
                profile,
            });
            specific = true;
        }
        // `--odp-slide N` alone is the native slide selector; when another ODP flag
        // consumed it, it is not added again.
        if !specific && let Some(index) = out.odp_slide {
            chosen.push(Selector::OdpSlide { index, profile });
        }
    }
    // JSON: `--json-pointer`/`--json-node` address a node by RFC 6901 pointer;
    // `--json-find` is a lexical search. Each stands alone (Phase 21.5.1).
    #[cfg(feature = "json")]
    {
        if let Some(pointer) = &out.json_pointer {
            chosen.push(Selector::JsonPointer {
                pointer: pointer.clone(),
            });
        }
        if let Some(pointer) = &out.json_node {
            chosen.push(Selector::JsonNode {
                pointer: pointer.clone(),
            });
        }
        if let Some(pattern) = &out.json_find {
            chosen.push(Selector::JsonFind {
                pattern: pattern.clone(),
            });
        }
    }
    // YAML: `--yaml-path`/`--yaml-node` address a node by dotted path;
    // `--yaml-documents` lists documents; `--yaml-anchor` resolves an anchor and its
    // aliases; `--yaml-find` is a lexical search. Each stands alone (Phase 21.6.1).
    #[cfg(feature = "yaml")]
    {
        if let Some(path) = &out.yaml_path {
            chosen.push(Selector::YamlPath { path: path.clone() });
        }
        if let Some(path) = &out.yaml_node {
            chosen.push(Selector::YamlNode { path: path.clone() });
        }
        if out.yaml_documents {
            chosen.push(Selector::YamlDocuments);
        }
        if let Some(name) = &out.yaml_anchor {
            chosen.push(Selector::YamlAnchor { name: name.clone() });
        }
        if let Some(pattern) = &out.yaml_find {
            chosen.push(Selector::YamlFind {
                pattern: pattern.clone(),
            });
        }
    }
    // CSV/TSV: `--csv-row`/`--csv-cell`/`--csv-range` address records, cells, and
    // rectangles; `--csv-header` is the header row; `--csv-find` is a lexical
    // search. Each stands alone (Phase 21.7.1).
    #[cfg(feature = "csv")]
    {
        if let Some(index) = out.csv_row {
            chosen.push(Selector::CsvRow { index });
        }
        if let Some(spec) = &out.csv_cell {
            chosen.push(Selector::CsvCell { spec: spec.clone() });
        }
        if out.csv_header {
            chosen.push(Selector::CsvHeader);
        }
        if let Some(spec) = &out.csv_range {
            chosen.push(Selector::CsvRange { spec: spec.clone() });
        }
        if let Some(pattern) = &out.csv_find {
            chosen.push(Selector::CsvFind {
                pattern: pattern.clone(),
            });
        }
    }
    // Markdown: `--md-heading`/`--md-block`/`--md-code`/`--md-link` address
    // headings, blocks, code blocks, and links/images; `--md-find` is a lexical
    // search. Each stands alone (Phase 21.8.1).
    #[cfg(feature = "markdown")]
    {
        if let Some(index) = out.md_heading {
            chosen.push(Selector::MdHeading { index });
        }
        if let Some(index) = out.md_block {
            chosen.push(Selector::MdBlock { index });
        }
        if let Some(index) = out.md_code {
            chosen.push(Selector::MdCode { index });
        }
        if let Some(index) = out.md_link {
            chosen.push(Selector::MdLink { index });
        }
        if let Some(pattern) = &out.md_find {
            chosen.push(Selector::MdFind {
                pattern: pattern.clone(),
            });
        }
    }
    // XML: `--xml-path`/`--xml-element` address a node by element path; `--xml-attr`
    // addresses an attribute as `PATH@NAME`; `--xml-namespaces` lists declarations;
    // `--xml-find` is a lexical search. Each stands alone (Phase 21.9).
    #[cfg(feature = "xml")]
    {
        if let Some(path) = &out.xml_path {
            chosen.push(Selector::XmlPath { path: path.clone() });
        }
        if let Some(path) = &out.xml_element {
            chosen.push(Selector::XmlElement { path: path.clone() });
        }
        if let Some(spec) = &out.xml_attr {
            chosen.push(Selector::XmlAttr { spec: spec.clone() });
        }
        if out.xml_namespaces {
            chosen.push(Selector::XmlNamespaces);
        }
        if let Some(pattern) = &out.xml_find {
            chosen.push(Selector::XmlFind {
                pattern: pattern.clone(),
            });
        }
    }
    // HTML: `--html-path`/`--html-element` address a node by element path;
    // `--html-attr` addresses an attribute as `PATH@NAME`; `--html-scripts` lists
    // raw script/style elements; `--html-find` is a lexical search. Each stands alone
    // (Phase 21.10).
    #[cfg(feature = "html")]
    {
        if let Some(path) = &out.html_path {
            chosen.push(Selector::HtmlPath { path: path.clone() });
        }
        if let Some(path) = &out.html_element {
            chosen.push(Selector::HtmlElement { path: path.clone() });
        }
        if let Some(spec) = &out.html_attr {
            chosen.push(Selector::HtmlAttr { spec: spec.clone() });
        }
        if out.html_scripts {
            chosen.push(Selector::HtmlScripts);
        }
        if let Some(pattern) = &out.html_find {
            chosen.push(Selector::HtmlFind {
                pattern: pattern.clone(),
            });
        }
    }
    // TOML: `--toml-path` addresses a value by dotted path; `--toml-table` lists a
    // table's keys; `--toml-find` is a lexical search. Each stands alone (Phase 21.11).
    #[cfg(feature = "toml")]
    {
        if let Some(path) = &out.toml_path {
            chosen.push(Selector::TomlPath { path: path.clone() });
        }
        if let Some(path) = &out.toml_table {
            chosen.push(Selector::TomlTable { path: path.clone() });
        }
        if let Some(pattern) = &out.toml_find {
            chosen.push(Selector::TomlFind {
                pattern: pattern.clone(),
            });
        }
    }
    // JSONL: `--jsonl-line` addresses a record by 0-based index; `--jsonl-pointer`
    // addresses a node as `N:POINTER`; `--jsonl-find` is a lexical search. Each
    // stands alone (Phase 21.12).
    #[cfg(feature = "jsonl")]
    {
        if let Some(index) = out.jsonl_line {
            chosen.push(Selector::JsonlLine { index });
        }
        if let Some(spec) = &out.jsonl_pointer {
            chosen.push(Selector::JsonlPointer { spec: spec.clone() });
        }
        if let Some(pattern) = &out.jsonl_find {
            chosen.push(Selector::JsonlFind {
                pattern: pattern.clone(),
            });
        }
    }
    // EML: `--eml-header` lists headers by name; `--eml-part` addresses a MIME part
    // by 0-based index; `--eml-attachments` lists attachments; `--eml-body` returns
    // the body text; `--eml-find` is a lexical search. Each stands alone
    // (Phase 21.13).
    #[cfg(feature = "eml")]
    {
        if let Some(name) = &out.eml_header {
            chosen.push(Selector::EmlHeader { name: name.clone() });
        }
        if let Some(index) = out.eml_part {
            chosen.push(Selector::EmlPart { index });
        }
        if out.eml_attachments {
            chosen.push(Selector::EmlAttachments);
        }
        if out.eml_body {
            chosen.push(Selector::EmlBody);
        }
        if let Some(pattern) = &out.eml_find {
            chosen.push(Selector::EmlFind {
                pattern: pattern.clone(),
            });
        }
    }
    // Parquet: `--parquet-schema` lists the schema; `--parquet-column N` addresses a
    // leaf column; `--parquet-row-group N` a row group; `--parquet-cell R:C` a cell.
    // Each stands alone (Phase 21.14).
    #[cfg(feature = "parquet")]
    {
        if out.parquet_schema {
            chosen.push(Selector::ParquetSchema);
        }
        if let Some(index) = out.parquet_column {
            chosen.push(Selector::ParquetColumn { index });
        }
        if let Some(index) = out.parquet_row_group {
            chosen.push(Selector::ParquetRowGroup { index });
        }
        if let Some((row, col)) = out.parquet_cell {
            chosen.push(Selector::ParquetCell {
                row: u64::from(row),
                col,
            });
        }
    }
    // Arrow IPC: `--arrow-schema` lists the schema; `--arrow-column NAME|N`
    // addresses a column; `--arrow-batch N` a record batch; `--arrow-cell R:C` a
    // cell. Each stands alone (Phase 21.16).
    #[cfg(feature = "arrow")]
    {
        if out.arrow_schema {
            chosen.push(Selector::ArrowSchema);
        }
        if let Some(spec) = &out.arrow_column {
            chosen.push(Selector::ArrowColumn { spec: spec.clone() });
        }
        if let Some(index) = out.arrow_batch {
            chosen.push(Selector::ArrowBatch { index });
        }
        if let Some((row, col)) = &out.arrow_cell {
            chosen.push(Selector::ArrowCell {
                row: *row,
                col: col.clone(),
            });
        }
    }
    match chosen.len() {
        0 => Err(Error::usage("exactly one selector flag is required")),
        1 => chosen
            .into_iter()
            .next()
            .ok_or_else(|| Error::usage("no selector")),
        _ => Err(Error::usage(
            "exactly one selector flag is required; more than one was given",
        )),
    }
}

#[cfg(feature = "field")]
fn field_representation(kind: &str) -> Result<Representation> {
    Ok(match kind {
        "metadata" => Representation::Metadata,
        "text" => Representation::Text,
        "structure" => Representation::Structure,
        "operators" => Representation::Operators,
        "encoded" => Representation::EncodedBytes,
        "decoded" => Representation::DecodedBytes,
        "exact" => Representation::ExactBytes,
        "preview" => Representation::Preview,
        "lineage" => Representation::Lineage,
        "full" => Representation::FullDocument,
        other => return Err(Error::usage(format!("unknown --kind {other:?}"))),
    })
}

#[cfg(feature = "field")]
fn observe_request(
    out: &FieldArgs,
    selector: Selector,
    representation: Representation,
) -> ObserveRequest {
    let mut req = ObserveRequest::new(selector, representation);
    req.use_cache = !out.no_cache;
    req
}

/// `--promote=BYTES`: a positive byte budget for the durable promoted store.
#[cfg(feature = "field")]
fn parse_promote_bytes(value: &str) -> Result<u64> {
    let n: u64 = value
        .parse()
        .map_err(|_| Error::usage(format!("--promote value {value:?} is not a byte count")))?;
    if n == 0 {
        return Err(Error::usage("--promote budget must be greater than zero"));
    }
    Ok(n)
}

/// The promotion policy selected by `--promote[=BYTES]` (disabled by default).
#[cfg(feature = "field")]
fn field_promote_policy(out: &FieldArgs) -> vole_document::field::promote::PromotePolicy {
    use vole_document::field::promote::{DEFAULT_PROMOTE_BUDGET_BYTES, PromotePolicy};
    if !out.promote {
        return PromotePolicy::default();
    }
    PromotePolicy {
        enabled: true,
        budget_bytes: out.promote_bytes.unwrap_or(DEFAULT_PROMOTE_BUDGET_BYTES),
        ..PromotePolicy::default()
    }
}

#[cfg(feature = "field")]
fn field_answer_json(answer: &FieldAnswer, stats: &ObserveStats, field: &FieldId) -> String {
    let value = match &answer.value {
        AnswerValue::Bytes(b) => {
            let sha = integrity::sha256(b);
            let hex_part = if b.len() <= 8192 {
                format!(",\"value_hex\":\"{}\"", integrity::to_hex(b))
            } else {
                String::new()
            };
            format!(
                "\"bytes_len\":{},\"bytes_sha256\":\"{}\"{}",
                b.len(),
                integrity::to_hex(&sha),
                hex_part
            )
        }
        AnswerValue::Text(t) => format!("\"text\":\"{}\"", json_escape(t)),
        AnswerValue::Json(j) => format!("\"value\":{j}"),
        AnswerValue::None => "\"value\":null".to_string(),
    };
    let span = match answer.source_span {
        Some((a, b)) => format!("[{a},{b}]"),
        None => "null".to_string(),
    };
    let deps = answer
        .dependency_ids
        .iter()
        .map(|d| format!("\"{}\"", d.to_hex()))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        concat!(
            "{{",
            "\"field\":\"{}\",",
            "\"selector\":\"{}\",",
            "\"representation\":\"{}\",",
            "\"provenance\":\"{}\",",
            "\"basis\":\"{}\",",
            "\"exact\":{},",
            "\"integrity_scope\":\"{}\",",
            "\"source_span\":{},",
            "\"dependency_ids\":[{}],",
            "{},",
            "\"stats\":{{\"index_nodes_read\":{},\"seed_nodes_fetched\":{},\"seed_nodes_materialized\":{},\"seed_nodes_executed\":{},\"seed_nodes_reused\":{},\"nodes_id_shared\":{},\"shared_resource_ids\":{},\"cache_bytes_written\":{},\"descriptor_bytes_read\":{},\"descriptor_read_mode\":\"{}\",\"manifest_bytes_read\":{},\"index_bytes_read\":{},\"seed_bytes_read\":{},\"bytes_read\":{},\"bytes_returned\":{},\"deepened\":{},\"wall_micros\":{}}}",
            "}}"
        ),
        field.to_hex(),
        json_escape(&answer.selector),
        json_escape(&answer.representation),
        json_escape(&answer.provenance),
        answer.basis.name(),
        answer.exact,
        answer.integrity_scope.name(),
        span,
        deps,
        value,
        stats.index_nodes_read,
        stats.seed_nodes_fetched,
        stats.seed_nodes_materialized,
        stats.seed_nodes_executed,
        stats.seed_nodes_reused,
        stats.nodes_id_shared,
        stats.shared_resource_ids,
        stats.cache_bytes_written,
        stats.descriptor_bytes_read,
        stats.descriptor_read_mode.name(),
        stats.manifest_bytes_read,
        stats.index_bytes_read,
        stats.seed_bytes_read,
        stats.bytes_read,
        stats.bytes_returned,
        stats.deepened,
        stats.wall_micros,
    )
}

/// Open the field store selected by `--entropyfs`/`--packed` (or the filesystem
/// default).
///
/// The engine-backed backend requires a build with the `entropyfs-store`
/// feature; asking for it without that feature is a typed `UnsupportedFeature`,
/// never a silent fallback to the filesystem backend. `--packed` and
/// `--entropyfs` are mutually exclusive backends of the same seed seam.
#[cfg(feature = "field")]
fn open_field_store(
    store_dir: &Path,
    entropyfs: bool,
    packed: bool,
    sync: vole_document::store::SyncPolicy,
) -> Result<FieldStore> {
    if entropyfs && packed {
        return Err(Error::usage(
            "--packed and --entropyfs are mutually exclusive seed backends",
        ));
    }
    if packed {
        return FieldStore::open_packed_with_policy(store_dir, sync);
    }
    #[cfg(feature = "entropyfs-store")]
    if entropyfs {
        return FieldStore::open_entropyfs(store_dir);
    }
    if entropyfs {
        return Err(Error::unsupported_feature(
            "--entropyfs requires a build with the entropyfs-store feature",
        ));
    }
    FieldStore::open(store_dir)
}

/// `field-store-stats --store DIR [--entropyfs]`: report advisory engine
/// accounting (blob count and bytes) for an EntropyFS-backed field store, so a
/// court can witness that the seed DAG is many individual engine blobs.
#[cfg(feature = "field")]
fn cmd_field_store_stats(args: &[String]) -> Result<()> {
    let out = parse_field_args(args)?;
    let store_dir = out
        .store
        .as_deref()
        .ok_or_else(|| Error::usage("field-store-stats requires --store DIR"))?;
    let store = open_field_store(store_dir, out.entropyfs, out.packed, out.sync_policy)?;
    if out.packed {
        println!("{{\"backend\":\"packed\"}}");
        return Ok(());
    }
    #[cfg(feature = "entropyfs-store")]
    {
        match store.engine_stats()? {
            Some(s) => println!(
                "{{\"backend\":\"entropyfs\",\"blob_count\":{},\"logical_bytes\":{},\"physical_used_bytes\":{}}}",
                s.blob_count, s.logical_bytes, s.physical_used_bytes
            ),
            None => println!("{{\"backend\":\"fs\"}}"),
        }
    }
    #[cfg(not(feature = "entropyfs-store"))]
    {
        let _ = store;
        println!("{{\"backend\":\"fs\"}}");
    }
    Ok(())
}

#[cfg(feature = "field")]
fn cmd_field_ingest(args: &[String], limits: Limits) -> Result<()> {
    let out = parse_field_args(args)?;
    let input = out
        .positional
        .first()
        .ok_or_else(|| Error::usage("field-ingest requires INPUT.voldoc"))?;
    let store_dir = out
        .store
        .as_deref()
        .ok_or_else(|| Error::usage("field-ingest requires --store DIR"))?;
    let pool = build_worker_pool(out.workers)?;
    let bytes = fs::read(input)?;
    let mut store = open_field_store(store_dir, out.entropyfs, out.packed, out.sync_policy)?;
    // Universal ingest: detect the format from bytes and invert with the right
    // adapter. Without the `package` feature only the PDF/opaque lane exists.
    #[cfg(feature = "package")]
    {
        match field_ingest::ingest_with(&mut store, &bytes, limits, pool.as_ref())? {
            field_ingest::IngestOutcome::Package(r) => {
                store.sync()?;
                print_package_ingest(&r);
            }
            field_ingest::IngestOutcome::Pdf(r) => {
                store.sync()?;
                print_pdf_ingest(&r);
            }
        }
        Ok(())
    }
    #[cfg(not(feature = "package"))]
    {
        let r = field_ingest::ingest_pdf_with(&mut store, &bytes, limits, pool.as_ref())?;
        store.sync()?;
        print_pdf_ingest(&r);
        Ok(())
    }
}

/// `field-build INPUT --store DIR [--profile runtime] [--workers N]
/// [--voldoc OUT.voldoc] [--entropyfs | --packed]`: the direct source → field
/// path. It serializes exactly one fixed, court-proved program (no candidate
/// search), stores it as the exact authority, and inverts it into the field —
/// all in one process.
#[cfg(feature = "field")]
fn cmd_field_build(args: &[String], limits: Limits) -> Result<()> {
    let out = parse_field_args(args)?;
    let input = out
        .positional
        .first()
        .ok_or_else(|| Error::usage("field-build requires INPUT"))?;
    let store_dir = out
        .store
        .as_deref()
        .ok_or_else(|| Error::usage("field-build requires --store DIR"))?;
    let profile = match out.profile.as_deref() {
        None => field_build::BuildProfile::Runtime,
        Some(s) => field_build::BuildProfile::parse(s)?,
    };
    let pool = build_worker_pool(out.workers)?;
    let source = fs::read(input)?;
    if source.len() as u64 > limits.max_input_bytes {
        return Err(Error::resource_limit(
            "input exceeds configured input limit",
        ));
    }
    let mut store = open_field_store(store_dir, out.entropyfs, out.packed, out.sync_policy)?;
    let report =
        field_build::build_field_with(&source, &mut store, limits, profile, pool.as_ref())?;
    store.sync()?;
    if let Some(path) = out.voldoc.as_deref() {
        // Write the exact authority the field stored (read back, never a
        // re-encode), so a later `field-ingest` reproduces this same field.
        let field = Field::open(&store, &report.ingest.field_id(), limits)?;
        write_atomic(path, field.descriptor_bytes())?;
    }
    println!("{}", direct_build_json(&report));
    Ok(())
}

/// The `field-build` receipt line: the fixed-profile facts plus the nested
/// ingest report (so a court can parse either the header or the ingest body).
#[cfg(feature = "field")]
fn direct_build_json(report: &field_build::DirectBuildReport) -> String {
    let ingest = match &report.ingest {
        field_build::DirectIngest::Pdf(r) => pdf_ingest_json(r),
        #[cfg(feature = "package")]
        field_build::DirectIngest::Package(r) => package_ingest_json(r),
    };
    format!(
        concat!(
            "{{",
            "\"ok\":true,",
            "\"profile\":\"{}\",",
            "\"candidate\":\"{}\",",
            "\"candidates_evaluated\":{},",
            "\"source_len\":{},",
            "\"encoded_len\":{},",
            "\"sha256\":\"{}\",",
            "\"descriptor_sha256\":\"{}\",",
            "\"ingest\":{}",
            "}}"
        ),
        report.profile,
        report.candidate,
        report.candidates_evaluated,
        report.source_len,
        report.encoded_len,
        report.source_sha256,
        report.descriptor_sha256,
        ingest,
    )
}

/// The hard upper bound on `--workers`, so the pool stays bounded.
#[cfg(feature = "parallel")]
const MAX_WORKERS: u32 = 128;

/// Build the optional worker pool from `--workers`. Absent or `1` is serial;
/// `0` resolves to `available_parallelism` (logged on stderr, clamped to the
/// bound); `N > 1` is exactly `N`. Without the `parallel` feature any non-serial
/// value is a typed usage error, never a silent serial fallback.
#[cfg(all(feature = "field", feature = "parallel"))]
fn build_worker_pool(workers: Option<u32>) -> Result<Option<vole_document::parallel::WorkerPool>> {
    let n = match workers {
        None => return Ok(None),
        Some(0) => std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .min(MAX_WORKERS as usize),
        Some(n) => {
            if n > MAX_WORKERS {
                return Err(Error::usage(format!(
                    "--workers must be at most {MAX_WORKERS}"
                )));
            }
            n as usize
        }
    };
    if n <= 1 {
        return Ok(None);
    }
    eprintln!("field-ingest: using {n} workers");
    Ok(Some(vole_document::parallel::WorkerPool::new(n)?))
}

#[cfg(all(feature = "field", not(feature = "parallel")))]
fn build_worker_pool(workers: Option<u32>) -> Result<Option<vole_document::parallel::WorkerPool>> {
    match workers {
        None | Some(1) => Ok(None),
        Some(_) => Err(Error::usage(
            "--workers requires a build compiled with the `parallel` feature",
        )),
    }
}

#[cfg(feature = "field")]
fn print_pdf_ingest(r: &field_ingest::IngestReport) {
    println!("{}", pdf_ingest_json(r));
}

#[cfg(feature = "field")]
fn pdf_ingest_json(r: &field_ingest::IngestReport) -> String {
    let index_root = match r.index_root {
        Some(id) => format!("\"{}\"", id.to_hex()),
        None => "null".to_string(),
    };
    format!(
        concat!(
            "{{",
            "\"format\":\"{}\",",
            "\"field\":\"{}\",",
            "\"root_node\":\"{}\",",
            "\"index_root\":{},",
            "\"node_count\":{},",
            "\"index_node_count\":{},",
            "\"source_len\":{},",
            "\"object_nodes\":{},",
            "\"stream_nodes\":{},",
            "\"decoded_stream_nodes\":{},",
            "\"page_nodes\":{},",
            "\"revision_nodes\":{},",
            "\"declined_streams\":{},",
            "\"resource_blob_nodes\":{},",
            "\"shared_resource_ids\":{},",
            "\"shared_resource_bytes\":{},",
            "\"nodes_id_shared\":{},",
            "\"seed_bytes_written\":{}",
            "}}"
        ),
        r.format.name(),
        r.field.to_hex(),
        r.root_node.to_hex(),
        index_root,
        r.node_count,
        r.index_node_count,
        r.source_len,
        r.object_nodes,
        r.stream_nodes,
        r.decoded_stream_nodes,
        r.page_nodes,
        r.revision_nodes,
        r.declined_streams,
        r.resource_blob_nodes,
        r.shared_resource_ids,
        r.shared_resource_bytes,
        r.nodes_id_shared,
        r.seed_bytes_written,
    )
}

#[cfg(all(feature = "field", feature = "package"))]
fn print_package_ingest(r: &vole_document::field::ingest_package::PackageIngestReport) {
    println!("{}", package_ingest_json(r));
}

#[cfg(all(feature = "field", feature = "package"))]
fn package_ingest_json(r: &vole_document::field::ingest_package::PackageIngestReport) -> String {
    let index_root = match r.index_root {
        Some(id) => format!("\"{}\"", id.to_hex()),
        None => "null".to_string(),
    };
    format!(
        concat!(
            "{{",
            "\"format\":\"{}\",",
            "\"field\":\"{}\",",
            "\"root_node\":\"{}\",",
            "\"index_root\":{},",
            "\"node_count\":{},",
            "\"index_node_count\":{},",
            "\"source_len\":{},",
            "\"member_count\":{},",
            "\"raw_nodes\":{},",
            "\"decoded_nodes\":{},",
            "\"declined_decodes\":{},",
            "\"opc_model_nodes\":{},",
            "\"docx_model_nodes\":{},",
            "\"epub_model_nodes\":{},",
            "\"ods_model_nodes\":{},",
            "\"xlsx_model_nodes\":{},",
            "\"pptx_model_nodes\":{},",
            "\"resource_blob_nodes\":{},",
            "\"shared_resource_ids\":{},",
            "\"shared_resource_bytes\":{},",
            "\"nodes_id_shared\":{},",
            "\"seed_bytes_written\":{},",
            "\"index_bytes_written\":{}",
            "}}"
        ),
        r.format.name(),
        r.field.to_hex(),
        r.root_node.to_hex(),
        index_root,
        r.node_count,
        r.index_node_count,
        r.source_len,
        r.member_count,
        r.raw_nodes,
        r.decoded_nodes,
        r.declined_decodes,
        r.opc_model_nodes,
        r.docx_model_nodes,
        r.epub_model_nodes,
        r.ods_model_nodes,
        r.xlsx_model_nodes,
        r.pptx_model_nodes,
        r.resource_blob_nodes,
        r.shared_resource_ids,
        r.shared_resource_bytes,
        r.nodes_id_shared,
        r.seed_bytes_written,
        r.index_bytes_written,
    )
}

#[cfg(feature = "field")]
fn cmd_field_edit(args: &[String], _limits: Limits) -> Result<()> {
    let out = parse_field_args(args)?;
    let store_dir = out
        .store
        .as_deref()
        .ok_or_else(|| Error::usage("field-edit requires --store DIR"))?;
    let field_hex = out
        .field
        .as_deref()
        .ok_or_else(|| Error::usage("field-edit requires --field HEX"))?;
    let page = out
        .page
        .ok_or_else(|| Error::usage("field-edit requires --page N"))?;
    let content_path = out
        .content
        .as_deref()
        .ok_or_else(|| Error::usage("field-edit requires --content FILE"))?;
    let content = fs::read(content_path)?;
    let mut store = open_field_store(store_dir, out.entropyfs, out.packed, out.sync_policy)?;
    let id = FieldId::from_hex(field_hex)?;
    let r = field_edit::replace_page_content(&mut store, &id, page, &content)?;
    store.sync()?;
    println!(
        concat!(
            "{{",
            "\"field\":\"{}\",",
            "\"previous\":\"{}\",",
            "\"page\":{},",
            "\"page_content\":\"{}\",",
            "\"content_literal\":\"{}\",",
            "\"index_root\":\"{}\",",
            "\"index_entries\":{},",
            "\"index_entries_reused\":{},",
            "\"index_entries_replaced\":{},",
            "\"seed_nodes_new\":{},",
            "\"seed_nodes_reused\":{},",
            "\"index_nodes_reused\":{},",
            "\"index_nodes_new\":{},",
            "\"bytes_newly_persisted\":{},",
            "\"descriptor_bytes_read\":{},",
            "\"manifest_bytes_read\":{},",
            "\"index_bytes_read\":{},",
            "\"seed_bytes_read\":{}",
            "}}"
        ),
        r.field.to_hex(),
        r.previous.to_hex(),
        r.page,
        r.page_content.to_hex(),
        r.content_literal.to_hex(),
        r.index_root.to_hex(),
        r.index_entries,
        r.index_entries_reused,
        r.index_entries_replaced,
        r.seed_nodes_new,
        r.seed_nodes_reused,
        r.index_nodes_reused,
        r.index_nodes_new,
        r.bytes_newly_persisted,
        r.descriptor_bytes_read,
        r.manifest_bytes_read,
        r.index_bytes_read,
        r.seed_bytes_read,
    );
    Ok(())
}

/// `field-external`: attach, show, or clear an explicit **external** context
/// (Phase 20.4). The record is stored beside the field under `external/`; it is
/// never on the exactness path, never in the seed DAG/index/manifest, and is
/// never read by a document-derived observation.
#[cfg(feature = "field")]
fn cmd_field_external(args: &[String]) -> Result<()> {
    use vole_document::field::external::{ExternalContext, ExternalLineage, ExternalOrigin};
    let mut store_dir: Option<PathBuf> = None;
    let mut field_hex: Option<String> = None;
    let mut lineage_spec: Option<String> = None;
    let mut dataset: Option<String> = None;
    let mut revision_family: Option<String> = None;
    let mut origin: Option<String> = None;
    let mut source: Option<String> = None;
    let mut clear = false;
    let mut i = 2;
    while i < args.len() {
        let a = args[i].as_str();
        let (flag, inline) = match a.split_once('=') {
            Some((f, v)) => (f, Some(v)),
            None => (a, None),
        };
        match flag {
            "--store" => {
                store_dir = Some(PathBuf::from(field_arg_value(
                    args, &mut i, "--store", inline,
                )?));
            }
            "--field" => field_hex = Some(field_arg_value(args, &mut i, "--field", inline)?),
            "--lineage" => {
                lineage_spec = Some(field_arg_value(args, &mut i, "--lineage", inline)?);
            }
            "--dataset" => dataset = Some(field_arg_value(args, &mut i, "--dataset", inline)?),
            "--revision-family" => {
                revision_family = Some(field_arg_value(args, &mut i, "--revision-family", inline)?);
            }
            "--origin" => origin = Some(field_arg_value(args, &mut i, "--origin", inline)?),
            "--source" => source = Some(field_arg_value(args, &mut i, "--source", inline)?),
            "--clear" => {
                clear = true;
                i += 1;
            }
            other => {
                return Err(Error::usage(format!(
                    "unknown field-external argument {other:?}"
                )));
            }
        }
    }
    let store_dir = store_dir.ok_or_else(|| Error::usage("field-external requires --store DIR"))?;
    let field_hex = field_hex.ok_or_else(|| Error::usage("field-external requires --field HEX"))?;
    if clear && lineage_spec.is_some() {
        return Err(Error::usage(
            "field-external: --clear and --lineage are mutually exclusive",
        ));
    }
    let store = open_field_store(
        &store_dir,
        false,
        false,
        vole_document::store::SyncPolicy::default(),
    )?;
    let id = FieldId::from_hex(&field_hex)?;
    if clear {
        let removed = store.clear_external_context(&id)?;
        println!(
            "{{\"field\":\"{}\",\"external_context\":\"cleared\",\"removed\":{}}}",
            id.to_hex(),
            removed
        );
        return Ok(());
    }
    if let Some(spec) = lineage_spec {
        let parts: Vec<&str> = spec.split(':').collect();
        if parts.len() != 3 {
            return Err(Error::usage(
                "--lineage must be FAMILY:MEMBER:HEAD (use '-' for an absent family/member)",
            ));
        }
        let opt = |s: &str| {
            if s == "-" || s.is_empty() {
                None
            } else {
                Some(s.to_string())
            }
        };
        let head = match parts[2] {
            "1" | "true" => true,
            "0" | "false" => false,
            other => {
                return Err(Error::usage(format!(
                    "--lineage head must be 0 or 1, got {other:?}"
                )));
            }
        };
        let ctx = ExternalContext {
            dataset_id: dataset,
            lineage: ExternalLineage {
                family: opt(parts[0]),
                member: opt(parts[1]),
                head,
                revision_family,
            },
            origin: match origin.as_deref() {
                Some(o) => ExternalOrigin::parse(o)?,
                None => ExternalOrigin::Harness,
            },
            source: source.unwrap_or_default(),
        };
        let bytes = ctx.encode_canonical().len();
        store.put_external_context(&id, &ctx)?;
        println!(
            "{{\"field\":\"{}\",\"external_context\":\"attached\",\"origin\":\"{}\",\"bytes\":{}}}",
            id.to_hex(),
            ctx.origin.name(),
            bytes
        );
        return Ok(());
    }
    match store.get_external_context(&id)? {
        Some(ctx) => println!(
            "{{\"field\":\"{}\",\"basis\":\"external-metadata\",\"external_context\":{}}}",
            id.to_hex(),
            ctx.answer_json()
        ),
        None => {
            return Err(Error::unsupported_feature(
                "no external context is attached to this field",
            ));
        }
    }
    Ok(())
}

#[cfg(feature = "field")]
fn cmd_field_observe(args: &[String], limits: Limits) -> Result<()> {
    let out = parse_field_args(args)?;
    let store_dir = out
        .store
        .as_deref()
        .ok_or_else(|| Error::usage("observe requires --store DIR"))?;
    let field_hex = out
        .field
        .as_deref()
        .ok_or_else(|| Error::usage("observe requires --field HEX"))?;
    let selector = field_selector(&out)?;
    let kind = out
        .kind
        .as_deref()
        .ok_or_else(|| Error::usage("observe requires --kind KIND"))?;
    let representation = field_representation(kind)?;
    let mut store = open_field_store(store_dir, out.entropyfs, out.packed, out.sync_policy)?;
    store.set_promote(field_promote_policy(&out));
    let id = FieldId::from_hex(field_hex)?;
    let req = observe_request(&out, selector, representation);
    let (answer, stats, field) = observe(&mut store, &id, &req, limits)?;
    store.sync()?;
    println!("{}", field_answer_json(&answer, &stats, &field));
    Ok(())
}

/// `observe-batch --store DIR --field HEX [--entropyfs] [--requests FILE|-] [--repeat N]`:
///
/// Open the store and field once and serve many observations in one process,
/// printing one `field_answer_json` line per observation. Each non-empty,
/// non-`#` request line is a full per-observation argument list **without**
/// `--store`/`--field` (which are session-level), parsed through the same
/// [`parse_field_args`] grammar as `observe`.
#[cfg(feature = "field")]
fn cmd_field_observe_batch(args: &[String], limits: Limits) -> Result<()> {
    use std::fs::File;
    use std::io::{BufRead, BufReader};
    use vole_document::field::session::{
        DEFAULT_MODEL_MEMO_BYTES, DocumentFieldSession, SessionOptions,
    };

    let out = parse_field_args(args)?;
    let store_dir = out
        .store
        .as_deref()
        .ok_or_else(|| Error::usage("observe-batch requires --store DIR"))?;
    if out.entropyfs && out.packed {
        return Err(Error::usage(
            "--packed and --entropyfs are mutually exclusive seed backends",
        ));
    }
    let field_hex = out
        .field
        .as_deref()
        .ok_or_else(|| Error::usage("observe-batch requires --field HEX"))?;
    let repeat = out.repeat.unwrap_or(1).max(1);
    let mut session = DocumentFieldSession::open(
        store_dir,
        field_hex,
        SessionOptions {
            entropyfs: out.entropyfs,
            packed: out.packed,
            model_memo_bytes: DEFAULT_MODEL_MEMO_BYTES,
            promote: field_promote_policy(&out),
        },
    )?;
    let reader: Box<dyn BufRead> = match out.requests.as_deref() {
        Some(p) if p != Path::new("-") => Box::new(BufReader::new(File::open(p)?)),
        _ => Box::new(BufReader::new(std::io::stdin())),
    };
    // A typed decline is a legitimate *answer* for an observation a document does
    // not have (e.g. a table in a document with none): it is reported on that
    // request's JSON line, not as a session failure. The batch exits non-zero only
    // when it answered nothing at all, so a lane-level rc keeps its meaning.
    let mut answered = 0usize;
    let mut declined = 0usize;
    let prof = std::env::var_os("VOLE_PROFILE_OPEN").is_some();
    let t_loop = std::time::Instant::now();
    let mut observe_us: u128 = 0;
    let mut observe_calls: usize = 0;
    let mut serialize_us: u128 = 0;
    for (i, line) in reader.lines().enumerate() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut la = vec!["vole-document".to_string(), "observe".to_string()];
        la.extend(line.split_whitespace().map(str::to_string));
        let lo = parse_field_args(&la)?;
        if lo.store.is_some() || lo.field.is_some() {
            return Err(Error::usage(
                "observe-batch: --store/--field are session-level; remove them from a request line",
            ));
        }
        let selector = field_selector(&lo)?;
        let kind = lo
            .kind
            .as_deref()
            .ok_or_else(|| Error::usage("observe-batch: each request needs --kind KIND"))?;
        let req = observe_request(&lo, selector, field_representation(kind)?);
        for _ in 0..repeat {
            let t_obs = std::time::Instant::now();
            let res = session.observe(&req, limits);
            if prof {
                observe_us += t_obs.elapsed().as_micros();
                observe_calls += 1;
            }
            match res {
                Ok((answer, stats, field)) => {
                    answered += 1;
                    let t_ser = std::time::Instant::now();
                    println!("{}", field_answer_json(&answer, &stats, &field));
                    if prof {
                        serialize_us += t_ser.elapsed().as_micros();
                    }
                }
                Err(e) => {
                    declined += 1;
                    eprintln!("observe-batch: request {i}: {e}");
                    println!(
                        "{{\"request\":{i},\"error\":{},\"message\":\"{}\"}}",
                        e.exit_code(),
                        json_escape(&e.to_string())
                    );
                }
            }
        }
    }
    if prof {
        eprintln!(
            "[vole-profile] request_loop_total_us={} observe_dispatch_us={} observe_calls={} serialize_us={}",
            t_loop.elapsed().as_micros(),
            observe_us,
            observe_calls,
            serialize_us
        );
    }
    session.sync()?;
    if answered == 0 && declined > 0 {
        return Err(Error::internal_invariant(
            "observe-batch: no request was answered",
        ));
    }
    Ok(())
}

#[cfg(feature = "field")]
fn cmd_field_find(args: &[String], limits: Limits) -> Result<()> {
    let out = parse_field_args(args)?;
    let store_dir = out
        .store
        .as_deref()
        .ok_or_else(|| Error::usage("find requires --store DIR"))?;
    let field_hex = out
        .field
        .as_deref()
        .ok_or_else(|| Error::usage("find requires --field HEX"))?;
    let text = out
        .text
        .clone()
        .ok_or_else(|| Error::usage("find requires --text PATTERN"))?;
    let mut store = open_field_store(store_dir, out.entropyfs, out.packed, out.sync_policy)?;
    store.set_promote(field_promote_policy(&out));
    let id = FieldId::from_hex(field_hex)?;
    // Format-agnostic lexical find: the common `SearchMatch` selector dispatches
    // through the detected format's adapter (Phase 12.7).
    let req = observe_request(&out, Selector::SearchMatch(text), Representation::Text);
    let (answer, stats, field) = observe(&mut store, &id, &req, limits)?;
    store.sync()?;
    println!("{}", field_answer_json(&answer, &stats, &field));
    Ok(())
}

#[cfg(feature = "field")]
fn cmd_field_explain(args: &[String], limits: Limits) -> Result<()> {
    let out = parse_field_args(args)?;
    let store_dir = out
        .store
        .as_deref()
        .ok_or_else(|| Error::usage("explain requires --store DIR"))?;
    let field_hex = out
        .field
        .as_deref()
        .ok_or_else(|| Error::usage("explain requires --field HEX"))?;
    let selector = field_selector(&out)?;
    let kind = out
        .kind
        .as_deref()
        .ok_or_else(|| Error::usage("explain requires --kind KIND"))?;
    let representation = field_representation(kind)?;
    let mut store = open_field_store(store_dir, out.entropyfs, out.packed, out.sync_policy)?;
    store.set_promote(field_promote_policy(&out));
    let id = FieldId::from_hex(field_hex)?;
    let req = observe_request(&out, selector, representation);
    if out.analyze {
        let manifest = store.get_field(&id)?;
        let planned_json = explain(&manifest, &store, &req)?.json;
        let fmt = vole_document::field::document_format::DocumentFormat::from_provenance(
            &manifest.provenance,
        );
        let format_name = fmt.map_or("unknown", |f| f.name());
        let adapter = fmt.map_or("unknown", |f| f.adapter());
        let (answer, stats, promoted) = observe(&mut store, &id, &req, limits)?;
        println!(
            "{{\"plan\":{},\"actual\":{}}}",
            planned_json,
            explain_actual_json(&stats, &answer, &promoted, format_name, adapter)
        );
    } else {
        let manifest = store.get_field(&id)?;
        let plan = explain(&manifest, &store, &req)?;
        println!("{}", plan.json);
    }
    store.sync()?;
    Ok(())
}

/// The executed-observation evidence object for `explain --analyze`, including
/// the promoted field id, the per-class physical byte counts (review fix #1),
/// and the reuse counters (11.8).
#[cfg(feature = "field")]
fn explain_actual_json(
    stats: &ObserveStats,
    answer: &FieldAnswer,
    field: &FieldId,
    format: &str,
    adapter: &str,
) -> String {
    format!(
        concat!(
            "{{",
            "\"field\":\"{}\",",
            "\"format\":\"{}\",",
            "\"adapter\":\"{}\",",
            "\"index_nodes_read\":{},",
            "\"seed_nodes_fetched\":{},",
            "\"seed_nodes_materialized\":{},",
            "\"seed_nodes_executed\":{},",
            "\"seed_nodes_reused\":{},",
            "\"nodes_id_shared\":{},",
            "\"shared_resource_ids\":{},",
            "\"inverse_work_units\":{},",
            "\"member_decodes\":{},",
            "\"xml_parses\":{},",
            "\"cache_bytes_written\":{},",
            "\"descriptor_bytes_read\":{},",
            "\"descriptor_read_mode\":\"{}\",",
            "\"manifest_bytes_read\":{},",
            "\"index_bytes_read\":{},",
            "\"seed_bytes_read\":{},",
            "\"bytes_read\":{},",
            "\"bytes_returned\":{},",
            "\"deepened\":{},",
            "\"whole_source_materialized\":{},",
            "\"wall_micros\":{},",
            "\"basis\":\"{}\",",
            "\"exact\":{}",
            "}}"
        ),
        field.to_hex(),
        json_escape(format),
        json_escape(adapter),
        stats.index_nodes_read,
        stats.seed_nodes_fetched,
        stats.seed_nodes_materialized,
        stats.seed_nodes_executed,
        stats.seed_nodes_reused,
        stats.nodes_id_shared,
        stats.shared_resource_ids,
        stats.seed_nodes_executed.saturating_add(stats.bytes_read),
        stats.member_decodes,
        stats.xml_parses,
        stats.cache_bytes_written,
        stats.descriptor_bytes_read,
        stats.descriptor_read_mode.name(),
        stats.manifest_bytes_read,
        stats.index_bytes_read,
        stats.seed_bytes_read,
        stats.bytes_read,
        stats.bytes_returned,
        stats.deepened,
        answer.integrity_scope == vole_document::field::provenance::IntegrityScope::WholeSource,
        stats.wall_micros,
        answer.basis.name(),
        answer.exact,
    )
}

#[cfg(feature = "field")]
fn cmd_field_preview(args: &[String], limits: Limits) -> Result<()> {
    let out = parse_field_args(args)?;
    let store_dir = out
        .store
        .as_deref()
        .ok_or_else(|| Error::usage("preview requires --store DIR"))?;
    let field_hex = out
        .field
        .as_deref()
        .ok_or_else(|| Error::usage("preview requires --field HEX"))?;
    let page = out
        .page
        .ok_or_else(|| Error::usage("preview requires --page N"))?;
    let as_json = out.json;
    let mut store = open_field_store(store_dir, out.entropyfs, out.packed, out.sync_policy)?;
    store.set_promote(field_promote_policy(&out));
    let id = FieldId::from_hex(field_hex)?;
    let req = observe_request(&out, Selector::Page(page), Representation::Preview);
    let (answer, stats, field) = observe(&mut store, &id, &req, limits)?;
    store.sync()?;
    if !as_json && let AnswerValue::Bytes(bytes) = &answer.value {
        std::io::stdout().write_all(bytes).map_err(Error::from)?;
        return Ok(());
    }
    println!("{}", field_answer_json(&answer, &stats, &field));
    Ok(())
}

/// `share-account --store DIR INPUT.voldoc...`: store every inline fine unit of
/// the cohort in `<DIR>/share` and print the [`share::ShareReport`] as JSON
/// (Phase 11.14).
///
/// The store is populated so the fine-unit sharing rests on **stored bytes**, not
/// a paper calculation: because the store is content-addressed, `store_bytes`
/// (distinct stored blobs) must equal `unique_bytes`, an independent check on the
/// in-memory report. The descriptor still carries every unit inline; `unique_bytes`
/// excludes all record/root framing and is a lower bound on any store form.
/// Deterministic for a fixed input set (content addressing + sorted `by_kind`).
#[cfg(feature = "field")]
fn cmd_share_account(args: &[String], limits: Limits) -> Result<()> {
    let mut store_dir: Option<PathBuf> = None;
    let mut inputs: Vec<PathBuf> = Vec::new();
    let mut i = 2;
    while i < args.len() {
        let a = args[i].as_str();
        let (flag, inline) = match a.split_once('=') {
            Some((f, v)) => (f, Some(v)),
            None => (a, None),
        };
        match flag {
            "--store" => {
                store_dir = Some(PathBuf::from(field_arg_value(
                    args, &mut i, "--store", inline,
                )?));
            }
            other if other.starts_with("--") => {
                return Err(Error::usage(format!(
                    "unknown share-account argument {other:?}"
                )));
            }
            other => {
                inputs.push(PathBuf::from(other));
                i += 1;
            }
        }
    }
    let store_dir = store_dir.ok_or_else(|| Error::usage("share-account requires --store DIR"))?;
    if inputs.is_empty() {
        return Err(Error::usage(
            "share-account requires at least one INPUT.voldoc",
        ));
    }
    let mut descriptors = Vec::with_capacity(inputs.len());
    for path in &inputs {
        let bytes = fs::read(path)?;
        descriptors.push(Descriptor::parse(&bytes, limits)?.descriptor);
    }
    // Store every inline fine unit so the accounting rests on stored bytes.
    let mut units_offered: u64 = 0;
    for d in &descriptors {
        units_offered += share::store_units(&store_dir, d)?;
    }
    let report = share::cohort_report(&descriptors)?;
    let store_bytes = share::share_store(&store_dir)?.stats()?.stored_bytes;
    let kinds: Vec<String> = report
        .by_kind
        .iter()
        .map(|(k, t, u)| {
            format!(
                "{{\"kind\":\"{}\",\"total\":{t},\"unique\":{u}}}",
                json_escape(k)
            )
        })
        .collect();
    println!(
        concat!(
            "{{",
            "\"ok\":true,",
            "\"files\":{},",
            "\"units_offered\":{},",
            "\"store_bytes\":{},",
            "\"total_bytes\":{},",
            "\"unique_bytes\":{},",
            "\"unit_count\":{},",
            "\"unique_count\":{},",
            "\"by_kind\":[{}]",
            "}}"
        ),
        inputs.len(),
        units_offered,
        store_bytes,
        report.total_bytes,
        report.unique_bytes,
        report.unit_count,
        report.unique_count,
        kinds.join(",")
    );
    Ok(())
}

/// `share report INPUT.voldoc...`: measure fine-unit sharing over a cohort of
/// descriptors (Phase 11.14). Read-only; no store is touched.
#[cfg(feature = "field")]
fn cmd_share_args(args: &[String], limits: Limits) -> Result<()> {
    match args.get(2).map(String::as_str) {
        Some("report") => {
            let inputs: Vec<PathBuf> = args
                .get(3..)
                .unwrap_or(&[])
                .iter()
                .map(PathBuf::from)
                .collect();
            if inputs.is_empty() {
                return Err(Error::usage(
                    "share report requires at least one INPUT.voldoc",
                ));
            }
            cmd_share_report(&inputs, limits)
        }
        Some("externalize") => {
            let mut store_dir: Option<PathBuf> = None;
            let mut positional: Vec<&str> = Vec::new();
            let mut i = 3;
            while i < args.len() {
                let a = args[i].as_str();
                let (flag, inline) = match a.split_once('=') {
                    Some((f, v)) => (f, Some(v)),
                    None => (a, None),
                };
                match flag {
                    "--store" => {
                        store_dir = Some(PathBuf::from(field_arg_value(
                            args, &mut i, "--store", inline,
                        )?));
                    }
                    other => positional.push(other),
                }
            }
            let store_dir =
                store_dir.ok_or_else(|| Error::usage("share externalize requires --store DIR"))?;
            let input = positional
                .first()
                .map(PathBuf::from)
                .ok_or_else(|| Error::usage("share externalize requires INPUT.voldoc"))?;
            let output = positional
                .get(1)
                .map(PathBuf::from)
                .ok_or_else(|| Error::usage("share externalize requires OUTPUT.voldoc"))?;
            if positional.len() > 2 {
                return Err(Error::usage(format!(
                    "unexpected extra argument {:?}",
                    positional[2]
                )));
            }
            cmd_share_externalize(&store_dir, &input, &output, limits)
        }
        other => Err(Error::usage(format!(
            "unknown share subcommand {:?}",
            other.unwrap_or("")
        ))),
    }
}

/// Print fine-unit sharing for each descriptor and for the cohort as a whole.
#[cfg(feature = "field")]
fn cmd_share_report(inputs: &[PathBuf], limits: Limits) -> Result<()> {
    let mut descriptors = Vec::with_capacity(inputs.len());
    for path in inputs {
        let bytes = fs::read(path)?;
        descriptors.push(Descriptor::parse(&bytes, limits)?.descriptor);
    }
    let report = share::cohort_report(&descriptors)?;
    let per_file: Vec<String> = inputs
        .iter()
        .zip(&descriptors)
        .map(|(path, d)| {
            let r = share::cohort_report(std::slice::from_ref(d))?;
            Ok(format!(
                concat!(
                    "{{",
                    "\"file\":\"{}\",",
                    "\"total_bytes\":{},",
                    "\"unique_bytes\":{},",
                    "\"unit_count\":{},",
                    "\"unique_count\":{}",
                    "}}"
                ),
                json_escape(&path.display().to_string()),
                r.total_bytes,
                r.unique_bytes,
                r.unit_count,
                r.unique_count
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    println!(
        "{{\"ok\":true,\"files\":{},\"report\":{},\"per_file\":[{}]}}",
        inputs.len(),
        report.to_json(),
        per_file.join(",")
    );
    Ok(())
}

/// Externalize the object table into `<DIR>/share` (wire-real) and store every
/// fine unit there (measured), writing the store-backed descriptor to `OUTPUT`.
#[cfg(feature = "field")]
fn cmd_share_externalize(
    store_dir: &Path,
    input: &Path,
    output: &Path,
    limits: Limits,
) -> Result<()> {
    let encoded = fs::read(input)?;
    let mut descriptor = Descriptor::parse(&encoded, limits)?.descriptor;
    let offered = share::store_units(store_dir, &descriptor)?;
    let objects = share::externalize_objects(store_dir, &mut descriptor)?;
    let (bytes, _cost) = descriptor.serialize()?;
    write_atomic(output, &bytes)?;
    let report = share::cohort_report(std::slice::from_ref(&descriptor))?;
    let share_bytes = share::share_store(store_dir)?.stats()?.stored_bytes;
    println!(
        concat!(
            "{{",
            "\"ok\":true,",
            "\"output\":\"{}\",",
            "\"objects\":{},",
            "\"units_offered\":{},",
            "\"root_bytes\":{},",
            "\"share_store_bytes\":{},",
            "\"report\":{}",
            "}}"
        ),
        json_escape(&output.display().to_string()),
        objects,
        offered,
        bytes.len(),
        share_bytes,
        report.to_json()
    );
    Ok(())
}

/// `cache --store DIR [--clear]`: report — and optionally reclaim — the
/// disposable derived-cache universe (ADR-0027). Never touches the store or the
/// descriptor.
#[cfg(feature = "field")]
fn cmd_field_cache(args: &[String]) -> Result<()> {
    let mut store_dir: Option<PathBuf> = None;
    let mut clear = false;
    let mut entropyfs = false;
    let mut packed = false;
    let mut i = 2;
    while i < args.len() {
        let a = args[i].as_str();
        let (flag, inline) = match a.split_once('=') {
            Some((f, v)) => (f, Some(v)),
            None => (a, None),
        };
        match flag {
            "--clear" => {
                clear = true;
                i += 1;
            }
            "--entropyfs" => {
                entropyfs = true;
                i += 1;
            }
            "--packed" => {
                packed = true;
                i += 1;
            }
            "--store" => {
                store_dir = Some(PathBuf::from(field_arg_value(
                    args, &mut i, "--store", inline,
                )?));
            }
            other => return Err(Error::usage(format!("unknown cache argument {other:?}"))),
        }
    }
    let store_dir = store_dir.ok_or_else(|| Error::usage("cache requires --store DIR"))?;
    let store = open_field_store(
        &store_dir,
        entropyfs,
        packed,
        vole_document::store::SyncPolicy::Batch,
    )?;
    let cache = DerivedCache::open(store.root().join("cache"))?;
    if clear {
        let reclaimed = cache.clear()?;
        println!(
            "{{\"cache_bytes\":{},\"reclaimed\":{}}}",
            cache.total_bytes()?,
            reclaimed
        );
    } else {
        println!("{{\"cache_bytes\":{}}}", cache.total_bytes()?);
    }
    Ok(())
}

#[cfg(feature = "field")]
fn cmd_field_materialize(args: &[String], limits: Limits) -> Result<()> {
    let out = parse_field_args(args)?;
    let store_dir = out
        .store
        .as_deref()
        .ok_or_else(|| Error::usage("materialize requires --store DIR"))?;
    let field_hex = out
        .field
        .as_deref()
        .ok_or_else(|| Error::usage("materialize requires --field HEX"))?;
    let output = out
        .output
        .as_deref()
        .ok_or_else(|| Error::usage("materialize requires --output FILE"))?;
    let store = open_field_store(store_dir, out.entropyfs, out.packed, out.sync_policy)?;
    let id = FieldId::from_hex(field_hex)?;
    let field = Field::open(&store, &id, limits)?;
    let bytes = field.materialize_exact(limits)?;
    write_atomic(output, &bytes)?;
    println!(
        "{{\"source_len\":{},\"sha256\":\"{}\"}}",
        bytes.len(),
        integrity::to_hex(&integrity::sha256(&bytes))
    );
    Ok(())
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
