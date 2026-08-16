//! The calibration battery — capability by measurement (specification 8).
//!
//! A model's capability is never taken from its name or its own claims. It is
//! measured by running real Creature Context work — here, proposing summaries
//! whose expected content is known — and scoring what comes back. Empty, invalid
//! or unverifiable output scores near zero; there is no artificial baseline. The
//! resulting `CapabilityProfile` is what the router selects on.

use crate::partner::{ModelPartner, WorkItem};
use creature_context_types::{
    AtlasEntity, EntityId, EntityKind, ScopeScale, SnapshotId,
    model::{CandidatePayload, CandidateRecord, CapabilityProfile, CapabilityState, ModelRole},
};
use std::collections::{BTreeMap, BTreeSet};

/// One unit of the battery: a piece of work with a checkable expected answer.
pub struct CalibrationTask {
    pub id: String,
    pub role: ModelRole,
    pub entity: AtlasEntity,
    pub expectation: Expectation,
}

/// How a task's output is scored. Each variant is a deterministic check against
/// the proposed candidates — no model grades another model.
pub enum Expectation {
    /// A contextual task: some proposed summary must contain this token.
    SummaryContains(String),
    /// A structural task: the partner must propose an inferred edge.
    ProposesInferredEdge,
}

/// A small default battery for the contextual role: summarise a few code symbols
/// whose names imply an obvious word the summary should contain. Enough to
/// measure whether a partner performs the contextual task at all; a richer,
/// per-language battery is a later refinement.
pub fn contextual_battery() -> Vec<CalibrationTask> {
    [
        ("read_file", EntityKind::Function, "file"),
        ("send_email", EntityKind::Function, "email"),
        ("calculate_total", EntityKind::Function, "total"),
        ("UserAccount", EntityKind::Type, "account"),
    ]
    .into_iter()
    .map(|(name, kind, token)| CalibrationTask {
        id: format!("summarise-{name}"),
        role: ModelRole::Contextual,
        entity: code_symbol(name, kind),
        expectation: Expectation::SummaryContains(token.into()),
    })
    .collect()
}

/// A minimal code-symbol entity for a calibration task.
fn code_symbol(name: &str, kind: EntityKind) -> AtlasEntity {
    AtlasEntity {
        id: EntityId::new(),
        scale: ScopeScale::Moon,
        kind,
        canonical_name: name.into(),
        aliases: vec![],
        relative_path: Some(format!("src/{name}.rs")),
        parent_id: None,
        purpose_clauses: vec![],
        protected_decision_ids: vec![],
        responsibilities: vec![],
        interfaces: vec![],
        capabilities: vec![],
        sockets: vec![],
        source_spans: vec![],
        structural_fingerprint: String::new(),
        local_evidence: vec![],
        inherited_evidence: vec![],
        green: None,
        open_conflict_ids: vec![],
        deterministic_summary: String::new(),
        inferred_summaries: vec![],
        uncertainty: vec![],
        snapshot_id: SnapshotId("calibration".into()),
        observed_at: String::new(),
        fresh_until: None,
    }
}

impl Expectation {
    fn satisfied_by(&self, outputs: &[CandidateRecord]) -> bool {
        outputs
            .iter()
            .any(|candidate| match (self, &candidate.payload) {
                (
                    Expectation::SummaryContains(token),
                    CandidatePayload::Summary { summary, .. },
                ) => summary.value.to_lowercase().contains(&token.to_lowercase()),
                (Expectation::ProposesInferredEdge, CandidatePayload::Edge(edge)) => {
                    edge.plane == creature_context_types::RelationshipPlane::Inferred
                }
                _ => false,
            })
    }
}

/// Whether a candidate is well-formed enough to count as structured output: a
/// confidence in range, and an edge on the inferred plane (a model claiming an
/// observed fact is malformed, not merely wrong).
fn is_structured(candidate: &CandidateRecord) -> bool {
    match &candidate.payload {
        CandidatePayload::Summary { summary, .. } => (0.0..=1.0).contains(&summary.confidence),
        CandidatePayload::Context(record) => (0.0..=1.0).contains(&record.confidence),
        CandidatePayload::Edge(edge) => {
            edge.plane == creature_context_types::RelationshipPlane::Inferred
        }
    }
}

/// Whether a candidate cites its sources — the attribution the semantic lane
/// requires so a proposal can be traced back to what it reasoned over.
fn is_attributed(candidate: &CandidateRecord) -> bool {
    match &candidate.payload {
        CandidatePayload::Summary { summary, .. } => !summary.source_record_ids.is_empty(),
        CandidatePayload::Context(record) => !record.source_id.is_empty(),
        CandidatePayload::Edge(edge) => !edge.proof_record_ids.is_empty(),
    }
}

/// Run the whole battery against `partner` and return its measured profile. The
/// state is `Verified`: calibration *is* the verification. A partner that
/// produces nothing scores zero on every axis — measured, not assumed.
pub fn calibrate(
    partner: &dyn ModelPartner,
    battery: &[CalibrationTask],
    calibrated_at: &str,
) -> CapabilityProfile {
    // (correct, total) per role, plus overall structured/attributed counts.
    let mut per_role: BTreeMap<ModelRole, (u32, u32)> = BTreeMap::new();
    let mut structured = 0u32;
    let mut attributed = 0u32;
    let mut tested_languages = BTreeSet::new();

    for task in battery {
        let work = WorkItem {
            entity: &task.entity,
            snapshot_id: task.entity.snapshot_id.clone(),
        };
        let outputs = partner.propose(&work);

        let counts = per_role.entry(task.role.clone()).or_insert((0, 0));
        counts.1 += 1;
        if task.expectation.satisfied_by(&outputs) {
            counts.0 += 1;
        }
        if outputs.iter().any(is_structured) {
            structured += 1;
        }
        if outputs.iter().any(is_attributed) {
            attributed += 1;
        }
        if let Some(path) = &task.entity.relative_path
            && let Some(ext) = std::path::Path::new(path)
                .extension()
                .and_then(|e| e.to_str())
        {
            tested_languages.insert(ext.to_string());
        }
    }

    let total = battery.len().max(1) as f32;
    let role_scores = per_role
        .into_iter()
        .map(|(role, (correct, count))| (role, correct as f32 / count.max(1) as f32))
        .collect();

    // The partner's own declared class is the ceiling on what data it may see;
    // calibration carries it through, it never invents a more permissive one.
    let privacy_class = partner.capability().privacy_class.clone();

    CapabilityProfile {
        id: partner.capability().id.clone(),
        provider_id: partner.capability().provider_id.clone(),
        model_id: partner.capability().model_id.clone(),
        state: CapabilityState::Verified,
        privacy_class,
        role_scores,
        structured_output_rate: structured as f32 / total,
        attribution_rate: attributed as f32 / total,
        p95_latency_ms: partner.capability().p95_latency_ms,
        measured_input_limit: partner.capability().measured_input_limit,
        measured_output_limit: partner.capability().measured_output_limit,
        memory_mib: partner.capability().memory_mib,
        storage_mib: partner.capability().storage_mib,
        tested_languages,
        calibration_version: "1".into(),
        calibrated_at: calibrated_at.into(),
        evidence_locator: None,
    }
}
