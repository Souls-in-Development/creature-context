//! Shared fixtures for the IDX encode/decode tests.
//!
//! `AtlasEntity` has no constructor and 25 public fields, so these helpers fill
//! defaults. They live here rather than as test-support methods on the
//! production types, which stay free of test-only surface.

use creature_context_types::{
    AtlasEdge, AtlasEntity, AtlasSnapshot, EdgeId, EntityId, EntityKind, RelationshipKind,
    RelationshipPlane, ScopeScale, SnapshotId,
};

pub const SNAPSHOT: &str = "snap-1";

pub fn entity(name: &str, path: &str, scale: ScopeScale) -> AtlasEntity {
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

#[allow(dead_code)] // used only by tests that exercise edges
pub fn edge(from: &AtlasEntity, to: &AtlasEntity, kind: RelationshipKind) -> AtlasEdge {
    AtlasEdge {
        id: EdgeId::new(),
        source_entity_id: from.id,
        target_entity_id: to.id,
        kind,
        plane: RelationshipPlane::Observed,
        proof_record_ids: vec![],
        evidence: vec![],
        source_id: "test".to_string(),
        confidence: 1.0,
        observed_at: "2026-08-04T00:00:00Z".to_string(),
        fresh_until: None,
        required: true,
        snapshot_id: SnapshotId(SNAPSHOT.to_string()),
    }
}

pub fn snapshot_with(entities: Vec<AtlasEntity>) -> AtlasSnapshot {
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
