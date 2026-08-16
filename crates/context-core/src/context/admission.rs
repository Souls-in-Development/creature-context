//! Candidate admission (specification 7.3).
//!
//! Model output enters the Atlas only through here, and only as a validated
//! candidate. The pipeline runs the specification's stages in order and reaches
//! exactly one of three outcomes: admit as an inferred fact, queue for human
//! review, or reject with a reason.
//!
//! The invariant it exists to hold: inference has no path to a deterministic
//! fact. A model may propose an inferred summary or an inferred edge; it may
//! never set an observed edge, satisfy a deterministic Green axis, or overwrite
//! a protected human decision. The thing described and the thing admitted must
//! not diverge.

use creature_context_types::{
    EntityId, RelationshipPlane, SnapshotId,
    model::{CandidatePayload, CandidateRecord, CandidateState},
};

/// The single outcome of admitting one candidate.
#[derive(Debug)]
pub enum AdmissionOutcome {
    /// Validated and admitted as an inferred fact.
    Admitted(Box<CandidateRecord>),
    /// Plausible but touching protected human intent — held for a human.
    Review(Box<CandidateRecord>),
    /// Rejected, with the reasons recorded on the candidate.
    Rejected {
        candidate: Box<CandidateRecord>,
        reasons: Vec<String>,
    },
}

/// The entity a candidate's payload concerns, for the protected-decision check.
fn target_entity(payload: &CandidatePayload) -> Option<EntityId> {
    match payload {
        CandidatePayload::Summary { entity_id, .. } => Some(*entity_id),
        CandidatePayload::Context(record) => Some(record.scope_id),
        CandidatePayload::Edge(edge) => Some(edge.source_entity_id),
    }
}

/// Collect every reason this candidate cannot be admitted as a deterministic or
/// well-formed fact. Empty means it passed validation.
fn rejection_reasons(candidate: &CandidateRecord, active: &SnapshotId) -> Vec<String> {
    let mut reasons = Vec::new();

    // Snapshot freshness: a candidate reasons about the world as it was when the
    // model saw it. If that world has moved on, the candidate is stale.
    if candidate.snapshot_id != *active {
        reasons.push(format!(
            "candidate pinned to snapshot {} but the active snapshot is {}",
            candidate.snapshot_id.0, active.0
        ));
    }

    match &candidate.payload {
        CandidatePayload::Summary { summary, .. } => {
            if !(0.0..=1.0).contains(&summary.confidence) {
                reasons.push(format!(
                    "confidence {} is outside 0..=1; schema-invalid, not clamped",
                    summary.confidence
                ));
            }
        }
        CandidatePayload::Edge(edge) => {
            // The core rule. An inferred candidate proposing an observed edge is
            // claiming a deterministic fact it has no standing to assert.
            if edge.plane == RelationshipPlane::Observed {
                reasons.push(
                    "an inferred candidate cannot set an observed edge; inference has no path \
                     to a deterministic fact or Green axis"
                        .to_string(),
                );
            }
        }
        CandidatePayload::Context(record) => {
            if !(0.0..=1.0).contains(&record.confidence) {
                reasons.push(format!(
                    "confidence {} is outside 0..=1; schema-invalid",
                    record.confidence
                ));
            }
        }
    }

    reasons
}

/// Admit, review, or reject `candidate` against the active snapshot and the set
/// of protected entities.
pub fn admit(
    candidate: CandidateRecord,
    active: &SnapshotId,
    protected: &[EntityId],
) -> AdmissionOutcome {
    let reasons = rejection_reasons(&candidate, active);
    if !reasons.is_empty() {
        let mut rejected = candidate;
        rejected.state = CandidateState::Rejected;
        rejected.rejection_reasons = reasons.clone();
        return AdmissionOutcome::Rejected {
            candidate: Box::new(rejected),
            reasons,
        };
    }

    // Valid, but if it touches protected human intent it is not admitted
    // automatically — a model cannot overwrite a protected decision, only
    // propose a change for a human to accept.
    if target_entity(&candidate.payload).is_some_and(|entity| protected.contains(&entity)) {
        let mut queued = candidate;
        queued.state = CandidateState::Review;
        return AdmissionOutcome::Review(Box::new(queued));
    }

    let mut admitted = candidate;
    admitted.state = CandidateState::Admitted;
    AdmissionOutcome::Admitted(Box::new(admitted))
}
