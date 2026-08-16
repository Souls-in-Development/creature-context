//! Milestone 4 Task 5, end to end: parsing a project yields provides/requires
//! sockets, the reconciler resolves them, and an unmet intra-repo import
//! darkens the integration axis through the existing Milestone 2 evaluator
//! (spec §6.4, §11.1). This is the gate the plan names: "a fixture with a
//! genuine unmet import produces a Hole(no_match) → Red on the integration
//! axis".

use creature_context_core::green::evaluate_snapshot;
use creature_context_parsers::enrich::enrich_snapshot;
use creature_context_types::{
    AtlasEntity, AtlasSnapshot, EntityId, EntityKind, Evidence, EvidenceOutcome, FactSource,
    GreenPolicy, ProofStrength, ScopeScale, SnapshotId, SocketDirection, SocketResolution,
    green::{GreenAxis, GreenCode},
};
use std::fs;
use std::path::PathBuf;

const SNAP: &str = "snap";

fn temp_root(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("cc-socket-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(path.join("src")).unwrap();
    path
}

/// Passing evidence on every axis, so a file's baseline is Green and any
/// downgrade is attributable to a socket rather than to absent evidence.
fn all_green() -> Vec<Evidence> {
    [
        GreenAxis::Content,
        GreenAxis::Structure,
        GreenAxis::Integration,
        GreenAxis::Verification,
        GreenAxis::Freshness,
        GreenAxis::Coherence,
    ]
    .into_iter()
    .map(|axis| Evidence {
        axis,
        source: FactSource::Observed,
        proof: ProofStrength::Test,
        outcome: EvidenceOutcome::Pass,
        confidence: 1.0,
        fingerprint: "fp".into(),
        observed_at: "2026-08-07T00:00:00Z".into(),
        producer: "test".into(),
        snapshot_id: SnapshotId(SNAP.into()),
        message: String::new(),
    })
    .collect()
}

fn entity(
    name: &str,
    scale: ScopeScale,
    parent: Option<EntityId>,
    path: Option<&str>,
) -> AtlasEntity {
    AtlasEntity {
        id: EntityId::new(),
        scale,
        kind: if path.is_some() {
            EntityKind::File
        } else {
            EntityKind::Module
        },
        canonical_name: name.to_string(),
        aliases: vec![],
        relative_path: path.map(|p| p.to_string()),
        parent_id: parent,
        purpose_clauses: vec![],
        protected_decision_ids: vec![],
        responsibilities: vec![],
        interfaces: vec![],
        capabilities: vec![],
        sockets: vec![],
        source_spans: vec![],
        structural_fingerprint: String::new(),
        local_evidence: if path.is_some() { all_green() } else { vec![] },
        inherited_evidence: vec![],
        green: None,
        open_conflict_ids: vec![],
        deterministic_summary: String::new(),
        inferred_summaries: vec![],
        uncertainty: vec![],
        snapshot_id: SnapshotId(SNAP.into()),
        observed_at: "2026-08-07T00:00:00Z".into(),
        fresh_until: None,
    }
}

/// Universe → Galaxy → System → Planet → the given file Moons.
fn project(files: &[&str]) -> (AtlasSnapshot, Vec<EntityId>) {
    let universe = entity("u", ScopeScale::Universe, None, None);
    let galaxy = entity("g", ScopeScale::Galaxy, Some(universe.id), None);
    let system = entity("s", ScopeScale::System, Some(galaxy.id), None);
    let planet = entity("src", ScopeScale::Planet, Some(system.id), None);

    let mut entities = vec![universe, galaxy, system, planet.clone()];
    let mut ids = Vec::new();
    for path in files {
        let file = entity(path, ScopeScale::Moon, Some(planet.id), Some(path));
        ids.push(file.id);
        entities.push(file);
    }

    let snapshot = AtlasSnapshot {
        id: SnapshotId(SNAP.into()),
        timestamp: "2026-08-07T00:00:00Z".into(),
        entities,
        edges: vec![],
        records: vec![],
        conflicts: vec![],
        sources: vec![],
    };
    (snapshot, ids)
}

fn requires_resolution(snapshot: &AtlasSnapshot, file_id: EntityId) -> SocketResolution {
    snapshot
        .entities
        .iter()
        .find(|e| e.id == file_id)
        .expect("file")
        .sockets
        .iter()
        .find(|s| s.direction == SocketDirection::Requires)
        .expect("a required socket on the consumer")
        .resolution
        .clone()
}

#[test]
fn a_provided_import_resolves_to_a_fit() {
    let root = temp_root("fit");
    fs::write(root.join("src/widget.rs"), "pub struct Widget {}\n").unwrap();
    fs::write(
        root.join("src/consumer.rs"),
        "use crate::widget::Widget;\npub fn build() {}\n",
    )
    .unwrap();

    let (mut snapshot, ids) = project(&["src/widget.rs", "src/consumer.rs"]);
    let consumer = ids[1];
    enrich_snapshot(&root, &mut snapshot);

    assert!(
        matches!(
            requires_resolution(&snapshot, consumer),
            SocketResolution::Fit(_)
        ),
        "Widget is provided next door, so the import fits"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn an_unmet_import_holes_and_darkens_integration_red() {
    let root = temp_root("hole");
    fs::write(
        root.join("src/consumer.rs"),
        "use crate::missing::Absent;\npub fn build() {}\n",
    )
    .unwrap();

    let (mut snapshot, ids) = project(&["src/consumer.rs"]);
    let consumer = ids[0];
    enrich_snapshot(&root, &mut snapshot);

    assert!(
        matches!(
            requires_resolution(&snapshot, consumer),
            SocketResolution::Hole(ref h) if h.reason == creature_context_types::HoleReason::NoMatch
        ),
        "nothing provides Absent — a no_match hole"
    );

    evaluate_snapshot(&mut snapshot, &GreenPolicy::default()).expect("evaluate");
    let consumer_entity = snapshot.entities.iter().find(|e| e.id == consumer).unwrap();
    let integration = &consumer_entity.green.as_ref().unwrap().axes[&GreenAxis::Integration];
    assert_eq!(
        integration.code,
        GreenCode::Red,
        "an unmet required socket is proof of absence on the integration axis"
    );
    assert!(
        integration.reasons.iter().any(|r| r.contains("Absent")),
        "the reason names the unmet shape: {:?}",
        integration.reasons
    );
    let _ = fs::remove_dir_all(&root);
}
