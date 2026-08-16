//! The rules-only partner — the mandatory zero-model fallback (specification 8).
//!
//! Every capability degrades to deterministic operation when no model is
//! available: the scanner, identity, Green, Orbit and admission all work with
//! zero models present. This partner is that degradation for the semantic lane.
//! It proposes nothing, because inference is precisely what it does not have, and
//! it reports its capability honestly — `Unavailable`, with zero role scores and
//! no artificial baseline. The lane is simply idle, and the deterministic
//! pipeline stands alone.

use crate::partner::{ModelPartner, WorkItem};
use creature_context_types::{
    context::PrivacyClass,
    model::{CandidateRecord, CapabilityProfile, CapabilityState},
};
use std::collections::{BTreeMap, BTreeSet};

/// A partner with no model behind it.
pub struct RulesOnlyPartner {
    capability: CapabilityProfile,
}

impl RulesOnlyPartner {
    pub fn new() -> Self {
        Self {
            capability: CapabilityProfile {
                id: "rules-only".into(),
                provider_id: "rules-only".into(),
                model_id: "none".into(),
                // The honest state: there is no model here. Not an unverified
                // one, not a weak one — none.
                state: CapabilityState::Unavailable,
                // Rules-only touches no model service, so nothing leaves the
                // device — an on-device, private data class is the truthful one.
                privacy_class: PrivacyClass::Private,
                // No role is served: an absent model scores zero, never a
                // baseline (spec §8).
                role_scores: BTreeMap::new(),
                structured_output_rate: 0.0,
                attribution_rate: 0.0,
                p95_latency_ms: 0,
                measured_input_limit: 0,
                measured_output_limit: 0,
                memory_mib: 0,
                storage_mib: 0,
                tested_languages: BTreeSet::new(),
                calibration_version: "rules-only".into(),
                calibrated_at: String::new(),
                evidence_locator: None,
            },
        }
    }
}

impl Default for RulesOnlyPartner {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelPartner for RulesOnlyPartner {
    fn capability(&self) -> &CapabilityProfile {
        &self.capability
    }

    /// Nothing. There is no inference to offer, so the lane stays idle and the
    /// deterministic pipeline is unaffected.
    fn propose(&self, _work: &WorkItem) -> Vec<CandidateRecord> {
        Vec::new()
    }
}
