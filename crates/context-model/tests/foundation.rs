//! Milestone 5 Task 5: the Apple Foundation Models adapter.
//!
//! On macOS this drives the real on-device model through the C ABI. The adapter
//! reports its capability by measurement, and when the model is available a real
//! inference is run and admitted as an inferred candidate — the model's output
//! reaches the Atlas only through admission, never directly (spec §7.3, §8, §16).
//! Where the model is not available the adapter proposes nothing and reports its
//! true state, so the test verifies honesty in both directions.

#![cfg(target_os = "macos")]

use creature_context_core::context::admission::AdmissionOutcome;
use creature_context_model::partner::{ModelPartner, WorkItem, propose_and_admit};
use creature_context_model::platform::apple::FoundationPartner;
use creature_context_types::{
    AtlasEntity, EntityId, EntityKind, ScopeScale, SnapshotId,
    model::{CapabilityState, ModelRole},
};

const SNAP: &str = "snap-foundation";

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
        deterministic_summary: String::new(),
        inferred_summaries: vec![],
        uncertainty: vec![],
        snapshot_id: SnapshotId(SNAP.into()),
        observed_at: "2026-08-09T00:00:00Z".into(),
        fresh_until: None,
    }
}

#[test]
fn the_adapter_reports_measured_capability_never_verified_from_detection() {
    let partner = FoundationPartner::detect();
    assert_ne!(
        partner.capability().state,
        CapabilityState::Verified,
        "detection alone never claims Verified — only a calibration battery does (spec §8)"
    );
    if partner.is_available() {
        assert_eq!(
            partner.capability().state,
            CapabilityState::ImplementedUnverified,
            "the model is reachable but has not been scored"
        );
    } else {
        assert_eq!(
            partner.capability().state,
            CapabilityState::Unavailable,
            "an unreachable model is reported unavailable, never faked"
        );
    }
}

#[test]
fn calibration_turns_detection_into_a_measured_verified_profile() {
    let partner = FoundationPartner::detect();
    if !partner.is_available() {
        eprintln!("Foundation Models unavailable on this host — skipping calibration");
        return;
    }
    // Detection alone was ImplementedUnverified with no scores; calibration runs
    // the real battery and measures.
    let calibrated = partner.calibrate("2026-08-10T00:00:00Z");
    let profile = calibrated.capability();

    assert_eq!(
        profile.state,
        CapabilityState::Verified,
        "calibration is verification — the model was actually run and scored"
    );
    let contextual = profile
        .role_scores
        .get(&ModelRole::Contextual)
        .copied()
        .unwrap_or(0.0);
    eprintln!(
        "CALIBRATED contextual={contextual} structured={} attribution={}",
        profile.structured_output_rate, profile.attribution_rate
    );
    assert!(
        contextual > 0.0,
        "the on-device model demonstrably performed the contextual task — a measured, earned score"
    );
}

#[test]
fn when_available_a_real_inference_is_admitted_as_inferred() {
    let partner = FoundationPartner::detect();
    if !partner.is_available() {
        // Honest skip: the on-device model is not usable on this host, so there
        // is nothing to verify. The adapter's Unavailable path is covered above.
        eprintln!("Foundation Models unavailable on this host — skipping live inference");
        return;
    }

    let entity = entity("build");
    let work = WorkItem {
        entity: &entity,
        snapshot_id: SnapshotId(SNAP.into()),
    };
    let active = SnapshotId(SNAP.into());
    let outcomes = propose_and_admit(&partner, &work, &active, &[]);

    assert_eq!(
        outcomes.len(),
        1,
        "the model produced exactly one summary proposal"
    );
    match &outcomes[0] {
        AdmissionOutcome::Admitted(candidate) => match &candidate.payload {
            creature_context_types::model::CandidatePayload::Summary { summary, .. } => {
                assert!(
                    !summary.value.trim().is_empty(),
                    "a real, non-empty on-device summary was admitted as inferred"
                );
                assert_eq!(summary.producer, "apple-foundation");
            }
            other => panic!("expected a summary payload, got {other:?}"),
        },
        other => panic!("a valid inferred summary must be admitted, got {other:?}"),
    }
}
