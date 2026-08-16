//! The semantic lane, wired into the resident service (specification 7.2).
//!
//! The background cognitive lane: over the project's stored snapshot it selects
//! a bounded slice of work, asks the model partner to propose enrichment, admits
//! each proposal through the Milestone 3 pipeline, applies what is admitted, and
//! persists. The model never writes directly — every proposal is a
//! `CandidateRecord` decided by admission (`context-model::lane`). With no model
//! available the pass is idle and changes nothing, so the deterministic pipeline
//! is unaffected.
//!
//! A pass is sequenced *after* the deterministic reconcile in the service loop
//! and run off the async thread, so it never blocks watching, and there is no
//! concurrent writer to guard against. A persisted enrichment survives the next
//! reconcile because identity reconciliation carries inferred summaries forward.

use crate::service::ServiceError;
use creature_context_core::project::{ProjectPaths, load_identity};
use creature_context_model::lane::run_semantic_lane;
use creature_context_model::partner::ModelPartner;
use creature_context_store::{AtlasRepository, write_projections};
use creature_context_types::{AtlasSnapshot, EntityId, EntityKind, ScopeScale};
use std::path::Path;

/// Parsed code symbols that do not yet have a model summary — functions, types
/// and components whose inferred summaries are empty. Bounded by `budget` so a
/// pass is short and the lane drains the backlog a slice at a time.
fn work_items(snapshot: &AtlasSnapshot, budget: usize) -> Vec<EntityId> {
    snapshot
        .entities
        .iter()
        .filter(|entity| entity.scale == ScopeScale::Moon)
        .filter(|entity| {
            matches!(
                entity.kind,
                EntityKind::Function | EntityKind::Type | EntityKind::Component
            )
        })
        .filter(|entity| entity.inferred_summaries.is_empty())
        .map(|entity| entity.id)
        .take(budget)
        .collect()
}

/// Entities a model may only propose changes to, never make them: those carrying
/// a protected decision (spec §7.3). Admission holds these for human review.
fn protected(snapshot: &AtlasSnapshot) -> Vec<EntityId> {
    snapshot
        .entities
        .iter()
        .filter(|entity| !entity.protected_decision_ids.is_empty())
        .map(|entity| entity.id)
        .collect()
}

/// Run one bounded semantic pass over the stored snapshot with `partner`.
/// Returns the number of enrichments admitted and persisted; returns 0 and
/// stores nothing when there is no work or nothing is admitted.
pub fn semantic_pass(
    root: &Path,
    partner: &dyn ModelPartner,
    budget: usize,
) -> Result<usize, ServiceError> {
    let paths = ProjectPaths::new(root);
    let mut repository = AtlasRepository::open(&paths.database)?;
    let mut snapshot = repository.load_snapshot()?;

    let work = work_items(&snapshot, budget);
    if work.is_empty() {
        return Ok(0);
    }
    let protected = protected(&snapshot);
    let report = run_semantic_lane(&mut snapshot, &work, partner, &protected);
    if report.admitted == 0 {
        return Ok(0); // nothing admitted — leave the store untouched
    }

    repository.replace_snapshot(&snapshot)?;
    write_projections(root, &snapshot, &load_identity(root)?.project_id)?;
    let png = creature_context_store::texture::render_snapshot_png(&snapshot, true);
    let _ = creature_context_core::project::atomic_write(&root.join("ATLAS.png"), &png);
    crate::metadata::apply(root, &snapshot);
    Ok(report.admitted)
}

/// Whether an on-device model partner is available on this host. Measured, not
/// assumed; false on any platform without a built adapter.
pub fn model_available() -> bool {
    #[cfg(target_os = "macos")]
    {
        creature_context_model::platform::apple::FoundationPartner::detect().is_available()
    }
    // Windows selects Phi Silica. This arm compiles only on Windows and is not
    // exercised on the macOS development host; the adapter's deterministic
    // behaviour (availability gating, no-model idleness) is covered by unit tests
    // in `creature_context_model::platform::windows`.
    #[cfg(target_os = "windows")]
    {
        creature_context_model::platform::windows::PhiSilicaPartner::detect().is_available()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        false
    }
}

/// Run one bounded pass with the host's model partner if one is available,
/// otherwise do nothing. This is the entry point the service loop schedules; it
/// owns the platform choice so the loop stays platform-agnostic.
pub fn run_pass_if_available(root: &Path, budget: usize) -> Result<usize, ServiceError> {
    #[cfg(target_os = "macos")]
    {
        use creature_context_model::platform::apple::FoundationPartner;
        use std::sync::OnceLock;
        // Measure the model once against the real calibration battery, then reuse
        // the calibrated (Verified) partner for the daemon's lifetime — the
        // resident system knows its model's true, measured capability rather than
        // a placeholder (spec §8). Calibration runs the model a handful of times,
        // so the first pass is slower; it is cached here so later passes are not.
        static PARTNER: OnceLock<Option<FoundationPartner>> = OnceLock::new();
        let partner = PARTNER.get_or_init(|| {
            let detected = FoundationPartner::detect();
            if detected.is_available() {
                Some(detected.calibrate(&chrono::Utc::now().to_rfc3339()))
            } else {
                None
            }
        });
        if let Some(partner) = partner {
            return semantic_pass(root, partner, budget);
        }
    }
    // Windows Phi Silica: the same shape as macOS, selecting the host's on-device
    // producer and calibrating it once against the real model before use. This arm
    // compiles only on Windows and has not been exercised on the macOS development
    // host — its native path is verified on a Copilot+ PC, not here (spec §8).
    #[cfg(target_os = "windows")]
    {
        use creature_context_model::platform::windows::PhiSilicaPartner;
        use std::sync::OnceLock;
        static PARTNER: OnceLock<Option<PhiSilicaPartner>> = OnceLock::new();
        let partner = PARTNER.get_or_init(|| {
            let detected = PhiSilicaPartner::detect();
            if detected.is_available() {
                Some(detected.calibrate(&chrono::Utc::now().to_rfc3339()))
            } else {
                None
            }
        });
        if let Some(partner) = partner {
            return semantic_pass(root, partner, budget);
        }
    }
    let _ = (root, budget);
    Ok(0)
}
