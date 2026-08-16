use creature_context_core::orbit::compile_orbit;
use creature_context_types::{
    AtlasEntity, AtlasSnapshot, EntityId, EntityKind, OrbitMode, OrbitRequest, OrbitScale,
    ScopeScale, SnapshotId,
};

const SNAPSHOT: &str = "snap-1";

fn entity(name: &str, path: &str, scale: ScopeScale) -> AtlasEntity {
    AtlasEntity {
        id: EntityId::new(),
        scale,
        kind: EntityKind::File,
        canonical_name: name.to_string(),
        aliases: vec![],
        relative_path: Some(path.to_string()),
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
        snapshot_id: SnapshotId(SNAPSHOT.to_string()),
        observed_at: "2026-08-04T00:00:00Z".to_string(),
        fresh_until: None,
    }
}

fn snapshot_with(entities: Vec<AtlasEntity>) -> AtlasSnapshot {
    AtlasSnapshot {
        id: SnapshotId(SNAPSHOT.to_string()),
        timestamp: "2026-08-04T00:00:00Z".to_string(),
        entities,
        edges: vec![],
        records: vec![],
        conflicts: vec![],
        sources: vec![],
    }
}

fn build_snapshot() -> AtlasSnapshot {
    // Create enough entities that a small budget forces omission.
    // Hierarchy requires exactly one Universe root and valid scale containment.
    let mut entities = vec![entity("root", ".", ScopeScale::Universe)];
    let universe_id = entities[0].id;

    let mut galaxy = entity("galaxy", ".", ScopeScale::Galaxy);
    let galaxy_id = galaxy.id;
    galaxy.parent_id = Some(universe_id);
    entities.push(galaxy);

    let mut system = entity("system", "src", ScopeScale::System);
    let system_id = system.id;
    system.parent_id = Some(galaxy_id);
    entities.push(system);

    let mut planet = entity("planet", "src/planet", ScopeScale::Planet);
    let planet_id = planet.id;
    planet.parent_id = Some(system_id);
    entities.push(planet);

    for i in 0..20 {
        let mut e = entity(
            &format!("file{i}.rs"),
            &format!("src/planet/file{i}.rs"),
            ScopeScale::Moon,
        );
        e.parent_id = Some(planet_id);
        entities.push(e);
    }
    snapshot_with(entities)
}

fn compile_test_orbit(budget: usize) -> creature_context_types::OrbitPacket {
    let snapshot = build_snapshot();
    let request = OrbitRequest {
        task: "understand files".to_string(),
        token_budget: budget,
        scale: OrbitScale::Galaxy,
        mode: OrbitMode::Focus,
        ..OrbitRequest::default()
    };
    compile_orbit(&snapshot, &request).expect("compile")
}

fn try_compile_test_orbit(
    budget: usize,
) -> Result<creature_context_types::OrbitPacket, creature_context_core::orbit::OrbitCompileError> {
    let snapshot = build_snapshot();
    let request = OrbitRequest {
        task: "understand files".to_string(),
        token_budget: budget,
        scale: OrbitScale::Galaxy,
        mode: OrbitMode::Focus,
        ..OrbitRequest::default()
    };
    compile_orbit(&snapshot, &request)
}

#[test]
fn every_selected_entity_carries_its_own_reason() {
    let packet = compile_test_orbit(64_000);
    assert!(!packet.selected_entities.is_empty());
    for selected in &packet.selected_entities {
        assert!(
            !selected.reasons.is_empty(),
            "entity {} selected with no reason",
            selected.entity.canonical_name
        );
    }
}

#[test]
fn ring_assignment_is_populated_and_ordered() {
    let packet = compile_test_orbit(64_000);
    let rings: Vec<u8> = packet.selected_entities.iter().map(|s| s.ring).collect();
    assert!(rings.contains(&0), "ring 0 must hold the exact target");
    assert!(
        rings.iter().any(|&r| r > 0),
        "outer rings must be populated"
    );
    let mut sorted = rings.clone();
    sorted.sort_unstable();
    assert_eq!(rings, sorted, "entities must be ordered by ring");
}

#[test]
fn omitted_categories_are_counted() {
    let packet = compile_test_orbit(350);
    assert!(
        !packet.omission_counts.is_empty(),
        "a budget that forces pruning must report what was omitted"
    );
    let total: usize = packet.omission_counts.values().sum();
    assert!(total > 0);
}

#[test]
fn tiny_budget_returns_trimmed_core_not_an_error() {
    // The budget is a maximum, never a minimum. A tiny budget must NOT fail with a
    // huge "requires at least N tokens" — it returns the most relevant core and
    // dynamically quantifies away the supporting context (recorded as omissions),
    // down to a single entity if need be.
    let packet = compile_test_orbit(50);
    assert!(
        !packet.selected_entities.is_empty(),
        "at least the core entity survives a tiny budget"
    );
    assert!(
        packet.omission_counts.values().sum::<usize>() > 0,
        "supporting context was trimmed to fit, and that is recorded as omissions"
    );
    // The spine is supporting context now, so a tiny budget sheds it too.
    assert!(
        packet.architectural_spine.is_empty(),
        "a 50-token budget cannot afford the architectural spine"
    );
}

#[test]
fn packet_never_exceeds_its_budget() {
    for budget in [2_000, 16_000, 64_000] {
        let packet = compile_test_orbit(budget);
        assert!(
            packet.estimated_total_tokens <= budget,
            "packet of {} tokens exceeded budget {budget}",
            packet.estimated_total_tokens
        );
    }
}
