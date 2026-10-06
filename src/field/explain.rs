//! EXPLAIN and EXPLAIN ANALYZE for field observations (Phase 11.5, ADR-0026).
//!
//! [`explain`] is a pure function of validated metadata: it never materializes a
//! node and never writes to the store. [`explain_analyze`] executes the
//! observation and returns both the intended plan and the measured actual work,
//! which is the central evidence surface for Phase-11 claims (ADR-0027).
//!
//! The JSON key sets are frozen and exact:
//!
//! * plan   — `selector`, `representation`, `shape`, `index_reads`,
//!   `required_nodes`, `will_materialize`, `will_not_materialize`.
//! * actual — `index_nodes_read`, `seed_nodes_fetched`, `seed_nodes_materialized`,
//!   `descriptor_bytes_read`, `manifest_bytes_read`, `index_bytes_read`,
//!   `seed_bytes_read`, `bytes_read`, `bytes_returned`, `deepened`, `wall_micros`,
//!   `basis`, `exact`.
//!
//! `bytes_read` is the **sum** of the four `*_bytes_read` classes: total physical
//! bytes this observation made the OS fetch, including the descriptor blob. It is
//! deliberately not the seed-store-only number it was before Phase 11.9's
//! review (ADR-0027 accounting).

use crate::error::Result;
use crate::field::observe::{ObserveRequest, ObserveStats, observe_with_field};
use crate::field::plan::{ObservePlan, plan};
use crate::field::provenance::{Basis, json_escape};
use crate::field::{Field, FieldId, FieldStore};
use crate::limits::Limits;

/// The intended plan plus its canonical JSON rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplainPlan {
    /// The structured plan.
    pub plan: ObservePlan,
    /// The plan as a flat JSON object with exactly the documented keys.
    pub json: String,
}

/// The measured cost of an executed observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplainActual {
    /// The observation's stats.
    pub stats: ObserveStats,
    /// The basis of the answer that was produced.
    pub answer_basis: Basis,
    /// Whether the answer was exact.
    pub exact: bool,
}

impl ExplainActual {
    /// The actual work as a flat JSON object with exactly the documented keys.
    pub fn to_json(&self) -> String {
        let s = &self.stats;
        format!(
            concat!(
                "{{",
                "\"index_nodes_read\":{},",
                "\"seed_nodes_fetched\":{},",
                "\"seed_nodes_materialized\":{},",
                "\"descriptor_bytes_read\":{},",
                "\"manifest_bytes_read\":{},",
                "\"index_bytes_read\":{},",
                "\"seed_bytes_read\":{},",
                "\"bytes_read\":{},",
                "\"bytes_returned\":{},",
                "\"deepened\":{},",
                "\"wall_micros\":{},",
                "\"basis\":\"{}\",",
                "\"exact\":{}",
                "}}"
            ),
            s.index_nodes_read,
            s.seed_nodes_fetched,
            s.seed_nodes_materialized,
            s.descriptor_bytes_read,
            s.manifest_bytes_read,
            s.index_bytes_read,
            s.seed_bytes_read,
            s.bytes_read,
            s.bytes_returned,
            s.deepened,
            s.wall_micros,
            self.answer_basis.name(),
            self.exact,
        )
    }
}

/// Explain an observation without executing it. Pure.
pub fn explain(field: &Field, store: &FieldStore, req: &ObserveRequest) -> Result<ExplainPlan> {
    let plan = plan(field, store, req)?;
    let json = plan_json(req, &plan);
    Ok(ExplainPlan { plan, json })
}

/// Execute an observation and report both the intended plan and actual work.
///
/// The field is opened **once** here and reused for both the plan and the
/// evaluation (`observe_with_field`), so a cold `explain --analyze` reads the
/// descriptor blob exactly once (review fix #2). The reported `wall_micros`
/// covers the whole analysis: open + plan + evaluation.
pub fn explain_analyze(
    store: &mut FieldStore,
    id: &FieldId,
    req: &ObserveRequest,
    limits: Limits,
) -> Result<(ExplainPlan, ExplainActual)> {
    let started = std::time::Instant::now();
    let field = Field::open(store, id, limits)?;
    let planned = plan(&field, store, req)?;
    let json = plan_json(req, &planned);
    let (answer, mut stats, _field) = observe_with_field(store, &field, req, limits)?;
    stats.wall_micros = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
    let actual = ExplainActual {
        stats,
        answer_basis: answer.basis,
        exact: answer.exact,
    };
    Ok((
        ExplainPlan {
            plan: planned,
            json,
        },
        actual,
    ))
}

fn plan_json(req: &ObserveRequest, plan: &ObservePlan) -> String {
    format!(
        concat!(
            "{{",
            "\"selector\":\"{}\",",
            "\"representation\":\"{}\",",
            "\"shape\":\"{}\",",
            "\"index_reads\":{},",
            "\"required_nodes\":{},",
            "\"will_materialize\":{},",
            "\"will_not_materialize\":{}",
            "}}"
        ),
        json_escape(&req.selector.canonical()),
        json_escape(req.representation.name()),
        plan.shape.name(),
        plan.index_reads,
        plan.required_nodes,
        string_array(&plan.will_materialize),
        string_array(&plan.will_not_materialize),
    )
}

fn string_array(items: &[String]) -> String {
    let body = items
        .iter()
        .map(|s| format!("\"{}\"", json_escape(s)))
        .collect::<Vec<_>>()
        .join(",");
    format!("[{body}]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_json_has_exactly_the_fourteen_keys() {
        let actual = ExplainActual {
            stats: ObserveStats::default(),
            answer_basis: Basis::DirectlyObserved,
            exact: true,
        };
        assert_eq!(
            actual.to_json(),
            "{\"index_nodes_read\":0,\"seed_nodes_fetched\":0,\"seed_nodes_materialized\":0,\"descriptor_bytes_read\":0,\"manifest_bytes_read\":0,\"index_bytes_read\":0,\"seed_bytes_read\":0,\"bytes_read\":0,\"bytes_returned\":0,\"deepened\":false,\"wall_micros\":0,\"basis\":\"directly-observed\",\"exact\":true}"
        );
    }

    #[test]
    fn string_array_escapes() {
        assert_eq!(
            string_array(&["a".into(), "b\"c".into()]),
            "[\"a\",\"b\\\"c\"]"
        );
        assert_eq!(string_array(&[]), "[]");
    }
}
