//! `load_planet_subtree` returns exactly one planet's descendants (its files and
//! their symbols) plus the contains edges sourced from the planet and its files,
//! stamped with the given snapshot — the bounded input the stitch re-evaluates.

use creature_context_store::AtlasRepository;
use creature_context_types::*;

fn uid(n: u128) -> uuid::Uuid {
    uuid::Uuid::from_u128(n)
}

fn entity(id: u128, scale: ScopeScale, parent: Option<u128>, snap: &SnapshotId) -> AtlasEntity {
    AtlasEntity {
        id: EntityId(uid(id)),
        scale,
        kind: EntityKind::Component,
        canonical_name: format!("e{id}"),
        aliases: vec![],
        relative_path: None,
        parent_id: parent.map(|p| EntityId(uid(p))),
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
        snapshot_id: snap.clone(),
        observed_at: "2026-08-07T00:00:00Z".into(),
        fresh_until: None,
    }
}

fn contains(src: u128, tgt: u128, snap: &SnapshotId) -> AtlasEdge {
    AtlasEdge {
        id: EdgeId(uid(1000 + src * 10 + tgt)),
        source_entity_id: EntityId(uid(src)),
        target_entity_id: EntityId(uid(tgt)),
        kind: RelationshipKind::Contains,
        plane: RelationshipPlane::Declared,
        proof_record_ids: vec![],
        evidence: vec![],
        source_id: "t".into(),
        confidence: 1.0,
        observed_at: "2026-08-07T00:00:00Z".into(),
        fresh_until: None,
        required: true,
        snapshot_id: snap.clone(),
    }
}

#[test]
fn load_planet_subtree_returns_files_symbols_and_edges() {
    let snap = SnapshotId("snap-1".into());
    let mut repo = AtlasRepository::in_memory().unwrap();
    // planet 1 -> file 2, file 3 ; file 2 -> symbol 4
    let entities = vec![
        entity(1, ScopeScale::Planet, Some(99), &snap),
        entity(2, ScopeScale::Moon, Some(1), &snap),
        entity(3, ScopeScale::Moon, Some(1), &snap),
        entity(4, ScopeScale::Moon, Some(2), &snap),
    ];
    let edges = vec![
        contains(1, 2, &snap),
        contains(1, 3, &snap),
        contains(2, 4, &snap),
    ];
    repo.upsert_subtree(&entities, &edges, &snap).unwrap();

    let (ents, eds) = repo
        .load_planet_subtree(EntityId(uid(1)), &snap)
        .unwrap();

    let ids: std::collections::BTreeSet<String> = ents.iter().map(|e| e.id.to_string()).collect();
    assert!(ids.contains(&EntityId(uid(2)).to_string()), "file 2 present");
    assert!(ids.contains(&EntityId(uid(3)).to_string()), "file 3 present");
    assert!(ids.contains(&EntityId(uid(4)).to_string()), "symbol 4 present");
    assert!(
        !ids.contains(&EntityId(uid(1)).to_string()),
        "planet itself is not a descendant"
    );
    assert_eq!(eds.len(), 3, "planet->file x2 and file->symbol x1");
}

#[test]
fn entity_green_codes_returns_scale_id_and_code() {
    use creature_context_types::GreenAssessment;
    let snap = SnapshotId("snap-g".into());
    let mut repo = AtlasRepository::in_memory().unwrap();

    let mut planet = entity(1, ScopeScale::Planet, Some(99), &snap);
    planet.green = Some(GreenAssessment {
        overall: GreenCode::Green,
        axes: Default::default(),
        snapshot_id: snap.clone(),
    });
    let mut file = entity(2, ScopeScale::Moon, Some(1), &snap);
    file.green = Some(GreenAssessment {
        overall: GreenCode::Red,
        axes: Default::default(),
        snapshot_id: snap.clone(),
    });
    // entity 3 has green = None → must read back as Unknown.
    let bare = entity(3, ScopeScale::Moon, Some(1), &snap);

    repo.upsert_subtree(&[planet, file, bare], &[], &snap).unwrap();
    repo.finalize_snapshot(&snap, EntityId(uid(1))).unwrap();

    let codes: Vec<(ScopeScale, GreenCode)> = repo
        .entity_green_codes()
        .unwrap()
        .into_iter()
        .map(|(s, _, c)| (s, c))
        .collect();
    assert!(codes.contains(&(ScopeScale::Planet, GreenCode::Green)));
    assert!(codes.contains(&(ScopeScale::Moon, GreenCode::Red)));
    assert!(codes.contains(&(ScopeScale::Moon, GreenCode::Unknown)));
    assert_eq!(codes.len(), 3);
}

#[test]
fn entity_tree_nodes_returns_parent_scale_and_code() {
    use creature_context_types::GreenAssessment;
    let snap = SnapshotId("snap-t".into());
    let mut repo = AtlasRepository::in_memory().unwrap();

    let mut planet = entity(1, ScopeScale::Planet, None, &snap);
    planet.green = Some(GreenAssessment {
        overall: GreenCode::Green,
        axes: Default::default(),
        snapshot_id: snap.clone(),
    });
    let file = entity(2, ScopeScale::Moon, Some(1), &snap);

    repo.upsert_subtree(&[planet, file], &[], &snap).unwrap();
    repo.finalize_snapshot(&snap, EntityId(uid(1))).unwrap();

    let nodes = repo.entity_tree_nodes().unwrap();
    assert_eq!(nodes.len(), 2);
    let p = nodes
        .iter()
        .find(|n| n.0 == EntityId(uid(1)).to_string())
        .unwrap();
    assert_eq!(p.1, None);
    assert_eq!(p.2, ScopeScale::Planet);
    assert_eq!(p.3, GreenCode::Green);
    let f = nodes
        .iter()
        .find(|n| n.0 == EntityId(uid(2)).to_string())
        .unwrap();
    assert_eq!(f.1, Some(EntityId(uid(1)).to_string()));
    assert_eq!(f.3, GreenCode::Unknown);
}
