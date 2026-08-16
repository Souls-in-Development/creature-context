//! Routing by measured capability (specification 8).
//!
//! The router chooses which partner does a piece of work. It decides on measured
//! evidence only — the role score a calibration battery produced, whether the
//! partner is cleared for the data's sensitivity, and (as a tie-break) latency.
//! It never reads a provider's or model's name: "role assignment is measured
//! rather than trusted from configuration or model branding" (spec §8). When
//! nothing qualifies it returns `None`, and the caller falls back to the
//! rules-only partner — the router fails closed, it never lowers the bar.

use creature_context_types::{
    context::PrivacyClass,
    model::{CapabilityProfile, CapabilityState, ModelRole},
};

/// What a piece of work needs from a partner.
pub struct RoutingRequest {
    pub role: ModelRole,
    /// The sensitivity of the data this work touches. A partner must be cleared
    /// for at least this class.
    pub data_class: PrivacyClass,
    /// The minimum measured role score to be eligible.
    pub min_score: f32,
}

/// Sensitivity rank — higher means more sensitive. A partner cleared for a class
/// may handle that class and everything less sensitive, so a partner rank must be
/// at least the data's rank to qualify.
fn sensitivity(class: &PrivacyClass) -> u8 {
    match class {
        PrivacyClass::Public => 0,
        PrivacyClass::Project => 1,
        PrivacyClass::Private => 2,
        PrivacyClass::Secret => 3,
    }
}

fn role_score(profile: &CapabilityProfile, role: &ModelRole) -> f32 {
    profile.role_scores.get(role).copied().unwrap_or(0.0)
}

/// Choose the best-qualified profile for `request`, or `None` if none qualifies.
/// Eligibility: measured (`Verified`), cleared for the data class, and at or above
/// the score floor. Among the eligible, the highest role score wins; ties break to
/// lower latency, then to id, so the choice is deterministic.
pub fn route<'a>(
    profiles: &'a [CapabilityProfile],
    request: &RoutingRequest,
) -> Option<&'a CapabilityProfile> {
    profiles
        .iter()
        .filter(|profile| profile.state == CapabilityState::Verified)
        .filter(|profile| sensitivity(&profile.privacy_class) >= sensitivity(&request.data_class))
        .filter(|profile| role_score(profile, &request.role) >= request.min_score)
        .max_by(|a, b| {
            role_score(a, &request.role)
                .total_cmp(&role_score(b, &request.role))
                .then(b.p95_latency_ms.cmp(&a.p95_latency_ms)) // lower latency wins
                .then(a.id.cmp(&b.id).reverse()) // lexicographically smaller id wins
        })
}
