//! Milestone 4 Task 6: stable identity across change.
//!
//! Paths and line numbers are mutable attributes; stable identifiers are
//! canonical (spec §3, §6). A rescan assigns fresh ids from the new text, so
//! `reconcile_identity` maps them back onto the previous snapshot's ids where
//! the entity is recognisably the same:
//!
//! - moved (same name and kind, new line) → the id is preserved;
//! - renamed unambiguously (one structural match of the same kind) → preserved;
//! - renamed ambiguously (several equally-good matches) → left with fresh ids,
//!   never merged. "Rename ambiguity creates candidates; it never silently
//!   merges stable identities" (spec §17).

use creature_context_core::identity::reconcile_identity;
use creature_context_types::{
    AtlasEdge, AtlasEntity, AtlasSnapshot, EdgeId, EntityId, EntityKind, RelationshipKind,
    RelationshipPlane, ScopeScale, SnapshotId, model::InferredSummary,
};

const SNAP: &str = "snap-identity";

fn base(kind: EntityKind, name: &str, scale: ScopeScale) -> AtlasEntity {
    AtlasEntity {
        id: EntityId::new(),
        scale,
        kind,
        canonical_name: name.into(),
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
        snapshot_id: SnapshotId(SNAP.into()),
        observed_at: "2026-08-08T00:00:00Z".into(),
        fresh_until: None,
    }
}

fn symbol(file: EntityId, name: &str, kind: EntityKind, fingerprint: &str) -> AtlasEntity {
    let mut e = base(kind, name, ScopeScale::Moon);
    e.parent_id = Some(file);
    e.relative_path = Some("src/lib.rs".into());
    e.structural_fingerprint = fingerprint.into();
    e
}

/// A snapshot with one file under a planet, plus the given symbols and their
/// containment edges — the shape enrichment produces.
fn snapshot(symbols: Vec<AtlasEntity>) -> AtlasSnapshot {
    let planet = base(EntityKind::Module, "src", ScopeScale::Planet);
    let mut file = base(EntityKind::File, "lib.rs", ScopeScale::Moon);
    file.parent_id = Some(planet.id);
    file.relative_path = Some("src/lib.rs".into());
    let file_id = file.id;

    let mut entities = vec![planet, file];
    let mut edges = vec![];
    for mut sym in symbols {
        sym.parent_id = Some(file_id);
        edges.push(AtlasEdge {
            id: EdgeId::new(),
            source_entity_id: file_id,
            target_entity_id: sym.id,
            kind: RelationshipKind::Contains,
            plane: RelationshipPlane::Observed,
            proof_record_ids: vec![],
            evidence: vec![],
            source_id: "test".into(),
            confidence: 1.0,
            observed_at: "2026-08-08T00:00:00Z".into(),
            fresh_until: None,
            required: false,
            snapshot_id: SnapshotId(SNAP.into()),
        });
        entities.push(sym);
    }
    AtlasSnapshot {
        id: SnapshotId(SNAP.into()),
        timestamp: "2026-08-08T00:00:00Z".into(),
        entities,
        edges,
        records: vec![],
        conflicts: vec![],
        sources: vec![],
    }
}

fn file_id(snapshot: &AtlasSnapshot) -> EntityId {
    snapshot
        .entities
        .iter()
        .find(|e| e.kind == EntityKind::File)
        .unwrap()
        .id
}

fn sym_id(snapshot: &AtlasSnapshot, name: &str) -> EntityId {
    snapshot
        .entities
        .iter()
        .find(|e| e.canonical_name == name && e.kind != EntityKind::File)
        .unwrap_or_else(|| panic!("symbol {name}"))
        .id
}

#[test]
fn a_moved_symbol_keeps_its_id() {
    let fid = file_id(&snapshot(vec![]));
    let prev = snapshot(vec![symbol(fid, "build", EntityKind::Function, "function")]);
    let prev_build = sym_id(&prev, "build");

    // The same symbol, re-parsed after edits above it (a fresh id).
    let mut next = snapshot(vec![symbol(fid, "build", EntityKind::Function, "function")]);
    assert_ne!(
        sym_id(&next, "build"),
        prev_build,
        "a rescan minted a new id"
    );

    reconcile_identity(&prev, &mut next);
    assert_eq!(
        sym_id(&next, "build"),
        prev_build,
        "same name and kind: identity is preserved across the move"
    );
}

#[test]
fn an_unambiguous_rename_keeps_its_id() {
    let fid = file_id(&snapshot(vec![]));
    let prev = snapshot(vec![symbol(fid, "build", EntityKind::Function, "function")]);
    let prev_build = sym_id(&prev, "build");

    // Renamed, same structure, the only function in the file.
    let mut next = snapshot(vec![symbol(
        fid,
        "assemble",
        EntityKind::Function,
        "function",
    )]);
    reconcile_identity(&prev, &mut next);

    assert_eq!(
        sym_id(&next, "assemble"),
        prev_build,
        "a lone structural match is the same entity under a new name"
    );
}

#[test]
fn the_containment_edge_follows_a_preserved_id() {
    let fid = file_id(&snapshot(vec![]));
    let prev = snapshot(vec![symbol(fid, "build", EntityKind::Function, "function")]);
    let prev_build = sym_id(&prev, "build");
    let mut next = snapshot(vec![symbol(
        fid,
        "assemble",
        EntityKind::Function,
        "function",
    )]);
    reconcile_identity(&prev, &mut next);

    // The remap must rewrite references, not just the entity's own id, or the
    // graph would dangle.
    let edge = next
        .edges
        .iter()
        .find(|e| e.kind == RelationshipKind::Contains)
        .unwrap();
    assert_eq!(
        edge.target_entity_id, prev_build,
        "the contains edge points at the preserved id, not the discarded one"
    );
}

#[test]
fn a_preserved_entity_keeps_its_inferred_summaries() {
    // The semantic lane enriches an entity; a later deterministic reconcile
    // produces a fresh (empty) index. The enrichment must travel with the
    // entity's preserved identity, or every file change would wipe the model's
    // work.
    let fid = file_id(&snapshot(vec![]));
    let mut enriched = symbol(fid, "build", EntityKind::Function, "function");
    enriched.inferred_summaries = vec![InferredSummary {
        value: "assembles a Widget".into(),
        producer: "model".into(),
        model_id: "m-1".into(),
        confidence: 0.6,
        source_record_ids: vec![],
        snapshot_id: SnapshotId(SNAP.into()),
    }];
    let prev = snapshot(vec![enriched]);

    // A rescan mints a fresh id and empty summaries for the same symbol.
    let mut next = snapshot(vec![symbol(fid, "build", EntityKind::Function, "function")]);
    reconcile_identity(&prev, &mut next);

    let carried = next
        .entities
        .iter()
        .find(|e| e.canonical_name == "build" && e.kind != EntityKind::File)
        .unwrap();
    assert_eq!(
        carried.inferred_summaries.len(),
        1,
        "the model's summary travels with the preserved entity"
    );
    assert!(carried.inferred_summaries[0].value.contains("Widget"));
}

#[test]
fn an_ambiguous_rename_is_not_merged() {
    let fid = file_id(&snapshot(vec![]));
    let prev = snapshot(vec![
        symbol(fid, "alpha", EntityKind::Function, "function"),
        symbol(fid, "beta", EntityKind::Function, "function"),
    ]);
    // Both renamed; two identical structural candidates for each.
    let mut next = snapshot(vec![
        symbol(fid, "one", EntityKind::Function, "function"),
        symbol(fid, "two", EntityKind::Function, "function"),
    ]);
    let before: Vec<EntityId> = ["one", "two"].iter().map(|n| sym_id(&next, n)).collect();

    let recon = reconcile_identity(&prev, &mut next);

    let after: Vec<EntityId> = ["one", "two"].iter().map(|n| sym_id(&next, n)).collect();
    assert_eq!(
        before, after,
        "ambiguous renames keep their fresh ids, unmerged"
    );
    assert_ne!(after[0], after[1], "and remain distinct from each other");
    assert!(
        recon.ambiguous >= 1,
        "the ambiguity is surfaced, not silently resolved"
    );
    // No next id collides with a prev id: nothing was merged onto a prior entity.
    let prev_ids: Vec<EntityId> = ["alpha", "beta"].iter().map(|n| sym_id(&prev, n)).collect();
    assert!(
        after.iter().all(|id| !prev_ids.contains(id)),
        "no silent merge"
    );
}
