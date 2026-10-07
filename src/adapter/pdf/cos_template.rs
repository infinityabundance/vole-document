//! PDF COS grammar/template candidate: a bounded dictionary of repeated
//! **COS-token phrases** as a *size* mechanism.
//!
//! Phase 4 showed that typed *lexical channels* lose; ADR-0011/0012/0013/0036
//! showed that *positional* prediction (xref offsets, `startxref`, `/Length`)
//! loses because every regenerated field still pays per-site framing. This
//! candidate attacks a different structural layer: the recurring COS syntax
//! itself — dictionary stems like `<< /Type /Page /Parent`, object framing like
//! ` >>\nendobj\n`, and `stream`/`endstream` boilerplate that a producer repeats
//! once per object.
//!
//! The mechanism is a small, finite **grammar**: a bounded set of terminal
//! phrases ("templates") discovered at COS-token boundaries, plus a greedy
//! leftmost-longest cover of the document by `EmitObject` (instantiate a
//! template) and `Inline` (its literal parameters) instructions. It reuses only
//! existing DRA ops (`EMIT_OBJECT` `0x01`, `INLINE` `0x02`) — no new opcode, DRA
//! version, universe, or feature bit. The definition cost is real and charged:
//! each template costs one object record, each occurrence costs an `EmitObject`
//! op, and each match splits its surrounding literal into an extra `Inline` op.
//!
//! Nothing is invented: a template fires only where its exact bytes recur, the
//! literal gaps are copied verbatim, and the finished program is round-tripped
//! through the normative decoder (serialize → parse → materialize →
//! byte-compare) before the candidate is returned. When the round trip is not
//! exact the candidate declines rather than emitting an inexact lane.
//!
//! The mechanism is bounded and deterministic: token count, discovery windows,
//! candidate count, template count, and op count are all capped, and every
//! selection is a total order over `(score, byte_len, first_token, token_count)`
//! rather than any hash-map iteration order. Whether the templates *pay* is not
//! assumed — it is decided by the complete-cost court. On the measured corpus
//! this candidate loses (see
//! [`docs/adr/0037-pdf-grammar-templates.md`](../../../docs/adr/0037-pdf-grammar-templates.md)).

use std::collections::HashMap;

use crate::SOURCE_FORMAT_PDF;
use crate::container::{Descriptor, ObjectSource, UNIVERSE};
use crate::dra::{Op, Program};
use crate::encode::candidates::{Candidate, CandidateKind};
use crate::error::Result;
use crate::integrity::sha256;
use crate::limits::Limits;

use super::lexer::lex;
use super::span::SpanKind;

/// Longest template, in COS tokens (`n`-gram width).
const MAX_TEMPLATE_TOKENS: usize = 16;
/// Largest tokenised document accepted for discovery; larger inputs decline.
const MAX_DISCOVERY_TOKENS: usize = 200_000;
/// Cap on the number of `(index, width)` discovery windows materialised.
const MAX_WINDOWS: usize = 8_000_000;
/// Cap on distinct candidate phrases held during discovery.
const MAX_CANDIDATES: usize = 500_000;
/// Largest number of templates selected for the cover.
const MAX_TEMPLATES: usize = 512;
/// Shortest template, in bytes; below this the per-occurrence framing cannot pay.
const MIN_TEMPLATE_BYTES: u64 = 4;

// Framing constants used **only to rank** candidate templates; the actual size
// is decided by the complete-cost court. `EMIT_OBJECT_OP_COST` is the encoded
// size of `Op::EmitObject` (opcode + `u32` id); `INLINE_SPLIT_COST` is the extra
// `Op::Inline` framing a match forces on its surrounding literal; and
// `OBJECT_RECORD_COST` is `container::record::RECORD_OVERHEAD` (8-byte header +
// 4-byte CRC32C) paid once per template object record.
const EMIT_OBJECT_OP_COST: i64 = 5;
const INLINE_SPLIT_COST: i64 = 5;
const OBJECT_RECORD_COST: i64 = 12;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// A built COS-template plan: the reconstruction program, the template objects
/// it references, and the counters used to describe the plan.
pub struct CosTemplatePlan {
    /// Ordered reconstruction instructions (`Inline` literals / `EmitObject`
    /// template instantiations).
    pub ops: Vec<Op>,
    /// One inline object per selected template, in object-id order.
    pub objects: Vec<Vec<u8>>,
    /// Number of templates actually instantiated at least once.
    pub templates: usize,
    /// Total COS tokens in the source.
    pub tokens: usize,
    /// Number of template instantiations emitted.
    pub matched_phrases: usize,
    /// Source bytes covered by a template instantiation.
    pub matched_bytes: u64,
    /// Source bytes carried as literal parameters.
    pub literal_bytes: u64,
}

/// One COS token: its byte range, class, and precomputed hash.
struct Token {
    start: usize,
    len: usize,
    kind: SpanKind,
    hash: u64,
}

/// A discovered candidate phrase (`(first_token, token_count)` identifies it).
struct GramInfo {
    count: u32,
    token_count: usize,
    first_token: usize,
}

/// A selected template ready for the cover.
struct Template {
    first_token: usize,
    token_count: usize,
    bytes: Vec<u8>,
}

/// Build the COS-template plan for `input`, or `None` when the input is not a
/// tokenisable PDF this mechanism can express.
///
/// Declines (`Ok(None)`) when the lexical cover fails, the input is empty or too
/// large to tokenise, no structural phrase recurs, no template is ever
/// instantiated, or the program would exceed the graph-op bound. The caller owns
/// the final byte-exactness check through the normative decoder.
pub fn build_cos_template_plan(input: &[u8], limits: Limits) -> Result<Option<CosTemplatePlan>> {
    if input.is_empty() {
        return Ok(None);
    }
    let lexed = match lex(input, limits) {
        Ok(l) => l,
        Err(_) => return Ok(None),
    };
    let spans = lexed.spans.spans;
    if spans.is_empty() || spans.len() > MAX_DISCOVERY_TOKENS {
        return Ok(None);
    }

    let tokens: Vec<Token> = spans
        .iter()
        .filter_map(|s| {
            let start = usize::try_from(s.start).ok()?;
            let len = usize::try_from(s.len).ok()?;
            let end = start.checked_add(len)?;
            let bytes = input.get(start..end)?;
            Some(Token {
                start,
                len,
                kind: s.kind,
                hash: fnv1a(bytes),
            })
        })
        .collect();
    if tokens.len() != spans.len() {
        return Ok(None);
    }

    // Prefix sums of token byte lengths and of structural-token counts, so a
    // candidate's byte width and structural-ness are O(1) lookups.
    let mut byte_prefix = Vec::with_capacity(tokens.len() + 1);
    let mut structural_prefix = Vec::with_capacity(tokens.len() + 1);
    byte_prefix.push(0u64);
    structural_prefix.push(0u32);
    for t in &tokens {
        let prev = *byte_prefix.last().expect("non-empty");
        byte_prefix.push(prev + t.len as u64);
        let prev_s = *structural_prefix.last().expect("non-empty");
        let bytes = &input[t.start..t.start + t.len];
        structural_prefix.push(prev_s + u32::from(is_structural(t.kind, bytes)));
    }

    // --- discovery: repeated token n-grams. ----------------------------------
    let mut grams: HashMap<u64, GramInfo> = HashMap::new();
    let mut windows: usize = 0;
    'outer: for i in 0..tokens.len() {
        let mut h = FNV_OFFSET;
        for n in 1..=MAX_TEMPLATE_TOKENS {
            let j = i + n - 1;
            if j >= tokens.len() {
                break;
            }
            h = h.wrapping_mul(FNV_PRIME).wrapping_add(tokens[j].hash);
            // Mix the width into the key so grams of different lengths do not
            // conflate (collisions are harmless anyway: the cover verifies
            // bytes, and an unused template is dropped).
            let key = h ^ (n as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
            windows += 1;
            if windows > MAX_WINDOWS {
                break 'outer;
            }
            match grams.get_mut(&key) {
                Some(info) => info.count += 1,
                None => {
                    if grams.len() >= MAX_CANDIDATES {
                        break 'outer;
                    }
                    grams.insert(
                        key,
                        GramInfo {
                            count: 1,
                            token_count: n,
                            first_token: i,
                        },
                    );
                }
            }
        }
    }

    // --- selection: deterministic total order, bounded to MAX_TEMPLATES. -----
    let mut selected: Vec<(i64, u64, usize, usize)> = Vec::new();
    for info in grams.values() {
        if info.count < 2 {
            continue;
        }
        let first = info.first_token;
        let count = info.token_count;
        let byte_len = byte_prefix[first + count] - byte_prefix[first];
        if byte_len < MIN_TEMPLATE_BYTES {
            continue;
        }
        if structural_prefix[first + count] == structural_prefix[first] {
            continue; // not a COS-structural phrase
        }
        let occurrences = i64::from(info.count);
        let len = byte_len as i64;
        let score = (occurrences - 1) * len
            - occurrences * (EMIT_OBJECT_OP_COST + INLINE_SPLIT_COST)
            - (len + OBJECT_RECORD_COST);
        if score <= 0 {
            continue;
        }
        selected.push((score, byte_len, first, count));
    }
    // score desc, byte_len desc, first_token asc, token_count asc.
    selected.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(b.1.cmp(&a.1))
            .then(a.2.cmp(&b.2))
            .then(a.3.cmp(&b.3))
    });
    selected.truncate(MAX_TEMPLATES);
    if selected.is_empty() {
        return Ok(None);
    }

    let mut templates: Vec<Template> = Vec::with_capacity(selected.len());
    for &(_, byte_len, first, count) in &selected {
        let start = tokens[first].start;
        let end = start + byte_len as usize;
        templates.push(Template {
            first_token: first,
            token_count: count,
            bytes: input[start..end].to_vec(),
        });
    }

    // Index templates by their first token's bytes so a cover position only
    // tests templates that can possibly start there.
    let mut by_head: HashMap<Vec<u8>, Vec<usize>> = HashMap::new();
    for (idx, t) in templates.iter().enumerate() {
        let head = &input
            [tokens[t.first_token].start..tokens[t.first_token].start + tokens[t.first_token].len];
        by_head.entry(head.to_vec()).or_default().push(idx);
    }

    // --- cover: greedy leftmost-longest over tokens. -------------------------
    let mut ops: Vec<Op> = Vec::new();
    let mut used = vec![0usize; templates.len()];
    let mut matched_phrases = 0usize;
    let mut matched_bytes = 0u64;
    let mut literal_start = 0usize; // byte offset of the pending literal run
    let mut i = 0usize;
    while i < tokens.len() {
        let head = &input[tokens[i].start..tokens[i].start + tokens[i].len];
        let mut best: Option<usize> = None;
        if let Some(bucket) = by_head.get(head) {
            for &idx in bucket {
                let t = &templates[idx];
                if t.token_count <= tokens.len() - i
                    && (best.is_none() || t.token_count > templates[best.expect("set")].token_count)
                    && template_matches(&templates[idx], i, &tokens, input)
                {
                    best = Some(idx);
                }
            }
        }
        match best {
            Some(idx) => {
                let t = &templates[idx];
                let end = tokens[i].start;
                if end > literal_start {
                    ops.push(Op::Inline {
                        bytes: input[literal_start..end].to_vec(),
                    });
                }
                ops.push(Op::EmitObject {
                    object_id: idx as u32,
                });
                used[idx] += 1;
                let last = i + t.token_count - 1;
                literal_start = tokens[last].start + tokens[last].len;
                matched_phrases += 1;
                matched_bytes += byte_prefix[last + 1] - byte_prefix[i];
                i += t.token_count;
            }
            None => i += 1,
        }
    }
    if literal_start < input.len() {
        ops.push(Op::Inline {
            bytes: input[literal_start..].to_vec(),
        });
    }
    if matched_phrases == 0 {
        return Ok(None);
    }
    if ops.len() as u64 > limits.max_graph_ops as u64 {
        return Ok(None);
    }

    // Renumber to only the templates actually instantiated, preserving order.
    let mut remap = vec![u32::MAX; templates.len()];
    let mut objects: Vec<Vec<u8>> = Vec::new();
    for (idx, t) in templates.iter_mut().enumerate() {
        if used[idx] > 0 {
            remap[idx] = objects.len() as u32;
            objects.push(std::mem::take(&mut t.bytes));
        }
    }
    for op in &mut ops {
        if let Op::EmitObject { object_id } = op {
            *object_id = remap[*object_id as usize];
        }
    }
    let templates_used = objects.len();

    Ok(Some(CosTemplatePlan {
        ops,
        objects,
        templates: templates_used,
        tokens: tokens.len(),
        matched_phrases,
        matched_bytes,
        literal_bytes: input.len() as u64 - matched_bytes,
    }))
}

/// Propose a COS-template candidate, or `None` if none is usable or it is not
/// byte-exact.
///
/// Wraps [`build_cos_template_plan`] into a `EMIT_OBJECT` / `INLINE` program and
/// verifies the finished candidate end to end through the normative decoder
/// (serialize → parse → materialize → byte-compare); an inexact program declines
/// rather than being emitted.
pub fn propose_pdf_cos_template(input: &[u8], limits: Limits) -> Result<Option<Candidate>> {
    let plan = match build_cos_template_plan(input, limits)? {
        Some(p) => p,
        None => return Ok(None),
    };

    let format_basis = format!(
        "pdf-cos-template;templates={};tokens={};matched_phrases={};matched_bytes={};\
         literal_bytes={}",
        plan.templates, plan.tokens, plan.matched_phrases, plan.matched_bytes, plan.literal_bytes,
    );

    let descriptor = Descriptor {
        universe: UNIVERSE.to_string(),
        source_format: SOURCE_FORMAT_PDF,
        format_basis,
        models: vec![],
        channels: vec![],
        objects: plan.objects.into_iter().map(ObjectSource::Inline).collect(),
        program: Program::new(plan.ops),
        observation_index: None,
        seek_directory: false,
        checkpoints: None,
        source_sha256: sha256(input),
        source_len: input.len() as u64,
    };

    let candidate = Candidate {
        kind: CandidateKind::PdfCosTemplate,
        descriptor,
    };

    // Verify byte-exactness through the normative decoder before returning. An
    // inexact program must never be emitted.
    let (encoded, _) = candidate.descriptor.serialize()?;
    let parsed = match Descriptor::parse(&encoded, limits) {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };
    let out = match crate::materialize::materialize(&parsed, limits) {
        Ok(o) => o,
        Err(_) => return Ok(None),
    };
    if out != input {
        return Ok(None);
    }

    Ok(Some(candidate))
}

/// Whether a token is a *structural* COS token — a name, dictionary/array
/// delimiter, or an alphabetic keyword (`obj`, `endobj`, `stream`, …). This is
/// what makes a template a phrase of the COS **grammar** rather than an
/// arbitrary byte repeat.
fn is_structural(kind: SpanKind, bytes: &[u8]) -> bool {
    match kind {
        SpanKind::Name
        | SpanKind::DictOpen
        | SpanKind::DictClose
        | SpanKind::ArrayOpen
        | SpanKind::ArrayClose => true,
        SpanKind::Regular => !bytes.is_empty() && bytes.iter().all(|b| b.is_ascii_alphabetic()),
        _ => false,
    }
}

/// Whether template `t` matches the token stream starting at token `i`.
fn template_matches(t: &Template, i: usize, tokens: &[Token], input: &[u8]) -> bool {
    if t.token_count > tokens.len() - i {
        return false;
    }
    for k in 0..t.token_count {
        let a = &tokens[t.first_token + k];
        let b = &tokens[i + k];
        if a.len != b.len || input[a.start..a.start + a.len] != input[b.start..b.start + b.len] {
            return false;
        }
    }
    true
}

/// FNV-1a 64-bit hash over `bytes`.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h = FNV_OFFSET;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::pdf::samples::{is_negative_control, sample_pdfs};

    /// A PDF whose objects repeat the same dictionary stem and framing, so the
    /// grammar has genuine phrase templates to discover.
    fn repetitive_pdf() -> Vec<u8> {
        let mut s = String::from("%PDF-1.7\n");
        for n in 1..=40u64 {
            s.push_str(&format!(
                "{n} 0 obj\n<< /Type /Pagemark /Index {n} /Child {} 0 R >>\nendobj\n",
                n % 7 + 1
            ));
        }
        s.push_str("xref\n0 41\n0000000000 65535 f \n");
        s.push_str("trailer\n<< /Size 41 /Root 1 0 R >>\nstartxref\n0\n%%EOF\n");
        s.into_bytes()
    }

    fn assert_exact(cand: &Candidate, input: &[u8]) {
        let (encoded, _) = cand.descriptor.serialize().unwrap();
        let parsed = Descriptor::parse(&encoded, Limits::DEFAULT).unwrap();
        let out = crate::materialize::materialize(&parsed, Limits::DEFAULT).unwrap();
        assert_eq!(out, input, "COS-template candidate must be byte-exact");
        assert_eq!(sha256(&out), sha256(input));
    }

    #[test]
    fn exact_on_repetitive_pdf() {
        let pdf = repetitive_pdf();
        let cand = propose_pdf_cos_template(&pdf, Limits::DEFAULT)
            .unwrap()
            .expect("repetitive PDF must propose a COS template");
        assert_eq!(cand.kind, CandidateKind::PdfCosTemplate);
        assert!(
            !cand.descriptor.objects.is_empty(),
            "at least one template object must be carried"
        );
        assert_exact(&cand, &pdf);
    }

    #[test]
    fn exact_on_sample_corpus() {
        let mut checked = 0usize;
        for (name, bytes) in sample_pdfs() {
            if is_negative_control(name) {
                continue;
            }
            if let Some(cand) = propose_pdf_cos_template(&bytes, Limits::DEFAULT).unwrap() {
                assert_eq!(cand.kind, CandidateKind::PdfCosTemplate);
                assert_exact(&cand, &bytes);
                checked += 1;
            }
        }
        // Most small samples carry no phrase that recurs enough to amortize the
        // per-occurrence framing; the ones that do (`many.pdf`, `flate.pdf`) must
        // be byte-exact. The number is asserted only to keep the test meaningful.
        assert!(checked >= 2, "must exercise at least two samples");
    }

    #[test]
    fn declines_on_non_pdf_and_empty() {
        assert!(
            propose_pdf_cos_template(b"", Limits::DEFAULT)
                .unwrap()
                .is_none()
        );
        assert!(
            propose_pdf_cos_template(b"a short string with no cos structure", Limits::DEFAULT)
                .unwrap()
                .is_none()
        );
        for (name, bytes) in sample_pdfs() {
            if is_negative_control(name) {
                // `notpdf.bin` and the truncated `malformed.pdf` carry no
                // recurring COS phrase.
                assert!(
                    propose_pdf_cos_template(&bytes, Limits::DEFAULT)
                        .unwrap()
                        .is_none(),
                    "{name} must decline"
                );
            }
        }
    }

    #[test]
    fn deterministic() {
        let pdf = repetitive_pdf();
        let a = propose_pdf_cos_template(&pdf, Limits::DEFAULT)
            .unwrap()
            .unwrap()
            .descriptor
            .serialize()
            .unwrap()
            .0;
        let b = propose_pdf_cos_template(&pdf, Limits::DEFAULT)
            .unwrap()
            .unwrap()
            .descriptor
            .serialize()
            .unwrap()
            .0;
        assert_eq!(a, b, "COS-template bytes must be deterministic");
    }

    #[test]
    fn declines_when_no_template_fits_the_op_bound() {
        let pdf = repetitive_pdf();
        let limits = Limits {
            max_graph_ops: 1,
            ..Limits::DEFAULT
        };
        assert!(
            propose_pdf_cos_template(&pdf, limits).unwrap().is_none(),
            "a multi-op program cannot fit a one-op graph"
        );
    }
}
