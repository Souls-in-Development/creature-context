//! Windows Phi Silica adapter (specification 8, 16).
//!
//! A `ModelPartner` backed by the on-device small language model that ships in
//! the Windows App SDK — Phi Silica — reached through the WinRT class
//! `Microsoft.Windows.AI.Text.LanguageModel` (`GetReadyState`, `EnsureReadyAsync`,
//! `GenerateResponseAsync`). It reports its capability by measurement: the bridge
//! asks the runtime whether the model is actually ready, and when it is, `propose`
//! runs a real summary and returns it as an *inferred* candidate for admission.
//! The model never writes: like every partner, its only output is a
//! `CandidateRecord` the deterministic reconciler decides on (spec §7.3).
//!
//! The WinRT call lives behind a C ABI in `platform/windows/CcPhiSilicaBridge.cpp`
//! (C++/WinRT), compiled and linked only inside a Windows build that provides the
//! Windows App SDK projection headers and sets the `phi_silica_bridge` cfg.
//! Everywhere else — this macOS development host, any target without the shim —
//! the adapter compiles to the honest fallback below that reports `Unavailable`
//! and proposes nothing, so the deterministic pipeline is unaffected.
//!
//! Honesty boundary: this adapter is written and unit-tested on a non-Windows
//! host, where it can only ever be exercised through its fallback. Its capability
//! is `Unavailable` here, `ImplementedUnverified` on real hardware where the model
//! is ready, and reaches `Verified` only when the calibration battery runs against
//! the live model on that device — never from this host (spec §8; the honesty
//! rule that a capability that has not run on a platform is never `Verified`).
//!
//! (The Windows AI platform is migrating Phi Silica to a successor model, Aion
//! Instruct, in late 2026; `LanguageModel` remains the entry point, so this
//! adapter is unaffected by that swap.)

use crate::partner::{ModelPartner, WorkItem};
use creature_context_types::{
    CandidateId,
    model::{
        CandidatePayload, CandidateRecord, CandidateState, CapabilityProfile, CapabilityState,
        InferredSummary,
    },
};

/// The bridge to the on-device model. Present only when a Windows build compiled
/// and linked the C++/WinRT shim (enabling `phi_silica_bridge`); otherwise a
/// fallback that is honest about its absence.
mod bridge {
    #[cfg(phi_silica_bridge)]
    mod ffi {
        use std::os::raw::c_char;
        unsafe extern "C" {
            pub fn cc_phi_silica_availability() -> i32;
            pub fn cc_phi_silica_summarize(prompt: *const c_char) -> *mut c_char;
            pub fn cc_phi_silica_free(ptr: *mut c_char);
        }
    }

    /// Whether Phi Silica is ready right now — measured via `LanguageModel`'s
    /// ready-state, not assumed.
    #[cfg(phi_silica_bridge)]
    pub fn available() -> bool {
        unsafe { ffi::cc_phi_silica_availability() == 1 }
    }

    /// Run one prompt through `GenerateResponseAsync` and return the model's text,
    /// or `None` on unavailability or error.
    #[cfg(phi_silica_bridge)]
    pub fn summarize(prompt: &str) -> Option<String> {
        use std::ffi::{CStr, CString};
        let c_prompt = CString::new(prompt).ok()?;
        unsafe {
            let out = ffi::cc_phi_silica_summarize(c_prompt.as_ptr());
            if out.is_null() {
                return None;
            }
            let text = CStr::from_ptr(out).to_string_lossy().into_owned();
            ffi::cc_phi_silica_free(out);
            let text = text.trim().to_string();
            (!text.is_empty()).then_some(text)
        }
    }

    #[cfg(not(phi_silica_bridge))]
    pub fn available() -> bool {
        false
    }

    #[cfg(not(phi_silica_bridge))]
    pub fn summarize(_prompt: &str) -> Option<String> {
        None
    }
}

/// A partner backed by Windows Phi Silica.
pub struct PhiSilicaPartner {
    capability: CapabilityProfile,
}

impl PhiSilicaPartner {
    /// Build the adapter, measuring the model's readiness now. The state is
    /// `ImplementedUnverified` when Phi Silica is ready but the calibration battery
    /// has not scored it, `Unavailable` when it is not — never `Verified` from
    /// availability alone; only a real battery verifies it (spec §8).
    pub fn detect() -> Self {
        let state = if bridge::available() {
            CapabilityState::ImplementedUnverified
        } else {
            CapabilityState::Unavailable
        };
        Self {
            capability: CapabilityProfile {
                id: "windows-phi-silica".into(),
                provider_id: "microsoft".into(),
                model_id: "phi-silica".into(),
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

    /// Whether Phi Silica is available on this machine.
    pub fn is_available(&self) -> bool {
        bridge::available()
    }

    /// Measure this partner against a real calibration battery and return a partner
    /// that reports the resulting profile — `Verified`, with role scores earned
    /// from actual on-device inference (spec §8). It runs the model once per task,
    /// so it is not cheap and is done once. Calibrating an unavailable partner
    /// scores zero on every task, honestly — which is exactly what happens on any
    /// host without Phi Silica, including the one this adapter is developed on, so
    /// no fabricated capability can ever come out of it.
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

impl ModelPartner for PhiSilicaPartner {
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
                    producer: "windows-phi-silica".into(),
                    model_id: "phi-silica".into(),
                    // A model's own confidence is not evidence; a fixed, modest
                    // value marks this as a proposal, and admission plus the proof
                    // floor decide what it can affect.
                    confidence: 0.5,
                    // Provenance is the producer and model id; no source record is
                    // cited because none is fabricated.
                    source_record_ids: vec![],
                    snapshot_id: work.snapshot_id.clone(),
                },
            },
            provider_id: "windows-phi-silica".into(),
            model_id: "phi-silica".into(),
            capability_profile_id: "windows-phi-silica".into(),
            schema_version: 1,
            state: CandidateState::Pending,
            rejection_reasons: vec![],
            created_at: String::new(),
            snapshot_id: work.snapshot_id.clone(),
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::partner::WorkItem;
    use creature_context_types::{
        AtlasEntity, EntityId, EntityKind, ScopeScale, SnapshotId, context::PrivacyClass,
    };

    /// A minimal Moon-scale code symbol to hand the partner as work.
    fn moon(name: &str, kind: EntityKind) -> AtlasEntity {
        AtlasEntity {
            id: EntityId::new(),
            scale: ScopeScale::Moon,
            kind,
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
            structural_fingerprint: String::new(),
            local_evidence: vec![],
            inherited_evidence: vec![],
            green: None,
            open_conflict_ids: vec![],
            deterministic_summary: String::new(),
            inferred_summaries: vec![],
            uncertainty: vec![],
            snapshot_id: SnapshotId("test".into()),
            observed_at: String::new(),
            fresh_until: None,
        }
    }

    /// On a host without the Phi Silica bridge — every host that is not a Windows
    /// build linking the shim, this development machine included — detection is
    /// honestly `Unavailable`, and the identity of the profile is fixed.
    #[test]
    fn reports_unavailable_without_the_bridge() {
        let partner = PhiSilicaPartner::detect();
        assert!(!partner.is_available());
        let cap = partner.capability();
        assert_eq!(cap.state, CapabilityState::Unavailable);
        assert_eq!(cap.id, "windows-phi-silica");
        assert_eq!(cap.provider_id, "microsoft");
        assert_eq!(cap.model_id, "phi-silica");
        assert_eq!(cap.privacy_class, PrivacyClass::Private);
    }

    /// With no reachable model, the partner proposes nothing — the deterministic
    /// pipeline runs exactly as it would with no model at all.
    #[test]
    fn proposes_nothing_without_the_bridge() {
        let partner = PhiSilicaPartner::detect();
        let entity = moon("read_file", EntityKind::Function);
        let work = WorkItem {
            entity: &entity,
            snapshot_id: entity.snapshot_id.clone(),
        };
        assert!(partner.propose(&work).is_empty());
    }

    /// The anti-facade guarantee: calibrating this partner where the model is not
    /// reachable cannot manufacture capability. The battery measures real output;
    /// there is none, so every role scores zero and both rates are zero. Only a
    /// device where the model actually answers can earn a non-zero profile.
    #[test]
    fn calibration_on_an_unavailable_host_earns_zero() {
        let partner = PhiSilicaPartner::detect().calibrate("2026-08-11T00:00:00Z");
        let cap = partner.capability();
        assert_eq!(cap.structured_output_rate, 0.0);
        assert_eq!(cap.attribution_rate, 0.0);
        assert!(cap.role_scores.values().all(|&score| score == 0.0));
    }
}
