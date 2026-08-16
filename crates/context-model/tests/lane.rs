//! Milestone 5 Task 3: the continuous semantic lane.
//!
//! The lane takes snapshot-pinned work, asks a partner to propose enrichment,
//! runs every proposal through admission, and applies only what is admitted —
//! adding inferred summaries and inferred edges. It never touches deterministic
//! state: Green, observed edges, evidence and the deterministic summary are
//! left exactly as the deterministic lane produced them (spec §7.2). With no
//! model present the lane is simply idle and changes nothing.

use creature_context_model::lane::run_semantic_lane;
use creature_context_model::partner::{ModelPartner, WorkItem};
use creature_context_model::rules::RulesOnlyPartner;
use creature_context_types::{
    AtlasEntity, AtlasSnapshot, CandidateId, EntityId, EntityKind, RecordId, ScopeScale,
    SnapshotId,
    model::{
        CandidatePayload, CandidateRecord, CandidateState, CapabilityProfile, InferredSummary,
    },
};

const SNAP: &str = "snap-lane";

fn entity(name: &str) -> AtlasEntity {
    AtlasEntity {
        id: EntityId::new(),
        scale: ScopeScale::Moon,
        kind: EntityKind::Function,
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
        structural_fingerprint: "function".into(),
        local_evidence: vec![],
        inherited_evidence: vec![],
        green: None,
        open_conflict_ids: vec![],
        deterministic_summary: "deterministic: a function".into(),
        inferred_summaries: vec![],
        uncertainty: vec![],
        snapshot_id: SnapshotId(SNAP.into()),
        observed_at: "2026-08-09T00:00:00Z".into(),
        fresh_until: None,
    }
}

fn snapshot(entities: Vec<AtlasEntity>) -> AtlasSnapshot {
    AtlasSnapshot {
        id: SnapshotId(SNAP.into()),
        timestamp: "2026-08-09T00:00:00Z".into(),
        entities,
        edges: vec![],
        records: vec![],
        conflicts: vec![],
        sources: vec![],
    }
}

/// Proposes one valid inferred summary per entity — a stand-in for a competent
/// contextual model.
struct SummarisingPartner {
    capability: CapabilityProfile,
}

impl ModelPartner for SummarisingPartner {
    fn capability(&self) -> &CapabilityProfile {
        &self.capability
    }
    fn propose(&self, work: &WorkItem) -> Vec<CandidateRecord> {
        vec![CandidateRecord {
            id: CandidateId::new(),
            payload: CandidatePayload::Summary {
                entity_id: work.entity.id,
                summary: InferredSummary {
                    value: format!(
                        "inferred: {} assembles a widget",
                        work.entity.canonical_name
                    ),
                    producer: "summariser".into(),
                    model_id: "summariser-1".into(),
                    confidence: 0.85,
                    source_record_ids: vec![RecordId::new()],
                    snapshot_id: work.snapshot_id.clone(),
                },
            },
            provider_id: "summariser".into(),
            model_id: "summariser-1".into(),
            capability_profile_id: "summariser".into(),
            schema_version: 1,
            state: CandidateState::Pending,
            rejection_reasons: vec![],
            created_at: "2026-08-09T00:00:00Z".into(),
            snapshot_id: work.snapshot_id.clone(),
        }]
    }
}

#[test]
fn with_no_model_the_lane_is_idle_and_changes_nothing() {
    let mut snap = snapshot(vec![entity("build")]);
    let before = snap.clone();
    let ids: Vec<EntityId> = snap.entities.iter().map(|e| e.id).collect();

    let report = run_semantic_lane(&mut snap, &ids, &RulesOnlyPartner::new(), &[]);
    assert_eq!(report.admitted, 0, "rules-only proposes nothing");
    assert_eq!(
        snap, before,
        "the snapshot is untouched — the deterministic lane stands alone"
    );
}

#[test]
fn an_admitted_summary_enriches_without_touching_deterministic_state() {
    let mut snap = snapshot(vec![entity("build")]);
    let id = snap.entities[0].id;
    let deterministic_before = snap.entities[0].deterministic_summary.clone();
    let green_before = snap.entities[0].green.clone();

    let partner = SummarisingPartner {
        capability: RulesOnlyPartner::new().capability().clone(),
    };
    let report = run_semantic_lane(&mut snap, &[id], &partner, &[]);

    assert_eq!(report.admitted, 1);
    let e = &snap.entities[0];
    assert_eq!(
        e.inferred_summaries.len(),
        1,
        "the admitted summary is added as an inferred enrichment"
    );
    assert!(e.inferred_summaries[0].value.contains("assembles a widget"));
    // The load-bearing property: enrichment did not overwrite deterministic state.
    assert_eq!(
        e.deterministic_summary, deterministic_before,
        "the deterministic summary is untouched"
    );
    assert_eq!(e.green, green_before, "Green is untouched by the model");
}

#[test]
fn a_proposal_over_a_protected_entity_is_held_for_review_not_applied() {
    let mut snap = snapshot(vec![entity("build")]);
    let id = snap.entities[0].id;
    let partner = SummarisingPartner {
        capability: RulesOnlyPartner::new().capability().clone(),
    };
    // The entity is protected: a model may propose a change, not make it.
    let report = run_semantic_lane(&mut snap, &[id], &partner, &[id]);

    assert_eq!(report.review, 1, "a protected target is queued for a human");
    assert_eq!(report.admitted, 0);
    assert!(
        snap.entities[0].inferred_summaries.is_empty(),
        "nothing was applied to a protected entity"
    );
}
