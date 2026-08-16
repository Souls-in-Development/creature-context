PRAGMA foreign_keys = ON;

-- Append-only permission rules. Superseded rules are retained; supersession is
-- recorded in permission_supersessions, not by deletion (specification 10).
CREATE TABLE IF NOT EXISTS permission_rules (
    id TEXT PRIMARY KEY,
    subject TEXT NOT NULL,
    action TEXT NOT NULL,
    resource TEXT NOT NULL,
    decision TEXT NOT NULL,
    created_at TEXT NOT NULL,
    payload_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS permission_rules_subject_idx ON permission_rules(subject);
CREATE INDEX IF NOT EXISTS permission_rules_resource_idx ON permission_rules(resource);

-- Usage is recorded separately from consent: an auto-approved action is a use,
-- never a new human decision, and never widens a rule (specification 10).
CREATE TABLE IF NOT EXISTS permission_uses (
    id TEXT PRIMARY KEY,
    permission_id TEXT NOT NULL,
    action_fingerprint TEXT NOT NULL,
    used_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS permission_uses_permission_idx ON permission_uses(permission_id);

CREATE TABLE IF NOT EXISTS permission_supersessions (
    id TEXT PRIMARY KEY,
    superseded TEXT NOT NULL,
    replacement TEXT NOT NULL,
    recorded_at TEXT NOT NULL,
    authority_source TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS permission_supersessions_superseded_idx ON permission_supersessions(superseded);
