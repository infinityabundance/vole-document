//! Encoder-only search governance: typed residual diagnostics, a parametric
//! candidate space, and a pure governor — all with **zero decode authority**.
//!
//! This module is compiled only under the non-default, **dependency-free**
//! `dsfb-search` feature. It adds no wire bytes, no feature bit, and no decoder
//! behavior: every candidate it can enable is built from the *same* existing
//! generators, then serialized, parsed, materialized, and byte-compared by the
//! unmodified [`crate::encode::court::run`]. A descriptor produced under
//! governance decodes in any build without this feature.
//!
//! The name `dsfb` is retained for continuity with the Phase-10 brief, but the
//! published `dsfb 0.1.2` crate is **Drift-Slew Fusion Bootstrap state
//! estimation** — a `f64`/`rand`-backed observer with no candidate-family,
//! residual, or search-directive API — so it is deliberately **not** a
//! dependency. See `docs/adr/0022-encoder-only-search-governance.md`.
//!
//! Nothing here is persisted into a `.voldoc`. [`ResidualTrace`] and its
//! diagnostics are transient encoder-side observations; the decode path
//! (`materialize`, `container`, `dra`, `store`) never imports this module.

use std::collections::HashSet;
use std::time::Instant;

use crate::SOURCE_FORMAT_PDF;
use crate::accounting::CostBreakdown;
use crate::adapter::pdf::channels::{self, KIND_COUNT};
use crate::adapter::pdf::cos::FilterClass;
use crate::adapter::pdf::physical::{ObjRole, PhysicalKind};
use crate::container::{Descriptor, UNIVERSE};
use crate::dra::{Op, Program};
use crate::encode::candidates::{Candidate, CandidateKind};
use crate::encode::court;
use crate::error::Result;
use crate::integrity::sha256;
use crate::limits::Limits;

#[cfg(feature = "rans")]
use crate::entropy::{
    ALPHABET, CODER_ORDER0_BYTE_RANS, CODER_VERSION_1, EntropyChannelDescriptor, EntropyModel,
    encode_channel,
};

// ---------------------------------------------------------------------------
// Tunable constants — frozen in the Phase-10.0 contract, never retuned on the
// holdout set (see `research/subagents/phase-10/dsfb-contract.md` §3.2/§4).
// ---------------------------------------------------------------------------

/// `RRaw ≥ RAW_FRAC_PERMILLE·source_len / 1000` marks the RAW floor.
pub const RAW_FRAC_PERMILLE: u64 = 900;
/// Sources at or below this size are dominated by per-candidate framing.
pub const SMALL_SOURCE_BYTES: u64 = 4096;
/// Improvement below this (permille of the incumbent) does not "pay".
pub const EPSILON_PERMILLE: u64 = 5;
/// Advisory local-context window reported by a diagnostic.
pub const LOCAL_CONTEXT_BYTES: u64 = 4096;
/// Maximum prefix length of the fixed family order (`0..=DEPTH_MAX`).
pub const DEPTH_MAX: u8 = 9;

/// Half-open byte range `[start, start+len)` of the source a diagnostic describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceRegion {
    /// First byte of the region.
    pub start: u64,
    /// Number of bytes in the region.
    pub len: u64,
}

/// How the source is structured at the diagnostic's region (advisory, from the
/// physical scan; never authority).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatStructure {
    /// No format adapter applies.
    Opaque,
    /// A physical PDF span cover applies.
    PdfPhysical(PhysicalKind),
    /// A PDF stream span applies, with the classified `/Filter`.
    PdfStream(FilterClass),
    /// A PDF cross-reference-stream object applies.
    PdfXref,
    /// A PDF trailer applies.
    PdfTrailer,
}

/// Candidate family that produced a diagnostic (same type as the generator id).
pub use crate::encode::candidates::CandidateKind as Family;

/// The typed residual taxonomy (frozen; one arm per real mechanism observed).
///
/// This is an encoder-side **cost attribution** over the priced candidate — it
/// is not a claim that a generating program was discovered, and it is distinct
/// from the wire record category `CostBreakdown::residuals` (always 0 here).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResidualClass {
    /// Structural bytes not predicted (xref offsets, `startxref`, op framing).
    RStruct,
    /// Numeric/literal values stored verbatim.
    RValue,
    /// Token bytes a typed channel does not model (per-kind payloads).
    RLexical,
    /// Ordering/transition residual of the kind stream.
    ROrder,
    /// Container/framing every candidate pays.
    RPackage,
    /// Entropy inefficiency + model overhead.
    RCodec,
    /// Bytes no mechanism explains — the RAW floor.
    RRaw,
}

impl ResidualClass {
    /// Stable short name for reports and receipts.
    pub const fn name(self) -> &'static str {
        match self {
            ResidualClass::RStruct => "RStruct",
            ResidualClass::RValue => "RValue",
            ResidualClass::RLexical => "RLexical",
            ResidualClass::ROrder => "ROrder",
            ResidualClass::RPackage => "RPackage",
            ResidualClass::RCodec => "RCodec",
            ResidualClass::RRaw => "RRaw",
        }
    }
}

/// Integer-only run statistics (no floats; reproducible across platforms).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunStats {
    /// Number of maximal runs of equal bytes.
    pub run_count: u64,
    /// Length of the longest run.
    pub max_run: u64,
    /// Mean run length, permille (milli-bytes).
    pub mean_run_milli: u64,
}

/// Best detected period of a byte stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Periodicity {
    /// Candidate period in bytes.
    pub period_bytes: u64,
    /// Share of positions that match the period, permille.
    pub strength_permille: u16,
}

/// Four-gram repetition structure of a byte stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recurrence {
    /// Number of distinct 4-grams observed.
    pub distinct_4grams: u64,
    /// Fraction of 4-gram windows that repeat an earlier window, permille.
    pub repeat_ratio_permille: u16,
}

/// One encoder-only observation over a priced candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResidualDiagnostic {
    /// Source region this diagnostic describes.
    pub source: SourceRegion,
    /// Source structure at the region.
    pub structure: FormatStructure,
    /// Candidate family.
    pub family: Family,
    /// Dominant residual class of this priced candidate.
    pub class: ResidualClass,
    /// Bytes attributed to the dominant class.
    pub residual_bytes: u64,
    /// Run statistics of the source.
    pub runs: RunStats,
    /// Best detected period of the source, if any.
    pub periodicity: Option<Periodicity>,
    /// Four-gram repetition structure of the source.
    pub recurrence: Recurrence,
    /// Advisory local-context window in bytes.
    pub local_context_bytes: u64,
    /// Complete serialized cost of the candidate (`== bytes.len()`).
    pub candidate_complete_cost: u64,
}

/// The priced winner of a court run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Priced {
    /// Winning family.
    pub family: CandidateKind,
    /// Serialized byte length.
    pub bytes_len: u64,
    /// Reconstruction work (DRA instruction count).
    pub graph_ops: usize,
}

/// A bounded search budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// Maximum total candidates admitted across the whole search.
    pub max_candidates: u64,
    /// Maximum number of configs priced.
    pub max_configs: u32,
    /// Wall-clock ceiling in milliseconds (advisory; the driver is synchronous).
    pub max_wall_ms: u64,
}

impl Budget {
    /// Budget used by the DsfbGuided driver.
    pub const GUIDED: Budget = Budget {
        max_candidates: 1 << 20,
        max_configs: 18,
        max_wall_ms: 120_000,
    };
}

/// The whole encoder-only trace for one workload.
#[derive(Debug, Clone)]
pub struct ResidualTrace {
    /// Total source length in bytes.
    pub source_len: u64,
    /// One diagnostic per priced candidate (the config winner).
    pub diagnostics: Vec<ResidualDiagnostic>,
    /// Current best priced candidate.
    pub best: Option<Priced>,
    /// Search budget.
    pub budget: Budget,
}

/// Which lexical partition a config's typed-channel candidate uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Partition {
    /// The existing 12-kind id partition.
    ByKind,
    /// A coarse 3-role partition (`0..=2` of the same 12-channel space).
    ByRole,
}

/// Which DEFLATE replay lanes a config admits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayMode {
    /// No replay candidates.
    Off,
    /// Raw plaintext replay (`PDF_DEFLATE_REPLAY`).
    Dedup,
    /// Shared rANS plaintext replay (`PDF_DEFLATE_REPLAY_RANS[_INDEXED]`).
    DedupRans,
}

/// A point in the (tiny, entirely mechanism-reusing) parametric search space.
///
/// Every knob is wire-legal with **no decoder change** (see
/// `docs/adr/0022-encoder-only-search-governance.md`): `scale_bits` is a
/// self-describing channel parameter; `partition` reuses the opaque kind-id
/// space; `replay`/`packed`/`depth` only *select* which existing generators run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchConfig {
    /// rANS model scale bits: one of `{8, 10, 12}` (coalesced to `12` otherwise).
    pub scale_bits: u8,
    /// Typed-channel partition.
    pub partition: Partition,
    /// DEFLATE replay lanes.
    pub replay: ReplayMode,
    /// Whether packed-framing (`PACK_SEGMENTS`/`PACKED_CHANNELS`) layout lanes run.
    pub packed: bool,
    /// Prefix length of the fixed family order (`0..=DEPTH_MAX`).
    pub depth: u8,
}

impl SearchConfig {
    /// The fixed heuristic: today's defaults (`scale_bits=12`, `ByKind`,
    /// `DedupRans`, packed, every family).
    pub const DEFAULT: SearchConfig = SearchConfig {
        scale_bits: 12,
        partition: Partition::ByKind,
        replay: ReplayMode::DedupRans,
        packed: true,
        depth: DEPTH_MAX,
    };

    /// The default config with a different `scale_bits`.
    pub const fn by_scale(scale_bits: u8) -> Self {
        SearchConfig {
            scale_bits,
            ..Self::DEFAULT
        }
    }

    /// The default config with the coarse role partition.
    pub const fn by_role() -> Self {
        SearchConfig {
            partition: Partition::ByRole,
            ..Self::DEFAULT
        }
    }
}

/// The exhaustive grid: `scale(3) × partition(2) × replay(3) × depth(5) = 90`.
///
/// Order is fixed. The first six entries are the *maximal* configs
/// (`depth=DEPTH_MAX`, `DedupRans`, packed) for each `(partition, scale_bits)`;
/// because a deeper / replay-richer / packed config's candidate set is a
/// superset of a shallower one's, the byte minimum over the whole grid equals
/// the minimum over those six.
pub fn config_grid() -> Vec<SearchConfig> {
    let mut grid = Vec::with_capacity(90);
    for &depth in &[9u8, 6, 4, 2, 0] {
        for &replay in &[ReplayMode::DedupRans, ReplayMode::Dedup, ReplayMode::Off] {
            for &partition in &[Partition::ByKind, Partition::ByRole] {
                for &scale_bits in &[12u8, 10, 8] {
                    grid.push(SearchConfig {
                        scale_bits,
                        partition,
                        replay,
                        packed: true,
                        depth,
                    });
                }
            }
        }
    }
    grid
}

/// Whether a config's candidate subset includes `kind`.
fn config_includes(cfg: SearchConfig, kind: CandidateKind) -> bool {
    let family_index = family_depth(kind);
    if family_index > cfg.depth {
        return false;
    }
    match kind {
        CandidateKind::ByteRans => cfg!(feature = "rans"),
        CandidateKind::PdfChannels => cfg!(feature = "rans"),
        CandidateKind::PdfLayout => cfg.packed,
        CandidateKind::PdfLayoutRans => cfg.packed && cfg!(feature = "rans"),
        CandidateKind::PdfDeflateReplay => cfg.replay != ReplayMode::Off,
        CandidateKind::PdfDeflateReplayRans | CandidateKind::PdfDeflateReplayRansIndexed => {
            cfg.replay == ReplayMode::DedupRans
        }
        _ => true,
    }
}

/// Position of a family in the fixed family order.
const fn family_depth(kind: CandidateKind) -> u8 {
    match kind {
        CandidateKind::Raw => 0,
        CandidateKind::Rle => 1,
        CandidateKind::ByteRans => 2,
        CandidateKind::PdfPhysical => 3,
        CandidateKind::PdfChannels => 4,
        CandidateKind::PdfLayout => 5,
        CandidateKind::PdfLayoutRans => 6,
        CandidateKind::PdfDeflateReplay => 7,
        CandidateKind::PdfDeflateReplayRans => 8,
        CandidateKind::PdfDeflateReplayRansIndexed => 9,
    }
}

/// A governor directive. `Widen` is a budget delta only; it has no wire effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchDirective {
    /// Price a specific config's candidate set.
    Next(SearchConfig),
    /// Admit the next deeper family for a family.
    Deepen(CandidateKind),
    /// Budget delta (candidates/configs/wall).
    Widen(i32),
    /// Cease searching.
    Stop(Accept),
}

/// What a `Stop` accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accept {
    /// The RAW floor.
    Raw,
    /// The best candidate priced so far.
    BestSoFar,
}

// ---------------------------------------------------------------------------
// Parametric candidate generation.
// ---------------------------------------------------------------------------

/// Coalesce a requested `scale_bits` onto the frozen `{8, 10, 12}` lattice.
const fn normalize_scale_bits(scale_bits: u8) -> u8 {
    match scale_bits {
        8 => 8,
        10 => 10,
        _ => 12,
    }
}

/// Build the candidate set honoring `cfg`.
///
/// This reuses the existing generators and never changes their default behavior:
/// at [`SearchConfig::DEFAULT`] it returns exactly the same candidates, in the
/// same order, as [`crate::encode::candidates::propose_all`].
pub fn propose_configured(
    input: &[u8],
    cfg: SearchConfig,
    limits: Limits,
) -> Result<Vec<Candidate>> {
    let mut out: Vec<Candidate> = Vec::new();

    out.push(Candidate {
        kind: CandidateKind::Raw,
        descriptor: crate::adapter::opaque::propose(input, limits)?,
    });

    if cfg.depth >= family_depth(CandidateKind::Rle)
        && let Some(c) = crate::encode::candidates::propose_rle(input, limits)?
    {
        out.push(c);
    }

    #[cfg(feature = "rans")]
    if cfg.depth >= family_depth(CandidateKind::ByteRans)
        && cfg.scale_bits != 0
        && let Some(c) = propose_byte_rans_scaled(input, limits, cfg.scale_bits)?
    {
        out.push(c);
    }

    if cfg.depth >= family_depth(CandidateKind::PdfPhysical)
        && let Some(c) = crate::adapter::pdf::propose_pdf(input, limits)?
    {
        out.push(c);
    }

    #[cfg(feature = "rans")]
    if cfg.depth >= family_depth(CandidateKind::PdfChannels)
        && let Some(c) = propose_channels_scaled(input, limits, cfg.partition, cfg.scale_bits)?
    {
        out.push(c);
    }

    if cfg.depth >= family_depth(CandidateKind::PdfLayout)
        && cfg.packed
        && let Some(c) = crate::adapter::pdf::propose_pdf_layout(input, limits)?
    {
        out.push(c);
    }

    #[cfg(feature = "rans")]
    if cfg.depth >= family_depth(CandidateKind::PdfLayoutRans)
        && cfg.packed
        && let Some(c) = crate::adapter::pdf::propose_pdf_layout_rans(input, limits)?
    {
        out.push(c);
    }

    #[cfg(feature = "deflate-replay")]
    if cfg.depth >= family_depth(CandidateKind::PdfDeflateReplay)
        && cfg.replay != ReplayMode::Off
        && let Some(c) = crate::adapter::pdf::propose_pdf_deflate_replay(input, limits)?
    {
        out.push(c);
    }

    #[cfg(all(feature = "deflate-replay", feature = "rans"))]
    if cfg.depth >= family_depth(CandidateKind::PdfDeflateReplayRans)
        && cfg.replay == ReplayMode::DedupRans
        && let Some(c) = crate::adapter::pdf::propose_pdf_deflate_replay_rans(input, limits)?
    {
        out.push(c);
    }

    #[cfg(all(feature = "deflate-replay", feature = "rans"))]
    if cfg.depth >= family_depth(CandidateKind::PdfDeflateReplayRansIndexed)
        && cfg.replay == ReplayMode::DedupRans
        && let Some(c) =
            crate::adapter::pdf::propose_pdf_deflate_replay_rans_indexed(input, limits)?
    {
        out.push(c);
    }

    Ok(out)
}

/// Order-0 byte-rANS candidate at a chosen `scale_bits` (mirrors
/// [`crate::encode::candidates::propose_byte_rans`], which hardcodes 12).
#[cfg(feature = "rans")]
fn propose_byte_rans_scaled(
    input: &[u8],
    limits: Limits,
    scale_bits: u8,
) -> Result<Option<Candidate>> {
    if input.is_empty() || input.len() as u64 > limits.max_channel_symbols {
        return Ok(None);
    }
    let scale_bits = normalize_scale_bits(scale_bits);
    let mut counts = [0u64; ALPHABET];
    for &b in input {
        counts[b as usize] += 1;
    }
    let model = EntropyModel::from_counts(&counts, scale_bits)?;
    let capsule = encode_channel(&model, input)?;

    let channel = EntropyChannelDescriptor {
        coder: CODER_ORDER0_BYTE_RANS,
        coder_version: CODER_VERSION_1,
        scale_bits: model.scale_bits,
        lane_count: 1,
        model_id: 0,
        symbol_count: capsule.symbol_count,
        decoded_length: capsule.decoded_length,
        initial_state: capsule.initial_state,
        payload: capsule.payload,
    };

    let descriptor = Descriptor {
        universe: UNIVERSE.to_string(),
        source_format: crate::SOURCE_FORMAT_OPAQUE,
        format_basis: "opaque;byte-rans".to_string(),
        models: vec![model],
        channels: vec![channel],
        objects: vec![],
        program: Program::new(vec![Op::DecodeChannel { channel_id: 0 }]),
        observation_index: None,
        seek_directory: false,
        source_sha256: sha256(input),
        source_len: input.len() as u64,
    };

    Ok(Some(Candidate {
        kind: CandidateKind::ByteRans,
        descriptor,
    }))
}

/// Typed-channel candidate at a chosen partition and `scale_bits` (mirrors
/// [`crate::adapter::pdf::propose_pdf_channels`], which hardcodes 12 / `ByKind`).
#[cfg(feature = "rans")]
fn propose_channels_scaled(
    input: &[u8],
    limits: Limits,
    partition: Partition,
    scale_bits: u8,
) -> Result<Option<Candidate>> {
    if !crate::adapter::pdf::detect(input, limits) {
        return Ok(None);
    }
    let plan = match partition {
        Partition::ByKind => channels::split(input, limits)?,
        Partition::ByRole => channels::split_role(input, limits)?,
    };
    let Some(plan) = plan else {
        return Ok(None);
    };
    let scale_bits = normalize_scale_bits(scale_bits);

    let mut streams: Vec<Vec<u8>> = Vec::with_capacity(2 + KIND_COUNT);
    streams.push(plan.kinds.clone());
    let mut lengths = Vec::with_capacity(plan.lengths.len() * 4);
    for &len in &plan.lengths {
        lengths.extend_from_slice(&len.to_le_bytes());
    }
    streams.push(lengths);
    for payload in &plan.payloads {
        streams.push(payload.clone());
    }

    let mut models = Vec::with_capacity(streams.len());
    let mut encoded = Vec::with_capacity(streams.len());
    for stream in &streams {
        let mut counts = [0u64; ALPHABET];
        for &b in stream {
            counts[b as usize] += 1;
        }
        let model = EntropyModel::from_counts(&counts, scale_bits)?;
        let sb = model.scale_bits;
        let capsule = encode_channel(&model, stream)?;
        let model_id = models.len() as u32;
        models.push(model);
        encoded.push(EntropyChannelDescriptor {
            coder: CODER_ORDER0_BYTE_RANS,
            coder_version: CODER_VERSION_1,
            scale_bits: sb,
            lane_count: 1,
            model_id,
            symbol_count: capsule.symbol_count,
            decoded_length: capsule.decoded_length,
            initial_state: capsule.initial_state,
            payload: capsule.payload,
        });
    }

    let program = Program::new(vec![Op::InterleaveChannels {
        kinds_channel: 0,
        lengths_channel: 1,
        first_payload_channel: 2,
        payload_channel_count: KIND_COUNT as u8,
    }]);

    let format_basis = format!(
        "pdf-channels;kinds={};tokens={};channels={}",
        KIND_COUNT,
        plan.token_count(),
        encoded.len()
    );

    let descriptor = Descriptor {
        universe: UNIVERSE.to_string(),
        source_format: SOURCE_FORMAT_PDF,
        format_basis,
        models,
        channels: encoded,
        objects: vec![],
        program,
        observation_index: None,
        seek_directory: false,
        source_sha256: sha256(input),
        source_len: input.len() as u64,
    };

    Ok(Some(Candidate {
        kind: CandidateKind::PdfChannels,
        descriptor,
    }))
}

// ---------------------------------------------------------------------------
// Diagnostics.
// ---------------------------------------------------------------------------

/// Classify the source structure (advisory; re-derived from the bytes).
fn classify(input: &[u8], limits: Limits) -> FormatStructure {
    match crate::adapter::pdf::scan(input, limits) {
        Ok(p) => {
            if p.header.is_none() || p.objects.is_empty() || p.eofs.is_empty() {
                return FormatStructure::Opaque;
            }
            if let Some(s) = p.streams.first() {
                return FormatStructure::PdfStream(s.filter);
            }
            if p.objects
                .iter()
                .any(|o| matches!(o.role, ObjRole::XRefStream))
            {
                return FormatStructure::PdfXref;
            }
            if p.spans
                .iter()
                .any(|s| matches!(s.kind, PhysicalKind::Trailer))
            {
                return FormatStructure::PdfTrailer;
            }
            let kind = p
                .spans
                .first()
                .map(|s| s.kind)
                .unwrap_or(PhysicalKind::Unclassified);
            FormatStructure::PdfPhysical(kind)
        }
        Err(_) => FormatStructure::Opaque,
    }
}

/// Maximal-run statistics of a byte stream.
fn run_stats(input: &[u8]) -> RunStats {
    if input.is_empty() {
        return RunStats {
            run_count: 0,
            max_run: 0,
            mean_run_milli: 0,
        };
    }
    let mut run_count: u64 = 1;
    let mut max_run: u64 = 1;
    let mut current: u64 = 1;
    for w in input.windows(2) {
        if w[0] == w[1] {
            current += 1;
            max_run = max_run.max(current);
        } else {
            current = 1;
            run_count += 1;
        }
    }
    RunStats {
        run_count,
        max_run,
        mean_run_milli: (input.len() as u64).saturating_mul(1000) / run_count,
    }
}

/// Best integer period up to 256 bytes (highest match share; ties → shorter).
fn periodicity(input: &[u8]) -> Option<Periodicity> {
    let len = input.len();
    if len < 4 {
        return None;
    }
    let max_period = (len / 2).min(256);
    let mut best: Option<Periodicity> = None;
    for period in 1..=max_period {
        let mut matches: u64 = 0;
        let mut total: u64 = 0;
        for (i, &b) in input.iter().enumerate().skip(period) {
            total += 1;
            if b == input[i - period] {
                matches += 1;
            }
        }
        if total == 0 {
            continue;
        }
        let strength = (matches.saturating_mul(1000) / total) as u16;
        if strength == 0 {
            continue;
        }
        match best {
            Some(b) if b.strength_permille >= strength => {}
            _ => {
                best = Some(Periodicity {
                    period_bytes: period as u64,
                    strength_permille: strength,
                });
            }
        }
    }
    best
}

/// Four-gram repetition structure (bounded to 1<<20 windows).
fn recurrence(input: &[u8]) -> Recurrence {
    let len = input.len();
    if len < 4 {
        return Recurrence {
            distinct_4grams: 0,
            repeat_ratio_permille: 0,
        };
    }
    let windows = (len - 3).min(1 << 20);
    let mut seen: HashSet<u32> = HashSet::with_capacity(windows.min(1 << 16));
    let mut repeats: u64 = 0;
    for chunk in input.windows(4).take(windows) {
        let key = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        if !seen.insert(key) {
            repeats += 1;
        }
    }
    Recurrence {
        distinct_4grams: seen.len() as u64,
        repeat_ratio_permille: (repeats.saturating_mul(1000) / windows as u64) as u16,
    }
}

/// Split a typed-channel candidate's channel payloads into the `kinds`+`lengths`
/// (order) channels and the per-kind payload (lexical) channels.
fn channel_payload_split(descriptor: Option<&Descriptor>) -> (u64, u64) {
    let Some(d) = descriptor else {
        return (0, 0);
    };
    let mut order: u64 = 0;
    let mut lexical: u64 = 0;
    for (i, ch) in d.channels.iter().enumerate() {
        let n = ch.payload.len() as u64;
        if i < 2 {
            order += n;
        } else {
            lexical += n;
        }
    }
    (order, lexical)
}

/// Attribute a candidate's complete cost to the residual classes and return the
/// dominant class. The attribution sums exactly to `cost.total()`.
fn ledger(
    kind: CandidateKind,
    descriptor: Option<&Descriptor>,
    cost: &CostBreakdown,
) -> (ResidualClass, [u64; 7]) {
    const STRUCT: usize = 0;
    const VALUE: usize = 1;
    const LEXICAL: usize = 2;
    const ORDER: usize = 3;
    const PACKAGE: usize = 4;
    const CODEC: usize = 5;
    const RAW: usize = 6;

    let package = cost.header
        + cost.universe
        + cost.format
        + cost.record_framing
        + cost.integrity
        + cost.trailer
        + cost.index
        + cost.directory;
    let codec = cost.models + cost.entropy_payload;
    let raw = cost.objects + cost.external_refs + cost.residuals;
    let graph = cost.graph;

    let mut l = [0u64; 7];
    l[PACKAGE] += package;
    l[RAW] += raw;

    match kind {
        CandidateKind::Raw => l[RAW] += graph,
        CandidateKind::Rle => l[VALUE] += graph,
        CandidateKind::ByteRans => {
            l[CODEC] += codec;
            l[VALUE] += graph;
        }
        CandidateKind::PdfPhysical => l[STRUCT] += graph,
        CandidateKind::PdfChannels => {
            let (order_payload, lexical_payload) = channel_payload_split(descriptor);
            l[ORDER] += order_payload + graph;
            l[LEXICAL] += lexical_payload;
            l[CODEC] += codec.saturating_sub(order_payload + lexical_payload);
        }
        CandidateKind::PdfLayout => l[STRUCT] += graph,
        CandidateKind::PdfLayoutRans => {
            l[CODEC] += codec;
            l[STRUCT] += graph;
        }
        CandidateKind::PdfDeflateReplay
        | CandidateKind::PdfDeflateReplayRans
        | CandidateKind::PdfDeflateReplayRansIndexed => l[CODEC] += codec + graph,
    }

    let class = |i: usize| match i {
        STRUCT => ResidualClass::RStruct,
        VALUE => ResidualClass::RValue,
        LEXICAL => ResidualClass::RLexical,
        ORDER => ResidualClass::ROrder,
        PACKAGE => ResidualClass::RPackage,
        CODEC => ResidualClass::RCodec,
        _ => ResidualClass::RRaw,
    };

    let mut dom = 0usize;
    let mut best = l[0];
    for (i, &v) in l.iter().enumerate().skip(1) {
        if v > best {
            best = v;
            dom = i;
        }
    }
    (class(dom), l)
}

/// Build one encoder-only diagnostic for a priced candidate.
pub fn diagnose(
    input: &[u8],
    kind: CandidateKind,
    descriptor: Option<&Descriptor>,
    cost: &CostBreakdown,
    limits: Limits,
) -> ResidualDiagnostic {
    let (class, l) = ledger(kind, descriptor, cost);
    ResidualDiagnostic {
        source: SourceRegion {
            start: 0,
            len: input.len() as u64,
        },
        structure: classify(input, limits),
        family: kind,
        class,
        residual_bytes: l[class as usize],
        runs: run_stats(input),
        periodicity: periodicity(input),
        recurrence: recurrence(input),
        local_context_bytes: (input.len() as u64).min(LOCAL_CONTEXT_BYTES),
        candidate_complete_cost: cost.total(),
    }
}

// ---------------------------------------------------------------------------
// The governor.
// ---------------------------------------------------------------------------

/// Pure, deterministic, integer-only decision function.
///
/// It maps the *dominant residual of the current best candidate* to a directive
/// using the frozen table (ADR-0022 §Decision). It has no RNG, no floats, no I/O,
/// and no persistence; its only effect is to choose which existing generators run.
pub fn govern(trace: &ResidualTrace) -> SearchDirective {
    if trace.diagnostics.is_empty() {
        return SearchDirective::Next(SearchConfig::DEFAULT);
    }
    if trace.diagnostics.len() as u32 >= trace.budget.max_configs {
        return SearchDirective::Stop(Accept::BestSoFar);
    }

    let raw_diag = trace
        .diagnostics
        .iter()
        .find(|d| d.family == CandidateKind::Raw);
    let best = trace.best.as_ref();

    // Explain the incumbent: its diagnostic, else the largest residual.
    let dom = best
        .and_then(|b| {
            trace
                .diagnostics
                .iter()
                .find(|d| d.family == b.family && d.candidate_complete_cost == b.bytes_len)
        })
        .or_else(|| trace.diagnostics.iter().max_by_key(|d| d.residual_bytes));
    let Some(dom) = dom else {
        return SearchDirective::Stop(Accept::BestSoFar);
    };

    let frac = dom
        .residual_bytes
        .saturating_mul(1000)
        .checked_div(trace.source_len)
        .unwrap_or(1000);

    // Rule 1 (RAW floor), independent of the incumbent's dominant class: stop
    // when the complete-cost court already prefers RAW over every enabled
    // mechanism, or when a dominant unexplained residual leaves RAW as cheap.
    let raw_is_floor = best
        .map(|b| b.family == CandidateKind::Raw)
        .unwrap_or(false)
        || (dom.class == ResidualClass::RRaw
            && frac >= RAW_FRAC_PERMILLE
            && best
                .zip(raw_diag)
                .map(|(b, r)| b.bytes_len >= r.candidate_complete_cost)
                .unwrap_or(false));
    if raw_is_floor {
        return SearchDirective::Stop(Accept::Raw);
    }

    match dom.class {
        ResidualClass::RRaw => SearchDirective::Next(SearchConfig::DEFAULT),
        ResidualClass::RCodec => SearchDirective::Next(SearchConfig::by_scale(10)),
        ResidualClass::RStruct => SearchDirective::Deepen(CandidateKind::PdfLayoutRans),
        ResidualClass::RLexical | ResidualClass::ROrder => {
            SearchDirective::Next(SearchConfig::by_role())
        }
        ResidualClass::RValue => SearchDirective::Deepen(CandidateKind::ByteRans),
        ResidualClass::RPackage => {
            if trace.source_len <= SMALL_SOURCE_BYTES {
                SearchDirective::Stop(Accept::BestSoFar)
            } else {
                SearchDirective::Widen(-1)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Drivers + court.
// ---------------------------------------------------------------------------

/// Which corpus set a workload belongs to (overfitting guard).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkloadSet {
    /// Thresholds may be inspected here.
    Tune,
    /// H2 is judged **only** here.
    Holdout,
    /// A negative control (governance must `Stop(Raw)`).
    NegativeControl,
}

/// One deterministic in-memory workload.
#[derive(Debug, Clone)]
pub struct Workload {
    /// Stable name.
    pub name: &'static str,
    /// Exact bytes.
    pub bytes: Vec<u8>,
    /// Which set it belongs to.
    pub set: WorkloadSet,
    /// Whether it is one of the deliberate non-PDF controls.
    pub negative: bool,
}

/// Deterministic xorshift64 for incompressible synthetic input.
fn xorshift_bytes(n: usize, seed: u64) -> Vec<u8> {
    let mut state = seed | 1;
    let mut out = Vec::with_capacity(n + 8);
    while out.len() < n {
        let mut x = state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        state = x;
        out.extend_from_slice(&x.to_le_bytes());
    }
    out.truncate(n);
    out
}

/// The tune / holdout / control workload set (small, locally generated).
pub fn workloads() -> Vec<Workload> {
    use crate::adapter::pdf::samples::{is_negative_control, sample_pdfs};

    let pick = |name: &str| -> Vec<u8> {
        sample_pdfs()
            .into_iter()
            .find(|(n, _)| *n == name)
            .map(|(_, b)| b)
            .unwrap_or_else(|| panic!("sample {name} not found"))
    };

    let tune = ["classic.pdf", "objstm.pdf", "flate.pdf"];
    let holdout = [
        "xrefstream.pdf",
        "incremental.pdf",
        "mixedeol.pdf",
        "trapstream.pdf",
        "many.pdf",
        "bigtext.pdf",
        "notpdf.bin",
    ];

    let mut out: Vec<Workload> = Vec::new();
    for name in tune {
        out.push(Workload {
            name,
            bytes: pick(name),
            set: WorkloadSet::Tune,
            negative: is_negative_control(name),
        });
    }
    for name in holdout {
        out.push(Workload {
            name,
            bytes: pick(name),
            set: WorkloadSet::Holdout,
            negative: is_negative_control(name),
        });
    }
    out.push(Workload {
        name: "malformed.pdf",
        bytes: pick("malformed.pdf"),
        set: WorkloadSet::NegativeControl,
        negative: true,
    });

    // Synthetic trio.
    out.push(Workload {
        name: "synthetic-rle",
        bytes: vec![0u8; 8192],
        set: WorkloadSet::Tune,
        negative: false,
    });
    out.push(Workload {
        name: "synthetic-rans",
        bytes: b"The quick brown fox jumps over the lazy dog. ".repeat(200),
        set: WorkloadSet::Tune,
        negative: false,
    });
    out.push(Workload {
        name: "synthetic-raw",
        bytes: xorshift_bytes(8192, 0x9E37_79B9_7F4A_7C15),
        set: WorkloadSet::Holdout,
        negative: false,
    });

    out
}

/// The measured outcome of one strategy on one workload.
#[derive(Debug, Clone)]
pub struct StrategyResult {
    /// Winning family.
    pub winner: CandidateKind,
    /// Final complete serialized bytes.
    pub final_bytes: u64,
    /// Sum of `court::run` `candidates_evaluated` over every config priced.
    pub candidates_evaluated: u64,
    /// Configs priced.
    pub configs_tried: u32,
    /// Process CPU-time delta, nanoseconds.
    pub cpu_ns: u64,
    /// Wall-time delta, nanoseconds.
    pub wall_ns: u64,
    /// Process peak RSS, bytes.
    pub peak_rss_bytes: u64,
    /// The `Stop` accept the governor chose (guided only).
    pub stopped: Option<Accept>,
    /// Dominant residual class observed (max over config winners).
    pub dominant_class: Option<ResidualClass>,
    /// The winning `.voldoc` bytes.
    pub bytes: Vec<u8>,
}

struct Winner {
    bytes: Vec<u8>,
    kind: CandidateKind,
    graph_ops: usize,
}

fn consider(best: &mut Option<Winner>, result: &court::CourtResult) {
    let replace = match best {
        None => true,
        Some(b) => {
            (result.bytes.len(), result.graph_ops, result.kind)
                < (b.bytes.len(), b.graph_ops, b.kind)
        }
    };
    if replace {
        *best = Some(Winner {
            bytes: result.bytes.clone(),
            kind: result.kind,
            graph_ops: result.graph_ops,
        });
    }
}

/// Price one config: run its candidates through the unmodified court and return
/// the result plus the winner's diagnostic.
fn price_config(
    input: &[u8],
    cfg: SearchConfig,
    limits: Limits,
) -> Result<(court::CourtResult, ResidualDiagnostic)> {
    let cands = propose_configured(input, cfg, limits)?;
    let snapshot = cands.clone();
    let result = court::run(input, cands, limits)?;
    let winner = snapshot.iter().find(|c| c.kind == result.kind);
    let diag = diagnose(
        input,
        result.kind,
        winner.map(|c| &c.descriptor),
        &result.cost,
        limits,
    );
    Ok((result, diag))
}

fn finish(
    best: Option<Winner>,
    evaluated: u64,
    configs_tried: u32,
    cpu0: u64,
    wall0: Instant,
    stopped: Option<Accept>,
    dominant_class: Option<ResidualClass>,
) -> StrategyResult {
    let winner = best.expect("at least one candidate is always priced");
    StrategyResult {
        winner: winner.kind,
        final_bytes: winner.bytes.len() as u64,
        candidates_evaluated: evaluated,
        configs_tried,
        cpu_ns: cpu_time_ns().saturating_sub(cpu0),
        wall_ns: wall0.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
        peak_rss_bytes: peak_rss_bytes(),
        stopped,
        dominant_class,
        bytes: winner.bytes,
    }
}

/// Exhaustive driver: enumerate the whole [`config_grid`].
pub fn run_exhaustive(input: &[u8], limits: Limits) -> Result<StrategyResult> {
    let wall0 = Instant::now();
    let cpu0 = cpu_time_ns();
    let mut best: Option<Winner> = None;
    let mut evaluated: u64 = 0;
    let mut configs_tried: u32 = 0;
    let mut dominant: Option<ResidualClass> = None;

    for cfg in config_grid() {
        let (result, diag) = price_config(input, cfg, limits)?;
        evaluated += u64::from(result.candidates_evaluated);
        configs_tried += 1;
        if dominant.map(|d| d < diag.class).unwrap_or(true) {
            dominant = Some(diag.class);
        }
        consider(&mut best, &result);
    }
    Ok(finish(
        best,
        evaluated,
        configs_tried,
        cpu0,
        wall0,
        None,
        dominant,
    ))
}

/// Fixed-heuristic driver: price exactly [`SearchConfig::DEFAULT`].
pub fn run_fixed(input: &[u8], limits: Limits) -> Result<StrategyResult> {
    let wall0 = Instant::now();
    let cpu0 = cpu_time_ns();
    let (result, diag) = price_config(input, SearchConfig::DEFAULT, limits)?;
    let mut best: Option<Winner> = None;
    consider(&mut best, &result);
    Ok(finish(
        best,
        u64::from(result.candidates_evaluated),
        1,
        cpu0,
        wall0,
        None,
        Some(diag.class),
    ))
}

/// Resolve a preferred config to an untried one (prefer `pref`, else the next
/// untried in fixed grid order after it).
fn resolve_untried(
    pref: SearchConfig,
    grid: &[SearchConfig],
    tried: &[SearchConfig],
) -> Option<SearchConfig> {
    if !tried.contains(&pref) {
        return Some(pref);
    }
    let idx = grid.iter().position(|c| *c == pref)?;
    grid[idx..].iter().copied().find(|c| !tried.contains(c))
}

/// `DsfbGuided` driver: the governor chooses the next config from the trace.
pub fn run_guided(input: &[u8], limits: Limits) -> Result<StrategyResult> {
    let wall0 = Instant::now();
    let cpu0 = cpu_time_ns();
    let grid = config_grid();
    let mut trace = ResidualTrace {
        source_len: input.len() as u64,
        diagnostics: Vec::new(),
        best: None,
        budget: Budget::GUIDED,
    };
    let mut tried: Vec<SearchConfig> = Vec::new();
    let mut evaluated: u64 = 0;
    let mut best: Option<Winner> = None;
    let mut stopped: Option<Accept> = None;

    loop {
        if trace.diagnostics.len() as u32 >= trace.budget.max_configs {
            stopped.get_or_insert(Accept::BestSoFar);
            break;
        }
        let directive = govern(&trace);
        let chosen = match directive {
            SearchDirective::Stop(accept) => {
                stopped = Some(accept);
                None
            }
            SearchDirective::Next(pref) => resolve_untried(pref, &grid, &tried),
            SearchDirective::Deepen(kind) => grid
                .iter()
                .copied()
                .find(|c| !tried.contains(c) && config_includes(*c, kind)),
            SearchDirective::Widen(delta) => {
                let cur = i64::from(trace.budget.max_configs) + i64::from(delta);
                trace.budget.max_configs = cur.clamp(0, 64) as u32;
                grid.iter().copied().find(|c| !tried.contains(c))
            }
        };
        let Some(cfg) = chosen.filter(|c| !tried.contains(c)) else {
            break;
        };

        let (result, diag) = price_config(input, cfg, limits)?;
        evaluated += u64::from(result.candidates_evaluated);
        tried.push(cfg);
        consider(&mut best, &result);
        let best_priced = best.as_ref().map(|b| Priced {
            family: b.kind,
            bytes_len: b.bytes.len() as u64,
            graph_ops: b.graph_ops,
        });
        trace.best = best_priced;
        trace.diagnostics.push(diag);
    }

    let dominant = trace.diagnostics.iter().map(|d| d.class).max();
    Ok(finish(
        best,
        evaluated,
        tried.len() as u32,
        cpu0,
        wall0,
        stopped,
        dominant,
    ))
}

// ---------------------------------------------------------------------------
// Process metrics (Linux; dependency-free).
// ---------------------------------------------------------------------------

/// Process CPU time (user + system), nanoseconds, read from `/proc/self/stat`.
///
/// Assumes the standard Linux `USER_HZ = 100` (one tick = 10 ms). Returns 0 when
/// unavailable (non-Linux), which the court records honestly as "not measured".
pub fn cpu_time_ns() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(stat) = std::fs::read_to_string("/proc/self/stat")
            && let Some(rest) = stat.rsplit(')').next()
        {
            let fields: Vec<&str> = rest.split_whitespace().collect();
            // After the comm field, index 0 is state (field 3); utime is field 14
            // (index 11) and stime field 15 (index 12).
            if fields.len() > 12 {
                let utime: u64 = fields[11].parse().unwrap_or(0);
                let stime: u64 = fields[12].parse().unwrap_or(0);
                return utime.saturating_add(stime).saturating_mul(10_000_000);
            }
        }
    }
    0
}

/// Process peak resident set size, bytes, read from `/proc/self/status` `VmHWM`.
pub fn peak_rss_bytes() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if let Some(value) = line.strip_prefix("VmHWM:") {
                    let kb = value
                        .trim()
                        .trim_end_matches("kB")
                        .trim()
                        .parse::<u64>()
                        .unwrap_or(0);
                    return kb.saturating_mul(1024);
                }
            }
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_matches_propose_all() {
        // The fixed heuristic must be exactly today's candidate portfolio.
        for (name, bytes) in crate::adapter::pdf::samples::sample_pdfs() {
            let limits = Limits::DEFAULT;
            let auto = crate::encode::candidates::propose_all(&bytes, limits).unwrap();
            let configured = propose_configured(&bytes, SearchConfig::DEFAULT, limits).unwrap();
            assert_eq!(auto.len(), configured.len(), "{name} candidate count");
            for (a, c) in auto.iter().zip(configured.iter()) {
                assert_eq!(a.kind, c.kind, "{name} kind order");
                assert_eq!(
                    a.descriptor.serialize().unwrap().0,
                    c.descriptor.serialize().unwrap().0,
                    "{name} {} bytes",
                    a.kind.name()
                );
            }
        }
    }

    #[test]
    fn every_config_is_wire_legal() {
        // Each grid point's candidates serialize, parse, materialize, and equal.
        for (name, bytes) in crate::adapter::pdf::samples::sample_pdfs() {
            for cfg in config_grid() {
                let cands = propose_configured(&bytes, cfg, Limits::DEFAULT).unwrap();
                for c in cands {
                    let (encoded, _) = c.descriptor.serialize().unwrap();
                    let parsed = Descriptor::parse(&encoded, Limits::DEFAULT).unwrap();
                    let out = crate::materialize::materialize(&parsed, Limits::DEFAULT).unwrap();
                    assert_eq!(out, bytes, "{name} {:?} must materialize exactly", cfg);
                }
            }
        }
    }

    #[test]
    fn govern_is_deterministic_and_integer_only() {
        let trace = ResidualTrace {
            source_len: 1000,
            diagnostics: vec![],
            best: None,
            budget: Budget::GUIDED,
        };
        assert_eq!(govern(&trace), govern(&trace));
        assert_eq!(govern(&trace), SearchDirective::Next(SearchConfig::DEFAULT));
    }
}
