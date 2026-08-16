//! The continuous semantic lane (specification 7.2).
//!
//! A background lane that enriches the Atlas with model output without ever
//! blocking or overruling the deterministic one. For each unit of work it asks a
//! partner to propose, runs every proposal through admission, and applies only
//! what is admitted — an inferred summary onto its entity, an inferred edge onto
//! the snapshot. Reviewed and rejected proposals are counted, never applied.
//!
//! The invariant: the lane only ever *adds* inferred enrichment. It never writes
//! a deterministic field — not Green, not an observed edge, not the deterministic
//! summary — so running it, or not running it, leaves the deterministic picture
//! identical. With a rules-only partner it proposes nothing and the lane is idle.

use crate::partner::{ModelPartner, WorkItem, propose_and_admit};
use creature_context_core::context::admission::AdmissionOutcome;
use creature_context_types::{
    AtlasSnapshot, EntityId,
    model::{CandidatePayload, CandidateRecord},
};

/// What one pass of the lane did. Everything admitted was applied; everything
/// reviewed or rejected was not.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LaneReport {
    pub admitted: usize,
    pub review: usize,
    pub rejected: usize,
}

/// Enrich `snapshot` by running the semantic lane over the given work entities
/// with `partner`, admitting through the Milestone 3 pipeline and protecting the
/// listed entities. Returns what happened. Only inferred enrichment is applied;
/// deterministic state is never touched.
pub fn run_semantic_lane(
    snapshot: &mut AtlasSnapshot,
    work: &[EntityId],
    partner: &dyn ModelPartner,
    protected: &[EntityId],
) -> LaneReport {
    let active = snapshot.id.clone();
    let mut report = LaneReport::default();

    // Propose and admit first, reading the snapshot immutably; collect the
    // admitted candidates to apply once the read pass is done.
    let mut admitted: Vec<CandidateRecord> = Vec::new();
    for entity_id in work {
        let Some(entity) = snapshot.entities.iter().find(|e| e.id == *entity_id) else {
            continue;
        };
        let item = WorkItem {
            entity,
            snapshot_id: active.clone(),
        };
        for outcome in propose_and_admit(partner, &item, &active, protected) {
            match outcome {
                AdmissionOutcome::Admitted(candidate) => {
                    report.admitted += 1;
                    admitted.push(*candidate);
                }
                AdmissionOutcome::Review(_) => report.review += 1,
                AdmissionOutcome::Rejected { .. } => report.rejected += 1,
            }
        }
    }

    for candidate in admitted {
        apply_admitted(snapshot, candidate);
    }
    report
}

/// Apply one admitted candidate as inferred enrichment. A summary is attached to
/// its entity's inferred summaries; an edge (admission has already guaranteed it
/// is on the inferred plane) is added to the graph; a context record is
/// appended. None of these are deterministic facts.
fn apply_admitted(snapshot: &mut AtlasSnapshot, candidate: CandidateRecord) {
    match candidate.payload {
        CandidatePayload::Summary { entity_id, summary } => {
            if let Some(entity) = snapshot.entities.iter_mut().find(|e| e.id == entity_id) {
                entity.inferred_summaries.push(summary);
            }
        }
        CandidatePayload::Edge(edge) => snapshot.edges.push(edge),
        CandidatePayload::Context(record) => snapshot.records.push(record),
    }
}
