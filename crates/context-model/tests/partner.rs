//! Milestone 5 Task 1: the model partner interface and its rules-only fallback.
//!
//! The architecture's load-bearing rule: models never write. Every model output
//! is a `CandidateRecord` that must pass Milestone 3 admission before it touches
//! the Atlas (spec §7.3). A partner *proposes*; the deterministic reconciler
//! decides. These tests hold two properties:
//!
//! - **Rules-only is mandatory** (spec §8): with no model present the partner is
//!   idle — it proposes nothing — and reports its capability honestly as
//!   unavailable, so the deterministic pipeline stands alone.
//! - **Containment**: a partner's proposal only reaches the Atlas through
//!   admission, so a valid inferred summary is admitted as inferred while an
//!   attempt to assert an observed fact is rejected.

use creature_context_core::context::admission::AdmissionOutcome;
use creature_context_model::partner::{ModelPartner, WorkItem, propose_and_admit};
use creature_context_model::rules::RulesOnlyPartner;
use creature_context_types::{
    AtlasEdge, AtlasEntity, CandidateId, EdgeId, EntityId, EntityKind, RecordId, RelationshipKind,
    RelationshipPlane, ScopeScale, SnapshotId,
    model::{CandidatePayload, CandidateRecord, CandidateState, CapabilityState, InferredSummary},
};

const SNAP: &str = "snap-partner";

fn entity() -> AtlasEntity {
    AtlasEntity {
        id: EntityId::new(),
        scale: ScopeScale::Moon,
        kind: EntityKind::Function,
        canonical_name: "build".into(),
        aliases: vec![],
        relative_path: Some("src/widget.rs".into()),
        parent_id: None,
        purpose_clauses: vec![],
        protected_decision_ids: vec![],
        responsibilities: vec![],
        interfaces: vec![],
        capabilities: vec![],
        sockets: vec![],
        source_spans: vec![],
        structural_fingerprint: "function".into(),
        local_evidence: vec![],
        inherited_evidence: vec![],
        green: None,
        open_conflict_ids: vec![],
        deterministic_summary: String::new(),
        inferred_summaries: vec![],
        uncertainty: vec![],
        snapshot_id: SnapshotId(SNAP.into()),
        observed_at: "2026-08-09T00:00:00Z".into(),
        fresh_until: None,
    }
}

fn work(entity: &AtlasEntity) -> WorkItem<'_> {
    WorkItem {
        entity,
        snapshot_id: SnapshotId(SNAP.into()),
    }
}

/// A stand-in for a real model: it proposes exactly the candidate it is handed,
/// so the test controls what flows into admission.
struct ScriptedPartner {
    proposal: CandidateRecord,
    capability: creature_context_types::model::CapabilityProfile,
}

impl ModelPartner for ScriptedPartner {
    fn capability(&self) -> &creature_context_types::model::CapabilityProfile {
        &self.capability
    }
    fn propose(&self, _work: &WorkItem) -> Vec<CandidateRecord> {
        vec![self.proposal.clone()]
    }
}

fn summary_candidate(entity_id: EntityId) -> CandidateRecord {
    CandidateRecord {
        id: CandidateId::new(),
        payload: CandidatePayload::Summary {
            entity_id,
            summary: InferredSummary {
                value: "assembles a Widget".into(),
                producer: "scripted".into(),
                model_id: "scripted-1".into(),
                confidence: 0.8,
                source_record_ids: vec![RecordId::new()],
                snapshot_id: SnapshotId(SNAP.into()),
            },
        },
        provider_id: "scripted".into(),
        model_id: "scripted-1".into(),
        capability_profile_id: "scripted".into(),
        schema_version: 1,
        state: CandidateState::Pending,
        rejection_reasons: vec![],
        created_at: "2026-08-09T00:00:00Z".into(),
        snapshot_id: SnapshotId(SNAP.into()),
    }
}

fn observed_edge_candidate() -> CandidateRecord {
    let edge = AtlasEdge {
        id: EdgeId::new(),
        source_entity_id: EntityId::new(),
        target_entity_id: EntityId::new(),
        kind: RelationshipKind::Calls,
        // The forbidden move: a model asserting a deterministic fact.
        plane: RelationshipPlane::Observed,
        proof_record_ids: vec![],
        evidence: vec![],
        source_id: "scripted".into(),
        confidence: 1.0,
        observed_at: "2026-08-09T00:00:00Z".into(),
        fresh_until: None,
        required: false,
        snapshot_id: SnapshotId(SNAP.into()),
    };
    CandidateRecord {
        id: CandidateId::new(),
        payload: CandidatePayload::Edge(edge),
        provider_id: "scripted".into(),
        model_id: "scripted-1".into(),
        capability_profile_id: "scripted".into(),
        schema_version: 1,
        state: CandidateState::Pending,
        rejection_reasons: vec![],
        created_at: "2026-08-09T00:00:00Z".into(),
        snapshot_id: SnapshotId(SNAP.into()),
    }
}

#[test]
fn the_rules_only_partner_is_idle_and_reports_no_model() {
    let partner = RulesOnlyPartner::new();
    let entity = entity();
    assert!(
        partner.propose(&work(&entity)).is_empty(),
        "no model present → no candidates → the semantic lane is idle"
    );
    assert_eq!(
        partner.capability().state,
        CapabilityState::Unavailable,
        "rules-only is honest that no model capability exists"
    );
    assert!(
        partner
            .capability()
            .role_scores
            .values()
            .all(|&score| score == 0.0),
        "an absent model scores zero on every role — no artificial baseline (spec §8)"
    );
}

#[test]
fn a_valid_inferred_summary_is_admitted_through_the_pipeline() {
    let entity = entity();
    let partner = ScriptedPartner {
        proposal: summary_candidate(entity.id),
        capability: RulesOnlyPartner::new().capability().clone(),
    };
    let active = SnapshotId(SNAP.into());
    let outcomes = propose_and_admit(&partner, &work(&entity), &active, &[]);
    assert_eq!(outcomes.len(), 1);
    assert!(
        matches!(outcomes[0], AdmissionOutcome::Admitted(_)),
        "a well-formed inferred summary is admitted as inferred"
    );
}

#[test]
fn a_partner_cannot_assert_an_observed_fact() {
    let entity = entity();
    let partner = ScriptedPartner {
        proposal: observed_edge_candidate(),
        capability: RulesOnlyPartner::new().capability().clone(),
    };
    let active = SnapshotId(SNAP.into());
    let outcomes = propose_and_admit(&partner, &work(&entity), &active, &[]);
    assert!(
        matches!(outcomes[0], AdmissionOutcome::Rejected { .. }),
        "a model proposing an observed edge is rejected: inference has no path to a \
         deterministic fact"
    );
}
