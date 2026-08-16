//! Milestone 4 Task 4: a scanned snapshot gains parsed symbols as Moon entities
//! under their files, joined by observed edges — and the result is still a valid
//! hierarchy.

use creature_context_core::atlas::AtlasHierarchy;
use creature_context_parsers::enrich::enrich_snapshot;
use creature_context_types::{
    AtlasEntity, AtlasSnapshot, EntityId, EntityKind, RelationshipPlane, ScopeScale, SnapshotId,
};
use std::fs;
use std::path::PathBuf;

fn temp_root(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("cc-enrich-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(path.join("src")).unwrap();
    path
}

fn moon(name: &str, path: &str, parent: EntityId) -> AtlasEntity {
    let mut e = full_entity(name, ScopeScale::Moon);
    e.relative_path = Some(path.to_string());
    e.parent_id = Some(parent);
    e
}

fn full_entity(name: &str, scale: ScopeScale) -> AtlasEntity {
    AtlasEntity {
        id: EntityId::new(),
        scale,
        kind: EntityKind::File,
        canonical_name: name.to_string(),
        aliases: vec![],
        relative_path: None,
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
        snapshot_id: SnapshotId("snap".into()),
        observed_at: "2026-08-07T00:00:00Z".into(),
        fresh_until: None,
    }
}

/// Universe → Galaxy → System → Planet → file Moon, so the enriched result can
/// be validated as a real hierarchy.
fn snapshot_with_file(file_path: &str) -> (AtlasSnapshot, EntityId) {
    let universe = full_entity("u", ScopeScale::Universe);
    let mut galaxy = full_entity("g", ScopeScale::Galaxy);
    galaxy.parent_id = Some(universe.id);
    let mut system = full_entity("s", ScopeScale::System);
    system.parent_id = Some(galaxy.id);
    let mut planet = full_entity("src", ScopeScale::Planet);
    planet.parent_id = Some(system.id);
    let file = moon("lib.rs", file_path, planet.id);
    let file_id = file.id;

    let snapshot = AtlasSnapshot {
        id: SnapshotId("snap".into()),
        timestamp: "2026-08-07T00:00:00Z".into(),
        entities: vec![universe, galaxy, system, planet, file],
        edges: vec![],
        records: vec![],
        conflicts: vec![],
        sources: vec![],
    };
    (snapshot, file_id)
}

#[test]
fn a_scanned_file_gains_its_symbols_as_moon_entities() {
    let root = temp_root("symbols");
    fs::write(
        root.join("src/lib.rs"),
        "pub struct Widget { id: u32 }\nfn build() -> Widget { Widget { id: 0 } }\n",
    )
    .unwrap();

    let (mut snapshot, file_id) = snapshot_with_file("src/lib.rs");
    let added = enrich_snapshot(&root, &mut snapshot);
    assert_eq!(added, 2, "the struct and the function must be added");

    let symbols: Vec<&str> = snapshot
        .entities
        .iter()
        .filter(|e| e.parent_id == Some(file_id) && e.scale == ScopeScale::Moon)
        .map(|e| e.canonical_name.as_str())
        .collect();
    assert!(symbols.contains(&"Widget"), "struct entity: {symbols:?}");
    assert!(symbols.contains(&"build"), "function entity: {symbols:?}");

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn symbol_edges_are_observed() {
    let root = temp_root("edges");
    fs::write(root.join("src/lib.rs"), "fn only() {}\n").unwrap();
    let (mut snapshot, file_id) = snapshot_with_file("src/lib.rs");
    enrich_snapshot(&root, &mut snapshot);

    let observed = snapshot
        .edges
        .iter()
        .filter(|e| e.source_entity_id == file_id && e.plane == RelationshipPlane::Observed)
        .count();
    assert_eq!(
        observed, 1,
        "the file→symbol contains edge must be observed"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn the_enriched_snapshot_is_still_a_valid_hierarchy() {
    // The load-bearing check: file Moons containing symbol Moons must not break
    // scale validation (Moon→Moon is now allowed).
    let root = temp_root("valid");
    fs::write(root.join("src/lib.rs"), "struct A {}\nfn b() {}\n").unwrap();
    let (mut snapshot, _) = snapshot_with_file("src/lib.rs");
    enrich_snapshot(&root, &mut snapshot);

    AtlasHierarchy::from_entities(&snapshot.entities)
        .expect("a file Moon containing symbol Moons must validate");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn an_unsupported_file_is_left_untouched() {
    let root = temp_root("unsupported");
    fs::write(root.join("src/lib.rs"), "not source we parse").unwrap();
    let (mut snapshot, _) = snapshot_with_file("notes.txt"); // no grammar for .txt
    let before = snapshot.entities.len();
    let added = enrich_snapshot(&root, &mut snapshot);

    assert_eq!(added, 0, "a file with no grammar adds no symbols");
    assert_eq!(snapshot.entities.len(), before, "and removes nothing");
    let _ = fs::remove_dir_all(&root);
}
