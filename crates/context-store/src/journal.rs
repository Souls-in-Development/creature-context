use creature_context_types::{activity::ActivityEvent, context::ContextRecord};
use rusqlite::{Connection, params};
use std::path::Path;
use thiserror::Error;

const MIGRATION: &str = include_str!("../migrations/0005_journals_ledgers.sql");

#[derive(Debug, Error)]
pub enum JournalError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub struct JournalStore {
    connection: Connection,
}

impl JournalStore {
    pub fn open(path: &Path) -> Result<Self, JournalError> {
        let connection = Connection::open(path)?;
        connection.execute_batch(MIGRATION)?;
        Ok(Self { connection })
    }

    pub fn in_memory() -> Result<Self, JournalError> {
        let connection = Connection::open_in_memory()?;
        connection.execute_batch(MIGRATION)?;
        Ok(Self { connection })
    }

    pub fn append_activity(&mut self, event: &ActivityEvent) -> Result<(), JournalError> {
        self.connection.execute(
            "INSERT INTO activity_journal (id, project_id, galaxy_id, kind, source_locator, observed_at, snapshot_id, privacy_class, payload_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                event.id.0.to_string(),
                event.project_id.to_string(),
                event.galaxy_id.to_string(),
                serde_json::to_value(&event.kind)?.as_str().unwrap_or(""),
                event.source_locator,
                event.observed_at,
                event.snapshot_id.0,
                serde_json::to_value(&event.privacy_class)?.as_str().unwrap_or(""),
                serde_json::to_string(&event.payload)?
            ],
        )?;
        Ok(())
    }

    pub fn append_record(&mut self, record: &ContextRecord) -> Result<(), JournalError> {
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "INSERT INTO context_records (id, record_type, value, scope_id, source_id, authority, confidence, created_at, observed_at, expires_at, content_hash, snapshot_id, privacy_class, state) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                record.id.0.to_string(),
                serde_json::to_value(&record.record_type)?.as_str().unwrap_or(""),
                record.value,
                record.scope_id.to_string(),
                record.source_id,
                serde_json::to_value(&record.authority)?.as_str().unwrap_or(""),
                record.confidence,
                record.created_at,
                record.observed_at,
                record.expires_at,
                record.content_hash,
                record.snapshot_id.0,
                serde_json::to_value(&record.privacy_class)?.as_str().unwrap_or(""),
                serde_json::to_value(&record.state)?.as_str().unwrap_or("")
            ],
        )?;

        for superseded in &record.supersedes {
            transaction.execute(
                "INSERT OR IGNORE INTO record_supersessions (superseding_id, superseded_id) VALUES (?1, ?2)",
                params![record.id.0.to_string(), superseded.0.to_string()],
            )?;
        }

        for contradicted in &record.contradicts {
            transaction.execute(
                "INSERT OR IGNORE INTO record_contradictions (source_id, target_id) VALUES (?1, ?2)",
                params![record.id.0.to_string(), contradicted.0.to_string()],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }
}
