//! Normalised runtime events.
//!
//! Watcher notifications are hints, not truth (specification 7.1). They are
//! normalised into these typed events, journalled before processing, and marked
//! applied after — so a restart resumes rather than repeats, and a crash
//! mid-handling loses nothing that was observed.
//!
//! `Overflow`, `RootUnavailable`, `PermissionChanged` and `RescanRequired` are
//! first-class kinds rather than error paths, because a filesystem event stream
//! cannot be assumed complete. Losing events is normal; pretending otherwise is
//! how a stale Atlas comes to look current.

use creature_context_store::JournalEntry;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeEventKind {
    FileAdded,
    FileModified,
    FileRemoved,
    FileRenamed,
    /// The notification stream dropped events. Truth must be re-established by
    /// reconciliation; the stream cannot be trusted to have been complete.
    Overflow,
    /// The watched root disappeared — unmounted, deleted, or moved.
    RootUnavailable,
    /// Access to the root changed, so previously readable paths may not be.
    PermissionChanged,
    /// A bounded rescan is required, whatever the reason.
    RescanRequired,
    /// Typed activity supplied by a client through `ingest`.
    ExternalActivity,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeEvent {
    pub id: Uuid,
    pub kind: RuntimeEventKind,
    #[serde(default)]
    pub relative_path: Option<String>,
    pub observed_at: String,
    /// Typed payload carried by the event.
    ///
    /// Kept rather than discarded: an ingested build result or diagnostic is
    /// the evidence itself, and an event recording only that something happened
    /// cannot later be replayed into an Atlas update.
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
}

impl RuntimeEvent {
    pub fn new(kind: RuntimeEventKind, observed_at: impl Into<String>) -> Self {
        Self {
            id: Uuid::now_v7(),
            kind,
            relative_path: None,
            observed_at: observed_at.into(),
            payload: None,
        }
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.relative_path = Some(path.into());
        self
    }

    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = Some(payload);
        self
    }

    /// Whether this event invalidates the current snapshot wholesale rather
    /// than describing one path.
    pub fn requires_full_reconciliation(&self) -> bool {
        matches!(
            self.kind,
            RuntimeEventKind::Overflow
                | RuntimeEventKind::RootUnavailable
                | RuntimeEventKind::PermissionChanged
                | RuntimeEventKind::RescanRequired
        )
    }
}

impl JournalEntry for RuntimeEvent {
    fn entry_id(&self) -> Uuid {
        self.id
    }
}
