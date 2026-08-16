PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS activity_journal (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    galaxy_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    source_locator TEXT NOT NULL,
    observed_at TEXT NOT NULL,
    snapshot_id TEXT NOT NULL,
    privacy_class TEXT NOT NULL,
    payload_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS activity_journal_project_idx ON activity_journal(project_id);
CREATE INDEX IF NOT EXISTS activity_journal_galaxy_idx ON activity_journal(galaxy_id);
CREATE INDEX IF NOT EXISTS activity_journal_kind_idx ON activity_journal(kind);
CREATE INDEX IF NOT EXISTS activity_journal_observed_idx ON activity_journal(observed_at);

CREATE TABLE IF NOT EXISTS context_records (
    id TEXT PRIMARY KEY,
    record_type TEXT NOT NULL,
    value TEXT NOT NULL,
    scope_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    authority TEXT NOT NULL,
    confidence REAL NOT NULL,
    created_at TEXT NOT NULL,
    observed_at TEXT NOT NULL,
    expires_at TEXT,
    content_hash TEXT NOT NULL,
    snapshot_id TEXT NOT NULL,
    privacy_class TEXT NOT NULL,
    state TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS context_records_type_idx ON context_records(record_type);
CREATE INDEX IF NOT EXISTS context_records_scope_idx ON context_records(scope_id);
CREATE INDEX IF NOT EXISTS context_records_state_idx ON context_records(state);

CREATE TABLE IF NOT EXISTS record_supersessions (
    superseding_id TEXT NOT NULL,
    superseded_id TEXT NOT NULL,
    PRIMARY KEY (superseding_id, superseded_id),
    FOREIGN KEY(superseding_id) REFERENCES context_records(id) ON DELETE CASCADE,
    FOREIGN KEY(superseded_id) REFERENCES context_records(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS record_contradictions (
    source_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    PRIMARY KEY (source_id, target_id),
    FOREIGN KEY(source_id) REFERENCES context_records(id) ON DELETE CASCADE,
    FOREIGN KEY(target_id) REFERENCES context_records(id) ON DELETE CASCADE
);
