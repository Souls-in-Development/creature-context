//! Milestone 5: the semantic lane, wired into the resident service (spec §7.2).
//!
//! A pass proposes model enrichment, admits it, applies what is admitted, and
//! persists — and the enrichment survives the next deterministic reconcile,
//! because identity reconciliation carries inferred summaries forward. With no
//! model the pass is idle. The daemon runs the real on-device model on its own
//! schedule; here a scripted partner stands in so the wiring is deterministic.

use creature_context_core::project::{ProjectPaths, init_project};
use creature_context_model::partner::{ModelPartner, WorkItem};
use creature_context_model::rules::RulesOnlyPartner;
use creature_context_runtime::semantic::semantic_pass;
use creature_context_runtime::service::reconcile_once;
use creature_context_store::AtlasRepository;
use creature_context_types::{
    AtlasSnapshot, CandidateId, EntityKind,
    model::{
        CandidatePayload, CandidateRecord, CandidateState, CapabilityProfile, InferredSummary,
    },
};
use std::fs;
use std::path::{Path, PathBuf};

fn project(name: &str, body: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("cc-semantic-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("PURPOSE.md"), "# Fixture\n\n## Goals\n- enrich\n").unwrap();
    fs::write(root.join("src/lib.rs"), body).unwrap();
    init_project(&root).unwrap();
    root
}

fn load(root: &Path) -> AtlasSnapshot {
    AtlasRepository::open(&ProjectPaths::new(root).database)
        .unwrap()
        .load_snapshot()
        .unwrap()
}

/// The model summary attached to the `build` symbol, if any.
fn build_summary(snapshot: &AtlasSnapshot) -> Option<String> {
    snapshot
        .entities
        .iter()
        .find(|e| e.canonical_name == "build" && e.kind != EntityKind::File)
        .and_then(|e| e.inferred_summaries.first())
        .map(|s| s.value.clone())
}

/// Stands in for a competent contextual model: proposes one inferred summary per
/// entity it is asked about.
struct SummarisingPartner {
    capability: CapabilityProfile,
}

impl ModelPartner for SummarisingPartner {
    fn capability(&self) -> &CapabilityProfile {
        &self.capability
    }
    fn propose(&self, work: &WorkItem) -> Vec<CandidateRecord> {
        vec![CandidateRecord {
            id: CandidateId::new(),
            payload: CandidatePayload::Summary {
                entity_id: work.entity.id,
                summary: InferredSummary {
                    value: format!("summary of {}", work.entity.canonical_name),
                    producer: "scripted".into(),
                    model_id: "m-1".into(),
                    confidence: 0.6,
                    source_record_ids: vec![],
                    snapshot_id: work.snapshot_id.clone(),
                },
            },
            provider_id: "scripted".into(),
            model_id: "m-1".into(),
            capability_profile_id: "scripted".into(),
            schema_version: 1,
            state: CandidateState::Pending,
            rejection_reasons: vec![],
            created_at: String::new(),
            snapshot_id: work.snapshot_id.clone(),
        }]
    }
}

#[test]
fn a_pass_with_no_model_is_idle_and_changes_nothing() {
    let root = project("idle", "pub fn build() {}\n");
    reconcile_once(&root).unwrap();

    let admitted = semantic_pass(&root, &RulesOnlyPartner::new(), 8).unwrap();
    assert_eq!(
        admitted, 0,
        "rules-only proposes nothing, so nothing is admitted"
    );
    assert!(
        build_summary(&load(&root)).is_none(),
        "the stored snapshot is untouched"
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_pass_enriches_persists_and_survives_a_reconcile() {
    let root = project("enrich", "pub fn build() {}\n");
    reconcile_once(&root).unwrap();

    let partner = SummarisingPartner {
        capability: RulesOnlyPartner::new().capability().clone(),
    };
    let admitted = semantic_pass(&root, &partner, 8).unwrap();
    assert!(admitted >= 1, "the proposal was admitted and persisted");
    assert!(
        build_summary(&load(&root))
            .expect("build has a summary")
            .contains("build"),
        "the admitted summary is stored on the symbol"
    );

    // The load-bearing property: a deterministic reconcile after an edit moves
    // the symbol but preserves its id, so the model's enrichment travels with it
    // instead of being wiped.
    fs::write(
        root.join("src/lib.rs"),
        "// moved down\n\npub fn build() {}\n",
    )
    .unwrap();
    reconcile_once(&root).unwrap();
    assert!(
        build_summary(&load(&root))
            .expect("build still has a summary")
            .contains("build"),
        "the semantic enrichment survives the deterministic reconcile"
    );

    let _ = fs::remove_dir_all(&root);
}
