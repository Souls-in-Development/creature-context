//! The permission ledger: durable, append-only storage of permission rules,
//! their uses, and their supersessions.
//!
//! This is the portable authority record of specification 10. Two invariants it
//! exists to hold: usage is recorded separately from consent (an auto-approved
//! action never becomes a new rule), and history is append-only (superseding a
//! rule records the supersession, it does not delete the rule).

use crate::StoreError;
use creature_context_types::{
    EventId, PermissionId,
    authority::{PermissionRule, PermissionUse},
};
use rusqlite::{Connection, params};

const MIGRATION: &str = include_str!("../migrations/0006_permission_ledger.sql");

pub struct PermissionLedger {
    connection: Connection,
}

impl PermissionLedger {
    pub fn open(path: &std::path::Path) -> Result<Self, StoreError> {
        let connection = Connection::open(path)?;
        connection.execute_batch(MIGRATION)?;
        Ok(Self { connection })
    }

    pub fn in_memory() -> Result<Self, StoreError> {
        let connection = Connection::open_in_memory()?;
        connection.execute_batch(MIGRATION)?;
        Ok(Self { connection })
    }

    /// Append a rule. Rules are never updated in place; a correction is a new
    /// rule plus a supersession (`supersede`).
    pub fn append_rule(&mut self, rule: &PermissionRule) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO permission_rules (id, subject, action, resource, decision, created_at, payload_json) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                rule.id.to_string(),
                rule.subject,
                serde_json::to_value(&rule.action)?.as_str().unwrap_or_default(),
                rule.resource,
                serde_json::to_value(&rule.decision)?.as_str().unwrap_or_default(),
                rule.created_at,
                serde_json::to_string(rule)?,
            ],
        )?;
        Ok(())
    }

    /// Record that a rule was exercised. This is usage, not consent: it adds a
    /// row to `permission_uses` and touches no rule.
    pub fn record_use(
        &mut self,
        permission_id: PermissionId,
        action_fingerprint: &str,
    ) -> Result<(), StoreError> {
        let use_record = PermissionUse {
            id: EventId::new(),
            permission_id,
            action_fingerprint: action_fingerprint.to_string(),
            used_at: chrono::Utc::now().to_rfc3339(),
        };
        self.connection.execute(
            "INSERT INTO permission_uses (id, permission_id, action_fingerprint, used_at) \
             VALUES (?1, ?2, ?3, ?4)",
            params![
                use_record.id.to_string(),
                use_record.permission_id.to_string(),
                use_record.action_fingerprint,
                use_record.used_at,
            ],
        )?;
        Ok(())
    }

    /// Supersede `old` with `replacement`, in one transaction: the replacement
    /// is appended and a supersession row links the two. The old rule remains.
    pub fn supersede(
        &mut self,
        old: PermissionId,
        replacement: &PermissionRule,
    ) -> Result<(), StoreError> {
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "INSERT INTO permission_rules (id, subject, action, resource, decision, created_at, payload_json) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                replacement.id.to_string(),
                replacement.subject,
                serde_json::to_value(&replacement.action)?.as_str().unwrap_or_default(),
                replacement.resource,
                serde_json::to_value(&replacement.decision)?.as_str().unwrap_or_default(),
                replacement.created_at,
                serde_json::to_string(replacement)?,
            ],
        )?;
        transaction.execute(
            "INSERT INTO permission_supersessions (id, superseded, replacement, recorded_at, authority_source) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                EventId::new().to_string(),
                old.to_string(),
                replacement.id.to_string(),
                chrono::Utc::now().to_rfc3339(),
                serde_json::to_value(&replacement.authority_source)?.as_str().unwrap_or_default(),
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Every rule ever recorded, oldest first, superseded rules included.
    pub fn rules(&self) -> Result<Vec<PermissionRule>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT payload_json FROM permission_rules ORDER BY created_at, id")?;
        let rules = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .map(|value| Ok(serde_json::from_str(&value?)?))
            .collect::<Result<Vec<PermissionRule>, StoreError>>()?;
        Ok(rules)
    }

    /// The uses recorded against one rule.
    pub fn uses_of(&self, permission_id: PermissionId) -> Result<Vec<PermissionUse>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, permission_id, action_fingerprint, used_at FROM permission_uses \
             WHERE permission_id = ?1 ORDER BY used_at, id",
        )?;
        let uses = statement
            .query_map(params![permission_id.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .map(|columns| {
                let (id, pid, fingerprint, used_at) = columns?;
                Ok(PermissionUse {
                    id: EventId(uuid::Uuid::parse_str(&id)?),
                    permission_id: PermissionId(uuid::Uuid::parse_str(&pid)?),
                    action_fingerprint: fingerprint,
                    used_at,
                })
            })
            .collect::<Result<Vec<PermissionUse>, StoreError>>()?;
        Ok(uses)
    }

    /// The ids of rules that have been superseded, so evaluation can ignore them.
    pub fn superseded_ids(&self) -> Result<Vec<PermissionId>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT superseded FROM permission_supersessions ORDER BY recorded_at, id")?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .map(|value| Ok(PermissionId(uuid::Uuid::parse_str(&value?)?)))
            .collect::<Result<Vec<PermissionId>, StoreError>>()?;
        Ok(ids)
    }
}
