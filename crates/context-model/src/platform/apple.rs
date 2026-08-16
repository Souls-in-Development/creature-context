//! Apple Foundation Models adapter (specification 8, 16).
//!
//! A `ModelPartner` backed by the on-device model, reached through the C ABI in
//! `platform/apple/.../CBridge.swift`. It reports its capability by measurement —
//! the bridge tells it whether the model is actually available — and when it is,
//! `propose` runs a real summary and returns it as an *inferred* candidate for
//! admission. The model never writes: like every partner, its only output is a
//! `CandidateRecord` that the deterministic reconciler decides on (spec §7.3).
//!
//! Where the bridge cannot be built (a non-macOS host, no Swift, no framework)
//! the adapter compiles to a fallback that reports `Unavailable` and proposes
//! nothing, so the deterministic pipeline is unaffected.

use crate::partner::{ModelPartner, WorkItem};
use creature_context_types::{
    CandidateId,
    model::{
        CandidatePayload, CandidateRecord, CandidateState, CapabilityProfile, CapabilityState,
        InferredSummary,
    },
};

/// The bridge to the on-device model. Present only when `build.rs` compiled and
/// linked the Swift C ABI; otherwise a fallback that is honest about its absence.
mod bridge {
    #[cfg(foundation_bridge)]
    mod ffi {
        use std::os::raw::c_char;
        unsafe extern "C" {
            pub fn cc_foundation_availability() -> i32;
            pub fn cc_foundation_summarize(prompt: *const c_char) -> *mut c_char;
            pub fn cc_foundation_free(ptr: *mut c_char);
        }
    }

    /// Whether the on-device model is available right now — measured, not assumed.
    #[cfg(foundation_bridge)]
    pub fn available() -> bool {
        unsafe { ffi::cc_foundation_availability() == 1 }
    }

    /// Run one prompt and return the model's response, or `None` on
    /// unavailability or error.
    #[cfg(foundation_bridge)]
    pub fn summarize(prompt: &str) -> Option<String> {
        use std::ffi::{CStr, CString};
        let c_prompt = CString::new(prompt).ok()?;
        unsafe {
            let out = ffi::cc_foundation_summarize(c_prompt.as_ptr());
            if out.is_null() {
                return None;
            }
            let text = CStr::from_ptr(out).to_string_lossy().into_owned();
            ffi::cc_foundation_free(out);
            let text = text.trim().to_string();
            (!text.is_empty()).then_some(text)
        }
    }

    #[cfg(not(foundation_bridge))]
    pub fn available() -> bool {
        false
    }

    #[cfg(not(foundation_bridge))]
    pub fn summarize(_prompt: &str) -> Option<String> {
        None
    }
}

/// A partner backed by Apple Foundation Models.
pub struct FoundationPartner {
    capability: CapabilityProfile,
}

impl FoundationPartner {
    /// Build the adapter, measuring the model's availability now. The capability
    /// state is `ImplementedUnverified` when the model is reachable but the
    /// calibration battery has not scored it, `Unavailable` when it is not — it
    /// is never `Verified` from availability alone; only a real battery verifies
    /// it (spec §8).
    pub fn detect() -> Self {
        let state = if bridge::available() {
            CapabilityState::ImplementedUnverified
        } else {
            CapabilityState::Unavailable
        };
        Self {
            capability: CapabilityProfile {
                id: "apple-foundation".into(),
                provider_id: "apple".into(),
                model_id: "system-language-model".into(),
                state,
                privacy_class: creature_context_types::context::PrivacyClass::Private,
                role_scores: Default::default(),
                structured_output_rate: 0.0,
                attribution_rate: 0.0,
                p95_latency_ms: 0,
                measured_input_limit: 0,
                measured_output_limit: 0,
                memory_mib: 0,
                storage_mib: 0,
                tested_languages: Default::default(),
                calibration_version: "detect".into(),
                calibrated_at: String::new(),
                evidence_locator: None,
            },
        }
    }

    /// Whether the on-device model is available on this machine.
    pub fn is_available(&self) -> bool {
        bridge::available()
    }

    /// Measure this partner against a real calibration battery and return a
    /// partner that reports the resulting profile — `Verified`, with role scores
    /// earned from actual on-device inference (spec §8). This is what turns
    /// "detected" into "measured"; it runs the model once per task, so it is not
    /// cheap and is done once. Calibrating an unavailable partner scores zero on
    /// every task, honestly.
    pub fn calibrate(self, calibrated_at: &str) -> Self {
        let profile = crate::calibration::calibrate(
            &self,
            &crate::calibration::contextual_battery(),
            calibrated_at,
        );
        Self {
            capability: profile,
        }
    }
}

impl ModelPartner for FoundationPartner {
    fn capability(&self) -> &CapabilityProfile {
        &self.capability
    }

    fn propose(&self, work: &WorkItem) -> Vec<CandidateRecord> {
        let prompt = format!(
            "In 12 words or fewer, describe what the {:?} named '{}' does. Answer with only the description.",
            work.entity.kind, work.entity.canonical_name
        );
        let Some(text) = bridge::summarize(&prompt) else {
            return Vec::new(); // unavailable or errored → propose nothing
        };
        vec![CandidateRecord {
            id: CandidateId::new(),
            payload: CandidatePayload::Summary {
                entity_id: work.entity.id,
                summary: InferredSummary {
                    value: text,
                    producer: "apple-foundation".into(),
                    model_id: "system-language-model".into(),
                    // A model's own confidence is not evidence; a fixed, modest
                    // value marks this as a proposal, and admission plus the
                    // proof floor decide what it can affect.
                    confidence: 0.5,
                    // Provenance is the producer and model id below; no source
                    // record is cited because none is fabricated — a made-up
                    // RecordId would reference nothing.
                    source_record_ids: vec![],
                    snapshot_id: work.snapshot_id.clone(),
                },
            },
            provider_id: "apple-foundation".into(),
            model_id: "system-language-model".into(),
            capability_profile_id: "apple-foundation".into(),
            schema_version: 1,
            state: CandidateState::Pending,
            rejection_reasons: vec![],
            created_at: String::new(),
            snapshot_id: work.snapshot_id.clone(),
        }]
    }
}
