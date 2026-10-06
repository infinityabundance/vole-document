//! Phase-10.1 governor court driver (feature `dsfb-search`).
//!
//! Runs `Exhaustive` / `FixedHeuristic` / `DsfbGuided` over the frozen
//! tune/holdout/control workload set, records final complete bytes, candidates
//! evaluated, CPU, wall, and peak RSS per workload × strategy, evaluates the
//! pre-registered hypotheses H1–H4, and writes JSON artifacts into the directory
//! given as `argv[1]`. It is invoked by `tools/governor-court.sh` inside the
//! pinned `dev` container.
//!
//! Usage: `cargo run --features dsfb-search --example governor_court -- OUTDIR`

use std::fs;
use std::path::PathBuf;

use vole_document::encode::candidates::CandidateKind;
use vole_document::encode::governor::{self, StrategyResult, Workload, WorkloadSet, workloads};
use vole_document::limits::Limits;

struct Row {
    name: &'static str,
    set: WorkloadSet,
    negative: bool,
    source_len: u64,
    strategy: &'static str,
    result: StrategyResult,
}

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

fn set_name(set: WorkloadSet) -> &'static str {
    match set {
        WorkloadSet::Tune => "tune",
        WorkloadSet::Holdout => "holdout",
        WorkloadSet::NegativeControl => "control",
    }
}

fn run_one(w: &Workload, strategy: &'static str) -> Row {
    let limits = Limits::DEFAULT;
    let result = match strategy {
        "exhaustive" => governor::run_exhaustive(&w.bytes, limits).expect("exhaustive"),
        "fixed" => governor::run_fixed(&w.bytes, limits).expect("fixed"),
        "guided" => governor::run_guided(&w.bytes, limits).expect("guided"),
        _ => unreachable!(),
    };
    Row {
        name: w.name,
        set: w.set,
        negative: w.negative,
        source_len: w.bytes.len() as u64,
        strategy,
        result,
    }
}

fn row_json(r: &Row) -> String {
    let stopped = match r.result.stopped {
        Some(governor::Accept::Raw) => "\"Raw\"",
        Some(governor::Accept::BestSoFar) => "\"BestSoFar\"",
        None => "null",
    };
    let dominant = match r.result.dominant_class {
        Some(c) => format!("\"{}\"", c.name()),
        None => "null".to_string(),
    };
    format!(
        "{{\"workload\":\"{}\",\"set\":\"{}\",\"negative\":{},\"source_len\":{},\"strategy\":\"{}\",\
         \"winner\":\"{}\",\"final_bytes\":{},\"candidates_evaluated\":{},\"configs_tried\":{},\
         \"cpu_ns\":{},\"wall_ns\":{},\"peak_rss_bytes\":{},\"stopped\":{},\"dominant_class\":{}}}",
        jesc(r.name),
        set_name(r.set),
        r.negative,
        r.source_len,
        r.strategy,
        r.result.winner.name(),
        r.result.final_bytes,
        r.result.candidates_evaluated,
        r.result.configs_tried,
        r.result.cpu_ns,
        r.result.wall_ns,
        r.result.peak_rss_bytes,
        stopped,
        dominant,
    )
}

fn find<'a>(rows: &'a [Row], name: &str, strategy: &str) -> &'a Row {
    rows.iter()
        .find(|r| r.name == name && r.strategy == strategy)
        .unwrap_or_else(|| panic!("missing row {name}/{strategy}"))
}

/// The RAW-only `.voldoc` bytes for an input (the negative-control floor).
fn raw_only(input: &[u8]) -> Vec<u8> {
    let (bytes, _) =
        vole_document::encode::encode_with(input, Limits::DEFAULT, Some(CandidateKind::Raw))
            .expect("raw-only encode");
    bytes
}

/// Pre-registered hypotheses, evaluated with the frozen rules.
fn evaluate(rows: &[Row]) -> String {
    let names: Vec<&str> = {
        let mut v: Vec<&str> = Vec::new();
        for r in rows {
            if !v.contains(&r.name) {
                v.push(r.name);
            }
        }
        v
    };

    // H1: guided.final <= fixed.final on every workload.
    let mut h1 = true;
    let mut h1_fail: Vec<&str> = Vec::new();
    for &n in &names {
        let f = find(rows, n, "fixed").result.final_bytes;
        let g = find(rows, n, "guided").result.final_bytes;
        if g > f {
            h1 = false;
            h1_fail.push(n);
        }
    }

    // H2: on >=80% of holdout, guided.final == exhaustive.final and guided
    // candidates <= 1/2 exhaustive candidates.
    let holdout: Vec<&str> = names
        .iter()
        .copied()
        .filter(|n| find(rows, n, "guided").set == WorkloadSet::Holdout)
        .collect();
    let mut h2_hits = 0usize;
    for &n in &holdout {
        let e = find(rows, n, "exhaustive").result.final_bytes;
        let g = find(rows, n, "guided").result.final_bytes;
        let ec = find(rows, n, "exhaustive").result.candidates_evaluated;
        let gc = find(rows, n, "guided").result.candidates_evaluated;
        if g == e && gc.saturating_mul(2) <= ec {
            h2_hits += 1;
        }
    }
    let h2 = holdout.is_empty() || h2_hits * 5 >= holdout.len() * 4;

    // H3: honest-failure conditions.
    let all_equal = names.iter().all(|&n| {
        find(rows, n, "fixed").result.final_bytes == find(rows, n, "exhaustive").result.final_bytes
    });
    let mut benefits: Vec<u64> = Vec::new();
    for &n in &names {
        let f = find(rows, n, "fixed").result.final_bytes;
        let e = find(rows, n, "exhaustive").result.final_bytes;
        benefits.push(f.saturating_sub(e).saturating_mul(1000) / f.max(1));
    }
    benefits.sort_unstable();
    let median_benefit_permille = benefits.get(benefits.len() / 2).copied().unwrap_or(0);
    let mut overhead: Vec<u64> = Vec::new();
    for &n in &names {
        let fc = find(rows, n, "fixed").result.cpu_ns;
        let gc = find(rows, n, "guided").result.cpu_ns;
        overhead.push(gc.saturating_sub(fc).saturating_mul(1000) / fc.max(1));
    }
    overhead.sort_unstable();
    let median_overhead_permille = overhead.get(overhead.len() / 2).copied().unwrap_or(0);
    let h3 = all_equal
        || median_benefit_permille < governor::EPSILON_PERMILLE
        || median_overhead_permille > median_benefit_permille;

    // H4: negative controls stop Raw and match the RAW descriptor bytes exactly.
    let mut h4 = true;
    let mut h4_fail: Vec<&str> = Vec::new();
    for &n in &names {
        let r = find(rows, n, "guided");
        if !r.negative {
            continue;
        }
        let expected = raw_only(&find_bytes(n));
        if r.result.winner != CandidateKind::Raw
            || r.result.stopped != Some(governor::Accept::Raw)
            || r.result.bytes != expected
        {
            h4 = false;
            h4_fail.push(n);
        }
    }

    format!(
        "{{\"H1_never_worse\":{},\"H1_failures\":[{}],\
         \"H2_approaches_exhaustive\":{},\"H2_holdout_hits\":{},\"H2_holdout_total\":{},\
         \"H3_honest_failure\":{},\"H3_all_equal\":{},\"H3_median_benefit_permille\":{},\
         \"H3_median_overhead_permille\":{},\
         \"H4_negative_control_raw\":{},\"H4_failures\":[{}]}}",
        h1,
        h1_fail
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(","),
        h2,
        h2_hits,
        holdout.len(),
        h3,
        all_equal,
        median_benefit_permille,
        median_overhead_permille,
        h4,
        h4_fail
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(","),
    )
}

/// Recover a workload's bytes from the global list (for the RAW comparison).
fn find_bytes(name: &str) -> Vec<u8> {
    workloads()
        .into_iter()
        .find(|w| w.name == name)
        .map(|w| w.bytes)
        .unwrap_or_default()
}

fn main() {
    let out: PathBuf = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("evidence/scratch/phase10"));
    fs::create_dir_all(&out).expect("create output dir");

    let list = workloads();
    let mut rows: Vec<Row> = Vec::new();
    for w in &list {
        eprintln!(
            "[governor-court] {} ({} B, {:?})",
            w.name,
            w.bytes.len(),
            w.set
        );
        for strategy in ["fixed", "exhaustive", "guided"] {
            rows.push(run_one(w, strategy));
        }
    }

    let results: Vec<String> = rows.iter().map(row_json).collect();
    fs::write(
        out.join("results.json"),
        format!("{{\"schema\":1,\"rows\":[{}]}}\n", results.join(",")),
    )
    .expect("write results");

    let wl: Vec<String> = list
        .iter()
        .map(|w| {
            format!(
                "{{\"name\":\"{}\",\"set\":\"{}\",\"negative\":{},\"source_len\":{}}}",
                jesc(w.name),
                set_name(w.set),
                w.negative,
                w.bytes.len()
            )
        })
        .collect();
    fs::write(
        out.join("workloads.json"),
        format!("{{\"schema\":1,\"workloads\":[{}]}}\n", wl.join(",")),
    )
    .expect("write workloads");

    fs::write(
        out.join("hypotheses.json"),
        format!("{}\n", evaluate(&rows)),
    )
    .expect("write hypotheses");

    // Governed descriptors + their sources, for the decode-without-the-feature
    // proof. `many.pdf`'s winner needs only `rans` (default); `flate.pdf`'s
    // winner needs `deflate-replay` (the underlying capability, exactly as today).
    for name in ["flate.pdf", "many.pdf"] {
        if let Some(w) = list.iter().find(|w| w.name == name) {
            let guided = governor::run_guided(&w.bytes, Limits::DEFAULT).expect("guided");
            let stem = name.trim_end_matches(".pdf");
            fs::write(out.join(format!("governed-{stem}.voldoc")), &guided.bytes)
                .expect("write governed");
            fs::write(out.join(format!("governed-{stem}.src")), &w.bytes).expect("write source");
            eprintln!(
                "[governor-court] governed {name}: winner={} bytes={} candidates={}",
                guided.winner.name(),
                guided.final_bytes,
                guided.candidates_evaluated
            );
        }
    }

    println!(
        "{:<16} {:>7} {:<8} {:<10} {:<28} {:>8} {:>9} {:>10}",
        "workload", "src", "set", "strategy", "winner", "final", "cands", "wall_ms"
    );
    for r in &rows {
        println!(
            "{:<16} {:>7} {:<8} {:<10} {:<28} {:>8} {:>9} {:>10}",
            r.name,
            r.source_len,
            set_name(r.set),
            r.strategy,
            r.result.winner.name(),
            r.result.final_bytes,
            r.result.candidates_evaluated,
            r.result.wall_ns / 1_000_000,
        );
    }
    println!("hypotheses: {}", evaluate(&rows));
}
