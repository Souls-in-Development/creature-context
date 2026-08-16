use creature_context_store::AtlasRepository;
use creature_context_types::*;

fn entity(snapshot: &SnapshotId) -> AtlasEntity {
    AtlasEntity {
        id: EntityId::new(),
        scale: ScopeScale::Universe,
        kind: EntityKind::Registry,
        canonical_name: "Universe".into(),
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
        deterministic_summary: String::new(),
        local_evidence: vec![],
        inherited_evidence: vec![],
        green: None,
        open_conflict_ids: vec![],
        inferred_summaries: vec![],
        uncertainty: vec![],
        observed_at: "2026-08-03T00:00:00Z".to_string(),
        fresh_until: None,
        snapshot_id: snapshot.clone(),
        structural_fingerprint: String::new(),
    }
}

#[test]
fn snapshot_round_trips_transactionally() {
    let id = SnapshotId("snapshot-test".into());
    let root = entity(&id);
    let snapshot = AtlasSnapshot {
        id: id.clone(),
        timestamp: "2026-08-03T00:00:00Z".to_string(),
        entities: vec![root],
        edges: vec![],
        records: vec![],
        conflicts: vec![],
        sources: vec![],
    };
    let mut repository = AtlasRepository::in_memory().unwrap();
    repository.replace_snapshot(&snapshot).unwrap();
    assert_eq!(repository.load_snapshot().unwrap(), snapshot);
}

#[test]
fn open_uses_wal_so_readers_survive_concurrent_writes() {
    // WAL is proven by its on-disk signature: a `-wal` sidecar sits next to the
    // database while the connection is open, and only in WAL mode — the default
    // rollback journal would produce a transient `-journal` instead. The sidecar's
    // presence is the observable guarantee behind the fix: an agent reading the
    // atlas mid-reindex sees the last committed snapshot rather than "database is
    // locked", so it stays with the tool instead of falling back to grep.
    let dir = std::env::temp_dir().join(format!(
        "creature-context-wal-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("atlas.db");

    let id = SnapshotId("wal-test".into());
    let snapshot = AtlasSnapshot {
        id: id.clone(),
        timestamp: "2026-08-03T00:00:00Z".to_string(),
        entities: vec![entity(&id)],
        edges: vec![],
        records: vec![],
        conflicts: vec![],
        sources: vec![],
    };

    let mut repository = AtlasRepository::open(&db).unwrap();
    repository.replace_snapshot(&snapshot).unwrap();

    let wal = dir.join("atlas.db-wal");
    assert!(
        wal.exists(),
        "expected WAL sidecar at {} — journal_mode did not switch to WAL",
        wal.display()
    );

    drop(repository);
    let _ = std::fs::remove_dir_all(&dir);
}
