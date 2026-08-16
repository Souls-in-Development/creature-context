//! Milestone 3 Task 4: candidate admission (specification 7.3).
//!
//! Replaces the vacuous test deleted in Milestone 1, which asserted five
//! `let x = true; assert!(x)` lines. This drives the real pipeline: model
//! output becomes an Atlas fact only as a validated candidate — admitted as
//! inferred, queued for review, or rejected with a reason — and never a path to
//! deterministic Green.

use creature_context_core::context::{AdmissionOutcome, admit};
use creature_context_types::{
    AtlasEdge, EdgeId, EntityId, RelationshipKind, RelationshipPlane, SnapshotId,
    model::{CandidatePayload, CandidateRecord, CandidateState, InferredSummary},
};

const ACTIVE: &str = "snap-active";

fn candidate(payload: CandidatePayload, snapshot: &str) -> CandidateRecord {
    CandidateRecord {
        id: creature_context_types::CandidateId::new(),
        payload,
        provider_id: "local".into(),
        model_id: "test-model".into(),
        capability_profile_id: "profile-1".into(),
        schema_version: 1,
        state: CandidateState::Pending,
        rejection_reasons: vec![],
        created_at: "2026-08-06T00:00:00Z".into(),
        snapshot_id: SnapshotId(snapshot.into()),
    }
}

fn summary_for(entity: EntityId, confidence: f32) -> CandidatePayload {
    CandidatePayload::Summary {
        entity_id: entity,
        summary: InferredSummary {
            value: "does X".into(),
            producer: "local".into(),
            model_id: "test-model".into(),
            confidence,
            source_record_ids: vec![],
            snapshot_id: SnapshotId(ACTIVE.into()),
        },
    }
}

fn edge_on(plane: RelationshipPlane) -> CandidatePayload {
    CandidatePayload::Edge(AtlasEdge {
        id: EdgeId::new(),
        source_entity_id: EntityId::new(),
        target_entity_id: EntityId::new(),
        kind: RelationshipKind::Calls,
        plane,
        proof_record_ids: vec![],
        evidence: vec![],
        source_id: "test-model".into(),
        confidence: 0.9,
        observed_at: "2026-08-06T00:00:00Z".into(),
        fresh_until: None,
        required: true,
        snapshot_id: SnapshotId(ACTIVE.into()),
    })
}

fn active() -> SnapshotId {
    SnapshotId(ACTIVE.into())
}

#[test]
fn a_valid_inferred_summary_is_admitted() {
    let c = candidate(summary_for(EntityId::new(), 0.8), ACTIVE);
    match admit(c, &active(), &[]) {
        AdmissionOutcome::Admitted(record) => {
            assert_eq!(record.state, CandidateState::Admitted);
        }
        other => panic!("expected admission, got {other:?}"),
    }
}

#[test]
fn a_stale_snapshot_candidate_is_rejected() {
    let c = candidate(summary_for(EntityId::new(), 0.8), "snap-old");
    match admit(c, &active(), &[]) {
        AdmissionOutcome::Rejected { reasons, .. } => {
            assert!(
                reasons.iter().any(|r| r.contains("snapshot")),
                "the reason must name the stale snapshot, got {reasons:?}"
            );
        }
        other => panic!("a candidate pinned to a stale snapshot must be rejected, got {other:?}"),
    }
}

#[test]
fn an_inferred_candidate_cannot_set_an_observed_edge() {
    // The load-bearing rule: inference has no path to a deterministic fact.
    let c = candidate(edge_on(RelationshipPlane::Observed), ACTIVE);
    match admit(c, &active(), &[]) {
        AdmissionOutcome::Rejected { reasons, .. } => {
            assert!(
                reasons.iter().any(|r| r.contains("observed")),
                "the reason must explain that an inferred candidate cannot set an observed edge, \
                 got {reasons:?}"
            );
        }
        other => panic!("an observed-plane edge from a model must be rejected, got {other:?}"),
    }
}

#[test]
fn an_inferred_edge_is_admitted() {
    let c = candidate(edge_on(RelationshipPlane::Inferred), ACTIVE);
    assert!(
        matches!(admit(c, &active(), &[]), AdmissionOutcome::Admitted(_)),
        "an inferred-plane edge is a legitimate candidate"
    );
}

#[test]
fn a_candidate_touching_a_protected_decision_is_queued_for_review() {
    let protected = EntityId::new();
    let c = candidate(summary_for(protected, 0.8), ACTIVE);
    match admit(c, &active(), &[protected]) {
        AdmissionOutcome::Review(record) => {
            assert_eq!(record.state, CandidateState::Review);
        }
        other => {
            panic!("a candidate over a protected decision must queue, not admit, got {other:?}")
        }
    }
}

#[test]
fn an_out_of_range_confidence_is_rejected() {
    let c = candidate(summary_for(EntityId::new(), 1.5), ACTIVE);
    assert!(
        matches!(admit(c, &active(), &[]), AdmissionOutcome::Rejected { .. }),
        "a confidence outside 0..=1 is schema-invalid and must be rejected, not clamped"
    );
}
