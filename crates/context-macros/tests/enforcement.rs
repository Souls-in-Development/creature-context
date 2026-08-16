use creature_context_types::rules::violations;
use creature_context_types::{
    AtlasEdge, AtlasEntity, AtlasSnapshot, AxisAssessment, EdgeId, EntityId, EntityKind,
    GreenAssessment, GreenAxis, GreenCode, ProofStrength, RelationshipKind, RelationshipPlane,
    ScopeScale, SnapshotId,
};
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn snapshot_id() -> SnapshotId {
    SnapshotId("snapshot-1".into())
}

fn entity(name: &str) -> AtlasEntity {
    let snapshot = snapshot_id();
    AtlasEntity {
        id: EntityId::new(),
        scale: ScopeScale::Moon,
        kind: EntityKind::File,
        canonical_name: name.into(),
        aliases: vec![],
        parent_id: None,
        relative_path: None,
        purpose_clauses: vec![],
        protected_decision_ids: vec![],
        responsibilities: vec![],
        interfaces: vec![],
        capabilities: vec![],
        sockets: vec![],
        source_spans: vec![],
        structural_fingerprint: name.into(),
        local_evidence: vec![],
        inherited_evidence: vec![],
        green: None,
        open_conflict_ids: vec![],
        deterministic_summary: name.into(),
        inferred_summaries: vec![],
        uncertainty: vec![],
        observed_at: "2026-08-04T00:00:00Z".into(),
        fresh_until: None,
        snapshot_id: snapshot.clone(),
    }
}

fn edge(
    source: EntityId,
    target: EntityId,
    kind: RelationshipKind,
    plane: RelationshipPlane,
    required: bool,
) -> AtlasEdge {
    AtlasEdge {
        id: EdgeId::new(),
        source_entity_id: source,
        target_entity_id: target,
        kind,
        plane,
        proof_record_ids: vec![],
        evidence: vec![],
        source_id: "test".into(),
        confidence: 1.0,
        observed_at: "2026-08-04T00:00:00Z".into(),
        fresh_until: None,
        required,
        snapshot_id: snapshot_id(),
    }
}

fn snapshot_with_rule_and_violation(source_name: &str, target_name: &str) -> AtlasSnapshot {
    let source = entity(source_name);
    let target = entity(target_name);
    let source_id = source.id;
    let target_id = target.id;
    AtlasSnapshot {
        id: snapshot_id(),
        timestamp: "2026-08-04T00:00:00Z".into(),
        entities: vec![source, target],
        edges: vec![
            edge(
                source_id,
                target_id,
                RelationshipKind::Conflicts,
                RelationshipPlane::Declared,
                true,
            ),
            edge(
                source_id,
                target_id,
                RelationshipKind::Imports,
                RelationshipPlane::Observed,
                false,
            ),
        ],
        records: vec![],
        conflicts: vec![],
        sources: vec![],
    }
}

fn snapshot_with_rule_only(source_name: &str, target_name: &str) -> AtlasSnapshot {
    let source = entity(source_name);
    let target = entity(target_name);
    let source_id = source.id;
    let target_id = target.id;
    AtlasSnapshot {
        id: snapshot_id(),
        timestamp: "2026-08-04T00:00:00Z".into(),
        entities: vec![source, target],
        edges: vec![edge(
            source_id,
            target_id,
            RelationshipKind::Conflicts,
            RelationshipPlane::Declared,
            true,
        )],
        records: vec![],
        conflicts: vec![],
        sources: vec![],
    }
}

fn snapshot_with_overall(code: GreenCode) -> AtlasSnapshot {
    let mut e = entity("anything");
    let snapshot = snapshot_id();
    e.green = Some(GreenAssessment {
        axes: GreenAxis::ALL
            .into_iter()
            .map(|axis| {
                (
                    axis,
                    AxisAssessment {
                        code,
                        required_proof: ProofStrength::Syntax,
                        evidence: vec![],
                        reasons: vec![],
                    },
                )
            })
            .collect(),
        overall: code,
        snapshot_id: snapshot.clone(),
    });
    AtlasSnapshot {
        id: snapshot,
        timestamp: "2026-08-04T00:00:00Z".into(),
        entities: vec![e],
        edges: vec![],
        records: vec![],
        conflicts: vec![],
        sources: vec![],
    }
}

fn snapshot_with_violation_on_plane(plane: RelationshipPlane) -> AtlasSnapshot {
    let source = entity("ui");
    let target = entity("db");
    let source_id = source.id;
    let target_id = target.id;
    AtlasSnapshot {
        id: snapshot_id(),
        timestamp: "2026-08-04T00:00:00Z".into(),
        entities: vec![source, target],
        edges: vec![
            edge(
                source_id,
                target_id,
                RelationshipKind::Conflicts,
                RelationshipPlane::Declared,
                true,
            ),
            edge(
                source_id,
                target_id,
                RelationshipKind::Imports,
                plane,
                false,
            ),
        ],
        records: vec![],
        conflicts: vec![],
        sources: vec![],
    }
}

#[test]
fn a_declared_required_rule_that_is_violated_halts_the_build() {
    let _guard = ENV_LOCK.lock().unwrap();
    let snapshot = snapshot_with_rule_and_violation("ui", "db");
    let found = violations(&snapshot);
    assert_eq!(
        found.len(),
        1,
        "a violated declared rule must halt the build"
    );
    assert!(found[0].contains("ui"));
    assert!(found[0].contains("db"));
}

#[test]
fn a_declared_rule_that_is_respected_does_not_halt_the_build() {
    let _guard = ENV_LOCK.lock().unwrap();
    let snapshot = snapshot_with_rule_only("ui", "db");
    assert!(
        violations(&snapshot).is_empty(),
        "an architecture that obeys its declared rules must compile"
    );
}

#[test]
fn generated_green_never_halts_the_build() {
    let _guard = ENV_LOCK.lock().unwrap();
    for code in [
        GreenCode::Red,
        GreenCode::Yellow,
        GreenCode::Unknown,
        GreenCode::Green,
    ] {
        let snapshot = snapshot_with_overall(code);
        assert!(
            violations(&snapshot).is_empty(),
            "{code:?} is generated evidence, not a declared rule — it must not gate the build"
        );
    }
}

#[test]
fn inferred_edges_never_halt_the_build() {
    let _guard = ENV_LOCK.lock().unwrap();
    let snapshot = snapshot_with_violation_on_plane(RelationshipPlane::Inferred);
    assert!(
        violations(&snapshot).is_empty(),
        "a model-proposed edge cannot create an architectural violation (specification 2)"
    );
}

#[test]
fn the_escape_hatch_disables_enforcement() {
    let _guard = ENV_LOCK.lock().unwrap();
    unsafe { std::env::set_var("CREATURE_CONTEXT_NO_ENFORCE", "1") };
    let snapshot = snapshot_with_rule_and_violation("ui", "db");
    assert!(violations(&snapshot).is_empty());
    unsafe { std::env::remove_var("CREATURE_CONTEXT_NO_ENFORCE") };
}

#[test]
fn this_repositorys_own_atlas_does_not_halt_its_own_build() {
    let _guard = ENV_LOCK.lock().unwrap();
    let idx = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../ATLAS.idx"));
    if let Ok(idx) = idx {
        let decoded = creature_context_store::decode_atlas_idx(&idx).expect("decode");
        assert!(
            violations(&decoded.snapshot).is_empty(),
            "Creature Context must be able to build itself after scanning itself"
        );
    }
}
