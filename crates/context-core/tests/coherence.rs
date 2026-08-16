//! Milestone 5 Task 4: the coherence axis (H).
//!
//! H is "whether authoritative intent, structural facts, observed behaviour and
//! current tool state agree" (spec §11). Before this the evaluator scored H from
//! evidence like any axis but ignored contradictions entirely: an entity with an
//! open contradiction assessed exactly as one with none. Now an open
//! `ConflictRecord` on an entity darkens H by its recorded severity.
//!
//! The containment rule the milestone lives by: "inference may identify a
//! possible conflict but cannot alone certify Green or a deterministic failure"
//! (spec §11). A model-suspected contradiction is created at Yellow severity and
//! can never redden H on its own; only a deterministically-verified or
//! human-confirmed contradiction carries Red severity. The evaluator reads the
//! severity; the cap is enforced where the conflict is created (admission).

use creature_context_core::green::evaluate_snapshot;
use creature_context_types::{
    AtlasEntity, AtlasSnapshot, ConflictId, ConflictRecord, ConflictState, EntityId, EntityKind,
    Evidence, EvidenceOutcome, FactSource, GreenPolicy, ProofStrength, RecordId, ScopeScale,
    SnapshotId,
    green::{GreenAxis, GreenCode},
};

const SNAP: &str = "snap-coherence";

fn coherence_evidence() -> Evidence {
    Evidence {
        axis: GreenAxis::Coherence,
        source: FactSource::Observed,
        proof: ProofStrength::Test,
        outcome: EvidenceOutcome::Pass,
        confidence: 1.0,
        fingerprint: "fp".into(),
        observed_at: "2026-08-09T00:00:00Z".into(),
        producer: "test".into(),
        snapshot_id: SnapshotId(SNAP.into()),
        message: String::new(),
    }
}

fn entity(name: &str, scale: ScopeScale, parent: Option<EntityId>) -> AtlasEntity {
    AtlasEntity {
        id: EntityId::new(),
        scale,
        kind: EntityKind::File,
        canonical_name: name.into(),
        aliases: vec![],
        relative_path: Some(format!("src/{name}")),
        parent_id: parent,
        purpose_clauses: vec![],
        protected_decision_ids: vec![],
        responsibilities: vec![],
        interfaces: vec![],
        capabilities: vec![],
        sockets: vec![],
        source_spans: vec![],
        structural_fingerprint: String::new(),
        // Passing coherence evidence, so a Green baseline exists and any darkening
        // is attributable to the conflict, not to absent evidence.
        local_evidence: vec![coherence_evidence()],
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

fn conflict(state: ConflictState, severity: GreenCode) -> ConflictRecord {
    ConflictRecord {
        id: ConflictId::new(),
        left_record_id: RecordId::new(),
        right_record_id: RecordId::new(),
        state,
        severity,
        resolution_record_id: None,
        created_at: "2026-08-09T00:00:00Z".into(),
        snapshot_id: SnapshotId(SNAP.into()),
    }
}

/// Universe → Galaxy → System → Planet → leaf, with the leaf owning `conflicts`.
/// Returns the coherence code and reasons for the leaf, plus the galaxy overall.
fn assess(conflicts: Vec<ConflictRecord>) -> (GreenCode, Vec<String>, GreenCode) {
    let universe = entity("u", ScopeScale::Universe, None);
    let galaxy = entity("g", ScopeScale::Galaxy, Some(universe.id));
    let system = entity("s", ScopeScale::System, Some(galaxy.id));
    let planet = entity("p", ScopeScale::Planet, Some(system.id));
    let mut leaf = entity("leaf", ScopeScale::Moon, Some(planet.id));
    leaf.open_conflict_ids = conflicts.iter().map(|c| c.id).collect();
    let leaf_id = leaf.id;
    let galaxy_id = galaxy.id;

    let mut snapshot = AtlasSnapshot {
        id: SnapshotId(SNAP.into()),
        timestamp: "2026-08-09T00:00:00Z".into(),
        entities: vec![universe, galaxy, system, planet, leaf],
        edges: vec![],
        records: vec![],
        conflicts,
        sources: vec![],
    };
    evaluate_snapshot(&mut snapshot, &GreenPolicy::default()).expect("evaluate");

    let leaf = snapshot.entities.iter().find(|e| e.id == leaf_id).unwrap();
    let h = &leaf.green.as_ref().unwrap().axes[&GreenAxis::Coherence];
    let galaxy_overall = snapshot
        .entities
        .iter()
        .find(|e| e.id == galaxy_id)
        .and_then(|e| e.green.as_ref())
        .map(|g| g.overall)
        .unwrap();
    (h.code, h.reasons.clone(), galaxy_overall)
}

#[test]
fn no_conflict_leaves_coherence_to_its_evidence() {
    let (code, _, _) = assess(vec![]);
    assert_eq!(
        code,
        GreenCode::Green,
        "with passing coherence evidence and no contradiction, H is Green"
    );
}

#[test]
fn an_open_verified_contradiction_reddens_coherence() {
    let (code, reasons, galaxy) = assess(vec![conflict(ConflictState::Open, GreenCode::Red)]);
    assert_eq!(
        code,
        GreenCode::Red,
        "a verified contradiction is a Red on the coherence axis (spec §11)"
    );
    assert!(
        !reasons.is_empty(),
        "the reason must name the contradiction, got {reasons:?}"
    );
    assert_ne!(
        galaxy,
        GreenCode::Green,
        "a contradiction on a child must roll up, not stop at its owner"
    );
}

#[test]
fn a_model_suspected_contradiction_is_yellow_never_red() {
    // A conflict created at Yellow severity is a suspicion, not a verified
    // failure. It must cap H at Yellow — inference cannot certify a Red.
    let (code, _, _) = assess(vec![conflict(ConflictState::Open, GreenCode::Yellow)]);
    assert_eq!(
        code,
        GreenCode::Yellow,
        "a model-suspected contradiction darkens to Yellow, never Red"
    );
}

#[test]
fn a_resolved_contradiction_does_not_darken() {
    let (code, _, _) = assess(vec![conflict(ConflictState::Resolved, GreenCode::Red)]);
    assert_eq!(
        code,
        GreenCode::Green,
        "a resolved contradiction is reconciled and no longer darkens coherence"
    );
}
