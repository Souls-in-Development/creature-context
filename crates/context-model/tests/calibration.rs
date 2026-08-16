//! Milestone 5 Task 2: capability by measurement, and routing by capability.
//!
//! A `CapabilityProfile` is populated by running real Creature Context work and
//! scoring the output — "empty, invalid or unverifiable output scores near zero;
//! it receives no artificial baseline" (spec §8). The router then chooses among
//! measured profiles by score, privacy and hardware — never by provider
//! branding — and fails closed to the rules-only fallback when nothing qualifies.

use creature_context_model::calibration::{CalibrationTask, Expectation, calibrate};
use creature_context_model::partner::{ModelPartner, WorkItem};
use creature_context_model::router::{RoutingRequest, route};
use creature_context_model::rules::RulesOnlyPartner;
use creature_context_types::{
    AtlasEntity, CandidateId, EntityId, EntityKind, RecordId, ScopeScale, SnapshotId,
    context::PrivacyClass,
    model::{
        CandidatePayload, CandidateRecord, CandidateState, CapabilityProfile, CapabilityState,
        InferredSummary, ModelRole,
    },
};
use std::collections::{BTreeMap, BTreeSet};

const SNAP: &str = "snap-calibration";

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

/// A partner that summarises each entity as "<name> is calibrated", citing a
/// source — a stand-in for a competent model. The battery below expects the
/// entity's name in the summary, so this partner scores well and an empty one
/// (rules-only) scores zero.
struct CompetentPartner {
    capability: CapabilityProfile,
}

impl ModelPartner for CompetentPartner {
    fn capability(&self) -> &CapabilityProfile {
        &self.capability
    }
    fn propose(&self, work: &WorkItem) -> Vec<CandidateRecord> {
        vec![CandidateRecord {
            id: CandidateId::new(),
            payload: CandidatePayload::Summary {
                entity_id: work.entity.id,
                summary: InferredSummary {
                    value: format!("{} is calibrated", work.entity.canonical_name),
                    producer: "competent".into(),
                    model_id: "competent-1".into(),
                    confidence: 0.9,
                    source_record_ids: vec![RecordId::new()],
                    snapshot_id: work.snapshot_id.clone(),
                },
            },
            provider_id: "competent".into(),
            model_id: "competent-1".into(),
            capability_profile_id: "competent".into(),
            schema_version: 1,
            state: CandidateState::Pending,
            rejection_reasons: vec![],
            created_at: "2026-08-09T00:00:00Z".into(),
            snapshot_id: work.snapshot_id.clone(),
        }]
    }
}

fn battery() -> Vec<CalibrationTask> {
    ["alpha", "beta", "gamma"]
        .into_iter()
        .map(|name| CalibrationTask {
            id: format!("summarise-{name}"),
            role: ModelRole::Contextual,
            entity: entity(name),
            expectation: Expectation::SummaryContains(name.into()),
        })
        .collect()
}

#[test]
fn a_competent_partner_scores_high_from_measured_output() {
    let partner = CompetentPartner {
        capability: RulesOnlyPartner::new().capability().clone(),
    };
    let profile = calibrate(&partner, &battery(), "2026-08-09T00:00:00Z");
    let contextual = profile.role_scores.get(&ModelRole::Contextual).copied();
    assert_eq!(
        contextual,
        Some(1.0),
        "every summary contained the expected name — a measured, earned score"
    );
    assert_eq!(profile.attribution_rate, 1.0, "every output cited a source");
    assert_eq!(
        profile.state,
        CapabilityState::Verified,
        "a profile that ran the battery is verified by measurement"
    );
}

#[test]
fn an_empty_output_partner_scores_near_zero() {
    let profile = calibrate(&RulesOnlyPartner::new(), &battery(), "2026-08-09T00:00:00Z");
    assert_eq!(
        profile.role_scores.get(&ModelRole::Contextual).copied(),
        Some(0.0),
        "rules-only proposes nothing, so it earns nothing — no artificial baseline (spec §8)"
    );
    assert_eq!(profile.structured_output_rate, 0.0);
    assert_eq!(profile.attribution_rate, 0.0);
}

/// A measured profile with a given role score, privacy class and brand-y name.
fn profile(id: &str, role: ModelRole, score: f32, privacy: PrivacyClass) -> CapabilityProfile {
    let mut role_scores = BTreeMap::new();
    role_scores.insert(role, score);
    CapabilityProfile {
        id: id.into(),
        provider_id: id.into(),
        model_id: id.into(),
        state: CapabilityState::Verified,
        privacy_class: privacy,
        role_scores,
        structured_output_rate: 1.0,
        attribution_rate: 1.0,
        p95_latency_ms: 100,
        measured_input_limit: 8000,
        measured_output_limit: 2000,
        memory_mib: 0,
        storage_mib: 0,
        tested_languages: BTreeSet::new(),
        calibration_version: "1".into(),
        calibrated_at: "2026-08-09T00:00:00Z".into(),
        evidence_locator: None,
    }
}

fn request(role: ModelRole, data_class: PrivacyClass, min_score: f32) -> RoutingRequest {
    RoutingRequest {
        role,
        data_class,
        min_score,
    }
}

#[test]
fn the_router_picks_the_higher_measured_score_not_the_brandier_name() {
    // "BigBrandAI" scores low; "plain" scores high. Score wins.
    let profiles = vec![
        profile(
            "BigBrandAI",
            ModelRole::Contextual,
            0.3,
            PrivacyClass::Private,
        ),
        profile("plain", ModelRole::Contextual, 0.9, PrivacyClass::Private),
    ];
    let chosen = route(
        &profiles,
        &request(ModelRole::Contextual, PrivacyClass::Project, 0.5),
    );
    assert_eq!(
        chosen.map(|p| p.id.as_str()),
        Some("plain"),
        "capability is measured, never taken from the provider's name (spec §8)"
    );
}

#[test]
fn the_router_respects_privacy_and_excludes_a_partner_that_cannot_hold_the_data() {
    // A cloud-grade (Public) partner cannot handle Private data, however good.
    let profiles = vec![profile(
        "cloud",
        ModelRole::Contextual,
        0.99,
        PrivacyClass::Public,
    )];
    let chosen = route(
        &profiles,
        &request(ModelRole::Contextual, PrivacyClass::Private, 0.5),
    );
    assert!(
        chosen.is_none(),
        "no partner may process data more sensitive than it is cleared for"
    );
}

#[test]
fn the_router_fails_closed_when_nothing_meets_the_floor() {
    let profiles = vec![profile(
        "weak",
        ModelRole::Contextual,
        0.2,
        PrivacyClass::Private,
    )];
    let chosen = route(
        &profiles,
        &request(ModelRole::Contextual, PrivacyClass::Project, 0.5),
    );
    assert!(
        chosen.is_none(),
        "below the score floor, the router selects nothing — the caller falls back to rules-only"
    );
}

#[test]
fn an_uncalibrated_profile_is_not_eligible() {
    let mut unverified = profile("adapter", ModelRole::Contextual, 0.9, PrivacyClass::Private);
    unverified.state = CapabilityState::ImplementedUnverified;
    let profiles = [unverified];
    let chosen = route(
        &profiles,
        &request(ModelRole::Contextual, PrivacyClass::Project, 0.5),
    );
    assert!(
        chosen.is_none(),
        "only measured (Verified) capability is eligible; an unrun adapter is not trusted"
    );
}
