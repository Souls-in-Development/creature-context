PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS atlas_entities (
    id TEXT PRIMARY KEY,
    scale TEXT NOT NULL,
    kind TEXT NOT NULL,
    canonical_name TEXT NOT NULL,
    parent_id TEXT,
    payload_json TEXT NOT NULL,
    snapshot_id TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS atlas_edges (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    relationship_kind TEXT NOT NULL,
    relationship_plane TEXT NOT NULL,
    required INTEGER NOT NULL CHECK(required IN (0, 1)),
    payload_json TEXT NOT NULL,
    snapshot_id TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS atlas_entities_parent_idx ON atlas_entities(parent_id);
CREATE INDEX IF NOT EXISTS atlas_entities_scale_idx ON atlas_entities(scale);
CREATE INDEX IF NOT EXISTS atlas_edges_source_idx ON atlas_edges(source_id);
CREATE INDEX IF NOT EXISTS atlas_edges_target_idx ON atlas_edges(target_id);

