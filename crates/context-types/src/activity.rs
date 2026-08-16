use serde::{Deserialize, Serialize};

use crate::{EntityId, EventId, SnapshotId, context::PrivacyClass};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ActivityKind {
    FileAdded,
    FileModified,
    FileRemoved,
    FileRenamed,
    WatcherOverflow,
    RootUnavailable,
    Sleep,
    Wake,
    GitChanged,
    VerificationRecorded,
    InstructionObserved,
    SessionObserved,
    HumanDecision,
    PermissionRecorded,
    ReconcileRequested,
    ReconcileApplied,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityEvent {
    pub id: EventId,
    pub project_id: EntityId,
    pub galaxy_id: EntityId,
    pub kind: ActivityKind,
    pub source_locator: String,
    pub observed_at: String,
    pub snapshot_id: SnapshotId,
    pub privacy_class: PrivacyClass,
    pub payload: serde_json::Value,
}
