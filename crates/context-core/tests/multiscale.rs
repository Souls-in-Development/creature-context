use creature_context_core::{
    atlas::AtlasHierarchy, green::evaluate_snapshot, orbit::compile_orbit,
};
use creature_context_types::*;

fn evidence(axis: GreenAxis, snapshot: &SnapshotId) -> Evidence {
    Evidence {
        axis,
        source: FactSource::Parsed,
        proof: ProofStrength::Syntax,
        outcome: EvidenceOutcome::Pass,
        confidence: 1.0,
        fingerprint: "fixture".into(),
        observed_at: "2026-08-02T00:00:00Z".into(),
        producer: "test".into(),
        snapshot_id: snapshot.clone(),
        message: String::new(),
    }
}

fn entity(
    name: &str,
    scale: ScopeScale,
    kind: EntityKind,
    parent: Option<EntityId>,
    snapshot: &SnapshotId,
) -> AtlasEntity {
    AtlasEntity {
        id: EntityId::new(),
        scale,
        kind,
        canonical_name: name.into(),
        aliases: vec![],
        parent_id: parent,
        relative_path: None,
        purpose_clauses: vec![format!("Purpose of {name}")],
        protected_decision_ids: vec![],
        responsibilities: vec![],
        interfaces: vec![],
        capabilities: vec![name.into()],
        sockets: vec![],
        source_spans: vec![],
        deterministic_summary: format!("{name} implementation"),
        local_evidence: GreenAxis::ALL
            .into_iter()
            .map(|axis| evidence(axis, snapshot))
            .collect(),
        inherited_evidence: vec![],
        green: None,
        open_conflict_ids: vec![],
        inferred_summaries: vec![],
        uncertainty: vec![],
        observed_at: "2026-08-02T00:00:00Z".into(),
        fresh_until: None,
        snapshot_id: snapshot.clone(),
        structural_fingerprint: name.into(),
    }
}

fn fixture() -> AtlasSnapshot {
    let snapshot = SnapshotId("snapshot-1".into());
    let universe = entity(
        "Universe",
        ScopeScale::Universe,
        EntityKind::Registry,
        None,
        &snapshot,
    );
    let galaxy = entity(
        "Shop",
        ScopeScale::Galaxy,
        EntityKind::Product,
        Some(universe.id),
        &snapshot,
    );
    let system = entity(
        "Identity",
        ScopeScale::System,
        EntityKind::Subsystem,
        Some(galaxy.id),
        &snapshot,
    );
    let left = entity(
        "PasswordAuth",
        ScopeScale::Planet,
        EntityKind::Component,
        Some(system.id),
        &snapshot,
    );
    let right = entity(
        "PasskeyAuth",
        ScopeScale::Planet,
        EntityKind::Component,
        Some(system.id),
        &snapshot,
    );
    let moon = entity(
        "validate.rs",
        ScopeScale::Moon,
        EntityKind::File,
        Some(left.id),
        &snapshot,
    );
    let edge = AtlasEdge {
        id: EdgeId::new(),
        source_entity_id: left.id,
        target_entity_id: right.id,
        kind: RelationshipKind::Shares,
        plane: RelationshipPlane::Declared,
        proof_record_ids: vec![],
        required: true,
        evidence: vec![evidence(GreenAxis::Integration, &snapshot)],
        source_id: "test".into(),
        confidence: 1.0,
        observed_at: "2026-08-02T00:00:00Z".into(),
        fresh_until: None,
        snapshot_id: snapshot.clone(),
    };
    AtlasSnapshot {
        id: snapshot,
        timestamp: "2026-08-02T00:00:00Z".into(),
        entities: vec![universe, galaxy, system, left, right, moon],
        edges: vec![edge],
        records: vec![],
        conflicts: vec![],
        sources: vec![],
    }
}

#[test]
fn hierarchy_provides_cross_scale_spine() {
    let snapshot = fixture();
    let hierarchy = AtlasHierarchy::from_entities(&snapshot.entities).unwrap();
    let moon = snapshot
        .entities
        .iter()
        .find(|e| e.scale == ScopeScale::Moon)
        .unwrap();
    let scales: Vec<_> = hierarchy
        .cross_scale_spine(moon.id)
        .into_iter()
        .map(|e| e.scale)
        .collect();
    assert_eq!(
        scales,
        vec![
            ScopeScale::Universe,
            ScopeScale::Galaxy,
            ScopeScale::System,
            ScopeScale::Planet,
            ScopeScale::Moon
        ]
    );
}

#[test]
fn green_rolls_up_without_inference() {
    let mut snapshot = fixture();
    evaluate_snapshot(&mut snapshot, &GreenPolicy::default()).unwrap();
    assert!(
        snapshot
            .entities
            .iter()
            .all(|entity| entity.green.as_ref().unwrap().overall == GreenCode::Green)
    );
}

#[test]
fn planet_orbit_keeps_architectural_spine_and_budget() {
    let mut snapshot = fixture();
    evaluate_snapshot(&mut snapshot, &GreenPolicy::default()).unwrap();
    let planet = snapshot
        .entities
        .iter()
        .find(|e| e.canonical_name == "PasswordAuth")
        .unwrap();
    let request = OrbitRequest {
        target_references: vec![EntityReference {
            stable_id: Some(planet.id),
            relative_path: None,
            symbol: None,
        }],
        scale: OrbitScale::Planet,
        token_budget: 10_000,
        ..OrbitRequest::default()
    };
    let packet = compile_orbit(&snapshot, &request).unwrap();
    assert!(
        packet
            .architectural_spine
            .iter()
            .any(|e| e.scale == ScopeScale::Galaxy)
    );
    assert!(
        packet
            .selected_entities
            .iter()
            .any(|e| e.entity.scale == ScopeScale::Moon)
    );
    assert!(packet.estimated_total_tokens <= packet.budget);
}

#[test]
fn comparison_orbit_reports_differences() {
    let mut snapshot = fixture();
    evaluate_snapshot(&mut snapshot, &GreenPolicy::default()).unwrap();
    let left = snapshot
        .entities
        .iter()
        .find(|e| e.canonical_name == "PasswordAuth")
        .unwrap();
    let right = snapshot
        .entities
        .iter()
        .find(|e| e.canonical_name == "PasskeyAuth")
        .unwrap();
    let request = OrbitRequest {
        mode: OrbitMode::Compare,
        scale: OrbitScale::Planet,
        target_references: vec![
            EntityReference {
                stable_id: Some(left.id),
                relative_path: None,
                symbol: None,
            },
            EntityReference {
                stable_id: Some(right.id),
                relative_path: None,
                symbol: None,
            },
        ],
        token_budget: 10_000,
        ..OrbitRequest::default()
    };
    let packet = compile_orbit(&snapshot, &request).unwrap();
    let comparison = packet.comparison.unwrap();
    assert!(!comparison.differences.is_empty() || !comparison.left_only.is_empty());
}

#[test]
fn identical_request_and_snapshot_produce_identical_packet() {
    let mut snapshot = fixture();
    evaluate_snapshot(&mut snapshot, &GreenPolicy::default()).unwrap();
    let request = OrbitRequest {
        task: "inspect identity".into(),
        scale: OrbitScale::Galaxy,
        token_budget: 10_000,
        ..OrbitRequest::default()
    };
    let first = compile_orbit(&snapshot, &request).unwrap();
    let second = compile_orbit(&snapshot, &request).unwrap();
    assert_eq!(
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&second).unwrap()
    );
    assert!(first.estimated_total_tokens <= first.budget);
}
