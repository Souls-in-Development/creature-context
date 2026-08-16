//! Milestone 4 Task 6: incremental reconvergence.
//!
//! Ported from creature-clean's `IncrementalUpdater` (ancestor + dependency
//! reconvergence via a work-list until a finite lattice converges). When one
//! entity's local evidence changes, `reconverge` recomputes only that entity
//! and the ancestors whose rolled-up assessment could depend on it — not the
//! whole tree — and the result is identical to a full `evaluate_snapshot`.
//!
//! Creature's rollup differs from the Swift original in one way, recorded here:
//! the only cross-entity dependency is parent←child (the Structure axis folds in
//! child assessments). A required *edge* contributes its own evidence, not the
//! target entity's status, and socket fits are precomputed by the reconciler —
//! so there is no reverse-edge status propagation to port. Reconvergence walks
//! ancestors.

use creature_context_core::green::{evaluate_snapshot, reconverge};
use creature_context_types::{
    AtlasEntity, AtlasSnapshot, EntityId, EntityKind, Evidence, EvidenceOutcome, FactSource,
    GreenPolicy, ProofStrength, ScopeScale, SnapshotId,
    green::{GreenAxis, GreenCode},
};

const SNAP: &str = "snap-incremental";

fn evidence(axis: GreenAxis, outcome: EvidenceOutcome) -> Evidence {
    Evidence {
        axis,
        source: FactSource::Observed,
        proof: ProofStrength::Test,
        outcome,
        confidence: 1.0,
        fingerprint: "fp".into(),
        observed_at: "2026-08-08T00:00:00Z".into(),
        producer: "test".into(),
        snapshot_id: SnapshotId(SNAP.into()),
        message: String::new(),
    }
}

fn all_axes(outcome: EvidenceOutcome) -> Vec<Evidence> {
    [
        GreenAxis::Content,
        GreenAxis::Structure,
        GreenAxis::Integration,
        GreenAxis::Verification,
        GreenAxis::Freshness,
        GreenAxis::Coherence,
    ]
    .into_iter()
    .map(|axis| evidence(axis, outcome))
    .collect()
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
        local_evidence: all_axes(EvidenceOutcome::Pass),
        inherited_evidence: vec![],
        green: None,
        open_conflict_ids: vec![],
        deterministic_summary: String::new(),
        inferred_summaries: vec![],
        uncertainty: vec![],
        snapshot_id: SnapshotId(SNAP.into()),
        observed_at: "2026-08-08T00:00:00Z".into(),
        fresh_until: None,
    }
}

/// Universe → Galaxy → System → Planet → {leaf_a, leaf_b}. All green.
fn tree() -> (AtlasSnapshot, [EntityId; 6]) {
    let universe = entity("u", ScopeScale::Universe, None);
    let galaxy = entity("g", ScopeScale::Galaxy, Some(universe.id));
    let system = entity("s", ScopeScale::System, Some(galaxy.id));
    let planet = entity("p", ScopeScale::Planet, Some(system.id));
    let leaf_a = entity("a", ScopeScale::Moon, Some(planet.id));
    let leaf_b = entity("b", ScopeScale::Moon, Some(planet.id));
    let ids = [
        universe.id,
        galaxy.id,
        system.id,
        planet.id,
        leaf_a.id,
        leaf_b.id,
    ];
    let snapshot = AtlasSnapshot {
        id: SnapshotId(SNAP.into()),
        timestamp: "2026-08-08T00:00:00Z".into(),
        entities: vec![universe, galaxy, system, planet, leaf_a, leaf_b],
        edges: vec![],
        records: vec![],
        conflicts: vec![],
        sources: vec![],
    };
    (snapshot, ids)
}

fn overall(snapshot: &AtlasSnapshot, id: EntityId) -> GreenCode {
    snapshot
        .entities
        .iter()
        .find(|e| e.id == id)
        .unwrap()
        .green
        .as_ref()
        .unwrap()
        .overall
}

#[test]
fn a_leaf_change_reconverges_only_its_ancestor_chain() {
    let (mut snapshot, [universe, galaxy, system, planet, leaf_a, leaf_b]) = tree();
    evaluate_snapshot(&mut snapshot, &GreenPolicy::default()).expect("full evaluate");
    assert_eq!(overall(&snapshot, universe), GreenCode::Green, "all green");

    // Fail leaf_a's verification, then reconverge from leaf_a alone.
    let a = snapshot
        .entities
        .iter_mut()
        .find(|e| e.id == leaf_a)
        .unwrap();
    a.local_evidence
        .push(evidence(GreenAxis::Verification, EvidenceOutcome::Fail));
    let changed = reconverge(&mut snapshot, &[leaf_a], &GreenPolicy::default());

    // leaf_a and every ancestor darken; leaf_b (a sibling) is untouched.
    assert_eq!(overall(&snapshot, leaf_a), GreenCode::Red);
    assert_eq!(overall(&snapshot, planet), GreenCode::Red);
    assert_eq!(overall(&snapshot, universe), GreenCode::Red);
    assert_eq!(
        overall(&snapshot, leaf_b),
        GreenCode::Green,
        "sibling intact"
    );

    // Only the leaf and its ancestors are reported changed — not the sibling.
    assert!(changed.contains(&leaf_a));
    assert!(changed.contains(&galaxy) && changed.contains(&system));
    assert!(
        !changed.contains(&leaf_b),
        "the sibling did not change: {changed:?}"
    );
}

#[test]
fn reconvergence_matches_a_full_reevaluation() {
    let (mut incremental, [.., leaf_a, _leaf_b]) = tree();
    evaluate_snapshot(&mut incremental, &GreenPolicy::default()).expect("evaluate");

    // Apply the same change two ways: incrementally, and by a full re-evaluate.
    let mut full = incremental.clone();
    for snap in [&mut incremental, &mut full] {
        let a = snap.entities.iter_mut().find(|e| e.id == leaf_a).unwrap();
        a.local_evidence
            .push(evidence(GreenAxis::Content, EvidenceOutcome::Warning));
    }
    reconverge(&mut incremental, &[leaf_a], &GreenPolicy::default());
    evaluate_snapshot(&mut full, &GreenPolicy::default()).expect("evaluate");

    for e in &full.entities {
        let inc = incremental.entities.iter().find(|x| x.id == e.id).unwrap();
        assert_eq!(
            inc.green.as_ref().unwrap().overall,
            e.green.as_ref().unwrap().overall,
            "incremental and full must agree for {}",
            e.canonical_name
        );
    }
}
